//! Actions Codex owns, done the way the Codex app does them: a short-lived `codex app-server`
//! over stdio. Archiving a thread moves its rollout into `archived_sessions` and marks it in
//! Codex's shared state database, so the Codex app sees the change the next time it reads its
//! threads. See *Harness adapters*, Archiving.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{channel, Receiver};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};

use crate::procs;

const TIMEOUT: Duration = Duration::from_secs(20);

/// The Codex CLI the installed app ships, so the protocol matches the app's own; else `codex`
/// on the PATH.
fn codex_binary() -> PathBuf {
    crate::detect::codex_app_cli().unwrap_or_else(|| PathBuf::from("codex"))
}

/// Sends one request to a fresh app-server for the account at `codex_home` and returns its result.
pub fn call(codex_home: &Path, method: &str, params: Value) -> Result<Value> {
    let mut child = Command::new(codex_binary())
        .args(["app-server", "--listen", "stdio://"])
        .env("CODEX_HOME", codex_home)
        .env("PATH", procs::user_path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .context("Couldn't start Codex. Is the Codex app installed?")?;
    let mut stdin = child.stdin.take().ok_or_else(|| anyhow!("no stdin"))?;
    let stdout = child.stdout.take().ok_or_else(|| anyhow!("no stdout"))?;
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(std::result::Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    let deadline = Instant::now() + TIMEOUT;
    let result = (|| {
        writeln!(stdin, "{}", json!({"id": 1, "method": "initialize", "params": {"clientInfo": {"name": "codealong", "title": "Codealong", "version": env!("CARGO_PKG_VERSION")}}}))?;
        response(&rx, 1, deadline)?;
        writeln!(stdin, "{}", json!({"method": "initialized"}))?;
        writeln!(stdin, "{}", json!({"id": 2, "method": method, "params": params}))?;
        response(&rx, 2, deadline)
    })();
    drop(stdin);
    let _ = child.kill();
    let _ = child.wait();
    result
}

/// Waits for the response to request `id`, skipping notifications.
fn response(rx: &Receiver<String>, id: i64, deadline: Instant) -> Result<Value> {
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            bail!("Codex didn't answer in time.");
        }
        let line = rx.recv_timeout(left).map_err(|_| anyhow!("Codex stopped before it answered."))?;
        let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };
        if v.get("id").and_then(Value::as_i64) != Some(id) {
            continue;
        }
        if let Some(err) = v.get("error") {
            let msg = err.get("message").and_then(Value::as_str).unwrap_or("unknown error");
            bail!("Codex: {msg}");
        }
        return Ok(v.get("result").cloned().unwrap_or(Value::Null));
    }
}

/// Archives or unarchives a thread in Codex.
pub fn set_archived(codex_home: &Path, thread_id: &str, archived: bool) -> Result<()> {
    let method = if archived { "thread/archive" } else { "thread/unarchive" };
    call(codex_home, method, json!({ "threadId": thread_id })).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Against the real Codex: `CODEALONG_TEST_THREAD=<an archived thread> cargo test -- --ignored`.
    /// Unarchives the thread and archives it again, leaving it as it was.
    #[test]
    #[ignore]
    fn archive_round_trip_through_codex() {
        let Ok(thread) = std::env::var("CODEALONG_TEST_THREAD") else { return };
        let home = crate::util::home().join(".codex");
        let archived = || std::fs::read_dir(home.join("archived_sessions")).unwrap().flatten().any(|e| e.file_name().to_string_lossy().contains(&thread));
        assert!(archived(), "pick a thread that's archived");
        set_archived(&home, &thread, false).unwrap();
        assert!(!archived());
        set_archived(&home, &thread, true).unwrap();
        assert!(archived());
    }
}
