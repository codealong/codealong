//! `codealong terminal attach <terminal>`: one of Codealong's own terminals shown in this one, as a
//! terminal multiplexer's client shows a session (server.rs `TerminalAttach`). The server sends
//! the terminal's screen as escape sequences (its scrollback, its screen, the program's modes),
//! then what its program prints as it prints it; this terminal's keys go to the program as
//! typed, and its size, which the terminal takes while it's attached. Ctrl+\ lets go, as dtach's
//! does, and the terminal runs on in Codealong.
//!
//! This terminal is put in raw mode while it's attached, so every key goes through as it is, and
//! put back as it lets go, with the modes the program had it in turned off again.

use std::io::{BufRead, BufReader, IsTerminal, Read, Write};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{anyhow, bail, Result};

use crate::os::sock::UnixStream;
use crate::os::tty;
use crate::server::{AttachIn, Request};

/// What this terminal sends for Ctrl+\, which lets go, as dtach's does: the byte itself, or its
/// encodings under the Kitty keyboard protocol (with the event type and without), which this
/// terminal uses once the program it shows turns that on (Claude Code and Codex do).
const LET_GO: [&[u8]; 3] = [b"\x1c", b"\x1b[92;5u", b"\x1b[92;5:1u"];

/// How long what may be the start of the let-go sequence, or of a character, is held for the rest
/// before it goes as it is.
const HOLD: i32 = 50;

/// What letting go turns off again, of what a program may have turned on in this terminal: the
/// other screen, mouse reports, focus reports, bracketed pastes, the Kitty keyboard protocol and
/// modifyOtherKeys; and the cursor shown and the colors reset.
const PUT_BACK: &str = "\x1b[?1049l\x1b[?1000l\x1b[?1002l\x1b[?1003l\x1b[?1006l\x1b[?1004l\x1b[?2004l\x1b[=0;1u\x1b[>4;0m\x1b[?25h\x1b[0m";

/// How often this terminal's size is looked at.
const SIZE_EVERY: Duration = Duration::from_millis(250);

pub fn run(what: &str) -> Result<()> {
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        bail!("`codealong terminal attach` shows a terminal of Codealong's in this one, so it runs only in a terminal.");
    }
    crate::serve::ensure(false, None).map_err(|e| anyhow!(e))?;
    let mut stream = UnixStream::connect(crate::server::path())?;
    let (cols, rows) = tty::size().unwrap_or((80, 24));
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
    // Every key reaches the program as it's typed, nothing echoed or taken by this terminal's line
    // discipline (Ctrl+C included, which goes to the program).
    tty::raw()?;
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
                if let Some(now) = tty::size().filter(|now| *now != last) {
                    last = now;
                    if send(&writer, &AttachIn::Size { cols: now.0, rows: now.1 }).is_err() {
                        return;
                    }
                }
            }
        });
    }
    // What's typed here, until Ctrl+\. Read from stdin itself, not through a buffer of the standard
    // library's, so a wait for more (`HOLD`) sees everything that came (`tty::read_stdin`).
    let mut buf = [0u8; 4096];
    let mut held: Vec<u8> = Vec::new();
    loop {
        // What's held for the let-go sequence goes as it is once nothing more comes in time (an
        // Escape alone, say); a character that hasn't come whole waits for the rest however long.
        if waits_for_more(&held) && !tty::stdin_ready(HOLD) {
            let now = timed_out(&mut held);
            if send(&writer, &AttachIn::Text { text: String::from_utf8_lossy(&now).into_owned() }).is_err() {
                break;
            }
            continue;
        }
        let n = match tty::read_stdin(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        };
        let (text, lets_go) = typed(&mut held, &buf[..n]);
        if !text.is_empty() && send(&writer, &AttachIn::Text { text: String::from_utf8_lossy(&text).into_owned() }).is_err() {
            break;
        }
        if lets_go {
            let _ = send(&writer, &AttachIn::Detach);
            break;
        }
    }
    finish(&format!("Let go of {name}: it runs on in Codealong."));
}

/// What was typed, split into what goes to the program and whether it lets go there
/// (`LET_GO`). `held` keeps what may be the start of a let-go sequence, or a character split
/// between reads, for the next: it goes with what follows it.
fn typed(held: &mut Vec<u8>, new: &[u8]) -> (Vec<u8>, bool) {
    let mut all = std::mem::take(held);
    all.extend_from_slice(new);
    let found = LET_GO.iter().filter_map(|go| all.windows(go.len()).position(|w| w == *go)).min();
    if let Some(at) = found {
        all.truncate(at);
        return (all, true);
    }
    // The start of a let-go sequence at the end, however short (an escape alone too, whose rest
    // may come in the next read): `HOLD` lets it go as it is if nothing follows.
    let keep = LET_GO.iter().filter(|go| go.len() > 1).flat_map(|go| (1..go.len()).filter(|n| all.ends_with(&go[..*n]))).max().unwrap_or(0);
    let mut tail = all.split_off(all.len() - keep);
    if let Err(e) = std::str::from_utf8(&all) {
        if e.error_len().is_none() {
            let mut cut = all.split_off(e.valid_up_to());
            cut.extend_from_slice(&tail);
            tail = cut;
        }
    }
    *held = tail;
    (all, false)
}

/// How many bytes at the end of `held` are a character not come whole yet.
fn unfinished_character(held: &[u8]) -> usize {
    match std::str::from_utf8(held) {
        Err(e) if e.error_len().is_none() => held.len() - e.valid_up_to(),
        _ => 0,
    }
}

/// Whether `held` waits for a moment only (`HOLD`): it holds more than a character not come whole.
fn waits_for_more(held: &[u8]) -> bool {
    !held.is_empty() && unfinished_character(held) < held.len()
}

/// What's held goes once nothing more came in time, but a character not come whole stays for the
/// rest of it, which may be slow over a slow link.
fn timed_out(held: &mut Vec<u8>) -> Vec<u8> {
    let keep = unfinished_character(held);
    let rest = held.split_off(held.len() - keep);
    std::mem::replace(held, rest)
}

fn send(to: &Mutex<UnixStream>, what: &AttachIn) -> std::io::Result<()> {
    let line = serde_json::to_string(what).map_err(std::io::Error::other)?;
    let mut to = to.lock().unwrap_or_else(|e| e.into_inner());
    writeln!(to, "{line}")
}

/// Puts this terminal back as it was and says why it let go.
fn finish(said: &str) -> ! {
    tty::restore();
    let mut out = std::io::stdout();
    let _ = write!(out, "{PUT_BACK}\r\n{said}\r\n");
    let _ = out.flush();
    std::process::exit(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn go(chunks: &[&[u8]]) -> (Vec<u8>, bool) {
        let mut held = Vec::new();
        let mut sent = Vec::new();
        for c in chunks {
            let (text, lets_go) = typed(&mut held, c);
            sent.extend(text);
            if lets_go {
                return (sent, true);
            }
        }
        sent.extend(held);
        (sent, false)
    }

    #[test]
    fn ctrl_backslash_lets_go_however_its_encoded() {
        assert_eq!(go(&[b"ls\r"]), (b"ls\r".to_vec(), false));
        assert_eq!(go(&[b"ab\x1ccd"]), (b"ab".to_vec(), true));
        assert_eq!(go(&[b"x\x1b[92;5u"]), (b"x".to_vec(), true), "the Kitty protocol's");
        assert_eq!(go(&[b"x\x1b[92;5:1u"]), (b"x".to_vec(), true), "with its event type");
        // Split at every place, the escape alone and its bracket too.
        let seq = b"\x1b[92;5u";
        for at in 1..seq.len() {
            assert_eq!(go(&[b"x", &seq[..at], &seq[at..]]), (b"x".to_vec(), true), "split after {at} bytes");
        }
        // Keys that start alike go through.
        assert_eq!(go(&[b"\x1b[A\x1b[92;2u"]), (b"\x1b[A\x1b[92;2u".to_vec(), false));
    }

    #[test]
    fn a_character_split_between_reads_goes_whole() {
        let word = "日本".as_bytes();
        let (text, _) = go(&[&word[..2], &word[2..4], &word[4..]]);
        assert_eq!(String::from_utf8(text).unwrap(), "日本");
        let mut held = Vec::new();
        let (first, _) = typed(&mut held, &word[..2]);
        assert!(first.is_empty() && held.len() == 2, "held for its end");
        assert!(!waits_for_more(&held), "however long its end takes");
        // An escape held for the let-go sequence goes once nothing follows; a character half
        // come stays.
        let mut held = Vec::new();
        let (sent, _) = typed(&mut held, b"\x1b");
        assert!(sent.is_empty() && waits_for_more(&held), "an escape alone may start the let-go sequence");
        assert_eq!(timed_out(&mut held), b"\x1b");
        assert!(held.is_empty());
        let euro = "€".as_bytes();
        let (sent, _) = typed(&mut held, &euro[..1]);
        assert!(sent.is_empty() && !waits_for_more(&held), "the start of a character isn't let go by the wait");
        assert!(timed_out(&mut held).is_empty());
        assert_eq!(held, &euro[..1]);
        let (rest, _) = typed(&mut held, &euro[1..]);
        assert_eq!(String::from_utf8(rest).unwrap(), "€");
    }
}
