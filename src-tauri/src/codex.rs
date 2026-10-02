//! Codex: the thread index (`~/.codex/session_index.jsonl`) and rollouts
//! (`~/.codex/sessions/YYYY/MM/DD/rollout-<time>-<id>.jsonl`). See *Harness adapters*.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::config::AccountCfg;
use crate::harness::{Applied, Look, NewLines};
use crate::model::{Harness, Session, Usage};
use crate::transcript::{clip_input, clip_output, item, kept_whole, text_of, Item, Page};
/// The Codex app's bundle ID. The ChatGPT chat app has the same name, so the app is known by this.
pub const APP_BUNDLE_ID: &str = "com.openai.codex";

use crate::util::{code, command_dir, file_name, mtime_ms, note_work, one_line, parallel_map, parse_rfc3339_ms, read_first_line, resolve, scan_lines_back, without_cd};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ThreadInfo {
    pub id: String,
    pub cwd: String,
    pub branch: Option<String>,
    pub started_at: Option<i64>,
    pub last_ts: i64,
    /// A turn started and hasn't completed or been aborted.
    pub turn_open: bool,
    pub turn_started_at: Option<i64>,
    pub last_complete_at: Option<i64>,
    pub last_agent_message: Option<String>,
    /// The end of that message, for Jev.
    pub last_agent_tail: Option<String>,
    pub last_user_message: Option<String>,
    /// When you sent that prompt. It doesn't move while the thread works, so lists sort by it.
    pub last_prompt_at: Option<i64>,
    /// An approval request with nothing after it.
    pub pending_approval: Option<String>,
    /// Tool calls the model made that have no result yet, oldest first. The app doesn't record
    /// its approval requests (a computer-use app to allow, a command to run), so a call that
    /// stays unanswered well past how long its tool takes is the only sign one is waiting.
    pub pending_calls: Vec<PendingCall>,
    /// The latest rate limits each turn reported, by limit id (`codex`, or a per-model one).
    pub rate_limits: BTreeMap<String, RateLimit>,
    /// Where its latest commands ran and file changes landed, oldest first.
    pub work: Vec<String>,
    /// Started in the Codex CLI (`codex` in a terminal), not the app.
    pub cli: bool,
    /// The thread it's a fork of (`forked_from_id`): `codex fork`, or the app's Fork.
    pub forked_from: Option<String>,
    /// The thread whose agent made it (`thread_source: agent_created_thread`): the app's
    /// `create_thread` tool answers the new thread's first turn with a `<codex_delegation>` that
    /// names it in `<source_thread_id>`.
    pub started_by: Option<String>,
}

/// A tool call without its output yet.
#[derive(Debug, Clone, PartialEq)]
pub struct PendingCall {
    pub call_id: String,
    pub at: i64,
    /// What it does, for the excerpt: its title, command or tool name.
    pub what: String,
    /// How long it can go unanswered before it's likely waiting for approval.
    pub limit_ms: i64,
}

/// A rate-limit report from a `token_count` event.
#[derive(Debug, Clone, PartialEq)]
pub struct RateLimit {
    /// Set on a per-model limit: `GPT-5.3-Codex-Spark`.
    pub name: Option<String>,
    pub plan: Option<String>,
    pub windows: Vec<RateWindow>,
    /// When the turn reported it.
    pub at: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RateWindow {
    pub used_percent: f64,
    pub minutes: i64,
    pub resets_at: Option<i64>,
}

fn parse_rate_limit(v: &Value, at: i64) -> Option<(String, RateLimit)> {
    let id = v.get("limit_id").and_then(Value::as_str).unwrap_or("codex").to_string();
    let windows = ["primary", "secondary"]
        .iter()
        .filter_map(|k| v.get(*k).filter(|w| w.is_object()))
        .map(|w| RateWindow {
            used_percent: w.get("used_percent").and_then(Value::as_f64).unwrap_or(0.0),
            minutes: w.get("window_minutes").and_then(Value::as_i64).unwrap_or(0),
            resets_at: w.get("resets_at").and_then(Value::as_i64).map(|s| s * 1000),
        })
        .collect::<Vec<_>>();
    if windows.is_empty() {
        return None;
    }
    let name = v.get("limit_name").and_then(Value::as_str).map(str::to_string);
    let plan = v.get("plan_type").and_then(Value::as_str).map(str::to_string);
    Some((id, RateLimit { name, plan, windows, at }))
}

/// `id → (thread name, updated at)`, newest line winning.
pub fn read_index(codex_home: &Path) -> HashMap<String, (String, i64)> {
    let mut out = HashMap::new();
    let Ok(text) = std::fs::read_to_string(codex_home.join("session_index.jsonl")) else { return out };
    for line in text.lines() {
        let Ok(v) = serde_json::from_str::<Value>(line) else { continue };
        let (Some(id), Some(name)) = (v.get("id").and_then(Value::as_str), v.get("thread_name").and_then(Value::as_str)) else {
            continue;
        };
        let updated = v.get("updated_at").and_then(Value::as_str).and_then(parse_rfc3339_ms).unwrap_or(0);
        let entry = out.entry(id.to_string()).or_insert((name.to_string(), updated));
        if updated >= entry.1 {
            *entry = (name.to_string(), updated);
        }
    }
    out
}

/// Folders the Codex app has as projects, from the state it saves in
/// `<CODEX_HOME>/.codex-global-state.json`. A new thread started in any other folder makes the
/// app add a project for it.
pub fn app_project_roots(codex_home: &Path) -> Vec<String> {
    #[derive(serde::Deserialize)]
    struct State {
        #[serde(rename = "local-projects", default)]
        projects: HashMap<String, Project>,
        #[serde(rename = "electron-saved-workspace-roots", default)]
        roots: Vec<String>,
    }
    #[derive(serde::Deserialize)]
    struct Project {
        #[serde(rename = "rootPaths", default)]
        root_paths: Vec<String>,
    }
    // The file runs to megabytes; only these two keys are kept while parsing.
    let Ok(bytes) = std::fs::read(codex_home.join(".codex-global-state.json")) else { return Vec::new() };
    let Ok(state) = serde_json::from_slice::<State>(&bytes) else { return Vec::new() };
    let mut out: Vec<String> = state.projects.into_values().flat_map(|p| p.root_paths).chain(state.roots).collect();
    out.sort();
    out.dedup();
    out
}

/// Where the app starts a thread that belongs to no project, its "projectless chat": each in a
/// folder of its own made from the date and its first words (`<date>/<slug>`).
pub fn chats_dir() -> PathBuf {
    crate::util::home().join("Documents/Codex")
}

/// Rollout files modified since `since_ms`, plus any whose thread is in `keep` (pinned ones),
/// however old.
pub fn rollout_files(codex_home: &Path, since_ms: i64, keep: &HashSet<String>) -> Vec<(PathBuf, i64)> {
    fn walk(dir: &Path, depth: usize, since: i64, keep: &HashSet<String>, out: &mut Vec<(PathBuf, i64)>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for e in entries.flatten() {
            let path = e.path();
            let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
            if is_dir && depth > 0 {
                // Every day folder is walked: a thread keeps writing to the folder of the day it started.
                walk(&path, depth - 1, since, keep, out);
            } else if !is_dir && file_name(&path).starts_with("rollout-") && path.extension().and_then(|x| x.to_str()) == Some("jsonl") {
                if let Some(m) = mtime_ms(&path) {
                    let pinned = || thread_id_of(&path).map(|id| keep.contains(&id)).unwrap_or(false);
                    if m >= since || (!keep.is_empty() && pinned()) {
                        out.push((path, m));
                    }
                }
            }
        }
    }
    let mut out = Vec::new();
    walk(&codex_home.join("sessions"), 3, since_ms, keep, &mut out);
    out
}

/// Rollouts in `archived_sessions` for the threads in `ids`: the ones archived from Codealong,
/// shown in its Archived list so they can be unarchived there.
pub fn archived_rollout_files(codex_home: &Path, ids: &HashSet<String>) -> Vec<(PathBuf, i64)> {
    if ids.is_empty() {
        return Vec::new();
    }
    std::fs::read_dir(codex_home.join("archived_sessions"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| thread_id_of(p).map(|id| ids.contains(&id)).unwrap_or(false))
        .filter_map(|p| mtime_ms(&p).map(|m| (p, m)))
        .collect()
}

/// The thread id at the end of a rollout file name.
pub fn thread_id_of(path: &Path) -> Option<String> {
    let name = file_name(path);
    let stem = name.strip_suffix(".jsonl")?;
    (stem.len() >= 36).then(|| stem[stem.len() - 36..].to_string())
}

/// Parses a rollout. Threads you didn't start (guardian reviews, subagents) return `None`.
/// With an earlier result and the offset it stopped at, only new lines are read.
///
/// A first parse reads the last 256KB, then reads back further for whatever that missed that
/// the status depends on: a turn can run for megabytes, so the event that started it and the
/// prompt it's working on can be far behind the tail.
pub fn parse_rollout(path: &Path, mtime: i64, prev: Option<(ThreadInfo, u64)>) -> Option<(ThreadInfo, u64)> {
    let len = std::fs::metadata(path).ok()?.len();
    // `floor` is where the lines after the session meta begin, on a first parse.
    let (mut info, start, mut text, floor) = match prev.filter(|(_, off)| *off <= len && len - off <= 1024 * 1024) {
        Some((info, off)) => (info, off, read_range(path, off, len)?, None),
        None => {
            let first = read_first_line(path, 4 * 1024 * 1024)?;
            let mut info = parse_meta(&first)?;
            if info.id.is_empty() {
                info.id = thread_id_of(path)?;
            }
            let floor = first.len() as u64;
            if info.started_by.is_some() {
                info.started_by = delegated_by(path, floor, len);
            }
            let start = len.saturating_sub(256 * 1024).max(floor);
            let text = read_range(path, start, len)?;
            (info, start, text, Some(floor))
        }
    };
    let mut start = start;
    if starts_mid_line(&text, start) {
        match text.find('\n') {
            Some(i) => {
                start += i as u64 + 1;
                text.drain(..=i);
            }
            None => text.clear(),
        }
    }
    let complete = text.rfind('\n').map(|i| i + 1).unwrap_or(0);
    let events = parse_events(&text[..complete]);
    if let Some(floor) = floor {
        for v in earlier_events(path, floor, start, &events) {
            apply_event(&mut info, &v);
        }
    }
    for v in &events {
        apply_event(&mut info, v);
    }
    if info.last_ts == 0 {
        info.last_ts = mtime;
    }
    Some((info, start + complete as u64))
}

/// The events before `end` that `later` has no newer copy of: the last turn event (started,
/// completed or aborted) and the last prompt, oldest first. Read backwards only as far as needed.
fn earlier_events(path: &Path, floor: u64, end: u64, later: &[Value]) -> Vec<Value> {
    let mut need_turn = !later.iter().any(is_turn_event);
    let mut need_prompt = !later.iter().any(|v| event_payload(v).and_then(prompt_of).is_some());
    let mut found = Vec::new();
    if !need_turn && !need_prompt {
        return found;
    }
    scan_lines_back(path, floor, end, |line| {
        // Cheap checks first: most bytes are tool output that none of these can match.
        let markers = ["\"task_started\"", "\"task_complete\"", "\"turn_aborted\"", "\"user_message\"", "\"UserMessage\""];
        if !line.contains("\"event_msg\"") || !markers.iter().any(|m| line.contains(m)) {
            return false;
        }
        let Ok(v) = serde_json::from_str::<Value>(line) else { return false };
        if need_turn && is_turn_event(&v) {
            need_turn = false;
            found.push(v);
        } else if need_prompt && event_payload(&v).and_then(prompt_of).is_some() {
            need_prompt = false;
            found.push(v);
        }
        !need_turn && !need_prompt
    });
    found.reverse();
    found
}

/// A read that starts mid-file begins with a partial line.
fn starts_mid_line(text: &str, start: u64) -> bool {
    start > 0 && !text.is_empty() && !text.starts_with('{')
}

fn read_range(path: &Path, start: u64, end: u64) -> Option<String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(path).ok()?;
    f.seek(SeekFrom::Start(start)).ok()?;
    let mut buf = Vec::with_capacity(end.saturating_sub(start) as usize);
    f.take(end.saturating_sub(start)).read_to_end(&mut buf).ok()?;
    Some(String::from_utf8_lossy(&buf).into_owned())
}

pub fn parse_meta(first_line: &str) -> Option<ThreadInfo> {
    let v: Value = serde_json::from_str(first_line).ok()?;
    if v.get("type").and_then(Value::as_str) != Some("session_meta") {
        return None;
    }
    let p = v.get("payload")?;
    let source = p.get("thread_source").and_then(Value::as_str);
    if !matches!(source, None | Some("user") | Some("agent_created_thread")) {
        return None;
    }
    if p.get("source").map(|s| s.get("subagent").is_some()).unwrap_or(false) {
        return None;
    }
    Some(ThreadInfo {
        id: p.get("id").and_then(Value::as_str).unwrap_or_default().to_string(),
        cwd: p.get("cwd").and_then(Value::as_str)?.to_string(),
        branch: p.pointer("/git/branch").and_then(Value::as_str).map(String::from),
        // The app writes "Codex Desktop"; the terminal UI "codex-tui" (and "codex_cli_rs" before).
        cli: matches!(p.get("originator").and_then(Value::as_str), Some("codex-tui" | "codex_cli_rs")),
        forked_from: p.get("forked_from_id").and_then(Value::as_str).filter(|id| !id.is_empty()).map(String::from),
        // Found in the lines after this one (`delegated_by`); marked here so they're read.
        started_by: (source == Some("agent_created_thread")).then(String::new),
        started_at: p
            .get("timestamp")
            .and_then(Value::as_str)
            .or_else(|| v.get("timestamp").and_then(Value::as_str))
            .and_then(parse_rfc3339_ms),
        ..Default::default()
    })
}

/// How far into a thread its delegation is looked for: it answers the first turn's first call,
/// after the app's instructions (tens of KB).
const DELEGATION_WITHIN: u64 = 512 * 1024;

/// The thread whose agent made this one, from the `<source_thread_id>` in the delegation that
/// opens it. Read once, on a thread's first parse.
fn delegated_by(path: &Path, floor: u64, len: u64) -> Option<String> {
    let text = read_range(path, floor, len.min(floor + DELEGATION_WITHIN))?;
    let id = element(&text, "source_thread_id")?.trim();
    (!id.is_empty() && id.chars().all(|c| c.is_ascii_hexdigit() || c == '-')).then(|| id.to_string())
}

fn parse_events(text: &str) -> Vec<Value> {
    text.lines().filter_map(|line| serde_json::from_str(line).ok()).collect()
}

/// The payload of an `event_msg` line.
fn event_payload(v: &Value) -> Option<&Value> {
    (v.get("type").and_then(Value::as_str) == Some("event_msg")).then(|| v.get("payload")).flatten()
}

fn is_turn_event(v: &Value) -> bool {
    let kind = event_payload(v).and_then(|p| p.get("type")).and_then(Value::as_str);
    matches!(kind, Some("task_started" | "task_complete" | "turn_aborted"))
}

/// The request in a prompt you sent: a `user_message` event, or a completed `UserMessage` item,
/// which is how newer Codex versions record it.
fn prompt_of(payload: &Value) -> Option<String> {
    let text = match payload.get("type").and_then(Value::as_str)? {
        "user_message" => payload.get("message").and_then(Value::as_str)?.to_string(),
        "item_completed" => {
            let item = payload.get("item").filter(|i| i.get("type").and_then(Value::as_str) == Some("UserMessage"))?;
            let parts = item.get("content")?.as_array()?;
            parts.iter().filter_map(|c| c.get("text").and_then(Value::as_str)).collect::<Vec<_>>().join("\n")
        }
        _ => return None,
    };
    request_of(&text).map(str::trim).filter(|r| !r.is_empty()).map(String::from)
}

/// A prompt you sent (true) or what the agent said (false), for searching sessions' text
/// (search.rs): `agent_message` events, or completed `AgentMessage` items in newer versions.
pub fn prose_of(v: &Value) -> Option<(bool, String)> {
    let payload = event_payload(v)?;
    if let Some(prompt) = prompt_of(payload) {
        return Some((true, prompt));
    }
    let text = match payload.get("type").and_then(Value::as_str)? {
        "agent_message" => payload.get("message").and_then(Value::as_str)?.to_string(),
        "item_completed" => {
            let item = payload.get("item").filter(|i| i.get("type").and_then(Value::as_str) == Some("AgentMessage"))?;
            item.get("content")?.as_array()?.iter().filter_map(|c| c.get("text").and_then(Value::as_str)).collect::<Vec<_>>().join("\n")
        }
        _ => return None,
    };
    let t = text.trim();
    (!t.is_empty()).then(|| (false, t.to_string()))
}

/// A prompt without the context Codex wraps around it: files or browser state before a
/// `## My request:` heading, an automation's `<heartbeat>`, a voice `<realtime_delegation>`.
/// An answer to the agent's question isn't a request.
fn request_of(text: &str) -> Option<&str> {
    let t = text.trim_start();
    if t.starts_with("<send_user_message_question_reply>") {
        return None;
    }
    if let Some((i, _)) = text.match_indices("## My request").find(|(i, _)| *i == 0 || text[..*i].ends_with('\n')) {
        return Some(text[i..].split_once('\n').map(|(_, rest)| rest).unwrap_or(""));
    }
    for (wrapper, inner) in [("<heartbeat>", "instructions"), ("<realtime_delegation>", "input")] {
        if t.starts_with(wrapper) {
            return element(t, inner);
        }
    }
    Some(text)
}

/// The text inside `<tag>…</tag>`.
fn element<'a>(text: &'a str, tag: &str) -> Option<&'a str> {
    let open = format!("<{tag}>");
    let start = text.find(&open)? + open.len();
    let end = text[start..].find(&format!("</{tag}>")).map_or(text.len(), |i| start + i);
    Some(&text[start..end])
}

/// Where a completed item worked: the folder a command ran in, or a file a change touched.
fn work_of(item: &Value, thread_cwd: &str) -> Option<String> {
    match item.get("type").and_then(Value::as_str)? {
        "CommandExecution" => {
            let cwd = item.get("cwd").and_then(Value::as_str).map(file_path_of).unwrap_or_else(|| thread_cwd.to_string());
            // Usually `[shell, "-lc", script]`.
            let script = match item.get("command")? {
                Value::Array(parts) => parts.last()?.as_str()?.to_string(),
                Value::String(s) => s.clone(),
                _ => return None,
            };
            command_dir(&script, &cwd)
        }
        "FileChange" => item.get("changes")?.as_object()?.keys().next().map(|p| resolve(thread_cwd, p)),
        _ => None,
    }
}

/// A `file://` URL as a path. Codex records command folders that way.
fn file_path_of(s: &str) -> String {
    let Some(rest) = s.strip_prefix("file://") else { return s.to_string() };
    let b = rest.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let hex = (b[i] == b'%' && i + 2 < b.len())
            .then(|| std::str::from_utf8(&b[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok()))
            .flatten();
        match hex {
            Some(byte) => {
                out.push(byte);
                i += 3;
            }
            None => {
                out.push(b[i]);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn apply_event(info: &mut ThreadInfo, v: &Value) {
    let ts = v.get("timestamp").and_then(Value::as_str).and_then(parse_rfc3339_ms);
    if let Some(t) = ts {
        info.last_ts = info.last_ts.max(t);
    }
    if v.get("type").and_then(Value::as_str) == Some("response_item") {
        if let Some(p) = v.get("payload") {
            note_call(info, p, ts.unwrap_or(info.last_ts));
        }
        return;
    }
    let Some(payload) = event_payload(v) else { return };
    let kind = payload.get("type").and_then(Value::as_str).unwrap_or_default();
    if matches!(kind, "task_started" | "task_complete" | "turn_aborted") {
        info.pending_calls.clear();
    }
    if kind == "item_completed" {
        if let Some(p) = payload.get("item").and_then(|item| work_of(item, &info.cwd)) {
            note_work(&mut info.work, p);
        }
    }
    match kind {
        "task_started" => {
            info.turn_open = true;
            info.turn_started_at = ts;
            info.pending_approval = None;
        }
        "task_complete" => {
            info.turn_open = false;
            info.last_complete_at = ts;
            info.pending_approval = None;
            if let Some(m) = payload.get("last_agent_message").and_then(Value::as_str).filter(|m| !m.trim().is_empty()) {
                info.last_agent_message = Some(one_line(m, 140));
                info.last_agent_tail = Some(crate::jev::tail(m, 600));
            }
        }
        "turn_aborted" => {
            info.turn_open = false;
            info.last_complete_at = ts;
            info.pending_approval = None;
        }
        "user_message" => {
            if let Some(m) = prompt_of(payload) {
                info.last_user_message = Some(one_line(&m, 140));
                info.last_prompt_at = ts.or(info.last_prompt_at);
            }
        }
        "token_count" => {
            let at = ts.unwrap_or(info.last_ts);
            if let Some((id, limit)) = payload.get("rate_limits").and_then(|rl| parse_rate_limit(rl, at)) {
                if info.rate_limits.get(&id).map(|old| old.at <= at).unwrap_or(true) {
                    info.rate_limits.insert(id, limit);
                }
            }
        }
        k if k.contains("approval_request") || k == "request_user_input" => {
            let what = payload
                .get("command")
                .map(|c| match c {
                    Value::Array(parts) => parts.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(" "),
                    Value::String(s) => s.clone(),
                    _ => String::new(),
                })
                .filter(|s| !s.is_empty())
                .map(|c| format!("Run · `{}`", one_line(&c, 80)))
                .unwrap_or_else(|| "Approval requested".to_string());
            info.pending_approval = Some(what);
        }
        _ => {
            if let Some(m) = prompt_of(payload) {
                info.last_user_message = Some(one_line(&m, 140));
                info.last_prompt_at = ts.or(info.last_prompt_at);
            }
            info.pending_approval = None;
        }
    }
}

/// Tracks a `response_item` tool call until its output arrives.
fn note_call(info: &mut ThreadInfo, p: &Value, at: i64) {
    let Some(call_id) = p.get("call_id").and_then(Value::as_str) else { return };
    match p.get("type").and_then(Value::as_str) {
        Some("function_call" | "custom_tool_call") => {
            let name = p.get("name").and_then(Value::as_str).unwrap_or_default();
            let Some(limit_ms) = call_limit_ms(name) else { return };
            info.pending_calls.retain(|c| c.call_id != call_id);
            info.pending_calls.push(PendingCall { call_id: call_id.to_string(), at, what: call_what(p, name), limit_ms });
        }
        Some("function_call_output" | "custom_tool_call_output") => info.pending_calls.retain(|c| c.call_id != call_id),
        _ => {}
    }
}

/// How long a call to this tool can take before it's likely waiting on you, from 22,000
/// calls in September 2026's rollouts. Commands return what they have by their yield time and
/// took at most 168 s; everything else nearly always answered within 30 s. None for tools that
/// wait by design.
fn call_limit_ms(name: &str) -> Option<i64> {
    match name {
        "sleep" | "wait" | "wait_agent" => None,
        "exec" | "exec_command" | "shell" | "local_shell" | "write_stdin" => Some(4 * 60 * 1000),
        _ => Some(60 * 1000),
    }
}

/// A call's own title if it gave one (computer-use calls do), else its command, else the tool.
fn call_what(p: &Value, name: &str) -> String {
    let args: Option<Value> = p.get("arguments").and_then(Value::as_str).and_then(|a| serde_json::from_str(a).ok());
    let field = |k: &str| args.as_ref().and_then(|a| a.get(k)).and_then(Value::as_str).map(str::trim).filter(|t| !t.is_empty()).map(String::from);
    if let Some(t) = field("title") {
        return one_line(&t, 80);
    }
    if let Some(c) = field("cmd") {
        return format!("`{}`", one_line(&c, 80));
    }
    let server = p.get("namespace").and_then(Value::as_str).map(|n| n.trim_start_matches("mcp__")).filter(|n| !n.is_empty());
    match server {
        Some(s) => format!("{s} · {name}"),
        None => name.to_string(),
    }
}

/* ---- The session panel's conversation ---- */

/// One rollout line as conversation items (transcript.rs): prompts and replies, and completed
/// commands, file changes, MCP calls and searches with what they gave back.
pub fn conversation_items(v: &Value, at: i64) -> Vec<Item> {
    if let Some((you, t)) = prose_of(v) {
        return vec![item(at, if you { "you" } else { "agent" }, t)];
    }
    let Some(p) = (v.get("type").and_then(Value::as_str) == Some("event_msg")).then(|| v.get("payload")).flatten() else { return Vec::new() };
    match p.get("type").and_then(Value::as_str) {
        Some("turn_aborted") => vec![item(at, "note", "The turn was stopped")],
        Some("item_completed") => {
            let Some(it) = p.get("item") else { return Vec::new() };
            let at = p.get("completed_at_ms").and_then(Value::as_i64).unwrap_or(at);
            let s = |k: &str| it.get(k).and_then(Value::as_str).unwrap_or("").to_string();
            match it.get("type").and_then(Value::as_str) {
                Some("CommandExecution") => {
                    // Usually `[shell, "-lc", script]`.
                    let script = match it.get("command") {
                        Some(Value::Array(a)) if a.len() >= 3 => a[2].as_str().unwrap_or("").to_string(),
                        Some(Value::Array(a)) => a.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(" "),
                        Some(Value::String(c)) => c.clone(),
                        _ => String::new(),
                    };
                    // A script a shell ran, not words joined from some other program's arguments.
                    let scripted = match it.get("command") {
                        Some(Value::Array(a)) => a.len() >= 3 && matches!(a[1].as_str(), Some("-c" | "-lc")),
                        Some(Value::String(_)) => true,
                        _ => false,
                    };
                    let shell = scripted && !script.trim().is_empty() && kept_whole(&script);
                    let exit = it.get("exit_code").and_then(Value::as_i64);
                    vec![Item {
                        input: Some(clip_input(&script)),
                        output: Some(clip_output(&s("aggregated_output"))),
                        failed: exit.is_some_and(|c| c != 0),
                        shell,
                        // The folder the agent chose for it, often another worktree than the thread's.
                        cwd: shell.then(|| it.get("cwd").and_then(Value::as_str).map(file_path_of)).flatten(),
                        ..item(at, "tool", format!("Shell · {}", code(&one_line(without_cd(&script), 80))))
                    }]
                }
                Some("FileChange") => {
                    let paths: Vec<String> = it.get("changes").and_then(Value::as_object).map(|m| m.keys().cloned().collect()).unwrap_or_default();
                    let label = match paths.as_slice() {
                        [one] => format!("Edit · {}", code(&crate::util::file_name(Path::new(one)))),
                        many => format!("Edit · {} files", many.len()),
                    };
                    vec![Item { input: Some(paths.join("\n")), failed: s("status") == "failed", ..item(at, "tool", label) }]
                }
                Some("McpToolCall") => {
                    let result = it.get("result");
                    vec![Item {
                        input: it.get("arguments").map(|a| clip_input(&serde_json::to_string_pretty(a).unwrap_or_default())),
                        output: result.and_then(|r| r.get("content")).map(|c| clip_output(&text_of(c))),
                        failed: result.and_then(|r| r.get("isError")).and_then(Value::as_bool) == Some(true),
                        ..item(at, "tool", format!("{} · {}", s("server"), s("tool")))
                    }]
                }
                Some("Extension") => {
                    let kind = s("kind");
                    let label = if kind == "web.search" { format!("Web search · {}", one_line(&s("query"), 80)) } else { kind };
                    vec![item(at, "tool", label)]
                }
                Some("ImageView") => vec![item(at, "tool", format!("Viewed an image{}", it.get("path").and_then(Value::as_str).map(|p| format!(" · {}", code(&crate::util::file_name(Path::new(p))))).unwrap_or_default()))],
                Some("ContextCompaction") => vec![item(at, "note", "The conversation was compacted here")],
                _ => Vec::new(),
            }
        }
        _ => Vec::new(),
    }
}

/// How a thread's latest turn ran, as its `turn_context` line has it: a side conversation's
/// copy runs the same way.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TurnRan {
    pub model: Option<String>,
    /// `read-only`, `workspace-write` or `danger-full-access` (`sandbox_policy.type`).
    pub sandbox: Option<String>,
    /// `untrusted`, `on-request` or `never`.
    pub approval: Option<String>,
}

/// A thread's latest `turn_context`. Read back from the end, since a turn's output can run to
/// megabytes.
fn last_turn(rollout: &Path) -> TurnRan {
    let mut ran = TurnRan::default();
    let Ok(len) = std::fs::metadata(rollout).map(|m| m.len()) else { return ran };
    scan_lines_back(rollout, 0, len, |line| {
        if !line.contains("\"turn_context\"") {
            return false;
        }
        let Ok(v) = serde_json::from_str::<Value>(line) else { return false };
        if v.get("type").and_then(Value::as_str) != Some("turn_context") {
            return false;
        }
        let text = |at: &str| v.pointer(at).and_then(Value::as_str).filter(|t| !t.is_empty()).map(String::from);
        ran = TurnRan {
            model: text("/payload/model"),
            sandbox: text("/payload/sandbox_policy/type").filter(|t| ["read-only", "workspace-write", "danger-full-access"].contains(&t.as_str())),
            approval: text("/payload/approval_policy").filter(|t| ["untrusted", "on-request", "never"].contains(&t.as_str())),
        };
        true
    });
    ran
}

/// How Codealong talks to a side conversation's copy of a thread (side.rs, `Adapter::side`): an
/// app-server of its own, JSON-RPC a line a message, doing what Codex's `/side` does (its
/// tui/src/app/side.rs; checked on 0.157.1, 2026-09-30). `thread/fork` with `ephemeral` copies
/// the thread without a rollout of its own (it needs `excludeTurns`, and a parent that has a
/// rollout), `thread/inject_items` puts the side conversation's rules after the copied history,
/// and each question is a `turn/start`. The answer comes as `item/agentMessage/delta`
/// notifications, each tool as an `item/started`, and `turn/completed` ends it. The copy runs
/// as the thread last did (`TurnRan`: its model, sandbox and approval policy), so it may do what
/// the thread may; an approval that comes to Codealong, with nobody to ask, is declined. A
/// thread that says nothing of how it ran is copied read-only, asking nothing.
pub struct SideThread {
    thread: String,
    cwd: String,
    ran: TurnRan,
    /// The copy's thread, once Codex has made it.
    copy: Option<String>,
    /// The copy has its rules and takes questions.
    ready: bool,
    /// The first question, asked while the copy was being made.
    waiting: Option<String>,
    next: i64,
    /// Some of this answer is written: its next message starts a new paragraph.
    said: bool,
}

impl SideThread {
    /// The requests that make the copy, by the ids their answers come back with.
    const INITIALIZE: i64 = 1;
    const FORK: i64 = 2;
    const INJECT: i64 = 3;

    pub fn new(thread: &str, cwd: &Path, ran: TurnRan) -> SideThread {
        SideThread { thread: thread.to_string(), cwd: cwd.to_string_lossy().into_owned(), ran, copy: None, ready: false, waiting: None, next: Self::INJECT, said: false }
    }

    fn turn(&mut self, text: &str) -> Value {
        self.next += 1;
        self.said = false;
        serde_json::json!({ "id": self.next, "method": "turn/start", "params": { "threadId": self.copy, "input": [{ "type": "text", "text": text }] } })
    }
}

impl crate::side::Dialect for SideThread {
    fn open(&mut self) -> Vec<Value> {
        vec![serde_json::json!({ "id": Self::INITIALIZE, "method": "initialize", "params": { "clientInfo": { "name": "codealong", "title": "Codealong", "version": env!("CARGO_PKG_VERSION") } } })]
    }

    fn ask(&mut self, text: &str) -> Vec<Value> {
        if !self.ready {
            self.waiting = Some(text.to_string());
            return Vec::new();
        }
        vec![self.turn(text)]
    }

    fn hear(&mut self, line: &Value) -> (Vec<crate::side::Event>, Vec<Value>) {
        use crate::side::Event;
        use serde_json::json;
        let nothing = (Vec::new(), Vec::new());
        let id = line.get("id");
        let method = line.get("method").and_then(Value::as_str);
        match (id, method) {
            // Something Codex asks of its client. A copy has nobody to ask, and no tools of ours.
            (Some(id), Some(method)) => {
                let reply = if method.ends_with("requestApproval") { json!({ "id": id, "result": { "decision": "decline" } }) } else { json!({ "id": id, "error": { "code": -32601, "message": "A side conversation doesn't take this." } }) };
                (Vec::new(), vec![reply])
            }
            // An answer to one of ours.
            (Some(id), None) => {
                if let Some(e) = line.get("error") {
                    let why = e.get("message").and_then(Value::as_str).unwrap_or("it wouldn't say why");
                    return (vec![Event::Done(Some(format!("Codex: {why}")))], Vec::new());
                }
                match id.as_i64() {
                    Some(Self::INITIALIZE) => {
                        let sandbox = self.ran.sandbox.as_deref().unwrap_or("read-only");
                        let approval = self.ran.approval.as_deref().filter(|_| self.ran.sandbox.is_some()).unwrap_or("never");
                        let mut params = json!({ "threadId": self.thread, "ephemeral": true, "excludeTurns": true, "cwd": self.cwd, "approvalPolicy": approval, "sandbox": sandbox, "developerInstructions": crate::side::RULES });
                        if let Some(m) = &self.ran.model {
                            params["model"] = json!(m);
                        }
                        (Vec::new(), vec![json!({ "method": "initialized" }), json!({ "id": Self::FORK, "method": "thread/fork", "params": params })])
                    }
                    Some(Self::FORK) => {
                        self.copy = line.pointer("/result/thread/id").and_then(Value::as_str).map(String::from);
                        let rules = json!({ "type": "message", "role": "user", "content": [{ "type": "input_text", "text": crate::side::RULES }] });
                        (Vec::new(), vec![json!({ "id": Self::INJECT, "method": "thread/inject_items", "params": { "threadId": self.copy, "items": [rules] } })])
                    }
                    Some(Self::INJECT) => {
                        self.ready = true;
                        let first = self.waiting.take().map(|q| self.turn(&q));
                        (Vec::new(), first.into_iter().collect())
                    }
                    _ => nothing,
                }
            }
            (None, Some(method)) => {
                let p = line.get("params").unwrap_or(&Value::Null);
                // The app-server tells of the thread it copied too.
                if p.get("threadId").and_then(Value::as_str).is_some_and(|t| Some(t) != self.copy.as_deref()) {
                    return nothing;
                }
                let item = p.get("item").unwrap_or(&Value::Null);
                let text = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).map(String::from);
                let events = match (method, item.get("type").and_then(Value::as_str)) {
                    ("item/agentMessage/delta", _) => {
                        self.said = true;
                        text(p, "delta").map(Event::Said).into_iter().collect()
                    }
                    ("item/started", Some("agentMessage")) if self.said => vec![Event::Said("\n\n".into())],
                    ("item/started", Some("commandExecution")) => vec![Event::Tool(code(&one_line(&text(item, "command").unwrap_or_default(), 100)))],
                    ("item/started", Some("mcpToolCall")) => vec![Event::Tool([text(item, "server"), text(item, "tool")].into_iter().flatten().collect::<Vec<_>>().join(" · "))],
                    ("item/started", Some("webSearch")) => vec![Event::Tool("Searching the web".into())],
                    ("turn/completed", _) => {
                        let why = match p.pointer("/turn/status").and_then(Value::as_str) {
                            Some("failed") => Some(p.pointer("/turn/error/message").and_then(Value::as_str).map(|m| format!("Codex: {m}")).unwrap_or_else(|| "Codex couldn't answer.".into())),
                            Some("interrupted") => Some("It was interrupted.".into()),
                            _ => None,
                        };
                        vec![Event::Done(why)]
                    }
                    _ => Vec::new(),
                };
                (events, Vec::new())
            }
            (None, None) => nothing,
        }
    }
}

/// Codex (`harness::Adapter`): its rollout for the conversation. Approvals aren't recorded
/// anywhere Codealong can read, so they're answered in Codex.
pub struct Codex;

impl crate::harness::Adapter for Codex {
    fn harness(&self) -> crate::model::Harness {
        crate::model::Harness::Codex
    }

    fn spec(&self) -> &'static crate::detect::Spec {
        const SPEC: crate::detect::Spec = crate::detect::Spec { harness: "codex", program: Some("codex"), default_command: None, var: Some(("CODEX_HOME", None)), default_dir: "~/.codex", siblings: Some(".codex") };
        &SPEC
    }

    /// Its sign-in is `auth.json` in its folder.
    fn signed_in(&self, home: &Path) -> (bool, Option<String>) {
        (home.join("auth.json").is_file(), None)
    }

    /// Codex wants its folder to exist before it will use it.
    fn sign_in_command(&self, account: &crate::config::AccountCfg) -> String {
        format!("mkdir -p {} && {} login", account.home, crate::usage::codex_cli(account.command()))
    }

    fn installed(&self, _home: &Path, on_path: &dyn Fn(&str) -> bool) -> bool {
        on_path("codex") || crate::detect::codex_app_cli().is_some()
    }

    fn folder_use(&self, dir: &Path) -> crate::harness::FolderUse {
        let sessions = dir.join("sessions");
        crate::harness::FolderUse {
            has_data: sessions.is_dir() || dir.join("auth.json").is_file(),
            last_used: [dir.join("session_index.jsonl"), latest_day(&sessions).unwrap_or_default()].iter().filter_map(|p| mtime_ms(p)).max(),
            signed_in: false,
        }
    }

    fn skills_dir(&self, account: &crate::config::AccountCfg) -> Option<PathBuf> {
        Some(account.home_path().join("skills"))
    }

    fn acp_command(&self, account: &crate::config::AccountCfg) -> std::result::Result<(String, Vec<(String, String)>), String> {
        let program = crate::acp::package_program("codex-acp", self.harness())?;
        Ok((program, vec![("CODEX_HOME".into(), account.home_path().to_string_lossy().into_owned())]))
    }

    /// The Codex app runs the account in `~/.codex`, where its threads usually start.
    fn places(&self, account: &crate::config::AccountCfg, home: &Path) -> std::result::Result<Vec<crate::start::Place>, String> {
        use crate::start::Place;
        Ok(if account.home_path() == home.join(".codex") { vec![Place::CodexApp, Place::Terminal, Place::Codealong] } else { vec![Place::Terminal, Place::Codealong] })
    }

    fn session_cwds(&self, home: &Path, since: i64) -> Vec<String> {
        crate::util::parallel_map(rollout_files(home, since, &HashSet::new()), |(p, _)| read_first_line(&p, 4 * 1024 * 1024).and_then(|l| parse_meta(&l)).map(|m| m.cwd))
    }

    /// Its rollout, by the id in the file name, archived or not.
    fn records(&self, account: &crate::config::AccountCfg, s: &crate::model::Session) -> Vec<PathBuf> {
        let suffix = format!("-{}.jsonl", s.id);
        let mut out = Vec::new();
        for root in ["sessions", "archived_sessions"] {
            walk_files(&account.home_path().join(root), 4, &mut |p| {
                if p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("rollout-") && n.ends_with(&suffix)) {
                    out.push(p.to_path_buf());
                }
            });
        }
        out
    }

    fn conversation(&self, ctx: &crate::harness::Ctx, s: &crate::model::Session, before: Option<u64>, limit: usize) -> anyhow::Result<Page> {
        crate::transcript::file_page(&record(self, ctx, s)?, conversation_items, before, limit)
    }

    /// A thread in a terminal is typed into; any other goes through the app.
    fn routes(&self, _account: &crate::config::AccountCfg, s: &crate::model::Session) -> Vec<crate::send::Route> {
        // A thread open in a terminal gets it typed there; one made in the CLI and closed
        // resumes in a new terminal; the app's own go to the app.
        match (s.surface, s.pid) {
            (crate::model::Surface::Cli, Some(pid)) => vec![crate::send::Route::Type { pid }],
            (crate::model::Surface::Cli, None) => vec![crate::send::Route::Resume],
            _ => vec![crate::send::Route::CodexApp],
        }
    }

    fn conversation_after(&self, ctx: &crate::harness::Ctx, s: &crate::model::Session, after: u64) -> anyhow::Result<Page> {
        crate::transcript::file_newer(&record(self, ctx, s)?, conversation_items, after)
    }

    fn open_calls(&self, ctx: &crate::harness::Ctx, s: &crate::model::Session) -> Vec<String> {
        record(self, ctx, s).map(|p| open_calls(&p)).unwrap_or_default()
    }

    /// A terminal's thread runs its commands under that terminal's `codex`; the app's threads
    /// have no process of their own.
    fn runs_its_commands(&self) -> bool {
        true
    }

    /// `codex fork <id>` opens its terminal UI on a copy of the thread, an app's thread too.
    /// Codex picks the copy's id, so it shows on the board once it's written.
    fn fork(&self, ctx: &crate::harness::Ctx, s: &crate::model::Session) -> anyhow::Result<crate::harness::Typed> {
        Ok(fork_of(ctx.account, &s.id))
    }

    /// Codex has no background of its own: Codealong runs the copy over the account's ACP
    /// adapter, which forks the thread through its app-server (`acp::fork`).
    fn fork_background(&self, ctx: &crate::harness::Ctx, s: &crate::model::Session, cwd: &Path, prompt: &str) -> anyhow::Result<Option<String>> {
        crate::acp::fork(ctx.account, &s.id, cwd, prompt).map(Some)
    }

    /// An ephemeral fork of the thread on an app-server of Codealong's own, as Codex's `/side`
    /// makes one (`SideThread`), on the model the thread last ran on and with its sandbox and
    /// approval policy, so the copy may do what the thread may.
    fn side(&self, ctx: &crate::harness::Ctx, s: &crate::model::Session, cwd: &Path) -> anyhow::Result<crate::side::Recipe> {
        let turn = self.records(ctx.account, s).first().map(|p| last_turn(p)).unwrap_or_default();
        Ok(crate::side::Recipe {
            program: crate::appserver::codex_binary(),
            args: ["app-server", "--listen", "stdio://"].iter().map(|a| a.to_string()).collect(),
            env: vec![("CODEX_HOME".into(), ctx.account.home_path().to_string_lossy().into_owned())],
            unset: Vec::new(),
            dialect: Box::new(SideThread::new(&s.id, cwd, turn)),
        })
    }

    /// Names the thread through Codex's app-server, as the Codex app does (`appserver::set_name`).
    /// The board reads names from the thread index; the app shows the name the next time it
    /// reads its threads.
    fn rename(&self, ctx: &crate::harness::Ctx, s: &crate::model::Session, name: &str) -> anyhow::Result<Vec<PathBuf>> {
        if s.archived {
            anyhow::bail!("Codex can't rename an archived thread. Unarchive it first.");
        }
        let home = ctx.account.home_path();
        crate::appserver::set_name(&home, &s.id, name)?;
        Ok(vec![home.join("session_index.jsonl")])
    }

    /// The app's own Stop, through its windows' socket; Escape in a terminal, the key its
    /// terminal UI stops a turn with.
    fn interrupts(&self, ctx: &crate::harness::Ctx, s: &crate::model::Session) -> bool {
        match (s.surface, s.pid) {
            (crate::model::Surface::Cli, Some(pid)) => crate::launch::terminal_kind(pid).is_some_and(|k| k.presses_keys()),
            (crate::model::Surface::Cli, None) => false,
            _ => ctx.account.home_path().join("ipc/ipc.sock").exists(),
        }
    }

    fn interrupt(&self, ctx: &crate::harness::Ctx, s: &crate::model::Session) -> anyhow::Result<String> {
        match (s.surface, s.pid) {
            (crate::model::Surface::Cli, Some(pid)) => {
                let Some(host) = crate::launch::terminal_host(pid, Path::new(&s.cwd))? else {
                    anyhow::bail!("It isn't in a terminal Codealong can press keys in ({}). Open it to stop it there.", crate::launch::PRESSES_KEYS_IN);
                };
                crate::launch::press_key(&host, crate::launch::Key::Escape)?;
            }
            (crate::model::Surface::Cli, None) => anyhow::bail!("It isn't running any more."),
            _ => crate::send::codex_interrupt_turn(&ctx.account.home_path(), &s.id)?,
        }
        Ok(format!("Stopped “{}”.", s.title))
    }

    /// `codex resume <id> <prompt>`; its files go as paths, and `--` keeps a prompt that starts
    /// with a dash from reading as an option.
    fn resume(&self, account: &AccountCfg, id: &str, text: &str) -> anyhow::Result<(String, Vec<(String, String)>)> {
        let input = format!("{} resume -- {id} \"${}\"\n", account.command(), crate::start::PROMPT_VAR);
        Ok((input, vec![(crate::start::PROMPT_VAR.to_string(), crate::attachments::expand(text, crate::attachments::Style::Path).trim().to_string())]))
    }

    fn app(&self) -> &'static str {
        "the Codex app"
    }

    /// A terminal's thread opens in its tab, else `codex resume` opens a new one; the Codex app
    /// reads the same threads, for the account it runs.
    fn targets(&self, s: &Session, o: &crate::harness::Opens) -> Vec<crate::model::Target> {
        let (term, who) = (o.term, o.who.clone());
        let mut t = Vec::new();
        match s.surface {
            crate::model::Surface::Cli => {
                t.push(o.terminal(s, who.clone(), false));
                if o.first && s.app_account {
                    let (why, busy) = if s.pid.is_some() { (format!("Open in {term}, finish there first"), true) } else { (who, false) };
                    t.push(o.way(s, "codex", "app", "Open in Codex".into(), why, false, busy));
                }
            }
            crate::model::Surface::App => {
                let verb = if o.respond { "Respond in" } else { "Open in" };
                t.push(o.way(s, "codex", "app", format!("{verb} Codex"), who.clone(), true, false));
                if o.first {
                    let (why, busy) = if s.live { ("Working in the Codex app".to_string(), true) } else { (who, false) };
                    t.push(o.way(s, "resume", "terminal", format!("Resume in {term}"), why, false, busy));
                }
            }
        }
        t
    }

    fn open(&self, s: &Session, action: &str) -> anyhow::Result<()> {
        match action {
            "codex" => crate::launch::open_codex(&crate::launch::codex_thread_url(&s.id)),
            other => anyhow::bail!("Unknown action {other:?}"),
        }
    }

    fn app_bundle(&self, action: &str) -> Option<&'static str> {
        (action == "codex").then_some(APP_BUNDLE_ID)
    }

    fn search_files(&self, account: &AccountCfg, since: i64) -> Vec<(PathBuf, String, i64)> {
        let files = rollout_files(&account.home_path(), since, &HashSet::new());
        files.into_iter().filter_map(|(p, m)| thread_id_of(&p).map(|id| (p, Session::key_for(Harness::Codex, &account.name, &id), m))).collect()
    }

    fn prose(&self, line: &[u8]) -> Option<(i64, bool, String)> {
        let has = |needle: &[u8]| memchr::memmem::find(line, needle).is_some();
        if !has(b"agent_message") && !has(b"user_message") && !has(b"AgentMessage") && !has(b"UserMessage") {
            return None;
        }
        let v: Value = serde_json::from_slice(line).ok()?;
        let at = v.get("timestamp").and_then(Value::as_str).and_then(parse_rfc3339_ms).unwrap_or(0);
        prose_of(&v).map(|(you, t)| (at, you, t))
    }

    // ---- Starting a thread ----

    /// Codex picks its own thread id. Its images go with `-i`, which takes several, so `--`
    /// ends them before the prompt; other files are paths it can read.
    fn start_in_terminal(&self, account: &AccountCfg, ask: &crate::harness::StartAsk) -> crate::harness::Typed {
        use crate::attachments::{self, Style};
        let prompt = attachments::expand_in(ask.prompt, ask.images_dir, ask.files_dir, Style::Path);
        let prompt = prompt.trim();
        let mut input = account.command().to_string();
        let mut env = Vec::new();
        for rel in attachments::referenced(ask.prompt).iter().filter(|r| r.starts_with("images/")) {
            if let Some(name) = rel.strip_prefix("images/") {
                input.push_str(&format!(" -i {}", crate::util::sh_quote(&ask.images_dir.join(name).to_string_lossy())));
            }
        }
        if !prompt.is_empty() {
            input.push_str(&format!(" -- \"${}\"", crate::start::PROMPT_VAR));
            env.push((crate::start::PROMPT_VAR.to_string(), prompt.to_string()));
        }
        input.push('\n');
        crate::harness::Typed { input, env, key: None }
    }

    /// `codex://threads/new` opens the app's new-thread page with the folder and prompt filled
    /// in; a folder the app has no project for becomes one.
    fn app_link(&self, folder: &Path, ask: &crate::harness::StartAsk) -> Option<String> {
        let prompt = crate::attachments::expand_in(ask.prompt, ask.images_dir, ask.files_dir, crate::attachments::Style::Path);
        Some(crate::start::with_param(format!("codex://threads/new?path={}", urlencoding::encode(&folder.to_string_lossy())), "prompt", prompt.trim()))
    }

    /// The app's new-thread link only fills a prompt in, so on a prompt Codealong makes the
    /// thread itself, and the app, once it has the thread open, runs the first turn as if you'd
    /// sent it there. When that fails the link still fills the prompt in: in the thread just made
    /// if there is one, else on the new-thread page.
    fn start_in_app(&self, account: &AccountCfg, lane: &str, folder: &Path, prompt: &str, link: &str) -> anyhow::Result<crate::start::Started> {
        use crate::start::Started;
        if prompt.trim().is_empty() {
            crate::launch::open_codex(link)?;
            return Ok(Started { key: None, message: Some(format!("A new Codex thread is open in {lane}.")) });
        }
        let waiting = || Some("Your prompt is waiting in a new Codex thread. Send it from there.".to_string());
        let home = account.home_path();
        let text = crate::attachments::expand(prompt.trim(), crate::attachments::Style::Path);
        let thread = match crate::appserver::new_thread(&home, folder, None) {
            Ok(t) => t,
            Err(_) => {
                crate::launch::open_codex(link)?;
                return Ok(Started { key: None, message: waiting() });
            }
        };
        let key = Some(Session::key_for(Harness::Codex, &account.name, &thread.id));
        let model = thread.model.as_deref().map(|m| (m, thread.effort.as_deref()));
        match crate::send::codex_start_turn(&home, &thread.id, &text, model) {
            Ok(()) => Ok(Started { key, message: None }),
            Err(_) => {
                crate::launch::open_codex(&format!("{}?prompt={}", crate::launch::codex_thread_url(&thread.id), urlencoding::encode(&text)))?;
                Ok(Started { key, message: waiting() })
            }
        }
    }

    fn app_projects(&self, account: &AccountCfg) -> Vec<String> {
        app_project_roots(&account.home_path())
    }

    /// A thread the app starts outside a project runs in a folder it makes for it.
    fn chats(&self) -> Option<(PathBuf, &'static str)> {
        Some((chats_dir(), "Codex chats"))
    }

    /// Codex archives a thread itself, so the Codex app shows it too.
    fn archive(&self, account: &AccountCfg, id: &str, archived: bool) -> anyhow::Result<()> {
        crate::appserver::set_archived(&account.home_path(), id, archived)
    }

    fn reader(&self, _wake: crate::harness::Wake) -> Box<dyn crate::harness::Reader> {
        Box::new(Reader::default())
    }
}

/* ---- Reading threads for the board ---- */

/// Codex's threads for the board (`harness::Reader`): each account's thread names, the rollouts
/// read so far, and the terminals that show threads.
#[derive(Default)]
pub struct Reader {
    /// Thread names by account (`session_index.jsonl`).
    index: HashMap<String, HashMap<String, (String, i64)>>,
    /// Each rollout with its account, what it said, and how far it's been read. `None` marks a
    /// thread that's never shown (a guardian review, a subagent), so its constant writes are
    /// ignored.
    rollouts: HashMap<PathBuf, (String, Option<(ThreadInfo, u64)>)>,
    /// Codex's terminal UIs: which threads are open in a terminal, and which one.
    terminals: Vec<Terminal>,
}

impl Reader {
    /// Loads the threads archived from Codealong from `archived_sessions`, so its Archived list
    /// can show them, and drops ones that were unarchived or are no longer listed.
    fn refresh_archived(&mut self, accounts: &[AccountCfg], look: &Look) {
        for a in accounts {
            let dir = a.home_path().join("archived_sessions");
            let ids = look.seen.archived_ids("codex", &a.name);
            self.rollouts.retain(|path, (acct, _)| acct != &a.name || !path.starts_with(&dir) || (path.exists() && thread_id_of(path).map(|id| ids.contains(&id)).unwrap_or(false)));
            for (path, m) in archived_rollout_files(&a.home_path(), &ids) {
                if !self.rollouts.contains_key(&path) {
                    let parsed = parse_rollout(&path, m, None);
                    self.rollouts.insert(path, (a.name.clone(), parsed));
                }
            }
        }
    }

    /// FSEvents can hold back appends until a writer closes its file. Codex keeps rollouts open
    /// across turns, so their lengths are checked too, and only bytes not yet read are read.
    fn poll_rollouts(&mut self, accounts: &[AccountCfg], look: &Look) -> Applied {
        let mut out = Applied::default();
        for a in accounts {
            let home = a.home_path();
            let sessions = home.join("sessions");
            let mut paths: BTreeSet<PathBuf> = rollout_files(&home, look.since, &look.seen.pinned_ids("codex", &a.name)).into_iter().map(|(p, _)| p).collect();
            // Include known files so a removal or truncation is noticed as well.
            paths.extend(self.rollouts.iter().filter(|(p, (acct, _))| acct == &a.name && p.starts_with(&sessions)).map(|(p, _)| p.clone()));
            for p in paths {
                let unread = match self.rollouts.get(&p) {
                    Some((_, Some((_, offset)))) => std::fs::metadata(&p).map(|m| m.len() != *offset).unwrap_or(true),
                    // Guardian reviews and subagents still don't need to be parsed.
                    Some((_, None)) => false,
                    None => true,
                };
                if unread {
                    self.read_rollout(&a.name, &p, &mut out);
                }
            }
            let index = read_index(&home);
            if self.index.get(&a.name) != Some(&index) {
                self.index.insert(a.name.clone(), index);
                out.dirty = true;
            }
        }
        out
    }

    /// Reads on in a rollout that changed, or for the first time in a new one.
    fn read_rollout(&mut self, account: &str, p: &Path, out: &mut Applied) {
        let prev = match self.rollouts.get(p) {
            // A thread that's never shown: skip its writes.
            Some((_, None)) => return,
            Some((_, Some(parsed))) => Some(parsed.clone()),
            None => None,
        };
        let is_new = prev.is_none();
        let from = prev.as_ref().map(|(_, off)| *off);
        match mtime_ms(p) {
            Some(m) => {
                let parsed = parse_rollout(p, m, prev);
                out.added |= is_new && parsed.is_some();
                if let (Some(from), Some((th, to))) = (from, parsed.as_ref()) {
                    out.lines.push(NewLines { path: p.to_path_buf(), from, to: *to, key: Session::key_for(Harness::Codex, account, &th.id) });
                }
                self.rollouts.insert(p.to_path_buf(), (account.to_string(), parsed));
            }
            None => {
                self.rollouts.remove(p);
            }
        }
        out.dirty = true;
    }
}

impl crate::harness::Reader for Reader {
    fn harness(&self) -> Harness {
        Harness::Codex
    }

    fn scan(&mut self, accounts: &[AccountCfg], look: &Look) {
        self.index.clear();
        self.rollouts.clear();
        for a in accounts {
            let home = a.home_path();
            self.index.insert(a.name.clone(), read_index(&home));
            let files = rollout_files(&home, look.since, &look.seen.pinned_ids("codex", &a.name));
            for (path, parsed) in parallel_map(files, |(p, m)| {
                let parsed = parse_rollout(&p, m, None);
                Some((p, parsed))
            }) {
                self.rollouts.insert(path, (a.name.clone(), parsed));
            }
        }
        if !accounts.is_empty() {
            self.terminals = terminals();
        }
        self.refresh_archived(accounts, look);
    }

    fn watch(&self, account: &AccountCfg) -> Vec<(PathBuf, bool)> {
        let home = account.home_path();
        vec![(home.clone(), false), (home.join("sessions"), true)]
    }

    fn owns(&self, account: &AccountCfg, path: &Path) -> bool {
        path.starts_with(account.home_path())
    }

    fn apply(&mut self, account: &AccountCfg, paths: &[PathBuf], _look: &Look) -> Applied {
        let home = account.home_path();
        let (index, sessions) = (home.join("session_index.jsonl"), home.join("sessions"));
        let mut out = Applied::default();
        if paths.contains(&index) {
            self.index.insert(account.name.clone(), read_index(&home));
            out.dirty = true;
        }
        for p in paths.iter().filter(|p| p.starts_with(&sessions) && p.extension().and_then(|x| x.to_str()) == Some("jsonl")) {
            self.read_rollout(&account.name, p, &mut out);
        }
        out
    }

    fn tick(&mut self, accounts: &[AccountCfg], look: &Look) -> Applied {
        let mut out = self.poll_rollouts(accounts, look);
        if !accounts.is_empty() {
            let fresh = terminals();
            out.dirty |= fresh != self.terminals;
            self.terminals = fresh;
        }
        out
    }

    fn archives_changed(&mut self, accounts: &[AccountCfg], look: &Look) {
        self.refresh_archived(accounts, look);
    }

    fn sessions(&self, account: &AccountCfg, look: &Look) -> Vec<Session> {
        let index = self.index.get(&account.name);
        // Codex decides what's archived: its archived threads live in `archived_sessions`.
        let archived_dir = account.home_path().join("archived_sessions");
        let mine = || self.rollouts.iter().filter_map(|(path, (acct, info))| info.as_ref().filter(|_| acct == &account.name).map(|(th, _)| (path, th)));
        let threads: Vec<(&str, &str, i64, i64, bool)> = mine().map(|(_, th)| (th.id.as_str(), th.cwd.as_str(), th.started_at.unwrap_or(0), th.last_ts, th.cli)).collect();
        let terminals = pair_terminals(&self.terminals, &threads);
        mine()
            .map(|(path, th)| {
                let key = Session::key_for(Harness::Codex, &account.name, &th.id);
                let name = index.and_then(|i| i.get(&th.id)).map(|(n, _)| n.as_str());
                let mut s = crate::board::codex_session(account, th, name, terminals.get(&th.id).copied(), look.seen.seen_at(&key), look.now);
                s.archived = path.starts_with(&archived_dir);
                s
            })
            .collect()
    }

    fn cwds(&self) -> Vec<String> {
        self.rollouts.values().filter_map(|(_, t)| t.as_ref().map(|(t, _)| t.cwd.clone())).collect()
    }

    fn logs(&self) -> Vec<(PathBuf, String)> {
        self.rollouts.iter().filter_map(|(p, (a, parsed))| parsed.as_ref().map(|(th, _)| (p.clone(), Session::key_for(Harness::Codex, a, &th.id)))).collect()
    }

    /// Codex's limits come with each turn, in the rollouts read anyway.
    fn usage(&self, account: &AccountCfg, now: i64) -> Option<Usage> {
        let reports = self.rollouts.values().filter(|(acct, _)| acct == &account.name).filter_map(|(_, p)| p.as_ref().map(|(th, _)| &th.rate_limits));
        Some(crate::usage::codex(account, reports, now))
    }
}

fn fork_of(account: &crate::config::AccountCfg, id: &str) -> crate::harness::Typed {
    crate::harness::Typed { input: format!("{} fork {id}\n", account.command()), env: Vec::new(), key: None }
}

/// The inputs of the calls at the end of a rollout with no output yet: a call is written there
/// before it runs, and waits there while the app asks you to approve it.
fn open_calls(path: &Path) -> Vec<String> {
    let Ok(page) = crate::transcript::file_page(path, call_items, None, 30) else { return Vec::new() };
    // A turn that ended or was stopped leaves no call running.
    let turn = page.items.iter().rposition(|i| i.kind == "note").map_or(0, |i| i + 1);
    crate::transcript::open_tools(&page.items[turn..]).into_iter().filter_map(|i| i.input.clone()).collect()
}

/// A rollout line as the tool calls in it: a call (`tool`, its code or arguments whole as the
/// input), its output (`result`), or a turn starting or ending (`note`).
fn call_items(v: &Value, at: i64) -> Vec<Item> {
    if is_turn_event(v) {
        return vec![item(at, "note", "")];
    }
    let Some(p) = (v.get("type").and_then(Value::as_str) == Some("response_item")).then(|| v.get("payload")).flatten() else { return Vec::new() };
    let id = p.get("call_id").and_then(Value::as_str).map(String::from);
    match p.get("type").and_then(Value::as_str) {
        Some("function_call" | "custom_tool_call") => {
            let input = p.get("input").or_else(|| p.get("arguments")).and_then(Value::as_str).map(String::from);
            vec![Item { id, input, ..item(at, "tool", "") }]
        }
        Some("function_call_output" | "custom_tool_call_output") => vec![Item { id, ..item(at, "result", "") }],
        _ => Vec::new(),
    }
}

fn record(a: &Codex, ctx: &crate::harness::Ctx, s: &crate::model::Session) -> anyhow::Result<PathBuf> {
    use crate::harness::Adapter;
    a.records(ctx.account, s).into_iter().next().ok_or_else(|| anyhow::anyhow!("Its rollout isn't on this Mac any more."))
}

fn walk_files(dir: &Path, depth: usize, visit: &mut dyn FnMut(&Path)) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            if depth > 0 {
                walk_files(&p, depth - 1, visit);
            }
        } else {
            visit(&p);
        }
    }
}

// ---- Terminal sessions ----

/// A Codex terminal UI running on this Mac.
#[derive(Debug, Clone, PartialEq)]
pub struct Terminal {
    pub pid: u32,
    pub cwd: PathBuf,
    /// The thread it was started on (`codex resume <id>`), when it was.
    pub thread: Option<String>,
    pub started_at: i64,
}

/// Codex's interactive terminals on this Mac. Its daemon, `exec` runs and the app's own
/// app-server aren't terminals; neither is anything run from inside an app bundle.
pub fn terminals() -> Vec<Terminal> {
    let Ok(out) = crate::procs::run("/bin/ps", &["-axo", "pid=,command="], None, std::time::Duration::from_secs(3)) else { return Vec::new() };
    out.lines()
        .filter_map(|line| {
            let (pid, command) = line.trim().split_once(' ')?;
            let pid: u32 = pid.parse().ok()?;
            let (thread, dir) = terminal_args(command)?;
            let cwd = dir.or_else(|| crate::opencode::cwd_of(pid))?;
            let cwd = std::fs::canonicalize(&cwd).unwrap_or(cwd);
            Some(Terminal { pid, cwd, thread, started_at: crate::procs::proc_start_ms(pid)? })
        })
        .collect()
}

/// For a process command line that is Codex's interactive UI: the thread it resumed, and the
/// folder `-C` gave it. None for anything else. `ps` joins the arguments with spaces, so a
/// prompt reads as words; only the first word that isn't an option matters.
pub fn terminal_args(command: &str) -> Option<(Option<String>, Option<PathBuf>)> {
    let mut words = command.split_whitespace().peekable();
    let program = words.next()?;
    if program.rsplit('/').next() != Some("codex") || program.contains(".app/") || program.contains("/packages/") {
        return None;
    }
    let mut dir = None;
    while let Some(w) = words.next() {
        match w {
            "-C" | "--cd" => dir = words.next().map(crate::util::expand),
            "-c" | "--config" | "--enable" | "--disable" | "--remote" | "--remote-auth-token-env" | "-m" | "--model" | "--local-provider" | "-p" | "--profile" | "-s" | "--sandbox" | "--add-dir" | "-a" | "--ask-for-approval" => {
                words.next();
            }
            // Images take several values, up to the next option.
            "-i" | "--image" => {
                while words.peek().is_some_and(|n| !n.starts_with('-')) {
                    words.next();
                }
            }
            "--" => return Some((None, dir)),
            _ if w.starts_with('-') => {}
            "resume" | "fork" => {
                let rest: Vec<&str> = words.collect();
                let id = rest.iter().find(|a| looks_like_uuid(a)).map(|a| a.to_string());
                let id = if w == "resume" { id } else { None };
                let dir = dir.or_else(|| rest.windows(2).find(|p| p[0] == "-C" || p[0] == "--cd").map(|p| crate::util::expand(p[1])));
                return Some((id, dir));
            }
            "exec" | "e" | "review" | "login" | "logout" | "mcp" | "plugin" | "app-server" | "remote-control" | "app" | "completion" | "update" | "doctor" | "sandbox" | "debug" | "apply" | "a" | "queue" | "archive" | "delete" | "migrate-rollouts" | "unarchive" | "cloud" | "exec-server" | "features" | "help" | "agents" | "mcp-server" | "proto" | "responses-api-proxy" => return None,
            // The prompt.
            _ => return Some((None, dir)),
        }
    }
    Some((None, dir))
}

fn looks_like_uuid(s: &str) -> bool {
    s.len() == 36 && s.chars().all(|c| c.is_ascii_hexdigit() || c == '-') && s.matches('-').count() == 4
}

/// Which terminal shows each thread. One started on a thread (`resume <id>`) shows it; the rest
/// show their folder's CLI threads that were active since they started, the newest terminal the
/// newest thread. `threads` is (id, folder, started, last activity, made in the CLI).
pub fn pair_terminals(terminals: &[Terminal], threads: &[(&str, &str, i64, i64, bool)]) -> HashMap<String, u32> {
    let mut out: HashMap<String, u32> = HashMap::new();
    for t in terminals {
        if let Some(id) = &t.thread {
            out.insert(id.clone(), t.pid);
        }
    }
    let mut folders: HashMap<&Path, Vec<&Terminal>> = HashMap::new();
    for t in terminals.iter().filter(|t| t.thread.is_none()) {
        folders.entry(t.cwd.as_path()).or_default().push(t);
    }
    for (dir, mut terms) in folders {
        terms.sort_by_key(|t| std::cmp::Reverse(t.started_at));
        let mut candidates: Vec<&(&str, &str, i64, i64, bool)> = threads.iter().filter(|th| th.4 && Path::new(th.1) == dir && !out.contains_key(th.0)).collect();
        candidates.sort_by_key(|th| std::cmp::Reverse(th.2));
        for term in terms {
            // A thread quiet since before the terminal started isn't the one it shows.
            if let Some(i) = candidates.iter().position(|th| th.3 >= term.started_at) {
                out.insert(candidates.remove(i).0.to_string(), term.pid);
            }
        }
    }
    out
}

/// Codex's newest `sessions/YYYY/MM/DD` folder, whose time is the latest new thread's.
fn latest_day(sessions: &Path) -> Option<PathBuf> {
    let mut dir = sessions.to_path_buf();
    for _ in 0..3 {
        let mut kids: Vec<String> = std::fs::read_dir(&dir).ok()?.flatten().filter(|e| e.path().is_dir()).map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        kids.sort();
        dir = dir.join(kids.pop()?);
    }
    Some(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tells_terminals_from_other_codex_processes() {
        let id = "01a0e375-0b36-77c1-8bf9-908e396b259e";
        assert_eq!(terminal_args("codex Reply with just the word pong."), Some((None, None)));
        assert_eq!(terminal_args("/opt/homebrew/bin/codex"), Some((None, None)));
        assert_eq!(terminal_args("codex -m gpt-5 -C /r/spur -i /a.png /b.png -- fix it"), Some((None, Some(PathBuf::from("/r/spur")))));
        assert_eq!(terminal_args(&format!("codex resume {id} carry on")), Some((Some(id.into()), None)));
        assert_eq!(terminal_args("codex resume --last"), Some((None, None)));
        assert_eq!(terminal_args("codex resume the work"), Some((None, None)));
        assert_eq!(terminal_args("codex exec fix it"), None);
        assert_eq!(terminal_args("/Users/me/.codex/packages/app-server-daemon/releases/0.157.1/bin/codex app-server --listen unix://"), None);
        assert_eq!(terminal_args("/Applications/Codex.app/Contents/Resources/codex app-server"), None);
        assert_eq!(terminal_args("claude --resume x"), None);
    }

    #[test]
    fn pairs_terminals_with_their_threads() {
        let term = |pid: u32, cwd: &str, thread: Option<&str>, at: i64| Terminal { pid, cwd: PathBuf::from(cwd), thread: thread.map(String::from), started_at: at };
        let terms = vec![term(1, "/r/a", None, 100), term(2, "/r/a", None, 200), term(3, "/r/b", Some("app-thread"), 50), term(4, "/r/c", None, 500)];
        let threads = [
            // (id, folder, started, last activity, cli)
            ("old", "/r/a", 10, 20, true),
            ("first", "/r/a", 110, 150, true),
            ("second", "/r/a", 210, 250, true),
            ("app-thread", "/r/b", 5, 60, false),
            ("from-app", "/r/c", 600, 700, false),
            ("stale", "/r/c", 10, 20, true),
        ];
        let p = pair_terminals(&terms, &threads);
        assert_eq!(p.get("second"), Some(&2));
        assert_eq!(p.get("first"), Some(&1));
        assert_eq!(p.get("old"), None, "quiet since before either terminal started");
        assert_eq!(p.get("app-thread"), Some(&3), "resumed by id, even one the app made");
        assert_eq!(p.get("from-app"), None);
        assert_eq!(p.get("stale"), None);
    }

    fn apply_events(info: &mut ThreadInfo, text: &str) {
        for v in parse_events(text) {
            apply_event(info, &v);
        }
    }

    const META_USER: &str = r#"{"timestamp":"2026-09-21T15:47:36.000Z","type":"session_meta","payload":{"id":"01a0c4a6-bc7b-7212-b766-77fcc7a15d43","cwd":"/Users/me/projects/sparkhq/spark-infra","originator":"Codex Desktop","source":"vscode","thread_source":"user","git":{"branch":"main"}}}"#;

    #[test]
    fn reads_the_app_project_folders() {
        let home = crate::fixtures::scratch("codex-state");
        assert!(app_project_roots(&home).is_empty());
        let state = r#"{"project-order":["local-a"],"local-projects":{"local-a":{"id":"local-a","name":"spur","rootPaths":["/r/spur"]},"local-b":{"id":"local-b","rootPaths":["/r/spark"]}},"electron-saved-workspace-roots":["/r/spur","/r/spark-app4"],"thread-writable-roots":{"t":["/elsewhere"]}}"#;
        std::fs::write(home.join(".codex-global-state.json"), state).unwrap();
        assert_eq!(app_project_roots(&home), vec!["/r/spark", "/r/spark-app4", "/r/spur"]);
    }

    #[test]
    fn keeps_only_threads_you_started() {
        assert!(parse_meta(META_USER).is_some());
        let guardian = r#"{"type":"session_meta","payload":{"id":"x","cwd":"/x","source":{"subagent":{"other":"guardian"}},"thread_source":"guardian_review"}}"#;
        assert!(parse_meta(guardian).is_none());
        let spawned = r#"{"type":"session_meta","payload":{"id":"x","cwd":"/x","source":{"subagent":{"thread_spawn":{}}},"thread_source":"subagent"}}"#;
        assert!(parse_meta(spawned).is_none());
    }

    #[test]
    fn reads_what_a_thread_was_forked_from_or_made_by() {
        assert_eq!(parse_meta(META_USER).unwrap().forked_from, None);
        let fork = r#"{"type":"session_meta","payload":{"id":"01a0c53b","cwd":"/x","source":"vscode","thread_source":"user","forked_from_id":"01a09b67-284e"}}"#;
        assert_eq!(parse_meta(fork).unwrap().forked_from.as_deref(), Some("01a09b67-284e"));
        // A thread the app made for another's agent names it in the delegation that opens it.
        let dir = crate::fixtures::scratch("codex-delegated");
        let path = dir.join("rollout-2026-09-09T14-07-39-01a087ff-6e3e-7832-bf7b-5caaae87716b.jsonl");
        let lines = [
            r#"{"timestamp":"2026-09-09T21:07:39.237Z","type":"session_meta","payload":{"id":"01a087ff-6e3e-7832-bf7b-5caaae87716b","cwd":"/x","originator":"Codex Desktop","source":"vscode","thread_source":"agent_created_thread"}}"#,
            r#"{"timestamp":"2026-09-09T21:07:40.000Z","type":"event_msg","payload":{"type":"task_started","turn_id":"t1"}}"#,
            r#"{"timestamp":"2026-09-09T21:07:41.000Z","type":"response_item","payload":{"type":"function_call_output","name":"create_thread","namespace":"codex_app","output":"<codex_delegation>\n  <source_thread_id>01a082d2-9712-7c01-a365-9bccbc10f876</source_thread_id>\n  <input>Enable passkeys</input>\n</codex_delegation>"}}"#,
        ];
        std::fs::write(&path, lines.join("\n") + "\n").unwrap();
        let (info, _) = parse_rollout(&path, 0, None).unwrap();
        assert_eq!(info.started_by.as_deref(), Some("01a082d2-9712-7c01-a365-9bccbc10f876"));
        assert_eq!(info.forked_from, None);
        // One you started names no one.
        std::fs::write(&path, format!("{META_USER}\n{}\n", lines[2])).unwrap();
        assert_eq!(parse_rollout(&path, 0, None).unwrap().0.started_by, None);
    }

    #[test]
    fn tracks_turns() {
        let mut info = parse_meta(META_USER).unwrap();
        let tail = r#"{"timestamp":"2026-09-21T16:00:00.000Z","type":"event_msg","payload":{"type":"user_message","message":"Tighten the photo tools"}}
{"timestamp":"2026-09-21T16:00:01.000Z","type":"event_msg","payload":{"type":"task_started","turn_id":"t"}}
{"timestamp":"2026-09-21T16:05:00.000Z","type":"event_msg","payload":{"type":"task_complete","turn_id":"t","last_agent_message":"Done. 4 files changed."}}
{"timestamp":"2026-09-21T16:06:00.000Z","type":"event_msg","payload":{"type":"task_started","turn_id":"u"}}
"#;
        apply_events(&mut info, tail);
        assert!(info.turn_open);
        assert_eq!(info.turn_started_at, parse_rfc3339_ms("2026-09-21T16:06:00.000Z"));
        assert_eq!(info.last_agent_message.as_deref(), Some("Done. 4 files changed."));
        assert_eq!(info.last_user_message.as_deref(), Some("Tighten the photo tools"));
        assert_eq!(info.last_prompt_at, parse_rfc3339_ms("2026-09-21T16:00:00.000Z"));
        assert_eq!(info.branch.as_deref(), Some("main"));

        // An answer to the agent's question isn't a new prompt.
        let reply = r#"{"timestamp":"2026-09-21T16:07:00.000Z","type":"event_msg","payload":{"type":"user_message","message":"<send_user_message_question_reply>\n[{\"answer\":\"Yes\"}]\n</send_user_message_question_reply>"}}"#;
        apply_events(&mut info, &format!("{reply}\n"));
        assert_eq!(info.last_prompt_at, parse_rfc3339_ms("2026-09-21T16:00:00.000Z"));
    }

    #[test]
    fn notices_approval_requests() {
        let mut info = parse_meta(META_USER).unwrap();
        let tail = r#"{"timestamp":"2026-09-21T16:00:01.000Z","type":"event_msg","payload":{"type":"task_started"}}
{"timestamp":"2026-09-21T16:00:02.000Z","type":"event_msg","payload":{"type":"exec_approval_request","command":["yarn","install"]}}
"#;
        apply_events(&mut info, tail);
        assert_eq!(info.pending_approval.as_deref(), Some("Run · `yarn install`"));
    }

    const TURN_STARTED: &str = r#"{"timestamp":"2026-09-24T18:03:59.377Z","ordinal":1,"type":"event_msg","payload":{"type":"task_started","turn_id":"u","started_at":1790273038}}"#;
    const PROMPT_ITEM: &str = r#"{"timestamp":"2026-09-24T18:04:01.042Z","ordinal":9,"type":"event_msg","payload":{"type":"item_completed","thread_id":"t","turn_id":"u","item":{"type":"UserMessage","id":"i","content":[{"type":"text","text":"Find the forwarded Spur issue and diagnose it\n","text_elements":[]}]}}}"#;
    const TURN_COMPLETE: &str = r#"{"timestamp":"2026-09-24T18:30:00.000Z","type":"event_msg","payload":{"type":"task_complete","turn_id":"u","last_agent_message":"Found it: the webhook retries twice."}}"#;
    const TOKEN_COUNT: &str = r#"{"timestamp":"2026-09-24T18:31:00.000Z","type":"event_msg","payload":{"type":"token_count","info":null}}"#;

    #[test]
    fn tracks_tool_calls_until_they_answer() {
        let mut info = ThreadInfo::default();
        apply_events(&mut info, r#"{"timestamp":"2026-09-25T17:33:00.000Z","type":"event_msg","payload":{"type":"task_started"}}
{"timestamp":"2026-09-25T17:33:25.000Z","type":"response_item","payload":{"type":"function_call","name":"js","namespace":"mcp__cua_repl","arguments":"{\"code\":\"await x()\",\"title\":\"Inspect the running app\"}","call_id":"call_a"}}
{"timestamp":"2026-09-25T17:33:26.000Z","type":"response_item","payload":{"type":"function_call_output","call_id":"call_a","output":[]}}
{"timestamp":"2026-09-25T17:33:50.000Z","type":"response_item","payload":{"type":"function_call","name":"js","namespace":"mcp__cua_repl","arguments":"{\"code\":\"await y()\"}","call_id":"call_b"}}
{"timestamp":"2026-09-25T17:33:51.000Z","type":"response_item","payload":{"type":"custom_tool_call","name":"exec","input":"text(1)","call_id":"call_c"}}
{"timestamp":"2026-09-25T17:33:52.000Z","type":"response_item","payload":{"type":"function_call","name":"wait_agent","arguments":"{}","call_id":"call_d"}}"#);
        let calls: Vec<_> = info.pending_calls.iter().map(|c| (c.call_id.as_str(), c.what.as_str(), c.limit_ms)).collect();
        assert_eq!(calls, vec![("call_b", "cua_repl · js", 60_000), ("call_c", "exec", 240_000)]);
        apply_events(&mut info, r#"{"timestamp":"2026-09-25T17:34:00.000Z","type":"event_msg","payload":{"type":"turn_aborted"}}"#);
        assert!(info.pending_calls.is_empty());
    }

    #[test]
    fn reads_prompts_from_items() {
        let mut info = parse_meta(META_USER).unwrap();
        apply_events(&mut info, &format!("{PROMPT_ITEM}\n"));
        assert_eq!(info.last_user_message.as_deref(), Some("Find the forwarded Spur issue and diagnose it"));
        assert_eq!(info.last_prompt_at, parse_rfc3339_ms("2026-09-24T18:04:01.042Z"));
    }

    #[test]
    fn records_where_commands_ran_and_files_changed() {
        let mut info = parse_meta(META_USER).unwrap();
        let tail = r#"{"timestamp":"2026-09-24T18:04:02.000Z","type":"event_msg","payload":{"type":"item_completed","item":{"type":"CommandExecution","command":["/bin/zsh","-lc","git status"],"cwd":"file:///Users/me/projects/sparkhq/spark-infra"}}}
{"timestamp":"2026-09-24T18:04:03.000Z","type":"event_msg","payload":{"type":"item_completed","item":{"type":"CommandExecution","command":["/bin/zsh","-lc","cd ../spark-fix && yarn test"],"cwd":"file:///Users/me/projects/sparkhq/spark-infra"}}}
{"timestamp":"2026-09-24T18:04:04.000Z","type":"event_msg","payload":{"type":"item_completed","item":{"type":"CommandExecution","command":["/bin/zsh","-lc","ls"],"cwd":"file:///Users/me/projects/my%20app"}}}
{"timestamp":"2026-09-24T18:04:05.000Z","type":"event_msg","payload":{"type":"item_completed","item":{"type":"FileChange","changes":{"/Users/me/projects/sparkhq/spark-fix/src/a.ts":{"type":"update"}}}}}
{"timestamp":"2026-09-24T18:04:06.000Z","type":"event_msg","payload":{"type":"item_completed","item":{"type":"Reasoning","id":"r"}}}
"#;
        apply_events(&mut info, tail);
        assert_eq!(
            info.work,
            [
                "/Users/me/projects/sparkhq/spark-infra",
                "/Users/me/projects/sparkhq/spark-fix",
                "/Users/me/projects/my app",
                "/Users/me/projects/sparkhq/spark-fix/src/a.ts",
            ]
        );
    }

    #[test]
    fn reads_the_request_out_of_wrapped_prompts() {
        let files = "\n# Files mentioned by the user:\n\n## shot.png: /tmp/shot.png\n\n## My request:\nWhy is this button grey?\n";
        assert_eq!(request_of(files).map(str::trim), Some("Why is this button grey?"));
        let browser = "<in-app-browser-context source=\"ambient-ui-state\">\n# In app browser:\n- Current URL: https://example.test\n</in-app-browser-context>\n\n## My request:\nIs the dev server up?";
        assert_eq!(request_of(browser).map(str::trim), Some("Is the dev server up?"));
        let heartbeat = "<heartbeat>\n  <automation_id>check-pools</automation_id>\n  <instructions>\nCheck the connection pools.\n  </instructions>\n</heartbeat>";
        assert_eq!(request_of(heartbeat).map(str::trim), Some("Check the connection pools."));
        let voice = "<realtime_delegation>\n  <input>Open a PR for this</input>\n  <transcript_delta>user: Open a PR for this</transcript_delta>\n</realtime_delegation>";
        assert_eq!(request_of(voice), Some("Open a PR for this"));
        assert_eq!(request_of("<send_user_message_question_reply>\n[{\"answer\":\"Yes\"}]\n</send_user_message_question_reply>"), None);
        assert_eq!(request_of("Tighten the photo tools"), Some("Tighten the photo tools"));
    }

    #[test]
    fn finds_the_calls_still_running_at_the_end_of_a_rollout() {
        let dir = crate::fixtures::scratch("codex-open-calls");
        let path = dir.join("rollout.jsonl");
        let line = |kind: &str, payload: &str| format!(r#"{{"timestamp":"2026-09-27T16:00:00.000Z","type":"{kind}","payload":{payload}}}"#);
        let exec = r#"text(await tools.exec_command({cmd:\"codealong whoami\"}));"#;
        let lines = [
            line("event_msg", r#"{"type":"task_started"}"#),
            line("response_item", &serde_json::json!({ "type": "custom_tool_call", "call_id": "a", "name": "exec", "input": exec }).to_string()),
            line("response_item", r#"{"type":"function_call","call_id":"b","name":"exec_command","arguments":"{\"cmd\":\"ls\"}"}"#),
            line("response_item", r#"{"type":"function_call_output","call_id":"b","output":"x"}"#),
        ];
        std::fs::write(&path, lines.join("\n") + "\n").unwrap();
        assert_eq!(open_calls(&path), vec![exec.to_string()]);
        // Once the turn ends, nothing is running, whatever went unanswered.
        std::fs::write(&path, [lines.join("\n"), line("event_msg", r#"{"type":"turn_aborted"}"#)].join("\n") + "\n").unwrap();
        assert!(open_calls(&path).is_empty());
    }

    /// A rollout with `head` more than 256KB before `tail`: a tool output longer than a read
    /// chunk, a tool result that quotes a turn event, and more output in between.
    fn long_rollout(test: &str, head: &[&str], tail: &[&str]) -> (PathBuf, PathBuf) {
        let dir = std::env::temp_dir().join(format!("codealong-{test}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let output = |n: usize| {
            format!(r#"{{"timestamp":"2026-09-24T18:05:00.000Z","type":"response_item","payload":{{"type":"custom_tool_call_output","output":"{}"}}}}"#, "x".repeat(n))
        };
        let lookalike = r#"{"timestamp":"2026-09-24T18:06:00.000Z","type":"event_msg","payload":{"type":"item_completed","item":{"type":"McpToolCall","result":{"structuredContent":{"type":"event_msg","payload":{"type":"task_complete","last_agent_message":"Not this one"}}}}}}"#;
        let mut lines = vec![META_USER.to_string()];
        lines.extend(head.iter().map(|l| l.to_string()));
        lines.push(output(1_500_000));
        lines.push(lookalike.to_string());
        lines.extend((0..10).map(|_| output(30_000)));
        lines.extend(tail.iter().map(|l| l.to_string()));
        let path = dir.join("rollout-2026-09-24T11-03-57-01a0d496-a50c-7031-b515-94579a60fb35.jsonl");
        std::fs::write(&path, lines.join("\n") + "\n").unwrap();
        (dir, path)
    }

    #[test]
    fn finds_a_turn_that_started_long_before_the_tail() {
        let (dir, path) = long_rollout("turn-open", &[TURN_STARTED, PROMPT_ITEM], &[TOKEN_COUNT]);
        let len = std::fs::metadata(&path).unwrap().len();
        let (info, off) = parse_rollout(&path, 0, None).unwrap();
        assert!(info.turn_open);
        assert_eq!(info.turn_started_at, parse_rfc3339_ms("2026-09-24T18:03:59.377Z"));
        assert_eq!(info.last_user_message.as_deref(), Some("Find the forwarded Spur issue and diagnose it"));
        assert_eq!(info.last_agent_message, None);
        assert_eq!(Some(info.last_ts), parse_rfc3339_ms("2026-09-24T18:31:00.000Z"));
        assert_eq!(off, len);

        // Later lines are read from where it stopped.
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
        writeln!(f, "{TURN_COMPLETE}").unwrap();
        drop(f);
        let (info, _) = parse_rollout(&path, 0, Some((info, off))).unwrap();
        assert!(!info.turn_open);
        assert_eq!(info.last_agent_message.as_deref(), Some("Found it: the webhook retries twice."));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn finds_a_finished_turn_and_its_prompt_before_the_tail() {
        let (dir, path) = long_rollout("turn-done", &[TURN_STARTED, PROMPT_ITEM, TURN_COMPLETE], &[TOKEN_COUNT]);
        let (info, _) = parse_rollout(&path, 0, None).unwrap();
        assert!(!info.turn_open);
        assert_eq!(info.last_complete_at, parse_rfc3339_ms("2026-09-24T18:30:00.000Z"));
        assert_eq!(info.last_agent_message.as_deref(), Some("Found it: the webhook retries twice."));
        assert_eq!(info.last_user_message.as_deref(), Some("Find the forwarded Spur issue and diagnose it"));
        assert_eq!(info.last_prompt_at, parse_rfc3339_ms("2026-09-24T18:04:01.042Z"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_turn_event_in_the_tail_wins_over_earlier_ones() {
        let (dir, path) = long_rollout("turn-tail", &[TURN_STARTED, PROMPT_ITEM], &[TURN_COMPLETE, TOKEN_COUNT]);
        let (info, _) = parse_rollout(&path, 0, None).unwrap();
        assert!(!info.turn_open);
        assert_eq!(info.last_agent_message.as_deref(), Some("Found it: the webhook retries twice."));
        assert_eq!(info.last_user_message.as_deref(), Some("Find the forwarded Spur issue and diagnose it"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn reads_thread_ids_from_file_names() {
        let p = Path::new("/x/rollout-2026-09-21T08-47-36-01a0c4a6-bc7b-7212-b766-77fcc7a15d43.jsonl");
        assert_eq!(thread_id_of(p).as_deref(), Some("01a0c4a6-bc7b-7212-b766-77fcc7a15d43"));
    }

    #[test]
    fn forks_a_thread_in_its_accounts_terminal_ui() {
        let a = crate::config::AccountCfg { name: "codex2".into(), harness: "codex".into(), home: "~/.codex2".into(), command: Some("CODEX_HOME=~/.codex2 codex".into()), start: None, preferred: false, acp: None };
        let f = fork_of(&a, "01a0e375-1c2d-7e3f-8a9b-0c1d2e3f4a5b");
        assert_eq!(f.input, "CODEX_HOME=~/.codex2 codex fork 01a0e375-1c2d-7e3f-8a9b-0c1d2e3f4a5b\n");
        assert_eq!(f.key, None);
        // The terminal it opens is paired with its folder's newest thread, not the one it forked.
        assert_eq!(terminal_args("codex fork 01a0e375-1c2d-7e3f-8a9b-0c1d2e3f4a5b"), Some((None, None)));
    }

    #[test]
    fn a_side_thread_is_copied_then_asked() {
        use crate::side::{Dialect, Event};
        use serde_json::json;
        let ran = TurnRan { model: Some("gpt-x".into()), sandbox: Some("workspace-write".into()), approval: Some("on-request".into()) };
        let mut t = SideThread::new("parent", Path::new("/w"), ran);
        assert_eq!(t.open()[0]["method"], "initialize");
        // Asked before the copy is made, the question waits.
        assert!(t.ask("Why?").is_empty());
        let out = t.hear(&json!({"id": 1, "result": {}})).1;
        assert_eq!(out[0], json!({"method": "initialized"}));
        assert_eq!(out[1]["method"], "thread/fork");
        let p = &out[1]["params"];
        // It runs as the thread last did, so it may do what the thread may.
        assert_eq!((&p["threadId"], &p["ephemeral"], &p["excludeTurns"], &p["sandbox"], &p["approvalPolicy"], &p["model"], &p["cwd"]), (&json!("parent"), &json!(true), &json!(true), &json!("workspace-write"), &json!("on-request"), &json!("gpt-x"), &json!("/w")));
        let out = t.hear(&json!({"id": 2, "result": {"thread": {"id": "copy", "ephemeral": true}}})).1;
        assert_eq!((&out[0]["method"], &out[0]["params"]["threadId"]), (&json!("thread/inject_items"), &json!("copy")));
        assert_eq!(out[0]["params"]["items"][0]["content"][0]["text"], crate::side::RULES);
        let out = t.hear(&json!({"id": 3, "result": {}})).1;
        assert_eq!(out, [json!({"id": 4, "method": "turn/start", "params": {"threadId": "copy", "input": [{"type": "text", "text": "Why?"}]}})]);
        let lines = [
            json!({"id": 4, "result": {"turn": {"id": "t1"}}}),
            json!({"method": "turn/started", "params": {"threadId": "copy"}}),
            // The thread it copied carries on, and isn't the answer.
            json!({"method": "item/agentMessage/delta", "params": {"threadId": "parent", "delta": "Meanwhile"}}),
            json!({"method": "item/started", "params": {"threadId": "copy", "item": {"type": "agentMessage", "text": ""}}}),
            json!({"method": "item/agentMessage/delta", "params": {"threadId": "copy", "delta": "Looking."}}),
            json!({"method": "item/started", "params": {"threadId": "copy", "item": {"type": "commandExecution", "command": "git log -3\n--oneline"}}}),
            json!({"method": "item/started", "params": {"threadId": "copy", "item": {"type": "mcpToolCall", "server": "docs", "tool": "search"}}}),
            json!({"method": "item/started", "params": {"threadId": "copy", "item": {"type": "agentMessage", "text": ""}}}),
            json!({"method": "item/agentMessage/delta", "params": {"threadId": "copy", "delta": "Because."}}),
            json!({"method": "turn/completed", "params": {"threadId": "copy", "turn": {"status": "completed"}}}),
        ];
        let heard: Vec<Event> = lines.iter().flat_map(|l| t.hear(l).0).collect();
        assert_eq!(heard, [Event::Said("Looking.".into()), Event::Tool("`git log -3`".into()), Event::Tool("docs · search".into()), Event::Said("\n\n".into()), Event::Said("Because.".into()), Event::Done(None)]);
        // The next question goes straight in.
        assert_eq!(t.ask("And?")[0]["id"], 5);
        // What would have to be asked of you is declined, and Codealong lends it no tools.
        assert_eq!(t.hear(&json!({"id": 9, "method": "item/commandExecution/requestApproval", "params": {}})).1, [json!({"id": 9, "result": {"decision": "decline"}})]);
        assert_eq!(t.hear(&json!({"id": 10, "method": "item/tool/call", "params": {}})).1[0]["error"]["code"], -32601);
        let failed = t.hear(&json!({"method": "turn/completed", "params": {"threadId": "copy", "turn": {"status": "failed", "error": {"message": "usage limit"}}}})).0;
        assert_eq!(failed, [Event::Done(Some("Codex: usage limit".into()))]);
        // A thread that can't be copied says why, to the question that waited.
        let mut t = SideThread::new("gone", Path::new("/w"), TurnRan::default());
        t.ask("Why?");
        let out = t.hear(&json!({"id": 1, "result": {}})).1;
        // And one that says nothing of how it ran is copied read-only, asking nothing.
        let p = &out[1]["params"];
        assert!(p.get("model").is_none());
        assert_eq!((&p["sandbox"], &p["approvalPolicy"]), (&json!("read-only"), &json!("never")));
        assert_eq!(t.hear(&json!({"id": 2, "error": {"code": -32600, "message": "no rollout found for thread id gone"}})).0, [Event::Done(Some("Codex: no rollout found for thread id gone".into()))]);
    }

    #[test]
    fn a_copy_runs_as_the_threads_latest_turn_did() {
        let dir = crate::fixtures::scratch("codex-ran");
        let rollout = dir.join("rollout.jsonl");
        let lines = [
            r#"{"timestamp":"2026-09-30T10:00:00.000Z","type":"session_meta","payload":{"id":"t"}}"#.to_string(),
            r#"{"timestamp":"2026-09-30T10:00:01.000Z","type":"turn_context","payload":{"model":"gpt-old","effort":"low","approval_policy":"never","sandbox_policy":{"type":"read-only"}}}"#.to_string(),
            r#"{"timestamp":"2026-09-30T10:00:02.000Z","type":"turn_context","payload":{"model":"gpt-new","effort":"xhigh","approval_policy":"on-request","sandbox_policy":{"type":"workspace-write","network_access":false}}}"#.to_string(),
            // A command's output that only mentions one.
            format!(r#"{{"timestamp":"2026-09-30T10:00:03.000Z","type":"response_item","payload":{{"output":"grep \"turn_context\" {}"}}}}"#, "x".repeat(2000)),
        ];
        std::fs::write(&rollout, lines.join("\n") + "\n").unwrap();
        assert_eq!(last_turn(&rollout), TurnRan { model: Some("gpt-new".into()), sandbox: Some("workspace-write".into()), approval: Some("on-request".into()) });
        // A policy that isn't one of the three names (a granular one) is left to Codex's settings.
        std::fs::write(&rollout, r#"{"timestamp":"2026-09-30T10:00:01.000Z","type":"turn_context","payload":{"model":"gpt-new","approval_policy":{"granular":{}},"sandbox_policy":{"type":"external-sandbox"}}}"#).unwrap();
        assert_eq!(last_turn(&rollout), TurnRan { model: Some("gpt-new".into()), sandbox: None, approval: None });
        std::fs::write(&rollout, &lines[0]).unwrap();
        assert_eq!(last_turn(&rollout), TurnRan::default());
        assert_eq!(last_turn(&dir.join("none.jsonl")), TurnRan::default());
    }

    #[test]
    fn polls_turns_while_the_rollout_stays_open() {
        use crate::harness::Reader as _;
        use crate::model::Status;
        use std::io::Write;

        let home = crate::fixtures::scratch("codex-poll");
        let dir = home.join("sessions/2026/09/24");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("rollout-2026-09-24T18-00-00-01a0d54c-7f02-7623-95b3-a3110c0f9672.jsonl");
        let mut file = std::fs::File::create(&path).unwrap();
        writeln!(file, r#"{{"type":"session_meta","payload":{{"id":"thread","cwd":"/codealong-test-missing-repo","thread_source":"user"}}}}"#).unwrap();
        writeln!(file, r#"{{"timestamp":"2026-09-24T18:00:00.000Z","type":"event_msg","payload":{{"type":"task_complete","last_agent_message":"Earlier result"}}}}"#).unwrap();
        let account = AccountCfg { name: "studio".into(), harness: "codex".into(), home: home.to_string_lossy().into_owned(), command: None, start: None, preferred: false, acp: None };
        let accounts = std::slice::from_ref(&account);
        let seen = crate::state::SeenState::default();
        let now = parse_rfc3339_ms("2026-09-24T18:35:00.000Z").unwrap();
        let look = Look { seen: &seen, since: now - 14 * crate::util::DAY_MS, now };
        // A startup scan has the previous completed turn. No filesystem events follow.
        let mut reader = Reader::default();
        reader.rollouts.insert(path.clone(), ("studio".into(), parse_rollout(&path, 0, None)));
        reader.index.insert("studio".into(), HashMap::new());
        assert_eq!(reader.sessions(&account, &look)[0].status, Status::Unread);

        writeln!(file, r#"{{"timestamp":"2026-09-24T18:34:00.000Z","type":"event_msg","payload":{{"type":"task_started"}}}}"#).unwrap();
        writeln!(file, r#"{{"timestamp":"2026-09-24T18:34:01.000Z","type":"event_msg","payload":{{"type":"user_message","message":"Continue the work"}}}}"#).unwrap();
        let read = reader.poll_rollouts(accounts, &look);
        let session = &reader.sessions(&account, &look)[0];
        assert_eq!(session.status, Status::Working);
        assert!(session.live);
        assert_eq!(session.excerpt.as_deref(), Some("Continue the work"));
        // The board is published again, and the engine gets the new lines to check.
        assert!(read.dirty && !read.added);
        assert_eq!(read.lines.iter().map(|l| l.key.as_str()).collect::<Vec<_>>(), ["codex:studio:thread"]);

        assert!(!reader.poll_rollouts(accounts, &look).dirty, "unchanged files don't need another update");

        // A poll between writes must leave the incomplete line for the next one.
        write!(file, r#"{{"timestamp":"2026-09-24T18:34:59.000Z","type":"event_msg","payload":{{"type":"task_complete","last_agent_message":"Done"}}}}"#).unwrap();
        reader.poll_rollouts(accounts, &look);
        assert_eq!(reader.sessions(&account, &look)[0].status, Status::Working);
        writeln!(file).unwrap();
        reader.poll_rollouts(accounts, &look);
        let session = &reader.sessions(&account, &look)[0];
        assert_eq!(session.status, Status::Unread);
        assert!(!session.live);
        assert_eq!(session.excerpt.as_deref(), Some("Done"));

        drop(file);
        std::fs::remove_file(&path).unwrap();
        reader.poll_rollouts(accounts, &look);
        assert!(reader.sessions(&account, &look).is_empty());

        // Discovery also works without a create event, including files from older day folders.
        std::fs::write(&path, "{\"type\":\"session_meta\",\"payload\":{\"id\":\"new-thread\",\"cwd\":\"/codealong-test-missing-repo\"}}\n").unwrap();
        std::fs::write(home.join("session_index.jsonl"), "{\"id\":\"new-thread\",\"thread_name\":\"New work\"}\n").unwrap();
        assert!(reader.poll_rollouts(accounts, &look).added, "a thread that's new may need a lane");
        assert_eq!(reader.sessions(&account, &look)[0].title, "New work");

        // Hidden subagent logs stay ignored even if they grow.
        reader.rollouts.insert(path.clone(), ("studio".into(), None));
        let mut hidden = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
        writeln!(hidden, "not a user thread").unwrap();
        assert!(!reader.poll_rollouts(accounts, &look).dirty);
        assert!(reader.sessions(&account, &look).is_empty());
        drop(hidden);
    }
}
