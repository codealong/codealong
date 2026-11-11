//! Turns: what came in to each session from outside its agent, and who it was from (with
//! `[experimental] analytics` on). The ledger knows sessions and model calls; what you want to
//! know of your own part in the work hangs on the unit between them: a stretch of agent work
//! begun by something coming in. A prompt you typed, another session's message, what Codealong
//! sent by itself (a PR's news, a schedule's run), or the session carrying on by itself (a
//! background task reporting back, a wakeup it set). Each is a row here, in the ledger's file,
//! and every call is given the turn it was made in, so what was used rolls up by who began it.
//!
//! Each harness's records say what came in (`Adapter::turn_step`): Claude Code marks every line
//! with what began its turn, Codex's rollouts have their turns and the lines Codealong leads a
//! message with, and any other harness that can be searched is read through its `prose`, where
//! those lines tell an agent's message from yours. Records only grow at the end, so each file is
//! read whole once and then from where the last read stopped, as the ledger reads calls
//! (`pass`, run by the ledger's thread after it). A line a fork or a moved session copies is the
//! same turn, kept for the session that had it first: files are read oldest first.
//!
//! What a turn counts as (`counts_as`) is who began it, with two rules of its own (`recount`): a
//! turn the session began itself counts with the one it follows, since it's the same work going
//! on; and the first prompt of a session an agent or Codealong started is theirs, though it's
//! typed in as yours would be (began.rs). Second hand is then everything that doesn't count as
//! yours.
//!
//! A prompt of yours is kept with how it opens, for the lists. The whole of it and the end of
//! the reply before it are kept only until it's been read for what it did (stance.rs), and never
//! longer than Claude Code keeps its own transcripts.

use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::Path;

use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::began::Began;
use crate::config::AccountCfg;
use crate::ledger::Caught;
use crate::util::{now_ms, DAY_MS};

/// How much of a prompt is kept for its read, and of the reply before it.
pub const TEXT_MOST: usize = 2000;
pub const BEFORE_MOST: usize = 1500;
/// How much of a prompt of yours is kept for good: how it opens.
pub const OPENING: usize = 300;
/// A prompt's whole text is kept no longer than this, read or not.
const TEXTS_FOR_MS: i64 = 30 * DAY_MS;
/// How long after a session started a prompt can be the one it was started with.
const OPENS_WITHIN_MS: i64 = 10 * 60 * 1000;
/// Calls this far back are looked at again for a turn each pass: a record read late (a session
/// moved here, a file that was locked) brings calls older than the last pass.
const LATE_MS: i64 = 2 * DAY_MS;

/// Who something that came in was from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    /// You typed it, in the session's terminal or app or in Codealong.
    You,
    /// Another session's agent sent it.
    Agent,
    /// Codealong sent it by itself, as you set it up to.
    Codealong,
    /// The session's own doing: a background task reporting back, a wakeup it set, a loop's tick.
    Itself,
}

impl Source {
    pub fn word(self) -> &'static str {
        match self {
            Source::You => "you",
            Source::Agent => "agent",
            Source::Codealong => "codealong",
            Source::Itself => "itself",
        }
    }

    pub fn from_word(word: &str) -> Option<Source> {
        [Source::You, Source::Agent, Source::Codealong, Source::Itself].into_iter().find(|s| s.word() == word)
    }
}

/// What an answer to a question the agent put to you picked of what it offered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Picked {
    /// The first choice, which is the one an agent recommends when it recommends one.
    First,
    /// Another of the choices.
    Offered,
    /// Words of your own.
    Own,
}

impl Picked {
    pub fn word(self) -> &'static str {
        match self {
            Picked::First => "first",
            Picked::Offered => "offered",
            Picked::Own => "own",
        }
    }
}

/// Something that came in to a session, as its record has it (`Adapter::turn_step`).
#[derive(Clone, Debug, PartialEq)]
pub struct Came {
    /// The harness's own name for it, the same in every record that copies it.
    pub id: String,
    pub at: i64,
    pub source: Source,
    /// `prompt` (typed), `message` (sent by a session or Codealong), `answer` (to a question the
    /// agent asked), `task` (a background task or subagent reporting back) or `wakeup`.
    pub kind: &'static str,
    /// The session an agent's message came from: its key where the record gives it, else its name.
    pub sender: Option<String>,
    /// It came while the agent worked, into the turn it was taking.
    pub mid: bool,
    /// What it said, without the line Codealong leads a message with.
    pub text: String,
    pub picked: Option<Picked>,
}

/// What a record's earlier lines said, carried from line to line and from one read to the next.
#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
pub struct State {
    /// The file's session, from the listing each read.
    #[serde(skip)]
    pub session: String,
    /// The end of what the agent last said: what the next prompt of yours answers.
    #[serde(default)]
    pub said: String,
    /// You stopped its turn since anything last came in.
    #[serde(default)]
    pub stopped: bool,
    /// The next prompt is a wakeup's, not yours, where the record marks that on a line before it.
    #[serde(default)]
    pub wakeup: bool,
    /// A turn is running, where the record says when one starts and ends.
    #[serde(default)]
    pub open: bool,
    /// How many things have come in.
    #[serde(default)]
    pub n: u64,
    /// The prompts of the turn that's running, hashed: a record that writes one twice.
    #[serde(default)]
    pub seen: Vec<u64>,
    /// The record is a subagent's: what comes in there is its parent agent's doing.
    #[serde(default)]
    pub sub: bool,
}

impl State {
    /// Notes what the agent said: its end is kept.
    pub fn say(&mut self, text: &str) {
        let t = text.trim();
        if !t.is_empty() {
            self.said = crate::jev::tail(t, BEFORE_MOST);
        }
    }

    /// A name for the next thing to come in, for a record whose lines have none of their own.
    pub fn name(&self) -> String {
        format!("{}#{}", self.session, self.n)
    }
}

/// Whose a message typed into a session is, by the line Codealong leads one with: its own
/// (`send::by_codealong`), an agent's (`codealong session send`, with its session's key where the
/// line gives it, else its title), or else yours.
pub fn whose(text: &str) -> (Source, Option<String>) {
    if crate::send::from_codealong(text).is_some() {
        (Source::Codealong, None)
    } else if crate::send::from_an_agent(text).is_some() {
        (Source::Agent, crate::send::sender_key_of(text).or_else(|| crate::send::sender_of(text)).map(String::from))
    } else {
        (Source::You, None)
    }
}

/// What a line of prose adds (`Adapter::prose`: when, whether it's yours, the text), for a
/// harness with no reader of its own: the agent's is noted for the prompt that follows, and
/// yours came in, as whoever its leading line says wrote it.
pub fn from_prose(state: &mut State, at: i64, you: bool, text: &str) -> Option<Came> {
    if !you {
        state.say(text);
        return None;
    }
    let (source, sender) = whose(text);
    let kind = if source == Source::You { "prompt" } else { "message" };
    Some(Came { id: state.name(), at, source, kind, sender, mid: false, text: crate::send::past_sender_lines(text).trim().to_string(), picked: None })
}

/// Something that came in, with what the record said around it.
struct Row {
    came: Came,
    before: String,
    after_stop: bool,
    /// Nothing came in or was said before it in its record.
    first: bool,
}

/// What a file's lines add, a line at a time: the rows, and the state after them.
fn rows_of(lines: impl Iterator<Item = Vec<u8>>, state: &mut State, step: &dyn Fn(&[u8], &mut State) -> Option<Came>) -> Vec<Row> {
    let mut rows = Vec::new();
    for line in lines {
        let first = state.n == 0 && state.said.is_empty();
        let Some(came) = step(&line, state) else { continue };
        if state.sub {
            continue;
        }
        rows.push(Row { before: state.said.clone(), after_stop: std::mem::take(&mut state.stopped), first, came });
        state.n += 1;
    }
    rows
}

/// The sessions a pass gave new turns, by row id, each with the earliest of them: what's worked
/// out from a session's turns is worked out again from there (`settle`).
type Dirty = HashMap<i64, i64>;

fn touch(dirty: &mut Dirty, session: i64, at: i64) {
    let e = dirty.entry(session).or_insert(at);
    *e = (*e).min(at);
}

/// Reads one record from where the last read stopped, up to its last whole line, as the ledger
/// reads calls (`ledger::read_file`). A file that got shorter was written anew and is read from
/// the start; its turns keep their rows.
fn read_file(db: &Connection, account: &AccountCfg, tag: &str, path: &Path, session: &str, step: &dyn Fn(&[u8], &mut State) -> Option<Came>, now: i64, dirty: &mut Dirty) -> Result<u64> {
    let meta = std::fs::metadata(path)?;
    let size = meta.len();
    let mtime = crate::util::mtime_ms(path).unwrap_or(0);
    let key = path.to_string_lossy().to_string();
    let had: Option<(u64, u64, i64, String)> =
        db.prepare_cached("SELECT offset, size, mtime, state FROM turn_files WHERE path = ?1")?.query_row([&key], |r| Ok((r.get::<_, i64>(0)? as u64, r.get::<_, i64>(1)? as u64, r.get(2)?, r.get(3)?))).optional()?;
    if had.as_ref().is_some_and(|f| f.1 == size && f.2 == mtime) {
        return Ok(0);
    }
    let (mut offset, mut state) = match had {
        Some((offset, _, _, state)) if offset <= size => (offset, serde_json::from_str::<State>(&state).unwrap_or_default()),
        _ => (0, State::default()),
    };
    state.session = session.to_string();
    let mut file = File::open(path)?;
    file.seek(SeekFrom::Start(offset))?;
    let mut reader = BufReader::with_capacity(256 * 1024, file);
    let start = offset;
    let lines = std::iter::from_fn(|| {
        let mut line = Vec::new();
        let n = reader.read_until(b'\n', &mut line).ok()?;
        if n == 0 || line.last() != Some(&b'\n') {
            return None;
        }
        offset += n as u64;
        Some(line)
    });
    let rows = rows_of(lines, &mut state, step);
    let tx = db.unchecked_transaction()?;
    put(&tx, account, tag, session, &rows, now, dirty)?;
    tx.execute(
        "INSERT INTO turn_files (path, offset, size, mtime, state) VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(path) DO UPDATE SET offset = excluded.offset, size = excluded.size, mtime = excluded.mtime, state = excluded.state",
        params![key, offset as i64, size as i64, mtime, serde_json::to_string(&state)?],
    )?;
    tx.commit()?;
    Ok(offset - start)
}

/// What each session last said, for a harness that keeps messages in a database: kept between
/// reads, as a file's `State` is.
#[derive(Serialize, Deserialize, Default)]
struct RowsState {
    #[serde(default)]
    said: BTreeMap<String, String>,
}

/// Sessions' last words kept for a database's reads, at most: the least lately heard go first.
const SAID_KEPT: usize = 400;

/// Reads what was said in a harness that keeps messages in a database (`Adapter::said`), from the
/// newest write the last read saw. A message's id is the database's own.
fn read_rows(db: &Connection, account: &AccountCfg, adapter: &dyn crate::harness::Adapter, now: i64, dirty: &mut Dirty) -> Result<()> {
    let tag = adapter.harness().config();
    let key = format!("{tag}:{}#said", account.name);
    let had: Option<(i64, String)> = db.prepare_cached("SELECT offset, state FROM turn_files WHERE path = ?1")?.query_row([&key], |r| Ok((r.get(0)?, r.get(1)?))).optional()?;
    let (after, mut state) = had.map(|(after, state)| (after, serde_json::from_str::<RowsState>(&state).unwrap_or_default())).unwrap_or_default();
    let mut said = adapter.said(account, 0, after);
    if said.is_empty() {
        return Ok(());
    }
    said.sort_by(|a, b| (a.session.as_str(), a.at).cmp(&(b.session.as_str(), b.at)));
    let newest = said.iter().map(|m| m.updated).max().unwrap_or(after).max(after);
    let tx = db.unchecked_transaction()?;
    for m in &said {
        let session = crate::model::Session::key_for(adapter.harness(), &account.name, &m.session);
        if !m.you {
            state.said.insert(m.session.clone(), crate::jev::tail(m.text.trim(), BEFORE_MOST));
            continue;
        }
        let (source, sender) = whose(&m.text);
        let before = state.said.get(&m.session).cloned().unwrap_or_default();
        let came = Came { id: m.id.clone(), at: m.at, source, kind: if source == Source::You { "prompt" } else { "message" }, sender, mid: false, text: crate::send::past_sender_lines(&m.text).trim().to_string(), picked: None };
        let first = before.is_empty() && !tx.prepare_cached("SELECT EXISTS (SELECT 1 FROM turns t JOIN sessions s ON s.id = t.session WHERE s.key = ?1)")?.query_row([&session], |r| r.get::<_, bool>(0))?;
        put(&tx, account, tag, &session, &[Row { came, before, after_stop: false, first }], now, dirty)?;
    }
    while state.said.len() > SAID_KEPT {
        state.said.pop_first();
    }
    tx.execute(
        "INSERT INTO turn_files (path, offset, size, mtime, state) VALUES (?1, ?2, 0, 0, ?3)
         ON CONFLICT(path) DO UPDATE SET offset = excluded.offset, state = excluded.state",
        params![key, newest, serde_json::to_string(&state)?],
    )?;
    tx.commit()?;
    Ok(())
}

/// How a prompt of yours opens, for a list: its first lines made one, cut.
pub fn opening(text: &str) -> String {
    crate::util::one_line(text, OPENING)
}

fn put(db: &Connection, account: &AccountCfg, tag: &str, session: &str, rows: &[Row], now: i64, dirty: &mut Dirty) -> Result<()> {
    if rows.is_empty() {
        return Ok(());
    }
    let sid = crate::ledger::session_row(db, session, &account.name, "")?;
    let mut add = db.prepare_cached(
        "INSERT INTO turns (id, session, at, source, kind, sender, mid, after_stop, picked, chars, opening, text, before, stance, read_by)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
         ON CONFLICT(id) DO NOTHING",
    )?;
    for r in rows {
        let c = &r.came;
        let yours = c.source == Source::You;
        let stance = if yours { crate::stance::by_rule(c, r.first) } else { None };
        // What a read needs is kept only for a prompt that waits on one, and not for an old one.
        let reads = yours && stance.is_none() && c.at >= now - TEXTS_FOR_MS;
        let added = add.execute(params![
            crate::ledger::row_id(tag, &c.id),
            sid,
            c.at,
            c.source.word(),
            c.kind,
            c.sender,
            c.mid as i64,
            (yours && r.after_stop) as i64,
            c.picked.map(Picked::word),
            c.text.chars().count() as i64,
            yours.then(|| opening(&c.text)),
            reads.then(|| crate::jev::clip(&c.text, TEXT_MOST)),
            reads.then(|| r.before.clone()),
            stance,
            stance.map(|_| crate::stance::BY_RULE),
        ])?;
        if added > 0 {
            touch(dirty, sid, c.at);
        }
    }
    Ok(())
}

/// What each of a session's turns counts as, in order: who began it, a turn the session began
/// itself counted with the one it follows (the session's own beginning, for one nothing came
/// before), and the first prompt of a session an agent or Codealong started counted as theirs
/// when it came as the session started.
pub fn counted(turns: &[(Source, i64)], began: Option<(Began, Option<&str>)>, started: Option<i64>) -> Vec<Source> {
    let opens_as = began.map(|(b, by)| b.opens_as(by)).unwrap_or(Source::You);
    let mut last: Option<Source> = None;
    turns
        .iter()
        .enumerate()
        .map(|(i, (source, at))| {
            let as_started = i == 0 && started.is_none_or(|s| *at <= s + OPENS_WITHIN_MS);
            let counts = match source {
                Source::Itself => last.unwrap_or(opens_as),
                Source::You if as_started => opens_as,
                s => *s,
            };
            last = Some(counts);
            counts
        })
        .collect()
}

/// Works out again what each of a session's turns counts as (`counted`), and writes what changed.
pub fn recount(db: &Connection, session: i64) -> Result<()> {
    let (began, by, started): (Option<String>, Option<String>, Option<i64>) = db.prepare_cached("SELECT began, began_by, started FROM sessions WHERE id = ?1")?.query_row([session], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
    let began = began.as_deref().and_then(Began::from_word).map(|b| (b, by.as_deref()));
    let mut stmt = db.prepare_cached("SELECT id, source, at, counts_as FROM turns WHERE session = ?1 ORDER BY at, id")?;
    let rows: Vec<(i64, Source, i64, Option<String>)> = stmt
        .query_map([session], |r| Ok((r.get(0)?, r.get::<_, String>(1)?, r.get(2)?, r.get(3)?)))?
        .filter_map(|r| r.ok().and_then(|(id, source, at, was)| Some((id, Source::from_word(&source)?, at, was))))
        .collect();
    let counts = counted(&rows.iter().map(|(_, source, at, _)| (*source, *at)).collect::<Vec<_>>(), began, started);
    let mut set = db.prepare_cached("UPDATE turns SET counts_as = ?2 WHERE id = ?1")?;
    for ((id, _, _, was), counts) in rows.iter().zip(counts) {
        if was.as_deref() != Some(counts.word()) {
            set.execute(params![id, counts.word()])?;
        }
    }
    Ok(())
}

/// The turn each call was made in: the latest thing that came in to its session at or before it.
const TURN_OF: &str = "(SELECT t.id FROM turns t WHERE t.session = calls.session AND t.at <= calls.at ORDER BY t.at DESC, t.id DESC LIMIT 1)";

/// After a pass: what each new turn's session's turns count as, the turn of each call made from
/// the earliest new turn on, and the turn of each call read since the last pass.
fn settle(db: &Connection, dirty: &Dirty, now: i64) -> Result<()> {
    let tx = db.unchecked_transaction()?;
    for (session, from) in dirty {
        recount(&tx, *session)?;
        if *from < i64::MAX {
            tx.prepare_cached(&format!("UPDATE calls SET turn = {TURN_OF} WHERE session = ?1 AND at >= ?2"))?.execute(params![session, from])?;
        }
    }
    // Calls the ledger read since: every one with no turn yet, the first time, and after that
    // the late ones.
    let since = crate::ledger::meta(&tx, "turned")?.and_then(|t| t.parse::<i64>().ok()).map(|t| t - LATE_MS).unwrap_or(0);
    tx.execute(&format!("UPDATE calls SET turn = {TURN_OF} WHERE turn IS NULL AND at >= ?1"), [since])?;
    crate::ledger::set_meta(&tx, "turned", &now.to_string())?;
    // A prompt that's been read, or grown old unread, keeps only how it opens.
    tx.execute("UPDATE turns SET text = NULL, before = NULL WHERE text IS NOT NULL AND (stance IS NOT NULL OR at < ?1)", [now - TEXTS_FOR_MS])?;
    tx.commit()?;
    Ok(())
}

/// One pass over every account's records, after the ledger's own (`ledger::spawn`): what the
/// engine noted of each session's beginning (began.rs), what came in since the last pass, what
/// follows from both. What couldn't be read is added to the ledger's own account of that.
pub fn pass(db: &Connection, accounts: &[AccountCfg], caught: &mut Caught) {
    pass_since(db, accounts, caught, 0)
}

/// `pass`, leaving out records last written before `since`: for a check of recent days
/// (`--snapshot --analytics`), which needn't read years of them.
pub fn pass_since(db: &Connection, accounts: &[AccountCfg], caught: &mut Caught, since: i64) {
    let now = now_ms();
    let mut dirty = Dirty::new();
    match crate::began::apply(db) {
        Ok(changed) => changed.into_iter().for_each(|id| touch(&mut dirty, id, i64::MAX)),
        Err(e) => caught.failed.push(format!("where sessions came from: {e:#}")),
    }
    if let Err(e) = crate::began::backfill(db, &crate::origins::current()) {
        caught.failed.push(format!("where earlier sessions came from: {e:#}"));
    }
    let mut files = Vec::new();
    for account in accounts {
        let Some(adapter) = crate::harness::of(account) else { continue };
        for (path, session) in adapter.turn_files(account, since) {
            let Ok(meta) = std::fs::metadata(&path) else { continue };
            files.push((meta.created().ok(), account, adapter, path, session));
        }
    }
    // Oldest first, every account's together: a line a later record copies (a fork, a session
    // moved to another account) stays with the session it came from.
    files.sort_by_key(|f| f.0);
    for (_, account, adapter, path, session) in files {
        match read_file(db, account, adapter.harness().config(), &path, &session, &|line, st| adapter.turn_step(line, st), now, &mut dirty) {
            Ok(n) => caught.read += n,
            // A record that's gone since it was listed (a thread archived meanwhile) is read
            // where it went, the next pass.
            Err(_) if !path.exists() => {}
            Err(e) => caught.failed.push(format!("what came in to {}: {e:#}", path.display())),
        }
    }
    for account in accounts {
        let Some(adapter) = crate::harness::of(account) else { continue };
        if let Err(e) = read_rows(db, account, adapter, now, &mut dirty) {
            caught.failed.push(format!("what came in to {}'s sessions: {e:#}", account.name));
        }
    }
    if let Err(e) = settle(db, &dirty, now) {
        caught.failed.push(format!("which turn each call was made in: {e:#}"));
    }
}

/* ---- Reports ---- */

/// How many of something, by a word.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Count {
    pub key: String,
    pub n: u64,
}

fn counts(m: BTreeMap<String, u64>) -> Vec<Count> {
    let mut v: Vec<Count> = m.into_iter().map(|(key, n)| Count { key, n }).collect();
    v.sort_by(|a, b| b.n.cmp(&a.n).then(a.key.cmp(&b.key)));
    v
}

/// One day, local time: what came in by who it was from, and your prompts by what they did.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PartDay {
    pub day: String,
    /// By `Source::word`: everything that came in, as its record says who from.
    pub sources: BTreeMap<String, u64>,
    /// By stance (stance.rs `STANCES`), an empty key for the ones not read: the prompts that count
    /// as yours.
    pub stances: BTreeMap<String, u64>,
}

/// What stands out of what the records say for sure of your prompts.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Marks {
    /// Sent while the agent worked, into its turn.
    pub mid: u64,
    /// Sent after you stopped its turn.
    pub after_stop: u64,
    /// Answers to questions the agent asked: the first choice, another one, words of your own.
    pub picked_first: u64,
    pub picked_offered: u64,
    pub own_words: u64,
    /// Calls you allowed and denied from Codealong, and suggested replies you sent as written.
    pub allowed: u64,
    pub denied: u64,
    pub suggested: u64,
    /// Characters you typed in all.
    pub chars: u64,
}

/// A group's turns: an account's, a folder's or a session's.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PartRow {
    pub key: String,
    /// For a session's row: its title as last seen, its account and its folder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<String>,
    /// How the session began (`Began::word`), for a session's row, when noted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub began: Option<String>,
    /// What came in, by what it counts as (`Source::word`: you, agent, codealong).
    pub counts: BTreeMap<String, u64>,
    /// Your prompts by stance, an empty key for the ones not read.
    pub stances: BTreeMap<String, u64>,
    pub last_at: i64,
}

impl PartRow {
    fn add(&mut self, counts_as: &str, stance: Option<&str>, n: u64, last_at: i64) {
        *self.counts.entry(counts_as.to_string()).or_default() += n;
        if let Some(stance) = stance {
            *self.stances.entry(stance.to_string()).or_default() += n;
        }
        self.last_at = self.last_at.max(last_at);
    }

    pub fn yours(&self) -> u64 {
        self.counts.get("you").copied().unwrap_or(0)
    }
}

/// A prompt of yours, for a list.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Prompt {
    /// Its row, as text: a number past what a page's numbers hold.
    pub id: String,
    pub at: i64,
    pub session: String,
    pub title: String,
    pub opening: String,
    /// What it did, when read; how sure the read was; and what read it (stance.rs).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stance: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sure: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_by: Option<String>,
    pub mid: bool,
    pub after_stop: bool,
}

/// Your part over some days: what came in to your sessions and who from, and what your prompts
/// did with what the agents had said.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Part {
    pub days: Vec<PartDay>,
    /// Everything that came in, by who its record says it was from (`Source::word`).
    pub sources: Vec<Count>,
    /// By what it counts as (`counted`): a turn the session began itself with the one it follows.
    pub hands: Vec<Count>,
    /// Your prompts by stance, an empty key for the ones not read.
    pub stances: Vec<Count>,
    pub marks: Marks,
    pub accounts: Vec<PartRow>,
    pub folders: Vec<PartRow>,
    /// The sessions you prompted most, at most `SESSIONS` of them.
    pub sessions: Vec<PartRow>,
    /// Your latest prompts that steered or redirected, at most `PROMPTS` of them.
    pub prompts: Vec<Prompt>,
    /// The earliest thing known to have come in: before it, nothing is known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since: Option<i64>,
    /// Prompts of yours still waiting on a read, in all, and whether Jev reads them.
    pub unread: u64,
    pub reading: bool,
}

const SESSIONS: usize = 200;
const PROMPTS: usize = 200;

/// What came in over the last `days` days, for one account or all of them.
pub fn part_in(db: &Connection, cfg: &crate::config::Config, days: u32, account: Option<&str>, now: i64) -> Result<Part> {
    let first: String = db.query_row("SELECT date('now', 'localtime', ?1)", [format!("-{} days", days.max(1) - 1)], |r| r.get(0))?;
    let from = now - (days.max(1) as i64 + 1) * DAY_MS;
    let mut day_rows: BTreeMap<String, PartDay> = BTreeMap::new();
    let (mut sources, mut hands, mut stances): (BTreeMap<String, u64>, BTreeMap<String, u64>, BTreeMap<String, u64>) = Default::default();
    let mut marks = Marks::default();
    let (mut accounts, mut folders, mut sessions): (BTreeMap<String, PartRow>, BTreeMap<String, PartRow>, HashMap<String, PartRow>) = Default::default();
    let filter = if account.is_some() { "AND s.account = ?2" } else { "AND ?2 IS NULL" };
    let sql = format!(
        "SELECT date(t.at / 1000, 'unixepoch', 'localtime') AS day, s.key, s.account, s.folder, coalesce(s.title, ''), s.began,
                t.source, coalesce(t.counts_as, t.source) AS hand, coalesce(t.stance, '') AS stance, t.kind,
                count(*), sum(t.mid), sum(t.after_stop), sum(t.chars), max(t.at),
                sum(t.picked = 'first'), sum(t.picked = 'offered'), sum(t.picked = 'own')
         FROM turns t JOIN sessions s ON s.id = t.session
         WHERE t.at >= ?1 {filter}
         GROUP BY day, t.session, t.source, hand, stance, t.kind"
    );
    let mut stmt = db.prepare(&sql)?;
    let mut rows = stmt.query(params![from, account])?;
    while let Some(r) = rows.next()? {
        let day: String = r.get(0)?;
        if day < first {
            continue;
        }
        let (key, acct, folder, title, began): (String, String, String, String, Option<String>) = (r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?);
        let (source, hand, stance, kind): (String, String, String, String) = (r.get(6)?, r.get(7)?, r.get(8)?, r.get(9)?);
        let n = r.get::<_, i64>(10)? as u64;
        let last_at: i64 = r.get(14)?;
        // A prompt that counts as yours: one you typed that isn't the opening of a session
        // somebody else started.
        let yours = source == "you" && hand == "you";
        let d = day_rows.entry(day.clone()).or_insert_with(|| PartDay { day, ..Default::default() });
        *d.sources.entry(source.clone()).or_default() += n;
        *sources.entry(source).or_default() += n;
        *hands.entry(hand.clone()).or_default() += n;
        if yours {
            *d.stances.entry(stance.clone()).or_default() += n;
            *stances.entry(stance.clone()).or_default() += n;
            marks.mid += r.get::<_, i64>(11)? as u64;
            marks.after_stop += r.get::<_, i64>(12)? as u64;
            marks.chars += r.get::<_, i64>(13)? as u64;
            marks.picked_first += r.get::<_, Option<i64>>(15)?.unwrap_or(0) as u64;
            marks.picked_offered += r.get::<_, Option<i64>>(16)?.unwrap_or(0) as u64;
            marks.own_words += r.get::<_, Option<i64>>(17)?.unwrap_or(0) as u64;
        }
        let _ = kind;
        let stance = yours.then_some(stance.as_str());
        accounts.entry(acct.clone()).or_insert_with(|| PartRow { key: acct.clone(), ..Default::default() }).add(&hand, stance, n, last_at);
        folders.entry(folder.clone()).or_insert_with(|| PartRow { key: folder.clone(), ..Default::default() }).add(&hand, stance, n, last_at);
        sessions
            .entry(key.clone())
            .or_insert_with(|| PartRow { key, title: Some(title), account: Some(acct), folder: Some(folder), began, ..Default::default() })
            .add(&hand, stance, n, last_at);
    }
    // The days in order, the ones nothing came in on too.
    let mut days_out = Vec::new();
    let mut day = first.clone();
    let today: String = db.query_row("SELECT date('now', 'localtime')", [], |r| r.get(0))?;
    while day <= today {
        days_out.push(day_rows.remove(&day).unwrap_or_else(|| PartDay { day: day.clone(), ..Default::default() }));
        day = crate::ledger::next_day(&day);
    }
    let by_yours = |m: Vec<PartRow>| {
        let mut v = m;
        v.sort_by(|a, b| b.yours().cmp(&a.yours()).then(b.last_at.cmp(&a.last_at)));
        v
    };
    let mut sessions = by_yours(sessions.into_values().collect());
    sessions.truncate(SESSIONS);

    let (allowed, denied, suggested): (i64, i64, i64) = db.query_row(
        "SELECT coalesce(sum(answer IN ('allow', 'always')), 0), coalesce(sum(answer = 'deny'), 0), coalesce(sum(answer = 'suggested'), 0) FROM answers a
         WHERE a.at >= ?1 AND (?2 IS NULL OR EXISTS (SELECT 1 FROM sessions s WHERE s.key = a.session AND s.account = ?2))",
        params![from, account],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    (marks.allowed, marks.denied, marks.suggested) = (allowed as u64, denied as u64, suggested as u64);

    let mut stmt = db.prepare(
        "SELECT t.id, t.at, s.key, coalesce(s.title, ''), coalesce(t.opening, ''), t.stance, t.stance_p, t.read_by, t.mid, t.after_stop
         FROM turns t JOIN sessions s ON s.id = t.session
         WHERE t.at >= ?1 AND (?2 IS NULL OR s.account = ?2) AND t.source = 'you' AND coalesce(t.counts_as, 'you') = 'you' AND t.stance IN ('steers', 'redirects')
         ORDER BY t.at DESC LIMIT ?3",
    )?;
    let prompts = stmt
        .query_map(params![from, account, PROMPTS as i64], |r| {
            Ok(Prompt {
                id: r.get::<_, i64>(0)?.to_string(),
                at: r.get(1)?,
                session: r.get(2)?,
                title: r.get(3)?,
                opening: r.get(4)?,
                stance: r.get(5)?,
                sure: r.get(6)?,
                read_by: r.get(7)?,
                mid: r.get::<_, i64>(8)? != 0,
                after_stop: r.get::<_, i64>(9)? != 0,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let since: Option<i64> = db.query_row("SELECT min(at) FROM turns WHERE at > 0", [], |r| r.get(0))?;
    let unread: i64 = db.query_row("SELECT count(*) FROM turns WHERE text IS NOT NULL AND stance IS NULL AND source = 'you' AND coalesce(counts_as, 'you') = 'you'", [], |r| r.get(0))?;
    Ok(Part {
        days: days_out,
        sources: counts(sources),
        hands: counts(hands),
        stances: counts(stances),
        marks,
        accounts: by_yours(accounts.into_values().collect()),
        folders: by_yours(folders.into_values().collect()),
        sessions,
        prompts,
        since,
        unread: unread as u64,
        reading: cfg.jev.prompts && crate::jev::has_key(),
    })
}

/// A call you allowed or denied from Codealong, or a suggested reply you sent as written
/// (`answer`: `allow`, `always`, `deny`, `suggested`), for `Marks`. Nothing is noted while
/// analytics are off.
pub fn note_answer(session: &str, answer: &str, what: &str) {
    if !crate::config::current().is_ok_and(|c| c.experimental.analytics) {
        return;
    }
    let done = crate::ledger::with_db(|db| {
        db.execute("INSERT INTO answers (at, session, answer, what) VALUES (?1, ?2, ?3, ?4)", params![now_ms(), session, answer, crate::util::one_line(what, 200)])?;
        Ok(())
    });
    if let Err(e) = done {
        eprintln!("Couldn't note an answer to {session} in the ledger: {e:#}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account() -> AccountCfg {
        AccountCfg { name: "claude".into(), harness: "claude".into(), home: "~/.claude".into(), command: None, start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false }
    }

    fn came(id: &str, at: i64, source: Source, text: &str) -> Came {
        Came { id: id.into(), at, source, kind: if source == Source::You { "prompt" } else { "message" }, sender: None, mid: false, text: text.into(), picked: None }
    }

    fn db() -> Connection {
        crate::ledger::in_memory()
    }

    #[test]
    fn a_message_is_whoever_its_leading_line_says() {
        assert_eq!(whose("Fix the parser"), (Source::You, None));
        assert_eq!(whose(&crate::send::by_codealong("On your PR #7: ken approved it.")), (Source::Codealong, None));
        let agents = "This message is from another agent, “Review the parser” (Codex), sent with `codealong session send`; it isn't from your user. To answer it, run `codealong session send codex:codex:t1 \"<your answer>\"`.\n\nTwo findings.";
        assert_eq!(whose(agents), (Source::Agent, Some("codex:codex:t1".into())));
        // Through a harness's prose: the agent's words are what the next prompt answers.
        let mut st = State { session: "cursor:cursor:c1".into(), ..Default::default() };
        assert_eq!(from_prose(&mut st, 5, false, "Done. Shall I push?"), None);
        let c = from_prose(&mut st, 9, true, agents).unwrap();
        assert_eq!((c.source, c.kind, c.text.as_str(), c.id.as_str()), (Source::Agent, "message", "Two findings.", "cursor:cursor:c1#0"));
        assert_eq!(st.said, "Done. Shall I push?");
    }

    #[test]
    fn a_turn_counts_as_who_began_the_work() {
        use Source::*;
        let at = |list: &[Source]| list.iter().enumerate().map(|(i, s)| (*s, 1000 + i as i64)).collect::<Vec<_>>();
        // Yours, a task reporting back on it, an agent's message, what follows from that.
        assert_eq!(counted(&at(&[You, Itself, Agent, Itself, Itself, You, Codealong, Itself]), None, None), vec![You, You, Agent, Agent, Agent, You, Codealong, Codealong]);
        // A helper opens with its starter's words, typed in as yours would be; what you type
        // into it later is yours.
        assert_eq!(counted(&at(&[You, Itself, You]), Some((Began::Helper, Some("claude:claude:p"))), Some(900)), vec![Agent, Agent, You]);
        assert_eq!(counted(&at(&[You]), Some((Began::Schedule, Some("s-1"))), None), vec![Codealong]);
        // A wakeup with nothing before it is the session's own beginning's.
        assert_eq!(counted(&at(&[Itself, You]), Some((Began::Review, None)), None), vec![Codealong, You]);
        // The first thing in a copy's record long after the session started isn't its opening.
        assert_eq!(counted(&[(You, 5_000_000)], Some((Began::Agent, Some("x"))), Some(1000)), vec![You]);
        // A fork is yours, and so is a headless run no session ran.
        assert_eq!(counted(&at(&[You]), Some((Began::Fork, Some("x"))), None), vec![You]);
        assert_eq!(counted(&at(&[You]), Some((Began::Headless, None)), None), vec![You]);
        assert_eq!(counted(&at(&[You]), Some((Began::Headless, Some("x"))), None), vec![Agent]);
    }

    #[test]
    fn calls_are_given_the_turn_they_were_made_in() {
        let db = db();
        let now = now_ms();
        let a = account();
        let sid = crate::ledger::session_row(&db, "claude:claude:a", "claude", "/work/app").unwrap();
        let call = |id: i64, at: i64| db.execute("INSERT INTO calls (id, at, session, model) VALUES (?1, ?2, ?3, 'm')", params![id, at, sid]).unwrap();
        call(1, now - 9000);
        call(2, now - 5000);
        call(3, now - 1000);
        let rows = vec![
            Row { came: came("u1", now - 8000, Source::You, "Fix the parser"), before: String::new(), after_stop: false, first: true },
            Row { came: came("u2", now - 3000, Source::Agent, "Two findings."), before: "Done.".into(), after_stop: false, first: false },
        ];
        let mut dirty = Dirty::new();
        put(&db, &a, "claude", "claude:claude:a", &rows, now, &mut dirty).unwrap();
        // The same lines again, as a fork's record copies them, add nothing.
        put(&db, &a, "claude", "claude:claude:fork", &rows, now, &mut Dirty::new()).unwrap();
        assert_eq!(db.query_row("SELECT count(*) FROM turns", [], |r| r.get::<_, i64>(0)).unwrap(), 2);
        assert_eq!(dirty.get(&sid), Some(&(now - 8000)));
        settle(&db, &dirty, now).unwrap();
        let turn_of = |call: i64| db.query_row("SELECT t.source FROM calls c LEFT JOIN turns t ON t.id = c.turn WHERE c.id = ?1", [call], |r| r.get::<_, Option<String>>(0)).unwrap();
        // A call from before anything is known to have come in has no turn.
        assert_eq!((turn_of(1), turn_of(2), turn_of(3)), (None, Some("you".into()), Some("agent".into())));
        // The first prompt of a session is read by rule, and keeps only how it opens.
        let (stance, opening, text): (Option<String>, Option<String>, Option<String>) = db.query_row("SELECT stance, opening, text FROM turns WHERE source = 'you'", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap();
        assert_eq!((stance.as_deref(), opening.as_deref(), text), (Some("sets-out"), Some("Fix the parser"), None));
        // A call read late is given its turn on the next pass.
        call(4, now - 2000);
        settle(&db, &Dirty::new(), now).unwrap();
        assert_eq!(turn_of(4), Some("agent".into()));
        // Found to be a helper, its opening counts as its starter's.
        db.execute("UPDATE sessions SET began = 'helper', began_by = 'claude:claude:p' WHERE id = ?1", [sid]).unwrap();
        recount(&db, sid).unwrap();
        let counts: Vec<String> = db.prepare("SELECT counts_as FROM turns ORDER BY at").unwrap().query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect();
        assert_eq!(counts, ["agent", "agent"]);
        let part = part_in(&db, &crate::config::Config::default(), 7, None, now).unwrap();
        assert_eq!(part.hands, vec![Count { key: "agent".into(), n: 2 }]);
        assert_eq!(part.sources.len(), 2);
        assert!(part.stances.is_empty(), "a helper's opening isn't a prompt of yours: {:?}", part.stances);
        assert_eq!(part.sessions[0].began.as_deref(), Some("helper"));
    }

    #[test]
    fn a_prompt_waiting_on_a_read_keeps_its_text_until_read_or_old() {
        let db = db();
        let now = now_ms();
        let a = account();
        let rows = vec![
            Row { came: came("u1", now - 5000, Source::You, "Fix the parser"), before: String::new(), after_stop: false, first: true },
            Row { came: came("u2", now - 4000, Source::You, "No, keep the old tokenizer and wrap it instead."), before: "I'll rewrite the tokenizer.".into(), after_stop: true, first: false },
            Row { came: came("u3", now - 40 * DAY_MS, Source::You, "An old one"), before: "Earlier.".into(), after_stop: false, first: false },
        ];
        let mut dirty = Dirty::new();
        put(&db, &a, "claude", "claude:claude:a", &rows, now, &mut dirty).unwrap();
        settle(&db, &dirty, now).unwrap();
        let kept = |id: &str| db.query_row("SELECT text, before, after_stop FROM turns WHERE id = ?1", [crate::ledger::row_id("claude", id)], |r| Ok((r.get::<_, Option<String>>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, i64>(2)?))).unwrap();
        assert_eq!(kept("u2"), (Some("No, keep the old tokenizer and wrap it instead.".into()), Some("I'll rewrite the tokenizer.".into()), 1));
        assert_eq!(kept("u3").0, None, "too old to be read");
        let part = part_in(&db, &crate::config::Config::default(), 7, None, now).unwrap();
        assert_eq!((part.unread, part.marks.after_stop), (1, 1));
        assert_eq!(part.stances, vec![Count { key: String::new(), n: 1 }, Count { key: "sets-out".into(), n: 1 }]);
        // Read, it keeps only how it opens, and is listed with the prompts that redirected.
        crate::stance::set(&db, crate::ledger::row_id("claude", "u2"), "redirects", Some(0.9), "jev-1.14.0").unwrap();
        settle(&db, &Dirty::new(), now).unwrap();
        assert_eq!(kept("u2").0, None);
        let part = part_in(&db, &crate::config::Config::default(), 7, None, now).unwrap();
        assert_eq!(part.prompts.len(), 1);
        assert_eq!((part.prompts[0].opening.as_str(), part.prompts[0].stance.as_deref(), part.prompts[0].after_stop), ("No, keep the old tokenizer and wrap it instead.", Some("redirects"), true));
    }
}
