//! Cursor (3.x): the app's agent chats and its command line's, `cursor-agent`, as one account
//! kept in `~/.cursor`. The app keeps every chat in its global state database
//! (`~/Library/Application Support/Cursor/User/globalStorage/state.vscdb`, SQLite in WAL mode):
//! `composerHeaders` lists them, and its key-value table, `cursorDiskKV`, holds each one's
//! `composerData:<id>` (its status, and the order of its messages) and its messages,
//! `bubbleId:<chat>:<message>`. The command line keeps a chat in
//! `chats/<md5 of its folder>/<id>/store.db` (its messages as protobufs) and writes the same
//! transcript of JSON lines the app does,
//! `projects/<folder as a slug>/agent-transcripts/<id>/<id>.jsonl`, which is what one of its
//! chats' panel reads. Neither lists the other's chats. See .claude/rules/cursor.md.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use rusqlite::Connection;
use serde_json::Value;

use crate::config::AccountCfg;
use crate::harness::{Applied, Ctx, Look};
use crate::model::{Harness, Session, Status, Surface};
use crate::procs;
use crate::transcript::{clip_input, clip_output, item, kept_whole, placed, Item, Page};
use crate::util::{code, command_dir, file_name, note_work, one_line, only_looks, parse_rfc3339_ms};

/// The command line as its installer names it (it also installs it as `agent`).
pub const CLI: &str = "cursor-agent";
/// The app's bundle ID.
pub const APP_BUNDLE_ID: &str = "com.todesktop.230313mzl4w4u92";
/// The app's global state database, under its data folder.
const STATE_DB: &str = "state.vscdb";
/// A command line chat's own database.
const STORE_DB: &str = "store.db";
/// How many of a chat's latest messages are read for what it's on.
const TAIL_MESSAGES: usize = 60;
/// How much of a transcript's end is read for what a command line chat is on.
const TAIL_BYTES: u64 = 256 * 1024;

/// The app's global state database for the account kept in `home`. Only the account in
/// `~/.cursor` has the app's chats: one in a folder of its own (`CURSOR_CONFIG_DIR`) is the
/// command line's alone. The app's data folder is in the Library of the home `~/.cursor` is in.
pub fn app_db(home: &Path) -> Option<PathBuf> {
    if file_name(home) != ".cursor" {
        return None;
    }
    Some(home.parent()?.join("Library/Application Support/Cursor/User/globalStorage").join(STATE_DB))
}

/// The command that runs an account's command line: its own, when config.toml gives one.
pub fn cli_command(account: &AccountCfg) -> &str {
    account.command.as_deref().unwrap_or(CLI)
}

/// Whether `cursor-agent` is installed for the user whose home is `home`: where its installer
/// puts it, or, for you, anywhere on your PATH.
pub fn cli_installed(home: &Path) -> bool {
    crate::detect::is_executable(&home.join(".local/bin").join(CLI)) || (home == crate::util::home() && crate::detect::on_user_path(CLI))
}

/// A chat, the app's or the command line's.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Chat {
    pub id: String,
    pub title: Option<String>,
    /// The folder it works in: a worktree the app made for it, else its window's folder.
    pub cwd: Option<String>,
    pub created_at: i64,
    pub last_updated: i64,
    /// Archived in Cursor.
    pub archived: bool,
    /// The app's word for where it stands (`composerData.status`): `none`, `generating`,
    /// `completed` or `aborted`. Empty for the command line's.
    pub status: String,
    /// Something in it waits on you: a call to allow, a question (`hasBlockingPendingActions`).
    pub blocking: bool,
    pub branch: Option<String>,
    pub model: Option<String>,
    /// How many messages it has.
    pub messages: i64,
    /// How long its composerData is: the app rewrites it with every message, so a change says
    /// to read it again, which a length says without parsing it.
    pub written: i64,
    /// It's the command line's: a `cursor-agent` process runs it, in a terminal or on its own.
    pub cli: bool,
    /// The command line's chat folder (`chats/<hash>/<id>`), and its transcript.
    pub dir: Option<PathBuf>,
    pub transcript: Option<PathBuf>,
    /// What its latest messages say.
    pub tail: Tail,
}

impl Chat {
    /// The app says a turn of it is under way.
    pub fn generating(&self) -> bool {
        self.status == "generating"
    }
}

/// What a chat's latest messages say.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Tail {
    /// The latest reply's start, and its end, for Jev.
    pub last_text: Option<String>,
    pub last_text_tail: Option<String>,
    /// The latest tool call, as a line.
    pub last_tool: Option<String>,
    /// The calls since your last prompt that haven't finished: what runs now, or waits on you.
    pub open: Vec<Call>,
    /// Where its latest calls worked, oldest first.
    pub work: Vec<String>,
    /// What you first asked it, and when you last did.
    pub first_prompt: Option<String>,
    pub last_prompt_at: Option<i64>,
    /// When its latest message was written: a reply goes on long after the prompt the app's
    /// `lastUpdatedAt` is the time of.
    pub last_at: Option<i64>,
    /// Its latest message says the turn ended on an error: what it said.
    pub error: Option<String>,
    /// Its latest turn hasn't ended: for the command line's, whose record says when one does.
    pub turn_open: bool,
}

/* ---- The app's chats ---- */

fn flag(v: &Value, key: &str) -> bool {
    match v.get(key) {
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_i64().is_some_and(|n| n != 0),
        _ => false,
    }
}

/// The app's chats changed since `since_ms`, plus any in `keep` (pinned ones), newest first,
/// without what their messages say (`read_state`, `app_tail`). Subagents' chats are left out,
/// and so are the empty ones every new window makes (no name, nothing written) and drafts.
pub fn read_app_chats(db: &Connection, since_ms: i64, keep: &HashSet<String>) -> rusqlite::Result<Vec<Chat>> {
    let mut stmt = db.prepare("SELECT composerId, createdAt, lastUpdatedAt, isArchived, CAST(value AS TEXT), checkpointAt FROM composerHeaders WHERE isSubagent = 0")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<i64>>(1)?, r.get::<_, Option<i64>>(2)?, r.get::<_, Option<i64>>(3)?, r.get::<_, Option<String>>(4)?, r.get::<_, Option<i64>>(5)?)))?;
    let mut out = Vec::new();
    for (id, created, updated, archived, value, checkpoint) in rows.flatten() {
        let v: Value = value.as_deref().and_then(|t| serde_json::from_str(t).ok()).unwrap_or(Value::Null);
        let name = v.get("name").and_then(Value::as_str).map(str::trim).filter(|n| !n.is_empty()).map(str::to_string);
        if flag(&v, "isDraft") || (name.is_none() && updated.is_none()) {
            continue;
        }
        let created_at = created.unwrap_or(0);
        // `lastUpdatedAt` is when you last sent it something; a checkpoint is written as a turn
        // goes on, and the latest message's time comes with its tail (`Tail::last_at`).
        let last_updated = updated.unwrap_or(created_at).max(checkpoint.unwrap_or(0));
        if last_updated < since_ms && !keep.contains(&id) {
            continue;
        }
        let cwd = v.pointer("/workspaceIdentifier/uri/fsPath").and_then(Value::as_str).map(|p| p.trim_end_matches('/').to_string()).filter(|p| !p.is_empty());
        // The branch it last worked on, of the repos it tracked.
        let branches = v.get("trackedGitRepos").and_then(Value::as_array).into_iter().flatten().flat_map(|r| r.get("branches").and_then(Value::as_array).into_iter().flatten());
        let branch = branches.max_by_key(|b| b.get("lastInteractionAt").and_then(Value::as_i64).unwrap_or(0)).and_then(|b| b.get("branchName")?.as_str()).map(str::to_string);
        out.push(Chat { id, title: name, cwd, created_at, last_updated, archived: archived.unwrap_or(0) != 0, blocking: flag(&v, "hasBlockingPendingActions"), branch, ..Chat::default() });
    }
    out.sort_by(|a, b| b.last_updated.cmp(&a.last_updated));
    Ok(out)
}

/// What a chat's composerData says now. The app writes it with each message, where its header
/// can lag a minute behind; it's read only once it has been written since (`written`).
fn read_state(db: &Connection, c: &mut Chat) {
    let got = db.query_row(
        "SELECT json_extract(t, '$.status'), json_extract(t, '$.lastUpdatedAt'), json_extract(t, '$.name'), json_extract(t, '$.hasBlockingPendingActions'), \
         json_extract(t, '$.gitWorktree.worktreePath'), json_extract(t, '$.gitWorktree.branchName'), json_extract(t, '$.modelConfig.modelName'), \
         json_array_length(t, '$.fullConversationHeadersOnly'), length(t) FROM (SELECT CAST(value AS TEXT) AS t FROM cursorDiskKV WHERE key = ?1)",
        [format!("composerData:{}", c.id)],
        |r| {
            Ok((
                r.get::<_, Option<String>>(0)?,
                r.get::<_, Option<i64>>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<i64>>(3)?,
                r.get::<_, Option<String>>(4)?,
                r.get::<_, Option<String>>(5)?,
                r.get::<_, Option<String>>(6)?,
                r.get::<_, Option<i64>>(7)?,
                r.get::<_, Option<i64>>(8)?,
            ))
        },
    );
    let Ok((status, updated, name, blocking, worktree, branch, model, messages, written)) = got else { return };
    c.status = status.unwrap_or_default();
    c.last_updated = c.last_updated.max(updated.unwrap_or(0));
    if let Some(name) = name.map(|n| n.trim().to_string()).filter(|n| !n.is_empty()) {
        c.title = Some(name);
    }
    // The header's word may be a minute old, and an aborted chat's is often left set.
    c.blocking = blocking.is_some_and(|b| b != 0) || (c.blocking && c.generating());
    if let Some(w) = worktree.filter(|w| !w.is_empty()) {
        c.cwd = Some(w.trim_end_matches('/').to_string());
    }
    c.branch = branch.or(c.branch.take());
    // "default" is Auto, the app's own pick each turn.
    c.model = model.filter(|m| !m.is_empty() && m != "default");
    c.messages = messages.unwrap_or(0);
    c.written = written.unwrap_or(0);
}

/// How long each chat's composerData is, by chat: cheap to ask, as nothing is parsed.
fn written(db: &Connection, ids: &[&str]) -> HashMap<String, i64> {
    if ids.is_empty() {
        return HashMap::new();
    }
    let keys: Vec<String> = ids.iter().map(|id| format!("composerData:{id}")).collect();
    let marks = vec!["?"; keys.len()].join(",");
    let Ok(mut stmt) = db.prepare(&format!("SELECT key, length(CAST(value AS TEXT)) FROM cursorDiskKV WHERE key IN ({marks})")) else { return HashMap::new() };
    let rows = stmt.query_map(rusqlite::params_from_iter(keys.iter()), |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)));
    rows.map(|rows| rows.flatten().filter_map(|(k, n)| Some((k.strip_prefix("composerData:")?.to_string(), n))).collect()).unwrap_or_default()
}

/// The ids of a chat's messages, in order, with whether each is yours.
fn message_ids(db: &Connection, chat: &str) -> Vec<(String, bool)> {
    let Ok(mut stmt) = db.prepare(
        "SELECT json_extract(j.value, '$.bubbleId'), json_extract(j.value, '$.type') FROM cursorDiskKV, json_each(CAST(cursorDiskKV.value AS TEXT), '$.fullConversationHeadersOnly') AS j \
         WHERE cursorDiskKV.key = ?1 ORDER BY j.key",
    ) else {
        return Vec::new();
    };
    let rows = stmt.query_map([format!("composerData:{chat}")], |r| Ok((r.get::<_, Option<String>>(0)?, r.get::<_, Option<i64>>(1)?)));
    rows.map(|rows| rows.flatten().filter_map(|(id, kind)| Some((id?, kind == Some(1)))).collect()).unwrap_or_default()
}

/// A chat's messages by their ids, as JSON. One that isn't there is left out.
fn messages(db: &Connection, chat: &str, ids: &[(String, bool)]) -> HashMap<String, Value> {
    if ids.is_empty() {
        return HashMap::new();
    }
    let prefix = format!("bubbleId:{chat}:");
    let keys: Vec<String> = ids.iter().map(|(id, _)| format!("{prefix}{id}")).collect();
    let marks = vec!["?"; keys.len()].join(",");
    let Ok(mut stmt) = db.prepare(&format!("SELECT key, CAST(value AS TEXT) FROM cursorDiskKV WHERE key IN ({marks})")) else { return HashMap::new() };
    let rows = stmt.query_map(rusqlite::params_from_iter(keys.iter()), |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?)));
    rows.map(|rows| rows.flatten().filter_map(|(k, v)| Some((k.strip_prefix(&prefix)?.to_string(), serde_json::from_str(&v?).ok()?))).collect()).unwrap_or_default()
}

/// Those messages in the order asked.
fn in_order(db: &Connection, chat: &str, ids: &[(String, bool)]) -> Vec<Value> {
    let mut found = messages(db, chat, ids);
    ids.iter().filter_map(|(id, _)| found.remove(id)).collect()
}

/// What the end of an app chat's messages says, in its folder, where a command that only looks
/// still counts as work.
fn app_tail(db: &Connection, c: &Chat) -> Tail {
    let ids = message_ids(db, &c.id);
    let first = ids.iter().find(|(_, you)| *you).cloned();
    let latest = &ids[ids.len().saturating_sub(TAIL_MESSAGES)..];
    let mut tail = tail_of(&in_order(db, &c.id, latest).iter().filter_map(bubble).collect::<Vec<_>>(), c.cwd.as_deref());
    // The first prompt is long gone from the end of a long chat.
    if let Some(f) = first.filter(|f| !latest.contains(f)) {
        tail.first_prompt = in_order(db, &c.id, &[f]).iter().filter_map(bubble).find(|b| b.you).map(|b| one_line(&b.text, 140)).filter(|t| !t.is_empty());
    }
    tail
}

/// Reads an app chat's composerData and the end of its messages again, and when it last wrote.
fn refresh(db: &Connection, c: &mut Chat) {
    read_state(db, c);
    c.tail = app_tail(db, c);
    c.last_updated = c.last_updated.max(c.tail.last_at.unwrap_or(0));
}

/// One of a chat's messages: yours, the agent's words, or one of its tool calls, each a message
/// of its own.
#[derive(Debug, Clone, PartialEq)]
struct Bubble {
    you: bool,
    at: i64,
    text: String,
    call: Option<Call>,
    /// The turn ended on an error, in its words.
    error: Option<String>,
}

fn bubble(v: &Value) -> Option<Bubble> {
    let kind = v.get("type").and_then(Value::as_i64)?;
    let at = v.get("createdAt").and_then(|t| t.as_str().and_then(parse_rfc3339_ms).or_else(|| t.as_i64())).unwrap_or(0);
    let text = v.get("text").and_then(Value::as_str).unwrap_or("").trim().to_string();
    let error = v.get("errorDetails").filter(|e| e.is_object()).map(|e| {
        let said = |k: &str| e.get(k).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty());
        match (said("title"), said("message")) {
            (Some(t), Some(m)) => format!("{t}: {m}"),
            (Some(t), None) => t.to_string(),
            (None, Some(m)) => m.to_string(),
            (None, None) => "The turn stopped with an error".to_string(),
        }
    });
    Some(Bubble { you: kind == 1, at, text, call: v.get("toolFormerData").filter(|t| t.is_object()).map(call), error })
}

/// A tool call of the agent's.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Call {
    pub id: String,
    /// The call as a line, e.g. "Run · `cargo test`".
    pub label: String,
    /// All of it: the command, or the arguments.
    pub input: String,
    pub command: Option<String>,
    pub cwd: Option<String>,
    /// Where it works: a command's folder, or the file it writes. A read or a search works
    /// nowhere, as an agent's reads of a repo it isn't working in don't.
    pub work: Option<String>,
    /// Its command only looks (`util::only_looks`), which is work only in the chat's own folder.
    pub looks: bool,
    /// It hasn't finished: it runs, or waits on you.
    pub open: bool,
    /// It waits on you to allow it, or to answer it.
    pub waits: bool,
    pub output: Option<String>,
    pub failed: bool,
}

/// Arguments the app keeps as a string of JSON, or as they are.
fn args_of(t: &Value, key: &str) -> Value {
    match t.get(key) {
        Some(Value::String(s)) => serde_json::from_str(s).unwrap_or(Value::Null),
        Some(v) => v.clone(),
        None => Value::Null,
    }
}

/// A tool's name as a line says it: `run_terminal_command_v2` is "Run", and one it has no word
/// for is its own name in words.
fn tool_word(name: &str) -> String {
    let base = name.strip_suffix("_v2").unwrap_or(name);
    let word = match base {
        "run_terminal_command" => "Run",
        "edit_file" | "search_replace" | "write" => "Edit",
        "read_file" => "Read",
        "delete_file" => "Delete",
        "ripgrep_raw_search" | "grep" | "grep_search" => "Search",
        "glob_file_search" | "file_search" => "Find files",
        "semantic_search_full" | "codebase_search" => "Search the codebase",
        "list_dir" => "List",
        "web_search" => "Search the web",
        "web_fetch" => "Fetch",
        "read_lints" => "Read lints",
        "todo_write" => "Update todos",
        "task" => "Subagent",
        "ask_question" => "Ask",
        "create_plan" => "Plan",
        "switch_mode" => "Switch mode",
        other => {
            let said = other.replace('_', " ");
            let mut chars = said.chars();
            return chars.next().map(|f| f.to_uppercase().collect::<String>() + chars.as_str()).unwrap_or_else(|| "Tool".into());
        }
    };
    word.to_string()
}

/// A call as the app keeps it (`toolFormerData`): its `params` and `result` strings of JSON, its
/// `status`, your `userDecision`, and `additionalData`, which says when it waits on you.
fn call(t: &Value) -> Call {
    let text = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
    let name = t.get("name").and_then(Value::as_str).unwrap_or("tool");
    let base = name.strip_suffix("_v2").unwrap_or(name);
    let raw = args_of(t, "rawArgs");
    let params = match args_of(t, "params") {
        Value::Null => raw.clone(),
        p => p,
    };
    let result = args_of(t, "result");
    let status = t.get("status").and_then(Value::as_str).unwrap_or("");
    let review = t.pointer("/additionalData/reviewData/status").and_then(Value::as_str).unwrap_or("");
    let asking = t.pointer("/additionalData/status").and_then(Value::as_str) == Some("pending");
    let command = (base == "run_terminal_command").then(|| text(&params, "command")).flatten();
    let cwd = text(&params, "cwd").filter(|c| c.starts_with('/'));
    let file = ["targetFile", "relativeWorkspacePath", "path", "targetDirectory"].iter().find_map(|k| text(&params, k)).filter(|p| p.starts_with('/'));
    // An MCP server's tool is `mcp-<server>-<tool>`, and both may have dashes in them.
    let word = match (name.strip_prefix("mcp-"), text(&raw, "providerIdentifier"), text(&raw, "toolName")) {
        (Some(_), Some(server), Some(tool)) => format!("{server} · {tool}"),
        (Some(rest), _, _) if rest.trim_matches('-').is_empty() => "MCP tool".to_string(),
        (Some(rest), _, _) => rest.to_string(),
        (None, _, _) => tool_word(name),
    };
    let label = match (&command, base) {
        (Some(c), _) => format!("{word} · {}", code(&one_line(c, 80))),
        (None, "ask_question" | "create_plan") => match text(&params, "title") {
            Some(title) => format!("{word} · {}", one_line(&title, 80)),
            None => word,
        },
        (None, _) => {
            let what = ["pattern", "globPattern", "query", "searchTerm", "url", "description", "toModeId"].iter().find_map(|k| text(&params, k));
            match (&file, what) {
                (Some(f), _) if matches!(base, "edit_file" | "read_file" | "delete_file" | "search_replace" | "write") => format!("{word} · {}", code(&file_name(Path::new(f)))),
                (_, Some(w)) => format!("{word} · {}", one_line(&w, 80)),
                (Some(f), None) => format!("{word} · {}", code(&file_name(Path::new(f)))),
                (None, None) => word,
            }
        }
    };
    let input = command.clone().unwrap_or_else(|| match &params {
        Value::Null => String::new(),
        p => serde_json::to_string_pretty(p).unwrap_or_default(),
    });
    let output = match base {
        "run_terminal_command" => text(&result, "output"),
        "read_file" => text(&result, "contents"),
        "web_fetch" => text(&result, "markdown"),
        _ => None,
    };
    let rejected = t.get("userDecision").and_then(Value::as_str) == Some("rejected") || result.get("rejected").and_then(Value::as_bool) == Some(true);
    let work = match (&command, &cwd) {
        (Some(c), Some(dir)) => command_dir(c, dir),
        _ => file.clone().filter(|_| matches!(base, "edit_file" | "delete_file" | "search_replace" | "write")),
    };
    let open = matches!(status, "loading" | "pending" | "running");
    Call {
        id: text(t, "toolCallId").unwrap_or_default(),
        label,
        input,
        looks: command.as_deref().is_some_and(only_looks),
        command,
        cwd,
        work,
        open,
        waits: open && (review == "Requested" || asking),
        output,
        failed: matches!(status, "error" | "cancelled") || rejected,
    }
}

/// What a run of messages, oldest first, says of where the chat stands, in its folder `own`.
fn tail_of(bubbles: &[Bubble], own: Option<&str>) -> Tail {
    let mut out = Tail::default();
    for b in bubbles {
        out.last_at = out.last_at.max(Some(b.at).filter(|t| *t > 0));
        if b.you {
            out.last_prompt_at = Some(b.at).filter(|t| *t > 0).or(out.last_prompt_at);
            if out.first_prompt.is_none() {
                out.first_prompt = Some(one_line(&b.text, 140)).filter(|t| !t.is_empty());
            }
            // Calls left before a prompt of yours aren't waiting any more.
            out.open.clear();
            out.error = None;
            continue;
        }
        if let Some(c) = &b.call {
            out.last_tool = Some(c.label.clone());
            // The agent picks each command's folder: one run somewhere else only to look isn't
            // work there.
            if let Some(w) = c.work.as_ref().filter(|w| !c.looks || own.is_some_and(|own| Path::new(w).starts_with(own))) {
                note_work(&mut out.work, w.clone());
            }
            if c.open {
                out.open.push(c.clone());
            }
        } else if !b.text.is_empty() {
            out.last_text = Some(one_line(&b.text, 140)).filter(|t| !t.is_empty());
            let chars: Vec<char> = b.text.chars().collect();
            out.last_text_tail = Some(chars[chars.len().saturating_sub(600)..].iter().collect());
        }
        out.error = b.error.clone();
    }
    out
}

/// A message's items (transcript.rs): your prompt, the agent's words, or one of its calls with
/// what it gave back, which the app keeps on the call itself.
fn bubble_items(v: &Value) -> Vec<Item> {
    let Some(b) = bubble(v) else { return Vec::new() };
    let mut out = Vec::new();
    if b.you {
        if !b.text.is_empty() {
            out.push(item(b.at, "you", b.text));
        }
        return out;
    }
    if let Some(c) = &b.call {
        let shell = c.command.as_deref().is_some_and(|cmd| !cmd.trim().is_empty() && kept_whole(cmd));
        out.push(Item {
            id: Some(c.id.clone()).filter(|id| !id.is_empty()),
            input: Some(clip_input(&c.input)).filter(|i| !i.is_empty()),
            output: c.output.as_deref().map(clip_output),
            failed: c.failed,
            shell,
            cwd: c.cwd.clone().filter(|_| shell),
            ..item(b.at, "tool", c.label.clone())
        });
    } else if !b.text.is_empty() {
        out.push(item(b.at, "agent", b.text.clone()));
    }
    if let Some(e) = &b.error {
        out.push(item(b.at, "note", e.clone()));
    }
    out
}

/// A page of an app chat's conversation, back from `before` (from the newest when none). A
/// message's place is one more than its index among the chat's messages, and `before` and
/// `after` count messages; a chat still generating has its last message read again by the next
/// read on, as it may still be written.
fn app_page(db: &Connection, chat: &str, generating: bool, before: Option<u64>, limit: usize) -> Page {
    let ids = message_ids(db, chat);
    let mut lo = before.map_or(ids.len(), |b| (b as usize).min(ids.len()));
    let mut newest_first: Vec<Vec<Item>> = Vec::new();
    let mut count = 0;
    // Fifty messages at a time, newest first, until there are enough items.
    while lo > 0 && count < limit {
        let from = lo.saturating_sub(50);
        let found = messages(db, chat, &ids[from..lo]);
        for i in (from..lo).rev() {
            let its = found.get(&ids[i].0).map(|v| placed(bubble_items(v), (i + 1) as u64)).unwrap_or_default();
            count += its.len();
            newest_first.push(its);
            lo = i;
            if count >= limit {
                break;
            }
        }
    }
    let items = newest_first.into_iter().rev().flatten().collect();
    let after = if generating { ids.len().saturating_sub(1) } else { ids.len() } as u64;
    Page { items, before: (lo > 0).then_some(lo as u64), after }
}

/// What an app chat said and did after `after`.
fn app_newer(db: &Connection, chat: &str, generating: bool, after: u64) -> Page {
    let ids = message_ids(db, chat);
    let from = (after as usize).min(ids.len());
    let found = messages(db, chat, &ids[from..]);
    let items = (from..ids.len()).flat_map(|i| found.get(&ids[i].0).map(|v| placed(bubble_items(v), (i + 1) as u64)).unwrap_or_default()).collect();
    let last = if generating { ids.len().saturating_sub(1) } else { ids.len() };
    Page { items, before: None, after: (last as u64).max(after) }
}

/* ---- The command line's chats ---- */

/// What a chat's `store.db` says of it: its `meta` row's value is JSON written as hex.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Meta {
    pub name: Option<String>,
    pub created_at: Option<i64>,
    pub model: Option<String>,
}

fn decode_hex(s: &str) -> Option<Vec<u8>> {
    let s = s.trim();
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok()).collect()
}

/// A `meta` value: hex for JSON, or the JSON itself.
pub fn parse_meta(value: &str) -> Meta {
    let json = decode_hex(value).and_then(|b| String::from_utf8(b).ok()).unwrap_or_else(|| value.to_string());
    let v: Value = serde_json::from_str(&json).unwrap_or(Value::Null);
    let text = |k: &str| v.get(k).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
    Meta { name: text("name"), created_at: v.get("createdAt").and_then(Value::as_i64), model: text("lastUsedModel") }
}

fn read_meta(store: &Path) -> Meta {
    let Some(db) = crate::harness::open_database(store) else { return Meta::default() };
    db.query_row("SELECT value FROM meta WHERE key = '0'", [], |r| r.get::<_, String>(0)).map(|v| parse_meta(&v)).unwrap_or_default()
}

/// A folder's path as Cursor names the folder of its records for it under `projects`: each run
/// of characters that aren't letters or digits one `-`.
pub fn slug(path: &str) -> String {
    let mut out = String::new();
    for c in path.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_string()
}

/// The folder whose slug `name` is, found by walking down from `root` through the folders whose
/// own slugs lead it. Many paths make one slug; the first folder that's there wins.
pub fn unslug(root: &Path, name: &str) -> Option<PathBuf> {
    fn walk(dir: &Path, rest: &str, depth: usize) -> Option<PathBuf> {
        if depth > 24 {
            return None;
        }
        let entries = std::fs::read_dir(dir).ok()?;
        let mut kids: Vec<(String, PathBuf)> = entries.flatten().filter(|e| e.file_type().is_ok_and(|t| t.is_dir())).map(|e| (slug(&e.file_name().to_string_lossy()), e.path())).filter(|(s, _)| !s.is_empty()).collect();
        // Longer names first, so `app-two` is tried before `app`.
        kids.sort_by(|a, b| b.0.len().cmp(&a.0.len()));
        for (s, path) in kids {
            if rest == s {
                return Some(path);
            }
            if let Some(found) = rest.strip_prefix(s.as_str()).and_then(|r| r.strip_prefix('-')).and_then(|more| walk(&path, more, depth + 1)) {
                return Some(found);
            }
        }
        None
    }
    walk(root, name, 0)
}

/// Where a chat's transcript is, under the account's `projects`, whichever folder's it is.
fn find_transcript(home: &Path, id: &str) -> Option<PathBuf> {
    let projects = std::fs::read_dir(home.join("projects")).ok()?;
    projects.flatten().map(|p| p.path().join("agent-transcripts").join(id).join(format!("{id}.jsonl"))).find(|p| p.is_file())
}

/// The command line's chats under `home`, written to since `since_ms` or in `keep`, newest
/// first, without their folders (`cli_cwd`).
pub fn read_cli_chats(home: &Path, since_ms: i64, keep: &HashSet<String>) -> Vec<Chat> {
    let Ok(folders) = std::fs::read_dir(home.join("chats")) else { return Vec::new() };
    let mut out = Vec::new();
    for folder in folders.flatten() {
        let Ok(chats) = std::fs::read_dir(folder.path()) else { continue };
        for chat in chats.flatten() {
            let dir = chat.path();
            let store = dir.join(STORE_DB);
            if !store.is_file() {
                continue;
            }
            let id = file_name(&dir);
            let transcript = find_transcript(home, &id);
            let files = [Some(store.clone()), Some(PathBuf::from(format!("{}-wal", store.display()))), transcript.clone()];
            let written = files.into_iter().flatten().filter_map(|p| crate::util::mtime_ms(&p)).max().unwrap_or(0);
            if written < since_ms && !keep.contains(&id) {
                continue;
            }
            let meta = read_meta(&store);
            out.push(Chat { id, title: meta.name, created_at: meta.created_at.unwrap_or(written), last_updated: written, model: meta.model, cli: true, dir: Some(dir), transcript, ..Chat::default() });
        }
    }
    out.sort_by(|a, b| b.last_updated.cmp(&a.last_updated));
    out
}

/// The folder a command line chat works in, from where its transcript is kept: its database's
/// folder is named for a hash of it, which can't be read back.
fn cli_cwd(c: &Chat) -> Option<String> {
    let project = c.transcript.as_deref()?.parent()?.parent()?.parent()?;
    unslug(Path::new("/"), &file_name(project)).map(|p| p.to_string_lossy().into_owned())
}

/// What you asked, and when, out of a prompt as a transcript writes it: the time as a sentence
/// in `<timestamp>`, the words in `<user_query>`.
fn prompt_of(text: &str) -> (String, Option<i64>) {
    let inner = |tag: &str| text.split_once(&format!("<{tag}>")).and_then(|(_, rest)| rest.split_once(&format!("</{tag}>"))).map(|(x, _)| x.trim());
    let asked = inner("user_query").unwrap_or(text).trim().to_string();
    (asked, inner("timestamp").and_then(prompt_time))
}

/// "Wednesday, Sep 2, 2026, 4:21 PM (UTC-7)" as epoch milliseconds.
fn prompt_time(s: &str) -> Option<i64> {
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let (when, zone) = s.split_once('(').map(|(w, z)| (w.trim(), z.trim_end_matches(')').trim())).unwrap_or((s.trim(), "UTC"));
    let parts: Vec<&str> = when.split(',').map(str::trim).collect();
    // [weekday,] "Sep 2", "2026", "4:21 PM"
    let parts = if parts.len() == 4 { &parts[1..] } else { &parts[..] };
    let [md, year, time] = parts else { return None };
    let (month, day) = md.split_once(' ')?;
    let month = MONTHS.iter().position(|m| month.starts_with(m))? + 1;
    let (clock, half) = time.split_once(' ').unwrap_or((time, ""));
    let (h, m) = clock.split_once(':')?;
    let mut hour: i64 = h.parse().ok()?;
    match half {
        "PM" if hour < 12 => hour += 12,
        "AM" if hour == 12 => hour = 0,
        _ => {}
    }
    let offset = zone.strip_prefix("UTC").or_else(|| zone.strip_prefix("GMT")).unwrap_or("");
    let sign = if offset.starts_with('-') { '-' } else { '+' };
    let unsigned = offset.trim_start_matches(['+', '-']);
    let offset = match unsigned.split_once(':') {
        _ if unsigned.is_empty() => "Z".to_string(),
        Some((h, m)) => format!("{sign}{:02}:{:02}", h.parse::<i64>().ok()?, m.parse::<i64>().ok()?),
        None => format!("{sign}{:02}:00", unsigned.parse::<i64>().ok()?),
    };
    parse_rfc3339_ms(&format!("{year}-{month:02}-{:02}T{hour:02}:{:02}:00{offset}", day.parse::<i64>().ok()?, m.parse::<i64>().ok()?))
}

/// A transcript line: yours, the agent's words and calls, or the end of a turn.
enum Line {
    You { text: String, at: Option<i64> },
    Agent { text: String, calls: Vec<Call> },
    TurnEnded { error: bool },
}

fn line(v: &Value) -> Option<Line> {
    if v.get("type").and_then(Value::as_str) == Some("turn_ended") {
        return Some(Line::TurnEnded { error: v.get("status").and_then(Value::as_str) == Some("error") });
    }
    let parts = v.pointer("/message/content").and_then(Value::as_array)?;
    let text: Vec<&str> = parts.iter().filter(|p| p.get("type").and_then(Value::as_str) == Some("text")).filter_map(|p| p.get("text").and_then(Value::as_str)).collect();
    let text = text.join("\n").trim().to_string();
    match v.get("role").and_then(Value::as_str)? {
        "user" => {
            let (asked, at) = prompt_of(&text);
            Some(Line::You { text: asked, at })
        }
        "assistant" => {
            let uses = parts.iter().filter(|p| p.get("type").and_then(Value::as_str) == Some("tool_use"));
            let calls = uses
                .map(|p| {
                    // The transcript names its tools as Claude Code does.
                    let name = match p.get("name").and_then(Value::as_str).unwrap_or("tool") {
                        "Shell" | "Bash" => "run_terminal_command",
                        "Read" => "read_file",
                        "Write" | "StrReplace" | "Edit" => "edit_file",
                        "Delete" => "delete_file",
                        "Grep" => "grep",
                        "Glob" => "glob_file_search",
                        "SemanticSearch" => "codebase_search",
                        "WebSearch" => "web_search",
                        "WebFetch" => "web_fetch",
                        "TodoWrite" => "todo_write",
                        "Task" => "task",
                        other => other,
                    };
                    call(&serde_json::json!({ "name": name, "params": p.get("input").cloned().unwrap_or(Value::Null), "status": "completed" }))
                })
                .collect();
            Some(Line::Agent { text, calls })
        }
        _ => None,
    }
}

/// A transcript line's items (transcript.rs). The lines carry no times of their own, only your
/// prompts say when.
fn transcript_items(v: &Value, at: i64) -> Vec<Item> {
    match line(v) {
        Some(Line::You { text, at: asked }) if !text.is_empty() => vec![item(asked.unwrap_or(at), "you", text)],
        Some(Line::Agent { text, calls }) => {
            let mut out = Vec::new();
            if !text.is_empty() {
                out.push(item(at, "agent", text));
            }
            for c in calls {
                let shell = c.command.as_deref().is_some_and(|cmd| !cmd.trim().is_empty() && kept_whole(cmd));
                out.push(Item { input: Some(clip_input(&c.input)).filter(|i| !i.is_empty()), shell, cwd: c.cwd.clone().filter(|_| shell), ..item(at, "tool", c.label) });
            }
            out
        }
        Some(Line::TurnEnded { error: true }) => vec![item(at, "note", "The turn stopped with an error")],
        _ => Vec::new(),
    }
}

/// What the end of a transcript says, in the chat's folder `own`.
fn transcript_tail(path: &Path, own: Option<&str>) -> Tail {
    let Ok(end) = crate::util::read_last(path, TAIL_BYTES) else { return Tail::default() };
    let mut bubbles = Vec::new();
    let mut ended = true;
    let mut errored = false;
    for v in end.text().lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()) {
        match line(&v) {
            Some(Line::You { text, at }) => {
                bubbles.push(Bubble { you: true, at: at.unwrap_or(0), text, call: None, error: None });
                ended = false;
                errored = false;
            }
            Some(Line::Agent { text, calls }) => {
                if !text.is_empty() {
                    bubbles.push(Bubble { you: false, at: 0, text, call: None, error: None });
                }
                bubbles.extend(calls.into_iter().map(|c| Bubble { you: false, at: 0, text: String::new(), call: Some(c), error: None }));
            }
            Some(Line::TurnEnded { error }) => {
                ended = true;
                errored = error;
            }
            None => {}
        }
    }
    let mut tail = tail_of(&bubbles, own);
    tail.turn_open = !ended;
    tail.error = errored.then(|| "The turn stopped with an error".to_string());
    if tail.first_prompt.is_none() {
        tail.first_prompt = crate::util::read_head(path, TAIL_BYTES as usize).and_then(|head| {
            head.lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()).find_map(|v| match line(&v) {
                Some(Line::You { text, .. }) => Some(one_line(&text, 140)).filter(|t| !t.is_empty()),
                _ => None,
            })
        });
    }
    tail
}

/* ---- The processes ---- */

/// A `cursor-agent` process on this Mac.
#[derive(Debug, Clone, PartialEq)]
pub struct Process {
    pub pid: u32,
    /// The chat it has open: from its arguments, else the chat database it holds open.
    pub chat: Option<String>,
    /// It's the interactive UI in a terminal, rather than a print run.
    pub terminal: bool,
}

/// What a process's command line says when it's `cursor-agent` working on a chat: whether it's
/// the interactive UI rather than a print run, and the chat it names. None for its other
/// commands (`login`, `models`…) and for anything else. Its launcher runs its bundled node as
/// the name it was run by (`exec -a`), so `ps` shows that name, node's options, the bundle's
/// `index.js` and the arguments, joined with spaces: a prompt reads as words, and only the
/// words before it matter.
pub fn cli_args(command: &str) -> Option<(bool, Option<String>)> {
    let mut words = command.split_whitespace().peekable();
    let program = words.next()?;
    if !matches!(program.rsplit('/').next(), Some("cursor-agent" | "agent")) {
        return None;
    }
    words.by_ref().find(|w| w.ends_with("/index.js") && w.contains("/cursor-agent/"))?;
    let mut interactive = true;
    let mut chat = None;
    while let Some(w) = words.next() {
        let (flag, value) = match w.split_once('=') {
            Some((f, v)) if f.starts_with("--") => (f, Some(v.to_string())),
            _ => (w, None),
        };
        match flag {
            "-p" | "--print" => interactive = false,
            // `--resume` alone picks a chat from a list.
            "--resume" | "--new-session-id" => {
                let id = match value {
                    Some(v) => Some(v),
                    None => words.next_if(|n| !n.starts_with('-')).map(str::to_string),
                };
                chat = id.filter(|id| id.len() >= 8 && id.chars().all(|c| c.is_ascii_hexdigit() || c == '-')).or(chat);
            }
            "--api-key" | "-H" | "--header" | "-e" | "--endpoint" | "--output-format" | "--mode" | "--model" | "--sandbox" | "--workspace" | "--add-dir" | "--plugin-dir" | "--worktree-base" if value.is_none() => {
                words.next();
            }
            _ if flag.starts_with('-') => {}
            "acp" | "install-shell-integration" | "uninstall-shell-integration" | "login" | "logout" | "mcp" | "plugin" | "worker" | "status" | "whoami" | "models" | "bedrock" | "about" | "update" | "create-chat" | "generate-rule" | "rule" | "ls" | "help" => return None,
            // `resume` goes on with the folder's latest chat.
            "resume" | "agent" => {}
            // A prompt.
            _ => break,
        }
    }
    Some((interactive, chat))
}

/// Whether a process's command line is the Cursor app's own.
fn is_app(command: &str) -> bool {
    let program = command.split(" --").next().unwrap_or(command).trim_end();
    program.ends_with("/Contents/MacOS/Cursor") && program.contains("Cursor.app")
}

/// The chat a command line process holds open: its `chats/<hash>/<id>/store.db` among its open
/// files (`lsof -Fn`'s `n<path>` lines).
fn held_chat(open_files: &str) -> Option<String> {
    open_files.lines().filter_map(|l| l.strip_prefix('n')).find_map(|p| {
        let p = Path::new(p.strip_suffix("-wal").unwrap_or(p));
        let dir = p.parent()?;
        (file_name(p) == STORE_DB && dir.parent()?.parent().map(file_name).as_deref() == Some("chats")).then(|| file_name(dir))
    })
}

/// The command line's processes, each with the chat it has open, and whether the app is
/// running. From the list of every process, and an `lsof` for a `cursor-agent` whose arguments
/// don't name its chat.
pub fn processes(listed: &[procs::PsLine]) -> (Vec<Process>, bool) {
    let mut app = false;
    let mut found = Vec::new();
    for p in listed {
        app |= is_app(&p.command);
        if let Some((interactive, chat)) = cli_args(&p.command) {
            let chat = chat.or_else(|| procs::run("/usr/sbin/lsof", &["-Fn", "-p", &p.pid.to_string()], None, Duration::from_secs(3)).ok().and_then(|files| held_chat(&files)));
            found.push(Process { pid: p.pid, chat, terminal: interactive && !p.tty.starts_with('?') });
        }
    }
    (found, app)
}

/* ---- Reading chats for the board ---- */

/// Cursor's chats for the board (`harness::Reader`): each account's chats as last read, newest
/// first, the command line's processes, and whether the app is running.
#[derive(Default)]
pub struct Reader {
    chats: HashMap<String, Vec<Chat>>,
    marks: crate::harness::DatabaseMarks,
    processes: Vec<Process>,
    app_running: bool,
    looked_at: Option<Instant>,
    read_at: Option<Instant>,
}

impl Reader {
    /// Reads an account's chats again: the app's list, the composerData of each that was written
    /// since and the end of its messages, and the command line's chats, with the end of each that
    /// changed or that a process has open. True when a chat is there that wasn't.
    fn reread(&mut self, a: &AccountCfg, look: &Look) -> bool {
        self.read_at = Some(Instant::now());
        let home = a.home_path();
        if self.looked_at.map_or(true, |t| t.elapsed() >= Duration::from_secs(2)) {
            self.look_at_processes(look);
        }
        let keep = look.seen.pinned_ids("cursor", &a.name);
        let before: HashMap<String, Chat> = self.chats.remove(&a.name).unwrap_or_default().into_iter().map(|c| (c.id.clone(), c)).collect();
        let mut rows = Vec::new();
        if let Some(db) = app_db(&home).and_then(|p| crate::harness::open_database(&p)) {
            let mut app = read_app_chats(&db, look.since, &keep).unwrap_or_default();
            let lengths = written(&db, &app.iter().map(|c| c.id.as_str()).collect::<Vec<_>>());
            for c in &mut app {
                let length = lengths.get(&c.id).copied().unwrap_or(0);
                match before.get(&c.id) {
                    // Nothing written since: what was read stands, as the header may lag.
                    Some(old) if old.written == length && length > 0 => *c = Chat { archived: c.archived, ..old.clone() },
                    _ => refresh(&db, c),
                }
            }
            app.retain(|c| c.messages > 0 || c.generating());
            rows.extend(app);
        }
        for mut c in read_cli_chats(&home, look.since, &keep) {
            let process = self.held(&c.id);
            // Walking the disk for its folder is done once a chat.
            let cwd = before.get(&c.id).and_then(|old| old.cwd.clone()).or_else(|| cli_cwd(&c));
            c.cwd = cwd.or_else(|| process.and_then(|p| procs::cwd_of(p.pid)).map(|d| d.to_string_lossy().into_owned()));
            match before.get(&c.id) {
                Some(old) if process.is_none() && old.last_updated == c.last_updated => c.tail = old.tail.clone(),
                _ => c.tail = c.transcript.as_deref().map(|t| transcript_tail(t, c.cwd.as_deref())).unwrap_or_default(),
            }
            rows.push(c);
        }
        rows.sort_by(|x, y| y.last_updated.cmp(&x.last_updated));
        let added = rows.iter().any(|c| !before.contains_key(&c.id));
        self.chats.insert(a.name.clone(), rows);
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

    fn held(&self, id: &str) -> Option<&Process> {
        self.processes.iter().find(|p| p.chat.as_deref() == Some(id))
    }
}

impl crate::harness::Reader for Reader {
    fn harness(&self) -> Harness {
        Harness::Cursor
    }

    fn scan(&mut self, accounts: &[AccountCfg], look: &Look) {
        self.chats.clear();
        if !accounts.is_empty() {
            self.look_at_processes(look);
        }
        for a in accounts {
            self.reread(a, look);
        }
    }

    /// The command line's chats, the transcripts both write, and the app's database's folder.
    fn watch(&self, account: &AccountCfg) -> Vec<(PathBuf, bool)> {
        let home = account.home_path();
        let mut out = vec![(home.join("chats"), true), (home.join("projects"), true)];
        out.extend(app_db(&home).and_then(|p| Some((p.parent()?.to_path_buf(), false))));
        out
    }

    fn owns(&self, account: &AccountCfg, path: &Path) -> bool {
        let home = account.home_path();
        path.starts_with(home.join("chats")) || path.starts_with(home.join("projects")) || app_db(&home).and_then(|p| Some(path.starts_with(p.parent()?))).unwrap_or(false)
    }

    fn apply(&mut self, account: &AccountCfg, paths: &[PathBuf], look: &Look) -> Applied {
        let counts = |p: &PathBuf| {
            let name = file_name(p);
            name.starts_with(STATE_DB) || name.starts_with(STORE_DB) || (name.ends_with(".jsonl") && p.components().any(|c| c.as_os_str() == "agent-transcripts"))
        };
        if !paths.iter().any(counts) {
            return Applied::default();
        }
        Applied { dirty: true, added: self.reread(account, look), lines: Vec::new() }
    }

    /// A process that came may have a chat that wasn't listed, and one that went leaves its chat
    /// as it was; and a chat the app was generating when it quit says so until the app opens it
    /// again, which only the app's processes tell.
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

    /// The app's database, which it writes all the time while it's open (its windows' state
    /// too) through a file it holds open, and the databases of the command line's chats a
    /// process has open. Read again at most every two seconds.
    fn glance(&mut self, accounts: &[AccountCfg], look: &Look) -> Applied {
        let mut out = Applied::default();
        for a in accounts {
            let home = a.home_path();
            let mut changed = app_db(&home).is_some_and(|db| self.marks.changed(&format!("{}|app", a.name), &db));
            let held: Vec<PathBuf> = self.chats.get(&a.name).into_iter().flatten().filter(|c| self.held(&c.id).is_some()).filter_map(|c| Some(c.dir.as_ref()?.join(STORE_DB))).collect();
            for db in held {
                changed |= self.marks.changed(&db.to_string_lossy(), &db);
            }
            if changed && self.read_at.map_or(true, |t| t.elapsed() >= Duration::from_secs(2)) {
                out.added |= self.reread(a, look);
                out.dirty = true;
            }
        }
        out
    }

    fn sessions(&self, account: &AccountCfg, look: &Look) -> Vec<Session> {
        let rows = self.chats.get(&account.name).map(|v| v.as_slice()).unwrap_or(&[]);
        rows.iter()
            .map(|c| {
                let key = Session::key_for(Harness::Cursor, &account.name, &c.id);
                let mut s = crate::board::cursor_session(account, c, self.held(&c.id), self.app_running, look.seen.seen_at(&key), look.now);
                s.archived |= look.archived_here(&key, s.last_activity);
                s
            })
            .collect()
    }

    fn cwds(&self) -> Vec<String> {
        self.chats.values().flatten().filter_map(|c| c.cwd.clone()).collect()
    }
}

/* ---- The adapter ---- */

/// The only way in for an app chat: nothing there takes a prompt from outside, so the app comes
/// forward on the chat's folder with the prompt on the clipboard to paste.
#[derive(Debug, Clone, PartialEq)]
pub struct OnClipboard {
    pub folder: Option<String>,
}

impl crate::send::Own for OnClipboard {
    fn way(&self) -> crate::send::Way {
        crate::send::Way::Cursor
    }

    fn takes_agents(&self) -> bool {
        false
    }

    fn leaves_it_to_paste(&self) -> bool {
        true
    }

    fn deliver(&self, d: &crate::send::Delivery) -> anyhow::Result<crate::send::Sent> {
        crate::send::copy(&crate::attachments::expand(d.text, crate::attachments::Style::Path))?;
        open_app(self.folder.as_deref())?;
        Ok(crate::send::opened(format!("Cursor is open on {}'s folder and the prompt is on the clipboard. Paste it into that chat to send it.", d.title())))
    }
}

/// Brings Cursor forward on a chat's folder: its window, which `open` makes when there's none.
/// No link selects a chat in it.
pub fn open_app(folder: Option<&str>) -> anyhow::Result<()> {
    let app = crate::detect::app("Cursor.app").ok_or_else(|| anyhow::anyhow!("Cursor isn't installed in Applications."))?;
    let app = app.to_string_lossy().into_owned();
    let mut args = vec!["-a", app.as_str()];
    if let Some(f) = folder.filter(|f| Path::new(f).is_dir()) {
        args.push(f);
    }
    procs::open(&args, Duration::from_secs(10)).map_err(|e| anyhow::anyhow!("Couldn't open Cursor: {e}"))
}

/// Cursor (`harness::Adapter`): the app's chats, read from its database, and the command line's,
/// which starts and resumes chats in a terminal. The app takes nothing from outside.
pub struct Cursor;

impl Cursor {
    fn chat_dir(&self, account: &AccountCfg, s: &Session) -> Option<PathBuf> {
        let chats = std::fs::read_dir(account.home_path().join("chats")).ok()?;
        chats.flatten().map(|f| f.path().join(&s.id)).find(|d| d.join(STORE_DB).is_file())
    }

    fn transcript(&self, account: &AccountCfg, s: &Session) -> anyhow::Result<PathBuf> {
        find_transcript(&account.home_path(), &s.id).ok_or_else(|| anyhow::anyhow!("Cursor hasn't written this chat's transcript yet."))
    }

    fn app_database(&self, account: &AccountCfg) -> anyhow::Result<Connection> {
        app_db(&account.home_path()).and_then(|p| crate::harness::open_database(&p)).ok_or_else(|| anyhow::anyhow!("Couldn't read Cursor's database."))
    }

    /// An app chat's calls that haven't finished, from its latest messages.
    fn app_open(&self, account: &AccountCfg, s: &Session) -> Vec<Call> {
        let Ok(db) = self.app_database(account) else { return Vec::new() };
        app_tail(&db, &Chat { id: s.id.clone(), cwd: Some(s.cwd.clone()), ..Chat::default() }).open
    }
}

/// Whether a session's turn is under way, so its last message may still be written.
fn under_way(s: &Session) -> bool {
    matches!(s.status, Status::Working | Status::Needs) && s.live
}

impl crate::harness::Adapter for Cursor {
    fn harness(&self) -> Harness {
        Harness::Cursor
    }

    /// One account in `~/.cursor`, which the app's chats go with (`app_db`); `CURSOR_CONFIG_DIR`
    /// moves the command line's folder.
    fn spec(&self) -> &'static crate::detect::Spec {
        const SPEC: crate::detect::Spec = crate::detect::Spec { harness: "cursor", program: Some(CLI), default_command: Some(CLI), var: Some(("CURSOR_CONFIG_DIR", None)), default_dir: "~/.cursor", siblings: None };
        &SPEC
    }

    fn app(&self) -> &'static str {
        "Cursor"
    }

    /// The app's sign-in, by the email it keeps beside its tokens, or the command line's file,
    /// when it keeps one rather than the keychain. Only the email is read.
    fn signed_in(&self, home: &Path) -> (bool, Option<String>) {
        let db = app_db(home).and_then(|p| crate::harness::open_database(&p));
        let email = db.and_then(|db| db.query_row("SELECT CAST(value AS TEXT) FROM ItemTable WHERE key = 'cursorAuth/cachedEmail'", [], |r| r.get::<_, String>(0)).ok());
        let email = email.map(|e| e.trim().to_string()).filter(|e| e.contains('@'));
        (email.is_some() || home.join("auth.json").is_file(), email)
    }

    /// The command line signs in with `login`; without it, the app does from its own window.
    fn sign_in_command(&self, account: &AccountCfg) -> String {
        if account.command.is_some() || cli_installed(&crate::util::home()) {
            format!("{} login", cli_command(account))
        } else {
            "open -a Cursor".to_string()
        }
    }

    fn installed(&self, home: &Path, on_path: &dyn Fn(&str) -> bool) -> bool {
        crate::detect::app("Cursor.app").is_some() || on_path(CLI) || crate::detect::is_executable(&home.join(".local/bin").join(CLI))
    }

    fn folder_use(&self, dir: &Path) -> crate::harness::FolderUse {
        let mut files: Vec<PathBuf> = std::fs::read_dir(dir.join("chats")).into_iter().flatten().flatten().map(|e| e.path()).collect();
        if let Some(db) = app_db(dir) {
            files.push(PathBuf::from(format!("{}-wal", db.display())));
            files.push(db);
        }
        let has_data = files.iter().any(|p| p.is_dir() || (p.is_file() && !file_name(p).ends_with("-wal")));
        crate::harness::FolderUse { has_data, last_used: files.iter().filter_map(|p| crate::util::mtime_ms(p)).max(), signed_in: dir.join("auth.json").is_file() }
    }

    /// The command line starts a chat in a terminal; the app only in its own window.
    fn places(&self, account: &AccountCfg, home: &Path) -> std::result::Result<Vec<crate::start::Place>, String> {
        if account.command.is_some() || cli_installed(home) {
            Ok(vec![crate::start::Place::Terminal, crate::start::Place::Background])
        } else {
            Err(format!("The Cursor app starts chats only in its own window. With its command line, {CLI}, installed, {} can start them in a terminal from here.", account.name))
        }
    }

    /// The command line's chat opens in its terminal, or a new one that resumes it
    /// (`cursor-agent --resume`); a print run has it until it ends. No link opens one of the
    /// app's chats, so the app comes forward on its folder.
    fn targets(&self, s: &Session, o: &crate::harness::Opens) -> Vec<crate::model::Target> {
        let who = o.who.clone();
        match s.surface {
            Surface::Cli => {
                let busy = s.live && s.pid.is_none();
                vec![o.terminal(s, if busy { format!("{who} · a run without a terminal has it open") } else { who }, busy)]
            }
            Surface::App => vec![o.way(s, "cursor", "app", "Open in Cursor".into(), format!("{who} · its folder's window, where the chat is in the list"), true, false)],
        }
    }

    fn open(&self, s: &Session, action: &str) -> anyhow::Result<()> {
        match action {
            "cursor" => open_app(Some(s.cwd.as_str()).filter(|c| !c.is_empty())),
            other => anyhow::bail!("Unknown action {other:?}"),
        }
    }

    fn app_bundle(&self, action: &str) -> Option<&'static str> {
        (action == "cursor").then_some(APP_BUNDLE_ID)
    }

    /// Every chat's transcript, the app's and the command line's alike.
    fn search_files(&self, account: &AccountCfg, since: i64) -> Vec<(PathBuf, String, i64)> {
        let Ok(projects) = std::fs::read_dir(account.home_path().join("projects")) else { return Vec::new() };
        let mut out = Vec::new();
        for p in projects.flatten() {
            let Ok(chats) = std::fs::read_dir(p.path().join("agent-transcripts")) else { continue };
            for chat in chats.flatten() {
                let id = file_name(&chat.path());
                let path = chat.path().join(format!("{id}.jsonl"));
                if let Some(at) = crate::util::mtime_ms(&path).filter(|at| *at >= since) {
                    out.push((path, Session::key_for(Harness::Cursor, &account.name, &id), at));
                }
            }
        }
        out
    }

    /// What you asked, and what the agent said.
    fn prose(&self, raw: &[u8]) -> Option<(i64, bool, String)> {
        if memchr::memmem::find(raw, br#""type":"text""#).is_none() {
            return None;
        }
        let v: Value = serde_json::from_slice(raw).ok()?;
        let said = match line(&v)? {
            Line::You { text, at } => (at.unwrap_or(0), true, text),
            Line::Agent { text, .. } => (0, false, text),
            Line::TurnEnded { .. } => return None,
        };
        (!said.2.is_empty()).then_some(said)
    }

    /// Codealong picks a new chat's id (`--new-session-id`, which takes a v4 UUID, as Codealong's
    /// are); the prompt goes as an argument, its attachments as paths whose folders are added
    /// (`--add-dir`, once each) so it reads them without asking. `--worktree` takes an optional
    /// name, so it goes after the prompt.
    fn start_in_terminal(&self, account: &AccountCfg, ask: &crate::harness::StartAsk) -> crate::harness::Typed {
        use crate::start::{FILES_VAR, IMAGES_VAR, PROMPT_VAR};
        let (has_images, has_files) = crate::attachments::kinds(ask.prompt);
        let prompt = crate::attachments::expand_in(ask.prompt, ask.images_dir, ask.files_dir, crate::attachments::Style::Path);
        let prompt = prompt.trim();
        let mut input = format!("{} --new-session-id {}", cli_command(account), ask.session_id);
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
            input.push_str(&format!(" \"${PROMPT_VAR}\""));
            env.push((PROMPT_VAR.to_string(), crate::start::as_argument(prompt)));
        }
        if ask.worktree {
            input.push_str(" --worktree");
        }
        input.push('\n');
        crate::harness::Typed { input, env, key: Some(Session::key_for(Harness::Cursor, &account.name, ask.session_id)) }
    }

    fn makes_worktrees(&self) -> bool {
        true
    }

    fn session_cwds(&self, home: &Path, since: i64) -> Vec<String> {
        let app = app_db(home).and_then(|p| crate::harness::open_database(&p)).and_then(|db| read_app_chats(&db, since, &HashSet::new()).ok()).unwrap_or_default();
        let cli = read_cli_chats(home, since, &HashSet::new()).into_iter().map(|c| Chat { cwd: cli_cwd(&c), ..c });
        app.into_iter().chain(cli).filter_map(|c| c.cwd).collect()
    }

    fn no_limits(&self) -> Option<&'static str> {
        Some("Cursor shows its usage on cursor.com.")
    }

    fn records(&self, account: &AccountCfg, s: &Session) -> Vec<PathBuf> {
        match s.surface {
            Surface::App => app_db(&account.home_path()).into_iter().collect(),
            Surface::Cli => self.transcript(account, s).ok().into_iter().chain(self.chat_dir(account, s).map(|d| d.join(STORE_DB))).collect(),
        }
    }

    fn conversation(&self, ctx: &Ctx, s: &Session, before: Option<u64>, limit: usize) -> anyhow::Result<Page> {
        match s.surface {
            Surface::App => Ok(app_page(&self.app_database(ctx.account)?, &s.id, under_way(s), before, limit)),
            Surface::Cli => crate::transcript::file_page(&self.transcript(ctx.account, s)?, transcript_items, before, limit),
        }
    }

    fn conversation_after(&self, ctx: &Ctx, s: &Session, after: u64) -> anyhow::Result<Page> {
        match s.surface {
            Surface::App => Ok(app_newer(&self.app_database(ctx.account)?, &s.id, under_way(s), after)),
            Surface::Cli => crate::transcript::file_newer(&self.transcript(ctx.account, s)?, transcript_items, after),
        }
    }

    /// The command line's chat takes a prompt typed into its terminal, or a new terminal that
    /// resumes it; one a print run has open takes nothing until that ends. Nothing reaches the
    /// app's from outside: the app comes forward with the prompt on the clipboard.
    fn routes(&self, _account: &AccountCfg, s: &Session) -> Vec<crate::send::Route> {
        match (s.surface, s.pid, s.live) {
            (Surface::Cli, Some(pid), _) => vec![crate::send::Route::Type { pid }],
            (Surface::Cli, None, false) => vec![crate::send::Route::Resume],
            (Surface::Cli, None, true) => Vec::new(),
            _ => vec![OnClipboard { folder: Some(s.cwd.clone()).filter(|c| !c.is_empty()) }.into()],
        }
    }

    /// `cursor-agent --resume <id> <prompt>` goes on with a chat in its terminal UI, on the
    /// model asked for, as a start is; without a prompt it opens on the chat and waits for one.
    fn resume(&self, account: &AccountCfg, s: &Session, ask: &crate::harness::StartAsk) -> anyhow::Result<(String, Vec<(String, String)>)> {
        let prompt = crate::attachments::expand(ask.prompt, crate::attachments::Style::Path).trim().to_string();
        let mut input = format!("{} --resume {}", cli_command(account), s.id);
        if let Some(m) = ask.model {
            input.push_str(&format!(" --model '{m}'"));
        }
        let mut env = Vec::new();
        if !prompt.is_empty() {
            input.push_str(&format!(" \"${}\"", crate::start::PROMPT_VAR));
            env.push((crate::start::PROMPT_VAR.to_string(), crate::start::as_argument(&prompt)));
        }
        input.push('\n');
        Ok((input, env))
    }

    /// A command line chat goes on in a terminal, or one behind your windows; the command line
    /// can't go on with the app's.
    fn resumes_in(&self, place: crate::start::Place) -> bool {
        use crate::start::Place;
        matches!(place, Place::Terminal | Place::Background)
    }

    /// What an app chat waits on you to allow or answer. It's answered in the app: nothing
    /// outside takes an answer, and the command line writes nothing down while its prompt is up.
    fn pending(&self, ctx: &Ctx, s: &Session) -> Option<crate::harness::Pending> {
        if s.surface != Surface::App {
            return None;
        }
        let open = self.app_open(ctx.account, s);
        let c = open.iter().find(|c| c.waits).or(open.first())?.clone();
        Some(crate::harness::Pending { id: c.id, label: c.label, input: Some(c.input).filter(|i| !i.is_empty()), cannot: Some("Cursor asks in its own window; answer it there.".into()), always: false, no_once: false, plan: false, message: false, always_rule: None, questions: Vec::new(), unwritten: false })
    }

    fn open_calls(&self, ctx: &Ctx, s: &Session) -> Vec<String> {
        match s.surface {
            Surface::App => self.app_open(ctx.account, s).into_iter().map(|c| c.input).collect(),
            Surface::Cli => Vec::new(),
        }
    }

    /// The app's database, and each command line chat's.
    fn databases(&self) -> &'static [&'static str] {
        &[STATE_DB, STORE_DB]
    }

    fn reader(&self, _wake: crate::harness::Wake) -> Box<dyn crate::harness::Reader> {
        Box::new(Reader::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::Adapter as _;
    use serde_json::json;

    /// The app's tables as 3.22 makes them, with a chat in each state: one generating with a
    /// call waiting to be allowed, one done in a worktree the app made, an empty one a new
    /// window made, a draft, a subagent's, and an old one.
    fn app_fixture() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE ItemTable (key TEXT UNIQUE ON CONFLICT REPLACE, value BLOB);
             CREATE TABLE cursorDiskKV (key TEXT UNIQUE ON CONFLICT REPLACE, value BLOB);
             CREATE TABLE composerHeaders (composerId TEXT PRIMARY KEY, workspaceId TEXT, createdAt INTEGER, lastUpdatedAt INTEGER, isArchived INTEGER, isSubagent INTEGER, recency INTEGER, checkpointAt INTEGER, value TEXT, subagentTypeName TEXT);",
        )
        .unwrap();
        let header = |id: &str, created: i64, updated: Option<i64>, subagent: i64, value: Value| {
            db.execute("INSERT INTO composerHeaders (composerId, workspaceId, createdAt, lastUpdatedAt, isArchived, isSubagent, value) VALUES (?1, 'w', ?2, ?3, 0, ?4, ?5)", rusqlite::params![id, created, updated, subagent, value.to_string()]).unwrap();
        };
        let folder = |p: &str| json!({ "id": "w", "uri": { "fsPath": p } });
        let repos = json!([{ "repoPath": "/Users/me/app", "branches": [{ "branchName": "old", "lastInteractionAt": 1 }, { "branchName": "fix-build", "lastInteractionAt": 9 }] }]);
        header("c1", 1_000, Some(5_000), 0, json!({ "type": "head", "name": "Fix the build", "hasBlockingPendingActions": true, "workspaceIdentifier": folder("/Users/me/app/"), "trackedGitRepos": repos }));
        header("c2", 1_000, Some(4_000), 0, json!({ "type": "head", "name": "Explain auth", "workspaceIdentifier": folder("/Users/me/api") }));
        header("empty", 3_000, None, 0, json!({ "type": "head", "workspaceIdentifier": folder("/Users/me/app") }));
        header("draft", 3_000, Some(3_500), 0, json!({ "type": "head", "name": "Half typed", "isDraft": true }));
        header("task-1", 3_000, None, 1, json!({ "type": "head", "name": "Subagent" }));
        header("old", 10, Some(20), 0, json!({ "type": "head", "name": "Old", "workspaceIdentifier": folder("/Users/me/app") }));
        let kv = |key: String, value: Value| {
            db.execute("INSERT INTO cursorDiskKV (key, value) VALUES (?1, ?2)", rusqlite::params![key, value.to_string()]).unwrap();
        };
        let order = |ids: &[(&str, i64)]| ids.iter().map(|(id, t)| json!({ "bubbleId": id, "type": t })).collect::<Vec<_>>();
        kv("composerData:c1".into(), json!({ "status": "generating", "lastUpdatedAt": 6_000, "name": "Fix the build", "hasBlockingPendingActions": true, "modelConfig": { "modelName": "gpt-5.5" }, "fullConversationHeadersOnly": order(&[("u1", 1), ("a1", 2), ("t1", 2), ("t2", 2)]) }));
        kv("composerData:c2".into(), json!({ "status": "completed", "lastUpdatedAt": 4_000, "modelConfig": { "modelName": "default" }, "gitWorktree": { "worktreePath": "/Users/me/.cursor/worktrees/api/sky", "branchName": "sky" }, "fullConversationHeadersOnly": order(&[("u1", 1), ("a1", 2)]) }));
        let bubble = |chat: &str, id: &str, v: Value| kv(format!("bubbleId:{chat}:{id}"), v);
        bubble("c1", "u1", json!({ "type": 1, "bubbleId": "u1", "text": "Fix the build", "createdAt": "2026-09-02T23:21:44.459Z" }));
        bubble("c1", "a1", json!({ "type": 2, "bubbleId": "a1", "text": "I'll run the tests first.", "createdAt": "2026-09-02T23:21:50.000Z" }));
        let ran = json!({ "name": "run_terminal_command_v2", "toolCallId": "tool_1", "status": "completed", "params": json!({ "command": "cd /Users/me/app && cargo test", "cwd": "/Users/me/app" }).to_string(), "result": json!({ "output": "2 failed\n", "rejected": false }).to_string(), "additionalData": { "status": "success" } });
        bubble("c1", "t1", json!({ "type": 2, "bubbleId": "t1", "createdAt": "2026-09-02T23:22:00.000Z", "toolFormerData": ran }));
        let asks = json!({ "name": "run_terminal_command_v2", "toolCallId": "tool_2", "status": "loading", "params": json!({ "command": "rm -rf target", "cwd": "/Users/me/app" }).to_string(), "additionalData": { "blockReason": "Not in allowlist: rm", "reviewData": { "status": "Requested" } } });
        bubble("c1", "t2", json!({ "type": 2, "bubbleId": "t2", "createdAt": "2026-09-02T23:23:00.000Z", "toolFormerData": asks }));
        bubble("c2", "u1", json!({ "type": 1, "bubbleId": "u1", "text": "How does auth work?", "createdAt": "2026-09-01T10:00:00.000Z" }));
        bubble("c2", "a1", json!({ "type": 2, "bubbleId": "a1", "text": "Sessions are cookies.\n\nThat's all.", "createdAt": "2026-09-01T10:00:05.000Z" }));
        db
    }

    fn read(db: &Connection) -> Vec<Chat> {
        let mut rows = read_app_chats(db, 1_000, &HashSet::new()).unwrap();
        for c in &mut rows {
            refresh(db, c);
        }
        rows
    }

    #[test]
    fn lists_the_apps_chats_but_not_empty_ones_drafts_or_subagents() {
        let db = app_fixture();
        let rows = read_app_chats(&db, 1_000, &HashSet::new()).unwrap();
        assert_eq!(rows.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(), ["c1", "c2"]);
        let c = &rows[0];
        assert_eq!((c.title.as_deref(), c.cwd.as_deref(), c.branch.as_deref(), c.blocking), (Some("Fix the build"), Some("/Users/me/app"), Some("fix-build"), true));
        // A pinned one comes however old it is.
        let kept = read_app_chats(&db, 1_000, &HashSet::from(["old".to_string()])).unwrap();
        assert!(kept.iter().any(|c| c.id == "old"));
    }

    #[test]
    fn reads_where_a_chat_stands_from_its_composer_data() {
        let db = app_fixture();
        let rows = read(&db);
        let (c1, c2) = (&rows[0], &rows[1]);
        assert!(c1.generating() && c1.blocking);
        // Its latest message, not `lastUpdatedAt`, which is when the last prompt was sent.
        assert_eq!((c1.last_updated, c1.messages, c1.model.as_deref()), (parse_rfc3339_ms("2026-09-02T23:23:00.000Z").unwrap(), 4, Some("gpt-5.5")));
        assert!(c1.written > 0);
        assert_eq!(written(&db, &["c1", "c2", "nope"]).get("c1"), Some(&c1.written));
        assert_eq!(c1.tail.first_prompt.as_deref(), Some("Fix the build"));
        assert_eq!(c1.tail.last_text.as_deref(), Some("I'll run the tests first."));
        assert_eq!(c1.tail.last_tool.as_deref(), Some("Run · `rm -rf target`"));
        assert_eq!(c1.tail.open.len(), 1);
        assert!(c1.tail.open[0].waits && c1.tail.open[0].id == "tool_2");
        // Each command a vote for where it works.
        assert_eq!(c1.tail.work, ["/Users/me/app", "/Users/me/app"]);
        // A worktree the app made for the chat is where it works; "default" is no model.
        assert_eq!((c2.status.as_str(), c2.cwd.as_deref(), c2.branch.as_deref(), c2.model.as_deref()), ("completed", Some("/Users/me/.cursor/worktrees/api/sky"), Some("sky"), None));
        assert_eq!(c2.tail.last_prompt_at, parse_rfc3339_ms("2026-09-01T10:00:00.000Z"));
        assert!(c2.tail.open.is_empty() && c2.tail.error.is_none());
    }

    #[test]
    fn pages_an_app_chats_conversation() {
        let db = app_fixture();
        let page = app_page(&db, "c1", true, None, 50);
        let kinds: Vec<&str> = page.items.iter().map(|i| i.kind).collect();
        assert_eq!(kinds, ["you", "agent", "tool", "tool"]);
        let run = &page.items[2];
        assert_eq!(run.text, "Run · `cd /Users/me/app && cargo test`");
        assert_eq!((run.input.as_deref(), run.output.as_deref(), run.shell, run.cwd.as_deref()), (Some("cd /Users/me/app && cargo test"), Some("2 failed"), true, Some("/Users/me/app")));
        assert_eq!(page.items[0].at, parse_rfc3339_ms("2026-09-02T23:21:44.459Z").unwrap());
        assert_eq!(page.items.iter().map(|i| i.cursor).collect::<Vec<_>>(), [1, 2, 3, 4]);
        // Still generating: its last message is read again by the next read on.
        assert_eq!((page.before, page.after), (None, 3));
        let newer = app_newer(&db, "c1", true, page.after);
        assert_eq!((newer.items.len(), newer.items[0].cursor, newer.after), (1, 4, 3));
        assert_eq!(app_newer(&db, "c1", false, 3).after, 4);
        // Back from the fourth message, two items at a time: the newest two before it.
        let back = app_page(&db, "c1", false, Some(3), 2);
        assert_eq!(back.items.iter().map(|i| i.kind).collect::<Vec<_>>(), ["agent", "tool"]);
        assert_eq!(back.before, Some(1));
        let first = app_page(&db, "c1", false, back.before, 2);
        assert_eq!((first.items[0].kind, first.before), ("you", None));
        let last = app_page(&db, "c1", false, None, 1);
        assert_eq!((last.items.len(), last.before, last.after), (1, Some(3), 4));
    }

    #[test]
    fn says_each_call_as_a_line() {
        let label = |t: Value| call(&t).label;
        assert_eq!(label(json!({ "name": "read_file_v2", "params": "{\"targetFile\":\"/a/src/main.rs\"}" })), "Read · `main.rs`");
        assert_eq!(label(json!({ "name": "edit_file_v2", "params": "{\"relativeWorkspacePath\":\"/a/src/lib.rs\"}" })), "Edit · `lib.rs`");
        assert_eq!(label(json!({ "name": "ripgrep_raw_search", "params": "{\"pattern\":\"fn main\",\"path\":\"/a\"}" })), "Search · fn main");
        assert_eq!(label(json!({ "name": "web_search", "params": "{\"searchTerm\":\"tauri panels\"}" })), "Search the web · tauri panels");
        let navigate = json!({ "name": "mcp-cursor-ide-browser-browser_navigate", "rawArgs": { "providerIdentifier": "cursor-ide-browser", "toolName": "browser_navigate", "args": { "url": "https://example.com" } } });
        assert_eq!(label(navigate), "cursor-ide-browser · browser_navigate");
        assert_eq!(label(json!({ "name": "mcp--", "rawArgs": {} })), "MCP tool");
        assert_eq!(label(json!({ "name": "set_active_branch" })), "Set active branch");
        assert_eq!(label(json!({ "name": "ask_question", "params": "{\"title\":\"Which database?\"}" })), "Ask · Which database?");
        // An edit is work where it writes; a read isn't.
        assert_eq!(call(&json!({ "name": "edit_file_v2", "params": "{\"relativeWorkspacePath\":\"/a/src/lib.rs\"}" })).work.as_deref(), Some("/a/src/lib.rs"));
        assert_eq!(call(&json!({ "name": "read_file_v2", "params": "{\"targetFile\":\"/b/x.rs\"}" })).work, None);
        // Turned down at its prompt.
        assert!(call(&json!({ "name": "switch_mode", "status": "cancelled", "userDecision": "rejected" })).failed);
        let asked = call(&json!({ "name": "ask_question", "status": "loading", "additionalData": { "status": "pending" } }));
        assert!(asked.open && asked.waits);
    }

    #[test]
    fn reads_the_transcript_both_write() {
        let dir = crate::fixtures::scratch("cursor-transcript");
        let path = dir.join("c.jsonl");
        let lines = [
            json!({ "role": "user", "message": { "content": [{ "type": "text", "text": "<timestamp>Wednesday, Sep 2, 2026, 4:21 PM (UTC-7)</timestamp>\n<user_query>\nFix the build\n</user_query>" }] } }),
            json!({ "role": "assistant", "message": { "content": [{ "type": "text", "text": "Running the tests." }, { "type": "tool_use", "name": "Shell", "input": { "command": "cargo test", "cwd": "/Users/me/app" } }, { "type": "tool_use", "name": "Read", "input": { "path": "/Users/me/app/src/lib.rs" } }] } }),
            json!({ "type": "turn_ended", "status": "success" }),
            json!({ "role": "user", "message": { "content": [{ "type": "text", "text": "<user_query>\nAnd the docs\n</user_query>" }] } }),
        ];
        std::fs::write(&path, lines.iter().map(|l| format!("{l}\n")).collect::<String>()).unwrap();
        let tail = transcript_tail(&path, Some("/Users/me/app"));
        assert_eq!(tail.first_prompt.as_deref(), Some("Fix the build"));
        assert_eq!(tail.last_tool.as_deref(), Some("Read · `lib.rs`"));
        assert!(tail.turn_open, "a prompt after the last turn ended");
        assert_eq!(tail.work, ["/Users/me/app"]);
        let page = crate::transcript::file_page(&path, transcript_items, None, 50).unwrap();
        let kinds: Vec<&str> = page.items.iter().map(|i| i.kind).collect();
        assert_eq!(kinds, ["you", "agent", "tool", "tool", "you"]);
        assert_eq!(page.items[0].text, "Fix the build");
        assert_eq!(page.items[0].at, parse_rfc3339_ms("2026-09-02T16:21:00-07:00").unwrap());
        assert_eq!(page.items[2].text, "Run · `cargo test`");
        assert!(page.items[2].shell);
        assert_eq!(Cursor.prose(lines[0].to_string().as_bytes()).map(|(_, you, t)| (you, t)), Some((true, "Fix the build".to_string())));
        assert_eq!(Cursor.prose(lines[2].to_string().as_bytes()), None);
    }

    #[test]
    fn reads_a_prompts_time_as_the_transcript_says_it() {
        assert_eq!(prompt_time("Wednesday, Sep 2, 2026, 4:21 PM (UTC-7)"), parse_rfc3339_ms("2026-09-02T16:21:00-07:00"));
        assert_eq!(prompt_time("Friday, Oct 9, 2026, 12:05 AM (UTC+5:30)"), parse_rfc3339_ms("2026-10-09T00:05:00+05:30"));
        assert_eq!(prompt_time("Oct 9, 2026, 12:05 PM (UTC)"), parse_rfc3339_ms("2026-10-09T12:05:00Z"));
        assert_eq!(prompt_time("soon"), None);
    }

    #[test]
    fn reads_a_command_line_chats_meta() {
        let text = r#"{"agentId":"a","name":"Fix the build","createdAt":1790000000000,"lastUsedModel":"gpt-5.5"}"#;
        let hex: String = text.bytes().map(|b| format!("{b:02x}")).collect();
        assert_eq!(parse_meta(&hex), Meta { name: Some("Fix the build".into()), created_at: Some(1_790_000_000_000), model: Some("gpt-5.5".into()) });
        assert_eq!(parse_meta(text).name.as_deref(), Some("Fix the build"));
    }

    #[test]
    fn finds_a_folder_from_its_slug() {
        assert_eq!(slug("/Users/me/projects/atlas-infra/.claude/worktrees/taco"), "Users-me-projects-atlas-infra-claude-worktrees-taco");
        let root = crate::fixtures::scratch("cursor-slug");
        for d in ["Users/me/my app/src", "Users/me/my-app-two", "Users/me/my"] {
            std::fs::create_dir_all(root.join(d)).unwrap();
        }
        assert_eq!(unslug(&root, "Users-me-my-app-src"), Some(root.join("Users/me/my app/src")));
        assert_eq!(unslug(&root, "Users-me-my-app-two"), Some(root.join("Users/me/my-app-two")));
        assert_eq!(unslug(&root, "Users-me-my"), Some(root.join("Users/me/my")));
        assert_eq!(unslug(&root, "Users-me-nowhere"), None);
    }

    #[test]
    fn knows_its_processes_by_their_arguments() {
        let bin = "/Users/me/.local/share/cursor-agent/versions/2026.08.11-e8db854/index.js";
        let ps = |rest: &str| format!("cursor-agent --use-system-ca {bin} {rest}");
        let id = "6ea31b00-0a64-4c8c-bfd0-f71170a1bf7f";
        assert_eq!(cli_args(&ps("")), Some((true, None)));
        assert_eq!(cli_args(&ps(&format!("--resume {id}"))), Some((true, Some(id.into()))));
        assert_eq!(cli_args(&ps(&format!("--new-session-id={id} --model gpt-5 fix it"))), Some((true, Some(id.into()))));
        assert_eq!(cli_args(&ps("-p --output-format json fix it")), Some((false, None)));
        // A prompt that names a command is still a prompt after its first word.
        assert_eq!(cli_args(&ps("fix the login page")), Some((true, None)));
        assert_eq!(cli_args(&ps("login")), None);
        assert_eq!(cli_args(&ps("acp")), None);
        assert_eq!(cli_args(&format!("/Users/me/.local/bin/agent {bin} --resume")), Some((true, None)));
        assert_eq!(cli_args("/opt/homebrew/bin/agent run"), None);
        assert!(is_app("/Applications/Cursor.app/Contents/MacOS/Cursor"));
        assert!(!is_app("/Applications/Cursor.app/Contents/Frameworks/Cursor Helper.app/Contents/MacOS/Cursor Helper --type=gpu-process"));
        assert_eq!(held_chat("p42\nn/Users/me/.cursor/chats/9f1c/6ea31b00-0a64/store.db-wal\n"), Some("6ea31b00-0a64".into()));
        assert_eq!(held_chat("p42\nn/Users/me/.cursor/ai-tracking/ai-code-tracking.db\n"), None);
    }

    /// Every chat on this Mac, however old, as the board would show it, and the newest one's
    /// conversation, read from your own Cursor folders without writing to them:
    /// `cargo test cursor_here -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn cursor_here() {
        let home = crate::util::home().join(".cursor");
        let account = AccountCfg { name: "cursor".into(), harness: "cursor".into(), home: home.to_string_lossy().into_owned(), command: None, start: None, preferred: false, acp: None };
        let started = Instant::now();
        let db = app_db(&home).and_then(|p| crate::harness::open_database(&p)).expect("Cursor's database");
        let mut chats = read(&db);
        chats.retain(|c| c.messages > 0);
        let app_ms = started.elapsed().as_millis();
        // What a read costs when nothing was written since: the list and how long each is.
        let started = Instant::now();
        let listed = read_app_chats(&db, 0, &HashSet::new()).unwrap();
        let lengths = written(&db, &listed.iter().map(|c| c.id.as_str()).collect::<Vec<_>>());
        println!("a read with nothing new: {} µs for {} chats' lengths", started.elapsed().as_micros(), lengths.len());
        chats.extend(read_cli_chats(&home, 0, &HashSet::new()).into_iter().map(|c| Chat { cwd: cli_cwd(&c), ..c }));
        let (procs, app_running) = processes(crate::procs::Listing::default().all());
        println!("{} chats ({app_ms} ms for the app's), the app {}running, {} command line processes", chats.len(), if app_running { "" } else { "not " }, procs.len());
        let now = crate::util::now_ms();
        for c in &chats {
            let s = crate::board::cursor_session(&account, c, None, app_running, 0, now);
            println!("{:8} {:3} {:40} {:50} | {} | {:?} {:?}", format!("{:?}", s.status), if c.cli { "cli" } else { "app" }, one_line(&s.title, 40), one_line(&s.cwd, 50), one_line(s.excerpt.as_deref().unwrap_or(""), 60), s.model, s.branch);
        }
        let newest = chats.iter().find(|c| !c.cli).expect("an app chat");
        let started = Instant::now();
        let page = app_page(&db, &newest.id, false, None, 30);
        println!("\n{} items of {:?} in {} ms, before {:?}, after {}", page.items.len(), newest.title, started.elapsed().as_millis(), page.before, page.after);
        for i in &page.items {
            println!("  {:6} {}{}", i.kind, one_line(&i.text, 90), i.output.as_deref().map(|o| format!("  → {}", one_line(o, 40))).unwrap_or_default());
        }
    }

    #[test]
    fn starts_and_resumes_in_a_terminal() {
        let account = AccountCfg { name: "cursor".into(), harness: "cursor".into(), home: "~/.cursor".into(), command: None, start: None, preferred: false, acp: None };
        let id = "6ea31b00-0a64-4c8c-bfd0-f71170a1bf7f";
        let ask = |prompt: &'static str, model: Option<&'static str>, worktree: bool| crate::harness::StartAsk { prompt, session_id: id, model, effort: None, worktree, images_dir: Path::new("/i"), files_dir: Path::new("/f") };
        let typed = Cursor.start_in_terminal(&account, &ask("-v please", Some("gpt-5"), true));
        assert_eq!(typed.input, format!("cursor-agent --new-session-id {id} --model 'gpt-5' \"${}\" --worktree\n", crate::start::PROMPT_VAR));
        assert_eq!(typed.env, vec![(crate::start::PROMPT_VAR.to_string(), " -v please".to_string())]);
        assert_eq!(typed.key.as_deref(), Some("cursor:cursor:6ea31b00-0a64-4c8c-bfd0-f71170a1bf7f"));
        assert_eq!(Cursor.start_in_terminal(&account, &ask("", None, false)).input, format!("cursor-agent --new-session-id {id}\n"));
        let s = Session::new(Harness::Cursor, "cursor", "abc");
        assert_eq!(Cursor.resume(&account, &s, &ask("", None, false)).unwrap(), ("cursor-agent --resume abc\n".to_string(), Vec::new()));
    }
}
