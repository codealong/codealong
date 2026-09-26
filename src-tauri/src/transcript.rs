//! A session as it went, for its panel: your prompts, the agent's replies, the tools it ran and
//! what they gave back, read from the session's own record: Claude Code's transcript, Codex's
//! rollout, OpenCode's database. Read a page at a time from the end back, so a session of any
//! length opens at once and scrolls back as far as it goes; and forward from where the last
//! read ended, as the session goes on.
//!
//! A page's cursors are byte offsets at line starts in a transcript or rollout, and message
//! numbers (`seq`) in OpenCode. Claude Code writes a tool's result as a line of its own after
//! the call; the result comes as an item of its own with the call's id, and the panel puts the
//! two together, whichever page each came in.

use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::{anyhow, Result};
use serde::Serialize;
use serde_json::Value;

use crate::config::Config;
use crate::model::Board;
use crate::util::{code, one_line, parse_rfc3339_ms, truncate, without_cd};
use crate::{claude, codex, opencode};

/// What a tool's input and output keep: enough to read, not a build log.
const INPUT_MAX: usize = 4000;
const OUTPUT_MAX: usize = 6000;
const CHUNK: u64 = 256 * 1024;
/// Lines past this are skipped rather than parsed: a pasted image or a huge tool output.
const LINE_MAX: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub at: i64,
    /// `you`, `agent`, `tool` (a call, with its output when the record has it there), `result`
    /// (a call's output, by `id`) or `note` (compaction, a message from another session…).
    pub kind: &'static str,
    /// Prose for you, agent and note; for a tool, a one-line label such as "Bash · `cargo test`".
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// A tool's whole input: the command, the file, the arguments.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    pub failed: bool,
    /// Where its line starts (or its message number); with `n`, unique in the session.
    pub cursor: u64,
    pub n: u32,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PageOf<T> {
    /// Oldest first.
    pub items: Vec<T>,
    /// Where the next page back ends; none at the session's start.
    pub before: Option<u64>,
    /// Where reading on (for what comes next) starts.
    pub after: u64,
}

pub type Page = PageOf<Item>;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Kind {
    Claude,
    Codex,
}

enum Source {
    File(PathBuf, Kind),
    OpenCode { db: PathBuf, session: String },
}

/// Where each session's record is, once found: finding a transcript scans every project folder.
fn found() -> &'static Mutex<Vec<(String, PathBuf)>> {
    static FOUND: Mutex<Vec<(String, PathBuf)>> = Mutex::new(Vec::new());
    &FOUND
}

fn source(board: &Board, cfg: &Config, key: &str) -> Result<Source> {
    let s = board.lanes.iter().flat_map(|l| &l.sessions).find(|s| s.key == key).ok_or_else(|| anyhow!("That session isn't on the board any more."))?;
    let account = cfg.accounts.iter().find(|a| a.name == s.account).ok_or_else(|| anyhow!("There's no account named {} in config.toml.", s.account))?;
    let kind = match account.harness.as_str() {
        "claude" => Kind::Claude,
        "codex" => Kind::Codex,
        "opencode" => return Ok(Source::OpenCode { db: account.home_path().join("opencode.db"), session: s.id.clone() }),
        other => return Err(anyhow!("Codealong can't read {other} conversations yet.")),
    };
    if let Some((_, p)) = found().lock().unwrap_or_else(|e| e.into_inner()).iter().find(|(k, p)| k == key && p.is_file()) {
        return Ok(Source::File(p.clone(), kind));
    }
    let path = crate::cli::records_for(account, s).into_iter().next().map(PathBuf::from).ok_or_else(|| anyhow!("Its transcript isn't on this Mac any more."))?;
    found().lock().unwrap_or_else(|e| e.into_inner()).push((key.to_string(), path.clone()));
    Ok(Source::File(path, kind))
}

/// Up to about `limit` items before `before` (from the end when none), whole lines at a time.
pub fn page(board: &Board, cfg: &Config, key: &str, before: Option<u64>, limit: usize) -> Result<Page> {
    match source(board, cfg, key)? {
        Source::File(path, kind) => file_page(&path, kind, before, limit),
        Source::OpenCode { db, session } => Ok(opencode::transcript_page(&db, &session, before, limit, opencode_items)),
    }
}

/// What came after `after`, oldest first.
pub fn newer(board: &Board, cfg: &Config, key: &str, after: u64) -> Result<Page> {
    match source(board, cfg, key)? {
        Source::File(path, kind) => file_newer(&path, kind, after),
        Source::OpenCode { db, session } => Ok(opencode::transcript_newer(&db, &session, after, opencode_items)),
    }
}

/// The end of the last whole line: a line still being written waits for the next read.
fn whole_end(f: &mut std::fs::File, len: u64) -> std::io::Result<u64> {
    let mut end = len;
    let mut byte = [0u8; 1];
    while end > 0 {
        f.seek(SeekFrom::Start(end - 1))?;
        f.read_exact(&mut byte)?;
        if byte[0] == b'\n' {
            return Ok(end);
        }
        // Look back a chunk at a time for the last newline.
        let lo = end.saturating_sub(CHUNK);
        let mut buf = vec![0; (end - lo) as usize];
        f.seek(SeekFrom::Start(lo))?;
        f.read_exact(&mut buf)?;
        match buf.iter().rposition(|&b| b == b'\n') {
            Some(i) => return Ok(lo + i as u64 + 1),
            None => end = lo,
        }
    }
    Ok(0)
}

fn file_page(path: &Path, kind: Kind, before: Option<u64>, limit: usize) -> Result<Page> {
    let mut f = std::fs::File::open(path)?;
    let len = f.metadata()?.len();
    let end = whole_end(&mut f, len)?;
    let after = end;
    // `hi` is always a line start; `buf` holds the bytes from `lo` up to it, so when it isn't
    // empty it ends with the newline of the line before `hi`.
    let mut hi = before.unwrap_or(end).min(end);
    let mut lo = hi;
    let mut buf: Vec<u8> = Vec::new();
    let mut newest_first: Vec<Vec<Item>> = Vec::new();
    let mut count = 0;
    let take = |line: &[u8], start: u64, out: &mut Vec<Vec<Item>>| {
        let items = if line.len() > LINE_MAX { Vec::new() } else { items_of(line, kind, start) };
        let n = items.len();
        out.push(items);
        n
    };
    loop {
        if let Some((_, body)) = buf.split_last() {
            if let Some(i) = body.iter().rposition(|&b| b == b'\n') {
                let start = lo + i as u64 + 1;
                count += take(&body[i + 1..], start, &mut newest_first);
                buf.truncate(i + 1);
                hi = start;
                if count >= limit {
                    break;
                }
                continue;
            }
            if lo == 0 {
                take(body, 0, &mut newest_first);
                hi = 0;
                break;
            }
        } else if lo == 0 {
            hi = 0;
            break;
        }
        // The line before `hi` starts further back: read another chunk.
        let next = lo.saturating_sub(CHUNK);
        let mut chunk = vec![0; (lo - next) as usize];
        f.seek(SeekFrom::Start(next))?;
        f.read_exact(&mut chunk)?;
        chunk.extend_from_slice(&buf);
        buf = chunk;
        lo = next;
    }
    let items = newest_first.into_iter().rev().flatten().collect();
    Ok(Page { items, before: (hi > 0).then_some(hi), after })
}

fn file_newer(path: &Path, kind: Kind, after: u64) -> Result<Page> {
    let mut f = std::fs::File::open(path)?;
    let len = f.metadata()?.len();
    let end = whole_end(&mut f, len)?;
    if end <= after {
        return Ok(Page { items: Vec::new(), before: None, after: end.max(after) });
    }
    let mut buf = Vec::with_capacity((end - after) as usize);
    f.seek(SeekFrom::Start(after))?;
    (&mut f).take(end - after).read_to_end(&mut buf)?;
    let mut items = Vec::new();
    let mut at = after;
    for line in buf.split(|&b| b == b'\n') {
        if !line.is_empty() && line.len() <= LINE_MAX {
            items.extend(items_of(line, kind, at));
        }
        at += line.len() as u64 + 1;
    }
    Ok(Page { items, before: None, after: end })
}

fn items_of(line: &[u8], kind: Kind, cursor: u64) -> Vec<Item> {
    let Ok(v) = serde_json::from_slice::<Value>(line) else { return Vec::new() };
    let at = v.get("timestamp").and_then(Value::as_str).and_then(parse_rfc3339_ms).unwrap_or(0);
    let mut out = match kind {
        Kind::Claude => claude_items(&v, at),
        Kind::Codex => codex_items(&v, at),
    };
    for (n, it) in out.iter_mut().enumerate() {
        it.cursor = cursor;
        it.n = n as u32;
    }
    out
}

fn item(at: i64, kind: &'static str, text: impl Into<String>) -> Item {
    Item { at, kind, text: text.into(), id: None, input: None, output: None, failed: false, cursor: 0, n: 0 }
}

/// Keeps the end of long output, where a command's result usually is.
fn clip_output(s: &str) -> String {
    let s = s.trim_end();
    if s.chars().count() <= OUTPUT_MAX {
        return s.to_string();
    }
    let tail: String = s.chars().rev().take(OUTPUT_MAX).collect::<Vec<_>>().into_iter().rev().collect();
    format!("…\n{}", tail.trim_start_matches(|c| c != '\n').trim_start())
}

fn clip_input(s: &str) -> String {
    truncate(s.trim(), INPUT_MAX)
}

/// The text of a content value: a string, or text blocks joined.
fn text_of(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Array(parts) => parts.iter().filter_map(|p| p.get("text").and_then(Value::as_str)).collect::<Vec<_>>().join("\n"),
        _ => String::new(),
    }
}

fn claude_items(v: &Value, at: i64) -> Vec<Item> {
    let flag = |k: &str| v.get(k).and_then(Value::as_bool) == Some(true);
    if flag("isSidechain") {
        return Vec::new();
    }
    match v.get("type").and_then(Value::as_str) {
        Some("user") => {
            if flag("isCompactSummary") {
                return vec![item(at, "note", "The conversation was compacted here")];
            }
            if let Some(t) = claude::typed_text(v) {
                return vec![item(at, "you", t)];
            }
            if flag("isMeta") {
                return Vec::new();
            }
            if v.pointer("/origin/kind").and_then(Value::as_str) == Some("peer") {
                let t = text_of(v.pointer("/message/content").unwrap_or(&Value::Null));
                return vec![item(at, "note", format!("From another session: {}", one_line(&t, 200)))];
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
                        out.push(Item { id: p.get("id").and_then(Value::as_str).map(String::from), input: Some(clip_input(&claude_input(name, &input))), ..item(at, "tool", claude::tool_request(name, &input)) });
                    }
                    _ => {}
                }
            }
            out
        }
        _ => Vec::new(),
    }
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
        _ => serde_json::to_string_pretty(input).unwrap_or_default(),
    }
}

fn codex_items(v: &Value, at: i64) -> Vec<Item> {
    if let Some((you, t)) = codex::prose_of(v) {
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
                    let exit = it.get("exit_code").and_then(Value::as_i64);
                    vec![Item {
                        input: Some(clip_input(&script)),
                        output: Some(clip_output(&s("aggregated_output"))),
                        failed: exit.is_some_and(|c| c != 0),
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

/// An OpenCode message's items: a user's text, or an assistant's text and tool parts.
fn opencode_items(v: &Value, seq: u64, at: i64) -> Vec<Item> {
    let mut out = Vec::new();
    if let Some(t) = v.get("text").and_then(Value::as_str) {
        if !t.trim().is_empty() {
            out.push(item(at, "you", t.trim()));
        }
    }
    for p in v.get("content").and_then(Value::as_array).into_iter().flatten() {
        match p.get("type").and_then(Value::as_str) {
            Some("text") => {
                let t = p.get("text").and_then(Value::as_str).unwrap_or("").trim();
                if !t.is_empty() {
                    out.push(item(at, "agent", t));
                }
            }
            Some("tool") => {
                let name = p.get("name").and_then(Value::as_str).unwrap_or("tool");
                let state = p.get("state");
                let input = state.and_then(|s| s.get("input"));
                let output = state.and_then(|s| s.get("output")).map(|o| match o {
                    Value::String(s) => s.clone(),
                    other => serde_json::to_string_pretty(other).unwrap_or_default(),
                });
                let failed = state.and_then(|s| s.get("status")).and_then(Value::as_str) == Some("error");
                let command = input.and_then(|i| i.get("command")).and_then(Value::as_str);
                let label = match command {
                    Some(c) => format!("{name} · {}", code(&one_line(c, 80))),
                    None => name.to_string(),
                };
                out.push(Item {
                    input: input.map(|i| clip_input(&command.map(String::from).unwrap_or_else(|| serde_json::to_string_pretty(i).unwrap_or_default()))),
                    output: output.map(|o| clip_output(&o)),
                    failed,
                    ..item(at, "tool", label)
                });
            }
            _ => {}
        }
    }
    for (n, it) in out.iter_mut().enumerate() {
        it.cursor = seq;
        it.n = n as u32;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(lines: &[&str]) -> PathBuf {
        // Tests run at once: each gets a folder of its own.
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("codealong-transcript-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("t.jsonl");
        std::fs::write(&p, lines.iter().map(|l| format!("{l}\n")).collect::<String>()).unwrap();
        p
    }

    const PROMPT: &str = r#"{"type":"user","origin":{"kind":"human"},"timestamp":"2026-09-26T10:00:00Z","message":{"role":"user","content":"Run the tests"}}"#;
    const CALL: &str = r#"{"type":"assistant","timestamp":"2026-09-26T10:00:01Z","message":{"content":[{"type":"text","text":"Running them."},{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"cargo test"}}]}}"#;
    const RESULT: &str = r#"{"type":"user","timestamp":"2026-09-26T10:00:05Z","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":"test result: ok","is_error":false}]}}"#;
    const REPLY: &str = r#"{"type":"assistant","timestamp":"2026-09-26T10:00:06Z","message":{"content":[{"type":"text","text":"All pass."}]}}"#;

    #[test]
    fn a_claude_transcript_reads_as_prompts_replies_and_tools() {
        let p = write(&[PROMPT, CALL, RESULT, REPLY]);
        let page = file_page(&p, Kind::Claude, None, 100).unwrap();
        let kinds: Vec<&str> = page.items.iter().map(|i| i.kind).collect();
        assert_eq!(kinds, ["you", "agent", "tool", "result", "agent"]);
        assert_eq!(page.items[2].text, "Bash · `cargo test`");
        assert_eq!(page.items[2].input.as_deref(), Some("cargo test"));
        assert_eq!((page.items[3].id.as_deref(), page.items[3].output.as_deref()), (Some("t1"), Some("test result: ok")));
        assert_eq!(page.before, None);
        assert_eq!(page.after, std::fs::metadata(&p).unwrap().len());
    }

    #[test]
    fn pages_go_back_a_line_at_a_time_and_meet_in_the_middle() {
        let p = write(&[PROMPT, CALL, RESULT, REPLY]);
        let last = file_page(&p, Kind::Claude, None, 2).unwrap();
        assert_eq!(last.items.iter().map(|i| i.kind).collect::<Vec<_>>(), ["result", "agent"]);
        let before = last.before.expect("more to go");
        let rest = file_page(&p, Kind::Claude, Some(before), 100).unwrap();
        assert_eq!(rest.items.iter().map(|i| i.kind).collect::<Vec<_>>(), ["you", "agent", "tool"]);
        assert_eq!(rest.before, None);
        // Every item's place is unique across the pages.
        let mut places: Vec<(u64, u32)> = rest.items.iter().chain(&last.items).map(|i| (i.cursor, i.n)).collect();
        places.dedup();
        assert_eq!(places.len(), 5);
    }

    #[test]
    fn reading_on_takes_only_whole_new_lines() {
        let p = write(&[PROMPT]);
        let first = file_page(&p, Kind::Claude, None, 10).unwrap();
        // A line still being written isn't read until it's whole.
        let mut f = std::fs::OpenOptions::new().append(true).open(&p).unwrap();
        std::io::Write::write_all(&mut f, format!("{REPLY}\n{}", &CALL[..20]).as_bytes()).unwrap();
        let next = file_newer(&p, Kind::Claude, first.after).unwrap();
        assert_eq!(next.items.iter().map(|i| i.text.as_str()).collect::<Vec<_>>(), ["All pass."]);
        assert_eq!(file_newer(&p, Kind::Claude, next.after).unwrap().items, Vec::<Item>::new());
    }

    #[test]
    fn codex_commands_carry_their_output_and_exit() {
        let cmd = r#"{"timestamp":"2026-09-26T10:00:00Z","type":"event_msg","payload":{"type":"item_completed","item":{"type":"CommandExecution","command":["/bin/zsh","-lc","npm test"],"aggregated_output":"1 failed","exit_code":1},"completed_at_ms":1790000000000}}"#;
        let p = write(&[cmd]);
        let page = file_page(&p, Kind::Codex, None, 10).unwrap();
        assert_eq!(page.items.len(), 1);
        let it = &page.items[0];
        assert_eq!((it.kind, it.text.as_str(), it.output.as_deref(), it.failed), ("tool", "Shell · `npm test`", Some("1 failed"), true));
    }

    #[test]
    fn long_output_keeps_its_end() {
        let long = format!("{}\nthe end", "x\n".repeat(OUTPUT_MAX));
        let clipped = clip_output(&long);
        assert!(clipped.starts_with('…') && clipped.ends_with("the end") && clipped.chars().count() <= OUTPUT_MAX + 2);
    }
}
