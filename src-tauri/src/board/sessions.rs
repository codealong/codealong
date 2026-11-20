//! A session's status, from what its harness recorded of it: for each harness a status ladder
//! and the function that builds its `Session` (`claude_session`, `codex_session`,
//! `opencode_session`, `antigravity_session`, `cursor_session`), and what those go by: how long
//! a turn may be silent before it isn't running, a Claude conversation that moved to the
//! background, the commands and agents a turn left running.
//!
//! Pure functions over what a harness's reader already read: the reader gathers a session's
//! inputs and calls its builder. The ladders share nothing yet, so a rule every harness should
//! follow is written into each builder and tested for each, below. What Codealong lays over a
//! status afterwards (Jev's read, a snooze) is overlays.rs, not here.

use super::*;

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
/// or use CPU well within this; `sleep 600` loops are the rare exception. A build queued for a
/// lock does nothing at all while it waits, so it counts as active while the build ahead of it
/// is (claude/mod.rs `watch_background`, procs.rs `CommandTree::waits_behind`).
pub const BACKGROUND_QUIET_MS: i64 = 15 * 60 * 1000;

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
/// commands its rollout says still run, while a Codex process has the thread loaded (`held`) and
/// they started since it took the thread up as far as that's known (`since`, codex/mod.rs
/// `terminals_since`), then those that ended in the last few minutes (`TASK_ENDED_SHOWN_MS`),
/// oldest first in each. Terminals go with the process that ran them, which may write no end for
/// them as it goes: one nothing holds runs none, and one another process took up runs none of the
/// earlier one's.
pub fn codex_tasks(th: &ThreadInfo, held: Option<bool>, since: Option<i64>, now: i64) -> Vec<BackgroundTask> {
    let t = &th.terminals;
    let running = t.running.iter().filter(|r| held != Some(false) && since.map_or(true, |since| r.at >= since));
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
    // A sign-in turned down, which only a sign-in mends; whether Codealong carries it on once
    // there's one is the engine's to say (`engine::with_sign_ins`).
    let signed_out = tr.and_then(|t| t.signed_out.clone()).filter(|_| status != Status::Working).map(|e| SignedOut { at: e.at, said: e.said, continues: false });
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
        signed_out,
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
    let written = [c.latest_step_at, c.failed.as_ref().map(|f| f.at)].into_iter().flatten().fold(c.last_modified, i64::max);
    let fresh = now - written < ANTIGRAVITY_STALE_MS;
    // Your prompt counts too: its terminal UI's log says it went in before a step or the row has
    // a time. One opened in a terminal and never prompted has neither (its row's time is gorm's
    // zero): there's nothing in it to read, and it has been there since its process began.
    let active = match written.max(c.last_input.unwrap_or(0)) {
        0 => process.and_then(|p| p.started).unwrap_or(0),
        at => at,
    };
    // While its process has the conversation, its summary is the process's word, unless another
    // `agy` that started since marked it idle; a step that runs or waits says so too, and a
    // terminal UI's log. A print run writes its summary only as it ends, so its record says
    // whether a turn is under way.
    let under_way = match (c.cli, process) {
        (true, Some(p)) => c.running() || c.idled_by_another || c.step_under_way || (!p.terminal && c.tail.turn_open) || c.ui_turn_open(),
        (true, None) => false,
        (false, _) => c.running() && app_running,
    };
    let news = written > seen_at;
    let status = if c.killed {
        Status::Idle
    } else if under_way && c.waiting {
        Status::Needs
    } else if under_way && (c.cli || fresh) {
        Status::Working
    } else if news && written > c.last_input.unwrap_or(0) {
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
        started_at: c.started_at,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::fixtures::{acct, insight, reg, tr, NOW};

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
        let listed = |held, since| codex_tasks(&th, held, since, NOW).into_iter().map(|t| (t.id, t.command, t.ended_at, t.exit_code)).collect::<Vec<_>>();
        let running = ("19957".to_string(), "run 19957".to_string(), None, None);
        let ended = ("2".to_string(), "run 2".to_string(), Some(NOW - 5 * MIN), Some(2));
        assert_eq!(listed(Some(true), Some(NOW - 60 * MIN)), [running.clone(), ended.clone()]);
        assert_eq!(listed(None, None), [running, ended.clone()], "a look that failed takes the rollout's word");
        // A thread no Codex process holds runs nothing: its terminals went with what ran them; nor
        // does one a process that started since loaded.
        assert_eq!(listed(Some(false), None), [ended.clone()]);
        assert_eq!(listed(Some(true), Some(NOW - 10 * MIN)), [ended]);
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
            started_at: Some(NOW - 10_000),
            ..Default::default()
        };
        c.tail.last_tool = Some("Run command · `npm test`".into());
        c.tail.last_text = Some("The tests pass now.".into());
        let s = antigravity_session(&a, &c, None, true, 0, NOW);
        assert_eq!((s.status, s.live, s.surface, s.started_at, s.excerpt.as_deref()), (Status::Working, true, Surface::App, Some(NOW - 10_000), Some("Run command · `npm test`")));
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
        let ui = Process { pid: 41, conversation: Some("c2".into()), terminal: true, logs: Vec::new(), started: None };
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

    /// An `agy` opened in a terminal and left at its box, never prompted (2026-11-19, open since
    /// the evening before): its row has no time and no steps. It waits for you to write, with
    /// nothing to read, since its process began, where it read as working, at the time of each
    /// read.
    #[test]
    fn a_command_line_conversation_never_prompted_waits_at_its_box() {
        const HOUR: i64 = 3_600_000;
        let a = AccountCfg { name: "antigravity".into(), harness: "antigravity".into(), home: "~/.gemini/antigravity".into(), command: None, start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false, chrome_profile: None };
        let c = Conversation { id: "c5".into(), cli: true, status: "cascade_run_status_idle".into(), ..Default::default() };
        let ui = Process { pid: 41, conversation: Some("c5".into()), terminal: true, logs: Vec::new(), started: Some(NOW - 16 * HOUR) };
        let s = antigravity_session(&a, &c, Some(&ui), false, 0, NOW);
        assert_eq!((s.status, s.live, s.last_activity, s.excerpt.as_deref()), (Status::Idle, true, NOW - 16 * HOUR, None));
        assert_eq!(antigravity_session(&a, &c, Some(&ui), false, 0, NOW + HOUR).last_activity, NOW - 16 * HOUR);
        // A prompt goes in, as its log says, before a step or the row has a time: it works, from
        // then.
        let mut told = c.clone();
        (told.said.prompted, told.last_input) = (Some(("c5".into(), NOW - 1_000)), Some(NOW - 1_000));
        let s = antigravity_session(&a, &told, Some(&ui), false, 0, NOW);
        assert_eq!((s.status, s.last_activity, s.last_prompt_at), (Status::Working, NOW - 1_000, NOW - 1_000));
    }

    /// The four turns in three conversations that a dropped connection ended on 2026-10-14, the
    /// one the quota refused and the one a 404 ended: each says why where its last reply ("I'll
    /// proceed once they finish") read as if it had stopped by itself.
    #[test]
    fn an_antigravity_turn_that_failed_says_why() {
        use crate::antigravity::Failed;
        let a = AccountCfg { name: "antigravity".into(), harness: "antigravity".into(), home: "~/.gemini/antigravity".into(), command: None, start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false, chrome_profile: None };
        let ui = Process { pid: 41, conversation: Some("c3".into()), terminal: true, logs: Vec::new(), started: None };
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
}
