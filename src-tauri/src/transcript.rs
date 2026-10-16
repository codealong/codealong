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
//!
//! A prompt of yours reads as you wrote it (`shown`): the files Codealong sent with it by their
//! references again, and the images it came with from elsewhere (a screenshot pasted into the
//! terminal, one attached in the harness's app), which the record holds, kept with Codealong's
//! own images and referred to the same way, so the panel shows them all in place.

use std::collections::BTreeMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use anyhow::Result;
use serde::Serialize;
use serde_json::Value;

use crate::attachments;
use crate::config::Config;
use crate::harness::{adapter, find, Ctx};
use crate::model::Board;
use crate::util::{parse_rfc3339_ms, truncate, whole_end};

/// What a tool's input and output keep: enough to read, not a build log.
const INPUT_MAX: usize = 4000;
const OUTPUT_MAX: usize = 6000;
const CHUNK: u64 = 256 * 1024;
/// Lines past this are skipped rather than parsed: a pasted image or a huge tool output.
const LINE_MAX: usize = 16 * 1024 * 1024;

/// The kind of an item that only carries a prompt's images, for a record that keeps them on a
/// line of its own ahead of the prompt (Codex). `shown` hands them to the prompt that follows;
/// no such item leaves the engine.
pub const IMAGES: &str = "images";

/// An image a prompt came with, as the session's record has it.
#[derive(Debug, Clone, PartialEq)]
pub struct Image {
    /// Its number where the prompt's text names it (`[Image #2]`).
    pub number: Option<u32>,
    /// Its bytes in base64, as the record holds them.
    pub base64: String,
}

impl Image {
    /// From a `data:image/png;base64,…` URL, which is how Codex records one.
    pub fn from_data_url(url: &str) -> Option<Image> {
        let (kind, base64) = url.strip_prefix("data:image/")?.split_once(',')?;
        kind.ends_with(";base64").then(|| Image { number: None, base64: base64.to_string() })
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub at: i64,
    /// `you`, `agent`, `tool` (a call, with its output when the record has it there), `result`
    /// (a call's output, by `id`), `codealong` (what Codealong sent by itself: a PR's news, a
    /// review handed back…), `peer` (another session's message, whole) or `note` (compaction,
    /// a message held for you to allow…).
    pub kind: &'static str,
    /// Prose for you, agent, peer and note; for a tool, a one-line label such as "Bash · `cargo test`".
    pub text: String,
    /// Who sent a `peer` message: the sending session's name, where its record says.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
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
    /// saw it: Claude Code does that when a turn is stopped before any reply (claude/conversation.rs).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub withdrawn: bool,
    /// The questions a call asks you (Claude Code's `AskUserQuestion`), so the panel shows them
    /// as questions rather than the call's input.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub questions: Vec<crate::harness::Question>,
    /// Your answers to them, on the call's result: each question's text to what you picked or
    /// wrote, several where it took more.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub answers: BTreeMap<String, Vec<String>>,
    /// What a call made or opened that has a page on the web (an artifact Claude Code
    /// published), on the call's result: its line in the panel links to it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link: Option<crate::harness::Link>,
    /// The files a call handed you to look at (Claude Code's `SendUserFile`), on the call: its
    /// line in the panel lists them, and a click opens one on the session's Mac.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sent: Option<crate::harness::SentFiles>,
    /// The images a prompt of yours came with, as its record has them, until `shown` keeps them
    /// and puts them in its text.
    #[serde(skip)]
    pub images: Vec<Image>,
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
    Ok(shown(adapter(s.harness).conversation(&Ctx { account }, s, before, limit)?))
}

/// What came after `after`, oldest first.
pub fn newer(board: &Board, cfg: &Config, key: &str, after: u64) -> Result<Page> {
    let (s, account) = find(board, cfg, key)?;
    Ok(shown(adapter(s.harness).conversation_after(&Ctx { account }, s, after)?))
}

/// A page as a window shows it: each prompt of yours as you wrote it, with its images, and what
/// Codealong sent by itself (send.rs `tell`) or another agent sent (`session send`), typed in
/// where the session had no other way to take it, as Codealong's (`codealong`) or that agent's
/// (`peer`), not yours.
fn shown(page: Page) -> Page {
    shown_in(page, &attachments::images_dir(), &attachments::files_dir())
}

fn shown_in(mut page: Page, images: &Path, files: &Path) -> Page {
    // Images on a line of their own are the next prompt's.
    let mut carried: Vec<Image> = Vec::new();
    for it in page.items.iter_mut() {
        let before = std::mem::take(&mut carried);
        if it.kind == IMAGES {
            carried = std::mem::take(&mut it.images);
            continue;
        }
        if it.kind != "you" {
            continue;
        }
        if let Some(said) = crate::send::from_codealong(&it.text) {
            it.text = attachments::written_in(said, images, files);
            it.kind = "codealong";
            continue;
        }
        if let Some(said) = crate::send::from_an_agent(&it.text) {
            let from = crate::send::sender_of(&it.text).map(String::from);
            it.text = attachments::written_in(said, images, files);
            it.from = from;
            it.kind = "peer";
            continue;
        }
        let own = std::mem::take(&mut it.images);
        let with = if before.is_empty() { own } else { before };
        // One that isn't an image a window shows is left out, and the text keeps its mark for it.
        let kept: Vec<(Option<u32>, String)> = with.iter().filter_map(|i| Some((i.number, attachments::keep_image_in(images, &data_encoding::BASE64.decode(i.base64.as_bytes()).ok()?)?))).collect();
        it.text = attachments::with_images(&attachments::written_in(&it.text, images, files), &kept);
    }
    // A prompt that was only images, none of them to show, has nothing to say.
    page.items.retain(|it| it.kind != IMAGES && !(it.kind == "you" && it.text.is_empty()));
    page
}

/// A page of a JSON-lines record (a transcript, a rollout) before `before`, whole lines at a time.
pub fn file_page(path: &Path, parse: Parse, before: Option<u64>, limit: usize) -> Result<Page> {
    let mut f = std::fs::File::open(path)?;
    let len = f.metadata()?.len();
    // A line still being written waits for the next read.
    let end = whole_end(&mut f, 0, len)?;
    let after = end;
    // Always a line start: the lines from here on are read.
    let mut hi = before.unwrap_or(end).min(end);
    let mut back = Backward { f: &mut f, lo: 0, chunk: Vec::new() };
    let mut newest_first: Vec<Vec<Item>> = Vec::new();
    let mut count = 0;
    while hi > 0 && count < limit {
        let start = back.line_start(hi)?;
        // The line without its newline.
        let items = if hi - 1 - start > LINE_MAX as u64 { Vec::new() } else { items_of(&back.bytes(start, hi - 1)?, parse, start) };
        count += items.len();
        newest_first.push(items);
        hi = start;
    }
    let items = newest_first.into_iter().rev().flatten().collect();
    Ok(Page { items, before: (hi > 0).then_some(hi), after })
}

/// A record read from the end back a chunk at a time, for where its lines start. Only the last
/// chunk is kept, and a line is read whole only once both its ends are found, and only when
/// it's short enough to parse (`LINE_MAX`): a line of any length costs a chunk of memory to
/// pass over.
struct Backward<'a> {
    f: &'a mut std::fs::File,
    /// The last chunk read, and where it starts in the file.
    lo: u64,
    chunk: Vec<u8>,
}

impl Backward<'_> {
    /// Where the line that ends at `hi` starts (`hi`, above 0, starts the line after it): just
    /// after the newline before that line's own, or the file's start.
    fn line_start(&mut self, hi: u64) -> std::io::Result<u64> {
        // What's before this has yet to be looked through.
        let mut end = hi - 1;
        loop {
            if end > self.lo && end <= self.lo + self.chunk.len() as u64 {
                if let Some(i) = self.chunk[..(end - self.lo) as usize].iter().rposition(|&b| b == b'\n') {
                    return Ok(self.lo + i as u64 + 1);
                }
                end = self.lo;
            }
            if end == 0 {
                return Ok(0);
            }
            let lo = end.saturating_sub(CHUNK);
            self.chunk.resize((end - lo) as usize, 0);
            self.f.seek(SeekFrom::Start(lo))?;
            self.f.read_exact(&mut self.chunk)?;
            self.lo = lo;
        }
    }

    /// The bytes from `start` to `end`: from the chunk when they're in it, else read.
    fn bytes(&mut self, start: u64, end: u64) -> std::io::Result<std::borrow::Cow<'_, [u8]>> {
        if start >= self.lo && end <= self.lo + self.chunk.len() as u64 {
            return Ok(std::borrow::Cow::Borrowed(&self.chunk[(start - self.lo) as usize..(end - self.lo) as usize]));
        }
        let mut line = vec![0; (end - start) as usize];
        self.f.seek(SeekFrom::Start(start))?;
        self.f.read_exact(&mut line)?;
        Ok(std::borrow::Cow::Owned(line))
    }
}

/// The whole lines of a JSON-lines record after `after`, read a chunk at a time: a line is kept
/// only while it's short enough to parse (`LINE_MAX`), so what's unread, however long, costs no
/// more than that to read on through. A read that starts inside a line drops that line, which
/// belongs to whoever read up to there, and a last line with no newline yet is left for the
/// next read.
pub fn file_newer(path: &Path, parse: Parse, after: u64) -> Result<Page> {
    let mut f = std::fs::File::open(path)?;
    let len = f.metadata()?.len();
    let from = after.min(len);
    let end = whole_end(&mut f, from, len)?;
    if end <= from {
        return Ok(Page { items: Vec::new(), before: None, after: end.max(after) });
    }
    // Inside a line, or too long a line: its bytes aren't kept.
    let mut dropping = from > 0 && {
        let mut before = [0u8; 1];
        f.seek(SeekFrom::Start(from - 1))?;
        f.read_exact(&mut before)?;
        before[0] != b'\n'
    };
    f.seek(SeekFrom::Start(from))?;
    let mut rest = f.take(end - from);
    let mut chunk = vec![0; CHUNK as usize];
    let mut line: Vec<u8> = Vec::new();
    let mut items = Vec::new();
    // Where the line being read starts.
    let mut at = from;
    let mut pos = from;
    loop {
        let n = match rest.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e.into()),
        };
        let mut part = &chunk[..n];
        while let Some(i) = part.iter().position(|&b| b == b'\n') {
            if !dropping {
                line.extend_from_slice(&part[..i]);
                if !line.is_empty() && line.len() <= LINE_MAX {
                    items.extend(items_of(&line, parse, at));
                }
            }
            line.clear();
            dropping = false;
            pos += i as u64 + 1;
            at = pos;
            part = &part[i + 1..];
        }
        pos += part.len() as u64;
        if !dropping {
            if line.len() + part.len() > LINE_MAX {
                dropping = true;
                line = Vec::new();
            } else {
                line.extend_from_slice(part);
            }
        }
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
    Item { at, kind, text: text.into(), from: None, id: None, input: None, output: None, failed: false, shell: false, cwd: None, withdrawn: false, questions: Vec::new(), answers: BTreeMap::new(), link: None, sent: None, images: Vec::new(), cursor: 0, n: 0 }
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

    #[test]
    fn what_codealong_typed_in_by_itself_shows_as_its_own() {
        let dir = crate::fixtures::scratch("codealong-typed");
        let page = Page { items: vec![item(1, "you", crate::send::by_codealong("On your PR #7: ken approved it.")), item(2, "you", "Thanks, merge it.")], before: None, after: 0 };
        let shown = shown_in(page, &dir.join("images"), &dir.join("files"));
        assert_eq!((shown.items[0].kind, shown.items[0].text.as_str()), ("codealong", "On your PR #7: ken approved it."));
        assert_eq!((shown.items[1].kind, shown.items[1].text.as_str()), ("you", "Thanks, merge it."));
        // A review handed back names its file by its full path as it's typed in, and shows it as a
        // file again, as yours do.
        let (images, files) = (dir.join("images"), dir.join("files"));
        let kept = attachments::keep_in(&images, &files, b"# Review", "review-7.md").unwrap();
        let typed = attachments::expand_in(&format!("Its review is in {}.", kept.reference()), &images, &files, attachments::Style::Path);
        let page = Page { items: vec![item(1, "you", crate::send::by_codealong(&typed))], before: None, after: 0 };
        let shown = shown_in(page, &images, &files);
        assert_eq!((shown.items[0].kind, shown.items[0].text.clone()), ("codealong", format!("Its review is in {}.", kept.reference())));
    }

    #[test]
    fn what_another_agent_typed_in_shows_whole_as_its_own() {
        let dir = crate::fixtures::scratch("agent-typed");
        let byline = "This message is from another agent, “Review the parser” (Claude Code on claude2, session claude:claude2:k), sent with `codealong session send`; it isn't from your user. To answer it, run `codealong session send claude:claude2:k \"<your answer>\"`.";
        let page = Page { items: vec![item(1, "you", format!("{byline}\n\nWhich test fails?\n\nThe parser's, I think.")), item(2, "you", "Thanks, merge it.")], before: None, after: 0 };
        let shown = shown_in(page, &dir.join("images"), &dir.join("files"));
        let said: Vec<(&str, &str, Option<&str>)> = shown.items.iter().map(|i| (i.kind, i.text.as_str(), i.from.as_deref())).collect();
        assert_eq!(said, [("peer", "Which test fails?\n\nThe parser's, I think.", Some("Review the parser")), ("you", "Thanks, merge it.", None)]);
    }
    use std::path::PathBuf;

    /// A page goes to other devices as it is; the phone app reads it with codealong-peer's `Page`.
    #[test]
    fn pages_read_back_on_other_devices() {
        let item = |kind: &'static str, id: Option<&str>, output: Option<&str>| Item { at: 7, kind, text: format!("{kind} text"), from: None, id: id.map(String::from), input: output.map(|_| "cargo test".to_string()), output: output.map(String::from), failed: kind == "result", shell: false, cwd: None, withdrawn: false, questions: Vec::new(), answers: BTreeMap::new(), link: None, sent: None, images: Vec::new(), cursor: 40, n: 2 };
        let page = Page { items: vec![Item { withdrawn: true, ..item("you", None, None) }, item("agent", None, None), item("tool", Some("t1"), None), item("result", Some("t1"), Some("ok")), item("note", None, None), Item { from: Some("Review the parser".into()), ..item("peer", None, None) }], before: Some(12), after: 900 };
        let theirs: codealong_peer::api::Page = serde_json::from_value(serde_json::to_value(&page).unwrap()).unwrap();
        assert_eq!((theirs.before, theirs.after, theirs.items.len()), (Some(12), 900, 6));
        assert_eq!(theirs.items[5].from.as_deref(), Some("Review the parser"));
        let r = &theirs.items[3];
        assert_eq!((r.kind.as_str(), r.id.as_deref(), r.output.as_deref(), r.failed, r.cursor, r.n), ("result", Some("t1"), Some("ok"), true, 40, 2));
        assert_eq!(serde_json::to_value(&theirs).unwrap(), serde_json::to_value(&page).unwrap(), "nothing lost on the way");
        // A command runs again only on the Mac it ran on, so the other devices read past its folder.
        let command = Item { shell: true, cwd: Some("/Users/me/app".into()), ..item("tool", Some("t2"), Some("ok")) };
        let theirs: codealong_peer::api::Page = serde_json::from_value(serde_json::to_value(Page { items: vec![command], before: None, after: 0 }).unwrap()).unwrap();
        assert_eq!(theirs.items[0].input.as_deref(), Some("cargo test"));
        // A page a call made is one to open from any device.
        let made = Item { link: Some(crate::harness::Link { url: "https://claude.ai/artifact/Vuaw".into(), title: "Security dashboard".into(), note: Some("Updated · version 9".into()) }), ..item("result", Some("t3"), Some("Published")) };
        let page = Page { items: vec![made], before: None, after: 0 };
        let theirs: codealong_peer::api::Page = serde_json::from_value(serde_json::to_value(&page).unwrap()).unwrap();
        assert_eq!(theirs.items[0].link.as_ref().map(|l| l.title.as_str()), Some("Security dashboard"));
        assert_eq!(serde_json::to_value(&theirs).unwrap(), serde_json::to_value(&page).unwrap());
        // So are the files a call sent, though they open only on the session's Mac.
        let sent = Item { sent: Some(crate::harness::SentFiles { files: vec!["/Users/me/app/shot.png".into()], caption: Some("Before and after".into()) }), ..item("tool", Some("t4"), None) };
        let page = Page { items: vec![sent], before: None, after: 0 };
        let theirs: codealong_peer::api::Page = serde_json::from_value(serde_json::to_value(&page).unwrap()).unwrap();
        assert_eq!(theirs.items[0].sent.as_ref().map(|s| s.files.clone()), Some(vec!["/Users/me/app/shot.png".to_string()]));
        assert_eq!(serde_json::to_value(&theirs).unwrap(), serde_json::to_value(&page).unwrap());
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
        let from_you = |text: &str| format!("{}\n\n{text}", crate::claude::peer::FROM_YOU);
        let started = peer_line(false, "Codealong", &from_you("Can you commit this?"));
        let queued = peer_line(true, "Codealong", &from_you(&format!("Also this:\n![image]({})", image.display())));
        let agent = peer_line(true, "Review the parser", "This message is from another agent, “Review the parser” (Claude Code on claude2, session k), sent with `codealong session send`; it isn't from your user.\n\nWhich test fails?\n\n- The parser's\n- The lexer's");
        // A Claude Code that keeps no body has only the envelope.
        let older = r#"{"type":"user","isMeta":true,"timestamp":"2026-09-26T10:00:04Z","origin":{"kind":"peer","from":"review"},"message":{"content":"Another Claude session sent a message:\n<cross-session-message from-name=\"review\">\nLooks good\n</cross-session-message>"}}"#;
        // A subagent's report isn't a message; nor is it yours.
        let handback = r#"{"type":"attachment","timestamp":"2026-09-26T10:00:04Z","attachment":{"type":"queued_command","prompt":"<agent-message from=\"a3c7\">\n[Subagent hand-back] The report follows:\n  Done.\n</agent-message>","origin":{"kind":"peer","from":"a3c7","senderTaskId":"a3c7","handback":true,"body":"[Subagent hand-back] The report follows:\n  Done."}}}"#;
        let p = write(&[PROMPT, &started, CALL, &queued, &agent, older, handback, RESULT]);
        let page = file_page(&p, crate::claude::conversation_items, None, 100).unwrap();
        let said: Vec<(&str, &str, Option<&str>)> = page.items.iter().filter(|i| i.kind != "tool" && i.kind != "result").map(|i| (i.kind, i.text.as_str(), i.from.as_deref())).collect();
        assert_eq!(
            said,
            [
                ("you", "Run the tests", None),
                ("you", "Can you commit this?", None),
                ("agent", "Running them.", None),
                ("you", "Also this:\n![image](images/0123456789abcdef.png)", None),
                // Whole, past its first line.
                ("peer", "Which test fails?\n\n- The parser's\n- The lexer's", Some("Review the parser")),
                ("peer", "Looks good", Some("review")),
            ]
        );
    }

    /// Enough of a PNG to be known as one.
    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";

    /// Folders of their own for what showing a page keeps, in place of Codealong's.
    fn kept_dirs(tag: &str) -> (crate::fixtures::Scratch, PathBuf, PathBuf) {
        let dir = crate::fixtures::scratch(tag);
        let (images, files) = (dir.join("images"), dir.join("files"));
        (dir, images, files)
    }

    fn said(page: &Page) -> Vec<(&str, &str)> {
        page.items.iter().map(|i| (i.kind, i.text.as_str())).collect()
    }

    #[test]
    fn a_claude_prompt_shows_the_images_it_came_with() {
        let (_dir, images, files) = kept_dirs("shown-claude");
        let (png, other) = (data_encoding::BASE64.encode(PNG), data_encoding::BASE64.encode(&[PNG, b"2"].concat()));
        let image = |data: &str| serde_json::json!({"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": data}});
        let prompt = |ids: &[u32], content: Value| serde_json::json!({"type": "user", "origin": {"kind": "human"}, "timestamp": "2026-09-26T10:00:00Z", "imagePasteIds": ids, "message": {"role": "user", "content": content}}).to_string();
        let lines = [
            // Pasted into the terminal: Claude Code numbers them through the session, and the text says where each is.
            prompt(&[3, 4], serde_json::json!([{"type": "text", "text": "This [Image #3] should look like [Image #4]"}, image(&png), image(&other)])),
            // Attached in the Claude app: the text doesn't name it.
            prompt(&[1], serde_json::json!([image(&png), {"type": "text", "text": "Why is this cut off?"}])),
            prompt(&[1], serde_json::json!([image(&other)])),
            // Sent while it worked.
            serde_json::json!({"type": "attachment", "timestamp": "2026-09-26T10:00:03Z", "attachment": {"type": "queued_command", "imagePasteIds": [5], "origin": {"kind": "human"}, "prompt": [{"type": "text", "text": "Also this [Image #5]"}, image(&png)]}}).to_string(),
            // Sent from Codealong, which hands Claude Code a kept image as a file mention.
            prompt(&[], Value::String(format!("Seeing this \n@\"{}\"", images.join("0123456789abcdef.png").display()))),
            // Nothing a window can show: the text keeps its mark.
            prompt(&[6], serde_json::json!([{"type": "text", "text": "And [Image #6]"}, image(&data_encoding::BASE64.encode(b"%PDF-1.7"))])),
            prompt(&[7], serde_json::json!([image("not base64")])),
        ];
        let p = write(&lines.iter().map(String::as_str).collect::<Vec<_>>());
        let raw = file_page(&p, crate::claude::conversation_items, None, 100).unwrap();
        assert_eq!(raw.items.iter().map(|i| i.images.len()).collect::<Vec<_>>(), [2, 1, 1, 1, 0, 1, 1]);
        assert_eq!(raw.items[0].images[1], Image { number: Some(4), base64: other.clone() });
        let page = shown_in(raw, &images, &files);
        // Each is kept under the name it has when it's pasted into Codealong.
        let (a, b) = (attachments::keep_image_in(&images, PNG).unwrap(), attachments::keep_image_in(&images, &[PNG, b"2"].concat()).unwrap());
        assert!(attachments::locate_in(&images, &files, &a).unwrap().exists() && a != b);
        assert_eq!(
            said(&page),
            [
                ("you", format!("This ![image]({a}) should look like ![image]({b})").as_str()),
                ("you", format!("Why is this cut off?\n![image]({a})").as_str()),
                ("you", format!("![image]({b})").as_str()),
                ("you", format!("Also this ![image]({a})").as_str()),
                ("you", "Seeing this \n![image](images/0123456789abcdef.png)"),
                ("you", "And [Image #6]"),
            ]
        );
        assert!(page.items.iter().all(|i| i.images.is_empty()));
        assert_eq!(std::fs::read_dir(&images).unwrap().count(), 2, "nothing else is kept");
    }

    #[test]
    fn a_codex_prompt_shows_the_images_it_came_with() {
        let (_dir, images, files) = kept_dirs("shown-codex");
        let url = format!("data:image/png;base64,{}", data_encoding::BASE64.encode(PNG));
        // What the model got, a line ahead of the prompt, and the prompt as Codex records it:
        // a pasted image by a temporary file's path, an attached one by itself.
        let got = serde_json::json!({"timestamp": "2026-09-26T10:00:00Z", "type": "response_item", "payload": {"type": "message", "role": "user", "content": [{"type": "input_text", "text": "## My request:\nWhy is this cut off?"}, {"type": "input_text", "text": "<image name=[Image #1]>"}, {"type": "input_image", "image_url": url, "detail": "high"}]}}).to_string();
        let prompt = |parts: Value| serde_json::json!({"timestamp": "2026-09-26T10:00:00Z", "type": "event_msg", "payload": {"type": "item_completed", "item": {"type": "UserMessage", "content": parts}}}).to_string();
        let pasted = serde_json::json!({"type": "local_image", "path": "/var/folders/x/T/codex-clipboard-1.png"});
        let started = r#"{"timestamp":"2026-09-26T10:00:00Z","type":"event_msg","payload":{"type":"task_started"}}"#;
        let reply = r#"{"timestamp":"2026-09-26T10:00:02Z","type":"event_msg","payload":{"type":"agent_message","message":"Looking."}}"#;
        let command = r#"{"timestamp":"2026-09-26T10:00:03Z","type":"event_msg","payload":{"type":"item_completed","item":{"type":"CommandExecution","command":["/bin/zsh","-lc","npm test"],"exit_code":0}}}"#;
        let lines = [
            got.clone(),
            started.to_string(),
            prompt(serde_json::json!([{"type": "text", "text": "\n# Files mentioned by the user:\n\n## codex-clipboard-1.png: /var/folders/x/T/codex-clipboard-1.png\n\n## My request:\nWhy is this cut off?\n"}, pasted])),
            reply.to_string(),
            // An image a tool hands the model comes on the same kind of line; it's nobody's prompt.
            got.clone(),
            command.to_string(),
            prompt(serde_json::json!([{"type": "text", "text": "And this one"}, {"type": "image", "image_url": url}])),
            got.clone(),
            prompt(serde_json::json!([pasted])),
        ];
        let p = write(&lines.iter().map(String::as_str).collect::<Vec<_>>());
        let read = |before: Option<u64>, limit: usize| crate::codex::with_images_before(&p, file_page(&p, crate::codex::conversation_items, before, limit).unwrap());
        let page = shown_in(read(None, 100), &images, &files);
        let a = attachments::keep_image_in(&images, PNG).unwrap();
        let shot = format!("![image]({a})");
        assert_eq!(
            said(&page),
            [("you", format!("Why is this cut off?\n{shot}").as_str()), ("agent", "Looking."), ("tool", "Shell · `npm test`"), ("you", format!("And this one\n{shot}").as_str()), ("you", shot.as_str())]
        );
        // A page that starts with a prompt reads its images from the line before it.
        let last = read(None, 1);
        assert_eq!(last.items.iter().map(|i| i.kind).collect::<Vec<_>>(), [IMAGES, "you"]);
        let before = last.before;
        assert_eq!(said(&shown_in(last, &images, &files)), [("you", shot.as_str())]);
        // The page before ends with that line, which is no item of its own.
        let earlier = shown_in(read(before, 2), &images, &files);
        assert_eq!(said(&earlier), [("you", format!("And this one\n{shot}").as_str())]);
        // A prompt that names only a file since gone, with nothing ahead of it, has nothing to show.
        let gone = write(&[prompt(serde_json::json!([pasted])).as_str(), reply]);
        let page = shown_in(crate::codex::with_images_before(&gone, file_page(&gone, crate::codex::conversation_items, None, 100).unwrap()), &images, &files);
        assert_eq!(said(&page), [("agent", "Looking.")]);
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

    /// A line longer than a chunk is read whole; one past `LINE_MAX` (a prompt that would read,
    /// were it parsed) is passed over both ways, and its neighbours keep their places.
    #[test]
    fn a_line_past_the_limit_is_passed_over_and_the_lines_around_it_read() {
        let long = RESULT.replace("test result: ok", &format!("{}the end", "y\\n".repeat(250_000)));
        let huge = PROMPT.replace("Run the tests", &"z".repeat(LINE_MAX));
        let p = write(&[PROMPT, &long, &huge, REPLY]);
        let at_long = PROMPT.len() as u64 + 1;
        let at_huge = at_long + long.len() as u64 + 1;
        let at_reply = at_huge + huge.len() as u64 + 1;
        let places = |page: &Page| page.items.iter().map(|i| (i.kind, i.cursor)).collect::<Vec<_>>();
        let all = [("you", 0), ("result", at_long), ("agent", at_reply)];

        let page = file_page(&p, crate::claude::conversation_items, None, 100).unwrap();
        assert_eq!(places(&page), all);
        assert!(page.items[1].output.as_deref().is_some_and(|o| o.ends_with("the end")));
        // A page at a time: the long line's page passes over the one past the limit.
        let last = file_page(&p, crate::claude::conversation_items, None, 1).unwrap();
        assert_eq!((places(&last), last.before), (vec![("agent", at_reply)], Some(at_reply)));
        let mid = file_page(&p, crate::claude::conversation_items, Some(at_reply), 1).unwrap();
        assert_eq!((places(&mid), mid.before), (vec![("result", at_long)], Some(at_long)));

        // Read on from the start, and from inside the line past the limit, which is someone else's.
        let on = file_newer(&p, crate::claude::conversation_items, 0).unwrap();
        assert_eq!((places(&on), on.after), (all.to_vec(), page.after));
        assert_eq!(places(&file_newer(&p, crate::claude::conversation_items, at_huge + 10).unwrap()), [("agent", at_reply)]);
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
