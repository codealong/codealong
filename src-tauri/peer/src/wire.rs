//! A connection to another device as the rest of Codealong sees it, whatever carries it: a TCP
//! stream on the tailnet, or a QUIC stream through iroh (net.rs). Reads and writes block, since
//! everything that uses one is threads and channels.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

use anyhow::Result;

pub struct Wire {
    pub reader: Box<dyn Read + Send>,
    pub writer: Box<dyn Write + Send>,
    /// How long a read may wait; None waits for ever.
    pub set_timeout: Box<dyn Fn(Option<Duration>) + Send + Sync>,
    pub close: Box<dyn Fn() + Send + Sync>,
}

impl Wire {
    pub fn tcp(s: TcpStream) -> Result<Wire> {
        // Accepted from a listener that doesn't block, a stream doesn't block either on macOS.
        s.set_nonblocking(false)?;
        let (reader, writer, timeouts) = (s.try_clone()?, s.try_clone()?, s.try_clone()?);
        Ok(Wire {
            reader: Box::new(reader),
            writer: Box::new(writer),
            set_timeout: Box::new(move |d| {
                let _ = timeouts.set_read_timeout(d);
            }),
            close: Box::new(move || {
                let _ = s.shutdown(std::net::Shutdown::Both);
            }),
        })
    }
}
