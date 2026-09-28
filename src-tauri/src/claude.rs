//! Claude Code: the live session registry (`<home>/sessions/<pid>.json`) and transcripts
//! (`<home>/projects/<encoded-cwd>/<session>.jsonl`). See *Harness adapters*.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::Value;

use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};

use crate::procs;
use crate::transcript::{clip_input, clip_output, item, text_of, Item, Page};
use crate::util::{code, command_dir, file_name, mtime_ms, note_work, one_line, parse_rfc3339_ms, read_head, resolve, scan_lines_back, without_cd};

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
            parts
                .iter()
                .filter(|p| p.get("type").and_then(Value::as_str) == Some("tool_result"))
                .map(|p| Item {
                    id: p.get("tool_use_id").and_then(Value::as_str).map(String::from),
                    output: Some(clip_output(&text_of(p.get("content").unwrap_or(&Value::Null)))),
                    failed: p.get("is_error").and_then(Value::as_bool) == Some(true),
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
                        out.push(Item { id: p.get("id").and_then(Value::as_str).map(String::from), input: Some(clip_input(&claude_input(name, &input))), ..item(at, "tool", tool_request(name, &input)) });
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
/// prompt answered by a keypress in the terminal it runs in, once its own record says it's
/// waiting on that prompt and its transcript that the call is the one you saw.
pub struct ClaudeCode;

impl crate::harness::Adapter for ClaudeCode {
    fn harness(&self) -> crate::model::Harness {
        crate::model::Harness::Claude
    }

    /// Its transcript, looked up by id under every project folder.
    fn records(&self, account: &crate::config::AccountCfg, s: &crate::model::Session) -> Vec<PathBuf> {
        let Ok(dirs) = std::fs::read_dir(account.home_path().join("projects")) else { return Vec::new() };
        dirs.flatten().map(|d| d.path().join(format!("{}.jsonl", s.id))).filter(|p| p.is_file()).collect()
    }

    fn conversation(&self, ctx: &crate::harness::Ctx, s: &crate::model::Session, before: Option<u64>, limit: usize) -> anyhow::Result<Page> {
        crate::transcript::file_page(&record(self, ctx, s)?, conversation_items, before, limit)
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
        crate::transcript::file_newer(&record(self, ctx, s)?, conversation_items, after)
    }

    fn pending(&self, ctx: &crate::harness::Ctx, s: &crate::model::Session) -> Option<crate::harness::Pending> {
        let (call, _) = waiting_call(self, ctx, s)?;
        let cannot = match crate::launch::terminal_kind(s.pid?) {
            Some(_) => None,
            None => Some("It isn't in a terminal Codealong can press keys in (Ghostty or tmux).".to_string()),
        };
        // Not every prompt has "Yes, and always allow" as its second choice, and a 2 pressed at
        // one without it is something else, so Always isn't offered in a terminal.
        let plan = call.text == PLAN;
        Some(crate::harness::Pending { id: call.id.clone().unwrap_or_default(), label: call.text, input: call.input, cannot, always: false, plan })
    }

    fn answer(&self, ctx: &crate::harness::Ctx, s: &crate::model::Session, id: &str, answer: crate::harness::Answer) -> anyhow::Result<String> {
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
            anyhow::bail!("It isn't in a terminal Codealong can press keys in (Ghostty or tmux). Open it to answer.");
        };
        crate::launch::press_key(&host, key)?;
        Ok(match answer {
            crate::harness::Answer::Allow | crate::harness::Answer::Always => format!("Allowed {}.", call.text),
            crate::harness::Answer::Deny => format!("Denied {}; it stops and waits for you.", call.text),
        })
    }

    fn open_calls(&self, ctx: &crate::harness::Ctx, s: &crate::model::Session) -> Vec<String> {
        record(self, ctx, s).map(|p| open_calls(&p)).unwrap_or_default()
    }

    /// Its Bash tool's shells, background ones included, are the session process's children.
    fn runs_its_commands(&self) -> bool {
        true
    }
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
    let pid = s.pid?;
    let reg: Value = serde_json::from_str(&std::fs::read_to_string(ctx.account.home_path().join("sessions").join(format!("{pid}.json"))).ok()?).ok()?;
    let field = |k: &str| reg.get(k).and_then(Value::as_str);
    if field("sessionId") != Some(s.id.as_str()) || field("status") != Some("waiting") || field("waitingFor") != Some("permission prompt") {
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
        if let Some(ts) = str_of(&v, "timestamp").as_deref().and_then(parse_rfc3339_ms) {
            info.last_ts = info.last_ts.max(ts);
        }
        // Sessions switch branches; the latest line says which one it's on now.
        if let Some(b) = str_of(&v, "gitBranch").filter(|b| !b.is_empty() && b != "HEAD") {
            info.git_branch = Some(b);
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
        Value::Array(items) => items.iter().filter(|i| i.get("type").and_then(Value::as_str) == Some("text")).filter_map(|i| i.get("text").and_then(Value::as_str)).collect::<Vec<_>>().join("\n"),
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

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(parse_registry(r#"{"pid":1}"#).is_none());
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
}
