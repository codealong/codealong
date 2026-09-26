//! The rules from *The model*: session status, lane bays, headlines and open targets.
//! Pure functions over already-read data, so every rule is unit-tested here.

use std::cmp::Reverse;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use crate::antigravity::Conversation;
use crate::claude::{RegistryEntry, TranscriptInfo};
use crate::codex::ThreadInfo;
use crate::opencode::{Live, SessionRow};
use crate::config::AccountCfg;
use crate::github::PrRaw;
use crate::model::*;
use crate::util::{code, one_line, tilde, truncate, DAY_MS};

/// A Codex turn with no events for this long is treated as interrupted.
const CODEX_STALE_MS: i64 = 15 * 60 * 1000;
/// An OpenCode run with no writes for this long is treated as interrupted, when its service
/// isn't there to say.
const OPENCODE_STALE_MS: i64 = 15 * 60 * 1000;
/// An Antigravity conversation that says it's busy but hasn't changed for this long isn't.
const ANTIGRAVITY_STALE_MS: i64 = 30 * 60 * 1000;
/// A Bash call can't run past 10 minutes, so a Claude session that has said busy this long with
/// nothing new in its transcript is stuck, not working: typically a command waiting on input
/// no one will give it, like a browser sign-in inside a background job.
pub const CLAUDE_STALL_MS: i64 = 30 * 60 * 1000;

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
}
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
    /// Lanes for sessions outside every project; hidden unless they need you.
    pub elsewhere: bool,
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
    pub project_order: &'a [String],
    pub warnings: Vec<String>,
    pub loading: bool,
    pub github_at: Option<i64>,
    /// Pinned session keys, in order; they're never cut from a lane's list.
    pub pinned: &'a [String],
    /// Who `gh` is signed in as, to tell your PRs from others'.
    pub github_login: Option<&'a str>,
}

fn plain(s: &str) -> String {
    s.replace('`', "'")
}

fn short_id(id: &str) -> String {
    id.chars().take(7).collect()
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
    // The registry doesn't refresh while a session is busy; its transcript, status changes and
    // subagent logs do. Stuck mid-turn, it can't go on without you.
    let stalled_since = claude_busy_since(reg, tr)
        .map(|t| t.max(quiet.subagent_at.unwrap_or(0)))
        .filter(|t| now - t > CLAUDE_STALL_MS);
    let background = reg.is_some_and(|r| r.status == "shell");
    let status = match reg {
        None => Status::Stopped,
        Some(_) if stalled_since.is_some() => Status::Needs,
        Some(r) => match r.status.as_str() {
            "busy" | "shell" => Status::Working,
            "waiting" | "blocked" => Status::Needs,
            "idle" if r.status_updated_at.unwrap_or(0) > seen_at => Status::Unread,
            _ => Status::Idle,
        },
    };
    let entrypoint = reg.and_then(|r| r.entrypoint.clone()).or_else(|| tr.and_then(|t| t.entrypoint.clone())).unwrap_or_default();
    let surface = if entrypoint.contains("desktop") { Surface::App } else { Surface::Cli };
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
        Status::Needs => tr.and_then(|t| t.pending_tool.clone()),
        Status::Working if background => Some(match quiet.running {
            Some(cmd) => format!("Working in the background · {}", code(&truncate(&one_line(cmd, 200), 80))),
            None => "Working in the background".to_string(),
        }),
        Status::Working => tr.and_then(|t| t.last_tool.clone()),
        _ => tr.and_then(|t| t.last_text.clone().or_else(|| t.last_prompt.clone())),
    };
    let key = format!("claude:{}:{}", account.name, id);
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
    Some(Session {
        open_target: format!("s|{key}|{action}"),
        visitor: None,
        headline: String::new(),
        detail: None,
        key,
        short_id: short_id(&id),
        resume_command: Some(format!("{} --resume {}", account.command(), id)),
        id,
        harness: Harness::Claude,
        account: account.name.clone(),
        surface,
        status,
        live: reg.is_some(),
        pid: reg.map(|r| r.pid),
        title: truncate(&plain(&title), 90),
        cwd,
        work: tr.map(|t| t.work.clone()).unwrap_or_default(),
        reply_tail: tr.and_then(|t| t.last_text_tail.clone()),
        first_prompt: tr.and_then(|t| t.first_prompt.clone()),
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
        archived: false,
        attach_command,
        stalled_since,
        background: background.then(|| quiet.running.unwrap_or_default().to_string()),
        app_account: account.home_path() == crate::util::home().join(".claude"),
        insight: None,
    })
}

/// A tool call in an open Codex turn that has gone unanswered for longer than its tool takes:
/// the app is almost certainly waiting for you to approve it, and doesn't record that. A day
/// on, the app has likely quit without closing the turn.
fn codex_unanswered(th: &ThreadInfo, now: i64) -> Option<String> {
    let call = th.pending_calls.iter().find(|c| now - c.at > c.limit_ms)?;
    (th.turn_open && now - th.last_ts < DAY_MS).then(|| format!("Waiting for approval? · {}", call.what))
}

/// A Codex thread. Threads have no process of their own, so they are never Stopped.
pub fn codex_session(account: &AccountCfg, th: &ThreadInfo, name: Option<&str>, seen_at: i64, now: i64) -> Session {
    let fresh = now - th.last_ts < CODEX_STALE_MS;
    let unanswered = codex_unanswered(th, now);
    let status = if th.pending_approval.is_some() || unanswered.is_some() {
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
        Status::Working => th.last_user_message.clone(),
        _ => th.last_agent_message.clone(),
    };
    let key = format!("codex:{}:{}", account.name, th.id);
    Session {
        open_target: format!("s|{key}|codex"),
        visitor: None,
        headline: String::new(),
        detail: None,
        key,
        id: th.id.clone(),
        short_id: short_id(&th.id),
        harness: Harness::Codex,
        account: account.name.clone(),
        surface: Surface::App,
        status,
        live: th.turn_open && fresh,
        pid: None,
        title: truncate(&plain(&title), 90),
        cwd: th.cwd.clone(),
        work: th.work.clone(),
        reply_tail: th.last_agent_tail.clone(),
        first_prompt: None,
        name: None,
        started_at: th.started_at,
        status_since: if status == Status::Working { th.turn_started_at } else { th.last_complete_at },
        last_prompt_at: th.last_prompt_at.or(th.started_at).unwrap_or(th.last_ts),
        last_activity: th.last_ts,
        excerpt,
        resume_command: None,
        app_account: false,
        branch: th.branch.clone(),
        archived: false,
        attach_command: None,
        stalled_since: None,
        background: None,
        insight: None,
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
        Status::Needs => pending,
        Status::Working => row.last_tool.clone().or_else(|| row.last_prompt.clone()),
        _ => row.last_text.clone().or_else(|| row.last_prompt.clone()),
    };
    let key = format!("opencode:{}:{}", account.name, row.id);
    let action = if terminal.is_some() { "focus" } else { "resume" };
    Session {
        open_target: format!("s|{key}|{action}"),
        visitor: None,
        headline: String::new(),
        detail: None,
        key,
        id: row.id.clone(),
        short_id: short_id(row.id.trim_start_matches("ses_")),
        harness: Harness::Opencode,
        account: account.name.clone(),
        surface: Surface::Cli,
        status,
        live: running || terminal.is_some(),
        pid: terminal,
        title: truncate(&plain(&title), 90),
        cwd: row.directory.clone(),
        work: Vec::new(),
        reply_tail: row.last_text_tail.clone(),
        first_prompt: None,
        name: None,
        started_at: Some(row.created_at),
        status_since: if running { row.last_prompt_at } else { row.idle_at },
        last_prompt_at: row.last_prompt_at.unwrap_or(row.created_at),
        last_activity: row.last_ts,
        excerpt,
        resume_command: Some(format!("{} --session {}", account.command(), row.id)),
        app_account: false,
        branch: None,
        archived: row.archived_at.is_some(),
        attach_command: None,
        stalled_since: None,
        background: None,
        insight: None,
    }
}

/// An Antigravity conversation, from its summary. The summary's status words haven't been
/// seen on a real conversation yet, so only `not_fully_idle` and the times are relied on;
/// a status that says waiting, blocked or paused is taken at its word.
pub fn antigravity_session(account: &AccountCfg, c: &Conversation, seen_at: i64, now: i64) -> Session {
    let fresh = now - c.last_modified < ANTIGRAVITY_STALE_MS;
    let waiting = ["waiting", "blocked", "paused"].iter().any(|w| c.status.contains(w));
    let news = c.last_modified > seen_at;
    let status = if c.killed {
        Status::Idle
    } else if c.status.contains("error") && news {
        Status::Error
    } else if waiting {
        Status::Needs
    } else if c.not_fully_idle && fresh {
        Status::Working
    } else if news && c.last_modified > c.last_input.unwrap_or(0) {
        Status::Unread
    } else {
        Status::Idle
    };
    let title = c.title.clone().or_else(|| c.preview.clone()).unwrap_or_else(|| "Antigravity conversation".to_string());
    let excerpt = match status {
        Status::Needs | Status::Error => Some(c.status.clone()).filter(|s| !s.is_empty()),
        _ => c.preview.clone(),
    };
    let key = format!("antigravity:{}:{}", account.name, c.id);
    Session {
        open_target: format!("s|{key}|antigravity"),
        visitor: None,
        headline: String::new(),
        detail: None,
        key,
        id: c.id.clone(),
        short_id: short_id(&c.id),
        harness: Harness::Antigravity,
        account: account.name.clone(),
        surface: Surface::App,
        status,
        live: status == Status::Working,
        pid: None,
        title: truncate(&plain(&title), 90),
        cwd: c.cwd.clone().unwrap_or_default(),
        work: Vec::new(),
        // The preview is the conversation's opening, as the app lists it.
        reply_tail: None,
        first_prompt: c.preview.clone(),
        name: None,
        started_at: None,
        status_since: if status == Status::Working { c.last_input } else { Some(c.last_modified) },
        last_prompt_at: c.last_input.unwrap_or(c.last_modified),
        last_activity: c.last_modified,
        excerpt,
        resume_command: None,
        app_account: false,
        branch: None,
        archived: false,
        attach_command: None,
        stalled_since: None,
        background: None,
        insight: None,
    }
}

/// Index of the seed whose path is the longest prefix of `cwd`.
pub fn best_seed(seeds: &[LaneSeed], cwd: &Path) -> Option<usize> {
    seeds
        .iter()
        .enumerate()
        .filter(|(_, s)| cwd.starts_with(&s.path))
        .max_by_key(|(_, s)| s.path.components().count())
        .map(|(i, _)| i)
}

/// How many of a session's latest commands and edits in worktrees decide which one it's in.
const WORK_WINDOW: usize = 5;

/// The lane a session is working in. Agents often start in one worktree and then work in
/// another (`cd ../repo-fix && …`), so it's where most of the last few commands ran and edits
/// landed, the latest winning a tie, and a stray command elsewhere doesn't move it. Work
/// outside every worktree doesn't count. With none, where the session was started.
pub fn session_seed(seeds: &[LaneSeed], s: &Session) -> Option<usize> {
    let recent: Vec<usize> = s.work.iter().rev().filter_map(|p| best_seed(seeds, Path::new(p))).take(WORK_WINDOW).collect();
    let votes = |i: usize| recent.iter().filter(|&&j| j == i).count();
    // Oldest first: of the lanes tied for most, `max_by_key` keeps the last, the latest.
    recent.iter().rev().copied().max_by_key(|&i| votes(i)).or_else(|| best_seed(seeds, Path::new(&s.cwd)))
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

/// Most urgent first, then the one you last sent a prompt. Activity would reorder working
/// sessions with every tool call; your prompts only move one when you do something.
fn sort_sessions(sessions: &mut [Session]) {
    sessions.sort_by(|a, b| urgency(a.status).cmp(&urgency(b.status)).then(b.last_prompt_at.cmp(&a.last_prompt_at)).then(b.last_activity.cmp(&a.last_activity)));
}

/// When you last sent a prompt to one of a lane's own sessions; for a lane with none, when
/// anything last happened in it.
fn lane_prompt_at(sessions: &[Session], last_activity: i64) -> i64 {
    sessions.iter().filter(|s| s.visitor.is_none() && !s.archived).map(|s| s.last_prompt_at).max().unwrap_or(last_activity)
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
/// sessions: every session on `main` would match.
pub fn pr_items(seeds: &[LaneSeed], lanes: &[Lane], prs: &HashMap<String, Vec<PrRaw>>, login: Option<&str>) -> Vec<PrItem> {
    let mut project_of: HashMap<&str, (&str, &str, Option<&str>)> = HashMap::new();
    for s in seeds {
        if let Some(slug) = s.repo_slug.as_deref() {
            project_of.entry(slug).or_insert((s.project.as_str(), s.repo.as_str(), s.default_branch.as_deref()));
        }
    }
    let mut out = Vec::new();
    for (slug, list) in prs {
        let Some(&(project, repo, default_branch)) = project_of.get(slug.as_str()) else { continue };
        let here: Vec<&Lane> = lanes.iter().filter(|l| l.repo_slug.as_deref() == Some(slug.as_str())).collect();
        for p in list {
            let head = p.head_ref.as_str();
            let (opened, ended) = (p.created_at, p.merged_at.or(p.closed_at));
            let before_end = move |s: &Session| ended.map_or(true, |end| s.started_at.unwrap_or(s.last_activity) <= end);
            let during = move |s: &Session| s.last_activity >= opened - 3_600_000 && before_end(s);
            let mut sessions: Vec<PrSession> = Vec::new();
            if default_branch != Some(head) {
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
                for (l, s) in own.chain(guests) {
                    if !s.archived && !sessions.iter().any(|x| x.key == s.key) {
                        sessions.push(PrSession { key: s.key.clone(), lane: l.id.clone() });
                    }
                }
            }
            out.push(PrItem {
                key: format!("{slug}#{}", p.number),
                repo_slug: slug.clone(),
                repo: repo.to_string(),
                project: project.to_string(),
                number: p.number,
                title: p.title.clone(),
                url: p.url.clone(),
                state: p.state,
                author: p.author.clone(),
                mine: login.is_some() && p.author.as_deref() == login,
                review_requested: p.review_requested,
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
            });
        }
    }
    out.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    out
}

/// The first rule that matches. See *The model*, Lane status.
pub fn bay_of(sessions: &[Session], pr: Option<&Pr>) -> (Bay, Status) {
    let has = |st: Status| sessions.iter().any(|s| s.status == st);
    let working = has(Status::Working);
    if has(Status::Needs) {
        return (Bay::Needs, Status::Needs);
    }
    if has(Status::Error) {
        return (Bay::Needs, Status::Error);
    }
    if let Some(pr) = pr {
        if pr.state.is_open() && !working {
            if pr.checks.failed > 0 {
                return (Bay::Needs, Status::Error);
            }
            if pr.state == PrState::Changes {
                return (Bay::Needs, Status::Needs);
            }
        }
    }
    if working {
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

/// Applies Jev's read of a session to what the board shows, before bays and headlines are
/// worked out, so they follow it:
/// - a finished turn whose reply asks you something is Needs you, with the question;
/// - a busy session gone quiet on a command that isn't waiting for typing is working on it,
///   not stuck;
/// - a command left running in the background says whether it polls or runs a job, and is
///   stuck when it looks stuck.
pub fn apply_insight(s: &mut Session, i: &SessionInsight) {
    s.insight = Some(i.clone());
    if s.status == Status::Unread && i.asks_you.unwrap_or(0.0) >= ASKS_YOU {
        s.status = Status::Needs;
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
            "polling" => {
                let waits = s.reply_tail.as_deref().map(last_sentence).filter(|q| !q.is_empty());
                s.excerpt = Some(format!("Polling in the background · {}", waits.unwrap_or(shown)));
            }
            "job" => s.excerpt = Some(format!("Running in the background · {shown}")),
            "stalled" if i.background_p.unwrap_or(0.0) >= BACKGROUND_STALLED => {
                s.status = Status::Needs;
                s.stalled_since = Some(s.status_since.unwrap_or(s.last_activity));
                s.excerpt = Some(format!("Stuck on {shown}"));
            }
            _ => {}
        }
    }
}

/// Whether a worktree looks finished: not the main checkout, no PR that says where it stands
/// already (a merged or closed one has its own bay), nothing running or waiting, and every
/// session of its own read by Jev as finished.
pub fn looks_finished(origin: Origin, pr: Option<&Pr>, sessions: &[Session]) -> bool {
    if origin == Origin::Main || pr.is_some_and(|p| !p.state.is_open()) {
        return false;
    }
    let own: Vec<&Session> = sessions.iter().filter(|s| s.visitor.is_none() && !s.archived).collect();
    !own.is_empty()
        && own.iter().all(|s| !matches!(s.status, Status::Working | Status::Needs) && s.insight.as_ref().and_then(|i| i.finished).is_some_and(|f| f >= FINISHED))
}

/// A session's own sentence. See *The model*, Headlines.
pub fn session_headline(s: &Session) -> (String, Option<String>) {
    let acct = code(&s.account);
    if s.stalled_since.is_some() {
        return (format!("{acct} is stuck · {}", s.title), s.excerpt.clone());
    }
    match s.status {
        Status::Needs => match (&s.excerpt, s.harness) {
            (Some(ex), Harness::Claude) => (format!("{acct} is waiting for approval"), Some(ex.clone())),
            (Some(ex), Harness::Codex | Harness::Opencode) => (format!("{acct} asks for approval"), Some(ex.clone())),
            (Some(ex), Harness::Antigravity) => (format!("{acct} needs you"), Some(ex.clone())),
            (None, _) => (format!("{acct} needs you"), Some(s.title.clone())),
        },
        Status::Error => (format!("{acct} stopped with an error"), s.excerpt.clone()),
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
        Status::Unread => (format!("{acct} finished · {}", s.title), s.excerpt.clone()),
        Status::Idle => (format!("{acct} · {}", s.title), Some(format!("last turn {{ago:{}}}", s.last_activity))),
        Status::Stopped => (format!("{acct} · {}", s.title), Some(format!("stopped {{ago:{}}}", s.last_activity))),
    }
}

/// The lane's one sentence, from the rule that picked its bay. See *The model*, Headlines.
pub fn headline(bay: Bay, sessions: &[Session], pr: Option<&Pr>, git: Option<&GitInfo>) -> (String, Option<String>) {
    let acct = |s: &Session| code(&s.account);
    let first = |st: Status| sessions.iter().find(|s| s.status == st);
    match bay {
        Bay::Needs => {
            if let Some(s) = first(Status::Needs).or_else(|| first(Status::Error)) {
                return session_headline(s);
            }
            match pr {
                Some(pr) if pr.checks.failed > 0 => {
                    let mut names: Vec<String> = pr.checks.failing.iter().take(3).map(|n| code(n)).collect();
                    if pr.checks.failing.len() > 3 {
                        names.push(format!("+{} more", pr.checks.failing.len() - 3));
                    }
                    (format!("Checks failing on #{}", pr.number), Some(names.join(" · ")))
                }
                Some(pr) => (format!("Changes requested on #{}", pr.number), Some(plain(&truncate(&pr.title, 80)))),
                None => ("Needs you".to_string(), None),
            }
        }
        Bay::Working => session_headline(first(Status::Working).expect("working bay has a working session")),
        Bay::Unread => session_headline(first(Status::Unread).expect("unread bay has an unread session")),
        Bay::Review => {
            let pr = pr.expect("review bay has a PR");
            let c = &pr.checks;
            // Ready to merge is GitHub's word for it: the reviews the repo requires, passing
            // checks and no conflicts. Until GitHub has worked that out, an approval is only that.
            let n = pr.number;
            let head = match pr.state {
                PrState::Draft => format!("#{n} is a draft"),
                _ if pr.merge == Merge::Conflicts => format!("#{n} has conflicts"),
                _ if pr.merge == Merge::Behind => format!("#{n} · branch out of date"),
                PrState::Changes => format!("#{n} · changes requested"),
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

fn target(id: String, icon: &str, label: String, detail: Option<String>, kbd: Option<&[&str]>, disabled: bool, group: &str) -> Target {
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
pub fn targets(seed: &LaneSeed, sessions: &[Session], pr: Option<&Pr>, apps: &AppNames) -> (Vec<Target>, Option<String>) {
    let term = &apps.terminal;
    let editor = &apps.editor;
    let mut t = Vec::new();
    // The most recent session leads, since opening the lane opens it; the rest keep the lane's
    // order, most urgent first.
    let latest = latest_session(sessions);
    let rest = sessions.iter().filter(|s| latest.map_or(true, |l| l.key != s.key));
    for (i, s) in latest.into_iter().chain(rest).take(4).enumerate() {
        let first = i == 0;
        let enter: Option<&[&str]> = if first { Some(&["⏎"]) } else { None };
        let who = format!("{} · {}", s.account, truncate(&s.title, 48));
        let respond = s.status == Status::Needs;
        match (s.harness, s.surface) {
            (Harness::Claude, Surface::Cli) if s.live && s.attach_command.is_some() => {
                let why = format!("{who} · runs in the background");
                t.push(target(format!("s|{}|attach", s.key), "terminal", format!("Attach in {term}"), Some(why), enter, false, "session"));
            }
            (Harness::Claude, Surface::Cli) if s.live => {
                let verb = if respond { "Respond in" } else { "Show in" };
                t.push(target(format!("s|{}|focus", s.key), "terminal", format!("{verb} {term}"), Some(who), enter, false, "session"));
                if first && s.app_account {
                    let why = format!("Live in {term}, finish there first");
                    t.push(target(format!("s|{}|claude-app", s.key), "app", "Open in Claude app".into(), Some(why), None, true, "session"));
                }
            }
            (Harness::Claude, Surface::Cli) => {
                t.push(target(format!("s|{}|resume", s.key), "terminal", format!("Resume in {term}"), Some(who.clone()), enter, false, "session"));
                // Only the account the Claude app is signed into can continue there.
                if first && s.app_account {
                    t.push(target(format!("s|{}|claude-app", s.key), "app", "Continue in Claude app".into(), Some(who), None, false, "session"));
                }
            }
            (Harness::Claude, Surface::App) => {
                let verb = if respond { "Respond in" } else { "Open in" };
                t.push(target(format!("s|{}|claude-app", s.key), "app", format!("{verb} Claude app"), Some(who.clone()), enter, false, "session"));
                if first {
                    if s.live {
                        t.push(target(format!("s|{}|resume", s.key), "terminal", format!("Resume in {term}"), Some("Live in the Claude app".into()), None, true, "session"));
                    } else {
                        t.push(target(format!("s|{}|resume", s.key), "terminal", format!("Resume in {term}"), Some(who), None, false, "session"));
                    }
                }
            }
            (Harness::Codex, _) => {
                let verb = if respond { "Respond in" } else { "Open in" };
                t.push(target(format!("s|{}|codex", s.key), "app", format!("{verb} Codex"), Some(who), enter, false, "session"));
            }
            (Harness::Opencode, _) => {
                // A terminal showing it is focused; otherwise `opencode --session` opens a new one.
                let (verb, action) = match (respond, s.pid.is_some()) {
                    (true, _) => ("Respond in", if s.pid.is_some() { "focus" } else { "resume" }),
                    (false, true) => ("Show in", "focus"),
                    (false, false) => ("Resume in", "resume"),
                };
                t.push(target(format!("s|{}|{action}", s.key), "terminal", format!("{verb} {term}"), Some(who), enter, false, "session"));
            }
            (Harness::Antigravity, _) => {
                let why = format!("{who} · no link to a conversation, so the app comes forward");
                t.push(target(format!("s|{}|antigravity", s.key), "app", "Open Antigravity".into(), Some(why), enter, false, "session"));
            }
        }
    }
    let here = tilde(&seed.path);
    t.push(target("l|terminal".into(), "terminal", format!("New {term} terminal here"), Some(here), Some(&["⌘", "1"]), false, "lane"));
    t.push(target("l|editor".into(), "code", format!("Open in {editor}"), None, Some(&["⌘", "2"]), false, "lane"));
    t.push(target("l|finder".into(), "folder", "Reveal in Finder".into(), None, None, false, "lane"));
    if let Some(pr) = pr {
        t.push(target("p|github".into(), "external", format!("Open #{} on GitHub", pr.number), None, Some(&["⌘", "O"]), false, "pr"));
        t.push(target("p|codex-review".into(), "eye", format!("Review #{} in Codex", pr.number), None, None, false, "pr"));
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
    let Inputs { now, apps, seeds, mut sessions, visitors, git, prs, project_order, warnings, loading, github_at, pinned, github_login } = inp;
    for s in &mut sessions {
        let (h, d) = session_headline(s);
        s.headline = h;
        s.detail = d;
    }
    let mut by_lane: Vec<Vec<Session>> = vec![Vec::new(); seeds.len()];
    // A worktree made inside a Claude session's scratchpad also shows the session that made it,
    // even though that session runs in another lane.
    let owners: HashMap<String, Session> = sessions.iter().filter(|s| s.harness == Harness::Claude && !s.archived).map(|s| (s.id.clone(), s.clone())).collect();
    let by_key: HashMap<String, Session> = sessions.iter().map(|s| (s.key.clone(), s.clone())).collect();
    for s in sessions {
        if let Some(i) = session_seed(seeds, &s) {
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
            if let Some(owner) = crate::repos::scratchpad_owner(&seed.path).and_then(|id| owners.get(&id)) {
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
        // Archived sessions stay in the lane's list, last, and out of its status.
        let (mut sessions, archived): (Vec<Session>, Vec<Session>) = sessions.into_iter().partition(|s| !s.archived);
        sort_sessions(&mut sessions);
        let mut kept = 0;
        sessions.retain(|s| {
            let keep = kept < MAX_SESSIONS_PER_LANE || pinned.contains(&s.key);
            kept += keep as usize;
            keep
        });
        let g = git.get(&seed.path).cloned();
        let pr = find_pr(seed, prs, now);
        // The bay and headline come from the lane's own sessions; the maker is shown after them.
        let (bay, glyph) = bay_of(&sessions, pr.as_ref());
        let (mut headline, mut detail) = headline(bay, &sessions, pr.as_ref(), g.as_ref());
        if let (Bay::Idle, true, Some(g)) = (bay, sessions.is_empty(), guests.first()) {
            let verb = if g.visitor.as_deref() == Some(MADE_IT) { "Made by" } else { "Worked on by" };
            headline = format!("{verb} {} · {}", code(&g.account), g.title);
            detail = Some(format!("last turn {{ago:{}}}", g.last_activity));
        }
        sessions.extend(guests);
        let (targets, default_target) = targets(seed, &sessions, pr.as_ref(), apps);
        let last_activity = sessions
            .iter()
            .map(|s| s.last_activity)
            .chain(g.as_ref().and_then(|g| g.last_commit_at))
            .max()
            .unwrap_or(0);
        sessions.extend(archived);
        let dirty = g.as_ref().map(|g| g.dirty > 0).unwrap_or(false);
        let parked = bay == Bay::Idle && !dirty && now - last_activity > 14 * DAY_MS;
        let looks_finished = looks_finished(seed.origin, pr.as_ref(), &sessions);
        lanes.push(Lane {
            id: seed.path.to_string_lossy().into_owned(),
            project: seed.project.clone(),
            repo: seed.repo.clone(),
            repo_slug: seed.repo_slug.clone(),
            path: seed.path.to_string_lossy().into_owned(),
            callsign: seed.callsign.clone(),
            origin: seed.origin,
            branch: seed.branch.clone(),
            detached: if seed.branch.is_none() { seed.head.as_ref().map(|h| h.chars().take(7).collect()) } else { None },
            bay,
            glyph,
            parked,
            hidden: seed.elsewhere && bay != Bay::Needs,
            headline,
            detail,
            sessions,
            pr,
            git: g,
            looks_finished,
            last_activity,
            targets,
            default_target,
        });
    }
    lanes.sort_by_key(|l| (l.bay, Reverse(lane_prompt_at(&l.sessions, l.last_activity)), Reverse(l.last_activity)));
    let pr_list = pr_items(seeds, &lanes, prs, github_login);

    let mut projects = Vec::new();
    let mut names: Vec<String> = project_order.to_vec();
    if lanes.iter().any(|l| l.project == ELSEWHERE) && !names.iter().any(|n| n == ELSEWHERE) {
        names.push(ELSEWHERE.to_string());
    }
    for name in names {
        let mine: Vec<&Lane> = lanes.iter().filter(|l| l.project == name).collect();
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
        warnings,
        loading,
        github_at,
        usage: Vec::new(),
        pinned: pinned.to_vec(),
        pinned_worktrees: Vec::new(),
        prs: pr_list,
        github_login: github_login.map(str::to_string),
        theme: Default::default(),
        perf_log: false,
        todos: Vec::new(),
        labels: Vec::new(),
        quick_shortcut: String::new(),
        jev: Default::default(),
        todo_hints: Default::default(),
    }
}

pub const ELSEWHERE: &str = "Elsewhere";

/// Match the floating bar's scope: all visible projects, without archived sessions or
/// duplicate visitors. An open review request also needs the user's attention.
pub fn logo_status(lanes: &[Lane], prs: &[PrItem]) -> LogoStatus {
    let sessions = lanes.iter().filter(|l| !l.hidden).flat_map(|l| &l.sessions)
        .filter(|s| !s.archived && s.visitor.is_none())
        .map(|s| match s.status {
            Status::Error => LogoStatus::Error,
            Status::Needs => LogoStatus::Needs,
            Status::Unread => LogoStatus::Unread,
            Status::Working => LogoStatus::Working,
            Status::Idle | Status::Stopped => LogoStatus::Idle,
        });
    let review = prs.iter().any(|p| p.review_requested && p.state.is_open());
    sessions.chain(review.then_some(LogoStatus::Needs)).min().unwrap_or_default()
}

/// The folders a new terminal at `path` belongs beside, nearest first: its worktree, the other
/// worktrees of its repo, then the rest of its project. `[apps] new_terminal = "project"` opens
/// it as a tab in a window already showing the nearest of them. A folder elsewhere (a project of
/// its own that isn't one) has only itself for company.
pub fn terminal_neighbours(lanes: &[Lane], path: &Path) -> Vec<Vec<String>> {
    let within = |l: &&Lane| path.starts_with(&l.path);
    let Some(home) = lanes.iter().filter(within).max_by_key(|l| l.path.len()) else {
        return vec![vec![path.to_string_lossy().into_owned()]];
    };
    let mut groups = vec![vec![home.path.clone()]];
    if home.origin == Origin::Other {
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

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_790_200_000_000;

    fn acct(name: &str) -> AccountCfg {
        AccountCfg { name: name.into(), harness: "claude".into(), home: format!("~/.{name}"), command: Some(name.into()) }
    }

    fn reg(status: &str, updated: i64) -> RegistryEntry {
        RegistryEntry {
            pid: 1,
            session_id: "abc1234567".into(),
            cwd: "/r/atlas-quarry/apps/server".into(),
            status: status.into(),
            kind: Some("interactive".into()),
            job_id: None,
            entrypoint: Some("cli".into()),
            name: Some("atlas-quarry-8f".into()),
            name_source: Some("derived".into()),
            started_at: Some(NOW - 3_600_000),
            status_updated_at: Some(updated),
        }
    }

    fn tr() -> TranscriptInfo {
        TranscriptInfo {
            session_id: "abc1234567".into(),
            cwd: "/r/atlas-quarry/apps/server".into(),
            title: Some("Quarry daily log + photo sync".into()),
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
            project: "Northwind".into(),
            repo: "atlas".into(),
            repo_slug: Some("northwindai/atlas".into()),
            head_owners: vec!["northwindai".into(), "ghempton".into()],
            callsign: "quarry".into(),
            origin: Origin::Script,
            branch: Some(branch.into()),
            head: Some("0ef4ed2".into()),
            default_branch: Some("main".into()),
            elsewhere: false,
        }
    }

    fn pr(state: PrState, failed: u32) -> PrRaw {
        PrRaw {
            number: 1037,
            title: "Let the agent add and caption project photos".into(),
            url: "https://github.com/northwindai/atlas/pull/1037".into(),
            state,
            head_ref: "quarry".into(),
            head_owner: Some("ghempton".into()),
            head_oid: Some("0ef4ed2".into()),
            updated_at: NOW - 3_600_000,
            merged_at: if state == PrState::Merged { Some(NOW - 7_200_000) } else { None },
            closed_at: None,
            checks: Checks { passed: 11, failed, failing: (0..failed).map(|i| format!("check-{i}")).collect(), ..Default::default() },
            merge: Merge::Unknown,
            author: Some("ghempton".into()),
            base_ref: Some("main".into()),
            created_at: NOW - 86_400_000,
            additions: 120,
            deletions: 8,
            changed_files: 6,
            review_requested: false,
        }
    }

    fn logo_board() -> Board {
        let session = claude_session(&acct("claude2"), Some(&reg("busy", NOW)), Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
        compose(Inputs {
            now: NOW,
            apps: &AppNames::default(),
            seeds: &[seed("/r/atlas-quarry", "quarry")],
            sessions: vec![session],
            visitors: &HashMap::new(),
            git: &HashMap::new(),
            prs: &HashMap::from([("northwindai/atlas".to_string(), vec![pr(PrState::Open, 0)])]),
            project_order: &["Northwind".to_string()],
            warnings: vec![],
            loading: false,
            github_at: None,
            pinned: &[],
            github_login: Some("ghempton"),
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
    fn links_prs_to_the_worktrees_and_sessions_on_their_branch() {
        let apps = AppNames::default();
        let mut s = seed("/r/atlas-quarry", "quarry");
        s.repo_slug = Some("northwindai/atlas".into());
        let seeds = vec![s];
        let a = acct("claude2");
        let mut on_branch = claude_session(&a, Some(&reg("idle", NOW)), Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
        on_branch.branch = Some("quarry".into());
        let mut elsewhere = on_branch.clone();
        elsewhere.key = "claude:claude2:other".into();
        elsewhere.branch = Some("app4".into());
        let prs = HashMap::from([("northwindai/atlas".to_string(), vec![pr(PrState::Open, 0)])]);
        let (git, visitors) = (HashMap::new(), HashMap::new());
        let order = vec!["Northwind".to_string()];
        let board = compose(Inputs {
            now: NOW,
            apps: &apps,
            seeds: &seeds,
            sessions: vec![on_branch.clone(), elsewhere],
            visitors: &visitors,
            git: &git,
            prs: &prs,
            project_order: &order,
            warnings: vec![],
            loading: false,
            github_at: None,
            pinned: &[],
            github_login: Some("ghempton"),
        });
        assert_eq!(board.prs.len(), 1);
        let p = &board.prs[0];
        assert_eq!(p.key, "northwindai/atlas#1037");
        assert!(p.mine);
        assert_eq!(p.lanes, vec!["/r/atlas-quarry".to_string()]);
        assert_eq!(p.sessions, vec![PrSession { key: on_branch.key, lane: "/r/atlas-quarry".into() }]);
        assert_eq!(board.github_login.as_deref(), Some("ghempton"));
    }

    #[test]
    fn a_session_working_in_a_worktree_from_the_checkout_goes_by_the_worktrees_branch() {
        let apps = AppNames::default();
        let mut wt = seed("/r/atlas-quarry", "quarry");
        wt.repo_slug = Some("northwindai/atlas".into());
        let mut main = seed("/r/atlas", "main");
        main.repo_slug = Some("northwindai/atlas".into());
        main.default_branch = Some("main".into());
        let seeds = vec![main, wt];
        let a = acct("claude2");
        // Started in the checkout on main, then made the worktree and did all its work there.
        let mut s = claude_session(&a, Some(&reg("idle", NOW)), Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
        s.cwd = "/r/atlas".into();
        s.branch = Some("main".into());
        s.work = vec!["/r/atlas-quarry".into(); 5];
        let prs = HashMap::from([("northwindai/atlas".to_string(), vec![pr(PrState::Open, 0)])]);
        let (git, visitors) = (HashMap::new(), HashMap::new());
        let order = vec!["Northwind".to_string()];
        let board = compose(Inputs {
            now: NOW,
            apps: &apps,
            seeds: &seeds,
            sessions: vec![s.clone()],
            visitors: &visitors,
            git: &git,
            prs: &prs,
            project_order: &order,
            warnings: vec![],
            loading: false,
            github_at: None,
            pinned: &[],
            github_login: None,
        });
        assert_eq!(board.prs[0].sessions, vec![PrSession { key: s.key, lane: "/r/atlas-quarry".into() }]);
    }

    #[test]
    fn a_long_lived_branch_holds_only_its_latest_pr() {
        let apps = AppNames::default();
        let seeds = vec![seed("/r/atlas-quarry", "quarry")];
        let a = acct("claude2");
        let mut earlier = claude_session(&a, None, Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
        earlier.key = "claude:claude2:earlier".into();
        earlier.branch = Some("quarry".into());
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
        let prs = HashMap::from([("northwindai/atlas".to_string(), vec![first, pr(PrState::Open, 0)])]);
        let (git, visitors) = (HashMap::new(), HashMap::new());
        let order = vec!["Northwind".to_string()];
        let board = compose(Inputs {
            now: NOW,
            apps: &apps,
            seeds: &seeds,
            sessions: vec![earlier.clone(), latest.clone()],
            visitors: &visitors,
            git: &git,
            prs: &prs,
            project_order: &order,
            warnings: vec![],
            loading: false,
            github_at: None,
            pinned: &[],
            github_login: None,
        });
        let get = |n: u64| board.prs.iter().find(|p| p.number == n).unwrap();
        let listed = |key: &str| PrSession { key: key.into(), lane: "/r/atlas-quarry".into() };
        assert_eq!(get(1037).lanes, vec!["/r/atlas-quarry".to_string()]);
        assert_eq!(get(1037).sessions, vec![listed(&latest.key)]);
        assert!(get(1015).lanes.is_empty(), "the worktree holds the newer PR, as the Worktrees view shows");
        assert_eq!(get(1015).sessions, vec![listed(&earlier.key)], "its sessions still show, under the worktree they ran in");
    }

    #[test]
    fn prs_link_the_sessions_that_made_or_worked_in_their_worktree() {
        let apps = AppNames::default();
        let scratch = "/private/tmp/claude-501/-r-osprey/abc12345-0000-0000-0000-000000000000/scratchpad/atlas-nx-cache";
        let seeds = vec![seed("/r/osprey", "main"), seed(scratch, "nx-cache")];
        let a = acct("claude");
        let mut maker = claude_session(&a, Some(&reg("idle", NOW)), Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
        maker.id = "abc12345-0000-0000-0000-000000000000".into();
        maker.key = format!("claude:claude:{}", maker.id);
        maker.cwd = "/r/osprey".into();
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
        let (git, order) = (HashMap::new(), vec!["Northwind".to_string()]);
        let board_with = |p: PrRaw| {
            let prs = HashMap::from([("northwindai/atlas".to_string(), vec![PrRaw { head_ref: "nx-cache".into(), ..p }])]);
            compose(Inputs {
                now: NOW,
                apps: &apps,
                seeds: &seeds,
                sessions: vec![maker.clone(), helper.clone(), earlier.clone()],
                visitors: &visitors,
                git: &git,
                prs: &prs,
                project_order: &order,
                warnings: vec![],
                loading: false,
                github_at: None,
                pinned: &[],
                github_login: None,
            })
        };
        let listed = |key: &str| PrSession { key: key.into(), lane: scratch.into() };
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
        let stopped = claude_session(&a, None, Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
        assert_eq!(stopped.status, Status::Stopped);
        assert!(!stopped.live);
        assert_eq!(stopped.resume_command.as_deref(), Some("claude2 --resume abc1234567"));
        assert_eq!(stopped.open_target, "s|claude:claude2:abc1234567|resume");
        assert_eq!(stopped.title, "Quarry daily log + photo sync");
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
    fn a_long_silent_busy_session_is_stuck() {
        let a = acct("claude2");
        let mut t = tr();
        t.last_ts = NOW - 3 * DAY_MS;
        let r = reg("busy", NOW - 3 * DAY_MS);
        let stuck = claude_session(&a, Some(&r), Some(&t), 0, NOW, &Quiet { subagent_at: None, running: Some("gcloud auth login") }).unwrap();
        assert_eq!(stuck.status, Status::Needs);
        assert_eq!(stuck.stalled_since, Some(NOW - 3 * DAY_MS));
        assert_eq!(stuck.excerpt.as_deref(), Some(format!("Stuck on `gcloud auth login` for {{for:{}}}", NOW - 3 * DAY_MS).as_str()));
        assert_eq!(session_headline(&stuck).0, "`claude2` is stuck · Quarry daily log + photo sync");
        let silent = claude_session(&a, Some(&r), Some(&t), 0, NOW, &Quiet::default()).unwrap();
        assert!(silent.excerpt.as_deref().unwrap_or_default().starts_with("No activity for {for:"));
        // A subagent still writing its log keeps the session working.
        let sub = claude_session(&a, Some(&r), Some(&t), 0, NOW, &Quiet { subagent_at: Some(NOW - 60_000), running: None }).unwrap();
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
        assert!(unsure.insight.is_some());
        apply_insight(&mut s, &insight(|i| i.asks_you = Some(0.9)));
        assert_eq!(s.status, Status::Needs);
        assert_eq!(s.excerpt.as_deref(), Some("Asks you · Want me to open the PR, or keep it local?"));
        // Seen already, it's yours to answer when you like: no change.
        let mut idle = claude_session(&a, Some(&reg("idle", NOW - 100)), Some(&tr()), NOW - 10, NOW, &Quiet::default()).unwrap();
        apply_insight(&mut idle, &insight(|i| i.asks_you = Some(0.9)));
        assert_eq!(idle.status, Status::Idle);
    }

    #[test]
    fn a_quiet_command_that_isnt_waiting_for_typing_is_working() {
        let a = acct("claude2");
        let mut t = tr();
        t.last_ts = NOW - 3 * DAY_MS;
        let quiet = Quiet { subagent_at: None, running: Some("cargo build --release") };
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
        let quiet = Quiet { subagent_at: None, running: Some("while true; do sleep 60; gh pr view 1054; done") };
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
        assert_eq!(session_headline(&s).0, "`claude2` is stuck · Quarry daily log + photo sync");
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
        let s = claude_session(&a, Some(&r), Some(&t), 0, NOW, &Quiet { subagent_at: None, running: Some(poll) }).unwrap();
        assert_eq!((s.status, s.stalled_since), (Status::Working, None));
        assert!(s.excerpt.as_deref().unwrap().starts_with("Working in the background · `while true; do sleep 60;"), "{:?}", s.excerpt);
        assert_eq!(s.background.as_deref(), Some(poll));
        let unknown = claude_session(&a, Some(&r), Some(&t), 0, NOW, &Quiet::default()).unwrap();
        assert_eq!((unknown.status, unknown.excerpt.as_deref()), (Status::Working, Some("Working in the background")));
        assert_eq!(claude_busy_since(Some(&r), Some(&t)), None);
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
    fn maps_codex_statuses() {
        let a = AccountCfg { name: "codex".into(), harness: "codex".into(), home: "~/.codex".into(), command: None };
        let mut th = ThreadInfo { id: "01a0c4a6".into(), cwd: "/r".into(), last_ts: NOW - 1_000, turn_open: true, turn_started_at: Some(NOW - 5_000), ..Default::default() };
        assert_eq!(codex_session(&a, &th, Some("Photo tools"), 0, NOW).status, Status::Working);
        assert_eq!(codex_session(&a, &th, None, 0, NOW).open_target, "s|codex:codex:01a0c4a6|codex");
        th.last_ts = NOW - CODEX_STALE_MS - 1;
        assert_eq!(codex_session(&a, &th, None, 0, NOW).status, Status::Idle);
        th.turn_open = false;
        th.last_complete_at = Some(NOW - 1_000);
        assert_eq!(codex_session(&a, &th, None, NOW - 2_000, NOW).status, Status::Unread);
        assert_eq!(codex_session(&a, &th, None, NOW, NOW).status, Status::Idle);
        th.pending_approval = Some("Run · `yarn install`".into());
        assert_eq!(codex_session(&a, &th, None, 0, NOW).status, Status::Needs);
        // A computer-use call that hasn't answered in a minute is waiting for approval.
        th.pending_approval = None;
        th.turn_open = true;
        th.last_ts = NOW - 30_000;
        th.pending_calls = vec![crate::codex::PendingCall { call_id: "c".into(), at: NOW - 30_000, what: "Inspect the Dock".into(), limit_ms: 60_000 }];
        assert_eq!(codex_session(&a, &th, None, 0, NOW).status, Status::Working);
        th.pending_calls[0].at = NOW - 61_000;
        let waiting = codex_session(&a, &th, None, 0, NOW);
        assert_eq!(waiting.status, Status::Needs);
        assert_eq!(waiting.excerpt.as_deref(), Some("Waiting for approval? · Inspect the Dock"));
    }

    #[test]
    fn maps_opencode_statuses() {
        let a = AccountCfg { name: "opencode".into(), harness: "opencode".into(), home: "~/.local/share/opencode".into(), command: None };
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
        busy.pending.insert("ses_abc1234567".into(), "Bash · `npm install`".into());
        let s = opencode_session(&a, &row, Some(&busy), None, 0, NOW);
        assert_eq!((s.status, s.excerpt.as_deref()), (Status::Needs, Some("Bash · `npm install`")));
        assert_eq!(session_headline(&s).0, "`opencode` asks for approval");
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
        let a = AccountCfg { name: "antigravity".into(), harness: "antigravity".into(), home: "~/.gemini/antigravity".into(), command: None };
        let mut c = Conversation {
            id: "c1".into(),
            title: Some("Photo tools".into()),
            preview: Some("Tighten the photo tools".into()),
            cwd: Some("/r".into()),
            not_fully_idle: true,
            last_modified: NOW - 1_000,
            last_input: Some(NOW - 5_000),
            ..Default::default()
        };
        let s = antigravity_session(&a, &c, 0, NOW);
        assert_eq!((s.status, s.live, s.surface), (Status::Working, true, Surface::App));
        assert_eq!(s.open_target, "s|antigravity:antigravity:c1|antigravity");
        assert_eq!(s.cwd, "/r");
        c.last_modified = NOW - ANTIGRAVITY_STALE_MS - 1;
        assert_eq!(antigravity_session(&a, &c, 0, NOW).status, Status::Idle);
        c.last_modified = NOW - 1_000;
        c.not_fully_idle = false;
        assert_eq!(antigravity_session(&a, &c, NOW - 2_000, NOW).status, Status::Unread);
        assert_eq!(antigravity_session(&a, &c, NOW, NOW).status, Status::Idle);
        // Nothing new since you wrote: not your turn.
        c.last_input = Some(NOW - 500);
        assert_eq!(antigravity_session(&a, &c, 0, NOW).status, Status::Idle);
        c.status = "waiting".into();
        let s = antigravity_session(&a, &c, 0, NOW);
        assert_eq!((s.status, s.excerpt.as_deref()), (Status::Needs, Some("waiting")));
        assert_eq!(session_headline(&s).0, "`antigravity` needs you");
        c.killed = true;
        assert_eq!(antigravity_session(&a, &c, 0, NOW).status, Status::Idle);
    }

    #[test]
    fn assigns_sessions_to_the_deepest_lane() {
        let seeds = vec![seed("/r/atlas", "app"), seed("/r/atlas/.claude/worktrees/bold-fermi-1ac421", "claude/bold-fermi")];
        assert_eq!(best_seed(&seeds, Path::new("/r/atlas/.claude/worktrees/bold-fermi-1ac421/src")), Some(1));
        assert_eq!(best_seed(&seeds, Path::new("/r/atlas/apps")), Some(0));
        assert_eq!(best_seed(&seeds, Path::new("/r/atlas-app2")), None);
    }

    #[test]
    fn lists_a_session_under_the_worktree_it_works_in() {
        let seeds = vec![seed("/r/codealong", "main"), seed("/r/codealong-pins", "pins")];
        let mut s = claude_session(&acct("claude"), Some(&reg("busy", NOW)), Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
        s.cwd = "/r/codealong".into();
        let at = |s: &Session| session_seed(&seeds, s).map(|i| seeds[i].path.to_string_lossy().into_owned());
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
    fn a_session_working_in_another_worktree_puts_it_in_working() {
        let apps = AppNames::default();
        let seeds = vec![seed("/r/codealong", "main"), seed("/r/codealong-pins", "pins")];
        let mut s = claude_session(&acct("claude"), Some(&reg("busy", NOW)), Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
        s.cwd = "/r/codealong".into();
        s.work = vec!["/r/codealong-pins".into(); 3];
        let (git, prs, visitors) = (HashMap::new(), HashMap::new(), HashMap::new());
        let order = vec!["Northwind".to_string()];
        let board = compose(Inputs { now: NOW, apps: &apps, seeds: &seeds, sessions: vec![s], visitors: &visitors, git: &git, prs: &prs, project_order: &order, warnings: vec![], loading: false, github_at: None, pinned: &[], github_login: None });
        let lane = |p: &str| board.lanes.iter().find(|l| l.path == p).unwrap();
        assert_eq!(lane("/r/codealong-pins").bay, Bay::Working);
        assert_eq!(lane("/r/codealong-pins").sessions.len(), 1);
        assert_eq!(lane("/r/codealong").bay, Bay::Idle);
        assert!(lane("/r/codealong").sessions.is_empty());
    }

    #[test]
    fn finds_prs_by_branch_and_owner() {
        let s = seed("/r/atlas-quarry", "quarry");
        let mut prs = HashMap::new();
        prs.insert("northwindai/atlas".to_string(), vec![pr(PrState::Approved, 0)]);
        assert_eq!(find_pr(&s, &prs, NOW).map(|p| p.number), Some(1037));
        let mut other_owner = pr(PrState::Open, 0);
        other_owner.head_owner = Some("stranger".into());
        prs.insert("northwindai/atlas".to_string(), vec![other_owner]);
        assert!(find_pr(&s, &prs, NOW).is_none());
        let mut main = s.clone();
        main.branch = Some("main".into());
        assert!(find_pr(&main, &prs, NOW).is_none());
        let mut old = pr(PrState::Merged, 0);
        old.merged_at = Some(NOW - 8 * DAY_MS);
        prs.insert("northwindai/atlas".to_string(), vec![old]);
        assert!(find_pr(&s, &prs, NOW).is_none());
        let mut moved_on = pr(PrState::Merged, 0);
        moved_on.head_oid = Some("ffffff0".into());
        prs.insert("northwindai/atlas".to_string(), vec![moved_on]);
        assert!(find_pr(&s, &prs, NOW).is_none(), "a reused branch drops its merged PR");
    }

    #[test]
    fn ranks_bays() {
        let a = acct("claude2");
        let needs = claude_session(&a, Some(&reg("waiting", NOW)), Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
        let working = claude_session(&a, Some(&reg("busy", NOW)), Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
        let unread = claude_session(&a, Some(&reg("idle", NOW)), Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
        let open_pr = find_pr(&seed("/r", "quarry"), &HashMap::from([("northwindai/atlas".to_string(), vec![pr(PrState::Open, 0)])]), NOW);
        let failing = find_pr(&seed("/r", "quarry"), &HashMap::from([("northwindai/atlas".to_string(), vec![pr(PrState::Open, 2)])]), NOW);
        let merged = find_pr(&seed("/r", "quarry"), &HashMap::from([("northwindai/atlas".to_string(), vec![pr(PrState::Merged, 0)])]), NOW);
        assert_eq!(bay_of(&[working.clone(), needs.clone()], None), (Bay::Needs, Status::Needs));
        assert_eq!(bay_of(&[unread.clone()], failing.as_ref()), (Bay::Needs, Status::Error));
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
        assert_eq!(
            headline(Bay::Needs, &[needs], None, None),
            ("`claude2` is waiting for approval".to_string(), Some("Bash · `yarn mikro-orm migration:up`".to_string()))
        );
        let working = claude_session(&a, Some(&reg("busy", NOW - 5_000)), Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
        let (h, d) = headline(Bay::Working, &[working], None, None);
        assert_eq!(h, "`claude2` · Quarry daily log + photo sync");
        assert_eq!(d, Some(format!("{{for:{}}} · running `yarn test`", NOW - 5_000)));
        // A message sent while it works restarts the clock.
        let nudged = TranscriptInfo { last_prompt_at: Some(NOW - 2_000), ..tr() };
        let working = claude_session(&a, Some(&reg("busy", NOW - 5_000)), Some(&nudged), 0, NOW, &Quiet::default()).unwrap();
        assert_eq!(headline(Bay::Working, &[working], None, None).1, Some(format!("{{for:{}}} · running `yarn test`", NOW - 2_000)));
        let failing = find_pr(&seed("/r", "quarry"), &HashMap::from([("northwindai/atlas".to_string(), vec![pr(PrState::Open, 2)])]), NOW);
        assert_eq!(
            headline(Bay::Needs, &[], failing.as_ref(), None),
            ("Checks failing on #1037".to_string(), Some("`check-0` · `check-1`".to_string()))
        );
        let review = |state: PrState, merge: Merge| {
            let pr = find_pr(&seed("/r", "quarry"), &HashMap::from([("northwindai/atlas".to_string(), vec![PrRaw { merge, ..pr(state, 0) }])]), NOW);
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
    fn builds_targets_with_app_names() {
        let apps = AppNames { terminal: "Ghostty".into(), terminal_id: "ghostty".into(), editor: "Cursor".into(), editor_id: "cursor".into(), new_terminal: Default::default() };
        let a = acct("claude2");
        let live = claude_session(&a, Some(&reg("waiting", NOW)), Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
        let (t, default) = targets(&seed("/r/atlas-quarry", "quarry"), &[live], None, &apps);
        assert_eq!(t[0].label, "Respond in Ghostty");
        assert_eq!(default.as_deref(), Some("s|claude:claude2:abc1234567|focus"));
        assert!(t.iter().all(|x| !x.id.ends_with("|claude-app")), "claude2 isn't the Claude app's account");
        assert!(t.iter().any(|x| x.label == "Open in Cursor"));
        let main = AccountCfg { name: "claude".into(), harness: "claude".into(), home: "~/.claude".into(), command: None };
        let mine = claude_session(&main, Some(&reg("waiting", NOW)), Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
        let (t, _) = targets(&seed("/r/atlas-quarry", "quarry"), &[mine], None, &apps);
        assert!(t[1].disabled && t[1].id.ends_with("|claude-app"), "live elsewhere, so offered but disabled");
        let (none, default) = targets(&seed("/r/x", "x"), &[], None, &apps);
        assert_eq!(default.as_deref(), Some("l|terminal"));
        assert_eq!(none.len(), 3);
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
        let (t, default) = targets(&lane, &[needs.clone(), newer.clone()], None, &apps);
        assert_eq!(default.as_deref(), Some(newer.open_target.as_str()));
        assert_eq!(t[0].id, newer.open_target);
        assert_eq!(t[0].kbd, Some(vec!["⏎".to_string()]));
        let other = t.iter().find(|x| x.id == needs.open_target).expect("the other session is still offered");
        assert_eq!(other.kbd, None);
        // A tie goes to the more urgent one, listed first.
        newer.last_activity = needs.last_activity;
        let (_, default) = targets(&lane, &[needs.clone(), newer.clone()], None, &apps);
        assert_eq!(default.as_deref(), Some(needs.open_target.as_str()));
        // A session from another lane that worked here more recently doesn't beat the lane's own,
        // but opens a lane that has none.
        let mut guest = newer.clone();
        guest.key = "claude:claude2:guest".into();
        guest.open_target = format!("s|{}|resume", guest.key);
        guest.visitor = Some("worked here".into());
        guest.last_activity = NOW;
        let (_, default) = targets(&lane, &[needs.clone(), guest.clone()], None, &apps);
        assert_eq!(default.as_deref(), Some(needs.open_target.as_str()));
        let (_, default) = targets(&lane, &[guest.clone()], None, &apps);
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
        // With no prompt in its log, a session counts from when it started.
        let unknown = at("busy", None, NOW, "unknown");
        assert_eq!(unknown.last_prompt_at, NOW - 3_600_000);
    }

    #[test]
    fn scratchpad_worktrees_show_the_session_that_made_them() {
        let apps = AppNames::default();
        let scratch = "/private/tmp/claude-501/-r-osprey/abc12345-0000-0000-0000-000000000000/scratchpad/atlas-nx-cache";
        let seeds = vec![seed("/r/osprey", "main"), seed(scratch, "nx-cache")];
        let a = acct("claude");
        let mut maker = claude_session(&a, Some(&reg("busy", NOW)), Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
        maker.id = "abc12345-0000-0000-0000-000000000000".into();
        maker.key = format!("claude:claude:{}", maker.id);
        maker.cwd = "/r/osprey".into();
        let (git, prs, order) = (HashMap::new(), HashMap::new(), vec!["Northwind".to_string()]);
        let visitors = HashMap::new();
        let board = compose(Inputs { now: NOW, apps: &apps, seeds: &seeds, sessions: vec![maker], visitors: &visitors, git: &git, prs: &prs, project_order: &order, warnings: vec![], loading: false, github_at: None, pinned: &[], github_login: None });
        let lane = board.lanes.iter().find(|l| l.path == scratch).unwrap();
        assert_eq!(lane.sessions.len(), 1);
        assert_eq!(lane.sessions[0].visitor.as_deref(), Some("made this worktree"));
        assert_eq!(lane.bay, Bay::Idle, "the maker works in another lane, so it doesn't make this one Working");
        assert_eq!(lane.headline, "Made by `claude` · Quarry daily log + photo sync");
        assert_eq!(lane.default_target.as_deref(), Some("s|claude:claude:abc12345-0000-0000-0000-000000000000|focus"));
        assert_eq!(board.lanes.iter().find(|l| l.path == "/r/osprey").unwrap().sessions.len(), 1);
    }

    #[test]
    fn archived_sessions_stay_listed_but_out_of_the_lanes_status() {
        let apps = AppNames::default();
        let seeds = vec![seed("/r/atlas-quarry", "quarry")];
        let a = acct("claude2");
        let mut needs = claude_session(&a, Some(&reg("waiting", NOW)), Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
        needs.archived = true;
        let (git, prs, visitors) = (HashMap::new(), HashMap::new(), HashMap::new());
        let order = vec!["Northwind".to_string()];
        let board = compose(Inputs {
            now: NOW,
            apps: &apps,
            seeds: &seeds,
            sessions: vec![needs],
            visitors: &visitors,
            git: &git,
            prs: &prs,
            project_order: &order,
            warnings: vec![],
            loading: false,
            github_at: None,
            pinned: &[],
            github_login: None,
        });
        let lane = &board.lanes[0];
        assert_eq!(lane.bay, Bay::Idle);
        assert_eq!(lane.sessions.len(), 1);
        assert!(lane.sessions[0].archived);
        assert!(lane.targets.iter().all(|t| !t.id.starts_with("s|")));
    }

    #[test]
    fn pinned_sessions_are_never_cut_from_a_lane() {
        let apps = AppNames::default();
        let seeds = vec![seed("/r/atlas-quarry", "quarry")];
        let a = acct("claude2");
        let sessions: Vec<Session> = (0..MAX_SESSIONS_PER_LANE + 5)
            .map(|i| {
                let mut s = claude_session(&a, None, Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
                s.key = format!("claude:claude2:s{i}");
                s.last_activity = NOW - i as i64 * 60_000;
                s
            })
            .collect();
        let oldest = format!("claude:claude2:s{}", MAX_SESSIONS_PER_LANE + 4);
        let pinned = vec![oldest.clone()];
        let (git, prs, visitors) = (HashMap::new(), HashMap::new(), HashMap::new());
        let order = vec!["Northwind".to_string()];
        let board = compose(Inputs {
            now: NOW,
            apps: &apps,
            seeds: &seeds,
            sessions,
            visitors: &visitors,
            git: &git,
            prs: &prs,
            project_order: &order,
            warnings: vec![],
            loading: false,
            github_at: None,
            pinned: &pinned,
            github_login: None,
        });
        let lane = &board.lanes[0];
        assert_eq!(lane.sessions.len(), MAX_SESSIONS_PER_LANE + 1);
        assert!(lane.sessions.iter().any(|s| s.key == oldest));
        assert_eq!(board.pinned, pinned);
    }

    #[test]
    fn composes_a_board() {
        let apps = AppNames::default();
        let seeds = vec![seed("/r/atlas-quarry", "quarry"), seed("/r/atlas-app3", "app3")];
        let a = acct("claude2");
        let needs = claude_session(&a, Some(&reg("waiting", NOW)), Some(&tr()), 0, NOW, &Quiet::default()).unwrap();
        let git = HashMap::new();
        let prs = HashMap::new();
        let order = vec!["Northwind".to_string()];
        let visitors = HashMap::from([(PathBuf::from("/r/atlas-app3"), vec!["claude:claude2:abc1234567".to_string()])]);
        let board = compose(Inputs {
            now: NOW,
            apps: &apps,
            seeds: &seeds,
            sessions: vec![needs],
            visitors: &visitors,
            git: &git,
            prs: &prs,
            project_order: &order,
            warnings: vec![],
            loading: false,
            github_at: None,
            pinned: &[],
            github_login: None,
        });
        assert_eq!(board.lanes.len(), 2);
        assert_eq!(board.lanes[0].bay, Bay::Needs);
        assert_eq!(board.lanes[0].sessions.len(), 1);
        assert_eq!(board.lanes[1].bay, Bay::Idle, "a visitor doesn't change the bay");
        assert_eq!(board.lanes[1].sessions[0].visitor.as_deref(), Some("worked here"));
        assert_eq!(board.lanes[1].headline, "Worked on by `claude2` · Quarry daily log + photo sync");
        assert_eq!(board.lanes[0].sessions[0].headline, "`claude2` is waiting for approval");
        assert_eq!(board.projects[0].lanes, 2);
    }

    #[test]
    fn a_new_terminal_goes_beside_its_worktree_then_its_repo_then_its_project() {
        let apps = AppNames::default();
        let mut other_repo = seed("/r/kestrel", "main");
        other_repo.repo = "kestrel".into();
        let mut elsewhere = seed("/tmp/scratch", "main");
        (elsewhere.project, elsewhere.origin) = ("Elsewhere".into(), Origin::Other);
        let seeds = vec![seed("/r/atlas", "main"), seed("/r/atlas-pins", "pins"), other_repo, elsewhere];
        let (git, prs, visitors) = (HashMap::new(), HashMap::new(), HashMap::new());
        let order = vec!["Northwind".to_string()];
        let board = compose(Inputs { now: NOW, apps: &apps, seeds: &seeds, sessions: vec![], visitors: &visitors, git: &git, prs: &prs, project_order: &order, warnings: vec![], loading: false, github_at: None, pinned: &[], github_login: None });
        let near = |p: &str| terminal_neighbours(&board.lanes, Path::new(p));
        assert_eq!(near("/r/atlas-pins/src"), vec![vec!["/r/atlas-pins".to_string()], vec!["/r/atlas".to_string()], vec!["/r/kestrel".to_string()]]);
        assert_eq!(near("/tmp/scratch"), vec![vec!["/tmp/scratch".to_string()]]);
        assert_eq!(near("/Users/me"), vec![vec!["/Users/me".to_string()]]);
    }
}
