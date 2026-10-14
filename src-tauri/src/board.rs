//! The rules from *The model*: session status, lane bays, headlines and open targets.
//! Pure functions over already-read data, so every rule is unit-tested here.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::antigravity::{Conversation, Process};
use crate::claude::{RegistryEntry, TranscriptInfo};
use crate::codex::ThreadInfo;
use crate::opencode::{Live, SessionRow};
use crate::config::AccountCfg;
use crate::github::PrRaw;
use crate::model::*;
use crate::state::{Snooze, UnreadMark};
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

/// What the engine found outside the transcript for a busy session that has gone quiet.
#[derive(Default)]
pub struct Quiet<'a> {
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
    /// Lanes for sessions outside every project; hidden unless they need you, or hold chats.
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
    /// PRs whose primary session is told of what happens on them (nudges.rs), turned on for
    /// each, and the ones turned off.
    pub nudged: HashSet<String>,
    pub not_nudged: HashSet<String>,
    /// `[pull_requests] nudge_by_default`: every open PR of yours is told, unless turned off.
    pub nudge_by_default: bool,
    /// The same for particular repos, by slug, where your `[repos."github.com/…"]` table says.
    pub nudge_by_default_in: HashMap<String, bool>,
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
        r.status_updated_at = Some(from.last_ts);
    }
    r
}

/// How a session's excerpt starts while it holds another session's message for you to allow
/// (`claude::held`): the sender follows.
pub const HELD_MESSAGE: &str = "Held message from ";

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
    // A process that came up after its transcript's last line and has been idle since has said
    // nothing new: Claude Code's background service restarting its jobs on a new version after
    // an update, or a `--resume` nobody has typed into yet. Its fresh registry times aren't a
    // turn that ended.
    let restarted = reg
        .zip(tr)
        .filter(|(r, t)| r.status == "idle" && r.started_at.is_some_and(|s| s > t.last_ts))
        .map(|(r, t)| moved_registry(r, t));
    let reg = restarted.as_ref().or(reg);
    // The registry doesn't refresh while a session is busy; its transcript, status changes and
    // subagent logs do. Stuck mid-turn, it can't go on without you.
    let stalled_since = claude_busy_since(reg, tr)
        .map(|t| t.max(quiet.subagent_at.unwrap_or(0)))
        .filter(|t| now - t > CLAUDE_STALL_MS);
    let background = reg.is_some_and(|r| r.status == "shell");
    let left_running = background && quiet.background_active_at.is_some_and(|t| now - t >= BACKGROUND_QUIET_MS);
    let status = match reg {
        // Its process ended, closed or with the Mac restarting, but a turn you haven't seen is
        // still yours to read for a while.
        None if tr.is_some_and(|t| t.last_ts > seen_at && now - t.last_ts < CLAUDE_UNSEEN_STOPPED_MS) => Status::Unread,
        None => Status::Stopped,
        Some(_) if stalled_since.is_some() => Status::Needs,
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
        .and_then(|t| t.title.clone())
        .or(user_named)
        .or_else(|| tr.and_then(|t| t.ai_title.clone()))
        .or_else(|| tr.and_then(|t| t.first_prompt.clone()))
        .or_else(|| reg.and_then(|r| r.name.clone()))
        .unwrap_or_else(|| format!("Session {}", short_id(&id)));
    let last_activity = [
        tr.map(|t| t.last_ts),
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
        // Its turn is over: it waits on the command (a CI watch, a build) to wake it, which
        // "working" alone hid behind a spinner (asked 2026-10-07).
        Status::Working if background => Some(match quiet.running {
            Some(cmd) => format!("Waiting on {}", code(&truncate(&one_line(cmd, 200), 80))),
            None => "Waiting on a command in the background".to_string(),
        }),
        Status::Working => tr.and_then(|t| t.last_tool.clone()),
        // Where it was when it stopped.
        _ if cut_off.is_some() => tr.and_then(|t| t.last_tool.clone().or_else(|| t.last_text.clone())),
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
        status_since: reg.and_then(|r| r.status_updated_at),
        last_prompt_at: tr
            .and_then(|t| t.last_prompt_at)
            .or_else(|| reg.and_then(|r| r.started_at))
            .or_else(|| tr.and_then(|t| t.started_at))
            .unwrap_or(last_activity),
        last_activity,
        excerpt,
        branch: tr.and_then(|t| t.git_branch.clone()),
        attach_command,
        stalled_since,
        background: background.then(|| quiet.running.unwrap_or_default().to_string()),
        app_account: account.home_path() == crate::util::home().join(".claude"),
        limit,
        connection_lost,
        cut_off,
        model: tr.and_then(|t| t.model.clone()),
        effort: tr.and_then(|t| t.effort.clone()),
        mode: tr.and_then(|t| t.permission_mode.as_deref()).and_then(crate::claude::session_mode),
        context: tr.and_then(|t| t.context).map(|used| ContextUse { used, window: tr.and_then(|t| t.window_model.as_deref().or(t.model.as_deref())).and_then(crate::prices::context_window) }),
        // Held only while it runs: the lock and the cleanup at exit are its process's.
        own_worktree: tr.and_then(|t| t.own_worktree.clone()).filter(|_| reg.is_some()),
        color: tr.and_then(|t| t.color.as_deref()).and_then(crate::emblems::palette_color).map(String::from),
        headless,
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
/// when that isn't known.
pub fn codex_session(account: &AccountCfg, th: &ThreadInfo, name: Option<&str>, terminal: Option<u32>, held: Option<bool>, seen_at: i64, now: i64) -> Session {
    let fresh = now - th.last_ts < CODEX_STALE_MS;
    // A turn left open in a rollout no Codex process holds was cut off: the terminal UI, the app
    // or the agent running it went away partway through, and nothing will close it.
    let cut_off = (th.turn_open && terminal.is_none() && held == Some(false)).then_some(th.last_ts);
    let unanswered = codex_unanswered(th, now).filter(|_| cut_off.is_none());
    let status = if cut_off.is_some() {
        if th.last_ts > seen_at { Status::Error } else { Status::Idle }
    } else if th.pending_approval.is_some() || unanswered.is_some() {
        Status::Needs
    } else if th.turn_open && fresh {
        Status::Working
    } else if th.last_complete_at.map(|t| t > seen_at).unwrap_or(false) {
        Status::Unread
    } else {
        Status::Idle
    };
    let title = name
        .map(String::from)
        .or_else(|| th.last_user_message.clone())
        .unwrap_or_else(|| "Codex thread".to_string());
    let excerpt = match status {
        Status::Needs => th.pending_approval.clone().or(unanswered),
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
        last_prompt: th.last_user_message.clone(),
        started_at: th.started_at,
        // Waiting on you since its oldest unanswered call, not since the turn before it ended:
        // Needs you is a queue by that time.
        status_since: match status {
            _ if cut_off.is_some() => cut_off,
            Status::Working => th.turn_started_at,
            Status::Needs => th.pending_calls.iter().map(|c| c.at).min().or(Some(th.last_ts)),
            _ => th.last_complete_at,
        },
        last_prompt_at: th.last_prompt_at.or(th.started_at).unwrap_or(th.last_ts),
        last_activity: th.last_ts,
        excerpt,
        resume_command: Some(format!("{} resume {}", account.command(), th.id)),
        app_account: account.home_path() == crate::util::home().join(".codex"),
        branch: th.branch.clone(),
        origin: codex_origin(account, th),
        model: th.model.clone(),
        effort: th.effort.clone(),
        mode: th.mode.clone(),
        context: th.context.map(|(used, window)| ContextUse { used, window: window.or_else(|| th.model.as_deref().and_then(crate::prices::context_window)) }),
        cut_off,
        // `codex resume` goes on in the same rollout, which still says `exec`: someone is at it
        // while a terminal has it, and it's headless again once that ends, as Claude's is.
        headless: th.exec && terminal.is_none(),
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
        ..known
    }
}

/// An Antigravity conversation, from its summary and the end of its record. `process` is the
/// command line process that has it open, when one does, and `app_running` whether the hub
/// app's agent is up. A turn is under way only while what runs it is there: the command line's
/// conversation in its own process, the app's in the app's agent, which is taken to have gone
/// quiet when nothing has been written for a while. A step whose permission prompt is up needs
/// you. The summary keeps no word for a failure, so none shows as one.
pub fn antigravity_session(account: &AccountCfg, c: &Conversation, process: Option<&Process>, app_running: bool, seen_at: i64, now: i64) -> Session {
    let fresh = now - c.last_modified < ANTIGRAVITY_STALE_MS;
    // While its process has the conversation, its summary is the process's word, unless another
    // `agy` that started since marked it idle; a step that runs or waits says so too. A print
    // run writes its summary only as it ends, so its record says whether a turn is under way.
    let under_way = match (c.cli, process) {
        (true, Some(p)) => c.running() || c.idled_by_another || c.step_under_way || (!p.terminal && c.tail.turn_open),
        (true, None) => false,
        (false, _) => c.running() && app_running,
    };
    let news = c.last_modified > seen_at;
    let status = if c.killed {
        Status::Idle
    } else if under_way && c.waiting {
        Status::Needs
    } else if under_way && (c.cli || fresh) {
        Status::Working
    } else if news && c.last_modified > c.last_input.unwrap_or(0) {
        Status::Unread
    } else {
        Status::Idle
    };
    let title = c.title.clone().or_else(|| c.preview.clone()).unwrap_or_else(|| "Antigravity conversation".to_string());
    let excerpt = match status {
        Status::Needs => c.tail.open.first().map(|call| call.label.clone()).or_else(|| Some("A step waits for you to allow it".to_string())),
        Status::Working => c.tail.last_tool.clone().or_else(|| c.preview.clone()),
        _ => c.tail.last_text.clone().or_else(|| c.preview.clone()),
    };
    // Its terminal, when it's the command line's own UI rather than a print run.
    let terminal = process.filter(|p| p.terminal).map(|p| p.pid);
    let action = match (c.cli, terminal) {
        (false, _) => "antigravity",
        (true, Some(_)) => "focus",
        (true, None) => "resume",
    };
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
        // The preview is the conversation's opening, as the app lists it.
        first_prompt: c.preview.clone(),
        status_since: if status == Status::Working { c.last_input } else { Some(c.last_modified) },
        last_prompt_at: c.last_input.unwrap_or(c.last_modified),
        last_activity: c.last_modified,
        excerpt,
        resume_command: c.cli.then(|| format!("{} --conversation {}", crate::antigravity::cli_command(account), c.id)),
        mode: c.mode.clone(),
        ..known
    }
}

/// A Cursor chat, from its record and the end of its messages. `process` is the command line
/// process that has a command line chat open, when one does, and `app_running` whether the app
/// is. The app's chat is under way while the app says it's generating and the app is there to
/// do it: one generating when the app quit says so until the app opens it again. Something to
/// allow or answer needs you; the command line writes nothing down while its prompt is up, so
/// its chats never show so. A turn that ended on an error shows as one until it's seen.
pub fn cursor_session(account: &AccountCfg, c: &crate::cursor::Chat, process: Option<&crate::cursor::Process>, app_running: bool, seen_at: i64, now: i64) -> Session {
    let under_way = match (c.cli, process) {
        (true, Some(p)) => c.tail.turn_open || !p.terminal,
        (true, None) => false,
        (false, _) => c.generating() && app_running && now - c.last_updated < CURSOR_STALE_MS,
    };
    let waiting = under_way && !c.cli && (c.blocking || c.tail.open.iter().any(|call| call.waits));
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
        status_since: if under_way { Some(last_prompt_at) } else { Some(c.last_updated) },
        last_prompt_at,
        last_activity: c.last_updated,
        excerpt,
        branch: c.branch.clone(),
        model: c.model.clone(),
        effort: c.effort.clone(),
        archived: c.archived,
        resume_command: c.cli.then(|| format!("{} --resume {}", crate::cursor::cli_command(account), c.id)),
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

/// When a session began waiting on you, for one that needs you: the list of those is a queue,
/// the longest wait first, so a new one can't keep pushing an old one down.
pub fn waiting_since(s: &Session) -> Option<i64> {
    matches!(s.status, Status::Needs | Status::Error).then(|| s.status_since.unwrap_or(s.last_activity))
}

/// Most urgent first; those that need you by how long they've waited, the rest by the one you
/// last sent a prompt. Activity would reorder working sessions with every tool call; your
/// prompts only move one when you do something.
fn sort_sessions(sessions: &mut [Session]) {
    sessions.sort_by(|a, b| {
        urgency(a.status)
            .cmp(&urgency(b.status))
            .then(waiting_since(a).cmp(&waiting_since(b)))
            .then(b.last_prompt_at.cmp(&a.last_prompt_at))
            .then(b.last_activity.cmp(&a.last_activity))
    });
}

/// When you last sent a prompt to one of a lane's own sessions; for a lane with none, when
/// anything last happened in it.
fn lane_prompt_at(sessions: &[Session], last_activity: i64) -> i64 {
    // A helper's prompts are its agent's, not yours, and a headless run's its script's.
    sessions.iter().filter(|s| s.visitor.is_none() && !s.archived && !s.unattended()).map(|s| s.last_prompt_at).max().unwrap_or(last_activity)
}

/// How long a lane in Needs you has waited: since the longest wait of its own agents that need
/// you and aren't snoozed. None when it needs you only through agents you've snoozed, which come
/// after the ones waiting.
fn lane_waiting_since(sessions: &[Session]) -> Option<i64> {
    sessions.iter().filter(|s| s.visitor.is_none() && !s.archived && s.snoozed_until.is_none()).filter_map(waiting_since).min()
}

/// Lanes by bay; in Needs you the longest wait first, then those with no agent waiting; within
/// that, and in the other bays, the one you last sent a prompt first.
fn lane_order(a: &Lane, b: &Lane) -> std::cmp::Ordering {
    let waited = |l: &Lane| if l.bay == Bay::Needs { lane_waiting_since(&l.sessions).map_or((1, 0), |t| (0, t)) } else { (0, 0) };
    a.bay
        .cmp(&b.bay)
        .then(waited(a).cmp(&waited(b)))
        .then(lane_prompt_at(&b.sessions, b.last_activity).cmp(&lane_prompt_at(&a.sessions, a.last_activity)))
        .then(b.last_activity.cmp(&a.last_activity))
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
    let ours = |p: &&PrRaw| &p.head_ref == branch && p.head_owner.as_ref().map(|o| seed.head_owners.contains(o)).unwrap_or(true);
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
    })
}

/// Every PR in a repo a project covers, with the worktrees that hold it and the sessions that
/// worked on it.
///
/// A worktree holds the PR the Worktrees view shows on it (`find_pr`), so a long-lived branch
/// that carries one PR after another holds only the latest.
///
/// A session worked on the PR if it was on the PR's branch while the PR was open: active after
/// the PR opened (give or take an hour) and started before it merged or closed. A session
/// placed in a worktree it doesn't run in (`session_seed`) goes by that worktree's branch. A session from
/// another lane records its own folder's branch, so it counts through a worktree on the PR's
/// branch that it made or worked in: the one that made it whenever it started before the PR
/// ended, since it wrote the branch even if the PR was opened later, and one that worked there
/// in the same window as the rest. A PR from the default branch (a fork's `main`) links no
/// sessions by branch: every session on `main` would match.
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
/// lists doesn't wait on you, though an earlier read of the repo said so.
pub fn pr_items(seeds: &[LaneSeed], lanes: &[Lane], prs: &HashMap<String, Vec<PrRaw>>, requested: &HashMap<String, Vec<PrRaw>>, login: Option<&str>, links: &PrLinks) -> Vec<PrItem> {
    let repos = repos_of(seeds);
    let polled = polled_slugs(seeds);
    let mut out = Vec::new();
    for (slug, list) in prs {
        let Some(&repo) = repos.get(slug.as_str()) else { continue };
        let searched = (!polled.contains(slug)).then(|| requested.get(slug).map_or(&[][..], Vec::as_slice));
        for p in list {
            // The search's copy is the newer.
            if searched.is_some_and(|found| found.iter().any(|x| x.number == p.number)) {
                continue;
            }
            let mut item = pr_item(lanes, slug, repo, p, branch_freed(list, p), login, links);
            if searched.is_some() {
                item.review_requested = false;
                item.requested_by = None;
                item.requested_at = None;
            }
            out.push(item);
        }
    }
    for (slug, list) in requested {
        let Some(&repo) = repos.get(slug.as_str()).filter(|_| !polled.contains(slug)) else { continue };
        // The search lists only open PRs; the repo's last read has the ones that ended.
        let history = || prs.get(slug).into_iter().flatten().chain(list);
        out.extend(list.iter().map(|p| pr_item(lanes, slug, repo, p, branch_freed(history(), p), login, links)));
    }
    out.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    out
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
    let found = repos.iter().find(|(s, _)| {
        s.eq_ignore_ascii_case(slug) || renamed.get(&s.to_ascii_lowercase()).is_some_and(|now| now.eq_ignore_ascii_case(slug))
    });
    match found {
        Some((&local, &repo)) => pr_item(lanes, local, repo, p, None, login, links),
        None => pr_item(lanes, slug, ("", slug.rsplit('/').next().unwrap_or(slug), None), p, None, login, links),
    }
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
fn pr_item(lanes: &[Lane], slug: &str, (project, repo, default_branch): (&str, &str, Option<&str>), p: &PrRaw, freed: Option<i64>, login: Option<&str>, links: &PrLinks) -> PrItem {
    let here: Vec<&Lane> = lanes.iter().filter(|l| l.repo_slug.as_deref() == Some(slug)).collect();
    let head = p.head_ref.as_str();
    let (opened, ended) = (p.created_at, p.merged_at.or(p.closed_at));
    let before_end = move |s: &Session| ended.map_or(true, |end| s.started_at.unwrap_or(s.last_activity) <= end);
    let during = move |s: &Session| s.last_activity >= opened - 3_600_000 && before_end(s);
    // How often a session listed under a lane of the repo pushed to the branch in the PR's time,
    // as `during` has it, and after an earlier PR from the branch ended: a branch used for one PR
    // after another (`yolo`) has the earlier ones' pushes too, and one of the same name in another
    // repo its own. A PR from the default branch counts none, as it lists no sessions by branch.
    let by_branch = default_branch != Some(head);
    let since = (opened - 3_600_000).max(freed.map_or(i64::MIN, |t| t + 1));
    let pushes = move |l: &Lane, k: &str| -> u32 {
        let pushed = links.pushed.get(k).filter(|_| by_branch && l.repo_slug.as_deref() == Some(slug)).into_iter().flatten();
        pushed.filter(|(b, at)| b == head && *at >= since && ended.map_or(true, |end| *at <= end)).count() as u32
    };
    let mut sessions: Vec<PrSession> = Vec::new();
    if by_branch {
        // Sessions in their own lanes first, then ones that came to a worktree on the branch.
        let own = here.iter().flat_map(|&l| {
            // A session that works here from another folder records that folder's branch,
            // so it goes by this worktree's.
            let here_branch = move |s: &Session| s.branch.is_none() || !Path::new(&s.cwd).starts_with(&l.id);
            let on_branch = move |s: &Session| s.branch.as_deref() == Some(head) || (here_branch(s) && l.branch.as_deref() == Some(head));
            l.sessions.iter().filter(move |s| s.visitor.is_none() && on_branch(s) && during(s)).map(move |s| (l, s))
        });
        let guests = here.iter().filter(|l| l.branch.as_deref() == Some(head)).flat_map(|&l| {
            let counts = move |s: &Session| match s.visitor.as_deref() {
                None => false,
                Some(MADE_IT) => before_end(s),
                Some(_) => during(s),
            };
            l.sessions.iter().filter(move |s| counts(s)).map(move |s| (l, s))
        });
        let pushers = here.iter().flat_map(|&l| l.sessions.iter().filter(move |s| s.visitor.is_none() && during(s) && pushes(l, &s.key) > 0).map(move |s| (l, s)));
        for (l, s) in own.chain(guests).chain(pushers) {
            if !s.archived && !sessions.iter().any(|x| x.key == s.key) {
                sessions.push(PrSession { key: s.key.clone(), lane: l.id.clone(), pushes: pushes(l, &s.key) });
            }
        }
    }
    let key = crate::prlinks::pr_key(&format!("{slug}#{}", p.number));
    for k in links.worked.get(&key).into_iter().flatten() {
        if sessions.iter().any(|x| &x.key == k) {
            continue;
        }
        // The lane it's in, else one it visited.
        let found = home(lanes, k);
        if let Some((l, s)) = found.filter(|(_, s)| !s.archived) {
            sessions.push(PrSession { key: s.key.clone(), lane: l.id.clone(), pushes: pushes(l, &s.key) });
        }
    }
    sessions.retain(|x| !links.unlinked.contains(&(key.clone(), x.key.clone())));
    let (primary, primary_by) = pr_primary(lanes, &sessions, &key, links).unzip();
    let mine = login.is_some() && p.author.as_deref() == login;
    let by_hand = links.nudged.contains(&key);
    let by_default = links.nudge_by_default_in.get(slug).copied().unwrap_or(links.nudge_by_default);
    let nudge = p.state.is_open() && !links.not_nudged.contains(&key) && (by_hand || (by_default && mine));
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
        review_requested: p.review_requested,
        requested_by: p.requested_by.clone().filter(|_| p.review_requested),
        requested_at: p.requested_at.filter(|_| p.review_requested),
        reviewers_asked: p.reviewers_asked.clone(),
        snoozed_until: None,
        nudge,
        nudge_by_default: nudge && !by_hand,
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
        auto_review: None,
    }
}

/// The PR's primary session, of the ones listed on it, and why: the one picked by hand; else the
/// one that pushed to its branch most often (`PrSession::pushes`); else the one that started
/// first. The one that opened it, whose records show it first (a session that later pushes to
/// it, comments or edits it is marked after), counts as having pushed once at least, since its
/// push may have left no record (a quiet `git push -q`, a harness that records none), and wins a
/// tie: a session that pushes a fix or two to another's PR doesn't take it over, and one that
/// goes on with the work does. Without pushes that's the one that opened it. A copy of a
/// conversation (a fork, a move to another account) carries its marks and pushes with their
/// times, so the one that started first wins a tie. An archived session isn't listed, so the
/// next takes over.
fn pr_primary(lanes: &[Lane], sessions: &[PrSession], key: &str, links: &PrLinks) -> Option<(String, PrimaryBy)> {
    if let Some(k) = links.picked.get(key).filter(|k| sessions.iter().any(|s| &s.key == *k)) {
        return Some((k.clone(), PrimaryBy::Picked));
    }
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
    let t = text.trim().trim_start_matches('…').trim();
    // A sentence ends at a stop followed by a space, so `App.tsx` and `e.g.` don't split one.
    let chars: Vec<(usize, char)> = t.char_indices().collect();
    let cut = chars.windows(2).filter(|w| matches!(w[0].1, '.' | '!' | '?') && w[1].1.is_whitespace()).map(|w| w[1].0).last().unwrap_or(0);
    truncate(t[cut..].trim(), 140)
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
    (parent.is_some() || pr.is_some()).then_some(Helper { parent, pr, report })
}

/// Sessions started in another session's own scratch folder (`Adapter::scratch_owner`: Claude
/// Code's scratchpad, a background job's folder) are that session's helpers, where nothing else
/// says whose they are: its agent ran them there (test chats in tmux, say) and reads them itself,
/// so their ended turns don't wait on you, as a helper's don't. None is told of, since none asked
/// to be (`Helper::report`). Headless runs have their own rule, and a session you made your own
/// (`own`) stays yours. `owner_of` names a folder's session by its harness and ID.
pub fn scratch_helpers(sessions: &mut [Session], owner_of: impl Fn(&Path) -> Option<(Harness, String)>, own: impl Fn(&str) -> bool) {
    let keys: HashMap<(Harness, String), String> = sessions.iter().map(|s| ((s.harness, s.id.clone()), s.key.clone())).collect();
    for s in sessions.iter_mut().filter(|s| s.helper.is_none() && !s.headless && !own(&s.key)) {
        let Some(parent) = owner_of(Path::new(&s.cwd)).and_then(|o| keys.get(&o)).filter(|k| **k != s.key) else { continue };
        s.helper = Some(Helper { parent: Some(parent.clone()), pr: None, report: false });
        s.origin.get_or_insert_with(|| SessionOrigin { key: parent.clone(), how: OriginHow::Started });
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
pub fn apply_helper(s: &mut Session) {
    if s.helper.is_some() && s.status == Status::Unread {
        s.status = Status::Idle;
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

/// Whether replies suggested for a session still answer where it stands: it's been sent nothing
/// since the prompt whose reply the suggester read (`prompt_at`; a prompt of yours, from Codealong
/// or where it runs), and no turn of its has started since they were suggested. A turn that
/// started after them has moved on from what they answered, even once it ends, so the engine
/// forgets them once it sees one (`Engine::forget_stale_suggestions`). Ones from Macs that didn't
/// say what was read go by when they were suggested.
pub fn suggestions_hold(s: &Session, g: &Suggestions) -> bool {
    s.last_prompt_at <= g.prompt_at.unwrap_or(g.at) && !(s.status == Status::Working && s.status_since.is_some_and(|since| since > g.at))
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
        s.status = Status::Needs;
        s.asks = true;
        let question = s.reply_tail.as_deref().map(last_sentence).filter(|q| !q.is_empty());
        s.excerpt = Some(match question {
            Some(q) => format!("Asks you · {q}"),
            None => "Asks you something".to_string(),
        });
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
/// (atlas's `app2`, reset onto `origin/main` after each), though a fresh clone's reflog
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

/// A session's own sentence. See *The model*, Headlines.
pub fn session_headline(s: &Session) -> (String, Option<String>) {
    let acct = code(&s.account);
    if s.stalled_since.is_some() {
        return (format!("{acct} is stuck · {}", s.title), s.excerpt.clone());
    }
    let title = &s.title;
    match s.status {
        // Every headline names the session: its title after what it's doing. A question, whether
        // its reply ends in one or it's held through the plugin (`claude::asked_label`), says so
        // up front and leaves the question itself below.
        Status::Needs if s.asks || s.excerpt.as_deref().is_some_and(|e| e.starts_with("Asks you")) => {
            (format!("{acct} asks you · {title}"), s.excerpt.as_deref().and_then(|e| e.strip_prefix("Asks you · ")).map(str::to_string))
        }
        Status::Needs if s.excerpt.as_deref().is_some_and(|e| e.starts_with(HELD_MESSAGE)) => {
            (format!("{acct} holds a message for you · {title}"), s.excerpt.as_deref().and_then(|e| e.strip_prefix("Held message ")).map(str::to_string))
        }
        Status::Needs => match (&s.excerpt, s.harness) {
            (Some(ex), Harness::Claude) => (format!("{acct} is waiting for approval · {title}"), Some(ex.clone())),
            (Some(ex), Harness::Codex | Harness::Opencode | Harness::Antigravity | Harness::Cursor) => (format!("{acct} asks for approval · {title}"), Some(ex.clone())),
            (None, _) => (format!("{acct} needs you · {title}"), None),
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
    let Inputs { now, apps, seeds, mut sessions, visitors, git, prs, requested, project_order, notices, loading, github_at, pinned, github_login, pr_links, previews, reusable: marks, review_snoozes, auto_reviews } = inp;
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
    for s in sessions {
        if let Some(i) = session_seed(seeds, &index, &s) {
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
        let (mut sessions, archived): (Vec<Session>, Vec<Session>) = sessions.into_iter().partition(|s| !s.archived);
        sort_sessions(&mut sessions);
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
        // A lane outside your projects shows while it needs you; an app's chats while there are
        // any you haven't archived.
        let hidden = seed.elsewhere && bay != Bay::Needs && !(seed.chats && !sessions.is_empty());
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
    let mut pr_list = pr_items(seeds, &lanes, prs, requested, github_login, pr_links);
    apply_review_snoozes(&mut pr_list, review_snoozes, now);
    crate::autoreview::apply(&mut pr_list, auto_reviews.0, auto_reviews.1, auto_reviews.2, &lanes, now);
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
        pinned: pinned.to_vec(),
        pinned_worktrees: Vec::new(),
        pinned_prs: Vec::new(),
        prs: pr_list,
        github_login: github_login.map(str::to_string),
        theme: Default::default(),
        zoom: crate::zoom::ACTUAL,
        perf_log: false,
        todos: Vec::new(),
        todos_unreadable: false,
        labels: Vec::new(),
        quick_shortcut: String::new(),
        waiting_shortcut: String::new(),
        improve_shortcut: String::new(),
        improve: Default::default(),
        jev: Default::default(),
        reviews: Default::default(),
        todo_hints: Default::default(),
        devices: Default::default(),
        project_rules: Vec::new(),
        when_limited: Default::default(),
        nudge_about: Default::default(),
        repos: Default::default(),
        team_files: Vec::new(),
        repo_settings: Default::default(),
    }
}

pub use codealong_peer::model::ELSEWHERE;

/// Match the floating bar's scope: all visible projects, without archived sessions, duplicate
/// visitors, or helpers and headless runs that don't need you. An open review request you
/// haven't snoozed also needs the user's attention.
pub fn logo_status(lanes: &[Lane], prs: &[PrItem]) -> LogoStatus {
    let sessions = lanes.iter().filter(|l| !l.hidden).flat_map(|l| &l.sessions)
        .filter(|s| !s.archived && s.visitor.is_none() && !s.unattended())
        .map(|s| match s.status {
            Status::Working => LogoStatus::Working,
            // Snoozed, it's off the bar's Needs you and Your turn, so off the mark too.
            _ if s.snoozed_until.is_some() => LogoStatus::Idle,
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

/// The folders a new terminal at `path` belongs beside, nearest first: its worktree, the other
/// worktrees of its repo, then the rest of its project. `[apps] new_terminal = "project"` opens
/// it as a tab in a window already showing the nearest of them. A folder elsewhere (a project of
/// its own that isn't one, or an app's chats) has only itself for company.
pub fn terminal_neighbours(lanes: &[Lane], path: &Path) -> Vec<Vec<String>> {
    let Some(home) = home_lane(lanes, path) else {
        return vec![vec![path.to_string_lossy().into_owned()]];
    };
    let mut groups = vec![vec![home.path.clone()]];
    if home.origin == Origin::Other || home.project == ELSEWHERE {
        return groups;
    }
    let others = |same: &dyn Fn(&Lane) -> bool| -> Vec<String> {
        lanes.iter().filter(|l| l.path != home.path && l.origin != Origin::Other && same(l)).map(|l| l.path.clone()).collect()
    };
    let repo = others(&|l| l.project == home.project && l.repo == home.repo);
    let project = others(&|l| l.project == home.project && l.repo != home.repo);
    groups.extend([repo, project].into_iter().filter(|g| !g.is_empty()));
    groups
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
        AccountCfg { name: name.into(), harness: "claude".into(), home: format!("~/.{name}"), command: Some(name.into()), start: None, preferred: false, acp: None, monthly: None, per_token: false, disabled: false }
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
        }
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
        // its own word: the search's copy of #1037 doesn't count there.
        let prs = HashMap::from([
            ("sprucehq/spark".to_string(), vec![pr(PrState::Open, 0)]),
            ("sprucehq/site".to_string(), vec![asked(5), PrRaw { title: "Old title".into(), ..asked(7) }]),
        ]);
        let requested = HashMap::from([
            ("sprucehq/site".to_string(), vec![asked(7), asked(9)]),
            ("sprucehq/spark".to_string(), vec![asked(1037)]),
            ("acme/elsewhere".to_string(), vec![asked(3)]),
        ]);
        let mut got: Vec<(String, bool, String)> = pr_items(&seeds, &[], &prs, &requested, None, &PrLinks::default()).into_iter().map(|p| (p.key, p.review_requested, p.project)).collect();
        got.sort();
        let want = [("sprucehq/site#5", false), ("sprucehq/site#7", true), ("sprucehq/site#9", true), ("sprucehq/spark#1037", false)];
        assert_eq!(got, want.map(|(k, r)| (k.to_string(), r, "Spruce".to_string())));
        let items = pr_items(&seeds, &[], &prs, &requested, None, &PrLinks::default());
        assert_eq!(items.iter().find(|p| p.number == 7).map(|p| p.title.as_str()), Some("Let the agent add and caption project photos"));
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
            auto_reviews: (&Default::default(), &Default::default(), &Default::default()),
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
            auto_reviews: (&Default::default(), &Default::default(), &Default::default()),
        });
        assert_eq!(board.prs.len(), 1);
        let p = &board.prs[0];
        assert_eq!(p.key, "sprucehq/spark#1037");
        assert!(p.mine);
        assert_eq!(p.lanes, vec!["/r/spark-portal".to_string()]);
        assert_eq!(p.sessions, vec![PrSession { key: on_branch.key, lane: "/r/spark-portal".into(), pushes: 0 }]);
        assert_eq!(board.github_login.as_deref(), Some("gwen"));
    }

    #[test]
    fn a_pr_the_board_doesnt_list_is_matched_in_the_project_repo_a_link_names() {
        let seeds = vec![seed("/r/spark-portal", "portal")];
        let a = acct("claude2");
        let mut maker = claude_session(&a, Some(&reg("idle", NOW)), Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
        maker.branch = Some("app4".into());
        let (git, visitors, prs) = (HashMap::new(), HashMap::new(), HashMap::new());
        let order = vec!["Spruce".to_string()];
        let board = compose(Inputs { now: NOW, apps: &AppNames::default(), seeds: &seeds, sessions: vec![maker.clone()], visitors: &visitors, git: &git, prs: &prs, requested: &HashMap::new(), project_order: &order, notices: vec![], loading: false, github_at: None, pinned: &[], github_login: None, pr_links: &PrLinks::default(), previews: &Previews::default(), reusable: &HashMap::new(), review_snoozes: &[], auto_reviews: (&Default::default(), &Default::default(), &Default::default()) });
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
    fn a_prs_primary_is_the_one_picked_else_the_one_that_opened_it_else_the_first_started() {
        let s = |key: &str, started: i64| Session { started_at: Some(started), ..crate::fixtures::session(key) };
        let lanes = vec![crate::fixtures::lane("/w", vec![s("a", 30), s("b", 10), s("c", 20), s("copy", 40)])];
        let listed = |keys: &[&str]| keys.iter().map(|k| PrSession { key: (*k).into(), lane: "/w".into(), pushes: 0 }).collect::<Vec<_>>();
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
    }

    /// northwindai/atlas#1203: one session opened a small PR from its worktree, with a quiet
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
            let items = pr_items(seeds, &lanes, &prs, &requested, Some("gwen"), &reused);
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
        let pr_in = |state: PrState| Pr { number: 1, title: String::new(), url: String::new(), state, checks: Checks::default(), merge: Merge::Unknown, updated_at: 0, merged_at: None, closed_at: None };
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
                auto_reviews: (&Default::default(), &Default::default(), &Default::default()),
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
        let prs = HashMap::from([("sprucehq/spark".to_string(), vec![pr(PrState::Open, 0)])]);
        let (git, visitors) = (HashMap::new(), HashMap::new());
        let order = vec!["Spruce".to_string()];
        let board = compose(Inputs {
            now: NOW,
            apps: &apps,
            seeds: &seeds,
            sessions: vec![s.clone()],
            visitors: &visitors,
            git: &git,
            prs: &prs,
            requested: &HashMap::new(),
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
            auto_reviews: (&Default::default(), &Default::default(), &Default::default()),
        });
        assert_eq!(board.prs[0].sessions, vec![PrSession { key: s.key, lane: "/r/spark-portal".into(), pushes: 0 }]);
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
            auto_reviews: (&Default::default(), &Default::default(), &Default::default()),
        });
        let get = |n: u64| board.prs.iter().find(|p| p.number == n).unwrap();
        let listed = |key: &str| PrSession { key: key.into(), lane: "/r/spark-portal".into(), pushes: 0 };
        assert_eq!(get(1037).lanes, vec!["/r/spark-portal".to_string()]);
        assert_eq!(get(1037).sessions, vec![listed(&latest.key)]);
        assert!(get(1015).lanes.is_empty(), "the worktree holds the newer PR, as the Worktrees view shows");
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
                auto_reviews: (&Default::default(), &Default::default(), &Default::default()),
            })
        };
        let listed = |key: &str| PrSession { key: key.into(), lane: scratch.into(), pushes: 0 };
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

    /// `/color`'s names, as the emblem's colors have them: cyan is teal, and no amber.
    #[test]
    fn the_color_it_was_given_is_an_emblem_color() {
        let a = acct("claude");
        let s = |color: Option<&str>| claude_session(&a, Some(&reg("idle", NOW)), Some(&TranscriptInfo { color: color.map(String::from), ..tr() }), 0, NOW, &Quiet::default()).unwrap().color;
        assert_eq!((s(Some("cyan")), s(Some("orange")), s(Some("mauve")), s(None)), (Some("teal".into()), Some("red".into()), None, None));
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
    fn a_pin_from_another_device_takes_only_sessions_listed_and_not_archived() {
        let archived = Session { archived: true, ..crate::fixtures::session("claude:claude:old") };
        let lanes = vec![crate::fixtures::lane("/a", vec![crate::fixtures::session("claude:claude:a"), archived])];
        let keys: Vec<String> = vec!["claude:claude:old".into(), "claude:claude:a".into(), "claude:claude:gone".into(), String::new()];
        assert_eq!(pinnable(&lanes, &keys, true), [&keys[1]]);
        // Any key comes off.
        assert_eq!(pinnable(&lanes, &keys, false).len(), 4);
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
        let at = |active_at| Quiet { subagent_at: None, running: Some(login), background_active_at: Some(active_at) };
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
        let a = AccountCfg { name: "codex".into(), harness: "codex".into(), home: "~/.codex".into(), command: None, start: None, preferred: false, acp: None, monthly: None, per_token: false, disabled: false };
        let mut th = ThreadInfo { id: "01a0c4a6".into(), cwd: "/r".into(), last_ts: NOW - 1_000, turn_open: true, turn_started_at: Some(NOW - 5_000), ..Default::default() };
        assert_eq!(codex_session(&a, &th, Some("Photo tools"), None, None, 0, NOW).status, Status::Working);
        assert_eq!(codex_session(&a, &th, None, None, None, 0, NOW).open_target, "s|codex:codex:01a0c4a6|codex");
        assert!(!codex_session(&a, &th, None, None, None, 0, NOW).headless);
        assert!(codex_session(&a, &ThreadInfo { exec: true, ..th.clone() }, None, None, None, 0, NOW).headless, "a `codex exec` run");
        assert!(!codex_session(&a, &ThreadInfo { exec: true, ..th.clone() }, None, Some(42), None, 0, NOW).headless, "resumed in a terminal");
        th.last_ts = NOW - CODEX_STALE_MS - 1;
        assert_eq!(codex_session(&a, &th, None, None, None, 0, NOW).status, Status::Idle);
        th.turn_open = false;
        th.last_complete_at = Some(NOW - 1_000);
        assert_eq!(codex_session(&a, &th, None, None, None, NOW - 2_000, NOW).status, Status::Unread);
        assert_eq!(codex_session(&a, &th, None, None, None, NOW, NOW).status, Status::Idle);
        th.pending_approval = Some("Run · `yarn install`".into());
        let asks = codex_session(&a, &th, None, None, None, 0, NOW);
        // It waits from its request, not from the turn before it.
        assert_eq!((asks.status, asks.status_since), (Status::Needs, Some(th.last_ts)));
        // A computer-use call that hasn't answered in a minute is waiting for approval.
        th.pending_approval = None;
        th.turn_open = true;
        th.last_ts = NOW - 30_000;
        th.pending_calls = vec![crate::codex::PendingCall { call_id: "c".into(), at: NOW - 30_000, what: "Inspect the Dock".into(), limit_ms: 60_000 }];
        assert_eq!(codex_session(&a, &th, None, None, None, 0, NOW).status, Status::Working);
        th.pending_calls[0].at = NOW - 61_000;
        let waiting = codex_session(&a, &th, None, None, None, 0, NOW);
        assert_eq!((waiting.status, waiting.status_since), (Status::Needs, Some(NOW - 61_000)));
        assert_eq!(waiting.excerpt.as_deref(), Some("Waiting for approval? · Inspect the Dock"));
    }

    #[test]
    fn a_codex_turn_no_process_holds_was_cut_off() {
        let a = AccountCfg { name: "codex".into(), harness: "codex".into(), home: "~/.codex".into(), command: None, start: None, preferred: false, acp: None, monthly: None, per_token: false, disabled: false };
        let th = ThreadInfo {
            id: "01a1315d".into(),
            cwd: "/r".into(),
            last_ts: NOW - 30_000,
            turn_open: true,
            turn_started_at: Some(NOW - 60_000),
            cli: true,
            last_user_message: Some("Run the slow test".into()),
            last_agent_message: Some("Done earlier.".into()),
            pending_calls: vec![crate::codex::PendingCall { call_id: "c".into(), at: NOW - 30_000, what: "Run · `sleep 120`".into(), limit_ms: 1_000 }],
            ..Default::default()
        };
        let cut = codex_session(&a, &th, None, None, Some(false), 0, NOW);
        assert_eq!((cut.cut_off, cut.status, cut.live, cut.status_since), (Some(NOW - 30_000), Status::Error, false, Some(NOW - 30_000)));
        // What it was on, and no guess that it waits for an approval.
        assert_eq!(cut.excerpt.as_deref(), Some("Run the slow test"));
        // Seen since: it stays cut off, as news no longer.
        let seen = codex_session(&a, &th, None, None, Some(false), NOW, NOW);
        assert_eq!((seen.cut_off, seen.status), (Some(NOW - 30_000), Status::Idle));
        // Held open, in a terminal, or not known: it may still be going.
        for (terminal, held) in [(None, Some(true)), (Some(42), Some(false)), (None, None)] {
            assert_eq!(codex_session(&a, &th, None, terminal, held, 0, NOW).cut_off, None, "{terminal:?} {held:?}");
        }
        // A turn that ended isn't cut off, held or not.
        let ended = ThreadInfo { turn_open: false, pending_calls: Vec::new(), ..th };
        assert_eq!(codex_session(&a, &ended, None, None, Some(false), 0, NOW).cut_off, None);
    }

    #[test]
    fn a_codex_cli_thread_opens_in_its_terminal() {
        let a = AccountCfg { name: "codex2".into(), harness: "codex".into(), home: "~/.codex2".into(), command: Some("CODEX_HOME=~/.codex2 codex".into()), start: None, preferred: false, acp: None, monthly: None, per_token: false, disabled: false };
        let th = ThreadInfo { id: "01a0e375".into(), cwd: "/r".into(), last_ts: NOW - 1_000, cli: true, ..Default::default() };
        let open = codex_session(&a, &th, None, Some(42), None, 0, NOW);
        assert_eq!((open.surface, open.live, open.pid, open.open_target.as_str()), (Surface::Cli, true, Some(42), "s|codex:codex2:01a0e375|focus"));
        let closed = codex_session(&a, &th, None, None, None, 0, NOW);
        assert_eq!((closed.surface, closed.live, closed.open_target.as_str()), (Surface::Cli, false, "s|codex:codex2:01a0e375|resume"));
        assert_eq!(closed.resume_command.as_deref(), Some("CODEX_HOME=~/.codex2 codex resume 01a0e375"));
        assert!(!closed.app_account, "the Codex app runs ~/.codex only");
        // An app thread resumed in a terminal shows there while it's open.
        let app = ThreadInfo { cli: false, ..th };
        assert_eq!(codex_session(&a, &app, None, Some(7), None, 0, NOW).surface, Surface::Cli);
        assert_eq!(codex_session(&a, &app, None, None, None, 0, NOW).surface, Surface::App);
    }

    #[test]
    fn maps_opencode_statuses() {
        let a = AccountCfg { name: "opencode".into(), harness: "opencode".into(), home: "~/.local/share/opencode".into(), command: None, start: None, preferred: false, acp: None, monthly: None, per_token: false, disabled: false };
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
        let a = AccountCfg { name: "antigravity".into(), harness: "antigravity".into(), home: "~/.gemini/antigravity".into(), command: None, start: None, preferred: false, acp: None, monthly: None, per_token: false, disabled: false };
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
        let a = AccountCfg { name: "antigravity".into(), harness: "antigravity".into(), home: "~/.gemini/antigravity".into(), command: None, start: None, preferred: false, acp: None, monthly: None, per_token: false, disabled: false };
        let c = Conversation { id: "c2".into(), cli: true, preview: Some("Fix the build".into()), status: "cascade_run_status_running".into(), not_fully_idle: true, last_modified: NOW - 2 * ANTIGRAVITY_STALE_MS, last_input: Some(NOW - 3 * ANTIGRAVITY_STALE_MS), ..Default::default() };
        // In its terminal UI: it works however long it's been quiet, and its tab is where it opens.
        let ui = Process { pid: 41, conversation: Some("c2".into()), terminal: true };
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

    #[test]
    fn maps_cursor_statuses() {
        use crate::cursor::{Call, Chat};
        let a = AccountCfg { name: "cursor".into(), harness: "cursor".into(), home: "~/.cursor".into(), command: None, start: None, preferred: false, acp: None, monthly: None, per_token: false, disabled: false };
        // The app's chat, generating while the app is up.
        let mut c = Chat { id: "c1".into(), title: Some("Fix the build".into()), cwd: Some("/r".into()), status: "generating".into(), created_at: NOW - 9_000, last_updated: NOW - 1_000, model: Some("gpt-5.5".into()), branch: Some("fix".into()), messages: 4, ..Default::default() };
        c.tail.last_tool = Some("Run · `cargo test`".into());
        c.tail.last_text = Some("The tests pass now.".into());
        c.tail.last_prompt_at = Some(NOW - 5_000);
        let s = cursor_session(&a, &c, None, true, 0, NOW);
        assert_eq!((s.status, s.live, s.surface, s.excerpt.as_deref()), (Status::Working, true, Surface::App, Some("Run · `cargo test`")));
        assert_eq!((s.open_target.as_str(), s.resume_command.as_deref(), s.status_since), ("s|cursor:cursor:c1|cursor", None, Some(NOW - 5_000)));
        assert_eq!((s.model.as_deref(), s.branch.as_deref(), s.cwd.as_str()), (Some("gpt-5.5"), Some("fix"), "/r"));
        // With the app quit, or nothing written for a long while, nothing runs it, whatever the
        // app left saying.
        assert_eq!(cursor_session(&a, &c, None, false, 0, NOW).status, Status::Unread);
        c.last_updated = NOW - CURSOR_STALE_MS - 1;
        assert!(!cursor_session(&a, &c, None, true, 0, NOW).live);
        c.last_updated = NOW - 1_000;
        // A call waiting to be allowed needs you, and says what waits.
        c.blocking = true;
        c.tail.open = vec![Call { label: "Run · `rm -rf target`".into(), open: true, waits: true, ..Default::default() }];
        let s = cursor_session(&a, &c, None, true, 0, NOW);
        assert_eq!((s.status, s.excerpt.as_deref()), (Status::Needs, Some("Run · `rm -rf target`")));
        assert_eq!(session_headline(&s).0, format!("`cursor` asks for approval · {}", s.title));
        // Waiting is only while it runs: an aborted chat often keeps the app's flag.
        c.status = "aborted".into();
        assert_eq!(cursor_session(&a, &c, None, true, NOW, NOW).status, Status::Idle);
        c.blocking = false;
        c.tail.open.clear();
        // Done: its reply is news until you've seen it; an error says so until then.
        c.status = "completed".into();
        let s = cursor_session(&a, &c, None, true, NOW - 2_000, NOW);
        assert_eq!((s.status, s.live, s.excerpt.as_deref()), (Status::Unread, false, Some("The tests pass now.")));
        assert_eq!(cursor_session(&a, &c, None, true, NOW, NOW).status, Status::Idle);
        c.tail.error = Some("Connection failed".into());
        let s = cursor_session(&a, &c, None, true, NOW - 2_000, NOW);
        assert_eq!((s.status, s.excerpt.as_deref()), (Status::Error, Some("Connection failed")));
        c.archived = true;
        assert!(cursor_session(&a, &c, None, true, 0, NOW).archived);
    }

    #[test]
    fn a_cursor_command_line_chat_runs_while_its_process_has_it() {
        use crate::cursor::{Chat, Process};
        let a = AccountCfg { name: "cursor".into(), harness: "cursor".into(), home: "~/.cursor".into(), command: None, start: None, preferred: false, acp: None, monthly: None, per_token: false, disabled: false };
        let mut c = Chat { id: "c2".into(), cli: true, title: Some("Fix the build".into()), created_at: NOW - 9_000, last_updated: NOW - 2_000, ..Default::default() };
        c.tail.turn_open = true;
        let ui = Process { pid: 41, chat: Some("c2".into()), terminal: true };
        let s = cursor_session(&a, &c, Some(&ui), false, 0, NOW);
        assert_eq!((s.status, s.live, s.surface, s.pid), (Status::Working, true, Surface::Cli, Some(41)));
        assert_eq!((s.open_target.as_str(), s.resume_command.as_deref()), ("s|cursor:cursor:c2|focus", Some("cursor-agent --resume c2")));
        // At its prompt, its turn over: still running, and yours to read.
        c.tail.turn_open = false;
        let s = cursor_session(&a, &c, Some(&ui), false, 0, NOW);
        assert_eq!((s.status, s.live), (Status::Unread, true));
        // A print run has no terminal to show, and works until it ends.
        let print = Process { terminal: false, ..ui.clone() };
        let s = cursor_session(&a, &c, Some(&print), false, 0, NOW);
        assert_eq!((s.status, s.pid, s.open_target.as_str()), (Status::Working, None, "s|cursor:cursor:c2|resume"));
        // Gone: what it left is yours to read, and a new terminal resumes it.
        let s = cursor_session(&a, &c, None, true, 0, NOW);
        assert_eq!((s.status, s.live, s.open_target.as_str()), (Status::Unread, false, "s|cursor:cursor:c2|resume"));
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
    fn an_apps_chats_show_like_worktrees_and_other_folders_elsewhere_while_they_need_you() {
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
            let board = compose(Inputs { now: NOW, apps: &apps, seeds: &seeds, sessions, visitors: &visitors, git: &git, prs: &prs, requested: &HashMap::new(), project_order: &order, notices: vec![], loading: false, github_at: None, pinned: &[], github_login: None, pr_links: &PrLinks::default(), previews: &Previews::default(), reusable: &HashMap::new(), review_snoozes: &[], auto_reviews: (&Default::default(), &Default::default(), &Default::default()) });
            let lane = |p: &str| board.lanes.iter().find(|l| l.path == p).unwrap().clone();
            assert!(!lane(root).looks_finished);
            assert!(lane(root).chats && !lane("/Users/me/notes").chats);
            (lane(root).hidden, lane("/Users/me/notes").hidden)
        };
        let notes = session_in("notes", "/Users/me/notes", "idle");
        assert_eq!(hidden(vec![chat.clone(), notes]), (false, true), "a chat whose turn ended shows; a folder elsewhere waits until it needs you");
        let archived = Session { archived: true, ..chat };
        let waiting = session_in("notes", "/Users/me/notes", "waiting");
        assert_eq!(hidden(vec![archived, waiting]), (true, false), "archived chats don't hold the lane up");
    }

    #[test]
    fn codealongs_chats_wait_hidden_and_make_no_project_until_a_chat_starts() {
        let apps = AppNames::default();
        let root = "/Users/me/Library/Application Support/Codealong/chats";
        let chats = LaneSeed { project: ELSEWHERE.into(), origin: Origin::Folder, elsewhere: true, chats: true, starts_chats: true, ..seed(root, "main") };
        let seeds = vec![seed("/r/spark", "main"), chats];
        let (git, prs, visitors) = (HashMap::new(), HashMap::new(), HashMap::new());
        let order = vec!["Spruce".to_string()];
        let board = |sessions: Vec<Session>| compose(Inputs { now: NOW, apps: &apps, seeds: &seeds, sessions, visitors: &visitors, git: &git, prs: &prs, requested: &HashMap::new(), project_order: &order, notices: vec![], loading: false, github_at: None, pinned: &[], github_login: None, pr_links: &PrLinks::default(), previews: &Previews::default(), reusable: &HashMap::new(), review_snoozes: &[], auto_reviews: (&Default::default(), &Default::default(), &Default::default()) });
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
        let board = compose(Inputs { now: NOW, apps: &apps, seeds: &seeds, sessions: vec![running], visitors: &visitors, git: &git, prs: &prs, requested: &HashMap::new(), project_order: &order, notices: vec![], loading: false, github_at: None, pinned: &[], github_login: None, pr_links: &PrLinks::default(), previews: &Previews::default(), reusable: &HashMap::new(), review_snoozes: &[], auto_reviews: (&Default::default(), &Default::default(), &Default::default()) });
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
        let board = compose(Inputs { now: NOW, apps: &apps, seeds: &seeds, sessions: vec![s], visitors: &visitors, git: &git, prs: &prs, requested: &HashMap::new(), project_order: &order, notices: vec![], loading: false, github_at: None, pinned: &[], github_login: None, pr_links: &PrLinks::default(), previews: &Previews::default(), reusable: &HashMap::new(), review_snoozes: &[], auto_reviews: (&Default::default(), &Default::default(), &Default::default()) });
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
        let board = |h: Session| compose(Inputs { now: NOW, apps: &apps, seeds: &seeds, sessions: vec![h, boss.clone()], visitors: &visitors, git: &git, prs: &prs, requested: &HashMap::new(), project_order: &order, notices: vec![], loading: false, github_at: None, pinned: &[], github_login: None, pr_links: &PrLinks::default(), previews: &Previews::default(), reusable: &HashMap::new(), review_snoozes: &[], auto_reviews: (&Default::default(), &Default::default(), &Default::default()) });
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
        assert_eq!(helper_of(Some(("claude:claude:boss", true)), here, None), Some(Helper { parent: Some("claude:claude:boss".into()), pr: None, report: true }));
        // The session that started it is gone: it's yours again.
        assert_eq!(helper_of(Some(("claude:claude:gone", true)), here, None), None);
        assert_eq!(helper_of(None, here, Some("o/r#7")), Some(Helper { parent: None, pr: Some("o/r#7".into()), report: false }));
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
            Session { helper: Some(Helper { parent: Some("claude:claude:other".into()), pr: None, report: true }), ..at("asked", JOB) },
            Session { headless: true, ..at("run", JOB) },
            at("mine", JOB),
            at("elsewhere", "/r/codealong"),
        ];
        scratch_helpers(&mut sessions, owner_of, |k| k == "claude:claude:mine");
        let boss = "claude:claude:54a9a7a0-b633";
        let helps = |id: &str| sessions.iter().find(|s| s.id == id).and_then(|s| s.helper.as_ref()).map(|h| (h.parent.clone().unwrap_or_default(), h.report));
        assert_eq!(helps("test-chat"), Some((boss.to_string(), false)));
        assert_eq!(sessions[1].origin, Some(SessionOrigin { key: boss.into(), how: OriginHow::Started }));
        assert_eq!(helps("asked"), Some(("claude:claude:other".to_string(), true)));
        assert_eq!((helps("run"), helps("mine"), helps("elsewhere"), helps("54a9a7a0-b633")), (None, None, None, None));
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
        let of = |id: &str, parent: Option<&str>| Session { helper: parent.map(|p| Helper { parent: Some(p.into()), pr: None, report: false }), ..Session::new(Harness::Claude, "claude", id) };
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
    fn builds_targets_with_app_names() {
        let apps = AppNames { terminal: "Ghostty".into(), terminal_id: "ghostty".into(), editor: "Cursor".into(), editor_id: "cursor".into(), ..Default::default() };
        let a = acct("claude2");
        let live = claude_session(&a, Some(&reg("waiting", NOW)), Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
        let (t, default) = targets(&seed("/r/spark-portal", "portal"), &[live], None, None, &apps);
        assert_eq!(t[0].label, "Respond in Ghostty");
        assert_eq!(default.as_deref(), Some("s|claude:claude2:abc1234567|focus"));
        assert!(t.iter().all(|x| !x.id.ends_with("|claude-app")), "claude2 isn't the Claude app's account");
        assert!(t.iter().any(|x| x.label == "Open in Cursor"));
        let main = AccountCfg { name: "claude".into(), harness: "claude".into(), home: "~/.claude".into(), command: None, start: None, preferred: false, acp: None, monthly: None, per_token: false, disabled: false };
        let mine = claude_session(&main, Some(&reg("waiting", NOW)), Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
        let (t, _) = targets(&seed("/r/spark-portal", "portal"), &[mine], None, None, &apps);
        assert!(t[1].disabled && t[1].id.ends_with("|claude-app"), "live elsewhere, so offered but disabled");
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
        let board = compose(Inputs { now: NOW, apps: &apps, seeds: &seeds, sessions: vec![maker], visitors: &visitors, git: &git, prs: &prs, requested: &HashMap::new(), project_order: &order, notices: vec![], loading: false, github_at: None, pinned: &[], github_login: None, pr_links: &PrLinks::default(), previews: &Previews::default(), reusable: &HashMap::new(), review_snoozes: &[], auto_reviews: (&Default::default(), &Default::default(), &Default::default()) });
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
            auto_reviews: (&Default::default(), &Default::default(), &Default::default()),
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
            auto_reviews: (&Default::default(), &Default::default(), &Default::default()),
        });
        let lane = &board.lanes[0];
        assert_eq!(lane.sessions.len(), MAX_SESSIONS_PER_LANE + 2);
        assert!(lane.sessions.iter().any(|s| s.key == oldest));
        assert!(lane.sessions.iter().any(|s| s.key == running));
        assert_eq!(board.pinned, pinned);
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
            auto_reviews: (&Default::default(), &Default::default(), &Default::default()),
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
        let board = compose(Inputs { now: NOW, apps: &apps, seeds: &seeds, sessions: vec![], visitors: &visitors, git: &git, prs: &prs, requested: &HashMap::new(), project_order: &order, notices: vec![], loading: false, github_at: None, pinned: &[], github_login: None, pr_links: &PrLinks::default(), previews: &Previews::default(), reusable: &HashMap::new(), review_snoozes: &[], auto_reviews: (&Default::default(), &Default::default(), &Default::default()) });
        let near = |p: &str| terminal_neighbours(&board.lanes, Path::new(p));
        assert_eq!(near("/r/spark-pins/src"), vec![vec!["/r/spark-pins".to_string()], vec!["/r/spark".to_string()], vec!["/r/spur".to_string()]]);
        assert_eq!(near("/tmp/scratch"), vec![vec!["/tmp/scratch".to_string()]]);
        assert_eq!(near("/Users/me/Documents/Codex/2026-09-27/can-you"), vec![vec!["/Users/me/Documents/Codex".to_string()]], "other folders elsewhere aren't company");
        assert_eq!(near("/Users/me"), vec![vec!["/Users/me".to_string()]]);
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
}
