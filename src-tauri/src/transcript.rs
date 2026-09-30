//! A session as it went, for its panel: your prompts, the agent's replies, the tools it ran and
//! what they gave back. Each harness reads its own record (`harness::Adapter::conversation`);
//! this holds what they share: the items, and reading a file of JSON lines a page at a time
//! from the end back, so a session of any length opens at once and scrolls back as far as it
//! goes, and forward from where the last read ended, as the session goes on.
//!
//! A page's cursors are byte offsets at line starts in a transcript or rollout, and message
//! numbers (`seq`) in OpenCode. Claude Code writes a tool's result as a line of its own after
//! the call; the result comes as an item of its own with the call's id, and the panel puts the
//! two together, whichever page each came in.

use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use anyhow::Result;
use serde::Serialize;
use serde_json::Value;

use crate::config::Config;
use crate::harness::{adapter, find, Ctx};
use crate::model::Board;
use crate::util::{parse_rfc3339_ms, truncate};

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
    /// A shell command's call whose `input` is the whole command: its panel can run it again in
    /// a new terminal (runcode.rs).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub shell: bool,
    /// The folder a shell command ran in, when its record says.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    /// A prompt of yours the session took back before its agent answered, so the agent never
    /// saw it: Claude Code does that when a turn is stopped before any reply (claude.rs).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub withdrawn: bool,
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

/// Turns one line of a harness's record into items, given the line's time.
pub type Parse = fn(&Value, i64) -> Vec<Item>;

/// Up to about `limit` items of a session's conversation before `before` (from the newest when none).
pub fn page(board: &Board, cfg: &Config, key: &str, before: Option<u64>, limit: usize) -> Result<Page> {
    let (s, account) = find(board, cfg, key)?;
    adapter(s.harness).conversation(&Ctx { account }, s, before, limit)
}

/// What came after `after`, oldest first.
pub fn newer(board: &Board, cfg: &Config, key: &str, after: u64) -> Result<Page> {
    let (s, account) = find(board, cfg, key)?;
    adapter(s.harness).conversation_after(&Ctx { account }, s, after)
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

/// A page of a JSON-lines record (a transcript, a rollout) before `before`, whole lines at a time.
pub fn file_page(path: &Path, parse: Parse, before: Option<u64>, limit: usize) -> Result<Page> {
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
        let items = if line.len() > LINE_MAX { Vec::new() } else { items_of(line, parse, start) };
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

/// The whole lines of a JSON-lines record after `after`.
pub fn file_newer(path: &Path, parse: Parse, after: u64) -> Result<Page> {
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
            items.extend(items_of(line, parse, at));
        }
        at += line.len() as u64 + 1;
    }
    Ok(Page { items, before: None, after: end })
}

fn items_of(line: &[u8], parse: Parse, cursor: u64) -> Vec<Item> {
    let Ok(v) = serde_json::from_slice::<Value>(line) else { return Vec::new() };
    let at = v.get("timestamp").and_then(Value::as_str).and_then(parse_rfc3339_ms).unwrap_or(0);
    let mut out = parse(&v, at);
    for (n, it) in out.iter_mut().enumerate() {
        it.cursor = cursor;
        it.n = n as u32;
    }
    out
}

pub fn item(at: i64, kind: &'static str, text: impl Into<String>) -> Item {
    Item { at, kind, text: text.into(), id: None, input: None, output: None, failed: false, shell: false, cwd: None, withdrawn: false, cursor: 0, n: 0 }
}

/// The tool calls among `items` that have no result yet, oldest first, for records that keep a
/// call's output apart from it (`result`, by `id`). A page from the end of the record holds a
/// call's result whenever it holds the call.
pub fn open_tools(items: &[Item]) -> Vec<&Item> {
    let answered: std::collections::HashSet<&str> = items.iter().filter(|i| i.kind == "result").filter_map(|i| i.id.as_deref()).collect();
    items.iter().filter(|i| i.kind == "tool" && i.id.as_deref().is_some_and(|id| !answered.contains(id))).collect()
}

/// Numbers a message's items and gives them its place, for records that aren't lines.
pub fn placed(mut items: Vec<Item>, cursor: u64) -> Vec<Item> {
    for (n, it) in items.iter_mut().enumerate() {
        it.cursor = cursor;
        it.n = n as u32;
    }
    items
}

/// Keeps the end of long output, where a command's result usually is.
pub fn clip_output(s: &str) -> String {
    let s = s.trim_end();
    if s.chars().count() <= OUTPUT_MAX {
        return s.to_string();
    }
    let tail: String = s.chars().rev().take(OUTPUT_MAX).collect::<Vec<_>>().into_iter().rev().collect();
    format!("…\n{}", tail.trim_start_matches(|c| c != '\n').trim_start())
}

pub fn clip_input(s: &str) -> String {
    truncate(s.trim(), INPUT_MAX)
}

/// Whether `clip_input` keeps all of `s`: a command cut short isn't one to run.
pub fn kept_whole(s: &str) -> bool {
    s.trim().chars().count() <= INPUT_MAX
}

/// The text of a content value: a string, or text blocks joined.
pub fn text_of(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Array(parts) => parts.iter().filter_map(|p| p.get("text").and_then(Value::as_str)).collect::<Vec<_>>().join("\n"),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// A page goes to other devices as it is; the phone app reads it with codealong-peer's `Page`.
    #[test]
    fn pages_read_back_on_other_devices() {
        let item = |kind: &'static str, id: Option<&str>, output: Option<&str>| Item { at: 7, kind, text: format!("{kind} text"), id: id.map(String::from), input: output.map(|_| "cargo test".to_string()), output: output.map(String::from), failed: kind == "result", shell: false, cwd: None, withdrawn: false, cursor: 40, n: 2 };
        let page = Page { items: vec![Item { withdrawn: true, ..item("you", None, None) }, item("agent", None, None), item("tool", Some("t1"), None), item("result", Some("t1"), Some("ok")), item("note", None, None)], before: Some(12), after: 900 };
        let theirs: codealong_peer::api::Page = serde_json::from_value(serde_json::to_value(&page).unwrap()).unwrap();
        assert_eq!((theirs.before, theirs.after, theirs.items.len()), (Some(12), 900, 5));
        let r = &theirs.items[3];
        assert_eq!((r.kind.as_str(), r.id.as_deref(), r.output.as_deref(), r.failed, r.cursor, r.n), ("result", Some("t1"), Some("ok"), true, 40, 2));
        assert_eq!(serde_json::to_value(&theirs).unwrap(), serde_json::to_value(&page).unwrap(), "nothing lost on the way");
        // A command runs again only on the Mac it ran on, so the other devices read past its folder.
        let command = Item { shell: true, cwd: Some("/Users/me/app".into()), ..item("tool", Some("t2"), Some("ok")) };
        let theirs: codealong_peer::api::Page = serde_json::from_value(serde_json::to_value(Page { items: vec![command], before: None, after: 0 }).unwrap()).unwrap();
        assert_eq!(theirs.items[0].input.as_deref(), Some("cargo test"));
    }

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
    const CALL: &str = r#"{"type":"assistant","cwd":"/Users/me/app","timestamp":"2026-09-26T10:00:01Z","message":{"content":[{"type":"text","text":"Running them."},{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"cargo test"}}]}}"#;
    const RESULT: &str = r#"{"type":"user","timestamp":"2026-09-26T10:00:05Z","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":"test result: ok","is_error":false}]}}"#;
    const REPLY: &str = r#"{"type":"assistant","timestamp":"2026-09-26T10:00:06Z","message":{"content":[{"type":"text","text":"All pass."}]}}"#;

    #[test]
    fn a_claude_transcript_reads_as_prompts_replies_and_tools() {
        let p = write(&[PROMPT, CALL, RESULT, REPLY]);
        let page = file_page(&p, crate::claude::conversation_items, None, 100).unwrap();
        let kinds: Vec<&str> = page.items.iter().map(|i| i.kind).collect();
        assert_eq!(kinds, ["you", "agent", "tool", "result", "agent"]);
        assert_eq!(page.items[2].text, "Bash · `cargo test`");
        assert_eq!(page.items[2].input.as_deref(), Some("cargo test"));
        // A command can run again where it ran: the line's folder is the shell's.
        assert_eq!((page.items[2].shell, page.items[2].cwd.as_deref()), (true, Some("/Users/me/app")));
        assert_eq!((page.items[1].shell, page.items[1].cwd.as_deref()), (false, None));
        assert_eq!((page.items[3].id.as_deref(), page.items[3].output.as_deref()), (Some("t1"), Some("test result: ok")));
        assert_eq!(page.before, None);
        assert_eq!(page.after, std::fs::metadata(&p).unwrap().len());
    }

    #[test]
    fn a_message_sent_while_claude_worked_reads_as_yours() {
        let queued = r#"{"type":"attachment","timestamp":"2026-09-26T10:00:03Z","attachment":{"type":"queued_command","prompt":"Also check the docs"}}"#;
        let task = r#"{"type":"attachment","timestamp":"2026-09-26T10:00:04Z","attachment":{"type":"queued_command","prompt":"<task-notification>\n<status>completed</status>\n</task-notification>"}}"#;
        let other = r#"{"type":"attachment","timestamp":"2026-09-26T10:00:04Z","attachment":{"type":"edited_text_file","filename":"a.rs"}}"#;
        let p = write(&[PROMPT, CALL, queued, task, other, RESULT]);
        let page = file_page(&p, crate::claude::conversation_items, None, 100).unwrap();
        let kinds: Vec<&str> = page.items.iter().map(|i| i.kind).collect();
        assert_eq!(kinds, ["you", "agent", "tool", "you", "result"]);
        assert_eq!(page.items[3].text, "Also check the docs");
    }

    /// Another session's message as Claude Code 2.1.28x writes it: on a `user` line when it
    /// starts a turn, on a `queued_command` attachment when a busy turn takes it in.
    fn peer_line(queued: bool, name: &str, body: &str) -> String {
        let envelope = format!("<cross-session-message from-name=\"{name}\">\n{body}\n</cross-session-message>");
        let origin = serde_json::json!({ "kind": "peer", "from": "unknown", "name": name, "fromMode": "bypass", "body": body });
        let line = if queued {
            serde_json::json!({ "type": "attachment", "timestamp": "2026-09-26T10:00:03Z", "attachment": { "type": "queued_command", "prompt": envelope, "origin": origin } })
        } else {
            serde_json::json!({ "type": "user", "isMeta": true, "timestamp": "2026-09-26T10:00:02Z", "origin": origin, "message": { "role": "user", "content": format!("Another Claude session sent a message:\n{envelope}\n\nThis came from another Claude session.") } })
        };
        line.to_string()
    }

    #[test]
    fn what_you_send_from_codealong_reads_as_yours() {
        let image = crate::attachments::images_dir().join("0123456789abcdef.png");
        let from_you = |text: &str| format!("{}\n\n{text}", crate::send::FROM_YOU);
        let started = peer_line(false, "Codealong", &from_you("Can you commit this?"));
        let queued = peer_line(true, "Codealong", &from_you(&format!("Also this:\n![image]({})", image.display())));
        let agent = peer_line(true, "Review the parser", "This message is from another agent, “Review the parser” (Claude Code on claude2, session k), sent with `codealong session send`; it isn't from your user.\n\nWhich test fails?");
        // A Claude Code that keeps no body has only the envelope.
        let older = r#"{"type":"user","isMeta":true,"timestamp":"2026-09-26T10:00:04Z","origin":{"kind":"peer","from":"review"},"message":{"content":"Another Claude session sent a message:\n<cross-session-message from-name=\"review\">\nLooks good\n</cross-session-message>"}}"#;
        // A subagent's report isn't a message; nor is it yours.
        let handback = r#"{"type":"attachment","timestamp":"2026-09-26T10:00:04Z","attachment":{"type":"queued_command","prompt":"<agent-message from=\"a3c7\">\n[Subagent hand-back] The report follows:\n  Done.\n</agent-message>","origin":{"kind":"peer","from":"a3c7","senderTaskId":"a3c7","handback":true,"body":"[Subagent hand-back] The report follows:\n  Done."}}}"#;
        let p = write(&[PROMPT, &started, CALL, &queued, &agent, older, handback, RESULT]);
        let page = file_page(&p, crate::claude::conversation_items, None, 100).unwrap();
        let said: Vec<(&str, &str)> = page.items.iter().filter(|i| i.kind != "tool" && i.kind != "result").map(|i| (i.kind, i.text.as_str())).collect();
        assert_eq!(
            said,
            [
                ("you", "Run the tests"),
                ("you", "Can you commit this?"),
                ("agent", "Running them."),
                ("you", "Also this:\n![image](images/0123456789abcdef.png)"),
                ("note", "From another session, “Review the parser”: Which test fails?"),
                ("note", "From another session, “review”: Looks good"),
            ]
        );
    }

    #[test]
    fn pages_go_back_a_line_at_a_time_and_meet_in_the_middle() {
        let p = write(&[PROMPT, CALL, RESULT, REPLY]);
        let last = file_page(&p, crate::claude::conversation_items, None, 2).unwrap();
        assert_eq!(last.items.iter().map(|i| i.kind).collect::<Vec<_>>(), ["result", "agent"]);
        let before = last.before.expect("more to go");
        let rest = file_page(&p, crate::claude::conversation_items, Some(before), 100).unwrap();
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
        let first = file_page(&p, crate::claude::conversation_items, None, 10).unwrap();
        // A line still being written isn't read until it's whole.
        let mut f = std::fs::OpenOptions::new().append(true).open(&p).unwrap();
        std::io::Write::write_all(&mut f, format!("{REPLY}\n{}", &CALL[..20]).as_bytes()).unwrap();
        let next = file_newer(&p, crate::claude::conversation_items, first.after).unwrap();
        assert_eq!(next.items.iter().map(|i| i.text.as_str()).collect::<Vec<_>>(), ["All pass."]);
        assert_eq!(file_newer(&p, crate::claude::conversation_items, next.after).unwrap().items, Vec::<Item>::new());
    }

    #[test]
    fn codex_commands_carry_their_output_and_exit() {
        let cmd = r#"{"timestamp":"2026-09-26T10:00:00Z","type":"event_msg","payload":{"type":"item_completed","item":{"type":"CommandExecution","command":["/bin/zsh","-lc","npm test"],"aggregated_output":"1 failed","exit_code":1},"completed_at_ms":1790000000000}}"#;
        let p = write(&[cmd]);
        let page = file_page(&p, crate::codex::conversation_items, None, 10).unwrap();
        assert_eq!(page.items.len(), 1);
        let it = &page.items[0];
        assert_eq!((it.kind, it.text.as_str(), it.output.as_deref(), it.failed), ("tool", "Shell · `npm test`", Some("1 failed"), true));
    }

    /// Only a whole command a shell ran can run again, in the folder the agent chose for it.
    #[test]
    fn commands_that_can_run_again_say_where_they_ran() {
        let codex = |command: &str| format!(r#"{{"timestamp":"2026-09-26T10:00:00Z","type":"event_msg","payload":{{"type":"item_completed","item":{{"type":"CommandExecution","command":{command},"cwd":"file:///Users/me/my%20app","exit_code":0}}}}}}"#);
        let long = format!(r#"["/bin/zsh","-lc","echo {}"]"#, "x".repeat(INPUT_MAX));
        let lines = [codex(r#"["/bin/zsh","-lc","npm test"]"#), codex(r#"["git","status","--short"]"#), codex(&long)];
        let p = write(&lines.iter().map(String::as_str).collect::<Vec<_>>());
        let page = file_page(&p, crate::codex::conversation_items, None, 10).unwrap();
        let got: Vec<(bool, Option<&str>)> = page.items.iter().map(|i| (i.shell, i.cwd.as_deref())).collect();
        assert_eq!(got, [(true, Some("/Users/me/my app")), (false, None), (false, None)]);
        let bash = |command: &str| serde_json::json!({"type":"assistant","cwd":"/Users/me/app","message":{"content":[{"type":"tool_use","id":"t1","name":"Bash","input":{"command":command}}]}});
        assert!(!crate::claude::conversation_items(&bash(&"x".repeat(INPUT_MAX + 1)), 0)[0].shell, "cut short");
        assert!(!crate::claude::conversation_items(&bash("  "), 0)[0].shell, "empty");
        let message = serde_json::json!({"content":[
            {"type":"tool","name":"shell","state":{"status":"completed","input":{"command":"ls -la"},"output":"ok"}},
            {"type":"tool","name":"read","state":{"status":"completed","input":{"filePath":"/a.rs"}}},
        ]});
        let items = crate::opencode::conversation_items(&message, 3, 0);
        let got: Vec<(bool, Option<&str>)> = items.iter().map(|i| (i.shell, i.cwd.as_deref())).collect();
        assert_eq!(got, [(true, None), (false, None)]);
    }

    #[test]
    fn long_output_keeps_its_end() {
        let long = format!("{}\nthe end", "x\n".repeat(OUTPUT_MAX));
        let clipped = clip_output(&long);
        assert!(clipped.starts_with('…') && clipped.ends_with("the end") && clipped.chars().count() <= OUTPUT_MAX + 2);
    }
}
