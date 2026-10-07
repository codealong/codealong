//! Side conversations: an ephemeral fork of the thread on an app-server of Codealong's own
//! (`Adapter::side`), or an ephemeral thread of its own about a PR (`Adapter::fresh_side`), and the
//! lines that make it and ask it.

use std::path::Path;

use serde_json::Value;

use crate::util::{code, one_line, scan_lines_back};

/// How a thread's latest turn ran, as its `turn_context` line has it: a side conversation's
/// copy runs the same way.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TurnRan {
    pub model: Option<String>,
    /// `read-only`, `workspace-write` or `danger-full-access` (`sandbox_policy.type`).
    pub sandbox: Option<String>,
    /// `untrusted`, `on-request` or `never`.
    pub approval: Option<String>,
}

/// A thread's latest `turn_context`. Read back from the end, since a turn's output can run to
/// megabytes.
pub(super) fn last_turn(rollout: &Path) -> TurnRan {
    let mut ran = TurnRan::default();
    let Ok(len) = std::fs::metadata(rollout).map(|m| m.len()) else { return ran };
    scan_lines_back(rollout, 0, len, |line| {
        if !line.contains("\"turn_context\"") {
            return false;
        }
        let Ok(v) = serde_json::from_str::<Value>(line) else { return false };
        if v.get("type").and_then(Value::as_str) != Some("turn_context") {
            return false;
        }
        let text = |at: &str| v.pointer(at).and_then(Value::as_str).filter(|t| !t.is_empty()).map(String::from);
        ran = TurnRan {
            model: text("/payload/model"),
            sandbox: text("/payload/sandbox_policy/type").filter(|t| ["read-only", "workspace-write", "danger-full-access"].contains(&t.as_str())),
            approval: text("/payload/approval_policy").filter(|t| ["untrusted", "on-request", "never"].contains(&t.as_str())),
        };
        true
    });
    ran
}

/// How Codealong talks to a side conversation's copy of a thread (side.rs, `Adapter::side`): an
/// app-server of its own, JSON-RPC a line a message, doing what Codex's `/side` does (its
/// tui/src/app/side.rs; checked on 0.157.1, 2026-09-30). `thread/fork` with `ephemeral` copies
/// the thread without a rollout of its own (it needs `excludeTurns`, and a parent that has a
/// rollout), `thread/inject_items` puts the side conversation's rules after the copied history,
/// and each question is a `turn/start`. The answer comes as `item/agentMessage/delta`
/// notifications, each tool as an `item/started`, and `turn/completed` ends it. The copy runs
/// as the thread last did (`TurnRan`: its model, sandbox and approval policy), so it may do what
/// the thread may; an approval that comes to Codealong, with nobody to ask, is declined. A
/// thread that says nothing of how it ran is copied read-only, asking nothing.
///
/// A conversation about a PR is made the same way from nothing: `thread/start` with `ephemeral`
/// (its params also take `developerInstructions`, 0.160's schema), read-only and asking nothing,
/// its rules injected as a copy's are.
///
/// It numbers its own requests and reads their answers itself, rather than asking through the
/// client that carries its lines (rpc.rs): a `Dialect` only turns lines into lines, which is
/// what lets the test below run it without a process. The handshake is appserver.rs's.
pub struct SideThread {
    /// The thread it copies; none for a thread of its own.
    thread: Option<String>,
    /// What it's told before its first question.
    rules: String,
    cwd: String,
    ran: TurnRan,
    /// The copy's thread, once Codex has made it.
    copy: Option<String>,
    /// The copy has its rules and takes questions.
    ready: bool,
    /// The first question, asked while the copy was being made.
    waiting: Option<String>,
    next: i64,
    /// Some of this answer is written: its next message starts a new paragraph.
    said: bool,
}

impl SideThread {
    /// The requests that make the copy, by the ids their answers come back with.
    const INITIALIZE: i64 = 1;
    const FORK: i64 = 2;
    const INJECT: i64 = 3;

    pub fn new(thread: &str, cwd: &Path, ran: TurnRan) -> SideThread {
        SideThread { thread: Some(thread.to_string()), rules: crate::side::RULES.to_string(), cwd: cwd.to_string_lossy().into_owned(), ran, copy: None, ready: false, waiting: None, next: Self::INJECT, said: false }
    }

    /// A thread of its own in `cwd`, told `rules`, that may only look (`Adapter::fresh_side`): on the
    /// account's usual model, read-only, asking nothing.
    pub fn fresh(cwd: &Path, rules: &str) -> SideThread {
        let ran = TurnRan { model: None, sandbox: Some("read-only".into()), approval: Some("never".into()) };
        SideThread { thread: None, rules: rules.to_string(), ..SideThread::new("", cwd, ran) }
    }

    fn turn(&mut self, text: &str) -> Value {
        self.next += 1;
        self.said = false;
        serde_json::json!({ "id": self.next, "method": "turn/start", "params": { "threadId": self.copy, "input": [{ "type": "text", "text": text }] } })
    }
}

impl crate::side::Dialect for SideThread {
    fn open(&mut self) -> Vec<Value> {
        vec![crate::rpc::request_line(Self::INITIALIZE, super::appserver::INITIALIZE, super::appserver::initialize_params())]
    }

    fn ask(&mut self, text: &str) -> Vec<Value> {
        if !self.ready {
            self.waiting = Some(text.to_string());
            return Vec::new();
        }
        vec![self.turn(text)]
    }

    fn hear(&mut self, line: &Value) -> (Vec<crate::side::Event>, Vec<Value>) {
        use crate::side::Event;
        use serde_json::json;
        let nothing = (Vec::new(), Vec::new());
        let id = line.get("id");
        let method = line.get("method").and_then(Value::as_str);
        match (id, method) {
            // Something Codex asks of its client. A copy has nobody to ask, and no tools of ours.
            (Some(id), Some(method)) => {
                let reply = if method.ends_with("requestApproval") { json!({ "id": id, "result": { "decision": "decline" } }) } else { json!({ "id": id, "error": { "code": -32601, "message": "A side conversation doesn't take this." } }) };
                (Vec::new(), vec![reply])
            }
            // An answer to one of ours.
            (Some(id), None) => {
                if let Some(e) = line.get("error") {
                    let why = e.get("message").and_then(Value::as_str).unwrap_or("it wouldn't say why");
                    return (vec![Event::Done(Some(format!("Codex: {why}")))], Vec::new());
                }
                match id.as_i64() {
                    Some(Self::INITIALIZE) => {
                        let sandbox = self.ran.sandbox.as_deref().unwrap_or("read-only");
                        let approval = self.ran.approval.as_deref().filter(|_| self.ran.sandbox.is_some()).unwrap_or("never");
                        let mut params = json!({ "ephemeral": true, "cwd": self.cwd, "approvalPolicy": approval, "sandbox": sandbox, "developerInstructions": self.rules });
                        if let Some(m) = &self.ran.model {
                            params["model"] = json!(m);
                        }
                        let method = match &self.thread {
                            Some(thread) => {
                                params["threadId"] = json!(thread);
                                params["excludeTurns"] = json!(true);
                                "thread/fork"
                            }
                            None => "thread/start",
                        };
                        (Vec::new(), vec![crate::rpc::note_line(super::appserver::INITIALIZED, Value::Null), json!({ "id": Self::FORK, "method": method, "params": params })])
                    }
                    Some(Self::FORK) => {
                        self.copy = line.pointer("/result/thread/id").and_then(Value::as_str).map(String::from);
                        let rules = json!({ "type": "message", "role": "user", "content": [{ "type": "input_text", "text": self.rules }] });
                        (Vec::new(), vec![json!({ "id": Self::INJECT, "method": "thread/inject_items", "params": { "threadId": self.copy, "items": [rules] } })])
                    }
                    Some(Self::INJECT) => {
                        self.ready = true;
                        let first = self.waiting.take().map(|q| self.turn(&q));
                        (Vec::new(), first.into_iter().collect())
                    }
                    _ => nothing,
                }
            }
            (None, Some(method)) => {
                let p = line.get("params").unwrap_or(&Value::Null);
                // The app-server tells of the thread it copied too.
                if p.get("threadId").and_then(Value::as_str).is_some_and(|t| Some(t) != self.copy.as_deref()) {
                    return nothing;
                }
                let item = p.get("item").unwrap_or(&Value::Null);
                let text = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).map(String::from);
                let events = match (method, item.get("type").and_then(Value::as_str)) {
                    ("item/agentMessage/delta", _) => {
                        self.said = true;
                        text(p, "delta").map(Event::Said).into_iter().collect()
                    }
                    ("item/started", Some("agentMessage")) if self.said => vec![Event::Said("\n\n".into())],
                    ("item/started", Some("commandExecution")) => vec![Event::Tool(code(&one_line(&text(item, "command").unwrap_or_default(), 100)))],
                    ("item/started", Some("mcpToolCall")) => vec![Event::Tool([text(item, "server"), text(item, "tool")].into_iter().flatten().collect::<Vec<_>>().join(" · "))],
                    ("item/started", Some("webSearch")) => vec![Event::Tool("Searching the web".into())],
                    ("turn/completed", _) => {
                        let why = match p.pointer("/turn/status").and_then(Value::as_str) {
                            Some("failed") => Some(p.pointer("/turn/error/message").and_then(Value::as_str).map(|m| format!("Codex: {m}")).unwrap_or_else(|| "Codex couldn't answer.".into())),
                            Some("interrupted") => Some("It was interrupted.".into()),
                            _ => None,
                        };
                        vec![Event::Done(why)]
                    }
                    _ => Vec::new(),
                };
                (events, Vec::new())
            }
            (None, None) => nothing,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The handshake is appserver.rs's, and reads on the wire as the copy always wrote it.
    #[test]
    fn a_side_thread_opens_with_the_app_servers_handshake() {
        use crate::side::Dialect;
        use serde_json::json;
        let mut t = SideThread::new("parent", Path::new("/w"), TurnRan::default());
        assert_eq!(t.open(), [json!({ "id": 1, "method": "initialize", "params": { "clientInfo": { "name": "codealong", "title": "Codealong", "version": env!("CARGO_PKG_VERSION") } } })]);
        assert_eq!(t.hear(&json!({"id": 1, "result": {}})).1[0].to_string(), r#"{"method":"initialized"}"#);
    }

    #[test]
    fn a_side_thread_is_copied_then_asked() {
        use crate::side::{Dialect, Event};
        use serde_json::json;
        let ran = TurnRan { model: Some("gpt-x".into()), sandbox: Some("workspace-write".into()), approval: Some("on-request".into()) };
        let mut t = SideThread::new("parent", Path::new("/w"), ran);
        assert_eq!(t.open()[0]["method"], "initialize");
        // Asked before the copy is made, the question waits.
        assert!(t.ask("Why?").is_empty());
        let out = t.hear(&json!({"id": 1, "result": {}})).1;
        assert_eq!(out[0], json!({"method": "initialized"}));
        assert_eq!(out[1]["method"], "thread/fork");
        let p = &out[1]["params"];
        // It runs as the thread last did, so it may do what the thread may.
        assert_eq!((&p["threadId"], &p["ephemeral"], &p["excludeTurns"], &p["sandbox"], &p["approvalPolicy"], &p["model"], &p["cwd"]), (&json!("parent"), &json!(true), &json!(true), &json!("workspace-write"), &json!("on-request"), &json!("gpt-x"), &json!("/w")));
        let out = t.hear(&json!({"id": 2, "result": {"thread": {"id": "copy", "ephemeral": true}}})).1;
        assert_eq!((&out[0]["method"], &out[0]["params"]["threadId"]), (&json!("thread/inject_items"), &json!("copy")));
        assert_eq!(out[0]["params"]["items"][0]["content"][0]["text"], crate::side::RULES);
        let out = t.hear(&json!({"id": 3, "result": {}})).1;
        assert_eq!(out, [json!({"id": 4, "method": "turn/start", "params": {"threadId": "copy", "input": [{"type": "text", "text": "Why?"}]}})]);
        let lines = [
            json!({"id": 4, "result": {"turn": {"id": "t1"}}}),
            json!({"method": "turn/started", "params": {"threadId": "copy"}}),
            // The thread it copied carries on, and isn't the answer.
            json!({"method": "item/agentMessage/delta", "params": {"threadId": "parent", "delta": "Meanwhile"}}),
            json!({"method": "item/started", "params": {"threadId": "copy", "item": {"type": "agentMessage", "text": ""}}}),
            json!({"method": "item/agentMessage/delta", "params": {"threadId": "copy", "delta": "Looking."}}),
            json!({"method": "item/started", "params": {"threadId": "copy", "item": {"type": "commandExecution", "command": "git log -3\n--oneline"}}}),
            json!({"method": "item/started", "params": {"threadId": "copy", "item": {"type": "mcpToolCall", "server": "docs", "tool": "search"}}}),
            json!({"method": "item/started", "params": {"threadId": "copy", "item": {"type": "agentMessage", "text": ""}}}),
            json!({"method": "item/agentMessage/delta", "params": {"threadId": "copy", "delta": "Because."}}),
            json!({"method": "turn/completed", "params": {"threadId": "copy", "turn": {"status": "completed"}}}),
        ];
        let heard: Vec<Event> = lines.iter().flat_map(|l| t.hear(l).0).collect();
        assert_eq!(heard, [Event::Said("Looking.".into()), Event::Tool("`git log -3`".into()), Event::Tool("docs · search".into()), Event::Said("\n\n".into()), Event::Said("Because.".into()), Event::Done(None)]);
        // The next question goes straight in.
        assert_eq!(t.ask("And?")[0]["id"], 5);
        // What would have to be asked of you is declined, and Codealong lends it no tools.
        assert_eq!(t.hear(&json!({"id": 9, "method": "item/commandExecution/requestApproval", "params": {}})).1, [json!({"id": 9, "result": {"decision": "decline"}})]);
        assert_eq!(t.hear(&json!({"id": 10, "method": "item/tool/call", "params": {}})).1[0]["error"]["code"], -32601);
        let failed = t.hear(&json!({"method": "turn/completed", "params": {"threadId": "copy", "turn": {"status": "failed", "error": {"message": "usage limit"}}}})).0;
        assert_eq!(failed, [Event::Done(Some("Codex: usage limit".into()))]);
        // A thread that can't be copied says why, to the question that waited.
        let mut t = SideThread::new("gone", Path::new("/w"), TurnRan::default());
        t.ask("Why?");
        let out = t.hear(&json!({"id": 1, "result": {}})).1;
        // And one that says nothing of how it ran is copied read-only, asking nothing.
        let p = &out[1]["params"];
        assert!(p.get("model").is_none());
        assert_eq!((&p["sandbox"], &p["approvalPolicy"]), (&json!("read-only"), &json!("never")));
        assert_eq!(t.hear(&json!({"id": 2, "error": {"code": -32600, "message": "no rollout found for thread id gone"}})).0, [Event::Done(Some("Codex: no rollout found for thread id gone".into()))]);
    }

    /// A thread about a PR is started, not copied: ephemeral, read-only, asking nothing, told its
    /// own rules, and asked as a copy is.
    #[test]
    fn a_thread_of_its_own_is_started_then_asked() {
        use crate::side::Dialect;
        use serde_json::json;
        let mut t = SideThread::fresh(Path::new("/w"), "About PR #7.");
        assert!(t.ask("Why?").is_empty());
        let out = t.hear(&json!({"id": 1, "result": {}})).1;
        assert_eq!(out[1]["method"], "thread/start");
        assert_eq!(out[1]["params"], json!({ "ephemeral": true, "cwd": "/w", "approvalPolicy": "never", "sandbox": "read-only", "developerInstructions": "About PR #7." }));
        let out = t.hear(&json!({"id": 2, "result": {"thread": {"id": "mine", "ephemeral": true}}})).1;
        assert_eq!((&out[0]["method"], &out[0]["params"]["threadId"], &out[0]["params"]["items"][0]["content"][0]["text"]), (&json!("thread/inject_items"), &json!("mine"), &json!("About PR #7.")));
        let out = t.hear(&json!({"id": 3, "result": {}})).1;
        assert_eq!(out, [json!({"id": 4, "method": "turn/start", "params": {"threadId": "mine", "input": [{"type": "text", "text": "Why?"}]}})]);
    }

    #[test]
    fn a_copy_runs_as_the_threads_latest_turn_did() {
        let dir = crate::fixtures::scratch("codex-ran");
        let rollout = dir.join("rollout.jsonl");
        let lines = [
            r#"{"timestamp":"2026-09-30T10:00:00.000Z","type":"session_meta","payload":{"id":"t"}}"#.to_string(),
            r#"{"timestamp":"2026-09-30T10:00:01.000Z","type":"turn_context","payload":{"model":"gpt-old","effort":"low","approval_policy":"never","sandbox_policy":{"type":"read-only"}}}"#.to_string(),
            r#"{"timestamp":"2026-09-30T10:00:02.000Z","type":"turn_context","payload":{"model":"gpt-new","effort":"xhigh","approval_policy":"on-request","sandbox_policy":{"type":"workspace-write","network_access":false}}}"#.to_string(),
            // A command's output that only mentions one.
            format!(r#"{{"timestamp":"2026-09-30T10:00:03.000Z","type":"response_item","payload":{{"output":"grep \"turn_context\" {}"}}}}"#, "x".repeat(2000)),
        ];
        std::fs::write(&rollout, lines.join("\n") + "\n").unwrap();
        assert_eq!(last_turn(&rollout), TurnRan { model: Some("gpt-new".into()), sandbox: Some("workspace-write".into()), approval: Some("on-request".into()) });
        // A policy that isn't one of the three names (a granular one) is left to Codex's settings.
        std::fs::write(&rollout, r#"{"timestamp":"2026-09-30T10:00:01.000Z","type":"turn_context","payload":{"model":"gpt-new","approval_policy":{"granular":{}},"sandbox_policy":{"type":"external-sandbox"}}}"#).unwrap();
        assert_eq!(last_turn(&rollout), TurnRan { model: Some("gpt-new".into()), sandbox: None, approval: None });
        std::fs::write(&rollout, &lines[0]).unwrap();
        assert_eq!(last_turn(&rollout), TurnRan::default());
        assert_eq!(last_turn(&dir.join("none.jsonl")), TurnRan::default());
    }
}
