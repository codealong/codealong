//! Side conversations: something asked about a session without telling the session. Codealong
//! runs a throwaway copy of the session's conversation as far as it's written (Claude Code: a
//! headless fork that keeps no record; Codex: an ephemeral fork of the thread), asks the copy,
//! and shows what it says beside the session. The session never hears of it and carries on. The
//! copy may do what the session itself may without asking (`Adapter::side`), and is told to look
//! and answer, changing something only when a question asks for that change in so many words: as
//! Codex's own `/side` has it. The harness keeps no record of the copy. Closing it gives the
//! session's box back at once, but a copy that's answering answers what it was asked first and
//! only then ends (`close`, `Side::closed`), so an answer asked for is never lost to a close;
//! quitting Codealong is the end of it.
//!
//! What was asked and answered is Codealong's to keep (`sides.json`), so a side conversation
//! can be read after it's closed (`past`, a closed one still answering as it answers), with what
//! the copy did to answer. A new question after that goes to a new copy, which knows none of it.
//!
//! A PR's questions are a side conversation too, by the PR's key (`ask_pr`): nobody's copy, but a
//! conversation of its own on an account you pick (`Adapter::fresh_side`), in a checkout of the
//! PR's repo here, handed the PR's description, discussion and whole diff to read, since it has
//! no `gh` it may run. It only looks, and is kept, closed and read as a session's is.
//!
//! Each harness says how its copy runs and how to talk to it (`Adapter::side`: a `Recipe`, the
//! process and a `Dialect` for its lines). This module runs the processes, keeps what was asked
//! and answered, and tells the windows as an answer is written. A session has one side
//! conversation open at a time, and any number closed and still answering; what's asked while the
//! copy answers waits its turn.
//!
//! Every side conversation is behind one lock, which the windows take to ask about theirs, so
//! nothing is written to a copy under it: a copy's lines are put in order for a thread of its own
//! to write (`Proc::out`), however long the copy takes to read them.
//!
//! The board lists each session's side conversations, kept and open, without what was said
//! (`glances`, `Board::sides`): its row counts them and lists the one answering under it, as it
//! does its helpers, and its header names each. The board is made again whenever that list would
//! change (`init`'s `changed`), not as an answer is written.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::Config;
use crate::model::{Board, Lane, Origin, PrItem};
use crate::rpc;
use crate::store;
use crate::util::{now_ms, truncate};

/// What a copy is told before its first question, so it answers instead of carrying on with
/// the work it remembers. Codex's `/side` says much the same to its copy.
pub const RULES: &str = "This is a side conversation. Everything before this is a copy of a conversation that carries on elsewhere without you: it's here for reference only. Don't continue, finish or repeat any task, plan or tool call from it. Answer what's asked from here on, briefly. You may read files and run commands that only look, to answer well. Don't change files, git state, settings or anything else unless what's asked here asks for that change in so many words; then make only that change, keep it small, and keep out of the way of the work the main conversation is doing. Don't start subagents. None of this reaches the main conversation.";

/// How long a copy that isn't answering is kept running for a follow-up.
const IDLE: Duration = Duration::from_secs(15 * 60);

/// How many side conversations are kept to read later, the latest ones.
const KEPT: usize = 300;

/// How many of a turn's tool calls are kept, the first ones.
const TOOLS: usize = 200;

/// One question and its answer.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Turn {
    pub question: String,
    /// The answer so far.
    #[serde(default)]
    pub answer: String,
    /// What the copy did to answer: its tool calls, a line each, in order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<String>,
    /// The tool it's running now, as a line.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    #[serde(default)]
    pub done: bool,
    /// Why it ended without an answer, or with half of one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// When it was asked.
    #[serde(default)]
    pub at: i64,
}

/// A session's side conversation, as the windows show it. With no turns there is none.
#[derive(Serialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Side {
    /// Which of the session's side conversations: its record's id (`Kept`).
    pub id: String,
    /// The session it's about.
    pub key: String,
    pub turns: Vec<Turn>,
    /// The copy is answering.
    pub running: bool,
    /// The copy is gone (it sat idle, or stopped): what was said can still be read, and the
    /// next question goes to a new copy, which knows none of it.
    pub ended: bool,
    /// The account whose agent answers, where that's chosen: a PR's conversation's (`ask_pr`).
    /// A session's copy answers on the session's own.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
    /// The model and effort it answers on, where they were chosen with the account; none for the
    /// account's own.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    /// It was closed while its copy answered, and the copy answers what it was asked before it
    /// ends: no longer the session's open one, it's read where the kept ones are (`past`).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub closed: bool,
}

/// A side conversation as Codealong keeps it, to read once its copy is gone: the turns that
/// ended, with or without an answer.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Kept {
    pub id: String,
    /// The session it was about.
    pub key: String,
    /// The session's title when it was asked.
    #[serde(default)]
    pub title: String,
    pub started_at: i64,
    /// When it was last asked or answered.
    pub last_at: i64,
    pub turns: Vec<Turn>,
    /// It was closed while its copy answered, and the copy is answering still: read as it is
    /// (`past`), never kept so.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub answering: bool,
}

/// A side conversation as the board lists it (`glances`): what it was first asked, how far it
/// went and whether its copy is answering, without what was said. The windows list it under its
/// session (src/boardLists.ts `sideRows`) and in its header, and read the rest as they show it.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Glance {
    /// Its record's id (`Side::id`, `Kept::id`).
    pub id: String,
    /// The first line of what it was first asked.
    pub question: String,
    /// How many questions it's been asked.
    pub asked: usize,
    /// Its copy is answering.
    pub running: bool,
    /// It's the session's open one, which shows above its box until it's closed: its copy is
    /// there to ask, or ended and hasn't been closed.
    pub open: bool,
    pub started_at: i64,
    /// When it was last asked, or what was kept of it was written.
    pub last_at: i64,
}

/// What a line from a copy's process means.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// More of the answer.
    Said(String),
    /// A tool it started, as a line.
    Tool(String),
    /// The answer is finished, or it ended with this in the way.
    Done(Option<String>),
}

/// How to talk to one harness's copy: JSON lines to its stdin, JSON lines from its stdout.
/// It only turns lines into lines, so it's tested without a process.
pub trait Dialect: Send {
    /// What to write as the process starts.
    fn open(&mut self) -> Vec<Value>;
    /// What to write to ask it `text`. The first question may wait in here until the copy is
    /// made (`hear` then writes it).
    fn ask(&mut self, text: &str) -> Vec<Value>;
    /// What a line it wrote means, and what to write back.
    fn hear(&mut self, line: &Value) -> (Vec<Event>, Vec<Value>);
}

/// The process that runs a copy (`Adapter::side`).
pub struct Recipe {
    /// Found on the PATH Codealong gives the programs it runs, unless it's a path.
    pub program: PathBuf,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    /// What it mustn't inherit from the app.
    pub unset: Vec<&'static str>,
    pub dialect: Box<dyn Dialect>,
}

struct Proc {
    /// Which of the session's copies this is: a line from an earlier one is dropped.
    id: u64,
    pid: u32,
    /// Its process, which ends with this.
    client: rpc::Client,
    dialect: Box<dyn Dialect>,
    /// The lines to write to it, a batch at a time, in the order they're made: its writer
    /// (`writer`) writes them, and ends once this goes.
    out: Sender<Vec<Value>>,
}

struct Open {
    side: Side,
    /// The session's title, for the record.
    title: String,
    started_at: i64,
    /// What was asked while it was answering, in order: the turns after the one it's on.
    queue: VecDeque<String>,
    proc: Option<Proc>,
    used: Instant,
}

#[derive(Default)]
struct State {
    open: HashMap<String, Open>,
    /// The ones closed while their copies answered, until the copies have answered.
    closing: Vec<Open>,
    next: u64,
}

impl State {
    /// The side conversation copy `id` of `key`'s answers: its open one, or one closed while it
    /// answered.
    fn of_copy(&mut self, key: &str, id: u64) -> Option<&mut Open> {
        let answers = |o: &Open| o.proc.as_ref().is_some_and(|p| p.id == id);
        match self.open.get_mut(key) {
            Some(o) if answers(o) => Some(o),
            _ => self.closing.iter_mut().find(|o| answers(o)),
        }
    }

    /// Takes out the one closed while it answered whose record is `id`.
    fn take_closing(&mut self, id: &str) -> Option<Open> {
        let at = self.closing.iter().position(|o| o.side.id == id)?;
        Some(self.closing.remove(at))
    }
}

fn state() -> std::sync::MutexGuard<'static, State> {
    static STATE: OnceLock<Mutex<State>> = OnceLock::new();
    STATE.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner())
}

type Notify = Box<dyn Fn(Side) + Send + Sync>;
static NOTIFY: OnceLock<Notify> = OnceLock::new();
static CHANGED: OnceLock<Box<dyn Fn() + Send + Sync>> = OnceLock::new();

/// Whether what's asked and answered is kept (`sides.json`): by the app, not by a check from
/// the command line or a test.
static KEEPS: AtomicBool = AtomicBool::new(false);

/// Tells `notify` whenever a side conversation changes, and `changed` whenever what `glances`
/// lists may have (one asked, answered, ended, closed or forgotten), keeps them to read later,
/// and ends copies left idle.
pub fn init(notify: impl Fn(Side) + Send + Sync + 'static, changed: impl Fn() + Send + Sync + 'static) {
    if NOTIFY.set(Box::new(notify)).is_err() {
        return;
    }
    let _ = CHANGED.set(Box::new(changed));
    KEEPS.store(true, Ordering::Relaxed);
    std::thread::spawn(|| loop {
        std::thread::sleep(Duration::from_secs(60));
        end_idle(IDLE);
    });
}

fn notify(side: Side) {
    if let Some(f) = NOTIFY.get() {
        f(side);
    }
}

/// What `glances` lists may have changed.
fn changed() {
    if let Some(f) = CHANGED.get() {
        f();
    }
}

/// Every side conversation, kept, open or closed and still answering, as the board lists them
/// (`Glance`): by the session's or PR's key, the latest started first.
pub fn glances() -> BTreeMap<String, Vec<Glance>> {
    let live: Vec<(String, Glance)> = {
        let st = state();
        st.open.values().chain(&st.closing).filter(|o| !o.side.turns.is_empty()).map(|o| (o.side.key.clone(), glance_of_open(o))).collect()
    };
    glances_of(&kept_glances(), live)
}

/// The kept ones and the open ones by key, an open one in place of what's kept of it, the latest
/// started first.
fn glances_of(kept: &[(String, Glance)], open: Vec<(String, Glance)>) -> BTreeMap<String, Vec<Glance>> {
    let mut by: BTreeMap<String, Vec<Glance>> = BTreeMap::new();
    let still: Vec<(String, Glance)> = kept.iter().filter(|(_, k)| !open.iter().any(|(_, o)| o.id == k.id)).cloned().collect();
    for (key, g) in still.into_iter().chain(open) {
        by.entry(key).or_default().push(g);
    }
    for list in by.values_mut() {
        list.sort_by(|a, b| b.started_at.cmp(&a.started_at).then_with(|| b.id.cmp(&a.id)));
    }
    by
}

fn glance_of_open(o: &Open) -> Glance {
    let s = &o.side;
    let last_at = s.turns.iter().map(|t| t.at).max().unwrap_or(o.started_at);
    Glance { id: s.id.clone(), question: first_line(s.turns.first().map_or("", |t| t.question.as_str())), asked: s.turns.len(), running: s.running, open: !s.closed, started_at: o.started_at, last_at }
}

fn glance_of_kept(k: &Kept) -> Glance {
    Glance { id: k.id.clone(), question: first_line(k.turns.first().map_or("", |t| t.question.as_str())), asked: k.turns.len(), running: false, open: false, started_at: k.started_at, last_at: k.last_at }
}

fn first_line(text: &str) -> String {
    truncate(text.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or(""), 200)
}

/// What's kept, as `glances` lists it: sides.json read once, and again after each change made
/// here (`keep`, `forget`). Read under the lock, so a read that began before a change can't put
/// back what the change replaced.
fn kept_glances() -> Vec<(String, Glance)> {
    let mut cache = kept_cache();
    cache.get_or_insert_with(|| load(&store_path()).iter().map(|k| (k.key.clone(), glance_of_kept(k))).collect()).clone()
}

fn kept_cache() -> std::sync::MutexGuard<'static, Option<Vec<(String, Glance)>>> {
    static CACHE: Mutex<Option<Vec<(String, Glance)>>> = Mutex::new(None);
    CACHE.lock().unwrap_or_else(|e| e.into_inner())
}

/// The session's side conversation, if it has one.
pub fn get(key: &str) -> Option<Side> {
    state().open.get(key).map(|o| o.side.clone()).filter(|s| !s.turns.is_empty())
}

/// Whether `pid` is a copy's process, a closed one's still answering too. Claude Code lists a
/// copy among an account's live sessions while it runs, and Codealong's own permission hook hears
/// from it: neither is a session.
pub fn runs(pid: u32) -> bool {
    let st = state();
    st.open.values().chain(&st.closing).any(|o| o.proc.as_ref().is_some_and(|p| p.pid == pid))
}

/// Asks the session's copy `text`, making the copy when there's none, and returns the side
/// conversation with the question in it. The answer comes through `init`'s `notify`.
pub fn ask(board: &Board, cfg: &Config, key: &str, text: &str) -> Result<Side> {
    let text = text.trim();
    if text.is_empty() {
        bail!("Ask it something: /side, then your question.");
    }
    ask_with(key, text, None, || recipe(board, cfg, key))
}

/// The model and effort a conversation of its own runs on, as the agent menu picks them with its
/// account (`Adapter::fresh_side`); none for the account's own.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RunsOn<'a> {
    pub model: Option<&'a str>,
    pub effort: Option<&'a str>,
}

/// Asks about a PR in a conversation of its own (`key` is the PR's, `owner/name#number`): an
/// agent on `account`, on the model and effort in `on`, that reads the PR and the code around it
/// and changes nothing (`Adapter::fresh_side`), made when there's none, as a session's copy is.
/// From there it's a side conversation like a session's, asked, closed, kept and read the same
/// way by the PR's key, and nobody's copy: the PR and GitHub hear nothing of it. A question asked
/// while one is open goes to it, on whatever it was made on.
pub fn ask_pr(board: &Board, cfg: &Config, key: &str, account: &str, on: RunsOn, text: &str) -> Result<Side> {
    let text = text.trim();
    if text.is_empty() {
        bail!("Ask it something about the PR.");
    }
    let on = RunsOn { model: on.model.map(str::trim).filter(|m| !m.is_empty()), effort: on.effort.map(str::trim).filter(|e| !e.is_empty()) };
    if let Some(m) = on.model.filter(|m| !crate::start::valid_model(m)) {
        bail!("{m:?} isn't a model name Codealong can start an agent on.");
    }
    if let Some(e) = on.effort.filter(|e| !crate::start::effort_levels().contains(e)) {
        bail!("{e:?} isn't an effort level Codealong knows.");
    }
    ask_with(key, text, Some((account, on)), || pr_recipe(board, cfg, key, account, on))
}

/// Asks `key`'s side conversation `text`, making its process with `made` when there's none, on
/// the account, model and effort the asker chose, where they chose them.
fn ask_with(key: &str, text: &str, chosen: Option<(&str, RunsOn)>, made: impl FnOnce() -> Result<(Recipe, PathBuf, String)>) -> Result<Side> {
    // Pasted files and screenshots go as their paths, which the copy reads, as they do to a session
    // (send.rs); the question shows as it was typed.
    let sent = crate::attachments::expand(text, crate::attachments::Style::Path);
    let has_copy = state().open.get(key).is_some_and(|o| o.proc.is_some());
    // Made outside the lock: it reads the session's record, or asks GitHub about the PR.
    let made = if has_copy { None } else { Some(made()?) };
    let now = now_ms();
    let side = {
        let mut st = state();
        st.next += 1;
        let id = st.next;
        let copy = if st.open.get(key).is_some_and(|o| o.proc.is_some()) {
            None
        } else {
            let Some((recipe, cwd, title)) = made else { bail!("Its copy just ended. Ask again.") };
            Some((spawn(key, id, recipe, &cwd)?, title))
        };
        let o = st.open.entry(key.to_string()).or_insert_with(|| Open { side: Side { key: key.to_string(), ..Default::default() }, title: String::new(), started_at: now, queue: VecDeque::new(), proc: None, used: Instant::now() });
        if let Some((copy, title)) = copy {
            // A new copy knows nothing of what the last one was asked: it's a new side
            // conversation, and the last one is among the kept ones.
            let (account, on) = chosen.map_or((None, RunsOn::default()), |(a, on)| (Some(a), on));
            o.side = Side { id: format!("{now}-{id}"), key: key.to_string(), account: account.map(String::from), model: on.model.map(String::from), effort: on.effort.map(String::from), ..Default::default() };
            o.title = title;
            o.started_at = now;
            o.queue.clear();
            o.proc = Some(copy);
        }
        o.used = Instant::now();
        o.side.turns.push(Turn { question: text.to_string(), at: now, ..Default::default() });
        if o.side.running {
            o.queue.push_back(sent);
        } else {
            o.side.running = true;
            if let Some(p) = o.proc.as_mut() {
                let lines = p.dialect.ask(&sent);
                write(p, &lines);
            }
        }
        o.side.clone()
    };
    notify(side.clone());
    changed();
    Ok(side)
}

/// Closes the session's side conversation: it's no longer the open one, and what was said is
/// kept to read. A copy that's answering answers what it was asked first and then ends (`heard`),
/// read meanwhile where the kept ones are: you close one to have the box back for the session,
/// not to lose the answer. One idle ends now.
pub fn close(key: &str) {
    close_as(key, true);
}

/// Closes it and ends its copy now, whatever it hadn't answered: for a check from the command
/// line, which leaves no copy running behind it.
pub fn close_now(key: &str) {
    close_as(key, false);
}

fn close_as(key: &str, finish: bool) {
    let (ended, closing) = {
        let mut st = state();
        let Some(mut o) = st.open.remove(key) else { return };
        if finish && o.side.running && o.proc.is_some() {
            o.side.closed = true;
            let side = o.side.clone();
            st.closing.push(o);
            (None, Some(side))
        } else {
            (Some(o), None)
        }
    };
    if let Some(mut o) = ended {
        cut_short(&mut o.side);
        keep(kept_of(&o));
        if let Some(p) = o.proc {
            end(p);
        }
    }
    // Gone from above its box, and still answering where it's read, if it is.
    notify(Side { key: key.to_string(), ..Default::default() });
    if let Some(side) = closing {
        notify(side);
    }
    changed();
}

/// Ends the turns a closing leaves without their answer, saying so.
fn cut_short(side: &mut Side) {
    for t in side.turns.iter_mut().filter(|t| !t.done) {
        t.done = true;
        t.tool = None;
        t.answer = t.answer.trim().to_string();
        t.error = Some(if t.answer.is_empty() { "Closed before it answered." } else { "Closed before it finished." }.to_string());
    }
}

/// The side conversations kept for a session, the latest first: the one that's open too, as far
/// as it's been answered, and the ones closed and still answering as they are, unanswered
/// questions and all.
pub fn past(key: &str) -> Vec<Kept> {
    let closing: Vec<Kept> = state().closing.iter().filter(|o| o.side.key == key).map(as_it_is).collect();
    with_closing(past_in(&store_path(), key), closing)
}

/// What's kept for a session with the ones still answering in place of what's kept of them, the
/// latest first.
fn with_closing(kept: Vec<Kept>, closing: Vec<Kept>) -> Vec<Kept> {
    let mut all: Vec<Kept> = kept.into_iter().filter(|k| !closing.iter().any(|c| c.id == k.id)).collect();
    all.extend(closing);
    all.sort_by_key(|k| std::cmp::Reverse(k.last_at));
    all
}

/// Forgets a kept side conversation, and stops it if it was closed and is still answering.
pub fn forget(id: &str) -> Result<()> {
    stop_closing(id);
    let forgot = forget_at(&store_path(), id);
    *kept_cache() = None;
    changed();
    forgot
}

/// Stops the one closed and still answering whose record is `id`, if there is one: forgotten,
/// what it would say isn't wanted either.
fn stop_closing(id: &str) {
    let stopped = state().take_closing(id);
    if let Some(mut o) = stopped {
        if let Some(p) = o.proc.take() {
            end(p);
        }
        o.side.running = false;
        notify(o.side);
    }
}

// ---- keeping ----

fn store_path() -> PathBuf {
    crate::config::support_dir().join("sides.json")
}

/// What the file holds, for the error when it doesn't read as that.
const WHAT: &str = "a list of side conversations";

/// What's kept, to show: a file that doesn't parse shows as nothing kept. Only a change to it
/// is turned away (`keep_in`, `forget_in`).
fn load(path: &Path) -> Vec<Kept> {
    store::read_or_default(path)
}

/// A side conversation as it's read while it answers: every turn, as far as it's gone.
fn as_it_is(o: &Open) -> Kept {
    let last_at = o.side.turns.iter().map(|t| t.at).max().unwrap_or(o.started_at);
    Kept { id: o.side.id.clone(), key: o.side.key.clone(), title: o.title.clone(), started_at: o.started_at, last_at, turns: o.side.turns.clone(), answering: true }
}

/// A side conversation as it's kept: the turns that have ended. None before the first has.
fn kept_of(o: &Open) -> Option<Kept> {
    let turns: Vec<Turn> = o.side.turns.iter().filter(|t| t.done).map(|t| Turn { tool: None, ..t.clone() }).collect();
    (!turns.is_empty()).then(|| Kept { id: o.side.id.clone(), key: o.side.key.clone(), title: o.title.clone(), started_at: o.started_at, last_at: now_ms(), turns, answering: false })
}

fn keep(kept: Option<Kept>) {
    if let Some(k) = kept.filter(|_| KEEPS.load(Ordering::Relaxed)) {
        // Nobody waits on this as a turn ends, so a failure is a notice.
        let path = store_path();
        let _ = store::noted(&path, keep_at(&path, k));
        *kept_cache() = None;
    }
}

/// The side conversations forgotten since Codealong started, by id, under the lock that keeping
/// and forgetting take in turn: a turn that ends as its conversation is forgotten is kept before
/// the forgetting, or not at all, never after it. One closed while it answered is kept as each
/// turn ends, from its copy's thread, whenever Delete comes.
fn forgotten() -> std::sync::MutexGuard<'static, std::collections::BTreeSet<String>> {
    static FORGOTTEN: Mutex<std::collections::BTreeSet<String>> = Mutex::new(std::collections::BTreeSet::new());
    FORGOTTEN.lock().unwrap_or_else(|e| e.into_inner())
}

/// Keeps it at `path`, unless it's been forgotten.
fn keep_at(path: &Path, kept: Kept) -> Result<()> {
    let forgotten = forgotten();
    if forgotten.contains(&kept.id) {
        return Ok(());
    }
    keep_in(path, kept)
}

/// Forgets it at `path`, and keeps it forgotten.
fn forget_at(path: &Path, id: &str) -> Result<()> {
    let mut forgotten = forgotten();
    forgotten.insert(id.to_string());
    forget_in(path, id)
}

/// Writes a side conversation in place of what was kept of it, the latest last, and lets the
/// oldest go past `KEPT`. A file that doesn't parse is an error and is left as it is: every
/// conversation kept so far is in it.
fn keep_in(path: &Path, kept: Kept) -> Result<()> {
    store::change(path, WHAT, |all: &mut Vec<Kept>| {
        all.retain(|k| k.id != kept.id);
        all.push(kept);
        let extra = all.len().saturating_sub(KEPT);
        all.drain(..extra);
        true
    })?;
    Ok(())
}

fn past_in(path: &Path, key: &str) -> Vec<Kept> {
    let mut of: Vec<Kept> = load(path).into_iter().filter(|k| k.key == key).collect();
    of.sort_by_key(|k| std::cmp::Reverse(k.last_at));
    of
}

fn forget_in(path: &Path, id: &str) -> Result<()> {
    store::change(path, WHAT, |all: &mut Vec<Kept>| {
        let before = all.len();
        all.retain(|k| k.id != id);
        all.len() != before
    })?;
    Ok(())
}

// ---- the copies ----

/// How the session's copy runs, and where (in its own folder, or its worktree's when that
/// folder is gone), and the session's title.
fn recipe(board: &Board, cfg: &Config, key: &str) -> Result<(Recipe, PathBuf, String)> {
    let (s, account) = crate::harness::find(board, cfg, key)?;
    account.ensure_on()?;
    let lane = board.lanes.iter().find(|l| l.sessions.iter().any(|x| x.key == key && x.visitor.is_none())).or_else(|| board.lanes.iter().find(|l| l.sessions.iter().any(|x| x.key == key)));
    let cwd = match lane {
        _ if Path::new(&s.cwd).is_dir() => PathBuf::from(&s.cwd),
        Some(l) if Path::new(&l.path).is_dir() => PathBuf::from(&l.path),
        _ => bail!("Its folder is gone, so a copy of it has nowhere to run."),
    };
    let recipe = crate::harness::adapter(s.harness).side(&crate::harness::Ctx { account }, s, &cwd)?;
    Ok((recipe, cwd, s.title.clone()))
}

// ---- a PR's conversation ----

/// How a PR's conversation runs: on `account`, on the model and effort in `on`, in a checkout of the PR's repo here
/// (`pr_folder`), told what the PR is (`pr_rules`) and handed its description, discussion and
/// whole diff to read (github.rs `for_reading`), fetched afresh for each new conversation, since
/// the PR may have moved on. Its title, for the record, is the PR's.
fn pr_recipe(board: &Board, cfg: &Config, key: &str, account: &str, on: RunsOn) -> Result<(Recipe, PathBuf, String)> {
    let Some(pr) = board.prs.iter().find(|p| p.key == key) else { bail!("Codealong doesn't list {key} any more.") };
    let Some(a) = cfg.accounts.iter().find(|a| a.name == account) else { bail!("There's no account named {account} in config.toml.") };
    a.ensure_on()?;
    let Some(adapter) = crate::harness::of(a) else { bail!("Codealong doesn't know {account}'s harness, {}.", a.harness) };
    let (md, diff, _) = crate::github::for_reading(pr)?;
    let reads = reading_dir(pr);
    std::fs::create_dir_all(&reads).and_then(|_| std::fs::write(reads.join("pr.md"), md)).and_then(|_| std::fs::write(reads.join("pr.diff"), diff)).with_context(|| format!("Couldn't write #{}'s diff for it to read.", pr.number))?;
    tidy_readings(&reads, Duration::from_secs(7 * 24 * 3600));
    let (folder, holds) = pr_folder(board, pr);
    let rules = pr_rules(pr, folder.as_deref(), holds, &reads);
    let cwd = folder.unwrap_or_else(|| reads.clone());
    let recipe = adapter.fresh_side(&crate::harness::Ctx { account: a }, &cwd, &rules, &reads, on)?;
    Ok((recipe, cwd, format!("#{} {}", pr.number, pr.title)))
}

/// Where what a PR's conversation reads is written: a folder of its own under Codealong's, by
/// the PR's repo and number (`+` is in no owner's or repo's name).
fn reading_dir(pr: &PrItem) -> PathBuf {
    crate::config::support_dir().join("pr-reading").join(format!("{}+{}", pr.repo_slug.replace('/', "+"), pr.number))
}

/// Lets the PRs' readings that nothing has written to for `age` go, but `keep`: each is a PR's
/// whole diff, which a new conversation fetches again anyway. Best done: one left is only space.
fn tidy_readings(keep: &Path, age: Duration) {
    let Some(all) = keep.parent().and_then(|p| std::fs::read_dir(p).ok()) else { return };
    for e in all.flatten() {
        let old = e.metadata().and_then(|m| m.modified()).is_ok_and(|t| t.elapsed().is_ok_and(|d| d > age));
        if old && e.path() != keep {
            let _ = std::fs::remove_dir_all(e.path());
        }
    }
}

/// Where a PR's conversation runs, and whether that folder has the PR's branch checked out: a
/// worktree on this Mac that holds it, else the repo's main checkout, else any worktree of the
/// repo, as a review started from the PR's card picks one (src/review.ts `reviewLane`). None
/// where this Mac has no checkout of the repo.
fn pr_folder(board: &Board, pr: &PrItem) -> (Option<PathBuf>, bool) {
    match pr_lane(board, pr) {
        Some((l, holds)) => (Some(PathBuf::from(&l.path)), holds),
        None => (None, false),
    }
}

/// The worktree `pr_folder` names, and whether it holds the PR's branch: where an automatic
/// review of the PR starts too (autoreview.rs).
pub fn pr_lane<'a>(board: &'a Board, pr: &PrItem) -> Option<(&'a Lane, bool)> {
    let here: Vec<&Lane> = board.lanes.iter().filter(|l| l.device.is_none() && Path::new(&l.path).is_dir()).collect();
    if let Some(l) = here.iter().find(|l| pr.lanes.contains(&l.id) && l.branch.as_deref() == Some(pr.head.as_str())) {
        return Some((l, true));
    }
    let of_repo: Vec<&Lane> = here.into_iter().filter(|l| [&l.repo_slug, &l.repo_slug_now].iter().any(|s| s.as_deref() == Some(pr.repo_slug.as_str()))).collect();
    of_repo.iter().find(|l| l.origin == Origin::Main).or(of_repo.first()).map(|l| (*l, false))
}

/// What a PR's conversation is told before its first question: what the PR is, where to read
/// it, what the folder it's in holds, and that it only looks. It's for someone reviewing the PR,
/// so it answers briefly and says where in the code.
fn pr_rules(pr: &PrItem, folder: Option<&Path>, holds: bool, reads: &Path) -> String {
    let reads = reads.display();
    let base = pr.base.as_deref().unwrap_or("its base branch");
    let what = format!("You're answering questions about pull request #{} in {}, “{}” ({}), which merges {} into {base}, from someone reviewing it.", pr.number, pr.repo_slug, pr.title, pr.url, pr.head);
    let read = format!("Its description, reviews, comments and files are in {reads}/pr.md, and its whole diff is in {reads}/pr.diff: read what you need of them before answering.");
    let here = match folder {
        Some(_) if holds => "The folder you're in has its branch checked out, so the code here is the PR's.".to_string(),
        Some(_) => format!("The folder you're in is a checkout of {} without its branch: the code here is what the PR changes, and its changes are only in the diff.", pr.repo_slug),
        None => format!("There's no checkout of {} on this Mac, so the diff is all of its code you have.", pr.repo_slug),
    };
    format!("{what} {read} {here} Read files and run commands that only look, to answer well. Don't change files, git state or anything else, don't push, and don't comment on GitHub: if you're asked to change something, say what you'd change instead. Don't start subagents. Answer briefly, and say where in the code (path and line) when it helps.")
}

fn spawn(key: &str, id: u64, recipe: Recipe, cwd: &Path) -> Result<Proc> {
    let failed = || format!("Couldn't start {} for a side conversation.", recipe.program.display());
    let mut cmd = crate::procs::command(&recipe.program, crate::procs::user_path(), Some(cwd)).with_context(failed)?;
    cmd.args(&recipe.args);
    for (k, v) in &recipe.env {
        cmd.env(k, v);
    }
    for k in &recipe.unset {
        cmd.env_remove(k);
    }
    // Every line it writes is the dialect's to make sense of, the answers to what the dialect
    // asked too: nothing is asked through the client, which only carries the lines.
    let of = key.to_string();
    let client = rpc::Client::spawn(cmd, rpc::Stderr::Tail, rpc::Wire::Bare, move |_, line| match line {
        rpc::Heard::Line(v) => heard(&of, id, v),
        // What it last complained of, for when it ends without answering.
        rpc::Heard::Closed(said) => stopped(&of, id, said.last().map(String::as_str).unwrap_or("")),
    })
    .with_context(failed)?;
    let out = writer(client.link());
    let mut proc = Proc { id, pid: client.pid(), client, dialect: recipe.dialect, out };
    let lines = proc.dialect.open();
    write(&mut proc, &lines);
    Ok(proc)
}

/// The thread that writes a copy's lines, as `write` hands them over, until the copy's `Proc`
/// goes. A line it can't take (in the 10 seconds a write may wait) leaves the rest of its batch
/// unwritten: its process is going, or has stopped reading, and the reader's end says so.
fn writer(link: std::sync::Arc<rpc::Link>) -> Sender<Vec<Value>> {
    let (out, lines) = channel::<Vec<Value>>();
    std::thread::spawn(move || {
        for batch in lines {
            for l in &batch {
                if link.send(l).is_err() {
                    break;
                }
            }
        }
    });
    out
}

/// Hands lines to the copy's writer, in order after those before them. Done under the lock, it
/// waits on nothing.
fn write(p: &mut Proc, lines: &[Value]) {
    if !lines.is_empty() {
        // A writer that's gone went with its process.
        let _ = p.out.send(lines.to_vec());
    }
}

/// A line from copy `id` of the session's conversation.
fn heard(key: &str, id: u64, line: &Value) {
    let (side, kept, answered) = {
        let mut st = state();
        let Some(o) = st.of_copy(key, id) else { return };
        let Some(p) = o.proc.as_mut() else { return };
        let (events, replies) = p.dialect.hear(line);
        write(p, &replies);
        if events.is_empty() {
            return;
        }
        // Heard from: a closed one at work on a long answer isn't stuck (`end_idle`).
        o.used = Instant::now();
        // Kept as each turn ends, so quitting loses only an answer half written.
        let ended = events.iter().any(|e| matches!(e, Event::Done(_)));
        for e in events {
            apply(o, e);
        }
        let (side, kept) = (o.side.clone(), if ended { kept_of(o) } else { None });
        // One closed while it answered has answered all it was asked: its copy's done.
        let answered = if side.closed && !side.running { st.take_closing(&side.id) } else { None };
        (side, kept, answered)
    };
    if let Some(p) = answered.and_then(|o| o.proc) {
        end(p);
    }
    let ended = kept.is_some();
    keep(kept);
    notify(side);
    if ended {
        changed();
    }
}

/// The turn the copy is on: the first without its answer.
fn current(side: &mut Side) -> Option<&mut Turn> {
    side.turns.iter_mut().find(|t| !t.done)
}

fn apply(o: &mut Open, e: Event) {
    match e {
        Event::Said(more) => {
            if let Some(t) = current(&mut o.side) {
                // An answer begins at its first word, whatever came before a tool call.
                if !(t.answer.is_empty() && more.trim().is_empty()) {
                    t.answer.push_str(&more);
                }
                t.tool = None;
            }
        }
        Event::Tool(line) => {
            if let Some(t) = current(&mut o.side) {
                let line = truncate(line.lines().next().unwrap_or("").trim(), 120);
                if t.tools.len() < TOOLS {
                    t.tools.push(line.clone());
                }
                t.tool = Some(line);
            }
        }
        Event::Done(error) => {
            if let Some(t) = current(&mut o.side) {
                t.done = true;
                t.tool = None;
                t.answer = t.answer.trim().to_string();
                t.error = error;
            }
            o.used = Instant::now();
            match (o.queue.pop_front(), o.proc.as_mut()) {
                (Some(next), Some(p)) => {
                    let lines = p.dialect.ask(&next);
                    write(p, &lines);
                }
                _ => o.side.running = false,
            }
        }
    }
}

/// Copy `id`'s process ended: what it hadn't answered never will be.
fn stopped(key: &str, id: u64, complaint: &str) {
    let (side, kept) = {
        let mut st = state();
        let Some(o) = st.of_copy(key, id) else { return };
        // Dropped, which reaps its process.
        o.proc = None;
        let why = if complaint.is_empty() { "The copy stopped before it answered.".to_string() } else { format!("The copy stopped: {}", truncate(complaint, 300)) };
        for t in o.side.turns.iter_mut().filter(|t| !t.done) {
            t.done = true;
            t.tool = None;
            t.error = Some(why.clone());
        }
        o.queue.clear();
        o.side.running = false;
        o.side.ended = true;
        let (side, kept) = (o.side.clone(), kept_of(o));
        if side.closed {
            st.take_closing(&side.id);
        }
        (side, kept)
    };
    keep(kept);
    notify(side);
    changed();
}

/// Ends the copies that have sat idle for `idle`: their processes hold a conversation's worth
/// of memory each. What was said stays to read. A closed one that nothing has come from for as
/// long is stuck, with nobody watching it, and ends too.
fn end_idle(idle: Duration) {
    let mut ended = Vec::new();
    let stuck: Vec<Open> = {
        let mut st = state();
        for o in st.open.values_mut() {
            if o.side.running || o.used.elapsed() < idle {
                continue;
            }
            if let Some(p) = o.proc.take() {
                end(p);
                o.side.ended = true;
                ended.push(o.side.clone());
            }
        }
        let (stuck, still) = std::mem::take(&mut st.closing).into_iter().partition(|o| o.used.elapsed() >= idle);
        st.closing = still;
        stuck
    };
    let any = !ended.is_empty() || !stuck.is_empty();
    for mut o in stuck {
        for t in o.side.turns.iter_mut().filter(|t| !t.done) {
            t.done = true;
            t.tool = None;
            t.answer = t.answer.trim().to_string();
            t.error = Some(format!("Stopped: once closed, nothing came from it for {} minutes.", idle.as_secs() / 60));
        }
        o.side.running = false;
        o.side.ended = true;
        keep(kept_of(&o));
        if let Some(p) = o.proc.take() {
            end(p);
        }
        ended.push(o.side);
    }
    for side in ended {
        notify(side);
    }
    if any {
        changed();
    }
}

/// Ends a copy's process: its input closes, which both harnesses' processes leave on, and it's
/// killed if it's still there a moment later (mid-answer, say).
fn end(p: Proc) {
    p.client.end(Duration::from_millis(1500));
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A copy that answers with what it's asked, a word a line: `cat` gives its lines back.
    struct Echo;

    impl Dialect for Echo {
        fn open(&mut self) -> Vec<Value> {
            Vec::new()
        }
        fn ask(&mut self, text: &str) -> Vec<Value> {
            vec![json!({ "tool": "looking" }), json!({ "say": format!("You asked: {text}") }), json!({ "done": true })]
        }
        fn hear(&mut self, line: &Value) -> (Vec<Event>, Vec<Value>) {
            let text = |k: &str| line.get(k).and_then(Value::as_str).map(String::from);
            let events = match (text("say"), text("tool")) {
                (Some(s), _) => vec![Event::Said(s)],
                (_, Some(t)) => vec![Event::Tool(t)],
                _ => vec![Event::Done(None)],
            };
            (events, Vec::new())
        }
    }

    fn open_with(key: &str, program: &str, args: &[&str]) -> u64 {
        let recipe = Recipe { program: program.into(), args: args.iter().map(|a| a.to_string()).collect(), env: Vec::new(), unset: Vec::new(), dialect: Box::new(Echo) };
        let mut st = state();
        st.next += 1;
        let id = st.next;
        let proc = spawn(key, id, recipe, Path::new("/")).unwrap();
        st.open.insert(key.to_string(), Open { side: Side { id: format!("t-{id}"), key: key.to_string(), ..Default::default() }, title: "A session".into(), started_at: 1, queue: VecDeque::new(), proc: Some(proc), used: Instant::now() });
        id
    }

    /// Asks as `ask` does once the copy is there.
    fn put(key: &str, text: &str) {
        let mut st = state();
        let o = st.open.get_mut(key).unwrap();
        o.side.turns.push(Turn { question: text.into(), ..Default::default() });
        if o.side.running {
            o.queue.push_back(text.into());
        } else {
            o.side.running = true;
            let p = o.proc.as_mut().unwrap();
            let lines = p.dialect.ask(text);
            write(p, &lines);
        }
    }

    fn wait_for(key: &str, f: impl Fn(&Side) -> bool) -> Side {
        for _ in 0..200 {
            if let Some(s) = state().open.get(key).map(|o| o.side.clone()).filter(|s| f(s)) {
                return s;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        panic!("the side conversation never got there: {:?}", state().open.get(key).map(|o| o.side.clone()));
    }

    #[test]
    fn answers_in_the_order_asked() {
        let key = "test:side:order";
        open_with(key, "/bin/cat", &[]);
        put(key, "one");
        put(key, "two");
        let side = wait_for(key, |s| !s.running);
        assert_eq!(side.turns.iter().map(|t| t.answer.as_str()).collect::<Vec<_>>(), ["You asked: one", "You asked: two"]);
        assert!(side.turns.iter().all(|t| t.done && t.tool.is_none() && t.error.is_none()));
        assert!(get(key).is_some());
        let pid = state().open.get(key).unwrap().proc.as_ref().unwrap().pid;
        assert!(runs(pid));
        close(key);
        assert!(get(key).is_none() && !runs(pid));
    }

    /// A copy that doesn't read what it's asked holds up no other side conversation, nor the
    /// windows asking about them: what's written to it is written outside the lock they share.
    #[test]
    fn a_stuck_copy_holds_up_no_other() {
        let (stuck, other) = ("test:side:stuck", "test:side:other");
        open_with(stuck, "/bin/sleep", &["600"]);
        open_with(other, "/bin/cat", &[]);
        let started = Instant::now();
        // More than the 64 KB its input holds.
        put(stuck, &"x".repeat(1 << 20));
        put(other, "one");
        let side = wait_for(other, |s| !s.running);
        assert!(started.elapsed() < Duration::from_secs(3), "{:?}", started.elapsed());
        assert_eq!(side.turns[0].answer, "You asked: one");
        assert!(get(stuck).is_some_and(|s| s.running));
        close_now(stuck);
        close(other);
        assert!(get(stuck).is_none() && !state().closing.iter().any(|o| o.side.key == stuck));
    }

    #[test]
    fn a_copy_that_stops_says_why() {
        let key = "test:side:stops";
        open_with(key, "/bin/sh", &["-c", "echo 'No conversation found' >&2; sleep 0.2"]);
        put(key, "anyone there?");
        let side = wait_for(key, |s| s.ended);
        assert!(!side.running);
        assert_eq!(side.turns[0].error.as_deref(), Some("The copy stopped: No conversation found"));
        assert!(state().open.get(key).unwrap().proc.is_none());
        close(key);
    }

    #[test]
    fn an_idle_copy_ends_and_keeps_what_was_said() {
        let key = "test:side:idle";
        open_with(key, "/bin/cat", &[]);
        put(key, "one");
        wait_for(key, |s| !s.running);
        // Only this test's copy: the others' are answering, or closed.
        std::thread::sleep(Duration::from_millis(30));
        {
            let mut st = state();
            let o = st.open.get_mut(key).unwrap();
            o.used = Instant::now() - Duration::from_secs(3600);
        }
        end_idle(Duration::from_secs(60));
        let side = get(key).unwrap();
        assert!(side.ended);
        assert_eq!(side.turns[0].answer, "You asked: one");
        close(key);
    }

    /// What `notify` told the windows, from the first test that asks: every test's, so each
    /// looks for its own key.
    fn told() -> &'static Mutex<Vec<Side>> {
        static TOLD: Mutex<Vec<Side>> = Mutex::new(Vec::new());
        let _ = NOTIFY.set(Box::new(|side| TOLD.lock().unwrap_or_else(|e| e.into_inner()).push(side)));
        &TOLD
    }

    /// The last the windows were told of `key`'s side conversation closed while it answered, once
    /// it has stopped answering.
    fn told_closed(key: &str) -> Side {
        for _ in 0..200 {
            let last = told().lock().unwrap_or_else(|e| e.into_inner()).iter().rev().find(|s| s.key == key && s.closed).cloned();
            if let Some(side) = last.filter(|s| !s.running) {
                return side;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        panic!("the closed side conversation never stopped answering");
    }

    fn closing_of(key: &str) -> Option<Glance> {
        state().closing.iter().find(|o| o.side.key == key).map(glance_of_open)
    }

    #[test]
    fn a_copy_closed_while_it_answers_answers_what_it_was_asked_and_then_ends() {
        let key = "test:side:closing";
        told();
        // An answer that takes a moment: each line comes back a fifth of a second after it went.
        open_with(key, "/bin/sh", &["-c", "while read -r l; do sleep 0.2; printf '%s\\n' \"$l\"; done"]);
        put(key, "one");
        put(key, "two");
        let pid = state().open.get(key).unwrap().proc.as_ref().unwrap().pid;
        close(key);
        // The session has no side conversation open, and the copy goes on, listed as answering
        // and read where the kept ones are, both questions in it.
        assert!(get(key).is_none() && runs(pid));
        let g = closing_of(key).unwrap();
        assert_eq!((g.asked, g.running, g.open), (2, true, false));
        let read = state().closing.iter().find(|o| o.side.key == key).map(as_it_is).unwrap();
        assert_eq!(read.turns.iter().map(|t| t.question.as_str()).collect::<Vec<_>>(), ["one", "two"]);
        // Read as answering, so a window that opens now says so.
        assert!(read.answering);
        let side = told_closed(key);
        assert_eq!(side.turns.iter().map(|t| (t.answer.as_str(), t.done, t.error.as_deref())).collect::<Vec<_>>(), [("You asked: one", true, None), ("You asked: two", true, None)]);
        // Then it's gone: its copy ended, and what it said is kept.
        assert!(closing_of(key).is_none() && !runs(pid) && get(key).is_none());
    }

    #[test]
    fn a_closed_copy_that_answers_nothing_is_stopped() {
        let key = "test:side:closed-stuck";
        told();
        open_with(key, "/bin/sleep", &["600"]);
        put(key, "anyone there?");
        let pid = state().open.get(key).unwrap().proc.as_ref().unwrap().pid;
        close(key);
        assert!(runs(pid));
        // Answering, it isn't idle, however long ago it was asked.
        end_idle(Duration::from_secs(60));
        assert!(closing_of(key).is_some());
        if let Some(o) = state().closing.iter_mut().find(|o| o.side.key == key) {
            o.used = Instant::now() - Duration::from_secs(3600);
        }
        end_idle(Duration::from_secs(60));
        let side = told_closed(key);
        assert!(side.turns[0].done && side.turns[0].error.as_deref().is_some_and(|e| e.starts_with("Stopped: once closed, nothing came from it for")), "{:?}", side.turns[0]);
        assert!(closing_of(key).is_none() && !runs(pid));
    }

    #[test]
    fn forgetting_a_closed_copy_still_answering_stops_it() {
        let key = "test:side:closed-forgotten";
        told();
        open_with(key, "/bin/sleep", &["600"]);
        put(key, "anyone there?");
        let pid = state().open.get(key).unwrap().proc.as_ref().unwrap().pid;
        close(key);
        let id = state().closing.iter().find(|o| o.side.key == key).unwrap().side.id.clone();
        stop_closing(&id);
        assert!(closing_of(key).is_none() && !runs(pid));
        assert!(!told_closed(key).running);
    }

    /// Delete on one closed and still answering, as a turn of its ends: the turn's keeping can't
    /// bring it back.
    #[test]
    fn a_turn_kept_after_its_conversation_is_forgotten_stays_forgotten() {
        let dir = store::test_dir("sides-forgotten");
        let path = dir.join("sides.json");
        let kept = |id: &str, answer: &str| Kept { id: id.into(), key: "claude:c:1".into(), title: "A session".into(), started_at: 1, last_at: 2, turns: vec![Turn { question: "why?".into(), answer: answer.into(), done: true, at: 1, ..Default::default() }], answering: false };
        let id = format!("forgotten-{}", std::process::id());
        keep_at(&path, kept(&id, "Because.")).unwrap();
        keep_at(&path, kept("other", "No.")).unwrap();
        forget_at(&path, &id).unwrap();
        keep_at(&path, kept(&id, "Because, and more.")).unwrap();
        assert_eq!(load(&path).iter().map(|k| k.id.as_str()).collect::<Vec<_>>(), ["other"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn whats_read_of_a_session_has_the_ones_still_answering_in_place_of_whats_kept_of_them() {
        let kept = |id: &str, last_at: i64, done: bool| Kept { id: id.into(), key: "claude:c:1".into(), title: "A session".into(), started_at: 1, last_at, turns: vec![Turn { question: "why?".into(), done, at: last_at, ..Default::default() }], answering: false };
        let all = with_closing(vec![kept("b", 20, true), kept("a", 10, true)], vec![Kept { answering: true, ..kept("a", 30, false) }]);
        assert_eq!(all.iter().map(|k| (k.id.as_str(), k.last_at, k.turns[0].done, k.answering)).collect::<Vec<_>>(), [("a", 30, false, true), ("b", 20, true, false)]);
        // What's kept never says it's answering.
        assert!(!serde_json::to_value(&all[1]).unwrap().as_object().unwrap().contains_key("answering"));
    }

    #[test]
    fn an_answer_starts_at_its_first_word() {
        let mut o = Open { side: Side { id: "s1".into(), key: "claude:c:1".into(), turns: vec![Turn { question: "q".into(), at: 5, ..Default::default() }, Turn { question: "next".into(), at: 6, ..Default::default() }], running: true, ..Default::default() }, title: "A session".into(), started_at: 5, queue: VecDeque::new(), proc: None, used: Instant::now() };
        assert_eq!(kept_of(&o), None);
        for e in [Event::Said("\n\n".into()), Event::Tool("Read · `a.rs`\nmore".into()), Event::Said("It ".into()), Event::Tool("Bash · `git status`".into())] {
            apply(&mut o, e);
        }
        assert_eq!(o.side.turns[0].tool.as_deref(), Some("Bash · `git status`"));
        for e in [Event::Said("does.\n".into()), Event::Done(None)] {
            apply(&mut o, e);
        }
        let answered = Turn { question: "q".into(), answer: "It does.".into(), tools: vec!["Read · `a.rs`".into(), "Bash · `git status`".into()], tool: None, done: true, error: None, at: 5 };
        assert_eq!(o.side.turns[0], answered);
        assert!(!o.side.running);
        // What's kept is what has ended: the question still waiting isn't in it.
        let kept = kept_of(&o).unwrap();
        assert_eq!((kept.id.as_str(), kept.key.as_str(), kept.title.as_str(), kept.started_at), ("s1", "claude:c:1", "A session", 5));
        assert_eq!(kept.turns, [answered]);
    }

    #[test]
    fn side_conversations_are_kept_to_read_later() {
        let dir = std::env::temp_dir().join(format!("codealong-sides-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("sides.json");
        let kept = |id: &str, key: &str, last_at: i64, answer: &str| Kept { id: id.into(), key: key.into(), title: "A session".into(), started_at: 1, last_at, turns: vec![Turn { question: "why?".into(), answer: answer.into(), done: true, at: 1, ..Default::default() }], answering: false };
        assert!(past_in(&path, "claude:c:1").is_empty());
        keep_in(&path, kept("a", "claude:c:1", 10, "Because.")).unwrap();
        keep_in(&path, kept("b", "codex:x:2", 20, "No.")).unwrap();
        keep_in(&path, kept("c", "claude:c:1", 30, "Yes.")).unwrap();
        // A follow-up is written in place of what was kept of its conversation.
        keep_in(&path, kept("a", "claude:c:1", 40, "Because, and more.")).unwrap();
        let of = past_in(&path, "claude:c:1");
        assert_eq!(of.iter().map(|k| (k.id.as_str(), k.turns[0].answer.as_str())).collect::<Vec<_>>(), [("a", "Because, and more."), ("c", "Yes.")]);
        assert_eq!(load(&path).len(), 3);
        forget_in(&path, "a").unwrap();
        assert_eq!(past_in(&path, "claude:c:1").iter().map(|k| k.id.as_str()).collect::<Vec<_>>(), ["c"]);
        // Only the latest are kept.
        for i in 0..KEPT + 5 {
            keep_in(&path, kept(&format!("n{i}"), "claude:c:9", 100 + i as i64, "ok")).unwrap();
        }
        let all = load(&path);
        assert_eq!(all.len(), KEPT);
        assert_eq!((all[0].id.as_str(), all[KEPT - 1].id.clone()), ("n5", format!("n{}", KEPT + 4)));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_board_lists_each_sessions_side_conversations_the_open_one_in_place_of_what_was_kept() {
        let turn = |q: &str, at: i64, done: bool| Turn { question: q.into(), done, at, ..Default::default() };
        let kept = |id: &str, key: &str, started_at: i64| Kept { id: id.into(), key: key.into(), title: "A session".into(), started_at, last_at: started_at + 1, turns: vec![turn("\n  Why is it slow?\nIt was fast.", started_at, true)], answering: false };
        let all: Vec<(String, Glance)> = [kept("a", "claude:c:1", 10), kept("b", "claude:c:1", 50), kept("c", "codex:x:2", 30)].iter().map(|k| (k.key.clone(), glance_of_kept(k))).collect();
        assert_eq!(all[0].1, Glance { id: "a".into(), question: "Why is it slow?".into(), asked: 1, running: false, open: false, started_at: 10, last_at: 11 });
        // `a` asked again: its copy answers the second question.
        let o = Open { side: Side { id: "a".into(), key: "claude:c:1".into(), turns: vec![turn("Why is it slow?", 10, true), turn("And now?", 70, false)], running: true, ..Default::default() }, title: "A session".into(), started_at: 10, queue: VecDeque::new(), proc: None, used: Instant::now() };
        let by = glances_of(&all, vec![(o.side.key.clone(), glance_of_open(&o))]);
        let of = |key: &str| by[key].iter().map(|g| (g.id.as_str(), g.asked, g.running, g.open, g.last_at)).collect::<Vec<_>>();
        assert_eq!(of("claude:c:1"), [("b", 1, false, false, 51), ("a", 2, true, true, 70)]);
        assert_eq!(of("codex:x:2"), [("c", 1, false, false, 31)]);
        assert_eq!(by.len(), 2);
    }

    /// sides.json holds every conversation kept: one that doesn't read (another build's shape, a
    /// write cut short) isn't started over from the turn that just ended.
    #[test]
    fn a_kept_file_that_does_not_read_is_never_written_over() {
        let dir = store::test_dir("sides-unreadable");
        let path = dir.join("sides.json");
        let kept = |id: &str| Kept { id: id.into(), key: "claude:c:1".into(), title: "A session".into(), started_at: 1, last_at: 2, turns: vec![Turn { question: "why?".into(), answer: "Because.".into(), done: true, at: 1, ..Default::default() }], answering: false };
        keep_in(&path, kept("a")).unwrap();
        for bytes in [r#"{"version":2,"kept":[]}"#, r#"[{"id":"a","key":"claude:c:1","#] {
            std::fs::write(&path, bytes).unwrap();
            let e = keep_in(&path, kept("b")).unwrap_err().to_string();
            assert!(e.starts_with("sides.json doesn't read as a list of side conversations"), "{e}");
            assert!(forget_in(&path, "a").is_err());
            assert_eq!(std::fs::read_to_string(&path).unwrap(), bytes);
            // Nothing to show meanwhile.
            assert!(past_in(&path, "claude:c:1").is_empty());
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// PR #7 of `o/r`, on the branch `feat`, held by the worktree `/held`.
    fn pr() -> PrItem {
        use crate::model::{Checks, Merge, PrState};
        PrItem { key: "o/r#7".into(), repo_slug: "o/r".into(), repo: "r".into(), project: "P".into(), number: 7, title: "Add teams".into(), url: "https://github.com/o/r/pull/7".into(), state: PrState::Open, author: Some("ana".into()), mine: false, review_requested: true, requested_by: None, requested_at: None, reviewers_asked: Vec::new(), snoozed_until: None, nudge: false, nudge_by_default: false, primary: None, primary_by: None, head: "feat".into(), base: Some("main".into()), checks: Checks::default(), merge: Merge::Unknown, additions: 1, deletions: 0, changed_files: 1, created_at: 0, updated_at: 0, merged_at: None, closed_at: None, lanes: vec!["held".into()], sessions: Vec::new(), head_oid: None, authored_by: None, labels: Vec::new(), auto_review: None, last_review: None }
    }

    #[test]
    fn a_prs_conversation_runs_where_its_branch_is_else_in_a_checkout_of_its_repo() {
        use crate::fixtures::{lane, scratch};
        let dir = scratch("pr-folder");
        let at = |name: &str| {
            let p = dir.join(name);
            std::fs::create_dir_all(&p).unwrap();
            p.to_string_lossy().into_owned()
        };
        let of = |id: &str, path: String, slug: &str, branch: &str, origin: Origin| Lane { path, repo_slug: Some(slug.into()), branch: Some(branch.into()), origin, ..lane(id, Vec::new()) };
        let (main, other, held) = (of("main", at("main"), "o/r", "main", Origin::Main), of("other", at("other"), "o/r", "x", Origin::Script), of("held", at("held"), "o/r", "feat", Origin::Script));
        let elsewhere = of("else", at("else"), "o/else", "main", Origin::Main);
        let board = |lanes: Vec<Lane>| Board { lanes, ..Default::default() };
        let pr = pr();
        assert_eq!(pr_folder(&board(vec![main.clone(), held.clone()]), &pr), (Some(PathBuf::from(&held.path)), true));
        // A worktree that held it and has moved on to another branch doesn't hold it now.
        let moved = Lane { branch: Some("next".into()), ..held.clone() };
        assert_eq!(pr_folder(&board(vec![other.clone(), moved, main.clone()]), &pr), (Some(PathBuf::from(&main.path)), false));
        // Without the main checkout, any of the repo's; one under the repo's old name too.
        let renamed = Lane { repo_slug: Some("o/old".into()), repo_slug_now: Some("o/r".into()), ..other.clone() };
        assert_eq!(pr_folder(&board(vec![elsewhere.clone(), renamed]), &pr), (Some(PathBuf::from(&other.path)), false));
        // Another device's worktree, or a folder that's gone, is no place to run.
        let theirs = Lane { device: Some(crate::model::DeviceTag { name: "Mini".into(), short: "M".into(), color: "blue".into(), online: true, last_seen: None }), ..held.clone() };
        let gone = Lane { path: dir.join("gone").to_string_lossy().into_owned(), ..main.clone() };
        assert_eq!(pr_folder(&board(vec![theirs, gone, elsewhere]), &pr), (None, false));
    }

    #[test]
    fn a_prs_conversation_is_told_what_it_is_where_to_read_it_and_to_only_look() {
        let pr = pr();
        let reads = Path::new("/support/pr-reading/o+r+7");
        let rules = pr_rules(&pr, Some(Path::new("/held")), true, reads);
        assert!(rules.starts_with("You're answering questions about pull request #7 in o/r, “Add teams” (https://github.com/o/r/pull/7), which merges feat into main, from someone reviewing it."), "{rules}");
        assert!(rules.contains("/support/pr-reading/o+r+7/pr.md") && rules.contains("/support/pr-reading/o+r+7/pr.diff"));
        assert!(rules.contains("has its branch checked out") && rules.contains("don't comment on GitHub"));
        assert!(pr_rules(&pr, Some(Path::new("/main")), false, reads).contains("a checkout of o/r without its branch"));
        assert!(pr_rules(&pr, None, false, reads).contains("no checkout of o/r on this Mac"));
        assert_eq!(reading_dir(&pr).file_name().unwrap(), "o+r+7");
    }

    #[test]
    fn closing_at_once_keeps_what_was_asked_and_says_what_it_cut_short() {
        let turn = |question: &str, answer: &str, done: bool| Turn { question: question.into(), answer: answer.into(), done, tool: (!done).then(|| "Read · `a.rs`".to_string()), ..Default::default() };
        let mut o = Open { side: Side { id: "s1".into(), key: "claude:c:1".into(), turns: vec![turn("one", "Done.", true), turn("two", "Half of ", false), turn("three", "", false)], running: true, ..Default::default() }, title: "A session".into(), started_at: 5, queue: VecDeque::new(), proc: None, used: Instant::now() };
        cut_short(&mut o.side);
        let kept = kept_of(&o).unwrap();
        assert_eq!(kept.turns.iter().map(|t| (t.answer.as_str(), t.error.as_deref())).collect::<Vec<_>>(), [("Done.", None), ("Half of", Some("Closed before it finished.")), ("", Some("Closed before it answered."))]);
        assert!(kept.turns.iter().all(|t| t.done && t.tool.is_none()));
    }
}
