//! Messages Claude Code holds for you to allow (2.1.292's code, checked in tmux 2026-10-07). A
//! session that skips permission prompts holds another session's message that doesn't claim
//! that class too, and one that asks holds a message that claims it skips them (peer.rs). It
//! writes a line to the transcript, shows "Held message from another session" in its terminal,
//! and its registry says `waiting` / `permission prompt` until you answer, busy or not: a turn
//! that runs goes on underneath it.
//!
//! The dialog lists "Deny — drop it and tell the sender it was declined" (focused first) and
//! "Deliver this message to Claude", without numbers, and digits do nothing there. Its choices
//! wrap, so End then Return delivers it wherever the focus is, and Escape denies it. Several held
//! messages queue, oldest first. Delivering writes "Released 1 held cross-session message to
//! Claude's queue (you approved it)." and then the message as another session's (`origin.kind:
//! "peer"`); denying writes nothing, and nor does the hold expiring (after the account's
//! `dialogExpiry`, five minutes unless it says otherwise, and never in a background job nobody is
//! attached to). So which of several held messages its terminal shows can't be read from its
//! files, and Codealong answers a held message only while it's the only one.
//!
//! The held line has only the start of the message. What Codealong sent itself, it keeps a day
//! (`held.json`), so the panel can show all of it before you deliver it.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::conversation::line_peer;
use super::transcript::you_typed;
use crate::util::{parse_rfc3339_ms, scan_lines_back};

/// How a held message's line starts (2.1.292): `Held peer message — from <address>[ [verified
/// pid N]][ (peer claims name: <name>)][; preview: «<first line>»<how much more>] — not delivered
/// to Claude (<N> held). <why>`.
const HELD: &str = "Held peer message — from ";
const NOT_DELIVERED: &str = " — not delivered to Claude (";
const CLAIMS: &str = " (peer claims name: ";
const PREVIEW: &str = "; preview: «";
/// What Claude Code writes as it lets held messages through: `Released <N> held cross-session
/// message[s] to Claude's queue (<why>).`, one at a time for yours.
const RELEASED: &str = "Released ";
const APPROVED: &str = "(you approved it)";
/// What it writes when a message you delivered went nowhere after all ("That held message was NOT
/// delivered…", "That held message was already resolved…").
const LOST: &str = "That held message was ";

/// A message a session holds for you to allow, from its transcript's line.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Held {
    /// The line's own uuid, which an answer names, so it's never taken for a later one.
    pub id: String,
    /// Who it's from, as a sentence says it: the name its sender gave, in quotes, else Claude
    /// Code's words for the sender ("an unidentified session").
    pub from: String,
    /// Its first line, as Claude Code shortens it.
    pub preview: String,
    /// How much there is of it when that's more than the preview: "3 lines, 1113 characters".
    pub more: Option<String>,
    /// How many messages it holds, this one and those before it still held.
    pub count: u32,
    pub at: i64,
}

/// What a transcript line says of held messages.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Hold {
    Held(Held),
    /// Messages let through or lost: `all` when every held one went (its permissions changed, or
    /// its settings now take such messages).
    Released { count: u32, all: bool },
    /// A prompt typed in its terminal, which no held message's dialog would have let through.
    Typed,
}

pub(super) fn hold_of(v: &Value) -> Option<Hold> {
    if you_typed(v) && line_peer(v).is_none() {
        return Some(Hold::Typed);
    }
    if v.get("type").and_then(Value::as_str) != Some("system") {
        return None;
    }
    let text = v.get("content").and_then(Value::as_str)?;
    if let Some(rest) = text.strip_prefix(HELD) {
        let (who, after) = rest.rsplit_once(NOT_DELIVERED)?;
        let count = after.split_once(" held)")?.0.trim().parse().ok()?;
        let (who, preview) = who.split_once(PREVIEW).map(|(w, p)| (w, Some(p))).unwrap_or((who, None));
        let (preview, more) = match preview.and_then(|p| p.split_once('»')) {
            Some((said, more)) => (said.trim().to_string(), more_of(more)),
            None => (String::new(), None),
        };
        let claimed = who.split_once(CLAIMS).and_then(|(_, n)| n.strip_suffix(')')).map(str::trim).filter(|n| !n.is_empty());
        let address = who.split(" [verified pid").next().unwrap_or(who).split(CLAIMS).next().unwrap_or(who).trim();
        let from = match claimed {
            Some(name) => crate::send::quoted(name),
            None if !address.is_empty() => address.to_string(),
            None => "another session".to_string(),
        };
        let at = v.get("timestamp").and_then(Value::as_str).and_then(parse_rfc3339_ms).unwrap_or(0);
        let id = v.get("uuid").and_then(Value::as_str).unwrap_or_default().to_string();
        return Some(Hold::Held(Held { id, from, preview, more, count, at }));
    }
    if let Some(rest) = text.strip_prefix(RELEASED) {
        let count = rest.split(' ').next()?.parse().ok()?;
        return rest.contains(" held cross-session ").then(|| Hold::Released { count, all: !rest.contains(APPROVED) });
    }
    text.starts_with(LOST).then_some(Hold::Released { count: 1, all: false })
}

/// "3 lines, 1113 characters" from ` …[3 lines, 1113 chars total — expand to review before
/// approving]`.
fn more_of(s: &str) -> Option<String> {
    let inside = s.split_once('[')?.1;
    let said = inside.split(" total").next()?.trim().replace(" chars", " characters");
    (!said.is_empty()).then_some(said)
}

/// The message held after `line`, given the one held before it.
pub(super) fn apply(held: &mut Option<Held>, hold: Hold) {
    match hold {
        Hold::Held(h) => *held = Some(h),
        Hold::Typed | Hold::Released { all: true, .. } => *held = None,
        Hold::Released { count, all: false } => {
            if let Some(h) = held {
                h.count = h.count.saturating_sub(count);
                if h.count == 0 {
                    *held = None;
                }
            }
        }
    }
}

/// The latest message a transcript holds for you, read back from its end: none once a line after
/// it says it went, and its count less those let through since. A turn can write megabytes
/// after it while the dialog waits, so this looks further back than a tail.
pub fn held_in(path: &Path) -> Option<Held> {
    let len = std::fs::metadata(path).ok()?.len();
    let mut released = 0;
    let mut found = None;
    scan_lines_back(path, len.saturating_sub(16 << 20), len, |line| {
        // Tool results are most of a transcript, and never one of these.
        if line.contains("\"tool_use_id\"") || !(line.contains("\"type\":\"system\"") || line.contains("\"type\":\"user\"")) {
            return false;
        }
        let Ok(v) = serde_json::from_str::<Value>(line) else { return false };
        match hold_of(&v) {
            Some(Hold::Held(h)) => {
                found = (h.count > released).then(|| Held { count: h.count - released, ..h });
                true
            }
            Some(Hold::Released { count, all: false }) => {
                released += count;
                false
            }
            Some(_) => true,
            None => false,
        }
    });
    found
}

/// What Codealong sent a session that it knew would be held, for the panel to show whole.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
struct Store {
    sent: Vec<Sent>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
struct Sent {
    key: String,
    at: i64,
    text: String,
}

const WHAT: &str = "the messages Codealong sent that wait to be allowed";
/// A held message waits for you hours at most: a day is plenty, and the file stays small.
const KEEP_MS: i64 = crate::util::DAY_MS;

pub fn path() -> PathBuf {
    crate::config::support_dir().join("held.json")
}

/// Keeps what was sent to the session `key`, which its session will hold.
pub fn note(key: &str, text: &str) {
    note_in(&path(), key, text, crate::util::now_ms());
}

fn note_in(path: &Path, key: &str, text: &str, now: i64) {
    let kept = crate::store::change(path, WHAT, |s: &mut Store| {
        s.sent.retain(|m| now - m.at < KEEP_MS);
        s.sent.push(Sent { key: key.to_string(), at: now, text: text.to_string() });
        true
    });
    // Only the panel's view of the message is lost: it then shows the first line Claude Code kept.
    if let Err(e) = kept {
        eprintln!("codealong: {e:#}");
    }
}

/// All of a held message, when Codealong sent it: the latest it sent the session `key` in the
/// minute before Claude Code held it, whose first line is the one Claude Code kept.
pub fn sent_text(key: &str, held: &Held) -> Option<String> {
    sent_text_in(&path(), key, held)
}

fn sent_text_in(path: &Path, key: &str, held: &Held) -> Option<String> {
    let store: Store = crate::store::read_or_default(path);
    let start = letters(held.preview.trim_end_matches('…'));
    store
        .sent
        .into_iter()
        .filter(|m| m.key == key && m.at <= held.at + 10_000 && held.at - m.at <= 60_000)
        .filter(|m| start.is_empty() || letters(m.text.lines().find(|l| !l.trim().is_empty()).unwrap_or("")).starts_with(&start))
        .max_by_key(|m| m.at)
        .map(|m| m.text)
}

/// Letters and digits alone, lowercased: Claude Code's preview drops quotes, brackets and the
/// like and shortens the line, so only these compare.
fn letters(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Lines as Claude Code 2.1.292 wrote them, from a scratch session in tmux.
    fn held_line(uuid: &str, ts: &str, name: &str, count: u32) -> String {
        serde_json::json!({
            "type": "system", "subtype": "informational", "level": "warning", "uuid": uuid, "timestamp": ts,
            "content": format!("Held peer message — from an unidentified session [verified pid 16413] (peer claims name: {name}); preview: «This message is from another agent, {name}:» …[4 lines, 110 chars total — expand to review before approving] — not delivered to Claude ({count} held). The sender did not attest its permission mode and this session bypasses prompts. Review it below, or set \"crossSessionInbound\" to \"accept\"."),
        })
        .to_string()
    }

    fn system(text: &str) -> String {
        serde_json::json!({ "type": "system", "subtype": "informational", "timestamp": "2026-10-07T16:34:00.518Z", "content": text }).to_string()
    }

    fn typed(text: &str) -> String {
        serde_json::json!({ "type": "user", "timestamp": "2026-10-07T16:40:00.000Z", "origin": { "kind": "human" }, "message": { "role": "user", "content": text } }).to_string()
    }

    fn file(name: &str, lines: &[String]) -> PathBuf {
        let dir = crate::store::test_dir(name);
        let path = dir.join("t.jsonl");
        std::fs::write(&path, lines.join("\n") + "\n").unwrap();
        path
    }

    #[test]
    fn reads_a_held_line() {
        let line = held_line("4aa38131", "2026-10-07T16:27:04.328Z", "Scratch sender one", 1);
        let Some(Hold::Held(h)) = hold_of(&serde_json::from_str(&line).unwrap()) else { panic!("not read") };
        assert_eq!(h.id, "4aa38131");
        assert_eq!(h.from, crate::send::quoted("Scratch sender one"));
        assert_eq!(h.preview, "This message is from another agent, Scratch sender one:");
        assert_eq!(h.more.as_deref(), Some("4 lines, 110 characters"));
        assert_eq!((h.count, h.at), (1, parse_rfc3339_ms("2026-10-07T16:27:04.328Z").unwrap()));
        // The conversation shows where it was held.
        let said = crate::claude::conversation_items(&serde_json::from_str(&line).unwrap(), 0);
        assert_eq!(said.iter().map(|i| (&*i.kind, &*i.text)).collect::<Vec<(&str, &str)>>(), [("note", "Held a message from “Scratch sender one” for you to allow")]);
        // A sender that gave no name is said as Claude Code says it.
        let bare = serde_json::json!({ "type": "system", "content": "Held peer message — from an unidentified session — not delivered to Claude (1 held). It is being reviewed before delivery." });
        let Some(Hold::Held(h)) = hold_of(&bare) else { panic!("not read") };
        assert_eq!((h.from.as_str(), h.preview.as_str(), h.more), ("an unidentified session", "", None));
    }

    #[test]
    fn a_held_message_stays_until_a_line_says_it_went() {
        let a = held_line("a", "2026-10-07T16:33:47.543Z", "Sender A", 1);
        let approved = system("Released 1 held cross-session message to Claude's queue (you approved it).");
        assert_eq!(held_in(&file("held-one", &[a.clone()])).map(|h| (h.id, h.count)), Some(("a".into(), 1)));
        assert_eq!(held_in(&file("held-approved", &[a.clone(), approved.clone()])), None);
        // A prompt typed in its terminal means no dialog was up, nor is one now.
        assert_eq!(held_in(&file("held-typed", &[a.clone(), typed("go on")])), None);
        // Its permissions changing lets every held one through.
        assert_eq!(held_in(&file("held-mode", &[a.clone(), system("Released 2 held cross-session messages to Claude's queue (permissions are prompting again).")])), None);
        // Two held: one delivered leaves one, the latest line's.
        let b = held_line("b", "2026-10-07T16:33:48.000Z", "Sender B", 2);
        assert_eq!(held_in(&file("held-two", &[a.clone(), b.clone()])).map(|h| (h.id, h.count)), Some(("b".into(), 2)));
        assert_eq!(held_in(&file("held-two-one", &[a.clone(), b.clone(), approved.clone()])).map(|h| (h.id, h.count)), Some(("b".into(), 1)));
        // Reading forward comes to the same.
        let mut held = None;
        for line in [&a, &b, &approved] {
            if let Some(h) = hold_of(&serde_json::from_str(line).unwrap()) {
                apply(&mut held, h);
            }
        }
        assert_eq!(held.map(|h| (h.id, h.count)), Some(("b".into(), 1)));
        // Another session's message delivered on its own is no answer to the dialog.
        let peer = serde_json::json!({ "type": "user", "isMeta": true, "origin": { "kind": "peer", "name": "Other", "body": "hi" }, "message": { "role": "user", "content": "<cross-session-message from-name=\"Other\">\nhi\n</cross-session-message>" } }).to_string();
        assert!(held_in(&file("held-peer", &[a, peer])).is_some());
    }

    #[test]
    fn keeps_what_it_sent_and_finds_it_by_the_first_line() {
        let dir = crate::store::test_dir("held-sent");
        let path = dir.join("held.json");
        let at = parse_rfc3339_ms("2026-10-07T16:27:04.328Z").unwrap();
        let text = "This message is from another agent, “Review 1021” (Codex):\n\nThe review is done.";
        note_in(&path, "claude:claude2:abc", "This message is from another agent, “Other”:\n\nhi", at - 2_000);
        note_in(&path, "claude:claude2:abc", text, at - 1_000);
        note_in(&path, "claude:claude2:xyz", text, at - 500);
        let held = Held { preview: "This message is from another agent, Review 1021 Codex:".into(), at, count: 1, ..Default::default() };
        assert_eq!(sent_text_in(&path, "claude:claude2:abc", &held).as_deref(), Some(text));
        // Not one sent long before, nor one whose first line isn't the one held.
        assert_eq!(sent_text_in(&path, "claude:claude2:abc", &Held { at: at + 120_000, ..held.clone() }), None);
        assert_eq!(sent_text_in(&path, "claude:claude2:abc", &Held { preview: "Something else".into(), ..held.clone() }), None);
        // What's a day old goes as the next is kept.
        note_in(&path, "claude:claude2:new", "later", at + KEEP_MS + 1);
        let store: Store = crate::store::read_or_default(&path);
        assert_eq!(store.sent.iter().map(|m| m.key.as_str()).collect::<Vec<_>>(), vec!["claude:claude2:new"]);
    }
}
