//! Searching what was said in sessions: your prompts and the agents' replies, for sessions
//! active in the last `[search] days`. Tool calls, their output and compaction summaries stay
//! out; they're most of a transcript's bytes and would match nearly anything.
//!
//! The engine reads only each transcript's head and tail, so the text is gathered here, in an
//! index of its own. Transcripts and rollouts only grow at the end, so each file is read whole
//! once and after that from where the last read stopped. A background thread builds the index
//! after launch (`spawn_warmer`), newest files first; a search catches up only files it has
//! already read, so it never waits on a first read.

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::AccountCfg;
use crate::util::{file_name, now_ms, parse_rfc3339_ms, DAY_MS};
use crate::{claude, codex, opencode, procs};

/// A message longer than this is cut: a paste can be hundreds of kilobytes.
const MAX_TEXT: usize = 32 * 1024;
/// How long a list of files stands before a search lists them again.
const RELIST_EVERY: Duration = Duration::from_secs(20);

/// What a search asks of the index: the accounts, and how many days back.
#[derive(Clone, PartialEq, Default)]
pub struct Scope {
    pub accounts: Vec<AccountCfg>,
    pub days: u32,
}

struct Entry {
    at: i64,
    you: bool,
    text: String,
    /// `text` lowercased, for matching.
    lower: String,
}

impl Entry {
    fn new(at: i64, you: bool, mut text: String) -> Entry {
        if text.len() > MAX_TEXT {
            let mut cut = MAX_TEXT;
            while !text.is_char_boundary(cut) {
                cut -= 1;
            }
            text.truncate(cut);
        }
        let lower = text.to_lowercase();
        Entry { at, you, text, lower }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Claude,
    Codex,
}

/// One transcript or rollout: where reading stopped, and what it said up to there.
struct FileText {
    key: String,
    kind: Kind,
    offset: u64,
    entries: Vec<Entry>,
}

#[derive(Default)]
struct OpencodeText {
    /// The newest `time_updated` read, so the next read takes only later writes.
    after: i64,
    /// Session key → message id → what it says.
    sessions: HashMap<String, HashMap<String, Entry>>,
}

#[derive(Default)]
struct Index {
    scope: Scope,
    files: HashMap<PathBuf, FileText>,
    opencode: HashMap<String, OpencodeText>,
    listed: Vec<(PathBuf, String, Kind)>,
    listed_at: Option<Instant>,
    /// Every listed file has been read once.
    ready: bool,
}

/// A session whose text has every word.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hit {
    pub key: String,
    /// The message that matched best, cut to a line around the first match.
    pub snippet: String,
    /// You wrote it; otherwise the agent did.
    pub you: bool,
    pub at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Found {
    pub hits: Vec<Hit>,
    /// Still reading sessions for the first time, so more may match soon.
    pub building: bool,
    /// Searching text is turned off (`days = 0`).
    pub off: bool,
}

fn index() -> &'static Mutex<Index> {
    static INDEX: OnceLock<Mutex<Index>> = OnceLock::new();
    INDEX.get_or_init(|| Mutex::new(Index::default()))
}

fn warmer() -> &'static Mutex<Option<Sender<()>>> {
    static WARMER: OnceLock<Mutex<Option<Sender<()>>>> = OnceLock::new();
    WARMER.get_or_init(|| Mutex::new(None))
}

/// Sets the accounts and days to search; a change starts the index over. The engine calls it
/// whenever config.toml loads.
pub fn configure(scope: Scope) {
    let changed = {
        let mut ix = index().lock().unwrap_or_else(|e| e.into_inner());
        if ix.scope == scope {
            false
        } else {
            *ix = Index { scope, ..Index::default() };
            true
        }
    };
    if changed {
        if let Some(tx) = warmer().lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
            let _ = tx.send(());
        }
    }
}

/// Builds the index in the background, at low priority, after `delay` (the engine's first
/// scan and git have the Mac then), and again whenever `configure` changes what to search.
pub fn spawn_warmer(delay: Duration) {
    let (tx, rx) = channel::<()>();
    *warmer().lock().unwrap_or_else(|e| e.into_inner()) = Some(tx);
    std::thread::spawn(move || {
        procs::utility_thread();
        std::thread::sleep(delay);
        loop {
            warm();
            // Caught up; wait for a new scope, or look for new files now and then.
            match rx.recv_timeout(Duration::from_secs(60)) {
                Ok(()) | Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(_) => break,
            }
            while rx.try_recv().is_ok() {}
        }
    });
}

/// Reads every file not read yet, one at a time, letting searches in between.
fn warm() {
    let pending: Vec<(PathBuf, String, Kind)> = {
        let mut ix = index().lock().unwrap_or_else(|e| e.into_inner());
        ix.relist(true);
        ix.listed.iter().filter(|(p, _, _)| !ix.files.contains_key(p)).cloned().collect()
    };
    let t = Instant::now();
    let mut read = 0u64;
    for (path, key, kind) in pending {
        // Read outside the lock, then put in: a search meanwhile goes without this file.
        let mut file = FileText { key, kind, offset: 0, entries: Vec::new() };
        read += catch_up(&path, &mut file);
        let mut ix = index().lock().unwrap_or_else(|e| e.into_inner());
        if ix.listed.iter().any(|(p, _, _)| *p == path) {
            ix.files.entry(path).or_insert(file);
        }
    }
    let mut ix = index().lock().unwrap_or_else(|e| e.into_inner());
    ix.catch_up_opencode();
    ix.ready = ix.listed.iter().all(|(p, _, _)| ix.files.contains_key(p));
    crate::perf::log(&format!("search index: read {} MB in {:?}, {} files", read / (1024 * 1024), t.elapsed(), ix.files.len()));
}

/// Searches the index as it stands, after reading what's been added to files already read.
pub fn find(words: &[String], limit: usize) -> Found {
    let mut ix = index().lock().unwrap_or_else(|e| e.into_inner());
    if ix.scope.days == 0 {
        return Found { hits: Vec::new(), building: false, off: true };
    }
    ix.relist(false);
    let known: Vec<PathBuf> = ix.listed.iter().map(|(p, _, _)| p.clone()).filter(|p| ix.files.contains_key(p)).collect();
    for p in known {
        if let Some(f) = ix.files.get_mut(&p) {
            catch_up(&p, f);
        }
    }
    ix.catch_up_opencode();
    let hits = ix.search(words, limit);
    Found { hits, building: !ix.ready, off: false }
}

/// Builds the whole index now and searches it: for the command, when the app isn't running.
pub fn find_now(scope: Scope, words: &[String], limit: usize) -> Found {
    configure(scope);
    warm();
    find(words, limit)
}

impl Index {
    /// Lists the transcripts and rollouts in scope, unless the list is fresh. Files that fell
    /// out of scope are dropped.
    fn relist(&mut self, force: bool) {
        if !force && self.listed_at.is_some_and(|t| t.elapsed() < RELIST_EVERY) {
            return;
        }
        self.listed_at = Some(Instant::now());
        let since = now_ms() - self.scope.days as i64 * DAY_MS;
        let mut listed: Vec<(PathBuf, String, Kind, i64)> = Vec::new();
        if self.scope.days > 0 {
            let none = Default::default();
            for a in &self.scope.accounts {
                let home = a.home_path();
                match a.harness.as_str() {
                    "claude" => {
                        for (p, m) in claude::transcript_files(&home, since, &none) {
                            let id = file_name(&p).trim_end_matches(".jsonl").to_string();
                            listed.push((p, format!("claude:{}:{}", a.name, id), Kind::Claude, m));
                        }
                    }
                    "codex" => {
                        for (p, m) in codex::rollout_files(&home, since, &none) {
                            if let Some(id) = codex::thread_id_of(&p) {
                                listed.push((p, format!("codex:{}:{}", a.name, id), Kind::Codex, m));
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        // The newest first, so the warmer reads what you're likeliest to look for first.
        listed.sort_by(|a, b| b.3.cmp(&a.3));
        self.listed = listed.into_iter().map(|(p, k, kind, _)| (p, k, kind)).collect();
        let keep: std::collections::HashSet<&PathBuf> = self.listed.iter().map(|(p, _, _)| p).collect();
        self.files.retain(|p, _| keep.contains(p));
        self.ready = self.listed.iter().all(|(p, _, _)| self.files.contains_key(p));
    }

    fn catch_up_opencode(&mut self) {
        let since = now_ms() - self.scope.days as i64 * DAY_MS;
        let accounts: Vec<AccountCfg> = self.scope.accounts.iter().filter(|a| a.harness == "opencode").cloned().collect();
        for a in accounts {
            let text = self.opencode.entry(a.name.clone()).or_default();
            for m in opencode::read_prose(&a.home_path(), since, text.after) {
                text.after = text.after.max(m.updated);
                let key = format!("opencode:{}:{}", a.name, m.session);
                text.sessions.entry(key).or_default().insert(m.id, Entry::new(m.at, m.you, m.text));
            }
        }
    }

    fn search(&self, words: &[String], limit: usize) -> Vec<Hit> {
        if words.is_empty() {
            return Vec::new();
        }
        let mut hits: Vec<Hit> = Vec::new();
        for f in self.files.values() {
            if let Some(h) = hit(&f.key, f.entries.iter(), words) {
                hits.push(h);
            }
        }
        for oc in self.opencode.values() {
            for (key, msgs) in &oc.sessions {
                if let Some(h) = hit(key, msgs.values(), words) {
                    hits.push(h);
                }
            }
        }
        hits.sort_by(|a, b| b.at.cmp(&a.at).then_with(|| a.key.cmp(&b.key)));
        hits.truncate(limit);
        hits
    }
}

/// The session's hit when every word is somewhere in what was said: the message with the most
/// of them, the latest among equals.
fn hit<'a>(key: &str, entries: impl Iterator<Item = &'a Entry> + Clone, words: &[String]) -> Option<Hit> {
    if !words.iter().all(|w| entries.clone().any(|e| e.lower.contains(w.as_str()))) {
        return None;
    }
    let best = entries.max_by_key(|e| (words.iter().filter(|w| e.lower.contains(w.as_str())).count(), e.at))?;
    Some(Hit { key: key.to_string(), snippet: snippet(&best.text, words), you: best.you, at: best.at })
}

/// One line of `text` around the first of `words` in it, with an ellipsis where it's cut.
pub fn snippet(text: &str, words: &[String]) -> String {
    const BEFORE: usize = 40;
    const LEN: usize = 160;
    let line = text.split_whitespace().collect::<Vec<_>>().join(" ").replace("**", "");
    let lower = line.to_lowercase();
    // Lowercasing can change a string's length (rarely, outside ASCII); then positions in one
    // don't hold in the other, and the line starts from the top.
    let at = if lower.len() == line.len() { words.iter().filter_map(|w| lower.find(w.as_str())).min().unwrap_or(0) } else { 0 };
    let chars: Vec<(usize, char)> = line.char_indices().collect();
    let at_char = chars.iter().position(|(i, _)| *i >= at).unwrap_or(0);
    let mut start = at_char.saturating_sub(BEFORE);
    // Start on a word, unless that goes back to the beginning anyway.
    if start > 0 {
        while start < at_char && chars[start - 1].1 != ' ' {
            start += 1;
        }
    }
    let end = (start + LEN).min(chars.len());
    let body: String = chars[start..end].iter().map(|(_, c)| c).collect();
    format!("{}{}{}", if start > 0 { "…" } else { "" }, body.trim(), if end < chars.len() { "…" } else { "" })
}

/// Reads a file from where the last read stopped to its last complete line. Returns how many
/// bytes it read. A file that got shorter is read again from the top.
fn catch_up(path: &Path, f: &mut FileText) -> u64 {
    let Ok(meta) = std::fs::metadata(path) else { return 0 };
    let len = meta.len();
    if len < f.offset {
        f.offset = 0;
        f.entries.clear();
    }
    if len == f.offset {
        return 0;
    }
    let Ok(file) = File::open(path) else { return 0 };
    let mut r = BufReader::with_capacity(256 * 1024, file);
    if r.seek(SeekFrom::Start(f.offset)).is_err() {
        return 0;
    }
    let start = f.offset;
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match r.read_until(b'\n', &mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                // A line still being written waits for the next read.
                if buf.last() != Some(&b'\n') {
                    break;
                }
                f.offset += n as u64;
                if let Some(e) = entry_of(&buf, f.kind) {
                    // Newer Codex versions record a message as an event and as an item.
                    if f.entries.last().is_some_and(|last| last.you == e.you && last.text == e.text) {
                        continue;
                    }
                    f.entries.push(e);
                }
            }
        }
    }
    f.offset - start
}

/// The prose on one line, if it has any. Lines that can't hold any are passed over before
/// they're parsed: most of a transcript is tool output.
fn entry_of(line: &[u8], kind: Kind) -> Option<Entry> {
    let has = |needle: &[u8]| memchr::memmem::find(line, needle).is_some();
    match kind {
        Kind::Claude => {
            if !has(br#""type":"user""#) && !has(br#""type":"assistant""#) {
                return None;
            }
            let v: Value = serde_json::from_slice(line).ok()?;
            let at = v.get("timestamp").and_then(Value::as_str).and_then(parse_rfc3339_ms).unwrap_or(0);
            if let Some(t) = claude::typed_text(&v) {
                return Some(Entry::new(at, true, t));
            }
            claude::reply_text(&v).map(|t| Entry::new(at, false, t))
        }
        Kind::Codex => {
            if !has(b"agent_message") && !has(b"user_message") && !has(b"AgentMessage") && !has(b"UserMessage") {
                return None;
            }
            let v: Value = serde_json::from_slice(line).ok()?;
            let at = v.get("timestamp").and_then(Value::as_str).and_then(parse_rfc3339_ms).unwrap_or(0);
            codex::prose_of(&v).map(|(you, t)| Entry::new(at, you, t))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(s: &str) -> Vec<String> {
        s.split_whitespace().map(str::to_lowercase).collect()
    }

    #[test]
    fn reads_prompts_and_replies_but_not_tools() {
        let dir = std::env::temp_dir().join(format!("codealong-search-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("s.jsonl");
        let lines = [
            r#"{"type":"user","timestamp":"2026-09-24T10:00:00Z","origin":{"kind":"human"},"message":{"role":"user","content":"Fix the webhook retries"}}"#,
            r#"{"type":"assistant","timestamp":"2026-09-24T10:00:05Z","message":{"role":"assistant","content":[{"type":"text","text":"The retry loop lives in stripe.ts."},{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"grep zebra"}}]}}"#,
            r#"{"type":"user","timestamp":"2026-09-24T10:00:06Z","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":"zebra everywhere"}]}}"#,
            r#"{"type":"user","timestamp":"2026-09-24T10:01:00Z","origin":{"kind":"task-notification"},"message":{"role":"user","content":"giraffe finished"}}"#,
            r#"{"type":"user","timestamp":"2026-09-24T10:02:00Z","isCompactSummary":true,"message":{"role":"user","content":"okapi summary"}}"#,
        ];
        // The last line isn't finished yet.
        std::fs::write(&path, format!("{}\n{}", lines.join("\n"), r#"{"type":"assistant","message":{"content":[{"type":"text","text":"half"#)).unwrap();
        let mut f = FileText { key: "claude:claude:s".into(), kind: Kind::Claude, offset: 0, entries: Vec::new() };
        catch_up(&path, &mut f);
        assert_eq!(f.entries.iter().map(|e| (e.you, e.text.as_str())).collect::<Vec<_>>(), vec![(true, "Fix the webhook retries"), (false, "The retry loop lives in stripe.ts.")]);
        for w in ["zebra", "giraffe", "okapi", "half"] {
            assert!(hit(&f.key, f.entries.iter(), &words(w)).is_none(), "{w} shouldn't match");
        }
        let h = hit(&f.key, f.entries.iter(), &words("webhook stripe")).unwrap();
        assert_eq!(h.snippet, "The retry loop lives in stripe.ts.");

        // The rest of the line arrives, then more: only the new bytes are read.
        let more = r#"{"type":"assistant","timestamp":"2026-09-24T10:03:00Z","message":{"content":[{"type":"text","text":"Done, both webhook and stripe fixed."}]}}"#;
        std::fs::write(&path, format!("{}\n{}\n", lines.join("\n"), more)).unwrap();
        let before = f.offset;
        assert_eq!(catch_up(&path, &mut f), std::fs::metadata(&path).unwrap().len() - before);
        let h = hit(&f.key, f.entries.iter(), &words("webhook stripe")).unwrap();
        assert_eq!(h.snippet, "Done, both webhook and stripe fixed.");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn reads_codex_messages_once() {
        let lines = [
            r#"{"timestamp":"2026-09-24T21:22:53.397Z","type":"event_msg","payload":{"type":"item_completed","item":{"type":"UserMessage","content":[{"type":"text","text":"Generate app logos"}]}}}"#,
            r#"{"timestamp":"2026-09-24T21:22:56.753Z","type":"event_msg","payload":{"type":"item_completed","item":{"type":"AgentMessage","content":[{"type":"Text","text":"I'll use the imagegen skill."}]}}}"#,
            r#"{"timestamp":"2026-09-24T21:22:56.754Z","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"I'll use the imagegen skill."}]}}"#,
            r#"{"timestamp":"2026-09-24T21:22:57Z","type":"event_msg","payload":{"type":"agent_message","message":"I'll use the imagegen skill."}}"#,
        ];
        let entries: Vec<Entry> = lines.iter().filter_map(|l| entry_of(l.as_bytes(), Kind::Codex)).collect();
        assert_eq!(entries.len(), 3);
        assert!(entries[0].you && !entries[1].you);
    }

    #[test]
    fn snippets_start_near_the_match() {
        let text = format!("{} the needle is here and the line goes on", "word ".repeat(40));
        let s = snippet(&text, &words("needle"));
        assert!(s.starts_with('…'));
        assert!(s.contains("the needle is here"));
        assert!(!s.starts_with("…ord"), "starts on a whole word: {s}");
        assert_eq!(snippet("Short   and\nsweet", &words("sweet")), "Short and sweet");
    }
}
