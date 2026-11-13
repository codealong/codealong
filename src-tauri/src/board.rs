//! The rules from *The model*: session status, lane bays, headlines and open targets.
//! Pure functions over already-read data, so every rule is unit-tested here.

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

/// A Codex turn with no events for this long is treated as interrupted.
const CODEX_STALE_MS: i64 = 15 * 60 * 1000;
/// An OpenCode run with no writes for this long is treated as interrupted, when its service
/// isn't there to say.
const OPENCODE_STALE_MS: i64 = 15 * 60 * 1000;
/// An Antigravity conversation that says it's busy but hasn't changed for this long isn't.
const ANTIGRAVITY_STALE_MS: i64 = 30 * 60 * 1000;
/// A Cursor chat the app says is generating but hasn't written for this long isn't: the app
/// writes a running chat's composerData every 30 seconds, so this is long past a slow call.
const CURSOR_STALE_MS: i64 = 10 * 60 * 1000;
/// A Cursor command line that hasn't written its chat this long after it started waits on what
/// it asks first, in its terminal: it writes its chat within a few seconds otherwise.
const CURSOR_STARTING_MS: i64 = 20 * 1000;
/// A Bash call can't run past 10 minutes, so a Claude session that has said busy this long with
/// nothing new in its transcript is stuck, not working: typically a command waiting on input
/// no one will give it, like a browser sign-in inside a background job.
pub const CLAUDE_STALL_MS: i64 = 30 * 60 * 1000;
/// How long a Claude session whose process has ended stays Your turn for a turn you haven't
/// seen: long enough to come back to after a restart or a weekend away, short enough that tabs
/// you read in the terminal and closed don't pile up (asked 2026-10-06).
const CLAUDE_UNSEEN_STOPPED_MS: i64 = 3 * DAY_MS;

/// When a busy Claude session last showed signs of life in its transcript or status; None when
/// it isn't busy.
///
/// Only `busy` counts. `shell` is a turn that ended with a command left running in the
/// background: it runs detached from the terminal, so it can't be waiting for your typing, and
/// Claude Code wakes the session when it finishes. Polls for a review or CI run for hours.
pub fn claude_busy_since(reg: Option<&RegistryEntry>, tr: Option<&TranscriptInfo>) -> Option<i64> {
    let r = reg.filter(|r| r.status == "busy")?;
    Some([tr.map(|t| t.last_ts), r.status_updated_at].into_iter().flatten().max().unwrap_or(0))
}

/// What the engine found outside the transcript for a busy session that has gone quiet, or
/// waits on the Claude app.
#[derive(Default)]
pub struct Quiet<'a> {
    /// An approval the Claude app asks for itself (claude/applog.rs), which Claude Code's
    /// registry doesn't know of: it says busy while the call the app asks about runs.
    pub app_ask: Option<&'a crate::claude::applog::Ask>,
    /// The newest write to the session's subagent logs.
    pub subagent_at: Option<i64>,
    /// The command its shell is running, from the process table: for a quiet busy session, and
    /// for one whose turn left a command running in the background.
    pub running: Option<&'a str>,
    /// The last time the processes of a command left running in the background were seen doing
    /// something (starting processes, using CPU); None when nobody has looked.
    pub background_active_at: Option<i64>,
}

/// A command left running in the background that has done nothing for this long is left
/// sitting there: a dev server, a sign-in waiting on a browser, `tail -f`. Its session's turn
/// is over, and it wakes only if the command ends, so it isn't Working. Polls start a process
/// or use CPU well within this; `sleep 600` loops are the rare exception.
pub const BACKGROUND_QUIET_MS: i64 = 15 * 60 * 1000;
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
    /// The folder an app keeps its chats in, a folder for each (`engine::chat_roots`): listed
    /// like your projects' worktrees, since you started those sessions yourself.
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
    /// The PRs a session was linked to by hand (pr-links.json), whatever its records mark: they go
    /// first where only so many are read (`worked_prs`).
    pub by_hand: HashSet<String>,
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

/// Claude Code sessions whose conversation moved to the background, and the jobs it moved to.
///
/// Moving a conversation to the background starts a job with a session of its own that goes on
/// from it, and the old transcript says where it went (`TranscriptInfo::continued_in`). The job
/// writes nothing until its next prompt, and then starts its transcript with a copy of the
/// conversation; the window it left shows Claude Code's list of agents meanwhile and runs no
/// session (`claude::read_registry`). So the conversation shows once, as the job, which reads
/// the old transcript until it has one of its own.
#[derive(Debug, Default, PartialEq)]
pub struct ClaudeMoves {
    /// A job with no transcript of its own yet, and the session it goes on from.
    pub from: HashMap<String, String>,
    /// A session left behind, and the one it went on in: it doesn't show.
    pub moved: HashMap<String, String>,
}

/// `live` has the sessions a process runs, and `transcripts` the ones whose transcript says
/// something, by id.
pub fn claude_moves(live: &HashSet<&str>, transcripts: &HashMap<&str, &TranscriptInfo>) -> ClaudeMoves {
    let mut out = ClaudeMoves::default();
    for (&id, t) in transcripts {
        let Some(to) = t.continued_in.as_deref() else { continue };
        let talked = transcripts.contains_key(to);
        if !talked {
            out.from.insert(to.to_string(), id.to_string());
        }
        // Taken back to its window it's its own again, and a job that went before a word was
        // said to it leaves the conversation here.
        if !live.contains(id) && (talked || live.contains(to)) {
            out.moved.insert(id.to_string(), to.to_string());
        }
    }
    out
}

/// A job going on from another session's conversation, or a process that came up on its own
/// and hasn't taken a turn yet, dated as that conversation: started when it did, and at rest
/// since its last word, so moving or restarting it doesn't make it new or unread.
pub fn moved_registry(r: &RegistryEntry, from: &TranscriptInfo) -> RegistryEntry {
    let mut r = r.clone();
    r.started_at = from.started_at.or(r.started_at);
    if r.status == "idle" {
        r.status_updated_at = Some(from.active_at());
    }
    r
}

/// Whether a Claude session that says it's idle may have left a command running in the
/// background, which wakes it as it ends. Claude Code says so (`shell`) only from its terminal
/// UI: the Claude app's sessions, the ones Codealong runs over ACP and headless runs say `idle`
/// (2.1.312's code maps their states to busy, waiting and idle alone). Theirs is a shell its
/// transcript says it started since its process did and hasn't heard the end of
/// (`Background::shells`); whether one still runs is the process table's to say
/// (`Quiet::running`).
pub fn untold_shell(r: &RegistryEntry, t: &TranscriptInfo) -> bool {
    r.status == "idle" && r.entrypoint.as_deref() != Some("cli") && t.background.shells.iter().any(|b| r.started_at.map_or(true, |s| b.at >= s))
}

/// What a Claude session its registry says is busy left running in the background with its turn
/// over: the subagents and workflows its transcript says it started since its process did and
/// hasn't heard the end of, what each is for. Claude Code says busy while any of them runs,
/// whether or not a turn does (2.1.322's `delegatedActive`), so only a transcript whose last turn
/// has ended (`TranscriptInfo::ended_at`) says it's them it's busy with. Read as a turn, such a
/// session showed working for as long as its agents ran, its last command for an excerpt, and a
/// reply that asked you something never reached Needs you (2026-11-10). Empty otherwise.
pub fn agents_left_running(r: &RegistryEntry, t: &TranscriptInfo) -> Vec<String> {
    if r.status != "busy" || t.ended_at.is_none() {
        return Vec::new();
    }
    t.background.agents.iter().filter(|a| r.started_at.map_or(true, |s| a.at >= s)).map(|a| a.about.clone()).collect()
}

/// How long a command a session ran in the background stays on its list once it ended
/// (`Session::tasks`): long enough to read how it went.
pub const TASK_ENDED_SHOWN_MS: i64 = 10 * 60 * 1000;

/// What a live Claude session runs in the background, for its list (`Session::tasks`): the
/// shells its transcript says it started since its process did, the running ones first, then
/// those that ended in the last few minutes (`TASK_ENDED_SHOWN_MS`), oldest first in each. One
/// whose process is gone runs none: Claude Code stops them as it exits.
pub fn claude_tasks(r: &RegistryEntry, t: &TranscriptInfo, now: i64) -> Vec<BackgroundTask> {
    let since = |s: &&Shell| r.started_at.map_or(true, |at| s.at >= at);
    let bg = &t.background;
    let running = bg.shells.iter().filter(since).filter(|s| s.ended.is_none());
    let mut ended: Vec<&Shell> = bg.shells.iter().chain(&bg.ended).filter(since).filter(|s| s.ended.as_ref().is_some_and(|e| now - e.at < TASK_ENDED_SHOWN_MS)).collect();
    ended.sort_by_key(|s| s.ended.as_ref().map(|e| e.at));
    running
        .chain(ended)
        .map(|s| BackgroundTask {
            id: s.id.clone(),
            command: s.command.clone(),
            about: s.about.clone(),
            monitor: s.monitor,
            started_at: s.at,
            ended_at: s.ended.as_ref().map(|e| e.at),
            status: s.ended.as_ref().map(|e| e.status.clone()),
            exit_code: s.ended.as_ref().and_then(|e| e.exit_code),
            output: s.output.clone(),
        })
        .collect()
}

/// What a Codex thread runs in background terminals, for its list (`Session::tasks`): the
/// commands its rollout says still run, while a Codex process has the thread loaded (`held`; one
/// nothing holds runs none, its terminals having gone with the process that ran them, which may
/// have written no end for them), then those that ended in the last few minutes
/// (`TASK_ENDED_SHOWN_MS`), oldest first in each.
pub fn codex_tasks(th: &ThreadInfo, held: Option<bool>, now: i64) -> Vec<BackgroundTask> {
    let t = &th.terminals;
    let running = t.running.iter().filter(|_| held != Some(false));
    let ended = t.ended.iter().filter(|e| e.ended.as_ref().is_some_and(|(at, _, _)| now - at < TASK_ENDED_SHOWN_MS));
    running
        .chain(ended)
        .map(|r| BackgroundTask {
            id: r.id.clone(),
            command: r.command.clone(),
            started_at: r.at,
            ended_at: r.ended.as_ref().map(|(at, _, _)| *at),
            status: r.ended.as_ref().map(|(_, status, _)| status.clone()),
            exit_code: r.ended.as_ref().and_then(|(_, _, code)| *code),
            ..BackgroundTask::default()
        })
        .collect()
}

/// A session's excerpt while it waits on the agents its turn left running (`agents_left_running`).
pub fn waiting_on_agents(agents: &[String]) -> String {
    match agents {
        [one] if !one.is_empty() => format!("Waiting on a background agent · {one}"),
        [_] => "Waiting on a background agent".to_string(),
        _ => format!("Waiting on {} background agents", agents.len()),
    }
}

/// Whether processes listed at `listed_at` can say a session's background shells have gone
/// (`untold_shell`): the engine lists them every few seconds off its thread, and one listed
/// before a shell started doesn't have it yet.
pub fn listed_after_shells(listed_at: Option<i64>, t: &TranscriptInfo) -> bool {
    listed_at.is_some_and(|at| t.background.shells.iter().all(|b| b.at <= at))
}

/// How a session's excerpt starts while it holds another session's message for you to allow
/// (`claude::held`): the sender follows.
pub use crate::model::HELD_MESSAGE;

/// A session's excerpt while it waits on your answers to its questions: the first of them, as
/// the windows know it (`lanes.ts` reads the start).
pub fn asks_you(question: &str) -> String {
    format!("Asks you · {}", one_line(question, 160))
}

/// A Claude Code session from its registry entry (when live) and transcript.
pub fn claude_session(
    account: &AccountCfg,
    reg: Option<&RegistryEntry>,
    tr: Option<&TranscriptInfo>,
    seen_at: i64,
    now: i64,
    quiet: &Quiet,
) -> Option<Session> {
    let id = reg.map(|r| r.session_id.clone()).or_else(|| tr.map(|t| t.session_id.clone()))?;
    let cwd = reg.map(|r| r.cwd.clone()).or_else(|| tr.map(|t| t.cwd.clone()))?;
    // One with a command still running in the background is as its terminal would say.
    let untold = reg
        .zip(tr)
        .filter(|(r, t)| untold_shell(r, t) && quiet.running.is_some())
        .map(|(r, _)| RegistryEntry { status: "shell".into(), ..r.clone() });
    let reg = untold.as_ref().or(reg);
    // A process that came up after its transcript's last line and has been idle since has said
    // nothing new: Claude Code's background service restarting its jobs on a new version after
    // an update, or a `--resume` nobody has typed into yet. Its fresh registry times aren't a
    // turn that ended.
    let restarted = reg
        .zip(tr)
        .filter(|(r, t)| r.status == "idle" && r.started_at.is_some_and(|s| s > t.last_ts))
        .map(|(r, t)| moved_registry(r, t));
    // Nor has one that has only compacted its conversation since its last turn
    // (`TranscriptInfo::compacted`): it came back as Your turn an hour after you'd seen it.
    let compacted = reg
        .zip(tr)
        .filter(|(r, t)| matches!(r.status.as_str(), "idle" | "shell") && t.compacted.is_some())
        .map(|(r, t)| RegistryEntry { status_updated_at: Some(t.active_at()), ..r.clone() });
    let reg = restarted.as_ref().or(compacted.as_ref()).or(reg);
    // Busy only with the agents its turn left running: it waits on them, as on a shell.
    let agents = reg.zip(tr).map(|(r, t)| agents_left_running(r, t)).unwrap_or_default();
    // An approval up in the Claude app is what a busy session waits on, however long it's quiet.
    let app_ask = quiet.app_ask.filter(|_| reg.is_some_and(|r| r.status == "busy"));
    // The registry doesn't refresh while a session is busy; its transcript, status changes and
    // subagent logs do. Stuck mid-turn, it can't go on without you; with its turn over, it isn't
    // mid-turn.
    let stalled_since = claude_busy_since(reg, tr)
        .map(|t| t.max(quiet.subagent_at.unwrap_or(0)))
        .filter(|t| now - t > CLAUDE_STALL_MS && app_ask.is_none() && agents.is_empty());
    let background = reg.is_some_and(|r| r.status == "shell");
    let left_running = background && quiet.background_active_at.is_some_and(|t| now - t >= BACKGROUND_QUIET_MS);
    let status = match reg {
        // Its process ended, closed or with the Mac restarting, but a turn you haven't seen is
        // still yours to read for a while.
        None if tr.map(TranscriptInfo::active_at).is_some_and(|t| t > seen_at && now - t < CLAUDE_UNSEEN_STOPPED_MS) => Status::Unread,
        None => Status::Stopped,
        Some(_) if stalled_since.is_some() || app_ask.is_some() => Status::Needs,
        Some(r) => match r.status.as_str() {
            "shell" if !left_running => Status::Working,
            "busy" => Status::Working,
            "waiting" | "blocked" => Status::Needs,
            "idle" | "shell" if r.status_updated_at.unwrap_or(0) > seen_at => Status::Unread,
            _ => Status::Idle,
        },
    };
    // A turn the account's limit refused is stopped, not done: what to do about it waits on you.
    let limit = tr.and_then(|t| t.limit.clone()).filter(|_| status != Status::Working);
    // The same for a turn a lost connection ended; whether Codealong carries it on is the
    // engine's to say (`engine::with_connections`).
    let connection_lost = tr.and_then(|t| t.connection_lost).filter(|_| status != Status::Working).map(|at| ConnectionLost { at, continues: false });
    // What went wrong, for a window to show and offer to try again.
    let api_error = tr.and_then(|t| t.api_error.clone()).filter(|_| status != Status::Working);
    // Its process went away partway through a turn: its terminal closed, the Mac restarted, it
    // was killed. Read only for terminal sessions (background jobs too), whose turns were checked
    // to end the ways `transcript::turn_step` knows; the Claude app restarts its own sessions.
    // A conversation that moved to the background goes on there.
    let cut_off = tr
        .filter(|t| reg.is_none() && t.entrypoint.as_deref() == Some("cli") && t.continued_in.is_none())
        .and_then(|t| t.unfinished);
    let status = if (limit.is_some() || cut_off.is_some()) && status == Status::Unread { Status::Error } else { status };
    let entrypoint = reg.and_then(|r| r.entrypoint.clone()).or_else(|| tr.and_then(|t| t.entrypoint.clone())).unwrap_or_default();
    let surface = if entrypoint.contains("desktop") { Surface::App } else { Surface::Cli };
    // Claude Code calls a run with no terminal at it `sdk-cli` (`-p`, or its output piped), and
    // leaves those out of its own /resume list with the Agent SDK's (`sdk-ts`, `sdk-py`). The
    // SDK's are often front ends someone types into, as Codealong's own sessions are, so only
    // the command line's print runs are headless. One resumed in a terminal later is headless
    // again once that ends: the transcript says how it began.
    let headless = entrypoint == "sdk-cli";
    let user_named = reg
        .filter(|r| matches!(r.name_source.as_deref(), Some("user") | Some("peer")))
        .and_then(|r| r.name.clone());
    // The order Claude Code titles its terminal tab in, so rows and tabs read the same.
    let title = tr
        .and_then(|t| t.title.clone().or_else(|| t.agent_name.clone()))
        .or(user_named)
        .or_else(|| tr.and_then(|t| t.ai_title.clone()))
        .or_else(|| tr.and_then(|t| t.first_prompt.clone()))
        .or_else(|| reg.and_then(|r| r.name.clone()))
        .unwrap_or_else(|| format!("Session {}", short_id(&id)));
    let last_activity = [
        tr.map(TranscriptInfo::active_at),
        reg.and_then(|r| r.status_updated_at),
        reg.and_then(|r| r.started_at),
    ]
    .into_iter()
    .flatten()
    .max()
    .unwrap_or(0);
    let excerpt = match status {
        _ if stalled_since.is_some() => {
            let t = stalled_since.unwrap_or(0);
            Some(match quiet.running {
                Some(cmd) => format!("Stuck on {} for {{for:{t}}}", code(&truncate(&one_line(cmd, 200), 80))),
                None => format!("No activity for {{for:{t}}}"),
            })
        }
        Status::Needs if app_ask.is_some() => app_ask.map(crate::claude::applog::Ask::label),
        // A background job's record says what it asks, which its transcript may not have yet.
        Status::Needs if reg.is_some_and(|r| r.asks.is_some()) => reg.and_then(|r| r.asks.as_deref()).map(asks_you),
        // A message held since its last call was made is what it waits on: a turn goes on under
        // its dialog, so the call may be one still running.
        Status::Needs => tr.and_then(|t| match &t.held {
            Some(h) if t.pending_tool_at.map_or(true, |at| at <= h.at) => Some(format!("{HELD_MESSAGE}{}", h.from)),
            _ => t.pending_tool.clone(),
        }),
        _ if left_running => Some(match quiet.running {
            Some(cmd) => format!("Left running · {}", code(&truncate(&one_line(cmd, 200), 80))),
            None => "Left a command running".to_string(),
        }),
        Status::Working if !agents.is_empty() => Some(waiting_on_agents(&agents)),
        // Its turn is over: it waits on the command (a CI watch, a build) to wake it, which
        // "working" alone hid behind a spinner (asked 2026-10-07).
        Status::Working if background => Some(match quiet.running {
            Some(cmd) => format!("Waiting on {}", code(&truncate(&one_line(cmd, 200), 80))),
            None => "Waiting on a command in the background".to_string(),
        }),
        // Its latest call, else the prompt it took and hasn't made one for yet: waiting on the
        // model, which said "running" the last turn's finished command for as long as the API
        // took to answer (2026-10-15).
        Status::Working => tr.and_then(|t| t.last_tool.clone().or_else(|| t.last_prompt.clone())),
        // Where it was when it stopped.
        _ if cut_off.is_some() => tr.and_then(|t| t.last_tool.clone().or_else(|| t.last_prompt.clone())),
        _ => tr.and_then(|t| t.last_text.clone().or_else(|| t.last_prompt.clone())),
    };
    let attach_command = reg
        .filter(|r| r.kind.as_deref() == Some("bg"))
        .and_then(|r| r.job_id.clone())
        .map(|job| format!("{} attach {job}", account.command()));
    let action = match (surface, reg.is_some()) {
        (Surface::App, _) => "claude-app",
        (Surface::Cli, true) if attach_command.is_some() => "attach",
        (Surface::Cli, true) => "focus",
        (Surface::Cli, false) => "resume",
    };
    // What Claude Code holds for the turn it's taking, which only a turn that's running has.
    let held = tr.filter(|_| reg.is_some_and(|r| matches!(r.status.as_str(), "busy" | "waiting"))).map(TranscriptInfo::held).unwrap_or_default();
    let known = Session::new(Harness::Claude, &account.name, &id);
    Some(Session {
        open_target: format!("s|{}|{action}", known.key),
        resume_command: Some(format!("{} --resume {}", account.command(), id)),
        surface,
        status,
        live: reg.is_some(),
        pid: reg.map(|r| r.pid),
        title: truncate(&plain(&title), 90),
        cwd,
        work: tr.map(|t| t.work.clone()).unwrap_or_default(),
        reply_tail: tr.and_then(|t| t.last_text_tail.clone()),
        reply_head: tr.and_then(|t| t.last_text.clone()),
        first_prompt: tr.and_then(|t| t.first_prompt.clone()),
        last_prompt: tr.and_then(|t| t.last_prompt.clone()),
        name: reg.and_then(|r| r.name.clone()),
        started_at: reg.and_then(|r| r.started_at).or_else(|| tr.and_then(|t| t.started_at)),
        // Waiting on the app since it asked, however long the turn has run; on its agents since
        // its turn ended, which the registry, busy since the turn began, doesn't say.
        status_since: app_ask.map(|a| a.at).or_else(|| tr.and_then(|t| t.ended_at).filter(|_| !agents.is_empty())).or_else(|| reg.and_then(|r| r.status_updated_at)),
        last_prompt_at: tr
            .and_then(|t| t.last_prompt_at)
            .or_else(|| reg.and_then(|r| r.started_at))
            .or_else(|| tr.and_then(|t| t.started_at))
            .unwrap_or(last_activity),
        last_activity,
        turn_ended: tr.and_then(|t| t.ended_at),
        input_at: tr.and_then(|t| t.input_at),
        excerpt,
        branch: tr.and_then(|t| t.git_branch.clone()),
        attach_command,
        remote_control: reg.and_then(|r| r.remote_control.clone()),
        stalled_since,
        background: background.then(|| quiet.running.unwrap_or_default().to_string()),
        agents,
        tasks: reg.zip(tr).map(|(r, t)| claude_tasks(r, t, now)).unwrap_or_default(),
        app_account: account.home_path() == crate::util::home().join(".claude"),
        limit,
        connection_lost,
        api_error,
        cut_off,
        flagged: tr.is_some_and(|t| t.flagged),
        model: tr.and_then(|t| t.model.clone()),
        effort: tr.and_then(|t| t.effort.clone()),
        mode: tr.and_then(|t| t.permission_mode.as_deref()).and_then(crate::claude::session_mode),
        context: tr.and_then(|t| t.context).map(|used| ContextUse { used, window: tr.and_then(|t| t.window_model.as_deref().or(t.model.as_deref())).and_then(crate::prices::context_window) }),
        // Held only while it runs: the lock and the cleanup at exit are its process's.
        own_worktree: tr.and_then(|t| t.own_worktree.clone()).filter(|_| reg.is_some()),
        color: tr.and_then(|t| t.color.as_deref()).and_then(crate::emblems::harness_color),
        headless,
        queued_at: held.iter().map(|m| m.at).max(),
        queued: held,
        ..known
    })
}

/// A tool call in an open Codex turn that has gone unanswered for longer than its tool takes:
/// the app is almost certainly waiting for you to approve it, and doesn't record that. A day
/// on, the app has likely quit without closing the turn.
fn codex_unanswered(th: &ThreadInfo, now: i64) -> Option<String> {
    let call = th.pending_calls.iter().find(|c| now - c.at > c.limit_ms)?;
    (th.turn_open && now - th.last_ts < DAY_MS).then(|| format!("Waiting for approval? · {}", call.what))
}

/// Where a Codex thread came from: a fork of a thread, or made by a thread's agent.
fn codex_origin(account: &AccountCfg, th: &ThreadInfo) -> Option<SessionOrigin> {
    let (id, how) = match (&th.forked_from, &th.started_by) {
        (Some(id), _) => (id, OriginHow::Fork),
        (None, Some(id)) => (id, OriginHow::Started),
        (None, None) => return None,
    };
    (id != &th.id).then(|| SessionOrigin { key: Session::key_for(Harness::Codex, &account.name, id), how })
}

/// A Codex thread. Threads have no process of their own, so they are never Stopped. `held` is
/// whether a Codex process has its rollout open (`codex/terminals.rs` `held_rollouts`), None
/// when that isn't known, and `quit` whether its last turn was stopped as the app quit
/// (`codex/applog.rs`) rather than by you. `asked` is the approval Codex's hook handed over that
/// its turn waits on (codex/asks.rs `waiting`): what it would do, and when it asked.
pub fn codex_session(account: &AccountCfg, th: &ThreadInfo, name: Option<&str>, terminal: Option<u32>, held: Option<bool>, quit: bool, seen_at: i64, now: i64, asked: Option<(String, i64)>) -> Session {
    let fresh = now - th.last_ts < CODEX_STALE_MS;
    // The app runs the threads of `~/.codex` only.
    let app_account = account.home_path() == crate::util::home().join(".codex");
    // A turn left open in a rollout no Codex process holds was cut off: the terminal UI, the app
    // or the agent running it went away partway through, and nothing will close it. So was one
    // the app stopped as it quit (the Mac shutting down or restarting, an update), though Codex
    // closes it as it closes one you stop; a thread a terminal UI has open isn't the app's.
    let by_the_app = quit && app_account && terminal.is_none();
    let cut_off = if th.turn_open && terminal.is_none() && held == Some(false) { Some(th.last_ts) } else { th.interrupted_at.filter(|_| by_the_app) };
    let unanswered = codex_unanswered(th, now).filter(|_| cut_off.is_none());
    let asked = asked.filter(|_| cut_off.is_none());
    let status = if let Some(at) = cut_off {
        if at > seen_at { Status::Error } else { Status::Idle }
    } else if asked.is_some() || th.pending_approval.is_some() || unanswered.is_some() {
        Status::Needs
    } else if th.turn_open && fresh {
        Status::Working
    } else if th.last_complete_at.map(|t| t > seen_at).unwrap_or(false) {
        Status::Unread
    } else {
        Status::Idle
    };
    // A thread Codex hasn't named (21 of the 83 Codealong made here, 2026-10-19) goes by its first
    // prompt, as the app shows one and as a Claude Code session without a title goes. Its latest
    // was often a message Codealong sent it.
    let title = name
        .map(String::from)
        .or_else(|| th.first_prompt.clone())
        .or_else(|| th.last_user_message.clone())
        .unwrap_or_else(|| "Codex thread".to_string());
    let excerpt = match status {
        // Known from Codex's hook, else guessed from a call left without its result.
        Status::Needs => asked.as_ref().map(|(what, _)| format!("Waiting for approval · {what}")).or_else(|| th.pending_approval.clone()).or(unanswered),
        // What the turn was on.
        Status::Working => th.last_user_message.clone(),
        _ if cut_off.is_some() => th.last_user_message.clone(),
        _ => th.last_agent_message.clone(),
    };
    // A thread shows where it's open: its terminal, else where it was made. `codex resume`
    // opens any of them in a terminal.
    let surface = if th.cli || terminal.is_some() { Surface::Cli } else { Surface::App };
    let action = match (surface, terminal) {
        (Surface::App, _) => "codex",
        (Surface::Cli, Some(_)) => "focus",
        (Surface::Cli, None) => "resume",
    };
    let known = Session::new(Harness::Codex, &account.name, &th.id);
    Session {
        open_target: format!("s|{}|{action}", known.key),
        surface,
        status,
        live: (th.turn_open && fresh && cut_off.is_none()) || terminal.is_some(),
        pid: terminal,
        title: truncate(&plain(&title), 90),
        cwd: th.cwd.clone(),
        work: th.work.clone(),
        reply_tail: th.last_agent_tail.clone(),
        reply_head: th.last_agent_message.clone(),
        first_prompt: th.first_prompt.clone(),
        last_prompt: th.last_user_message.clone(),
        started_at: th.started_at,
        // Waiting on you since its oldest unanswered call, not since the turn before it ended:
        // Needs you is a queue by that time.
        status_since: match status {
            _ if cut_off.is_some() => cut_off,
            Status::Working => th.turn_started_at,
            Status::Needs => asked.as_ref().map(|(_, at)| *at).or_else(|| th.pending_calls.iter().map(|c| c.at).min()).or(Some(th.last_ts)),
            _ => th.last_complete_at,
        },
        last_prompt_at: th.last_prompt_at.or(th.started_at).unwrap_or(th.last_ts),
        last_activity: th.last_ts,
        excerpt,
        resume_command: Some(format!("{} resume {}", account.command(), th.id)),
        app_account,
        branch: th.branch.clone(),
        origin: codex_origin(account, th),
        flagged: th.flagged,
        model: th.model.clone(),
        effort: th.effort.clone(),
        mode: th.mode.clone(),
        context: th.context.map(|(used, window)| ContextUse { used, window: window.or_else(|| th.model.as_deref().and_then(crate::prices::context_window)) }),
        cut_off,
        // `codex resume` goes on in the same rollout, which still says `exec`: someone is at it
        // while a terminal has it, and it's headless again once that ends, as Claude's is.
        headless: th.exec && terminal.is_none(),
        tasks: codex_tasks(th, held, now),
        ..known
    }
}

/// An OpenCode session. `live` is what its background service says, when the service is
/// running; without it, a prompt after the last run ended means a run in progress, until the
/// session goes quiet. `terminal` is the interactive OpenCode showing it, if one is.
pub fn opencode_session(account: &AccountCfg, row: &SessionRow, live: Option<&Live>, terminal: Option<u32>, seen_at: i64, now: i64) -> Session {
    let running = match live {
        Some(l) => l.active.contains(&row.id),
        None => row.turn_open() && now - row.last_ts < OPENCODE_STALE_MS,
    };
    let pending = live.and_then(|l| l.pending.get(&row.id)).cloned();
    let asked_since = pending.as_ref().map(|p| p.since);
    // A run's end is news until you open the session here or look at it in OpenCode.
    let unseen = row.idle_at.filter(|_| !running).map(|t| t > seen_at && row.viewed_at.map(|v| v < t).unwrap_or(true)).unwrap_or(false);
    let status = if pending.is_some() {
        Status::Needs
    } else if running {
        Status::Working
    } else if unseen && row.outcome.as_deref() == Some("failed") {
        Status::Error
    } else if unseen {
        Status::Unread
    } else {
        Status::Idle
    };
    let title = row.title.clone().or_else(|| row.last_prompt.clone()).unwrap_or_else(|| "OpenCode session".to_string());
    let excerpt = match status {
        Status::Needs => pending.map(|p| p.ask),
        Status::Working => row.last_tool.clone().or_else(|| row.last_prompt.clone()),
        _ => row.last_text.clone().or_else(|| row.last_prompt.clone()),
    };
    let action = if terminal.is_some() { "focus" } else { "resume" };
    let known = Session::new(Harness::Opencode, &account.name, &row.id);
    Session {
        open_target: format!("s|{}|{action}", known.key),
        // Every id starts `ses_`, which says nothing.
        short_id: short_id(row.id.trim_start_matches("ses_")),
        status,
        live: running || terminal.is_some(),
        pid: terminal,
        title: truncate(&plain(&title), 90),
        cwd: row.directory.clone(),
        reply_tail: row.last_text_tail.clone(),
        reply_head: row.last_text.clone(),
        last_prompt: row.last_prompt.clone(),
        started_at: Some(row.created_at),
        // Waiting on you since it asked what it asks now (`Asked::since`): each request of a run
        // has a time of its own, so a window that answered one can tell the next from it (ahead.ts).
        status_since: if status == Status::Needs { asked_since } else if running { row.last_prompt_at } else { row.idle_at },
        last_prompt_at: row.last_prompt_at.unwrap_or(row.created_at),
        last_activity: row.last_ts,
        excerpt,
        resume_command: Some(format!("{} --session {}", account.command(), row.id)),
        archived: row.archived_at.is_some(),
        origin: row.fork_of.as_ref().map(|id| SessionOrigin { key: Session::key_for(Harness::Opencode, &account.name, id), how: OriginHow::Fork }),
        context: row.context.as_ref().map(|(used, model)| ContextUse { used: *used, window: model.as_deref().and_then(crate::prices::context_window) }),
        mode: row.agent.as_deref().map(crate::opencode::agent_mode),
        model: row.model.clone(),
        effort: row.effort.clone(),
        ..known
    }
}

/// An Antigravity conversation, from its summary and the end of its record. `process` is the
/// command line process that has it open, when one does, and `app_running` whether the hub
/// app's agent is up. A turn is under way only while what runs it is there: the command line's
/// conversation in its own process, the app's in the app's agent, which is taken to have gone
/// quiet when nothing has been written for a while. A step whose permission prompt is up needs
/// you. A turn that ended on an error says why, by the note it ended with: a lost connection
/// (which the engine may carry on, `engine::with_connections`), the account's quota, or anything
/// else, which shows as an error until it's seen.
pub fn antigravity_session(account: &AccountCfg, c: &Conversation, process: Option<&Process>, app_running: bool, seen_at: i64, now: i64) -> Session {
    // Its latest step can be later than its summary says: another `agy` writes back older copies
    // of the row. The note a failed turn ended with is its latest step, read whether or not a
    // process has it.
    let active = [c.latest_step_at, c.failed.as_ref().map(|f| f.at)].into_iter().flatten().fold(c.last_modified, i64::max);
    let fresh = now - active < ANTIGRAVITY_STALE_MS;
    // While its process has the conversation, its summary is the process's word, unless another
    // `agy` that started since marked it idle; a step that runs or waits says so too, and a
    // terminal UI's log. A print run writes its summary only as it ends, so its record says
    // whether a turn is under way.
    let under_way = match (c.cli, process) {
        (true, Some(p)) => c.running() || c.idled_by_another || c.step_under_way || (!p.terminal && c.tail.turn_open) || c.ui_turn_open(),
        (true, None) => false,
        (false, _) => c.running() && app_running,
    };
    let news = active > seen_at;
    let status = if c.killed {
        Status::Idle
    } else if under_way && c.waiting {
        Status::Needs
    } else if under_way && (c.cli || fresh) {
        Status::Working
    } else if news && active > c.last_input.unwrap_or(0) {
        Status::Unread
    } else {
        Status::Idle
    };
    let failed = c.failed.as_ref().filter(|_| !matches!(status, Status::Working | Status::Needs));
    let limit = failed.and_then(|f| match f.on {
        FailedOn::Quota { resets_at } => Some(SessionLimit { at: f.at, resets_at, ..SessionLimit::default() }),
        _ => None,
    });
    let connection_lost = failed.filter(|f| f.on == FailedOn::Connection).map(|f| ConnectionLost { at: f.at, continues: false });
    // What went wrong, but for a quota's refusal, which is its limit.
    let api_error = failed.filter(|f| !matches!(f.on, FailedOn::Quota { .. })).map(|f| ApiError { at: f.at, said: crate::antigravity::api_error_words(&f.said).to_string() });
    // A lost connection reads as a finished turn, as Claude Code's does: it says so beside it.
    let status = if failed.is_some_and(|f| f.on != FailedOn::Connection) && status == Status::Unread { Status::Error } else { status };
    let title = c.title.clone().or_else(|| c.preview.clone()).unwrap_or_else(|| "Antigravity conversation".to_string());
    let excerpt = match status {
        Status::Needs => c.tail.open.first().map(|call| call.asked()).or_else(|| Some("A step waits for you to allow it".to_string())),
        Status::Working => c.tail.last_tool.clone().or_else(|| c.preview.clone()),
        // What went wrong, where the reply before it would read as the turn's end.
        _ => failed.map(|f| f.said.clone()).or_else(|| c.tail.last_text.clone()).or_else(|| c.preview.clone()),
    };
    // Its terminal, when it's the command line's own UI rather than a print run.
    let terminal = process.filter(|p| p.terminal).map(|p| p.pid);
    let action = match (c.cli, terminal) {
        (false, _) => "antigravity",
        (true, Some(_)) => "focus",
        (true, None) => "resume",
    };
    // The agent mode its terminal UI says it's in, where one has it open, or in the default one
    // what the tool permission asks: as the latest turn that went in that mode ran with it, else
    // as that UI started with it. And the modes Shift+Tab steps that UI through, which Codealong
    // can press there (cycle.rs). Otherwise the permissions its latest turn ran with, which the
    // agent mode it ran in went into.
    let agent = process.and(c.said.agent_mode.as_deref());
    let default = match agent {
        Some(_) if c.said.prompted_in_default(&c.id) && c.mode.is_some() => c.mode.clone(),
        Some(_) => c.said.default_turn.clone().or_else(|| c.said.tool_permission.as_deref().and_then(crate::antigravity::permission_mode)),
        None => c.mode.clone(),
    };
    let modes = match (terminal, agent) {
        (Some(_), Some(_)) => crate::antigravity::cycled_modes(default.as_ref()),
        _ => Vec::new(),
    };
    let mode = crate::antigravity::session_mode(default.as_ref(), agent).map(|m| match agent {
        Some(id) if !modes.is_empty() => m.with_id(id),
        _ => m,
    });
    let known = Session::new(Harness::Antigravity, &account.name, &c.id);
    Session {
        open_target: format!("s|{}|{action}", known.key),
        surface: if c.cli { Surface::Cli } else { Surface::App },
        status,
        live: matches!(status, Status::Working | Status::Needs) || process.is_some(),
        pid: terminal,
        title: truncate(&plain(&title), 90),
        cwd: c.cwd.clone().unwrap_or_default(),
        work: c.tail.work.clone(),
        reply_tail: c.tail.last_text_tail.clone(),
        reply_head: c.tail.last_text.clone(),
        // From its record: the preview is the conversation's title from agy 1.4 on.
        first_prompt: c.asked.clone().or_else(|| c.preview.clone()),
        status_since: if status == Status::Working { c.last_input } else { Some(active) },
        last_prompt_at: c.last_input.unwrap_or(active),
        last_activity: active,
        excerpt,
        resume_command: c.cli.then(|| format!("{} --conversation {}", crate::antigravity::cli_command(account), c.id)),
        mode,
        modes,
        // Its level is part of the model's name, as `--model` takes it.
        model: c.model.clone(),
        queued_at: process.and_then(|_| c.said.queued.last().copied()),
        limit,
        connection_lost,
        api_error,
        ..known
    }
}

/// Whether the turn a command line chat's transcript leaves open is one `p`, the process that has
/// the chat, is taking. One left open before that process began isn't: what ran it ended partway
/// (its terminal closed on it), so its end was never written, and the chat was resumed since,
/// where it waits at its box. Resuming writes the chat's database but not its transcript, and a
/// reviewer resumed so read as working for as long as its tab stayed open (2026-11-10). A turn of
/// the process's own is in the transcript within moments of its prompt, and one it was started
/// on counts until then, for as long as a start takes.
pub fn cursor_turn_under_way(c: &crate::cursor::Chat, p: &crate::cursor::Process, now: i64) -> bool {
    if !c.tail.turn_open {
        return false;
    }
    match p.started.zip(c.transcript_at) {
        Some((started, written)) => written >= started || (p.prompt.is_some() && now - started < CURSOR_STARTING_MS),
        None => true,
    }
}

/// A Cursor chat, from its record and the end of its messages. `process` is the command line
/// process that has a command line chat open, when one does, and `app_running` whether the app
/// is. The app's chat is under way while the app says it's generating and the app is there to
/// do it: one generating when the app quit says so until the app opens it again. Something to
/// allow or answer needs you; the command line writes nothing down while its prompt is up, so
/// its chats never show so, but one that hasn't written its chat a while after it started waits
/// on what it asks first (`Chat::unwritten`). A turn that ended on an error shows as one until
/// it's seen.
/// `asked`: the permission prompt a command line chat's terminal shows, which nothing else says
/// (`cursor::Asked`).
pub fn cursor_session(account: &AccountCfg, c: &crate::cursor::Chat, process: Option<&crate::cursor::Process>, asked: Option<&crate::cursor::Asked>, app_running: bool, seen_at: i64, now: i64) -> Session {
    let under_way = match (c.cli, process) {
        // Its database comes a few seconds before its transcript has the prompt it was started
        // on, which it's taking a turn on all the same: for as long as a start takes, at most.
        (true, Some(p)) => cursor_turn_under_way(c, p, now) || !p.terminal || c.unwritten || (p.prompt.is_some() && c.tail.first_prompt.is_none() && now - c.created_at < CURSOR_STARTING_MS),
        (true, None) => false,
        (false, _) => c.generating() && app_running && now - c.last_updated < CURSOR_STALE_MS,
    };
    let held_up = c.unwritten && process.is_some_and(|p| p.terminal) && now - c.created_at >= CURSOR_STARTING_MS;
    let asked = asked.filter(|_| c.cli && under_way);
    let waiting = held_up || asked.is_some() || under_way && !c.cli && (c.blocking || c.tail.open.iter().any(|call| call.waits));
    let news = c.last_updated > seen_at;
    let status = if waiting {
        Status::Needs
    } else if under_way {
        Status::Working
    } else if news && c.tail.error.is_some() {
        Status::Error
    } else if news && c.last_updated > c.tail.last_prompt_at.unwrap_or(0) {
        Status::Unread
    } else {
        Status::Idle
    };
    let title = c.title.clone().or_else(|| c.tail.first_prompt.clone()).unwrap_or_else(|| "Cursor chat".to_string());
    let excerpt = match status {
        Status::Needs if held_up => Some("Cursor asks something in its terminal before it starts".to_string()),
        Status::Needs if asked.is_some() => asked.map(|a| a.label()),
        Status::Needs => c.tail.open.iter().find(|call| call.waits).or(c.tail.open.first()).map(|call| call.label.clone()).or_else(|| Some("Something waits for you in Cursor".to_string())),
        Status::Working => c.tail.last_tool.clone().or_else(|| c.tail.first_prompt.clone()),
        Status::Error => c.tail.error.clone(),
        _ => c.tail.last_text.clone().or_else(|| c.tail.first_prompt.clone()),
    };
    // Its terminal, when it's the command line's own UI rather than a print run.
    let terminal = process.filter(|p| p.terminal).map(|p| p.pid);
    let action = match (c.cli, terminal) {
        (false, _) => "cursor",
        (true, Some(_)) => "focus",
        (true, None) => "resume",
    };
    let known = Session::new(Harness::Cursor, &account.name, &c.id);
    let last_prompt_at = c.tail.last_prompt_at.unwrap_or(c.created_at);
    Session {
        open_target: format!("s|{}|{action}", known.key),
        surface: if c.cli { Surface::Cli } else { Surface::App },
        status,
        live: under_way || process.is_some(),
        pid: terminal,
        title: truncate(&plain(&title), 90),
        cwd: c.cwd.clone().unwrap_or_default(),
        work: c.tail.work.clone(),
        reply_tail: c.tail.last_text_tail.clone(),
        reply_head: c.tail.last_text.clone(),
        first_prompt: c.tail.first_prompt.clone(),
        started_at: Some(c.created_at).filter(|t| *t > 0),
        status_since: if under_way { Some(last_prompt_at) } else { Some(c.said_at()) },
        last_prompt_at,
        last_activity: c.last_updated,
        excerpt,
        branch: c.branch.clone(),
        model: c.model.clone(),
        effort: c.effort.clone(),
        archived: c.archived,
        resume_command: (c.cli && !c.unwritten).then(|| format!("{} --resume {}", crate::cursor::cli_command(account), c.id)),
        ..known
    }
}

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

/// The session that runs as your orchestrator, for talking to it (dictation.rs): one of this
/// Mac's that was started on the orchestrator's prompt, or failing that is called an
/// orchestrator, and isn't archived or some session's helper. A running one before a stopped one,
/// then the latest to do anything.
pub fn orchestrator(lanes: &[Lane]) -> Option<&Session> {
    let started_as = |s: &Session| s.first_prompt.as_deref().is_some_and(|p| p.trim_start().starts_with(ORCHESTRATES));
    let called = |s: &Session| s.name.as_deref().unwrap_or(&s.title).to_lowercase().contains("orchestrator");
    lanes
        .iter()
        .flat_map(|l| &l.sessions)
        .filter(|s| !s.archived && s.helper.is_none() && s.visitor.is_none() && (started_as(s) || called(s)))
        .max_by_key(|s| (s.live, started_as(s), s.last_activity))
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

/// Whether a checkout's remotes (`LaneSeed::head_owners`) include a PR's head owner. GitHub
/// names owners in its own case, and a remote's URL in whatever case it was typed.
fn has_owner(owners: &[String], owner: &str) -> bool {
    owners.iter().any(|o| o.eq_ignore_ascii_case(owner))
}

/// The PR for a lane: an open one on its branch, else one merged or closed in the last
/// seven days whose head is still the lane's HEAD.
pub fn find_pr(seed: &LaneSeed, prs: &HashMap<String, Vec<PrRaw>>, now: i64) -> Option<Pr> {
    let slug = seed.repo_slug.as_ref()?;
    let branch = seed.branch.as_ref()?;
    if seed.default_branch.as_ref() == Some(branch) {
        return None;
    }
    let list = prs.get(slug)?;
    let ours = |p: &&PrRaw| &p.head_ref == branch && p.head_owner.as_ref().map_or(true, |o| has_owner(&seed.head_owners, o));
    let open = list.iter().filter(ours).filter(|p| p.state.is_open()).max_by_key(|p| p.updated_at);
    let pick = open.or_else(|| {
        list.iter()
            .filter(ours)
            .filter(|p| !p.state.is_open())
            .filter(|p| p.merged_at.or(p.closed_at).map(|t| now - t < 7 * DAY_MS).unwrap_or(false))
            .filter(|p| match (&p.head_oid, &seed.head) {
                (Some(pr_head), Some(lane_head)) => pr_head == lane_head,
                _ => true,
            })
            .max_by_key(|p| p.merged_at.or(p.closed_at))
    })?;
    Some(Pr {
        number: pick.number,
        title: pick.title.clone(),
        url: pick.url.clone(),
        state: pick.state,
        checks: pick.checks.clone(),
        merge: pick.merge,
        updated_at: pick.updated_at,
        merged_at: pick.merged_at,
        closed_at: pick.closed_at,
        stale_approval: pick.stale_approval.clone(),
    })
}

/// Every PR in a repo a project covers, with the worktrees that hold it and the sessions that
/// worked on it.
///
/// A worktree holds the PR its heading in Sessions shows (`find_pr`), so a long-lived branch
/// that carries one PR after another holds only the latest.
///
/// A session worked on the PR if it was on the PR's branch while the PR was open: active after
/// the PR opened (give or take an hour) and started before it merged or closed. A session
/// placed in a worktree it doesn't run in (`session_seed`) goes by that worktree's branch. A session from
/// another lane records its own folder's branch, so it counts through a worktree on the PR's
/// branch that it made or worked in: the one that made it whenever it started before the PR
/// ended, since it wrote the branch even if the PR was opened later, and one that worked there
/// in the same window as the rest. A PR from the default branch (a fork's `main`) links no
/// sessions by branch: every session on `main` would match. Nor does a PR from a fork the
/// worktree's checkout has no remote for, to it or its sessions' pushes: its branch is another
/// of the same name (someone's `app3` beside yours), as `find_pr` has it.
///
/// So do sessions in the repo whose records show they pushed to the PR's branch while they
/// worked in the PR's time (`links.pushed`): one in a worktree on a branch of its own that
/// pushes it to the PR's (`git push origin local:<head>`) has none of the rest. A branch's name
/// says nothing of its repo, so only sessions in the PR's repo count.
///
/// Then come the sessions whose records show they made or changed the PR, or that were linked
/// to it by hand (`links.worked`), whatever their branch or time, each under the lane it's in.
/// A session unlinked by hand is left off. One of them is the PR's primary (`pr_primary`).
///
/// A repo whose PRs aren't read (`polled_slugs`) lists the PRs GitHub's search says wait on your
/// review (`requested`), and the search has the last word on reviews there: a PR it no longer
/// lists doesn't wait on you, though an earlier read of the repo said so. A repo whose PRs are
/// read keeps its read's word on the PRs it lists, and lists the search's it doesn't: past the
/// hundred most recently updated that a read holds, in a busy repo, or any while no read of it
/// has come (its first not yet, or every one failing).
///
/// Then the PRs sessions made or changed in repos whose PRs aren't read, read one by one
/// (`made`, `worked_prs`), so a PR an agent opened anywhere is listed with it: in its project's
/// repo where one covers it, else under the project of the session it's for, the primary's, or
/// the first that's on the board (`made_item`).
pub fn pr_items(seeds: &[LaneSeed], lanes: &[Lane], prs: &HashMap<String, Vec<PrRaw>>, requested: &HashMap<String, Vec<PrRaw>>, made: &HashMap<String, Vec<PrRaw>>, login: Option<&str>, links: &PrLinks) -> Vec<PrItem> {
    let repos = repos_of(seeds);
    let owners = owners_of(seeds);
    let polled = polled_slugs(seeds);
    let read_alone: HashSet<String> = made.iter().flat_map(|(slug, list)| list.iter().map(move |p| crate::prlinks::pr_key(&format!("{slug}#{}", p.number)))).collect();
    let mut out = Vec::new();
    for (slug, list) in prs {
        let Some(&repo) = repos.get(slug.as_str()) else { continue };
        let searched = (!polled.contains(slug)).then(|| requested.get(slug).map_or(&[][..], Vec::as_slice));
        for p in list {
            // The search's copy is the newer, and so is one read by its number: a repo no longer
            // read keeps its last read.
            if searched.is_some_and(|found| found.iter().any(|x| x.number == p.number) || read_alone.contains(&crate::prlinks::pr_key(&format!("{slug}#{}", p.number)))) {
                continue;
            }
            let mut item = pr_item(lanes, &owners, slug, repo, p, branch_freed(list, p), login, links);
            if searched.is_some() {
                item.review_requested = false;
                item.requested_by = None;
                item.requested_at = None;
            }
            out.push(item);
        }
    }
    for (slug, list) in requested {
        let Some(&repo) = repos.get(slug.as_str()) else { continue };
        let read = prs.get(slug).filter(|_| polled.contains(slug));
        let unread = |p: &&PrRaw| !read.is_some_and(|read| read.iter().any(|x| x.number == p.number));
        // The search lists only open PRs; the repo's last read has the ones that ended.
        let history = || prs.get(slug).into_iter().flatten().chain(list);
        out.extend(list.iter().filter(unread).map(|p| pr_item(lanes, &owners, slug, repo, p, branch_freed(history(), p), login, links)));
    }
    let listed: HashSet<String> = out.iter().map(|p| crate::prlinks::pr_key(&p.key)).collect();
    for (slug, list) in made {
        for p in list.iter().filter(|p| !listed.contains(&crate::prlinks::pr_key(&format!("{slug}#{}", p.number)))) {
            out.extend(made_item(&repos, &owners, lanes, slug, p, login, links));
        }
    }
    out.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    out
}

/// A PR sessions made in a repo whose PRs aren't read (`pr_items`): in the project whose repo it's
/// in, by its name in any case, else under the project of the session it's for. None when no
/// session it's for is on the board, which leaves it no project.
fn made_item(repos: &HashMap<&str, (&str, &str, Option<&str>)>, owners: &HashMap<&Path, &[String]>, lanes: &[Lane], slug: &str, p: &PrRaw, login: Option<&str>, links: &PrLinks) -> Option<PrItem> {
    if let Some((&local, &repo)) = repos.iter().find(|(s, _)| s.eq_ignore_ascii_case(slug)) {
        return Some(pr_item(lanes, owners, local, repo, p, None, login, links));
    }
    let mut item = pr_item(lanes, owners, slug, ("", slug.rsplit('/').next().unwrap_or(slug), None), p, None, login, links);
    let key = crate::prlinks::pr_key(&item.key);
    let worked = links.worked.get(&key).into_iter().flatten().filter(|k| !links.unlinked.contains(&(key.clone(), (*k).clone())));
    let project = item.primary.iter().chain(item.sessions.iter().map(|s| &s.key)).chain(worked).find_map(|k| home(lanes, k)).map(|(l, _)| l.project.clone())?;
    item.project = project;
    Some(item)
}

/// The PRs sessions on the board (`lanes`) made or changed by their records, or were linked to by
/// hand, as `owner/name#number`: one linked by hand first, however long ago it was marked, then
/// the latest marked. Records outlast the sessions the board lists (a background job removed,
/// say), and a session unlinked from a PR counts for none. The engine reads the ones in repos
/// whose PRs aren't read by their numbers (ghpoll.rs `made_to_read`), which are listed only with
/// such a session (`made_item`).
pub fn worked_prs(links: &PrLinks, lanes: &[Lane]) -> Vec<String> {
    let on_board: HashSet<&str> = lanes.iter().flat_map(|l| l.sessions.iter().map(|s| s.key.as_str())).collect();
    let mut prs: Vec<(Reverse<i64>, &String)> = links
        .worked
        .iter()
        .filter(|(pr, sessions)| sessions.iter().any(|s| on_board.contains(s.as_str()) && !links.unlinked.contains(&((*pr).clone(), s.clone()))))
        .map(|(pr, _)| {
            let marked = links.made.get(pr).into_iter().flatten().map(|(_, at)| *at).max().filter(|_| !links.by_hand.contains(pr));
            (Reverse(marked.unwrap_or(i64::MAX)), pr)
        })
        .collect();
    prs.sort();
    prs.into_iter().map(|(_, pr)| pr.clone()).collect()
}

/// Applies the snoozes you gave reviews asked of you. Until its time a PR that waits on your
/// review carries `snoozed_until`, and Review requested, the floating bar's reviews and the mark
/// leave it out; then it's back as GitHub has it. A PR that no longer waits on you has none,
/// and the snooze runs out in state.json unused.
pub fn apply_review_snoozes(prs: &mut [PrItem], snoozes: &[Snooze], now: i64) {
    if snoozes.is_empty() {
        return;
    }
    for p in prs.iter_mut().filter(|p| p.review_requested && p.state.is_open()) {
        p.snoozed_until = snoozes.iter().find(|z| z.key == p.key && z.until > now).map(|z| z.until);
    }
}

/// The repos whose PRs are read, one GitHub query each: those with a worktree on a branch other
/// than the default, which may have a PR of its own. The rest hear only of PRs that wait on your
/// review, from one search across GitHub.
pub fn polled_slugs(seeds: &[LaneSeed]) -> BTreeSet<String> {
    seeds.iter().filter(|s| s.branch.is_some() && s.branch != s.default_branch).filter_map(|s| s.repo_slug.clone()).collect()
}

/// One PR that `pr_items` doesn't list, for a link to it in a conversation: one older than the
/// PRs the board reads, or in a repo no project covers. It's matched to worktrees and sessions
/// as the listed ones are. `slug` is the repo as the link names it; a project's repo that
/// answers to it in another case, or by the name it had before a rename (`renamed`, old name to
/// new, lowercased), is the one it's matched in.
pub fn pr_card(seeds: &[LaneSeed], lanes: &[Lane], slug: &str, renamed: &HashMap<String, String>, p: &PrRaw, login: Option<&str>, links: &PrLinks) -> PrItem {
    let repos = repos_of(seeds);
    let owners = owners_of(seeds);
    let found = repos.iter().find(|(s, _)| {
        s.eq_ignore_ascii_case(slug) || renamed.get(&s.to_ascii_lowercase()).is_some_and(|now| now.eq_ignore_ascii_case(slug))
    });
    match found {
        Some((&local, &repo)) => pr_item(lanes, &owners, local, repo, p, None, login, links),
        None => pr_item(lanes, &owners, slug, ("", slug.rsplit('/').next().unwrap_or(slug), None), p, None, login, links),
    }
}

/// The GitHub owners each worktree's checkout has a remote for (`LaneSeed::head_owners`), by
/// its path, a lane's id: the forks whose branches it can be on.
fn owners_of(seeds: &[LaneSeed]) -> HashMap<&Path, &[String]> {
    seeds.iter().map(|s| (s.path.as_path(), s.head_owners.as_slice())).collect()
}

/// Each project's repo by its slug: the project, the repo's name and its default branch.
fn repos_of(seeds: &[LaneSeed]) -> HashMap<&str, (&str, &str, Option<&str>)> {
    let mut repos: HashMap<&str, (&str, &str, Option<&str>)> = HashMap::new();
    for s in seeds {
        if let Some(slug) = s.repo_slug.as_deref() {
            repos.entry(slug).or_insert((s.project.as_str(), s.repo.as_str(), s.default_branch.as_deref()));
        }
    }
    repos
}

/// When the last PR from `p`'s branch before it ended, of the repo's `list`: a branch such as
/// `yolo` serves one PR after another, and a push before then was for that one. A branch of the
/// same name in another fork (`head_owner`) is another branch.
fn branch_freed<'a>(list: impl IntoIterator<Item = &'a PrRaw>, p: &PrRaw) -> Option<i64> {
    let before = list.into_iter().filter(|q| q.number != p.number && q.head_ref == p.head_ref && q.head_owner == p.head_owner);
    before.filter_map(|q| q.merged_at.or(q.closed_at)).filter(|&end| end <= p.created_at).max()
}

/// A PR with its worktrees and sessions, as `pr_items` says. `freed` is when an earlier PR from
/// its branch ended (`branch_freed`), when that's known.
fn pr_item(lanes: &[Lane], owners: &HashMap<&Path, &[String]>, slug: &str, (project, repo, default_branch): (&str, &str, Option<&str>), p: &PrRaw, freed: Option<i64>, login: Option<&str>, links: &PrLinks) -> PrItem {
    let here: Vec<&Lane> = lanes.iter().filter(|l| l.repo_slug.as_deref() == Some(slug)).collect();
    // Whether a lane's branch of the PR's name can be the PR's: not when the PR comes from a fork
    // its checkout has no remote for. A lane with no seed to say (a test's) can.
    let holds_head = |l: &Lane| p.head_owner.as_ref().map_or(true, |o| owners.get(Path::new(&l.id)).map_or(true, |have| has_owner(have, o)));
    let head = p.head_ref.as_str();
    let (opened, ended) = (p.created_at, p.merged_at.or(p.closed_at));
    let before_end = move |s: &Session| ended.map_or(true, |end| s.started_at.unwrap_or(s.last_activity) <= end);
    let during = move |s: &Session| s.last_activity >= opened - 3_600_000 && before_end(s);
    // How often a session listed under a lane of the repo pushed to the branch in the PR's time,
    // as `during` has it, and after an earlier PR from the branch ended: a branch used for one PR
    // after another (`yolo`) has the earlier ones' pushes too, and one of the same name in another
    // repo, or a fork the lane's checkout has no remote for, its own. A PR from the default
    // branch counts none, as it lists no sessions by branch.
    let by_branch = default_branch != Some(head);
    let since = (opened - 3_600_000).max(freed.map_or(i64::MIN, |t| t + 1));
    let pushes = move |l: &Lane, k: &str| -> u32 {
        let pushed = links.pushed.get(k).filter(|_| by_branch && l.repo_slug.as_deref() == Some(slug) && holds_head(l)).into_iter().flatten();
        pushed.filter(|(b, at)| b == head && *at >= since && ended.map_or(true, |end| *at <= end)).count() as u32
    };
    let mut sessions: Vec<PrSession> = Vec::new();
    if by_branch {
        // Sessions in their own lanes first, then ones that came to a worktree on the branch.
        let own = here.iter().filter(|l| holds_head(l)).flat_map(|&l| {
            // One that works here from another folder goes by this worktree's branch, and by its
            // folder's, where it may have worked on the PR before it came (`compose`).
            let on_branch = move |s: &Session| s.branch.as_deref().or(l.branch.as_deref()) == Some(head) || s.folder_branch.as_deref() == Some(head);
            l.sessions.iter().filter(move |s| s.visitor.is_none() && on_branch(s) && during(s)).map(move |s| (l, s))
        });
        let guests = here.iter().filter(|l| l.branch.as_deref() == Some(head) && holds_head(l)).flat_map(|&l| {
            let counts = move |s: &Session| match s.visitor.as_deref() {
                None => false,
                Some(MADE_IT) => before_end(s),
                Some(_) => during(s),
            };
            l.sessions.iter().filter(move |s| counts(s)).map(move |s| (l, s))
        });
        let pushers = here.iter().flat_map(|&l| l.sessions.iter().filter(move |s| s.visitor.is_none() && during(s) && pushes(l, &s.key) > 0).map(move |s| (l, s)));
        for (l, s) in own.chain(guests).chain(pushers) {
            if !sessions.iter().any(|x| x.key == s.key) {
                sessions.push(PrSession { key: s.key.clone(), lane: l.id.clone(), pushes: pushes(l, &s.key), primary: None });
            }
        }
    }
    let key = crate::prlinks::pr_key(&format!("{slug}#{}", p.number));
    for k in links.worked.get(&key).into_iter().flatten() {
        if sessions.iter().any(|x| &x.key == k) {
            continue;
        }
        // The lane it's in, else one it visited.
        if let Some((l, s)) = home(lanes, k) {
            sessions.push(PrSession { key: s.key.clone(), lane: l.id.clone(), pushes: pushes(l, &s.key), primary: None });
        }
    }
    sessions.retain(|x| !links.unlinked.contains(&(key.clone(), x.key.clone())));
    let (primary, primary_by) = pr_primary(lanes, &sessions, &key, links).unzip();
    let mine = login.is_some() && p.author.as_deref() == login;
    let by_hand = links.nudged.contains(&key);
    let watched = links.watched.contains(&key) && primary_by == Some(PrimaryBy::Picked);
    let by_default = links.nudge_by_default_in.get(slug).copied().unwrap_or(links.nudge_by_default);
    let nudge = p.state.is_open() && !links.not_nudged.contains(&key) && (by_hand || (by_default && mine));
    // Assigned to you since your last review: it waits on your review as a request does, asked
    // by whoever assigned you, since then, whichever move its turn says.
    let assigned = p.assigned_review(login).filter(|_| !p.review_requested);
    let requested_by = p.requested_by.clone().filter(|_| p.review_requested).or_else(|| assigned.map(|a| a.by.clone()));
    let requested_at = p.requested_at.filter(|_| p.review_requested).or_else(|| assigned.map(|a| a.at));
    let review_requested = p.review_requested || assigned.is_some();
    PrItem {
        key: format!("{slug}#{}", p.number),
        repo_slug: slug.to_string(),
        repo: repo.to_string(),
        project: project.to_string(),
        number: p.number,
        title: p.title.clone(),
        url: p.url.clone(),
        state: p.state,
        author: p.author.clone(),
        mine,
        review_requested,
        requested_by,
        requested_at,
        reviewers_asked: p.reviewers_asked.clone(),
        snoozed_until: None,
        nudge,
        nudge_by_default: nudge && !by_hand,
        watched,
        primary,
        primary_by,
        head: p.head_ref.clone(),
        base: p.base_ref.clone(),
        checks: p.checks.clone(),
        merge: p.merge,
        additions: p.additions,
        deletions: p.deletions,
        changed_files: p.changed_files,
        created_at: p.created_at,
        updated_at: p.updated_at,
        merged_at: p.merged_at,
        closed_at: p.closed_at,
        lanes: here.iter().filter(|l| l.pr.as_ref().map(|x| x.number) == Some(p.number)).map(|l| l.id.clone()).collect(),
        sessions,
        head_oid: p.head_oid.clone(),
        authored_by: p.authored_by,
        labels: p.labels.clone(),
        auto_review: None,
        last_review: p.last_review.clone(),
        stale_approval: p.stale_approval.clone(),
        turn: turn_of(p, mine, login, &links.agent_posts),
        shared_review: Default::default(),
    }
}

/// Whose move an open PR is, where its review state doesn't say (`Turn`), from what was said on
/// it lately (`PrRaw::said`). A PR of yours that a reviewer asked changes of is their move while
/// they're asked again or you wrote since their last word; yours where commits came after it and
/// nobody said so (`ReplyNeeded`), or where they wrote again after asking (`ReviewerReplied`);
/// and with nothing since, the request for changes says it. Of several such reviewers, the move
/// that's yours goes first, naming each it holds for. Someone else's PR you wrote on or reviewed
/// is yours where its author wrote after you were on it (anything of yours, your agent's posted
/// reviews too) and nothing of yours answered them since: a review you submitted, or words of your
/// own, not your agent's post, which leaves the sign-off to you as a request on GitHub would; and
/// not once your latest review approves the commit it's at, when what they say is theirs to act
/// on. It's yours too where someone assigned you to it after your last review and GitHub doesn't
/// ask for your review (`Assigned`, `PrRaw::assigned_review`); of the two, the later says it. A
/// review just submitted here counts at once (`last_review`, ghpoll.rs `on_approved`), before
/// GitHub is read again. `agent_posts` are your agent's posts (`PrLinks::agent_posts`). A push is taken to be when its
/// head commit was made, the nearest GitHub says: one made long before it's pushed reads as earlier
/// than it was. Nobody said so only where what was said reaches back to the push
/// (`PrRaw::said_from`): a run of a bot's comments can hide an older word of yours.
pub fn turn_of(p: &PrRaw, mine: bool, login: Option<&str>, agent_posts: &HashSet<String>) -> Option<Turn> {
    if !p.state.is_open() || p.state == PrState::Draft {
        return None;
    }
    let same = |a: &str, b: &str| a.eq_ignore_ascii_case(b);
    let wrote = |who: &str| p.said.iter().filter(|s| s.asked.is_none() && same(&s.who, who)).map(|s| s.at).max();
    let author = p.author.as_deref()?;
    let turn = |kind, who: &str, since| Some(Turn { kind, who: who.to_string(), also: Vec::new(), since });
    if !mine {
        let assigned = p.assigned_review(login).filter(|_| !p.review_requested).and_then(|a| turn(TurnKind::Assigned, &a.by, Some(a.at)));
        let replied = || -> Option<Turn> {
            let (me, theirs) = (login?, wrote(author)?);
            let mine_said = || p.said.iter().filter(|s| s.asked.is_none() && same(&s.who, me));
            let on_it = mine_said().any(|s| s.at < theirs);
            let agents = |s: &&crate::github::Said| s.url.as_ref().is_some_and(|u| agent_posts.contains(u));
            let answered = mine_said().filter(|s| !agents(s)).map(|s| s.at).max().max(p.last_review.as_ref().and_then(|l| l.at));
            let approved = p.last_review.as_ref().is_some_and(|l| l.state == LastReviewState::Approved && l.commit.is_some() && l.commit == p.head_oid);
            if on_it && answered.is_none_or(|a| theirs > a) && !approved { turn(TurnKind::AuthorReplied, author, Some(theirs)) } else { None }
        };
        return assigned.into_iter().chain(replied()).max_by_key(|t| t.since);
    }
    let of = |c: &crate::github::ChangesAsked| -> Option<Turn> {
        let asked = p.said.iter().filter(|s| s.asked.as_deref().is_some_and(|r| same(r, &c.who))).map(|s| s.at).max();
        if p.reviewers_asked.iter().any(|r| same(r, &c.who)) {
            return turn(TurnKind::RereviewAsked, &c.who, asked);
        }
        let theirs = wrote(&c.who).map_or(c.at, |t| t.max(c.at));
        let yours = wrote(author).max(asked);
        // Commits came since their review, where GitHub says which commit that was of.
        let pushed = c.commit.is_some() && p.head_oid.is_some() && p.head_oid != c.commit;
        match p.head_at {
            Some(at) if pushed && at > theirs && yours.is_none_or(|y| at > y) && p.said_from.is_none_or(|from| from <= at) => turn(TurnKind::ReplyNeeded, &c.who, Some(at)),
            _ if yours.is_some_and(|y| y > theirs) => turn(TurnKind::RereviewAsked, &c.who, yours),
            _ if theirs > c.at => turn(TurnKind::ReviewerReplied, &c.who, Some(theirs)),
            _ => None,
        }
    };
    let urgency = |t: &Option<Turn>| match t.as_ref().map(|t| t.kind) {
        Some(TurnKind::ReplyNeeded) => 0,
        Some(TurnKind::ReviewerReplied) => 1,
        None => 2,
        Some(_) => 3,
    };
    let turns: Vec<Option<Turn>> = p.changes_asked.iter().filter(|c| !same(&c.who, author)).map(of).collect();
    let first = turns.iter().min_by_key(|t| urgency(t))?.clone()?;
    let also = turns.iter().flatten().filter(|t| t.kind == first.kind && t.who != first.who).map(|t| t.who.clone()).collect();
    Some(Turn { also, ..first })
}

/// The PR's primary session, of the ones listed on it, and why: the one picked by hand; else the
/// one that pushed to its branch most often (`PrSession::pushes`); else the one that started
/// first. The one that opened it, whose records show it first (a session that later pushes to
/// it, comments or edits it is marked after), counts as having pushed once at least, since its
/// push may have left no record (a quiet `git push -q`, a harness that records none), and wins a
/// tie: a session that pushes a fix or two to another's PR doesn't take it over, and one that
/// goes on with the work does. Without pushes that's the one that opened it. A copy of a
/// conversation (a fork, a move to another account) carries its marks and pushes with their
/// times, so the one that started first wins a tie. An archived session is listed but never the
/// primary, picked or not: the next takes over. Nor is an automatic review (`Helper::pr`), unless picked: it reads the work
/// and says what it found, and runs in the PR's worktree on its branch, so with the session
/// that opened it gone it took over as the one that started first, and was told of what
/// happened on the PR in place of the work's own session.
fn pr_primary(lanes: &[Lane], sessions: &[PrSession], key: &str, links: &PrLinks) -> Option<(String, PrimaryBy)> {
    let archived = |k: &str| home(lanes, k).is_some_and(|(_, s)| s.archived);
    let sessions: Vec<&PrSession> = sessions.iter().filter(|s| !archived(&s.key)).collect();
    if let Some(k) = links.picked.get(key).filter(|k| sessions.iter().any(|s| &s.key == *k)) {
        return Some((k.clone(), PrimaryBy::Picked));
    }
    let reviews = |k: &str| home(lanes, k).is_some_and(|(_, s)| s.helper.as_ref().is_some_and(|h| h.pr.is_some()));
    let sessions: Vec<&PrSession> = sessions.into_iter().filter(|s| !reviews(&s.key)).collect();
    let started = |k: &str| home(lanes, k).map_or(i64::MAX, |(_, s)| s.started_at.unwrap_or(s.last_activity));
    // When its records first show it.
    let mark = |k: &str| links.made.get(key).into_iter().flatten().find(|(m, _)| m == k).map(|(_, at)| crate::prlinks::mark_order(*at));
    let opener = sessions.iter().filter_map(|s| Some((mark(&s.key)?, started(&s.key), s.key.as_str()))).min().map(|(_, _, k)| k);
    let pushes = |s: &PrSession| if Some(s.key.as_str()) == opener { s.pushes.max(1) } else { s.pushes };
    // A session without a mark after every one with one.
    let best = sessions.iter().min_by_key(|s| (Reverse(pushes(s)), mark(&s.key).unwrap_or((true, i64::MAX)), started(&s.key), s.key.as_str()))?;
    let by = if Some(best.key.as_str()) == opener {
        PrimaryBy::Opened
    } else if best.pushes > 0 {
        PrimaryBy::Pushed
    } else {
        PrimaryBy::Started
    };
    Some((best.key.clone(), by))
}

/// The first rule that matches. See *The model*, Lane status. Needs you, Working and Your turn
/// are about the lane's agents; a PR that needs work (failing checks, changes requested) keeps
/// it in In review, where the Inbox's PR needs work finds it.
pub fn bay_of(sessions: &[Session], pr: Option<&Pr>) -> (Bay, Status) {
    let has = |st: Status| sessions.iter().any(|s| s.status == st);
    if has(Status::Needs) {
        return (Bay::Needs, Status::Needs);
    }
    if has(Status::Error) {
        return (Bay::Needs, Status::Error);
    }
    if has(Status::Working) {
        return (Bay::Working, Status::Working);
    }
    if has(Status::Unread) {
        return (Bay::Unread, Status::Unread);
    }
    match pr {
        Some(pr) if pr.state.is_open() => (Bay::Review, Status::Idle),
        Some(_) => (Bay::Merged, Status::Idle),
        None => (Bay::Idle, Status::Idle),
    }
}

pub fn checks_summary(c: &Checks) -> String {
    let mut parts = Vec::new();
    if c.failed > 0 {
        parts.push(format!("{} failing", c.failed));
    }
    if c.pending > 0 {
        parts.push(format!("{} running", c.pending));
    }
    if c.passed > 0 {
        parts.push(format!("{} passed", c.passed));
    }
    if parts.is_empty() {
        return "No checks".to_string();
    }
    if parts.len() == 1 && c.passed > 0 {
        return format!("{} checks passed", c.passed);
    }
    parts.join(" · ")
}

/* ---- Jev's read (insights.rs) ---- */

/// How sure Jev has to be that a finished turn asks you something before it's Needs you.
pub const ASKS_YOU: f64 = 0.7;
/// A quiet command is stuck only when Jev gives it at least this chance of waiting for typing.
pub const WAITS_FOR_INPUT: f64 = 0.3;
/// A background command is stuck when Jev is this sure.
pub const BACKGROUND_STALLED: f64 = 0.7;
/// A background command is a server or watcher left running when Jev is this sure.
pub const BACKGROUND_LEFT_RUNNING: f64 = 0.7;
/// A session counts as finished for its worktree from this sure.
pub const FINISHED: f64 = 0.8;

/// The last sentence of a reply, for a row: where its question is.
pub fn last_sentence(text: &str) -> String {
    truncate(sentences(text).last().copied().unwrap_or(""), 140)
}

/// Where a reply's question is, for a row that says it asks you: its last sentence that ends in
/// a question mark, else its last sentence. A reply often goes on after its question, with what
/// it tested or the sources it read, and the last of that isn't what it asks.
pub fn question_in(text: &str) -> String {
    let asked = sentences(text).into_iter().filter(|s| is_question(s)).last();
    asked.map_or_else(|| last_sentence(text), |q| truncate(q, 140))
}

/// Whether a reply has a question in it, wherever it is and however it's set (`**Keep it?**`),
/// for where there's no read of what it asks to go by.
pub fn has_question(text: &str) -> bool {
    sentences(text).into_iter().any(is_question)
}

fn is_question(sentence: &str) -> bool {
    sentence.trim_end_matches(CLOSERS).ends_with('?')
}

/// What can close a sentence after its stop: Markdown's emphasis, quotes, a bracket.
const CLOSERS: [char; 7] = ['*', '_', '"', '\'', '”', '’', ')'];

/// A reply's sentences, in order.
fn sentences(text: &str) -> Vec<&str> {
    let t = text.trim().trim_start_matches('…').trim();
    // A sentence ends at a stop, and what closes it, followed by a space, so `App.tsx`, `e.g.`
    // and a link's `?q=` don't split one, and `?**` does.
    let chars: Vec<(usize, char)> = t.char_indices().collect();
    let mut starts = vec![0];
    for (i, &(_, c)) in chars.iter().enumerate() {
        if matches!(c, '.' | '!' | '?') {
            let after = chars[i + 1..].iter().find(|(_, c)| !CLOSERS.contains(c));
            if let Some(&(at, _)) = after.filter(|(_, c)| c.is_whitespace()) {
                starts.push(at);
            }
        }
    }
    let ends = starts.iter().skip(1).copied().chain([t.len()]);
    starts.iter().zip(ends).map(|(&a, b)| t[a..b].trim()).collect()
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

/// Applies quiet from Codealong to a session, after its status and Jev's read of it and before a
/// mark of unread or a snooze, before bays and headlines are worked out. A quiet session's turn
/// that ended is idle rather than Your turn, so it's in no queue, on no pill and off the mark:
/// one that loops, or wakes on a schedule, doesn't wait on you each time it stops. What needs you
/// still does: a reply Jev reads as asking you something (Needs you by then), a call to allow, a
/// question, an error. Your own Mark as unread, and a snooze's end, still bring it back, as they
/// come after.
pub fn apply_quiet(s: &mut Session, quiet: bool) {
    if !quiet {
        return;
    }
    s.quiet = true;
    if s.status == Status::Unread {
        s.status = Status::Idle;
    }
}

/// Puts on a session the labels you gave it (state.rs `labels`), by their ids among the todos'
/// labels, in the order you gave them. One since deleted from the labels shows on none.
pub fn apply_labels(s: &mut Session, ids: &[String], labels: &[Label]) {
    s.labels = ids.iter().filter_map(|id| labels.iter().find(|l| &l.id == id)).cloned().collect();
}

/// Applies a mark of seen from a session's own agent (`codealong session seen me`), after quiet
/// and before what you do to it yourself: a turn whose agent said it needs nothing from you is
/// idle rather than Your turn, and says so (`Session::agent_seen`). Only a turn that ended as
/// Your turn: a reply Jev reads as asking you something is Needs you by then, and a call to
/// allow, a question or an error still wait on you, as does a turn its harness ended (the
/// connection lost, a usage limit, cut off), which isn't the reply its agent spoke for. The agent
/// marks the turn it's taking, and what came after the mark is past what it spoke for
/// (`agent_seen_spent`). Your Mark as unread, a side conversation's answer and a snooze that ends
/// still bring it back, as they come after (`settle_agent_seen`).
pub fn apply_agent_seen(s: &mut Session, mark: Option<&AgentSeen>) {
    let Some(m) = mark else { return };
    let cut_short = s.connection_lost.is_some() || s.limit.is_some() || s.cut_off.is_some();
    if s.status != Status::Unread || cut_short || agent_seen_spent(s, m.at) {
        return;
    }
    s.status = Status::Idle;
    s.agent_seen = Some(m.clone());
}

/// Whether a session has moved on from the turn its agent marked seen at `at`: something came
/// into it since, a prompt or, where its harness says (`Session::input_at`), another session's
/// message, a background task's report or a scheduled wakeup, which the record keeps after the
/// turn they brought has ended, though Codealong wasn't running to see it; or a turn of its
/// started since, seen as it runs (`moved_on`), where its harness says no more. Spent, the mark
/// is void, and the engine forgets it (`Engine::forget_spent_agent_seen`).
pub fn agent_seen_spent(s: &Session, at: i64) -> bool {
    s.input_at.is_some_and(|t| t > at) || moved_on(s, at, at).is_some()
}

/// Once the rest is applied: a turn its agent marked seen that something after brought back (a
/// mark of unread, a side conversation's answer, a snooze that ended, a move to the top) waits on
/// you as any does, and no longer says its agent saw it.
pub fn settle_agent_seen(s: &mut Session) {
    if s.status != Status::Idle {
        s.agent_seen = None;
    }
}

/// What a session helps (`Session::helper`): the session whose agent started it as a helper
/// (origins.json, with whether that session is told what it does), while the board has that
/// session, or the PR whose automatic review it is (reviews.json). A helper whose session is gone
/// is one of your own again, so nothing it does is left where nobody looks.
pub fn helper_of(parent: Option<(&str, bool)>, on_board: impl Fn(&str) -> bool, reviews: Option<&str>) -> Option<Helper> {
    let (parent, report) = match parent.filter(|(p, _)| on_board(p)) {
        Some((p, report)) => (Some(p.to_string()), report),
        None => (None, false),
    };
    let pr = reviews.map(String::from);
    (parent.is_some() || pr.is_some()).then_some(Helper { parent, pr, report, yours: false })
}

/// Sessions started in another session's own scratch folder (`Adapter::scratch_owner`: Claude
/// Code's scratchpad, a background job's folder) are that session's helpers, where nothing else
/// says whose they are: its agent ran them there (test chats in tmux, say) and reads them itself,
/// so their ended turns don't wait on you, as a helper's don't. None is told of, since none asked
/// to be (`Helper::report`). Only a session nothing says the origin of: a fork of one, or one
/// started without `--helper`, was made for you. Headless runs have their own rule, and a
/// session you made your own (`own`) stays yours. `owner_of` names a folder's session by its
/// harness and ID.
pub fn scratch_helpers(sessions: &mut [Session], owner_of: impl Fn(&Path) -> Option<(Harness, String)>, own: impl Fn(&str) -> bool) {
    let keys: HashMap<(Harness, String), String> = sessions.iter().map(|s| ((s.harness, s.id.clone()), s.key.clone())).collect();
    for s in sessions.iter_mut().filter(|s| s.helper.is_none() && s.origin.is_none() && !s.headless && !own(&s.key)) {
        let Some(parent) = owner_of(Path::new(&s.cwd)).and_then(|o| keys.get(&o)).filter(|k| **k != s.key) else { continue };
        s.helper = Some(Helper { parent: Some(parent.clone()), pr: None, report: false, yours: false });
        s.origin = Some(SessionOrigin { key: parent.clone(), how: OriginHow::Started });
    }
}

/// A session's helpers on the board (`Session::helper`): the sessions its agent started as
/// helpers, each once, from the lane it runs in, the archived ones left out.
pub fn helpers_of<'a>(lanes: &'a [Lane], key: &str) -> Vec<&'a Session> {
    lanes.iter().flat_map(|l| &l.sessions).filter(|s| s.visitor.is_none() && !s.archived && s.helper.as_ref().and_then(|h| h.parent.as_deref()) == Some(key)).collect()
}

/// Applies a helper's rule to it, where quiet is applied (`apply_quiet`): a turn it ends is idle
/// rather than Your turn, as its agent reads what it wrote, not you. One that needs you (a call
/// to allow, a question, an error) still does, and your own Mark as unread still brings it back.
/// One you're talking to (`Helper::yours`) wrote it for you, so its turn is yours.
pub fn apply_helper(s: &mut Session) {
    if s.helps_an_agent() && s.status == Status::Unread {
        s.status = Status::Idle;
    }
}

/// Applies a listening session's rule to it (listen.rs), with a helper's: while it hears, a turn
/// it ends is idle rather than Your turn. It ends one every few seconds, each an answer to what
/// was just heard, which the windows show as it comes; none waits on you. Nor does one whose
/// answer reads as a question for you (`apply_insight`): what a coach says is often a question,
/// to put to the meeting, and it's shown where its answers are, not as something to answer. One
/// that needs you at a prompt (a call to allow) or stopped on an error still does, and your own
/// Mark as unread still brings it back. Once hearing has ended its turns are yours again, the one
/// that sums the meeting up first. That it's hearing is said on it (`Session::listening`), for
/// whatever shows it.
pub fn apply_listening(s: &mut Session, hearing: bool) {
    s.listening = hearing;
    if hearing && (s.status == Status::Unread || (s.status == Status::Needs && s.asks)) {
        s.status = Status::Idle;
        s.asks = false;
    }
}

/// Applies a headless run's rule to it (`Session::headless`), with a helper's: a turn it ends is
/// idle, an error too, as what ran it reads how it ended and nobody is at it to go on from
/// there. One waiting to have a call allowed still needs you, and your own Mark as unread still
/// brings it back.
pub fn apply_headless(s: &mut Session) {
    if s.headless && matches!(s.status, Status::Unread | Status::Error) {
        s.status = Status::Idle;
    }
}

/// The session whose agent started a headless run, by the run's process: the nearest of its
/// parents that's a session's own (`by_pid`), as a command an agent runs goes on under it (an
/// eval's `claude -p` under python, under the shell Claude Code's Bash tool opened, under that
/// session's claude). `parent` gives a process's parent, None once it's gone or isn't yours. A
/// run a script in a terminal of yours started climbs to launchd without meeting one.
pub fn started_by<'a>(pid: u32, parent: impl Fn(u32) -> Option<u32>, by_pid: &HashMap<u32, &'a str>) -> Option<&'a str> {
    let mut at = pid;
    // However deep a script nests, a climb this long is a loop in what `parent` said.
    for _ in 0..64 {
        at = parent(at).filter(|p| *p > 1)?;
        if let Some(key) = by_pid.get(&at) {
            return Some(key);
        }
    }
    None
}

/// The session that started a headless run: the nearest running session above its process
/// (`started_by`), else the first named in the environment the run started with (`under`, each a
/// harness and a session ID: `harness::started_under`) of the sessions on the board (`by_id`). A
/// script an agent starts on its own (`start_new_session`, `nohup … &`) is launchd's once its
/// launcher returns, so no session is above it, but it keeps what the agent's tool set: an
/// eval's driver did, and each `claude -p` its judge ran was listed as a session of its own. A
/// script you run in a terminal of yours carries no session's.
pub fn run_started_by<'a>(pid: u32, parent: impl Fn(u32) -> Option<u32>, under: impl Fn(u32) -> Vec<(Harness, String)>, by_pid: &HashMap<u32, &'a str>, by_id: &HashMap<(Harness, &str), &'a str>) -> Option<&'a str> {
    started_by(pid, parent, by_pid).or_else(|| under(pid).iter().find_map(|(harness, id)| by_id.get(&(*harness, id.as_str())).copied()))
}

/// What moved a session on from where it stood when an agent gave its word on it (a tag, replies
/// suggested), so that word no longer answers where it stands. The engine keeps it as it takes
/// the word away (`tags::Store::gone`, `suggestions::Store::gone`), for `codealong changes` to
/// say why a session that had one has none now.
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "camelCase", tag = "by")]
pub enum MovedOn {
    /// It was sent a prompt after the agent read it (its `last_prompt_at`), or its user answered
    /// it from Codealong.
    Prompt { at: i64 },
    /// A turn of its started after the word was given, at this time. A message from another
    /// session or from Codealong, a background command's report or a scheduled wakeup can start
    /// one without moving its `last_prompt_at`.
    Turn { at: i64 },
}

impl MovedOn {
    pub fn at(&self) -> i64 {
        match self {
            MovedOn::Prompt { at } | MovedOn::Turn { at } => *at,
        }
    }
}

/// What moved the session on from an agent's word on it given at `at`, about the reply to the
/// prompt it sent at `prompt_at`: a prompt sent since, or a turn started since the word was
/// given. A turn that started after it has moved on from what it answered, even once it ends, so
/// the engine takes the word away once it sees one (`Engine::forget_stale_tags`,
/// `Engine::forget_stale_suggestions`). Busy with nothing said since a turn that ended before
/// the word isn't one (`Session::turn_ended`): Claude Code compacting a conversation left idle
/// took away the tags and replies of sessions still waiting on what they were for. None while
/// the word holds; a turn the agent read it in started before it.
pub fn moved_on(s: &Session, prompt_at: i64, at: i64) -> Option<MovedOn> {
    if s.last_prompt_at > prompt_at {
        return Some(MovedOn::Prompt { at: s.last_prompt_at });
    }
    let said_nothing = s.turn_ended.is_some_and(|ended| ended <= at);
    s.status_since.filter(|since| s.status == Status::Working && *since > at && !said_nothing).map(|since| MovedOn::Turn { at: since })
}

/// Whether replies suggested for a session still answer where it stands (`moved_on`): it's been
/// sent nothing since the prompt whose reply the suggester read (`prompt_at`; a prompt of yours,
/// from Codealong or where it runs), and no turn of its has started since they were suggested.
/// Ones from Macs that didn't say what was read go by when they were suggested.
pub fn suggestions_hold(s: &Session, g: &Suggestions) -> bool {
    moved_on(s, g.prompt_at.unwrap_or(g.at), g.at).is_none()
}

/// Why replies for a session are turned down, given its last prompt as the suggester read it, or
/// none when they answer where it stands. Read before you replied, they'd answer the turn before
/// yours; suggested while it takes a turn, the reply it's writing isn't what was read.
pub fn suggestion_refused(s: &Session, read_prompt_at: i64) -> Option<String> {
    read_refused(s, read_prompt_at, "suggest")
}

/// Why an agent's word on a session is turned down, given the session's last prompt as the agent
/// read it: `verb` is what the agent does (suggest, tag), for the sentence. None when its word is
/// for where the session stands.
pub fn read_refused(s: &Session, read_prompt_at: i64, verb: &str) -> Option<String> {
    if s.last_prompt_at > read_prompt_at {
        return Some(format!("It's been sent a prompt since you read it (lastPromptAt {} now, {read_prompt_at} as you read it). Read it again (`codealong session transcript`) and {verb} for where it stands.", s.last_prompt_at));
    }
    if s.status == Status::Working && !s.waits_in_background() {
        let verb = format!("{}{}", verb[..1].to_uppercase(), &verb[1..]);
        return Some(format!("It's taking a turn now, so what you read isn't where it stands. {verb} once it waits on its user again."));
    }
    None
}

/// Puts the replies suggested for a session, or the choices picked for questions it asks, on it
/// while they hold (`suggestions_hold`): after its status and Jev's read of it, before a snooze or
/// a mark of unread moves when its status began.
pub fn apply_suggestions(s: &mut Session, suggested: Option<&Suggestions>) {
    s.suggestions = suggested.filter(|g| (!g.replies.is_empty() || !g.picks.is_empty()) && suggestions_hold(s, g)).cloned();
}

/// Applies a snooze from Codealong to a session, after its status and Jev's read of it, before
/// bays and headlines are worked out. Until its time the session carries `snoozed_until`, and the
/// floating bar and the mark leave it out of Needs you and Your turn. When the time comes it's
/// back as it is then, and one with nothing to show for itself (its turn seen, or its process
/// gone) is Your turn from then until you've seen it: a snooze always brings its session back. A
/// prompt you sent it after snoozing it ends the snooze without that: you took it up.
pub fn apply_snooze(s: &mut Session, snooze: Option<&Snooze>, seen_at: i64, now: i64) {
    let Some(z) = snooze else { return };
    if s.last_prompt_at > z.at || s.archived {
        return;
    }
    if now < z.until {
        s.snoozed_until = Some(z.until);
    } else if seen_at < z.until && matches!(s.status, Status::Idle | Status::Stopped) {
        s.status = Status::Unread;
        s.status_since = Some(z.until);
    }
}

/// Applies a move to the top from Codealong to a session (`Session::raised`), after its snooze,
/// before bays and headlines are worked out: every queue goes to it first (`raised_order`). It
/// holds until you take it up: a prompt sent to it after it was moved puts it back in its place,
/// and so does its finished turn marked seen, which takes the move away for good
/// (`seen_takes_up`). One moved while its turn was over and seen (idle or stopped) waits on you
/// as Your turn from then, as a mark of unread does, since it was moved to the top of what waits;
/// one that's running keeps it, and is first once its turn ends. One moved back to its place
/// (`Raise::lowered`) and an archived one show nothing.
pub fn apply_raise(s: &mut Session, raise: Option<&Raise>) {
    let Some(r) = raise.filter(|r| r.lowered.is_none()) else { return };
    if s.archived || s.last_prompt_at > r.raised.at {
        return;
    }
    if matches!(s.status, Status::Idle | Status::Stopped) {
        s.status = Status::Unread;
        s.status_since = Some(r.raised.at);
    }
    s.raised = Some(r.raised.clone());
}

/// Whether marking a session seen takes it up, ending a move to the top: its turn is over (Your
/// turn, a reply that asks you something, a turn that was cut off, or one already seen), as the
/// windows' Mark as seen has it (lanes.ts `seenable`). Opening one that's running marks it seen
/// too, which mustn't take away a move meant for when its turn ends; nor does looking at a call to
/// allow, which waits on an answer.
pub fn seen_takes_up(s: &Session) -> bool {
    s.asks || (s.status == Status::Error && s.cut_off.is_some()) || matches!(s.status, Status::Unread | Status::Idle | Status::Stopped)
}

/// The order of sessions moved to the top (`Session::raised`), ahead of the rest: the ones you
/// moved, then the ones agents did, so an agent's never goes over yours, each the latest first.
/// Two that weren't moved are equal, and go by their status and wait.
pub fn raised_order(a: Option<&Raised>, b: Option<&Raised>) -> std::cmp::Ordering {
    let rank = |r: &Raised| (r.agent, Reverse(r.at));
    match (a, b) {
        (Some(a), Some(b)) => rank(a).cmp(&rank(b)),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    }
}

/// Why an agent may not move a session to the top, or back to its place (`raised` false), or none
/// when it may: you moved it to the top yourself, which is yours to undo and goes ahead of any
/// agent's; you snoozed it; or you moved it back to its place from where an agent had put it and
/// it hasn't moved on since (no prompt sent to it, and its status hasn't changed).
pub fn raise_refused(s: &Session, raise: Option<&Raise>, raised: bool) -> Option<String> {
    if s.raised.as_ref().is_some_and(|r| !r.agent) {
        return Some("Your user moved it to the top themselves, which is theirs to undo.".to_string());
    }
    if !raised {
        return None;
    }
    if s.snoozed_until.is_some() {
        return Some("Your user snoozed it. Leave it until it's back.".to_string());
    }
    if s.aside.is_some() {
        return Some("Your user set it aside until its side conversation answers. Leave it until then.".to_string());
    }
    let lowered = raise.and_then(|r| r.lowered)?;
    let moved_on = s.last_prompt_at > lowered || s.status_since.is_some_and(|t| t > lowered);
    (!moved_on).then(|| "Your user moved it back to its place after it was moved to the top. Leave it there until it moves on.".to_string())
}

/// Applies a mark of unread from Codealong to a session: one whose turn is over (idle or
/// stopped) is Your turn again, from when it was marked, until you see it or send it a prompt
/// after that, as a turn that just ended would be. One that's running or waits on you shows that
/// instead, and an archived one stays as it is.
pub fn apply_unread(s: &mut Session, mark: Option<&UnreadMark>, seen_at: i64) {
    let Some(u) = mark else { return };
    if s.archived || seen_at >= u.at || s.last_prompt_at > u.at {
        return;
    }
    if matches!(s.status, Status::Idle | Status::Stopped) {
        s.status = Status::Unread;
        s.status_since = Some(u.at);
    }
}

/// Applies what a session's side conversations answered to it, before a mark of unread and a
/// snooze: finishing what it was asked is a turn ending, so the latest one to answer since you saw
/// the session is `Session::side_answer`, and one whose turn is over (idle or stopped) is Your turn
/// from then, saying so, until you see it or send it a prompt after that. One that's running or
/// waits on you shows that instead and keeps the answer for when you come to it, and an archived
/// one stays as it is.
pub fn apply_side_answer(s: &mut Session, sides: &[crate::side::Glance], seen_at: i64) {
    let unseen = sides.iter().filter_map(|g| g.answered_at.filter(|at| *at > seen_at && *at >= s.last_prompt_at).map(|at| (at, g))).max_by_key(|(at, _)| *at);
    let Some((at, g)) = unseen.filter(|_| !s.archived) else { return };
    if matches!(s.status, Status::Idle | Status::Stopped) {
        s.status = Status::Unread;
        s.status_since = Some(at);
        s.excerpt = Some(format!("Answered on the side · {}", g.question));
    }
    s.side_answer = Some(SideAnswer { id: g.id.clone(), open: g.open, at });
}

/// Applies a side conversation you set a session aside for (state.rs `aside`), after what its
/// side conversations answered and before a mark of unread and a snooze. A session that needs you
/// (a call to allow, a question, a held message) set aside while its side conversation answers
/// waits on that first: it carries `Session::aside` and says what it asked on the side, and the
/// windows list it with what's working, out of every queue, off the pills and the mark. Once the
/// side conversation is done (answered, closed or gone) it needs you again as it did, where it
/// waited, with the answer kept for when you come to it (`apply_side_answer`). Only the need it
/// was set aside from, and only until something's sent it (`aside_holds`).
pub fn apply_aside(s: &mut Session, at: Option<i64>, sides: &[crate::side::Glance]) {
    let Some(at) = at.filter(|at| aside_holds(s, *at, sides)) else { return };
    s.aside = Some(at);
    if let Some(g) = sides.iter().find(|g| g.running && g.open) {
        s.excerpt = Some(format!("{ASKING_ON_THE_SIDE}{}", g.question));
    }
}

/// What a session set aside says, before what it asked on the side (`apply_aside`), which its
/// headline leaves below (`session_headline`).
const ASKING_ON_THE_SIDE: &str = "Asking on the side · ";

/// Whether a session set aside at `at` still waits on its side conversation: it needs you as it
/// did then (none of its own since: no new need began, nothing was sent it), and its open side
/// conversation is still answering, none having finished since. One you closed lets it go,
/// though its copy answers on: you've done with it. Once it doesn't hold, the
/// engine forgets it (`Engine::forget_spent_aside`), so a side conversation asked later doesn't
/// set it aside again.
pub fn aside_holds(s: &Session, at: i64, sides: &[crate::side::Glance]) -> bool {
    s.status == Status::Needs
        && !s.archived
        && s.status_since.unwrap_or(s.last_activity) <= at
        && s.last_prompt_at <= at
        && sides.iter().any(|g| g.running && g.open)
        && !sides.iter().any(|g| g.answered_at.is_some_and(|t| t > at))
}

/// Applies Jev's read of a session to what the board shows, before bays and headlines are
/// worked out, so they follow it:
/// - a finished turn whose reply asks you something is Needs you, with the question;
/// - a busy session gone quiet on a command that isn't waiting for typing is working on it,
///   not stuck;
/// - a command left running in the background says whether it polls or runs a job, is left
///   running when it's a server or watcher (the turn is over; the engine makes that *Your
///   turn* when you haven't seen it), and is stuck when it looks stuck.
pub fn apply_insight(s: &mut Session, i: &SessionInsight) {
    s.insight = Some(i.clone());
    if s.status == Status::Unread && i.asks_you.unwrap_or(0.0) >= ASKS_YOU {
        needs_your_answer(s);
    }
    if let (Some(since), Some(p)) = (s.stalled_since, i.waits_for_input) {
        if p < WAITS_FOR_INPUT {
            s.status = Status::Working;
            s.stalled_since = None;
            s.status_since = Some(s.status_since.unwrap_or(since).min(since));
            s.excerpt = s.excerpt.as_deref().map(|e| e.replacen("Stuck on", "Running", 1));
        }
    }
    if let (Some(cmd), Some(kind)) = (s.background.clone(), i.background.as_deref()) {
        let shown = code(&truncate(&one_line(&cmd, 200), 80));
        match kind {
            // Once its processes have sat still for a while, what the command looks like is moot.
            "polling" | "job" if s.status != Status::Working => {}
            "polling" => {
                let waits = s.reply_tail.as_deref().map(last_sentence).filter(|q| !q.is_empty());
                s.excerpt = Some(format!("Polling in the background · {}", waits.unwrap_or(shown)));
            }
            "job" => s.excerpt = Some(format!("Running in the background · {shown}")),
            "server" if i.background_p.unwrap_or(0.0) >= BACKGROUND_LEFT_RUNNING => {
                if s.status == Status::Working {
                    s.status = Status::Idle;
                }
                s.excerpt = Some(format!("Left running · {shown}"));
            }
            "stalled" if i.background_p.unwrap_or(0.0) >= BACKGROUND_STALLED => {
                s.status = Status::Needs;
                s.stalled_since = Some(s.status_since.unwrap_or(s.last_activity));
                s.excerpt = Some(format!("Stuck on {shown}"));
            }
            _ => {}
        }
    }
}

/// Makes a session whose turn ended on a reply that asks you something Needs you, with the
/// question.
fn needs_your_answer(s: &mut Session) {
    s.status = Status::Needs;
    s.asks = true;
    let question = s.reply_tail.as_deref().map(question_in).filter(|q| !q.is_empty());
    s.excerpt = Some(match question {
        Some(q) => format!("Asks you · {q}"),
        None => "Asks you something".to_string(),
    });
}

/// A session waiting on the agents its turn left running (`Session::agents`) whose reply asks you
/// something you haven't seen (its turn ended after `seen_at`) Needs you, as an unread turn's
/// does (`apply_insight`): it sits at its prompt, and its agents carry on whatever you answer.
/// After `apply_insight`, which doesn't know when you looked.
pub fn asks_while_agents_run(s: &mut Session, i: &SessionInsight, seen_at: i64) {
    if s.status == Status::Working && !s.agents.is_empty() && s.status_since.unwrap_or(0) > seen_at && i.asks_you.unwrap_or(0.0) >= ASKS_YOU {
        needs_your_answer(s);
    }
}

/// Whether a worktree looks finished: not the main checkout or a plain folder (an app's chats
/// among them), which aren't cleaned up, no PR that says where it stands already (a merged or
/// closed one has its own bay), nothing running or waiting, and every session of its own read
/// by Jev as finished.
/// Whether a worktree could take one piece of work after another: a repo's worktree in one of
/// your projects. A plain folder, an app's chats and a repo outside every project have no work
/// of yours to reuse them for.
pub fn can_reuse(seed: &LaneSeed) -> bool {
    !seed.elsewhere && !seed.chats && seed.origin != Origin::Folder
}

/// How many times a worktree must have taken new work to count as reused: once when it was
/// made, maybe once more when its work was redone.
const REUSED_AT: u32 = 3;

/// Whether a worktree has been reused: it took new work (another branch, or its branch reset)
/// three times or more, as far back as its reflog goes (`git::new_work`).
pub fn reused(git: Option<&GitInfo>) -> bool {
    git.and_then(|g| g.new_work_count).is_some_and(|n| n >= REUSED_AT)
}

/// Whether a worktree takes one piece of work after another: as marked (its menu's Reuse for
/// new work, reusable.rs), else a main checkout does, being where work starts when there's no
/// other, and so does one that has been reused; the rest are kept for one piece of work each.
pub fn reusable(seed: &LaneSeed, marks: &HashMap<String, bool>, git: Option<&GitInfo>) -> bool {
    can_reuse(seed) && marks.get(seed.path.to_string_lossy().as_ref()).copied().unwrap_or(seed.origin == Origin::Main || reused(git))
}

/// A worktree Settings suggests making reusable: marked neither way and not reusable already,
/// whose branch has carried two or more merged PRs among the repo's recent ones. A branch that
/// goes on from one merged PR to the next is a worktree taking one piece of work after another
/// (one repo's `app2`, reset onto `origin/main` after each), though a fresh clone's reflog
/// doesn't show it yet. PRs from anyone's fork count: a teammate's `app2` says the repo's
/// people name reused worktrees so.
pub fn looks_reusable(seed: &LaneSeed, marked: bool, git: Option<&GitInfo>, prs: &HashMap<String, Vec<PrRaw>>) -> bool {
    if marked || !can_reuse(seed) || seed.origin == Origin::Main || reused(git) {
        return false;
    }
    let (Some(slug), Some(branch)) = (seed.repo_slug.as_ref(), seed.branch.as_ref()) else { return false };
    if seed.default_branch.as_ref() == Some(branch) {
        return false;
    }
    let merged = |p: &&PrRaw| &p.head_ref == branch && p.state == PrState::Merged;
    prs.get(slug).map_or(0, |list| list.iter().filter(merged).count()) >= 2
}

/// Whether a session in a reusable worktree belongs to the work before its latest: it was last
/// active before the worktree last took new work (`git::reset_at`). One that needs you or is
/// working belongs to now, whenever it began.
pub fn earlier(s: &Session, reset_at: Option<i64>) -> bool {
    reset_at.is_some_and(|at| s.last_activity < at) && !matches!(s.status, Status::Needs | Status::Error | Status::Working)
}

/// What a reusable worktree holds, from its PR, its git state and its current work's sessions:
/// in review while its PR is open; open while an agent of its own is at work there or it has
/// uncommitted changes or commits that haven't landed; landed once its PR merged, until the
/// next work starts; otherwise free. A session idle there doesn't hold it: it can take the
/// next work, as another can.
pub fn reuse_of(pr: Option<&Pr>, git: Option<&GitInfo>, sessions: &[Session]) -> Reuse {
    if pr.is_some_and(|p| p.state.is_open()) {
        return Reuse::Review;
    }
    let at_work = sessions.iter().any(|s| s.visitor.is_none() && !s.archived && !s.earlier && matches!(s.status, Status::Working | Status::Needs | Status::Error));
    if at_work || git.is_some_and(|g| g.dirty > 0) {
        return Reuse::Open;
    }
    // A squash merge leaves the branch's commits off the default branch, so a merged PR on the
    // worktree's HEAD (`find_pr`) says they landed.
    if pr.is_some_and(|p| p.state == PrState::Merged) {
        return Reuse::Landed;
    }
    if git.and_then(|g| g.base_ahead).unwrap_or(0) > 0 {
        return Reuse::Open;
    }
    Reuse::Free
}

/// What decides whether a worktree can go (`cleanup`): as the board has it, or as it's read
/// again just before it goes (cleanup.rs).
#[derive(Debug, Clone, Copy)]
pub struct Leaving<'a> {
    pub origin: Origin,
    /// Codealong's own chats (`LaneSeed::starts_chats`).
    pub chats: bool,
    /// It's reusable (`reusable`), and whether you marked it so.
    pub reusable: Option<bool>,
    pub git: Option<&'a GitInfo>,
    /// How many sessions run in it: a process has them open.
    pub running: usize,
    /// A worktree inside its folder, by its name: removing this one would take that one's files.
    pub holds: Option<&'a str>,
    /// Its lock still holds (`lock_holds`), with the lock's reason.
    pub lock: Option<&'a str>,
}

/// Whether a worktree can be removed with nothing in it lost: no file uncommitted or untracked
/// (ignored ones, such as dependencies and env files, don't count), no commit that isn't on some
/// remote, no session running in it or lock held on it, and no worktree inside it. A main
/// checkout, a plain folder and a reusable worktree are kept whatever they hold. None while its
/// git hasn't been read, when nothing else has said it stays.
pub fn cleanup(w: &Leaving) -> Option<Cleanup> {
    let kept = |why: String| Some(Cleanup { ok: false, why });
    if w.origin == Origin::Main {
        return kept("the main checkout".into());
    }
    if w.origin == Origin::Folder || w.chats {
        return kept("a folder, not a worktree".into());
    }
    if let Some(marked) = w.reusable {
        return kept(if marked { "marked reusable" } else { "reusable: it has taken several pieces of work" }.into());
    }
    let mut why = Vec::new();
    match w.running {
        0 => {}
        1 => why.push("a session is running in it".to_string()),
        n => why.push(format!("{n} sessions are running in it")),
    }
    // A session's own lock goes without saying while it runs.
    if let (Some(reason), 0) = (w.lock, w.running) {
        why.push(match claude_lock(reason) {
            Some(_) => "locked by a running session".to_string(),
            None if reason.is_empty() => "locked".to_string(),
            None => format!("locked: {}", crate::util::truncate(reason, 60)),
        });
    }
    if let Some(inner) = w.holds {
        why.push(format!("another worktree is inside it ({inner})"));
    }
    if let Some(g) = w.git {
        match g.dirty {
            0 => {}
            1 => why.push("1 uncommitted file".into()),
            n => why.push(format!("{n} uncommitted files")),
        }
        match g.unpushed {
            Some(0) => {}
            Some(1) => why.push("1 commit on no remote".into()),
            Some(n) => why.push(format!("{n} commits on no remote")),
            // Git couldn't say: HEAD has no commit yet, or git failed.
            None => why.push("its commits couldn't be checked".into()),
        }
    }
    if !why.is_empty() {
        return kept(why.join(", "));
    }
    w.git.map(|_| Cleanup { ok: true, why: String::new() })
}

/// A lock Claude Code put on a worktree a session of its runs in, `claude session <name> (pid
/// <n> start <time>)`: the process that holds it, and when it started, as `ps -o lstart` writes
/// it in UTC ("Mon Oct 12 22:47:57 2026"), which tells it from another process given the same
/// number since. None for any other lock.
pub fn claude_lock(reason: &str) -> Option<(u32, Option<&str>)> {
    let rest = reason.trim().strip_prefix("claude session ")?;
    let (_, held) = rest.rsplit_once(" (pid ")?;
    let held = held.strip_suffix(')')?;
    let (pid, start) = match held.split_once(" start ") {
        Some((pid, start)) => (pid, Some(start.trim())),
        None => (held, None),
    };
    Some((pid.trim().parse().ok()?, start))
}

/// Whether a worktree's lock holds, as the board sees it: Claude Code's while the session that
/// took it runs (`live_pids`, the processes of the board's running sessions), any other always.
/// A session hung up on leaves its lock behind (`/exit` takes it off), which holds nothing.
pub fn lock_holds(reason: &str, live_pids: &HashSet<u32>) -> bool {
    claude_lock(reason).map_or(true, |(pid, _)| live_pids.contains(&pid))
}

pub fn looks_finished(origin: Origin, pr: Option<&Pr>, sessions: &[Session]) -> bool {
    if matches!(origin, Origin::Main | Origin::Folder) || pr.is_some_and(|p| !p.state.is_open()) {
        return false;
    }
    let own: Vec<&Session> = sessions.iter().filter(|s| s.visitor.is_none() && !s.archived && !s.unattended()).collect();
    !own.is_empty()
        && own.iter().all(|s| !matches!(s.status, Status::Working | Status::Needs) && s.insight.as_ref().and_then(|i| i.finished).is_some_and(|f| f >= FINISHED))
}

/* ---- Finished ---- */

/// A read that a reply may ask you something, from this sure, keeps its session open and
/// unfinished: well under `ASKS_YOU`, which makes it Needs you.
pub const MAY_ASK: f64 = 0.3;
/// A read of whether what you asked for is done says it isn't under this, whatever else is known.
pub const NOT_FINISHED: f64 = 0.5;

/// Whether a session's last reply may ask you something: by Jev's read of it (or an agent's tag)
/// when there's one, else by its having a question in it, Markdown's or one with more after it.
pub fn reply_asks(s: &Session) -> bool {
    match s.insight.as_ref().and_then(|i| i.asks_you) {
        Some(p) => p >= MAY_ASK,
        None => s.reply_tail.as_deref().is_some_and(has_question),
    }
}

/// What's known of a session's work besides the read of its last reply (`settle_finished`
/// gathers it): what `finished_of` weighs that read against.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Work {
    /// The PRs it worked on, as the PRs view lists it under them (`pr_items`): each one's
    /// number and state.
    pub prs: Vec<(u64, PrState)>,
    /// The todos linked to it that aren't archived: whether each is done.
    pub todos: Vec<bool>,
    /// Its worktree holds what would go with it (uncommitted files, commits on no remote), where
    /// the worktree is its alone: no main checkout or plain folder, which hold others' work too,
    /// and no other session of the work the worktree holds now.
    pub unsaved: bool,
    /// How far its change got by the first line of its last reply, where that says
    /// (`insights::landing_said`): the session's own word, read whether or not Jev reads
    /// sessions, which is the only time `SessionInsight::landing` has it.
    pub said: Option<&'static str>,
}

/// Where a session's work stands: finished, or the first thing that says it isn't
/// (`standing_of`). `finished_of` is its `Finished`, and a session marked done is noted with the
/// rest (dismissed.rs), which is how you're told which ones you left unfinished.
#[derive(Debug, Clone, PartialEq)]
pub enum Standing {
    /// Not yours to weigh: a helper's work or a headless run's is its starter's, and an archived
    /// session is behind you already.
    NotYours,
    /// Its turn is running.
    Working,
    /// It waits on you to allow a call or to answer a question it put to you.
    Needs,
    /// It stopped on an error.
    Error,
    /// Its last reply asks you something.
    Asks,
    /// Its harness cut its turn short: a usage limit, a lost connection, a hangup.
    CutShort,
    /// Something of its is still to come: a command or agents left running, a message queued, a
    /// snooze.
    ToCome,
    /// Its reply hands you a next step or waits on what it started.
    LeavesStep,
    /// Its change hasn't landed: blocked, uncommitted, or committed and not pushed.
    NotLanded,
    /// The read of its reply says what you asked for isn't done.
    NotDone,
    /// A PR it worked on is still open.
    OpenPr,
    /// A todo linked to it isn't done.
    OpenTodo,
    /// Its own worktree holds uncommitted files or commits on no remote.
    Unsaved,
    /// Nothing says either way: no more finished than not.
    Unknown,
    /// Finished, for sure or by the read of its reply (`Finished::sure`).
    Finished(Finished),
}

/// Every standing's word, for the tests that hold each to a sentence.
#[cfg(test)]
pub const STANDINGS: [&str; 16] = ["not-yours", "working", "needs", "error", "asks", "cut-short", "to-come", "step", "not-landed", "not-done", "open-pr", "open-todo", "unsaved", "unknown", "looks-finished", "finished"];

impl Standing {
    /// Its word, as the ledger keeps it; a finished one's is `finished` when something besides
    /// its reply says so, else `looks-finished`.
    pub fn word(&self) -> &'static str {
        match self {
            Standing::NotYours => "not-yours",
            Standing::Working => "working",
            Standing::Needs => "needs",
            Standing::Error => "error",
            Standing::Asks => "asks",
            Standing::CutShort => "cut-short",
            Standing::ToCome => "to-come",
            Standing::LeavesStep => "step",
            Standing::NotLanded => "not-landed",
            Standing::NotDone => "not-done",
            Standing::OpenPr => "open-pr",
            Standing::OpenTodo => "open-todo",
            Standing::Unsaved => "unsaved",
            Standing::Unknown => "unknown",
            Standing::Finished(f) if f.sure => "finished",
            Standing::Finished(_) => "looks-finished",
        }
    }

    /// How a session marked done stood, as a sentence says it: "It was still working".
    pub fn said(word: &str) -> &'static str {
        match word {
            "working" => "It was still working",
            "needs" => "It was waiting on you to allow a call or answer it",
            "error" => "It had stopped on an error",
            "asks" => "Its last reply asked you something",
            "cut-short" => "Its turn had been cut short",
            "to-come" => "It had work still to come",
            "step" => "Its reply left you a next step",
            "not-landed" => "Its change hadn't landed",
            "not-done" => "Its reply read as not done",
            "open-pr" => "A PR it worked on was still open",
            "open-todo" => "A todo linked to it wasn't done",
            "unsaved" => "Its worktree held work that wasn't saved",
            "finished" => "Finished",
            "looks-finished" => "It looked finished",
            "not-yours" => "It worked for another session",
            _ => "Nothing said whether it was finished",
        }
    }

    /// Whether a standing's word is one of work left undone: not finished, and not merely
    /// unknown.
    pub fn unfinished(word: &str) -> bool {
        !matches!(word, "finished" | "looks-finished" | "unknown" | "not-yours")
    }
}

/// Where a session's work stands (`Standing`), for a session of yours.
///
/// It isn't finished while its turn runs or waits on you, when its reply may ask you something
/// (`reply_asks`), hands you a next step or waits on what it started (`SessionInsight::ending`:
/// only a report or an offer of more leaves nothing), says the work goes on at a later time
/// (`SessionInsight::waits_until`), says its change didn't land or that it couldn't finish (the
/// read's `landing`, else its own status line, `Work::said`), or reads as not done
/// (`NOT_FINISHED`); nor while a PR it worked on is open, a todo linked to it isn't done, or its
/// own worktree holds what would be lost.
///
/// Then it's finished for sure when something besides the reply says so: a PR it worked on
/// merged, a todo linked to it done, or its status line saying the change landed. With none of
/// those it looks finished when the read of its reply says what you asked for is done
/// (`FINISHED`), and otherwise nothing is known: a session nothing is known of is no more
/// finished than not.
pub fn standing_of(s: &Session, work: &Work) -> Standing {
    if s.archived || s.helps_an_agent() || s.headless {
        return Standing::NotYours;
    }
    if s.asks {
        return Standing::Asks;
    }
    match s.status {
        Status::Working => return Standing::Working,
        Status::Needs => return Standing::Needs,
        Status::Error => return Standing::Error,
        Status::Unread | Status::Idle | Status::Stopped => {}
    }
    if s.limit.is_some() || s.connection_lost.is_some() || s.cut_off.is_some() {
        return Standing::CutShort;
    }
    if s.background.is_some() || s.running.is_some() || s.stalled_since.is_some() || s.queued_at.is_some() || !s.queued.is_empty() || s.snoozed_until.is_some() {
        return Standing::ToCome;
    }
    if reply_asks(s) {
        return Standing::Asks;
    }
    let read = s.insight.as_ref();
    if read.and_then(|i| i.ending.as_deref()).is_some_and(|e| !matches!(e, "report" | "offer")) {
        return Standing::LeavesStep;
    }
    // The read's landing has the status line under an agent's tag of it; with no read of it, the
    // line itself.
    let landing = read.and_then(|i| i.landing.as_deref()).or(work.said);
    // Work put off to a later time isn't done, however the reply reads: not till then, and not
    // once the time has come, when it's due.
    if read.is_some_and(|i| i.waits_until.is_some()) {
        return Standing::ToCome;
    }
    if landing.is_some_and(|l| matches!(l, "blocked" | "uncommitted" | "committed")) {
        return Standing::NotLanded;
    }
    let done = read.and_then(|i| i.finished);
    if done.is_some_and(|p| p < NOT_FINISHED) {
        return Standing::NotDone;
    }
    if work.prs.iter().any(|(_, state)| state.is_open()) {
        return Standing::OpenPr;
    }
    if work.todos.contains(&false) {
        return Standing::OpenTodo;
    }
    if work.unsaved {
        return Standing::Unsaved;
    }
    let mut why = Vec::new();
    let merged: Vec<u64> = work.prs.iter().filter(|(_, state)| *state == PrState::Merged).map(|(n, _)| *n).collect();
    match merged.as_slice() {
        [] => {}
        [n] => why.push(format!("#{n} merged")),
        [a, b] => why.push(format!("#{a} and #{b} merged")),
        many => why.push(format!("{} PRs merged", many.len())),
    }
    match work.todos.len() {
        0 => {}
        1 => why.push("its todo is done".to_string()),
        n => why.push(format!("its {n} todos are done")),
    }
    // A status line an agent's tag reads otherwise isn't its last word on the change.
    if work.said == Some("landed") && landing == Some("landed") {
        why.push("it says its change landed".to_string());
    }
    let sure = !why.is_empty();
    if done.is_some_and(|p| p >= FINISHED) {
        why.push(match read.and_then(|i| i.by.as_ref().map(|_| i.by_title.as_deref().unwrap_or("an agent"))) {
            Some(by) => format!("{by} read it as done"),
            None => "its reply reads as done".to_string(),
        });
    }
    if why.is_empty() {
        Standing::Unknown
    } else {
        Standing::Finished(Finished { sure, why })
    }
}

/// Whether a session's work is done with nothing left for you, and why: `standing_of`'s
/// `Finished`, for a session of yours whose turn ended as turns do, with nothing left running,
/// queued or put off.
pub fn finished_of(s: &Session, work: &Work) -> Option<Finished> {
    match standing_of(s, work) {
        Standing::Finished(f) => Some(f),
        _ => None,
    }
}

/// What's known of each session's work besides the read of its last reply, from the PRs that list
/// it, the todos linked to it and its worktree: by its key, for the sessions a worktree lists as
/// its own.
pub fn works(lanes: &[Lane], prs: &[PrItem], todos: &[crate::todos::Todo]) -> HashMap<String, Work> {
    let mut worked: HashMap<&str, Vec<(u64, PrState)>> = HashMap::new();
    for p in prs {
        for s in &p.sessions {
            worked.entry(s.key.as_str()).or_default().push((p.number, p.state));
        }
    }
    let mut linked: HashMap<&str, Vec<bool>> = HashMap::new();
    for t in todos.iter().filter(|t| t.archived_at.is_none()) {
        for key in &t.sessions {
            linked.entry(key.as_str()).or_default().push(t.status == crate::todos::TodoStatus::Done);
        }
    }
    let mut found = HashMap::new();
    for lane in lanes.iter() {
        let own = |s: &&Session| s.visitor.is_none() && !s.archived && !s.earlier && !s.unattended();
        let alone = !matches!(lane.origin, Origin::Main | Origin::Folder) && !lane.chats && lane.sessions.iter().filter(own).count() == 1;
        let holds = alone && lane.git.as_ref().is_some_and(|g| g.dirty > 0 || g.unpushed.unwrap_or(0) > 0);
        for s in lane.sessions.iter().filter(|s| s.visitor.is_none()) {
            found.insert(
                s.key.clone(),
                Work {
                    prs: worked.get(s.key.as_str()).cloned().unwrap_or_default(),
                    todos: linked.get(s.key.as_str()).cloned().unwrap_or_default(),
                    unsaved: holds && own(&s),
                    said: crate::insights::landing_said(s),
                },
            );
        }
    }
    found
}

/// Where the session `key` stands on a board (`standing_of`), as it's listed in its own worktree;
/// None for a session the board doesn't list.
pub fn standing_on(lanes: &[Lane], prs: &[PrItem], todos: &[crate::todos::Todo], key: &str) -> Option<Standing> {
    let s = lanes.iter().flat_map(|l| &l.sessions).find(|s| s.key == key && s.visitor.is_none())?;
    let works = works(lanes, prs, todos);
    Some(standing_of(s, works.get(key).unwrap_or(&Work::default())))
}

/// Says of each session on the board whether it's finished (`finished_of`), from the PRs that
/// list it, the todos linked to it and its worktree, once the board has all three; a session
/// that only visits a worktree is as finished there as where it's listed. Then puts each
/// worktree's own sessions back in their order, which has the finished turns after the others
/// (`sort_sessions`), and says the worktree's sentence again where a turn of its waits on you:
/// it's the first one's.
pub fn settle_finished(lanes: &mut [Lane], prs: &[PrItem], todos: &[crate::todos::Todo]) {
    let works = works(lanes, prs, todos);
    let mut found: HashMap<String, Finished> = HashMap::new();
    for s in lanes.iter().flat_map(|l| &l.sessions).filter(|s| s.visitor.is_none()) {
        if let Some(f) = works.get(&s.key).and_then(|work| finished_of(s, work)) {
            found.insert(s.key.clone(), f);
        }
    }
    for lane in lanes.iter_mut() {
        let mut moved = false;
        for s in &mut lane.sessions {
            let now = found.get(&s.key).cloned();
            moved |= now.is_some() != s.finished.is_some();
            s.finished = now;
        }
        if !moved {
            continue;
        }
        let current = lane.sessions.iter().take_while(|s| s.visitor.is_none() && !s.archived && !s.earlier && !s.unattended()).count();
        sort_sessions(&mut lane.sessions[..current]);
        if lane.bay == Bay::Unread {
            (lane.headline, lane.detail) = headline(lane.bay, &lane.sessions[..current], lane.pr.as_ref(), lane.git.as_ref());
        }
    }
}

/// A session's own sentence. See *The model*, Headlines.
pub fn session_headline(s: &Session) -> (String, Option<String>) {
    let acct = code(&s.account);
    if s.need() == Some(Need::Stuck) {
        return (format!("{acct} is stuck · {}", s.title), s.excerpt.clone());
    }
    let title = &s.title;
    match s.status {
        // Every headline names the session: its title after what it's doing. A question, whether
        // its reply ends in one or it's held through the plugin (`claude::asked_label`), says so
        // up front and leaves the question itself below.
        // Set aside, it waits on its side conversation before it waits on you (`apply_aside`).
        Status::Needs if s.aside.is_some() => (format!("{acct} waits on its side conversation · {title}"), s.excerpt.as_deref().and_then(|e| e.strip_prefix(ASKING_ON_THE_SIDE)).map(str::to_string)),
        Status::Needs => match (s.need(), s.harness) {
            (Some(Need::Reply | Need::Question), _) => (format!("{acct} asks you · {title}"), s.excerpt.as_deref().and_then(|e| e.strip_prefix("Asks you · ")).map(str::to_string)),
            (Some(Need::Message), _) => (format!("{acct} holds a message for you · {title}"), s.excerpt.as_deref().and_then(|e| e.strip_prefix("Held message ")).map(str::to_string)),
            // A command it runs stopped at a question in its terminal: what it asks below.
            (Some(Need::Input), _) => (format!("{acct}'s command asks you · {title}"), s.excerpt.as_deref().and_then(|e| e.strip_prefix(TERMINAL_ASKS)).map(str::to_string)),
            (Some(Need::Approval), Harness::Claude) => (format!("{acct} is waiting for approval · {title}"), s.excerpt.clone()),
            (Some(Need::Approval), Harness::Codex | Harness::Opencode | Harness::Antigravity | Harness::Cursor) => (format!("{acct} asks for approval · {title}"), s.excerpt.clone()),
            (Some(Need::Stuck | Need::Other) | None, _) => (format!("{acct} needs you · {title}"), None),
        },
        Status::Error if s.limit.is_some() => (format!("{acct} hit its usage limit · {title}"), s.excerpt.clone()),
        Status::Error if s.cut_off.is_some() => (format!("{acct} was cut off · {title}"), s.excerpt.clone()),
        Status::Error => (format!("{acct} stopped with an error · {title}"), s.excerpt.clone()),
        Status::Working => {
            // How long it has worked on what you last asked. Claude Code doesn't restart a busy
            // session's clock for a message sent while it works, but that message is a new ask.
            let since = s.status_since.map(|t| t.max(s.last_prompt_at));
            let detail = match (since, &s.excerpt) {
                (Some(t), Some(ex)) => Some(format!("{{for:{t}}} · {ex}")),
                (Some(t), None) => Some(format!("{{for:{t}}}")),
                (None, ex) => ex.clone(),
            };
            (format!("{acct} · {}", s.title), detail)
        }
        // Its glyph and its bay (Your turn) already say it finished, and the word on every row
        // there was noise: the title stands alone, as a working session's does.
        Status::Unread => (format!("{acct} · {}", s.title), s.excerpt.clone()),
        Status::Idle => (format!("{acct} · {}", s.title), Some(format!("last turn {{ago:{}}}", s.last_activity))),
        Status::Stopped => (format!("{acct} · {}", s.title), Some(format!("stopped {{ago:{}}}", s.last_activity))),
    }
}

/// The lane's one sentence, from the rule that picked its bay. See *The model*, Headlines.
pub fn headline(bay: Bay, sessions: &[Session], pr: Option<&Pr>, git: Option<&GitInfo>) -> (String, Option<String>) {
    let acct = |s: &Session| code(&s.account);
    let first = |st: Status| sessions.iter().find(|s| s.status == st);
    match bay {
        Bay::Needs => session_headline(first(Status::Needs).or_else(|| first(Status::Error)).expect("needs bay has an agent that needs you")),
        Bay::Working => session_headline(first(Status::Working).expect("working bay has a working session")),
        Bay::Unread => session_headline(first(Status::Unread).expect("unread bay has an unread session")),
        Bay::Review => {
            let pr = pr.expect("review bay has a PR");
            let c = &pr.checks;
            let n = pr.number;
            // Failing checks are what the PR needs from you first; the detail names them.
            if c.failed > 0 && !c.failing.is_empty() {
                let mut names: Vec<String> = c.failing.iter().take(3).map(|n| code(n)).collect();
                if c.failing.len() > 3 {
                    names.push(format!("+{} more", c.failing.len() - 3));
                }
                return (format!("#{n} · checks failing"), Some(names.join(" · ")));
            }
            // Ready to merge is GitHub's word for it: the reviews the repo requires, passing
            // checks and no conflicts. Until GitHub has worked that out, an approval is only that.
            let head = match pr.state {
                PrState::Draft => format!("#{n} is a draft"),
                _ if pr.merge == Merge::Conflicts => format!("#{n} has conflicts"),
                _ if pr.merge == Merge::Behind => format!("#{n} · branch out of date"),
                PrState::Changes => format!("#{n} · changes requested"),
                _ if c.failed > 0 => format!("#{n} · checks failing"),
                _ if c.pending > 0 && pr.state == PrState::Approved => format!("#{n} approved · checks running"),
                _ if c.pending > 0 => format!("#{n} · checks running"),
                PrState::Approved if pr.merge == Merge::Clean => format!("#{n} approved · ready to merge"),
                PrState::Approved => format!("#{n} approved"),
                _ if pr.merge == Merge::Clean => format!("#{n} ready to merge"),
                _ => format!("#{n} waiting for review"),
            };
            (head, Some(checks_summary(c)))
        }
        Bay::Merged => {
            let pr = pr.expect("merged bay has a PR");
            let (verb, when) = match pr.merged_at {
                Some(t) => ("merged", t),
                None => ("closed", pr.closed_at.unwrap_or(pr.updated_at)),
            };
            let detail = match sessions.iter().find(|s| s.live) {
                Some(s) => format!("{} is still open", acct(s)),
                None => "No agents running".to_string(),
            };
            (format!("#{} {verb} {{ago:{when}}}", pr.number), Some(detail))
        }
        Bay::Idle => {
            if let Some(s) = sessions.iter().max_by_key(|s| s.last_activity) {
                let (h, _) = session_headline(s);
                return (h, Some(format!("last turn {{ago:{}}}", s.last_activity)));
            }
            match git {
                Some(g) if g.dirty > 0 => ("Uncommitted changes".to_string(), Some(format!("{} files", g.dirty))),
                Some(g) => (
                    "No agents".to_string(),
                    g.last_commit.as_ref().map(|c| match g.last_commit_at {
                        Some(t) => format!("Last commit {{ago:{t}}} · {}", plain(&truncate(c, 70))),
                        None => plain(&truncate(c, 70)),
                    }),
                ),
                None => ("No agents".to_string(), None),
            }
        }
    }
}

pub fn target(id: String, icon: &str, label: String, detail: Option<String>, kbd: Option<&[&str]>, disabled: bool, group: &str) -> Target {
    Target {
        id,
        icon: icon.to_string(),
        label,
        detail,
        kbd: kbd.map(|k| k.iter().map(|s| s.to_string()).collect()),
        disabled,
        group: group.to_string(),
    }
}

/// The session a lane opens to: its most recent own session, else the most recent one that
/// visited it. A tie goes to the one listed first, the more urgent.
fn latest_session(sessions: &[Session]) -> Option<&Session> {
    let newest = |visitors: bool| sessions.iter().filter(|s| s.visitor.is_some() == visitors).min_by_key(|s| Reverse(s.last_activity));
    newest(false).or_else(|| newest(true))
}

/// Everywhere a lane can open, the default first. See *Surfaces*, Open where it lives.
/// A worktree's app: the URL set for it and the server that serves it. The server is the one on
/// the port given with the URL, or the port a local URL names; with neither, the one serving a
/// page (the lowest port, when several do). With no URL set, a page served in the worktree is
/// its app, at `localhost`. The session is the one whose command started the server: one of its
/// parent processes is the session's. It opens in the Chrome profile set for the worktree, or
/// else in its project's.
pub fn lane_preview(set: Option<&crate::previews::Entry>, servers: &[&crate::previews::Server], sessions: &[Session], project_profile: Option<&str>) -> Option<Preview> {
    let port = set.and_then(|e| e.port.or_else(|| crate::previews::local_port(&e.url)));
    let server = match port {
        Some(p) => servers.iter().find(|s| s.port == p),
        None => servers.iter().filter(|s| s.page).min_by_key(|s| s.port),
    };
    let server = server.map(|s| PreviewServer {
        pid: s.pid,
        port: s.port,
        name: s.name.clone(),
        title: s.title.clone(),
        started_at: s.started_ms,
        session: sessions.iter().find(|x| x.pid.is_some_and(|p| s.ancestors.contains(&p))).map(|x| x.key.clone()),
    });
    let own = set.and_then(|e| e.chrome_profile.clone());
    let chrome_profile_set = own.is_some();
    let chrome_profile = own.or_else(|| project_profile.map(str::to_string));
    match set {
        Some(e) => Some(Preview { url: e.url.clone(), set: true, port: e.port, server, chrome_profile, chrome_profile_set }),
        None => server.map(|s| Preview { url: format!("http://localhost:{}", s.port), set: false, port: None, server: Some(s), chrome_profile, chrome_profile_set }),
    }
}

/// The worktree on this Mac whose app a web link is to, when its preview opens in a Chrome
/// profile: the link opens there too, where the app has its sign-in, and any other link opens
/// in the default browser. Another device's previews open on that device, in its own profiles.
pub fn app_of_link<'a>(lanes: &'a [Lane], url: &str) -> Option<&'a Lane> {
    lanes.iter().filter(|l| l.device.is_none()).find(|l| {
        l.preview.as_ref().is_some_and(|p| p.chrome_profile.is_some() && crate::previews::is_app_link(url, &p.url, p.server.as_ref().map(|s| s.port).or(p.port)))
    })
}

/// The sessions on this Mac that made the page a web link is to (`Session.artifacts`), the one
/// that last did something with it first: its account's Chrome profile is where you're signed in
/// to see it (`engine::link_opens`). Its harness says which links are to it (`Adapter::same_page`:
/// a Claude artifact has two addresses). Another device's sessions are its accounts', not this
/// Mac's.
pub fn sessions_with_page<'a>(lanes: &'a [Lane], url: &str) -> Vec<&'a Session> {
    let mut found: Vec<(&Session, i64)> = lanes
        .iter()
        .filter(|l| l.device.is_none())
        .flat_map(|l| &l.sessions)
        .filter_map(|s| {
            let harness = crate::harness::adapter(s.harness);
            s.artifacts.iter().find(|a| harness.same_page(&a.url, url)).map(|a| (s, a.at))
        })
        .collect();
    found.sort_by_key(|(_, at)| Reverse(*at));
    found.into_iter().map(|(s, _)| s).collect()
}

/// What a session's Chrome item says: Show in Chrome while one of its tabs is open, else Reopen
/// with the site of its last page, and the profile when Chrome has more than one. `chromeLabel`
/// in `src/open.ts` says the same.
pub fn chrome_label(c: &ChromeTabs) -> String {
    let what = if c.tabs.is_empty() { format!("Reopen {} in Chrome", site(&c.url)) } else { "Show in Chrome".to_string() };
    match &c.profile {
        Some(p) => format!("{what} ({p})"),
        None => what,
    }
}

/// A web address's host, without `www.`.
fn site(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let host = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    host.strip_prefix("www.").unwrap_or(host)
}

pub fn targets(seed: &LaneSeed, sessions: &[Session], pr: Option<&Pr>, preview: Option<&Preview>, apps: &AppNames) -> (Vec<Target>, Option<String>) {
    let term = &apps.terminal;
    let editor = &apps.editor;
    let mut t = Vec::new();
    // The most recent session leads, since opening the lane opens it; the rest keep the lane's
    // order, most urgent first.
    let latest = latest_session(sessions);
    let rest = sessions.iter().filter(|s| latest.map_or(true, |l| l.key != s.key));
    let shown: Vec<&Session> = latest.into_iter().chain(rest).take(4).collect();
    for (i, s) in shown.iter().enumerate() {
        let first = i == 0;
        let enter: Option<&[&str]> = if first { Some(&["⏎"]) } else { None };
        let who = format!("{} · {}", s.account, truncate(&s.title, 48));
        let respond = s.status == Status::Needs;
        if s.hosted {
            // Codealong runs it: its panel is where it's read, sent to and answered.
            let verb = if respond { "Respond in" } else { "Show in" };
            t.push(target(format!("s|{}|hosted", s.key), "app", format!("{verb} Codealong"), Some(who), enter, false, "session"));
            continue;
        }
        // Each harness says where its sessions open (`Adapter::targets`).
        let opens = crate::harness::Opens { term, who, first, respond };
        t.extend(crate::harness::adapter(s.harness).targets(s, &opens));
    }
    // Their tabs in Chrome, apart from the ways to open them: ⌘⏎ and the lane's default are those.
    for s in &shown {
        if let Some(c) = &s.chrome {
            let page = if c.title.is_empty() { crate::previews::short_url(&c.url) } else { c.title.as_str() };
            let detail = format!("{} · {} · {}", truncate(page, 48), s.account, truncate(&s.title, 48));
            t.push(target(format!("s|{}|chrome", s.key), "world", chrome_label(c), Some(detail), None, false, "browser"));
        }
    }
    let here = tilde(&seed.path);
    t.push(target("l|terminal".into(), "terminal", format!("New {term} terminal here"), Some(here), Some(&["⌘", "1"]), false, "lane"));
    t.push(target("l|editor".into(), "code", format!("Open in {editor}"), None, Some(&["⌘", "2"]), false, "lane"));
    if let Some(p) = preview {
        let at = crate::previews::short_url(&p.url);
        let mut detail = match &p.server {
            Some(s) => format!("{at} · {} on port {}", s.name, s.port),
            None => format!("{at} · not running"),
        };
        if let Some(profile) = &p.chrome_profile {
            detail.push_str(&format!(" · Chrome's {profile} profile"));
        }
        t.push(target("l|preview".into(), "world", "Open preview".into(), Some(detail), Some(&["⌘", "5"]), false, "lane"));
    }
    t.push(target("l|finder".into(), "folder", "Reveal in Finder".into(), None, None, false, "lane"));
    if let Some(pr) = pr {
        t.push(target("p|github".into(), "external", format!("Open #{} on GitHub", pr.number), None, Some(&["⌘", "O"]), false, "pr"));
    }
    // The main action pulls up the most recent session where it lives; with no sessions, a
    // terminal at the lane. The editor is always secondary.
    let default = t
        .iter()
        .find(|x| x.group == "session" && !x.disabled)
        .map(|x| x.id.clone())
        .or_else(|| Some("l|terminal".to_string()));
    (t, default)
}

pub fn compose(inp: Inputs) -> Board {
    let Inputs { now, apps, seeds, mut sessions, visitors, git, prs, requested, made, project_order, notices, loading, github_at, pinned, github_login, pr_links, previews, reusable: marks, review_snoozes, auto_reviews, session_settings, machines } = inp;
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
        let reuse_on = reusable(seed, marks, g.as_ref());
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
        let looks_reusable = looks_reusable(seed, mark.is_some(), g.as_ref(), prs);
        let cleanup = cleanup(&Leaving {
            origin: seed.origin,
            chats: seed.chats || seed.starts_chats,
            reusable: reuse_on.then_some(mark == Some(true)),
            git: g.as_ref(),
            running,
            // Looked for only where it would be said: a main checkout holds the others.
            holds: (seed.origin != Origin::Main && !reuse_on).then(|| seeds.iter().find(|o| o.path != seed.path && o.path.starts_with(&seed.path))).flatten().map(|o| o.callsign.as_str()),
            lock: g.as_ref().and_then(|g| g.locked.as_deref()).filter(|reason| lock_holds(reason, &live_pids)),
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
    crate::autoreview::asked_again(&mut pr_list, auto_reviews.0, &polled_slugs(seeds), machines);
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
        mcp: Vec::new(),
        sides: Default::default(),
        terminals: Default::default(),
        listenings: Default::default(),
        meeting: None,
        pinned: pinned.to_vec(),
        pinned_worktrees: Vec::new(),
        pinned_prs: Vec::new(),
        pinned_repos: Vec::new(),
        prs: pr_list,
        github_login: github_login.map(str::to_string),
        theme: Default::default(),
        zoom: crate::zoom::ACTUAL,
        perf_log: false,
        todos: Vec::new(),
        todos_unreadable: false,
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
        jev: Default::default(),
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
    }
}

pub use codealong_peer::model::ELSEWHERE;

/// Match the floating bar's scope: all visible projects, without archived sessions, duplicate
/// visitors, or helpers and headless runs that don't need you. An open review request you
/// haven't snoozed also needs the user's attention.
pub fn logo_status<'a>(lanes: impl IntoIterator<Item = &'a Lane>, prs: &[PrItem]) -> LogoStatus {
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
    let review = prs.iter().any(PrItem::waits_on_review);
    sessions.chain(review.then_some(LogoStatus::Needs)).min().unwrap_or_default()
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

    const NOW: i64 = 1_790_200_000_000;

    fn acct(name: &str) -> AccountCfg {
        AccountCfg { name: name.into(), harness: "claude".into(), home: format!("~/.{name}"), command: Some(name.into()), start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false, chrome_profile: None }
    }

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
    fn a_listening_sessions_finished_turns_arent_yours_while_it_hears() {
        let of = |status: Status, hearing: bool| {
            let mut s = Session { status, ..crate::fixtures::session("k") };
            apply_listening(&mut s, hearing);
            s.status
        };
        assert_eq!(of(Status::Unread, true), Status::Idle);
        // An answer that reads as a question for you is a cue, shown as its answers are.
        let mut asks = Session { status: Status::Needs, asks: true, ..crate::fixtures::session("k") };
        apply_listening(&mut asks, true);
        assert_eq!((asks.status, asks.asks), (Status::Idle, false));
        // What needs you at a prompt still does, and a turn under way still shows as one.
        for status in [Status::Needs, Status::Error, Status::Working, Status::Idle] {
            assert_eq!(of(status, true), status);
        }
        // Once hearing has ended, its turns are yours again.
        assert_eq!(of(Status::Unread, false), Status::Unread);
        // That it's hearing is said on it, and no longer once it isn't; sent only when it is.
        let mut s = crate::fixtures::session("k");
        assert!(!serde_json::to_value(&s).unwrap().as_object().unwrap().contains_key("listening"));
        apply_listening(&mut s, true);
        assert_eq!(serde_json::to_value(&s).unwrap()["listening"], true);
        apply_listening(&mut s, false);
        assert!(!s.listening);
    }

    fn reg(status: &str, updated: i64) -> RegistryEntry {
        RegistryEntry {
            pid: 1,
            session_id: "abc1234567".into(),
            cwd: "/r/spark-portal/apps/server".into(),
            status: status.into(),
            kind: Some("interactive".into()),
            job_id: None,
            parked_job: None,
            spare: false,
            entrypoint: Some("cli".into()),
            name: Some("spark-portal-8f".into()),
            name_source: Some("derived".into()),
            started_at: Some(NOW - 3_600_000),
            status_updated_at: Some(updated),
            ..Default::default()
        }
    }

    fn tr() -> TranscriptInfo {
        TranscriptInfo {
            session_id: "abc1234567".into(),
            cwd: "/r/spark-portal/apps/server".into(),
            title: Some("Portal daily log + photo sync".into()),
            pending_tool: Some("Bash · `yarn mikro-orm migration:up`".into()),
            last_tool: Some("running `yarn test`".into()),
            last_text: Some("Done.".into()),
            last_ts: NOW - 60_000,
            ..Default::default()
        }
    }

    fn seed(path: &str, branch: &str) -> LaneSeed {
        LaneSeed {
            path: PathBuf::from(path),
            project: "Spruce".into(),
            repo: "spark".into(),
            repo_slug: Some("sprucehq/spark".into()),
            head_owners: vec!["sprucehq".into(), "gwen".into()],
            callsign: "portal".into(),
            origin: Origin::Script,
            branch: Some(branch.into()),
            head: Some("0ef4ed2".into()),
            default_branch: Some("main".into()),
            elsewhere: false,
            chats: false,
            starts_chats: false,
            exact: false,
        }
    }

    fn pr(state: PrState, failed: u32) -> PrRaw {
        PrRaw {
            number: 1037,
            title: "Let the agent add and caption project photos".into(),
            url: "https://github.com/sprucehq/spark/pull/1037".into(),
            state,
            head_ref: "portal".into(),
            head_owner: Some("gwen".into()),
            head_oid: Some("0ef4ed2".into()),
            updated_at: NOW - 3_600_000,
            merged_at: if state == PrState::Merged { Some(NOW - 7_200_000) } else { None },
            closed_at: None,
            checks: Checks { passed: 11, failed, failing: (0..failed).map(|i| format!("check-{i}")).collect(), ..Default::default() },
            merge: Merge::Unknown,
            author: Some("gwen".into()),
            base_ref: Some("main".into()),
            created_at: NOW - 86_400_000,
            additions: 120,
            deletions: 8,
            changed_files: 6,
            review_requested: false,
            requested_by: None,
            requested_at: None,
            reviewers_asked: Vec::new(),
            assigned: None,
            authored_by: None,
            labels: Vec::new(),
            last_review: None,
            stale_approval: None,
            head_at: None,
            said: Vec::new(),
            said_from: None,
            changes_asked: Vec::new(),
        }
    }

    fn wrote(who: &str, at: i64) -> crate::github::Said {
        crate::github::Said { who: who.into(), at, asked: None, url: None }
    }

    fn asked(who: &str, reviewer: &str, at: i64) -> crate::github::Said {
        crate::github::Said { who: who.into(), at, asked: Some(reviewer.into()), url: None }
    }

    /// A PR of gwen's that ken asked changes of at 100, at commit `aaa`: his findings at 99, the
    /// request at 100.
    fn changes_asked() -> PrRaw {
        PrRaw {
            head_oid: Some("aaa".into()),
            head_at: Some(90),
            said: vec![wrote("ken", 99), wrote("ken", 100)],
            changes_asked: vec![crate::github::ChangesAsked { who: "ken".into(), at: 100, commit: Some("aaa".into()) }],
            ..pr(PrState::Changes, 0)
        }
    }

    fn kind(p: &PrRaw, mine: bool) -> Option<(TurnKind, String, Option<i64>)> {
        turn_of(p, mine, Some(if mine { "gwen" } else { "me" }), &HashSet::from(["agents-post".to_string()])).map(|t| (t.kind, t.who, t.since))
    }

    #[test]
    fn a_push_after_a_request_for_changes_that_nobody_said_is_yours_to_say() {
        use TurnKind::*;
        let p = changes_asked();
        // Nothing since the request: it says itself.
        assert_eq!(kind(&p, true), None);
        // The fixes pushed and nothing said, as on a PR between 07:04 and 07:07 on 2026-10-22.
        let pushed = PrRaw { head_oid: Some("bbb".into()), head_at: Some(110), ..p.clone() };
        assert_eq!(kind(&pushed, true), Some((ReplyNeeded, "ken".into(), Some(110))));
        // Said before the push, not after it: still yours.
        let before = PrRaw { said: [p.said.clone(), vec![wrote("gwen", 105)]].concat(), ..pushed.clone() };
        assert_eq!(kind(&before, true), Some((ReplyNeeded, "ken".into(), Some(110))));
        // Asked again after it, or written to: his move.
        let again = PrRaw { said: [pushed.said.clone(), vec![asked("gwen", "ken", 112)]].concat(), ..pushed.clone() };
        assert_eq!(kind(&again, true), Some((RereviewAsked, "ken".into(), Some(112))));
        let told = PrRaw { said: [pushed.said.clone(), vec![wrote("gwen", 115)]].concat(), ..pushed.clone() };
        assert_eq!(kind(&told, true), Some((RereviewAsked, "ken".into(), Some(115))));
        // Still asked, however it stands: his Codealong has it waiting on him.
        let waiting = PrRaw { reviewers_asked: vec!["Ken".into()], ..pushed.clone() };
        assert_eq!(kind(&waiting, true), Some((RereviewAsked, "ken".into(), None)));
        // He wrote again after that ("all fixed"), so it's yours: ask for the approval.
        let replied = PrRaw { said: [told.said.clone(), vec![wrote("ken", 120)]].concat(), ..told.clone() };
        assert_eq!(kind(&replied, true), Some((ReviewerReplied, "ken".into(), Some(120))));
        // A review whose commit GitHub doesn't say can't be told a push came after.
        let unknown = PrRaw { changes_asked: vec![crate::github::ChangesAsked { commit: None, ..p.changes_asked[0].clone() }], ..pushed.clone() };
        assert_eq!(kind(&unknown, true), None);
        // A draft, or a PR that ended, is nobody's move.
        assert_eq!(kind(&PrRaw { state: PrState::Draft, ..pushed.clone() }, true), None);
        assert_eq!(kind(&PrRaw { state: PrState::Merged, ..pushed.clone() }, true), None);
    }

    #[test]
    fn of_several_reviewers_the_one_whose_move_is_yours_says_it() {
        use TurnKind::*;
        let two = |ana_said: Vec<crate::github::Said>| PrRaw {
            head_oid: Some("bbb".into()),
            head_at: Some(110),
            said: [vec![wrote("ken", 100), asked("gwen", "ken", 112)], ana_said].concat(),
            changes_asked: vec![
                crate::github::ChangesAsked { who: "ken".into(), at: 100, commit: Some("aaa".into()) },
                crate::github::ChangesAsked { who: "ana".into(), at: 111, commit: Some("bbb".into()) },
            ],
            ..pr(PrState::Changes, 0)
        };
        // Ken is asked again; ana asked for changes of the commit it's at, and nothing since.
        assert_eq!(kind(&two(vec![]), true), None);
        // Ana wrote again after that: yours.
        assert_eq!(kind(&two(vec![wrote("ana", 130)]), true), Some((ReviewerReplied, "ana".into(), Some(130))));
        // You wrote since, which both of them hear: their re-reviews are what's left, both named.
        assert_eq!(kind(&two(vec![wrote("gwen", 125)]), true), Some((RereviewAsked, "ken".into(), Some(125))));
        assert_eq!(turn_of(&two(vec![wrote("gwen", 125)]), true, Some("gwen"), &HashSet::new()).map(|t| t.also), Some(vec!["ana".to_string()]));
        // Both owed a word on the fixes: each is named, so each can be asked.
        let both = PrRaw {
            head_oid: Some("ccc".into()),
            head_at: Some(140),
            said: vec![wrote("ken", 100), wrote("ana", 111)],
            changes_asked: vec![
                crate::github::ChangesAsked { who: "ken".into(), at: 100, commit: Some("aaa".into()) },
                crate::github::ChangesAsked { who: "ana".into(), at: 111, commit: Some("bbb".into()) },
            ],
            ..pr(PrState::Changes, 0)
        };
        let t = turn_of(&both, true, Some("gwen"), &HashSet::new()).unwrap();
        assert_eq!((t.kind, t.who.as_str(), t.also), (ReplyNeeded, "ken", vec!["ana".to_string()]));
    }

    #[test]
    fn nobody_said_so_only_where_what_was_said_reaches_back_to_the_push() {
        // Pushed at 110; GitHub's last twenty comments begin at 120, a bot's, so a word of yours
        // between the push and then can't be told from none.
        let p = PrRaw { head_oid: Some("bbb".into()), head_at: Some(110), said_from: Some(120), ..changes_asked() };
        assert_eq!(kind(&p, true), None);
        // Reaching back to it, it's said.
        assert_eq!(kind(&PrRaw { said_from: Some(105), ..p.clone() }, true).map(|t| t.0), Some(TurnKind::ReplyNeeded));
        // What's known still says the rest: asked again after it is their move.
        let asked_again = PrRaw { said: [p.said.clone(), vec![asked("gwen", "ken", 130)]].concat(), ..p.clone() };
        assert_eq!(kind(&asked_again, true).map(|t| t.0), Some(TurnKind::RereviewAsked));
    }

    #[test]
    fn someone_elses_pr_is_yours_once_its_author_writes_after_you() {
        let p = |said| PrRaw { said, author: Some("gwen".into()), ..pr(PrState::Open, 0) };
        assert_eq!(kind(&p(vec![wrote("me", 100), wrote("gwen", 110)]), false), Some((TurnKind::AuthorReplied, "gwen".into(), Some(110))));
        assert_eq!(kind(&p(vec![wrote("gwen", 110), wrote("me", 120)]), false), None);
        // You haven't written on it: nothing of yours to answer.
        assert_eq!(kind(&p(vec![wrote("gwen", 110)]), false), None);
        // Asking you again isn't words to answer: Re-review says it.
        assert_eq!(kind(&p(vec![wrote("me", 100), asked("gwen", "me", 110)]), false), None);
        // Approved as it is, their thanks are theirs to act on; after new commits, yours again.
        let approved = |commit: &str| Some(LastReview { state: LastReviewState::Approved, commit: Some(commit.into()), at: Some(100) });
        let thanked = |commit: &str| PrRaw { last_review: approved(commit), ..p(vec![wrote("me", 100), wrote("gwen", 110)]) };
        assert_eq!(kind(&thanked("0ef4ed2"), false), None);
        assert_eq!(kind(&thanked("a11ce55"), false).map(|t| t.0), Some(TurnKind::AuthorReplied));
        // Your agent's review, posted as you after their reply, leaves it yours to sign off; it does
        // say you were on it, before their reply.
        let post = |at| crate::github::Said { url: Some("agents-post".into()), ..wrote("me", at) };
        assert_eq!(kind(&p(vec![wrote("me", 100), wrote("gwen", 110), post(120)]), false).map(|t| t.0), Some(TurnKind::AuthorReplied));
        assert_eq!(kind(&p(vec![post(100), wrote("gwen", 110)]), false).map(|t| t.0), Some(TurnKind::AuthorReplied));
        // A Mac that didn't post it takes it for your answer.
        let elsewhere = turn_of(&p(vec![wrote("me", 100), wrote("gwen", 110), post(120)]), false, Some("me"), &HashSet::new());
        assert_eq!(elsewhere, None);
        // A review you submitted after it answers them, as soon as it's submitted here, before
        // GitHub's next answer has it among what was said.
        let submitted = |state| PrRaw { last_review: Some(LastReview { state, commit: Some("a11ce55".into()), at: Some(120) }), ..p(vec![wrote("me", 100), wrote("gwen", 110)]) };
        assert_eq!(kind(&submitted(LastReviewState::Commented), false), None);
        assert_eq!(kind(&submitted(LastReviewState::Changes), false), None);
    }

    #[test]
    fn someone_elses_pr_assigned_to_you_since_your_last_review_waits_on_it() {
        let p = |said, at| PrRaw { said, author: Some("gwen".into()), assigned: Some(crate::github::Assigned { by: "ken".into(), at }), ..pr(PrState::Open, 0) };
        // Assigned, with no review of yours since: your move, from whoever assigned you.
        assert_eq!(kind(&p(vec![], 100), false), Some((TurnKind::Assigned, "ken".into(), Some(100))));
        // A review of yours after it answers it, as soon as it's submitted here; one before doesn't.
        let reviewed = |at| PrRaw { last_review: Some(LastReview { state: LastReviewState::Commented, commit: None, at: Some(at) }), ..p(vec![], 100) };
        assert_eq!(kind(&reviewed(120), false), None);
        assert_eq!(kind(&reviewed(90), false).map(|t| t.0), Some(TurnKind::Assigned));
        // GitHub asking for your review says it itself, and a PR of yours is never yours to review.
        assert_eq!(kind(&PrRaw { review_requested: true, ..p(vec![], 100) }, false), None);
        assert_eq!(kind(&p(vec![], 100), true), None);
        // Of an assignment and the author's words after yours, the later says it.
        assert_eq!(kind(&p(vec![wrote("me", 90), wrote("gwen", 110)], 100), false).map(|t| t.0), Some(TurnKind::AuthorReplied));
        assert_eq!(kind(&p(vec![wrote("me", 90), wrote("gwen", 95)], 100), false).map(|t| t.0), Some(TurnKind::Assigned));

        // On the board it waits on your review, asked by whoever assigned you, since then; a repo
        // whose PRs aren't read has only the search's word on it.
        let seeds = [seed("/r/spark-portal", "portal")];
        let items = |prs: &HashMap<String, Vec<PrRaw>>, requested: &HashMap<String, Vec<PrRaw>>, seeds: &[LaneSeed]| pr_items(seeds, &[], prs, requested, &HashMap::new(), Some("me"), &PrLinks::default());
        let prs = HashMap::from([("sprucehq/spark".to_string(), vec![p(vec![], 100)])]);
        let got = items(&prs, &HashMap::new(), &seeds).remove(0);
        assert_eq!((got.review_requested, got.requested_by.as_deref(), got.requested_at), (true, Some("ken"), Some(100)));
        let gone = [seed("/r/spark-portal", "main")];
        assert!(!items(&prs, &HashMap::new(), &gone)[0].review_requested);
        assert!(items(&HashMap::new(), &prs, &gone)[0].review_requested);
        let reviewed = HashMap::from([("sprucehq/spark".to_string(), vec![reviewed(120)])]);
        assert_eq!(items(&reviewed, &HashMap::new(), &seeds)[0].requested_by, None);
        // Its author writing after the assignment says the move, and it still waits on you, from
        // the search too, where Author replied alone wouldn't bring it back (autoreview.rs
        // `asked_again`).
        let replied = HashMap::from([("sprucehq/spark".to_string(), vec![p(vec![wrote("me", 90), wrote("gwen", 110)], 100)])]);
        let got = items(&HashMap::new(), &replied, &gone).remove(0);
        assert_eq!((got.review_requested, got.requested_by.as_deref(), got.turn.map(|t| t.kind)), (true, Some("ken"), Some(TurnKind::AuthorReplied)));
        // A draft assigned to you doesn't wait on you, its assignee often being whoever does the
        // work: only a request for your review has a draft wait on you, and reviewed
        // (autoreview.rs `wants`, `[reviews] drafts`).
        let draft = HashMap::from([("sprucehq/spark".to_string(), vec![PrRaw { state: PrState::Draft, ..p(vec![], 100) }])]);
        assert!(!items(&draft, &HashMap::new(), &seeds)[0].review_requested);
        let asked = HashMap::from([("sprucehq/spark".to_string(), vec![PrRaw { state: PrState::Draft, review_requested: true, ..p(vec![], 100) }])]);
        assert!(items(&asked, &HashMap::new(), &seeds)[0].review_requested);
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

    #[test]
    fn lists_review_requests_in_repos_checked_out_only_on_their_default_branch() {
        let mut site = seed("/r/site", "main");
        site.repo = "site".into();
        site.repo_slug = Some("sprucehq/site".into());
        let seeds = [seed("/r/spark-portal", "portal"), site];
        assert_eq!(polled_slugs(&seeds), BTreeSet::from(["sprucehq/spark".to_string()]));
        let asked = |number: u64| PrRaw { number, review_requested: true, ..pr(PrState::Open, 0) };
        // The site's PRs were read before its last worktree left its branch: #5 has since been
        // reviewed, #7 still waits and is newer from the search. The repo read on its own keeps
        // its own word: the search's copy of #1037 doesn't count there. The search's #2000, past
        // the hundred PRs the repo's read holds, is listed from the search.
        let prs = HashMap::from([
            ("sprucehq/spark".to_string(), vec![pr(PrState::Open, 0)]),
            ("sprucehq/site".to_string(), vec![asked(5), PrRaw { title: "Old title".into(), ..asked(7) }]),
        ]);
        let requested = HashMap::from([
            ("sprucehq/site".to_string(), vec![asked(7), asked(9)]),
            ("sprucehq/spark".to_string(), vec![asked(1037), asked(2000)]),
            ("acme/elsewhere".to_string(), vec![asked(3)]),
        ]);
        let mut got: Vec<(String, bool, String)> = pr_items(&seeds, &[], &prs, &requested, &HashMap::new(), None, &PrLinks::default()).into_iter().map(|p| (p.key, p.review_requested, p.project)).collect();
        got.sort();
        let want = [("sprucehq/site#5", false), ("sprucehq/site#7", true), ("sprucehq/site#9", true), ("sprucehq/spark#1037", false), ("sprucehq/spark#2000", true)];
        assert_eq!(got, want.map(|(k, r)| (k.to_string(), r, "Spruce".to_string())));
        let items = pr_items(&seeds, &[], &prs, &requested, &HashMap::new(), None, &PrLinks::default());
        assert_eq!(items.iter().find(|p| p.number == 7).map(|p| p.title.as_str()), Some("Let the agent add and caption project photos"));
    }

    #[test]
    fn a_claude_session_lists_what_its_running_turn_holds_of_yours() {
        let held = |id: &str, text: &str| QueuedMessage { id: id.into(), at: NOW - 5_000, text: text.into() };
        let queue = vec![held("q1", "Also check the docs"), held("q2", "<task-notification>\n</task-notification>")];
        let t = TranscriptInfo { queue, ..tr() };
        let s = claude_session(&acct("claude"), Some(&reg("busy", NOW)), Some(&t), 0, NOW, &Quiet::default()).unwrap();
        assert_eq!((s.queued.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), s.queued_at), (vec!["q1"], Some(NOW - 5_000)));
        // Held while it waits on you too; once the turn is over, or the process gone, nothing is.
        assert_eq!(claude_session(&acct("claude"), Some(&reg("waiting", NOW)), Some(&t), 0, NOW, &Quiet::default()).unwrap().queued.len(), 1);
        for s in [claude_session(&acct("claude"), Some(&reg("idle", NOW)), Some(&t), 0, NOW, &Quiet::default()), claude_session(&acct("claude"), None, Some(&t), 0, NOW, &Quiet::default())] {
            let s = s.unwrap();
            assert_eq!((s.queued.len(), s.queued_at), (0, None));
        }
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
            session_settings: &Default::default(), auto_reviews: (&Default::default(), &Default::default(), &Default::default()), machines: &Default::default(),
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
    fn links_prs_to_the_worktrees_and_sessions_on_their_branch() {
        let apps = AppNames::default();
        let mut s = seed("/r/spark-portal", "portal");
        s.repo_slug = Some("sprucehq/spark".into());
        let seeds = vec![s];
        let a = acct("claude2");
        let mut on_branch = claude_session(&a, Some(&reg("idle", NOW)), Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
        on_branch.branch = Some("portal".into());
        let mut elsewhere = on_branch.clone();
        elsewhere.key = "claude:claude2:other".into();
        elsewhere.branch = Some("app4".into());
        let prs = HashMap::from([("sprucehq/spark".to_string(), vec![pr(PrState::Open, 0)])]);
        let (git, visitors) = (HashMap::new(), HashMap::new());
        let order = vec!["Spruce".to_string()];
        let board = compose(Inputs {
            now: NOW,
            apps: &apps,
            seeds: &seeds,
            sessions: vec![on_branch.clone(), elsewhere],
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
            github_login: Some("gwen"),
            pr_links: &PrLinks::default(),
            previews: &Previews::default(),
            reusable: &HashMap::new(),
            review_snoozes: &[],
            session_settings: &Default::default(), auto_reviews: (&Default::default(), &Default::default(), &Default::default()), machines: &Default::default(),
        });
        assert_eq!(board.prs.len(), 1);
        let p = &board.prs[0];
        assert_eq!(p.key, "sprucehq/spark#1037");
        assert!(p.mine);
        assert_eq!(p.lanes, vec!["/r/spark-portal".to_string()]);
        assert_eq!(p.sessions, vec![PrSession { key: on_branch.key, lane: "/r/spark-portal".into(), pushes: 0, primary: None }]);
        assert_eq!(board.github_login.as_deref(), Some("gwen"));
    }

    #[test]
    fn a_pr_from_a_fork_the_checkout_has_no_remote_for_is_from_another_branch() {
        let s = |key: &str, started: i64| Session { cwd: "/r/spark-portal".into(), branch: Some("portal".into()), started_at: Some(started), last_activity: NOW, ..crate::fixtures::session(key) };
        let opened = NOW - 86_400_000;
        let lane = Lane { repo_slug: Some("sprucehq/spark".into()), branch: Some("portal".into()), ..crate::fixtures::lane("/r/spark-portal", vec![s("ours", opened), s("reviewer", opened + 1_000)]) };
        let lanes = vec![lane];
        // The checkout's remotes are sprucehq's and gwen's.
        let seeds = vec![seed("/r/spark-portal", "portal")];
        let push = vec![("portal".to_string(), opened + 3_600_000)];
        let links = PrLinks {
            worked: HashMap::from([("sprucehq/spark#1037".to_string(), vec!["reviewer".to_string()])]),
            pushed: HashMap::from([("ours".to_string(), push.clone()), ("reviewer".to_string(), push)]),
            ..Default::default()
        };
        let listed = |owner: &str| {
            let p = PrRaw { head_owner: Some(owner.into()), author: Some(owner.into()), ..pr(PrState::Open, 0) };
            let item = pr_card(&seeds, &lanes, "sprucehq/spark", &HashMap::new(), &p, Some("gwen"), &links);
            item.sessions.into_iter().map(|x| (x.key, x.pushes)).collect::<Vec<_>>()
        };
        assert_eq!(listed("gwen"), vec![("ours".to_string(), 1), ("reviewer".to_string(), 1)]);
        // GitHub may name the owner in another case than the remote's URL.
        assert_eq!(listed("Gwen"), listed("gwen"));
        // Ana's `portal` isn't the one checked out here: only the session whose records show it
        // worked on her PR is listed, and its pushes to this `portal` weren't to hers.
        assert_eq!(listed("ana"), vec![("reviewer".to_string(), 0)]);
    }

    #[test]
    fn a_pr_the_board_doesnt_list_is_matched_in_the_project_repo_a_link_names() {
        let seeds = vec![seed("/r/spark-portal", "portal")];
        let a = acct("claude2");
        let mut maker = claude_session(&a, Some(&reg("idle", NOW)), Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
        maker.branch = Some("app4".into());
        let (git, visitors, prs) = (HashMap::new(), HashMap::new(), HashMap::new());
        let order = vec!["Spruce".to_string()];
        let board = compose(Inputs { now: NOW, apps: &AppNames::default(), seeds: &seeds, sessions: vec![maker.clone()], visitors: &visitors, git: &git, prs: &prs, requested: &HashMap::new(), made: &HashMap::new(), project_order: &order, notices: vec![], loading: false, github_at: None, pinned: &[], github_login: None, pr_links: &PrLinks::default(), previews: &Previews::default(), reusable: &HashMap::new(), review_snoozes: &[], session_settings: &Default::default(), auto_reviews: (&Default::default(), &Default::default(), &Default::default()), machines: &Default::default() });
        let old = pr(PrState::Merged, 0);
        let links = PrLinks { worked: HashMap::from([("sprucehq/spark#1037".to_string(), vec![maker.key.clone()]), ("acme/tool#1037".to_string(), vec![maker.key.clone()])]), ..Default::default() };
        let renamed = HashMap::from([("sprucehq/spark".to_string(), "Spruce/spark".to_string())]);
        // In another case, or by the repo's name now: the project's repo, and the session its
        // records say made it.
        for slug in ["SpruceHQ/Spark", "spruce/spark"] {
            let item = pr_card(&seeds, &board.lanes, slug, &renamed, &old, Some("gwen"), &links);
            assert_eq!((item.key.as_str(), item.project.as_str(), item.repo.as_str()), ("sprucehq/spark#1037", "Spruce", "spark"), "{slug}");
            assert_eq!(item.sessions.iter().map(|x| x.key.as_str()).collect::<Vec<_>>(), vec![maker.key.as_str()], "{slug}");
            assert!(item.mine);
        }
        // A repo no project covers still gets the sessions linked to it.
        let other = pr_card(&seeds, &board.lanes, "acme/tool", &renamed, &old, None, &links);
        assert_eq!((other.key.as_str(), other.project.as_str(), other.repo.as_str()), ("acme/tool#1037", "", "tool"));
        assert_eq!(other.sessions.len(), 1);
        assert!(other.lanes.is_empty());
    }

    #[test]
    fn a_pr_a_session_made_where_no_repo_is_read_is_listed_with_it() {
        let maker = Session { cwd: "/r/spark-portal".into(), branch: Some("portal".into()), ..crate::fixtures::session("maker") };
        let lanes = vec![Lane { project: "Spruce".into(), repo_slug: Some("sprucehq/spark".into()), branch: Some("portal".into()), ..crate::fixtures::lane("/r/spark-portal", vec![maker]) }];
        // Spark's PRs are read; its docs repo is checked out on its default branch alone.
        let docs = LaneSeed { repo: "docs".into(), repo_slug: Some("SpruceHQ/docs".into()), ..seed("/r/docs", "main") };
        let seeds = vec![seed("/r/spark-portal", "portal"), docs];
        let upstream = PrRaw { number: 8580, url: "https://github.com/Upstream/orm/pull/8580".into(), head_ref: "skip-scan".into(), updated_at: NOW - 1_000, ..pr(PrState::Open, 0) };
        let docs_pr = PrRaw { number: 12, url: "https://github.com/sprucehq/docs/pull/12".into(), head_ref: "photos".into(), ..pr(PrState::Merged, 0) };
        let made = HashMap::from([("Upstream/orm".to_string(), vec![upstream.clone()]), ("sprucehq/docs".to_string(), vec![docs_pr.clone()])]);
        let mut links = PrLinks { worked: HashMap::from([("upstream/orm#8580".to_string(), vec!["maker".to_string()])]), ..Default::default() };
        let listed = |requested: &HashMap<String, Vec<PrRaw>>, lanes: &[Lane], links: &PrLinks| {
            let items = pr_items(&seeds, lanes, &HashMap::new(), requested, &made, Some("gwen"), links);
            items.into_iter().map(|p| (p.key, p.project, p.repo, p.sessions.into_iter().map(|s| s.key).collect::<Vec<_>>())).collect::<Vec<_>>()
        };
        let none = HashMap::new();
        let upstream_item = ("Upstream/orm#8580".to_string(), "Spruce".to_string(), "orm".to_string(), vec!["maker".to_string()]);
        let docs_item = ("SpruceHQ/docs#12".to_string(), "Spruce".to_string(), "docs".to_string(), vec![]);
        // Upstream, under the project of the session that made it, with it; the docs PR in its
        // project's repo, whatever case its link has.
        assert_eq!(listed(&none, &lanes, &links), vec![upstream_item.clone(), docs_item.clone()]);

        // Waiting on your review there too, it's listed once.
        let asked = HashMap::from([("SpruceHQ/docs".to_string(), vec![PrRaw { review_requested: true, ..docs_pr.clone() }])]);
        assert_eq!(listed(&asked, &lanes, &links).len(), 2);

        // A repo no longer read keeps its last read, from before the docs PR merged: the one read
        // by its number is the newer.
        let stale = HashMap::from([("SpruceHQ/docs".to_string(), vec![PrRaw { state: PrState::Open, merged_at: None, ..docs_pr }])]);
        let docs: Vec<PrState> = pr_items(&seeds, &lanes, &stale, &none, &made, Some("gwen"), &links).into_iter().filter(|p| p.repo == "docs").map(|p| p.state).collect();
        assert_eq!(docs, [PrState::Merged]);

        // With no session it's for on the board, or the one there unlinked from it, nothing
        // gives the upstream PR a project.
        assert_eq!(listed(&none, &[], &links), vec![docs_item.clone()]);
        links.unlinked.insert(("upstream/orm#8580".into(), "maker".into()));
        assert_eq!(listed(&none, &lanes, &links), vec![docs_item]);
    }

    #[test]
    fn the_prs_worked_on_come_latest_marked_first() {
        let worked = |keys: &[(&str, &str)]| keys.iter().map(|(pr, s)| (pr.to_string(), vec![s.to_string()])).collect();
        let on_board = ["a", "b", "c", "linked"].map(crate::fixtures::session);
        let lanes = vec![crate::fixtures::lane("/w", on_board.to_vec())];
        let mut links = PrLinks {
            // A session the board no longer lists made #5.
            worked: worked(&[("o/r#1", "a"), ("o/r#2", "b"), ("o/r#3", "c"), ("o/r#4", "linked"), ("o/r#5", "gone")]),
            made: HashMap::from([
                ("o/r#1".to_string(), vec![("a".to_string(), 100)]),
                ("o/r#2".to_string(), vec![("b".to_string(), 300), ("a".to_string(), 50)]),
                ("o/r#3".to_string(), vec![("c".to_string(), 200)]),
                ("o/r#5".to_string(), vec![("gone".to_string(), 900)]),
                ("o/r#6".to_string(), vec![("a".to_string(), 10)]),
            ]),
            ..Default::default()
        };
        // #6, marked long ago by a, was linked to a by hand today.
        links.worked.insert("o/r#6".into(), vec!["a".into()]);
        links.by_hand.extend(["o/r#4".to_string(), "o/r#6".to_string()]);
        // Links by hand first, then by the latest session's mark.
        assert_eq!(worked_prs(&links, &lanes), ["o/r#4", "o/r#6", "o/r#2", "o/r#3", "o/r#1"]);
        // Every session it had unlinked by hand: it's no one's.
        links.unlinked.insert(("o/r#3".into(), "c".into()));
        assert_eq!(worked_prs(&links, &lanes), ["o/r#4", "o/r#6", "o/r#2", "o/r#1"]);
    }

    #[test]
    fn a_prs_primary_is_the_one_picked_else_the_one_that_opened_it_else_the_first_started() {
        let s = |key: &str, started: i64| Session { started_at: Some(started), ..crate::fixtures::session(key) };
        // An automatic review of the PR, in its worktree on its branch, started first of all.
        let review = Session { helper: Some(Helper { pr: Some("o/r#1".into()), ..Default::default() }), ..s("review", 5) };
        let lanes = vec![crate::fixtures::lane("/w", vec![s("a", 30), s("b", 10), s("c", 20), s("copy", 40), review])];
        let listed = |keys: &[&str]| keys.iter().map(|k| PrSession { key: (*k).into(), lane: "/w".into(), pushes: 0, primary: None }).collect::<Vec<_>>();
        let primary = |keys: &[&str], links: &PrLinks| pr_primary(&lanes, &listed(keys), "o/r#1", links).map(|(k, _)| k);
        let mut links = PrLinks::default();
        // Nothing in the records: the one that started first.
        assert_eq!(primary(&["a", "b", "c"], &links).as_deref(), Some("b"));
        assert_eq!(primary(&[], &links), None);
        // The first mark is the one that opened it; a mark without a time comes last, and a copy
        // of its conversation, with the same marks, loses to the one that started first.
        links.made.insert("o/r#1".into(), vec![("a".into(), 0), ("copy".into(), 200), ("c".into(), 200), ("b".into(), 300)]);
        assert_eq!(primary(&["a", "b", "c", "copy"], &links).as_deref(), Some("c"));
        // One that isn't listed (archived, unlinked) leaves it to the next.
        assert_eq!(primary(&["a", "b"], &links).as_deref(), Some("b"));
        // Picked by hand wins, while it's listed.
        links.picked.insert("o/r#1".into(), "a".into());
        assert_eq!(primary(&["a", "b", "c"], &links).as_deref(), Some("a"));
        assert_eq!(primary(&["b", "c"], &links).as_deref(), Some("c"));
        // The review is never it, though it started first, unless it's picked: with nothing else
        // listed, the PR has none.
        let links = PrLinks::default();
        assert_eq!(primary(&["review", "c", "b"], &links).as_deref(), Some("b"));
        assert_eq!(primary(&["review"], &links), None);
        let picked = PrLinks { picked: HashMap::from([("o/r#1".to_string(), "review".to_string())]), ..Default::default() };
        assert_eq!(primary(&["review", "b"], &picked).as_deref(), Some("review"));
    }

    /// From a real PR: one session opened a small PR from its worktree, with a quiet
    /// push that left no record, and another, in a worktree on a branch of its own, took the work
    /// over, pushing it to the PR's branch again and again (`git push origin tiers:portal`).
    #[test]
    fn a_session_that_pushes_to_a_pr_more_than_its_opener_becomes_its_primary() {
        let s = |key: &str, cwd: &str, branch: &str, started: i64| Session { cwd: cwd.into(), branch: Some(branch.into()), started_at: Some(started), last_activity: NOW, ..crate::fixtures::session(key) };
        let lane = |id: &str, slug: &str, branch: &str, sessions: Vec<Session>| Lane { repo_slug: Some(slug.into()), branch: Some(branch.into()), ..crate::fixtures::lane(id, sessions) };
        let opened = NOW - 86_400_000;
        let lanes = vec![
            lane("/r/spark-portal", "sprucehq/spark", "portal", vec![s("opener", "/r/spark-portal", "portal", opened - 3_600_000)]),
            // Another clone on the PR's branch too.
            lane("/r/spark-portal2", "sprucehq/spark", "portal", vec![s("fixer", "/r/spark-portal2", "portal", opened + 1_000)]),
            lane("/r/spark-tiers", "sprucehq/spark", "tiers", vec![s("router", "/r/spark-tiers", "tiers", opened + 2_000)]),
            // A branch of the same name in another repo.
            lane("/r/tool", "acme/tool", "portal", vec![s("elsewhere", "/r/tool", "portal", opened)]),
        ];
        let seeds = vec![seed("/r/spark-portal", "portal")];
        let pushed = |n: usize| (0..n).map(|i| ("portal".to_string(), opened + 3_600_000 * (i as i64 + 1))).collect::<Vec<_>>();
        let card = |links: &PrLinks| {
            let p = pr_card(&seeds, &lanes, "sprucehq/spark", &HashMap::new(), &pr(PrState::Open, 0), Some("gwen"), links);
            let listed: Vec<(String, u32)> = p.sessions.iter().map(|x| (x.key.clone(), x.pushes)).collect();
            (p.primary.unwrap_or_default(), p.primary_by, listed)
        };
        let key = "sprucehq/spark#1037".to_string();
        // The opener's records show it made the PR; the router's that it edited it later.
        let mut links = PrLinks {
            worked: HashMap::from([(key.clone(), vec!["opener".to_string(), "router".to_string()])]),
            made: HashMap::from([(key.clone(), vec![("opener".to_string(), opened), ("router".to_string(), opened + 3_600_000)])]),
            pushed: HashMap::from([("router".to_string(), pushed(1)), ("elsewhere".to_string(), pushed(5))]),
            ..Default::default()
        };
        // One push each, the opener's left no record: it stays the opener's.
        let (primary, by, listed) = card(&links);
        assert_eq!((primary.as_str(), by), ("opener", Some(PrimaryBy::Opened)));
        assert_eq!(listed, vec![("opener".to_string(), 0), ("fixer".to_string(), 0), ("router".to_string(), 1)], "not the other repo's");
        // A second push takes it over.
        links.pushed.insert("router".into(), pushed(3));
        assert_eq!(card(&links).0, "router");
        assert_eq!(card(&links).1, Some(PrimaryBy::Pushed));
        // As it does without a mark of its own, listed by its pushes alone.
        links.worked.get_mut(&key).unwrap().retain(|k| k != "router");
        links.made.get_mut(&key).unwrap().retain(|(k, _)| k != "router");
        assert_eq!(card(&links).0, "router");
        // An opener that pushed as often keeps it, and one that pushed a fix or two to another's
        // PR doesn't take it.
        links.pushed.insert("opener".into(), pushed(3));
        links.pushed.insert("fixer".into(), pushed(2));
        assert_eq!((card(&links).0.as_str(), card(&links).1), ("opener", Some(PrimaryBy::Opened)));
        // Nor do pushes to the branch for an earlier PR from it.
        links.pushed.get_mut("fixer").unwrap().extend((0..4).map(|i| ("portal".to_string(), opened - 86_400_000 + i)));
        assert_eq!(card(&links).0, "opener");
        // One in another repo, listed by its records, gets nothing for pushing to a branch of the
        // same name there.
        links.worked.get_mut(&key).unwrap().push("elsewhere".into());
        links.made.get_mut(&key).unwrap().push(("elsewhere".into(), opened + 7_200_000));
        links.pushed.insert("elsewhere".into(), pushed(9));
        let (primary, _, listed) = card(&links);
        assert_eq!(primary, "opener");
        assert!(listed.contains(&("elsewhere".to_string(), 0)), "{listed:?}");
        // Pushes after it merged don't count.
        let mut merged = pr(PrState::Merged, 0);
        merged.merged_at = Some(opened + 3_600_000 * 2 + 1);
        links.pushed.insert("opener".into(), vec![]);
        let p = pr_card(&seeds, &lanes, "sprucehq/spark", &HashMap::new(), &merged, Some("gwen"), &links);
        assert_eq!(p.sessions.iter().find(|x| x.key == "router").map(|x| x.pushes), Some(2));
        // Picked by hand wins.
        links.picked.insert(key.clone(), "fixer".into());
        assert_eq!((card(&links).0.as_str(), card(&links).1), ("fixer", Some(PrimaryBy::Picked)));
        // Without marks or pushes: the one that started first.
        let none = PrLinks::default();
        assert_eq!((card(&none).0.as_str(), card(&none).1), ("opener", Some(PrimaryBy::Started)));
        // A branch an earlier PR freed half an hour before this one opened: the pushes before
        // that were the earlier one's, though they're in the hour before this one.
        let mut earlier = pr(PrState::Merged, 0);
        earlier.number = 1036;
        earlier.created_at = opened - 7_200_000;
        earlier.merged_at = Some(opened - 1_800_000);
        let mut router = (0..3).map(|i| ("portal".to_string(), opened - 2_400_000 + i)).collect::<Vec<_>>();
        router.push(("portal".to_string(), opened - 1_200_000));
        let reused = PrLinks { pushed: HashMap::from([("router".to_string(), router)]), ..Default::default() };
        let router_pushes = |seeds: &[LaneSeed], prs: Vec<PrRaw>, requested: Vec<PrRaw>| {
            let (prs, requested) = (HashMap::from([("sprucehq/spark".to_string(), prs)]), HashMap::from([("sprucehq/spark".to_string(), requested)]));
            let items = pr_items(seeds, &lanes, &prs, &requested, &HashMap::new(), Some("gwen"), &reused);
            items.iter().find(|p| p.number == 1037).and_then(|p| p.sessions.iter().find(|x| x.key == "router")).map(|x| x.pushes)
        };
        assert_eq!(router_pushes(&seeds, vec![earlier.clone(), pr(PrState::Open, 0)], vec![]), Some(1));
        // A branch of the same name in another fork is another branch.
        let forked = PrRaw { head_owner: Some("ken".into()), ..earlier.clone() };
        assert_eq!(router_pushes(&seeds, vec![forked, pr(PrState::Open, 0)], vec![]), Some(4));
        // A PR only the search for reviews lists, in a repo whose PRs aren't read now (no worktree
        // off its default branch), has the earlier one from the repo's last read.
        let unpolled = vec![seed("/r/spark-portal", "main")];
        assert_eq!(router_pushes(&unpolled, vec![earlier], vec![pr(PrState::Open, 0)]), Some(1));
    }

    #[test]
    fn a_pr_of_yours_nudges_by_default_unless_turned_off_for_it() {
        let seeds = vec![seed("/r/spark-portal", "portal")];
        let card = |p: &PrRaw, login: Option<&str>, links: &PrLinks| {
            let item = pr_card(&seeds, &[], "sprucehq/spark", &HashMap::new(), p, login, links);
            (item.nudge, item.nudge_by_default)
        };
        let open = pr(PrState::Open, 0);
        let mut links = PrLinks::default();
        assert_eq!(card(&open, Some("gwen"), &links), (false, false));
        links.nudge_by_default = true;
        assert_eq!(card(&open, Some("gwen"), &links), (true, true));
        // Someone else's isn't, nor one that ended.
        assert_eq!(card(&open, Some("ken"), &links), (false, false));
        assert_eq!(card(&pr(PrState::Merged, 0), Some("gwen"), &links), (false, false));
        // Turned on for it: on whatever the setting says; off for it: off.
        links.nudged.insert("sprucehq/spark#1037".into());
        assert_eq!(card(&open, Some("ken"), &links), (true, false));
        links.nudged.clear();
        links.not_nudged.insert("sprucehq/spark#1037".into());
        assert_eq!(card(&open, Some("gwen"), &links), (false, false));
    }

    /// A watch is the session's picked as the PR's primary: archived, the next takes over the PR
    /// without it, and it's back with the session. It stays said as the PR ends, unlike the nudge,
    /// so that the ending is told in full.
    #[test]
    fn a_watch_goes_with_the_session_picked_for_it() {
        let s = |key: &str, archived: bool| Session { cwd: "/r/spark-portal".into(), branch: Some("portal".into()), started_at: Some(NOW - 1_000), last_activity: NOW, archived, ..crate::fixtures::session(key) };
        let lanes = |archived: bool| vec![Lane { repo_slug: Some("sprucehq/spark".into()), branch: Some("portal".into()), ..crate::fixtures::lane("/r/spark-portal", vec![s("watcher", archived), s("other", false)]) }];
        let seeds = vec![seed("/r/spark-portal", "portal")];
        let key = "sprucehq/spark#1037".to_string();
        let card = |p: PrRaw, archived: bool, links: &PrLinks| {
            let item = pr_card(&seeds, &lanes(archived), "sprucehq/spark", &HashMap::new(), &p, Some("gwen"), links);
            (item.watched, item.primary.unwrap_or_default())
        };
        let mut links = PrLinks { watched: HashSet::from([key.clone()]), picked: HashMap::from([(key.clone(), "watcher".to_string())]), worked: HashMap::from([(key.clone(), vec!["watcher".to_string()])]), ..Default::default() };
        assert_eq!(card(pr(PrState::Open, 0), false, &links), (true, "watcher".to_string()));
        assert_eq!(card(pr(PrState::Merged, 0), false, &links), (true, "watcher".to_string()));
        assert_eq!(card(pr(PrState::Open, 0), true, &links), (false, "other".to_string()));
        // Nobody picked, as with a file written before a watch picked one: nobody is watching.
        links.picked.clear();
        assert!(!card(pr(PrState::Open, 0), false, &links).0);
        // Picked, and no watch.
        links.picked.insert(key, "watcher".into());
        links.watched.clear();
        assert!(!card(pr(PrState::Open, 0), false, &links).0);
    }

    /// A session archived once its PR is done with (Mark as done, say) is still listed on it, by
    /// its branch or by its records, as one that made the PR from another repo's checkout is; but
    /// it's never the PR's primary, even picked: the next takes over, or none does.
    #[test]
    fn an_archived_session_stays_listed_on_its_pr_but_isnt_its_primary() {
        let s = |key: &str, cwd: &str, branch: &str, archived: bool| Session { cwd: cwd.into(), branch: Some(branch.into()), started_at: Some(NOW - 3 * 3_600_000), last_activity: NOW, archived, ..crate::fixtures::session(key) };
        // The maker opened the PR from the app's checkout, as its records say; the fixer worked
        // on the PR's branch.
        let lanes = |maker: bool, fixer: bool| {
            vec![
                Lane { repo_slug: Some("sprucehq/spark".into()), branch: Some("portal".into()), ..crate::fixtures::lane("/r/spark-portal", vec![s("fixer", "/r/spark-portal", "portal", fixer)]) },
                Lane { repo_slug: Some("sprucehq/app".into()), branch: Some("main".into()), ..crate::fixtures::lane("/r/app", vec![s("maker", "/r/app", "main", maker)]) },
            ]
        };
        let seeds = vec![seed("/r/spark-portal", "portal")];
        let key = "sprucehq/spark#1037".to_string();
        let mut links = PrLinks { worked: HashMap::from([(key.clone(), vec!["maker".to_string()])]), made: HashMap::from([(key.clone(), vec![("maker".to_string(), NOW - 2 * 86_400_000)])]), ..Default::default() };
        let card = |lanes: &[Lane], links: &PrLinks| {
            let item = pr_card(&seeds, lanes, "sprucehq/spark", &HashMap::new(), &pr(PrState::Merged, 0), Some("gwen"), links);
            (item.sessions.into_iter().map(|x| (x.key, x.lane)).collect::<Vec<_>>(), item.primary)
        };
        let both = vec![("fixer".to_string(), "/r/spark-portal".to_string()), ("maker".to_string(), "/r/app".to_string())];
        assert_eq!(card(&lanes(false, false), &links), (both.clone(), Some("maker".to_string())));
        assert_eq!(card(&lanes(true, false), &links), (both.clone(), Some("fixer".to_string())));
        assert_eq!(card(&lanes(true, true), &links), (both.clone(), None));
        links.picked.insert(key.clone(), "maker".into());
        assert_eq!(card(&lanes(true, false), &links), (both.clone(), Some("fixer".to_string())));
        // Unlinked by hand, it goes as an unarchived one does.
        links.unlinked.insert((key, "maker".into()));
        assert_eq!(card(&lanes(true, false), &links).0, both[..1]);
    }

    #[test]
    fn main_checkouts_and_reused_worktrees_are_reusable_unless_marked_otherwise() {
        let main = LaneSeed { origin: Origin::Main, ..seed("/r/spark", "app") };
        let wt = seed("/r/spark-app3", "app3");
        let none = HashMap::new();
        assert!(reusable(&main, &none, None));
        assert!(!reusable(&wt, &none, None));
        let marks = HashMap::from([("/r/spark".to_string(), false), ("/r/spark-app3".to_string(), true)]);
        assert!(!reusable(&main, &marks, None));
        assert!(reusable(&wt, &marks, None));
        // One that took new work three times or more has been reused, unless marked otherwise.
        let git = |n: u32| GitInfo { new_work_count: Some(n), ..Default::default() };
        assert!(reusable(&wt, &none, Some(&git(3))));
        assert!(!reusable(&wt, &none, Some(&git(2))));
        assert!(!reusable(&wt, &HashMap::from([("/r/spark-app3".to_string(), false)]), Some(&git(9))));
        // A plain folder, an app's chats and a repo outside your projects never are.
        let all = HashMap::from([("/r/notes".to_string(), true)]);
        assert!(!reusable(&LaneSeed { origin: Origin::Folder, ..seed("/r/notes", "x") }, &all, None));
        assert!(!reusable(&LaneSeed { chats: true, ..seed("/r/notes", "x") }, &all, None));
        assert!(!reusable(&LaneSeed { elsewhere: true, ..seed("/r/notes", "x") }, &all, None));
    }

    #[test]
    fn a_branch_that_carried_several_merged_prs_looks_reusable() {
        let wt = seed("/r/spark-app3", "app3");
        let merged = |n: u64, owner: &str| PrRaw { number: n, head_ref: "app3".into(), head_owner: Some(owner.into()), ..pr(PrState::Merged, 0) };
        let prs = |list: Vec<PrRaw>| HashMap::from([("sprucehq/spark".to_string(), list)]);
        let two = prs(vec![merged(1, "gwen"), merged(2, "gwen")]);
        assert!(looks_reusable(&wt, false, None, &two));
        // A teammate's `app3` from their fork says the same of the name.
        assert!(looks_reusable(&wt, false, None, &prs(vec![merged(1, "gwen"), merged(2, "someone")])));
        // One merged PR is a branch's usual life, and an open one hasn't landed.
        assert!(!looks_reusable(&wt, false, None, &prs(vec![merged(1, "gwen"), PrRaw { state: PrState::Open, ..merged(2, "gwen") }])));
        // One that has been reused is reusable already, so it isn't suggested.
        let git = |n: u32| GitInfo { new_work_count: Some(n), ..Default::default() };
        assert!(!looks_reusable(&wt, false, Some(&git(3)), &two));
        assert!(looks_reusable(&wt, false, Some(&git(2)), &two));
        // Marked either way, or reusable already, it isn't suggested; nor is the default branch.
        assert!(!looks_reusable(&wt, true, Some(&git(9)), &two));
        assert!(!looks_reusable(&LaneSeed { origin: Origin::Main, ..wt.clone() }, false, Some(&git(9)), &two));
        let on_main = |n| PrRaw { head_ref: "main".into(), ..merged(n, "gwen") };
        assert!(!looks_reusable(&LaneSeed { branch: Some("main".into()), ..wt.clone() }, false, None, &prs(vec![on_main(1), on_main(2)])));
    }

    #[test]
    fn what_a_reusable_worktree_holds() {
        let git = |dirty: u32, ahead: u32| GitInfo { dirty, base_ahead: Some(ahead), ..Default::default() };
        let base = claude_session(&acct("claude"), Some(&reg("idle", NOW)), Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
        let with = |status: Status| Session { status, live: true, ..base.clone() };
        let pr_in = |state: PrState| Pr { number: 1, title: String::new(), url: String::new(), state, checks: Checks::default(), merge: Merge::Unknown, updated_at: 0, merged_at: None, closed_at: None, stale_approval: None };
        let clean = git(0, 0);
        assert_eq!(reuse_of(None, Some(&clean), &[]), Reuse::Free);
        // An agent sitting idle there doesn't hold it; one at work does.
        assert_eq!(reuse_of(None, Some(&clean), &[with(Status::Idle), with(Status::Unread)]), Reuse::Free);
        assert_eq!(reuse_of(None, Some(&clean), &[with(Status::Working)]), Reuse::Open);
        assert_eq!(reuse_of(None, Some(&clean), &[with(Status::Needs)]), Reuse::Open);
        // Visitors, and earlier work's sessions, belong elsewhere.
        assert_eq!(reuse_of(None, Some(&clean), &[Session { visitor: Some("worked here".into()), ..with(Status::Working) }]), Reuse::Free);
        // Uncommitted changes, or commits that haven't landed, are work under way.
        assert_eq!(reuse_of(None, Some(&git(2, 0)), &[]), Reuse::Open);
        assert_eq!(reuse_of(None, Some(&git(0, 3)), &[]), Reuse::Open);
        assert_eq!(reuse_of(Some(&pr_in(PrState::Open)), Some(&git(0, 3)), &[]), Reuse::Review);
        assert_eq!(reuse_of(Some(&pr_in(PrState::Changes)), Some(&git(1, 3)), &[]), Reuse::Review);
        // A squash merge leaves the commits off the default branch; the merged PR says they landed.
        assert_eq!(reuse_of(Some(&pr_in(PrState::Merged)), Some(&git(0, 3)), &[]), Reuse::Landed);
        assert_eq!(reuse_of(Some(&pr_in(PrState::Merged)), Some(&git(1, 3)), &[]), Reuse::Open);
        // Closed without merging, its commits haven't landed.
        assert_eq!(reuse_of(Some(&pr_in(PrState::Closed)), Some(&git(0, 3)), &[]), Reuse::Open);
    }

    #[test]
    fn a_reused_worktree_sets_aside_the_sessions_from_before_its_latest_work() {
        let path = "/r/spark-app3";
        let seeds = vec![seed(path, "app3")];
        let base = claude_session(&acct("claude"), Some(&reg("idle", NOW)), Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
        let at = |key: &str, status: Status, ago: i64| Session { key: key.into(), id: key.into(), cwd: path.into(), status, live: false, last_activity: NOW - ago, last_prompt_at: NOW - ago, ..base.clone() };
        // Shipped the last work and was never read; the worktree was reset for new work since.
        let shipped = at("c:shipped", Status::Unread, 5 * 3_600_000);
        let asked = at("c:asked", Status::Needs, 6 * 3_600_000);
        let fresh = at("c:fresh", Status::Idle, 60_000);
        let git = HashMap::from([(PathBuf::from(path), GitInfo { reset_at: Some(NOW - 2 * 3_600_000), base_ahead: Some(0), ..Default::default() })]);
        let (visitors, prs, order) = (HashMap::new(), HashMap::new(), vec!["Spruce".to_string()]);
        let board_with = |sessions: Vec<Session>, marks: &HashMap<String, bool>| {
            compose(Inputs {
                now: NOW,
                apps: &AppNames::default(),
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
                pinned: &[],
                github_login: None,
                pr_links: &PrLinks::default(),
                previews: &Previews::default(),
                reusable: marks,
                review_snoozes: &[],
                session_settings: &Default::default(), auto_reviews: (&Default::default(), &Default::default(), &Default::default()), machines: &Default::default(),
            })
        };
        let marked = HashMap::from([(path.to_string(), true)]);
        let b = board_with(vec![shipped.clone(), asked.clone(), fresh.clone()], &marked);
        let lane = &b.lanes[0];
        // The one that needs you belongs to now whenever it began; the shipped one comes after,
        // and out of the worktree's status.
        let keys: Vec<(&str, bool)> = lane.sessions.iter().map(|s| (s.key.as_str(), s.earlier)).collect();
        assert_eq!(keys, vec![("c:asked", false), ("c:fresh", false), ("c:shipped", true)]);
        assert_eq!(lane.bay, Bay::Needs);
        assert_eq!(lane.reuse, Some(Reuse::Open));
        // Without the one that needs you, the unread reply from before doesn't make it Your turn.
        let b = board_with(vec![shipped.clone(), fresh.clone()], &marked);
        assert_eq!(b.lanes[0].bay, Bay::Idle);
        assert_eq!(b.lanes[0].reuse, Some(Reuse::Free));
        // Kept for its own work, nothing is set aside.
        let b = board_with(vec![shipped.clone(), asked.clone(), fresh.clone()], &HashMap::new());
        assert!(b.lanes[0].sessions.iter().all(|s| !s.earlier));
        assert_eq!(b.lanes[0].reuse, None);
    }

    #[test]
    fn a_session_working_in_a_worktree_from_the_checkout_goes_by_the_worktrees_branch() {
        let apps = AppNames::default();
        let mut wt = seed("/r/spark-portal", "portal");
        wt.repo_slug = Some("sprucehq/spark".into());
        let mut main = seed("/r/spark", "main");
        main.repo_slug = Some("sprucehq/spark".into());
        main.default_branch = Some("main".into());
        let seeds = vec![main, wt];
        let a = acct("claude2");
        // Started in the checkout on main, then made the worktree and did all its work there.
        let mut s = claude_session(&a, Some(&reg("idle", NOW)), Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
        s.cwd = "/r/spark".into();
        s.branch = Some("main".into());
        s.work = vec!["/r/spark-portal".into(); 5];
        // Started in the worktree itself, on the branch it had before.
        let mut earlier = s.clone();
        (earlier.id, earlier.key, earlier.cwd, earlier.branch) = ("e".into(), Session::key_for(Harness::Claude, "claude2", "e"), "/r/spark-portal".into(), Some("photos".into()));
        let prs = HashMap::from([("sprucehq/spark".to_string(), vec![pr(PrState::Open, 0)])]);
        let (git, visitors) = (HashMap::new(), HashMap::new());
        let order = vec!["Spruce".to_string()];
        let board = compose(Inputs {
            now: NOW,
            apps: &apps,
            seeds: &seeds,
            sessions: vec![s.clone(), earlier],
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
            session_settings: &Default::default(), auto_reviews: (&Default::default(), &Default::default(), &Default::default()), machines: &Default::default(),
        });
        assert_eq!(board.prs[0].sessions, vec![PrSession { key: s.key.clone(), lane: "/r/spark-portal".into(), pushes: 0, primary: None }]);
        // Main was the checkout's branch, not one it worked on: the windows show the worktree's,
        // and its PR in the session's header.
        let branch = |key: &str| board.lanes.iter().flat_map(|l| &l.sessions).find(|x| x.key == key).unwrap().branch.clone();
        assert_eq!(branch(&s.key), None);
        assert_eq!(branch(&Session::key_for(Harness::Claude, "claude2", "e")).as_deref(), Some("photos"));
    }

    #[test]
    fn a_session_that_moved_to_a_worktree_keeps_the_pr_of_its_folders_branch() {
        let apps = AppNames::default();
        let mut wt = seed("/r/spark-portal", "portal");
        wt.repo_slug = Some("sprucehq/spark".into());
        let mut checkout = seed("/r/spark", "photos");
        checkout.repo_slug = Some("sprucehq/spark".into());
        checkout.default_branch = Some("main".into());
        let seeds = vec![checkout, wt];
        // It worked on the photos PR in the checkout, then went on in the portal worktree.
        let mut s = claude_session(&acct("claude2"), Some(&reg("idle", NOW)), Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
        s.cwd = "/r/spark".into();
        s.branch = Some("photos".into());
        s.work = vec!["/r/spark-portal".into(); 5];
        let mut photos = pr(PrState::Open, 0);
        (photos.number, photos.url, photos.head_ref) = (1038, "https://github.com/sprucehq/spark/pull/1038".into(), "photos".into());
        let prs = HashMap::from([("sprucehq/spark".to_string(), vec![pr(PrState::Open, 0), photos])]);
        let (git, visitors) = (HashMap::new(), HashMap::new());
        let order = vec!["Spruce".to_string()];
        let board = compose(Inputs { now: NOW, apps: &apps, seeds: &seeds, sessions: vec![s.clone()], visitors: &visitors, git: &git, prs: &prs, requested: &HashMap::new(), made: &HashMap::new(), project_order: &order, notices: vec![], loading: false, github_at: None, pinned: &[], github_login: None, pr_links: &PrLinks::default(), previews: &Previews::default(), reusable: &HashMap::new(), review_snoozes: &[], session_settings: &Default::default(), auto_reviews: (&Default::default(), &Default::default(), &Default::default()), machines: &Default::default() });
        let linked = |number: u64| board.prs.iter().find(|p| p.number == number).unwrap().sessions.iter().map(|x| x.key.clone()).collect::<Vec<_>>();
        assert_eq!((linked(1037), linked(1038)), (vec![s.key.clone()], vec![s.key.clone()]));
        let listed = board.lanes.iter().find(|l| l.path == "/r/spark-portal").unwrap();
        assert_eq!(listed.sessions[0].branch, None);
    }

    #[test]
    fn a_long_lived_branch_holds_only_its_latest_pr() {
        let apps = AppNames::default();
        let seeds = vec![seed("/r/spark-portal", "portal")];
        let a = acct("claude2");
        let mut earlier = claude_session(&a, None, Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
        earlier.key = "claude:claude2:earlier".into();
        earlier.branch = Some("portal".into());
        earlier.started_at = Some(NOW - 8 * DAY_MS);
        earlier.last_activity = NOW - 6 * DAY_MS;
        let mut latest = earlier.clone();
        latest.key = "claude:claude2:latest".into();
        latest.started_at = Some(NOW - 7_200_000);
        latest.last_activity = NOW - 60_000;
        // The branch's first PR merged five days ago, and the worktree has moved on to the next.
        let mut first = pr(PrState::Merged, 0);
        first.number = 1015;
        first.head_oid = Some("1111111".into());
        first.created_at = NOW - 10 * DAY_MS;
        first.merged_at = Some(NOW - 5 * DAY_MS);
        let prs = HashMap::from([("sprucehq/spark".to_string(), vec![first, pr(PrState::Open, 0)])]);
        let (git, visitors) = (HashMap::new(), HashMap::new());
        let order = vec!["Spruce".to_string()];
        let board = compose(Inputs {
            now: NOW,
            apps: &apps,
            seeds: &seeds,
            sessions: vec![earlier.clone(), latest.clone()],
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
            session_settings: &Default::default(), auto_reviews: (&Default::default(), &Default::default(), &Default::default()), machines: &Default::default(),
        });
        let get = |n: u64| board.prs.iter().find(|p| p.number == n).unwrap();
        let listed = |key: &str| PrSession { key: key.into(), lane: "/r/spark-portal".into(), pushes: 0, primary: None };
        assert_eq!(get(1037).lanes, vec!["/r/spark-portal".to_string()]);
        assert_eq!(get(1037).sessions, vec![listed(&latest.key)]);
        assert!(get(1015).lanes.is_empty(), "the worktree holds the newer PR, as its heading in Sessions shows");
        assert_eq!(get(1015).sessions, vec![listed(&earlier.key)], "its sessions still show, under the worktree they ran in");
    }

    #[test]
    fn prs_link_the_sessions_that_made_or_worked_in_their_worktree() {
        let apps = AppNames::default();
        let scratch = "/private/tmp/claude-501/-r-vault/abc12345-0000-0000-0000-000000000000/scratchpad/spark-nx-cache";
        let seeds = vec![seed("/r/vault", "main"), seed(scratch, "nx-cache")];
        let a = acct("claude");
        let mut maker = claude_session(&a, Some(&reg("idle", NOW)), Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
        maker.id = "abc12345-0000-0000-0000-000000000000".into();
        maker.key = Session::key_for(Harness::Claude, "claude", &maker.id);
        maker.cwd = "/r/vault".into();
        // A session records the branch of the folder it runs in, not the one it made.
        maker.branch = Some("main".into());
        let mut helper = maker.clone();
        helper.id = "helper".into();
        helper.key = "claude:claude:helper".into();
        let mut earlier = helper.clone();
        earlier.id = "earlier".into();
        earlier.key = "claude:claude:earlier".into();
        earlier.started_at = Some(NOW - 3 * DAY_MS);
        earlier.last_activity = NOW - 2 * DAY_MS;
        let visitors = HashMap::from([(PathBuf::from(scratch), vec![helper.key.clone(), earlier.key.clone()])]);
        let (git, order) = (HashMap::new(), vec!["Spruce".to_string()]);
        let board_with = |p: PrRaw| {
            let prs = HashMap::from([("sprucehq/spark".to_string(), vec![PrRaw { head_ref: "nx-cache".into(), ..p }])]);
            compose(Inputs {
                now: NOW,
                apps: &apps,
                seeds: &seeds,
                sessions: vec![maker.clone(), helper.clone(), earlier.clone()],
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
                session_settings: &Default::default(), auto_reviews: (&Default::default(), &Default::default(), &Default::default()), machines: &Default::default(),
            })
        };
        let listed = |key: &str| PrSession { key: key.into(), lane: scratch.into(), pushes: 0, primary: None };
        let board = board_with(pr(PrState::Open, 0));
        assert_eq!(board.prs[0].lanes, vec![scratch.to_string()]);
        // The maker, and the session that worked there while the PR was open, listed under the
        // worktree; not the one that worked there two days before the PR opened.
        assert_eq!(board.prs[0].sessions, vec![listed(&maker.key), listed(&helper.key)]);
        // Sessions that started after the PR merged didn't work on it, even the maker.
        let mut merged = pr(PrState::Merged, 0);
        merged.merged_at = Some(NOW - 2 * 3_600_000);
        assert!(board_with(merged).prs[0].sessions.is_empty());
    }

    #[test]
    fn maps_claude_statuses() {
        let a = acct("claude2");
        let s = |status: &str, updated: i64, seen: i64| claude_session(&a, Some(&reg(status, updated)), Some(&tr()), seen, NOW, &Quiet::default()).unwrap().status;
        assert_eq!(s("busy", NOW, 0), Status::Working);
        assert_eq!(s("shell", NOW, 0), Status::Working);
        assert_eq!(s("waiting", NOW, 0), Status::Needs);
        assert_eq!(s("idle", NOW - 10, NOW - 100), Status::Unread);
        assert_eq!(s("idle", NOW - 100, NOW - 10), Status::Idle);
        let stopped = claude_session(&a, None, Some(&tr()), NOW - 10, NOW, &Quiet::default()).unwrap();
        assert_eq!(stopped.status, Status::Stopped);
        assert!(!stopped.live);
        assert_eq!(stopped.resume_command.as_deref(), Some("claude2 --resume abc1234567"));
        assert_eq!(stopped.open_target, "s|claude:claude2:abc1234567|resume");
        assert_eq!(stopped.title, "Portal daily log + photo sync");
    }

    #[test]
    fn a_name_given_a_session_comes_before_the_ones_claude_code_gives_it() {
        let a = acct("claude2");
        let title = |t: &TranscriptInfo| claude_session(&a, None, Some(t), NOW - 10, NOW, &Quiet::default()).unwrap().title;
        let own = TranscriptInfo { title: None, agent_name: Some("Atlas nightly sync".into()), ai_title: Some("Atlas sync, nightly".into()), ..tr() };
        assert_eq!(title(&own), "Atlas nightly sync");
        assert_eq!(title(&TranscriptInfo { title: Some("Nightly digest".into()), ..own }), "Nightly digest");
    }

    #[test]
    fn a_turn_you_havent_seen_outlives_its_process() {
        // The Mac restarted, or its terminal was closed, after a turn you hadn't seen yet.
        let a = acct("claude2");
        let unseen = claude_session(&a, None, Some(&tr()), NOW - 120_000, NOW, &Quiet::default()).unwrap();
        assert_eq!((unseen.status, unseen.live, unseen.excerpt.as_deref()), (Status::Unread, false, Some("Done.")));
        assert_eq!(unseen.open_target, "s|claude:claude2:abc1234567|resume");
        // A reply that asks you something still needs you, and a turn its limit refused is still one.
        let mut asks = unseen.clone();
        apply_insight(&mut asks, &insight(|i| i.asks_you = Some(0.9)));
        assert_eq!(asks.status, Status::Needs);
        let limited = TranscriptInfo { limit: Some(crate::model::SessionLimit { at: NOW - 60_000, ..Default::default() }), ..tr() };
        assert_eq!(claude_session(&a, None, Some(&limited), NOW - 120_000, NOW, &Quiet::default()).unwrap().status, Status::Error);
        // Seen, it's stopped, and so is one you left unseen for three days.
        assert_eq!(claude_session(&a, None, Some(&tr()), NOW - 60_000, NOW, &Quiet::default()).unwrap().status, Status::Stopped);
        let old = TranscriptInfo { last_ts: NOW - 3 * DAY_MS, ..tr() };
        assert_eq!(claude_session(&a, None, Some(&old), 0, NOW, &Quiet::default()).unwrap().status, Status::Stopped);
        let recent = TranscriptInfo { last_ts: NOW - 3 * DAY_MS + 60_000, ..tr() };
        assert_eq!(claude_session(&a, None, Some(&recent), 0, NOW, &Quiet::default()).unwrap().status, Status::Unread);
    }

    #[test]
    fn a_held_message_is_what_it_waits_on() {
        let a = acct("claude2");
        let held = crate::claude::Held { id: "h1".into(), from: "“Review 1021”".into(), count: 1, at: NOW - 30_000, ..Default::default() };
        let s = |t: &TranscriptInfo| claude_session(&a, Some(&reg("waiting", NOW - 30_000)), Some(t), 0, NOW, &Quiet::default()).unwrap();
        // Idle when it came: nothing else open.
        let idle = s(&TranscriptInfo { held: Some(held.clone()), pending_tool: None, ..tr() });
        assert_eq!((idle.status, idle.excerpt.as_deref()), (Status::Needs, Some("Held message from “Review 1021”")));
        assert_eq!(session_headline(&idle), ("`claude2` holds a message for you · Portal daily log + photo sync".to_string(), Some("from “Review 1021”".to_string())));
        // Busy when it came: the command it was running isn't what it asks about.
        let busy = s(&TranscriptInfo { held: Some(held.clone()), pending_tool_at: Some(NOW - 40_000), ..tr() });
        assert_eq!(busy.excerpt.as_deref(), Some("Held message from “Review 1021”"));
        // A call made since may be.
        let since = s(&TranscriptInfo { held: Some(held), pending_tool_at: Some(NOW - 10_000), ..tr() });
        assert_eq!(since.excerpt.as_deref(), Some("Bash · `yarn mikro-orm migration:up`"));
    }

    /// A background job's record says what it asks while its transcript has nothing of it yet.
    #[test]
    fn a_jobs_question_is_what_it_waits_on() {
        let a = acct("claude");
        let job = RegistryEntry { kind: Some("bg".into()), job_id: Some("1a2b3c4d".into()), asks: Some("Keep its settings?\n(ticks every 15 min)".into()), ..reg("waiting", NOW - 30_000) };
        let s = claude_session(&a, Some(&job), Some(&TranscriptInfo { pending_tool: None, ..tr() }), 0, NOW, &Quiet::default()).unwrap();
        assert_eq!((s.status, s.excerpt.as_deref()), (Status::Needs, Some("Asks you · Keep its settings?")));
        assert_eq!(session_headline(&s), ("`claude` asks you · Portal daily log + photo sync".to_string(), Some("Keep its settings?".to_string())));
    }

    #[test]
    fn a_turn_a_usage_limit_stopped_needs_you_until_seen() {
        let a = acct("claude2");
        let limit = crate::model::SessionLimit { meter: Some("session".into()), at: NOW - 10, ..Default::default() };
        let t = TranscriptInfo { limit: Some(limit.clone()), last_text: Some("You've hit your session limit · resets 6:10pm".into()), ..tr() };
        let s = |status: &str, seen: i64| claude_session(&a, Some(&reg(status, NOW - 10)), Some(&t), seen, NOW, &Quiet::default()).unwrap();
        let unseen = s("idle", NOW - 100);
        assert_eq!((unseen.status, unseen.limit.as_ref()), (Status::Error, Some(&limit)));
        assert_eq!(session_headline(&unseen), ("`claude2` hit its usage limit · Portal daily log + photo sync".to_string(), Some("You've hit your session limit · resets 6:10pm".to_string())));
        // Seen, it's at rest with the limit still said; tried again, it's working and isn't limited.
        let seen = s("idle", NOW);
        assert_eq!((seen.status, seen.limit.is_some()), (Status::Idle, true));
        let busy = s("busy", NOW - 100);
        assert_eq!((busy.status, busy.limit), (Status::Working, None));
    }

    #[test]
    fn a_turn_a_lost_connection_ended_says_so_until_it_works_again() {
        let a = acct("claude");
        let t = TranscriptInfo { connection_lost: Some(NOW - 10), ..tr() };
        let s = |status: &str| claude_session(&a, Some(&reg(status, NOW - 10)), Some(&t), 0, NOW, &Quiet::default()).unwrap();
        // A finished turn to read, as any other; the engine says whether it's carried on.
        let ended = s("idle");
        assert_eq!((ended.status, ended.connection_lost), (Status::Unread, Some(ConnectionLost { at: NOW - 10, continues: false })));
        let busy = s("busy");
        assert_eq!((busy.status, busy.connection_lost), (Status::Working, None));
    }

    /// `/color`'s names as it gives them, and none for one it doesn't.
    #[test]
    fn the_color_it_was_given_is_kept_by_name() {
        let a = acct("claude");
        let s = |color: Option<&str>| claude_session(&a, Some(&reg("idle", NOW)), Some(&TranscriptInfo { color: color.map(String::from), ..tr() }), 0, NOW, &Quiet::default()).unwrap().color;
        assert_eq!((s(Some("Cyan")), s(Some("orange")), s(Some("default")), s(Some("mauve")), s(None)), (Some("cyan".into()), Some("orange".into()), None, None, None));
    }

    #[test]
    fn a_background_job_is_attached_not_focused() {
        let a = acct("claude2");
        let mut r = reg("idle", NOW);
        r.kind = Some("bg".into());
        r.job_id = Some("6e07c149".into());
        let s = claude_session(&a, Some(&r), Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
        assert_eq!(s.attach_command.as_deref(), Some("claude2 attach 6e07c149"));
        assert_eq!(s.open_target, "s|claude:claude2:abc1234567|attach");
    }

    #[test]
    fn a_conversation_moved_to_the_background_shows_once_as_its_job() {
        let mut old = tr();
        old.continued_in = Some("398f3533-73b3".into());
        let moves = |live: &[&str], talked: bool| {
            let job = TranscriptInfo { session_id: "398f3533-73b3".into(), ..tr() };
            let mut transcripts: HashMap<&str, &TranscriptInfo> = HashMap::from([("abc1234567", &old)]);
            if talked {
                transcripts.insert("398f3533-73b3", &job);
            }
            claude_moves(&live.iter().copied().collect(), &transcripts)
        };
        // Before its next prompt the job reads the old transcript, which doesn't show.
        let running = moves(&["398f3533-73b3"], false);
        assert_eq!(running.from, HashMap::from([("398f3533-73b3".to_string(), "abc1234567".to_string())]));
        assert_eq!(running.moved, HashMap::from([("abc1234567".to_string(), "398f3533-73b3".to_string())]));
        // After it, the job's own transcript has the conversation.
        let talked = moves(&["398f3533-73b3"], true);
        assert!(talked.from.is_empty());
        assert_eq!(talked.moved.len(), 1);
        assert_eq!(moves(&[], true).moved.len(), 1);
        // A job that went before a word was said to it leaves the conversation where it was.
        assert!(moves(&[], false).moved.is_empty());
        // So does taking it back to its window.
        assert!(moves(&["abc1234567", "398f3533-73b3"], false).moved.is_empty());
    }

    #[test]
    fn a_moved_conversation_keeps_its_place_and_what_you_saw() {
        let a = acct("claude2");
        let old = TranscriptInfo { started_at: Some(NOW - 5 * 3_600_000), last_prompt_at: Some(NOW - 2 * 3_600_000), last_ts: NOW - 3_600_000, ..tr() };
        let mut job = reg("idle", NOW - 60_000);
        job.session_id = "398f3533-73b3".into();
        job.kind = Some("bg".into());
        job.job_id = Some("398f3533".into());
        job.started_at = Some(NOW - 61_000);
        let moved = moved_registry(&job, &old);
        // You saw its last reply before it moved: it isn't new.
        let s = claude_session(&a, Some(&moved), Some(&old), NOW - 30 * 60_000, NOW, &Quiet::default()).unwrap();
        assert_eq!(s.status, Status::Idle);
        assert_eq!((s.started_at, s.last_prompt_at, s.last_activity), (Some(NOW - 5 * 3_600_000), NOW - 2 * 3_600_000, NOW - 3_600_000));
        assert_eq!(s.excerpt.as_deref(), Some("Done."));
        assert_eq!(s.attach_command.as_deref(), Some("claude2 attach 398f3533"));
        // One you hadn't seen still is.
        assert_eq!(claude_session(&a, Some(&moved), Some(&old), NOW - 2 * 3_600_000, NOW, &Quiet::default()).unwrap().status, Status::Unread);
        // Once it's working the job's own times hold.
        assert_eq!(moved_registry(&reg("busy", NOW - 1_000), &old).status_updated_at, Some(NOW - 1_000));
    }

    #[test]
    fn a_restarted_process_that_has_said_nothing_since_keeps_its_place_and_what_you_saw() {
        let a = acct("claude");
        // Claude Code's background service restarted the job on a new version a minute ago,
        // hours after its last reply, which you'd seen.
        let said = TranscriptInfo { started_at: Some(NOW - 5 * 3_600_000), last_prompt_at: Some(NOW - 4 * 3_600_000), last_ts: NOW - 3 * 3_600_000, ..tr() };
        let job = |status: &str, updated: i64| RegistryEntry { kind: Some("bg".into()), job_id: Some("b887d77f".into()), started_at: Some(NOW - 60_000), ..reg(status, updated) };
        let s = claude_session(&a, Some(&job("idle", NOW - 58_000)), Some(&said), NOW - 2 * 3_600_000, NOW, &Quiet::default()).unwrap();
        assert_eq!(s.status, Status::Idle);
        assert_eq!((s.started_at, s.status_since, s.last_activity), (Some(NOW - 5 * 3_600_000), Some(NOW - 3 * 3_600_000), NOW - 3 * 3_600_000));
        assert_eq!(s.excerpt.as_deref(), Some("Done."));
        // A reply you hadn't seen still waits for you.
        assert_eq!(claude_session(&a, Some(&job("idle", NOW - 58_000)), Some(&said), NOW - 4 * 3_600_000, NOW, &Quiet::default()).unwrap().status, Status::Unread);
        // A turn it takes after coming up is news once it ends, dated by the process.
        let answered = TranscriptInfo { last_ts: NOW - 10_000, ..said.clone() };
        let s = claude_session(&a, Some(&job("idle", NOW - 9_000)), Some(&answered), NOW - 2 * 3_600_000, NOW, &Quiet::default()).unwrap();
        assert_eq!((s.status, s.started_at, s.last_activity), (Status::Unread, Some(NOW - 60_000), NOW - 9_000));
        let busy = claude_session(&a, Some(&job("busy", NOW - 30_000)), Some(&said), NOW - 2 * 3_600_000, NOW, &Quiet::default()).unwrap();
        assert_eq!((busy.status, busy.started_at), (Status::Working, Some(NOW - 60_000)));
    }

    #[test]
    fn a_conversation_compacted_while_idle_keeps_its_place_and_what_you_saw() {
        let a = acct("claude");
        // Its turn ended an hour ago and you saw it; Claude Code compacted it a minute ago, going
        // busy and idle again and writing lines of its own.
        let compacted = TranscriptInfo { last_ts: NOW - 50_000, compacted: Some(NOW - 3_600_000), ..tr() };
        let seen = NOW - 30 * 60_000;
        let s = claude_session(&a, Some(&reg("idle", NOW - 49_000)), Some(&compacted), seen, NOW, &Quiet::default()).unwrap();
        assert_eq!((s.status, s.status_since, s.last_activity), (Status::Idle, Some(NOW - 3_600_000), NOW - 3_600_000));
        assert_eq!(s.excerpt.as_deref(), Some("Done."));
        // A turn you hadn't seen still waits for you.
        assert_eq!(claude_session(&a, Some(&reg("idle", NOW - 49_000)), Some(&compacted), NOW - 2 * 3_600_000, NOW, &Quiet::default()).unwrap().status, Status::Unread);
        // Its process gone since, it's stopped, not unread.
        assert_eq!(claude_session(&a, None, Some(&compacted), seen, NOW, &Quiet::default()).unwrap().status, Status::Stopped);
        // Without the compaction, the registry's time is a turn that ended.
        let said = TranscriptInfo { compacted: None, ..compacted.clone() };
        assert_eq!(claude_session(&a, Some(&reg("idle", NOW - 49_000)), Some(&said), seen, NOW, &Quiet::default()).unwrap().status, Status::Unread);
        // While it compacts it's working.
        assert_eq!(claude_session(&a, Some(&reg("busy", NOW - 30_000)), Some(&compacted), seen, NOW, &Quiet::default()).unwrap().status, Status::Working);
    }

    #[test]
    fn a_long_silent_busy_session_is_stuck() {
        let a = acct("claude2");
        let mut t = tr();
        t.last_ts = NOW - 3 * DAY_MS;
        let r = reg("busy", NOW - 3 * DAY_MS);
        let stuck = claude_session(&a, Some(&r), Some(&t), 0, NOW, &Quiet { subagent_at: None, running: Some("gcloud auth login"), ..Default::default() }).unwrap();
        assert_eq!(stuck.status, Status::Needs);
        assert_eq!(stuck.stalled_since, Some(NOW - 3 * DAY_MS));
        assert_eq!(stuck.excerpt.as_deref(), Some(format!("Stuck on `gcloud auth login` for {{for:{}}}", NOW - 3 * DAY_MS).as_str()));
        assert_eq!(session_headline(&stuck).0, "`claude2` is stuck · Portal daily log + photo sync");
        let silent = claude_session(&a, Some(&r), Some(&t), 0, NOW, &Quiet::default()).unwrap();
        assert!(silent.excerpt.as_deref().unwrap_or_default().starts_with("No activity for {for:"));
        // A subagent still writing its log keeps the session working.
        let sub = claude_session(&a, Some(&r), Some(&t), 0, NOW, &Quiet { subagent_at: Some(NOW - 60_000), running: None, ..Default::default() }).unwrap();
        assert_eq!(sub.status, Status::Working);
        // So does a transcript that's still moving.
        t.last_ts = NOW - 60_000;
        let busy = claude_session(&a, Some(&reg("busy", NOW - 3 * DAY_MS)), Some(&t), 0, NOW, &Quiet::default()).unwrap();
        assert_eq!(busy.status, Status::Working);
        assert_eq!(busy.stalled_since, None);
    }

    /// The Claude app's built-in browser asks before it reads a site, inside a call Claude Code
    /// is still making: the session says busy, its transcript goes quiet, and it waits on you.
    #[test]
    fn an_approval_up_in_the_claude_app_needs_you() {
        let a = acct("claude");
        let mut t = tr();
        t.last_ts = NOW - 12 * 3_600_000;
        let r = reg("busy", NOW - DAY_MS);
        let ask = crate::claude::applog::Ask { id: "a1".into(), session: "local_c3".into(), tool: "browser:screenshot".into(), site: Some("https://app.northwind.dev".into()), at: NOW - 12 * 3_600_000 };
        let quiet = Quiet { app_ask: Some(&ask), ..Default::default() };
        let s = claude_session(&a, Some(&r), Some(&t), 0, NOW, &quiet).unwrap();
        assert_eq!((s.status, s.stalled_since), (Status::Needs, None), "waiting on you, not stuck");
        assert_eq!(s.status_since, Some(NOW - 12 * 3_600_000), "since the app asked, not since the turn began");
        assert_eq!(s.excerpt.as_deref(), Some("browser · screenshot · `app.northwind.dev`"));
        // Asked right away, before the quiet that makes a session stuck.
        t.last_ts = NOW - 5_000;
        assert_eq!(claude_session(&a, Some(&r), Some(&t), 0, NOW, &quiet).unwrap().status, Status::Needs);
        // Only while the turn runs: once it's over, the request went with it.
        assert_eq!(claude_session(&a, Some(&reg("idle", NOW - 1_000)), Some(&t), 0, NOW, &quiet).unwrap().status, Status::Unread);
    }

    fn insight(f: impl FnOnce(&mut SessionInsight)) -> SessionInsight {
        let mut i = SessionInsight::default();
        f(&mut i);
        i
    }

    #[test]
    fn finds_the_last_sentence() {
        assert_eq!(last_sentence("…tests pass in App.tsx. Should I also migrate the old table?"), "Should I also migrate the old table?");
        assert_eq!(last_sentence("One line only"), "One line only");
        assert_eq!(last_sentence("Done! Next up: the docs."), "Next up: the docs.");
        assert_eq!(last_sentence(""), "");
    }

    /// A reply that asks often goes on after its question; the row shows the question.
    #[test]
    fn finds_the_question_a_reply_asks() {
        assert_eq!(question_in("…would go against that choice. Do you want me to make that change? To test, I trusted a folder. I removed it after."), "Do you want me to make that change?");
        assert_eq!(question_in("**Do you want these as main addresses, or is forwarding enough?** Sources: - [Google](https://support.google.com/mail/answer/22370?hl=en)"), "**Do you want these as main addresses, or is forwarding enough?**");
        assert_eq!(question_in("Should I merge it? Or should I wait for Wes?"), "Or should I wait for Wes?", "the last of several");
        assert_eq!(question_in("I'm still waiting on your `aws sso login` to rerun the plan."), "I'm still waiting on your `aws sso login` to rerun the plan.", "no question mark: the last sentence");
        assert_eq!(question_in("See claude.ai/settings/usage?from=cli for more. Then tell me."), "Then tell me.", "a question mark inside a link isn't a question");
    }

    #[test]
    fn tells_a_reply_with_a_question_in_it() {
        assert!(has_question("Deleted the branch. **Should I keep the worktree?**"), "set in Markdown");
        assert!(has_question("Do you want me to make that change? To test, I trusted a folder."), "with more after it");
        assert!(has_question("Pushed it. Merge it now?\n"));
        assert!(!has_question("Deleted the branch; this session closes now."));
        assert!(!has_question("See claude.ai/settings/usage?from=cli for more. Then tell me."), "a question mark inside a link isn't a question");
    }

    #[test]
    fn a_finished_turn_that_asks_you_something_needs_you() {
        let a = acct("claude2");
        let mut s = claude_session(&a, Some(&reg("idle", NOW - 10)), Some(&tr()), NOW - 100, NOW, &Quiet::default()).unwrap();
        s.reply_tail = Some("…all green. Want me to open the PR, or keep it local?".into());
        assert_eq!(s.status, Status::Unread);
        let mut unsure = s.clone();
        apply_insight(&mut unsure, &insight(|i| i.asks_you = Some(0.5)));
        assert_eq!(unsure.status, Status::Unread);
        assert!(unsure.insight.is_some() && !unsure.asks);
        apply_insight(&mut s, &insight(|i| i.asks_you = Some(0.9)));
        assert_eq!(s.status, Status::Needs);
        // It sits at its prompt, so a reply is its answer.
        assert!(s.asks);
        assert_eq!(s.excerpt.as_deref(), Some("Asks you · Want me to open the PR, or keep it local?"));
        // Seen already, it's yours to answer when you like: no change.
        let mut idle = claude_session(&a, Some(&reg("idle", NOW - 100)), Some(&tr()), NOW - 10, NOW, &Quiet::default()).unwrap();
        apply_insight(&mut idle, &insight(|i| i.asks_you = Some(0.9)));
        assert_eq!(idle.status, Status::Idle);
        assert!(!idle.asks);
        // Waiting on a permission prompt is never a question to type an answer to.
        let mut prompt = claude_session(&a, Some(&reg("waiting", NOW - 10)), Some(&tr()), NOW - 100, NOW, &Quiet::default()).unwrap();
        apply_insight(&mut prompt, &insight(|i| i.asks_you = Some(0.9)));
        assert_eq!((prompt.status, prompt.asks), (Status::Needs, false));
    }

    #[test]
    fn a_snooze_keeps_a_session_off_until_its_time_then_brings_it_back() {
        const MIN: i64 = 60_000;
        let a = acct("claude2");
        let z = Snooze { key: "k".into(), at: NOW - 10 * MIN, until: NOW + 50 * MIN };
        let unread = claude_session(&a, Some(&reg("idle", NOW - 20 * MIN)), Some(&tr()), NOW - 30 * MIN, NOW, &Quiet::default()).unwrap();
        assert_eq!(unread.status, Status::Unread);
        assert!(unread.last_prompt_at < z.at);
        let snoozed = |s: &Session, seen: i64, now: i64| {
            let mut s = s.clone();
            apply_snooze(&mut s, Some(&z), seen, now);
            (s.status, s.snoozed_until, s.status_since)
        };
        // Until its time it keeps its status and says when it's back.
        assert_eq!(snoozed(&unread, NOW - 30 * MIN, NOW), (Status::Unread, Some(z.until), unread.status_since));
        // Then it's back as it is.
        assert_eq!(snoozed(&unread, NOW - 30 * MIN, z.until), (Status::Unread, None, unread.status_since));
        // One with nothing to show for itself is Your turn from its time until you've seen it.
        let idle = Session { status: Status::Idle, ..unread.clone() };
        assert_eq!(snoozed(&idle, NOW - 5 * MIN, z.until + MIN), (Status::Unread, None, Some(z.until)));
        let stopped = Session { status: Status::Stopped, live: false, ..unread.clone() };
        assert_eq!(snoozed(&stopped, NOW - 5 * MIN, z.until + MIN).0, Status::Unread);
        assert_eq!(snoozed(&idle, z.until + MIN, z.until + 2 * MIN).0, Status::Idle, "seen since");
        let working = Session { status: Status::Working, ..unread.clone() };
        assert_eq!(snoozed(&working, NOW - 5 * MIN, z.until + MIN).0, Status::Working);
        // A prompt sent since snoozing ends it.
        let prompted = Session { status: Status::Idle, last_prompt_at: NOW - 5 * MIN, ..unread.clone() };
        assert_eq!(snoozed(&prompted, NOW - 30 * MIN, NOW), (Status::Idle, None, prompted.status_since));
        assert_eq!(snoozed(&prompted, NOW - 30 * MIN, z.until + MIN).0, Status::Idle);
        let archived = Session { archived: true, ..idle.clone() };
        assert_eq!(snoozed(&archived, NOW - 30 * MIN, z.until + MIN).0, Status::Idle);
        let mut untouched = idle.clone();
        apply_snooze(&mut untouched, None, 0, NOW);
        assert_eq!((untouched.status, untouched.snoozed_until), (Status::Idle, None));
    }

    #[test]
    fn a_move_to_the_top_holds_until_its_session_is_taken_up() {
        const MIN: i64 = 60_000;
        let at = NOW - 10 * MIN;
        let raise = |agent: bool| Raise { key: "k".into(), raised: Raised { at, agent, ..Default::default() }, lowered: None };
        let unread = Session { status: Status::Unread, status_since: Some(NOW - 30 * MIN), last_prompt_at: NOW - 40 * MIN, ..crate::fixtures::session("k") };
        let raised = |s: &Session, r: Option<&Raise>| {
            let mut s = s.clone();
            apply_raise(&mut s, r);
            (s.status, s.status_since, s.raised.map(|r| r.at))
        };
        // Waiting on you, it keeps its status and its wait, and says it's first.
        assert_eq!(raised(&unread, Some(&raise(false))), (Status::Unread, unread.status_since, Some(at)));
        let needs = Session { status: Status::Needs, ..unread.clone() };
        assert_eq!(raised(&needs, Some(&raise(true))).2, Some(at));
        // One that's running keeps it, for when its turn ends.
        let working = Session { status: Status::Working, ..unread.clone() };
        assert_eq!(raised(&working, Some(&raise(false))), (Status::Working, unread.status_since, Some(at)));
        // One whose turn is over and seen waits on you again, from when it was moved.
        let idle = Session { status: Status::Idle, ..unread.clone() };
        assert_eq!(raised(&idle, Some(&raise(false))), (Status::Unread, Some(at), Some(at)));
        let stopped = Session { status: Status::Stopped, live: false, ..unread.clone() };
        assert_eq!(raised(&stopped, Some(&raise(false))).0, Status::Unread);
        // A prompt sent to it since takes it up.
        let prompted = Session { status: Status::Idle, last_prompt_at: at + MIN, ..unread.clone() };
        assert_eq!(raised(&prompted, Some(&raise(false))), (Status::Idle, unread.status_since, None));
        // Moved back to its place, archived, or never moved, it shows nothing.
        let lowered = Raise { lowered: Some(NOW), ..raise(true) };
        assert_eq!(raised(&unread, Some(&lowered)).2, None);
        assert_eq!(raised(&Session { archived: true, ..idle.clone() }, Some(&raise(false))), (Status::Idle, unread.status_since, None));
        assert_eq!(raised(&idle, None), (Status::Idle, unread.status_since, None));
        // Seeing it takes it up once its turn is over, not while it runs or waits on a call.
        assert!(seen_takes_up(&unread) && seen_takes_up(&idle) && seen_takes_up(&Session { asks: true, ..needs.clone() }));
        assert!(seen_takes_up(&Session { status: Status::Error, cut_off: Some(NOW), ..unread.clone() }), "a turn cut off");
        assert!(!seen_takes_up(&working) && !seen_takes_up(&needs) && !seen_takes_up(&Session { status: Status::Error, ..unread.clone() }));
    }

    #[test]
    fn moved_to_the_top_goes_first_yours_before_agents_the_latest_first() {
        use std::cmp::Ordering::{Greater, Less};
        let r = |at: i64, agent: bool| Raised { at, agent, ..Default::default() };
        assert_eq!(raised_order(Some(&r(1, true)), None), Less);
        assert_eq!(raised_order(None, Some(&r(1, false))), Greater);
        assert_eq!(raised_order(Some(&r(2, false)), Some(&r(1, false))), Less, "the latest first");
        assert_eq!(raised_order(Some(&r(1, false)), Some(&r(9, true))), Less, "yours before an agent's");
        assert!(raised_order(None, None).is_eq());
        // In a lane, first among its kind: never ahead of a more urgent status.
        let s = |key: &str, status: Status, raised: Option<Raised>| Session { status, status_since: Some(NOW), raised, ..crate::fixtures::session(key) };
        let mut sessions = vec![s("a", Status::Unread, None), s("b", Status::Unread, Some(r(NOW, true))), s("c", Status::Needs, None)];
        sort_sessions(&mut sessions);
        assert_eq!(sessions.iter().map(|s| s.key.as_str()).collect::<Vec<_>>(), ["c", "b", "a"]);
    }

    #[test]
    fn an_agent_may_not_move_up_what_you_snoozed_or_moved_back() {
        let s = Session { status: Status::Unread, status_since: Some(NOW - 1_000), last_prompt_at: NOW - 2_000, ..crate::fixtures::session("k") };
        let lowered = |at: i64| Raise { key: "k".into(), raised: Raised { at: NOW - 5_000, agent: true, ..Default::default() }, lowered: Some(at) };
        assert_eq!(raise_refused(&s, None, true), None);
        assert!(raise_refused(&Session { snoozed_until: Some(NOW + 1), ..s.clone() }, None, true).is_some_and(|w| w.contains("snoozed")));
        assert!(raise_refused(&s, Some(&lowered(NOW)), true).is_some_and(|w| w.contains("back to its place")));
        // Once it's moved on since, it may.
        assert_eq!(raise_refused(&s, Some(&lowered(NOW - 1_500)), true), None, "its status changed since");
        assert_eq!(raise_refused(&Session { last_prompt_at: NOW + 1, ..s.clone() }, Some(&lowered(NOW)), true), None, "sent a prompt since");
        // What you moved up is yours: an agent neither moves it again nor puts it back, though it
        // may put back its own.
        let yours = Session { raised: Some(Raised { at: NOW, ..Default::default() }), ..s.clone() };
        assert!(raise_refused(&yours, None, true).is_some_and(|w| w.contains("theirs to undo")));
        assert!(raise_refused(&yours, None, false).is_some_and(|w| w.contains("theirs to undo")));
        let its = Session { raised: Some(Raised { at: NOW, agent: true, ..Default::default() }), ..s.clone() };
        assert_eq!(raise_refused(&its, None, false), None);
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
    fn a_session_wears_the_labels_it_was_given_that_still_are() {
        let label = |id: &str, name: &str| Label { id: id.into(), name: name.into(), color: "blue".into() };
        let labels = vec![label("l1", "Release"), label("l2", "Bug")];
        let mut s = crate::fixtures::session("claude:claude:a");
        apply_labels(&mut s, &["l2".into(), "l-gone".into(), "l1".into()], &labels);
        assert_eq!(s.labels, vec![label("l2", "Bug"), label("l1", "Release")], "in the order given, a deleted one left out");
        apply_labels(&mut s, &[], &labels);
        assert!(s.labels.is_empty());
    }

    #[test]
    fn a_quiet_session_waits_on_you_only_when_it_needs_you() {
        const MIN: i64 = 60_000;
        let a = acct("claude2");
        let ended = claude_session(&a, Some(&reg("idle", NOW - 20 * MIN)), Some(&tr()), NOW - 30 * MIN, NOW, &Quiet::default()).unwrap();
        assert_eq!(ended.status, Status::Unread);
        let quiet = |s: &Session| {
            let mut s = s.clone();
            apply_quiet(&mut s, true);
            s
        };
        // A turn that ended is idle, so it's not Your turn: not its lane's bay, and not the mark.
        let q = quiet(&ended);
        assert_eq!((q.status, q.quiet, q.status_since), (Status::Idle, true, ended.status_since));
        assert_eq!(bay_of(std::slice::from_ref(&q), None).0, Bay::Idle);
        // A reply that asks you something is Needs you by then, and stays so.
        let mut asks = ended.clone();
        apply_insight(&mut asks, &insight(|i| i.asks_you = Some(0.9)));
        assert_eq!(quiet(&asks).status, Status::Needs);
        // A call to allow or a question, an error, and work under way show as they are.
        let waiting = claude_session(&a, Some(&reg("waiting", NOW - MIN)), Some(&tr()), NOW - 30 * MIN, NOW, &Quiet::default()).unwrap();
        assert_eq!(quiet(&waiting).status, Status::Needs);
        for st in [Status::Error, Status::Working, Status::Stopped] {
            assert_eq!(quiet(&Session { status: st, ..ended.clone() }).status, st);
        }
        // Not quiet, it's as it was.
        let mut heard = ended.clone();
        apply_quiet(&mut heard, false);
        assert_eq!((heard.status, heard.quiet), (Status::Unread, false));
        // Your own mark of unread, and a snooze's end, come after and bring it back.
        let mut marked = quiet(&ended);
        apply_unread(&mut marked, Some(&UnreadMark { key: ended.key.clone(), at: NOW - 5 * MIN }), NOW - 30 * MIN);
        assert_eq!(marked.status, Status::Unread);
        let mut woke = quiet(&ended);
        apply_snooze(&mut woke, Some(&Snooze { key: ended.key.clone(), at: NOW - 15 * MIN, until: NOW - MIN }), NOW - 30 * MIN, NOW);
        assert_eq!(woke.status, Status::Unread);
    }

    #[test]
    fn suggested_replies_hold_until_the_session_moves_on() {
        const MIN: i64 = 60_000;
        let a = acct("claude2");
        // Its turn ended 20 minutes ago, on a prompt sent 30 minutes ago; replies were suggested 10
        // minutes ago.
        let ended = claude_session(&a, Some(&reg("idle", NOW - 20 * MIN)), Some(&tr()), NOW - 30 * MIN, NOW, &Quiet::default()).unwrap();
        let ended = Session { last_prompt_at: NOW - 30 * MIN, ..ended };
        // It was read with its prompt of 30 minutes ago the last.
        let suggested = Suggestions { replies: vec!["Yes, push it".into(), "Run the tests first".into()], picks: Default::default(), why: Default::default(), note: Some("It asks whether to push".into()), by: Some("claude:claude:sup".into()), by_title: Some("Supervisor".into()), at: NOW - 10 * MIN, prompt_at: Some(NOW - 30 * MIN) };
        let with = |s: &Session, g: Option<&Suggestions>| {
            let mut s = s.clone();
            apply_suggestions(&mut s, g);
            s.suggestions
        };
        assert_eq!(with(&ended, Some(&suggested)), Some(suggested.clone()));
        // Waiting on you, or still at the turn they were suggested in, they hold.
        assert!(with(&Session { status: Status::Needs, ..ended.clone() }, Some(&suggested)).is_some());
        assert!(with(&Session { status: Status::Working, status_since: Some(NOW - 20 * MIN), ..ended.clone() }, Some(&suggested)).is_some());
        // A prompt sent since, from anywhere, answered them.
        assert_eq!(with(&Session { last_prompt_at: NOW - 5 * MIN, ..ended.clone() }, Some(&suggested)), None);
        // A prompt sent after it was read and before they were suggested answered the turn they
        // were written for: they're for the turn before.
        assert_eq!(with(&Session { last_prompt_at: NOW - 15 * MIN, ..ended.clone() }, Some(&suggested)), None);
        // Ones from a Mac that didn't say what was read go by when they were suggested.
        let undated = Suggestions { prompt_at: None, ..suggested.clone() };
        assert!(with(&Session { last_prompt_at: NOW - 15 * MIN, ..ended.clone() }, Some(&undated)).is_some());
        // A turn started since (a background task woke it) has moved on from them.
        assert!(!suggestions_hold(&Session { status: Status::Working, status_since: Some(NOW - 5 * MIN), ..ended.clone() }, &suggested));
        // None, or none left, puts none on it, and takes away what it had.
        assert_eq!(with(&Session { suggestions: Some(suggested.clone()), ..ended.clone() }, None), None);
        assert_eq!(with(&ended, Some(&Suggestions { replies: vec![], ..suggested.clone() })), None);
        // Choices picked for the questions it asks hold as replies do.
        let picked = Suggestions { replies: vec![], picks: [("Push it?".to_string(), vec!["Yes".to_string()])].into(), ..suggested.clone() };
        assert_eq!(with(&Session { status: Status::Needs, ..ended.clone() }, Some(&picked)), Some(picked.clone()));
        assert_eq!(with(&Session { status: Status::Working, status_since: Some(NOW - 5 * MIN), ..ended.clone() }, Some(&picked)), None);
    }

    #[test]
    fn what_moved_a_session_on_says_when() {
        const MIN: i64 = 60_000;
        let a = acct("claude2");
        let ended = claude_session(&a, Some(&reg("idle", NOW - 20 * MIN)), Some(&tr()), NOW - 30 * MIN, NOW, &Quiet::default()).unwrap();
        let ended = Session { last_prompt_at: NOW - 30 * MIN, ..ended };
        // Read with its prompt of 30 minutes ago the last, tagged 10 minutes ago.
        let (read, at) = (NOW - 30 * MIN, NOW - 10 * MIN);
        assert_eq!(moved_on(&ended, read, at), None);
        assert_eq!(moved_on(&Session { last_prompt_at: NOW - 5 * MIN, ..ended.clone() }, read, at), Some(MovedOn::Prompt { at: NOW - 5 * MIN }));
        // A turn its user's prompt didn't start (a scheduled wakeup, another session's message).
        let woke = Session { status: Status::Working, status_since: Some(NOW - 2 * MIN), ..ended.clone() };
        assert_eq!(moved_on(&woke, read, at), Some(MovedOn::Turn { at: NOW - 2 * MIN }));
        // The turn it was read in is where it stands.
        assert_eq!(moved_on(&Session { status_since: Some(NOW - 20 * MIN), ..woke.clone() }, read, at), None);
        // A prompt says so over the turn it started.
        assert_eq!(moved_on(&Session { last_prompt_at: NOW - 2 * MIN, ..woke }, read, at), Some(MovedOn::Prompt { at: NOW - 2 * MIN }));
    }

    #[test]
    fn a_compaction_is_no_turn_for_tags_and_suggested_replies() {
        const MIN: i64 = 60_000;
        let a = acct("claude");
        // Its turn ended an hour ago, on a prompt sent before it; an agent read it then and both
        // tagged it and suggested replies 50 minutes ago.
        let (read, at) = (NOW - 70 * MIN, NOW - 50 * MIN);
        let ended = TranscriptInfo { last_prompt_at: Some(read), ended_at: Some(NOW - 60 * MIN), last_ts: NOW - 60 * MIN, ..tr() };
        let tag = crate::tags::Tag { insight: SessionInsight { asks_you: Some(1.0), ..Default::default() }, at, prompt_at: read };
        let suggested = Suggestions { replies: vec!["Yes, push it".into()], picks: Default::default(), why: Default::default(), note: None, by: None, by_title: None, at, prompt_at: Some(read) };
        let holds = |s: &Session| (crate::tags::moved_on(s, &tag), suggestions_hold(s, &suggested));
        // Claude Code compacts it, going busy for a minute and writing nothing it says.
        let compacting = claude_session(&a, Some(&reg("busy", NOW - MIN)), Some(&ended), NOW - 55 * MIN, NOW, &Quiet::default()).unwrap();
        assert_eq!((compacting.status, compacting.status_since), (Status::Working, Some(NOW - MIN)));
        assert_eq!(holds(&compacting), (None, true));
        // Once it's done, it's where it was.
        let compacted = TranscriptInfo { compacted: Some(NOW - 60 * MIN), last_ts: NOW - 10_000, ..ended.clone() };
        assert_eq!(holds(&claude_session(&a, Some(&reg("idle", NOW - 9_000)), Some(&compacted), NOW - 55 * MIN, NOW, &Quiet::default()).unwrap()), (None, true));
        // A turn another session's message started has said it, and takes both away.
        let messaged = TranscriptInfo { ended_at: None, last_ts: NOW - MIN, ..ended.clone() };
        let woke = claude_session(&a, Some(&reg("busy", NOW - MIN)), Some(&messaged), NOW - 55 * MIN, NOW, &Quiet::default()).unwrap();
        assert_eq!(holds(&woke), (Some(MovedOn::Turn { at: NOW - MIN }), false));
        // So does a turn that ended since they were given, though nothing's been said after it.
        let again = TranscriptInfo { ended_at: Some(NOW - 5 * MIN), ..ended.clone() };
        assert_eq!(holds(&claude_session(&a, Some(&reg("busy", NOW - MIN)), Some(&again), NOW - 55 * MIN, NOW, &Quiet::default()).unwrap()), (Some(MovedOn::Turn { at: NOW - MIN }), false));
        // A turn they were given in, gone on after you allowed its call, has moved on too.
        let allowed = TranscriptInfo { ended_at: None, unfinished: Some(NOW - 52 * MIN), ..ended };
        assert_eq!(holds(&claude_session(&a, Some(&reg("busy", NOW - MIN)), Some(&allowed), NOW - 55 * MIN, NOW, &Quiet::default()).unwrap()).1, false);
    }

    #[test]
    fn replies_are_turned_down_for_a_session_that_moved_on_since_it_was_read() {
        const MIN: i64 = 60_000;
        let a = acct("claude2");
        let ended = claude_session(&a, Some(&reg("idle", NOW - 20 * MIN)), Some(&tr()), NOW - 30 * MIN, NOW, &Quiet::default()).unwrap();
        let ended = Session { last_prompt_at: NOW - 30 * MIN, ..ended };
        // As it was read, they're taken.
        assert_eq!(suggestion_refused(&ended, NOW - 30 * MIN), None);
        assert_eq!(suggestion_refused(&Session { status: Status::Needs, ..ended.clone() }, NOW - 30 * MIN), None);
        // You replied after it was read: the turn they'd answer is gone.
        let replied = Session { last_prompt_at: NOW - MIN, ..ended.clone() };
        assert!(suggestion_refused(&replied, NOW - 30 * MIN).unwrap().contains("Read it again"));
        // Taking a turn, a prompt or not, it's moving on from what was read; a turn that left a
        // command waiting in the background is over.
        let working = Session { status: Status::Working, status_since: Some(NOW - MIN), ..ended.clone() };
        assert!(suggestion_refused(&working, NOW - 30 * MIN).unwrap().contains("taking a turn"));
        assert_eq!(suggestion_refused(&Session { background: Some("npm run dev".into()), ..working }, NOW - 30 * MIN), None);
    }

    #[test]
    fn a_turn_its_agent_marked_seen_is_idle_until_it_needs_you() {
        const MIN: i64 = 60_000;
        let a = acct("claude2");
        let session = claude_session(&a, Some(&reg("idle", NOW - 20 * MIN)), Some(&tr()), NOW - 30 * MIN, NOW, &Quiet::default()).unwrap();
        let ended = Session { last_prompt_at: NOW - 25 * MIN, ..session };
        assert_eq!(ended.status, Status::Unread);
        let mark = AgentSeen { at: NOW - 21 * MIN, note: Some("Nothing new on the deploy".into()) };
        let marked = |s: &Session| {
            let mut s = s.clone();
            apply_agent_seen(&mut s, Some(&mark));
            s
        };
        // The turn it was marked in ended: idle, not its lane's bay, saying its agent saw it.
        let m = marked(&ended);
        assert_eq!((m.status, m.agent_seen.as_ref(), m.status_since), (Status::Idle, Some(&mark), ended.status_since));
        assert_eq!(bay_of(std::slice::from_ref(&m), None).0, Bay::Idle);
        // A prompt sent after the mark: the turn answering it is Your turn.
        let prompted = Session { last_prompt_at: NOW - 20 * MIN, ..ended.clone() };
        assert_eq!((marked(&prompted).status, marked(&prompted).agent_seen), (Status::Unread, None));
        // A reply that asks you something is Needs you by then, and stays so.
        let mut asks = ended.clone();
        apply_insight(&mut asks, &insight(|i| i.asks_you = Some(0.9)));
        assert_eq!(marked(&asks).status, Status::Needs);
        // A call to allow, an error, work under way and a turn you'd seen show as they are.
        for st in [Status::Needs, Status::Error, Status::Working, Status::Stopped, Status::Idle] {
            let s = marked(&Session { status: st, ..ended.clone() });
            assert_eq!((s.status, s.agent_seen), (st, None));
        }
        let mut untouched = ended.clone();
        apply_agent_seen(&mut untouched, None);
        assert_eq!(untouched.status, Status::Unread);
        // A turn its harness ended isn't the reply its agent spoke for.
        let lost = Session { connection_lost: Some(ConnectionLost { at: NOW - 19 * MIN, ..Default::default() }), ..ended.clone() };
        assert_eq!(marked(&lost).status, Status::Unread);
        let cut = Session { cut_off: Some(NOW - 19 * MIN), ..ended.clone() };
        assert_eq!(marked(&cut).status, Status::Unread);
        // Another session's message or a background task's report taken in after the mark, or a
        // later turn a scheduled wakeup started and ended while Codealong was closed: its record
        // says something came in after it, though no prompt did.
        let came = Session { input_at: Some(NOW - 20 * MIN), ..ended.clone() };
        assert!(agent_seen_spent(&came, mark.at));
        assert_eq!((marked(&came).status, marked(&came).agent_seen), (Status::Unread, None));
        let before = Session { input_at: Some(NOW - 25 * MIN), ..ended.clone() };
        assert_eq!(marked(&before).status, Status::Idle, "what started the turn it was marked in");
        // Where its harness says no more than its prompts, a turn started after the mark is seen
        // as it runs, and the engine forgets the mark then.
        let woke = Session { status: Status::Working, status_since: Some(NOW - 10 * MIN), ..ended.clone() };
        assert!(agent_seen_spent(&woke, mark.at));
        let taking = Session { status: Status::Working, status_since: Some(NOW - 24 * MIN), ..ended.clone() };
        assert!(!agent_seen_spent(&taking, mark.at), "the turn it was marked in");
        // Quiet already has it idle: nothing more to say.
        let mut quiet = ended.clone();
        apply_quiet(&mut quiet, true);
        apply_agent_seen(&mut quiet, Some(&mark));
        assert_eq!((quiet.status, quiet.agent_seen), (Status::Idle, None));
        // Your own mark of unread comes after and brings it back, no longer saying its agent saw it.
        let mut brought = marked(&ended);
        apply_unread(&mut brought, Some(&UnreadMark { key: ended.key.clone(), at: NOW - 5 * MIN }), NOW - 30 * MIN);
        settle_agent_seen(&mut brought);
        assert_eq!((brought.status, brought.agent_seen), (Status::Unread, None));
        let mut kept = marked(&ended);
        settle_agent_seen(&mut kept);
        assert_eq!(kept.agent_seen, Some(mark.clone()));
    }

    #[test]
    fn a_session_marked_unread_is_your_turn_until_seen_or_prompted() {
        const MIN: i64 = 60_000;
        let a = acct("claude2");
        let seen = claude_session(&a, Some(&reg("idle", NOW - 20 * MIN)), Some(&tr()), NOW - 30 * MIN, NOW, &Quiet::default()).unwrap();
        let idle = Session { status: Status::Idle, ..seen };
        let mark = UnreadMark { key: idle.key.clone(), at: NOW - 5 * MIN };
        let marked = |s: &Session, seen_at: i64| {
            let mut s = s.clone();
            apply_unread(&mut s, Some(&mark), seen_at);
            (s.status, s.status_since)
        };
        // Its turn over and seen before the mark: Your turn again, from the mark.
        assert_eq!(marked(&idle, NOW - 10 * MIN), (Status::Unread, Some(mark.at)));
        let stopped = Session { status: Status::Stopped, live: false, ..idle.clone() };
        assert_eq!(marked(&stopped, NOW - 10 * MIN).0, Status::Unread);
        // Seen since, or prompted since, it's as it was.
        assert_eq!(marked(&idle, NOW - MIN).0, Status::Idle);
        let prompted = Session { last_prompt_at: NOW - MIN, ..idle.clone() };
        assert_eq!(marked(&prompted, NOW - 10 * MIN).0, Status::Idle);
        // Running or waiting on you, it shows that; archived, it stays as it is.
        let working = Session { status: Status::Working, ..idle.clone() };
        assert_eq!(marked(&working, NOW - 10 * MIN).0, Status::Working);
        let needs = Session { status: Status::Needs, ..idle.clone() };
        assert_eq!(marked(&needs, NOW - 10 * MIN).0, Status::Needs);
        let archived = Session { archived: true, ..idle.clone() };
        assert_eq!(marked(&archived, NOW - 10 * MIN).0, Status::Idle);
        let mut untouched = idle.clone();
        apply_unread(&mut untouched, None, 0);
        assert_eq!(untouched.status, Status::Idle);
    }

    #[test]
    fn a_side_conversations_answer_is_your_turn_until_seen_or_prompted() {
        const MIN: i64 = 60_000;
        let a = acct("claude2");
        let seen = claude_session(&a, Some(&reg("idle", NOW - 20 * MIN)), Some(&tr()), NOW - 30 * MIN, NOW, &Quiet::default()).unwrap();
        let idle = Session { status: Status::Idle, last_prompt_at: NOW - 40 * MIN, ..seen };
        let side = |id: &str, open: bool, answered_at: Option<i64>| crate::side::Glance { id: id.into(), question: "Why a queue?".into(), open, answered_at, ..Default::default() };
        let answered = NOW - 5 * MIN;
        let applied = |s: &Session, sides: &[crate::side::Glance], seen_at: i64| {
            let mut s = s.clone();
            apply_side_answer(&mut s, sides, seen_at);
            s
        };
        let one = [side("a", true, Some(answered))];

        // Its turn over and seen before the answer: Your turn, from the answer, saying what for.
        let s = applied(&idle, &one, NOW - 10 * MIN);
        assert_eq!((s.status, s.status_since, s.excerpt.as_deref()), (Status::Unread, Some(answered), Some("Answered on the side · Why a queue?")));
        assert_eq!(s.side_answer, Some(SideAnswer { id: "a".into(), open: true, at: answered }));
        let stopped = Session { status: Status::Stopped, live: false, ..idle.clone() };
        assert_eq!(applied(&stopped, &one, NOW - 10 * MIN).status, Status::Unread);

        // Seen since, or prompted since, it's as it was, and nothing's left to show.
        let s = applied(&idle, &one, NOW - MIN);
        assert_eq!((s.status, s.side_answer), (Status::Idle, None));
        let prompted = Session { last_prompt_at: NOW - MIN, ..idle.clone() };
        assert_eq!(applied(&prompted, &one, NOW - 10 * MIN).status, Status::Idle);

        // Running or waiting on you, it shows that, and has the answer for when you come to it.
        for status in [Status::Working, Status::Needs, Status::Error] {
            let s = applied(&Session { status, excerpt: Some("Reading".into()), ..idle.clone() }, &one, NOW - 10 * MIN);
            assert_eq!((s.status, s.excerpt.as_deref(), s.side_answer.is_some()), (status, Some("Reading"), true));
        }
        // A turn of its own that ended stays what it waits for, since when it did.
        let turn = Session { status: Status::Unread, status_since: Some(NOW - 20 * MIN), excerpt: Some("Done.".into()), ..idle.clone() };
        let s = applied(&turn, &one, NOW - 30 * MIN);
        assert_eq!((s.status_since, s.excerpt.as_deref(), s.side_answer.is_some()), (Some(NOW - 20 * MIN), Some("Done."), true));

        // Archived, it stays as it is.
        let s = applied(&Session { archived: true, ..idle.clone() }, &one, NOW - 10 * MIN);
        assert_eq!((s.status, s.side_answer), (Status::Idle, None));
        // What's kept from before answers have times, or one still answering, waits on nothing.
        let s = applied(&idle, &[side("old", false, None)], NOW - 10 * MIN);
        assert_eq!((s.status, s.side_answer), (Status::Idle, None));
        // Of several, the latest shows.
        let s = applied(&idle, &[side("first", false, Some(answered - MIN)), side("last", false, Some(answered)), side("seen", false, Some(NOW - 20 * MIN))], NOW - 10 * MIN);
        assert_eq!(s.side_answer, Some(SideAnswer { id: "last".into(), open: false, at: answered }));
    }

    #[test]
    fn a_session_set_aside_waits_on_its_side_conversation_then_needs_you_again() {
        const MIN: i64 = 60_000;
        let a = acct("claude2");
        let base = claude_session(&a, Some(&reg("waiting", NOW - 20 * MIN)), Some(&tr()), NOW - 30 * MIN, NOW, &Quiet::default()).unwrap();
        let asked = NOW - 20 * MIN;
        let needs = Session { status: Status::Needs, need: Some(Need::Question), status_since: Some(asked), last_prompt_at: NOW - 40 * MIN, excerpt: Some("Restore test?".into()), ..base };
        let side = |running: bool, answered_at: Option<i64>| crate::side::Glance { id: "a".into(), question: "Are we restoring for real?".into(), running, open: true, answered_at, ..Default::default() };
        let set = NOW - 5 * MIN;
        let applied = |s: &Session, at: Option<i64>, sides: &[crate::side::Glance]| {
            let mut s = s.clone();
            apply_aside(&mut s, at, sides);
            s
        };

        // Set aside while its side conversation answers: still needing you underneath, but set
        // aside since then, saying what it asked on the side, and out of the lane's wait.
        let s = applied(&needs, Some(set), &[side(true, None)]);
        assert_eq!((s.status, s.aside, s.excerpt.as_deref()), (Status::Needs, Some(set), Some("Asking on the side · Are we restoring for real?")));
        assert!(s.held_back());
        assert_eq!(lane_waiting_since(std::slice::from_ref(&s)), None);
        assert!(raise_refused(&s, None, true).is_some());

        // Once it has answered, or nothing answers any more, or you closed it as it answered, it
        // needs you again as it did.
        let closed = crate::side::Glance { open: false, ..side(true, None) };
        for sides in [vec![side(false, Some(NOW - MIN))], vec![side(false, None)], vec![], vec![side(true, Some(NOW - MIN))], vec![closed]] {
            let s = applied(&needs, Some(set), &sides);
            assert_eq!((s.status, s.aside, s.excerpt.as_deref()), (Status::Needs, None, Some("Restore test?")), "{sides:?}");
        }
        // An answer from before it was set aside doesn't end it.
        assert_eq!(applied(&needs, Some(set), &[side(true, Some(set - MIN))]).aside, Some(set));

        // Only the need it was set aside from, and only until something's sent it.
        let later = Session { status_since: Some(set + MIN), ..needs.clone() };
        assert_eq!(applied(&later, Some(set), &[side(true, None)]).aside, None);
        let prompted = Session { last_prompt_at: set + MIN, ..needs.clone() };
        assert_eq!(applied(&prompted, Some(set), &[side(true, None)]).aside, None);
        // Not needing you, or archived, there's nothing to set aside; nor without a mark.
        for status in [Status::Working, Status::Unread, Status::Idle, Status::Error] {
            assert_eq!(applied(&Session { status, ..needs.clone() }, Some(set), &[side(true, None)]).aside, None, "{status:?}");
        }
        assert_eq!(applied(&Session { archived: true, ..needs.clone() }, Some(set), &[side(true, None)]).aside, None);
        assert_eq!(applied(&needs, None, &[side(true, None)]).aside, None);
    }

    #[test]
    fn a_quiet_command_that_isnt_waiting_for_typing_is_working() {
        let a = acct("claude2");
        let mut t = tr();
        t.last_ts = NOW - 3 * DAY_MS;
        let quiet = Quiet { subagent_at: None, running: Some("cargo build --release"), ..Default::default() };
        let mut s = claude_session(&a, Some(&reg("busy", NOW - 3 * DAY_MS)), Some(&t), 0, NOW, &quiet).unwrap();
        let mut login = s.clone();
        apply_insight(&mut login, &insight(|i| i.waits_for_input = Some(0.8)));
        assert_eq!(login.status, Status::Needs);
        apply_insight(&mut s, &insight(|i| i.waits_for_input = Some(0.1)));
        assert_eq!((s.status, s.stalled_since), (Status::Working, None));
        assert!(s.excerpt.as_deref().unwrap().starts_with("Running `cargo build --release` for {for:"));
    }

    #[test]
    fn a_background_command_polls_runs_or_is_stuck() {
        let a = acct("claude2");
        let quiet = Quiet { subagent_at: None, running: Some("while true; do sleep 60; gh pr view 1054; done"), ..Default::default() };
        let mut s = claude_session(&a, Some(&reg("shell", NOW - DAY_MS)), Some(&tr()), 0, NOW, &quiet).unwrap();
        s.reply_tail = Some("I'll keep checking. Waiting for the reviewer to approve #1054.".into());
        let mut poll = s.clone();
        apply_insight(&mut poll, &insight(|i| (i.background, i.background_p) = (Some("polling".into()), Some(0.9))));
        assert_eq!((poll.status, poll.excerpt.as_deref()), (Status::Working, Some("Polling in the background · Waiting for the reviewer to approve #1054.")));
        let mut job = s.clone();
        apply_insight(&mut job, &insight(|i| (i.background, i.background_p) = (Some("job".into()), Some(0.9))));
        assert!(job.excerpt.as_deref().unwrap().starts_with("Running in the background · `while true"));
        let mut maybe = s.clone();
        apply_insight(&mut maybe, &insight(|i| (i.background, i.background_p) = (Some("stalled".into()), Some(0.5))));
        assert_eq!(maybe.status, Status::Working);
        apply_insight(&mut s, &insight(|i| (i.background, i.background_p) = (Some("stalled".into()), Some(0.8))));
        assert_eq!(s.status, Status::Needs);
        assert!(s.stalled_since.is_some());
        assert_eq!(session_headline(&s).0, "`claude2` is stuck · Portal daily log + photo sync");
        // It needs you, but its turn is over: what you send it is a prompt, not an answer.
        assert!(!crate::send::waits_on_you(&s));
    }

    #[test]
    fn a_worktree_looks_finished_when_every_session_in_it_is() {
        let a = acct("claude2");
        let done = |f: f64| {
            let mut s = claude_session(&a, None, Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
            apply_insight(&mut s, &insight(|i| i.finished = Some(f)));
            s
        };
        assert!(looks_finished(Origin::Script, None, &[done(0.9)]));
        assert!(!looks_finished(Origin::Script, None, &[done(0.9), done(0.5)]));
        assert!(!looks_finished(Origin::Main, None, &[done(0.9)]));
        assert!(!looks_finished(Origin::Script, None, &[]));
        let mut visitor = done(0.1);
        visitor.visitor = Some("worked here".into());
        assert!(looks_finished(Origin::Script, None, &[done(0.9), visitor]));
        let mut working = done(0.9);
        working.status = Status::Working;
        assert!(!looks_finished(Origin::Script, None, &[working]));
    }

    /// A session whose turn ended and that you haven't seen, with this read of its reply.
    fn ended(read: impl FnOnce(&mut SessionInsight)) -> Session {
        Session { status: Status::Unread, insight: Some(insight(read)), ..crate::fixtures::session("claude:claude:a") }
    }

    #[test]
    fn a_session_is_finished_by_what_is_known_of_its_work() {
        let merged = Work { prs: vec![(12, PrState::Merged)], ..Default::default() };
        let said = |s: &Session, work: &Work| finished_of(s, work).map(|f| (f.sure, f.why.join(", ")));
        // A merged PR says so by itself, with no read of the reply at all, and with one that
        // agrees: what's known comes first.
        let unread = Session { insight: None, ..ended(|_| {}) };
        assert_eq!(said(&unread, &merged), Some((true, "#12 merged".into())));
        assert_eq!(said(&ended(|i| i.finished = Some(0.9)), &merged), Some((true, "#12 merged, its reply reads as done".into())));
        // So does a todo linked to it that's done, and its own status line.
        let all = Work { prs: vec![(12, PrState::Merged), (13, PrState::Merged), (9, PrState::Closed)], todos: vec![true], said: Some("landed"), ..Default::default() };
        assert_eq!(said(&unread, &all), Some((true, "#12 and #13 merged, its todo is done, it says its change landed".into())));
        let many = Work { prs: (1..=3).map(|n| (n, PrState::Merged)).collect(), todos: vec![true, true], ..Default::default() };
        assert_eq!(said(&unread, &many), Some((true, "3 PRs merged, its 2 todos are done".into())));
        // A status line an agent's tag reads otherwise isn't the last word on the change.
        let line = Work { said: Some("landed"), ..Default::default() };
        assert_eq!(said(&ended(|i| i.landing = Some("landed".into())), &line), Some((true, "it says its change landed".into())));
        assert_eq!(said(&ended(|i| i.landing = Some("pushed".into())), &line), None);
        // The line is the session's own word with nothing reading sessions (`[jev] sessions` off,
        // no tag): one that says the change didn't land holds it, whatever merged; one that only
        // says it's pushed doesn't, its PR having merged since.
        assert_eq!(said(&unread, &line), Some((true, "it says its change landed".into())));
        for held in ["uncommitted", "committed"] {
            assert_eq!(said(&unread, &Work { said: Some(held), ..merged.clone() }), None, "{held}");
        }
        assert_eq!(said(&unread, &Work { said: Some("pushed"), ..merged.clone() }), Some((true, "#12 merged".into())));
        // Seen or stopped, it's as finished as unread: the lists of what to clear go by it too.
        for status in [Status::Idle, Status::Stopped] {
            assert!(finished_of(&Session { status, ..unread.clone() }, &merged).is_some(), "{status:?}");
        }
    }

    #[test]
    fn a_session_only_looks_finished_by_the_read_of_its_reply() {
        let nothing = Work::default();
        let said = |s: &Session| finished_of(s, &nothing).map(|f| (f.sure, f.why.join(", ")));
        assert_eq!(said(&ended(|i| i.finished = Some(FINISHED))), Some((false, "its reply reads as done".into())));
        let tagged = ended(|i| (i.finished, i.by, i.by_title) = (Some(1.0), Some("claude:claude:boss".into()), Some("Orchestrator".into())));
        assert_eq!(said(&tagged), Some((false, "Orchestrator read it as done".into())));
        // Nothing known of it either way is no reason to call it finished.
        assert_eq!(said(&ended(|i| i.finished = Some(0.7))), None);
        assert_eq!(said(&ended(|_| {})), None);
        assert_eq!(said(&Session { insight: None, ..ended(|_| {}) }), None);
    }

    #[test]
    fn a_session_with_anything_left_is_not_finished() {
        const MIN: i64 = 60_000;
        let merged = Work { prs: vec![(12, PrState::Merged)], todos: vec![true], said: Some("landed"), ..Default::default() };
        let done = || ended(|i| i.finished = Some(1.0));
        assert!(finished_of(&done(), &merged).is_some());
        let left = |what: &str, s: Session| assert_eq!(finished_of(&s, &merged), None, "{what}");
        // What its reply says: it may ask you something, hands you a step, waits on what it
        // started, didn't land its change, or isn't done.
        left("may ask", ended(|i| (i.finished, i.asks_you) = (Some(1.0), Some(MAY_ASK))));
        left("a question in a reply nothing read", Session { insight: None, reply_tail: Some("Merged. Shall I deploy it?".into()), ..done() });
        for ending in ["handoff", "waiting", "question", "blocked"] {
            left(ending, ended(|i| (i.finished, i.ending) = (Some(1.0), Some(ending.into()))));
        }
        for ending in ["report", "offer"] {
            assert!(finished_of(&ended(|i| (i.finished, i.ending) = (Some(1.0), Some(ending.into()))), &merged).is_some(), "{ending}");
        }
        for landing in ["blocked", "uncommitted", "committed"] {
            left(landing, ended(|i| (i.finished, i.landing) = (Some(1.0), Some(landing.into()))));
        }
        left("reads as not done", ended(|i| i.finished = Some(0.4)));
        left("its work goes on at a later time", ended(|i| (i.finished, i.waits_until) = (Some(1.0), Some(NOW + DAY_MS))));
        left("work it put off is due", ended(|i| (i.finished, i.waits_until) = (Some(1.0), Some(NOW - DAY_MS))));
        // What it is: working, waiting on you, archived, an agent's helper, a script's run.
        left("working", Session { status: Status::Working, ..done() });
        left("needs you", Session { status: Status::Needs, ..done() });
        left("asks in its reply", Session { status: Status::Needs, asks: true, ..done() });
        left("failed", Session { status: Status::Error, ..done() });
        left("archived", Session { archived: true, ..done() });
        left("a helper", Session { helper: Some(Helper::default()), ..done() });
        left("headless", Session { headless: true, ..done() });
        assert!(finished_of(&Session { helper: Some(Helper { yours: true, ..Default::default() }), ..done() }, &merged).is_some(), "a helper you talk to is yours");
        // What's still to come, and a turn its harness cut short.
        left("a command left running", Session { background: Some("npm run dev".into()), ..done() });
        left("a message waiting to go in", Session { queued_at: Some(NOW), ..done() });
        left("snoozed", Session { snoozed_until: Some(NOW + MIN), ..done() });
        left("stuck", Session { stalled_since: Some(NOW), ..done() });
        left("hit its limit", Session { limit: Some(SessionLimit::default()), ..done() });
        left("lost its connection", Session { connection_lost: Some(ConnectionLost::default()), ..done() });
        left("cut off", Session { cut_off: Some(NOW), ..done() });
        // What's left open of its work.
        let open = |work: Work| assert_eq!(finished_of(&done(), &work), None, "{work:?}");
        open(Work { prs: vec![(12, PrState::Merged), (13, PrState::Approved)], ..Default::default() });
        open(Work { prs: vec![(13, PrState::Draft)], ..Default::default() });
        open(Work { todos: vec![true, false], ..merged.clone() });
        open(Work { unsaved: true, ..merged.clone() });
    }

    #[test]
    fn the_board_says_which_sessions_are_finished_and_lists_them_after_the_rest() {
        const MIN: i64 = 60_000;
        let at = |id: &str, prompted: i64| Session { status: Status::Unread, live: false, last_prompt_at: NOW - prompted * MIN, last_activity: NOW - prompted * MIN, ..crate::fixtures::session(&Session::key_for(Harness::Claude, "claude", id)) };
        let read = |s: Session, f: f64| Session { insight: Some(insight(|i| i.finished = Some(f))), ..s };
        let (done, open, tied, kept) = (read(at("done", 1), 0.9), at("open", 5), read(at("tied", 2), 0.9), read(at("kept", 3), 0.9));
        let pr = |n: u64, state: PrState, of: &Session| PrItem {
            sessions: vec![PrSession { key: of.key.clone(), lane: "/r/w".into(), pushes: 0, primary: None }],
            ..pr_item(&[], &HashMap::new(), "o/r", ("P", "r", None), &PrRaw { number: n, ..pr(state, 0) }, None, None, &PrLinks::default())
        };
        let todo = |status: crate::todos::TodoStatus, archived: bool, of: &Session| -> crate::todos::Todo {
            serde_json::from_value(serde_json::json!({ "id": "t", "status": status, "sessions": [of.key], "archivedAt": archived.then_some(NOW) })).expect("a todo")
        };
        let own = |sessions: Vec<Session>, git: GitInfo| Lane { origin: Origin::Script, bay: Bay::Unread, git: Some(git), sessions, ..crate::fixtures::lane("/r/w", vec![]) };
        let mut lanes = vec![own(vec![done.clone(), tied.clone(), kept.clone(), open.clone()], GitInfo::default())];
        let prs = [pr(7, PrState::Open, &tied)];
        let todos = [todo(crate::todos::TodoStatus::InProgress, false, &kept), todo(crate::todos::TodoStatus::InProgress, true, &done)];
        settle_finished(&mut lanes, &prs, &todos);
        // By the name each was made with: a fixture's id is its whole key.
        let name = |s: &Session| s.key.rsplit(':').next().unwrap_or_default().to_string();
        let said: Vec<(String, Option<bool>)> = lanes[0].sessions.iter().map(|s| (name(s), s.finished.as_ref().map(|f| f.sure))).collect();
        let said: Vec<(&str, Option<bool>)> = said.iter().map(|(n, f)| (n.as_str(), *f)).collect();
        // The turns that still want something keep their order, the latest prompted first; the
        // finished one, prompted last of all, comes after them. An archived todo holds nothing.
        assert_eq!(said, [("tied", None), ("kept", None), ("open", None), ("done", Some(false))]);
        assert!(lanes[0].headline.contains("Title claude:claude:tied"), "the worktree's sentence is its first session's: {}", lanes[0].headline);
        // A worktree that's one session's alone holds its work: what's uncommitted there, or on no
        // remote, would go with it. Beside another session, or in a main checkout, whose it is
        // isn't known.
        let dirty = GitInfo { dirty: 2, ..Default::default() };
        let unpushed = GitInfo { unpushed: Some(1), ..Default::default() };
        for git in [dirty.clone(), unpushed] {
            let mut alone = vec![own(vec![done.clone()], git)];
            settle_finished(&mut alone, &[], &[]);
            assert_eq!(alone[0].sessions[0].finished, None);
        }
        let mut shared = vec![own(vec![done.clone(), open.clone()], dirty.clone())];
        settle_finished(&mut shared, &[], &[]);
        assert!(shared[0].sessions.iter().any(|s| name(s) == "done" && s.finished.is_some()));
        let mut main = vec![Lane { origin: Origin::Main, ..own(vec![done.clone()], dirty) }];
        settle_finished(&mut main, &[], &[]);
        assert!(main[0].sessions[0].finished.is_some());
        // Where it only visits, it's as finished as where it's listed; and what it was said to be
        // goes once it no longer is.
        let visitor = Session { visitor: Some("worked here".into()), finished: None, ..done.clone() };
        let stale = Session { finished: Some(Finished::default()), ..open.clone() };
        let mut both = vec![own(vec![done.clone(), stale], GitInfo::default()), Lane { id: "/r/v".into(), bay: Bay::Idle, ..own(vec![visitor], GitInfo::default()) }];
        settle_finished(&mut both, &[pr(8, PrState::Merged, &done)], &[]);
        assert_eq!(both[1].sessions[0].finished.as_ref().map(|f| f.why.join(", ")).as_deref(), Some("#8 merged, its reply reads as done"));
        assert_eq!(both[0].sessions.iter().map(|s| (name(s), s.finished.is_some())).collect::<Vec<_>>(), [("open".to_string(), false), ("done".to_string(), true)]);
        // Its status line is read with nothing reading sessions (`[jev] sessions` off, no tag):
        // one that says the change isn't committed holds a session whose PR merged, in a main
        // checkout too, where git says nothing of whose the changes are; one that says it landed
        // is why it's finished.
        let says = |line: &str| Session { insight: None, reply_head: Some(line.into()), ..done.clone() };
        let mut held = vec![Lane { origin: Origin::Main, ..own(vec![says("Not committed: edits in /r/w; the tests fail.")], GitInfo::default()) }];
        settle_finished(&mut held, &[pr(9, PrState::Merged, &done)], &[]);
        assert_eq!(held[0].sessions[0].finished, None);
        let mut landed = vec![own(vec![says("Landed: 1a2b3c4 is in origin/main.")], GitInfo::default())];
        settle_finished(&mut landed, &[], &[]);
        assert_eq!(landed[0].sessions[0].finished.as_ref().map(|f| (f.sure, f.why.join(", "))), Some((true, "it says its change landed".to_string())));
    }

    #[test]
    fn a_command_left_running_in_the_background_is_never_stuck() {
        let a = acct("claude2");
        let mut t = tr();
        t.last_ts = NOW - 3 * DAY_MS;
        let r = reg("shell", NOW - 3 * DAY_MS);
        let poll = "while true; do sleep 60; gh pr view 1054 --json reviewDecision; done";
        let s = claude_session(&a, Some(&r), Some(&t), 0, NOW, &Quiet { subagent_at: None, running: Some(poll), ..Default::default() }).unwrap();
        assert_eq!((s.status, s.stalled_since), (Status::Working, None));
        assert!(s.excerpt.as_deref().unwrap().starts_with("Waiting on `while true; do sleep 60;"), "{:?}", s.excerpt);
        assert_eq!(s.background.as_deref(), Some(poll));
        // The windows draw it waiting, by the command it was left running.
        assert_eq!(serde_json::to_value(&s).unwrap()["background"], poll);
        let unknown = claude_session(&a, Some(&r), Some(&t), 0, NOW, &Quiet::default()).unwrap();
        assert_eq!((unknown.status, unknown.excerpt.as_deref()), (Status::Working, Some("Waiting on a command in the background")));
        assert_eq!(serde_json::to_value(&unknown).unwrap()["background"], "", "a command it couldn't read still waits");
        assert_eq!(claude_busy_since(Some(&r), Some(&t)), None);
    }

    #[test]
    fn a_background_command_that_sits_still_is_left_running() {
        let a = acct("claude2");
        let r = reg("shell", NOW - DAY_MS);
        let login = "spruce login --host https://app.spruce.example";
        let at = |active_at| Quiet { running: Some(login), background_active_at: Some(active_at), ..Quiet::default() };
        let busy = claude_session(&a, Some(&r), Some(&tr()), 0, NOW, &at(NOW - 60_000)).unwrap();
        assert_eq!(busy.status, Status::Working);
        let left = claude_session(&a, Some(&r), Some(&tr()), 0, NOW, &at(NOW - BACKGROUND_QUIET_MS)).unwrap();
        assert_eq!((left.status, left.excerpt.as_deref()), (Status::Unread, Some("Left running · `spruce login --host https://app.spruce.example`")));
        assert_eq!(left.background.as_deref(), Some(login));
        let seen = claude_session(&a, Some(&r), Some(&tr()), NOW, NOW, &at(NOW - BACKGROUND_QUIET_MS)).unwrap();
        assert_eq!(seen.status, Status::Idle);
        // Jev can still say it waits on you, and a poll's name doesn't wake it.
        let mut stuck = left.clone();
        apply_insight(&mut stuck, &insight(|i| (i.background, i.background_p) = (Some("stalled".into()), Some(0.9))));
        assert_eq!(stuck.status, Status::Needs);
        let mut poll = left.clone();
        apply_insight(&mut poll, &insight(|i| (i.background, i.background_p) = (Some("polling".into()), Some(0.9))));
        assert_eq!((poll.status, poll.excerpt), (Status::Unread, left.excerpt.clone()));
    }

    #[test]
    fn a_session_without_a_terminal_waits_on_the_command_it_left_running() {
        let a = acct("claude");
        // Run over ACP: its registry says idle with the turn's Monitor still running.
        let r = RegistryEntry { entrypoint: Some("sdk-ts".into()), ..reg("idle", NOW - 60_000) };
        let watch = "until [ -f /tmp/atlas/ready ]; do sleep 3; done";
        let shell = Shell { id: "b7k2m9p4q".into(), at: NOW - 61_000, command: watch.into(), monitor: true, ..Shell::default() };
        let t = TranscriptInfo { background: crate::claude::Background { shells: vec![shell], ..Default::default() }, ..tr() };
        let running = Quiet { running: Some(watch), ..Quiet::default() };
        let s = claude_session(&a, Some(&r), Some(&t), 0, NOW, &running).unwrap();
        assert_eq!((s.status, s.background.as_deref()), (Status::Working, Some(watch)));
        assert!(s.excerpt.as_deref().unwrap().starts_with("Waiting on `until [ -f"), "{:?}", s.excerpt);
        assert!(s.waits_in_background());
        assert_eq!(s.tasks.iter().map(|t| (t.id.as_str(), t.command.as_str(), t.monitor)).collect::<Vec<_>>(), [("b7k2m9p4q", watch, true)]);
        // Jev is asked what the command does, not whether the reply asks you something.
        assert_eq!(crate::insights::ask(&s, NOW).map(|(k, _)| k), Some(crate::insights::Kind::Background));
        // Its turn is over when no shell of its runs one, whatever the transcript last heard;
        // but processes listed before the shell started can't say so.
        let gone = claude_session(&a, Some(&r), Some(&t), 0, NOW, &Quiet::default()).unwrap();
        assert_eq!((gone.status, gone.background), (Status::Unread, None));
        assert!(listed_after_shells(Some(NOW - 61_000), &t) && listed_after_shells(Some(NOW), &t));
        assert!(!listed_after_shells(Some(NOW - 65_000), &t) && !listed_after_shells(None, &t));
        // And when what it started went with an earlier process.
        let restarted = RegistryEntry { started_at: Some(NOW - 30_000), ..r.clone() };
        assert_eq!(claude_session(&a, Some(&restarted), Some(&t), 0, NOW, &running).unwrap().status, Status::Unread);
        // A terminal says so itself, and its idle is idle.
        let terminal = reg("idle", NOW - 60_000);
        assert!(!untold_shell(&terminal, &t));
        assert_eq!(claude_session(&a, Some(&terminal), Some(&t), 0, NOW, &running).unwrap().status, Status::Unread);
    }

    #[test]
    fn lists_what_a_live_session_runs_in_the_background_and_what_just_ended() {
        use crate::claude::ShellEnd;
        const MIN: i64 = 60_000;
        let shell = |id: &str, at: i64, ended: Option<(i64, &str, i64)>| Shell { id: id.into(), at, command: format!("run {id}"), ended: ended.map(|(at, status, code)| ShellEnd { at, status: status.into(), exit_code: Some(code) }), ..Shell::default() };
        let r = reg("idle", NOW - 60 * MIN);
        let bg = crate::claude::Background {
            // Ended in the queue, not yet told, and still running.
            shells: vec![shell("b1", NOW - 50 * MIN, Some((NOW - MIN, "completed", 0))), shell("b2", NOW - 40 * MIN, None)],
            // Ended a while ago, just now, and before this process started.
            ended: vec![shell("b3", NOW - 45 * MIN, Some((NOW - 30 * MIN, "completed", 0))), shell("b4", NOW - 20 * MIN, Some((NOW - 3 * MIN, "failed", 1))), shell("b0", NOW - 90 * MIN, Some((NOW - 2 * MIN, "stopped", 0)))],
            ..Default::default()
        };
        let t = TranscriptInfo { background: bg, ..tr() };
        let tasks = claude_tasks(&r, &t, NOW);
        let listed: Vec<_> = tasks.iter().map(|t| (t.id.as_str(), t.ended_at, t.status.as_deref(), t.exit_code)).collect();
        assert_eq!(listed, [("b2", None, None, None), ("b4", Some(NOW - 3 * MIN), Some("failed"), Some(1)), ("b1", Some(NOW - MIN), Some("completed"), Some(0))]);
        assert_eq!(tasks[0].command, "run b2");
        // A session whose process is gone runs none.
        let a = acct("claude");
        assert!(claude_session(&a, None, Some(&t), 0, NOW, &Quiet::default()).unwrap().tasks.is_empty());
        assert_eq!(claude_session(&a, Some(&r), Some(&t), 0, NOW, &Quiet::default()).unwrap().tasks, tasks);
    }

    #[test]
    fn lists_the_commands_a_codex_thread_left_in_background_terminals() {
        use crate::codex::{LeftRunning, Terminals};
        const MIN: i64 = 60_000;
        let terminal = |id: &str, at: i64, ended: Option<i64>| LeftRunning { id: id.into(), at, command: format!("run {id}"), ended: ended.map(|e| (e, "failed".to_string(), Some(2))) };
        let th = ThreadInfo {
            id: "t1".into(),
            last_ts: NOW - MIN,
            terminals: Terminals { running: vec![terminal("19957", NOW - 20 * MIN, None)], ended: vec![terminal("1", NOW - 40 * MIN, Some(NOW - 30 * MIN)), terminal("2", NOW - 9 * MIN, Some(NOW - 5 * MIN))], ..Default::default() },
            ..ThreadInfo::default()
        };
        let listed = |held| codex_tasks(&th, held, NOW).into_iter().map(|t| (t.id, t.command, t.ended_at, t.exit_code)).collect::<Vec<_>>();
        let running = ("19957".to_string(), "run 19957".to_string(), None, None);
        let ended = ("2".to_string(), "run 2".to_string(), Some(NOW - 5 * MIN), Some(2));
        assert_eq!(listed(Some(true)), [running.clone(), ended.clone()]);
        assert_eq!(listed(None), [running, ended.clone()], "a look that failed takes the rollout's word");
        // A thread no Codex process holds runs nothing: its terminals went with what ran them.
        assert_eq!(listed(Some(false)), [ended]);
        assert_eq!(codex_session(&acct("codex"), &th, None, None, Some(true), false, 0, NOW, None).tasks.len(), 2);
    }

    #[test]
    fn a_turn_that_ended_with_agents_running_waits_on_them() {
        use crate::claude::BackgroundAgent;
        const MIN: i64 = 60_000;
        let a = acct("claude");
        let agent = |id: &str, at: i64, about: &str| BackgroundAgent { id: id.into(), at, about: about.into() };
        // Busy since its prompt 40 minutes ago, as Claude Code says while its agents run; its turn
        // ended five minutes ago on a question.
        let r = reg("busy", NOW - 40 * MIN);
        let ended = NOW - 5 * MIN;
        let t = TranscriptInfo {
            ended_at: Some(ended),
            last_ts: ended,
            last_prompt_at: Some(NOW - 40 * MIN),
            last_text_tail: Some("Shall I file it with these dates?".into()),
            background: crate::claude::Background { agents: vec![agent("a1", NOW - 35 * MIN, "Draft the drift corrections"), agent("a2", NOW - 35 * MIN, "")], ..Default::default() },
            ..tr()
        };
        let quiet = Quiet { subagent_at: Some(NOW - 31 * MIN), ..Quiet::default() };
        let s = claude_session(&a, Some(&r), Some(&t), 0, NOW, &quiet).unwrap();
        assert_eq!((s.status, s.excerpt.as_deref(), s.stalled_since), (Status::Working, Some("Waiting on 2 background agents"), None), "not stuck past 30 minutes");
        assert_eq!((s.agents.len(), s.status_since, s.background.as_deref()), (2, Some(ended), None));
        assert!(s.waits_in_background() && s.left_running());
        assert!(!crate::harness::turn_running(&s), "there's no turn to stop");
        assert_eq!(serde_json::to_value(&s).unwrap()["agents"], serde_json::json!(["Draft the drift corrections", ""]));
        // One says what it's for.
        let one = TranscriptInfo { background: crate::claude::Background { agents: t.background.agents[..1].to_vec(), ..Default::default() }, ..t.clone() };
        assert_eq!(claude_session(&a, Some(&r), Some(&one), 0, NOW, &quiet).unwrap().excerpt.as_deref(), Some("Waiting on a background agent · Draft the drift corrections"));
        // Jev reads its reply as a turn's: asking you, it Needs you until you've seen it.
        assert_eq!(crate::insights::ask(&s, NOW).map(|(k, _)| k), Some(crate::insights::Kind::Turn));
        let asks = insight(|i| i.asks_you = Some(0.9));
        let mut asked = s.clone();
        apply_insight(&mut asked, &asks);
        asks_while_agents_run(&mut asked, &asks, NOW - 10 * MIN);
        assert_eq!((asked.status, asked.asks, asked.excerpt.as_deref()), (Status::Needs, true, Some("Asks you · Shall I file it with these dates?")));
        let mut seen = s.clone();
        asks_while_agents_run(&mut seen, &asks, NOW);
        assert_eq!(seen.status, Status::Working);
        let mut waits = s.clone();
        asks_while_agents_run(&mut waits, &insight(|i| i.asks_you = Some(0.05)), 0);
        assert_eq!((waits.status, waits.excerpt), (Status::Working, s.excerpt.clone()));
        // Mid-turn, busy is its turn's, whatever agents run beside it.
        let turn = TranscriptInfo { ended_at: None, ..t.clone() };
        let working = claude_session(&a, Some(&r), Some(&turn), 0, NOW, &Quiet { subagent_at: Some(NOW), ..Quiet::default() }).unwrap();
        assert_eq!((working.status, working.agents.len(), working.excerpt.as_deref()), (Status::Working, 0, Some("running `yarn test`")));
        assert!(crate::harness::turn_running(&working));
        // Agents an earlier process started went with it, and idle is idle.
        let restarted = RegistryEntry { started_at: Some(NOW - 30 * MIN), ..r.clone() };
        assert!(agents_left_running(&restarted, &t).is_empty());
        assert!(agents_left_running(&reg("idle", ended), &t).is_empty());
    }

    #[test]
    fn jev_takes_a_server_out_of_working() {
        let a = acct("claude2");
        let quiet = Quiet { subagent_at: None, running: Some("npm run dev"), ..Default::default() };
        let s = claude_session(&a, Some(&reg("shell", NOW - 60_000)), Some(&tr()), 0, NOW, &quiet).unwrap();
        assert_eq!(s.status, Status::Working);
        let mut server = s.clone();
        apply_insight(&mut server, &insight(|i| (i.background, i.background_p) = (Some("server".into()), Some(0.9))));
        assert_eq!((server.status, server.excerpt.as_deref()), (Status::Idle, Some("Left running · `npm run dev`")));
        let mut unsure = s.clone();
        apply_insight(&mut unsure, &insight(|i| (i.background, i.background_p) = (Some("server".into()), Some(0.5))));
        assert_eq!(unsure.status, Status::Working);
    }

    #[test]
    fn claude_excerpts_follow_status() {
        let a = acct("claude2");
        let needs = claude_session(&a, Some(&reg("waiting", NOW)), Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
        assert_eq!(needs.excerpt.as_deref(), Some("Bash · `yarn mikro-orm migration:up`"));
        let working = claude_session(&a, Some(&reg("busy", NOW)), Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
        assert_eq!(working.excerpt.as_deref(), Some("running `yarn test`"));
        // Waiting on the model's answer to a prompt it has made no call for yet: what it's on.
        let asked = TranscriptInfo { last_tool: None, last_prompt: Some("how is this going?".into()), ..tr() };
        let waiting = claude_session(&a, Some(&reg("busy", NOW)), Some(&asked), 0, NOW, &Quiet::default()).unwrap();
        assert_eq!(waiting.excerpt.as_deref(), Some("how is this going?"));
    }

    #[test]
    fn a_terminal_session_gone_partway_through_a_turn_was_cut_off() {
        let a = acct("claude2");
        let cut = TranscriptInfo { entrypoint: Some("cli".into()), unfinished: Some(NOW - 60_000), ..tr() };
        let gone = claude_session(&a, None, Some(&cut), 0, NOW, &Quiet::default()).unwrap();
        // News until seen, with where it was when it stopped.
        assert_eq!((gone.cut_off, gone.status, gone.live), (Some(NOW - 60_000), Status::Error, false));
        assert_eq!(gone.excerpt.as_deref(), Some("running `yarn test`"));
        assert_eq!(session_headline(&gone), ("`claude2` was cut off · Portal daily log + photo sync".to_string(), Some("running `yarn test`".to_string())));
        let seen = claude_session(&a, None, Some(&cut), NOW, NOW, &Quiet::default()).unwrap();
        assert_eq!((seen.cut_off, seen.status), (Some(NOW - 60_000), Status::Stopped));
        // Still running, the Claude app's or a headless run's, or moved to the background: not cut off.
        assert_eq!(claude_session(&a, Some(&reg("busy", NOW)), Some(&cut), 0, NOW, &Quiet::default()).unwrap().cut_off, None);
        for t in [
            TranscriptInfo { entrypoint: Some("claude-desktop".into()), ..cut.clone() },
            TranscriptInfo { entrypoint: Some("sdk-cli".into()), ..cut.clone() },
            TranscriptInfo { continued_in: Some("def".into()), ..cut.clone() },
        ] {
            assert_eq!(claude_session(&a, None, Some(&t), 0, NOW, &Quiet::default()).unwrap().cut_off, None, "{:?}", t.entrypoint);
        }
        // A turn that ended is just stopped.
        let ended = TranscriptInfo { unfinished: None, ..cut };
        let done = claude_session(&a, None, Some(&ended), 0, NOW, &Quiet::default()).unwrap();
        assert_eq!((done.cut_off, done.status), (None, Status::Unread));
    }

    #[test]
    fn maps_codex_statuses() {
        let a = AccountCfg { name: "codex".into(), harness: "codex".into(), home: "~/.codex".into(), command: None, start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false, chrome_profile: None };
        let mut th = ThreadInfo { id: "01a0c4a6".into(), cwd: "/r".into(), last_ts: NOW - 1_000, turn_open: true, turn_started_at: Some(NOW - 5_000), ..Default::default() };
        assert_eq!(codex_session(&a, &th, Some("Photo tools"), None, None, false, 0, NOW, None).status, Status::Working);
        assert_eq!(codex_session(&a, &th, None, None, None, false, 0, NOW, None).open_target, "s|codex:codex:01a0c4a6|codex");
        assert!(!codex_session(&a, &th, None, None, None, false, 0, NOW, None).headless);
        assert!(codex_session(&a, &ThreadInfo { exec: true, ..th.clone() }, None, None, None, false, 0, NOW, None).headless, "a `codex exec` run");
        assert!(!codex_session(&a, &ThreadInfo { exec: true, ..th.clone() }, None, Some(42), None, false, 0, NOW, None).headless, "resumed in a terminal");
        th.last_ts = NOW - CODEX_STALE_MS - 1;
        assert_eq!(codex_session(&a, &th, None, None, None, false, 0, NOW, None).status, Status::Idle);
        th.turn_open = false;
        th.last_complete_at = Some(NOW - 1_000);
        assert_eq!(codex_session(&a, &th, None, None, None, false, NOW - 2_000, NOW, None).status, Status::Unread);
        assert_eq!(codex_session(&a, &th, None, None, None, false, NOW, NOW, None).status, Status::Idle);
        th.pending_approval = Some("Run · `yarn install`".into());
        let asks = codex_session(&a, &th, None, None, None, false, 0, NOW, None);
        // It waits from its request, not from the turn before it.
        assert_eq!((asks.status, asks.status_since), (Status::Needs, Some(th.last_ts)));
        // A computer-use call that hasn't answered in a minute is waiting for approval.
        th.pending_approval = None;
        th.turn_open = true;
        th.last_ts = NOW - 30_000;
        th.pending_calls = vec![crate::codex::PendingCall { call_id: "c".into(), at: NOW - 30_000, what: "Inspect the Dock".into(), limit_ms: 60_000, input: String::new() }];
        assert_eq!(codex_session(&a, &th, None, None, None, false, 0, NOW, None).status, Status::Working);
        th.pending_calls[0].at = NOW - 61_000;
        let waiting = codex_session(&a, &th, None, None, None, false, 0, NOW, None);
        assert_eq!((waiting.status, waiting.status_since), (Status::Needs, Some(NOW - 61_000)));
        assert_eq!(waiting.excerpt.as_deref(), Some("Waiting for approval? · Inspect the Dock"));
        // One Codex's hook handed over waits from the moment it asked, and says so for sure.
        let asked = codex_session(&a, &th, None, None, None, false, 0, NOW, Some(("`touch ~/.northwind`".into(), NOW - 2_000)));
        assert_eq!((asked.status, asked.status_since), (Status::Needs, Some(NOW - 2_000)));
        assert_eq!(asked.excerpt.as_deref(), Some("Waiting for approval · `touch ~/.northwind`"));
    }

    #[test]
    fn a_codex_turn_no_process_holds_was_cut_off() {
        let a = AccountCfg { name: "codex".into(), harness: "codex".into(), home: "~/.codex".into(), command: None, start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false, chrome_profile: None };
        let th = ThreadInfo {
            id: "01a1315d".into(),
            cwd: "/r".into(),
            last_ts: NOW - 30_000,
            turn_open: true,
            turn_started_at: Some(NOW - 60_000),
            cli: true,
            last_user_message: Some("Run the slow test".into()),
            last_agent_message: Some("Done earlier.".into()),
            pending_calls: vec![crate::codex::PendingCall { call_id: "c".into(), at: NOW - 30_000, what: "Run · `sleep 120`".into(), limit_ms: 1_000, input: String::new() }],
            ..Default::default()
        };
        let cut = codex_session(&a, &th, None, None, Some(false), false, 0, NOW, None);
        assert_eq!((cut.cut_off, cut.status, cut.live, cut.status_since), (Some(NOW - 30_000), Status::Error, false, Some(NOW - 30_000)));
        // What it was on, and no guess that it waits for an approval.
        assert_eq!(cut.excerpt.as_deref(), Some("Run the slow test"));
        // Seen since: it stays cut off, as news no longer.
        let seen = codex_session(&a, &th, None, None, Some(false), false, NOW, NOW, None);
        assert_eq!((seen.cut_off, seen.status), (Some(NOW - 30_000), Status::Idle));
        // Held open, in a terminal, or not known: it may still be going.
        for (terminal, held) in [(None, Some(true)), (Some(42), Some(false)), (None, None)] {
            assert_eq!(codex_session(&a, &th, None, terminal, held, false, 0, NOW, None).cut_off, None, "{terminal:?} {held:?}");
        }
        // A turn that ended isn't cut off, held or not.
        let ended = ThreadInfo { turn_open: false, pending_calls: Vec::new(), ..th };
        assert_eq!(codex_session(&a, &ended, None, None, Some(false), false, 0, NOW, None).cut_off, None);
    }

    #[test]
    fn a_codex_turn_the_app_stopped_as_it_quit_was_cut_off() {
        let a = AccountCfg { name: "codex".into(), harness: "codex".into(), home: "~/.codex".into(), command: None, start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false, chrome_profile: None };
        // Stopped as the Mac restarted, and written to by the app opening it again after.
        let th = ThreadInfo {
            id: "01a0f3c7".into(),
            cwd: "/r".into(),
            last_ts: NOW - 60_000,
            turn_started_at: Some(NOW - 600_000),
            last_complete_at: Some(NOW - 400_000),
            interrupted_at: Some(NOW - 400_000),
            last_user_message: Some("Export the kestrel report".into()),
            last_agent_message: Some("The export runs now.".into()),
            ..Default::default()
        };
        let cut = codex_session(&a, &th, None, None, Some(true), true, NOW - 500_000, NOW, None);
        assert_eq!((cut.cut_off, cut.status, cut.live, cut.status_since), (Some(NOW - 400_000), Status::Error, false, Some(NOW - 400_000)));
        assert_eq!(session_headline(&cut), ("`codex` was cut off · Export the kestrel report".to_string(), Some("Export the kestrel report".to_string())));
        // Seen since it was stopped, though the app wrote to it after: news no longer.
        assert_eq!(codex_session(&a, &th, None, None, Some(true), true, NOW - 200_000, NOW, None).status, Status::Idle);
        // Stopped by you, its turn just ended.
        let stopped = codex_session(&a, &th, None, None, Some(true), false, NOW - 500_000, NOW, None);
        assert_eq!((stopped.cut_off, stopped.status), (None, Status::Unread));
        // Another account's, which the app doesn't run, or one a terminal UI has open: stopped
        // there as the app happened to quit.
        let other = AccountCfg { name: "codex2".into(), home: "~/.codex2".into(), command: Some("CODEX_HOME=~/.codex2 codex".into()), ..a.clone() };
        assert_eq!(codex_session(&other, &th, None, None, Some(true), true, NOW - 500_000, NOW, None).status, Status::Unread);
        assert_eq!(codex_session(&a, &th, None, Some(42), Some(true), true, NOW - 500_000, NOW, None).cut_off, None);
        // A turn started since is the thread's news.
        let again = ThreadInfo { turn_open: true, interrupted_at: None, last_ts: NOW - 1_000, ..th };
        assert_eq!(codex_session(&a, &again, None, None, Some(true), true, 0, NOW, None).cut_off, None);
    }

    /// A thread Codex hasn't named goes by its first prompt, not its latest, which may be a message
    /// Codealong sent it; a name wins over both.
    #[test]
    fn an_unnamed_codex_thread_goes_by_its_first_prompt() {
        let a = AccountCfg { name: "codex".into(), harness: "codex".into(), home: "~/.codex".into(), command: None, start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false, chrome_profile: None };
        let th = ThreadInfo {
            id: "01a0f2c7".into(),
            cwd: "/r".into(),
            last_ts: NOW - 1_000,
            first_prompt: Some("Tidy the photo uploader".into()),
            last_user_message: Some("On your PR #7: ken approved it.".into()),
            ..Default::default()
        };
        let s = codex_session(&a, &th, None, None, None, false, 0, NOW, None);
        assert_eq!((s.title.as_str(), s.first_prompt.as_deref(), s.last_prompt.as_deref()), ("Tidy the photo uploader", Some("Tidy the photo uploader"), Some("On your PR #7: ken approved it.")));
        assert_eq!(codex_session(&a, &th, Some("Photo uploader"), None, None, false, 0, NOW, None).title, "Photo uploader");
        // Its first not known (too far from its start), its latest.
        let far = ThreadInfo { first_prompt: None, first_prompt_unread: true, ..th };
        assert_eq!(codex_session(&a, &far, None, None, None, false, 0, NOW, None).title, "On your PR #7: ken approved it.");
    }

    #[test]
    fn a_codex_cli_thread_opens_in_its_terminal() {
        let a = AccountCfg { name: "codex2".into(), harness: "codex".into(), home: "~/.codex2".into(), command: Some("CODEX_HOME=~/.codex2 codex".into()), start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false, chrome_profile: None };
        let th = ThreadInfo { id: "01a0e375".into(), cwd: "/r".into(), last_ts: NOW - 1_000, cli: true, ..Default::default() };
        let open = codex_session(&a, &th, None, Some(42), None, false, 0, NOW, None);
        assert_eq!((open.surface, open.live, open.pid, open.open_target.as_str()), (Surface::Cli, true, Some(42), "s|codex:codex2:01a0e375|focus"));
        let closed = codex_session(&a, &th, None, None, None, false, 0, NOW, None);
        assert_eq!((closed.surface, closed.live, closed.open_target.as_str()), (Surface::Cli, false, "s|codex:codex2:01a0e375|resume"));
        assert_eq!(closed.resume_command.as_deref(), Some("CODEX_HOME=~/.codex2 codex resume 01a0e375"));
        assert!(!closed.app_account, "the Codex app runs ~/.codex only");
        // An app thread resumed in a terminal shows there while it's open.
        let app = ThreadInfo { cli: false, ..th };
        assert_eq!(codex_session(&a, &app, None, Some(7), None, false, 0, NOW, None).surface, Surface::Cli);
        assert_eq!(codex_session(&a, &app, None, None, None, false, 0, NOW, None).surface, Surface::App);
    }

    #[test]
    fn maps_opencode_statuses() {
        let a = AccountCfg { name: "opencode".into(), harness: "opencode".into(), home: "~/.local/share/opencode".into(), command: None, start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false, chrome_profile: None };
        let mut row = SessionRow {
            id: "ses_abc1234567".into(),
            directory: "/r".into(),
            title: Some("Fix the build".into()),
            created_at: NOW - 60_000,
            updated_at: NOW - 1_000,
            last_prompt: Some("Fix the build".into()),
            last_prompt_at: Some(NOW - 50_000),
            last_tool: Some("bash · `npm test`".into()),
            last_text: Some("Done.".into()),
            last_ts: NOW - 1_000,
            ..Default::default()
        };
        // Without the service, a prompt after the last run's end is a run in progress.
        let s = opencode_session(&a, &row, None, None, 0, NOW);
        assert_eq!((s.status, s.live, s.excerpt.as_deref()), (Status::Working, true, Some("bash · `npm test`")));
        assert_eq!(s.open_target, "s|opencode:opencode:ses_abc1234567|resume");
        assert_eq!(s.resume_command.as_deref(), Some("opencode --session ses_abc1234567"));
        assert_eq!(s.short_id, "abc1234");
        row.last_ts = NOW - OPENCODE_STALE_MS - 1;
        assert_eq!(opencode_session(&a, &row, None, None, 0, NOW).status, Status::Idle);
        // The service knows better either way.
        row.last_ts = NOW - 1_000;
        let quiet = Live::default();
        assert_eq!(opencode_session(&a, &row, Some(&quiet), None, 0, NOW).status, Status::Idle);
        let mut busy = Live::default();
        busy.active.insert("ses_abc1234567".into());
        assert_eq!(opencode_session(&a, &row, Some(&busy), Some(42), 0, NOW).open_target, "s|opencode:opencode:ses_abc1234567|focus");
        busy.pending.insert("ses_abc1234567".into(), crate::opencode::Asked { id: "per_1".into(), ask: "Bash · `npm install`".into(), since: NOW - 3_000 });
        let s = opencode_session(&a, &row, Some(&busy), None, 0, NOW);
        assert_eq!((s.status, s.excerpt.as_deref(), s.status_since), (Status::Needs, Some("Bash · `npm install`"), Some(NOW - 3_000)));
        assert_eq!(session_headline(&s).0, format!("`opencode` asks for approval · {}", s.title));
        // A run that ended is news until seen here or viewed in OpenCode.
        row.idle_at = Some(NOW - 500);
        row.outcome = Some("succeeded".into());
        assert_eq!(opencode_session(&a, &row, Some(&quiet), None, NOW - 2_000, NOW).status, Status::Unread);
        assert_eq!(opencode_session(&a, &row, Some(&quiet), None, NOW, NOW).status, Status::Idle);
        row.viewed_at = Some(NOW - 100);
        assert_eq!(opencode_session(&a, &row, Some(&quiet), None, NOW - 2_000, NOW).status, Status::Idle);
        row.viewed_at = None;
        row.outcome = Some("failed".into());
        assert_eq!(opencode_session(&a, &row, Some(&quiet), None, NOW - 2_000, NOW).status, Status::Error);
        row.archived_at = Some(NOW);
        assert!(opencode_session(&a, &row, Some(&quiet), None, NOW - 2_000, NOW).archived);
    }

    #[test]
    fn maps_antigravity_statuses() {
        let a = AccountCfg { name: "antigravity".into(), harness: "antigravity".into(), home: "~/.gemini/antigravity".into(), command: None, start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false, chrome_profile: None };
        // The app's conversation, with a turn under way while the app's agent is up.
        let mut c = Conversation {
            id: "c1".into(),
            title: Some("Photo tools".into()),
            preview: Some("Tighten the photo tools".into()),
            cwd: Some("/r".into()),
            status: "cascade_run_status_running".into(),
            not_fully_idle: true,
            last_modified: NOW - 1_000,
            last_input: Some(NOW - 5_000),
            ..Default::default()
        };
        c.tail.last_tool = Some("Run command · `npm test`".into());
        c.tail.last_text = Some("The tests pass now.".into());
        let s = antigravity_session(&a, &c, None, true, 0, NOW);
        assert_eq!((s.status, s.live, s.surface, s.excerpt.as_deref()), (Status::Working, true, Surface::App, Some("Run command · `npm test`")));
        assert_eq!(s.open_target, "s|antigravity:antigravity:c1|antigravity");
        assert_eq!((s.cwd.as_str(), s.resume_command.as_deref()), ("/r", None));
        // With the app quit nothing runs it, whatever its summary was left saying, and it goes
        // quiet when nothing has been written for a while.
        assert_eq!(antigravity_session(&a, &c, None, false, NOW, NOW).status, Status::Idle);
        c.last_modified = NOW - ANTIGRAVITY_STALE_MS - 1;
        assert_eq!(antigravity_session(&a, &c, None, true, 0, NOW).status, Status::Idle);
        c.last_modified = NOW - 1_000;
        // A step whose permission prompt is up needs you, and says what waits.
        c.waiting = true;
        c.tail.open = vec![crate::antigravity::Call { label: "Run command · `rm -rf build`".into(), ..Default::default() }];
        let s = antigravity_session(&a, &c, None, true, 0, NOW);
        assert_eq!((s.status, s.excerpt.as_deref()), (Status::Needs, Some("Run command · `rm -rf build`")));
        assert_eq!(session_headline(&s).0, format!("`antigravity` asks for approval · {}", s.title));
        c.waiting = false;
        // Done: its reply is news until you've seen it, and nothing new since you wrote isn't
        // your turn.
        c.status = "cascade_run_status_idle".into();
        c.not_fully_idle = false;
        let s = antigravity_session(&a, &c, None, true, NOW - 2_000, NOW);
        assert_eq!((s.status, s.live, s.excerpt.as_deref()), (Status::Unread, false, Some("The tests pass now.")));
        assert_eq!(antigravity_session(&a, &c, None, true, NOW, NOW).status, Status::Idle);
        c.last_input = Some(NOW - 500);
        assert_eq!(antigravity_session(&a, &c, None, true, 0, NOW).status, Status::Idle);
        c.killed = true;
        assert_eq!(antigravity_session(&a, &c, None, true, 0, NOW).status, Status::Idle);
    }

    #[test]
    fn a_command_line_conversation_runs_while_its_process_has_it() {
        let a = AccountCfg { name: "antigravity".into(), harness: "antigravity".into(), home: "~/.gemini/antigravity".into(), command: None, start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false, chrome_profile: None };
        let c = Conversation { id: "c2".into(), cli: true, preview: Some("Fix the build".into()), status: "cascade_run_status_running".into(), not_fully_idle: true, last_modified: NOW - 2 * ANTIGRAVITY_STALE_MS, last_input: Some(NOW - 3 * ANTIGRAVITY_STALE_MS), ..Default::default() };
        // In its terminal UI: it works however long it's been quiet, and its tab is where it opens.
        let ui = Process { pid: 41, conversation: Some("c2".into()), terminal: true, logs: Vec::new() };
        let s = antigravity_session(&a, &c, Some(&ui), false, 0, NOW);
        assert_eq!((s.status, s.live, s.surface, s.pid), (Status::Working, true, Surface::Cli, Some(41)));
        assert_eq!((s.open_target.as_str(), s.resume_command.as_deref()), ("s|antigravity:antigravity:c2|focus", Some("agy --conversation c2")));
        // Another `agy` that starts marks its summary idle while the terminal UI goes on with the
        // turn: it still works, and a step that waits on you needs you. Once the turn is over
        // and no step runs, what it left is yours to read.
        let mut idled = Conversation { status: "cascade_run_status_idle".into(), not_fully_idle: false, idled_by_another: true, ..c.clone() };
        assert_eq!(antigravity_session(&a, &idled, Some(&ui), false, 0, NOW).status, Status::Working);
        idled = Conversation { idled_by_another: false, step_under_way: true, waiting: true, ..idled };
        assert_eq!(antigravity_session(&a, &idled, Some(&ui), false, 0, NOW).status, Status::Needs);
        idled.waiting = false;
        assert_eq!(antigravity_session(&a, &idled, Some(&ui), false, 0, NOW).status, Status::Working);
        idled.step_under_way = false;
        assert_eq!(antigravity_session(&a, &idled, Some(&ui), false, 0, NOW).status, Status::Unread);
        // Its terminal UI's log says it started a turn that no step has ended: it works while the
        // model thinks before its next step. A message typed meanwhile waits in the UI's queue.
        let mut told = idled.clone();
        told.said.prompted = Some(("c2".into(), NOW - 9_000));
        assert_eq!(antigravity_session(&a, &told, Some(&ui), false, 0, NOW).status, Status::Working);
        told.said.queued = vec![NOW - 6_000, NOW - 4_000];
        let s = antigravity_session(&a, &told, Some(&ui), false, 0, NOW);
        assert_eq!((s.status, s.queued_at), (Status::Working, Some(NOW - 4_000)));
        assert_eq!(antigravity_session(&a, &told, None, false, 0, NOW).queued_at, None);
        // A step ended the turn, or Escape stopped it: what it left is yours to read, even when the
        // summary is an older copy from before your prompt, by when its latest step began.
        told.said.queued.clear();
        told.last_input = Some(NOW - 9_000);
        let ended = Conversation { turn_ended: true, latest_step_at: Some(NOW - 3_000), ..told.clone() };
        let s = antigravity_session(&a, &ended, Some(&ui), false, 0, NOW);
        assert_eq!((s.status, s.status_since, s.last_activity), (Status::Unread, Some(NOW - 3_000), NOW - 3_000));
        assert_eq!(antigravity_session(&a, &Conversation { latest_step_at: None, ..ended.clone() }, Some(&ui), false, 0, NOW).status, Status::Idle);
        assert_eq!(antigravity_session(&a, &ended, Some(&ui), false, NOW - 2_000, NOW).status, Status::Idle);
        told.said.stopped = Some(NOW - 1_000);
        told.latest_step_at = Some(NOW - 3_000);
        assert_eq!(antigravity_session(&a, &told, Some(&ui), false, 0, NOW).status, Status::Unread);
        // A print run has no terminal to show.
        let print = Process { terminal: false, ..ui.clone() };
        let s = antigravity_session(&a, &c, Some(&print), false, 0, NOW);
        assert_eq!((s.status, s.live, s.pid, s.open_target.as_str()), (Status::Working, true, None, "s|antigravity:antigravity:c2|resume"));
        // A print run that goes on with an old conversation leaves its summary as it was until
        // it ends: its record says a turn is under way, and that it's over.
        let mut resumed = Conversation { status: "cascade_run_status_idle".into(), not_fully_idle: false, ..c.clone() };
        resumed.tail.turn_open = true;
        assert_eq!(antigravity_session(&a, &resumed, Some(&print), false, 0, NOW).status, Status::Working);
        resumed.tail.turn_open = false;
        assert_eq!(antigravity_session(&a, &resumed, Some(&print), false, NOW, NOW).status, Status::Idle);
        // Its process gone mid-turn (killed, a closed tab), its summary still says running: it
        // isn't, and what it left is yours to read.
        let s = antigravity_session(&a, &c, None, true, 0, NOW);
        assert_eq!((s.status, s.live), (Status::Unread, false));
        // An account with a command of its own resumes with it.
        let own = AccountCfg { command: Some("agy --sandbox".into()), ..a.clone() };
        assert_eq!(antigravity_session(&own, &c, None, false, 0, NOW).resume_command.as_deref(), Some("agy --sandbox --conversation c2"));
    }

    /// The four turns in three conversations that a dropped connection ended on 2026-10-14, the
    /// one the quota refused and the one a 404 ended: each says why where its last reply ("I'll
    /// proceed once they finish") read as if it had stopped by itself.
    #[test]
    fn an_antigravity_turn_that_failed_says_why() {
        use crate::antigravity::Failed;
        let a = AccountCfg { name: "antigravity".into(), harness: "antigravity".into(), home: "~/.gemini/antigravity".into(), command: None, start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false, chrome_profile: None };
        let ui = Process { pid: 41, conversation: Some("c3".into()), terminal: true, logs: Vec::new() };
        let mut c = Conversation { id: "c3".into(), cli: true, status: "cascade_run_status_idle".into(), last_modified: NOW - 1_000, last_input: Some(NOW - 60_000), ..Default::default() };
        c.tail.last_text = Some("Running the browser checks now; I'll proceed once they finish.".into());
        let failed = |on: FailedOn, said: &str| Some(Failed { on, at: NOW - 1_000, said: said.into() });

        // A lost connection: a finished turn, which the engine may carry on, saying what failed.
        c.failed = failed(FailedOn::Connection, "Its request to the API failed: broken pipe");
        let s = antigravity_session(&a, &c, Some(&ui), false, 0, NOW);
        assert_eq!((s.status, s.connection_lost, s.limit.is_some()), (Status::Unread, Some(ConnectionLost { at: NOW - 1_000, continues: false }), false));
        assert_eq!((s.live, s.excerpt.as_deref()), (true, Some("Its request to the API failed: broken pipe")));
        // The quota: stopped at its limit until it lifts, which waits on you.
        c.failed = failed(FailedOn::Quota { resets_at: Some(NOW + 8_000_000) }, "Individual quota reached.");
        let s = antigravity_session(&a, &c, Some(&ui), false, 0, NOW);
        assert_eq!((s.status, s.connection_lost.is_some()), (Status::Error, false));
        assert_eq!(s.limit, Some(SessionLimit { at: NOW - 1_000, resets_at: Some(NOW + 8_000_000), ..SessionLimit::default() }));
        assert_eq!(session_headline(&s).0, format!("`antigravity` hit its usage limit · {}", s.title));
        // Anything else is an error until it's seen, in its own words.
        c.failed = failed(FailedOn::Other, "NOT_FOUND (code 404): models/barium-b-agy is not found");
        let s = antigravity_session(&a, &c, Some(&ui), false, 0, NOW);
        assert_eq!((s.status, s.limit.is_none(), s.excerpt.as_deref()), (Status::Error, true, Some("NOT_FOUND (code 404): models/barium-b-agy is not found")));
        assert_eq!(antigravity_session(&a, &c, Some(&ui), false, NOW, NOW).status, Status::Idle);
        // Its terminal closed, and another `agy` put back an older copy of its row: the note says
        // when it failed, which you haven't seen.
        let restored = Conversation { last_modified: NOW - 30_000, ..c.clone() };
        let s = antigravity_session(&a, &restored, None, false, NOW - 20_000, NOW);
        assert_eq!((s.status, s.last_activity), (Status::Error, NOW - 1_000));
        // Sent something since, it works again, and the failure is behind it.
        c.status = "cascade_run_status_running".into();
        let s = antigravity_session(&a, &c, Some(&ui), false, 0, NOW);
        assert_eq!((s.status, s.limit, s.connection_lost), (Status::Working, None, None));
    }

    #[test]
    fn maps_cursor_statuses() {
        use crate::cursor::{Call, Chat};
        let a = AccountCfg { name: "cursor".into(), harness: "cursor".into(), home: "~/.cursor".into(), command: None, start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false, chrome_profile: None };
        // The app's chat, generating while the app is up.
        let mut c = Chat { id: "c1".into(), title: Some("Fix the build".into()), cwd: Some("/r".into()), status: "generating".into(), created_at: NOW - 9_000, last_updated: NOW - 1_000, model: Some("gpt-5.5".into()), branch: Some("fix".into()), messages: 4, ..Default::default() };
        c.tail.last_tool = Some("Run · `cargo test`".into());
        c.tail.last_text = Some("The tests pass now.".into());
        c.tail.last_prompt_at = Some(NOW - 5_000);
        let s = cursor_session(&a, &c, None, None, true, 0, NOW);
        assert_eq!((s.status, s.live, s.surface, s.excerpt.as_deref()), (Status::Working, true, Surface::App, Some("Run · `cargo test`")));
        assert_eq!((s.open_target.as_str(), s.resume_command.as_deref(), s.status_since), ("s|cursor:cursor:c1|cursor", None, Some(NOW - 5_000)));
        assert_eq!((s.model.as_deref(), s.branch.as_deref(), s.cwd.as_str()), (Some("gpt-5.5"), Some("fix"), "/r"));
        // With the app quit, or nothing written for a long while, nothing runs it, whatever the
        // app left saying.
        assert_eq!(cursor_session(&a, &c, None, None, false, 0, NOW).status, Status::Unread);
        c.last_updated = NOW - CURSOR_STALE_MS - 1;
        assert!(!cursor_session(&a, &c, None, None, true, 0, NOW).live);
        c.last_updated = NOW - 1_000;
        // A call waiting to be allowed needs you, and says what waits.
        c.blocking = true;
        c.tail.open = vec![Call { label: "Run · `rm -rf target`".into(), open: true, waits: true, ..Default::default() }];
        let s = cursor_session(&a, &c, None, None, true, 0, NOW);
        assert_eq!((s.status, s.excerpt.as_deref()), (Status::Needs, Some("Run · `rm -rf target`")));
        assert_eq!(session_headline(&s).0, format!("`cursor` asks for approval · {}", s.title));
        // Waiting is only while it runs: an aborted chat often keeps the app's flag.
        c.status = "aborted".into();
        assert_eq!(cursor_session(&a, &c, None, None, true, NOW, NOW).status, Status::Idle);
        c.blocking = false;
        c.tail.open.clear();
        // Done: its reply is news until you've seen it; an error says so until then.
        c.status = "completed".into();
        let s = cursor_session(&a, &c, None, None, true, NOW - 2_000, NOW);
        assert_eq!((s.status, s.live, s.excerpt.as_deref()), (Status::Unread, false, Some("The tests pass now.")));
        assert_eq!(cursor_session(&a, &c, None, None, true, NOW, NOW).status, Status::Idle);
        c.tail.error = Some("Connection failed".into());
        let s = cursor_session(&a, &c, None, None, true, NOW - 2_000, NOW);
        assert_eq!((s.status, s.excerpt.as_deref()), (Status::Error, Some("Connection failed")));
        c.archived = true;
        assert!(cursor_session(&a, &c, None, None, true, 0, NOW).archived);
    }

    #[test]
    fn a_cursor_command_line_chat_runs_while_its_process_has_it() {
        use crate::cursor::{Chat, Process};
        let a = AccountCfg { name: "cursor".into(), harness: "cursor".into(), home: "~/.cursor".into(), command: None, start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false, chrome_profile: None };
        let mut c = Chat { id: "c2".into(), cli: true, title: Some("Fix the build".into()), created_at: NOW - 9_000, last_updated: NOW - 2_000, ..Default::default() };
        c.tail.turn_open = true;
        let ui = Process { pid: 41, chat: Some("c2".into()), terminal: true, prompt: None, started: None };
        let s = cursor_session(&a, &c, Some(&ui), None, false, 0, NOW);
        assert_eq!((s.status, s.live, s.surface, s.pid), (Status::Working, true, Surface::Cli, Some(41)));
        assert_eq!((s.open_target.as_str(), s.resume_command.as_deref()), ("s|cursor:cursor:c2|focus", Some("cursor-agent --resume c2")));
        // Its terminal shows a permission prompt, which nothing else says: it waits on you, as an
        // approval and not a question of its reply's.
        let asked = crate::cursor::Asked { question: "Allow this web search?".into(), what: "🔍 Web Search: northwind traders".into(), why: None, always: None, tells: false, waiting: 1 };
        let s = cursor_session(&a, &c, Some(&ui), Some(&asked), false, 0, NOW);
        assert_eq!((s.status, s.asks, s.excerpt.as_deref()), (Status::Needs, false, Some("Web search · northwind traders")));
        assert_eq!(session_headline(&s).0, "`cursor` asks for approval · Fix the build");
        // At its prompt, its turn over: still running, and yours to read.
        c.tail.turn_open = false;
        let s = cursor_session(&a, &c, Some(&ui), None, false, 0, NOW);
        assert_eq!((s.status, s.live), (Status::Unread, true));
        // A print run has no terminal to show, and works until it ends.
        let print = Process { terminal: false, ..ui.clone() };
        let s = cursor_session(&a, &c, Some(&print), None, false, 0, NOW);
        assert_eq!((s.status, s.pid, s.open_target.as_str()), (Status::Working, None, "s|cursor:cursor:c2|resume"));
        // Gone: what it left is yours to read, and a new terminal resumes it.
        let s = cursor_session(&a, &c, None, None, true, 0, NOW);
        assert_eq!((s.status, s.live, s.open_target.as_str()), (Status::Unread, false, "s|cursor:cursor:c2|resume"));
        // At rest since its transcript's last write: a mode switched later writes only its
        // database, which is activity but no new status.
        let switched = Chat { last_updated: NOW - 1_000, transcript_at: Some(NOW - 2_000), ..c.clone() };
        let s = cursor_session(&a, &switched, None, None, true, 0, NOW);
        assert_eq!((s.status_since, s.last_activity), (Some(NOW - 2_000), NOW - 1_000));
        // Started, with its chat not written yet: starting, then waiting on what it asks first in
        // its terminal, where it's opened, and with no chat to resume.
        let review = Chat { id: "c3".into(), cli: true, unwritten: true, created_at: NOW - 5_000, last_updated: NOW - 5_000, ..Default::default() };
        let ui = Process { pid: 42, chat: Some("c3".into()), terminal: true, prompt: Some("Review pull request #7".into()), started: None };
        let s = cursor_session(&a, &review, Some(&ui), None, false, 0, NOW);
        assert_eq!((s.status, s.live, s.pid, s.resume_command.as_deref()), (Status::Working, true, Some(42), None));
        let s = cursor_session(&a, &review, Some(&ui), None, false, 0, NOW + CURSOR_STARTING_MS);
        assert_eq!((s.status, s.excerpt.as_deref(), s.open_target.as_str()), (Status::Needs, Some("Cursor asks something in its terminal before it starts"), "s|cursor:cursor:c3|focus"));
        assert_eq!(session_headline(&s).0, "`cursor` asks for approval · Cursor chat");
        // A print run asks nothing: it works until it writes its chat.
        let s = cursor_session(&a, &review, Some(&Process { terminal: false, ..ui.clone() }), None, false, 0, NOW + CURSOR_STARTING_MS);
        assert_eq!(s.status, Status::Working);
        // Written, before its transcript has the prompt it was started on: at work on it. Started
        // with none, it waits at its box.
        let written = Chat { unwritten: false, ..review.clone() };
        assert_eq!(cursor_session(&a, &written, Some(&ui), None, false, 0, NOW).status, Status::Working);
        assert_eq!(cursor_session(&a, &written, Some(&Process { prompt: None, ..ui.clone() }), None, false, NOW, NOW).status, Status::Idle);
        // A transcript that never gets it doesn't keep it at work for good.
        assert_eq!(cursor_session(&a, &written, Some(&ui), None, false, NOW, NOW + CURSOR_STARTING_MS).status, Status::Idle);
    }

    /// A reviewer's terminal was closed with its turn under way, so its transcript never got the
    /// turn's end; resumed the next day, the chat sat at its box and read as working for as long
    /// as its tab stayed open (2026-11-10). The turn the transcript leaves open isn't the
    /// process's that resumed it: that one began after the transcript was last written.
    #[test]
    fn a_cursor_chat_resumed_after_its_turn_was_cut_off_waits_at_its_box() {
        use crate::cursor::{Chat, Process};
        let a = AccountCfg { name: "cursor".into(), harness: "cursor".into(), home: "~/.cursor".into(), command: None, start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false, chrome_profile: None };
        const HOUR: i64 = 3_600_000;
        // The transcript stopped an hour ago; resuming touched only the chat's database, a minute ago.
        let mut cut = Chat { id: "c4".into(), cli: true, title: Some("Review PR 7".into()), created_at: NOW - 2 * HOUR, last_updated: NOW - 60_000, transcript_at: Some(NOW - HOUR), ..Default::default() };
        cut.tail.turn_open = true;
        cut.tail.last_prompt_at = Some(NOW - HOUR - 60_000);
        let resumed = Process { pid: 43, chat: Some("c4".into()), terminal: true, prompt: None, started: Some(NOW - 61_000) };
        let s = cursor_session(&a, &cut, Some(&resumed), None, false, 0, NOW);
        assert_eq!((s.status, s.live, s.pid), (Status::Unread, true, Some(43)));
        assert_eq!(cursor_session(&a, &cut, Some(&resumed), None, false, NOW, NOW).status, Status::Idle, "seen");
        // A prompt its terminal showed before isn't its wait now.
        let asked = crate::cursor::Asked { question: "Allow this web search?".into(), what: "🔍 Web Search: northwind traders".into(), why: None, always: None, tells: false, waiting: 1 };
        assert_eq!(cursor_session(&a, &cut, Some(&resumed), Some(&asked), false, NOW, NOW).status, Status::Idle);
        // Sent a prompt, it writes its transcript within moments: at work on a turn of its own.
        let prompted = Chat { last_updated: NOW - 1_000, transcript_at: Some(NOW - 1_000), ..cut.clone() };
        assert_eq!(cursor_session(&a, &prompted, Some(&resumed), None, false, 0, NOW).status, Status::Working);
        // Resumed on a prompt (a review's next round), it's at work from the start, for as long
        // as a start takes without the transcript saying so.
        let on_prompt = Process { prompt: Some("It has new commits".into()), started: Some(NOW - 5_000), ..resumed.clone() };
        assert_eq!(cursor_session(&a, &cut, Some(&on_prompt), None, false, 0, NOW).status, Status::Working);
        assert_eq!(cursor_session(&a, &cut, Some(&on_prompt), None, false, NOW + CURSOR_STARTING_MS, NOW + CURSOR_STARTING_MS).status, Status::Idle);
        // One that has run since before its transcript's last write is taking that turn, and a
        // process whose start the kernel doesn't say is taken to be.
        let since = Process { started: Some(NOW - 2 * HOUR), ..resumed.clone() };
        assert_eq!(cursor_session(&a, &cut, Some(&since), None, false, 0, NOW).status, Status::Working);
        assert_eq!(cursor_session(&a, &cut, Some(&Process { started: None, ..resumed.clone() }), None, false, 0, NOW).status, Status::Working);
        // A print run works until it ends, whatever its transcript says.
        assert_eq!(cursor_session(&a, &cut, Some(&Process { terminal: false, ..resumed }), None, false, 0, NOW).status, Status::Working);
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
            let board = compose(Inputs { now: NOW, apps: &apps, seeds: &seeds, sessions, visitors: &visitors, git: &git, prs: &prs, requested: &HashMap::new(), made: &HashMap::new(), project_order: &order, notices: vec![], loading: false, github_at: None, pinned: &[], github_login: None, pr_links: &PrLinks::default(), previews: &Previews::default(), reusable: &HashMap::new(), review_snoozes: &[], session_settings: &Default::default(), auto_reviews: (&Default::default(), &Default::default(), &Default::default()), machines: &Default::default() });
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
        let board = |sessions: Vec<Session>| compose(Inputs { now: NOW, apps: &apps, seeds: &seeds, sessions, visitors: &visitors, git: &git, prs: &prs, requested: &HashMap::new(), made: &HashMap::new(), project_order: &order, notices: vec![], loading: false, github_at: None, pinned: &[], github_login: None, pr_links: &PrLinks::default(), previews: &Previews::default(), reusable: &HashMap::new(), review_snoozes: &[], session_settings: &Default::default(), auto_reviews: (&Default::default(), &Default::default(), &Default::default()), machines: &Default::default() });
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
    fn a_worktree_can_go_only_when_nothing_in_it_would_be_lost() {
        let clean = GitInfo { unpushed: Some(0), ..Default::default() };
        let w = Leaving { origin: Origin::ClaudeApp, chats: false, reusable: None, git: Some(&clean), running: 0, holds: None, lock: None };
        let said = |w: Leaving| cleanup(&w).map(|c| (c.ok, c.why));
        assert_eq!(said(w), Some((true, String::new())));
        // Kept whatever they hold.
        assert_eq!(said(Leaving { origin: Origin::Main, ..w }), Some((false, "the main checkout".into())));
        assert_eq!(said(Leaving { origin: Origin::Folder, ..w }), Some((false, "a folder, not a worktree".into())));
        assert_eq!(said(Leaving { chats: true, ..w }), Some((false, "a folder, not a worktree".into())));
        assert_eq!(said(Leaving { reusable: Some(true), ..w }), Some((false, "marked reusable".into())));
        assert_eq!(said(Leaving { reusable: Some(false), git: None, ..w }), Some((false, "reusable: it has taken several pieces of work".into())));
        // Unknown until its git is read.
        assert_eq!(said(Leaving { git: None, ..w }), None);
        // What would be lost, all of it said.
        let work = GitInfo { dirty: 3, unpushed: Some(2), ..Default::default() };
        assert_eq!(said(Leaving { git: Some(&work), ..w }), Some((false, "3 uncommitted files, 2 commits on no remote".into())));
        let one = GitInfo { dirty: 1, unpushed: Some(1), ..Default::default() };
        assert_eq!(said(Leaving { git: Some(&one), ..w }), Some((false, "1 uncommitted file, 1 commit on no remote".into())));
        let unborn = GitInfo { unpushed: None, ..Default::default() };
        assert_eq!(said(Leaving { git: Some(&unborn), ..w }), Some((false, "its commits couldn't be checked".into())));
        assert_eq!(said(Leaving { holds: Some("brave-otter"), ..w }), Some((false, "another worktree is inside it (brave-otter)".into())));
        // A running session; its own lock goes without saying.
        let lock = "claude session brave-otter (pid 46561 start Mon Oct 12 22:47:57 2026)";
        assert_eq!(said(Leaving { running: 1, lock: Some(lock), ..w }), Some((false, "a session is running in it".into())));
        assert_eq!(said(Leaving { running: 2, ..w }), Some((false, "2 sessions are running in it".into())));
        assert_eq!(said(Leaving { lock: Some(lock), ..w }), Some((false, "locked by a running session".into())));
        assert_eq!(said(Leaving { lock: Some("moving disks"), ..w }), Some((false, "locked: moving disks".into())));
        assert_eq!(said(Leaving { lock: Some(""), ..w }), Some((false, "locked".into())));
    }

    #[test]
    fn reads_claude_codes_lock_and_whether_it_holds() {
        assert_eq!(claude_lock("claude session brave-otter (pid 46561 start Mon Oct 12 22:47:57 2026)\n"), Some((46561, Some("Mon Oct 12 22:47:57 2026"))));
        assert_eq!(claude_lock("claude session my (odd) name (pid 7)"), Some((7, None)));
        assert_eq!(claude_lock("moving disks"), None);
        assert_eq!(claude_lock("claude session x (pid many)"), None);
        let live = HashSet::from([46561]);
        assert!(lock_holds("claude session a (pid 46561 start Mon Oct 12 22:47:57 2026)", &live));
        // Left behind by a session that's gone.
        assert!(!lock_holds("claude session a (pid 99 start Mon Oct 12 22:47:57 2026)", &live));
        // Anyone else's holds.
        assert!(lock_holds("moving disks", &live));
    }

    #[test]
    fn the_board_says_which_worktrees_can_go() {
        let apps = AppNames::default();
        let main = LaneSeed { origin: Origin::Main, callsign: "spark".into(), ..seed("/r/spark", "main") };
        let done = LaneSeed { origin: Origin::ClaudeApp, callsign: "done".into(), ..seed("/r/spark/.claude/worktrees/done", "worktree-done") };
        let busy = LaneSeed { origin: Origin::ClaudeApp, callsign: "busy".into(), ..seed("/r/spark/.claude/worktrees/busy", "worktree-busy") };
        let left = LaneSeed { origin: Origin::ClaudeApp, callsign: "left".into(), ..seed("/r/spark/.claude/worktrees/left", "worktree-left") };
        let outer = LaneSeed { callsign: "outer".into(), ..seed("/r/spark-outer", "outer") };
        let inner = LaneSeed { origin: Origin::ClaudeApp, callsign: "inner".into(), ..seed("/r/spark-outer/.claude/worktrees/inner", "worktree-inner") };
        let unread = LaneSeed { callsign: "unread".into(), ..seed("/r/spark-unread", "unread") };
        let seeds = vec![main, done, busy, left, outer, inner, unread];
        let clean = GitInfo { unpushed: Some(0), ..Default::default() };
        let lock = |pid: u32| GitInfo { locked: Some(format!("claude session x (pid {pid} start Mon Oct 12 22:47:57 2026)")), ..clean.clone() };
        let git: HashMap<PathBuf, GitInfo> = HashMap::from([
            (PathBuf::from("/r/spark"), clean.clone()),
            (PathBuf::from("/r/spark/.claude/worktrees/done"), clean.clone()),
            (PathBuf::from("/r/spark/.claude/worktrees/busy"), lock(4242)),
            // Its session was hung up on: the lock it left holds nothing.
            (PathBuf::from("/r/spark/.claude/worktrees/left"), lock(99)),
            (PathBuf::from("/r/spark-outer"), clean.clone()),
            (PathBuf::from("/r/spark-outer/.claude/worktrees/inner"), clean.clone()),
        ]);
        let mut running = claude_session(&acct("claude"), Some(&reg("idle", NOW)), Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
        (running.cwd, running.pid, running.live) = ("/r/spark/.claude/worktrees/busy".into(), Some(4242), true);
        // Archived, it still runs.
        running.archived = true;
        let (prs, visitors) = (HashMap::new(), HashMap::new());
        let order = vec!["Spruce".to_string()];
        let board = compose(Inputs { now: NOW, apps: &apps, seeds: &seeds, sessions: vec![running], visitors: &visitors, git: &git, prs: &prs, requested: &HashMap::new(), made: &HashMap::new(), project_order: &order, notices: vec![], loading: false, github_at: None, pinned: &[], github_login: None, pr_links: &PrLinks::default(), previews: &Previews::default(), reusable: &HashMap::new(), review_snoozes: &[], session_settings: &Default::default(), auto_reviews: (&Default::default(), &Default::default(), &Default::default()), machines: &Default::default() });
        let said = |p: &str| board.lanes.iter().find(|l| l.path == p).unwrap().cleanup.clone().map(|c| (c.ok, c.why));
        assert_eq!(said("/r/spark"), Some((false, "the main checkout".into())));
        assert_eq!(said("/r/spark/.claude/worktrees/done"), Some((true, String::new())));
        assert_eq!(said("/r/spark/.claude/worktrees/busy"), Some((false, "a session is running in it".into())));
        assert_eq!(said("/r/spark/.claude/worktrees/left"), Some((true, String::new())));
        assert_eq!(said("/r/spark-outer"), Some((false, "another worktree is inside it (inner)".into())));
        assert_eq!(said("/r/spark-outer/.claude/worktrees/inner"), Some((true, String::new())));
        assert_eq!(said("/r/spark-unread"), None);
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
        let board = compose(Inputs { now: NOW, apps: &apps, seeds: &seeds, sessions: vec![s], visitors: &visitors, git: &git, prs: &prs, requested: &HashMap::new(), made: &HashMap::new(), project_order: &order, notices: vec![], loading: false, github_at: None, pinned: &[], github_login: None, pr_links: &PrLinks::default(), previews: &Previews::default(), reusable: &HashMap::new(), review_snoozes: &[], session_settings: &Default::default(), auto_reviews: (&Default::default(), &Default::default(), &Default::default()), machines: &Default::default() });
        let lane = |p: &str| board.lanes.iter().find(|l| l.path == p).unwrap();
        assert_eq!(lane("/r/codealong-pins").bay, Bay::Working);
        assert_eq!(lane("/r/codealong-pins").sessions.len(), 1);
        assert_eq!(lane("/r/codealong").bay, Bay::Idle);
        assert!(lane("/r/codealong").sessions.is_empty());
    }

    #[test]
    fn a_helper_is_out_of_its_worktrees_status_until_it_needs_you() {
        const MIN: i64 = 60_000;
        let apps = AppNames::default();
        let seeds = vec![seed("/r/codealong", "main")];
        let (git, prs, visitors) = (HashMap::new(), HashMap::new(), HashMap::new());
        let order = vec!["Spruce".to_string()];
        let a = acct("claude");
        let session = |id: &str, status: &str, seen_at: i64| {
            let mut s = claude_session(&a, Some(&reg(status, NOW - 20 * MIN)), Some(&tr()), seen_at, NOW, &Quiet::default()).unwrap();
            (s.id, s.key, s.cwd) = (id.into(), Session::key_for(Harness::Claude, "claude", id), "/r/codealong".into());
            s
        };
        // You've seen what the session that started it last said.
        let boss = session("boss", "idle", NOW);
        assert_eq!(boss.status, Status::Idle);
        let helper = |status: &str| {
            let mut s = session("rev", status, 0);
            s.helper = helper_of(Some((&boss.key, false)), |k| k == boss.key, None);
            apply_helper(&mut s);
            s
        };
        let board = |h: Session| compose(Inputs { now: NOW, apps: &apps, seeds: &seeds, sessions: vec![h, boss.clone()], visitors: &visitors, git: &git, prs: &prs, requested: &HashMap::new(), made: &HashMap::new(), project_order: &order, notices: vec![], loading: false, github_at: None, pinned: &[], github_login: None, pr_links: &PrLinks::default(), previews: &Previews::default(), reusable: &HashMap::new(), review_snoozes: &[], session_settings: &Default::default(), auto_reviews: (&Default::default(), &Default::default(), &Default::default()), machines: &Default::default() });
        // Working, it's its agent's work: the worktree stands as its own sessions do, and the mark
        // doesn't spin for it. It's listed after them.
        let working = board(helper("busy"));
        let lane = &working.lanes[0];
        assert_eq!((lane.bay, lane.sessions.iter().map(|s| s.id.as_str()).collect::<Vec<_>>()), (Bay::Idle, vec!["boss", "rev"]));
        assert_eq!(working.logo_status, LogoStatus::Idle);
        // A turn it ended is its agent's to read, not Your turn.
        let ended = helper("idle");
        assert_eq!(ended.status, Status::Idle);
        assert_eq!(board(ended).lanes[0].bay, Bay::Idle);
        // Waiting on you to allow something, it's in Needs you like any session, saying so.
        let waiting = board(helper("waiting"));
        assert_eq!((waiting.lanes[0].bay, waiting.lanes[0].sessions[0].id.as_str()), (Bay::Needs, "rev"));
        assert_eq!(waiting.logo_status, LogoStatus::Needs);
    }

    #[test]
    fn a_helper_helps_a_session_still_on_the_board_or_a_pr() {
        let here = |k: &str| k == "claude:claude:boss";
        assert_eq!(helper_of(Some(("claude:claude:boss", true)), here, None), Some(Helper { parent: Some("claude:claude:boss".into()), pr: None, report: true, yours: false }));
        // The session that started it is gone: it's yours again.
        assert_eq!(helper_of(Some(("claude:claude:gone", true)), here, None), None);
        assert_eq!(helper_of(None, here, Some("o/r#7")), Some(Helper { parent: None, pr: Some("o/r#7".into()), report: false, yours: false }));
        assert_eq!(helper_of(None, here, None), None);
        // A helper's ended turn is idle; anyone else's, and its other statuses, are as they were.
        let ended = Session { status: Status::Unread, ..Session::new(Harness::Claude, "claude", "rev") };
        let mut theirs = ended.clone();
        apply_helper(&mut theirs);
        assert_eq!(theirs.status, Status::Unread);
        for (st, want) in [(Status::Unread, Status::Idle), (Status::Needs, Status::Needs), (Status::Error, Status::Error), (Status::Working, Status::Working)] {
            let mut h = Session { status: st, helper: helper_of(None, here, Some("o/r#7")), ..ended.clone() };
            apply_helper(&mut h);
            assert_eq!((h.status, h.helping()), (want, !matches!(want, Status::Needs | Status::Error)));
        }
        // One you're talking to wrote its turn for you: it waits on you as yours do, still the
        // PR's review.
        let mut talked = Session { helper: Some(Helper { pr: Some("o/r#7".into()), yours: true, ..Default::default() }), ..ended.clone() };
        apply_helper(&mut talked);
        assert_eq!(talked.status, Status::Unread);
        assert!(!talked.helping() && !talked.unattended() && !talked.helps_an_agent() && talked.helper.is_some());
    }

    /// A background job's agent tried effort levels on test chats in tmux, in its own folder, and
    /// each one's ended turn waited on you under the job's name (2026-10-14).
    #[test]
    fn a_session_in_another_sessions_scratch_folder_is_its_helper() {
        const JOB: &str = "/Users/me/.claude/jobs/54a9a7a0";
        let at = |id: &str, cwd: &str| Session { cwd: cwd.into(), status: Status::Unread, ..Session::new(Harness::Claude, "claude", id) };
        let owner_of = |p: &Path| p.starts_with(JOB).then(|| (Harness::Claude, "54a9a7a0-b633".to_string()));
        let mut sessions = vec![
            at("54a9a7a0-b633", "/r/codealong"),
            at("test-chat", &format!("{JOB}/tmp/efftest")),
            // Started there with `codealong start --helper`, it keeps what that said.
            Session { helper: Some(Helper { parent: Some("claude:claude:other".into()), pr: None, report: true, yours: false }), ..at("asked", JOB) },
            Session { headless: true, ..at("run", JOB) },
            at("mine", JOB),
            at("elsewhere", "/r/codealong"),
            // A fork you made of one there, and one its agent started for you without --helper.
            Session { origin: Some(SessionOrigin { key: "claude:claude:mine".into(), how: OriginHow::Fork }), ..at("fork", JOB) },
            Session { origin: Some(SessionOrigin { key: "claude:claude:54a9a7a0-b633".into(), how: OriginHow::Started }), ..at("yours", JOB) },
        ];
        scratch_helpers(&mut sessions, owner_of, |k| k == "claude:claude:mine");
        let boss = "claude:claude:54a9a7a0-b633";
        let helps = |id: &str| sessions.iter().find(|s| s.id == id).and_then(|s| s.helper.as_ref()).map(|h| (h.parent.clone().unwrap_or_default(), h.report));
        assert_eq!(helps("test-chat"), Some((boss.to_string(), false)));
        assert_eq!(sessions[1].origin, Some(SessionOrigin { key: boss.into(), how: OriginHow::Started }));
        assert_eq!(helps("asked"), Some(("claude:claude:other".to_string(), true)));
        assert_eq!((helps("run"), helps("mine"), helps("elsewhere"), helps("54a9a7a0-b633")), (None, None, None, None));
        assert_eq!((helps("fork"), helps("yours")), (None, None));
        // Its ended turn is its agent's to read.
        apply_helper(&mut sessions[1]);
        assert_eq!(sessions[1].status, Status::Idle);
        // A session whose own folder it isn't on the board makes no helper.
        let mut alone = vec![at("test-chat", &format!("{JOB}/tmp/efftest"))];
        scratch_helpers(&mut alone, owner_of, |_| false);
        assert_eq!(alone[0].helper, None);
    }

    #[test]
    fn a_print_run_is_headless_and_never_waits_on_you_for_its_end() {
        let a = acct("claude2");
        let at = |entrypoint: &str| TranscriptInfo { entrypoint: Some(entrypoint.into()), ..tr() };
        let headless = |r: Option<&RegistryEntry>, t: &TranscriptInfo| claude_session(&a, r, Some(t), 0, NOW, &Quiet::default()).unwrap().headless;
        // `claude -p`'s, running or done; not a terminal's, the app's or the Agent SDK's.
        assert!(headless(None, &at("sdk-cli")));
        assert!(headless(Some(&RegistryEntry { entrypoint: Some("sdk-cli".into()), ..reg("busy", NOW) }), &tr()));
        for e in ["cli", "claude-desktop", "sdk-ts"] {
            assert!(!headless(None, &at(e)), "{e}");
        }
        // Resumed in a terminal, it's that while it runs there.
        assert!(!headless(Some(&RegistryEntry { entrypoint: Some("cli".into()), ..reg("idle", NOW) }), &at("sdk-cli")));
        // Its ended turn is idle, an error too; one waiting on a call to allow still needs you.
        for (st, want) in [(Status::Unread, Status::Idle), (Status::Error, Status::Idle), (Status::Needs, Status::Needs), (Status::Working, Status::Working)] {
            let mut s = Session { status: st, headless: true, ..Session::new(Harness::Claude, "claude", "p") };
            apply_headless(&mut s);
            assert_eq!(s.status, want);
        }
        let mut yours = Session { status: Status::Error, ..Session::new(Harness::Claude, "claude", "y") };
        apply_headless(&mut yours);
        assert_eq!(yours.status, Status::Error);
        // Out of what waits on you while it doesn't need you, as a helper is.
        let run = |status| Session { status, headless: true, ..Session::new(Harness::Claude, "claude", "p") };
        assert!(run(Status::Working).unattended() && run(Status::Idle).unattended());
        assert!(!run(Status::Needs).unattended());
        assert!(!Session { status: Status::Working, ..Session::new(Harness::Claude, "claude", "y") }.unattended());
    }

    #[test]
    fn a_headless_run_was_started_by_the_nearest_session_above_its_process() {
        // An eval's judge: claude -p (40) under python (30), under the shell the agent's Bash
        // tool opened (20), under the agent's claude (10), under a background job's host (5).
        let parents = HashMap::from([(40, 30), (30, 20), (20, 10), (10, 5), (5, 1), (77, 1), (90, 91), (91, 90)]);
        let parent = |p: u32| parents.get(&p).copied();
        let by_pid = HashMap::from([(10, "claude:claude2:agent"), (5, "claude:claude2:host")]);
        assert_eq!(started_by(40, parent, &by_pid), Some("claude:claude2:agent"));
        // Run from a script in a terminal of yours, it meets no session's process on the way up.
        assert_eq!(started_by(77, parent, &by_pid), None);
        // Nor once a process on the way is gone, nor round a loop.
        assert_eq!(started_by(41, parent, &by_pid), None);
        assert_eq!(started_by(90, parent, &by_pid), None);
    }

    #[test]
    fn a_headless_run_no_session_is_above_is_the_one_its_environment_names() {
        // The judge's claude -p (40) under a driver the agent started on its own (30), left to
        // launchd; a run beside the agent (50); one from a terminal of yours (77); one whose
        // session isn't on the board (88).
        let parents = HashMap::from([(40, 30), (30, 1), (50, 10), (10, 1), (77, 1), (88, 1)]);
        let under = |p: u32| match p {
            40 | 50 => vec![(Harness::Claude, "agent".to_string())],
            88 => vec![(Harness::Claude, "gone".to_string())],
            _ => vec![],
        };
        let by_pid = HashMap::from([(10, "claude:claude2:beside")]);
        let by_id = HashMap::from([((Harness::Claude, "agent"), "claude:claude2:agent")]);
        let found = |pid| run_started_by(pid, |p: u32| parents.get(&p).copied(), under, &by_pid, &by_id);
        assert_eq!(found(40), Some("claude:claude2:agent"));
        // The session above it is the one that ran it, whatever its environment says.
        assert_eq!(found(50), Some("claude:claude2:beside"));
        assert_eq!(found(77), None);
        assert_eq!(found(88), None);
        // An ID is another harness's session's only under that harness.
        let codex = HashMap::from([((Harness::Codex, "agent"), "codex:codex:agent")]);
        assert_eq!(run_started_by(40, |p: u32| parents.get(&p).copied(), under, &by_pid, &codex), None);
        // A run from a Codex thread's command names the thread and the Claude Code session that
        // started Codex: the one on the board is the run's.
        let both = |_: u32| vec![(Harness::Claude, "gone".to_string()), (Harness::Codex, "agent".to_string())];
        assert_eq!(run_started_by(40, |p: u32| parents.get(&p).copied(), both, &by_pid, &codex), Some("codex:codex:agent"));
    }

    #[test]
    fn a_real_process_climbs_to_the_one_that_started_its_shell() {
        // This test as the session, a shell its command opened, and a run the shell started.
        let mut shell = std::process::Command::new("/bin/sh").args(["-c", "sleep 5 & echo $!; wait"]).stdout(std::process::Stdio::piped()).spawn().unwrap();
        let mut line = String::new();
        std::io::BufRead::read_line(&mut std::io::BufReader::new(shell.stdout.take().unwrap()), &mut line).unwrap();
        let run: u32 = line.trim().parse().unwrap();
        let by_pid = HashMap::from([(std::process::id(), "claude:claude:me")]);
        let found = started_by(run, crate::procs::parent_pid, &by_pid);
        let _ = std::process::Command::new("/bin/kill").arg(run.to_string()).status();
        let _ = shell.wait();
        assert_eq!(found, Some("claude:claude:me"));
    }

    #[test]
    fn a_sessions_helpers_are_the_ones_it_started_as_helpers_each_once() {
        let of = |id: &str, parent: Option<&str>| Session { helper: parent.map(|p| Helper { parent: Some(p.into()), pr: None, report: false, yours: false }), ..Session::new(Harness::Claude, "claude", id) };
        let visiting = Session { visitor: Some("worked here".into()), ..of("a", Some("claude:claude:boss")) };
        let archived = Session { archived: true, ..of("old", Some("claude:claude:boss")) };
        let lanes = [
            Lane { sessions: vec![of("boss", None), of("a", Some("claude:claude:boss")), of("b", Some("claude:claude:other")), archived], ..Lane::new("/r/app", "P", "app") },
            Lane { sessions: vec![visiting, of("c", Some("claude:claude:boss"))], ..Lane::new("/r/app-two", "P", "app") },
        ];
        let ids: Vec<&str> = helpers_of(&lanes, "claude:claude:boss").iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, ["a", "c"]);
    }

    #[test]
    fn finds_prs_by_branch_and_owner() {
        let s = seed("/r/spark-portal", "portal");
        let mut prs = HashMap::new();
        prs.insert("sprucehq/spark".to_string(), vec![pr(PrState::Approved, 0)]);
        assert_eq!(find_pr(&s, &prs, NOW).map(|p| p.number), Some(1037));
        prs.insert("sprucehq/spark".to_string(), vec![PrRaw { head_owner: Some("Gwen".into()), ..pr(PrState::Open, 0) }]);
        assert_eq!(find_pr(&s, &prs, NOW).map(|p| p.number), Some(1037), "an owner in another case than the remote's");
        let mut other_owner = pr(PrState::Open, 0);
        other_owner.head_owner = Some("stranger".into());
        prs.insert("sprucehq/spark".to_string(), vec![other_owner]);
        assert!(find_pr(&s, &prs, NOW).is_none());
        let mut main = s.clone();
        main.branch = Some("main".into());
        assert!(find_pr(&main, &prs, NOW).is_none());
        let mut old = pr(PrState::Merged, 0);
        old.merged_at = Some(NOW - 8 * DAY_MS);
        prs.insert("sprucehq/spark".to_string(), vec![old]);
        assert!(find_pr(&s, &prs, NOW).is_none());
        let mut moved_on = pr(PrState::Merged, 0);
        moved_on.head_oid = Some("ffffff0".into());
        prs.insert("sprucehq/spark".to_string(), vec![moved_on]);
        assert!(find_pr(&s, &prs, NOW).is_none(), "a reused branch drops its merged PR");
    }

    #[test]
    fn ranks_bays() {
        let a = acct("claude2");
        let needs = claude_session(&a, Some(&reg("waiting", NOW)), Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
        let working = claude_session(&a, Some(&reg("busy", NOW)), Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
        let unread = claude_session(&a, Some(&reg("idle", NOW)), Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
        let open_pr = find_pr(&seed("/r", "portal"), &HashMap::from([("sprucehq/spark".to_string(), vec![pr(PrState::Open, 0)])]), NOW);
        let failing = find_pr(&seed("/r", "portal"), &HashMap::from([("sprucehq/spark".to_string(), vec![pr(PrState::Open, 2)])]), NOW);
        let merged = find_pr(&seed("/r", "portal"), &HashMap::from([("sprucehq/spark".to_string(), vec![pr(PrState::Merged, 0)])]), NOW);
        assert_eq!(bay_of(&[working.clone(), needs.clone()], None), (Bay::Needs, Status::Needs));
        // A PR that needs work doesn't make the worktree need you: its agents do.
        assert_eq!(bay_of(&[unread.clone()], failing.as_ref()), (Bay::Unread, Status::Unread));
        assert_eq!(bay_of(&[], failing.as_ref()), (Bay::Review, Status::Idle));
        assert_eq!(bay_of(&[working.clone()], failing.as_ref()), (Bay::Working, Status::Working));
        assert_eq!(bay_of(&[unread.clone()], open_pr.as_ref()), (Bay::Unread, Status::Unread));
        assert_eq!(bay_of(&[], open_pr.as_ref()), (Bay::Review, Status::Idle));
        assert_eq!(bay_of(&[], merged.as_ref()), (Bay::Merged, Status::Idle));
        assert_eq!(bay_of(&[], None), (Bay::Idle, Status::Idle));
    }

    #[test]
    fn writes_headlines() {
        let a = acct("claude2");
        let needs = claude_session(&a, Some(&reg("waiting", NOW)), Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
        // One whose reply asks you something asks you; it isn't waiting for approval.
        let asking = Session { asks: true, excerpt: Some("Asks you · Want the PR?".into()), ..needs.clone() };
        assert_eq!(session_headline(&asking), ("`claude2` asks you · Portal daily log + photo sync".into(), Some("Want the PR?".into())));
        // So does one whose question is held through the plugin, though what you type doesn't
        // answer it.
        let held = Session { asks: false, excerpt: Some("Asks you · Which color?".into()), ..needs.clone() };
        assert_eq!(session_headline(&held), ("`claude2` asks you · Portal daily log + photo sync".into(), Some("Which color?".into())));
        // Needs you with nothing to say: the title is in the headline, nothing under it.
        assert_eq!(session_headline(&Session { excerpt: None, ..needs.clone() }), ("`claude2` needs you · Portal daily log + photo sync".to_string(), None));
        // A finished turn is its title alone: its glyph and bay say it finished.
        let done = Session { status: Status::Unread, excerpt: Some("Pushed the fix".into()), ..needs.clone() };
        assert_eq!(session_headline(&done), ("`claude2` · Portal daily log + photo sync".to_string(), Some("Pushed the fix".to_string())));
        assert_eq!(
            headline(Bay::Needs, &[needs], None, None),
            ("`claude2` is waiting for approval · Portal daily log + photo sync".to_string(), Some("Bash · `yarn mikro-orm migration:up`".to_string()))
        );
        let working = claude_session(&a, Some(&reg("busy", NOW - 5_000)), Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
        let (h, d) = headline(Bay::Working, &[working], None, None);
        assert_eq!(h, "`claude2` · Portal daily log + photo sync");
        assert_eq!(d, Some(format!("{{for:{}}} · running `yarn test`", NOW - 5_000)));
        // A message sent while it works restarts the clock.
        let nudged = TranscriptInfo { last_prompt_at: Some(NOW - 2_000), ..tr() };
        let working = claude_session(&a, Some(&reg("busy", NOW - 5_000)), Some(&nudged), 0, NOW, &Quiet::default()).unwrap();
        assert_eq!(headline(Bay::Working, &[working], None, None).1, Some(format!("{{for:{}}} · running `yarn test`", NOW - 2_000)));
        let failing = find_pr(&seed("/r", "portal"), &HashMap::from([("sprucehq/spark".to_string(), vec![pr(PrState::Open, 2)])]), NOW);
        assert_eq!(
            headline(Bay::Review, &[], failing.as_ref(), None),
            ("#1037 · checks failing".to_string(), Some("`check-0` · `check-1`".to_string()))
        );
        let review = |state: PrState, merge: Merge| {
            let pr = find_pr(&seed("/r", "portal"), &HashMap::from([("sprucehq/spark".to_string(), vec![PrRaw { merge, ..pr(state, 0) }])]), NOW);
            headline(Bay::Review, &[], pr.as_ref(), None)
        };
        assert_eq!(review(PrState::Approved, Merge::Clean), ("#1037 approved · ready to merge".to_string(), Some("11 checks passed".to_string())));
        // Only GitHub can say it's ready: approved and passing can still conflict with the base.
        assert_eq!(review(PrState::Approved, Merge::Unknown).0, "#1037 approved");
        assert_eq!(review(PrState::Approved, Merge::Conflicts).0, "#1037 has conflicts");
        assert_eq!(review(PrState::Open, Merge::Blocked).0, "#1037 waiting for review");
        // A repo that requires no review.
        assert_eq!(review(PrState::Open, Merge::Clean).0, "#1037 ready to merge");
    }

    #[test]
    fn a_worktrees_url_opens_on_the_server_it_names() {
        let server = |port: u16, page: bool, ancestors: Vec<u32>| crate::previews::Server {
            lane: PathBuf::from("/r/spark-portal"),
            pid: port as u32,
            port,
            name: "node".into(),
            cwd: PathBuf::from("/r/spark-portal/apps/app"),
            started_ms: NOW - 60_000,
            page,
            title: page.then(|| "Spruce".to_string()),
            ancestors,
        };
        let (api, app, storybook) = (server(3040, false, vec![3040, 900]), server(5210, true, vec![5210, 900]), server(6006, true, vec![6006, 1]));
        let servers = [&api, &app, &storybook];
        let entry = |url: &str, port: Option<u16>| crate::previews::Entry { worktree: "/r/spark-portal".into(), url: url.into(), port, chrome_profile: None, at: NOW };
        let port_of = |p: &Option<Preview>| p.as_ref().and_then(|p| p.server.as_ref()).map(|s| s.port);

        // A URL a proxy serves opens on the worktree's page, the lowest port when two serve one.
        let proxied = lane_preview(Some(&entry("https://portal.app.test", None)), &servers, &[], None);
        assert_eq!(port_of(&proxied), Some(5210));
        assert!(proxied.as_ref().is_some_and(|p| p.set && p.url == "https://portal.app.test"));
        // The port given with it, or the one a local URL names, picks the server, page or not.
        assert_eq!(port_of(&lane_preview(Some(&entry("https://portal.app.test", Some(3040))), &servers, &[], None)), Some(3040));
        assert_eq!(port_of(&lane_preview(Some(&entry("http://localhost:6006", None)), &servers, &[], None)), Some(6006));
        // Nothing listening: set, not running.
        let down = lane_preview(Some(&entry("https://portal.app.test", None)), &[], &[], None).unwrap();
        assert!(down.set && down.server.is_none());
        assert!(lane_preview(Some(&entry("https://portal.app.test", Some(5299))), &servers, &[], None).unwrap().server.is_none());
        // With no URL set, a page served there opens at localhost; an API alone isn't an app.
        let found = lane_preview(None, &servers, &[], None).unwrap();
        assert_eq!((found.url.as_str(), found.set), ("http://localhost:5210", false));
        assert_eq!(lane_preview(None, &[&api], &[], None), None);
        assert_eq!(lane_preview(None, &[], &[], None), None);
        // The session whose command started it: its process is one of the server's parents.
        let a = acct("claude2");
        let mut started = claude_session(&a, Some(&reg("shell", NOW)), Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
        started.pid = Some(900);
        let found = lane_preview(None, &servers, &[started.clone()], None).unwrap();
        assert_eq!(found.server.unwrap().session.as_deref(), Some(started.key.as_str()));
        // It's opened from the worktree's menus, and says where and whether it runs.
        let apps = AppNames { editor: "Cursor".into(), ..Default::default() };
        let (t, default) = targets(&seed("/r/spark-portal", "portal"), &[], None, proxied.as_ref(), &apps);
        let open = t.iter().find(|x| x.id == "l|preview").unwrap();
        assert_eq!((open.label.as_str(), open.detail.as_deref()), ("Open preview", Some("portal.app.test · node on port 5210")));
        assert_eq!(default.as_deref(), Some("l|terminal"));
        let (t, _) = targets(&seed("/r/spark-portal", "portal"), &[], None, Some(&down), &apps);
        assert_eq!(t.iter().find(|x| x.id == "l|preview").unwrap().detail.as_deref(), Some("portal.app.test · not running"));

        // It opens in the project's Chrome profile unless the worktree names its own.
        let mut own = entry("https://portal.app.test", None);
        let project = lane_preview(Some(&own), &servers, &[], Some("Work")).unwrap();
        assert_eq!((project.chrome_profile.as_deref(), project.chrome_profile_set), (Some("Work"), false));
        own.chrome_profile = Some("Dev".into());
        let mine = lane_preview(Some(&own), &servers, &[], Some("Work")).unwrap();
        assert_eq!((mine.chrome_profile.as_deref(), mine.chrome_profile_set), (Some("Dev"), true));
        assert_eq!(lane_preview(None, &servers, &[], Some("Work")).unwrap().chrome_profile.as_deref(), Some("Work"));
        assert_eq!(lane_preview(None, &servers, &[], None).unwrap().chrome_profile, None);
        let (t, _) = targets(&seed("/r/spark-portal", "portal"), &[], None, Some(&mine), &apps);
        assert_eq!(t.iter().find(|x| x.id == "l|preview").unwrap().detail.as_deref(), Some("portal.app.test · node on port 5210 · Chrome's Dev profile"));

        // A link into its app opens in its profile, from any worktree's conversation; a link
        // elsewhere, or into an app that opens in the default browser, doesn't.
        let at = |id: &str, preview: Option<Preview>| Lane { preview, ..crate::fixtures::lane(id, vec![]) };
        let lanes = vec![at("/r/spark", None), at("/r/spark-plain", lane_preview(None, &[&api, &storybook], &[], None)), at("/r/spark-portal", Some(mine.clone()))];
        let app = |url: &str| app_of_link(&lanes, url).map(|l| l.id.as_str());
        assert_eq!(app("https://portal.app.test/orgs/a/projects/b"), Some("/r/spark-portal"));
        assert_eq!(app("http://127.0.0.1:5210/"), Some("/r/spark-portal"));
        assert_eq!(app("http://localhost:6006"), None);
        assert_eq!(app("https://github.com/o/r/pull/1"), None);
        let elsewhere = DeviceTag { name: "Mini".into(), short: "abc".into(), color: "blue".into(), online: true, last_seen: None };
        let theirs = vec![Lane { device: Some(elsewhere), ..lanes[2].clone() }];
        assert!(app_of_link(&theirs, "https://portal.app.test/x").is_none());
    }

    #[test]
    fn a_page_is_the_sessions_that_made_it_latest_first() {
        let made = |url: &str, at: i64| Artifact { url: url.into(), title: "Roles".into(), note: None, at };
        let session = |key: &str, account: &str, artifacts: Vec<Artifact>| Session { account: account.into(), artifacts, ..crate::fixtures::session(key) };
        // One artifact as Claude Code links what it published, which the Claude app links by its
        // uuid; and another.
        let roles = "https://claude.ai/artifact/Lra3DKxkDb6AU1Apmc4F19";
        let viewed = "https://claude.ai/code/artifact/a0c97093-8bbc-43cc-852c-63bff702d568";
        let other = "https://claude.ai/artifact/6zxdiitFmMYwA7d92F7zSo";
        let mine = Lane {
            sessions: vec![session("claude:claude:a", "claude", vec![made(roles, 10)]), session("claude:claude2:b", "claude2", vec![made(other, 5), made(roles, 20)])],
            ..crate::fixtures::lane("/r/atlas", vec![])
        };
        let keys = |lanes: &[Lane], url: &str| sessions_with_page(lanes, url).iter().map(|s| s.key.clone()).collect::<Vec<_>>();
        let lanes = vec![mine.clone()];
        assert_eq!(keys(&lanes, roles), ["claude:claude2:b", "claude:claude:a"]);
        assert_eq!(keys(&lanes, viewed), ["claude:claude2:b", "claude:claude:a"]);
        assert_eq!(keys(&lanes, &format!("{other}/?v=2#top")), ["claude:claude2:b"]);
        assert!(keys(&lanes, "https://claude.ai/artifact/Other").is_empty());
        // Another machine's sessions are signed in to its accounts, not this Mac's.
        let elsewhere = DeviceTag { name: "Mini".into(), short: "abc".into(), color: "blue".into(), online: true, last_seen: None };
        assert!(keys(&[Lane { device: Some(elsewhere), ..mine }], roles).is_empty());
    }

    #[test]
    fn builds_targets_with_app_names() {
        let apps = AppNames { terminal: "Ghostty".into(), terminal_id: "ghostty".into(), editor: "Cursor".into(), editor_id: "cursor".into(), ..Default::default() };
        let a = acct("claude2");
        let live = claude_session(&a, Some(&reg("waiting", NOW)), Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
        let (t, default) = targets(&seed("/r/spark-portal", "portal"), &[live.clone()], None, None, &apps);
        assert_eq!(t[0].label, "Respond in Ghostty");
        assert_eq!(default.as_deref(), Some("s|claude:claude2:abc1234567|focus"));
        // A session running in another terminal than the one config.toml names is shown there.
        let (t, _) = targets(&seed("/r/spark-portal", "portal"), &[Session { terminal: Some("Rex".into()), ..live }], None, None, &apps);
        assert_eq!(t[0].label, "Respond in Rex");
        assert!(t.iter().all(|x| !x.id.ends_with("|claude-app")), "claude2 isn't the Claude app's account");
        assert!(t.iter().any(|x| x.label == "Open in Cursor"));
        let main = AccountCfg { name: "claude".into(), harness: "claude".into(), home: "~/.claude".into(), command: None, start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false, chrome_profile: None };
        let mine = claude_session(&main, Some(&reg("waiting", NOW)), Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
        let (t, _) = targets(&seed("/r/spark-portal", "portal"), &[mine], None, None, &apps);
        let app = |t: &[Target]| t.iter().find(|x| x.id.ends_with("|claude-app")).map(|x| (x.label.clone(), x.detail.clone().unwrap_or_default(), x.disabled));
        // Live in a terminal, the app takes it only through its Remote Control.
        assert_eq!(app(&t), Some(("Open in Claude app".to_string(), "Needs Remote Control (/remote-control)".to_string(), true)));
        let controlled = RegistryEntry { remote_control: Some("session_01abc".into()), ..reg("waiting", NOW) };
        let mine = claude_session(&main, Some(&controlled), Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
        assert_eq!(mine.remote_control.as_deref(), Some("session_01abc"));
        let (t, _) = targets(&seed("/r/spark-portal", "portal"), &[mine], None, None, &apps);
        assert_eq!(t[0].label, "Respond in Ghostty", "the terminal still leads");
        assert!(app(&t).is_some_and(|(_, why, off)| !off && why.starts_with("claude · ")));
        // A background job too, beside attaching it.
        let job = RegistryEntry { kind: Some("bg".into()), job_id: Some("b887d77f".into()), ..controlled };
        let (t, _) = targets(&seed("/r/spark-portal", "portal"), &[claude_session(&main, Some(&job), Some(&tr()), 0, NOW, &Quiet::default()).unwrap()], None, None, &apps);
        assert_eq!(t[0].label, "Attach in Ghostty");
        assert!(app(&t).is_some_and(|(_, _, off)| !off));
        let (none, default) = targets(&seed("/r/x", "x"), &[], None, None, &apps);
        assert_eq!(default.as_deref(), Some("l|terminal"));
        assert_eq!(none.len(), 3);
    }

    #[test]
    fn a_session_offers_its_chrome_tabs() {
        let apps = AppNames { terminal: "Ghostty".into(), ..Default::default() };
        let a = acct("claude2");
        let mut s = claude_session(&a, Some(&reg("idle", NOW)), Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
        s.chrome = Some(ChromeTabs { tabs: vec![12, 11], url: "https://www.admin.google.com/ac/users?x=1".into(), title: "Users".into(), profile: Some("Work".into()), profile_dir: Some("Profile 12".into()), at: NOW });
        let (t, default) = targets(&seed("/r/x", "x"), &[s.clone()], None, None, &apps);
        let c = t.iter().find(|x| x.id == "s|claude:claude2:abc1234567|chrome").expect("a Chrome target");
        assert_eq!((c.label.as_str(), c.group.as_str()), ("Show in Chrome (Work)", "browser"));
        assert!(c.detail.as_deref().unwrap().starts_with("Users · claude2 · "));
        // It isn't how the session or the lane opens.
        assert_eq!(default.as_deref(), Some(s.open_target.as_str()));
        let tabs = s.chrome.as_mut().unwrap();
        tabs.tabs.clear();
        tabs.profile = None;
        assert_eq!(chrome_label(tabs), "Reopen admin.google.com in Chrome");
    }

    #[test]
    fn a_lane_opens_its_most_recent_session() {
        let apps = AppNames::default();
        let lane = seed("/r/x", "x");
        let a = acct("claude2");
        let mut needs = claude_session(&a, Some(&reg("waiting", NOW - 3_600_000)), Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
        needs.last_activity = NOW - 3_600_000;
        let mut newer = claude_session(&a, None, Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
        newer.key = "claude:claude2:newer".into();
        newer.open_target = format!("s|{}|resume", newer.key);
        newer.last_activity = NOW - 60_000;
        // The lane lists the one that needs you first, but opening the lane resumes the newer one.
        let (t, default) = targets(&lane, &[needs.clone(), newer.clone()], None, None, &apps);
        assert_eq!(default.as_deref(), Some(newer.open_target.as_str()));
        assert_eq!(t[0].id, newer.open_target);
        assert_eq!(t[0].kbd, Some(vec!["⏎".to_string()]));
        let other = t.iter().find(|x| x.id == needs.open_target).expect("the other session is still offered");
        assert_eq!(other.kbd, None);
        // A tie goes to the more urgent one, listed first.
        newer.last_activity = needs.last_activity;
        let (_, default) = targets(&lane, &[needs.clone(), newer.clone()], None, None, &apps);
        assert_eq!(default.as_deref(), Some(needs.open_target.as_str()));
        // A session from another lane that worked here more recently doesn't beat the lane's own,
        // but opens a lane that has none.
        let mut guest = newer.clone();
        guest.key = "claude:claude2:guest".into();
        guest.open_target = format!("s|{}|resume", guest.key);
        guest.visitor = Some("worked here".into());
        guest.last_activity = NOW;
        let (_, default) = targets(&lane, &[needs.clone(), guest.clone()], None, None, &apps);
        assert_eq!(default.as_deref(), Some(needs.open_target.as_str()));
        let (_, default) = targets(&lane, &[guest.clone()], None, None, &apps);
        assert_eq!(default.as_deref(), Some(guest.open_target.as_str()));
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
        let board = compose(Inputs { now: NOW, apps: &apps, seeds: &seeds, sessions: vec![maker], visitors: &visitors, git: &git, prs: &prs, requested: &HashMap::new(), made: &HashMap::new(), project_order: &order, notices: vec![], loading: false, github_at: None, pinned: &[], github_login: None, pr_links: &PrLinks::default(), previews: &Previews::default(), reusable: &HashMap::new(), review_snoozes: &[], session_settings: &Default::default(), auto_reviews: (&Default::default(), &Default::default(), &Default::default()), machines: &Default::default() });
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
            session_settings: &Default::default(), auto_reviews: (&Default::default(), &Default::default(), &Default::default()), machines: &Default::default(),
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
            session_settings: &Default::default(), auto_reviews: (&Default::default(), &Default::default(), &Default::default()), machines: &Default::default(),
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
            let board = compose(Inputs { now: NOW, apps: &apps, seeds: &seeds, sessions, visitors: &visitors, git: &git, prs: &prs, requested: &HashMap::new(), made: &HashMap::new(), project_order: &order, notices: vec![], loading: false, github_at: None, pinned: &[], github_login: None, pr_links: &PrLinks::default(), previews: &Previews::default(), reusable: &HashMap::new(), review_snoozes: &[], session_settings: &Default::default(), auto_reviews: (&Default::default(), &Default::default(), &Default::default()), machines: &Default::default() });
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
            session_settings: &Default::default(), auto_reviews: (&Default::default(), &Default::default(), &Default::default()), machines: &Default::default(),
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
        let board = compose(Inputs { now: NOW, apps: &apps, seeds: &seeds, sessions: vec![], visitors: &visitors, git: &git, prs: &prs, requested: &HashMap::new(), made: &HashMap::new(), project_order: &order, notices: vec![], loading: false, github_at: None, pinned: &[], github_login: None, pr_links: &PrLinks::default(), previews: &Previews::default(), reusable: &HashMap::new(), review_snoozes: &[], session_settings: &Default::default(), auto_reviews: (&Default::default(), &Default::default(), &Default::default()), machines: &Default::default() });
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
            machines: &Default::default(),
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
        for need in [Some(Need::Approval), Some(Need::Question), Some(Need::Reply), Some(Need::Message), Some(Need::Stuck), Some(Need::Input), None] {
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
        let says: serde_json::Map<String, serde_json::Value> = all()
            .filter(|s| s.visitor.is_none())
            .map(|s| {
                let said = serde_json::json!({
                    "unattended": s.unattended(),
                    "helpsAnAgent": s.helps_an_agent(),
                    "waitsInBackground": s.waits_in_background(),
                    "waitsAtAPrompt": s.waits_at_a_prompt(),
                });
                (s.key.clone(), said)
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
}
