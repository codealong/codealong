//! What two Codealongs say to each other (peers.rs): JSON lines over whatever carries them, a
//! TCP connection on the tailnet (tailnet.rs) or a QUIC stream through iroh (irohnet.rs).
//!
//! A connection opens with each side's `hello` (its device key, its name and a fresh nonce),
//! then each side's `proof`: its signature over the other side's nonce and both keys. So each
//! side knows the other holds the key it claims whatever carried the connection, and the rest of
//! the app deals in device keys only. After that, linked devices send their feeds (peerfeed.rs) and
//! ask each other things (`req` and `res`); a device that isn't linked can only ask to link.

use std::io::{self, BufRead};

use anyhow::{anyhow, bail, Result};
use data_encoding::HEXLOWER;
use iroh::{PublicKey, SecretKey, Signature};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::peerfeed::FeedMsg;

pub const VERSION: u32 = 1;

/// The longest line either side reads. A busy device's whole feed fits with room to spare, and a
/// peer that sends without end can't make the other buffer it.
pub const MAX_LINE: usize = 32 * 1024 * 1024;

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "t", rename_all = "kebab-case", rename_all_fields = "camelCase")]
pub enum Msg {
    Hello {
        v: u32,
        /// The device key, as hex.
        key: String,
        name: String,
        app: String,
        /// 32 random bytes, as hex, for the other side to sign.
        nonce: String,
    },
    /// The sender's signature over the receiver's nonce and both keys (`proof_message`).
    Proof { sig: String },
    /// Whether the sender has the receiver on its list of linked devices: sent once the proofs
    /// check out, and again whenever that changes.
    Standing { linked: bool },
    Feed(FeedMsg),
    Req {
        id: u64,
        method: String,
        #[serde(default)]
        params: Value,
    },
    Res {
        id: u64,
        ok: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        result: Option<Value>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    /// Asks to be linked. `secret` comes from a link code this device made (irohnet.rs); without
    /// one, only a device on the same tailnet account can ask, and you answer on this device.
    LinkAsk {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        secret: Option<String>,
    },
    LinkAnswer { accepted: bool },
    /// The sender took the receiver off its list; the receiver does the same.
    Unlinked,
    Ping,
    Pong,
}

impl Msg {
    pub fn line(&self) -> String {
        let mut s = serde_json::to_string(self).unwrap_or_else(|_| r#"{"t":"ping"}"#.to_string());
        s.push('\n');
        s
    }
}

/// A message read off the wire, or why the connection is done.
pub fn read_msg(r: &mut impl BufRead) -> io::Result<Option<Msg>> {
    loop {
        let Some(line) = read_line(r, MAX_LINE)? else { return Ok(None) };
        if line.trim().is_empty() {
            continue;
        }
        return serde_json::from_str(&line).map(Some).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("That isn't a message Codealong knows: {e}")));
    }
}

/// One line without its newline, or None at the end. A line longer than `max` is an error, so
/// nothing reads without end.
pub fn read_line(r: &mut impl BufRead, max: usize) -> io::Result<Option<String>> {
    let mut buf = Vec::new();
    loop {
        let (done, used) = {
            let available = match r.fill_buf() {
                Ok(b) => b,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            };
            if available.is_empty() {
                return if buf.is_empty() { Ok(None) } else { Err(io::Error::new(io::ErrorKind::UnexpectedEof, "The line ended early.")) };
            }
            match memchr::memchr(b'\n', available) {
                Some(i) => {
                    buf.extend_from_slice(&available[..i]);
                    (true, i + 1)
                }
                None => {
                    buf.extend_from_slice(available);
                    (false, available.len())
                }
            }
        };
        r.consume(used);
        if buf.len() > max {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "A message was too long."));
        }
        if done {
            return String::from_utf8(buf).map(Some).map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "A message wasn't text."));
        }
    }
}

/// 32 bytes from the system's secure random source, as hex: a key made fresh and thrown away
/// is exactly that, without another dependency.
pub fn nonce() -> String {
    HEXLOWER.encode(&SecretKey::generate().to_bytes())
}

pub fn key_hex(key: &PublicKey) -> String {
    HEXLOWER.encode(key.as_bytes())
}

pub fn parse_key(s: &str) -> Result<PublicKey> {
    s.trim().parse::<PublicKey>().map_err(|_| anyhow!("That isn't a device key."))
}

/// A device's short id: the first five bytes of its key, as hex. Session and lane keys from
/// another device carry it (`@<short>:<key>`); it's unique enough among one person's devices, and
/// trust always goes by the whole key.
pub fn short(key: &PublicKey) -> String {
    key.fmt_short().to_string()
}

/// What a proof signs: a label, the nonce the verifier made, then the signer's key and the
/// verifier's. A signature made for one connection or one pair of devices is worth nothing on
/// another.
fn proof_message(nonce: &[u8], signer: &PublicKey, verifier: &PublicKey) -> Vec<u8> {
    let mut m = b"codealong-peer-proof-v1".to_vec();
    m.extend_from_slice(nonce);
    m.extend_from_slice(signer.as_bytes());
    m.extend_from_slice(verifier.as_bytes());
    m
}

/// Signs the other side's nonce.
pub fn prove(mine: &SecretKey, their_nonce: &str, theirs: &PublicKey) -> Result<String> {
    let nonce = decode_nonce(their_nonce)?;
    let sig = mine.sign(&proof_message(&nonce, &mine.public(), theirs));
    Ok(HEXLOWER.encode(&sig.to_bytes()))
}

/// Checks the other side's proof: its signature over the nonce this side sent.
pub fn check_proof(theirs: &PublicKey, sig: &str, my_nonce: &str, mine: &PublicKey) -> Result<()> {
    let nonce = decode_nonce(my_nonce)?;
    let bytes = HEXLOWER.decode(sig.trim().as_bytes()).map_err(|_| anyhow!("The proof isn't hex."))?;
    let bytes: [u8; 64] = bytes.try_into().map_err(|_| anyhow!("The proof is the wrong length."))?;
    theirs.verify(&proof_message(&nonce, theirs, mine), &Signature::from_bytes(&bytes)).map_err(|_| anyhow!("The other device couldn't prove it holds its key."))
}

fn decode_nonce(s: &str) -> Result<Vec<u8>> {
    let n = HEXLOWER.decode(s.trim().as_bytes()).map_err(|_| anyhow!("The nonce isn't hex."))?;
    if n.len() != 32 {
        bail!("The nonce is the wrong length.");
    }
    Ok(n)
}

/// The six digits both devices show while linking, the same on each for the same pair of keys, so
/// you can see the device asking is the one in front of you.
pub fn link_code(a: &PublicKey, b: &PublicKey) -> String {
    let (lo, hi) = if a.as_bytes() <= b.as_bytes() { (a, b) } else { (b, a) };
    let mut h = Sha256::new();
    h.update(b"codealong-link-code-v1");
    h.update(lo.as_bytes());
    h.update(hi.as_bytes());
    let d = h.finalize();
    let n = u32::from_be_bytes([d[0], d[1], d[2], d[3]]) % 1_000_000;
    format!("{:03} {:03}", n / 1000, n % 1000)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_round_trip_as_lines() {
        let msgs = [
            Msg::Hello { v: 1, key: "ab".into(), name: "Mac mini".into(), app: "0.1.0".into(), nonce: "00".into() },
            Msg::Req { id: 7, method: "search".into(), params: serde_json::json!({"words": ["webhook"]}) },
            Msg::Res { id: 7, ok: false, result: None, error: Some("no".into()) },
            Msg::LinkAsk { secret: None },
            Msg::Standing { linked: true },
            Msg::Unlinked,
        ];
        for m in msgs {
            let line = m.line();
            assert!(line.ends_with('\n') && !line[..line.len() - 1].contains('\n'), "{line}");
            let back: Msg = serde_json::from_str(line.trim()).unwrap();
            assert_eq!(serde_json::to_value(&back).unwrap(), serde_json::to_value(&m).unwrap());
        }
        assert!(Msg::LinkAnswer { accepted: true }.line().contains(r#""t":"link-answer""#));
        assert!(Msg::Ping.line().contains(r#"{"t":"ping"}"#));
    }

    #[test]
    fn proofs_hold_only_for_their_own_pair_and_nonce() {
        let (a, b, c) = (SecretKey::generate(), SecretKey::generate(), SecretKey::generate());
        let nonce_b = nonce();
        // A proves itself to B by signing B's nonce.
        let sig = prove(&a, &nonce_b, &b.public()).unwrap();
        check_proof(&a.public(), &sig, &nonce_b, &b.public()).unwrap();
        // Replayed to C, or checked against another nonce or key, it fails.
        assert!(check_proof(&a.public(), &sig, &nonce_b, &c.public()).is_err());
        assert!(check_proof(&a.public(), &sig, &nonce(), &b.public()).is_err());
        assert!(check_proof(&c.public(), &sig, &nonce_b, &b.public()).is_err());
        assert!(check_proof(&a.public(), "zz", &nonce_b, &b.public()).is_err());
    }

    #[test]
    fn both_macs_show_the_same_link_code() {
        let (a, b) = (SecretKey::generate().public(), SecretKey::generate().public());
        let code = link_code(&a, &b);
        assert_eq!(code, link_code(&b, &a));
        assert_eq!(code.len(), 7);
        assert!(code.chars().enumerate().all(|(i, c)| if i == 3 { c == ' ' } else { c.is_ascii_digit() }), "{code}");
    }

    #[test]
    fn keys_print_and_parse_back() {
        let k = SecretKey::generate().public();
        assert_eq!(parse_key(&key_hex(&k)).unwrap(), k);
        assert_eq!(short(&k).len(), 10);
        assert!(parse_key("nope").is_err());
    }

    #[test]
    fn lines_have_a_limit() {
        let text = format!("{}\n{{\"t\":\"ping\"}}\n", "x".repeat(100));
        let mut r = std::io::BufReader::with_capacity(8, text.as_bytes());
        assert!(read_line(&mut r, 50).is_err());
        let mut r = std::io::BufReader::with_capacity(8, "{\"t\":\"ping\"}\n\n{\"t\":\"pong\"}".as_bytes());
        assert!(matches!(read_msg(&mut r).unwrap(), Some(Msg::Ping)));
        // The last line has no newline: the connection ended mid-message.
        assert!(read_msg(&mut r).is_err());
    }
}
