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
    with_server(codex_home, |s| s.request(method, params))
}

/// A fresh app-server, initialized, for as long as `f` runs.
struct Server {
    stdin: std::process::ChildStdin,
    rx: Receiver<String>,
    next: i64,
    deadline: Instant,
}

impl Server {
    fn request(&mut self, method: &str, params: Value) -> Result<Value> {
        self.next += 1;
        writeln!(self.stdin, "{}", json!({"id": self.next, "method": method, "params": params}))?;
        response(&self.rx, self.next, self.deadline)
    }
}

fn with_server<T>(codex_home: &Path, f: impl FnOnce(&mut Server) -> Result<T>) -> Result<T> {
    let mut child = Command::new(codex_binary())
        .args(["app-server", "--listen", "stdio://"])
        .env("CODEX_HOME", codex_home)
        .env("PATH", procs::user_path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .context("Couldn't start Codex. Is the Codex app installed?")?;
    let stdin = child.stdin.take().ok_or_else(|| anyhow!("no stdin"))?;
    let stdout = child.stdout.take().ok_or_else(|| anyhow!("no stdout"))?;
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(std::result::Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    let mut server = Server { stdin, rx, next: 0, deadline: Instant::now() + TIMEOUT };
    let result = (|| {
        server.request("initialize", json!({"clientInfo": {"name": "codealong", "title": "Codealong", "version": env!("CARGO_PKG_VERSION")}}))?;
        writeln!(server.stdin, "{}", json!({"method": "initialized"}))?;
        f(&mut server)
    })();
    drop(server);
    let _ = child.kill();
    let _ = child.wait();
    result
}

/// A thread made for the Codex app to run: its id, and the model and effort the account's
/// settings chose for it.
pub struct NewThread {
    pub id: String,
    pub model: Option<String>,
    pub effort: Option<String>,
}

/// Makes a thread in `cwd` that the Codex app can open, without running a turn in it. A new
/// thread's rollout isn't written until something is in its history, and the app can't open a
/// thread without one, so a short note goes in first. The first turn then goes to the app
/// (`send::codex_start_turn`), which runs it as if you'd typed it there.
pub fn new_thread(codex_home: &Path, cwd: &Path, model: Option<&str>) -> Result<NewThread> {
    with_server(codex_home, |s| {
        let mut params = json!({ "cwd": cwd });
        if let Some(m) = model {
            params["model"] = json!(m);
        }
        let r = s.request("thread/start", params)?;
        let id = r.pointer("/thread/id").and_then(Value::as_str).ok_or_else(|| anyhow!("Codex didn't make a thread."))?.to_string();
        let note = json!({ "type": "message", "role": "developer", "content": [{ "type": "input_text", "text": "This thread was started from Codealong." }] });
        s.request("thread/inject_items", json!({ "threadId": id, "items": [note] }))?;
        let text = |k: &str| r.get(k).and_then(Value::as_str).filter(|v| !v.is_empty()).map(String::from);
        Ok(NewThread { model: text("model"), effort: text("reasoningEffort"), id })
    })
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

    /// Against the real Codex app: `CODEALONG_TEST_CODEX_START=<folder> cargo test codex_app_runs
    /// -- --ignored`. Makes a thread in the folder and has the app run a first turn in it, as a
    /// start from Codealong does; the thread stays in Codex, to archive when done.
    #[test]
    #[ignore]
    fn codex_app_runs_a_new_thread() {
        let Ok(folder) = std::env::var("CODEALONG_TEST_CODEX_START") else { return };
        let home = crate::util::home().join(".codex");
        let t = new_thread(&home, Path::new(&folder), None).unwrap();
        let model = t.model.as_deref().map(|m| (m, t.effort.as_deref()));
        crate::send::codex_start_turn(&home, &t.id, "Reply with just the word pong.", model).unwrap();
        println!("thread {}", t.id);
    }

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
