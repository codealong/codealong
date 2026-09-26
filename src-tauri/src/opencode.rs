//! OpenCode (2.x): its SQLite database (`<data dir>/opencode.db`, WAL mode) holds every
//! session (`session_v2`) and message (`session_message`), and its background service
//! (`~/.local/state/opencode/service.json`) says which sessions are running right now and what
//! they're waiting on. Permission and question requests never reach the database, so they come
//! from the service alone. See *Harness adapters*.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::{Connection, OpenFlags};
use serde_json::Value;

use crate::procs;
use crate::transcript::{clip_input, clip_output, item, Item, Page};
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
    if !path.is_file() {
        return None;
    }
    let db = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX).ok()?;
    // OpenCode writes constantly while a session runs; a short wait beats an empty read.
    db.busy_timeout(Duration::from_millis(500)).ok()?;
    Some(db)
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

/// A message's words, for searching sessions' text (search.rs).
pub struct Prose {
    pub id: String,
    pub session: String,
    pub at: i64,
    pub updated: i64,
    /// You sent it; otherwise the agent said it.
    pub you: bool,
    pub text: String,
}

/// The text of messages written since `after` (by `time_updated`, since a reply streams into
/// its row) in top-level sessions updated since `since_ms`.
pub fn read_prose(home: &Path, since_ms: i64, after: i64) -> Vec<Prose> {
    let Some(db) = open(&home.join("opencode.db")) else { return Vec::new() };
    let read = || -> rusqlite::Result<Vec<Prose>> {
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
                (!text.is_empty()).then_some(Prose { id, session, at, updated, you, text })
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
    let config = format!("user = \"opencode:{}\"\ndata-binary = \"{}\"\n", curl_quote(&svc.password), curl_quote(&body.to_string()));
    let url = format!("{}{path}", svc.url);
    let out = procs::run_stdin(
        "/usr/bin/curl",
        &["-sS", "--fail-with-body", "-m", "10", "-K", "-", "-X", "POST", "-H", "Content-Type: application/json", "-H", "Accept: application/json", &url],
        &config,
        Duration::from_secs(15),
    )?;
    Ok(serde_json::from_str(&out).unwrap_or(Value::Null))
}

/// A value for a double-quoted string in a curl config file, where backslash escapes apply.
fn curl_quote(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n").replace('\r', "\\r")
}

// ---- Terminal sessions ----

/// OpenCode's interactive terminals on this Mac: (pid, folder). A `--session` one names the
/// session it opened on; the rest show the folder's latest session. The background service
/// and one-off commands (`run`, `auth`…) aren't terminals.
pub fn terminals() -> Vec<Terminal> {
    let Ok(out) = procs::run("/bin/ps", &["-axo", "pid=,command="], None, Duration::from_secs(3)) else { return Vec::new() };
    out.lines()
        .filter_map(|line| {
            let (pid, command) = line.trim().split_once(' ')?;
            let pid: u32 = pid.parse().ok()?;
            let (session, dir) = terminal_args(command)?;
            let cwd = dir.or_else(|| cwd_of(pid))?;
            Some(Terminal { pid, cwd, session })
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

/// A process's working folder, from lsof.
fn cwd_of(pid: u32) -> Option<PathBuf> {
    let out = procs::run("/usr/sbin/lsof", &["-a", "-p", &pid.to_string(), "-d", "cwd", "-Fn"], None, Duration::from_secs(3)).ok()?;
    out.lines().find_map(|l| l.strip_prefix('n')).map(PathBuf::from)
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
                    ..item(at, "tool", label)
                });
            }
            _ => {}
        }
    }
    crate::transcript::placed(out, seq)
}


/// OpenCode (`harness::Adapter`): its database for the conversation, and its service's
/// permission requests, answered through the service as its own TUI answers them.
pub struct OpenCode;

impl crate::harness::Adapter for OpenCode {
    fn harness(&self) -> crate::model::Harness {
        crate::model::Harness::Opencode
    }

    fn records(&self, account: &crate::config::AccountCfg, _s: &crate::model::Session) -> Vec<PathBuf> {
        vec![account.home_path().join("opencode.db")]
    }

    fn conversation(&self, ctx: &crate::harness::Ctx, s: &crate::model::Session, before: Option<u64>, limit: usize) -> anyhow::Result<Page> {
        Ok(transcript_page(&ctx.account.home_path().join("opencode.db"), &s.id, before, limit, conversation_items))
    }

    fn conversation_after(&self, ctx: &crate::harness::Ctx, s: &crate::model::Session, after: u64) -> anyhow::Result<Page> {
        Ok(transcript_newer(&ctx.account.home_path().join("opencode.db"), &s.id, after, conversation_items))
    }

    fn pending(&self, ctx: &crate::harness::Ctx, s: &crate::model::Session) -> Option<crate::harness::Pending> {
        let svc = service(&ctx.account.home_path())?;
        permission_request(&svc, &s.id).map(|r| crate::harness::Pending { id: r.id, label: r.label, input: r.input, cannot: None })
    }

    fn answer(&self, ctx: &crate::harness::Ctx, s: &crate::model::Session, id: &str, answer: crate::harness::Answer) -> anyhow::Result<String> {
        let svc = service(&ctx.account.home_path()).ok_or_else(|| anyhow::anyhow!("OpenCode's service isn't running; answer it in OpenCode."))?;
        let Some(r) = permission_request(&svc, &s.id) else { anyhow::bail!("It isn't waiting on a permission any more.") };
        if r.id != id {
            anyhow::bail!("It's waiting on a different request now: {}. Look again before answering.", r.label);
        }
        let decision = match answer {
            crate::harness::Answer::Allow => "once",
            crate::harness::Answer::Deny => "reject",
        };
        post(&svc, &format!("/api/session/{}/permission/{}/reply", urlencoding::encode(&s.id), urlencoding::encode(id)), &serde_json::json!({ "decision": decision }))?;
        Ok(match answer {
            crate::harness::Answer::Allow => format!("Allowed {}.", r.label),
            crate::harness::Answer::Deny => format!("Denied {}.", r.label),
        })
    }
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
    fn reads_sessions_with_their_latest_messages() {
        let db = db_with(
            "INSERT INTO session_v2 (id, directory, title, time_created, time_updated, time_idle, idle_outcome) VALUES \
             ('ses_a', '/r/kestrel', 'Fix the build', 1000, 5000, 4000, 'succeeded'), \
             ('ses_old', '/r/kestrel', 'Old', 10, 20, 20, 'succeeded'), \
             ('ses_pinned', '/r/kestrel', 'Pinned', 10, 30, 30, 'succeeded');\
             INSERT INTO session_v2 (id, parent_id, directory, time_created, time_updated) VALUES ('ses_sub', 'ses_a', '/r/kestrel', 1000, 5000);\
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
        assert_eq!(terminal_args("opencode --session ses_a /r/kestrel"), Some((Some("ses_a".into()), Some(PathBuf::from("/r/kestrel")))));
        assert_eq!(terminal_args("opencode --prompt hello --auto"), Some((None, None)));
        assert_eq!(terminal_args("opencode mini"), Some((None, None)));
        assert_eq!(terminal_args("opencode serve --service"), None);
        assert_eq!(terminal_args("opencode run fix it"), None);
        assert_eq!(terminal_args("/usr/bin/node /x/opencode"), None);
    }

    #[test]
    fn reads_the_service_state_file() {
        let dir = std::env::temp_dir().join(format!("codealong-oc-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("service.json");
        std::fs::write(&path, format!(r#"{{"id":"x","version":"2.0.16","url":"http://127.0.0.1:49374/","pid":{},"password":"pw"}}"#, std::process::id())).unwrap();
        let svc = service_at(&path).unwrap();
        assert_eq!(svc.url, "http://127.0.0.1:49374");
        std::fs::write(&path, r#"{"url":"http://127.0.0.1:1","pid":4000000,"password":"pw"}"#).unwrap();
        assert_eq!(service_at(&path), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
