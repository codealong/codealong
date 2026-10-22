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


use crate::config::AccountCfg;
use crate::harness;
use crate::model::Harness;
use crate::util::{now_ms, DAY_MS};

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
    /// `text` lowercased with its runs of whitespace made single spaces, for matching words and
    /// the phrase they make.
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
        let lower = text.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase();
        Entry { at, you, text, lower }
    }
}

/// One session's record that's a file of lines (a transcript, a rollout): where reading
/// stopped, and what it said up to there. Its harness reads its lines (`Adapter::prose`).
struct FileText {
    key: String,
    harness: Harness,
    offset: u64,
    entries: Vec<Entry>,
}

/// What an account's sessions said where their harness keeps messages in a database
/// (`Adapter::said`).
#[derive(Default)]
struct RowsText {
    /// The newest write read, so the next read takes only later ones.
    after: i64,
    /// Session key → message id → what it says.
    sessions: HashMap<String, HashMap<String, Entry>>,
}

#[derive(Default)]
struct Index {
    scope: Scope,
    files: HashMap<PathBuf, FileText>,
    rows: HashMap<String, RowsText>,
    listed: Vec<(PathBuf, String, Harness)>,
    listed_at: Option<Instant>,
    /// Every listed file has been read once.
    ready: bool,
}

pub use codealong_peer::api::{Found, Hit};

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
    let changed = index().lock().unwrap_or_else(|e| e.into_inner()).rescope(scope);
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
        crate::os::system::utility_thread();
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
    let pending: Vec<(PathBuf, String, Harness)> = {
        let mut ix = index().lock().unwrap_or_else(|e| e.into_inner());
        ix.relist(true);
        ix.listed.iter().filter(|(p, _, _)| !ix.files.contains_key(p)).cloned().collect()
    };
    let t = Instant::now();
    let mut read = 0u64;
    for (path, key, harness) in pending {
        // Read outside the lock, then put in: a search meanwhile goes without this file.
        let mut file = FileText { key, harness, offset: 0, entries: Vec::new() };
        read += catch_up(&path, &mut file);
        let mut ix = index().lock().unwrap_or_else(|e| e.into_inner());
        if ix.listed.iter().any(|(p, _, _)| *p == path) {
            ix.files.entry(path).or_insert(file);
        }
    }
    let mut ix = index().lock().unwrap_or_else(|e| e.into_inner());
    ix.catch_up_rows();
    ix.ready = ix.listed.iter().all(|(p, _, _)| ix.files.contains_key(p));
    crate::perf::log(&format!("search index: read {} MB in {:?}, {} files", read / (1024 * 1024), t.elapsed(), ix.files.len()));
}

/// Searches the index as it stands, after reading what's been added to files already read.
pub fn find(words: &[String], limit: usize) -> Found {
    let mut ix = index().lock().unwrap_or_else(|e| e.into_inner());
    if ix.scope.days == 0 {
        return Found { hits: Vec::new(), building: false, off: true };
    }
    ix.read_new();
    let hits = ix.search(words, limit, now_ms());
    Found { hits, building: !ix.ready, off: false }
}

/// A session whose prompts and replies share words with something typed, for Jev to weigh where
/// it goes (route.rs).
#[derive(Debug, Clone, PartialEq)]
pub struct Related {
    pub key: String,
    /// The words its best messages share, as `keywords` has them.
    pub shared: Vec<String>,
    /// Its messages that share the most, the best first: whether you wrote it, and a line around
    /// the words.
    pub lines: Vec<(bool, String)>,
}

/// The sessions `wanted` takes whose messages share the most telling of `text`'s words
/// (`keywords`), at most `limit` of them. Unlike `find`, a message needn't have every word: what's
/// typed to route is a sentence, and few messages have all of its words. Each message is weighed
/// on its own, since a long conversation has nearly every word somewhere in it.
pub fn related(text: &str, limit: usize, wanted: impl Fn(&str) -> bool) -> Vec<Related> {
    let words = keywords(text);
    if words.is_empty() {
        return Vec::new();
    }
    let mut ix = index().lock().unwrap_or_else(|e| e.into_inner());
    if ix.scope.days == 0 {
        return Vec::new();
    }
    ix.read_new();
    ix.related(&words, limit, now_ms(), wanted)
}

/// The most of a text's words `related` searches by, the first ones: a message's are bits of a `u32`
/// (`words_in`).
const KEYWORDS: usize = 16;

/// Words that say nothing of what a message is about. Shorter words are left out anyway.
const COMMON: &[&str] = &[
    "about", "after", "again", "all", "also", "and", "any", "are", "aren", "before", "been", "being", "both", "but", "can", "could", "did", "didn", "does", "doesn", "doing", "don", "done", "each", "for", "from", "get", "got", "had", "has", "have", "here", "how", "into", "isn", "it's", "its", "just", "let", "lets", "like", "make", "more", "most", "need", "not", "now", "one", "only", "other", "our", "out", "over", "please", "should", "some", "still", "such", "sure", "than", "that", "the", "their", "them", "then", "there", "these", "they", "thing", "things", "this", "those", "too", "use", "very", "want", "was", "wasn", "way", "were", "what", "when", "where", "which", "who", "why", "will", "with", "won", "would", "yet", "you", "your",
];

/// The words of `text` that may say what it's about, for `related`: in lower case, of three
/// letters or more and none of `COMMON`, each cut to what its other forms start with (`stem`),
/// once, at most `KEYWORDS`. Attachments are left out. Words split as route.rs splits them to
/// find a repo's name, so `spark-infra` stays one.
pub fn keywords(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for w in crate::attachments::without(text).split(|c: char| !(c.is_alphanumeric() || matches!(c, '-' | '_' | '.'))) {
        let w = w.trim_matches(|c| matches!(c, '-' | '_' | '.')).to_lowercase();
        if w.chars().count() < 3 || COMMON.contains(&w.as_str()) {
            continue;
        }
        let w = stem(&w).to_string();
        if !out.contains(&w) {
            out.push(w);
        }
        if out.len() == KEYWORDS {
            break;
        }
    }
    out
}

/// A word without the ending its other forms don't share, where four letters or more are left:
/// `wraps` finds "wrapping", `killed` finds "kills", `messages` finds "message".
fn stem(w: &str) -> &str {
    ["ing", "es", "ed", "s"].iter().filter_map(|end| w.strip_suffix(end)).find(|s| s.chars().count() >= 4).unwrap_or(w)
}

/// Which of the words `ac` looks for start a word of `text`, as bits: "retry" is in "retrying",
/// not in "pretty".
fn words_in(text: &str, ac: &aho_corasick::AhoCorasick) -> u32 {
    let mut found = 0;
    for m in ac.find_overlapping_iter(text) {
        // A match of one UTF-8 string in another starts on a character.
        if !text[..m.start()].chars().next_back().is_some_and(char::is_alphanumeric) {
            found |= 1 << m.pattern().as_usize();
        }
    }
    found
}

/// The most messages of a session `related` gives, and how many messages a word may be in, as a
/// share of them, and still tell which is meant: of 40,000 messages here (2026-10-22), "say",
/// "since" and "last" were each in 7 to 9%, "schedule", "watch" and "hide" in 1 to 4%.
const RELATED_LINES: usize = 2;
const TELLING_SHARE: usize = 20;

/// What a window or another device finds for the words it typed: tidied (`tidy`), at most 200
/// sessions, and timed in the perf log.
pub fn typed(words: &[String]) -> Found {
    let words = tidy(words);
    let t = Instant::now();
    let found = find(&words, 200);
    if crate::perf::enabled() {
        crate::perf::log(&format!("search: {} words, {} sessions in {} ms", words.len(), found.hits.len(), t.elapsed().as_millis()));
    }
    found
}

/// Words as typed, made ready to search by: trimmed, in lower case, the empty ones gone.
pub fn tidy(words: &[String]) -> Vec<String> {
    words.iter().map(|w| w.trim().to_lowercase()).filter(|w| !w.is_empty()).collect()
}

/// Builds the whole index now and searches it: for the command, when the app isn't running.
pub fn find_now(scope: Scope, words: &[String], limit: usize) -> Found {
    build_now(scope);
    find(words, limit)
}

/// Builds the whole index now, as the app does in the background: for a check of what's searched
/// without the app.
pub fn build_now(scope: Scope) {
    configure(scope);
    warm();
}

impl Index {
    /// Searches `scope` from now on, and starts over unless it's what this one searched: the same
    /// accounts in another order (the usage bar's chips, dragged) keep everything read. Whether it
    /// started over.
    fn rescope(&mut self, scope: Scope) -> bool {
        if self.scope.days == scope.days && crate::config::same_accounts(&self.scope.accounts, &scope.accounts) {
            self.scope = scope;
            return false;
        }
        *self = Index { scope, ..Index::default() };
        true
    }

    /// Lists the records in scope that are files (`Adapter::search_files`), unless the list is
    /// fresh. Files that fell out of scope are dropped.
    fn relist(&mut self, force: bool) {
        if !force && self.listed_at.is_some_and(|t| t.elapsed() < RELIST_EVERY) {
            return;
        }
        self.listed_at = Some(Instant::now());
        let since = now_ms() - self.scope.days as i64 * DAY_MS;
        let mut listed: Vec<(PathBuf, String, Harness, i64)> = Vec::new();
        if self.scope.days > 0 {
            for a in &self.scope.accounts {
                let Some(adapter) = harness::of(a) else { continue };
                listed.extend(adapter.search_files(a, since).into_iter().map(|(p, key, m)| (p, key, adapter.harness(), m)));
            }
        }
        // The newest first, so the warmer reads what you're likeliest to look for first.
        listed.sort_by(|a, b| b.3.cmp(&a.3));
        self.listed = listed.into_iter().map(|(p, k, harness, _)| (p, k, harness)).collect();
        let keep: std::collections::HashSet<&PathBuf> = self.listed.iter().map(|(p, _, _)| p).collect();
        self.files.retain(|p, _| keep.contains(p));
        self.ready = self.listed.iter().all(|(p, _, _)| self.files.contains_key(p));
    }

    /// Lists the files again when that's due, and reads what's been added to the ones read
    /// already and to the rows.
    fn read_new(&mut self) {
        self.relist(false);
        let known: Vec<PathBuf> = self.listed.iter().map(|(p, _, _)| p.clone()).filter(|p| self.files.contains_key(p)).collect();
        for p in known {
            if let Some(f) = self.files.get_mut(&p) {
                catch_up(&p, f);
            }
        }
        self.catch_up_rows();
    }

    /// Reads what was said since the last read where messages are rows (`Adapter::said`).
    fn catch_up_rows(&mut self) {
        let since = now_ms() - self.scope.days as i64 * DAY_MS;
        for a in self.scope.accounts.clone() {
            let Some(adapter) = harness::of(&a) else { continue };
            let after = self.rows.get(&a.name).map_or(0, |t| t.after);
            let said = adapter.said(&a, since, after);
            if said.is_empty() {
                continue;
            }
            let text = self.rows.entry(a.name.clone()).or_default();
            for m in said {
                text.after = text.after.max(m.updated);
                let key = crate::model::Session::key_for(adapter.harness(), &a.name, &m.session);
                text.sessions.entry(key).or_default().insert(m.id, Entry::new(m.at, m.you, m.text));
            }
        }
    }

    /// The sessions whose messages have every word, at most `limit` of them: the ones the windows
    /// rank first (`kept_first`), so a common word's hundreds of old mentions don't push out the
    /// session that said it an hour ago.
    fn search(&self, words: &[String], limit: usize, now: i64) -> Vec<Hit> {
        if words.is_empty() {
            return Vec::new();
        }
        let mut hits: Vec<(f64, Hit)> = Vec::new();
        for f in self.files.values() {
            if let Some(h) = hit(&f.key, f.entries.iter(), words) {
                hits.push((kept_first(h.score, f.entries.iter().map(|e| e.at).max().unwrap_or(0), now), h));
            }
        }
        for rows in self.rows.values() {
            for (key, msgs) in &rows.sessions {
                if let Some(h) = hit(key, msgs.values(), words) {
                    hits.push((kept_first(h.score, msgs.values().map(|e| e.at).max().unwrap_or(0), now), h));
                }
            }
        }
        hits.sort_by(|(a, x), (b, y)| a.total_cmp(b).then(y.at.cmp(&x.at)).then_with(|| x.key.cmp(&y.key)));
        hits.truncate(limit);
        hits.into_iter().map(|(_, h)| h).collect()
    }

    /// The sessions whose messages share the most telling of `words`, at most `limit`. A word
    /// counts by how rare it is among all the messages, `ln(messages / messages with it)²`, so a
    /// rare word outweighs a few common ones, and not at all in more than a `TELLING_SHARE`th of
    /// them; a message by the words it has, less the
    /// longer it is (a long one has more words of every kind), and only with two telling words, or
    /// every one when there are fewer. A session counts its best message and half its next, the
    /// latest first among equals. Only sessions `wanted` takes are ranked, so others can't crowd
    /// them out, but every message counts toward how rare a word is.
    fn related(&self, words: &[String], limit: usize, now: i64, wanted: impl Fn(&str) -> bool) -> Vec<Related> {
        let Ok(ac) = aho_corasick::AhoCorasick::new(words) else { return Vec::new() };
        let sessions: Vec<(&str, Vec<&Entry>)> = self
            .files
            .values()
            .map(|f| (f.key.as_str(), f.entries.iter().collect()))
            .chain(self.rows.values().flat_map(|r| r.sessions.iter().map(|(k, msgs)| (k.as_str(), msgs.values().collect()))))
            .collect();
        let has: Vec<Vec<u32>> = sessions.iter().map(|(_, msgs)| msgs.iter().map(|e| words_in(&e.lower, &ac)).collect()).collect();
        let n: usize = has.iter().map(Vec::len).sum();
        let weight: Vec<f64> = (0..words.len())
            .map(|w| {
                let with = has.iter().flatten().filter(|m| *m & (1 << w) != 0).count();
                if with == 0 || with * TELLING_SHARE > n.max(2 * TELLING_SHARE) {
                    0.0
                } else {
                    (n as f64 / with as f64).ln().max(0.1).powi(2)
                }
            })
            .collect();
        let telling: u32 = (0..words.len()).filter(|w| weight[*w] > 0.0).fold(0, |m, w| m | 1 << w);
        if telling == 0 {
            return Vec::new();
        }
        let enough = telling.count_ones().min(2);
        let mean = sessions.iter().flat_map(|(_, msgs)| msgs.iter().map(|e| e.lower.len())).sum::<usize>() as f64 / n.max(1) as f64;
        let bits = |m: u32| (0..words.len()).filter(move |w| m & (1 << w) != 0);
        // Each session's messages with enough telling words, the best first.
        let mut found: Vec<(f64, i64, usize, Vec<(f64, &Entry, u32)>)> = sessions
            .iter()
            .enumerate()
            .filter(|(_, (key, _))| wanted(key))
            .filter_map(|(i, (_, msgs))| {
                let mut best: Vec<(f64, &Entry, u32)> = msgs
                    .iter()
                    .zip(&has[i])
                    .filter(|(_, m)| (**m & telling).count_ones() >= enough)
                    .map(|(e, m)| (bits(m & telling).map(|w| weight[w]).sum::<f64>() / (1.0 + e.lower.len() as f64 / mean.max(1.0)).sqrt(), *e, m & telling))
                    .collect();
                if best.is_empty() {
                    return None;
                }
                best.sort_by(|a, b| b.0.total_cmp(&a.0).then(b.1.at.cmp(&a.1.at)));
                let score = best[0].0 + best.get(1).map_or(0.0, |b| b.0 / 2.0);
                let latest = msgs.iter().map(|e| e.at).max().unwrap_or(0).min(now);
                best.truncate(RELATED_LINES * 2);
                Some((score, latest, i, best))
            })
            .collect();
        found.sort_by(|a, b| b.0.total_cmp(&a.0).then(b.1.cmp(&a.1)).then_with(|| sessions[a.2].0.cmp(sessions[b.2].0)));
        found.truncate(limit);
        found
            .into_iter()
            .map(|(_, _, i, best)| {
                let mut lines: Vec<(bool, String)> = Vec::new();
                let mut shared = 0;
                for (_, e, m) in best {
                    let line = snippet(&e.text, &bits(m).map(|w| words[w].clone()).collect::<Vec<_>>());
                    if !lines.iter().any(|(_, l)| *l == line) {
                        lines.push((e.you, line));
                        shared |= m;
                    }
                    if lines.len() == RELATED_LINES {
                        break;
                    }
                }
                Related { key: sessions[i].0.to_string(), shared: bits(shared).map(|w| words[w].clone()).collect(), lines }
            })
            .collect()
    }
}

/// The session's hit when every word is somewhere in what was said: the message that matched
/// best, the latest among equals.
fn hit<'a>(key: &str, entries: impl Iterator<Item = &'a Entry> + Clone, words: &[String]) -> Option<Hit> {
    if !words.iter().all(|w| entries.clone().any(|e| has_word(&e.lower, w, words.len()))) {
        return None;
    }
    let (score, _, best) = entries.map(|e| (match_score(e, words), words.iter().filter(|w| has_word(&e.lower, w, words.len())).count(), e)).max_by_key(|(s, n, e)| (*s, *n, e.at))?;
    Some(Hit { key: key.to_string(), snippet: snippet(&best.text, words), you: best.you, at: best.at, score })
}

/// Where the windows rank a session found by what was said in it, the lowest first, as the quick
/// panel weighs it (src/quick.ts `searchCost`): how many times its age has doubled, in hours since
/// its last message, and a quarter of a doubling more for each point its best line (`score`)
/// is short of `BEST_SCORE`. The panel counts the age from the session's last activity, which its
/// last message seldom trails by much.
fn kept_first(score: u8, latest: i64, now: i64) -> f64 {
    const HOUR_MS: f64 = 60.0 * 60.0 * 1000.0;
    f64::from(BEST_SCORE - score.min(BEST_SCORE)) / 4.0 + (1.0 + (now - latest).max(0) as f64 / HOUR_MS).log2()
}

/// What `match_score` gives at best: the words as typed, by you.
const BEST_SCORE: u8 = 7;

/// How well one message matches: the words as typed, in order, beat all of them apart, which
/// beat only some; and where a message has them all, what you wrote beats what the agent did.
/// Among messages with only some, the latest stands for the session.
fn match_score(e: &Entry, words: &[String]) -> u8 {
    let short = |w: &String| w.chars().count() <= 2;
    let level = if words.len() > 1 && has_edged(&e.lower, &words.join(" "), short(&words[0]), short(&words[words.len() - 1])) {
        3
    } else if words.iter().all(|w| has_word(&e.lower, w, words.len())) {
        2
    } else if words.iter().any(|w| has_word(&e.lower, w, words.len())) {
        1
    } else {
        0
    };
    level * 2 + u8::from(e.you && level >= 2)
}

/// Whether `text` has `word`. Among other words, a word of one or two letters counts only
/// whole ("class c" shouldn't match every message with a c in it); alone it's a search as you
/// type and matches anywhere. The same rule as `hasWord` in src/quick.ts.
pub fn has_word(text: &str, word: &str, of: usize) -> bool {
    let whole = of > 1 && word.chars().count() <= 2;
    has_edged(text, word, whole, whole)
}

/// Whether `text` has `needle`, starting a word when `start` and ending one when `end`.
fn has_edged(text: &str, needle: &str, start: bool, end: bool) -> bool {
    if !start && !end {
        return text.contains(needle);
    }
    let edge = |c: Option<char>| !c.is_some_and(char::is_alphanumeric);
    text.match_indices(needle).any(|(i, _)| (!start || edge(text[..i].chars().next_back())) && (!end || edge(text[i + needle.len()..].chars().next())))
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
                if let Some(e) = entry_of(&buf, f.harness) {
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

/// The prose on one line of a harness's record, if it has any (`Adapter::prose`).
fn entry_of(line: &[u8], harness: Harness) -> Option<Entry> {
    harness::adapter(harness).prose(line).map(|(at, you, text)| Entry::new(at, you, text))
}

#[cfg(test)]
mod tests {


    use super::*;

    fn words(s: &str) -> Vec<String> {
        s.split_whitespace().map(str::to_lowercase).collect()
    }

    #[test]
    fn reads_prompts_and_replies_but_not_tools() {
        let dir = crate::fixtures::scratch("search");
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
        let mut f = FileText { key: "claude:claude:s".into(), harness: Harness::Claude, offset: 0, entries: Vec::new() };
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
    }

    #[test]
    fn accounts_in_another_order_keep_what_was_read() {
        let account = |name: &str| AccountCfg { name: name.into(), harness: "claude".into(), home: format!("~/.{name}"), command: None, start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false };
        let scope = |names: &[&str], days: u32| Scope { accounts: names.iter().map(|n| account(n)).collect(), days };
        let mut ix = Index { scope: scope(&["a", "b"], 14), ready: true, ..Index::default() };
        ix.files.insert(PathBuf::from("/s.jsonl"), FileText { key: "claude:a:s".into(), harness: Harness::Claude, offset: 10, entries: Vec::new() });
        assert!(!ix.rescope(scope(&["b", "a"], 14)));
        assert!(ix.ready && ix.files.len() == 1);
        assert_eq!(ix.scope.accounts[0].name, "b", "it searches them in the new order");
        // Another account, or other days, start it over.
        assert!(ix.rescope(scope(&["b"], 14)));
        assert!(!ix.ready && ix.files.is_empty());
        assert!(ix.rescope(scope(&["b"], 7)));
    }

    #[test]
    fn reads_codex_messages_once() {
        let lines = [
            r#"{"timestamp":"2026-09-24T21:22:53.397Z","type":"event_msg","payload":{"type":"item_completed","item":{"type":"UserMessage","content":[{"type":"text","text":"Generate app logos"}]}}}"#,
            r#"{"timestamp":"2026-09-24T21:22:56.753Z","type":"event_msg","payload":{"type":"item_completed","item":{"type":"AgentMessage","content":[{"type":"Text","text":"I'll use the imagegen skill."}]}}}"#,
            r#"{"timestamp":"2026-09-24T21:22:56.754Z","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"I'll use the imagegen skill."}]}}"#,
            r#"{"timestamp":"2026-09-24T21:22:57Z","type":"event_msg","payload":{"type":"agent_message","message":"I'll use the imagegen skill."}}"#,
        ];
        let entries: Vec<Entry> = lines.iter().filter_map(|l| entry_of(l.as_bytes(), Harness::Codex)).collect();
        assert_eq!(entries.len(), 3);
        assert!(entries[0].you && !entries[1].you);
    }

    #[test]
    fn short_words_count_whole_and_phrases_rank_first() {
        let e = |you: bool, t: &str| Entry::new(0, you, t.to_string());
        let scattered = [e(true, "Which class is it?"), e(false, "Claude checked it.")];
        assert!(hit("a", scattered.iter(), &words("class c")).is_none(), "the c in Claude isn't the word c");
        assert!(has_word("co", "co", 1), "alone, a short word matches anywhere");
        assert!(has_word("see claude", "c", 1));

        let apart = [e(false, "C is for the class of service."), e(true, "Plan the merge")];
        let phrase = [e(true, "Check our class\nC status and plan")];
        let a = hit("a", apart.iter(), &words("class c")).unwrap();
        let p = hit("p", phrase.iter(), &words("class c")).unwrap();
        assert!(p.score > a.score);
        assert_eq!(p.score, BEST_SCORE, "the words as typed, by you, score the best");
        assert!(p.you && p.snippet.contains("class C status"));
        let cleanup = [e(true, "the class cleanup, c too")];
        assert!(hit("c", cleanup.iter(), &words("class c")).unwrap().score < p.score, "class cleanup isn't the phrase class c");
    }

    #[test]
    fn keeps_the_sessions_the_windows_rank_first() {
        const HOUR: i64 = 60 * 60 * 1000;
        let now = 1_790_200_000_000;
        let file = |key: &str, ago: i64, you: bool, text: &str| FileText { key: key.into(), harness: Harness::Claude, offset: 0, entries: vec![Entry::new(now - ago, you, text.to_string())] };
        let mut ix = Index::default();
        // Three weeks ago, the words as typed by you; an hour ago, by the agent among others.
        ix.files.insert(PathBuf::from("/old.jsonl"), file("old", 21 * 24 * HOUR, true, "the harbor watch"));
        ix.files.insert(PathBuf::from("/new.jsonl"), file("new", HOUR, false, "watch the harbor"));
        let keys = |ix: &Index, limit| ix.search(&words("harbor watch"), limit, now).into_iter().map(|h| h.key).collect::<Vec<_>>();
        assert_eq!(keys(&ix, 1), vec!["new"], "the recent mention is kept over the old phrase");
        assert_eq!(keys(&ix, 9), vec!["new", "old"]);
        // As old as each other, the better line comes first.
        ix.files.insert(PathBuf::from("/peer.jsonl"), file("peer", HOUR, true, "the harbor watch"));
        assert_eq!(keys(&ix, 2), vec!["peer", "new"]);
        // The session's last message counts, not when its best line was said.
        ix.files.get_mut(Path::new("/old.jsonl")).unwrap().entries.push(Entry::new(now, false, "done".into()));
        assert_eq!(keys(&ix, 1), vec!["old"]);
    }

    #[test]
    fn keywords_leave_out_common_and_short_words_and_attachments() {
        assert_eq!(keywords("Can you fix the flaky retry test in spark-infra? It's the one we saw."), ["fix", "flaky", "retry", "test", "spark-infra", "saw"]);
        assert_eq!(keywords("Make it like ![image](images/0123456789abcdef.png) Retry, retry."), ["retry"]);
        assert_eq!(keywords("it wraps the messages it killed, uses notes"), ["wrap", "messag", "kill", "uses", "note"], "endings go where four letters stay");
        assert!(keywords("do it now").is_empty());
    }

    #[test]
    fn related_sessions_have_messages_with_the_telling_words_not_all_of_them() {
        let now = 1_790_200_000_000;
        let file = |key: &str, said: &[(bool, &str)]| FileText { key: key.into(), harness: Harness::Claude, offset: 0, entries: said.iter().map(|(you, t)| Entry::new(now - 1_000, *you, t.to_string())).collect() };
        let mut ix = Index::default();
        ix.files.insert(PathBuf::from("/webhook.jsonl"), file("webhook", &[(true, "The Stripe webhook retries twice"), (false, "Retrying with backoff in stripe.ts now.")]));
        ix.files.insert(PathBuf::from("/stripe.jsonl"), file("stripe", &[(true, "Update the stripe plan names, then retry the sync")]));
        ix.files.insert(PathBuf::from("/pretty.jsonl"), file("pretty", &[(false, "pretty output, backoff aside")]));
        // Eighty messages in all, five of them with "fix".
        for i in 0..76 {
            let filler = if i < 5 { format!("fix the build {i}") } else { format!("release notes {i}") };
            ix.files.insert(PathBuf::from(format!("/f{i}.jsonl")), file(&format!("f{i}"), &[(true, &filler)]));
        }
        let words = keywords("fix the webhook retry backoff for stripe");
        let all = |_: &str| true;
        let found = ix.related(&words, 5, now, all);
        // "fix" is in one message of sixteen, too many to tell which is meant; the rest are in
        // one to three. A message with one of them doesn't do while there are others.
        assert_eq!(found.iter().map(|r| r.key.as_str()).collect::<Vec<_>>(), ["webhook", "stripe"]);
        assert_eq!(found[0].shared, ["webhook", "retry", "backoff", "stripe"]);
        // The message with the most of the rarest words first; retry starts "retrying", not
        // "retries" or "pretty".
        assert_eq!(found[0].lines, [(false, "Retrying with backoff in stripe.ts now.".to_string()), (true, "The Stripe webhook retries twice".to_string())]);
        assert_eq!(found[1].shared, ["retry", "stripe"]);
        // ...unless one is all there is to share. The shorter message comes first.
        assert_eq!(ix.related(&keywords("fix the backoff"), 5, now, all).iter().map(|r| r.key.as_str()).collect::<Vec<_>>(), ["pretty", "webhook"]);
        assert!(ix.related(&keywords("fix it"), 5, now, all).is_empty(), "nothing telling, nothing found");
        // Only the sessions wanted are ranked, so one left out takes no place from them, but its
        // messages still say how rare a word is.
        assert_eq!(ix.related(&words, 1, now, |k| k != "webhook").iter().map(|r| r.key.as_str()).collect::<Vec<_>>(), ["stripe"]);
        assert!(ix.related(&keywords("fix the webhook backoff"), 5, now, |k| k == "pretty").is_empty(), "pretty shares backoff alone while webhook is still a word");
    }

    #[test]
    fn a_long_conversation_with_every_word_apart_loses_to_a_message_with_them_together() {
        let now = 1_790_200_000_000;
        let file = |key: &str, said: Vec<String>| FileText { key: key.into(), harness: Harness::Claude, offset: 0, entries: said.into_iter().map(|t| Entry::new(now - 1_000, true, t)).collect() };
        let mut ix = Index::default();
        let mut long: Vec<String> = (0..30).map(|i| format!("step {i} of the plan")).collect();
        long.extend(["the overflow is fine".into(), "chips are fine".into(), "the strip is fine".into()]);
        ix.files.insert(PathBuf::from("/long.jsonl"), file("long", long));
        ix.files.insert(PathBuf::from("/short.jsonl"), file("short", vec!["The strip's overflow wraps its chips".into()]));
        let found = ix.related(&keywords("the overflow on the strip wraps when there are many chips"), 5, now, |_| true);
        assert_eq!(found.iter().map(|r| r.key.as_str()).collect::<Vec<_>>(), ["short"], "no message of the long one has two of the words");
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
