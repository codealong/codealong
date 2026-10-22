//! Claude Code's background service, through its control socket
//! (`/tmp/cc-daemon-<uid>/<hash>/control.sock`): handing a background job a prompt, which is a
//! way in (`Background`), and typing into its terminal (Escape, or a prompt with the keys that
//! send it now).

use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Result};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::SEND_NOW;
use crate::attachments::{self, Style};
use crate::launch::pasted;
use crate::send::{boxed, sent, Delivery, MayHaveArrived, Own, Sent, Way};

/// Claude Code's daemon socket for an account, when its daemon is running.
pub fn daemon_socket(home: &Path) -> Option<PathBuf> {
    let path = daemon_socket_path(home, unsafe { libc::getuid() });
    path.exists().then_some(path)
}

/// `/tmp/cc-daemon-<uid>/<first 8 hex of sha256(config dir)>/control.sock`. The config dir is
/// hashed as an absolute path without a trailing slash (`~/.claude2` gave the folder here).
pub fn daemon_socket_path(home: &Path, uid: u32) -> PathBuf {
    let dir = home.to_string_lossy();
    let dir = dir.trim_end_matches('/');
    let hash = Sha256::digest(dir.as_bytes());
    let short: String = hash.iter().take(4).map(|b| format!("{b:02x}")).collect();
    PathBuf::from(format!("/tmp/cc-daemon-{uid}/{short}/control.sock"))
}

/// The daemon's `reply` request: one JSON line each way.
pub fn daemon_request(key: &str, job: &str, text: &str) -> String {
    format!("{}\n", json!({ "proto": 1, "op": "reply", "short": job, "text": text, "auth": key }))
}

/// Hands the prompt to a background job through Claude Code's daemon, which pastes it into the
/// job's terminal. A job that's starting up or restarting says so; it's asked again for a
/// couple of seconds, as Claude Code's own agent view does.
fn daemon_reply(socket: &Path, key: &str, job: &str, text: &str) -> Result<()> {
    let request = daemon_request(key, job, text);
    let mut last = String::new();
    for _ in 0..10 {
        let mut stream = UnixStream::connect(socket)?;
        stream.set_read_timeout(Some(Duration::from_secs(10)))?;
        stream.set_write_timeout(Some(Duration::from_secs(5)))?;
        stream.write_all(request.as_bytes())?;
        let mut line = String::new();
        BufReader::new(&stream).read_line(&mut line)?;
        let answer: Value = serde_json::from_str(line.trim()).map_err(|_| anyhow!("Claude Code's daemon gave an answer Codealong can't read."))?;
        if answer.get("ok").and_then(Value::as_bool) == Some(true) {
            return Ok(());
        }
        let code = answer.get("code").and_then(Value::as_str).unwrap_or("");
        last = answer.get("error").and_then(Value::as_str).unwrap_or(code).to_string();
        if !matches!(code, "ESTARTING" | "ENOREPLY" | "ERESPAWNING") {
            break;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    bail!("Claude Code's daemon didn't take it: {last}")
}

/// Hands a background job `text` as `daemon_reply` does, with the service's key read from its
/// file. A job waiting on its questions takes it as the answer to the first of them
/// (`claude::answer_job_questions`).
pub fn daemon_say(socket: &Path, key_file: &Path, job: &str, text: &str) -> Result<()> {
    let key = std::fs::read_to_string(key_file).map_err(|e| anyhow!("Claude Code's daemon key: {e}"))?;
    daemon_reply(socket, key.trim(), job, text)
}

/// The daemon's `attach` request, as `claude attach` sends it but without a terminal's
/// capabilities (`caps` is optional, and the job goes on without them as it does with nobody
/// attached; null is refused).
pub fn daemon_attach_request(key: &str, job: &str, attach_id: &str) -> String {
    format!("{}\n", json!({ "proto": 1, "op": "attach", "short": job, "auth": key, "cols": 200, "rows": 50, "attachId": attach_id }))
}

/// Presses Escape in a background job's terminal. The daemon has no request for it, and its
/// `reply` pastes text and presses Return.
pub fn daemon_press_escape(socket: &Path, key_file: &Path, job: &str) -> Result<()> {
    daemon_type(socket, key_file, job, &[b"\x1b"])
}

/// Types into a background job's terminal: attaches as `claude attach` does, which hands what
/// comes after the request line to the job's terminal as typed, writes each of `writes` a moment
/// apart (a key that arrives with the paste before it is taken as part of the paste), and
/// detaches. Attaching redraws the job and sizes its terminal to this one; detaching gives a
/// terminal attached meanwhile its own size back. Once writing has started, a failure may have
/// left some of it there (`MayHaveArrived`).
pub fn daemon_type(socket: &Path, key_file: &Path, job: &str, writes: &[&[u8]]) -> Result<()> {
    let key = std::fs::read_to_string(key_file).map_err(|_| anyhow!("Codealong can't read the background service's key."))?;
    let mut stream = UnixStream::connect(socket)?;
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;
    stream.write_all(daemon_attach_request(key.trim(), job, &crate::start::new_session_id()).as_bytes())?;
    // The answer is one line; the job's screen follows it.
    let mut line = Vec::new();
    let mut byte = [0u8; 1];
    while stream.read(&mut byte)? == 1 && byte[0] != b'\n' {
        line.push(byte[0]);
    }
    let answer: Value = serde_json::from_slice(&line).map_err(|_| anyhow!("Claude Code's daemon gave an answer Codealong can't read."))?;
    if answer.get("ok").and_then(Value::as_bool) != Some(true) {
        let why = answer.get("error").and_then(Value::as_str).or_else(|| answer.get("code").and_then(Value::as_str)).unwrap_or("no reason given");
        bail!("Claude Code's daemon didn't let Codealong in: {why}");
    }
    // Apart from what attaching writes to the job first (focus, a redraw), so it reads as a key
    // of its own. The screen is read and dropped meanwhile: the daemon closes a connection whose
    // unread output piles up.
    let drain = |stream: &mut UnixStream, for_ms: u64| {
        let until = Instant::now() + Duration::from_millis(for_ms);
        let mut chunk = [0u8; 65536];
        while let Some(left) = until.checked_duration_since(Instant::now()) {
            let _ = stream.set_read_timeout(Some(left.max(Duration::from_millis(10))));
            match stream.read(&mut chunk) {
                Ok(0) => break,
                Ok(_) => {}
                Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => break,
                Err(_) => break,
            }
        }
    };
    drain(&mut stream, 200);
    for (i, bytes) in writes.iter().enumerate() {
        if i > 0 {
            drain(&mut stream, 300);
        }
        stream.write_all(bytes).map_err(|e| MayHaveArrived(format!("Claude Code's background service stopped taking what Codealong typed into the job partway ({e}): look there before trying again.")))?;
    }
    drain(&mut stream, 400);
    Ok(())
}

/// The way in for a background job: the daemon pastes the prompt into the job's terminal, where
/// it reads as typed, so as its user's own words.
#[derive(Debug, Clone, PartialEq)]
pub struct Background {
    pub socket: PathBuf,
    pub key_file: PathBuf,
    pub job: String,
}

impl Own for Background {
    fn way(&self) -> Way {
        Way::Background
    }

    fn takes_agents(&self) -> bool {
        false
    }

    /// Pasted into the job's box: a reply that starts with `!` runs in its shell, as typed
    /// there (2.1.293's code: the reply's mode is the text's, so `!` is bash mode).
    fn as_typed(&self) -> bool {
        true
    }

    fn deliver(&self, d: &Delivery) -> Result<Sent> {
        let title = d.title();
        let expanded = attachments::expand(d.text, Style::Mention);
        let typed = boxed(&expanded);
        // `reply` pastes and presses Return; the send-now keys go in through an attach.
        if d.now {
            daemon_type(&self.socket, &self.key_file, &self.job, &[pasted(&typed).as_bytes(), SEND_NOW.as_bytes()])?;
            return Ok(sent(format!("Sent now to {title} in the background: it reads it without waiting for its current step.")));
        }
        let since = crate::util::now_ms();
        daemon_say(&self.socket, &self.key_file, &self.job, &typed)?;
        if !d.busy() {
            return Ok(sent(format!("Sent to {title} in the background.")));
        }
        let queued = crate::harness::adapter(d.session.harness).queued_as(d.account, d.session, &typed, since);
        Ok(Sent { queued, ..sent(format!("Queued for {title}; the background session takes it after its current step.")) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A folder of its own under the system's temporary one, short enough for a socket path.
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ca-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn finds_the_daemon_socket_by_the_config_dirs_hash() {
        // The first 8 hex of sha256 of the config folder, without a trailing slash (checked
        // against the folder the daemon made for a real ~/.claude2).
        assert_eq!(daemon_socket_path(Path::new("/Users/someone/.claude2"), 501), PathBuf::from("/tmp/cc-daemon-501/44bc50fb/control.sock"));
        assert_eq!(daemon_socket_path(Path::new("/Users/someone/.claude2/"), 501), PathBuf::from("/tmp/cc-daemon-501/44bc50fb/control.sock"));
    }

    #[test]
    fn writes_the_daemon_reply_as_one_line() {
        let line = daemon_request("k", "6e07c149", "two\nlines");
        assert!(line.ends_with('\n') && line.matches('\n').count() == 1);
        let v: Value = serde_json::from_str(line.trim()).unwrap();
        assert_eq!(v, json!({ "proto": 1, "op": "reply", "short": "6e07c149", "text": "two\nlines", "auth": "k" }));
    }

    #[test]
    fn presses_escape_in_a_background_job_by_attaching_to_it() {
        let dir = scratch("attach");
        let socket = dir.join("c.sock");
        std::fs::write(dir.join("control.key"), "k\n").unwrap();
        let listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
        let daemon = std::thread::spawn(move || {
            let (mut conn, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(conn.try_clone().unwrap());
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            // The answer, then the job's screen as the daemon starts streaming it.
            conn.write_all(b"{\"ok\":true,\"op\":\"attach\"}\n\x1b[?1049h\x1b[2J screen").unwrap();
            let mut typed = Vec::new();
            reader.read_to_end(&mut typed).unwrap();
            (serde_json::from_str::<Value>(line.trim()).unwrap(), typed)
        });
        daemon_press_escape(&socket, &dir.join("control.key"), "6e07c149").unwrap();
        let (request, typed) = daemon.join().unwrap();
        assert_eq!((request["op"].as_str(), request["short"].as_str(), request["auth"].as_str()), (Some("attach"), Some("6e07c149"), Some("k")));
        // The daemon refuses a null for what it takes as an object or nothing.
        assert!(request.get("caps").is_none());
        assert_eq!(typed, b"\x1b");

        // A job the daemon doesn't have is refused, with its reason.
        let _ = std::fs::remove_file(&socket);
        let listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
        let daemon = std::thread::spawn(move || {
            let (mut conn, _) = listener.accept().unwrap();
            let mut line = String::new();
            BufReader::new(conn.try_clone().unwrap()).read_line(&mut line).unwrap();
            conn.write_all(b"{\"ok\":false,\"error\":\"job not found\",\"code\":\"ENOJOB\"}\n").unwrap();
        });
        let err = daemon_press_escape(&socket, &dir.join("control.key"), "6e07c149").unwrap_err();
        daemon.join().unwrap();
        assert!(err.to_string().contains("job not found"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// What Claude Code's daemon is written, byte for byte.
    #[test]
    fn the_daemon_is_written_these_lines() {
        assert_eq!(daemon_request("k", "6e07c149", "two\nlines"), concat!(r#"{"auth":"k","op":"reply","proto":1,"short":"6e07c149","text":"two\nlines"}"#, "\n"));
        assert_eq!(daemon_attach_request("k", "6e07c149", "a1"), concat!(r#"{"attachId":"a1","auth":"k","cols":200,"op":"attach","proto":1,"rows":50,"short":"6e07c149"}"#, "\n"));
    }

    /// The daemon is written the reply request once, and its refusal comes back with its reason.
    #[test]
    fn a_daemon_that_says_no_is_said_so() {
        let dir = scratch("reply");
        let socket = dir.join("c.sock");
        let listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
        let daemon = std::thread::spawn(move || {
            let (mut conn, _) = listener.accept().unwrap();
            let mut line = String::new();
            BufReader::new(conn.try_clone().unwrap()).read_line(&mut line).unwrap();
            conn.write_all(b"{\"ok\":false,\"error\":\"job not found\",\"code\":\"ENOJOB\"}\n").unwrap();
            line
        });
        let err = daemon_reply(&socket, "k", "6e07c149", "hi").unwrap_err();
        assert_eq!(daemon.join().unwrap(), daemon_request("k", "6e07c149", "hi"));
        assert_eq!(err.to_string(), "Claude Code's daemon didn't take it: job not found");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
