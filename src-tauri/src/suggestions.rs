//! Replies suggested for the sessions on this Mac (`codealong session suggest`): another session,
//! a supervisor that reads the sessions waiting on you, writes what you might answer each, and
//! the session's box shows them to send with a click (SessionReply.tsx). Nothing reaches the
//! session until you pick one. Kept in `suggestions.json` next to the config, by session key, until
//! something is sent to the session or a turn of its starts (`board::suggestions_hold`,
//! `Engine::forget_stale_suggestions`), and a week at most. The app writes it, and the command
//! does when the app isn't running, under the file's lock; the engine reads it again when it
//! changes (`current`). For questions a session asks, the suggestion is choices of their own,
//! picked in its question box for you to answer with (`picked`), not replies.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

use codealong_peer::api::Question;

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

/// The choices `picks` name among the questions a session asks, by question, as its question box
/// shows them picked. A pick is a choice's label, or `<header>: <label>` (the question itself
/// before the colon will do) where more than one question offers it; case, a full stop and a
/// "(Recommended)" after it don't matter. An error says which pick doesn't name one choice, or
/// what picks too many for a question that takes one.
pub fn picked(questions: &[Question], picks: &[String]) -> Result<BTreeMap<String, Vec<String>>> {
    if questions.is_empty() {
        bail!("It asks no questions now, so there's no choice of its to pick: suggest replies instead.");
    }
    let all: Vec<&Question> = questions.iter().collect();
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for pick in picks.iter().map(|p| p.trim()).filter(|p| !p.is_empty()) {
        let mut found = matching(&all, pick);
        if found.is_empty() {
            if let Some((head, rest)) = pick.split_once(':') {
                let headed: Vec<&Question> = questions.iter().filter(|q| q.header.as_deref().is_some_and(|h| same(h, head)) || same(&q.question, head)).collect();
                found = matching(&headed, rest.trim());
            }
        }
        let (q, label) = match found.len() {
            1 => found.remove(0),
            0 => bail!("“{pick}” isn't one of its choices, which are {}.", choices(questions)),
            _ if found.iter().all(|(q, _)| q.question == found[0].0.question) => bail!("“{pick}” could be more than one of its choices: write the one you mean exactly."),
            _ => bail!("“{pick}” is a choice in more than one of its questions: write it as `<header>: {pick}`."),
        };
        let chosen = out.entry(q.question.clone()).or_default();
        if chosen.contains(&label) {
            continue;
        }
        if !q.multi_select && !chosen.is_empty() {
            bail!("“{}” takes one choice: pick “{}” or “{label}”.", named(q), chosen[0]);
        }
        chosen.push(label);
    }
    if out.is_empty() {
        bail!("Give a choice of its to pick.");
    }
    Ok(out)
}

/// The choices of these questions that `pick` names, with their questions: the ones it names
/// exactly, else the ones it names as an agent would write them (`same`).
fn matching<'a>(questions: &[&'a Question], pick: &str) -> Vec<(&'a Question, String)> {
    let with = |hit: &dyn Fn(&str) -> bool| questions.iter().flat_map(|q| q.options.iter().filter(|o| hit(&o.label)).map(move |o| (*q, o.label.clone()))).collect::<Vec<_>>();
    let exact = with(&|label| label.trim() == pick);
    if exact.is_empty() {
        with(&|label| same(label, pick))
    } else {
        exact
    }
}

/// A question by its header, else its text.
fn named(q: &Question) -> &str {
    q.header.as_deref().filter(|h| !h.trim().is_empty()).unwrap_or(&q.question)
}

/// The questions' choices, for an error: "“Yes”, “No”", or with several questions, each's header
/// before its own.
fn choices(questions: &[Question]) -> String {
    let of = |q: &Question| q.options.iter().map(|o| format!("“{}”", o.label)).collect::<Vec<_>>().join(", ");
    match questions {
        [q] => of(q),
        _ => questions.iter().map(|q| format!("{}: {}", named(q), of(q))).collect::<Vec<_>>().join("; "),
    }
}

/// Whether two labels are the same choice as an agent would write it.
fn same(a: &str, b: &str) -> bool {
    fn plain(s: &str) -> String {
        let s = s.trim().to_lowercase();
        let s = s.strip_suffix("(recommended)").unwrap_or(&s).trim_end();
        s.trim_end_matches('.').trim_end().to_string()
    }
    plain(a) == plain(b)
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
        Suggestions { replies: vec!["Yes, push it".into()], picks: BTreeMap::new(), note: None, by: Some("claude:claude:sup".into()), by_title: Some("Supervisor".into()), at, prompt_at: None }
    }

    fn asked(question: &str, header: &str, labels: &[&str], multi_select: bool) -> Question {
        let options = labels.iter().map(|l| codealong_peer::api::Choice { label: l.to_string(), description: None }).collect();
        Question { question: question.into(), header: Some(header.into()), options, multi_select }
    }

    #[test]
    fn picks_name_the_choices_of_the_questions_it_asks() {
        let scope = asked("How far should it go?", "Scope", &["Read, review and merge (Recommended)", "Read and review only", "Read only first"], false);
        let push = asked("Push it?", "Push", &["Yes", "No"], false);
        let checks = asked("Which checks?", "Checks", &["Lint", "Tests", "Yes"], true);
        let picks = |p: &[&str]| picked(&[scope.clone(), push.clone(), checks.clone()], &p.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        let want = |pairs: &[(&str, &[&str])]| pairs.iter().map(|(q, l)| (q.to_string(), l.iter().map(|s| s.to_string()).collect())).collect::<BTreeMap<String, Vec<String>>>();
        // By label, as an agent writes it: case, a full stop and "(Recommended)" don't matter.
        assert_eq!(picks(&["read and review only."]).unwrap(), want(&[("How far should it go?", &["Read and review only"])]));
        assert_eq!(picks(&["Read, review and merge"]).unwrap(), want(&[("How far should it go?", &["Read, review and merge (Recommended)"])]));
        // Several for a question that takes them, one for each of the others.
        assert_eq!(picks(&["Lint", "Tests", "No"]).unwrap(), want(&[("Push it?", &["No"]), ("Which checks?", &["Lint", "Tests"])]));
        // A label two questions offer is named by its question's header.
        assert!(picks(&["Yes"]).unwrap_err().to_string().contains("`<header>: Yes`"));
        assert_eq!(picks(&["Push: yes"]).unwrap(), want(&[("Push it?", &["Yes"])]));
        assert_eq!(picks(&["Which checks?: Yes"]).unwrap(), want(&[("Which checks?", &["Yes"])]));
        // What isn't a choice, two for a question that takes one, and nothing at all.
        assert!(picks(&["Merge it"]).unwrap_err().to_string().contains("Push: “Yes”, “No”"));
        assert!(picks(&["Push: No", "Push: Yes"]).unwrap_err().to_string().contains("“Push” takes one choice"));
        assert!(picks(&["Read only first", "Read and review only"]).unwrap_err().to_string().contains("“Scope” takes one choice"));
        assert!(picks(&[" "]).is_err());
        // A label written exactly is that one, whatever else it could be; one that's only close to
        // two isn't either.
        let files = asked("Which file?", "File", &["README.md", "readme.md"], false);
        let file = |p: &str| picked(std::slice::from_ref(&files), &[p.to_string()]);
        assert_eq!(file("readme.md").unwrap(), want(&[("Which file?", &["readme.md"])]));
        assert_eq!(file("File: README.md").unwrap(), want(&[("Which file?", &["README.md"])]));
        assert!(file("Readme.md").unwrap_err().to_string().contains("write the one you mean exactly"));
        assert!(picked(&[], &["Yes".into()]).unwrap_err().to_string().contains("asks no questions"));
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
