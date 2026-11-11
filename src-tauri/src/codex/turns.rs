//! What came in to a Codex thread, line by line of its rollout, for turns.rs.
//!
//! A rollout says when each turn starts and ends (`task_started`, then `task_complete` or
//! `turn_aborted`), and a prompt is an `event_msg` inside its turn: an `item_completed` whose
//! item is a `UserMessage` (with an `id`, which names it here), or in older rollouts a
//! `user_message` with none, which goes by its time and words (`turns::named`), as a fork's copy
//! of it does. Some rollouts have a prompt both ways, so a turn's prompts are told
//! apart by their text (`State::seen`). Nothing marks who a prompt is from: another session's
//! message and Codealong's own are typed in as yours are, led by the line Codealong puts first
//! (`turns::whose`). The app's automations wake a thread with a `<heartbeat>`, which is the
//! thread's own doing, and an answer to a question the agent asked comes as a
//! `<send_user_message_question_reply>`, in words of your own as far as the rollout says.
//!
//! A second prompt inside a turn that's still running was steered into it (2 of this machine's
//! latest rollouts had one, 2026-11-11). A turn you stopped ends in `turn_aborted`, as one the app
//! stopped as it quit does (.claude/rules/codex.md), so the next prompt is told it followed a
//! stop either way. A spawned agent's rollout and a guardian review's (`session_meta`'s
//! `source.subagent`) aren't read: what comes in there is their parent's doing.

use std::hash::{Hash, Hasher};

use serde_json::Value;

use super::rollout::{event_payload, request_of};
use crate::turns::{whose, Came, Picked, Source, State};
use crate::util::parse_rfc3339_ms;

const QUESTION_REPLY: &str = "<send_user_message_question_reply>";
const HEARTBEAT: &str = "<heartbeat>";

pub fn turn_step(line: &[u8], st: &mut State) -> Option<Came> {
    let has = |needle: &[u8]| memchr::memmem::find(line, needle).is_some();
    if !has(b"event_msg") && !has(b"session_meta") {
        return None;
    }
    if !has(b"session_meta") && !has(b"task_") && !has(b"turn_aborted") && !has(b"user_message") && !has(b"UserMessage") && !has(b"agent_message") && !has(b"AgentMessage") {
        return None;
    }
    let v: Value = serde_json::from_slice(line).ok()?;
    if v.get("type").and_then(Value::as_str) == Some("session_meta") {
        st.sub = v.pointer("/payload/source/subagent").is_some_and(|s| !s.is_null());
        return None;
    }
    let payload = event_payload(&v)?;
    let at = v.get("timestamp").and_then(Value::as_str).and_then(parse_rfc3339_ms).unwrap_or(0);
    let item_text = |kind: &str| {
        let item = payload.get("item").filter(|i| i.get("type").and_then(Value::as_str) == Some(kind))?;
        let text = item.get("content")?.as_array()?.iter().filter_map(|c| c.get("text").and_then(Value::as_str)).collect::<Vec<_>>().join("\n");
        Some((item.get("id").and_then(Value::as_str).map(String::from), text))
    };
    match payload.get("type").and_then(Value::as_str)? {
        "task_started" => {
            st.open = true;
            st.seen.clear();
            None
        }
        "task_complete" => {
            st.open = false;
            if let Some(last) = payload.get("last_agent_message").and_then(Value::as_str) {
                st.say(last);
            }
            None
        }
        "turn_aborted" => {
            st.open = false;
            st.stopped = true;
            None
        }
        "agent_message" => {
            st.say(payload.get("message").and_then(Value::as_str)?);
            None
        }
        "user_message" => prompt(st, at, None, payload.get("message").and_then(Value::as_str)?),
        "item_completed" => {
            if let Some((_, text)) = item_text("AgentMessage") {
                st.say(&text);
                return None;
            }
            let (id, text) = item_text("UserMessage")?;
            prompt(st, at, id, &text)
        }
        _ => None,
    }
}

fn prompt(st: &mut State, at: i64, id: Option<String>, raw: &str) -> Option<Came> {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    raw.hash(&mut h);
    let hash = h.finish();
    // The same prompt, written the other way too.
    if st.seen.contains(&hash) {
        return None;
    }
    // Another prompt inside a turn that already has one was steered into it.
    let mid = st.open && !st.seen.is_empty();
    st.seen.push(hash);
    // An older rollout's prompt has no id: it goes by when it came and what it said, which a
    // fork's or a moved thread's copy of the line has too.
    let id = id.unwrap_or_else(|| crate::turns::named(at, raw));
    let t = raw.trim_start();
    if t.starts_with(HEARTBEAT) {
        return Some(Came { id, at, source: Source::Itself, kind: "wakeup", sender: None, mid, text: String::new(), picked: None });
    }
    if let Some(reply) = t.strip_prefix(QUESTION_REPLY) {
        let text = reply.split("</send_user_message_question_reply>").next().unwrap_or(reply).trim().to_string();
        return Some(Came { id, at, source: Source::You, kind: "answer", sender: None, mid, text, picked: Some(Picked::Own) });
    }
    let text = request_of(raw).map(str::trim).filter(|r| !r.is_empty())?;
    let (source, sender) = whose(text);
    let kind = if source == Source::You { "prompt" } else { "message" };
    Some(Came { id, at, source, kind, sender, mid, text: crate::send::past_sender_lines(text).trim().to_string(), picked: None })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn step(st: &mut State, v: Value) -> Option<Came> {
        turn_step(format!("{v}\n").as_bytes(), st)
    }

    fn event(payload: Value) -> Value {
        json!({"timestamp": "2026-11-10T10:00:00.000Z", "type": "event_msg", "payload": payload})
    }

    fn user_item(id: &str, text: &str) -> Value {
        event(json!({"type": "item_completed", "turn_id": "t1", "item": {"type": "UserMessage", "id": id, "content": [{"type": "text", "text": text}]}}))
    }

    #[test]
    fn a_prompt_is_whose_its_leading_line_says_inside_its_turn() {
        let mut st = State { session: "codex:codex:t1".into(), ..Default::default() };
        assert_eq!(step(&mut st, event(json!({"type": "task_started", "turn_id": "t1"}))), None);
        let typed = step(&mut st, user_item("i1", "Fix the parser")).unwrap();
        assert_eq!((typed.source, typed.kind, typed.id.as_str(), typed.mid, typed.text.as_str()), (Source::You, "prompt", "i1", false, "Fix the parser"));
        // The same prompt the older way is the same prompt.
        assert_eq!(step(&mut st, event(json!({"type": "user_message", "message": "Fix the parser"}))), None);
        // Another one in the turn was steered in.
        assert_eq!(step(&mut st, user_item("i2", "Also the lexer")).map(|c| c.mid), Some(true));
        assert_eq!(step(&mut st, event(json!({"type": "item_completed", "item": {"type": "AgentMessage", "id": "a1", "phase": "final_answer", "content": [{"type": "text", "text": "Both fixed."}]}}))), None);
        assert_eq!(step(&mut st, event(json!({"type": "task_complete", "turn_id": "t1", "last_agent_message": "Both fixed. Push?"}))), None);
        assert_eq!((st.said.as_str(), st.open), ("Both fixed. Push?", false));
        // Codealong's own message and another agent's are typed in, led by their lines.
        step(&mut st, event(json!({"type": "task_started", "turn_id": "t2"})));
        let own = step(&mut st, user_item("i3", &crate::send::by_codealong("On your PR #7: ken approved it."))).unwrap();
        assert_eq!((own.source, own.kind, own.text.as_str(), own.mid), (Source::Codealong, "message", "On your PR #7: ken approved it.", false));
        step(&mut st, event(json!({"type": "task_complete", "turn_id": "t2"})));
        step(&mut st, event(json!({"type": "task_started", "turn_id": "t3"})));
        let agents = "This message is from another agent, “Fix the parser” (Claude Code), sent with `codealong session send`; it isn't from your user. To answer it, run `codealong session send claude:claude:a \"<your answer>\"`.\n\nLook again at the lexer.";
        assert_eq!(step(&mut st, user_item("i4", agents)).map(|c| (c.source, c.sender, c.text)), Some((Source::Agent, Some("claude:claude:a".into()), "Look again at the lexer.".into())));
        // An older rollout's prompt has no id of its own.
        step(&mut st, event(json!({"type": "task_started", "turn_id": "t4"})));
        let legacy = event(json!({"type": "user_message", "message": "## My request:\nShip it"}));
        let came = step(&mut st, legacy.clone()).unwrap();
        assert_eq!(came.text, "Ship it");
        // The same line read from a fork's rollout is the same prompt.
        let mut fork = State { session: "codex:codex:fork".into(), ..Default::default() };
        assert_eq!(step(&mut fork, legacy).map(|c| c.id), Some(came.id));
    }

    #[test]
    fn a_heartbeat_an_answer_and_a_stop() {
        let mut st = State::default();
        step(&mut st, event(json!({"type": "task_started", "turn_id": "t1"})));
        assert_eq!(step(&mut st, user_item("h1", "<heartbeat><instructions>Check the deploy</instructions></heartbeat>")).map(|c| (c.source, c.kind)), Some((Source::Itself, "wakeup")));
        assert_eq!(step(&mut st, event(json!({"type": "turn_aborted", "turn_id": "t1", "reason": "interrupted"}))), None);
        assert!(st.stopped && !st.open);
        step(&mut st, event(json!({"type": "task_started", "turn_id": "t2"})));
        let answer = step(&mut st, user_item("q1", "<send_user_message_question_reply>Use Postgres</send_user_message_question_reply>")).unwrap();
        assert_eq!((answer.source, answer.kind, answer.picked, answer.text.as_str()), (Source::You, "answer", Some(Picked::Own), "Use Postgres"));
        // A spawned agent's rollout says so on its first line; a thread of your own doesn't.
        let mut sub = State::default();
        step(&mut sub, json!({"type": "session_meta", "payload": {"id": "t9", "source": {"subagent": {"thread_spawn": {"parent_thread_id": "t1", "depth": 1}}}}}));
        assert!(sub.sub);
        step(&mut st, json!({"type": "session_meta", "payload": {"id": "t1", "source": "cli"}}));
        assert!(!st.sub);
        // Lines that aren't events, and events that bring nothing in.
        assert_eq!(step(&mut st, json!({"type": "response_item", "payload": {"role": "user", "content": [{"type": "input_text", "text": "<environment_context>…"}]}})), None);
        assert_eq!(step(&mut st, event(json!({"type": "token_count", "info": {}}))), None);
    }
}
