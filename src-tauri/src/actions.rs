//! What a device does about its sessions and worktrees when asked, in one table (`REQUESTS`):
//! each request's name (what one device asks another: codealong-peer's `api`), what it does on
//! this device, how it crosses to another, and, for those that need someone at the Mac a session
//! or worktree is on, why another device can't ask for it. A window's command goes through
//! `run`, which does it here or asks the device that the session, the worktree or the command
//! names (`On`); a linked device's request goes through `answer`, which does it here; and the
//! `codealong` command's goes through `run_as` (the app's socket, server.rs) or, with the app
//! closed, `local`. So a request does the same whether a window here, another Mac, a phone or
//! the command asks. Who may ask for it from elsewhere is `api::REQUESTS`'s to say, once, and
//! what the command may not ask for is each request's `command` here.

use std::collections::BTreeMap;
use std::time::Duration;

use codealong_peer::api::{self, Reach};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::engine::Handle;
use crate::peers::Peers;
use crate::util::words;
use crate::{acp, config, harness, peerfeed, send, transcript};

/// What only happens on the device a session runs on: another device can't ask for them, so
/// they have names here but none in `api`.
pub const FORK_TERMINAL: &str = "session-fork-terminal";
pub const MOVE: &str = "session-move";
pub const CLOSE: &str = "session-close";
pub const SIDE_ASK: &str = "session-side-ask";
pub const SIDE_CLOSE: &str = "session-side-close";
pub const OPEN: &str = "open-target";
pub const SNOOZE: &str = "session-snooze";
pub const PREVIEW: &str = "worktree-preview";
pub const REUSABLE: &str = "worktree-reusable";

/// What each request carries: those another device can make, as codealong-peer has them, and
/// those that stay on their Mac, as a window writes them.
pub mod params {
    use serde::{Deserialize, Serialize};

    pub use codealong_peer::api::params::*;

    /// `session-move`: the account of its harness it goes on under.
    #[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
    #[serde(default)]
    pub struct Move {
        pub account: String,
    }

    /// `session-side-ask`: the question for its side conversation.
    #[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
    #[serde(default)]
    pub struct Side {
        pub text: String,
    }

    /// `open-target`: one of a worktree's open targets, as the board lists them.
    #[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
    #[serde(default)]
    pub struct Open {
        pub target: String,
    }

    /// `session-snooze`: until when (ms since the epoch), or awake with none.
    #[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
    #[serde(default)]
    pub struct Snooze {
        pub keys: Vec<String>,
        pub until: Option<i64>,
    }

    /// `worktree-preview`: what changes in its preview, and what stays.
    pub use crate::previews::Change as Preview;

    /// `worktree-reusable`.
    #[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
    #[serde(default)]
    pub struct Reusable {
        pub on: bool,
    }
}

/// Which device a request goes to.
#[derive(Clone, Copy, Debug)]
pub enum On<'a> {
    /// The one a session's key or a worktree's lane id names: another device's start with its
    /// part (`peerfeed::split_remote`), and that device knows them by the rest.
    Key(&'a str),
    /// A device by its short id, or this one with none, for a request about no one session or
    /// worktree.
    Device(Option<&'a str>),
}

impl<'a> On<'a> {
    /// The other device it goes to (none for this one), and what that device calls the session
    /// or worktree (empty for none).
    fn split(self) -> (Option<&'a str>, &'a str) {
        match self {
            On::Key(key) => match peerfeed::split_remote(key) {
                Some((device, own)) => (Some(device), own),
                None => (None, key),
            },
            On::Device(device) => (device, ""),
        }
    }
}

/// Who's asking.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum From {
    /// A window on this device: you, at this Mac.
    Here,
    /// A linked device, whose request peers.rs has let through (`api::reach`); or the engine's
    /// `--snapshot`, which like a device isn't you at a window: what it sends claims nothing of
    /// yours, and nothing is left for someone here to finish.
    Device,
    /// The `codealong` command on this device, where an agent may have typed it: with the key
    /// of the session that ran it, when the command found one. It gets what a window has less
    /// what is its user's alone to do (each request's `command`), and what it sends says it's
    /// an agent's.
    Command(Option<String>),
}

/// A request as a function gets it: `key` names the session or worktree on this device (empty
/// for a request about none), `params` the rest.
pub struct Call<'a> {
    pub key: &'a str,
    pub params: &'a Value,
    pub from: &'a From,
}

impl Call<'_> {
    /// The session it's about, for a request that must name one.
    fn session(&self) -> Result<&str, String> {
        Some(self.key).filter(|k| !k.is_empty()).ok_or_else(|| "The request names no session.".to_string())
    }

    /// Its params, as the request's type (`params`) reads them: what a maker left out reads as
    /// empty, and so do params left out altogether.
    fn read<T: DeserializeOwned>(&self) -> Result<T, String> {
        let none = json!({});
        let params = if self.params.is_null() { &none } else { self.params };
        T::deserialize(params).map_err(|e| format!("The request doesn't read: {e}"))
    }
}

struct Request {
    name: &'static str,
    /// Why another device can't ask for it; None when one can (`api::reach` says which).
    only_here: Option<&'static str>,
    /// Whether the `codealong` command may ask for it, with these params: why not, when it
    /// may not.
    command: fn(&Value) -> Result<(), String>,
    here: fn(&Handle, &Call) -> Result<Value, String>,
    /// What another device is asked, given the params and what it calls the session or worktree
    /// (empty for a request to a device): the params with that as their `key`, unless the
    /// request says otherwise.
    there: fn(Value, &str) -> Result<Value, String>,
    /// What another device's answer says here, given its short id: what it said, unless the
    /// request says otherwise.
    back: fn(Value, &str) -> Result<Value, String>,
    /// How long another device is given to answer, when not as long as its kind takes (`ask`).
    wait: Option<Duration>,
}

/// A request any linked device can make and the command too; each line says what differs.
const ANY: Request = Request { name: "", only_here: None, command: |_| Ok(()), here: |_, _| Ok(Value::Null), there: with_key, back: |v, _| Ok(v), wait: None };

const REQUESTS: &[Request] = &[
    Request { name: api::SESSION_PAGE, here: page, ..ANY },
    Request { name: api::SESSION_NEWER, here: newer, ..ANY },
    Request { name: api::SESSION_PENDING, here: pending, ..ANY },
    Request { name: api::SESSION_WAY, here: way, ..ANY },
    Request { name: api::SESSION_STREAM, here: stream, ..ANY },
    Request { name: api::SESSION_ANSWER, here: answer_call, command: deny_only, ..ANY },
    Request { name: api::SESSION_SEND, here: send_prompt, ..ANY },
    Request { name: api::SESSION_CANCEL, here: cancel, ..ANY },
    Request { name: api::SESSION_ARCHIVE, here: archive, ..ANY },
    Request { name: api::SESSION_RENAME, here: rename, ..ANY },
    Request { name: api::MARK_SEEN, here: mark_seen, ..ANY },
    // A search runs as you type, so a device slow to answer is left out rather than waited for.
    Request { name: api::SEARCH, here: search, back: found_back, wait: Some(Duration::from_secs(4)), ..ANY },
    Request { name: api::ROUTE, here: route, ..ANY },
    Request { name: api::START_OPTIONS, here: start_options, ..ANY },
    Request { name: api::START, here: start, there: start_there, back: started_back, ..ANY },
    Request { name: api::SESSION_FORK, here: fork, ..ANY },
    // A fork into a terminal opens one, and closing one ends it: both need someone at that Mac.
    Request { name: FORK_TERMINAL, only_here: Some("Fork it on the Mac it runs on."), here: fork_terminal, ..ANY },
    // A move opens a terminal too, and closes the session's own. Which account a conversation
    // goes on under, and whose usage it spends, is its user's to choose.
    Request { name: MOVE, only_here: Some("Move it on the Mac it runs on."), here: move_session, command: move_in_windows, ..ANY },
    Request { name: CLOSE, only_here: Some("Close it on the Mac it runs on."), here: close, ..ANY },
    // A side conversation's copy runs on the session's Mac, and its answer goes to the windows
    // there. The copy may do what its session may without asking, which isn't the command's to
    // borrow.
    Request { name: SIDE_ASK, only_here: Some("Ask it on the side on the Mac it runs on."), here: side_ask, command: side_in_windows, ..ANY },
    Request { name: SIDE_CLOSE, only_here: Some("Its side conversation is on the Mac it runs on."), here: side_close, command: side_in_windows, ..ANY },
    // What a click opens is on the screen of the device it's on.
    Request { name: OPEN, only_here: Some("That's on another device: it opens there, and its sessions show in their panel here."), here: open, ..ANY },
    // Snoozes keep sessions off the floating bar, which shows only this Mac's.
    Request { name: SNOOZE, only_here: Some("Snooze it on the Mac it runs on."), here: snooze, ..ANY },
    // A worktree's preview and reuse mark are kept on its Mac, with its other settings.
    Request { name: PREVIEW, only_here: Some("That worktree is on another device; set its preview there."), here: set_preview, ..ANY },
    Request { name: REUSABLE, only_here: Some("That worktree is on another device; mark it there."), here: set_reusable, ..ANY },
];

fn find(name: &str) -> Option<&'static Request> {
    REQUESTS.iter().find(|r| r.name == name)
}

/// The command may turn down what a session waits on, never let it through: an agent may have
/// typed it, and allowing a call or answering a question is the session's user's to do. It's
/// the rule a message from the command goes by too, which isn't sent to a session waiting on
/// its user (send.rs).
fn deny_only(params: &Value) -> Result<(), String> {
    let denies = params.get("answer").and_then(Value::as_str) == Some("deny");
    let answers = params.get("answers").is_some_and(|a| !a.is_null());
    match denies && !answers {
        true => Ok(()),
        false => Err("Allowing what a session asks, or answering its questions, is its user's to do, in Codealong or where the session runs. The command can only deny it.".into()),
    }
}

fn side_in_windows(_: &Value) -> Result<(), String> {
    Err("A side conversation is asked and closed in Codealong's windows.".into())
}

fn move_in_windows(_: &Value) -> Result<(), String> {
    Err("A session is moved to another account in Codealong's windows.".into())
}

/// Does `name` for the session or worktree `key` from a window here: on this device, or, for
/// one on another device, by asking that device (with the key it knows it by).
pub fn run(engine: &Handle, peers: &Peers, name: &str, key: &str, params: impl Serialize) -> Result<Value, String> {
    run_as(engine, peers, &From::Here, name, On::Key(key), params)
}

/// `run` for a request about no one session or worktree, on a device by its short id, or on
/// this one with none.
pub fn run_on(engine: &Handle, peers: &Peers, name: &str, device: Option<&str>, params: impl Serialize) -> Result<Value, String> {
    run_as(engine, peers, &From::Here, name, On::Device(device), params)
}

/// `run`, for whoever asks on this device: a window, or the `codealong` command through the
/// app's socket (server.rs), which may not ask for all a window may.
pub fn run_as(engine: &Handle, peers: &Peers, from: &From, name: &str, on: On, params: impl Serialize) -> Result<Value, String> {
    let params = to_value(params)?;
    let req = asked(from, name, &params)?;
    match on.split() {
        (Some(device), own) => ask_there(peers, req, from, device, own, params),
        (None, key) => (req.here)(engine, &Call { key, params: &params, from }),
    }
}

/// Does `name` for sessions on any device, from a window here: one request a device, with the
/// `keys` each knows its sessions by in `params`. Another device's goes in the background, since
/// the window hears of the change from that device's feed, and one the table keeps on the
/// session's Mac (`only_here`) isn't sent. Returns this device's answer, if any were here.
pub fn run_keys(engine: &Handle, peers: &Peers, name: &str, keys: Vec<String>, params: impl Serialize) -> Option<Value> {
    let req = find(name)?;
    let params = to_value(params).ok()?;
    let mut answer = None;
    for (device, keys) in by_device(keys) {
        let mut params = params.clone();
        if let Value::Object(m) = &mut params {
            m.insert("keys".into(), json!(keys));
        }
        match device {
            None => answer = (req.here)(engine, &Call { key: "", params: &params, from: &From::Here }).ok(),
            Some(device) if elsewhere(req, &From::Here).is_ok() => {
                let peers = peers.clone();
                std::thread::spawn(move || {
                    let _ = ask_there(&peers, req, &From::Here, &device, "", params);
                });
            }
            Some(_) => {}
        }
    }
    answer
}

/// Sessions' keys by the device each is on (None for this one), as that device knows them.
fn by_device(keys: Vec<String>) -> BTreeMap<Option<String>, Vec<String>> {
    let mut by: BTreeMap<Option<String>, Vec<String>> = BTreeMap::new();
    for key in keys {
        match peerfeed::split_remote(&key) {
            Some((device, own)) => by.entry(Some(device.to_string())).or_default().push(own.to_string()),
            None => by.entry(None).or_default().push(key),
        }
    }
    by
}

/// Has another device do `req`, as `from` asks it here: what it's asked and what its answer says
/// here are the request's to say (`there`, `back`).
fn ask_there(peers: &Peers, req: &Request, from: &From, device: &str, own: &str, params: Value) -> Result<Value, String> {
    elsewhere(req, from)?;
    let params = (req.there)(params, own)?;
    (req.back)(ask(peers, req, device, params)?, device)
}

/// `name` from another device, from a window here: for what asks every linked device at once
/// (`Peers::search`), which says which devices.
pub fn ask_device(peers: &Peers, name: &str, device: &str, params: impl Serialize) -> Result<Value, String> {
    let params = to_value(params)?;
    let req = asked(&From::Here, name, &params)?;
    ask_there(peers, req, &From::Here, device, "", params)
}

/// The params another device is asked with, by default: with the key it knows the session or
/// worktree by, when the request is about one. The command may leave out a request's params.
fn with_key(params: Value, own: &str) -> Result<Value, String> {
    let mut params = if params.is_null() { json!({}) } else { params };
    if let (Value::Object(m), false) = (&mut params, own.is_empty()) {
        m.insert("key".into(), json!(own));
    }
    Ok(params)
}

/// Whether `from` may have another device do `req`: not what needs someone at the Mac a session
/// runs on, nor a prompt from the command, which the other device would take as its user's own,
/// from another of their devices, with nothing to say an agent sent it.
fn elsewhere(req: &Request, from: &From) -> Result<(), String> {
    if let Some(why) = req.only_here {
        return Err(why.to_string());
    }
    if req.name == api::SESSION_SEND && matches!(from, From::Command(_)) {
        return Err("That session is on another device, where a message from the command would arrive as its user's own, so nothing was sent.".into());
    }
    Ok(())
}

/// Whether the command may ask a linked device for `name` itself (server.rs's `device-ask`, for
/// checking a link by hand): what it may ask for about a session here, and what it may have
/// another device do. A request this table doesn't know is the other device's to refuse.
pub fn command_may_ask(name: &str, params: &Value) -> Result<(), String> {
    let Some(req) = find(name) else { return Ok(()) };
    let from = From::Command(None);
    asked(&from, name, params)?;
    elsewhere(req, &from)
}

/// Does `name` for the session `key` on this device, as `from` asks it, whatever board the
/// handle is over: `--snapshot`'s modes go through it over a board read once (snapshot.rs), and
/// the `codealong` command when the app is closed (cli.rs).
pub fn local(engine: &Handle, from: &From, name: &str, key: &str, params: impl Serialize) -> Result<Value, String> {
    let params = to_value(params)?;
    let req = asked(from, name, &params)?;
    (req.here)(engine, &Call { key, params: &params, from })
}

/// The request `name`, when `from` may ask for it on this device with these params.
fn asked(from: &From, name: &str, params: &Value) -> Result<&'static Request, String> {
    let req = find(name).ok_or_else(|| format!("Codealong doesn't know how to {name}."))?;
    if matches!(from, From::Command(_)) {
        (req.command)(params)?;
    }
    Ok(req)
}

/// Asks another device for `req`, waiting as long as the request says, else as its kind takes:
/// a read briefly, an action longer (it may start an app, or type into a terminal).
fn ask(peers: &Peers, req: &Request, device: &str, params: Value) -> Result<Value, String> {
    let kind = if api::reach(req.name) == Some(Reach::Read) { Duration::from_secs(15) } else { Duration::from_secs(40) };
    peers.ask(device, req.name, params, req.wait.unwrap_or(kind))
}

/// A request's answer as the type it is, for a window's command.
pub fn typed<T: DeserializeOwned>(v: Value) -> Result<T, String> {
    serde_json::from_value(v).map_err(|e| e.to_string())
}

/// Answers a linked device's request (peers.rs has checked it may make it) as a window's would be.
pub fn answer(engine: &Handle, name: &str, params: Value) -> Result<Value, String> {
    let req = find(name).filter(|r| r.only_here.is_none()).ok_or_else(|| format!("Codealong on this device doesn't know how to {name} yet; update it here."))?;
    let key = params.get("key").and_then(Value::as_str).unwrap_or_default().to_string();
    (req.here)(engine, &Call { key: &key, params: &params, from: &From::Device })
}

fn cfg() -> Result<config::Config, String> {
    Ok(config::load_strict().map_err(words)?)
}

fn to_value<T: serde::Serialize>(v: T) -> Result<Value, String> {
    serde_json::to_value(v).map_err(|e| e.to_string())
}


/// A page of the conversation, from its own record (transcript.rs): the newest without
/// `before`, else what came before it.
fn page(engine: &Handle, c: &Call) -> Result<Value, String> {
    let p: params::Page = c.read()?;
    to_value(transcript::page(&engine.board(), &cfg()?, c.session()?, p.before, 60).map_err(words)?)
}

/// What it said and did after `after`, as it goes on.
fn newer(engine: &Handle, c: &Call) -> Result<Value, String> {
    let p: params::Newer = c.read()?;
    to_value(transcript::newer(&engine.board(), &cfg()?, c.session()?, p.after).map_err(words)?)
}

/// What it waits on you to allow, when Codealong can answer for it (`Adapter::pending`).
fn pending(engine: &Handle, c: &Call) -> Result<Value, String> {
    let key = c.session()?;
    if acp::hosts(key) {
        return to_value(acp::pending(key));
    }
    let (cfg, board) = (cfg()?, engine.board());
    let (s, account) = harness::find(&board, &cfg, key).map_err(words)?;
    to_value(harness::adapter(s.harness).pending(&harness::Ctx { account }, s))
}

/// How a message sent to it would reach it (send.rs `way_in`).
fn way(engine: &Handle, c: &Call) -> Result<Value, String> {
    let key = c.session()?;
    if acp::hosts(key) {
        return Ok(Value::Null);
    }
    to_value(send::way_in(&engine.board(), &cfg()?, key).map_err(words)?)
}

/// The reply a session Codealong runs is writing, for a panel that opens mid-turn.
fn stream(_: &Handle, c: &Call) -> Result<Value, String> {
    to_value(acp::stream(c.session()?))
}

/// Allows or denies the tool call `id` it waits on, if it's still the one waiting; or, with
/// `answers`, answers the questions it asks (`Pending::questions`).
fn answer_call(engine: &Handle, c: &Call) -> Result<Value, String> {
    let key = c.session()?;
    let p: params::Answer = c.read()?;
    let (answer, id) = (p.answer.ok_or("The request has no answer.")?, p.id);
    if acp::hosts(key) {
        return acp::answer(key, &id, answer).map(Value::String).map_err(words);
    }
    let (cfg, board) = (cfg()?, engine.board());
    let (s, account) = harness::find(&board, &cfg, key).map_err(words)?;
    let ctx = harness::Ctx { account };
    match p.answers.filter(|a| !a.is_null()) {
        Some(a) => harness::adapter(s.harness).answer_questions(&ctx, s, &id, &a),
        None => harness::adapter(s.harness).answer(&ctx, s, &id, answer),
    }
    .map(Value::String)
    .map_err(words)
}

/// Sends it a prompt where it runs (send.rs), into its running turn at once with `now`: yours
/// from a window, an agent's from the command. From another device, one that would only open
/// with the prompt ready to send is refused: that needs someone here.
fn send_prompt(engine: &Handle, c: &Call) -> Result<Value, String> {
    let sender = match c.from {
        From::Here => send::Sender::You,
        From::Device => send::Sender::Elsewhere,
        From::Command(session) => send::Sender::Agent(session.clone()),
    };
    let p: params::Send = c.read()?;
    let sent = send::send_here(engine, c.session()?, &p.text, sender, p.now).map_err(words)?;
    if *c.from == From::Device && sent.opened {
        return Err(format!("It needs someone at this device: {}", sent.message));
    }
    to_value(sent)
}

/// Stops the turn it's taking: one Codealong runs, through its agent; any other, through its
/// harness (`Adapter::interrupt`). Says what happened.
fn cancel(engine: &Handle, c: &Call) -> Result<Value, String> {
    let key = c.session()?;
    if acp::hosts(key) {
        return acp::cancel(key).map(|_| json!("Stopped.")).map_err(words);
    }
    let (cfg, board) = (cfg()?, engine.board());
    let (s, account) = harness::find(&board, &cfg, key).map_err(words)?;
    if !harness::turn_running(s) {
        return Err("Its turn has already ended.".into());
    }
    harness::adapter(s.harness).interrupt(&harness::Ctx { account }, s).map(Value::String).map_err(words)
}

/// The time of this device's board that shows a change, for a window here to know when the
/// engine has caught up (ahead.ts); none for another device, whose boards keep their own time.
fn shown_at(c: &Call, at: Option<i64>) -> Value {
    if *c.from == From::Here { json!(at) } else { Value::Null }
}

/// Archives it, or unarchives it with `archived: false` (Codex threads in Codex itself).
fn archive(engine: &Handle, c: &Call) -> Result<Value, String> {
    let p: params::Archive = c.read()?;
    let at = engine.archive(c.session()?, p.archived).map_err(words)?;
    Ok(shown_at(c, at))
}

/// Names it where its harness keeps its name, so the harness shows it too.
fn rename(engine: &Handle, c: &Call) -> Result<Value, String> {
    let p: params::Rename = c.read()?;
    let at = engine.rename(c.session()?, &p.name).map_err(words)?;
    Ok(shown_at(c, at))
}

/// Marks finished turns seen, several sessions at once (`keys`).
fn mark_seen(engine: &Handle, c: &Call) -> Result<Value, String> {
    let keys = c.read::<params::Keys>()?.keys;
    Ok(match c.from {
        From::Here => json!(engine.mark_seen_shown(keys)),
        From::Device | From::Command(_) => {
            engine.mark_seen(keys);
            Value::Null
        }
    })
}

/// What was said in this device's sessions.
fn search(_: &Handle, c: &Call) -> Result<Value, String> {
    to_value(crate::search::typed(&c.read::<params::Search>()?.words))
}

/// What another device found, by the keys its sessions have here.
fn found_back(v: Value, device: &str) -> Result<Value, String> {
    let mut found: crate::search::Found = typed(v)?;
    for h in &mut found.hits {
        h.key = peerfeed::remote_key(device, &h.key);
    }
    to_value(found)
}

/// Where typed text goes, as the quick panel suggests it (route.rs).
fn route(engine: &Handle, c: &Call) -> Result<Value, String> {
    let p: params::Route = c.read()?;
    let project = Some(p.project.as_str()).filter(|p| !p.is_empty()).unwrap_or(crate::route::ALL_PROJECTS);
    to_value(crate::route::suggest(&engine.board(), project, &p.text).map_err(words)?)
}

/// Where each account here can start a session.
fn start_options(_: &Handle, _: &Call) -> Result<Value, String> {
    to_value(crate::start::options_here().map_err(words)?)
}

/// Starts a session in a worktree here and links it to its todos.
fn start(engine: &Handle, c: &Call) -> Result<Value, String> {
    to_value(crate::start::start_here(engine, &spec_of(c.params)?).map_err(words)?)
}

fn spec_of(params: &Value) -> Result<crate::start::StartSpec, String> {
    params::Start::deserialize(params).map(|p| p.spec).map_err(|_| "The request has no start.".to_string())
}

/// A start in another device's worktree, as that device knows it, with none of the todos: they
/// are this device's, and it has its own list.
fn start_there(params: Value, lane: &str) -> Result<Value, String> {
    let spec = crate::start::StartSpec { lane_id: lane.to_string(), todos: Vec::new(), from: None, ..spec_of(&params)? };
    to_value(params::Start { spec })
}

/// The session another device started, by the key it has here.
fn started_back(v: Value, device: &str) -> Result<Value, String> {
    let mut started: crate::start::Started = typed(v)?;
    started.key = started.key.map(|k| peerfeed::remote_key(device, &k));
    to_value(started)
}

/// Forks it into a copy that goes on without a window, on `prompt` when there's one
/// (`start::fork_background`).
fn fork(engine: &Handle, c: &Call) -> Result<Value, String> {
    let p: params::Fork = c.read()?;
    to_value(crate::start::fork_background(&engine.board(), &cfg()?, c.session()?, &p.prompt).map_err(words)?)
}

/// Forks it into a new terminal in its folder (`start::fork`).
fn fork_terminal(engine: &Handle, c: &Call) -> Result<Value, String> {
    to_value(crate::start::fork(&engine.board(), &cfg()?, c.session()?).map_err(words)?)
}

/// Moves it to another account of its harness, `account` (`start::move_to`): the copy goes on
/// in a new terminal, and the session is closed and archived: from here on the conversation is
/// the copy's, and so are the todos it was on and its place among the pinned sessions. A session
/// its account's usage limit stopped is moved to go on, so the copy is told to.
fn move_session(engine: &Handle, c: &Call) -> Result<Value, String> {
    let key = c.session()?;
    let account = c.read::<params::Move>()?.account;
    let cfg = cfg()?;
    let stopped = stop_to_move(engine, &cfg, key)?;
    let board = engine.board();
    let limited = board.session(key).is_some_and(|(_, s)| s.limit.is_some());
    let prompt = if limited { crate::start::GO_ON } else { "" };
    let mut started = crate::start::move_to(&board, &cfg, key, &account, prompt).map_err(words)?;
    if stopped {
        started.message = started.message.map(|m| format!("{m} Its turn was stopped first, so the copy goes on from where it was interrupted."));
    }
    if let Some(copy) = &started.key {
        for t in board.todos.iter().filter(|t| t.sessions.iter().any(|s| s == key)) {
            // The session stays on it too, as one that worked on it.
            let link = crate::todos::TodoOp::Link { id: t.id.clone(), worktree: None, session: Some(copy.clone()), linked: true };
            if let Err(e) = engine.todo(link) {
                eprintln!("Moved {key}, but couldn't put {copy} on todo {}: {e}", t.id);
            }
        }
        // Before it's archived, which unpins it.
        if board.pinned.iter().any(|k| k == key) {
            engine.set_pinned(board.pinned.iter().map(|k| if k == key { copy.clone() } else { k.clone() }).collect());
        }
    }
    if let Err(e) = engine.archive(key, true) {
        eprintln!("Moved {key}, but couldn't archive it: {e:#}");
    }
    to_value(started)
}

/// Stops the turn a session is taking so it can move, as Stop does (`cancel`), and waits for the
/// board to show the turn ended: a copy made mid-turn would see it cut off, where one made after
/// sees it interrupted. Says whether there was a turn to stop.
fn stop_to_move(engine: &Handle, cfg: &config::Config, key: &str) -> Result<bool, String> {
    let running = || engine.read(|b| b.session(key).is_some_and(|(_, s)| harness::turn_running(s)));
    if !running() {
        return Ok(false);
    }
    let board = engine.board();
    let (s, account) = harness::find(&board, cfg, key).map_err(words)?;
    if !harness::stoppable(s, Some(account)) {
        return Err(format!("“{}” is in the middle of a turn Codealong can't stop. Stop it where it runs, then move it.", s.title));
    }
    if acp::hosts(key) {
        acp::cancel(key).map_err(words)?;
    } else {
        harness::adapter(s.harness).interrupt(&harness::Ctx { account }, s).map_err(words)?;
    }
    let until = std::time::Instant::now() + Duration::from_secs(30);
    while running() {
        if std::time::Instant::now() > until {
            return Err(format!("Stopped “{}”, but its turn still hadn't ended 30 seconds later. Move it once it has.", s.title));
        }
        std::thread::sleep(Duration::from_millis(300));
    }
    Ok(true)
}

/// Closes its terminal tab, or stops it when it's a background job (idle.rs). Says what happened.
fn close(engine: &Handle, c: &Call) -> Result<Value, String> {
    crate::idle::close_one(&engine.board(), &cfg()?, c.session()?).map(Value::String).map_err(words)
}

/// Asks a throwaway copy of it `text`, on the side (side.rs): the session carries on, unaware.
/// Returns the side conversation with the question in it; the answer goes to the windows.
fn side_ask(engine: &Handle, c: &Call) -> Result<Value, String> {
    let p: params::Side = c.read()?;
    to_value(crate::side::ask(&engine.board(), &cfg()?, c.session()?, &p.text).map_err(words)?)
}

/// Ends its side conversation: the copy stops, and what was said is kept to read (`side::past`).
fn side_close(_: &Handle, c: &Call) -> Result<Value, String> {
    crate::side::close(c.session()?);
    Ok(Value::Null)
}

/// Opens one of a worktree's targets (`target`), as a click does: a session's terminal, the
/// worktree in an editor (`engine::open_on`).
fn open(engine: &Handle, c: &Call) -> Result<Value, String> {
    engine.open(c.key, &c.read::<params::Open>()?.target).map_err(words)?;
    Ok(Value::Null)
}

/// Snoozes sessions (`keys`) until `until` (ms since the epoch), or wakes them with none.
fn snooze(engine: &Handle, c: &Call) -> Result<Value, String> {
    let p: params::Snooze = c.read()?;
    Ok(shown_at(c, engine.snooze(p.keys, p.until)))
}

/// Changes the worktree's preview: its URL, port and the Chrome profile it opens in
/// (`previews::set`). Says whether that changed anything and what it's set to now, and, for a
/// window here, the time of the board that shows it (`at`).
fn set_preview(engine: &Handle, c: &Call) -> Result<Value, String> {
    let (set, at) = engine.set_preview(c.key, &c.read::<params::Preview>()?).map_err(words)?;
    Ok(json!({ "at": shown_at(c, at), "changed": set.changed, "preview": set.preview }))
}

/// Marks the worktree reusable or not (`reusable::mark`). Says whether that changed it, and the
/// time of the board that shows it, as `set_preview` does.
fn set_reusable(engine: &Handle, c: &Call) -> Result<Value, String> {
    let (changed, at) = engine.set_reusable(c.key, c.read::<params::Reusable>()?.on).map_err(words)?;
    Ok(json!({ "at": shown_at(c, at), "changed": changed }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_request_a_device_can_make_is_answered_here() {
        for (name, _) in api::REQUESTS {
            // peers.rs keeps the list of replies to stream on the connection itself.
            if name == api::SESSION_WATCH {
                continue;
            }
            assert!(find(name).is_some_and(|r| r.only_here.is_none()), "nothing answers {name}");
        }
        // And what only happens here is no request another device can make.
        for r in REQUESTS.iter().filter(|r| r.only_here.is_some()) {
            assert_eq!(api::reach(r.name), None, "{} is only here, but api lists it", r.name);
        }
        for r in REQUESTS.iter().filter(|r| r.only_here.is_none()) {
            assert!(api::reach(r.name).is_some(), "{} can be asked for, but api doesn't list who may", r.name);
        }
    }

    #[test]
    fn another_devices_request_for_what_needs_someone_here_is_refused() {
        let engine = Handle::for_tests();
        let err = answer(&engine, FORK_TERMINAL, json!({ "key": "claude:claude:abc" })).unwrap_err();
        assert!(err.contains("doesn't know how to"), "{err}");
        let err = answer(&engine, "session-launch-rockets", json!({})).unwrap_err();
        assert!(err.contains("update it here"), "{err}");
        let err = answer(&engine, api::SESSION_PAGE, json!({})).unwrap_err();
        assert_eq!(err, "The request names no session.");
    }

    fn no_devices() -> (Peers, crate::fixtures::Scratch) {
        let dir = crate::fixtures::scratch("actions");
        let hooks = crate::peers::Hooks { board: Box::new(crate::model::Board::default), answer: Box::new(|_, _| Err("no".into())), emit: Box::new(|_| {}) };
        (Peers::new(dir.to_path_buf(), hooks), dir)
    }

    #[test]
    fn what_is_on_another_device_goes_there_unless_it_needs_someone_at_that_mac() {
        let (peers, _dir) = no_devices();
        let engine = Handle::for_tests();
        // A worktree's targets open, and its preview and reuse mark are set, on its own Mac.
        let lane = "@a1b2c3d4e5:/Users/sam/app";
        for (name, words) in [(OPEN, "it opens there"), (PREVIEW, "set its preview there"), (REUSABLE, "mark it there")] {
            let err = run(&engine, &peers, name, lane, json!({ "target": "editor", "on": true })).unwrap_err();
            assert!(err.contains(words), "{name}: {err}");
        }
        // What another device can do is asked of it: this one has none linked, so the asking fails.
        let spec = json!({ "spec": { "laneId": lane, "account": "claude", "place": "terminal", "prompt": "", "worktree": false, "todos": [] } });
        assert!(run(&engine, &peers, api::START, lane, spec).unwrap_err().contains("isn't linked"));
        assert!(run_on(&engine, &peers, api::START_OPTIONS, Some("a1b2c3d4e5"), json!({})).unwrap_err().contains("isn't linked"));
        // Snoozing another device's session is left undone, and this device's answers.
        assert_eq!(run_keys(&engine, &peers, SNOOZE, vec!["@a1b2c3d4e5:claude:claude:abc".into()], json!({ "until": 1 })), None);
        assert_eq!(run_keys(&engine, &peers, SNOOZE, vec!["@a1b2c3d4e5:claude:claude:abc".into(), "claude:claude:def".into()], json!({ "until": 1 })), Some(Value::Null));
    }

    #[test]
    fn sessions_are_asked_for_on_the_device_each_is_on() {
        let by = by_device(vec!["claude:claude:a".into(), "@a1b2c3d4e5:codex:codex:b".into(), "@f6e5d4c3b2:claude:claude:c".into(), "@a1b2c3d4e5:claude:claude:d".into()]);
        let by: Vec<(Option<&str>, Vec<&str>)> = by.iter().map(|(d, k)| (d.as_deref(), k.iter().map(String::as_str).collect())).collect();
        assert_eq!(by, vec![(None, vec!["claude:claude:a"]), (Some("a1b2c3d4e5"), vec!["codex:codex:b", "claude:claude:d"]), (Some("f6e5d4c3b2"), vec!["claude:claude:c"])]);
    }

    #[test]
    fn another_device_is_asked_what_it_always_was() {
        // A session's request carries the key that device knows it by; a device's, none.
        let page = find(api::SESSION_PAGE).unwrap();
        assert_eq!((page.there)(json!({ "before": 7 }), "claude:claude:abc").unwrap(), json!({ "before": 7, "key": "claude:claude:abc" }));
        assert_eq!((page.there)(Value::Null, "claude:claude:abc").unwrap(), json!({ "key": "claude:claude:abc" }));
        let seen = find(api::MARK_SEEN).unwrap();
        assert_eq!((seen.there)(json!({ "keys": ["claude:claude:abc"] }), "").unwrap(), json!({ "keys": ["claude:claude:abc"] }));
        // A start goes with the worktree as that device knows it and none of this device's todos,
        // and the session it started comes back by the key it has here.
        let start = find(api::START).unwrap();
        let spec = json!({ "laneId": "@a1b2c3d4e5:/Users/sam/app", "account": "claude", "place": "background", "prompt": "Fix the build", "model": null, "effort": null, "worktree": false, "todos": ["t-1"], "from": "claude:claude:me" });
        let there = (start.there)(json!({ "spec": spec }), "/Users/sam/app").unwrap();
        assert_eq!(there, json!({ "spec": { "laneId": "/Users/sam/app", "account": "claude", "place": "background", "prompt": "Fix the build", "model": null, "effort": null, "worktree": false, "todos": [] } }));
        let back = (start.back)(json!({ "key": "claude:claude:new", "message": "Started." }), "a1b2c3d4e5").unwrap();
        assert_eq!(back["key"], "@a1b2c3d4e5:claude:claude:new");
        // What a search found there is named by the keys its sessions have here, and a device
        // slow to answer is left out after a few seconds.
        let search = find(api::SEARCH).unwrap();
        let found = json!({ "hits": [{ "key": "claude:claude:s1", "snippet": "the webhook", "you": false, "at": 1 }], "building": false, "off": false });
        let back = (search.back)(found, "a1b2c3d4e5").unwrap();
        assert_eq!(back["hits"][0]["key"], "@a1b2c3d4e5:claude:claude:s1");
        assert_eq!(search.wait, Some(Duration::from_secs(4)));
    }

    #[test]
    fn the_command_gets_what_a_window_has_less_what_is_its_users_to_do() {
        let engine = Handle::for_tests();
        let command = From::Command(Some("claude:claude:me".into()));
        let ask = |name: &str, params: Value| local(&engine, &command, name, "claude:claude:abc", params);
        // Letting a call through, for good or once, and answering a question are refused before
        // anything is looked at; a denial gets as far as looking for the session.
        for params in [json!({ "id": "t1", "answer": "allow" }), json!({ "id": "t1", "answer": "always" }), json!({ "id": "t1", "answer": "deny", "answers": { "Which?": "This" } }), json!({ "id": "t1" })] {
            let err = ask(api::SESSION_ANSWER, params.clone()).unwrap_err();
            assert!(err.contains("can only deny"), "{params}: {err}");
        }
        let err = ask(api::SESSION_ANSWER, json!({ "id": "t1", "answer": "deny", "answers": null })).unwrap_err();
        assert!(err.contains("isn't on the board"), "{err}");
        // A side conversation's copy acts with its session's permissions, and a move chooses
        // whose usage the conversation spends.
        for name in [SIDE_ASK, SIDE_CLOSE, MOVE] {
            assert!(ask(name, json!({ "text": "what changed?", "account": "claude2" })).unwrap_err().contains("in Codealong's windows"), "{name}");
        }
        // The rest is the command's as it is a window's: these get as far as the session.
        for name in [api::SESSION_PENDING, api::SESSION_CANCEL, api::SESSION_RENAME, api::SESSION_FORK, api::SESSION_SEND, CLOSE] {
            let err = ask(name, json!({ "name": "A name", "text": "hi" })).unwrap_err();
            assert!(err.contains("isn't on the board"), "{name}: {err}");
        }
        // Nor may it have another device do any of that, or send a prompt there, where it would
        // arrive as the user's own.
        let deny = json!({ "id": "t1", "answer": "deny" });
        assert!(command_may_ask(api::SESSION_ANSWER, &deny).is_ok() && command_may_ask(api::SESSION_PAGE, &json!({})).is_ok());
        assert!(command_may_ask(api::SESSION_ANSWER, &json!({ "id": "t1", "answer": "allow" })).unwrap_err().contains("can only deny"));
        assert!(command_may_ask(api::SESSION_SEND, &json!({ "text": "hi" })).unwrap_err().contains("arrive as its user's own"));
        let send = find(api::SESSION_SEND).unwrap();
        assert!(elsewhere(send, &command).is_err() && elsewhere(send, &From::Here).is_ok());
        assert!(elsewhere(find(FORK_TERMINAL).unwrap(), &From::Here).unwrap_err().contains("on the Mac it runs on"));
        // A window may allow, and so may a device, which peers.rs lets through only with remote
        // control on: both get as far as the session.
        for from in [From::Here, From::Device] {
            let err = local(&engine, &from, api::SESSION_ANSWER, "claude:claude:abc", json!({ "id": "t1", "answer": "allow" })).unwrap_err();
            assert!(err.contains("isn't on the board"), "{from:?}: {err}");
        }
    }
}
