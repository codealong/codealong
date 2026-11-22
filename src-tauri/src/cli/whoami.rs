//! Which session runs this command (`find_me`; `session_running` has its rules, for the server's
//! answer to `account check` too): the live session among the processes above it, or the one
//! Codealong runs whose id they were given (`by_process`); else the one session whose unanswered
//! tool call is this command (`running_in`); else `CODEALONG_SESSION`, for a session the board
//! doesn't show yet. And `whoami`: that session, its worktree, who else runs there, the rest of
//! its project and its todos, which `whoami --context` hands a session as it starts.
//!
//! What each harness records of the commands it runs is its own to say (`Adapter::open_calls`,
//! `Adapter::runs_its_commands`, `harness::started_under`), not this file's.

use std::path::Path;

use anyhow::Result;
use serde::Serialize;

use crate::config::Config;
use crate::model::{Board, Harness, Lane, Origin, Session, Status, ELSEWHERE};
use crate::procs;
use crate::start::{self, lane_for_path};
use crate::todos::{self, Todo};
use crate::util::{now_ms, tilde};

use super::*;

// ---- whoami ----

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Whoami {
    found: bool,
    /// `process` (a parent process is a session the board knows, or was given the ID of a
    /// session Codealong runs), `record` (a session's unanswered tool call runs this command) or
    /// `environment` (CODEALONG_SESSION was set by the app's start).
    how: Option<String>,
    session: Option<SessionRow>,
    worktree: Option<LaneRow>,
    /// The other agents running in the worktree now (`agents_in`), this session aside. None
    /// means no other agent shares the checkout, however many sessions worked there before.
    others: Vec<SessionRow>,
    /// Open todos linked to the session, then open ones linked to the worktree when no other
    /// agent is running there (`my_todos`).
    todos: Vec<TodoRow>,
    /// The rest of the worktree's project: the main checkout of each of its other repos, and
    /// its folders (`project_folders`).
    project_folders: Vec<FolderRow>,
}

/// One of the places in the project `whoami`'s worktree is in, beside that worktree.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FolderRow {
    path: String,
    /// The repo's name, or the folder's.
    repo: String,
    /// `owner/name` on GitHub.
    #[serde(skip_serializing_if = "Option::is_none")]
    slug: Option<String>,
    /// A folder the project holds that isn't a repo.
    folder: bool,
}

fn folder_row(l: &Lane) -> FolderRow {
    FolderRow { path: l.path.clone(), repo: l.repo.clone(), slug: l.repo_slug.clone(), folder: l.origin == Origin::Folder }
}

/// The session running this command: the parent process that's a live session on the board, or
/// a session Codealong runs whose ID a process under the app was given (`hosted_above`), else
/// the session whose unanswered tool call runs this command (`running_in`), else
/// `CODEALONG_SESSION` from a start the app made, for a session the board hasn't shown yet.
/// The variable comes last and only for such a session: a terminal keeps it after its session
/// ends, and long-lived processes started from one (Claude Code's background service,
/// OpenCode's) pass whichever session's variable they had on to everything they run.
pub(super) fn find_me(c: &mut Client) -> Result<(Option<String>, Option<&'static str>)> {
    if let Some(me) = &c.me {
        return Ok(me.clone());
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    let var = std::env::var(start::SESSION_VAR).ok();
    let cfg = c.cfg()?.clone();
    let me = session_running(c.raw_board()?, &cfg, std::process::id(), &args, var.as_deref(), now_ms());
    c.me = Some(me.clone());
    Ok(me)
}

/// `find_me`'s rules for the command `pid`, run with `args`, given its `CODEALONG_SESSION`
/// (`var`): the server's answer to `account check` too, by the process at its socket's other end.
pub(crate) fn session_running(board: &Board, cfg: &Config, pid: u32, args: &[String], var: Option<&str>, now: i64) -> (Option<String>, Option<&'static str>) {
    if let Some(key) = by_process(board, &chain_of(pid)) {
        return (Some(key), Some("process"));
    }
    if let Some(key) = running_in(board, cfg, args, now) {
        return (Some(key), Some("record"));
    }
    match var {
        Some(key) if !key.trim().is_empty() && !all_sessions(board).iter().any(|(s, _)| s.key == key) => (Some(key.to_string()), Some("environment")),
        _ => (None, None),
    }
}

/// The processes from `pid` up, each with its name.
pub(super) fn chain_of(pid: u32) -> Vec<(u32, String)> {
    procs::ancestors(pid).into_iter().filter_map(|p| procs::parent_and_name(p).map(|(_, name)| (p, name))).collect()
}

/// The live session above a chain of processes, or the hosted one it runs under.
pub(super) fn by_process(board: &Board, chain: &[(u32, String)]) -> Option<String> {
    let live: Vec<&Session> = all_sessions(board).into_iter().map(|(s, _)| s).filter(|s| s.live).collect();
    session_above(chain, &live).or_else(|| hosted_above(chain, &live, |p| procs::environment(p).map(|env| crate::harness::started_under(&env)).unwrap_or_default()))
}

/// The live session nearest above this command, given its chain of processes (`chain`, this
/// command first, each with its name): a session started from another's shell (`claude -p` run
/// by an agent) is inside it. The chain is read only as far as Codealong's own app, a process of
/// the same program as this command: what the app runs (a helper through its Codex service) isn't
/// run by the session whose shell started the app, which it would otherwise be taken for once the
/// app was opened from an agent's terminal (2026-10-14).
fn session_above(chain: &[(u32, String)], live: &[&Session]) -> Option<String> {
    let (_, me) = chain.first()?;
    chain.iter().skip(1).take_while(|(_, name)| name != me).find_map(|(p, _)| live.iter().find(|s| s.pid == Some(*p))).map(|s| s.key.clone())
}

/// The session Codealong runs itself (`Session::hosted`) that this command runs under, which
/// `session_above` can't find: the board gives such a session no process (acp.rs `overlay`). Its
/// harness gives what it runs the session's ID in their environment (`Adapter::session_var`, read
/// from a process's by `under`), so it's the nearest process of the chain below Codealong's app
/// whose environment names a live hosted session. A command not under the app isn't one's, and
/// the IDs the app itself started with are passed on to everything the app runs, beside the
/// session's own (a Codex thread's commands keep a `CLAUDE_CODE_SESSION_ID` the app had). Before
/// this, such a session was found only by its record of the command, which a Claude Code session
/// Codealong ran had often not written yet when a command run at once read it, so `start
/// --helper` and `session done` refused the session as none (2026-10-16); and a Codex helper
/// that ran `session send` from a script had no call that was the command, so its message to its
/// parent went as nobody's and was held for that session's user (2026-10-27).
fn hosted_above(chain: &[(u32, String)], live: &[&Session], under: impl Fn(u32) -> Vec<(Harness, String)>) -> Option<String> {
    let (_, me) = chain.first()?;
    let app = 1 + chain.iter().skip(1).position(|(_, name)| name == me)?;
    let inherited = under(chain[app].0);
    chain[..app]
        .iter()
        .flat_map(|(p, _)| under(*p))
        .filter(|named| !inherited.contains(named))
        .find_map(|(harness, id)| live.iter().find(|s| s.hosted && s.harness == harness && s.id == id))
        .map(|s| s.key.clone())
}

/// How recently a session that isn't in a turn by the board's last look was active, for it to
/// be running this command anyway: the board can lag a turn that just started.
const RUNNING_MS: i64 = 5 * 60 * 1000;

/// The session whose unanswered tool call runs the command with `args`, when exactly one has such a call:
/// how a session is found when no process above this one is a session the board knows. The
/// Codex app runs every thread's commands under one process, and OpenCode runs its tools in its
/// service; each writes a call to its record before running it. A live session whose commands
/// are its own process's children isn't asked: that process isn't above this one. Otherwise an
/// agent that hands another the command in a prompt would match as well.
fn running_in(board: &Board, cfg: &Config, args: &[String], now: i64) -> Option<String> {
    let hits: Vec<String> = all_sessions(board)
        .into_iter()
        .map(|(s, _)| s)
        .filter(|s| may_run_this(s, now))
        .filter(|s| {
            let Some(account) = cfg.accounts.iter().find(|a| a.name == s.account) else { return false };
            crate::harness::adapter(s.harness).open_calls(&crate::harness::Ctx { account }, s).iter().any(|call| runs_command(call, args))
        })
        .map(|s| s.key.clone())
        .collect();
    match hits.as_slice() {
        [one] => Some(one.clone()),
        _ => None,
    }
}

/// Whether a session could be running this command although no process above it is the
/// session's: it's in a turn, or was active a moment ago, and its commands don't run under a
/// process of its own that the board knows.
fn may_run_this(s: &Session, now: i64) -> bool {
    let turn = matches!(s.status, Status::Working | Status::Needs) || now - s.last_activity < RUNNING_MS;
    !s.archived && turn && !(s.live && s.pid.is_some() && crate::harness::adapter(s.harness).runs_its_commands())
}

/// Whether a recorded tool call runs this command, given its arguments: `codealong` as a
/// command (not a folder such as `projects/codealong`) followed by the subcommand, past any
/// options, and every one-word argument (ids, options, values) somewhere in the call. Free
/// text is left out, since quoting and escapes change how it's written; the words may sit
/// apart, as in a loop over ids.
fn runs_command(call: &str, args: &[String]) -> bool {
    let words: Vec<&str> = args.iter().map(String::as_str).filter(|a| !a.is_empty() && a.is_ascii() && !a.contains(|c: char| c.is_whitespace() || "\"'`\\$".contains(c))).collect();
    let Some(sub) = words.iter().find(|w| !w.starts_with('-')) else { return false };
    let invoked = call.match_indices("codealong").any(|(i, m)| {
        // Past a quote that closed the command's path, escaped or not.
        let rest = call[i + m.len()..].trim_start_matches(['"', '\'', '\\']);
        let next = rest.split_whitespace().find(|w| !w.starts_with('-'));
        rest.starts_with(char::is_whitespace) && next.and_then(|w| w.strip_prefix(*sub)).is_some_and(|after| !after.starts_with(|c: char| c.is_ascii_alphanumeric() || c == '-' || c == '_'))
    });
    invoked && words.iter().all(|w| call.contains(w))
}

pub(super) fn whoami(c: &mut Client, context: bool, json: bool) -> Result<()> {
    let (key, how) = find_me(c)?;
    let cwd = std::env::current_dir().ok();
    let board = c.board()?;
    let session = key.as_deref().and_then(|k| all_sessions(board).into_iter().find(|(s, _)| s.key == k));
    let lane = session.map(|(_, l)| l).or_else(|| cwd.as_deref().and_then(|d| lane_for_path(board, d)));
    let out = Whoami {
        found: session.is_some(),
        how: session.and(how).map(str::to_string),
        session: session.map(|(s, l)| session_row(s, l, &board.todos)),
        worktree: lane.map(|l| lane_row(l, board)),
        others: lane.map(|l| others_in(l, key.as_deref()).map(|s| session_row(s, l, &board.todos)).collect()).unwrap_or_default(),
        todos: my_todos(board, key.as_deref(), lane).into_iter().map(|t| todo_row(t, &store_of(board), &board.lanes, false)).collect(),
        project_folders: lane.map(|l| project_folders(board, l).into_iter().map(folder_row).collect()).unwrap_or_default(),
    };
    // Before JSON: a hook reads the command's output through a pipe.
    if context {
        print!("{}", context_text(&out, key.as_deref()));
        return Ok(());
    }
    if json {
        return print_json(&out);
    }
    match &out.session {
        Some(s) => {
            let by = match out.how.as_deref() {
                Some("process") => "its process",
                Some("record") => "its own record of this command",
                _ => "CODEALONG_SESSION",
            };
            println!("Session {} ({} on {}, {}), found by {by}.", s.key, s.harness, s.account, s.status)
        }
        None => println!("No session of this shell is on the board.{}", if key.is_some() { " CODEALONG_SESSION names one the board doesn't show yet." } else { "" }),
    }
    if let Some(w) = &out.worktree {
        println!("Worktree {} ({}{}){}", tilde(Path::new(&w.id)), w.callsign, w.branch.as_deref().map(|b| format!(", {b}")).unwrap_or_default(), w.pr.map(|n| format!(", PR #{n}")).unwrap_or_default());
        // The chats that ended in an app's chats folder were never this one's neighbours.
        let ended = if lane.is_some_and(|l| l.chats) { 0 } else { w.sessions.saturating_sub(w.agents) };
        println!("{}", others_text(&out.others, ended));
        if let Some(line) = folders_text(&w.project, &out.project_folders, usize::MAX) {
            println!("{line}");
        }
    }
    for t in &out.todos {
        let mine = key.as_deref().is_some_and(|k| t.sessions.iter().any(|s| s == k));
        println!("Todo {} {} ({}{})", t.id, t.title, status_text(t), if mine { "" } else { ", linked to the worktree" });
    }
    Ok(())
}

/// The agents running in a worktree other than the caller, as `changes` tells a worktree's
/// agents (`shared`): none in an app's chats folder (`Lane::chats`), whose chats are there
/// together for having no folder of yours, not for working with you; and no headless run, which
/// a script or an agent runs and reads itself.
pub(super) fn others_in<'a>(l: &'a Lane, key: Option<&'a str>) -> impl Iterator<Item = &'a Session> {
    agents_in(l).filter(move |s| !l.chats && !s.headless && Some(s.key.as_str()) != key)
}

/// `whoami`'s line under the worktree: the other agents running there, or that there are none
/// and how many sessions there have ended.
pub(super) fn others_text(others: &[SessionRow], ended: usize) -> String {
    if others.is_empty() {
        let before = match ended {
            0 => String::new(),
            1 => " (1 earlier session has ended)".to_string(),
            n => format!(" ({n} earlier sessions have ended)"),
        };
        return format!("No other agent is running here{before}.");
    }
    others.iter().map(|s| format!("Also running here: {} ({} on {}, {}) {}", s.key, s.harness, s.account, s.status, s.title)).collect::<Vec<_>>().join("\n")
}

/// The todos `whoami` names: the open ones linked to the session, then the open ones linked to
/// its worktree when no other agent is running there. In a checkout several agents share, the
/// worktree's todos are the others' work. When the caller isn't known, one agent running there
/// may be the caller itself.
pub(super) fn my_todos<'a>(board: &'a Board, key: Option<&str>, lane: Option<&Lane>) -> Vec<&'a Todo> {
    let open = |t: &&Todo| todos::is_open(t);
    let mut out: Vec<&Todo> = board.todos.iter().filter(open).filter(|t| key.is_some_and(|k| t.sessions.iter().any(|s| s == k))).collect();
    let alone = lane.filter(|l| others_in(l, key).count() <= usize::from(key.is_none()));
    if let Some(l) = alone {
        let more: Vec<&Todo> = board.todos.iter().filter(open).filter(|t| t.worktrees.contains(&l.id) && !out.iter().any(|x| x.id == t.id)).collect();
        out.extend(more);
    }
    out
}

/// The rest of `lane`'s project, for a session working there to know where it is: the main
/// checkout of each of the project's other repos, and each folder it holds that isn't a repo,
/// the most recently active first. A session told only of its own worktree didn't know the
/// project's other repos were on this Mac (2026-11-22). Their other worktrees aren't named
/// (asked 2026-11-22: main checkouts only), and a worktree in none of your projects
/// (Elsewhere) or an app's chats folder has no project to name.
pub(super) fn project_folders<'a>(board: &'a Board, lane: &Lane) -> Vec<&'a Lane> {
    if lane.project == ELSEWHERE || lane.chats {
        return Vec::new();
    }
    // Two clones of a repo are the one repo; repos of one name under two owners are two.
    let same_repo = |l: &Lane| match (&l.repo_slug, &lane.repo_slug) {
        _ if l.id == lane.id => true,
        (Some(a), Some(b)) => a.eq_ignore_ascii_case(b),
        (None, None) => l.origin != Origin::Folder && lane.origin != Origin::Folder && l.repo == lane.repo,
        _ => false,
    };
    let mut out: Vec<&Lane> = board.lanes.iter().filter(|l| l.project == lane.project && !l.chats && matches!(l.origin, Origin::Main | Origin::Folder) && !same_repo(l)).collect();
    out.sort_by(|a, b| b.last_activity.cmp(&a.last_activity).then_with(|| a.path.cmp(&b.path)));
    out
}

/// How many of the project's other places `whoami --context` names; the rest it counts.
const FOLDERS_NAMED: usize = 8;

/// The line naming the rest of `project` (`project_folders`), `limit` of them at most, or none
/// when there's nothing else in it.
fn folders_text(project: &str, folders: &[FolderRow], limit: usize) -> Option<String> {
    let (repos, plain) = (folders.iter().any(|f| !f.folder), folders.iter().any(|f| f.folder));
    let what = match (repos, plain) {
        (true, true) => format!("The main checkouts of project {project}'s other repos, and its folders"),
        (true, false) => format!("The main checkouts of project {project}'s other repos"),
        (false, true) => format!("Project {project}'s folders"),
        (false, false) => return None,
    };
    let named: Vec<String> = folders.iter().take(limit).map(|f| match (&f.slug, f.folder) {
        (Some(slug), false) => format!("{} ({slug})", tilde(Path::new(&f.path))),
        _ => tilde(Path::new(&f.path)),
    }).collect();
    let more = match folders.len().saturating_sub(limit) {
        0 => String::new(),
        n => format!(", and {n} more (`codealong whoami` lists them)"),
    };
    Some(format!("{what}: {}{more}.", named.join(", ")))
}

/// What `whoami --context` adds to a session's context as it starts: which session it is, where
/// the rest of its project is, and its todos. `key` is the session's even when the board doesn't
/// show it yet, as happens for a session the app just started.
fn context_text(out: &Whoami, key: Option<&str>) -> String {
    let mut lines = Vec::new();
    if let Some(s) = &out.session {
        lines.push(format!("This is Codealong session {} ({} on {}), in worktree {}.", s.key, s.harness, s.account, tilde(Path::new(&s.worktree))));
    }
    if let Some(line) = out.worktree.as_ref().and_then(|w| folders_text(&w.project, &out.project_folders, FOLDERS_NAMED)) {
        lines.push(format!("{line} Read them there when the work needs them, and before changing one, read the instructions at its root (CLAUDE.md, AGENTS.md)."));
    }
    for t in &out.todos {
        if key.is_some_and(|k| t.sessions.iter().any(|s| s == k)) {
            let done = if t.status == "in-progress" { format!(" When it's ready for the user to review, say so with `codealong todo review {id}`. When the work is merged or the user says it's done, mark it done with `codealong todo done {id}`.", id = t.id) } else { String::new() };
            lines.push(format!("Todo {} \"{}\" is linked to this session ({}).{done}", t.id, t.title, status_text(t)));
        } else {
            lines.push(format!("Todo {} \"{}\" is linked to this worktree ({}). If you take it on, `codealong todo start {}` links this session to it.", t.id, t.title, status_text(t), t.id));
        }
    }
    lines.into_iter().map(|l| l + "\n").collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::tests::{session, todo};
    use crate::fixtures::lane;
    use crate::todos::TodoStatus;

    /// A Codex helper's command, run under the app opened from a Claude session's terminal, was
    /// taken for that session's: its byline named it and the reply went there (2026-10-14).
    #[test]
    fn a_command_is_a_sessions_only_up_to_codealongs_own_app() {
        let at = |key: &str, pid: u32| Session { pid: Some(pid), ..session(key, true, None) };
        let (setup, agent) = (at("claude:claude:setup", 100), at("claude:claude:agent", 300));
        let live = [&setup, &agent];
        let chain = |links: &[(u32, &str)]| links.iter().map(|(p, n)| (*p, n.to_string())).collect::<Vec<_>>();
        // Under the app's Codex service, which the app runs, under the shell of the session that opened it.
        let helper = chain(&[(50, "codealong"), (51, "zsh"), (52, "codex"), (53, "node"), (60, "codealong"), (61, "zsh"), (100, "claude")]);
        assert_eq!(session_above(&helper, &live), None);
        // A session's own shell finds it, a `claude -p` started from another's the nearer one.
        assert_eq!(session_above(&chain(&[(50, "codealong"), (51, "zsh"), (300, "claude"), (60, "codealong")]), &live).as_deref(), Some("claude:claude:agent"));
        assert_eq!(session_above(&chain(&[(50, "codealong"), (51, "zsh"), (300, "claude"), (52, "zsh"), (100, "claude")]), &live).as_deref(), Some("claude:claude:agent"));
    }

    /// A Claude Code session Codealong ran over ACP was found only by its record of the command,
    /// which it hadn't written yet when `start --helper` read it, so the helper was refused as
    /// started from no session (2026-10-16). The board gives it no process; its shells carry its ID.
    #[test]
    fn a_session_codealong_runs_is_found_by_the_id_its_shell_was_given_under_the_app() {
        let with_id = |key: &str| Session { id: key.rsplit(':').next().unwrap().to_string(), ..session(key, true, None) };
        let hosted = |key: &str| Session { hosted: true, pid: None, ..with_id(key) };
        let (mine, other) = (hosted("claude:claude:mine"), hosted("claude:claude:other"));
        let terminal = Session { pid: Some(400), ..with_id("claude:claude:term") };
        let live = [&mine, &other, &terminal];
        let chain = |links: &[(u32, &str)]| links.iter().map(|(p, n)| (*p, n.to_string())).collect::<Vec<_>>();
        // This command, the shell its session's claude started, that claude, the ACP adapter, the app.
        let under_app = chain(&[(50, "codealong"), (51, "zsh"), (52, "claude"), (53, "node"), (60, "codealong")]);
        let named = |env: &'static [(u32, &'static str)]| move |p: u32| env.iter().filter(|(q, _)| *q == p).map(|(_, id)| (Harness::Claude, id.to_string())).collect::<Vec<_>>();
        // Apple's zsh shows no environment; the command's own names the session.
        assert_eq!(hosted_above(&under_app, &live, named(&[(50, "mine")])).as_deref(), Some("claude:claude:mine"));
        // A `claude -p` run from its shell (54) names its own ID to what it runs, which isn't on
        // the board, and was started with the session's.
        let nested = chain(&[(50, "codealong"), (51, "zsh"), (54, "claude"), (55, "zsh"), (52, "claude"), (53, "node"), (60, "codealong")]);
        assert_eq!(hosted_above(&nested, &live, named(&[(50, "-p run"), (54, "mine")])).as_deref(), Some("claude:claude:mine"));
        // The app opened from a session's shell passes that ID to everything it runs, a Codex
        // helper's commands included.
        assert_eq!(hosted_above(&under_app, &live, named(&[(50, "other"), (53, "other"), (60, "other")])), None);
        // Not under the app, it isn't a hosted session's, and only a hosted one is found so.
        assert_eq!(hosted_above(&chain(&[(50, "codealong"), (51, "zsh"), (52, "claude")]), &live, named(&[(50, "mine")])), None);
        assert_eq!(hosted_above(&under_app, &live, named(&[(50, "term")])), None);
        // One that's gone from the board, or no longer live, isn't found.
        assert_eq!(hosted_above(&under_app, &[&other], named(&[(50, "mine")])), None);
    }

    /// A Codex helper Codealong ran sent its review to its parent from a script (`python3 -`
    /// calling `subprocess.run`), so no call in its rollout was the `codealong` command and the
    /// message went as nobody's, held for the parent's user (2026-10-27). Codex gives a thread's
    /// commands its ID, beside what its app-server inherited from the app.
    #[test]
    fn a_codex_thread_codealong_runs_is_found_by_its_id_through_a_wrapper() {
        let hosted = |harness: Harness, key: &str| Session { id: key.rsplit(':').next().unwrap().to_string(), harness, hosted: true, pid: None, ..session(key, true, None) };
        let (helper, boss) = (hosted(Harness::Codex, "codex:codex:helper"), hosted(Harness::Claude, "claude:claude:boss"));
        let live = [&helper, &boss];
        let chain = |links: &[(u32, &str)]| links.iter().map(|(p, n)| (*p, n.to_string())).collect::<Vec<_>>();
        // This command, the script, Codex's shell, its app-server, codex-acp, the app.
        let wrapped = chain(&[(50, "codealong"), (51, "python3"), (52, "bash"), (53, "codex"), (54, "node"), (60, "codealong")]);
        let env = |p: u32| match p {
            50 | 51 => vec![(Harness::Claude, "boss".to_string()), (Harness::Codex, "helper".to_string())],
            53 | 54 | 60 => vec![(Harness::Claude, "boss".to_string())],
            _ => vec![],
        };
        // The app was opened from the boss's shell, whose ID everything it runs keeps.
        assert_eq!(hosted_above(&wrapped, &live, env).as_deref(), Some("codex:codex:helper"));
        // Opened the usual way, the app has none.
        let fresh = |p: u32| if p <= 51 { vec![(Harness::Codex, "helper".to_string())] } else { vec![] };
        assert_eq!(hosted_above(&wrapped, &live, fresh).as_deref(), Some("codex:codex:helper"));
        // A thread's ID names only a Codex session.
        let claude_id = |p: u32| if p <= 51 { vec![(Harness::Claude, "helper".to_string())] } else { vec![] };
        assert_eq!(hosted_above(&wrapped, &live, claude_id), None);
    }

    /// The main checkout, where this session and another live one work, and a worktree of its
    /// own for a second session, with an ended session and a live visitor.
    fn shared_and_own(other_live: bool) -> Board {
        let todos = vec![
            todo("a", TodoStatus::InProgress, &["me"], &["/r/main"]),
            todo("b", TodoStatus::Done, &["me"], &[]),
            todo("c", TodoStatus::InProgress, &["other"], &["/r/main"]),
            todo("d", TodoStatus::Todo, &[], &["/r/wt"]),
            todo("e", TodoStatus::Done, &["gone"], &["/r/wt"]),
        ];
        let lanes = vec![
            lane("/r/main", vec![session("me", true, None), session("other", other_live, None), session("old", false, None)]),
            lane("/r/wt", vec![session("me2", true, None), session("gone", false, None), session("other", true, Some("worked here"))]),
        ];
        Board { lanes, todos, ..Board::default() }
    }

    #[test]
    fn whoami_names_its_own_open_todos_and_a_worktrees_only_when_no_one_else_is_there() {
        let ids = |ts: Vec<&Todo>| ts.iter().map(|t| t.id.clone()).collect::<Vec<_>>();
        let b = shared_and_own(true);
        let (main, wt) = (&b.lanes[0], &b.lanes[1]);
        // Its done todo is left out, and so is the shared checkout's other todo.
        assert_eq!(ids(my_todos(&b, Some("me"), Some(main))), ["a"]);
        // A worktree of its own: ended sessions and visitors don't share it; done todos stay out.
        assert_eq!(ids(my_todos(&b, Some("me2"), Some(wt))), ["d"]);
        // A caller that isn't known may be the one live session there, but not one of two.
        assert_eq!(ids(my_todos(&b, None, Some(wt))), ["d"]);
        assert!(my_todos(&b, None, Some(main)).is_empty());
        assert!(my_todos(&b, None, None).is_empty());
        // Once the other session ends, the checkout's todos are this one's to see.
        let b = shared_and_own(false);
        assert_eq!(ids(my_todos(&b, Some("me"), Some(&b.lanes[0]))), ["a", "c"]);
    }

    /// Antigravity's command line runs every chat started with no folder in one scratch folder,
    /// and `whoami` named the unrelated chats there as running beside a session (2026-10-14).
    #[test]
    fn an_apps_chats_and_headless_runs_are_no_one_running_beside_you() {
        let with = |key: &str| Session { status: Status::Working, ..session(key, true, None) };
        let chats = Lane { chats: true, ..lane("/Users/me/.gemini/antigravity-cli/scratch", vec![with("me"), with("plan-review"), with("harness-test")]) };
        assert_eq!(others_in(&chats, Some("me")).count(), 0);
        let b = Board { lanes: vec![chats], todos: vec![todo("t", TodoStatus::Todo, &[], &["/Users/me/.gemini/antigravity-cli/scratch"])], ..Board::default() };
        assert_eq!(my_todos(&b, Some("me"), Some(&b.lanes[0])).len(), 1);
        let runs = lane("/r/evals", vec![with("me"), Session { headless: true, ..with("judge") }, with("neighbour")]);
        assert_eq!(others_in(&runs, Some("me")).map(|s| s.key.as_str()).collect::<Vec<_>>(), ["neighbour"]);
    }

    #[test]
    fn a_starting_session_is_told_which_todos_are_its_own() {
        let b = shared_and_own(false);
        let (main, store) = (&b.lanes[0], store_of(&b));
        let rows = |ids: &[&str]| ids.iter().map(|id| todo_row(b.todos.iter().find(|t| t.id == *id).unwrap(), &store, &b.lanes, false)).collect::<Vec<_>>();
        let me = &main.sessions[0];
        let out = Whoami { found: true, how: Some("process".into()), session: Some(session_row(me, main, &b.todos)), worktree: Some(lane_row(main, &b)), others: vec![], todos: rows(&["a", "c"]), project_folders: vec![] };
        assert_eq!(
            context_text(&out, Some("me")),
            "This is Codealong session me (claude on claude), in worktree /r/main.\n\
             Todo a \"A\" is linked to this session (in-progress). When it's ready for the user to review, say so with `codealong todo review a`. When the work is merged or the user says it's done, mark it done with `codealong todo done a`.\n\
             Todo c \"C\" is linked to this worktree (in-progress). If you take it on, `codealong todo start c` links this session to it.\n"
        );
        // A session the app just started isn't on the board yet, but its todo is its own.
        let out = Whoami { found: false, how: None, session: None, worktree: Some(lane_row(main, &b)), others: vec![], todos: rows(&["a"]), project_folders: vec![] };
        assert!(context_text(&out, Some("me")).starts_with("Todo a \"A\" is linked to this session"));
        let out = Whoami { found: false, how: None, session: None, worktree: None, others: vec![], todos: vec![], project_folders: vec![] };
        assert_eq!(context_text(&out, None), "");
    }

    /// A project of two repos and a folder, cloned and worked on in several places, beside
    /// another project and Elsewhere.
    fn northwind() -> Board {
        let at = |path: &str, project: &str, repo: &str, slug: Option<&str>, origin: Origin, active: i64| Lane { repo_slug: slug.map(String::from), origin, last_activity: active, ..Lane::new(path, project, repo) };
        let lanes = vec![
            at("/w/northwind-app", "Northwind", "app", Some("northwind/app"), Origin::Main, 5),
            at("/w/northwind-app/.claude/worktrees/fix", "Northwind", "app", Some("northwind/app"), Origin::ClaudeApp, 9),
            // A second clone of the app, its remote written in other capitals.
            at("/w/clones/app", "Northwind", "app", Some("Northwind/App"), Origin::Main, 7),
            at("/w/northwind-ios", "Northwind", "ios", Some("northwind/ios"), Origin::Main, 3),
            at("/w/northwind-ios/.codealong/worktrees/sync", "Northwind", "ios", Some("northwind/ios"), Origin::Other, 8),
            // A repo of the same name under another owner is another repo.
            at("/w/forks/app", "Northwind", "app", Some("kestrel/app"), Origin::Main, 1),
            at("/w/notes/northwind", "Northwind", "northwind", None, Origin::Folder, 4),
            at("/w/atlas", "Atlas", "atlas", Some("atlas/atlas"), Origin::Main, 6),
            at("/w/scratch", ELSEWHERE, "scratch", None, Origin::Folder, 2),
            at("/w/scratch-two", ELSEWHERE, "scratch-two", None, Origin::Folder, 2),
        ];
        Board { lanes, ..Board::default() }
    }

    /// A session told only of its own worktree didn't know the project's other repos were on
    /// this Mac, nor where (2026-11-22).
    #[test]
    fn a_session_is_told_where_the_rest_of_its_project_is() {
        let b = northwind();
        let paths = |i: usize| project_folders(&b, &b.lanes[i]).iter().map(|l| l.path.as_str()).collect::<Vec<_>>();
        // From a worktree of the app: the other repos' main checkouts and the folder, the latest
        // first; none of the app's own clones, and no repo's other worktrees.
        assert_eq!(paths(1), ["/w/notes/northwind", "/w/northwind-ios", "/w/forks/app"]);
        assert_eq!(paths(0), paths(2));
        // From the folder: every repo's main checkout, both clones of the app among them.
        assert_eq!(paths(6), ["/w/clones/app", "/w/northwind-app", "/w/northwind-ios", "/w/forks/app"]);
        // A project of one repo has nothing more, and Elsewhere is no project.
        assert!(paths(7).is_empty());
        assert!(paths(8).is_empty());
        let chats = Lane { chats: true, ..Lane::new("/w/chats", "Northwind", "chats") };
        assert!(project_folders(&b, &chats).is_empty());

        let wt = &b.lanes[1];
        let folders = project_folders(&b, wt).into_iter().map(folder_row).collect::<Vec<_>>();
        let out = Whoami { found: false, how: None, session: None, worktree: Some(lane_row(wt, &b)), others: vec![], todos: vec![], project_folders: folders };
        assert_eq!(
            context_text(&out, None),
            "The main checkouts of project Northwind's other repos, and its folders: /w/notes/northwind, /w/northwind-ios (northwind/ios), /w/forks/app (kestrel/app). \
             Read them there when the work needs them, and before changing one, read the instructions at its root (CLAUDE.md, AGENTS.md).\n"
        );
        // A long list names the latest and counts the rest; a project of repos alone, or of folders, says so.
        assert_eq!(folders_text("Northwind", &out.project_folders, 1).unwrap(), "The main checkouts of project Northwind's other repos, and its folders: /w/notes/northwind, and 2 more (`codealong whoami` lists them).");
        assert_eq!(folders_text("Northwind", &out.project_folders[1..], 8).unwrap(), "The main checkouts of project Northwind's other repos: /w/northwind-ios (northwind/ios), /w/forks/app (kestrel/app).");
        assert_eq!(folders_text("Northwind", &out.project_folders[..1], 8).unwrap(), "Project Northwind's folders: /w/notes/northwind.");
        assert_eq!(folders_text("Northwind", &[], 8), None);
    }

    #[test]
    fn only_sessions_without_a_process_of_their_own_above_this_one_are_asked() {
        let now = 10 * RUNNING_MS;
        let with = |harness: &str, status: &str, live: bool, pid: Option<u32>, active: i64| -> Session {
            let mut s = session("s", live, None);
            s.harness = serde_json::from_value(serde_json::json!(harness)).unwrap();
            s.status = serde_json::from_value(serde_json::json!(status)).unwrap();
            s.pid = pid;
            s.last_activity = active;
            s
        };
        // A Codex app thread in a turn has no process; OpenCode's tools run in its service,
        // whatever terminal shows the session.
        assert!(may_run_this(&with("codex", "working", true, None, 0), now));
        assert!(may_run_this(&with("opencode", "working", true, Some(42), 0), now));
        // Claude Code and a Codex terminal run commands under their own process, which the
        // process check already looked for.
        assert!(!may_run_this(&with("claude", "working", true, Some(42), now), now));
        assert!(!may_run_this(&with("codex", "working", true, Some(42), now), now));
        // Out of its turn and quiet for a while, or archived, it's running nothing.
        assert!(!may_run_this(&with("codex", "idle", false, None, now - RUNNING_MS - 1), now));
        assert!(may_run_this(&with("codex", "idle", false, None, now - 1000), now));
        assert!(!may_run_this(&Session { archived: true, ..with("codex", "working", true, None, now) }, now));
    }

    #[test]
    fn a_tool_call_runs_this_command_only_when_it_invokes_it() {
        let args = |line: &str| line.split(' ').map(String::from).collect::<Vec<_>>();
        let done = args("todo done t-abc");
        // Claude Code's command, with a `cd` first, or through the plugin's quoted shim.
        assert!(runs_command("cd /Users/me/projects/codealong-x && codealong todo done t-abc", &done));
        assert!(runs_command("\"${CLAUDE_PLUGIN_ROOT}/bin/codealong\" --json todo done t-abc", &done));
        assert!(runs_command("/Users/me/codealong-target/debug/codealong todo done t-abc", &done));
        // A loop puts the id before the command.
        assert!(runs_command("for id in t-abc t-def; do codealong todo done $id; done", &done));
        // Codex's code mode, escaped in a string, after other commands; OpenCode's input as JSON.
        assert!(runs_command(r#"text(await tools.exec_command({cmd:\"sed -n '1,20p' a.rs\ncodealong whoami\",\"max_output_tokens\":8000}));"#, &args("whoami")));
        assert!(runs_command(r#"{"command":"codealong todo start t-abc","description":"Start it"}"#, &args("todo start t-abc")));
        // Free text isn't matched word for word; ids and options are.
        let add = vec!["todo".to_string(), "update".into(), "t-abc".into(), "--title".into(), "It's \"done\"".into()];
        assert!(runs_command("codealong todo update t-abc --title 'It'\\''s \"done\"'", &add));
        // The repo's folder isn't the command; another id or subcommand isn't this call.
        assert!(!runs_command("cd ~/projects/codealong && grep -n whoami src-tauri/src/cli.rs", &args("whoami")));
        assert!(!runs_command("cat ~/Library/Application\\ Support/Codealong/codealong.sock whoami", &args("whoami")));
        assert!(!runs_command("codealong todo done t-other", &done));
        assert!(!runs_command("codealong todo start t-abc", &done));
        assert!(!runs_command("codealong whoamix", &args("whoami")));
        assert!(!runs_command("anything", &[]));
    }
}
