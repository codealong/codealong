//! Sending a prompt to a session that already exists, the way its harness takes one in.
//!
//! Wherever it can, the prompt arrives as if you'd typed it: pasted into the session's Ghostty
//! tab, handed to Claude Code's daemon for a background job, posted to OpenCode's service, or
//! started as a turn by the Codex app itself. A Claude app session has no such way in, so it
//! gets a message on Claude Code's own cross-session socket, which the session reads as coming
//! from another session. A session that isn't running any more is resumed with the prompt.
//!
//! A session that's waiting on you for a permission prompt, or stuck mid-turn, is never typed
//! into: what's typed would answer the prompt. One whose last reply asks you something is at its
//! prompt, so what's typed is its answer. Busy sessions are fine; each harness queues the prompt
//! until the agent can take it.

use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Result};
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::attachments::{self, Style};
use crate::config::{AccountCfg, Config};
use crate::launch;
use crate::model::{Board, Harness, Session, Status, Surface};
use crate::procs;
use crate::start;

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Sent {
    /// One sentence for the panel: where the prompt went, or what to do next.
    pub message: String,
    /// The session or its app was opened for you to send it there, rather than sent.
    pub opened: bool,
}

/// How a prompt reaches a session, tried in order until one works.
#[derive(Debug, Clone, PartialEq)]
pub enum Route {
    /// Claude Code's daemon, for a background job: pasted into the job's terminal as typed.
    Daemon { socket: PathBuf, key_file: PathBuf, job: String },
    /// Pasted into the session's Ghostty tab, then Return.
    Type { pid: u32 },
    /// A line on the session's cross-session socket.
    Peer { socket: String, session_id: String },
    /// Not running: a new terminal resumes it with the prompt.
    Resume,
    /// A Claude app session that isn't running: the app opens it, with the prompt on the
    /// clipboard.
    ClaudeApp,
    /// A turn started through the Codex app's own IPC, else the app opens the thread with the
    /// prompt filled in.
    CodexApp,
    Opencode,
    /// Nothing takes a prompt from outside: the app comes forward with it on the clipboard.
    Antigravity,
}

/// Sends `text` to the session with this key.
pub fn send(board: &Board, cfg: &Config, session_key: &str, text: &str) -> Result<Sent> {
    let session = board
        .lanes
        .iter()
        .flat_map(|l| &l.sessions)
        .find(|s| s.key == session_key)
        .ok_or_else(|| anyhow!("That session isn't on the board any more."))?;
    if text.trim().is_empty() {
        bail!("There's nothing to send.");
    }
    let account = cfg.accounts.iter().find(|a| a.name == session.account).ok_or_else(|| anyhow!("There's no account named {} in config.toml.", session.account))?;
    if session.hosted {
        // Codealong runs it: the prompt goes in over its adapter, after the running turn if
        // there is one, and a question it waits on is answered from its panel.
        let message = crate::acp::send(account, session_key, &attachments::expand(text, Style::Path))?;
        return Ok(Sent { message, opened: false });
    }
    ensure_not_waiting(session)?;
    // Each harness says which ways reach the session, best first (`harness::Adapter::routes`).
    let routes = crate::harness::adapter(session.harness).routes(account, session);
    let near = crate::board::terminal_neighbours(&board.lanes, Path::new(&session.cwd));
    let place = launch::Placement { new_terminal: cfg.apps.new_terminal, near: &near };
    let mut last_err = None;
    for route in routes {
        match attempt(&route, session, account, cfg, place, text) {
            Ok(Some(sent)) => return Ok(sent),
            // This way doesn't reach the session; the next one may.
            Ok(None) => {}
            Err(e) => last_err = Some(e),
        }
    }
    Err(last_err.unwrap_or_else(|| anyhow!("Codealong can't reach {} to send it this. Open it and send it there.", quoted(&session.title))))
}

/// A session waiting on you is never typed into: what's typed would answer its prompt. One that
/// only asks you something in its reply (`Session::asks`) takes the answer like any prompt.
fn ensure_not_waiting(session: &Session) -> Result<()> {
    if session.status == Status::Needs && !session.asks {
        bail!("{} is waiting on you; open it to answer.", quoted(&session.title));
    }
    Ok(())
}

/// Claude Code's ways in, best first, from its registry entry and whether its daemon runs.
/// Pure, so the choice has tests. A live session: its daemon for a background job, its
/// terminal, then its cross-session socket; one that isn't running: the Claude app for an app
/// session, else a terminal that resumes it.
pub fn claude_routes(session: &Session, registry: Option<&Value>, home: &Path, daemon: Option<&Path>) -> Vec<Route> {
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
        out.push(Route::Peer { socket: socket.to_string(), session_id: id.to_string() });
    }
    out
}

/// A live Claude Code session's registry entry, if it's this session's.
pub fn claude_registry(home: &Path, session: &Session) -> Option<Value> {
    session.pid.and_then(|pid| read_registry(home, pid)).filter(|r| r.get("sessionId").and_then(Value::as_str) == Some(session.id.as_str()))
}

/// One way in. `Ok(None)` when it doesn't reach this session, so the next way is tried.
fn attempt(route: &Route, session: &Session, account: &AccountCfg, cfg: &Config, place: launch::Placement, text: &str) -> Result<Option<Sent>> {
    let busy = session.status == Status::Working;
    let title = quoted(&session.title);
    let sent = |message: String| Ok(Some(Sent { message, opened: false }));
    match route {
        Route::Daemon { socket, key_file, job } => {
            let key = std::fs::read_to_string(key_file).map_err(|e| anyhow!("Claude Code's daemon key: {e}"))?;
            daemon_reply(socket, key.trim(), job, &attachments::expand(text, Style::Mention))?;
            sent(if busy { format!("Queued for {title}; the background session takes it after its current step.") } else { format!("Sent to {title} in the background.") })
        }
        Route::Type { pid } => {
            let style = if session.harness == Harness::Claude { Style::Mention } else { Style::Path };
            let Some(app) = launch::terminal_type_session(&cfg.apps.terminal, *pid, Path::new(&session.cwd), &attachments::expand(text, style))? else {
                return Ok(None);
            };
            sent(if busy { format!("Typed into {title} in {app}; it takes it after its current step.") } else { format!("Typed into {title} in {app}.") })
        }
        Route::Peer { socket, session_id } => {
            peer_send(socket, session_id, &attachments::expand(text, Style::Path))?;
            sent(format!("Sent to {title} as a message from another session. A session that skips permission prompts asks you before taking it."))
        }
        Route::Resume => {
            let (input, env) = resume_input(account, &session.id, text);
            launch::terminal_run(&cfg.apps.terminal, place, Path::new(&session.cwd), &input, &env)?;
            sent(format!("Resumed {title} in {} with this prompt.", launch::terminal_name(&cfg.apps.terminal)))
        }
        Route::ClaudeApp => {
            copy(&attachments::expand(text, Style::Path))?;
            launch::open_url(&launch::claude_app_url(&session.id))?;
            Ok(Some(Sent { message: format!("{title} is open in the Claude app and the prompt is on the clipboard. Paste it there to send it."), opened: true }))
        }
        Route::CodexApp => {
            let prompt = attachments::expand(text, Style::Path);
            match codex_start_turn(&account.home_path(), &session.id, &prompt, None) {
                Ok(()) => sent(if busy { format!("Sent to {title} in Codex; it takes it after its current turn.") } else { format!("Sent to {title} in Codex.") }),
                Err(_) => {
                    // The app isn't running, didn't take the thread, or has changed how its
                    // windows talk: its own link fills the reply in instead.
                    launch::open_codex(&format!("{}?prompt={}", launch::codex_thread_url(&session.id), urlencoding::encode(&prompt)))?;
                    Ok(Some(Sent { message: format!("Codex has {title} open with the prompt filled in; send it there."), opened: true }))
                }
            }
        }
        Route::Opencode => {
            let svc = crate::opencode::service(&account.home_path()).ok_or_else(|| anyhow!("OpenCode's service isn't running, so Codealong can't reach {title}. Open it and send it there."))?;
            let body = json!({ "text": attachments::expand(text, Style::Path), "delivery": "queue" });
            crate::opencode::post(&svc, &format!("/api/session/{}/prompt", urlencoding::encode(&session.id)), &body)?;
            sent(if busy { format!("Queued for {title} in OpenCode; it takes it after its current step.") } else { format!("Sent to {title} in OpenCode.") })
        }
        Route::Antigravity => {
            copy(&attachments::expand(text, Style::Path))?;
            launch::open_antigravity()?;
            Ok(Some(Sent { message: "Antigravity is open and the prompt is on the clipboard. Paste it there to send it.".to_string(), opened: true }))
        }
    }
}

fn quoted(title: &str) -> String {
    format!("“{}”", title.trim())
}

fn copy(text: &str) -> Result<()> {
    procs::run_stdin("/usr/bin/pbcopy", &[], text, Duration::from_secs(5)).map(|_| ())
}

// ---- Claude Code ----

/// A live session's registry entry: `<home>/sessions/<pid>.json`.
fn read_registry(home: &Path, pid: u32) -> Option<Value> {
    serde_json::from_str(&std::fs::read_to_string(home.join("sessions").join(format!("{pid}.json"))).ok()?).ok()
}

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

/// The prompt as a cross-session message. The envelope is the one Claude Code writes for its
/// own sends (`<cross-session-message from-name="…">`, the body on lines of its own), so the
/// session shows who it's from. It asserts no permission mode: a session that skips prompts
/// then holds it for you to allow, rather than taking Codealong's word for it.
pub fn peer_content(text: &str) -> String {
    let body = format!("Your user sent this to this session from Codealong:\n\n{}", text.trim()).replace("</cross-session-message", "<\\/cross-session-message");
    format!("<cross-session-message from-name=\"Codealong\">\n{body}\n</cross-session-message>")
}

/// The line written to a session's socket. `session_id` makes a session that has since taken
/// over the pid drop it.
pub fn peer_frame(session_id: &str, msg_id: &str, text: &str) -> String {
    let frame = json!({
        "msgV": 1,
        "msg_id": msg_id,
        "type": "user",
        "session_id": session_id,
        "message": { "role": "user", "content": peer_content(text) },
        "priority": "next",
    });
    format!("{frame}\n")
}

/// Nothing comes back on the connection, so a successful write is all there is to see. Claude
/// Code's own sender waits a moment before closing, so the line is read before the socket goes.
fn peer_send(socket: &str, session_id: &str, text: &str) -> Result<()> {
    let frame = peer_frame(session_id, &start::new_session_id(), text);
    // Claude Code caps a line at a million characters.
    if frame.len() > 1_000_000 {
        bail!("That's too long to send to another session; shorten it or put it in a file.");
    }
    let mut stream = UnixStream::connect(socket).map_err(|e| anyhow!("The session's message socket didn't answer: {e}"))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;
    stream.write_all(frame.as_bytes())?;
    stream.flush()?;
    std::thread::sleep(Duration::from_millis(150));
    let _ = stream.shutdown(std::net::Shutdown::Both);
    Ok(())
}

/// The terminal input that resumes a Claude Code session with a prompt, and the environment
/// that carries the prompt: typed through the shell, as a new session's is (start.rs), so no
/// quoting of yours can break the command.
pub fn resume_input(account: &AccountCfg, session_id: &str, text: &str) -> (String, Vec<(String, String)>) {
    if account.harness == "codex" {
        // `codex resume <id> <prompt>`; its files go as paths, and `--` keeps a prompt that
        // starts with a dash from reading as an option.
        let input = format!("{} resume -- {session_id} \"${}\"\n", account.command(), start::PROMPT_VAR);
        return (input, vec![(start::PROMPT_VAR.to_string(), attachments::expand(text, Style::Path).trim().to_string())]);
    }
    let (has_images, has_files) = attachments::kinds(text);
    let mut input = account.command().to_string();
    let mut env = Vec::new();
    for (has, var, dir) in [(has_images, start::IMAGES_VAR, attachments::images_dir()), (has_files, start::FILES_VAR, attachments::files_dir())] {
        if has {
            input.push_str(&format!(" --add-dir \"${var}\""));
            env.push((var.to_string(), dir.to_string_lossy().to_string()));
        }
    }
    input.push_str(&format!(" --resume {session_id} \"${}\"\n", start::PROMPT_VAR));
    env.push((start::PROMPT_VAR.to_string(), start::as_argument(attachments::expand(text, Style::Mention).trim())));
    (input, env)
}

// ---- Codex app ----

/// A frame on the Codex app's IPC socket: a 4-byte little-endian length, then JSON.
pub fn ipc_frame(message: &Value) -> Vec<u8> {
    let body = message.to_string().into_bytes();
    let mut out = (body.len() as u32).to_le_bytes().to_vec();
    out.extend(body);
    out
}

/// Takes one whole frame off the front of `buf`, if one is there.
pub fn ipc_take(buf: &mut Vec<u8>) -> Option<Value> {
    if buf.len() < 4 {
        return None;
    }
    let n = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;
    if buf.len() < 4 + n {
        return None;
    }
    let frame: Vec<u8> = buf.drain(..4 + n).skip(4).collect();
    Some(serde_json::from_slice(&frame).unwrap_or(Value::Null))
}

/// The request that starts a turn in a thread the app has open, sent to the window that owns it.
/// The app's router drops a request whose `version` isn't the one it expects for the method
/// (2 for this one in 0.155), so a changed app fails here rather than doing something else.
/// A thread with no turns yet needs its model named: the window otherwise sends an empty one,
/// which the API refuses.
pub fn codex_start_request(request_id: &str, client_id: &str, owner: &str, thread: &str, text: &str, message_id: &str, model: Option<(&str, Option<&str>)>) -> Value {
    let mut request = json!({
        "threadId": thread,
        "input": [{ "type": "text", "text": text, "text_elements": [] }],
        "clientUserMessageId": message_id,
    });
    if let Some((m, effort)) = model {
        request["model"] = json!(m);
        if let Some(e) = effort {
            request["effort"] = json!(e);
        }
    }
    json!({
        "type": "request",
        "requestId": request_id,
        "sourceClientId": client_id,
        "version": 2,
        "method": "thread-follower-start-turn",
        "targetClientId": owner,
        "timeoutMs": 10_000,
        "params": {
            "conversationId": thread,
            "turnStart": { "request": request, "context": {} },
        },
    })
}

struct Ipc {
    stream: UnixStream,
    buf: Vec<u8>,
    client_id: String,
}

impl Ipc {
    fn send(&mut self, message: &Value) -> Result<()> {
        self.stream.write_all(&ipc_frame(message))?;
        Ok(())
    }

    /// Waits for the response to `request_id`. The router also asks every client whether it can
    /// handle other clients' requests; Codealong can't, and says so at once, or they'd wait on it.
    fn response(&mut self, request_id: &str, deadline: Instant) -> Result<Value> {
        loop {
            while let Some(m) = ipc_take(&mut self.buf) {
                match m.get("type").and_then(Value::as_str) {
                    Some("response") if m.get("requestId").and_then(Value::as_str) == Some(request_id) => return Ok(m),
                    Some("client-discovery-request") => {
                        let id = m.get("requestId").cloned().unwrap_or(Value::Null);
                        self.send(&json!({ "type": "client-discovery-response", "requestId": id, "response": { "canHandle": false } }))?;
                    }
                    Some("request") => {
                        let id = m.get("requestId").cloned().unwrap_or(Value::Null);
                        self.send(&json!({ "type": "response", "requestId": id, "resultType": "error", "error": "no-handler-for-request" }))?;
                    }
                    _ => {}
                }
            }
            let left = deadline.checked_duration_since(Instant::now()).ok_or_else(|| anyhow!("Codex didn't answer in time."))?;
            self.stream.set_read_timeout(Some(left.max(Duration::from_millis(10))))?;
            let mut chunk = [0u8; 65536];
            let n = self.stream.read(&mut chunk)?;
            if n == 0 {
                bail!("Codex closed the connection.");
            }
            self.buf.extend_from_slice(&chunk[..n]);
        }
    }
}

/// Starts a turn with `text` in a Codex thread, through the socket the app's windows use to
/// reach each other (`<CODEX_HOME>/ipc/ipc.sock`). The window that owns the thread runs the turn
/// in the app's own app-server, so the app shows it as if you'd sent it there. A window owns only
/// the threads it has loaded, and asking about any other goes unanswered until the router gives
/// up ten seconds later; so after a short wait Codealong opens the thread in the app, which loads
/// it and takes it within a couple of seconds, and asks again.
pub fn codex_start_turn(codex_home: &Path, thread: &str, text: &str, model: Option<(&str, Option<&str>)>) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(18);
    let stream = UnixStream::connect(codex_home.join("ipc/ipc.sock"))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;
    let mut ipc = Ipc { stream, buf: Vec::new(), client_id: String::new() };
    let init = start::new_session_id();
    ipc.send(&json!({ "type": "request", "requestId": init, "method": "initialize", "params": { "clientType": "codealong" } }))?;
    let r = ipc.response(&init, deadline)?;
    ipc.client_id = r.pointer("/result/clientId").and_then(Value::as_str).ok_or_else(|| anyhow!("Codex didn't take Codealong as a client."))?.to_string();
    let client = ipc.client_id.clone();

    let mut opened = false;
    let owner = loop {
        let find = start::new_session_id();
        ipc.send(&json!({
            "type": "request", "requestId": find, "sourceClientId": client, "version": 1,
            "method": "thread-owner-discovery", "params": { "hostId": "local", "conversationId": thread },
        }))?;
        let wait = (Instant::now() + Duration::from_millis(1500)).min(deadline);
        if let Ok(r) = ipc.response(&find, wait) {
            if r.get("resultType").and_then(Value::as_str) == Some("success") {
                if let Some(owner) = r.get("handledByClientId").and_then(Value::as_str) {
                    if opened {
                        // A window takes a thread as it starts loading it, and a turn that
                        // arrives before its settings are in goes out with an empty model
                        // (refused) and the app's default reviewer. They're in within about a
                        // second.
                        std::thread::sleep(Duration::from_secs(2));
                    }
                    break owner.to_string();
                }
            }
        }
        if Instant::now() >= deadline {
            bail!("No Codex window took this thread.");
        }
        if !opened {
            launch::open_codex(&launch::codex_thread_url(thread))?;
            opened = true;
        }
        std::thread::sleep(Duration::from_millis(300));
    };

    let turn = start::new_session_id();
    ipc.send(&codex_start_request(&turn, &client, &owner, thread, text, &start::new_session_id(), model))?;
    let r = ipc.response(&turn, deadline)?;
    if r.get("resultType").and_then(Value::as_str) != Some("success") {
        bail!("Codex didn't start the turn: {}", r.get("error").and_then(Value::as_str).unwrap_or("no reason given"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Harness, Session, Status, Surface};

    fn session(harness: Harness, surface: Surface, status: Status, live: bool, pid: Option<u32>) -> Session {
        Session {
            key: "claude:claude:abc".into(),
            id: "abc".into(),
            short_id: "abc".into(),
            harness,
            account: "claude".into(),
            surface,
            status,
            live,
            pid,
            title: "Fix the build".into(),
            cwd: "/tmp".into(),
            work: Vec::new(),
            reply_tail: None,
            first_prompt: None,
            name: None,
            started_at: None,
            status_since: None,
            last_activity: 0,
            last_prompt_at: 0,
            excerpt: None,
            resume_command: None,
            app_account: false,
            open_target: String::new(),
            visitor: None,
            headline: String::new(),
            detail: None,
            branch: None,
            archived: false,
            attach_command: None,
            stalled_since: None,
            background: None,
            insight: None,
            asks: false,
            hosted: false,
        }
    }

    /// The ways `send` would try, with the registry and daemon given rather than read.
    fn route_for(s: &Session, registry: Option<&Value>, home: &Path, daemon: Option<&Path>) -> Result<Vec<Route>> {
        ensure_not_waiting(s)?;
        Ok(if s.harness == Harness::Claude {
            claude_routes(s, registry, home, daemon)
        } else {
            let account: AccountCfg = serde_json::from_value(json!({ "name": "a", "harness": "codex", "home": "/h" })).unwrap();
            crate::harness::adapter(s.harness).routes(&account, s)
        })
    }

    fn registry(extra: Value) -> Value {
        let mut r = json!({ "pid": 42, "sessionId": "abc", "peerProtocol": 1, "messagingSocketPath": "/tmp/cc-socks/42.sock", "kind": "interactive" });
        r.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
        r
    }

    #[test]
    fn never_types_into_a_session_waiting_on_you() {
        let s = session(Harness::Claude, Surface::Cli, Status::Needs, true, Some(42));
        let err = route_for(&s, Some(&registry(json!({}))), Path::new("/h"), None).unwrap_err();
        assert!(err.to_string().contains("waiting on you"), "{err}");
    }

    #[test]
    fn answers_a_question_its_reply_asks_like_any_prompt() {
        let mut s = session(Harness::Claude, Surface::Cli, Status::Needs, true, Some(42));
        s.asks = true;
        let routes = route_for(&s, Some(&registry(json!({ "status": "idle" }))), Path::new("/h"), None).unwrap();
        assert_eq!(routes, vec![Route::Type { pid: 42 }, Route::Peer { socket: "/tmp/cc-socks/42.sock".into(), session_id: "abc".into() }]);
    }

    #[test]
    fn types_into_a_terminal_session_and_falls_back_to_its_socket() {
        let s = session(Harness::Claude, Surface::Cli, Status::Working, true, Some(42));
        let routes = route_for(&s, Some(&registry(json!({}))), Path::new("/h"), None).unwrap();
        assert_eq!(routes, vec![Route::Type { pid: 42 }, Route::Peer { socket: "/tmp/cc-socks/42.sock".into(), session_id: "abc".into() }]);
    }

    #[test]
    fn reaches_claude_app_sessions_only_through_their_socket() {
        let s = session(Harness::Claude, Surface::App, Status::Unread, true, Some(42));
        let routes = route_for(&s, Some(&registry(json!({ "entrypoint": "claude-desktop" }))), Path::new("/h"), None).unwrap();
        assert_eq!(routes, vec![Route::Peer { socket: "/tmp/cc-socks/42.sock".into(), session_id: "abc".into() }]);
    }

    #[test]
    fn hands_background_jobs_to_the_daemon_first() {
        let s = session(Harness::Claude, Surface::Cli, Status::Idle, true, Some(42));
        let reg = registry(json!({ "kind": "bg", "jobId": "6e07c149" }));
        let routes = route_for(&s, Some(&reg), Path::new("/h"), Some(Path::new("/tmp/d/control.sock"))).unwrap();
        assert_eq!(
            routes,
            vec![
                Route::Daemon { socket: "/tmp/d/control.sock".into(), key_file: "/h/daemon/control.key".into(), job: "6e07c149".into() },
                Route::Peer { socket: "/tmp/cc-socks/42.sock".into(), session_id: "abc".into() },
            ]
        );
        // Without the daemon running, the job's socket still takes it.
        let routes = route_for(&s, Some(&reg), Path::new("/h"), None).unwrap();
        assert_eq!(routes, vec![Route::Peer { socket: "/tmp/cc-socks/42.sock".into(), session_id: "abc".into() }]);
    }

    #[test]
    fn skips_a_socket_of_a_protocol_it_doesnt_know() {
        let s = session(Harness::Claude, Surface::App, Status::Idle, true, Some(42));
        let routes = route_for(&s, Some(&registry(json!({ "peerProtocol": 2 }))), Path::new("/h"), None).unwrap();
        assert!(routes.is_empty());
    }

    #[test]
    fn resumes_sessions_that_arent_running() {
        let s = session(Harness::Claude, Surface::Cli, Status::Stopped, false, None);
        assert_eq!(route_for(&s, None, Path::new("/h"), None).unwrap(), vec![Route::Resume]);
        let s = session(Harness::Claude, Surface::App, Status::Idle, false, None);
        assert_eq!(route_for(&s, None, Path::new("/h"), None).unwrap(), vec![Route::ClaudeApp]);
        let s = session(Harness::Codex, Surface::App, Status::Working, true, None);
        assert_eq!(route_for(&s, None, Path::new("/h"), None).unwrap(), vec![Route::CodexApp]);
        let s = session(Harness::Opencode, Surface::Cli, Status::Idle, true, Some(7));
        assert_eq!(route_for(&s, None, Path::new("/h"), None).unwrap(), vec![Route::Opencode]);
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
    fn wraps_the_prompt_as_claude_code_wraps_its_own_messages() {
        let c = peer_content("  run the tests\n");
        assert!(c.starts_with("<cross-session-message from-name=\"Codealong\">\n"), "{c}");
        assert!(c.ends_with("\nrun the tests\n</cross-session-message>"), "{c}");
        assert!(!c.contains("from-mode"));
        // A closing tag in the prompt can't end the envelope early.
        let c = peer_content("see </cross-session-message> here");
        assert_eq!(c.matches("</cross-session-message>").count(), 1);
        let frame = peer_frame("abc", "m1", "hi");
        assert!(frame.ends_with('\n') && frame.matches('\n').count() == 1);
        let v: Value = serde_json::from_str(frame.trim()).unwrap();
        assert_eq!(v["session_id"], "abc");
        assert_eq!(v["type"], "user");
        assert_eq!(v["priority"], "next");
        assert_eq!(v["message"]["role"], "user");
    }

    #[test]
    fn frames_codex_ipc_messages() {
        let m = json!({ "type": "request", "requestId": "1" });
        let mut buf = ipc_frame(&m);
        assert_eq!(u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize, buf.len() - 4);
        buf.extend(ipc_frame(&json!({ "n": 2 })));
        let half = buf.split_off(buf.len() - 3);
        assert_eq!(ipc_take(&mut buf), Some(m));
        assert_eq!(ipc_take(&mut buf), None);
        buf.extend(half);
        assert_eq!(ipc_take(&mut buf), Some(json!({ "n": 2 })));
        assert!(buf.is_empty());
        let r = codex_start_request("r", "me", "owner", "t1", "hello", "u1", None);
        assert_eq!(r["version"], 2);
        assert_eq!(r["targetClientId"], "owner");
        assert_eq!(r["params"]["turnStart"]["request"]["input"][0], json!({ "type": "text", "text": "hello", "text_elements": [] }));
        assert!(r["params"]["turnStart"]["request"].get("model").is_none());
        let r = codex_start_request("r", "me", "owner", "t1", "hello", "u1", Some(("gpt-x", Some("high"))));
        assert_eq!((&r["params"]["turnStart"]["request"]["model"], &r["params"]["turnStart"]["request"]["effort"]), (&json!("gpt-x"), &json!("high")));
    }

    #[test]
    fn resumes_with_the_prompt_in_the_environment() {
        let account = AccountCfg { name: "claude2".into(), harness: "claude".into(), home: "~/.claude2".into(), command: None, start: None, preferred: false, acp: None };
        let (input, env) = resume_input(&account, "abc", "-v please");
        assert_eq!(input, format!("claude2 --resume abc \"${}\"\n", start::PROMPT_VAR));
        assert_eq!(env, vec![(start::PROMPT_VAR.to_string(), " -v please".to_string())]);
        let codex = AccountCfg { name: "codex".into(), harness: "codex".into(), home: "~/.codex".into(), command: Some("codex".into()), start: None, preferred: false, acp: None };
        let (input, env) = resume_input(&codex, "01a0e379", "-v please");
        assert_eq!(input, format!("codex resume -- 01a0e379 \"${}\"\n", start::PROMPT_VAR));
        assert_eq!(env, vec![(start::PROMPT_VAR.to_string(), "-v please".to_string())]);
    }
}
