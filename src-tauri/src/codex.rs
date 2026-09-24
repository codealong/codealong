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

/// The thread id at the end of a rollout file name.
pub fn thread_id_of(path: &Path) -> Option<String> {
    let name = file_name(path);
    let stem = name.strip_suffix(".jsonl")?;
    (stem.len() >= 36).then(|| stem[stem.len() - 36..].to_string())
}

/// Parses a rollout. Threads you didn't start (guardian reviews, subagents) return `None`.
/// With an earlier result and the offset it stopped at, only new lines are read.
pub fn parse_rollout(path: &Path, mtime: i64, prev: Option<(ThreadInfo, u64)>) -> Option<(ThreadInfo, u64)> {
    let len = std::fs::metadata(path).ok()?.len();
    let (mut info, start, mut text) = match prev.filter(|(_, off)| *off <= len && len - off <= 1024 * 1024) {
        Some((info, off)) => (info, off, read_range(path, off, len)?),
        None => {
            let first = read_first_line(path, 4 * 1024 * 1024)?;
            let mut info = parse_meta(&first)?;
            if info.id.is_empty() {
                info.id = thread_id_of(path)?;
            }
            let start = len.saturating_sub(256 * 1024).max(first.len() as u64);
            let text = read_range(path, start, len)?;
            (info, start, text)
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
    apply_events(&mut info, &text[..complete]);
    if info.last_ts == 0 {
        info.last_ts = mtime;
    }
    Some((info, start + complete as u64))
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

pub fn apply_events(info: &mut ThreadInfo, tail: &str) {
    for line in tail.lines() {
        let Ok(v) = serde_json::from_str::<Value>(line) else { continue };
        let ts = v.get("timestamp").and_then(Value::as_str).and_then(parse_rfc3339_ms);
        if let Some(t) = ts {
            info.last_ts = info.last_ts.max(t);
        }
        if v.get("type").and_then(Value::as_str) != Some("event_msg") {
            continue;
        }
        let Some(payload) = v.get("payload") else { continue };
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
                if let Some(m) = payload.get("message").and_then(Value::as_str).filter(|m| !m.trim().is_empty()) {
                    info.last_user_message = Some(one_line(m, 140));
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
                info.pending_approval = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn reads_thread_ids_from_file_names() {
        let p = Path::new("/x/rollout-2026-09-21T08-47-36-01a0c4a6-bc7b-7212-b766-77fcc7a15d43.jsonl");
        assert_eq!(thread_id_of(p).as_deref(), Some("01a0c4a6-bc7b-7212-b766-77fcc7a15d43"));
    }
}
