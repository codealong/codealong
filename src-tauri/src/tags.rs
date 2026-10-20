//! Tags an agent put on sessions (`codealong session tag`): an orchestrator that read a session's
//! conversation says where it stands in the words Jev's read uses (`SessionInsight`): whether its
//! last reply asks its user something, how far the work got, whether it's finished, how risky
//! what it waits on looks, what a command it left running does. The board shows a tag as it
//! shows Jev's read, the agent's fields over Jev's for the same moment and named by who tagged
//! it (`insights::merged`, `board::apply_insight`), whether or not `[jev] sessions` is on.
//!
//! Kept in `tags.json` next to the config, by session key, while the tag answers where the
//! session stands: until something is sent to it or a turn of its starts (`holds`, as replies
//! suggested for it are held), and a week at most. What moved a session on is kept in its place
//! (`Store::gone`), so `codealong changes` can say why a session it lists has no tag now. The app
//! writes it, and the command does when the app isn't running, under the file's lock; the engine
//! reads it again when it changes (`current`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

use crate::board::MovedOn;
use crate::config::support_dir;
use crate::insights::{BACKGROUND, LANDING};
use crate::model::{Session, SessionInsight};
use crate::store;
use crate::util::DAY_MS;

/// What the file holds, for the error when it doesn't read as that.
const WHAT: &str = "a list of tags agents put on sessions";
/// Tags older than this are gone, whatever the session did.
const KEEP_MS: i64 = 7 * DAY_MS;
/// At most this many characters in the note.
pub const NOTE_LONGEST: usize = 600;

/// One session's tag: the agent's read, when it was put on, and the session's last prompt as the
/// agent read it.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Tag {
    pub insight: SessionInsight,
    /// When it was put on, by the clock of the session's Mac.
    pub at: i64,
    /// The session's `last_prompt_at` as the agent read it: the tag is for the reply to that
    /// prompt, and goes once it's been sent another.
    pub prompt_at: i64,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq)]
pub struct Store {
    /// By the key of the session they're on.
    #[serde(default)]
    pub sessions: BTreeMap<String, Tag>,
    /// What moved on the sessions whose tags the engine took away (`forget`), by their keys,
    /// until they're tagged again, and a week at most.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub gone: BTreeMap<String, MovedOn>,
}

pub fn path() -> PathBuf {
    support_dir().join("tags.json")
}

/// Whether a tag still answers where its session stands: no prompt sent since the agent read it,
/// and no turn started since it was put on (`board::moved_on`).
pub fn holds(s: &Session, t: &Tag) -> bool {
    moved_on(s, t).is_none()
}

/// What moved its session on since the tag was put on, if anything did (`board::moved_on`).
pub fn moved_on(s: &Session, t: &Tag) -> Option<MovedOn> {
    crate::board::moved_on(s, t.prompt_at, t.at)
}

/// An agent's read as the request gives it, checked: each field as `SessionInsight` has it, by
/// name where it's a name, and the note trimmed and held short. None when it says nothing, which
/// takes a tag away. An error says what doesn't read.
pub fn read(p: &crate::actions::params::Tag) -> Result<Option<SessionInsight>> {
    let unit = |v: Option<f64>, what: &str| -> Result<Option<f64>> {
        match v {
            Some(x) if !(0.0..=1.0).contains(&x) => bail!("{what} is 0 (no) to 1 (yes), not {x}."),
            v => Ok(v),
        }
    };
    let named = |v: Option<String>, names: &[(&str, &str)], what: &str| -> Result<Option<String>> {
        match v.map(|v| v.trim().to_lowercase()).filter(|v| !v.is_empty()) {
            Some(v) if !names.iter().any(|(n, _)| *n == v) => bail!("{what} is one of {}, not “{v}”.", names.iter().map(|(n, _)| *n).collect::<Vec<_>>().join(", ")),
            v => Ok(v),
        }
    };
    let risk = match p.risk {
        Some(r) if !(0.0..=3.0).contains(&r) => bail!("risk is 0 (reads only) to 3 (destructive), not {r}."),
        r => r,
    };
    let note = p.note.as_deref().map(|n| n.split_whitespace().collect::<Vec<_>>().join(" ")).filter(|n| !n.is_empty());
    if note.as_ref().is_some_and(|n| n.chars().count() > NOTE_LONGEST) {
        bail!("The note is at most {NOTE_LONGEST} characters: a sentence or two on why.");
    }
    let background = named(p.background.clone(), &BACKGROUND, "background")?;
    let tag = SessionInsight {
        asks_you: unit(p.asks_you, "asks_you")?,
        landing: named(p.landing.clone(), &LANDING, "landing")?,
        finished: unit(p.finished, "finished")?,
        risk,
        background_p: background.as_ref().map(|_| 1.0),
        background,
        ..Default::default()
    };
    if tag == SessionInsight::default() {
        return Ok(None);
    }
    let filled = |v: &Option<String>| v.as_deref().map(str::trim).filter(|v| !v.is_empty()).map(String::from);
    Ok(Some(SessionInsight { by: filled(&p.by), by_title: filled(&p.by_title), note, ..tag }))
}

/// Puts this tag on the session `key`, in place of any it had, or with none takes its away, and
/// forgets what moved it on before. Says whether that changed anything.
pub fn set(key: &str, tag: Option<Tag>) -> Result<bool> {
    set_in(&path(), key, tag, crate::util::now_ms())
}

fn set_in(path: &Path, key: &str, tag: Option<Tag>, now: i64) -> Result<bool> {
    let mut changed = false;
    change_in(path, now, |s| {
        let moved = s.gone.remove(key).is_some();
        changed = match tag {
            Some(t) => s.sessions.insert(key.to_string(), t.clone()).is_none_or(|old| old != t),
            None => s.sessions.remove(key).is_some(),
        };
        changed || moved
    })?;
    Ok(changed)
}

/// Takes away the tags of these sessions, when they were put on at the time given with each, so
/// that ones put on again since stay, keeping what moved each session on in its place. Says
/// whether that changed anything.
pub fn forget(stale: &[(String, i64, MovedOn)]) -> Result<bool> {
    forget_in(&path(), stale, crate::util::now_ms())
}

fn forget_in(path: &Path, stale: &[(String, i64, MovedOn)], now: i64) -> Result<bool> {
    let mut changed = false;
    change_in(path, now, |s| {
        for (key, at, moved) in stale {
            if s.sessions.get(key).is_some_and(|t| t.at == *at) {
                s.sessions.remove(key);
                s.gone.insert(key.clone(), *moved);
                changed = true;
            }
        }
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
            s.sessions.retain(|_, t| now - t.at < KEEP_MS);
            s.gone.retain(|_, m| now - m.at() < KEEP_MS);
        }
        changed
    });
    *CACHE.lock().unwrap_or_else(|e| e.into_inner()) = None;
    store::noted(path, done).map(|_| ())
}

static CACHE: Mutex<Option<((i64, u64), Arc<Store>)>> = Mutex::new(None);

/// The file as last read, read again when it changed on disk. One that doesn't parse shows as
/// no tags.
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
    use crate::actions::params;
    use crate::model::{Harness, Status};

    fn tag(at: i64) -> Tag {
        Tag { insight: SessionInsight { finished: Some(1.0), by: Some("claude:claude:boss".into()), ..Default::default() }, at, prompt_at: at - 1000 }
    }

    #[test]
    fn reads_a_request_by_the_names_the_board_uses() {
        let p = params::Tag { landing: Some("Pushed".into()), finished: Some(1.0), risk: Some(2.0), background: Some("polling".into()), note: Some("  PR #12\n is up ".into()), by: Some("claude:claude:boss".into()), by_title: Some("Orchestrator".into()), ..Default::default() };
        let i = read(&p).unwrap().unwrap();
        assert_eq!((i.landing.as_deref(), i.finished, i.risk, i.background.as_deref(), i.background_p), (Some("pushed"), Some(1.0), Some(2.0), Some("polling"), Some(1.0)));
        assert_eq!((i.note.as_deref(), i.by.as_deref(), i.by_title.as_deref()), (Some("PR #12 is up"), Some("claude:claude:boss"), Some("Orchestrator")));
        assert_eq!(i.asks_you, None);
        // Nothing said takes the tag away; a note alone says nothing about the session.
        assert_eq!(read(&params::Tag::default()).unwrap(), None);
        assert_eq!(read(&params::Tag { note: Some("just a note".into()), ..Default::default() }).unwrap(), None);
        // What doesn't read.
        assert!(read(&params::Tag { landing: Some("merged".into()), ..Default::default() }).unwrap_err().to_string().contains("landing is one of landed, pushed"));
        assert!(read(&params::Tag { background: Some("daemon".into()), ..Default::default() }).unwrap_err().to_string().contains("background is one of"));
        assert!(read(&params::Tag { risk: Some(4.0), ..Default::default() }).unwrap_err().to_string().contains("risk is 0"));
        assert!(read(&params::Tag { asks_you: Some(1.5), ..Default::default() }).unwrap_err().to_string().contains("asks_you is 0"));
        assert!(read(&params::Tag { finished: Some(1.0), note: Some("x".repeat(NOTE_LONGEST + 1)), ..Default::default() }).is_err());
    }

    #[test]
    fn a_tag_holds_until_the_session_moves_on() {
        let t = tag(10_000);
        let s = Session { status: Status::Unread, last_prompt_at: 9_000, ..Session::new(Harness::Claude, "claude", "a") };
        assert!(holds(&s, &t));
        assert!(!holds(&Session { last_prompt_at: 9_500, ..s.clone() }, &t), "a prompt sent since it was read");
        assert!(!holds(&Session { status: Status::Working, status_since: Some(11_000), ..s.clone() }, &t), "a turn started since");
        assert!(holds(&Session { status: Status::Working, status_since: Some(9_900), ..s.clone() }, &t), "tagged mid-turn: the turn it was read in");
    }

    #[test]
    fn tags_are_set_replaced_and_forgotten() {
        let dir = store::test_dir("tags");
        let p = dir.join("tags.json");
        let now = 10 * DAY_MS;
        assert!(set_in(&p, "claude:claude:a", Some(tag(now)), now).unwrap());
        assert!(!set_in(&p, "claude:claude:a", Some(tag(now)), now).unwrap(), "the same again changes nothing");
        assert!(set_in(&p, "codex:codex:b", Some(tag(now - 1)), now).unwrap());
        let read: Store = store::read(&p, WHAT).unwrap().unwrap();
        assert_eq!(read.sessions.keys().collect::<Vec<_>>(), vec!["claude:claude:a", "codex:codex:b"]);
        // Forgetting what was tagged then leaves what's been tagged since, and keeps what moved
        // the session on until it's tagged again.
        let turn = MovedOn::Turn { at: now + 5 };
        assert!(!forget_in(&p, &[("claude:claude:a".into(), now - 5, turn)], now).unwrap());
        assert!(forget_in(&p, &[("claude:claude:a".into(), now, turn)], now).unwrap());
        let read: Store = store::read(&p, WHAT).unwrap().unwrap();
        assert_eq!((read.sessions.contains_key("claude:claude:a"), read.gone.get("claude:claude:a")), (false, Some(&turn)));
        assert!(!set_in(&p, "claude:claude:a", None, now).unwrap(), "it has no tag to take away, and what moved it on goes");
        let read: Store = store::read(&p, WHAT).unwrap().unwrap();
        assert!(read.gone.is_empty());
        // None takes it away; a week on, the rest go with the next change.
        assert!(set_in(&p, "codex:codex:b", None, now).unwrap());
        assert!(!set_in(&p, "codex:codex:b", None, now).unwrap());
        set_in(&p, "claude:claude:old", Some(tag(now - 8 * DAY_MS)), now - 8 * DAY_MS).unwrap();
        set_in(&p, "claude:claude:new", Some(tag(now)), now).unwrap();
        let read: Store = store::read(&p, WHAT).unwrap().unwrap();
        assert_eq!(read.sessions.keys().collect::<Vec<_>>(), vec!["claude:claude:new"]);
        // A file that doesn't read is left as it is.
        std::fs::write(&p, "not json").unwrap();
        assert!(set_in(&p, "claude:claude:a", Some(tag(now)), now).is_err());
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "not json");
    }
}
