//! What a Claude Code session is writing, ahead of its record. With `[sessions] live_replies` on,
//! the Codealong plugin has a `MessageDisplay` hook (install.rs `LIVE_HOOK`), which Claude Code
//! hands each batch of a reply's finished lines as its terminal shows them; the hook passes them
//! to the app (`codealong session hook message-display`, server.rs), and the panel shows the
//! message at the end of the conversation until the record has it. Claude Code writes a reply to
//! its record only once the whole message is in, and the message a question comes in, with what
//! the agent wrote before it, often only once the question is answered
//! (.claude/rules/claude-code.md), so until then this is the one copy outside its terminal.
//!
//! Each batch's hook is a process of its own, so batches come in any order, a message's even
//! after the next one's has begun: a message's are kept by their index and read as far as they run
//! unbroken from the first, so the order they came in says nothing. What does: Claude Code writes
//! a message to its record before the next one can begin (a tool runs and the model is asked again
//! in between), so only one of a session's messages is ever unwritten, and that's the one shown.
//! A message goes once its record has it (a reply that begins with its text, after the start of
//! the record's newest page as its first batch came), or once a prompt is written after that
//! batch came (a turn cut off before the message was); its later batches are then let go. An
//! earlier reply in that page with the very same words takes it for written, and it shows only
//! once the record has it, as it would without the hook.
//!
//! Each hook's batch comes on a socket connection's thread of its own, so what's announced to the
//! windows is announced one at a time, each time what the session is writing then, and only when
//! it differs from what was announced last (`announce`): a thread that took its batch in before
//! another's can't announce the shorter text after the longer, and a read (`stream`) settles with
//! the record but leaves announcing to the batches.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde_json::Value;

use super::conversation::conversation_items;

struct Message {
    /// Claude Code's own ID for the message, the same on each of its batches.
    id: String,
    /// Each batch by its index.
    parts: BTreeMap<u64, String>,
    /// The session's record, and where it's read from next for the message: the start of its
    /// newest page as the first batch came, then where the last look ended. No line before it
    /// begins with the message's text, which only grows, so none is read twice.
    record: PathBuf,
    next: u64,
    /// The record's length as the first batch came: a prompt after it began another turn.
    seen_end: u64,
    /// When its first batch came: of two unwritten for a moment, the later one shows.
    since: i64,
}

impl Message {
    /// Its text as far as the batches run unbroken from the first.
    fn text(&self) -> String {
        let mut out = String::new();
        for (want, (i, part)) in self.parts.iter().enumerate() {
            if *i != want as u64 {
                break;
            }
            out.push_str(part);
        }
        out
    }
}

#[derive(Default)]
struct Live {
    messages: Vec<Message>,
    /// Messages the record has, the latest last, whose late batches are let go.
    written: Vec<String>,
    /// What the windows were last told it's writing (`announce`).
    announced: String,
    /// When its latest batch came, for keeping only so many sessions.
    at: i64,
}

/// By Codealong's session key: a session moved to another account keeps its ID there.
static LIVE: Mutex<BTreeMap<String, Live>> = Mutex::new(BTreeMap::new());

/// Sessions kept; past this the one quiet longest goes. A question can wait for hours, so it's
/// never by age alone.
const KEEP: usize = 200;
/// A session's messages kept at once, and the written ones remembered for their late batches.
const MESSAGES: usize = 8;
const WRITTEN: usize = 16;
/// The newest page of the record, in items, where a message's text is looked for.
const PAGE: usize = 40;

/// Held while what a session is writing is announced, so announcements go one at a time.
static ANNOUNCING: Mutex<()> = Mutex::new(());

fn live() -> std::sync::MutexGuard<'static, BTreeMap<String, Live>> {
    LIVE.lock().unwrap_or_else(|e| e.into_inner())
}

/// A batch as the hook was handed it, for the session `key`: `announce` is told what the session
/// is writing afterwards, empty for nothing, when that differs from what it was told last. A
/// subagent's batch (`agent_id`) is left out, since its record isn't the session's.
pub fn take(key: &str, hook: &Value, now: i64, announce: &dyn Fn(String)) {
    if took(key, hook, now).is_some() {
        announce_now(key, announce);
    }
}

/// Takes a batch in, unless it's a subagent's or a written message's.
fn took(key: &str, hook: &Value, now: i64) -> Option<()> {
    let s = |k: &str| hook.get(k).and_then(Value::as_str);
    if s("agent_id").is_some_and(|a| !a.is_empty()) {
        return None;
    }
    let (id, index, delta, record) = (s("message_id")?, hook.get("index")?.as_u64()?, s("delta").unwrap_or(""), s("transcript_path")?);
    let state = |all: &BTreeMap<String, Live>| all.get(key).map(|l| (l.written.iter().any(|w| w == id), l.messages.iter().any(|m| m.id == id)));
    let known = state(&live());
    if known.is_some_and(|(written, _)| written) {
        return None;
    }
    // Where to look for a new message's text is read off the lock: it reads the record.
    let page = known.is_none_or(|(_, kept)| !kept).then(|| newest_page(Path::new(record)));
    {
        let mut all = live();
        if state(&all).is_some_and(|(written, _)| written) {
            return None;
        }
        let l = all.entry(key.to_string()).or_default();
        if !l.messages.iter().any(|m| m.id == id) {
            // Another batch of it was taken in meanwhile, or this one reads the record.
            let (next, seen_end) = page?;
            l.messages.push(Message { id: id.to_string(), parts: BTreeMap::new(), record: PathBuf::from(record), next, seen_end, since: now });
            if l.messages.len() > MESSAGES {
                l.messages.remove(0);
            }
        }
        if let Some(m) = l.messages.iter_mut().find(|m| m.id == id) {
            m.parts.insert(index, delta.to_string());
        }
        l.at = now;
        if all.len() > KEEP {
            let quiet = all.iter().min_by_key(|(_, l)| l.at).map(|(k, _)| k.clone());
            all.remove(&quiet?);
        }
    }
    Some(())
}

/// Tells `announce` what the session is writing now, when that differs from what it was told
/// last: one announcement at a time, each settled with the record as it goes.
fn announce_now(key: &str, announce: &dyn Fn(String)) {
    let _one = ANNOUNCING.lock().unwrap_or_else(|e| e.into_inner());
    let Some(shown) = settle(key) else { return };
    {
        let mut all = live();
        let Some(l) = all.get_mut(key).filter(|l| l.announced != shown) else { return };
        l.announced = shown.clone();
    }
    announce(shown);
}

/// What the session is writing that its record doesn't have yet, if anything.
pub fn stream(key: &str) -> Option<String> {
    settle(key).filter(|s| !s.trim().is_empty())
}

/// Looks for each of the session's messages in its record, forgets the ones it has, and says
/// which is shown now: the unwritten one, empty for none.
fn settle(key: &str) -> Option<String> {
    let looks: Vec<(String, String, PathBuf, u64, u64)> = {
        let all = live();
        let l = all.get(key)?;
        l.messages.iter().map(|m| (m.id.clone(), m.text(), m.record.clone(), m.next, m.seen_end)).filter(|(_, text, ..)| !text.trim().is_empty()).collect()
    };
    let seen: Vec<(String, bool, u64)> = looks.into_iter().map(|(id, text, record, next, seen_end)| {
        let (written, next) = look(&record, next, seen_end, &text);
        (id, written, next)
    }).collect();
    let mut all = live();
    let l = all.get_mut(key)?;
    for (id, written, next) in seen {
        if written {
            l.messages.retain(|m| m.id != id);
            l.written.push(id);
            if l.written.len() > WRITTEN {
                l.written.remove(0);
            }
        } else if let Some(m) = l.messages.iter_mut().find(|m| m.id == id) {
            m.next = m.next.max(next);
        }
    }
    Some(l.messages.iter().filter(|m| !m.text().trim().is_empty()).max_by_key(|m| m.since).map(Message::text).unwrap_or_default())
}

/// Whether the record has a message by now, or has moved on past it: from `next` on, a reply that
/// begins with its text, or a prompt after `seen_end`. With where to read from next time.
fn look(record: &Path, next: u64, seen_end: u64, text: &str) -> (bool, u64) {
    let Ok(page) = crate::transcript::file_newer(record, conversation_items, next) else {
        // A record gone or unreadable has nothing to wait for.
        return (true, next);
    };
    let shown = words(text);
    let written = page.items.iter().any(|it| match it.kind {
        "agent" => words(&it.text).starts_with(&shown),
        "you" | "peer" => it.cursor >= seen_end,
        _ => false,
    });
    (written, page.after)
}

/// Where a new message's text is looked for in its record, the start of the newest page, and
/// the record's end. The message's own reply may be written already: Claude Code writes a message
/// within a moment of its end, and a batch's hook can take longer to come, even past the call's
/// result.
fn newest_page(record: &Path) -> (u64, u64) {
    match crate::transcript::file_page(record, conversation_items, None, PAGE) {
        Ok(page) => (page.items.first().map_or(page.after, |it| it.cursor), page.after),
        Err(_) => (0, 0),
    }
}

/// Text with its runs of whitespace as single spaces: the record trims a reply, and a batch
/// ends with the line breaks after it.
fn words(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Takes a batch in, and says what it announced, if anything.
    fn take(key: &str, hook: &Value, now: i64) -> Option<String> {
        let got = std::cell::RefCell::new(None);
        super::take(key, hook, now, &|text| *got.borrow_mut() = Some(text));
        got.into_inner()
    }

    fn batch(record: &Path, message: &str, index: u64, delta: &str, last: bool) -> Value {
        json!({ "hook_event_name": "MessageDisplay", "session_id": "s1", "transcript_path": record, "turn_id": "t1", "message_id": message, "index": index, "final": last, "delta": delta })
    }

    fn line(v: Value) -> String {
        format!("{v}\n")
    }

    fn prompt(text: &str) -> String {
        line(json!({ "type": "user", "timestamp": "2026-10-16T21:00:00Z", "message": { "content": text } }))
    }

    fn reply(text: &str) -> String {
        line(json!({ "type": "assistant", "timestamp": "2026-10-16T21:00:00Z", "message": { "stop_reason": "tool_use", "content": [{ "type": "text", "text": text }] } }))
    }

    fn result(id: &str) -> String {
        line(json!({ "type": "user", "timestamp": "2026-10-16T21:00:00Z", "message": { "content": [{ "type": "tool_result", "tool_use_id": id, "content": "ok" }] } }))
    }

    #[test]
    fn batches_show_in_order_until_the_record_has_the_message() {
        let dir = crate::fixtures::scratch("live");
        let record = dir.join("s1.jsonl");
        let before = prompt("Go") + &reply("Looking.") + &result("t0");
        std::fs::write(&record, &before).unwrap();
        let key = "claude:test-order:s1";
        // The second batch's hook ran first: nothing shows until the first is in.
        assert_eq!(take(key, &batch(&record, "m1", 1, "- two\n", false), 1), None);
        assert_eq!(stream(key), None);
        assert_eq!(take(key, &batch(&record, "m1", 0, "**One** thing:\n\n", false), 2).as_deref(), Some("**One** thing:\n\n- two\n"));
        assert_eq!(take(key, &batch(&record, "m1", 2, "Which next?", true), 3).as_deref(), Some("**One** thing:\n\n- two\nWhich next?"));
        // The same batch again changes nothing.
        assert_eq!(take(key, &batch(&record, "m1", 2, "Which next?", true), 4), None);
        assert_eq!(stream(key).as_deref(), Some("**One** thing:\n\n- two\nWhich next?"));
        // Another reply written after isn't it.
        std::fs::write(&record, before.clone() + &reply("Unrelated")).unwrap();
        assert!(stream(key).is_some());
        // Written, trimmed and with its line breaks as the record has them: it goes, and a late
        // batch of it doesn't bring it back.
        std::fs::write(&record, before + &reply("Unrelated") + &reply("**One** thing:\n\n- two\nWhich next?")).unwrap();
        assert_eq!(stream(key), None);
        assert_eq!(take(key, &batch(&record, "m1", 0, "**One** thing:\n\n", false), 5), None);
        assert_eq!(stream(key), None);
    }

    #[test]
    fn what_a_batch_pushes_is_settled_with_the_record_first() {
        let dir = crate::fixtures::scratch("live-push");
        let record = dir.join("s1.jsonl");
        std::fs::write(&record, prompt("Go")).unwrap();
        let key = "claude:test-push:s1";
        assert_eq!(take(key, &batch(&record, "m1", 0, "Working on it.\n", false), 1).as_deref(), Some("Working on it.\n"));
        // Written meanwhile: the next batch, the last of it, tells the windows it's nothing now.
        std::fs::write(&record, prompt("Go") + &reply("Working on it.\nDone.")).unwrap();
        assert_eq!(take(key, &batch(&record, "m1", 1, "Done.", true), 2).as_deref(), Some(""));
    }

    #[test]
    fn a_read_between_a_batch_and_its_announcement_leaves_it_to_announce() {
        let dir = crate::fixtures::scratch("live-read");
        let record = dir.join("s1.jsonl");
        std::fs::write(&record, prompt("Go")).unwrap();
        let key = "claude:test-read:s1";
        let said = std::cell::RefCell::new(Vec::new());
        let note = |text: String| said.borrow_mut().push(text);
        // A batch taken in, then read (a question's panel asking) before it's announced.
        took(key, &batch(&record, "m1", 0, "Before the question.\n", true), 1);
        assert_eq!(stream(key).as_deref(), Some("Before the question.\n"));
        announce_now(key, &note);
        assert_eq!(*said.borrow(), ["Before the question.\n"]);
        // Two batches taken in on their threads, the first one's announcement last: each
        // announces what it's writing then, so the shorter text never follows the longer.
        took(key, &batch(&record, "m2", 0, "Next, ", false), 2);
        took(key, &batch(&record, "m2", 1, "and more.\n", true), 3);
        announce_now(key, &note);
        announce_now(key, &note);
        assert_eq!(*said.borrow(), ["Before the question.\n", "Next, and more.\n"]);
    }

    #[test]
    fn the_next_message_takes_its_place_and_a_prompt_after_it_ends_it() {
        let dir = crate::fixtures::scratch("live-next");
        let record = dir.join("s1.jsonl");
        std::fs::write(&record, prompt("Go")).unwrap();
        let key = "claude:test-next:s1";
        take(key, &batch(&record, "m1", 0, "First.\n", true), 1);
        // Written, with its call's result, before the second begins.
        std::fs::write(&record, prompt("Go") + &reply("First.") + &result("t1")).unwrap();
        assert_eq!(take(key, &batch(&record, "m2", 0, "Second, so far.\n", false), 2).as_deref(), Some("Second, so far.\n"));
        assert_eq!(stream(key).as_deref(), Some("Second, so far.\n"));
        // A turn cut off before it was written, then a new prompt: it's not what the session is writing.
        std::fs::write(&record, prompt("Go") + &reply("First.") + &result("t1") + &prompt("Go on")).unwrap();
        assert_eq!(stream(key), None);
    }

    #[test]
    fn a_message_whose_batches_all_come_after_the_next_ones_doesnt_take_its_place() {
        let dir = crate::fixtures::scratch("live-late");
        let record = dir.join("s1.jsonl");
        // The first message was written, and its call answered, before the second began.
        std::fs::write(&record, prompt("Go") + &reply("First, on.") + &result("t1")).unwrap();
        let key = "claude:test-late:s1";
        assert_eq!(take(key, &batch(&record, "m2", 0, "Second, ", false), 1).as_deref(), Some("Second, "));
        // Every batch of the first comes after: it's written, so it's let go.
        assert_eq!(take(key, &batch(&record, "m1", 0, "First, ", false), 2), None);
        assert_eq!(take(key, &batch(&record, "m1", 1, "on.\n", true), 3), None);
        assert_eq!(take(key, &batch(&record, "m2", 1, "on.\n", true), 4).as_deref(), Some("Second, on.\n"));
        assert_eq!(stream(key).as_deref(), Some("Second, on.\n"));
        // A second batch of the first before its first: kept aside, shown never.
        assert_eq!(take(key, &batch(&record, "m0", 1, "zero.\n", true), 5), None);
        assert_eq!(stream(key).as_deref(), Some("Second, on.\n"));
    }

    #[test]
    fn a_message_written_before_its_first_batch_came_is_not_shown() {
        let dir = crate::fixtures::scratch("live-early");
        let record = dir.join("s1.jsonl");
        let key = "claude:test-early:s1";
        // A one-paragraph reply, written as it ended, and its call answered, before its hook
        // reached the app; then a reply to a new prompt, with the earlier one's hook later still.
        std::fs::write(&record, prompt("Go") + &reply("All done.") + &result("t1")).unwrap();
        assert_eq!(take(key, &batch(&record, "m1", 0, "All done.", true), 1), None);
        assert_eq!(stream(key), None);
        std::fs::write(&record, prompt("Go") + &reply("All done.") + &result("t1") + &prompt("Again") + &reply("Once more.")).unwrap();
        assert_eq!(take(key, &batch(&record, "m2", 0, "Once more.", true), 2), None);
        assert_eq!(stream(key), None);
    }

    #[test]
    fn a_subagents_batches_and_ones_without_a_message_are_left_out() {
        let dir = crate::fixtures::scratch("live-agent");
        let record = dir.join("s1.jsonl");
        std::fs::write(&record, "").unwrap();
        let key = "claude:test-agent:s1";
        let mut b = batch(&record, "m1", 0, "From a subagent.\n", true);
        b["agent_id"] = json!("a1");
        assert_eq!(take(key, &b, 1), None);
        assert_eq!(take(key, &json!({ "index": 0, "delta": "No message." }), 1), None);
        assert_eq!(stream(key), None);
        // A record that's gone has nothing to wait for.
        assert_eq!(take(key, &batch(&dir.join("gone.jsonl"), "m1", 0, "Lost.\n", true), 2), None);
        assert_eq!(stream(key), None);
    }
}
