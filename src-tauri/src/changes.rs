//! What changed since an earlier look at the board, for an agent that keeps watch over every
//! session on a loop (`codealong changes`, the orchestrate skill's tick): sessions that started,
//! took a turn, ended one, wait on their user or stopped; PRs in the user's repos opened,
//! reviewed, merged or closed; todos added, started, finished or set waiting; and worktrees a
//! second agent started working in. With it, the sessions that wait on the user now, so a quiet
//! tick is one command. Headless runs (`Session::headless`: `claude -p`, `codex exec`) are
//! counted by the worktree they ran in rather than told one by one, since a script or an agent
//! can run hundreds and reads what they say itself, and they don't make a worktree shared. Nor
//! do an app's chats (`Lane::chats`), which are listed together for having no folder of yours.
//!
//! Each look is kept as a `Look` in changes.json, under the cursor it prints, and the next look
//! says what differs from it. Differences, not times: a session's times are its harness's, and a
//! PR's review has none. Where there's no earlier look (the first one, a cursor that's gone, a
//! duration given instead) or no mark for an item in it, times decide: started, active, opened,
//! merged or closed since then.
//!
//! A part that can't be read this time (PRs without the app or before GitHub's first poll;
//! sessions and todos while the app's first scan runs; todos while todos.json doesn't read) is
//! said so, and the earlier look's part is kept for it, so nothing in it shows as new, or as
//! gone, once it reads again. A PR a board with PRs stops listing (its repo didn't answer) keeps
//! its mark the same way, for a week.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::api;
use crate::model::{Board, Lane, Origin, PrItem, PrState, Session, Status};
use crate::todos::{Todo, TodoStatus};
use crate::util::DAY_MS;

/// How long a look is kept for a cursor to name, and how many are kept at most.
const KEEP_MS: i64 = 7 * DAY_MS;
const KEEP_LOOKS: usize = 12;
/// How long a PR the board stopped listing keeps its mark.
const PR_KEPT_MS: i64 = 7 * DAY_MS;
/// How recently a live session must have done something to count as working in its worktree,
/// besides one working or waiting on its user.
const ACTIVE_MS: i64 = 60 * 60_000;
/// How much of a prompt, and of a reply's first line and end, a change carries.
const PROMPT_CUT: usize = 400;
const REPLY_WHOLE: usize = 900;
const REPLY_HEAD: usize = 200;
const REPLY_TAIL: usize = 600;

/// One part of a look: when it was read, and a mark for each item in it by its key.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Part<T> {
    pub at: i64,
    pub marks: BTreeMap<String, T>,
    /// For PRs, by repo slug: what a PR of the repo the look had no mark for is told against,
    /// when GitHub last gave its PRs, or for one it hasn't yet, the time the look began from.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub repos: BTreeMap<String, i64>,
}

/// What a look saw, kept for the next one to compare with. A part that couldn't be read is the
/// earlier look's, or with none, where this look began (`kept`). None in a look kept before that.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Look {
    pub at: i64,
    #[serde(default)]
    pub sessions: Option<Part<SessionMark>>,
    #[serde(default)]
    pub prs: Option<Part<PrMark>>,
    #[serde(default)]
    pub todos: Option<Part<TodoMark>>,
    /// Worktrees agents were working in, by path, with their sessions' keys.
    #[serde(default)]
    pub shared: Option<Part<Vec<String>>>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionMark {
    pub status: Status,
    /// Its `last_prompt_at`: a different one says a turn ran in between.
    #[serde(default)]
    pub prompt_at: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PrMark {
    pub state: PrState,
    /// Your review was asked for.
    #[serde(default)]
    pub asks_you: bool,
    /// When a board last listed it.
    pub seen: i64,
    /// How current its repo's PRs were as it was marked (`Board::github_current`): it changes
    /// only by newer ones.
    #[serde(default = "never")]
    pub current: i64,
}

fn never() -> i64 {
    i64::MIN
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TodoMark {
    pub status: TodoStatus,
    #[serde(default)]
    pub waiting: bool,
    /// For saying which todo went, once it's gone.
    #[serde(default)]
    pub title: String,
}

/// What happened to a session since the earlier look, by where it is now.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum SessionChange {
    /// It started.
    New,
    /// A turn of it began: a new prompt, or it went on from waiting.
    Working,
    /// It waits on its user: a call to allow, a question, an error.
    Waiting,
    /// A turn of it ended.
    Finished,
    /// Its process ended.
    Stopped,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum PrChange {
    Opened,
    /// Out of draft.
    Ready,
    /// Back to draft.
    Draft,
    Approved,
    ChangesRequested,
    /// Open again after an approval or a request for changes: a review dismissed or asked again.
    ReviewReset,
    Reopened,
    Merged,
    Closed,
    /// Your review was asked for.
    ReviewAsked,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum TodoChange {
    Added,
    Started,
    Done,
    /// Back to the list, from done or in progress.
    Reopened,
    /// Marked as waiting on something outside the list.
    Waiting,
    /// No longer waiting.
    Ready,
    Removed,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionRow {
    pub change: SessionChange,
    pub key: String,
    pub title: String,
    pub account: String,
    pub status: Status,
    pub live: bool,
    /// The worktree it's listed under, and its short name.
    pub worktree: String,
    pub callsign: String,
    pub project: String,
    pub branch: Option<String>,
    /// PRs it worked on (`owner/name#number`), and todos linked to it.
    pub prs: Vec<String>,
    pub todos: Vec<String>,
    /// What it's doing or asking now, as its row in Codealong says.
    pub excerpt: Option<String>,
    /// For `session suggest` and `session tag`: its `lastPromptAt` as this look read it, when its
    /// user last prompted it. A message from another session or from Codealong doesn't move it.
    pub last_prompt_at: i64,
    /// What began its latest turn, and its last reply (the first line and the end of a long one),
    /// when they were read: `codealong changes` reads them for the first `--texts` changes. That's
    /// its user's latest prompt, or a message sent after it (`prompt_from`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    /// Who sent `prompt`, when it wasn't its user.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_from: Option<PromptFrom>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply: Option<String>,
    /// It has replies suggested to its user (`session suggest`), or is tagged by an agent
    /// (`session tag`): the tagging session's key.
    pub suggested: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tagged_by: Option<String>,
    /// It works for another agent (`start --helper`, an automatic review), not for its user.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub helper: bool,
}

/// Who sent the message that began a session's latest turn, when it wasn't its user.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum PromptFrom {
    /// Another session: `codealong session send`, or Claude Code's own messages between sessions.
    Agent,
    /// Codealong by itself (send.rs `tell`): a PR's news, a helper's.
    Codealong,
}

/// The headless runs in a worktree that changed since the earlier look (`Session::headless`), in
/// one line rather than a row each.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HeadlessRow {
    pub worktree: String,
    pub callsign: String,
    pub project: String,
    /// How many started, took a turn, ended one or stopped.
    pub runs: usize,
    /// How many of those are working now.
    pub working: usize,
}

/// A session that waits on its user now, as `sessions --waiting` lists it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WaitingRow {
    pub key: String,
    pub title: String,
    pub account: String,
    pub status: Status,
    pub callsign: String,
    /// Since when it waits.
    pub since: i64,
    pub last_prompt_at: i64,
    pub suggested: bool,
    pub tagged: bool,
    /// It's among this look's changed sessions, with what it said there.
    pub changed: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PrRow {
    pub change: PrChange,
    pub key: String,
    pub title: String,
    pub url: String,
    pub project: String,
    pub state: PrState,
    /// Its state at the earlier look, when it changed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub was: Option<PrState>,
    pub author: Option<String>,
    pub mine: bool,
    pub review_requested: bool,
    /// The sessions that worked on it, by key.
    pub sessions: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TodoRow {
    pub change: TodoChange,
    pub id: String,
    pub title: String,
    pub status: TodoStatus,
    pub project: String,
    /// What it waits for, while it's marked waiting.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub waiting: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SharedRow {
    pub worktree: String,
    pub callsign: String,
    pub project: String,
    /// A repo's main checkout, where several agents at once may be the way that repo works.
    pub main: bool,
    pub sessions: Vec<SharedSession>,
    /// The ones that weren't working there at the earlier look.
    pub joined: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SharedSession {
    pub key: String,
    pub title: String,
    pub status: Status,
}

/// What a look found.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Changes {
    /// What to give `--since` next time: it names this look.
    pub cursor: String,
    /// What it compared with: the earlier look's time, or the time given.
    pub since: i64,
    pub at: i64,
    /// An earlier look was found for `--since`, so what changed is told by difference; else by
    /// times alone, which can't tell a review's change.
    pub compared: bool,
    /// What couldn't be read, or read in full, this time.
    pub notes: Vec<String>,
    pub sessions: Vec<SessionRow>,
    /// Headless runs that changed, by worktree, the most first; `sessions` leaves them out.
    #[serde(default)]
    pub headless: Vec<HeadlessRow>,
    pub waiting: Vec<WaitingRow>,
    /// None when PRs couldn't be read.
    pub prs: Option<Vec<PrRow>>,
    pub todos: Vec<TodoRow>,
    pub shared: Vec<SharedRow>,
}

impl Changes {
    /// Nothing changed and nothing waits on the user without a suggestion or a tag. Headless
    /// runs don't count: what ran them reads them.
    pub fn quiet(&self) -> bool {
        self.sessions.is_empty() && self.prs.as_ref().map_or(true, Vec::is_empty) && self.todos.is_empty() && self.shared.is_empty()
    }
}

/// What this look can read of the board.
pub struct Sight<'a> {
    pub board: &'a Board,
    /// The sessions are all there: the app's first scan has ended.
    pub sessions: bool,
    /// The PRs are there: the board is the app's, and GitHub has answered.
    pub prs: bool,
    /// The todos are there: the app's first scan has ended, and todos.json read. A board without
    /// them has none, and every todo would look removed.
    pub todos: bool,
    /// The session asking, which is left out.
    pub me: Option<&'a str>,
    /// Only this project's changes are told; every item is still marked.
    pub project: Option<&'a str>,
}

/// What changed since `earlier` (or, where it has no mark, since `since`), and the look to keep
/// for next time. The cursor is left for `keep` to give.
pub fn compare(sight: &Sight, earlier: Option<&Look>, since: i64, now: i64) -> (Changes, Look) {
    let board = sight.board;
    let in_project = |p: &str| sight.project.map_or(true, |want| want.eq_ignore_ascii_case(p));
    let mut out = Changes { since, at: now, compared: earlier.is_some(), ..Changes::default() };
    let mut look = Look { at: now, ..Look::default() };
    let since_of = |at: Option<i64>| at.unwrap_or(since);

    // Sessions.
    let sessions = homes(board);
    if sight.sessions {
        let part = earlier.and_then(|l| l.sessions.as_ref());
        let from = since_of(part.map(|p| p.at));
        let mut marks = BTreeMap::new();
        for (s, lane) in &sessions {
            if s.archived {
                continue;
            }
            marks.insert(s.key.clone(), SessionMark { status: s.status, prompt_at: s.last_prompt_at });
            if sight.me == Some(s.key.as_str()) || !in_project(&lane.project) {
                continue;
            }
            match session_change(s, part.and_then(|p| p.marks.get(&s.key)), from) {
                Some(_) if s.headless => count_headless(&mut out.headless, s, lane),
                Some(change) => out.sessions.push(session_row(change, s, lane, board)),
                None => {}
            }
        }
        look.sessions = Some(Part { at: now, marks, repos: BTreeMap::new() });
        // Most in need of the user first, then the newest.
        out.sessions.sort_by_key(|r| (rank(r.change), -r.last_prompt_at));
        out.headless.sort_by(|a, b| b.runs.cmp(&a.runs).then_with(|| a.callsign.cmp(&b.callsign)));
    } else {
        look.sessions = Some(kept(earlier.and_then(|l| l.sessions.as_ref()), since));
        out.notes.push("Sessions: the app is still reading them after a start; nothing about them is told until it's done.".into());
    }

    // What waits on the user now.
    let changed: BTreeSet<&str> = out.sessions.iter().map(|r| r.key.as_str()).collect();
    out.waiting = sessions
        .iter()
        .filter(|(s, lane)| sight.sessions && waits_on_user(s) && sight.me != Some(s.key.as_str()) && in_project(&lane.project))
        .map(|(s, lane)| WaitingRow {
            key: s.key.clone(),
            title: s.title.clone(),
            account: s.account.clone(),
            status: s.status,
            callsign: lane.callsign.clone(),
            since: crate::board::waiting_since(s).unwrap_or(s.status_since.unwrap_or(s.last_activity)),
            last_prompt_at: s.last_prompt_at,
            suggested: s.suggestions.is_some(),
            tagged: s.insight.as_ref().is_some_and(|i| i.by.is_some()),
            changed: changed.contains(s.key.as_str()),
        })
        .collect();
    out.waiting.sort_by_key(|w| w.since);

    // PRs, each repo's as of when it was last as GitHub had it (`github_current`), which holds
    // still while its read fails or is on its way: a PR the earlier look had no mark for is told
    // against when its repo was last current then, so what happened meanwhile is told once it is
    // current again, and nothing from before looks new.
    let pr_part = earlier.and_then(|l| l.prs.as_ref());
    if sight.prs {
        let from = since_of(pr_part.map(|p| p.at));
        let boundary = |slug: &str| pr_part.and_then(|p| p.repos.get(slug).copied()).unwrap_or(from);
        let mut marks: BTreeMap<String, PrMark> = pr_part.map(|p| p.marks.iter().filter(|(_, m)| now - m.seen < PR_KEPT_MS).map(|(k, m)| (k.clone(), m.clone())).collect()).unwrap_or_default();
        // How current a repo's PRs on the board are: one polled but never read, not at all.
        let fresh = board.github_at.map_or(now, |t| t.min(now));
        let current = |slug: &str| match board.github_current.get(slug) {
            Some(Some(t)) => *t,
            Some(None) => i64::MIN,
            None => fresh,
        };
        let mut rows = Vec::new();
        for p in &board.prs {
            let mark = pr_part.and_then(|part| part.marks.get(&p.key));
            // A PR is told by GitHub's word newer than its mark's: what Codealong shows ahead of it
            // (a merge done here), or an answer kept from before a restart, changes nothing.
            if mark.is_some_and(|m| current(&p.repo_slug) <= m.current) {
                continue;
            }
            if in_project(&p.project) {
                if let Some((change, was)) = pr_change(p, mark, boundary(&p.repo_slug)) {
                    rows.push(pr_row(change, was, p));
                }
            }
            marks.insert(p.key.clone(), PrMark { state: p.state, asks_you: p.review_requested, seen: now, current: current(&p.repo_slug) });
        }
        let slugs: BTreeSet<&str> = board.prs.iter().map(|p| p.repo_slug.as_str()).chain(board.github_current.keys().map(String::as_str)).chain(pr_part.into_iter().flat_map(|p| p.repos.keys().map(String::as_str))).collect();
        let repos = slugs.into_iter().map(|s| (s.to_string(), board.github_current.get(s).copied().flatten().unwrap_or_else(|| boundary(s)))).collect();
        look.prs = Some(Part { at: fresh, marks, repos });
        out.prs = Some(rows);
    } else {
        look.prs = Some(kept(pr_part, since));
    }

    // Todos.
    let todo_part = earlier.and_then(|l| l.todos.as_ref());
    if sight.todos {
        let from = since_of(todo_part.map(|p| p.at));
        let mut marks = BTreeMap::new();
        for t in &board.todos {
            if in_project(&t.project) {
                if let Some(change) = todo_change(t, todo_part.and_then(|p| p.marks.get(&t.id)), from) {
                    out.todos.push(todo_row(change, t));
                }
            }
            marks.insert(t.id.clone(), TodoMark { status: t.status, waiting: t.waiting.is_some(), title: t.title.clone() });
        }
        for (id, m) in todo_part.map(|p| &p.marks).into_iter().flatten() {
            if !marks.contains_key(id) && sight.project.is_none() {
                out.todos.push(TodoRow { change: TodoChange::Removed, id: id.clone(), title: m.title.clone(), status: m.status, project: String::new(), waiting: None });
            }
        }
        look.todos = Some(Part { at: now, marks, repos: BTreeMap::new() });
    } else {
        look.todos = Some(kept(todo_part, since));
    }

    // Worktrees that agents share.
    if sight.sessions {
        let part = earlier.and_then(|l| l.shared.as_ref());
        let mut marks = BTreeMap::new();
        for lane in board.lanes.iter().filter(|l| !l.chats) {
            let here: Vec<&Session> = lane.sessions.iter().filter(|s| working_here(s, now)).collect();
            if here.is_empty() {
                continue;
            }
            let keys: Vec<String> = here.iter().map(|s| s.key.clone()).collect();
            let before = part.and_then(|p| p.marks.get(&lane.id));
            let joined: Vec<String> = keys.iter().filter(|k| before.map_or(true, |b| !b.contains(k))).cloned().collect();
            if here.len() >= 2 && !joined.is_empty() && in_project(&lane.project) {
                out.shared.push(SharedRow {
                    worktree: lane.id.clone(),
                    callsign: lane.callsign.clone(),
                    project: lane.project.clone(),
                    main: lane.origin == Origin::Main,
                    sessions: here.iter().map(|s| SharedSession { key: s.key.clone(), title: s.title.clone(), status: s.status }).collect(),
                    joined,
                });
            }
            marks.insert(lane.id.clone(), keys);
        }
        look.shared = Some(Part { at: now, marks, repos: BTreeMap::new() });
    } else {
        look.shared = earlier.and_then(|l| l.shared.clone());
    }
    (out, look)
}

/// The part a look keeps for one it couldn't read: the earlier look's, or with none, one with no
/// marks from where this look began, so the next look tells what happened since then by times
/// rather than only what happened since this one.
fn kept<T: Clone>(earlier: Option<&Part<T>>, since: i64) -> Part<T> {
    earlier.cloned().unwrap_or_else(|| Part { at: since, marks: BTreeMap::new(), repos: BTreeMap::new() })
}

/// Every session once, with the lane it's listed under as its own rather than one it visits.
fn homes(board: &Board) -> Vec<(&Session, &Lane)> {
    let mut seen = BTreeSet::new();
    let own = board.lanes.iter().flat_map(|l| l.sessions.iter().filter(|s| s.visitor.is_none()).map(move |s| (s, l)));
    let visiting = board.lanes.iter().flat_map(|l| l.sessions.iter().filter(|s| s.visitor.is_some()).map(move |s| (s, l)));
    own.chain(visiting).filter(|(s, _)| seen.insert(s.key.as_str())).collect()
}

fn waits(status: Status) -> bool {
    matches!(status, Status::Needs | Status::Error)
}

/// Waits on its user as Codealong's queue has it (`sessions --waiting`).
fn waits_on_user(s: &Session) -> bool {
    !s.archived && matches!(s.status, Status::Needs | Status::Error | Status::Unread) && s.snoozed_until.is_none()
}

/// What happened to `s` since its mark, or with none since `since`. Archived sessions are left
/// out, and so are a helper's turns, which are its agent's to read: only its waiting on the user
/// is told.
pub fn session_change(s: &Session, mark: Option<&SessionMark>, since: i64) -> Option<SessionChange> {
    use SessionChange::*;
    if s.archived || (s.helper.is_some() && !waits(s.status)) {
        return None;
    }
    let now = match s.status {
        Status::Needs | Status::Error => Waiting,
        Status::Working => Working,
        Status::Unread | Status::Idle => Finished,
        Status::Stopped => Stopped,
    };
    match mark {
        Some(m) => {
            let new_turn = m.prompt_at != s.last_prompt_at;
            let changed = match s.status {
                Status::Needs | Status::Error => !waits(m.status) || new_turn,
                Status::Working => m.status != Status::Working || new_turn,
                Status::Unread | Status::Idle => matches!(m.status, Status::Working | Status::Needs | Status::Error) || new_turn,
                Status::Stopped => m.status != Status::Stopped,
            };
            changed.then_some(now)
        }
        None if s.started_at.is_some_and(|t| t >= since) => Some(New),
        None => (s.last_activity >= since).then_some(now),
    }
}

/// Counts a headless run that changed into its worktree's line.
fn count_headless(rows: &mut Vec<HeadlessRow>, s: &Session, lane: &Lane) {
    let at = match rows.iter().position(|r| r.worktree == lane.id) {
        Some(at) => at,
        None => {
            rows.push(HeadlessRow { worktree: lane.id.clone(), callsign: lane.callsign.clone(), project: lane.project.clone(), runs: 0, working: 0 });
            rows.len() - 1
        }
    };
    rows[at].runs += 1;
    rows[at].working += usize::from(s.status == Status::Working);
}

/// The order changes are told in: what the user is needed for first.
fn rank(c: SessionChange) -> u8 {
    match c {
        SessionChange::Waiting => 0,
        SessionChange::Finished => 1,
        SessionChange::New => 2,
        SessionChange::Working => 3,
        SessionChange::Stopped => 4,
    }
}

fn session_row(change: SessionChange, s: &Session, lane: &Lane, board: &Board) -> SessionRow {
    SessionRow {
        change,
        key: s.key.clone(),
        title: s.title.clone(),
        account: s.account.clone(),
        status: s.status,
        live: s.live,
        worktree: lane.id.clone(),
        callsign: lane.callsign.clone(),
        project: lane.project.clone(),
        branch: s.branch.clone().or_else(|| lane.branch.clone()),
        prs: board.prs.iter().filter(|p| p.sessions.iter().any(|x| x.key == s.key)).map(|p| p.key.clone()).collect(),
        todos: board.todos.iter().filter(|t| t.sessions.contains(&s.key)).map(|t| t.id.clone()).collect(),
        excerpt: s.excerpt.clone(),
        last_prompt_at: s.last_prompt_at,
        prompt: None,
        prompt_from: None,
        reply: None,
        suggested: s.suggestions.is_some(),
        tagged_by: s.insight.as_ref().and_then(|i| i.by.clone()),
        helper: s.helper.is_some(),
    }
}

/// What happened to a PR since its mark, or with none since `since` (by its own times: a review
/// has none), and its state at the mark when that changed. One opened and merged since is told
/// merged.
pub fn pr_change(p: &PrItem, mark: Option<&PrMark>, since: i64) -> Option<(PrChange, Option<PrState>)> {
    match mark {
        Some(m) if m.state != p.state => Some((by_state(m.state, p.state), Some(m.state))),
        Some(m) => (p.state.is_open() && p.review_requested && !m.asks_you).then_some((PrChange::ReviewAsked, None)),
        None if p.state == PrState::Merged && p.merged_at.is_some_and(|t| t >= since) => Some((PrChange::Merged, None)),
        None if p.state == PrState::Closed && p.closed_at.is_some_and(|t| t >= since) => Some((PrChange::Closed, None)),
        None if p.created_at >= since => Some((PrChange::Opened, None)),
        None => None,
    }
}

fn by_state(was: PrState, now: PrState) -> PrChange {
    match now {
        PrState::Merged => PrChange::Merged,
        PrState::Closed => PrChange::Closed,
        PrState::Approved => PrChange::Approved,
        PrState::Changes => PrChange::ChangesRequested,
        PrState::Draft => PrChange::Draft,
        PrState::Open => match was {
            PrState::Draft => PrChange::Ready,
            PrState::Merged | PrState::Closed => PrChange::Reopened,
            _ => PrChange::ReviewReset,
        },
    }
}

fn pr_row(change: PrChange, was: Option<PrState>, p: &PrItem) -> PrRow {
    PrRow {
        change,
        key: p.key.clone(),
        title: p.title.clone(),
        url: p.url.clone(),
        project: p.project.clone(),
        state: p.state,
        was,
        author: p.author.clone(),
        mine: p.mine,
        review_requested: p.review_requested,
        sessions: p.sessions.iter().map(|s| s.key.clone()).collect(),
    }
}

/// What happened to a todo since its mark, or with none since `since`.
pub fn todo_change(t: &Todo, mark: Option<&TodoMark>, since: i64) -> Option<TodoChange> {
    match mark {
        Some(m) if m.status != t.status => Some(match t.status {
            TodoStatus::Done => TodoChange::Done,
            TodoStatus::InProgress => TodoChange::Started,
            TodoStatus::Todo => TodoChange::Reopened,
        }),
        Some(m) if m.waiting != t.waiting.is_some() => Some(if t.waiting.is_some() { TodoChange::Waiting } else { TodoChange::Ready }),
        Some(_) => None,
        None if t.created_at >= since => Some(TodoChange::Added),
        None if t.done_at.is_some_and(|d| d >= since) => Some(TodoChange::Done),
        None => None,
    }
}

fn todo_row(change: TodoChange, t: &Todo) -> TodoRow {
    TodoRow { change, id: t.id.clone(), title: t.title.clone(), status: t.status, project: t.project.clone(), waiting: t.waiting.as_ref().map(|w| w.note.clone()) }
}

/// A session of a worktree's own that's at work there now: working or waiting on its user, or
/// live and active within the hour. Helpers work for another session and headless runs for
/// what ran them, so neither is counted.
fn working_here(s: &Session, now: i64) -> bool {
    s.visitor.is_none() && !s.archived && s.helper.is_none() && !s.headless && (matches!(s.status, Status::Working | Status::Needs | Status::Error) || (s.live && now - s.last_activity < ACTIVE_MS))
}

/// The message that began the latest turn in a page of a conversation, who sent it when it wasn't
/// the session's user, and the reply after it: the last thing its agent said since, whole when
/// it's short, else its first line and its end, where a status line and a question go. Another
/// session's message is a note of it where its harness records it as one (Claude Code), else a
/// prompt led by the byline `session send` gave it; Codealong's own is `codealong`.
pub fn texts(items: &[api::Item]) -> (Option<String>, Option<PromptFrom>, Option<String>) {
    fn said(i: &api::Item) -> Option<(&str, Option<PromptFrom>)> {
        match i.kind.as_str() {
            "you" if i.withdrawn => None,
            "you" => Some(match crate::send::from_an_agent(&i.text) {
                Some(text) => (text, Some(PromptFrom::Agent)),
                None => (i.text.as_str(), None),
            }),
            "note" if i.text.starts_with(crate::send::FROM_ANOTHER_SESSION) => Some((i.text.as_str(), Some(PromptFrom::Agent))),
            "codealong" => Some((i.text.as_str(), Some(PromptFrom::Codealong))),
            _ => None,
        }
    }
    let last = items.iter().rposition(|i| said(i).is_some());
    let (prompt, from) = last.and_then(|i| said(&items[i])).map_or((None, None), |(text, from)| (Some(crate::util::truncate(text.trim(), PROMPT_CUT)).filter(|p| !p.is_empty()), from));
    let after = last.map_or(0, |i| i + 1);
    let reply = items[after..].iter().rev().find(|i| i.kind == "agent" && !i.text.trim().is_empty()).map(|i| cut_reply(i.text.trim()));
    let from = from.filter(|_| prompt.is_some());
    (prompt, from, reply)
}

fn cut_reply(text: &str) -> String {
    if text.chars().count() <= REPLY_WHOLE {
        return text.to_string();
    }
    let head = crate::util::truncate(text.lines().next().unwrap_or("").trim(), REPLY_HEAD);
    let chars: Vec<char> = text.chars().collect();
    let tail: String = chars[chars.len() - REPLY_TAIL..].iter().collect();
    format!("{head}\n…\n{}", tail.trim_start())
}

/// Where `--since` points.
#[derive(Debug, Clone, PartialEq)]
pub enum Since {
    /// A cursor an earlier look printed.
    Cursor(String),
    /// A time, from a duration given.
    Time(i64),
}

/// A cursor (digits), or a duration back from `now`: 90m, 24h, 3d, 1w.
pub fn parse_since(s: &str, now: i64) -> Option<Since> {
    let s = s.trim();
    if !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()) {
        return Some(Since::Cursor(s.to_string()));
    }
    crate::idle::parse_after(s).map(|ms| Since::Time(now - ms))
}

/// The looks kept, by cursor.
#[derive(Serialize, Deserialize, Default)]
struct Looks {
    looks: BTreeMap<String, Look>,
}

fn path() -> PathBuf {
    crate::config::support_dir().join("changes.json")
}

/// The look a cursor names, if it's still kept.
pub fn earlier(cursor: &str) -> Result<Option<Look>> {
    Ok(crate::store::read::<Looks>(&path(), "changes.json")?.and_then(|mut l| l.looks.remove(cursor)))
}

/// Keeps a look under a cursor of its own, which it returns, and lets go of the oldest.
pub fn keep(look: Look) -> Result<String> {
    let mut cursor = String::new();
    crate::store::change(&path(), "changes.json", |all: &mut Looks| {
        cursor = file(&mut all.looks, look);
        true
    })?;
    Ok(cursor)
}

/// Puts a look into the kept ones under the first free cursor from its time, and lets go of those
/// older than a week and the oldest past `KEEP_LOOKS`.
fn file(looks: &mut BTreeMap<String, Look>, look: Look) -> String {
    let mut at = look.at;
    while looks.contains_key(&at.to_string()) {
        at += 1;
    }
    let cursor = at.to_string();
    let now = look.at;
    looks.insert(cursor.clone(), look);
    looks.retain(|k, l| *k == cursor || now - l.at < KEEP_MS);
    while looks.len() > KEEP_LOOKS {
        let Some(oldest) = looks.keys().find(|k| **k != cursor).cloned() else { break };
        looks.remove(&oldest);
    }
    cursor
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::{lane, session};
    use crate::model::{Checks, Merge};

    const NOW: i64 = 1_800_000_000_000;
    const MIN: i64 = 60_000;

    fn s(key: &str, status: Status, prompt_at: i64) -> Session {
        Session { status, last_prompt_at: prompt_at, last_activity: prompt_at, started_at: Some(NOW - DAY_MS), ..session(key) }
    }

    fn mark(status: Status, prompt_at: i64) -> SessionMark {
        SessionMark { status, prompt_at }
    }

    #[test]
    fn a_session_is_told_once_per_change_of_where_it_stands() {
        let t = NOW - 30 * MIN;
        let since = NOW - 15 * MIN;
        // Working then, its turn ended since: finished; seen as it was: nothing.
        assert_eq!(session_change(&s("a", Status::Unread, t), Some(&mark(Status::Working, t)), since), Some(SessionChange::Finished));
        assert_eq!(session_change(&s("a", Status::Unread, t), Some(&mark(Status::Unread, t)), since), None);
        // Read since (unread to idle) is no change; a whole turn between the looks is.
        assert_eq!(session_change(&s("a", Status::Idle, t), Some(&mark(Status::Unread, t)), since), None);
        assert_eq!(session_change(&s("a", Status::Idle, NOW - MIN), Some(&mark(Status::Idle, t)), since), Some(SessionChange::Finished));
        // Waiting on its user: told when it starts waiting, not again while it waits on the same turn.
        assert_eq!(session_change(&s("a", Status::Needs, t), Some(&mark(Status::Working, t)), since), Some(SessionChange::Waiting));
        assert_eq!(session_change(&s("a", Status::Error, t), Some(&mark(Status::Needs, t)), since), None);
        // A new prompt while it works, and going on from waiting.
        assert_eq!(session_change(&s("a", Status::Working, NOW - MIN), Some(&mark(Status::Working, t)), since), Some(SessionChange::Working));
        assert_eq!(session_change(&s("a", Status::Working, t), Some(&mark(Status::Needs, t)), since), Some(SessionChange::Working));
        assert_eq!(session_change(&s("a", Status::Working, t), Some(&mark(Status::Working, t)), since), None);
        assert_eq!(session_change(&s("a", Status::Stopped, t), Some(&mark(Status::Idle, t)), since), Some(SessionChange::Stopped));
        assert_eq!(session_change(&s("a", Status::Stopped, t), Some(&mark(Status::Stopped, t)), since), None);
    }

    #[test]
    fn with_no_mark_times_decide() {
        let since = NOW - 15 * MIN;
        let started = Session { started_at: Some(NOW - 5 * MIN), ..s("a", Status::Working, NOW - 5 * MIN) };
        assert_eq!(session_change(&started, None, since), Some(SessionChange::New));
        assert_eq!(session_change(&s("a", Status::Needs, NOW - 5 * MIN), None, since), Some(SessionChange::Waiting));
        assert_eq!(session_change(&s("a", Status::Unread, NOW - 20 * MIN), None, since), None);
    }

    #[test]
    fn archived_sessions_and_a_helpers_turns_are_left_out() {
        let since = NOW - 15 * MIN;
        let archived = Session { archived: true, ..s("a", Status::Needs, NOW - MIN) };
        assert_eq!(session_change(&archived, None, since), None);
        let helper = Session { helper: Some(crate::model::Helper { parent: Some("p".into()), ..Default::default() }), ..s("h", Status::Idle, NOW - MIN) };
        assert_eq!(session_change(&helper, Some(&mark(Status::Working, NOW - 20 * MIN)), since), None);
        let asks = Session { status: Status::Needs, ..helper };
        assert_eq!(session_change(&asks, Some(&mark(Status::Working, NOW - 20 * MIN)), since), Some(SessionChange::Waiting));
    }

    fn pr(n: u64, state: PrState) -> PrItem {
        PrItem {
            key: format!("o/r#{n}"),
            repo_slug: "o/r".into(),
            repo: "r".into(),
            project: "P".into(),
            number: n,
            title: format!("PR {n}"),
            url: format!("https://github.com/o/r/pull/{n}"),
            state,
            author: Some("ana".into()),
            mine: false,
            review_requested: false,
            requested_by: None,
            requested_at: None,
            reviewers_asked: Vec::new(),
            snoozed_until: None,
            nudge: false,
            nudge_by_default: false,
            primary: None,
            primary_by: None,
            head: "b".into(),
            base: Some("main".into()),
            checks: Checks::default(),
            merge: Merge::Unknown,
            additions: 1,
            deletions: 0,
            changed_files: 1,
            created_at: NOW - DAY_MS,
            updated_at: NOW - DAY_MS,
            merged_at: None,
            closed_at: None,
            lanes: Vec::new(),
            sessions: Vec::new(),
            head_oid: None,
            auto_review: None,
        }
    }

    fn pr_mark(state: PrState) -> PrMark {
        PrMark { state, asks_you: false, seen: NOW - 15 * MIN, current: i64::MIN }
    }

    #[test]
    fn a_prs_review_and_merge_are_told_from_its_mark() {
        let since = NOW - 15 * MIN;
        assert_eq!(pr_change(&pr(1, PrState::Approved), Some(&pr_mark(PrState::Open)), since), Some((PrChange::Approved, Some(PrState::Open))));
        assert_eq!(pr_change(&pr(1, PrState::Open), Some(&pr_mark(PrState::Draft)), since), Some((PrChange::Ready, Some(PrState::Draft))));
        assert_eq!(pr_change(&pr(1, PrState::Open), Some(&pr_mark(PrState::Changes)), since), Some((PrChange::ReviewReset, Some(PrState::Changes))));
        assert_eq!(pr_change(&pr(1, PrState::Merged), Some(&pr_mark(PrState::Approved)), since), Some((PrChange::Merged, Some(PrState::Approved))));
        assert_eq!(pr_change(&pr(1, PrState::Open), Some(&pr_mark(PrState::Open)), since), None);
        let asked = PrItem { review_requested: true, ..pr(1, PrState::Open) };
        assert_eq!(pr_change(&asked, Some(&pr_mark(PrState::Open)), since), Some((PrChange::ReviewAsked, None)));
        assert_eq!(pr_change(&asked, Some(&PrMark { asks_you: true, ..pr_mark(PrState::Open) }), since), None);
    }

    #[test]
    fn a_pr_with_no_mark_is_new_only_when_it_was_opened_since() {
        let since = NOW - 15 * MIN;
        // Listed for the first time but opened a day ago: its repo didn't answer before, say.
        assert_eq!(pr_change(&pr(1, PrState::Open), None, since), None);
        assert_eq!(pr_change(&PrItem { created_at: NOW - 5 * MIN, ..pr(1, PrState::Open) }, None, since), Some((PrChange::Opened, None)));
        assert_eq!(pr_change(&PrItem { merged_at: Some(NOW - 5 * MIN), ..pr(1, PrState::Merged) }, None, since), Some((PrChange::Merged, None)));
        assert_eq!(pr_change(&PrItem { merged_at: Some(NOW - 50 * MIN), ..pr(1, PrState::Merged) }, None, since), None);
        // Opened and merged since: merged.
        assert_eq!(pr_change(&PrItem { created_at: NOW - 9 * MIN, merged_at: Some(NOW - 5 * MIN), ..pr(1, PrState::Merged) }, None, since), Some((PrChange::Merged, None)));
    }

    #[test]
    fn a_repo_that_isnt_current_holds_its_time_while_others_move_on() {
        let other = |n: u64, state: PrState| PrItem { key: format!("x/y#{n}"), repo_slug: "x/y".into(), ..pr(n, state) };
        let sight = |b| Sight { board: b, sessions: true, prs: true, todos: true, me: None, project: None };
        let current = |or: i64, xy: Option<i64>| [("o/r".to_string(), Some(or)), ("x/y".to_string(), xy)].into_iter().collect();
        // The first look comes while x/y has never been read: it goes from where the look begins.
        let closed = PrItem { closed_at: Some(NOW - DAY_MS), ..other(3, PrState::Closed) };
        let failing = Board { github_at: Some(NOW - 10 * MIN), github_current: current(NOW - 10 * MIN, None), ..board(vec![], vec![pr(1, PrState::Open), other(1, PrState::Open), closed.clone()]) };
        let (out, first) = compare(&sight(&failing), None, NOW - 15 * MIN, NOW);
        assert_eq!(out.prs, Some(vec![]));
        assert_eq!(first.prs.as_ref().unwrap().repos.get("x/y"), Some(&(NOW - 15 * MIN)));
        // o/r is read again, and x/y still isn't (its read fails, or is on its way): its time holds.
        let still = Board { github_at: Some(NOW + 5 * MIN), github_current: current(NOW + 5 * MIN, None), ..failing.clone() };
        let (_, second) = compare(&sight(&still), Some(&first), first.at, NOW + 6 * MIN);
        assert_eq!(second.prs.as_ref().unwrap().repos.get("x/y"), Some(&(NOW - 15 * MIN)));
        assert_eq!(second.prs.as_ref().unwrap().repos.get("o/r"), Some(&(NOW + 5 * MIN)));
        // x/y is current again: a PR opened and merged meanwhile is told, and so is the closed one
        // that was reopened; the open one from the day before isn't.
        let merged = PrItem { created_at: NOW, merged_at: Some(NOW + 3 * MIN), ..other(2, PrState::Merged) };
        let back = Board { github_at: Some(NOW + 20 * MIN), github_current: current(NOW + 20 * MIN, Some(NOW + 20 * MIN)), ..board(vec![], vec![pr(1, PrState::Open), other(1, PrState::Open), merged, other(3, PrState::Open)]) };
        let (out, _) = compare(&sight(&back), Some(&second), second.at, NOW + 21 * MIN);
        assert_eq!(out.prs.unwrap().iter().map(|r| (r.key.as_str(), r.change)).collect::<Vec<_>>(), vec![("x/y#2", PrChange::Merged), ("x/y#3", PrChange::Reopened)]);
    }

    #[test]
    fn a_repo_never_read_keeps_where_the_first_look_began() {
        let sight = |b| Sight { board: b, sessions: true, prs: true, todos: true, me: None, project: None };
        // x/y is polled but its read has never come, and the board has no PRs of it.
        let unread = |at: i64| Board { github_at: Some(at), github_current: [("o/r".to_string(), Some(at)), ("x/y".to_string(), None)].into_iter().collect(), ..board(vec![], vec![]) };
        let (first_board, second_board) = (unread(NOW - 10 * MIN), unread(NOW + 5 * MIN));
        let (_, first) = compare(&sight(&first_board), None, NOW - 15 * MIN, NOW);
        let (_, second) = compare(&sight(&second_board), Some(&first), first.at, NOW + 6 * MIN);
        assert_eq!(second.prs.as_ref().unwrap().repos.get("x/y"), Some(&(NOW - 15 * MIN)));
        // Its read comes: a PR opened while it hadn't is told.
        let opened = PrItem { key: "x/y#1".into(), repo_slug: "x/y".into(), created_at: NOW, ..pr(1, PrState::Open) };
        let read = Board { github_at: Some(NOW + 20 * MIN), github_current: [("o/r".to_string(), Some(NOW + 20 * MIN)), ("x/y".to_string(), Some(NOW + 20 * MIN))].into_iter().collect(), ..board(vec![], vec![opened]) };
        let (out, _) = compare(&sight(&read), Some(&second), second.at, NOW + 21 * MIN);
        assert_eq!(out.prs.unwrap().iter().map(|r| (r.key.as_str(), r.change)).collect::<Vec<_>>(), vec![("x/y#1", PrChange::Opened)]);
    }

    #[test]
    fn a_pr_changes_only_by_github_word_newer_than_its_mark() {
        let sight = |b| Sight { board: b, sessions: true, prs: true, todos: true, me: None, project: None };
        let at = |t: i64, p: PrItem| Board { github_at: Some(t), github_current: [("o/r".to_string(), Some(t))].into_iter().collect(), ..board(vec![], vec![p]) };
        let read = at(NOW - 10 * MIN, pr(1, PrState::Open));
        let (_, first) = compare(&sight(&read), None, NOW - 15 * MIN, NOW);
        // Merged here: Codealong shows it at once, ahead of GitHub's next read. Nothing's told yet.
        let merged_here = at(NOW - 10 * MIN, PrItem { merged_at: Some(NOW + MIN), ..pr(1, PrState::Merged) });
        let (out, second) = compare(&sight(&merged_here), Some(&first), first.at, NOW + 2 * MIN);
        assert_eq!(out.prs, Some(vec![]));
        // A restart with GitHub refusing hands back the answer kept from before: still nothing.
        let (out, third) = compare(&sight(&read), Some(&second), second.at, NOW + 3 * MIN);
        assert_eq!(out.prs, Some(vec![]));
        // GitHub's next read says merged: told once.
        let confirmed = at(NOW + 20 * MIN, PrItem { merged_at: Some(NOW + MIN), ..pr(1, PrState::Merged) });
        let (out, fourth) = compare(&sight(&confirmed), Some(&third), third.at, NOW + 21 * MIN);
        assert_eq!(out.prs.unwrap().iter().map(|r| (r.key.as_str(), r.change)).collect::<Vec<_>>(), vec![("o/r#1", PrChange::Merged)]);
        let (out, _) = compare(&sight(&confirmed), Some(&fourth), fourth.at, NOW + 22 * MIN);
        assert_eq!(out.prs, Some(vec![]));
    }

    #[test]
    fn a_closed_prs_mark_outlasts_its_repo_dropping_off_the_board() {
        // The board stops listing a closed PR (its repo fails after a restart): its mark stays for a
        // week, so its reopening is told once it's back.
        let closed = PrItem { closed_at: Some(NOW - DAY_MS), ..pr(4, PrState::Closed) };
        let sight = |b| Sight { board: b, sessions: true, prs: true, todos: true, me: None, project: None };
        let (listed, gone, back) = (board(vec![], vec![closed]), board(vec![], vec![]), board(vec![], vec![pr(4, PrState::Open)]));
        let (_, first) = compare(&sight(&listed), None, NOW - 15 * MIN, NOW);
        let (_, second) = compare(&sight(&gone), Some(&first), first.at, NOW + MIN);
        let (out, _) = compare(&sight(&back), Some(&second), second.at, NOW + 2 * MIN);
        assert_eq!(out.prs.unwrap().iter().map(|r| (r.key.as_str(), r.change)).collect::<Vec<_>>(), vec![("o/r#4", PrChange::Reopened)]);
    }

    #[test]
    fn prs_are_as_of_when_github_last_answered_so_an_outage_hides_nothing() {
        // GitHub stopped answering an hour ago; a PR opened during it shows once it answers again.
        let stale = Board { github_at: Some(NOW - 60 * MIN), ..board(vec![], vec![pr(1, PrState::Open)]) };
        let (_, first) = compare(&Sight { board: &stale, sessions: true, prs: true, todos: true, me: None, project: None }, None, NOW - 2 * 60 * MIN, NOW);
        assert_eq!(first.prs.as_ref().unwrap().at, NOW - 60 * MIN);
        let opened = PrItem { created_at: NOW - 30 * MIN, ..pr(2, PrState::Open) };
        let back = Board { github_at: Some(NOW + MIN), ..board(vec![], vec![pr(1, PrState::Open), opened]) };
        let (out, _) = compare(&Sight { board: &back, sessions: true, prs: true, todos: true, me: None, project: None }, Some(&first), first.at, NOW + 2 * MIN);
        assert_eq!(out.prs.unwrap().iter().map(|r| (r.key.as_str(), r.change)).collect::<Vec<_>>(), vec![("o/r#2", PrChange::Opened)]);
    }

    fn board(sessions: Vec<Session>, prs: Vec<PrItem>) -> Board {
        Board { lanes: vec![lane("/r", sessions)], prs, ..Board::default() }
    }

    #[test]
    fn prs_that_cant_be_read_keep_the_earlier_marks_so_nothing_comes_back_new() {
        let since = NOW - 15 * MIN;
        let b = board(vec![], vec![PrItem { created_at: NOW - 20 * MIN, ..pr(1, PrState::Open) }]);
        let sight = |prs| Sight { board: &b, sessions: true, prs, todos: true, me: None, project: None };
        let (_, first) = compare(&sight(true), None, NOW - 30 * MIN, since);
        assert!(first.prs.as_ref().unwrap().marks.contains_key("o/r#1"));
        // GitHub doesn't answer: PRs aren't told, and the marks stay.
        let (out, second) = compare(&sight(false), Some(&first), first.at, NOW);
        assert_eq!(out.prs, None);
        assert_eq!(second.prs, first.prs);
        // A board whose PRs miss one (its repo didn't answer) keeps its mark; when it's back, it isn't new.
        let empty = board(vec![], vec![]);
        let (_, third) = compare(&Sight { board: &empty, sessions: true, prs: true, todos: true, me: None, project: None }, Some(&second), second.at, NOW + MIN);
        assert!(third.prs.as_ref().unwrap().marks.contains_key("o/r#1"));
        let (out, _) = compare(&sight(true), Some(&third), third.at, NOW + 2 * MIN);
        assert_eq!(out.prs, Some(vec![]));
    }

    #[test]
    fn sessions_are_left_alone_until_the_app_has_read_them() {
        let b = board(vec![s("a", Status::Unread, NOW - MIN)], vec![]);
        let earlier = Look { at: NOW - 15 * MIN, sessions: Some(Part { at: NOW - 15 * MIN, marks: BTreeMap::from([("a".to_string(), mark(Status::Working, NOW - MIN))]), repos: BTreeMap::new() }), ..Look::default() };
        let (out, look) = compare(&Sight { board: &b, sessions: false, prs: false, todos: true, me: None, project: None }, Some(&earlier), earlier.at, NOW);
        assert!(out.sessions.is_empty() && !out.notes.is_empty());
        assert_eq!(look.sessions, earlier.sessions);
        let (out, _) = compare(&Sight { board: &b, sessions: true, prs: false, todos: true, me: None, project: None }, Some(&earlier), earlier.at, NOW);
        assert_eq!(out.sessions.iter().map(|r| (r.key.as_str(), r.change)).collect::<Vec<_>>(), vec![("a", SessionChange::Finished)]);
    }

    #[test]
    fn the_asking_session_is_left_out_and_what_waits_is_listed() {
        let b = board(vec![s("me", Status::Unread, NOW - MIN), s("b", Status::Needs, NOW - MIN), s("c", Status::Unread, NOW - DAY_MS)], vec![]);
        let (out, _) = compare(&Sight { board: &b, sessions: true, prs: false, todos: true, me: Some("me"), project: None }, None, NOW - 15 * MIN, NOW);
        assert_eq!(out.sessions.iter().map(|r| r.key.as_str()).collect::<Vec<_>>(), vec!["b"]);
        let waiting: Vec<(&str, bool)> = out.waiting.iter().map(|w| (w.key.as_str(), w.changed)).collect();
        assert_eq!(waiting, vec![("c", false), ("b", true)]);
    }

    #[test]
    fn a_second_agent_in_a_worktree_is_told_once() {
        let one = s("a", Status::Working, NOW - MIN);
        let two = s("b", Status::Working, NOW - MIN);
        let idle_long = Session { live: false, ..s("c", Status::Idle, NOW - DAY_MS) };
        let alone = board(vec![one.clone(), idle_long.clone()], vec![]);
        let both = board(vec![one, two, idle_long], vec![]);
        let sight = |b| Sight { board: b, sessions: true, prs: false, todos: true, me: None, project: None };
        let (out, first) = compare(&sight(&alone), None, NOW - 15 * MIN, NOW - 10 * MIN);
        assert!(out.shared.is_empty());
        let (out, second) = compare(&sight(&both), Some(&first), first.at, NOW);
        assert_eq!(out.shared.len(), 1);
        assert_eq!(out.shared[0].joined, vec!["b".to_string()]);
        let (out, _) = compare(&sight(&both), Some(&second), second.at, NOW + MIN);
        assert!(out.shared.is_empty());
    }

    #[test]
    fn an_apps_chats_share_nothing() {
        let chats = Lane { chats: true, ..lane("/Users/me/.gemini/antigravity-cli/scratch", vec![s("a", Status::Working, NOW - MIN), s("b", Status::Working, NOW - MIN)]) };
        let b = Board { lanes: vec![chats], ..Board::default() };
        let (out, _) = compare(&Sight { board: &b, sessions: true, prs: false, todos: true, me: None, project: None }, None, NOW - 15 * MIN, NOW);
        assert_eq!(out.sessions.len(), 2);
        assert!(out.shared.is_empty());
    }

    #[test]
    fn headless_runs_are_counted_by_worktree_and_share_nothing() {
        let run = |key: &str, status: Status| Session { headless: true, started_at: Some(NOW - 2 * MIN), ..s(key, status, NOW - 2 * MIN) };
        let b = Board {
            lanes: vec![
                lane("/r", vec![s("a", Status::Working, NOW - MIN), run("p1", Status::Working)]),
                lane("/tmp/judge", vec![run("j1", Status::Working), run("j2", Status::Working), run("j3", Status::Idle)]),
            ],
            ..Board::default()
        };
        let (out, look) = compare(&Sight { board: &b, sessions: true, prs: false, todos: true, me: None, project: None }, None, NOW - 15 * MIN, NOW);
        assert_eq!(out.sessions.iter().map(|r| r.key.as_str()).collect::<Vec<_>>(), vec!["a"]);
        let counted: Vec<(&str, usize, usize)> = out.headless.iter().map(|h| (h.callsign.as_str(), h.runs, h.working)).collect();
        assert_eq!(counted, vec![("tmp/judge", 3, 2), ("r", 1, 1)]);
        // Several in one folder, or one beside an agent, isn't a shared worktree.
        assert!(out.shared.is_empty());
        assert!(!out.quiet());
        assert!(Changes { sessions: vec![], ..out.clone() }.quiet(), "headless runs alone are quiet");
        // Marked like any session, so the next look tells only what changed again.
        assert!(look.sessions.as_ref().unwrap().marks.contains_key("j3"));
        let (out, _) = compare(&Sight { board: &b, sessions: true, prs: false, todos: true, me: None, project: None }, Some(&look), look.at, NOW + MIN);
        assert!(out.headless.is_empty());
    }

    #[test]
    fn todos_are_left_alone_until_they_can_be_read() {
        let mut b = board(vec![], vec![]);
        b.todos = vec![todo("t1"), Todo { status: TodoStatus::Done, ..todo("t2") }];
        let sight = |b, todos| Sight { board: b, sessions: true, prs: false, todos, me: None, project: None };
        let (_, first) = compare(&sight(&b, true), None, NOW - 15 * MIN, NOW - 10 * MIN);
        assert_eq!(first.todos.as_ref().unwrap().marks.len(), 2);
        // Right after a restart the app's board has no todos yet: none is gone, and the marks stay.
        let starting = board(vec![], vec![]);
        let (out, second) = compare(&sight(&starting, false), Some(&first), first.at, NOW);
        assert!(out.todos.is_empty());
        assert_eq!(second.todos, first.todos);
        // Once they read, nothing comes back as added.
        let (out, _) = compare(&sight(&b, true), Some(&second), second.at, NOW + MIN);
        assert!(out.todos.is_empty());
    }

    #[test]
    fn a_first_look_that_cant_read_keeps_where_it_began() {
        let b = Board {
            lanes: vec![lane("/r", vec![Session { started_at: Some(NOW - 90 * MIN), ..s("a", Status::Idle, NOW - 90 * MIN) }])],
            todos: vec![Todo { created_at: NOW - 90 * MIN, ..todo("t1") }],
            ..Board::default()
        };
        let sight = |read| Sight { board: &b, sessions: read, prs: false, todos: read, me: None, project: None };
        // The first look, asked for two hours, comes while the app is starting.
        let (_, first) = compare(&sight(false), None, NOW - 120 * MIN, NOW - 60 * MIN);
        // The next goes from that look, and still tells what happened in the two hours.
        let (out, _) = compare(&sight(true), Some(&first), first.at, NOW);
        assert_eq!(out.sessions.iter().map(|r| (r.key.as_str(), r.change)).collect::<Vec<_>>(), vec![("a", SessionChange::New)]);
        assert_eq!(out.todos.iter().map(|r| (r.id.as_str(), r.change)).collect::<Vec<_>>(), vec![("t1", TodoChange::Added)]);
    }

    #[test]
    fn todos_are_told_by_status_and_waiting() {
        let mut t = todo("t1");
        let m = TodoMark { status: TodoStatus::Todo, waiting: false, title: "Fix it".into() };
        assert_eq!(todo_change(&t, Some(&m), NOW - MIN), None);
        t.status = TodoStatus::InProgress;
        assert_eq!(todo_change(&t, Some(&m), NOW - MIN), Some(TodoChange::Started));
        t.status = TodoStatus::Todo;
        t.waiting = Some(crate::todos::Waiting { note: "Wes".into(), since: NOW });
        assert_eq!(todo_change(&t, Some(&m), NOW - MIN), Some(TodoChange::Waiting));
        assert_eq!(todo_change(&t, None, NOW - MIN), None);
        assert_eq!(todo_change(&Todo { created_at: NOW, ..t }, None, NOW - MIN), Some(TodoChange::Added));
    }

    fn item(kind: &str, text: &str) -> api::Item {
        serde_json::from_value(serde_json::json!({ "at": NOW, "kind": kind, "text": text, "cursor": 0, "n": 0 })).unwrap()
    }

    fn todo(id: &str) -> Todo {
        serde_json::from_value(serde_json::json!({ "id": id, "title": "Fix it", "createdAt": NOW - DAY_MS })).unwrap()
    }

    #[test]
    fn a_page_gives_the_latest_prompt_and_the_reply_after_it() {
        let long = format!("Landed: abc is in main.\n{}\nShall I deploy it?", "x".repeat(2000));
        let page = vec![item("you", "First"), item("agent", "Old reply"), item("you", "Ship it"), item("tool", "Bash · `git push`"), item("agent", &long)];
        let (prompt, from, reply) = texts(&page);
        assert_eq!((prompt.as_deref(), from), (Some("Ship it"), None));
        let reply = reply.unwrap();
        assert!(reply.starts_with("Landed: abc is in main.\n…\n") && reply.ends_with("Shall I deploy it?"), "{reply}");
        // A turn that hasn't replied yet has no reply.
        let (_, _, reply) = texts(&[item("agent", "Old reply"), item("you", "Next")]);
        assert_eq!(reply, None);
    }

    /// "Skanksa Review" answered another session's message with a review, and `changes` gave its
    /// user's prompt of the day before as the turn's (2026-10-14).
    #[test]
    fn a_turn_another_session_began_gives_its_message_marked_as_theirs() {
        let theirs = format!("{}, “Antigravity review”: Here's my review of #1227.", crate::send::FROM_ANOTHER_SESSION);
        let page = vec![item("you", "its going now"), item("agent", "Watching it."), item("note", &theirs), item("agent", "Reviewed: two findings.")];
        assert_eq!(texts(&page), (Some(theirs.clone()), Some(PromptFrom::Agent), Some("Reviewed: two findings.".into())));
        // Typed in where its harness takes it so, without the byline `session send` led it with.
        let typed = "This message is from another agent, “Review” (Codex on codex, session k), sent with `codealong session send`; it isn't from your user.\n\nWhich test fails?";
        assert_eq!(texts(&[item("you", "Fix it"), item("you", typed)]).0.as_deref(), Some("Which test fails?"));
        assert_eq!(texts(&[item("you", typed)]).1, Some(PromptFrom::Agent));
        // Codealong's own, and a later prompt of the user's, which is theirs again.
        assert_eq!(texts(&[item("codealong", "On your PR #7: ken approved it.")]).1, Some(PromptFrom::Codealong));
        assert_eq!(texts(&[item("note", &theirs), item("you", "Thanks, merge it")]).0.as_deref(), Some("Thanks, merge it"));
        assert_eq!(texts(&[item("note", &theirs), item("you", "Thanks, merge it")]).1, None);
        // A note of something else begins no turn.
        assert_eq!(texts(&[item("you", "Fix it"), item("note", "The conversation was compacted here")]).0.as_deref(), Some("Fix it"));
        // Serialized as the help says.
        assert_eq!(serde_json::to_value(PromptFrom::Agent).unwrap(), "agent");
    }

    #[test]
    fn since_is_a_cursor_or_a_duration() {
        assert_eq!(parse_since("1791917560237", NOW), Some(Since::Cursor("1791917560237".into())));
        assert_eq!(parse_since("90m", NOW), Some(Since::Time(NOW - 90 * MIN)));
        assert_eq!(parse_since("yesterday", NOW), None);
    }

    #[test]
    fn looks_are_kept_under_cursors_of_their_own_and_let_go_after_a_week() {
        let mut looks = BTreeMap::new();
        let old = file(&mut looks, Look { at: NOW - 8 * DAY_MS, ..Look::default() });
        let a = file(&mut looks, Look { at: NOW, ..Look::default() });
        let b = file(&mut looks, Look { at: NOW, ..Look::default() });
        assert_ne!(a, b);
        assert!(!looks.contains_key(&old));
        for i in 0..KEEP_LOOKS + 3 {
            file(&mut looks, Look { at: NOW + 1_000 + i as i64, ..Look::default() });
        }
        assert_eq!(looks.len(), KEEP_LOOKS);
        assert!(!looks.contains_key(&a));
    }
}
