//! Side conversations: something asked about a session without telling the session. Codealong
//! runs a throwaway copy of the session's conversation as far as it's written (Claude Code: a
//! headless fork that keeps no record; Codex: an ephemeral fork of the thread), asks the copy,
//! and shows what it says beside the session. The copy can look around (read files, run
//! commands that change nothing) but not change anything; the session never hears of it and
//! carries on; and nothing of it is kept: closing it, or quitting Codealong, is the end of it.
//!
//! Each harness says how its copy runs and how to talk to it (`Adapter::side`: a `Recipe`, the
//! process and a `Dialect` for its lines). This module runs the processes, keeps what was asked
//! and answered, and tells the windows as an answer is written. A session has one side
//! conversation at a time; what's asked while the copy answers waits its turn.

use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use serde::Serialize;
use serde_json::Value;

use crate::config::Config;
use crate::model::Board;
use crate::util::truncate;

/// What a copy is told before its first question, so it answers instead of carrying on with
/// the work it remembers. Codex's `/side` says much the same to its copy.
pub const RULES: &str = "This is a side conversation. Everything before this is a copy of a conversation that carries on elsewhere without you: it's here for reference only. Don't continue, finish or repeat any task, plan or tool call from it. Answer what's asked from here on, briefly. You may read files and run commands that only look, to answer well. Don't change files, git state, settings or anything else, and don't start subagents. Nothing said here is kept, and none of it reaches the main conversation.";

/// How long a copy that isn't answering is kept running for a follow-up.
const IDLE: Duration = Duration::from_secs(15 * 60);

/// One question and its answer.
#[derive(Serialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Turn {
    pub question: String,
    /// The answer so far.
    pub answer: String,
    /// The tool the copy is running, as a line.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    pub done: bool,
    /// Why it ended without an answer, or with half of one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// A session's side conversation, as the windows show it. With no turns there is none.
#[derive(Serialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Side {
    /// The session it's about.
    pub key: String,
    pub turns: Vec<Turn>,
    /// The copy is answering.
    pub running: bool,
    /// The copy is gone (it sat idle, or stopped): what was said can still be read, and the
    /// next question goes to a new copy, which knows none of it.
    pub ended: bool,
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
    stdin: ChildStdin,
    child: Child,
    dialect: Box<dyn Dialect>,
}

struct Open {
    side: Side,
    /// What was asked while it was answering, in order: the turns after the one it's on.
    queue: VecDeque<String>,
    proc: Option<Proc>,
    used: Instant,
}

#[derive(Default)]
struct State {
    open: HashMap<String, Open>,
    next: u64,
}

fn state() -> std::sync::MutexGuard<'static, State> {
    static STATE: OnceLock<Mutex<State>> = OnceLock::new();
    STATE.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner())
}

type Notify = Box<dyn Fn(Side) + Send + Sync>;
static NOTIFY: OnceLock<Notify> = OnceLock::new();

/// Tells `notify` whenever a side conversation changes, and ends copies left idle.
pub fn init(notify: impl Fn(Side) + Send + Sync + 'static) {
    if NOTIFY.set(Box::new(notify)).is_err() {
        return;
    }
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

/// The session's side conversation, if it has one.
pub fn get(key: &str) -> Option<Side> {
    state().open.get(key).map(|o| o.side.clone()).filter(|s| !s.turns.is_empty())
}

/// Whether `pid` is a copy's process. Claude Code lists a copy among an account's live sessions
/// while it runs, and Codealong's own permission hook hears from it: neither is a session.
pub fn runs(pid: u32) -> bool {
    state().open.values().any(|o| o.proc.as_ref().is_some_and(|p| p.pid == pid))
}

/// Asks the session's copy `text`, making the copy when there's none, and returns the side
/// conversation with the question in it. The answer comes through `init`'s `notify`.
pub fn ask(board: &Board, cfg: &Config, key: &str, text: &str) -> Result<Side> {
    let text = text.trim();
    if text.is_empty() {
        bail!("Ask it something: /side, then your question.");
    }
    let has_copy = state().open.get(key).is_some_and(|o| o.proc.is_some());
    // Looked up outside the lock: it reads the session's record.
    let made = if has_copy { None } else { Some(recipe(board, cfg, key)?) };
    let side = {
        let mut st = state();
        st.next += 1;
        let id = st.next;
        let copy = if st.open.get(key).is_some_and(|o| o.proc.is_some()) {
            None
        } else {
            let Some((recipe, cwd)) = made else { bail!("Its copy just ended. Ask again.") };
            Some(spawn(key, id, recipe, &cwd)?)
        };
        let o = st.open.entry(key.to_string()).or_insert_with(|| Open { side: Side { key: key.to_string(), ..Default::default() }, queue: VecDeque::new(), proc: None, used: Instant::now() });
        if let Some(copy) = copy {
            // A new copy knows nothing of what the last one was asked.
            o.side.turns.clear();
            o.side.ended = false;
            o.queue.clear();
            o.proc = Some(copy);
        }
        o.used = Instant::now();
        o.side.turns.push(Turn { question: text.to_string(), ..Default::default() });
        if o.side.running {
            o.queue.push_back(text.to_string());
        } else {
            o.side.running = true;
            if let Some(p) = o.proc.as_mut() {
                let lines = p.dialect.ask(text);
                write(p, &lines);
            }
        }
        o.side.clone()
    };
    notify(side.clone());
    Ok(side)
}

/// Ends the session's side conversation: its copy stops, and what was said is gone.
pub fn close(key: &str) {
    let Some(o) = state().open.remove(key) else { return };
    if let Some(p) = o.proc {
        end(p);
    }
    notify(Side { key: key.to_string(), ..Default::default() });
}

/// How the session's copy runs, and where: in its own folder, or its worktree's when that
/// folder is gone.
fn recipe(board: &Board, cfg: &Config, key: &str) -> Result<(Recipe, PathBuf)> {
    let (s, account) = crate::harness::find(board, cfg, key)?;
    let lane = board.lanes.iter().find(|l| l.sessions.iter().any(|x| x.key == key && x.visitor.is_none())).or_else(|| board.lanes.iter().find(|l| l.sessions.iter().any(|x| x.key == key)));
    let cwd = match lane {
        _ if Path::new(&s.cwd).is_dir() => PathBuf::from(&s.cwd),
        Some(l) if Path::new(&l.path).is_dir() => PathBuf::from(&l.path),
        _ => bail!("Its folder is gone, so a copy of it has nowhere to run."),
    };
    let recipe = crate::harness::adapter(s.harness).side(&crate::harness::Ctx { account }, s, &cwd)?;
    Ok((recipe, cwd))
}

fn spawn(key: &str, id: u64, recipe: Recipe, cwd: &Path) -> Result<Proc> {
    let mut cmd = Command::new(&recipe.program);
    cmd.args(&recipe.args).current_dir(cwd).env("PATH", crate::procs::user_path()).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    for (k, v) in &recipe.env {
        cmd.env(k, v);
    }
    for k in &recipe.unset {
        cmd.env_remove(k);
    }
    let mut child = cmd.spawn().with_context(|| format!("Couldn't start {} for a side conversation.", recipe.program.display()))?;
    let (Some(stdin), Some(stdout), Some(stderr)) = (child.stdin.take(), child.stdout.take(), child.stderr.take()) else {
        let _ = child.kill();
        bail!("Couldn't reach the copy's process.");
    };
    // What it last complained of, for when it ends without answering.
    let complaint = Arc::new(Mutex::new(String::new()));
    {
        let complaint = complaint.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(std::result::Result::ok) {
                if !line.trim().is_empty() {
                    *complaint.lock().unwrap_or_else(|e| e.into_inner()) = line.trim().to_string();
                }
            }
        });
    }
    {
        let key = key.to_string();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(std::result::Result::ok) {
                if let Ok(v) = serde_json::from_str::<Value>(&line) {
                    heard(&key, id, &v);
                }
            }
            let why = complaint.lock().unwrap_or_else(|e| e.into_inner()).clone();
            stopped(&key, id, &why);
        });
    }
    let mut proc = Proc { id, pid: child.id(), stdin, child, dialect: recipe.dialect };
    let lines = proc.dialect.open();
    write(&mut proc, &lines);
    Ok(proc)
}

/// Writes lines to the copy. One it can't take means its process is going: the reader's end
/// says so.
fn write(p: &mut Proc, lines: &[Value]) {
    for l in lines {
        if writeln!(p.stdin, "{l}").is_err() {
            return;
        }
    }
    let _ = p.stdin.flush();
}

/// A line from copy `id` of the session's conversation.
fn heard(key: &str, id: u64, line: &Value) {
    let side = {
        let mut st = state();
        let Some(o) = st.open.get_mut(key) else { return };
        let Some(p) = o.proc.as_mut().filter(|p| p.id == id) else { return };
        let (events, replies) = p.dialect.hear(line);
        write(p, &replies);
        if events.is_empty() {
            return;
        }
        for e in events {
            apply(o, e);
        }
        o.side.clone()
    };
    notify(side);
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
                t.tool = Some(truncate(line.lines().next().unwrap_or("").trim(), 120));
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
    let side = {
        let mut st = state();
        let Some(o) = st.open.get_mut(key) else { return };
        if o.proc.as_ref().map(|p| p.id) != Some(id) {
            return;
        }
        if let Some(mut p) = o.proc.take() {
            std::thread::spawn(move || {
                let _ = p.child.wait();
            });
        }
        let why = if complaint.is_empty() { "The copy stopped before it answered.".to_string() } else { format!("The copy stopped: {}", truncate(complaint, 300)) };
        for t in o.side.turns.iter_mut().filter(|t| !t.done) {
            t.done = true;
            t.tool = None;
            t.error = Some(why.clone());
        }
        o.queue.clear();
        o.side.running = false;
        o.side.ended = true;
        o.side.clone()
    };
    notify(side);
}

/// Ends the copies that have sat idle for `idle`: their processes hold a conversation's worth
/// of memory each. What was said stays to read.
fn end_idle(idle: Duration) {
    let mut ended = Vec::new();
    {
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
    }
    for side in ended {
        notify(side);
    }
}

/// Ends a copy's process: its input closes, which both harnesses' processes leave on, and it's
/// killed if it's still there a moment later (mid-answer, say).
fn end(p: Proc) {
    let Proc { stdin, mut child, .. } = p;
    drop(stdin);
    std::thread::spawn(move || {
        let until = Instant::now() + Duration::from_millis(1500);
        while Instant::now() < until {
            if matches!(child.try_wait(), Ok(Some(_))) {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let _ = child.kill();
        let _ = child.wait();
    });
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
        st.open.insert(key.to_string(), Open { side: Side { key: key.to_string(), ..Default::default() }, queue: VecDeque::new(), proc: Some(proc), used: Instant::now() });
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

    #[test]
    fn an_answer_starts_at_its_first_word() {
        let mut o = Open { side: Side { turns: vec![Turn { question: "q".into(), ..Default::default() }], running: true, ..Default::default() }, queue: VecDeque::new(), proc: None, used: Instant::now() };
        for e in [Event::Said("\n\n".into()), Event::Tool("Read · `a.rs`\nmore".into()), Event::Said("It ".into()), Event::Said("does.\n".into()), Event::Done(None)] {
            apply(&mut o, e);
        }
        assert_eq!(o.side.turns[0], Turn { question: "q".into(), answer: "It does.".into(), tool: None, done: true, error: None });
        assert!(!o.side.running);
    }
}
