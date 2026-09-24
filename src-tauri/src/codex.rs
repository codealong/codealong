//! Codex: the thread index (`~/.codex/session_index.jsonl`) and rollouts
//! (`~/.codex/sessions/YYYY/MM/DD/rollout-<time>-<id>.jsonl`). See *Harness adapters*.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::util::{file_name, mtime_ms, one_line, parse_rfc3339_ms, read_first_line};

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
    pub last_user_message: Option<String>,
    /// An approval request with nothing after it.
    pub pending_approval: Option<String>,
    /// The latest rate limits each turn reported, by limit id (`codex`, or a per-model one).
    pub rate_limits: BTreeMap<String, RateLimit>,
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

/// Hands the whole lines in `floor..end` (both at line starts) to `visit`, last first, reading
/// a megabyte at a time, until `visit` returns true.
fn scan_lines_back(path: &Path, floor: u64, end: u64, mut visit: impl FnMut(&str) -> bool) {
    use std::io::{Read, Seek, SeekFrom};
    const CHUNK: u64 = 1024 * 1024;
    let Ok(mut f) = std::fs::File::open(path) else { return };
    let mut hi = end;
    // The start of a line the last read cut off, which the next (earlier) read finishes.
    let mut carry: Vec<u8> = Vec::new();
    while hi > floor {
        let lo = hi.saturating_sub(CHUNK).max(floor);
        let mut buf = Vec::with_capacity((hi - lo) as usize + carry.len());
        if f.seek(SeekFrom::Start(lo)).is_err() || (&mut f).take(hi - lo).read_to_end(&mut buf).is_err() {
            return;
        }
        buf.extend_from_slice(&carry);
        // What comes before the first newline belongs to a line that starts in an earlier chunk.
        let whole_from = if lo == floor {
            0
        } else {
            match buf.iter().position(|&b| b == b'\n') {
                Some(i) => i + 1,
                None => {
                    carry = buf;
                    hi = lo;
                    continue;
                }
            }
        };
        for line in buf[whole_from..].split(|&b| b == b'\n').rev() {
            if let Ok(line) = std::str::from_utf8(line) {
                if !line.is_empty() && visit(line) {
                    return;
                }
            }
        }
        carry = buf[..whole_from].to_vec();
        hi = lo;
    }
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
        started_at: p
            .get("timestamp")
            .and_then(Value::as_str)
            .or_else(|| v.get("timestamp").and_then(Value::as_str))
            .and_then(parse_rfc3339_ms),
        ..Default::default()
    })
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

fn apply_event(info: &mut ThreadInfo, v: &Value) {
    let ts = v.get("timestamp").and_then(Value::as_str).and_then(parse_rfc3339_ms);
    if let Some(t) = ts {
        info.last_ts = info.last_ts.max(t);
    }
    let Some(payload) = event_payload(v) else { return };
    let kind = payload.get("type").and_then(Value::as_str).unwrap_or_default();
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
            }
            info.pending_approval = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply_events(info: &mut ThreadInfo, text: &str) {
        for v in parse_events(text) {
            apply_event(info, &v);
        }
    }

    const META_USER: &str = r#"{"timestamp":"2026-09-21T15:47:36.000Z","type":"session_meta","payload":{"id":"01a0c4a6-bc7b-7212-b766-77fcc7a15d43","cwd":"/Users/me/projects/atlasai/atlas-infra","originator":"Codex Desktop","source":"vscode","thread_source":"user","git":{"branch":"main"}}}"#;

    #[test]
    fn keeps_only_threads_you_started() {
        assert!(parse_meta(META_USER).is_some());
        let guardian = r#"{"type":"session_meta","payload":{"id":"x","cwd":"/x","source":{"subagent":{"other":"guardian"}},"thread_source":"guardian_review"}}"#;
        assert!(parse_meta(guardian).is_none());
        let spawned = r#"{"type":"session_meta","payload":{"id":"x","cwd":"/x","source":{"subagent":{"thread_spawn":{}}},"thread_source":"subagent"}}"#;
        assert!(parse_meta(spawned).is_none());
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
        assert_eq!(info.branch.as_deref(), Some("main"));
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
    const PROMPT_ITEM: &str = r#"{"timestamp":"2026-09-24T18:04:01.042Z","ordinal":9,"type":"event_msg","payload":{"type":"item_completed","thread_id":"t","turn_id":"u","item":{"type":"UserMessage","id":"i","content":[{"type":"text","text":"Find the forwarded Kestrel issue and diagnose it\n","text_elements":[]}]}}}"#;
    const TURN_COMPLETE: &str = r#"{"timestamp":"2026-09-24T18:30:00.000Z","type":"event_msg","payload":{"type":"task_complete","turn_id":"u","last_agent_message":"Found it: the webhook retries twice."}}"#;
    const TOKEN_COUNT: &str = r#"{"timestamp":"2026-09-24T18:31:00.000Z","type":"event_msg","payload":{"type":"token_count","info":null}}"#;

    #[test]
    fn reads_prompts_from_items() {
        let mut info = parse_meta(META_USER).unwrap();
        apply_events(&mut info, &format!("{PROMPT_ITEM}\n"));
        assert_eq!(info.last_user_message.as_deref(), Some("Find the forwarded Kestrel issue and diagnose it"));
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

    #[test]
    fn finds_a_turn_that_started_long_before_the_tail() {
        let (dir, path) = long_rollout("turn-open", &[TURN_STARTED, PROMPT_ITEM], &[TOKEN_COUNT]);
        let len = std::fs::metadata(&path).unwrap().len();
        let (info, off) = parse_rollout(&path, 0, None).unwrap();
        assert!(info.turn_open);
        assert_eq!(info.turn_started_at, parse_rfc3339_ms("2026-09-24T18:03:59.377Z"));
        assert_eq!(info.last_user_message.as_deref(), Some("Find the forwarded Kestrel issue and diagnose it"));
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
        assert_eq!(info.last_user_message.as_deref(), Some("Find the forwarded Kestrel issue and diagnose it"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_turn_event_in_the_tail_wins_over_earlier_ones() {
        let (dir, path) = long_rollout("turn-tail", &[TURN_STARTED, PROMPT_ITEM], &[TURN_COMPLETE, TOKEN_COUNT]);
        let (info, _) = parse_rollout(&path, 0, None).unwrap();
        assert!(!info.turn_open);
        assert_eq!(info.last_agent_message.as_deref(), Some("Found it: the webhook retries twice."));
        assert_eq!(info.last_user_message.as_deref(), Some("Find the forwarded Kestrel issue and diagnose it"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn reads_thread_ids_from_file_names() {
        let p = Path::new("/x/rollout-2026-09-21T08-47-36-01a0c4a6-bc7b-7212-b766-77fcc7a15d43.jsonl");
        assert_eq!(thread_id_of(p).as_deref(), Some("01a0c4a6-bc7b-7212-b766-77fcc7a15d43"));
    }
}
