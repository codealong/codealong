//! What came in to a Claude Code session, line by line of its transcript, for turns.rs.
//!
//! Claude Code says what began each turn on the turn's first line (2.1.322, this machine's
//! transcripts, 2026-11-11): `turnOrigin` is `human` (a prompt typed or pasted, a slash command),
//! `peer` (another session's message, on an `isMeta` line with `origin: {kind: "peer"}`),
//! `task_notification` (a background task or a subagent reporting back), `scheduled` (a wakeup
//! the session set, a loop's tick: its command's tags, after a `system` / `scheduled_task_fire`
//! line) or `sdk` (a program driving it through the Agent SDK, as Codealong's own sessions are).
//! Older lines have only `origin.kind` (`human`, `peer`, `task-notification`), and the oldest
//! neither, where a prompt is whatever `user_prompt` takes for one. Of 5,700 such lines over a
//! week here, 23% were typed.
//!
//! What comes in while a turn runs isn't a `user` line: the turn takes it in as a
//! `queued_command` attachment, marked the same way (`attachment.origin.kind`), timed when it
//! came. A line with no mark and none of the above is no turn: a command of Claude Code's own
//! (`/model`), its output, a caveat, a skill's expansion (`isMeta`).
//!
//! Three more lines are yours without being prompts. An answer to a question the agent asked
//! is on the call's result (`toolUseResult.answers`, with the `questions` and their choices), so
//! whether you picked what it offered is known for sure. A call refused with words is those
//! words (`asks::REFUSED_SAYING`). And a turn you stopped leaves "[Request interrupted by user"
//! or a result "User rejected tool use", which the next prompt is told by (`State::stopped`).
//!
//! A subagent's log isn't read: what comes in there is its parent agent's doing. A fork made
//! through the Agent SDK copies lines under new ids, each naming the line it copies
//! (`forkedFrom.messageUuid`), which is the name a copied line goes by here, so a copy is the
//! same turn as its original.

use serde_json::Value;

use super::conversation::peer_source;
use super::transcript::{line_words, ran_a_turn, reply_text};
use crate::turns::{Came, Picked, Source, State};
use crate::util::parse_rfc3339_ms;

/// What Claude Code writes where you stopped a turn: as a line of its own, or as the result of
/// the call that was running.
const INTERRUPTED: &str = "[Request interrupted by user";
const REJECTED: &str = "User rejected tool use";

pub fn turn_step(line: &[u8], st: &mut State) -> Option<Came> {
    let has = |needle: &[u8]| memchr::memmem::find(line, needle).is_some();
    if !has(br#""type":"user""#) && !has(br#""type":"assistant""#) && !has(b"queued_command") && !has(b"scheduled_task_fire") {
        return None;
    }
    let v: Value = serde_json::from_slice(line).ok()?;
    let flag = |k: &str| v.get(k).and_then(Value::as_bool) == Some(true);
    if flag("isSidechain") || flag("isCompactSummary") {
        return None;
    }
    match v.get("type").and_then(Value::as_str)? {
        "system" => {
            if v.get("subtype").and_then(Value::as_str) == Some("scheduled_task_fire") {
                st.wakeup = true;
            }
            None
        }
        "assistant" => {
            if let Some(text) = reply_text(&v) {
                st.say(&text);
            }
            None
        }
        "attachment" => queued(&v),
        "user" => user(&v, st),
        _ => None,
    }
}

/// The line's own name, or the one it copies.
fn id_of(v: &Value) -> Option<String> {
    v.pointer("/forkedFrom/messageUuid").or_else(|| v.get("uuid")).and_then(Value::as_str).map(String::from)
}

fn at_of(v: &Value) -> i64 {
    v.get("timestamp").and_then(Value::as_str).and_then(parse_rfc3339_ms).unwrap_or(0)
}

/// A content's text: a string, or its text parts and its results' text joined.
fn text_in(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(items) => items
            .iter()
            .filter_map(|i| match i.get("type").and_then(Value::as_str) {
                Some("text") => i.get("text").and_then(Value::as_str).map(String::from),
                Some("tool_result") => i.get("content").map(text_in),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

fn came(v: &Value, source: Source, kind: &'static str, text: String) -> Option<Came> {
    Some(Came { id: id_of(v)?, at: at_of(v), source, kind, sender: None, mid: false, text, picked: None })
}

fn user(v: &Value, st: &mut State) -> Option<Came> {
    let content = v.pointer("/message/content")?;
    let is_result = content.as_array().is_some_and(|items| items.iter().any(|i| i.get("type").and_then(Value::as_str) == Some("tool_result")));
    if is_result {
        return result(v, content, st);
    }
    // A wakeup's prompt is the `user` line after its `scheduled_task_fire`, whatever it's marked.
    let wakeup = std::mem::take(&mut st.wakeup);
    if text_in(content).trim_start().starts_with(INTERRUPTED) {
        st.stopped = true;
        return None;
    }
    if let Some((source, sender, text)) = peer_source(v) {
        let kind = if source == Source::You { "prompt" } else { "message" };
        return Some(Came { sender, ..came(v, source, kind, text)? });
    }
    let origin = v.get("turnOrigin").and_then(Value::as_str).or_else(|| v.pointer("/origin/kind").and_then(Value::as_str));
    // A subagent's report back comes as a peer's with nobody to have sent it (`handback`), on a
    // line marked meta as a peer's is; any other meta line is no turn.
    if v.get("isMeta").and_then(Value::as_bool) == Some(true) && origin != Some("peer") {
        return None;
    }
    match origin {
        Some("human") | Some("sdk") => came(v, Source::You, "prompt", line_words(v).unwrap_or_default()),
        Some("scheduled") => came(v, Source::Itself, "wakeup", String::new()),
        Some("task_notification") | Some("task-notification") | Some("peer") => came(v, Source::Itself, "task", String::new()),
        Some(_) => None,
        None if wakeup => came(v, Source::Itself, "wakeup", String::new()),
        // Before the marks: a prompt is what reads as one.
        None => (ran_a_turn(v) || line_words(v).is_some_and(|w| !w.trim().is_empty()) && super::transcript::you_typed(v)).then(|| came(v, Source::You, "prompt", line_words(v).unwrap_or_default())).flatten(),
    }
}

/// A message the running turn took in: yours, another session's, or a task's report.
fn queued(v: &Value) -> Option<Came> {
    let a = v.get("attachment").filter(|a| a.get("type").and_then(Value::as_str) == Some("queued_command"))?;
    let at = a.get("timestamp").and_then(Value::as_str).and_then(parse_rfc3339_ms).unwrap_or_else(|| at_of(v));
    let text = a.get("prompt").map(text_in).unwrap_or_default();
    let mid = |c: Came| Came { mid: true, at, ..c };
    if let Some((source, sender, text)) = peer_source(v) {
        let kind = if source == Source::You { "prompt" } else { "message" };
        return Some(mid(Came { sender, ..came(v, source, kind, text)? }));
    }
    let origin = a.pointer("/origin/kind").and_then(Value::as_str);
    let task = matches!(origin, Some("task-notification") | Some("task_notification") | Some("peer")) || (origin.is_none() && text.trim_start().starts_with("<task-notification"));
    if task {
        return came(v, Source::Itself, "task", String::new()).map(mid);
    }
    came(v, Source::You, "prompt", text.trim().to_string()).map(mid)
}

/// What a call's result says of you: your answer to a question it asked, the words you refused
/// it with, or that you stopped the turn.
fn result(v: &Value, content: &Value, st: &mut State) -> Option<Came> {
    if let Some(answers) = v.pointer("/toolUseResult/answers").and_then(Value::as_object).filter(|a| !a.is_empty()) {
        let questions = v.pointer("/toolUseResult/questions").and_then(Value::as_array).cloned().unwrap_or_default();
        let said: Vec<String> = answers.iter().map(|(q, a)| format!("{q} → {}", answer_words(a))).collect();
        // What your answer answers is the question, with what it offered.
        let asked: Vec<String> = questions.iter().map(|q| format!("{} ({})", q.get("question").and_then(Value::as_str).unwrap_or(""), labels(q).join(" | "))).collect();
        st.say(&asked.join("\n"));
        return Some(Came { picked: Some(picked(&questions, answers)), ..came(v, Source::You, "answer", said.join("; "))? });
    }
    let text = text_in(content);
    if let Some((_, words)) = text.split_once(super::asks::REFUSED_SAYING) {
        return Some(Came { mid: true, ..came(v, Source::You, "prompt", words.trim().to_string())? });
    }
    if v.get("toolUseResult").and_then(Value::as_str) == Some(REJECTED) || text.contains(INTERRUPTED) {
        st.stopped = true;
    }
    None
}

fn answer_words(a: &Value) -> String {
    a.as_str().map(String::from).unwrap_or_else(|| a.as_array().map(|l| l.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(", ")).unwrap_or_default())
}

fn labels(question: &Value) -> Vec<&str> {
    question.get("options").and_then(Value::as_array).map(|o| o.iter().filter_map(|o| o.get("label").and_then(Value::as_str)).collect()).unwrap_or_default()
}

/// What your answers picked of what the questions offered: words of your own if any answer has
/// some, else another choice than the first if any picked one, else the first.
fn picked(questions: &[Value], answers: &serde_json::Map<String, Value>) -> Picked {
    let mut all = Picked::First;
    for (asked, answer) in answers {
        let offered = questions.iter().find(|q| q.get("question").and_then(Value::as_str) == Some(asked.as_str())).map(labels).unwrap_or_default();
        let given: Vec<String> = match answer {
            Value::Array(list) => list.iter().filter_map(Value::as_str).map(String::from).collect(),
            other => other.as_str().map(|s| s.split(", ").map(String::from).collect()).unwrap_or_default(),
        };
        let whole = answer_words(answer);
        let one = if offered.first().is_some_and(|f| *f == whole) {
            Picked::First
        } else if !given.is_empty() && given.iter().all(|g| offered.contains(&g.as_str())) || offered.contains(&whole.as_str()) {
            Picked::Offered
        } else {
            Picked::Own
        };
        all = match (all, one) {
            (Picked::Own, _) | (_, Picked::Own) => Picked::Own,
            (Picked::Offered, _) | (_, Picked::Offered) => Picked::Offered,
            _ => Picked::First,
        };
    }
    all
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn step(st: &mut State, v: Value) -> Option<Came> {
        turn_step(format!("{v}\n").as_bytes(), st)
    }

    fn user_line(uuid: &str, origin: &str, content: Value) -> Value {
        json!({"type": "user", "uuid": uuid, "timestamp": "2026-11-10T10:00:00.000Z", "turnOrigin": origin, "origin": {"kind": origin}, "message": {"role": "user", "content": content}})
    }

    #[test]
    fn a_turn_is_begun_by_what_its_first_line_says() {
        let mut st = State::default();
        let typed = step(&mut st, user_line("u1", "human", json!("Fix the parser"))).unwrap();
        assert_eq!((typed.source, typed.kind, typed.text.as_str(), typed.id.as_str(), typed.mid), (Source::You, "prompt", "Fix the parser", "u1", false));
        assert!(typed.at > 0);
        // What the agent says is what the next prompt answers; a call isn't.
        assert_eq!(step(&mut st, json!({"type": "assistant", "message": {"content": [{"type": "text", "text": "Done. Shall I push?"}]}})), None);
        assert_eq!(step(&mut st, json!({"type": "assistant", "message": {"content": [{"type": "tool_use", "name": "Bash", "input": {}}]}})), None);
        assert_eq!(st.said, "Done. Shall I push?");
        // A background task reporting back, and a wakeup, are the session's own.
        let task = step(&mut st, json!({"type": "user", "uuid": "u2", "turnOrigin": "task_notification", "origin": {"kind": "task-notification"}, "message": {"content": "<task-notification>…</task-notification>"}})).unwrap();
        assert_eq!((task.source, task.kind), (Source::Itself, "task"));
        assert_eq!(step(&mut st, json!({"type": "system", "subtype": "scheduled_task_fire", "taskId": "t1"})), None);
        let wake = step(&mut st, json!({"type": "user", "uuid": "u3", "turnOrigin": "scheduled", "message": {"content": "<command-message>loop</command-message>\n<command-name>/loop</command-name>"}})).unwrap();
        assert_eq!((wake.source, wake.kind), (Source::Itself, "wakeup"));
        // An older wakeup has no mark: it's the line after its fire.
        step(&mut st, json!({"type": "system", "subtype": "scheduled_task_fire", "taskId": "t1"}));
        assert_eq!(step(&mut st, json!({"type": "user", "uuid": "u4", "message": {"content": "check the deploy"}})).map(|c| c.source), Some(Source::Itself));
        // A program driving it through the SDK types as you do.
        assert_eq!(step(&mut st, json!({"type": "user", "uuid": "u5", "turnOrigin": "sdk", "message": {"content": "Carry on"}})).map(|c| (c.source, c.text)), Some((Source::You, "Carry on".into())));
        // Not turns: a skill's expansion, a command of Claude Code's own, a subagent's line, a summary.
        assert_eq!(step(&mut st, json!({"type": "user", "uuid": "m", "isMeta": true, "message": {"content": "Base directory for this skill…"}})), None);
        assert_eq!(step(&mut st, json!({"type": "user", "uuid": "c", "message": {"content": "<local-command-stdout>Set model to Opus</local-command-stdout>"}})), None);
        assert_eq!(step(&mut st, json!({"type": "user", "uuid": "s", "isSidechain": true, "turnOrigin": "human", "message": {"content": "Find the tests"}})), None);
        assert_eq!(step(&mut st, json!({"type": "user", "uuid": "k", "isCompactSummary": true, "message": {"content": "This session is being continued…"}})), None);
    }

    #[test]
    fn another_sessions_message_is_its_agents_codealongs_or_yours() {
        let mut st = State::default();
        let peer = |uuid: &str, name: &str, body: &str| json!({"type": "user", "uuid": uuid, "isMeta": true, "turnOrigin": "peer", "origin": {"kind": "peer", "name": name, "from": "unknown", "body": body}, "message": {"content": "<cross-session-message>…</cross-session-message>"}});
        let agent = step(&mut st, peer("p1", "parser-review", "This message is from another agent, “Review the parser” (Codex), sent with `codealong session send`; it isn't from your user. To answer it, run `codealong session send codex:codex:t1 \"<your answer>\"`.\n\nTwo findings.")).unwrap();
        assert_eq!((agent.source, agent.kind, agent.sender.as_deref(), agent.text.as_str()), (Source::Agent, "message", Some("codex:codex:t1"), "Two findings."));
        // One sent with Claude Code's own SendMessage names its session.
        assert_eq!(step(&mut st, peer("p2", "parser-review", "Two more.")).map(|c| (c.source, c.sender)), Some((Source::Agent, Some("parser-review".into()))));
        let own = step(&mut st, peer("p3", "Codealong", &crate::send::by_codealong("On your PR #7: ken approved it."))).unwrap();
        assert_eq!((own.source, own.text.as_str()), (Source::Codealong, "On your PR #7: ken approved it."));
        // What you send a Claude app session from Codealong comes the same way, and is yours.
        let yours = step(&mut st, peer("p4", crate::claude::peer::FROM_CODEALONG, &format!("{}\n\nCarry on", crate::claude::peer::FROM_YOU))).unwrap();
        assert_eq!((yours.source, yours.kind, yours.text.as_str()), (Source::You, "prompt", "Carry on"));
        // A subagent's report back is nobody's message.
        let handback = json!({"type": "user", "uuid": "p5", "isMeta": true, "turnOrigin": "peer", "origin": {"kind": "peer", "handback": true, "body": "Found three."}, "message": {"content": "…"}});
        assert_eq!(step(&mut st, handback).map(|c| (c.source, c.kind)), Some((Source::Itself, "task")));
    }

    #[test]
    fn what_a_running_turn_takes_in_came_mid_turn() {
        let mut st = State::default();
        let queued = |uuid: &str, kind: &str, prompt: Value| json!({"type": "attachment", "uuid": uuid, "timestamp": "2026-11-10T10:05:00.000Z", "attachment": {"type": "queued_command", "prompt": prompt, "timestamp": "2026-11-10T10:03:00.000Z", "origin": {"kind": kind}}});
        let yours = step(&mut st, queued("q1", "human", json!("Also update the docs"))).unwrap();
        assert_eq!((yours.source, yours.mid, yours.text.as_str()), (Source::You, true, "Also update the docs"));
        assert_eq!(yours.at, parse_rfc3339_ms("2026-11-10T10:03:00.000Z").unwrap(), "timed when it came");
        assert_eq!(step(&mut st, queued("q2", "task-notification", json!("<task-notification>…"))).map(|c| (c.source, c.mid)), Some((Source::Itself, true)));
        let from_peer = json!({"type": "attachment", "uuid": "q3", "timestamp": "2026-11-10T10:05:00.000Z", "attachment": {"type": "queued_command", "prompt": "<cross-session-message>…", "origin": {"kind": "peer", "name": "parser-review", "body": "Two findings."}}});
        assert_eq!(step(&mut st, from_peer).map(|c| (c.source, c.mid, c.text)), Some((Source::Agent, true, "Two findings.".into())));
        // Another attachment is nothing.
        assert_eq!(step(&mut st, json!({"type": "attachment", "uuid": "f", "attachment": {"type": "file", "filename": "queued_command.txt"}})), None);
    }

    #[test]
    fn an_answer_a_refusal_and_a_stop_are_yours() {
        let mut st = State::default();
        let questions = json!([{"question": "Which database?", "options": [{"label": "Postgres (Recommended)"}, {"label": "SQLite"}]}, {"question": "Which tests?", "multiSelect": true, "options": [{"label": "Unit"}, {"label": "End to end"}]}]);
        let answered = |uuid: &str, answers: Value| json!({"type": "user", "uuid": uuid, "timestamp": "2026-11-10T10:00:00.000Z", "toolUseResult": {"questions": questions, "answers": answers}, "message": {"content": [{"type": "tool_result", "tool_use_id": "t", "content": "User has answered your questions"}]}});
        let first = step(&mut st, answered("a1", json!({"Which database?": "Postgres (Recommended)"}))).unwrap();
        assert_eq!((first.source, first.kind, first.picked, first.text.as_str()), (Source::You, "answer", Some(Picked::First), "Which database? → Postgres (Recommended)"));
        assert!(st.said.contains("Which database? (Postgres (Recommended) | SQLite)"), "{}", st.said);
        assert_eq!(step(&mut st, answered("a2", json!({"Which database?": "SQLite"}))).unwrap().picked, Some(Picked::Offered));
        assert_eq!(step(&mut st, answered("a3", json!({"Which database?": "Postgres (Recommended)", "Which tests?": "Unit, End to end"}))).unwrap().picked, Some(Picked::Offered));
        assert_eq!(step(&mut st, answered("a4", json!({"Which database?": "Neither, keep the flat file"}))).unwrap().picked, Some(Picked::Own));
        // A call refused with words is those words, said into the turn.
        let refused = json!({"type": "user", "uuid": "r1", "message": {"content": [{"type": "tool_result", "tool_use_id": "t", "is_error": true, "content": format!("{}Explain it first", crate::claude::asks::REFUSED_SAYING)}]}});
        assert_eq!(step(&mut st, refused).map(|c| (c.source, c.mid, c.text)), Some((Source::You, true, "Explain it first".into())));
        // A turn you stopped says so on what comes next from you.
        assert!(!st.stopped);
        let rejected = json!({"type": "user", "uuid": "r2", "toolUseResult": "User rejected tool use", "message": {"content": [{"type": "tool_result", "tool_use_id": "t", "is_error": true, "content": "The user doesn't want to proceed with this tool use."}]}});
        assert_eq!(step(&mut st, rejected), None);
        assert!(st.stopped);
        st.stopped = false;
        assert_eq!(step(&mut st, json!({"type": "user", "uuid": "i", "message": {"content": [{"type": "text", "text": "[Request interrupted by user for tool use]"}]}})), None);
        assert!(st.stopped);
        // An ordinary result is the agent's own doing.
        assert_eq!(step(&mut st, json!({"type": "user", "uuid": "o", "message": {"content": [{"type": "tool_result", "tool_use_id": "t", "content": "ok"}]}})), None);
        // A copy made through the SDK goes by the line it copies.
        let copy = json!({"type": "user", "uuid": "new", "forkedFrom": {"sessionId": "s", "messageUuid": "u1"}, "turnOrigin": "human", "message": {"content": "Fix the parser"}});
        assert_eq!(step(&mut st, copy).map(|c| c.id), Some("u1".into()));
    }
}
