//! Sessions Codealong runs itself, over the Agent Client Protocol (agentclientprotocol.com):
//! an adapter process per account speaks JSON-RPC on its stdin and stdout, one message a line,
//! and runs any number of sessions. Codex (`codex-acp`) and Claude Code (`claude-agent-acp`)
//! need an adapter, which `codealong install` puts in Codealong's folder; OpenCode has one
//! built in (`opencode acp`). An account's `acp` setting in config.toml runs another command.
//!
//! The adapters keep sessions where their harness always does, under the harness's own id: a
//! Claude Code transcript, a Codex rollout, a row in OpenCode's database. So a hosted session
//! is on the board like any other, read from those records; what this module adds is what only
//! the process knows: that a turn is running, the permission request it waits on, the reply as
//! it streams, and the way to send it a prompt. The engine lays that over the board
//! (`overlay`), and the panel sends and answers through here.
//!
//! The processes are children of the app, so they end with it. The sessions Codealong started
//! are remembered (`hosted.json`); sending to one after a restart resumes it in a new process.
//! A turn running as the app or its process ends is cut off partway, its agent's own record
//! ending it as stopped: hosted.json notes each turn while it runs (`note_turn`), so a session
//! whose turn is still noted at launch, or whose process ended under it, shows as cut off
//! (`Session::cut_off`) rather than as one stopped on purpose.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::config::{self, AccountCfg};
use crate::harness::{Answer, Pending, Switch};
use crate::model::{Harness, Session, Status};
use crate::rpc::{self, Failure};
use crate::store;
use crate::util::{now_ms, sh_quote, truncate};

/// The adapter packages `codealong install` puts in `<support dir>/acp`, at versions checked
/// against Codealong.
pub const PACKAGES: [(&str, &str); 2] = [("@agentclientprotocol/claude-agent-acp", "0.81.2"), ("@agentclientprotocol/codex-acp", "1.13.1")];

/// How long a request other than a prompt may take: starting a session loads the agent's
/// tools and MCP servers, which can take a while.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(90);

/// A turn that was cut off (`Hosted::cut_off`) is still the session's last while its record has no
/// prompt from more than this after it began: the turn's own is written once the session is
/// picked up in the process, which can take a while, and a later one means it went on elsewhere.
const CUT_OFF_PROMPT_MS: i64 = 5 * 60_000;

/// How long npm may take to install the adapters: under a minute on a good connection, and
/// it's stopped rather than left to hold `codealong install` for good on a bad one.
const INSTALL_TIMEOUT: Duration = Duration::from_secs(5 * 60);

/// What changed, for whoever shows it.
pub enum Change {
    /// A turn started or ended, a permission request came or went, a process started or
    /// ended: the board needs composing again.
    Board,
    /// The reply being written, for a panel showing the session.
    Stream(Stream),
}

/// The reply being written; it goes to other devices watching the session too, so it's in
/// codealong-peer.
pub use codealong_peer::api::Stream;

/// A permission request waiting for an answer.
#[derive(Clone, Debug)]
struct Permission {
    /// The JSON-RPC id to answer.
    rpc: Value,
    tool_call: String,
    label: String,
    input: Option<String>,
    /// As offered, in order.
    options: Vec<Offered>,
    /// It's a plan to approve (`is_plan`): `input` is the plan.
    plan: bool,
    /// An answer to it is being written: a second isn't sent with it.
    answering: bool,
    /// When it was asked: the session waits on you from then, not from when its turn began, so
    /// a window that answered the call before can tell the next one from it (ahead.ts).
    asked_at: i64,
}

/// One of the answers an agent offers to a permission request.
#[derive(Clone, Debug, PartialEq)]
struct Offered {
    id: String,
    /// ACP's `allow_once`, `allow_always`, `reject_once` or `reject_always`.
    kind: String,
    /// Its words, e.g. "Yes, and use auto mode".
    name: String,
}

/// How a plan to approve reads, as Claude Code's own does in a terminal (`claude::transcript::PLAN`).
const PLAN: &str = "Plan";

/// A session Codealong runs, as the process last said.
#[derive(Clone, Debug, Default)]
struct Hosted {
    account: String,
    id: String,
    cwd: String,
    /// Loaded in the account's current process.
    loaded: bool,
    /// Picked up in the account's current process since it started, so its harness may hold it
    /// there even once it isn't `loaded`: after a delete that failed part way, its next goes
    /// there too (`delete_held`).
    held: bool,
    /// Being deleted there (`delete_held`): nothing goes to it until that's over.
    deleting: bool,
    running: bool,
    /// The running turn's claim on the prompt it's sending: none between turns.
    turn: Option<Claim>,
    /// When the running turn started, or the last one ended.
    since: i64,
    permission: Option<Permission>,
    /// Prompts sent while a turn ran, sent in order once it ends.
    queue: Vec<String>,
    /// Replaying history into the process (`session/load`): its updates aren't news.
    replaying: bool,
    text: String,
    tool: Option<String>,
    /// Why the last turn failed, when it did.
    error: Option<String>,
    /// Its config options as the agent last listed them (`configOptions`), for switching its model.
    options: Value,
    /// It's in hosted.json, which notes its turn while it runs (`note_turn`).
    remembered: bool,
    /// When the turn that was cut off partway began: its process ended while it ran, or the app
    /// did (its turn still noted in hosted.json at launch). None once another begins.
    cut_off: Option<i64>,
}

/// A turn's claim on its prompt, from when the turn begins (`begin`), or takes the next prompt
/// queued, to when that prompt is written. A Stop before then ends the turn with nothing sent,
/// however long the session takes to be picked up in the process; a Stop after goes to the
/// agent, after the prompt (`rpc::Link::request_if`).
#[derive(Clone, Debug, Default)]
struct Claim(Arc<AtomicU8>);

impl Claim {
    const WAITING: u8 = 0;
    const SENT: u8 = 1;
    const STOPPED: u8 = 2;

    fn from_waiting(&self, to: u8) -> bool {
        self.0.compare_exchange(Self::WAITING, to, Ordering::SeqCst, Ordering::SeqCst).is_ok()
    }

    /// Takes the claim to write the prompt: false when the turn was stopped first.
    fn send(&self) -> bool {
        self.from_waiting(Self::SENT)
    }

    /// Stops the turn before its prompt is written: false when it has been, or the turn was
    /// stopped already.
    fn stop(&self) -> bool {
        self.from_waiting(Self::STOPPED)
    }

    fn stopped(&self) -> bool {
        self.0.load(Ordering::SeqCst) == Self::STOPPED
    }

    fn is(&self, other: &Claim) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

/// One adapter process.
struct Agent {
    account: String,
    /// The process, which is killed when this goes.
    client: rpc::Client,
    /// Its `agentCapabilities`.
    caps: Mutex<Value>,
}

#[derive(Default)]
struct State {
    agents: HashMap<String, Arc<Agent>>,
    /// By session key.
    sessions: HashMap<String, Hosted>,
    remembered: bool,
}

static STATE: OnceLock<Mutex<State>> = OnceLock::new();
static NOTIFY: OnceLock<Box<dyn Fn(Change) + Send + Sync>> = OnceLock::new();

fn state() -> std::sync::MutexGuard<'static, State> {
    let m = STATE.get_or_init(|| Mutex::new(State::default()));
    let mut s = m.lock().unwrap_or_else(|e| e.into_inner());
    if !s.remembered {
        s.remembered = true;
        for r in load_remembered() {
            let key = key_of(r.harness, &r.account, &r.id);
            s.sessions.entry(key).or_insert(Hosted { account: r.account, id: r.id, cwd: r.cwd, remembered: true, cut_off: r.turn, ..Default::default() });
        }
    }
    s
}

/// Tells the app how to show changes. Called once, at launch.
pub fn init(notify: impl Fn(Change) + Send + Sync + 'static) {
    let _ = NOTIFY.set(Box::new(notify));
}

/// The processes are the app's children: a `codealong` command run without the app would take
/// them with it when it exits.
fn in_app() -> Result<()> {
    if NOTIFY.get().is_none() {
        bail!("Sessions Codealong runs need the app running.");
    }
    Ok(())
}

fn notify(c: Change) {
    if let Some(f) = NOTIFY.get() {
        f(c);
    }
}

fn key_of(h: Harness, account: &str, id: &str) -> String {
    Session::key_for(h, account, id)
}

// ---- remembering ----

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
struct Remembered {
    harness: Harness,
    account: String,
    id: String,
    cwd: String,
    /// When its running turn began, while it runs (`note_turn`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    turn: Option<i64>,
}

/// What hosted.json holds, for the error when it doesn't read as that.
const WHAT: &str = "a list of the sessions Codealong runs";

fn remembered_path() -> PathBuf {
    config::support_dir().join("hosted.json")
}

/// The sessions to pick up again at launch. A file that doesn't parse gives none, and is left
/// as it is (`remember_in`).
fn load_remembered() -> Vec<Remembered> {
    store::read_or_default(&remembered_path())
}

fn remember(r: Remembered) {
    // Nobody waits on this as a session starts, so a failure is a notice.
    let path = remembered_path();
    let _ = store::noted(&path, remember_in(&path, r));
}

/// Adds a session to hosted.json, under the file's lock: two sessions starting at once on two
/// accounts both land. A file that doesn't parse is an error and is left as it is, since an
/// older build that doesn't know a newer one's harness must not write its own list over it.
fn remember_in(path: &Path, r: Remembered) -> Result<()> {
    store::change(path, WHAT, |all: &mut Vec<Remembered>| {
        if all.iter().any(|x| x.harness == r.harness && x.id == r.id && x.account == r.account) {
            return false;
        }
        all.push(r);
        true
    })?;
    Ok(())
}

/// Notes in hosted.json that session `key`'s turn begun at `began` runs (`runs`), or that it
/// ended as turns do. A later turn's note stands over an earlier one's, whichever thread gets
/// there first (a turn stopped as it was picked up, followed by another), and an end takes off
/// only its own. Only for a session hosted.json has (`Hosted::remembered`); nothing waits on
/// it, so a failure is a notice.
fn note_turn(key: &str, began: i64, runs: bool) {
    let Some((account, id)) = state().sessions.get(key).filter(|h| h.remembered).map(|h| (h.account.clone(), h.id.clone())) else { return };
    let path = remembered_path();
    let _ = store::noted(&path, note_turn_in(&path, &account, &id, began, runs));
}

fn note_turn_in(path: &Path, account: &str, id: &str, began: i64, runs: bool) -> Result<()> {
    store::change(path, WHAT, |all: &mut Vec<Remembered>| {
        let Some(r) = all.iter_mut().find(|r| r.account == account && r.id == id) else { return false };
        let to = if runs { r.turn.max(Some(began)) } else if r.turn == Some(began) { None } else { r.turn };
        std::mem::replace(&mut r.turn, to) != to
    })?;
    Ok(())
}

// ---- the adapters ----

/// Where `codealong install` puts the adapter packages.
pub fn install_dir() -> PathBuf {
    config::support_dir().join("acp")
}

fn installed_bin(name: &str) -> Option<PathBuf> {
    let p = install_dir().join("node_modules/.bin").join(name);
    p.exists().then_some(p)
}

/// How an account's adapter starts: a program and its arguments, and what goes in its
/// environment. None when there's no adapter for it, with why.
pub fn adapter_command(a: &AccountCfg) -> std::result::Result<(String, Vec<(String, String)>), String> {
    if let Some(cmd) = a.acp.as_deref().filter(|c| !c.trim().is_empty()) {
        return Ok((cmd.to_string(), Vec::new()));
    }
    crate::harness::of(a).ok_or_else(|| format!("{} has no Agent Client Protocol adapter.", a.harness))?.acp_command(a)
}

/// An adapter package's program, quoted for the shell, or what to run to install it.
pub(crate) fn package_program(bin: &str, h: Harness) -> std::result::Result<String, String> {
    installed_bin(bin).map(|p| sh_quote(&p.to_string_lossy())).ok_or_else(|| format!("Run `codealong install` to add the adapter that lets Codealong run {} sessions itself.", h.word()))
}

/// Installs the adapter packages with npm, or brings them to the versions Codealong expects.
/// Says what it did.
pub fn install_adapters() -> Result<String> {
    let dir = install_dir();
    std::fs::create_dir_all(&dir)?;
    let manifest = dir.join("package.json");
    if !manifest.exists() {
        std::fs::write(&manifest, "{\n  \"name\": \"codealong-acp\",\n  \"private\": true\n}\n")?;
    }
    let current = PACKAGES.iter().all(|(name, version)| {
        std::fs::read(dir.join("node_modules").join(name).join("package.json"))
            .ok()
            .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
            .and_then(|v| v.get("version").and_then(Value::as_str).map(|v| v == *version))
            .unwrap_or(false)
    });
    if current {
        return Ok(format!("ACP adapters: up to date in {}", crate::util::tilde(&dir)));
    }
    let specs: Vec<String> = PACKAGES.iter().map(|(n, v)| format!("{n}@{v}")).collect();
    let mut args = vec!["install", "--no-fund", "--no-audit", "--save-exact"];
    args.extend(specs.iter().map(String::as_str));
    let out = run_npm("npm", &args, &dir, INSTALL_TIMEOUT)?;
    if !out.ok {
        bail!("npm couldn't install the adapters: {}", out.err.lines().last().unwrap_or(""));
    }
    Ok(format!("ACP adapters: {} in {}", specs.join(", "), crate::util::tilde(&dir)))
}

/// Runs npm in the adapters' folder for at most `timeout`, like every other program the engine
/// runs and waits for.
fn run_npm(npm: &str, args: &[&str], dir: &Path, timeout: Duration) -> Result<crate::procs::Output> {
    let started = std::time::Instant::now();
    match crate::procs::run_output(npm, args, Some(dir), timeout) {
        Ok(out) => Ok(out),
        // `run_output` fails when the program doesn't start and when its time runs out, and
        // only the second takes this long.
        Err(_) if started.elapsed() >= timeout => bail!("npm timed out after {} minutes installing the adapters. Check the connection and try again.", timeout.as_secs() / 60),
        Err(e) => Err(e.context("npm isn't installed: the adapters are Node packages. Install Node (brew install node) and try again.")),
    }
}

impl Agent {
    /// Sends a request and waits for its answer; None waits as long as the process lives.
    fn request(&self, method: &str, params: Value, timeout: Option<Duration>) -> Result<Value> {
        self.request_if(method, params, timeout, || true).unwrap_or_else(|| unreachable!("a request that's always wanted is always made"))
    }

    /// As `request`, if `wanted()` says so as it's written (`rpc::Link::request_if`).
    fn request_if(&self, method: &str, params: Value, timeout: Option<Duration>, wanted: impl FnOnce() -> bool) -> Option<Result<Value>> {
        Some(self.client.request_if(method, params, timeout, wanted)?.map_err(|e| match e {
            Failure::Timeout => anyhow!("{} didn't answer {method} in time.", self.account),
            Failure::Closed(_) => anyhow!("{}'s agent stopped.", self.account),
            Failure::Refused(e) => anyhow!("{}", e.get("message").and_then(Value::as_str).unwrap_or("error")),
            Failure::Write(e) => e.into(),
        }))
    }

    /// The protocol's handshake: who's asking and what Codealong lends the agent. It answers
    /// with what it can do.
    fn initialize(&self) -> Result<Value> {
        self.request(
            "initialize",
            json!({
                "protocolVersion": 1,
                "clientInfo": rpc::client_info(),
                // The agents read and write files and run commands with their own tools.
                "clientCapabilities": { "fs": { "readTextFile": false, "writeTextFile": false }, "terminal": false },
            }),
            Some(REQUEST_TIMEOUT),
        )
    }

    fn can(&self, what: &str) -> bool {
        let caps = self.caps.lock().unwrap_or_else(|e| e.into_inner());
        match what {
            "load" => caps.get("loadSession").and_then(Value::as_bool).unwrap_or(false),
            other => caps.pointer(&format!("/sessionCapabilities/{other}")).is_some(),
        }
    }
}

/// The account's adapter process, started when there isn't one running.
fn agent(a: &AccountCfg) -> Result<Arc<Agent>> {
    if let Some(ag) = running(&a.name) {
        return Ok(ag);
    }
    let (command, env) = adapter_command(a).map_err(|e| anyhow!(e))?;
    // Through a shell, so an account's command may set variables the way it does in a terminal.
    let mut cmd = Command::new("/bin/sh");
    cmd.args(["-c", &format!("exec {command}")]).envs(env).env("PATH", adapter_path()).current_dir(crate::util::home());
    let ag = connect(&a.name, cmd, log_file(&a.name)).with_context(|| format!("Couldn't start {}'s agent ({command}).", a.name))?;
    let init = match ag.initialize() {
        Ok(v) => v,
        // Its process is killed as `ag` goes, here and below.
        Err(e) => bail!("{}'s agent didn't start ({e:#}). Its log: {}", a.name, crate::util::tilde(&log_path(&a.name))),
    };
    *ag.caps.lock().unwrap_or_else(|e| e.into_inner()) = init.get("agentCapabilities").cloned().unwrap_or(Value::Null);
    keep_agent(&a.name, ag).map_err(|e| anyhow!("{e} Its log: {}", crate::util::tilde(&log_path(&a.name))))
}

/// The account's process, if it has one that's still running. One whose output has ended is
/// let go here, if its end hasn't been heard yet (`stopped`), so the next use starts another.
fn running(account: &str) -> Option<Arc<Agent>> {
    {
        let mut st = state();
        match st.agents.get(account) {
            Some(ag) if !ag.client.is_closed() => return Some(ag.clone()),
            Some(_) => let_go(&mut st, account),
            None => return None,
        }
    }
    notify(Change::Board);
    None
}

/// Whether the account has a process that's running.
fn alive(st: &State, account: &str) -> bool {
    st.agents.get(account).is_some_and(|a| !a.client.is_closed())
}

/// Makes `ag` the account's process, and returns the account's process: another start may have
/// beaten this one to it, and the first one in stays while it runs. One that stopped as it started
/// isn't kept, or the account's sessions would be sent to a process that's gone until the app
/// restarts. Whichever way, its end is heard as the account's (`stopped`) only once it's kept:
/// it's checked under the lock that end takes.
fn keep_agent(account: &str, ag: Arc<Agent>) -> Result<Arc<Agent>> {
    let (kept, ended) = {
        let mut st = state();
        let ended = match st.agents.get(account) {
            Some(existing) if !existing.client.is_closed() => return Ok(existing.clone()),
            Some(_) => {
                let_go(&mut st, account);
                true
            }
            None => false,
        };
        if ag.client.is_closed() {
            (Err(anyhow!("{account}'s agent stopped as it started.")), ended)
        } else {
            st.agents.insert(account.to_string(), ag.clone());
            (Ok(ag), ended)
        }
    };
    if ended {
        notify(Change::Board);
    }
    kept
}

/// Starts an account's adapter process, with its stderr in `log`. What it writes that isn't an
/// answer is heard on its client's thread (`hear`), and so is its end (`stopped`).
fn connect(account: &str, cmd: Command, log: std::fs::File) -> std::io::Result<Arc<Agent>> {
    let of = account.to_string();
    let client = rpc::Client::spawn(cmd, rpc::Stderr::Log(log), rpc::Wire::JsonRpc2, move |link, heard| match heard {
        rpc::Heard::Line(msg) => hear(&of, link, msg),
        rpc::Heard::Closed(_) => stopped(&of, link),
    })?;
    Ok(Arc::new(Agent { account: account.to_string(), client, caps: Mutex::new(Value::Null) }))
}

/// The login shell's PATH, and where OpenCode's installer puts it, which only an interactive
/// shell adds.
fn adapter_path() -> String {
    format!("{}:{}", crate::procs::user_path(), crate::util::home().join(".opencode/bin").display())
}

fn log_path(account: &str) -> PathBuf {
    crate::util::home().join("Library/Logs/Codealong").join(format!("acp-{account}.log"))
}

fn log_file(account: &str) -> std::fs::File {
    let path = log_path(account);
    let _ = std::fs::create_dir_all(path.parent().unwrap_or(Path::new("/tmp")));
    std::fs::OpenOptions::new().create(true).append(true).open(&path).or_else(|_| std::fs::File::create("/dev/null")).expect("/dev/null opens")
}

/// A message from an account's process that isn't the answer to a request of Codealong's.
fn hear(account: &str, link: &rpc::Link, msg: &Value) {
    let method = msg.get("method").and_then(Value::as_str);
    match (method, msg.get("id")) {
        (Some("session/update"), _) => on_update(account, link, msg.get("params").unwrap_or(&Value::Null)),
        (Some("session/request_permission"), Some(id)) => on_permission(account, link, id.clone(), msg.get("params").unwrap_or(&Value::Null)),
        (Some(_), Some(id)) => {
            // Files and terminals are the agents' own; nothing else was offered.
            let _ = link.refuse(id, -32601, "Codealong doesn't offer that.");
        }
        _ => {}
    }
}

/// Whether `link` is the account's process. One that another start beat to it (`agent`) is let
/// go, and nothing it says or does as it goes is about the sessions of the one that stayed.
fn current(st: &State, account: &str, link: &rpc::Link) -> bool {
    st.agents.get(account).is_some_and(|a| std::ptr::eq::<rpc::Link>(&*a.client, link))
}

/// An account's process ended. Its requests have failed (the client fails them first), and its
/// sessions stop until they're sent to again.
fn stopped(account: &str, link: &rpc::Link) {
    {
        let mut st = state();
        if !current(&st, account, link) {
            return;
        }
        let_go(&mut st, account);
    }
    notify(Change::Board);
}

/// Forgets the account's process, which has ended, and stops what its sessions were doing in it.
fn let_go(st: &mut State, account: &str) {
    st.agents.remove(account);
    for h in st.sessions.values_mut().filter(|h| h.account == account && (h.loaded || h.held)) {
        h.loaded = false;
        h.held = false;
        if h.running {
            // Its prompt went, so the agent was at it: cut off partway. One whose prompt never
            // went just didn't go.
            if h.turn.take().is_some_and(|turn| !turn.stop()) {
                h.cut_off = Some(h.since);
            }
            h.running = false;
            h.since = now_ms();
            h.error = Some("Its agent stopped.".into());
        }
        h.permission = None;
        h.queue.clear();
    }
}

/// The key of the session a message from the account's process `link` is about; none from a
/// process that isn't the account's (`current`).
fn session_key(account: &str, link: &rpc::Link, params: &Value) -> Option<String> {
    let id = params.get("sessionId").and_then(Value::as_str)?;
    let st = state();
    if !current(&st, account, link) {
        return None;
    }
    st.sessions.iter().find(|(_, h)| h.account == account && h.id == id).map(|(k, _)| k.clone())
}

fn on_update(account: &str, link: &rpc::Link, params: &Value) {
    let Some(key) = session_key(account, link, params) else { return };
    let u = params.get("update").unwrap_or(&Value::Null);
    let kind = u.get("sessionUpdate").and_then(Value::as_str).unwrap_or("");
    let stream = {
        let mut st = state();
        let Some(h) = st.sessions.get_mut(&key) else { return };
        if h.replaying {
            return;
        }
        match kind {
            "agent_message_chunk" => {
                if let Some(t) = u.pointer("/content/text").and_then(Value::as_str) {
                    h.text.push_str(t);
                }
            }
            "tool_call" | "tool_call_update" => {
                h.text.clear();
                let status = u.get("status").and_then(Value::as_str);
                if matches!(status, Some("completed") | Some("failed")) {
                    h.tool = None;
                } else if let Some(title) = u.get("title").and_then(Value::as_str) {
                    h.tool = Some(truncate(&one_line(title), 120));
                }
            }
            _ => return,
        }
        Stream { key: key.clone(), text: h.text.clone(), tool: h.tool.clone() }
    };
    notify(Change::Stream(stream));
}

/// A tool call's title on one line, all of it and as it's written: `util::one_line` keeps only
/// a text's first line, and takes Markdown's links and bold out of it.
fn one_line(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn on_permission(account: &str, link: &rpc::Link, rpc: Value, params: &Value) {
    let Some(key) = session_key(account, link, params) else {
        let _ = link.respond(&rpc, json!({ "outcome": { "outcome": "cancelled" } }));
        return;
    };
    let call = params.get("toolCall").unwrap_or(&Value::Null);
    let title = call.get("title").and_then(Value::as_str).unwrap_or("A tool call");
    let name = call.get("name").and_then(Value::as_str).or_else(|| call.pointer("/_meta/claudeCode/toolName").and_then(Value::as_str));
    let raw = call.get("rawInput").filter(|v| !v.is_null() && v.as_object().map_or(true, |o| !o.is_empty()));
    let input = raw.map(|v| match v.get("command").and_then(Value::as_str) {
        Some(cmd) => cmd.to_string(),
        None => serde_json::to_string_pretty(v).unwrap_or_default(),
    });
    let label = match name {
        Some(n) if n != title => format!("{n} · `{}`", truncate(&one_line(title), 100)),
        _ => truncate(&one_line(title), 110),
    };
    let options = offered(params);
    let tool_call = call.get("toolCallId").and_then(Value::as_str).unwrap_or("").to_string();
    // A plan reads as itself, all of it, as a terminal session's does: it's what you approve.
    let plan = is_plan(call);
    let (label, input) = match raw.and_then(|v| v.get("plan")).and_then(Value::as_str).filter(|p| plan && !p.trim().is_empty()) {
        Some(text) => (PLAN.to_string(), Some(text.to_string())),
        None if plan => (PLAN.to_string(), input.map(|i| truncate(&i, 4000))),
        None => (label, input.map(|i| truncate(&i, 4000))),
    };
    {
        let mut st = state();
        let Some(h) = st.sessions.get_mut(&key) else { return };
        // A request still unanswered is overtaken: the agent only asks one at a time.
        h.permission = Some(Permission { rpc, tool_call, label, input, options, plan, answering: false, asked_at: now_ms() });
    }
    notify(Change::Board);
}

/// The answers a permission request offers, in order.
fn offered(params: &Value) -> Vec<Offered> {
    let Some(os) = params.get("options").and_then(Value::as_array) else { return Vec::new() };
    os.iter().filter_map(|o| Some(Offered { id: o.get("optionId")?.as_str()?.to_string(), kind: o.get("kind")?.as_str()?.to_string(), name: o.get("name").and_then(Value::as_str).unwrap_or("").to_string() })).collect()
}

/// Whether a permission request is a plan to approve: a call that switches the session's mode
/// (ACP's tool kind `switch_mode`), its plan in `rawInput.plan`. claude-agent-acp asks so for
/// Claude Code's `ExitPlanMode` (0.81.2: title "Approve Plan"), codex-acp for a plan to implement
/// (1.13.1: "Implement this plan?").
fn is_plan(call: &Value) -> bool {
    call.get("kind").and_then(Value::as_str) == Some("switch_mode")
}

/// Whether one of a plan's answers starts the session over from the plan alone, its
/// conversation cleared: claude-agent-acp 0.81.2 offers that first among the answers that allow
/// from now on, `exit-plan-clear-auto`, `-bypass` or `-accept-edits`, "Yes, clear context (12%
/// used) and use auto mode", as Claude Code's terminal does with `showClearContextOnPlanAccept`
/// set. No window says an answer does that, so none of them picks it.
fn clears_context(o: &Offered) -> bool {
    o.id.contains("clear") || o.name.to_lowercase().contains("clear context")
}

/// The answer to give `p` for `answer`, and no other, or why there's none. Allow is the agent's
/// answer that allows the call once and Deny the one that refuses it once, never those that keep
/// for calls like it. Always is the first that allows from now on; for a plan, that's the first
/// that goes on with the session's conversation, in a mode that asks less (claude-agent-acp:
/// auto, or bypassing permissions when the session did before it planned, else accepting
/// edits), while Allow goes on asking ("Yes, manually approve edits") and Deny keeps it planning,
/// ending the turn ("No, keep planning").
fn pick(p: &Permission, answer: Answer) -> std::result::Result<&Offered, &'static str> {
    let kind = match answer {
        Answer::Allow => "allow_once",
        Answer::Always => "allow_always",
        Answer::Deny => "reject_once",
    };
    let offered = |kind: &str| p.options.iter().find(|o| o.kind == kind && !(p.plan && clears_context(o)));
    offered(kind).ok_or(match answer {
        Answer::Allow if offered("allow_always").is_some() => "Its agent offers no way to allow this just once, only from now on: Always does that.",
        Answer::Allow => "Its agent offers no way to allow this.",
        Answer::Always => "Its agent offers no way to allow this from now on.",
        Answer::Deny => "Its agent offers no way to deny this just once.",
    })
}

// ---- what the app does ----

/// Starts a session in `cwd` on `prompt` and returns its key. The turn runs on; the board shows it.
pub fn start(a: &AccountCfg, cwd: &Path, prompt: &str, model: Option<&str>, effort: Option<&str>) -> Result<String> {
    in_app()?;
    let harness = Harness::from_config(&a.harness);
    let ag = agent(a)?;
    let r = ag.request("session/new", json!({ "cwd": cwd, "mcpServers": [] }), Some(REQUEST_TIMEOUT))?;
    let id = r.get("sessionId").and_then(Value::as_str).ok_or_else(|| anyhow!("{}'s agent made no session.", a.name))?.to_string();
    let options = tune(&ag, &id, &r, model, effort)?;
    let key = key_of(harness, &a.name, &id);
    let cwd_s = cwd.to_string_lossy().into_owned();
    remember(Remembered { harness, account: a.name.clone(), id: id.clone(), cwd: cwd_s.clone(), turn: None });
    let mut hosted = Hosted { account: a.name.clone(), id, cwd: cwd_s, loaded: true, held: true, options, remembered: true, ..Default::default() };
    let turn = (!prompt.trim().is_empty()).then(|| begin(&mut hosted));
    state().sessions.insert(key.clone(), hosted);
    if let Some(turn) = turn {
        run_turn(ag, key.clone(), prompt.to_string(), turn);
    }
    notify(Change::Board);
    Ok(key)
}

/// Takes on session `id`, which its harness's command line or app ran, and runs it from now on as
/// one Codealong started (`start::resume` in Codealong): loaded in the account's process from its
/// harness's record of it, on `model` and `effort` where the agent offers them, and on `prompt`
/// when there's one. Returns its key, the session's own: the ACP session is the harness's.
pub fn adopt(a: &AccountCfg, id: &str, cwd: &Path, prompt: &str, model: Option<&str>, effort: Option<&str>) -> Result<String> {
    in_app()?;
    let harness = Harness::from_config(&a.harness);
    let ag = agent(a)?;
    let key = key_of(harness, &a.name, id);
    let cwd_s = cwd.to_string_lossy().into_owned();
    let mut hosted = Hosted { account: a.name.clone(), id: id.to_string(), cwd: cwd_s.clone(), loaded: false, remembered: true, ..Default::default() };
    let was = (hosted.since, hosted.error.clone());
    // Its turn begins as it's taken on, so a send meanwhile goes after it.
    let turn = (!prompt.trim().is_empty()).then(|| begin(&mut hosted));
    state().sessions.insert(key.clone(), hosted);
    let loaded = match load(&ag, &key, id, &cwd_s) {
        Ok(r) => r,
        Err(e) => {
            state().sessions.remove(&key);
            return Err(e);
        }
    };
    remember(Remembered { harness, account: a.name.clone(), id: id.to_string(), cwd: cwd_s, turn: None });
    let options = match tune(&ag, id, &loaded, model, effort) {
        Ok(o) => o,
        Err(e) => {
            if let Some(turn) = &turn {
                abandon(&key, turn, was, &e);
            }
            return Err(e);
        }
    };
    set(&key, |h| h.options = options);
    if let Some(turn) = turn {
        run_turn(ag, key.clone(), prompt.to_string(), turn);
    }
    notify(Change::Board);
    Ok(key)
}

/// Asks for `model` and `effort` of session `id`, where the agent's answer about the session
/// (`answer`, which lists its `configOptions`) offers them: Claude Code's aliases (opus,
/// sonnet…) are among its models. The effort levels are the model's, so they're looked up in the
/// options the model's answer lists. Returns the options as the agent lists them after.
fn tune(ag: &Agent, id: &str, answer: &Value, model: Option<&str>, effort: Option<&str>) -> Result<Value> {
    let mut options = answer.get("configOptions").cloned().unwrap_or(Value::Null);
    for (category, value) in [("model", model), ("thought_level", effort)] {
        let Some((v, config_id)) = value.and_then(|v| Some((v, offered_option(&options, category, v)?))) else { continue };
        let got = ag.request("session/set_config_option", json!({ "sessionId": id, "configId": config_id, "value": v }), Some(REQUEST_TIMEOUT))?;
        if let Some(o) = got.get("configOptions") {
            options = o.clone();
        }
    }
    Ok(options)
}

/// Switches a session Codealong runs to a model or an effort level (`send::switch_model`,
/// `switch_effort`), picking it up in the account's process first when that's new since. The
/// model or level must be one its agent offers: the levels are its model's.
pub fn switch(a: &AccountCfg, key: &str, to: Switch) -> Result<()> {
    in_app()?;
    let (id, cwd, loaded) = {
        let st = state();
        let h = st.sessions.get(key).ok_or_else(|| anyhow!("Codealong doesn't run that session."))?;
        if h.deleting {
            bail!("It's being archived.");
        }
        (h.id.clone(), h.cwd.clone(), h.loaded && alive(&st, &a.name))
    };
    let ag = agent(a)?;
    if !loaded {
        load(&ag, key, &id, &cwd)?;
    }
    let options = state().sessions.get(key).map(|h| h.options.clone()).unwrap_or_default();
    let (model, effort) = match to {
        Switch::Model(m) => (Some(m), None),
        Switch::Effort(e) => (None, Some(e)),
    };
    let category = if model.is_some() { "model" } else { "thought_level" };
    if offered_option(&options, category, to.value()).is_none() {
        bail!("{}'s agent doesn't offer {}.", a.name, to.value());
    }
    let options = tune(&ag, &id, &json!({ "configOptions": options }), model, effort)?;
    set(key, |h| h.options = options);
    notify(Change::Board);
    Ok(())
}

/// Forks session `id` into a new one Codealong runs (`session/fork`): a copy of its
/// conversation that goes on in `cwd`, on `prompt` when there's one. Returns the copy's key.
pub fn fork(a: &AccountCfg, id: &str, cwd: &Path, prompt: &str) -> Result<String> {
    in_app()?;
    let harness = Harness::from_config(&a.harness);
    let ag = agent(a)?;
    if !ag.can("fork") {
        bail!("{}'s agent can't fork a session.", a.name);
    }
    let r = ag.request("session/fork", json!({ "sessionId": id, "cwd": cwd, "mcpServers": [] }), Some(REQUEST_TIMEOUT))?;
    let copy = r.get("sessionId").and_then(Value::as_str).ok_or_else(|| anyhow!("{}'s agent made no copy.", a.name))?.to_string();
    let key = key_of(harness, &a.name, &copy);
    let cwd_s = cwd.to_string_lossy().into_owned();
    remember(Remembered { harness, account: a.name.clone(), id: copy.clone(), cwd: cwd_s.clone(), turn: None });
    // codex-acp lets go of the thread once it's copied (1.13.1), so the first prompt picks it up
    // again as it would a session from before the process started.
    state().sessions.insert(key.clone(), Hosted { account: a.name.clone(), id: copy, cwd: cwd_s, loaded: false, remembered: true, ..Default::default() });
    let text = crate::attachments::expand(prompt.trim(), crate::attachments::Style::Path);
    if !text.is_empty() {
        send(a, &key, &text)?;
    }
    notify(Change::Board);
    Ok(key)
}

/// The id of a session's config option in `category` (ACP's `model`, `thought_level`) when it
/// offers `value`.
fn offered_option(options: &Value, category: &str, value: &str) -> Option<String> {
    let o = options.as_array()?.iter().find(|o| o.get("category").and_then(Value::as_str) == Some(category))?;
    let id = o.get("id")?.as_str()?.to_string();
    o.get("options")?.as_array()?.iter().any(|x| x.get("value").and_then(Value::as_str) == Some(value)).then_some(id)
}

/// The value a session's config option in `category` is set to (`currentValue`).
fn current_option(options: &Value, category: &str) -> Option<String> {
    let o = options.as_array()?.iter().find(|o| o.get("category").and_then(Value::as_str) == Some(category))?;
    o.get("currentValue")?.as_str().filter(|v| !v.is_empty()).map(String::from)
}

/// Whether Codealong runs this session (or ran it, and can again).
pub fn hosts(key: &str) -> bool {
    state().sessions.contains_key(key)
}

/// Sends a prompt to a session Codealong runs. Says what happened, in a sentence.
pub fn send(a: &AccountCfg, key: &str, text: &str) -> Result<String> {
    in_app()?;
    // Whether a turn is running or this one begins is decided under one lock, before the session
    // is picked up in the process: two sends at once start one turn, the other queued behind it.
    let begun = {
        let mut guard = state();
        let st = &mut *guard;
        let alive = alive(st, &a.name);
        let h = st.sessions.get_mut(key).ok_or_else(|| anyhow!("Codealong doesn't run that session."))?;
        if h.deleting {
            bail!("It's being archived.");
        }
        if h.running {
            h.queue.push(text.to_string());
            None
        } else {
            let was = (h.since, h.error.clone());
            let turn = begin(h);
            Some((h.id.clone(), h.cwd.clone(), h.loaded && alive, was, turn))
        }
    };
    notify(Change::Board);
    let Some((id, cwd, loaded, was, turn)) = begun else {
        return Ok("Queued: it goes after the current turn.".into());
    };
    let ag = match agent(a).and_then(|ag| if loaded { Ok(ag) } else { load(&ag, key, &id, &cwd).map(|_| ag) }) {
        Ok(ag) => ag,
        Err(e) => {
            abandon(key, &turn, was, &e);
            return Err(e);
        }
    };
    // Stopped while it was being picked up: the turn has ended, and this never goes.
    if turn.stopped() {
        bail!("Stopped before it was sent.");
    }
    run_turn(ag, key.to_string(), text.to_string(), turn);
    Ok("Sent.".into())
}

/// Opens a session in the process: resumed where the agent can, else loaded from its history.
/// Returns the agent's answer, which lists the session's config options.
fn load(ag: &Arc<Agent>, key: &str, id: &str, cwd: &str) -> Result<Value> {
    let params = json!({ "sessionId": id, "cwd": cwd, "mcpServers": [] });
    let answer = if ag.can("resume") {
        ag.request("session/resume", params, Some(REQUEST_TIMEOUT))?
    } else if ag.can("load") {
        set(key, |h| h.replaying = true);
        let r = ag.request("session/load", params, Some(REQUEST_TIMEOUT));
        set(key, |h| h.replaying = false);
        r?
    } else {
        bail!("{}'s agent can't pick a session up again.", ag.account);
    };
    let options = answer.get("configOptions").cloned().unwrap_or_default();
    set(key, |h| {
        h.loaded = true;
        h.held = true;
        h.options = options;
    });
    Ok(answer)
}

fn set(key: &str, f: impl FnOnce(&mut Hosted)) {
    if let Some(h) = state().sessions.get_mut(key) {
        f(h);
    }
}

/// Marks the session's turn begun, for whoever runs it (`run_turn`): a send from then on queues
/// behind it, while the session is still being picked up in the process too. Returns the turn's
/// claim on its prompt.
fn begin(h: &mut Hosted) -> Claim {
    let turn = Claim::default();
    h.running = true;
    h.turn = Some(turn.clone());
    // After the last turn's begin and end, within a millisecond too: a turn's note in hosted.json
    // is known by it (`note_turn`).
    h.since = now_ms().max(h.since + 1);
    h.error = None;
    h.cut_off = None;
    h.text.clear();
    h.tool = None;
    turn
}

/// A turn `begin` marked that never ran, because the session couldn't be picked up: the session
/// is as it was (`was`: when its last turn ended, and how), and what was sent meanwhile goes too,
/// which its error says. A turn stopped meanwhile has ended already, and is left as Stop left it.
fn abandon(key: &str, turn: &Claim, was: (i64, Option<String>), e: &anyhow::Error) {
    if !turn.stop() {
        return;
    }
    set(key, |h| {
        h.running = false;
        h.turn = None;
        (h.since, h.error) = was;
        if !h.queue.is_empty() {
            h.queue.clear();
            h.error = Some(format!("{e:#} What was sent while it was being picked up wasn't sent either."));
        }
    });
    notify(Change::Board);
}

/// Runs a turn `begin` marked on a thread of its own, then the next queued prompt, until the
/// queue is empty or a turn fails. Each prompt goes only while its turn's claim holds: a turn stopped first was
/// ended by whoever stopped it, and one ended without it (its process stopped) may have been
/// followed by another, so neither is touched here. Each is noted in hosted.json while it runs
/// (`note_turn`), and one whose process ends under it is cut off, left noted.
fn run_turn(ag: Arc<Agent>, key: String, text: String, turn: Claim) {
    std::thread::spawn(move || {
        let (mut text, mut turn) = (text, turn);
        loop {
            let (id, began) = {
                let st = state();
                let h = st.sessions.get(&key);
                (h.map(|h| h.id.clone()).unwrap_or_default(), h.filter(|h| h.turn.as_ref().is_some_and(|t| t.is(&turn))).map(|h| h.since))
            };
            // Before its prompt goes, so a turn the app's end cuts off is known at the next launch.
            if let Some(b) = began {
                note_turn(&key, b, true);
            }
            let prompt = json!([{ "type": "text", "text": text }]);
            let Some(r) = ag.request_if("session/prompt", json!({ "sessionId": id, "prompt": prompt }), None, || turn.send()) else {
                if let Some(b) = began {
                    note_turn(&key, b, false);
                }
                return;
            };
            // Its process ended under it, as it does when the app ends: cut off partway.
            let cut = r.is_err() && ag.client.is_closed();
            let next = {
                let mut st = state();
                let Some(h) = st.sessions.get_mut(&key).filter(|h| h.turn.as_ref().is_some_and(|t| t.is(&turn))) else { return };
                if cut {
                    h.cut_off = began;
                }
                h.permission = None;
                h.text.clear();
                h.tool = None;
                h.error = match &r {
                    Ok(v) => match v.get("stopReason").and_then(Value::as_str) {
                        Some("refusal") => Some("The agent refused.".into()),
                        Some("max_tokens") => Some("The reply ran out of room.".into()),
                        _ => None,
                    },
                    Err(e) => Some(format!("{e:#}")),
                };
                // What was sent while a turn that failed ran goes with it, and its error says so:
                // it was written to follow that turn, and the windows showed it sent, so it isn't
                // sent now as though nothing had happened, nor left to follow the next send.
                if r.is_err() && !h.queue.is_empty() {
                    h.queue.clear();
                    if let Some(e) = h.error.as_mut() {
                        e.push_str(" What was sent while it ran wasn't sent: send it again if it's still wanted.");
                    }
                }
                if h.queue.is_empty() {
                    h.running = false;
                    h.turn = None;
                    h.since = now_ms();
                    None
                } else {
                    h.since = now_ms();
                    let next = Claim::default();
                    h.turn = Some(next.clone());
                    Some((h.queue.remove(0), next))
                }
            };
            // Its end is noted whether or not a queued prompt follows: that one notes its own
            // turn, and a Stop before it begins leaves nothing to.
            if let (false, Some(b)) = (cut, began) {
                note_turn(&key, b, false);
            }
            notify(Change::Stream(Stream { key: key.clone(), text: String::new(), tool: None }));
            notify(Change::Board);
            match next {
                Some(n) => (text, turn) = n,
                None => return,
            }
        }
    });
}

/// Stops the running turn. The agent ends it as cancelled; a permission request waiting is
/// answered as cancelled first, as the protocol asks. A turn whose prompt hasn't been written
/// yet, its session still being picked up in the process, ends here, and its prompt never goes.
pub fn cancel(key: &str) -> Result<()> {
    let asked = {
        let mut st = state();
        let h = st.sessions.get_mut(key).ok_or_else(|| anyhow!("Codealong doesn't run that session."))?;
        h.queue.clear();
        if h.turn.as_ref().is_some_and(Claim::stop) {
            h.running = false;
            h.turn = None;
            h.since = now_ms();
            None
        } else {
            Some((h.account.clone(), h.id.clone(), h.permission.take()))
        }
    };
    let Some((account, id, permission)) = asked else {
        notify(Change::Board);
        return Ok(());
    };
    let ag = state().agents.get(&account).cloned().ok_or_else(|| anyhow!("That session isn't running."))?;
    if let Some(p) = permission {
        ag.client.respond(&p.rpc, json!({ "outcome": { "outcome": "cancelled" } }))?;
    }
    ag.client.notify("session/cancel", json!({ "sessionId": id }))?;
    notify(Change::Board);
    Ok(())
}

/// Ends a session the account's process has picked up with the agent's `session/delete`, where
/// it offers that, and says whether it did: false when the process doesn't hold the session,
/// which is then the harness's to act on as it would any other. What a delete does is the
/// adapter's to say, so only a harness whose adapter's is known calls this: codex-acp's archives
/// the thread through its own app-server, the one that holds it (codex/mod.rs `archive`). One
/// taking a turn is refused rather than stopped.
pub fn delete_held(account: &str, harness: Harness, id: &str) -> Result<bool> {
    let key = key_of(harness, account, id);
    let ag = {
        let mut guard = state();
        let st = &mut *guard;
        let Some(ag) = st.agents.get(account).filter(|a| !a.client.is_closed() && a.can("delete")).cloned() else { return Ok(false) };
        let Some(h) = st.sessions.get_mut(&key) else { return Ok(false) };
        if h.deleting {
            bail!("It's being archived.");
        }
        if !h.held {
            return Ok(false);
        }
        if h.running {
            bail!("It's taking a turn in Codealong: stop it first.");
        }
        // Taken out of the process from here, under the lock a send takes too: a send meanwhile
        // is refused rather than written to a session being closed, and one after picks it up
        // again from its history, as it must after a delete that failed part way (codex-acp
        // closes the session before it archives the thread).
        h.loaded = false;
        h.deleting = true;
        ag
    };
    let deleted = ag.request("session/delete", json!({ "sessionId": id }), Some(REQUEST_TIMEOUT));
    // The process lets go of a session it deleted. One it failed to may still be its, so trying
    // again goes there too.
    let ok = deleted.is_ok();
    set(&key, |h| {
        h.deleting = false;
        if ok {
            h.held = false;
        }
    });
    notify(Change::Board);
    deleted?;
    Ok(true)
}

/// What a hosted session waits on you to allow.
pub fn pending(key: &str) -> Option<Pending> {
    let st = state();
    Some(pending_of(st.sessions.get(key)?.permission.as_ref()?))
}

fn pending_of(p: &Permission) -> Pending {
    let always = pick(p, Answer::Always).ok();
    // What a plan's Always does is the agent's to say, in its own words.
    let always_rule = always.filter(|_| p.plan).map(|o| format!("Approves the plan and keeps the conversation, as its agent's “{}”.", o.name.trim()));
    Pending { id: p.tool_call.clone(), label: p.label.clone(), input: p.input.clone(), cannot: None, always: always.is_some(), no_once: pick(p, Answer::Allow).is_err(), plan: p.plan, message: false, always_rule, questions: Vec::new(), unwritten: false, terminal: None }
}

/// Answers the permission request for tool call `id` with the option its agent offered for that
/// answer (`pick`), and no other. The request stays waiting until the answer is written, so one
/// that couldn't be is answered again.
pub fn answer(key: &str, id: &str, answer: Answer) -> Result<String> {
    let (ag, rpc, option, plan) = {
        let mut guard = state();
        let st = &mut *guard;
        let h = st.sessions.get_mut(key).ok_or_else(|| anyhow!("Codealong doesn't run that session."))?;
        let p = match h.permission.as_mut() {
            Some(p) if p.tool_call == id => p,
            _ => bail!("It isn't waiting on that any more."),
        };
        if p.answering {
            bail!("It's still answering that.");
        }
        let picked = pick(p, answer).map_err(|e| anyhow!(e))?;
        let (option, plan) = (picked.id.clone(), p.plan.then(|| picked.name.trim().to_string()));
        let ag = st.agents.get(&h.account).cloned().ok_or_else(|| anyhow!("That session's agent stopped."))?;
        p.answering = true;
        (ag, p.rpc.clone(), option, plan)
    };
    let written = ag.client.respond(&rpc, json!({ "outcome": { "outcome": "selected", "optionId": option } }));
    {
        let mut st = state();
        // The agent may have asked something else since, which stays.
        if let Some(h) = st.sessions.get_mut(key).filter(|h| h.permission.as_ref().is_some_and(|p| p.rpc == rpc)) {
            match &written {
                Ok(()) => h.permission = None,
                Err(_) => h.permission.as_mut().expect("checked").answering = false,
            }
        }
    }
    written?;
    notify(Change::Board);
    // What a plan's answers do is the agent's to say, so it's said in its words.
    Ok(match (answer, plan) {
        (Answer::Allow | Answer::Always, Some(name)) => format!("Approved the plan: “{name}”."),
        (Answer::Deny, Some(name)) => format!("Denied the plan: “{name}”."),
        (Answer::Allow, None) => "Allowed.".into(),
        (Answer::Always, None) => "Allowed, and from now on.".into(),
        (Answer::Deny, None) => "Denied.".into(),
    })
}

/// The reply streaming in a session, if it's running.
pub fn stream(key: &str) -> Option<Stream> {
    let st = state();
    let h = st.sessions.get(key).filter(|h| h.running)?;
    Some(Stream { key: key.to_string(), text: h.text.clone(), tool: h.tool.clone() })
}

/// Lays what the process knows over a session read from its records.
pub fn overlay(s: &mut Session, seen_at: i64) {
    let st = state();
    let Some(h) = st.sessions.get(&s.key) else { return };
    s.hosted = true;
    // You're at it here, whatever started it: one taken on from a `claude -p` run included.
    s.headless = false;
    let alive = h.loaded && alive(&st, &h.account);
    // Live, but with no pid of its own: the adapter's process serves all the account's hosted
    // sessions, and a session's pid is taken for its own (closing idle terminals, finding the
    // session a command runs in).
    if alive {
        s.live = true;
        s.pid = None;
    }
    s.open_target = format!("s|{}|hosted", s.key);
    // Its agent says the model it's on as soon as it's switched; its record says so only once the
    // next turn starts.
    if let Some(m) = current_option(&h.options, "model") {
        s.model = Some(m);
    }
    // Its effort the same way, but for `default`, the account's own, which only a reply names.
    // (claude-agent-acp would give its recommended level instead to a client that asked for one,
    // which Codealong doesn't.)
    if let Some(e) = current_option(&h.options, "thought_level").filter(|e| e != "default") {
        s.effort = Some(e);
    }
    if let Some(p) = &h.permission {
        s.status = Status::Needs;
        s.status_since = Some(p.asked_at);
        s.excerpt = Some(p.label.clone());
        s.stalled_since = None;
        s.asks = false;
    } else if h.running {
        s.status = Status::Working;
        s.status_since = Some(h.since);
        s.stalled_since = None;
        s.asks = false;
        if let Some(t) = &h.tool {
            s.excerpt = Some(format!("`{}`", t.replace('`', "")));
        }
    } else if h.cut_off.is_some_and(|at| !s.live && s.last_prompt_at <= at + CUT_OFF_PROMPT_MS) {
        // As its harness's reader says of a turn that what ran it left open (board.rs
        // `codex_session`): its agent's record ended this one as stopped.
        s.cut_off = Some(s.last_activity);
        s.status = if s.last_activity > seen_at { Status::Error } else { Status::Idle };
        s.status_since = Some(s.last_activity);
        s.stalled_since = None;
        s.asks = false;
    } else if let Some(e) = &h.error {
        s.status = Status::Error;
        s.excerpt = Some(e.clone());
    } else if matches!(s.status, Status::Stopped | Status::Working | Status::Needs) {
        // Its records can't say whether a process holds it, and a hosted one has no turn
        // running: read and quiet, or with something new since you looked.
        let ended = h.since.max(s.last_activity);
        s.status = if ended > seen_at { Status::Unread } else { Status::Idle };
        s.status_since = Some(ended);
        s.stalled_since = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wait_until(what: &str, secs: u64, f: impl Fn() -> bool) {
        let end = std::time::Instant::now() + Duration::from_secs(secs);
        while !f() {
            assert!(std::time::Instant::now() < end, "timed out waiting for {what}");
            std::thread::sleep(Duration::from_millis(200));
        }
    }

    /// A session waits on you from when its agent asked the call, so each call of a turn has a
    /// time of its own.
    #[test]
    fn a_hosted_session_needs_you_from_when_the_call_was_asked() {
        let key = key_of(Harness::Codex, "test-needs-since", "s1");
        let asked = |at: i64| Permission { rpc: json!(1), tool_call: "t".into(), label: "Run ls".into(), input: None, options: Vec::new(), plan: false, answering: false, asked_at: at };
        state().sessions.insert(key.clone(), Hosted { account: "test-needs-since".into(), id: "s1".into(), cwd: "/w".into(), running: true, since: 1_000, permission: Some(asked(5_000)), ..Default::default() });
        let mut s = crate::fixtures::session(&key);
        overlay(&mut s, 0);
        assert_eq!((s.status, s.status_since, s.excerpt.as_deref()), (Status::Needs, Some(5_000), Some("Run ls")));
        // The next call in the same turn.
        state().sessions.get_mut(&key).unwrap().permission = Some(asked(9_000));
        overlay(&mut s, 0);
        assert_eq!(s.status_since, Some(9_000));
        // Answered, it's working on the turn it was taking.
        state().sessions.get_mut(&key).unwrap().permission = None;
        overlay(&mut s, 0);
        assert_eq!((s.status, s.status_since), (Status::Working, Some(1_000)));
        state().sessions.remove(&key);
    }

    /// The effort its agent is set to is the one it's on before a reply says so, but for the
    /// account's own, which only a reply names.
    #[test]
    fn a_hosted_session_is_on_the_effort_its_agent_is_set_to() {
        let key = key_of(Harness::Claude, "test-effort", "s1");
        let effort = |current: &str| json!([{ "id": "effort", "category": "thought_level", "type": "select", "currentValue": current, "options": [{ "value": "default" }, { "value": "high" }] }]);
        state().sessions.insert(key.clone(), Hosted { account: "test-effort".into(), id: "s1".into(), cwd: "/w".into(), options: effort("high"), ..Default::default() });
        let mut s = crate::fixtures::session(&key);
        s.effort = Some("xhigh".into());
        overlay(&mut s, 0);
        assert_eq!(s.effort.as_deref(), Some("high"));
        state().sessions.get_mut(&key).unwrap().options = effort("default");
        s.effort = Some("xhigh".into());
        overlay(&mut s, 0);
        assert_eq!(s.effort.as_deref(), Some("xhigh"));
        state().sessions.remove(&key);
    }

    #[test]
    fn reads_the_model_a_session_is_set_to() {
        let options = json!([
            { "id": "mode", "category": "mode", "type": "select", "currentValue": "default", "options": [] },
            { "id": "model", "category": "model", "type": "select", "currentValue": "sonnet", "options": [{ "value": "sonnet", "name": "Sonnet" }, { "value": "haiku", "name": "Haiku" }] },
        ]);
        assert_eq!(current_option(&options, "model").as_deref(), Some("sonnet"));
        assert_eq!(offered_option(&options, "model", "haiku").as_deref(), Some("model"));
        assert_eq!(offered_option(&options, "model", "opus"), None);
        assert_eq!(current_option(&Value::Null, "model"), None);
    }

    #[test]
    fn a_hosted_file_that_does_not_read_is_never_written_over() {
        let dir = store::test_dir("hosted-unreadable");
        let path = dir.join("hosted.json");
        let r = |id: &str| Remembered { harness: Harness::Codex, account: "codex".into(), id: id.into(), cwd: "/r/app".into(), turn: None };
        remember_in(&path, r("t1")).unwrap();
        remember_in(&path, r("t2")).unwrap();
        // One already there isn't written again.
        let before = std::fs::read_to_string(&path).unwrap();
        remember_in(&path, r("t1")).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), before);
        assert_eq!(store::read_or_default::<Vec<Remembered>>(&path), [r("t1"), r("t2")]);

        // A newer build's harness, and a write cut short.
        for bytes in [r#"[{"harness":"hal","account":"hal","id":"t0","cwd":"/r/app"}]"#, r#"[{"harness":"codex","#] {
            std::fs::write(&path, bytes).unwrap();
            let e = remember_in(&path, r("t3")).unwrap_err().to_string();
            assert!(e.starts_with("hosted.json doesn't read as a list of the sessions Codealong runs"), "{e}");
            assert_eq!(std::fs::read_to_string(&path).unwrap(), bytes);
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// hosted.json notes a turn while it runs: a later turn's note replaces it, and only the end
    /// of the turn noted takes it off, so a turn still noted at launch is one the app's end cut
    /// off. A file from before turns were noted reads as it was.
    #[test]
    fn a_turn_is_noted_while_it_runs() {
        let dir = store::test_dir("hosted-turns");
        let path = dir.join("hosted.json");
        let r = |id: &str| Remembered { harness: Harness::Codex, account: "codex".into(), id: id.into(), cwd: "/r/app".into(), turn: None };
        remember_in(&path, r("t1")).unwrap();
        remember_in(&path, r("t2")).unwrap();
        let turns = || store::read_or_default::<Vec<Remembered>>(&path).into_iter().map(|r| r.turn).collect::<Vec<_>>();
        note_turn_in(&path, "codex", "t1", 1_000, true).unwrap();
        assert_eq!(turns(), [Some(1_000), None]);
        note_turn_in(&path, "codex", "t1", 2_000, true).unwrap();
        note_turn_in(&path, "codex", "t1", 1_000, true).unwrap();
        assert_eq!(turns(), [Some(2_000), None], "the turn before, heard late, leaves the next noted");
        note_turn_in(&path, "codex", "t1", 1_000, false).unwrap();
        assert_eq!(turns(), [Some(2_000), None], "and so does its end");
        note_turn_in(&path, "codex", "t1", 2_000, false).unwrap();
        assert_eq!(turns(), [None, None]);
        note_turn_in(&path, "codex", "t9", 3_000, true).unwrap();
        assert_eq!(turns(), [None, None], "a session it doesn't have");
        assert!(!std::fs::read_to_string(&path).unwrap().contains("turn"));
        std::fs::write(&path, r#"[{"harness":"codex","account":"codex","id":"t1","cwd":"/r/app"}]"#).unwrap();
        assert_eq!(store::read_or_default::<Vec<Remembered>>(&path), [r("t1")]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A session whose turn was cut off partway shows so, though its agent's record ended the
    /// turn as stopped: an error until it's seen, quiet after. Not while something else runs it,
    /// nor once its record has a prompt from after, and a turn that begins ends it.
    #[test]
    fn a_turn_cut_off_shows_until_it_goes_on() {
        let key = key_of(Harness::Codex, "test-cut-off", "s1");
        state().sessions.insert(key.clone(), Hosted { account: "test-cut-off".into(), id: "s1".into(), cwd: "/w".into(), cut_off: Some(1_000), ..Default::default() });
        let stopped = Session { live: false, status: Status::Idle, last_prompt_at: 1_500, last_activity: 9_000, ..crate::fixtures::session(&key) };
        let shown = |s: &Session, seen_at: i64| {
            let mut s = s.clone();
            overlay(&mut s, seen_at);
            (s.cut_off, s.status)
        };
        assert_eq!(shown(&stopped, 0), (Some(9_000), Status::Error));
        assert_eq!(shown(&stopped, 9_000), (Some(9_000), Status::Idle));
        assert_eq!(shown(&Session { live: true, ..stopped.clone() }, 0).0, None, "a terminal runs it");
        assert_eq!(shown(&Session { last_prompt_at: 1_000 + CUT_OFF_PROMPT_MS + 1, ..stopped.clone() }, 0), (None, Status::Idle), "it went on elsewhere");
        begin(state().sessions.get_mut(&key).unwrap());
        assert_eq!(state().sessions[&key].cut_off, None);
        // Each turn begins after the last, so its note is its own.
        let later = now_ms() + 60_000;
        state().sessions.get_mut(&key).unwrap().since = later;
        begin(state().sessions.get_mut(&key).unwrap());
        assert_eq!(state().sessions[&key].since, later + 1);
        state().sessions.remove(&key);
    }

    /// A process that ends while a turn's prompt is with it cuts the turn off; one whose prompt
    /// hadn't gone yet just didn't go.
    #[test]
    fn a_process_that_ends_under_a_turn_cuts_it_off() {
        let account = "test-adapter-cut-under";
        let (went, waiting) = (key_of(Harness::Codex, account, "s1"), key_of(Harness::Codex, account, "s2"));
        let sent = Claim::default();
        assert!(sent.send());
        let hosted = |id: &str, turn: Claim| Hosted { account: account.into(), id: id.into(), cwd: "/w".into(), loaded: true, running: true, since: 1_000, turn: Some(turn), ..Default::default() };
        state().sessions.insert(went.clone(), hosted("s1", sent));
        state().sessions.insert(waiting.clone(), hosted("s2", Claim::default()));
        let_go(&mut state(), account);
        let cut = |key: &str| state().sessions.remove(key).unwrap().cut_off;
        assert_eq!((cut(&went), cut(&waiting)), (Some(1_000), None));

        // Its process leaves as the prompt comes: the turn is cut off, whichever hears it first.
        init(|_| {});
        let dir = store::test_dir("acp-cut-off");
        let script = r#"while IFS= read -r line; do printf '%s
' "$line" >> "$1"; case "$line" in *'"method":"session/prompt"'*) exit 0;; esac; done"#;
        let key = key_of(Harness::Codex, account, "s3");
        state().sessions.insert(key.clone(), Hosted { account: account.into(), id: "s3".into(), cwd: "/w".into(), loaded: true, ..Default::default() });
        stand_in(account, script, &dir);
        let a: AccountCfg = serde_json::from_value(json!({ "name": account, "harness": "codex", "home": "/h" })).unwrap();
        assert_eq!(send(&a, &key, "Review it").unwrap(), "Sent.");
        wait_until("the turn to end", 10, || !state().sessions[&key].running);
        assert!(state().sessions[&key].cut_off.is_some());
        state().sessions.remove(&key);
        state().agents.remove(account);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Against a real adapter: `CODEALONG_TEST_ACP_RESUME=<key of a session in hosted.json>
    /// cargo test acp_resumes -- --ignored --nocapture`. Sends to a session this process never
    /// started, as after a restart: it's resumed in a new adapter process first.
    #[test]
    #[ignore]
    fn acp_resumes_a_session() {
        let Ok(key) = std::env::var("CODEALONG_TEST_ACP_RESUME") else { return };
        let cfg = config::current().unwrap();
        init(|_| {});
        let account = state().sessions.get(&key).expect("a remembered session").account.clone();
        let a = cfg.accounts.iter().find(|x| x.name == account).unwrap().clone();
        println!("{}", send(&a, &key, "What was the last thing I asked you? Answer in one short sentence.").unwrap());
        wait_until("the turn to end", 180, || !state().sessions[&key].running);
        assert!(state().sessions[&key].error.is_none(), "{:?}", state().sessions[&key].error);
    }

    /// Against a real adapter: `CODEALONG_TEST_ACP_FORK=<key of a thread of your own>
    /// CODEALONG_TEST_ACP_DIR=<its folder> cargo test acp_forks -- --ignored --nocapture`. Forks
    /// it as /fork does for a Codex thread, and runs the copy on a question only the conversation
    /// it copied answers; its rollout has the answer. The copy is a real thread, remembered in
    /// hosted.json.
    #[test]
    #[ignore]
    fn acp_forks_a_session() {
        let (Ok(key), Ok(dir)) = (std::env::var("CODEALONG_TEST_ACP_FORK"), std::env::var("CODEALONG_TEST_ACP_DIR")) else { return };
        let mut parts = key.splitn(3, ':').skip(1);
        let (account, id) = (parts.next().unwrap(), parts.next().unwrap());
        let cfg = config::current().unwrap();
        let a = cfg.accounts.iter().find(|x| x.name == account).expect("an account by that name").clone();
        init(|_| {});
        let copy = fork(&a, id, Path::new(&dir), "Which word did I ask you to remember? Answer with the word alone, in lowercase.").unwrap();
        println!("{copy}");
        wait_until("the copy's turn to end", 240, || !state().sessions[&copy].running);
        assert!(state().sessions[&copy].error.is_none(), "{:?}", state().sessions[&copy].error);
    }

    /// Against a real adapter: `CODEALONG_TEST_ACP_ADOPT=<key of a session of your own that isn't
    /// running> CODEALONG_TEST_ACP_DIR=<its folder> cargo test acp_takes_on -- --ignored
    /// --nocapture`. Takes it on as Resume in Codealong does, on Haiku, with a question only its
    /// conversation answers, then switches it to Sonnet and high effort as its box does; its
    /// transcript has the answer and both models. It's remembered in hosted.json from then on:
    /// take it out after.
    #[test]
    #[ignore]
    fn acp_takes_on_a_session() {
        let (Ok(key), Ok(dir)) = (std::env::var("CODEALONG_TEST_ACP_ADOPT"), std::env::var("CODEALONG_TEST_ACP_DIR")) else { return };
        let mut parts = key.splitn(3, ':').skip(1);
        let (account, id) = (parts.next().unwrap(), parts.next().unwrap());
        let cfg = config::current().unwrap();
        let a = cfg.accounts.iter().find(|x| x.name == account).expect("an account by that name").clone();
        init(|_| {});
        let hosted = adopt(&a, id, Path::new(&dir), "Which word did I ask you to remember? Answer with the word alone, in lowercase.", Some("haiku"), None).unwrap();
        assert_eq!(hosted, key);
        wait_until("the turn to end", 180, || !state().sessions[&key].running);
        assert!(state().sessions[&key].error.is_none(), "{:?}", state().sessions[&key].error);
        // Between turns it switches to another of the agent's models, and only one it offers.
        switch(&a, &key, Switch::Model("sonnet")).unwrap();
        assert_eq!(current_option(&state().sessions[&key].options, "model").as_deref(), Some("sonnet"));
        assert!(switch(&a, &key, Switch::Model("no-such-model")).unwrap_err().to_string().contains("doesn't offer no-such-model"));
        // And to one of the levels Sonnet takes.
        switch(&a, &key, Switch::Effort("high")).unwrap();
        assert_eq!(current_option(&state().sessions[&key].options, "thought_level").as_deref(), Some("high"));
    }

    /// Against a real adapter: `CODEALONG_TEST_ACP=<account> CODEALONG_TEST_ACP_DIR=<folder>
    /// cargo test acp_runs -- --ignored --nocapture`. Starts a session, has it ask to run a
    /// command in its manual mode, denies it, then sends it a follow-up. The session stays in
    /// the account's records.
    #[test]
    #[ignore]
    fn acp_runs_a_session() {
        let (Ok(name), Ok(dir)) = (std::env::var("CODEALONG_TEST_ACP"), std::env::var("CODEALONG_TEST_ACP_DIR")) else { return };
        let cfg = config::current().unwrap();
        let a = cfg.accounts.iter().find(|x| x.name == name).expect("an account by that name").clone();
        let streamed = Arc::new(Mutex::new(String::new()));
        let s2 = streamed.clone();
        init(move |c| {
            if let Change::Stream(s) = c {
                if !s.text.is_empty() {
                    *s2.lock().unwrap() = s.text;
                }
            }
        });
        let key = start(&a, Path::new(&dir), "", None, None).unwrap();
        println!("session {key}");
        let ag = state().agents.get(&a.name).cloned().unwrap();
        let id = state().sessions.get(&key).unwrap().id.clone();
        // Claude Code calls its asking mode "default"; Codex asks in "read-only".
        let mode = if a.harness == "codex" { "read-only" } else { "default" };
        println!("mode {mode}: {:?}", ag.request("session/set_mode", json!({ "sessionId": id, "modeId": mode }), Some(REQUEST_TIMEOUT)));
        let _ = std::fs::remove_file(Path::new(&dir).join("acp-denied.txt"));
        send(&a, &key, "Run exactly this shell command and nothing else: touch acp-denied.txt. Then say done.").unwrap();
        wait_until("a permission request", 120, || pending(&key).is_some() || !state().sessions[&key].running);
        // Codex's read-only mode refuses the command by itself rather than asking.
        match pending(&key) {
            Some(p) => {
                println!("asked: {} / {:?}", p.label, p.input);
                println!("{}", answer(&key, &p.id, Answer::Deny).unwrap());
            }
            None => println!("it didn't ask"),
        }
        wait_until("the turn to end", 180, || !state().sessions[&key].running);
        println!("file made: {}", Path::new(&dir).join("acp-denied.txt").exists());
        send(&a, &key, "Reply with just the word pong.").unwrap();
        wait_until("the second turn to end", 180, || !state().sessions[&key].running);
        println!("streamed: {:?}", streamed.lock().unwrap());
        assert!(state().sessions[&key].error.is_none());
    }

    /// What an adapter is written, line for line, by the handshake, by answering and cancelling
    /// what it asks, and by stopping a turn; and what its end does to its sessions. Against a
    /// stand-in that keeps each line it reads in a file.
    #[test]
    fn an_adapter_is_written_the_protocols_lines() {
        let dir = store::test_dir("acp-lines");
        let heard = dir.join("heard");
        let ask = |id: &str, session: &str| format!(r#"echo '{{"jsonrpc":"2.0","id":"{id}","method":"session/request_permission","params":{{"sessionId":"{session}","toolCall":{{"toolCallId":"call-{id}","title":"Run   ls"}},"options":[{{"optionId":"ok","kind":"allow_once"}},{{"optionId":"no","kind":"reject_once"}}]}}}}'"#);
        let script = [
            r#"while IFS= read -r line; do printf '%s\n' "$line" >> "$1"; case "$line" in"#.to_string(),
            // Once greeted it asks about a session of the account's, about one that's nobody's,
            // and for something Codealong doesn't lend.
            format!(r#"*'"method":"initialize"'*) echo '{{"jsonrpc":"2.0","id":1,"result":{{"agentCapabilities":{{"loadSession":true}}}}}}'; {}; {}; echo '{{"jsonrpc":"2.0","id":9,"method":"fs/read_text_file","params":{{}}}}';;"#, ask("p1", "s1"), ask("p2", "nobody")),
            // Allowed, it asks again; told to stop, it leaves.
            format!(r#"*'"optionId"'*) {};;"#, ask("p3", "s1")),
            r#"*'"method":"session/cancel"'*) exit 0;; esac; done"#.to_string(),
        ]
        .join(" ");
        let mut cmd = Command::new("/bin/sh");
        cmd.args(["-c", &script, "sh"]).arg(&heard);
        let account = "test-adapter-lines";
        let key = key_of(Harness::Codex, account, "s1");
        state().sessions.insert(key.clone(), Hosted { account: account.into(), id: "s1".into(), cwd: "/w".into(), loaded: true, running: true, ..Default::default() });
        let ag = connect(account, cmd, std::fs::File::create(dir.join("log")).unwrap()).unwrap();
        state().agents.insert(account.into(), ag.clone());
        let wrote = || std::fs::read_to_string(&heard).unwrap_or_default().lines().map(String::from).collect::<Vec<_>>();

        assert_eq!(ag.initialize().unwrap(), json!({ "agentCapabilities": { "loadSession": true } }));
        wait_until("its questions to be heard", 10, || pending(&key).is_some() && wrote().len() == 3);
        let p = pending(&key).unwrap();
        assert_eq!((p.id.as_str(), p.label.as_str(), p.always), ("call-p1", "Run ls", false));
        assert_eq!(answer(&key, "call-p1", Answer::Allow).unwrap(), "Allowed.");
        wait_until("it to ask again", 10, || pending(&key).is_some_and(|p| p.id == "call-p3"));
        cancel(&key).unwrap();
        wait_until("it to leave", 10, || !state().agents.contains_key(account));

        assert_eq!(
            wrote(),
            [
                format!(r#"{{"id":1,"jsonrpc":"2.0","method":"initialize","params":{{"clientCapabilities":{{"fs":{{"readTextFile":false,"writeTextFile":false}},"terminal":false}},"clientInfo":{{"name":"codealong","title":"Codealong","version":"{}"}},"protocolVersion":1}}}}"#, env!("CARGO_PKG_VERSION")),
                r#"{"id":"p2","jsonrpc":"2.0","result":{"outcome":{"outcome":"cancelled"}}}"#.to_string(),
                r#"{"error":{"code":-32601,"message":"Codealong doesn't offer that."},"id":9,"jsonrpc":"2.0"}"#.to_string(),
                r#"{"id":"p1","jsonrpc":"2.0","result":{"outcome":{"optionId":"ok","outcome":"selected"}}}"#.to_string(),
                r#"{"id":"p3","jsonrpc":"2.0","result":{"outcome":{"outcome":"cancelled"}}}"#.to_string(),
                r#"{"jsonrpc":"2.0","method":"session/cancel","params":{"sessionId":"s1"}}"#.to_string(),
            ]
        );
        // Its turn ended with it, and what's asked of it now says so.
        let h = state().sessions.remove(&key).unwrap();
        assert_eq!((h.loaded, h.running, h.error.as_deref()), (false, false, Some("Its agent stopped.")));
        assert_eq!(ag.request("session/new", json!({}), None).unwrap_err().to_string(), "test-adapter-lines's agent stopped.");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A stand-in adapter, made `account`'s process: `script` runs in sh with the file to keep
    /// each line it reads in as `$1`, `heard` in `dir`.
    fn stand_in(account: &str, script: &str, dir: &Path) -> Arc<Agent> {
        let mut cmd = Command::new("/bin/sh");
        cmd.args(["-c", script, "sh"]).arg(dir.join("heard"));
        let ag = connect(account, cmd, std::fs::File::create(dir.join("log")).unwrap()).unwrap();
        state().agents.insert(account.into(), ag.clone());
        ag
    }

    /// The lines a stand-in has read.
    fn heard(dir: &Path) -> Vec<String> {
        std::fs::read_to_string(dir.join("heard")).unwrap_or_default().lines().map(String::from).collect()
    }

    /// A permission request as a stand-in asks it of session s1, with `options` as JSON.
    fn asks(id: &str, options: &str) -> String {
        format!(r#"echo '{{"jsonrpc":"2.0","id":"{id}","method":"session/request_permission","params":{{"sessionId":"s1","toolCall":{{"toolCallId":"call-{id}","title":"Run ls"}},"options":[{options}]}}}}'"#)
    }

    /// Allow is only ever the agent's answer that allows a call once, and Deny the one that refuses
    /// it once: an agent offering to allow calls like it from now on, and not once, isn't allowed
    /// them for good by an Allow. What can't be answered the way asked waits on, answered by none.
    #[test]
    fn an_allow_is_never_taken_for_good() {
        let dir = store::test_dir("acp-once");
        let script = [
            r#"while IFS= read -r line; do printf '%s\n' "$line" >> "$1"; case "$line" in"#.to_string(),
            // Once greeted it asks, offering to allow from now on but not once.
            format!(r#"*'"method":"initialize"'*) echo '{{"jsonrpc":"2.0","id":1,"result":{{}}}}'; {};;"#, asks("p1", r#"{"optionId":"always","kind":"allow_always"},{"optionId":"no","kind":"reject_once"}"#)),
            // Answered, it asks again, offering to refuse from now on but not once.
            format!(r#"*'"id":"p1"'*) {};; esac; done"#, asks("p2", r#"{"optionId":"ok","kind":"allow_once"},{"optionId":"never","kind":"reject_always"}"#)),
        ]
        .join(" ");
        let account = "test-adapter-once";
        let key = key_of(Harness::Claude, account, "s1");
        state().sessions.insert(key.clone(), Hosted { account: account.into(), id: "s1".into(), cwd: "/w".into(), loaded: true, running: true, ..Default::default() });
        let ag = stand_in(account, &script, &dir);
        ag.initialize().unwrap();
        wait_until("it to ask", 10, || pending(&key).is_some());
        assert_eq!(pending(&key).map(|p| (p.always, p.no_once)), Some((true, true)));
        let e = answer(&key, "call-p1", Answer::Allow).unwrap_err().to_string();
        assert!(e.contains("no way to allow this just once"), "{e}");
        // Nothing was written, and it can still be answered.
        assert!(pending(&key).is_some_and(|p| p.id == "call-p1"));
        assert_eq!(answer(&key, "call-p1", Answer::Deny).unwrap(), "Denied.");
        wait_until("it to ask again", 10, || pending(&key).is_some_and(|p| p.id == "call-p2"));
        assert_eq!(pending(&key).map(|p| (p.always, p.no_once)), Some((false, false)));
        let e = answer(&key, "call-p2", Answer::Deny).unwrap_err().to_string();
        assert!(e.contains("no way to deny this just once"), "{e}");
        assert_eq!(answer(&key, "call-p2", Answer::Allow).unwrap(), "Allowed.");
        assert!(pending(&key).is_none());
        wait_until("both answers to be read", 10, || heard(&dir).len() == 3);
        assert_eq!(
            heard(&dir)[1..],
            [r#"{"id":"p1","jsonrpc":"2.0","result":{"outcome":{"optionId":"no","outcome":"selected"}}}"#, r#"{"id":"p2","jsonrpc":"2.0","result":{"outcome":{"optionId":"ok","outcome":"selected"}}}"#]
        );
        state().sessions.remove(&key);
        state().agents.remove(account);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The answers claude-agent-acp 0.81.2 offers for Claude Code's `ExitPlanMode`, as its
    /// `buildExitPlanModePermissionOptions` makes them and sorts them by kind: here for a session
    /// that offers auto and bypass modes and wasn't bypassing permissions before it planned.
    const PLAN_OPTIONS: &str = r#"[{"optionId":"exit-plan-default","name":"Yes, manually approve edits","kind":"allow_once"},{"optionId":"exit-plan-clear-auto","name":"Yes, clear context (12% used) and use auto mode","kind":"allow_always"},{"optionId":"exit-plan-auto","name":"Yes, and use auto mode","kind":"allow_always"},{"optionId":"exit-plan-bypass","name":"Yes, and bypass permissions","kind":"allow_always"},{"optionId":"reject","name":"No, keep planning","kind":"reject_once"}]"#;

    /// A plan's request as claude-agent-acp 0.81.2 sends it, its id `id`.
    fn plan_request(id: &str) -> String {
        let plan = r"# Fix the build\n\n1. Pin the toolchain";
        format!(r#"{{"jsonrpc":"2.0","id":"{id}","method":"session/request_permission","params":{{"toolCall":{{"toolCallId":"toolu_{id}","name":"ExitPlanMode","status":"pending","rawInput":{{"plan":"{plan}"}},"title":"Approve Plan","kind":"switch_mode","content":[{{"type":"content","content":{{"type":"text","text":"{plan}"}}}}],"locations":[]}},"_meta":{{"permission":{{"version":1,"title":"Ready to code?"}}}},"options":{PLAN_OPTIONS},"sessionId":"s1"}}}}"#)
    }

    /// Each answer to a plan picks the option that does what the windows say it does, from the
    /// lists claude-agent-acp and codex-acp really send, and never the one that clears the
    /// conversation: Allow approves it and goes on asking, Always goes on asking less, both with
    /// the conversation, and Deny keeps it planning.
    #[test]
    fn a_plans_answers_keep_its_conversation() {
        let permission = |plan: bool, options: &str| Permission { rpc: json!(1), tool_call: "t".into(), label: PLAN.into(), input: None, options: offered(&json!({ "options": serde_json::from_str::<Value>(options).unwrap() })), plan, answering: false, asked_at: 0 };
        let picks = |p: &Permission| [Answer::Allow, Answer::Always, Answer::Deny].map(|a| pick(p, a).map(|o| o.id.clone()).map_err(String::from));
        let ok = |a: &str, b: &str, c: &str| [Ok(a.to_string()), Ok(b.to_string()), Ok(c.to_string())];
        assert_eq!(picks(&permission(true, PLAN_OPTIONS)), ok("exit-plan-default", "exit-plan-auto", "reject"));
        // A session that bypassed permissions before it planned is offered that first.
        let bypassed = r#"[{"optionId":"exit-plan-default","name":"Yes, manually approve edits","kind":"allow_once"},{"optionId":"exit-plan-clear-bypass","name":"Yes, clear context and bypass permissions","kind":"allow_always"},{"optionId":"exit-plan-bypass","name":"Yes, and bypass permissions","kind":"allow_always"},{"optionId":"exit-plan-auto","name":"Yes, and use auto mode","kind":"allow_always"},{"optionId":"reject","name":"No, keep planning","kind":"reject_once"}]"#;
        assert_eq!(picks(&permission(true, bypassed)), ok("exit-plan-default", "exit-plan-bypass", "reject"));
        // One offering neither mode goes on accepting edits.
        let neither = r#"[{"optionId":"exit-plan-default","name":"Yes, manually approve edits","kind":"allow_once"},{"optionId":"exit-plan-clear-accept-edits","name":"Yes, clear context and auto-accept edits","kind":"allow_always"},{"optionId":"exit-plan-accept-edits","name":"Yes, auto-accept edits","kind":"allow_always"},{"optionId":"reject","name":"No, keep planning","kind":"reject_once"}]"#;
        assert_eq!(picks(&permission(true, neither)), ok("exit-plan-default", "exit-plan-accept-edits", "reject"));
        // Only clearing the conversation from now on is no Always at all.
        let p = permission(true, r#"[{"optionId":"exit-plan-default","name":"Yes, manually approve edits","kind":"allow_once"},{"optionId":"exit-plan-clear-auto","name":"Yes, clear context and use auto mode","kind":"allow_always"}]"#);
        assert_eq!(picks(&p)[1], Err("Its agent offers no way to allow this from now on.".to_string()));
        let shown = pending_of(&p);
        assert_eq!((shown.always, shown.always_rule, shown.no_once), (false, None, false));
        // codex-acp's plan to implement.
        let codex = r#"[{"optionId":"implement_plan","name":"Yes, implement this plan","kind":"allow_once"},{"optionId":"revise_plan","name":"No, and tell Codex what to do differently","kind":"reject_once"}]"#;
        assert_eq!(picks(&permission(true, codex)), [Ok("implement_plan".to_string()), Err("Its agent offers no way to allow this from now on.".to_string()), Ok("revise_plan".to_string())]);
        // A call that isn't a plan isn't read for what a plan's answers say.
        let call = r#"[{"optionId":"allow-once","name":"Yes","kind":"allow_once"},{"optionId":"allow-with-updates","name":"Yes, and don't ask again for clear commands","kind":"allow_always"}]"#;
        assert_eq!(picks(&permission(false, call))[1], Ok("allow-with-updates".to_string()));
    }

    /// A plan claude-agent-acp asks to approve reads as a plan, as a terminal session's does, and
    /// each answer writes the option `pick` chose.
    #[test]
    fn a_plan_is_asked_and_answered_as_one() {
        let dir = store::test_dir("acp-plan");
        for id in ["p1", "p2", "p3"] {
            std::fs::write(dir.join(format!("{id}.json")), format!("{}\n", plan_request(id))).unwrap();
        }
        let d = dir.to_string_lossy();
        let script = [
            r#"while IFS= read -r line; do printf '%s\n' "$line" >> "$1"; case "$line" in"#.to_string(),
            format!(r#"*'"method":"initialize"'*) echo '{{"jsonrpc":"2.0","id":1,"result":{{}}}}'; cat '{d}/p1.json';;"#),
            format!(r#"*'"id":"p1"'*) cat '{d}/p2.json';; *'"id":"p2"'*) cat '{d}/p3.json';; esac; done"#),
        ]
        .join(" ");
        let account = "test-adapter-plan";
        let key = key_of(Harness::Claude, account, "s1");
        state().sessions.insert(key.clone(), Hosted { account: account.into(), id: "s1".into(), cwd: "/w".into(), loaded: true, running: true, ..Default::default() });
        let ag = stand_in(account, &script, &dir);
        ag.initialize().unwrap();
        wait_until("it to ask", 10, || pending(&key).is_some());
        let p = pending(&key).unwrap();
        assert_eq!((p.label.as_str(), p.input.as_deref(), p.plan, p.always, p.no_once), ("Plan", Some("# Fix the build\n\n1. Pin the toolchain"), true, true, false));
        assert_eq!(p.always_rule.as_deref(), Some("Approves the plan and keeps the conversation, as its agent's “Yes, and use auto mode”."));
        assert_eq!(answer(&key, "toolu_p1", Answer::Always).unwrap(), "Approved the plan: “Yes, and use auto mode”.");
        wait_until("it to ask again", 10, || pending(&key).is_some_and(|p| p.id == "toolu_p2"));
        assert_eq!(answer(&key, "toolu_p2", Answer::Allow).unwrap(), "Approved the plan: “Yes, manually approve edits”.");
        wait_until("it to ask a third time", 10, || pending(&key).is_some_and(|p| p.id == "toolu_p3"));
        assert_eq!(answer(&key, "toolu_p3", Answer::Deny).unwrap(), "Denied the plan: “No, keep planning”.");
        wait_until("the answers to be read", 10, || heard(&dir).len() == 4);
        let chosen: Vec<String> = heard(&dir)[1..].iter().map(|l| serde_json::from_str::<Value>(l).unwrap()["result"]["outcome"]["optionId"].as_str().unwrap().to_string()).collect();
        assert_eq!(chosen, ["exit-plan-auto", "exit-plan-default", "reject"]);
        state().sessions.remove(&key);
        state().agents.remove(account);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Two starts at once each start the account a process, and the one that's second is let go
    /// (`agent`): what it says and asks as it goes, and its end, leave the sessions of the one
    /// that stayed as they are; the end of the account's own process is theirs.
    #[test]
    fn a_process_let_go_leaves_the_accounts_sessions_alone() {
        let dir = store::test_dir("acp-let-go");
        let (stayed, let_go) = (dir.join("stayed"), dir.join("let-go"));
        std::fs::create_dir_all(&stayed).unwrap();
        std::fs::create_dir_all(&let_go).unwrap();
        let keeps = r#"while IFS= read -r line; do printf '%s\n' "$line" >> "$1"; done"#;
        let account = "test-adapter-let-go";
        let key = key_of(Harness::Codex, account, "s1");
        let asked = Permission { rpc: json!("w1"), tool_call: "call-w1".into(), label: "Run ls".into(), input: None, options: vec![Offered { id: "ok".into(), kind: "allow_once".into(), name: "Yes".into() }], plan: false, answering: false, asked_at: 0 };
        state().sessions.insert(key.clone(), Hosted { account: account.into(), id: "s1".into(), cwd: "/w".into(), loaded: true, running: true, permission: Some(asked), queue: vec!["And then?".into()], text: "Working".into(), ..Default::default() });
        let ag = stand_in(account, keeps, &stayed);
        let mut cmd = Command::new("/bin/sh");
        cmd.args(["-c", keeps, "sh"]).arg(let_go.join("heard"));
        let other = connect(account, cmd, std::fs::File::create(let_go.join("log")).unwrap()).unwrap();

        hear(account, &other.client, &json!({ "jsonrpc": "2.0", "method": "session/update", "params": { "sessionId": "s1", "update": { "sessionUpdate": "agent_message_chunk", "content": { "type": "text", "text": " on it" } } } }));
        hear(account, &other.client, &json!({ "jsonrpc": "2.0", "id": "l1", "method": "session/request_permission", "params": { "sessionId": "s1", "toolCall": { "toolCallId": "call-l1", "title": "Run rm" }, "options": [{ "optionId": "ok", "kind": "allow_once" }] } }));
        stopped(account, &other.client);
        let h = state().sessions[&key].clone();
        assert_eq!((h.loaded, h.running, h.text.as_str(), h.queue.len(), h.error), (true, true, "Working", 1, None));
        assert_eq!(h.permission.map(|p| p.tool_call).as_deref(), Some("call-w1"));
        assert!(state().agents.get(account).is_some_and(|a| Arc::ptr_eq(a, &ag)));
        // What it asked is answered as asked of nobody's session, and only to it.
        wait_until("its question to be answered", 10, || !heard(&let_go).is_empty());
        assert_eq!(heard(&let_go), [r#"{"id":"l1","jsonrpc":"2.0","result":{"outcome":{"outcome":"cancelled"}}}"#]);
        assert!(heard(&stayed).is_empty());

        stopped(account, &ag.client);
        let h = state().sessions.remove(&key).unwrap();
        assert_eq!((h.loaded, h.running, h.queue.len(), h.permission.is_none(), h.error.as_deref()), (false, false, 0, true, Some("Its agent stopped.")));
        assert!(!state().agents.contains_key(account));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// An account's process that stops between answering the handshake and being kept isn't kept,
    /// and one kept that has stopped, its end not heard yet, is let go at the next use: either way
    /// the account's sessions get a process that runs, rather than one that's gone until the app
    /// restarts.
    #[test]
    fn a_process_that_stops_as_it_starts_isnt_kept() {
        let dir = store::test_dir("acp-stops-starting");
        let account = "test-adapter-stops-starting";
        // It answers the handshake and leaves.
        let mut cmd = Command::new("/bin/sh");
        cmd.args(["-c", r#"read -r line; echo '{"jsonrpc":"2.0","id":1,"result":{}}'"#]);
        let gone = connect(account, cmd, std::fs::File::create(dir.join("log")).unwrap()).unwrap();
        gone.initialize().unwrap();
        wait_until("it to leave", 10, || gone.client.is_closed());
        assert_eq!(keep_agent(account, gone.clone()).err().map(|e| e.to_string()).as_deref(), Some("test-adapter-stops-starting's agent stopped as it started."));
        assert!(running(account).is_none());

        // One that was kept and has stopped since, with a session's turn running in it.
        let key = key_of(Harness::Codex, account, "s1");
        state().sessions.insert(key.clone(), Hosted { account: account.into(), id: "s1".into(), cwd: "/w".into(), loaded: true, running: true, queue: vec!["And then?".into()], ..Default::default() });
        state().agents.insert(account.into(), gone.clone());
        assert!(running(account).is_none());
        assert!(!state().agents.contains_key(account));
        let h = state().sessions[&key].clone();
        assert_eq!((h.loaded, h.running, h.queue.len(), h.error.as_deref()), (false, false, 0, Some("Its agent stopped.")));
        // The next one is kept, and the old one's end, heard late, leaves it alone.
        state().agents.insert(account.into(), gone.clone());
        let keeps = r#"while IFS= read -r line; do printf '%s\n' "$line" >> "$1"; done"#;
        let mut cmd = Command::new("/bin/sh");
        cmd.args(["-c", keeps, "sh"]).arg(dir.join("heard"));
        let next = connect(account, cmd, std::fs::File::create(dir.join("log2")).unwrap()).unwrap();
        assert!(Arc::ptr_eq(&keep_agent(account, next.clone()).unwrap(), &next));
        stopped(account, &gone.client);
        assert!(running(account).is_some_and(|a| Arc::ptr_eq(&a, &next)));
        state().sessions.remove(&key);
        state().agents.remove(account);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Two sends at once to a session Codealong runs start one turn and queue the other, while
    /// the session is still being picked up in the process too: it's resumed once, then given
    /// the prompts one after the other.
    #[test]
    fn two_sends_at_once_start_one_turn() {
        init(|_| {});
        let dir = store::test_dir("acp-two-sends");
        let script = [
            r#"while IFS= read -r line; do printf '%s\n' "$line" >> "$1"; id=$(printf '%s' "$line" | sed -n 's/^{"id":\([0-9]*\),.*/\1/p'); case "$line" in"#,
            // Picking a session up takes a while, as it does a long conversation.
            r#"*'"method":"session/resume"'*) sleep 1; echo "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{}}";;"#,
            r#"*'"method":"session/prompt"'*) echo "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{\"stopReason\":\"end_turn\"}}";; esac; done"#,
        ]
        .join(" ");
        let account = "test-adapter-two-sends";
        let key = key_of(Harness::Claude, account, "s1");
        state().sessions.insert(key.clone(), Hosted { account: account.into(), id: "s1".into(), cwd: "/w".into(), ..Default::default() });
        let ag = stand_in(account, &script, &dir);
        *ag.caps.lock().unwrap() = json!({ "sessionCapabilities": { "resume": {} } });
        let a: AccountCfg = serde_json::from_value(json!({ "name": account, "harness": "claude", "home": "/h" })).unwrap();
        let first = {
            let (a, key) = (a.clone(), key.clone());
            std::thread::spawn(move || send(&a, &key, "First"))
        };
        wait_until("the session to be picked up", 10, || heard(&dir).iter().any(|l| l.contains(r#""method":"session/resume""#)));
        let asked = std::time::Instant::now();
        assert_eq!(send(&a, &key, "Second").unwrap(), "Queued: it goes after the current turn.");
        assert!(asked.elapsed() < Duration::from_millis(800), "the second send waited on the first's picking up");
        assert_eq!(first.join().unwrap().unwrap(), "Sent.");
        wait_until("both turns to end", 10, || !state().sessions[&key].running);
        let said: Vec<String> = heard(&dir)
            .iter()
            .map(|l| {
                let v: Value = serde_json::from_str(l).unwrap();
                format!("{} {}", v["method"].as_str().unwrap_or(""), v.pointer("/params/prompt/0/text").and_then(Value::as_str).unwrap_or("")).trim().to_string()
            })
            .collect();
        assert_eq!(said, ["session/resume", "session/prompt First", "session/prompt Second"]);
        assert_eq!(state().sessions[&key].error, None);
        state().sessions.remove(&key);
        state().agents.remove(account);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A session its account's process holds is deleted there, where the agent offers that, and
    /// picked up again if it's sent to after; one the process doesn't hold, or whose agent has no
    /// delete, is left to its harness with nothing sent, and one taking a turn is refused.
    #[test]
    fn a_held_session_is_deleted_by_its_process() {
        init(|_| {});
        let dir = store::test_dir("acp-delete");
        let script = r#"while IFS= read -r line; do printf '%s\n' "$line" >> "$1"; id=$(printf '%s' "$line" | sed -n 's/^{"id":\([0-9]*\),.*/\1/p'); case "$line" in *'"method":"session/delete"'*) echo "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{}}";; esac; done"#;
        let account = "test-adapter-delete";
        let key = key_of(Harness::Codex, account, "s1");
        let ag = stand_in(account, script, &dir);
        *ag.caps.lock().unwrap() = json!({ "sessionCapabilities": { "delete": {} } });

        assert!(!delete_held(account, Harness::Codex, "s1").unwrap(), "not Codealong's");
        state().sessions.insert(key.clone(), Hosted { account: account.into(), id: "s1".into(), cwd: "/w".into(), ..Default::default() });
        assert!(!delete_held(account, Harness::Codex, "s1").unwrap(), "not picked up in this process");
        set(&key, |h| (h.loaded, h.held, h.running) = (true, true, true));
        assert_eq!(delete_held(account, Harness::Codex, "s1").unwrap_err().to_string(), "It's taking a turn in Codealong: stop it first.");
        *ag.caps.lock().unwrap() = json!({ "sessionCapabilities": {} });
        set(&key, |h| h.running = false);
        assert!(!delete_held(account, Harness::Codex, "s1").unwrap(), "its agent has no delete");
        assert!(heard(&dir).is_empty());

        *ag.caps.lock().unwrap() = json!({ "sessionCapabilities": { "delete": {} } });
        assert!(delete_held(account, Harness::Codex, "s1").unwrap());
        assert_eq!(heard(&dir), [r#"{"id":1,"jsonrpc":"2.0","method":"session/delete","params":{"sessionId":"s1"}}"#]);
        assert!(!state().sessions[&key].loaded && !state().sessions[&key].held);
        // Deleted, it's the harness's to archive again.
        assert!(!delete_held(account, Harness::Codex, "s1").unwrap());
        state().sessions.remove(&key);
        state().agents.remove(account);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A send while a session is being deleted in its process is refused, not written to it, as
    /// is a second delete; a delete that fails leaves it to be picked up again, as the adapter may
    /// have closed it, and the next delete goes to the process again, which may still hold it.
    #[test]
    fn a_session_being_deleted_takes_no_send_and_a_failed_delete_lets_it_go() {
        init(|_| {});
        let dir = store::test_dir("acp-delete-send");
        // It takes a while over the delete, then fails it.
        let script = r#"while IFS= read -r line; do printf '%s\n' "$line" >> "$1"; id=$(printf '%s' "$line" | sed -n 's/^{"id":\([0-9]*\),.*/\1/p'); case "$line" in *'"method":"session/delete"'*) sleep 1; echo "{\"jsonrpc\":\"2.0\",\"id\":$id,\"error\":{\"code\":-32603,\"message\":\"archive failed\"}}";; esac; done"#;
        let account = "test-adapter-delete-send";
        let key = key_of(Harness::Codex, account, "s1");
        let ag = stand_in(account, script, &dir);
        *ag.caps.lock().unwrap() = json!({ "sessionCapabilities": { "delete": {} } });
        state().sessions.insert(key.clone(), Hosted { account: account.into(), id: "s1".into(), cwd: "/w".into(), loaded: true, held: true, ..Default::default() });
        let deleting = std::thread::spawn(move || delete_held(account, Harness::Codex, "s1"));
        wait_until("the delete to be sent", 10, || heard(&dir).len() == 1);
        let a: AccountCfg = serde_json::from_value(json!({ "name": account, "harness": "codex", "home": "/h" })).unwrap();
        assert_eq!(send(&a, &key, "Go on").unwrap_err().to_string(), "It's being archived.");
        assert_eq!(delete_held(account, Harness::Codex, "s1").unwrap_err().to_string(), "It's being archived.");
        assert!(deleting.join().unwrap().unwrap_err().to_string().contains("archive failed"));
        let h = state().sessions[&key].clone();
        assert_eq!((h.loaded, h.held, h.deleting, h.running), (false, true, false, false));
        assert_eq!(heard(&dir).len(), 1, "nothing but the delete was written");
        // Tried again, it goes to the process, which may still hold the session.
        assert!(delete_held(account, Harness::Codex, "s1").unwrap_err().to_string().contains("archive failed"));
        assert_eq!(heard(&dir).len(), 2);
        state().sessions.remove(&key);
        state().agents.remove(account);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Stop while a session is still being picked up in the process ends the turn there, and
    /// its prompt never goes, nor anything sent meanwhile; the next send goes as usual.
    #[test]
    fn a_stop_while_picking_up_sends_nothing() {
        init(|_| {});
        let dir = store::test_dir("acp-stop-picking-up");
        let script = [
            r#"while IFS= read -r line; do printf '%s\n' "$line" >> "$1"; id=$(printf '%s' "$line" | sed -n 's/^{"id":\([0-9]*\),.*/\1/p'); case "$line" in"#,
            r#"*'"method":"session/resume"'*) sleep 1; echo "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{}}";;"#,
            r#"*'"method":"session/prompt"'*) echo "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{\"stopReason\":\"end_turn\"}}";; esac; done"#,
        ]
        .join(" ");
        let account = "test-adapter-stop-picking-up";
        let key = key_of(Harness::Claude, account, "s1");
        state().sessions.insert(key.clone(), Hosted { account: account.into(), id: "s1".into(), cwd: "/w".into(), ..Default::default() });
        let ag = stand_in(account, &script, &dir);
        *ag.caps.lock().unwrap() = json!({ "sessionCapabilities": { "resume": {} } });
        let a: AccountCfg = serde_json::from_value(json!({ "name": account, "harness": "claude", "home": "/h" })).unwrap();
        let first = {
            let (a, key) = (a.clone(), key.clone());
            std::thread::spawn(move || send(&a, &key, "First"))
        };
        wait_until("the session to be picked up", 10, || heard(&dir).iter().any(|l| l.contains(r#""method":"session/resume""#)));
        assert_eq!(send(&a, &key, "Second").unwrap(), "Queued: it goes after the current turn.");
        cancel(&key).unwrap();
        assert!(!state().sessions[&key].running);
        assert_eq!(first.join().unwrap().unwrap_err().to_string(), "Stopped before it was sent.");
        // Long enough for a prompt to be written, had one gone.
        std::thread::sleep(Duration::from_millis(500));
        let said = |dir: &Path| heard(dir).iter().map(|l| serde_json::from_str::<Value>(l).unwrap()).map(|v| format!("{} {}", v["method"].as_str().unwrap_or(""), v.pointer("/params/prompt/0/text").and_then(Value::as_str).unwrap_or("")).trim().to_string()).collect::<Vec<_>>();
        assert_eq!(said(&dir), ["session/resume"]);
        let h = state().sessions[&key].clone();
        assert_eq!((h.loaded, h.running, h.queue.len(), h.error), (true, false, 0, None));
        assert_eq!(send(&a, &key, "Again").unwrap(), "Sent.");
        wait_until("its turn to end", 10, || !state().sessions[&key].running);
        assert_eq!(said(&dir), ["session/resume", "session/prompt Again"]);
        state().sessions.remove(&key);
        state().agents.remove(account);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A turn that fails takes what was sent while it ran with it, and its error says so: none of
    /// it goes, then or behind the next send.
    #[test]
    fn a_failed_turn_drops_its_queue_and_says_so() {
        init(|_| {});
        let dir = store::test_dir("acp-failed-turn");
        let script = [
            r#"while IFS= read -r line; do printf '%s\n' "$line" >> "$1"; id=$(printf '%s' "$line" | sed -n 's/^{"id":\([0-9]*\),.*/\1/p'); case "$line" in"#,
            // The first prompt fails a moment later, as an agent's overloaded API might.
            r#"*'"text":"First"'*) sleep 1; echo "{\"jsonrpc\":\"2.0\",\"id\":$id,\"error\":{\"code\":-32603,\"message\":\"Overloaded.\"}}";;"#,
            r#"*'"method":"session/prompt"'*) echo "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{\"stopReason\":\"end_turn\"}}";; esac; done"#,
        ]
        .join(" ");
        let account = "test-adapter-failed-turn";
        let key = key_of(Harness::Claude, account, "s1");
        state().sessions.insert(key.clone(), Hosted { account: account.into(), id: "s1".into(), cwd: "/w".into(), loaded: true, ..Default::default() });
        stand_in(account, &script, &dir);
        let a: AccountCfg = serde_json::from_value(json!({ "name": account, "harness": "claude", "home": "/h" })).unwrap();
        assert_eq!(send(&a, &key, "First").unwrap(), "Sent.");
        assert_eq!(send(&a, &key, "Second").unwrap(), "Queued: it goes after the current turn.");
        wait_until("the turn to fail", 10, || !state().sessions[&key].running);
        let h = state().sessions[&key].clone();
        assert_eq!(h.error.as_deref(), Some("Overloaded. What was sent while it ran wasn't sent: send it again if it's still wanted."));
        assert!(h.queue.is_empty());
        assert_eq!(send(&a, &key, "Third").unwrap(), "Sent.");
        wait_until("the next turn to end", 10, || !state().sessions[&key].running);
        let prompts: Vec<String> = heard(&dir).iter().filter_map(|l| serde_json::from_str::<Value>(l).unwrap().pointer("/params/prompt/0/text").and_then(Value::as_str).map(String::from)).collect();
        assert_eq!(prompts, ["First", "Third"]);
        assert_eq!(state().sessions[&key].error, None);
        assert_eq!(state().sessions[&key].cut_off, None, "a turn that failed wasn't cut off");
        state().sessions.remove(&key);
        state().agents.remove(account);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_install_that_hangs_is_stopped_and_says_so() {
        let dir = store::test_dir("acp-npm");
        let failed = |npm: &str, args: &[&str], timeout: Duration| run_npm(npm, args, &dir, timeout).err().expect("it fails").to_string();
        let e = failed("/bin/sleep", &["30"], Duration::from_millis(300));
        assert!(e.starts_with("npm timed out after "), "{e}");
        // No npm at all still says what to install.
        let e = failed("/nowhere/npm", &["install"], Duration::from_secs(5));
        assert!(e.starts_with("npm isn't installed: the adapters are Node packages."), "{e}");
        // And one that ran is read as before, whatever it exited with.
        let Ok(out) = run_npm("/bin/sh", &["-c", "echo 'npm ERR! 404' >&2; exit 1"], &dir, Duration::from_secs(5)) else { panic!("it ran") };
        assert_eq!((out.ok, out.err.lines().last()), (false, Some("npm ERR! 404")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sets_only_the_options_a_session_offers() {
        // As claude-agent-acp lists them once a model that takes effort is chosen.
        let options = json!([
            { "id": "model", "category": "model", "options": [{ "value": "default" }, { "value": "opus" }] },
            { "id": "effort", "category": "thought_level", "options": [{ "value": "low" }, { "value": "xhigh" }] },
        ]);
        assert_eq!(offered_option(&options, "model", "opus").as_deref(), Some("model"));
        assert_eq!(offered_option(&options, "thought_level", "xhigh").as_deref(), Some("effort"));
        assert_eq!(offered_option(&options, "thought_level", "max"), None);
        assert_eq!(offered_option(&Value::Null, "model", "opus"), None);
    }

    #[test]
    fn keys_read_like_the_boards() {
        assert_eq!(key_of(Harness::Claude, "claude2", "abc"), "claude:claude2:abc");
        assert_eq!(key_of(Harness::Opencode, "opencode", "ses_1"), "opencode:opencode:ses_1");
    }

    #[test]
    fn an_account_setting_picks_the_adapter() {
        let mut a: AccountCfg = serde_json::from_value(json!({ "name": "oc", "harness": "opencode", "home": "~/.local/share/opencode", "command": "XDG_DATA_HOME=/x opencode" })).unwrap();
        assert_eq!(adapter_command(&a).unwrap().0, "XDG_DATA_HOME=/x opencode acp");
        a.acp = Some("my-agent --acp".into());
        assert_eq!(adapter_command(&a).unwrap().0, "my-agent --acp");
        let ag: AccountCfg = serde_json::from_value(json!({ "name": "ag", "harness": "antigravity", "home": "~/.gemini" })).unwrap();
        assert!(adapter_command(&ag).is_err());
    }
}
