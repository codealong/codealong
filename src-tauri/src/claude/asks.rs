//! Permission requests Claude Code hands the app through the Codealong plugin's
//! `PermissionRequest` hook, for you to answer from Codealong's windows or your other devices:
//! a call to allow or deny, or the questions of an `AskUserQuestion`, answered with your picks.
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
//!
//! A request lives only in the app that holds it. When that app quits before it replies (a
//! relaunch), or none is up when the hook asks, the hook asks again once one answers, for as
//! long as the session still waits on it (`ask_until_replied`).

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use serde_json::{json, Value};

use super::conversation::{claude_input, questions_of};
use super::registry::{registry_of, INPUT_NEEDED, PERMISSION_PROMPT};
use super::transcript::tool_request;
use crate::model::{Harness, Session, Status};
use crate::transcript::{clip_input, Item};
use crate::util::{code, one_line, read_last};

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

#[derive(Debug, Clone, PartialEq)]
pub enum Decision {
    Allow,
    Always,
    Deny,
    /// No, with what to do instead: the call is refused and the turn goes on with your words.
    DenySaying(String),
    /// A question's answers: each question's text to a label, several, or words of your own.
    Answers(Value),
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

/// The tools whose prompt Codealong answers. Claude Code wants the person's own input for
/// questions and plans (`requiresUserInteraction`) and drops a plain allow for them; a question
/// takes its answers in the allow's `updatedInput`, but a plan is left to the prompt where the
/// session runs.
pub fn answers_tool(tool: &str) -> bool {
    tool != "ExitPlanMode"
}

/// What Claude Code's own prompt makes a call's result when you say No with words of your own
/// (2.1.289's code, and the transcripts here): the words follow it.
pub const REFUSED_SAYING: &str = "The user doesn't want to proceed with this tool use. The tool use was rejected (eg. if it was a file edit, the new_string was NOT written to the file). To tell you how to proceed, the user said:\n";

/// Claude Code's tool that asks you questions with choices.
pub const QUESTIONS: &str = "AskUserQuestion";

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
    let questions = asked.tool == QUESTIONS;
    match &decision {
        Decision::Always if asked.suggestions.is_empty() => return Some(Err(anyhow!("Claude Code offered no way to allow this from now on: allow it once instead."))),
        Decision::Allow | Decision::Always if questions => return Some(Err(anyhow!("It asks you questions: pick your answers."))),
        Decision::Answers(_) if !questions => return Some(Err(anyhow!("It isn't asking you questions: allow or deny it."))),
        _ => {}
    }
    let (ack, got) = channel();
    let delivered = tx.send((decision, ack)).is_ok() && got.recv_timeout(Duration::from_secs(5)).unwrap_or(false);
    Some(if delivered { Ok(asked) } else { Err(anyhow!("It stopped waiting before the answer reached it. Look where it runs.")) })
}

/// What the hook prints for Claude Code (`hookSpecificOutput.decision`).
pub fn decision_json(d: &Decision, asked: &Asked) -> Value {
    match d {
        Decision::Allow => json!({ "behavior": "allow" }),
        Decision::Always => json!({ "behavior": "allow", "updatedPermissions": asked.suggestions }),
        // As the prompt's own No does: the turn stops, and the session waits for you.
        Decision::Deny => json!({ "behavior": "deny", "message": "Denied in Codealong.", "interrupt": true }),
        // As the prompt's No with words typed into it does: the call's result is Claude Code's
        // own refusal with them (2.1.289's), and without `interrupt` the turn goes on to read it.
        Decision::DenySaying(words) => json!({ "behavior": "deny", "message": format!("{REFUSED_SAYING}{}", words.trim()) }),
        // The questions as asked, with the answers beside them, as the prompt gives them.
        Decision::Answers(answers) => {
            let mut input = asked.input.clone();
            if let Some(o) = input.as_object_mut() {
                o.insert("answers".into(), answers.clone());
            }
            json!({ "behavior": "allow", "updatedInput": input })
        }
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
    s.excerpt = Some(asked_label(&a));
    s.stalled_since = None;
    s.asks = false;
}

/// Where a held request's session is, to see whether it still asks.
pub struct Watch {
    /// The account's folder, where the session's registry is (`sessions/<pid>.json`).
    pub home: PathBuf,
    /// The session's Claude Code process.
    pub pid: u32,
    /// The file the call is written in; empty when the hook didn't say.
    pub record: PathBuf,
    /// How long that file was when the request came: a result for the call written after that
    /// is its answer.
    pub from: u64,
}

impl Watch {
    pub fn new(home: PathBuf, pid: u32, record: PathBuf) -> Watch {
        let from = std::fs::metadata(&record).map(|m| m.len()).unwrap_or(0);
        Watch { home, pid, record, from }
    }
}

/// How long a request is taken as asked before the session's registry must say so: Claude Code
/// writes it as the prompt shows, while the hook starts.
const GRACE: Duration = Duration::from_secs(4);

/// Holds the request on this connection until it's answered here or elsewhere, then writes the
/// reply line the hook waits for: the decision, or null for none. `changed` tells the engine the
/// board should show it, or stop.
pub fn hold(stream: &mut UnixStream, asked: Asked, watch: Watch, changed: &dyn Fn()) {
    let (tx, rx) = channel();
    let id = asked.id.clone();
    held().push(Held { asked: asked.clone(), tx });
    changed();
    let start = Instant::now();
    let mut checked = Instant::now();
    let mut answer = None;
    loop {
        match rx.recv_timeout(Duration::from_millis(400)) {
            Ok((d, ack)) => {
                let ok = reply(stream, Some(decision_json(&d, &asked)));
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
            if start.elapsed() >= GRACE && !still_asked(&watch, &asked) {
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

/// Whether the session still waits on this request: its registry says it waits on a permission
/// prompt, and no result for the call has been written since the request came.
fn still_asked(w: &Watch, asked: &Asked) -> bool {
    let waiting = registry_of(&w.home, w.pid).is_some_and(|r| r.session_id == asked.session && r.waits_on(prompt_of(&asked.tool)));
    waiting && !call_answered(&w.record, asked, w.from)
}

/// The hook's side of a request: `ask` hands it to the app and gives the app's reply (the
/// decision, or null once the app let it go), or None when no app replied, because none is up
/// or it quit first. Then it's asked again every `pause` while the session still waits on it,
/// with the same grace `hold` gives. Claude Code shows its own prompt meanwhile and leaves the
/// hook running when it's answered there, so the hook has to see that for itself.
pub fn ask_until_replied(asked: &Asked, watch: &Watch, pause: Duration, mut ask: impl FnMut() -> Option<Value>) -> Value {
    let start = Instant::now();
    loop {
        if let Some(reply) = ask() {
            return reply;
        }
        std::thread::sleep(pause);
        if start.elapsed() >= GRACE && !still_asked(watch, asked) {
            return Value::Null;
        }
    }
}

/// What Claude Code's registry says a session waits on while the tool's prompt is up: a
/// permission prompt, or for questions, input.
fn prompt_of(tool: &str) -> &'static str {
    if tool == QUESTIONS {
        INPUT_NEEDED
    } else {
        PERMISSION_PROMPT
    }
}

/// Whether a result for the request's call (a call of its tool, with its input) was written at
/// or after `from` in the record: it was answered where the session runs, or denied. Claude Code
/// can write the call itself only then, with its result. Only the end of the record is read.
fn call_answered(record: &Path, asked: &Asked, from: u64) -> bool {
    if record.as_os_str().is_empty() {
        return false;
    }
    // A tail that starts mid-file starts at the first line that's whole.
    let Ok(tail) = read_last(record, TAIL) else { return false };
    let mut calls: Vec<String> = Vec::new();
    let mut results: Vec<(u64, String)> = Vec::new();
    let mut at = tail.start;
    for line in tail.bytes.split(|&b| b == b'\n') {
        let here = at;
        at += line.len() as u64 + 1;
        let Ok(v) = serde_json::from_slice::<Value>(line) else { continue };
        let Some(content) = v.get("message").and_then(|m| m.get("content")).and_then(Value::as_array) else { continue };
        for p in content {
            let id = |k: &str| p.get(k).and_then(Value::as_str).map(String::from);
            match p.get("type").and_then(Value::as_str) {
                Some("tool_use") if p.get("name").and_then(Value::as_str) == Some(asked.tool.as_str()) && p.get("input") == Some(&asked.input) => calls.extend(id("id")),
                Some("tool_result") => results.extend(id("tool_use_id").map(|i| (here, i))),
                _ => {}
            }
        }
    }
    results.iter().any(|(at, id)| *at >= from && calls.contains(id))
}

/// How much of a record's end is read for the call's result, each second a request is held.
const TAIL: u64 = 512 * 1024;

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

/// The Claude Code process a hook runs under: the first of its parents with a registry entry for
/// the session in the account's folder. The hook is its child, by way of a shell.
pub fn session_process(home: &Path, session: &str) -> Option<u32> {
    let mut pid = std::process::id();
    for _ in 0..6 {
        pid = crate::procs::parent_pid(pid).filter(|&p| p > 1)?;
        if registry_of(home, pid).is_some_and(|r| r.session_id == session) {
            return Some(pid);
        }
    }
    None
}

/// The account a hook's session is on, by its folder (`CLAUDE_CONFIG_DIR`, else `~/.claude`):
/// None for one config.toml doesn't list, whose sessions Codealong doesn't show.
pub fn account_for<'a>(cfg: &'a crate::config::Config, home: &Path) -> Option<&'a crate::config::AccountCfg> {
    let same = |a: &Path| a == home || std::fs::canonicalize(a).ok().zip(std::fs::canonicalize(home).ok()).is_some_and(|(x, y)| x == y);
    cfg.accounts.iter().find(|a| a.harness == "claude" && same(&a.home_path()))
}

/// A permission request the plugin's hook holds in the app, as the panel shows it: a call to
/// allow, or questions to answer.
pub(super) fn asked_pending(a: &Asked) -> crate::harness::Pending {
    let questions = if a.tool == QUESTIONS { questions_of(&a.input) } else { Vec::new() };
    crate::harness::Pending {
        id: a.id.clone(),
        label: asked_label(a),
        input: questions.is_empty().then(|| clip_input(&claude_input(&a.tool, &a.input))),
        cannot: None,
        always: questions.is_empty() && !a.suggestions.is_empty(),
        no_once: false,
        plan: false,
        always_rule: always_text(&a.suggestions),
        questions,
        unwritten: false,
    }
}

/// Whether a held request's call is in its session's conversation yet, among `items` read from
/// the end of its record. Claude Code holds back the message a question comes in until it's
/// answered, and with it whatever the agent wrote before the call. A subagent's call goes in its
/// own record, and nothing of the session's own is held for it.
pub fn written_in(items: &[Item], a: &Asked) -> bool {
    if a.agent.is_some() {
        return true;
    }
    let call = tool_request(&a.tool, &a.input);
    crate::transcript::open_tools(items).iter().any(|i| i.text == call)
}

/// A held request as a line: the call, or what it asks you.
pub fn asked_label(a: &Asked) -> String {
    match questions_of(&a.input).first().filter(|_| a.tool == QUESTIONS) {
        Some(q) => format!("Asks you · {}", one_line(&q.question, 160)),
        None => tool_request(&a.tool, &a.input),
    }
}

/// Your answers to the questions, as Claude Code takes them: each question answered by its text,
/// with a label or words of your own, or a list of them where it takes several. Empty answers
/// and questions it didn't ask are left out; none at all is refused.
pub(super) fn picked(questions: &[crate::harness::Question], answers: &Value) -> anyhow::Result<serde_json::Map<String, Value>> {
    let mut out = serde_json::Map::new();
    for q in questions {
        let words = |v: &Value| v.as_str().map(str::trim).filter(|t| !t.is_empty()).map(|t| Value::String(t.to_string()));
        let answer = match answers.get(&q.question) {
            Some(Value::Array(list)) if q.multi_select => Some(list.iter().filter_map(words).collect::<Vec<_>>()).filter(|l| !l.is_empty()).map(Value::Array),
            Some(Value::Array(list)) => list.iter().find_map(words),
            Some(v) => words(v),
            None => None,
        };
        if let Some(a) = answer {
            out.insert(q.question.clone(), a);
        }
    }
    if out.is_empty() {
        anyhow::bail!("Pick an answer first.");
    }
    Ok(out)
}

/// What answering Always adds, in a sentence: Claude Code's own suggestions for the call, the
/// ones its "Yes, and don't ask again" applies (permission updates: rules, a mode, folders).
pub fn always_text(suggestions: &[Value]) -> Option<String> {
    let parts: Vec<String> = suggestions
        .iter()
        .filter_map(|u| {
            let s = |k: &str| u.get(k).and_then(Value::as_str);
            let place = match s("destination") {
                Some("session") => " for this session",
                Some("localSettings") => " in this project, for you",
                Some("projectSettings") => " in this project, for everyone",
                Some("userSettings") => " everywhere, for you",
                _ => "",
            };
            let list = |k: &str, f: &dyn Fn(&Value) -> Option<String>| -> Vec<String> { u.get(k).and_then(Value::as_array).map(|a| a.iter().filter_map(f).collect()).unwrap_or_default() };
            match s("type")? {
                "addRules" if s("behavior") == Some("allow") => {
                    let rules = list("rules", &|r| {
                        let tool = r.get("toolName")?.as_str()?;
                        Some(code(&match r.get("ruleContent").and_then(Value::as_str) {
                            Some(c) => format!("{tool}({c})"),
                            None => tool.to_string(),
                        }))
                    });
                    (!rules.is_empty()).then(|| format!("Allows {}{place}", rules.join(", ")))
                }
                "addDirectories" => {
                    let dirs = list("directories", &|d| d.as_str().map(|p| code(&crate::util::tilde(Path::new(p)))));
                    (!dirs.is_empty()).then(|| format!("Allows working in {}{place}", dirs.join(", ")))
                }
                "setMode" => {
                    let mode = match s("mode")? {
                        "acceptEdits" => "accepting edits",
                        "default" => "asking first",
                        "plan" => "plan mode",
                        "dontAsk" => "not asking",
                        "auto" => "auto mode",
                        "bypassPermissions" => "skipping permission prompts",
                        _ => return None,
                    };
                    Some(format!("Switches to {mode}{place}"))
                }
                _ => None,
            }
        })
        .collect();
    (!parts.is_empty()).then(|| format!("{}.", parts.join("; ")))
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
        assert!(parse(&hook("AskUserQuestion"), 5).is_some());
        assert!(parse(&hook("ExitPlanMode"), 5).is_none());
        let mut sub = hook("Bash");
        sub["agent_id"] = json!("a1b2");
        assert_eq!(parse(&sub, 5).unwrap().1, PathBuf::from("/p/-r/abc/subagents/agent-a1b2.jsonl"));
        assert!(parse(&json!({ "tool_name": "Bash" }), 5).is_none(), "no session");
    }

    #[test]
    fn decisions_are_what_claude_code_takes() {
        let (a, _) = parse(&hook("Bash"), 0).unwrap();
        assert_eq!(decision_json(&Decision::Allow, &a), json!({ "behavior": "allow" }));
        assert_eq!(decision_json(&Decision::Always, &a)["updatedPermissions"], json!(a.suggestions));
        assert_eq!(decision_json(&Decision::Deny, &a)["interrupt"], json!(true));
        // Words with a No: the turn isn't interrupted, and they come after Claude Code's own.
        let d = decision_json(&Decision::DenySaying(" Explain it first\n".into()), &a);
        assert_eq!(d, json!({ "behavior": "deny", "message": format!("{REFUSED_SAYING}Explain it first") }));
        // What 2.1.284 took for a question: its input with the answers beside the questions.
        let mut q = hook("AskUserQuestion");
        q["tool_input"] = json!({ "questions": [{ "question": "Which color?", "options": [{ "label": "Red" }, { "label": "Blue" }] }] });
        let (q, _) = parse(&q, 0).unwrap();
        let d = decision_json(&Decision::Answers(json!({ "Which color?": "Blue" })), &q);
        assert_eq!(d["behavior"], "allow");
        assert_eq!(d["updatedInput"]["questions"][0]["question"], "Which color?");
        assert_eq!(d["updatedInput"]["answers"], json!({ "Which color?": "Blue" }));
        let out: Value = serde_json::from_str(&hook_output(&json!({ "behavior": "allow" })).unwrap()).unwrap();
        assert_eq!(out["hookSpecificOutput"]["hookEventName"], "PermissionRequest");
        assert_eq!(hook_output(&Value::Null), None);
    }

    #[test]
    fn a_held_question_is_unwritten_until_its_call_is_in_the_record() {
        let mut q = hook("AskUserQuestion");
        q["tool_input"] = json!({ "questions": [{ "question": "Which color?", "options": [{ "label": "Red" }, { "label": "Blue" }] }] });
        let (a, _) = parse(&q, 0).unwrap();
        let items = |lines: &[Value]| lines.iter().flat_map(|v| super::super::conversation::conversation_items(v, 0)).collect::<Vec<_>>();
        let said = json!({ "type": "assistant", "message": { "content": [{ "type": "text", "text": "Here's what those files are." }] } });
        let call = |id: &str, input: &Value| json!({ "type": "assistant", "message": { "content": [{ "type": "tool_use", "id": id, "name": QUESTIONS, "input": input }] } });
        let result = |id: &str| json!({ "type": "user", "message": { "content": [{ "type": "tool_result", "tool_use_id": id, "content": "Answered" }] } });
        let earlier = call("t0", &json!({ "questions": [{ "question": "Which size?", "options": [{ "label": "S" }] }] }));
        // Claude Code writes the message a question comes in once it's answered: until then the
        // record ends before it, here at an earlier question's answer.
        assert!(!written_in(&items(&[earlier.clone(), result("t0")]), &a));
        assert!(written_in(&items(&[earlier.clone(), result("t0"), said, call("t1", &q["tool_input"])]), &a));
        // The same question asked before and answered isn't this one.
        assert!(!written_in(&items(&[call("t1", &q["tool_input"]), result("t1")]), &a));
        // A subagent's call goes in its own record, and nothing of the session's is held for it.
        q["agent_id"] = json!("a1b2");
        assert!(written_in(&[], &parse(&q, 0).unwrap().0));
    }

    #[test]
    fn a_call_is_answered_once_its_result_is_written_after_the_request_came() {
        let dir = crate::fixtures::scratch("asks");
        let record = dir.join("s.jsonl");
        let (a, _) = parse(&hook("Bash"), 0).unwrap();
        let line = |v: Value| format!("{v}\n");
        let call = |id: &str, cmd: &str| line(json!({ "type": "assistant", "isSidechain": true, "message": { "content": [{ "type": "tool_use", "id": id, "name": "Bash", "input": { "command": cmd } }] } }));
        let result = |id: &str| line(json!({ "type": "user", "message": { "content": [{ "type": "tool_result", "tool_use_id": id }] } }));
        // The same command ran and was answered before: that's not this request's answer.
        let before = call("t0", "rm -rf build") + &result("t0");
        std::fs::write(&record, &before).unwrap();
        let from = before.len() as u64;
        assert!(!call_answered(&record, &a, from));
        // Claude Code may write the call only with its answer.
        std::fs::write(&record, before.clone() + &call("t1", "rm -rf build")).unwrap();
        assert!(!call_answered(&record, &a, from));
        std::fs::write(&record, before.clone() + &call("t1", "rm -rf build") + &result("t1")).unwrap();
        assert!(call_answered(&record, &a, from));
        // Another call's result isn't.
        std::fs::write(&record, before.clone() + &call("t2", "make") + &result("t2")).unwrap();
        assert!(!call_answered(&record, &a, from));
        assert!(!call_answered(Path::new(""), &a, 0));
        assert!(!call_answered(&dir.join("missing.jsonl"), &a, 0));
    }

    #[test]
    fn finds_the_account_by_its_folder() {
        let acct = |name: &str, harness: &str, home: &str| crate::config::AccountCfg { name: name.into(), harness: harness.into(), home: home.into(), command: None, start: None, preferred: false, acp: None };
        let cfg = crate::config::Config { accounts: vec![acct("codex", "codex", "/h/.codex"), acct("claude2", "claude", "/h/.claude2")], ..Default::default() };
        assert_eq!(account_for(&cfg, Path::new("/h/.claude2")).map(|a| a.name.as_str()), Some("claude2"));
        assert!(account_for(&cfg, Path::new("/h/.codex")).is_none(), "not a Claude Code account");
        assert!(account_for(&cfg, Path::new("/h/.claude")).is_none());
    }

    fn session(id: &str) -> Session {
        Session { status: Status::Needs, live: true, pid: Some(1), cwd: "/r".into(), ..Session::new(Harness::Claude, "claude", id) }
    }

    fn watch(record: PathBuf) -> Watch {
        Watch::new(PathBuf::from("/nonexistent"), 1, record)
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
    fn a_hook_asks_again_while_the_prompt_is_up_until_an_app_replies() {
        let home = crate::fixtures::scratch("asks-again");
        std::fs::create_dir_all(home.join("sessions")).unwrap();
        let registry = |says: Value| {
            let mut v = json!({ "pid": 4242, "sessionId": "abc", "cwd": "/r" });
            v.as_object_mut().unwrap().extend(says.as_object().unwrap().clone());
            std::fs::write(home.join("sessions/4242.json"), v.to_string()).unwrap();
        };
        let record = home.join("abc.jsonl");
        std::fs::write(&record, "").unwrap();
        let (a, _) = parse(&hook("Bash"), 1).unwrap();
        let w = Watch::new(home.clone(), 4242, record.clone());
        registry(json!({ "status": "waiting", "waitingFor": "permission prompt" }));
        // No app up, then one that quit before it replied, then one that allows it.
        let mut tries = 0;
        let got = ask_until_replied(&a, &w, Duration::from_millis(10), || {
            tries += 1;
            (tries == 3).then(|| json!({ "behavior": "allow" }))
        });
        assert_eq!((tries, got["behavior"].as_str()), (3, Some("allow")));
        // The app's null is its last word: it let the request go.
        let mut tries = 0;
        assert_eq!(ask_until_replied(&a, &w, Duration::from_millis(10), || { tries += 1; Some(Value::Null) }), Value::Null);
        assert_eq!(tries, 1);
        // Answered where it runs while no app was up: its result is written, and the hook goes
        // after the grace without an answer, as the registry still says it waits (on another).
        let result = json!({ "type": "user", "message": { "content": [{ "type": "tool_result", "tool_use_id": "t1" }] } });
        let call = json!({ "type": "assistant", "message": { "content": [{ "type": "tool_use", "id": "t1", "name": "Bash", "input": { "command": "rm -rf build" } }] } });
        std::fs::write(&record, format!("{call}\n{result}\n")).unwrap();
        let start = Instant::now();
        assert_eq!(ask_until_replied(&a, &w, Duration::from_millis(50), || None), Value::Null);
        assert!(start.elapsed() >= GRACE && start.elapsed() < GRACE + Duration::from_secs(2));
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
        let entry = |says: Value| {
            let mut v = json!({ "pid": 1, "sessionId": "abc", "cwd": "/r" });
            v.as_object_mut().unwrap().extend(says.as_object().unwrap().clone());
            crate::claude::registry::parse_registry(&v.to_string()).unwrap()
        };
        assert!(entry(json!({ "status": "waiting", "waitingFor": "permission prompt" })).waits_on(prompt_of("Bash")));
        assert!(!entry(json!({ "status": "waiting", "waitingFor": "input needed" })).waits_on(prompt_of("Bash")));
        assert!(entry(json!({ "status": "waiting", "waitingFor": "input needed" })).waits_on(prompt_of(QUESTIONS)));
        assert!(!entry(json!({ "status": "busy" })).waits_on(prompt_of("Bash")));
        // A field in a shape Codealong doesn't know is as good as missing, and the entry still reads.
        assert!(!entry(json!({ "status": "waiting", "waitingFor": { "kind": "permission prompt" }, "peerProtocol": "1" })).waits_on(prompt_of("Bash")));
    }

    #[test]
    fn a_question_takes_answers_and_a_call_takes_allow_or_deny() {
        let (tx, _rx) = channel();
        let mut q = parse(&hook("AskUserQuestion"), 0).unwrap().0;
        q.id = "ask-q1".into();
        let mut c = parse(&hook("Bash"), 0).unwrap().0;
        c.id = "ask-c1".into();
        c.suggestions.clear();
        held().push(Held { asked: q, tx: tx.clone() });
        held().push(Held { asked: c, tx });
        let refused = |id: &str, d: Decision| answer(id, d).unwrap().unwrap_err().to_string();
        assert!(refused("ask-q1", Decision::Allow).contains("pick your answers"));
        assert!(refused("ask-c1", Decision::Answers(json!({}))).contains("allow or deny"));
        assert!(refused("ask-c1", Decision::Always).contains("allow it once"));
        held().retain(|h| h.asked.id != "ask-q1" && h.asked.id != "ask-c1");
    }

    #[test]
    fn says_what_always_adds_from_claude_codes_own_suggestions() {
        // What 2.1.284 suggested for `touch` in the session's folder.
        let s = serde_json::json!([
            { "type": "addDirectories", "directories": ["/Users/x/work"], "destination": "session" },
            { "type": "setMode", "mode": "acceptEdits", "destination": "session" }
        ]);
        let s = s.as_array().unwrap();
        assert_eq!(always_text(s).as_deref(), Some("Allows working in `/Users/x/work` for this session; Switches to accepting edits for this session."));
        let rules = serde_json::json!([{ "type": "addRules", "rules": [{ "toolName": "Bash", "ruleContent": "npm test:*" }, { "toolName": "WebFetch" }], "behavior": "allow", "destination": "localSettings" }]);
        assert_eq!(always_text(rules.as_array().unwrap()).as_deref(), Some("Allows `Bash(npm test:*)`, `WebFetch` in this project, for you."));
        assert_eq!(always_text(&[]), None);
        assert_eq!(always_text(&[serde_json::json!({ "type": "removeRules" })]), None);
    }

    #[test]
    fn a_question_is_answered_with_its_choices_or_your_own_words() {
        let input = serde_json::json!({ "questions": [
            { "question": "Which color do you prefer?", "header": "Color", "options": [{ "label": "Red", "description": "The color red" }, { "label": "Blue" }], "multiSelect": false },
            { "question": "Which days?", "options": [{ "label": "Mon" }, { "label": "Tue" }], "multiSelect": true }
        ] });
        let qs = questions_of(&input);
        assert_eq!(qs.len(), 2);
        assert_eq!((qs[0].header.as_deref(), qs[0].options[0].description.as_deref(), qs[1].multi_select), (Some("Color"), Some("The color red"), true));
        let a = Asked { id: "ask-q".into(), session: "abc".into(), agent: None, tool: "AskUserQuestion".into(), input: input.clone(), suggestions: vec![], at: 1 };
        let p = asked_pending(&a);
        assert_eq!((p.label.as_str(), p.input, p.always, p.questions.len()), ("Asks you · Which color do you prefer?", None, false, 2));
        let got = picked(&qs, &serde_json::json!({ "Which color do you prefer?": " Green, please ", "Which days?": ["Mon", "", "Tue"], "Not asked": "x" })).unwrap();
        assert_eq!(Value::Object(got), serde_json::json!({ "Which color do you prefer?": "Green, please", "Which days?": ["Mon", "Tue"] }));
        let one = picked(&qs, &serde_json::json!({ "Which color do you prefer?": ["Blue", "Red"] })).unwrap();
        assert_eq!(one["Which color do you prefer?"], "Blue", "one pick for a question that takes one");
        assert!(picked(&qs, &serde_json::json!({ "Which days?": [] })).is_err());
    }

    #[test]
    fn a_held_request_is_the_call_the_panel_answers() {
        let a = Asked { id: "ask-9".into(), session: "abc".into(), agent: None, tool: "Bash".into(), input: serde_json::json!({ "command": "touch alpha.txt" }), suggestions: vec![serde_json::json!({ "type": "setMode", "mode": "acceptEdits", "destination": "session" })], at: 1 };
        let p = asked_pending(&a);
        assert_eq!((p.id.as_str(), p.label.as_str(), p.input.as_deref(), p.cannot, p.always), ("ask-9", "Bash · `touch alpha.txt`", Some("touch alpha.txt"), None, true));
        assert_eq!(p.always_rule.as_deref(), Some("Switches to accepting edits for this session."));
    }
}
