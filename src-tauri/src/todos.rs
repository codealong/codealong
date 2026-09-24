//! Todos: work you mean to hand an agent. Each has a title, a description that can hold images,
//! labels, a status, and the worktrees and sessions that took it on. They're kept in
//! `todos.json` next to the config, in your order; the engine owns the file, applies every
//! change through `apply`, and publishes the list with the board. The file is watched, so an
//! agent or a text editor can add to it too.
//!
//! Starting a session from a todo links it: at once when Codealong chose the session's ID (the
//! terminal), otherwise when a new session of that account shows up in the same repo within
//! half an hour (the Claude and Codex apps, which pick their own IDs and sometimes their own
//! worktree).

use std::collections::HashSet;
use std::path::PathBuf;

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

use crate::config::support_dir;

/// How long a start in an app waits for its session to show up before it's forgotten.
pub const PENDING_MS: i64 = 30 * 60 * 1000;
/// A session that says it started this long before the start still counts: clocks and
/// transcript timestamps don't agree to the millisecond.
const PENDING_SLACK_MS: i64 = 60 * 1000;
const MAX_TITLE: usize = 300;
const MAX_DESCRIPTION: usize = 100_000;
const MAX_LABEL: usize = 40;

/// The label colors, as `--label-<color>` tokens in design/tokens.json.
pub const LABEL_COLORS: [&str; 8] = ["gray", "red", "pink", "purple", "indigo", "blue", "teal", "green"];

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "kebab-case")]
pub enum TodoStatus {
    #[default]
    Todo,
    /// An agent is on it.
    InProgress,
    Done,
}

/// A start in the Claude or Codex app, waiting for its session to show up.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PendingStart {
    /// The worktree it started in.
    pub lane: String,
    /// `claude` or `codex`.
    pub harness: String,
    pub account: String,
    pub at: i64,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Todo {
    pub id: String,
    /// The project it belongs to; empty until you pick one or link a worktree.
    #[serde(default)]
    pub project: String,
    #[serde(default)]
    pub title: String,
    /// Plain text. Images are `![…](images/<name>)`; see images.rs.
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub status: TodoStatus,
    /// Label ids, in the order you added them.
    #[serde(default)]
    pub labels: Vec<String>,
    /// Worktrees (lane ids) it's linked to.
    #[serde(default)]
    pub worktrees: Vec<String>,
    /// Session keys it's linked to.
    #[serde(default)]
    pub sessions: Vec<String>,
    #[serde(default)]
    pub created_at: i64,
    #[serde(default)]
    pub updated_at: i64,
    #[serde(default)]
    pub done_at: Option<i64>,
    #[serde(default)]
    pub pending: Vec<PendingStart>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Label {
    pub id: String,
    pub name: String,
    /// One of `LABEL_COLORS`.
    pub color: String,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct TodoStore {
    /// In your order.
    #[serde(default)]
    pub todos: Vec<Todo>,
    /// In your order.
    #[serde(default)]
    pub labels: Vec<Label>,
}

/// The fields a change sets; the rest stay as they are.
#[derive(Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct TodoPatch {
    pub title: Option<String>,
    pub description: Option<String>,
    pub status: Option<TodoStatus>,
    pub labels: Option<Vec<String>>,
    pub project: Option<String>,
}

/// A change from the UI. Changes are small and specific, so one made in a window never undoes
/// another made meanwhile, by another window or by the engine linking a session.
#[derive(Deserialize, Clone, Debug)]
#[serde(tag = "op", rename_all = "kebab-case", rename_all_fields = "camelCase")]
pub enum TodoOp {
    /// A new todo, at the top. The window picks the id, so it can show the todo at once.
    Create { todo: Todo },
    Update { id: String, patch: TodoPatch },
    /// Links (or unlinks) a worktree or a session.
    Link { id: String, worktree: Option<String>, session: Option<String>, linked: bool },
    Delete { id: String },
    /// Puts a deleted todo back where it was.
    Restore { todo: Todo, index: usize },
    /// Todo ids in their new order. Ids left out keep their places after the ones given.
    Order { ids: Vec<String> },
    /// The whole label list. A deleted label stays on its todos until it's back or they change,
    /// so deleting one can be undone.
    Labels { labels: Vec<Label> },
}

/// What linking needs to know about a worktree: its project, for a todo that has none yet.
pub trait Lanes {
    fn project_of(&self, lane: &str) -> Option<String>;
    /// A repo's identity, the same for all of its worktrees.
    fn repo_of(&self, lane: &str) -> Option<String>;
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn clip(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        s.chars().take(max).collect()
    }
}

fn one_line(s: &str, max: usize) -> String {
    clip(&s.split_whitespace().collect::<Vec<_>>().join(" "), max)
}

fn push_unique(list: &mut Vec<String>, item: &str) -> bool {
    if item.is_empty() || list.iter().any(|x| x == item) {
        return false;
    }
    list.push(item.to_string());
    true
}

/// Sets a status, keeping `done_at` in step.
fn set_status(t: &mut Todo, status: TodoStatus, now: i64) {
    if t.status == status {
        return;
    }
    t.status = status;
    t.done_at = (status == TodoStatus::Done).then_some(now);
}

/// A linked session means an agent is on it.
fn started(t: &mut Todo, now: i64) {
    if t.status == TodoStatus::Todo {
        set_status(t, TodoStatus::InProgress, now);
    }
}

/// A todo without a project takes its first worktree's.
fn adopt_project(t: &mut Todo, lane: &str, lanes: &dyn Lanes) {
    if t.project.is_empty() {
        if let Some(p) = lanes.project_of(lane) {
            t.project = p;
        }
    }
}

fn link(t: &mut Todo, worktree: Option<&str>, session: Option<&str>, lanes: &dyn Lanes, now: i64) -> bool {
    let mut changed = false;
    if let Some(w) = worktree {
        changed |= push_unique(&mut t.worktrees, w);
        adopt_project(t, w, lanes);
    }
    if let Some(s) = session {
        if push_unique(&mut t.sessions, s) {
            changed = true;
            started(t, now);
        }
    }
    changed
}

fn clean_labels(labels: Vec<Label>) -> Vec<Label> {
    let mut seen = HashSet::new();
    labels
        .into_iter()
        .filter(|l| valid_id(&l.id) && seen.insert(l.id.clone()))
        .map(|l| Label {
            name: one_line(&l.name, MAX_LABEL),
            color: if LABEL_COLORS.contains(&l.color.as_str()) { l.color } else { "gray".into() },
            id: l.id,
        })
        .filter(|l| !l.name.is_empty())
        .collect()
}

impl TodoStore {
    /// Applies a change. Returns whether anything changed; an error says why it couldn't.
    pub fn apply(&mut self, op: TodoOp, lanes: &dyn Lanes, now: i64) -> Result<bool> {
        match op {
            TodoOp::Create { todo } => {
                if !valid_id(&todo.id) {
                    bail!("That todo has no usable id.");
                }
                if self.todos.iter().any(|t| t.id == todo.id) {
                    return Ok(false);
                }
                let mut seen = HashSet::new();
                let mut t = Todo {
                    id: todo.id.clone(),
                    project: one_line(&todo.project, 200),
                    title: one_line(&todo.title, MAX_TITLE),
                    description: clip(&todo.description, MAX_DESCRIPTION),
                    status: TodoStatus::Todo,
                    labels: todo.labels.iter().filter(|l| valid_id(l) && seen.insert(l.to_string())).cloned().collect(),
                    worktrees: Vec::new(),
                    sessions: Vec::new(),
                    created_at: now,
                    updated_at: now,
                    done_at: None,
                    pending: Vec::new(),
                };
                set_status(&mut t, todo.status, now);
                for w in &todo.worktrees {
                    link(&mut t, Some(w), None, lanes, now);
                }
                for s in &todo.sessions {
                    link(&mut t, None, Some(s), lanes, now);
                }
                self.todos.insert(0, t);
                Ok(true)
            }
            TodoOp::Update { id, patch } => {
                let Some(t) = self.todos.iter_mut().find(|t| t.id == id) else { bail!("That todo is gone.") };
                let before = t.clone();
                if let Some(title) = patch.title {
                    t.title = one_line(&title, MAX_TITLE);
                }
                if let Some(d) = patch.description {
                    t.description = clip(&d, MAX_DESCRIPTION);
                }
                if let Some(labels) = patch.labels {
                    let mut seen = HashSet::new();
                    t.labels = labels.into_iter().filter(|l| valid_id(l) && seen.insert(l.clone())).collect();
                }
                if let Some(p) = patch.project {
                    t.project = one_line(&p, 200);
                }
                if let Some(s) = patch.status {
                    set_status(t, s, now);
                }
                let changed = *t != before;
                if changed {
                    t.updated_at = now;
                }
                Ok(changed)
            }
            TodoOp::Link { id, worktree, session, linked } => {
                let Some(t) = self.todos.iter_mut().find(|t| t.id == id) else { bail!("That todo is gone.") };
                let changed = if linked {
                    link(t, worktree.as_deref(), session.as_deref(), lanes, now)
                } else {
                    let before = (t.worktrees.len(), t.sessions.len());
                    t.worktrees.retain(|w| Some(w.as_str()) != worktree.as_deref());
                    t.sessions.retain(|s| Some(s.as_str()) != session.as_deref());
                    // Unlinking the worktree a start is waiting in gives up on that start.
                    t.pending.retain(|p| Some(p.lane.as_str()) != worktree.as_deref());
                    before != (t.worktrees.len(), t.sessions.len())
                };
                if changed {
                    t.updated_at = now;
                }
                Ok(changed)
            }
            TodoOp::Delete { id } => {
                let before = self.todos.len();
                self.todos.retain(|t| t.id != id);
                Ok(self.todos.len() != before)
            }
            TodoOp::Restore { todo, index } => {
                if !valid_id(&todo.id) || self.todos.iter().any(|t| t.id == todo.id) {
                    return Ok(false);
                }
                let at = index.min(self.todos.len());
                self.todos.insert(at, todo);
                Ok(true)
            }
            TodoOp::Order { ids } => {
                let before: Vec<String> = self.todos.iter().map(|t| t.id.clone()).collect();
                let rank = |id: &str| ids.iter().position(|x| x == id).unwrap_or(usize::MAX);
                // Stable, so the ones not given keep their order after the ones that are.
                self.todos.sort_by_key(|t| rank(&t.id));
                Ok(self.todos.iter().map(|t| &t.id).ne(before.iter()))
            }
            TodoOp::Labels { labels } => {
                let labels = clean_labels(labels);
                let changed = labels != self.labels;
                self.labels = labels;
                Ok(changed)
            }
        }
    }

    /// A session started from a todo: links its worktree, and the session when Codealong chose
    /// its ID, or waits for it to show up. Starting one reopens a todo that was done.
    pub fn started(&mut self, id: &str, lane: &str, session: Option<&str>, pending: Option<PendingStart>, lanes: &dyn Lanes, now: i64) -> bool {
        let Some(t) = self.todos.iter_mut().find(|t| t.id == id) else { return false };
        link(t, Some(lane), session, lanes, now);
        if let Some(p) = pending {
            t.pending.push(p);
        }
        set_status(t, TodoStatus::InProgress, now);
        t.updated_at = now;
        true
    }

    /// Links the sessions that starts in the apps were waiting for, and forgets starts that
    /// waited too long. A start takes the earliest new session of its account and harness in
    /// its worktree, else in another worktree of the same repo (the apps can make their own),
    /// that no todo has yet. Returns whether anything changed.
    pub fn resolve_pending(&mut self, sessions: &[SessionRef], lanes: &dyn Lanes, now: i64) -> bool {
        if self.todos.iter().all(|t| t.pending.is_empty()) {
            return false;
        }
        let mut taken: HashSet<String> = self.todos.iter().flat_map(|t| t.sessions.iter().cloned()).collect();
        let mut changed = false;
        for t in &mut self.todos {
            if t.pending.is_empty() {
                continue;
            }
            let mut waiting = Vec::new();
            for p in std::mem::take(&mut t.pending) {
                if now - p.at > PENDING_MS {
                    changed = true;
                    continue;
                }
                let repo = lanes.repo_of(&p.lane);
                let pick = sessions
                    .iter()
                    .filter(|s| s.harness == p.harness && s.account == p.account && s.started >= p.at - PENDING_SLACK_MS && !taken.contains(s.key))
                    .filter(|s| s.lane == p.lane || (repo.is_some() && lanes.repo_of(s.lane) == repo))
                    .min_by_key(|s| (s.lane != p.lane, s.started));
                match pick {
                    Some(s) => {
                        taken.insert(s.key.to_string());
                        link(t, None, Some(s.key), lanes, now);
                        t.updated_at = now;
                        changed = true;
                    }
                    None => waiting.push(p),
                }
            }
            t.pending = waiting;
        }
        changed
    }
}

/// A session on the board, as pending starts are matched against it.
#[derive(Clone, Copy, Debug)]
pub struct SessionRef<'a> {
    pub key: &'a str,
    /// The lane it's listed under as its own (not as a visitor).
    pub lane: &'a str,
    pub harness: &'a str,
    pub account: &'a str,
    /// When it started, or its first activity.
    pub started: i64,
}

pub fn path() -> PathBuf {
    support_dir().join("todos.json")
}

/// The store and its text as read, or an empty store when there's no file yet. A file that
/// doesn't parse is an error, so it's never overwritten with an empty list.
pub fn load() -> Result<(TodoStore, String)> {
    let p = path();
    let text = match std::fs::read_to_string(&p) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok((TodoStore::default(), String::new())),
        Err(e) => bail!("Couldn't read {}: {e}", p.display()),
    };
    match serde_json::from_str::<TodoStore>(&text) {
        Ok(mut s) => {
            s.labels = clean_labels(std::mem::take(&mut s.labels));
            s.todos.retain(|t| valid_id(&t.id));
            Ok((s, text))
        }
        Err(e) => bail!("todos.json doesn't read as a todo list ({e}). Codealong won't change it until it does."),
    }
}

/// Writes the store through a temporary file, so the watcher never reads half a file. Returns
/// the text written.
pub fn save(store: &TodoStore) -> Result<String> {
    let text = serde_json::to_string_pretty(store)?;
    std::fs::create_dir_all(support_dir())?;
    let p = path();
    let tmp = p.with_extension("json.tmp");
    std::fs::write(&tmp, &text)?;
    std::fs::rename(&tmp, &p)?;
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    struct FakeLanes(HashMap<&'static str, (&'static str, &'static str)>);

    impl Lanes for FakeLanes {
        fn project_of(&self, lane: &str) -> Option<String> {
            self.0.get(lane).map(|(p, _)| p.to_string())
        }
        fn repo_of(&self, lane: &str) -> Option<String> {
            self.0.get(lane).map(|(_, r)| r.to_string())
        }
    }

    fn lanes() -> FakeLanes {
        FakeLanes(HashMap::from([
            ("/r/app", ("Northwind", "northwindai/atlas")),
            ("/r/app3", ("Northwind", "northwindai/atlas")),
            ("/u/.codex/worktrees/e8e5/app", ("Northwind", "northwindai/atlas")),
            ("/r/kestrel", ("Kestrel", "kestrelhq/kestrel")),
        ]))
    }

    fn todo(id: &str) -> Todo {
        Todo {
            id: id.into(),
            project: String::new(),
            title: format!("Todo {id}"),
            description: String::new(),
            status: TodoStatus::Todo,
            labels: vec![],
            worktrees: vec![],
            sessions: vec![],
            created_at: 0,
            updated_at: 0,
            done_at: None,
            pending: vec![],
        }
    }

    fn store(ids: &[&str]) -> TodoStore {
        let mut s = TodoStore::default();
        for id in ids.iter().rev() {
            s.apply(TodoOp::Create { todo: todo(id) }, &lanes(), 10).unwrap();
        }
        s
    }

    fn ids(s: &TodoStore) -> Vec<&str> {
        s.todos.iter().map(|t| t.id.as_str()).collect()
    }

    #[test]
    fn new_todos_go_on_top_with_clean_text() {
        let mut s = store(&["a"]);
        let mut t = todo("b");
        t.title = "  Fix the\n login   redirect ".into();
        t.created_at = 99;
        t.pending = vec![PendingStart { lane: "/r/app".into(), harness: "claude".into(), account: "claude".into(), at: 1 }];
        assert!(s.apply(TodoOp::Create { todo: t.clone() }, &lanes(), 50).unwrap());
        assert_eq!(ids(&s), vec!["b", "a"]);
        assert_eq!(s.todos[0].title, "Fix the login redirect");
        // The engine keeps the clock, and a new todo waits on nothing.
        assert_eq!((s.todos[0].created_at, s.todos[0].updated_at), (50, 50));
        assert!(s.todos[0].pending.is_empty());
        // The same id twice is one todo.
        assert!(!s.apply(TodoOp::Create { todo: t }, &lanes(), 60).unwrap());
        assert!(s.apply(TodoOp::Create { todo: todo("bad id!") }, &lanes(), 60).is_err());
    }

    #[test]
    fn a_todo_made_from_a_session_is_in_progress_in_its_project() {
        let mut s = TodoStore::default();
        let mut t = todo("a");
        t.worktrees = vec!["/r/app3".into()];
        t.sessions = vec!["claude:claude2:s1".into()];
        s.apply(TodoOp::Create { todo: t }, &lanes(), 5).unwrap();
        assert_eq!(s.todos[0].status, TodoStatus::InProgress);
        assert_eq!(s.todos[0].project, "Northwind");
    }

    #[test]
    fn updates_change_only_what_they_name() {
        let mut s = store(&["a"]);
        s.apply(TodoOp::Link { id: "a".into(), worktree: None, session: Some("codex:codex:t1".into()), linked: true }, &lanes(), 20).unwrap();
        let patch = TodoPatch { description: Some("See ![image](images/abc123def456.png)".into()), ..TodoPatch::default() };
        assert!(s.apply(TodoOp::Update { id: "a".into(), patch }, &lanes(), 30).unwrap());
        let t = &s.todos[0];
        assert_eq!(t.sessions, vec!["codex:codex:t1".to_string()]);
        assert_eq!(t.title, "Todo a");
        assert_eq!(t.updated_at, 30);
        // Nothing new is no change.
        let same = TodoPatch { title: Some("Todo a".into()), ..TodoPatch::default() };
        assert!(!s.apply(TodoOp::Update { id: "a".into(), patch: same }, &lanes(), 40).unwrap());
        assert_eq!(s.todos[0].updated_at, 30);
        let gone = TodoOp::Update { id: "zz".into(), patch: TodoPatch::default() };
        assert!(s.apply(gone, &lanes(), 40).is_err());
    }

    #[test]
    fn done_keeps_its_time_until_reopened() {
        let mut s = store(&["a"]);
        let done = TodoPatch { status: Some(TodoStatus::Done), ..TodoPatch::default() };
        s.apply(TodoOp::Update { id: "a".into(), patch: done.clone() }, &lanes(), 70).unwrap();
        assert_eq!(s.todos[0].done_at, Some(70));
        s.apply(TodoOp::Update { id: "a".into(), patch: done }, &lanes(), 80).unwrap();
        assert_eq!(s.todos[0].done_at, Some(70));
        let reopen = TodoPatch { status: Some(TodoStatus::Todo), ..TodoPatch::default() };
        s.apply(TodoOp::Update { id: "a".into(), patch: reopen }, &lanes(), 90).unwrap();
        assert_eq!(s.todos[0].done_at, None);
    }

    #[test]
    fn linking_a_session_starts_the_todo_and_unlinking_keeps_its_status() {
        let mut s = store(&["a"]);
        let link = |session: &str, linked| TodoOp::Link { id: "a".into(), worktree: None, session: Some(session.into()), linked };
        assert!(s.apply(link("claude:claude:s1", true), &lanes(), 20).unwrap());
        assert_eq!(s.todos[0].status, TodoStatus::InProgress);
        assert!(!s.apply(link("claude:claude:s1", true), &lanes(), 21).unwrap());
        assert!(s.apply(link("claude:claude:s1", false), &lanes(), 22).unwrap());
        assert!(s.todos[0].sessions.is_empty());
        assert_eq!(s.todos[0].status, TodoStatus::InProgress);
        // A worktree alone doesn't mean an agent is on it; it does give the todo a project.
        let mut s = store(&["b"]);
        s.apply(TodoOp::Link { id: "b".into(), worktree: Some("/r/kestrel".into()), session: None, linked: true }, &lanes(), 20).unwrap();
        assert_eq!((s.todos[0].status, s.todos[0].project.as_str()), (TodoStatus::Todo, "Kestrel"));
    }

    #[test]
    fn deleting_can_be_undone_in_place() {
        let mut s = store(&["a", "b", "c"]);
        let b = s.todos[1].clone();
        assert!(s.apply(TodoOp::Delete { id: "b".into() }, &lanes(), 20).unwrap());
        assert_eq!(ids(&s), vec!["a", "c"]);
        assert!(s.apply(TodoOp::Restore { todo: b.clone(), index: 1 }, &lanes(), 21).unwrap());
        assert_eq!(ids(&s), vec!["a", "b", "c"]);
        assert!(!s.apply(TodoOp::Restore { todo: b, index: 0 }, &lanes(), 22).unwrap());
    }

    #[test]
    fn reordering_keeps_the_rest_in_place() {
        let mut s = store(&["a", "b", "c", "d"]);
        assert!(s.apply(TodoOp::Order { ids: vec!["c".into(), "a".into()] }, &lanes(), 20).unwrap());
        assert_eq!(ids(&s), vec!["c", "a", "b", "d"]);
        assert!(!s.apply(TodoOp::Order { ids: vec!["c".into(), "a".into()] }, &lanes(), 21).unwrap());
    }

    #[test]
    fn labels_are_cleaned_and_their_colors_known() {
        let mut s = TodoStore::default();
        let labels = vec![
            Label { id: "l1".into(), name: " Bug ".into(), color: "red".into() },
            Label { id: "l1".into(), name: "Again".into(), color: "red".into() },
            Label { id: "l2".into(), name: "UI".into(), color: "chartreuse".into() },
            Label { id: "l3".into(), name: "   ".into(), color: "blue".into() },
        ];
        assert!(s.apply(TodoOp::Labels { labels }, &lanes(), 1).unwrap());
        let names: Vec<(&str, &str)> = s.labels.iter().map(|l| (l.name.as_str(), l.color.as_str())).collect();
        assert_eq!(names, vec![("Bug", "red"), ("UI", "gray")]);
    }

    fn pending(lane: &str, harness: &str, account: &str, at: i64) -> PendingStart {
        PendingStart { lane: lane.into(), harness: harness.into(), account: account.into(), at }
    }

    fn session<'a>(key: &'a str, lane: &'a str, harness: &'a str, account: &'a str, started: i64) -> SessionRef<'a> {
        SessionRef { key, lane, harness, account, started }
    }

    #[test]
    fn a_terminal_start_links_its_session_at_once() {
        let mut s = store(&["a"]);
        s.todos[0].status = TodoStatus::Done;
        assert!(s.started("a", "/r/app3", Some("claude:claude2:uuid"), None, &lanes(), 100));
        let t = &s.todos[0];
        assert_eq!((t.worktrees.clone(), t.sessions.clone()), (vec!["/r/app3".to_string()], vec!["claude:claude2:uuid".to_string()]));
        assert_eq!((t.status, t.done_at, t.project.as_str()), (TodoStatus::InProgress, None, "Northwind"));
    }

    #[test]
    fn an_app_start_links_the_next_session_in_its_repo() {
        let mut s = store(&["a", "b"]);
        let at = 1_000_000;
        s.started("a", "/r/app", None, Some(pending("/r/app", "codex", "codex", at)), &lanes(), at);
        let board = [
            // Too early, the wrong account, the wrong repo, or another harness: not it.
            session("codex:codex:old", "/r/app", "codex", "codex", at - 10 * 60_000),
            session("codex:codex2:x", "/r/app", "codex", "codex2", at + 5_000),
            session("codex:codex:kestrel", "/r/kestrel", "codex", "codex", at + 5_000),
            session("claude:claude:c", "/r/app", "claude", "claude", at + 5_000),
            // Codex made a worktree of its own for the thread.
            session("codex:codex:new", "/u/.codex/worktrees/e8e5/app", "codex", "codex", at + 20_000),
        ];
        assert!(!s.resolve_pending(&board[..4], &lanes(), at + 30_000));
        assert_eq!(s.todos[0].pending.len(), 1);
        assert!(s.resolve_pending(&board, &lanes(), at + 30_000));
        assert_eq!(s.todos[0].sessions, vec!["codex:codex:new".to_string()]);
        assert!(s.todos[0].pending.is_empty());
        // A session already on a todo isn't taken again.
        s.started("b", "/r/app", None, Some(pending("/r/app", "codex", "codex", at)), &lanes(), at);
        assert!(!s.resolve_pending(&board, &lanes(), at + 40_000));
        assert_eq!(s.todos[1].pending.len(), 1);
    }

    #[test]
    fn a_start_prefers_its_own_worktree_and_gives_up_after_half_an_hour() {
        let mut s = store(&["a"]);
        let at = 5_000_000;
        s.started("a", "/r/app3", None, Some(pending("/r/app3", "claude", "claude", at)), &lanes(), at);
        let board = [session("claude:claude:elsewhere", "/r/app", "claude", "claude", at + 1_000), session("claude:claude:here", "/r/app3", "claude", "claude", at + 9_000)];
        assert!(s.resolve_pending(&board, &lanes(), at + 10_000));
        assert_eq!(s.todos[0].sessions, vec!["claude:claude:here".to_string()]);
        s.started("a", "/r/app3", None, Some(pending("/r/app3", "claude", "claude", at)), &lanes(), at);
        assert!(s.resolve_pending(&[], &lanes(), at + PENDING_MS + 1));
        assert!(s.todos[0].pending.is_empty());
    }

    #[test]
    fn unlinking_the_worktree_drops_a_start_waiting_there() {
        let mut s = store(&["a"]);
        s.started("a", "/r/app", None, Some(pending("/r/app", "codex", "codex", 1)), &lanes(), 1);
        s.apply(TodoOp::Link { id: "a".into(), worktree: Some("/r/app".into()), session: None, linked: false }, &lanes(), 2).unwrap();
        assert!(s.todos[0].pending.is_empty() && s.todos[0].worktrees.is_empty());
    }

    #[test]
    fn reads_what_it_writes_and_what_a_person_would() {
        let mut s = store(&["a"]);
        s.labels = vec![Label { id: "l1".into(), name: "Bug".into(), color: "red".into() }];
        let text = serde_json::to_string(&s).unwrap();
        assert!(text.contains(r#""status":"todo""#) && text.contains(r#""createdAt":10"#));
        assert_eq!(serde_json::from_str::<TodoStore>(&text).unwrap(), s);
        // By hand: an id and a title are enough.
        let hand: TodoStore = serde_json::from_str(r#"{"todos":[{"id":"x1","title":"Try the bar","status":"in-progress"}]}"#).unwrap();
        assert_eq!((hand.todos[0].status, hand.todos[0].labels.len()), (TodoStatus::InProgress, 0));
        let op: TodoOp = serde_json::from_str(r#"{"op":"link","id":"a","session":"claude:claude:s","linked":true}"#).unwrap();
        assert!(matches!(op, TodoOp::Link { worktree: None, linked: true, .. }));
    }
}
