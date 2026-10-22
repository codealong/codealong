//! What you did with what you typed into the quick panel, beside what Jev suggested for it
//! (route.rs, src/route.ts), kept to measure how often Jev's suggestion is what you did:
//! `routes.json` next to the config, the latest `KEEP` of them. `--snapshot --routes [days]`
//! reports them (`report`).
//!
//! Jev is asked again each time typing pauses, so nothing is kept as it answers: only what it
//! was asked with is remembered for a while (`asked`), and a route is kept once you act on what
//! you typed. The panel says what it suggested, its rows the likeliest first, and what you did
//! and how you got to it (`note`); this adds what Jev was asked with, found by the text.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::config::support_dir;
use crate::store;
use crate::util::DAY_MS;

/// Routes kept, the latest last.
const KEEP: usize = 1000;
/// What Jev was asked with is remembered for this many of the latest asks.
const ASKS: usize = 50;
/// How much of what was typed is kept.
const TEXT_CHARS: usize = 1000;
/// What the file holds, for the error when it doesn't read as that.
const WHAT: &str = "a list of quick panel routes";

/// What a suggestion does, or what you did.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// What was typed went to a session.
    Send,
    /// A session was shown or opened.
    Open,
    /// A new session started with it.
    Start,
    /// It was kept as a todo.
    Todo,
    /// Something else the panel found: a todo, a PR.
    Other,
}

/// A suggestion, or what you did: what, and to which session (its key) or worktree (its lane id).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Choice {
    pub kind: Kind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    /// A start in a new worktree.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub new_worktree: bool,
    /// How likely Jev made a suggestion.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub p: Option<f64>,
}

/// How you got to what you did.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Via {
    /// One of Jev's rows.
    Suggested,
    /// The new session's row, or ⌘⏎.
    StartRow,
    /// Send to's list.
    SendTo,
    /// What the panel found by the words.
    Found,
}

/// What the panel says when you act on what you typed.
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Note {
    pub project: String,
    /// As the panel sent it to Jev.
    pub text: String,
    /// Jev's suggestions as the panel listed them, the likeliest first; none when you acted
    /// before Jev answered.
    pub suggested: Option<Vec<Choice>>,
    pub did: Choice,
    pub via: Via,
}

/// What Jev was asked with and said, as kept with a route.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Asked {
    pub model: String,
    pub ms: u64,
    /// The sessions that went with lines of their conversation sharing the text's words
    /// (route.rs `related`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related: Vec<String>,
}

/// One route, as kept.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Kept {
    pub at: i64,
    pub project: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggested: Option<Vec<Choice>>,
    pub did: Choice,
    pub via: Via,
    /// Where what you did was among the suggestions, 0 for the first: the same thing to the
    /// same session or worktree, and a start in a new worktree or not as it was suggested. None
    /// when it was none of them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rank: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asked: Option<Asked>,
}

#[derive(Serialize, Deserialize, Default, Debug, PartialEq)]
struct File {
    #[serde(default)]
    routes: Vec<Kept>,
}

pub fn path() -> PathBuf {
    support_dir().join("routes.json")
}

/// The latest asks, by project and text.
static ASKED: Mutex<VecDeque<(String, String, Asked)>> = Mutex::new(VecDeque::new());

/// Remembers what Jev was asked with for what was typed in a project, for when it's acted on.
pub fn asked(project: &str, text: &str, asked: Asked) {
    let mut asks = ASKED.lock().unwrap_or_else(|e| e.into_inner());
    asks.retain(|(p, t, _)| !(p == project && t == text));
    asks.push_back((project.to_string(), text.to_string(), asked));
    while asks.len() > ASKS {
        asks.pop_front();
    }
}

/// Keeps a route you acted on. A file that doesn't parse is an error and is left as it is.
pub fn note(n: Note, now: i64) -> Result<()> {
    let asked = n.suggested.as_ref().and_then(|_| ASKED.lock().unwrap_or_else(|e| e.into_inner()).iter().rev().find(|(p, t, _)| *p == n.project && *t == n.text).map(|(_, _, a)| a.clone()));
    // Nobody waits on this as the panel acts, so a failure is also a notice.
    store::noted(&path(), note_in(&path(), kept(n, asked, now)))
}

fn note_in(path: &Path, k: Kept) -> Result<()> {
    store::change(path, WHAT, |file: &mut File| {
        file.routes.push(k);
        let extra = file.routes.len().saturating_sub(KEEP);
        file.routes.drain(..extra);
        true
    })?;
    Ok(())
}

fn kept(n: Note, asked: Option<Asked>, now: i64) -> Kept {
    let rank = n.suggested.as_ref().and_then(|s| s.iter().position(|c| c.kind == n.did.kind && c.target == n.did.target && c.new_worktree == n.did.new_worktree));
    Kept { at: now, project: n.project, text: crate::util::truncate(&n.text, TEXT_CHARS), suggested: n.suggested, did: n.did, via: n.via, rank, asked }
}

/// How the routes kept since a time went.
#[derive(Serialize, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub routes: usize,
    /// Acted on before Jev answered.
    pub before_jev: usize,
    /// What you did was Jev's first suggestion.
    pub first: usize,
    /// It was another of them.
    pub other: usize,
    /// It was none of them.
    pub none: usize,
    /// Of those Jev answered, the share where its first suggestion was what you did.
    pub first_share: Option<f64>,
    /// The ones that weren't Jev's first, the latest first.
    pub misses: Vec<Kept>,
}

pub fn report(since: i64) -> Result<Report> {
    let file: File = store::read(&path(), WHAT)?.unwrap_or_default();
    Ok(report_of(&file.routes, since))
}

fn report_of(routes: &[Kept], since: i64) -> Report {
    let routes: Vec<&Kept> = routes.iter().filter(|k| k.at >= since).collect();
    let answered: Vec<&&Kept> = routes.iter().filter(|k| k.suggested.is_some()).collect();
    let first = answered.iter().filter(|k| k.rank == Some(0)).count();
    let other = answered.iter().filter(|k| k.rank.is_some_and(|r| r > 0)).count();
    Report {
        routes: routes.len(),
        before_jev: routes.len() - answered.len(),
        first,
        other,
        none: answered.len() - first - other,
        first_share: (!answered.is_empty()).then(|| first as f64 / answered.len() as f64),
        misses: answered.iter().rev().filter(|k| k.rank != Some(0)).map(|k| (**k).clone()).collect(),
    }
}

/// The time `days` ago.
pub fn since_days(days: u32, now: i64) -> i64 {
    now - days as i64 * DAY_MS
}

#[cfg(test)]
mod tests {
    use super::*;

    fn choice(kind: Kind, target: &str) -> Choice {
        Choice { kind, target: Some(target.into()), new_worktree: false, p: None }
    }

    fn note(suggested: Option<Vec<Choice>>, did: Choice) -> Note {
        Note { project: "Acme".into(), text: "fix the webhook retries".into(), suggested, did, via: Via::Suggested }
    }

    #[test]
    fn a_route_is_ranked_by_where_what_you_did_was_suggested() {
        let suggested = vec![choice(Kind::Send, "c:1"), choice(Kind::Open, "c:1"), choice(Kind::Start, "/p/api")];
        assert_eq!(kept(note(Some(suggested.clone()), choice(Kind::Send, "c:1")), None, 5).rank, Some(0));
        assert_eq!(kept(note(Some(suggested.clone()), choice(Kind::Start, "/p/api")), None, 5).rank, Some(2));
        // The right session for the wrong thing is a miss.
        assert_eq!(kept(note(Some(suggested.clone()), choice(Kind::Send, "c:2")), None, 5).rank, None);
        assert_eq!(kept(note(Some(suggested.clone()), choice(Kind::Todo, "/p/api")), None, 5).rank, None);
        // A start beside the worktree it was suggested in is another start, and the other way.
        let in_new = Choice { new_worktree: true, ..choice(Kind::Start, "/p/api") };
        assert_eq!(kept(note(Some(suggested), in_new.clone()), None, 5).rank, None);
        assert_eq!(kept(note(Some(vec![in_new.clone()]), choice(Kind::Start, "/p/api")), None, 5).rank, None);
        assert_eq!(kept(note(Some(vec![in_new.clone()]), in_new), None, 5).rank, Some(0));
        assert_eq!(kept(note(None, choice(Kind::Send, "c:1")), None, 5).rank, None);
    }

    #[test]
    fn keeps_the_latest_routes_with_what_jev_was_asked_with() {
        let dir = store::test_dir("routelog");
        let path = dir.join("routes.json");
        let one = Asked { model: "jev-1.14.0".into(), ms: 900, related: vec!["c:1".into()] };
        asked("Acme", "fix the webhook retries", one.clone());
        asked("Blog", "fix the webhook retries", Asked { model: "other".into(), ..one.clone() });
        let n = note(Some(vec![choice(Kind::Send, "c:1")]), choice(Kind::Send, "c:1"));
        let found = ASKED.lock().unwrap().iter().rev().find(|(p, t, _)| *p == n.project && *t == n.text).map(|(_, _, a)| a.clone());
        note_in(&path, kept(n, found, 7)).unwrap();
        let file: File = store::read(&path, WHAT).unwrap().unwrap();
        assert_eq!(file.routes[0].asked, Some(one), "the ask in its own project");
        for i in 0..KEEP {
            note_in(&path, kept(note(None, choice(Kind::Start, "/p/api")), None, 10 + i as i64)).unwrap();
        }
        let file: File = store::read(&path, WHAT).unwrap().unwrap();
        assert_eq!(file.routes.len(), KEEP);
        assert_eq!(file.routes[0].at, 10, "the oldest went");
    }

    #[test]
    fn reports_how_often_the_first_suggestion_was_it() {
        let at = |at: i64, suggested: Option<Vec<Choice>>, did: Choice| Kept { at, ..kept(note(suggested, did), None, at) };
        let s = || Some(vec![choice(Kind::Send, "c:1"), choice(Kind::Start, "/p/api")]);
        let routes = vec![
            at(1, s(), choice(Kind::Send, "c:1")),
            at(10, s(), choice(Kind::Send, "c:1")),
            at(11, s(), choice(Kind::Start, "/p/api")),
            at(12, s(), choice(Kind::Open, "c:9")),
            at(13, None, choice(Kind::Send, "c:1")),
            at(14, s(), choice(Kind::Send, "c:1")),
        ];
        let r = report_of(&routes, 10);
        assert_eq!((r.routes, r.before_jev, r.first, r.other, r.none), (5, 1, 2, 1, 1));
        assert_eq!(r.first_share, Some(0.5));
        assert_eq!(r.misses.iter().map(|k| k.at).collect::<Vec<_>>(), vec![12, 11], "the latest first");
        assert_eq!(report_of(&[], 0).first_share, None);
    }
}
