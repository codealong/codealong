//! Telling a PR's own session what happens on it, for the PRs it's turned on for: by you, from
//! the PR's card or menu, or by an agent with `codealong pr nudge` (both write pr-links.json,
//! prlinks.rs), or for every PR of yours by `[pull_requests] nudge_by_default`, unless turned off
//! for that one; a PR says so while it's open (`PrItem::nudge`). Every board the engine publishes
//! is looked at for such PRs with something new on GitHub: approved, changes asked for, reviewed
//! or commented on by someone else (read from the PR's detail once its `updatedAt` moves), merged
//! or closed, with checks that just failed, or with a branch that just came to conflict with its
//! base (someone merged into the base what it conflicts with). The PR's primary session (`PrItem::primary`: the
//! one picked, else the one that pushed to it most), when it runs here, is sent one message saying
//! what happened, as Codealong's own (send.rs `tell`: its harness's way in for other sessions'
//! messages, else typed into its terminal), on a thread of its own, since reading GitHub and a
//! send take a moment. One here that isn't running is resumed out of sight with it
//! (`send::tell_waking`) by `[sessions] resume_for_pr_news`, and told then only what asks for work
//! (`for_stopped`), unless another of the PR's sessions runs (`stopped_primaries`). The PR's other
//! sessions (helpers, reviewers, ones that came to its branch) are
//! told nothing. What you did to the PR yourself, which is what its agent did through `gh` too, is
//! never told back. `[pull_requests] nudge_about` can keep it to what asks for work (`keep`):
//! comments and reviews that ask for changes or say something, and failing checks and conflicts or
//! a merge if it says so, never an approval or a close. An agent's review that Codealong posted from someone's
//! account (autoreview.rs `signed`), as a comment or submitted as their review, is told as their
//! agent's, never as their own word (`agent_wrote`): the session would otherwise take it for
//! that person's say.
//!
//! What's seen on the board and what's been told are kept apart: an update is owed to each
//! session from the board it shows on until a send to that session goes through. A session
//! waiting on you to answer a prompt isn't sent anything (what's typed would answer it), and is
//! told once it isn't; a send that fails, or a PR whose news can't be read from GitHub, is tried
//! again a little later, each time later than the last, a few times before it's given up on.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::Mutex;

use crate::config::NudgeAbout;
use crate::engine::Handle;
use crate::github::PrDetail;
use crate::model::{Board, Lane, Merge, PrItem, PrState, Session};
use serde::{Deserialize, Serialize};

/// What a PR on the board stood at when last seen, for telling only what's new.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Seen {
    on: bool,
    updated_at: i64,
    state: PrState,
    failed: bool,
    /// GitHub last said its branch conflicts with its base (`Merge::Conflicts`).
    conflicts: bool,
}

/// What's new on a PR since the last board, or since a session was last told, to tell it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Why {
    /// Its `updatedAt` moved: read what others did on it since this time (ms) from its detail.
    pub since: Option<i64>,
    /// It's approved now and wasn't told so: it became approved, or the nudge was turned on for
    /// a PR approved already.
    pub approved: bool,
    /// Changes were asked for, by its review decision.
    pub changes: bool,
    /// It was merged or closed.
    pub ended: Option<PrState>,
    /// Checks that just failed, by name.
    pub failed: Vec<String>,
    /// Its branch came to conflict with its base.
    pub conflicts: bool,
}

impl Why {
    /// Adds what's new since to what's still to tell: news from the earliest time, and every
    /// change of state.
    fn add(&mut self, more: Why) {
        self.since = match (self.since, more.since) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        self.approved |= more.approved;
        self.changes |= more.changes;
        self.ended = more.ended.or(self.ended);
        self.conflicts |= more.conflicts;
        for check in more.failed {
            if !self.failed.contains(&check) {
                self.failed.push(check);
            }
        }
    }
}

/// What to tell one session about one PR.
#[derive(Debug, Clone, PartialEq)]
pub struct Job {
    pub pr: PrItem,
    pub why: Why,
    pub session: String,
    /// The last try: when the PR's news can't be read from GitHub, what the board knows is told
    /// rather than nothing.
    pub last: bool,
    /// What it's told of (`[pull_requests] nudge_about`), as the board said when it was due, and
    /// only what asks for work where it isn't running (`for_stopped`).
    pub about: NudgeAbout,
    /// Its session isn't running, so it's resumed to be told (`Board::pr_stopped`,
    /// `send::tell_waking`).
    pub stopped: bool,
}

/// What a nudge tells a session that isn't running, which it's resumed for: what asks for work,
/// as `feedback-and-checks` has it, or less where `about` says less. An approval, a merge or a
/// close is no reason to start a session that stopped.
fn for_stopped(about: NudgeAbout) -> NudgeAbout {
    match about {
        NudgeAbout::Everything | NudgeAbout::FeedbackChecksAndMerges => NudgeAbout::FeedbackAndChecks,
        NudgeAbout::FeedbackAndMerges => NudgeAbout::Feedback,
        about => about,
    }
}

/// What becomes of a PR's news while its primary session, a session on this Mac, isn't running
/// (`stopped_primaries`): what `due` goes by, and what the PR's card says under its switch.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Stopped {
    /// It's resumed out of sight to be told, or told where its harness takes a message while it
    /// doesn't run (send.rs `tell_waking`).
    Resumes,
    /// Not while another of the PR's sessions runs.
    Beside,
    /// Not at all.
    Never,
}

/// What becomes of each nudged PR's news while its primary session, on this Mac, isn't running,
/// by the PR's key (`Board::pr_stopped`). It's resumed for it (`[sessions] resume_for_pr_news`,
/// `on`), or reached where its harness takes a message while it doesn't run (`reaches`, send.rs
/// `reaches_stopped`); but not while another of the PR's sessions runs here, or a session runs on
/// its branch on another device of yours that's connected (`elsewhere`, `running_elsewhere`),
/// unless that one is a helper or a review (`Session::helper`), since the PR's work may have gone
/// on there and two agents on one branch get in each other's way; and never with the setting off,
/// for one you archived or marked done, or one that goes on only with someone at this Mac (a
/// Claude app session). `reaches` is asked only where nothing else rules it out.
pub fn stopped_primaries(board: &Board, elsewhere: &BTreeSet<String>, on: bool, reaches: impl Fn(&Session) -> bool) -> BTreeMap<String, Stopped> {
    let runs = |k: &str| board.session(k).is_some_and(|(_, s)| s.live && !s.archived && s.helper.is_none());
    let stopped = |pr: &PrItem| {
        let key = pr.primary.as_deref()?;
        let (_, s) = board.session(key).filter(|(lane, s)| lane.device.is_none() && !s.live)?;
        Some(if !on || s.archived {
            Stopped::Never
        } else if elsewhere.contains(&crate::prlinks::pr_key(&pr.key)) || pr.sessions.iter().any(|o| o.key != key && runs(&o.key)) {
            Stopped::Beside
        } else if reaches(s) {
            Stopped::Resumes
        } else {
            Stopped::Never
        })
    };
    board.prs.iter().filter(|pr| pr.nudge).filter_map(|pr| Some((pr.key.clone(), stopped(pr)?))).collect()
}

/// The PRs another of your devices runs a session on, by `prlinks::pr_key`: a worktree there on
/// the PR's branch (its lane's PR) with a session running that isn't a helper or a review, while
/// that device is connected. This Mac's board doesn't list another device's sessions (the windows
/// get them as peers.rs `DevicesView::lanes`), so the engine is handed these (`stopped_primaries`).
pub fn running_elsewhere(lanes: &[Lane]) -> BTreeSet<String> {
    let runs = |l: &&Lane| l.device.as_ref().is_some_and(|d| d.online) && l.sessions.iter().any(|s| s.live && !s.archived && s.helper.is_none());
    lanes
        .iter()
        .filter(runs)
        .filter_map(|l| Some((l, l.pr.as_ref()?.number)))
        .flat_map(|(l, number)| [&l.repo_slug, &l.repo_slug_now].into_iter().flatten().map(move |slug| crate::prlinks::pr_key(&format!("{slug}#{number}"))))
        .collect()
}

/// Whether a job for a session that wasn't running still goes, by the board as it is now: what
/// `due` found may have changed while GitHub was read and earlier jobs went (another of the PR's
/// sessions started, `resume_for_pr_news` went off). One that runs again is told as any is.
fn still_due(board: &Board, job: &Job) -> bool {
    let runs = board.session(&job.session).is_some_and(|(lane, s)| lane.device.is_none() && s.live);
    runs || board.prs.iter().any(|p| p.key == job.pr.key && p.primary.as_deref() == Some(job.session.as_str()) && board.pr_stopped.get(&p.key) == Some(&Stopped::Resumes))
}

/// Whether the session `key` runs on this Mac, which you haven't archived.
fn runs_here(board: &Board, key: &str) -> bool {
    board.session(key).is_some_and(|(lane, s)| lane.device.is_none() && s.live && !s.archived)
}

/// The session a PR's primary `key` is told as: the copy it went on as, a while after it was
/// resumed as one (`Nudger::went_on_as`), else itself.
fn goes_to(copies: &HashMap<String, (String, i64)>, key: &str) -> String {
    copies.get(key).map_or(key, |(copy, _)| copy.as_str()).to_string()
}

/// How long news for a primary session resumed as a copy goes to the copy (`went_on_as`): long
/// past the copy's showing on the board and becoming its PRs' primary (`follow`).
const COPY_MS: i64 = 10 * 60_000;

/// What of `why` a nudge tells of, as `[pull_requests] nudge_about` says: everything; or only
/// what asks for work, the reviews and comments its detail has (`since`, which `words` sorts) and
/// changes asked for, with failing checks and conflicts where it says `checks`. Never then an
/// approval or a close, nor anything else about a PR that's no longer `open` than its merge, where
/// it says `merges`.
fn keep(about: NudgeAbout, open: bool, why: Why) -> Why {
    let checks = about.checks();
    match about {
        NudgeAbout::Everything => why,
        _ if !open => Why { ended: why.ended.filter(|s| about.merges() && *s == PrState::Merged), ..Why::default() },
        _ => Why { approved: false, ended: None, failed: if checks { why.failed } else { Vec::new() }, conflicts: checks && why.conflicts, ..why },
    }
}

/// How many times a send is tried before it's given up on.
const TRIES: u32 = 6;

/// How long after a failed send it's tried again: the first wait, doubled each time, up to the
/// longest. Six tries span about a quarter of an hour.
const RETRY_MS: i64 = 30_000;
const RETRY_MAX_MS: i64 = 10 * 60_000;

/// What a session is still to be told about a PR.
#[derive(Debug)]
struct Owed {
    /// The PR as the board last listed it.
    pr: PrItem,
    /// What hasn't gone to a send yet.
    why: Why,
    /// What's gone to a send that hasn't said how it went.
    sending: Option<Why>,
    /// Sends that failed in a row.
    failures: u32,
    /// Not tried again before this (ms).
    next_at: i64,
}

/// What each PR on the board stood at when last seen, so each update is noticed once, as it
/// happens, and what each session is still to be told.
#[derive(Default)]
pub struct Nudger {
    seen: HashMap<String, Seen>,
    /// By PR (`prlinks::pr_key`) and session.
    owed: HashMap<(String, String), Owed>,
    /// Sessions resumed as a copy, each with its copy and when (`went_on_as`).
    copies: HashMap<String, (String, i64)>,
}

impl Nudger {
    /// The jobs due now, at `now` (ms). For each PR with its nudge on that the last board showed
    /// otherwise, what's new is owed to its primary session when that runs on this Mac, and what
    /// asks for work when it's to be resumed for it (`Board::pr_stopped`), or to the copy it went
    /// on as (`went_on_as`); then each session owed something is sent it, one job each, unless it
    /// waits on you, a send to it is under way, the board doesn't have it yet, or a
    /// failed one's wait isn't over. A PR first seen is only noted, so turning the nudge on tells
    /// of what happens from then on (and of an approval it has already, when turned on for that
    /// PR: turning `nudge_by_default` on doesn't tell every approved PR of yours to merge); a PR
    /// the board no longer lists is forgotten, though what's owed for it is still told.
    pub fn due(&mut self, board: &Board, now: i64) -> Vec<Job> {
        let resumes = |pr: &PrItem| board.pr_stopped.get(&pr.key) == Some(&Stopped::Resumes);
        self.copies.retain(|_, (_, at)| now - *at < COPY_MS);
        let mut seen_now = HashMap::with_capacity(board.prs.len());
        for pr in &board.prs {
            let key = crate::prlinks::pr_key(&pr.key);
            let was = self.seen.get(&key).copied();
            // GitHub says it doesn't know while it works mergeability out again, after the base
            // moved: a conflict it found stands until it says otherwise, so one conflict is told
            // once, however often the base moves under it.
            let conflicts = match pr.merge {
                Merge::Conflicts => true,
                Merge::Unknown => was.is_some_and(|w| w.conflicts),
                _ => false,
            };
            let seen = Seen { on: pr.nudge, updated_at: pr.updated_at, state: pr.state, failed: pr.checks.failed > 0, conflicts };
            seen_now.insert(key.clone(), seen);
            for ((pr_key, _), owed) in self.owed.iter_mut() {
                if *pr_key == key {
                    owed.pr = pr.clone();
                    // A conflict resolved before it could be told (its session waited on you, a
                    // send failed) is no news, though GitHub may not know again by the time it
                    // could be.
                    owed.why.conflicts &= conflicts;
                }
            }
            let Some(was) = was else { continue };
            // The board says the nudge only while a PR is open: one that just ended is still
            // told of that.
            let ended = was.on && was.state.is_open() && !pr.state.is_open();
            if !(pr.nudge || ended) {
                continue;
            }
            let why = Why {
                since: (was.on && pr.updated_at > was.updated_at).then_some(was.updated_at),
                approved: pr.state == PrState::Approved && (was.state != PrState::Approved || (!was.on && !pr.nudge_by_default)),
                changes: was.on && pr.state == PrState::Changes && was.state != PrState::Changes,
                ended: ended.then_some(pr.state),
                failed: if was.on && seen.failed && !was.failed { pr.checks.failing.clone() } else { Vec::new() },
                conflicts: was.on && conflicts && !was.conflicts,
            };
            let about = board.repo_settings.nudge_about(board.nudge_about, pr);
            let why = keep(about, pr.state.is_open(), why);
            if why == Why::default() {
                continue;
            }
            let Some(primary) = pr.primary.as_deref() else { continue };
            let s = goes_to(&self.copies, primary);
            if board.automation_blocked(&s) { continue; }
            // One that isn't running is resumed only for what asks for work; the copy one went on
            // as is told as it runs, once the board has it.
            let why = if runs_here(board, &s) || s != primary {
                why
            } else if resumes(pr) {
                keep(for_stopped(about), pr.state.is_open(), why)
            } else {
                continue;
            };
            if why != Why::default() {
                let owed = self.owed.entry((key.clone(), s)).or_insert_with(|| Owed { pr: pr.clone(), why: Why::default(), sending: None, failures: 0, next_at: 0 });
                owed.why.add(why);
            }
        }
        self.seen = seen_now;
        self.more(board, now)
    }

    /// The jobs due now of what's owed (`due`), by `board`, without looking for what's new on it:
    /// what one job at a time held back, which the thread that sent the last ones asks for as it
    /// ends (`send_now`) rather than waiting for a board that changed.
    pub fn more(&mut self, board: &Board, now: i64) -> Vec<Job> {
        let resumes = |pr: &PrItem| board.pr_stopped.get(&pr.key) == Some(&Stopped::Resumes);
        self.copies.retain(|_, (_, at)| now - *at < COPY_MS);
        let copies = &self.copies;
        let runs_here = |k: &str| runs_here(board, k);
        // A session that stopped is told nothing more unless it's resumed for it, nor one whose PR
        // had its nudge turned off, nor one that's no longer its PR's primary. A copy a primary
        // went on as is waited for while the board doesn't have it.
        let off: HashSet<String> = board.prs.iter().filter(|p| p.state.is_open() && !p.nudge).map(|p| crate::prlinks::pr_key(&p.key)).collect();
        let listed: HashMap<String, &PrItem> = board.prs.iter().map(|p| (crate::prlinks::pr_key(&p.key), p)).collect();
        let waited_for = |s: &str| copies.values().any(|(copy, _)| copy == s) && board.session(s).is_none();
        let reached = |p: Option<&&PrItem>, s: &str| runs_here(s) || waited_for(s) || p.is_some_and(|p| p.primary.as_deref() == Some(s) && resumes(p));
        let primary = |p: &PrItem, s: &str| p.primary.as_deref().map(|k| goes_to(copies, k)).as_deref() == Some(s);
        self.owed.retain(|(pr, s), o| {
            let p = listed.get(pr);
            if board.automation_blocked(s) { return o.sending.is_some(); }
            o.sending.is_some() || (reached(p, s) && !off.contains(pr) && p.is_none_or(|p| primary(p, s)))
        });
        // A send already under way to a session is let finish first: two typing into one terminal
        // at once would interleave, and one that isn't running would be resumed twice.
        let mut busy: HashSet<String> = self.owed.iter().filter(|(_, o)| o.sending.is_some()).map(|((_, s), _)| s.clone()).collect();
        let waits = |k: &str| board.session(k).is_some_and(|(_, s)| crate::send::waits_on_you(s));
        let mut out = Vec::new();
        for ((_, s), o) in self.owed.iter_mut() {
            if o.sending.is_some() || o.why == Why::default() || now < o.next_at || busy.contains(s) || waits(s) || board.session(s).is_none() {
                continue;
            }
            busy.insert(s.clone());
            let why = std::mem::take(&mut o.why);
            o.sending = Some(why.clone());
            let (about, stopped) = (board.repo_settings.nudge_about(board.nudge_about, &o.pr), !runs_here(s));
            out.push(Job { pr: o.pr.clone(), why, session: s.clone(), last: o.failures + 1 >= TRIES, about: if stopped { for_stopped(about) } else { about }, stopped });
        }
        out.sort_by(|a, b| (&a.pr.key, &a.session).cmp(&(&b.pr.key, &b.session)));
        out
    }

    /// How a job went, at `now` (ms). What was told is owed no longer; what wasn't is owed again,
    /// with what's come in since, and tried again later. Whether it will be.
    pub fn told(&mut self, job: &Job, ok: bool, now: i64) -> bool {
        let mut key = (crate::prlinks::pr_key(&job.pr.key), job.session.clone());
        // Its session went on as a copy meanwhile, and what it was owed with it (`went_on_as`).
        if let Some((copy, _)) = self.copies.get(&job.session).filter(|_| !self.owed.contains_key(&key)) {
            key.1 = copy.clone();
        }
        let Some(o) = self.owed.get_mut(&key) else { return false };
        let sent = o.sending.take().unwrap_or_default();
        let again = !ok && o.failures + 1 < TRIES;
        if again {
            o.failures += 1;
            o.next_at = now + (RETRY_MS << (o.failures - 1).min(10)).min(RETRY_MAX_MS);
            let mut why = sent;
            why.add(std::mem::take(&mut o.why));
            o.why = why;
        } else {
            // Told, or given up on: what's come in since starts afresh.
            o.failures = 0;
            o.next_at = 0;
            if o.why == Why::default() {
                self.owed.remove(&key);
            }
        }
        again
    }

    /// The session `from` went on as `copy` as it was resumed (Claude Code copies a session a
    /// background job had into a new job), which `follow` makes the primary of its PRs in its
    /// place: what it's owed, and what comes for those PRs a while from now, is the copy's, which
    /// is waited for while the board doesn't have it.
    pub fn went_on_as(&mut self, from: &str, copy: &str, now: i64) {
        self.copies.insert(from.to_string(), (copy.to_string(), now));
        let owed: Vec<(String, String)> = self.owed.keys().filter(|(_, s)| s == from).cloned().collect();
        for (pr, s) in owed {
            let Some(o) = self.owed.remove(&(pr.clone(), s)) else { continue };
            let to = self.owed.entry((pr, copy.to_string())).or_insert_with(|| Owed { pr: o.pr.clone(), why: Why::default(), sending: None, failures: 0, next_at: 0 });
            to.why.add(o.why);
            to.sending = to.sending.take().or(o.sending);
        }
    }
}

/// How much of what someone wrote goes in.
const BODY_CHARS: usize = 600;

/// The agent named by the line autoreview.rs `signed` ends a review with: `<sub>Reviewed 3af044a
/// with Codex · posted via Codealong</sub>`, or the line as it was written before 2026-10-26,
/// `<sub>An agent's review (Codex) of 3af044a, posted by Codealong.</sub>`, which the reviews
/// already on GitHub keep.
fn signed_by(line: &str) -> Option<&str> {
    let now = line.strip_prefix("<sub>Reviewed ").and_then(|r| r.strip_suffix(" via Codealong</sub>")).and_then(|r| r.split_once(" with ")).and_then(|(_, r)| r.rsplit_once(" · "));
    let before = line.strip_prefix("<sub>An agent's review (").filter(|_| line.ends_with("by Codealong.</sub>")).and_then(|r| r.split_once(") of "));
    now.or(before).map(|(agent, _)| agent.trim()).filter(|a| !a.is_empty())
}

/// The agents whose reviews Codealong posted in `body` from its author's account (autoreview.rs
/// `signed`), each named once by the line it signed with, and `body` without those lines and the
/// unseen marks; no agents and `body` itself when it holds none. A review needs a mark as well as
/// the line naming its agent, each at the very start of a line, as Codealong writes them, so
/// nothing in a review's text can hide them: the mistake this leaves is the safer one, telling as
/// an agent's the words of someone who pasted the whole footer at the start of its lines (in a
/// code block, say), never an agent's review as someone's word. A footer quoted within a line or
/// indented isn't one. Where in `body` each review starts isn't told (its text can hold rules of
/// its own, and an edit in Codealong moves the marks to the end), so a body isn't split into
/// whose words are whose.
fn agents_in(body: &str) -> (Vec<String>, String) {
    let (mut marked, mut names, mut kept): (bool, Vec<String>, Vec<&str>) = (false, Vec::new(), Vec::new());
    for line in body.lines() {
        let t = line.trim_end();
        if t.starts_with(crate::github::REVIEW_MARK) && t.ends_with("-->") {
            marked = true;
            continue;
        }
        if let Some(name) = signed_by(t) {
            if !names.iter().any(|n| n == name) {
                names.push(name.to_string());
            }
            continue;
        }
        // A line taken out leaves one blank line where there were two.
        if !(t.is_empty() && kept.last().is_some_and(|l: &&str| l.trim().is_empty())) {
            kept.push(line);
        }
    }
    if !marked || names.is_empty() {
        return (Vec::new(), body.to_string());
    }
    (names, kept.join("\n").trim().to_string())
}

/// The line a message tells of something `author` did (`done`) with `body`. A comment Codealong
/// posted for an agent's review (`whole`: autoreview.rs `comment_body`, nothing of theirs in it)
/// is told as their agent's; a review's text with an agent's review in it may hold words of
/// theirs too (a draft's, or what they wrote as they submitted it), so it's told as theirs with
/// the agent's in it, which isn't their word.
fn told(author: &str, done: &str, body: &str, whole: bool) -> String {
    let (agents, text) = agents_in(body);
    if agents.is_empty() {
        return format!("{author} {done}: “{}”", excerpt(&text));
    }
    let names = agents.join(" and ");
    if whole {
        format!("{author}'s agent ({names}) {done}, from {author}'s account; an agent's review, not {author}'s word: “{}”", excerpt(&text))
    } else {
        format!("{author} {done}, with their agent's review ({names}) in it; the agent's review isn't {author}'s word: “{}”", excerpt(&text))
    }
}

fn excerpt(body: &str) -> String {
    let body = body.trim();
    if body.chars().count() <= BODY_CHARS {
        return body.to_string();
    }
    let cut: String = body.chars().take(BODY_CHARS).collect();
    format!("{}…", cut.trim_end())
}

/// What an open PR's message ends with: the news is no go-ahead to merge.
pub const NOT_A_MERGE: &str = "This message doesn't ask you to merge it: merge it only if you were asked to.";

/// What the session is told: the PR, each new thing on it, where it stands, and that it's the
/// session's to take on, but not to merge: a message that came by itself must
/// never read as your go-ahead to merge (asked 2026-10-09), so an open PR's always says it
/// doesn't ask that. None when nothing new was by someone else (the PR's own agent pushed, say). `detail` is the PR as GitHub has it now, read when `why.since` says to; `login` is who
/// `gh` is signed in as, whose reviews aren't news. Only what `about` tells of is said (`keep`):
/// with feedback only, an approval or a dismissal among the reviews isn't either.
pub fn words(pr: &PrItem, why: &Why, detail: Option<&PrDetail>, login: Option<&str>, about: NudgeAbout) -> Option<String> {
    let why = &keep(about, pr.state.is_open(), why.clone());
    let mut items: Vec<String> = Vec::new();
    let (mut approved_said, mut changes_said) = (false, false);
    if let (Some(since), Some(d)) = (why.since, detail) {
        let fresh = |at: Option<i64>| at.is_some_and(|t| t > since);
        // A review's own words come as a comment at the same time by the same author.
        let mut said: Vec<&str> = Vec::new();
        // With feedback only, an approval or a dismissal isn't told, nor its words: the latest
        // review's, nor an earlier one's (`PrComment::review`), which the detail keeps as a comment.
        let not_feedback = |state: &str| about != NudgeAbout::Everything && matches!(state, "APPROVED" | "DISMISSED");
        for r in d.reviews.iter().filter(|r| fresh(r.at) && Some(r.author.as_str()) != login) {
            let body = d.comments.iter().find(|c| c.author == r.author && c.at == r.at && c.path.is_none()).map(|c| c.id.as_str());
            said.extend(body);
            let words = match r.state.as_str() {
                "APPROVED" => {
                    approved_said = true;
                    "approved it"
                }
                "CHANGES_REQUESTED" => {
                    changes_said = true;
                    "asked for changes"
                }
                "DISMISSED" => "had a review dismissed",
                _ => "reviewed it",
            };
            if not_feedback(&r.state) {
                continue;
            }
            match body.and_then(|id| d.comments.iter().find(|c| c.id == id)) {
                Some(c) => items.push(told(&r.author, words, &c.body, false)),
                None => items.push(format!("{} {words}.", r.author)),
            }
        }
        let mut comments: Vec<_> = d.comments.iter().filter(|c| fresh(c.at) && !c.mine && !said.contains(&c.id.as_str()) && !c.review.as_deref().is_some_and(not_feedback)).collect();
        comments.sort_by_key(|c| c.at);
        // A line comment has no mark of its own: one that went in with a review that has an
        // agent's review in it (`draft_plan` puts its comments on their lines) may be the agent's.
        let agent_reviews: HashMap<&str, String> = d.comments.iter().filter(|c| c.review.is_some()).filter_map(|c| Some((c.id.as_str(), Some(agents_in(&c.body).0.join(" and ")).filter(|n| !n.is_empty())?))).collect();
        for c in comments {
            let done = c.path.as_ref().map_or_else(|| "commented".to_string(), |path| format!("commented on {path}"));
            match c.in_review.as_deref().and_then(|r| agent_reviews.get(r)).filter(|_| c.path.is_some() && agents_in(&c.body).0.is_empty()) {
                Some(names) => items.push(format!("{} {done}, in a review with their agent's review ({names}) in it; it may be the agent's, not {}'s word: “{}”", c.author, c.author, excerpt(&c.body))),
                // A review's own text is told with its review; any other comment with an agent's
                // review in it is one Codealong posted whole.
                None => items.push(told(&c.author, &done, &c.body, c.review.is_none() && c.path.is_none())),
            }
        }
    }
    if why.approved && !approved_said {
        items.push("It was approved.".into());
    }
    if why.changes && !changes_said {
        items.push("Changes were asked for.".into());
    }
    if !why.failed.is_empty() {
        items.push(format!("Its checks failed: {}.", why.failed.join(", ")));
    }
    // A conflict on a PR that ended is no news.
    let conflicts = why.conflicts && why.ended.is_none();
    if conflicts {
        items.push(match pr.base.as_deref() {
            Some(base) => format!("Its branch now conflicts with {base}, its base."),
            None => "Its branch now conflicts with its base.".into(),
        });
    }
    match why.ended {
        Some(PrState::Merged) => items.push("It was merged.".into()),
        Some(_) => items.push("It was closed without being merged.".into()),
        None => {}
    }
    if items.is_empty() {
        return None;
    }
    let whose = if pr.mine { "your PR" } else { "PR" };
    // Where it stands would say the conflict again.
    let standing = if pr.state.is_open() && !conflicts { standing(pr) } else { "" };
    let next = if why.ended.is_some() {
        "Nothing more is wanted on it, unless you were told otherwise.".to_string()
    } else if pr.state == PrState::Approved && !changes_said && !why.changes && why.failed.is_empty() && !conflicts {
        NOT_A_MERGE.to_string()
    } else {
        format!("Take it from there, unless you were told to leave it to someone else. {NOT_A_MERGE}")
    };
    let list: Vec<String> = items.iter().map(|i| format!("- {i}")).collect();
    Some(format!("On {whose} #{} “{}” ({}):\n{}\n{}{next}", pr.number, pr.title.trim(), pr.url, list.join("\n"), standing))
}

/// Where an open PR stands, for the message's last line; empty when there's nothing to say.
fn standing(pr: &PrItem) -> &'static str {
    match (pr.merge, &pr.checks) {
        (Merge::Clean, _) => "It's ready to merge. ",
        (Merge::Conflicts, _) => "Its branch conflicts with its base. ",
        (Merge::Behind, _) => "Its branch is behind its base, which the repo wants brought up to date first. ",
        (_, c) if c.failed > 0 => "Some of its checks failed. ",
        (_, c) if c.pending > 0 => "Its checks are still running. ",
        _ => "",
    }
}

/// The message for a job, given how reading the PR's detail went (None when it wasn't read):
/// None when there's nothing to say, and an error when the detail couldn't be read and this
/// isn't the last try, to try again rather than leave out what others wrote.
fn message(job: &Job, detail: Option<Result<&PrDetail, &str>>, login: Option<&str>) -> Result<Option<String>, String> {
    let detail = match detail {
        Some(Err(e)) if !job.last => return Err(format!("What's new on {} couldn't be read: {e}", job.pr.key)),
        Some(Ok(d)) => Some(d),
        _ => None,
    };
    Ok(words(&job.pr, &job.why, detail, login, job.about))
}

/// Does each job in turn: reads the PR's detail from GitHub where its `updatedAt` moved (once
/// for the jobs that share it), works out the message, and sends it to the session as a window's
/// send does; tells `nudger` how each went, then does what it's owed besides (`Nudger::more`)
/// until nothing is; and tells the engine, which says so in a notice when something didn't go.
pub fn send_now(engine: &Handle, nudger: &Mutex<Nudger>, jobs: Vec<Job>) {
    let read = |pr: &PrItem| crate::github::detail(&pr.repo_slug, pr.number).map_err(|e| e.to_string());
    let went = send_all(engine, nudger, jobs, read, |job, text| deliver(engine, job, text), crate::util::now_ms);
    engine.nudged(went);
}

/// Sends a job's message as a window's send does, by `send::tell`, or `send::tell_waking` for a
/// session that isn't running, if that still goes (`still_due`). The copy it went on as, where it
/// was resumed as one.
fn deliver(engine: &Handle, job: &Job, text: &str) -> Result<Option<String>, String> {
    if engine.read(|b| b.automation_blocked(&job.session)) { return Ok(None); }
    let told = if !job.stopped {
        crate::send::tell(engine, &job.session, text).map(|_| None)
    } else if engine.read(|b| still_due(b, job)) {
        crate::send::tell_waking(engine, &job.session, text).map(|resumed| resumed.and_then(|r| r.key).filter(|k| *k != job.session))
    } else {
        return Ok(None);
    };
    told.map_err(|e| {
        let title = engine.read(|b| b.session(&job.session).map(|(_, s)| s.title.clone())).unwrap_or_else(|| job.session.clone());
        format!("“{title}” wasn't told what's new on {}: {e}", job.pr.key)
    })
}

/// `send_now`, reading a PR's detail with `read`, sending with `tell` and telling the time with
/// `now`, in rounds: the jobs it was handed, then what `Nudger::more` hands out after each round.
/// What the jobs came to, for the notice it leaves (`Outcomes`).
fn send_all(engine: &Handle, nudger: &Mutex<Nudger>, jobs: Vec<Job>, mut read: impl FnMut(&PrItem) -> Result<PrDetail, String>, mut tell: impl FnMut(&Job, &str) -> Result<Option<String>, String>, now: impl Fn() -> i64) -> Result<(), String> {
    let login = engine.read(|b| b.github_login.clone());
    let mut outcomes = Outcomes::default();
    let mut jobs = jobs;
    while !jobs.is_empty() {
        // Read afresh each round: a retry has news its first try didn't, or GitHub answers now.
        let mut details: HashMap<(String, i64), Result<PrDetail, String>> = HashMap::new();
        for job in jobs {
            // One that falls over is told as failed, so its session isn't left waiting on a send
            // that never says how it went.
            let went = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let detail = job.why.since.map(|since| details.entry((job.pr.key.clone(), since)).or_insert_with(|| read(&job.pr)).as_ref().map_err(String::as_str));
                message(&job, detail, login.as_deref()).and_then(|text| match text {
                    Some(text) => tell(&job, &text),
                    None => Ok(None),
                })
            }))
            .unwrap_or_else(|_| Err(format!("Telling a session what's new on {} stopped partway.", job.pr.key)));
            let copy = went.as_ref().ok().cloned().flatten();
            let again = {
                let mut nudger = nudger.lock().unwrap_or_else(|e| e.into_inner());
                let at = now();
                let again = nudger.told(&job, went.is_ok(), at);
                if let Some(copy) = &copy {
                    nudger.went_on_as(&job.session, copy, at);
                }
                again
            };
            if let Some(copy) = copy {
                outcomes.followed(follow(engine, &job.session, &copy));
            }
            outcomes.went(&job, went.err().map(|e| if again { format!("{e} Codealong tries again in a few minutes.") } else { e }));
        }
        // What a session was owed besides, which one job at a time held back, goes now rather than
        // with the next board that changed.
        jobs = nudger.lock().unwrap_or_else(|e| e.into_inner()).more(&engine.board(), now());
    }
    outcomes.notice()
}

/// What a worker's jobs came to, for the notice it leaves (`Handle::nudged`): each PR's last send,
/// so a retry that went takes back the failure before it, whichever session it went to (a copy
/// the PR's primary went on as meanwhile, `Nudger::went_on_as`), and each copy that couldn't be
/// made its PRs' primary (`follow`).
#[derive(Default)]
struct Outcomes {
    sends: BTreeMap<String, String>,
    follows: Vec<String>,
}

impl Outcomes {
    fn went(&mut self, job: &Job, failed: Option<String>) {
        let key = crate::prlinks::pr_key(&job.pr.key);
        match failed {
            Some(e) => self.sends.insert(key, e),
            None => self.sends.remove(&key),
        };
    }

    fn followed(&mut self, went: Result<(), String>) {
        self.follows.extend(went.err());
    }

    fn notice(self) -> Result<(), String> {
        let all: Vec<String> = self.sends.into_values().chain(self.follows).collect();
        if all.is_empty() { Ok(()) } else { Err(all.join(" ")) }
    }
}

/// Makes the copy a session was resumed as (Claude Code copies a session a background job had
/// into a new job) the primary of the PRs it was primary of, in its place, as Primary session in a
/// PR's menu does: the copy carries their work on, and the session it copied, which no longer
/// runs, would otherwise go on being told, and be resumed again beside it.
fn follow(engine: &Handle, from: &str, copy: &str) -> Result<(), String> {
    let (title, prs) = engine.read(|b| (b.session(from).map(|(_, s)| s.title.clone()), b.prs.iter().filter(|p| p.primary.as_deref() == Some(from)).map(|p| p.key.clone()).collect::<Vec<_>>()));
    let now = crate::util::now_ms();
    crate::prlinks::change(|links| prs.iter().fold(false, |changed, pr| links.set_primary(pr, copy, now) || changed))
        .map_err(|e| format!("“{}” went on as {copy}, which couldn't be made the primary session of {} in its place: {e:#}", title.as_deref().unwrap_or(from), prs.join(", ")))?;
    engine.pr_links_changed();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::github::{PrComment, PrReview};
    use crate::model::{Checks, PrSession, Session};

    /// An open PR of yours with its nudge on, last updated at `UPDATED`, whose primary session is
    /// the first of `sessions`.
    fn pr(key: &str, state: PrState, sessions: &[&str]) -> PrItem {
        PrItem {
            key: key.into(),
            repo_slug: key.split('#').next().unwrap().into(),
            repo: "spark".into(),
            project: "Spruce".into(),
            number: key.rsplit('#').next().unwrap().parse().unwrap(),
            title: "Let the agent add photos".into(),
            url: format!("https://github.com/{}/pull/{}", key.split('#').next().unwrap(), key.rsplit('#').next().unwrap()),
            state,
            author: Some("me".into()),
            mine: true,
            review_requested: false,
            requested_by: None,
            requested_at: None,
            reviewers_asked: Vec::new(),
            snoozed_until: None,
            nudge: state.is_open(),
            nudge_by_default: false,
            primary: sessions.first().map(|k| k.to_string()),
            primary_by: None,
            head: "photos".into(),
            base: Some("main".into()),
            checks: Checks::default(),
            merge: Merge::Unknown,
            additions: 1,
            deletions: 0,
            changed_files: 1,
            created_at: 0,
            updated_at: UPDATED,
            merged_at: None,
            closed_at: None,
            lanes: vec!["/work".into()],
            sessions: sessions.iter().map(|k| PrSession { key: (*k).into(), lane: "/work".into(), pushes: 0, primary: None }).collect(),
            head_oid: None,
            authored_by: None,
            labels: Vec::new(),
            auto_review: None,
            last_review: None,
            stale_approval: None,
            turn: None,
            shared_review: Default::default(),
        }
    }

    const UPDATED: i64 = 1_000_000;

    fn off(p: PrItem) -> PrItem {
        PrItem { nudge: false, ..p }
    }

    fn live(key: &str) -> Session {
        Session { live: true, ..crate::fixtures::session(key) }
    }

    fn board(prs: Vec<PrItem>, sessions: Vec<Session>) -> Board {
        Board { prs, lanes: vec![crate::fixtures::lane("/work", sessions)], ..Board::default() }
    }

    const A: &str = "claude:claude:a";
    const B: &str = "claude:claude:b";

    fn comment(id: &str, author: &str, body: &str, at: i64, path: Option<&str>, mine: bool) -> PrComment {
        PrComment { id: id.into(), author: author.into(), body: body.into(), at: Some(at), url: None, path: path.map(String::from), line: None, before: false, outdated: false, replies: 0, resolved: false, mine, review: None, in_review: None }
    }

    fn detail(reviews: Vec<PrReview>, comments: Vec<PrComment>) -> PrDetail {
        PrDetail { body: String::new(), body_html: String::new(), mergeable: None, commits: 1, reviews, requested: vec![], comments, files: vec![], more_files: false }
    }

    /// The jobs due on `b` at `NOW`, each told as sent.
    fn sent(nudger: &mut Nudger, b: &Board) -> Vec<Job> {
        let jobs = nudger.due(b, NOW);
        for job in &jobs {
            assert!(!nudger.told(job, true, NOW));
        }
        jobs
    }

    const NOW: i64 = 1_800_000_000_000;

    #[test]
    fn provider_flags_drop_pr_news_including_news_queued_before_the_flag() {
        let initial = board(vec![pr("o/r#1", PrState::Open, &[A])], vec![live(A)]);
        let mut nudger = Nudger::default();
        assert!(nudger.due(&initial, NOW).is_empty());
        let mut changed = initial.clone();
        changed.prs[0].updated_at += 1;
        changed.lanes[0].sessions[0].status = crate::model::Status::Needs;
        assert!(nudger.due(&changed, NOW + 1).is_empty());
        changed.lanes[0].sessions[0].status = crate::model::Status::Idle;
        changed.lanes[0].sessions[0].flagged = true;
        assert!(nudger.more(&changed, NOW + 2).is_empty());
        changed.prs[0].updated_at += 1;
        assert!(nudger.due(&changed, NOW + 3).is_empty());
        changed.sessions.block_flagged_sessions = false;
        changed.prs[0].updated_at += 1;
        assert_eq!(nudger.due(&changed, NOW + 4).len(), 1);
    }

    #[test]
    fn an_update_on_a_pr_its_on_for_is_a_job_for_the_sessions_that_run_here_once() {
        let mut nudger = Nudger::default();
        let sessions = vec![live(A), Session { live: false, ..crate::fixtures::session(B) }];
        // First seen: noted. Then updated on GitHub: a job to read what's new since, for the
        // session that runs.
        assert!(sent(&mut nudger, &board(vec![pr("o/r#1", PrState::Open, &[A, B])], sessions.clone())).is_empty());
        let moved = PrItem { updated_at: UPDATED + 5, ..pr("o/r#1", PrState::Open, &[A, B]) };
        let jobs = sent(&mut nudger, &board(vec![moved.clone()], sessions.clone()));
        assert_eq!(jobs.len(), 1);
        assert_eq!((jobs[0].session.as_str(), jobs[0].why.clone()), (A, Why { since: Some(UPDATED), ..Why::default() }));
        assert!(sent(&mut nudger, &board(vec![moved.clone()], sessions.clone())).is_empty(), "told once");
        // Approved, with the update that brought it; then changes asked for; then merged.
        let approved = PrItem { state: PrState::Approved, updated_at: UPDATED + 9, ..moved.clone() };
        assert_eq!(sent(&mut nudger, &board(vec![approved.clone()], sessions.clone()))[0].why, Why { since: Some(UPDATED + 5), approved: true, ..Why::default() });
        let changes = PrItem { state: PrState::Changes, ..approved.clone() };
        assert_eq!(sent(&mut nudger, &board(vec![changes], sessions.clone()))[0].why, Why { changes: true, ..Why::default() });
        let merged = off(PrItem { state: PrState::Merged, ..approved });
        assert_eq!(sent(&mut nudger, &board(vec![merged.clone()], sessions.clone()))[0].why, Why { ended: Some(PrState::Merged), ..Why::default() });
        assert!(sent(&mut nudger, &board(vec![merged], sessions)).is_empty(), "and nothing after");
    }

    #[test]
    fn checks_failing_is_told_once_and_the_approval_it_had_when_turned_on() {
        let mut nudger = Nudger::default();
        let failing = |n: u32| PrItem { checks: Checks { failed: n, failing: vec!["lint".into()], ..Checks::default() }, ..pr("o/r#1", PrState::Open, &[A]) };
        sent(&mut nudger, &board(vec![failing(0)], vec![live(A)]));
        assert_eq!(sent(&mut nudger, &board(vec![failing(1)], vec![live(A)]))[0].why, Why { failed: vec!["lint".into()], ..Why::default() });
        assert!(sent(&mut nudger, &board(vec![failing(2)], vec![live(A)])).is_empty(), "still failing is no news");
        // Off for a PR that's approved; turned on: told of the approval it has, and not of what
        // happened while it was off.
        let mut nudger = Nudger::default();
        sent(&mut nudger, &board(vec![off(pr("o/r#2", PrState::Open, &[A]))], vec![live(A)]));
        assert!(sent(&mut nudger, &board(vec![off(PrItem { updated_at: UPDATED + 1, ..pr("o/r#2", PrState::Approved, &[A]) })], vec![live(A)])).is_empty());
        let on = PrItem { updated_at: UPDATED + 2, ..pr("o/r#2", PrState::Approved, &[A]) };
        assert_eq!(sent(&mut nudger, &board(vec![on], vec![live(A)]))[0].why, Why { approved: true, ..Why::default() });
    }

    #[test]
    fn only_the_prs_primary_session_is_told_and_only_while_it_runs() {
        let mut nudger = Nudger::default();
        let both = vec![live(A), live(B)];
        let at = |updated: i64, primary: Option<&str>| PrItem { updated_at: updated, primary: primary.map(String::from), ..pr("o/r#1", PrState::Open, &[A, B]) };
        sent(&mut nudger, &board(vec![at(UPDATED, Some(A))], both.clone()));
        let jobs = sent(&mut nudger, &board(vec![at(UPDATED + 1, Some(A))], both.clone()));
        assert_eq!(jobs.iter().map(|j| j.session.as_str()).collect::<Vec<_>>(), [A]);
        // Another picked: it's told from then on.
        let jobs = sent(&mut nudger, &board(vec![at(UPDATED + 2, Some(B))], both.clone()));
        assert_eq!(jobs.iter().map(|j| j.session.as_str()).collect::<Vec<_>>(), [B]);
        // The primary stopped: nobody, though another of its sessions runs.
        assert!(sent(&mut nudger, &board(vec![at(UPDATED + 3, Some(B))], vec![live(A), Session { live: false, ..crate::fixtures::session(B) }])).is_empty());
        assert!(sent(&mut nudger, &board(vec![at(UPDATED + 4, None)], both)).is_empty());
    }

    /// A session on this Mac that isn't running.
    fn stopped(key: &str) -> Session {
        Session { live: false, ..crate::fixtures::session(key) }
    }

    /// `board` as the engine makes it with `[sessions] resume_for_pr_news` on, every session that
    /// isn't running one Codealong can reach.
    fn resuming(prs: Vec<PrItem>, sessions: Vec<Session>) -> Board {
        let mut b = board(prs, sessions);
        b.pr_stopped = stopped_primaries(&b, &BTreeSet::new(), true, |_| true);
        b
    }

    #[test]
    fn a_primary_that_stopped_is_resumed_for_what_asks_for_work() {
        let mut nudger = Nudger::default();
        let gone = stopped(A);
        let at = |updated: i64, state| PrItem { updated_at: updated, ..pr("o/r#1", state, &[A]) };
        sent(&mut nudger, &resuming(vec![at(UPDATED, PrState::Open)], vec![gone.clone()]));
        // Commented on: it's resumed to be told, of what asks for work only.
        let jobs = sent(&mut nudger, &resuming(vec![at(UPDATED + 1, PrState::Open)], vec![gone.clone()]));
        assert_eq!(jobs.iter().map(|j| (j.session.as_str(), j.stopped, j.about)).collect::<Vec<_>>(), [(A, true, NudgeAbout::FeedbackAndChecks)]);
        // An approval, a merge or a close is no reason to start it, nor what the board knows of
        // them when GitHub can't be read.
        assert!(sent(&mut nudger, &resuming(vec![at(UPDATED + 1, PrState::Approved)], vec![gone.clone()])).is_empty());
        assert!(sent(&mut nudger, &resuming(vec![off(at(UPDATED + 1, PrState::Merged))], vec![gone.clone()])).is_empty());
        let approved = Job { last: true, ..jobs[0].clone() };
        let read = |why: Why| message(&Job { why, ..approved.clone() }, None, Some("me")).unwrap();
        assert_eq!(read(Why { since: Some(UPDATED), approved: true, ..Why::default() }), None);
        let d = detail(vec![PrReview { author: "ken".into(), state: "APPROVED".into(), at: Some(UPDATED + 1), mine: false }], vec![]);
        assert_eq!(message(&Job { why: Why { since: Some(UPDATED), ..Why::default() }, ..approved.clone() }, Some(Ok(&d)), Some("me")).unwrap(), None, "ken's approval alone");
        assert!(read(Why { failed: vec!["lint".into()], ..Why::default() }).unwrap().contains("- Its checks failed: lint.\n"));
        // Running, it's told as before.
        let mut nudger = Nudger::default();
        sent(&mut nudger, &resuming(vec![at(UPDATED, PrState::Open)], vec![live(A)]));
        let jobs = sent(&mut nudger, &resuming(vec![at(UPDATED + 1, PrState::Approved)], vec![live(A)]));
        assert_eq!(jobs.iter().map(|j| (j.stopped, j.about, j.why.approved)).collect::<Vec<_>>(), [(false, NudgeAbout::Everything, true)]);
    }

    /// What becomes of a PR's news while its primary session isn't running, as the engine works it
    /// out for each board.
    #[test]
    fn a_stopped_primary_is_resumed_unless_another_of_its_prs_sessions_runs() {
        use crate::model::Helper;
        let of = |b: &Board, on: bool, reaches: bool| stopped_primaries(b, &BTreeSet::new(), on, |_| reaches).get("o/r#1").copied();
        let with = |sessions: Vec<Session>| board(vec![pr("o/r#1", PrState::Open, &[A, B])], sessions);
        assert_eq!(of(&with(vec![stopped(A), stopped(B)]), true, true), Some(Stopped::Resumes));
        assert_eq!(of(&with(vec![stopped(A), live(B)]), true, true), Some(Stopped::Beside), "the work may have gone on in B");
        // A helper or a review running for it doesn't take the PR on.
        let helper = Session { helper: Some(Helper { parent: Some(A.into()), ..Default::default() }), ..live(B) };
        assert_eq!(of(&with(vec![stopped(A), helper]), true, true), Some(Stopped::Resumes));
        // A session on its branch on another device, which this Mac's board doesn't list.
        let elsewhere = BTreeSet::from(["o/r#1".to_string()]);
        assert_eq!(stopped_primaries(&with(vec![stopped(A)]), &elsewhere, true, |_| true).get("o/r#1"), Some(&Stopped::Beside));
        // Never with the setting off, for one you archived, or one Codealong can't reach.
        assert_eq!(of(&with(vec![stopped(A)]), false, true), Some(Stopped::Never));
        assert_eq!(of(&with(vec![Session { archived: true, ..stopped(A) }]), true, true), Some(Stopped::Never));
        assert_eq!(of(&with(vec![stopped(A)]), true, false), Some(Stopped::Never));
        // Nothing for a primary that runs, nor for a PR with its nudge off.
        assert_eq!(of(&with(vec![live(A)]), true, true), None);
        assert_eq!(of(&board(vec![off(pr("o/r#1", PrState::Open, &[A]))], vec![stopped(A)]), true, true), None);
        // And the nudger goes by it.
        let told = |b: Board| {
            let mut nudger = Nudger::default();
            sent(&mut nudger, &b);
            let mut moved = b;
            moved.prs[0].updated_at += 1;
            sent(&mut nudger, &moved).len()
        };
        assert_eq!(told(resuming(vec![pr("o/r#1", PrState::Open, &[A, B])], vec![stopped(A), stopped(B)])), 1);
        assert_eq!(told(resuming(vec![pr("o/r#1", PrState::Open, &[A, B])], vec![stopped(A), live(B)])), 0);
        assert_eq!(told(board(vec![pr("o/r#1", PrState::Open, &[A, B])], vec![stopped(A), stopped(B)])), 0);
    }

    /// What another device runs that a PR's primary here isn't resumed beside: a session running
    /// on the PR's branch there, not a helper's, while the device is connected, under the repo's
    /// name then and now.
    #[test]
    fn what_another_device_runs_on_a_prs_branch() {
        use crate::model::{DeviceTag, Helper, Pr};
        let lane = |online: bool, number: Option<u64>, sessions: Vec<Session>| {
            let mut l = crate::fixtures::lane("/there", sessions);
            l.device = Some(DeviceTag { name: "Studio".into(), short: "st".into(), color: "blue".into(), online, last_seen: None });
            (l.repo_slug, l.repo_slug_now) = (Some("O/R".into()), Some("o/renamed".into()));
            l.pr = number.map(|number| Pr { number, title: String::new(), url: String::new(), state: PrState::Open, checks: Checks::default(), merge: Merge::Unknown, updated_at: 0, merged_at: None, closed_at: None, stale_approval: None });
            l
        };
        assert_eq!(running_elsewhere(&[lane(true, Some(7), vec![live(B)])]), BTreeSet::from(["o/r#7".to_string(), "o/renamed#7".to_string()]));
        let helper = Session { helper: Some(Helper { parent: Some(A.into()), ..Default::default() }), ..live(B) };
        for quiet in [lane(false, Some(7), vec![live(B)]), lane(true, None, vec![live(B)]), lane(true, Some(7), vec![stopped(B)]), lane(true, Some(7), vec![helper])] {
            assert!(running_elsewhere(&[quiet]).is_empty());
        }
        // This Mac's own lanes aren't another device's.
        let mut here = lane(true, Some(7), vec![live(B)]);
        here.device = None;
        assert!(running_elsewhere(&[here]).is_empty());
    }

    /// A resume is checked again on the board as it is when its turn comes, after GitHub was read
    /// and earlier jobs went.
    #[test]
    fn a_resume_is_checked_again_before_it_goes() {
        let p = pr("o/r#1", PrState::Open, &[A, B]);
        let job = Job { pr: p.clone(), why: Why { since: Some(UPDATED), ..Why::default() }, session: A.into(), last: false, about: NudgeAbout::FeedbackAndChecks, stopped: true };
        assert!(still_due(&resuming(vec![p.clone()], vec![stopped(A), stopped(B)]), &job));
        assert!(!still_due(&resuming(vec![p.clone()], vec![stopped(A), live(B)]), &job), "B started meanwhile");
        assert!(!still_due(&board(vec![p.clone()], vec![stopped(A), stopped(B)]), &job), "the setting went off");
        assert!(!still_due(&resuming(vec![PrItem { primary: Some(B.into()), ..p.clone() }], vec![stopped(A), stopped(B)]), &job), "B was made its primary");
        assert!(still_due(&resuming(vec![p], vec![live(A), live(B)]), &job), "A runs again: it's told as any is");
    }

    /// A primary resumed as a copy goes on in it: what came in while it was resumed, and what comes
    /// after, goes to the copy, waited for while the board doesn't have it.
    #[test]
    fn what_comes_for_a_primary_resumed_as_a_copy_goes_to_the_copy() {
        let mut nudger = Nudger::default();
        let at = |updated: i64| resuming(vec![PrItem { updated_at: updated, ..pr("o/r#1", PrState::Open, &[A]) }], vec![stopped(A)]);
        sent(&mut nudger, &at(UPDATED));
        let job = nudger.due(&at(UPDATED + 1), NOW).remove(0);
        // More comes in while it's resumed.
        assert!(nudger.due(&at(UPDATED + 2), NOW).is_empty());
        assert!(!nudger.told(&job, true, NOW));
        nudger.went_on_as(A, B, NOW);
        // The board doesn't have the copy yet, and still says A is the primary: it's waited for.
        assert!(nudger.due(&at(UPDATED + 3), NOW).is_empty());
        // Then it has it, running, the PR's primary: it's told all that came after the resume.
        let copy = resuming(vec![PrItem { updated_at: UPDATED + 3, primary: Some(B.into()), ..pr("o/r#1", PrState::Open, &[A, B]) }], vec![stopped(A), live(B)]);
        let jobs = nudger.due(&copy, NOW);
        assert_eq!(jobs.iter().map(|j| (j.session.as_str(), j.stopped, j.why.since)).collect::<Vec<_>>(), [(B, false, Some(UPDATED + 1))]);
        assert!(!nudger.told(&jobs[0], true, NOW));
        // Long after, A is its own again.
        let later = NOW + COPY_MS;
        assert!(nudger.due(&at(UPDATED + 4), later).iter().all(|j| j.session == A));
    }

    /// A send that failed and is tried again before the worker ends reads the PR afresh, with what
    /// came in meanwhile, and once it goes, its failure is no notice; one not tried again yet is.
    #[test]
    fn a_retry_in_the_same_worker_reads_afresh_and_takes_back_its_failure() {
        let at = |updated: i64| board(vec![PrItem { updated_at: updated, ..pr("o/r#1", PrState::Open, &[A]) }], vec![live(A)]);
        let fix = comment("c1", "ana", "Fix that.", UPDATED + 1, None, false);
        let more = comment("c2", "ana", "And this.", UPDATED + 3, None, false);
        let run = |clock: &dyn Fn(i64) -> i64| {
            let nudger = Mutex::new(Nudger::default());
            nudger.lock().unwrap().due(&at(UPDATED), NOW);
            let jobs = nudger.lock().unwrap().due(&at(UPDATED + 5), NOW);
            let mut reads = vec![detail(vec![], vec![fix.clone(), more.clone()]), detail(vec![], vec![fix.clone()])];
            let mut tries: Vec<String> = Vec::new();
            let calls = std::cell::Cell::new(0);
            let went = send_all(
                &Handle::over(at(UPDATED + 5)),
                &nudger,
                jobs,
                |_| Ok(reads.pop().unwrap()),
                |_, text| {
                    tries.push(text.to_string());
                    if tries.len() == 1 { Err("its terminal didn't take it".into()) } else { Ok(None) }
                },
                || {
                    calls.set(calls.get() + 1);
                    clock(calls.get())
                },
            );
            (went, tries)
        };
        // The retry is due by the time the round ends.
        let (went, tries) = run(&|call| if call == 1 { NOW } else { NOW + RETRY_MS });
        assert_eq!((went, tries.len()), (Ok(()), 2));
        assert!(!tries[0].contains("And this.") && tries[1].contains("And this."), "{tries:?}");
        // It isn't: the failure is the notice, saying it's tried again.
        let (went, tries) = run(&|_| NOW);
        assert_eq!(tries.len(), 1);
        assert!(went.unwrap_err().ends_with("its terminal didn't take it Codealong tries again in a few minutes."));
    }

    /// A PR's failed send is taken back by the one that goes after it, though it went to the copy
    /// its primary went on as meanwhile; a copy that couldn't be made primary stays said.
    #[test]
    fn a_prs_failure_is_taken_back_by_a_send_to_the_copy_its_primary_went_on_as() {
        let job = |pr: &str, session: &str| Job { pr: self::pr(pr, PrState::Open, &[session]), why: Why::default(), session: session.into(), last: false, about: NudgeAbout::Everything, stopped: true };
        let mut outcomes = Outcomes::default();
        outcomes.went(&job("o/r#1", A), Some("o/r#1 couldn't be read.".into()));
        outcomes.went(&job("o/r#2", A), None);
        outcomes.followed(Err("B couldn't be made o/r#2's primary.".into()));
        outcomes.went(&job("o/r#1", B), None);
        assert_eq!(outcomes.notice(), Err("B couldn't be made o/r#2's primary.".into()));
    }

    /// Two PRs whose news is for one session give it one job at a time, so one that isn't running
    /// isn't resumed twice.
    #[test]
    fn a_session_gets_one_job_at_a_time() {
        let mut nudger = Nudger::default();
        let at = |updated: i64| resuming(vec![PrItem { updated_at: updated, ..pr("o/r#1", PrState::Open, &[A]) }, PrItem { updated_at: updated, ..pr("o/r#2", PrState::Open, &[A]) }], vec![stopped(A)]);
        sent(&mut nudger, &at(UPDATED));
        let first = nudger.due(&at(UPDATED + 1), NOW);
        assert_eq!(first.len(), 1);
        assert!(nudger.more(&at(UPDATED + 1), NOW).is_empty(), "while the first is under way");
        assert!(!nudger.told(&first[0], true, NOW));
        // The rest is handed out as the first is done, on the same board, without news.
        let second = nudger.more(&at(UPDATED + 1), NOW);
        assert_eq!(second.len(), 1);
        assert_ne!(first[0].pr.key, second[0].pr.key);
        assert!(!nudger.told(&second[0], true, NOW));
        assert!(nudger.more(&at(UPDATED + 1), NOW).is_empty());
    }

    #[test]
    fn what_a_session_was_owed_is_resumed_for_once_it_stops() {
        use crate::model::Status;
        let mut nudger = Nudger::default();
        let waiting = Session { status: Status::Needs, ..live(A) };
        let at = |updated: i64, state| PrItem { updated_at: updated, ..pr("o/r#1", state, &[A]) };
        sent(&mut nudger, &resuming(vec![at(UPDATED, PrState::Open)], vec![waiting.clone()]));
        assert!(sent(&mut nudger, &resuming(vec![at(UPDATED + 1, PrState::Approved)], vec![waiting])).is_empty(), "A waits on you");
        // It stopped before it could be told: resumed for it, told only what asks for work.
        let jobs = sent(&mut nudger, &resuming(vec![at(UPDATED + 1, PrState::Approved)], vec![stopped(A)]));
        assert_eq!(jobs.iter().map(|j| (j.stopped, j.about, j.why.since)).collect::<Vec<_>>(), [(true, NudgeAbout::FeedbackAndChecks, Some(UPDATED))]);
        // Without the setting, what it was owed goes with it.
        let mut nudger = Nudger::default();
        let waiting = Session { status: Status::Needs, ..live(A) };
        sent(&mut nudger, &board(vec![at(UPDATED, PrState::Open)], vec![waiting.clone()]));
        sent(&mut nudger, &board(vec![at(UPDATED + 1, PrState::Open)], vec![waiting]));
        assert!(sent(&mut nudger, &board(vec![at(UPDATED + 1, PrState::Open)], vec![stopped(A)])).is_empty());
        assert!(sent(&mut nudger, &resuming(vec![at(UPDATED + 1, PrState::Open)], vec![stopped(A)])).is_empty(), "and isn't owed again");
    }

    #[test]
    fn what_a_session_was_owed_goes_when_it_is_no_longer_the_primary() {
        use crate::model::Status;
        let mut nudger = Nudger::default();
        let waiting = Session { status: Status::Needs, ..live(A) };
        let at = |updated: i64, primary: &str| PrItem { updated_at: updated, primary: Some(primary.into()), ..pr("o/r#1", PrState::Open, &[A, B]) };
        sent(&mut nudger, &board(vec![at(UPDATED, A)], vec![waiting.clone(), live(B)]));
        assert!(sent(&mut nudger, &board(vec![at(UPDATED + 1, A)], vec![waiting, live(B)])).is_empty(), "A waits on you");
        assert!(sent(&mut nudger, &board(vec![at(UPDATED + 1, B)], vec![live(A), live(B)])).is_empty(), "B became the primary after it");
    }

    #[test]
    fn turning_the_default_on_tells_no_approval_a_pr_had_already() {
        let mut nudger = Nudger::default();
        let approved = |nudge: bool, by_default: bool, updated: i64| PrItem { nudge, nudge_by_default: by_default, updated_at: updated, ..pr("o/r#1", PrState::Approved, &[A]) };
        sent(&mut nudger, &board(vec![approved(false, false, UPDATED)], vec![live(A)]));
        assert!(sent(&mut nudger, &board(vec![approved(true, true, UPDATED)], vec![live(A)])).is_empty());
        // What happens on it from then on is told.
        let merged = PrItem { state: PrState::Merged, nudge: false, nudge_by_default: false, ..approved(true, true, UPDATED) };
        assert_eq!(sent(&mut nudger, &board(vec![merged], vec![live(A)]))[0].why, Why { ended: Some(PrState::Merged), ..Why::default() });
    }

    #[test]
    fn a_pr_first_seen_or_on_another_device_tells_nobody() {
        use crate::model::DeviceTag;
        let mut nudger = Nudger::default();
        assert!(sent(&mut nudger, &board(vec![pr("o/r#1", PrState::Approved, &[A])], vec![live(A)])).is_empty());
        // Gone from the board and back, changed: first sight again.
        assert!(sent(&mut nudger, &board(vec![], vec![live(A)])).is_empty());
        assert!(sent(&mut nudger, &board(vec![PrItem { updated_at: UPDATED + 1, ..pr("o/r#1", PrState::Approved, &[A]) }], vec![live(A)])).is_empty());
        let mut nudger = Nudger::default();
        let elsewhere = |at: i64| {
            let mut b = board(vec![PrItem { updated_at: at, ..pr("o/r#1", PrState::Open, &[A]) }], vec![live(A)]);
            b.lanes[0].device = Some(DeviceTag { name: "Studio".into(), short: "st".into(), color: "blue".into(), online: true, last_seen: None });
            b
        };
        sent(&mut nudger, &elsewhere(UPDATED));
        assert!(sent(&mut nudger, &elsewhere(UPDATED + 1)).is_empty());
    }

    #[test]
    fn a_session_waiting_on_you_is_told_once_it_isnt() {
        use crate::model::Status;
        let mut nudger = Nudger::default();
        let waiting = Session { status: Status::Needs, ..live(A) };
        sent(&mut nudger, &board(vec![pr("o/r#1", PrState::Open, &[A])], vec![waiting.clone()]));
        let approved = PrItem { updated_at: UPDATED + 5, ..pr("o/r#1", PrState::Approved, &[A]) };
        assert!(sent(&mut nudger, &board(vec![approved.clone()], vec![waiting])).is_empty(), "what's typed would answer its prompt");
        // Answered: told, though the PR hasn't changed since.
        let jobs = sent(&mut nudger, &board(vec![approved.clone()], vec![live(A)]));
        assert_eq!(jobs[0].why, Why { since: Some(UPDATED), approved: true, ..Why::default() });
        assert!(sent(&mut nudger, &board(vec![approved], vec![live(A)])).is_empty());
    }

    #[test]
    fn a_refused_send_is_tried_again_later_with_what_came_since() {
        let mut nudger = Nudger::default();
        let at = |updated: i64, state| board(vec![PrItem { updated_at: updated, ..pr("o/r#1", state, &[A]) }], vec![live(A)]);
        nudger.due(&at(UPDATED, PrState::Open), NOW);
        let job = nudger.due(&at(UPDATED + 5, PrState::Approved), NOW).remove(0);
        // While the send is under way, more comes in: it waits for that send.
        assert!(nudger.due(&at(UPDATED + 9, PrState::Changes), NOW).is_empty());
        assert!(nudger.told(&job, false, NOW), "refused: tried again");
        assert!(nudger.due(&at(UPDATED + 9, PrState::Changes), NOW + 1000).is_empty(), "not straight away");
        let again = nudger.due(&at(UPDATED + 9, PrState::Changes), NOW + RETRY_MS).remove(0);
        // From the first update on, with both changes of state.
        assert_eq!(again.why, Why { since: Some(UPDATED), approved: true, changes: true, ..Why::default() });
        assert!(!nudger.told(&again, true, NOW + RETRY_MS));
        assert!(nudger.due(&at(UPDATED + 9, PrState::Changes), NOW + 10 * RETRY_MAX_MS).is_empty(), "told: owed no longer");
    }

    #[test]
    fn a_send_that_keeps_failing_waits_longer_each_time_and_is_given_up_on() {
        let mut nudger = Nudger::default();
        let b = |updated: i64| board(vec![PrItem { updated_at: updated, ..pr("o/r#1", PrState::Open, &[A]) }], vec![live(A)]);
        nudger.due(&b(UPDATED), NOW);
        let mut at = NOW;
        let mut job = nudger.due(&b(UPDATED + 5), at).remove(0);
        let mut waits = Vec::new();
        for _ in 1..TRIES {
            assert!(!job.last);
            assert!(nudger.told(&job, false, at));
            // The first second it's handed out again.
            let (next, again) = (at..).step_by(1000).find_map(|t| nudger.due(&b(UPDATED + 5), t).pop().map(|j| (t, j))).unwrap();
            waits.push((next - at) / 1000);
            (at, job) = (next, again);
        }
        assert_eq!(waits, [30, 60, 120, 240, 480]);
        // The last try tells what the board knows when GitHub can't be read, and isn't retried.
        assert!(job.last);
        assert!(!nudger.told(&job, false, at));
        assert!(nudger.due(&b(UPDATED + 5), at + 10 * RETRY_MAX_MS).is_empty());
    }

    #[test]
    fn news_that_cant_be_read_is_tried_again_before_its_left_out() {
        let p = pr("o/r#1", PrState::Approved, &[A]);
        let job = Job { pr: p.clone(), why: Why { since: Some(UPDATED), approved: true, ..Why::default() }, session: A.into(), last: false, about: NudgeAbout::Everything, stopped: false };
        let e = message(&job, Some(Err("gh timed out after 30s")), Some("me")).unwrap_err();
        assert_eq!(e, "What's new on o/r#1 couldn't be read: gh timed out after 30s");
        // The last try says what the board knows.
        let text = message(&Job { last: true, ..job.clone() }, Some(Err("gh timed out after 30s")), Some("me")).unwrap().unwrap();
        assert!(text.contains("- It was approved.\n"), "{text}");
        // Read: what others wrote.
        let d = detail(vec![PrReview { author: "ken".into(), state: "APPROVED".into(), at: Some(UPDATED + 1), mine: false }], vec![]);
        assert!(message(&job, Some(Ok(&d)), Some("me")).unwrap().unwrap().contains("- ken approved it.\n"));
    }

    #[test]
    fn says_what_others_did_since_and_nothing_for_what_was_yours() {
        let p = pr("o/r#1", PrState::Changes, &[A]);
        let d = detail(
            vec![
                PrReview { author: "ana".into(), state: "CHANGES_REQUESTED".into(), at: Some(UPDATED + 10), mine: false },
                PrReview { author: "me".into(), state: "COMMENTED".into(), at: Some(UPDATED + 20), mine: false },
                PrReview { author: "old".into(), state: "APPROVED".into(), at: Some(UPDATED - 1), mine: false },
            ],
            vec![
                comment("c1", "ana", "Please add a test.", UPDATED + 10, None, false),
                comment("c2", "ana", "This can be a map.", UPDATED + 11, Some("src/a.ts"), false),
                comment("c3", "me", "Done.", UPDATED + 20, None, true),
                comment("c4", "ken", "Before all that.", UPDATED - 5, None, false),
            ],
        );
        let why = Why { since: Some(UPDATED), changes: true, ..Why::default() };
        let text = words(&p, &why, Some(&d), Some("me"), NudgeAbout::Everything).unwrap();
        assert_eq!(
            text,
            "On your PR #1 “Let the agent add photos” (https://github.com/o/r/pull/1):\n- ana asked for changes: “Please add a test.”\n- ana commented on src/a.ts: “This can be a map.”\nTake it from there, unless you were told to leave it to someone else. This message doesn't ask you to merge it: merge it only if you were asked to."
        );
        // Only your own push moved it: nothing to say.
        assert_eq!(words(&p, &Why { since: Some(UPDATED + 30), ..Why::default() }, Some(&d), Some("me"), NudgeAbout::Everything), None);
        // The detail couldn't be read: what the board knows is said.
        assert!(words(&p, &why, None, Some("me"), NudgeAbout::Everything).unwrap().contains("- Changes were asked for.\n"));
    }

    #[test]
    fn says_an_approval_a_merge_and_failing_checks() {
        let approved = PrItem { merge: Merge::Clean, ..pr("o/r#1", PrState::Approved, &[A]) };
        let d = detail(vec![PrReview { author: "ken".into(), state: "APPROVED".into(), at: Some(UPDATED + 1), mine: false }], vec![]);
        let text = words(&approved, &Why { since: Some(UPDATED), approved: true, ..Why::default() }, Some(&d), Some("me"), NudgeAbout::Everything).unwrap();
        // The approval is news, never a go-ahead to merge.
        assert_eq!(text, "On your PR #1 “Let the agent add photos” (https://github.com/o/r/pull/1):\n- ken approved it.\nIt's ready to merge. This message doesn't ask you to merge it: merge it only if you were asked to.");
        // Turned on for one approved already, with nothing read: said in the board's words.
        assert!(words(&approved, &Why { approved: true, ..Why::default() }, None, None, NudgeAbout::Everything).unwrap().contains("- It was approved.\n"));
        let merged = off(PrItem { state: PrState::Merged, mine: false, ..pr("o/r#1", PrState::Open, &[A]) });
        assert_eq!(
            words(&merged, &Why { ended: Some(PrState::Merged), ..Why::default() }, None, None, NudgeAbout::Everything).unwrap(),
            "On PR #1 “Let the agent add photos” (https://github.com/o/r/pull/1):\n- It was merged.\nNothing more is wanted on it, unless you were told otherwise."
        );
        let failing = PrItem { checks: Checks { failed: 2, failing: vec!["lint".into(), "test".into()], ..Checks::default() }, ..pr("o/r#1", PrState::Open, &[A]) };
        let text = words(&failing, &Why { failed: vec!["lint".into(), "test".into()], ..Why::default() }, None, None, NudgeAbout::Everything).unwrap();
        assert!(text.contains("- Its checks failed: lint, test.\nSome of its checks failed. Take it from there"), "{text}");
        // No open PR's message reads as a go-ahead to merge, whatever came in.
        let open = PrItem { merge: Merge::Clean, ..pr("o/r#1", PrState::Approved, &[A]) };
        for why in [Why { approved: true, ..Why::default() }, Why { changes: true, ..Why::default() }, Why { failed: vec!["lint".into()], ..Why::default() }] {
            let text = words(&open, &why, None, None, NudgeAbout::Everything).unwrap();
            assert!(text.ends_with(NOT_A_MERGE) && !text.contains("merging it"), "{text}");
        }
    }

    /// What a nudge tells about goes as the PR's repo says: its team's file, then your table for
    /// the repo, over `[pull_requests] nudge_about`.
    #[test]
    fn what_a_nudge_tells_about_goes_as_its_repo_says() {
        let open = pr("o/r#1", PrState::Open, &[A]);
        let approved = PrItem { state: PrState::Approved, ..open.clone() };
        let in_repo = |prs: Vec<PrItem>, mine: Option<NudgeAbout>| {
            let mut b = board(prs, vec![live(A)]);
            b.repo_settings.team.insert(("o/r".into(), "main".into()), crate::teamfile::TeamCfg { nudge_about: Some(NudgeAbout::Feedback), ..Default::default() });
            if let Some(about) = mine {
                let table = crate::config::RepoCfg { pull_requests: crate::config::PullRequestsPatch { nudge_about: Some(about), ..Default::default() }, ..Default::default() };
                b.repo_settings.mine.insert(crate::config::repo_key("o/r"), table);
            }
            b
        };
        let mut nudger = Nudger::default();
        sent(&mut nudger, &in_repo(vec![open.clone()], None));
        assert!(sent(&mut nudger, &in_repo(vec![approved.clone()], None)).is_empty(), "the team keeps it to feedback");
        let mut nudger = Nudger::default();
        sent(&mut nudger, &in_repo(vec![open], Some(NudgeAbout::Everything)));
        let jobs = sent(&mut nudger, &in_repo(vec![approved], Some(NudgeAbout::Everything)));
        assert_eq!((jobs[0].why.approved, jobs[0].about), (true, NudgeAbout::Everything), "yours for the repo goes over the team's");
    }

    /// An agent's review Codealong posted from someone's account is told as their agent's when
    /// it's a comment of its own, and as theirs with their agent's review in it when it's a
    /// review's text, which may hold words of theirs; a line comment in such a review may be the
    /// agent's. Every agent is named, and rules, moved marks and code examples in it don't hide
    /// it. A mark quoted within a line, or a footer indented, leaves someone's words their own;
    /// one pasted whole at the start of its lines is taken for an agent's, the safer mistake.
    #[test]
    fn an_agents_review_from_someones_account_is_told_as_their_agents() {
        let open = pr("o/r#1", PrState::Open, &[A]);
        let sub = |agent: &str, how: &str| format!("<sub>Reviewed 3af044a with {agent} · {how} via Codealong</sub>");
        let mark = "<!-- codealong-review o/r#1 3af044adb7a528d4b352bfcc26e4122ef59af1aa -->";
        let claude = "Claude Code, claude-opus-5-5";
        let review = |id: &str, author: &str, body: &str, at: i64, state: &str| PrComment { review: Some(state.into()), ..comment(id, author, body, at, None, false) };
        let line = |id: &str, author: &str, body: &str, at: i64, in_review: &str| PrComment { in_review: Some(in_review.into()), ..comment(id, author, body, at, Some("main.tf"), false) };
        let d = detail(
            vec![PrReview { author: "ken".into(), state: "APPROVED".into(), at: Some(UPDATED + 3), mine: false }, PrReview { author: "ana".into(), state: "CHANGES_REQUESTED".into(), at: Some(UPDATED + 4), mine: false }],
            vec![
                comment("c1", "wes", &format!("Fix the race.\n\n---\n\nAdd a test.\n\n{}\n{mark}", sub(claude, "posted")), UPDATED + 1, None, false),
                comment("c2", "wes", "Agreed, go ahead. Keep `<!-- codealong-review o/r#1 abc -->` in the fixture.", UPDATED + 2, None, false),
                comment("c3", "bo", &format!("The footer looks like this:\n```\n{}\n{mark}\n```", sub(claude, "posted")), UPDATED + 2, None, false),
                review("R1", "ken", &format!("Fine.\n\n{}\n{mark}", sub(claude, "submitted")), UPDATED + 3, "APPROVED"),
                line("T1", "ken", "Pin this version.", UPDATED + 3, "R1"),
                // Two rounds by two agents, after words of ana's, edited in Codealong: the marks
                // moved to the end.
                review("R2", "ana", &format!("Please rename it.\n\n---\n\nThe lock is held too long.\n\n{}\n\n---\n\nStill held.\n\n{}\n{mark}\n{mark} <!-- round 2 -->", sub(claude, "drafted"), sub("Codex, gpt-6-astra", "drafted")), UPDATED + 4, "CHANGES_REQUESTED"),
                line("T2", "ana", "Rename this.", UPDATED + 4, "R2"),
                line("T3", "bo", "Mine alone.", UPDATED + 5, "R9"),
                // An agent's code example with another fence's character in it, before its footer.
                comment("c4", "lu", &format!("Use this:\n```text\n~~~\n```\n\n{}\n{mark}", sub(claude, "posted")), UPDATED + 6, None, false),
                // Someone's four-backtick example holding a three-backtick one with the footer.
                comment("c5", "bo", &format!("````md\n```\n{}\n{mark}\n```\n````", sub(claude, "posted")), UPDATED + 7, None, false),
                // And the footer as an indented code block.
                comment("c6", "bo", &format!("Indented:\n\n    {}\n    {mark}", sub(claude, "posted")), UPDATED + 8, None, false),
                // An agent's review with a fence in a list, backticks in an HTML block, and a fence
                // with a no-break space after it, before its footer: none of it hides the footer.
                comment("c7", "lu", &format!("- ~~~text\n  x\n  ~~~\n\n<pre>\n```\n</pre>\n\n```\u{a0}\n\n{}\n{mark}", sub(claude, "posted")), UPDATED + 9, None, false),
                // One posted before 2026-10-26, with the line as it was written then.
                comment("c8", "lu", &format!("Old one.\n\n<sub>An agent's review (Codex) of 3af044a, posted by Codealong.</sub>\n{mark}"), UPDATED + 10, None, false),
            ],
        );
        let text = words(&open, &Why { since: Some(UPDATED), ..Why::default() }, Some(&d), Some("me"), NudgeAbout::Everything).unwrap();
        for said in [
            format!("- wes's agent ({claude}) commented, from wes's account; an agent's review, not wes's word: “Fix the race.\n\n---\n\nAdd a test.”\n"),
            "- wes commented: “Agreed, go ahead. Keep `<!-- codealong-review o/r#1 abc -->` in the fixture.”\n".to_string(),
            // Pasted whole at the start of its lines, a footer is taken for one, code block or
            // not: the safer mistake.
            format!("- bo's agent ({claude}) commented, from bo's account; an agent's review, not bo's word: “The footer looks like this:\n```\n```”\n"),
            format!("- ken approved it, with their agent's review ({claude}) in it; the agent's review isn't ken's word: “Fine.”\n"),
            format!("- ken commented on main.tf, in a review with their agent's review ({claude}) in it; it may be the agent's, not ken's word: “Pin this version.”\n"),
            format!("- ana asked for changes, with their agent's review ({claude} and Codex, gpt-6-astra) in it; the agent's review isn't ana's word: “Please rename it.\n\n---\n\nThe lock is held too long.\n\n---\n\nStill held.”\n"),
            format!("- ana commented on main.tf, in a review with their agent's review ({claude} and Codex, gpt-6-astra) in it; it may be the agent's, not ana's word: “Rename this.”\n"),
            "- bo commented on main.tf: “Mine alone.”".to_string(),
            format!("- lu's agent ({claude}) commented, from lu's account; an agent's review, not lu's word: “Use this:\n```text\n~~~\n```”\n"),
            format!("- bo's agent ({claude}) commented, from bo's account; an agent's review, not bo's word: “````md\n```\n```\n````”\n"),
            format!("- bo commented: “Indented:\n\n    {}\n    {mark}”", sub(claude, "posted")),
            format!("- lu's agent ({claude}) commented, from lu's account; an agent's review, not lu's word: “- ~~~text"),
            "- lu's agent (Codex) commented, from lu's account; an agent's review, not lu's word: “Old one.”".to_string(),
        ] {
            assert!(text.contains(&said), "{said}\nin\n{text}");
        }
    }

    #[test]
    fn with_merges_too_it_tells_of_a_merge_and_nothing_else_once_its_pr_ended() {
        let with = |prs: Vec<PrItem>, sessions: Vec<Session>, about: NudgeAbout| Board { nudge_about: about, ..resuming(prs, sessions) };
        let open = pr("o/r#1", PrState::Open, &[A]);
        // Merged, with a comment that came in with it: only the merge is told, and the message
        // says nothing is wanted on it.
        let mut nudger = Nudger::default();
        sent(&mut nudger, &with(vec![open.clone()], vec![live(A)], NudgeAbout::FeedbackAndMerges));
        let merged = off(PrItem { state: PrState::Merged, updated_at: UPDATED + 9, ..open.clone() });
        let jobs = sent(&mut nudger, &with(vec![merged.clone()], vec![live(A)], NudgeAbout::FeedbackAndMerges));
        assert_eq!(jobs.iter().map(|j| (j.why.clone(), j.about)).collect::<Vec<_>>(), [(Why { ended: Some(PrState::Merged), ..Why::default() }, NudgeAbout::FeedbackAndMerges)]);
        assert_eq!(
            words(&merged, &Why { since: Some(UPDATED), ended: Some(PrState::Merged), ..Why::default() }, None, None, NudgeAbout::FeedbackChecksAndMerges).unwrap(),
            "On your PR #1 “Let the agent add photos” (https://github.com/o/r/pull/1):\n- It was merged.\nNothing more is wanted on it, unless you were told otherwise."
        );
        // Still never an approval or a close; checks only where it says so.
        let mut nudger = Nudger::default();
        sent(&mut nudger, &with(vec![open.clone()], vec![live(A)], NudgeAbout::FeedbackAndMerges));
        let approved = PrItem { state: PrState::Approved, ..open.clone() };
        assert!(sent(&mut nudger, &with(vec![approved.clone()], vec![live(A)], NudgeAbout::FeedbackAndMerges)).is_empty());
        let failing = PrItem { checks: Checks { failed: 1, failing: vec!["lint".into()], ..Checks::default() }, ..approved.clone() };
        assert!(sent(&mut nudger, &with(vec![failing.clone()], vec![live(A)], NudgeAbout::FeedbackAndMerges)).is_empty());
        let closed = off(PrItem { state: PrState::Closed, ..failing.clone() });
        assert!(sent(&mut nudger, &with(vec![closed], vec![live(A)], NudgeAbout::FeedbackAndMerges)).is_empty(), "a close isn't a merge");
        let mut nudger = Nudger::default();
        sent(&mut nudger, &with(vec![approved], vec![live(A)], NudgeAbout::FeedbackChecksAndMerges));
        assert_eq!(sent(&mut nudger, &with(vec![failing], vec![live(A)], NudgeAbout::FeedbackChecksAndMerges))[0].why, Why { failed: vec!["lint".into()], ..Why::default() });
        // A merge is no reason to resume a session that stopped.
        let mut nudger = Nudger::default();
        sent(&mut nudger, &with(vec![open], vec![stopped(A)], NudgeAbout::FeedbackChecksAndMerges));
        assert!(sent(&mut nudger, &with(vec![merged], vec![stopped(A)], NudgeAbout::FeedbackChecksAndMerges)).is_empty());
        assert_eq!(for_stopped(NudgeAbout::FeedbackAndMerges), NudgeAbout::Feedback);
    }

    #[test]
    fn with_feedback_only_it_tells_of_comments_and_changes_asked_for_never_an_approval_or_a_merge() {
        let mut nudger = Nudger::default();
        let feedback = |prs: Vec<PrItem>, about: NudgeAbout| Board { nudge_about: about, ..board(prs, vec![live(A)]) };
        let open = pr("o/r#1", PrState::Open, &[A]);
        sent(&mut nudger, &feedback(vec![open.clone()], NudgeAbout::Feedback));
        // Approved, merged, or checks failing: nothing, unless checks are asked for too.
        let approved = PrItem { state: PrState::Approved, ..open.clone() };
        assert!(sent(&mut nudger, &feedback(vec![approved.clone()], NudgeAbout::Feedback)).is_empty());
        let failing = PrItem { checks: Checks { failed: 1, failing: vec!["lint".into()], ..Checks::default() }, ..approved.clone() };
        assert!(sent(&mut nudger, &feedback(vec![failing.clone()], NudgeAbout::Feedback)).is_empty());
        let passing = PrItem { checks: Checks::default(), ..failing.clone() };
        sent(&mut nudger, &feedback(vec![passing], NudgeAbout::FeedbackAndChecks));
        let jobs = sent(&mut nudger, &feedback(vec![failing.clone()], NudgeAbout::FeedbackAndChecks));
        assert_eq!((jobs[0].why.clone(), jobs[0].about), (Why { failed: vec!["lint".into()], ..Why::default() }, NudgeAbout::FeedbackAndChecks));
        // Changes asked for are told, and so is what moved the PR, for its comments.
        let changes = PrItem { state: PrState::Changes, updated_at: UPDATED + 5, ..failing.clone() };
        assert_eq!(sent(&mut nudger, &feedback(vec![changes.clone()], NudgeAbout::Feedback))[0].why, Why { since: Some(UPDATED), changes: true, ..Why::default() });
        let merged = off(PrItem { state: PrState::Merged, updated_at: UPDATED + 9, ..changes });
        assert!(sent(&mut nudger, &feedback(vec![merged.clone()], NudgeAbout::Feedback)).is_empty(), "a merge isn't feedback");
        // Its words: an approval and a dismissal are left out, their text with them; the rest is
        // said.
        let d = detail(
            vec![
                PrReview { author: "ken".into(), state: "APPROVED".into(), at: Some(UPDATED + 1), mine: false },
                PrReview { author: "ana".into(), state: "CHANGES_REQUESTED".into(), at: Some(UPDATED + 2), mine: false },
                PrReview { author: "bo".into(), state: "DISMISSED".into(), at: Some(UPDATED + 3), mine: false },
            ],
            vec![comment("c1", "ken", "Looks good.", UPDATED + 1, None, false), comment("c2", "ken", "Nit: a map.", UPDATED + 1, Some("src/a.ts"), false), comment("c3", "ana", "Add a test.", UPDATED + 2, None, false)],
        );
        let why = Why { since: Some(UPDATED), approved: true, ..Why::default() };
        let text = words(&approved, &why, Some(&d), Some("me"), NudgeAbout::Feedback).unwrap();
        assert_eq!(
            text,
            "On your PR #1 “Let the agent add photos” (https://github.com/o/r/pull/1):\n- ana asked for changes: “Add a test.”\n- ken commented on src/a.ts: “Nit: a map.”\nTake it from there, unless you were told to leave it to someone else. This message doesn't ask you to merge it: merge it only if you were asked to."
        );
        assert!(words(&approved, &why, Some(&d), Some("me"), NudgeAbout::Everything).unwrap().contains("- ken approved it: “Looks good.”\n"));
        // Only an approval: nothing to say. Nor anything about a PR that ended.
        let only = detail(vec![PrReview { author: "ken".into(), state: "APPROVED".into(), at: Some(UPDATED + 1), mine: false }], vec![]);
        assert_eq!(words(&approved, &why, Some(&only), Some("me"), NudgeAbout::Feedback), None);
        assert_eq!(words(&merged, &Why { since: Some(UPDATED), ended: Some(PrState::Merged), ..Why::default() }, Some(&d), Some("me"), NudgeAbout::FeedbackAndChecks), None);
        // Two approvals by one reviewer since: the detail keeps the latest as the review, and the
        // earlier one's words aren't told as a comment either.
        let graph = r#"{"data":{"viewer":{"login":"me"},"repository":{"pullRequest":{"bodyHTML":"","mergeable":null,"commits":{"totalCount":1},
          "reviews":{"nodes":[
            {"id":"R1","author":{"login":"ken"},"state":"APPROVED","submittedAt":"2026-09-23T10:00:00Z","body":"Looks good.","url":"u1"},
            {"id":"R2","author":{"login":"ken"},"state":"APPROVED","submittedAt":"2026-09-23T11:00:00Z","body":"Still good.","url":"u2"}]},
          "reviewRequests":{"nodes":[]},"comments":{"nodes":[]},"reviewThreads":{"nodes":[]}}}}}"#;
        let twice = crate::github::parse_detail(graph, "[]").unwrap();
        let since = Why { since: Some(0), ..Why::default() };
        assert_eq!(words(&approved, &since, Some(&twice), Some("me"), NudgeAbout::Feedback), None);
        assert!(words(&approved, &since, Some(&twice), Some("me"), NudgeAbout::Everything).unwrap().contains("- ken approved it: “Still good.”\n"));
    }

    /// A branch that comes to conflict with its base is told once: GitHub's not knowing while it
    /// works it out again, after the base moved, is no news either way, and only a conflict that
    /// comes back after it went is told again.
    #[test]
    fn a_branch_that_comes_to_conflict_is_told_once() {
        let mut nudger = Nudger::default();
        let at = |merge: Merge| board(vec![PrItem { merge, ..pr("o/r#1", PrState::Open, &[A]) }], vec![live(A)]);
        sent(&mut nudger, &at(Merge::Clean));
        assert_eq!(sent(&mut nudger, &at(Merge::Conflicts))[0].why, Why { conflicts: true, ..Why::default() });
        for merge in [Merge::Unknown, Merge::Conflicts, Merge::Unknown, Merge::Conflicts] {
            assert!(sent(&mut nudger, &at(merge)).is_empty(), "still conflicting after {merge:?}");
        }
        assert!(sent(&mut nudger, &at(Merge::Blocked)).is_empty(), "resolved");
        assert!(sent(&mut nudger, &at(Merge::Unknown)).is_empty());
        assert_eq!(sent(&mut nudger, &at(Merge::Conflicts))[0].why, Why { conflicts: true, ..Why::default() }, "and back");
        // Conflicting when first seen, or when its nudge is turned on: as failing checks, only
        // what comes after is news.
        let mut nudger = Nudger::default();
        sent(&mut nudger, &at(Merge::Conflicts));
        assert!(sent(&mut nudger, &at(Merge::Unknown)).is_empty());
        assert!(sent(&mut nudger, &at(Merge::Conflicts)).is_empty());
        let mut nudger = Nudger::default();
        let turned = |nudge: bool, merge: Merge| board(vec![PrItem { nudge, merge, ..pr("o/r#1", PrState::Open, &[A]) }], vec![live(A)]);
        sent(&mut nudger, &turned(false, Merge::Clean));
        assert!(sent(&mut nudger, &turned(false, Merge::Conflicts)).is_empty());
        assert!(sent(&mut nudger, &turned(true, Merge::Conflicts)).is_empty());
        // Conflicts go with failing checks: not with feedback alone.
        for (about, told) in [(NudgeAbout::Everything, true), (NudgeAbout::FeedbackAndChecks, true), (NudgeAbout::Feedback, false)] {
            let mut nudger = Nudger::default();
            let with = |merge: Merge| Board { nudge_about: about, ..at(merge) };
            sent(&mut nudger, &with(Merge::Clean));
            assert_eq!(!sent(&mut nudger, &with(Merge::Conflicts)).is_empty(), told, "{about:?}");
        }
    }

    #[test]
    fn says_a_branch_now_conflicts_with_its_base() {
        let why = Why { conflicts: true, ..Why::default() };
        // Approved, with a conflict: there's work to do, and still no go-ahead to merge.
        let approved = PrItem { merge: Merge::Conflicts, ..pr("o/r#1", PrState::Approved, &[A]) };
        assert_eq!(
            words(&approved, &why, None, None, NudgeAbout::FeedbackAndChecks).unwrap(),
            "On your PR #1 “Let the agent add photos” (https://github.com/o/r/pull/1):\n- Its branch now conflicts with main, its base.\nTake it from there, unless you were told to leave it to someone else. This message doesn't ask you to merge it: merge it only if you were asked to."
        );
        // Told with other news, where it stands doesn't say it again.
        let failing = PrItem { checks: Checks { failed: 1, failing: vec!["lint".into()], ..Checks::default() }, base: None, ..approved.clone() };
        let text = words(&failing, &Why { failed: vec!["lint".into()], ..why.clone() }, None, None, NudgeAbout::Everything).unwrap();
        assert!(text.contains("- Its checks failed: lint.\n- Its branch now conflicts with its base.\nTake it from there"), "{text}");
        // The PR closed before it was told: no news.
        let closed = PrItem { state: PrState::Closed, merge: Merge::Unknown, ..approved };
        assert!(!words(&closed, &Why { ended: Some(PrState::Closed), ..why }, None, None, NudgeAbout::Everything).unwrap().contains("conflicts"));
    }

    /// A conflict owed to a session that waits on you, or whose send failed, goes once a board
    /// shows it resolved, though GitHub doesn't know again by the time it could be told; one never
    /// resolved is told then.
    #[test]
    fn a_conflict_resolved_before_it_could_be_told_is_not() {
        use crate::model::Status;
        let waiting = Session { status: Status::Needs, ..live(A) };
        let at = |merge: Merge, s: &Session| board(vec![PrItem { merge, ..pr("o/r#1", PrState::Open, &[A]) }], vec![s.clone()]);
        let mut nudger = Nudger::default();
        sent(&mut nudger, &at(Merge::Clean, &waiting));
        assert!(sent(&mut nudger, &at(Merge::Conflicts, &waiting)).is_empty());
        assert!(sent(&mut nudger, &at(Merge::Clean, &waiting)).is_empty());
        assert!(sent(&mut nudger, &at(Merge::Unknown, &live(A))).is_empty(), "resolved while it waited");
        let mut nudger = Nudger::default();
        sent(&mut nudger, &at(Merge::Clean, &waiting));
        sent(&mut nudger, &at(Merge::Conflicts, &waiting));
        assert_eq!(sent(&mut nudger, &at(Merge::Unknown, &live(A)))[0].why, Why { conflicts: true, ..Why::default() }, "never resolved");
        // A failed send isn't tried again once it's resolved.
        let mut nudger = Nudger::default();
        nudger.due(&at(Merge::Clean, &live(A)), NOW);
        let job = nudger.due(&at(Merge::Conflicts, &live(A)), NOW).remove(0);
        nudger.due(&at(Merge::Clean, &live(A)), NOW);
        assert!(nudger.told(&job, false, NOW));
        assert!(nudger.due(&at(Merge::Unknown, &live(A)), NOW + RETRY_MAX_MS).is_empty());
    }

    #[test]
    fn says_where_the_pr_stands() {
        let at = |merge, failed, pending| standing(&PrItem { merge, checks: Checks { failed, pending, ..Checks::default() }, ..pr("o/r#1", PrState::Approved, &[]) });
        assert_eq!(at(Merge::Clean, 0, 0), "It's ready to merge. ");
        assert_eq!(at(Merge::Conflicts, 0, 2), "Its branch conflicts with its base. ");
        assert_eq!(at(Merge::Blocked, 1, 2), "Some of its checks failed. ");
        assert_eq!(at(Merge::Unknown, 0, 2), "Its checks are still running. ");
        assert_eq!(at(Merge::Unknown, 0, 0), "");
    }
}
