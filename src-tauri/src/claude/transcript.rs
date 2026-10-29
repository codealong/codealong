//! Claude Code's transcripts (`<home>/projects/<encoded cwd>/<session>.jsonl`): what the start
//! of one says of its session, what its end says the session is doing now, which lines are
//! prompts you typed, and how a tool call reads.

use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::conversation::{line_peer, sent_from_codealong};
use crate::model::{QueuedMessage, SessionLimit};
use crate::util::{code, command_dir, file_name, mtime_ms, note_work, one_line, parse_rfc3339_ms, read_head, read_tail, resolve, scan_lines_back, without_cd, work_before};

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
    /// When the latest reply, message or turn's end was written (`said`).
    pub said_at: Option<i64>,
    /// What replaying Claude Code's queue carries from line to line (`hold_line`).
    pub holding: Holding,
}

/// What replaying what Claude Code holds carries from line to line (`hold_line`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Holding {
    /// The latest unmarked command line, as typed (`/goal …`), until the line after it says
    /// whether it took a turn (`ran_a_turn`).
    command: Option<String>,
    /// What the `dequeue`s since the last turn's prompt were put down to (`turn_took`).
    dequeued: Vec<String>,
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
    /// The color `/color` gave the session, from `agent-color` lines (`{"type":"agent-color",
    /// "agentColor":"teal","sessionId":…}`), as Claude Code names it: red, blue, green, yellow,
    /// purple, orange, pink or cyan. `/color default` writes `default`, which takes it away.
    /// Re-stamped with the session's other metadata, like `agent-name` (2.1.296's code).
    pub color: Option<String>,
    pub first_prompt: Option<String>,
    pub last_prompt: Option<String>,
    /// When you last sent a prompt. It doesn't move while the session works, so lists sort by it.
    pub last_prompt_at: Option<i64>,
    pub started_at: Option<i64>,
    pub last_ts: i64,
    /// When its last turn ended, while a compaction has come since with no reply or message
    /// after it. Claude Code compacts a long conversation left idle for about 54 minutes, before
    /// its prompt cache expires (from 2.1.296, behind a remote flag; its notice says "Compacted
    /// while idle"), and the registry goes busy and idle again around it, though no turn was
    /// taken. The lines it writes, and the links it stamps again as it starts, are timed then.
    pub compacted: Option<i64>,
    /// When its last turn ended, while nothing has been said since (`said`, `ends_turn`). A turn
    /// starts by saying something, its prompt, another session's message or a background task's
    /// report (all but 2 of 4,026 turns that came after one ended here, 2026-10-22), so a session
    /// that goes busy with this set hasn't started one: Claude Code compacting it (`compacted`)
    /// writes nothing until the boundary, and only metadata and the links it stamps again before.
    pub ended_at: Option<i64>,
    /// The tool call waiting on a result, e.g. "Bash · `yarn test`".
    pub pending_tool: Option<String>,
    /// The latest tool call as an activity, e.g. "editing `App.tsx`", since the latest prompt
    /// you typed: one from before that is the last turn's, long finished. A message sent while
    /// it worked goes into the turn it's taking, whose calls it leaves.
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
    /// A provider refusal anywhere in the conversation, including before the current tail.
    pub flagged: bool,
    /// When the last turn last did something, while it hasn't ended (`turn_step`). A session
    /// whose process is gone with one was cut off partway through it.
    pub unfinished: Option<i64>,
    /// The model the latest reply came from.
    pub model: Option<String>,
    /// The effort level the latest reply was made at, or one `/effort` set since.
    pub effort: Option<String>,
    /// How many tokens of context the latest reply's call sent: its input, cache reads and cache
    /// writes, as Claude Code's own context meter counts them (2.1.296's code: its status line's
    /// `context_window.total_input_tokens`, `used_percentage` of the model's window). A compaction
    /// since then says what's left (`compact_boundary`'s `compactMetadata.postTokens`).
    pub context: Option<u64>,
    /// The permission mode it runs in, as Claude Code writes it (`acceptEdits`): from the
    /// `permission-mode` line it re-stamps with its other metadata, and the mode each prompt went
    /// in. A mode switched to sets the stamp without writing it, so it shows from the next prompt
    /// or re-stamp (2.1.296's code).
    pub permission_mode: Option<String>,
    /// The model whose window that fills: `model`, or one `/model` switched to with a context
    /// size (`claude-sonnet-4-5[1m]`) while replies name its model without it.
    pub window_model: Option<String>,
    /// A message from another session it holds for you to allow (held.rs), until a line says
    /// it went.
    pub held: Option<super::held::Held>,
    /// When the tool call in `pending_tool` was made.
    pub pending_tool_at: Option<i64>,
    /// The worktree Claude Code made for the session and holds, from the latest `worktree-state`
    /// line (`own_worktree`).
    pub own_worktree: Option<String>,
    /// What Claude Code holds for the turn to take in, whoever sent it, in the order it came
    /// (`queue_step`). Only a running turn holds anything: a process gone with some left is read
    /// past (board.rs `claude_session`).
    pub queue: Vec<QueuedMessage>,
    /// The shells it started in the background (`shell_task`) that it hasn't been told have
    /// ended, with when each started, oldest first: each wakes it as it ends. Its registry says
    /// so only from a terminal (board.rs `untold_shell`).
    pub background: Vec<(String, i64)>,
}

impl TranscriptInfo {
    /// What it holds of yours (`queue`): words typed or sent, not a background task's
    /// notification or another session's message, which the conversation shows as theirs once
    /// the turn takes it in. Each says what the conversation will say it said (`prompt_words`).
    pub fn held(&self) -> Vec<QueuedMessage> {
        self.queue.iter().filter_map(|m| Some(QueuedMessage { text: prompt_words(&m.text)?, ..m.clone() })).collect()
    }

    /// When it last did something: its last line, or the end of its last turn while a compaction
    /// is all that came since (`compacted`).
    pub fn active_at(&self) -> i64 {
        self.compacted.unwrap_or(self.last_ts)
    }
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
    // A large append rebuilds the tail's status, but flags in the already-read prefix are known.
    let (was_flagged, flag_from) = prev.as_ref().filter(|(_, _, st)| st.offset <= len).map(|(_, info, st)| (info.flagged, st.offset)).unwrap_or_default();
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
            let mut info = from_head(&head);
            // The tail reads on from what was said before it, and a compaction's own lines can
            // run past it (up to 450 KB of them here).
            let (said_at, ended_at, compacted) = said_before(path, tail.start);
            info.compacted = said_at.filter(|_| compacted);
            info.ended_at = ended_at;
            (info.queue, info.background) = held_before(path, tail.start);
            (head, info, TailState { said_at, ..TailState::default() }, tail)
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
    // A `/model` switch to a context size can be further back too, while replies name the model
    // without it. Looked for only where the size would change the window: a model whose own
    // window is under a million.
    if fresh {
        if let Some(model) = info.window_model.clone().filter(|m| !m.contains('[') && crate::prices::context_window(m).is_some_and(|w| w < 1_000_000)) {
            if let Some(switched) = model_switch_before(path, tail.start).filter(|s| s.split('[').next() == Some(model.as_str())) {
                info.window_model = Some(switched);
            }
        }
    }
    // Where it worked decides its worktree (`board::session_seed`). Later reads carry it forward.
    if fresh {
        work_before(path, 0, tail.start, &mut info.work, |line| line_work(line, &head.cwd));
    }
    info.flagged |= was_flagged;
    if fresh && !info.flagged {
        info.flagged = crate::providerflags::before(path, flag_from, tail.start, &["\"refusal\"", "\"dlp_request_denied\"", "\"safety_monitor_blocked\""], flagged);
    }
    state.offset = tail.end;
    if info.last_ts == 0 {
        info.last_ts = mtime;
    }
    Some((head, info, state))
}

/// The model the latest `/model` switch before `end` named, context size and all.
fn model_switch_before(path: &Path, end: u64) -> Option<String> {
    let mut found = None;
    scan_lines_back(path, 0, end, |line| {
        if !line.contains("Set model to") || !line.contains("\"type\":\"user\"") {
            return false;
        }
        let Ok(v) = serde_json::from_str::<Value>(line) else { return false };
        let content = v.pointer("/message/content");
        let said = content.and_then(Value::as_str).or_else(|| content?.as_array()?.iter().find_map(|i| i.get("text")?.as_str()));
        found = said.and_then(model_set);
        found.is_some()
    });
    found
}

/// Where a reply line's calls worked (`work_path`), in order. `cwd` is the folder for a line that
/// names none.
fn line_work(line: &str, cwd: &str) -> Vec<String> {
    // Most bytes are results and replies; only a call can be work.
    if !line.contains("\"type\":\"tool_use\"") {
        return Vec::new();
    }
    let Ok(v) = serde_json::from_str::<Value>(line) else { return Vec::new() };
    if v.get("type").and_then(Value::as_str) != Some("assistant") {
        return Vec::new();
    }
    let at = str_of(&v, "cwd").unwrap_or_else(|| cwd.to_string());
    let items = v.pointer("/message/content").and_then(Value::as_array).into_iter().flatten();
    let calls = items.filter(|i| i.get("type").and_then(Value::as_str) == Some("tool_use"));
    calls.filter_map(|i| work_path(&str_of(i, "name").unwrap_or_default(), i.get("input").unwrap_or(&Value::Null), &at)).collect()
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

/// When the latest reply, message or turn's end before `end` (a line start) was written, reading
/// backwards, the same when it ended a turn (`TranscriptInfo::ended_at`), and whether a
/// compaction's boundary came after it.
fn said_before(path: &Path, end: u64) -> (Option<i64>, Option<i64>, bool) {
    let mut at = None;
    let mut ended = false;
    let mut compacted = false;
    scan_lines_back(path, 0, end, |line| {
        // Only these can be either.
        if !["\"type\":\"assistant\"", "\"type\":\"user\"", "\"turn_duration\"", "\"compact_boundary\""].iter().any(|s| line.contains(s)) {
            return false;
        }
        let Ok(v) = serde_json::from_str::<Value>(line) else { return false };
        compacted |= compaction_step(&v) == Some(true);
        at = str_of(&v, "timestamp").as_deref().and_then(parse_rfc3339_ms).filter(|_| said(&v));
        ended = ends_turn(&v);
        at.is_some()
    });
    (at, at.filter(|_| ended), compacted)
}

/// Whether a line is something the conversation said: a reply, a message or a turn's end, as
/// against what Claude Code writes around them (titles, links stamped again, a recap of the turn
/// while you're away, a compaction).
fn said(v: &Value) -> bool {
    compaction_step(v) == Some(false) || v.get("type").and_then(Value::as_str) == Some("system") && str_of(v, "subtype").as_deref() == Some("turn_duration")
}

/// What a line says of a compaction (`TranscriptInfo::compacted`): Some(true) for its boundary,
/// Some(false) for a reply or a message, which come only once it's over, None for the rest. After
/// the boundary come the summary (a `user` line with `isCompactSummary`), the files, listings and
/// hook output it attaches again, and with an idle one a `system` / `informational` notice.
fn compaction_step(v: &Value) -> Option<bool> {
    if v.get("isSidechain").and_then(Value::as_bool) == Some(true) {
        return None;
    }
    match v.get("type").and_then(Value::as_str)? {
        "system" => (str_of(v, "subtype").as_deref() == Some("compact_boundary")).then_some(true),
        "assistant" => Some(false),
        "user" => (v.get("isCompactSummary").and_then(Value::as_bool) != Some(true)).then_some(false),
        _ => None,
    }
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

/// Claude records classifier refusals as assistant messages with `stop_reason: "refusal"`,
/// including its synthetic "safeguards flagged this session" API error (2.1.303). A quoted
/// warning, tool result or a subagent's old inline reply isn't this session being flagged.
fn flagged(v: &Value) -> bool {
    v.get("type").and_then(Value::as_str) == Some("assistant")
        && v.get("isSidechain").and_then(Value::as_bool) != Some(true)
        && (v.pointer("/message/stop_reason").and_then(Value::as_str) == Some("refusal")
            || matches!(v.get("apiError").and_then(Value::as_str), Some("dlp_request_denied" | "safety_monitor_blocked")))
}

/// Titles, the last prompt, activity and pending tool calls from transcript lines.
pub fn apply_tail(info: &mut TranscriptInfo, state: &mut TailState, tail: &str) {
    let open_tools = &mut state.open_tools;
    for line in tail.lines() {
        let Ok(v) = serde_json::from_str::<Value>(line) else { continue };
        info.flagged |= flagged(&v);
        let kind = v.get("type").and_then(Value::as_str);
        let ts = str_of(&v, "timestamp").as_deref().and_then(parse_rfc3339_ms);
        match compaction_step(&v) {
            Some(true) => info.compacted = state.said_at,
            Some(false) => info.compacted = None,
            None => {}
        }
        if said(&v) {
            state.said_at = state.said_at.max(ts);
            info.ended_at = ts.filter(|_| ends_turn(&v));
        }
        // Moving the conversation to the background isn't something it did.
        if let Some(ts) = ts.filter(|_| kind != Some("continued-in")) {
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
        background_step(&mut info.background, &v, ts);
        let typed = hold_line(&mut info.queue, &mut state.holding, &v);
        // A subagent's prompt, in transcripts from before their own logs, went in its mode.
        if matches!(kind, Some("permission-mode") | Some("user")) && v.get("isSidechain").and_then(Value::as_bool) != Some(true) {
            if let Some(mode) = str_of(&v, "permissionMode").filter(|m| !m.is_empty()) {
                info.permission_mode = Some(mode);
            }
        }
        match kind {
            Some("continued-in") => {
                info.continued_in = str_of(&v, "continuedInSessionId").filter(|id| !id.is_empty());
            }
            // A compaction leaves the context its summary: what's left until the next reply says.
            Some("system") if v.get("subtype").and_then(Value::as_str) == Some("compact_boundary") => {
                info.context = v.pointer("/compactMetadata/postTokens").and_then(Value::as_u64).filter(|t| *t > 0);
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
            Some("agent-color") => {
                info.color = str_of(&v, "agentColor").map(|c| c.trim().to_lowercase()).filter(|c| !c.is_empty() && c != "default");
            }
            Some("agent-name") => {
                if info.title.is_none() {
                    info.title = str_of(&v, "agentName").filter(|t| !t.trim().is_empty()).map(|t| one_line(&t, 90));
                }
            }
            Some("last-prompt") => {
                info.last_prompt = str_of(&v, "lastPrompt").map(|p| one_line(crate::send::past_sender_lines(&p), 140));
            }
            Some("worktree-state") => {
                info.own_worktree = own_worktree(&v);
            }
            Some("assistant") => {
                // A limit's refusal is written as a reply; any other reply means it went on.
                info.limit = limit_of(&v).map(|l| SessionLimit { model: info.model.clone(), ..l });
                info.connection_lost = connection_lost_at(&v);
                // A subagent's reply, in transcripts from before their own logs, is its model and
                // its context, not this session's.
                let sidechain = v.get("isSidechain").and_then(Value::as_bool) == Some(true);
                if let Some(m) = v.pointer("/message/model").and_then(Value::as_str).filter(|m| !m.starts_with('<') && !sidechain) {
                    info.model = Some(m.to_string());
                    if info.window_model.as_deref().map(|w| w.split('[').next().unwrap_or(w)) != Some(m) {
                        info.window_model = Some(m.to_string());
                    }
                    if let Some(u) = v.pointer("/message/usage") {
                        let n = |k: &str| u.get(k).and_then(Value::as_u64).unwrap_or(0);
                        info.context = Some(n("input_tokens") + n("cache_read_input_tokens") + n("cache_creation_input_tokens")).filter(|c| *c > 0).or(info.context);
                    }
                    info.effort = reply_effort(&v);
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
                if user_prompt(&v).is_some() || ran_a_turn(&v) {
                    open_tools.clear();
                }
                // Written as the line's text, or its first text part.
                let content = v.pointer("/message/content");
                let said = content.and_then(Value::as_str).or_else(|| content?.as_array()?.iter().find_map(|i| i.get("text")?.as_str()));
                if let Some(m) = said.and_then(model_set) {
                    info.window_model = Some(m.clone());
                    info.model = Some(m);
                }
                if let Some(e) = said.and_then(effort_set) {
                    info.effort = e;
                }
                if you_typed(&v) {
                    info.limit = None;
                    info.connection_lost = None;
                    info.last_tool = None;
                    // Its `last-prompt` line often comes only after the reply has begun (in 104 of
                    // 191 prompts here, 2026-10-15), so the prompt itself says what it's on now.
                    if let Some(p) = typed {
                        // A resume's prompt from Codealong, for a PR's news say, starts with the
                        // line that says so.
                        info.last_prompt = Some(one_line(crate::send::past_sender_lines(&p), 140));
                    }
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
pub(super) fn model_set(content: &str) -> Option<String> {
    let rest = content.strip_prefix("<local-command-stdout>")?.strip_prefix("Set model to ")?;
    let said = match rest.strip_prefix('`') {
        Some(quoted) => quoted.split('`').next()?,
        None => rest.split(['<', ' ']).next()?,
    };
    let id = said.rsplit_once(" (").and_then(|(_, id)| id.strip_suffix(')')).unwrap_or(said);
    Some(id.trim().to_string()).filter(|m| !m.is_empty())
}

/// The effort level a reply was made at, as Claude Code reads it back for its own (2.1.299's
/// code): `perTurnEffort` where the line has it, else `effort`, either of which may be a number.
/// A model that takes none (Haiku 4.5) gets `perTurnEffort: null` and no `effort`, as do replies
/// from before effort was written.
fn reply_effort(v: &Value) -> Option<String> {
    match v.get("perTurnEffort").or_else(|| v.get("effort"))? {
        Value::String(e) => Some(e.trim().to_string()).filter(|e| !e.is_empty()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// The effort level `/effort` set, from what it wrote (2.1.299): "Set effort level to high (this
/// session only): …", "Effort 'max' exceeds the cap for … set to 'xhigh' instead …", or "Effort
/// level set to auto", the model's own, which only the next reply names (`Some(None)`). None
/// where it changed nothing: an environment variable that overrides it, a failure.
pub(super) fn effort_set(content: &str) -> Option<Option<String>> {
    let rest = content.strip_prefix("<local-command-stdout>")?;
    if rest.starts_with("Effort level set to auto") {
        return Some(None);
    }
    let level = match rest.strip_prefix("Set effort level to ") {
        Some(level) => level.split([' ', ':', '<']).next()?,
        None => rest.strip_prefix("Effort '")?.split_once("; set to '")?.1.split('\'').next()?,
    };
    Some(Some(level.trim().to_string()).filter(|l| !l.is_empty()))
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
        ..SessionLimit::default()
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
            if texts.any(|t| t.starts_with(INTERRUPTED)) {
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

/// Whether a line ends the turn it's in (`turn_step`): every line that does but a prompt you
/// typed, which starts one.
fn ends_turn(v: &Value) -> bool {
    turn_step(v) == Some(None) && !you_typed(v)
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

/// Most background shells kept per session (`TranscriptInfo::background`): each one ends with a
/// notification, so any past a few are ones whose end was never read.
const BACKGROUND_MOST: usize = 16;

/// Whether `id` names a shell Claude Code runs in the background (a Bash call's, one moved
/// there past its timeout, a Monitor's): `b` and eight of `0-9a-z`, each kind of task having a
/// letter of its own (2.1.312's code: `a` a subagent's; TaskUpdate's ids are numbers).
fn shell_task(id: &str) -> bool {
    id.len() == 9 && id.starts_with('b') && id[1..].bytes().all(|c| c.is_ascii_digit() || c.is_ascii_lowercase())
}

/// Follows the shells a session runs in the background through a line. A call's result starts
/// one: a Bash call's `backgroundTaskId`, a Monitor's `taskId`. A notification with a `<status>`
/// ends one (completed, failed, killed, or stopped as the session ended; a Monitor's events come
/// without), as a line of its own or one held for the turn to take in (`queued_command`); so
/// does TaskStop's result, which no notification follows.
fn background_step(running: &mut Vec<(String, i64)>, v: &Value, at: Option<i64>) {
    if v.get("isSidechain").and_then(Value::as_bool) == Some(true) {
        return;
    }
    let result = v.get("toolUseResult");
    let started = result.and_then(|r| r.get("backgroundTaskId").or_else(|| r.get("taskId"))).and_then(Value::as_str).filter(|id| shell_task(id));
    if let (Some(id), Some(at)) = (started, at) {
        if !running.iter().any(|(r, _)| r == id) {
            running.push((id.to_string(), at));
        }
        if running.len() > BACKGROUND_MOST {
            running.remove(0);
        }
    }
    let stopped = result.and_then(|r| r.get("task_id")).and_then(Value::as_str);
    // The text of a line, not a tool's result, which can quote a notification.
    let told = match v.get("type").and_then(Value::as_str) {
        Some("user") => v.pointer("/message/content"),
        Some("attachment") if v.pointer("/attachment/type").and_then(Value::as_str) == Some("queued_command") => v.pointer("/attachment/prompt"),
        _ => None,
    };
    let text = told.map(crate::transcript::text_of).unwrap_or_default();
    let ended: Vec<&str> = text
        .split("<task-notification>")
        .skip(1)
        .filter_map(|n| n.split("</task-notification>").next())
        .filter(|n| n.contains("<status>"))
        .filter_map(|n| n.split("<task-id>").nth(1)?.split("</task-id>").next())
        .collect();
    running.retain(|(id, _)| Some(id.as_str()) != stopped && !ended.contains(&id.as_str()));
}

fn str_of(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(String::from)
}

/// A real prompt you typed, not a tool result, command wrapper, compaction summary or meta line.
fn user_prompt(v: &Value) -> Option<String> {
    let flag = |k: &str| v.get(k).and_then(Value::as_bool) == Some(true);
    if flag("isMeta") || flag("isSidechain") || flag("isCompactSummary") || flag("stackedExpansion") {
        return None;
    }
    let text = prompt_text(v)?;
    if text.trim_start().starts_with("Caveat:") {
        return None;
    }
    command_run(v, &text).or_else(|| prompt_words(&text)).map(|t| one_line(&t, 140))
}

/// A slash command you ran that starts a turn (a skill, or a command of the project's or a
/// plugin's), as you typed it: `/name args`. Claude Code (2.1.302's code) writes only its tags,
/// `<command-message>name</command-message>\n<command-name>/name</command-name>\n<command-args>args</command-args>`
/// (no args tag without args), marked typed, with what the command expands to on a meta line
/// after it. Several run at once (`/a /b words`) are a line each, the first holding the whole
/// line as typed (`stackedOriginalInput`) and the others marked `stackedExpansion`, which aren't
/// prompts of their own (2.1.302's code). Its own commands that run without a turn (`/model`,
/// `/effort`, `/login`) were written in tags on `user` lines too, but never marked (2.1.224 to
/// 2.1.299 here, 2026-10-18), and 2.1.302 writes them as `system` / `local_command` lines; nor is
/// a scheduled run marked (`turnOrigin: "scheduled"`), so only a marked line is one. Unmarked
/// older transcripts' commands stay unshown, as there's nothing to tell the two apart by.
fn command_run(v: &Value, text: &str) -> Option<String> {
    if v.pointer("/origin/kind").and_then(Value::as_str) != Some("human") {
        return None;
    }
    if let Some(typed) = v.get("stackedOriginalInput").and_then(Value::as_str).map(str::trim).filter(|t| !t.is_empty()).filter(|_| text.trim_start().starts_with("<command-")) {
        return Some(typed.to_string());
    }
    command_typed(text)
}

/// A command line's tags as it was typed, `/name args`, whoever ran it.
fn command_typed(text: &str) -> Option<String> {
    if !text.trim_start().starts_with("<command-") {
        return None;
    }
    let between = |tag: &str| {
        let (open, close) = (format!("<{tag}>"), format!("</{tag}>"));
        let from = text.find(&open)? + open.len();
        text[from..].find(&close).map(|len| text[from..from + len].trim())
    };
    let name = between("command-name").filter(|n| n.starts_with('/'))?;
    Some(match between("command-args").filter(|a| !a.is_empty()) {
        Some(args) => format!("{name} {args}"),
        None => name.to_string(),
    })
}

/// A command line nothing marks (no `origin`, nor a scheduled run's `turnOrigin`), as typed: one
/// of Claude Code's own, which may or may not take a turn. The line after says (`ran_a_turn`).
pub(super) fn unmarked_command(v: &Value) -> Option<String> {
    if v.get("type").and_then(Value::as_str) != Some("user") || v.get("origin").is_some() || v.get("turnOrigin").is_some() {
        return None;
    }
    command_typed(v.pointer("/message/content")?.as_str()?)
}

/// The turn a command of Claude Code's own took when you ran it (2.1.307's `/goal`, which hands
/// the agent its condition): the command's line is unmarked, as one that takes no turn is
/// (`unmarked_command`), and the turn's marks (`turnOrigin`, `turnPosition`, the next prompt
/// number) go on the line after it, the command's output, which goes to the agent as a `user`
/// line. A command whose output goes nowhere near the agent writes that as a `system` /
/// `local_command` line instead (from 2.1.302). The two lines are written together, with one
/// `promptId` and one time.
pub(super) fn ran_a_turn(v: &Value) -> bool {
    let flag = |k: &str| v.get(k).and_then(Value::as_bool) == Some(true);
    v.get("type").and_then(Value::as_str) == Some("user")
        && v.get("origin").is_none()
        && !flag("isMeta")
        && !flag("isSidechain")
        && v.get("turnOrigin").and_then(Value::as_str) == Some("human")
        && v.pointer("/message/content").and_then(Value::as_str).is_some_and(|t| t.trim_start().starts_with("<local-command-stdout>"))
}

/// What Claude Code holds for the turn to take in, after one of its `queue-operation` lines: a
/// prompt sent while a turn ran, a background task's notification, another session's message.
/// `enqueue` adds one, with its text as `content` (a few have none: nothing to show); `remove`
/// takes out the one with that `content`, which the running turn took in (`reason:
/// "absorbed_mid_turn"`); `popAll` takes them all back into the input box (↑ there); and
/// `dequeue` takes one out with nothing to say which. Claude Code takes the first of the
/// highest priority (2.1.307's code: `now`, `next`, `later`), which isn't written; prompts and
/// most notifications are `next`, so it's taken as the first held, and a turn that starts on a
/// prompt of yours settles what's left of yours (`turn_took`): replayed over 800 transcripts here
/// (2026-10-21), that left a message of yours looking held at 12 of 7,516 turns' ends, ten of them
/// automated prompts in plain words (a loop's) and two commands, where taking the first alone left
/// 157. Each is known by the time it came, which is how its send finds it (`queued_as`).
/// Says what a `dequeue` was put down to.
pub(super) fn queue_step(queue: &mut Vec<QueuedMessage>, v: &Value) -> Option<QueuedMessage> {
    let content = v.get("content").and_then(Value::as_str);
    match v.get("operation").and_then(Value::as_str) {
        Some("enqueue") => {
            let id = str_of(v, "timestamp")?;
            let at = parse_rfc3339_ms(&id).unwrap_or(0);
            queue.push(QueuedMessage { id, at, text: content.unwrap_or_default().to_string() });
        }
        Some("remove") => {
            if let Some(i) = content.and_then(|c| queue.iter().position(|m| m.text == c)) {
                queue.remove(i);
            }
        }
        Some("popAll") => queue.clear(),
        Some("dequeue") if !queue.is_empty() => return Some(queue.remove(0)),
        _ => {}
    }
    None
}

/// Replays a transcript line onto what Claude Code holds: its queue's (`queue_step`), a turn's
/// prompt of yours (`turn_took`) or the command line before one (`ran_a_turn`). Says what the
/// prompt was typed as, where it's one: its own words, or the command the line before says.
fn hold_line(queue: &mut Vec<QueuedMessage>, holding: &mut Holding, v: &Value) -> Option<String> {
    match v.get("type").and_then(Value::as_str) {
        Some("queue-operation") => {
            holding.dequeued.extend(queue_step(queue, v).map(|m| m.text));
            None
        }
        Some("user") => {
            let mut typed = None;
            if you_typed(v) {
                typed = typed_text(v).or_else(|| holding.command.take().filter(|_| ran_a_turn(v)));
                if let Some(at) = str_of(v, "timestamp").as_deref().and_then(parse_rfc3339_ms) {
                    turn_took(queue, at, typed.as_deref(), &holding.dequeued);
                }
                holding.dequeued.clear();
            }
            holding.command = unmarked_command(v);
            typed
        }
        _ => None,
    }
}

/// How long a send waits for Claude Code to write that it holds the message (`queued_as`): its
/// writer drains every 100 ms.
const HOLD_WAIT: std::time::Duration = std::time::Duration::from_millis(1500);

/// The id Claude Code holds the message `text`, sent at `since` (ms), under (`queue_step`): the
/// `enqueue` written since with `text` as its `content`, read again until it's written, for a
/// moment; else, once that's over, the first of yours since (a long paste may be held as
/// `[Pasted text #1 …]`). Claude Code stamps it on this Mac's clock, after the paste. Sends to a
/// session go one at a time, so what came since is this one's, or typed in its terminal meanwhile.
pub fn queued_as(path: &Path, text: &str, since: i64) -> Option<String> {
    queued_as_within(path, text, since, HOLD_WAIT)
}

fn queued_as_within(path: &Path, text: &str, since: i64, wait: std::time::Duration) -> Option<String> {
    let until = std::time::Instant::now() + wait;
    loop {
        let Ok(len) = std::fs::metadata(path).map(|m| m.len()) else { return None };
        // Newest first, back to the queue's last line from before the send. Only its own lines
        // are in time order: an attachment is stamped when what it carries came, which can be
        // seconds before the lines ahead of it.
        let mut held: Vec<QueuedMessage> = Vec::new();
        scan_lines_back(path, len.saturating_sub(QUEUE_SCAN), len, |line| {
            if !line.contains("\"queue-operation\"") {
                return false;
            }
            let Ok(v) = serde_json::from_str::<Value>(line) else { return false };
            if v.get("type").and_then(Value::as_str) != Some("queue-operation") {
                return false;
            }
            let Some(at) = str_of(&v, "timestamp").as_deref().and_then(parse_rfc3339_ms) else { return false };
            if at < since {
                return true;
            }
            if v.get("operation").and_then(Value::as_str) == Some("enqueue") {
                queue_step(&mut held, &v);
            }
            false
        });
        held.reverse();
        if let Some(m) = held.iter().find(|m| m.text.trim() == text.trim()) {
            return Some(m.id.clone());
        }
        if std::time::Instant::now() >= until {
            return held.iter().find(|m| prompt_words(&m.text).is_some()).map(|m| m.id.clone());
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

/// How far back the queue's lines are looked for: by a send, for the one it made (`queued_as`),
/// and by a first read, before its tail (`held_before`).
const QUEUE_SCAN: u64 = 1024 * 1024;

/// What Claude Code held as the tail read from `end` starts (`queue_step`): a message can be held
/// while more than the tail holds is written (a long step's output, a subagent's), and a held
/// command for as long as the turn another command took (`turn_took`). A first read replays the
/// queue's lines, the turns' prompts and the command lines before them from `QUEUE_SCAN` before
/// its tail, once (`hold_line`); later reads carry what's held forward. A prompt's line is known
/// for a turn's by its `turnPosition` (2.1.30x). The same look back finds the shells it runs in
/// the background (`background_step`), which a long turn's output can push out of the tail too.
fn held_before(path: &Path, end: u64) -> (Vec<QueuedMessage>, Vec<(String, i64)>) {
    let (mut lines, mut tasks): (Vec<Value>, Vec<Value>) = (Vec::new(), Vec::new());
    scan_lines_back(path, end.saturating_sub(QUEUE_SCAN), end, |line| {
        if line.contains("\"queue-operation\"") || line.contains("\"turnPosition\"") || line.contains("<command-name>") {
            lines.extend(serde_json::from_str::<Value>(line).ok());
        }
        if ["\"backgroundTaskId\"", "\"taskId\"", "\"task_id\"", "<task-notification>"].iter().any(|s| line.contains(s)) {
            tasks.extend(serde_json::from_str::<Value>(line).ok());
        }
        false
    });
    let (mut queue, mut holding) = (Vec::new(), Holding::default());
    for v in lines.iter().rev() {
        hold_line(&mut queue, &mut holding, v);
    }
    let mut background = Vec::new();
    for v in tasks.iter().rev() {
        background_step(&mut background, v, str_of(v, "timestamp").as_deref().and_then(parse_rfc3339_ms));
    }
    (queue, background)
}

/// What a turn that started at `at` on a prompt of yours, typed as `typed`, took in of what Claude
/// Code held (`queue_step`): every message of yours in words held before it, which it puts in
/// together, whichever the `dequeue`s are put down to (a notification it never took off can sit
/// first), and the command it runs, where it's one: held commands (`/`, and `!` for the shell) go
/// one at a time, each a turn of its own, so the others stay held (2.1.307's code). The command is
/// taken out by its words unless the `dequeue`s before the prompt were put down to it already
/// (`dequeued`): the same command held twice is still held once.
fn turn_took(queue: &mut Vec<QueuedMessage>, at: i64, typed: Option<&str>, dequeued: &[String]) {
    let typed = typed.map(str::trim);
    // Held as sent, with what you were looking at ahead of it (`without_context`).
    let held = |text: &str| Some(without_context(text).trim()) == typed;
    if !dequeued.iter().any(|d| held(d)) {
        if let Some(i) = queue.iter().position(|m| m.at <= at && held(&m.text)) {
            queue.remove(i);
        }
    }
    queue.retain(|m| m.at > at || prompt_words(&m.text).is_none() || without_context(&m.text).trim_start().starts_with(['/', '!']));
}

/// How Claude Code wraps text pasted into a prompt (2.1.28x): the tag, the text on lines of its
/// own, then a closing tag that names the same id. A long message Codealong sends a terminal
/// session is pasted, so it comes back as one: `\n\n<pasted_content id="3c58">\n…\n</pasted_content
/// id="3c58">\n`, with nothing typed around it (2026-10-05).
const PASTE_OPEN: &str = "<pasted_content id=\"";
const PASTE_CLOSE: &str = "</pasted_content id=\"";

/// How the line Claude Code writes as a turn is stopped starts: `[Request interrupted by user]`
/// in a reply, `… for tool use]` after the call it cut short.
pub(super) const INTERRUPTED: &str = "[Request interrupted";

/// What a prompt says, or None for a line of Claude Code's own rather than words of yours: empty,
/// tagged (a command's output, a notification) or an interruption. A paste is yours, so its tags
/// come off instead, and what was pasted stays where it was, even when it starts with a tag. A
/// comment sent from an artifact says what the comment does (`comment_in`).
pub(super) fn prompt_words(text: &str) -> Option<String> {
    let t = without_context(text).trim();
    if let Some(comment) = comment_in(t) {
        return (!comment.is_empty()).then_some(comment);
    }
    if t.is_empty() || (t.starts_with('<') && !t.starts_with(PASTE_OPEN)) || t.starts_with(INTERRUPTED) {
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
    if flag("isSidechain") || flag("isCompactSummary") || flag("stackedExpansion") {
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
        None => ran_a_turn(v) || user_prompt(v).is_some(),
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
    let text = prompt_text(v)?;
    command_run(v, &text).or_else(|| prompt_words(&text))
}

/// What Claude Code and the Claude app put ahead of the words you typed, for the model alone,
/// each a block of its own followed by a blank line: the app's reminder ahead of a chat's first
/// prompt (one without a folder, or in a worktree), as a text part of its own or at the start of
/// the prompt's text (2.1.280); and what you were looking at in an artifact as you sent it,
/// `<artifact-view-context artifact="<id>">`, a line of JSON and a note to the model, in the
/// Claude app and in a terminal (2.1.308). Read as tags, such a prompt wasn't yours: 23 of this
/// Mac's typed prompts started with an artifact's (2026-10-26), and none showed.
const AHEAD_OF_PROMPT: [&str; 2] = ["system-reminder", "artifact-view-context"];

/// A prompt's text without the blocks put ahead of what you typed (`AHEAD_OF_PROMPT`), and the
/// space before them and after; a text without one is as it was, its indent kept. A block that
/// isn't closed is left as it is.
pub(super) fn without_context(text: &str) -> &str {
    let mut rest = text;
    while let Some(after) = AHEAD_OF_PROMPT.iter().find_map(|tag| {
        let block = rest.trim_start();
        block.strip_prefix('<')?.strip_prefix(tag)?.starts_with(['>', ' ']).then_some(())?;
        let close = format!("</{tag}>");
        block.find(&close).map(|at| block[at + close.len()..].trim_start())
    }) {
        rest = after;
    }
    rest
}

/// How the Claude app hands a session a comment sent to it from an artifact's comment thread
/// (2.1.308): this heading, `Artifact: <link>`, the thread and the time, a note to the model, the
/// comment quoted between lines that carry one token, each of its lines led by `>`, where in the
/// artifact it's anchored (`<comment-anchor-context>`, its `Location:` line written for HTML), and
/// what to do with it. What it says is the comment; the rest is for the model (7 of this Mac's
/// prompts, 2026-10-26, each shown whole).
pub(super) const COMMENT_HEAD: &str = "[Artifact comment sent to Claude]";

/// The comment a prompt hands on (`COMMENT_HEAD`), as it was written; None for any other prompt,
/// or one whose quote doesn't close.
pub(super) fn comment_in(text: &str) -> Option<String> {
    let quoted = &text[comment_quote(text)?];
    let lines: Vec<&str> = quoted.lines().map(|l| l.strip_prefix("> ").or_else(|| l.strip_prefix('>')).unwrap_or(l)).collect();
    Some(lines.join("\n").trim().to_string())
}

/// Where in a prompt that hands on a comment (`COMMENT_HEAD`) its quoted lines are: between the
/// line that opens the quote and the one that closes it, which carry the same token.
pub(super) fn comment_quote(text: &str) -> Option<std::ops::Range<usize>> {
    let from = text.len() - text.trim_start().strip_prefix(COMMENT_HEAD)?.len();
    let open = "\n=== BEGIN ARTIFACT COMMENT ";
    let token_at = from + text[from..].find(open)? + open.len();
    let token_len = text[token_at..].find(" ===\n")?;
    let start = token_at + token_len + " ===\n".len();
    let end = start + text[start..].find(&format!("=== END ARTIFACT COMMENT {} ===", &text[token_at..token_at + token_len]))?;
    Some(start..end)
}

/// A prompt line's text, or its text parts joined, without what was put ahead of what you typed.
fn prompt_text(v: &Value) -> Option<String> {
    Some(match v.pointer("/message/content")? {
        Value::String(s) => without_context(s).to_string(),
        Value::Array(items) => items
            .iter()
            .filter(|i| i.get("type").and_then(Value::as_str) == Some("text"))
            .filter_map(|i| i.get("text").and_then(Value::as_str))
            .map(without_context)
            .filter(|t| !t.is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
        _ => return None,
    })
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

/// The tool that hands you files to look at: `files` (paths, absolute or from the folder it works
/// in), with a `caption`, `status` and `display` that say how the Claude app shows them.
pub const SEND_FILE: &str = "SendUserFile";

/// The first file a `SendUserFile` call names, by its name, and how many more.
fn sent_files(input: &Value) -> Option<String> {
    let files: Vec<&str> = input.get("files")?.as_array()?.iter().filter_map(Value::as_str).map(str::trim).filter(|f| !f.is_empty()).collect();
    let first = code(&short_path(files.first()?));
    Some(match files.len() {
        1 => first,
        n => format!("{first} and {} more", n - 1),
    })
}

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
        SEND_FILE => sent_files(input),
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
        SEND_FILE => sent_files(input).map(|f| format!("sending you {f}")),
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

/// The branch a transcript line says the session pushed to, with the line's time: a command's
/// result with `toolUseResult.gitOperation.push.branch`, the branch as the remote has it (a push
/// of `local-tiers:feature` names `feature`). Nothing on it says which remote or repo. Claude Code
/// writes it from what `git push` printed, so a quiet push (`git push -q`) gets none, and not
/// every push whose output a pipe cut short does either: of one session's 11 pushes to a branch,
/// 10 had it (2.1.296, 2026-10-13).
pub(super) fn pushed(line: &str) -> Option<(String, i64)> {
    if !(line.contains("\"gitOperation\"") && line.contains("\"push\":{")) {
        return None;
    }
    let v: Value = serde_json::from_str(line).ok()?;
    let branch = v.pointer("/toolUseResult/gitOperation/push/branch")?.as_str().filter(|b| !b.is_empty())?;
    let at = v.get("timestamp").and_then(Value::as_str).and_then(parse_rfc3339_ms).unwrap_or(0);
    Some((branch.to_string(), at))
}

/// What a reply's call used, for the token ledger: an `assistant` line's `message.usage`
/// (`input_tokens` not read from or into the cache, `cache_read_input_tokens`,
/// `cache_creation_input_tokens` split by how long they're kept in `cache_creation`, and
/// `output_tokens` with its thinking in `output_tokens_details.thinking_tokens`), `speed: "fast"`
/// for fast mode and `server_tool_use.web_search_requests`. No cost is written (`costUSD` went
/// in 1.0.9). A reply is written again on a line of its own for each part of it as it streams,
/// each with its usage so far and the same message and request IDs, which name the call. A turn
/// refused at a limit is a `<synthetic>` reply that used nothing.
pub fn token_use(line: &[u8], st: &mut crate::ledger::TokenState) -> Option<crate::ledger::TokenUse> {
    if memchr::memmem::find(line, br#""usage""#).is_none() || memchr::memmem::find(line, br#""type":"assistant""#).is_none() {
        return None;
    }
    let v: Value = serde_json::from_slice(line).ok()?;
    if v.get("type").and_then(Value::as_str) != Some("assistant") {
        return None;
    }
    if let Some(cwd) = v.get("cwd").and_then(Value::as_str).filter(|c| !c.is_empty()) {
        st.folder = cwd.to_string();
    }
    let m = v.get("message")?;
    let model = m.get("model").and_then(Value::as_str).filter(|m| !m.is_empty() && *m != "<synthetic>")?;
    let u = m.get("usage")?;
    let n = |v: &Value, k: &str| v.get(k).and_then(Value::as_u64).unwrap_or(0);
    let written = n(u, "cache_creation_input_tokens");
    let hour = u.get("cache_creation").map(|c| n(c, "ephemeral_1h_input_tokens")).unwrap_or(0).min(written);
    let id = m.get("id").and_then(Value::as_str)?;
    let id = match v.get("requestId").and_then(Value::as_str) {
        Some(req) => format!("{id}:{req}"),
        None => id.to_string(),
    };
    Some(crate::ledger::TokenUse {
        id,
        at: v.get("timestamp").and_then(Value::as_str).and_then(parse_rfc3339_ms).unwrap_or(0),
        session: st.session.clone(),
        folder: st.folder.clone(),
        model: model.to_string(),
        input: n(u, "input_tokens"),
        cache_read: n(u, "cache_read_input_tokens"),
        cache_write: written - hour,
        cache_write_1h: hour,
        output: n(u, "output_tokens"),
        reasoning: u.get("output_tokens_details").map(|d| n(d, "thinking_tokens")).unwrap_or(0),
        fast: u.get("speed").and_then(Value::as_str) == Some("fast"),
        searches: u.get("server_tool_use").map(|s| n(s, "web_search_requests")).unwrap_or(0),
        cost: None,
    })
}


/// The start of a transcript, for this folder's tests.
#[cfg(test)]
pub(super) const HEAD: &str = r#"{"type":"permission-mode","mode":"default"}
{"type":"user","cwd":"/Users/me/projects/sparkhq/spark-portal","sessionId":"25b09e0e","gitBranch":"portal","entrypoint":"cli","timestamp":"2026-09-23T14:00:00.000Z","message":{"role":"user","content":"Can you make a photo write tool?"}}
"#;

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use crate::claude::conversation::claude_input;
    use crate::claude::peer::FROM_YOU;

    #[test]
    fn provider_flags_are_structured_sticky_and_found_before_the_tail() {
        use serde_json::json;
        let refusal = json!({"type":"assistant","message":{"stop_reason":"refusal","content":[]}});
        assert!(flagged(&refusal));
        for api_error in ["dlp_request_denied", "safety_monitor_blocked"] {
            assert!(flagged(&json!({"type":"assistant","apiError":api_error,"message":{"content":[]}})));
        }
        for other in [
            json!({"type":"assistant","isSidechain":true,"message":{"stop_reason":"refusal"}}),
            json!({"type":"user","message":{"stop_reason":"refusal"}}),
            json!({"type":"assistant","message":{"stop_reason":"end_turn","content":[{"type":"text","text":"The safeguards flagged this session."}]}}),
            json!({"type":"assistant","isApiErrorMessage":true,"error":"rate_limit"}),
        ] { assert!(!flagged(&other), "{other}"); }
        let dir = crate::fixtures::scratch("provider-flags-claude");
        let path = dir.join("s.jsonl");
        let padding = json!({"type":"user","message":{"content":[{"type":"tool_result","content":"x".repeat(300_000)}]}});
        let later = json!({"type":"assistant","message":{"stop_reason":"end_turn","content":[]}});
        std::fs::write(&path, format!("{HEAD}{refusal}\n{padding}\n{later}\n")).unwrap();
        let parsed = parse_transcript(&path, 1, None).unwrap();
        assert!(parsed.1.flagged, "cold read looks beyond the 256KB tail");
        use std::io::Write;
        writeln!(std::fs::OpenOptions::new().append(true).open(&path).unwrap(), "{later}").unwrap();
        assert!(parse_transcript(&path, 2, Some(parsed)).unwrap().1.flagged);
        assert!(parse_transcript(&path, 2, None).unwrap().1.flagged, "restart keeps the flag");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn provider_flags_survive_large_append_gaps() {
        use serde_json::json;
        use std::io::Write;
        let dir = crate::fixtures::scratch("provider-flags-claude-gaps");
        let path = dir.join("s.jsonl");
        let refusal = json!({"type":"assistant","message":{"stop_reason":"refusal","content":[]}});
        let later = json!({"type":"assistant","message":{"stop_reason":"end_turn","content":[]}});
        let padding = json!({"type":"user","message":{"content":[{"type":"tool_result","content":"x".repeat(300_000)}]}});
        std::fs::write(&path, format!("{HEAD}{later}\n")).unwrap();
        let parsed = parse_transcript(&path, 1, None).unwrap();
        assert!(!parsed.1.flagged);
        let mut file = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
        writeln!(file, "{refusal}").unwrap();
        for _ in 0..5 { writeln!(file, "{padding}").unwrap(); }
        writeln!(file, "{later}").unwrap();
        let parsed = parse_transcript(&path, 2, Some(parsed)).unwrap();
        assert!(parsed.1.flagged, "the gap before the new tail is checked");
        for _ in 0..5 { writeln!(file, "{padding}").unwrap(); }
        writeln!(file, "{later}").unwrap();
        assert!(parse_transcript(&path, 3, Some(parsed)).unwrap().1.flagged, "a rebuilt tail keeps the known flag");
        assert!(parse_transcript(&path, 3, None).unwrap().1.flagged, "a restart keeps the flag");
        std::fs::remove_dir_all(dir).unwrap();
    }

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
    fn reads_the_effort_a_reply_was_made_at_and_one_effort_set() {
        let said = |text: &str| effort_set(&format!("<local-command-stdout>{text}</local-command-stdout>"));
        assert_eq!(said("Set effort level to max (this session only): Maximum capability with deepest reasoning."), Some(Some("max".into())));
        assert_eq!(said("Set effort level to high (saved as your default for new sessions): Comprehensive implementation"), Some(Some("high".into())));
        assert_eq!(said("Effort 'max' exceeds the cap for opus set by your settings or organization; set to 'xhigh' instead (this session only): Deeper"), Some(Some("xhigh".into())));
        assert_eq!(said("Effort level set to auto (this session only)"), Some(None));
        assert_eq!(said("Not applied: CLAUDE_CODE_EFFORT_LEVEL=low overrides effort this session, and max is session-only (nothing saved)"), None);
        assert_eq!(said("Set model to `Opus 5.5`"), None);
        let reply = |effort: &str| format!(r#"{{"type":"assistant",{effort}"message":{{"model":"claude-opus-5-5","content":[{{"type":"text","text":"Done."}}]}}}}"#);
        let set = |level: &str| format!(r#"{{"type":"user","message":{{"role":"user","content":"<local-command-stdout>Set effort level to {level} (this session only): …</local-command-stdout>"}}}}"#);
        // The turn's own level wins over the one set; older replies name only `effort`.
        assert_eq!(parse_lines("", &reply(r#""effort":"high","perTurnEffort":"low","#)).effort.as_deref(), Some("low"));
        assert_eq!(parse_lines("", &reply(r#""effort":"xhigh","#)).effort.as_deref(), Some("xhigh"));
        // `/effort` is the level until the next reply.
        assert_eq!(parse_lines("", &[reply(r#""effort":"xhigh","#), set("max")].join("\n")).effort.as_deref(), Some("max"));
        // A switch to a model that takes none: its replies' `perTurnEffort` is null, and the
        // level before isn't its.
        let haiku = r#"{"type":"assistant","perTurnEffort":null,"message":{"model":"claude-haiku-4-5-20251001","content":[{"type":"text","text":"Hi."}]}}"#;
        assert_eq!(parse_lines("", &[reply(r#""effort":"max","perTurnEffort":"max","#), haiku.to_string()].join("\n")).effort, None);
        assert_eq!(parse_lines("", &[reply(r#""effort":"xhigh","#), reply("")].join("\n")).effort, None);
        // A refusal written as a reply isn't one the model made.
        let refused = r#"{"type":"assistant","isApiErrorMessage":true,"perTurnEffort":"max","message":{"model":"<synthetic>","content":[{"type":"text","text":"API Error"}]}}"#;
        assert_eq!(parse_lines("", &[reply(r#""effort":"high","#), refused.to_string()].join("\n")).effort.as_deref(), Some("high"));
    }

    #[test]
    fn reads_how_full_the_context_is_as_claude_code_counts_it() {
        let reply = |sidechain: bool, input: u64| {
            format!(r#"{{"type":"assistant","isSidechain":{sidechain},"message":{{"model":"claude-opus-5-5","usage":{{"input_tokens":{input},"cache_read_input_tokens":26783,"cache_creation_input_tokens":24542,"output_tokens":280}},"content":[{{"type":"text","text":"Done."}}]}}}}"#)
        };
        // Input, cache reads and cache writes; not the reply's own output, nor a subagent's call.
        let info = parse_lines("", &[reply(false, 2), reply(true, 900_000)].join("\n"));
        assert_eq!(info.context, Some(2 + 26783 + 24542));
        // A subagent on another model leaves the session's model, and so its window, alone.
        let haiku = r#"{"type":"assistant","isSidechain":true,"message":{"model":"claude-haiku-4-5-20251001","usage":{"input_tokens":5},"content":[]}}"#;
        let info = parse_lines("", &[reply(false, 2), haiku.to_string()].join("\n"));
        assert_eq!((info.model.as_deref(), info.window_model.as_deref()), (Some("claude-opus-5-5"), Some("claude-opus-5-5")));

        // A compaction leaves what it says is left, until the next reply, read on from there.
        let compacted = r#"{"type":"system","subtype":"compact_boundary","compactMetadata":{"trigger":"auto","preTokens":967152,"postTokens":6627}}"#;
        let mut info = parse_lines("", &reply(false, 900_000));
        let mut state = TailState::default();
        apply_tail(&mut info, &mut state, compacted);
        assert_eq!(info.context, Some(6627));
        apply_tail(&mut info, &mut state, &reply(false, 1000));
        assert_eq!(info.context, Some(1000 + 26783 + 24542));
    }

    #[test]
    fn a_switch_to_a_models_1m_context_holds_while_its_replies_name_it_without() {
        let line = |kind: &str, body: &str| format!(r#"{{"type":"{kind}","sessionId":"s","message":{body}}}"#);
        let lines = [
            line("user", r#"{"role":"user","content":"<local-command-stdout>Set model to `sonnet[1m] (claude-sonnet-4-5[1m])`</local-command-stdout>"}"#),
            line("assistant", r#"{"model":"claude-sonnet-4-5","usage":{"input_tokens":300000},"content":[{"type":"text","text":"Hi."}]}"#),
        ];
        let info = parse_lines("", &lines.join("\n"));
        assert_eq!(info.window_model.as_deref(), Some("claude-sonnet-4-5[1m]"));
        assert_eq!(info.window_model.as_deref().and_then(crate::prices::context_window), Some(1_000_000));
        // Another model's reply is that model's window.
        let other = line("assistant", r#"{"model":"claude-haiku-4-5-20251001","content":[]}"#);
        assert_eq!(parse_lines("", &[lines[0].clone(), lines[1].clone(), other].join("\n")).window_model.as_deref(), Some("claude-haiku-4-5-20251001"));

        // A first read finds the switch past more than its tail holds.
        let dir = crate::fixtures::scratch("window-model");
        let path = dir.join("s.jsonl");
        let big = line("user", &format!(r#"{{"role":"user","content":[{{"type":"tool_result","tool_use_id":"t","content":"{}"}}]}}"#, "x".repeat(300_000)));
        std::fs::write(&path, format!("{HEAD}{}\n{big}\n{}\n", lines[0], lines[1])).unwrap();
        let (_, info, _) = parse_transcript(&path, 1, None).unwrap();
        assert_eq!(info.window_model.as_deref(), Some("claude-sonnet-4-5[1m]"));
    }

    #[test]
    fn the_mode_is_the_latest_stamp_or_prompt_of_its_own() {
        let stamped = r#"{"type":"permission-mode","permissionMode":"bypassPermissions","sessionId":"s"}
{"type":"user","isSidechain":true,"permissionMode":"plan","message":{"role":"user","content":"a subagent's prompt"}}
"#;
        assert_eq!(parse_lines(HEAD, stamped).permission_mode.as_deref(), Some("bypassPermissions"));
        let prompted = format!("{stamped}{}\n", r#"{"type":"user","permissionMode":"acceptEdits","message":{"role":"user","content":"go on"}}"#);
        assert_eq!(parse_lines(HEAD, &prompted).permission_mode.as_deref(), Some("acceptEdits"));
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

    /// `/color` as 2.1.296 writes it; `/color default` takes it away again.
    #[test]
    fn reads_the_color_it_was_given() {
        let teal = r#"{"type":"agent-color","agentColor":"cyan","sessionId":"s"}"#;
        assert_eq!(parse_lines(HEAD, teal).color.as_deref(), Some("cyan"));
        assert_eq!(parse_lines(HEAD, &format!("{teal}\n{}", r#"{"type":"agent-color","agentColor":"default","sessionId":"s"}"#)).color, None);
        assert_eq!(parse_lines(HEAD, r#"{"type":"agent-name","agentName":"x","sessionId":"s"}"#).color, None);
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

    /// A session Codealong resumed with something to tell it goes on with what it was told, not
    /// with the line that says Codealong sent it.
    #[test]
    fn the_latest_prompt_is_what_codealong_said_past_its_line() {
        let told = serde_json::json!({ "type": "user", "timestamp": "2026-10-15T20:47:20.000Z", "origin": { "kind": "human" }, "message": { "content": crate::send::by_codealong("On your PR #7: ken approved it.") } });
        assert_eq!(parse_lines(HEAD, &format!("{told}\n")).last_prompt.as_deref(), Some("On your PR #7: ken approved it."));
        let recorded = serde_json::json!({ "type": "last-prompt", "lastPrompt": crate::send::by_codealong("On your PR #7: ken approved it.") });
        assert_eq!(parse_lines(HEAD, &format!("{recorded}\n")).last_prompt.as_deref(), Some("On your PR #7: ken approved it."));
    }

    #[test]
    fn a_prompt_leaves_the_last_turns_calls_behind() {
        // As 2.1.300 wrote one: the prompt, then the reply, and its `last-prompt` line after.
        let before = r#"{"type":"last-prompt","lastPrompt":"start the eval"}
{"type":"assistant","timestamp":"2026-10-15T20:18:36.000Z","message":{"content":[{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"cat /tmp/tasks/b.output"}}]}}
{"type":"user","timestamp":"2026-10-15T20:18:36.400Z","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":"ok"}]}}
"#;
        let asked = format!("{before}{}\n", r#"{"type":"user","timestamp":"2026-10-15T20:47:20.000Z","origin":{"kind":"human"},"message":{"content":"how is this going?"}}"#);
        let info = parse_lines(HEAD, &asked);
        assert_eq!((info.last_tool, info.last_prompt.as_deref()), (None, Some("how is this going?")));
        let replying = format!("{asked}{}\n", r#"{"type":"assistant","timestamp":"2026-10-15T20:57:05.000Z","message":{"content":[{"type":"tool_use","id":"t2","name":"Bash","input":{"command":"date -u"}}]}}"#);
        assert_eq!(parse_lines(HEAD, &replying).last_tool.as_deref(), Some("running `date -u`"));
        // A message sent while it worked goes into the same turn, which is still on its call.
        let sent = format!("{before}{}\n", r#"{"type":"attachment","timestamp":"2026-10-15T20:19:00.000Z","attachment":{"type":"queued_command","prompt":"and the score?","origin":{"kind":"human"}}}"#);
        let info = parse_lines(HEAD, &sent);
        assert_eq!((info.last_tool.as_deref(), info.last_prompt.as_deref()), (Some("running `cat /tmp/tasks/b.output`"), Some("start the eval")));
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
        let sent = "\n\n<pasted_content id=\"3c58\">\nI went over these mocks again.\n\n1) Simplify \"Shown to\".\n</pasted_content id=\"3c58\">\n";
        assert_eq!(typed_text(&line(sent)).as_deref(), Some("I went over these mocks again.\n\n1) Simplify \"Shown to\"."));
        assert_eq!(user_prompt(&line(sent)).as_deref(), Some("I went over these mocks again."));
        // Pasted around words typed, each stays where it was, a paste that starts with a tag too.
        let mixed = "<pasted_content id=\"a\">\nlog\n</pasted_content id=\"a\">\n\nwhy does this fail? <pasted_content id=\"b\">\n<html>\n</pasted_content id=\"b\">";
        assert_eq!(typed_text(&line(mixed)).as_deref(), Some("log\n\nwhy does this fail? <html>"));
        // Claude Code's own tagged lines are still not prompts, nor is an empty paste.
        assert_eq!(typed_text(&line("<local-command-stdout>Total cost: $0.12</local-command-stdout>")), None);
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
        assert_eq!(typed_text(&line(&["<local-command-stdout>Cleared</local-command-stdout>"])), None);
        assert_eq!(user_prompt(&line(&[reminder, "Give Graham access on GitHub"])).as_deref(), Some("Give Graham access on GitHub"));
        // 2.1.280 put it at the start of the prompt's text instead.
        let said = |text: &str| serde_json::json!({ "type": "user", "origin": { "kind": "human" }, "message": { "content": text } });
        let worktree = "<system-reminder>\nYou are operating in a git worktree.\nWorktree name: kestrel\n</system-reminder>\n\nRows in the PRs view overflow.";
        assert_eq!(typed_text(&said(worktree)).as_deref(), Some("Rows in the PRs view overflow."));
        assert_eq!(user_prompt(&said(worktree)).as_deref(), Some("Rows in the PRs view overflow."));
    }

    #[test]
    fn a_prompt_sent_looking_at_an_artifact_is_what_you_typed() {
        let line = |content: Value| serde_json::json!({ "type": "user", "origin": { "kind": "human" }, "message": { "content": content } });
        // As 2.1.308 wrote one in the Claude app, the artifact's state ahead of the words typed.
        let looking = "<artifact-view-context artifact=\"a0c97093\">\n{\"context\":{\"mode\":\"canvas\",\"page\":\"activity\",\"selected\":[]}}\n(The JSON line above is this viewer's live state in the artifact <not typed by the user>.)\n</artifact-view-context>\n\n";
        let asked = format!("{looking}So how would later runs share memories?");
        assert_eq!(typed_text(&line(asked.clone().into())).as_deref(), Some("So how would later runs share memories?"));
        assert_eq!(user_prompt(&line(asked.clone().into())).as_deref(), Some("So how would later runs share memories?"));
        assert_eq!(prompt_words(&asked).as_deref(), Some("So how would later runs share memories?"));
        // In a terminal, after the images attached to it.
        let image = serde_json::json!({ "type": "image", "source": { "type": "base64", "media_type": "image/png", "data": "" } });
        let with_image = line(serde_json::json!([image, { "type": "text", "text": format!("{looking}Redesign this page") }]));
        assert_eq!(typed_text(&with_image).as_deref(), Some("Redesign this page"));
        // A command run looking at one is still the command; the block alone, or one never
        // closed, is no prompt.
        let ran = format!("{looking}<command-message>design</command-message>\n<command-name>/design</command-name>");
        assert_eq!(typed_text(&line(ran.into())).as_deref(), Some("/design"));
        assert_eq!(typed_text(&line(looking.into())), None);
        assert_eq!(prompt_words("<artifact-view-context artifact=\"a\">\nwords"), None);
        // Another tag, or the name as a prefix of one, is left alone.
        assert_eq!(prompt_words("<artifact-view-contexts>x</artifact-view-contexts>\n\nwords"), None);
        // A comment sent from an artifact's thread is the comment, as written; one whose quote
        // doesn't close is shown whole, and an empty one says nothing.
        let comment = |quoted: &str| format!("[Artifact comment sent to Claude]\nArtifact: https://claude.ai/code/artifact/a1\n\nIt is quoted below.\n\n=== BEGIN ARTIFACT COMMENT 7f ===\n{quoted}\n\nAnswer this comment in its thread.");
        let sent = comment("> Use the usual list\n>    with its filters\n=== END ARTIFACT COMMENT 7f ===");
        assert_eq!(typed_text(&line(sent.clone().into())).as_deref(), Some("Use the usual list\n   with its filters"));
        assert_eq!(user_prompt(&line(sent.into())).as_deref(), Some("Use the usual list"));
        let open = comment("> Use the usual list\n=== END ARTIFACT COMMENT 8e ===");
        assert_eq!(typed_text(&line(open.clone().into())).as_deref(), Some(open.trim()));
        assert_eq!(prompt_words(&comment(">\n=== END ARTIFACT COMMENT 7f ===")), None);
        // A part without one is as it was, its indent kept.
        let parts = line(serde_json::json!([{ "type": "text", "text": format!("{looking}Example:") }, { "type": "text", "text": "    return 1" }]));
        assert_eq!(typed_text(&parts).as_deref(), Some("Example:\n    return 1"));
    }

    #[test]
    fn a_slash_command_you_ran_is_your_prompt() {
        let line = |origin: Value, text: &str| serde_json::json!({ "type": "user", "timestamp": "2026-10-18T19:10:45.770Z", "origin": origin, "message": { "content": text } });
        let human = serde_json::json!({ "kind": "human" });
        // As 2.1.302 wrote a session's first prompt, `/doctor prompt-audit`, and 2.1.290 a skill
        // run without arguments.
        let ran = "<command-message>doctor</command-message>\n<command-name>/doctor</command-name>\n<command-args>prompt-audit</command-args>";
        assert_eq!(typed_text(&line(human.clone(), ran)).as_deref(), Some("/doctor prompt-audit"));
        assert_eq!(user_prompt(&line(human.clone(), ran)).as_deref(), Some("/doctor prompt-audit"));
        let bare = "<command-message>fix-vulns</command-message>\n<command-name>/fix-vulns</command-name>";
        assert_eq!(typed_text(&line(human.clone(), bare)).as_deref(), Some("/fix-vulns"));
        // Its words come as typed, over several lines too.
        let long = "<command-message>design</command-message>\n<command-name>/design</command-name>\n<command-args>a teaser\nfor LinkedIn</command-args>";
        assert_eq!(typed_text(&line(human.clone(), long)).as_deref(), Some("/design a teaser\nfor LinkedIn"));
        // Claude Code's own commands run without a turn and aren't marked (2.1.299's `/effort`),
        // a scheduled run isn't yours, and a skill the agent loaded names itself without a slash.
        let local = "<command-name>/effort</command-name>\n            <command-message>effort</command-message>\n            <command-args>high</command-args>";
        assert_eq!(typed_text(&line(Value::Null, local)), None);
        assert_eq!(user_prompt(&line(Value::Null, local)), None);
        let mut tick = line(Value::Null, "<command-message>codealong:orchestrate</command-message>\n<command-name>/codealong:orchestrate</command-name>\n<command-args>tick</command-args>");
        tick["turnOrigin"] = "scheduled".into();
        assert_eq!(typed_text(&tick), None);
        assert_eq!(typed_text(&line(human.clone(), "<command-message>review</command-message>\n<command-name>review</command-name>")), None);

        // It's what the session's on, and the conversation shows it as yours.
        let tail = format!("{}\n", line(human, ran));
        let info = parse_lines(&tail, &tail);
        assert_eq!((info.first_prompt.as_deref(), info.last_prompt.as_deref()), (Some("/doctor prompt-audit"), Some("/doctor prompt-audit")));
        let items = crate::claude::conversation::conversation_items(&serde_json::from_str(tail.trim()).unwrap(), 0);
        assert_eq!(items.iter().map(|it| (it.kind, it.text.as_str())).collect::<Vec<_>>(), vec![("you", "/doctor prompt-audit")]);
    }

    /// `/goal <condition>` as 2.1.307 wrote it: its command's line unmarked, then its output's,
    /// marked as the turn's, then what it tells the agent. The three share a time and a prompt.
    pub(crate) const GOAL: [&str; 3] = [
        r#"{"type":"user","promptId":"p7","uuid":"c1","parentUuid":"g1","timestamp":"2026-10-21T10:00:00.000Z","message":{"role":"user","content":"<command-name>/goal</command-name>\n            <command-message>goal</command-message>\n            <command-args>the parser's tests pass</command-args>"}}"#,
        r#"{"type":"user","promptId":"p7","uuid":"c2","parentUuid":"c1","timestamp":"2026-10-21T10:00:00.000Z","turnOrigin":"human","turnPosition":{"promptIndex":7,"turnIndex":18},"message":{"role":"user","content":"<local-command-stdout>Goal set: the parser's tests pass</local-command-stdout>"}}"#,
        r#"{"type":"user","isMeta":true,"promptId":"p7","uuid":"c3","parentUuid":"c2","timestamp":"2026-10-21T10:00:00.000Z","message":{"role":"user","content":"A session-scoped Stop hook is now active with condition: \"the parser's tests pass\"."}}"#,
    ];

    #[test]
    fn a_command_of_claude_codes_own_that_took_a_turn_is_your_prompt() {
        let v = |l: &str| serde_json::from_str::<Value>(l).unwrap();
        let [ran, said, meta] = GOAL;
        assert_eq!(unmarked_command(&v(ran)).as_deref(), Some("/goal the parser's tests pass"));
        assert!(!you_typed(&v(ran)) && you_typed(&v(said)) && !you_typed(&v(meta)));
        // It's what the session's on now, from when you ran it.
        let before = r#"{"type":"user","timestamp":"2026-10-21T09:00:00.000Z","origin":{"kind":"human"},"message":{"content":"Fix the build"}}"#;
        let info = parse_lines(HEAD, &([before, ran, said, meta].join("\n") + "\n"));
        assert_eq!((info.last_prompt.as_deref(), info.last_prompt_at), (Some("/goal the parser's tests pass"), parse_rfc3339_ms("2026-10-21T10:00:00.000Z")));
        // One that took no turn writes what it did as a `system` line (2.1.302 on), so it isn't.
        let cost = ran.replace("/goal", "/cost");
        let local = r#"{"type":"system","subtype":"local_command","timestamp":"2026-10-21T10:00:00.000Z","content":"<local-command-stdout>Total cost: $0.12</local-command-stdout>"}"#;
        let info = parse_lines(HEAD, &([before, cost.as_str(), local].join("\n") + "\n"));
        assert_eq!((info.last_prompt.as_deref(), info.last_prompt_at), (Some("Fix the build"), parse_rfc3339_ms("2026-10-21T09:00:00.000Z")));
        // A scheduled run marks its own line, and isn't yours.
        assert_eq!(unmarked_command(&v(&ran.replace(r#""promptId":"p7""#, r#""turnOrigin":"scheduled""#))), None);
    }

    /// A `queue-operation` line as Claude Code writes one.
    fn queue_line(at: &str, operation: &str, content: Option<&str>) -> String {
        let mut v = serde_json::json!({ "type": "queue-operation", "operation": operation, "timestamp": at, "sessionId": "s" });
        if let Some(c) = content {
            v["content"] = c.into();
        }
        v.to_string()
    }

    #[test]
    fn what_claude_code_holds_goes_by_its_queue_lines() {
        let held = |id: &str, text: &str| QueuedMessage { id: id.into(), at: parse_rfc3339_ms(id).unwrap(), text: text.into() };
        let notice = "<task-notification>\n<task-id>b1</task-id>\n<status>completed</status>\n</task-notification>";
        let mut lines = vec![
            queue_line("2026-10-21T10:00:01.000Z", "enqueue", Some("Also check the docs")),
            queue_line("2026-10-21T10:00:02.000Z", "enqueue", Some(notice)),
            queue_line("2026-10-21T10:00:03.000Z", "enqueue", Some("And the changelog")),
            // The running turn took this one in.
            queue_line("2026-10-21T10:00:04.000Z", "remove", Some("And the changelog")),
        ];
        let info = |lines: &[String]| parse_lines(HEAD, &(lines.join("\n") + "\n"));
        assert_eq!(info(&lines).queue.len(), 2);
        // Only yours show: the notification is the agent's to read.
        assert_eq!(info(&lines).held(), vec![held("2026-10-21T10:00:01.000Z", "Also check the docs")]);
        // A turn that starts on what's held takes the first.
        lines.push(queue_line("2026-10-21T10:00:05.000Z", "dequeue", None));
        assert_eq!((info(&lines).queue.len(), info(&lines).held()), (1, vec![]));
        // ↑ in the box takes back all there is.
        lines.push(queue_line("2026-10-21T10:00:06.000Z", "enqueue", Some("One more thing")));
        assert_eq!(info(&lines).held(), vec![held("2026-10-21T10:00:06.000Z", "One more thing")]);
        lines.push(queue_line("2026-10-21T10:00:07.000Z", "popAll", Some("One more thing")));
        assert!(info(&lines).queue.is_empty());
        // A notification Claude Code never took off sits first, so a `dequeue` is put down to it;
        // the turn that starts on your prompt says yours went in.
        lines.push(queue_line("2026-10-21T10:00:08.000Z", "enqueue", Some(notice)));
        lines.push(queue_line("2026-10-21T10:00:09.000Z", "enqueue", Some("Now the release notes")));
        lines.push(queue_line("2026-10-21T10:00:10.000Z", "dequeue", None));
        assert_eq!(info(&lines).held(), vec![held("2026-10-21T10:00:09.000Z", "Now the release notes")]);
        lines.push(r#"{"type":"user","timestamp":"2026-10-21T10:00:10.010Z","origin":{"kind":"human"},"message":{"content":"Now the release notes"}}"#.to_string());
        assert_eq!(info(&lines).held(), vec![]);
        // Held commands go one at a time: the turn the first takes leaves the second held.
        lines.push(queue_line("2026-10-21T10:00:11.000Z", "enqueue", Some("/review the parser")));
        lines.push(queue_line("2026-10-21T10:00:12.000Z", "enqueue", Some("/review the docs")));
        lines.push(queue_line("2026-10-21T10:00:13.000Z", "dequeue", None));
        lines.push(r#"{"type":"user","timestamp":"2026-10-21T10:00:13.010Z","origin":{"kind":"human"},"message":{"content":"<command-message>review</command-message>\n<command-name>/review</command-name>\n<command-args>the parser</command-args>"}}"#.to_string());
        assert_eq!(info(&lines).held(), vec![held("2026-10-21T10:00:12.000Z", "/review the docs")]);
        // The same command held twice is still held once after the first's turn.
        lines.push(queue_line("2026-10-21T10:00:14.000Z", "enqueue", Some("/review the docs")));
        lines.push(queue_line("2026-10-21T10:00:15.000Z", "dequeue", None));
        lines.push(r#"{"type":"user","timestamp":"2026-10-21T10:00:15.010Z","origin":{"kind":"human"},"message":{"content":"<command-message>review</command-message>\n<command-name>/review</command-name>\n<command-args>the docs</command-args>"}}"#.to_string());
        assert_eq!(info(&lines).held(), vec![held("2026-10-21T10:00:14.000Z", "/review the docs")]);
        // One sent looking at an artifact is held in your words, and a command so sent is still
        // one at a time.
        let looking = "<artifact-view-context artifact=\"a1\">\n{\"context\":{}}\n</artifact-view-context>\n\n";
        lines.push(queue_line("2026-10-21T10:00:16.000Z", "popAll", None));
        lines.push(queue_line("2026-10-21T10:00:17.000Z", "enqueue", Some(&format!("{looking}And the timeline?"))));
        lines.push(queue_line("2026-10-21T10:00:18.000Z", "enqueue", Some(&format!("{looking}/review the mocks"))));
        assert_eq!(info(&lines).held(), vec![held("2026-10-21T10:00:17.000Z", "And the timeline?"), held("2026-10-21T10:00:18.000Z", "/review the mocks")]);
        lines.push(queue_line("2026-10-21T10:00:19.000Z", "dequeue", None));
        lines.push(format!(r#"{{"type":"user","timestamp":"2026-10-21T10:00:19.010Z","origin":{{"kind":"human"}},"message":{{"content":{}}}}}"#, Value::from(format!("{looking}And the timeline?"))));
        assert_eq!(info(&lines).held(), vec![held("2026-10-21T10:00:18.000Z", "/review the mocks")]);
    }

    #[test]
    fn a_send_finds_what_claude_code_held_it_under() {
        let dir = crate::fixtures::scratch("claude-queued-as");
        let path = dir.join("s.jsonl");
        let since = parse_rfc3339_ms("2026-10-21T10:00:00.000Z").unwrap();
        let lines = [
            queue_line("2026-10-21T09:58:00.000Z", "enqueue", Some("Fix it")),
            r#"{"type":"assistant","timestamp":"2026-10-21T09:59:58.500Z","message":{"stop_reason":"tool_use","content":[]}}"#.to_string(),
            queue_line("2026-10-21T10:00:00.120Z", "enqueue", Some("<task-notification>\n</task-notification>")),
            queue_line("2026-10-21T10:00:00.200Z", "enqueue", Some("Fix it")),
            // Stamped when what it carries came, before the lines ahead of it.
            r#"{"type":"attachment","timestamp":"2026-10-21T09:59:50.000Z","attachment":{"type":"queued_command","prompt":"Earlier"}}"#.to_string(),
        ];
        std::fs::write(&path, lines.join("\n") + "\n").unwrap();
        let quick = std::time::Duration::from_millis(50);
        // The one written since, with its words, not an earlier one that said the same.
        assert_eq!(queued_as_within(&path, "Fix it ", since, quick).as_deref(), Some("2026-10-21T10:00:00.200Z"));
        // A long paste can be held in other words: the first of yours since then, once the wait
        // for its own words is over.
        assert_eq!(queued_as_within(&path, "a paste of many lines", since, quick).as_deref(), Some("2026-10-21T10:00:00.200Z"));
        // The send after it, its own not written yet, doesn't take that one's.
        let next = parse_rfc3339_ms("2026-10-21T10:00:00.900Z").unwrap();
        assert_eq!(queued_as_within(&path, "And the docs", next, quick), None);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_first_read_finds_what_was_held_before_its_tail() {
        let dir = crate::fixtures::scratch("claude-held-before");
        let path = dir.join("s.jsonl");
        let ended = r#"{"type":"system","subtype":"turn_duration","timestamp":"2026-10-21T09:59:00.000Z","durationMs":1000}"#;
        let started = r#"{"type":"user","timestamp":"2026-10-21T10:00:00.000Z","origin":{"kind":"human"},"turnOrigin":"human","turnPosition":{"promptIndex":2,"turnIndex":2},"message":{"content":"Fix the build"}}"#;
        // Held before the last turn ended, or before this one started, so not held now,
        // whatever the dequeues say.
        let lines = [
            queue_line("2026-10-21T09:58:00.000Z", "enqueue", Some("From before")),
            ended.to_string(),
            queue_line("2026-10-21T09:59:30.000Z", "enqueue", Some("Fix the build")),
            started.to_string(),
            queue_line("2026-10-21T10:00:01.000Z", "enqueue", Some("Also check the docs")),
        ];
        // A step's output the size of the tail, written while it's held.
        let padding = serde_json::json!({ "type": "progress", "timestamp": "2026-10-21T10:00:02.000Z", "data": "x".repeat(300_000) });
        std::fs::write(&path, format!("{HEAD}{}\n{padding}\n", lines.join("\n"))).unwrap();
        let (_, info, _) = parse_transcript(&path, 1, None).unwrap();
        assert_eq!(info.held().iter().map(|m| m.text.as_str()).collect::<Vec<_>>(), ["Also check the docs"]);
        // A `/goal` that took its turn, its `dequeue` put down to a notification never taken off:
        // the line before its turn's says what it was, as a later read's does.
        let notice = "<task-notification>\n<task-id>b1</task-id>\n</task-notification>";
        let lines = [
            queue_line("2026-10-21T09:59:00.000Z", "enqueue", Some(notice)),
            queue_line("2026-10-21T09:59:59.000Z", "enqueue", Some("/goal the parser's tests pass")),
            queue_line("2026-10-21T09:59:59.500Z", "dequeue", None),
        ];
        std::fs::write(&path, format!("{HEAD}{}\n{}\n{padding}\n", lines.join("\n"), GOAL.join("\n"))).unwrap();
        let (_, info, _) = parse_transcript(&path, 2, None).unwrap();
        assert_eq!(info.held(), vec![]);
        std::fs::remove_dir_all(dir).unwrap();
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
    fn a_first_read_looks_back_for_where_it_worked() {
        let dir = crate::fixtures::scratch("work-before");
        let path = dir.join("s.jsonl");
        let call = |cwd: &str, command: &str| serde_json::json!({ "type": "assistant", "cwd": cwd, "message": { "content": [{ "type": "tool_use", "id": "t", "name": "Bash", "input": { "command": command } }] } }).to_string();
        let output = |n: usize| format!(r#"{{"type":"user","message":{{"content":[{{"type":"tool_result","tool_use_id":"t","content":"{}"}}]}}}}"#, "x".repeat(n));
        let wt = "/r/app/.claude/worktrees/release";
        // Work in the worktree, a long build's output, then a command whose copy to a temp file
        // says nothing of where it works: the tail alone has no work.
        let mut lines = vec![HEAD.trim_end().to_string(), call("/r/app", &format!("cd {wt} && make")), call(wt, "git commit -am wip"), call(wt, "make test")];
        lines.extend((0..3).map(|_| output(120_000)));
        lines.push(call(wt, "make brief; cp out/brief.md /tmp/brief.md"));
        std::fs::write(&path, lines.join("\n") + "\n").unwrap();
        let (_, info, _) = parse_transcript(&path, 1, None).unwrap();
        assert_eq!(info.work, [wt, wt, wt]);
        // As many as reading it as it grew would have kept, oldest first, the tail's own after
        // them: places outside every worktree among them (a scratch file), which the board
        // passes over before it counts.
        let write = |file: &str| serde_json::json!({ "type": "assistant", "cwd": wt, "message": { "content": [{ "type": "tool_use", "id": "t", "name": "Write", "input": { "file_path": file, "content": "" } }] } }).to_string();
        let mut lines = vec![HEAD.trim_end().to_string(), call("/r/w0", "make")];
        lines.extend((1..5).map(|i| call(&format!("/r/w{i}"), "make")));
        lines.extend((0..7).map(|i| write(&format!("/tmp/notes-{i}.md"))));
        lines.extend((0..3).map(|_| output(120_000)));
        lines.push(call("/r/app", "make"));
        std::fs::write(&path, lines.join("\n") + "\n").unwrap();
        let (_, info, _) = parse_transcript(&path, 1, None).unwrap();
        let mut kept: Vec<String> = (1..5).map(|i| format!("/r/w{i}")).collect();
        kept.extend((0..7).map(|i| format!("/tmp/notes-{i}.md")));
        kept.push("/r/app".into());
        assert_eq!(info.work, kept);
        // Not past `WORK_LOOKBACK`: a long log with little work in it isn't read whole.
        let mut lines = vec![HEAD.trim_end().to_string(), call(wt, "make")];
        lines.extend((0..40).map(|_| output(120_000)));
        std::fs::write(&path, lines.join("\n") + "\n").unwrap();
        let (_, info, _) = parse_transcript(&path, 1, None).unwrap();
        assert!(info.work.is_empty());
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
    fn a_compaction_is_nothing_new() {
        let at = |ts: &str| parse_rfc3339_ms(ts);
        let ended = r#"{"type":"assistant","timestamp":"2026-10-21T16:58:26.000Z","message":{"stop_reason":"end_turn","content":[{"type":"text","text":"Landed."}]}}
{"type":"system","subtype":"turn_duration","timestamp":"2026-10-21T16:58:30.000Z"}
{"type":"system","subtype":"away_summary","timestamp":"2026-10-21T17:14:04.000Z","content":"You asked for the schedule pickers. They're done."}"#;
        // Compacted while idle, nearly an hour on, as 2.1.306 writes it: the PR's link stamped
        // again as it starts, the boundary once it's done, and what follows it.
        let boundary = r#"{"type":"pr-link","sessionId":"25b09e0e","prNumber":12,"prUrl":"https://github.com/atlas/app/pull/12","prRepository":"atlas/app","timestamp":"2026-10-21T17:52:12.000Z"}
{"type":"system","subtype":"compact_boundary","timestamp":"2026-10-21T17:53:08.248Z","compactMetadata":{"trigger":"auto","preTokens":246565,"postTokens":20935}}
{"type":"user","isCompactSummary":true,"timestamp":"2026-10-21T17:53:08.250Z","message":{"role":"user","content":"This session is being continued from a previous conversation that ran out of context."}}"#;
        let attached = |text: &str| format!(r#"{{"type":"attachment","timestamp":"2026-10-21T17:53:08.300Z","attachment":{{"type":"file","filename":"/r/app/src/main.ts","content":"{text}"}}}}"#);
        let notice = r#"{"type":"system","subtype":"informational","level":"notice","timestamp":"2026-10-21T17:53:08.725Z","content":"Compacted while idle, before the prompt cache expired"}
{"type":"ai-title","aiTitle":"Ship the schedule pickers"}"#;
        let compaction = format!("{boundary}\n{}\n{notice}", attached("export {}"));
        let info = parse_lines(HEAD, &format!("{ended}\n{compaction}\n"));
        assert_eq!(info.compacted, at("2026-10-21T16:58:30.000Z"));
        assert_eq!((info.last_ts, Some(info.active_at())), (at("2026-10-21T17:53:08.725Z").unwrap(), info.compacted));
        // Its turn ended then and nothing's been said since, while it compacts and after.
        assert_eq!(parse_lines(HEAD, &format!("{ended}\n")).ended_at, at("2026-10-21T16:58:30.000Z"));
        let starting = boundary.lines().next().unwrap();
        assert_eq!(parse_lines(HEAD, &format!("{ended}\n{starting}\n")).ended_at, at("2026-10-21T16:58:30.000Z"));
        assert_eq!(info.ended_at, at("2026-10-21T16:58:30.000Z"));
        // What starts a turn says something first: a prompt, another session's message, a
        // background task's report. A call is a turn going on.
        let message = r#"{"type":"user","isMeta":true,"timestamp":"2026-10-21T18:05:00.000Z","origin":{"kind":"peer","name":"atlas-2f"},"message":{"content":"The build is green"}}"#;
        let report = r#"{"type":"user","timestamp":"2026-10-21T18:06:00.000Z","origin":{"kind":"task-notification"},"message":{"content":"<task-notification>done</task-notification>"}}"#;
        let call = r#"{"type":"assistant","timestamp":"2026-10-21T18:07:00.000Z","message":{"stop_reason":"tool_use","content":[]}}"#;
        let typed = r#"{"type":"user","timestamp":"2026-10-21T18:10:00.000Z","origin":{"kind":"human"},"message":{"content":"Thanks"}}"#;
        for line in [message, report, call, typed] {
            assert_eq!(parse_lines(HEAD, &format!("{ended}\n{compaction}\n{line}\n")).ended_at, None, "{line}");
        }
        let replied = r#"{"type":"assistant","timestamp":"2026-10-21T18:10:09.000Z","message":{"stop_reason":"end_turn","content":[{"type":"text","text":"Glad it's in."}]}}"#;
        assert_eq!(parse_lines(HEAD, &format!("{ended}\n{compaction}\n{typed}\n{replied}\n")).ended_at, at("2026-10-21T18:10:09.000Z"));
        // Read a line at a time, as it's written.
        let mut info = parse_lines(HEAD, "");
        let mut state = TailState::default();
        for line in ended.lines().chain(compaction.lines()) {
            apply_tail(&mut info, &mut state, line);
        }
        assert_eq!(info.compacted, at("2026-10-21T16:58:30.000Z"));
        // A message after it ends it, and so does the reply a compaction partway through a turn
        // goes on to.
        let prompt = r#"{"type":"user","timestamp":"2026-10-21T18:10:00.000Z","origin":{"kind":"human"},"message":{"content":"Thanks"}}"#;
        let reply = r#"{"type":"assistant","timestamp":"2026-10-21T17:53:20.000Z","message":{"stop_reason":"tool_use","content":[]}}"#;
        for (after, ts) in [(prompt, "2026-10-21T18:10:00.000Z"), (reply, "2026-10-21T17:53:20.000Z")] {
            let info = parse_lines(HEAD, &format!("{ended}\n{compaction}\n{after}\n"));
            assert_eq!((info.compacted, Some(info.active_at())), (None, at(ts)), "{after}");
        }

        // A compaction's lines can run past the tail a first read takes: its boundary, and what
        // was said before it, are further back.
        let dir = crate::fixtures::scratch("compacted");
        let path = dir.join("s.jsonl");
        let big = attached(&"x".repeat(300_000));
        std::fs::write(&path, format!("{HEAD}{ended}\n{boundary}\n{big}\n{notice}\n")).unwrap();
        let parsed = parse_transcript(&path, 1, None).unwrap();
        assert_eq!(parsed.1.compacted, at("2026-10-21T16:58:30.000Z"));
        assert_eq!(parsed.1.ended_at, at("2026-10-21T16:58:30.000Z"));
        use std::io::Write;
        writeln!(std::fs::OpenOptions::new().append(true).open(&path).unwrap(), "{prompt}").unwrap();
        let parsed = parse_transcript(&path, 2, Some(parsed)).unwrap();
        assert_eq!((parsed.1.compacted, parsed.1.ended_at), (None, None));
        // Compacted again later, read on from there, it's the prompt's turn that ended before it.
        writeln!(std::fs::OpenOptions::new().append(true).open(&path).unwrap(), "{compaction}").unwrap();
        assert_eq!(parse_transcript(&path, 3, Some(parsed)).unwrap().1.compacted, at("2026-10-21T18:10:00.000Z"));
        // One a message came after is over, wherever the tail starts.
        std::fs::write(&path, format!("{HEAD}{ended}\n{compaction}\n{prompt}\n{big}\n{notice}\n")).unwrap();
        assert_eq!(parse_transcript(&path, 1, None).unwrap().1.compacted, None);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn follows_the_shells_it_runs_in_the_background() {
        use serde_json::json;
        let at = |ts: &str| parse_rfc3339_ms(ts).unwrap();
        let result = |ts: &str, r: Value| json!({"type": "user", "timestamp": ts, "message": {"content": [{"type": "tool_result", "tool_use_id": "t", "content": "ok"}]}, "toolUseResult": r}).to_string();
        let notice = |id: &str, status: &str| format!("<task-notification>\n<task-id>{id}</task-id>\n<tool-use-id>t</tool-use-id>\n{status}<summary>Monitor event</summary>\n</task-notification>");
        let told = |text: &str| json!({"type": "user", "timestamp": "2026-10-29T22:00:00.000Z", "origin": {"kind": "task-notification"}, "message": {"content": text}}).to_string();
        let held = |text: &str| json!({"type": "attachment", "timestamp": "2026-10-29T22:00:00.000Z", "attachment": {"type": "queued_command", "prompt": text}}).to_string();
        let started = [
            result("2026-10-29T21:50:00.000Z", json!({"stdout": "", "backgroundTaskId": "b3x8c1v5n"})),
            result("2026-10-29T21:57:57.000Z", json!({"taskId": "b7k2m9p4q", "timeoutMs": 900000, "persistent": false})),
            // TaskUpdate's ids are its list's numbers, and a subagent's start with `a`.
            result("2026-10-29T21:58:00.000Z", json!({"success": true, "taskId": "1"})),
            result("2026-10-29T21:58:01.000Z", json!({"backgroundTaskId": "a1b2c3d4e"})),
        ];
        let mut info = parse_lines(HEAD, &started.join("\n"));
        let both = vec![("b3x8c1v5n".to_string(), at("2026-10-29T21:50:00.000Z")), ("b7k2m9p4q".to_string(), at("2026-10-29T21:57:57.000Z"))];
        assert_eq!(info.background, both);
        let mut state = TailState::default();
        // A Monitor's event isn't its end, nor is a command's output that quotes one, nor a
        // subagent's.
        let quoted = result("2026-10-29T21:59:00.000Z", json!({"stdout": notice("b7k2m9p4q", "<status>completed</status>\n")}));
        let sidechain = json!({"type": "user", "isSidechain": true, "message": {"content": notice("b7k2m9p4q", "<status>completed</status>\n")}}).to_string();
        apply_tail(&mut info, &mut state, &[told(&notice("b7k2m9p4q", "")), quoted, sidechain].join("\n"));
        assert_eq!(info.background, both);
        // A notification with a status is, held for the turn or a line of its own.
        apply_tail(&mut info, &mut state, &held(&notice("b3x8c1v5n", "<status>completed</status>\n")));
        assert_eq!(info.background, both[1..]);
        apply_tail(&mut info, &mut state, &told(&notice("b7k2m9p4q", "<status>killed</status>\n")));
        assert!(info.background.is_empty());
        // TaskStop's result is, with no notification after it.
        let stop = result("2026-10-29T22:01:00.000Z", json!({"message": "Successfully stopped task: b3x8c1v5n", "task_id": "b3x8c1v5n", "task_type": "local_bash"}));
        let mut info = parse_lines(HEAD, &format!("{}\n{stop}", started[0]));
        assert!(info.background.is_empty());
        // Ends never read leave no more than a few behind.
        let many: Vec<String> = (0..40).map(|i| result("2026-10-29T22:02:00.000Z", json!({"backgroundTaskId": format!("b{i:08}")}))).collect();
        apply_tail(&mut info, &mut TailState::default(), &many.join("\n"));
        assert_eq!((info.background.len(), info.background[0].0.as_str()), (BACKGROUND_MOST, "b00000024"));
        // A first read finds one started before its tail, past a long command's output, and
        // not one that ended there.
        let dir = crate::fixtures::scratch("background-before-tail");
        let path = dir.join("s.jsonl");
        let output = result("2026-10-29T21:58:30.000Z", json!({"stdout": "x".repeat(300_000)}));
        let ended = held(&notice("b3x8c1v5n", "<status>completed</status>\n"));
        std::fs::write(&path, format!("{HEAD}{}\n{ended}\n{output}\n{}\n", started.join("\n"), told(&notice("b7k2m9p4q", "")))).unwrap();
        let fresh = parse_transcript(&path, 1, None).unwrap().1;
        assert_eq!(fresh.background, both[1..]);
        std::fs::remove_dir_all(dir).unwrap();
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
        // Files it sends you go by their names.
        let one = serde_json::json!({"files": ["/Users/me/app/tmp/teaser.mp4"], "caption": "24 s, no audio", "status": "normal"});
        assert_eq!(tool_request(SEND_FILE, &one), "SendUserFile · `teaser.mp4`");
        assert_eq!(tool_activity(SEND_FILE, &one), "sending you `teaser.mp4`");
        let three = serde_json::json!({"files": ["shots/before.png", "shots/after.png", "report.pdf"]});
        assert_eq!(tool_request(SEND_FILE, &three), "SendUserFile · `before.png` and 2 more");
        assert_eq!(tool_request(SEND_FILE, &serde_json::json!({"files": []})), "SendUserFile");
    }

    #[test]
    fn reads_what_a_reply_used_with_an_hours_cache_writes_apart() {
        // As 2.1.296 writes it: one line per part of the reply, the same IDs on each.
        let line = r#"{"type":"assistant","cwd":"/Users/me/app","requestId":"req_1","timestamp":"2026-10-12T18:00:00.000Z","message":{"id":"msg_1","model":"claude-opus-5-5","usage":{"input_tokens":2,"cache_creation_input_tokens":24542,"cache_read_input_tokens":26783,"output_tokens":280,"output_tokens_details":{"thinking_tokens":77},"server_tool_use":{"web_search_requests":1,"web_fetch_requests":0},"service_tier":"standard","cache_creation":{"ephemeral_1h_input_tokens":24000,"ephemeral_5m_input_tokens":542},"speed":"fast"}}}"#;
        let mut st = crate::ledger::TokenState { session: "claude:claude:s".into(), ..Default::default() };
        let u = token_use(line.as_bytes(), &mut st).unwrap();
        assert_eq!(u.id, "msg_1:req_1");
        assert_eq!((u.input, u.cache_read, u.cache_write, u.cache_write_1h, u.output, u.reasoning), (2, 26783, 542, 24000, 280, 77));
        assert_eq!((u.fast, u.searches, u.model.as_str(), u.folder.as_str(), u.session.as_str()), (true, 1, "claude-opus-5-5", "/Users/me/app", "claude:claude:s"));
        let refused = r#"{"type":"assistant","isApiErrorMessage":true,"message":{"id":"x","model":"<synthetic>","usage":{"input_tokens":0,"output_tokens":0}}}"#;
        assert_eq!(token_use(refused.as_bytes(), &mut st), None);
        let you = r#"{"type":"user","message":{"role":"user","content":"what's the \"usage\"?"}}"#;
        assert_eq!(token_use(you.as_bytes(), &mut st), None);
    }
}
