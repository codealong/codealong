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
/// starts first, slowly on a busy Mac. A stuck start's terminal is found by when its process began
/// too (starting.rs `process_of`).
pub(crate) const LAUNCH_MS: i64 = 5 * 60 * 1000;
/// How long a start in a terminal goes without its session before it's said to be stuck: its
/// harness's UI makes one within seconds of its prompt, unless it waits on a screen of its own.
pub(crate) const STUCK_MS: i64 = 3 * 60 * 1000;
/// A session that says it started a little before the start was asked for can still be it.
pub(crate) const PENDING_SLACK_MS: i64 = 60 * 1000;
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
    /// Helpers among `sessions` whose session was told they hadn't shown up (helpers.rs
    /// `News::Unseen`), so a server started since doesn't tell it again (`told_unseen`).
    #[serde(default, skip_serializing_if = "std::collections::BTreeSet::is_empty")]
    pub unseen: std::collections::BTreeSet<String>,
    /// Sessions created by a non-destructive handoff (`keep: true`): they stay linked to their
    /// source via `OriginHow::Fork`, without marking the source as moved or blocking sends to it.
    #[serde(default, skip_serializing_if = "std::collections::BTreeSet::is_empty")]
    pub kept: std::collections::BTreeSet<String>,
    /// Pending handoffs whose target harness picks its own session ID (keyed by `Pending::at`):
    /// once `resolve` matches the new session, either marks it kept or returns a `ResolvedHandover`
    /// so the engine transfers todos, labels, pins and schedules from `from` to the new session.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub handovers: BTreeMap<i64, PendingHandover>,
}

/// Extra state carried by a pending cross-provider handoff while waiting for its session ID.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PendingHandover {
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub keep: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub was_pinned: bool,
}

/// A pending move that just matched its target session, for the engine to finish handing over
/// todos, labels, pins, terminals and schedules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedHandover {
    pub from: String,
    pub copy: String,
    pub was_pinned: bool,
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
    /// How long it waits: a start in a terminal with a prompt waits a day, for a session that
    /// opens with it once its terminal UI's prompt is answered; any other start (a background
    /// service like `claude --bg`, a desktop app, or a blank terminal) waits half an hour.
    fn waits(&self) -> i64 {
        if self.terminal && self.opens.is_some() { PROMPTED_MS } else { PENDING_MS }
    }

    /// Started in a terminal with a prompt and still waited for: its UI can stop on a screen of
    /// its own before it makes its session, whose terminal is read for it (starting.rs
    /// `Screens`). One started without a prompt makes no session until someone types one.
    pub fn in_terminal(&self, now: i64) -> bool {
        self.terminal && self.opens.is_some() && now - self.at <= self.waits()
    }

    /// Waiting in a terminal (`in_terminal`) `STUCK_MS` or more without its session: its UI is
    /// likely on a screen of its own (Codex's review of a repo's hooks, its update offer, its
    /// question about its background server), which nobody sees in a terminal behind your
    /// windows, and which Codealong can't answer.
    pub fn stuck(&self, now: i64) -> bool {
        self.in_terminal(now) && now - self.at >= STUCK_MS
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

    /// The session the session with this key is a helper of, when it was started as one (or
    /// forked from one), and whether that session is told what it does.
    pub fn helper_of(&self, key: &str) -> Option<(&str, bool)> {
        let mut at = key;
        let mut seen = HashSet::new();
        while seen.insert(at) {
            let e = self.sessions.get(at)?;
            if e.helper {
                return Some((e.from.as_str(), e.report));
            }
            if e.how != OriginHow::Fork {
                return None;
            }
            at = e.from.as_str();
        }
        None
    }

    /// The session the session with this key was started as a helper of (or forked from such a
    /// helper), when the start that made it said which session it made (`Entry::known`), not when
    /// it was only matched later. Made your own since, it's still noted here; the board says it
    /// helps no more.
    pub fn known_helper_of(&self, key: &str) -> Option<&str> {
        let mut at = key;
        let mut seen = HashSet::new();
        while seen.insert(at) {
            let e = self.sessions.get(at)?;
            if e.helper {
                return e.known.then_some(e.from.as_str());
            }
            if e.how != OriginHow::Fork || !e.known {
                return None;
            }
            at = e.from.as_str();
        }
        None
    }

    /// The session whose agent started the session with this key (`codealong start`, a helper or
    /// not), when the start said which session it made (`Entry::known`). A session that isn't
    /// running is resumed for a message from one it started this way, or from the one that
    /// started it (send.rs `wakes`).
    pub fn known_starter_of(&self, key: &str) -> Option<&str> {
        self.sessions.get(key).filter(|e| e.how == OriginHow::Started && e.known).map(|e| e.from.as_str())
    }

    /// Whether session `key` with origin `e` is a move (replaces `e.from`): a fork into another
    /// account or harness that wasn't created with `keep: true` (`self.kept`).
    fn is_moved_entry(&self, key: &str, e: &Entry) -> bool {
        e.how == OriginHow::Fork && !self.kept.contains(key) && is_move(&e.from, key)
    }

    /// The session `key` moved to (start.rs `move_to`, handoff.rs): a move of it into another
    /// account or harness that `exists` says is still there, followed on where that one moved too.
    /// A copy closed before it was sent anything is still there, made again when it's next sent
    /// something (`claude::Reader::closed_copies`); one whose conversation is gone from this Mac
    /// isn't, and the session stays where it was.
    pub fn moved_to(&self, key: &str, exists: impl Fn(&str) -> bool) -> Option<String> {
        let mut at = key.to_string();
        // Moved back again, a key comes round a second time.
        let mut seen = HashSet::from([at.clone()]);
        loop {
            let next = self.sessions.iter().filter(|(k, e)| e.from == at && self.is_moved_entry(k, e) && !seen.contains(*k) && exists(k)).max_by_key(|(_, e)| e.at);
            let Some((next, _)) = next else { break };
            seen.insert(next.clone());
            at = next.clone();
        }
        (at != key).then_some(at)
    }

    /// Every move to another account or harness (`moved_to`), as the session it moved from and
    /// its copy, the oldest first, so one moved on again comes after the move that made it.
    pub fn moves(&self) -> Vec<(String, String)> {
        let mut moves: Vec<(i64, &String, &String)> = self.sessions.iter().filter(|(k, e)| self.is_moved_entry(k, e)).map(|(k, e)| (e.at, &e.from, k)).collect();
        moves.sort();
        moves.into_iter().map(|(_, from, to)| (from.clone(), to.clone())).collect()
    }

    /// Indexes every move (`is_moved_entry`) by the session it moved from, so callers that follow
    /// many keys against one store don't scan `sessions` on every lookup.
    pub fn move_index(&self) -> MoveIndex<'_> {
        let mut by_from: HashMap<&str, Vec<(&str, i64)>> = HashMap::new();
        for (k, e) in &self.sessions {
            if self.is_moved_entry(k, e) {
                by_from.entry(e.from.as_str()).or_default().push((k.as_str(), e.at));
            }
        }
        MoveIndex { by_from }
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

    /// Notes `copy` as the start `key` was (`known_starter_of`), a helper and told of as `key`
    /// was, where its harness copied `key` as it went on (`carry_start`). Says whether it changed.
    fn carry_start(&mut self, copy: &str, key: &str, now: i64) -> bool {
        let Some(entry) = self.sessions.get(key).filter(|e| e.how == OriginHow::Started && e.known).cloned() else { return false };
        let entry = Entry { at: now, ..entry };
        copy != key && copy != entry.from && self.sessions.insert(copy.to_string(), entry.clone()).is_none_or(|old| Entry { at: now, ..old } != entry)
    }

    fn prune(&mut self, now: i64) {
        self.sessions.retain(|_, e| now - e.at < KEEP_MS);
        self.pending.retain(|p| now - p.at < p.waits());
        let sessions = &self.sessions;
        self.unseen.retain(|k| sessions.contains_key(k));
        self.kept.retain(|k| sessions.contains_key(k));
        let pending_at: HashSet<i64> = self.pending.iter().map(|p| p.at).collect();
        self.handovers.retain(|at, _| pending_at.contains(at));
    }
}

/// Moves out of each session (`Store::moved_to`), indexed by source key.
pub struct MoveIndex<'a> {
    by_from: HashMap<&'a str, Vec<(&'a str, i64)>>,
}

impl<'a> MoveIndex<'a> {
    pub fn is_empty(&self) -> bool {
        self.by_from.is_empty()
    }

    /// Same result as `Store::moved_to`, without scanning every origin on each lookup.
    pub fn moved_to(&self, key: &str, exists: impl Fn(&str) -> bool) -> Option<String> {
        let first = self.by_from.get(key)?;
        let mut at = key;
        let mut seen: Option<HashSet<&str>> = None;
        let mut step = first;
        loop {
            let next = step.iter().filter(|&&(k, _)| k != key && seen.as_ref().is_none_or(|s| !s.contains(k)) && exists(k)).max_by_key(|&&(_, t)| t);
            let Some(&(next, _)) = next else { break };
            if at != key {
                seen.get_or_insert_with(|| HashSet::from([key, at])).insert(next);
            }
            at = next;
            match self.by_from.get(at) {
                Some(more) => step = more,
                None => break,
            }
        }
        (at != key).then(|| at.to_string())
    }
}

/// Whether a fork from session `from` to `to` is a move: into another account (same harness or
/// cross-provider). The windows tell it the same way (lineage.ts `kinHow`).
pub fn is_move(from: &str, to: &str) -> bool {
    fn whose(key: &str) -> (&str, &str) {
        let mut parts = key.splitn(3, ':');
        (parts.next().unwrap_or_default(), parts.next().unwrap_or_default())
    }
    let ((from_harness, from_account), (to_harness, to_account)) = (whose(from), whose(to));
    !from_harness.is_empty() && !from_account.is_empty() && !to_harness.is_empty() && !to_account.is_empty() && (from_harness != to_harness || from_account != to_account)
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
    resolve_with_handovers(store, sessions, lanes, now).0
}

/// Like `resolve`, also returning any pending cross-provider moves (`Store::handovers` with
/// `!keep`) that just matched their target session so the engine can finish handing over todos,
/// labels, pins and schedules.
pub fn resolve_with_handovers(store: &mut Store, sessions: &[Candidate], lanes: &dyn Lanes, now: i64) -> (bool, Vec<ResolvedHandover>) {
    if store.pending.is_empty() {
        return (false, Vec::new());
    }
    let mut changed = false;
    let mut handovers = Vec::new();
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
            store.handovers.remove(&p.at);
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
                let ho = store.handovers.remove(&p.at);
                // A start from no session found its session, and has no origin to note.
                if !p.from.is_empty() {
                    let from = p.from.clone();
                    store.sessions.insert(s.key.to_string(), Entry { from: p.from, how: p.how, at: p.at, helper: p.helper, report: p.report, known: false });
                    if let Some(h) = ho {
                        if h.keep {
                            store.kept.insert(s.key.to_string());
                        } else {
                            handovers.push(ResolvedHandover { from, copy: s.key.to_string(), was_pinned: h.was_pinned });
                        }
                    }
                }
                changed = true;
            }
            None => waiting.push(p),
        }
    }
    store.pending = waiting;
    (changed, handovers)
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

/// Notes that the session that started the helper `key` was told it hadn't shown up (helpers.rs
/// `News::Unseen`), once and for good.
pub fn told_unseen(key: &str) {
    if let Err(e) = change(|s| s.unseen.insert(key.to_string())) {
        eprintln!("Couldn't note that {key} was told of: {e:#}");
    }
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

/// Notes a handoff from `from` to `key`: when `moved` is true, `key` takes `from`'s place as a
/// move (`moved_to`); when false (`keep: true`), `key` is kept out of `moved_to` so `from` stays
/// open and sendable while staying linked in lineage.
pub fn write_handoff(key: &str, from: &str, moved: bool) -> Result<()> {
    if key == from || key.is_empty() || from.is_empty() {
        return Ok(());
    }
    let now = crate::util::now_ms();
    change(|s| {
        let entry = Entry { from: from.to_string(), how: OriginHow::Fork, at: now, helper: false, report: false, known: true };
        let mut changed = s.sessions.insert(key.to_string(), entry).is_none_or(|old| old.from != from || old.how != OriginHow::Fork || !old.known);
        if moved {
            changed |= s.kept.remove(key);
        } else {
            changed |= s.kept.insert(key.to_string());
        }
        changed
    })
}

/// Notes a pending handoff whose target harness picks its own session ID (Codex, Antigravity,
/// OpenCode in a terminal, or Claude app): once matched by `resolve_pending_handovers`, its
/// origin and ownership handover complete automatically.
pub fn pending_handoff(from: &str, lane: &str, account: &str, opens: Option<String>, terminal: bool, keep: bool, was_pinned: bool) {
    if from.is_empty() || account.is_empty() {
        return;
    }
    let mut at = crate::util::now_ms();
    if let Err(e) = change(|s| {
        while s.handovers.contains_key(&at) {
            at += 1;
        }
        s.pending.push(Pending { from: from.to_string(), how: OriginHow::Fork, lane: lane.to_string(), account: account.to_string(), at, helper: false, report: false, opens, terminal });
        s.handovers.insert(at, PendingHandover { keep, was_pinned });
        true
    }) {
        eprintln!("Couldn't note pending handoff from {from} to {account}: {e:#}");
    }
}

/// Notes that `copy`, the copy its harness made of the session `key` as it went on for a message
/// from the session that started `key` (send.rs `wake`), carries on that start: the copy does the
/// work `key` was started for, so a later message from that session resumes it in turn, and a
/// helper's session goes on being told of it. Noted as a fork of `key` alone (start.rs `resume`),
/// it would be neither. Nothing where `key` wasn't such a start.
pub fn carry_start(copy: &str, key: &str) {
    if let Err(e) = change(|s| s.carry_start(copy, key, crate::util::now_ms())) {
        eprintln!("Couldn't note that {copy} carries on {key}'s start: {e:#}");
    }
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

/// Stops waiting for the session of the start made `at`: it was stopped before it made one
/// (starting.rs `stop`), so nothing will.
pub fn forget_start(at: i64) -> Result<()> {
    change(|s| {
        let before = s.pending.len();
        s.pending.retain(|p| p.at != at);
        s.pending.len() != before
    })
}

/// Notes where a start came from: the session that asked for it (`StartSpec::from`), by the new
/// session's key when the start knew it, else as a start waiting for its session (`Pending`,
/// `resolve`). `StartSpec::helper`: it was started as a helper of that session, which
/// `StartSpec::report` says is told what it does. A start in a terminal from no session (yours,
/// in Codealong) waits too, with no `from`, only so it's said when it's stuck (`stuck_notices`):
/// it notes no origin.
pub fn started(spec: &crate::start::StartSpec, key: Option<&str>, terminal: bool) {
    let from = spec.from.as_deref().unwrap_or_default();
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

/// A worktree's name on the board, or its path where the board has no such worktree.
pub fn place_of(board: &crate::model::Board, lane: &str) -> String {
    board.lanes.iter().find(|l| l.id == lane).map(|l| l.callsign.clone()).unwrap_or_else(|| crate::util::tilde(Path::new(lane)))
}

/// A notice for each start in a terminal that's stuck (`Pending::stuck`): it has no session on
/// the board yet, and its terminal may be behind your windows. What its terminal shows, where
/// that was read (`screen`, by the start's time: starting.rs `Screens`), goes with it; else it
/// guesses. Each has a key of its own (`start:` and when it was started), so dismissing one
/// doesn't hide the next, even in the same worktree, and goes once its session shows. One a
/// session on the board asked for has none while that session needs you, for it (starting.rs
/// `overlay`) or for something of its own, its line under the session answering it; one whose
/// session is taking a turn, which isn't laid over, or is put away, has its notice.
pub fn stuck_notices(pending: &[Pending], board: &crate::model::Board, word_of: impl Fn(&str) -> String, screen: impl Fn(i64) -> Option<String>, now: i64) -> Vec<crate::model::Notice> {
    let carried = |p: &Pending| board.session(&p.from).is_some_and(|(_, s)| !s.archived && s.status != crate::model::Status::Working);
    pending
        .iter()
        .filter(|p| p.stuck(now) && !carried(p))
        .map(|p| {
            let place = place_of(board, &p.lane);
            let word = word_of(&p.account);
            let asker = board.session(&p.from).map(|(_, s)| crate::send::quoted(&s.title)).unwrap_or_else(|| "another session".into());
            let by = match (p.from.is_empty(), p.helper) {
                (true, _) => String::new(),
                (false, true) => format!(" as a helper of {asker},"),
                (false, false) => format!(" by {asker},"),
            };
            let screen = screen(p.at);
            let waits = if screen.is_some() {
                "its terminal shows this, and waits on it to be answered before it starts"
            } else {
                "its terminal may be waiting on a screen of its own before it starts, such as an update offer or a question about its background server"
            };
            let text = format!(
                "{word} was started in {terminal} {mins} minutes ago, in {place},{by} and hasn't made its session yet: {waits}. Answer it in that tab; if you closed it, dismiss this.",
                terminal = board.apps.terminal,
                mins = p.minutes(now),
            );
            crate::model::Notice { key: format!("start:{}", p.at), title: format!("{word} hasn't started in {place}"), text, screen, ..Default::default() }
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

/// Matches pending starts to the sessions on the board, writes down what matched, and returns
/// any pending moves whose ownership handover (`todos`, labels, pins, schedules) should now run.
pub fn resolve_pending_handovers(sessions: &[Candidate], lanes: &dyn Lanes, now: i64) -> (bool, Vec<ResolvedHandover>) {
    if current().pending.is_empty() {
        return (false, Vec::new());
    }
    let mut changed = false;
    let mut handovers = Vec::new();
    if let Err(e) = change(|s| {
        let (c, h) = resolve_with_handovers(s, sessions, lanes, now);
        changed = c;
        handovers = h;
        changed
    }) {
        eprintln!("Couldn't note where new sessions came from: {e:#}");
    }
    (changed, handovers)
}

/// Matches pending starts to the sessions on the board and writes down what matched.
#[allow(dead_code)]
pub fn resolve_pending(sessions: &[Candidate], lanes: &dyn Lanes, now: i64) -> bool {
    resolve_pending_handovers(sessions, lanes, now).0
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
    use crate::model::{Board, Session, Status};

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
    fn a_session_moved_twice_goes_on_in_its_newest_copy_still_there() {
        let mut store = Store::default();
        let mut entry = |key: &str, at| store.sessions.insert(key.into(), Entry { from: "claude:claude:a".into(), how: OriginHow::Fork, at, helper: false, report: false, known: false });
        entry("claude:claude2:b", 10);
        entry("claude:claude3:c", 20);
        assert_eq!(store.moved_to("claude:claude:a", |_| true).as_deref(), Some("claude:claude3:c"));
        assert_eq!(store.moved_to("claude:claude:a", |k| k != "claude:claude3:c").as_deref(), Some("claude:claude2:b"));
        assert_eq!(store.moved_to("claude:claude:a", |k| k != "claude:claude2:b").as_deref(), Some("claude:claude3:c"));
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
        // Every move, each after the one that made the session it moved from.
        assert_eq!(store.moves(), vec![("claude:claude:a".to_string(), "claude:claude2:b".to_string()), ("claude:claude2:b".to_string(), "claude:claude:c".to_string())]);
        assert!(is_move("claude:claude:a", "claude:claude2:b") && is_move("claude:claude:a", "codex:codex:b") && !is_move("claude:claude:a", "claude:claude:b"));

        // A cross-provider move follows `moved_to`, while a kept handoff copy (`kept`) does not.
        store.sessions.insert("codex:codex:m".into(), Entry { from: "claude:claude:c".into(), how: OriginHow::Fork, at: 40, helper: false, report: false, known: false });
        assert_eq!(store.moved_to("claude:claude:c", all).as_deref(), Some("codex:codex:m"));
        store.kept.insert("codex:codex:m".into());
        assert_eq!(store.moved_to("claude:claude:c", all), None);
    }

    #[test]
    fn pending_handoffs_resolve_to_moved_handover_or_kept_copy() {
        let mut store = Store::default();
        store.pending.push(Pending { how: OriginHow::Fork, opens: opening("# Handoff from \"Fix\""), terminal: true, ..pending("claude:claude:src", "/r/app", "codex", 1_000) });
        store.handovers.insert(1_000, PendingHandover { keep: false, was_pinned: true });
        store.pending.push(Pending { how: OriginHow::Fork, opens: opening("# Handoff from \"Keep\""), terminal: true, ..pending("claude:claude:src2", "/r/app", "antigravity", 1_001) });
        store.handovers.insert(1_001, PendingHandover { keep: true, was_pinned: false });

        let c1 = Candidate { key: "codex:codex:new1", lane: "/r/app", account: "codex", started: 1_500, created: Some(1_500), prompt: Some("# Handoff from \"Fix\""), launched: Some(1_200) };
        let c2 = Candidate { key: "antigravity:antigravity:new2", lane: "/r/app", account: "antigravity", started: 1_600, created: Some(1_600), prompt: Some("# Handoff from \"Keep\""), launched: Some(1_300) };
        let (changed, handovers) = resolve_with_handovers(&mut store, &[c1, c2], &Repos, 2_000);
        assert!(changed);
        assert_eq!(handovers, vec![ResolvedHandover { from: "claude:claude:src".into(), copy: "codex:codex:new1".into(), was_pinned: true }]);
        assert_eq!(store.moved_to("claude:claude:src", |_| true).as_deref(), Some("codex:codex:new1"));
        assert_eq!(store.moved_to("claude:claude:src2", |_| true), None, "kept copy does not mark source moved");
        assert!(store.origin_of("antigravity:antigravity:new2").is_some(), "kept copy still links to source in lineage");
    }

    /// A copy its harness made of a session as it went on for the session that started it is
    /// that session's start, helper and all, where a fork is noted as the resume noted it.
    #[test]
    fn a_copy_that_goes_on_for_its_starter_carries_its_start() {
        let mut store = Store::default();
        let entry = |from: &str, how, helper, known| Entry { from: from.into(), how, at: 10, helper, report: helper, known };
        store.sessions.insert("claude:claude2:job".into(), entry("claude:claude:boss", OriginHow::Started, true, true));
        // The resume noted the copy as a fork first (start.rs `resume`).
        store.sessions.insert("claude:claude2:copy".into(), entry("claude:claude2:job", OriginHow::Fork, false, true));
        assert!(store.carry_start("claude:claude2:copy", "claude:claude2:job", 20));
        assert_eq!(store.known_starter_of("claude:claude2:copy"), Some("claude:claude:boss"));
        assert_eq!(store.helper_of("claude:claude2:copy"), Some(("claude:claude:boss", true)));
        assert!(!store.carry_start("claude:claude2:copy", "claude:claude2:job", 30));
        // Not from a start matched afterwards, a fork, or a session nothing started.
        store.sessions.insert("claude:claude2:matched".into(), entry("claude:claude:boss", OriginHow::Started, false, false));
        store.sessions.insert("claude:claude2:fork".into(), entry("claude:claude:boss", OriginHow::Fork, false, true));
        for key in ["claude:claude2:matched", "claude:claude2:fork", "claude:claude2:alone"] {
            assert!(!store.carry_start("claude:claude2:other", key, 20), "{key}");
        }
        assert_eq!(store.origin_of("claude:claude2:other"), None);
    }

    #[test]
    fn a_fork_of_a_helper_stays_a_helper_of_its_parent() {
        let mut store = Store::default();
        let entry = |from: &str, how, helper, known| Entry { from: from.into(), how, at: 10, helper, report: helper, known };
        store.sessions.insert("claude:claude-work:helper".into(), entry("claude:claude2:boss", OriginHow::Started, true, true));
        store.sessions.insert("claude:claude-work:fork1".into(), entry("claude:claude-work:helper", OriginHow::Fork, false, true));
        store.sessions.insert("claude:claude-work:fork2".into(), entry("claude:claude-work:fork1", OriginHow::Fork, false, true));
        assert_eq!(store.helper_of("claude:claude-work:fork1"), Some(("claude:claude2:boss", true)));
        assert_eq!(store.helper_of("claude:claude-work:fork2"), Some(("claude:claude2:boss", true)));
        assert_eq!(store.known_helper_of("claude:claude-work:fork1"), Some("claude:claude2:boss"));
        // A fork of a non-helper stays non-helper, and cycles terminate safely.
        store.sessions.insert("claude:claude2:plain".into(), entry("claude:claude2:boss", OriginHow::Started, false, true));
        store.sessions.insert("claude:claude2:plain-fork".into(), entry("claude:claude2:plain", OriginHow::Fork, false, true));
        assert_eq!(store.helper_of("claude:claude2:plain-fork"), None);
        store.sessions.insert("claude:claude:cyc1".into(), entry("claude:claude:cyc2", OriginHow::Fork, false, true));
        store.sessions.insert("claude:claude:cyc2".into(), entry("claude:claude:cyc1", OriginHow::Fork, false, true));
        assert_eq!(store.helper_of("claude:claude:cyc1"), None);
        assert_eq!(store.known_helper_of("claude:claude:cyc1"), None);
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
        let start = || Pending { helper: true, report: true, terminal: true, opens: opening(PROMPT), ..pending("claude:claude:boss", "/r/app", "codex", AT) };
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
        // A background-service start (not in a terminal) with a prompt never marks itself stuck
        // and stops waiting after PENDING_MS instead of lingering for a full day.
        let bg = Pending { helper: true, report: true, terminal: false, opens: opening(PROMPT), ..pending("claude:claude:boss", "/r/app", "claude-work", AT) };
        assert!(!bg.stuck(AT + STUCK_MS));
        let mut bg_store = Store { pending: vec![bg], ..Default::default() };
        assert!(!resolve(&mut bg_store, &[], &Repos, AT + PENDING_MS - 1));
        assert!(resolve(&mut bg_store, &[], &Repos, AT + PENDING_MS + 1));
        assert!(bg_store.pending.is_empty());
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
        let mut board = Board { lanes: vec![crate::fixtures::lane("/r/app", vec![boss.clone()])], ..Board::default() };
        board.apps.terminal = "Ghostty".into();
        let word = |_: &str| "Codex".to_string();
        let unread = |_: i64| None;
        // The session that asked is on the board, and needs you for it there: no notice.
        board.lanes[0].sessions[0].status = Status::Needs;
        assert!(stuck_notices(&[start.clone()], &board, word, unread, AT + 66 * 60_000).is_empty());
        // Taking a turn, it isn't laid over, so the notice says it; and put away, it's no longer
        // where you'd look.
        board.lanes[0].sessions[0].status = Status::Working;
        assert_eq!(stuck_notices(&[start.clone()], &board, word, unread, AT + 66 * 60_000).len(), 1);
        board.lanes[0].sessions[0].status = Status::Needs;
        board.lanes[0].sessions[0].archived = true;
        assert!(stuck_notices(&[start.clone()], &board, word, unread, AT + 60_000).is_empty());
        let said = stuck_notices(&[start.clone()], &board, word, unread, AT + 66 * 60_000);
        assert_eq!((said.len(), said[0].key.as_str(), said[0].title.as_str()), (1, "start:1000000", format!("Codex hasn't started in {}", board.lanes[0].callsign).as_str()));
        // Two stuck in one worktree are two notices, one dismissed apart from the other.
        let again = Pending { at: AT + 60_000, ..start.clone() };
        let two = stuck_notices(&[start.clone(), again], &board, word, unread, AT + 66 * 60_000);
        assert!(two.len() == 2 && two[0].key != two[1].key);
        assert!(said[0].text.starts_with(&format!("Codex was started in Ghostty 66 minutes ago, in {}, as a helper of “Dashboards”, and hasn't made its session yet: its terminal may be waiting on a screen of its own", board.lanes[0].callsign)), "{}", said[0].text);
        assert_eq!(said[0].screen, None);
        // Where its terminal was read, it says what that shows rather than guessing
        // (2026-11-15: Codex's review of a repo's hooks).
        let read = stuck_notices(&[start.clone()], &board, word, |at| (at == AT).then(|| "Hooks need review\n› 1. Review hooks".to_string()), AT + 5 * 60_000);
        assert_eq!(read[0].screen.as_deref(), Some("Hooks need review\n› 1. Review hooks"));
        assert!(read[0].text.ends_with("hasn't made its session yet: its terminal shows this, and waits on it to be answered before it starts. Answer it in that tab; if you closed it, dismiss this."), "{}", read[0].text);
        // Started by a session the board doesn't show, in a folder it doesn't either; and by you.
        let other = Pending { helper: false, lane: "/r/gone".into(), from: "claude:claude:gone".into(), ..start.clone() };
        assert!(stuck_notices(&[other], &board, word, unread, AT + STUCK_MS)[0].text.contains("in /r/gone, by another session, and hasn't"));
        let yours = Pending { helper: false, report: false, from: String::new(), ..start };
        let said = stuck_notices(&[yours.clone()], &board, word, unread, AT + STUCK_MS);
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

    #[test]
    fn move_index_matches_store_moved_to() {
        let entry = |from: &str, how: OriginHow, at: i64| Entry { from: from.into(), how, at, helper: false, report: false, known: true };
        let mut store = Store::default();
        assert!(store.move_index().is_empty());
        // Multi-hop move (a -> b -> c), cycle back (c -> a), tie at same timestamp, missing copy,
        // same-account fork, and a non-fork start across accounts.
        store.sessions.insert("claude:two:b".into(), entry("claude:one:a", OriginHow::Fork, 10));
        store.sessions.insert("claude:three:c".into(), entry("claude:two:b", OriginHow::Fork, 20));
        store.sessions.insert("claude:one:a".into(), entry("claude:three:c", OriginHow::Fork, 30));
        store.sessions.insert("claude:two:older".into(), entry("claude:one:a", OriginHow::Fork, 5));
        store.sessions.insert("claude:four:gone".into(), entry("claude:three:c", OriginHow::Fork, 40));
        store.sessions.insert("claude:one:same".into(), entry("claude:one:a", OriginHow::Fork, 50));
        store.sessions.insert("claude:two:started".into(), entry("claude:one:a", OriginHow::Started, 60));
        store.sessions.insert("claude:two:tie1".into(), entry("claude:one:t", OriginHow::Fork, 10));
        store.sessions.insert("claude:two:tie2".into(), entry("claude:one:t", OriginHow::Fork, 10));

        let index = store.move_index();
        assert!(!index.is_empty());
        let exists = |k: &str| k != "claude:four:gone";
        for key in ["claude:one:a", "claude:two:b", "claude:three:c", "claude:one:t", "claude:one:none"] {
            assert_eq!(index.moved_to(key, exists), store.moved_to(key, exists), "mismatch for {key}");
        }
        assert_eq!(index.moved_to("claude:one:a", exists).as_deref(), Some("claude:three:c"));
        assert_eq!(index.moved_to("claude:one:a", |k| k == "claude:two:b").as_deref(), Some("claude:two:b"));
        assert_eq!(index.moved_to("claude:one:a", |_| false), None);
        assert_eq!(index.moved_to("claude:one:t", exists).as_deref(), Some("claude:two:tie2"));
    }
}
