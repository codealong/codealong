//! Permission requests Claude Code hands the app through the Codealong plugin's
//! `PermissionRequest` hook, for you to answer from Codealong's windows or your other devices.
//!
//! The hook (`codealong session hook permission-request`, cli.rs) sends the request over the
//! app's socket (server.rs) and waits there; the connection's thread holds it here until you
//! answer, then writes the decision back for the hook to print. Claude Code shows its own prompt
//! meanwhile and takes whichever answer comes first (checked in 2.1.284's code), so a request is
//! let go, with no answer, once it's answered or gone elsewhere: the call has a result in the
//! transcript, the session no longer says it waits on a permission prompt, or the hook's process
//! has ended. The second rule also covers the calls Claude Code holds its prompt back for until
//! the hook returns (a background subagent's): their session doesn't say it's waiting, so the
//! hook goes a few seconds later and the prompt shows where the session runs.

use std::io::{Read, Seek, SeekFrom, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use serde_json::{json, Value};

use crate::model::{Harness, Session, Status};
use crate::transcript::Item;

/// A request as the hook gives it, less what Codealong doesn't use.
#[derive(Debug, Clone, PartialEq)]
pub struct Asked {
    /// Codealong's own id for it, which an answer names.
    pub id: String,
    /// The Claude Code session's id; a subagent's requests come under its session's.
    pub session: String,
    /// The subagent that asks, when one does.
    pub agent: Option<String>,
    pub tool: String,
    pub input: Value,
    /// What Claude Code's "Yes, and don't ask again" would add: an Always answer adds these.
    pub suggestions: Vec<Value>,
    pub at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Allow,
    Always,
    Deny,
}

struct Held {
    asked: Asked,
    /// The holding thread: the answer, and where to say whether the hook got it.
    tx: Sender<(Decision, Sender<bool>)>,
}

static HELD: Mutex<Vec<Held>> = Mutex::new(Vec::new());
static NEXT: AtomicU64 = AtomicU64::new(1);

fn held() -> std::sync::MutexGuard<'static, Vec<Held>> {
    HELD.lock().unwrap_or_else(|e| e.into_inner())
}

/// The tools whose prompt a plain allow doesn't answer: Claude Code wants the person's own
/// input for them (`requiresUserInteraction`), so it drops the hook's allow and keeps asking.
/// Codealong leaves them to the prompt where the session runs.
pub fn answers_tool(tool: &str) -> bool {
    !matches!(tool, "AskUserQuestion" | "ExitPlanMode")
}

/// The request in a hook's input, when it's one Codealong can answer.
pub fn parse(hook: &Value, at: i64) -> Option<(Asked, PathBuf)> {
    let s = |k: &str| hook.get(k).and_then(Value::as_str).map(str::trim).filter(|v| !v.is_empty()).map(String::from);
    let tool = s("tool_name")?;
    if !answers_tool(&tool) {
        return None;
    }
    let asked = Asked {
        id: format!("ask-{}", NEXT.fetch_add(1, Ordering::Relaxed)),
        session: s("session_id")?,
        agent: s("agent_id"),
        tool,
        input: hook.get("tool_input").cloned().unwrap_or(Value::Null),
        suggestions: hook.get("permission_suggestions").and_then(Value::as_array).cloned().unwrap_or_default(),
        at,
    };
    let transcript = s("transcript_path").map(|p| call_record(Path::new(&p), asked.agent.as_deref())).unwrap_or_default();
    Some((asked, transcript))
}

/// Where the call is written: the session's transcript, or its subagent's log beside it
/// (`<session>/subagents/agent-<id>.jsonl`).
fn call_record(transcript: &Path, agent: Option<&str>) -> PathBuf {
    match agent {
        Some(a) => transcript.with_extension("").join("subagents").join(format!("agent-{a}.jsonl")),
        None => transcript.to_path_buf(),
    }
}

/// Whether `id` names a request held here, answered or not: a panel still showing one that's
/// gone gets told so rather than having a key pressed for it.
pub fn is_ask_id(id: &str) -> bool {
    id.starts_with("ask-")
}

/// The request a Claude Code session waits on, the oldest first when it has several.
pub fn of_session(session: &str) -> Option<Asked> {
    held().iter().map(|h| &h.asked).filter(|a| a.session == session).min_by_key(|a| a.at).cloned()
}

/// Answers the request `id`. None when no request of that id is held here; an error when the
/// hook was gone before the answer reached it.
pub fn answer(id: &str, decision: Decision) -> Option<Result<Asked>> {
    let (asked, tx) = held().iter().find(|h| h.asked.id == id).map(|h| (h.asked.clone(), h.tx.clone()))?;
    if decision == Decision::Always && asked.suggestions.is_empty() {
        return Some(Err(anyhow!("Claude Code offered no way to allow this from now on: allow it once instead.")));
    }
    let (ack, got) = channel();
    let delivered = tx.send((decision, ack)).is_ok() && got.recv_timeout(Duration::from_secs(5)).unwrap_or(false);
    Some(if delivered { Ok(asked) } else { Err(anyhow!("It stopped waiting before the answer reached it. Look where it runs.")) })
}

/// What the hook prints for Claude Code (`hookSpecificOutput.decision`).
pub fn decision_json(d: Decision, suggestions: &[Value]) -> Value {
    match d {
        Decision::Allow => json!({ "behavior": "allow" }),
        Decision::Always => json!({ "behavior": "allow", "updatedPermissions": suggestions }),
        // As the prompt's own No does: the turn stops, and the session waits for you.
        Decision::Deny => json!({ "behavior": "deny", "message": "Denied in Codealong.", "interrupt": true }),
    }
}

/// Lays a held request over its session as the engine reads it: it needs you, for that call.
/// The time changes with each new request, so a panel showing the session asks again what it
/// waits on.
pub fn overlay(s: &mut Session) {
    if s.harness != Harness::Claude {
        return;
    }
    let Some(a) = of_session(&s.id) else { return };
    let since = if s.status == Status::Needs { s.status_since.unwrap_or(0).max(a.at) } else { a.at };
    s.status = Status::Needs;
    s.status_since = Some(since);
    s.excerpt = Some(crate::claude::tool_request(&a.tool, &a.input));
    s.stalled_since = None;
    s.asks = false;
}

/// Where a held request's session is, to see whether it still asks.
pub struct Watch {
    /// The account's folder, where the session's registry is.
    pub home: PathBuf,
    pub session: Session,
    /// The file the call is written in; empty when the hook didn't say.
    pub record: PathBuf,
}

/// How long a request is taken as asked before the session's own files must say so: the prompt
/// and the transcript line are written as the hook starts, and the files reach the disk soon
/// after.
const GRACE: Duration = Duration::from_secs(4);

/// Holds the request on this connection until it's answered here or elsewhere, then writes the
/// reply line the hook waits for: the decision, or null for none. `changed` tells the engine the
/// board should show it, or stop.
pub fn hold(stream: &mut UnixStream, asked: Asked, watch: Watch, changed: &dyn Fn()) {
    let (tx, rx) = channel();
    let suggestions = asked.suggestions.clone();
    let id = asked.id.clone();
    held().push(Held { asked, tx });
    changed();
    let start = Instant::now();
    let mut checked = Instant::now();
    let mut answer = None;
    loop {
        match rx.recv_timeout(Duration::from_millis(400)) {
            Ok((d, ack)) => {
                let ok = reply(stream, Some(decision_json(d, &suggestions)));
                let _ = ack.send(ok);
                answer = Some(d);
                break;
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        if closed(stream) {
            break;
        }
        if checked.elapsed() >= Duration::from_secs(1) {
            checked = Instant::now();
            if start.elapsed() >= GRACE && !still_asked(&watch, &id) {
                break;
            }
        }
    }
    held().retain(|h| h.asked.id != id);
    if answer.is_none() {
        reply(stream, None);
    }
    changed();
}

/// Whether the session still waits on this request: it says it waits on a permission prompt,
/// and the call has no result yet.
fn still_asked(w: &Watch, id: &str) -> bool {
    if !crate::send::claude_registry(&w.home, &w.session).is_some_and(|r| waits_on_permission(&r)) {
        return false;
    }
    let Some(asked) = held().iter().find(|h| h.asked.id == id).map(|h| h.asked.clone()) else { return false };
    call_open(&w.record, &asked).unwrap_or(true)
}

/// Claude Code's registry says the session has a permission prompt up (`requires_action`).
pub fn waits_on_permission(registry: &Value) -> bool {
    let f = |k: &str| registry.get(k).and_then(Value::as_str);
    f("status") == Some("waiting") && f("waitingFor") == Some("permission prompt")
}

/// Whether a call of the request's tool is among the record's calls with no result yet: once
/// the prompt is answered where the session runs, the call gets its result (a denied one too).
/// The input isn't compared, since the hook can get it a little rewritten. None when the
/// record can't be read.
fn call_open(record: &Path, asked: &Asked) -> Option<bool> {
    if record.as_os_str().is_empty() {
        return None;
    }
    let mut f = std::fs::File::open(record).ok()?;
    let len = f.metadata().ok()?.len();
    let from = len.saturating_sub(TAIL);
    f.seek(SeekFrom::Start(from)).ok()?;
    let mut buf = Vec::new();
    f.take(TAIL).read_to_end(&mut buf).ok()?;
    // The first line of a tail that starts mid-file is cut short.
    let lines = buf.split(|&b| b == b'\n').skip(usize::from(from > 0));
    let items: Vec<Item> = lines.filter_map(|l| serde_json::from_slice::<Value>(l).ok()).flat_map(|v| calls(&v, 0)).collect();
    Some(crate::transcript::open_tools(&items).iter().any(|i| i.text == asked.tool))
}

/// How much of a record's end is read for its open calls, each second a request is held: the
/// call is written just before its prompt shows, and little comes after it while the prompt is
/// up.
const TAIL: u64 = 512 * 1024;

/// Tool calls and their results in a Claude Code record, subagents' lines included: an item per
/// call (`tool`, its name) and per result (`result`), by the call's id.
fn calls(v: &Value, at: i64) -> Vec<Item> {
    let Some(content) = v.get("message").and_then(|m| m.get("content")).and_then(Value::as_array) else { return Vec::new() };
    content
        .iter()
        .filter_map(|p| {
            let (kind, id) = match p.get("type").and_then(Value::as_str) {
                Some("tool_use") => ("tool", p.get("id")),
                Some("tool_result") => ("result", p.get("tool_use_id")),
                _ => return None,
            };
            let name = p.get("name").and_then(Value::as_str).unwrap_or_default();
            Some(Item { id: id.and_then(Value::as_str).map(String::from), ..crate::transcript::item(at, kind, name) })
        })
        .collect()
}

/// The hook's process is gone: its end of the connection is closed.
fn closed(stream: &mut UnixStream) -> bool {
    if stream.set_nonblocking(true).is_err() {
        return true;
    }
    let mut b = [0u8; 1];
    let gone = match stream.read(&mut b) {
        Ok(0) => true,
        Ok(_) => false,
        Err(e) => e.kind() != std::io::ErrorKind::WouldBlock,
    };
    let _ = stream.set_nonblocking(false);
    gone
}

/// The reply line the hook reads: a decision, or null for none. Whether it was written.
pub fn reply(stream: &mut UnixStream, decision: Option<Value>) -> bool {
    let mut line = json!({ "ok": true, "result": decision }).to_string();
    line.push('\n');
    stream.write_all(line.as_bytes()).and_then(|_| stream.flush()).is_ok()
}

/// What the hook prints for a reply's decision: nothing when there's none, so Claude Code's own
/// prompt stays the way to answer.
pub fn hook_output(decision: &Value) -> Option<String> {
    if decision.is_null() {
        return None;
    }
    Some(json!({ "hookSpecificOutput": { "hookEventName": "PermissionRequest", "decision": decision } }).to_string())
}

/// The session a hook's request comes from, once the board shows it, with its account's
/// folder: None for a session Codealong doesn't show (an account config.toml doesn't list).
pub fn session_for(board: &crate::model::Board, cfg: &crate::config::Config, id: &str) -> Option<(Session, PathBuf)> {
    let s = board.lanes.iter().flat_map(|l| &l.sessions).find(|s| s.harness == Harness::Claude && s.id == id)?;
    let account = cfg.accounts.iter().find(|a| a.name == s.account)?;
    Some((s.clone(), account.home_path()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hook(tool: &str) -> Value {
        json!({
            "session_id": "abc",
            "transcript_path": "/p/-r/abc.jsonl",
            "cwd": "/r",
            "permission_mode": "default",
            "hook_event_name": "PermissionRequest",
            "tool_name": tool,
            "tool_input": { "command": "rm -rf build" },
            "permission_suggestions": [{ "type": "addRules", "rules": [{ "toolName": "Bash", "ruleContent": "rm -rf build" }], "behavior": "allow", "destination": "localSettings" }]
        })
    }

    #[test]
    fn reads_a_hooks_request_and_leaves_questions_and_plans_to_the_prompt() {
        let (a, record) = parse(&hook("Bash"), 5).unwrap();
        assert_eq!((a.session.as_str(), a.tool.as_str(), a.agent, a.at), ("abc", "Bash", None, 5));
        assert_eq!(a.suggestions.len(), 1);
        assert_eq!(record, PathBuf::from("/p/-r/abc.jsonl"));
        assert!(parse(&hook("AskUserQuestion"), 5).is_none());
        assert!(parse(&hook("ExitPlanMode"), 5).is_none());
        let mut sub = hook("Bash");
        sub["agent_id"] = json!("a1b2");
        assert_eq!(parse(&sub, 5).unwrap().1, PathBuf::from("/p/-r/abc/subagents/agent-a1b2.jsonl"));
        assert!(parse(&json!({ "tool_name": "Bash" }), 5).is_none(), "no session");
    }

    #[test]
    fn decisions_are_what_claude_code_takes() {
        let s = vec![json!({ "type": "addRules" })];
        assert_eq!(decision_json(Decision::Allow, &s), json!({ "behavior": "allow" }));
        assert_eq!(decision_json(Decision::Always, &s)["updatedPermissions"], json!(s));
        assert_eq!(decision_json(Decision::Deny, &s)["interrupt"], json!(true));
        let out: Value = serde_json::from_str(&hook_output(&json!({ "behavior": "allow" })).unwrap()).unwrap();
        assert_eq!(out["hookSpecificOutput"]["hookEventName"], "PermissionRequest");
        assert_eq!(hook_output(&Value::Null), None);
    }

    #[test]
    fn a_call_is_open_until_its_result_is_written_in_the_session_or_its_subagents_log() {
        let dir = std::env::temp_dir().join(format!("codealong-asks-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let record = dir.join("s.jsonl");
        let call = json!({ "type": "assistant", "isSidechain": true, "message": { "content": [{ "type": "tool_use", "id": "t1", "name": "Bash", "input": {} }] } });
        std::fs::write(&record, format!("{call}\n")).unwrap();
        let (a, _) = parse(&hook("Bash"), 0).unwrap();
        assert_eq!(call_open(&record, &a), Some(true));
        let (edit, _) = parse(&hook("Edit"), 0).unwrap();
        assert_eq!(call_open(&record, &edit), Some(false));
        let result = json!({ "type": "user", "isSidechain": true, "message": { "content": [{ "type": "tool_result", "tool_use_id": "t1" }] } });
        std::fs::write(&record, format!("{call}\n{result}\n")).unwrap();
        assert_eq!(call_open(&record, &a), Some(false));
        assert_eq!(call_open(Path::new(""), &a), None);
        assert_eq!(call_open(&dir.join("missing.jsonl"), &a), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn session(id: &str) -> Session {
        Session {
            key: format!("claude:claude:{id}"),
            id: id.into(),
            short_id: id.into(),
            harness: Harness::Claude,
            account: "claude".into(),
            surface: crate::model::Surface::Cli,
            status: Status::Needs,
            live: true,
            pid: Some(1),
            title: String::new(),
            cwd: "/r".into(),
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
            snoozed_until: None,
        }
    }

    fn watch(record: PathBuf) -> Watch {
        Watch { home: PathBuf::from("/nonexistent"), session: session("abc"), record }
    }

    #[test]
    fn an_answer_goes_back_on_the_hooks_connection_and_the_request_goes() {
        let (mut ours, mut hooks) = UnixStream::pair().unwrap();
        let (a, _) = parse(&hook("Bash"), 1).unwrap();
        let id = a.id.clone();
        let t = std::thread::spawn(move || hold(&mut ours, a, watch(PathBuf::new()), &|| {}));
        while of_session("abc").is_none_or(|x| x.id != id) {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(answer("ask-none", Decision::Allow).is_none());
        let asked = answer(&id, Decision::Always).unwrap().unwrap();
        assert_eq!(asked.tool, "Bash");
        t.join().unwrap();
        let mut line = String::new();
        std::io::BufRead::read_line(&mut std::io::BufReader::new(&mut hooks), &mut line).unwrap();
        let v: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(v["result"]["behavior"], "allow");
        assert_eq!(v["result"]["updatedPermissions"][0]["destination"], "localSettings");
        assert!(held().iter().all(|h| h.asked.id != id));
        assert!(answer(&id, Decision::Allow).is_none(), "answered once");
    }

    #[test]
    fn a_request_goes_unanswered_when_the_hook_ends_or_the_session_stops_asking() {
        // The hook's process ended.
        let (mut ours, hooks) = UnixStream::pair().unwrap();
        let (a, _) = parse(&hook("Bash"), 2).unwrap();
        let id = a.id.clone();
        drop(hooks);
        hold(&mut ours, a, watch(PathBuf::new()), &|| {});
        assert!(answer(&id, Decision::Allow).is_none());
        // The session's registry doesn't say it waits on a prompt (Claude Code holds its prompt
        // back until the hook returns, or it was answered there): let go after the grace.
        let (mut ours, mut hooks) = UnixStream::pair().unwrap();
        let (a, _) = parse(&hook("Bash"), 3).unwrap();
        let start = Instant::now();
        hold(&mut ours, a, watch(PathBuf::new()), &|| {});
        assert!(start.elapsed() >= GRACE && start.elapsed() < GRACE + Duration::from_secs(3));
        let mut line = String::new();
        std::io::BufRead::read_line(&mut std::io::BufReader::new(&mut hooks), &mut line).unwrap();
        assert_eq!(serde_json::from_str::<Value>(&line).unwrap()["result"], Value::Null);
    }

    #[test]
    fn a_held_request_shows_its_session_as_needing_you() {
        let mut s = session("overlay-1");
        s.status = Status::Working;
        let (tx, _rx) = channel();
        let asked = Asked { id: "ask-o1".into(), session: "overlay-1".into(), agent: None, tool: "Bash".into(), input: json!({ "command": "make" }), suggestions: vec![], at: 42 };
        held().push(Held { asked, tx });
        overlay(&mut s);
        held().retain(|h| h.asked.id != "ask-o1");
        assert_eq!((s.status, s.status_since), (Status::Needs, Some(42)));
        assert_eq!(s.excerpt.as_deref(), Some("Bash · `make`"));
    }

    #[test]
    fn the_registry_says_when_a_prompt_is_up() {
        assert!(waits_on_permission(&json!({ "status": "waiting", "waitingFor": "permission prompt" })));
        assert!(!waits_on_permission(&json!({ "status": "waiting", "waitingFor": "input needed" })));
        assert!(!waits_on_permission(&json!({ "status": "busy" })));
    }
}
