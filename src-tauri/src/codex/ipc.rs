//! The socket the Codex app's windows use to reach each other (`<CODEX_HOME>/ipc/ipc.sock`):
//! starting a turn in a thread, which is a way in (`InApp`), sending a prompt into the turn it's
//! taking, stopping one, and setting the model and effort its next turn runs on, through the
//! window that owns the thread.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Result};
use serde_json::{json, Value};

use crate::attachments::{self, Style};
use crate::codex::{appserver, models};
use crate::config::AccountCfg;
use crate::harness::Switch;
use crate::launch;
use crate::model::Session;
use crate::procs;
use crate::send::{opened, sent, Delivery, MayHaveArrived, Own, Sent, Way};

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
    let (_, input) = appserver::input_items(text);
    let mut request = json!({
        "threadId": thread,
        "input": input,
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
        let init = crate::start::new_session_id();
        ipc.send(&json!({ "type": "request", "requestId": init, "method": "initialize", "params": { "clientType": "codealong" } }))?;
        let r = ipc.response(&init, deadline)?;
        ipc.client_id = r.pointer("/result/clientId").and_then(Value::as_str).ok_or_else(|| anyhow!("Codex didn't take Codealong as a client."))?.to_string();
        Ok(ipc)
    }

    /// The window that has `thread` loaded, if one answers by `wait`. A window owns only the
    /// threads it has loaded, and asking about any other goes unanswered until the router gives
    /// up ten seconds later.
    fn owner(&mut self, thread: &str, wait: Instant) -> Result<Option<String>> {
        let find = crate::start::new_session_id();
        self.send(&json!({
            "type": "request", "requestId": find, "sourceClientId": self.client_id, "version": 1,
            "method": "thread-owner-discovery", "params": { "hostId": "local", "conversationId": thread },
        }))?;
        Ok(self.response(&find, wait).ok().filter(|r| r.get("resultType").and_then(Value::as_str) == Some("success")).and_then(|r| r.get("handledByClientId").and_then(Value::as_str).map(String::from)))
    }

    /// The window that has `thread` loaded; when none has, the thread is opened in the app after
    /// a short wait, which loads it and takes it within a couple of seconds, and asked about
    /// again until `deadline`.
    fn owner_opening(&mut self, codex_home: &Path, thread: &str, deadline: Instant) -> Result<String> {
        let mut opened = false;
        // Once a look finds no window: whether more than one router listens (asked once), and
        // while that's so, whether the app's log says a window has taken the thread, before the
        // open or since. One that has it answers at once if it can be reached, so from then
        // Codealong waits only the couple of seconds a window takes to load it, in case it had
        // let it go.
        let mut routers = None;
        let mut split = false;
        let mut by = deadline;
        loop {
            let wait = (Instant::now() + Duration::from_millis(1500)).min(by);
            if let Some(owner) = self.owner(thread, wait)? {
                if opened {
                    // A window takes a thread as it starts loading it, and a turn that arrives
                    // before its settings are in goes out with an empty model (refused) and the
                    // app's default reviewer. They're in within about a second.
                    std::thread::sleep(Duration::from_secs(2));
                }
                return Ok(owner);
            }
            if !split && *routers.get_or_insert_with(|| routers_split(codex_home)) && taken_in_log(&app_logs(), thread) {
                split = true;
                by = (Instant::now() + Duration::from_secs(3)).min(deadline);
            }
            if Instant::now() >= by {
                bail!(if split { SPLIT } else { "No Codex window took this thread." });
            }
            if !opened {
                launch::open_codex(&launch::codex_thread_url(thread))?;
                opened = true;
            }
            std::thread::sleep(Duration::from_millis(300));
        }
    }
}

/// Why no window answered for a thread when the app's windows can't reach each other.
const SPLIT: &str = "Codex's windows can't reach each other right now, so the one with this thread didn't answer; quitting and reopening Codex fixes that.";

/// How many routers listen at `socket`, from `netstat -an -f unix`: a socket bound there lists
/// the vnode it's bound to (Inode, the fifth column), a connection accepted from one lists none,
/// and both end with the path.
fn routers_at(netstat: &str, socket: &Path) -> usize {
    let path = socket.to_string_lossy();
    netstat
        .lines()
        .filter(|l| l.strip_suffix(&*path).is_some_and(|rest| rest.ends_with(' ')))
        .filter(|l| l.split_whitespace().nth(4).is_some_and(|inode| inode != "0"))
        .count()
}

/// Whether more than one router listens at the app's socket, so that a window may own a thread
/// and still not be reached. Each of the app's clients has a router manager of its own (26.930's
/// `getOrStartRouterEndpoint`), and each that finds no router, as the app starts or when the
/// process running it quits, starts one: it removes the socket the one before it made and listens
/// on a new one. The path leads only to the last, and a client that joined an earlier one stays
/// with it until the app is reopened. A second router is usual, then, and most often no window is
/// left on it (two after a fresh start, 2026-10-10); `taken_in_log` tells a window that is.
fn routers_split(codex_home: &Path) -> bool {
    procs::run("/usr/sbin/netstat", &["-an", "-f", "unix"], None, Duration::from_secs(3)).is_ok_and(|out| routers_at(&out, &codex_home.join("ipc/ipc.sock")) > 1)
}

/// Where the Codex app logs: a folder a day (`<year>/<month>/<day>`), and in it a file a day for
/// each of the app's processes, named with its pid (`codex-desktop-<launch>-<pid>-t0-…`).
fn app_logs() -> PathBuf {
    crate::util::home().join("Library/Logs/com.openai.codex")
}

/// Whether a window of the running Codex app has taken `thread`, by the app's log in `logs`: a
/// window logs `thread_stream_role_changed conversationId=<thread> … role=owner` as it takes a
/// thread it loads (26.930), and nothing as it lets one go. Only the running app's files count,
/// so a thread a window had before the app was reopened doesn't.
fn taken_in_log(logs: &Path, thread: &str) -> bool {
    if thread.is_empty() || !thread.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return false;
    }
    let Ok(pids) = procs::run("/usr/bin/pgrep", &["-f", r"/(ChatGPT|Codex)\.app/Contents/MacOS/"], None, Duration::from_secs(3)) else {
        return false;
    };
    let files: Vec<String> = pids.split_whitespace().map(|pid| format!("--include=*-{pid}-t*")).collect();
    if files.is_empty() {
        return false;
    }
    let taken = format!("thread_stream_role_changed conversationId={thread} .* role=owner( |$)");
    let logs = logs.to_string_lossy();
    let mut args = vec!["-rqE"];
    args.extend(files.iter().map(String::as_str));
    args.extend([taken.as_str(), &*logs]);
    procs::run("/usr/bin/grep", &args, None, Duration::from_secs(5)).is_ok()
}

/// The error for a request the router took and may have handed to the window: it may have gone.
fn unheard(asked: &str, why: &str) -> MayHaveArrived {
    MayHaveArrived(format!("Codealong asked Codex to {asked} and didn't hear how it went ({why}): look there before sending it again."))
}

/// The router says these once it has handed a request to the window, which may go on with it;
/// its other refusals, and the window's own, come before anything is sent.
fn handed_on(why: &str) -> bool {
    matches!(why, "request-timeout" | "client-disconnected" | "server-closed")
}

/// Starts a turn with `text` in a Codex thread, through the socket the app's windows use to
/// reach each other. The window that owns the thread runs the turn in the app's own app-server,
/// so the app shows it as if you'd sent it there. A thread no window has loaded is opened in the
/// app after a short wait, which loads it and takes it within a couple of seconds, and asked
/// about again. Once the window has the request, no answer may mean it went (`MayHaveArrived`).
pub fn codex_start_turn(codex_home: &Path, thread: &str, text: &str, model: Option<(&str, Option<&str>)>) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(18);
    let mut ipc = Ipc::connect(codex_home, deadline)?;
    let client = ipc.client_id.clone();
    let owner = ipc.owner_opening(codex_home, thread, deadline)?;
    let turn = crate::start::new_session_id();
    ipc.send(&codex_start_request(&turn, &client, &owner, thread, text, &crate::start::new_session_id(), model))?;
    let r = ipc.response(&turn, deadline).map_err(|e| unheard("start a turn with this", &e.to_string()))?;
    if r.get("resultType").and_then(Value::as_str) != Some("success") {
        let why = r.get("error").and_then(Value::as_str).unwrap_or("no reason given");
        if handed_on(why) {
            return Err(unheard("start a turn with this", why).into());
        }
        bail!("Codex didn't start the turn: {why}");
    }
    Ok(())
}

/// The request that sends a prompt into the turn a thread is taking, sent to the window that
/// owns it: what the app sends when you steer from a window that doesn't (26.924's code; version
/// 1). `restoreMessage` is the composer's message as the app makes one for a plain prompt,
/// which the window reads the thread's folders from and puts back in the box if the steer is
/// turned down.
pub fn codex_steer_request(request_id: &str, client_id: &str, owner: &str, thread: &str, text: &str, message_id: &str, cwd: &str, at: i64) -> Value {
    let (prose, input) = appserver::input_items(text);
    json!({
        "type": "request",
        "requestId": request_id,
        "sourceClientId": client_id,
        "version": 1,
        "method": "thread-follower-steer-turn",
        "targetClientId": owner,
        "timeoutMs": 10_000,
        "params": {
            "conversationId": thread,
            "clientUserMessageId": message_id,
            "input": input,
            "attachments": [],
            "restoreMessage": {
                "id": message_id,
                "text": prose,
                "context": { "prompt": prose, "addedFiles": [], "fileAttachments": [], "ideContext": null, "imageAttachments": [], "workspaceRoots": [cwd] },
                "cwd": cwd,
                "createdAt": at,
            },
        },
    })
}

/// Sends `text` into the turn a Codex thread is taking in the app, through the window that runs
/// it, as the app's own steer does: the turn takes it in at its next step. A thread no window
/// has loaded isn't running in the app, and one whose turn has ended is turned down; either way
/// nothing went. No answer once it's asked may mean it went (`MayHaveArrived`).
pub fn codex_steer_turn(codex_home: &Path, thread: &str, text: &str, cwd: &str) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(12);
    let mut ipc = Ipc::connect(codex_home, deadline)?;
    let Some(owner) = ipc.owner(thread, (Instant::now() + Duration::from_secs(3)).min(deadline))? else {
        bail!(not_running(codex_home, thread));
    };
    let id = crate::start::new_session_id();
    ipc.send(&codex_steer_request(&id, &ipc.client_id.clone(), &owner, thread, text, &crate::start::new_session_id(), cwd, crate::util::now_ms()))?;
    let asked = "send this into its running turn";
    let r = ipc.response(&id, deadline).map_err(|e| unheard(asked, &e.to_string()))?;
    if r.get("resultType").and_then(Value::as_str) != Some("success") {
        let why = r.get("error").and_then(Value::as_str).unwrap_or("no reason given");
        if handed_on(why) {
            return Err(unheard(asked, why).into());
        }
        bail!("Codex didn't take it into the turn: {why}");
    }
    Ok(())
}

/// Why no window answered for a thread that should be running in the app: a thread that's
/// running there is loaded in some window, so none claiming it means it isn't, unless the one
/// that took it can't be reached.
fn not_running(codex_home: &Path, thread: &str) -> &'static str {
    if routers_split(codex_home) && taken_in_log(&app_logs(), thread) {
        SPLIT
    } else {
        "No Codex window has this thread open, so it isn't running in the app."
    }
}

/// Whether the Codex app queues what's sent while a turn runs, to start once the turn ends,
/// rather than steering it into the turn: `[desktop] followUpQueueMode` in the account's
/// config.toml, the app's setting for it (26.1007), which is `steer` unless set. Its third,
/// `interrupt`, stops the turn first, which Codealong leaves to Stop, so it steers.
fn queues_follow_ups(codex_home: &Path) -> bool {
    std::fs::read_to_string(codex_home.join("config.toml"))
        .ok()
        .and_then(|s| toml::from_str::<toml::Table>(&s).ok())
        .is_some_and(|t| t.get("desktop").and_then(|d| d.get("followUpQueueMode")).and_then(|m| m.as_str()) == Some("queue"))
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

/// Stops the turn a Codex thread is taking in the app, through the window that runs it.
pub fn codex_interrupt_turn(codex_home: &Path, thread: &str) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(12);
    let mut ipc = Ipc::connect(codex_home, deadline)?;
    let Some(owner) = ipc.owner(thread, (Instant::now() + Duration::from_secs(3)).min(deadline))? else {
        bail!(not_running(codex_home, thread));
    };
    let id = crate::start::new_session_id();
    ipc.send(&codex_interrupt_request(&id, &ipc.client_id.clone(), &owner, thread))?;
    let r = ipc.response(&id, deadline)?;
    if r.get("resultType").and_then(Value::as_str) != Some("success") {
        bail!("Codex didn't stop the turn: {}", r.get("error").and_then(Value::as_str).unwrap_or("no reason given"));
    }
    Ok(())
}

/// The request that sets what a thread's next turn runs on, sent to the window that owns it:
/// what a window that doesn't sends as you pick a model or an effort level in its box (26.930's
/// `updateThreadSettingsForNextTurn`, version 2). The owner sends them to its app-server's
/// `thread/settings/update`, which takes a setting left out as unchanged and writes the thread's
/// settings to its rollout as `thread_settings_applied` (rollout.rs `apply_event`), and its box
/// shows them. Without `activeTurnId` they're for the next turn, a running one's too; with it the
/// window changes the running turn's permissions instead. It answers `applied`, false only when
/// a `condition` sent with them (`ifModelEquals`, `ifEffortEquals`) didn't hold.
pub fn codex_settings_request(request_id: &str, client_id: &str, owner: &str, thread: &str, model: Option<&str>, effort: Option<&str>) -> Value {
    let mut settings = json!({});
    if let Some(m) = model {
        settings["model"] = json!(m);
    }
    if let Some(e) = effort {
        settings["effort"] = json!(e);
    }
    json!({
        "type": "request",
        "requestId": request_id,
        "sourceClientId": client_id,
        "version": 2,
        "method": "thread-follower-update-thread-settings",
        "targetClientId": owner,
        "timeoutMs": 10_000,
        "params": { "conversationId": thread, "threadSettings": settings },
    })
}

/// Sets the model and effort level a Codex thread's next turn in the app runs on, through the
/// window that has it, as picking them in its box does; a thread no window has is opened in the
/// app first, as a send's is.
pub fn codex_update_settings(codex_home: &Path, thread: &str, model: Option<&str>, effort: Option<&str>) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(18);
    let mut ipc = Ipc::connect(codex_home, deadline)?;
    let owner = ipc.owner_opening(codex_home, thread, deadline)?;
    ipc.update_settings(&owner, thread, model, effort, deadline)
}

impl Ipc {
    /// Sets what `thread`'s next turn runs on, through `owner`, the window that has it.
    fn update_settings(&mut self, owner: &str, thread: &str, model: Option<&str>, effort: Option<&str>, deadline: Instant) -> Result<()> {
        let id = crate::start::new_session_id();
        self.send(&codex_settings_request(&id, &self.client_id.clone(), owner, thread, model, effort))?;
        let r = self.response(&id, deadline)?;
        if r.get("resultType").and_then(Value::as_str) != Some("success") {
            bail!("Codex didn't take it: {}", r.get("error").and_then(Value::as_str).unwrap_or("no reason given"));
        }
        if r.pointer("/result/applied") == Some(&json!(false)) {
            bail!("Codex didn't take it.");
        }
        Ok(())
    }
}

/// The model and level a switch sends a thread in the app (`InApp`'s `switch`): a model with the
/// level the thread is on, or the nearest one the model takes, and a level, the nearest the
/// thread's model takes (`models::nearest_effort`), both as its rollout says now.
fn switch_settings(account: &AccountCfg, session: &Session, to: Switch) -> (Option<String>, Option<String>) {
    let home = account.home_path();
    let (model_now, effort_now) = super::settings_now(account, session);
    match to {
        Switch::Model(m) => (Some(m.to_string()), effort_now.as_deref().map(|e| models::nearest_effort(&home, m, e).to_string())),
        Switch::Effort(e) => (None, Some(model_now.as_deref().map_or(e, |m| models::nearest_effort(&home, m, e)).to_string())),
    }
}

/// How queueing a send in the app went, as its answer: queued under `id`, which the queue lists it
/// by (`Sent::queued`), so a window knows it there; maybe queued, as sent, unsure
/// (`send::sent_unsure`), under the same id, which the queue lists if Codex kept it; or None when
/// Codex didn't take it, for the turn to take it instead (an app without the queue).
fn queued_as(went: Result<()>, id: String, title: &str) -> Option<Result<Sent>> {
    match went {
        Ok(()) => Some(Ok(Sent { queued: Some(id), ..sent(format!("Queued for {title} in Codex: it starts once its current turn ends.")) })),
        Err(e) if e.is::<MayHaveArrived>() => Some(crate::send::sent_unsure(e).map(|s| Sent { queued: Some(id), ..s })),
        Err(_) => None,
    }
}

/// The way in for a thread the app has: a turn started through the app's own windows, as if
/// you'd sent it there. When that doesn't work nothing was sent, and the send fails, so the
/// prompt stays in the box it was sent from; the app opens the thread with it filled in too, to
/// send from there.
#[derive(Debug, Clone, PartialEq)]
pub struct InApp;

impl Own for InApp {
    fn way(&self) -> Way {
        Way::CodexApp
    }

    fn takes_agents(&self) -> bool {
        true
    }

    fn deliver(&self, d: &Delivery) -> Result<Sent> {
        let title = d.title();
        let prompt = attachments::expand(d.text, Style::Path);
        // Going on there with nothing to say yet (`Adapter::app_way`): the thread opens.
        if prompt.trim().is_empty() {
            launch::open_codex(&launch::codex_thread_url(&d.session.id))?;
            return Ok(opened(format!("Codex has {title} open.")));
        }
        // While a turn runs, your send goes as the app's own box would send it (`queues_follow_ups`),
        // and Send now into the turn. Another agent's message, or Codealong's, goes into the turn
        // too, as Claude Code takes one between its steps: queued, it waited for the queue's next
        // turn in the app, minutes into a long one (2026-10-13). A turn started meanwhile doesn't
        // wait for the running one: 26.1007's app-server puts it into that turn, and the window
        // shows it twice, in the turn and as the turn it took itself to be starting.
        let home = d.account.home_path();
        if d.busy() && !d.now && d.agent.is_none() && queues_follow_ups(&home) {
            let id = crate::start::new_session_id();
            if let Some(went) = queued_as(appserver::queue(&home, &d.session.id, &prompt, &id), id, &title) {
                return went;
            }
        }
        if d.busy() || d.now {
            match codex_steer_turn(&home, &d.session.id, &prompt, &d.session.cwd) {
                Ok(()) => return Ok(sent(format!("Sent{} to {title} in Codex: it takes it into the turn it's taking.", if d.now { " now" } else { "" }))),
                Err(e) if e.is::<MayHaveArrived>() => return Err(e),
                // Its turn ended meanwhile, or no window runs it: a turn starts on it, as any
                // send does.
                Err(_) => {}
            }
        }
        match codex_start_turn(&home, &d.session.id, &prompt, None) {
            Ok(()) => Ok(sent(format!("Sent to {title} in Codex."))),
            Err(e) if e.is::<MayHaveArrived>() => Err(e),
            // A prompt filled in for someone to send isn't a message an agent can leave.
            Err(e) if d.from_agent() => bail!("Codex didn't take it for {title} ({e}), so nothing was sent."),
            Err(e) => {
                // The app isn't running, no window took the thread, or the app has changed how
                // its windows talk: nothing went. The app's own link fills the prompt in to send
                // from there, and the send still fails, so the prompt stays in the box it was
                // sent from rather than passing for sent.
                let link = format!("{}?prompt={}", launch::codex_thread_url(&d.session.id), urlencoding::encode(&prompt));
                match launch::open_codex(&link) {
                    Ok(()) => bail!("{e} Nothing was sent to {title}: it's waiting in Codex's box for you to send there."),
                    Err(_) => bail!("{e} Nothing was sent to {title}."),
                }
            }
        }
    }

    fn switches(&self) -> bool {
        true
    }

    /// The app runs the thread's next turn on it, as a pick in its box does (`switch_settings`).
    fn switch(&self, session: &Session, account: &AccountCfg, to: Switch) -> Result<String> {
        let (model, effort) = switch_settings(account, session, to);
        codex_update_settings(&account.home_path(), &session.id, model.as_deref(), effort.as_deref())?;
        Ok(match to {
            Switch::Model(m) => m.to_string(),
            Switch::Effort(e) => effort.unwrap_or_else(|| e.to_string()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Harness, Status, Surface};

    /// A folder of its own under the system's temporary one, short enough for a socket path.
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ca-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A router on `home`'s socket whose window "window" has every thread: it takes Codealong
    /// in, and answers the first request for the window with `result`, handing that request back.
    fn window_router(home: &Path, result: Value) -> std::thread::JoinHandle<Value> {
        std::fs::create_dir_all(home.join("ipc")).unwrap();
        let listener = std::os::unix::net::UnixListener::bind(home.join("ipc/ipc.sock")).unwrap();
        std::thread::spawn(move || {
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
                            conn.write_all(&ipc_frame(&json!({ "type": "response", "requestId": id, "resultType": "success", "result": result }))).unwrap();
                            return m;
                        }
                    };
                    conn.write_all(&ipc_frame(&answer)).unwrap();
                }
                let n = conn.read(&mut chunk).unwrap();
                assert!(n > 0, "Codealong hung up before asking the window");
                buf.extend_from_slice(&chunk[..n]);
            }
        })
    }

    #[test]
    fn stops_a_codex_turn_through_the_window_that_owns_the_thread() {
        let home = scratch("codex");
        let router = window_router(&home, json!({ "ok": true }));
        codex_interrupt_turn(&home, "t1").unwrap();
        let asked = router.join().unwrap();
        assert_eq!(asked["method"], "thread-follower-interrupt-turn");
        assert_eq!((&asked["version"], &asked["sourceClientId"], &asked["targetClientId"]), (&json!(3), &json!("me"), &json!("window")));
        assert_eq!(asked["params"], json!({ "conversationId": "t1", "mode": "user-stop" }));
        let _ = std::fs::remove_dir_all(&home);
    }

    /// Asks the router for the window that has `t1`, and sets that thread's settings there. Not
    /// `codex_update_settings`, which on a busy Mac could give up on the router's answer and open
    /// the thread in the real Codex app.
    fn set_t1(home: &Path, model: Option<&str>, effort: Option<&str>) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut ipc = Ipc::connect(home, deadline)?;
        let owner = ipc.owner("t1", deadline)?.expect("the router names the window");
        ipc.update_settings(&owner, "t1", model, effort, deadline)
    }

    #[test]
    fn sets_a_codex_threads_next_turn_through_the_window_that_owns_it() {
        let home = scratch("codex-settings");
        let router = window_router(&home, json!({ "applied": true }));
        set_t1(&home, Some("gpt-x"), Some("low")).unwrap();
        let asked = router.join().unwrap();
        assert_eq!((&asked["method"], &asked["version"], &asked["targetClientId"]), (&json!("thread-follower-update-thread-settings"), &json!(2), &json!("window")));
        // No `activeTurnId`: a running turn's permissions aren't what's changed.
        assert_eq!(asked["params"], json!({ "conversationId": "t1", "threadSettings": { "model": "gpt-x", "effort": "low" } }));
        let _ = std::fs::remove_dir_all(&home);
        // A level alone leaves the model as it is, and a condition that didn't hold is no switch.
        let home = scratch("codex-settings-refused");
        let router = window_router(&home, json!({ "applied": false }));
        assert_eq!(set_t1(&home, None, Some("high")).unwrap_err().to_string(), "Codex didn't take it.");
        assert_eq!(router.join().unwrap()["params"]["threadSettings"], json!({ "effort": "high" }));
        let _ = std::fs::remove_dir_all(&home);
    }

    /// A model goes with the level the thread's rollout says now, not the board's: a level
    /// switched a moment before is written there before the board has read it, and a model
    /// switch with the board's would put the old level back.
    #[test]
    fn a_codex_model_switch_keeps_the_level_set_just_before() {
        let home = scratch("codex-switch");
        let day = home.join("sessions/2026/10/17");
        std::fs::create_dir_all(&day).unwrap();
        let lines = [
            r#"{"timestamp":"2026-10-17T10:00:00.000Z","type":"session_meta","payload":{"id":"t1","cwd":"/r","originator":"Codex Desktop"}}"#,
            r#"{"timestamp":"2026-10-17T10:00:01.000Z","type":"turn_context","payload":{"model":"gpt-x","effort":"xhigh"}}"#,
            r#"{"timestamp":"2026-10-17T10:05:00.000Z","type":"event_msg","payload":{"type":"thread_settings_applied","thread_settings":{"model":"gpt-x","reasoning_effort":"low"}}}"#,
        ];
        std::fs::write(day.join("rollout-2026-10-17T10-00-00-t1.jsonl"), lines.join("\n") + "\n").unwrap();
        let account: AccountCfg = serde_json::from_value(json!({ "name": "codex", "harness": "codex", "home": home })).unwrap();
        let s = Session { surface: Surface::App, model: Some("gpt-x".into()), effort: Some("xhigh".into()), title: "Fix the build".into(), cwd: "/r".into(), ..Session::new(Harness::Codex, "codex", "t1") };
        let some = |v: &str| Some(v.to_string());
        assert_eq!(switch_settings(&account, &s, Switch::Model("gpt-y")), (some("gpt-y"), some("low")));
        assert_eq!(switch_settings(&account, &s, Switch::Effort("high")), (None, some("high")));
        // Without its rollout, the board's.
        let gone = Session { id: "t2".into(), key: "codex:codex:t2".into(), ..s };
        assert_eq!(switch_settings(&account, &gone, Switch::Model("gpt-y")), (some("gpt-y"), some("xhigh")));
        let _ = std::fs::remove_dir_all(&home);
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

    /// What the Codex app's windows are written, byte for byte.
    #[test]
    fn the_codex_apps_windows_are_written_these_frames() {
        assert_eq!(ipc_frame(&json!({ "a": 1 })), b"\x07\x00\x00\x00{\"a\":1}");
        assert_eq!(
            codex_start_request("r1", "me", "window", "t1", "hello", "u1", Some(("gpt-x", Some("high")))).to_string(),
            concat!(
                r#"{"method":"thread-follower-start-turn","params":{"conversationId":"t1","turnStart":{"context":{},"request":{"clientUserMessageId":"u1","effort":"high","#,
                r#""input":[{"text":"hello","text_elements":[],"type":"text"}],"model":"gpt-x","threadId":"t1"}}},"#,
                r#""requestId":"r1","sourceClientId":"me","targetClientId":"window","timeoutMs":10000,"type":"request","version":2}"#
            )
        );
        assert_eq!(
            codex_interrupt_request("r2", "me", "window", "t1").to_string(),
            r#"{"method":"thread-follower-interrupt-turn","params":{"conversationId":"t1","mode":"user-stop"},"requestId":"r2","sourceClientId":"me","targetClientId":"window","timeoutMs":10000,"type":"request","version":3}"#
        );
        assert_eq!(
            codex_settings_request("r3", "me", "window", "t1", Some("gpt-x"), None).to_string(),
            r#"{"method":"thread-follower-update-thread-settings","params":{"conversationId":"t1","threadSettings":{"model":"gpt-x"}},"requestId":"r3","sourceClientId":"me","targetClientId":"window","timeoutMs":10000,"type":"request","version":2}"#
        );
    }

    /// Everything a turn's start writes on the windows' socket, in order, the ids Codealong makes
    /// up aside: it joins, finds the window that has the thread (telling the router meanwhile
    /// that it handles nothing itself), and starts the turn there.
    #[test]
    fn starts_a_codex_turn_through_the_window_that_owns_the_thread() {
        let home = scratch("codex-start");
        std::fs::create_dir_all(home.join("ipc")).unwrap();
        let listener = std::os::unix::net::UnixListener::bind(home.join("ipc/ipc.sock")).unwrap();
        let router = std::thread::spawn(move || {
            let (mut conn, _) = listener.accept().unwrap();
            let mut buf = Vec::new();
            let mut chunk = [0u8; 4096];
            let mut heard = Vec::new();
            loop {
                while let Some(m) = ipc_take(&mut buf) {
                    let id = m["requestId"].clone();
                    let method = m["method"].as_str().map(String::from);
                    heard.push(m);
                    let mut say = |v: Value| conn.write_all(&ipc_frame(&v)).unwrap();
                    match method.as_deref() {
                        Some("initialize") => say(json!({ "type": "response", "requestId": id, "resultType": "success", "result": { "clientId": "me" } })),
                        Some("thread-owner-discovery") => {
                            say(json!({ "type": "client-discovery-request", "requestId": "d1" }));
                            say(json!({ "type": "request", "requestId": "q1", "method": "something-else" }));
                            say(json!({ "type": "response", "requestId": id, "resultType": "success", "handledByClientId": "window" }));
                        }
                        Some("thread-follower-start-turn") => {
                            say(json!({ "type": "response", "requestId": id, "resultType": "success", "result": {} }));
                            return heard;
                        }
                        _ => {}
                    }
                }
                let n = conn.read(&mut chunk).unwrap();
                assert!(n > 0, "Codealong hung up before starting the turn");
                buf.extend_from_slice(&chunk[..n]);
            }
        });
        codex_start_turn(&home, "t1", "hello", None).unwrap();
        let mut heard = router.join().unwrap();
        // The ids are made up afresh each time; the answers to the router's own keep its ids.
        for m in heard.iter_mut().filter(|m| m.get("method").is_some()) {
            m["requestId"] = json!("id");
        }
        heard[4]["params"]["turnStart"]["request"]["clientUserMessageId"] = json!("id");
        let wrote: Vec<String> = heard.iter().map(Value::to_string).collect();
        assert_eq!(
            wrote,
            [
                r#"{"method":"initialize","params":{"clientType":"codealong"},"requestId":"id","type":"request"}"#,
                r#"{"method":"thread-owner-discovery","params":{"conversationId":"t1","hostId":"local"},"requestId":"id","sourceClientId":"me","type":"request","version":1}"#,
                r#"{"requestId":"d1","response":{"canHandle":false},"type":"client-discovery-response"}"#,
                r#"{"error":"no-handler-for-request","requestId":"q1","resultType":"error","type":"response"}"#,
                r#"{"method":"thread-follower-start-turn","params":{"conversationId":"t1","turnStart":{"context":{},"request":{"clientUserMessageId":"id","input":[{"text":"hello","text_elements":[],"type":"text"}],"threadId":"t1"}}},"requestId":"id","sourceClientId":"me","targetClientId":"window","timeoutMs":10000,"type":"request","version":2}"#,
            ]
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn asks_the_codex_window_to_steer_as_the_app_does() {
        let r = codex_steer_request("r", "me", "owner", "t1", "and the docs", "u1", "/r/app", 1_790_000_000_000);
        assert_eq!((&r["method"], &r["version"], &r["targetClientId"], &r["sourceClientId"]), (&json!("thread-follower-steer-turn"), &json!(1), &json!("owner"), &json!("me")));
        let p = &r["params"];
        assert_eq!(p["conversationId"], "t1");
        assert_eq!(p["input"], json!([{ "type": "text", "text": "and the docs", "text_elements": [] }]));
        assert_eq!(p["attachments"], json!([]));
        // The window reads the thread's folders from the message it would put back in the box,
        // and its id is the message's.
        let m = &p["restoreMessage"];
        assert_eq!((&m["id"], &p["clientUserMessageId"]), (&json!("u1"), &json!("u1")));
        assert_eq!((&m["text"], &m["cwd"], &m["createdAt"]), (&json!("and the docs"), &json!("/r/app"), &json!(1_790_000_000_000i64)));
        assert_eq!(m["context"], json!({ "prompt": "and the docs", "addedFiles": [], "fileAttachments": [], "ideContext": null, "imageAttachments": [], "workspaceRoots": ["/r/app"] }));
        // Attached images go as `localImage` items, taken out of the text.
        let img = attachments::images_dir().join("0123456789abcdef.png");
        let with_img = codex_steer_request("r", "me", "owner", "t1", "Match this:\n![image](images/0123456789abcdef.png)", "u1", "/r/app", 1_790_000_000_000);
        assert_eq!(with_img["params"]["input"], json!([{ "type": "text", "text": "Match this:", "text_elements": [] }, { "type": "localImage", "path": img }]));
        assert_eq!(with_img["params"]["restoreMessage"]["text"], "Match this:");
        let img_only = codex_start_request("r", "me", "owner", "t1", "![image](images/0123456789abcdef.png)", "u1", None);
        assert_eq!(img_only["params"]["turnStart"]["request"]["input"], json!([{ "type": "localImage", "path": img }]));
    }

    /// A router that answers Codealong's join and its lookups for a thread's window with
    /// `owner` (None: no window answers), and anything else with `started`, until it hangs up;
    /// then it says what it was asked.
    fn router(home: &Path, owner: Option<&'static str>, started: Value) -> std::thread::JoinHandle<Vec<String>> {
        std::fs::create_dir_all(home.join("ipc")).unwrap();
        let listener = std::os::unix::net::UnixListener::bind(home.join("ipc/ipc.sock")).unwrap();
        std::thread::spawn(move || {
            let (mut conn, _) = listener.accept().unwrap();
            let mut buf = Vec::new();
            let mut chunk = [0u8; 4096];
            let mut asked = Vec::new();
            loop {
                while let Some(m) = ipc_take(&mut buf) {
                    asked.push(m["method"].as_str().unwrap_or_default().to_string());
                    let id = m["requestId"].clone();
                    let mut answer = match (m["method"].as_str(), owner) {
                        (Some("initialize"), _) => json!({ "resultType": "success", "result": { "clientId": "me" } }),
                        (Some("thread-owner-discovery"), Some(window)) => json!({ "resultType": "success", "handledByClientId": window }),
                        (Some("thread-owner-discovery"), None) => json!({ "resultType": "error", "error": "no-client-found" }),
                        _ => started.clone(),
                    };
                    answer["type"] = json!("response");
                    answer["requestId"] = id;
                    // Codealong may have hung up meanwhile, once it gave up on the last.
                    if conn.write_all(&ipc_frame(&answer)).is_err() {
                        return asked;
                    }
                }
                match conn.read(&mut chunk) {
                    Ok(0) | Err(_) => return asked,
                    Ok(n) => buf.extend_from_slice(&chunk[..n]),
                }
            }
        })
    }

    /// Stands in for netstat, listing `routers` listening at `home`'s socket, for the app's
    /// process and its log, which says a window took the thread when `taken`, and for the links
    /// opened, which it keeps: the Codex app isn't running, so they open nothing.
    fn stand_ins(home: &Path, routers: usize, taken: bool) -> std::rc::Rc<std::cell::RefCell<Vec<String>>> {
        let socket = home.join("ipc/ipc.sock").to_string_lossy().into_owned();
        let opened = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let kept = opened.clone();
        crate::procs::stand_in::set(Some(Box::new(move |program, args, _, _| match program {
            "/usr/sbin/netstat" => {
                let mut out = String::from("Active LOCAL (UNIX) domain sockets\nAddress          Type   Recv-Q Send-Q            Inode             Conn             Refs          Nextref Addr\n");
                for n in 0..routers {
                    out += &format!("cd1b15dcda766bb{n} stream      0      0  42afbf88e136ef{n}                0                0                0 {socket}\n");
                    out += &format!("1eabd92b42207c5{n} stream      0      0                0 731306bd370ce13{n}                0                0 {socket}\n");
                }
                Ok(out)
            }
            "/usr/bin/open" => {
                kept.borrow_mut().push(args.last().unwrap().to_string());
                Ok(String::new())
            }
            "/usr/bin/pgrep" => Ok("4242\n".into()),
            "/usr/bin/grep" if taken => Ok(String::new()),
            _ => Err(anyhow!("{program} isn't here")),
        })));
        opened
    }

    #[test]
    fn counts_the_routers_listening_on_the_codex_socket() {
        let out = concat!(
            "Active LOCAL (UNIX) domain sockets\n",
            "Address          Type   Recv-Q Send-Q            Inode             Conn             Refs          Nextref Addr\n",
            "1eabd92b42207c55 stream      0      0                0 731306bd370ce138                0                0 /Users/me/.codex/ipc/ipc.sock\n",
            "cd1b15dcda766bb8 stream      0      0  42afbf88e136ef6                0                0                0 /Users/me/.codex/ipc/ipc.sock\n",
            " 12cde69c79bb8ac stream      0      0                0 c7a9a81174bf18c3                0                0 /Users/me/.codex/ipc/ipc.sock\n",
            "4488249790064287 stream      0      0 9d7dcf9987a383fd                0                0                0 /Users/me/.codex/ipc/ipc.sock\n",
            "9ee99f668c4053df stream      0      0 cbdf3c8b9c687c69                0                0                0 /Users/me/other/.codex/ipc/ipc.sock\n",
            "2d834054652471cf stream      0      0 5a1c0e3b9d2f4e11                0                0                0 /Users/me/Library/My Folder/ipc.sock\n",
        );
        assert_eq!(routers_at(out, Path::new("/Users/me/.codex/ipc/ipc.sock")), 2);
        assert_eq!(routers_at(out, Path::new("/Users/me/Library/My Folder/ipc.sock")), 1);
        assert_eq!(routers_at(out, Path::new("/Users/you/.codex/ipc/ipc.sock")), 0);
    }

    /// A window of the app took the thread, by its log, and isn't on the router Codealong
    /// reaches: Codealong says so, and why, a few seconds after opening the thread rather than
    /// eighteen.
    #[test]
    #[cfg(target_os = "macos")]
    fn says_when_the_codex_apps_windows_cant_reach_each_other() {
        let home = scratch("codex-split");
        let router = router(&home, None, json!({}));
        let opened = stand_ins(&home, 5, true);
        let began = Instant::now();
        let err = codex_start_turn(&home, "t1", "hello", None).unwrap_err();
        crate::procs::stand_in::set(None);
        router.join().unwrap();
        assert_eq!(err.to_string(), SPLIT);
        assert!(began.elapsed() < Duration::from_secs(6), "{:?}", began.elapsed());
        assert_eq!(opened.borrow().first().map(String::as_str), Some("codex://threads/t1"));
        let _ = std::fs::remove_dir_all(&home);
    }

    /// A router more than the one Codealong reaches is usual, and with no window that took the
    /// thread on it, a thread no window answers for is one no window has open.
    #[test]
    fn a_second_router_alone_isnt_taken_for_a_split() {
        let home = scratch("codex-routers");
        let router = router(&home, None, json!({}));
        stand_ins(&home, 2, false);
        let err = codex_steer_turn(&home, "t1", "and the docs", "/r/app").unwrap_err();
        crate::procs::stand_in::set(None);
        router.join().unwrap();
        assert_eq!(err.to_string(), "No Codex window has this thread open, so it isn't running in the app.");
        let _ = std::fs::remove_dir_all(&home);
    }

    /// The running app's log says which threads its windows took; another process's log, a
    /// window following a thread, and a thread whose id isn't one don't count.
    #[test]
    fn finds_the_threads_the_running_codex_apps_windows_took_in_its_log() {
        let logs = scratch("codex-logs");
        let day = logs.join("2026/10/10");
        std::fs::create_dir_all(&day).unwrap();
        let line = |thread: &str, role: &str| format!("2026-10-10T15:38:32.172Z info [electron-message-handler] thread_stream_role_changed conversationId={thread} ownerClientId=null previousOwnerClientId=null previousRole=null rendererWebContentsId=1 rendererWindowAppearance=primary rendererWindowFocused=true rendererWindowId=1 rendererWindowVisible=true role={role}\n");
        std::fs::write(day.join("codex-desktop-0286bd4b-4242-t0-i1-174939-0.log"), line("01a11e06", "owner") + &line("01a1229a", "follower")).unwrap();
        std::fs::write(day.join("codex-desktop-df0b953d-87025-t0-i1-000002-0.log"), line("01a122b1", "owner")).unwrap();
        // pgrep names the running app; grep is the real one.
        crate::procs::stand_in::set(Some(Box::new(|program, args, _, _| match program {
            "/usr/bin/pgrep" => Ok("4242\n".into()),
            _ if std::process::Command::new(program).args(args).status()?.success() => Ok(String::new()),
            _ => Err(anyhow!("not found")),
        })));
        let taken = |thread: &str| taken_in_log(&logs, thread);
        let found = (taken("01a11e06"), taken("01a1229a"), taken("01a122b1"), taken("01a11e06 .*"), taken(""));
        crate::procs::stand_in::set(None);
        assert_eq!(found, (true, false, false, false, false));
        let _ = std::fs::remove_dir_all(&logs);
    }

    /// Once the window has the request, a turn the router lost track of may have started: it's
    /// left to look at, not filled in to send again.
    #[test]
    fn a_turn_the_codex_window_was_handed_may_have_started() {
        let home = scratch("codex-handed");
        let router = router(&home, Some("window"), json!({ "resultType": "error", "error": "request-timeout" }));
        let err = codex_start_turn(&home, "t1", "hello", None).unwrap_err();
        router.join().unwrap();
        assert!(err.is::<MayHaveArrived>(), "{err}");
        let _ = std::fs::remove_dir_all(&home);
    }

    /// While a turn runs, a send goes into it, as the app's box sends one unless it's set to
    /// queue them: a turn started meanwhile would land in the running one anyway, and show twice.
    #[test]
    fn a_send_to_a_codex_thread_taking_a_turn_steers_into_it() {
        let home = scratch("codex-busy");
        let router = router(&home, Some("window"), json!({ "resultType": "success", "result": {} }));
        let account: AccountCfg = serde_json::from_value(json!({ "name": "codex", "harness": "codex", "home": home })).unwrap();
        let s = Session { surface: Surface::App, status: Status::Working, live: true, title: "Fix the build".into(), cwd: "/tmp".into(), ..Session::new(Harness::Codex, "codex", "abc") };
        let sent = InApp.deliver(&Delivery { session: &s, account: &account, text: "and the docs", agent: None, bypasses: false, now: false, untyped: None }).unwrap();
        assert_eq!(router.join().unwrap(), ["initialize", "thread-owner-discovery", "thread-follower-steer-turn"]);
        assert_eq!(sent.message, "Sent to \u{201c}Fix the build\u{201d} in Codex: it takes it into the turn it's taking.");
        let _ = std::fs::remove_dir_all(&home);
    }

    /// With the app set to queue follow-ups, yours is queued, but another agent's message goes into
    /// the turn, where it reads it at its next step.
    #[test]
    fn an_agents_message_to_a_codex_thread_taking_a_turn_steers_into_it_when_yours_would_queue() {
        let home = scratch("codex-busy-agent");
        std::fs::write(home.join("config.toml"), "[desktop]\nfollowUpQueueMode = \"queue\"\n").unwrap();
        let router = router(&home, Some("window"), json!({ "resultType": "success", "result": {} }));
        let account: AccountCfg = serde_json::from_value(json!({ "name": "codex", "harness": "codex", "home": home })).unwrap();
        let s = Session { surface: Surface::App, status: Status::Working, live: true, title: "Fix the build".into(), cwd: "/tmp".into(), ..Session::new(Harness::Codex, "codex", "abc") };
        let sent = InApp.deliver(&Delivery { session: &s, account: &account, text: "No conflict: go ahead.", agent: Some("Review the parser"), bypasses: false, now: false, untyped: None }).unwrap();
        assert_eq!(router.join().unwrap(), ["initialize", "thread-owner-discovery", "thread-follower-steer-turn"]);
        assert_eq!(sent.message, "Sent to \u{201c}Fix the build\u{201d} in Codex: it takes it into the turn it's taking.");
        let _ = std::fs::remove_dir_all(&home);
    }

    /// A send queued in the app answers the id it went under, which the queue lists it by; so does
    /// one Codex may have kept without saying so, as sent, unsure, so a window can still tell it
    /// among what's held. Codex turning it down leaves it to the turn.
    #[test]
    fn a_queued_send_answers_the_id_it_went_under_even_unsure() {
        let queued = queued_as(Ok(()), "u1".into(), "“Fix”").unwrap().unwrap();
        assert_eq!((queued.queued.as_deref(), queued.uncertain), (Some("u1"), false));
        let quiet = MayHaveArrived("Codealong asked Codex to queue this and didn't hear how it went: look there before sending it again.".into());
        let maybe = queued_as(Err(quiet.into()), "u2".into(), "“Fix”").unwrap().unwrap();
        assert_eq!((maybe.queued.as_deref(), maybe.uncertain, maybe.message.as_str()), (Some("u2"), true, "Codealong asked Codex to queue this and didn't hear how it went: look there before sending it again."));
        assert!(queued_as(Err(anyhow!("Codex didn't queue it: ephemeral thread does not support queued submissions")), "u3".into(), "“Fix”").is_none());
    }

    #[test]
    fn reads_whether_the_codex_app_queues_follow_ups() {
        let home = scratch("codex-follow-ups");
        let set = |config: &str| {
            std::fs::write(home.join("config.toml"), config).unwrap();
            queues_follow_ups(&home)
        };
        assert!(!queues_follow_ups(&home));
        assert!(set("model = \"gpt-6\"\n\n[desktop]\nfollowUpQueueMode = \"queue\"\n"));
        assert!(!set("[desktop]\nfollowUpQueueMode = \"steer\"\n"));
        assert!(!set("[desktop]\nfollowUpQueueMode = \"interrupt\"\n"));
        assert!(!set("followUpQueueMode = \"queue\"\n"));
        assert!(!set("[desktop\nfollowUpQueueMode = \"queue\"\n"));
        let _ = std::fs::remove_dir_all(&home);
    }

    /// A send Codex didn't take fails, so the prompt stays where it was written, and the app has
    /// it filled in to send from there.
    #[test]
    #[cfg(target_os = "macos")]
    fn a_codex_send_that_was_only_filled_in_is_not_sent() {
        let account: AccountCfg = serde_json::from_value(json!({ "name": "codex", "harness": "codex", "home": "/nonexistent/codex" })).unwrap();
        let s = Session { surface: Surface::App, status: Status::Idle, live: true, title: "Fix the build".into(), cwd: "/tmp".into(), ..Session::new(Harness::Codex, "codex", "abc") };
        let opened = stand_ins(Path::new("/nonexistent/codex"), 1, false);
        let err = InApp.deliver(&Delivery { session: &s, account: &account, text: "hi there", agent: None, bypasses: false, now: false, untyped: None }).map(|_| ()).unwrap_err();
        crate::procs::stand_in::set(None);
        assert_eq!(err.to_string(), "The Codex app isn't running. Nothing was sent to \u{201c}Fix the build\u{201d}: it's waiting in Codex's box for you to send there.");
        assert_eq!(*opened.borrow(), ["codex://threads/abc?prompt=hi%20there"]);
    }

    #[test]
    fn an_agent_leaves_no_codex_prompt_for_someone_to_send() {
        // No Codex app answers here, so the turn can't start; what you send would open the
        // thread with the prompt filled in, an agent's goes nowhere.
        let account: AccountCfg = serde_json::from_value(json!({ "name": "codex", "harness": "codex", "home": "/nonexistent/codex" })).unwrap();
        let s = Session { surface: Surface::App, status: Status::Idle, live: true, title: "Fix the build".into(), cwd: "/tmp".into(), ..Session::new(Harness::Codex, "codex", "abc") };
        let err = InApp.deliver(&Delivery { session: &s, account: &account, text: "hi", agent: Some("codealong"), bypasses: false, now: false, untyped: None }).map(|_| ()).unwrap_err();
        assert!(err.to_string().contains("nothing was sent"), "{err}");
    }
}
