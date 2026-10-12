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
    /// The session whose lines the transcript starts with, when that's another one: a fork
    /// the Claude app made (its Fork, and a `btw`) copies its session's lines as they were,
    /// that session's ID on them included (2.1.293), where Claude Code's own forks rewrite it.
    pub copied_from: Option<String>,
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
    /// Tool calls still waiting on a result: (id, request, when it was made).
    pub open_tools: Vec<(String, String, i64)>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TranscriptInfo {
    pub session_id: String,
    /// `HeadInfo::copied_from`.
    pub copied_from: Option<String>,
    /// The folder it runs in: where the latest prompt you sent it was taken, else where it
    /// started. A session resumed in another folder keeps writing this transcript, in the
    /// first folder's project, and its lines say the new folder (checked on 2.1.289).
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
    /// When the last turn ended for want of a connection to the API (`connection_lost_at`),
    /// until a reply or a prompt of yours comes after.
    pub connection_lost: Option<i64>,
    /// When the last turn last did something, while it hasn't ended (`turn_step`). A session
    /// whose process is gone with one was cut off partway through it.
    pub unfinished: Option<i64>,
    /// The model the latest reply came from.
    pub model: Option<String>,
    /// A message from another session it holds for you to allow (held.rs), until a line says
    /// it went.
    pub held: Option<super::held::Held>,
    /// When the tool call in `pending_tool` was made.
    pub pending_tool_at: Option<i64>,
    /// The worktree Claude Code made for the session and holds, from the latest `worktree-state`
    /// line (`own_worktree`).
    pub own_worktree: Option<String>,
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
            // The file is named for its session; the first lines may be another's, copied.
            let own = file_name(path).trim_end_matches(".jsonl").to_string();
            if head.session_id != own {
                head.copied_from = Some(std::mem::replace(&mut head.session_id, own)).filter(|s| !s.is_empty());
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
        if let Some((at, cwd)) = prompt_before(path, tail.start) {
            info.last_prompt_at = Some(at);
            if let Some(cwd) = cwd {
                info.cwd = cwd;
            }
        }
    }
    state.offset = tail.end;
    if info.last_ts == 0 {
        info.last_ts = mtime;
    }
    Some((head, info, state))
}

/// When the last prompt you typed (or message you sent while it worked) before `end` (a line
/// start) was sent, and the folder it was sent in, reading backwards.
fn prompt_before(path: &Path, end: u64) -> Option<(i64, Option<String>)> {
    let mut at = None;
    scan_lines_back(path, 0, end, |line| {
        // Most lines are tool calls and their results; only a prompt, or a message you sent
        // while it worked, can pass these checks.
        let typed = line.contains("\"type\":\"user\"") && !line.contains("\"tool_use_id\"");
        if !typed && !line.contains("\"queued_command\"") {
            return false;
        }
        let Ok(v) = serde_json::from_str::<Value>(line) else { return false };
        if !you_typed(&v) && !you_sent_while_working(&v) {
            return false;
        }
        at = str_of(&v, "timestamp").as_deref().and_then(parse_rfc3339_ms).map(|t| (t, str_of(&v, "cwd").filter(|c| !c.is_empty())));
        at.is_some()
    });
    at
}

fn from_head(head: &HeadInfo) -> TranscriptInfo {
    TranscriptInfo {
        session_id: head.session_id.clone(),
        copied_from: head.copied_from.clone(),
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
        if let Some(hold) = super::held::hold_of(&v) {
            super::held::apply(&mut info.held, hold);
        }
        if let Some(step) = turn_step(&v) {
            info.unfinished = step;
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
            Some("worktree-state") => {
                info.own_worktree = own_worktree(&v);
            }
            Some("assistant") => {
                // A limit's refusal is written as a reply; any other reply means it went on.
                info.limit = limit_of(&v).map(|l| SessionLimit { model: info.model.clone(), ..l });
                info.connection_lost = connection_lost_at(&v);
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
                            let at = str_of(&v, "timestamp").as_deref().and_then(parse_rfc3339_ms).unwrap_or(0);
                            open_tools.push((id, tool_request(&name, &input), at));
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
                                open_tools.retain(|(open, _, _)| open != &id);
                            }
                        }
                    }
                }
                if user_prompt(&v).is_some() {
                    open_tools.clear();
                }
                // Written as the line's text, or its first text part.
                let content = v.pointer("/message/content");
                let said = content.and_then(Value::as_str).or_else(|| content?.as_array()?.iter().find_map(|i| i.get("text")?.as_str()));
                if let Some(m) = said.and_then(model_set) {
                    info.model = Some(m);
                }
                if you_typed(&v) {
                    info.limit = None;
                    info.connection_lost = None;
                    if let Some(ts) = str_of(&v, "timestamp").as_deref().and_then(parse_rfc3339_ms) {
                        info.last_prompt_at = Some(ts);
                    }
                    if let Some(cwd) = str_of(&v, "cwd").filter(|c| !c.is_empty()) {
                        info.cwd = cwd;
                    }
                }
            }
            // A message you sent while it worked is a new ask too. It's written once the turn
            // takes it in, timed when it came, so lines before it can be timed after it.
            Some("attachment") if you_sent_while_working(&v) => {
                if let Some(ts) = str_of(&v, "timestamp").as_deref().and_then(parse_rfc3339_ms) {
                    info.last_prompt_at = info.last_prompt_at.max(Some(ts));
                }
                if let Some(cwd) = str_of(&v, "cwd").filter(|c| !c.is_empty()) {
                    info.cwd = cwd;
                }
            }
            _ => {}
        }
    }
    info.pending_tool = open_tools.last().map(|(_, req, _)| req.clone());
    info.pending_tool_at = open_tools.last().map(|(_, _, at)| *at);
}

/// The model `/model` switched to, from what it wrote: "Set model to `haiku
/// (claude-haiku-4-5-20251001)`" for `/model haiku` (2.1.280), which switches the session alone;
/// "Set model to `Opus 5.5` and saved as your default for new sessions" from its picker; "Set
/// model to claude-opus-5" before 2.1.220. The model's ID where it's given, else its name.
fn model_set(content: &str) -> Option<String> {
    let rest = content.strip_prefix("<local-command-stdout>")?.strip_prefix("Set model to ")?;
    let said = match rest.strip_prefix('`') {
        Some(quoted) => quoted.split('`').next()?,
        None => rest.split(['<', ' ']).next()?,
    };
    let id = said.rsplit_once(" (").and_then(|(_, id)| id.strip_suffix(')')).unwrap_or(said);
    Some(id.trim().to_string()).filter(|m| !m.is_empty())
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

/// What Claude Code says as it ends a turn for want of a connection, after the text "API Error: ":
/// its words for a request that couldn't connect (ENOTFOUND and EAI_AGAIN, no route, a connection
/// reset, any other connection error) and for a reply cut off by the Mac sleeping or the connection
/// going (2.1.293's code). A refused connection and a proxy that won't tunnel are left out: they
/// don't come right by waiting.
const CONNECTION_LOST: &[&str] = &[
    "Can't reach the API server",
    "No internet route",
    "Connection dropped",
    "Unable to connect to API",
    "Your computer went to sleep",
    "Connection lost",
];

/// When an assistant line says the turn ended for want of a connection: Claude Code writes the
/// error as a reply, `isApiErrorMessage` with `error: "server_error"`, no `apiErrorStatus` and
/// `message.model` `"<synthetic>"`, its text one of `CONNECTION_LOST` ("API Error: Can't reach the
/// API server — check your internet or DNS (ENOTFOUND)"). It retries ten times first, half a
/// second apart and doubling up to 32 seconds, which only counts time the Mac is awake; but with
/// the lid closed the Mac wakes for a few seconds every minute or two without DNS, and the
/// retries run out in those (seen on this Mac on 2026-10-07). Only a reply the sleep itself cut
/// off is retried without counting. Nothing else is written, and the session waits for a prompt.
pub(super) fn connection_lost_at(v: &Value) -> Option<i64> {
    if v.get("isApiErrorMessage").and_then(Value::as_bool) != Some(true) || v.get("apiErrorStatus").is_some_and(|s| !s.is_null()) {
        return None;
    }
    let text = v.pointer("/message/content")?.as_array()?.iter().find_map(|i| i.get("text")?.as_str())?;
    let said = text.strip_prefix("API Error: ")?;
    CONNECTION_LOST.iter().any(|w| said.starts_with(w)).then(|| str_of(v, "timestamp").as_deref().and_then(parse_rfc3339_ms).unwrap_or(0))
}

/// What a line says about whether the turn it's in has ended (`TranscriptInfo::unfinished`):
/// `Some(Some(at))` for a step that waits on another, `Some(None)` for one that ends the turn or
/// starts a new one, None for a line that says neither.
///
/// A step waits on another when it's a tool call, its line written with `stop_reason:
/// "tool_use"`, or a tool's result, which the agent answers next. A turn ends with a reply
/// written with any other `stop_reason` (`end_turn`, `stop_sequence`), an API error, the
/// `turn_duration` line the terminal writes as each turn ends, or Escape's "[Request interrupted
/// by user" (after the refused call's result); a prompt you typed starts a new one.
///
/// A process killed partway through a turn (a hangup, as its tab closing gives) writes none of
/// those: its transcript ends with the call and the result Claude Code gives the command it
/// killed ("Exit code 137"), then the metadata it writes as it exits (`last-prompt`, `ai-title`,
/// `agent-name`, `mode`). Checked in tmux on 2.1.296, killed in a foreground command and stopped
/// with Escape there (2026-10-12). A prompt typed and killed before any reply isn't caught, nor
/// is one Escape takes back before the agent wrote anything, which writes nothing either.
fn turn_step(v: &Value) -> Option<Option<i64>> {
    if v.get("isSidechain").and_then(Value::as_bool) == Some(true) {
        return None;
    }
    let at = || str_of(v, "timestamp").as_deref().and_then(parse_rfc3339_ms);
    match v.get("type").and_then(Value::as_str)? {
        "assistant" if v.get("isApiErrorMessage").and_then(Value::as_bool) == Some(true) => Some(None),
        // A line still being written has no stop_reason yet, and says nothing.
        "assistant" => match v.pointer("/message/stop_reason").and_then(Value::as_str)? {
            "tool_use" => at().map(Some),
            _ => Some(None),
        },
        "system" if str_of(v, "subtype").as_deref() == Some("turn_duration") => Some(None),
        "user" => {
            let content = v.pointer("/message/content");
            let items = content.and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default();
            let mut texts = content.and_then(Value::as_str).into_iter().chain(items.iter().filter_map(|i| i.get("text")?.as_str()));
            if texts.any(|t| t.starts_with("[Request interrupted")) {
                Some(None)
            } else if items.iter().any(|i| i.get("type").and_then(Value::as_str) == Some("tool_result")) {
                at().map(Some)
            } else if you_typed(v) {
                Some(None)
            } else {
                None
            }
        }
        _ => None,
    }
}

/// The worktree a `worktree-state` line says the session made and holds. Claude Code writes one
/// as it makes the worktree (`--worktree`, `EnterWorktree`) or resumes in it, again with its
/// metadata as the transcript grows ("last-wins"), and `worktreeSession: null` once it has left
/// the worktree (2.1.292's code). The line is what `/exit` goes by: a worktree it made goes
/// as the session exits, at once when nothing in it changed and no commit was added since it
/// was made and the session has no name of yours, else after asking "Exiting worktree session"
/// whether to keep it; a `WorktreeCreate` hook's worktree goes through the `WorktreeRemove`
/// hook. A worktree it only went into (`enteredExisting`, `EnterWorktree` with a path) is left
/// in place, so it isn't the session's own. A hangup, as a tab closing gives, removes nothing
/// and runs no hook: only `SessionEnd`.
fn own_worktree(v: &Value) -> Option<String> {
    let w = v.get("worktreeSession")?;
    if w.get("enteredExisting").and_then(Value::as_bool) == Some(true) {
        return None;
    }
    str_of(w, "worktreePath").filter(|p| !p.is_empty())
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
    if text.trim_start().starts_with("Caveat:") {
        return None;
    }
    prompt_words(&text).map(|t| one_line(&t, 140))
}

/// How Claude Code wraps text pasted into a prompt (2.1.28x): the tag, the text on lines of its
/// own, then a closing tag that names the same id. A long message Codealong sends a terminal
/// session is pasted, so it comes back as one: `\n\n<pasted_content id="3c58">\n…\n</pasted_content
/// id="3c58">\n`, with nothing typed around it (2026-10-05).
const PASTE_OPEN: &str = "<pasted_content id=\"";
const PASTE_CLOSE: &str = "</pasted_content id=\"";

/// What a prompt says, or None for a line of Claude Code's own rather than words of yours: empty,
/// tagged (a command's output, a notification) or an interruption. A paste is yours, so its tags
/// come off instead, and what was pasted stays where it was, even when it starts with a tag.
pub(super) fn prompt_words(text: &str) -> Option<String> {
    let t = text.trim();
    if t.is_empty() || (t.starts_with('<') && !t.starts_with(PASTE_OPEN)) || t.starts_with("[Request interrupted") {
        return None;
    }
    let words = unwrap_pastes(t);
    let words = words.trim();
    (!words.is_empty()).then(|| words.to_string())
}

/// A prompt's text with Claude Code's paste tags taken off, with the line break each puts
/// between itself and what was pasted. A tag that isn't closed on its own line is left alone.
fn unwrap_pastes(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    loop {
        let next = [PASTE_OPEN, PASTE_CLOSE].into_iter().filter_map(|tag| rest.find(tag).map(|at| (at, tag))).min();
        let Some((at, tag)) = next else { break };
        let id_from = at + tag.len();
        let Some(end) = rest[id_from..].find("\">").map(|i| id_from + i + 2).filter(|&end| !rest[id_from..end].contains('\n')) else { break };
        let (mut before, mut after) = (&rest[..at], &rest[end..]);
        if tag == PASTE_OPEN {
            after = after.strip_prefix('\n').unwrap_or(after);
        } else {
            before = before.strip_suffix('\n').unwrap_or(before);
        }
        out.push_str(before);
        rest = after;
    }
    out.push_str(rest);
    out
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

/// A message you sent while the session worked, as its turn took it in: Claude Code writes that
/// as a `queued_command` attachment rather than a `user` line, marked as a prompt is (`human`,
/// `peer`, a background task's notification otherwise). Older ones have no mark, and a
/// notification is told by its tag.
pub(super) fn you_sent_while_working(v: &Value) -> bool {
    if v.get("type").and_then(Value::as_str) != Some("attachment") || v.pointer("/attachment/type").and_then(Value::as_str) != Some("queued_command") || v.get("isSidechain").and_then(Value::as_bool) == Some(true) {
        return false;
    }
    if let Some(peer) = line_peer(v) {
        return sent_from_codealong(&peer).is_some();
    }
    match v.pointer("/attachment/origin/kind").and_then(Value::as_str) {
        Some(kind) => kind == "human",
        None => prompt_words(&crate::transcript::text_of(v.pointer("/attachment/prompt").unwrap_or(&Value::Null))).is_some(),
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
    prompt_words(&text)
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

/// The PR a transcript line says the session made or changed, by its URL, with the line's time,
/// from one of two marks: a command's result that acted on a PR
/// (`toolUseResult.gitOperation.pr`: `number`, `url`, `action` created, edited, commented or
/// closed; written for `gh pr create` and the like, and a closed one has no URL), or Claude
/// Code's own `pr-link` line (`prNumber`, `prUrl`, `prRepository`), which it writes once a
/// command created a PR or pushed to one, however the command did it (`gh api repos/…/pulls`, a
/// script), and again with every re-stamp of its metadata (2.1.289's `linkSessionToPR`; a `gh pr
/// list` or `gh pr view` writes none). Only lines with one of the marks are parsed.
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
    fn reads_the_model_model_switched_to() {
        let said = |text: &str| model_set(&format!("<local-command-stdout>{text}</local-command-stdout>"));
        assert_eq!(said("Set model to `haiku (claude-haiku-4-5-20251001)`").as_deref(), Some("claude-haiku-4-5-20251001"));
        assert_eq!(said("Set model to `opus[1m] (claude-opus-5-5[1m])`").as_deref(), Some("claude-opus-5-5[1m]"));
        assert_eq!(said("Set model to `Opus 5.5` and saved as your default for new sessions").as_deref(), Some("Opus 5.5"));
        assert_eq!(said("Set model to claude-opus-5").as_deref(), Some("claude-opus-5"));
        assert_eq!(said("Kept model as Opus"), None);
        // Until the next reply, the switch is the model it's on; the reply says it after.
        let line = |kind: &str, body: &str| format!(r#"{{"type":"{kind}","sessionId":"s","message":{body}}}"#);
        let tail = [
            line("assistant", r#"{"model":"claude-opus-5-5","content":[{"type":"text","text":"Done."}]}"#),
            line("user", r#"{"role":"user","content":"<local-command-stdout>Set model to `haiku (claude-haiku-4-5-20251001)`</local-command-stdout>"}"#),
        ];
        assert_eq!(parse_lines("", &tail.join("\n")).model.as_deref(), Some("claude-haiku-4-5-20251001"));
        let replied = [tail[1].clone(), line("assistant", r#"{"model":"claude-haiku-4-5-20251001","content":[{"type":"text","text":"Hi."}]}"#)];
        assert_eq!(parse_lines("", &replied.join("\n")).model.as_deref(), Some("claude-haiku-4-5-20251001"));
    }

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
    fn reads_the_worktree_it_made() {
        // As 2.1.292 writes them: the latest says, and null once it has left.
        let made = r#"{"type":"worktree-state","worktreeSession":{"originalCwd":"/r/app","worktreePath":"/r/app/.claude/worktrees/brave-otter","worktreeName":"brave-otter","worktreeBranch":"worktree-brave-otter","originalBranch":"main","originalHeadCommit":"1ec9744","sessionId":"s"},"sessionId":"s"}"#;
        let left = r#"{"type":"worktree-state","worktreeSession":null,"sessionId":"s"}"#;
        let entered = r#"{"type":"worktree-state","worktreeSession":{"originalCwd":"/r/app","worktreePath":"/r/app-fix","enteredExisting":true,"sessionId":"s"},"sessionId":"s"}"#;
        assert_eq!(parse_lines(HEAD, made).own_worktree.as_deref(), Some("/r/app/.claude/worktrees/brave-otter"));
        assert_eq!(parse_lines(HEAD, &[made, left].join("\n")).own_worktree, None);
        assert_eq!(parse_lines(HEAD, &[left, made].join("\n")).own_worktree.as_deref(), Some("/r/app/.claude/worktrees/brave-otter"));
        // One it only went into stays where it is when the session exits.
        assert_eq!(parse_lines(HEAD, &[made, entered].join("\n")).own_worktree, None);
        assert_eq!(parse_lines(HEAD, "").own_worktree, None);
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
    fn follows_a_session_resumed_in_another_folder() {
        // Resumed elsewhere, it goes on in the same transcript, and its prompts say where.
        let head = r#"{"type":"user","cwd":"/r/chats/2026-10-05-a1b2c3","sessionId":"s","timestamp":"2026-10-05T10:00:00.000Z","message":{"content":"How do I parse dates?"}}
{"type":"assistant","cwd":"/r/chats/2026-10-05-a1b2c3","sessionId":"s","message":{"content":[{"type":"text","text":"Like this."}]}}
"#;
        let moved = r#"{"type":"user","cwd":"/r/app","sessionId":"s","origin":{"kind":"human"},"timestamp":"2026-10-05T11:00:00.000Z","message":{"content":"Use it here"}}
"#;
        assert_eq!(parse_lines(head, head).cwd, "/r/chats/2026-10-05-a1b2c3");
        assert_eq!(parse_lines(head, &format!("{head}{moved}")).cwd, "/r/app");

        // A first read whose tail is all one long turn looks back for the prompt that began it.
        let dir = crate::fixtures::scratch("resumed-elsewhere");
        let path = dir.join("s.jsonl");
        let long = format!("{{\"type\":\"assistant\",\"cwd\":\"/r/app/src\",\"message\":{{\"content\":[{{\"type\":\"text\",\"text\":\"{}\"}}]}}}}\n", "x".repeat(300_000));
        std::fs::write(&path, format!("{head}{moved}{long}")).unwrap();
        let (head, info, _) = parse_transcript(&path, 1, None).unwrap();
        assert_eq!((head.cwd.as_str(), info.cwd.as_str()), ("/r/chats/2026-10-05-a1b2c3", "/r/app"));
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
        let path = dir.join("25b09e0e.jsonl");
        let big = format!("{{\"type\":\"attachment\",\"blob\":\"{}\"}}\n", "x".repeat(600_000));
        let text = format!("{big}{HEAD}");
        std::fs::write(&path, text).unwrap();
        let head = read_head_info(&path).unwrap();
        assert_eq!(head.cwd, "/Users/me/projects/sparkhq/spark-portal");
        assert_eq!(head.first_prompt.as_deref(), Some("Can you make a photo write tool?"));
        let (head, info, state) = parse_transcript(&path, 1, None).unwrap();
        assert_eq!((info.session_id.as_str(), info.copied_from.as_deref()), ("25b09e0e", None));
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
    fn a_fork_the_claude_app_made_is_its_own_session_and_names_the_one_it_copied() {
        // As 2.1.293 wrote a `btw`: the conversation it copied, with that session's ID on its
        // lines, then its own.
        let dir = crate::fixtures::scratch("app-fork");
        let path = dir.join("f5f4b31f.jsonl");
        let own = r#"{"type":"custom-title","customTitle":"btw: who keeps the drawings?","sessionId":"f5f4b31f"}
{"type":"user","cwd":"/Users/me/projects/sparkhq/spark-portal","sessionId":"f5f4b31f","origin":{"kind":"human"},"timestamp":"2026-09-23T16:00:00.000Z","message":{"role":"user","content":"who keeps the drawings?"}}
"#;
        std::fs::write(&path, format!("{HEAD}{own}")).unwrap();
        let (_, info, _) = parse_transcript(&path, 1, None).unwrap();
        assert_eq!((info.session_id.as_str(), info.copied_from.as_deref()), ("f5f4b31f", Some("25b09e0e")));
        assert_eq!(info.title.as_deref(), Some("btw: who keeps the drawings?"));
        assert_eq!(info.first_prompt.as_deref(), Some("Can you make a photo write tool?"), "the conversation it copied begins it");
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
    fn a_pasted_prompt_is_what_was_pasted() {
        let line = |text: &str| serde_json::json!({ "type": "user", "origin": { "kind": "human" }, "message": { "content": text } });
        // A long message Codealong sends a terminal session comes back wrapped whole, as this one did.
        let sent = "\n\n<pasted_content id=\"3c58\">\nI ran these mocks by Gordon.\n\n1) Simplify \"Applies to\".\n</pasted_content id=\"3c58\">\n";
        assert_eq!(typed_text(&line(sent)).as_deref(), Some("I ran these mocks by Gordon.\n\n1) Simplify \"Applies to\"."));
        assert_eq!(user_prompt(&line(sent)).as_deref(), Some("I ran these mocks by Gordon."));
        // Pasted around words typed, each stays where it was, a paste that starts with a tag too.
        let mixed = "<pasted_content id=\"a\">\nlog\n</pasted_content id=\"a\">\n\nwhy does this fail? <pasted_content id=\"b\">\n<html>\n</pasted_content id=\"b\">";
        assert_eq!(typed_text(&line(mixed)).as_deref(), Some("log\n\nwhy does this fail? <html>"));
        // Claude Code's own tagged lines are still not prompts, nor is an empty paste.
        assert_eq!(typed_text(&line("<command-name>/cost</command-name>")), None);
        assert_eq!(typed_text(&line("<pasted_content id=\"c\">\n\n</pasted_content id=\"c\">")), None);
        // A tag that doesn't close on its own line is left as it is.
        assert_eq!(prompt_words("<pasted_content id=\"d\nnot a tag").as_deref(), Some("<pasted_content id=\"d\nnot a tag"));
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
    fn a_message_you_sent_while_it_worked_is_your_last_prompt() {
        // Written once the turn takes it in, after a reply timed later than it (2.1.295).
        let queued = |at: &str, prompt: &str, origin: Value| serde_json::json!({ "type": "attachment", "timestamp": at, "cwd": "/w/wt", "attachment": { "type": "queued_command", "prompt": prompt, "origin": origin } }).to_string();
        let tail = [
            r#"{"type":"user","timestamp":"2026-10-09T16:56:56.560Z","cwd":"/w","origin":{"kind":"human"},"message":{"content":"Change the skill"}}"#.to_string(),
            r#"{"type":"assistant","timestamp":"2026-10-09T16:57:30.134Z","message":{"content":[{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"yarn test"}}]}}"#.to_string(),
            queued("2026-10-09T16:57:21.398Z", "Also assign Wes", serde_json::json!({ "kind": "human" })),
            queued("2026-10-09T16:57:26.574Z", "Did we apply it?", serde_json::json!({ "kind": "human" })),
        ];
        let info = parse_lines(HEAD, &(tail.join("\n") + "\n"));
        assert_eq!((info.last_prompt_at, info.cwd.as_str()), (parse_rfc3339_ms("2026-10-09T16:57:26.574Z"), "/w/wt"));
        // Yours from Codealong counts; a background task's notification, a subagent's report and
        // another agent's message don't.
        let others = [
            tail[0].clone(),
            queued("2026-10-09T16:58:00.000Z", "<task-notification>\n<task-id>b1</task-id>\n</task-notification>", serde_json::json!({ "kind": "task-notification" })),
            queued("2026-10-09T16:59:00.000Z", "<agent-message from=\"a3c7\">\nDone.\n</agent-message>", serde_json::json!({ "kind": "peer", "from": "a3c7", "handback": true, "body": "Done." })),
            queued("2026-10-09T17:00:00.000Z", "hi", serde_json::json!({ "kind": "peer", "from": "unknown", "name": "review", "body": "This message is from another agent, “review”.\n\nhi" })),
        ];
        assert_eq!(parse_lines(HEAD, &(others.join("\n") + "\n")).last_prompt_at, parse_rfc3339_ms("2026-10-09T16:56:56.560Z"));
        let codealong = queued("2026-10-09T17:01:00.000Z", "Now the docs", serde_json::json!({ "kind": "peer", "from": "unknown", "name": "Codealong", "body": format!("{FROM_YOU}\n\nNow the docs") }));
        assert_eq!(parse_lines(HEAD, &(codealong + "\n")).last_prompt_at, parse_rfc3339_ms("2026-10-09T17:01:00.000Z"));
        // Older ones have no mark, and a notification is told by its tag.
        let old = |prompt: &str| serde_json::json!({ "type": "attachment", "timestamp": "2026-10-09T17:02:00.000Z", "attachment": { "type": "queued_command", "prompt": prompt } }).to_string() + "\n";
        assert_eq!(parse_lines(HEAD, &old("Also check the docs")).last_prompt_at, parse_rfc3339_ms("2026-10-09T17:02:00.000Z"));
        assert_eq!(parse_lines(HEAD, &old("<task-notification>\n<status>completed</status>\n</task-notification>")).last_prompt_at, None);
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
        // A message you sent while it worked, as far back, is the later one.
        let sent = r#"{"type":"attachment","timestamp":"2026-09-23T15:02:00.000Z","attachment":{"type":"queued_command","prompt":"Keep the old API too","origin":{"kind":"human"}}}"#;
        let mut lines = vec![HEAD.trim_end().to_string(), prompt.to_string(), output(0), sent.to_string()];
        lines.extend((1..12).map(output));
        std::fs::write(&path, lines.join("\n") + "\n").unwrap();
        let (_, info, _) = parse_transcript(&path, 1, None).unwrap();
        assert_eq!(info.last_prompt_at, parse_rfc3339_ms("2026-09-23T15:02:00.000Z"));
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

    /// A turn that ended for want of a connection, as Claude Code 2.1.293 wrote it with the lid
    /// closed, cut to the fields that matter here.
    fn lost(text: &str) -> String {
        serde_json::json!({ "type": "assistant", "timestamp": "2026-10-07T23:48:20.610Z", "error": "server_error", "isApiErrorMessage": true, "message": { "model": "<synthetic>", "content": [{ "type": "text", "text": text }] } }).to_string()
    }

    #[test]
    fn a_turn_a_lost_connection_ended_says_when_until_something_comes_after() {
        let at = parse_rfc3339_ms("2026-10-07T23:48:20.610Z");
        for text in [
            "API Error: Can't reach the API server — check your internet or DNS (ENOTFOUND)",
            "API Error: Your computer went to sleep mid-response. The response above may be incomplete.",
            "API Error: Connection lost before a response was produced. Try again.",
            "API Error: No internet route — check your connection or VPN (ENETUNREACH)",
        ] {
            assert_eq!(parse_lines(HEAD, &format!("{}\n", lost(text))).connection_lost, at, "{text}");
        }
        // Errors that waiting doesn't mend, and a limit, aren't one.
        for text in ["API Error: Connection refused — a firewall or proxy may be blocking it (ECONNREFUSED)", "API Error: 529 Overloaded. This is a server-side issue, usually temporary."] {
            assert_eq!(parse_lines(HEAD, &format!("{}\n", lost(text))).connection_lost, None, "{text}");
        }
        assert_eq!(parse_lines(HEAD, &format!("{SPEND_CAPPED}\n")).connection_lost, None);
        // An agent's own words about it are no error of Claude Code's.
        let said = r#"{"type":"assistant","timestamp":"2026-10-07T23:50:00.000Z","message":{"model":"claude-opus-5-5","content":[{"type":"text","text":"API Error: Can't reach the API server is what it printed."}]}}"#;
        assert_eq!(parse_lines(HEAD, &format!("{said}\n")).connection_lost, None);
        // A task's notice leaves it, the turn it starts having said nothing yet; a prompt of yours
        // or a reply ends it.
        let error = lost("API Error: Can't reach the API server — check your internet or DNS (ENOTFOUND)");
        let notice = r#"{"type":"user","timestamp":"2026-10-07T23:50:00.567Z","origin":{"kind":"task-notification"},"message":{"content":"<task-notification>done</task-notification>"}}"#;
        assert_eq!(parse_lines(HEAD, &format!("{error}\n{notice}\n")).connection_lost, at);
        let prompt = r#"{"type":"user","timestamp":"2026-10-08T00:23:42.747Z","origin":{"kind":"human"},"message":{"content":"continue"}}"#;
        assert_eq!(parse_lines(HEAD, &format!("{error}\n{prompt}\n")).connection_lost, None);
        let reply = r#"{"type":"assistant","timestamp":"2026-10-08T00:23:46.497Z","message":{"model":"claude-opus-5-5","content":[{"type":"text","text":"Going on."}]}}"#;
        assert_eq!(parse_lines(HEAD, &format!("{error}\n{notice}\n{reply}\n")).connection_lost, None);
    }

    /// A turn killed in a foreground command and one stopped with Escape there, as Claude Code
    /// 2.1.296 wrote them in tmux, cut to the fields that matter here.
    const CALL: &str = r#"{"type":"assistant","timestamp":"2026-10-12T18:21:20.000Z","message":{"model":"claude-haiku-5-5","stop_reason":"tool_use","content":[{"type":"tool_use","id":"toolu_1","name":"Bash","input":{"command":"python3 -c 'import time; time.sleep(120)'"}}]}}"#;
    const KILLED: &str = r#"{"type":"user","timestamp":"2026-10-12T18:23:22.000Z","message":{"role":"user","content":[{"type":"tool_result","content":"Exit code 137","is_error":true,"tool_use_id":"toolu_1"}]}}
{"type":"last-prompt","lastPrompt":"Run exactly this"}
{"type":"ai-title","aiTitle":"Run a long command"}"#;

    #[test]
    fn a_turn_says_when_it_last_did_something_until_it_ends() {
        let called = parse_rfc3339_ms("2026-10-12T18:21:20.000Z");
        let killed = parse_rfc3339_ms("2026-10-12T18:23:22.000Z");
        // The prompt that opened the turn is HEAD's; nothing has happened in it yet.
        assert_eq!(parse_lines(HEAD, "").unfinished, None);
        assert_eq!(parse_lines(HEAD, &format!("{CALL}\n")).unfinished, called);
        assert_eq!(parse_lines(HEAD, &format!("{CALL}\n{KILLED}\n")).unfinished, killed);
        // Ended by a reply, whatever stopped it, by the terminal's own mark, or by an API error.
        for end in [
            r#"{"type":"assistant","timestamp":"2026-10-12T18:23:30.000Z","message":{"stop_reason":"end_turn","content":[{"type":"text","text":"It printed 42."}]}}"#,
            r#"{"type":"assistant","timestamp":"2026-10-12T18:23:30.000Z","message":{"stop_reason":"stop_sequence","content":[{"type":"text","text":"Done."}]}}"#,
            r#"{"type":"system","subtype":"turn_duration","timestamp":"2026-10-12T18:23:31.000Z"}"#,
            &lost("API Error: Can't reach the API server — check your internet or DNS (ENOTFOUND)"),
        ] {
            assert_eq!(parse_lines(HEAD, &format!("{CALL}\n{KILLED}\n{end}\n")).unfinished, None, "{end}");
        }
        // Stopped with Escape: the refused call's result, then the interruption.
        let escaped = r#"{"type":"user","timestamp":"2026-10-12T18:24:03.000Z","message":{"role":"user","content":[{"type":"tool_result","content":"The user doesn't want to proceed with this tool use.","is_error":true,"tool_use_id":"toolu_1"}]}}
{"type":"user","timestamp":"2026-10-12T18:24:03.000Z","message":{"role":"user","content":[{"type":"text","text":"[Request interrupted by user for tool use]"}]}}"#;
        assert_eq!(parse_lines(HEAD, &format!("{CALL}\n{escaped}\n")).unfinished, None);
        // A prompt of yours starts a new turn; a line still being written, and a subagent's, say nothing.
        let prompt = r#"{"type":"user","timestamp":"2026-10-12T18:30:00.000Z","origin":{"kind":"human"},"message":{"content":"Try again"}}"#;
        assert_eq!(parse_lines(HEAD, &format!("{CALL}\n{KILLED}\n{prompt}\n")).unfinished, None);
        let writing = r#"{"type":"assistant","timestamp":"2026-10-12T18:23:40.000Z","message":{"stop_reason":null,"content":[{"type":"text","text":"It"}]}}"#;
        let helper = r#"{"type":"assistant","isSidechain":true,"timestamp":"2026-10-12T18:23:40.000Z","message":{"stop_reason":"end_turn","content":[{"type":"text","text":"Found it."}]}}"#;
        assert_eq!(parse_lines(HEAD, &format!("{CALL}\n{KILLED}\n{writing}\n{helper}\n")).unfinished, killed);
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
