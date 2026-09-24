//! Claude Code: the live session registry (`<home>/sessions/<pid>.json`) and transcripts
//! (`<home>/projects/<encoded-cwd>/<session>.jsonl`). See *Harness adapters*.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::Value;

use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};

use crate::procs;
use crate::util::{code, file_name, mtime_ms, one_line, parse_rfc3339_ms, read_head};

#[derive(Debug, Clone, PartialEq)]
pub struct RegistryEntry {
    pub pid: u32,
    pub session_id: String,
    pub cwd: String,
    /// `busy`, `idle`, `shell`, `waiting`, `blocked`…
    pub status: String,
    pub kind: Option<String>,
    /// Set on a background job (`kind: "bg"`): what `claude attach` takes.
    pub job_id: Option<String>,
    pub entrypoint: Option<String>,
    pub name: Option<String>,
    pub name_source: Option<String>,
    pub started_at: Option<i64>,
    pub status_updated_at: Option<i64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawRegistry {
    pid: u32,
    session_id: Option<String>,
    cwd: Option<String>,
    status: Option<String>,
    kind: Option<String>,
    job_id: Option<String>,
    entrypoint: Option<String>,
    name: Option<String>,
    name_source: Option<String>,
    started_at: Option<i64>,
    status_updated_at: Option<i64>,
}

pub fn parse_registry(text: &str) -> Option<RegistryEntry> {
    let raw: RawRegistry = serde_json::from_str(text).ok()?;
    Some(RegistryEntry {
        pid: raw.pid,
        session_id: raw.session_id?,
        cwd: raw.cwd?,
        status: raw.status.unwrap_or_else(|| "idle".into()),
        kind: raw.kind,
        job_id: raw.job_id,
        entrypoint: raw.entrypoint,
        name: raw.name,
        name_source: raw.name_source,
        started_at: raw.started_at,
        status_updated_at: raw.status_updated_at,
    })
}

/// Live sessions for one account. Entries whose process is gone, or whose pid now belongs
/// to a newer process, are dropped.
pub fn read_registry(home: &Path) -> Vec<RegistryEntry> {
    let Ok(entries) = std::fs::read_dir(home.join("sessions")) else { return vec![] };
    let mut out = Vec::new();
    for e in entries.flatten() {
        let path = e.path();
        if path.extension().and_then(|x| x.to_str()) != Some("json") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else { continue };
        let Some(entry) = parse_registry(&text) else { continue };
        if !procs::pid_alive(entry.pid) {
            continue;
        }
        if let (Some(proc_start), Some(started)) = (procs::proc_start_ms(entry.pid), entry.started_at) {
            if proc_start > started + 60_000 {
                continue;
            }
        }
        out.push(entry);
    }
    out
}

/// What the start of a transcript says. It never changes, so the engine reads it once.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HeadInfo {
    pub session_id: String,
    pub cwd: String,
    pub git_branch: Option<String>,
    pub entrypoint: Option<String>,
    pub started_at: Option<i64>,
    pub first_prompt: Option<String>,
}

/// Where reading stopped, so the next change reads only the new lines.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TailState {
    pub offset: u64,
    /// Tool calls still waiting on a result: (id, request).
    pub open_tools: Vec<(String, String)>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TranscriptInfo {
    pub session_id: String,
    pub cwd: String,
    pub git_branch: Option<String>,
    pub entrypoint: Option<String>,
    /// From `custom-title` or `agent-name` lines.
    pub title: Option<String>,
    /// The title Claude Code generates and shows in the terminal tab, from `ai-title` lines.
    /// They repeat every few dozen lines, so the tail always has the current one.
    pub ai_title: Option<String>,
    pub first_prompt: Option<String>,
    pub last_prompt: Option<String>,
    pub started_at: Option<i64>,
    pub last_ts: i64,
    /// The tool call waiting on a result, e.g. "Bash · `yarn test`".
    pub pending_tool: Option<String>,
    /// The latest tool call as an activity, e.g. "editing `App.tsx`".
    pub last_tool: Option<String>,
    /// First line of the latest assistant text.
    pub last_text: Option<String>,
}

/// Transcript files modified since `since_ms`, one level below `<home>/projects`.
/// Transcripts modified since `since_ms`, plus any whose session is in `keep` (pinned ones),
/// however old.
pub fn transcript_files(home: &Path, since_ms: i64, keep: &HashSet<String>) -> Vec<(PathBuf, i64)> {
    let mut out = Vec::new();
    let Ok(dirs) = std::fs::read_dir(home.join("projects")) else { return out };
    for d in dirs.flatten() {
        if !d.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        let Ok(files) = std::fs::read_dir(d.path()) else { continue };
        for f in files.flatten() {
            let path = f.path();
            if path.extension().and_then(|x| x.to_str()) != Some("jsonl") {
                continue;
            }
            if let Some(m) = mtime_ms(&path) {
                if m >= since_ms || (!keep.is_empty() && keep.contains(file_name(&path).trim_end_matches(".jsonl"))) {
                    out.push((path, m));
                }
            }
        }
    }
    out
}

/// Just the working directory, for first-run discovery.
pub fn transcript_cwd(path: &Path) -> Option<String> {
    let head = read_head(path, 64 * 1024)?;
    head.lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .find_map(|v| v.get("cwd").and_then(Value::as_str).map(String::from))
}

/// Reads lines from the start until the identity and the first prompt are known. Early
/// lines can be megabytes of attachments, so only lines that can matter are parsed.
pub fn read_head_info(path: &Path) -> Option<HeadInfo> {
    let mut reader = BufReader::new(File::open(path).ok()?);
    let mut head = HeadInfo::default();
    let mut line = String::new();
    let mut read = 0usize;
    for _ in 0..400 {
        line.clear();
        let n = reader.read_line(&mut line).ok()?;
        if n == 0 {
            break;
        }
        read += n;
        head_line(&mut head, &line);
        if (!head.cwd.is_empty() && head.first_prompt.is_some()) || read > 32 * 1024 * 1024 {
            break;
        }
    }
    (!head.cwd.is_empty()).then_some(head)
}

fn head_line(head: &mut HeadInfo, line: &str) {
    let wants_identity = head.cwd.is_empty() && line.contains("\"cwd\"");
    let wants_prompt = head.first_prompt.is_none() && line.contains("\"type\":\"user\"");
    if !wants_identity && !wants_prompt {
        return;
    }
    let Ok(v) = serde_json::from_str::<Value>(line) else { return };
    if head.cwd.is_empty() {
        if let Some(cwd) = v.get("cwd").and_then(Value::as_str) {
            head.cwd = cwd.to_string();
            head.session_id = str_of(&v, "sessionId").unwrap_or_default();
            head.git_branch = str_of(&v, "gitBranch").filter(|b| !b.is_empty() && b != "HEAD");
            head.entrypoint = str_of(&v, "entrypoint");
            head.started_at = str_of(&v, "timestamp").as_deref().and_then(parse_rfc3339_ms);
        }
    }
    if head.first_prompt.is_none() && v.get("type").and_then(Value::as_str) == Some("user") {
        head.first_prompt = user_prompt(&v);
    }
}

pub type Parsed = (HeadInfo, TranscriptInfo, TailState);

/// How much of the end of a transcript is read when there's no earlier position to resume from.
const FRESH_TAIL: u64 = 256 * 1024;

/// Parses a transcript. With the result of an earlier parse, only lines added since are read.
pub fn parse_transcript(path: &Path, mtime: i64, prev: Option<Parsed>) -> Option<Parsed> {
    let len = std::fs::metadata(path).ok()?.len();
    let resume = prev.filter(|(_, _, st)| st.offset <= len && len - st.offset <= 4 * FRESH_TAIL);
    let (head, mut info, mut state, text, start) = match resume {
        Some((head, info, state)) => {
            let start = state.offset;
            let text = read_range(path, start, len)?;
            (head, info, state, text, start)
        }
        None => {
            let mut head = read_head_info(path)?;
            if head.session_id.is_empty() {
                head.session_id = file_name(path).trim_end_matches(".jsonl").to_string();
            }
            let start = len.saturating_sub(FRESH_TAIL);
            let mut text = read_range(path, start, len)?;
            let mut start = start;
            if start > 0 {
                // Drop the partial first line.
                match text.find('\n') {
                    Some(i) => {
                        start += i as u64 + 1;
                        text.drain(..=i);
                    }
                    None => text.clear(),
                }
            }
            let info = from_head(&head);
            (head, info, TailState::default(), text, start)
        }
    };
    // Only whole lines are consumed; a line still being written is read next time.
    let complete = text.rfind('\n').map(|i| i + 1).unwrap_or(0);
    apply_tail(&mut info, &mut state, &text[..complete]);
    state.offset = start + complete as u64;
    if info.last_ts == 0 {
        info.last_ts = mtime;
    }
    Some((head, info, state))
}

fn read_range(path: &Path, start: u64, end: u64) -> Option<String> {
    let mut f = File::open(path).ok()?;
    f.seek(SeekFrom::Start(start)).ok()?;
    let mut buf = Vec::with_capacity(end.saturating_sub(start) as usize);
    f.take(end.saturating_sub(start)).read_to_end(&mut buf).ok()?;
    Some(String::from_utf8_lossy(&buf).into_owned())
}

fn from_head(head: &HeadInfo) -> TranscriptInfo {
    TranscriptInfo {
        session_id: head.session_id.clone(),
        cwd: head.cwd.clone(),
        git_branch: head.git_branch.clone(),
        entrypoint: head.entrypoint.clone(),
        started_at: head.started_at,
        first_prompt: head.first_prompt.clone(),
        ..Default::default()
    }
}

/// Parses head and tail text; used by tests.
#[cfg(test)]
pub fn parse_lines(head: &str, tail: &str) -> TranscriptInfo {
    let mut h = HeadInfo::default();
    for line in head.lines() {
        head_line(&mut h, line);
    }
    let mut info = from_head(&h);
    apply_tail(&mut info, &mut TailState::default(), tail);
    info
}

/// Titles, the last prompt, activity and pending tool calls from transcript lines.
pub fn apply_tail(info: &mut TranscriptInfo, state: &mut TailState, tail: &str) {
    let open_tools = &mut state.open_tools;
    for line in tail.lines() {
        let Ok(v) = serde_json::from_str::<Value>(line) else { continue };
        if let Some(ts) = str_of(&v, "timestamp").as_deref().and_then(parse_rfc3339_ms) {
            info.last_ts = info.last_ts.max(ts);
        }
        match v.get("type").and_then(Value::as_str) {
            Some("custom-title") => {
                if let Some(t) = str_of(&v, "customTitle").filter(|t| !t.trim().is_empty()) {
                    info.title = Some(one_line(&t, 90));
                }
            }
            Some("ai-title") => {
                if let Some(t) = str_of(&v, "aiTitle").filter(|t| !t.trim().is_empty()) {
                    info.ai_title = Some(one_line(&t, 90));
                }
            }
            Some("agent-name") => {
                if info.title.is_none() {
                    info.title = str_of(&v, "agentName").filter(|t| !t.trim().is_empty()).map(|t| one_line(&t, 90));
                }
            }
            Some("last-prompt") => {
                info.last_prompt = str_of(&v, "lastPrompt").map(|p| one_line(&p, 140));
            }
            Some("assistant") => {
                let Some(items) = v.pointer("/message/content").and_then(Value::as_array) else { continue };
                for item in items {
                    match item.get("type").and_then(Value::as_str) {
                        Some("tool_use") => {
                            let name = str_of(item, "name").unwrap_or_default();
                            let id = str_of(item, "id").unwrap_or_default();
                            let input = item.get("input").cloned().unwrap_or(Value::Null);
                            info.last_tool = Some(tool_activity(&name, &input));
                            open_tools.push((id, tool_request(&name, &input)));
                        }
                        Some("text") => {
                            if let Some(t) = str_of(item, "text").filter(|t| !t.trim().is_empty()) {
                                info.last_text = Some(one_line(&t, 140));
                            }
                        }
                        _ => {}
                    }
                }
            }
            Some("user") => {
                if let Some(items) = v.pointer("/message/content").and_then(Value::as_array) {
                    for item in items {
                        if item.get("type").and_then(Value::as_str) == Some("tool_result") {
                            if let Some(id) = str_of(item, "tool_use_id") {
                                open_tools.retain(|(open, _)| open != &id);
                            }
                        }
                    }
                }
                if user_prompt(&v).is_some() {
                    open_tools.clear();
                }
            }
            _ => {}
        }
    }
    info.pending_tool = open_tools.last().map(|(_, req)| req.clone());
}

fn str_of(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(String::from)
}

/// A real prompt you typed, not a tool result, command wrapper or meta line.
fn user_prompt(v: &Value) -> Option<String> {
    if v.get("isMeta").and_then(Value::as_bool) == Some(true) || v.get("isSidechain").and_then(Value::as_bool) == Some(true) {
        return None;
    }
    let content = v.pointer("/message/content")?;
    let text = match content {
        Value::String(s) => s.clone(),
        Value::Array(items) => items
            .iter()
            .find(|i| i.get("type").and_then(Value::as_str) == Some("text"))
            .and_then(|i| str_of(i, "text"))?,
        _ => return None,
    };
    let t = text.trim();
    if t.is_empty() || t.starts_with('<') || t.starts_with("Caveat:") || t.starts_with("[Request interrupted") {
        return None;
    }
    Some(one_line(t, 140))
}

fn short_path(p: &str) -> String {
    Path::new(p).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| p.to_string())
}

fn input_str(input: &Value, key: &str) -> Option<String> {
    input.get(key).and_then(Value::as_str).map(String::from)
}

/// What a pending tool call asks for, e.g. "Bash · `yarn mikro-orm migration:up`".
pub fn tool_request(name: &str, input: &Value) -> String {
    let detail = match name {
        "Bash" => input_str(input, "command").map(|c| code(&one_line(&c, 80))),
        "Edit" | "Write" | "MultiEdit" | "NotebookEdit" | "Read" => input_str(input, "file_path").map(|p| code(&short_path(&p))),
        "WebFetch" => input_str(input, "url").map(|u| code(&one_line(&u, 60))),
        "WebSearch" => input_str(input, "query").map(|q| one_line(&q, 60)),
        "Task" | "Agent" => input_str(input, "description").map(|d| one_line(&d, 60)),
        _ => None,
    };
    let label = name.strip_prefix("mcp__").map(|rest| rest.replace("__", " · ")).unwrap_or_else(|| name.to_string());
    match detail {
        Some(d) => format!("{label} · {d}"),
        None => label,
    }
}

/// The same call as an activity, e.g. "running `yarn test`".
pub fn tool_activity(name: &str, input: &Value) -> String {
    match name {
        "Bash" => input_str(input, "command").map(|c| format!("running {}", code(&one_line(&c, 60)))),
        "Edit" | "Write" | "MultiEdit" | "NotebookEdit" => input_str(input, "file_path").map(|p| format!("editing {}", code(&short_path(&p)))),
        "Read" => input_str(input, "file_path").map(|p| format!("reading {}", code(&short_path(&p)))),
        "Grep" | "Glob" => input_str(input, "pattern").map(|p| format!("searching {}", code(&one_line(&p, 40)))),
        "WebFetch" | "WebSearch" => Some("browsing the web".to_string()),
        "Task" | "Agent" => Some("running a subagent".to_string()),
        _ => None,
    }
    .unwrap_or_else(|| format!("using {}", code(name)))
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEAD: &str = r#"{"type":"permission-mode","mode":"default"}
{"type":"user","cwd":"/Users/me/projects/atlasai/atlas-quarry","sessionId":"25b09e0e","gitBranch":"quarry","entrypoint":"cli","timestamp":"2026-09-23T14:00:00.000Z","message":{"role":"user","content":"Can you make a photo write tool?"}}
"#;

    #[test]
    fn reads_identity_titles_and_pending_tools() {
        let tail = r#"{"type":"custom-title","customTitle":"Quarry daily log + photo sync","sessionId":"25b09e0e"}
{"type":"assistant","timestamp":"2026-09-23T15:00:00.000Z","message":{"content":[{"type":"text","text":"Running the migration now."},{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"yarn mikro-orm migration:up"}}]}}
{"type":"last-prompt","lastPrompt":"apply the migration"}
"#;
        let info = parse_lines(HEAD, tail);
        assert_eq!(info.cwd, "/Users/me/projects/atlasai/atlas-quarry");
        assert_eq!(info.session_id, "25b09e0e");
        assert_eq!(info.git_branch.as_deref(), Some("quarry"));
        assert_eq!(info.title.as_deref(), Some("Quarry daily log + photo sync"));
        assert_eq!(info.first_prompt.as_deref(), Some("Can you make a photo write tool?"));
        assert_eq!(info.pending_tool.as_deref(), Some("Bash · `yarn mikro-orm migration:up`"));
        assert_eq!(info.last_tool.as_deref(), Some("running `yarn mikro-orm migration:up`"));
        assert_eq!(info.last_text.as_deref(), Some("Running the migration now."));
        assert_eq!(info.last_ts, parse_rfc3339_ms("2026-09-23T15:00:00.000Z").unwrap());
    }

    #[test]
    fn resolved_tools_are_not_pending() {
        let tail = r#"{"type":"assistant","timestamp":"2026-09-23T15:00:00.000Z","message":{"content":[{"type":"tool_use","id":"t1","name":"Edit","input":{"file_path":"/x/src/App.tsx"}}]}}
{"type":"user","timestamp":"2026-09-23T15:00:01.000Z","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":"ok"}]}}
"#;
        let info = parse_lines(HEAD, tail);
        assert_eq!(info.pending_tool, None);
        assert_eq!(info.last_tool.as_deref(), Some("editing `App.tsx`"));
    }

    #[test]
    fn skips_meta_prompts() {
        let head = r#"{"type":"user","cwd":"/x","sessionId":"s","message":{"content":"<command-name>/clear</command-name>"}}
{"type":"user","cwd":"/x","sessionId":"s","isMeta":true,"message":{"content":"Caveat: meta"}}
{"type":"user","cwd":"/x","sessionId":"s","message":{"content":[{"type":"text","text":"Fix the flaky test"}]}}
"#;
        assert_eq!(parse_lines(head, "").first_prompt.as_deref(), Some("Fix the flaky test"));
    }

    #[test]
    fn finds_the_first_prompt_past_huge_lines() {
        let dir = std::env::temp_dir().join(format!("codealong-head-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("s.jsonl");
        let big = format!("{{\"type\":\"attachment\",\"blob\":\"{}\"}}\n", "x".repeat(600_000));
        let text = format!("{big}{HEAD}");
        std::fs::write(&path, text).unwrap();
        let head = read_head_info(&path).unwrap();
        assert_eq!(head.cwd, "/Users/me/projects/atlasai/atlas-quarry");
        assert_eq!(head.first_prompt.as_deref(), Some("Can you make a photo write tool?"));
        let (head, info, state) = parse_transcript(&path, 1, None).unwrap();
        assert_eq!(info.session_id, "25b09e0e");
        assert_eq!(head.first_prompt.as_deref(), Some("Can you make a photo write tool?"));

        // Appending reads only the new lines, including a line finished later.
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
        write!(f, "{{\"type\":\"custom-title\",\"customTitle\":\"Photo tools\"}}\n{{\"type\":\"assistant\",\"message\":{{\"content\":[{{\"type\":\"tool_use\",\"id\":\"t9\",\"name\":\"Bash\",").unwrap();
        drop(f);
        let (head, info, state) = parse_transcript(&path, 2, Some((head, info, state))).unwrap();
        assert_eq!(info.title.as_deref(), Some("Photo tools"));
        assert_eq!(info.pending_tool, None, "a half-written line waits");
        let mut f = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
        writeln!(f, "\"input\":{{\"command\":\"yarn test\"}}}}]}}}}").unwrap();
        drop(f);
        let (_, info, _) = parse_transcript(&path, 3, Some((head, info, state))).unwrap();
        assert_eq!(info.pending_tool.as_deref(), Some("Bash · `yarn test`"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn parses_registry_entries() {
        let e = parse_registry(r#"{"pid":22303,"sessionId":"9f62","cwd":"/x","status":"busy","kind":"interactive","entrypoint":"cli","name":"atlas-app4-4b","nameSource":"derived","startedAt":1,"statusUpdatedAt":2}"#).unwrap();
        assert_eq!(e.status, "busy");
        assert_eq!(e.name.as_deref(), Some("atlas-app4-4b"));
        assert!(parse_registry(r#"{"pid":1}"#).is_none());
    }

    #[test]
    fn describes_tools() {
        let input = serde_json::json!({"command": "git status\n--short"});
        assert_eq!(tool_request("Bash", &input), "Bash · `git status`");
        assert_eq!(tool_request("mcp__northwind__search", &Value::Null), "northwind · search");
    }
}
