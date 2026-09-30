//! Sending a prompt to a session that already exists, the way its harness takes one in.
//!
//! Wherever it can, the prompt arrives as if you'd typed it: pasted into the session's Ghostty
//! tab, handed to Claude Code's daemon for a background job, posted to OpenCode's service, or
//! started as a turn by the Codex app itself. A Claude app session has no such way in, so it
//! gets a message on Claude Code's own cross-session socket, which the session reads as coming
//! from another session. A session that isn't running any more is resumed with the prompt.
//!
//! A session that skips permission prompts holds such a message for you to allow unless it
//! claims the session's own permission class, and the Claude app has no way to show a held one.
//! What you type in Codealong's windows here is yours, as if typed in the session, so it makes
//! that claim; from another device it doesn't, and from an agent only when the agent's own
//! session skips permission prompts too, as Claude Code's own messages between sessions claim
//! their sender's class. So an agent can't use Codealong to get past the hold, and a Claude app
//! session is told nothing rather than holding a message no one will see.
//!
//! A session that's waiting on you for a permission prompt, or stuck mid-turn, is never typed
//! into: what's typed would answer the prompt. One whose last reply asks you something is at its
//! prompt, so what's typed is its answer. Busy sessions are fine; each harness queues the prompt
//! until the agent can take it.
//!
//! An agent's message (`codealong session send`) is never yours. It starts with who sent it, that
//! it isn't from the session's user, and how to answer, and it goes only the ways that don't read
//! as the user's own words or need someone at this Mac: Claude Code's socket for messages between
//! sessions, which tells the model as much and holds the message where the session skips
//! permission prompts, and for the harnesses that have no such socket, their terminal or service.
//! It never resumes a session that isn't running, and never leaves a prompt for someone to send.

use std::borrow::Cow;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Result};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::attachments::{self, Style};
use crate::config::{AccountCfg, Config};
use crate::launch;
use crate::model::{Board, Harness, Session, Status, Surface};
use crate::procs;
use crate::start;

/// How a send went; it goes back to other devices too, so it's in codealong-peer.
pub use codealong_peer::api::Sent;

/// Who a message is from: how the session is told, and which ways in it may take.
#[derive(Debug, Clone, PartialEq)]
pub enum Sender {
    /// You, in one of Codealong's windows on this Mac.
    You,
    /// Another of your devices, or the engine's `--snapshot --send`, where an agent may have
    /// written it.
    Elsewhere,
    /// An agent, through `codealong session send`: the key of the session that ran the command,
    /// when the command found one.
    Agent(Option<String>),
}

/// Who an agent's message is from, as the session it goes to reads it.
#[derive(Debug, Clone, PartialEq)]
struct Byline {
    /// The line the message starts with: that it's another agent's and not the user's, whose,
    /// and how to answer it.
    intro: String,
    /// The name its envelope carries on Claude Code's socket, which Claude Code shows as the
    /// sender's.
    name: String,
}

/// An agent's byline. `from` is the sending session's key; the board names it.
fn byline(board: &Board, from: Option<&str>) -> Byline {
    let Some(key) = from else {
        return Byline {
            intro: "This message was sent with `codealong session send` from a command line that isn't a session Codealong knows, so it may be an agent's rather than your user's.".into(),
            name: "codealong".into(),
        };
    };
    let author = board.session(key).map(|(_, s)| s);
    let who = match author {
        Some(s) => format!("{} ({} on {}, session {key})", quoted(&s.title), s.harness.word(), s.account),
        None => format!("session {key}"),
    };
    Byline {
        intro: format!("This message is from another agent, {who}, sent with `codealong session send`; it isn't from your user. To answer it, run `codealong session send {key} \"<your answer>\"`."),
        name: author.map(|s| s.title.clone()).unwrap_or_else(|| key.to_string()),
    }
}

/// An agent's message without the byline it starts with, as a note of it shows it (claude.rs,
/// `peer_item`): the byline says whose it is, which the note says already.
pub fn without_byline(text: &str) -> &str {
    let t = text.trim();
    let starts = ["This message is from another agent, ", "This message was sent with `codealong session send`"];
    match t.split_once("\n\n") {
        Some((intro, rest)) if starts.iter().any(|s| intro.starts_with(s)) => rest.trim(),
        _ => t,
    }
}

/// What goes to the session: the text as it gets it, before its attachments are written out for
/// the way in, and who it's from.
struct Message<'a> {
    text: Cow<'a, str>,
    sender: &'a Sender,
    /// An agent's: its text starts with the intro.
    byline: Option<Byline>,
    /// Whether its sender skips permission prompts itself, so that the message may say so to a
    /// session that skips them too: you, or an agent whose own session does.
    bypasses: bool,
}

impl<'a> Message<'a> {
    fn new(board: &Board, cfg: &Config, text: &'a str, sender: &'a Sender) -> Message<'a> {
        match sender {
            Sender::Agent(from) => {
                let b = byline(board, from.as_deref());
                let bypasses = from.as_deref().and_then(|k| crate::harness::find(board, cfg, k).ok()).is_some_and(|(s, account)| s.live && crate::harness::adapter(s.harness).skips_permission_prompts(&crate::harness::Ctx { account }, s));
                Message { text: Cow::Owned(format!("{}\n\n{}", b.intro, text.trim())), sender, byline: Some(b), bypasses }
            }
            _ => Message { text: Cow::Borrowed(text), sender, byline: None, bypasses: *sender == Sender::You },
        }
    }

    fn from_agent(&self) -> bool {
        matches!(self.sender, Sender::Agent(_))
    }
}

/// How a prompt reaches a session, tried in order until one works.
#[derive(Debug, Clone, PartialEq)]
pub enum Route {
    /// Claude Code's daemon, for a background job: pasted into the job's terminal as typed.
    Daemon { socket: PathBuf, key_file: PathBuf, job: String },
    /// Pasted into the session's Ghostty tab, then Return.
    Type { pid: u32 },
    /// A line on the session's cross-session socket; `mode` is the permission mode it runs in,
    /// when its files say.
    Peer { socket: String, session_id: String, mode: Option<String> },
    /// Not running: a new terminal resumes it with the prompt.
    Resume,
    /// A Claude app session that isn't running: the app opens it, with the prompt on the
    /// clipboard.
    ClaudeApp,
    /// A turn started through the Codex app's own IPC, else the app opens the thread with the
    /// prompt filled in.
    CodexApp,
    Opencode,
    /// Nothing takes a prompt from outside: the app comes forward with it on the clipboard.
    Antigravity,
}

/// Sends `text` to the session with this key. The same text sent to the same session while it's
/// still on its way (the peek closed and opened again, and you sent what it still showed, or
/// another window sent it too) joins that send and gets its outcome, rather than going twice.
pub fn send(board: &Board, cfg: &Config, session_key: &str, text: &str, sender: Sender) -> Result<Sent> {
    join(session_key, text.trim(), || send_now(board, cfg, session_key, text, &sender))
}

/// A send on its way, which a send of the same text to the same session waits on.
struct Flight {
    key: String,
    text: String,
    outcome: Mutex<Option<std::result::Result<Sent, String>>>,
    landed: Condvar,
    /// How many sends joined it.
    joined: std::sync::atomic::AtomicUsize,
}

static FLIGHTS: Mutex<Vec<Arc<Flight>>> = Mutex::new(Vec::new());

/// Runs `send` unless the same text is on its way to the same session already, in which case
/// it waits for that one and says how it went.
fn join(key: &str, text: &str, send: impl FnOnce() -> Result<Sent>) -> Result<Sent> {
    let lock = || FLIGHTS.lock().unwrap_or_else(|e| e.into_inner());
    let (flight, joined) = {
        let mut all = lock();
        match all.iter().find(|f| f.key == key && f.text == text) {
            Some(f) => (f.clone(), true),
            None => {
                let f = Arc::new(Flight { key: key.to_string(), text: text.to_string(), outcome: Mutex::new(None), landed: Condvar::new(), joined: Default::default() });
                all.push(f.clone());
                (f, false)
            }
        }
    };
    if joined {
        flight.joined.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let mut out = flight.outcome.lock().unwrap_or_else(|e| e.into_inner());
        while out.is_none() {
            out = flight.landed.wait(out).unwrap_or_else(|e| e.into_inner());
        }
        return out.clone().expect("landed").map_err(|e| anyhow!(e));
    }
    /// Lands the flight however the send ends, a panic included, so no one waits on it forever.
    struct Landing<'a>(&'a Arc<Flight>, Option<std::result::Result<Sent, String>>);
    impl Drop for Landing<'_> {
        fn drop(&mut self) {
            let outcome = self.1.take().unwrap_or_else(|| Err("The send stopped partway.".to_string()));
            *self.0.outcome.lock().unwrap_or_else(|e| e.into_inner()) = Some(outcome);
            self.0.landed.notify_all();
            FLIGHTS.lock().unwrap_or_else(|e| e.into_inner()).retain(|f| !Arc::ptr_eq(f, self.0));
        }
    }
    let mut landing = Landing(&flight, None);
    let result = send();
    landing.1 = Some(result.as_ref().map(Sent::clone).map_err(|e| format!("{e:#}")));
    result
}

/// A way in that failed after the prompt may have reached the session, such as typing Ghostty
/// didn't confirm: no other way is tried, or the session could get it twice.
#[derive(Debug)]
struct MayHaveArrived(String);

impl std::fmt::Display for MayHaveArrived {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for MayHaveArrived {}

/// How one way in went, when it didn't fail.
enum Tried {
    Sent(Sent),
    /// Nothing went this way, so the next way is tried; why, when the note on where it went
    /// instead should say (typing into its terminal didn't work).
    Passed(Option<String>),
}

fn send_now(board: &Board, cfg: &Config, session_key: &str, text: &str, sender: &Sender) -> Result<Sent> {
    let session = board
        .lanes
        .iter()
        .flat_map(|l| &l.sessions)
        .find(|s| s.key == session_key)
        .ok_or_else(|| anyhow!("That session isn't on the board any more."))?;
    if text.trim().is_empty() {
        bail!("There's nothing to send.");
    }
    let account = cfg.accounts.iter().find(|a| a.name == session.account).ok_or_else(|| anyhow!("There's no account named {} in config.toml.", session.account))?;
    let msg = Message::new(board, cfg, text, sender);
    if session.hosted {
        // Codealong runs it: the prompt goes in over its adapter, after the running turn if
        // there is one, and a question it waits on is answered from its panel.
        let message = crate::acp::send(account, session_key, &attachments::expand(&msg.text, Style::Path))?;
        return Ok(Sent { message, opened: false, fell_back: false });
    }
    ensure_not_waiting(session, sender)?;
    // Each harness says which ways reach the session, best first (`harness::Adapter::routes`).
    let mut routes = crate::harness::adapter(session.harness).routes(account, session);
    if msg.from_agent() {
        routes = agent_routes(session, routes)?;
    }
    let near = crate::board::terminal_neighbours(&board.lanes, Path::new(&session.cwd));
    let place = launch::Placement { new_terminal: cfg.apps.new_terminal, near: &near };
    let mut last_err = None;
    // Why typing into its terminal didn't work, for the note on the way it went instead.
    let mut untyped = None;
    for route in routes {
        match attempt(&route, session, account, cfg, place, &msg, untyped.as_deref()) {
            Ok(Tried::Sent(sent)) => return Ok(sent),
            // This way doesn't reach the session; the next one may.
            Ok(Tried::Passed(why)) => untyped = why.or(untyped),
            Err(e) if e.is::<MayHaveArrived>() => return Err(e),
            Err(e) => last_err = Some(e),
        }
    }
    Err(last_err.unwrap_or_else(|| anyhow!("Codealong can't reach {} to send it this. Open it and send it there.", quoted(&session.title))))
}

/// A session waiting on you is never typed into: what's typed would answer its prompt. One that
/// only asks you something in its reply (`Session::asks`) takes the answer like any prompt.
fn ensure_not_waiting(session: &Session, sender: &Sender) -> Result<()> {
    if session.status == Status::Needs && !session.asks {
        match sender {
            Sender::Agent(_) => bail!("{} is waiting on its user to answer a prompt, so nothing was sent. Try again once they have.", quoted(&session.title)),
            _ => bail!("{} is waiting on you; open it to answer.", quoted(&session.title)),
        }
    }
    Ok(())
}

/// The ways in an agent's message may take, of the session's own. Claude Code takes messages from
/// other sessions on its socket, which tells the model a message is another session's and holds
/// it for the user where the session skips permission prompts; typed into its terminal or handed
/// to its daemon, the message would read as its user's own words. Other harnesses have no such
/// socket and take it typed or posted, the byline saying whose it is. A way that resumes a session
/// or leaves the prompt for someone to send isn't for an agent.
fn agent_routes(session: &Session, routes: Vec<Route>) -> Result<Vec<Route>> {
    let kept: Vec<Route> = routes
        .into_iter()
        .filter(|r| match r {
            Route::Peer { .. } | Route::CodexApp | Route::Opencode => true,
            Route::Type { .. } => session.harness != Harness::Claude,
            Route::Daemon { .. } | Route::Resume | Route::ClaudeApp | Route::Antigravity => false,
        })
        .collect();
    if kept.is_empty() {
        let title = quoted(&session.title);
        bail!(match session.harness {
            Harness::Antigravity => "Antigravity takes no prompts from outside; Codealong can only put one on the clipboard for someone at this Mac, so nothing was sent.".to_string(),
            _ if !session.live => format!("{title} isn't running, and a message from another session doesn't start it again, so nothing was sent. Its user can resume it, or `codealong start` starts a new session."),
            _ => format!("{title} doesn't take messages from other sessions (its Claude Code doesn't listen for them), and typing into its terminal would read as its user's words, so nothing was sent."),
        });
    }
    Ok(kept)
}

/// Claude Code's ways in, best first, from its registry entry and whether its daemon runs.
/// Pure, so the choice has tests. A live session: its daemon for a background job, its
/// terminal, then its cross-session socket; one that isn't running: the Claude app for an app
/// session, else a terminal that resumes it.
pub fn claude_routes(session: &Session, registry: Option<&Value>, home: &Path, daemon: Option<&Path>, mode: Option<String>) -> Vec<Route> {
    if !session.live {
        return vec![if session.surface == Surface::App { Route::ClaudeApp } else { Route::Resume }];
    }
    let mut out = Vec::new();
    let kind = registry.and_then(|r| r.get("kind")).and_then(Value::as_str);
    let job = registry.and_then(|r| r.get("jobId")).and_then(Value::as_str);
    if let (Some("bg"), Some(job), Some(socket)) = (kind, job, daemon) {
        out.push(Route::Daemon { socket: socket.to_path_buf(), key_file: home.join("daemon/control.key"), job: job.to_string() });
    }
    if session.surface == Surface::Cli && kind != Some("bg") {
        if let Some(pid) = session.pid {
            out.push(Route::Type { pid });
        }
    }
    let peer = registry
        .filter(|r| r.get("peerProtocol").and_then(Value::as_u64) == Some(1))
        .and_then(|r| Some((r.get("messagingSocketPath")?.as_str()?, r.get("sessionId")?.as_str()?)));
    if let Some((socket, id)) = peer {
        out.push(Route::Peer { socket: socket.to_string(), session_id: id.to_string(), mode });
    }
    out
}

/// A live Claude Code session's registry entry, if it's this session's.
pub fn claude_registry(home: &Path, session: &Session) -> Option<Value> {
    session.pid.and_then(|pid| read_registry(home, pid)).filter(|r| r.get("sessionId").and_then(Value::as_str) == Some(session.id.as_str()))
}

/// One way in: it went, or nothing went and the next way is tried, or it failed. `untyped` is
/// why typing into the session's terminal didn't work, when that was tried first.
fn attempt(route: &Route, session: &Session, account: &AccountCfg, cfg: &Config, place: launch::Placement, msg: &Message, untyped: Option<&str>) -> Result<Tried> {
    let text: &str = &msg.text;
    let busy = session.status == Status::Working;
    let title = quoted(&session.title);
    let sent = |message: String| Ok(Tried::Sent(Sent { message, opened: false, fell_back: false }));
    let opened = |message: String| Ok(Tried::Sent(Sent { message, opened: true, fell_back: false }));
    match route {
        Route::Daemon { socket, key_file, job } => {
            let key = std::fs::read_to_string(key_file).map_err(|e| anyhow!("Claude Code's daemon key: {e}"))?;
            daemon_reply(socket, key.trim(), job, &attachments::expand(text, Style::Mention))?;
            sent(if busy { format!("Queued for {title}; the background session takes it after its current step.") } else { format!("Sent to {title} in the background.") })
        }
        Route::Type { pid } => {
            let style = if session.harness == Harness::Claude { Style::Mention } else { Style::Path };
            // A prompt Claude Code took back may be in its input box again, where what's typed
            // would be added to it and both go as one prompt.
            let clear = session.harness == Harness::Claude && crate::claude::last_prompt_taken_back(account, session);
            match launch::terminal_type_session(*pid, Path::new(&session.cwd), &attachments::expand(text, style), clear) {
                Ok(launch::Typed::Into(app)) => sent(if busy { format!("Typed into {title} in {app}; it takes it after its current step.") } else { format!("Typed into {title} in {app}.") }),
                Ok(launch::Typed::Nowhere(why)) => Ok(Tried::Passed(Some(why))),
                Err(_) => Err(MayHaveArrived(format!("Codealong started typing this into {title}, and its terminal didn't confirm it went in: look there before sending it again.")).into()),
            }
        }
        Route::Peer { socket, session_id, mode } => {
            let skips = mode.as_deref() == Some("bypassPermissions");
            let claim = (skips && msg.bypasses).then_some("bypass");
            if skips && claim.is_none() && session.surface == Surface::App {
                if msg.from_agent() {
                    bail!("{title} skips permission prompts and your session doesn't, so Claude Code would hold your message for its user to allow, and the Claude app can't show that; nothing was sent.");
                }
                bail!("{title} skips permission prompts, so Claude Code would hold a message from here for you to allow, and the Claude app can't show that. Send it from Codealong on the Mac it runs on, or in the Claude app.");
            }
            let from = msg.byline.as_ref().map(|b| b.name.as_str());
            peer_send(socket, session_id, &attachments::expand(text, Style::Path), claim, from)?;
            let held = skips && claim.is_none();
            Ok(Tried::Sent(Sent { message: peer_note(&title, untyped, busy, held, msg.from_agent()), opened: false, fell_back: untyped.is_some() }))
        }
        Route::Resume => {
            let (input, env) = resume_input(account, &session.id, text);
            launch::terminal_run(&cfg.apps.terminal, place, Path::new(&session.cwd), &input, &env)?;
            sent(format!("Resumed {title} in {} with this prompt.", launch::terminal_name(&cfg.apps.terminal)))
        }
        Route::ClaudeApp => {
            copy(&attachments::expand(text, Style::Path))?;
            launch::open_url(&launch::claude_app_url(&session.id))?;
            opened(format!("{title} is open in the Claude app and the prompt is on the clipboard. Paste it there to send it."))
        }
        Route::CodexApp => {
            let prompt = attachments::expand(text, Style::Path);
            match codex_start_turn(&account.home_path(), &session.id, &prompt, None) {
                Ok(()) => sent(if busy { format!("Sent to {title} in Codex; it takes it after its current turn.") } else { format!("Sent to {title} in Codex.") }),
                // A prompt filled in for someone to send isn't a message an agent can leave.
                Err(e) if msg.from_agent() => bail!("Codex didn't take it for {title} ({e}), so nothing was sent."),
                Err(_) => {
                    // The app isn't running, didn't take the thread, or has changed how its
                    // windows talk: its own link fills the reply in instead.
                    launch::open_codex(&format!("{}?prompt={}", launch::codex_thread_url(&session.id), urlencoding::encode(&prompt)))?;
                    opened(format!("Codex has {title} open with the prompt filled in; send it there."))
                }
            }
        }
        Route::Opencode => {
            let svc = crate::opencode::service(&account.home_path()).ok_or_else(|| anyhow!("OpenCode's service isn't running, so Codealong can't reach {title}. Open it and send it there."))?;
            let body = json!({ "text": attachments::expand(text, Style::Path), "delivery": "queue" });
            crate::opencode::post(&svc, &format!("/api/session/{}/prompt", urlencoding::encode(&session.id)), &body)?;
            sent(if busy { format!("Queued for {title} in OpenCode; it takes it after its current step.") } else { format!("Sent to {title} in OpenCode.") })
        }
        Route::Antigravity => {
            copy(&attachments::expand(text, Style::Path))?;
            launch::open_antigravity()?;
            opened("Antigravity is open and the prompt is on the clipboard. Paste it there to send it.".to_string())
        }
    }
}

/// What a prompt sent on a session's socket says it did. `untyped` is why it wasn't typed into
/// the session's terminal, when that was tried first: a session reads a message from another
/// session as a teammate's, not as yours, so it says it went that way, first, since the bar
/// shows the start of it. `held`: the session skips permission prompts and the message claims
/// no mode, so it asks you (or, for an agent's, its user) first.
fn peer_note(title: &str, untyped: Option<&str>, busy: bool, held: bool, agent: bool) -> String {
    let lead = match untyped {
        Some(why) => format!("Sent as a message from Codealong, not typed into {title}: {why}."),
        None if busy && !held => format!("Sent to {title}; it reads it as it works."),
        None => format!("Sent to {title}."),
    };
    match (held, agent) {
        (true, false) => format!("{lead} It skips permission prompts, so it asks you in its terminal before taking it."),
        (true, true) => format!("{lead} It skips permission prompts, so it asks its user in its terminal before taking it."),
        _ => lead,
    }
}

fn quoted(title: &str) -> String {
    format!("“{}”", title.trim())
}

fn copy(text: &str) -> Result<()> {
    procs::run_stdin("/usr/bin/pbcopy", &[], text, Duration::from_secs(5)).map(|_| ())
}

// ---- Claude Code ----

/// A live session's registry entry: `<home>/sessions/<pid>.json`.
fn read_registry(home: &Path, pid: u32) -> Option<Value> {
    serde_json::from_str(&std::fs::read_to_string(home.join("sessions").join(format!("{pid}.json"))).ok()?).ok()
}

/// Claude Code's daemon socket for an account, when its daemon is running.
pub fn daemon_socket(home: &Path) -> Option<PathBuf> {
    let path = daemon_socket_path(home, unsafe { libc::getuid() });
    path.exists().then_some(path)
}

/// `/tmp/cc-daemon-<uid>/<first 8 hex of sha256(config dir)>/control.sock`. The config dir is
/// hashed as an absolute path without a trailing slash (`~/.claude2` gave the folder here).
pub fn daemon_socket_path(home: &Path, uid: u32) -> PathBuf {
    let dir = home.to_string_lossy();
    let dir = dir.trim_end_matches('/');
    let hash = Sha256::digest(dir.as_bytes());
    let short: String = hash.iter().take(4).map(|b| format!("{b:02x}")).collect();
    PathBuf::from(format!("/tmp/cc-daemon-{uid}/{short}/control.sock"))
}

/// The daemon's `reply` request: one JSON line each way.
pub fn daemon_request(key: &str, job: &str, text: &str) -> String {
    format!("{}\n", json!({ "proto": 1, "op": "reply", "short": job, "text": text, "auth": key }))
}

/// Hands the prompt to a background job through Claude Code's daemon, which pastes it into the
/// job's terminal. A job that's starting up or restarting says so; it's asked again for a
/// couple of seconds, as Claude Code's own agent view does.
fn daemon_reply(socket: &Path, key: &str, job: &str, text: &str) -> Result<()> {
    let request = daemon_request(key, job, text);
    let mut last = String::new();
    for _ in 0..10 {
        let mut stream = UnixStream::connect(socket)?;
        stream.set_read_timeout(Some(Duration::from_secs(10)))?;
        stream.set_write_timeout(Some(Duration::from_secs(5)))?;
        stream.write_all(request.as_bytes())?;
        let mut line = String::new();
        BufReader::new(&stream).read_line(&mut line)?;
        let answer: Value = serde_json::from_str(line.trim()).map_err(|_| anyhow!("Claude Code's daemon gave an answer Codealong can't read."))?;
        if answer.get("ok").and_then(Value::as_bool) == Some(true) {
            return Ok(());
        }
        let code = answer.get("code").and_then(Value::as_str).unwrap_or("");
        last = answer.get("error").and_then(Value::as_str).unwrap_or(code).to_string();
        if !matches!(code, "ESTARTING" | "ENOREPLY" | "ERESPAWNING") {
            break;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    bail!("Claude Code's daemon didn't take it: {last}")
}

/// The daemon's `attach` request, as `claude attach` sends it but without a terminal's
/// capabilities (`caps` is optional, and the job goes on without them as it does with nobody
/// attached; null is refused).
pub fn daemon_attach_request(key: &str, job: &str, attach_id: &str) -> String {
    format!("{}\n", json!({ "proto": 1, "op": "attach", "short": job, "auth": key, "cols": 200, "rows": 50, "attachId": attach_id }))
}

/// Presses Escape in a background job's terminal. The daemon has no request for it, and its
/// `reply` pastes text and presses Return, so this attaches as `claude attach` does, which hands
/// what comes after the request line to the job's terminal as typed, and detaches. Attaching
/// redraws the job and sizes its terminal to this one; detaching gives a terminal attached
/// meanwhile its own size back.
pub fn daemon_press_escape(socket: &Path, key_file: &Path, job: &str) -> Result<()> {
    let key = std::fs::read_to_string(key_file).map_err(|_| anyhow!("Codealong can't read the background service's key."))?;
    let mut stream = UnixStream::connect(socket)?;
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;
    stream.write_all(daemon_attach_request(key.trim(), job, &start::new_session_id()).as_bytes())?;
    // The answer is one line; the job's screen follows it.
    let mut line = Vec::new();
    let mut byte = [0u8; 1];
    while stream.read(&mut byte)? == 1 && byte[0] != b'\n' {
        line.push(byte[0]);
    }
    let answer: Value = serde_json::from_slice(&line).map_err(|_| anyhow!("Claude Code's daemon gave an answer Codealong can't read."))?;
    if answer.get("ok").and_then(Value::as_bool) != Some(true) {
        let why = answer.get("error").and_then(Value::as_str).or_else(|| answer.get("code").and_then(Value::as_str)).unwrap_or("no reason given");
        bail!("Claude Code's daemon didn't let Codealong in: {why}");
    }
    // Apart from what attaching writes to the job first (focus, a redraw), so it reads as a key
    // of its own. The screen is read and dropped meanwhile: the daemon closes a connection whose
    // unread output piles up.
    let drain = |stream: &mut UnixStream, for_ms: u64| {
        let until = Instant::now() + Duration::from_millis(for_ms);
        let mut chunk = [0u8; 65536];
        while let Some(left) = until.checked_duration_since(Instant::now()) {
            let _ = stream.set_read_timeout(Some(left.max(Duration::from_millis(10))));
            match stream.read(&mut chunk) {
                Ok(0) => break,
                Ok(_) => {}
                Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => break,
                Err(_) => break,
            }
        }
    };
    drain(&mut stream, 200);
    stream.write_all(b"\x1b")?;
    drain(&mut stream, 400);
    Ok(())
}

/// The sender's name on what you send, and the line its text starts with. The session's
/// transcript keeps both (claude.rs, `sent_from_codealong`), which is how its panel knows the
/// message was yours.
pub const FROM_CODEALONG: &str = "Codealong";
pub const FROM_YOU: &str = "Your user sent this to this session from Codealong:";

/// The prompt as a cross-session message. The envelope is the one Claude Code writes for its
/// own sends (`<cross-session-message from-name="…">`, the body on lines of its own), so the
/// session shows who it's from: Codealong for what you sent, or `from`, an agent's session, whose
/// text carries its own byline. `claim` is the permission class it claims (`bypass` or
/// `prompting`, `from-mode`), which Claude Code compares with the session's own: a session
/// that skips prompts holds a message that claims none, or another, for you to allow. The
/// attributes go in the order Claude Code writes them, since it reads the envelope only when
/// writing it again gives the same text.
pub fn peer_content(text: &str, claim: Option<&str>, from: Option<&str>) -> String {
    let (name, body) = match from {
        Some(name) => (envelope_name(name), text.trim().to_string()),
        None => (FROM_CODEALONG.to_string(), format!("{FROM_YOU}\n\n{}", text.trim())),
    };
    let body = body.replace("</cross-session-message", "<\\/cross-session-message");
    let mode = claim.map(|m| format!(" from-mode=\"{m}\"")).unwrap_or_default();
    format!("<cross-session-message from-name=\"{name}\"{mode}>\n{body}\n</cross-session-message>")
}

/// A sender's name as Claude Code writes `from-name` (2.1.284): without quotes, angle brackets,
/// or control and format characters, trimmed, and at most 64 characters, a longer one cut with
/// "…". Anything else would come out different when Claude Code writes the envelope again, and
/// it would then not read it as one. This keeps letters, digits, spaces and punctuation, which
/// come out the same.
fn envelope_name(name: &str) -> String {
    let kept: String = name
        .chars()
        .map(|c| if c.is_whitespace() { ' ' } else { c })
        .filter(|&c| c.is_alphanumeric() || c == ' ' || (c.is_ascii_punctuation() && !matches!(c, '"' | '<' | '>')) || "“”‘’–—…·".contains(c))
        .collect();
    let kept = kept.split(' ').filter(|w| !w.is_empty()).collect::<Vec<_>>().join(" ");
    let chars: Vec<char> = kept.chars().collect();
    match chars.len() {
        0 => "codealong".to_string(),
        n if n > 64 => chars[..63].iter().collect::<String>().trim_end().to_string() + "…",
        _ => kept,
    }
}

/// The line written to a session's socket. `session_id` makes a session that has since taken
/// over the pid drop it.
pub fn peer_frame(session_id: &str, msg_id: &str, text: &str, claim: Option<&str>, from: Option<&str>) -> String {
    let frame = json!({
        "msgV": 1,
        "msg_id": msg_id,
        "type": "user",
        "session_id": session_id,
        "message": { "role": "user", "content": peer_content(text, claim, from) },
        "priority": "next",
    });
    format!("{frame}\n")
}

/// Nothing comes back on the connection, so a successful write is all there is to see. Claude
/// Code's own sender waits a moment before closing, so the line is read before the socket goes.
fn peer_send(socket: &str, session_id: &str, text: &str, claim: Option<&str>, from: Option<&str>) -> Result<()> {
    let frame = peer_frame(session_id, &start::new_session_id(), text, claim, from);
    // Claude Code caps a line at a million characters.
    if frame.len() > 1_000_000 {
        bail!("That's too long to send to another session; shorten it or put it in a file.");
    }
    let mut stream = UnixStream::connect(socket).map_err(|e| anyhow!("The session's message socket didn't answer: {e}"))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;
    stream.write_all(frame.as_bytes())?;
    stream.flush()?;
    std::thread::sleep(Duration::from_millis(150));
    let _ = stream.shutdown(std::net::Shutdown::Both);
    Ok(())
}

/// The terminal input that resumes a Claude Code session with a prompt, and the environment
/// that carries the prompt: typed through the shell, as a new session's is (start.rs), so no
/// quoting of yours can break the command.
pub fn resume_input(account: &AccountCfg, session_id: &str, text: &str) -> (String, Vec<(String, String)>) {
    if account.harness == "codex" {
        // `codex resume <id> <prompt>`; its files go as paths, and `--` keeps a prompt that
        // starts with a dash from reading as an option.
        let input = format!("{} resume -- {session_id} \"${}\"\n", account.command(), start::PROMPT_VAR);
        return (input, vec![(start::PROMPT_VAR.to_string(), attachments::expand(text, Style::Path).trim().to_string())]);
    }
    let (has_images, has_files) = attachments::kinds(text);
    let mut input = account.command().to_string();
    let mut env = Vec::new();
    for (has, var, dir) in [(has_images, start::IMAGES_VAR, attachments::images_dir()), (has_files, start::FILES_VAR, attachments::files_dir())] {
        if has {
            input.push_str(&format!(" --add-dir \"${var}\""));
            env.push((var.to_string(), dir.to_string_lossy().to_string()));
        }
    }
    input.push_str(&format!(" --resume {session_id} \"${}\"\n", start::PROMPT_VAR));
    env.push((start::PROMPT_VAR.to_string(), start::as_argument(attachments::expand(text, Style::Mention).trim())));
    (input, env)
}

// ---- Codex app ----

/// A frame on the Codex app's IPC socket: a 4-byte little-endian length, then JSON.
pub fn ipc_frame(message: &Value) -> Vec<u8> {
    let body = message.to_string().into_bytes();
    let mut out = (body.len() as u32).to_le_bytes().to_vec();
    out.extend(body);
    out
}

/// Takes one whole frame off the front of `buf`, if one is there.
pub fn ipc_take(buf: &mut Vec<u8>) -> Option<Value> {
    if buf.len() < 4 {
        return None;
    }
    let n = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;
    if buf.len() < 4 + n {
        return None;
    }
    let frame: Vec<u8> = buf.drain(..4 + n).skip(4).collect();
    Some(serde_json::from_slice(&frame).unwrap_or(Value::Null))
}

/// The request that starts a turn in a thread the app has open, sent to the window that owns it.
/// The app's router drops a request whose `version` isn't the one it expects for the method
/// (2 for this one in 0.155), so a changed app fails here rather than doing something else.
/// A thread with no turns yet needs its model named: the window otherwise sends an empty one,
/// which the API refuses.
pub fn codex_start_request(request_id: &str, client_id: &str, owner: &str, thread: &str, text: &str, message_id: &str, model: Option<(&str, Option<&str>)>) -> Value {
    let mut request = json!({
        "threadId": thread,
        "input": [{ "type": "text", "text": text, "text_elements": [] }],
        "clientUserMessageId": message_id,
    });
    if let Some((m, effort)) = model {
        request["model"] = json!(m);
        if let Some(e) = effort {
            request["effort"] = json!(e);
        }
    }
    json!({
        "type": "request",
        "requestId": request_id,
        "sourceClientId": client_id,
        "version": 2,
        "method": "thread-follower-start-turn",
        "targetClientId": owner,
        "timeoutMs": 10_000,
        "params": {
            "conversationId": thread,
            "turnStart": { "request": request, "context": {} },
        },
    })
}

struct Ipc {
    stream: UnixStream,
    buf: Vec<u8>,
    client_id: String,
}

impl Ipc {
    fn send(&mut self, message: &Value) -> Result<()> {
        self.stream.write_all(&ipc_frame(message))?;
        Ok(())
    }

    /// Waits for the response to `request_id`. The router also asks every client whether it can
    /// handle other clients' requests; Codealong can't, and says so at once, or they'd wait on it.
    fn response(&mut self, request_id: &str, deadline: Instant) -> Result<Value> {
        loop {
            while let Some(m) = ipc_take(&mut self.buf) {
                match m.get("type").and_then(Value::as_str) {
                    Some("response") if m.get("requestId").and_then(Value::as_str) == Some(request_id) => return Ok(m),
                    Some("client-discovery-request") => {
                        let id = m.get("requestId").cloned().unwrap_or(Value::Null);
                        self.send(&json!({ "type": "client-discovery-response", "requestId": id, "response": { "canHandle": false } }))?;
                    }
                    Some("request") => {
                        let id = m.get("requestId").cloned().unwrap_or(Value::Null);
                        self.send(&json!({ "type": "response", "requestId": id, "resultType": "error", "error": "no-handler-for-request" }))?;
                    }
                    _ => {}
                }
            }
            let left = deadline.checked_duration_since(Instant::now()).ok_or_else(|| anyhow!("Codex didn't answer in time."))?;
            self.stream.set_read_timeout(Some(left.max(Duration::from_millis(10))))?;
            let mut chunk = [0u8; 65536];
            let n = self.stream.read(&mut chunk)?;
            if n == 0 {
                bail!("Codex closed the connection.");
            }
            self.buf.extend_from_slice(&chunk[..n]);
        }
    }
}

impl Ipc {
    /// Connects to the socket the Codex app's windows use to reach each other
    /// (`<CODEX_HOME>/ipc/ipc.sock`) and joins it as a client.
    fn connect(codex_home: &Path, deadline: Instant) -> Result<Ipc> {
        let stream = UnixStream::connect(codex_home.join("ipc/ipc.sock")).map_err(|_| anyhow!("The Codex app isn't running."))?;
        stream.set_write_timeout(Some(Duration::from_secs(5)))?;
        let mut ipc = Ipc { stream, buf: Vec::new(), client_id: String::new() };
        let init = start::new_session_id();
        ipc.send(&json!({ "type": "request", "requestId": init, "method": "initialize", "params": { "clientType": "codealong" } }))?;
        let r = ipc.response(&init, deadline)?;
        ipc.client_id = r.pointer("/result/clientId").and_then(Value::as_str).ok_or_else(|| anyhow!("Codex didn't take Codealong as a client."))?.to_string();
        Ok(ipc)
    }

    /// The window that has `thread` loaded, if one answers by `wait`. A window owns only the
    /// threads it has loaded, and asking about any other goes unanswered until the router gives
    /// up ten seconds later.
    fn owner(&mut self, thread: &str, wait: Instant) -> Result<Option<String>> {
        let find = start::new_session_id();
        self.send(&json!({
            "type": "request", "requestId": find, "sourceClientId": self.client_id, "version": 1,
            "method": "thread-owner-discovery", "params": { "hostId": "local", "conversationId": thread },
        }))?;
        Ok(self.response(&find, wait).ok().filter(|r| r.get("resultType").and_then(Value::as_str) == Some("success")).and_then(|r| r.get("handledByClientId").and_then(Value::as_str).map(String::from)))
    }
}

/// Starts a turn with `text` in a Codex thread, through the socket the app's windows use to
/// reach each other. The window that owns the thread runs the turn in the app's own app-server,
/// so the app shows it as if you'd sent it there. A thread no window has loaded is opened in the
/// app after a short wait, which loads it and takes it within a couple of seconds, and asked
/// about again.
pub fn codex_start_turn(codex_home: &Path, thread: &str, text: &str, model: Option<(&str, Option<&str>)>) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(18);
    let mut ipc = Ipc::connect(codex_home, deadline)?;
    let client = ipc.client_id.clone();

    let mut opened = false;
    let owner = loop {
        let wait = (Instant::now() + Duration::from_millis(1500)).min(deadline);
        if let Some(owner) = ipc.owner(thread, wait)? {
            if opened {
                // A window takes a thread as it starts loading it, and a turn that arrives
                // before its settings are in goes out with an empty model (refused) and the
                // app's default reviewer. They're in within about a second.
                std::thread::sleep(Duration::from_secs(2));
            }
            break owner;
        }
        if Instant::now() >= deadline {
            bail!("No Codex window took this thread.");
        }
        if !opened {
            launch::open_codex(&launch::codex_thread_url(thread))?;
            opened = true;
        }
        std::thread::sleep(Duration::from_millis(300));
    };

    let turn = start::new_session_id();
    ipc.send(&codex_start_request(&turn, &client, &owner, thread, text, &start::new_session_id(), model))?;
    let r = ipc.response(&turn, deadline)?;
    if r.get("resultType").and_then(Value::as_str) != Some("success") {
        bail!("Codex didn't start the turn: {}", r.get("error").and_then(Value::as_str).unwrap_or("no reason given"));
    }
    Ok(())
}

/// The request that stops the turn a thread is taking, sent to the window that owns it: what
/// the app's own Stop sends from a window that doesn't (`user-stop`, which also pauses the
/// thread's goal and stops its subagents). Without the turn's id the router expects version 3;
/// with one, 4 (26.924's code).
pub fn codex_interrupt_request(request_id: &str, client_id: &str, owner: &str, thread: &str) -> Value {
    json!({
        "type": "request",
        "requestId": request_id,
        "sourceClientId": client_id,
        "version": 3,
        "method": "thread-follower-interrupt-turn",
        "targetClientId": owner,
        "timeoutMs": 10_000,
        "params": { "conversationId": thread, "mode": "user-stop" },
    })
}

/// Stops the turn a Codex thread is taking in the app, through the window that runs it. A thread
/// that's running there is loaded in some window, so one that no window claims isn't running in
/// the app.
pub fn codex_interrupt_turn(codex_home: &Path, thread: &str) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(12);
    let mut ipc = Ipc::connect(codex_home, deadline)?;
    let Some(owner) = ipc.owner(thread, (Instant::now() + Duration::from_secs(3)).min(deadline))? else {
        bail!("No Codex window has this thread open, so it isn't running in the app.");
    };
    let id = start::new_session_id();
    ipc.send(&codex_interrupt_request(&id, &ipc.client_id.clone(), &owner, thread))?;
    let r = ipc.response(&id, deadline)?;
    if r.get("resultType").and_then(Value::as_str) != Some("success") {
        bail!("Codex didn't stop the turn: {}", r.get("error").and_then(Value::as_str).unwrap_or("no reason given"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Harness, Session, Status, Surface};

    fn session(harness: Harness, surface: Surface, status: Status, live: bool, pid: Option<u32>) -> Session {
        Session {
            key: "claude:claude:abc".into(),
            id: "abc".into(),
            short_id: "abc".into(),
            harness,
            account: "claude".into(),
            surface,
            status,
            live,
            pid,
            title: "Fix the build".into(),
            cwd: "/tmp".into(),
            work: Vec::new(),
            reply_tail: None,
            first_prompt: None,
            name: None,
            started_at: None,
            status_since: None,
            last_activity: 0,
            last_prompt_at: 0,
            excerpt: None,
            resume_command: None,
            app_account: false,
            open_target: String::new(),
            visitor: None,
            headline: String::new(),
            detail: None,
            branch: None,
            archived: false,
            attach_command: None,
            continues: None,
            stalled_since: None,
            background: None,
            insight: None,
            asks: false,
            hosted: false,
            stoppable: false,
            snoozed_until: None,
        }
    }

    /// The ways `send` would try, with the registry and daemon given rather than read.
    fn route_for(s: &Session, registry: Option<&Value>, home: &Path, daemon: Option<&Path>) -> Result<Vec<Route>> {
        ensure_not_waiting(s, &Sender::You)?;
        Ok(if s.harness == Harness::Claude {
            claude_routes(s, registry, home, daemon, None)
        } else {
            let account: AccountCfg = serde_json::from_value(json!({ "name": "a", "harness": "codex", "home": "/h" })).unwrap();
            crate::harness::adapter(s.harness).routes(&account, s)
        })
    }

    fn registry(extra: Value) -> Value {
        let mut r = json!({ "pid": 42, "sessionId": "abc", "peerProtocol": 1, "messagingSocketPath": "/tmp/cc-socks/42.sock", "kind": "interactive" });
        r.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
        r
    }

    #[test]
    fn never_types_into_a_session_waiting_on_you() {
        let s = session(Harness::Claude, Surface::Cli, Status::Needs, true, Some(42));
        let err = route_for(&s, Some(&registry(json!({}))), Path::new("/h"), None).unwrap_err();
        assert!(err.to_string().contains("waiting on you"), "{err}");
    }

    #[test]
    fn answers_a_question_its_reply_asks_like_any_prompt() {
        let mut s = session(Harness::Claude, Surface::Cli, Status::Needs, true, Some(42));
        s.asks = true;
        let routes = route_for(&s, Some(&registry(json!({ "status": "idle" }))), Path::new("/h"), None).unwrap();
        assert_eq!(routes, vec![Route::Type { pid: 42 }, Route::Peer { socket: "/tmp/cc-socks/42.sock".into(), session_id: "abc".into(), mode: None }]);
    }

    #[test]
    fn types_into_a_terminal_session_and_falls_back_to_its_socket() {
        let s = session(Harness::Claude, Surface::Cli, Status::Working, true, Some(42));
        let routes = route_for(&s, Some(&registry(json!({}))), Path::new("/h"), None).unwrap();
        assert_eq!(routes, vec![Route::Type { pid: 42 }, Route::Peer { socket: "/tmp/cc-socks/42.sock".into(), session_id: "abc".into(), mode: None }]);
    }

    #[test]
    fn tells_the_socket_route_the_mode_the_session_runs_in() {
        let s = session(Harness::Claude, Surface::App, Status::Unread, true, Some(42));
        let routes = claude_routes(&s, Some(&registry(json!({ "entrypoint": "claude-desktop" }))), Path::new("/h"), None, Some("bypassPermissions".into()));
        assert_eq!(routes, vec![Route::Peer { socket: "/tmp/cc-socks/42.sock".into(), session_id: "abc".into(), mode: Some("bypassPermissions".into()) }]);
    }

    #[test]
    fn claims_the_sessions_class_only_for_a_sender_that_has_it() {
        let account: AccountCfg = serde_json::from_value(json!({ "name": "claude", "harness": "claude", "home": "/h" })).unwrap();
        let cfg = Config::default();
        let place = launch::Placement { new_terminal: cfg.apps.new_terminal, near: &[] };
        let app = session(Harness::Claude, Surface::App, Status::Unread, true, Some(42));
        // From another device, a Claude app session that skips prompts
        // would hold it where no one sees it: nothing is sent, and the error says why.
        let board = Board::default();
        let msg = |sender: &'static Sender| Message::new(&board, &cfg, "hi", sender);
        let route = Route::Peer { socket: "/nonexistent/cc.sock".into(), session_id: "abc".into(), mode: Some("bypassPermissions".into()) };
        let err = attempt(&route, &app, &account, &cfg, place, &msg(&Sender::Elsewhere), None).map(|_| ()).unwrap_err();
        assert!(err.to_string().contains("can't show that"), "{err}");
        // Nor from an agent whose own session asks for permission.
        static AGENT: Sender = Sender::Agent(None);
        let err = attempt(&route, &app, &account, &cfg, place, &msg(&AGENT), None).map(|_| ()).unwrap_err();
        assert!(err.to_string().contains("nothing was sent"), "{err}");
        // What you type goes to the socket (here there's none to answer).
        let err = attempt(&route, &app, &account, &cfg, place, &msg(&Sender::You), None).map(|_| ()).unwrap_err();
        assert!(err.to_string().contains("socket didn't answer"), "{err}");
        // So does an agent's whose session skips permission prompts too.
        let bypassing = Message { bypasses: true, ..msg(&AGENT) };
        let err = attempt(&route, &app, &account, &cfg, place, &bypassing, None).map(|_| ()).unwrap_err();
        assert!(err.to_string().contains("socket didn't answer"), "{err}");
        // A session that asks for permission takes it from anywhere.
        let route = Route::Peer { socket: "/nonexistent/cc.sock".into(), session_id: "abc".into(), mode: Some("default".into()) };
        let err = attempt(&route, &app, &account, &cfg, place, &msg(&Sender::Elsewhere), None).map(|_| ()).unwrap_err();
        assert!(err.to_string().contains("socket didn't answer"), "{err}");
        let err = attempt(&route, &app, &account, &cfg, place, &msg(&AGENT), None).map(|_| ()).unwrap_err();
        assert!(err.to_string().contains("socket didn't answer"), "{err}");
    }

    fn board_with(s: Session) -> Board {
        let mut lane: crate::model::Lane = serde_json::from_value(json!({
            "id": "/w", "project": "P", "repo": "r", "repoSlug": null, "path": "/w", "callsign": "w",
            "origin": "main", "branch": null, "detached": null, "bay": "idle", "glyph": "idle", "parked": false,
            "hidden": false, "headline": "", "detail": null, "sessions": [], "pr": null, "git": null,
            "lastActivity": 0, "targets": [], "defaultTarget": null
        }))
        .expect("a lane");
        lane.sessions = vec![s];
        Board { lanes: vec![lane], ..Board::default() }
    }

    #[test]
    fn an_agents_message_says_whose_it_is_and_how_to_answer() {
        let mut me = session(Harness::Claude, Surface::Cli, Status::Working, true, Some(7));
        me.key = "claude:claude2:me".into();
        me.account = "claude2".into();
        me.title = "Review the parser".into();
        let board = board_with(me);
        let b = byline(&board, Some("claude:claude2:me"));
        assert_eq!(
            b.intro,
            "This message is from another agent, “Review the parser” (Claude Code on claude2, session claude:claude2:me), sent with `codealong session send`; it isn't from your user. To answer it, run `codealong session send claude:claude2:me \"<your answer>\"`."
        );
        assert_eq!(b.name, "Review the parser");
        // A session the board doesn't show yet is named by its key.
        let b = byline(&board, Some("codex:codex:new"));
        assert!(b.intro.starts_with("This message is from another agent, session codex:codex:new, sent with") && b.name == "codex:codex:new", "{b:?}");
        // From a command line no session runs, it may be a person's or an agent's.
        let b = byline(&board, None);
        assert!(b.intro.contains("may be an agent's rather than your user's") && b.name == "codealong", "{b:?}");
        // The text starts with the byline, whichever way it goes; yours goes as it is.
        let agent = Sender::Agent(Some("claude:claude2:me".into()));
        let msg = Message::new(&board, &Config::default(), "  Which test fails?\n", &agent);
        assert_eq!(msg.text, format!("{}\n\nWhich test fails?", byline(&board, Some("claude:claude2:me")).intro));
        assert_eq!(msg.byline.map(|b| b.name).as_deref(), Some("Review the parser"));
        // A note of it in the session's panel leaves the byline out, either kind.
        assert_eq!(without_byline(&msg.text), "Which test fails?");
        assert_eq!(without_byline(&format!("{}\n\nAnd the docs?", byline(&board, None).intro)), "And the docs?");
        assert_eq!(without_byline("Plain words\n\nstay whole"), "Plain words\n\nstay whole");
        assert_eq!(Message::new(&board, &Config::default(), "hi", &Sender::You).text, "hi");
    }

    #[test]
    fn an_agents_envelope_names_its_session_and_claims_nothing() {
        let c = peer_content("This message is from another agent…\n\nhi", None, Some("Review the parser"));
        assert!(c.starts_with("<cross-session-message from-name=\"Review the parser\">\n"), "{c}");
        assert!(!c.contains("Your user sent this") && !c.contains("from-mode"), "{c}");
        // Names come out as Claude Code writes them again, or it wouldn't read the envelope.
        assert_eq!(envelope_name("Fix \"the\" <build>\tnow\u{200b}"), "Fix the build now");
        assert_eq!(envelope_name("  🙂  "), "codealong");
        assert_eq!(envelope_name("Déjà vu — “quoted” (#12)"), "Déjà vu — “quoted” (#12)");
        let long = envelope_name(&"word ".repeat(30));
        assert!(long.chars().count() <= 64 && long.starts_with("word word") && long.ends_with('…'), "{long}");
    }

    #[test]
    fn agents_reach_claude_code_only_on_its_socket() {
        let peer = Route::Peer { socket: "/tmp/cc-socks/42.sock".into(), session_id: "abc".into(), mode: None };
        let s = session(Harness::Claude, Surface::Cli, Status::Working, true, Some(42));
        assert_eq!(agent_routes(&s, vec![Route::Type { pid: 42 }, peer.clone()]).unwrap(), vec![peer.clone()]);
        let daemon = Route::Daemon { socket: "/tmp/d/control.sock".into(), key_file: "/h/daemon/control.key".into(), job: "6e07c149".into() };
        assert_eq!(agent_routes(&s, vec![daemon.clone(), peer.clone()]).unwrap(), vec![peer]);
        // Typing into its terminal would read as its user's words.
        let err = agent_routes(&s, vec![Route::Type { pid: 42 }]).unwrap_err();
        assert!(err.to_string().contains("doesn't take messages from other sessions"), "{err}");
        let err = agent_routes(&s, vec![daemon]).unwrap_err();
        assert!(err.to_string().contains("doesn't take messages from other sessions"), "{err}");
        // Other harnesses have no such socket; their terminal or service takes it.
        let codex = session(Harness::Codex, Surface::Cli, Status::Idle, true, Some(9));
        assert_eq!(agent_routes(&codex, vec![Route::Type { pid: 9 }]).unwrap(), vec![Route::Type { pid: 9 }]);
        let app = session(Harness::Codex, Surface::App, Status::Idle, true, None);
        assert_eq!(agent_routes(&app, vec![Route::CodexApp]).unwrap(), vec![Route::CodexApp]);
        let oc = session(Harness::Opencode, Surface::Cli, Status::Idle, true, Some(7));
        assert_eq!(agent_routes(&oc, vec![Route::Opencode]).unwrap(), vec![Route::Opencode]);
        // A message doesn't start a session again, or wait on someone to send it.
        for (s, route) in [
            (session(Harness::Claude, Surface::Cli, Status::Stopped, false, None), Route::Resume),
            (session(Harness::Claude, Surface::App, Status::Idle, false, None), Route::ClaudeApp),
            (session(Harness::Codex, Surface::Cli, Status::Stopped, false, None), Route::Resume),
        ] {
            let err = agent_routes(&s, vec![route]).unwrap_err();
            assert!(err.to_string().contains("isn't running"), "{err}");
        }
        let ag = session(Harness::Antigravity, Surface::App, Status::Idle, true, None);
        assert!(agent_routes(&ag, vec![Route::Antigravity]).unwrap_err().to_string().contains("clipboard"));
    }

    #[test]
    fn an_agent_waits_for_a_session_waiting_on_its_user() {
        let s = session(Harness::Claude, Surface::Cli, Status::Needs, true, Some(42));
        let err = ensure_not_waiting(&s, &Sender::Agent(None)).unwrap_err();
        assert!(err.to_string().contains("waiting on its user"), "{err}");
    }

    #[test]
    fn an_agent_leaves_no_codex_prompt_for_someone_to_send() {
        // No Codex app answers here, so the turn can't start; what you send would open the
        // thread with the prompt filled in, an agent's goes nowhere.
        let account: AccountCfg = serde_json::from_value(json!({ "name": "codex", "harness": "codex", "home": "/nonexistent/codex" })).unwrap();
        let cfg = Config::default();
        let place = launch::Placement { new_terminal: cfg.apps.new_terminal, near: &[] };
        let s = session(Harness::Codex, Surface::App, Status::Idle, true, None);
        let agent = Sender::Agent(None);
        let msg = Message::new(&Board::default(), &cfg, "hi", &agent);
        let err = attempt(&Route::CodexApp, &s, &account, &cfg, place, &msg, None).map(|_| ()).unwrap_err();
        assert!(err.to_string().contains("nothing was sent"), "{err}");
    }

    #[test]
    fn reaches_claude_app_sessions_only_through_their_socket() {
        let s = session(Harness::Claude, Surface::App, Status::Unread, true, Some(42));
        let routes = route_for(&s, Some(&registry(json!({ "entrypoint": "claude-desktop" }))), Path::new("/h"), None).unwrap();
        assert_eq!(routes, vec![Route::Peer { socket: "/tmp/cc-socks/42.sock".into(), session_id: "abc".into(), mode: None }]);
    }

    #[test]
    fn hands_background_jobs_to_the_daemon_first() {
        let s = session(Harness::Claude, Surface::Cli, Status::Idle, true, Some(42));
        let reg = registry(json!({ "kind": "bg", "jobId": "6e07c149" }));
        let routes = route_for(&s, Some(&reg), Path::new("/h"), Some(Path::new("/tmp/d/control.sock"))).unwrap();
        assert_eq!(
            routes,
            vec![
                Route::Daemon { socket: "/tmp/d/control.sock".into(), key_file: "/h/daemon/control.key".into(), job: "6e07c149".into() },
                Route::Peer { socket: "/tmp/cc-socks/42.sock".into(), session_id: "abc".into(), mode: None },
            ]
        );
        // Without the daemon running, the job's socket still takes it.
        let routes = route_for(&s, Some(&reg), Path::new("/h"), None).unwrap();
        assert_eq!(routes, vec![Route::Peer { socket: "/tmp/cc-socks/42.sock".into(), session_id: "abc".into(), mode: None }]);
    }

    #[test]
    fn skips_a_socket_of_a_protocol_it_doesnt_know() {
        let s = session(Harness::Claude, Surface::App, Status::Idle, true, Some(42));
        let routes = route_for(&s, Some(&registry(json!({ "peerProtocol": 2 }))), Path::new("/h"), None).unwrap();
        assert!(routes.is_empty());
    }

    #[test]
    fn resumes_sessions_that_arent_running() {
        let s = session(Harness::Claude, Surface::Cli, Status::Stopped, false, None);
        assert_eq!(route_for(&s, None, Path::new("/h"), None).unwrap(), vec![Route::Resume]);
        let s = session(Harness::Claude, Surface::App, Status::Idle, false, None);
        assert_eq!(route_for(&s, None, Path::new("/h"), None).unwrap(), vec![Route::ClaudeApp]);
        let s = session(Harness::Codex, Surface::App, Status::Working, true, None);
        assert_eq!(route_for(&s, None, Path::new("/h"), None).unwrap(), vec![Route::CodexApp]);
        let s = session(Harness::Opencode, Surface::Cli, Status::Idle, true, Some(7));
        assert_eq!(route_for(&s, None, Path::new("/h"), None).unwrap(), vec![Route::Opencode]);
    }

    #[test]
    fn finds_the_daemon_socket_by_the_config_dirs_hash() {
        // The first 8 hex of sha256 of the config folder, without a trailing slash (checked
        // against the folder the daemon made for a real ~/.claude2).
        assert_eq!(daemon_socket_path(Path::new("/Users/someone/.claude2"), 501), PathBuf::from("/tmp/cc-daemon-501/44bc50fb/control.sock"));
        assert_eq!(daemon_socket_path(Path::new("/Users/someone/.claude2/"), 501), PathBuf::from("/tmp/cc-daemon-501/44bc50fb/control.sock"));
    }

    #[test]
    fn writes_the_daemon_reply_as_one_line() {
        let line = daemon_request("k", "6e07c149", "two\nlines");
        assert!(line.ends_with('\n') && line.matches('\n').count() == 1);
        let v: Value = serde_json::from_str(line.trim()).unwrap();
        assert_eq!(v, json!({ "proto": 1, "op": "reply", "short": "6e07c149", "text": "two\nlines", "auth": "k" }));
    }

    /// A folder of its own under the system's temporary one, short enough for a socket path.
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ca-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn presses_escape_in_a_background_job_by_attaching_to_it() {
        let dir = scratch("attach");
        let socket = dir.join("c.sock");
        std::fs::write(dir.join("control.key"), "k\n").unwrap();
        let listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
        let daemon = std::thread::spawn(move || {
            let (mut conn, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(conn.try_clone().unwrap());
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            // The answer, then the job's screen as the daemon starts streaming it.
            conn.write_all(b"{\"ok\":true,\"op\":\"attach\"}\n\x1b[?1049h\x1b[2J screen").unwrap();
            let mut typed = Vec::new();
            reader.read_to_end(&mut typed).unwrap();
            (serde_json::from_str::<Value>(line.trim()).unwrap(), typed)
        });
        daemon_press_escape(&socket, &dir.join("control.key"), "6e07c149").unwrap();
        let (request, typed) = daemon.join().unwrap();
        assert_eq!((request["op"].as_str(), request["short"].as_str(), request["auth"].as_str()), (Some("attach"), Some("6e07c149"), Some("k")));
        // The daemon refuses a null for what it takes as an object or nothing.
        assert!(request.get("caps").is_none());
        assert_eq!(typed, b"\x1b");

        // A job the daemon doesn't have is refused, with its reason.
        let _ = std::fs::remove_file(&socket);
        let listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
        let daemon = std::thread::spawn(move || {
            let (mut conn, _) = listener.accept().unwrap();
            let mut line = String::new();
            BufReader::new(conn.try_clone().unwrap()).read_line(&mut line).unwrap();
            conn.write_all(b"{\"ok\":false,\"error\":\"job not found\",\"code\":\"ENOJOB\"}\n").unwrap();
        });
        let err = daemon_press_escape(&socket, &dir.join("control.key"), "6e07c149").unwrap_err();
        daemon.join().unwrap();
        assert!(err.to_string().contains("job not found"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn stops_a_codex_turn_through_the_window_that_owns_the_thread() {
        let home = scratch("codex");
        std::fs::create_dir_all(home.join("ipc")).unwrap();
        let listener = std::os::unix::net::UnixListener::bind(home.join("ipc/ipc.sock")).unwrap();
        let router = std::thread::spawn(move || {
            let (mut conn, _) = listener.accept().unwrap();
            let mut buf = Vec::new();
            let mut chunk = [0u8; 4096];
            loop {
                while let Some(m) = ipc_take(&mut buf) {
                    let id = m["requestId"].clone();
                    let answer = match m["method"].as_str() {
                        Some("initialize") => json!({ "type": "response", "requestId": id, "resultType": "success", "result": { "clientId": "me" } }),
                        Some("thread-owner-discovery") => json!({ "type": "response", "requestId": id, "resultType": "success", "handledByClientId": "window" }),
                        _ => {
                            conn.write_all(&ipc_frame(&json!({ "type": "response", "requestId": id, "resultType": "success", "result": { "ok": true } }))).unwrap();
                            return m;
                        }
                    };
                    conn.write_all(&ipc_frame(&answer)).unwrap();
                }
                let n = conn.read(&mut chunk).unwrap();
                assert!(n > 0, "Codealong hung up before asking to stop");
                buf.extend_from_slice(&chunk[..n]);
            }
        });
        codex_interrupt_turn(&home, "t1").unwrap();
        let asked = router.join().unwrap();
        assert_eq!(asked["method"], "thread-follower-interrupt-turn");
        assert_eq!((&asked["version"], &asked["sourceClientId"], &asked["targetClientId"]), (&json!(3), &json!("me"), &json!("window")));
        assert_eq!(asked["params"], json!({ "conversationId": "t1", "mode": "user-stop" }));
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn offers_a_stop_only_while_a_turn_runs_where_codealong_can_reach_it() {
        use crate::harness::stoppable;
        let hosted = |status, live, stalled: Option<i64>| Session { hosted: true, stalled_since: stalled, ..session(Harness::Claude, Surface::Cli, status, live, None) };
        assert!(stoppable(&hosted(Status::Working, true, None), None));
        // Stuck mid-turn, on a command waiting for input, say.
        assert!(stoppable(&hosted(Status::Needs, true, Some(1)), None));
        assert!(!stoppable(&hosted(Status::Idle, true, None), None));
        assert!(!stoppable(&hosted(Status::Working, false, None), None));

        let home = scratch("stoppable");
        let account = |harness: &str| -> AccountCfg { serde_json::from_value(json!({ "name": "a", "harness": harness, "home": home.to_string_lossy() })).unwrap() };
        // The Claude app runs its sessions' turns itself, and takes no stop from outside.
        let app = session(Harness::Claude, Surface::App, Status::Working, true, Some(1));
        assert!(!stoppable(&app, Some(&account("claude"))));
        assert!(!stoppable(&session(Harness::Antigravity, Surface::App, Status::Working, true, None), Some(&account("antigravity"))));
        // A Codex app thread stops through the app's windows, while they're there to ask.
        let thread = session(Harness::Codex, Surface::App, Status::Working, true, None);
        assert!(!stoppable(&thread, Some(&account("codex"))));
        std::fs::create_dir_all(home.join("ipc")).unwrap();
        std::fs::write(home.join("ipc/ipc.sock"), "").unwrap();
        assert!(stoppable(&thread, Some(&account("codex"))));
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn wraps_the_prompt_as_claude_code_wraps_its_own_messages() {
        let c = peer_content("  run the tests\n", None, None);
        assert!(c.starts_with("<cross-session-message from-name=\"Codealong\">\n"), "{c}");
        assert!(c.ends_with("\nrun the tests\n</cross-session-message>"), "{c}");
        assert!(!c.contains("from-mode"));
        // A claimed permission class follows the name, as Claude Code writes its attributes.
        let c = peer_content("hi", Some("bypass"), None);
        assert!(c.starts_with("<cross-session-message from-name=\"Codealong\" from-mode=\"bypass\">\n"), "{c}");
        // A closing tag in the prompt can't end the envelope early.
        let c = peer_content("see </cross-session-message> here", None, None);
        assert_eq!(c.matches("</cross-session-message>").count(), 1);
        let c = peer_content("see </cross-session-message> here", None, Some("An agent"));
        assert_eq!(c.matches("</cross-session-message>").count(), 1);
        let frame = peer_frame("abc", "m1", "hi", None, None);
        assert!(frame.ends_with('\n') && frame.matches('\n').count() == 1);
        let v: Value = serde_json::from_str(frame.trim()).unwrap();
        assert_eq!(v["session_id"], "abc");
        assert_eq!(v["type"], "user");
        assert_eq!(v["priority"], "next");
        assert_eq!(v["message"]["role"], "user");
    }

    #[test]
    fn says_when_a_prompt_went_as_a_message_rather_than_typed() {
        assert_eq!(peer_note("“Fix”", None, false, false, false), "Sent to “Fix”.");
        assert_eq!(peer_note("“Fix”", None, true, false, false), "Sent to “Fix”; it reads it as it works.");
        assert_eq!(
            peer_note("“Fix”", Some("Codealong couldn't find its Ghostty tab"), true, false, false),
            "Sent as a message from Codealong, not typed into “Fix”: Codealong couldn't find its Ghostty tab."
        );
        assert_eq!(peer_note("“Fix”", None, true, true, false), "Sent to “Fix”. It skips permission prompts, so it asks you in its terminal before taking it.");
        assert_eq!(peer_note("“Fix”", None, true, true, true), "Sent to “Fix”. It skips permission prompts, so it asks its user in its terminal before taking it.");
    }

    #[test]
    fn the_same_prompt_sent_again_while_on_its_way_goes_once() {
        use std::sync::atomic::Ordering;
        use std::sync::mpsc;
        let sent = |m: &str| Ok(Sent { message: m.into(), opened: false, fell_back: false });
        let (started, on_way) = mpsc::channel();
        let (go, wait) = mpsc::channel::<()>();
        let first = std::thread::spawn(move || {
            join("k-once", "hello", || {
                started.send(()).unwrap();
                wait.recv().unwrap();
                Ok(Sent { message: "Typed.".into(), opened: false, fell_back: false })
            })
        });
        on_way.recv().unwrap();
        let second = std::thread::spawn(|| join("k-once", "hello", || panic!("sent twice")));
        let flight = FLIGHTS.lock().unwrap().iter().find(|f| f.key == "k-once").cloned().unwrap();
        let t = Instant::now();
        while flight.joined.load(Ordering::SeqCst) == 0 {
            assert!(t.elapsed() < Duration::from_secs(30), "the second send never joined");
            std::thread::sleep(Duration::from_millis(5));
        }
        // Other text, or the same text to another session, goes on its own meanwhile.
        assert_eq!(join("k-once", "other", || sent("Other.")).unwrap().message, "Other.");
        assert_eq!(join("k-else", "hello", || sent("Else.")).unwrap().message, "Else.");
        go.send(()).unwrap();
        assert_eq!(first.join().unwrap().unwrap().message, "Typed.");
        assert_eq!(second.join().unwrap().unwrap().message, "Typed.");
        // Once it has landed, the same text goes again.
        assert_eq!(join("k-once", "hello", || sent("Again.")).unwrap().message, "Again.");
        // A send that fails lands too, so trying again sends it.
        assert!(join("k-fail", "x", || Err(anyhow!("Ghostty said no."))).unwrap_err().to_string().contains("Ghostty said no."));
        assert_eq!(join("k-fail", "x", || sent("Sent.")).unwrap().message, "Sent.");
    }

    #[test]
    fn frames_codex_ipc_messages() {
        let m = json!({ "type": "request", "requestId": "1" });
        let mut buf = ipc_frame(&m);
        assert_eq!(u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize, buf.len() - 4);
        buf.extend(ipc_frame(&json!({ "n": 2 })));
        let half = buf.split_off(buf.len() - 3);
        assert_eq!(ipc_take(&mut buf), Some(m));
        assert_eq!(ipc_take(&mut buf), None);
        buf.extend(half);
        assert_eq!(ipc_take(&mut buf), Some(json!({ "n": 2 })));
        assert!(buf.is_empty());
        let r = codex_start_request("r", "me", "owner", "t1", "hello", "u1", None);
        assert_eq!(r["version"], 2);
        assert_eq!(r["targetClientId"], "owner");
        assert_eq!(r["params"]["turnStart"]["request"]["input"][0], json!({ "type": "text", "text": "hello", "text_elements": [] }));
        assert!(r["params"]["turnStart"]["request"].get("model").is_none());
        let r = codex_start_request("r", "me", "owner", "t1", "hello", "u1", Some(("gpt-x", Some("high"))));
        assert_eq!((&r["params"]["turnStart"]["request"]["model"], &r["params"]["turnStart"]["request"]["effort"]), (&json!("gpt-x"), &json!("high")));
    }

    #[test]
    fn resumes_with_the_prompt_in_the_environment() {
        let account = AccountCfg { name: "claude2".into(), harness: "claude".into(), home: "~/.claude2".into(), command: None, start: None, preferred: false, acp: None };
        let (input, env) = resume_input(&account, "abc", "-v please");
        assert_eq!(input, format!("claude2 --resume abc \"${}\"\n", start::PROMPT_VAR));
        assert_eq!(env, vec![(start::PROMPT_VAR.to_string(), " -v please".to_string())]);
        let codex = AccountCfg { name: "codex".into(), harness: "codex".into(), home: "~/.codex".into(), command: Some("codex".into()), start: None, preferred: false, acp: None };
        let (input, env) = resume_input(&codex, "01a0e379", "-v please");
        assert_eq!(input, format!("codex resume -- 01a0e379 \"${}\"\n", start::PROMPT_VAR));
        assert_eq!(env, vec![(start::PROMPT_VAR.to_string(), "-v please".to_string())]);
    }
}
