//! The ways a prompt reaches a Claude Code session, from its registry entry. Its own are the
//! background service's socket (daemon.rs), the session's (peer.rs) and the Claude app (app.rs).

use std::path::Path;

use super::app::InApp;
use super::daemon::Background;
use super::peer::Message;
use super::registry::RegistryEntry;
use crate::model::{Session, Surface};
use crate::send::Route;

/// Claude Code's ways in, best first, from its registry entry and whether its daemon runs.
/// Pure, so the choice has tests. A live session: its daemon for a background job, its
/// terminal, then its cross-session socket; one that isn't running: the Claude app for an app
/// session, else a terminal that resumes it.
pub fn claude_routes(session: &Session, registry: Option<&RegistryEntry>, home: &Path, daemon: Option<&Path>, mode: Option<String>) -> Vec<Route> {
    if !session.live {
        return vec![if session.surface == Surface::App { InApp.into() } else { Route::Resume }];
    }
    let mut out = Vec::new();
    let kind = registry.and_then(|r| r.kind.as_deref());
    let job = registry.and_then(|r| r.job_id.as_deref());
    if let (Some("bg"), Some(job), Some(socket)) = (kind, job, daemon) {
        out.push(Background { socket: socket.to_path_buf(), key_file: home.join("daemon/control.key"), job: job.to_string() }.into());
    }
    if session.surface == Surface::Cli && kind != Some("bg") {
        if let Some(pid) = session.pid {
            out.push(Route::Type { pid });
        }
    }
    let peer = registry.filter(|r| r.peer_protocol == Some(1)).and_then(|r| Some((r.messaging_socket.as_deref()?, r.session_id.as_str())));
    if let Some((socket, id)) = peer {
        out.push(Message { socket: socket.to_string(), session_id: id.to_string(), mode }.into());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::claude::registry::parse_registry;
    use crate::model::{Harness, Status};
    use crate::send::{ensure_not_waiting, Sender};
    use serde_json::{json, Value};

    fn session(surface: Surface, status: Status, live: bool, pid: Option<u32>) -> Session {
        Session { surface, status, live, pid, title: "Fix the build".into(), cwd: "/tmp".into(), ..Session::new(Harness::Claude, "claude", "abc") }
    }

    /// The ways `send` would try, with the registry and daemon given rather than read.
    fn route_for(s: &Session, registry: Option<&RegistryEntry>, home: &Path, daemon: Option<&Path>) -> anyhow::Result<Vec<Route>> {
        ensure_not_waiting(s, &Sender::You)?;
        Ok(claude_routes(s, registry, home, daemon, None))
    }

    fn registry(extra: Value) -> RegistryEntry {
        let mut r = json!({ "pid": 42, "sessionId": "abc", "cwd": "/tmp", "peerProtocol": 1, "messagingSocketPath": "/tmp/cc-socks/42.sock", "kind": "interactive" });
        r.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
        parse_registry(&r.to_string()).unwrap()
    }

    /// The session's socket, as a way in.
    fn socket(mode: Option<&str>) -> Route {
        Message { socket: "/tmp/cc-socks/42.sock".into(), session_id: "abc".into(), mode: mode.map(String::from) }.into()
    }

    #[test]
    fn answers_a_question_its_reply_asks_like_any_prompt() {
        let mut s = session(Surface::Cli, Status::Needs, true, Some(42));
        s.asks = true;
        let routes = route_for(&s, Some(&registry(json!({ "status": "idle" }))), Path::new("/h"), None).unwrap();
        assert_eq!(routes, vec![Route::Type { pid: 42 }, socket(None)]);
    }

    #[test]
    fn types_into_a_terminal_session_and_falls_back_to_its_socket() {
        let s = session(Surface::Cli, Status::Working, true, Some(42));
        let routes = route_for(&s, Some(&registry(json!({}))), Path::new("/h"), None).unwrap();
        assert_eq!(routes, vec![Route::Type { pid: 42 }, socket(None)]);
    }

    #[test]
    fn tells_the_socket_route_the_mode_the_session_runs_in() {
        let s = session(Surface::App, Status::Unread, true, Some(42));
        let routes = claude_routes(&s, Some(&registry(json!({ "entrypoint": "claude-desktop" }))), Path::new("/h"), None, Some("bypassPermissions".into()));
        assert_eq!(routes, vec![socket(Some("bypassPermissions"))]);
    }

    #[test]
    fn reaches_claude_app_sessions_only_through_their_socket() {
        let s = session(Surface::App, Status::Unread, true, Some(42));
        let routes = route_for(&s, Some(&registry(json!({ "entrypoint": "claude-desktop" }))), Path::new("/h"), None).unwrap();
        assert_eq!(routes, vec![socket(None)]);
    }

    #[test]
    fn hands_background_jobs_to_the_daemon_first() {
        let s = session(Surface::Cli, Status::Idle, true, Some(42));
        let reg = registry(json!({ "kind": "bg", "jobId": "6e07c149" }));
        let routes = route_for(&s, Some(&reg), Path::new("/h"), Some(Path::new("/tmp/d/control.sock"))).unwrap();
        assert_eq!(routes, vec![Background { socket: "/tmp/d/control.sock".into(), key_file: "/h/daemon/control.key".into(), job: "6e07c149".into() }.into(), socket(None)]);
        // Without the daemon running, the job's socket still takes it.
        let routes = route_for(&s, Some(&reg), Path::new("/h"), None).unwrap();
        assert_eq!(routes, vec![socket(None)]);
    }

    #[test]
    fn runs_a_shell_command_only_where_its_typed_into_the_box() {
        use crate::send::{shell_command, typed_routes};
        assert!(shell_command("! make test") && shell_command("  !ls") && !shell_command("Run `! ls` for me"));
        // A terminal session's terminal; never its socket, where the agent would get the words.
        let s = session(Surface::Cli, Status::Idle, true, Some(42));
        assert_eq!(typed_routes(claude_routes(&s, Some(&registry(json!({}))), Path::new("/h"), None, None)), vec![Route::Type { pid: 42 }]);
        // A background job's daemon pastes it into the job's box.
        let reg = registry(json!({ "kind": "bg", "jobId": "6e07c149" }));
        let job = Background { socket: "/tmp/d/control.sock".into(), key_file: "/h/daemon/control.key".into(), job: "6e07c149".into() };
        assert_eq!(typed_routes(claude_routes(&s, Some(&reg), Path::new("/h"), Some(Path::new("/tmp/d/control.sock")), None)), vec![job.into()]);
        // A Claude app session, or one that isn't running, has no box to type it into.
        let app = session(Surface::App, Status::Idle, true, Some(42));
        assert!(typed_routes(claude_routes(&app, Some(&registry(json!({ "entrypoint": "claude-desktop" }))), Path::new("/h"), None, None)).is_empty());
        let stopped = session(Surface::Cli, Status::Stopped, false, None);
        assert!(typed_routes(claude_routes(&stopped, None, Path::new("/h"), None, None)).is_empty());
    }

    #[test]
    fn skips_a_socket_of_a_protocol_it_doesnt_know() {
        let s = session(Surface::App, Status::Idle, true, Some(42));
        let routes = route_for(&s, Some(&registry(json!({ "peerProtocol": 2 }))), Path::new("/h"), None).unwrap();
        assert!(routes.is_empty());
    }

    #[test]
    fn resumes_sessions_that_arent_running() {
        let s = session(Surface::Cli, Status::Stopped, false, None);
        assert_eq!(route_for(&s, None, Path::new("/h"), None).unwrap(), vec![Route::Resume]);
        let s = session(Surface::App, Status::Idle, false, None);
        assert_eq!(route_for(&s, None, Path::new("/h"), None).unwrap(), vec![InApp.into()]);
    }
}
