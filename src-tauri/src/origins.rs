//! Where sessions came from, as Codealong saw it happen: a session whose agent ran `codealong
//! start` (with `--helper`, a helper of it: `Session::helper`, told of with `--report`), and a
//! fork made from Codealong. The harnesses write most forks down themselves (claude/registry.rs's
//! `Forks`, Codex's `forked_from_id`, OpenCode's `fork_session_id`), but nothing records which
//! session ran the command that started another, nor Claude Code's fork into a
//! terminal (`--resume --fork-session`), whose transcript doesn't say. Nor which session started
//! a headless run, which the engine finds by the run's process while it runs and notes here once
//! the run has ended (`ended_runs`). Kept in `origins.json` next to the config; the engine reads
//! it again when it changes (`current`).

use std::collections::{BTreeMap, HashMap, HashSet};
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
/// How long after that a start with a prompt still waits for a session that opens with it, run by
/// a process that began as the start did. A terminal UI can stop on a screen of its own before it
/// makes its session, until you answer it: a Codex helper started in a terminal sat on one for 66
/// minutes and was never its session's (2026-10-14; codex.md has the screens). Its process began
/// two seconds before the start was noted, and that and the prompt say it's the one, however late.
const PROMPTED_MS: i64 = 24 * 60 * 60 * 1000;
/// How long after a start its harness's process can begin: the terminal opens a tab and its shell
/// starts first, slowly on a busy Mac.
const LAUNCH_MS: i64 = 5 * 60 * 1000;
/// How long a start in a terminal goes without its session before it's said to be stuck: its
/// harness's UI makes one within seconds of its prompt, unless it waits on a screen of its own.
const STUCK_MS: i64 = 3 * 60 * 1000;
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
    /// Started in a terminal, whose UI can stop on a screen of its own before it makes its
    /// session (`stuck`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub terminal: bool,
}

impl Pending {
    /// How long it waits: a start with a prompt a day, for a session that opens with it.
    fn waits(&self) -> i64 {
        if self.opens.is_some() { PROMPTED_MS } else { PENDING_MS }
    }

    /// Started in a terminal with a prompt `STUCK_MS` ago or more and still without its session,
    /// while it's waited for: its UI is likely on a screen of its own (Codex's update offer, its
    /// question about its background server), which nobody sees in a terminal behind your
    /// windows, and which Codealong can't read or answer. One started without a prompt makes no
    /// session until someone types one, so it isn't.
    pub fn stuck(&self, now: i64) -> bool {
        self.terminal && self.opens.is_some() && now - self.at >= STUCK_MS && now - self.at <= self.waits()
    }

    /// How many whole minutes ago it was started.
    pub fn minutes(&self, now: i64) -> i64 {
        (now - self.at).max(0) / 60_000
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
    /// running is resumed for a message from one it started this way, or from the one that
    /// started it (send.rs `wakes`).
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

    /// Notes where each headless run came from (`ended_runs`), unless something noted it
    /// already: a start that said which session it made knows better. Not `known`: the run's
    /// process said where it came from, not a start.
    fn note_runs(&mut self, runs: &[(String, String)], now: i64) -> bool {
        let mut changed = false;
        for (key, from) in runs {
            if key != from && !key.is_empty() && !from.is_empty() && !self.sessions.contains_key(key) {
                self.sessions.insert(key.clone(), Entry { from: from.clone(), how: OriginHow::Started, at: now, helper: false, report: false, known: false });
                changed = true;
            }
        }
        changed
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
    /// When it started where its harness records that (`Session::started_at`): never a time it
    /// was active, which says nothing of when it began.
    pub created: Option<i64>,
    /// Its first prompt, else its latest, as its harness's reader keeps it.
    pub prompt: Option<&'a str>,
    /// When the process running it began, where one is running it (`procs::proc_start_ms`). Only
    /// as sure as the session's `pid`: a Codex thread's terminal UI is inferred
    /// (codex/terminals.rs `pair_terminals`, the newest terminal in a folder taking its newest
    /// thread), so two of them stuck in one folder and answered oldest first are swapped, and the
    /// older start's late session goes to the newer start (Codex's look, 2026-10-14).
    pub launched: Option<i64>,
}

/// Gives each pending start the first session of its account that started after it, in its
/// worktree, else in another worktree of the same repo (the apps can make their own), that came
/// from nowhere yet: one that opens with its prompt first. A session whose process began before
/// the start was asked for isn't its, however late the session showed. After `PENDING_MS`, only
/// one in its own worktree that opens with its prompt and whose process, or the session itself,
/// began as the start did (`LAUNCH_MS`), and that no other start waiting there could take so too:
/// helpers' prompts often open alike (a review's boilerplate), and a session you start later with
/// the same words isn't a stuck start's. Returns whether anything changed.
pub fn resolve(store: &mut Store, sessions: &[Candidate], lanes: &dyn Lanes, now: i64) -> bool {
    if store.pending.is_empty() {
        return false;
    }
    let mut changed = false;
    let mut taken: HashSet<String> = store.sessions.keys().cloned().collect();
    let mut waiting = Vec::new();
    let starts: Vec<Pending> = store.pending.iter().filter(|q| now - q.at <= q.waits()).cloned().collect();
    let opens = |q: &Pending, s: &Candidate| q.opens.is_some() && s.prompt.and_then(opening) == q.opens;
    // Its own, late: its process began as the start did; or, with no process left to say, the
    // session itself did by its harness's record, which the first half hour would have taken had
    // the board been looked at (the app closed meanwhile, say).
    let claims = |q: &Pending, s: &Candidate| {
        let soon = |t: i64| t >= q.at - PENDING_SLACK_MS && t <= q.at + LAUNCH_MS;
        s.launched.or(s.created).is_some_and(soon) && opens(q, s) && q.account == s.account && q.lane == s.lane && s.key != q.from
    };
    // Agents' starts first: one from no session notes no origin, and mustn't take the session of
    // an agent's that opens alike before that start does.
    let mut all = std::mem::take(&mut store.pending);
    all.sort_by_key(|p| p.from.is_empty());
    for p in all {
        if now - p.at > p.waits() {
            changed = true;
            continue;
        }
        let repo = lanes.repo_of(&p.lane);
        let late = now - p.at > PENDING_MS;
        let pick = sessions
            .iter()
            .filter(|s| s.account == p.account && s.started >= p.at - PENDING_SLACK_MS && s.key != p.from && !taken.contains(s.key))
            .filter(|s| s.launched.is_none_or(|l| l >= p.at - PENDING_SLACK_MS))
            .filter(|s| s.lane == p.lane || (repo.is_some() && lanes.repo_of(s.lane) == repo))
            // One from no session notes no origin, so it takes only one that opens with its
            // prompt, and never an agent's helper that shows first.
            .filter(|s| !p.from.is_empty() || opens(&p, s))
            .filter(|s| !late || (claims(&p, s) && starts.iter().filter(|q| claims(q, s)).count() == 1))
            .min_by_key(|s| (!opens(&p, s), s.lane != p.lane, s.started));
        match pick {
            Some(s) => {
                taken.insert(s.key.to_string());
                // A start from no session found its session, and has no origin to note.
                if !p.from.is_empty() {
                    store.sessions.insert(s.key.to_string(), Entry { from: p.from, how: p.how, at: p.at, helper: p.helper, report: p.report, known: false });
                }
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

/// The headless runs on the board that have outlived their process and aren't noted here yet,
/// each with the session the engine found had started it while it ran (`found`,
/// `engine::with_run_origins`). Nothing on disk says where a run came from, so once Codealong
/// starts again there's no process left to find its session by, and a run still listed has a
/// record of its own that keeps it on the board. One that keeps none (`claude -p
/// --no-session-persistence`) leaves the board as it ends, so an eval's thousand judge calls
/// aren't written down.
pub fn ended_runs(board: &crate::model::Board, found: &HashMap<String, String>, store: &Store) -> Vec<(String, String)> {
    board
        .lanes
        .iter()
        .flat_map(|l| l.sessions.iter().filter(|s| s.visitor.is_none()))
        .filter(|s| s.headless && !s.live && !store.sessions.contains_key(&s.key))
        .filter_map(|s| Some((s.key.clone(), found.get(&s.key)?.clone())))
        .collect()
}

/// Notes where the runs `ended_runs` gives came from, in one write.
pub fn record_runs(runs: &[(String, String)]) {
    let now = crate::util::now_ms();
    if let Err(e) = change(|s| s.note_runs(runs, now)) {
        eprintln!("Couldn't note where headless runs came from: {e:#}");
    }
}

/// Takes back the note of where `key` came from: a move whose copy didn't start.
pub fn forget(key: &str) -> Result<()> {
    change(|s| s.sessions.remove(key).is_some())
}

/// Notes where a start came from: the session that asked for it (`StartSpec::from`), by the new
/// session's key when the start knew it, else as a start waiting for its session (`Pending`,
/// `resolve`). `StartSpec::helper`: it was started as a helper of that session, which
/// `StartSpec::report` says is told what it does. A start in a terminal from no session (yours,
/// in Codealong) waits too, with no `from`, only so it's said when it's stuck (`stuck_notices`):
/// it notes no origin.
pub fn started(spec: &crate::start::StartSpec, key: Option<&str>) {
    let from = spec.from.as_deref().unwrap_or_default();
    let terminal = matches!(spec.place, crate::start::Place::Terminal | crate::start::Place::Background);
    match key {
        Some(key) if !from.is_empty() => {
            if let Err(e) = write_as(key, from, OriginHow::Started, spec.helper, spec.report) {
                eprintln!("Couldn't note where {key} came from: {e:#}");
            }
        }
        Some(_) => {}
        // Yours waits only to be said if it's stuck, which takes a terminal and a prompt.
        None if from.is_empty() && !(terminal && opening(&spec.prompt).is_some()) => {}
        None => {
            let p = Pending { from: from.to_string(), how: OriginHow::Started, lane: spec.lane_id.clone(), account: spec.account.clone(), at: crate::util::now_ms(), helper: spec.helper && !from.is_empty(), report: spec.report && !from.is_empty(), opens: opening(&spec.prompt), terminal };
            if let Err(e) = change(|s| {
                s.pending.push(p);
                true
            }) {
                eprintln!("Couldn't note a start from {from}: {e:#}");
            }
        }
    }
}

/// A notice for each start in a terminal that's stuck (`Pending::stuck`): it has no session on
/// the board yet, and its terminal may be behind your windows. Each has a key of its own (`start:`
/// and when it was started), so dismissing one doesn't hide the next, even in the same worktree,
/// and goes once its session shows.
pub fn stuck_notices(pending: &[Pending], board: &crate::model::Board, word_of: impl Fn(&str) -> String, now: i64) -> Vec<crate::model::Notice> {
    pending
        .iter()
        .filter(|p| p.stuck(now))
        .map(|p| {
            let place = board.lanes.iter().find(|l| l.id == p.lane).map(|l| l.callsign.clone()).unwrap_or_else(|| crate::util::tilde(Path::new(&p.lane)));
            let word = word_of(&p.account);
            let asker = board.session(&p.from).map(|(_, s)| crate::send::quoted(&s.title)).unwrap_or_else(|| "another session".into());
            let by = match (p.from.is_empty(), p.helper) {
                (true, _) => String::new(),
                (false, true) => format!(" as a helper of {asker},"),
                (false, false) => format!(" by {asker},"),
            };
            let text = format!(
                "{word} was started in {terminal} {mins} minutes ago, in {place},{by} and hasn't made its session yet: its terminal may be waiting on a screen of its own before it starts, such as an update offer or a question about its background server. Answer it in that tab; if you closed it, dismiss this.",
                terminal = board.apps.terminal,
                mins = p.minutes(now),
            );
            crate::model::Notice { key: format!("start:{}", p.at), title: format!("{word} hasn't started in {place}"), text, ..Default::default() }
        })
        .collect()
}

/// The board's sessions as pending starts are matched against them, each its own lane's (not a
/// visitor), with when its process began asked only of the accounts a start waits on.
pub fn candidates<'a>(board: &'a crate::model::Board, pending: &[Pending]) -> Vec<Candidate<'a>> {
    let waited: HashSet<&str> = pending.iter().map(|p| p.account.as_str()).collect();
    let launched = |s: &crate::model::Session| s.pid.filter(|_| s.live && waited.contains(s.account.as_str())).and_then(crate::procs::proc_start_ms);
    board
        .lanes
        .iter()
        .flat_map(|l| {
            l.sessions.iter().filter(|s| s.visitor.is_none()).map(|s| Candidate {
                key: &s.key,
                lane: &l.id,
                account: &s.account,
                started: s.started_at.unwrap_or(s.last_activity),
                created: s.started_at,
                prompt: s.first_prompt.as_deref().or(s.last_prompt.as_deref()),
                launched: launched(s),
            })
        })
        .collect()
}

/// The starts still waiting for their session as of this board: what's pending, less what its
/// sessions settle (`resolve`, on a copy). The app writes that down as it publishes; a board
/// made without it (a snapshot with the app closed) would otherwise call a start stuck whose
/// session it lists.
pub fn waiting_for(board: &crate::model::Board, now: i64) -> Vec<Pending> {
    let current = current();
    if current.pending.is_empty() {
        return Vec::new();
    }
    let mut store = (*current).clone();
    let sessions = candidates(board, &store.pending);
    resolve(&mut store, &sessions, &crate::todos::LaneIndex::of(&board.lanes), now);
    store.pending
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
    use crate::model::{Board, Session};

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
        Pending { from: from.into(), how: OriginHow::Started, lane: lane.into(), account: account.into(), at, helper: false, report: false, opens: None, terminal: false }
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
        let c = |key: &'static str, lane: &'static str, account: &'static str, started: i64| Candidate { key, lane, account, started, created: Some(started), prompt: None, launched: None };
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
        let sessions = [Candidate { key: "opencode:opencode:new", lane: "/r/app", account: "opencode", started: 1_000_100, created: Some(1_000_100), prompt: None, launched: None }];
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
    /// before it made its session (2026-10-14), and the sessions and starts it isn't to be taken
    /// for: the next review's, and one you started with the same words.
    #[test]
    fn a_start_with_a_prompt_takes_the_session_its_process_runs_however_late() {
        const AT: i64 = 1_000_000;
        const PROMPT: &str = "Review the commits on this branch that origin/main doesn't have.\n\nCLAUDE.md has the rules.";
        let start = || Pending { helper: true, report: true, opens: opening(PROMPT), ..pending("claude:claude:boss", "/r/app", "codex", AT) };
        let late = AT + 66 * 60 * 1000;
        // Each session showed an hour on, its process begun when `launched` says.
        let c = |key: &'static str, prompt: Option<&'static str>, launched: i64| Candidate { key, lane: "/r/app", account: "codex", started: late, created: Some(late), prompt, launched: Some(launched) };
        // As Codex's reader keeps a prompt: its first line.
        let said = "Review the commits on this branch that origin/main doesn't have.";
        // Past the half hour, a session that opens otherwise isn't it, nor one with no prompt yet,
        // nor one with no process to say when it began; the start waits on.
        let mut store = Store { pending: vec![start()], ..Default::default() };
        let unproven = [c("codex:codex:other", Some("Fix the login bug"), AT - 2000), c("codex:codex:blank", None, AT - 2000), Candidate { launched: None, ..c("codex:codex:gone", Some(said), AT) }];
        assert!(!resolve(&mut store, &unproven, &Repos, late + 1000));
        // One that began as the start did and has finished since, its process gone, found only
        // now (the app was closed meanwhile): its own, and so not stuck.
        let mut finished = Store { pending: vec![start()], ..Default::default() };
        let done = Candidate { started: AT + 20_000, created: Some(AT + 20_000), launched: None, ..c("codex:codex:done", Some(said), AT) };
        assert!(resolve(&mut finished, &[done], &Repos, late + 1000));
        assert_eq!(finished.helper_of("codex:codex:done"), Some(("claude:claude:boss", true)));
        // Not one whose harness keeps no start time, and whose last activity only says it finished
        // then (Antigravity's): that's no sign of when it began.
        let mut finished = Store { pending: vec![start()], ..Default::default() };
        let unknown = Candidate { started: AT + 20_000, created: None, launched: None, ..c("codex:codex:unknown", Some(said), AT) };
        assert!(!resolve(&mut finished, &[unknown], &Repos, late + 1000));
        // Nor one you started an hour on with the same words.
        assert!(!resolve(&mut store, &[c("codex:codex:yours", Some(said), late)], &Repos, late + 1000));
        assert_eq!(store.pending.len(), 1);
        // The one its process runs (begun two seconds before the start was noted, as on
        // 2026-10-14) is its helper, told of as it asked.
        assert!(resolve(&mut store, &[c("codex:codex:yours", Some(said), late), c("codex:codex:review", Some(said), AT - 2000)], &Repos, late + 2000));
        assert!(store.pending.is_empty());
        assert_eq!(store.helper_of("codex:codex:review"), Some(("claude:claude:boss", true)));
        assert_eq!(store.origin_of("codex:codex:yours"), None);
        // Within the half hour it prefers that one too, over one that started first.
        let mut store = Store { pending: vec![start()], ..Default::default() };
        let sessions = [Candidate { started: AT + 100, ..c("codex:codex:yours", Some("Fix the login bug"), AT + 100) }, Candidate { started: AT + 200, ..c("codex:codex:review", Some(said), AT + 50) }];
        assert!(resolve(&mut store, &sessions, &Repos, AT + 300));
        assert_eq!(store.helper_of("codex:codex:review"), Some(("claude:claude:boss", true)));
        // Late, only in its own worktree, not another of the repo's.
        let mut store = Store { pending: vec![start()], ..Default::default() };
        assert!(!resolve(&mut store, &[Candidate { lane: "/r/app-two", ..c("codex:codex:review", Some(said), AT) }], &Repos, late + 1000));
        // Another session started a review with the same words a minute ago. The stuck one's
        // session is the stuck one's, its process begun before the newer start, and the newer
        // start's own is that start's, whichever shows first.
        let since = Pending { from: "claude:claude:other".into(), at: late - 60_000, ..start() };
        let (stuck, theirs) = (c("codex:codex:stuck", Some(said), AT + 1000), c("codex:codex:theirs", Some(said), late - 59_000));
        let mut store = Store { pending: vec![start(), since.clone()], ..Default::default() };
        assert!(resolve(&mut store, &[c("codex:codex:stuck", Some(said), AT + 1000)], &Repos, late + 1000));
        assert_eq!(store.helper_of("codex:codex:stuck"), Some(("claude:claude:boss", true)));
        assert_eq!(store.pending.iter().map(|p| p.from.as_str()).collect::<Vec<_>>(), ["claude:claude:other"]);
        let mut store = Store { pending: vec![start(), since], ..Default::default() };
        assert!(resolve(&mut store, &[theirs, stuck], &Repos, late + 1000));
        assert_eq!(store.helper_of("codex:codex:theirs"), Some(("claude:claude:other", true)));
        assert_eq!(store.helper_of("codex:codex:stuck"), Some(("claude:claude:boss", true)));
        // Two started at once with the same words: neither takes a late session it can't tell.
        let twin = Pending { from: "claude:claude:other".into(), at: AT + 1000, ..start() };
        let mut store = Store { pending: vec![start(), twin], ..Default::default() };
        assert!(!resolve(&mut store, &[c("codex:codex:stuck", Some(said), AT + 1000)], &Repos, late + 1000));
        // A process begun before a start was asked for runs no session of it, in its first half
        // hour too.
        let mut store = Store { pending: vec![pending("claude:claude:boss", "/r/app", "codex", late)], ..Default::default() };
        assert!(!resolve(&mut store, &[c("codex:codex:stuck", Some(said), AT + 1000)], &Repos, late + 1000));
        // It gives up after a day, and one without a prompt after the half hour.
        let mut store = Store { pending: vec![start(), pending("claude:claude:boss", "/r/app", "codex", AT)], ..Default::default() };
        assert!(resolve(&mut store, &[], &Repos, late));
        assert_eq!(store.pending.iter().map(|p| p.opens.is_some()).collect::<Vec<_>>(), vec![true]);
        assert!(resolve(&mut store, &[], &Repos, 1_000_000 + PROMPTED_MS + 1));
        assert!(store.pending.is_empty());
        // A start without a prompt notes none.
        assert_eq!(opening("  \n "), None);
    }

    /// A Codex helper's terminal UI sat on a screen of its own for an hour, behind your windows,
    /// and nothing said so (2026-10-14).
    #[test]
    fn a_start_in_a_terminal_without_its_session_after_a_while_is_said_to_be_stuck() {
        const AT: i64 = 1_000_000;
        let start = Pending { helper: true, report: true, terminal: true, opens: opening("Review the branch"), ..pending("claude:claude:boss", "/r/app", "codex", AT) };
        assert!(!start.stuck(AT + STUCK_MS - 1));
        assert!(start.stuck(AT + STUCK_MS) && start.stuck(AT + 66 * 60_000));
        // Not once it stops waiting, nor a start in an app, which shows you its prompt, nor one
        // with no prompt, which waits for someone to type one.
        assert!(!start.stuck(AT + PROMPTED_MS + 1));
        assert!(!Pending { terminal: false, ..start.clone() }.stuck(AT + STUCK_MS));
        assert!(!Pending { opens: None, ..start.clone() }.stuck(AT + STUCK_MS));
        let boss = Session { title: "Dashboards".into(), ..crate::fixtures::session("claude:claude:boss") };
        let mut board = Board { lanes: vec![crate::fixtures::lane("/r/app", vec![boss])], ..Board::default() };
        board.apps.terminal = "Ghostty".into();
        let word = |_: &str| "Codex".to_string();
        assert!(stuck_notices(&[start.clone()], &board, word, AT + 60_000).is_empty());
        let said = stuck_notices(&[start.clone()], &board, word, AT + 66 * 60_000);
        assert_eq!((said.len(), said[0].key.as_str(), said[0].title.as_str()), (1, "start:1000000", format!("Codex hasn't started in {}", board.lanes[0].callsign).as_str()));
        // Two stuck in one worktree are two notices, one dismissed apart from the other.
        let again = Pending { at: AT + 60_000, ..start.clone() };
        let two = stuck_notices(&[start.clone(), again], &board, word, AT + 66 * 60_000);
        assert!(two.len() == 2 && two[0].key != two[1].key);
        assert!(said[0].text.starts_with(&format!("Codex was started in Ghostty 66 minutes ago, in {}, as a helper of “Dashboards”, and hasn't made its session yet: its terminal may be waiting on a screen of its own", board.lanes[0].callsign)), "{}", said[0].text);
        // Started by a session the board doesn't show, in a folder it doesn't either; and by you.
        let other = Pending { helper: false, lane: "/r/gone".into(), from: "claude:claude:gone".into(), ..start.clone() };
        assert!(stuck_notices(&[other], &board, word, AT + STUCK_MS)[0].text.contains("in /r/gone, by another session, and hasn't"));
        let yours = Pending { helper: false, report: false, from: String::new(), ..start };
        let said = stuck_notices(&[yours.clone()], &board, word, AT + STUCK_MS);
        assert!(said[0].text.contains(&format!("in {}, and hasn't made its session yet", board.lanes[0].callsign)), "{}", said[0].text);
        // Yours doesn't take an agent's helper that shows first, which its own start takes; it
        // takes its own, and notes no origin.
        let agents = Pending { from: "claude:claude:boss".into(), at: AT + 10, opens: opening("Check the migration"), ..yours.clone() };
        let mut store = Store { pending: vec![yours, agents], ..Default::default() };
        let helper = Candidate { key: "codex:codex:helper", lane: "/r/app", account: "codex", started: AT + 100, created: Some(AT + 100), prompt: Some("Check the migration"), launched: Some(AT + 20) };
        assert!(resolve(&mut store, &[helper], &Repos, AT + 200));
        assert_eq!(store.origin_of("codex:codex:helper").map(|o| o.key), Some("claude:claude:boss".into()));
        assert_eq!(store.pending.len(), 1);
        let mine = Candidate { key: "codex:codex:mine", lane: "/r/app", account: "codex", started: AT + 300, created: Some(AT + 300), prompt: Some("Review the branch"), launched: Some(AT + 50) };
        assert!(resolve(&mut store, &[mine], &Repos, AT + 400));
        assert!(store.pending.is_empty() && store.origin_of("codex:codex:mine").is_none());
        // Nor when it opens alike and was started first: the agent's start takes its helper.
        let yours = Pending { from: String::new(), helper: false, report: false, opens: opening("Review the branch"), ..pending("", "/r/app", "codex", AT) };
        let agents = Pending { from: "claude:claude:boss".into(), helper: true, report: true, at: AT + 10, ..yours.clone() };
        let mut store = Store { pending: vec![yours, agents], ..Default::default() };
        let helper = Candidate { key: "codex:codex:helper", lane: "/r/app", account: "codex", started: AT + 100, created: Some(AT + 100), prompt: Some("Review the branch"), launched: Some(AT + 20) };
        let other = Candidate { key: "codex:codex:other", prompt: Some("Fix the login bug"), started: AT + 150, ..helper };
        assert!(resolve(&mut store, &[helper, other], &Repos, AT + 200));
        assert_eq!(store.helper_of("codex:codex:helper"), Some(("claude:claude:boss", true)));
        assert_eq!(store.origin_of("codex:codex:other"), None);
    }

    #[test]
    fn a_headless_run_is_noted_once_it_has_outlived_its_process() {
        let run = |key: &str, live: bool| Session { headless: true, live, ..crate::fixtures::session(key) };
        let ended = run("claude:claude:ended", false);
        let visiting = Session { visitor: Some("worked here".into()), ..ended.clone() };
        let board = Board {
            lanes: vec![
                crate::fixtures::lane("/r/app", vec![crate::fixtures::session("claude:claude:boss"), ended, run("claude:claude:running", true), run("claude:claude:noted", false), run("claude:claude:unknown", false)]),
                crate::fixtures::lane("/r/app-two", vec![visiting]),
            ],
            ..Board::default()
        };
        let found: HashMap<String, String> = ["ended", "running", "noted"].iter().map(|k| (format!("claude:claude:{k}"), "claude:claude:boss".to_string())).collect();
        // A start that said which session it made named another.
        let mut store = Store::default();
        store.sessions.insert("claude:claude:noted".into(), Entry { from: "claude:claude:other".into(), how: OriginHow::Started, at: 5, helper: true, report: false, known: true });
        // Not one still running, which a later look finds again, nor one nobody found the
        // session of, and each once whatever lanes list it.
        let runs = ended_runs(&board, &found, &store);
        assert_eq!(runs, vec![("claude:claude:ended".to_string(), "claude:claude:boss".to_string())]);
        assert!(store.note_runs(&runs, 10));
        assert_eq!(store.origin_of("claude:claude:ended"), Some(SessionOrigin { key: "claude:claude:boss".into(), how: OriginHow::Started }));
        assert_eq!((store.helper_of("claude:claude:ended"), store.known_starter_of("claude:claude:ended")), (None, None));
        assert!(ended_runs(&board, &found, &store).is_empty());
        // What a start noted stays.
        assert!(!store.note_runs(&[("claude:claude:noted".to_string(), "claude:claude:boss".to_string())], 10));
        assert_eq!(store.helper_of("claude:claude:noted"), Some(("claude:claude:other", false)));
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
