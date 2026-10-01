//! Claude Code: the live session registry (`<home>/sessions/<pid>.json`) and transcripts
//! (`<home>/projects/<encoded-cwd>/<session>.jsonl`). See *Harness adapters*.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Deserialize;
use serde_json::Value;

use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};

use crate::config::AccountCfg;
use crate::harness::{Applied, Look, NewLines};
use crate::model::{Harness, OriginHow, Session, SessionOrigin, Usage};
use crate::procs;
use crate::transcript::{clip_input, clip_output, item, kept_whole, text_of, Item, Page};
use crate::util::{code, command_dir, file_name, mtime_ms, note_work, one_line, parallel_map, parse_rfc3339_ms, read_head, resolve, scan_lines_back, without_cd};

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
    /// Set on a window whose conversation moved to that background job: until you go back to
    /// it, the window shows Claude Code's list of agents and runs no session.
    pub parked_job: Option<String>,
    /// A background session the account's service started ahead of time, so the next
    /// background start takes it over at once. It has no conversation until then, and loses
    /// the mark when its first turn starts.
    pub spare: bool,
    pub entrypoint: Option<String>,
    pub name: Option<String>,
    pub name_source: Option<String>,
    pub started_at: Option<i64>,
    pub status_updated_at: Option<i64>,
    /// For a background job that goes on from another session's conversation (a fork, or a
    /// window's `/background`), that session's id, from the service's roster.
    pub resumes: Option<String>,
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
    parked_job_id: Option<String>,
    #[serde(default)]
    spare: bool,
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
        parked_job: raw.parked_job_id,
        spare: raw.spare,
        entrypoint: raw.entrypoint,
        name: raw.name,
        name_source: raw.name_source,
        started_at: raw.started_at,
        status_updated_at: raw.status_updated_at,
        resumes: None,
    })
}

/// The conversation a background job goes on from, from the service's roster
/// (`<home>/daemon/roster.json`): `workers.<job>.dispatch.launch` says `mode: "resume"` and
/// names the session, by its id (`claude --bg --resume <id>`, with `--fork-session` for a copy)
/// or by its transcript's path (`/background`). None for a job that started afresh, and for
/// one that goes on as the session itself.
pub fn resumed_from(roster: &Value, job: &str) -> Option<String> {
    let worker = roster.get("workers")?.get(job)?;
    let launch = worker.pointer("/dispatch/launch")?;
    if launch.get("mode")?.as_str()? != "resume" {
        return None;
    }
    let from = Path::new(launch.get("sessionId")?.as_str()?).file_stem()?.to_str()?.to_string();
    let own = worker.get("sessionId").and_then(Value::as_str);
    (!from.is_empty() && own != Some(from.as_str())).then_some(from)
}

/// A record newer than this may not name what it forked from yet, so it's read again.
const JOB_SETTLES_MS: i64 = 60_000;

/// What background jobs were forked from, from their records (`<home>/jobs/<job>/state.json`):
/// `forkParentSessionId` names the session whose conversation a job copied, for `/fork`,
/// `/background` and `claude --bg --resume <id> --fork-session`. A record names it as the job
/// starts and never changes it, and the records stay after a job ends, so each is read once.
#[derive(Default, Debug)]
pub struct Forks {
    /// Job folders read, so they aren't read again.
    read: HashSet<String>,
    /// By the job's session id, the one it was forked from.
    parents: HashMap<String, String>,
}

impl Forks {
    /// Reads the records of jobs not read yet. Returns whether it learned of a fork.
    pub fn refresh(&mut self, home: &Path, now: i64) -> bool {
        let Ok(entries) = std::fs::read_dir(home.join("jobs")) else { return false };
        let mut changed = false;
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if self.read.contains(&name) {
                continue;
            }
            let path = e.path().join("state.json");
            let Ok(bytes) = std::fs::read(&path) else { continue };
            let Some((id, parent)) = parse_job_state(&bytes) else { continue };
            match parent {
                Some(parent) => {
                    self.parents.insert(id, parent);
                    changed = true;
                }
                None if now - mtime_ms(&path).unwrap_or(0) < JOB_SETTLES_MS => continue,
                None => {}
            }
            self.read.insert(name);
        }
        changed
    }

    /// The session a job's session was forked from.
    pub fn parent_of(&self, id: &str) -> Option<&str> {
        self.parents.get(id).map(String::as_str)
    }
}

/// A job's session and the session it was forked from, from its record.
pub fn parse_job_state(bytes: &[u8]) -> Option<(String, Option<String>)> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Raw {
        session_id: Option<String>,
        fork_parent_session_id: Option<String>,
    }
    let raw: Raw = serde_json::from_slice(bytes).ok()?;
    let id = raw.session_id.filter(|id| !id.is_empty())?;
    let parent = raw.fork_parent_session_id.filter(|p| !p.is_empty() && p != &id);
    Some((id, parent))
}

/// Live sessions for one account. Entries whose process is gone, or whose pid now belongs
/// to a newer process, are dropped, and so are windows whose conversation moved to the
/// background: the job it moved to runs it now (`board::claude_moves`). So are spares, which
/// Claude Code's own lookups skip too: nothing has happened in one yet. And so are the copies
/// side conversations run (side.rs): a headless fork that keeps no record still lists itself
/// here while it runs (`kind` interactive, `entrypoint` sdk-cli), and it's no session.
pub fn read_registry(home: &Path) -> Vec<RegistryEntry> {
    let Ok(entries) = std::fs::read_dir(home.join("sessions")) else { return vec![] };
    let mut out = Vec::new();
    // Read once, and only when there's a job to look up.
    let mut roster: Option<Value> = None;
    for e in entries.flatten() {
        let path = e.path();
        if path.extension().and_then(|x| x.to_str()) != Some("json") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else { continue };
        let Some(mut entry) = parse_registry(&text) else { continue };
        if entry.parked_job.is_some() || entry.spare || !procs::pid_alive(entry.pid) || crate::side::runs(entry.pid) {
            continue;
        }
        if let (Some(proc_start), Some(started)) = (procs::proc_start_ms(entry.pid), entry.started_at) {
            if proc_start > started + 60_000 {
                continue;
            }
        }
        if let Some(job) = entry.job_id.as_deref() {
            let roster = roster.get_or_insert_with(|| std::fs::read(home.join("daemon/roster.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or(Value::Null));
            entry.resumes = resumed_from(roster, job);
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
    // One turn can write far more than the tail holds, so the prompt that started it may be
    // further back. A first read looks back for it once; later reads carry it forward.
    if fresh && info.last_prompt_at.is_none() {
        info.last_prompt_at = prompt_before(path, start);
    }
    state.offset = start + complete as u64;
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

/// How far back a transcript is read for the mode its latest prompt went in.
const MODE_SCAN: u64 = 8 * 1024 * 1024;

/// The permission mode a live session runs in, as its files have it: a Claude app session's is
/// in the app's record of it (`hostSessionId` names it), where the app keeps the mode you pick;
/// otherwise it's the mode its latest prompt went in, which its transcript writes on that line.
fn permission_mode(registry: Option<&Value>, transcripts: &[PathBuf]) -> Option<String> {
    let app = registry.and_then(|r| r.get("hostSessionId")).and_then(Value::as_str);
    if let Some(mode) = app.and_then(|id| crate::claudeapp::permission_mode(&crate::claudeapp::records_dir(), id)) {
        return Some(mode);
    }
    let path = transcripts.first()?;
    let end = std::fs::metadata(path).ok()?.len();
    let mut mode = None;
    scan_lines_back(path, end.saturating_sub(MODE_SCAN), end, |line| {
        mode = line_mode(line);
        mode.is_some()
    });
    mode
}

/// The mode a side conversation's copy runs in, given the session's (`permission_mode`): the
/// session's own where that lets it act without asking (it skips prompts, takes edits, or
/// leaves the asking to Claude Code's own reviewer), so a copy asked to change something can,
/// as the session could. Any other mode, and a session whose mode isn't written, gets the usual
/// one, where what would ask is refused: a copy in plan mode would plan, and nothing says a
/// session with no mode on record may act unasked.
/// The flag always goes, since without it a copy would start in the account's default mode.
fn side_mode(session: Option<&str>) -> &'static str {
    match session {
        Some("bypassPermissions") => "bypassPermissions",
        Some("acceptEdits") => "acceptEdits",
        Some("dontAsk") => "dontAsk",
        Some("auto") => "auto",
        _ => "default",
    }
}

/// A transcript line's own `permissionMode`. A quote inside a string is escaped, so the key
/// unescaped is never something a tool read or printed.
fn line_mode(line: &str) -> Option<String> {
    if !line.contains("\"permissionMode\":\"") {
        return None;
    }
    serde_json::from_str::<Value>(line).ok()?.get("permissionMode")?.as_str().map(str::to_string)
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
/* ---- The session panel's conversation, and answering permission prompts ---- */

/// One transcript line as conversation items (transcript.rs): a prompt you typed, the agent's
/// text and tool calls, a tool's result by the call's id, and notes.
pub fn conversation_items(v: &Value, at: i64) -> Vec<Item> {
    let flag = |k: &str| v.get(k).and_then(Value::as_bool) == Some(true);
    if flag("isSidechain") {
        return Vec::new();
    }
    match v.get("type").and_then(Value::as_str) {
        Some("user") => {
            if flag("isCompactSummary") {
                return vec![item(at, "note", "The conversation was compacted here")];
            }
            if let Some(peer) = line_peer(v) {
                return vec![peer_item(at, &peer)];
            }
            if let Some(t) = typed_text(v) {
                return vec![item(at, "you", t)];
            }
            if flag("isMeta") {
                return Vec::new();
            }
            let Some(parts) = v.pointer("/message/content").and_then(Value::as_array) else { return Vec::new() };
            let results: Vec<&Value> = parts.iter().filter(|p| p.get("type").and_then(Value::as_str) == Some("tool_result")).collect();
            // The line's `toolUseResult` is its one result's: for questions, what you answered.
            let answers = if results.len() == 1 { answers_of(v.pointer("/toolUseResult/answers")) } else { Default::default() };
            results
                .into_iter()
                .map(|p| Item {
                    id: p.get("tool_use_id").and_then(Value::as_str).map(String::from),
                    output: Some(clip_output(&text_of(p.get("content").unwrap_or(&Value::Null)))),
                    failed: p.get("is_error").and_then(Value::as_bool) == Some(true),
                    answers: answers.clone(),
                    ..item(at, "result", "")
                })
                .collect()
        }
        // A message you sent while it worked: Claude Code writes it when the turn takes it in, as
        // a `queued_command` attachment rather than a prompt. Background tasks' notifications
        // and other sessions' messages come the same way, wrapped in a tag.
        Some("attachment") => {
            if v.pointer("/attachment/type").and_then(Value::as_str) != Some("queued_command") {
                return Vec::new();
            }
            if let Some(peer) = line_peer(v) {
                return vec![peer_item(at, &peer)];
            }
            let t = text_of(v.pointer("/attachment/prompt").unwrap_or(&Value::Null));
            let t = t.trim();
            if t.is_empty() || t.starts_with('<') {
                return Vec::new();
            }
            vec![item(at, "you", t)]
        }
        Some("assistant") => {
            let Some(parts) = v.pointer("/message/content").and_then(Value::as_array) else { return Vec::new() };
            let mut out = Vec::new();
            for p in parts {
                match p.get("type").and_then(Value::as_str) {
                    Some("text") => {
                        let t = p.get("text").and_then(Value::as_str).unwrap_or("").trim();
                        if !t.is_empty() {
                            out.push(item(at, "agent", t));
                        }
                    }
                    Some("tool_use") => {
                        let name = p.get("name").and_then(Value::as_str).unwrap_or("Tool");
                        let input = p.get("input").cloned().unwrap_or(Value::Null);
                        let whole = claude_input(name, &input);
                        let shell = name == "Bash" && !whole.trim().is_empty() && kept_whole(&whole);
                        out.push(Item {
                            id: p.get("id").and_then(Value::as_str).map(String::from),
                            input: Some(clip_input(&whole)),
                            questions: if name == crate::asks::QUESTIONS { questions_of(&input) } else { Vec::new() },
                            shell,
                            // The line's folder is the shell's, where the command ran.
                            cwd: shell.then(|| v.get("cwd").and_then(Value::as_str).map(String::from)).flatten(),
                            ..item(at, "tool", tool_request(name, &input))
                        });
                    }
                    _ => {}
                }
            }
            out
        }
        _ => Vec::new(),
    }
}

/// A message from another session, as Claude Code records it: `origin: {kind: "peer", name,
/// body}` on the `user` line that started a turn with it, or on the `queued_command` attachment
/// by which a busy turn took it in. `body` is the text inside the message's envelope; a Claude
/// Code that doesn't keep it has only the envelope, in the line's own text.
struct Peer {
    name: String,
    body: String,
}

fn line_peer(v: &Value) -> Option<Peer> {
    let (origin, text) = match v.get("type").and_then(Value::as_str) {
        Some("user") => (v.get("origin")?, v.pointer("/message/content")),
        Some("attachment") => (v.pointer("/attachment/origin")?, v.pointer("/attachment/prompt")),
        _ => return None,
    };
    // A subagent's report back comes the same way; like a background task's notification, it
    // isn't a message anyone sent.
    if origin.get("kind").and_then(Value::as_str) != Some("peer") || origin.get("handback").and_then(Value::as_bool) == Some(true) {
        return None;
    }
    let body = match origin.get("body").and_then(Value::as_str) {
        Some(b) => b.to_string(),
        None => envelope_body(&text_of(text.unwrap_or(&Value::Null)))?,
    };
    // Older lines name the sender in `from`; newer ones say `unknown` there and use `name`.
    let name = [origin.get("name"), origin.get("from")].into_iter().flatten().filter_map(Value::as_str).find(|n| !n.is_empty() && *n != "unknown").unwrap_or("");
    Some(Peer { name: name.to_string(), body: body.replace("<\\/cross-session-message", "</cross-session-message") })
}

/// The text inside a `<cross-session-message …>` envelope.
fn envelope_body(text: &str) -> Option<String> {
    let start = text.find("<cross-session-message")?;
    let inside = start + text[start..].find('>')? + 1;
    let end = inside + text[inside..].find("</cross-session-message>")?;
    Some(text[inside..end].trim().to_string())
}

/// What you sent from Codealong, as the text you wrote. A Claude app session takes it as another
/// session's message (send.rs, `peer_content`), from Codealong, with a first line saying it's
/// from you, which an agent's message never starts with. It went with its attachments' full
/// paths; they read as references again, as they did in the box you sent it from.
fn sent_from_codealong(peer: &Peer) -> Option<String> {
    // The first line as it is now, and as Codealong worded it until 2026-09-28.
    const YOURS: [&str; 2] = [crate::send::FROM_YOU, "Your user typed this in Codealong's quick panel and sent it to this session:"];
    if peer.name != crate::send::FROM_CODEALONG {
        return None;
    }
    let body = peer.body.trim_start();
    let text = YOURS.iter().find_map(|first| body.strip_prefix(first))?.trim();
    (!text.is_empty()).then(|| crate::attachments::shorten(text))
}

/// Another session's message in the conversation: yours when Codealong sent it for you, else a
/// note of who it's from and what it says.
fn peer_item(at: i64, peer: &Peer) -> Item {
    if let Some(text) = sent_from_codealong(peer) {
        return item(at, "you", text);
    }
    let from = if peer.name.is_empty() { String::new() } else { format!(", “{}”", peer.name) };
    item(at, "note", format!("From another session{from}: {}", one_line(crate::send::without_byline(&peer.body), 200)))
}

/* ---- Prompts Claude Code took back ---- */

// Escape pressed before the agent has written anything back, with the input box empty, doesn't
// interrupt the turn as it does later: Claude Code (2.1.285's `auto_restore_cancel`) winds the
// conversation back to before the prompt and puts the prompt's text back in its input box, so
// the agent never sees it. Nothing in the transcript says so. The next line written goes on
// from the line before the prompt (its `parentUuid` is outside the prompt's own lines), and
// until one is, the session's registry says idle since after the prompt, with nothing back.

/// How far past a prompt the lines that settle it are read: its attachments (a session's first
/// prompt carries the whole system prompt), then a reply or whatever went on without it.
const FATE_SCAN: u64 = 2 * 1024 * 1024;

/// What became of a prompt, by the lines written after it.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Fate {
    /// The agent had it: it answered, or the turn was interrupted with the prompt kept.
    Kept,
    /// The conversation went on without it.
    TakenBack,
    /// Nothing after it settles it yet.
    Open,
}

/// What became of the prompt on the first of `lines`, and when it was sent; none when that line
/// isn't a prompt of yours.
fn fate_of<'a>(mut lines: impl Iterator<Item = &'a str>) -> Option<(Fate, i64)> {
    let first: Value = serde_json::from_str(lines.next()?).ok()?;
    if first.get("type").and_then(Value::as_str) != Some("user") {
        return None;
    }
    typed_text(&first)?;
    let at = first.get("timestamp").and_then(Value::as_str).and_then(parse_rfc3339_ms).unwrap_or(0);
    let mut ours: HashSet<String> = HashSet::from([first.get("uuid")?.as_str()?.to_string()]);
    for line in lines {
        // Titles, modes and file snapshots aren't in the conversation's chain.
        if !line.contains("\"parentUuid\"") {
            continue;
        }
        let Ok(v) = serde_json::from_str::<Value>(line) else { continue };
        if v.get("isSidechain").and_then(Value::as_bool) == Some(true) {
            continue;
        }
        let Some(id) = v.get("uuid").and_then(Value::as_str) else { continue };
        match v.get("parentUuid").and_then(Value::as_str) {
            // A chain that starts again, as compaction's does, isn't a rewind.
            None => return Some((Fate::Kept, at)),
            Some(parent) if !ours.contains(parent) => return Some((Fate::TakenBack, at)),
            Some(_) => {}
        }
        match v.get("type").and_then(Value::as_str) {
            Some("assistant") => return Some((Fate::Kept, at)),
            // Escape with text in the box interrupts instead, keeping the prompt.
            Some("user") if text_of(v.pointer("/message/content").unwrap_or(&Value::Null)).trim_start().starts_with("[Request interrupted") => return Some((Fate::Kept, at)),
            // A session's first prompt taken back leaves nothing before it to go on from, so the
            // next prompt follows on from its lines, often with its text again.
            Some("user") if typed_text(&v).is_some() => return Some((Fate::TakenBack, at)),
            _ => {}
        }
        ours.insert(id.to_string());
    }
    Some((Fate::Open, at))
}

/// Whether Claude Code took back the prompt whose line starts at `cursor`. `idle_since` is when
/// the session's registry says it last went idle, for a session whose Escape can take one back.
fn taken_back(path: &Path, cursor: u64, idle_since: Option<i64>) -> bool {
    let Ok(len) = std::fs::metadata(path).map(|m| m.len()) else { return false };
    let end = len.min(cursor + FATE_SCAN);
    let Some(text) = read_range(path, cursor, end) else { return false };
    // Whole lines only: the last may still be being written, or cut off by the scan's end.
    let Some(whole) = text.rfind('\n').map(|i| &text[..i]) else { return false };
    match fate_of(whole.lines()) {
        Some((Fate::TakenBack, _)) => true,
        Some((Fate::Open, at)) => end == len && idle_since.is_some_and(|t| t > at),
        _ => false,
    }
}

/// When a live session last went idle, by its registry, when it's idle now. Only Claude Code's
/// own screen takes a prompt back, so the Claude app's sessions aren't asked about.
fn idle_since(home: &Path, s: &crate::model::Session) -> Option<i64> {
    if s.surface == crate::model::Surface::App {
        return None;
    }
    let reg = crate::send::claude_registry(home, s)?;
    if reg.get("status").and_then(Value::as_str) != Some("idle") {
        return None;
    }
    reg.get("statusUpdatedAt").and_then(Value::as_i64)
}

/// Marks the prompts among `items` that Claude Code took back. Only a prompt with nothing from
/// the agent after it can have been, so the others aren't read again.
fn mark_taken_back(path: &Path, items: &mut [Item], idle_since: Option<i64>) {
    for i in 0..items.len() {
        if items[i].kind != "you" {
            continue;
        }
        let answered = items[i + 1..].iter().take_while(|it| it.kind != "you").any(|it| matches!(it.kind, "agent" | "tool"));
        if !answered && taken_back(path, items[i].cursor, idle_since) {
            items[i].withdrawn = true;
        }
    }
}

/// Where the last prompt before `after` starts, when nothing has come back for it before
/// `after`: it may have been taken back since, so a read of what's newer starts there again.
fn open_prompt_before(path: &Path, after: u64) -> Option<u64> {
    let page = crate::transcript::file_page(path, conversation_items, Some(after), 1).ok()?;
    page.items.last().filter(|it| it.kind == "you").map(|it| it.cursor)
}

/// Whether a live terminal session's latest prompt was taken back and nothing was sent since,
/// with the session idle: Claude Code may have put the prompt's text back in its input box,
/// where something typed would be added to it (send.rs clears the box first).
pub fn last_prompt_taken_back(account: &crate::config::AccountCfg, s: &crate::model::Session) -> bool {
    use crate::harness::Adapter;
    let Some(idle) = idle_since(&account.home_path(), s) else { return false };
    let Some(path) = ClaudeCode.records(account, s).into_iter().next() else { return false };
    let Ok(mut page) = crate::transcript::file_page(&path, conversation_items, None, 20) else { return false };
    let Some(last) = page.items.iter().rposition(|it| it.kind == "you") else { return false };
    mark_taken_back(&path, &mut page.items[last..], Some(idle));
    page.items[last].withdrawn
}

/// A Claude Code tool's input as you'd want to read it: the command, the file and the change,
/// else its arguments.
fn claude_input(name: &str, input: &Value) -> String {
    let s = |k: &str| input.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    match name {
        "Bash" => s("command"),
        "Read" | "Write" => s("file_path"),
        "Edit" => format!("{}\n- {}\n+ {}", s("file_path"), s("old_string").replace('\n', "\n- "), s("new_string").replace('\n', "\n+ ")),
        "Grep" | "Glob" => [s("pattern"), s("path")].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(" in "),
        // Plan mode's approval: the plan itself, in Markdown.
        "ExitPlanMode" => s("plan"),
        _ => serde_json::to_string_pretty(input).unwrap_or_default(),
    }
}


/// Claude Code (`harness::Adapter`): its transcript for the conversation, and its permission
/// prompt answered through the plugin's hook when the hook handed the app the request
/// (asks.rs), else by a keypress in the terminal it runs in, once its own record says it's
/// waiting on that prompt and its transcript that the call is the one you saw.
pub struct ClaudeCode;

impl crate::harness::Adapter for ClaudeCode {
    fn harness(&self) -> crate::model::Harness {
        crate::model::Harness::Claude
    }

    /// The keychain holds its sign-in (only whether it does is asked), and its state file the
    /// email.
    fn signed_in(&self, home: &Path) -> (bool, Option<String>) {
        let email = std::fs::read(state_file(home))
            .ok()
            .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
            .and_then(|v| v.pointer("/oauthAccount/emailAddress").and_then(Value::as_str).map(str::to_string));
        (crate::usage::claude_signed_in(home), email)
    }

    fn installed(&self, home: &Path, on_path: &dyn Fn(&str) -> bool) -> bool {
        on_path("claude") || crate::detect::is_executable(&home.join(".local/bin/claude")) || crate::detect::is_executable(&home.join(".claude/local/claude"))
    }

    fn folder_use(&self, dir: &Path) -> crate::harness::FolderUse {
        let projects = dir.join("projects");
        let mut paths = vec![dir.join("history.jsonl"), dir.join("sessions")];
        paths.extend(std::fs::read_dir(&projects).into_iter().flatten().flatten().map(|e| e.path()));
        crate::harness::FolderUse {
            has_data: projects.is_dir() || dir.join("history.jsonl").is_file(),
            last_used: paths.iter().filter_map(|p| mtime_ms(p)).max(),
            // Claude Code keeps sign-ins in the keychain on a Mac, and in the folder elsewhere.
            signed_in: dir.join(".credentials.json").is_file(),
        }
    }

    /// `<config dir>/skills`: a second account's config dir is its home here.
    fn skills_dir(&self, account: &crate::config::AccountCfg) -> Option<PathBuf> {
        Some(account.home_path().join("skills"))
    }

    fn acp_command(&self, account: &crate::config::AccountCfg) -> std::result::Result<(String, Vec<(String, String)>), String> {
        let program = crate::acp::package_program("claude-agent-acp", self.harness())?;
        Ok((program, vec![("CLAUDE_CONFIG_DIR".into(), account.home_path().to_string_lossy().into_owned())]))
    }

    /// The Claude app runs the account in `~/.claude`.
    fn places(&self, account: &crate::config::AccountCfg, home: &Path) -> std::result::Result<Vec<crate::start::Place>, String> {
        use crate::start::Place;
        Ok(if account.home_path() == home.join(".claude") {
            vec![Place::Terminal, Place::Background, Place::ClaudeApp, Place::Codealong]
        } else {
            vec![Place::Terminal, Place::Background, Place::Codealong]
        })
    }

    fn session_cwds(&self, home: &Path, since: i64) -> Vec<String> {
        crate::util::parallel_map(transcript_files(home, since, &HashSet::new()), |(p, _)| transcript_cwd(&p))
    }

    /// Its transcript, looked up by id under every project folder: for a job that has written
    /// none of its own yet, the one of the conversation it goes on from.
    fn records(&self, account: &crate::config::AccountCfg, s: &crate::model::Session) -> Vec<PathBuf> {
        let Ok(dirs) = std::fs::read_dir(account.home_path().join("projects")) else { return Vec::new() };
        let id = s.continues.as_deref().unwrap_or(&s.id);
        dirs.flatten().map(|d| d.path().join(format!("{id}.jsonl"))).filter(|p| p.is_file()).collect()
    }

    fn conversation(&self, ctx: &crate::harness::Ctx, s: &crate::model::Session, before: Option<u64>, limit: usize) -> anyhow::Result<Page> {
        match record(self, ctx, s) {
            Ok(path) => {
                let mut page = crate::transcript::file_page(&path, conversation_items, before, limit)?;
                mark_taken_back(&path, &mut page.items, idle_since(&ctx.account.home_path(), s));
                Ok(page)
            }
            // A live session writes its transcript at its first prompt.
            Err(_) if s.live => Ok(Page { items: Vec::new(), before: None, after: 0 }),
            Err(e) => Err(e),
        }
    }

    fn routes(&self, account: &crate::config::AccountCfg, s: &crate::model::Session) -> Vec<crate::send::Route> {
        let home = account.home_path();
        let registry = crate::send::claude_registry(&home, s);
        let mode = permission_mode(registry.as_ref(), &self.records(account, s));
        crate::send::claude_routes(s, registry.as_ref(), &home, crate::send::daemon_socket(&home).as_deref(), mode)
    }

    fn skips_permission_prompts(&self, ctx: &crate::harness::Ctx, s: &crate::model::Session) -> bool {
        let registry = crate::send::claude_registry(&ctx.account.home_path(), s);
        registry.is_some() && permission_mode(registry.as_ref(), &self.records(ctx.account, s)).as_deref() == Some("bypassPermissions")
    }

    fn conversation_after(&self, ctx: &crate::harness::Ctx, s: &crate::model::Session, after: u64) -> anyhow::Result<Page> {
        match record(self, ctx, s) {
            // A prompt with nothing back for it may have been taken back since it was read, so it
            // comes again; the panel puts it in place of the one it has, by its cursor.
            Ok(path) => {
                let from = open_prompt_before(&path, after).unwrap_or(after);
                let mut page = crate::transcript::file_newer(&path, conversation_items, from)?;
                mark_taken_back(&path, &mut page.items, idle_since(&ctx.account.home_path(), s));
                Ok(page)
            }
            Err(_) if s.live => Ok(Page { items: Vec::new(), before: None, after }),
            Err(e) => Err(e),
        }
    }

    fn pending(&self, ctx: &crate::harness::Ctx, s: &crate::model::Session) -> Option<crate::harness::Pending> {
        // A request the plugin's hook handed the app is answered through the hook, wherever the
        // session runs (asks.rs).
        if let Some(a) = crate::asks::of_session(&s.id) {
            return Some(asked_pending(&a));
        }
        // Questions the hook didn't hand over (the plugin isn't in the account, or the app started
        // after they were asked) still show here, to answer where the session runs.
        if let Some((call, _)) = waiting_on(self, ctx, s, "input needed").filter(|(c, _)| !c.questions.is_empty()) {
            return Some(crate::harness::Pending {
                id: call.id.clone().unwrap_or_default(),
                label: format!("Asks you · {}", one_line(&call.questions[0].question, 160)),
                input: None,
                cannot: Some("Codealong didn't get these as they were asked, so they're answered where the session runs.".into()),
                always: false,
                plan: false,
                always_rule: None,
                questions: call.questions,
            });
        }
        let (call, _) = waiting_call(self, ctx, s)?;
        let cannot = match crate::launch::terminal_kind(s.pid?) {
            Some(k) if k.presses_keys() => None,
            Some(k) => Some(format!("{} can't press a single key for Codealong. Open it to answer.", k.name())),
            None => Some(format!("It isn't in a terminal Codealong can press keys in ({}).", crate::launch::PRESSES_KEYS_IN)),
        };
        // Not every prompt has "Yes, and always allow" as its second choice, and a 2 pressed at
        // one without it is something else, so Always isn't offered in a terminal.
        let plan = call.text == PLAN;
        Some(crate::harness::Pending { id: call.id.clone().unwrap_or_default(), label: call.text, input: call.input, cannot, always: false, plan, always_rule: None, questions: Vec::new() })
    }

    fn answer(&self, ctx: &crate::harness::Ctx, s: &crate::model::Session, id: &str, answer: crate::harness::Answer) -> anyhow::Result<String> {
        let decision = match answer {
            crate::harness::Answer::Allow => crate::asks::Decision::Allow,
            crate::harness::Answer::Always => crate::asks::Decision::Always,
            crate::harness::Answer::Deny => crate::asks::Decision::Deny,
        };
        if let Some(r) = crate::asks::answer(id, decision) {
            let a = r?;
            let call = tool_request(&a.tool, &a.input);
            return Ok(match answer {
                crate::harness::Answer::Allow => format!("Allowed {call}."),
                crate::harness::Answer::Always => format!("Allowed {call}, and from now on."),
                crate::harness::Answer::Deny => format!("Denied {call}; it stops and waits for you."),
            });
        }
        if crate::asks::is_ask_id(id) {
            anyhow::bail!("It isn't waiting on that any more.");
        }
        let Some((call, pid)) = waiting_call(self, ctx, s) else {
            anyhow::bail!("It isn't waiting on a permission prompt any more.");
        };
        if call.id.as_deref() != Some(id) {
            anyhow::bail!("It's waiting on a different call now: {}. Look again before answering.", call.text);
        }
        // 1 is "Yes" in every prompt; Escape cancels it, which denies the call and ends the turn.
        let key = match answer {
            crate::harness::Answer::Allow => crate::launch::Key::Digit1,
            crate::harness::Answer::Deny => crate::launch::Key::Escape,
            crate::harness::Answer::Always => anyhow::bail!("Claude Code's prompt in a terminal is answered here with Allow or Deny only."),
        };
        let Some(host) = crate::launch::terminal_host(pid, Path::new(&s.cwd))? else {
            anyhow::bail!("It isn't in a terminal Codealong can press keys in ({}). Open it to answer.", crate::launch::PRESSES_KEYS_IN);
        };
        crate::launch::press_key(&host, key)?;
        Ok(match answer {
            crate::harness::Answer::Allow | crate::harness::Answer::Always => format!("Allowed {}.", call.text),
            crate::harness::Answer::Deny => format!("Denied {}; it stops and waits for you.", call.text),
        })
    }

    fn answer_questions(&self, _ctx: &crate::harness::Ctx, s: &crate::model::Session, id: &str, answers: &Value) -> anyhow::Result<String> {
        let Some(asked) = crate::asks::of_session(&s.id).filter(|a| a.id == id) else {
            anyhow::bail!("It isn't waiting on that any more.");
        };
        let answers = picked(&questions_of(&asked.input), answers)?;
        let said = answers.iter().map(|(q, a)| format!("{} → {}", one_line(q, 80), a.as_str().map(String::from).unwrap_or_else(|| a.as_array().map(|l| l.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(", ")).unwrap_or_default()))).collect::<Vec<_>>().join("; ");
        match crate::asks::answer(id, crate::asks::Decision::Answers(Value::Object(answers))) {
            Some(r) => r.map(|_| format!("Answered: {said}.")),
            None => anyhow::bail!("It isn't waiting on that any more."),
        }
    }

    fn open_calls(&self, ctx: &crate::harness::Ctx, s: &crate::model::Session) -> Vec<String> {
        record(self, ctx, s).map(|p| open_calls(&p)).unwrap_or_default()
    }

    /// Its Bash tool's shells, background ones included, are the session process's children.
    fn runs_its_commands(&self) -> bool {
        true
    }

    fn fork(&self, ctx: &crate::harness::Ctx, s: &crate::model::Session) -> anyhow::Result<crate::harness::Fork> {
        // A job with no transcript of its own yet has the conversation it goes on from, so far.
        Ok(fork_as(ctx.account, s.continues.as_deref().unwrap_or(&s.id), &crate::start::new_session_id()))
    }

    /// A background job under the account's service, whichever way the session runs: Claude
    /// Code copies the conversation from its transcript (`start::fork_command`).
    fn fork_background(&self, ctx: &crate::harness::Ctx, s: &crate::model::Session, cwd: &Path, prompt: &str) -> anyhow::Result<Option<String>> {
        let command = crate::start::fork_command(ctx.account, s.continues.as_deref().unwrap_or(&s.id), prompt);
        let (_, id) = crate::start::start_background(ctx.account, cwd, &command)?;
        Ok(id.map(|id| format!("claude:{}:{id}", ctx.account.name)))
    }

    /// Claude Code run headless on a fork of the session that keeps no record (checked on
    /// 2.1.286, 2026-09-30): `--resume <id> --fork-session --no-session-persistence` reads the
    /// conversation as far as it's written and writes no transcript. It runs in the session's
    /// own permission mode (`side_mode`), so it may do what the session may without asking, and
    /// with nobody to ask, whatever would ask is refused: in the usual mode an edit or a command
    /// that changes things, while reads and commands that only look go ahead. Nothing else
    /// about the run is changed (its tools, its system prompt, the account's usual model), so
    /// its first question reads the session's prompt cache rather than paying for the whole
    /// conversation again: a 35,000-token haiku session's copy read 35,120 tokens from the cache
    /// and wrote 594. A session on another model than the account's usual one gets its answers
    /// from the usual one: a transcript names the model without its context size, so the
    /// session's own can't be asked for by name.
    fn side(&self, ctx: &crate::harness::Ctx, s: &crate::model::Session, _cwd: &Path) -> anyhow::Result<crate::side::Recipe> {
        // A job with no transcript of its own yet has the conversation it goes on from, so far.
        let id = s.continues.as_deref().unwrap_or(&s.id);
        let home = ctx.account.home_path();
        let registry = crate::send::claude_registry(&home, s);
        let mode = side_mode(permission_mode(registry.as_ref(), &self.records(ctx.account, s)).as_deref());
        // `~/.claude` is where Claude Code looks with no folder named, whatever the app was
        // started with (install.rs runs `claude plugin` the same way).
        let usual = home == crate::util::home().join(".claude");
        let args = ["-p", "--input-format", "stream-json", "--output-format", "stream-json", "--verbose", "--include-partial-messages", "--resume", id, "--fork-session", "--no-session-persistence", "--permission-mode", mode];
        Ok(crate::side::Recipe {
            program: "claude".into(),
            args: args.iter().map(|a| a.to_string()).collect(),
            env: if usual { Vec::new() } else { vec![("CLAUDE_CONFIG_DIR".into(), home.to_string_lossy().into_owned())] },
            // And it's no child of whatever session the app may have been started from.
            unset: [usual.then_some("CLAUDE_CONFIG_DIR"), Some("CLAUDE_CODE_SESSION_ID"), Some("CLAUDE_CODE_CHILD_SESSION"), Some("CODEALONG_SESSION")].into_iter().flatten().collect(),
            dialect: Box::new(SideCopy::default()),
        })
    }

    /// A background job stops under its account's service, which keeps the conversation
    /// (`claude stop <job>`). Run through a login shell, as a start is: the account's command
    /// can be an alias.
    fn stop(&self, ctx: &crate::harness::Ctx, s: &crate::model::Session) -> anyhow::Result<String> {
        let Some(command) = s.attach_command.as_deref().and_then(|attach| stop_command(ctx.account, attach)) else {
            anyhow::bail!("It isn't a background job, so it closes with its terminal.");
        };
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
        if let Err(e) = procs::run(&shell, &["-ilc", &command], None, std::time::Duration::from_secs(30)) {
            // Claude Code's own reason comes last, after whatever the shell's startup printed.
            let said = e.to_string();
            let why = said.lines().map(str::trim).filter(|l| !l.is_empty()).last().unwrap_or("it failed").to_string();
            anyhow::bail!("Couldn't stop the job: {why}");
        }
        Ok(format!("Stopped “{}”. Its conversation is kept and resumes like any other.", s.title))
    }

    /// Names it as Claude Code's `/rename` does (`name_transcript`). A running session reads the
    /// name back the next time it reads the end of its transcript, after about 32 KB of its own
    /// writes, at a compaction or as it exits, and goes by it from then on, its terminal's title
    /// too (2.1.285's code, which says other processes may write it). A job with no conversation
    /// of its own yet gets it in the transcript it goes on from as well, which the board reads
    /// for it until then.
    fn rename(&self, ctx: &crate::harness::Ctx, s: &crate::model::Session, name: &str) -> anyhow::Result<Vec<PathBuf>> {
        let dirs: Vec<PathBuf> = std::fs::read_dir(ctx.account.home_path().join("projects")).map(|d| d.flatten().map(|e| e.path()).collect()).unwrap_or_default();
        let ids = std::iter::once(s.id.as_str()).chain(s.continues.as_deref());
        let paths: Vec<PathBuf> = ids.flat_map(|id| dirs.iter().map(move |d| d.join(format!("{id}.jsonl")))).filter(|p| p.is_file()).collect();
        if paths.is_empty() {
            anyhow::bail!("“{}” has no transcript yet to keep a name in: Claude Code writes one at its first prompt.", s.title);
        }
        for p in &paths {
            name_transcript(p, name)?;
        }
        Ok(paths)
    }

    /// Escape, Claude Code's own interrupt: pressed in the terminal it runs in, or, for a
    /// background job, through its account's service.
    fn interrupts(&self, ctx: &crate::harness::Ctx, s: &crate::model::Session) -> bool {
        if s.surface != crate::model::Surface::Cli {
            return false;
        }
        if s.attach_command.is_some() {
            return crate::send::daemon_socket(&ctx.account.home_path()).is_some();
        }
        s.pid.and_then(crate::launch::terminal_kind).is_some_and(|k| k.presses_keys())
    }

    /// Presses Escape only while its own record says a turn is running: busy, or waiting on a
    /// prompt, which Escape answers No to, ending the turn. With nothing running, an Escape would
    /// go to whatever the terminal shows then (two in a row open Claude Code's rewind).
    fn interrupt(&self, ctx: &crate::harness::Ctx, s: &crate::model::Session) -> anyhow::Result<String> {
        let home = ctx.account.home_path();
        let registry = crate::send::claude_registry(&home, s).ok_or_else(|| anyhow::anyhow!("It isn't running any more."))?;
        let status = registry.get("status").and_then(Value::as_str).unwrap_or_default();
        if !matches!(status, "busy" | "waiting") {
            anyhow::bail!("Its turn has already ended.");
        }
        if registry.get("kind").and_then(Value::as_str) == Some("bg") {
            let job = registry.get("jobId").and_then(Value::as_str).ok_or_else(|| anyhow::anyhow!("Claude Code didn't record which job it is."))?;
            let socket = crate::send::daemon_socket(&home).ok_or_else(|| anyhow::anyhow!("Its account's background service isn't running."))?;
            crate::send::daemon_press_escape(&socket, &home.join("daemon/control.key"), job)?;
        } else {
            let pid = s.pid.ok_or_else(|| anyhow::anyhow!("It isn't running any more."))?;
            let Some(host) = crate::launch::terminal_host(pid, Path::new(&s.cwd))? else {
                anyhow::bail!("It isn't in a terminal Codealong can press keys in ({}). Open it to stop it there.", crate::launch::PRESSES_KEYS_IN);
            };
            crate::launch::press_key(&host, crate::launch::Key::Escape)?;
        }
        Ok(format!("Stopped “{}”.", s.title))
    }

    /// The endpoint behind its `/usage`, which turns away calls that come often.
    fn polls_usage(&self) -> bool {
        true
    }

    fn read_usage(&self, account: &AccountCfg, now: i64) -> Usage {
        crate::usage::claude(account, now)
    }

    fn reader(&self, _wake: crate::harness::Wake) -> Box<dyn crate::harness::Reader> {
        Box::new(Reader::default())
    }
}

/* ---- Reading sessions for the board ---- */

/// How often the commands sessions left running in the background are looked at.
const BACKGROUND_EVERY: Duration = Duration::from_secs(60);
/// How often the Claude app's records are read again, for what you archived there.
const APP_RECORDS_EVERY: Duration = Duration::from_secs(30);

/// A command a session left running in the background, as last seen.
struct BackgroundWatch {
    /// The registry's status time: a new turn that leaves a command running starts afresh.
    since: i64,
    tree: procs::CommandTree,
    /// When its processes were last seen starting a process or using CPU.
    active_at: i64,
}

/// Claude Code's sessions for the board (`harness::Reader`): each account's live registry and
/// transcripts as last read, and what a look at processes said about the sessions that have
/// gone quiet.
#[derive(Default)]
pub struct Reader {
    registry: HashMap<String, Vec<RegistryEntry>>,
    /// What each account's background jobs were forked from, read from their records.
    forks: HashMap<String, Forks>,
    transcripts: HashMap<PathBuf, (String, Parsed)>,
    /// How far each subagent log has been handed to the engine to check.
    subagent_offsets: HashMap<PathBuf, u64>,
    /// The Claude app's session records, for what you archived there.
    app_records: crate::claudeapp::Records,
    app_records_at: Option<Instant>,
    /// (pid, status time) → the command a stuck session's shell is running.
    stuck: Mutex<HashMap<(u32, i64), Option<String>>>,
    /// Sessions whose turn left a command running in the background, by pid: when their
    /// processes last did something, from a look at them every minute.
    background: HashMap<u32, BackgroundWatch>,
    background_at: Option<Instant>,
}

impl Reader {
    fn refresh_registry(&mut self, a: &AccountCfg, now: i64) -> bool {
        let home = a.home_path();
        let fresh = read_registry(&home);
        let changed = self.registry.get(&a.name) != Some(&fresh);
        self.registry.insert(a.name.clone(), fresh);
        // A new job shows in the registry as it starts, and its record says what it's a fork of.
        let forked = self.forks.entry(a.name.clone()).or_default().refresh(&home, now);
        changed || forked
    }

    /// Looks at what the commands sessions left running in the background are doing, once a
    /// minute and only while there are some: one `ps` for all of them.
    fn watch_background(&mut self, now: i64) {
        if self.background_at.is_some_and(|t| t.elapsed() < BACKGROUND_EVERY) {
            return;
        }
        self.background_at = Some(Instant::now());
        let shells: Vec<(u32, i64)> = self.registry.values().flatten().filter(|r| r.status == "shell").map(|r| (r.pid, r.status_updated_at.unwrap_or(0))).collect();
        self.background.retain(|pid, _| shells.iter().any(|(p, _)| p == pid));
        if shells.is_empty() {
            return;
        }
        let pids: Vec<u32> = shells.iter().map(|(p, _)| *p).collect();
        let mut trees = procs::command_trees(&pids);
        for (pid, since) in shells {
            let Some(tree) = trees.remove(&pid) else { continue };
            match self.background.get_mut(&pid) {
                Some(w) if w.since == since => {
                    if tree.active_since(&w.tree) {
                        w.active_at = now;
                    }
                    w.tree = tree;
                }
                _ => {
                    // Seen for the first time, perhaps long after the turn ended (the app just
                    // started): give it a few looks to show it's doing something.
                    let active_at = since.max(now - crate::board::BACKGROUND_QUIET_MS + 3 * BACKGROUND_EVERY.as_millis() as i64);
                    self.background.insert(pid, BackgroundWatch { since, tree, active_at });
                }
            }
        }
    }

    /// What a stuck session's shell is running, read once per stall.
    fn stuck_command(&self, r: &RegistryEntry) -> Option<String> {
        let mut cache = self.stuck.lock().ok()?;
        cache.entry((r.pid, r.status_updated_at.unwrap_or(0))).or_insert_with(|| procs::running_command(r.pid)).clone()
    }
}

impl crate::harness::Reader for Reader {
    fn harness(&self) -> Harness {
        Harness::Claude
    }

    fn scan(&mut self, accounts: &[AccountCfg], look: &Look) {
        self.registry.clear();
        self.transcripts.clear();
        if !accounts.is_empty() {
            self.app_records.refresh(&crate::claudeapp::records_dir());
            self.app_records_at = Some(Instant::now());
        }
        for a in accounts {
            let home = a.home_path();
            self.registry.insert(a.name.clone(), read_registry(&home));
            self.forks.entry(a.name.clone()).or_default().refresh(&home, look.now);
            let files = transcript_files(&home, look.since, &look.seen.pinned_ids("claude", &a.name));
            for (path, parsed) in parallel_map(files, |(p, m)| parse_transcript(&p, m, None).map(|r| (p, r))) {
                self.transcripts.insert(path, (a.name.clone(), parsed));
            }
        }
    }

    fn watch(&self, account: &AccountCfg) -> Vec<(PathBuf, bool)> {
        let home = account.home_path();
        vec![(home.join("sessions"), false), (home.join("projects"), true)]
    }

    fn owns(&self, account: &AccountCfg, path: &Path) -> bool {
        let home = account.home_path();
        path.starts_with(home.join("sessions")) || path.starts_with(home.join("projects"))
    }

    fn apply(&mut self, account: &AccountCfg, paths: &[PathBuf], look: &Look) -> Applied {
        let home = account.home_path();
        let (sessions, projects) = (home.join("sessions"), home.join("projects"));
        let mut out = Applied::default();
        if paths.iter().any(|p| p.starts_with(&sessions)) && self.refresh_registry(account, look.now) {
            out.dirty = true;
            out.added = true;
        }
        for p in paths.iter().filter(|p| p.starts_with(&projects) && p.extension().and_then(|x| x.to_str()) == Some("jsonl")) {
            if p.parent().and_then(Path::parent) == Some(projects.as_path()) {
                let prev = self.transcripts.remove(p).map(|(_, parsed)| parsed);
                let is_new = prev.is_none();
                let from = prev.as_ref().map(|(_, _, st)| st.offset);
                if let Some(parsed) = mtime_ms(p).and_then(|m| parse_transcript(p, m, prev)) {
                    if let Some(from) = from {
                        out.lines.push(NewLines { path: p.clone(), from, to: parsed.2.offset, key: format!("claude:{}:{}", account.name, parsed.1.session_id) });
                    }
                    self.transcripts.insert(p.clone(), (account.name.clone(), parsed));
                }
                out.added |= is_new;
                out.dirty = true;
            } else if p.parent().map(file_name).as_deref() == Some("subagents") {
                // `<session>/subagents/agent-*.jsonl`: a subagent's lines count for its session.
                let Some(id) = p.parent().and_then(Path::parent).map(file_name) else { continue };
                let Ok(len) = std::fs::metadata(p).map(|m| m.len()) else { continue };
                // A log first seen now is new, or its earlier lines were read by the last full search.
                let from = self.subagent_offsets.get(p).copied().unwrap_or(len.saturating_sub(1 << 20));
                out.lines.push(NewLines { path: p.clone(), from, to: len, key: format!("claude:{}:{id}", account.name) });
                self.subagent_offsets.insert(p.clone(), len);
            }
        }
        out
    }

    fn tick(&mut self, accounts: &[AccountCfg], look: &Look) -> Applied {
        let mut out = Applied::default();
        for a in accounts {
            out.dirty |= self.refresh_registry(a, look.now);
        }
        self.watch_background(look.now);
        if !accounts.is_empty() && self.app_records_at.map_or(true, |t| t.elapsed() >= APP_RECORDS_EVERY) {
            self.app_records_at = Some(Instant::now());
            out.dirty |= self.app_records.refresh(&crate::claudeapp::records_dir());
        }
        out
    }

    fn sessions(&self, account: &AccountCfg, look: &Look) -> Vec<Session> {
        use crate::board;
        let (a, now) = (account, look.now);
        let mut out = Vec::new();
        let app_archived = self.app_records.archived();
        let mut by_id: HashMap<&str, (Option<&RegistryEntry>, Option<(&Path, &TranscriptInfo)>)> = HashMap::new();
        for r in self.registry.get(&a.name).map(|v| v.as_slice()).unwrap_or(&[]) {
            by_id.entry(r.session_id.as_str()).or_default().0 = Some(r);
        }
        for (path, (acct, (_, t, _))) in &self.transcripts {
            if acct == &a.name {
                let slot = by_id.entry(t.session_id.as_str()).or_default();
                if slot.1.map(|(_, old)| old.last_ts < t.last_ts).unwrap_or(true) {
                    slot.1 = Some((path.as_path(), t));
                }
            }
        }
        let live: HashSet<&str> = by_id.iter().filter(|(_, (r, _))| r.is_some()).map(|(id, _)| *id).collect();
        let said: HashMap<&str, &TranscriptInfo> = by_id.iter().filter_map(|(id, (_, t))| t.map(|(_, t)| (*id, t))).collect();
        let moves = board::claude_moves(&live, &said);
        for (&id, &(reg, tr)) in &by_id {
            // Its conversation went on in a job, which shows it.
            if moves.moved.contains_key(id) {
                continue;
            }
            let key = format!("claude:{}:{}", a.name, id);
            // A job that hasn't written its own transcript yet shows the one it goes on
            // from: a window's conversation it took over, or one it's a fork of.
            let resumes = reg.and_then(|r| r.resumes.as_ref()).filter(|_| tr.is_none());
            let from = moves.from.get(id).or(resumes).and_then(|f| Some((f, by_id.get(f.as_str())?.1?)));
            let tr = tr.or(from.map(|(_, t)| t));
            let moved = reg.zip(from).map(|(r, (_, (_, t)))| board::moved_registry(r, t));
            let reg = moved.as_ref().or(reg);
            let seen_at = from.map(|(f, _)| look.seen.seen_at(&format!("claude:{}:{f}", a.name))).unwrap_or(0).max(look.seen.seen_at(&key));
            let info = tr.map(|(_, t)| t);
            // Only a busy session that's gone quiet costs a look outside its transcript,
            // and one whose turn left a command running in the background (read once
            // per status change).
            let background = reg.filter(|r| r.status == "shell");
            let (subagent_at, running) = match board::claude_busy_since(reg, info) {
                _ if background.is_some() => (None, background.and_then(|r| self.stuck_command(r))),
                Some(since) if now - since > board::CLAUDE_STALL_MS => {
                    let sub = tr.and_then(|(p, _)| subagent_activity(p, id));
                    let quiet = now - since.max(sub.unwrap_or(0)) > board::CLAUDE_STALL_MS;
                    (sub, if quiet { reg.and_then(|r| self.stuck_command(r)) } else { None })
                }
                _ => (None, None),
            };
            let background_active_at = background.and_then(|r| self.background.get(&r.pid)).filter(|w| Some(w.since) == background.and_then(|r| r.status_updated_at)).map(|w| w.active_at);
            let quiet = board::Quiet { subagent_at, running: running.as_deref(), background_active_at };
            if let Some(mut s) = board::claude_session(a, reg, info, seen_at, now, &quiet) {
                // Archived here until it's active again, or archived in the Claude app.
                s.archived = look.archived_here(&key, s.last_activity) || app_archived.contains(id);
                s.continues = from.map(|(f, _)| f.clone());
                // A job's record says what it's a fork of; the roster says for a
                // running one whose record hasn't been read yet.
                let parent = self.forks.get(&a.name).and_then(|f| f.parent_of(id)).or(reg.and_then(|r| r.resumes.as_deref()));
                s.origin = parent.filter(|p| *p != id).map(|p| SessionOrigin { key: format!("claude:{}:{p}", a.name), how: OriginHow::Fork });
                out.push(s);
            }
        }
        out
    }

    fn cwds(&self) -> Vec<String> {
        let live = self.registry.values().flatten().map(|e| e.cwd.clone());
        live.chain(self.transcripts.values().map(|(_, (_, t, _))| t.cwd.clone())).collect()
    }

    fn logs(&self) -> Vec<(PathBuf, String)> {
        let mut files = Vec::new();
        for (p, (a, (_, info, _))) in &self.transcripts {
            let key = format!("claude:{}:{}", a, info.session_id);
            // A subagent works where its session sent it, so its log counts for the session.
            files.extend(subagent_logs(p, &info.session_id).into_iter().map(|s| (s, key.clone())));
            files.push((p.clone(), key));
        }
        files
    }
}

/// The newest write to a session's subagent logs (`<transcript dir>/<id>/subagents`).
fn subagent_activity(transcript: &Path, session_id: &str) -> Option<i64> {
    let dir = transcript.parent()?.join(session_id).join("subagents");
    std::fs::read_dir(dir).ok()?.flatten().filter_map(|e| mtime_ms(&e.path())).max()
}

/// A session's subagent logs.
fn subagent_logs(transcript: &Path, session_id: &str) -> Vec<PathBuf> {
    let Some(dir) = transcript.parent().map(|d| d.join(session_id).join("subagents")) else { return vec![] };
    let Ok(entries) = std::fs::read_dir(dir) else { return vec![] };
    entries.flatten().map(|e| e.path()).filter(|p| p.extension().and_then(|x| x.to_str()) == Some("jsonl")).collect()
}

/// `<account command> stop <job>` for a background job, its job read from its attach command.
/// None for anything else, or a job id that isn't plain letters and digits, since it's typed
/// into a shell.
fn stop_command(account: &crate::config::AccountCfg, attach: &str) -> Option<String> {
    let (_, job) = attach.rsplit_once(" attach ")?;
    (!job.is_empty() && job.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')).then(|| format!("{} stop {job}", account.command()))
}

/// Names a session where Claude Code keeps its name: a `custom-title` line at the end of its
/// transcript, keys in Claude Code's order, and `<session>/custom-title.json` beside it, which
/// `/resume` lists sessions by.
pub fn name_transcript(transcript: &Path, name: &str) -> anyhow::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
    let (Some(dir), Some(id)) = (transcript.parent(), transcript.file_stem().and_then(|s| s.to_str())) else {
        anyhow::bail!("{} isn't a transcript.", transcript.display());
    };
    let line = format!(r#"{{"type":"custom-title","customTitle":{},"sessionId":{}}}"#, serde_json::to_string(name)?, serde_json::to_string(id)?);
    // A line cut off midway is ended first, as Claude Code does before it appends, so the name
    // isn't lost inside it. Appending leaves Claude Code's own writes whole.
    let cut = ends_mid_line(transcript);
    std::fs::OpenOptions::new().append(true).open(transcript)?.write_all(format!("{}{line}\n", if cut { "\n" } else { "" }).as_bytes())?;
    let folder = dir.join(id);
    std::fs::DirBuilder::new().recursive(true).mode(0o700).create(&folder)?;
    let part = folder.join(format!(".custom-title.json.{}", std::process::id()));
    std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&part)?.write_all(serde_json::json!({ "customTitle": name }).to_string().as_bytes())?;
    std::fs::rename(&part, folder.join("custom-title.json"))?;
    Ok(())
}

/// The file's last line has no newline yet.
fn ends_mid_line(path: &Path) -> bool {
    let Ok(mut f) = File::open(path) else { return false };
    let mut last = [0u8; 1];
    f.seek(SeekFrom::End(-1)).is_ok() && f.read_exact(&mut last).is_ok() && last[0] != b'\n'
}

/// Forks session `id` into a new one named `new_id`: `--fork-session` copies the transcript, and
/// takes `--session-id` for the copy's ID (resuming takes it only with `--fork-session`), so the
/// copy's key is known before it's written, as a start's is.
fn fork_as(account: &crate::config::AccountCfg, id: &str, new_id: &str) -> crate::harness::Fork {
    let key = format!("claude:{}:{new_id}", account.name);
    crate::harness::Fork {
        input: format!("{} --resume {id} --fork-session --session-id {new_id}\n", account.command()),
        env: vec![(crate::start::SESSION_VAR.to_string(), key.clone())],
        key: Some(key),
    }
}

/// A permission request the plugin's hook holds in the app, as the panel shows it: a call to
/// allow, or questions to answer.
fn asked_pending(a: &crate::asks::Asked) -> crate::harness::Pending {
    let questions = if a.tool == crate::asks::QUESTIONS { questions_of(&a.input) } else { Vec::new() };
    crate::harness::Pending {
        id: a.id.clone(),
        label: asked_label(a),
        input: questions.is_empty().then(|| clip_input(&claude_input(&a.tool, &a.input))),
        cannot: None,
        always: questions.is_empty() && !a.suggestions.is_empty(),
        plan: false,
        always_rule: always_text(&a.suggestions),
        questions,
    }
}

/// A held request as a line: the call, or what it asks you.
pub fn asked_label(a: &crate::asks::Asked) -> String {
    match questions_of(&a.input).first().filter(|_| a.tool == crate::asks::QUESTIONS) {
        Some(q) => format!("Asks you · {}", one_line(&q.question, 160)),
        None => tool_request(&a.tool, &a.input),
    }
}

/// The questions in an `AskUserQuestion` call's input.
fn questions_of(input: &Value) -> Vec<crate::harness::Question> {
    let text = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).map(str::trim).filter(|t| !t.is_empty()).map(String::from);
    input
        .get("questions")
        .and_then(Value::as_array)
        .map(|qs| {
            qs.iter()
                .filter_map(|q| {
                    let options = q.get("options").and_then(Value::as_array).map(|o| o.iter().filter_map(|c| Some(codealong_peer::api::Choice { label: text(c, "label")?, description: text(c, "description") })).collect()).unwrap_or_default();
                    Some(crate::harness::Question { question: text(q, "question")?, header: text(q, "header"), options, multi_select: q.get("multiSelect").and_then(Value::as_bool) == Some(true) })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Your answers as Claude Code records them on the result (`toolUseResult.answers`): each
/// question's text to a label or words of your own, or a list where it took several. Older
/// versions joined several into one string with commas, which is kept as it is.
fn answers_of(v: Option<&Value>) -> std::collections::BTreeMap<String, Vec<String>> {
    let Some(map) = v.and_then(Value::as_object) else { return Default::default() };
    map.iter()
        .filter_map(|(q, a)| {
            let list: Vec<String> = match a {
                Value::String(t) => vec![t.clone()],
                Value::Array(l) => l.iter().filter_map(Value::as_str).map(String::from).collect(),
                _ => Vec::new(),
            };
            let list: Vec<String> = list.into_iter().map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect();
            (!list.is_empty()).then(|| (q.clone(), list))
        })
        .collect()
}

/// Your answers to the questions, as Claude Code takes them: each question answered by its text,
/// with a label or words of your own, or a list of them where it takes several. Empty answers
/// and questions it didn't ask are left out; none at all is refused.
fn picked(questions: &[crate::harness::Question], answers: &Value) -> anyhow::Result<serde_json::Map<String, Value>> {
    let mut out = serde_json::Map::new();
    for q in questions {
        let words = |v: &Value| v.as_str().map(str::trim).filter(|t| !t.is_empty()).map(|t| Value::String(t.to_string()));
        let answer = match answers.get(&q.question) {
            Some(Value::Array(list)) if q.multi_select => Some(list.iter().filter_map(words).collect::<Vec<_>>()).filter(|l| !l.is_empty()).map(Value::Array),
            Some(Value::Array(list)) => list.iter().find_map(words),
            Some(v) => words(v),
            None => None,
        };
        if let Some(a) = answer {
            out.insert(q.question.clone(), a);
        }
    }
    if out.is_empty() {
        anyhow::bail!("Pick an answer first.");
    }
    Ok(out)
}

/// What answering Always adds, in a sentence: Claude Code's own suggestions for the call, the
/// ones its "Yes, and don't ask again" applies (permission updates: rules, a mode, folders).
pub fn always_text(suggestions: &[Value]) -> Option<String> {
    let parts: Vec<String> = suggestions
        .iter()
        .filter_map(|u| {
            let s = |k: &str| u.get(k).and_then(Value::as_str);
            let place = match s("destination") {
                Some("session") => " for this session",
                Some("localSettings") => " in this project, for you",
                Some("projectSettings") => " in this project, for everyone",
                Some("userSettings") => " everywhere, for you",
                _ => "",
            };
            let list = |k: &str, f: &dyn Fn(&Value) -> Option<String>| -> Vec<String> { u.get(k).and_then(Value::as_array).map(|a| a.iter().filter_map(f).collect()).unwrap_or_default() };
            match s("type")? {
                "addRules" if s("behavior") == Some("allow") => {
                    let rules = list("rules", &|r| {
                        let tool = r.get("toolName")?.as_str()?;
                        Some(code(&match r.get("ruleContent").and_then(Value::as_str) {
                            Some(c) => format!("{tool}({c})"),
                            None => tool.to_string(),
                        }))
                    });
                    (!rules.is_empty()).then(|| format!("Allows {}{place}", rules.join(", ")))
                }
                "addDirectories" => {
                    let dirs = list("directories", &|d| d.as_str().map(|p| code(&crate::util::tilde(Path::new(p)))));
                    (!dirs.is_empty()).then(|| format!("Allows working in {}{place}", dirs.join(", ")))
                }
                "setMode" => {
                    let mode = match s("mode")? {
                        "acceptEdits" => "accepting edits",
                        "default" => "asking first",
                        "plan" => "plan mode",
                        "dontAsk" => "not asking",
                        "auto" => "auto mode",
                        "bypassPermissions" => "skipping permission prompts",
                        _ => return None,
                    };
                    Some(format!("Switches to {mode}{place}"))
                }
                _ => None,
            }
        })
        .collect();
    (!parts.is_empty()).then(|| format!("{}.", parts.join("; ")))
}

/// The inputs of the calls at the end of a transcript with no result yet: Claude Code writes a
/// call there before running it.
fn open_calls(path: &Path) -> Vec<String> {
    let Ok(page) = crate::transcript::file_page(path, conversation_items, None, 20) else { return Vec::new() };
    crate::transcript::open_tools(&page.items).into_iter().filter_map(|i| i.input.clone()).collect()
}

fn record(a: &ClaudeCode, ctx: &crate::harness::Ctx, s: &crate::model::Session) -> anyhow::Result<PathBuf> {
    use crate::harness::Adapter;
    a.records(ctx.account, s).into_iter().next().ok_or_else(|| anyhow::anyhow!("Its transcript isn't on this Mac any more."))
}

/// The tool call a session's permission prompt is about, with its process: only while Claude
/// Code's own record of the session says it's waiting on a permission prompt, and the call is
/// the last one in its transcript with no result yet.
fn waiting_call(a: &ClaudeCode, ctx: &crate::harness::Ctx, s: &crate::model::Session) -> Option<(Item, u32)> {
    waiting_on(a, ctx, s, "permission prompt")
}

/// The call a live session's prompt is up for, when its registry says it waits `for` that
/// (a "permission prompt", or "input needed" for questions), and its process.
fn waiting_on(a: &ClaudeCode, ctx: &crate::harness::Ctx, s: &crate::model::Session, what: &str) -> Option<(Item, u32)> {
    let pid = s.pid?;
    let reg: Value = serde_json::from_str(&std::fs::read_to_string(ctx.account.home_path().join("sessions").join(format!("{pid}.json"))).ok()?).ok()?;
    let field = |k: &str| reg.get(k).and_then(Value::as_str);
    if field("sessionId") != Some(s.id.as_str()) || field("status") != Some("waiting") || field("waitingFor") != Some(what) {
        return None;
    }
    let page = crate::transcript::file_page(&record(a, ctx, s).ok()?, conversation_items, None, 40).ok()?;
    let call = crate::transcript::open_tools(&page.items).last().copied()?;
    Some((call.clone(), pid))
}

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
fn you_typed(v: &Value) -> bool {
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

/// What a pending tool call asks for, e.g. "Bash · `yarn mikro-orm migration:up`".
/// How plan mode's approval (`ExitPlanMode`) reads: its call is the plan, waiting for your yes.
pub const PLAN: &str = "Plan";

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
        _ => None,
    };
    let label = name.strip_prefix("mcp__").map(|rest| rest.replace("__", " · ")).unwrap_or_else(|| name.to_string());
    match detail {
        Some(d) => format!("{label} · {d}"),
        None => label,
    }
}

/// How Codealong talks to a side conversation's copy (side.rs, `Adapter::side`): Claude Code's
/// stream-json, a line a message each way. A question is a `user` line; the answer comes as
/// `stream_event` lines (the API's own events: `content_block_delta` with a `text_delta`), each
/// tool call whole in an `assistant` line, and a `result` line when the turn ends.
#[derive(Default)]
pub struct SideCopy {
    /// It has been told what it is, with its first question.
    told: bool,
    /// Some of this answer is written: what comes after a tool call starts a new paragraph.
    said: bool,
}

impl crate::side::Dialect for SideCopy {
    fn open(&mut self) -> Vec<Value> {
        Vec::new()
    }

    /// The rules go in the first question, not the system prompt: a resumed conversation sends
    /// the system prompt it recorded whatever a later launch passes, and the session's prompt
    /// cache holds only while that and the tools stay as they were.
    fn ask(&mut self, text: &str) -> Vec<Value> {
        let text = if self.told { text.to_string() } else { format!("<system-reminder>\n{}\n</system-reminder>\n\n{text}", crate::side::RULES) };
        self.told = true;
        self.said = false;
        vec![serde_json::json!({ "type": "user", "message": { "role": "user", "content": text } })]
    }

    fn hear(&mut self, line: &Value) -> (Vec<crate::side::Event>, Vec<Value>) {
        use crate::side::Event;
        let text = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).map(String::from);
        let kind = |v: &Value| v.get("type").and_then(Value::as_str).map(String::from).unwrap_or_default();
        // A subagent's lines aren't the answer.
        if line.get("parent_tool_use_id").is_some_and(|p| !p.is_null()) {
            return (Vec::new(), Vec::new());
        }
        let events = match kind(line).as_str() {
            "stream_event" => {
                let e = line.get("event").unwrap_or(&Value::Null);
                match kind(e).as_str() {
                    "content_block_start" if self.said && e.get("content_block").is_some_and(|b| kind(b) == "text") => vec![Event::Said("\n\n".into())],
                    "content_block_delta" => match e.get("delta").filter(|d| kind(d) == "text_delta").and_then(|d| text(d, "text")) {
                        Some(t) => {
                            self.said = true;
                            vec![Event::Said(t)]
                        }
                        None => Vec::new(),
                    },
                    _ => Vec::new(),
                }
            }
            "assistant" => {
                let blocks = line.pointer("/message/content").and_then(Value::as_array).cloned().unwrap_or_default();
                blocks.iter().filter(|b| kind(b) == "tool_use").map(|b| Event::Tool(tool_request(&text(b, "name").unwrap_or_default(), b.get("input").unwrap_or(&Value::Null)))).collect()
            }
            "result" => {
                let failed = line.get("is_error").and_then(Value::as_bool).unwrap_or(false);
                let why = || text(line, "result").filter(|r| !r.trim().is_empty()).unwrap_or_else(|| "It ended without an answer.".into());
                vec![Event::Done(failed.then(why))]
            }
            _ => Vec::new(),
        };
        (events, Vec::new())
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

/// The file Claude Code keeps its own state in, the folders it trusts among it (2.1.285's code):
/// `~/.claude.json` for the default folder and `<home>/.claude.json` for any other
/// (`CLAUDE_CONFIG_DIR`), or an old install's `<home>/.config.json` while that's still there.
pub fn state_file(home: &Path) -> PathBuf {
    let legacy = home.join(".config.json");
    if legacy.is_file() {
        return legacy;
    }
    let user_home = crate::util::home();
    if home == user_home.join(".claude") {
        user_home.join(".claude.json")
    } else {
        home.join(".claude.json")
    }
}

const TRUSTED: &str = "hasTrustDialogAccepted";

/// Trusts `folder` for the account, as answering Yes to Claude Code's trust prompt there does,
/// so a session Codealong starts there isn't stopped by the prompt (whose default answer quits)
/// and `--bg` doesn't refuse it. False when Claude Code trusts it already, and then the file
/// isn't touched. Claude Code rewrites the file all the time, so the change is made under its
/// own lock and written whole, and it only ever adds the one entry.
pub fn trust_folder(home: &Path, folder: &Path) -> anyhow::Result<bool> {
    trust_folder_in(&state_file(home), folder, &crate::util::home())
}

fn trust_folder_in(file: &Path, folder: &Path, user_home: &Path) -> anyhow::Result<bool> {
    // Claude Code goes by its working folder, which the system gives with links resolved.
    let folder = std::fs::canonicalize(folder)?;
    let user_home = std::fs::canonicalize(user_home).unwrap_or_else(|_| user_home.to_path_buf());
    // It trusts the home folder one session at a time, and every folder outside a repo goes by
    // the folders above it, so trusting home or above would trust nearly everything.
    if user_home.starts_with(&folder) {
        anyhow::bail!("{} holds the home folder, which Claude Code trusts one session at a time", folder.display());
    }
    let keys = trust_keys(&folder)?;
    // Most folders are trusted already (a worktree goes by its repo's main checkout), so the
    // lock is taken only to add an entry.
    let trusted = |text: &str| matches!(with_trust(text, &keys), Ok(None));
    if std::fs::read_to_string(file).is_ok_and(|t| trusted(&t)) {
        return Ok(false);
    }
    let _lock = StateLock::take(file)?;
    let text = match std::fs::read_to_string(file) {
        Ok(t) => t,
        // An account that hasn't run yet: Claude Code starts from whatever the file holds.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => "{}".to_string(),
        Err(e) => return Err(e.into()),
    };
    let Some(next) = with_trust(&text, &keys)? else { return Ok(false) };
    write_whole(file, &next)?;
    Ok(true)
}

/// The `projects` entries Claude Code looks at to tell whether it trusts `folder` (2.1.285's
/// code): its own first, the one a trust adds, then each folder's above it, up to its worktree's
/// root in a repo and all the way up outside one, and its repo's main checkout's.
fn trust_keys(folder: &Path) -> anyhow::Result<Vec<String>> {
    let located = crate::repos::locate(folder);
    let mut keys: Vec<&Path> = Vec::new();
    for dir in folder.ancestors() {
        keys.push(dir);
        if located.as_ref().is_some_and(|(root, _)| dir == root) {
            break;
        }
    }
    keys.extend(located.as_ref().map(|(_, main)| main.as_path()));
    // A main checkout is its own.
    keys.dedup();
    let own = keys[0].to_str().ok_or_else(|| anyhow::anyhow!("{} isn't a name Claude Code can keep", folder.display()))?;
    Ok(std::iter::once(own.to_string()).chain(keys[1..].iter().filter_map(|k| k.to_str()).map(String::from)).collect())
}

/// `text`, Claude Code's state file, with the entry for `keys[0]` (the folder's own, see
/// `trust_keys`) marked trusted, or None when an entry of `keys` is trusted already. Only the
/// entry it adds or changes is written anew: everything else keeps its text and its order.
fn with_trust(text: &str, keys: &[String]) -> anyhow::Result<Option<String>> {
    use serde_json::value::RawValue;
    let own = keys.first().ok_or_else(|| anyhow::anyhow!("no folder to trust"))?;
    let mut top = Node::entries(text)?;
    let projects = Node::object_in(&mut top, "projects")?;
    for key in keys {
        let entry = projects.iter().rev().find(|(k, _)| k == key);
        if entry.is_some_and(|(_, v)| v.is_trusted()) {
            return Ok(None);
        }
    }
    let entry = Node::object_in(projects, own)?;
    let yes = RawValue::from_string("true".into())?;
    match entry.iter_mut().rev().find(|(k, _)| k == TRUSTED) {
        Some((_, v)) => *v = Node::Raw(yes),
        None => entry.push((TRUSTED.to_string(), Node::Raw(yes))),
    }
    // Claude Code writes the file as JSON.stringify(config, null, 2) does, which serde_json's
    // pretty printer matches.
    let mut out = serde_json::to_string_pretty(&Node::Object(top))?;
    if text.ends_with('\n') {
        out.push('\n');
    }
    Ok(Some(out))
}

/// A JSON value from Claude Code's state file: text left as it was, or an object opened up to
/// change, its keys in their order.
enum Node {
    Raw(Box<serde_json::value::RawValue>),
    Object(Vec<(String, Node)>),
}

impl Node {
    /// An object's entries, their values left as text.
    fn entries(text: &str) -> anyhow::Result<Vec<(String, Node)>> {
        let Entries(entries) = serde_json::from_str(text)?;
        Ok(entries.into_iter().map(|(k, v)| (k, Node::Raw(v))).collect())
    }

    /// The object under `key` in `entries` (the last, as JSON.parse takes it), opened up, and made
    /// when it's missing or null.
    fn object_in<'a>(entries: &'a mut Vec<(String, Node)>, key: &str) -> anyhow::Result<&'a mut Vec<(String, Node)>> {
        let i = match entries.iter().rposition(|(k, _)| k == key) {
            Some(i) => i,
            None => {
                entries.push((key.to_string(), Node::Object(Vec::new())));
                entries.len() - 1
            }
        };
        if let Node::Raw(raw) = &entries[i].1 {
            let opened = if raw.get() == "null" { Vec::new() } else { Node::entries(raw.get())? };
            entries[i].1 = Node::Object(opened);
        }
        match &mut entries[i].1 {
            Node::Object(inner) => Ok(inner),
            Node::Raw(_) => unreachable!(),
        }
    }

    fn is_trusted(&self) -> bool {
        match self {
            Node::Raw(raw) => serde_json::from_str::<Value>(raw.get()).is_ok_and(|v| v.get(TRUSTED) == Some(&Value::Bool(true))),
            Node::Object(entries) => entries.iter().rev().find(|(k, _)| k == TRUSTED).is_some_and(|(_, v)| matches!(v, Node::Raw(r) if r.get() == "true")),
        }
    }
}

impl serde::Serialize for Node {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        match self {
            Node::Raw(raw) => raw.serialize(s),
            Node::Object(entries) => {
                let mut map = s.serialize_map(Some(entries.len()))?;
                for (k, v) in entries {
                    map.serialize_entry(k, v)?;
                }
                map.end()
            }
        }
    }
}

/// An object's entries in the order they're written, each value as its text.
struct Entries(Vec<(String, Box<serde_json::value::RawValue>)>);

impl<'de> Deserialize<'de> for Entries {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Visit;
        impl<'de> serde::de::Visitor<'de> for Visit {
            type Value = Entries;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("an object")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(self, mut map: A) -> Result<Entries, A::Error> {
                let mut entries = Vec::new();
                while let Some(entry) = map.next_entry()? {
                    entries.push(entry);
                }
                Ok(Entries(entries))
            }
        }
        d.deserialize_map(Visit)
    }
}

/// Claude Code's lock on its state file, taken as it takes it (proper-lockfile): a folder beside
/// the file, `<file>.lock`, which only one process can make. Claude Code holds it while it reads
/// the file again and writes it, and freshens its time every 5 seconds; one untouched for 10 is
/// a crashed process's, and is taken over.
struct StateLock(PathBuf);

impl StateLock {
    fn take(file: &Path) -> anyhow::Result<StateLock> {
        use std::time::{Duration, Instant};
        let mut path = file.as_os_str().to_owned();
        path.push(".lock");
        let path = PathBuf::from(path);
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match std::fs::create_dir(&path) {
                Ok(()) => return Ok(StateLock(path)),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(e.into()),
            }
            match std::fs::metadata(&path).and_then(|m| m.modified()) {
                // Let go of in the meantime.
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Ok(at) if at.elapsed().is_ok_and(|age| age > Duration::from_secs(10)) => {
                    let _ = std::fs::remove_dir(&path);
                    continue;
                }
                _ => {}
            }
            if Instant::now() > deadline {
                anyhow::bail!("Claude Code has held {} for more than 5 seconds", path.display());
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

impl Drop for StateLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir(&self.0);
    }
}

/// Replaces `file` with `text` in one step, so nobody reads it half written: through a link to
/// it, as Claude Code writes it, and readable only by you, as Claude Code keeps it.
fn write_whole(file: &Path, text: &str) -> anyhow::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let target = std::fs::canonicalize(file).unwrap_or_else(|_| file.to_path_buf());
    let name = target.file_name().and_then(|n| n.to_str()).unwrap_or(".claude.json");
    let temp = target.with_file_name(format!("{name}.codealong-{}", std::process::id()));
    let written = (|| -> std::io::Result<()> {
        let mut f = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&temp)?;
        f.write_all(text.as_bytes())?;
        f.sync_all()?;
        std::fs::rename(&temp, &target)
    })();
    if written.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    Ok(written?)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A transcript line in the conversation's chain, as Claude Code 2.1.285 writes them.
    fn chained(kind: &str, uuid: &str, parent: Option<&str>, at: &str, rest: Value) -> String {
        let mut v = serde_json::json!({ "type": kind, "uuid": uuid, "parentUuid": parent, "timestamp": at, "sessionId": "s1" });
        v.as_object_mut().unwrap().extend(rest.as_object().unwrap().clone());
        v.to_string()
    }

    fn prompt(uuid: &str, parent: Option<&str>, text: &str) -> String {
        chained("user", uuid, parent, "2026-09-30T00:15:50.220Z", serde_json::json!({ "origin": { "kind": "human" }, "message": { "role": "user", "content": text } }))
    }

    fn attachment(uuid: &str, parent: &str) -> String {
        chained("attachment", uuid, Some(parent), "2026-09-30T00:15:50.220Z", serde_json::json!({ "attachment": { "type": "total_tokens_reminder", "text": "<total_tokens>1</total_tokens>" } }))
    }

    fn reply(uuid: &str, parent: &str, text: &str) -> String {
        chained("assistant", uuid, Some(parent), "2026-09-30T00:16:10.000Z", serde_json::json!({ "message": { "role": "assistant", "content": [{ "type": "text", "text": text }] } }))
    }

    fn fate(lines: &[String]) -> Option<Fate> {
        fate_of(lines.iter().map(String::as_str)).map(|(f, _)| f)
    }

    #[test]
    fn a_prompt_the_conversation_went_on_without_was_taken_back() {
        // Stopped before a reply: the recap written minutes later goes on from the line before the
        // prompt, as the one on 2026-09-29 did.
        let recap = chained("system", "r", Some("a1"), "2026-09-30T00:19:10.590Z", serde_json::json!({ "subtype": "away_summary", "content": "You're building…" }));
        let title = r#"{"type":"ai-title","aiTitle":"Security dashboard","sessionId":"s1"}"#.to_string();
        assert_eq!(fate(&[prompt("p", Some("a1"), "Who is the sponsor?"), attachment("p1", "p"), title, recap]), Some(Fate::TakenBack));
        // The next prompt goes on from there too.
        assert_eq!(fate(&[prompt("p", Some("a1"), "Who is the sponsor?"), attachment("p1", "p"), prompt("q", Some("a1"), "Just say OK.")]), Some(Fate::TakenBack));
        // A session's first prompt has nothing before it to go on from: the next prompt follows
        // its lines, with its text put back in front.
        assert_eq!(fate(&[prompt("p", None, "Write an essay."), attachment("p1", "p"), prompt("q", Some("p1"), "Write an essay.Just say OK.")]), Some(Fate::TakenBack));
    }

    #[test]
    fn a_prompt_answered_or_interrupted_was_kept() {
        let p = || [prompt("p", Some("a1"), "Run the tests"), attachment("p1", "p")];
        assert_eq!(fate(&[p().as_slice(), &[reply("x", "p1", "Running them.")]].concat()), Some(Fate::Kept));
        // Escape with text in the box interrupts the turn and keeps the prompt.
        let interrupted = chained("user", "i", Some("p1"), "2026-09-30T00:16:00.000Z", serde_json::json!({ "message": { "role": "user", "content": [{ "type": "text", "text": "[Request interrupted by user]" }] } }));
        assert_eq!(fate(&[p().as_slice(), &[interrupted]].concat()), Some(Fate::Kept));
        // Compaction starts the chain again.
        let boundary = chained("system", "c", None, "2026-09-30T00:16:00.000Z", serde_json::json!({ "subtype": "compact_boundary" }));
        assert_eq!(fate(&[p().as_slice(), &[boundary]].concat()), Some(Fate::Kept));
        // Nothing yet either way.
        assert_eq!(fate(&p()), Some(Fate::Open));
        // Only prompts of yours are asked about.
        assert_eq!(fate(&[attachment("p1", "p")]), None);
    }

    #[test]
    fn marks_a_prompt_taken_back_in_the_conversation() {
        let dir = std::env::temp_dir().join(format!("codealong-taken-back-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.jsonl");
        let write = |lines: &[String]| std::fs::write(&path, lines.iter().map(|l| format!("{l}\n")).collect::<String>()).unwrap();
        let answered = [prompt("o", Some("z"), "Earlier question"), reply("a0", "o", "Earlier answer.")];
        let mut lines = [answered.as_slice(), &[prompt("p", Some("a0"), "Who is the sponsor?"), attachment("p1", "p")]].concat();
        write(&lines);
        let page = |idle: Option<i64>| {
            let mut page = crate::transcript::file_page(&path, conversation_items, None, 100).unwrap();
            mark_taken_back(&path, &mut page.items, idle);
            page.items.iter().map(|i| (i.kind, i.withdrawn)).collect::<Vec<_>>()
        };
        // Until something is written after it, the registry says: idle since after the prompt.
        let sent = parse_rfc3339_ms("2026-09-30T00:15:50.220Z").unwrap();
        assert_eq!(page(None), [("you", false), ("agent", false), ("you", false)]);
        assert_eq!(page(Some(sent - 5_000)), [("you", false), ("agent", false), ("you", false)], "idle since before it: it hasn't started yet");
        assert_eq!(page(Some(sent + 900)), [("you", false), ("agent", false), ("you", true)]);
        // A read of what's newer starts at it again while nothing has come back for it.
        let cursor = std::fs::metadata(&path).unwrap().len() - (lines[2].len() + lines[3].len() + 2) as u64;
        let end = std::fs::metadata(&path).unwrap().len();
        assert_eq!(open_prompt_before(&path, end), Some(cursor));
        // Once a line goes on without it, it's taken back whatever the registry says.
        lines.push(prompt("q", Some("a0"), "Just say OK."));
        write(&lines);
        assert_eq!(page(None), [("you", false), ("agent", false), ("you", true), ("you", false)]);
        // Answered, it's there for good.
        lines.truncate(4);
        lines.push(reply("x", "p1", "The sponsor is…"));
        write(&lines);
        assert_eq!(page(Some(sent + 900)), [("you", false), ("agent", false), ("you", false), ("agent", false)]);
        assert_eq!(open_prompt_before(&path, std::fs::metadata(&path).unwrap().len()), None);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn says_what_always_adds_from_claude_codes_own_suggestions() {
        // What 2.1.284 suggested for `touch` in the session's folder.
        let s = serde_json::json!([
            { "type": "addDirectories", "directories": ["/Users/x/work"], "destination": "session" },
            { "type": "setMode", "mode": "acceptEdits", "destination": "session" }
        ]);
        let s = s.as_array().unwrap();
        assert_eq!(always_text(s).as_deref(), Some("Allows working in `/Users/x/work` for this session; Switches to accepting edits for this session."));
        let rules = serde_json::json!([{ "type": "addRules", "rules": [{ "toolName": "Bash", "ruleContent": "npm test:*" }, { "toolName": "WebFetch" }], "behavior": "allow", "destination": "localSettings" }]);
        assert_eq!(always_text(rules.as_array().unwrap()).as_deref(), Some("Allows `Bash(npm test:*)`, `WebFetch` in this project, for you."));
        assert_eq!(always_text(&[]), None);
        assert_eq!(always_text(&[serde_json::json!({ "type": "removeRules" })]), None);
    }

    #[test]
    fn a_question_is_answered_with_its_choices_or_your_own_words() {
        let input = serde_json::json!({ "questions": [
            { "question": "Which color do you prefer?", "header": "Color", "options": [{ "label": "Red", "description": "The color red" }, { "label": "Blue" }], "multiSelect": false },
            { "question": "Which days?", "options": [{ "label": "Mon" }, { "label": "Tue" }], "multiSelect": true }
        ] });
        let qs = questions_of(&input);
        assert_eq!(qs.len(), 2);
        assert_eq!((qs[0].header.as_deref(), qs[0].options[0].description.as_deref(), qs[1].multi_select), (Some("Color"), Some("The color red"), true));
        let a = crate::asks::Asked { id: "ask-q".into(), session: "abc".into(), agent: None, tool: "AskUserQuestion".into(), input: input.clone(), suggestions: vec![], at: 1 };
        let p = asked_pending(&a);
        assert_eq!((p.label.as_str(), p.input, p.always, p.questions.len()), ("Asks you · Which color do you prefer?", None, false, 2));
        let got = picked(&qs, &serde_json::json!({ "Which color do you prefer?": " Green, please ", "Which days?": ["Mon", "", "Tue"], "Not asked": "x" })).unwrap();
        assert_eq!(Value::Object(got), serde_json::json!({ "Which color do you prefer?": "Green, please", "Which days?": ["Mon", "Tue"] }));
        let one = picked(&qs, &serde_json::json!({ "Which color do you prefer?": ["Blue", "Red"] })).unwrap();
        assert_eq!(one["Which color do you prefer?"], "Blue", "one pick for a question that takes one");
        assert!(picked(&qs, &serde_json::json!({ "Which days?": [] })).is_err());
    }

    #[test]
    fn questions_show_in_the_conversation_and_wait_where_the_session_runs() {
        use crate::harness::Adapter;
        let home = std::env::temp_dir().join(format!("codealong-questions-{}", std::process::id()));
        let dir = home.join("projects/-x");
        std::fs::create_dir_all(home.join("sessions")).unwrap();
        std::fs::create_dir_all(&dir).unwrap();
        let id = "5c1d7e2a";
        let ask = r#"{"type":"assistant","timestamp":"2026-09-30T20:33:55Z","message":{"content":[{"type":"tool_use","id":"toolu_q","name":"AskUserQuestion","input":{"questions":[{"question":"Which limit?","header":"Limit","multiSelect":false,"options":[{"label":"100 days (Recommended)","description":"A quarter and a bit."},{"label":"45 days"}]},{"question":"Which days?","multiSelect":true,"options":[{"label":"Mon"},{"label":"Tue"}]}]}}]}}"#;
        let answered = r#"{"type":"user","timestamp":"2026-09-30T20:40:10Z","message":{"content":[{"type":"tool_result","tool_use_id":"toolu_q","content":"Your questions have been answered"}]},"toolUseResult":{"questions":[],"answers":{"Which limit?":"100 days (Recommended)","Which days?":["Mon","Tue"],"Blank":" "},"annotations":{}}}"#;
        std::fs::write(dir.join(format!("{id}.jsonl")), format!("{ask}\n")).unwrap();
        let account = crate::config::AccountCfg { name: "claude".into(), harness: "claude".into(), home: home.to_string_lossy().into(), command: None, start: None, preferred: false, acp: None };
        let ctx = crate::harness::Ctx { account: &account };
        let session: crate::model::Session = serde_json::from_value(serde_json::json!({
            "key": format!("claude:claude:{id}"), "id": id, "shortId": "s", "harness": "claude", "account": "claude", "surface": "cli",
            "status": "needs", "live": true, "pid": 4242, "title": "Limits", "cwd": "/x", "work": [], "name": null, "startedAt": null,
            "statusSince": null, "lastActivity": 0, "lastPromptAt": 0, "excerpt": null, "resumeCommand": null, "appAccount": false,
            "openTarget": "", "visitor": null, "headline": "", "detail": null, "branch": null, "archived": false, "attachCommand": null,
            "continues": null, "stalledSince": null
        }))
        .unwrap();
        let registry = |waiting_for: &str| std::fs::write(home.join("sessions/4242.json"), serde_json::json!({ "pid": 4242, "sessionId": id, "status": "waiting", "waitingFor": waiting_for }).to_string()).unwrap();

        // The call's questions come with it, for the panel to show as questions.
        let page = ClaudeCode.conversation(&ctx, &session, None, 50).unwrap();
        let qs = &page.items[0].questions;
        assert_eq!((qs.len(), qs[0].header.as_deref(), qs[0].options[0].description.as_deref(), qs[1].multi_select), (2, Some("Limit"), Some("A quarter and a bit."), true));

        // Asked with no hook to hand them over: they show, and are answered where the session runs.
        registry("input needed");
        let p = ClaudeCode.pending(&ctx, &session).unwrap();
        assert_eq!((p.id.as_str(), p.label.as_str(), p.questions.len(), p.cannot.is_some()), ("toolu_q", "Asks you · Which limit?", 2, true));
        // A permission prompt isn't a question, and nor is a call without any.
        registry("permission prompt");
        assert!(ClaudeCode.pending(&ctx, &session).map_or(true, |p| p.questions.is_empty()));

        // Answered: the result has your picks, several where it took them.
        std::fs::write(dir.join(format!("{id}.jsonl")), format!("{ask}\n{answered}\n")).unwrap();
        let page = ClaudeCode.conversation(&ctx, &session, None, 50).unwrap();
        let r = &page.items[1];
        assert_eq!((r.kind, r.id.as_deref()), ("result", Some("toolu_q")));
        assert_eq!(r.answers.get("Which limit?"), Some(&vec!["100 days (Recommended)".to_string()]));
        assert_eq!(r.answers.get("Which days?"), Some(&vec!["Mon".to_string(), "Tue".to_string()]));
        assert!(!r.answers.contains_key("Blank"));
        registry("input needed");
        assert!(ClaudeCode.pending(&ctx, &session).is_none(), "nothing waits once it's answered");
        std::fs::remove_dir_all(&home).ok();
    }

    #[test]
    fn a_held_request_is_the_call_the_panel_answers() {
        let a = crate::asks::Asked { id: "ask-9".into(), session: "abc".into(), agent: None, tool: "Bash".into(), input: serde_json::json!({ "command": "touch alpha.txt" }), suggestions: vec![serde_json::json!({ "type": "setMode", "mode": "acceptEdits", "destination": "session" })], at: 1 };
        let p = asked_pending(&a);
        assert_eq!((p.id.as_str(), p.label.as_str(), p.input.as_deref(), p.cannot, p.always), ("ask-9", "Bash · `touch alpha.txt`", Some("touch alpha.txt"), None, true));
        assert_eq!(p.always_rule.as_deref(), Some("Switches to accepting edits for this session."));
    }

    #[test]
    fn reads_the_mode_the_latest_prompt_went_in() {
        let dir = std::env::temp_dir().join(format!("codealong-mode-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let t = dir.join("s.jsonl");
        let lines = [
            r#"{"type":"user","permissionMode":"default","message":{"role":"user","content":"one"}}"#,
            r#"{"type":"user","permissionMode":"bypassPermissions","message":{"role":"user","content":"two"}}"#,
            // A tool's output that has the key in it is a string, its quotes escaped.
            r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","content":"\"permissionMode\":\"default\""}]}}"#,
        ];
        std::fs::write(&t, lines.join("\n") + "\n").unwrap();
        assert_eq!(permission_mode(None, std::slice::from_ref(&t)).as_deref(), Some("bypassPermissions"));
        assert_eq!(line_mode(lines[2]), None);
        assert_eq!(permission_mode(None, &[]), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

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
        let tail = r#"{"type":"assistant","cwd":"/Users/me/projects/atlasai/atlas-quarry","message":{"content":[{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"git worktree add ../atlas-fix"}}]}}
{"type":"assistant","cwd":"/Users/me/projects/atlasai/atlas-quarry","message":{"content":[{"type":"tool_use","id":"t2","name":"Bash","input":{"command":"cd ../atlas-fix && yarn test"}}]}}
{"type":"assistant","cwd":"/Users/me/projects/atlasai/atlas-quarry","message":{"content":[{"type":"tool_use","id":"t3","name":"Edit","input":{"file_path":"/Users/me/projects/atlasai/atlas-fix/src/a.ts"}}]}}
{"type":"assistant","cwd":"/Users/me/projects/atlasai/atlas-fix","message":{"content":[{"type":"tool_use","id":"t4","name":"Bash","input":{"command":"yarn lint"}}]}}
{"type":"assistant","cwd":"/Users/me/projects/atlasai/atlas-fix","message":{"content":[{"type":"tool_use","id":"t5","name":"Read","input":{"file_path":"/Users/me/projects/atlasai/atlas/README.md"}}]}}
"#;
        let info = parse_lines(HEAD, tail);
        // Making the worktree names a folder outside the one it runs in, so it isn't work there.
        assert_eq!(
            info.work,
            [
                "/Users/me/projects/atlasai/atlas-fix",
                "/Users/me/projects/atlasai/atlas-fix/src/a.ts",
                "/Users/me/projects/atlasai/atlas-fix",
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
    fn finds_the_call_running_at_the_end_of_a_transcript() {
        let dir = std::env::temp_dir().join(format!("codealong-claude-open-calls-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("s.jsonl");
        let call = |id: &str, command: &str| serde_json::json!({ "type": "assistant", "timestamp": "2026-09-27T16:00:00.000Z", "message": { "content": [{ "type": "tool_use", "id": id, "name": "Bash", "input": { "command": command } }] } }).to_string();
        let result = |id: &str| serde_json::json!({ "type": "user", "timestamp": "2026-09-27T16:00:01.000Z", "message": { "content": [{ "type": "tool_result", "tool_use_id": id, "content": "ok" }] } }).to_string();
        std::fs::write(&path, [call("t1", "ls"), result("t1"), call("t2", "codealong todo done t-abc")].join("\n") + "\n").unwrap();
        assert_eq!(open_calls(&path), vec!["codealong todo done t-abc".to_string()]);
        std::fs::write(&path, [call("t1", "ls"), result("t1")].join("\n") + "\n").unwrap();
        assert!(open_calls(&path).is_empty());
        std::fs::remove_dir_all(&dir).ok();
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
            peer("2026-09-23T15:05:00.000Z", "Codealong", &format!("{}\n\nNow the docs", crate::send::FROM_YOU)),
            // An agent's isn't, whatever it calls itself.
            peer("2026-09-23T15:06:00.000Z", "Codealong", "This message is from another agent, “x”.\n\nhi"),
            peer("2026-09-23T15:07:00.000Z", "review", &format!("{}\n\nhi", crate::send::FROM_YOU)),
        ]
        .join("\n");
        let info = parse_lines(HEAD, &(tail + "\n"));
        assert_eq!(info.last_prompt_at, parse_rfc3339_ms("2026-09-23T15:05:00.000Z"));
    }

    #[test]
    fn finds_the_last_prompt_behind_a_long_turn() {
        let dir = std::env::temp_dir().join(format!("codealong-prompt-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("s.jsonl");
        let prompt = r#"{"type":"user","timestamp":"2026-09-23T15:00:00.000Z","origin":{"kind":"human"},"message":{"content":"Refactor the parser"}}"#;
        let output = |i: usize| format!(r#"{{"type":"user","timestamp":"2026-09-23T15:10:00.000Z","message":{{"content":[{{"type":"tool_result","tool_use_id":"t{i}","content":"{}"}}]}}}}"#, "x".repeat(120_000));
        let mut lines = vec![HEAD.trim_end().to_string(), prompt.to_string()];
        lines.extend((0..12).map(output));
        std::fs::write(&path, lines.join("\n") + "\n").unwrap();
        let (_, info, _) = parse_transcript(&path, 1, None).unwrap();
        assert_eq!(info.last_prompt_at, parse_rfc3339_ms("2026-09-23T15:00:00.000Z"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn parses_registry_entries() {
        let e = parse_registry(r#"{"pid":22303,"sessionId":"9f62","cwd":"/x","status":"busy","kind":"interactive","entrypoint":"cli","name":"atlas-app4-4b","nameSource":"derived","startedAt":1,"statusUpdatedAt":2}"#).unwrap();
        assert_eq!(e.status, "busy");
        assert_eq!(e.name.as_deref(), Some("atlas-app4-4b"));
        assert_eq!(e.parked_job, None);
        assert!(!e.spare);
        assert!(parse_registry(r#"{"pid":1}"#).is_none());
    }

    #[test]
    fn reads_the_conversation_a_background_job_goes_on_from() {
        let roster = serde_json::json!({"workers": {
            "37620e2b": {"sessionId": "37620e2b-35d0-47b2-8e04-9a55468b75fe", "dispatch": {"launch": {"mode": "resume", "sessionId": "3628c9e6-0309-4a30-aed9-3e68d0e66b7e", "fork": true}}},
            "5a1b2c3d": {"sessionId": "5a1b2c3d-0000-4000-8000-000000000000", "dispatch": {"source": "slash", "launch": {"mode": "resume", "fork": true, "sessionId": "/Users/me/.claude/projects/-r-x/9f62aa00-0000-4000-8000-000000000000.jsonl"}}},
            "77777777": {"sessionId": "77777777-0000-4000-8000-000000000000", "dispatch": {"launch": {"mode": "resume", "sessionId": "77777777-0000-4000-8000-000000000000"}}},
            "d3d3cd21": {"sessionId": "d3d3cd21-0478-4ac5-a902-a215a327e140", "dispatch": {"launch": {"mode": "prompt"}}}
        }});
        // A fork names the session it copies; a window's /background, its transcript.
        assert_eq!(resumed_from(&roster, "37620e2b").as_deref(), Some("3628c9e6-0309-4a30-aed9-3e68d0e66b7e"));
        assert_eq!(resumed_from(&roster, "5a1b2c3d").as_deref(), Some("9f62aa00-0000-4000-8000-000000000000"));
        // Going on as the session itself, started afresh, or not in the roster.
        assert_eq!(resumed_from(&roster, "77777777"), None);
        assert_eq!(resumed_from(&roster, "d3d3cd21"), None);
        assert_eq!(resumed_from(&roster, "00000000"), None);
    }

    #[test]
    fn reads_what_each_job_was_forked_from_once() {
        let home = std::env::temp_dir().join(format!("codealong-job-forks-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let job = |name: &str, state: &str| {
            std::fs::create_dir_all(home.join("jobs").join(name)).unwrap();
            std::fs::write(home.join("jobs").join(name).join("state.json"), state).unwrap();
        };
        job("fdfea444", r#"{"state":"done","sessionId":"fdfea444-5c75","forkSessionId":"fdfea444-5c75","forkParentSessionId":"b887d77f-c07e","intent":"x"}"#);
        job("f905c97d", r#"{"state":"stopped","sessionId":"f905c97d-d6fc","intent":""}"#);
        job("starting", r#"{"state":"starting"}"#);
        std::fs::write(home.join("jobs").join("pins.json"), "{}").unwrap();
        let later = crate::util::now_ms() + JOB_SETTLES_MS + 1;
        let mut forks = Forks::default();
        assert!(forks.refresh(&home, later));
        assert_eq!(forks.parent_of("fdfea444-5c75"), Some("b887d77f-c07e"));
        assert_eq!(forks.parent_of("f905c97d-d6fc"), None);
        assert_eq!(forks.parent_of("b887d77f-c07e"), None);
        // Read once: a record that changes afterwards isn't read again.
        job("fdfea444", r#"{"sessionId":"fdfea444-5c75","forkParentSessionId":"someone-else"}"#);
        assert!(!forks.refresh(&home, later));
        assert_eq!(forks.parent_of("fdfea444-5c75"), Some("b887d77f-c07e"));
        // One without a session yet, or just written without a parent, is read again later.
        job("starting", r#"{"sessionId":"aaaa","forkParentSessionId":"bbbb"}"#);
        job("fresh", r#"{"sessionId":"cccc"}"#);
        assert!(forks.refresh(&home, crate::util::now_ms()));
        assert_eq!(forks.parent_of("aaaa"), Some("bbbb"));
        job("fresh", r#"{"sessionId":"cccc","forkParentSessionId":"dddd"}"#);
        assert!(forks.refresh(&home, crate::util::now_ms()));
        assert_eq!(forks.parent_of("cccc"), Some("dddd"));
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn parked_windows_and_spares_run_no_session() {
        let home = std::env::temp_dir().join(format!("codealong-parked-{}", std::process::id()));
        std::fs::create_dir_all(home.join("sessions")).unwrap();
        let (pid, now) = (std::process::id(), crate::util::now_ms());
        let entry = |id: &str, extra: &str| format!(r#"{{"pid":{pid},"sessionId":"{id}","cwd":"/x","status":"idle","kind":"interactive","startedAt":{now}{extra}}}"#);
        std::fs::write(home.join("sessions/1.json"), entry("b664dd38", r#","parkedJobId":"398f3533""#)).unwrap();
        std::fs::write(home.join("sessions/2.json"), entry("c1456f74", "")).unwrap();
        // A spare, as 2.1.286 registers one: named its job, idle, no transcript.
        let spare = r#","jobId":"acdfc0fb","name":"acdfc0fb","spare":true"#;
        std::fs::write(home.join("sessions/3.json"), entry("acdfc0fb-8aa7-4e09-b822-056b058f5ca5", spare)).unwrap();
        assert_eq!(parse_registry(&entry("b664dd38", r#","parkedJobId":"398f3533""#)).unwrap().parked_job.as_deref(), Some("398f3533"));
        assert!(parse_registry(&entry("acdfc0fb-8aa7-4e09-b822-056b058f5ca5", spare)).unwrap().spare);
        let live: Vec<String> = read_registry(&home).into_iter().map(|e| e.session_id).collect();
        assert_eq!(live, vec!["c1456f74"]);
        std::fs::remove_dir_all(&home).unwrap();
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

    #[test]
    fn describes_tools() {
        let input = serde_json::json!({"command": "git status\n--short"});
        assert_eq!(tool_request("Bash", &input), "Bash · `git status`");
        // Plan mode's approval reads as its plan, not as the tool's JSON.
        let plan = serde_json::json!({ "plan": "# Fix the build\n\n1. Pin the toolchain" });
        assert_eq!(tool_request("ExitPlanMode", &plan), PLAN);
        assert_eq!(claude_input("ExitPlanMode", &plan), "# Fix the build\n\n1. Pin the toolchain");
        assert_eq!(tool_request("mcp__northwind__search", &Value::Null), "northwind · search");
        // The row already names the worktree a leading `cd` goes to.
        let input = serde_json::json!({"command": "cd /Users/me/projects/app-fix && yarn test"});
        assert_eq!(tool_request("Bash", &input), "Bash · `yarn test`");
        assert_eq!(tool_activity("Bash", &input), "running `yarn test`");
    }

    #[test]
    fn stops_a_job_by_the_id_it_attaches_to() {
        let a = crate::config::AccountCfg { name: "claude2".into(), harness: "claude".into(), home: "~/.claude2".into(), command: None, start: None, preferred: false, acp: None };
        assert_eq!(stop_command(&a, "claude2 attach 6e07c149").as_deref(), Some("claude2 stop 6e07c149"));
        assert_eq!(stop_command(&a, "claude2 attach 6e07c149; rm -rf ~"), None);
        assert_eq!(stop_command(&a, "claude2 --resume 6e07c149"), None);
    }

    #[test]
    fn names_a_session_where_claude_code_keeps_it() {
        use crate::harness::Adapter;
        use std::os::unix::fs::PermissionsExt;
        let home = std::env::temp_dir().join(format!("codealong-rename-{}", std::process::id()));
        let dir = home.join("projects/-x");
        std::fs::create_dir_all(&dir).unwrap();
        // A job that goes on from a conversation, whose transcript ends in a line cut off midway.
        let (job, from) = ("398f3533-73b3-4089-af4e-06603a870e04", "25b09e0e");
        std::fs::write(dir.join(format!("{from}.jsonl")), format!("{HEAD}{{\"type\":\"ai-title\",\"aiTitle\":\"Pho")).unwrap();
        std::fs::write(dir.join(format!("{job}.jsonl")), format!("{{\"type\":\"ai-title\",\"aiTitle\":\"Photo tools\",\"sessionId\":\"{job}\"}}\n")).unwrap();
        let account = crate::config::AccountCfg { name: "claude".into(), harness: "claude".into(), home: home.to_string_lossy().into(), command: None, start: None, preferred: false, acp: None };
        let session = |id: &str, continues: Option<&str>| -> crate::model::Session {
            serde_json::from_value(serde_json::json!({
                "key": format!("claude:claude:{id}"), "id": id, "shortId": "s", "harness": "claude", "account": "claude", "surface": "cli",
                "status": "idle", "live": true, "pid": null, "title": "Photo tools", "cwd": "/x", "work": [], "name": null, "startedAt": null,
                "statusSince": null, "lastActivity": 0, "lastPromptAt": 0, "excerpt": null, "resumeCommand": null, "appAccount": false,
                "openTarget": "", "visitor": null, "headline": "", "detail": null, "branch": null, "archived": false, "attachCommand": null,
                "continues": continues, "stalledSince": null
            }))
            .unwrap()
        };
        let ctx = crate::harness::Ctx { account: &account };
        let name = r#"Photo tools, "v2""#;
        let changed = ClaudeCode.rename(&ctx, &session(job, Some(from)), name).unwrap();
        assert_eq!(changed, vec![dir.join(format!("{job}.jsonl")), dir.join(format!("{from}.jsonl"))]);
        // The job's own transcript, for the job to take the name up; its line as Claude Code writes it.
        let own = std::fs::read_to_string(dir.join(format!("{job}.jsonl"))).unwrap();
        assert!(own.ends_with(&format!("}}\n{{\"type\":\"custom-title\",\"customTitle\":\"Photo tools, \\\"v2\\\"\",\"sessionId\":\"{job}\"}}\n")), "{own}");
        // The one the board reads it by meanwhile: the cut-off line is ended first, and the name reads back.
        let text = std::fs::read_to_string(dir.join(format!("{from}.jsonl"))).unwrap();
        assert!(text.contains("\"aiTitle\":\"Pho\n{\"type\":\"custom-title\""), "{text}");
        assert_eq!(parse_lines(HEAD, &text[HEAD.len()..]).title.as_deref(), Some(name));
        // What /resume lists it by, readable only by you.
        let sidecar = dir.join(from).join("custom-title.json");
        assert_eq!(std::fs::read_to_string(&sidecar).unwrap(), r#"{"customTitle":"Photo tools, \"v2\""}"#);
        assert_eq!(std::fs::metadata(&sidecar).unwrap().permissions().mode() & 0o777, 0o600);
        assert_eq!(std::fs::metadata(dir.join(from)).unwrap().permissions().mode() & 0o777, 0o700);
        // A session that hasn't written a transcript yet has nowhere to keep a name.
        assert!(ClaudeCode.rename(&ctx, &session("0f5e1d2c", None), name).is_err());
        std::fs::remove_dir_all(&home).ok();
    }

    #[test]
    fn forks_under_an_id_it_picks() {
        let a = crate::config::AccountCfg { name: "claude2".into(), harness: "claude".into(), home: "~/.claude2".into(), command: None, start: None, preferred: false, acp: None };
        let f = fork_as(&a, "4f1c2a9e-0b7d-4e21-9c55-1a2b3c4d5e6f", "9d8c7b6a-5f4e-4d3c-8b2a-1f0e9d8c7b6a");
        assert_eq!(f.input, "claude2 --resume 4f1c2a9e-0b7d-4e21-9c55-1a2b3c4d5e6f --fork-session --session-id 9d8c7b6a-5f4e-4d3c-8b2a-1f0e9d8c7b6a\n");
        assert_eq!(f.key.as_deref(), Some("claude:claude2:9d8c7b6a-5f4e-4d3c-8b2a-1f0e9d8c7b6a"));
        assert_eq!(f.env, vec![("CODEALONG_SESSION".to_string(), "claude:claude2:9d8c7b6a-5f4e-4d3c-8b2a-1f0e9d8c7b6a".to_string())]);
    }

    /// A state file as Claude Code 2.1.285 writes it: JSON.stringify(config, null, 2).
    const STATE: &str = r#"{
  "numStartups": 5,
  "projects": {
    "/r/b": {
      "allowedTools": [],
      "mcpServers": {},
      "hasTrustDialogAccepted": false
    },
    "/r/c": {
      "hasTrustDialogAccepted": true
    }
  },
  "oauthAccount": {
    "emailAddress": "a@b.c"
  }
}"#;

    fn keys(ks: &[&str]) -> Vec<String> {
        ks.iter().map(|k| k.to_string()).collect()
    }

    #[test]
    fn trusts_a_folder_by_adding_its_entry_and_nothing_else() {
        let out = with_trust(STATE, &keys(&["/r/a"])).unwrap().unwrap();
        let expected = STATE.replace("      \"hasTrustDialogAccepted\": true\n    }\n  },", "      \"hasTrustDialogAccepted\": true\n    },\n    \"/r/a\": {\n      \"hasTrustDialogAccepted\": true\n    }\n  },");
        assert_eq!(out, expected);
    }

    #[test]
    fn trusts_a_folder_it_has_an_entry_for_keeping_the_rest_of_it() {
        let out = with_trust(STATE, &keys(&["/r/b"])).unwrap().unwrap();
        assert_eq!(out, STATE.replacen("\"hasTrustDialogAccepted\": false", "\"hasTrustDialogAccepted\": true", 1));
        // A file that ends with a newline keeps it.
        assert!(with_trust(&format!("{STATE}\n"), &keys(&["/r/b"])).unwrap().unwrap().ends_with("}\n}\n"));
    }

    #[test]
    fn leaves_a_folder_claude_code_trusts_already_alone() {
        // Its own entry, one above it, or its repo's main checkout.
        assert_eq!(with_trust(STATE, &keys(&["/r/c"])).unwrap(), None);
        assert_eq!(with_trust(STATE, &keys(&["/r/c/sub", "/r/c"])).unwrap(), None);
        assert_eq!(with_trust(STATE, &keys(&["/w/x", "/r/c"])).unwrap(), None);
        assert!(with_trust(STATE, &keys(&["/r/b/sub", "/r/b"])).unwrap().is_some());
    }

    #[test]
    fn makes_the_projects_it_needs_and_refuses_a_file_it_cant_read() {
        let trusted = "{\n  \"projects\": {\n    \"/r/a\": {\n      \"hasTrustDialogAccepted\": true\n    }\n  }\n}";
        assert_eq!(with_trust("{}", &keys(&["/r/a"])).unwrap().unwrap(), trusted);
        assert_eq!(with_trust("{\"projects\": null}", &keys(&["/r/a"])).unwrap().unwrap(), trusted);
        assert!(with_trust("", &keys(&["/r/a"])).is_err());
        assert!(with_trust("{\"projects\": {", &keys(&["/r/a"])).is_err());
        assert!(with_trust("[]", &keys(&["/r/a"])).is_err());
    }

    fn trust_scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("codealong-trust-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::canonicalize(&dir).unwrap()
    }

    #[test]
    fn looks_at_the_folders_claude_code_looks_at() {
        let dir = trust_scratch("keys");
        let main = dir.join("main");
        let wt = dir.join("wt");
        std::fs::create_dir_all(main.join(".git/worktrees/wt")).unwrap();
        std::fs::write(main.join(".git/worktrees/wt/commondir"), "../..\n").unwrap();
        std::fs::create_dir_all(wt.join("sub")).unwrap();
        std::fs::write(wt.join(".git"), format!("gitdir: {}\n", main.join(".git/worktrees/wt").display())).unwrap();
        let s = |p: &Path| p.to_str().unwrap().to_string();
        assert_eq!(trust_keys(&wt.join("sub")).unwrap(), vec![s(&wt.join("sub")), s(&wt), s(&main)]);
        assert_eq!(trust_keys(&main).unwrap(), vec![s(&main)]);
        // Outside a repo, every folder up to the top.
        let loose = dir.join("loose");
        std::fs::create_dir_all(&loose).unwrap();
        let got = trust_keys(&loose).unwrap();
        assert_eq!(got.first(), Some(&s(&loose)));
        assert_eq!(got.last().map(String::as_str), Some("/"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn trusts_a_folder_in_the_file_under_claude_codes_lock() {
        let dir = trust_scratch("file");
        let file = dir.join(".claude.json");
        let folder = dir.join("proj");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(&file, STATE).unwrap();
        let home = dir.join("home");
        assert!(trust_folder_in(&file, &folder, &home).unwrap());
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(v["projects"][folder.to_str().unwrap()][TRUSTED], Value::Bool(true));
        assert_eq!(v["oauthAccount"]["emailAddress"], "a@b.c");
        assert!(!dir.join(".claude.json.lock").exists());
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(&file).unwrap().permissions().mode() & 0o777, 0o600);
        // Once trusted, the file isn't written again.
        let before = std::fs::read_to_string(&file).unwrap();
        assert!(!trust_folder_in(&file, &folder, &home).unwrap());
        assert_eq!(std::fs::read_to_string(&file).unwrap(), before);
        // Never home or a folder above it.
        assert!(trust_folder_in(&file, &dir, &folder).is_err());
        assert!(trust_folder_in(&file, &folder, &folder).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Reads this Mac's Claude Code state files, writing nothing, and checks that trusting a new
    /// folder would only add its entry: `cargo test trust_keeps_real_state -- --ignored`.
    #[test]
    #[ignore]
    fn trust_keeps_real_state() {
        let user_home = crate::util::home();
        for home in [user_home.join(".claude"), user_home.join(".claude2")] {
            let Ok(text) = std::fs::read_to_string(state_file(&home)) else { continue };
            let key = "/nowhere/codealong-trust-check";
            let out = with_trust(&text, &keys(&[key])).unwrap().unwrap();
            let same = text.bytes().zip(out.bytes()).take_while(|(a, b)| a == b).count();
            let tail = text.bytes().rev().zip(out.bytes().rev()).take_while(|(a, b)| a == b).count().min(text.len() - same);
            assert_eq!(same + tail, text.len(), "{}: more than an insertion", home.display());
            let added = &out[same..out.len() - tail];
            assert_eq!(added, format!(",\n    \"{key}\": {{\n      \"{TRUSTED}\": true\n    }}"), "{}", home.display());
            let mut before: Value = serde_json::from_str(&text).unwrap();
            before["projects"][key] = serde_json::json!({ TRUSTED: true });
            assert_eq!(serde_json::from_str::<Value>(&out).unwrap(), before);
            println!("{}: {} bytes, {} projects, adds {} bytes", home.display(), text.len(), before["projects"].as_object().unwrap().len(), added.len());
        }
    }

    #[test]
    fn waits_for_claude_codes_lock_and_takes_over_a_dead_ones() {
        use std::time::{Duration, Instant, SystemTime};
        let dir = trust_scratch("lock");
        let file = dir.join(".claude.json");
        let lock = dir.join(".claude.json.lock");
        let home = dir.join("home");
        std::fs::write(&file, "{}").unwrap();
        // Held: the write waits until it's let go of.
        std::fs::create_dir(&lock).unwrap();
        let held = lock.clone();
        let letting_go = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(300));
            std::fs::remove_dir(&held).unwrap();
        });
        let started = Instant::now();
        std::fs::create_dir_all(dir.join("a")).unwrap();
        assert!(trust_folder_in(&file, &dir.join("a"), &home).unwrap());
        assert!(started.elapsed() >= Duration::from_millis(300));
        letting_go.join().unwrap();
        // Untouched for longer than proper-lockfile's 10 seconds: left by a process that's gone.
        std::fs::create_dir(&lock).unwrap();
        std::fs::File::open(&lock).unwrap().set_modified(SystemTime::now() - Duration::from_secs(20)).unwrap();
        std::fs::create_dir_all(dir.join("b")).unwrap();
        assert!(trust_folder_in(&file, &dir.join("b"), &home).unwrap());
        assert!(!lock.exists());
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(v["projects"].as_object().unwrap().len(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_side_copy_is_told_what_it_is_once_and_heard() {
        use crate::side::{Dialect, Event};
        use serde_json::json;
        let mut copy = SideCopy::default();
        assert!(copy.open().is_empty());
        let first = copy.ask("What changed?");
        let said = first[0].pointer("/message/content").and_then(Value::as_str).unwrap();
        assert!(said.starts_with("<system-reminder>\nThis is a side conversation.") && said.ends_with("</system-reminder>\n\nWhat changed?"), "{said}");
        let lines = [
            json!({"type": "system", "subtype": "init", "tools": ["Bash"]}),
            json!({"type": "stream_event", "parent_tool_use_id": null, "event": {"type": "content_block_start", "index": 0, "content_block": {"type": "thinking", "thinking": ""}}}),
            json!({"type": "stream_event", "parent_tool_use_id": null, "event": {"type": "content_block_delta", "index": 0, "delta": {"type": "thinking_delta", "thinking": "hm"}}}),
            json!({"type": "stream_event", "parent_tool_use_id": null, "event": {"type": "content_block_start", "index": 1, "content_block": {"type": "text", "text": ""}}}),
            json!({"type": "stream_event", "parent_tool_use_id": null, "event": {"type": "content_block_delta", "index": 1, "delta": {"type": "text_delta", "text": "Let me look."}}}),
            // The text again, whole, and the call.
            json!({"type": "assistant", "parent_tool_use_id": null, "message": {"content": [{"type": "text", "text": "Let me look."}, {"type": "tool_use", "id": "toolu_1", "name": "Bash", "input": {"command": "git status"}}]}}),
            // A subagent's words aren't the answer.
            json!({"type": "stream_event", "parent_tool_use_id": "toolu_2", "event": {"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": "Searching"}}}),
            json!({"type": "user", "message": {"content": [{"type": "tool_result", "tool_use_id": "toolu_1", "content": "M a.rs"}]}}),
            json!({"type": "stream_event", "parent_tool_use_id": null, "event": {"type": "content_block_start", "index": 0, "content_block": {"type": "text", "text": ""}}}),
            json!({"type": "stream_event", "parent_tool_use_id": null, "event": {"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": "One file."}}}),
            json!({"type": "result", "subtype": "success", "is_error": false, "result": "One file."}),
        ];
        let heard: Vec<Event> = lines.iter().flat_map(|l| copy.hear(l).0).collect();
        assert_eq!(heard, [Event::Said("Let me look.".into()), Event::Tool("Bash · `git status`".into()), Event::Said("\n\n".into()), Event::Said("One file.".into()), Event::Done(None)]);
        // A follow-up is only itself.
        assert_eq!(copy.ask("And then?")[0].pointer("/message/content"), Some(&json!("And then?")));
        let failed = copy.hear(&json!({"type": "result", "subtype": "success", "is_error": true, "result": "API Error: overloaded"})).0;
        assert_eq!(failed, [Event::Done(Some("API Error: overloaded".into()))]);
        let failed = copy.hear(&json!({"type": "result", "subtype": "error_during_execution", "is_error": true})).0;
        assert_eq!(failed, [Event::Done(Some("It ended without an answer.".into()))]);
    }

    #[test]
    fn a_side_copy_runs_on_its_account_and_keeps_no_record() {
        use crate::harness::Adapter;
        let account = |home: &str| crate::config::AccountCfg { name: "c".into(), harness: "claude".into(), home: home.into(), command: None, start: None, preferred: false, acp: None };
        let mut s: crate::model::Session = serde_json::from_value(serde_json::json!({
            "key": "claude:c:abc", "id": "abc", "shortId": "abc", "harness": "claude", "account": "c", "surface": "cli", "status": "idle", "live": true, "pid": null, "title": "t", "cwd": "/w",
            "name": null, "startedAt": null, "statusSince": null, "lastActivity": 0, "lastPromptAt": 0, "excerpt": null, "resumeCommand": null, "appAccount": false, "openTarget": "", "visitor": null,
            "headline": "", "detail": null, "branch": null, "archived": false, "attachCommand": null, "stalledSince": null
        }))
        .unwrap();
        let usual = account("~/.claude");
        let r = ClaudeCode.side(&crate::harness::Ctx { account: &usual }, &s, Path::new("/w")).unwrap();
        assert_eq!(r.program, PathBuf::from("claude"));
        // A session whose mode isn't written anywhere gets the usual one, where what would ask is refused.
        assert_eq!(r.args.join(" "), "-p --input-format stream-json --output-format stream-json --verbose --include-partial-messages --resume abc --fork-session --no-session-persistence --permission-mode default");
        assert_eq!(side_mode(Some("bypassPermissions")), "bypassPermissions");
        assert_eq!((side_mode(Some("acceptEdits")), side_mode(Some("auto"))), ("acceptEdits", "auto"));
        assert_eq!((side_mode(Some("plan")), side_mode(Some("default")), side_mode(Some("manual")), side_mode(None)), ("default", "default", "default", "default"));
        assert!(r.env.is_empty() && r.unset.contains(&"CLAUDE_CONFIG_DIR"));
        // Another account's folder is named, and a job with no transcript yet copies the one it goes on from.
        s.continues = Some("before".into());
        let other = account("/Users/me/.claude2");
        let r = ClaudeCode.side(&crate::harness::Ctx { account: &other }, &s, Path::new("/w")).unwrap();
        assert_eq!(r.env, [("CLAUDE_CONFIG_DIR".to_string(), "/Users/me/.claude2".to_string())]);
        assert!(!r.unset.contains(&"CLAUDE_CONFIG_DIR") && r.unset.contains(&"CODEALONG_SESSION"));
        assert!(r.args.join(" ").contains("--resume before --fork-session"));
    }

    #[test]
    fn the_reader_takes_in_transcripts_and_hands_new_lines_on() {
        use crate::harness::Reader as _;
        use std::io::Write;

        let home = std::env::temp_dir().join(format!("codealong-claude-reader-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let project = home.join("projects/-Users-me-app");
        std::fs::create_dir_all(project.join("s1/subagents")).unwrap();
        let account = AccountCfg { name: "claude9".into(), harness: "claude".into(), home: home.to_string_lossy().into_owned(), command: None, start: None, preferred: false, acp: None };
        let seen = crate::state::SeenState::default();
        let now = parse_rfc3339_ms("2026-09-23T16:00:00.000Z").unwrap();
        let look = Look { seen: &seen, since: 0, now };
        let mut reader = Reader::default();
        let transcript = project.join("s1.jsonl");
        assert!(reader.owns(&account, &transcript) && reader.owns(&account, &home.join("sessions/41.json")));
        assert!(!reader.owns(&account, &home.join("settings.json")), "only its sessions and transcripts are its to read");

        let mut file = std::fs::File::create(&transcript).unwrap();
        writeln!(file, r#"{{"type":"user","sessionId":"s1","cwd":"/Users/me/app","timestamp":"2026-09-23T15:00:00.000Z","message":{{"role":"user","content":"Fix the build"}}}}"#).unwrap();
        let first = reader.apply(&account, std::slice::from_ref(&transcript), &look);
        assert!(first.dirty && first.added && first.lines.is_empty(), "a transcript read for the first time is a new session");
        assert_eq!(reader.cwds(), ["/Users/me/app"]);
        let sessions = reader.sessions(&account, &look);
        assert_eq!((sessions.len(), sessions[0].key.as_str()), (1, "claude:claude9:s1"));

        // What it wrote since is handed on, from where the last read ended.
        let before = std::fs::metadata(&transcript).unwrap().len();
        writeln!(file, r#"{{"type":"assistant","timestamp":"2026-09-23T15:01:00.000Z","message":{{"content":[{{"type":"text","text":"Done."}}]}}}}"#).unwrap();
        let next = reader.apply(&account, std::slice::from_ref(&transcript), &look);
        assert!(next.dirty && !next.added);
        assert_eq!(next.lines.iter().map(|l| (l.key.as_str(), l.from, l.to)).collect::<Vec<_>>(), [("claude:claude9:s1", before, std::fs::metadata(&transcript).unwrap().len())]);

        // A subagent's log counts for its session, and each stretch of it is handed on once.
        let log = project.join("s1/subagents/agent-a.jsonl");
        std::fs::write(&log, "{}\n").unwrap();
        let sub = reader.apply(&account, std::slice::from_ref(&log), &look);
        assert_eq!(sub.lines.iter().map(|l| (l.key.as_str(), l.from, l.to)).collect::<Vec<_>>(), [("claude:claude9:s1", 0, 3)]);
        assert!(reader.apply(&account, std::slice::from_ref(&log), &look).lines.iter().all(|l| l.from == l.to));
        assert!(reader.logs().contains(&(log.clone(), "claude:claude9:s1".to_string())));
        std::fs::remove_dir_all(&home).unwrap();
    }
}
