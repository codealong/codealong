//! OpenCode (2.x): its SQLite database (`<data dir>/opencode.db`, WAL mode) holds every
//! session (`session_v2`) and message (`session_message`), and its background service
//! (`~/.local/state/opencode/service.json`) says which sessions are running right now and what
//! they're waiting on. Permission and question requests never reach the database, so they come
//! from the service alone. See *Harness adapters*.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rusqlite::Connection;
use serde_json::Value;

use crate::config::AccountCfg;
use crate::harness::{Applied, Look, Said, Wake};
use crate::model::{Harness, Session};
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
    /// The newest write to the session or its messages.
    pub last_ts: i64,
    /// The session it's a fork of (`fork_session_id`), which OpenCode's fork copies it from.
    pub fork_of: Option<String>,
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

fn read_sessions_from(db: &Connection, since_ms: i64, keep: &HashSet<String>) -> rusqlite::Result<Vec<SessionRow>> {
    let mut by_id: HashMap<String, SessionRow> = HashMap::new();
    let mut stmt = db.prepare(
        "SELECT id, directory, title, time_created, time_updated, time_idle, idle_outcome, time_archived, time_viewed \
         FROM session_v2 WHERE parent_id IS NULL AND (time_updated >= ?1 OR time_created >= ?1)",
    )?;
    let rows = stmt.query_map([since_ms], |r| {
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
    })?;
    for row in rows.flatten() {
        by_id.insert(row.id.clone(), row);
    }
    if !keep.is_empty() {
        let mut stmt = db.prepare(
            "SELECT id, directory, title, time_created, time_updated, time_idle, idle_outcome, time_archived, time_viewed \
             FROM session_v2 WHERE id = ?1",
        )?;
        for id in keep {
            if by_id.contains_key(id) {
                continue;
            }
            let row = stmt.query_row([id], |r| {
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
            });
            if let Ok(row) = row {
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
                row.last_prompt = v.get("text").and_then(Value::as_str).map(|t| one_line(t, 140)).filter(|t| !t.is_empty());
            }
            "assistant" => {
                let (text, tool) = assistant_summary(&v);
                row.last_text = text;
                row.last_text_tail = last_text_part(&v).map(|t| crate::jev::tail(t, 600)).filter(|t| !t.is_empty());
                row.last_tool = tool;
            }
            _ => {}
        }
    }
    let mut out: Vec<SessionRow> = by_id.into_values().collect();
    out.sort_by(|a, b| b.last_ts.cmp(&a.last_ts));
    Ok(out)
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

/// A session's messages before `before` (by `seq`; from the newest when none), up to about
/// `limit` items, for its panel (transcript.rs). `items` turns a message's `data` into items.
pub fn transcript_page<T>(db: &Path, session: &str, before: Option<u64>, limit: usize, items: impl Fn(&Value, u64, i64) -> Vec<T>) -> crate::transcript::PageOf<T> {
    let Some(db) = open(db) else { return crate::transcript::PageOf { items: Vec::new(), before: None, after: 0 } };
    let newest: i64 = db.query_row("SELECT COALESCE(MAX(seq), 0) FROM session_message WHERE session_id = ?1", [session], |r| r.get(0)).unwrap_or(0);
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
    crate::transcript::PageOf { items: out.into_iter().rev().flatten().collect(), before: (first_seq.is_some() && has_older).then_some(oldest as u64), after: newest as u64 }
}

/// A session's messages after `after` (by `seq`), oldest first. A message still streaming is
/// read again as it grows, from its own `seq`, so the panel replaces it by its place.
pub fn transcript_newer<T>(db: &Path, session: &str, after: u64, items: impl Fn(&Value, u64, i64) -> Vec<T>) -> crate::transcript::PageOf<T> {
    let Some(db) = open(db) else { return crate::transcript::PageOf { items: Vec::new(), before: None, after } };
    let read = || -> rusqlite::Result<(Vec<T>, u64)> {
        let mut stmt = db.prepare("SELECT seq, time_created, data FROM session_message WHERE session_id = ?1 AND seq > ?2 AND type IN ('user', 'assistant') ORDER BY seq")?;
        let mut last = after;
        let mut out = Vec::new();
        for (seq, at, data) in stmt.query_map(rusqlite::params![session, after as i64], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, String>(2)?)))?.flatten() {
            let v: Value = serde_json::from_str(&data).unwrap_or(Value::Null);
            out.extend(items(&v, seq as u64, at));
            last = seq as u64;
        }
        Ok((out, last))
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

/// Whether the account has a provider to talk to: a legacy `auth.json`, or credentials or an
/// OpenCode account in its database.
pub fn signed_in(home: &Path) -> bool {
    if home.join("auth.json").is_file() {
        return true;
    }
    let Some(db) = open(&home.join("opencode.db")) else { return false };
    let count = |table: &str| db.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get::<_, i64>(0)).unwrap_or(0);
    count("credential") > 0 || count("account") > 0
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
    pub pending: HashMap<String, String>,
    pub at: i64,
}

/// Asks the service what's running and what's waiting. Three short local requests at most:
/// the active list, and for each active session its permission and form lists.
pub fn poll(svc: &Service, now: i64) -> Option<Live> {
    let active = get(svc, "/api/session/active")?;
    let mut live = Live { at: now, ..Default::default() };
    if let Some(map) = active.get("data").and_then(Value::as_object) {
        live.active = map.keys().cloned().collect();
    }
    for id in &live.active {
        if let Some(ask) = get(svc, &format!("/api/session/{id}/permission")).and_then(|v| permission_ask(&v)) {
            live.pending.insert(id.clone(), ask);
            continue;
        }
        if let Some(ask) = get(svc, &format!("/api/session/{id}/form")).and_then(|v| form_ask(&v)) {
            live.pending.insert(id.clone(), ask);
        }
    }
    Some(live)
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
    let config = format!("user = \"opencode:{}\"\n", svc.password.replace('"', "\\\""));
    let url = format!("{}{path}", svc.url);
    let out = procs::run_stdin("/usr/bin/curl", &["-sS", "-m", "5", "-K", "-", "-H", "Accept: application/json", &url], &config, Duration::from_secs(8)).ok()?;
    serde_json::from_str(&out).ok()
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
    let config = format!("user = \"opencode:{}\"\ndata-binary = \"{}\"\n", curl_quote(&svc.password), curl_quote(&body.to_string()));
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
        post(&svc, &format!("/api/session/{}/prompt", urlencoding::encode(&d.session.id)), &prompt_body(&crate::attachments::expand(d.text, crate::attachments::Style::Path), d.now))?;
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
/// or, sent `now`, steered into the turn at once.
fn prompt_body(text: &str, now: bool) -> Value {
    serde_json::json!({ "text": text, "delivery": if now { "steer" } else { "queue" } })
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
    if let Some(t) = v.get("text").and_then(Value::as_str) {
        if !t.trim().is_empty() {
            out.push(item(at, "you", t.trim()));
        }
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
        Applied { dirty: true, added: self.reread(account, look), lines: Vec::new() }
    }

    /// Which sessions are live in a terminal, and where each shows.
    fn tick(&mut self, accounts: &[AccountCfg], look: &Look) -> Applied {
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
            procs::utility_thread();
            let now = crate::util::now_ms();
            let results: Vec<(String, Option<Live>)> = accounts.into_iter().map(|a| (a.name.clone(), service(&a.home_path()).and_then(|svc| poll(&svc, now)))).collect();
            {
                let mut polled = polled.lock().unwrap_or_else(|e| e.into_inner());
                polled.inflight = false;
                for (account, live) in results {
                    match live {
                        Some(l) => polled.live.insert(account, l),
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
        (crate::opencode::signed_in(home), None)
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
        Ok(vec![crate::start::Place::Terminal, crate::start::Place::Codealong])
    }

    /// Its messages are rows in its database.
    fn said(&self, account: &AccountCfg, since: i64, after: i64) -> Vec<Said> {
        read_prose(&account.home_path(), since, after)
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

    fn no_limits(&self) -> Option<&'static str> {
        Some("OpenCode bills the provider you bring, so it has no plan limits to show.")
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
        permission_request(&svc, &s.id).map(|r| crate::harness::Pending { id: r.id, label: r.label, input: r.input, cannot: None, always: true, plan: false, always_rule: None, questions: Vec::new() })
    }

    fn answer(&self, ctx: &crate::harness::Ctx, s: &crate::model::Session, id: &str, answer: crate::harness::Answer) -> anyhow::Result<String> {
        let svc = service(&ctx.account.home_path()).ok_or_else(|| anyhow::anyhow!("OpenCode's service isn't running; answer it in OpenCode."))?;
        let Some(r) = permission_request(&svc, &s.id) else { anyhow::bail!("It isn't waiting on a permission any more.") };
        if r.id != id {
            anyhow::bail!("It's waiting on a different request now: {}. Look again before answering.", r.label);
        }
        let decision = match answer {
            crate::harness::Answer::Allow => "once",
            crate::harness::Answer::Always => "always",
            crate::harness::Answer::Deny => "reject",
        };
        post(&svc, &format!("/api/session/{}/permission/{}/reply", urlencoding::encode(&s.id), urlencoding::encode(id)), &serde_json::json!({ "decision": decision }))?;
        Ok(match answer {
            crate::harness::Answer::Allow => format!("Allowed {}.", r.label),
            crate::harness::Answer::Always => format!("Allowed {}, and from now on.", r.label),
            crate::harness::Answer::Deny => format!("Denied {}.", r.label),
        })
    }

    /// Its command line can't fork, but its service can: the copy is a session of its own
    /// (no parent), named "<title> (fork #1)", which the terminal UI then opens.
    fn fork(&self, ctx: &crate::harness::Ctx, s: &crate::model::Session) -> anyhow::Result<crate::harness::Typed> {
        let (_, id) = fork_in_service(ctx.account, s)?;
        Ok(fork_into(ctx.account, &id))
    }

    /// The service makes the copy and runs it, as it runs every session: the prompt goes to it
    /// as a message sent to a session does, and a terminal can open it later.
    fn fork_background(&self, ctx: &crate::harness::Ctx, s: &crate::model::Session, _cwd: &Path, prompt: &str) -> anyhow::Result<Option<String>> {
        let (svc, id) = fork_in_service(ctx.account, s)?;
        if !prompt.trim().is_empty() {
            let body = serde_json::json!({ "text": crate::attachments::expand(prompt.trim(), crate::attachments::Style::Path), "delivery": "queue" });
            post(&svc, &format!("/api/session/{}/prompt", urlencoding::encode(&id)), &body).map_err(|e| anyhow::anyhow!("OpenCode forked {} but didn't take the prompt: {e}", s.title))?;
        }
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

/// Has the service copy session `s`, and returns the service and the copy's id.
fn fork_in_service(account: &crate::config::AccountCfg, s: &crate::model::Session) -> anyhow::Result<(Service, String)> {
    let svc = service(&account.home_path()).ok_or_else(|| anyhow::anyhow!("OpenCode's service isn't running, so Codealong can't fork {}. Open OpenCode and try again.", s.title))?;
    let got = post(&svc, &format!("/api/session/{}/fork", urlencoding::encode(&s.id)), &serde_json::json!({})).map_err(|e| anyhow::anyhow!("OpenCode couldn't fork {}: {e}", s.title))?;
    let id = forked_id(&got).ok_or_else(|| anyhow::anyhow!("OpenCode forked {} but didn't say what it called the copy.", s.title))?;
    Ok((svc, id))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn db_with(sql: &str) -> Connection {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE session_v2 (id TEXT PRIMARY KEY, project_id TEXT NOT NULL DEFAULT 'p', parent_id TEXT, directory TEXT NOT NULL, title TEXT, \
             time_created INTEGER NOT NULL, time_updated INTEGER NOT NULL, time_idle INTEGER, idle_outcome TEXT, time_archived INTEGER, time_viewed INTEGER);\
             CREATE TABLE session_message (id TEXT PRIMARY KEY, session_id TEXT NOT NULL, type TEXT NOT NULL, seq INTEGER NOT NULL, \
             time_created INTEGER NOT NULL, time_updated INTEGER NOT NULL, data TEXT NOT NULL);",
        )
        .unwrap();
        db.execute_batch(sql).unwrap();
        db
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
        let a = crate::config::AccountCfg { name: "opencode".into(), harness: "opencode".into(), home: "~/.local/share/opencode".into(), command: None, start: None, preferred: false, acp: None };
        let f = fork_into(&a, &id);
        assert_eq!(f.input, "opencode --session ses_f15f6e5a9ffe57CibPTgUS3IlW\n");
        assert_eq!(f.key.as_deref(), Some("opencode:opencode:ses_f15f6e5a9ffe57CibPTgUS3IlW"));
        // A refusal (an empty session) has no copy in it.
        assert_eq!(forked_id(&serde_json::json!({"_tag": "InvalidRequestError", "kind": "empty_session"})), None);
    }

    #[test]
    fn a_prompt_is_posted_to_the_service_queued() {
        use crate::harness::Adapter as _;
        let account = AccountCfg { name: "opencode".into(), harness: "opencode".into(), home: "/h".into(), command: None, start: None, preferred: false, acp: None };
        let s = Session { status: crate::model::Status::Idle, live: true, pid: Some(7), ..Session::new(Harness::Opencode, "opencode", "ses_1") };
        assert_eq!(OpenCode.routes(&account, &s), vec![Posted.into()]);
        assert_eq!(prompt_body("two\nlines", false).to_string(), r#"{"delivery":"queue","text":"two\nlines"}"#);
        assert_eq!(prompt_body("now", true).to_string(), r#"{"delivery":"steer","text":"now"}"#);
    }
}
