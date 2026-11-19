//! `codealong worktrees`, `worktree remove`, `preview` and `reuse`: the worktrees the board knows
//! (`LaneRow`), one removed once its work is done, the app a worktree serves, and whether a
//! worktree takes one piece of work after another.
//!
//! A worktree's agents are the sessions running in it now (`agents_in`), not every session that
//! worked there; `whoami` goes by the same. Removing one is the app's request (`Client::act`),
//! which a repo's own teardown can keep for minutes (`REMOVAL_WAIT`).

use std::path::Path;

use anyhow::{anyhow, Context, Result};
use serde::Serialize;

use crate::actions::{self, params};
use crate::api;
use crate::model::{Board, Lane, Origin, PreviewServer, Reuse, Session, Status};
use crate::util::{now_ms, tilde, truncate};

use super::*;

// ---- worktrees ----

/// How long the command waits for the app to remove a worktree: a repo's own teardown can take
/// minutes (cleanup.rs).
pub(super) const REMOVAL_WAIT: std::time::Duration = std::time::Duration::from_secs(600);

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct LaneRow {
    pub(super) id: String,
    project: String,
    repo: String,
    pub(super) callsign: String,
    pub(super) branch: Option<String>,
    bay: String,
    status: String,
    headline: String,
    pub(super) pr: Option<u64>,
    pr_state: Option<String>,
    pr_title: Option<String>,
    /// Agents running in it now (`agents_in`).
    pub(super) agents: usize,
    /// Of those, the ones working or waiting on you.
    working: usize,
    /// Every session that worked in it and isn't archived, running or not.
    pub(super) sessions: usize,
    last_activity: i64,
    pinned: bool,
    /// Ids of todos linked to it.
    todos: Vec<String>,
    /// Where its app opens (`codealong preview`), and whether a server in its folder serves it now.
    preview: Option<String>,
    preview_running: Option<bool>,
    /// What it holds, when it's reusable (`codealong reuse`): free, open, review or landed. None
    /// for a worktree kept for one piece of work.
    reuse: Option<String>,
    /// Whether it can be removed with nothing in it lost (`codealong worktree remove`), or why it
    /// stays. None until its git is read.
    cleanup: Option<crate::model::Cleanup>,
    /// Improve Codealong starts its sessions here (`Board::improve`): a checkout of Codealong's
    /// own source, its main one when the board has it.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    improve: bool,
}

/// Whether a session is running now: a process has it open (a terminal, the Claude app, a
/// background job), or it's in a turn or waiting on you to go on. A long-lived worktree
/// gathers dozens of sessions that ended days ago; counted as its agents, they made it look
/// crowded to an agent deciding whether to share it.
fn running(s: &Session) -> bool {
    s.live || matches!(s.status, Status::Working | Status::Needs)
}

/// The agents running in a worktree: its own sessions (not visitors from other worktrees)
/// that are running and not archived.
pub(super) fn agents_in(l: &Lane) -> impl Iterator<Item = &Session> {
    l.sessions.iter().filter(|s| s.visitor.is_none() && !s.archived && running(s))
}

pub(super) fn lane_row(l: &Lane, board: &Board) -> LaneRow {
    LaneRow {
        id: l.id.clone(),
        project: l.project.clone(),
        repo: l.repo.clone(),
        callsign: l.callsign.clone(),
        branch: l.branch.clone(),
        bay: lower_debug(&l.bay),
        status: lower_debug(&l.glyph),
        headline: l.headline.clone(),
        pr: l.pr.as_ref().map(|p| p.number),
        pr_state: l.pr.as_ref().map(|p| lower_debug(&p.state)),
        pr_title: l.pr.as_ref().map(|p| p.title.clone()),
        agents: agents_in(l).count(),
        working: agents_in(l).filter(|s| matches!(s.status, Status::Working | Status::Needs)).count(),
        sessions: l.sessions.iter().filter(|s| s.visitor.is_none() && !s.archived).count(),
        last_activity: l.last_activity,
        pinned: board.pinned_worktrees.contains(&l.id),
        todos: board.todos.iter().filter(|t| t.worktrees.contains(&l.id)).map(|t| t.id.clone()).collect(),
        preview: l.preview.as_ref().map(|p| p.url.clone()),
        preview_running: l.preview.as_ref().map(|p| p.server.is_some()),
        reuse: l.reuse.as_ref().map(lower_debug),
        cleanup: l.cleanup.clone(),
        improve: board.improve.lane_id.as_deref() == Some(l.id.as_str()),
    }
}

pub(super) fn worktrees(c: &mut Client, a: &WorktreesArgs, json: bool) -> Result<()> {
    let board = c.board()?;
    let rows: Vec<LaneRow> = board
        .lanes
        .iter()
        .filter(|l| (a.all || !l.hidden) && (!a.reusable || l.reuse.is_some()) && a.project.as_deref().map_or(true, |p| p.eq_ignore_ascii_case(&l.project)) && a.bay.as_deref().map_or(true, |b| b.eq_ignore_ascii_case(&lower_debug(&l.bay))))
        .map(|l| lane_row(l, board))
        .collect();
    if json {
        return print_json(&rows);
    }
    let now = now_ms();
    // Columns for reuse, cleaning up and previews only when a worktree has them.
    let reuse = rows.iter().any(|r| r.reuse.is_some());
    let cleanup = rows.iter().any(|r| r.cleanup.is_some());
    let previews = rows.iter().any(|r| r.preview.is_some());
    let table: Vec<Vec<String>> = rows
        .iter()
        .map(|r| {
            let mut row = vec![r.bay.clone(), r.callsign.clone(), r.branch.clone().unwrap_or_default(), r.pr.map(|n| format!("#{n} {}", r.pr_state.clone().unwrap_or_default())).unwrap_or_default(), r.agents.to_string()];
            if reuse {
                row.push(r.reuse.clone().unwrap_or_default());
            }
            if cleanup {
                row.push(match &r.cleanup {
                    Some(c) if c.ok => "can go".into(),
                    Some(c) => truncate(&c.why, 40),
                    None => String::new(),
                });
            }
            row.extend([ago(now - r.last_activity), truncate(&r.headline, 60)]);
            if previews {
                row.push(r.preview.as_deref().map(|u| format!("{}{}", crate::previews::short_url(u), if r.preview_running == Some(true) { "" } else { " (not running)" })).unwrap_or_default());
            }
            row.push(tilde(Path::new(&r.id)));
            row
        })
        .collect();
    let mut head = vec!["BAY", "WORKTREE", "BRANCH", "PR", "AGENTS"];
    if reuse {
        head.push("REUSE");
    }
    if cleanup {
        head.push("CLEAN UP");
    }
    head.extend(["AGO", "HEADLINE"]);
    if previews {
        head.push("PREVIEW");
    }
    head.push("PATH");
    print!("{}", table_text(&head, table));
    Ok(())
}

/// What `worktree remove` says, as JSON.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RemovedOut {
    worktree: String,
    dry_run: bool,
    #[serde(flatten)]
    removal: api::Removal,
}

/// Removes a worktree when nothing in it would be lost, or with `dry_run` says whether it can go
/// and what would run (cleanup.rs): through the app when it's up, as its menu does, else here. One
/// that stays is an error, unless only asked.
pub(super) fn worktree_remove(c: &mut Client, worktree: &str, dry_run: bool, json: bool) -> Result<()> {
    let id = resolve_worktree(c, worktree)?;
    orchestrator_guard(c, api::WORKTREE_REMOVE)?;
    let answer = c.act(api::WORKTREE_REMOVE, &id, params::Remove { dry_run, ..Default::default() }, None)?;
    let removal: api::Removal = serde_json::from_value(answer).context("The app's answer doesn't read")?;
    let stays = !removal.removed && !dry_run;
    if json {
        print_json(&RemovedOut { worktree: id, dry_run, removal })?;
    } else {
        println!("{}", removal.message);
        if !removal.steps.is_empty() && !stays {
            println!("{}", if dry_run { "Would run:" } else { "Ran:" });
            for step in &removal.steps {
                println!("  {step}");
            }
        }
    }
    if stays {
        std::process::exit(1);
    }
    Ok(())
}

// ---- preview ----

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PreviewOut {
    worktree: String,
    /// Where it opens; none when no URL is set and nothing in the worktree serves a page.
    url: Option<String>,
    /// The URL was set by hand; false when it's a page found serving there.
    set: bool,
    /// The port set with the URL.
    port: Option<u16>,
    /// A server in the worktree's folder serves it now.
    running: bool,
    server: Option<PreviewServer>,
    /// The Chrome profile it opens in; none opens it in the default browser.
    chrome_profile: Option<String>,
    /// The profile was set for the worktree, not taken from its project's `chrome_profile`.
    chrome_profile_set: bool,
}

pub(super) fn preview_show(c: &mut Client, worktree: &str, json: bool) -> Result<()> {
    let id = resolve_worktree(c, worktree)?;
    let lane = c.board()?.lanes.iter().find(|l| l.id == id).ok_or_else(|| anyhow!("That worktree just changed. Try again."))?;
    let p = lane.preview.as_ref();
    let out = PreviewOut {
        worktree: id.clone(),
        url: p.map(|p| p.url.clone()),
        set: p.is_some_and(|p| p.set),
        port: p.and_then(|p| p.port),
        running: p.is_some_and(|p| p.server.is_some()),
        server: p.and_then(|p| p.server.clone()),
        chrome_profile: p.and_then(|p| p.chrome_profile.clone()),
        chrome_profile_set: p.is_some_and(|p| p.chrome_profile_set),
    };
    if json {
        return print_json(&out);
    }
    let Some(p) = p else {
        println!("{} has no preview: no URL is set (`codealong preview set <url>`) and nothing in its folder serves a page.", tilde(Path::new(&id)));
        return Ok(());
    };
    println!("{}{}", p.url, if p.set { "" } else { " (found: a page served in the worktree)" });
    match &p.server {
        Some(s) => println!("Running: {} on port {}, pid {}, for {}{}", s.name, s.port, s.pid, ago(now_ms() - s.started_at), s.title.as_deref().map(|t| format!(" · {t}")).unwrap_or_default()),
        None => println!("Not running: nothing listening in {} serves it{}.", tilde(Path::new(&id)), p.port.map(|n| format!(" on port {n}")).unwrap_or_default()),
    }
    match &p.chrome_profile {
        Some(c) if p.chrome_profile_set => println!("Opens in Chrome's {c} profile, set for this worktree."),
        Some(c) => println!("Opens in Chrome's {c} profile, set for {} in config.toml.", lane.project),
        None => println!("Opens in the default browser."),
    }
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PreviewSetOut {
    worktree: String,
    url: Option<String>,
    port: Option<u16>,
    /// The worktree's own Chrome profile; none opens it in its project's, or the default browser.
    chrome_profile: Option<String>,
    /// False when it already was so.
    changed: bool,
}

/// Changes a worktree's URL, port and Chrome profile, or clears them (`previews::set`): through
/// the app when it's up, as its sheet does, else in the file, which the app watches.
pub(super) fn preview_set(c: &mut Client, worktree: &str, change: crate::previews::Change, json: bool) -> Result<()> {
    let id = resolve_worktree(c, worktree)?;
    let set: crate::previews::Set = serde_json::from_value(c.act(actions::PREVIEW, &id, change, None)?).context("reading what the change did")?;
    let (e, changed) = (set.preview.as_ref(), set.changed);
    let out = PreviewSetOut { worktree: id.clone(), url: e.map(|e| e.url.clone()), port: e.and_then(|e| e.port), chrome_profile: e.and_then(|e| e.chrome_profile.clone()), changed };
    if json {
        return print_json(&out);
    }
    let at = tilde(Path::new(&out.worktree));
    let profile = out.chrome_profile.as_deref().map(|p| format!(", in Chrome's {p} profile")).unwrap_or_default();
    match (&out.url, changed) {
        (Some(u), true) => println!("{at} opens at {u}{}{profile}.", out.port.map(|p| format!(", served on port {p}")).unwrap_or_default()),
        (Some(u), false) => println!("{at} already opens at {u}{profile}."),
        (None, true) => println!("Took the preview URL off {at}."),
        (None, false) => println!("{at} had no preview URL."),
    }
    Ok(())
}

// ---- reuse ----

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReuseOut {
    worktree: String,
    reusable: bool,
    /// How it was marked, by the menu or `codealong reuse on|off`; none when it goes by the
    /// default (a main checkout is reusable, the rest aren't).
    marked: Option<bool>,
    /// What it holds, when it's reusable: free, open, review or landed.
    state: Option<String>,
    /// Not marked, but its branch has carried several merged PRs: it looks like one that's
    /// reused.
    looks_reusable: bool,
    /// False when it already was so (`on` and `off`).
    #[serde(skip_serializing_if = "Option::is_none")]
    changed: Option<bool>,
}

/// What a reusable worktree's state says, after its path.
fn reuse_words(state: Reuse) -> &'static str {
    match state {
        Reuse::Free => "free: nothing uncommitted or unlanded and no agent at work, so new work can start there",
        Reuse::Open => "has work under way: new work that belongs with it can join it",
        Reuse::Review => "has its PR in review",
        Reuse::Landed => "has its work landed: new work starts once it's back on the default branch",
    }
}

fn reuse_out(c: &mut Client, id: &str) -> Result<ReuseOut> {
    let marked = crate::reusable::load()?.get(id);
    let lane = c.board()?.lanes.iter().find(|l| l.id == id).ok_or_else(|| anyhow!("That worktree just changed. Try again."))?;
    Ok(ReuseOut { worktree: id.to_string(), reusable: lane.reuse.is_some(), marked, state: lane.reuse.as_ref().map(lower_debug), looks_reusable: lane.looks_reusable, changed: None })
}

fn print_reuse(out: &ReuseOut, lane: Option<&Lane>) {
    let at = tilde(Path::new(&out.worktree));
    match lane.and_then(|l| l.reuse) {
        Some(state) => {
            // Not marked, it's reusable by default: a main checkout, or one that has been reused.
            let why = match (out.marked, lane) {
                (Some(_), _) => String::new(),
                (None, Some(l)) if l.origin == Origin::Main => " (a main checkout is, unless marked otherwise)".into(),
                (None, l) => format!(" (it has taken new work {} times, so it is unless marked otherwise)", l.and_then(|l| l.git.as_ref()).and_then(|g| g.new_work_count).unwrap_or(0)),
            };
            println!("{at} is reusable{why} and {}.", reuse_words(state))
        }
        None if out.marked == Some(true) => println!("{at} is marked reusable, but only a repo's worktree in one of your projects can be."),
        None => println!("{at} is kept for its own piece of work: Jev doesn't suggest new work there."),
    }
    if out.looks_reusable {
        println!("Its branch has carried several merged PRs, as a reused worktree's does: `codealong reuse on` makes it reusable.");
    }
}

pub(super) fn reuse_show(c: &mut Client, worktree: &str, json: bool) -> Result<()> {
    let id = resolve_worktree(c, worktree)?;
    let out = reuse_out(c, &id)?;
    if json {
        return print_json(&out);
    }
    print_reuse(&out, c.board()?.lanes.iter().find(|l| l.id == id));
    Ok(())
}

/// Marks a worktree reusable or not (`reusable::mark`): through the app when it's up, as its
/// menu does, else in the file, which the app watches.
pub(super) fn reuse_set(c: &mut Client, worktree: &str, on: bool, json: bool) -> Result<()> {
    let id = resolve_worktree(c, worktree)?;
    let done = c.act(actions::REUSABLE, &id, params::Reusable { on }, None)?;
    let changed = done["changed"].as_bool().unwrap_or(false);
    // What it holds comes from the app, which reads the file once the watcher says: `reuse
    // show` has it then.
    if json {
        return print_json(&ReuseOut { worktree: id.clone(), reusable: on, marked: Some(on), state: None, looks_reusable: false, changed: Some(changed) });
    }
    let at = tilde(Path::new(&id));
    match (on, changed) {
        (true, true) => println!("{at} is reusable now: new work can start there once its work lands, and Jev suggests it."),
        (true, false) => println!("{at} was reusable already."),
        (false, true) => println!("{at} is kept for its own piece of work now: Jev stops suggesting new work there."),
        (false, false) => println!("{at} was kept for its own piece of work already."),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::tests::{session, todo};
    use crate::fixtures::lane;
    use serde_json::json;
    use crate::todos::TodoStatus;

    #[test]
    fn a_worktrees_agents_are_the_sessions_running_in_it_not_every_one_that_worked_there() {
        let with = |key: &str, status: &str, live: bool| Session { status: serde_json::from_value(json!(status)).unwrap(), ..session(key, live, None) };
        let long_lived = |sessions: Vec<Session>| {
            let mut all = vec![with("me", "working", true), with("ended", "stopped", false), with("seen", "idle", false)];
            all.extend(sessions);
            // Put away, and a session from another worktree that worked here too.
            all.push(Session { archived: true, ..with("put-away", "idle", true) });
            all.push(session("visitor", true, Some("worked here")));
            Board { lanes: vec![lane("/r/long", all)], todos: vec![todo("t", TodoStatus::Todo, &[], &["/r/long"])], ..Board::default() }
        };
        // Alone in a checkout where sessions ended long ago: none of them is an agent there.
        let b = long_lived(vec![]);
        let l = &b.lanes[0];
        let row = lane_row(l, &b);
        assert_eq!((row.agents, row.working, row.sessions), (1, 1, 3));
        assert_eq!(others_in(l, Some("me")).count(), 0);
        assert_eq!(others_text(&[], row.sessions - row.agents), "No other agent is running here (2 earlier sessions have ended).");
        assert_eq!(others_text(&[], 0), "No other agent is running here.");
        assert_eq!(my_todos(&b, Some("me"), Some(l)).len(), 1, "the worktree's todo is this session's to see");
        // A terminal left open between turns, and a Codex thread waiting on an approval, whose
        // app has no process for it: both share the checkout.
        let b = long_lived(vec![with("open", "idle", true), with("asking", "needs", false)]);
        let l = &b.lanes[0];
        let row = lane_row(l, &b);
        assert_eq!((row.agents, row.working, row.sessions), (3, 2, 5));
        let others: Vec<SessionRow> = others_in(l, Some("me")).map(|s| session_row(s, l, &b.todos)).collect();
        assert_eq!(others_text(&others, row.sessions - row.agents), "Also running here: open (claude on claude, idle) Title open\nAlso running here: asking (claude on claude, needs) Title asking");
        assert!(my_todos(&b, Some("me"), Some(l)).is_empty(), "the worktree's todo may be theirs");
        // The worktree Improve Codealong starts in says so.
        assert!(!row.improve);
        let improve = Board { improve: crate::improve::Improve { lane_id: Some("/r/long".into()), ..Default::default() }, ..b };
        assert!(lane_row(&improve.lanes[0], &improve).improve);
    }
}
