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
//!
//! An open todo is Ready or Waiting. It waits on the todos it depends on until they're done,
//! and on anything else once you mark it as waiting (a person's answer, a release), until you
//! say it's ready, start a session from it, or finish it.
//!
//! A todo under way is In progress, or In review once its work waits on your verdict: marked so
//! by you or by its agent (`codealong todo review`), until it's done, started again or sent
//! back; or while a PR of its work is open for review, or merged and the todo not yet done
//! (`pr_in_review`).
//!
//! A todo can have a due date: a day on the calendar, with no time or zone, so it's the same
//! day wherever you read it. It only says when you mean it done by: the list keeps your order,
//! and nothing moves or changes status when the day comes.
//!
//! A todo has one primary session, so two agents don't do the same work: the session the work
//! on it goes on in, which other sessions send what they find instead of taking it on too. The
//! first session linked to it is the primary one, and the next one is once that one has stopped
//! (`holds`); you, or an agent told to (`codealong todo start --take`), can make another one it.
//! A session linked while the primary one runs works alongside it.
//!
//! An archived todo is put away, done or not: it leaves every list but Archived, keeping its
//! status, and nothing waits on it, as nothing waits on a deleted one. It's back where its status
//! puts it once it's unarchived, a session starts from it, or it's reopened or started
//! (`set_status`).

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use anyhow::{bail, Result};
use serde::{Deserialize, Deserializer, Serialize};

use crate::board::ELSEWHERE;
use crate::config::support_dir;
use crate::model::Lane;
use crate::util::{civil_from_days, days_from_civil};

/// How long a start in an app waits for its session to show up before it's forgotten.
pub const PENDING_MS: i64 = 30 * 60 * 1000;
/// How long a session made primary holds the todo before the board lists it: one started in a
/// terminal is linked as it starts, and shows up a few seconds later.
const NEW_PRIMARY_MS: i64 = 5 * 60 * 1000;
/// A session that says it started this long before the start still counts: clocks and
/// transcript timestamps don't agree to the millisecond.
const PENDING_SLACK_MS: i64 = 60 * 1000;
const MAX_TITLE: usize = 300;
const MAX_DESCRIPTION: usize = 100_000;
const MAX_LABEL: usize = 40;
const MAX_WAITING_NOTE: usize = 200;

/// The label colors, as `--label-<color>` tokens in design/tokens.json.
pub const LABEL_COLORS: [&str; 8] = ["gray", "red", "pink", "purple", "indigo", "blue", "teal", "green"];

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "kebab-case")]
pub enum TodoStatus {
    #[default]
    Todo,
    /// An agent is on it. The other spellings are for lists written by hand or by an agent.
    #[serde(alias = "in_progress", alias = "inprogress", alias = "doing")]
    InProgress,
    #[serde(alias = "complete", alias = "completed")]
    Done,
}

/// A todo marked as waiting, on something outside the list.
#[derive(Serialize, Clone, PartialEq, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Waiting {
    /// What it waits for, in a few words. Can be empty.
    pub note: String,
    /// When it was marked; 0 when written by hand without a time.
    pub since: i64,
}

/// A todo marked as ready for review: the agent's part of its work is done, and it waits on your
/// verdict (a look, a test, a merge).
#[derive(Serialize, Clone, PartialEq, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct TodoReview {
    /// What to look at, in a few words. Can be empty.
    pub note: String,
    /// When it was marked; 0 when written by hand without a time.
    pub since: i64,
}

/// A wait, or a review, as it's written: `{"note": …, "since": …}`, or by hand just the note, or
/// `true`; `false` is none.
#[derive(Deserialize)]
#[serde(untagged)]
enum WaitingIn {
    Flag(bool),
    Note(String),
    Full {
        #[serde(default)]
        note: String,
        #[serde(default)]
        since: i64,
    },
}

impl WaitingIn {
    fn into_waiting(self) -> Option<Waiting> {
        match self {
            WaitingIn::Flag(false) => None,
            WaitingIn::Flag(true) => Some(Waiting::default()),
            WaitingIn::Note(note) => Some(Waiting { note, since: 0 }),
            WaitingIn::Full { note, since } => Some(Waiting { note, since }),
        }
    }
}

/// A todo's `waiting`: none when it's missing, null or false.
fn waiting_field<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Waiting>, D::Error> {
    Ok(Option::<WaitingIn>::deserialize(d)?.and_then(WaitingIn::into_waiting))
}

/// A change's `waiting`: missing leaves it as it is, null or false ends it, anything else marks
/// the todo as waiting.
fn waiting_change<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Option<Waiting>>, D::Error> {
    waiting_field(d).map(Some)
}

/// A todo's `review`, written as a wait is: none when it's missing, null or false.
fn review_field<'de, D: Deserializer<'de>>(d: D) -> Result<Option<TodoReview>, D::Error> {
    Ok(waiting_field(d)?.map(|w| TodoReview { note: w.note, since: w.since }))
}

/// A change's `review`: missing leaves it as it is, null or false ends it, anything else marks
/// the todo as ready for review.
fn review_change<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Option<TodoReview>>, D::Error> {
    review_field(d).map(Some)
}

/// A due date made usable: `2026-10-03` (`2026-10-3` too), or a time on that day
/// (`2026-10-03T09:00:00Z`) as the day. Anything else, or a day the calendar doesn't have, is
/// none.
pub fn clean_due(s: &str) -> Option<String> {
    let day = s.trim().split(['T', ' ']).next()?;
    let mut parts = day.split('-').map(|p| p.parse::<i64>().ok().filter(|_| !p.is_empty() && p.len() <= 4));
    let (y, m, d) = (parts.next()??, parts.next()??, parts.next()??);
    if parts.next().is_some() || !(1970..=9999).contains(&y) || !(1..=12).contains(&m) || d < 1 {
        return None;
    }
    // A day past the month's end comes back as a day of the next month.
    if civil_from_days(days_from_civil(y, m, d)) != (y, m, d) {
        return None;
    }
    Some(format!("{y:04}-{m:02}-{d:02}"))
}

/// A due date as days since 1970-01-01, to compare with `util::today_local`.
pub fn due_day(due: &str) -> Option<i64> {
    let day = clean_due(due)?;
    let n = |r: std::ops::Range<usize>| day[r].parse::<i64>().ok();
    Some(days_from_civil(n(0..4)?, n(5..7)?, n(8..10)?))
}

/// A day as a due date is written.
pub fn day_text(days: i64) -> String {
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}")
}

const WEEKDAYS: [&str; 7] = ["sunday", "monday", "tuesday", "wednesday", "thursday", "friday", "saturday"];

/// 0 for Sunday through 6 for Saturday. 1970-01-01 was a Thursday.
fn weekday(days: i64) -> i64 {
    (days + 4).rem_euclid(7)
}

/// A due date from what someone types: a date (`2026-10-03`), `today`, `tomorrow`, a weekday
/// (`fri`, `friday`: the next one after today), `next week` (next Monday), or days and weeks
/// from today (`3d`, `2w`, `in 10 days`). `today` is days since 1970-01-01.
pub fn due_from_words(text: &str, today: i64) -> Result<String> {
    let words = text.trim().to_lowercase();
    let words = words.split_whitespace().collect::<Vec<_>>().join(" ");
    if let Some(day) = clean_due(&words) {
        return Ok(day);
    }
    let ahead_to = |wd: i64| match (wd - weekday(today)).rem_euclid(7) {
        0 => 7,
        n => n,
    };
    let days = match words.as_str() {
        "today" => Some(0),
        "tomorrow" => Some(1),
        "next week" => Some(ahead_to(1)),
        w => WEEKDAYS
            .iter()
            .position(|d| w.len() >= 3 && d.starts_with(w))
            .map(|i| ahead_to(i as i64))
            .or_else(|| {
                let w = w.strip_prefix("in ").or_else(|| w.strip_prefix('+')).unwrap_or(w).trim();
                let split = w.find(|c: char| !c.is_ascii_digit()).unwrap_or(w.len());
                let n: i64 = w[..split].parse().ok().filter(|n| *n <= 3650)?;
                match w[split..].trim() {
                    "d" | "day" | "days" => Some(n),
                    "w" | "wk" | "week" | "weeks" => Some(n * 7),
                    _ => None,
                }
            }),
    };
    match days {
        Some(n) => Ok(day_text(today + n)),
        None => bail!("\"{}\" isn't a day Codealong reads. Give a date (2026-10-03), today, tomorrow, a weekday, next week, or 3d / 2w from today.", text.trim()),
    }
}

/// How far off a due date is, from `today`: `today`, `tomorrow`, `in 3 days`, `yesterday`,
/// `3 days ago`.
pub fn due_phrase(due: &str, today: i64) -> String {
    match due_day(due).map(|d| d - today) {
        None => due.to_string(),
        Some(0) => "today".into(),
        Some(1) => "tomorrow".into(),
        Some(-1) => "yesterday".into(),
        Some(n) if n > 0 => format!("in {n} days"),
        Some(n) => format!("{} days ago", -n),
    }
}

/// A todo's `due` as it's read: any date `clean_due` takes, and nothing for anything else, so a
/// list written by hand never fails to read for its dates.
fn due_field<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    Ok(Option::<serde_json::Value>::deserialize(d)?.as_ref().and_then(|v| v.as_str()).and_then(clean_due))
}

/// A change's `due`: missing leaves it, null or empty takes it off, and anything else is checked
/// when the change is applied, so a date that doesn't read is refused rather than dropped.
fn due_change<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Option<String>>, D::Error> {
    Ok(Some(Option::<String>::deserialize(d)?))
}

/// A change's due date as the todo keeps it, or why it can't.
fn checked_due(due: Option<&str>) -> Result<Option<String>> {
    match due.map(str::trim) {
        None | Some("") => Ok(None),
        Some(d) => match clean_due(d) {
            Some(day) => Ok(Some(day)),
            None => bail!("A due date is a day, written 2026-10-03, not \"{d}\"."),
        },
    }
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

/// A todo's primary session (see the top of this file).
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Primary {
    /// Its key.
    pub session: String,
    /// When it became the primary one.
    #[serde(default)]
    pub since: i64,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Todo {
    /// Letters, digits, `-` and `_`. One written by hand without an id, or with other
    /// characters, gets one when it's read.
    #[serde(default)]
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
    /// Todos this one waits on, by id, in the order you added them. It's blocked until each
    /// is done. An id whose todo was deleted stays, so the delete can be undone, and is
    /// otherwise ignored.
    #[serde(default)]
    pub depends_on: Vec<String>,
    /// Marked as waiting, on something outside the list. Left out when it isn't.
    #[serde(default, deserialize_with = "waiting_field", skip_serializing_if = "Option::is_none")]
    pub waiting: Option<Waiting>,
    /// Marked as ready for review: only an in-progress todo is. Left out when it isn't, so a
    /// build from before reads the file as it did.
    #[serde(default, deserialize_with = "review_field", skip_serializing_if = "Option::is_none")]
    pub review: Option<TodoReview>,
    /// The day it's due, as `2026-10-03`. A done todo keeps it. Left out when it has none.
    #[serde(default, deserialize_with = "due_field", skip_serializing_if = "Option::is_none")]
    pub due: Option<String>,
    /// Its primary session, kept after that session stops as the one that had it last. Left out
    /// when it has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary: Option<Primary>,
    /// When it was archived (see the top of this file). Left out when it isn't, so a build from
    /// before reads the file as it did.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archived_at: Option<i64>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Label {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    /// One of `LABEL_COLORS`; any other is gray.
    #[serde(default)]
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
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct TodoPatch {
    pub title: Option<String>,
    pub description: Option<String>,
    pub status: Option<TodoStatus>,
    pub labels: Option<Vec<String>>,
    pub project: Option<String>,
    /// The whole list of todos this one waits on.
    pub depends_on: Option<Vec<String>>,
    /// `Some(None)` (null) says it's ready; `Some(Some(_))` marks it as waiting, keeping the
    /// time it began if it already was. Left out when it doesn't change, since null means ready.
    #[serde(default, deserialize_with = "waiting_change", skip_serializing_if = "Option::is_none")]
    pub waiting: Option<Option<Waiting>>,
    /// `Some(None)` (null) sends it back to In progress; `Some(Some(_))` marks it as ready for
    /// review, keeping the time that began if it already was. Left out when it doesn't change.
    #[serde(default, deserialize_with = "review_change", skip_serializing_if = "Option::is_none")]
    pub review: Option<Option<TodoReview>>,
    /// `Some(None)` (null) takes the due date off; `Some(Some(day))` sets it. Left out when it
    /// doesn't change.
    #[serde(default, deserialize_with = "due_change", skip_serializing_if = "Option::is_none")]
    pub due: Option<Option<String>>,
    /// Archives it, or brings it back from the archive. Left out when it doesn't change.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archived: Option<bool>,
}

/// A change from the UI. Changes are small and specific, so one made in a window never undoes
/// another made meanwhile, by another window or by the engine linking a session.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "op", rename_all = "kebab-case", rename_all_fields = "camelCase")]
pub enum TodoOp {
    /// A new todo, at the top. The window picks the id, so it can show the todo at once.
    Create { todo: Todo },
    Update { id: String, patch: TodoPatch },
    /// Links (or unlinks) a worktree or a session.
    Link { id: String, worktree: Option<String>, session: Option<String>, linked: bool },
    /// Makes a session the todo's primary one, linking it if it isn't; with none, leaves the todo
    /// without one, so the next session linked becomes it.
    Primary { id: String, session: Option<String> },
    Delete { id: String },
    /// Puts a deleted todo back where it was.
    Restore { todo: Todo, index: usize },
    /// Todo ids in their new order. Ids left out keep their places after the ones given.
    Order { ids: Vec<String> },
    /// The whole label list. A deleted label stays on its todos until it's back or they change,
    /// so deleting one can be undone.
    Labels { labels: Vec<Label> },
}

/// What linking needs to know from the board: a worktree's project, for a todo that has none
/// yet, and whether a session runs, for its primary one.
pub trait Lanes {
    fn project_of(&self, lane: &str) -> Option<String>;
    /// A repo's identity, the same for all of its worktrees.
    fn repo_of(&self, lane: &str) -> Option<String>;
    /// Whether a session is running: its process is up and it isn't archived. None when the
    /// board doesn't list it.
    fn running(&self, _session: &str) -> Option<bool> {
        None
    }
}

/// Each worktree's project and repo, by lane id, and whether each session runs, as a board has
/// them: what the engine, the `codealong` command and a start from todos link by. It's a copy, so
/// nothing holds the board while todos change.
#[derive(Default)]
pub struct LaneIndex {
    lanes: HashMap<String, (String, String)>,
    sessions: HashMap<String, bool>,
}

impl LaneIndex {
    pub fn of(lanes: &[Lane]) -> LaneIndex {
        let mut sessions = HashMap::new();
        for s in lanes.iter().flat_map(|l| &l.sessions) {
            *sessions.entry(s.key.clone()).or_insert(false) |= s.live && !s.archived;
        }
        LaneIndex { lanes: lanes.iter().map(|l| (l.id.clone(), (l.project.clone(), repo_identity(l)))).collect(), sessions }
    }
}

impl Lanes for LaneIndex {
    /// None for a worktree outside every project: a todo isn't filed under Elsewhere.
    fn project_of(&self, lane: &str) -> Option<String> {
        self.lanes.get(lane).map(|(p, _)| p.clone()).filter(|p| p != ELSEWHERE)
    }
    fn repo_of(&self, lane: &str) -> Option<String> {
        self.lanes.get(lane).map(|(_, r)| r.clone())
    }
    fn running(&self, session: &str) -> Option<bool> {
        self.sessions.get(session).copied()
    }
}

/// Whether a todo's primary session still has it: it's running, or it became primary so lately
/// that the board doesn't list it yet. `primaryOf` in src/todos.ts shows the same.
pub fn holds(p: &Primary, lanes: &dyn Lanes, now: i64) -> bool {
    match lanes.running(&p.session) {
        Some(running) => running,
        None => now - p.since < NEW_PRIMARY_MS,
    }
}

/// What every worktree of a repo shares: its GitHub slug, else its project and name.
fn repo_identity(l: &Lane) -> String {
    l.repo_slug.clone().unwrap_or_else(|| format!("{}/{}", l.project, l.repo))
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// An id made usable: other characters become `-`, and an empty one takes `fallback`.
fn usable_id(id: &str, fallback: &str) -> String {
    let cleaned: String = id.trim().chars().take(60).map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '-' }).collect();
    if cleaned.trim_matches('-').is_empty() {
        fallback.to_string()
    } else {
        cleaned
    }
}

/// A list read from disk, with every todo and label given a usable, unique id, and the labels on
/// each todo following theirs. Nothing someone wrote by hand is dropped for its id.
fn repaired(mut s: TodoStore) -> TodoStore {
    let mut seen = HashSet::new();
    let mut renamed = HashMap::new();
    for (i, l) in s.labels.iter_mut().enumerate() {
        let usable = usable_id(&l.id, &format!("l-{}", i + 1));
        let mut id = usable.clone();
        while !seen.insert(id.clone()) {
            id.push('_');
        }
        // Todos follow a label whose id had to change; a repeat keeps pointing at the first.
        if usable != l.id && !l.id.is_empty() {
            renamed.entry(l.id.clone()).or_insert_with(|| id.clone());
        }
        l.id = id;
    }
    s.labels = clean_labels(s.labels);
    let mut seen = HashSet::new();
    for (i, t) in s.todos.iter_mut().enumerate() {
        let mut id = usable_id(&t.id, &format!("t-{}", i + 1));
        while !seen.insert(id.clone()) {
            id.push('_');
        }
        t.id = id;
        let mut deps = HashSet::new();
        let own = t.id.clone();
        t.depends_on.retain(|d| d != &own && valid_id(d) && deps.insert(d.clone()));
        if t.status == TodoStatus::Done {
            t.waiting = None;
        }
        if t.status != TodoStatus::InProgress || t.waiting.is_some() {
            t.review = None;
        }
        if let Some(w) = &mut t.waiting {
            w.note = one_line(&w.note, MAX_WAITING_NOTE);
        }
        if let Some(r) = &mut t.review {
            r.note = one_line(&r.note, MAX_WAITING_NOTE);
        }
        for l in &mut t.labels {
            if let Some(new) = renamed.get(l) {
                *l = new.clone();
            }
        }
    }
    s
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

/// Sets a status, keeping `done_at` in step. A done todo isn't waiting on anything, and only one
/// in progress is in review. One reopened or started is out of the archive: its work goes on.
fn set_status(t: &mut Todo, status: TodoStatus, now: i64) {
    if status == TodoStatus::Done {
        t.waiting = None;
    } else {
        t.archived_at = None;
    }
    if status != TodoStatus::InProgress {
        t.review = None;
    }
    if t.status == status {
        return;
    }
    t.status = status;
    t.done_at = (status == TodoStatus::Done).then_some(now);
}

/// Marks a todo as waiting, or says it's ready. A new note keeps the time the wait began; a
/// wait put back by an undo keeps its own. A done todo can't be marked.
fn set_waiting(t: &mut Todo, waiting: Option<Waiting>, now: i64) {
    t.waiting = match waiting {
        Some(w) if t.status != TodoStatus::Done => {
            let since = match &t.waiting {
                Some(old) if old.since > 0 => old.since,
                _ if w.since > 0 && w.since <= now => w.since,
                _ => now,
            };
            Some(Waiting { note: one_line(&w.note, MAX_WAITING_NOTE), since })
        }
        _ => None,
    };
    // Waiting on something says its work isn't with you to review.
    if t.waiting.is_some() {
        t.review = None;
    }
}

/// Marks a todo as ready for review, or sends it back to In progress. Its work is under way, so a
/// todo not started yet is in progress from then on, and it waits on nothing outside the list.
/// The time keeps as a wait's does. A done todo can't be marked.
fn set_review(t: &mut Todo, review: Option<TodoReview>, now: i64) {
    t.review = match review {
        Some(r) if t.status != TodoStatus::Done => {
            let since = match &t.review {
                Some(old) if old.since > 0 => old.since,
                _ if r.since > 0 && r.since <= now => r.since,
                _ => now,
            };
            set_status(t, TodoStatus::InProgress, now);
            t.waiting = None;
            Some(TodoReview { note: one_line(&r.note, MAX_WAITING_NOTE), since })
        }
        _ => None,
    };
}

/// Archives a todo, keeping the time it was if it already was, or brings it back.
fn set_archived(t: &mut Todo, archived: bool, now: i64) {
    t.archived_at = archived.then(|| t.archived_at.unwrap_or(now));
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
        // One linked while the primary session runs works alongside it. A session that has
        // stopped takes over only a todo that has no primary one at all.
        let taken = match &t.primary {
            Some(p) => p.session == s || holds(p, lanes, now) || lanes.running(s) == Some(false),
            None => s.is_empty(),
        };
        if !taken {
            t.primary = Some(Primary { session: s.to_string(), since: now });
            changed = true;
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
    /// A dependency list made usable: each id once, of a todo in the list, not the todo itself.
    /// One that would close a loop (this todo waits on B, and B already waits on this one,
    /// directly or through others) is refused, since neither could ever be started.
    fn checked_dependencies(&self, id: &str, deps: &[String]) -> Result<Vec<String>> {
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for d in deps {
            if d == id || !seen.insert(d.clone()) {
                continue;
            }
            let Some(dep) = self.todos.iter().find(|t| &t.id == d) else { bail!("There's no todo {d} to wait on.") };
            if self.waits_on(d, id) {
                bail!("\"{}\" already waits on this one, so this one can't wait on it.", dep.title);
            }
            out.push(d.clone());
        }
        Ok(out)
    }

    /// Whether `from` waits on `target`, directly or through the todos it waits on.
    pub fn waits_on(&self, from: &str, target: &str) -> bool {
        let mut stack = vec![from.to_string()];
        let mut seen = HashSet::new();
        while let Some(id) = stack.pop() {
            if !seen.insert(id.clone()) {
                continue;
            }
            let Some(t) = self.todos.iter().find(|t| t.id == id) else { continue };
            for d in &t.depends_on {
                if d == target {
                    return true;
                }
                stack.push(d.clone());
            }
        }
        false
    }

    /// The todos `t` waits on that aren't done yet, in its order. Deleted ones don't count.
    pub fn blocked_by<'a>(&'a self, t: &Todo) -> Vec<&'a Todo> {
        blocked_by(t, &self.todos)
    }

    pub fn is_waiting(&self, t: &Todo) -> bool {
        is_waiting(t, &self.todos)
    }

    pub fn is_ready(&self, t: &Todo) -> bool {
        is_ready(t, &self.todos)
    }

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
                let depends_on = self.checked_dependencies(&todo.id, &todo.depends_on)?;
                let due = checked_due(todo.due.as_deref())?;
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
                    depends_on,
                    waiting: None,
                    review: None,
                    due,
                    primary: None,
                    archived_at: None,
                };
                set_status(&mut t, todo.status, now);
                // The engine keeps the clock: a new todo's wait, or review, begins now.
                set_waiting(&mut t, todo.waiting.map(|w| Waiting { since: 0, ..w }), now);
                if todo.review.is_some() {
                    set_review(&mut t, todo.review.map(|r| TodoReview { since: 0, ..r }), now);
                }
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
                let depends_on = match &patch.depends_on {
                    Some(d) => Some(self.checked_dependencies(&id, d)?),
                    None => None,
                };
                let due = match &patch.due {
                    Some(d) => Some(checked_due(d.as_deref())?),
                    None => None,
                };
                let Some(t) = self.todos.iter_mut().find(|t| t.id == id) else { bail!("That todo is gone.") };
                let before = t.clone();
                if let Some(d) = depends_on {
                    t.depends_on = d;
                }
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
                    // Saying an agent is on it says it's no longer waiting, nor waiting on you.
                    if s == TodoStatus::InProgress {
                        t.waiting = None;
                        t.review = None;
                    }
                }
                if let Some(w) = patch.waiting {
                    set_waiting(t, w, now);
                }
                if let Some(r) = patch.review {
                    set_review(t, r, now);
                }
                if let Some(d) = due {
                    t.due = d;
                }
                if let Some(a) = patch.archived {
                    set_archived(t, a, now);
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
                    if t.primary.as_ref().is_some_and(|p| Some(p.session.as_str()) == session.as_deref()) {
                        t.primary = None;
                    }
                    // Unlinking the worktree a start is waiting in gives up on that start.
                    t.pending.retain(|p| Some(p.lane.as_str()) != worktree.as_deref());
                    before != (t.worktrees.len(), t.sessions.len())
                };
                if changed {
                    t.updated_at = now;
                }
                Ok(changed)
            }
            TodoOp::Primary { id, session } => {
                let Some(t) = self.todos.iter_mut().find(|t| t.id == id) else { bail!("That todo is gone.") };
                let before = t.clone();
                match session.filter(|s| !s.is_empty()) {
                    Some(s) => {
                        link(t, None, Some(&s), lanes, now);
                        if t.primary.as_ref().is_none_or(|p| p.session != s) {
                            t.primary = Some(Primary { session: s, since: now });
                        }
                    }
                    None => t.primary = None,
                }
                let changed = *t != before;
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

    /// A session started from a todo: links its worktree (unless it runs in a new one), and the
    /// session when Codealong chose its ID, or waits for it to show up. Starting one reopens a
    /// todo that was done, and ends a wait or a review it was marked with: the work goes on.
    pub fn started(&mut self, id: &str, lane: Option<&str>, session: Option<&str>, pending: Option<PendingStart>, lanes: &dyn Lanes, now: i64) -> bool {
        let Some(t) = self.todos.iter_mut().find(|t| t.id == id) else { return false };
        link(t, lane, session, lanes, now);
        if let Some(p) = pending {
            t.pending.push(p);
        }
        set_status(t, TodoStatus::InProgress, now);
        t.waiting = None;
        t.review = None;
        t.updated_at = now;
        true
    }

    /// Links the sessions that starts in the apps were waiting for, and forgets starts that
    /// waited too long. A start takes the earliest new session of its account and harness in
    /// its worktree, else in another worktree of the same repo (the apps can make their own),
    /// that no todo has yet. A start for several todos put the same pending start on each, and
    /// they all take the session the first one takes. Returns whether anything changed.
    pub fn resolve_pending(&mut self, sessions: &[SessionRef], lanes: &dyn Lanes, now: i64) -> bool {
        if self.todos.iter().all(|t| t.pending.is_empty()) {
            return false;
        }
        let mut taken: HashSet<String> = self.todos.iter().flat_map(|t| t.sessions.iter().cloned()).collect();
        // The starts linked on this pass, and the session each took.
        let mut took: Vec<(PendingStart, String)> = Vec::new();
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
                if let Some((_, key)) = took.iter().find(|(q, _)| *q == p) {
                    link(t, None, Some(key.as_str()), lanes, now);
                    t.updated_at = now;
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
                        took.push((p.clone(), s.key.to_string()));
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

/// What a session started on a todo is asked: the title, the description, then which todo it
/// is, so the agent can mark it done, and its primary session (`primary_line`). `holding` says
/// whether that session still has it. `todoPrompt` in `src/todos.ts` writes the same text.
pub fn prompt(t: &Todo, holding: bool) -> String {
    let footer = format!("This is Codealong todo {id}. When it's ready for me to review, say so with `codealong todo review {id}`. When the work is merged or I say it's done, mark it done with `codealong todo done {id}`.", id = t.id);
    let primary = primary_line(t, holding).unwrap_or_default();
    [t.title.trim(), t.description.trim(), &footer, &primary].iter().filter(|s| !s.is_empty()).cloned().collect::<Vec<_>>().join("\n\n")
}

/// What a session started on a todo is told of the todo's primary session: that it's running,
/// so this one sends it what it finds; or that it had the todo before, where its work is.
/// `primaryLine` in `src/todos.ts` writes the same text.
fn primary_line(t: &Todo, holding: bool) -> Option<String> {
    let key = &t.primary.as_ref()?.session;
    Some(match holding {
        true => format!("{id}'s primary session is {key}, which is running: send it what you find with `codealong session send {key} \"…\"` rather than do the same work.", id = t.id),
        false => format!("Session {key} had {id} before you: `codealong session show {key}` has its records.", id = t.id),
    })
}

/// What one session started on several todos is asked: each one's title and description,
/// numbered, then which todos they are, and their primary sessions. One todo is `prompt`'s.
/// `todosPrompt` in `src/todos.ts` writes the same text.
pub fn prompt_for(ts: &[&Todo], holding: impl Fn(&Todo) -> bool) -> String {
    if let [t] = ts {
        return prompt(t, holding(t));
    }
    let mut parts = vec![format!("Work on these {} todos:", ts.len())];
    for (i, t) in ts.iter().enumerate() {
        let title = t.title.trim();
        parts.push(format!("{}. {}", i + 1, if title.is_empty() { "Untitled" } else { title }));
        if !t.description.trim().is_empty() {
            parts.push(t.description.trim().to_string());
        }
    }
    let ids: Vec<String> = ts.iter().enumerate().map(|(i, t)| format!("{} ({})", t.id, i + 1)).collect();
    let (last, rest) = ids.split_last().map(|(l, r)| (l.clone(), r.join(", "))).unwrap_or_default();
    parts.push(format!("These are Codealong todos {rest} and {last}. When one is ready for me to review, say so with `codealong todo review <id>`. When the work on one is merged or I say it's done, mark it done with `codealong todo done <id>`."));
    parts.extend(ts.iter().filter_map(|t| primary_line(t, holding(t))));
    parts.join("\n\n")
}

/// The todos `t` waits on that aren't done yet, in its order, among `todos`. Deleted and
/// archived ones don't count.
pub fn blocked_by<'a>(t: &Todo, todos: &'a [Todo]) -> Vec<&'a Todo> {
    t.depends_on.iter().filter_map(|d| todos.iter().find(|x| &x.id == d)).filter(|x| x.status != TodoStatus::Done && x.archived_at.is_none()).collect()
}

/// Whether `t` is still to do: not done, and not archived. The todos Jev links a session to, and
/// the ones `whoami` names.
pub fn is_open(t: &Todo) -> bool {
    t.status != TodoStatus::Done && t.archived_at.is_none()
}

/// Whether `t` is under Waiting: open, and marked as waiting, or not started and waiting on a todo
/// that isn't done. One an agent is already on stays In progress while its todos finish.
/// `sectionOf` in src/todos.ts has the same rule.
pub fn is_waiting(t: &Todo, todos: &[Todo]) -> bool {
    is_open(t) && (t.waiting.is_some() || (t.status == TodoStatus::Todo && !blocked_by(t, todos).is_empty()))
}

/// Whether `t` is under Ready: not started, not archived, and waiting on nothing.
pub fn is_ready(t: &Todo, todos: &[Todo]) -> bool {
    t.status == TodoStatus::Todo && t.archived_at.is_none() && !is_waiting(t, todos)
}

/// Whether `t` is under In review: in progress, not marked as waiting, and marked as ready for
/// review or its PR is in review (`pr_in_review`). `itemSection` in src/todos.ts has the same
/// rule.
pub fn is_in_review(t: &Todo, lanes: &[Lane]) -> bool {
    t.status == TodoStatus::InProgress && t.archived_at.is_none() && t.waiting.is_none() && (t.review.is_some() || pr_in_review(t, lanes))
}

/// Whether the PR of a todo's work is with its reviewers: the PR of its worktrees, then of its
/// sessions' newest first while the session is on the worktree's branch (a checkout gone on to
/// other work has that work's PR), an open one before the rest, is open and not a draft, or
/// merged while the todo isn't done, which waits on you to say it is. `prInReview` and `todoPr`
/// in src/todos.ts have the same rule.
pub fn pr_in_review(t: &Todo, lanes: &[Lane]) -> bool {
    use crate::model::PrState;
    // Its places in the order `todoItem` lists them, linked worktrees first, then the worktrees of
    // its sessions newest first, each with whether its PR is the todo's.
    let mut places: Vec<(&Lane, bool)> = t.worktrees.iter().filter_map(|w| lanes.iter().find(|l| &l.id == w)).map(|l| (l, true)).collect();
    let mut sessions: Vec<(&Lane, &crate::model::Session)> = lanes
        .iter()
        .flat_map(|l| l.sessions.iter().filter(|s| s.visitor.is_none() && t.sessions.contains(&s.key)).map(move |s| (l, s)))
        .collect();
    sessions.sort_by_key(|(_, s)| -s.last_activity);
    for (l, s) in sessions {
        let on_branch = s.branch.is_none() || s.branch == l.branch;
        match places.iter_mut().find(|(p, _)| p.id == l.id) {
            Some((_, counts)) => *counts |= on_branch,
            None => places.push((l, on_branch)),
        }
    }
    let prs: Vec<PrState> = places.iter().filter(|(_, counts)| *counts).filter_map(|(l, _)| l.pr.as_ref().map(|p| p.state)).collect();
    let pr = prs.iter().find(|s| s.is_open()).or(prs.first());
    matches!(pr, Some(PrState::Open | PrState::Approved | PrState::Changes | PrState::Merged))
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

/// An advisory lock on `todos.json.lock`, held through a read-modify-write of the store, so the
/// app and the `codealong` command never write over each other's change. Released when dropped.
pub use crate::store::Lock;

pub fn lock() -> Result<Lock> {
    crate::store::lock(&path())
}

/// Reads the store under the lock, lets `f` change it, and saves it when `f` says it changed.
/// Returns the store as it is afterwards and whether it was saved.
pub fn change<R>(f: impl FnOnce(&mut TodoStore) -> Result<(bool, R)>) -> Result<(TodoStore, bool, R)> {
    let _lock = lock()?;
    let (mut store, _) = load()?;
    let (changed, out) = f(&mut store)?;
    if changed {
        save(&store)?;
    }
    Ok((store, changed, out))
}

/// The store and its text as read, or an empty store when there's no file yet. A file that
/// doesn't parse is an error, so it's never overwritten with an empty list.
pub fn load() -> Result<(TodoStore, String)> {
    let p = path();
    let Some(text) = crate::store::text(&p)? else { return Ok((TodoStore::default(), String::new())) };
    Ok((repaired(crate::store::parse(&p, "a todo list", &text)?), text))
}

/// Writes the store through a temporary file, so the watcher never reads half a file. Returns
/// the text written.
pub fn save(store: &TodoStore) -> Result<String> {
    let text = serde_json::to_string_pretty(store)?;
    crate::store::write(&path(), &text)?;
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_boards_worktrees_are_indexed_by_project_and_repo() {
        use crate::fixtures::lane;
        let lanes = [
            Lane { repo_slug: Some("sprucehq/spark".into()), ..lane("/r/app", Vec::new()) },
            lane("/r/tools", Vec::new()),
            Lane { project: ELSEWHERE.into(), ..lane("/tmp/x", Vec::new()) },
        ];
        let index = LaneIndex::of(&lanes);
        // A repo's worktrees share its GitHub slug; a repo without one, its project and name.
        assert_eq!(index.repo_of("/r/app").as_deref(), Some("sprucehq/spark"));
        assert_eq!(index.repo_of("/r/tools").as_deref(), Some("P/r"));
        assert_eq!(index.project_of("/r/app").as_deref(), Some("P"));
        // A todo isn't filed under Elsewhere, though its worktree still has a repo to link by.
        assert_eq!(index.project_of("/tmp/x"), None);
        assert_eq!(index.repo_of("/tmp/x"), Some(format!("{ELSEWHERE}/r")));
        assert_eq!((index.project_of("/gone"), LaneIndex::default().repo_of("/r/app")), (None, None));
    }

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
            ("/r/app", ("Spruce", "sprucehq/spark")),
            ("/r/app3", ("Spruce", "sprucehq/spark")),
            ("/u/.codex/worktrees/e8e5/app", ("Spruce", "sprucehq/spark")),
            ("/r/spur", ("Spur", "spurhq/spur")),
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
            depends_on: vec![],
            waiting: None,
            review: None,
            due: None,
            primary: None,
            archived_at: None,
        }
    }

    fn wait(note: &str) -> TodoPatch {
        TodoPatch { waiting: Some(Some(Waiting { note: note.into(), since: 0 })), ..Default::default() }
    }

    fn update(s: &mut TodoStore, id: &str, patch: TodoPatch, now: i64) -> bool {
        s.apply(TodoOp::Update { id: id.into(), patch }, &lanes(), now).unwrap()
    }

    #[test]
    fn waiting_on_open_todos_until_they_are_done() {
        let mut s = store(&["a", "b"]);
        update(&mut s, "a", TodoPatch { depends_on: Some(vec!["b".into()]), ..Default::default() }, 20);
        let a = |s: &TodoStore| s.todos[0].clone();
        assert!(s.is_waiting(&a(&s)) && !s.is_ready(&a(&s)));
        assert!(s.is_ready(&s.todos[1]));
        // Once an agent is on it, it's in progress, whatever it waits on.
        update(&mut s, "a", TodoPatch { status: Some(TodoStatus::InProgress), ..Default::default() }, 30);
        assert!(!s.is_waiting(&a(&s)) && !s.is_ready(&a(&s)));
        update(&mut s, "a", TodoPatch { status: Some(TodoStatus::Todo), ..Default::default() }, 40);
        update(&mut s, "b", TodoPatch { status: Some(TodoStatus::Done), ..Default::default() }, 50);
        assert!(s.is_ready(&a(&s)));
        // Done is neither.
        assert!(!s.is_waiting(&s.todos[1]) && !s.is_ready(&s.todos[1]));
    }

    #[test]
    fn marked_as_waiting_until_ready_started_or_done() {
        let mut s = store(&["a"]);
        assert!(update(&mut s, "a", wait("  Graham's\n retest letter "), 20));
        assert_eq!(s.todos[0].waiting, Some(Waiting { note: "Graham's retest letter".into(), since: 20 }));
        assert!(s.is_waiting(&s.todos[0]) && !s.is_ready(&s.todos[0]));
        // A new note keeps the time it began.
        update(&mut s, "a", wait("Graham's letter"), 30);
        assert_eq!(s.todos[0].waiting, Some(Waiting { note: "Graham's letter".into(), since: 20 }));
        // Ready ends it.
        assert!(update(&mut s, "a", TodoPatch { waiting: Some(None), ..Default::default() }, 40));
        assert!(s.todos[0].waiting.is_none() && s.is_ready(&s.todos[0]));
        // Linking a session starts the todo but doesn't say the wait is over; marking it in
        // progress does.
        update(&mut s, "a", wait(""), 50);
        s.apply(TodoOp::Link { id: "a".into(), worktree: None, session: Some("claude:claude:s1".into()), linked: true }, &lanes(), 60).unwrap();
        assert_eq!(s.todos[0].status, TodoStatus::InProgress);
        assert!(s.is_waiting(&s.todos[0]));
        update(&mut s, "a", TodoPatch { status: Some(TodoStatus::InProgress), ..Default::default() }, 70);
        assert!(s.todos[0].waiting.is_none() && !s.is_waiting(&s.todos[0]));
        // Unless the same change marks it again.
        let both = TodoPatch { status: Some(TodoStatus::InProgress), ..wait("the release") };
        update(&mut s, "a", both, 80);
        assert_eq!(s.todos[0].waiting.as_ref().map(|w| (w.note.as_str(), w.since)), Some(("the release", 80)));
        // Starting a session from it ends the wait.
        s.started("a", Some("/r/app"), Some("claude:claude:s2"), None, &lanes(), 90);
        assert!(s.todos[0].waiting.is_none());
        // Done ends it, and a done todo can't be marked.
        update(&mut s, "a", wait("x"), 100);
        update(&mut s, "a", TodoPatch { status: Some(TodoStatus::Done), ..Default::default() }, 110);
        assert!(s.todos[0].waiting.is_none());
        assert!(!update(&mut s, "a", wait("x"), 120));
        // An undo puts a wait back with its own time.
        update(&mut s, "a", TodoPatch { status: Some(TodoStatus::Todo), ..Default::default() }, 130);
        update(&mut s, "a", TodoPatch { waiting: Some(Some(Waiting { note: "back".into(), since: 25 })), ..Default::default() }, 140);
        assert_eq!(s.todos[0].waiting, Some(Waiting { note: "back".into(), since: 25 }));
        // A new todo can wait from the start; its wait begins when the engine makes it.
        let mut t = todo("b");
        t.waiting = Some(Waiting { note: "the vendor".into(), since: 1 });
        s.apply(TodoOp::Create { todo: t }, &lanes(), 150).unwrap();
        assert_eq!(s.todos[0].waiting, Some(Waiting { note: "the vendor".into(), since: 150 }));
    }

    fn review(note: &str) -> TodoPatch {
        TodoPatch { review: Some(Some(TodoReview { note: note.into(), since: 0 })), ..Default::default() }
    }

    #[test]
    fn marked_as_ready_for_review_until_done_started_or_sent_back() {
        let mut s = store(&["a"]);
        // Its work is under way once it's in review, and it waits on nothing outside the list.
        update(&mut s, "a", wait("Graham"), 10);
        assert!(update(&mut s, "a", review("  the bar\n in dark mode "), 20));
        assert_eq!(s.todos[0].review, Some(TodoReview { note: "the bar in dark mode".into(), since: 20 }));
        assert_eq!((s.todos[0].status, s.todos[0].waiting.is_none()), (TodoStatus::InProgress, true));
        assert!(is_in_review(&s.todos[0], &[]));
        // A new note keeps the time it began; null sends it back.
        update(&mut s, "a", review("the bar"), 30);
        assert_eq!(s.todos[0].review.as_ref().map(|r| r.since), Some(20));
        assert!(update(&mut s, "a", TodoPatch { review: Some(None), ..Default::default() }, 40));
        assert!(s.todos[0].review.is_none() && !is_in_review(&s.todos[0], &[]));
        // Marking it waiting, or saying an agent is on it, ends a review.
        update(&mut s, "a", review(""), 50);
        update(&mut s, "a", wait("the release"), 60);
        assert!(s.todos[0].review.is_none());
        update(&mut s, "a", review(""), 70);
        update(&mut s, "a", TodoPatch { status: Some(TodoStatus::InProgress), ..Default::default() }, 80);
        assert!(s.todos[0].review.is_none());
        // Starting a session from it ends it: the work goes on.
        update(&mut s, "a", review(""), 90);
        s.started("a", Some("/r/app"), Some("claude:claude:s2"), None, &lanes(), 100);
        assert!(s.todos[0].review.is_none());
        // Done, or back to the list, ends it, and a done todo can't be marked.
        update(&mut s, "a", review(""), 110);
        update(&mut s, "a", TodoPatch { status: Some(TodoStatus::Done), ..Default::default() }, 120);
        assert!(s.todos[0].review.is_none());
        assert!(!update(&mut s, "a", review("x"), 130));
        update(&mut s, "a", TodoPatch { status: Some(TodoStatus::Todo), ..Default::default() }, 140);
        update(&mut s, "a", review(""), 150);
        update(&mut s, "a", TodoPatch { status: Some(TodoStatus::Todo), ..Default::default() }, 160);
        assert!(s.todos[0].review.is_none());
        // A file says what it likes: only an in-progress todo that isn't waiting keeps one.
        let hand: TodoStore = serde_json::from_str(
            r#"{"todos":[{"id":"a","status":"in-progress","review":true},{"id":"b","status":"in-progress","review":"the docs"},{"id":"c","status":"todo","review":true},{"id":"d","status":"in-progress","waiting":true,"review":true},{"id":"e","status":"in-progress","review":false}]}"#,
        )
        .unwrap();
        let s = repaired(hand);
        let notes: Vec<Option<&str>> = s.todos.iter().map(|t| t.review.as_ref().map(|r| r.note.as_str())).collect();
        assert_eq!(notes, vec![Some(""), Some("the docs"), None, None, None]);
        let text = serde_json::to_string(&s).unwrap();
        assert_eq!(text.matches("\"review\"").count(), 2, "{text}");
        assert_eq!(serde_json::from_str::<TodoStore>(&text).unwrap(), s);
        // A change without `review` leaves it.
        let TodoOp::Update { patch, .. } = serde_json::from_str(r#"{"op":"update","id":"a","patch":{"title":"T"}}"#).unwrap() else { panic!() };
        assert!(patch.review.is_none());
    }

    #[test]
    fn a_todo_is_in_review_while_its_pr_is_with_its_reviewers() {
        use crate::fixtures::{lane, session};
        use crate::model::{Pr, PrState};
        let pr = |state: PrState| Some(Pr { number: 7, title: String::new(), url: String::new(), state, checks: Default::default(), merge: Default::default(), updated_at: 0, merged_at: None, closed_at: None, stale_approval: None });
        let mut t = Todo { status: TodoStatus::InProgress, worktrees: vec!["/r/app".into()], ..todo("a") };
        let with = |state: PrState| [Lane { pr: pr(state), ..lane("/r/app", Vec::new()) }];
        assert!(is_in_review(&t, &with(PrState::Open)) && is_in_review(&t, &with(PrState::Changes)) && is_in_review(&t, &with(PrState::Approved)));
        // Merged, it waits on you to say it's done; a draft is still being written.
        assert!(is_in_review(&t, &with(PrState::Merged)));
        assert!(!is_in_review(&t, &with(PrState::Draft)) && !is_in_review(&t, &with(PrState::Closed)));
        // Through a session's worktree too, an open PR (a draft here) before a closed one.
        let lanes = [
            Lane { pr: pr(PrState::Closed), ..lane("/r/app", Vec::new()) },
            Lane { pr: pr(PrState::Draft), ..lane("/r/app3", vec![session("claude:claude:s1")]) },
        ];
        t.sessions = vec!["claude:claude:s1".into()];
        assert!(!is_in_review(&t, &lanes));
        // A session's worktree that has gone on to another branch has that work's PR, not this one's.
        let elsewhere = |branch: &str| {
            let s = crate::model::Session { branch: Some(branch.into()), ..session("claude:claude:s1") };
            [Lane { pr: pr(PrState::Open), branch: Some("next-work".into()), ..lane("/r/app3", vec![s]) }]
        };
        let todo = Todo { worktrees: vec![], ..t.clone() };
        assert!(!is_in_review(&todo, &elsewhere("first-work")));
        assert!(is_in_review(&todo, &elsewhere("next-work")));
        // The worktrees go in the order of their newest session, on the branch or not, as the
        // app lists them: A's draft comes before B's open PR, as `todoPr` in src/todos.ts has it.
        let on = |key: &str, branch: &str, at: i64| crate::model::Session { branch: Some(branch.into()), last_activity: at, ..session(key) };
        let lanes = [
            Lane { pr: pr(PrState::Draft), branch: Some("a".into()), ..lane("/r/a", vec![on("claude:claude:s1", "other", 300), on("claude:claude:s2", "a", 100)]) },
            Lane { pr: pr(PrState::Open), branch: Some("b".into()), ..lane("/r/b", vec![on("claude:claude:s3", "b", 200)]) },
        ];
        let three = Todo { worktrees: vec![], sessions: vec!["claude:claude:s1".into(), "claude:claude:s2".into(), "claude:claude:s3".into()], ..t.clone() };
        assert!(!is_in_review(&three, &lanes));
        // Waiting, or not started, it isn't.
        assert!(!is_in_review(&Todo { waiting: Some(Waiting::default()), ..t.clone() }, &with(PrState::Open)));
        assert!(!is_in_review(&Todo { status: TodoStatus::Todo, ..t }, &with(PrState::Open)));
    }

    #[test]
    fn waiting_reads_as_written_by_hand_and_changes_travel_intact() {
        let hand: TodoStore = serde_json::from_str(
            r#"{"todos":[{"id":"a","waiting":true},{"id":"b","waiting":"Graham"},{"id":"c","waiting":false},{"id":"d","waiting":{"note":"x"}},{"id":"e"},{"id":"f","waiting":null},{"id":"g","status":"done","waiting":"late"}]}"#,
        )
        .unwrap();
        let s = repaired(hand);
        let notes: Vec<Option<&str>> = s.todos.iter().map(|t| t.waiting.as_ref().map(|w| w.note.as_str())).collect();
        assert_eq!(notes, vec![Some(""), Some("Graham"), None, Some("x"), None, None, None]);
        // Only a waiting todo carries the field.
        let text = serde_json::to_string(&s).unwrap();
        assert_eq!(text.matches("\"waiting\"").count(), 3, "{text}");
        assert_eq!(serde_json::from_str::<TodoStore>(&text).unwrap(), s);
        // A change without `waiting` leaves it; null says it's ready.
        let op = |json: &str| match serde_json::from_str::<TodoOp>(json).unwrap() {
            TodoOp::Update { patch, .. } => patch.waiting,
            _ => panic!("not an update"),
        };
        assert_eq!(op(r#"{"op":"update","id":"a","patch":{"title":"T"}}"#), None);
        assert_eq!(op(r#"{"op":"update","id":"a","patch":{"waiting":null}}"#), Some(None));
        assert_eq!(op(r#"{"op":"update","id":"a","patch":{"waiting":{"note":"n","since":5}}}"#), Some(Some(Waiting { note: "n".into(), since: 5 })));
        // And each survives the trip from the command to the app.
        for patch in [TodoPatch::default(), TodoPatch { waiting: Some(None), ..Default::default() }, wait("n")] {
            let text = serde_json::to_string(&TodoOp::Update { id: "a".into(), patch: patch.clone() }).unwrap();
            let TodoOp::Update { patch: back, .. } = serde_json::from_str(&text).unwrap() else { panic!() };
            assert_eq!(back.waiting, patch.waiting, "{text}");
        }
    }

    #[test]
    fn due_dates_are_days() {
        assert_eq!(clean_due("2026-10-03").as_deref(), Some("2026-10-03"));
        assert_eq!(clean_due(" 2026-10-3 ").as_deref(), Some("2026-10-03"));
        assert_eq!(clean_due("2026-10-03T23:30:00-07:00").as_deref(), Some("2026-10-03"));
        assert_eq!(clean_due("2028-02-29").as_deref(), Some("2028-02-29"));
        for bad in ["2026-02-29", "2026-13-01", "2026-10-00", "Oct 3", "2026-10", "10-03-2026", "", "2026--10-03"] {
            assert_eq!(clean_due(bad), None, "{bad}");
        }
        let today = days_from_civil(2026, 9, 30);
        assert_eq!(due_day("2026-10-03"), Some(today + 3));
        assert_eq!(day_text(today), "2026-09-30");
        let phrases: Vec<String> = [0, 1, -1, 5, -9].iter().map(|n| due_phrase(&day_text(today + n), today)).collect();
        assert_eq!(phrases, vec!["today", "tomorrow", "yesterday", "in 5 days", "9 days ago"]);
    }

    #[test]
    fn due_dates_read_from_words() {
        // 2026-09-30 is a Wednesday.
        let today = days_from_civil(2026, 9, 30);
        let read = |w: &str| due_from_words(w, today).unwrap();
        assert_eq!(read("today"), "2026-09-30");
        assert_eq!(read(" Tomorrow "), "2026-10-01");
        assert_eq!(read("fri"), "2026-10-02");
        assert_eq!(read("Friday"), "2026-10-02");
        // A weekday is the next one after today: today's name is a week off.
        assert_eq!(read("wed"), "2026-10-07");
        assert_eq!(read("next week"), "2026-10-05");
        assert_eq!(read("3d"), "2026-10-03");
        assert_eq!(read("+2w"), "2026-10-14");
        assert_eq!(read("in 10 days"), "2026-10-10");
        assert_eq!(read("2026-12-01"), "2026-12-01");
        for bad in ["someday", "t", "next friday", "3 months", "2026-02-30"] {
            assert!(due_from_words(bad, today).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_due_date_is_set_changed_and_taken_off() {
        let mut s = store(&["a"]);
        let due = |d: Option<&str>| TodoPatch { due: Some(d.map(String::from)), ..Default::default() };
        assert!(update(&mut s, "a", due(Some("2026-10-3")), 20));
        assert_eq!(s.todos[0].due.as_deref(), Some("2026-10-03"));
        assert!(!update(&mut s, "a", due(Some("2026-10-03")), 30));
        // One that doesn't read is refused, and the date it had stays.
        let err = s.apply(TodoOp::Update { id: "a".into(), patch: due(Some("soon")) }, &lanes(), 40).unwrap_err();
        assert!(err.to_string().contains("2026-10-03"), "{err}");
        assert_eq!(s.todos[0].due.as_deref(), Some("2026-10-03"));
        // A done todo keeps it: whether it was done in time is still worth knowing.
        update(&mut s, "a", TodoPatch { status: Some(TodoStatus::Done), ..Default::default() }, 50);
        assert_eq!(s.todos[0].due.as_deref(), Some("2026-10-03"));
        // Other changes leave it; null or empty takes it off.
        update(&mut s, "a", TodoPatch { title: Some("T".into()), ..Default::default() }, 60);
        assert!(s.todos[0].due.is_some());
        assert!(update(&mut s, "a", due(None), 70));
        assert_eq!(s.todos[0].due, None);
        update(&mut s, "a", due(Some("2026-11-01")), 80);
        assert!(update(&mut s, "a", due(Some("")), 90));
        assert_eq!(s.todos[0].due, None);
        // A new todo can have one from the start.
        let mut t = todo("b");
        t.due = Some("2026-12-24".into());
        s.apply(TodoOp::Create { todo: t }, &lanes(), 100).unwrap();
        assert_eq!(s.todos[0].due.as_deref(), Some("2026-12-24"));
    }

    #[test]
    fn due_dates_read_as_written_by_hand_and_changes_travel_intact() {
        let hand: TodoStore = serde_json::from_str(
            r#"{"todos":[{"id":"a","due":"2026-10-03"},{"id":"b","due":"next week"},{"id":"c","due":20261003},{"id":"d","due":null},{"id":"e","due":"2026-10-03T09:00:00Z"}]}"#,
        )
        .unwrap();
        let dues: Vec<Option<&str>> = hand.todos.iter().map(|t| t.due.as_deref()).collect();
        assert_eq!(dues, vec![Some("2026-10-03"), None, None, None, Some("2026-10-03")]);
        // Only a todo with a due date carries the field.
        let text = serde_json::to_string(&hand).unwrap();
        assert_eq!(text.matches("\"due\"").count(), 2, "{text}");
        let op = |json: &str| match serde_json::from_str::<TodoOp>(json).unwrap() {
            TodoOp::Update { patch, .. } => patch.due,
            _ => panic!("not an update"),
        };
        assert_eq!(op(r#"{"op":"update","id":"a","patch":{"title":"T"}}"#), None);
        assert_eq!(op(r#"{"op":"update","id":"a","patch":{"due":null}}"#), Some(None));
        assert_eq!(op(r#"{"op":"update","id":"a","patch":{"due":"2026-10-03"}}"#), Some(Some("2026-10-03".into())));
        for due in [None, Some(None), Some(Some("2026-10-03".to_string()))] {
            let patch = TodoPatch { due: due.clone(), ..Default::default() };
            let text = serde_json::to_string(&TodoOp::Update { id: "a".into(), patch }).unwrap();
            let TodoOp::Update { patch: back, .. } = serde_json::from_str(&text).unwrap() else { panic!() };
            assert_eq!(back.due, due, "{text}");
        }
    }

    #[test]
    fn dependencies_wait_on_real_todos_and_never_loop() {
        let mut s = store(&["a", "b", "c"]);
        let deps = |ids: &[&str]| TodoPatch { depends_on: Some(ids.iter().map(|x| x.to_string()).collect()), ..Default::default() };
        // Repeats and the todo itself drop out.
        assert!(s.apply(TodoOp::Update { id: "a".into(), patch: deps(&["b", "b", "a"]) }, &lanes(), 20).unwrap());
        assert_eq!(s.todos[0].depends_on, vec!["b"]);
        assert!(s.apply(TodoOp::Update { id: "b".into(), patch: deps(&["c"]) }, &lanes(), 20).unwrap());
        assert!(s.waits_on("a", "c") && !s.waits_on("c", "a"));
        // c → a would close a → b → c → a.
        let err = s.apply(TodoOp::Update { id: "c".into(), patch: deps(&["a"]) }, &lanes(), 30).unwrap_err();
        assert!(err.to_string().contains("already waits on this one"), "{err}");
        assert!(s.apply(TodoOp::Update { id: "a".into(), patch: deps(&["zz"]) }, &lanes(), 30).is_err());
        // Blocked until what it waits on is done.
        let a = s.todos[0].clone();
        assert_eq!(s.blocked_by(&a).iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), vec!["b"]);
        let done = TodoPatch { status: Some(TodoStatus::Done), ..Default::default() };
        s.apply(TodoOp::Update { id: "b".into(), patch: done }, &lanes(), 40).unwrap();
        assert!(s.blocked_by(&a).is_empty());
        // A deleted dependency stays on the todo, for undo, and doesn't block.
        s.apply(TodoOp::Delete { id: "b".into() }, &lanes(), 50).unwrap();
        let a = s.todos.iter().find(|t| t.id == "a").unwrap().clone();
        assert_eq!(a.depends_on, vec!["b"]);
        assert!(s.blocked_by(&a).is_empty());
        // A new todo can wait on others from the start.
        let mut d = todo("d");
        d.depends_on = vec!["c".into(), "d".into()];
        s.apply(TodoOp::Create { todo: d }, &lanes(), 60).unwrap();
        assert_eq!(s.todos[0].depends_on, vec!["c"]);
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
        assert_eq!(s.todos[0].project, "Spruce");
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

    fn archive(on: bool) -> TodoPatch {
        TodoPatch { archived: Some(on), ..TodoPatch::default() }
    }

    #[test]
    fn an_archived_todo_keeps_its_status_and_is_under_no_section() {
        let mut s = store(&["a", "b"]);
        update(&mut s, "a", review(""), 10);
        assert!(update(&mut s, "a", archive(true), 20));
        let a = s.todos[0].clone();
        assert_eq!((a.archived_at, a.status, a.review.is_some()), (Some(20), TodoStatus::InProgress, true));
        assert!(!is_open(&a) && !s.is_ready(&a) && !s.is_waiting(&a) && !is_in_review(&a, &[]));
        // Archiving again keeps the time it was; nothing new is no change.
        assert!(!update(&mut s, "a", archive(true), 30));
        assert_eq!(s.todos[0].archived_at, Some(20));
        // Unarchived, it's back where its status puts it.
        assert!(update(&mut s, "a", archive(false), 40));
        assert!(s.todos[0].archived_at.is_none() && is_in_review(&s.todos[0], &[]));
        // A done one stays done, and marking an archived one done keeps it archived.
        update(&mut s, "b", TodoPatch { status: Some(TodoStatus::Done), ..archive(true) }, 50);
        update(&mut s, "b", TodoPatch { status: Some(TodoStatus::Done), ..TodoPatch::default() }, 60);
        assert_eq!((s.todos[1].status, s.todos[1].archived_at), (TodoStatus::Done, Some(50)));
    }

    #[test]
    fn reopening_starting_or_a_start_from_it_brings_an_archived_todo_back() {
        let mut s = store(&["a"]);
        for (status, at) in [(TodoStatus::Todo, 10), (TodoStatus::InProgress, 30)] {
            update(&mut s, "a", archive(true), at);
            update(&mut s, "a", TodoPatch { status: Some(status), ..TodoPatch::default() }, at + 10);
            assert!(s.todos[0].archived_at.is_none(), "{status:?}");
        }
        update(&mut s, "a", archive(true), 50);
        s.started("a", Some("/r/app"), Some("claude:claude:s1"), None, &lanes(), 60);
        assert!(s.todos[0].archived_at.is_none());
        // A new todo isn't made archived.
        let t = Todo { archived_at: Some(5), ..todo("b") };
        s.apply(TodoOp::Create { todo: t }, &lanes(), 70).unwrap();
        assert!(s.todos[0].archived_at.is_none());
    }

    #[test]
    fn nothing_waits_on_an_archived_todo() {
        let mut s = store(&["a", "b"]);
        update(&mut s, "a", TodoPatch { depends_on: Some(vec!["b".into()]), ..TodoPatch::default() }, 10);
        assert!(s.is_waiting(&s.todos[0]));
        update(&mut s, "b", archive(true), 20);
        assert!(s.is_ready(&s.todos[0]) && s.blocked_by(&s.todos[0]).is_empty());
        // It still has it on its list, as it has a deleted one, for when it's back.
        assert_eq!(s.todos[0].depends_on, vec!["b".to_string()]);
        update(&mut s, "b", archive(false), 30);
        assert!(s.is_waiting(&s.todos[0]));
    }

    #[test]
    fn archived_is_written_only_when_it_is_and_reads_back() {
        let t = todo("a");
        assert!(!serde_json::to_string(&t).unwrap().contains("archived"));
        let read: Todo = serde_json::from_str(r#"{"id":"a","status":"done","archivedAt":5}"#).unwrap();
        assert_eq!(read.archived_at, Some(5));
        let patch: TodoPatch = serde_json::from_str(r#"{"archived":true}"#).unwrap();
        assert_eq!(patch.archived, Some(true));
        assert!(!serde_json::to_string(&TodoPatch::default()).unwrap().contains("archived"));
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
        s.apply(TodoOp::Link { id: "b".into(), worktree: Some("/r/spur".into()), session: None, linked: true }, &lanes(), 20).unwrap();
        assert_eq!((s.todos[0].status, s.todos[0].project.as_str()), (TodoStatus::Todo, "Spur"));
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

    /// The fake board's worktrees, with the sessions it lists: each running or not.
    struct Board(FakeLanes, HashMap<&'static str, bool>);

    impl Lanes for Board {
        fn project_of(&self, lane: &str) -> Option<String> {
            self.0.project_of(lane)
        }
        fn repo_of(&self, lane: &str) -> Option<String> {
            self.0.repo_of(lane)
        }
        fn running(&self, session: &str) -> Option<bool> {
            self.1.get(session).copied()
        }
    }

    fn board(sessions: &[(&'static str, bool)]) -> Board {
        Board(lanes(), sessions.iter().copied().collect())
    }

    fn primary(s: &TodoStore) -> Option<&str> {
        s.todos[0].primary.as_ref().map(|p| p.session.as_str())
    }

    #[test]
    fn the_first_session_on_a_todo_is_its_primary_one_until_it_stops() {
        let mut s = store(&["a"]);
        let link = |session: &str| TodoOp::Link { id: "a".into(), worktree: None, session: Some(session.into()), linked: true };
        let on = board(&[("c:1", true), ("c:2", true), ("c:3", true)]);
        s.apply(link("c:1"), &on, 10).unwrap();
        assert_eq!(primary(&s), Some("c:1"));
        assert_eq!(s.todos[0].primary.as_ref().unwrap().since, 10);
        // Another linked while it runs works alongside it.
        s.apply(link("c:2"), &on, 20).unwrap();
        assert_eq!((primary(&s), s.todos[0].sessions.len()), (Some("c:1"), 2));
        // Once it has stopped, the next session linked takes the todo; linking again one that's on
        // it already does too.
        let stopped = board(&[("c:1", false), ("c:2", true), ("c:3", true)]);
        assert!(s.apply(link("c:2"), &stopped, 30).unwrap());
        assert_eq!(primary(&s), Some("c:2"));
        // An archived session or one whose process ended isn't running (`LaneIndex`); one the
        // board doesn't list holds it only for a moment after it took it, before it shows up.
        let gone = board(&[]);
        s.apply(link("c:3"), &gone, 30 + NEW_PRIMARY_MS - 1).unwrap();
        assert_eq!(primary(&s), Some("c:2"));
        s.apply(TodoOp::Link { id: "a".into(), worktree: None, session: Some("c:4".into()), linked: true }, &gone, 30 + NEW_PRIMARY_MS).unwrap();
        assert_eq!(primary(&s), Some("c:4"));
    }

    #[test]
    fn a_session_that_has_stopped_takes_over_only_a_todo_no_session_has_had() {
        let link = |session: &str| TodoOp::Link { id: "a".into(), worktree: None, session: Some(session.into()), linked: true };
        let all_stopped = board(&[("c:1", false), ("c:2", false)]);
        let mut s = store(&["a"]);
        s.apply(link("c:1"), &all_stopped, 10).unwrap();
        assert_eq!(primary(&s), Some("c:1"));
        // The primary one stays the one that had it last.
        s.apply(link("c:2"), &all_stopped, 20).unwrap();
        assert_eq!(primary(&s), Some("c:1"));
    }

    #[test]
    fn a_session_is_made_primary_by_hand_and_unlinking_it_leaves_none() {
        let mut s = store(&["a"]);
        let on = board(&[("c:1", true), ("c:2", true)]);
        s.apply(TodoOp::Link { id: "a".into(), worktree: None, session: Some("c:1".into()), linked: true }, &on, 10).unwrap();
        // Made primary, a session is linked too, and the todo is under way.
        assert!(s.apply(TodoOp::Primary { id: "a".into(), session: Some("c:2".into()) }, &on, 20).unwrap());
        assert_eq!((primary(&s), s.todos[0].sessions.clone(), s.todos[0].updated_at), (Some("c:2"), vec!["c:1".to_string(), "c:2".to_string()], 20));
        assert!(!s.apply(TodoOp::Primary { id: "a".into(), session: Some("c:2".into()) }, &on, 30).unwrap());
        // Unlinked, the primary session leaves the todo without one; the next linked is it.
        s.apply(TodoOp::Link { id: "a".into(), worktree: None, session: Some("c:2".into()), linked: false }, &on, 40).unwrap();
        assert_eq!(primary(&s), None);
        // None leaves it without one, so a start can take it.
        s.apply(TodoOp::Primary { id: "a".into(), session: Some("c:1".into()) }, &on, 50).unwrap();
        assert!(s.apply(TodoOp::Primary { id: "a".into(), session: None }, &on, 60).unwrap());
        assert_eq!(primary(&s), None);
        assert!(s.apply(TodoOp::Primary { id: "gone".into(), session: None }, &on, 70).is_err());
    }

    #[test]
    fn a_start_on_a_todo_whose_primary_session_runs_works_alongside_it() {
        let mut s = store(&["a"]);
        let on = board(&[("claude:claude:s1", true)]);
        s.started("a", Some("/r/app"), Some("claude:claude:s1"), None, &on, 10);
        s.started("a", Some("/r/app"), Some("claude:claude:s2"), None, &on, 20);
        assert_eq!((primary(&s), s.todos[0].sessions.len()), (Some("claude:claude:s1"), 2));
        // An app's start, linked when its session shows up, does the same.
        let p = PendingStart { lane: "/r/app".into(), harness: "codex".into(), account: "codex".into(), at: 30 };
        s.started("a", Some("/r/app"), None, Some(p), &on, 30);
        s.resolve_pending(&[session("codex:codex:t1", "/r/app", "codex", "codex", 40)], &on, 40);
        assert_eq!((primary(&s), s.todos[0].sessions.len()), (Some("claude:claude:s1"), 3));
        // Once the primary session stops, a start takes the todo.
        let stopped = board(&[("claude:claude:s1", false)]);
        s.started("a", Some("/r/app"), Some("claude:claude:s3"), None, &stopped, 50);
        assert_eq!(primary(&s), Some("claude:claude:s3"));
    }

    #[test]
    fn a_todos_primary_session_is_written_only_when_it_has_one() {
        let t = todo("a");
        assert!(!serde_json::to_string(&t).unwrap().contains("primary"));
        let read: Todo = serde_json::from_str(r#"{"id":"a","primary":{"session":"c:1","since":5}}"#).unwrap();
        assert_eq!(read.primary, Some(Primary { session: "c:1".into(), since: 5 }));
        // Written by hand without a time, it's as old as can be: it holds only while it runs.
        let read: Todo = serde_json::from_str(r#"{"id":"a","primary":{"session":"c:1"}}"#).unwrap();
        assert!(!holds(read.primary.as_ref().unwrap(), &board(&[]), NEW_PRIMARY_MS));
        assert!(holds(read.primary.as_ref().unwrap(), &board(&[("c:1", true)]), NEW_PRIMARY_MS));
    }

    fn session<'a>(key: &'a str, lane: &'a str, harness: &'a str, account: &'a str, started: i64) -> SessionRef<'a> {
        SessionRef { key, lane, harness, account, started }
    }

    #[test]
    fn a_terminal_start_links_its_session_at_once() {
        let mut s = store(&["a"]);
        s.todos[0].status = TodoStatus::Done;
        assert!(s.started("a", Some("/r/app3"), Some("claude:claude2:uuid"), None, &lanes(), 100));
        let t = &s.todos[0];
        assert_eq!((t.worktrees.clone(), t.sessions.clone()), (vec!["/r/app3".to_string()], vec!["claude:claude2:uuid".to_string()]));
        assert_eq!((t.status, t.done_at, t.project.as_str()), (TodoStatus::InProgress, None, "Spruce"));
        // In a new worktree, only the session is linked: it won't be in the one it started from.
        let mut s = store(&["b"]);
        s.started("b", None, Some("claude:claude:new-wt"), None, &lanes(), 100);
        assert_eq!((s.todos[0].worktrees.len(), s.todos[0].sessions.len()), (0, 1));
    }

    #[test]
    fn an_app_start_links_the_next_session_in_its_repo() {
        let mut s = store(&["a", "b"]);
        let at = 1_000_000;
        s.started("a", Some("/r/app"), None, Some(pending("/r/app", "codex", "codex", at)), &lanes(), at);
        let board = [
            // Too early, the wrong account, the wrong repo, or another harness: not it.
            session("codex:codex:old", "/r/app", "codex", "codex", at - 10 * 60_000),
            session("codex:codex2:x", "/r/app", "codex", "codex2", at + 5_000),
            session("codex:codex:spur", "/r/spur", "codex", "codex", at + 5_000),
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
        s.started("b", Some("/r/app"), None, Some(pending("/r/app", "codex", "codex", at)), &lanes(), at);
        assert!(!s.resolve_pending(&board, &lanes(), at + 40_000));
        assert_eq!(s.todos[1].pending.len(), 1);
    }

    #[test]
    fn todos_started_together_in_an_app_take_one_session() {
        let mut s = store(&["a", "b", "c"]);
        let at = 2_000_000;
        // One start for a and c; b waits on a start of its own in the same worktree, a moment
        // later.
        for id in ["a", "c"] {
            s.started(id, Some("/r/app"), None, Some(pending("/r/app", "claude", "claude", at)), &lanes(), at);
        }
        s.started("b", Some("/r/app"), None, Some(pending("/r/app", "claude", "claude", at + 1)), &lanes(), at + 1);
        let board = [session("claude:claude:first", "/r/app", "claude", "claude", at + 4_000), session("claude:claude:second", "/r/app", "claude", "claude", at + 6_000)];
        assert!(s.resolve_pending(&board[..1], &lanes(), at + 10_000));
        assert_eq!(s.todos[0].sessions, vec!["claude:claude:first".to_string()]);
        assert_eq!(s.todos[2].sessions, vec!["claude:claude:first".to_string()]);
        assert!(s.todos[0].pending.is_empty() && s.todos[2].pending.is_empty());
        // b's start isn't theirs: it waits for a session of its own.
        assert_eq!((s.todos[1].sessions.len(), s.todos[1].pending.len()), (0, 1));
        assert!(s.resolve_pending(&board, &lanes(), at + 12_000));
        assert_eq!(s.todos[1].sessions, vec!["claude:claude:second".to_string()]);
    }

    #[test]
    fn a_start_prefers_its_own_worktree_and_gives_up_after_half_an_hour() {
        let mut s = store(&["a"]);
        let at = 5_000_000;
        s.started("a", Some("/r/app3"), None, Some(pending("/r/app3", "claude", "claude", at)), &lanes(), at);
        let board = [session("claude:claude:elsewhere", "/r/app", "claude", "claude", at + 1_000), session("claude:claude:here", "/r/app3", "claude", "claude", at + 9_000)];
        assert!(s.resolve_pending(&board, &lanes(), at + 10_000));
        assert_eq!(s.todos[0].sessions, vec!["claude:claude:here".to_string()]);
        s.started("a", Some("/r/app3"), None, Some(pending("/r/app3", "claude", "claude", at)), &lanes(), at);
        assert!(s.resolve_pending(&[], &lanes(), at + PENDING_MS + 1));
        assert!(s.todos[0].pending.is_empty());
    }

    #[test]
    fn unlinking_the_worktree_drops_a_start_waiting_there() {
        let mut s = store(&["a"]);
        s.started("a", Some("/r/app"), None, Some(pending("/r/app", "codex", "codex", 1)), &lanes(), 1);
        s.apply(TodoOp::Link { id: "a".into(), worktree: Some("/r/app".into()), session: None, linked: false }, &lanes(), 2).unwrap();
        assert!(s.todos[0].pending.is_empty() && s.todos[0].worktrees.is_empty());
    }

    #[test]
    fn a_list_written_by_hand_keeps_every_todo() {
        let hand: TodoStore = serde_json::from_str(
            r#"{"todos":[{"id":"fix login","title":"A","labels":["my label"]},{"title":"No id","status":"in_progress"},{"id":"fix-login","title":"Clash","status":"completed"}],
                "labels":[{"id":"my label","name":"Mine","color":"teal"},{"name":"No id either"}]}"#,
        )
        .unwrap();
        let s = repaired(hand);
        let ids: Vec<&str> = s.todos.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, vec!["fix-login", "t-2", "fix-login_"]);
        assert_eq!((s.todos[1].status, s.todos[2].status), (TodoStatus::InProgress, TodoStatus::Done));
        assert_eq!(s.labels.iter().map(|l| l.id.as_str()).collect::<Vec<_>>(), vec!["my-label", "l-2"]);
        // The todo keeps its label under the label's new id.
        assert_eq!(s.todos[0].labels, vec!["my-label".to_string()]);
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
