//! The ways a prompt reaches a Claude Code session, from its registry entry.

use std::path::Path;

use serde_json::Value;

use crate::model::{Session, Surface};
use crate::send::Route;

/// Claude Code's ways in, best first, from its registry entry and whether its daemon runs.
/// Pure, so the choice has tests. A live session: its daemon for a background job, its
/// terminal, then its cross-session socket; one that isn't running: the Claude app for an app
/// session, else a terminal that resumes it.
pub fn claude_routes(session: &Session, registry: Option<&Value>, home: &Path, daemon: Option<&Path>, mode: Option<String>) -> Vec<Route> {
    if !session.live {
        return vec![if session.surface == Surface::App { Route::ClaudeApp } else { Route::Resume }];
    }
    let mut out = Vec::new();
    let kind = registry.and_then(|r| r.get("kind")).and_then(Value::as_str);
    let job = registry.and_then(|r| r.get("jobId")).and_then(Value::as_str);
    if let (Some("bg"), Some(job), Some(socket)) = (kind, job, daemon) {
        out.push(Route::Daemon { socket: socket.to_path_buf(), key_file: home.join("daemon/control.key"), job: job.to_string() });
    }
    if session.surface == Surface::Cli && kind != Some("bg") {
        if let Some(pid) = session.pid {
            out.push(Route::Type { pid });
        }
    }
    let peer = registry
        .filter(|r| r.get("peerProtocol").and_then(Value::as_u64) == Some(1))
        .and_then(|r| Some((r.get("messagingSocketPath")?.as_str()?, r.get("sessionId")?.as_str()?)));
    if let Some((socket, id)) = peer {
        out.push(Route::Peer { socket: socket.to_string(), session_id: id.to_string(), mode });
    }
    out
}

/// A live Claude Code session's registry entry, if it's this session's.
pub fn claude_registry(home: &Path, session: &Session) -> Option<Value> {
    session.pid.and_then(|pid| read_entry(home, pid)).filter(|r| r.get("sessionId").and_then(Value::as_str) == Some(session.id.as_str()))
}

/// A live session's registry entry: `<home>/sessions/<pid>.json`.
fn read_entry(home: &Path, pid: u32) -> Option<Value> {
    serde_json::from_str(&std::fs::read_to_string(home.join("sessions").join(format!("{pid}.json"))).ok()?).ok()
}
