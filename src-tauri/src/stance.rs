//! What each prompt of yours did with what its agent had last said (with `[experimental]
//! analytics` on): set out new work, agreed, steered, redirected, or asked something. It's the
//! one part of turns.rs that's a read rather than a record, and it says of each prompt what read
//! it (`read_by`).
//!
//! What the records say for sure is read by rule, as a prompt is noted (`by_rule`): the first
//! thing said in a session sets out its work, a slash command is a command, and an answer that
//! picks what the agent offered agrees with it (its first choice, the one an agent recommends
//! when it recommends one) or steers it (another one). The rest is Jev's (`[jev] prompts`): one
//! choice question a prompt, with the end of the reply before it, asked by the ledger's thread
//! a few at a time, newest first (`pass`). An agent that read a conversation can say the same
//! (`codealong analytics tag`), and its word stands over Jev's, as a session's tag does over
//! Jev's read of it (tags.rs).
//!
//! The wording was tried on this machine's own prompts before anything was built on it:
//! `.claude/rules/jev.md` has how it did.

use anyhow::{bail, Result};
use rusqlite::{params, Connection};
use serde::Serialize;
use serde_json::{json, Value};

use crate::jev;
use crate::turns::{Came, Picked};

/// What a prompt can do, each as Jev is told it. `command` is never asked: a rule's alone.
pub const STANCES: [(&str, &str); 5] = [
    ("sets-out", "Tells the agent to do new work that isn't about its last reply: a new task, or the next thing now that the last one is finished."),
    ("agrees", "Goes along with what the agent said, proposed or asked: tells it to go ahead, to continue, to carry on or to try again, to do what it offered or recommended, says it looks good, or says the user has done what the agent asked of them. It adds no new requirement and changes nothing about the agent's plan."),
    ("steers", "Keeps what the agent did or proposed and adjusts it or adds to it: a further requirement or a constraint, a correction of a detail, a narrower or wider scope, a choice between options the agent gave, or a fact or a decision the agent needed."),
    ("redirects", "Rejects what the agent did or proposed and replaces it: says its approach or its result is wrong, tells it not to do what it proposed or to hold off, to undo it, or to do something different instead."),
    ("asks", "A question for the agent: asks for information, an explanation or its opinion, or asks to be shown something, about its last reply or about anything else. It tells the agent to change nothing."),
];

/// A slash command's stance: it runs something of the harness's or a skill's, and answers nothing.
pub const COMMAND: &str = "command";
/// `read_by` for what a rule read (`by_rule`), and for what Jev couldn't.
pub const BY_RULE: &str = "rule";
const UNREADABLE: &str = "unreadable";
/// Prompts read each pass, and how many requests are out at once.
const PER_PASS: usize = 48;
const AT_ONCE: usize = 4;

pub fn is_stance(word: &str) -> bool {
    word == COMMAND || STANCES.iter().any(|(s, _)| *s == word)
}

/// Whether a prompt is a slash command: `/name` or `/plugin:name`, alone or with arguments. Not a
/// path, which has more slashes in its first word.
pub fn is_command(text: &str) -> bool {
    let first = text.trim_start().split_whitespace().next().unwrap_or("");
    first.strip_prefix('/').is_some_and(|name| !name.is_empty() && name.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, ':' | '-' | '_')))
}

/// What the record itself says a prompt of yours did, where it does. `first`: nothing was said
/// in its session before it.
pub fn by_rule(c: &Came, first: bool) -> Option<&'static str> {
    if c.kind == "answer" {
        return match c.picked {
            Some(Picked::First) => Some("agrees"),
            Some(Picked::Offered) => Some("steers"),
            _ => None,
        };
    }
    // A slash command, or what a command you ran for the session printed (terminal.rs
    // `report_ended`), which Codealong sends in your name.
    if is_command(&c.text) || c.text.starts_with(crate::terminal::RAN_IT) {
        return Some(COMMAND);
    }
    first.then_some("sets-out")
}

/// A prompt waiting on a read.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Unread {
    /// Its row, as text: a number past what a page's numbers hold.
    pub id: String,
    pub at: i64,
    pub session: String,
    pub title: String,
    pub text: String,
    /// The end of what the agent had last said.
    pub before: String,
    /// You had stopped the agent's turn, or sent it while the agent worked.
    pub after_stop: bool,
    pub mid: bool,
    /// An answer to a question the agent asked, in words of your own.
    pub answer: bool,
}

/// The prompts of yours that wait on a read, the newest first.
pub fn unread(db: &Connection, limit: usize) -> Result<Vec<Unread>> {
    let mut stmt = db.prepare_cached(
        "SELECT t.id, t.at, s.key, coalesce(s.title, ''), t.text, coalesce(t.before, ''), t.after_stop, t.mid, t.kind
         FROM turns t JOIN sessions s ON s.id = t.session
         WHERE t.text IS NOT NULL AND t.stance IS NULL AND t.source = 'you' AND coalesce(t.counts_as, 'you') = 'you'
         ORDER BY t.at DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map([limit as i64], |r| {
        Ok(Unread {
            id: r.get::<_, i64>(0)?.to_string(),
            at: r.get(1)?,
            session: r.get(2)?,
            title: r.get(3)?,
            text: r.get(4)?,
            before: r.get(5)?,
            after_stop: r.get::<_, i64>(6)? != 0,
            mid: r.get::<_, i64>(7)? != 0,
            answer: r.get::<_, String>(8)? == "answer",
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// The request that asks Jev what one prompt did. What's known besides the two texts is said in
/// plain words, as Jev reads them literally: that the user had stopped the agent, that the agent
/// was still working, that the message answers a question the agent put.
pub fn request(model: &str, u: &Unread) -> Value {
    let mut state = serde_json::Map::new();
    if !u.before.trim().is_empty() {
        state.insert("end of the agent's last reply".into(), json!(u.before));
    }
    state.insert("user's message".into(), json!(u.text));
    let mut notes = Vec::new();
    if u.after_stop {
        notes.push("The user stopped the agent in the middle of its work just before sending this message.");
    } else if u.mid {
        notes.push("The user sent this message while the agent was still working, without waiting for it to finish.");
    }
    if u.answer {
        notes.push("The message is the user's answer, in their own words, to a question the agent asked.");
    }
    if !notes.is_empty() {
        state.insert("what happened".into(), json!(notes.join(" ")));
    }
    let instructions = if u.before.trim().is_empty() {
        "What does `user's message` do? Nothing the agent said before it is known."
    } else {
        "What does `user's message` do with what the agent said in `end of the agent's last reply`?"
    };
    jev::body(
        model,
        Value::Object(state),
        json!({ "stance": { "type": "choice", "instructions": instructions, "criteria": Value::Object(STANCES.iter().map(|(k, v)| (k.to_string(), json!(v))).collect()) } }),
    )
}

/// What an answer says a prompt did, and how likely.
pub fn read(response: &Value) -> Option<(String, f64)> {
    jev::choice(response, "stance").into_iter().next().filter(|(s, _)| is_stance(s))
}

/// Keeps Jev's read of a prompt, unless something read it meanwhile: an agent can tag a prompt
/// while Jev's answer for it is on its way, and the agent's word stands.
pub fn keep_read(db: &Connection, id: i64, stance: &str, sure: f64, model: &str) -> Result<bool> {
    if !is_stance(stance) {
        bail!("“{stance}” isn't something a prompt does.");
    }
    Ok(db.execute("UPDATE turns SET stance = ?2, stance_p = ?3, read_by = ?4 WHERE id = ?1 AND source = 'you' AND stance IS NULL", params![id, stance, sure, model])? > 0)
}

/// Says what a prompt did, over whatever read it before (an agent's tag). A stance no rule or
/// reader has is refused.
pub fn set(db: &Connection, id: i64, stance: &str, sure: Option<f64>, by: &str) -> Result<()> {
    if !is_stance(stance) {
        bail!("“{stance}” isn't something a prompt does. It's one of: {}, {COMMAND}.", STANCES.map(|(s, _)| s).join(", "));
    }
    let n = db.execute("UPDATE turns SET stance = ?2, stance_p = ?3, read_by = ?4 WHERE id = ?1 AND source = 'you'", params![id, stance, sure, by])?;
    if n == 0 {
        bail!("No prompt of yours has the id {id}.");
    }
    Ok(())
}

/// Reads a few of the prompts that wait on one, with Jev, where `[jev] prompts` is on and there's
/// a key: a few requests at a time, written as they come back. A request TypeSafe turns away for
/// now (rate limited, overloaded, no connection) ends the pass, and the next takes it up; one it
/// can't take at all leaves its prompt unread for good, saying so.
pub fn pass(db: &Connection) {
    let Ok(cfg) = crate::config::current() else { return };
    if !cfg.experimental.analytics || !cfg.jev.prompts || !jev::has_key() {
        return;
    }
    let waiting = match unread(db, PER_PASS) {
        Ok(w) => w,
        Err(e) => return eprintln!("Couldn't list the prompts waiting on a read: {e:#}"),
    };
    for batch in waiting.chunks(AT_ONCE) {
        let answers: Vec<Result<Value>> = std::thread::scope(|scope| {
            let asked: Vec<_> = batch.iter().map(|u| scope.spawn(|| jev::ask(jev::Feature::Prompts, &request(&cfg.jev.model, u)))).collect();
            asked.into_iter().map(|h| h.join().unwrap_or_else(|_| Err(anyhow::anyhow!("The request's thread stopped.")))).collect()
        });
        let mut later = false;
        for (u, answer) in batch.iter().zip(answers) {
            let Ok(id) = u.id.parse::<i64>() else { continue };
            let done = match answer.as_ref().map(read) {
                Ok(Some((stance, sure))) => keep_read(db, id, &stance, sure, answer.as_ref().ok().and_then(|a| a["model"].as_str()).unwrap_or(&cfg.jev.model)).map(|_| ()),
                // An answer with no stance in it, or a request Jev can't take: not asked again.
                Ok(None) => unreadable(db, id),
                Err(e) if refused(e) => unreadable(db, id),
                Err(_) => {
                    later = true;
                    Ok(())
                }
            };
            if let Err(e) = done {
                eprintln!("Couldn't keep the read of a prompt: {e:#}");
            }
        }
        if later {
            break;
        }
    }
}

/// Whether TypeSafe refused the request itself (too big, not something it takes), as against
/// turning it away for now.
fn refused(e: &anyhow::Error) -> bool {
    let words = e.to_string();
    words.contains("couldn't take")
}

fn unreadable(db: &Connection, id: i64) -> Result<()> {
    db.execute("UPDATE turns SET text = NULL, before = NULL, read_by = ?2 WHERE id = ?1 AND stance IS NULL", params![id, UNREADABLE])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::turns::Source;

    fn came(kind: &'static str, text: &str, picked: Option<Picked>) -> Came {
        Came { id: "x".into(), at: 1, source: Source::You, kind, sender: None, mid: false, text: text.into(), picked }
    }

    #[test]
    fn what_the_record_says_for_sure_is_read_by_rule() {
        assert_eq!(by_rule(&came("prompt", "Fix the parser", None), true), Some("sets-out"));
        assert_eq!(by_rule(&came("prompt", "Fix the parser", None), false), None);
        // A command is one wherever it comes; a path isn't a command.
        assert_eq!(by_rule(&came("prompt", "/code-review high", None), true), Some(COMMAND));
        assert_eq!(by_rule(&came("prompt", "/codealong:orchestrate tick", None), false), Some(COMMAND));
        assert_eq!(by_rule(&came("prompt", "/Users/me/projects/northwind-app/src is where it breaks", None), false), None);
        assert_eq!(by_rule(&came("prompt", "/ no", None), false), None);
        assert_eq!(by_rule(&came("prompt", "I ran this in my terminal:\n\n```\nnpm test\n```\n\nIt exited with status 0.", None), false), Some(COMMAND));
        // An answer that picks what was offered needs no read; words of your own do.
        assert_eq!(by_rule(&came("answer", "Postgres", Some(Picked::First)), false), Some("agrees"));
        assert_eq!(by_rule(&came("answer", "SQLite", Some(Picked::Offered)), false), Some("steers"));
        assert_eq!(by_rule(&came("answer", "Neither, keep the flat file", Some(Picked::Own)), false), None);
        assert!(is_stance("redirects") && is_stance(COMMAND) && !is_stance("approves"));
    }

    #[test]
    fn a_request_says_what_happened_in_plain_words() {
        let u = Unread { id: "7".into(), at: 1, session: "claude:claude:a".into(), title: "Fix the parser".into(), text: "No, keep the old tokenizer.".into(), before: "I'll rewrite the tokenizer.".into(), after_stop: true, mid: true, answer: false };
        let r = request("jev-preview", &u);
        assert_eq!(r["state"]["user's message"], "No, keep the old tokenizer.");
        assert_eq!(r["state"]["end of the agent's last reply"], "I'll rewrite the tokenizer.");
        assert_eq!(r["state"]["what happened"], "The user stopped the agent in the middle of its work just before sending this message.");
        assert_eq!(r["questions"]["stance"]["criteria"].as_object().unwrap().len(), 5);
        // With nothing said before it, the question doesn't name a reply.
        let alone = request("m", &Unread { before: String::new(), after_stop: false, mid: false, ..u.clone() });
        assert!(alone["state"].get("end of the agent's last reply").is_none() && alone["state"].get("what happened").is_none());
        assert!(alone["questions"]["stance"]["instructions"].as_str().unwrap().contains("Nothing the agent said"));
        // The likeliest stance is the read; an answer naming none isn't one.
        let answer = json!({"answers": {"stance": {"probabilities": {"redirects": 0.81, "steers": 0.17, "asks": 0.02}}}});
        assert_eq!(read(&answer), Some(("redirects".into(), 0.81)));
        assert_eq!(read(&json!({"answers": {}})), None);
    }

    #[test]
    fn jevs_read_never_goes_over_an_agents() {
        let db = crate::ledger::in_memory();
        let sid = crate::ledger::session_row(&db, "claude:claude:a", "claude", "").unwrap();
        db.execute("INSERT INTO turns (id, session, at, source, kind, text, counts_as) VALUES (7, ?1, 5, 'you', 'prompt', 'No, wrap it.', 'you')", [sid]).unwrap();
        // The agent's tag lands while Jev's answer is on its way.
        set(&db, 7, "redirects", None, "agent").unwrap();
        assert!(!keep_read(&db, 7, "steers", 0.6, "jev-1.14.0").unwrap());
        unreadable(&db, 7).unwrap();
        let (stance, by): (String, String) = db.query_row("SELECT stance, read_by FROM turns WHERE id = 7", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        assert_eq!((stance.as_str(), by.as_str()), ("redirects", "agent"));
        // One nothing has read takes Jev's.
        db.execute("INSERT INTO turns (id, session, at, source, kind, text, counts_as) VALUES (8, ?1, 6, 'you', 'prompt', 'Go ahead.', 'you')", [sid]).unwrap();
        assert!(keep_read(&db, 8, "agrees", 0.9, "jev-1.14.0").unwrap());
    }
}
