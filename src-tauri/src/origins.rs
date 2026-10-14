//! Where sessions came from, as Codealong saw it happen: a session whose agent ran `codealong
//! start` (with `--helper`, a helper of it: `Session::helper`, told of with `--report`), and a
//! fork made from Codealong. The harnesses write most forks down themselves (claude/registry.rs's
//! `Forks`, Codex's `forked_from_id`, OpenCode's `fork_session_id`), but nothing records which
//! session ran the command that started another, nor Claude Code's fork into a
//! terminal (`--resume --fork-session`), whose transcript doesn't say. Kept in `origins.json`
//! next to the config; the engine reads it again when it changes (`current`).

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::config::support_dir;
use crate::model::{OriginHow, SessionOrigin};
use crate::store;
use crate::todos::Lanes;

/// How long a start whose session wasn't known waits for the next new session of its account in
/// its worktree.
const PENDING_MS: i64 = 30 * 60 * 1000;
/// How long after that a start with a prompt still waits for a session that opens with it. A
/// terminal UI can stop on a screen of its own before it makes its session, until you answer it:
/// a Codex helper started in a terminal sat on one for 66 minutes and was never its session's
/// (2026-10-14; codex.md has the screens). The prompt says it's the one, however late.
const PROMPTED_MS: i64 = 24 * 60 * 60 * 1000;
/// A session that says it started a little before the start was asked for can still be it.
const PENDING_SLACK_MS: i64 = 60 * 1000;
/// Entries are kept this long. Claude Code deletes transcripts after about 30 days.
const KEEP_MS: i64 = 90 * 24 * 60 * 60 * 1000;

/// What the file holds, for the error when it doesn't read as that.
const WHAT: &str = "a list of where sessions came from";

#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq)]
pub struct Store {
    /// By the key of the session that came from another.
    #[serde(default)]
    pub sessions: BTreeMap<String, Entry>,
    /// Starts whose session's key wasn't known as they started (an app or OpenCode picks it),
    /// waiting for the first new session of theirs to show up (`resolve`).
    #[serde(default)]
    pub pending: Vec<Pending>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Entry {
    pub from: String,
    pub how: OriginHow,
    pub at: i64,
    /// Started as a helper of `from` (`codealong start --helper`): it works for that session's
    /// agent, not for you.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub helper: bool,
    /// A helper whose session is told when its turn ends or it waits on its user
    /// (`codealong start --helper --report`, helpers.rs).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub report: bool,
    /// The start that made it said which session it made, so this is that session for sure;
    /// a pending start's match (`resolve`) is only likely. A helper's messages to its session
    /// skip that session's hold only when it is (send.rs `from_its_helper`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub known: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Pending {
    pub from: String,
    pub how: OriginHow,
    /// The worktree it started in.
    pub lane: String,
    pub account: String,
    pub at: i64,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub helper: bool,
    /// A helper whose session is told when its turn ends or it waits on its user
    /// (`codealong start --helper --report`, helpers.rs).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub report: bool,
    /// How its prompt opens, as a harness records a prompt (`opening`), when it had one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opens: Option<String>,
}

impl Pending {
    /// How long it waits: a start with a prompt a day, for a session that opens with it.
    fn waits(&self) -> i64 {
        if self.opens.is_some() { PROMPTED_MS } else { PENDING_MS }
    }
}

/// How a prompt opens, as the harnesses' readers keep one (`Session::first_prompt`,
/// `last_prompt`): its first line, cut short.
pub fn opening(prompt: &str) -> Option<String> {
    Some(crate::util::one_line(prompt, 140)).filter(|o| !o.is_empty())
}

impl Store {
    /// Where the session with this key came from.
    pub fn origin_of(&self, key: &str) -> Option<SessionOrigin> {
        self.sessions.get(key).map(|e| SessionOrigin { key: e.from.clone(), how: e.how })
    }

    /// The session the session with this key is a helper of, when it was started as one, and
    /// whether that session is told what it does.
    pub fn helper_of(&self, key: &str) -> Option<(&str, bool)> {
        self.sessions.get(key).filter(|e| e.helper).map(|e| (e.from.as_str(), e.report))
    }

    /// The session the session with this key was started as a helper of, when the start that made
    /// it said which session it made (`Entry::known`), not when it was only matched later. Made
    /// your own since, it's still noted here; the board says it helps no more.
    pub fn known_helper_of(&self, key: &str) -> Option<&str> {
        self.sessions.get(key).filter(|e| e.helper && e.known).map(|e| e.from.as_str())
    }

    /// The session whose agent started the session with this key (`codealong start`, a helper or
    /// not), when the start said which session it made (`Entry::known`). A session that isn't
    /// running is resumed for a message from one it started this way (send.rs `wakes`).
    pub fn known_starter_of(&self, key: &str) -> Option<&str> {
        self.sessions.get(key).filter(|e| e.how == OriginHow::Started && e.known).map(|e| e.from.as_str())
    }

    /// The session `key` moved to (start.rs `move_to`): a fork of it into another account of its
    /// harness that `exists` says is still there, followed on where that one moved too. A copy
    /// closed before it was sent anything is still there, made again when it's next sent
    /// something (`claude::Reader::closed_copies`); one whose conversation is gone from this Mac
    /// isn't, and the session stays where it was.
    pub fn moved_to(&self, key: &str, exists: impl Fn(&str) -> bool) -> Option<String> {
        let mut at = key.to_string();
        // Moved back again, a key comes round a second time.
        let mut seen = HashSet::from([at.clone()]);
        loop {
            let next = self.sessions.iter().filter(|(k, e)| e.from == at && e.how == OriginHow::Fork && is_move(&at, k) && !seen.contains(*k) && exists(k)).max_by_key(|(_, e)| e.at);
            let Some((next, _)) = next else { break };
            seen.insert(next.clone());
            at = next.clone();
        }
        (at != key).then_some(at)
    }

    fn prune(&mut self, now: i64) {
        self.sessions.retain(|_, e| now - e.at < KEEP_MS);
        self.pending.retain(|p| now - p.at < p.waits());
    }
}

/// Whether a fork from session `from` to `to` is a move: into another account of the same
/// harness. The windows tell it the same way (lineage.ts `kinHow`).
pub fn is_move(from: &str, to: &str) -> bool {
    let whose = |key: &str| {
        let mut parts = key.splitn(3, ':');
        (parts.next().unwrap_or_default().to_string(), parts.next().unwrap_or_default().to_string())
    };
    let ((from_harness, from_account), (to_harness, to_account)) = (whose(from), whose(to));
    from_harness == to_harness && from_account != to_account
}

/// A session on the board, as pending starts are matched against it.
pub struct Candidate<'a> {
    pub key: &'a str,
    /// The lane it's listed under as its own.
    pub lane: &'a str,
    pub account: &'a str,
    /// When it started, or its first activity.
    pub started: i64,
    /// Its first prompt, else its latest, as its harness's reader keeps it.
    pub prompt: Option<&'a str>,
}

/// Gives each pending start the first session of its account that started after it, in its
/// worktree, else in another worktree of the same repo (the apps can make their own), that came
/// from nowhere yet: one that opens with its prompt first, and after `PENDING_MS` only such a
/// one. Returns whether anything changed.
pub fn resolve(store: &mut Store, sessions: &[Candidate], lanes: &dyn Lanes, now: i64) -> bool {
    if store.pending.is_empty() {
        return false;
    }
    let mut changed = false;
    let mut taken: HashSet<String> = store.sessions.keys().cloned().collect();
    let mut waiting = Vec::new();
    for p in std::mem::take(&mut store.pending) {
        if now - p.at > p.waits() {
            changed = true;
            continue;
        }
        let repo = lanes.repo_of(&p.lane);
        let opens = |s: &Candidate| p.opens.is_some() && s.prompt.and_then(opening) == p.opens;
        let late = now - p.at > PENDING_MS;
        let pick = sessions
            .iter()
            .filter(|s| s.account == p.account && s.started >= p.at - PENDING_SLACK_MS && s.key != p.from && !taken.contains(s.key))
            .filter(|s| s.lane == p.lane || (repo.is_some() && lanes.repo_of(s.lane) == repo))
            .filter(|s| !late || opens(s))
            .min_by_key(|s| (!opens(s), s.lane != p.lane, s.started));
        match pick {
            Some(s) => {
                taken.insert(s.key.to_string());
                store.sessions.insert(s.key.to_string(), Entry { from: p.from, how: p.how, at: p.at, helper: p.helper, report: p.report, known: false });
                changed = true;
            }
            None => waiting.push(p),
        }
    }
    store.pending = waiting;
    changed
}

pub fn path() -> PathBuf {
    support_dir().join("origins.json")
}

/// Reads the file, changes it and writes it back in one step, unless `f` says nothing changed.
/// Under the file's own lock (`origins.json.lock`), since the `codealong` command notes starts
/// as the app does. A file that doesn't parse is an error and is left as it is: an older build
/// that doesn't know a newer one's way of starting must not write an empty list over it. In
/// the app nobody waits on these, so a failure is also a notice (`store::noted`).
fn change(f: impl FnOnce(&mut Store) -> bool) -> Result<()> {
    let p = path();
    store::noted(&p, change_in(&p, f))
}

fn change_in(path: &Path, f: impl FnOnce(&mut Store) -> bool) -> Result<()> {
    store::change(path, WHAT, |s: &mut Store| {
        let changed = f(s);
        if changed {
            s.prune(crate::util::now_ms());
        }
        changed
    })?;
    Ok(())
}

/// Notes that the session `key` came from `from`. A session never comes from itself.
pub fn record(key: &str, from: &str, how: OriginHow) {
    if let Err(e) = write_as(key, from, how, false, false) {
        eprintln!("Couldn't note where {key} came from: {e:#}");
    }
}

/// `record`, saying whether it was written: a move's copy is known by this note until its
/// first message, so the move doesn't go ahead without it.
pub fn write(key: &str, from: &str, how: OriginHow) -> Result<()> {
    write_as(key, from, how, false, false)
}

/// `write`, noting whether it's a helper of `from`, and whether `from` is told what it does.
fn write_as(key: &str, from: &str, how: OriginHow, helper: bool, report: bool) -> Result<()> {
    if key == from || key.is_empty() || from.is_empty() {
        return Ok(());
    }
    let now = crate::util::now_ms();
    change(|s| {
        let entry = Entry { from: from.to_string(), how, at: now, helper, report, known: true };
        s.sessions.insert(key.to_string(), entry).is_none_or(|old| old.from != from || old.how != how || old.helper != helper || old.report != report || !old.known)
    })
}

/// Takes back the note of where `key` came from: a move whose copy didn't start.
pub fn forget(key: &str) -> Result<()> {
    change(|s| s.sessions.remove(key).is_some())
}

/// Notes a start from `from` whose session isn't known yet: the next one of `account` in `lane`,
/// or a later one that opens with `prompt`.
pub fn record_pending(from: &str, how: OriginHow, lane: &str, account: &str, prompt: &str, helper: bool, report: bool) {
    let p = Pending { from: from.to_string(), how, lane: lane.to_string(), account: account.to_string(), at: crate::util::now_ms(), helper, report, opens: opening(prompt) };
    if let Err(e) = change(|s| {
        s.pending.push(p);
        true
    }) {
        eprintln!("Couldn't note a start from {from}: {e:#}");
    }
}

/// Notes where a start came from: the session that asked for it (`StartSpec::from`), by the new
/// session's key when the start knew it, else by the next session of its account in its worktree,
/// or one that opens with its `prompt`. `helper`: it was started as a helper of that session
/// (`StartSpec::helper`), which `report` says is told what it does.
pub fn started(from: Option<&str>, key: Option<&str>, lane: &str, account: &str, prompt: &str, helper: bool, report: bool) {
    let Some(from) = from.filter(|f| !f.is_empty()) else { return };
    match key {
        Some(key) => {
            if let Err(e) = write_as(key, from, OriginHow::Started, helper, report) {
                eprintln!("Couldn't note where {key} came from: {e:#}");
            }
        }
        None => record_pending(from, OriginHow::Started, lane, account, prompt, helper, report),
    }
}

/// Matches pending starts to the sessions on the board and writes down what matched.
pub fn resolve_pending(sessions: &[Candidate], lanes: &dyn Lanes, now: i64) -> bool {
    if current().pending.is_empty() {
        return false;
    }
    let mut changed = false;
    if let Err(e) = change(|s| {
        changed = resolve(s, sessions, lanes, now);
        changed
    }) {
        eprintln!("Couldn't note where new sessions came from: {e:#}");
    }
    changed
}

/// The file as last read, read again when it changed on disk. One that doesn't parse shows
/// as no origins; only a change to it is turned away.
pub fn current() -> Arc<Store> {
    static CACHE: Mutex<Option<((i64, u64), Arc<Store>)>> = Mutex::new(None);
    let stamp = std::fs::metadata(path()).ok().map(|m| (crate::util::mtime_ms(&path()).unwrap_or(0), m.len())).unwrap_or((0, 0));
    let mut slot = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((at, store)) = slot.as_ref() {
        if *at == stamp {
            return store.clone();
        }
    }
    let store = Arc::new(if stamp == (0, 0) { Store::default() } else { store::read_or_default(&path()) });
    *slot = Some((stamp, store.clone()));
    store
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Repos;
    impl Lanes for Repos {
        fn project_of(&self, _: &str) -> Option<String> {
            None
        }
        fn repo_of(&self, lane: &str) -> Option<String> {
            lane.strip_prefix("/r/").map(|l| l.split('-').next().unwrap_or(l).to_string())
        }
    }

    fn pending(from: &str, lane: &str, account: &str, at: i64) -> Pending {
        Pending { from: from.into(), how: OriginHow::Started, lane: lane.into(), account: account.into(), at, helper: false, report: false, opens: None }
    }

    #[test]
    fn a_session_moved_to_another_account_names_where_it_went() {
        let mut store = Store::default();
        let mut entry = |key: &str, from: &str, how, at| store.sessions.insert(key.into(), Entry { from: from.into(), how, at, helper: false, report: false, known: false });
        // Moved to claude2, forked there, and moved back.
        entry("claude:claude2:b", "claude:claude:a", OriginHow::Fork, 10);
        entry("claude:claude2:fork", "claude:claude2:b", OriginHow::Fork, 20);
        entry("claude:claude:c", "claude:claude2:b", OriginHow::Fork, 30);
        // A fork in its own account and a session its agent started aren't moves.
        entry("claude:claude:twin", "claude:claude:x", OriginHow::Fork, 10);
        entry("claude:claude2:helper", "claude:claude:y", OriginHow::Started, 10);
        let all = |_: &str| true;
        assert_eq!(store.moved_to("claude:claude:a", all).as_deref(), Some("claude:claude:c"));
        assert_eq!(store.moved_to("claude:claude2:b", all).as_deref(), Some("claude:claude:c"));
        assert_eq!(store.moved_to("claude:claude:c", all), None);
        assert_eq!((store.moved_to("claude:claude:x", all), store.moved_to("claude:claude:y", all)), (None, None));
        // A copy that's gone (closed before it was sent anything) leaves the session where it was.
        assert_eq!(store.moved_to("claude:claude:a", |k| k != "claude:claude:c").as_deref(), Some("claude:claude2:b"));
        assert_eq!(store.moved_to("claude:claude:a", |k| k != "claude:claude2:b"), None);
        assert!(is_move("claude:claude:a", "claude:claude2:b") && !is_move("claude:claude:a", "codex:codex:b") && !is_move("claude:claude:a", "claude:claude:b"));
    }

    #[test]
    fn a_pending_start_takes_the_first_new_session_of_its_account_in_its_worktree() {
        let mut store = Store { pending: vec![pending("claude:claude:boss", "/r/app", "opencode", 1_000_000)], ..Default::default() };
        let c = |key: &'static str, lane: &'static str, account: &'static str, started: i64| Candidate { key, lane, account, started, prompt: None };
        let sessions = [
            // Before the start, another account, and the one that asked: none of them.
            c("opencode:opencode:old", "/r/app", "opencode", 1_000_000 - 2 * PENDING_SLACK_MS),
            c("codex:codex:other", "/r/app", "codex", 1_000_100),
            c("claude:claude:boss", "/r/app", "opencode", 1_000_100),
            // In another worktree of the repo, and later in its own: its own wins.
            c("opencode:opencode:elsewhere", "/r/app-two", "opencode", 1_000_050),
            c("opencode:opencode:new", "/r/app", "opencode", 1_000_200),
        ];
        assert!(resolve(&mut store, &sessions, &Repos, 1_000_300));
        assert!(store.pending.is_empty());
        assert_eq!(store.origin_of("opencode:opencode:new"), Some(SessionOrigin { key: "claude:claude:boss".into(), how: OriginHow::Started }));
        assert_eq!(store.origin_of("opencode:opencode:elsewhere"), None);
        // A second start doesn't take the session the first did.
        store.pending.push(Pending { how: OriginHow::Fork, ..pending("claude:claude:boss", "/r/app", "opencode", 1_000_150) });
        assert!(resolve(&mut store, &sessions, &Repos, 1_000_300));
        assert_eq!(store.origin_of("opencode:opencode:new").map(|o| o.how), Some(OriginHow::Started));
        assert_eq!(store.origin_of("opencode:opencode:elsewhere").map(|o| o.how), Some(OriginHow::Fork));
        // Without one in its worktree, another worktree of the repo's.
        let mut store = Store { pending: vec![pending("claude:claude:boss", "/r/app", "opencode", 1_000_000)], ..Default::default() };
        assert!(resolve(&mut store, &sessions[..4], &Repos, 1_000_300));
        assert_eq!(store.origin_of("opencode:opencode:elsewhere").map(|o| o.key), Some("claude:claude:boss".into()));
    }

    #[test]
    fn a_helper_started_without_a_key_is_a_helper_once_found() {
        let helper = Pending { helper: true, report: true, ..pending("claude:claude:boss", "/r/app", "opencode", 1_000_000) };
        let mut store = Store { pending: vec![helper], ..Default::default() };
        let sessions = [Candidate { key: "opencode:opencode:new", lane: "/r/app", account: "opencode", started: 1_000_100, prompt: None }];
        assert!(resolve(&mut store, &sessions, &Repos, 1_000_300));
        assert_eq!(store.helper_of("opencode:opencode:new"), Some(("claude:claude:boss", true)));
        assert_eq!(store.origin_of("opencode:opencode:new").map(|o| o.how), Some(OriginHow::Started));
        // Matched, not known: it could be another session started about then.
        assert_eq!(store.known_helper_of("opencode:opencode:new"), None);
        store.sessions.get_mut("opencode:opencode:new").unwrap().known = true;
        assert_eq!(store.known_helper_of("opencode:opencode:new"), Some("claude:claude:boss"));
        // A session another started without it is no helper.
        store.sessions.insert("codex:codex:b".into(), Entry { from: "claude:claude:boss".into(), how: OriginHow::Started, at: 1, helper: false, report: false, known: false });
        assert_eq!(store.helper_of("codex:codex:b"), None);
        assert_eq!(store.known_starter_of("codex:codex:b"), None);
        store.sessions.get_mut("codex:codex:b").unwrap().known = true;
        assert_eq!(store.known_helper_of("codex:codex:b"), None);
        // Known, it and the helper were both started by the session; a fork wasn't.
        assert_eq!(store.known_starter_of("codex:codex:b"), Some("claude:claude:boss"));
        assert_eq!(store.known_starter_of("opencode:opencode:new"), Some("claude:claude:boss"));
        store.sessions.insert("codex:codex:copy".into(), Entry { from: "claude:claude:boss".into(), how: OriginHow::Fork, at: 1, helper: false, report: false, known: true });
        assert_eq!(store.known_starter_of("codex:codex:copy"), None);
        // Written down only when it is one.
        let json = serde_json::to_value(&store.sessions).unwrap();
        assert_eq!((json["opencode:opencode:new"]["helper"].as_bool(), json["codex:codex:b"].get("helper")), (Some(true), None));
    }

    #[test]
    fn a_pending_start_stops_waiting_after_a_while() {
        let mut store = Store { pending: vec![pending("claude:claude:boss", "/r/app", "opencode", 0)], ..Default::default() };
        assert!(resolve(&mut store, &[], &Repos, PENDING_MS + 1));
        assert!(store.pending.is_empty());
        let mut store = Store { pending: vec![pending("claude:claude:boss", "/r/app", "opencode", 0)], ..Default::default() };
        assert!(!resolve(&mut store, &[], &Repos, PENDING_MS - 1));
        assert_eq!(store.pending.len(), 1);
    }

    /// A Codex helper started in a terminal whose UI sat on a screen of its own for an hour
    /// before it made its session (2026-10-14).
    #[test]
    fn a_start_with_a_prompt_takes_a_session_that_opens_with_it_however_late() {
        const PROMPT: &str = "Review the commits on this branch that origin/main doesn't have.\n\nCLAUDE.md has the rules.";
        let start = || Pending { helper: true, report: true, opens: opening(PROMPT), ..pending("claude:claude:boss", "/r/app", "codex", 1_000_000) };
        let late = 1_000_000 + 66 * 60 * 1000;
        let c = |key: &'static str, prompt: Option<&'static str>| Candidate { key, lane: "/r/app", account: "codex", started: late, prompt };
        // As Codex's reader keeps a prompt: its first line.
        let said = "Review the commits on this branch that origin/main doesn't have.";
        // Past the half hour, a session of yours that opens otherwise isn't it, and one with no
        // prompt yet isn't either; the start waits on.
        let mut store = Store { pending: vec![start()], ..Default::default() };
        assert!(!resolve(&mut store, &[c("codex:codex:yours", Some("Fix the login bug")), c("codex:codex:blank", None)], &Repos, late + 1000));
        assert_eq!(store.pending.len(), 1);
        // The one that opens with its prompt is its helper, told of as it asked.
        assert!(resolve(&mut store, &[c("codex:codex:yours", Some("Fix the login bug")), c("codex:codex:review", Some(said))], &Repos, late + 2000));
        assert!(store.pending.is_empty());
        assert_eq!(store.helper_of("codex:codex:review"), Some(("claude:claude:boss", true)));
        assert_eq!(store.origin_of("codex:codex:yours"), None);
        // Within the half hour it prefers that one too, over one that started first.
        let mut store = Store { pending: vec![start()], ..Default::default() };
        let sessions = [Candidate { started: 1_000_100, ..c("codex:codex:yours", Some("Fix the login bug")) }, Candidate { started: 1_000_200, ..c("codex:codex:review", Some(said)) }];
        assert!(resolve(&mut store, &sessions, &Repos, 1_000_300));
        assert_eq!(store.helper_of("codex:codex:review"), Some(("claude:claude:boss", true)));
        // It gives up after a day, and one without a prompt after the half hour.
        let mut store = Store { pending: vec![start(), pending("claude:claude:boss", "/r/app", "codex", 1_000_000)], ..Default::default() };
        assert!(resolve(&mut store, &[], &Repos, late));
        assert_eq!(store.pending.iter().map(|p| p.opens.is_some()).collect::<Vec<_>>(), vec![true]);
        assert!(resolve(&mut store, &[], &Repos, 1_000_000 + PROMPTED_MS + 1));
        assert!(store.pending.is_empty());
        // A start without a prompt notes none.
        assert_eq!(opening("  \n "), None);
    }

    #[test]
    fn old_entries_are_let_go() {
        let mut store = Store::default();
        store.sessions.insert("a".into(), Entry { from: "x".into(), how: OriginHow::Fork, at: 0, helper: false, report: false, known: false });
        store.sessions.insert("b".into(), Entry { from: "x".into(), how: OriginHow::Fork, at: KEEP_MS, helper: false, report: false, known: false });
        store.prune(KEEP_MS + 1);
        assert_eq!(store.sessions.keys().collect::<Vec<_>>(), vec!["b"]);
    }

    /// origins.json from a build that knows a way of starting this one doesn't: not an empty
    /// list to add to and write back.
    #[test]
    fn a_file_that_does_not_read_is_never_written_over() {
        let dir = crate::store::test_dir("origins-unreadable");
        let path = dir.join("origins.json");
        let note = |key: &'static str| {
            change_in(&path, move |s| s.sessions.insert(key.into(), Entry { from: "claude:claude:a".into(), how: OriginHow::Fork, at: crate::util::now_ms(), helper: false, report: false, known: false }).is_none())
        };
        // No file yet: the first note makes it.
        note("codex:codex:b").unwrap();
        assert_eq!(crate::store::read_or_default::<Store>(&path).origin_of("codex:codex:b").map(|o| o.how), Some(OriginHow::Fork));

        let newer = r#"{"sessions":{"codex:codex:b":{"from":"claude:claude:a","how":"teleported","at":5}}}"#;
        for bytes in [newer, r#"{"sessions":{"#] {
            std::fs::write(&path, bytes).unwrap();
            let e = note("codex:codex:c").unwrap_err().to_string();
            assert!(e.starts_with("origins.json doesn't read as a list of where sessions came from"), "{e}");
            assert_eq!(std::fs::read_to_string(&path).unwrap(), bytes);
            // The board goes without.
            assert_eq!(crate::store::read_or_default::<Store>(&path), Store::default());
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn reads_what_older_and_newer_files_hold() {
        let store: Store = serde_json::from_str(r#"{"sessions":{"codex:codex:b":{"from":"claude:claude:a","how":"started","at":5}},"later":true}"#).unwrap();
        assert_eq!(store.origin_of("codex:codex:b"), Some(SessionOrigin { key: "claude:claude:a".into(), how: OriginHow::Started }));
        assert_eq!(serde_json::from_str::<Store>("{}").unwrap(), Store::default());
    }
}
