//! The app's socket for the `codealong` command (cli.rs): JSON lines over a Unix socket next to
//! the config, one request per line and one reply per line. A request is what a window's Tauri
//! command does, through the same engine handle, so the engine stays the one writer of
//! todos.json and a reply is the board the windows have. The socket is mode 0600: it answers
//! the user running the app, and only them.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::support_dir;
use crate::engine::Handle;
use crate::start::StartSpec;
use crate::todos::{PendingStart, TodoOp};

pub fn path() -> PathBuf {
    support_dir().join("codealong.sock")
}

/// What the command can ask. `method` picks one; the rest are its arguments.
#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "method", rename_all = "kebab-case", rename_all_fields = "camelCase")]
pub enum Request {
    /// Is the app up, and which one.
    Status,
    /// The board as the windows have it.
    Board,
    /// A change to a todo or the labels; the reply is the time of the board that shows it.
    TodoApply { op: TodoOp },
    /// A session started for a todo, as `start_session` reports it.
    TodoStarted { id: String, lane: Option<String>, session: Option<String>, pending: Option<PendingStart> },
    /// Starts a session, and links it to its todo.
    Start { spec: StartSpec },
    /// Sessions whose prompts and replies have every word (search.rs).
    Search { words: Vec<String> },
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Reply {
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl Reply {
    fn ok(v: impl Serialize) -> Reply {
        match serde_json::to_value(v) {
            Ok(result) => Reply { ok: true, result: Some(result), error: None },
            Err(e) => Reply::err(format!("Couldn't encode the reply: {e}")),
        }
    }
    fn err(e: impl Into<String>) -> Reply {
        Reply { ok: false, result: None, error: Some(e.into()) }
    }
}

/// What `status` says about the app.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub version: String,
    pub pid: u32,
    /// The board's time, and whether the first scan is still running.
    pub generated_at: i64,
    pub loading: bool,
}

/// Listens on the socket for as long as the app runs. Errors are printed, not fatal: the
/// command falls back to the files when there's no socket.
pub fn serve(engine: Handle) {
    let p = path();
    let _ = std::fs::create_dir_all(support_dir());
    // A socket file left by an app that didn't exit cleanly; a live one would have answered.
    if p.exists() && UnixStream::connect(&p).is_err() {
        let _ = std::fs::remove_file(&p);
    }
    let listener = match UnixListener::bind(&p) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Couldn't open {}: {e}. The codealong command will read the files instead.", p.display());
            return;
        }
    };
    let _ = std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600));
    std::thread::Builder::new()
        .name("socket".into())
        .spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                let engine = engine.clone();
                let _ = std::thread::Builder::new().name("socket-client".into()).spawn(move || connection(stream, &engine));
            }
        })
        .ok();
}

fn connection(stream: UnixStream, engine: &Handle) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(60)));
    let mut out = match stream.try_clone() {
        Ok(s) => s,
        Err(_) => return,
    };
    let reader = BufReader::new(stream);
    for line in reader.lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let reply = handle(engine, &line);
        let mut text = serde_json::to_string(&reply).unwrap_or_else(|_| r#"{"ok":false,"error":"Couldn't encode the reply."}"#.into());
        text.push('\n');
        if out.write_all(text.as_bytes()).and_then(|_| out.flush()).is_err() {
            break;
        }
    }
}

pub fn handle(engine: &Handle, line: &str) -> Reply {
    let req: Request = match serde_json::from_str(line) {
        Ok(r) => r,
        Err(e) => return Reply::err(format!("That isn't a request the app knows: {e}")),
    };
    match req {
        Request::Status => {
            let b = engine.board();
            Reply::ok(Status { version: env!("CARGO_PKG_VERSION").to_string(), pid: std::process::id(), generated_at: b.generated_at, loading: b.loading })
        }
        Request::Board => Reply::ok(engine.board()),
        Request::TodoApply { op } => match engine.todo_within(op, Duration::from_secs(90)) {
            Ok(at) => Reply::ok(at),
            Err(e) => Reply::err(e),
        },
        Request::TodoStarted { id, lane, session, pending } => {
            engine.todo_started(id, lane, session, pending);
            Reply::ok(true)
        }
        Request::Search { words } => Reply::ok(crate::search::find(&words, 1000)),
        Request::Start { spec } => match crate::start_and_link(engine, &spec) {
            Ok(started) => Reply::ok(started),
            Err(e) => Reply::err(e),
        },
    }
}

/// One request to the running app, or None when no app answers on the socket.
pub fn ask(req: &Request) -> Option<Result<Value, String>> {
    let mut stream = UnixStream::connect(path()).ok()?;
    let _ = stream.set_read_timeout(Some(Duration::from_secs(120)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
    let mut text = serde_json::to_string(req).ok()?;
    text.push('\n');
    stream.write_all(text.as_bytes()).ok()?;
    stream.flush().ok()?;
    let mut line = String::new();
    BufReader::new(stream).read_line(&mut line).ok()?;
    if line.trim().is_empty() {
        return None;
    }
    let reply: Reply = match serde_json::from_str(&line) {
        Ok(r) => r,
        Err(e) => return Some(Err(format!("The app's reply didn't read: {e}"))),
    };
    Some(if reply.ok { Ok(reply.result.unwrap_or(Value::Null)) } else { Err(reply.error.unwrap_or_else(|| "The app said no.".into())) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_round_trip_as_json_lines() {
        let op = TodoOp::Update { id: "t-1".into(), patch: crate::todos::TodoPatch { status: Some(crate::todos::TodoStatus::Done), ..Default::default() } };
        let text = serde_json::to_string(&Request::TodoApply { op }).unwrap();
        assert!(text.contains(r#""method":"todo-apply""#) && text.contains(r#""status":"done""#), "{text}");
        let back: Request = serde_json::from_str(&text).unwrap();
        assert!(matches!(back, Request::TodoApply { op: TodoOp::Update { .. } }));
        let status: Request = serde_json::from_str(r#"{"method":"status"}"#).unwrap();
        assert!(matches!(status, Request::Status));
        let reply: Reply = serde_json::from_str(r#"{"ok":false,"error":"no"}"#).unwrap();
        assert!(!reply.ok && reply.error.as_deref() == Some("no"));
    }
}
