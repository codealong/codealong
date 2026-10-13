//! Antigravity (2.x): the hub app and its command line, `agy`, are one sign-in with a folder
//! each under `~/.gemini` (`antigravity`, `antigravity-cli`), laid out alike.
//! `conversation_summaries.db` lists a folder's conversations; `conversations/<id>.db` holds
//! one's steps; `brain/<id>/.system_generated/logs/transcript_full.jsonl` is the same steps as
//! JSON lines, which is what a panel reads; and a command line process keeps
//! `presence/<id>.lock` open while it has a conversation. See .claude/rules/antigravity.md.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use rusqlite::types::Value as Sql;
use rusqlite::Connection;
use serde_json::Value;

use crate::config::AccountCfg;
use crate::harness::{Applied, Ctx, Look};
use crate::model::{Harness, Session, Surface};
use crate::procs;
use crate::transcript::{clip_input, clip_output, item, kept_whole, Item, Page};
use crate::util::{code, command_dir, file_name, note_work, one_line, only_looks, parse_rfc3339_ms};

/// The hub app's folder and the command line's, beside each other.
const APP_DIR: &str = "antigravity";
const CLI_DIR: &str = "antigravity-cli";
const SUMMARIES: &str = "conversation_summaries.db";
/// Where a command line conversation without a workspace runs its commands.
const SCRATCH: &str = "scratch";
/// The command line as its installer names it.
pub const CLI: &str = "agy";
/// The hub app's bundle ID.
pub const APP_BUNDLE_ID: &str = "com.google.antigravity";
/// `CORTEX_STEP_STATUS_WAITING` in a conversation's `steps` table: the step's permission prompt
/// is up (read from `agy` 1.2.14's own definitions).
const STEP_WAITING: i64 = 9;
/// How much of a transcript's end is read for what a conversation is on.
const TAIL: u64 = 256 * 1024;

/// One of an account's folders of conversations.
#[derive(Debug, Clone, PartialEq)]
pub struct Folder {
    pub path: PathBuf,
    /// The command line's, whose conversations run in `agy` processes.
    pub cli: bool,
}

/// The folders an account's conversations are in: its own, and with the hub app's folder the
/// command line's beside it, which the same sign-in uses. An account kept in the command
/// line's folder has only that.
pub fn folders(home: &Path) -> Vec<Folder> {
    let name = file_name(home);
    if name == CLI_DIR {
        return vec![Folder { path: home.to_path_buf(), cli: true }];
    }
    let mut out = vec![Folder { path: home.to_path_buf(), cli: false }];
    if let Some(parent) = home.parent().filter(|_| name == APP_DIR) {
        out.push(Folder { path: parent.join(CLI_DIR), cli: true });
    }
    out
}

/// The command that runs an account's command line: its own, when config.toml gives one.
pub fn cli_command(account: &AccountCfg) -> &str {
    account.command.as_deref().unwrap_or(CLI)
}

/// Whether `agy` is installed for the user whose home is `home`: where its installer puts it,
/// or, for you, anywhere on your PATH.
pub fn cli_installed(home: &Path) -> bool {
    crate::detect::is_executable(&home.join(".local/bin").join(CLI)) || (home == crate::util::home() && crate::detect::on_user_path(CLI))
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Conversation {
    pub id: String,
    pub title: Option<String>,
    /// The summary's preview text: the conversation's opening, as the app lists it.
    pub preview: Option<String>,
    pub step_count: i64,
    /// The first workspace folder, as a path.
    pub cwd: Option<String>,
    pub workspaces: Vec<String>,
    /// The summary's status, lowercased: `cascade_run_status_idle`, `_running`, `_busy` or
    /// `_canceling`.
    pub status: String,
    pub source: String,
    /// The agent is still doing something.
    pub not_fully_idle: bool,
    pub killed: bool,
    pub last_modified: i64,
    /// When you last wrote to it.
    pub last_input: Option<i64>,
    /// The folder it's kept in.
    pub dir: PathBuf,
    /// It's the command line's: an `agy` process runs it, in a terminal or on its own.
    pub cli: bool,
    /// What the end of its record says.
    pub tail: Tail,
    /// A step's permission prompt is up, as its own database says.
    pub waiting: bool,
}

impl Conversation {
    /// Its summary says a turn is under way.
    pub fn running(&self) -> bool {
        self.not_fully_idle || ["running", "busy", "canceling"].iter().any(|w| self.status.ends_with(w))
    }
}

/// A folder's conversations changed since `since_ms`, plus any in `keep` (pinned ones), newest
/// first, without what the end of each one's record says (`with_tails`). Nested ones (subagents,
/// side questions) are left out.
pub fn read_conversations(folder: &Folder, since_ms: i64, keep: &HashSet<String>) -> Vec<Conversation> {
    let Some(db) = crate::harness::open_database(&folder.path.join(SUMMARIES)) else { return Vec::new() };
    let mut rows = read_from(&db, &file_name(&folder.path), since_ms, keep).unwrap_or_default();
    for c in &mut rows {
        c.dir = folder.path.clone();
        c.cli = folder.cli;
        // Without a workspace, the command line's works in its own scratch folder.
        if c.cwd.is_none() && folder.cli {
            c.cwd = Some(folder.path.join(SCRATCH).to_string_lossy().into_owned());
        }
    }
    rows
}

/// The summary table's rows for the folder named `dir`. Each folder's table also lists the
/// other folders' conversations (`app_data_dir` says whose each is), without their status.
fn read_from(db: &Connection, dir: &str, since_ms: i64, keep: &HashSet<String>) -> rusqlite::Result<Vec<Conversation>> {
    const COLUMNS: &str = "conversation_id, title, preview, step_count, last_modified_time, workspace_uris, status, source, \
         parent_conversation_id, nesting_depth, not_fully_idle, killed, last_user_input_time";
    // Older tables have no `app_data_dir`: every row is the folder's own.
    let mut stmt = db
        .prepare(&format!("SELECT {COLUMNS}, app_data_dir FROM conversation_summaries"))
        .or_else(|_| db.prepare(&format!("SELECT {COLUMNS}, '' FROM conversation_summaries")))?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, Option<String>>(1)?,
            r.get::<_, Option<String>>(2)?,
            r.get::<_, Option<i64>>(3)?.unwrap_or(0),
            r.get::<_, Sql>(4)?,
            r.get::<_, Option<String>>(5)?,
            r.get::<_, Option<String>>(6)?,
            r.get::<_, Option<String>>(7)?,
            r.get::<_, Option<String>>(8)?,
            r.get::<_, Option<i64>>(9)?.unwrap_or(0),
            r.get::<_, Sql>(10)?,
            r.get::<_, Sql>(11)?,
            r.get::<_, Sql>(12)?,
            r.get::<_, Option<String>>(13)?.unwrap_or_default(),
        ))
    })?;
    let mut out = Vec::new();
    for (id, title, preview, step_count, modified, uris, status, source, parent, depth, busy, killed, input, owner) in rows.flatten() {
        if depth > 0 || parent.map(|p| !p.is_empty()).unwrap_or(false) {
            continue;
        }
        if !owner.is_empty() && owner != dir {
            continue;
        }
        let last_modified = timestamp(&modified).unwrap_or(0);
        if last_modified < since_ms && !keep.contains(&id) {
            continue;
        }
        let workspaces: Vec<String> = uris.as_deref().and_then(|u| serde_json::from_str::<Vec<String>>(u).ok()).unwrap_or_default().iter().filter_map(|u| uri_path(u)).collect();
        out.push(Conversation {
            cwd: workspaces.first().cloned(),
            workspaces,
            id,
            title: title.map(|t| t.trim().to_string()).filter(|t| !t.is_empty()),
            preview: preview.map(|t| t.trim().to_string()).filter(|t| !t.is_empty()),
            step_count,
            status: status.unwrap_or_default().trim().to_lowercase(),
            source: source.unwrap_or_default(),
            not_fully_idle: truthy(&busy),
            killed: truthy(&killed),
            last_modified,
            last_input: timestamp(&input),
            ..Default::default()
        });
    }
    out.sort_by(|a, b| b.last_modified.cmp(&a.last_modified));
    Ok(out)
}

/// A gorm `datetime` column: text like `2026-09-24 14:16:07.434615-07:00`, or seconds or
/// milliseconds since the epoch. The year 1 is gorm's "never".
fn timestamp(v: &Sql) -> Option<i64> {
    match v {
        Sql::Text(t) => parse_rfc3339_ms(t.trim()).filter(|ms| *ms > 0),
        Sql::Integer(n) if *n > 100_000_000_000 => Some(*n),
        Sql::Integer(n) if *n > 0 => Some(n * 1000),
        Sql::Real(f) if *f > 100_000_000_000.0 => Some(*f as i64),
        Sql::Real(f) if *f > 0.0 => Some((f * 1000.0) as i64),
        _ => None,
    }
}

/// gorm's `numeric` booleans: 1/0, "true"/"false", or "1"/"0".
fn truthy(v: &Sql) -> bool {
    match v {
        Sql::Integer(n) => *n != 0,
        Sql::Real(f) => *f != 0.0,
        Sql::Text(t) => matches!(t.trim().to_lowercase().as_str(), "true" | "1" | "t"),
        _ => false,
    }
}

/// `file:///Users/me/My%20Repo` → `/Users/me/My Repo`. Other URIs (remote workspaces) aren't
/// folders here.
pub fn uri_path(uri: &str) -> Option<String> {
    let rest = uri.strip_prefix("file://")?;
    let path = rest.trim_start_matches("localhost");
    let path = if path.starts_with('/') { path } else { return None };
    let decoded = urlencoding::decode(path).ok()?.into_owned();
    Some(decoded.trim_end_matches('/').to_string()).filter(|p| !p.is_empty())
}

/// Whether a step of the conversation waits on you, from its own database.
fn step_waits(dir: &Path, id: &str) -> bool {
    let Some(db) = crate::harness::open_database(&dir.join("conversations").join(format!("{id}.db"))) else { return false };
    db.query_row("SELECT 1 FROM steps WHERE status = ?1 LIMIT 1", [STEP_WAITING], |_| Ok(())).is_ok()
}

/* ---- A conversation's record: its steps as JSON lines ---- */

/// A conversation's steps as lines: `transcript_full.jsonl` has each call's arguments as they
/// were, `transcript.jsonl` as strings of JSON, so the first is read when it's there.
pub fn transcript_file(dir: &Path, id: &str) -> Option<PathBuf> {
    let logs = dir.join("brain").join(id).join(".system_generated/logs");
    ["transcript_full.jsonl", "transcript.jsonl"].iter().map(|f| logs.join(f)).find(|p| p.is_file())
}

/// A tool call in a planner's step.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Call {
    /// The step its result is written as: results follow their planner's step, in the calls'
    /// order.
    pub result_step: i64,
    /// The call as a line, e.g. "Run command · `cargo test`".
    pub label: String,
    /// All of it: the command, or the arguments.
    pub input: String,
    /// The command a `run_command` runs, and the folder it runs in.
    pub command: Option<String>,
    pub cwd: Option<String>,
    /// Where it works: a command's folder, or the file it writes. A view, a listing and a search
    /// work nowhere, as an agent's reads of a repo it isn't working in don't.
    pub work: Option<String>,
    /// Its command only looks (`util::only_looks`), which is work only in the conversation's
    /// own folder.
    pub looks: bool,
}

/// One line of a transcript.
struct Step<'a> {
    index: i64,
    source: &'a str,
    kind: &'a str,
    status: &'a str,
    at: i64,
    content: &'a str,
    calls: Vec<Call>,
}

fn step(v: &Value) -> Option<Step<'_>> {
    let text = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("");
    let index = v.get("step_index").and_then(Value::as_i64)?;
    let calls = v.get("tool_calls").and_then(Value::as_array).map(|a| a.iter().enumerate().map(|(k, c)| call(c, index + 1 + k as i64)).collect()).unwrap_or_default();
    Some(Step { index, source: text("source"), kind: text("type"), status: text("status"), at: parse_rfc3339_ms(text("created_at")).unwrap_or(0), content: text("content"), calls })
}

/// An argument of a call as text, whichever way its transcript wrote it: as it was, or as a
/// string of JSON.
fn arg(args: &Value, key: &str) -> Option<String> {
    match args.get(key)? {
        Value::String(s) if s.starts_with('"') => Some(serde_json::from_str::<String>(s).unwrap_or_else(|_| s.clone())),
        Value::String(s) => Some(s.clone()),
        Value::Null => None,
        other => Some(other.to_string()),
    }
}

fn call(c: &Value, result_step: i64) -> Call {
    let name = c.get("name").and_then(Value::as_str).unwrap_or("tool");
    let args = c.get("args").cloned().unwrap_or(Value::Null);
    let said = name.replace('_', " ");
    let mut said = said.chars();
    let said = said.next().map(|f| f.to_uppercase().collect::<String>() + said.as_str()).unwrap_or_default();
    let command = arg(&args, "CommandLine").filter(|_| name == "run_command");
    let cwd = arg(&args, "Cwd").filter(|c| !c.is_empty());
    let file = ["TargetFile", "AbsolutePath", "DirectoryPath", "SearchPath", "SearchDirectory"].iter().find_map(|k| arg(&args, k)).filter(|p| p.starts_with('/'));
    let label = match (&command, arg(&args, "toolSummary").filter(|s| !s.trim().is_empty())) {
        (Some(c), _) => format!("{said} · {}", code(&one_line(c, 80))),
        (None, Some(summary)) => format!("{said} · {}", one_line(&summary, 80)),
        (None, None) => match &file {
            Some(f) => format!("{said} · {}", code(&file_name(Path::new(f)))),
            None => said,
        },
    };
    // The arguments without the two that only say what the call is for.
    let rest = || {
        let kept: serde_json::Map<String, Value> = args.as_object().into_iter().flatten().filter(|(k, _)| !matches!(k.as_str(), "toolAction" | "toolSummary")).map(|(k, _)| (k.clone(), Value::String(arg(&args, k).unwrap_or_default()))).collect();
        serde_json::to_string_pretty(&kept).unwrap_or_default()
    };
    let work = match (&command, &cwd) {
        (Some(c), Some(dir)) => command_dir(c, dir),
        _ => arg(&args, "TargetFile").filter(|p| p.starts_with('/')),
    };
    let looks = command.as_deref().is_some_and(only_looks);
    Call { result_step, label, input: command.clone().unwrap_or_else(rest), command, cwd, work, looks }
}

/// What you asked, out of a `USER_INPUT` step: Antigravity wraps it in `<USER_REQUEST>` and
/// adds the time and what settings changed after it.
fn request_of(content: &str) -> &str {
    let inner = content.split_once("<USER_REQUEST>").and_then(|(_, rest)| rest.split_once("</USER_REQUEST>")).map(|(asked, _)| asked);
    inner.unwrap_or(content).trim()
}

/// What a tool gave back, out of its result step: the text after the times it starts with, and
/// for a command its output alone when it exited with 0. With whether it failed.
fn result_of(content: &str, status: &str) -> (String, bool) {
    let body: Vec<&str> = content.lines().skip_while(|l| l.starts_with("Created At:") || l.starts_with("Completed At:") || l.trim().is_empty()).collect();
    let body = body.join("\n");
    let code = body.split_once("The command exited with code ").and_then(|(_, rest)| rest.split(|c: char| !c.is_ascii_digit() && c != '-').next()?.parse::<i32>().ok());
    let failed = code.is_some_and(|c| c != 0) || matches!(status, "ERROR" | "CANCELED");
    let text = match (code, body.split_once("Output:\n")) {
        (Some(0), Some((_, out))) => out.to_string(),
        _ => body,
    };
    (text.replace("\r\n", "\n"), failed)
}

/// The steps that are neither yours, the agent's, nor a tool's result: the app's own notes to
/// the model.
const ASIDES: [&str; 6] = ["SYSTEM_MESSAGE", "CHECKPOINT", "EPHEMERAL_MESSAGE", "CONVERSATION_HISTORY", "KNOWLEDGE_ARTIFACTS", "USER_INPUT"];

/// A transcript line's items (transcript.rs): your prompt, the agent's reply and its calls, or
/// a call's result, which is a step of its own. A call has no id in the record, so each gets
/// the number of the step its result is written as.
pub fn conversation_items(v: &Value, _at: i64) -> Vec<Item> {
    let Some(s) = step(v) else { return Vec::new() };
    match (s.source, s.kind) {
        ("USER_EXPLICIT", "USER_INPUT") => {
            let asked = request_of(s.content);
            if asked.is_empty() { Vec::new() } else { vec![item(s.at, "you", asked)] }
        }
        ("MODEL", "PLANNER_RESPONSE") => {
            let mut out = Vec::new();
            if !s.content.trim().is_empty() {
                out.push(item(s.at, "agent", s.content.trim()));
            }
            for c in &s.calls {
                let shell = c.command.as_deref().is_some_and(|cmd| !cmd.trim().is_empty() && kept_whole(cmd));
                out.push(Item { id: Some(c.result_step.to_string()), input: Some(clip_input(&c.input)), shell, cwd: c.cwd.clone().filter(|_| shell), ..item(s.at, "tool", c.label.clone()) });
            }
            out
        }
        (_, "ERROR_MESSAGE") if !s.content.trim().is_empty() => vec![item(s.at, "note", s.content.trim())],
        ("MODEL", kind) if !ASIDES.contains(&kind) => {
            let (output, failed) = result_of(s.content, s.status);
            vec![Item { id: Some(s.index.to_string()), output: Some(clip_output(&output)), failed, ..item(s.at, "result", "") }]
        }
        _ => Vec::new(),
    }
}

/// What the end of a conversation's record says.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Tail {
    /// The latest reply's start, and its end, for Jev.
    pub last_text: Option<String>,
    pub last_text_tail: Option<String>,
    /// The latest tool call, as a line.
    pub last_tool: Option<String>,
    /// The latest calls with no result yet: what runs now, or waits on you.
    pub open: Vec<Call>,
    /// Where its latest calls worked, oldest first.
    pub work: Vec<String>,
    /// Its latest step isn't a reply that ends a turn: your prompt, a call, or a call's result.
    pub turn_open: bool,
}

/// Reads the end of a transcript. `own` is the conversation's folder, where a command that
/// only looks still counts as work.
pub fn tail(path: &Path, own: Option<&str>) -> Tail {
    // Whole lines only: a cut drops the line it fell in, whatever it fell inside.
    crate::util::read_last(path, TAIL).map(|end| tail_of(&end.text(), own)).unwrap_or_default()
}

fn tail_of(text: &str, own: Option<&str>) -> Tail {
    let values: Vec<Value> = text.lines().filter_map(|l| serde_json::from_str(l).ok()).collect();
    // Lines aren't always in the steps' order: calls made together write their results before
    // their own step.
    let mut steps: Vec<Step> = values.iter().filter_map(step).collect();
    steps.sort_by_key(|s| s.index);
    let written: HashSet<i64> = steps.iter().map(|s| s.index).collect();
    let mut out = Tail::default();
    for s in steps.iter().filter(|s| s.source == "MODEL" && s.kind == "PLANNER_RESPONSE") {
        if !s.content.trim().is_empty() {
            out.last_text = Some(one_line(s.content, 140)).filter(|t| !t.is_empty());
            let chars: Vec<char> = s.content.trim().chars().collect();
            out.last_text_tail = Some(chars[chars.len().saturating_sub(600)..].iter().collect());
        }
        for c in &s.calls {
            out.last_tool = Some(c.label.clone());
            // The agent picks each command's folder: one run somewhere else only to look isn't
            // work there.
            if let Some(w) = c.work.as_ref().filter(|w| !c.looks || own.is_some_and(|own| Path::new(w).starts_with(own))) {
                note_work(&mut out.work, w.clone());
            }
        }
        if !s.calls.is_empty() {
            out.open = s.calls.iter().filter(|c| !written.contains(&c.result_step)).cloned().collect();
        }
    }
    // The app's own notes to the model don't say where a turn stands.
    out.turn_open = steps.iter().rev().find(|s| s.kind == "USER_INPUT" || s.source == "MODEL").is_some_and(|s| s.kind != "PLANNER_RESPONSE" || !s.calls.is_empty());
    out
}

/// The only way in for a conversation in the app: nothing there takes a prompt from outside, so
/// the app comes forward with the prompt on the clipboard to paste.
#[derive(Debug, Clone, PartialEq)]
pub struct OnClipboard;

impl crate::send::Own for OnClipboard {
    fn way(&self) -> crate::send::Way {
        crate::send::Way::Antigravity
    }

    fn takes_agents(&self) -> bool {
        false
    }

    fn leaves_it_to_paste(&self) -> bool {
        true
    }

    fn deliver(&self, d: &crate::send::Delivery) -> anyhow::Result<crate::send::Sent> {
        crate::send::copy(&crate::attachments::expand(d.text, crate::attachments::Style::Path))?;
        crate::launch::open_antigravity()?;
        Ok(crate::send::opened("Antigravity is open and the prompt is on the clipboard. Paste it there to send it.".to_string()))
    }
}

/// What a conversation was first asked, and when, from the start of its record.
fn first_request(path: &Path) -> Option<(String, i64)> {
    let text = crate::util::read_head(path, TAIL as usize)?;
    text.lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()).find_map(|v| {
        let s = step(&v).filter(|s| s.kind == "USER_INPUT")?;
        Some((one_line(request_of(s.content), 140), s.at)).filter(|(asked, _)| !asked.is_empty())
    })
}

/// A conversation a command line process has open that its folder's table doesn't list yet: a
/// print run writes its row only as it ends, though its record is written as it goes (checked
/// on 1.2.14). It runs in the process's folder.
fn started(dirs: &[Folder], p: &Process, now: i64) -> Option<Conversation> {
    let id = p.conversation.as_ref()?;
    let dir = dirs.iter().find(|f| f.cli && f.path.join("presence").join(format!("{id}.lock")).exists())?;
    let record = transcript_file(&dir.path, id);
    let (asked, at) = record.as_deref().and_then(first_request).unzip();
    let cwd = procs::cwd_of(p.pid).map(|c| c.to_string_lossy().into_owned());
    let tail = record.as_deref().map(|p| tail(p, cwd.as_deref())).unwrap_or_default();
    Some(Conversation {
        id: id.clone(),
        preview: asked,
        workspaces: cwd.iter().cloned().collect(),
        cwd,
        status: "cascade_run_status_running".into(),
        not_fully_idle: true,
        last_modified: record.as_deref().and_then(crate::util::mtime_ms).unwrap_or(now),
        last_input: at,
        dir: dir.path.clone(),
        cli: true,
        tail,
        ..Default::default()
    })
}

/* ---- The command line's processes ---- */

/// An `agy` process on this Mac.
#[derive(Debug, Clone, PartialEq)]
pub struct Process {
    pub pid: u32,
    /// The conversation it has open, from the presence lock it holds.
    pub conversation: Option<String>,
    /// It's the interactive UI in a terminal, rather than a print run.
    pub terminal: bool,
}

/// For a process's command line that's `agy` running a conversation: whether it's the
/// interactive UI rather than a print run. None for its other commands (`models`, `update`…)
/// and for anything else. `ps` joins the arguments with spaces, so a prompt reads as words;
/// only the words before it matter.
pub fn cli_args(command: &str) -> Option<bool> {
    let mut words = command.split_whitespace();
    let program = words.next()?;
    if program.rsplit('/').next() != Some(CLI) {
        return None;
    }
    while let Some(w) = words.next() {
        match w.split('=').next().unwrap_or(w) {
            "-p" | "--print" | "--prompt" | "--input-format" => return Some(false),
            "-i" | "--prompt-interactive" => return Some(true),
            "--add-dir" | "--agent" | "--conversation" | "--effort" | "--json-schema" | "--log-file" | "--mode" | "--model" | "--output-format" | "--print-timeout" | "--project" if !w.contains('=') => {
                words.next();
            }
            _ if w.starts_with('-') => {}
            "agent" | "agents" | "changelog" | "help" | "install" | "mcp" | "mic-serve" | "models" | "plugin" | "plugins" | "remote-control" | "update" => return None,
            // A prompt.
            _ => return Some(true),
        }
    }
    Some(true)
}

/// Whether a process's command line is the hub app's agent, which runs the app's conversations.
fn is_app_agent(command: &str) -> bool {
    let program = command.split(" --").next().unwrap_or(command);
    program.ends_with("/language_server") && program.contains("Antigravity")
}

/// The conversation a command line process has open: the presence lock among its open files
/// (`lsof -Fn`'s `n<path>` lines).
fn held_conversation(open_files: &str) -> Option<String> {
    open_files.lines().filter_map(|l| l.strip_prefix('n')).find_map(|p| {
        let p = Path::new(p);
        (p.parent().map(file_name).as_deref() == Some("presence") && p.extension().and_then(|x| x.to_str()) == Some("lock")).then(|| p.file_stem().map(|s| s.to_string_lossy().into_owned())).flatten()
    })
}

/// The command line's processes, each with the conversation it has open, and whether the hub
/// app's agent is running. From the list of every process, and an `lsof` for each `agy`.
pub fn processes(listed: &[procs::PsLine]) -> (Vec<Process>, bool) {
    let mut app = false;
    let mut found = Vec::new();
    for p in listed {
        app |= is_app_agent(&p.command);
        if let Some(interactive) = cli_args(&p.command) {
            let conversation = procs::run("/usr/sbin/lsof", &["-Fn", "-p", &p.pid.to_string()], None, Duration::from_secs(3)).ok().and_then(|files| held_conversation(&files));
            found.push(Process { pid: p.pid, conversation, terminal: interactive && !p.tty.starts_with('?') });
        }
    }
    (found, app)
}

/* ---- Starting a conversation ---- */

/// How long a list of models is good for.
const MODELS_FOR: Duration = Duration::from_secs(60 * 60);

/// The models the command line offers (`agy models`), which asks Google for them and takes a
/// second or two: asked off the caller's thread and kept for an hour, so the first call has
/// none. The reader asks as the app starts, so the list is there by the time a panel wants it.
fn models(command: &str) -> Vec<crate::start::ModelOption> {
    static KEPT: std::sync::Mutex<Option<(Instant, Vec<crate::start::ModelOption>)>> = std::sync::Mutex::new(None);
    let mut kept = KEPT.lock().unwrap_or_else(|e| e.into_inner());
    if kept.as_ref().map_or(true, |(at, _)| at.elapsed() >= MODELS_FOR) {
        // Marked asked now, so a slow answer isn't asked for twice.
        let have = kept.take().map(|(_, list)| list).unwrap_or_default();
        *kept = Some((Instant::now(), have));
        let command = format!("{command} models");
        std::thread::spawn(move || {
            // Through a shell: an account's command may set variables first.
            let listed = procs::run_background("/bin/sh", &["-c", &command], None, Duration::from_secs(30)).map(|out| listed_models(&out)).unwrap_or_default();
            if !listed.is_empty() {
                *KEPT.lock().unwrap_or_else(|e| e.into_inner()) = Some((Instant::now(), listed));
            }
        });
    }
    kept.as_ref().map(|(_, list)| list.clone()).unwrap_or_default()
}

/// `agy models` prints a line a model: what `--model` takes, a tab, and its name.
fn listed_models(out: &str) -> Vec<crate::start::ModelOption> {
    out.lines().filter_map(|l| l.split_once('\t')).map(|(id, label)| (id.trim(), label.trim())).filter(|(id, label)| crate::start::valid_model(id) && !label.is_empty()).map(|(id, label)| crate::start::ModelOption { id: id.to_string(), label: label.to_string() }).collect()
}

/* ---- The sign-in and plan usage ---- */

/// Where `agy` 1.3.4 keeps the sign-in it shares with the app, in the login keychain.
const KEYCHAIN_SERVICE: &str = "gemini";
const KEYCHAIN_ACCOUNT: &str = "antigravity";
/// The token file, beside the folders under ~/.gemini, that the sign-in was before `agy` moved
/// it into the keychain.
const TOKEN_FILE: &str = "jetski-standalone-oauth-token";
/// A locked keychain can hold `security` at a prompt; don't wait on it forever.
const KEYCHAIN_TIMEOUT: Duration = Duration::from_secs(10);
/// A usage read takes two seconds or so. One still going after this is stuck: signed out, `agy`
/// waits for a sign-in in the browser rather than exit.
const USAGE_TIMEOUT: Duration = Duration::from_secs(30);

/// Whether the account kept in `home` is signed in: the keychain holds the sign-in, or the token
/// file is there. Reads no secret.
fn signed_in(home: &Path) -> bool {
    home.parent().is_some_and(|p| p.join(TOKEN_FILE).is_file())
        || procs::run("/usr/bin/security", &["find-generic-password", "-s", KEYCHAIN_SERVICE, "-a", KEYCHAIN_ACCOUNT], None, KEYCHAIN_TIMEOUT).is_ok()
}

/// Where `agy` logs a usage read: one file, written over each time. Left to itself, each run
/// leaves a log of its own in the command line's folder, so a read every couple of minutes would
/// pile them up there.
fn usage_log() -> PathBuf {
    crate::util::home().join("Library/Logs/Codealong/agy-usage.log")
}

/// An account's plan usage, from the command line's `/usage` in a print run, which answers
/// without taking a turn, spends no quota and leaves no conversation behind (`agy` 1.1.11 on).
/// Blocks for a few seconds; call it off the engine thread.
pub fn read_usage(account: &AccountCfg, now: i64) -> crate::model::Usage {
    let mut u = crate::model::Usage { loading: false, ..crate::usage::loading(account) };
    let cli = cli_command(account);
    if !signed_in(&account.home_path()) {
        u.error = Some(format!("Not signed in: run {} once to sign in", code(cli)));
        u.needs_sign_in = true;
        return u;
    }
    let log = usage_log();
    let _ = std::fs::create_dir_all(log.parent().unwrap_or(Path::new("/tmp")));
    // Through a shell, as `models` asks: an account's command may set variables first.
    let command = format!("{cli} -p /usage --output-format json --log-file {}", crate::util::sh_quote(&log.to_string_lossy()));
    let answer = match procs::run_in("/bin/sh", &["-c", &command], &crate::util::home(), &[], "", USAGE_TIMEOUT) {
        Ok(out) => usage_answer(&out.out),
        Err(_) => UsageAnswer::Failed(format!("{} didn't answer in {} seconds", code(cli), USAGE_TIMEOUT.as_secs())),
    };
    match answer {
        UsageAnswer::Limits(meters) => {
            u.as_of = Some(now);
            if meters.is_empty() {
                u.error = Some("This plan doesn't report usage limits.".to_string());
            }
            u.meters = meters;
        }
        UsageAnswer::SignedOut => {
            u.error = Some(format!("Sign-in expired: run {} once to sign in again", code(cli)));
            u.needs_sign_in = true;
        }
        UsageAnswer::Failed(why) => u.error = Some(format!("Couldn't read usage: {why}")),
    }
    u
}

#[derive(Debug, PartialEq)]
enum UsageAnswer {
    Limits(Vec<crate::model::Meter>),
    SignedOut,
    Failed(String),
}

/// What a print run's `/usage` printed: `{status, error, command: {name, data: {groups}}}` on one
/// line, `status` `SUCCESS` or `ERROR`.
fn usage_answer(out: &str) -> UsageAnswer {
    // The answer is its last line; anything before it isn't the command's.
    let Some(v) = out.lines().rev().find_map(|l| serde_json::from_str::<Value>(l.trim()).ok().filter(Value::is_object)) else {
        return UsageAnswer::Failed("unexpected response".into());
    };
    if v.get("status").and_then(Value::as_str) != Some("SUCCESS") {
        let why = v.get("error").and_then(Value::as_str).unwrap_or("it failed");
        // What a run that couldn't sign in says.
        if why.contains("authentication") {
            return UsageAnswer::SignedOut;
        }
        return UsageAnswer::Failed(one_line(why, 120));
    }
    UsageAnswer::Limits(usage_meters(&v))
}

/// A meter for each of the answer's limits, `command.data.groups[].buckets[]`: each group of
/// models shares a limit, `{id, window, remaining_fraction, reset_time}`, weekly as seen.
fn usage_meters(v: &Value) -> Vec<crate::model::Meter> {
    let mut out = Vec::new();
    for g in v.pointer("/command/data/groups").and_then(Value::as_array).into_iter().flatten() {
        let name = group_label(g.get("name").and_then(Value::as_str).unwrap_or_default());
        if name.is_empty() {
            continue;
        }
        for b in g.get("buckets").and_then(Value::as_array).into_iter().flatten() {
            let Some(left) = b.get("remaining_fraction").and_then(Value::as_f64) else { continue };
            let window = b.get("window").and_then(Value::as_str).unwrap_or("weekly");
            let percent = ((1.0 - left) * 100.0).clamp(0.0, 100.0);
            out.push(crate::model::Meter {
                key: if window == "weekly" { format!("model:{name}") } else { format!("model:{name}:{window}") },
                label: name.clone(),
                window: format!("{}, {name} models", crate::usage::title(window)),
                percent,
                // An untouched limit's reset is a week from whenever it's asked, so it says nothing.
                resets_at: if left < 1.0 { b.get("reset_time").and_then(Value::as_str).and_then(parse_rfc3339_ms) } else { None },
                severity: crate::usage::severity(None, percent),
                model: Some(name.clone()),
            });
        }
    }
    out
}

/// A group's name for the bar: `Gemini Models` is `Gemini`, `Claude and GPT models` is
/// `Claude & GPT`.
fn group_label(name: &str) -> String {
    let name = name.trim();
    let cut = name.len().checked_sub(" models".len()).filter(|&i| name.get(i..).is_some_and(|end| end.eq_ignore_ascii_case(" models"))).map_or(name, |i| &name[..i]);
    cut.trim().replace(" and ", " & ")
}

/* ---- Reading conversations for the board ---- */

/// Antigravity's conversations for the board (`harness::Reader`): each account's summary tables
/// as last read, newest first, the command line's processes, and whether the app's agent runs.
#[derive(Default)]
pub struct Reader {
    conversations: HashMap<String, Vec<Conversation>>,
    marks: crate::harness::DatabaseMarks,
    processes: Vec<Process>,
    app_running: bool,
    looked_at: Option<Instant>,
}

impl Reader {
    /// Reads an account's summary tables again, and for the conversations that changed or that
    /// a process has open, the end of their records. True when a conversation is there that
    /// wasn't.
    fn reread(&mut self, a: &AccountCfg, look: &Look) -> bool {
        let dirs = folders(&a.home_path());
        // A change in the command line's folder is often a process that came or went.
        if dirs.iter().any(|f| f.cli) && self.looked_at.map_or(true, |t| t.elapsed() >= Duration::from_secs(2)) {
            self.look_at_processes(look);
        }
        let keep = look.seen.pinned_ids("antigravity", &a.name);
        let before: HashMap<String, Conversation> = self.conversations.remove(&a.name).unwrap_or_default().into_iter().map(|c| (c.id.clone(), c)).collect();
        let mut rows: Vec<Conversation> = dirs.iter().flat_map(|f| read_conversations(f, look.since, &keep)).collect();
        // The same conversation once, should two folders' tables both claim it.
        let mut listed = HashSet::new();
        rows.retain(|c| listed.insert(c.id.clone()));
        rows.extend(self.processes.iter().filter(|p| p.conversation.as_ref().is_some_and(|id| !listed.contains(id))).filter_map(|p| started(&dirs, p, look.now)));
        rows.sort_by(|x, y| y.last_modified.cmp(&x.last_modified));
        let mut added = false;
        for c in &mut rows {
            let held = self.processes.iter().any(|p| p.conversation.as_deref() == Some(c.id.as_str()));
            match before.get(&c.id) {
                Some(old) if !held && old.last_modified == c.last_modified && old.step_count == c.step_count => c.tail = old.tail.clone(),
                old => {
                    added |= old.is_none();
                    c.tail = transcript_file(&c.dir, &c.id).map(|p| tail(&p, c.cwd.as_deref())).unwrap_or_default();
                }
            }
            c.waiting = (c.running() || held) && step_waits(&c.dir, &c.id);
        }
        self.conversations.insert(a.name.clone(), rows);
        added
    }

    /// Looks at the processes again. True when they changed.
    fn look_at_processes(&mut self, look: &Look) -> bool {
        self.looked_at = Some(Instant::now());
        let (processes, app_running) = processes(look.ps.all());
        let changed = processes != self.processes || app_running != self.app_running;
        self.processes = processes;
        self.app_running = app_running;
        changed
    }
}

impl crate::harness::Reader for Reader {
    fn harness(&self) -> Harness {
        Harness::Antigravity
    }

    fn scan(&mut self, accounts: &[AccountCfg], look: &Look) {
        self.conversations.clear();
        if !accounts.is_empty() {
            self.look_at_processes(look);
        }
        for a in accounts {
            self.reread(a, look);
        }
    }

    /// Each folder's summary table, its WAL and its journal, all in the folder itself.
    fn watch(&self, account: &AccountCfg) -> Vec<(PathBuf, bool)> {
        folders(&account.home_path()).into_iter().map(|f| (f.path, false)).collect()
    }

    fn owns(&self, account: &AccountCfg, path: &Path) -> bool {
        folders(&account.home_path()).iter().any(|f| path.starts_with(&f.path))
    }

    fn apply(&mut self, account: &AccountCfg, paths: &[PathBuf], look: &Look) -> Applied {
        let dirs = folders(&account.home_path());
        if !paths.iter().any(|p| file_name(p).contains(".db") && dirs.iter().any(|f| p.parent() == Some(f.path.as_path()))) {
            return Applied::default();
        }
        Applied { dirty: true, added: self.reread(account, look), ..Applied::default() }
    }

    /// Keeps the list of models a start can ask for at hand (`models`).
    fn poll(&mut self, accounts: &[AccountCfg]) {
        for a in accounts.iter().filter(|a| a.command.is_some() || cli_installed(&crate::util::home())) {
            models(cli_command(a));
        }
    }

    /// Which conversations a command line process has open, and whether the app's agent runs.
    /// A process that came may have a conversation its table doesn't list yet, and one that
    /// went leaves its conversation as it was: either way they're read again.
    fn tick(&mut self, accounts: &[AccountCfg], look: &Look) -> Applied {
        let mut out = Applied::default();
        if !accounts.is_empty() && self.look_at_processes(look) {
            out.dirty = true;
            for a in accounts {
                out.added |= self.reread(a, look);
            }
        }
        out
    }

    /// The summary tables, and the databases of the conversations under way or open in a
    /// process, where steps are written, and one starts to wait on you, without the summary
    /// changing.
    fn glance(&mut self, accounts: &[AccountCfg], look: &Look) -> Applied {
        let mut out = Applied::default();
        for a in accounts {
            let mut changed = false;
            for f in folders(&a.home_path()) {
                changed |= self.marks.changed(&format!("{}|{}", a.name, f.path.display()), &f.path.join(SUMMARIES));
            }
            let held = |c: &&Conversation| self.processes.iter().any(|p| p.conversation.as_deref() == Some(c.id.as_str()));
            let under_way: Vec<PathBuf> = self.conversations.get(&a.name).into_iter().flatten().filter(|c| c.running() || held(c)).map(|c| c.dir.join("conversations").join(format!("{}.db", c.id))).collect();
            for db in under_way {
                changed |= self.marks.changed(&db.to_string_lossy(), &db);
            }
            if changed {
                out.added |= self.reread(a, look);
                out.dirty = true;
            }
        }
        out
    }

    fn sessions(&self, account: &AccountCfg, look: &Look) -> Vec<Session> {
        let rows = self.conversations.get(&account.name).map(|v| v.as_slice()).unwrap_or(&[]);
        rows.iter()
            .map(|c| {
                let key = Session::key_for(Harness::Antigravity, &account.name, &c.id);
                let process = self.processes.iter().find(|p| p.conversation.as_deref() == Some(c.id.as_str()));
                let mut s = crate::board::antigravity_session(account, c, process, self.app_running, look.seen.seen_at(&key), look.now);
                s.archived = look.archived_here(&key, s.last_activity);
                s
            })
            .collect()
    }

    fn cwds(&self) -> Vec<String> {
        self.conversations.values().flatten().filter_map(|c| c.cwd.clone()).collect()
    }
}

/// The folder a conversation on the board is kept in: the command line's for one it runs, else
/// the first of the account's that has its record.
fn folder_of(account: &AccountCfg, s: &Session) -> PathBuf {
    let dirs = folders(&account.home_path());
    let has = |f: &&Folder| f.path.join("conversations").join(format!("{}.db", s.id)).is_file() || f.path.join("brain").join(&s.id).is_dir();
    let by_surface = dirs.iter().find(|f| f.cli == (s.surface == Surface::Cli));
    dirs.iter().find(has).or(by_surface).or(dirs.first()).map(|f| f.path.clone()).unwrap_or_else(|| account.home_path())
}

/// Antigravity (`harness::Adapter`): the hub app's conversations and the command line's, read
/// from their records. The app takes nothing from outside; the command line starts and resumes
/// conversations in a terminal.
pub struct Antigravity;

impl Antigravity {
    fn record(&self, ctx: &Ctx, s: &Session) -> anyhow::Result<PathBuf> {
        transcript_file(&folder_of(ctx.account, s), &s.id).ok_or_else(|| anyhow::anyhow!("Antigravity hasn't written this conversation's record yet."))
    }
}

impl crate::harness::Adapter for Antigravity {
    fn harness(&self) -> Harness {
        Harness::Antigravity
    }

    /// One account, in the hub app's folder: the command line's folder goes with it (`folders`),
    /// and nothing moves either.
    fn spec(&self) -> &'static crate::detect::Spec {
        const SPEC: crate::detect::Spec = crate::detect::Spec { harness: "antigravity", program: None, default_command: None, var: None, default_dir: "~/.gemini/antigravity", siblings: None };
        &SPEC
    }

    /// The sign-in is in the keychain, or from before, a token file under ~/.gemini; only
    /// whether it's there is read.
    fn signed_in(&self, home: &Path) -> (bool, Option<String>) {
        (signed_in(home), None)
    }

    /// The command line signs in as it starts; without it, the app does from its own window.
    fn sign_in_command(&self, account: &AccountCfg) -> String {
        if cli_installed(&crate::util::home()) {
            cli_command(account).to_string()
        } else {
            "open -a Antigravity".to_string()
        }
    }

    fn installed(&self, home: &Path, on_path: &dyn Fn(&str) -> bool) -> bool {
        crate::detect::app("Antigravity.app").is_some() || on_path(CLI) || crate::detect::is_executable(&home.join(".local/bin").join(CLI))
    }

    fn folder_use(&self, dir: &Path) -> crate::harness::FolderUse {
        let files: Vec<PathBuf> = folders(dir).iter().flat_map(|f| [f.path.join(SUMMARIES), f.path.join(format!("{SUMMARIES}-wal"))]).collect();
        crate::harness::FolderUse { has_data: files.iter().any(|p| p.is_file() && !file_name(p).ends_with("-wal")), last_used: files.iter().filter_map(|p| crate::util::mtime_ms(p)).max(), signed_in: false }
    }

    fn skills_dir(&self, _account: &AccountCfg) -> Option<PathBuf> {
        Some(crate::util::home().join(".gemini/config/skills"))
    }

    /// The command line starts a conversation in a terminal; the app only in its own window.
    fn places(&self, account: &AccountCfg, home: &Path) -> std::result::Result<Vec<crate::start::Place>, String> {
        if account.command.is_some() || cli_installed(home) {
            Ok(vec![crate::start::Place::Terminal, crate::start::Place::Background])
        } else {
            Err(format!("The Antigravity app starts conversations only in its own window. With its command line, {CLI}, installed, {} can start them in a terminal from here.", account.name))
        }
    }

    /// The command line's conversation opens in its terminal, or a new one that resumes it
    /// (`agy --conversation`); a print run has it until it ends. The app has no link to a
    /// conversation, so the app itself comes forward.
    fn targets(&self, s: &Session, o: &crate::harness::Opens) -> Vec<crate::model::Target> {
        let who = o.who.clone();
        match s.surface {
            Surface::Cli => {
                let busy = s.live && s.pid.is_none();
                vec![o.terminal(s, if busy { format!("{who} · a run without a terminal has it open") } else { who }, busy)]
            }
            Surface::App => vec![o.way(s, "antigravity", "app", "Open Antigravity".into(), format!("{who} · no link to a conversation, so the app comes forward"), true, false)],
        }
    }

    fn open(&self, _s: &Session, action: &str) -> anyhow::Result<()> {
        match action {
            "antigravity" => crate::launch::open_antigravity(),
            other => anyhow::bail!("Unknown action {other:?}"),
        }
    }

    fn app_bundle(&self, action: &str) -> Option<&'static str> {
        (action == "antigravity").then_some(APP_BUNDLE_ID)
    }

    /// Each conversation's transcript, in both folders.
    fn search_files(&self, account: &AccountCfg, since: i64) -> Vec<(PathBuf, String, i64)> {
        let rows = folders(&account.home_path()).into_iter().flat_map(|f| read_conversations(&f, since, &HashSet::new()));
        rows.filter_map(|c| Some((transcript_file(&c.dir, &c.id)?, Session::key_for(Harness::Antigravity, &account.name, &c.id), c.last_modified))).collect()
    }

    /// What you asked, and what the agent replied.
    fn prose(&self, line: &[u8]) -> Option<(i64, bool, String)> {
        let has = |needle: &[u8]| memchr::memmem::find(line, needle).is_some();
        if !has(br#""type":"USER_INPUT""#) && !has(br#""type":"PLANNER_RESPONSE""#) {
            return None;
        }
        let v: Value = serde_json::from_slice(line).ok()?;
        let s = step(&v)?;
        let text = match (s.source, s.kind) {
            ("USER_EXPLICIT", "USER_INPUT") => request_of(s.content),
            ("MODEL", "PLANNER_RESPONSE") => s.content.trim(),
            _ => "",
        };
        (!text.is_empty()).then(|| (s.at, s.kind == "USER_INPUT", text.to_string()))
    }

    /// The models `agy models` lists, which its `--model` takes.
    fn models(&self, account: &AccountCfg) -> Vec<crate::start::ModelOption> {
        models(cli_command(account))
    }

    fn takes_model(&self) -> bool {
        true
    }

    /// `agy -i <prompt>` opens its terminal UI on a prompt (checked on 1.2.14); it names the
    /// conversation itself. Attachments go as paths, their folders added (`--add-dir`, once
    /// each) so it reads them without asking.
    fn start_in_terminal(&self, account: &AccountCfg, ask: &crate::harness::StartAsk) -> crate::harness::Typed {
        use crate::start::{FILES_VAR, IMAGES_VAR, PROMPT_VAR};
        let (has_images, has_files) = crate::attachments::kinds(ask.prompt);
        let prompt = crate::attachments::expand_in(ask.prompt, ask.images_dir, ask.files_dir, crate::attachments::Style::Path);
        let prompt = prompt.trim();
        let mut input = cli_command(account).to_string();
        let mut env = Vec::new();
        for (has, var, dir) in [(has_images, IMAGES_VAR, ask.images_dir), (has_files, FILES_VAR, ask.files_dir)] {
            if has {
                input.push_str(&format!(" --add-dir \"${var}\""));
                env.push((var.to_string(), dir.to_string_lossy().to_string()));
            }
        }
        if let Some(m) = ask.model {
            input.push_str(&format!(" --model '{m}'"));
        }
        if !prompt.is_empty() {
            input.push_str(&format!(" -i \"${PROMPT_VAR}\""));
            env.push((PROMPT_VAR.to_string(), prompt.to_string()));
        }
        input.push('\n');
        crate::harness::Typed { input, env, key: None }
    }

    fn session_cwds(&self, home: &Path, since: i64) -> Vec<String> {
        folders(home).iter().flat_map(|f| read_conversations(f, since, &HashSet::new())).filter_map(|c| c.cwd).collect()
    }

    /// The quotas are read through the command line (`read_usage`); without it they're only
    /// in the app.
    fn no_limits(&self) -> Option<&'static str> {
        (!cli_installed(&crate::util::home())).then_some("Antigravity shows its quotas in the app. With its command line, agy, installed, Codealong shows them here.")
    }

    fn polls_usage(&self) -> bool {
        cli_installed(&crate::util::home())
    }

    fn read_usage(&self, account: &AccountCfg, now: i64) -> crate::model::Usage {
        read_usage(account, now)
    }

    fn records(&self, account: &AccountCfg, s: &Session) -> Vec<PathBuf> {
        let dir = folder_of(account, s);
        transcript_file(&dir, &s.id).into_iter().chain([dir.join("conversations").join(format!("{}.db", s.id)), dir.join(SUMMARIES)]).collect()
    }

    fn conversation(&self, ctx: &Ctx, s: &Session, before: Option<u64>, limit: usize) -> anyhow::Result<Page> {
        crate::transcript::file_page(&self.record(ctx, s)?, conversation_items, before, limit)
    }

    fn conversation_after(&self, ctx: &Ctx, s: &Session, after: u64) -> anyhow::Result<Page> {
        crate::transcript::file_newer(&self.record(ctx, s)?, conversation_items, after)
    }

    /// The command line's conversation takes a prompt typed into its terminal, or a new terminal
    /// that resumes it; one a print run has open takes nothing until that ends. Nothing reaches
    /// the app's from outside: the app comes forward with the prompt on the clipboard.
    fn routes(&self, _account: &AccountCfg, s: &Session) -> Vec<crate::send::Route> {
        match (s.surface, s.pid, s.live) {
            (Surface::Cli, Some(pid), _) => vec![crate::send::Route::Type { pid }],
            (Surface::Cli, None, false) => vec![crate::send::Route::Resume],
            (Surface::Cli, None, true) => Vec::new(),
            _ => vec![OnClipboard.into()],
        }
    }

    /// `agy --conversation <id> -i <prompt>` goes on with a conversation in its terminal UI, on
    /// the prompt (checked on 1.2.14).
    /// On the model asked for, as a start is, and without a prompt it opens on the
    /// conversation and waits for one.
    fn resume(&self, account: &AccountCfg, s: &Session, ask: &crate::harness::StartAsk) -> anyhow::Result<(String, Vec<(String, String)>)> {
        let prompt = crate::attachments::expand(ask.prompt, crate::attachments::Style::Path).trim().to_string();
        let mut input = format!("{} --conversation {}", cli_command(account), s.id);
        if let Some(m) = ask.model {
            input.push_str(&format!(" --model '{m}'"));
        }
        let mut env = Vec::new();
        if !prompt.is_empty() {
            input.push_str(&format!(" -i \"${}\"", crate::start::PROMPT_VAR));
            env.push((crate::start::PROMPT_VAR.to_string(), prompt));
        }
        input.push('\n');
        Ok((input, env))
    }

    /// A terminal, or one behind your windows.
    fn resumes_in(&self, place: crate::start::Place) -> bool {
        use crate::start::Place;
        matches!(place, Place::Terminal | Place::Background)
    }

    /// What its step waits on you to allow. It's answered where the conversation runs: nothing
    /// outside takes an answer.
    fn pending(&self, ctx: &Ctx, s: &Session) -> Option<crate::harness::Pending> {
        let c = tail(&self.record(ctx, s).ok()?, None).open.into_iter().next()?;
        let cannot = format!("Antigravity asks in {}; answer it there.", if s.surface == Surface::Cli { "its terminal" } else { "its own window" });
        Some(crate::harness::Pending { id: c.result_step.to_string(), label: c.label, input: Some(c.input), cannot: Some(cannot), always: false, no_once: false, plan: false, message: false, always_rule: None, questions: Vec::new(), unwritten: false, terminal: None })
    }

    fn open_calls(&self, ctx: &Ctx, s: &Session) -> Vec<String> {
        self.record(ctx, s).map(|p| tail(&p, None).open.into_iter().map(|c| c.input).collect()).unwrap_or_default()
    }

    /// A conversation the command line starts with no workspace works in its folder's `scratch`.
    fn chats(&self) -> Option<(PathBuf, &'static str)> {
        Some((crate::util::home().join(".gemini").join(CLI_DIR).join(SCRATCH), "Antigravity chats"))
    }

    /// Its list of conversations, in each of its folders.
    fn databases(&self) -> &'static [&'static str] {
        &[SUMMARIES]
    }

    fn reader(&self, _wake: crate::harness::Wake) -> Box<dyn crate::harness::Reader> {
        Box::new(Reader::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCHEMA: &str = "CREATE TABLE conversation_summaries (conversation_id TEXT PRIMARY KEY, title TEXT NOT NULL DEFAULT '', preview TEXT NOT NULL DEFAULT '', \
         step_count INTEGER NOT NULL DEFAULT 0, last_modified_time DATETIME NOT NULL, workspace_uris TEXT NOT NULL, status TEXT NOT NULL DEFAULT '', \
         source TEXT NOT NULL DEFAULT '', project_id TEXT NOT NULL DEFAULT '', agent_name TEXT NOT NULL DEFAULT '', parent_conversation_id TEXT NOT NULL DEFAULT '', \
         nesting_depth INTEGER NOT NULL DEFAULT 0, not_fully_idle NUMERIC NOT NULL DEFAULT false, killed NUMERIC NOT NULL DEFAULT false, \
         last_user_input_time DATETIME NOT NULL, last_user_input_step_index INTEGER NOT NULL DEFAULT -1, app_data_dir TEXT NOT NULL DEFAULT '', raw_summary BLOB, group_id TEXT NOT NULL DEFAULT '');";

    #[test]
    fn reads_the_summary_table() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(&format!(
            "{SCHEMA}\
             INSERT INTO conversation_summaries (conversation_id, title, preview, step_count, last_modified_time, workspace_uris, status, not_fully_idle, last_user_input_time) VALUES \
             ('c1', 'Fix the build', 'Fix the build so tests pass', 12, '2026-09-24 14:20:07.434615-07:00', '[\"file:///Users/me/My%20Repo/\"]', 'CASCADE_RUN_STATUS_RUNNING', 1, '2026-09-24 14:16:07.000000-07:00'), \
             ('c2', 'Old one', '', 3, '2026-08-01 10:00:00+00:00', '[]', '', 0, '2026-08-01 09:00:00+00:00'), \
             ('c3', 'Nested', '', 3, '2026-09-24 14:20:07-07:00', '[]', '', 0, '2026-09-24 14:16:07-07:00');\
             UPDATE conversation_summaries SET parent_conversation_id = 'c1', nesting_depth = 1 WHERE conversation_id = 'c3';"
        ))
        .unwrap();
        let since = parse_rfc3339_ms("2026-09-01T00:00:00Z").unwrap();
        let rows = read_from(&db, "antigravity", since, &HashSet::new()).unwrap();
        assert_eq!(rows.len(), 1);
        let c = &rows[0];
        assert_eq!(c.id, "c1");
        assert_eq!(c.cwd.as_deref(), Some("/Users/me/My Repo"));
        assert_eq!(c.status, "cascade_run_status_running");
        assert!(c.not_fully_idle && c.running());
        assert_eq!(c.last_modified, parse_rfc3339_ms("2026-09-24T21:20:07.434Z").unwrap());
        assert_eq!(c.last_input, parse_rfc3339_ms("2026-09-24T21:16:07Z"));
        // A pinned one comes back however old.
        let keep: HashSet<String> = ["c2".to_string()].into();
        assert_eq!(read_from(&db, "antigravity", since, &keep).unwrap().len(), 2);
    }

    /// Each folder's table lists the other folders' conversations too (the command line's has
    /// the app's, without a status): a folder shows only its own.
    #[test]
    fn a_folder_shows_only_its_own_conversations() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(&format!(
            "{SCHEMA}\
             INSERT INTO conversation_summaries (conversation_id, last_modified_time, workspace_uris, status, last_user_input_time, app_data_dir) VALUES \
             ('app', '2026-09-27 15:21:54+00:00', '[]', '', '0001-01-01 00:00:00+00:00', 'antigravity'), \
             ('cli', '2026-10-01 16:31:04+00:00', '[]', 'CASCADE_RUN_STATUS_IDLE', '2026-10-01 16:31:02+00:00', 'antigravity-cli');"
        ))
        .unwrap();
        let ids = |dir: &str| read_from(&db, dir, 0, &HashSet::new()).unwrap().into_iter().map(|c| c.id).collect::<Vec<_>>();
        assert_eq!(ids("antigravity-cli"), ["cli"]);
        assert_eq!(ids("antigravity"), ["app"]);
        // gorm's zero time is no time.
        assert_eq!(read_from(&db, "antigravity", 0, &HashSet::new()).unwrap()[0].last_input, None);
    }

    #[test]
    fn reads_timestamps_and_booleans_however_stored() {
        assert_eq!(timestamp(&Sql::Text("2026-09-24 14:16:07.434615-07:00".into())), parse_rfc3339_ms("2026-09-24T21:16:07.434Z"));
        assert_eq!(timestamp(&Sql::Integer(1_790_285_194)), Some(1_790_285_194_000));
        assert_eq!(timestamp(&Sql::Integer(1_790_285_194_631)), Some(1_790_285_194_631));
        assert_eq!(timestamp(&Sql::Null), None);
        assert!(truthy(&Sql::Integer(1)) && truthy(&Sql::Text("true".into())));
        assert!(!truthy(&Sql::Integer(0)) && !truthy(&Sql::Text("false".into())) && !truthy(&Sql::Null));
        assert_eq!(uri_path("file:///Users/me/My%20Repo").as_deref(), Some("/Users/me/My Repo"));
        assert_eq!(uri_path("vscode-remote://ssh/x"), None);
    }

    #[test]
    fn the_command_lines_folder_goes_with_the_apps() {
        let both = folders(Path::new("/Users/me/.gemini/antigravity"));
        assert_eq!(both, [Folder { path: "/Users/me/.gemini/antigravity".into(), cli: false }, Folder { path: "/Users/me/.gemini/antigravity-cli".into(), cli: true }]);
        assert_eq!(folders(Path::new("/Users/me/.gemini/antigravity-cli")), [Folder { path: "/Users/me/.gemini/antigravity-cli".into(), cli: true }]);
        // A separately installed IDE keeps its own, and the command line's isn't read twice.
        assert_eq!(folders(Path::new("/Users/me/.gemini/antigravity-ide")).len(), 1);
    }

    // A conversation as `agy` 1.2.14 wrote it (transcript_full.jsonl), with two calls made
    // together, whose results are written before their own step.
    const LINES: &[&str] = &[
        r#"{"step_index":0,"source":"USER_EXPLICIT","type":"USER_INPUT","status":"DONE","created_at":"2026-10-01T16:55:10Z","content":"<USER_REQUEST>\nCount the lines, then fix notes.txt\n</USER_REQUEST>\n<ADDITIONAL_METADATA>\nThe current local time is: 2026-10-01T09:55:10-07:00.\n</ADDITIONAL_METADATA>"}"#,
        r#"{"step_index":1,"source":"MODEL","type":"PLANNER_RESPONSE","status":"DONE","created_at":"2026-10-01T16:55:14Z","content":"Counting first.","tool_calls":[{"name":"run_command","args":{"CommandLine":"wc -l a.txt","Cwd":"/Users/me/app","WaitMsBeforeAsync":5000,"toolAction":"Counting lines in a.txt","toolSummary":"Line count"}}]}"#,
        r#"{"step_index":2,"source":"MODEL","type":"GENERIC","status":"DONE","created_at":"2026-10-01T16:55:15Z","content":"Created At: 2026-10-01T09:55:14-07:00\nCompleted At: 2026-10-01T09:55:15-07:00\n\nThe command exited with code 0.\nOutput:\n       1 a.txt\r\n\n"}"#,
        r#"{"step_index":3,"source":"SYSTEM","type":"SYSTEM_MESSAGE","status":"DONE","created_at":"2026-10-01T16:55:15Z","content":"The following is a <SYSTEM_MESSAGE> not actually sent by the user."}"#,
        r#"{"step_index":5,"source":"MODEL","type":"GENERIC","status":"DONE","created_at":"2026-10-01T16:55:22Z","content":"Created At: 2026-10-01T09:55:22-07:00\nCompleted At: 2026-10-01T09:55:22-07:00\nThe following changes were made by the replace_file_content tool"}"#,
        r#"{"step_index":4,"source":"MODEL","type":"PLANNER_RESPONSE","status":"DONE","created_at":"2026-10-01T16:55:22Z","tool_calls":[{"name":"replace_file_content","args":{"TargetFile":"/Users/me/app-fix/notes.txt","ReplacementContent":"BETA","toolSummary":"Fix notes.txt"}},{"name":"run_command","args":{"CommandLine":"ls /definitely-not-here","Cwd":"/Users/me/app"}}]}"#,
        r#"{"step_index":6,"source":"MODEL","type":"GENERIC","status":"DONE","created_at":"2026-10-01T16:55:24Z","content":"Created At: 2026-10-01T09:55:24-07:00\nCompleted At: 2026-10-01T09:55:24-07:00\n\nThe command exited with code 1.\nOutput:\nls: /definitely-not-here: No such file or directory\n"}"#,
        r#"{"step_index":7,"source":"MODEL","type":"PLANNER_RESPONSE","status":"DONE","created_at":"2026-10-01T16:55:25Z","content":"Done: one line, and **notes.txt** is fixed."}"#,
    ];

    #[test]
    fn a_transcript_reads_as_prompts_replies_and_tools() {
        let items: Vec<Item> = LINES.iter().flat_map(|l| conversation_items(&serde_json::from_str(l).unwrap(), 0)).collect();
        let kinds: Vec<&str> = items.iter().map(|i| i.kind).collect();
        assert_eq!(kinds, ["you", "agent", "tool", "result", "result", "tool", "tool", "result", "agent"]);
        assert_eq!(items[0].text, "Count the lines, then fix notes.txt");
        assert_eq!(items[0].at, parse_rfc3339_ms("2026-10-01T16:55:10Z").unwrap());
        // A command can run again where it ran, and its output is what it printed.
        assert_eq!((items[2].text.as_str(), items[2].input.as_deref(), items[2].shell, items[2].cwd.as_deref()), ("Run command · `wc -l a.txt`", Some("wc -l a.txt"), true, Some("/Users/me/app")));
        assert_eq!((items[3].id.as_deref(), items[3].output.as_deref(), items[3].failed), (items[2].id.as_deref(), Some("       1 a.txt"), false));
        // Calls made together are answered by the steps after theirs, in their order.
        assert_eq!((items[5].text.as_str(), items[5].id.as_deref()), ("Replace file content · Fix notes.txt", Some("5")));
        assert!(items[5].input.as_deref().unwrap().contains("\"ReplacementContent\": \"BETA\"") && !items[5].input.as_deref().unwrap().contains("toolSummary"));
        assert_eq!((items[6].id.as_deref(), items[7].id.as_deref(), items[7].failed), (Some("6"), Some("6"), true));
        assert!(items[7].output.as_deref().unwrap().starts_with("The command exited with code 1."));
    }

    /// The transcript without `_full` writes each argument as a string of JSON.
    #[test]
    fn reads_arguments_written_as_json_strings() {
        let v = serde_json::json!({"step_index": 1, "source": "MODEL", "type": "PLANNER_RESPONSE", "status": "DONE", "created_at": "2026-10-01T16:31:49Z",
            "tool_calls": [{"name": "run_command", "args": {"CommandLine": "\"wc -l a.txt\"", "Cwd": "\"/Users/me/app\"", "WaitMsBeforeAsync": "5000"}}]});
        let items = conversation_items(&v, 0);
        assert_eq!((items[0].input.as_deref(), items[0].cwd.as_deref()), (Some("wc -l a.txt"), Some("/Users/me/app")));
    }

    /// An agent reads a repo it isn't working in with its file tools; only what it writes says
    /// where it works.
    #[test]
    fn looking_at_a_file_is_not_work_on_it() {
        let work = |name: &str, args: Value| call(&serde_json::json!({"name": name, "args": args}), 1).work;
        assert_eq!(work("view_file", serde_json::json!({"AbsolutePath": "/Users/me/app-fix/notes.txt"})), None);
        assert_eq!(work("list_dir", serde_json::json!({"DirectoryPath": "/Users/me/app-fix"})), None);
        assert_eq!(work("grep_search", serde_json::json!({"SearchPath": "/Users/me/app-fix", "Query": "beta"})), None);
        assert_eq!(work("write_to_file", serde_json::json!({"TargetFile": "/Users/me/app-fix/new.txt"})).as_deref(), Some("/Users/me/app-fix/new.txt"));
        // A view still says which file on its line.
        assert_eq!(call(&serde_json::json!({"name": "view_file", "args": {"AbsolutePath": "/Users/me/app-fix/notes.txt"}}), 1).label, "View file · `notes.txt`");
    }

    #[test]
    fn the_end_of_a_record_says_what_a_conversation_is_on() {
        let whole = tail_of(&LINES.join("\n"), Some("/Users/me/app"));
        assert_eq!(whole.last_text.as_deref(), Some("Done: one line, and notes.txt is fixed."));
        assert_eq!(whole.last_tool.as_deref(), Some("Run command · `ls /definitely-not-here`"));
        assert!(whole.open.is_empty(), "every call has its result");
        // Where it worked: commands in their folder, an edit in its file's.
        assert_eq!(whole.work, ["/Users/me/app", "/Users/me/app-fix/notes.txt"]);
        // Counting lines only looks, which is work only in the conversation's own folder.
        assert_eq!(tail_of(&LINES.join("\n"), Some("/Users/me/elsewhere")).work, ["/Users/me/app-fix/notes.txt"]);
        assert_eq!(tail_of(&LINES.join("\n"), None).work, ["/Users/me/app-fix/notes.txt"]);
        // Mid-turn, the call with no result yet is the one running, or waiting on you.
        let running = tail_of(&LINES[..6].join("\n"), None);
        assert_eq!(running.open.iter().map(|c| c.input.as_str()).collect::<Vec<_>>(), ["ls /definitely-not-here"]);
        assert_eq!(running.last_text.as_deref(), Some("Counting first."));
    }

    #[test]
    fn tells_the_terminal_ui_from_print_runs_and_other_commands() {
        assert_eq!(cli_args("/Users/me/.local/bin/agy"), Some(true));
        assert_eq!(cli_args("agy --model gemini-3.8-flash-low -i Run the tests models"), Some(true));
        assert_eq!(cli_args("agy --conversation 88db39ed-a2e3-4dd7-b9aa-0bf7bb66b35d"), Some(true));
        assert_eq!(cli_args("agy --model gemini-3.8-flash-low --input-format stream-json --output-format stream-json"), Some(false));
        assert_eq!(cli_args("agy --output-format=json -p Reply OK"), Some(false));
        assert_eq!(cli_args("agy models"), None);
        assert_eq!(cli_args("/Users/me/.local/bin/agy remote-control start"), None);
        assert_eq!(cli_args("/usr/bin/magyar"), None);
        assert!(is_app_agent("/Applications/Antigravity.app/Contents/Resources/bin/language_server --standalone --override_ide_name antigravity"));
        assert!(!is_app_agent("/Applications/Windsurf.app/Contents/Resources/bin/language_server --standalone"));
    }

    #[test]
    fn finds_the_conversation_a_process_has_open() {
        let files = "p61332\nfcwd\nn/Users/me/app\nf73\nn/Users/me/.gemini/antigravity-cli/conversations/0b86875e-96cc-4f1d-aebd-68814b6c553f.db\nf40\nn/Users/me/.gemini/antigravity-cli/presence\nf79\nn/Users/me/.gemini/antigravity-cli/presence/0b86875e-96cc-4f1d-aebd-68814b6c553f.lock\n";
        assert_eq!(held_conversation(files).as_deref(), Some("0b86875e-96cc-4f1d-aebd-68814b6c553f"));
        assert_eq!(held_conversation("p1\nfcwd\nn/Users/me/app\n"), None);
    }

    /// A folder read end to end: the summary table, a conversation's own database for the step
    /// that waits, and its transcript.
    #[test]
    fn reads_a_folder_as_the_command_line_leaves_it() {
        let root = crate::fixtures::scratch("antigravity");
        let dir = root.join("antigravity-cli");
        let logs = dir.join("brain/c1/.system_generated/logs");
        std::fs::create_dir_all(&logs).unwrap();
        std::fs::create_dir_all(dir.join("conversations")).unwrap();
        std::fs::write(logs.join("transcript_full.jsonl"), LINES[..6].join("\n") + "\n").unwrap();
        let db = Connection::open(dir.join(SUMMARIES)).unwrap();
        db.execute_batch(&format!(
            "{SCHEMA}\
             INSERT INTO conversation_summaries (conversation_id, preview, step_count, last_modified_time, workspace_uris, status, not_fully_idle, last_user_input_time, app_data_dir) VALUES \
             ('c1', 'Count the lines, then fix notes.txt', 6, '2026-10-01 16:55:24+00:00', '[\"file:///Users/me/app\"]', 'CASCADE_RUN_STATUS_RUNNING', 1, '2026-10-01 16:55:10+00:00', 'antigravity-cli');"
        ))
        .unwrap();
        drop(db);
        let steps = Connection::open(dir.join("conversations/c1.db")).unwrap();
        steps.execute_batch("CREATE TABLE steps (idx INTEGER PRIMARY KEY, step_type INTEGER NOT NULL DEFAULT 0, status INTEGER NOT NULL DEFAULT 0); INSERT INTO steps VALUES (0, 14, 3), (6, 132, 9);").unwrap();
        drop(steps);

        let account = AccountCfg { name: "antigravity".into(), harness: "antigravity".into(), home: root.join("antigravity").to_string_lossy().into_owned(), command: None, start: None, preferred: false, acp: None, monthly: None, per_token: false };
        let seen = crate::state::SeenState::default();
        let look = Look { seen: &seen, since: 0, now: parse_rfc3339_ms("2026-10-01T16:56:00Z").unwrap(), ps: Default::default() };
        let mut reader = Reader::default();
        assert!(reader.reread(&account, &look), "a conversation that's new may need a lane");
        assert!(!reader.reread(&account, &look));
        let c = &reader.conversations["antigravity"][0];
        assert!(c.cli && c.waiting && c.dir == dir);
        assert_eq!(c.tail.open[0].input, "ls /definitely-not-here");

        // The panel reads its record, and what it waits on is the call with no result.
        let session = Session { surface: Surface::Cli, ..crate::board::antigravity_session(&account, c, None, false, 0, look.now) };
        let ctx = Ctx { account: &account };
        use crate::harness::Adapter as _;
        let page = Antigravity.conversation(&ctx, &session, None, 50).unwrap();
        assert_eq!(page.items.iter().map(|i| i.kind).collect::<Vec<_>>(), ["you", "agent", "tool", "result", "result", "tool", "tool"]);
        let waits = Antigravity.pending(&ctx, &session).unwrap();
        assert_eq!((waits.label.as_str(), waits.id.as_str(), waits.cannot.is_some()), ("Run command · `ls /definitely-not-here`", "6", true));
        assert_eq!(Antigravity.open_calls(&ctx, &session), ["ls /definitely-not-here"]);
        assert_eq!(Antigravity.records(&account, &session)[0], logs.join("transcript_full.jsonl"));
    }

    #[test]
    fn starts_and_resumes_in_its_terminal_ui() {
        use crate::harness::Adapter as _;
        let a = AccountCfg { name: "antigravity".into(), harness: "antigravity".into(), home: "~/.gemini/antigravity".into(), command: None, start: None, preferred: false, acp: None, monthly: None, per_token: false };
        let ask = |prompt: &'static str, model: Option<&'static str>| crate::harness::StartAsk { prompt, session_id: "unused", model, effort: None, worktree: false, images_dir: Path::new("/i"), files_dir: Path::new("/f") };
        // With nothing to say it only opens; it names the conversation itself.
        let bare = Antigravity.start_in_terminal(&a, &ask("", None));
        assert_eq!((bare.input.as_str(), bare.env.len(), bare.key), ("agy\n", 0, None));
        // The prompt goes in the environment, so nothing in it reaches the shell.
        let typed = Antigravity.start_in_terminal(&a, &ask("  -v $(say nope) `x`  ", Some("gemini-3.8-flash-low")));
        assert_eq!(typed.input, format!("agy --model 'gemini-3.8-flash-low' -i \"${}\"\n", crate::start::PROMPT_VAR));
        assert_eq!(typed.env, vec![(crate::start::PROMPT_VAR.to_string(), "-v $(say nope) `x`".to_string())]);
        // An image goes as a path, with its folder added so it's read without asking.
        let image = Antigravity.start_in_terminal(&a, &ask("look\n![image](images/0123456789abcdef.png)", None));
        assert_eq!(image.input, format!("agy --add-dir \"${}\" -i \"${}\"\n", crate::start::IMAGES_VAR, crate::start::PROMPT_VAR));
        assert!(image.env.contains(&(crate::start::IMAGES_VAR.to_string(), "/i".to_string())) && image.env.iter().any(|(k, v)| k == crate::start::PROMPT_VAR && v.contains("/i/0123456789abcdef.png")));
        // An account's own command starts and resumes it.
        let own = AccountCfg { command: Some("agy --sandbox".into()), ..a.clone() };
        assert_eq!(Antigravity.start_in_terminal(&own, &ask("", None)).input, "agy --sandbox\n");
        let conversation = Session::new(Harness::Antigravity, &own.name, "88db39ed");
        let (input, env) = Antigravity.resume(&own, &conversation, &ask("-v please", None)).unwrap();
        assert_eq!(input, format!("agy --sandbox --conversation 88db39ed -i \"${}\"\n", crate::start::PROMPT_VAR));
        assert_eq!(env, vec![(crate::start::PROMPT_VAR.to_string(), "-v please".to_string())]);
        // On another model, or with nothing to say yet: it opens on the conversation.
        assert_eq!(Antigravity.resume(&own, &conversation, &ask("", Some("gemini-3.8-flash-low"))).unwrap(), ("agy --sandbox --conversation 88db39ed --model 'gemini-3.8-flash-low'\n".to_string(), Vec::new()));
    }

    #[test]
    fn reads_the_models_the_command_line_lists() {
        let out = "Fetching available models...\ngemini-3.8-flash-low\tGemini 3.8 Flash (Low)\nclaude-sonnet-4-6\tClaude Sonnet 4.6 (Thinking)\nnot a model line\nbad id;rm\tNope\n";
        let listed: Vec<(String, String)> = listed_models(out).into_iter().map(|m| (m.id, m.label)).collect();
        assert_eq!(listed, [("gemini-3.8-flash-low".to_string(), "Gemini 3.8 Flash (Low)".to_string()), ("claude-sonnet-4-6".to_string(), "Claude Sonnet 4.6 (Thinking)".to_string())]);
    }

    #[test]
    fn reads_the_limits_usage_answers() {
        use crate::model::Severity;
        // `agy -p /usage --output-format json` on 1.3.4 (2026-10-13), a weekly limit per group.
        let out = r#"{"conversation_id":"","status":"SUCCESS","response":"Gemini Models\tWeekly Limit Remaining\t97%\t2026-10-20T21:48:07Z\n","duration_seconds":0,"num_turns":0,"usage":{"input_tokens":0,"output_tokens":0,"thinking_tokens":0,"cache_read_tokens":0,"total_tokens":0},"command":{"name":"usage","data":{"description":"Within each group, models share a weekly limit.","groups":[{"name":"Gemini Models","description":"Models within this group: Gemini Flash, Gemini Pro","buckets":[{"id":"gemini-weekly","name":"Weekly Limit Remaining","window":"weekly","remaining_fraction":0.08,"reset_time":"2026-10-20T21:48:07Z"}]},{"name":"Claude and GPT models","description":"Models within this group: Claude Opus, Claude Sonnet, GPT-OSS","buckets":[{"id":"3p-weekly","name":"Weekly Limit Remaining","window":"weekly","remaining_fraction":1,"reset_time":"2026-10-20T22:14:23Z"}]}]}}}"#;
        let UsageAnswer::Limits(meters) = usage_answer(&format!("Checking for updates...\n{out}\n")) else { panic!("limits") };
        let shown: Vec<(&str, &str, &str, i64, Option<i64>, Severity, Option<&str>)> = meters.iter().map(|m| (m.key.as_str(), m.label.as_str(), m.window.as_str(), m.percent.round() as i64, m.resets_at, m.severity, m.model.as_deref())).collect();
        assert_eq!(
            shown,
            [
                ("model:Gemini", "Gemini", "Weekly, Gemini models", 92, parse_rfc3339_ms("2026-10-20T21:48:07Z"), Severity::Critical, Some("Gemini")),
                // Untouched: its reset is only a week from the asking, so it shows none.
                ("model:Claude & GPT", "Claude & GPT", "Weekly, Claude & GPT models", 0, None, Severity::Normal, Some("Claude & GPT")),
            ]
        );
        // Signed out and nobody there to sign in, it says so once it's ended.
        assert_eq!(usage_answer(r#"{"conversation_id":"","status":"ERROR","response":"","error":"authentication failed or timed out","num_turns":0}"#), UsageAnswer::SignedOut);
        assert_eq!(usage_answer(r#"{"status":"ERROR","error":"quota service unavailable"}"#), UsageAnswer::Failed("quota service unavailable".into()));
        assert_eq!(usage_answer("Authentication required. Please visit the URL to log in:"), UsageAnswer::Failed("unexpected response".into()));
        assert_eq!(group_label("Gemini Models"), "Gemini");
        assert_eq!(group_label("Pro MODELS "), "Pro");
        assert_eq!(group_label("Models"), "Models");
    }

    #[test]
    fn a_prompt_reaches_a_conversation_where_it_runs() {
        use crate::harness::Adapter as _;
        use crate::send::Route;
        let a = AccountCfg { name: "antigravity".into(), harness: "antigravity".into(), home: "~/.gemini/antigravity".into(), command: None, start: None, preferred: false, acp: None, monthly: None, per_token: false };
        let c = Conversation { id: "c1".into(), cli: true, ..Default::default() };
        let ui = Process { pid: 41, conversation: Some("c1".into()), terminal: true };
        let session = |c: &Conversation, p: Option<&Process>| crate::board::antigravity_session(&a, c, p, true, 0, 0);
        // Typed into its terminal; a new terminal resumes one that isn't running; a print run
        // has it until it ends.
        assert_eq!(Antigravity.routes(&a, &session(&c, Some(&ui))), [Route::Type { pid: 41 }]);
        assert_eq!(Antigravity.routes(&a, &session(&c, None)), [Route::Resume]);
        assert!(Antigravity.routes(&a, &session(&c, Some(&Process { terminal: false, ..ui.clone() }))).is_empty());
        // Nothing reaches the app's from outside.
        assert_eq!(Antigravity.routes(&a, &session(&Conversation { cli: false, ..c.clone() }, None)), [OnClipboard.into()]);
    }

    /// Runs a real print run of `agy` in a scratch folder and reads it as the engine does while
    /// it works and once it's done. It needs `agy` installed and signed in, and spends a little
    /// of its quota. Opt in: `cargo test reads_a_live_run -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn reads_a_live_run() {
        use crate::harness::Reader as _;
        use crate::model::Status;
        let name = format!("codealong-agy-live-{}", std::process::id());
        let folder = std::env::temp_dir().join(&name);
        std::fs::create_dir_all(&folder).unwrap();
        let account = AccountCfg { name: "antigravity".into(), harness: "antigravity".into(), home: "~/.gemini/antigravity".into(), command: None, start: None, preferred: false, acp: None, monthly: None, per_token: false };
        let accounts = std::slice::from_ref(&account);
        let seen = crate::state::SeenState::default();
        let look = || {
            let now = crate::util::now_ms();
            Look { seen: &seen, since: now - crate::util::DAY_MS, now, ps: Default::default() }
        };
        let mut reader = Reader::default();
        reader.scan(accounts, &look());
        let mut child = std::process::Command::new(crate::util::home().join(".local/bin").join(CLI))
            .args(["-p", "Run the shell command 'sleep 6' with your command tool, then reply with the single word DONE.", "--model", "gemini-3.8-flash-low", "--output-format", "json", "--dangerously-skip-permissions"])
            .current_dir(&folder)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("agy starts");
        let mine = |r: &Reader, look: &Look| r.sessions(&account, look).into_iter().find(|s| s.cwd.contains(&name));
        let (mut worked, mut ticks, started) = (None, 0, Instant::now());
        let done = loop {
            assert!(started.elapsed() < Duration::from_secs(120), "the run never showed as done");
            std::thread::sleep(Duration::from_millis(400));
            let l = look();
            // The engine glances every second or two, and ticks every ten: here, every two.
            reader.glance(accounts, &l);
            ticks += 1;
            if ticks % 5 == 0 {
                reader.tick(accounts, &l);
            }
            let exited = child.try_wait().unwrap().is_some();
            match mine(&reader, &l) {
                Some(s) if s.status == Status::Working => {
                    println!("working: live {} pid {:?} · {:?}", s.live, s.pid, s.excerpt);
                    worked = Some(s);
                }
                Some(s) if exited && worked.is_some() => {
                    // Its process is gone: the next look at processes says so.
                    reader.tick(accounts, &look());
                    break mine(&reader, &look()).unwrap_or(s);
                }
                _ => {}
            }
        };
        let worked = worked.expect("it showed as working while it ran");
        assert!(worked.live && worked.pid.is_none() && worked.surface == Surface::Cli, "a print run is live with no terminal to show");
        assert!(worked.excerpt.as_deref().is_some_and(|e| e.starts_with("Run command")), "{:?}", worked.excerpt);
        println!("done: {:?} live {} · {:?} · resume with {:?}", done.status, done.live, done.excerpt, done.resume_command);
        assert_eq!((done.status, done.live), (Status::Unread, false));
        assert!(done.excerpt.as_deref().is_some_and(|e| e.contains("DONE")), "{:?}", done.excerpt);
        // Its panel reads the whole of it.
        use crate::harness::Adapter as _;
        let page = Antigravity.conversation(&Ctx { account: &account }, &done, None, 50).unwrap();
        let kinds: Vec<&str> = page.items.iter().map(|i| i.kind).collect();
        println!("{kinds:?}");
        assert!(kinds.first() == Some(&"you") && kinds.contains(&"tool") && kinds.contains(&"result") && kinds.last() == Some(&"agent"));
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn search_reads_what_was_asked_and_replied() {
        use crate::harness::Adapter as _;
        let said: Vec<(bool, String)> = LINES.iter().filter_map(|l| Antigravity.prose(l.as_bytes())).map(|(_, you, text)| (you, text)).collect();
        // Your request without what Antigravity wraps it in, and the replies; no calls, results or notes.
        assert_eq!(said, [(true, "Count the lines, then fix notes.txt".to_string()), (false, "Counting first.".to_string()), (false, "Done: one line, and **notes.txt** is fixed.".to_string())]);
        assert_eq!(Antigravity.prose(LINES[0].as_bytes()).unwrap().0, parse_rfc3339_ms("2026-10-01T16:55:10Z").unwrap());
    }

    /// A record longer than what's read of it, where each cut (the start's and the end's) falls
    /// inside a two-byte character in one of the two files: only the line it fell in is lost.
    #[test]
    fn a_cut_inside_a_character_costs_only_its_line() {
        let dir = crate::fixtures::scratch("agy-cut");
        for lead in ["", "x"] {
            let path = dir.join(format!("transcript{}.jsonl", lead.len()));
            let note = format!(r#"{{"step_index":1,"source":"SYSTEM","type":"SYSTEM_MESSAGE","status":"DONE","created_at":"2026-10-01T16:55:15Z","content":"{lead}{}"}}"#, "é".repeat(200_000));
            std::fs::write(&path, format!("{}\n{note}\n{}\n", LINES[0], LINES[7])).unwrap();
            assert!(std::fs::metadata(&path).unwrap().len() > TAIL + 100_000);
            assert_eq!(first_request(&path).map(|(asked, _)| asked).as_deref(), Some("Count the lines, then fix notes.txt"), "lead {lead:?}");
            assert_eq!(tail(&path, None).last_text.as_deref(), Some("Done: one line, and notes.txt is fixed."), "lead {lead:?}");
        }
    }
}
