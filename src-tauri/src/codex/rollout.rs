//! Codex's threads on disk: the thread index (`<CODEX_HOME>/session_index.jsonl`) and rollouts
//! (`<CODEX_HOME>/sessions/YYYY/MM/DD/rollout-<time>-<id>.jsonl`), and what a rollout says of
//! its thread: its turns, prompts, calls and rate limits.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::model::{ModeKind, SessionMode};

use crate::util::{command_dir, file_name, mtime_ms, note_work, one_line, only_looks, parse_rfc3339_ms, read_first_line, read_range, read_tail, resolve, scan_lines_back, work_before};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ThreadInfo {
    pub id: String,
    pub cwd: String,
    pub branch: Option<String>,
    pub started_at: Option<i64>,
    pub last_ts: i64,
    /// A turn started and hasn't completed or been aborted.
    pub turn_open: bool,
    /// Its latest turn's id (`task_started`'s `turn_id`), which an approval names (asks.rs).
    pub turn_id: Option<String>,
    /// A recorded provider content-policy error or refusal, kept across later turns.
    pub flagged: bool,
    pub turn_started_at: Option<i64>,
    pub last_complete_at: Option<i64>,
    /// When its last turn was stopped (`turn_aborted` with `interrupted`) with none started
    /// since: by you, or by what ran it as it shut down, which Codex records the same way
    /// (applog.rs).
    pub interrupted_at: Option<i64>,
    pub last_agent_message: Option<String>,
    /// The end of that message, for Jev.
    pub last_agent_tail: Option<String>,
    /// The first prompt it was given, which names it where Codex hasn't, as the app-server's
    /// `preview` does ("usually the first user message in the thread").
    pub first_prompt: Option<String>,
    /// Its first prompt is further from its start than a first parse looks
    /// (`FIRST_PROMPT_WITHIN`): none is known, and none read later is taken for it.
    pub first_prompt_unread: bool,
    /// The latest prompt. This and the first are what a message asks (`send::past_sender_lines`):
    /// Codealong's messages and other sessions' are user messages here too.
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
    /// Run by `codex exec` (`source: "exec"`, originator `codex_exec`): a prompt run with nobody
    /// at it, whose output went to what ran it.
    pub exec: bool,
    /// The thread it's a fork of (`forked_from_id`): `codex fork`, or the app's Fork.
    pub forked_from: Option<String>,
    /// The model its latest turn ran on, from its `turn_context`.
    pub model: Option<String>,
    /// The reasoning effort its latest turn ran at, from its `turn_context`'s `effort`
    /// (`xhigh`): none for a model that takes none.
    pub effort: Option<String>,
    /// The permissions its latest turn ran with, from its `turn_context` (`turn_mode`).
    pub mode: Option<SessionMode>,
    /// The folder its latest turn or settings said it runs in (`turn_context`'s and
    /// `thread_settings_applied`'s `cwd`), and when: the one whose config the process that wrote
    /// them read, a resume's choice included, where `cwd` stays the folder it was made in.
    pub said_cwd: Option<(String, i64)>,
    /// The tokens in its context after its latest call and its model's window: the last
    /// `token_count`'s `last_token_usage.total_tokens` (what went in and what came out) and
    /// `model_context_window`, which Codex's own context meter is worked out from.
    pub context: Option<(u64, Option<u64>)>,
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
    /// Its input or arguments as written, its first 4,000 characters: where an approval's
    /// command is found (asks.rs).
    pub input: String,
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

/// What a rollout's calls used, for the token ledger. Each call ends in a `token_count` event
/// whose `info` has `last_token_usage`, the call's own (`input_tokens` with the cached part
/// among them as `cached_input_tokens`, `output_tokens` with `reasoning_output_tokens` among
/// them), and `total_token_usage`, the thread's running total. An event can come again with
/// the same total, and is counted once; the event's time and that total name it, so a fork's
/// copy of its thread's events is the same call. A subagent's rollout (a guardian review, a
/// spawned agent) counts from nothing. The model is the latest `turn_context`'s, and
/// `thread_settings_applied` says when its service tier is priority, Codex's fast mode, which
/// lasts until another names a tier (0.144 on; one with no tier leaves it).
pub fn token_use(line: &[u8], st: &mut crate::ledger::TokenState) -> Option<crate::ledger::TokenUse> {
    let has = |needle: &[u8]| memchr::memmem::find(line, needle).is_some();
    if !has(b"\"token_count\"") && !has(b"\"turn_context\"") && !has(b"\"session_meta\"") && !has(b"thread_settings_applied") {
        return None;
    }
    let v: Value = serde_json::from_slice(line).ok()?;
    let payload = v.get("payload")?;
    let str_of = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_string);
    match (v.get("type").and_then(Value::as_str)?, payload.get("type").and_then(Value::as_str)) {
        ("session_meta", _) => {
            st.folder = str_of(payload, "cwd").unwrap_or(std::mem::take(&mut st.folder));
            // A spawned agent's work is the thread's that spawned it.
            if let Some(parent) = payload.pointer("/source/subagent/thread_spawn/parent_thread_id").and_then(Value::as_str).filter(|p| !p.is_empty()) {
                st.parent = parent.to_string();
            }
            None
        }
        ("turn_context", _) => {
            st.folder = str_of(payload, "cwd").unwrap_or(std::mem::take(&mut st.folder));
            st.model = str_of(payload, "model").unwrap_or(std::mem::take(&mut st.model));
            None
        }
        ("event_msg", Some("thread_settings_applied")) => {
            if let Some(tier) = payload.pointer("/thread_settings/service_tier") {
                st.fast = matches!(tier.as_str(), Some("priority" | "fast"));
            }
            None
        }
        ("event_msg", Some("token_count")) => {
            let info = payload.get("info").filter(|i| i.is_object())?;
            let n = |v: &Value, k: &str| v.get(k).and_then(Value::as_u64).unwrap_or(0);
            let total = info.get("total_token_usage").map(|t| n(t, "total_tokens")).unwrap_or(0);
            if total != 0 && total == st.total {
                return None;
            }
            st.total = total;
            let last = info.get("last_token_usage")?;
            let input = n(last, "input_tokens");
            let cached = n(last, "cached_input_tokens").min(input);
            let written = n(last, "cache_write_input_tokens").min(input - cached);
            let ts = str_of(&v, "timestamp").unwrap_or_default();
            Some(crate::ledger::TokenUse {
                id: format!("{ts}:{total}"),
                at: parse_rfc3339_ms(&ts).unwrap_or(0),
                session: st.owner(),
                folder: st.folder.clone(),
                model: if st.model.is_empty() { "codex".to_string() } else { st.model.clone() },
                input: input - cached - written,
                cache_read: cached,
                cache_write: written,
                cache_write_1h: 0,
                output: n(last, "output_tokens"),
                reasoning: n(last, "reasoning_output_tokens"),
                fast: st.fast,
                searches: 0,
                cost: None,
            })
        }
        _ => None,
    }
}

/// Every rollout, archived ones too: the ledger keeps what their calls used whether or not the
/// thread is still listed.
pub fn all_rollouts(codex_home: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = rollout_files(codex_home, 0, &HashSet::new()).into_iter().map(|(p, _)| p).collect();
    let archived = std::fs::read_dir(codex_home.join("archived_sessions")).into_iter().flatten().flatten().map(|e| e.path());
    out.extend(archived.filter(|p| file_name(p).starts_with("rollout-") && p.extension().and_then(|x| x.to_str()) == Some("jsonl")));
    out
}

/// The thread id at the end of a rollout file name.
pub fn thread_id_of(path: &Path) -> Option<String> {
    let name = file_name(path);
    let stem = name.strip_suffix(".jsonl")?;
    (stem.len() >= 36).then(|| stem[stem.len() - 36..].to_string())
}

/// How much of a rollout's first line, its session meta, is read.
const META_MAX: u64 = 4 * 1024 * 1024;

/// Whether a rollout is of a thread that never shows (a guardian review, a subagent), as its
/// first line says once it's written whole, rather than one a parse found nothing in because it
/// looked too soon: Codex makes the file before it writes its session meta, and `thread/start`
/// left one empty for 2ms before its 22KB first line (0.156.1's app-server on a scratch
/// `CODEX_HOME`, 2026-10-22). A look in between, as the file appeared, took a helper Codealong
/// started for a thread that never shows (`Reader::read_rollout`). One read decides it, so a line
/// finished since the parse reads as the thread it is. A first line too long to read whole
/// never will be.
pub fn never_shown(path: &Path) -> bool {
    match read_first_line(path, META_MAX) {
        Some(line) if line.ends_with('\n') => parse_meta(&line).is_none(),
        Some(line) => line.len() as u64 >= META_MAX,
        None => false,
    }
}

/// Parses a rollout. Threads you didn't start (guardian reviews, subagents) return `None`, and
/// so does one whose session meta isn't written yet (`never_shown` tells them apart).
/// With an earlier result and the offset it stopped at, only new lines are read.
///
/// A first parse reads the last 256KB, then reads back further for whatever that missed that
/// the status depends on: a turn can run for megabytes, so the event that started it and the
/// prompt it's working on can be far behind the tail.
pub fn parse_rollout(path: &Path, mtime: i64, prev: Option<(ThreadInfo, u64)>) -> Option<(ThreadInfo, u64)> {
    let len = std::fs::metadata(path).ok()?.len();
    // A large append rebuilds the tail's status, but flags in the already-read prefix are known.
    let (was_flagged, flag_from) = prev.as_ref().filter(|(_, off)| *off <= len).map(|(info, off)| (info.flagged, *off)).unwrap_or_default();
    // `floor` is where the lines after the session meta begin, on a first parse.
    let (mut info, tail, floor) = match prev.filter(|(_, off)| *off <= len && len - off <= 1024 * 1024) {
        Some((info, off)) => (info, read_tail(path, off).ok()?, None),
        None => {
            let first = read_first_line(path, META_MAX)?;
            let mut info = parse_meta(&first)?;
            if info.id.is_empty() {
                info.id = thread_id_of(path)?;
            }
            let floor = first.len() as u64;
            if info.started_by.is_some() {
                info.started_by = delegated_by(path, floor, len);
            }
            let tail = read_tail(path, len.saturating_sub(256 * 1024).max(floor)).ok()?;
            (info, tail, Some(floor))
        }
    };
    let events = parse_events(&tail.text());
    if let Some(floor) = floor {
        // Read before the events behind the tail, whose prompt is the latest one there.
        read_first_prompt(&mut info, path, floor, tail.start, FIRST_PROMPT_WITHIN);
        for v in earlier_events(path, floor, tail.start, &events) {
            apply_event(&mut info, &v);
        }
    }
    for v in &events {
        apply_event(&mut info, v);
    }
    // Where it worked decides its worktree (`board::session_seed`). Later reads carry it forward.
    if let Some(floor) = floor {
        let cwd = info.cwd.clone();
        work_before(path, floor, tail.start, &mut info.work, |line| {
            // Cheap check first: most bytes are tool output.
            if !line.contains("\"item_completed\"") {
                return Vec::new();
            }
            serde_json::from_str::<Value>(line).ok().and_then(|v| worked_at(&v, &cwd)).into_iter().collect()
        });
    }
    info.flagged |= was_flagged;
    if let Some(floor) = floor.filter(|_| !info.flagged) {
        info.flagged = crate::providerflags::before(path, floor.max(flag_from), tail.start, &["\"error\"", "\"refusal\""], flagged);
    }
    if info.last_ts == 0 {
        info.last_ts = mtime;
    }
    Some((info, tail.end))
}

/// The events before `end` that `later` has no newer copy of: the last turn event (started,
/// completed or aborted), the last prompt, the last `turn_context` (its model and permissions)
/// and a `thread_settings_applied` newer than that (the model and level set since), oldest
/// first, and where a turn that's still open started before `end`, its calls and their results
/// in between, which say what it still waits on (`ThreadInfo::pending_calls`). Read backwards
/// only as far as needed: a turn's context comes just after its start.
fn earlier_events(path: &Path, floor: u64, end: u64, later: &[Value]) -> Vec<Value> {
    let is_context = |v: &Value| v.get("type").and_then(Value::as_str) == Some("turn_context");
    let is_settings = |v: &Value| event_payload(v).and_then(|p| p.get("type")).and_then(Value::as_str) == Some("thread_settings_applied");
    let mut need_turn = !later.iter().any(is_turn_event);
    let mut need_prompt = !later.iter().any(|v| event_payload(v).and_then(prompt_of).is_some());
    let mut need_context = !later.iter().any(is_context);
    // Settings set before the last turn's context are older than what it says.
    let mut need_settings = need_context && !later.iter().any(is_settings);
    let mut found = Vec::new();
    if !need_turn && !need_prompt && !need_context {
        return found;
    }
    // A turn that began behind `end` has its calls there too; once a turn event is found, the
    // calls before it belong to turns gone by.
    let calls = need_turn;
    scan_lines_back(path, floor, end, |line| {
        if calls && need_turn && line.contains("\"response_item\"") && (line.contains("_call\"") || line.contains("_call_output\"")) {
            if let Some(v) = call_line(line) {
                found.push(v);
            }
            return false;
        }
        // Cheap checks first: most bytes are tool output that none of these can match.
        let markers = ["\"task_started\"", "\"task_complete\"", "\"turn_aborted\"", "\"user_message\"", "\"UserMessage\"", "\"thread_settings_applied\""];
        let context = need_context && line.contains("\"turn_context\"");
        if !context && (!line.contains("\"event_msg\"") || !markers.iter().any(|m| line.contains(m))) {
            return false;
        }
        let Ok(v) = serde_json::from_str::<Value>(line) else { return false };
        if need_context && is_context(&v) {
            need_context = false;
            need_settings = false;
            found.push(v);
        } else if need_settings && is_settings(&v) {
            need_settings = false;
            found.push(v);
        } else if need_turn && is_turn_event(&v) {
            need_turn = false;
            found.push(v);
        } else if need_prompt && event_payload(&v).and_then(prompt_of).is_some() {
            need_prompt = false;
            found.push(v);
        }
        !need_turn && !need_prompt && !need_context
    });
    found.reverse();
    found
}

/// A call's line, or its result's, as little of it as `note_call` reads: its time, kind, call id,
/// tool and input. A result is most of a rollout's bytes, so only its call id is read out of it.
/// None for any other line.
fn call_line(line: &str) -> Option<Value> {
    if line.contains("_call_output\"") {
        let at = line.find("\"call_id\":\"")? + "\"call_id\":\"".len();
        let id = &line[at..at + line[at..].find('"')?];
        return Some(serde_json::json!({ "type": "response_item", "payload": { "type": "function_call_output", "call_id": id } }));
    }
    let v: Value = serde_json::from_str(line).ok()?;
    let p = v.get("payload")?;
    let kind = p.get("type").and_then(Value::as_str)?;
    if !matches!(kind, "function_call" | "custom_tool_call" | "function_call_output" | "custom_tool_call_output") {
        return None;
    }
    let keep = ["type", "call_id", "name", "input", "arguments", "namespace"];
    let payload: serde_json::Map<String, Value> = keep.iter().filter_map(|k| Some((k.to_string(), p.get(*k)?.clone()))).collect();
    Some(serde_json::json!({ "timestamp": v.get("timestamp"), "type": "response_item", "payload": payload }))
}

/// How far into a thread its first prompt is looked for, as far as Claude Code's is
/// (claude/transcript.rs). It follows the instructions and context Codex puts first, and the
/// images it came with, which are written out whole before it: of this Mac's 591 threads longer
/// than a tail, 99 in 100 had it within 230 KB, 3 past 4 MB and the farthest at 10 MB; those with
/// none before the tail had 2.2 MB there at most (2026-10-19).
const FIRST_PROMPT_WITHIN: u64 = 32 * 1024 * 1024;

/// Reads a thread's first prompt from `floor..end`, forward only as far as it and no more than
/// `within` bytes: on a first parse, for a thread whose start is behind the tail. One further,
/// or one that can't be read, stays unknown (`first_prompt_unread`) rather than taken from the
/// prompts read after.
fn read_first_prompt(info: &mut ThreadInfo, path: &Path, floor: u64, end: u64, within: u64) {
    let to = end.min(floor + within);
    match first_prompt_in(path, floor, to) {
        Ok(Some(first)) => info.first_prompt = Some(first),
        // None before the tail: the first read after is it.
        Ok(None) => info.first_prompt_unread = to < end,
        Err(_) => info.first_prompt_unread = true,
    }
}

/// The first prompt in `floor..to`, read forward only as far as it.
fn first_prompt_in(path: &Path, floor: u64, to: u64) -> std::io::Result<Option<String>> {
    use std::io::{BufRead, Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(path)?;
    f.seek(SeekFrom::Start(floor))?;
    let mut lines = std::io::BufReader::new(f.take(to.saturating_sub(floor)));
    let mut line = Vec::new();
    loop {
        line.clear();
        if lines.read_until(b'\n', &mut line)? == 0 {
            return Ok(None);
        }
        // Cheap check first: most bytes are instructions, images and tool output.
        let text = String::from_utf8_lossy(&line);
        if !text.contains("\"event_msg\"") || !(text.contains("\"user_message\"") || text.contains("\"UserMessage\"")) {
            continue;
        }
        let Ok(v) = serde_json::from_str::<Value>(&text) else { continue };
        if let Some(m) = event_payload(&v).and_then(prompt_of) {
            return Ok(Some(asked(&m)));
        }
    }
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
        exec: p.get("source").and_then(Value::as_str) == Some("exec"),
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

pub(super) fn is_turn_event(v: &Value) -> bool {
    let kind = event_payload(v).and_then(|p| p.get("type")).and_then(Value::as_str);
    matches!(kind, Some("task_started" | "task_complete" | "turn_aborted"))
}

/// The id of a message in the turn before the one your prompt at `cursor` is in: what a copy
/// that leaves that prompt out goes through, since codex-acp forks through the whole turn that
/// holds the item it's named (acp.rs `fork_point`). The ids are those `thread/read` gives its
/// items, which a rollout has in its `item_completed` lines. None when no turn comes before; an
/// error when one does but the rollout names none of its items, as an older Codex's doesn't.
pub(super) fn item_before_turn(rollout: &Path, cursor: u64) -> anyhow::Result<Option<String>> {
    // Read back from the prompt: first to its own turn's start, then to an item of an earlier turn.
    let mut in_turn = false;
    let mut earlier_turn = false;
    let mut item = None;
    scan_lines_back(rollout, 0, cursor, |line| {
        if !line.contains("\"task_started\"") && !(in_turn && line.contains("\"item_completed\"")) {
            return false;
        }
        let Ok(v) = serde_json::from_str::<Value>(line) else { return false };
        let Some(p) = event_payload(&v) else { return false };
        match p.get("type").and_then(Value::as_str) {
            Some("task_started") if !in_turn => in_turn = true,
            Some("task_started") => earlier_turn = true,
            Some("item_completed") if in_turn => {
                let said = matches!(p.pointer("/item/type").and_then(Value::as_str), Some("AgentMessage" | "UserMessage"));
                item = p.pointer("/item/id").and_then(Value::as_str).filter(|id| said && !id.is_empty()).map(String::from);
            }
            _ => {}
        }
        item.is_some()
    });
    if item.is_none() && earlier_turn {
        anyhow::bail!("This thread's record doesn't name its messages as Codex now does, so Codealong can't go back to one.");
    }
    Ok(item)
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

/// Where an event says the thread worked: a completed command or file change (`work_of`).
fn worked_at(v: &Value, thread_cwd: &str) -> Option<String> {
    let payload = event_payload(v).filter(|p| p.get("type").and_then(Value::as_str) == Some("item_completed"))?;
    work_of(payload.get("item")?, thread_cwd)
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
            // The agent picks each command's folder: one run somewhere else only to look isn't
            // work there, as with a `cd`.
            if !Path::new(&cwd).starts_with(thread_cwd) && only_looks(&script) {
                return None;
            }
            command_dir(&script, &cwd)
        }
        "FileChange" => item.get("changes")?.as_object()?.keys().next().map(|p| resolve(thread_cwd, p)),
        _ => None,
    }
}

/// A `file://` URL as a path. Codex records command folders that way.
pub(super) fn file_path_of(s: &str) -> String {
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

/// The permissions a turn ran with, from its `turn_context`, named as Codex's own status line
/// names them (0.157.1's tui `permissions_display` and `approval_mode_display`): the sandbox
/// (`sandbox_policy.type`: Read Only, Workspace, Full Access) and who answers what goes past it
/// (`approval_policy`, and `approvals_reviewer`: `auto_review` for Approve for me, a reviewer
/// agent, or `user`). The Plan collaboration mode (`collaboration_mode.mode`) changes nothing
/// whatever the sandbox. Every turn writes all of them (this Mac's 348 latest threads on
/// 2026-10-13: on-request with auto_review in workspace-write, never in read-only, a few asking
/// you or with full access); a turn that says none of them says nothing.
pub fn turn_mode(p: &Value) -> Option<SessionMode> {
    named_mode(p, turn_sandbox(p))
}

fn turn_sandbox(p: &Value) -> Option<&str> {
    p.pointer("/sandbox_policy/type").and_then(Value::as_str)
}

/// The permissions a thread's next turn runs with, as `thread_settings_applied` says them when
/// they're set: before each of the app's turns, and as soon as a pick in the terminal UI's
/// `/permissions` takes (0.162.1, 2026-10-24), well before a turn says it. They name the sandbox
/// by its permission profile, a built-in one's id starting with `:`. None for settings that name
/// no profile, which leave the sandbox unsaid.
fn settings_mode(t: &Value) -> Option<SessionMode> {
    named_mode(t, Some(settings_sandbox(t)?))
}

fn settings_sandbox(t: &Value) -> Option<&'static str> {
    Some(match t.pointer("/active_permission_profile/id").and_then(Value::as_str)? {
        ":workspace" => "workspace-write",
        ":read-only" => "read-only",
        ":danger-full-access" => "danger-full-access",
        // One config.toml sets up.
        _ => "custom",
    })
}

/// The permissions Codex's terminal UI lists under `/permissions`, in its order (0.162.1's
/// `open_permission_profiles_popup`), by the ids Codealong gives them, with the settings each
/// sets: what a session in a terminal is switched among (`Codex::mode_picker`). Approve for me
/// is listed only where its feature is on (`features.guardian_approval`, on unless config.toml
/// turns it off).
pub const PICKED: [(&str, &str, &str, &str); 4] = [
    (ASK_FOR_APPROVAL, "workspace-write", "on-request", "user"),
    (APPROVE_FOR_ME, "workspace-write", "on-request", "auto_review"),
    ("full-access", "danger-full-access", "never", "user"),
    ("read-only", "read-only", "on-request", "user"),
];

pub const ASK_FOR_APPROVAL: &str = "ask-for-approval";
pub const APPROVE_FOR_ME: &str = "approve-for-me";

/// The modes `/permissions` lists (`PICKED`), each as a session in it shows it: Approve for me
/// only where `approve_for_me` says its feature is on.
pub fn picked_modes(approve_for_me: bool) -> Vec<SessionMode> {
    PICKED
        .iter()
        .filter(|(id, ..)| approve_for_me || *id != APPROVE_FOR_ME)
        .filter_map(|(_, sandbox, approval, reviewer)| permissions(&serde_json::json!({ "approval_policy": approval, "approvals_reviewer": reviewer }), Some(sandbox)))
        .collect()
}

/// The mode a turn's settings name: Plan, whatever its permissions, else those.
fn named_mode(p: &Value, sandbox: Option<&str>) -> Option<SessionMode> {
    if p.pointer("/collaboration_mode/mode").and_then(Value::as_str) == Some("plan") {
        return Some(SessionMode::new(ModeKind::Plan, "Plan", "Plans without changing anything, until you take the plan."));
    }
    permissions(p, sandbox)
}

/// The permissions a turn's settings name, by its sandbox and who answers what goes past it,
/// with the id of `/permissions`' entry for them where they're one of those (`PICKED`).
fn permissions(p: &Value, sandbox: Option<&str>) -> Option<SessionMode> {
    let text = |at: &str| p.pointer(at).and_then(Value::as_str);
    // A policy of its own rules is an object (`{"granular": …}`).
    let approval = p.get("approval_policy").map(|a| a.as_str().unwrap_or("custom"));
    if sandbox.is_none() && approval.is_none() {
        return None;
    }
    // `guardian_subagent` is its older name, which config.toml still takes.
    let reviewer = matches!(text("/approvals_reviewer"), Some("auto_review" | "guardian_subagent"));
    let (kind, label, detail, id) = match (sandbox, approval) {
        (Some("danger-full-access"), _) => (ModeKind::Skips, "Full Access", "Edits files anywhere and uses the internet without asking.", "full-access"),
        (Some("read-only"), Some("on-request")) if reviewer => (ModeKind::ReadOnly, "Read Only", "Reads files, and a reviewer agent answers for you when it would edit files or use the internet.", "read-only"),
        (Some("read-only"), Some("on-request")) => (ModeKind::ReadOnly, "Read Only", "Reads files, and asks before it edits files or uses the internet.", "read-only"),
        (Some("read-only"), Some("never")) => (ModeKind::ReadOnly, "Read Only", "Reads files, and refuses anything more without asking.", ""),
        (_, Some("on-request")) if reviewer => (ModeKind::Auto, "Approve for me", "Edits files in its workspace and runs commands there; a reviewer agent answers for you when it would use the internet or edit other files.", APPROVE_FOR_ME),
        (Some("workspace-write"), Some("on-request")) => (ModeKind::Edits, "Ask for approval", "Edits files in its workspace and runs commands there, and asks before it uses the internet or edits other files.", ASK_FOR_APPROVAL),
        (_, Some("untrusted")) => (ModeKind::Asks, "Untrusted", "Asks before any command it doesn't know to be safe.", ""),
        (Some("workspace-write"), Some("never")) => (ModeKind::Other, "Never ask", "Edits files in its workspace and runs commands there, and refuses anything more without asking.", ""),
        _ => (ModeKind::Other, "Custom permissions", "Runs with permissions set up its own way.", ""),
    };
    Some(SessionMode::new(kind, label, detail).with_id(id))
}

/// The id of the permissions a thread has by the end of its rollout (`permissions`), from its
/// latest turn or settings that say them, within the last `max` bytes: how a switch sees itself
/// taken (`Codex::mode_said`), in Plan too, which a pick leaves as it is. Empty for permissions
/// `/permissions` doesn't list.
pub fn mode_at_end(path: &Path, max: u64) -> Option<String> {
    let tail = crate::util::read_last(path, max).ok()?;
    let text = tail.text();
    text.lines().rev().filter(|l| l.contains("turn_context") || l.contains("thread_settings_applied")).find_map(|line| {
        let v: Value = serde_json::from_str(line).ok()?;
        let mode = match v.get("type").and_then(Value::as_str) {
            Some("turn_context") => v.get("payload").and_then(|p| permissions(p, turn_sandbox(p))),
            _ => event_payload(&v).filter(|p| p.get("type").and_then(Value::as_str) == Some("thread_settings_applied")).and_then(|p| p.get("thread_settings")).and_then(|t| permissions(t, Some(settings_sandbox(t)?))),
        };
        mode.map(|m| m.id)
    })
}

/// Codex persists turn failures as `task_complete.error`; standalone ErrorEvents name the
/// same provider policy codes in `codex_error_info`. Older content-filter failures use the stream error's message. Check only that error or a typed
/// assistant refusal, never prompts, tool output, ordinary prose or opaque moderation metadata.
fn flagged(v: &Value) -> bool {
    let error = event_payload(v).and_then(|p| match p.get("type").and_then(Value::as_str) {
        Some("error") => Some(p),
        Some("task_complete") => p.get("error"),
        _ => None,
    });
    if let Some(p) = error {
        if matches!(p.get("codex_error_info").and_then(Value::as_str), Some("cyber_policy" | "bio_policy" | "misalignment_policy_violation" | "content_filter")) {
            return true;
        }
        let message = p.get("message").and_then(Value::as_str).unwrap_or_default().to_lowercase();
        return message.contains("reason: content_filter") || message.contains("response was blocked by a content filter");
    }
    v.get("type").and_then(Value::as_str) == Some("response_item")
        && v.pointer("/payload/type").and_then(Value::as_str) == Some("message")
        && v.pointer("/payload/role").and_then(Value::as_str) == Some("assistant")
        && v.pointer("/payload/content").and_then(Value::as_array).is_some_and(|parts| parts.iter().any(|p| p.get("type").and_then(Value::as_str) == Some("refusal")))
}

fn note_cwd(info: &mut ThreadInfo, p: &Value, at: Option<i64>) {
    if let (Some(cwd), Some(at)) = (p.get("cwd").and_then(Value::as_str).filter(|c| !c.is_empty()), at) {
        info.said_cwd = Some((cwd.to_string(), at));
    }
}

fn apply_event(info: &mut ThreadInfo, v: &Value) {
    info.flagged |= flagged(v);
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
    if v.get("type").and_then(Value::as_str) == Some("turn_context") {
        if let Some(m) = v.pointer("/payload/model").and_then(Value::as_str).filter(|m| !m.is_empty()) {
            info.model = Some(m.to_string());
        }
        if let Some(p) = v.get("payload") {
            note_cwd(info, p, ts);
            info.mode = turn_mode(p).or(info.mode.take());
            info.effort = p.get("effort").and_then(Value::as_str).filter(|e| !e.is_empty()).map(str::to_string);
        }
        return;
    }
    let Some(payload) = event_payload(v) else { return };
    let kind = payload.get("type").and_then(Value::as_str).unwrap_or_default();
    if matches!(kind, "task_started" | "task_complete" | "turn_aborted") {
        info.pending_calls.clear();
    }
    if let Some(p) = worked_at(v, &info.cwd) {
        note_work(&mut info.work, p);
    }
    match kind {
        "task_started" => {
            info.turn_open = true;
            info.turn_id = payload.get("turn_id").and_then(Value::as_str).map(String::from);
            info.turn_started_at = ts;
            info.pending_approval = None;
            info.interrupted_at = None;
        }
        "task_complete" => {
            info.turn_open = false;
            info.last_complete_at = ts;
            info.pending_approval = None;
            info.interrupted_at = None;
            if let Some(m) = payload.get("last_agent_message").and_then(Value::as_str).filter(|m| !m.trim().is_empty()) {
                info.last_agent_message = Some(one_line(m, 140));
                info.last_agent_tail = Some(crate::jev::tail(m, 600));
            }
        }
        "turn_aborted" => {
            info.turn_open = false;
            info.last_complete_at = ts;
            info.pending_approval = None;
            info.interrupted_at = ts.filter(|_| payload.get("reason").and_then(Value::as_str) == Some("interrupted"));
        }
        // The model, level and permissions the thread's next turn runs on, written as they're
        // set: by the app before each turn, and between turns by a pick in its box or a switch
        // from Codealong (ipc.rs `codex_settings_request`, the terminal UI's `/permissions`),
        // which no `turn_context` says until then.
        "thread_settings_applied" => {
            if let Some(t) = payload.get("thread_settings") {
                if let Some(m) = t.get("model").and_then(Value::as_str).filter(|m| !m.is_empty()) {
                    info.model = Some(m.to_string());
                }
                info.effort = t.get("reasoning_effort").and_then(Value::as_str).filter(|e| !e.is_empty()).map(str::to_string);
                info.mode = settings_mode(t).or(info.mode.take());
                note_cwd(info, t, ts);
            }
        }
        "user_message" => {
            if let Some(m) = prompt_of(payload) {
                note_prompt(info, &m, ts);
            }
        }
        "token_count" => {
            let at = ts.unwrap_or(info.last_ts);
            if let Some((id, limit)) = payload.get("rate_limits").and_then(|rl| parse_rate_limit(rl, at)) {
                if info.rate_limits.get(&id).map(|old| old.at <= at).unwrap_or(true) {
                    info.rate_limits.insert(id, limit);
                }
            }
            if let Some(used) = payload.pointer("/info/last_token_usage/total_tokens").and_then(Value::as_u64).filter(|u| *u > 0) {
                info.context = Some((used, payload.pointer("/info/model_context_window").and_then(Value::as_u64).filter(|w| *w > 0)));
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
                note_prompt(info, &m, ts);
            }
            info.pending_approval = None;
        }
    }
}

/// A prompt, as the latest, and as the first where none came before it.
fn note_prompt(info: &mut ThreadInfo, prompt: &str, ts: Option<i64>) {
    let asked = asked(prompt);
    if !info.first_prompt_unread {
        info.first_prompt.get_or_insert_with(|| asked.clone());
    }
    info.last_user_message = Some(asked);
    info.last_prompt_at = ts.or(info.last_prompt_at);
}

/// What a prompt asks, on a line, for a title or an excerpt. The conversation reads prompts whole
/// (`prose_of`), since it tells Codealong's and other sessions' messages apart by their lines.
fn asked(prompt: &str) -> String {
    one_line(crate::send::past_sender_lines(prompt), 140)
}

/// Tracks a `response_item` tool call until its output arrives.
fn note_call(info: &mut ThreadInfo, p: &Value, at: i64) {
    let Some(call_id) = p.get("call_id").and_then(Value::as_str) else { return };
    match p.get("type").and_then(Value::as_str) {
        Some("function_call" | "custom_tool_call") => {
            let name = p.get("name").and_then(Value::as_str).unwrap_or_default();
            let Some(limit_ms) = call_limit_ms(name) else { return };
            info.pending_calls.retain(|c| c.call_id != call_id);
            let input = p.get("input").or_else(|| p.get("arguments")).and_then(Value::as_str).map(|i| i.chars().take(4000).collect()).unwrap_or_default();
            info.pending_calls.push(PendingCall { call_id: call_id.to_string(), at, what: call_what(p, name), limit_ms, input });
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

/// The PR a rollout line says the thread made, by its URL, with the line's time. Codex records a
/// `CommandExecution` with the command and its output together, so a `gh pr create` there is
/// followed by the URL it printed; and the Codex app's `attach_artifact` tool shows a PR on the
/// thread. Only lines with one of those marks are parsed.
pub(super) fn pr_made(line: &str) -> Option<(String, i64)> {
    let create = line.contains("\"CommandExecution\"") && line.contains("gh pr create");
    let attach = line.contains("\"attach_artifact\"") && line.contains("pull_request");
    if !(create || attach) {
        return None;
    }
    let v: Value = serde_json::from_str(line).ok()?;
    let at = v.get("timestamp").and_then(Value::as_str).and_then(parse_rfc3339_ms).unwrap_or(0);
    let item = v.pointer("/payload/item")?;
    let url = match item.get("type").and_then(Value::as_str)? {
        "CommandExecution" => {
            let command = item.get("command").map(|c| c.to_string()).unwrap_or_default();
            if !command.contains("gh pr create") {
                return None;
            }
            // The last PR link it printed: `gh pr create` prints the new PR's last.
            let out = item.get("stdout").and_then(Value::as_str).unwrap_or("");
            out.lines().rev().find(|l| crate::prlinks::from_url(l).is_some()).map(str::to_string)
        }
        "McpToolCall" => {
            let args = item.get("arguments")?;
            (item.get("tool").and_then(Value::as_str) == Some("attach_artifact") && args.get("artifact_type").and_then(Value::as_str) == Some("pull_request"))
                .then(|| args.get("url").and_then(Value::as_str).map(str::to_string))
                .flatten()
        }
        _ => None,
    }?;
    Some((url, at))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn apply_events(info: &mut ThreadInfo, text: &str) {
        for v in parse_events(text) {
            apply_event(info, &v);
        }
    }

    #[test]
    fn provider_flags_ignore_discussion_and_survive_tail_reads() {
        use serde_json::json;
        for code in ["cyber_policy", "bio_policy", "misalignment_policy_violation", "content_filter"] {
            assert!(flagged(&json!({"type":"event_msg","payload":{"type":"error","codex_error_info":code}})));
            assert!(flagged(&json!({"type":"event_msg","payload":{"type":"task_complete","error":{"codex_error_info":code}}})));
        }
        let blocked = json!({"type":"event_msg","payload":{"type":"error","message":"stream disconnected before completion: Incomplete response returned, reason: content_filter"}});
        assert!(flagged(&blocked));
        assert!(flagged(&json!({"type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"refusal","refusal":"Request refused"}]}})));
        for other in [
            json!({"type":"event_msg","payload":{"type":"error","message":"Invalid configuration key content_filter_guidance"}}),
            json!({"type":"event_msg","payload":{"type":"agent_message","message":"content_filter"}}),
            json!({"type":"event_msg","payload":{"type":"user_message","message":"cyber_policy"}}),
            json!({"type":"response_item","payload":{"type":"function_call_output","output":"content_filter"}}),
            json!({"type":"event_msg","payload":{"type":"error","codex_error_info":"sandbox_error","message":"Approval denied"}}),
            json!({"type":"event_msg","payload":{"type":"task_complete","error":{"codex_error_info":"sandbox_error"}}}),
            json!({"type":"event_msg","payload":{"type":"turn_moderation_metadata","metadata":{"presentation":"inline"}}}),
        ] { assert!(!flagged(&other), "{other}"); }
        let dir = crate::fixtures::scratch("provider-flags-codex");
        let path = dir.join("rollout.jsonl");
        let padding = json!({"type":"response_item","payload":{"type":"function_call_output","output":"x".repeat(300_000)}});
        let later = json!({"type":"event_msg","payload":{"type":"task_complete"}});
        std::fs::write(&path, format!("{META_USER}\n{blocked}\n{padding}\n{later}\n")).unwrap();
        let parsed = parse_rollout(&path, 1, None).unwrap();
        assert!(parsed.0.flagged);
        use std::io::Write;
        writeln!(std::fs::OpenOptions::new().append(true).open(&path).unwrap(), "{later}").unwrap();
        assert!(parse_rollout(&path, 2, Some(parsed)).unwrap().0.flagged);
        assert!(parse_rollout(&path, 2, None).unwrap().0.flagged);
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// Only a whole first line says a rollout never shows. A parse that looked before Codex wrote
    /// the line found nothing, and a thread's line written since must not take that for its say.
    #[test]
    fn only_a_whole_first_line_says_a_rollout_never_shows() {
        let dir = crate::fixtures::scratch("codex-never-shown");
        let path = dir.join("rollout-2026-10-22T13-30-18-01a1654e-b202-7d20-8f43-1e676c07126b.jsonl");
        let thread = r#"{"type":"session_meta","payload":{"id":"t","cwd":"/codealong-test-missing-repo","originator":"codealong"}}"#;
        let says = |text: &str| {
            std::fs::write(&path, text).unwrap();
            never_shown(&path)
        };
        assert!(!says(""), "nothing written yet");
        assert!(!says(&thread[..40]), "half a first line");
        assert!(!says(&format!("{thread}\n")), "a thread's, written since a parse found nothing");
        assert!(says("{\"type\":\"session_meta\",\"payload\":{\"id\":\"g\",\"cwd\":\"/x\",\"source\":{\"subagent\":{\"other\":\"guardian\"}}}}\n"), "a guardian review's");
        assert!(says("{\"type\":\"session_meta\",\"payload\":{\"id\":\"s\",\"cwd\":\"/x\",\"thread_source\":\"subagent\"}}\n"), "a subagent's");
        assert!(says("not a rollout\n"), "a whole line that's no session meta");
    }

    #[test]
    fn completed_turn_flags_survive_large_append_gaps_and_restarts() {
        use serde_json::json;
        use std::io::Write;
        let dir = crate::fixtures::scratch("provider-flags-codex-gaps");
        let path = dir.join("rollout.jsonl");
        let later = json!({"type":"event_msg","payload":{"type":"task_complete"}});
        let blocked = json!({"type":"event_msg","payload":{"type":"task_complete","error":{"message":"Request blocked", "codex_error_info":"cyber_policy"}}});
        let padding = json!({"type":"response_item","payload":{"type":"function_call_output","output":"x".repeat(300_000)}});
        std::fs::write(&path, format!("{META_USER}\n{later}\n")).unwrap();
        let parsed = parse_rollout(&path, 1, None).unwrap();
        assert!(!parsed.0.flagged);
        let mut file = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
        writeln!(file, "{blocked}").unwrap();
        for _ in 0..5 { writeln!(file, "{padding}").unwrap(); }
        writeln!(file, "{later}").unwrap();
        let parsed = parse_rollout(&path, 2, Some(parsed)).unwrap();
        assert!(parsed.0.flagged, "the gap before the new tail is checked");
        for _ in 0..5 { writeln!(file, "{padding}").unwrap(); }
        writeln!(file, "{later}").unwrap();
        assert!(parse_rollout(&path, 3, Some(parsed)).unwrap().0.flagged, "a rebuilt tail keeps the known flag");
        assert!(parse_rollout(&path, 3, None).unwrap().0.flagged, "a restart reads the nested completed-turn error");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn reads_the_model_its_latest_turn_ran_on() {
        let mut info = ThreadInfo::default();
        let turn = |model: &str| format!(r#"{{"timestamp":"2026-10-06T10:00:00.000Z","type":"turn_context","payload":{{"model":"{model}","effort":"high"}}}}"#);
        apply_events(&mut info, &[turn("gpt-5.4"), turn("gpt-5.5")].join("\n"));
        assert_eq!(info.model.as_deref(), Some("gpt-5.5"));
        // Settings set between turns, as `thread/settings/update` writes them (0.160.1), are what
        // its next turn runs on, its permissions named by their profile.
        let settings = r#"{"timestamp":"2026-10-06T10:01:00.000Z","type":"event_msg","payload":{"type":"thread_settings_applied","thread_id":"t1","thread_settings":{"model":"gpt-6-luna","model_provider_id":"openai","approval_policy":"on-request","approvals_reviewer":"user","active_permission_profile":{"id":":read-only"},"reasoning_effort":"low","collaboration_mode":{"mode":"default","settings":{"model":"gpt-6-luna","reasoning_effort":"low","developer_instructions":null}}}}}"#;
        apply_events(&mut info, settings);
        assert_eq!((info.model.as_deref(), info.effort.as_deref()), (Some("gpt-6-luna"), Some("low")));
        assert_eq!(info.mode.as_ref().map(|m| (m.label.as_str(), m.id.as_str())), Some(("Read Only", "read-only")));
        apply_events(&mut info, &turn("gpt-6-astra"));
        assert_eq!((info.model.as_deref(), info.effort.as_deref()), (Some("gpt-6-astra"), Some("high")));
    }

    #[test]
    fn a_pick_in_permissions_is_the_mode_at_once() {
        // As 0.162.1's terminal UI wrote a pick of Approve for me, before any turn said it.
        let picked = |reviewer: &str, profile: &str| format!(r#"{{"timestamp":"2026-10-24T15:51:59.695Z","ordinal":22,"type":"event_msg","payload":{{"type":"thread_settings_applied","thread_id":"t1","thread_settings":{{"model":"gpt-6-luna","approval_policy":"on-request","approvals_reviewer":"{reviewer}","active_permission_profile":{{"id":"{profile}"}},"reasoning_effort":"low","collaboration_mode":{{"mode":"default"}}}}}}}}"#);
        let context = r#"{"timestamp":"2026-10-24T15:48:46.000Z","type":"turn_context","payload":{"model":"gpt-6-luna","approval_policy":"on-request","approvals_reviewer":"user","sandbox_policy":{"type":"workspace-write","network_access":true}}}"#;
        let mut info = ThreadInfo::default();
        apply_events(&mut info, context);
        assert_eq!(info.mode.as_ref().map(|m| m.id.as_str()), Some(ASK_FOR_APPROVAL));
        apply_events(&mut info, &picked("auto_review", ":workspace"));
        assert_eq!(info.mode.as_ref().map(|m| (m.kind, m.label.as_str(), m.id.as_str())), Some((ModeKind::Auto, "Approve for me", APPROVE_FOR_ME)));
        apply_events(&mut info, &picked("user", ":danger-full-access"));
        assert_eq!(info.mode.as_ref().map(|m| m.id.as_str()), Some("full-access"));
        // A profile of the config's own is no entry of `/permissions`.
        apply_events(&mut info, &picked("user", "locked-down"));
        assert_eq!(info.mode.as_ref().map(|m| (m.label.as_str(), m.id.as_str())), Some(("Custom permissions", "")));

        // The end of a rollout says the latest of either.
        let dir = crate::fixtures::scratch("codex-mode-at-end");
        let path = dir.join("rollout.jsonl");
        std::fs::write(&path, format!("{context}\n{}\n", picked("auto_review", ":workspace"))).unwrap();
        assert_eq!(mode_at_end(&path, 1 << 16).as_deref(), Some(APPROVE_FOR_ME));
        std::fs::write(&path, format!("{}\n{context}\n", picked("auto_review", ":workspace"))).unwrap();
        assert_eq!(mode_at_end(&path, 1 << 16).as_deref(), Some(ASK_FOR_APPROVAL));
        // A pick in Plan leaves it in Plan, and the permissions are still told.
        let planning = picked("auto_review", ":workspace").replace(r#""mode":"default""#, r#""mode":"plan""#);
        std::fs::write(&path, format!("{context}\n{planning}\n")).unwrap();
        assert_eq!(mode_at_end(&path, 1 << 16).as_deref(), Some(APPROVE_FOR_ME));
        let mut info = ThreadInfo::default();
        apply_events(&mut info, &planning);
        assert_eq!(info.mode.map(|m| m.kind), Some(ModeKind::Plan));
        // Both say the folder they ran in, and when.
        let mut info = ThreadInfo::default();
        apply_events(&mut info, &context.replace(r#""model":"gpt-6-luna","#, r#""model":"gpt-6-luna","cwd":"/Users/me/projects/atlas","#));
        assert_eq!(info.said_cwd, Some(("/Users/me/projects/atlas".to_string(), 1_792_856_926_000)));
        apply_events(&mut info, &picked("user", ":workspace").replace(r#""model":"gpt-6-luna","#, r#""cwd":"/Users/me/projects/kestrel","model":"gpt-6-luna","#));
        assert_eq!(info.said_cwd.map(|(c, _)| c).as_deref(), Some("/Users/me/projects/kestrel"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn lists_the_permissions_as_codex_does() {
        let modes = picked_modes(true);
        assert_eq!(modes.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), PICKED.iter().map(|(id, ..)| *id).collect::<Vec<_>>());
        assert_eq!(modes.iter().map(|m| m.label.as_str()).collect::<Vec<_>>(), ["Ask for approval", "Approve for me", "Full Access", "Read Only"]);
        assert_eq!(picked_modes(false).iter().map(|m| m.label.as_str()).collect::<Vec<_>>(), ["Ask for approval", "Full Access", "Read Only"]);
    }

    #[test]
    fn names_the_permissions_its_latest_turn_ran_with_as_codex_does() {
        let mode = |payload: &str| turn_mode(&serde_json::from_str(payload).unwrap()).map(|m| (m.kind, m.label));
        let says = |kind: ModeKind, label: &str| Some((kind, label.to_string()));
        assert_eq!(mode(r#"{"approval_policy":"on-request","approvals_reviewer":"auto_review","sandbox_policy":{"type":"workspace-write"},"collaboration_mode":{"mode":"default"}}"#), says(ModeKind::Auto, "Approve for me"));
        assert_eq!(mode(r#"{"approval_policy":"on-request","approvals_reviewer":"user","sandbox_policy":{"type":"workspace-write"}}"#), says(ModeKind::Edits, "Ask for approval"));
        assert_eq!(mode(r#"{"approval_policy":"never","approvals_reviewer":"user","sandbox_policy":{"type":"danger-full-access"}}"#), says(ModeKind::Skips, "Full Access"));
        assert_eq!(mode(r#"{"approval_policy":"never","approvals_reviewer":"auto_review","sandbox_policy":{"type":"read-only"}}"#), says(ModeKind::ReadOnly, "Read Only"));
        assert_eq!(mode(r#"{"approval_policy":{"granular":{}},"sandbox_policy":{"type":"external-sandbox"}}"#), says(ModeKind::Other, "Custom permissions"));
        // Plan changes nothing, whatever the sandbox.
        assert_eq!(mode(r#"{"approval_policy":"never","sandbox_policy":{"type":"danger-full-access"},"collaboration_mode":{"mode":"plan"}}"#), says(ModeKind::Plan, "Plan"));
        assert_eq!(mode(r#"{"model":"gpt-5.5"}"#), None);

        // A turn that says nothing of it leaves the one before.
        let mut info = ThreadInfo::default();
        let turn = |payload: &str| format!(r#"{{"timestamp":"2026-10-13T10:00:00.000Z","type":"turn_context","payload":{payload}}}"#);
        apply_events(&mut info, &[turn(r#"{"approval_policy":"on-request","approvals_reviewer":"user","sandbox_policy":{"type":"read-only"}}"#), turn(r#"{"model":"gpt-5.5"}"#)].join("\n"));
        assert_eq!(info.mode.map(|m| m.label).as_deref(), Some("Read Only"));
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
        // `codex exec`'s are kept, as runs nobody is at.
        assert!(!parse_meta(META_USER).unwrap().exec);
        let exec = r#"{"type":"session_meta","payload":{"id":"x","cwd":"/x","originator":"codex_exec","source":"exec","thread_source":"user"}}"#;
        assert!(parse_meta(exec).is_some_and(|th| th.exec && !th.cli));
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
    fn keeps_when_a_turn_was_interrupted_until_another_starts() {
        let mut info = ThreadInfo::default();
        apply_events(&mut info, TURN_STARTED);
        apply_events(&mut info, r#"{"timestamp":"2026-10-19T21:50:50.554Z","ordinal":1159,"type":"event_msg","payload":{"type":"turn_aborted","turn_id":"u","reason":"interrupted","started_at":1792446437,"completed_at":1792446650,"duration_ms":213451}}"#);
        assert_eq!((info.turn_open, info.interrupted_at), (false, parse_rfc3339_ms("2026-10-19T21:50:50.554Z")));
        // The app opening it again writes its settings, which changes nothing.
        apply_events(&mut info, r#"{"timestamp":"2026-10-19T22:02:16.297Z","type":"event_msg","payload":{"type":"thread_settings_applied","thread_settings":{"model":"gpt-6-astra"}}}"#);
        assert_eq!(info.interrupted_at, parse_rfc3339_ms("2026-10-19T21:50:50.554Z"));
        apply_events(&mut info, TURN_STARTED);
        assert_eq!(info.interrupted_at, None);
        // A turn ended for another reason, or by a Codex that doesn't say why.
        for reason in [r#","reason":"replaced""#, ""] {
            apply_events(&mut info, &format!(r#"{{"timestamp":"2026-10-19T22:10:00.000Z","type":"event_msg","payload":{{"type":"turn_aborted"{reason}}}}}"#));
            assert_eq!(info.interrupted_at, None, "{reason}");
        }
    }

    #[test]
    fn reads_prompts_from_items() {
        let mut info = parse_meta(META_USER).unwrap();
        apply_events(&mut info, &format!("{PROMPT_ITEM}\n"));
        assert_eq!(info.last_user_message.as_deref(), Some("Find the forwarded Spur issue and diagnose it"));
        assert_eq!(info.last_prompt_at, parse_rfc3339_ms("2026-09-24T18:04:01.042Z"));
    }

    /// Edit from here copies a thread through the turn before your prompt's, named by an item of
    /// it, so a prompt sent into a running turn goes back to that turn's start too.
    #[test]
    fn a_fork_from_your_prompt_goes_through_the_turn_before() {
        let event = |payload: Value| serde_json::json!({ "timestamp": "2026-10-20T21:13:31.036Z", "type": "event_msg", "payload": payload }).to_string();
        let started = |turn: &str| event(serde_json::json!({ "type": "task_started", "turn_id": turn }));
        let item = |turn: &str, kind: &str, id: &str| event(serde_json::json!({ "type": "item_completed", "turn_id": turn, "item": { "type": kind, "id": id } }));
        let prompt = |text: &str| event(serde_json::json!({ "type": "user_message", "message": text }));
        let lines = [
            started("t1"),
            prompt("Fix the parser"),
            item("t1", "UserMessage", "u1"),
            item("t1", "AgentMessage", "msg_a1"),
            item("t1", "Reasoning", "rs_1"),
            event(serde_json::json!({ "type": "task_complete" })),
            started("t2"),
            prompt("Now test it"),
            item("t2", "AgentMessage", "msg_a2"),
            prompt("And the docs"),
        ];
        let dir = crate::fixtures::scratch("codex-fork-from");
        let path = dir.join("rollout.jsonl");
        std::fs::write(&path, lines.join("\n") + "\n").unwrap();
        let at = |n: usize| lines[..n].iter().map(|l| l.len() as u64 + 1).sum::<u64>();
        assert_eq!(item_before_turn(&path, at(7)).unwrap().as_deref(), Some("msg_a1"));
        // Sent into the running turn: back to its start, through the turn before.
        assert_eq!(item_before_turn(&path, at(9)).unwrap().as_deref(), Some("msg_a1"));
        assert_eq!(item_before_turn(&path, at(1)).unwrap(), None);
        // An older Codex wrote no items: there was a turn before, but nothing names it.
        let old = [started("t1"), prompt("Fix the parser"), started("t2"), prompt("Now test it")];
        std::fs::write(&path, old.join("\n") + "\n").unwrap();
        let at = |n: usize| old[..n].iter().map(|l| l.len() as u64 + 1).sum::<u64>();
        assert!(item_before_turn(&path, at(3)).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn records_where_commands_ran_and_files_changed() {
        let mut info = parse_meta(META_USER).unwrap();
        let tail = r#"{"timestamp":"2026-09-24T18:04:02.000Z","type":"event_msg","payload":{"type":"item_completed","item":{"type":"CommandExecution","command":["/bin/zsh","-lc","git status"],"cwd":"file:///Users/me/projects/sparkhq/spark-infra"}}}
{"timestamp":"2026-09-24T18:04:03.000Z","type":"event_msg","payload":{"type":"item_completed","item":{"type":"CommandExecution","command":["/bin/zsh","-lc","cd ../spark-fix && yarn test"],"cwd":"file:///Users/me/projects/sparkhq/spark-infra"}}}
{"timestamp":"2026-09-24T18:04:04.000Z","type":"event_msg","payload":{"type":"item_completed","item":{"type":"CommandExecution","command":["/bin/zsh","-lc","yarn build"],"cwd":"file:///Users/me/projects/my%20app"}}}
{"timestamp":"2026-09-24T18:04:04.500Z","type":"event_msg","payload":{"type":"item_completed","item":{"type":"CommandExecution","command":["/bin/zsh","-lc","git log --oneline -5 | cat"],"cwd":"file:///Users/me/projects/sparkhq/spark-docs"}}}
{"timestamp":"2026-09-24T18:04:05.000Z","type":"event_msg","payload":{"type":"item_completed","item":{"type":"FileChange","changes":{"/Users/me/projects/sparkhq/spark-fix/src/a.ts":{"type":"update"}}}}}
{"timestamp":"2026-09-24T18:04:06.000Z","type":"event_msg","payload":{"type":"item_completed","item":{"type":"Reasoning","id":"r"}}}
"#;
        apply_events(&mut info, tail);
        // Looking counts in the thread's own folder; a look into another repo isn't work there.
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

    /// A turn that began behind the tail still waits on the calls it made there without results:
    /// what an approval it asks is about (asks.rs).
    #[test]
    fn a_turn_begun_behind_the_tail_keeps_its_calls_without_results() {
        let call = r#"{"timestamp":"2026-09-24T18:04:30.000Z","type":"response_item","payload":{"type":"custom_tool_call","status":"completed","call_id":"ca","name":"exec","input":"await tools.exec_command({cmd:\"touch ~/.northwind\"});"}}"#;
        let done = r#"{"timestamp":"2026-09-24T18:04:31.000Z","type":"response_item","payload":{"type":"function_call","call_id":"cb","name":"exec_command","arguments":"{\"cmd\":\"ls\"}"}}"#;
        let result = r#"{"timestamp":"2026-09-24T18:04:32.000Z","type":"response_item","payload":{"type":"function_call_output","call_id":"cb","output":"README.md"}}"#;
        let (dir, path) = long_rollout("calls-behind", &[TURN_STARTED, call, done, result], &[TOKEN_COUNT]);
        let (info, _) = parse_rollout(&path, 0, None).unwrap();
        assert!(info.turn_open);
        assert_eq!(info.pending_calls.iter().map(|c| c.call_id.as_str()).collect::<Vec<_>>(), ["ca"]);
        assert!(info.pending_calls[0].input.contains("touch ~/.northwind"));
        std::fs::remove_dir_all(&dir).unwrap();
        // A turn that ended behind the tail leaves none.
        let ended = r#"{"timestamp":"2026-09-24T18:04:40.000Z","type":"event_msg","payload":{"type":"task_complete","turn_id":"u"}}"#;
        let (dir, path) = long_rollout("calls-ended", &[TURN_STARTED, call, ended], &[TOKEN_COUNT]);
        let (info, _) = parse_rollout(&path, 0, None).unwrap();
        assert!(info.pending_calls.is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn reads_how_full_the_context_is_from_the_last_token_count() {
        let count = r#"{"timestamp":"2026-09-24T18:32:00.000Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"total_tokens":1415510},"last_token_usage":{"input_tokens":124888,"cached_input_tokens":123392,"output_tokens":212,"total_tokens":125100},"model_context_window":258400}}}"#;
        let (dir, path) = long_rollout("context", &[TURN_STARTED, PROMPT_ITEM], &[TOKEN_COUNT, count]);
        let (info, _) = parse_rollout(&path, 0, None).unwrap();
        assert_eq!(info.context, Some((125100, Some(258400))));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn finds_the_model_and_permissions_of_a_turn_that_started_long_before_the_tail() {
        let context = r#"{"timestamp":"2026-09-24T18:04:00.000Z","type":"turn_context","payload":{"model":"gpt-5.5","approval_policy":"on-request","approvals_reviewer":"user","sandbox_policy":{"type":"workspace-write"},"effort":"xhigh"}}"#;
        let (dir, path) = long_rollout("turn-context", &[TURN_STARTED, context, PROMPT_ITEM], &[TOKEN_COUNT]);
        let (info, _) = parse_rollout(&path, 0, None).unwrap();
        assert_eq!(info.model.as_deref(), Some("gpt-5.5"));
        assert_eq!(info.effort.as_deref(), Some("xhigh"));
        assert_eq!(info.mode.map(|m| m.label).as_deref(), Some("Ask for approval"));
        assert!(info.turn_open);
        std::fs::remove_dir_all(&dir).unwrap();
        // A model and level set during that turn, as the app writes them, are what its next
        // turn runs on; the permissions stay the turn's.
        let settings = r#"{"timestamp":"2026-09-24T18:04:30.000Z","type":"event_msg","payload":{"type":"thread_settings_applied","thread_settings":{"model":"gpt-6-luna","approval_policy":"on-request","reasoning_effort":"low"}}}"#;
        let (dir, path) = long_rollout("turn-settings", &[TURN_STARTED, context, PROMPT_ITEM, settings], &[TOKEN_COUNT]);
        let (info, _) = parse_rollout(&path, 0, None).unwrap();
        assert_eq!((info.model.as_deref(), info.effort.as_deref()), (Some("gpt-6-luna"), Some("low")));
        assert_eq!(info.mode.map(|m| m.label).as_deref(), Some("Ask for approval"));
        std::fs::remove_dir_all(&dir).unwrap();
        // Settings from before the turn are older than its context.
        let (dir, path) = long_rollout("old-settings", &[settings, TURN_STARTED, context, PROMPT_ITEM], &[TOKEN_COUNT]);
        let (info, _) = parse_rollout(&path, 0, None).unwrap();
        assert_eq!((info.model.as_deref(), info.effort.as_deref()), (Some("gpt-5.5"), Some("xhigh")));
        std::fs::remove_dir_all(&dir).unwrap();
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
    fn a_first_parse_finds_where_it_worked_before_the_tail() {
        let ran = |at: &str| format!(r#"{{"timestamp":"2026-09-24T18:04:{at}.000Z","type":"event_msg","payload":{{"type":"item_completed","item":{{"type":"CommandExecution","command":["/bin/zsh","-lc","cd ../spark-fix && yarn test"],"cwd":"file:///Users/me/projects/sparkhq/spark-infra"}}}}}}"#);
        let (a, b) = (ran("02"), ran("03"));
        let (dir, path) = long_rollout("work-before", &[TURN_STARTED, PROMPT_ITEM, &a, &b], &[TOKEN_COUNT]);
        let (info, _) = parse_rollout(&path, 0, None).unwrap();
        assert_eq!(info.work, ["/Users/me/projects/sparkhq/spark-fix", "/Users/me/projects/sparkhq/spark-fix"]);
        assert!(info.turn_open);

        // Not past `WORK_LOOKBACK`, one output longer than that included, while the turn and its
        // prompt are looked for as far back as they are.
        let output = format!(r#"{{"timestamp":"2026-09-24T18:05:00.000Z","type":"response_item","payload":{{"type":"custom_tool_call_output","output":"{}"}}}}"#, "x".repeat(5_000_000));
        let lines = [META_USER, TURN_STARTED, PROMPT_ITEM, &a, &output, TOKEN_COUNT];
        std::fs::write(&path, lines.join("\n") + "\n").unwrap();
        let (info, _) = parse_rollout(&path, 0, None).unwrap();
        assert!(info.work.is_empty());
        assert!(info.turn_open);
        assert_eq!(info.last_user_message.as_deref(), Some("Find the forwarded Spur issue and diagnose it"));
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

    /// A thread's first prompt is read from its start however far behind the tail, and what
    /// Codealong sent it later is its latest prompt without the line that says Codealong sent it:
    /// such a thread was titled "Codealong sent this by itself…" once a PR's news reached it.
    #[test]
    fn a_first_parse_finds_the_first_prompt_and_what_later_ones_ask() {
        let prompt = |at: &str, text: &str| json!({ "timestamp": format!("2026-09-24T18:{at}.000Z"), "type": "event_msg", "payload": { "type": "user_message", "message": text } }).to_string();
        let again = prompt("10:00", "Now the docs too");
        let news = prompt("40:00", &crate::send::by_codealong("On your PR #7: ken approved it."));
        let (dir, path) = long_rollout("first-prompt", &[TURN_STARTED, PROMPT_ITEM, TURN_COMPLETE, &again], &[&news, TOKEN_COUNT]);
        let (info, off) = parse_rollout(&path, 0, None).unwrap();
        assert_eq!(info.first_prompt.as_deref(), Some("Find the forwarded Spur issue and diagnose it"));
        assert_eq!(info.last_user_message.as_deref(), Some("On your PR #7: ken approved it."));
        // Its latest before the tail, when the tail has none.
        let (dir2, path2) = long_rollout("first-prompt-back", &[TURN_STARTED, PROMPT_ITEM, TURN_COMPLETE, &again], &[TOKEN_COUNT]);
        let (back, _) = parse_rollout(&path2, 0, None).unwrap();
        assert_eq!((back.first_prompt.as_deref(), back.last_user_message.as_deref()), (Some("Find the forwarded Spur issue and diagnose it"), Some("Now the docs too")));

        // A later read keeps it.
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
        writeln!(f, "{}", prompt("50:00", "And the changelog")).unwrap();
        drop(f);
        let (info, _) = parse_rollout(&path, 0, Some((info, off))).unwrap();
        assert_eq!((info.first_prompt.as_deref(), info.last_user_message.as_deref()), (Some("Find the forwarded Spur issue and diagnose it"), Some("And the changelog")));
        std::fs::remove_dir_all(&dir).unwrap();
        std::fs::remove_dir_all(&dir2).unwrap();
    }

    /// A first prompt behind the images it came with, written out whole before it, is found past
    /// them; one further than a first parse looks stays unknown, and no later prompt is taken for
    /// it.
    #[test]
    fn a_first_prompt_behind_its_images_is_found_or_left_unknown() {
        let image = json!({ "timestamp": "2026-09-24T18:04:00.000Z", "type": "response_item", "payload": { "type": "message", "role": "user", "content": [{ "type": "input_image", "image_url": format!("data:image/png;base64,{}", "A".repeat(5_500_000)) }] } }).to_string();
        let news = json!({ "timestamp": "2026-09-24T18:40:00.000Z", "type": "event_msg", "payload": { "type": "user_message", "message": crate::send::by_codealong("On your PR #7: ken approved it.") } }).to_string();
        let again = json!({ "timestamp": "2026-09-24T18:10:00.000Z", "type": "event_msg", "payload": { "type": "user_message", "message": "Now the docs too" } }).to_string();
        let (dir, path) = long_rollout("first-prompt-images", &[TURN_STARTED, &image, PROMPT_ITEM, TURN_COMPLETE, &again], &[&news, TOKEN_COUNT]);
        let (info, _) = parse_rollout(&path, 0, None).unwrap();
        assert_eq!((info.first_prompt.as_deref(), info.first_prompt_unread), (Some("Find the forwarded Spur issue and diagnose it"), false));

        let floor = META_USER.len() as u64 + 1;
        let len = std::fs::metadata(&path).unwrap().len();
        let mut far = parse_meta(META_USER).unwrap();
        read_first_prompt(&mut far, &path, floor, len, 1_000_000);
        assert_eq!((far.first_prompt.as_deref(), far.first_prompt_unread), (None, true));
        apply_events(&mut far, &format!("{again}\n{news}\n"));
        assert_eq!((far.first_prompt, far.last_user_message.as_deref()), (None, Some("On your PR #7: ken approved it.")));
        // Read all the way with none there, the first read after is it.
        let mut none = parse_meta(META_USER).unwrap();
        read_first_prompt(&mut none, &path, floor, floor + TURN_STARTED.len() as u64 + 1, FIRST_PROMPT_WITHIN);
        apply_events(&mut none, &format!("{again}\n"));
        assert_eq!((none.first_prompt.as_deref(), none.first_prompt_unread), (Some("Now the docs too"), false));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A thread read from its start takes its first prompt as it comes, Codealong's too.
    #[test]
    fn the_first_prompt_is_the_first_one_read() {
        let mut info = parse_meta(META_USER).unwrap();
        let told = json!({ "timestamp": "2026-09-24T18:04:00.000Z", "type": "event_msg", "payload": { "type": "user_message", "message": crate::send::by_codealong("Review pull request #7 in o/r") } });
        apply_events(&mut info, &format!("{told}\n{PROMPT_ITEM}\n"));
        assert_eq!(info.first_prompt.as_deref(), Some("Review pull request #7 in o/r"));
        assert_eq!(info.last_user_message.as_deref(), Some("Find the forwarded Spur issue and diagnose it"));
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
    fn reads_what_each_call_used_once_with_its_turns_model_and_tier() {
        // As 0.157.1 writes them.
        let lines = [
            r#"{"timestamp":"2026-10-12T17:45:28.000Z","type":"session_meta","payload":{"id":"t","cwd":"/Users/me/app"}}"#,
            r#"{"timestamp":"2026-10-12T17:45:28.567Z","type":"turn_context","payload":{"turn_id":"u","cwd":"/Users/me/app","model":"gpt-6-astra"}}"#,
            r#"{"timestamp":"2026-10-12T17:45:29.000Z","type":"event_msg","payload":{"type":"thread_settings_applied","thread_settings":{"service_tier":"priority"}}}"#,
            r#"{"timestamp":"2026-10-12T18:03:57.247Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":124888,"cached_input_tokens":123392,"cache_write_input_tokens":0,"output_tokens":212,"reasoning_output_tokens":0,"total_tokens":125100},"last_token_usage":{"input_tokens":124888,"cached_input_tokens":123392,"cache_write_input_tokens":0,"output_tokens":212,"reasoning_output_tokens":10,"total_tokens":125100},"model_context_window":258400}}}"#,
            // The same count again, as Codex sometimes sends it.
            r#"{"timestamp":"2026-10-12T18:03:58.000Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"total_tokens":125100},"last_token_usage":{"input_tokens":124888,"cached_input_tokens":123392,"output_tokens":212,"total_tokens":125100}}}}"#,
            r#"{"timestamp":"2026-10-12T18:04:00.000Z","type":"event_msg","payload":{"type":"token_count","info":null,"rate_limits":{}}}"#,
        ];
        let mut st = crate::ledger::TokenState { session: "codex:codex:t".into(), ..Default::default() };
        let uses: Vec<_> = lines.iter().filter_map(|l| token_use(l.as_bytes(), &mut st)).collect();
        assert_eq!(uses.len(), 1);
        let u = &uses[0];
        assert_eq!((u.input, u.cache_read, u.output, u.reasoning), (124888 - 123392, 123392, 212, 10));
        assert_eq!((u.model.as_str(), u.fast, u.folder.as_str(), u.id.as_str()), ("gpt-6-astra", true, "/Users/me/app", "2026-10-12T18:03:57.247Z:125100"));
        assert_eq!(u.session, "codex:codex:t");
    }

    #[test]
    fn a_spawned_agents_calls_are_its_threads() {
        let meta = r#"{"timestamp":"2026-10-12T16:23:32.736Z","type":"session_meta","payload":{"id":"child","cwd":"/w","source":{"subagent":{"thread_spawn":{"parent_thread_id":"parent","depth":1}}}}}"#;
        let count = r#"{"timestamp":"2026-10-12T16:23:39.303Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"total_tokens":26270},"last_token_usage":{"input_tokens":26000,"cached_input_tokens":0,"output_tokens":270,"total_tokens":26270}}}}"#;
        let mut st = crate::ledger::TokenState { session: "codex:codex:child".into(), ..Default::default() };
        assert_eq!(token_use(meta.as_bytes(), &mut st), None);
        // Read on later from where it stopped, the file's session is set afresh; the parent stays.
        st.session = "codex:codex:child".into();
        assert_eq!(token_use(count.as_bytes(), &mut st).unwrap().session, "codex:codex:parent");
        // A guardian review names no parent and is its own.
        let mut guard = crate::ledger::TokenState { session: "codex:codex:g".into(), ..Default::default() };
        token_use(r#"{"type":"session_meta","payload":{"source":{"subagent":{"other":"guardian"}}}}"#.as_bytes(), &mut guard);
        assert_eq!(token_use(count.as_bytes(), &mut guard).unwrap().session, "codex:codex:g");
    }
}
