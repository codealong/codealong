//! Reaching linked Macs that aren't on your tailnet, through iroh (peers.rs). A Mac's device
//! key is its iroh address: another Mac dials the key, iroh finds a path (straight through,
//! punching through NATs, or by one of number 0's relays, which pass encrypted packets along
//! and can't read them), and the connection itself proves both keys. Linking a Mac this way
//! takes a link code (`Ticket`) made on one Mac and entered on the other: this Mac's key and a
//! one-time secret.
//!
//! iroh is async and Codealong's engine is threads and channels, so iroh runs on a runtime of
//! its own and each connection is handed to peers.rs as a reader and a writer that block.

use std::io::{self, Read, Write};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{anyhow, bail, Result};
use data_encoding::BASE32_NOPAD;
use iroh::endpoint::{presets, Connection, Incoming, RecvStream, SendStream, VarInt};
use iroh::{Endpoint, EndpointAddr, PublicKey, SecretKey};
use tokio::runtime::Runtime;

use crate::peers::Wire;

/// The protocol name iroh connections for Codealong carry, so nothing else is taken for one.
pub const ALPN: &[u8] = b"codealong/peer/1";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(25);

pub struct Iroh {
    rt: Arc<Runtime>,
    ep: Endpoint,
}

impl Iroh {
    /// Starts this Mac's endpoint, on number 0's relays and address lookup.
    pub fn start(key: SecretKey) -> Result<Iroh> {
        let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(2).thread_name("iroh").enable_all().build()?;
        let ep = rt.block_on(Endpoint::builder(presets::N0).secret_key(key).alpns(vec![ALPN.to_vec()]).bind()).map_err(|e| anyhow!("iroh didn't start: {e}"))?;
        Ok(Iroh { rt: Arc::new(rt), ep })
    }

    /// Waits for another Mac to connect; None once the endpoint has closed. The connection is
    /// finished by `finish`, on a thread of its own, so a slow one doesn't hold up the rest.
    pub fn accept(&self) -> Option<Incoming> {
        self.rt.block_on(self.ep.accept())
    }

    pub fn finish(&self, incoming: Incoming) -> Result<(PublicKey, Wire)> {
        let rt = self.rt.clone();
        self.rt.block_on(async move {
            let conn = incoming.await.map_err(|e| anyhow!("{e}"))?;
            let (send, recv) = tokio::time::timeout(CONNECT_TIMEOUT, conn.accept_bi()).await.map_err(|_| anyhow!("The other Mac opened nothing."))?.map_err(|e| anyhow!("{e}"))?;
            Ok((conn.remote_id(), wire(rt, conn, send, recv)))
        })
    }

    /// Dials a Mac by its device key.
    pub fn connect(&self, key: PublicKey) -> Result<Wire> {
        let rt = self.rt.clone();
        self.rt.block_on(async move {
            tokio::time::timeout(CONNECT_TIMEOUT, async {
                let conn = self.ep.connect(EndpointAddr::from(key), ALPN).await.map_err(|e| anyhow!("{e}"))?;
                let (send, recv) = conn.open_bi().await.map_err(|e| anyhow!("{e}"))?;
                Ok::<Wire, anyhow::Error>(wire(rt, conn, send, recv))
            })
            .await
            .map_err(|_| anyhow!("The other Mac didn't answer through iroh."))?
        })
    }

    /// Whether this Mac has reached a relay, so other Macs can find it.
    pub fn online(&self, wait: Duration) -> bool {
        self.rt.block_on(async { tokio::time::timeout(wait, self.ep.online()).await.is_ok() })
    }

    pub fn close(&self) {
        self.rt.block_on(self.ep.close());
    }
}

/// A connection as peers.rs takes it. The reader and writer hold the runtime, so it lives as
/// long as a connection does.
fn wire(rt: Arc<Runtime>, conn: Connection, send: SendStream, recv: RecvStream) -> Wire {
    let timeout = Arc::new(Mutex::new(None::<Duration>));
    let closer = conn.clone();
    let limit = timeout.clone();
    Wire {
        reader: Box::new(Reader { rt: rt.clone(), recv, timeout, _conn: conn }),
        writer: Box::new(Writer { rt, send }),
        set_timeout: Box::new(move |t| {
            if let Ok(mut l) = limit.lock() {
                *l = t;
            }
        }),
        close: Box::new(move || closer.close(VarInt::from_u32(0), b"bye")),
    }
}

struct Reader {
    rt: Arc<Runtime>,
    recv: RecvStream,
    timeout: Arc<Mutex<Option<Duration>>>,
    _conn: Connection,
}

impl Read for Reader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let limit = self.timeout.lock().map(|t| *t).unwrap_or(None);
        let recv = &mut self.recv;
        let read = self.rt.block_on(async move {
            match limit {
                Some(d) => match tokio::time::timeout(d, recv.read(buf)).await {
                    Ok(r) => r.map_err(io::Error::other),
                    Err(_) => Err(io::Error::new(io::ErrorKind::TimedOut, "The other Mac went quiet.")),
                },
                None => recv.read(buf).await.map_err(io::Error::other),
            }
        })?;
        // None: the other side finished the stream.
        Ok(read.unwrap_or(0))
    }
}

struct Writer {
    rt: Arc<Runtime>,
    send: SendStream,
}

impl Write for Writer {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let send = &mut self.send;
        self.rt.block_on(send.write_all(buf)).map_err(io::Error::other)?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// A link code: a Mac's device key and a one-time secret, for linking a Mac that isn't on your
/// tailnet. Letters and digits only (base32), so it survives a chat message or being retyped.
#[derive(Debug, Clone, PartialEq)]
pub struct Ticket {
    pub key: PublicKey,
    pub secret: String,
}

const TICKET_PREFIX: &str = "cal1";
const SECRET_BYTES: usize = 10;

impl Ticket {
    pub fn new(key: PublicKey) -> Ticket {
        let secret = crate::peerproto::nonce()[..SECRET_BYTES * 2].to_string();
        Ticket { key, secret }
    }

    pub fn encode(&self) -> String {
        let mut bytes = self.key.as_bytes().to_vec();
        bytes.extend(data_encoding::HEXLOWER.decode(self.secret.as_bytes()).unwrap_or_default());
        format!("{TICKET_PREFIX}{}", BASE32_NOPAD.encode(&bytes).to_lowercase())
    }

    pub fn decode(s: &str) -> Result<Ticket> {
        let s: String = s.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_lowercase();
        let Some(body) = s.strip_prefix(TICKET_PREFIX) else { bail!("That isn't a Codealong link code.") };
        let bytes = BASE32_NOPAD.decode(body.to_uppercase().as_bytes()).map_err(|_| anyhow!("That link code has a typo."))?;
        if bytes.len() != 32 + SECRET_BYTES {
            bail!("That link code is cut short.");
        }
        let key: [u8; 32] = bytes[..32].try_into().expect("32 bytes");
        let key = PublicKey::from_bytes(&key).map_err(|_| anyhow!("That link code has a typo."))?;
        Ok(Ticket { key, secret: data_encoding::HEXLOWER.encode(&bytes[32..]) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn link_codes_read_back_forgivingly() {
        let t = Ticket::new(SecretKey::generate().public());
        let code = t.encode();
        assert!(code.starts_with("cal1") && code.chars().all(|c| c.is_ascii_alphanumeric()), "{code}");
        assert_eq!(Ticket::decode(&code).unwrap(), t);
        // Pasted with spaces, in capitals, or broken over lines, it still reads.
        let messy = format!(" {} \n", code.to_uppercase().chars().collect::<Vec<_>>().chunks(8).map(|c| c.iter().collect::<String>()).collect::<Vec<_>>().join(" "));
        assert_eq!(Ticket::decode(&messy).unwrap(), t);
        assert!(Ticket::decode(&code[..code.len() - 4]).is_err());
        assert!(Ticket::decode("hello").is_err());
    }

    /// Two endpoints on this Mac reach each other through number 0's relays and prove their
    /// keys. Needs the internet, so it's run by hand: `cargo test iroh_connects -- --ignored`.
    #[test]
    #[ignore]
    fn iroh_connects() {
        let (ka, kb) = (SecretKey::generate(), SecretKey::generate());
        let (a, b) = (Iroh::start(ka.clone()).unwrap(), Iroh::start(kb.clone()).unwrap());
        assert!(a.online(Duration::from_secs(20)) && b.online(Duration::from_secs(20)));
        let b = Arc::new(b);
        let accepting = {
            let b = b.clone();
            std::thread::spawn(move || {
                let incoming = b.accept().expect("an incoming connection");
                let (who, mut wire) = b.finish(incoming).unwrap();
                let mut line = String::new();
                std::io::BufRead::read_line(&mut std::io::BufReader::new(&mut wire.reader), &mut line).unwrap();
                (who, line)
            })
        };
        let mut wire = a.connect(kb.public()).unwrap();
        wire.writer.write_all(b"hello\n").unwrap();
        let (who, line) = accepting.join().unwrap();
        assert_eq!(who, ka.public());
        assert_eq!(line, "hello\n");
    }
}
