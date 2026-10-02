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

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::config::{self, AccountCfg};
use crate::harness::{Answer, Pending};
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
    /// (optionId, kind) as offered.
    options: Vec<(String, String)>,
}

/// A session Codealong runs, as the process last said.
#[derive(Clone, Debug, Default)]
struct Hosted {
    account: String,
    id: String,
    cwd: String,
    /// Loaded in the account's current process.
    loaded: bool,
    running: bool,
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
            s.sessions.entry(key).or_insert(Hosted { account: r.account, id: r.id, cwd: r.cwd, ..Default::default() });
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
}

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
    store::change(path, "a list of the sessions Codealong runs", |all: &mut Vec<Remembered>| {
        if all.iter().any(|x| x.harness == r.harness && x.id == r.id && x.account == r.account) {
            return false;
        }
        all.push(r);
        true
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
        self.client.request(method, params, timeout).map_err(|e| match e {
            Failure::Timeout => anyhow!("{} didn't answer {method} in time.", self.account),
            Failure::Closed(_) => anyhow!("{}'s agent stopped.", self.account),
            Failure::Refused(e) => anyhow!("{}", e.get("message").and_then(Value::as_str).unwrap_or("error")),
            Failure::Write(e) => e.into(),
        })
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

/// The account's adapter process, started when there isn't one.
fn agent(a: &AccountCfg) -> Result<Arc<Agent>> {
    if let Some(ag) = state().agents.get(&a.name) {
        return Ok(ag.clone());
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
    let mut st = state();
    // Another start may have raced this one; the first one in stays.
    if let Some(existing) = st.agents.get(&a.name) {
        return Ok(existing.clone());
    }
    st.agents.insert(a.name.clone(), ag.clone());
    Ok(ag)
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
        (Some("session/update"), _) => on_update(account, msg.get("params").unwrap_or(&Value::Null)),
        (Some("session/request_permission"), Some(id)) => on_permission(account, link, id.clone(), msg.get("params").unwrap_or(&Value::Null)),
        (Some(_), Some(id)) => {
            // Files and terminals are the agents' own; nothing else was offered.
            let _ = link.refuse(id, -32601, "Codealong doesn't offer that.");
        }
        _ => {}
    }
}

/// An account's process ended. Its requests have failed (the client fails them first), and its
/// sessions stop until they're sent to again.
fn stopped(account: &str, link: &rpc::Link) {
    {
        let mut st = state();
        if st.agents.get(account).is_some_and(|a| std::ptr::eq::<rpc::Link>(&*a.client, link)) {
            st.agents.remove(account);
        }
        for h in st.sessions.values_mut().filter(|h| h.account == account && h.loaded) {
            h.loaded = false;
            if h.running {
                h.running = false;
                h.since = now_ms();
                h.error = Some("Its agent stopped.".into());
            }
            h.permission = None;
            h.queue.clear();
        }
    }
    notify(Change::Board);
}

fn session_key(account: &str, params: &Value) -> Option<String> {
    let id = params.get("sessionId").and_then(Value::as_str)?;
    let st = state();
    st.sessions.iter().find(|(_, h)| h.account == account && h.id == id).map(|(k, _)| k.clone())
}

fn on_update(account: &str, params: &Value) {
    let Some(key) = session_key(account, params) else { return };
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
    let Some(key) = session_key(account, params) else {
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
    let options = params
        .get("options")
        .and_then(Value::as_array)
        .map(|os| os.iter().filter_map(|o| Some((o.get("optionId")?.as_str()?.to_string(), o.get("kind")?.as_str()?.to_string()))).collect())
        .unwrap_or_default();
    let tool_call = call.get("toolCallId").and_then(Value::as_str).unwrap_or("").to_string();
    {
        let mut st = state();
        let Some(h) = st.sessions.get_mut(&key) else { return };
        // A request still unanswered is overtaken: the agent only asks one at a time.
        h.permission = Some(Permission { rpc, tool_call, label, input: input.map(|i| truncate(&i, 4000)), options });
    }
    notify(Change::Board);
}

// ---- what the app does ----

/// Starts a session in `cwd` on `prompt` and returns its key. The turn runs on; the board shows it.
pub fn start(a: &AccountCfg, cwd: &Path, prompt: &str, model: Option<&str>, effort: Option<&str>) -> Result<String> {
    in_app()?;
    let harness = Harness::from_config(&a.harness);
    let ag = agent(a)?;
    let r = ag.request("session/new", json!({ "cwd": cwd, "mcpServers": [] }), Some(REQUEST_TIMEOUT))?;
    let id = r.get("sessionId").and_then(Value::as_str).ok_or_else(|| anyhow!("{}'s agent made no session.", a.name))?.to_string();
    // Only what the agent offers: Claude Code's aliases (opus, sonnet…) are among its models. The
    // effort levels are the model's, so they're looked up in the options the model's answer lists.
    let mut options = r.get("configOptions").cloned().unwrap_or(Value::Null);
    for (category, value) in [("model", model), ("thought_level", effort)] {
        let Some((v, config_id)) = value.and_then(|v| Some((v, offered_option(&options, category, v)?))) else { continue };
        let got = ag.request("session/set_config_option", json!({ "sessionId": id, "configId": config_id, "value": v }), Some(REQUEST_TIMEOUT))?;
        if let Some(o) = got.get("configOptions") {
            options = o.clone();
        }
    }
    let key = key_of(harness, &a.name, &id);
    let cwd_s = cwd.to_string_lossy().into_owned();
    remember(Remembered { harness, account: a.name.clone(), id: id.clone(), cwd: cwd_s.clone() });
    state().sessions.insert(key.clone(), Hosted { account: a.name.clone(), id, cwd: cwd_s, loaded: true, ..Default::default() });
    if !prompt.trim().is_empty() {
        run_turn(ag, key.clone(), prompt.to_string());
    }
    notify(Change::Board);
    Ok(key)
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
    remember(Remembered { harness, account: a.name.clone(), id: copy.clone(), cwd: cwd_s.clone() });
    // codex-acp lets go of the thread once it's copied (1.13.1), so the first prompt picks it up
    // again as it would a session from before the process started.
    state().sessions.insert(key.clone(), Hosted { account: a.name.clone(), id: copy, cwd: cwd_s, loaded: false, ..Default::default() });
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

/// Whether Codealong runs this session (or ran it, and can again).
pub fn hosts(key: &str) -> bool {
    state().sessions.contains_key(key)
}

/// Sends a prompt to a session Codealong runs. Says what happened, in a sentence.
pub fn send(a: &AccountCfg, key: &str, text: &str) -> Result<String> {
    in_app()?;
    let (id, cwd, loaded) = {
        let st = state();
        let h = st.sessions.get(key).ok_or_else(|| anyhow!("Codealong doesn't run that session."))?;
        (h.id.clone(), h.cwd.clone(), h.loaded && st.agents.contains_key(&a.name))
    };
    let ag = agent(a)?;
    if !loaded {
        load(&ag, key, &id, &cwd)?;
    }
    let queued = {
        let mut st = state();
        let h = st.sessions.get_mut(key).ok_or_else(|| anyhow!("That session is gone."))?;
        if h.running {
            h.queue.push(text.to_string());
            true
        } else {
            false
        }
    };
    if queued {
        notify(Change::Board);
        return Ok("Queued: it goes after the current turn.".into());
    }
    run_turn(ag, key.to_string(), text.to_string());
    notify(Change::Board);
    Ok("Sent.".into())
}

/// Opens a session in the process: resumed where the agent can, else loaded from its history.
fn load(ag: &Arc<Agent>, key: &str, id: &str, cwd: &str) -> Result<()> {
    let params = json!({ "sessionId": id, "cwd": cwd, "mcpServers": [] });
    if ag.can("resume") {
        ag.request("session/resume", params, Some(REQUEST_TIMEOUT))?;
    } else if ag.can("load") {
        set(key, |h| h.replaying = true);
        let r = ag.request("session/load", params, Some(REQUEST_TIMEOUT));
        set(key, |h| h.replaying = false);
        r?;
    } else {
        bail!("{}'s agent can't pick a session up again.", ag.account);
    }
    set(key, |h| h.loaded = true);
    Ok(())
}

fn set(key: &str, f: impl FnOnce(&mut Hosted)) {
    if let Some(h) = state().sessions.get_mut(key) {
        f(h);
    }
}

/// Runs a turn on a thread of its own, then the next queued prompt, until the queue is empty.
fn run_turn(ag: Arc<Agent>, key: String, text: String) {
    set(&key, |h| {
        h.running = true;
        h.since = now_ms();
        h.error = None;
        h.text.clear();
        h.tool = None;
    });
    std::thread::spawn(move || {
        let mut text = text;
        loop {
            let id = state().sessions.get(&key).map(|h| h.id.clone()).unwrap_or_default();
            let prompt = json!([{ "type": "text", "text": text }]);
            let r = ag.request("session/prompt", json!({ "sessionId": id, "prompt": prompt }), None);
            let next = {
                let mut st = state();
                let Some(h) = st.sessions.get_mut(&key) else { return };
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
                if h.queue.is_empty() || r.is_err() {
                    h.running = false;
                    h.since = now_ms();
                    None
                } else {
                    h.since = now_ms();
                    Some(h.queue.remove(0))
                }
            };
            notify(Change::Stream(Stream { key: key.clone(), text: String::new(), tool: None }));
            notify(Change::Board);
            match next {
                Some(t) => text = t,
                None => return,
            }
        }
    });
}

/// Stops the running turn. The agent ends it as cancelled; a permission request waiting is
/// answered as cancelled first, as the protocol asks.
pub fn cancel(key: &str) -> Result<()> {
    let (account, id, permission) = {
        let mut st = state();
        let h = st.sessions.get_mut(key).ok_or_else(|| anyhow!("Codealong doesn't run that session."))?;
        h.queue.clear();
        (h.account.clone(), h.id.clone(), h.permission.take())
    };
    let ag = state().agents.get(&account).cloned().ok_or_else(|| anyhow!("That session isn't running."))?;
    if let Some(p) = permission {
        ag.client.respond(&p.rpc, json!({ "outcome": { "outcome": "cancelled" } }))?;
    }
    ag.client.notify("session/cancel", json!({ "sessionId": id }))?;
    notify(Change::Board);
    Ok(())
}

/// What a hosted session waits on you to allow.
pub fn pending(key: &str) -> Option<Pending> {
    let st = state();
    let p = st.sessions.get(key)?.permission.as_ref()?;
    let always = p.options.iter().any(|(_, kind)| kind == "allow_always");
    Some(Pending { id: p.tool_call.clone(), label: p.label.clone(), input: p.input.clone(), cannot: None, always, plan: false, always_rule: None, questions: Vec::new() })
}

/// Answers the permission request for tool call `id`.
pub fn answer(key: &str, id: &str, answer: Answer) -> Result<String> {
    let (account, p) = {
        let mut st = state();
        let h = st.sessions.get_mut(key).ok_or_else(|| anyhow!("Codealong doesn't run that session."))?;
        match &h.permission {
            Some(p) if p.tool_call == id => (h.account.clone(), h.permission.take().expect("checked")),
            _ => bail!("It isn't waiting on that any more."),
        }
    };
    let wanted: &[&str] = match answer {
        Answer::Allow => &["allow_once", "allow_always"],
        Answer::Always => &["allow_always"],
        Answer::Deny => &["reject_once", "reject_always"],
    };
    let option = wanted.iter().find_map(|k| p.options.iter().find(|(_, kind)| kind == k)).map(|(o, _)| o.clone()).ok_or_else(|| anyhow!("The agent offered no way to answer that."))?;
    let ag = state().agents.get(&account).cloned().ok_or_else(|| anyhow!("That session's agent stopped."))?;
    ag.client.respond(&p.rpc, json!({ "outcome": { "outcome": "selected", "optionId": option } }))?;
    notify(Change::Board);
    Ok(match answer {
        Answer::Allow => "Allowed.".into(),
        Answer::Always => "Allowed, and from now on.".into(),
        Answer::Deny => "Denied.".into(),
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
    let alive = h.loaded && st.agents.contains_key(&h.account);
    // Live, but with no pid of its own: the adapter's process serves all the account's hosted
    // sessions, and a session's pid is taken for its own (closing idle terminals, finding the
    // session a command runs in).
    if alive {
        s.live = true;
        s.pid = None;
    }
    s.open_target = format!("s|{}|hosted", s.key);
    if h.permission.is_some() {
        s.status = Status::Needs;
        s.status_since = Some(h.since);
        s.excerpt = h.permission.as_ref().map(|p| p.label.clone());
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

    #[test]
    fn a_hosted_file_that_does_not_read_is_never_written_over() {
        let dir = store::test_dir("hosted-unreadable");
        let path = dir.join("hosted.json");
        let r = |id: &str| Remembered { harness: Harness::Codex, account: "codex".into(), id: id.into(), cwd: "/r/app".into() };
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

    /// Against a real adapter: `CODEALONG_TEST_ACP_RESUME=<key of a session in hosted.json>
    /// cargo test acp_resumes -- --ignored --nocapture`. Sends to a session this process never
    /// started, as after a restart: it's resumed in a new adapter process first.
    #[test]
    #[ignore]
    fn acp_resumes_a_session() {
        let Ok(key) = std::env::var("CODEALONG_TEST_ACP_RESUME") else { return };
        let cfg = config::load().unwrap().unwrap_or_default();
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
        let cfg = config::load().unwrap().unwrap_or_default();
        let a = cfg.accounts.iter().find(|x| x.name == account).expect("an account by that name").clone();
        init(|_| {});
        let copy = fork(&a, id, Path::new(&dir), "Which word did I ask you to remember? Answer with the word alone, in lowercase.").unwrap();
        println!("{copy}");
        wait_until("the copy's turn to end", 240, || !state().sessions[&copy].running);
        assert!(state().sessions[&copy].error.is_none(), "{:?}", state().sessions[&copy].error);
    }

    /// Against a real adapter: `CODEALONG_TEST_ACP=<account> CODEALONG_TEST_ACP_DIR=<folder>
    /// cargo test acp_runs -- --ignored --nocapture`. Starts a session, has it ask to run a
    /// command in its manual mode, denies it, then sends it a follow-up. The session stays in
    /// the account's records.
    #[test]
    #[ignore]
    fn acp_runs_a_session() {
        let (Ok(name), Ok(dir)) = (std::env::var("CODEALONG_TEST_ACP"), std::env::var("CODEALONG_TEST_ACP_DIR")) else { return };
        let cfg = config::load().unwrap().unwrap_or_default();
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
