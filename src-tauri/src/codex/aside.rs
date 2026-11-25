//! Questions a Codex thread's agent asks without waiting on them: `request_user_input_async`,
//! which Codex 0.169 gives the model in its Default mode where the model's catalog says so (GPT-6
//! here), rather than `request_user_input`, which it keeps for Plan mode (the
//! `default_mode_request_user_input` feature that would change that is "under development",
//! off). The tool answers `{"accepted":true}` at once and the agent goes on, ends its turn, or
//! sleeps in its turn waiting for an answer (seen 2026-11-24: `clock.sleep` a minute at a time).
//! The questions are an agent message of their own in the rollout, an `event_msg`
//! `item_completed` whose `AgentMessage` has `delivery: "async"` and `questions`, each a `title`
//! and maybe `options`: one of them, or words of your own, answers it (the tool's own words).
//! codex-acp 1.13.1 passes none of it on over ACP, so the rollout is where it's read.
//!
//! Your next message answers them, as Codex's own clients send it (its terminal UI's
//! `async_questions`, the desktop's): `<send_user_message_question_reply>`, then a JSON list of
//! `{answer, question, questionItemId}`, the id `["request_user_input_async", <item id>,
//! <index>]` as JSON, the question cut to 512 bytes on one line, then the closing tag. Its
//! terminal UI sends that into the running turn, and drops what's unanswered once the turn ends.
//!
//! A thread whose agent asked and that has had no message since waits on you
//! (board/sessions.rs `codex_session`). One Codealong runs is answered from the panel, into its
//! running turn or as its next (actions.rs `answer_here`); elsewhere Codex's own window or
//! terminal shows the questions, and answers them.

use serde_json::{json, Value};

use codealong_peer::api::{Choice, Pending, Question};

/// What the id of questions asked aside starts with, among what a session waits on.
pub const ASIDE_ID: &str = "aside-";

/// Codex's envelope around answers to questions asked aside (`context-fragments`
/// `AnsweredQuestion`).
pub const REPLY_OPEN: &str = "<send_user_message_question_reply>";
const REPLY_CLOSE: &str = "</send_user_message_question_reply>";

/// How much of a question an answer repeats, as Codex cuts it.
const QUESTION_BYTES: usize = 512;

/// Questions the agent asked without waiting on them.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Aside {
    /// The id of the agent message they came in: the tool call's.
    pub id: String,
    pub at: i64,
    pub questions: Vec<Question>,
}

/// The questions an `item_completed` event's payload asks aside, if it's an agent message of them.
pub fn of(payload: &Value, at: i64) -> Option<Aside> {
    let item = payload.get("item").filter(|i| i.get("type").and_then(Value::as_str) == Some("AgentMessage") && i.get("delivery").and_then(Value::as_str) == Some("async"))?;
    let questions: Vec<Question> = item
        .get("questions")?
        .as_array()?
        .iter()
        .filter_map(|q| {
            let title = q.get("title")?.as_str()?.trim();
            let options = q.get("options").and_then(Value::as_array).map(|o| o.iter().filter_map(Value::as_str).filter(|l| !l.trim().is_empty()).map(|l| Choice { label: l.to_string(), description: None }).collect()).unwrap_or_default();
            (!title.is_empty()).then(|| Question { question: title.to_string(), header: None, options, multi_select: false, form: None })
        })
        .collect();
    let id = item.get("id")?.as_str()?.to_string();
    (!questions.is_empty()).then_some(Aside { id, at, questions })
}

/// The questions as the panel shows them, `cannot` saying where they're answered instead when
/// it isn't here.
pub fn pending(a: &Aside, cannot: Option<String>) -> Pending {
    Pending { id: format!("{ASIDE_ID}{}", a.id), label: crate::board::asks_you(&a.questions[0].question), cannot, questions: a.questions.clone(), ..Default::default() }
}

/// The message that answers the questions with `answers` (by question: a choice or words of your
/// own, or a list of them, joined), in Codex's envelope, each answered question by its id. One
/// left unanswered is left out; none answered is refused. Declined (no `answers`), each is
/// answered with nothing: Codex's own clients send nothing for a question left to expire, but
/// an agent that sleeps in its turn waiting for an answer goes on only once a reply comes.
pub fn reply(a: &Aside, answers: Option<&Value>) -> anyhow::Result<String> {
    let replies: Vec<Value> = a
        .questions
        .iter()
        .enumerate()
        .filter_map(|(i, q)| {
            let answer = match answers.map(|a| a.get(&q.question)) {
                None => String::new(),
                Some(Some(Value::Array(list))) => list.iter().filter_map(Value::as_str).map(str::trim).filter(|t| !t.is_empty()).collect::<Vec<_>>().join(", "),
                Some(Some(Value::String(s))) => s.trim().to_string(),
                Some(_) => return None,
            };
            let cut = &q.question[..q.question.floor_char_boundary(q.question.len().min(QUESTION_BYTES))];
            let id = json!(["request_user_input_async", a.id, i]).to_string();
            (answers.is_none() || !answer.is_empty()).then(|| json!({ "answer": answer, "question": cut.replace(['\n', '\r'], " "), "questionItemId": id }))
        })
        .collect();
    if replies.is_empty() {
        anyhow::bail!("Pick an answer first.");
    }
    Ok(format!("{REPLY_OPEN}\n{}\n{REPLY_CLOSE}", Value::Array(replies)))
}

/// A reply in Codex's envelope as the conversation shows it, each question quoted with its
/// answer under it, as Codex's terminal UI shows one (`async_question_reply`); None for a
/// message that isn't one.
pub fn shown(text: &str) -> Option<String> {
    let json = text.trim().strip_prefix(REPLY_OPEN)?.strip_suffix(REPLY_CLOSE)?;
    let replies = match serde_json::from_str::<Value>(json).ok()? {
        Value::Array(list) => list,
        one @ Value::Object(_) => vec![one],
        _ => return None,
    };
    let parts: Vec<String> = replies
        .iter()
        .filter_map(|r| {
            let answer = r.get("answer")?.as_str()?;
            Some(match r.get("question").and_then(Value::as_str) {
                Some(q) => format!("> {q}\n\n{answer}"),
                None => answer.to_string(),
            })
        })
        .collect();
    (!parts.is_empty()).then(|| parts.join("\n\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The questions as Codex 0.169 recorded them (2026-11-24), and the answer its clients send.
    #[test]
    fn questions_asked_aside_are_answered_as_codex_takes_them() {
        let payload = json!({ "type": "item_completed", "item": {
            "type": "AgentMessage", "id": "call_5a89", "phase": "final_answer", "delivery": "async",
            "content": [{ "type": "Text", "text": "Which color?\n- Red\n- Blue\n\nWhich days?\n- Mon\n- Wed" }],
            "questions": [{ "title": "Which color?", "options": ["Red", "Blue"] }, { "title": "Anything else?\nSay so." }],
        } });
        let a = of(&payload, 7).unwrap();
        assert_eq!((a.id.as_str(), a.at), ("call_5a89", 7));
        let p = pending(&a, None);
        assert_eq!((p.id.as_str(), p.label.as_str()), ("aside-call_5a89", "Asks you · Which color?"));
        assert_eq!(p.questions[0].options.iter().map(|o| o.label.as_str()).collect::<Vec<_>>(), ["Red", "Blue"]);
        assert!(p.questions[1].options.is_empty());
        let text = reply(&a, Some(&json!({ "Which color?": "Blue", "Anything else?\nSay so.": "Ship it today" }))).unwrap();
        assert_eq!(
            text,
            "<send_user_message_question_reply>\n[{\"answer\":\"Blue\",\"question\":\"Which color?\",\"questionItemId\":\"[\\\"request_user_input_async\\\",\\\"call_5a89\\\",0]\"},{\"answer\":\"Ship it today\",\"question\":\"Anything else? Say so.\",\"questionItemId\":\"[\\\"request_user_input_async\\\",\\\"call_5a89\\\",1]\"}]\n</send_user_message_question_reply>"
        );
        // What the conversation shows of it, and of a reply the desktop sent with no question.
        assert_eq!(shown(&text).unwrap(), "> Which color?\n\nBlue\n\n> Anything else? Say so.\n\nShip it today");
        assert_eq!(shown("<send_user_message_question_reply>\n[{\"answer\":\"Yes\"}]\n</send_user_message_question_reply>").unwrap(), "Yes");
        assert_eq!(shown("Use Postgres"), None);
        assert!(reply(&a, Some(&json!({}))).is_err());
        // Declined, each is answered with nothing.
        let declined = reply(&a, None).unwrap();
        assert!(declined.contains(r#"{"answer":"","question":"Which color?""#) && declined.contains(r#""answer":"","question":"Anything else? Say so.""#), "{declined}");
        // Any other agent message asks nothing.
        assert!(of(&json!({ "type": "item_completed", "item": { "type": "AgentMessage", "id": "m", "phase": "final_answer", "content": [] } }), 0).is_none());
    }
}
