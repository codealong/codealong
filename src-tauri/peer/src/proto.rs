//! What two Codealongs say to each other (the engine's peers.rs, and client.rs here): JSON lines
//! over whatever carries them, a TCP connection on the tailnet (the engine's tailnet.rs) or a QUIC
//! stream through iroh (net.rs).
//!
//! A connection opens with each side's `hello` (its device key, its name and a fresh nonce),
//! then each side's `proof`: its signature over the other side's nonce and both keys. So each
//! side knows the other holds the key it claims whatever carried the connection, and the rest of
//! the app deals in device keys only. After that, linked devices send their feeds (peerfeed.rs) and
//! ask each other things (`req` and `res`); a device that isn't linked can only ask to link.

use std::collections::HashMap;
use std::io::{self, BufRead};

use anyhow::{anyhow, bail, Result};
use data_encoding::HEXLOWER;
use iroh::{PublicKey, SecretKey, Signature};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::feed::FeedMsg;

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
        /// What kind of device it is (`MAC`, `PHONE`, `TABLET`); devices from before kinds send
        /// none, and they're Macs.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        kind: Option<String>,
        /// What it knows beyond version 1 (`KNOWS`), so the other side sends it only what it can
        /// read.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        knows: Vec<String>,
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
    /// The worktrees the sender still has from the receiver's last feed, by id, with the hash that
    /// feed gave each (`FeedMsg::hashes`): the receiver's next feed then carries only what
    /// changed since, not everything. Sent right after the proofs, and only to devices that know
    /// `HAVE`.
    Have { lanes: HashMap<String, u64> },
    /// A session's reply as it's written, for a device watching it (`api::SESSION_WATCH`).
    Stream(crate::api::Stream),
    /// A session's side conversation as it changes, for a device that has asked about that
    /// session's (`api::SIDE_ASK`, `api::SIDE_GET`, `api::SIDE_PAST`, `api::SIDE_CLOSE`) and says
    /// it knows `SIDES`: the same as the sender's own windows are told, in the same order.
    Side(crate::api::Side),
    /// A message from a newer device that this one doesn't know. It's skipped; devices from before
    /// this ended the connection on one, so new messages go only to devices that say they know them.
    #[serde(other)]
    Unknown,
}

/// A Mac: it has a board, sends it to the devices it's linked with, and answers their requests.
pub const MAC: &str = "mac";
/// A phone or a tablet running an app that only watches (client.rs): no board, answers nothing,
/// and it dials the devices it's linked with itself, while it's open.
pub const PHONE: &str = "phone";
pub const TABLET: &str = "tablet";

/// What this build knows beyond version 1, as its hello says: `Have` (resuming a feed where the
/// last one left off), watching sessions' replies (`api::SESSION_WATCH`, `Msg::Stream`),
/// Cursor's sessions and accounts (`Harness::Cursor`), which a build from before can't read: one
/// in a message would lose it the whole message, so a device that doesn't say it gets none; and
/// the sender's PR list in its feed (`FeedMsg::prs`, `pr::PrCard`), sent only to a device that
/// says `PRS`. `PR_DRAFTS` is a Mac's word that a review asked of it submits your draft review
/// only when it's the one the asking device showed (`params::PrReview::draft`); a device asks a
/// Mac to review only when it says so, since one from before submits whatever draft is there.
/// `EMBLEMS` says its windows draw every color a session's emblem comes in (`Emblem`); a device
/// that doesn't say it gets each color as the label color nearest it, which the builds before
/// drew emblems in. `PEER_ITEMS` says its windows show another session's message in a
/// conversation as an item of its own (`api::Item`'s `peer`, whole, with who sent it); a device
/// that doesn't say it gets the note the builds before sent in its place, the message's first
/// line under who it's from, since one from before would show it as the agent's own words.
/// `SCHEDULES` says it keeps the schedules your machines share (`FeedMsg::schedules`,
/// `schedules::Shared`): only a device that says it is sent them. `SIDES` says it takes a
/// session's side conversations as they change (`Msg::Side`) once it has asked about them.
pub const HAVE: &str = "have";
pub const WATCH: &str = "watch";
pub const CURSOR: &str = "cursor";
pub const PRS: &str = "prs";
pub const PR_DRAFTS: &str = "pr-drafts";
pub const EMBLEMS: &str = "emblems";
pub const PEER_ITEMS: &str = "peer-items";
pub const SCHEDULES: &str = "schedules";
pub const SIDES: &str = "sides";
pub const KNOWS: [&str; 9] = [HAVE, WATCH, CURSOR, PRS, PR_DRAFTS, EMBLEMS, PEER_ITEMS, SCHEDULES, SIDES];

/// Whether a device of this kind only watches.
pub fn watches(kind: Option<&str>) -> bool {
    matches!(kind, Some(PHONE | TABLET))
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
            Msg::Hello { v: 1, key: "ab".into(), name: "Mac mini".into(), app: "0.1.0".into(), nonce: "00".into(), kind: None, knows: vec![] },
            Msg::Hello { v: 1, key: "cd".into(), name: "iPhone".into(), app: "0.1.0".into(), nonce: "01".into(), kind: Some(PHONE.into()), knows: KNOWS.map(String::from).to_vec() },
            Msg::Have { lanes: HashMap::from([("/a".to_string(), 7u64)]) },
            Msg::Stream(crate::api::Stream { key: "claude:claude:9f".into(), text: "Looking at".into(), tool: None }),
            Msg::Side(crate::api::Side { id: "1-1".into(), key: "claude:claude:9f".into(), turns: vec![crate::api::SideTurn { question: "Why?".into(), answer: "Because".into(), ..Default::default() }], running: true, ..Default::default() }),
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
    fn messages_from_newer_devices_are_skipped() {
        let mut r = std::io::BufReader::new("{\"t\":\"hologram\",\"key\":\"k\"}\n{\"t\":\"ping\"}\n".as_bytes());
        assert!(matches!(read_msg(&mut r).unwrap(), Some(Msg::Unknown)));
        assert!(matches!(read_msg(&mut r).unwrap(), Some(Msg::Ping)));
        // A hello from before kinds is a Mac's.
        let old: Msg = serde_json::from_str(r#"{"t":"hello","v":1,"key":"ab","name":"Mini","app":"0.1.0","nonce":"00"}"#).unwrap();
        assert!(matches!(old, Msg::Hello { kind: None, .. }));
        assert!(watches(Some(PHONE)) && watches(Some(TABLET)) && !watches(Some(MAC)) && !watches(None) && !watches(Some("linux")));
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
