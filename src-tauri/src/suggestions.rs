//! Replies suggested for the sessions on this Mac (`codealong session suggest`): another session,
//! a supervisor that reads the sessions waiting on you, writes what you might answer each, and
//! the session's box shows them to send with a click (SessionReply.tsx). Nothing reaches the
//! session until you pick one. Kept in `suggestions.json` next to the config, by session key, until
//! something is sent to the session or a turn of its starts (`board::suggestions_hold`,
//! `Engine::forget_stale_suggestions`), and a week at most. The app writes it, and the command
//! does when the app isn't running, under the file's lock; the engine reads it again when it
//! changes (`current`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

use crate::config::support_dir;
use crate::model::Suggestions;
use crate::store;
use crate::util::DAY_MS;

/// What the file holds, for the error when it doesn't read as that.
const WHAT: &str = "a list of replies suggested for sessions";
/// Replies older than this are gone, whatever the session did.
const KEEP_MS: i64 = 7 * DAY_MS;
/// At most this many replies for a session: they're choices to take in at a glance.
pub const MOST: usize = 6;
/// At most this many characters in a reply, and in the note.
pub const LONGEST: usize = 4000;
pub const NOTE_LONGEST: usize = 600;

#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq)]
pub struct Store {
    /// By the key of the session they're for.
    #[serde(default)]
    pub sessions: BTreeMap<String, Suggestions>,
}

pub fn path() -> PathBuf {
    support_dir().join("suggestions.json")
}

/// The replies as given, each trimmed, the empty ones and repeats left out; an error says what's
/// too much.
pub fn tidy(replies: Vec<String>) -> Result<Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    for r in replies {
        let r = r.trim().to_string();
        if r.is_empty() || out.contains(&r) {
            continue;
        }
        if r.chars().count() > LONGEST {
            bail!("A suggested reply is at most {LONGEST} characters: it's what its user would send, not a briefing.");
        }
        out.push(r);
    }
    if out.len() > MOST {
        bail!("Suggest at most {MOST} replies, the best first.");
    }
    Ok(out)
}

/// The note as given, trimmed; none when it's empty. An error when it's too long.
pub fn tidy_note(note: Option<String>) -> Result<Option<String>> {
    let note = note.map(|n| n.trim().to_string()).filter(|n| !n.is_empty());
    if note.as_ref().is_some_and(|n| n.chars().count() > NOTE_LONGEST) {
        bail!("The note is at most {NOTE_LONGEST} characters: a sentence or two on why, or what to weigh.");
    }
    Ok(note)
}

/// Gives the session `key` these suggestions, in place of any it had, or with none takes its
/// away. Says whether that changed anything.
pub fn set(key: &str, suggested: Option<Suggestions>) -> Result<bool> {
    set_in(&path(), key, suggested, crate::util::now_ms())
}

fn set_in(path: &Path, key: &str, suggested: Option<Suggestions>, now: i64) -> Result<bool> {
    let mut changed = false;
    change_in(path, now, |s| {
        changed = match suggested {
            Some(g) => s.sessions.insert(key.to_string(), g.clone()).is_none_or(|old| old != g),
            None => s.sessions.remove(key).is_some(),
        };
        changed
    })?;
    Ok(changed)
}

/// Takes away the suggestions of these sessions, when they were suggested at the time given with
/// each, so that ones suggested again since stay. Says whether that changed anything.
pub fn forget(stale: &[(String, i64)]) -> Result<bool> {
    forget_in(&path(), stale, crate::util::now_ms())
}

fn forget_in(path: &Path, stale: &[(String, i64)], now: i64) -> Result<bool> {
    let mut changed = false;
    change_in(path, now, |s| {
        let before = s.sessions.len();
        s.sessions.retain(|key, g| !stale.iter().any(|(k, at)| k == key && *at == g.at));
        changed = s.sessions.len() != before;
        changed
    })?;
    Ok(changed)
}

/// Reads the file, changes it and writes it back in one step under its lock, leaving out what's
/// older than a week, unless `f` says nothing changed. A file that doesn't parse is an error and
/// is left as it is. The cache is dropped, so the board shows the change at once.
fn change_in(path: &Path, now: i64, f: impl FnOnce(&mut Store) -> bool) -> Result<()> {
    let done = store::change(path, WHAT, |s: &mut Store| {
        let changed = f(s);
        if changed {
            s.sessions.retain(|_, g| now - g.at < KEEP_MS);
        }
        changed
    });
    *CACHE.lock().unwrap_or_else(|e| e.into_inner()) = None;
    store::noted(path, done).map(|_| ())
}

static CACHE: Mutex<Option<((i64, u64), Arc<Store>)>> = Mutex::new(None);

/// The file as last read, read again when it changed on disk. One that doesn't parse shows as
/// none suggested.
pub fn current() -> Arc<Store> {
    let p = path();
    let stamp = std::fs::metadata(&p).ok().map(|m| (crate::util::mtime_ms(&p).unwrap_or(0), m.len())).unwrap_or((0, 0));
    let mut slot = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((at, store)) = slot.as_ref() {
        if *at == stamp {
            return store.clone();
        }
    }
    let store = Arc::new(if stamp == (0, 0) { Store::default() } else { store::read_or_default(&p) });
    *slot = Some((stamp, store.clone()));
    store
}

#[cfg(test)]
mod tests {
    use super::*;

    fn suggested(at: i64) -> Suggestions {
        Suggestions { replies: vec!["Yes, push it".into()], note: None, by: Some("claude:claude:sup".into()), by_title: Some("Supervisor".into()), at }
    }

    #[test]
    fn replies_are_tidied_and_held_to_a_few() {
        assert_eq!(tidy(vec![" Yes ".into(), "".into(), "Yes".into(), "No".into()]).unwrap(), vec!["Yes".to_string(), "No".to_string()]);
        assert!(tidy((0..7).map(|i| format!("Reply {i}")).collect()).unwrap_err().to_string().contains("at most 6"));
        assert!(tidy(vec!["x".repeat(LONGEST + 1)]).is_err());
        assert_eq!(tidy_note(Some("  ".into())).unwrap(), None);
        assert!(tidy_note(Some("x".repeat(NOTE_LONGEST + 1))).is_err());
    }

    #[test]
    fn suggestions_are_set_replaced_and_forgotten() {
        let dir = store::test_dir("suggestions");
        let p = dir.join("suggestions.json");
        let now = 10 * DAY_MS;
        assert!(set_in(&p, "claude:claude:a", Some(suggested(now)), now).unwrap());
        assert!(!set_in(&p, "claude:claude:a", Some(suggested(now)), now).unwrap(), "the same again changes nothing");
        assert!(set_in(&p, "codex:codex:b", Some(suggested(now - 1)), now).unwrap());
        let read: Store = store::read(&p, WHAT).unwrap().unwrap();
        assert_eq!(read.sessions.keys().collect::<Vec<_>>(), vec!["claude:claude:a", "codex:codex:b"]);
        // Forgetting what was suggested then leaves what's been suggested since.
        assert!(!forget_in(&p, &[("claude:claude:a".into(), now - 5)], now).unwrap());
        assert!(forget_in(&p, &[("claude:claude:a".into(), now)], now).unwrap());
        // None takes them away; a week on, the rest go with the next change.
        assert!(set_in(&p, "codex:codex:b", None, now).unwrap());
        assert!(!set_in(&p, "codex:codex:b", None, now).unwrap());
        set_in(&p, "claude:claude:old", Some(suggested(now - 8 * DAY_MS)), now - 8 * DAY_MS).unwrap();
        set_in(&p, "claude:claude:new", Some(suggested(now)), now).unwrap();
        let read: Store = store::read(&p, WHAT).unwrap().unwrap();
        assert_eq!(read.sessions.keys().collect::<Vec<_>>(), vec!["claude:claude:new"]);
        // A file that doesn't read is left as it is.
        std::fs::write(&p, "not json").unwrap();
        assert!(set_in(&p, "claude:claude:a", Some(suggested(now)), now).is_err());
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "not json");
    }
}
