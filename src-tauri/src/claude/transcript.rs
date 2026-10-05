//! Claude Code's transcripts (`<home>/projects/<encoded cwd>/<session>.jsonl`): what the start
//! of one says of its session, what its end says the session is doing now, which lines are
//! prompts you typed, and how a tool call reads.

use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::conversation::{line_peer, sent_from_codealong};
use crate::model::SessionLimit;
use crate::util::{code, command_dir, file_name, mtime_ms, note_work, one_line, parse_rfc3339_ms, read_head, read_tail, resolve, scan_lines_back, without_cd};

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
    /// When you last sent a prompt. It doesn't move while the session works, so lists sort by it.
    pub last_prompt_at: Option<i64>,
    pub started_at: Option<i64>,
    pub last_ts: i64,
    /// The tool call waiting on a result, e.g. "Bash · `yarn test`".
    pub pending_tool: Option<String>,
    /// The latest tool call as an activity, e.g. "editing `App.tsx`".
    pub last_tool: Option<String>,
    /// First line of the latest assistant text.
    pub last_text: Option<String>,
    /// The end of the latest assistant text, for Jev.
    pub last_text_tail: Option<String>,
    /// Where its latest commands ran and edits landed, oldest first.
    pub work: Vec<String>,
    /// The session the conversation went on in when it moved to the background (a
    /// `continued-in` line), while nothing has been said here since.
    pub continued_in: Option<String>,
    /// The usage limit the last turn ended on, until a reply or a prompt of yours comes after.
    pub limit: Option<SessionLimit>,
    /// The model the latest reply came from.
    pub model: Option<String>,
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
    let fresh = resume.is_none();
    let (head, mut info, mut state, tail) = match resume {
        Some((head, info, state)) => {
            let tail = read_tail(path, state.offset).ok()?;
            (head, info, state, tail)
        }
        None => {
            let mut head = read_head_info(path)?;
            if head.session_id.is_empty() {
                head.session_id = file_name(path).trim_end_matches(".jsonl").to_string();
            }
            let tail = read_tail(path, len.saturating_sub(FRESH_TAIL)).ok()?;
            let info = from_head(&head);
            (head, info, TailState::default(), tail)
        }
    };
    // Only whole lines are read; a line still being written is read next time.
    apply_tail(&mut info, &mut state, &tail.text());
    // One turn can write far more than the tail holds, so the prompt that started it may be
    // further back. A first read looks back for it once; later reads carry it forward.
    if fresh && info.last_prompt_at.is_none() {
        info.last_prompt_at = prompt_before(path, tail.start);
    }
    state.offset = tail.end;
    if info.last_ts == 0 {
        info.last_ts = mtime;
    }
    Some((head, info, state))
}

/// When the last prompt you typed before `end` (a line start) was sent, reading backwards.
fn prompt_before(path: &Path, end: u64) -> Option<i64> {
    let mut at = None;
    scan_lines_back(path, 0, end, |line| {
        // Most lines are tool calls and their results; only a prompt can pass both checks.
        if !line.contains("\"type\":\"user\"") || line.contains("\"tool_use_id\"") {
            return false;
        }
        let Ok(v) = serde_json::from_str::<Value>(line) else { return false };
        if !you_typed(&v) {
            return false;
        }
        at = str_of(&v, "timestamp").as_deref().and_then(parse_rfc3339_ms);
        at.is_some()
    });
    at
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
        let kind = v.get("type").and_then(Value::as_str);
        // Moving the conversation to the background isn't something it did.
        if let Some(ts) = str_of(&v, "timestamp").as_deref().and_then(parse_rfc3339_ms).filter(|_| kind != Some("continued-in")) {
            info.last_ts = info.last_ts.max(ts);
        }
        // Sessions switch branches; the latest line says which one it's on now.
        if let Some(b) = str_of(&v, "gitBranch").filter(|b| !b.is_empty() && b != "HEAD") {
            info.git_branch = Some(b);
        }
        // A conversation that goes on here after it moved is back here.
        if matches!(kind, Some("user") | Some("assistant")) {
            info.continued_in = None;
        }
        match kind {
            Some("continued-in") => {
                info.continued_in = str_of(&v, "continuedInSessionId").filter(|id| !id.is_empty());
            }
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
                // A limit's refusal is written as a reply; any other reply means it went on.
                info.limit = limit_of(&v).map(|l| SessionLimit { model: info.model.clone(), ..l });
                if let Some(m) = v.pointer("/message/model").and_then(Value::as_str).filter(|m| !m.starts_with('<')) {
                    info.model = Some(m.to_string());
                }
                let Some(items) = v.pointer("/message/content").and_then(Value::as_array) else { continue };
                // The shell's folder when the call was made: a `cd` inside the session's
                // folders carries over to later commands, and shows up here.
                let cwd = str_of(&v, "cwd").unwrap_or_else(|| info.cwd.clone());
                for item in items {
                    match item.get("type").and_then(Value::as_str) {
                        Some("tool_use") => {
                            let name = str_of(item, "name").unwrap_or_default();
                            let id = str_of(item, "id").unwrap_or_default();
                            let input = item.get("input").cloned().unwrap_or(Value::Null);
                            info.last_tool = Some(tool_activity(&name, &input));
                            open_tools.push((id, tool_request(&name, &input)));
                            if let Some(p) = work_path(&name, &input, &cwd) {
                                note_work(&mut info.work, p);
                            }
                        }
                        Some("text") => {
                            if let Some(t) = str_of(item, "text").filter(|t| !t.trim().is_empty()) {
                                info.last_text = Some(one_line(&t, 140));
                                info.last_text_tail = Some(crate::jev::tail(&t, 600));
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
                if you_typed(&v) {
                    info.limit = None;
                    if let Some(ts) = str_of(&v, "timestamp").as_deref().and_then(parse_rfc3339_ms) {
                        info.last_prompt_at = Some(ts);
                    }
                }
            }
            _ => {}
        }
    }
    info.pending_tool = open_tools.last().map(|(_, req)| req.clone());
}

/// The usage limit an assistant line says the API refused the turn on: Claude Code writes the
/// refusal as a reply with `isApiErrorMessage`, `error: "rate_limit"` and `apiErrorStatus: 429`,
/// and from 2.1.257 a `quotaLimits` object (`rateLimitType` `five_hour` or `seven_day`,
/// `resetsAt` in seconds, and `overageStatus` `rejected` when extra usage couldn't carry it; its
/// `overageDisabledReason` is why: `out_of_credits`, or `org_level_disabled_until` behind the
/// "monthly spend limit" message). Read from 131 refusals on this Mac, 2026-10-02. A model
/// that's unavailable is a `rate_limit` too, but not a 429, and moving wouldn't help it.
pub(super) fn limit_of(v: &Value) -> Option<SessionLimit> {
    let refused = v.get("isApiErrorMessage").and_then(Value::as_bool) == Some(true)
        && v.get("error").and_then(Value::as_str) == Some("rate_limit")
        && v.get("apiErrorStatus").and_then(Value::as_i64) == Some(429);
    if !refused {
        return None;
    }
    let q = v.get("quotaLimits");
    let field = |k: &str| q.and_then(|q| q.get(k));
    let meter = field("rateLimitType").and_then(Value::as_str).and_then(|t| match t {
        "five_hour" => Some("session".to_string()),
        "seven_day" => Some("week".to_string()),
        // `seven_day_opus` and the like, as the usage endpoint's per-model weeks are keyed.
        _ => t.strip_prefix("seven_day_").filter(|m| !m.is_empty()).map(|m| format!("model:{}", crate::usage::title(m))),
    });
    Some(SessionLimit {
        meter,
        resets_at: field("resetsAt").and_then(Value::as_i64).map(|s| s * 1000),
        spend_capped: field("overageStatus").and_then(Value::as_str) == Some("rejected"),
        at: str_of(v, "timestamp").as_deref().and_then(parse_rfc3339_ms).unwrap_or(0),
        move_to: None,
        model: None,
    })
}

fn str_of(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(String::from)
}

/// A real prompt you typed, not a tool result, command wrapper, compaction summary or meta line.
fn user_prompt(v: &Value) -> Option<String> {
    let flag = |k: &str| v.get(k).and_then(Value::as_bool) == Some(true);
    if flag("isMeta") || flag("isSidechain") || flag("isCompactSummary") {
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

/// A turn you took: a prompt you typed, pasted or ran as a slash command, or sent from
/// Codealong. Claude Code marks what's typed `origin: {kind: "human"}`; a background task's
/// notification (`task-notification`) or another session's message (`peer`) is marked
/// otherwise, and what you send a Claude app session from Codealong comes as another session's
/// message (`sent_from_codealong`). Older transcripts have no mark, so for them this is whatever
/// `user_prompt` takes for a prompt.
pub(super) fn you_typed(v: &Value) -> bool {
    if v.get("type").and_then(Value::as_str) != Some("user") {
        return false;
    }
    let flag = |k: &str| v.get(k).and_then(Value::as_bool) == Some(true);
    if flag("isSidechain") || flag("isCompactSummary") {
        return false;
    }
    // Claude Code marks another session's message as meta, yours from Codealong included.
    if let Some(peer) = line_peer(v) {
        return sent_from_codealong(&peer).is_some();
    }
    if flag("isMeta") {
        return false;
    }
    match v.pointer("/origin/kind").and_then(Value::as_str) {
        Some(kind) => kind == "human",
        None => user_prompt(v).is_some(),
    }
}

/// The whole of a prompt you typed, for searching sessions' text (search.rs).
pub fn typed_text(v: &Value) -> Option<String> {
    if !you_typed(v) {
        return None;
    }
    if let Some(peer) = line_peer(v) {
        return sent_from_codealong(&peer);
    }
    let text = match v.pointer("/message/content")? {
        Value::String(s) => s.clone(),
        // The Claude app puts a system reminder of its own in front of the first prompt of a
        // chat it started without a folder, as a part of its own: what you typed follows it.
        Value::Array(items) => items
            .iter()
            .filter(|i| i.get("type").and_then(Value::as_str) == Some("text"))
            .filter_map(|i| i.get("text").and_then(Value::as_str))
            .filter(|t| !t.trim_start().starts_with("<system-reminder>"))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => return None,
    };
    let t = text.trim();
    (!t.is_empty() && !t.starts_with('<') && !t.starts_with("[Request interrupted")).then(|| t.to_string())
}

/// What an agent said in an assistant line, its text parts joined; tool calls and thinking
/// aren't text. Subagents' lines are left out.
pub fn reply_text(v: &Value) -> Option<String> {
    if v.get("type").and_then(Value::as_str) != Some("assistant") || v.get("isSidechain").and_then(Value::as_bool) == Some(true) {
        return None;
    }
    let parts = v.pointer("/message/content")?.as_array()?;
    let text = parts.iter().filter(|p| p.get("type").and_then(Value::as_str) == Some("text")).filter_map(|p| p.get("text").and_then(Value::as_str)).collect::<Vec<_>>().join("\n");
    let t = text.trim();
    (!t.is_empty()).then(|| t.to_string())
}

fn short_path(p: &str) -> String {
    Path::new(p).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| p.to_string())
}

fn input_str(input: &Value, key: &str) -> Option<String> {
    input.get(key).and_then(Value::as_str).map(String::from)
}

/// Where a tool call works: the folder a command runs in, or the file an edit changes. Reads
/// and searches don't count; agents often look into other worktrees without working there.
fn work_path(name: &str, input: &Value, cwd: &str) -> Option<String> {
    match name {
        "Bash" => command_dir(&input_str(input, "command")?, cwd),
        "Edit" | "Write" | "MultiEdit" => Some(resolve(cwd, &input_str(input, "file_path")?)),
        "NotebookEdit" => Some(resolve(cwd, &input_str(input, "notebook_path")?)),
        _ => None,
    }
}

/// How plan mode's approval (`ExitPlanMode`) reads: its call is the plan, waiting for your yes.
pub const PLAN: &str = "Plan";

/// What a pending tool call asks for, e.g. "Bash · `yarn mikro-orm migration:up`".
pub fn tool_request(name: &str, input: &Value) -> String {
    if name == "ExitPlanMode" {
        return PLAN.to_string();
    }
    let detail = match name {
        "Bash" => input_str(input, "command").map(|c| code(&one_line(without_cd(&c), 80))),
        "Edit" | "Write" | "MultiEdit" | "NotebookEdit" | "Read" => input_str(input, "file_path").map(|p| code(&short_path(&p))),
        "WebFetch" => input_str(input, "url").map(|u| code(&one_line(&u, 60))),
        "WebSearch" => input_str(input, "query").map(|q| one_line(&q, 60)),
        "Task" | "Agent" => input_str(input, "description").map(|d| one_line(&d, 60)),
        // A publish names its file, or the title of what it makes from a type; anything else
        // (read, list, open…) says which.
        "Artifact" => match input_str(input, "action").as_deref() {
            None | Some("publish") => input_str(input, "file_path").map(|p| code(&short_path(&p))).or_else(|| input_str(input, "title").map(|t| one_line(&t, 60))),
            Some(action) => Some(one_line(action, 20)),
        },
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
        "Bash" => input_str(input, "command").map(|c| format!("running {}", code(&one_line(without_cd(&c), 60)))),
        "Edit" | "Write" | "MultiEdit" | "NotebookEdit" => input_str(input, "file_path").map(|p| format!("editing {}", code(&short_path(&p)))),
        "Read" => input_str(input, "file_path").map(|p| format!("reading {}", code(&short_path(&p)))),
        "Grep" | "Glob" => input_str(input, "pattern").map(|p| format!("searching {}", code(&one_line(&p, 40)))),
        "WebFetch" | "WebSearch" => Some("browsing the web".to_string()),
        "Task" | "Agent" => Some("running a subagent".to_string()),
        _ => None,
    }
    .unwrap_or_else(|| format!("using {}", code(name)))
}

/// The PR a transcript line says a command made or changed, by its URL, with the line's time:
/// Claude Code marks a command's result with `toolUseResult.gitOperation.pr` (`number`, `url`,
/// `action`: created, edited, commented, closed) when the command acted on a PR. A closed one
/// has no URL. Only lines with that mark are parsed.
/// A PR the session made or changed, from one of two marks on a line: a command's result that
/// acted on a PR (`toolUseResult.gitOperation.pr`, written for `gh pr create` and the like), or
/// Claude Code's own `pr-link` line (`prNumber`, `prUrl`, `prRepository`), which it writes once
/// a command created a PR or pushed to one, however the command did it (`gh api
/// repos/…/pulls`, a script), and again with every re-stamp of its metadata (2.1.289's
/// `linkSessionToPR`; a `gh pr list` or `gh pr view` writes none).
pub(super) fn pr_made(line: &str) -> Option<(String, i64)> {
    let operated = line.contains("\"gitOperation\"") && line.contains("\"pr\":{");
    let linked = line.contains("\"type\":\"pr-link\"");
    if !(operated || linked) {
        return None;
    }
    let v: Value = serde_json::from_str(line).ok()?;
    let at = v.get("timestamp").and_then(Value::as_str).and_then(parse_rfc3339_ms).unwrap_or(0);
    if v.get("type").and_then(Value::as_str) == Some("pr-link") {
        return Some((v.get("prUrl")?.as_str()?.to_string(), at));
    }
    Some((v.pointer("/toolUseResult/gitOperation/pr/url")?.as_str()?.to_string(), at))
}


/// The start of a transcript, for this folder's tests.
#[cfg(test)]
pub(super) const HEAD: &str = r#"{"type":"permission-mode","mode":"default"}
{"type":"user","cwd":"/Users/me/projects/sparkhq/spark-portal","sessionId":"25b09e0e","gitBranch":"portal","entrypoint":"cli","timestamp":"2026-09-23T14:00:00.000Z","message":{"role":"user","content":"Can you make a photo write tool?"}}
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::claude::conversation::claude_input;
    use crate::claude::peer::FROM_YOU;

    #[test]
    fn reads_identity_titles_and_pending_tools() {
        let tail = r#"{"type":"custom-title","customTitle":"Portal daily log + photo sync","sessionId":"25b09e0e"}
{"type":"assistant","timestamp":"2026-09-23T15:00:00.000Z","message":{"content":[{"type":"text","text":"Running the migration now."},{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"yarn mikro-orm migration:up"}}]}}
{"type":"last-prompt","lastPrompt":"apply the migration"}
"#;
        let info = parse_lines(HEAD, tail);
        assert_eq!(info.cwd, "/Users/me/projects/sparkhq/spark-portal");
        assert_eq!(info.session_id, "25b09e0e");
        assert_eq!(info.git_branch.as_deref(), Some("portal"));
        assert_eq!(info.title.as_deref(), Some("Portal daily log + photo sync"));
        assert_eq!(info.first_prompt.as_deref(), Some("Can you make a photo write tool?"));
        assert_eq!(info.pending_tool.as_deref(), Some("Bash · `yarn mikro-orm migration:up`"));
        assert_eq!(info.last_tool.as_deref(), Some("running `yarn mikro-orm migration:up`"));
        assert_eq!(info.last_text.as_deref(), Some("Running the migration now."));
        assert_eq!(info.last_text_tail.as_deref(), Some("Running the migration now."));
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
    fn records_where_commands_ran_and_edits_landed() {
        let tail = r#"{"type":"assistant","cwd":"/Users/me/projects/sparkhq/spark-portal","message":{"content":[{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"git worktree add ../spark-fix"}}]}}
{"type":"assistant","cwd":"/Users/me/projects/sparkhq/spark-portal","message":{"content":[{"type":"tool_use","id":"t2","name":"Bash","input":{"command":"cd ../spark-fix && yarn test"}}]}}
{"type":"assistant","cwd":"/Users/me/projects/sparkhq/spark-portal","message":{"content":[{"type":"tool_use","id":"t3","name":"Edit","input":{"file_path":"/Users/me/projects/sparkhq/spark-fix/src/a.ts"}}]}}
{"type":"assistant","cwd":"/Users/me/projects/sparkhq/spark-fix","message":{"content":[{"type":"tool_use","id":"t4","name":"Bash","input":{"command":"yarn lint"}}]}}
{"type":"assistant","cwd":"/Users/me/projects/sparkhq/spark-fix","message":{"content":[{"type":"tool_use","id":"t5","name":"Bash","input":{"command":"cd /Users/me/projects/sparkhq/spark && git show origin/main:README.md | head -40"}}]}}
{"type":"assistant","cwd":"/Users/me/projects/sparkhq/spark-fix","message":{"content":[{"type":"tool_use","id":"t6","name":"Read","input":{"file_path":"/Users/me/projects/sparkhq/spark/README.md"}}]}}
"#;
        let info = parse_lines(HEAD, tail);
        // Making the worktree names a folder outside the one it runs in, so it isn't work there;
        // nor is reading another checkout, with the Read tool or from a shell that went there.
        assert_eq!(
            info.work,
            [
                "/Users/me/projects/sparkhq/spark-fix",
                "/Users/me/projects/sparkhq/spark-fix/src/a.ts",
                "/Users/me/projects/sparkhq/spark-fix",
            ]
        );
        assert_eq!(info.last_tool.as_deref(), Some("reading `README.md`"));
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
        let dir = crate::fixtures::scratch("head");
        let path = dir.join("s.jsonl");
        let big = format!("{{\"type\":\"attachment\",\"blob\":\"{}\"}}\n", "x".repeat(600_000));
        let text = format!("{big}{HEAD}");
        std::fs::write(&path, text).unwrap();
        let head = read_head_info(&path).unwrap();
        assert_eq!(head.cwd, "/Users/me/projects/sparkhq/spark-portal");
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
    }

    #[test]
    fn times_the_last_prompt_you_typed() {
        let tail = r#"{"type":"user","timestamp":"2026-09-23T15:00:00.000Z","origin":{"kind":"human"},"message":{"content":"<pasted_content id=\"a\">\nlog\n</pasted_content id=\"a\">\n\nwhy does this fail?"}}
{"type":"assistant","timestamp":"2026-09-23T15:00:05.000Z","message":{"content":[{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"yarn test"}}]}}
{"type":"user","timestamp":"2026-09-23T15:00:09.000Z","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":"ok"}]}}
{"type":"user","timestamp":"2026-09-23T15:01:00.000Z","origin":{"kind":"task-notification"},"message":{"content":"<task-notification>\n<task-id>b1</task-id>\n</task-notification>"}}
{"type":"user","timestamp":"2026-09-23T15:02:00.000Z","origin":{"kind":"peer","from":"review"},"isMeta":true,"message":{"content":"Another Claude session sent a message"}}
{"type":"user","timestamp":"2026-09-23T15:03:00.000Z","isCompactSummary":true,"isVisibleInTranscriptOnly":true,"message":{"content":"This session is being continued from a previous conversation."}}
"#;
        let info = parse_lines(HEAD, tail);
        assert_eq!(info.last_prompt_at, parse_rfc3339_ms("2026-09-23T15:00:00.000Z"));
        // Transcripts from before Claude Code marked prompts: a plain prompt counts.
        let old = r#"{"type":"user","timestamp":"2026-09-23T16:00:00.000Z","message":{"content":"Now the migration"}}
"#;
        assert_eq!(parse_lines(HEAD, old).last_prompt_at, parse_rfc3339_ms("2026-09-23T16:00:00.000Z"));
        assert_eq!(parse_lines(HEAD, "").last_prompt_at, None);
    }

    #[test]
    fn a_prompt_behind_the_claude_apps_system_reminder_is_what_you_typed() {
        let line = |parts: &[&str]| serde_json::json!({ "type": "user", "origin": { "kind": "human" }, "message": { "content": parts.iter().map(|t| serde_json::json!({ "type": "text", "text": t })).collect::<Vec<_>>() } });
        let reminder = "<system-reminder>\nThe user started this session without choosing a project folder.\n</system-reminder>\n\n";
        assert_eq!(typed_text(&line(&[reminder, "Give Graham access on GitHub"])).as_deref(), Some("Give Graham access on GitHub"));
        assert_eq!(typed_text(&line(&[reminder])), None);
        assert_eq!(typed_text(&line(&["<command-name>/clear</command-name>"])), None);
    }

    #[test]
    fn what_you_send_from_codealong_is_a_prompt_you_typed() {
        let peer = |at: &str, name: &str, body: &str| serde_json::json!({ "type": "user", "isMeta": true, "timestamp": at, "origin": { "kind": "peer", "from": "unknown", "name": name, "body": body }, "message": { "content": "Another Claude session sent a message" } }).to_string();
        let tail = [
            r#"{"type":"user","timestamp":"2026-09-23T15:00:00.000Z","origin":{"kind":"human"},"message":{"content":"Fix the build"}}"#.to_string(),
            peer("2026-09-23T15:05:00.000Z", "Codealong", &format!("{}\n\nNow the docs", FROM_YOU)),
            // An agent's isn't, whatever it calls itself.
            peer("2026-09-23T15:06:00.000Z", "Codealong", "This message is from another agent, “x”.\n\nhi"),
            peer("2026-09-23T15:07:00.000Z", "review", &format!("{}\n\nhi", FROM_YOU)),
        ]
        .join("\n");
        let info = parse_lines(HEAD, &(tail + "\n"));
        assert_eq!(info.last_prompt_at, parse_rfc3339_ms("2026-09-23T15:05:00.000Z"));
    }

    #[test]
    fn finds_the_last_prompt_behind_a_long_turn() {
        let dir = crate::fixtures::scratch("prompt");
        let path = dir.join("s.jsonl");
        let prompt = r#"{"type":"user","timestamp":"2026-09-23T15:00:00.000Z","origin":{"kind":"human"},"message":{"content":"Refactor the parser"}}"#;
        let output = |i: usize| format!(r#"{{"type":"user","timestamp":"2026-09-23T15:10:00.000Z","message":{{"content":[{{"type":"tool_result","tool_use_id":"t{i}","content":"{}"}}]}}}}"#, "x".repeat(120_000));
        let mut lines = vec![HEAD.trim_end().to_string(), prompt.to_string()];
        lines.extend((0..12).map(output));
        std::fs::write(&path, lines.join("\n") + "\n").unwrap();
        let (_, info, _) = parse_transcript(&path, 1, None).unwrap();
        assert_eq!(info.last_prompt_at, parse_rfc3339_ms("2026-09-23T15:00:00.000Z"));
    }

    #[test]
    fn a_conversation_moved_to_the_background_says_where_it_went() {
        // Moving it writes where it went, after the last turn's lines; that isn't activity.
        let moved = r#"{"type":"assistant","timestamp":"2026-09-28T18:38:00.000Z","message":{"content":[{"type":"text","text":"Landed."}]}}
{"type":"system","subtype":"turn_duration","timestamp":"2026-09-28T18:38:07.260Z"}
{"type":"cost-state","sessionId":"b664dd38"}
{"type":"continued-in","timestamp":"2026-09-29T02:27:21.233Z","sessionId":"b664dd38","continuedInSessionId":"398f3533-73b3-4089-af4e-06603a870e04"}
"#;
        let info = parse_lines(HEAD, moved);
        assert_eq!(info.continued_in.as_deref(), Some("398f3533-73b3-4089-af4e-06603a870e04"));
        assert_eq!(info.last_ts, parse_rfc3339_ms("2026-09-28T18:38:07.260Z").unwrap());
        // Taken back to its window and talked to, it's here again.
        let back = format!("{moved}{}\n", r#"{"type":"user","timestamp":"2026-09-29T02:30:00.000Z","origin":{"kind":"human"},"message":{"content":"One more thing"}}"#);
        assert_eq!(parse_lines(HEAD, &back).continued_in, None);
    }

    // A refusal as Claude Code 2.1.287 wrote it, cut to the fields that matter here.
    const SPEND_CAPPED: &str = r#"{"type":"assistant","timestamp":"2026-10-02T20:06:19.504Z","quotaLimits":{"status":"rejected","resetsAt":1791007200,"unifiedRateLimitFallbackAvailable":false,"rateLimitType":"seven_day","overageStatus":"rejected","overageDisabledReason":"org_level_disabled_until","isUsingOverage":false},"error":"rate_limit","isApiErrorMessage":true,"apiErrorStatus":429,"message":{"model":"<synthetic>","role":"assistant","content":[{"type":"text","text":"You've hit your monthly spend limit · raise it at claude.ai/settings/usage?from=cc_cli_lim"}]}}"#;

    #[test]
    fn a_turn_refused_at_a_limit_says_which_and_when_it_lifts() {
        let reply = r#"{"type":"assistant","timestamp":"2026-10-02T20:00:00.000Z","message":{"model":"claude-opus-5-5","content":[{"type":"text","text":"On it."}]}}"#;
        let info = parse_lines(HEAD, &format!("{reply}\n{SPEND_CAPPED}\n"));
        let limit = info.limit.expect("a limit");
        assert_eq!(limit.meter.as_deref(), Some("week"));
        assert_eq!(limit.resets_at, Some(1_791_007_200_000));
        assert!(limit.spend_capped);
        assert_eq!(limit.at, parse_rfc3339_ms("2026-10-02T20:06:19.504Z").unwrap());
        // The refusal's own model is Claude Code's stand-in; the session's is the reply's before.
        assert_eq!(limit.model.as_deref(), Some("claude-opus-5-5"));
        assert_eq!(info.last_text.as_deref(), Some("You've hit your monthly spend limit · raise it at claude.ai/settings/usage?from=cc_cli_lim"));

        let session = r#"{"type":"assistant","timestamp":"2026-09-08T23:46:12.959Z","quotaLimits":{"status":"rejected","resetsAt":1757378400,"rateLimitType":"five_hour","overageStatus":"rejected","overageDisabledReason":"out_of_credits"},"error":"rate_limit","isApiErrorMessage":true,"apiErrorStatus":429,"message":{"model":"<synthetic>","content":[{"type":"text","text":"You've hit your session limit · resets 6:10pm (America/Los_Angeles)"}]}}"#;
        assert_eq!(parse_lines(HEAD, session).limit.and_then(|l| l.meter).as_deref(), Some("session"));
        let opus = SPEND_CAPPED.replace(r#""seven_day""#, r#""seven_day_opus""#).replace(r#""overageStatus":"rejected""#, r#""overageStatus":"allowed""#);
        let limit = parse_lines(HEAD, &opus).limit.unwrap();
        assert_eq!((limit.meter.as_deref(), limit.spend_capped), (Some("model:Opus"), false));
    }

    #[test]
    fn only_a_429_is_a_limit_and_what_comes_after_lifts_it() {
        // A model that's unavailable is a rate_limit with no 429: no account would do better.
        let unavailable = r#"{"type":"assistant","timestamp":"2026-07-01T14:46:02.917Z","error":"rate_limit","isApiErrorMessage":true,"message":{"model":"<synthetic>","content":[{"type":"text","text":"Claude Fable 5 is currently unavailable."}]}}"#;
        assert_eq!(parse_lines(HEAD, unavailable).limit, None);
        // A background task's notice starts a turn that's refused again: still limited.
        let notice = r#"{"type":"user","timestamp":"2026-10-02T20:06:25.178Z","origin":{"kind":"task-notification"},"message":{"content":"<task-notification>done</task-notification>"}}"#;
        assert!(parse_lines(HEAD, &format!("{SPEND_CAPPED}\n{notice}\n")).limit.is_some());
        // A prompt you send lifts it while it's tried, and a reply that isn't refused for good.
        let prompt = r#"{"type":"user","timestamp":"2026-10-02T21:00:00.000Z","origin":{"kind":"human"},"message":{"content":"go on"}}"#;
        assert_eq!(parse_lines(HEAD, &format!("{SPEND_CAPPED}\n{prompt}\n")).limit, None);
        let reply = r#"{"type":"assistant","timestamp":"2026-10-02T21:00:05.000Z","message":{"model":"claude-opus-5-5","content":[{"type":"text","text":"Going on."}]}}"#;
        assert_eq!(parse_lines(HEAD, &format!("{SPEND_CAPPED}\n{notice}\n{reply}\n")).limit, None);
    }

    #[test]
    fn describes_tools() {
        let input = serde_json::json!({"command": "git status\n--short"});
        assert_eq!(tool_request("Bash", &input), "Bash · `git status`");
        // Plan mode's approval reads as its plan, not as the tool's JSON.
        let plan = serde_json::json!({ "plan": "# Fix the build\n\n1. Pin the toolchain" });
        assert_eq!(tool_request("ExitPlanMode", &plan), PLAN);
        assert_eq!(claude_input("ExitPlanMode", &plan), "# Fix the build\n\n1. Pin the toolchain");
        assert_eq!(tool_request("mcp__spruce__search", &Value::Null), "spruce · search");
        // The row already names the worktree a leading `cd` goes to.
        let input = serde_json::json!({"command": "cd /Users/me/projects/app-fix && yarn test"});
        assert_eq!(tool_request("Bash", &input), "Bash · `yarn test`");
        assert_eq!(tool_activity("Bash", &input), "running `yarn test`");
    }
}
