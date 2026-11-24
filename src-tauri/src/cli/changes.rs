//! `codealong changes`: what changed on the board since an earlier look (changes.rs), for an
//! agent that keeps watch over the sessions on a loop. A look ends with a cursor, which the next
//! one starts from (`--since`, or kept in `--cursor-file`).

use std::path::Path;

use anyhow::{anyhow, Context, Result};

use crate::actions::params;
use crate::api;
use crate::util::{self, expand, now_ms, tilde, truncate};

use super::*;

// ---- changes ----

/// How long `changes` spends reading changed sessions' conversations, all told and on each: a
/// busy app answers slowly, and the board alone says where each stands.
const TEXTS_BUDGET: std::time::Duration = std::time::Duration::from_secs(30);
const TEXT_WAIT: std::time::Duration = std::time::Duration::from_secs(10);

pub(super) fn changes(c: &mut Client, a: &ChangesArgs, json: bool) -> Result<()> {
    use crate::changes::{self as ch, Since};
    let now = now_ms();
    let cursor_file = a.cursor_file.as_deref().map(expand);
    let saved = cursor_file.as_ref().and_then(|p| std::fs::read_to_string(p).ok()).map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    let asked = a.since.clone().or(saved).unwrap_or_else(|| "24h".into());
    let since = ch::parse_since(&asked, now).ok_or_else(|| anyhow!("--since is a cursor an earlier run printed, or like 90m, 24h or 3d, not {asked}."))?;
    let mut notes = Vec::new();
    let (earlier, since_ms) = match since {
        Since::Time(t) => (None, t),
        Since::Cursor(cursor) => match ch::earlier(&cursor)? {
            Some(look) => {
                let at = look.at;
                (Some(look), at)
            }
            None => {
                notes.push(format!("No look is kept for cursor {cursor} (each is kept a week, on the machine that took it), so this goes by times since then."));
                (None, cursor.parse().unwrap_or(now - util::DAY_MS))
            }
        },
    };
    let me = find_me(c)?.0;
    c.board()?;
    let (from_app, app_up) = (c.from_app, c.app.is_some());
    let board = c.board()?;
    let github = board.notices.iter().find(|n| n.key == "github").map(|n| n.title.clone());
    match (from_app, board.github_polled, github) {
        (false, _, _) if app_up => notes.push("PRs: the app didn't answer, so they're told next time.".into()),
        (false, _, _) => notes.push("PRs: Codealong's server isn't running, so they're told once it is.".into()),
        (true, false, _) => notes.push("PRs: GitHub hasn't answered since the app started, so they're told once it has.".into()),
        (true, true, Some(title)) => notes.push(format!("PRs may be behind: {title}.")),
        _ => {}
    }
    match (board.loading, board.todos_unreadable) {
        (true, _) => notes.push("Todos: the app is still reading them after a start; nothing about them is told until it's done.".into()),
        (false, true) => notes.push("Todos: todos.json doesn't read, so they're told once it does.".into()),
        _ => {}
    }
    let todos = !board.loading && !board.todos_unreadable;
    // Asked by an orchestrator, what it keeps watch over: its project, or every project but those
    // other orchestrators keep watch over.
    let (project, skip) = match (a.project.clone(), me.as_deref().and_then(|k| board.session(k)).and_then(|(_, s)| s.orchestrator.clone())) {
        (Some(p), _) => (Some(p), Vec::new()),
        (None, Some(o)) => {
            let file = crate::schedules::load().unwrap_or_default();
            let skip = file.schedules.iter().find(|s| s.id == o.schedule).map(|s| crate::schedules::orchestrate::told_elsewhere(&file, s)).unwrap_or_default();
            (o.project, skip)
        }
        (None, None) => (None, Vec::new()),
    };
    let sight = ch::Sight { board, sessions: !board.loading, prs: from_app && board.github_polled, todos, me: me.as_deref(), project: project.as_deref(), skip: &skip };
    let (mut out, look) = ch::compare(&sight, earlier.as_ref(), since_ms, now);
    ch::untagged_and_unsuggested(&mut out, &crate::tags::current().gone, &crate::suggestions::current().gone);
    out.cursor = ch::keep(look)?;
    notes.append(&mut out.notes);
    out.notes = notes;

    // The conversations, while there's time: the board's excerpt stands for one that doesn't read.
    let started = std::time::Instant::now();
    let mut slow = 0;
    for row in out.sessions.iter_mut().take(a.texts) {
        // What a boundary group's session said isn't for a session outside it (boundary.rs).
        if c.veils(&row.key)? {
            continue;
        }
        let left = TEXTS_BUDGET.saturating_sub(started.elapsed());
        let page = match left.is_zero() {
            true => None,
            false => c.act_within(api::SESSION_PAGE, &row.key, params::Page { before: None, ..Default::default() }, None, left.min(TEXT_WAIT)).ok().and_then(|v| serde_json::from_value::<api::Page>(v).ok()),
        };
        match page {
            Some(page) => (row.prompt, row.prompt_from, row.reply) = ch::texts(&page.items),
            None => slow += 1,
        }
    }
    if slow > 0 {
        out.notes.push(format!("{slow} of the changed sessions' conversations didn't read in time; their excerpt says where they stand."));
    }
    // Saved last, so a run cut short is told again from the same look.
    if let Some(path) = &cursor_file {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).with_context(|| format!("making {}", dir.display()))?;
        }
        std::fs::write(path, format!("{}\n", out.cursor)).with_context(|| format!("saving the cursor in {}", path.display()))?;
    }
    if json {
        return print_json(&out);
    }
    print_changes(&out, now);
    Ok(())
}

fn print_changes(out: &crate::changes::Changes, now: i64) {
    let how = if out.compared { "by what was seen then" } else { "by times alone" };
    let when = match ago(now - out.since).as_str() {
        "now" => "a moment".to_string(),
        a => a.to_string(),
    };
    println!("Since {when} ago, {how}. Next time: codealong changes --since {}", out.cursor);
    for n in &out.notes {
        println!("Note: {n}");
    }
    if out.quiet() {
        println!("\nNo changes.");
    }
    if !out.sessions.is_empty() {
        println!("\nSessions");
        let rows = out.sessions.iter().map(|r| vec![word(&r.change), r.account.clone(), r.callsign.clone(), truncate(&r.title, 40), truncate(r.excerpt.as_deref().unwrap_or(""), 60), r.key.clone()]).collect();
        print!("{}", table_text(&["CHANGE", "ACCOUNT", "WORKTREE", "TITLE", "NOW", "KEY"], rows));
    }
    if !out.headless.is_empty() {
        println!("\nHeadless runs");
        let rows = out.headless.iter().map(|h| vec![h.runs.to_string(), h.working.to_string(), h.callsign.clone()]).collect();
        print!("{}", table_text(&["RUNS", "WORKING", "WORKTREE"], rows));
    }
    let bare: Vec<&crate::changes::WaitingRow> = out.waiting.iter().filter(|w| !w.suggested && !w.tagged).collect();
    if !out.waiting.is_empty() {
        println!("\nWaiting on you: {}, {} with no suggested replies or tag", out.waiting.len(), bare.len());
        let rows = bare.iter().map(|w| vec![word(&w.status), ago(now - w.since), w.callsign.clone(), truncate(&w.title, 50), w.key.clone()]).collect();
        print!("{}", table_text(&["STATUS", "FOR", "WORKTREE", "TITLE", "KEY"], rows));
    }
    if let Some(prs) = out.prs.as_ref().filter(|p| !p.is_empty()) {
        println!("\nPRs");
        let rows = prs.iter().map(|p| vec![word(&p.change), p.key.clone(), p.author.clone().unwrap_or_default(), truncate(&p.title, 60)]).collect();
        print!("{}", table_text(&["CHANGE", "PR", "AUTHOR", "TITLE"], rows));
    }
    if !out.todos.is_empty() {
        println!("\nTodos");
        let rows = out.todos.iter().map(|t| vec![word(&t.change), t.id.clone(), truncate(&t.title, 70)]).collect();
        print!("{}", table_text(&["CHANGE", "ID", "TITLE"], rows));
    }
    for s in &out.shared {
        let who: Vec<String> = s.sessions.iter().map(|x| format!("{} ({})", truncate(&x.title, 30), word(&x.status))).collect();
        println!("\nShared: {}{} has {} agents at work: {}", tilde(Path::new(&s.worktree)), if s.main { " (main checkout)" } else { "" }, s.sessions.len(), who.join(", "));
    }
    for c in &out.chatter {
        println!("\nChatter: {} ↔ {} exchanged {} messages ({} flips)", truncate(&c.a.title, 30), truncate(&c.b.title, 30), c.count, c.flips);
    }
}
