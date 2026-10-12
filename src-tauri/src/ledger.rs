//! The token ledger: what every model call on this Mac used, kept for as long as you keep
//! Codealong, and what that cost. Harnesses delete their records (Claude Code its transcripts
//! after about 30 days), so the history over months is this file's, `ledger.sqlite` next to the
//! config. It's SQLite rather than one of store.rs's JSON files: it holds a row per call, hundreds
//! of thousands of them, read by day, account, model, folder and session.
//!
//! Each harness says where its calls are recorded (`Adapter::token_files`, `token_use`,
//! `token_uses_since`). Records only grow at the end, so each file is read whole once and after
//! that from where the last read stopped, as search.rs reads them. A background thread reads
//! them after launch (`spawn`), oldest file first, and every minute after.
//!
//! One call is often written more than once: Claude Code writes a reply's usage again on each
//! line of it as it streams, the first copies short of the final count, and copies a
//! conversation's lines into a fork, a session moved to another account and a side question.
//! So a call is one row by the harness's own name for it, kept with the largest count of each
//! kind of token any copy has, and with the session that wrote it first: reading the oldest
//! files first gives a copied call to the session it came from.
//!
//! What's kept is tokens, not dollars: a report prices them (prices.rs) as it's asked, so a
//! change in prices reprices the whole history. What was really paid is a report's too: an
//! account's plan (`monthly` in config.toml) a day's share at a time, Claude's extra usage as
//! its usage endpoint reports it (`note_extra`), what a harness recorded a call cost (OpenCode,
//! from your provider), and an account that pays by the token (`per_token`) at API prices.

use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::config::AccountCfg;
use crate::prices::{self, Price, Used};
use crate::util::{now_ms, DAY_MS};

/// What one model call used, as a harness's records say.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TokenUse {
    /// The harness's own name for the call, the same in every record that copies it: Claude's
    /// message and request IDs, a Codex token count's time and running total, OpenCode's
    /// message ID.
    pub id: String,
    pub at: i64,
    /// The key of the session it was for (`Session::key_for`).
    pub session: String,
    /// Where the session was working.
    pub folder: String,
    pub model: String,
    /// Input tokens read neither from the cache nor into it.
    pub input: u64,
    pub cache_read: u64,
    /// Written to the cache for five minutes.
    pub cache_write: u64,
    /// Written to the cache for an hour.
    pub cache_write_1h: u64,
    /// Output, thinking included.
    pub output: u64,
    /// The thinking part of `output`, where the harness says.
    pub reasoning: u64,
    /// Claude's fast mode, OpenAI's priority tier.
    pub fast: bool,
    pub searches: u64,
    /// What the harness recorded it cost, in dollars (OpenCode does, from your provider's prices).
    pub cost: Option<f64>,
}

impl TokenUse {
    /// The whole prompt: what decides whether a long-context price applies.
    pub fn prompt(&self) -> u64 {
        self.input + self.cache_read + self.cache_write + self.cache_write_1h
    }
}

/// What earlier lines of a record said, carried from line to line and from one read to the
/// next: the folder and model lines name once for many calls, Codex's running total.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TokenState {
    /// The file's session, from the listing each read.
    #[serde(skip)]
    pub session: String,
    #[serde(default)]
    pub folder: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub fast: bool,
    #[serde(default)]
    pub total: u64,
    /// The session whose work this record's calls are, when that's another's than the file's
    /// own: a subagent's, by its parent's ID.
    #[serde(default)]
    pub parent: String,
}

impl TokenState {
    /// The key of the session the calls go to: the file's, or its parent's when it has one, in
    /// the same harness and account.
    pub fn owner(&self) -> String {
        match (self.parent.is_empty(), self.session.rsplit_once(':')) {
            (false, Some((prefix, _))) => format!("{prefix}:{}", self.parent),
            _ => self.session.clone(),
        }
    }
}

/// The ledger's file.
pub fn path() -> PathBuf {
    crate::config::support_dir().join("ledger.sqlite")
}

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS sessions (
  id INTEGER PRIMARY KEY,
  key TEXT NOT NULL UNIQUE,
  account TEXT NOT NULL,
  folder TEXT NOT NULL DEFAULT ''
);
CREATE TABLE IF NOT EXISTS calls (
  id INTEGER PRIMARY KEY,
  at INTEGER NOT NULL,
  session INTEGER NOT NULL,
  model TEXT NOT NULL,
  input INTEGER NOT NULL DEFAULT 0,
  cache_read INTEGER NOT NULL DEFAULT 0,
  cache_write INTEGER NOT NULL DEFAULT 0,
  cache_write_1h INTEGER NOT NULL DEFAULT 0,
  output INTEGER NOT NULL DEFAULT 0,
  reasoning INTEGER NOT NULL DEFAULT 0,
  fast INTEGER NOT NULL DEFAULT 0,
  searches INTEGER NOT NULL DEFAULT 0,
  prompt INTEGER NOT NULL DEFAULT 0,
  cost REAL
);
CREATE INDEX IF NOT EXISTS calls_at ON calls(at);
CREATE INDEX IF NOT EXISTS calls_session ON calls(session);
CREATE TABLE IF NOT EXISTS files (
  path TEXT PRIMARY KEY,
  offset INTEGER NOT NULL,
  size INTEGER NOT NULL,
  mtime INTEGER NOT NULL,
  state TEXT NOT NULL DEFAULT '{}'
);
CREATE TABLE IF NOT EXISTS parents (
  key TEXT PRIMARY KEY,
  parent TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS extra (
  account TEXT NOT NULL,
  at INTEGER NOT NULL,
  used REAL NOT NULL,
  monthly_limit REAL,
  currency TEXT NOT NULL,
  PRIMARY KEY (account, at)
);
";

pub fn open(path: &Path) -> Result<Connection> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let db = Connection::open(path).with_context(|| format!("Couldn't open {}", path.display()))?;
    db.busy_timeout(Duration::from_secs(10))?;
    db.pragma_update(None, "journal_mode", "WAL")?;
    db.execute_batch(SCHEMA)?;
    Ok(db)
}

/// The app's ledger, opened on first use.
fn shared() -> &'static Mutex<Option<Connection>> {
    static DB: OnceLock<Mutex<Option<Connection>>> = OnceLock::new();
    DB.get_or_init(|| Mutex::new(None))
}

fn with_db<R>(f: impl FnOnce(&Connection) -> Result<R>) -> Result<R> {
    let mut slot = shared().lock().unwrap_or_else(|e| e.into_inner());
    if slot.is_none() {
        *slot = Some(open(&path())?);
    }
    f(slot.as_ref().expect("opened above"))
}

/// When the ledger last caught up with every record this launch, and whether its first read of
/// them all ever is still going: a ledger that wasn't there at launch is being filled.
static CAUGHT_UP: AtomicI64 = AtomicI64::new(0);
static FILLING: AtomicBool = AtomicBool::new(false);

fn nudger() -> &'static Mutex<Option<Sender<()>>> {
    static TX: OnceLock<Mutex<Option<Sender<()>>>> = OnceLock::new();
    TX.get_or_init(|| Mutex::new(None))
}

/// Reads every account's records after `delay`, then what's new every minute or when nudged.
pub fn spawn(delay: Duration) {
    let (tx, rx) = channel::<()>();
    *nudger().lock().unwrap_or_else(|e| e.into_inner()) = Some(tx);
    FILLING.store(!path().exists(), Ordering::Relaxed);
    std::thread::spawn(move || {
        crate::procs::utility_thread();
        std::thread::sleep(delay);
        // A connection of its own: the windows' reports read through the shared one meanwhile,
        // which WAL lets them do while this one writes.
        let db = match open(&path()) {
            Ok(db) => db,
            Err(e) => {
                *problem_slot().lock().unwrap_or_else(|e| e.into_inner()) = Some(format!("The token ledger couldn't open: {e:#}"));
                return;
            }
        };
        loop {
            let t = Instant::now();
            let accounts = crate::config::current().map(|c| c.accounts).unwrap_or_default();
            let caught = catch_up(&db, &accounts);
            *problem_slot().lock().unwrap_or_else(|e| e.into_inner()) = caught.problem();
            CAUGHT_UP.store(now_ms(), Ordering::Relaxed);
            FILLING.store(false, Ordering::Relaxed);
            if caught.read > 0 {
                crate::perf::log(&format!("token ledger: read {} MB in {:?}", caught.read / (1024 * 1024), t.elapsed()));
            }
            match rx.recv_timeout(Duration::from_secs(60)) {
                Ok(()) | Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(_) => break,
            }
            while rx.try_recv().is_ok() {}
        }
    });
}

fn problem_slot() -> &'static Mutex<Option<String>> {
    static SLOT: OnceLock<Mutex<Option<String>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(None))
}

/// What the last read of the records couldn't read, in words, for a notice and the Usage
/// window: its numbers leave those records out until a read gets them.
pub fn problem() -> Option<String> {
    problem_slot().lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// Asks the background thread to read what's new now: the Usage window is opening.
pub fn nudge() {
    if let Some(tx) = nudger().lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
        let _ = tx.send(());
    }
}

/// The call's row ID: its harness's name for it, hashed. Not the account's: a copy in another
/// account's records (a session moved there) is the same call.
fn row_id(harness: &str, id: &str) -> i64 {
    let h = Sha256::digest(format!("{harness}:{id}").as_bytes());
    i64::from_be_bytes(h[..8].try_into().expect("eight bytes"))
}

struct FileRow {
    offset: u64,
    size: u64,
    mtime: i64,
    state: TokenState,
}

fn file_row(db: &Connection, path: &str) -> Result<Option<FileRow>> {
    Ok(db
        .query_row("SELECT offset, size, mtime, state FROM files WHERE path = ?1", [path], |r| {
            Ok(FileRow { offset: r.get::<_, i64>(0)? as u64, size: r.get::<_, i64>(1)? as u64, mtime: r.get(2)?, state: serde_json::from_str(&r.get::<_, String>(3)?).unwrap_or_default() })
        })
        .optional()?)
}

/// What a read of every record did: how many bytes it read, and what it couldn't read.
#[derive(Debug, Default)]
pub struct Caught {
    pub read: u64,
    pub failed: Vec<String>,
}

impl Caught {
    /// The failures in a sentence, or none.
    pub fn problem(&self) -> Option<String> {
        let first = self.failed.first()?;
        Some(match self.failed.len() {
            1 => format!("Couldn't read what a session used: {first}"),
            n => format!("Couldn't read what sessions used from {n} records, the first {first}"),
        })
    }
}

/// Reads every account's records into `db`, each file from where it was left. Each file's calls
/// go in with one transaction, so a report never waits long on it.
pub fn catch_up(db: &Connection, accounts: &[AccountCfg]) -> Caught {
    catch_up_since(db, accounts, 0)
}

/// `catch_up`, leaving out files last written before `since`: for a check of recent days
/// (`--snapshot --tokens`), which needn't read years of records.
pub fn catch_up_since(db: &Connection, accounts: &[AccountCfg], since: i64) -> Caught {
    let mut files = Vec::new();
    for account in accounts {
        let Some(adapter) = crate::harness::of(account) else { continue };
        for (path, session) in adapter.token_files(account) {
            let Ok(meta) = std::fs::metadata(&path) else { continue };
            if since > 0 && crate::util::mtime_ms(&path).unwrap_or(0) < since {
                continue;
            }
            files.push((meta.created().ok(), account, adapter, path, session));
        }
    }
    // Oldest first, every account's together: a call a later file copies (a fork, a session
    // moved to another account) stays with the session it came from.
    files.sort_by_key(|f| f.0);
    let mut caught = Caught::default();
    for (_, account, adapter, path, session) in files {
        match read_file(db, account, adapter.harness().config(), &path, &session, &|line, st| adapter.token_use(line, st)) {
            Ok(n) => caught.read += n,
            // Deleted since it was listed, as Claude Code deletes old transcripts: nothing to read.
            Err(e) if e.downcast_ref::<std::io::Error>().is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound) => {}
            Err(e) => caught.failed.push(format!("{}: {e:#}", crate::util::tilde(&path))),
        }
    }
    for account in accounts {
        let Some(adapter) = crate::harness::of(account) else { continue };
        if let Err(e) = read_rows(db, account, adapter.harness().config(), &|after| adapter.token_uses_since(account, after.max(since))) {
            caught.failed.push(format!("{}'s calls: {e:#}", account.name));
        }
    }
    caught
}

/// Reads one file from where the last read stopped, a line at a time, up to its last whole
/// line. A file that got shorter was written anew and is read from the start.
fn read_file(db: &Connection, account: &AccountCfg, tag: &str, path: &Path, session: &str, parse: &dyn Fn(&[u8], &mut TokenState) -> Option<TokenUse>) -> Result<u64> {
    let meta = std::fs::metadata(path)?;
    let size = meta.len();
    let mtime = crate::util::mtime_ms(path).unwrap_or(0);
    let key = path.to_string_lossy().to_string();
    let had = file_row(db, &key)?;
    if had.as_ref().is_some_and(|f| f.size == size && f.mtime == mtime) {
        return Ok(0);
    }
    let (mut offset, mut state) = match had {
        Some(f) if f.offset <= size => (f.offset, f.state),
        _ => (0, TokenState::default()),
    };
    state.session = session.to_string();
    let mut file = File::open(path)?;
    file.seek(SeekFrom::Start(offset))?;
    let mut reader = BufReader::with_capacity(256 * 1024, file);
    let mut line = Vec::new();
    let mut uses = Vec::new();
    let start = offset;
    loop {
        line.clear();
        let n = reader.read_until(b'\n', &mut line)?;
        if n == 0 || line.last() != Some(&b'\n') {
            break;
        }
        offset += n as u64;
        if let Some(u) = parse(&line, &mut state) {
            uses.push(u);
        }
    }
    let tx = db.unchecked_transaction()?;
    // A subagent's record names its parent, which may be a subagent too: the chain leads its calls
    // to the session at its root (`put_uses`). Records are read oldest first, so a parent's own
    // parent is known before its subagents' calls come.
    if !state.parent.is_empty() {
        tx.execute("INSERT OR REPLACE INTO parents (key, parent) VALUES (?1, ?2)", params![session, state.owner()])?;
    }
    put_uses(&tx, account, tag, &uses)?;
    tx.execute(
        "INSERT INTO files (path, offset, size, mtime, state) VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(path) DO UPDATE SET offset = excluded.offset, size = excluded.size, mtime = excluded.mtime, state = excluded.state",
        params![key, offset as i64, size as i64, mtime, serde_json::to_string(&state)?],
    )?;
    tx.commit()?;
    Ok(offset - start)
}

/// Reads the calls a harness keeps in a database, from the newest write the last read saw.
fn read_rows(db: &Connection, account: &AccountCfg, tag: &str, since: &dyn Fn(i64) -> std::result::Result<(Vec<TokenUse>, i64), String>) -> Result<()> {
    let key = format!("{tag}:{}#rows", account.name);
    let after = file_row(db, &key)?.map(|f| f.offset as i64).unwrap_or(0);
    let (uses, newest) = since(after).map_err(anyhow::Error::msg)?;
    if uses.is_empty() && newest == after {
        return Ok(());
    }
    let tx = db.unchecked_transaction()?;
    put_uses(&tx, account, tag, &uses)?;
    tx.execute(
        "INSERT INTO files (path, offset, size, mtime, state) VALUES (?1, ?2, 0, 0, '{}')
         ON CONFLICT(path) DO UPDATE SET offset = excluded.offset",
        params![key, newest.max(after)],
    )?;
    tx.commit()?;
    Ok(())
}

fn put_uses(db: &Connection, account: &AccountCfg, tag: &str, uses: &[TokenUse]) -> Result<()> {
    // Each call's session, followed up its parents to the one at the root.
    let mut roots: HashMap<String, String> = HashMap::new();
    let mut parent_of = db.prepare_cached("SELECT parent FROM parents WHERE key = ?1")?;
    let mut root = |key: &str| -> Result<String> {
        if let Some(r) = roots.get(key) {
            return Ok(r.clone());
        }
        let mut at = key.to_string();
        for _ in 0..16 {
            match parent_of.query_row([&at], |r| r.get::<_, String>(0)).optional()? {
                Some(up) if up != at => at = up,
                _ => break,
            }
        }
        roots.insert(key.to_string(), at.clone());
        Ok(at)
    };
    let uses: Vec<TokenUse> = uses.iter().map(|u| Ok(TokenUse { session: root(&u.session)?, ..u.clone() })).collect::<Result<_>>()?;
    let uses = &uses[..];
    let mut sessions: HashMap<String, i64> = HashMap::new();
    let mut folders: HashMap<String, String> = HashMap::new();
    for u in uses {
        if !u.folder.is_empty() {
            folders.insert(u.session.clone(), u.folder.clone());
        }
    }
    let mut add_session = db.prepare_cached("INSERT INTO sessions (key, account, folder) VALUES (?1, ?2, ?3) ON CONFLICT(key) DO UPDATE SET folder = CASE WHEN excluded.folder != '' THEN excluded.folder ELSE folder END")?;
    let mut session_id = db.prepare_cached("SELECT id FROM sessions WHERE key = ?1")?;
    let mut add = db.prepare_cached(
        "INSERT INTO calls (id, at, session, model, input, cache_read, cache_write, cache_write_1h, output, reasoning, fast, searches, prompt, cost)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
         ON CONFLICT(id) DO UPDATE SET
           input = max(input, excluded.input), cache_read = max(cache_read, excluded.cache_read),
           cache_write = max(cache_write, excluded.cache_write), cache_write_1h = max(cache_write_1h, excluded.cache_write_1h),
           output = max(output, excluded.output), reasoning = max(reasoning, excluded.reasoning),
           searches = max(searches, excluded.searches), prompt = max(prompt, excluded.prompt),
           fast = max(fast, excluded.fast), cost = coalesce(excluded.cost, cost)",
    )?;
    for u in uses {
        let sid = match sessions.get(&u.session) {
            Some(id) => *id,
            None => {
                add_session.execute(params![u.session, account.name, folders.get(&u.session).cloned().unwrap_or_default()])?;
                let id: i64 = session_id.query_row([&u.session], |r| r.get(0))?;
                sessions.insert(u.session.clone(), id);
                id
            }
        };
        add.execute(params![
            row_id(tag, &u.id),
            u.at,
            sid,
            u.model,
            u.input as i64,
            u.cache_read as i64,
            u.cache_write as i64,
            u.cache_write_1h as i64,
            u.output as i64,
            u.reasoning as i64,
            u.fast as i64,
            u.searches as i64,
            u.prompt() as i64,
            u.cost,
        ])?;
    }
    Ok(())
}

/// Claude's extra usage this billing period, as its usage endpoint reported it at `at`. A
/// reading the same as the last is kept only after six hours, so the file doesn't grow with each
/// two-minute read.
pub fn note_extra(account: &str, at: i64, used: f64, monthly_limit: Option<f64>, currency: &str) {
    let done = with_db(|db| note_extra_in(db, account, at, used, monthly_limit, currency));
    if let Err(e) = done {
        eprintln!("The token ledger couldn't note {account}'s extra usage: {e:#}");
    }
}

fn note_extra_in(db: &Connection, account: &str, at: i64, used: f64, monthly_limit: Option<f64>, currency: &str) -> Result<()> {
    let last: Option<(i64, f64)> = db.query_row("SELECT at, used FROM extra WHERE account = ?1 ORDER BY at DESC LIMIT 1", [account], |r| Ok((r.get(0)?, r.get(1)?))).optional()?;
    if last.is_some_and(|(t, u)| (u - used).abs() < 1e-9 && at - t < 6 * 3600 * 1000) {
        return Ok(());
    }
    db.execute("INSERT OR REPLACE INTO extra (account, at, used, monthly_limit, currency) VALUES (?1, ?2, ?3, ?4, ?5)", params![account, at, used, monthly_limit, currency])?;
    Ok(())
}

// ---- Reports ----

/// Tokens of each kind.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tokens {
    pub input: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    pub output: u64,
    pub reasoning: u64,
}

impl Tokens {
    pub fn total(&self) -> u64 {
        self.input + self.cache_read + self.cache_write + self.output
    }
    fn add(&mut self, o: &Tokens) {
        self.input += o.input;
        self.cache_read += o.cache_read;
        self.cache_write += o.cache_write;
        self.output += o.output;
        self.reasoning += o.reasoning;
    }
}

/// What a group of calls used and cost.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Spent {
    /// The account, model, folder or session key the row is for; empty for a total.
    pub key: String,
    /// What it would have cost at API prices, in dollars, its unpriced calls left out.
    pub api: f64,
    /// What was really paid for it by the token: what the harness recorded, or an account's
    /// that pays by the token, at API prices. A plan's share and extra usage are the report's.
    pub paid: f64,
    pub tokens: Tokens,
    pub calls: u64,
    /// Tokens of calls no price was found for.
    pub unpriced: u64,
    /// The newest call.
    pub last_at: i64,
    /// The session's account, for a session's row.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
    /// The session's folder, for a session's row.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<String>,
}

impl Spent {
    fn add(&mut self, o: &Spent) {
        self.api += o.api;
        self.paid += o.paid;
        self.tokens.add(&o.tokens);
        self.calls += o.calls;
        self.unpriced += o.unpriced;
        self.last_at = self.last_at.max(o.last_at);
    }
}

/// One day, local time.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Day {
    /// `2026-10-12`.
    pub day: String,
    /// At API prices, by account.
    pub api: BTreeMap<String, f64>,
    /// Really paid: each plan's share of its month, extra usage, and by the token.
    pub paid: f64,
    pub tokens: u64,
}

/// What an account paid in the report's days, by kind.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Paid {
    pub account: String,
    /// Its `monthly` setting.
    pub monthly: Option<f64>,
    pub per_token: bool,
    /// The plan's share of the days.
    pub plan: f64,
    /// Claude's extra usage in the days, as its readings went up.
    pub extra: f64,
    /// The latest extra usage reading: this period's total and its monthly limit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extra_now: Option<(f64, Option<f64>)>,
    /// By the token: recorded, or at API prices for a `per_token` account.
    pub tokens: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub days: Vec<Day>,
    pub total: Spent,
    /// What was really paid in all, plans and extra usage included.
    pub paid: f64,
    pub accounts: Vec<Spent>,
    pub models: Vec<Spent>,
    pub folders: Vec<Spent>,
    /// The sessions that cost the most, at most `SESSIONS` of them.
    pub sessions: Vec<Spent>,
    pub paying: Vec<Paid>,
    /// Models with calls no price was found for.
    pub unpriced: Vec<String>,
    /// Accounts whose harness records no tokens Codealong can read.
    pub untracked: Vec<String>,
    /// When the ledger last caught up with every record; 0 before its first read ends.
    pub as_of: i64,
    pub filling: bool,
    /// What the last read of the records couldn't read (`problem`), which these numbers leave out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
}

const SESSIONS: usize = 200;

/// The grouped sums a report prices: a day, session, model, speed and prompt size each.
struct Group {
    day: String,
    session: String,
    account: String,
    folder: String,
    model: String,
    fast: bool,
    prompt: u64,
    tokens: Tokens,
    cache_write_1h: u64,
    searches: u64,
    calls: u64,
    recorded: Option<f64>,
    last_at: i64,
}

/// Which calls a report sums: those since a time, for one account or all, or one session's.
enum Calls<'a> {
    Since(i64, Option<&'a str>),
    Session(&'a str),
}

/// The calls, summed by day, session, model, speed, how far their prompts went past the sizes
/// some model's prices change above (prices.rs `thresholds`: priced as a prompt just past the
/// last one passed) and whether the harness recorded a cost, so each sum is priced one way.
fn groups(db: &Connection, calls: Calls, thresholds: &[u64]) -> Result<Vec<Group>> {
    let step = match thresholds {
        [] => "0".to_string(),
        _ => format!("CASE {} ELSE 0 END", thresholds.iter().rev().map(|t| format!("WHEN c.prompt > {t} THEN {}", t + 1)).collect::<Vec<_>>().join(" ")),
    };
    // Written out for each, so SQLite looks a session's calls up by its index.
    let (filter, args): (&str, Vec<rusqlite::types::Value>) = match calls {
        Calls::Since(from, None) => ("c.at >= ?1", vec![from.into()]),
        Calls::Since(from, Some(account)) => ("c.at >= ?1 AND s.account = ?2", vec![from.into(), account.to_string().into()]),
        Calls::Session(key) => ("c.session = (SELECT id FROM sessions WHERE key = ?1)", vec![key.to_string().into()]),
    };
    let sql = format!(
        "SELECT date(c.at / 1000, 'unixepoch', 'localtime') AS day, s.key, s.account, s.folder, c.model, c.fast, {step} AS step,
                sum(c.input), sum(c.cache_read), sum(c.cache_write), sum(c.cache_write_1h), sum(c.output), sum(c.reasoning),
                sum(c.searches), count(*), sum(c.cost), max(c.at), coalesce(c.cost, 0) > 0 AS recorded
         FROM calls c JOIN sessions s ON s.id = c.session
         WHERE {filter}
         GROUP BY day, c.session, c.model, c.fast, step, recorded"
    );
    let mut stmt = db.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(args), |r| {
        let cache_write: i64 = r.get(9)?;
        let cache_write_1h: i64 = r.get(10)?;
        Ok(Group {
            day: r.get(0)?,
            session: r.get(1)?,
            account: r.get(2)?,
            folder: r.get(3)?,
            model: r.get(4)?,
            fast: r.get::<_, i64>(5)? != 0,
            prompt: r.get::<_, i64>(6)? as u64,
            tokens: Tokens {
                input: r.get::<_, i64>(7)? as u64,
                cache_read: r.get::<_, i64>(8)? as u64,
                cache_write: (cache_write + cache_write_1h) as u64,
                output: r.get::<_, i64>(11)? as u64,
                reasoning: r.get::<_, i64>(12)? as u64,
            },
            cache_write_1h: cache_write_1h as u64,
            searches: r.get::<_, i64>(13)? as u64,
            calls: r.get::<_, i64>(14)? as u64,
            recorded: r.get::<_, Option<f64>>(15)?.filter(|_| r.get::<_, i64>(17).is_ok_and(|x| x != 0)),
            last_at: r.get(16)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// A group priced: at API prices (what the harness recorded, when it recorded a cost), and what
/// was paid for it by the token.
fn price_group(g: &Group, price: Option<&Price>, per_token: bool) -> Spent {
    let used = Used {
        input: g.tokens.input,
        cache_read: g.tokens.cache_read,
        cache_write: g.tokens.cache_write - g.cache_write_1h,
        cache_write_1h: g.cache_write_1h,
        output: g.tokens.output,
        searches: g.searches,
        prompt: g.prompt,
        fast: g.fast,
    };
    let recorded = g.recorded.filter(|c| *c > 0.0);
    let api = recorded.or_else(|| price.and_then(|p| prices::cost(p, &used)));
    let paid = recorded.or(if per_token { api } else { None }).unwrap_or(0.0);
    Spent {
        key: String::new(),
        api: api.unwrap_or(0.0),
        paid,
        tokens: g.tokens,
        calls: g.calls,
        unpriced: if api.is_none() { g.tokens.total() } else { 0 },
        last_at: g.last_at,
        account: None,
        folder: None,
    }
}

/// The local date `days` days before today's, and the dates from it through today.
fn day_range(db: &Connection, days: u32) -> Result<Vec<String>> {
    let first: String = db.query_row("SELECT date('now', 'localtime', ?1)", [format!("-{} days", days.max(1) - 1)], |r| r.get(0))?;
    let today: String = db.query_row("SELECT date('now', 'localtime')", [], |r| r.get(0))?;
    let mut out = vec![first];
    while out.last().is_some_and(|d| *d < today) && out.len() < 20_000 {
        let next = next_day(out.last().expect("not empty"));
        out.push(next);
    }
    Ok(out)
}

/// `2026-10-12` → `2026-10-13`.
pub fn next_day(day: &str) -> String {
    let mut it = day.split('-').map(|p| p.parse::<u32>().unwrap_or(1));
    let (mut y, mut m, mut d) = (it.next().unwrap_or(1970), it.next().unwrap_or(1), it.next().unwrap_or(1));
    d += 1;
    if d > days_in_month(y, m) {
        d = 1;
        m += 1;
        if m > 12 {
            m = 1;
            y += 1;
        }
    }
    format!("{y:04}-{m:02}-{d:02}")
}

pub fn days_in_month(y: u32, m: u32) -> u32 {
    match m {
        2 if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// A day's share of a month's price.
fn plan_share(day: &str, monthly: f64) -> f64 {
    let mut it = day.split('-').map(|p| p.parse::<u32>().unwrap_or(1));
    let (y, m) = (it.next().unwrap_or(1970), it.next().unwrap_or(1));
    monthly / days_in_month(y, m) as f64
}

/// What every call in the last `days` days (today and the days before it) used and cost, for
/// one account or all of them, and what was paid.
pub fn report(days: u32, account: Option<&str>) -> Result<Report> {
    let cfg = crate::config::current()?;
    let mut r = with_db(|db| report_in(db, &cfg, days, account, now_ms()))?;
    r.as_of = CAUGHT_UP.load(Ordering::Relaxed);
    r.filling = FILLING.load(Ordering::Relaxed);
    r.problem = problem();
    Ok(r)
}

pub fn report_in(db: &Connection, cfg: &crate::config::Config, days: u32, account: Option<&str>, now: i64) -> Result<Report> {
    let dates = day_range(db, days)?;
    // A day's calls start at its local midnight, at most a day and a bit before its date in UTC.
    let from = now - (days.max(1) as i64 + 1) * DAY_MS;
    let first = dates.first().cloned().unwrap_or_default();
    let accounts: Vec<&AccountCfg> = cfg.accounts.iter().filter(|a| account.is_none_or(|n| n == a.name)).collect();
    let per_token: HashMap<&str, bool> = accounts.iter().map(|a| (a.name.as_str(), a.per_token)).collect();
    let mut price_cache: HashMap<String, Option<Price>> = HashMap::new();
    let mut unpriced: BTreeMap<String, ()> = BTreeMap::new();

    let mut total = Spent::default();
    let mut by_account: BTreeMap<String, Spent> = BTreeMap::new();
    let mut by_model: BTreeMap<String, Spent> = BTreeMap::new();
    let mut by_folder: BTreeMap<String, Spent> = BTreeMap::new();
    let mut by_session: HashMap<String, Spent> = HashMap::new();
    let mut by_day: BTreeMap<String, Day> = dates.iter().map(|d| (d.clone(), Day { day: d.clone(), ..Default::default() })).collect();
    let mut paying: BTreeMap<String, Paid> = accounts
        .iter()
        .map(|a| (a.name.clone(), Paid { account: a.name.clone(), monthly: a.monthly, per_token: a.per_token, ..Default::default() }))
        .collect();

    for g in groups(db, Calls::Since(from, account), &prices::thresholds(&cfg.prices))? {
        if g.day < first {
            continue;
        }
        let price = price_cache.entry(g.model.clone()).or_insert_with(|| prices::price_of(&g.model, &cfg.prices)).as_ref();
        let s = price_group(&g, price, per_token.get(g.account.as_str()).copied().unwrap_or(false));
        if s.unpriced > 0 {
            unpriced.insert(g.model.clone(), ());
        }
        total.add(&s);
        by_account.entry(g.account.clone()).or_insert_with(|| Spent { key: g.account.clone(), ..Default::default() }).add(&s);
        by_model.entry(g.model.clone()).or_insert_with(|| Spent { key: g.model.clone(), ..Default::default() }).add(&s);
        by_folder.entry(g.folder.clone()).or_insert_with(|| Spent { key: g.folder.clone(), ..Default::default() }).add(&s);
        by_session
            .entry(g.session.clone())
            .or_insert_with(|| Spent { key: g.session.clone(), account: Some(g.account.clone()), folder: Some(g.folder.clone()), ..Default::default() })
            .add(&s);
        if let Some(d) = by_day.get_mut(&g.day) {
            *d.api.entry(g.account.clone()).or_insert(0.0) += s.api;
            d.paid += s.paid;
            d.tokens += s.tokens.total();
        }
        if let Some(p) = paying.get_mut(&g.account) {
            p.tokens += s.paid;
        }
    }

    // Plans, a day's share at a time, and extra usage as its readings went up.
    for a in &accounts {
        let p = paying.get_mut(&a.name).expect("every account has a row");
        if let Some(monthly) = a.monthly.filter(|m| *m > 0.0) {
            for d in by_day.values_mut() {
                let share = plan_share(&d.day, monthly);
                d.paid += share;
                p.plan += share;
            }
        }
        let mut stmt = db.prepare_cached("SELECT at, used, monthly_limit, date(at / 1000, 'unixepoch', 'localtime') FROM extra WHERE account = ?1 ORDER BY at")?;
        let readings = stmt.query_map([&a.name], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, f64>(1)?, r.get::<_, Option<f64>>(2)?, r.get::<_, String>(3)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
        let mut before: Option<f64> = None;
        for (_, used, limit, day) in &readings {
            // A reading below the last is a new billing period, its whole amount new.
            let added = match before {
                Some(b) if *used >= b => used - b,
                _ => *used,
            };
            before = Some(*used);
            if added > 0.0 && *day >= first {
                if let Some(d) = by_day.get_mut(day) {
                    d.paid += added;
                }
                p.extra += added;
            }
            p.extra_now = Some((*used, *limit));
        }
        // An account's row says all it paid, its plan and extra usage with what it paid by the
        // token, as the days do; an account that used nothing in them still paid for its plan.
        let more = p.plan + p.extra;
        if more > 0.0 {
            by_account.entry(a.name.clone()).or_insert_with(|| Spent { key: a.name.clone(), ..Default::default() }).paid += more;
        }
    }

    let paid = by_day.values().map(|d| d.paid).sum();
    let untracked = accounts.iter().filter(|a| crate::harness::of(a).is_none_or(|h| !h.records_tokens())).map(|a| a.name.clone()).collect();
    let by_cost = |m: BTreeMap<String, Spent>| {
        let mut v: Vec<Spent> = m.into_values().collect();
        v.sort_by(|a, b| b.api.total_cmp(&a.api).then(b.tokens.total().cmp(&a.tokens.total())));
        v
    };
    let mut sessions: Vec<Spent> = by_session.into_values().collect();
    sessions.sort_by(|a, b| b.api.total_cmp(&a.api).then(b.last_at.cmp(&a.last_at)));
    sessions.truncate(SESSIONS);
    Ok(Report {
        days: by_day.into_values().collect(),
        total,
        paid,
        accounts: by_cost(by_account),
        models: by_cost(by_model),
        folders: by_cost(by_folder),
        sessions,
        paying: paying.into_values().collect(),
        unpriced: unpriced.into_keys().collect(),
        untracked,
        as_of: 0,
        filling: false,
        problem: None,
    })
}

/// What one session used and cost, by model, its subagents' calls included.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSpent {
    pub total: Spent,
    pub models: Vec<Spent>,
}

pub fn session(key: &str) -> Result<SessionSpent> {
    let cfg = crate::config::current()?;
    with_db(|db| session_in(db, &cfg, key))
}

pub fn session_in(db: &Connection, cfg: &crate::config::Config, key: &str) -> Result<SessionSpent> {
    let mut total = Spent { key: key.to_string(), ..Default::default() };
    let mut models: BTreeMap<String, Spent> = BTreeMap::new();
    for g in groups(db, Calls::Session(key), &prices::thresholds(&cfg.prices))? {
        let per_token = cfg.accounts.iter().any(|a| a.name == g.account && a.per_token);
        let s = price_group(&g, prices::price_of(&g.model, &cfg.prices).as_ref(), per_token);
        total.add(&s);
        models.entry(g.model.clone()).or_insert_with(|| Spent { key: g.model.clone(), ..Default::default() }).add(&s);
    }
    let mut models: Vec<Spent> = models.into_values().collect();
    models.sort_by(|a, b| b.api.total_cmp(&a.api));
    Ok(SessionSpent { total, models })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn account(name: &str) -> AccountCfg {
        AccountCfg { name: name.into(), harness: "claude".into(), home: "~/.claude".into(), command: None, start: None, preferred: false, acp: None, monthly: None, per_token: false }
    }

    fn call(id: &str, session: &str, at: i64, input: u64, output: u64) -> TokenUse {
        TokenUse { id: id.into(), at, session: session.into(), folder: "/work/app".into(), model: "claude-opus-5-5".into(), input, output, ..Default::default() }
    }

    fn db() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(SCHEMA).unwrap();
        db
    }

    #[test]
    fn a_call_written_several_times_is_one_with_the_largest_counts() {
        let db = db();
        let a = account("claude");
        let now = now_ms();
        // The first copy of a streamed reply has the output so far; the last, all of it.
        put_uses(&db, &a, "claude", &[call("msg_1:req_1", "claude:claude:s1", now, 10, 5), call("msg_1:req_1", "claude:claude:s1", now, 10, 900)]).unwrap();
        // A fork copies it into its own transcript; it stays the first session's.
        put_uses(&db, &a, "claude", &[call("msg_1:req_1", "claude:claude:fork", now, 10, 900)]).unwrap();
        let (n, out, session): (i64, i64, String) = db.query_row("SELECT count(*), max(output), s.key FROM calls c JOIN sessions s ON s.id = c.session", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap();
        assert_eq!((n, out, session.as_str()), (1, 900, "claude:claude:s1"));
    }

    #[test]
    fn a_report_prices_calls_by_day_account_model_folder_and_session() {
        let db = db();
        let now = now_ms();
        put_uses(&db, &account("claude"), "claude", &[call("a", "claude:claude:s1", now, 1_000_000, 0), call("b", "claude:claude:s2", now, 0, 1_000_000)]).unwrap();
        let cfg = Config { accounts: vec![account("claude")], ..Default::default() };
        let r = report_in(&db, &cfg, 7, None, now).unwrap();
        // Claude Opus 5.5: $4 a million in, $20 out.
        assert!((r.total.api - 24.0).abs() < 1e-9, "{}", r.total.api);
        assert_eq!(r.days.len(), 7);
        assert!((r.days.last().unwrap().api["claude"] - 24.0).abs() < 1e-9);
        assert_eq!(r.sessions.iter().map(|s| s.key.as_str()).collect::<Vec<_>>(), ["claude:claude:s2", "claude:claude:s1"]);
        assert_eq!(r.folders[0].key, "/work/app");
        assert_eq!(r.paid, 0.0, "a plan with no price set pays nothing the report knows of");
    }

    #[test]
    fn an_unknown_model_is_unpriced_not_free() {
        let db = db();
        let now = now_ms();
        let mut c = call("a", "codex:codex:t", now, 500, 500);
        c.model = "gpt-0-unknown".into();
        put_uses(&db, &account("codex"), "codex", &[c]).unwrap();
        let r = report_in(&db, &Config { accounts: vec![account("codex")], ..Default::default() }, 7, None, now).unwrap();
        assert_eq!(r.unpriced, ["gpt-0-unknown"]);
        assert_eq!(r.total.unpriced, 1000);
        assert_eq!(r.total.api, 0.0);
    }

    #[test]
    fn paid_is_a_plans_daily_share_extra_usage_as_it_rises_and_per_token_accounts_at_api_prices() {
        let db = db();
        let now = now_ms();
        let mut plan = account("claude");
        plan.monthly = Some(310.0);
        let mut api = account("work");
        api.per_token = true;
        put_uses(&db, &api, "claude", &[call("x", "claude:work:s", now, 1_000_000, 0)]).unwrap();
        // $5 of extra usage this period when first read, $7 later today.
        note_extra_in(&db, "claude", now - 1000, 5.0, Some(100.0), "USD").unwrap();
        note_extra_in(&db, "claude", now, 7.0, Some(100.0), "USD").unwrap();
        let cfg = Config { accounts: vec![plan, api], ..Default::default() };
        let r = report_in(&db, &cfg, 1, None, now).unwrap();
        let claude = r.paying.iter().find(|p| p.account == "claude").unwrap();
        let today = r.days.last().unwrap().day.clone();
        let share = plan_share(&today, 310.0);
        assert!((claude.plan - share).abs() < 1e-9);
        assert!((claude.extra - 7.0).abs() < 1e-9);
        assert_eq!(claude.extra_now, Some((7.0, Some(100.0))));
        let work = r.paying.iter().find(|p| p.account == "work").unwrap();
        assert!((work.tokens - 4.0).abs() < 1e-9);
        assert!((r.paid - (share + 7.0 + 4.0)).abs() < 1e-9, "{}", r.paid);
    }

    #[test]
    fn a_recorded_cost_prices_its_own_calls_alone() {
        // One call a provider charged for, one a subscription's sign-in recorded as 0: the second
        // is priced from its tokens.
        let db = db();
        let now = now_ms();
        let mut paid = call("a", "opencode:opencode:s", now, 1_000_000, 0);
        paid.cost = Some(0.5);
        let mut free = call("b", "opencode:opencode:s", now, 1_000_000, 0);
        free.cost = Some(0.0);
        put_uses(&db, &account("opencode"), "opencode", &[paid, free]).unwrap();
        let r = report_in(&db, &Config { accounts: vec![account("opencode")], ..Default::default() }, 1, None, now).unwrap();
        assert!((r.total.api - (0.5 + 4.0)).abs() < 1e-9, "{}", r.total.api);
        assert!((r.total.paid - 0.5).abs() < 1e-9, "{}", r.total.paid);
    }

    #[test]
    fn a_long_prompt_threshold_from_config_toml_prices_by_it() {
        let db = db();
        let now = now_ms();
        let mut c = call("a", "claude:claude:s", now, 80_000, 0);
        c.model = "my-model".into();
        put_uses(&db, &account("claude"), "claude", &[c]).unwrap();
        let mut cfg = Config { accounts: vec![account("claude")], ..Default::default() };
        let rates = |input: f64| prices::Rates { input: Some(input), output: Some(1.0), ..Default::default() };
        cfg.prices.insert("my-model".into(), Price { rates: rates(1.0), above: Some(prices::Above { tokens: 64_000, rates: rates(10.0), priority: None }), ..Default::default() });
        let r = report_in(&db, &cfg, 1, None, now).unwrap();
        assert!((r.total.api - 0.8).abs() < 1e-9, "{}", r.total.api);
    }

    #[test]
    fn an_accounts_row_says_all_it_paid() {
        let db = db();
        let now = now_ms();
        let mut plan = account("claude");
        plan.monthly = Some(310.0);
        let idle = AccountCfg { monthly: Some(31.0), ..account("spare") };
        put_uses(&db, &plan, "claude", &[call("a", "claude:claude:s", now, 10, 10)]).unwrap();
        let r = report_in(&db, &Config { accounts: vec![plan, idle], ..Default::default() }, 1, None, now).unwrap();
        let today = r.days.last().unwrap().day.clone();
        let row = |name: &str| r.accounts.iter().find(|a| a.key == name).unwrap().paid;
        assert!((row("claude") - plan_share(&today, 310.0)).abs() < 1e-9);
        // An account that used nothing still paid its plan.
        assert!((row("spare") - plan_share(&today, 31.0)).abs() < 1e-9);
    }

    #[test]
    fn a_sessions_calls_are_looked_up_by_its_index() {
        let db = db();
        let plan: Vec<String> = db
            .prepare("EXPLAIN QUERY PLAN SELECT * FROM calls c JOIN sessions s ON s.id = c.session WHERE c.session = (SELECT id FROM sessions WHERE key = ?1)")
            .unwrap()
            .query_map(["k"], |r| r.get::<_, String>(3))
            .unwrap()
            .flatten()
            .collect();
        assert!(plan.iter().any(|p| p.contains("calls_session")), "{plan:?}");
    }

    #[test]
    fn extra_usage_that_falls_starts_a_new_period() {
        let db = db();
        let now = now_ms();
        let mut plan = account("claude");
        plan.monthly = None;
        note_extra_in(&db, "claude", now - 3000, 40.0, None, "USD").unwrap();
        note_extra_in(&db, "claude", now - 2000, 2.0, None, "USD").unwrap();
        let r = report_in(&db, &Config { accounts: vec![plan], ..Default::default() }, 1, None, now).unwrap();
        assert!((r.paying[0].extra - 42.0).abs() < 1e-9);
    }

    #[test]
    fn a_file_is_read_on_from_where_it_stopped_and_again_when_written_anew() {
        let dir = std::env::temp_dir().join(format!("codealong-ledger-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("t.jsonl");
        std::fs::write(&file, "1\n2\n").unwrap();
        let db = db();
        let a = account("claude");
        let parse = |line: &[u8], st: &mut TokenState| {
            let n: u64 = std::str::from_utf8(line).ok()?.trim().parse().ok()?;
            Some(TokenUse { id: n.to_string(), at: now_ms(), session: st.session.clone(), model: "m".into(), input: n, ..Default::default() })
        };
        read_file(&db, &a, "t", &file, "k", &parse).unwrap();
        // A line still being written isn't read until it ends.
        std::fs::write(&file, "1\n2\n3\n4").unwrap();
        read_file(&db, &a, "t", &file, "k", &parse).unwrap();
        let n: i64 = db.query_row("SELECT count(*) FROM calls", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 3);
        std::fs::write(&file, "9\n").unwrap();
        read_file(&db, &a, "t", &file, "k", &parse).unwrap();
        let n: i64 = db.query_row("SELECT count(*) FROM calls", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 4);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_subagents_subagent_counts_for_the_session_at_the_root() {
        let dir = std::env::temp_dir().join(format!("codealong-ledger-tree-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = db();
        let a = account("codex");
        // Each file's first line names its parent, the rest are calls.
        let parse = |line: &[u8], st: &mut TokenState| {
            let text = std::str::from_utf8(line).ok()?.trim().to_string();
            if let Some(parent) = text.strip_prefix("parent ") {
                st.parent = parent.to_string();
                return None;
            }
            Some(TokenUse { id: text, at: now_ms(), session: st.owner(), model: "m".into(), input: 1, ..Default::default() })
        };
        for (name, body) in [("child", "parent root\nc1\n"), ("grandchild", "parent child\ng1\ng2\n")] {
            let file = dir.join(name);
            std::fs::write(&file, body).unwrap();
            read_file(&db, &a, "codex", &file, &format!("codex:codex:{name}"), &parse).unwrap();
        }
        let owners: Vec<String> = db.prepare("SELECT DISTINCT s.key FROM calls c JOIN sessions s ON s.id = c.session").unwrap().query_map([], |r| r.get(0)).unwrap().flatten().collect();
        assert_eq!(owners, ["codex:codex:root"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn days_step_over_months_and_leap_years() {
        assert_eq!(next_day("2026-10-31"), "2026-11-01");
        assert_eq!(next_day("2026-12-31"), "2027-01-01");
        assert_eq!(next_day("2028-02-28"), "2028-02-29");
        assert_eq!(next_day("2026-02-28"), "2026-03-01");
    }
}
