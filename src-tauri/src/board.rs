//! The rules from *The model*, as pure functions over already-read data, each with its tests
//! beside it. They're in parts by what they're about, a file each under board/, and each part's
//! rules are named from here, so the harnesses' readers and the engine call `board::rule`
//! whichever file it's in:
//!
//! - `board/sessions.rs`: a session's status from what its harness recorded, a status ladder
//!   and a builder for each harness (`claude_session`, `codex_session`…).
//! - `board/overlays.rs`: what Codealong lays over that status afterwards (`apply_*`): Jev's
//!   read of a reply, quiet, helpers and headless runs, side conversations, a mark of unread,
//!   a snooze, a move to the top.
//! - `board/standing.rs`: where a session's work stands, and whether it's finished.
//! - `board/prs.rs`: PRs with their worktrees and sessions, and whose move a PR is.
//! - `board/worktrees.rs`: whether a worktree is reusable, and whether it can go.
//! - `board/headlines.rs`: a lane's bay and its headline, a session's headline, and where a
//!   lane opens.
//! - This file: what a board is made from (`Inputs`, `LaneSeed`) and `compose`, which makes it;
//!   the lane a session is listed in and the order of sessions and of lanes; the rules over the
//!   whole board (its orchestrator, the sessions a pin takes, the logo, where a new terminal
//!   goes); and what several parts share.
//! - `board/fixtures.rs`: what the tests of several of these start from (test-only).
//!
//! A new rule goes in the part of its kind, with its test in that file's `tests`, and needs no
//! line here: a part starts with `use super::*`, so the imports below serve them all, and what
//! it makes `pub` is named from here by the `pub use` lines. A rule only another part needs is
//! `pub(super)`. src/lanes.ts mirrors `bay_of`, the headlines and `sort_sessions`, and
//! `rule_cases`, in this file's tests, composes the cases both are held to, checked in as
//! src/testing/ruleCases.json: after a change to one of those, write the cases again first
//! (`CODEALONG_WRITE_FIXTURES=1 cargo test rule_cases`, from src-tauri/), then `npm test rules`.

mod headlines;
mod overlays;
mod prs;
mod sessions;
mod standing;
mod worktrees;
#[cfg(test)]
mod fixtures;

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::antigravity::{Conversation, FailedOn, Process};
use crate::claude::{RegistryEntry, Shell, TranscriptInfo};
use crate::codex::ThreadInfo;
use crate::opencode::{Live, SessionRow};
use crate::config::AccountCfg;
use crate::github::PrRaw;
use crate::model::*;
use crate::state::{Raise, Snooze, UnreadMark};
use crate::util::{code, one_line, tilde, truncate, DAY_MS};

pub use headlines::*;
pub use overlays::*;
pub use prs::*;
pub use sessions::*;
pub use standing::*;
pub use worktrees::*;
// What `crate::model` has a name for too, meaning something else. The rule's hid that one
// while this was one file; named on a line of its own it still does, here and in every part
// (a name that a `*` brings in gives way to one that's named).
pub use overlays::ASKS_YOU;

const MAX_SESSIONS_PER_LANE: usize = 30;
/// How a session from another lane is labeled in the worktree it made. One that only worked
/// there is labeled "worked here".
const MADE_IT: &str = "made this worktree";

#[derive(Debug, Clone)]
pub struct LaneSeed {
    pub path: PathBuf,
    pub project: String,
    pub repo: String,
    pub repo_slug: Option<String>,
    /// GitHub owners a PR head may come from (the repo and its forks).
    pub head_owners: Vec<String>,
    pub callsign: String,
    pub origin: Origin,
    pub branch: Option<String>,
    pub head: Option<String>,
    /// The default branch name without the remote, e.g. `main`.
    pub default_branch: Option<String>,
    /// Lanes for sessions outside every project; hidden while they hold no session you haven't
    /// archived.
    pub elsewhere: bool,
    /// The folder an app keeps its chats in, a folder for each (engine/reposcan.rs `chat_roots`):
    /// listed like your projects' worktrees, since you started those sessions yourself.
    pub chats: bool,
    /// Codealong's own chats, where a session that belongs to no project starts
    /// (`Lane::starts_chats`).
    pub starts_chats: bool,
    /// Only sessions started in this very folder are listed in it, not ones in folders inside
    /// it, and work there doesn't count: a plain folder outside your projects can be your home
    /// folder, which would take in everything.
    pub exact: bool,
}

pub struct Inputs<'a> {
    pub now: i64,
    pub apps: &'a AppNames,
    pub seeds: &'a [LaneSeed],
    pub sessions: Vec<Session>,
    /// Lane path → keys of sessions from other lanes whose logs show they worked there.
    pub visitors: &'a HashMap<PathBuf, Vec<String>>,
    pub git: &'a HashMap<PathBuf, GitInfo>,
    pub prs: &'a HashMap<String, Vec<PrRaw>>,
    /// Open PRs that GitHub's search says wait on your review, by the slug a project's repo has
    /// here, for the repos whose own PRs aren't read (`pr_items`).
    pub requested: &'a HashMap<String, Vec<PrRaw>>,
    /// The PRs sessions made or changed in repos whose PRs aren't read, by the slug they were
    /// asked for by, read one by one (ghpoll.rs `made`, `pr_items`).
    pub made: &'a HashMap<String, Vec<PrRaw>>,
    pub project_order: &'a [String],
    pub notices: Vec<crate::model::Notice>,
    pub loading: bool,
    pub github_at: Option<i64>,
    /// Pinned session keys, in order; they're never cut from a lane's list.
    pub pinned: &'a [String],
    /// Who `gh` is signed in as, to tell your PRs from others'.
    pub github_login: Option<&'a str>,
    /// Sessions tied to PRs other than by branch.
    pub pr_links: &'a PrLinks,
    /// Worktrees' app URLs and the servers in their folders.
    pub previews: &'a Previews,
    /// Worktrees marked reusable or not by hand, by path (reusable.rs).
    pub reusable: &'a HashMap<String, bool>,
    /// Reviews you snoozed, by PR key (`apply_review_snoozes`).
    pub review_snoozes: &'a [Snooze],
    /// Where Codealong's own reviews stand (reviews.json), and whether `[reviews] auto` is on
    /// (`autoreview::apply`).
    pub auto_reviews: (&'a crate::autoreview::Store, &'a crate::config::ReviewsCfg, &'a crate::teamfile::RepoSettings),
    pub session_settings: &'a crate::config::SessionsCfg,
    /// Your other machines that share automations, as their feeds said (shared.rs): which of them
    /// reviews each PR that asks for yours (`autoreview::reviewer_of`).
    pub machines: &'a crate::shared::Machines,
    /// The worktrees whose HEAD, as git last read it, GitHub says is the last commit of a PR that
    /// merged (ghpoll.rs `Heads`, engine/gitruns.rs `heads_now`): they lose nothing to their commits on no
    /// remote (`cleanup`).
    pub at_merged: &'a HashSet<PathBuf>,
    /// The PRs (lowercase `owner/name#number`) that a session on another online Mac is actively
    /// working on (`working_elsewhere`), which stay out of `waits_to_merge`.
    pub working_elsewhere: &'a BTreeSet<String>,
}

/// Worktrees' app URLs set by hand, and the servers listening in their folders (previews.rs).
#[derive(Default, Debug)]
pub struct Previews {
    pub set: Vec<crate::previews::Entry>,
    pub servers: Vec<crate::previews::Server>,
    /// The Chrome profile each project's previews open in, by project name (config.toml).
    pub chrome_profiles: HashMap<String, String>,
}

/// Sessions tied to PRs other than by branch (prlinks.rs), by PR key (`owner/name#number`, in
/// lowercase).
#[derive(Default, Debug)]
pub struct PrLinks {
    /// Sessions whose records show they made or changed the PR, and ones linked by hand.
    pub worked: HashMap<String, Vec<String>>,
    /// (PR, session) pairs unlinked by hand: never listed, however they matched.
    pub unlinked: HashSet<(String, String)>,
    /// The sessions whose records show they made or changed the PR, each with the time of its
    /// first mark: the first made it.
    pub made: HashMap<String, Vec<(String, i64)>>,
    /// The branches each session's records show it pushed to, by session key, each push with its
    /// time (prlinks.rs `Pushes`).
    pub pushed: HashMap<String, Vec<(String, i64)>>,
    /// The session picked by hand as the PR's primary.
    pub picked: HashMap<String, String>,
    /// When each PR was last linked to a session in pr-links.json, by hand or by its automatic
    /// review (autoreview.rs `link`): where only so many are read (`worked_prs`), a link counts as
    /// a mark made then, whatever the session's records mark.
    pub linked_at: HashMap<String, i64>,
    /// PRs whose primary session is told of what happens on them (nudges.rs), turned on for
    /// each, and the ones turned off.
    pub nudged: HashSet<String>,
    pub not_nudged: HashSet<String>,
    /// The PRs whose nudge tells of everything (`PrItem::watched`).
    pub watched: HashSet<String>,
    /// `[pull_requests] nudge_by_default`: every open PR of yours is told, unless turned off.
    pub nudge_by_default: bool,
    /// The same for particular repos, by slug, where your `[repos."github.com/…"]` table says.
    pub nudge_by_default_in: HashMap<String, bool>,
    /// The comments Codealong posted your agent's reviews as from this Mac, by link (reviews.json):
    /// they say you were on a PR, but don't answer its author for you (`turn_of`). Only this Mac
    /// knows them: another takes them for your words, since telling them by their hidden mark would
    /// mean reading every comment's text on every PR the board reads.
    pub agent_posts: HashSet<String>,
}

fn plain(s: &str) -> String {
    s.replace('`', "'")
}

/// How a session's excerpt starts while it holds another session's message for you to allow
/// (`claude::held`): the sender follows.
pub use crate::model::HELD_MESSAGE;

/// Index of the seed whose path is the longest prefix of `cwd`; an exact seed only for its own
/// folder. What `SeedIndex` finds, said plainly, to check it against.
#[cfg(test)]
pub fn best_seed(seeds: &[LaneSeed], cwd: &Path) -> Option<usize> {
    seeds
        .iter()
        .enumerate()
        .filter(|(_, s)| if s.exact { cwd == s.path } else { cwd.starts_with(&s.path) })
        .max_by_key(|(_, s)| s.path.components().count())
        .map(|(i, _)| i)
}

/// The seed a path is in: the one whose folder is the longest prefix of it, an exact seed only
/// for its own folder (`best_seed`). The seeds are kept by folder, so a path's seed is found by
/// walking up its folders rather than by comparing it with every seed: with a couple of hundred
/// lanes and twice as many sessions, comparing took most of each board's composing (2026-10-12).
/// A path hashes and compares by its components, so a doubled or trailing slash finds the seed
/// `starts_with` does.
pub struct SeedIndex {
    /// The last seed for each folder, as `best_seed` takes the last of seeds as deep: one that
    /// takes in the folders inside it, and an exact one.
    under: HashMap<PathBuf, usize>,
    exact: HashMap<PathBuf, usize>,
    /// The seeds' folders' names, and whether a seed's folder has none (`/`, or one ending in
    /// `..`), so most paths are known not to be a seed's folder without hashing them whole.
    names: HashSet<std::ffi::OsString>,
    nameless: bool,
    /// How long the folders of the seeds that take in folders are, written plainly
    /// (`Path::components`), so only folders that long are looked up.
    lengths: HashSet<usize>,
}

impl SeedIndex {
    pub fn new(seeds: &[LaneSeed]) -> SeedIndex {
        let (mut under, mut exact, mut names, mut nameless, mut lengths) = (HashMap::new(), HashMap::new(), HashSet::new(), false, HashSet::new());
        for (i, s) in seeds.iter().enumerate() {
            if s.exact {
                exact.insert(s.path.clone(), i);
            } else {
                under.insert(s.path.clone(), i);
                lengths.insert(s.path.components().collect::<PathBuf>().as_os_str().len());
            }
            match s.path.file_name() {
                Some(name) => _ = names.insert(name.to_os_string()),
                None => nameless = true,
            }
        }
        SeedIndex { under, exact, names, nameless, lengths }
    }

    /// What `best_seed` says for `cwd`.
    pub fn best(&self, cwd: &Path) -> Option<usize> {
        self.own(cwd).or_else(|| match cwd.parent() {
            Some(dir) => self.holding(dir),
            None => self.holding(Path::new("")),
        })
    }

    /// The seed whose folder is `cwd` itself.
    pub fn own(&self, cwd: &Path) -> Option<usize> {
        if !self.nameless && !cwd.file_name().is_some_and(|n| self.names.contains(n)) {
            return None;
        }
        self.exact.get(cwd).max(self.under.get(cwd)).copied()
    }

    /// The deepest seed that takes in `dir` and the folders inside it: what `best_seed` says for
    /// any path in `dir` that isn't a seed's own folder.
    pub fn holding(&self, dir: &Path) -> Option<usize> {
        // Written plainly, a folder above it is the same folder as a seed's only if it's as
        // long, and its length is cheaper to look up than the whole path.
        let dir: PathBuf = dir.components().collect();
        dir.ancestors()
            .filter(|a| self.lengths.contains(&a.as_os_str().len()))
            .find_map(|a| self.under.get(a))
            // An empty path is a prefix of every path, though no folder's parent.
            .or_else(|| self.under.get(Path::new("")))
            .copied()
    }
}

/// How many of a session's latest commands and edits in worktrees decide which one it's in.
const WORK_WINDOW: usize = 5;

/// The lane a session is working in. Agents often start in one worktree and then work in
/// another (`cd ../repo-fix && …`), so it's where most of the last few commands ran and edits
/// landed, the latest winning a tie, and a stray command elsewhere doesn't move it. Going
/// somewhere only to look isn't work there (`util::command_dir`): an agent reads other repos
/// from where it is. Work outside every worktree doesn't count, nor work in a folder that's a
/// lane only because a session started there: a plain folder, or an app's chats, where a chat
/// that was later given a folder or went to work in a repo still runs its shell. With none,
/// where the session was started.
pub fn session_seed(seeds: &[LaneSeed], index: &SeedIndex, s: &Session) -> Option<usize> {
    let recent: Vec<usize> = s.work.iter().rev().filter_map(|p| index.best(Path::new(p)).filter(|&i| !seeds[i].exact && !seeds[i].chats)).take(WORK_WINDOW).collect();
    let votes = |i: usize| recent.iter().filter(|&&j| j == i).count();
    // Oldest first: of the lanes tied for most, `max_by_key` keeps the last, the latest.
    recent.iter().rev().copied().max_by_key(|&i| votes(i)).or_else(|| index.best(Path::new(&s.cwd)))
}

/// How urgent a status is. Idle and stopped rank the same: a Codex thread is never stopped, so
/// telling them apart would only sort the harnesses apart.
fn urgency(s: Status) -> u8 {
    match s {
        Status::Needs => 0,
        Status::Error => 1,
        Status::Working => 2,
        Status::Unread => 3,
        Status::Idle | Status::Stopped => 4,
    }
}

/// Where a session is on the board: the lane it's listed under as its own, else one it only
/// visits (a lane lists a visitor when it worked there). None once it's gone.
pub fn home<'a>(lanes: &'a [Lane], key: &str) -> Option<(&'a Lane, &'a Session)> {
    let find = |own: bool| lanes.iter().find_map(|l| l.sessions.iter().find(|s| s.key == key && (!own || s.visitor.is_none())).map(|s| (l, s)));
    find(true).or_else(|| find(false))
}

/// The prompt a session is started on to run as your orchestrator (the plugin's
/// /codealong:orchestrate; its scheduled ticks say `tick` after it, in the same session).
const ORCHESTRATES: &str = "/codealong:orchestrate";

/// The session that runs as your orchestrator, for talking to it (dictation.rs): an
/// orchestrator's own (`Session::orchestrator`), All projects' before a project's, else one of
/// this Mac's that was started on the orchestrator's prompt, or failing that is called an
/// orchestrator, and isn't archived or some session's helper. A running one before a stopped one,
/// then the latest to do anything.
pub fn orchestrator(lanes: &[Lane]) -> Option<&Session> {
    let started_as = started_as_orchestrator;
    let called = |s: &Session| s.name.as_deref().unwrap_or(&s.title).to_lowercase().contains("orchestrator");
    let own = |s: &Session| s.orchestrator.as_ref().map(|o| if o.project.is_none() { 2 } else { 1 }).unwrap_or(0);
    lanes
        .iter()
        .flat_map(|l| &l.sessions)
        .filter(|s| !s.archived && s.helper.is_none() && s.visitor.is_none() && (own(s) > 0 || started_as(s) || called(s)))
        .max_by_key(|s| (own(s), s.live, started_as(s), s.last_activity))
}

/// Whether a session was started on the orchestrator's prompt, whether or not a schedule marks it
/// as one now (an earlier thread of one).
pub fn started_as_orchestrator(s: &Session) -> bool {
    s.first_prompt.as_deref().is_some_and(|p| p.trim_start().starts_with(ORCHESTRATES))
}

/// When a session began waiting on you, for one that needs you: the list of those is a queue,
/// the longest wait first, so a new one can't keep pushing an old one down.
pub fn waiting_since(s: &Session) -> Option<i64> {
    matches!(s.status, Status::Needs | Status::Error).then(|| s.status_since.unwrap_or(s.last_activity))
}

/// Most urgent first, the ones moved to the top first among their kind (`raised_order`); those
/// that need you by how long they've waited, the rest by the one you last sent a prompt, with
/// the finished turns that are only yours to read after the others (`Session::waits_finished`).
/// Activity would reorder working sessions with every tool call; your prompts only move one when
/// you do something. The key settles a tie: the readers hand their sessions over in their maps'
/// order, which differs every time, and a lane whose order moved would make every board a new
/// one to publish.
fn sort_sessions(sessions: &mut [Session]) {
    sessions.sort_by(|a, b| {
        urgency(a.status)
            .cmp(&urgency(b.status))
            .then(raised_order(a.raised.as_ref(), b.raised.as_ref()))
            .then(a.waits_finished().cmp(&b.waits_finished()))
            .then(waiting_since(a).cmp(&waiting_since(b)))
            .then(b.last_prompt_at.cmp(&a.last_prompt_at))
            .then(b.last_activity.cmp(&a.last_activity))
            .then(a.key.cmp(&b.key))
    });
}

/// When you last sent a prompt to one of a lane's own sessions; for a lane with none, when
/// anything last happened in it.
fn lane_prompt_at(sessions: &[Session], last_activity: i64) -> i64 {
    // A helper's prompts are its agent's, not yours, and a headless run's its script's.
    sessions.iter().filter(|s| s.visitor.is_none() && !s.archived && !s.unattended()).map(|s| s.last_prompt_at).max().unwrap_or(last_activity)
}

/// How long a lane in Needs you has waited: since the longest wait of its own agents that need
/// you and aren't snoozed or set aside. None when it needs you only through agents you've snoozed
/// or set aside, which come after the ones waiting.
fn lane_waiting_since(sessions: &[Session]) -> Option<i64> {
    sessions.iter().filter(|s| s.visitor.is_none() && !s.archived && !s.held_back()).filter_map(waiting_since).min()
}

/// The first of a lane's own agents moved to the top (`Session::raised`), in the queues' order.
fn lane_raised(sessions: &[Session]) -> Option<&Raised> {
    sessions.iter().filter(|s| s.visitor.is_none() && !s.archived).filter_map(|s| s.raised.as_ref()).min_by(|a, b| raised_order(Some(a), Some(b)))
}

/// Lanes by bay, those with an agent moved to the top first (`raised_order`); in Needs you the
/// longest wait first, then those with no agent waiting; within that, and in the other bays, the
/// one you last sent a prompt first.
fn lane_order(a: &Lane, b: &Lane) -> std::cmp::Ordering {
    let waited = |l: &Lane| if l.bay == Bay::Needs { lane_waiting_since(&l.sessions).map_or((1, 0), |t| (0, t)) } else { (0, 0) };
    a.bay
        .cmp(&b.bay)
        .then(raised_order(lane_raised(&a.sessions), lane_raised(&b.sessions)))
        .then(waited(a).cmp(&waited(b)))
        .then(lane_prompt_at(&b.sessions, b.last_activity).cmp(&lane_prompt_at(&a.sessions, a.last_activity)))
        .then(b.last_activity.cmp(&a.last_activity))
}

/// The sessions a pin asked for from another device takes (`session-pin`): for a pin, only ones
/// the board lists and hasn't archived, since a list shown a while ago could name one archived
/// since, whose pin archiving took off, or none at all; for an unpin, any.
pub fn pinnable<'a>(lanes: &[Lane], keys: &'a [String], pinned: bool) -> Vec<&'a String> {
    if !pinned {
        return keys.iter().collect();
    }
    let listed: std::collections::HashSet<&str> = lanes.iter().flat_map(|l| l.sessions.iter()).filter(|s| !s.archived).map(|s| s.key.as_str()).collect();
    keys.iter().filter(|k| listed.contains(k.as_str())).collect()
}

/// A notice for each account here whose usage reads signed out while sessions of it here are
/// stopped on its sign-in being turned down (`Session::signed_out`), offering to sign it in: one
/// sign-in mends them all. An account that reads signed in gets none: its sessions are told to
/// carry on (reconnect.rs), and a sign-in renewed by another process moments later is the usual
/// case (claude/transcript.rs `signed_out_of`).
pub fn signed_out_notices(board: &Board) -> Vec<Notice> {
    board
        .usage
        .iter()
        .filter(|u| u.needs_sign_in)
        .filter_map(|u| {
            let mut stopped: Vec<&Session> = board
                .lanes
                .iter()
                .filter(|l| l.device.is_none())
                .flat_map(|l| &l.sessions)
                .filter(|s| s.account == u.account && s.visitor.is_none() && !s.archived && s.status != Status::Working && s.signed_out.is_some())
                .collect();
            stopped.sort_by(|a, b| a.key.cmp(&b.key));
            stopped.dedup_by(|a, b| a.key == b.key);
            let (them, it) = match stopped.as_slice() {
                [] => return None,
                [one] => (crate::send::quoted(&one.title), "it"),
                many => (format!("{} sessions", many.len()), "them"),
            };
            // Those Codealong won't carry on are named, so you send only them something.
            let left: Vec<String> = stopped.iter().filter(|s| !s.signed_out.as_ref().is_some_and(|o| o.continues)).map(|s| crate::send::quoted(&s.title)).collect();
            let then = if left.is_empty() {
                format!("Sign in, and Codealong tells {it} to carry on.")
            } else if left.len() == stopped.len() {
                format!("Sign in, then send {it} something to go on.")
            } else {
                format!("Sign in, and Codealong tells them to carry on, but for {}, which you send something to go on.", crate::util::listed(&left, "and"))
            };
            let account = &u.account;
            Some(Notice { key: format!("signed-out:{account}"), title: format!("Signed out of {account}"), text: format!("{them} stopped because {account}'s sign-in was turned down. {then}"), sign_in: Some(account.clone()), ..Default::default() })
        })
        .collect()
}

pub fn compose(inp: Inputs) -> Board {
    let Inputs { now, apps, seeds, mut sessions, visitors, git, prs, requested, made, project_order, notices, loading, github_at, pinned, github_login, pr_links, previews, reusable: marks, review_snoozes, auto_reviews, session_settings, machines, at_merged, working_elsewhere } = inp;
    for s in &mut sessions {
        let (h, d) = session_headline(s);
        s.headline = h;
        s.detail = d;
    }
    let mut by_lane: Vec<Vec<Session>> = vec![Vec::new(); seeds.len()];
    // A worktree made inside a session's own scratch folder (Claude Code's scratchpad) also shows
    // the session that made it, even though that session runs in another lane.
    let owners: HashMap<(Harness, String), Session> = sessions.iter().filter(|s| !s.archived).map(|s| ((s.harness, s.id.clone()), s.clone())).collect();
    let by_key: HashMap<String, Session> = sessions.iter().map(|s| (s.key.clone(), s.clone())).collect();
    let index = SeedIndex::new(seeds);
    // The processes of the sessions running now, whose locks on their worktrees hold.
    let live_pids: HashSet<u32> = sessions.iter().filter(|s| s.live).filter_map(|s| s.pid).collect();
    for mut s in sessions {
        if let Some(i) = session_seed(seeds, &index, &s) {
            // A session listed in another checkout than its folder's (an agent that went into a
            // worktree with `cd`) records its folder's branch, not the one it works on, so it goes
            // by its lane's, as whatever reads a session's branch does when there's none.
            if index.best(Path::new(&s.cwd)) != Some(i) {
                s.folder_branch = s.branch.take();
            }
            by_lane[i].push(s);
        }
    }
    // Archived sessions don't visit other lanes.
    let by_key: HashMap<String, Session> = by_key.into_iter().filter(|(_, s)| !s.archived).collect();
    // Sessions from other lanes shown here: the one that made a scratchpad worktree, and any
    // whose logs show they worked in it.
    let guests: Vec<Vec<Session>> = seeds
        .iter()
        .enumerate()
        .map(|(i, seed)| {
            let mut out: Vec<Session> = Vec::new();
            if let Some(owner) = crate::harness::scratch_owner(&seed.path).and_then(|owner| owners.get(&owner)) {
                out.push(Session { visitor: Some(MADE_IT.into()), ..owner.clone() });
            }
            for key in visitors.get(&seed.path).map(|v| v.as_slice()).unwrap_or(&[]) {
                if let Some(s) = by_key.get(key) {
                    if !out.iter().any(|o| &o.key == key) {
                        out.push(Session { visitor: Some("worked here".into()), ..s.clone() });
                    }
                }
            }
            out.retain(|g| !by_lane[i].iter().any(|s| s.key == g.key));
            out
        })
        .collect();
    let mut lanes = Vec::with_capacity(seeds.len());
    for ((seed, sessions), guests) in seeds.iter().zip(by_lane).zip(guests) {
        let g = git.get(&seed.path).cloned();
        let reuse_on = reusable(seed, marks);
        // Counted before the list is cut, archived ones too: archiving doesn't end a session.
        let running = sessions.iter().filter(|s| s.live).count();
        // Archived sessions stay in the lane's list, last, and out of its status. In a reusable
        // worktree so do the ones from before its latest work, ahead of the archived ones; and
        // helpers and headless runs that don't need you, ahead of those (`Session::unattended`).
        let (mut sessions, mut archived): (Vec<Session>, Vec<Session>) = sessions.into_iter().partition(|s| !s.archived);
        sort_sessions(&mut sessions);
        sort_sessions(&mut archived);
        let reset = g.as_ref().and_then(|g| g.reset_at).filter(|_| reuse_on);
        for s in &mut sessions {
            s.earlier = earlier(s, reset);
        }
        // Stable: each part keeps its order.
        sessions.sort_by_key(|s| (s.earlier, s.unattended()));
        let mut kept = 0;
        // A running session is never cut: it holds its worktree (`cleanup`), and Codealong's
        // removal looks for one on its lane's list.
        sessions.retain(|s| {
            let keep = kept < MAX_SESSIONS_PER_LANE || pinned.contains(&s.key) || s.live;
            kept += keep as usize;
            keep
        });
        let current = sessions.iter().take_while(|s| !s.earlier && !s.unattended()).count();
        let pr = find_pr(seed, prs, now);
        // The bay and headline come from the lane's own current sessions; the maker is shown
        // after them.
        let (bay, glyph) = bay_of(&sessions[..current], pr.as_ref());
        let (mut headline, mut detail) = headline(bay, &sessions[..current], pr.as_ref(), g.as_ref());
        let reuse = reuse_on.then(|| reuse_of(pr.as_ref(), g.as_ref(), &sessions[..current]));
        if let (Bay::Idle, 0, Some(g)) = (bay, current, guests.first()) {
            let verb = if g.visitor.as_deref() == Some(MADE_IT) { "Made by" } else { "Worked on by" };
            headline = format!("{verb} {} · {}", code(&g.account), g.title);
            detail = Some(format!("last turn {{ago:{}}}", g.last_activity));
        }
        // A lane outside your projects shows, in All projects too, while it holds a session you
        // haven't archived, as an app's chats do: Elsewhere is a project like the others. Only one
        // with nothing to list (Codealong's chats before one starts) waits out of sight.
        let hidden = seed.elsewhere && bay != Bay::Needs && sessions.is_empty();
        // Visitors after the current work's sessions, and the helpers and the earlier work's
        // after them.
        let earlier_ones = sessions.split_off(current);
        sessions.extend(guests);
        sessions.extend(earlier_ones);
        let set = previews.set.iter().find(|e| Path::new(&e.worktree) == seed.path);
        let servers: Vec<&crate::previews::Server> = previews.servers.iter().filter(|s| s.lane == seed.path).collect();
        let preview = lane_preview(set, &servers, &sessions, previews.chrome_profiles.get(&seed.project).map(String::as_str));
        let (targets, default_target) = targets(seed, &sessions, pr.as_ref(), preview.as_ref(), apps);
        let last_activity = sessions
            .iter()
            .map(|s| s.last_activity)
            .chain(g.as_ref().and_then(|g| g.last_commit_at))
            .max()
            .unwrap_or(0);
        sessions.extend(archived);
        let dirty = g.as_ref().map(|g| g.dirty > 0).unwrap_or(false);
        let mark = marks.get(seed.path.to_string_lossy().as_ref()).copied();
        // One you marked reusable is kept to take the next work however long it has been quiet;
        // a main checkout, reusable unless marked otherwise, parks as others do.
        let parked = bay == Bay::Idle && !dirty && now - last_activity > 14 * DAY_MS && !(reuse_on && mark == Some(true));
        let looks_finished = !reuse_on && looks_finished(seed.origin, pr.as_ref(), &sessions);
        let looks_reusable = looks_reusable(seed, mark.is_some(), prs);
        let cleanup = cleanup(&Leaving {
            origin: seed.origin,
            chats: seed.chats || seed.starts_chats,
            reusable: reuse_on,
            git: g.as_ref(),
            running,
            // Looked for only where it would be said: a main checkout holds the others.
            holds: (seed.origin != Origin::Main && !reuse_on).then(|| seeds.iter().find(|o| o.path != seed.path && o.path.starts_with(&seed.path))).flatten().map(|o| o.callsign.as_str()),
            lock: g.as_ref().and_then(|g| g.locked.as_deref()).filter(|reason| lock_holds(reason, &live_pids)),
            merged: at_merged.contains(&seed.path),
        });
        lanes.push(Lane {
            repo_slug: seed.repo_slug.clone(),
            callsign: seed.callsign.clone(),
            origin: seed.origin,
            branch: seed.branch.clone(),
            detached: if seed.branch.is_none() { seed.head.as_ref().map(|h| h.chars().take(7).collect()) } else { None },
            bay,
            glyph,
            parked,
            hidden,
            headline,
            detail,
            sessions,
            pr,
            git: g,
            looks_finished,
            last_activity,
            targets,
            default_target,
            preview,
            reuse,
            looks_reusable,
            starts_chats: seed.starts_chats,
            chats: seed.chats || seed.starts_chats,
            cleanup,
            ..Lane::new(&seed.path.to_string_lossy(), &seed.project, &seed.repo)
        });
    }
    lanes.sort_by(lane_order);
    let mut pr_list = pr_items(seeds, &lanes, prs, requested, made, github_login, pr_links);
    crate::autoreview::asked_again(&mut pr_list, auto_reviews.0, read_now(seeds, made), machines);
    merge_waits(&mut pr_list, &lanes, working_elsewhere);
    apply_review_snoozes(&mut pr_list, review_snoozes, now);
    crate::autoreview::apply(&mut pr_list, auto_reviews.0, auto_reviews.1, session_settings, auto_reviews.2, &lanes, machines, now);
    // Once the PRs say whose they are and which session opened each.
    crate::autoreview::nest_own(&mut lanes, &pr_list);

    let mut projects = Vec::new();
    let mut names: Vec<String> = project_order.to_vec();
    // Codealong's chats are always there to start in, and count toward no project until a
    // session is.
    let counted = |l: &&Lane| !(l.starts_chats && l.sessions.is_empty());
    if lanes.iter().filter(counted).any(|l| l.project == ELSEWHERE) && !names.iter().any(|n| n == ELSEWHERE) {
        names.push(ELSEWHERE.to_string());
    }
    for name in names {
        let mine: Vec<&Lane> = lanes.iter().filter(counted).filter(|l| l.project == name).collect();
        if mine.is_empty() {
            continue;
        }
        let mut repos: BTreeMap<String, (usize, bool)> = BTreeMap::new();
        for l in &mine {
            let e = repos.entry(l.repo.clone()).or_insert((0, l.origin == Origin::Folder));
            e.0 += 1;
        }
        projects.push(ProjectSummary {
            name,
            lanes: mine.len(),
            repos: repos.into_iter().map(|(name, (lanes, folder))| RepoSummary { name, lanes, folder }).collect(),
            icon: None,
        });
    }
    Board {
        generated_at: now,
        // The engine numbers it once it's published.
        rev: 0,
        logo_status: logo_status(&lanes, &pr_list),
        apps: apps.clone(),
        projects,
        lanes,
        notices,
        loading,
        github_at,
        github_polled: false,
        github_current: Default::default(),
        usage: Vec::new(),
        training: Vec::new(),
        resets: Vec::new(),
        sign_ins: Vec::new(),
        mcp: Vec::new(),
        sides: Default::default(),
        starting: Default::default(),
        terminals: Default::default(),
        listenings: Default::default(),
        meeting: None,
        meeting_soon: None,
        pinned: pinned.to_vec(),
        pinned_worktrees: Vec::new(),
        pinned_prs: Vec::new(),
        pinned_repos: Vec::new(),
        prs: pr_list,
        github_login: github_login.map(str::to_string),
        load: None,
        theme: Default::default(),
        zoom: crate::zoom::ACTUAL,
        glass: false,
        show_keys: false,
        tools: crate::config::ToolDisplay::default(),
        perf_log: false,
        todos: Vec::new(),
        todos_unreadable: false,
        today: String::new(),
        todo_queue: Default::default(),
        todo_clock: None,
        queue: Default::default(),
        labels: Vec::new(),
        quick_shortcut: String::new(),
        chat_shortcut: String::new(),
        waiting_shortcut: String::new(),
        improve_shortcut: String::new(),
        main_window_shortcut: String::new(),
        talk_shortcut: String::new(),
        talk_orchestrator_shortcut: String::new(),
        listen_shortcut: String::new(),
        improve: Default::default(),
        decisions: Default::default(),
        experimental: Default::default(),
        reviews: Default::default(),
        todo_hints: Default::default(),
        devices: Default::default(),
        project_rules: Vec::new(),
        sessions: session_settings.clone(),
        when_limited: Default::default(),
        nudge_about: Default::default(),
        pr_stopped: Default::default(),
        repos: Default::default(),
        team_files: Vec::new(),
        repo_settings: Default::default(),
        schedules: Vec::new(),
        schedules_problem: None,
        machines: Default::default(),
        shared_schedules: Default::default(),
        merged_here: Default::default(),
    }
}

pub use codealong_peer::model::ELSEWHERE;

/// Match the floating bar's scope: all visible projects, without archived sessions, duplicate
/// visitors, or helpers and headless runs that don't need you. An open review request or ready
/// merge of yours you haven't snoozed also needs the user's attention.
pub fn logo_status<'a>(lanes: impl IntoIterator<Item = &'a Lane>, prs: &[PrItem]) -> LogoStatus {
    logo_status_with(lanes, prs, true, true)
}

/// `logo_status`, counting reviews and ready merges only when `[queue]` has each of them wait on
/// you (engine/publish.rs `compose`).
pub fn logo_status_with<'a>(lanes: impl IntoIterator<Item = &'a Lane>, prs: &[PrItem], reviews: bool, merges: bool) -> LogoStatus {
    let sessions = lanes.into_iter().filter(|l| !l.hidden).flat_map(|l| &l.sessions)
        .filter(|s| !s.archived && s.visitor.is_none() && !s.unattended())
        .map(|s| match s.status {
            Status::Working => LogoStatus::Working,
            // Snoozed, it's off the bar's Needs you and Your turn, so off the mark too. Set aside,
            // its side conversation is at work for it, as the bar lists it.
            _ if s.snoozed_until.is_some() => LogoStatus::Idle,
            _ if s.aside.is_some() => LogoStatus::Working,
            Status::Error => LogoStatus::Error,
            Status::Needs => LogoStatus::Needs,
            Status::Unread => LogoStatus::Unread,
            Status::Idle | Status::Stopped => LogoStatus::Idle,
        });
    let pr = prs.iter().any(|p| (reviews && p.waits_on_review()) || (merges && p.waits_on_merge() && !p.waits_on_review()));
    sessions.chain(pr.then_some(LogoStatus::Needs)).min().unwrap_or_default()
}

/// The folder a session works in, for a new terminal there: its own when that's its worktree's
/// or inside it (it was started in a subfolder, or it's a chat with a folder of its own in the
/// lane of an app's chats), else the worktree's, which is where it's shown. `runFolder` in
/// RunButton.tsx says the same for the commands run from its conversation.
pub fn session_folder(lane_path: &str, cwd: &str) -> String {
    let inside = !cwd.is_empty() && Path::new(cwd).starts_with(lane_path);
    if inside { cwd } else { lane_path }.to_string()
}

/// Where a new terminal belongs (`terminal_neighbours`), for `[apps] new_terminal = "project"`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Neighbours {
    /// The folders it belongs beside, nearest first: its worktree, the other worktrees of its
    /// repo, then the rest of its project. A tab goes in a window already showing the nearest
    /// of them. A folder elsewhere (a project of its own that isn't one, or an app's chats) has
    /// only itself for company.
    pub folders: Vec<Vec<String>>,
    /// The project its worktree is listed in (Elsewhere for one in none of yours), for a
    /// terminal that keeps each project's tabs together on their own (Rex, in a session named
    /// for it). None for a folder in no worktree.
    pub project: Option<String>,
}

/// A new terminal with nothing to go beside: a window of its own, or the front one.
pub static ALONE: Neighbours = Neighbours { folders: Vec::new(), project: None };

/// Where a new terminal at `path` belongs: beside which folders, and in which project.
pub fn terminal_neighbours(lanes: &[Lane], path: &Path) -> Neighbours {
    let Some(home) = home_lane(lanes, path) else {
        return Neighbours { folders: vec![vec![path.to_string_lossy().into_owned()]], project: None };
    };
    let mut near = Neighbours { folders: vec![vec![home.path.clone()]], project: Some(home.project.clone()) };
    if home.origin == Origin::Other || home.project == ELSEWHERE {
        return near;
    }
    let others = |same: &dyn Fn(&Lane) -> bool| -> Vec<String> {
        lanes.iter().filter(|l| l.path != home.path && l.origin != Origin::Other && same(l)).map(|l| l.path.clone()).collect()
    };
    let repo = others(&|l| l.project == home.project && l.repo == home.repo);
    let project = others(&|l| l.project == home.project && l.repo != home.repo);
    near.folders.extend([repo, project].into_iter().filter(|g| !g.is_empty()));
    near
}

/// The worktree a folder is in: the deepest lane holding it.
fn home_lane<'a>(lanes: &'a [Lane], path: &Path) -> Option<&'a Lane> {
    lanes.iter().filter(|l| path.starts_with(&l.path)).max_by_key(|l| l.path.len())
}

/// The most of a session's title a tab's name keeps, the worktree's name aside.
const TAB_TITLE: usize = 40;

/// The name of a tab Codealong opens for a session at `path`, where its terminal names tabs: the
/// worktree's name as the board says it, then the session's title, since the title an agent gives
/// its tab says nothing of where it works. None outside every worktree, where the agent's own
/// title is as good. Control characters go, runs of white space become one space, and a long
/// title is cut to fit a tab.
pub fn tab_label(lanes: &[Lane], path: &Path, title: &str) -> Option<String> {
    let words = |s: &str| s.split(|c: char| c.is_whitespace() || c.is_control()).filter(|w| !w.is_empty()).collect::<Vec<_>>().join(" ");
    let name = words(&home_lane(lanes, path)?.callsign);
    if name.is_empty() {
        return None;
    }
    let mut title = words(title);
    if title.chars().count() > TAB_TITLE {
        title = format!("{}…", title.chars().take(TAB_TITLE - 1).collect::<String>().trim_end());
    }
    Some(if title.is_empty() { name } else { format!("{name} · {title}") })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::fixtures::{acct, insight, pr, reg, seed, tr, NOW};

    #[test]
    fn the_orchestrator_is_the_session_started_as_one_and_running_before_stopped() {
        use crate::fixtures::{lane, session};
        let with = |key: &str, f: fn(&mut Session)| {
            let mut s = session(key);
            f(&mut s);
            s
        };
        let found = |sessions: Vec<Session>| orchestrator(&[lane("/chats", sessions)]).map(|s| s.key.clone());
        assert_eq!(found(vec![session("plain")]), None);
        // Started on the plugin's prompt, whatever it's called since.
        let started = with("started", |s| s.first_prompt = Some("/codealong:orchestrate".into()));
        assert_eq!(found(vec![session("plain"), started.clone()]), Some("started".into()));
        // Failing that, one that's called an orchestrator.
        let called = with("called", |s| s.title = "Orchestrator".into());
        assert_eq!(found(vec![session("plain"), called.clone()]), Some("called".into()));
        let named = with("named", |s| s.name = Some("my-orchestrator".into()));
        assert_eq!(found(vec![named]), Some("named".into()));
        // A running one before a stopped one, however lately that did anything; then the one
        // started as one before one only called it.
        let stopped = with("stopped", |s| {
            s.first_prompt = Some("/codealong:orchestrate".into());
            s.live = false;
            s.last_activity = 9_000;
        });
        assert_eq!(found(vec![stopped.clone(), called.clone()]), Some("called".into()));
        assert_eq!(found(vec![called.clone(), started.clone()]), Some("started".into()));
        assert_eq!(found(vec![stopped.clone()]), Some("stopped".into()));
        // Not one put away, a helper, or another worktree's session only visiting.
        let archived = with("archived", |s| {
            s.title = "Orchestrator".into();
            s.archived = true;
        });
        let helper = with("helper", |s| {
            s.title = "Review the orchestrator".into();
            s.helper = Some(crate::model::Helper { parent: Some("boss".into()), pr: None, report: false, yours: false });
        });
        let visitor = with("visitor", |s| {
            s.title = "Orchestrator".into();
            s.visitor = Some("worked here".into());
        });
        assert_eq!(found(vec![archived, helper, visitor]), None);
    }

    #[test]
    fn a_sessions_folder_is_its_own_inside_its_worktree() {
        assert_eq!(session_folder("/p/app", "/p/app"), "/p/app");
        assert_eq!(session_folder("/p/app", "/p/app/src-tauri"), "/p/app/src-tauri");
        // One that visits from another worktree, and one that says nothing of where it started.
        assert_eq!(session_folder("/p/app", "/p/app-fix"), "/p/app");
        assert_eq!(session_folder("/p/app", "/p/other/src"), "/p/app");
        assert_eq!(session_folder("/p/app", ""), "/p/app");
    }

    fn logo_board() -> Board {
        let session = claude_session(&acct("claude2"), Some(&reg("busy", NOW)), Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
        compose(Inputs {
            now: NOW,
            apps: &AppNames::default(),
            seeds: &[seed("/r/spark-portal", "portal")],
            sessions: vec![session],
            visitors: &HashMap::new(),
            git: &HashMap::new(),
            prs: &HashMap::from([("sprucehq/spark".to_string(), vec![pr(PrState::Open, 0)])]),
            requested: &HashMap::new(),
            made: &HashMap::new(),
            project_order: &["Spruce".to_string()],
            notices: vec![],
            loading: false,
            github_at: None,
            pinned: &[],
            github_login: Some("gwen"),
            pr_links: &PrLinks::default(),
            previews: &Previews::default(),
            reusable: &HashMap::new(),
            review_snoozes: &[],
            session_settings: &Default::default(), auto_reviews: (&Default::default(), &Default::default(), &Default::default()), machines: &Default::default(), at_merged: &HashSet::new(), working_elsewhere: &BTreeSet::new(),
        })
    }

    #[test]
    fn logo_prioritizes_attention_and_returns_to_idle_as_sessions_clear() {
        let mut b = logo_board();
        assert_eq!(b.logo_status, LogoStatus::Working, "composed boards carry the shared status");
        assert_eq!(serde_json::to_value(&b).unwrap()["logoStatus"], "working");
        let session = b.lanes[0].sessions[0].clone();
        b.lanes[0].sessions.clear();
        assert_eq!(logo_status(&b.lanes, &b.prs), LogoStatus::Idle);
        for (status, expected) in [
            (Status::Stopped, LogoStatus::Idle),
            (Status::Idle, LogoStatus::Idle),
            (Status::Working, LogoStatus::Working),
            (Status::Unread, LogoStatus::Unread),
            (Status::Needs, LogoStatus::Needs),
            (Status::Error, LogoStatus::Error),
        ] {
            b.lanes[0].sessions.push(Session { status, ..session.clone() });
            assert_eq!(logo_status(&b.lanes, &b.prs), expected);
        }
        for expected in [LogoStatus::Needs, LogoStatus::Unread, LogoStatus::Working, LogoStatus::Idle, LogoStatus::Idle, LogoStatus::Idle] {
            b.lanes[0].sessions.pop();
            assert_eq!(logo_status(&b.lanes, &b.prs), expected);
        }
    }

    #[test]
    fn logo_ignores_archived_visiting_and_hidden_sessions() {
        let mut b = logo_board();
        let mut error = b.lanes[0].sessions[0].clone();
        error.status = Status::Error;
        error.archived = true;
        b.lanes[0].sessions.push(error.clone());
        error.archived = false;
        error.visitor = Some("worked here".into());
        b.lanes[0].sessions.push(error.clone());
        let mut hidden = b.lanes[0].clone();
        hidden.hidden = true;
        error.visitor = None;
        hidden.sessions = vec![error];
        b.lanes.push(hidden);
        assert_eq!(logo_status(&b.lanes, &b.prs), LogoStatus::Working);
        b.lanes[1].hidden = false;
        assert_eq!(logo_status(&b.lanes, &b.prs), LogoStatus::Error, "attention in another visible project counts");
    }

    #[test]
    fn logo_leaves_out_snoozed_sessions_unless_they_work() {
        let mut b = logo_board();
        let s = &mut b.lanes[0].sessions[0];
        s.snoozed_until = Some(NOW + 60_000);
        assert_eq!(logo_status(&b.lanes, &b.prs), LogoStatus::Working);
        b.lanes[0].sessions[0].status = Status::Needs;
        b.prs.clear();
        assert_eq!(logo_status(&b.lanes, &b.prs), LogoStatus::Idle);
        b.lanes[0].sessions[0].snoozed_until = None;
        assert_eq!(logo_status(&b.lanes, &b.prs), LogoStatus::Needs);
        // Set aside for its side conversation, that's at work for it.
        b.lanes[0].sessions[0].aside = Some(NOW - 60_000);
        assert_eq!(logo_status(&b.lanes, &b.prs), LogoStatus::Working);
    }

    #[test]
    fn logo_counts_only_open_review_requests() {
        let mut b = logo_board();
        b.prs[0].review_requested = true;
        assert_eq!(logo_status(&b.lanes, &b.prs), LogoStatus::Needs);
        b.lanes[0].sessions[0].status = Status::Error;
        assert_eq!(logo_status(&b.lanes, &b.prs), LogoStatus::Error);
        b.lanes[0].sessions.clear();
        for state in [PrState::Closed, PrState::Merged] {
            b.prs[0].state = state;
            assert_eq!(logo_status(&b.lanes, &b.prs), LogoStatus::Idle);
        }
    }

    #[test]
    fn a_snoozed_review_is_off_the_logo_until_its_time() {
        let mut b = logo_board();
        b.lanes[0].sessions.clear();
        b.prs[0].review_requested = true;
        let key = b.prs[0].key.clone();
        let snooze = |until: i64| vec![Snooze { key: key.clone(), at: NOW - 1_000, until }];
        apply_review_snoozes(&mut b.prs, &snooze(NOW + 60_000), NOW);
        assert_eq!(b.prs[0].snoozed_until, Some(NOW + 60_000));
        assert_eq!(logo_status(&b.lanes, &b.prs), LogoStatus::Idle);
        // Its time come, or woken, it's back.
        apply_review_snoozes(&mut b.prs, &snooze(NOW), NOW);
        assert_eq!(b.prs[0].snoozed_until, None);
        assert_eq!(logo_status(&b.lanes, &b.prs), LogoStatus::Needs);
        // Only a PR that waits on your review carries one.
        b.prs[0].review_requested = false;
        apply_review_snoozes(&mut b.prs, &snooze(NOW + 60_000), NOW);
        assert_eq!(b.prs[0].snoozed_until, None);
    }

    #[test]
    fn logo_counts_reviews_and_merges_only_when_each_is_in_the_queue() {
        let mut b = logo_board();
        b.lanes[0].sessions.clear();
        let mut review = b.prs[0].clone();
        review.review_requested = true;
        review.waits_to_merge = false;
        let mut merge = b.prs[0].clone();
        merge.number = 2;
        merge.key = "o/r#2".into();
        merge.review_requested = false;
        merge.waits_to_merge = true;
        assert_eq!(logo_status_with(&b.lanes, &[review.clone()], true, false), LogoStatus::Needs);
        assert_eq!(logo_status_with(&b.lanes, &[review.clone()], false, true), LogoStatus::Idle);
        assert_eq!(logo_status_with(&b.lanes, &[merge.clone()], false, true), LogoStatus::Needs);
        assert_eq!(logo_status_with(&b.lanes, &[merge.clone()], true, false), LogoStatus::Idle);
        assert_eq!(logo_status_with(&b.lanes, &[review, merge], false, false), LogoStatus::Idle);
    }

    #[test]
    fn a_pin_from_another_device_takes_only_sessions_listed_and_not_archived() {
        let archived = Session { archived: true, ..crate::fixtures::session("claude:claude:old") };
        let lanes = vec![crate::fixtures::lane("/a", vec![crate::fixtures::session("claude:claude:a"), archived])];
        let keys: Vec<String> = vec!["claude:claude:old".into(), "claude:claude:a".into(), "claude:claude:gone".into(), String::new()];
        assert_eq!(pinnable(&lanes, &keys, true), [&keys[1]]);
        // Any key comes off.
        assert_eq!(pinnable(&lanes, &keys, false).len(), 4);
    }

    #[test]
    fn assigns_sessions_to_the_deepest_lane() {
        let seeds = vec![seed("/r/spark", "app"), seed("/r/spark/.claude/worktrees/bold-fermi-1ac421", "claude/bold-fermi")];
        assert_eq!(best_seed(&seeds, Path::new("/r/spark/.claude/worktrees/bold-fermi-1ac421/src")), Some(1));
        assert_eq!(best_seed(&seeds, Path::new("/r/spark/apps")), Some(0));
        assert_eq!(best_seed(&seeds, Path::new("/r/spark-app2")), None);
    }

    #[test]
    fn the_seed_index_finds_what_best_seed_finds() {
        let exact = |path: &str| LaneSeed { exact: true, ..seed(path, "main") };
        let seeds = vec![
            seed("/r/spark", "app"),
            seed("/r/spark/.claude/worktrees/bold", "bold"),
            exact("/Users/me"),
            seed("/Users/me/projects/api", "api"),
            // The same folder twice: the last of seeds as deep is taken.
            seed("/r/spark/.claude/worktrees/bold", "bold again"),
            exact("/r/spark/apps"),
            seed("/r/plain/", "trailing slash"),
        ];
        let index = SeedIndex::new(&seeds);
        let paths = [
            "/r/spark",
            "/r/spark/",
            "/r/spark/src/main.rs",
            "/r/spark//src",
            "/r/spark/./src",
            "/r/spark/../spark/src",
            "/r/plain//x/",
            "/r/spark/.claude/worktrees/bold",
            "/r/spark/.claude/worktrees/bold/src/deep/file.ts",
            "/r/spark/.claude/worktrees/bolder",
            "/r/spark-app2",
            "/r/spark/apps",
            "/r/spark/apps/web",
            "/Users/me",
            "/Users/me/Downloads",
            "/Users/me/projects/api/src",
            "/r/plain",
            "/r/plain/x",
            "/",
            "",
            "relative/path",
        ];
        for p in paths {
            assert_eq!(index.best(Path::new(p)), best_seed(&seeds, Path::new(p)), "{p}");
        }
        // A seed for the root takes in every absolute path; an empty one, every path.
        let wide = vec![seed("/", "root"), seed("/r/spark", "app"), seed("", "empty")];
        let index = SeedIndex::new(&wide);
        for p in ["/r/spark/src", "/elsewhere", "relative", ""] {
            assert_eq!(index.best(Path::new(p)), best_seed(&wide, Path::new(p)), "{p}");
        }
    }

    #[test]
    fn lists_a_session_under_the_worktree_it_works_in() {
        let seeds = vec![seed("/r/codealong", "main"), seed("/r/codealong-pins", "pins")];
        let mut s = claude_session(&acct("claude"), Some(&reg("busy", NOW)), Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
        s.cwd = "/r/codealong".into();
        let at = |s: &Session| session_seed(&seeds, &SeedIndex::new(&seeds), s).map(|i| seeds[i].path.to_string_lossy().into_owned());
        let work = |paths: &[&str]| paths.iter().map(|p| p.to_string()).collect::<Vec<_>>();
        assert_eq!(at(&s).as_deref(), Some("/r/codealong"), "no work yet: where it started");
        s.work = work(&["/r/codealong", "/r/codealong-pins", "/r/codealong-pins/src/App.tsx", "/r/codealong-pins"]);
        assert_eq!(at(&s).as_deref(), Some("/r/codealong-pins"));
        s.work.push("/r/codealong".into());
        assert_eq!(at(&s).as_deref(), Some("/r/codealong-pins"), "one stray command doesn't move it");
        s.work = work(&["/r/codealong-pins", "/r/codealong-pins", "/r/codealong", "/r/codealong"]);
        assert_eq!(at(&s).as_deref(), Some("/r/codealong"), "a tie goes to the latest");
        s.work = work(&["/r/codealong-pins", "/tmp/scratch", "/Users/me/.claude/plans/x.md"]);
        assert_eq!(at(&s).as_deref(), Some("/r/codealong-pins"), "work outside every worktree doesn't count");
        s.cwd = "/Users/me/Downloads".into();
        s.work.clear();
        assert_eq!(at(&s), None);
    }

    #[test]
    fn a_plain_folder_elsewhere_takes_in_only_the_sessions_started_there() {
        let home = LaneSeed { project: ELSEWHERE.into(), origin: Origin::Folder, elsewhere: true, exact: true, ..seed("/Users/me", "main") };
        let seeds = vec![seed("/r/codealong", "main"), home];
        assert_eq!(best_seed(&seeds, Path::new("/Users/me")), Some(1));
        assert_eq!(best_seed(&seeds, Path::new("/Users/me/Downloads")), None, "a folder inside it has a lane of its own");
        let mut s = claude_session(&acct("claude"), Some(&reg("busy", NOW)), Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
        s.cwd = "/r/codealong".into();
        s.work = vec!["/Users/me".into(), "/Users/me".into(), "/Users/me/.claude/plans/x.md".into()];
        let at = |s: &Session| session_seed(&seeds, &SeedIndex::new(&seeds), s).map(|i| seeds[i].path.to_string_lossy().into_owned());
        assert_eq!(at(&s).as_deref(), Some("/r/codealong"), "work there doesn't count");
        s.cwd = "/Users/me".into();
        assert_eq!(at(&s).as_deref(), Some("/Users/me"));
    }

    #[test]
    fn work_in_an_apps_chats_does_not_keep_a_session_there() {
        let root = "/Users/me/Library/Application Support/Claude/scratch-workspaces";
        let chats = LaneSeed { project: ELSEWHERE.into(), origin: Origin::Folder, elsewhere: true, chats: true, ..seed(root, "main") };
        let seeds = vec![seed("/r/spark-infra", "main"), chats];
        let scratch = format!("{root}/a/b/scratch-2026-09-29-347017");
        let mut s = claude_session(&acct("claude"), Some(&reg("busy", NOW)), Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
        s.cwd = scratch.clone();
        s.work = vec![scratch.clone(), scratch.clone()];
        let at = |s: &Session| session_seed(&seeds, &SeedIndex::new(&seeds), s).map(|i| seeds[i].path.to_string_lossy().into_owned());
        assert_eq!(at(&s).as_deref(), Some(root), "a chat with nowhere else to be");
        s.cwd = "/r/spark-infra".into();
        assert_eq!(at(&s).as_deref(), Some("/r/spark-infra"), "given a folder, it goes there, though its shell is still in the chat's");
        s.cwd = scratch.clone();
        s.work.insert(0, "/r/spark-infra".into());
        assert_eq!(at(&s).as_deref(), Some("/r/spark-infra"), "one that worked in a repo is about that repo");
    }

    #[test]
    fn an_apps_chats_and_other_folders_elsewhere_show_like_worktrees_while_they_hold_a_session() {
        let apps = AppNames::default();
        let root = "/Users/me/Library/Application Support/Claude/scratch-workspaces";
        let elsewhere = |path: &str, chats: bool| LaneSeed { project: ELSEWHERE.into(), origin: Origin::Folder, elsewhere: true, chats, exact: !chats, ..seed(path, "main") };
        let seeds = vec![elsewhere(root, true), elsewhere("/Users/me/notes", false)];
        let a = acct("claude");
        let session_in = |id: &str, cwd: &str, status: &str| {
            let mut s = claude_session(&a, Some(&reg(status, NOW)), Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
            (s.id, s.key, s.cwd) = (id.into(), Session::key_for(Harness::Claude, "claude", id), cwd.into());
            s
        };
        let chat = session_in("chat", &format!("{root}/a/b/scratch-2026-09-28-045217"), "idle");
        let (git, prs, visitors) = (HashMap::new(), HashMap::new(), HashMap::new());
        let order = vec!["Spruce".to_string()];
        let hidden = |sessions: Vec<Session>| {
            let board = compose(Inputs { now: NOW, apps: &apps, seeds: &seeds, sessions, visitors: &visitors, git: &git, prs: &prs, requested: &HashMap::new(), made: &HashMap::new(), project_order: &order, notices: vec![], loading: false, github_at: None, pinned: &[], github_login: None, pr_links: &PrLinks::default(), previews: &Previews::default(), reusable: &HashMap::new(), review_snoozes: &[], session_settings: &Default::default(), auto_reviews: (&Default::default(), &Default::default(), &Default::default()), machines: &Default::default(), at_merged: &HashSet::new(), working_elsewhere: &BTreeSet::new() });
            let lane = |p: &str| board.lanes.iter().find(|l| l.path == p).unwrap().clone();
            assert!(!lane(root).looks_finished);
            assert!(lane(root).chats && !lane("/Users/me/notes").chats);
            (lane(root).hidden, lane("/Users/me/notes").hidden)
        };
        let notes = session_in("notes", "/Users/me/notes", "idle");
        assert_eq!(hidden(vec![chat.clone(), notes.clone()]), (false, false), "a chat whose turn ended shows, and a folder elsewhere whose session needs nothing: All projects lists them");
        let archived = Session { archived: true, ..chat };
        let waiting = session_in("notes", "/Users/me/notes", "waiting");
        assert_eq!(hidden(vec![archived.clone(), waiting]), (true, false), "archived chats don't hold the lane up");
        assert_eq!(hidden(vec![archived, Session { archived: true, ..notes }]), (true, true), "nor does a folder's archived session");
    }

    #[test]
    fn codealongs_chats_wait_hidden_and_make_no_project_until_a_chat_starts() {
        let apps = AppNames::default();
        let root = "/Users/me/Library/Application Support/Codealong/chats";
        let chats = LaneSeed { project: ELSEWHERE.into(), origin: Origin::Folder, elsewhere: true, chats: true, starts_chats: true, ..seed(root, "main") };
        let seeds = vec![seed("/r/spark", "main"), chats];
        let (git, prs, visitors) = (HashMap::new(), HashMap::new(), HashMap::new());
        let order = vec!["Spruce".to_string()];
        let board = |sessions: Vec<Session>| compose(Inputs { now: NOW, apps: &apps, seeds: &seeds, sessions, visitors: &visitors, git: &git, prs: &prs, requested: &HashMap::new(), made: &HashMap::new(), project_order: &order, notices: vec![], loading: false, github_at: None, pinned: &[], github_login: None, pr_links: &PrLinks::default(), previews: &Previews::default(), reusable: &HashMap::new(), review_snoozes: &[], session_settings: &Default::default(), auto_reviews: (&Default::default(), &Default::default(), &Default::default()), machines: &Default::default(), at_merged: &HashSet::new(), working_elsewhere: &BTreeSet::new() });
        let empty = board(vec![]);
        let lane = empty.lanes.iter().find(|l| l.path == root).unwrap();
        assert!(lane.starts_chats && lane.hidden, "there to start in, out of sight");
        assert!(!empty.projects.iter().any(|p| p.name == ELSEWHERE), "an empty chats lane makes no Elsewhere");
        let mut chat = claude_session(&acct("claude"), Some(&reg("idle", NOW)), Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
        chat.cwd = format!("{root}/2026-10-05-a1b2c3");
        let started = board(vec![chat]);
        let lane = started.lanes.iter().find(|l| l.path == root).unwrap();
        assert_eq!((lane.hidden, lane.sessions.len()), (false, 1));
        assert!(started.projects.iter().any(|p| p.name == ELSEWHERE && p.lanes == 1));
    }

    #[test]
    fn a_session_working_in_another_worktree_puts_it_in_working() {
        let apps = AppNames::default();
        let seeds = vec![seed("/r/codealong", "main"), seed("/r/codealong-pins", "pins")];
        let mut s = claude_session(&acct("claude"), Some(&reg("busy", NOW)), Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
        s.cwd = "/r/codealong".into();
        s.work = vec!["/r/codealong-pins".into(); 3];
        let (git, prs, visitors) = (HashMap::new(), HashMap::new(), HashMap::new());
        let order = vec!["Spruce".to_string()];
        let board = compose(Inputs { now: NOW, apps: &apps, seeds: &seeds, sessions: vec![s], visitors: &visitors, git: &git, prs: &prs, requested: &HashMap::new(), made: &HashMap::new(), project_order: &order, notices: vec![], loading: false, github_at: None, pinned: &[], github_login: None, pr_links: &PrLinks::default(), previews: &Previews::default(), reusable: &HashMap::new(), review_snoozes: &[], session_settings: &Default::default(), auto_reviews: (&Default::default(), &Default::default(), &Default::default()), machines: &Default::default(), at_merged: &HashSet::new(), working_elsewhere: &BTreeSet::new() });
        let lane = |p: &str| board.lanes.iter().find(|l| l.path == p).unwrap();
        assert_eq!(lane("/r/codealong-pins").bay, Bay::Working);
        assert_eq!(lane("/r/codealong-pins").sessions.len(), 1);
        assert_eq!(lane("/r/codealong").bay, Bay::Idle);
        assert!(lane("/r/codealong").sessions.is_empty());
    }

    #[test]
    fn sessions_sort_by_status_then_your_last_prompt() {
        let a = acct("claude");
        let at = |status: &str, prompt: Option<i64>, activity: i64, key: &str| {
            let t = TranscriptInfo { last_prompt_at: prompt, last_ts: activity, ..tr() };
            let mut s = claude_session(&a, Some(&reg(status, NOW - 7_200_000)), Some(&t), NOW, NOW, &Quiet::default()).unwrap();
            s.key = key.into();
            s
        };
        // The one working hardest right now doesn't jump ahead of the one you asked more recently.
        // A stopped session (no registry entry) ranks with idle ones, by your last prompt.
        let mut stopped = at("idle", Some(NOW - 30_000), NOW - 20_000, "stopped");
        stopped.status = Status::Stopped;
        let mut sessions = vec![
            at("busy", Some(NOW - 3_600_000), NOW - 1_000, "old-ask"),
            at("idle", Some(NOW - 60_000), NOW - 30_000, "finished"),
            stopped,
            at("busy", Some(NOW - 600_000), NOW - 400_000, "new-ask"),
            at("waiting", Some(NOW - 7_200_000), NOW - 7_000_000, "needs"),
        ];
        sort_sessions(&mut sessions);
        assert_eq!(sessions.iter().map(|s| s.key.as_str()).collect::<Vec<_>>(), vec!["needs", "new-ask", "old-ask", "stopped", "finished"]);
        // Those that need you go by how long they've waited, not by your last prompt: a new one
        // can't keep pushing an old one down.
        let mut waiting = vec![at("waiting", Some(NOW - 60_000), NOW - 30_000, "prompted-last"), at("waiting", Some(NOW - 600_000), NOW - 500_000, "prompted-first")];
        waiting[0].status_since = Some(NOW - 30_000);
        waiting[1].status_since = Some(NOW - 500_000);
        sort_sessions(&mut waiting);
        assert_eq!(waiting.iter().map(|s| s.key.as_str()).collect::<Vec<_>>(), vec!["prompted-first", "prompted-last"]);
        // With no prompt in its log, a session counts from when it started.
        let unknown = at("busy", None, NOW, "unknown");
        assert_eq!(unknown.last_prompt_at, NOW - 3_600_000);
    }

    #[test]
    fn needs_you_lists_the_longest_wait_first() {
        const MIN: i64 = 60_000;
        let a = acct("claude");
        let session = |status: &str, since: i64, prompt: i64| {
            let t = TranscriptInfo { last_prompt_at: Some(prompt), last_ts: since, ..tr() };
            claude_session(&a, Some(&reg(status, since)), Some(&t), NOW, NOW, &Quiet::default()).unwrap()
        };
        let lane = |id: &str, bay: Bay, sessions: Vec<Session>| Lane { bay, ..crate::fixtures::lane(id, sessions) };
        let mut snoozed = session("waiting", NOW - 90 * MIN, NOW - 100 * MIN);
        snoozed.snoozed_until = Some(NOW + 60 * MIN);
        let mut lanes = vec![
            lane("new", Bay::Needs, vec![session("waiting", NOW - MIN, NOW - 2 * MIN)]),
            // Needs you only through a snoozed agent: after those waiting.
            lane("snoozed", Bay::Needs, vec![snoozed]),
            lane("old", Bay::Needs, vec![session("busy", NOW - MIN, NOW - MIN), session("waiting", NOW - 40 * MIN, NOW - 45 * MIN)]),
            // The other bays still put the one you last prompted first.
            lane("idle-old", Bay::Idle, vec![session("idle", NOW - 50 * MIN, NOW - 60 * MIN)]),
            lane("idle-new", Bay::Idle, vec![session("idle", NOW - 5 * MIN, NOW - 10 * MIN)]),
        ];
        lanes.sort_by(lane_order);
        assert_eq!(lanes.iter().map(|l| l.id.as_str()).collect::<Vec<_>>(), vec!["old", "new", "snoozed", "idle-new", "idle-old"]);
    }

    #[test]
    fn scratchpad_worktrees_show_the_session_that_made_them() {
        let apps = AppNames::default();
        let scratch = "/private/tmp/claude-501/-r-vault/abc12345-0000-0000-0000-000000000000/scratchpad/spark-nx-cache";
        let seeds = vec![seed("/r/vault", "main"), seed(scratch, "nx-cache")];
        let a = acct("claude");
        let mut maker = claude_session(&a, Some(&reg("busy", NOW)), Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
        maker.id = "abc12345-0000-0000-0000-000000000000".into();
        maker.key = Session::key_for(Harness::Claude, "claude", &maker.id);
        maker.cwd = "/r/vault".into();
        let (git, prs, order) = (HashMap::new(), HashMap::new(), vec!["Spruce".to_string()]);
        let visitors = HashMap::new();
        let board = compose(Inputs { now: NOW, apps: &apps, seeds: &seeds, sessions: vec![maker], visitors: &visitors, git: &git, prs: &prs, requested: &HashMap::new(), made: &HashMap::new(), project_order: &order, notices: vec![], loading: false, github_at: None, pinned: &[], github_login: None, pr_links: &PrLinks::default(), previews: &Previews::default(), reusable: &HashMap::new(), review_snoozes: &[], session_settings: &Default::default(), auto_reviews: (&Default::default(), &Default::default(), &Default::default()), machines: &Default::default(), at_merged: &HashSet::new(), working_elsewhere: &BTreeSet::new() });
        let lane = board.lanes.iter().find(|l| l.path == scratch).unwrap();
        assert_eq!(lane.sessions.len(), 1);
        assert_eq!(lane.sessions[0].visitor.as_deref(), Some("made this worktree"));
        assert_eq!(lane.bay, Bay::Idle, "the maker works in another lane, so it doesn't make this one Working");
        assert_eq!(lane.headline, "Made by `claude` · Portal daily log + photo sync");
        assert_eq!(lane.default_target.as_deref(), Some("s|claude:claude:abc12345-0000-0000-0000-000000000000|focus"));
        assert_eq!(board.lanes.iter().find(|l| l.path == "/r/vault").unwrap().sessions.len(), 1);
    }

    #[test]
    fn archived_sessions_stay_listed_but_out_of_the_lanes_status() {
        let apps = AppNames::default();
        let seeds = vec![seed("/r/spark-portal", "portal")];
        let a = acct("claude2");
        let mut needs = claude_session(&a, Some(&reg("waiting", NOW)), Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
        needs.archived = true;
        let (git, prs, visitors) = (HashMap::new(), HashMap::new(), HashMap::new());
        let order = vec!["Spruce".to_string()];
        let board = compose(Inputs {
            now: NOW,
            apps: &apps,
            seeds: &seeds,
            sessions: vec![needs],
            visitors: &visitors,
            git: &git,
            prs: &prs,
            requested: &HashMap::new(),
            made: &HashMap::new(),
            project_order: &order,
            notices: vec![],
            loading: false,
            github_at: None,
            pinned: &[],
            github_login: None,
            pr_links: &PrLinks::default(),
            previews: &Previews::default(),
            reusable: &HashMap::new(),
            review_snoozes: &[],
            session_settings: &Default::default(), auto_reviews: (&Default::default(), &Default::default(), &Default::default()), machines: &Default::default(), at_merged: &HashSet::new(), working_elsewhere: &BTreeSet::new(),
        });
        let lane = &board.lanes[0];
        assert_eq!(lane.bay, Bay::Idle);
        assert_eq!(lane.sessions.len(), 1);
        assert!(lane.sessions[0].archived);
        assert!(lane.targets.iter().all(|t| !t.id.starts_with("s|")));
    }

    #[test]
    fn pinned_and_running_sessions_are_never_cut_from_a_lane() {
        let apps = AppNames::default();
        let seeds = vec![seed("/r/spark-portal", "portal")];
        let a = acct("claude2");
        let sessions: Vec<Session> = (0..MAX_SESSIONS_PER_LANE + 5)
            .map(|i| {
                let mut s = claude_session(&a, None, Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
                s.key = Session::key_for(Harness::Claude, "claude2", &format!("s{i}"));
                s.last_activity = NOW - i as i64 * 60_000;
                // An old one still running, behind thirty newer ones.
                s.live = i == MAX_SESSIONS_PER_LANE + 3;
                s
            })
            .collect();
        let running = format!("claude:claude2:s{}", MAX_SESSIONS_PER_LANE + 3);
        let oldest = format!("claude:claude2:s{}", MAX_SESSIONS_PER_LANE + 4);
        let pinned = vec![oldest.clone()];
        let (git, prs, visitors) = (HashMap::new(), HashMap::new(), HashMap::new());
        let order = vec!["Spruce".to_string()];
        let board = compose(Inputs {
            now: NOW,
            apps: &apps,
            seeds: &seeds,
            sessions,
            visitors: &visitors,
            git: &git,
            prs: &prs,
            requested: &HashMap::new(),
            made: &HashMap::new(),
            project_order: &order,
            notices: vec![],
            loading: false,
            github_at: None,
            pinned: &pinned,
            github_login: None,
            pr_links: &PrLinks::default(),
            previews: &Previews::default(),
            reusable: &HashMap::new(),
            review_snoozes: &[],
            session_settings: &Default::default(), auto_reviews: (&Default::default(), &Default::default(), &Default::default()), machines: &Default::default(), at_merged: &HashSet::new(), working_elsewhere: &BTreeSet::new(),
        });
        let lane = &board.lanes[0];
        assert_eq!(lane.sessions.len(), MAX_SESSIONS_PER_LANE + 2);
        assert!(lane.sessions.iter().any(|s| s.key == oldest));
        assert!(lane.sessions.iter().any(|s| s.key == running));
        assert_eq!(board.pinned, pinned);
    }

    /// The readers hand sessions over in their maps' order, which differs every time. A lane's
    /// list mustn't follow it, archived sessions and ties included, or every board composed is a
    /// new one to publish when nothing changed (it did, 7 to 11 lanes a time, 2026-10-16).
    #[test]
    fn a_lanes_sessions_come_in_one_order_whatever_order_they_were_read_in() {
        let apps = AppNames::default();
        let seeds = vec![seed("/r/spark-portal", "portal")];
        let a = acct("claude2");
        let sessions: Vec<Session> = (0..8)
            .map(|i| {
                let mut s = claude_session(&a, None, Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
                s.key = Session::key_for(Harness::Claude, "claude2", &format!("s{i}"));
                // Two of each time, so the times alone leave ties; half of them archived.
                s.last_prompt_at = NOW - (i / 2) as i64 * 60_000;
                s.last_activity = s.last_prompt_at;
                s.archived = i % 4 >= 2;
                s
            })
            .collect();
        let (git, prs, visitors) = (HashMap::new(), HashMap::new(), HashMap::new());
        let order = vec!["Spruce".to_string()];
        let keys = |sessions: Vec<Session>| {
            let board = compose(Inputs { now: NOW, apps: &apps, seeds: &seeds, sessions, visitors: &visitors, git: &git, prs: &prs, requested: &HashMap::new(), made: &HashMap::new(), project_order: &order, notices: vec![], loading: false, github_at: None, pinned: &[], github_login: None, pr_links: &PrLinks::default(), previews: &Previews::default(), reusable: &HashMap::new(), review_snoozes: &[], session_settings: &Default::default(), auto_reviews: (&Default::default(), &Default::default(), &Default::default()), machines: &Default::default(), at_merged: &HashSet::new(), working_elsewhere: &BTreeSet::new() });
            board.lanes[0].sessions.iter().map(|s| s.key.clone()).collect::<Vec<_>>()
        };
        let read = keys(sessions.clone());
        assert_eq!(read, keys(sessions.into_iter().rev().collect()));
        // Archived ones last, the latest first, as the others are.
        let archived: Vec<String> = [2, 3, 6, 7].iter().map(|i| format!("claude:claude2:s{i}")).collect();
        assert_eq!(read[4..], archived[..]);
    }

    #[test]
    fn composes_a_board() {
        let apps = AppNames::default();
        let seeds = vec![seed("/r/spark-portal", "portal"), seed("/r/spark-app3", "app3")];
        let a = acct("claude2");
        let needs = claude_session(&a, Some(&reg("waiting", NOW)), Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
        let git = HashMap::new();
        let prs = HashMap::new();
        let order = vec!["Spruce".to_string()];
        let visitors = HashMap::from([(PathBuf::from("/r/spark-app3"), vec!["claude:claude2:abc1234567".to_string()])]);
        let board = compose(Inputs {
            now: NOW,
            apps: &apps,
            seeds: &seeds,
            sessions: vec![needs],
            visitors: &visitors,
            git: &git,
            prs: &prs,
            requested: &HashMap::new(),
            made: &HashMap::new(),
            project_order: &order,
            notices: vec![],
            loading: false,
            github_at: None,
            pinned: &[],
            github_login: None,
            pr_links: &PrLinks::default(),
            previews: &Previews::default(),
            reusable: &HashMap::new(),
            review_snoozes: &[],
            session_settings: &Default::default(), auto_reviews: (&Default::default(), &Default::default(), &Default::default()), machines: &Default::default(), at_merged: &HashSet::new(), working_elsewhere: &BTreeSet::new(),
        });
        assert_eq!(board.lanes.len(), 2);
        assert_eq!(board.lanes[0].bay, Bay::Needs);
        assert_eq!(board.lanes[0].sessions.len(), 1);
        assert_eq!(board.lanes[1].bay, Bay::Idle, "a visitor doesn't change the bay");
        assert_eq!(board.lanes[1].sessions[0].visitor.as_deref(), Some("worked here"));
        assert_eq!(board.lanes[1].headline, "Worked on by `claude2` · Portal daily log + photo sync");
        assert_eq!(board.lanes[0].sessions[0].headline, "`claude2` is waiting for approval · Portal daily log + photo sync");
        assert_eq!(board.projects[0].lanes, 2);
    }

    #[test]
    fn a_new_terminal_goes_beside_its_worktree_then_its_repo_then_its_project() {
        let apps = AppNames::default();
        let mut other_repo = seed("/r/spur", "main");
        other_repo.repo = "spur".into();
        let mut elsewhere = seed("/tmp/scratch", "main");
        (elsewhere.project, elsewhere.origin) = ("Elsewhere".into(), Origin::Other);
        let folder = |path: &str, chats: bool| LaneSeed { project: ELSEWHERE.into(), origin: Origin::Folder, elsewhere: true, chats, exact: !chats, ..seed(path, "main") };
        let seeds = vec![seed("/r/spark", "main"), seed("/r/spark-pins", "pins"), other_repo, elsewhere, folder("/Users/me/Documents/Codex", true), folder("/Users/me/notes", false)];
        let (git, prs, visitors) = (HashMap::new(), HashMap::new(), HashMap::new());
        let order = vec!["Spruce".to_string()];
        let board = compose(Inputs { now: NOW, apps: &apps, seeds: &seeds, sessions: vec![], visitors: &visitors, git: &git, prs: &prs, requested: &HashMap::new(), made: &HashMap::new(), project_order: &order, notices: vec![], loading: false, github_at: None, pinned: &[], github_login: None, pr_links: &PrLinks::default(), previews: &Previews::default(), reusable: &HashMap::new(), review_snoozes: &[], session_settings: &Default::default(), auto_reviews: (&Default::default(), &Default::default(), &Default::default()), machines: &Default::default(), at_merged: &HashSet::new(), working_elsewhere: &BTreeSet::new() });
        let near = |p: &str| terminal_neighbours(&board.lanes, Path::new(p));
        let at = |p: &str| (near(p).folders, near(p).project);
        let strings = |groups: &[&[&str]]| groups.iter().map(|g| g.iter().map(|f| f.to_string()).collect()).collect::<Vec<Vec<String>>>();
        assert_eq!(at("/r/spark-pins/src"), (strings(&[&["/r/spark-pins"], &["/r/spark"], &["/r/spur"]]), Some("Spruce".to_string())));
        assert_eq!(at("/tmp/scratch"), (strings(&[&["/tmp/scratch"]]), Some(ELSEWHERE.to_string())));
        // Other folders elsewhere aren't company, but they're the same project's.
        assert_eq!(at("/Users/me/Documents/Codex/2026-09-27/can-you"), (strings(&[&["/Users/me/Documents/Codex"]]), Some(ELSEWHERE.to_string())));
        assert_eq!(at("/Users/me/notes"), (strings(&[&["/Users/me/notes"]]), Some(ELSEWHERE.to_string())));
        assert_eq!(at("/Users/me"), (strings(&[&["/Users/me"]]), None), "a folder in no worktree is in no project");
    }

    #[test]
    fn a_sessions_tab_is_named_by_its_worktree_then_its_title() {
        let lanes = vec![crate::fixtures::lane("/r/spark", vec![]), Lane { callsign: "pins".into(), ..crate::fixtures::lane("/r/spark-pins", vec![]) }];
        let label = |p: &str, title: &str| tab_label(&lanes, Path::new(p), title);
        assert_eq!(label("/r/spark-pins/src", "Fix the flaky test").as_deref(), Some("pins · Fix the flaky test"));
        assert_eq!(label("/r/spark", "  Two\tlines\nof \u{1b}[31mtitle ").as_deref(), Some("r/spark · Two lines of [31mtitle"));
        assert_eq!(label("/r/spark-pins", "").as_deref(), Some("pins"));
        let long = label("/r/spark-pins", &"word ".repeat(20)).unwrap();
        assert_eq!(long, format!("pins · {}…", "word ".repeat(8).trim_end()));
        assert!(long.chars().count() <= "pins · ".len() + TAB_TITLE, "{long}");
        assert_eq!(label("/tmp/elsewhere", "Fix it"), None, "outside every worktree the agent's own title is as good");
    }

    /// Worktrees whose sessions are in every state the windows' rules tell apart, as the engine
    /// composes them from `sessions`: what each worktree's bay, glyph and sentence are, what each
    /// session's sentence is, and the order its sessions come in. lanes.ts works the same things
    /// out again (a filter that leaves some sessions out, a change made in a window ahead of the
    /// engine), each rule a copy of one here kept in step by hand: src/rules.test.ts holds it to
    /// these. `without` leaves one session out, as a filter does.
    fn rule_cases(without: Option<&str>) -> Vec<Lane> {
        const MIN: i64 = 60_000;
        // The id a scratchpad's folder names its maker by.
        const MAKER: &str = "abc12345-0000-0000-0000-000000000000";
        // A session of `id` in the worktree `at`, last prompted and last active so many minutes
        // ago: its status began with its last activity unless a case says otherwise.
        let at = |path: &str, id: &str, status: Status, prompted: i64, active: i64| Session {
            status,
            cwd: format!("/r/spark-{path}"),
            title: format!("Work on {id}"),
            status_since: Some(NOW - active * MIN),
            last_prompt_at: NOW - prompted * MIN,
            last_activity: NOW - active * MIN,
            ..crate::fixtures::session(&Session::key_for(Harness::Claude, "claude", id))
        };
        let said = |s: Session, excerpt: &str| Session { excerpt: Some(excerpt.into()), ..s };
        let helping = |s: Session, parent: &str, yours: bool| Session { helper: Some(Helper { parent: Some(Session::key_for(Harness::Claude, "claude", parent)), yours, ..Default::default() }), ..s };
        let mut sessions = vec![
            // Every way of needing you, the longest wait first whatever was prompted last, and one
            // of every other status beside them.
            Session { status_since: Some(NOW - 30 * MIN), ..said(at("needs", "approval", Status::Needs, 3, 1), "Bash · `make release`") },
            Session { harness: Harness::Codex, account: "codex".into(), ..said(at("needs", "codex-approval", Status::Needs, 60, 25), "Waiting for approval · `make`") },
            Session { excerpt: Some(asks_you("Which color?")), status_since: Some(NOW - 20 * MIN), ..at("needs", "question", Status::Needs, 40, 14) },
            Session { excerpt: Some(format!("{HELD_MESSAGE}“Review 12”")), ..at("needs", "message", Status::Needs, 16, 15) },
            // Set aside while its side conversation answers: still needing you, listed apart.
            Session { aside: Some(NOW - 5 * MIN), excerpt: Some(format!("{ASKING_ON_THE_SIDE}Are we restoring for real?")), status_since: Some(NOW - 22 * MIN), ..at("needs", "aside", Status::Needs, 41, 22) },
            Session { asks: true, ..said(at("needs", "reply", Status::Needs, 90, 10), "Asks you · Want the PR?") },
            Session { stalled_since: Some(NOW - 45 * MIN), excerpt: Some(format!("Stuck on `cargo build` for {{for:{}}}", NOW - 45 * MIN)), ..at("needs", "stuck", Status::Needs, 50, 45) },
            Session { excerpt: Some(format!("{TERMINAL_ASKS}`Continue? [y/N]`")), running: Some(RunningIn { terminal: "Codealong".into(), command: "./scripts/restore.sh".into(), yours: true }), ..at("needs", "input", Status::Needs, 18, 17) },
            // A helper it started waits in its terminal on a screen of its own (starting.rs).
            Session { excerpt: Some(format!("{START_WAITS}Codex: Hooks need review")), ..at("needs", "start", Status::Needs, 19, 16) },
            at("needs", "unsaid", Status::Needs, 6, 5),
            // No time for when it began to wait: it waits since it was last active.
            Session { status_since: None, ..said(at("needs", "untimed", Status::Needs, 70, 28), "Bash · `ls`") },
            Session { status_since: Some(NOW - 35 * MIN), ..said(at("needs", "failed-beside", Status::Error, 80, 2), "API error") },
            Session { status_since: Some(NOW - 33 * MIN), ..said(at("needs", "failed-later", Status::Error, 85, 30), "Timed out") },
            said(at("needs", "working-beside", Status::Working, 2, 1), "`npm run build`"),
            said(at("needs", "unread-beside", Status::Unread, 8, 2), "Done here."),
            at("needs", "idle-beside", Status::Idle, 9, 3),
            Session { live: false, ..at("needs", "stopped-beside", Status::Stopped, 7, 4) },
            // A helper and a headless run that wait on you count as any session does.
            helping(said(at("needs", "helper-waits", Status::Needs, 12, 4), "Bash · `gh pr diff 12`"), "approval", false),
            Session { headless: true, ..said(at("needs", "headless-waits", Status::Needs, 13, 11), "Bash · `rm -rf dist`") },
            // A turn that failed beside one that works: the worktree needs you for the failure.
            Session { live: false, cut_off: Some(NOW - 20 * MIN), ..said(at("failed", "cut-off", Status::Error, 30, 20), "`cargo test`") },
            Session { limit: Some(SessionLimit { at: NOW - 10 * MIN, ..Default::default() }), ..said(at("failed", "limited", Status::Error, 15, 10), "You've hit your limit") },
            said(at("failed", "errored", Status::Error, 6, 5), "API error"),
            said(at("failed", "working", Status::Working, 1, 1), "`make`"),
            helping(said(at("failed", "helper-failed", Status::Error, 4, 3), "Timed out"), "errored", false),
            // Working beside a finished turn: by the prompt, not by what was active last; one
            // prompted again since its status began; two that tie; and one that waits on a
            // command, which is working all the same.
            said(at("working", "prompted-first", Status::Working, 50, 1), "`npm test`"),
            said(at("working", "prompted-last", Status::Working, 2, 40), "Fix the build"),
            Session { status_since: Some(NOW - 30 * MIN), ..said(at("working", "prompted-again", Status::Working, 10, 5), "`cargo check`") },
            said(at("working", "same-prompt-a", Status::Working, 20, 9), "`make a`"),
            said(at("working", "same-prompt-b", Status::Working, 20, 3), "`make b`"),
            said(at("working", "tie-b", Status::Working, 22, 22), "`make tie b`"),
            said(at("working", "tie-a", Status::Working, 22, 22), "`make tie a`"),
            Session { background: Some("gh run watch".into()), ..said(at("working", "background", Status::Working, 25, 8), "Waiting on `gh run watch`") },
            Session { agents: vec!["Draft the release notes".into()], ..said(at("working", "agents", Status::Working, 27, 7), "Waiting on a background agent · Draft the release notes") },
            // A command you ran for it, whose output it's sent when it ends (terminal.rs `overlay_yours`).
            Session {
                running: Some(RunningIn { terminal: "Codealong".into(), command: "npm test".into(), yours: true }),
                ..said(at("working", "your-command", Status::Working, 29, 6), "Waiting on your command · `npm test`")
            },
            Session { excerpt: None, status_since: None, ..at("working", "silent", Status::Working, 26, 12) },
            said(at("working", "finished-beside", Status::Unread, 1, 1), "Done."),
            at("working", "idle-beside-work", Status::Idle, 3, 2),
            // A helper you talk to is yours, and counts.
            helping(said(at("working", "helper-yours", Status::Working, 5, 4), "`gh pr view`"), "prompted-first", true),
            // Finished turns: the ones moved to the top first, yours ahead of an agent's; what's
            // snoozed still listed; helpers and headless runs that don't need you after them, and
            // out of the worktree's status with what's archived.
            said(at("unread", "finished", Status::Unread, 20, 9), "Pushed the fix."),
            said(at("unread", "finished-first", Status::Unread, 9, 90), "All green."),
            // One whose work is finished (`finished_of`) is only yours to read: after the turns
            // that want something of you, though it was prompted last.
            Session { insight: Some(insight(|i| i.finished = Some(0.9))), ..said(at("unread", "wrapped-up", Status::Unread, 1, 1), "All done here.") },
            Session { archived: true, ..said(at("unread", "archived", Status::Needs, 4, 3), "Bash · `rm -rf build`") },
            helping(at("unread", "helper", Status::Working, 2, 1), "finished", false),
            Session { headless: true, ..at("unread", "headless", Status::Working, 7, 6) },
            Session { background: Some("npm run dev".into()), ..said(at("unread", "left-running", Status::Idle, 30, 29), "Left running · `npm run dev`") },
            Session { snoozed_until: Some(NOW + 60 * MIN), ..said(at("unread", "snoozed", Status::Unread, 8, 7), "Done.") },
            Session { raised: Some(Raised { at: NOW - 2 * MIN, ..Default::default() }), ..said(at("unread", "raised", Status::Unread, 300, 200), "Ready when you are.") },
            Session { raised: Some(Raised { at: NOW - MIN, agent: true, ..Default::default() }), ..said(at("unread", "raised-by-an-agent", Status::Unread, 400, 100), "Have a look.") },
            Session { raised: Some(Raised { at: NOW - 3 * MIN, ..Default::default() }), ..said(at("unread", "raised-before", Status::Unread, 100, 150), "Whenever.") },
            Session { raised: Some(Raised { at: NOW - 4 * MIN, agent: true, ..Default::default() }), ..said(at("unread", "raised-by-an-agent-before", Status::Unread, 50, 60), "When you can.") },
            // Nothing active: the one last active speaks for the worktree, whichever was prompted
            // last; one that stopped ranks with the idle ones.
            said(at("idle", "active-last", Status::Idle, 300, 20), "Merged."),
            said(at("idle", "prompted-last", Status::Idle, 30, 100), "Nothing else to do."),
            Session { live: false, ..at("idle", "stopped", Status::Stopped, 10, 500) },
            // An orchestrator's row says what its last run came to, not when it was.
            Session { orchestrator: Some(crate::model::Orchestrator { schedule: "orch".into(), ..Default::default() }), ..said(at("idle", "orchestrator", Status::Idle, 600, 600), NOTHING_NEW) },
            // A PR, in each state it can be in, speaks for a worktree whose agents are quiet,
            // and not for one where an agent works.
            said(at("review", "in-review", Status::Idle, 410, 400), "Opened the PR."),
            at("draft", "drafted", Status::Idle, 420, 410),
            at("approved", "was-approved", Status::Idle, 430, 420),
            at("changes", "asked-for-changes", Status::Idle, 440, 430),
            said(at("merged", "after-merge", Status::Idle, 610, 600), "Merged it."),
            at("closed", "after-close", Status::Idle, 620, 610),
            said(at("review-at-work", "at-work", Status::Working, 2, 1), "`git push`"),
            // Another worktree's session that worked here is listed, and isn't this worktree's
            // to answer for; with none of its own, it speaks for the worktree.
            at("visited", "at-home", Status::Idle, 50, 40),
            said(at("visitors-home", "visitor", Status::Needs, 5, 4), "Bash · `make deploy`"),
            // A worktree a session made in its scratchpad lists its maker, which works elsewhere.
            Session { id: MAKER.into(), ..at("makers-home", MAKER, Status::Working, 3, 2) },
            // A reusable worktree's work from before it was last reset isn't its work now.
            said(at("reused", "from-before", Status::Unread, 400, 300), "Shipped it."),
            at("reused", "current", Status::Idle, 30, 20),
        ];
        // As the engine does once nothing more is laid over a session (`Session::settle_need`).
        sessions.iter_mut().for_each(Session::settle_need);
        sessions.retain(|s| Some(s.key.as_str()) != without);
        let places = [
            "needs", "failed", "working", "unread", "idle", "review", "draft", "approved", "changes", "merged", "closed", "review-at-work", "visited", "visitors-home", "visited-only", "reused", "empty", "dirty", "committed",
            "committed-undated", "makers-home",
        ];
        let mut seeds: Vec<LaneSeed> = places.iter().map(|p| LaneSeed { callsign: p.to_string(), ..seed(&format!("/r/spark-{p}"), p) }).collect();
        seeds.push(LaneSeed { callsign: "made".into(), ..seed(&format!("/private/tmp/claude-501/-r-spark-makers-home/{MAKER}/scratchpad/spark-made"), "made") });
        let pr_of = |branch: &str, state: PrState, number: u64| PrRaw {
            head_ref: branch.into(),
            number,
            merged_at: (state == PrState::Merged).then_some(NOW - 7_200_000),
            closed_at: (state == PrState::Closed).then_some(NOW - 3_600_000),
            ..pr(PrState::Open, 0)
        };
        let prs = [("review", PrState::Open), ("draft", PrState::Draft), ("approved", PrState::Approved), ("changes", PrState::Changes), ("merged", PrState::Merged), ("closed", PrState::Closed), ("review-at-work", PrState::Open)];
        let prs = HashMap::from([("sprucehq/spark".to_string(), prs.iter().enumerate().map(|(i, (branch, state))| PrRaw { state: *state, ..pr_of(branch, *state, 1037 + i as u64) }).collect::<Vec<_>>())]);
        let visitor = Session::key_for(Harness::Claude, "claude", "visitor");
        let visitors = HashMap::from([(PathBuf::from("/r/spark-visited"), vec![visitor.clone()]), (PathBuf::from("/r/spark-visited-only"), vec![visitor])]);
        let git = HashMap::from([
            (PathBuf::from("/r/spark-reused"), GitInfo { reset_at: Some(NOW - 120 * MIN), base_ahead: Some(0), ..Default::default() }),
            (PathBuf::from("/r/spark-dirty"), GitInfo { dirty: 3, last_commit: Some("Fix the tests".into()), last_commit_at: Some(NOW - 50 * MIN), ..Default::default() }),
            (PathBuf::from("/r/spark-committed"), GitInfo { last_commit: Some("Let `portal` photos carry a caption, and say whose they are in the heading of each page they show on".into()), last_commit_at: Some(NOW - 90 * MIN), ..Default::default() }),
            (PathBuf::from("/r/spark-committed-undated"), GitInfo { last_commit: Some("Start".into()), ..Default::default() }),
        ]);
        let board = compose(Inputs {
            now: NOW,
            apps: &AppNames::default(),
            seeds: &seeds,
            sessions,
            visitors: &visitors,
            git: &git,
            prs: &prs,
            requested: &HashMap::new(),
            made: &HashMap::new(),
            project_order: &["Spruce".to_string()],
            notices: vec![],
            loading: false,
            github_at: None,
            pinned: &[],
            github_login: Some("gwen"),
            pr_links: &PrLinks::default(),
            previews: &Previews::default(),
            reusable: &HashMap::from([("/r/spark-reused".to_string(), true)]),
            review_snoozes: &[],
            session_settings: &Default::default(),
            auto_reviews: (&Default::default(), &Default::default(), &Default::default()),
            machines: &Default::default(), at_merged: &HashSet::new(), working_elsewhere: &BTreeSet::new(),
        });
        // As the engine does once the board has its PRs and todos.
        let mut lanes = board.lanes;
        settle_finished(&mut lanes, &board.prs, &[]);
        // Where each opens says its folder with this Mac's home for `~`, and no rule reads it.
        lanes.iter_mut().for_each(|l| l.targets.clear());
        lanes
    }

    /// The rule cases as src/rules.test.ts reads them: the worktrees as composed; of each
    /// session, what the engine's own rules say of it that no field does; and of each worktree
    /// with one of its sessions left out, what it's then said to be. Written with
    /// `CODEALONG_WRITE_FIXTURES=1 cargo test rule_cases`.
    #[test]
    fn rule_cases_fixture_is_current() {
        let lanes = rule_cases(None);
        // The cases hold a rule only by having what it tells apart: every bay, every way of
        // needing you, and what's left out of a worktree's status each way it can be.
        let bays: std::collections::BTreeSet<Bay> = lanes.iter().map(|l| l.bay).collect();
        assert_eq!(bays.len(), 6, "{bays:?}");
        let all = || lanes.iter().flat_map(|l| &l.sessions);
        let needs: Vec<Option<Need>> = all().filter(|s| s.status == Status::Needs && !s.archived).map(|s| s.need).collect();
        for need in [Some(Need::Approval), Some(Need::Question), Some(Need::Reply), Some(Need::Message), Some(Need::Stuck), Some(Need::Input), Some(Need::Start), None] {
            assert!(needs.contains(&need), "{need:?}");
        }
        assert!(all().any(|s| s.visitor.is_some()) && all().any(|s| s.earlier) && all().any(|s| s.archived) && all().any(|s| s.unattended()), "a visitor, earlier work, an archived session and one unattended");
        let visits: std::collections::BTreeSet<&str> = all().filter_map(|s| s.visitor.as_deref()).collect();
        assert_eq!(visits.len(), 2, "a session that worked in a worktree and one that made it: {visits:?}");
        // A PR in each state it can be in, each still its worktree's.
        let states: Vec<PrState> = lanes.iter().filter_map(|l| l.pr.as_ref().map(|p| p.state)).collect();
        for state in [PrState::Draft, PrState::Open, PrState::Approved, PrState::Changes, PrState::Merged, PrState::Closed] {
            assert!(states.contains(&state), "{state:?}");
        }
        // By worktree and key: two cases in different worktrees may share an id.
        let says: serde_json::Map<String, serde_json::Value> = lanes
            .iter()
            .flat_map(|l| l.sessions.iter().map(move |s| (l, s)))
            .filter(|(_, s)| s.visitor.is_none())
            .map(|(l, s)| {
                let said = serde_json::json!({
                    "unattended": s.unattended(),
                    "helpsAnAgent": s.helps_an_agent(),
                    "waitsInBackground": s.waits_in_background(),
                    "waitsAtAPrompt": s.waits_at_a_prompt(),
                    "unfinished": s.unfinished(),
                });
                (format!("{}::{}", l.id, s.key), said)
            })
            .collect();
        // Each worktree again with one of the sessions it lists gone, a visitor among them, as a
        // filter leaves it.
        let mut without = Vec::new();
        for lane in &lanes {
            for s in &lane.sessions {
                let left = rule_cases(Some(&s.key));
                let now = left.iter().find(|l| l.id == lane.id).expect("the worktree stays");
                without.push(serde_json::json!({ "lane": lane.id, "session": s.key, "bay": now.bay, "glyph": now.glyph, "headline": now.headline, "detail": now.detail }));
            }
        }
        let made = serde_json::json!({ "lanes": lanes, "says": says, "without": without });
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/testing/ruleCases.json");
        if std::env::var_os("CODEALONG_WRITE_FIXTURES").is_some() {
            // A worktree to a line, and a session left out to a line, so a rule that changes
            // shows as the lines it changed.
            let lines = |of: &str| made[of].as_array().unwrap().iter().map(|l| format!("    {l}")).collect::<Vec<_>>().join(",\n");
            let text = format!("{{\n  \"lanes\": [\n{}\n  ],\n  \"says\": {},\n  \"without\": [\n{}\n  ]\n}}\n", lines("lanes"), made["says"], lines("without"));
            std::fs::write(&path, text).unwrap();
        }
        let kept: serde_json::Value = std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        assert!(kept == made, "src/testing/ruleCases.json is stale: a rule of the board's changed, or what a session or worktree is sent as did. Write it again with `CODEALONG_WRITE_FIXTURES=1 cargo test rule_cases`, then `npm test rules` says which rule of lanes.ts no longer agrees.");
    }

    #[test]
    fn an_account_that_reads_signed_out_with_sessions_stopped_on_it_offers_a_sign_in() {
        let out = |key: &str, continues: bool| Session { title: key.into(), signed_out: Some(SignedOut { at: NOW - 60_000, said: "OAuth token revoked · Please run /login".into(), continues }), ..crate::fixtures::session(key) };
        let usage = |account: &str, signed_in: bool| Usage { account: account.into(), harness: Harness::Claude, plan: None, meters: Vec::new(), as_of: Some(NOW), error: None, needs_sign_in: !signed_in, loading: false, no_limits: false, preferred: false, rate_limited: false };
        let mut board = Board { lanes: vec![crate::fixtures::lane("/r", vec![out("a", true), out("b", true), crate::fixtures::session("c")])], usage: vec![usage("claude", false)], ..Board::default() };
        let said = signed_out_notices(&board);
        assert_eq!(said.len(), 1);
        assert_eq!((said[0].key.as_str(), said[0].title.as_str(), said[0].sign_in.as_deref()), ("signed-out:claude", "Signed out of claude", Some("claude")));
        assert_eq!(said[0].text, "2 sessions stopped because claude's sign-in was turned down. Sign in, and Codealong tells them to carry on.");
        // One alone is named; one Codealong won't carry on is yours to send on.
        board.lanes[0].sessions = vec![out("a", false)];
        assert_eq!(signed_out_notices(&board)[0].text, "“a” stopped because claude's sign-in was turned down. Sign in, then send it something to go on.");
        // Of some Codealong carries on and some it doesn't, those it doesn't are named.
        board.lanes[0].sessions = vec![out("a", true), out("b", false)];
        assert_eq!(signed_out_notices(&board)[0].text, "2 sessions stopped because claude's sign-in was turned down. Sign in, and Codealong tells them to carry on, but for “b”, which you send something to go on.");
        // None while the account reads signed in (they're carried on), for another account's
        // sessions, one that works again, one archived, or another Mac's.
        board.usage = vec![usage("claude", true)];
        assert!(signed_out_notices(&board).is_empty());
        board.usage = vec![usage("claude2", false)];
        assert!(signed_out_notices(&board).is_empty());
        board.usage = vec![usage("claude", false)];
        for s in [Session { status: Status::Working, ..out("a", true) }, Session { archived: true, ..out("a", true) }] {
            board.lanes[0].sessions = vec![s];
            assert!(signed_out_notices(&board).is_empty());
        }
        board.lanes[0].sessions = vec![out("a", true)];
        board.lanes[0].device = Some(DeviceTag { name: "Studio".into(), short: "st".into(), color: "blue".into(), online: true, last_seen: None });
        assert!(signed_out_notices(&board).is_empty());
    }
}
