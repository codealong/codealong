//! `codealong todo`: the todo list. `TodoRow` is a todo as it's printed, with the section the app
//! lists it under.
//!
//! Every change is a `TodoOp` through `Client::apply`: the engine makes it when the server runs,
//! and the file takes it under its lock when none does, so nothing here writes todos.json. An id
//! takes a unique prefix, or a link to the todo (`resolve_todo`). A status set from a session
//! links that session to the todo unless it's asked not to (`link_caller`), as far as it can: a
//! link that fails is a note, not an error.

use std::collections::HashSet;
use std::path::Path;

use anyhow::{anyhow, bail, Result};
use serde::Serialize;
use serde_json::Value;

use crate::attachments;
use crate::model::{Board, Lane};
use crate::start::lane_for_path;
use crate::todos::{self, Label, Todo, TodoOp, TodoPatch, TodoReview, TodoStatus, TodoStore, Waiting, LABEL_COLORS};
use crate::util::{self, base36, now_ms, one_line, tilde, truncate};

use super::*;

/// A todo's status for a line of text, with what it's marked as waiting for and when it's due.
pub(super) fn status_text(r: &TodoRow) -> String {
    let status = match (&r.waiting, &r.review) {
        (Some(w), _) if w.note.is_empty() => format!("{}, marked as waiting", r.status),
        (Some(w), _) => format!("{}, waiting for {}", r.status, w.note),
        (None, Some(v)) if v.note.is_empty() => format!("{}, ready for review", r.status),
        (None, Some(v)) => format!("{}, ready for review: {}", r.status, v.note),
        (None, None) if r.section == "review" => format!("{}, its PR in review", r.status),
        (None, None) => r.status.clone(),
    };
    match &r.due {
        Some(due) => format!("{status}, due {} ({due})", todos::due_phrase(due, util::today_local())),
        None => status,
    }
}

// ---- todos ----

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(super) struct TodoRow {
    pub(super) id: String,
    pub(super) title: String,
    pub(super) status: String,
    project: String,
    /// Label names.
    labels: Vec<String>,
    worktrees: Vec<String>,
    pub(super) sessions: Vec<String>,
    /// Its primary session's key: where the work on it goes on, or went on last when that session
    /// has stopped. Null when no session has been on it.
    primary: Option<String>,
    /// Ids of the todos it waits on.
    depends_on: Vec<String>,
    /// The ones of those not done yet: while any are, it's blocked.
    blocked_by: Vec<String>,
    /// Marked as waiting: `note` says on what, `since` when. Null when it isn't.
    waiting: Option<Waiting>,
    /// Marked as ready for review: `note` says what to look at, `since` when. Null when it isn't.
    review: Option<TodoReview>,
    /// Where the app lists it: in-progress, review, ready, waiting, done or archived.
    section: &'static str,
    /// When it was archived. Null when it isn't.
    archived_at: Option<i64>,
    /// The day it's due, as 2026-10-03, and how many days off that is on this Mac's clock:
    /// 0 today, below 0 once it's past. Null when it has none.
    due: Option<String>,
    due_in_days: Option<i64>,
    created_at: i64,
    updated_at: i64,
    done_at: Option<i64>,
    /// Titles of the todos it waits on, and ids and titles of the todos waiting on it. Only on `show`.
    #[serde(skip_serializing_if = "Option::is_none")]
    waits_on: Option<Vec<TodoRef>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    needed_by: Option<Vec<TodoRef>>,
    /// The description, with attachments as full paths. Only on `show`.
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    attachments: Option<Vec<String>>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct TodoRef {
    id: String,
    title: String,
    status: String,
}

fn todo_ref(t: &Todo) -> TodoRef {
    TodoRef { id: t.id.clone(), title: t.title.clone(), status: status_name(t.status).into() }
}

fn status_name(s: TodoStatus) -> &'static str {
    match s {
        TodoStatus::Todo => "todo",
        TodoStatus::InProgress => "in-progress",
        TodoStatus::Done => "done",
    }
}

pub(super) fn todo_row(t: &Todo, store: &TodoStore, lanes: &[Lane], full: bool) -> TodoRow {
    TodoRow {
        id: t.id.clone(),
        title: t.title.clone(),
        status: status_name(t.status).into(),
        project: t.project.clone(),
        labels: t.labels.iter().map(|id| store.labels.iter().find(|l| &l.id == id).map(|l| l.name.clone()).unwrap_or_else(|| id.clone())).collect(),
        worktrees: t.worktrees.clone(),
        sessions: t.sessions.clone(),
        primary: t.primary.as_ref().map(|p| p.session.clone()),
        depends_on: t.depends_on.iter().filter(|d| store.todos.iter().any(|x| &x.id == *d)).cloned().collect(),
        blocked_by: store.blocked_by(t).iter().map(|x| x.id.clone()).collect(),
        waiting: t.waiting.clone(),
        review: t.review.clone(),
        section: section_of(store, t, lanes),
        archived_at: t.archived_at,
        due: t.due.clone(),
        due_in_days: t.due.as_deref().and_then(todos::due_day).map(|d| d - util::today_local()),
        created_at: t.created_at,
        updated_at: t.updated_at,
        done_at: t.done_at,
        waits_on: full.then(|| t.depends_on.iter().filter_map(|d| store.todos.iter().find(|x| &x.id == d)).map(todo_ref).collect()),
        needed_by: full.then(|| store.todos.iter().filter(|x| x.depends_on.contains(&t.id)).map(todo_ref).collect()),
        description: full.then(|| attachments::expand(&t.description, attachments::Style::Path)),
        attachments: full.then(|| attachments::referenced(&t.description).iter().filter_map(|p| attachments::locate(p)).map(|p| p.to_string_lossy().into_owned()).collect()),
    }
}

/// The app's section for a todo, as `todo list` filters and `section` in the JSON name it. A PR
/// in review puts it under In review only by a board with PRs (`Client::pr_lanes`).
fn section_of(store: &TodoStore, t: &Todo, lanes: &[Lane]) -> &'static str {
    match t.status {
        _ if t.archived_at.is_some() => "archived",
        TodoStatus::Done => "done",
        _ if store.is_waiting(t) => "waiting",
        _ if todos::is_in_review(t, lanes) => "review",
        TodoStatus::InProgress => "in-progress",
        TodoStatus::Todo => "ready",
    }
}

pub(super) fn store_of(board: &Board) -> TodoStore {
    TodoStore { todos: board.todos.clone(), labels: board.labels.clone() }
}

pub(super) fn resolve_todo(store: &TodoStore, id: &str) -> Result<String> {
    let id = named_by(id, "todo");
    if id.trim().is_empty() {
        bail!("Give a todo id.");
    }
    if store.todos.iter().any(|t| t.id == id) {
        return Ok(id.to_string());
    }
    let hits: Vec<&Todo> = store.todos.iter().filter(|t| t.id.starts_with(id)).collect();
    match hits.len() {
        1 => Ok(hits[0].id.clone()),
        0 => bail!("There's no todo {id}. `codealong todo list --status all` lists them."),
        n => bail!("{n} todos start with {id}; give more of the id."),
    }
}

pub(super) fn parse_todo_status(s: &str) -> Result<TodoStatus> {
    serde_json::from_value(Value::String(s.to_string())).map_err(|_| anyhow!("Status is todo, in-progress or done, not {s}."))
}

fn new_todo_id() -> String {
    let now = now_ms() as u64;
    let mut b = [0u8; 4];
    crate::os::system::random_bytes(&mut b);
    format!("t-{}{}", base36(now), base36(u32::from_le_bytes(b) as u64 % 36u64.pow(4)))
}

/// Ids for the label names, making any that don't exist (through a Labels change).
fn label_ids(c: &mut Client, store: &mut TodoStore, names: &[String]) -> Result<Vec<String>> {
    let (labels, ids) = todos::named(&store.labels, names, now_ms());
    if labels != store.labels {
        c.apply(TodoOp::Labels { labels: labels.clone() })?;
        store.labels = labels;
    }
    Ok(ids)
}

pub(super) fn todo(cmd: TodoCmd, c: &mut Client, json: bool) -> Result<()> {
    match cmd {
        TodoCmd::List(a) => todo_list(c, &a, json),
        TodoCmd::Show { id } => {
            let store = c.todos()?;
            let id = resolve_todo(&store, &id)?;
            let t = store.todos.iter().find(|t| t.id == id).unwrap();
            let row = todo_row(t, &store, &c.pr_lanes(), true);
            if json {
                return print_json(&row);
            }
            print_todo(&row);
            Ok(())
        }
        TodoCmd::Add(a) => {
            // Links are resolved first, so a wrong worktree or session leaves no half-made todo.
            let worktree = a.worktree.as_deref().map(|w| resolve_worktree(c, w)).transpose()?;
            let session = a.session.as_deref().map(|s| resolve_session(c, s)).transpose()?;
            let mut store = c.todos()?;
            let depends_on = a.depends_on.iter().map(|d| resolve_todo(&store, d)).collect::<Result<Vec<_>>>()?;
            let labels = label_ids(c, &mut store, &a.label)?;
            let description = with_attachments(read_text(a.description, a.description_file)?.unwrap_or_default(), &a.attach)?;
            let id = new_todo_id();
            let now = now_ms();
            let waiting = a.wait_for.map(|note| Waiting { note, since: now });
            let due = a.due.as_deref().map(|w| todos::due_from_words(w, util::today_local())).transpose()?;
            let todo = Todo { id: id.clone(), project: a.project.unwrap_or_default(), title: a.title, description, status: TodoStatus::Todo, labels, worktrees: vec![], sessions: vec![], created_at: now, updated_at: now, done_at: None, pending: vec![], depends_on, waiting, review: None, due, primary: None, archived_at: None, snoozed_until: None };
            c.apply(TodoOp::Create { todo })?;
            link(c, &id, worktree.as_deref(), session.as_deref(), true)?;
            show_after(c, &id, json)
        }
        TodoCmd::Update(a) => {
            let mut store = c.todos()?;
            let id = resolve_todo(&store, &a.id)?;
            let current = store.todos.iter().find(|t| t.id == id).unwrap().clone();
            let mut labels = None;
            if !a.add_label.is_empty() || !a.remove_label.is_empty() {
                let add = label_ids(c, &mut store, &a.add_label)?;
                let remove: Vec<String> = a.remove_label.iter().filter_map(|n| store.labels.iter().find(|l| l.name.eq_ignore_ascii_case(n)).map(|l| l.id.clone())).collect();
                let mut list: Vec<String> = current.labels.iter().filter(|l| !remove.contains(l)).cloned().collect();
                for l in add {
                    if !list.contains(&l) {
                        list.push(l);
                    }
                }
                labels = Some(list);
            }
            let mut depends_on = None;
            if !a.add_dependency.is_empty() || !a.remove_dependency.is_empty() {
                let add = a.add_dependency.iter().map(|d| resolve_todo(&store, d)).collect::<Result<Vec<_>>>()?;
                let remove: Vec<String> = a.remove_dependency.iter().map(|d| resolve_todo(&store, d).unwrap_or_else(|_| d.clone())).collect();
                let mut list: Vec<String> = current.depends_on.iter().filter(|d| !remove.contains(d)).cloned().collect();
                for d in add {
                    if !list.contains(&d) {
                        list.push(d);
                    }
                }
                depends_on = Some(list);
            }
            let status = a.status.as_deref().map(parse_todo_status).transpose()?;
            if status == Some(TodoStatus::InProgress) {
                warn_blocked(&store, &id, &[]);
            }
            let due = match (a.due.as_deref(), a.no_due) {
                (Some(w), _) => Some(Some(todos::due_from_words(w, util::today_local())?)),
                (None, true) => Some(None),
                (None, false) => None,
            };
            let patch = TodoPatch { depends_on, title: a.title, description: read_text(a.description, a.description_file)?, status, labels, project: a.project, waiting: None, review: None, due, archived: None, snoozed: None };
            c.apply(TodoOp::Update { id: id.clone(), patch })?;
            if let Some(s) = status.filter(|_| !a.no_link) {
                link_caller(c, &id, s, json);
            }
            show_after(c, &id, json)
        }
        TodoCmd::Start { id, take } => {
            let store = c.todos()?;
            let id = resolve_todo(&store, &id)?;
            let t = store.todos.iter().find(|t| t.id == id).unwrap();
            let me = find_me(c)?.0;
            if let Some(p) = primary_elsewhere(c, t, me.as_deref())? {
                let who = session_words(c, &p.session);
                match (&me, take) {
                    (Some(_), false) => bail!(
                        "{id}'s primary session is {who}, which is running. Send it what you have with `codealong session send {key} \"…\"` rather than take {id} on too. `codealong todo start {id} --take` makes this session the primary one instead: for when your user asks, or that session is stuck.",
                        key = p.session
                    ),
                    (None, false) => eprintln!("note: {id}'s primary session is {who}, which is running."),
                    _ => {}
                }
            }
            if take && me.is_none() {
                bail!("Codealong couldn't tell which session runs this command, so there's none to make {id}'s primary session.");
            }
            warn_blocked(&store, &id, &[]);
            c.apply(TodoOp::Update { id: id.clone(), patch: TodoPatch { status: Some(TodoStatus::InProgress), ..Default::default() } })?;
            link_caller(c, &id, TodoStatus::InProgress, json);
            if let Some(me) = me.filter(|_| take) {
                c.apply(TodoOp::Primary { id: id.clone(), session: Some(me) })?;
            }
            show_after(c, &id, json)
        }
        TodoCmd::Move(a) => {
            let store = c.todos()?;
            let id = resolve_todo(&store, &a.id)?;
            let mut ids: Vec<String> = store.todos.iter().map(|t| t.id.clone()).filter(|x| x != &id).collect();
            let at = match (a.top, a.bottom, &a.before, &a.after) {
                (true, ..) => 0,
                (_, true, ..) => ids.len(),
                (_, _, Some(other), _) | (_, _, _, Some(other)) => {
                    let other = resolve_todo(&store, other)?;
                    let i = ids.iter().position(|x| x == &other).ok_or_else(|| anyhow!("A todo can't move next to itself."))?;
                    if a.after.is_some() { i + 1 } else { i }
                }
                _ => bail!("Give --top, --bottom, --before or --after."),
            };
            ids.insert(at, id.clone());
            c.apply(TodoOp::Order { ids })?;
            show_after(c, &id, json)
        }
        TodoCmd::Done { id, no_link } => set_status(c, &id, TodoStatus::Done, !no_link, json),
        TodoCmd::Reopen { id } => set_status(c, &id, TodoStatus::Todo, false, json),
        TodoCmd::Wait(a) => {
            let store = c.todos()?;
            let id = resolve_todo(&store, &a.id)?;
            let t = store.todos.iter().find(|t| t.id == id).unwrap();
            if t.status == TodoStatus::Done {
                bail!("{id} is done; `codealong todo reopen {id}` first.");
            }
            let note = a.what.or_else(|| t.waiting.as_ref().map(|w| w.note.clone())).unwrap_or_default();
            c.apply(TodoOp::Update { id: id.clone(), patch: TodoPatch { waiting: Some(Some(Waiting { note, since: 0 })), ..Default::default() } })?;
            show_after(c, &id, json)
        }
        TodoCmd::Ready { id } => {
            let store = c.todos()?;
            let id = resolve_todo(&store, &id)?;
            c.apply(TodoOp::Update { id: id.clone(), patch: TodoPatch { waiting: Some(None), ..Default::default() } })?;
            let store = c.todos()?;
            if let Some(t) = store.todos.iter().find(|t| t.id == id).filter(|t| t.status == TodoStatus::Todo) {
                let open = store.blocked_by(t);
                if !open.is_empty() {
                    let names: Vec<String> = open.iter().map(|x| format!("{} \"{}\"", x.id, x.title)).collect();
                    eprintln!("note: {id} still waits on {}, so it stays under Waiting. `todo update {id} --remove-dependency <id>` takes one off.", names.join(", "));
                }
            }
            show_after(c, &id, json)
        }
        TodoCmd::Review(a) => {
            let store = c.todos()?;
            let id = resolve_todo(&store, &a.id)?;
            let t = store.todos.iter().find(|t| t.id == id).unwrap();
            let review = match a.off {
                true => None,
                false if t.status == TodoStatus::Done => bail!("{id} is done; `codealong todo reopen {id}` first."),
                false => Some(TodoReview { note: a.note.or_else(|| t.review.as_ref().map(|r| r.note.clone())).unwrap_or_default(), since: 0 }),
            };
            c.apply(TodoOp::Update { id: id.clone(), patch: TodoPatch { review: Some(review), ..Default::default() } })?;
            show_after(c, &id, json)
        }
        TodoCmd::Link(a) => {
            let store = c.todos()?;
            let id = resolve_todo(&store, &a.id)?;
            if a.session.is_none() && a.worktree.is_none() {
                bail!("Give --session or --worktree.");
            }
            link(c, &id, a.worktree.as_deref(), a.session.as_deref(), true)?;
            show_after(c, &id, json)
        }
        TodoCmd::Unlink(a) => {
            let store = c.todos()?;
            let id = resolve_todo(&store, &a.id)?;
            if a.session.is_none() && a.worktree.is_none() {
                bail!("Give --session or --worktree.");
            }
            link(c, &id, a.worktree.as_deref(), a.session.as_deref(), false)?;
            show_after(c, &id, json)
        }
        TodoCmd::Archive { id } => set_archived(c, &id, true, json),
        TodoCmd::Unarchive { id } => set_archived(c, &id, false, json),
        TodoCmd::Delete { id } => {
            let store = c.todos()?;
            let id = resolve_todo(&store, &id)?;
            c.apply(TodoOp::Delete { id: id.clone() })?;
            if json {
                print_json(&serde_json::json!({ "deleted": id }))
            } else {
                println!("Deleted {id}.");
                Ok(())
            }
        }
        TodoCmd::Labels { cmd } => {
            let store = c.todos()?;
            match cmd {
                None => {}
                Some(LabelCmd::Add { name, color }) => {
                    let mut labels = store.labels.clone();
                    let name = one_line(&name, 40);
                    if labels.iter().any(|l| l.name.eq_ignore_ascii_case(&name)) {
                        bail!("There's a label called {name} already.");
                    }
                    let color = match color {
                        Some(c) if LABEL_COLORS.contains(&c.as_str()) => c,
                        Some(c) => bail!("Colors are {}, not {c}.", LABEL_COLORS.join(", ")),
                        None => {
                            let used: HashSet<&str> = labels.iter().map(|l| l.color.as_str()).collect();
                            LABEL_COLORS.iter().find(|c| !used.contains(*c)).unwrap_or(&LABEL_COLORS[labels.len() % LABEL_COLORS.len()]).to_string()
                        }
                    };
                    labels.push(Label { id: format!("l-{}", base36(now_ms() as u64)), name, color });
                    c.apply(TodoOp::Labels { labels })?;
                }
                Some(LabelCmd::Remove { name }) => {
                    let labels: Vec<Label> = store.labels.iter().filter(|l| !l.name.eq_ignore_ascii_case(&name)).cloned().collect();
                    if labels.len() == store.labels.len() {
                        bail!("There's no label called {name}.");
                    }
                    c.apply(TodoOp::Labels { labels })?;
                }
            }
            let labels = c.todos()?.labels;
            if json {
                return print_json(&labels);
            }
            for l in &labels {
                println!("{:<12} {:<8} {}", l.name, l.color, l.id);
            }
            Ok(())
        }
    }
}

/// A note on stderr, so JSON on stdout stays clean: starting a blocked todo is allowed, but
/// whoever starts it should know.
/// Says what a todo being started still waits on, leaving out the todos started with it.
pub(super) fn warn_blocked(store: &TodoStore, id: &str, with: &[String]) {
    let Some(t) = store.todos.iter().find(|t| t.id == id) else { return };
    let open: Vec<&Todo> = store.blocked_by(t).into_iter().filter(|x| !with.contains(&x.id)).collect();
    if !open.is_empty() {
        let names: Vec<String> = open.iter().map(|x| format!("{} \"{}\" ({})", x.id, x.title, status_name(x.status))).collect();
        eprintln!("note: {} waits on {}, not done yet.", t.id, names.join(", "));
    }
    if let Some(w) = &t.waiting {
        let what = if w.note.is_empty() { String::new() } else { format!(" for \"{}\"", w.note) };
        eprintln!("note: {} was marked as waiting{what}; starting it ends that.", t.id);
    }
}

/// Sets a todo's status, and links the session running this command to it when `link` says so.
fn set_status(c: &mut Client, id: &str, status: TodoStatus, link: bool, json: bool) -> Result<()> {
    let store = c.todos()?;
    let id = resolve_todo(&store, id)?;
    c.apply(TodoOp::Update { id: id.clone(), patch: TodoPatch { status: Some(status), ..Default::default() } })?;
    if link {
        link_caller(c, &id, status, json);
    }
    show_after(c, &id, json)
}

/// Archives a todo, or brings it back from the archive.
fn set_archived(c: &mut Client, id: &str, archived: bool, json: bool) -> Result<()> {
    let store = c.todos()?;
    let id = resolve_todo(&store, id)?;
    c.apply(TodoOp::Update { id: id.clone(), patch: TodoPatch { archived: Some(archived), ..Default::default() } })?;
    show_after(c, &id, json)
}

/// Links the session running this command to a todo it just said it's working on or has
/// finished. In progress links the session and the worktree the command runs in; done links
/// only the session, since a finished todo is found through its sessions, and a worktree
/// several agents share would list it for all of them. Best effort, after the status has
/// changed: a shell outside any session still changes it, and a link that fails is a note,
/// not an error. An agent is told when its session wasn't found, so it can link one itself.
fn link_caller(c: &mut Client, id: &str, status: TodoStatus, json: bool) {
    if status == TodoStatus::Todo {
        return;
    }
    let mut links = Vec::new();
    if status == TodoStatus::InProgress {
        let here = std::env::current_dir().ok().and_then(|d| lane_for_path(c.board().ok()?, &d).map(|l| l.id.clone()));
        links.extend(here.map(|lane| TodoOp::Link { id: id.into(), worktree: Some(lane), session: None, linked: true }));
    }
    match find_me(c).ok().and_then(|(key, _)| key) {
        // Linked as found, not looked up: a session the app just started isn't on the board yet.
        Some(key) => links.push(TodoOp::Link { id: id.into(), worktree: None, session: Some(key), linked: true }),
        // Output that isn't a terminal means an agent or a script; a person at a terminal expects none.
        None if json => eprintln!("note: Codealong couldn't tell which session runs this command, so no session is linked to {id}. `codealong todo link {id} --session <key>` links one."),
        None => {}
    }
    for op in links {
        if let Err(e) = c.apply(op) {
            eprintln!("note: {id} changed, but linking it failed: {e:#}");
        }
    }
}

/// A todo's primary session when it's another than `me` and still has the todo
/// (`todos::holds`): the session to send what you have, rather than take the todo on too.
pub(super) fn primary_elsewhere(c: &mut Client, t: &Todo, me: Option<&str>) -> Result<Option<todos::Primary>> {
    let Some(p) = t.primary.clone().filter(|p| Some(p.session.as_str()) != me) else { return Ok(None) };
    let lanes = todos::LaneIndex::of(&c.board()?.lanes);
    Ok(todos::holds(&p, &lanes, now_ms()).then_some(p))
}

/// A session as a message names it: its title, account and worktree, then its key.
pub(super) fn session_words(c: &mut Client, key: &str) -> String {
    match c.board().ok().and_then(|b| b.session(key)) {
        Some((lane, s)) => format!("\"{}\" ({} · {}, {key})", s.title, s.account, lane.callsign),
        None => key.to_string(),
    }
}

/// Links (or unlinks) the worktree and session named, `.` and `me` included.
fn link(c: &mut Client, id: &str, worktree: Option<&str>, session: Option<&str>, linked: bool) -> Result<()> {
    if let Some(w) = worktree {
        let lane = resolve_worktree(c, w)?;
        c.apply(TodoOp::Link { id: id.into(), worktree: Some(lane), session: None, linked })?;
    }
    if let Some(s) = session {
        let key = resolve_session(c, s)?;
        c.apply(TodoOp::Link { id: id.into(), worktree: None, session: Some(key), linked })?;
    }
    Ok(())
}

fn show_after(c: &mut Client, id: &str, json: bool) -> Result<()> {
    let store = c.todos()?;
    let Some(t) = store.todos.iter().find(|t| t.id == id) else { bail!("The todo {id} isn't in the list any more.") };
    let row = todo_row(t, &store, &c.pr_lanes(), true);
    if json {
        return print_json(&row);
    }
    print_todo(&row);
    Ok(())
}

fn print_todo(r: &TodoRow) {
    println!("{}  {}  [{}]{}", r.id, r.title, r.status, if r.project.is_empty() { String::new() } else { format!("  {}", r.project) });
    if let Some(at) = r.archived_at {
        match ago(now_ms() - at).as_str() {
            "now" => println!("Archived just now"),
            a => println!("Archived {a} ago"),
        }
    }
    if !r.labels.is_empty() {
        println!("Labels: {}", r.labels.join(", "));
    }
    for w in &r.worktrees {
        println!("Worktree: {}", tilde(Path::new(w)));
    }
    for s in &r.sessions {
        println!("Session: {s}{}", if r.primary.as_ref() == Some(s) { " (primary)" } else { "" });
    }
    for w in r.waits_on.iter().flatten() {
        println!("Waits on: {} {} ({})", w.id, w.title, w.status);
    }
    for n in r.needed_by.iter().flatten() {
        println!("Needed by: {} {} ({})", n.id, n.title, n.status);
    }
    if let Some(due) = &r.due {
        let late = r.status != "done" && r.due_in_days.is_some_and(|n| n < 0);
        println!("Due: {due} ({}{})", todos::due_phrase(due, util::today_local()), if late { ", overdue" } else { "" });
    }
    if let Some(w) = &r.waiting {
        let since = match w.since {
            0 => String::new(),
            at => match ago(now_ms() - at).as_str() {
                "now" => " (marked just now)".into(),
                a => format!(" (marked {a} ago)"),
            },
        };
        match w.note.as_str() {
            "" => println!("Waiting{since}"),
            note => println!("Waiting for: {note}{since}"),
        }
    }
    if let Some(d) = r.description.as_deref().filter(|d| !d.trim().is_empty()) {
        println!();
        println!("{}", d.trim_end());
    }
}

fn todo_list(c: &mut Client, a: &TodoListArgs, json: bool) -> Result<()> {
    let worktree = a.worktree.as_deref().map(|w| resolve_worktree(c, w)).transpose()?;
    let (me, here) = match a.mine {
        true => {
            let me = find_me(c)?.0;
            let here = std::env::current_dir().ok().and_then(|d| lane_for_path(c.board().ok()?, &d).map(|l| l.id.clone()));
            (me, here)
        }
        false => (None, None),
    };
    let store = c.todos()?;
    let lanes = c.pr_lanes();
    let label = a.label.as_deref().map(|n| store.labels.iter().find(|l| l.name.eq_ignore_ascii_case(n)).map(|l| l.id.clone()).ok_or_else(|| anyhow!("There's no label called {n}."))).transpose()?;
    let today = util::today_local();
    let due_by = a.due_by.as_deref().map(|w| todos::due_from_words(w, today)).transpose()?.and_then(|d| todos::due_day(&d));
    let status = a.status.as_deref().unwrap_or(if a.archived { "all" } else { "open" });
    let rows: Vec<TodoRow> = store
        .todos
        .iter()
        .filter(|t| t.archived_at.is_some() == a.archived)
        .filter(|t| match status {
            "all" => true,
            "open" => t.status != TodoStatus::Done,
            s => parse_todo_status(s).map(|st| st == t.status).unwrap_or(false),
        })
        .filter(|t| a.project.as_deref().map_or(true, |p| p.eq_ignore_ascii_case(&t.project)))
        .filter(|t| label.as_deref().map_or(true, |l| t.labels.iter().any(|x| x == l)))
        .filter(|t| worktree.as_deref().map_or(true, |w| t.worktrees.iter().any(|x| x == w)))
        .filter(|t| !a.mine || me.as_deref().is_some_and(|k| t.sessions.iter().any(|s| s == k)) || here.as_deref().is_some_and(|w| t.worktrees.iter().any(|x| x == w)))
        .filter(|t| !a.ready || store.is_ready(t))
        .filter(|t| !a.waiting || store.is_waiting(t))
        .filter(|t| !a.review || section_of(&store, t, &lanes) == "review")
        .filter(|t| due_by.map_or(true, |by| t.due.as_deref().and_then(todos::due_day).is_some_and(|d| d <= by)))
        .map(|t| todo_row(t, &store, &lanes, false))
        .collect();
    if !["all", "open", "todo", "in-progress", "done"].contains(&status) {
        bail!("--status is todo, in-progress, done, open or all.");
    }
    if json {
        return print_json(&rows);
    }
    // What each waits on: the todos not done yet, then what it was marked as waiting for.
    let waits = |r: &TodoRow| {
        let mut parts = r.blocked_by.clone();
        if let Some(w) = &r.waiting {
            parts.push(if w.note.is_empty() { "(marked)".into() } else { format!("\"{}\"", truncate(&w.note, 40)) });
        }
        parts.join(", ")
    };
    // A due column only when something's due, so lists without dates stay as narrow as they were.
    let dated = rows.iter().any(|r| r.due.is_some());
    let due = |r: &TodoRow| r.due.as_deref().map(|d| format!("{d} ({})", todos::due_phrase(d, today))).unwrap_or_default();
    let table: Vec<Vec<String>> = rows
        .iter()
        .map(|r| {
            let mut row = vec![r.id.clone(), r.status.clone(), r.project.clone(), truncate(&r.title, 60), r.labels.join(", "), waits(r)];
            if dated {
                row.push(due(r));
            }
            row.push(r.worktrees.iter().map(|w| tilde(Path::new(w))).collect::<Vec<_>>().join(", "));
            row
        })
        .collect();
    let head: &[&str] = if dated { &["ID", "STATUS", "PROJECT", "TITLE", "LABELS", "WAITS ON", "DUE", "WORKTREES"] } else { &["ID", "STATUS", "PROJECT", "TITLE", "LABELS", "WAITS ON", "WORKTREES"] };
    print!("{}", table_text(head, table));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_usable_and_prefixes_resolve() {
        let id = new_todo_id();
        assert!(id.starts_with("t-") && id.len() > 8 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'));
        let t = |id: &str| Todo { id: id.into(), project: String::new(), title: String::new(), description: String::new(), status: TodoStatus::Todo, labels: vec![], worktrees: vec![], sessions: vec![], created_at: 0, updated_at: 0, done_at: None, pending: vec![], depends_on: vec![], waiting: None, review: None, due: None, primary: None, archived_at: None, snoozed_until: None };
        let store = TodoStore { todos: vec![t("t-abc1"), t("t-abd2")], labels: vec![] };
        assert_eq!(resolve_todo(&store, "t-abc").unwrap(), "t-abc1");
        assert!(resolve_todo(&store, "t-ab").unwrap_err().to_string().contains("2 todos"));
        assert!(resolve_todo(&store, "zz").is_err());
        assert!(resolve_todo(&store, "").is_err() && resolve_todo(&store, " ").is_err());
    }
}
