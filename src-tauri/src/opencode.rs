//! OpenCode (2.x): its SQLite database (`<data dir>/opencode.db`, WAL mode) holds every
//! session (`session_v2`) and message (`session_message`), and its background service
//! (`~/.local/state/opencode/service.json`) says which sessions are running right now and what
//! they're waiting on. Permission and question requests never reach the database, so they come
//! from the service alone. See .claude/rules/opencode.md.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rusqlite::Connection;
use serde_json::Value;

use crate::config::AccountCfg;
use crate::harness::{Applied, Look, Said, Wake};
use crate::model::{Harness, ModeKind, Session, SessionMode};
use crate::procs;
use crate::transcript::{clip_input, clip_output, item, kept_whole, Item, Page};
use crate::util::{code, home, one_line, truncate};

/// A session as the database has it, with the latest message each way.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SessionRow {
    pub id: String,
    pub directory: String,
    pub title: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    /// When the last run ended, once one has. A run in progress leaves it where it was.
    pub idle_at: Option<i64>,
    /// `succeeded`, `failed` or `interrupted`, recorded with `idle_at`.
    pub outcome: Option<String>,
    pub archived_at: Option<i64>,
    /// When you last looked at it in OpenCode.
    pub viewed_at: Option<i64>,
    pub last_prompt: Option<String>,
    pub last_prompt_at: Option<i64>,
    /// The latest assistant text, first line.
    pub last_text: Option<String>,
    /// The end of the latest assistant text, for Jev.
    pub last_text_tail: Option<String>,
    /// The tool the latest assistant message is on, e.g. "bash · `npm test`".
    pub last_tool: Option<String>,
    /// The tokens in its context after the latest assistant message, and that message's model.
    pub context: Option<(u64, Option<String>)>,
    /// The newest write to the session or its messages.
    pub last_ts: i64,
    /// The session it's a fork of (`fork_session_id`), which OpenCode's fork copies it from.
    pub fork_of: Option<String>,
    /// The agent its next turn runs as (`agent`), else the one its latest reply ran as: `build`,
    /// `plan` or one of yours.
    pub agent: Option<String>,
    /// The model its next turn runs on (`model`), else the one its latest reply came from, by
    /// its id within its provider (`big-pickle`).
    pub model: Option<String>,
    /// The reasoning variant `model` was picked with (`high`), else the one its latest reply
    /// was made at when that reply was on it (`model_and_effort`).
    pub effort: Option<String>,
}

impl SessionRow {
    /// A prompt was sent after the last run ended.
    pub fn turn_open(&self) -> bool {
        match (self.last_prompt_at, self.idle_at) {
            (Some(p), Some(i)) => p > i,
            (Some(_), None) => true,
            (None, _) => false,
        }
    }
}

/// Sessions updated since `since_ms`, plus any in `keep` (pinned ones), however old. Subagent
/// sessions (ones with a parent) are left out.
pub fn read_sessions(home: &Path, since_ms: i64, keep: &HashSet<String>) -> Vec<SessionRow> {
    let Some(db) = open(&home.join("opencode.db")) else { return Vec::new() };
    read_sessions_from(&db, since_ms, keep).unwrap_or_default()
}

fn open(path: &Path) -> Option<Connection> {
    crate::harness::open_database(path)
}

/// The columns of `session_v2` a `SessionRow` is read from, in `session_row`'s order.
const ROW: &str = "id, directory, title, time_created, time_updated, time_idle, idle_outcome, time_archived, time_viewed";

/// A row of `session_v2` (`ROW`), before its messages are read.
fn session_row(r: &rusqlite::Row) -> rusqlite::Result<SessionRow> {
    Ok(SessionRow {
        id: r.get(0)?,
        directory: r.get(1)?,
        title: r.get::<_, Option<String>>(2)?.map(|t| t.trim().to_string()).filter(|t| !t.is_empty()),
        created_at: r.get(3)?,
        updated_at: r.get(4)?,
        idle_at: r.get(5)?,
        outcome: r.get(6)?,
        archived_at: r.get(7)?,
        viewed_at: r.get(8)?,
        ..Default::default()
    })
}

fn read_sessions_from(db: &Connection, since_ms: i64, keep: &HashSet<String>) -> rusqlite::Result<Vec<SessionRow>> {
    let mut by_id: HashMap<String, SessionRow> = HashMap::new();
    let mut stmt = db.prepare(&format!("SELECT {ROW} FROM session_v2 WHERE parent_id IS NULL AND (time_updated >= ?1 OR time_created >= ?1)"))?;
    let rows = stmt.query_map([since_ms], session_row)?;
    for row in rows.flatten() {
        by_id.insert(row.id.clone(), row);
    }
    if !keep.is_empty() {
        let mut stmt = db.prepare(&format!("SELECT {ROW} FROM session_v2 WHERE id = ?1"))?;
        for id in keep {
            if by_id.contains_key(id) {
                continue;
            }
            if let Ok(row) = stmt.query_row([id], session_row) {
                by_id.insert(row.id.clone(), row);
            }
        }
    }
    for row in by_id.values_mut() {
        row.last_ts = row.updated_at.max(row.created_at);
    }
    // Asked apart: databases from before forks have no such column, and still read.
    if let Ok(mut stmt) = db.prepare("SELECT id, fork_session_id FROM session_v2 WHERE fork_session_id IS NOT NULL AND fork_session_id != ''") {
        if let Ok(rows) = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))) {
            for (id, from) in rows.flatten() {
                if let Some(row) = by_id.get_mut(&id) {
                    row.fork_of = Some(from);
                }
            }
        }
    }
    // The agent picked for its next turn; NULL is the config's `default_agent`, which its replies
    // name. Asked apart, as forks are: older databases have no such column.
    if let Ok(mut stmt) = db.prepare("SELECT id, agent FROM session_v2 WHERE agent IS NOT NULL AND agent != ''") {
        if let Ok(rows) = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))) {
            for (id, agent) in rows.flatten() {
                if let Some(row) = by_id.get_mut(&id) {
                    row.agent = Some(agent);
                }
            }
        }
    }
    // The model picked for its next turn, kept beside the agent and asked apart for the same
    // reason.
    let mut picked: HashMap<String, Value> = HashMap::new();
    if let Ok(mut stmt) = db.prepare("SELECT id, model FROM session_v2 WHERE model IS NOT NULL AND model != ''") {
        if let Ok(rows) = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))) {
            for (id, model) in rows.flatten() {
                if let (Some(row), Ok(model)) = (by_id.get_mut(&id), serde_json::from_str::<Value>(&model)) {
                    (row.model, row.effort) = model_and_effort(Some(&model), None);
                    picked.insert(id, model);
                }
            }
        }
    }
    // The newest user and assistant message of each session; `seq` counts up within a session.
    let mut stmt = db.prepare(
        "SELECT m.session_id, m.type, m.time_created, m.time_updated, m.data FROM session_message m \
         JOIN (SELECT session_id, type, max(seq) AS seq FROM session_message WHERE type IN ('user', 'assistant') GROUP BY session_id, type) l \
         ON l.session_id = m.session_id AND l.type = m.type AND l.seq = m.seq",
    )?;
    let messages = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?, r.get::<_, i64>(3)?, r.get::<_, String>(4)?)))?;
    for (session_id, kind, created, updated, data) in messages.flatten() {
        let Some(row) = by_id.get_mut(&session_id) else { continue };
        let v: Value = serde_json::from_str(&data).unwrap_or(Value::Null);
        row.last_ts = row.last_ts.max(updated).max(created);
        match kind.as_str() {
            "user" => {
                row.last_prompt_at = Some(created);
                row.last_prompt = v.get("text").and_then(Value::as_str).map(|t| one_line(crate::send::past_sender_lines(t), 140)).filter(|t| !t.is_empty());
            }
            "assistant" => {
                let (text, tool) = assistant_summary(&v);
                row.last_text = text;
                row.last_text_tail = last_text_part(&v).map(|t| crate::jev::tail(t, 600)).filter(|t| !t.is_empty());
                row.last_tool = tool;
                if row.agent.is_none() {
                    row.agent = v.get("agent").and_then(Value::as_str).filter(|a| !a.is_empty()).map(str::to_string);
                }
                (row.model, row.effort) = model_and_effort(picked.get(&session_id), v.get("model"));
            }
            _ => {}
        }
    }
    for row in by_id.values_mut() {
        row.context = context_of(db, &row.id);
    }
    let mut out: Vec<SessionRow> = by_id.into_values().collect();
    out.sort_by(|a, b| b.last_ts.cmp(&a.last_ts));
    Ok(out)
}

/// A session's model and effort, from the model picked for its next turn (`session_v2.model`)
/// and its latest reply's, each as OpenCode refers to a model (`Model.Ref`): `{id, providerID,
/// variant}`, where a variant left out or `default` is none picked, and one of the effort levels
/// is that effort (2.0.16's code). A prompt carries no model, so its turn runs on the picked
/// one. The reply's variant counts only when it was on that model, provider and all.
fn model_and_effort(picked: Option<&Value>, reply: Option<&Value>) -> (Option<String>, Option<String>) {
    let named = |m: &&Value| m.get("id").and_then(Value::as_str).is_some_and(|id| !id.is_empty());
    let reply = reply.filter(named);
    let Some(model) = picked.filter(named).or(reply) else { return (None, None) };
    let same = |r: &&Value| ["id", "providerID"].iter().all(|k| r.get(k) == model.get(k));
    let variant = model.get("variant").or_else(|| reply.filter(same).and_then(|r| r.get("variant")));
    let effort = variant.and_then(Value::as_str).filter(|e| !e.is_empty() && *e != "default").map(str::to_string);
    (model.get("id").and_then(Value::as_str).map(str::to_string), effort)
}

/// A session's agent as its mode: OpenCode names its primary agents its modes (its ACP server
/// lists them as the session's modes), Tab switches between them, and each has permissions of
/// its own. In 2.0.16 every agent starts from allowing everything but folders outside the
/// project and `.env` files, `plan` denies edits outside its plan folder, and config files can
/// change any of it (`permission`), so only `plan` is said to change nothing. `--auto` in a
/// terminal answers every request there, which only its arguments say.
pub fn agent_mode(agent: &str) -> SessionMode {
    let name = agent.trim();
    match name {
        "plan" => SessionMode::new(ModeKind::Plan, "Plan", "OpenCode's plan agent: it plans without editing files, and runs commands as its permission settings allow."),
        "build" => SessionMode::new(ModeKind::Other, "Build", "OpenCode's build agent: it does what its permission settings allow (by default everything in the project) and asks about the rest."),
        _ => {
            let mut label: String = name.chars().take(1).flat_map(char::to_uppercase).collect();
            label.push_str(&name.chars().skip(1).collect::<String>());
            SessionMode::new(ModeKind::Other, &label, &format!("OpenCode's {name} agent: it does what its permission settings allow and asks about the rest."))
        }
    }
}

/// How full a session's context is, as OpenCode's own meter has it: every token of its latest
/// assistant message that has them, after its latest completed compaction (one running or failed
/// leaves the context as it was: its `status`, 2.0.16's code). A reply has none until its step
/// ends, so one that's streaming, or failed before it did, leaves the one before it. Looked up by
/// the table's index on session, type and order, a few messages at a time.
fn context_of(db: &Connection, session: &str) -> Option<(u64, Option<String>)> {
    let compacted: i64 = db.query_row("SELECT coalesce(max(seq), -1) FROM session_message WHERE session_id = ?1 AND type = 'compaction' AND json_extract(data, '$.status') = 'completed'", [session], |r| r.get(0)).unwrap_or(-1);
    let mut stmt = db.prepare_cached("SELECT data FROM session_message WHERE session_id = ?1 AND type = 'assistant' AND seq > ?2 ORDER BY seq DESC LIMIT 8").ok()?;
    let rows = stmt.query_map(rusqlite::params![session, compacted], |r| r.get::<_, String>(0)).ok()?;
    for data in rows.flatten() {
        let Ok(v) = serde_json::from_str::<Value>(&data) else { continue };
        let Some(t) = v.get("tokens") else { continue };
        let n = |v: &Value, k: &str| v.get(k).and_then(Value::as_u64).unwrap_or(0);
        let cache = t.get("cache").cloned().unwrap_or(Value::Null);
        let used = n(t, "input") + n(t, "output") + n(t, "reasoning") + n(&cache, "read") + n(&cache, "write");
        if used > 0 {
            return Some((used, v.pointer("/model/id").and_then(Value::as_str).map(str::to_string)));
        }
    }
    None
}

/// The text of messages written since `after` (by `time_updated`, since a reply streams into
/// its row) in top-level sessions updated since `since_ms`.
pub fn read_prose(home: &Path, since_ms: i64, after: i64) -> Vec<Said> {
    let Some(db) = open(&home.join("opencode.db")) else { return Vec::new() };
    let read = || -> rusqlite::Result<Vec<Said>> {
        let mut stmt = db.prepare(
            "SELECT m.id, m.session_id, m.type, m.time_created, m.time_updated, m.data FROM session_message m \
             JOIN session_v2 s ON s.id = m.session_id \
             WHERE s.parent_id IS NULL AND s.time_updated >= ?1 AND m.time_updated > ?2 AND m.type IN ('user', 'assistant')",
        )?;
        let rows = stmt.query_map([since_ms, after], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, i64>(3)?, r.get::<_, i64>(4)?, r.get::<_, String>(5)?))
        })?;
        Ok(rows
            .flatten()
            .filter_map(|(id, session, kind, at, updated, data)| {
                let v: Value = serde_json::from_str(&data).ok()?;
                let you = kind == "user";
                let text = if you {
                    v.get("text").and_then(Value::as_str)?.to_string()
                } else {
                    let parts = v.get("content")?.as_array()?;
                    parts.iter().filter(|p| p.get("type").and_then(Value::as_str) == Some("text")).filter_map(|p| p.get("text").and_then(Value::as_str)).collect::<Vec<_>>().join("\n")
                };
                let text = text.trim().to_string();
                (!text.is_empty()).then_some(Said { id, session, at, updated, you, text })
            })
            .collect())
    };
    read().unwrap_or_default()
}

/// What assistant messages written since `after` used, for the token ledger, and when the
/// newest was written. A message's `data` has `tokens: {input, output, reasoning, cache: {read,
/// write}}` (input without the cache's part, output without the thinking's), `model: {id,
/// providerID}` and `cost`, priced by OpenCode from models.dev as it's written: what a provider
/// you pay charged, and 0 for a free model or a subscription's sign-in. A subagent's session
/// counts for the session that started it. A fork's copies of the messages before it have IDs of
/// their own and keep their first times (2.0.16, 2026-10-12), so they were started before the
/// fork was made: while the session it copied is there, they're that session's calls and are left
/// out; once it's gone, the fork's copies are the record of them that's left.
pub fn token_uses(home: &Path, account: &str, after: i64) -> Result<(Vec<crate::ledger::TokenUse>, i64), String> {
    let path = home.join("opencode.db");
    if !path.is_file() {
        return Ok((Vec::new(), after));
    }
    let Some(db) = open(&path) else { return Err(format!("{} couldn't be opened", path.display())) };
    token_uses_in(&db, account, after).map_err(|e| format!("{}: {e}", path.display()))
}

/// How long before the newest write the last read saw a message may have been started and still
/// be written to, its reply streaming into its row: far longer than any reply takes. Messages are
/// looked for by when they were started, which the table has an index on; when they were last
/// written has none, and every minute's look would read the whole table.
const STILL_WRITING: i64 = 7 * 24 * 3600 * 1000;

fn token_uses_in(db: &Connection, account: &str, after: i64) -> rusqlite::Result<(Vec<crate::ledger::TokenUse>, i64)> {
    // Each fork whose session is still there, by when it was made. Databases from before 2.0.16
    // have no forks, nor the column.
    let mut forks: HashMap<String, i64> = HashMap::new();
    if let Ok(mut stmt) = db.prepare("SELECT f.id, f.time_created FROM session_v2 f JOIN session_v2 o ON o.id = f.fork_session_id") {
        forks.extend(stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?.flatten());
    }
    let mut stmt = db.prepare(
        "SELECT m.id, COALESCE(s.parent_id, s.id), m.time_created, m.time_updated, m.data, s.directory, s.id FROM session_message m \
         JOIN session_v2 s ON s.id = m.session_id WHERE m.type = 'assistant' AND m.time_created > ?1 AND m.time_updated > ?2",
    )?;
    let started_after = if after > 0 { after - STILL_WRITING } else { 0 };
    let rows = stmt.query_map([started_after, after], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?, r.get::<_, i64>(3)?, r.get::<_, String>(4)?, r.get::<_, String>(5)?, r.get::<_, String>(6)?))
    })?;
    let mut newest = after;
    let mut out = Vec::new();
    for row in rows {
        let (id, session, at, updated, data, folder, own) = row?;
        newest = newest.max(updated);
        if forks.get(&own).is_some_and(|made| at < *made) {
            continue;
        }
        let Ok(v) = serde_json::from_str::<Value>(&data) else { continue };
        let Some(t) = v.get("tokens") else { continue };
        let n = |v: &Value, k: &str| v.get(k).and_then(Value::as_u64).unwrap_or(0);
        let reasoning = n(t, "reasoning");
        let cache = t.get("cache").cloned().unwrap_or(Value::Null);
        out.push(crate::ledger::TokenUse {
            id,
            at,
            session: Session::key_for(Harness::Opencode, account, &session),
            folder,
            model: v.pointer("/model/id").and_then(Value::as_str).unwrap_or("").to_string(),
            input: n(t, "input"),
            cache_read: n(&cache, "read"),
            cache_write: n(&cache, "write"),
            cache_write_1h: 0,
            output: n(t, "output") + reasoning,
            reasoning,
            fast: false,
            searches: 0,
            cost: v.get("cost").and_then(Value::as_f64),
        });
    }
    Ok((out, newest))
}

/// A session's messages before `before` (by `seq`; from the newest when none), up to about
/// `limit` items, for its panel (transcript.rs). `items` turns a message's `data` into items.
pub fn transcript_page<T>(db: &Path, session: &str, before: Option<u64>, limit: usize, items: impl Fn(&Value, u64, i64) -> Vec<T>) -> crate::transcript::PageOf<T> {
    let Some(db) = open(db) else { return crate::transcript::PageOf { items: Vec::new(), before: None, after: 0 } };
    let newest: i64 = db.query_row("SELECT COALESCE(MAX(seq), 0) FROM session_message WHERE session_id = ?1", [session], |r| r.get(0)).unwrap_or(0);
    // Asked before the messages are read: a reply that has ended by now is read whole below, and
    // one still going is read again from its own place.
    let after = db
        .query_row("SELECT seq, data FROM session_message WHERE session_id = ?1 AND type = 'assistant' AND seq <= ?2 ORDER BY seq DESC LIMIT 1", rusqlite::params![session, newest], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
        .ok()
        .filter(|(_, data)| streaming(&serde_json::from_str(data).unwrap_or(Value::Null)))
        .map_or(newest, |(seq, _)| (seq - 1).max(0));
    let mut out: Vec<Vec<T>> = Vec::new();
    let mut count = 0;
    let mut first_seq: Option<i64> = None;
    let read = |from: i64, out: &mut Vec<Vec<T>>, count: &mut usize, first: &mut Option<i64>| -> rusqlite::Result<bool> {
        let mut stmt = db.prepare("SELECT seq, time_created, data FROM session_message WHERE session_id = ?1 AND seq < ?2 AND type IN ('user', 'assistant') ORDER BY seq DESC LIMIT 50")?;
        let rows: Vec<(i64, i64, String)> = stmt.query_map(rusqlite::params![session, from], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.flatten().collect();
        let more = rows.len() == 50;
        for (seq, at, data) in rows {
            let v: Value = serde_json::from_str(&data).unwrap_or(Value::Null);
            let its = items(&v, seq as u64, at);
            *count += its.len();
            out.push(its);
            *first = Some(seq);
            if *count >= limit {
                return Ok(true);
            }
        }
        Ok(!more)
    };
    let mut from = before.map(|b| b as i64).unwrap_or(newest + 1);
    loop {
        match read(from, &mut out, &mut count, &mut first_seq) {
            Ok(true) | Err(_) => break,
            Ok(false) => from = first_seq.unwrap_or(0),
        }
    }
    let oldest = first_seq.unwrap_or(0);
    let has_older: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM session_message WHERE session_id = ?1 AND seq < ?2)", rusqlite::params![session, oldest], |r| r.get(0)).unwrap_or(false);
    crate::transcript::PageOf { items: out.into_iter().rev().flatten().collect(), before: (first_seq.is_some() && has_older).then_some(oldest as u64), after: after as u64 }
}

/// Whether an assistant message is still being written. OpenCode writes a reply into its row as
/// it streams, the same row each time, and stamps `time.completed` when it ends.
fn streaming(message: &Value) -> bool {
    message.pointer("/time/completed").map_or(true, Value::is_null)
}

/// A session's messages after `after` (by `seq`), oldest first. The `after` it hands back stops
/// short of a reply still streaming, so the next read has that message again as it grows, and
/// the panel replaces it by its place (its cursor).
pub fn transcript_newer<T>(db: &Path, session: &str, after: u64, items: impl Fn(&Value, u64, i64) -> Vec<T>) -> crate::transcript::PageOf<T> {
    let Some(db) = open(db) else { return crate::transcript::PageOf { items: Vec::new(), before: None, after } };
    let read = || -> rusqlite::Result<(Vec<T>, u64)> {
        let mut stmt = db.prepare("SELECT seq, time_created, data, type FROM session_message WHERE session_id = ?1 AND seq > ?2 AND type IN ('user', 'assistant') ORDER BY seq")?;
        let mut last = after;
        // The newest reply read, and whether it's still going: only the newest can be, as a
        // session runs one turn at a time, though a prompt queued meanwhile can come after it.
        let mut reply: Option<(u64, bool)> = None;
        let mut out = Vec::new();
        for (seq, at, data, kind) in stmt.query_map(rusqlite::params![session, after as i64], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?)))?.flatten() {
            let v: Value = serde_json::from_str(&data).unwrap_or(Value::Null);
            if kind == "assistant" {
                reply = Some((seq as u64, streaming(&v)));
            }
            out.extend(items(&v, seq as u64, at));
            last = seq as u64;
        }
        match reply {
            Some((seq, true)) => Ok((out, seq.saturating_sub(1))),
            _ => Ok((out, last)),
        }
    };
    match read() {
        Ok((items, last)) => crate::transcript::PageOf { items, before: None, after: last },
        Err(_) => crate::transcript::PageOf { items: Vec::new(), before: None, after },
    }
}

/// The latest text and the latest tool call in an assistant message's content.
/// The latest text part of an assistant message, whole.
fn last_text_part(message: &Value) -> Option<&str> {
    message
        .get("content")?
        .as_array()?
        .iter()
        .rev()
        .filter(|p| p.get("type").and_then(Value::as_str) == Some("text"))
        .find_map(|p| p.get("text").and_then(Value::as_str))
}

fn assistant_summary(message: &Value) -> (Option<String>, Option<String>) {
    let parts = message.get("content").and_then(Value::as_array).cloned().unwrap_or_default();
    let text = parts
        .iter()
        .rev()
        .filter(|p| p.get("type").and_then(Value::as_str) == Some("text"))
        .find_map(|p| p.get("text").and_then(Value::as_str))
        .map(|t| one_line(t, 140))
        .filter(|t| !t.is_empty());
    let tool = parts.iter().rev().find(|p| p.get("type").and_then(Value::as_str) == Some("tool")).and_then(tool_summary);
    (text, tool)
}

/// The inputs of a session's tool calls that haven't finished, as JSON: the tool parts of its
/// latest assistant messages that aren't completed or failed yet. The service writes a part as
/// the call starts, before it runs.
fn open_calls(db: &Path, session: &str) -> Vec<String> {
    open(db).map(|db| open_calls_in(&db, session)).unwrap_or_default()
}

fn open_calls_in(db: &Connection, session: &str) -> Vec<String> {
    let read = || -> rusqlite::Result<Vec<String>> {
        let mut stmt = db.prepare("SELECT data FROM session_message WHERE session_id = ?1 AND type = 'assistant' ORDER BY seq DESC LIMIT 3")?;
        let rows = stmt.query_map([session], |r| r.get::<_, String>(0))?.flatten().collect();
        Ok(rows)
    };
    read()
        .unwrap_or_default()
        .iter()
        .filter_map(|data| serde_json::from_str::<Value>(data).ok())
        .flat_map(|v| v.get("content").and_then(Value::as_array).cloned().unwrap_or_default())
        .filter(|p| p.get("type").and_then(Value::as_str) == Some("tool") && !matches!(p.pointer("/state/status").and_then(Value::as_str), Some("completed" | "error")))
        .filter_map(|p| p.pointer("/state/input").map(Value::to_string))
        .collect()
}

/// "bash · `npm test`": the tool's name and the argument that says what it's doing.
fn tool_summary(part: &Value) -> Option<String> {
    let name = part.get("name").and_then(Value::as_str)?;
    let input = part.pointer("/state/input");
    let arg = input.and_then(|i| {
        ["command", "filePath", "path", "pattern", "url", "description"].iter().find_map(|k| i.get(*k).and_then(Value::as_str))
    });
    Some(match arg {
        Some(a) => format!("{name} · {}", code(&truncate(&one_line(a, 200), 80))),
        None => name.to_string(),
    })
}

/// Whether the account can start sessions, and the email of the OpenCode account it's signed in
/// to, if any. Once OpenCode has run here (its database is there) it can, since its free models
/// (`opencode/…`) need no sign-in; an older OpenCode's sign-ins are a legacy `auth.json`.
pub fn signed_in(home: &Path) -> (bool, Option<String>) {
    let db = open(&home.join("opencode.db"));
    let email = db.as_ref().and_then(|db| db.query_row("SELECT email FROM account ORDER BY time_updated DESC LIMIT 1", [], |r| r.get::<_, String>(0)).ok());
    (db.is_some() || home.join("auth.json").is_file(), email)
}

/// What Settings says of the sign-in (`Adapter::sign_in_note`), from its database.
pub fn sign_in_note(home: &Path) -> Option<String> {
    let db = open(&home.join("opencode.db"))?;
    sign_in_note_in(&db, home.join("auth.json").is_file())
}

/// The providers it's signed in to, else, with no OpenCode account either, that it runs only
/// OpenCode's free models. None leaves it to the email. An older OpenCode, whose database has
/// no credentials, kept its sign-ins in `auth.json` (`legacy`); OpenCode 2.0 moved them into
/// the database and left the file, so once the database has them the file says nothing.
fn sign_in_note_in(db: &Connection, legacy: bool) -> Option<String> {
    let providers = match providers(db) {
        Some(p) => p,
        None if legacy => return None,
        None => Vec::new(),
    };
    if !providers.is_empty() {
        return Some(format!("Signed in to {}", crate::util::listed(&providers, "and")));
    }
    let accounts = db.query_row("SELECT count(*) FROM account", [], |r| r.get::<_, i64>(0)).unwrap_or(0);
    (accounts == 0).then(|| "Free models only".to_string())
}

/// The providers it has a credential for, the first signed in to first, each by the name in
/// the models.dev catalog OpenCode keeps (`kv`'s `models-dev:catalog`, whose `body` is the
/// catalog's JSON as text), else by its id; None for a database with no credentials (before
/// 2.0). A credential's `integration_id` is the provider's id (`anthropic`, `github-copilot`,
/// `opencode` for OpenCode Zen), or a well-known source's URL; 2.0's migration moved
/// `auth.json`'s sign-ins there under the same keys.
fn providers(db: &Connection) -> Option<Vec<String>> {
    let mut q = db.prepare("SELECT integration_id FROM credential GROUP BY integration_id ORDER BY min(time_created)").ok()?;
    let ids: Vec<String> = q.query_map([], |r| r.get(0)).map(|rows| rows.flatten().collect()).unwrap_or_default();
    // A quote would end the path, so such an id keeps its own name.
    let name = |id: &str| -> Option<String> {
        if id.contains('"') {
            return None;
        }
        db.query_row("SELECT json_extract(json_extract(value, '$.body'), ?1) FROM kv WHERE key = 'models-dev:catalog'", [format!("$.\"{id}\".name")], |r| r.get(0)).ok().flatten()
    };
    Some(ids.into_iter().map(|id| name(&id).unwrap_or(id)).collect())
}

// ---- The background service ----

/// The running service, from the state file OpenCode writes when it starts one.
#[derive(Debug, Clone, PartialEq)]
pub struct Service {
    pub url: String,
    pub pid: u32,
    password: String,
}

/// The default data folder, the one the service in `~/.local/state/opencode` serves.
pub fn default_home() -> PathBuf {
    home().join(".local/share/opencode")
}

/// The service for an account, if it's running. Only the default data folder has one that
/// Codealong can find: OpenCode keeps the state file under `~/.local/state`, whatever the data
/// folder.
pub fn service(account_home: &Path) -> Option<Service> {
    if account_home != default_home() {
        return None;
    }
    service_at(&home().join(".local/state/opencode/service.json"))
}

pub fn service_at(path: &Path) -> Option<Service> {
    let text = std::fs::read_to_string(path).ok()?;
    let v: Value = serde_json::from_str(&text).ok()?;
    let pid = v.get("pid").and_then(Value::as_u64)? as u32;
    if !procs::pid_alive(pid) {
        return None;
    }
    Some(Service {
        url: v.get("url").and_then(Value::as_str)?.trim_end_matches('/').to_string(),
        pid,
        password: v.get("password").and_then(Value::as_str)?.to_string(),
    })
}

/// What the service says right now: the sessions with a run in progress, and for each of
/// those, what it's waiting on you for, if anything.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Live {
    pub active: HashSet<String>,
    /// Session id → the permission or question it's waiting on.
    pub pending: HashMap<String, Asked>,
    pub at: i64,
}

/// A permission or question a session waits on you for.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Asked {
    /// The request's own id (`per_…`), or its line where the service gave none.
    pub id: String,
    /// The line that says it: "Run · `npm test`".
    pub ask: String,
    /// When a poll first saw it, kept while the session waits on the same request: the time the
    /// session waits on you from, which nothing else in its records keeps (a rename moves the
    /// session's own time, say).
    pub since: i64,
}

/// A request not seen in a poll for this long is forgotten: seen again, it's dated afresh.
const FORGET_ASKED_MS: i64 = 10 * 60_000;

/// When each request was first and last seen, by session and request id, kept through polls
/// that failed or missed it, so a request seen again keeps the time it was first seen.
pub type Seen = HashMap<(String, String), (i64, i64)>;

impl Live {
    /// This poll's answer, each request with the time it was first seen, which `seen` keeps.
    pub fn dated(mut self, seen: &mut Seen) -> Live {
        seen.retain(|_, (_, last)| self.at - *last < FORGET_ASKED_MS);
        for (session, asked) in self.pending.iter_mut() {
            let times = seen.entry((session.clone(), asked.id.clone())).or_insert((asked.since, self.at));
            times.1 = self.at;
            asked.since = times.0;
        }
        self
    }
}

/// Asks the service what's running and what's waiting. Three short local requests at most:
/// the active list, and for each active session its permission and form lists.
pub fn poll(svc: &Service, now: i64) -> Option<Live> {
    let active = get(svc, "/api/session/active")?;
    let mut live = Live { at: now, ..Default::default() };
    if let Some(map) = active.get("data").and_then(Value::as_object) {
        live.active = map.keys().cloned().collect();
    }
    let asked = |list: &Value, ask: String| Asked { id: first_id(list).unwrap_or_else(|| ask.clone()), ask, since: now };
    for id in &live.active {
        if let Some((list, ask)) = get(svc, &format!("/api/session/{id}/permission")).and_then(|v| permission_ask(&v).map(|a| (v, a))) {
            live.pending.insert(id.clone(), asked(&list, ask));
            continue;
        }
        if let Some((list, ask)) = get(svc, &format!("/api/session/{id}/form")).and_then(|v| form_ask(&v).map(|a| (v, a))) {
            live.pending.insert(id.clone(), asked(&list, ask));
        }
    }
    Some(live)
}

/// The id of the first request in a list the service gave.
fn first_id(list: &Value) -> Option<String> {
    list.get("data")?.as_array()?.first()?.get("id")?.as_str().map(String::from)
}

/// The first pending permission request, as a line: "Run · `npm test`".
pub fn permission_ask(list: &Value) -> Option<String> {
    let req = list.get("data").and_then(Value::as_array)?.first()?;
    let action = req.get("action").and_then(Value::as_str).unwrap_or("Permission");
    let resource = req.get("resources").and_then(Value::as_array).and_then(|r| r.first()).and_then(Value::as_str);
    let message = req.get("message").and_then(Value::as_str).filter(|m| !m.trim().is_empty());
    Some(match (message, resource) {
        (Some(m), _) => one_line(m, 140),
        (None, Some(r)) => format!("{} · {}", title_case(action), code(&truncate(&one_line(r, 200), 80))),
        (None, None) => format!("{} requested", title_case(action)),
    })
}

/// The first pending question (OpenCode calls them forms), by its title.
pub fn form_ask(list: &Value) -> Option<String> {
    let form = list.get("data").and_then(Value::as_array)?.first()?;
    let title = form.get("title").and_then(Value::as_str).filter(|t| !t.trim().is_empty());
    Some(title.map(|t| one_line(t, 140)).unwrap_or_else(|| "Question waiting".to_string()))
}

fn title_case(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(first) => first.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// A GET on the service. The password goes to curl on stdin, never on the command line.
fn get(svc: &Service, path: &str) -> Option<Value> {
    let url = format!("{}{path}", svc.url);
    let out = procs::run_stdin("/usr/bin/curl", &["-sS", "-m", "5", "-K", "-", "-H", "Accept: application/json", &url], &sign_in(svc), Duration::from_secs(8)).ok()?;
    serde_json::from_str(&out).ok()
}

/// The line of a curl config that signs in to the service, the same for every request.
fn sign_in(svc: &Service) -> String {
    format!("user = \"opencode:{}\"\n", curl_quote(&svc.password))
}

/// A POST of JSON on the service. The password and the body both go to curl on stdin: the body
/// is your prompt, which has no business in a process list either.
pub fn post(svc: &Service, path: &str, body: &Value) -> anyhow::Result<Value> {
    send(svc, "POST", path, body)
}

/// A PATCH of JSON on the service, as `post` sends it.
fn patch(svc: &Service, path: &str, body: &Value) -> anyhow::Result<Value> {
    send(svc, "PATCH", path, body)
}

fn send(svc: &Service, method: &str, path: &str, body: &Value) -> anyhow::Result<Value> {
    let config = format!("{}data-binary = \"{}\"\n", sign_in(svc), curl_quote(&body.to_string()));
    let url = format!("{}{path}", svc.url);
    let out = procs::run_stdin(
        "/usr/bin/curl",
        &["-sS", "--fail-with-body", "-m", "10", "-K", "-", "-X", method, "-H", "Content-Type: application/json", "-H", "Accept: application/json", &url],
        &config,
        Duration::from_secs(15),
    )?;
    Ok(serde_json::from_str(&out).unwrap_or(Value::Null))
}

/// The way in for every session: the prompt posted to the service, which queues it for the
/// session; any terminal on the session shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct Posted;

impl crate::send::Own for Posted {
    fn way(&self) -> crate::send::Way {
        crate::send::Way::Opencode
    }

    fn takes_agents(&self) -> bool {
        true
    }

    fn deliver(&self, d: &crate::send::Delivery) -> anyhow::Result<crate::send::Sent> {
        let title = d.title();
        let svc = service(&d.account.home_path()).ok_or_else(|| anyhow::anyhow!("OpenCode's service isn't running, so Codealong can't reach {title}. Open it and send it there."))?;
        post(&svc, &format!("/api/session/{}/prompt", urlencoding::encode(&d.session.id)), &prompt_body(d.text, d.now))?;
        Ok(crate::send::sent(if d.now {
            format!("Sent now to {title} in OpenCode: it takes it into the turn it's taking.")
        } else if d.busy() {
            format!("Queued for {title} in OpenCode; it takes it after its current step.")
        } else {
            format!("Sent to {title} in OpenCode.")
        }))
    }
}

/// What the service is posted for a prompt: queued, so a running turn takes it after its step,
/// or, sent `now`, steered into the turn at once. Kept images on disk go in `files` as data URLs,
/// taken out of the text; other attachments stay in the text as paths.
fn prompt_body_in(text: &str, now: bool, images: &Path, files: &Path) -> Value {
    let (prose, read) = crate::attachments::read_images_in(text, images, files, crate::attachments::Style::Path);
    let mut body = serde_json::json!({ "text": prose, "delivery": if now { "steer" } else { "queue" } });
    if !read.is_empty() {
        body["files"] = Value::Array(
            read.into_iter()
                .map(|(name, mime, data)| serde_json::json!({ "type": "file", "url": format!("data:{mime};base64,{data}"), "filename": name, "mime": mime }))
                .collect(),
        );
    }
    body
}

fn prompt_body(text: &str, now: bool) -> Value {
    prompt_body_in(text, now, &crate::attachments::images_dir(), &crate::attachments::files_dir())
}

/// A value for a double-quoted string in a curl config file, where backslash escapes apply.
fn curl_quote(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n").replace('\r', "\\r")
}

// ---- Terminal sessions ----

/// OpenCode's interactive terminals on this Mac: (pid, folder). A `--session` one names the
/// session it opened on; the rest show the folder's latest session. The background service
/// and one-off commands (`run`, `auth`…) aren't terminals.
pub fn terminals(processes: &[procs::PsLine]) -> Vec<Terminal> {
    processes
        .iter()
        .filter_map(|p| {
            let (session, dir) = terminal_args(&p.command)?;
            let cwd = dir.or_else(|| procs::cwd_of(p.pid))?;
            Some(Terminal { pid: p.pid, cwd, session })
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq)]
pub struct Terminal {
    pub pid: u32,
    pub cwd: PathBuf,
    /// The session it was started on (`--session`), when it was.
    pub session: Option<String>,
}

/// For a process command line that is an interactive OpenCode: the session it was given, and
/// the folder argument. None for anything else.
pub fn terminal_args(command: &str) -> Option<(Option<String>, Option<PathBuf>)> {
    let mut words = command.split_whitespace();
    let program = words.next()?;
    let name = program.rsplit('/').next().unwrap_or(program);
    if name != "opencode" {
        return None;
    }
    let mut session = None;
    let mut dir = None;
    let mut words = words.peekable();
    while let Some(w) = words.next() {
        match w {
            "--session" | "-s" => session = words.next().map(String::from),
            "--server" | "--prompt" | "--log-level" => {
                words.next();
            }
            "mini" => {}
            _ if w.starts_with('-') => {}
            _ if dir.is_none() && (w.starts_with('/') || w.starts_with('~') || w.starts_with('.')) => dir = Some(crate::util::expand(w)),
            // Any other word is a subcommand: not a terminal.
            _ => return None,
        }
    }
    Some((session, dir))
}

/// The terminal showing a session, if one is: the one started on it, else one in its folder
/// when the session is the folder's latest.
pub fn terminal_for<'a>(terminals: &'a [Terminal], row: &SessionRow, latest_in_dir: bool) -> Option<&'a Terminal> {
    terminals
        .iter()
        .find(|t| t.session.as_deref() == Some(row.id.as_str()))
        .or_else(|| (latest_in_dir).then(|| terminals.iter().find(|t| t.session.is_none() && t.cwd == Path::new(&row.directory))).flatten())
}

/* ---- The session panel's conversation, and answering permission requests ---- */

/// An OpenCode message's items (transcript.rs): a user's text, or an assistant's text and tool parts.
pub fn conversation_items(v: &Value, seq: u64, at: i64) -> Vec<Item> {
    let mut out = Vec::new();
    let images: Vec<crate::transcript::Image> = v
        .get("files")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|f| f.get("url").and_then(Value::as_str).and_then(crate::transcript::Image::from_data_url))
        .collect();
    let user_text = v.get("text").and_then(Value::as_str).unwrap_or("").trim();
    if !user_text.is_empty() || !images.is_empty() {
        out.push(Item { images, ..item(at, "you", user_text) });
    }
    for p in v.get("content").and_then(Value::as_array).into_iter().flatten() {
        match p.get("type").and_then(Value::as_str) {
            Some("text") => {
                let t = p.get("text").and_then(Value::as_str).unwrap_or("").trim();
                if !t.is_empty() {
                    out.push(item(at, "agent", t));
                }
            }
            Some("tool") => {
                let name = p.get("name").and_then(Value::as_str).unwrap_or("tool");
                let state = p.get("state");
                let input = state.and_then(|s| s.get("input"));
                let output = state.and_then(|s| s.get("output")).map(|o| match o {
                    Value::String(s) => s.clone(),
                    other => serde_json::to_string_pretty(other).unwrap_or_default(),
                });
                let failed = state.and_then(|s| s.get("status")).and_then(Value::as_str) == Some("error");
                let command = input.and_then(|i| i.get("command")).and_then(Value::as_str);
                // Its shell tool (`bash` before 2.0) runs in the session's folder, which the record
                // doesn't repeat.
                let shell = matches!(name, "shell" | "bash") && command.is_some_and(|c| !c.trim().is_empty() && kept_whole(c));
                // OpenCode names its tools in lower case; the panel names them as the others do.
                let name = title_case(name);
                let label = match command {
                    Some(c) => format!("{name} · {}", code(&one_line(c, 80))),
                    None => name,
                };
                out.push(Item {
                    input: input.map(|i| clip_input(&command.map(String::from).unwrap_or_else(|| serde_json::to_string_pretty(i).unwrap_or_default()))),
                    output: output.map(|o| clip_output(&o)),
                    failed,
                    shell,
                    ..item(at, "tool", label)
                });
            }
            _ => {}
        }
    }
    crate::transcript::placed(out, seq)
}

/* ---- Reading sessions for the board ---- */

/// What each account's background service said last, while it was running, and whether it's
/// being asked now.
#[derive(Default)]
struct Polled {
    live: HashMap<String, Live>,
    /// When each account's requests were first seen (`Live::dated`), kept when a poll fails.
    seen: HashMap<String, Seen>,
    inflight: bool,
}

/// OpenCode's sessions for the board (`harness::Reader`): the database's rows, what the service
/// says is running and waiting, and the terminals that show sessions.
pub struct Reader {
    /// Sessions by account, newest first.
    sessions: HashMap<String, Vec<SessionRow>>,
    polled: Arc<Mutex<Polled>>,
    polled_at: Option<Instant>,
    /// OpenCode's interactive terminals, refreshed on the tick.
    terminals: Vec<Terminal>,
    marks: crate::harness::DatabaseMarks,
    wake: Wake,
}

impl Reader {
    fn new(wake: Wake) -> Reader {
        Reader { sessions: HashMap::new(), polled: Default::default(), polled_at: None, terminals: Vec::new(), marks: Default::default(), wake }
    }

    /// Reads an account's database again. True when a session is there that wasn't.
    fn reread(&mut self, a: &AccountCfg, look: &Look) -> bool {
        let rows = read_sessions(&a.home_path(), look.since, &look.seen.pinned_ids("opencode", &a.name));
        let before: HashSet<&str> = self.sessions.get(&a.name).map(|v| v.iter().map(|r| r.id.as_str()).collect()).unwrap_or_default();
        let added = rows.iter().any(|r| !before.contains(r.id.as_str()));
        self.sessions.insert(a.name.clone(), rows);
        // A write means a run may have started or asked something: ask the service.
        crate::harness::Reader::poll(self, std::slice::from_ref(a));
        added
    }
}

impl crate::harness::Reader for Reader {
    fn harness(&self) -> Harness {
        Harness::Opencode
    }

    fn scan(&mut self, accounts: &[AccountCfg], look: &Look) {
        self.sessions.clear();
        for a in accounts {
            let rows = read_sessions(&a.home_path(), look.since, &look.seen.pinned_ids("opencode", &a.name));
            self.sessions.insert(a.name.clone(), rows);
        }
        if !accounts.is_empty() {
            self.terminals = terminals(look.ps.all());
        }
    }

    /// The database, its WAL and its journal, all in the folder itself.
    fn watch(&self, account: &AccountCfg) -> Vec<(PathBuf, bool)> {
        vec![(account.home_path(), false)]
    }

    fn owns(&self, account: &AccountCfg, path: &Path) -> bool {
        path.starts_with(account.home_path())
    }

    fn apply(&mut self, account: &AccountCfg, paths: &[PathBuf], look: &Look) -> Applied {
        let home = account.home_path();
        if !paths.iter().any(|p| p.parent() == Some(home.as_path()) && crate::util::file_name(p).contains(".db")) {
            return Applied::default();
        }
        Applied { dirty: true, added: self.reread(account, look), ..Applied::default() }
    }

    /// Which sessions are live in a terminal, and where each shows.
    fn processes(&mut self, accounts: &[AccountCfg], look: &Look) -> Applied {
        if accounts.is_empty() {
            return Applied::default();
        }
        let fresh = terminals(look.ps.all());
        let dirty = fresh != self.terminals;
        self.terminals = fresh;
        Applied { dirty, ..Default::default() }
    }

    fn glance(&mut self, accounts: &[AccountCfg], look: &Look) -> Applied {
        let mut out = Applied::default();
        for a in accounts {
            if self.marks.changed(&a.name, &a.home_path().join("opencode.db")) {
                out.added |= self.reread(a, look);
                out.dirty = true;
            }
        }
        out
    }

    /// Asks each account's background service what's running and what it's waiting on: one
    /// poll at a time, and not more than one every 2 seconds.
    fn poll(&mut self, accounts: &[AccountCfg]) {
        if accounts.is_empty() || self.polled_at.is_some_and(|t| t.elapsed() < Duration::from_secs(2)) {
            return;
        }
        {
            let mut polled = self.polled.lock().unwrap_or_else(|e| e.into_inner());
            if polled.inflight {
                return;
            }
            polled.inflight = true;
        }
        self.polled_at = Some(Instant::now());
        let (accounts, polled, wake) = (accounts.to_vec(), self.polled.clone(), self.wake.clone());
        std::thread::spawn(move || {
            crate::os::system::utility_thread();
            let now = crate::util::now_ms();
            let results: Vec<(String, Option<Live>)> = accounts.into_iter().map(|a| (a.name.clone(), service(&a.home_path()).and_then(|svc| poll(&svc, now)))).collect();
            {
                let mut polled = polled.lock().unwrap_or_else(|e| e.into_inner());
                polled.inflight = false;
                for (account, live) in results {
                    match live {
                        Some(l) => {
                            let l = l.dated(polled.seen.entry(account.clone()).or_default());
                            polled.live.insert(account, l)
                        }
                        None => polled.live.remove(&account),
                    };
                }
            }
            wake();
        });
    }

    fn sessions(&self, account: &AccountCfg, look: &Look) -> Vec<Session> {
        let polled = self.polled.lock().unwrap_or_else(|e| e.into_inner());
        let live = polled.live.get(&account.name);
        let rows = self.sessions.get(&account.name).map(|v| v.as_slice()).unwrap_or(&[]);
        // A terminal in a folder shows the folder's latest session (rows are newest first).
        let mut latest: HashMap<&str, &str> = HashMap::new();
        for r in rows {
            latest.entry(r.directory.as_str()).or_insert(r.id.as_str());
        }
        rows.iter()
            .map(|r| {
                let key = Session::key_for(Harness::Opencode, &account.name, &r.id);
                let latest_here = latest.get(r.directory.as_str()) == Some(&r.id.as_str());
                let terminal = terminal_for(&self.terminals, r, latest_here).map(|t| t.pid);
                let mut s = crate::board::opencode_session(account, r, live, terminal, look.seen.seen_at(&key), look.now);
                // Archived in OpenCode, or here until it's active again.
                s.archived |= look.archived_here(&key, s.last_activity);
                s
            })
            .collect()
    }

    fn cwds(&self) -> Vec<String> {
        self.sessions.values().flatten().map(|r| r.directory.clone()).collect()
    }
}

/// OpenCode (`harness::Adapter`): its database for the conversation, and its service's
/// permission requests, answered through the service as its own TUI answers them.
pub struct OpenCode;

impl crate::harness::Adapter for OpenCode {
    fn harness(&self) -> crate::model::Harness {
        crate::model::Harness::Opencode
    }

    fn spec(&self) -> &'static crate::detect::Spec {
        const SPEC: crate::detect::Spec = crate::detect::Spec { harness: "opencode", program: Some("opencode"), default_command: Some("opencode"), var: Some(("XDG_DATA_HOME", Some("opencode"))), default_dir: "~/.local/share/opencode", siblings: None };
        &SPEC
    }

    fn signed_in(&self, home: &Path) -> (bool, Option<String>) {
        crate::opencode::signed_in(home)
    }

    /// It signs in to the providers you bring, not to an account of its own.
    fn sign_in_note(&self, home: &Path) -> Option<String> {
        crate::opencode::sign_in_note(home)
    }

    fn sign_in_command(&self, account: &crate::config::AccountCfg) -> String {
        format!("{} auth login", account.command())
    }

    fn installed(&self, home: &Path, on_path: &dyn Fn(&str) -> bool) -> bool {
        on_path("opencode") || crate::detect::is_executable(&home.join(".opencode/bin/opencode"))
    }

    fn folder_use(&self, dir: &Path) -> crate::harness::FolderUse {
        crate::harness::FolderUse {
            has_data: dir.join("opencode.db").is_file(),
            last_used: [dir.join("opencode.db"), dir.join("opencode.db-wal")].iter().filter_map(|p| crate::util::mtime_ms(p)).max(),
            signed_in: false,
        }
    }

    /// It reads `~/.config/opencode/skills` (and Claude Code's folders too).
    fn skills_dir(&self, _account: &crate::config::AccountCfg) -> Option<PathBuf> {
        Some(crate::util::home().join(".config/opencode/skills"))
    }

    /// Its command may set its data folder; `acp` is its own subcommand.
    fn acp_command(&self, account: &crate::config::AccountCfg) -> std::result::Result<(String, Vec<(String, String)>), String> {
        Ok((format!("{} acp", account.command()), Vec::new()))
    }

    fn places(&self, _account: &crate::config::AccountCfg, _home: &Path) -> std::result::Result<Vec<crate::start::Place>, String> {
        Ok(vec![crate::start::Place::Terminal, crate::start::Place::Background, crate::start::Place::Codealong])
    }

    /// Its messages are rows in its database.
    fn said(&self, account: &AccountCfg, since: i64, after: i64) -> Vec<Said> {
        read_prose(&account.home_path(), since, after)
    }

    fn records_tokens(&self) -> bool {
        true
    }

    fn token_uses_since(&self, account: &AccountCfg, after: i64) -> std::result::Result<(Vec<crate::ledger::TokenUse>, i64), String> {
        token_uses(&account.home_path(), &account.name, after)
    }

    /// A terminal showing it is focused; otherwise `opencode --session` opens a new one, which
    /// is also where a request its service holds is answered.
    fn targets(&self, s: &Session, o: &crate::harness::Opens) -> Vec<crate::model::Target> {
        let (verb, action) = match (o.respond, s.pid.is_some()) {
            (true, shown) => ("Respond in", if shown { "focus" } else { "resume" }),
            (false, true) => ("Show in", "focus"),
            (false, false) => ("Resume in", "resume"),
        };
        vec![o.way(s, action, "terminal", format!("{verb} {}", o.term), o.who.clone(), true, false)]
    }

    /// OpenCode picks its own session id and takes the prompt as an option; its files go as
    /// paths, and it has no `--worktree`, `--add-dir` or `--model` Codealong passes.
    fn start_in_terminal(&self, account: &AccountCfg, ask: &crate::harness::StartAsk) -> crate::harness::Typed {
        let prompt = crate::attachments::expand_in(ask.prompt, ask.images_dir, ask.files_dir, crate::attachments::Style::Path);
        let prompt = prompt.trim();
        let mut input = account.command().to_string();
        let mut env = Vec::new();
        if !prompt.is_empty() {
            input.push_str(&format!(" --prompt \"${}\"", crate::start::PROMPT_VAR));
            env.push((crate::start::PROMPT_VAR.to_string(), crate::start::as_argument(prompt)));
        }
        input.push('\n');
        crate::harness::Typed { input, env, key: None }
    }

    fn session_cwds(&self, home: &Path, since: i64) -> Vec<String> {
        read_sessions(home, since, &HashSet::new()).into_iter().map(|r| r.directory).collect()
    }

    fn no_limits(&self, _account: &AccountCfg) -> Option<&'static str> {
        Some("OpenCode bills the provider you bring, so it has no plan limits to show.")
    }

    /// The provider it sends your work to keeps that setting, not OpenCode.
    fn training(&self, account: &AccountCfg) -> crate::training::Training {
        crate::training::Training::unknown(account, "OpenCode sends your work to the providers it's signed in to, and each one's own settings say whether it trains on it.")
    }

    /// The servers in its config's `mcp.servers` (`disabled: true` or `enabled: false` turning one
    /// off), and those an older config keeps as `mcp`'s own entries, which 2.0.16 still takes in,
    /// whatever the account: `config.json`, `opencode.json` and `opencode.jsonc` in
    /// `~/.config/opencode`, each read over the one before (2.0.16's bundle).
    fn mcp(&self, account: &AccountCfg) -> crate::mcp::Mcp {
        let dir = crate::util::home().join(".config/opencode");
        let mut servers = serde_json::Map::new();
        let mut notes = Vec::new();
        for file in ["config.json", "opencode.json", "opencode.jsonc"].map(|f| dir.join(f)) {
            let Ok(text) = std::fs::read_to_string(&file) else { continue };
            match serde_json::from_str::<Value>(&without_comments(&text)) {
                Ok(v) => servers.extend(mcp_servers(&v["mcp"])),
                Err(_) if text.trim().is_empty() => {}
                Err(_) => notes.push(format!("{} doesn't parse", crate::util::tilde(&file))),
            }
        }
        let from = crate::util::tilde(&dir);
        crate::mcp::Mcp::new(account, crate::mcp::servers_in(&Value::Object(servers), &from), (!notes.is_empty()).then(|| notes.join("; ")))
    }

    fn records(&self, account: &crate::config::AccountCfg, _s: &crate::model::Session) -> Vec<PathBuf> {
        vec![account.home_path().join("opencode.db")]
    }

    /// Its service takes prompts for any session, whichever terminal shows it.
    fn routes(&self, _account: &crate::config::AccountCfg, _s: &crate::model::Session) -> Vec<crate::send::Route> {
        vec![Posted.into()]
    }

    fn conversation(&self, ctx: &crate::harness::Ctx, s: &crate::model::Session, before: Option<u64>, limit: usize) -> anyhow::Result<Page> {
        Ok(transcript_page(&ctx.account.home_path().join("opencode.db"), &s.id, before, limit, conversation_items))
    }

    fn conversation_after(&self, ctx: &crate::harness::Ctx, s: &crate::model::Session, after: u64) -> anyhow::Result<Page> {
        Ok(transcript_newer(&ctx.account.home_path().join("opencode.db"), &s.id, after, conversation_items))
    }

    fn open_calls(&self, ctx: &crate::harness::Ctx, s: &crate::model::Session) -> Vec<String> {
        open_calls(&ctx.account.home_path().join("opencode.db"), &s.id)
    }

    fn pending(&self, ctx: &crate::harness::Ctx, s: &crate::model::Session) -> Option<crate::harness::Pending> {
        let svc = service(&ctx.account.home_path())?;
        // OpenCode takes "always" for any request it asks about.
        permission_request(&svc, &s.id).map(|r| crate::harness::Pending { id: r.id, label: r.label, input: r.input, cannot: None, always: true, no_once: false, plan: false, message: false, always_rule: None, questions: Vec::new(), unwritten: false, terminal: None })
    }

    fn answer(&self, ctx: &crate::harness::Ctx, s: &crate::model::Session, id: &str, answer: crate::harness::Answer) -> anyhow::Result<String> {
        let decision = match answer {
            crate::harness::Answer::Allow => "once",
            crate::harness::Answer::Always => "always",
            crate::harness::Answer::Deny => "reject",
        };
        let label = reply(ctx, s, id, serde_json::json!({ "decision": decision }))?;
        Ok(match answer {
            crate::harness::Answer::Allow => format!("Allowed {label}."),
            crate::harness::Answer::Always => format!("Allowed {label}, and from now on."),
            crate::harness::Answer::Deny => format!("Denied {label}."),
        })
    }

    /// Its reply takes a `message` beside a rejection (2.0.16's API), which the run goes on with.
    fn deny_saying(&self, ctx: &crate::harness::Ctx, s: &crate::model::Session, id: &str, words: &str) -> anyhow::Result<Option<String>> {
        let label = reply(ctx, s, id, serde_json::json!({ "decision": "reject", "message": words.trim() }))?;
        Ok(Some(format!("Denied {label}, and told it what to do instead.")))
    }

    /// Its command line can't fork, but its service can: the copy is a session of its own
    /// (no parent), named "<title> (fork #1)", which the terminal UI then opens.
    fn fork(&self, ctx: &crate::harness::Ctx, s: &crate::model::Session) -> anyhow::Result<crate::harness::Typed> {
        let (_, id) = fork_in_service(ctx.account, s, None)?;
        Ok(fork_into(ctx.account, &id))
    }

    /// The service makes the copy and runs it, as it runs every session: the prompt goes to it
    /// as a message sent to a session does, and a terminal can open it later.
    fn fork_background(&self, ctx: &crate::harness::Ctx, s: &crate::model::Session, _cwd: &Path, prompt: &str) -> anyhow::Result<Option<String>> {
        let (svc, id) = fork_in_service(ctx.account, s, None)?;
        prompt_fork(&svc, s, &id, prompt)?;
        Ok(Some(Session::key_for(Harness::Opencode, &ctx.account.name, &id)))
    }

    /// The service copies what came before your prompt's message (`fork` with `before`), and the
    /// copy goes on as a background fork does.
    fn fork_from(&self, ctx: &crate::harness::Ctx, s: &crate::model::Session, _cwd: &Path, cursor: u64, prompt: &str) -> anyhow::Result<Option<String>> {
        let db = open(&ctx.account.home_path().join("opencode.db")).ok_or_else(|| anyhow::anyhow!("Codealong couldn't read OpenCode's database."))?;
        let before = message_before(&db, &s.id, cursor)?.ok_or_else(|| crate::harness::nothing_before(Harness::Opencode))?;
        let (svc, id) = fork_in_service(ctx.account, s, Some(&before))?;
        prompt_fork(&svc, s, &id, prompt)?;
        Ok(Some(Session::key_for(Harness::Opencode, &ctx.account.name, &id)))
    }

    /// Names the session through its service (`PATCH /api/session/<id>` with its title; checked on
    /// 2.0.16 against a service of its own), which keeps it in its database, where the board and
    /// its terminal read it.
    fn rename(&self, ctx: &crate::harness::Ctx, s: &crate::model::Session, name: &str) -> anyhow::Result<Vec<PathBuf>> {
        let home = ctx.account.home_path();
        let svc = service(&home).ok_or_else(|| anyhow::anyhow!("OpenCode's service isn't running, so Codealong can't rename “{}”. Open OpenCode and try again.", s.title))?;
        patch(&svc, &format!("/api/session/{}", urlencoding::encode(&s.id)), &serde_json::json!({ "title": name })).map_err(|e| anyhow::anyhow!("OpenCode couldn't rename “{}”: {e}", s.title))?;
        Ok(vec![home.join("opencode.db")])
    }

    /// Its service runs every session's turns, whichever terminal shows it, and stops one as its
    /// terminal UI's Escape does.
    fn interrupts(&self, ctx: &crate::harness::Ctx, _s: &crate::model::Session) -> bool {
        service(&ctx.account.home_path()).is_some()
    }

    /// Its service takes a prompt for the running turn (`delivery: "steer"`) as well as after it
    /// (`queue`).
    fn steers(&self, ctx: &crate::harness::Ctx, _s: &crate::model::Session) -> bool {
        service(&ctx.account.home_path()).is_some()
    }

    /// Stops the run without `resume`, which would go straight on with a message steering it:
    /// a Stop leaves the session waiting for you.
    fn interrupt(&self, ctx: &crate::harness::Ctx, s: &crate::model::Session) -> anyhow::Result<String> {
        let svc = service(&ctx.account.home_path()).ok_or_else(|| anyhow::anyhow!("OpenCode's service isn't running, so Codealong can't reach {}.", s.title))?;
        let got = post(&svc, &format!("/api/session/{}/interrupt", urlencoding::encode(&s.id)), &serde_json::json!({}))?;
        if !interrupted(&got) {
            anyhow::bail!("OpenCode says {} wasn't running anything.", s.title);
        }
        Ok(format!("Stopped “{}”.", s.title))
    }

    /// Every session and message is a row of `opencode.db`.
    fn databases(&self) -> &'static [&'static str] {
        &["opencode.db"]
    }

    fn reader(&self, wake: Wake) -> Box<dyn crate::harness::Reader> {
        Box::new(Reader::new(wake))
    }
}

/// Whether the service's answer to an interrupt says a run was stopped: `{interrupted}`, false
/// for a session that was idle.
fn interrupted(answer: &Value) -> bool {
    answer.get("interrupted").or_else(|| answer.pointer("/data/interrupted")).and_then(Value::as_bool) == Some(true)
}

/// The copy's id in the service's answer to a fork: `{data: {id, fork: {sessionID, boundary}}}`.
fn forked_id(answer: &Value) -> Option<String> {
    answer.get("data")?.get("id")?.as_str().map(String::from)
}

/// Has the service copy session `s`, only what came before message `before` when there's one,
/// and returns the service and the copy's id.
fn fork_in_service(account: &crate::config::AccountCfg, s: &crate::model::Session, before: Option<&str>) -> anyhow::Result<(Service, String)> {
    let svc = service(&account.home_path()).ok_or_else(|| anyhow::anyhow!("OpenCode's service isn't running, so Codealong can't fork {}. Open OpenCode and try again.", s.title))?;
    let body = before.map_or_else(|| serde_json::json!({}), |m| serde_json::json!({ "before": m }));
    let got = post(&svc, &format!("/api/session/{}/fork", urlencoding::encode(&s.id)), &body).map_err(|e| anyhow::anyhow!("OpenCode couldn't fork {}: {e}", s.title))?;
    let id = forked_id(&got).ok_or_else(|| anyhow::anyhow!("OpenCode forked {} but didn't say what it called the copy.", s.title))?;
    Ok((svc, id))
}

/// Sends a fork its first prompt, when there's one, as a message sent to a session goes.
fn prompt_fork(svc: &Service, s: &crate::model::Session, copy: &str, prompt: &str) -> anyhow::Result<()> {
    if !prompt.trim().is_empty() {
        let body = prompt_body(prompt.trim(), false);
        post(svc, &format!("/api/session/{}/prompt", urlencoding::encode(copy)), &body).map_err(|e| anyhow::anyhow!("OpenCode forked {} but didn't take the prompt: {e}", s.title))?;
    }
    Ok(())
}

/// The id of your message at `seq` (a conversation item's `cursor`), which a fork copies what
/// came before; None when none of the agent's messages came before it, so the copy would be
/// empty.
fn message_before(db: &Connection, session: &str, seq: u64) -> anyhow::Result<Option<String>> {
    let id: String = db
        .query_row("SELECT id FROM session_message WHERE session_id = ?1 AND seq = ?2 AND type = 'user'", rusqlite::params![session, seq as i64], |r| r.get(0))
        .map_err(|_| anyhow::anyhow!("That message isn't one of yours in OpenCode's record of the session."))?;
    let answered: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM session_message WHERE session_id = ?1 AND seq < ?2 AND type = 'assistant')", rusqlite::params![session, seq as i64], |r| r.get(0)).unwrap_or(false);
    Ok(answered.then_some(id))
}

/// Opens the terminal UI on `id`, a fork the service has made.
fn fork_into(account: &crate::config::AccountCfg, id: &str) -> crate::harness::Typed {
    crate::harness::Typed { input: format!("{} --session {id}\n", account.command()), env: Vec::new(), key: Some(Session::key_for(Harness::Opencode, &account.name, id)) }
}

struct PermissionRequest {
    id: String,
    label: String,
    input: Option<String>,
}

/// Answers the permission request `id` with `body`, checking first that it's still the one the
/// session waits on. Returns what it wanted to do, for the sentence that says what happened.
fn reply(ctx: &crate::harness::Ctx, s: &crate::model::Session, id: &str, body: Value) -> anyhow::Result<String> {
    let svc = service(&ctx.account.home_path()).ok_or_else(|| anyhow::anyhow!("OpenCode's service isn't running; answer it in OpenCode."))?;
    let Some(r) = permission_request(&svc, &s.id) else { anyhow::bail!("It isn't waiting on a permission any more.") };
    if r.id != id {
        anyhow::bail!("It's waiting on a different request now: {}. Look again before answering.", r.label);
    }
    post(&svc, &format!("/api/session/{}/permission/{}/reply", urlencoding::encode(&s.id), urlencoding::encode(id)), &body)?;
    Ok(r.label)
}

/// The oldest permission request a session's waiting on, from the service: what it wants to do
/// (`action`) and to what (`resources`, such as a command or a file).
fn permission_request(svc: &Service, session: &str) -> Option<PermissionRequest> {
    let v = get(svc, &format!("/api/session/{}/permission", urlencoding::encode(session)))?;
    let r = v.get("data")?.as_array()?.first()?;
    let action = r.get("action").and_then(Value::as_str).unwrap_or("permission");
    let resources: Vec<&str> = r.get("resources").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
    let first = resources.first().map(|x| format!(" · {}", code(&one_line(x, 80)))).unwrap_or_default();
    Some(PermissionRequest {
        id: r.get("id")?.as_str()?.to_string(),
        label: format!("{}{first}", title_case(action)),
        input: (!resources.is_empty()).then(|| clip_input(&resources.join("\n"))).or_else(|| r.get("message").and_then(Value::as_str).map(String::from)),
    })
}

/// A config's servers by name: 2.0's `mcp.servers`, after the ones an older config keeps as
/// `mcp`'s own entries (each an object; `timeout` is a setting of `mcp`'s, not a server).
fn mcp_servers(mcp: &Value) -> serde_json::Map<String, Value> {
    let mut out: serde_json::Map<String, Value> = mcp.as_object().into_iter().flatten().filter(|(k, v)| *k != "servers" && *k != "timeout" && v.is_object()).map(|(k, v)| (k.clone(), v.clone())).collect();
    out.extend(mcp["servers"].as_object().cloned().unwrap_or_default());
    out
}

/// JSON with comments, as OpenCode takes its config (`opencode.jsonc`): `//` and `/* */` comments
/// and commas before a closing bracket left out, strings as they are.
fn without_comments(text: &str) -> String {
    // Each pass keeps strings whole: a `"` opens one outside a string, and a `\` in one keeps
    // the character after it.
    fn pass(text: &str, mut outside: impl FnMut(char, &mut std::iter::Peekable<std::str::Chars>, &mut String)) -> String {
        let mut out = String::with_capacity(text.len());
        let mut chars = text.chars().peekable();
        let mut in_string = false;
        while let Some(c) = chars.next() {
            if in_string {
                out.push(c);
                match c {
                    '\\' => out.extend(chars.next()),
                    '"' => in_string = false,
                    _ => {}
                }
            } else if c == '"' {
                in_string = true;
                out.push(c);
            } else {
                outside(c, &mut chars, &mut out);
            }
        }
        out
    }
    let uncommented = pass(text, |c, chars, out| match (c, chars.peek()) {
        ('/', Some('/')) => {
            while chars.peek().is_some_and(|&n| n != '\n') {
                chars.next();
            }
        }
        ('/', Some('*')) => {
            chars.next();
            let mut last = ' ';
            for n in chars.by_ref() {
                if last == '*' && n == '/' {
                    break;
                }
                last = n;
            }
        }
        _ => out.push(c),
    });
    pass(&uncommented, |c, chars, out| {
        let closes = || chars.clone().find(|n| !n.is_whitespace()).is_some_and(|n| n == '}' || n == ']');
        if c != ',' || !closes() {
            out.push(c);
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn its_config_may_have_comments_and_trailing_commas() {
        let text = "{\n  // servers\n  \"mcp\": {\n    \"northwind\": { \"type\": \"local\", \"command\": [\"nw\", \"--url=http://x//y\"] }, /* parked */\n    \"off\": { \"type\": \"remote\", \"url\": \"https://e.com\", \"enabled\": false },\n  },\n}\n";
        let v: Value = serde_json::from_str(&without_comments(text)).unwrap();
        assert_eq!(v["mcp"]["northwind"]["command"][1], "--url=http://x//y");
        assert_eq!(crate::mcp::servers_in(&Value::Object(mcp_servers(&v["mcp"])), "f").iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["northwind"]);
    }

    #[test]
    fn its_servers_are_under_mcp_servers_and_an_older_config_still_counts() {
        let v = serde_json::json!({ "mcp": {
            "timeout": { "connect": 5000 },
            "atlas": { "type": "local", "command": ["atlas"] },
            "servers": {
                "kestrel": { "type": "remote", "url": "https://mcp.example.com" },
                "parked": { "type": "local", "command": ["x"], "disabled": true },
            },
        } });
        let names = crate::mcp::servers_in(&Value::Object(mcp_servers(&v["mcp"])), "f").into_iter().map(|s| s.name).collect::<Vec<_>>();
        assert_eq!(names, ["atlas", "kestrel"]);
        assert!(mcp_servers(&Value::Null).is_empty());
    }

    /// A request the session still waits on keeps the time it was first seen, whatever else
    /// changes and through a poll that missed it; a new one takes the time of the poll that saw
    /// it, as does one seen again after a long while.
    #[test]
    fn a_request_waited_on_keeps_when_it_was_first_seen() {
        let asked = |id: &str, at: i64| Asked { id: id.into(), ask: "Bash · `npm test`".into(), since: at };
        let poll = |at: i64, pending: Vec<(&str, &str)>| Live { at, pending: pending.into_iter().map(|(s, id)| (s.to_string(), asked(id, at))).collect(), ..Default::default() };
        let mut seen = Seen::new();
        poll(1_000, vec![("s1", "per_1"), ("s2", "per_2")]).dated(&mut seen);
        let now = poll(4_000, vec![("s1", "per_1"), ("s2", "per_3"), ("s3", "per_4")]).dated(&mut seen);
        let since = |l: &Live, s: &str| l.pending[s].since;
        assert_eq!((since(&now, "s1"), since(&now, "s2"), since(&now, "s3")), (1_000, 4_000, 4_000));
        // A poll that failed, or missed it, leaves its time alone.
        poll(6_000, vec![]).dated(&mut seen);
        assert_eq!(since(&poll(8_000, vec![("s1", "per_1")]).dated(&mut seen), "s1"), 1_000);
        // The same id on another session is another request.
        assert_eq!(since(&poll(9_000, vec![("s5", "per_1")]).dated(&mut seen), "s5"), 9_000);
        // Forgotten after a long while unseen.
        let later = 8_000 + FORGET_ASKED_MS;
        assert_eq!(since(&poll(later, vec![("s1", "per_1")]).dated(&mut seen), "s1"), later);
    }

    /// The tables read, with the columns read.
    const SCHEMA: &str = "CREATE TABLE session_v2 (id TEXT PRIMARY KEY, project_id TEXT NOT NULL DEFAULT 'p', parent_id TEXT, directory TEXT NOT NULL, title TEXT, \
        time_created INTEGER NOT NULL, time_updated INTEGER NOT NULL, time_idle INTEGER, idle_outcome TEXT, time_archived INTEGER, time_viewed INTEGER);\
        CREATE TABLE session_message (id TEXT PRIMARY KEY, session_id TEXT NOT NULL, type TEXT NOT NULL, seq INTEGER NOT NULL, \
        time_created INTEGER NOT NULL, time_updated INTEGER NOT NULL, data TEXT NOT NULL);";

    fn db_with(sql: &str) -> Connection {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(SCHEMA).unwrap();
        db.execute_batch(sql).unwrap();
        db
    }

    /// Settings names the providers it's signed in to as models.dev does, and says when it runs
    /// only the free models.
    #[test]
    fn says_which_providers_it_is_signed_in_to() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE credential (id TEXT PRIMARY KEY, integration_id TEXT NOT NULL, label TEXT NOT NULL, value TEXT NOT NULL, time_created INTEGER NOT NULL, time_updated INTEGER NOT NULL);\
             CREATE TABLE account (id TEXT PRIMARY KEY, email TEXT NOT NULL, url TEXT NOT NULL, time_created INTEGER NOT NULL, time_updated INTEGER NOT NULL);\
             CREATE TABLE kv (key TEXT PRIMARY KEY, value TEXT NOT NULL, time_created INTEGER NOT NULL, time_updated INTEGER NOT NULL);",
        )
        .unwrap();
        assert_eq!(sign_in_note_in(&db, false).as_deref(), Some("Free models only"));
        // The `auth.json` 2.0's migration left behind says nothing once the database keeps them.
        assert_eq!(sign_in_note_in(&db, true).as_deref(), Some("Free models only"));
        // An OpenCode account leaves it to the email.
        db.execute_batch("INSERT INTO account VALUES ('a', 'ana@example.com', 'https://opencode.ai', 1, 1);").unwrap();
        assert_eq!(sign_in_note_in(&db, false), None);
        // A provider the catalog doesn't name keeps its id; two credentials for one are one.
        let catalog = serde_json::json!({ "anthropic": { "name": "Anthropic" }, "github-copilot": { "name": "GitHub Copilot" } }).to_string();
        let kv = serde_json::json!({ "updatedAt": 1, "digest": "d", "body": catalog }).to_string();
        db.execute("INSERT INTO kv VALUES ('models-dev:catalog', ?1, 1, 1)", [kv]).unwrap();
        db.execute_batch(
            "INSERT INTO credential VALUES ('c1', 'github-copilot', 'OAuth', '{}', 10, 10), ('c2', 'anthropic', 'API key', '{}', 20, 20), \
             ('c3', 'anthropic', 'OAuth', '{}', 30, 30), ('c4', 'acme\"', 'API key', '{}', 40, 40);",
        )
        .unwrap();
        assert_eq!(sign_in_note_in(&db, false).as_deref(), Some("Signed in to GitHub Copilot, Anthropic and acme\""));
        // An older OpenCode's database has no credentials: its `auth.json` says it's signed in.
        let old = Connection::open_in_memory().unwrap();
        old.execute_batch("CREATE TABLE account (id TEXT PRIMARY KEY, email TEXT NOT NULL, url TEXT NOT NULL, time_created INTEGER NOT NULL, time_updated INTEGER NOT NULL);").unwrap();
        assert_eq!(sign_in_note_in(&old, true), None);
        assert_eq!(sign_in_note_in(&old, false).as_deref(), Some("Free models only"));
    }

    /// Edit from here forks what came before your message, by its id, and only once the agent
    /// has said something before it: a copy of nothing is a new session.
    #[test]
    fn a_fork_from_your_message_copies_what_came_before_it() {
        let db = db_with(
            "INSERT INTO session_message VALUES ('msg_1', 'ses_a', 'user', 1, 1, 1, '{\"text\":\"Fix it\"}'), ('msg_2', 'ses_a', 'assistant', 2, 2, 2, '{}'), \
             ('msg_3', 'ses_a', 'user', 3, 3, 3, '{\"text\":\"Now test it\"}'), ('msg_4', 'ses_b', 'user', 3, 3, 3, '{}');",
        );
        assert_eq!(message_before(&db, "ses_a", 3).unwrap().as_deref(), Some("msg_3"));
        assert_eq!(message_before(&db, "ses_a", 1).unwrap(), None);
        // Only a message of yours, and in that session.
        assert!(message_before(&db, "ses_a", 2).is_err());
        assert!(message_before(&db, "ses_a", 9).is_err());
    }

    #[test]
    fn finds_the_tool_calls_a_session_is_running() {
        let part = |status: &str, command: &str| serde_json::json!({ "type": "tool", "name": "bash", "state": { "status": status, "input": { "command": command } } });
        let msg = |parts: Vec<Value>| serde_json::json!({ "content": parts }).to_string().replace('\'', "''");
        let db = db_with(&format!(
            "INSERT INTO session_message VALUES ('m1', 'ses_a', 'assistant', 1, 1, 1, '{}'), ('m2', 'ses_a', 'assistant', 2, 2, 2, '{}'), ('m3', 'ses_b', 'assistant', 1, 1, 1, '{}');",
            msg(vec![part("completed", "ls"), part("error", "false")]),
            msg(vec![part("running", "codealong whoami"), part("pending", "sleep 1")]),
            msg(vec![part("running", "codealong todo done t-abc")]),
        ));
        assert_eq!(open_calls_in(&db, "ses_a"), vec![r#"{"command":"codealong whoami"}"#.to_string(), r#"{"command":"sleep 1"}"#.to_string()]);
        assert!(open_calls_in(&db, "ses_c").is_empty());
    }

    #[test]
    fn reads_how_full_the_context_is_from_the_latest_measured_reply_since_a_compaction() {
        let reply = |input: u64| format!(r#"{{"model":{{"id":"claude-opus-5-5"}},"tokens":{{"input":{input},"output":4,"reasoning":6,"cache":{{"read":488,"write":2}}}}}}"#);
        let streaming = r#"{"model":{"id":"claude-opus-5-5"}}"#;
        let db = db_with(&format!(
            "INSERT INTO session_v2 (id, directory, time_created, time_updated) VALUES ('ses_a', '/r', 1, 1);\
             INSERT INTO session_message (id, session_id, type, seq, time_created, time_updated, data) VALUES \
             ('m1', 'ses_a', 'assistant', 1, 10, 10, '{old}'), ('m2', 'ses_a', 'assistant', 2, 20, 20, '{now}'), ('m3', 'ses_a', 'assistant', 3, 30, 30, '{streaming}');",
            old = reply(500_000),
            now = reply(7000),
        ));
        // The reply still streaming has no tokens yet: the one before it says.
        assert_eq!(context_of(&db, "ses_a"), Some((7000 + 4 + 6 + 488 + 2, Some("claude-opus-5-5".to_string()))));
        // A compaction running, or one that failed, leaves the context as it was.
        db.execute_batch(
            "INSERT INTO session_message (id, session_id, type, seq, time_created, time_updated, data) VALUES \
             ('c1', 'ses_a', 'compaction', 4, 40, 40, '{\"status\":\"failed\"}'), ('c2', 'ses_a', 'compaction', 5, 50, 50, '{\"status\":\"running\"}');",
        )
        .unwrap();
        assert_eq!(context_of(&db, "ses_a").map(|c| c.0), Some(7000 + 4 + 6 + 488 + 2));
        // Once one is done, only what came after it; nothing measured yet is none.
        db.execute_batch("UPDATE session_message SET data = '{\"status\":\"completed\"}' WHERE id = 'c2';").unwrap();
        assert_eq!(context_of(&db, "ses_a"), None);
    }

    #[test]
    fn reads_what_each_reply_used_leaving_out_a_forks_copies() {
        // As 2.0.16 forks: the copy has an ID of its own and the first one's times.
        let reply = |input: u64, cost: f64| format!(r#"{{"model":{{"id":"claude-opus-5-5","providerID":"anthropic"}},"cost":{cost},"tokens":{{"input":{input},"output":4,"reasoning":6,"cache":{{"read":488,"write":0}}}}}}"#);
        let db = db_with(&format!(
            "ALTER TABLE session_v2 ADD COLUMN fork_session_id TEXT;\
             INSERT INTO session_v2 (id, directory, time_created, time_updated) VALUES ('ses_a', '/r', 1000, 1000);\
             INSERT INTO session_v2 (id, directory, time_created, time_updated, fork_session_id) VALUES ('ses_b', '/r', 5000, 5000, 'ses_a');\
             INSERT INTO session_v2 (id, parent_id, directory, time_created, time_updated) VALUES ('ses_sub', 'ses_b', '/r', 6000, 6000);\
             INSERT INTO session_message (id, session_id, type, seq, time_created, time_updated, data) VALUES \
             ('msg_1', 'ses_a', 'assistant', 1, 2000, 2100, '{one}'), ('msg_1b', 'ses_b', 'assistant', 1, 2000, 2100, '{one}'), \
             ('msg_2', 'ses_b', 'assistant', 2, 5500, 5600, '{two}'), ('msg_3', 'ses_sub', 'assistant', 1, 6500, 6600, '{two}');",
            one = reply(7659, 0.0),
            two = reply(100, 0.25),
        ));
        let (uses, newest) = token_uses_in(&db, "opencode", 0).unwrap();
        let mut got: Vec<(String, String, u64)> = uses.iter().map(|u| (u.id.clone(), u.session.clone(), u.input)).collect();
        got.sort();
        assert_eq!(
            got,
            [
                ("msg_1".to_string(), "opencode:opencode:ses_a".to_string(), 7659),
                ("msg_2".to_string(), "opencode:opencode:ses_b".to_string(), 100),
                ("msg_3".to_string(), "opencode:opencode:ses_b".to_string(), 100),
            ]
        );
        assert_eq!(newest, 6600);
        let two = uses.iter().find(|u| u.id == "msg_2").unwrap();
        assert_eq!((two.output, two.reasoning, two.cache_read, two.cost), (10, 6, 488, Some(0.25)));
        // Read on from there: nothing new.
        assert!(token_uses_in(&db, "opencode", newest).unwrap().0.is_empty());
        // With the session it copied gone, the fork's copy is the record that's left.
        db.execute_batch("DELETE FROM session_message WHERE session_id = 'ses_a'; DELETE FROM session_v2 WHERE id = 'ses_a';").unwrap();
        let (uses, _) = token_uses_in(&db, "opencode", 0).unwrap();
        assert!(uses.iter().any(|u| u.id == "msg_1b" && u.session == "opencode:opencode:ses_b"));
    }

    #[test]
    fn reads_what_a_session_was_forked_from_when_the_database_says() {
        let rows = |db: &Connection| read_sessions_from(db, 0, &HashSet::new()).unwrap().into_iter().map(|r| (r.id, r.fork_of)).collect::<HashMap<_, _>>();
        let before = db_with("INSERT INTO session_v2 (id, directory, time_created, time_updated) VALUES ('ses_a', '/r', 1, 1);");
        assert_eq!(rows(&before)["ses_a"], None);
        let forked = db_with(
            "ALTER TABLE session_v2 ADD COLUMN fork_session_id TEXT;\
             INSERT INTO session_v2 (id, directory, time_created, time_updated) VALUES ('ses_a', '/r', 1, 1);\
             INSERT INTO session_v2 (id, directory, time_created, time_updated, fork_session_id) VALUES ('ses_b', '/r', 2, 2, 'ses_a');",
        );
        let got = rows(&forked);
        assert_eq!(got["ses_a"], None);
        assert_eq!(got["ses_b"].as_deref(), Some("ses_a"));
    }

    #[test]
    fn its_mode_is_the_agent_picked_else_the_one_its_latest_reply_ran_as() {
        let agents = |db: &Connection| read_sessions_from(db, 0, &HashSet::new()).unwrap().into_iter().map(|r| (r.id, r.agent)).collect::<HashMap<_, _>>();
        let replies = "INSERT INTO session_v2 (id, directory, time_created, time_updated) VALUES ('ses_a', '/r', 1, 1), ('ses_b', '/r', 1, 1);\
             INSERT INTO session_message (id, session_id, type, seq, time_created, time_updated, data) VALUES \
             ('m1', 'ses_a', 'assistant', 1, 2, 2, '{\"agent\":\"build\"}'), ('m2', 'ses_b', 'assistant', 1, 2, 2, '{\"agent\":\"build\"}');";
        // A database from before the column goes by the replies.
        let got = agents(&db_with(replies));
        assert_eq!(got["ses_a"].as_deref(), Some("build"));
        let picked = db_with(&format!("ALTER TABLE session_v2 ADD COLUMN agent TEXT; {replies} UPDATE session_v2 SET agent = 'plan' WHERE id = 'ses_b';"));
        let got = agents(&picked);
        assert_eq!((got["ses_a"].as_deref(), got["ses_b"].as_deref()), (Some("build"), Some("plan")));
        assert_eq!(agent_mode("plan").kind, ModeKind::Plan);
        assert_eq!((agent_mode("build").label, agent_mode("review").label), ("Build".to_string(), "Review".to_string()));
    }

    #[test]
    fn its_model_is_the_one_picked_else_its_latest_replys_with_that_replys_variant() {
        let models = |db: &Connection| read_sessions_from(db, 0, &HashSet::new()).unwrap().into_iter().map(|r| (r.id, (r.model, r.effort))).collect::<HashMap<_, _>>();
        // Three sessions whose latest reply was at a variant, and one at none.
        let high = r#"'{"model":{"id":"gpt-6","providerID":"openai","variant":"high"}}'"#;
        let replies = format!(
            "INSERT INTO session_v2 (id, directory, time_created, time_updated) VALUES ('ses_a', '/r', 1, 1), ('ses_b', '/r', 1, 1), ('ses_c', '/r', 1, 1), ('ses_d', '/r', 1, 1);\
             INSERT INTO session_message (id, session_id, type, seq, time_created, time_updated, data) VALUES \
             ('m1', 'ses_a', 'assistant', 1, 2, 2, {high}), ('m2', 'ses_b', 'assistant', 1, 2, 2, {high}), ('m3', 'ses_c', 'assistant', 1, 2, 2, {high}), \
             ('m4', 'ses_d', 'assistant', 1, 2, 2, '{{\"model\":{{\"id\":\"big-pickle\",\"providerID\":\"opencode\",\"variant\":\"default\"}}}}');"
        );
        let pair = |m: &str, e: Option<&str>| (Some(m.to_string()), e.map(str::to_string));
        // A database from before the column goes by the replies; `default` is no variant picked.
        let got = models(&db_with(&replies));
        assert_eq!((&got["ses_a"], &got["ses_d"]), (&pair("gpt-6", Some("high")), &pair("big-pickle", None)));
        let picked = |a: &str, b: &str, c: &str| {
            models(&db_with(&format!(
                "ALTER TABLE session_v2 ADD COLUMN model TEXT; {replies} \
                 UPDATE session_v2 SET model = '{a}' WHERE id = 'ses_a'; UPDATE session_v2 SET model = '{b}' WHERE id = 'ses_b'; \
                 UPDATE session_v2 SET model = '{c}' WHERE id = 'ses_c';"
            )))
        };
        // Picked without a variant, the latest reply's counts while it was on that model; after a
        // switch to another model, or the same one from another provider, it doesn't.
        let got = picked(r#"{"id":"gpt-6","providerID":"openai"}"#, r#"{"id":"claude-opus-5-5","providerID":"anthropic"}"#, r#"{"id":"gpt-6","providerID":"azure"}"#);
        assert_eq!((&got["ses_a"], &got["ses_b"], &got["ses_c"]), (&pair("gpt-6", Some("high")), &pair("claude-opus-5-5", None), &pair("gpt-6", None)));
        // A variant picked with the model is its effort, `default` none, whatever the reply was at.
        let got = picked(r#"{"id":"gpt-6","providerID":"openai","variant":"default"}"#, r#"{"id":"claude-opus-5-5","providerID":"anthropic","variant":"max"}"#, "not json");
        assert_eq!((&got["ses_a"], &got["ses_b"], &got["ses_c"]), (&pair("gpt-6", None), &pair("claude-opus-5-5", Some("max")), &pair("gpt-6", Some("high"))));
    }

    #[test]
    fn reads_sessions_with_their_latest_messages() {
        let db = db_with(
            "INSERT INTO session_v2 (id, directory, title, time_created, time_updated, time_idle, idle_outcome) VALUES \
             ('ses_a', '/r/spur', 'Fix the build', 1000, 5000, 4000, 'succeeded'), \
             ('ses_old', '/r/spur', 'Old', 10, 20, 20, 'succeeded'), \
             ('ses_pinned', '/r/spur', 'Pinned', 10, 30, 30, 'succeeded');\
             INSERT INTO session_v2 (id, parent_id, directory, time_created, time_updated) VALUES ('ses_sub', 'ses_a', '/r/spur', 1000, 5000);\
             INSERT INTO session_message (id, session_id, type, seq, time_created, time_updated, data) VALUES \
             ('msg_1', 'ses_a', 'user', 1, 1500, 1500, '{\"text\":\"Fix the build\\nplease\",\"files\":[]}'), \
             ('msg_2', 'ses_a', 'assistant', 2, 1600, 3900, '{\"content\":[{\"type\":\"text\",\"text\":\"Looking.\"},{\"type\":\"tool\",\"name\":\"bash\",\"state\":{\"status\":\"completed\",\"input\":{\"command\":\"npm test\"}}},{\"type\":\"text\",\"text\":\"Done.\\nTwo files changed.\"}]}'), \
             ('msg_3', 'ses_a', 'user', 3, 4500, 4500, '{\"text\":\"Now the lint\"}');",
        );
        let keep: HashSet<String> = ["ses_pinned".to_string()].into();
        let rows = read_sessions_from(&db, 100, &keep).unwrap();
        let ids: Vec<&str> = rows.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, vec!["ses_a", "ses_pinned"]);
        let a = &rows[0];
        assert_eq!(a.title.as_deref(), Some("Fix the build"));
        assert_eq!(a.last_prompt.as_deref(), Some("Now the lint"));
        assert_eq!(a.last_prompt_at, Some(4500));
        assert_eq!(a.last_text.as_deref(), Some("Done."));
        assert_eq!(a.last_tool.as_deref(), Some("bash · `npm test`"));
        assert_eq!(a.last_ts, 5000);
        assert!(a.turn_open());
        let mut done = a.clone();
        done.idle_at = Some(4600);
        assert!(!done.turn_open());
    }

    /// A reply OpenCode is still writing grows in its row, under the same `seq`: the panel's
    /// next read has it again, until it's stamped complete.
    #[test]
    fn a_reply_still_streaming_is_read_again_as_it_grows() {
        let dir = crate::fixtures::scratch("oc-stream");
        let path = dir.join("opencode.db");
        let db = Connection::open(&path).unwrap();
        db.execute_batch(SCHEMA).unwrap();
        let reply = |text: &str, done: bool| {
            let time = if done { serde_json::json!({ "created": 1, "completed": 9 }) } else { serde_json::json!({ "created": 1 }) };
            serde_json::json!({ "time": time, "content": [{ "type": "text", "text": text }] }).to_string()
        };
        let write = |seq: i64, kind: &str, data: &str| {
            db.execute("INSERT OR REPLACE INTO session_message VALUES (?1, 'ses_a', ?2, ?3, ?3, ?3, ?4)", rusqlite::params![format!("m{seq}"), kind, seq, data]).unwrap();
        };
        let texts = |items: &[Item]| items.iter().map(|i| (i.cursor, i.text.clone())).collect::<Vec<_>>();
        write(1, "user", r#"{"text":"Count to three"}"#);
        write(2, "assistant", &reply("One", false));
        let first = transcript_page(&path, "ses_a", None, 50, conversation_items);
        assert_eq!(texts(&first.items), [(1, "Count to three".to_string()), (2, "One".to_string())]);
        // The same message, longer: read again, from its own place.
        write(2, "assistant", &reply("One, two", false));
        let next = transcript_newer(&path, "ses_a", first.after, conversation_items);
        assert_eq!(texts(&next.items), [(2, "One, two".to_string())]);
        // A prompt queued meanwhile comes after it, and doesn't stop it being read again.
        write(3, "user", r#"{"text":"Then stop"}"#);
        write(2, "assistant", &reply("One, two, three", true));
        let done = transcript_newer(&path, "ses_a", next.after, conversation_items);
        assert_eq!(texts(&done.items), [(2, "One, two, three".to_string()), (3, "Then stop".to_string())]);
        // Once it's complete, nothing is read twice.
        assert_eq!(done.after, 3);
        assert!(transcript_newer(&path, "ses_a", done.after, conversation_items).items.is_empty());
        // A new reply found by a newer read is read again too.
        write(4, "assistant", &reply("Stopping", false));
        let found = transcript_newer(&path, "ses_a", done.after, conversation_items);
        assert_eq!((texts(&found.items), found.after), (vec![(4, "Stopping".to_string())], 3));
    }

    #[test]
    fn reads_what_a_session_waits_on() {
        let perms = serde_json::json!({"data": [{"id": "per_1", "sessionID": "ses_a", "action": "bash", "resources": ["npm install"]}]});
        assert_eq!(permission_ask(&perms).as_deref(), Some("Bash · `npm install`"));
        let worded = serde_json::json!({"data": [{"id": "per_1", "sessionID": "ses_a", "action": "edit", "resources": ["/r/x"], "message": "Edit src/app.ts?"}]});
        assert_eq!(permission_ask(&worded).as_deref(), Some("Edit src/app.ts?"));
        assert_eq!(permission_ask(&serde_json::json!({"data": []})), None);
        let forms = serde_json::json!({"data": [{"id": "frm_1", "sessionID": "ses_a", "title": "Which database?", "fields": []}]});
        assert_eq!(form_ask(&forms).as_deref(), Some("Which database?"));
    }

    #[test]
    fn tells_terminals_from_other_opencode_processes() {
        assert_eq!(terminal_args("/Users/me/.opencode/bin/opencode"), Some((None, None)));
        assert_eq!(terminal_args("opencode --session ses_a /r/spur"), Some((Some("ses_a".into()), Some(PathBuf::from("/r/spur")))));
        assert_eq!(terminal_args("opencode --prompt hello --auto"), Some((None, None)));
        assert_eq!(terminal_args("opencode mini"), Some((None, None)));
        assert_eq!(terminal_args("opencode serve --service"), None);
        assert_eq!(terminal_args("opencode run fix it"), None);
        assert_eq!(terminal_args("/usr/bin/node /x/opencode"), None);
    }

    #[test]
    fn reads_the_service_state_file() {
        let dir = crate::fixtures::scratch("oc");
        let path = dir.join("service.json");
        std::fs::write(&path, format!(r#"{{"id":"x","version":"2.0.16","url":"http://127.0.0.1:49374/","pid":{},"password":"pw"}}"#, std::process::id())).unwrap();
        let svc = service_at(&path).unwrap();
        assert_eq!(svc.url, "http://127.0.0.1:49374");
        std::fs::write(&path, r#"{"url":"http://127.0.0.1:1","pid":4000000,"password":"pw"}"#).unwrap();
        assert_eq!(service_at(&path), None);
    }

    #[test]
    fn reads_whether_its_service_stopped_a_run() {
        assert!(interrupted(&serde_json::json!({ "interrupted": true })));
        assert!(!interrupted(&serde_json::json!({ "interrupted": false })));
        assert!(!interrupted(&Value::Null));
    }

    #[test]
    fn opens_the_fork_its_service_made() {
        // The service's answer to a fork, as 2.0.16 gave it.
        let answer: Value = serde_json::from_str(r#"{"data":{"id":"ses_f15f6e5a9ffe57CibPTgUS3IlW","fork":{"sessionID":"ses_f15f72088ffep4qnjzWbYxa0w6","boundary":{"type":"through","messageID":"msg_0dee2f9b8001Pd8F98MdK1test"}},"projectID":"global","title":"orig (fork #1)"}}"#).unwrap();
        let id = forked_id(&answer).unwrap();
        assert_eq!(id, "ses_f15f6e5a9ffe57CibPTgUS3IlW");
        let a = crate::config::AccountCfg { name: "opencode".into(), harness: "opencode".into(), home: "~/.local/share/opencode".into(), command: None, start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false };
        let f = fork_into(&a, &id);
        assert_eq!(f.input, "opencode --session ses_f15f6e5a9ffe57CibPTgUS3IlW\n");
        assert_eq!(f.key.as_deref(), Some("opencode:opencode:ses_f15f6e5a9ffe57CibPTgUS3IlW"));
        // A refusal (an empty session) has no copy in it.
        assert_eq!(forked_id(&serde_json::json!({"_tag": "InvalidRequestError", "kind": "empty_session"})), None);
    }

    #[test]
    fn a_prompt_is_posted_to_the_service_queued() {
        use crate::harness::Adapter as _;
        let account = AccountCfg { name: "opencode".into(), harness: "opencode".into(), home: "/h".into(), command: None, start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false };
        let s = Session { status: crate::model::Status::Idle, live: true, pid: Some(7), ..Session::new(Harness::Opencode, "opencode", "ses_1") };
        assert_eq!(OpenCode.routes(&account, &s), vec![Posted.into()]);
        assert_eq!(prompt_body("two\nlines", false).to_string(), r#"{"delivery":"queue","text":"two\nlines"}"#);
        assert_eq!(prompt_body("now", true).to_string(), r#"{"delivery":"steer","text":"now"}"#);

        let dir = crate::fixtures::scratch("oc-img");
        let imgs = dir.join("images");
        let files = dir.join("files");
        let shot = "0123456789abcdef.png";
        std::fs::create_dir_all(&imgs).unwrap();
        std::fs::write(imgs.join(shot), b"\x89PNG").unwrap();
        let body = prompt_body_in(&format!("Look\n\n![image](images/{shot})"), false, &imgs, &files);
        assert_eq!(body["text"], "Look");
        assert_eq!(body["files"][0]["mime"], "image/png");
        assert_eq!(body["files"][0]["filename"], shot);
        assert_eq!(body["files"][0]["url"], "data:image/png;base64,iVBORw==");
        let items = conversation_items(&body, 1, 100);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].text, "Look");
        assert_eq!(items[0].images, vec![crate::transcript::Image { number: None, base64: "iVBORw==".into() }]);
    }

    /// A password with a backslash or a quote in it signs in the same on a read as on a post:
    /// the read used to escape only quotes, and curl took the backslash for an escape.
    #[test]
    fn signs_in_the_same_way_for_every_request() {
        let svc = Service { url: "http://127.0.0.1:1".into(), pid: 1, password: r#"a\b"c"#.into() };
        assert_eq!(sign_in(&svc), "user = \"opencode:a\\\\b\\\"c\"\n");
        assert_eq!(curl_quote("two\nlines"), "two\\nlines");
    }
}
