//! Side conversations: a headless Claude Code on a fork of the session that keeps no record
//! (`Adapter::side`), and its stream-json lines.

use serde_json::Value;

use super::transcript::tool_request;

/// The mode a side conversation's copy runs in, given the session's (`permission_mode`): the
/// session's own where that lets it act without asking (it skips prompts, takes edits, or
/// leaves the asking to Claude Code's own reviewer), so a copy asked to change something can,
/// as the session could. Any other mode, and a session whose mode isn't written, gets the usual
/// one, where what would ask is refused: a copy in plan mode would plan, and nothing says a
/// session with no mode on record may act unasked.
/// The flag always goes, since without it a copy would start in the account's default mode.
pub(super) fn side_mode(session: Option<&str>) -> &'static str {
    match session {
        Some("bypassPermissions") => "bypassPermissions",
        Some("acceptEdits") => "acceptEdits",
        Some("dontAsk") => "dontAsk",
        Some("auto") => "auto",
        _ => "default",
    }
}

/// How Codealong talks to a side conversation's copy (side.rs, `Adapter::side`): Claude Code's
/// stream-json, a line a message each way. A question is a `user` line; the answer comes as
/// `stream_event` lines (the API's own events: `content_block_delta` with a `text_delta`), each
/// tool call whole in an `assistant` line, and a `result` line when the turn ends (one that
/// isn't for a task notification). A conversation of its own about a PR (`Adapter::fresh_side`) talks
/// the same way, told other rules.
pub struct SideCopy {
    /// What it's told with its first question: a copy's rules (`side::RULES`), or a PR's.
    rules: String,
    /// It has been told what it is, with its first question.
    told: bool,
    /// Some of this answer is written: what comes after a tool call starts a new paragraph.
    said: bool,
}

impl Default for SideCopy {
    fn default() -> SideCopy {
        SideCopy::told(crate::side::RULES)
    }
}

impl SideCopy {
    /// One told `rules` with its first question.
    pub fn told(rules: &str) -> SideCopy {
        SideCopy { rules: rules.to_string(), told: false, said: false }
    }
}

impl crate::side::Dialect for SideCopy {
    fn open(&mut self) -> Vec<Value> {
        Vec::new()
    }

    /// The rules go in the first question, not the system prompt: a resumed conversation sends
    /// the system prompt it recorded whatever a later launch passes, and the session's prompt
    /// cache holds only while that and the tools stay as they were. A PR's conversation has
    /// them in the same place, so the two are told alike.
    fn ask(&mut self, text: &str) -> Vec<Value> {
        let text = if self.told { text.to_string() } else { format!("<system-reminder>\n{}\n</system-reminder>\n\n{text}", self.rules) };
        self.told = true;
        self.said = false;
        vec![serde_json::json!({ "type": "user", "message": { "role": "user", "content": text } })]
    }

    fn hear(&mut self, line: &Value) -> (Vec<crate::side::Event>, Vec<Value>) {
        use crate::side::Event;
        let text = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).map(String::from);
        let kind = |v: &Value| v.get("type").and_then(Value::as_str).map(String::from).unwrap_or_default();
        // A subagent's lines aren't the answer.
        if line.get("parent_tool_use_id").is_some_and(|p| !p.is_null()) {
            return (Vec::new(), Vec::new());
        }
        let events = match kind(line).as_str() {
            "stream_event" => {
                let e = line.get("event").unwrap_or(&Value::Null);
                match kind(e).as_str() {
                    "content_block_start" if self.said && e.get("content_block").is_some_and(|b| kind(b) == "text") => vec![Event::Said("\n\n".into())],
                    "content_block_delta" => match e.get("delta").filter(|d| kind(d) == "text_delta").and_then(|d| text(d, "text")) {
                        Some(t) => {
                            self.said = true;
                            vec![Event::Said(t)]
                        }
                        None => Vec::new(),
                    },
                    _ => Vec::new(),
                }
            }
            "assistant" => {
                let blocks = line.pointer("/message/content").and_then(Value::as_array).cloned().unwrap_or_default();
                blocks.iter().filter(|b| kind(b) == "tool_use").map(|b| Event::Tool(tool_request(&text(b, "name").unwrap_or_default(), b.get("input").unwrap_or(&Value::Null)))).collect()
            }
            // A background command the session left running is reported stopped as the copy
            // resumes, and that report is a turn of its own, ended by its own empty `result`
            // ahead of the question's: taken for the answer, it ended the question with nothing
            // said, and the real answer that came after had no question left to go to.
            "result" if line.pointer("/origin/kind").and_then(Value::as_str) == Some("task-notification") => Vec::new(),
            "result" => {
                let failed = line.get("is_error").and_then(Value::as_bool).unwrap_or(false);
                let why = || text(line, "result").filter(|r| !r.trim().is_empty()).unwrap_or_else(|| "It ended without an answer.".into());
                vec![Event::Done(failed.then(why))]
            }
            _ => Vec::new(),
        };
        (events, Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    use crate::claude::ClaudeCode;
    use crate::model::{Harness, Session};

    #[test]
    fn a_side_copy_is_told_what_it_is_once_and_heard() {
        use crate::side::{Dialect, Event};
        use serde_json::json;
        let mut copy = SideCopy::default();
        assert!(copy.open().is_empty());
        let first = copy.ask("What changed?");
        let said = first[0].pointer("/message/content").and_then(Value::as_str).unwrap();
        assert!(said.starts_with("<system-reminder>\nThis is a side conversation.") && said.ends_with("</system-reminder>\n\nWhat changed?"), "{said}");
        let lines = [
            // A command the session left running in the background, ended as the copy resumes
            // (2.1.289): its turn's end isn't the answer's.
            json!({"type": "system", "subtype": "task_notification", "status": "stopped", "summary": "Background shell command didn't finish before the previous session ended"}),
            json!({"type": "system", "subtype": "init", "tools": ["Bash"]}),
            json!({"type": "result", "subtype": "success", "is_error": false, "num_turns": 0, "result": "", "origin": {"kind": "task-notification"}}),
            json!({"type": "system", "subtype": "init", "tools": ["Bash"]}),
            json!({"type": "stream_event", "parent_tool_use_id": null, "event": {"type": "content_block_start", "index": 0, "content_block": {"type": "thinking", "thinking": ""}}}),
            json!({"type": "stream_event", "parent_tool_use_id": null, "event": {"type": "content_block_delta", "index": 0, "delta": {"type": "thinking_delta", "thinking": "hm"}}}),
            json!({"type": "stream_event", "parent_tool_use_id": null, "event": {"type": "content_block_start", "index": 1, "content_block": {"type": "text", "text": ""}}}),
            json!({"type": "stream_event", "parent_tool_use_id": null, "event": {"type": "content_block_delta", "index": 1, "delta": {"type": "text_delta", "text": "Let me look."}}}),
            // The text again, whole, and the call.
            json!({"type": "assistant", "parent_tool_use_id": null, "message": {"content": [{"type": "text", "text": "Let me look."}, {"type": "tool_use", "id": "toolu_1", "name": "Bash", "input": {"command": "git status"}}]}}),
            // A subagent's words aren't the answer.
            json!({"type": "stream_event", "parent_tool_use_id": "toolu_2", "event": {"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": "Searching"}}}),
            json!({"type": "user", "message": {"content": [{"type": "tool_result", "tool_use_id": "toolu_1", "content": "M a.rs"}]}}),
            json!({"type": "stream_event", "parent_tool_use_id": null, "event": {"type": "content_block_start", "index": 0, "content_block": {"type": "text", "text": ""}}}),
            json!({"type": "stream_event", "parent_tool_use_id": null, "event": {"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": "One file."}}}),
            json!({"type": "result", "subtype": "success", "is_error": false, "result": "One file."}),
        ];
        let heard: Vec<Event> = lines.iter().flat_map(|l| copy.hear(l).0).collect();
        assert_eq!(heard, [Event::Said("Let me look.".into()), Event::Tool("Bash · `git status`".into()), Event::Said("\n\n".into()), Event::Said("One file.".into()), Event::Done(None)]);
        // A follow-up is only itself.
        assert_eq!(copy.ask("And then?")[0].pointer("/message/content"), Some(&json!("And then?")));
        let failed = copy.hear(&json!({"type": "result", "subtype": "success", "is_error": true, "result": "API Error: overloaded"})).0;
        assert_eq!(failed, [Event::Done(Some("API Error: overloaded".into()))]);
        let failed = copy.hear(&json!({"type": "result", "subtype": "error_during_execution", "is_error": true})).0;
        assert_eq!(failed, [Event::Done(Some("It ended without an answer.".into()))]);
    }

    #[test]
    fn a_side_copy_runs_on_its_account_and_keeps_no_record() {
        use crate::harness::Adapter;
        let account = |home: &str| crate::config::AccountCfg { name: "c".into(), harness: "claude".into(), home: home.into(), command: None, start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false };
        let mut s = Session { live: true, title: "t".into(), cwd: "/w".into(), ..Session::new(Harness::Claude, "c", "abc") };
        let usual = account("~/.claude");
        let r = ClaudeCode.side(&crate::harness::Ctx { account: &usual }, &s, Path::new("/w")).unwrap();
        assert_eq!(r.program, PathBuf::from("claude"));
        // A session whose mode isn't written anywhere gets the usual one, where what would ask is refused.
        assert_eq!(r.args.join(" "), "-p --input-format stream-json --output-format stream-json --verbose --include-partial-messages --resume abc --fork-session --no-session-persistence --permission-mode default");
        assert_eq!(side_mode(Some("bypassPermissions")), "bypassPermissions");
        assert_eq!((side_mode(Some("acceptEdits")), side_mode(Some("auto"))), ("acceptEdits", "auto"));
        assert_eq!((side_mode(Some("plan")), side_mode(Some("default")), side_mode(Some("manual")), side_mode(None)), ("default", "default", "default", "default"));
        assert!(r.env.is_empty() && r.unset.contains(&"CLAUDE_CONFIG_DIR"));
        // Another account's folder is named, and a job with no transcript yet copies the one it goes on from.
        s.continues = Some("before".into());
        let other = account("/Users/me/.claude2");
        let r = ClaudeCode.side(&crate::harness::Ctx { account: &other }, &s, Path::new("/w")).unwrap();
        assert_eq!(r.env, [("CLAUDE_CONFIG_DIR".to_string(), "/Users/me/.claude2".to_string())]);
        assert!(!r.unset.contains(&"CLAUDE_CONFIG_DIR") && r.unset.contains(&"CODEALONG_SESSION"));
        assert!(r.args.join(" ").contains("--resume before --fork-session"));
    }

    /// A PR's conversation resumes nothing and keeps no record, in the usual mode, able to read
    /// what it's handed, and is told its own rules with its first question.
    #[test]
    fn a_conversation_of_its_own_resumes_nothing() {
        use crate::harness::Adapter;
        let other = crate::config::AccountCfg { name: "c2".into(), harness: "claude".into(), home: "/Users/me/.claude2".into(), command: None, start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false };
        let mut r = ClaudeCode.fresh_side(&crate::harness::Ctx { account: &other }, Path::new("/w"), "About PR #7.", Path::new("/support/pr/7")).unwrap();
        assert_eq!(r.args.join(" "), "-p --input-format stream-json --output-format stream-json --verbose --include-partial-messages --no-session-persistence --permission-mode default --add-dir=/support/pr/7");
        assert_eq!(r.env, [("CLAUDE_CONFIG_DIR".to_string(), "/Users/me/.claude2".to_string())]);
        let first = r.dialect.ask("Why?");
        assert_eq!(first[0].pointer("/message/content").and_then(Value::as_str), Some("<system-reminder>\nAbout PR #7.\n</system-reminder>\n\nWhy?"));
    }
}
