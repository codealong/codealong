//! `codealong terminal attach <terminal>`: one of Codealong's own terminals shown in this one, as a
//! terminal multiplexer's client shows a session (server.rs `TerminalAttach`). The server sends
//! the terminal's screen as escape sequences (its scrollback, its screen, the program's modes),
//! then what its program prints as it prints it; this terminal's keys go to the program as
//! typed, and its size, which the terminal takes while it's attached. Ctrl+\ lets go, as dtach's
//! does, and the terminal runs on in Codealong.
//!
//! This terminal is put in raw mode while it's attached, so every key goes through as it is, and
//! put back as it lets go, with the modes the program had it in turned off again.

use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use anyhow::{anyhow, bail, Result};

use crate::server::{AttachIn, Request};

/// The key that lets go: Ctrl+\, as dtach's.
const LET_GO: u8 = 0x1c;

/// What letting go turns off again, of what a program may have turned on in this terminal: the
/// other screen, mouse reports, focus reports, bracketed pastes, the Kitty keyboard protocol and
/// modifyOtherKeys; and the cursor shown and the colors reset.
const PUT_BACK: &str = "\x1b[?1049l\x1b[?1000l\x1b[?1002l\x1b[?1003l\x1b[?1006l\x1b[?1004l\x1b[?2004l\x1b[=0;1u\x1b[>4;0m\x1b[?25h\x1b[0m";

/// How often this terminal's size is looked at.
const SIZE_EVERY: Duration = Duration::from_millis(250);

/// This terminal's settings before it was put in raw mode, put back as it lets go.
static SAVED: OnceLock<libc::termios> = OnceLock::new();

pub fn run(what: &str) -> Result<()> {
    // SAFETY: isatty only asks about a descriptor.
    if unsafe { libc::isatty(0) } != 1 || unsafe { libc::isatty(1) } != 1 {
        bail!("`codealong terminal attach` shows a terminal of Codealong's in this one, so it runs only in a terminal.");
    }
    crate::serve::ensure(false, None).map_err(|e| anyhow!(e))?;
    let mut stream = UnixStream::connect(crate::server::path())?;
    let (cols, rows) = size_of(1).unwrap_or((80, 24));
    writeln!(stream, "{}", serde_json::to_string(&Request::TerminalAttach { terminal: what.to_string(), cols, rows })?)?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let reply: serde_json::Value = serde_json::from_str(&line).map_err(|_| anyhow!("Codealong's server didn't answer as it should; is it this build?"))?;
    if reply["ok"] != true {
        bail!("{}", reply["error"].as_str().unwrap_or("Couldn't attach to that terminal."));
    }
    let r = &reply["result"];
    let name = [r["title"].as_str(), r["shell"].as_str()].into_iter().flatten().map(str::trim).find(|n| !n.is_empty()).unwrap_or("the terminal").to_string();
    raw_mode()?;
    let writer = Arc::new(Mutex::new(stream));
    // What the program prints, to this terminal: the screen first, which is in the reader already.
    {
        let name = name.clone();
        std::thread::spawn(move || {
            let mut out = std::io::stdout();
            let mut buf = vec![0u8; 64 * 1024];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if out.write_all(&buf[..n]).and_then(|()| out.flush()).is_err() {
                            break;
                        }
                    }
                }
            }
            finish(&format!("{name} ended, or was closed in Codealong."));
        });
    }
    // This terminal's size as it changes, which the terminal takes.
    {
        let writer = writer.clone();
        std::thread::spawn(move || {
            let mut last = (cols, rows);
            loop {
                std::thread::sleep(SIZE_EVERY);
                if let Some(now) = size_of(1).filter(|now| *now != last) {
                    last = now;
                    if send(&writer, &AttachIn::Size { cols: now.0, rows: now.1 }).is_err() {
                        return;
                    }
                }
            }
        });
    }
    // What's typed here, until Ctrl+\.
    let mut stdin = std::io::stdin().lock();
    let mut buf = [0u8; 4096];
    loop {
        let n = match stdin.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        let typed = &buf[..n];
        if let Some(at) = typed.iter().position(|b| *b == LET_GO) {
            if at > 0 {
                let _ = send(&writer, &AttachIn::Text { text: String::from_utf8_lossy(&typed[..at]).into_owned() });
            }
            let _ = send(&writer, &AttachIn::Detach);
            break;
        }
        if send(&writer, &AttachIn::Text { text: String::from_utf8_lossy(typed).into_owned() }).is_err() {
            break;
        }
    }
    finish(&format!("Let go of {name}: it runs on in Codealong."));
}

fn send(to: &Mutex<UnixStream>, what: &AttachIn) -> std::io::Result<()> {
    let line = serde_json::to_string(what).map_err(std::io::Error::other)?;
    let mut to = to.lock().unwrap_or_else(|e| e.into_inner());
    writeln!(to, "{line}")
}

/// Puts this terminal back as it was and says why it let go.
fn finish(said: &str) -> ! {
    if let Some(saved) = SAVED.get() {
        // SAFETY: tcsetattr with settings tcgetattr gave for the same descriptor.
        unsafe { libc::tcsetattr(0, libc::TCSANOW, saved) };
    }
    let mut out = std::io::stdout();
    let _ = write!(out, "{PUT_BACK}\r\n{said}\r\n");
    let _ = out.flush();
    std::process::exit(0)
}

/// Raw mode: every key reaches the program as it's typed, nothing echoed or taken by this
/// terminal's line discipline (Ctrl+C included, which goes to the program).
fn raw_mode() -> Result<()> {
    // SAFETY: tcgetattr fills a plain struct for an open descriptor; cfmakeraw and tcsetattr take it.
    unsafe {
        let mut t: libc::termios = std::mem::zeroed();
        if libc::tcgetattr(0, &mut t) != 0 {
            bail!("Couldn't read this terminal's settings: {}", std::io::Error::last_os_error());
        }
        let _ = SAVED.set(t);
        libc::cfmakeraw(&mut t);
        if libc::tcsetattr(0, libc::TCSANOW, &t) != 0 {
            bail!("Couldn't put this terminal in raw mode: {}", std::io::Error::last_os_error());
        }
    }
    Ok(())
}

/// A terminal's size in cells, by a descriptor on it.
fn size_of(fd: i32) -> Option<(u16, u16)> {
    // SAFETY: TIOCGWINSZ fills a plain struct for an open descriptor.
    unsafe {
        let mut ws: libc::winsize = std::mem::zeroed();
        (libc::ioctl(fd, libc::TIOCGWINSZ, &mut ws) == 0 && ws.ws_col > 0 && ws.ws_row > 0).then_some((ws.ws_col, ws.ws_row))
    }
}
