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
pub const PREVIEW: &str = "worktree-preview";
pub const REUSABLE: &str = "worktree-reusable";
pub const RUN_COMMAND: &str = "session-run-command";

/// What each request carries: those another device can make, as codealong-peer has them, and
/// those that stay on their Mac, as a window writes them.
pub mod params {
    use serde::{Deserialize, Serialize};

    pub use codealong_peer::api::params::*;

    /// `session-move`: the account of its harness it goes on under, or the worktree it goes on
    /// in, by its lane's id (Codealong's chats for a new chat).
    #[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
    #[serde(default)]
    pub struct Move {
        pub account: String,
        #[serde(skip_serializing_if = "String::is_empty")]
        pub lane: String,
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

    /// `worktree-preview`: what changes in its preview, and what stays.
    pub use crate::previews::Change as Preview;

    /// `worktree-reusable`.
    #[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
    #[serde(default)]
    pub struct Reusable {
        pub on: bool,
    }

    /// `session-run-command`: a command led by `!` you ran for the session, and the folder it runs
    /// in.
    #[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
    #[serde(default)]
    pub struct RunCommand {
        pub text: String,
        pub folder: String,
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
    /// yours, since an agent on another Mac may have asked it, and nothing is left for someone
    /// here to finish.
    Device,
    /// A linked phone or tablet, which only watches (`Linked::watches`): it runs no agents, so
    /// what it sends is yours, as a window's is. Otherwise as a device: nothing is left for
    /// someone here to finish.
    Phone,
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
    /// What's done instead when another device doesn't do it, given its short id, what it calls
    /// the session and why: it's from before the request, or doesn't let this device drive its
    /// agents. Its answer stands, unless this fails too.
    instead: Option<fn(&Handle, &Peers, &str, &str, &str) -> Result<Value, String>>,
}

/// A request any linked device can make and the command too; each line says what differs.
const ANY: Request = Request { name: "", only_here: None, command: |_| Ok(()), here: |_, _| Ok(Value::Null), there: with_key, back: |v, _| Ok(v), wait: None, instead: None };

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
    Request { name: api::MARK_UNREAD, here: mark_unread, ..ANY },
    // A snooze is kept on the session's Mac, so it's snoozed there and everywhere it shows.
    Request { name: api::SESSION_SNOOZE, here: snooze, ..ANY },
    // So is quiet, which is the session's for as long as it lasts, and a helper made your own.
    Request { name: api::SESSION_QUIET, here: quiet, ..ANY },
    Request { name: api::SESSION_OWN, here: make_own, ..ANY },
    // A pin is this Mac's, as it lists its sessions; another device asks for one here.
    Request { name: api::SESSION_PIN, here: pin, ..ANY },
    // Replies suggested for a session are kept on its Mac too, and only shown: nothing reaches the
    // session until someone picks one, so an agent may suggest them through the command.
    Request { name: api::SESSION_SUGGEST, here: suggest, ..ANY },
    // A tag changes how a session is listed, as its read by Jev does, and nothing the agent does.
    Request { name: api::SESSION_TAG, here: tag, ..ANY },
    // A PR is read from GitHub by this device's `gh`, as you: a phone has no GitHub of its own.
    // Reviewing and merging act as you there, so they need remote control (`api::REQUESTS`).
    Request { name: api::PR_DETAIL, here: pr_detail, ..ANY },
    Request { name: api::PR_MERGE_BOX, here: pr_merge_box, ..ANY },
    Request { name: api::PR_REVIEW, here: pr_review, ..ANY },
    Request { name: api::PR_MERGE, here: pr_merge, ..ANY },
    // A review's snooze is kept here, as a session's is.
    Request { name: api::PR_SNOOZE, here: pr_snooze, ..ANY },
    Request { name: api::IMAGE_KEEP, here: image_keep, ..ANY },
    Request { name: api::IMAGE_READ, here: image_read, ..ANY },
    // A search runs as you type, so a device slow to answer is left out rather than waited for.
    Request { name: api::SEARCH, here: search, back: found_back, wait: Some(Duration::from_secs(4)), ..ANY },
    Request { name: api::ROUTE, here: route, ..ANY },
    Request { name: api::START_OPTIONS, here: start_options, ..ANY },
    Request { name: api::START, here: start, there: start_there, back: started_back, ..ANY },
    // A fork answers as a start does, with the copy by the key it has on its device.
    Request { name: api::SESSION_FORK, here: fork, back: started_back, ..ANY },
    Request { name: api::SESSION_RESUME, here: resume, command: resume_is_its_users, ..ANY },
    Request { name: api::SESSION_MODEL, here: switch_model, ..ANY },
    // A fork into a terminal opens one, and closing one ends it: both need someone at that Mac.
    Request { name: FORK_TERMINAL, only_here: Some("Fork it on the Mac it runs on."), here: fork_terminal, ..ANY },
    // A move opens a terminal too, and closes the session's own. Which account a conversation
    // goes on under, and whose usage it spends, is its user's to choose.
    Request { name: MOVE, only_here: Some("Move it on the Mac it runs on."), here: move_session, command: move_in_windows, ..ANY },
    Request { name: CLOSE, only_here: Some("Close it on the Mac it runs on."), here: close, ..ANY },
    // Done ends no more than the session, and what it leaves for someone at its Mac (a question
    // about its worktree) it says. It's you who's done with a session, not an agent. A worktree
    // its harness made can take most of a minute to remove (idle.rs `exit`), and a background
    // job's through the repo's own teardown longer (cleanup.rs). A Mac that won't do it still
    // lets it be marked seen and archived.
    // From the command, only a session's own and the ones it started (`done`, agentdone.rs).
    Request { name: api::SESSION_DONE, here: done, wait: Some(Duration::from_secs(300)), instead: Some(done_anyway), ..ANY },
    // A side conversation's copy runs on the session's Mac, and its answer goes to the windows
    // there. The copy may do what its session may without asking, which isn't the command's to
    // borrow.
    Request { name: SIDE_ASK, only_here: Some("Ask it on the side on the Mac it runs on."), here: side_ask, command: side_in_windows, ..ANY },
    Request { name: SIDE_CLOSE, only_here: Some("Its side conversation is on the Mac it runs on."), here: side_close, command: side_in_windows, ..ANY },
    // What a click opens is on the screen of the device it's on.
    Request { name: OPEN, only_here: Some("That's on another device: it opens there, and its sessions show in their panel here."), here: open, ..ANY },
    // A worktree's preview and reuse mark are kept on its Mac, with its other settings.
    Request { name: PREVIEW, only_here: Some("That worktree is on another device; set its preview there."), here: set_preview, ..ANY },
    Request { name: REUSABLE, only_here: Some("That worktree is on another device; mark it there."), here: set_reusable, ..ANY },
    // A command led by `!` that you ran for a session runs in a tab of yours on its Mac, as you,
    // and the session is told what it printed. Not the command's: its agent asks with `codealong
    // terminal run`, which you allow.
    Request { name: RUN_COMMAND, only_here: Some("Run it on the Mac the session runs on."), here: run_command, command: run_is_yours, ..ANY },
    // A worktree goes only when nothing in it would be lost, checked again on its Mac, so an
    // agent may clean one up too. Can't be undone, so another device needs remote control. A
    // repo's own teardown can take a minute or more (cleanup.rs).
    Request { name: api::WORKTREE_REMOVE, here: remove_worktree, wait: Some(Duration::from_secs(300)), ..ANY },
];

fn find(name: &str) -> Option<&'static Request> {
    REQUESTS.iter().find(|r| r.name == name)
}

/// The command may turn down what a session waits on, never let it through: an agent may have
/// typed it, and allowing a call or answering a question is the session's user's to do. It's
/// the rule a message from the command goes by too, which isn't sent to a session waiting on
/// its user (send.rs). Nor does it say what to do instead: the session takes that as its user's
/// words.
fn deny_only(params: &Value) -> Result<(), String> {
    let denies = params.get("answer").and_then(Value::as_str) == Some("deny");
    let answers = params.get("answers").is_some_and(|a| !a.is_null());
    let saying = params.get("message").and_then(Value::as_str).is_some_and(|m| !m.trim().is_empty());
    match (denies && !answers, saying) {
        (true, false) => Ok(()),
        (true, true) => Err("The command can deny it, not say what to do instead: the session would take that as its user's words. Send a message once it's denied.".into()),
        (false, _) => Err("Allowing what a session asks, or answering its questions, is its user's to do, in Codealong or where the session runs. The command can only deny it.".into()),
    }
}

/// The command never resumes a session, on a prompt or on none: an agent may have typed it, and a
/// message from an agent doesn't start a session again (send.rs `NOT_STARTED_AGAIN`), here or on
/// another device.
fn resume_is_its_users(_: &Value) -> Result<(), String> {
    Err(format!("That session {}", send::NOT_STARTED_AGAIN))
}

fn run_is_yours(_: &Value) -> Result<(), String> {
    Err("An agent asks its user to run a command in their terminal with `codealong terminal run`, which they allow.".into())
}

fn side_in_windows(_: &Value) -> Result<(), String> {
    Err("A side conversation is asked and closed in Codealong's windows.".into())
}

fn move_in_windows(_: &Value) -> Result<(), String> {
    Err("A session is moved to another account or folder in Codealong's windows.".into())
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
        (Some(device), own) => ask_there(peers, req, from, device, own, params).or_else(|why| match req.instead {
            Some(instead) => instead(engine, peers, device, own, &why).map_err(|_| why),
            None => Err(why),
        }),
        (None, key) => (req.here)(engine, &Call { key, params: &params, from }),
    }
}

/// Does `name` for sessions on any device, from a window here: one request a device, with the
/// `keys` each knows its sessions by in `params`. Another device's goes in the background, since
/// the window hears of the change from that device's feed, and one the table keeps on the
/// session's Mac (`only_here`) isn't sent. Returns this device's answer, if any were here.
pub fn run_keys(engine: &Handle, peers: &Peers, name: &str, keys: Vec<String>, params: impl Serialize) -> Option<Value> {
    each_device(engine, peers, name, keys, params).and_then(|(answer, _)| answer)
}

/// `run_keys`, waiting for the other devices' answers too, for a window that says when one
/// didn't do it: it isn't connected, or its Codealong is from before the request.
pub fn run_keys_heard(engine: &Handle, peers: &Peers, name: &str, keys: Vec<String>, params: impl Serialize) -> Result<Option<Value>, String> {
    let (answer, there) = each_device(engine, peers, name, keys, params).ok_or_else(|| format!("Codealong doesn't know how to {name}."))?;
    for (device, asked) in there {
        let why = match asked.join() {
            Ok(Ok(_)) => continue,
            Ok(Err(why)) => why,
            Err(_) => "That stopped partway.".to_string(),
        };
        return Err(if why.contains(UNKNOWN) { format!("Codealong on {} can't do that from here until it's updated.", peers.name(&device)) } else { why });
    }
    Ok(answer)
}

/// What another device was asked by `each_device`, by its short id: its answer, once it comes.
type Asked = (String, std::thread::JoinHandle<Result<Value, String>>);

/// `run_keys`'s requests: this device's answer, and each other device's as it comes.
fn each_device(engine: &Handle, peers: &Peers, name: &str, keys: Vec<String>, params: impl Serialize) -> Option<(Option<Value>, Vec<Asked>)> {
    let req = find(name)?;
    let params = to_value(params).ok()?;
    let mut answer = None;
    let mut there = Vec::new();
    for (device, keys) in by_device(keys) {
        let mut params = params.clone();
        if let Value::Object(m) = &mut params {
            m.insert("keys".into(), json!(keys));
        }
        match device {
            None => answer = (req.here)(engine, &Call { key: "", params: &params, from: &From::Here }).ok(),
            Some(device) if elsewhere(req, &From::Here).is_ok() => {
                let peers = peers.clone();
                let asking = device.clone();
                there.push((device, std::thread::spawn(move || ask_there(&peers, req, &From::Here, &asking, "", params))));
            }
            Some(_) => {}
        }
    }
    Some((answer, there))
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
    // The other device would take it as its user's, and not as an agent's, which may mark done
    // only its own session and those it started (agentdone.rs).
    if req.name == api::SESSION_DONE && matches!(from, From::Command(_)) {
        return Err("That session is on another device, where the command doesn't mark sessions done. Mark it done in Codealong.".into());
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

/// How a device says it doesn't know a request (`answer`), as every Mac since 2026-09-29 says it.
const UNKNOWN: &str = "doesn't know how to";

/// Answers a linked device's request (peers.rs has checked it may make it) as a window's would be.
/// `watching`: the device only watches (a phone), so what it sends is yours.
pub fn answer(engine: &Handle, name: &str, params: Value, watching: bool) -> Result<Value, String> {
    let req = find(name).filter(|r| r.only_here.is_none()).ok_or_else(|| format!("Codealong on this device {UNKNOWN} {name} yet; update it here."))?;
    let key = params.get("key").and_then(Value::as_str).unwrap_or_default().to_string();
    let from = if watching { From::Phone } else { From::Device };
    (req.here)(engine, &Call { key: &key, params: &params, from: &from })
}

fn cfg() -> Result<config::Config, String> {
    Ok(config::current().map_err(words)?)
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
    to_value(pending_of(engine, c.session()?)?)
}

/// What the session `key` waits on you for, as `pending` says: a command its agent asks to run in
/// your terminal first (terminal.rs), then what its harness asks.
fn pending_of(engine: &Handle, key: &str) -> Result<Option<api::Pending>, String> {
    if let Some(asked) = crate::terminal::of_session(key) {
        let terminal = engine.read(|b| crate::launch::terminal_name(&b.apps.terminal_id));
        return Ok(Some(crate::terminal::pending(&asked, &terminal)));
    }
    if acp::hosts(key) {
        return Ok(acp::pending(key));
    }
    let (cfg, board) = (cfg()?, engine.board());
    let (s, account) = harness::find(&board, &cfg, key).map_err(words)?;
    Ok(harness::adapter(s.harness).pending(&harness::Ctx { account }, s))
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

/// How long a deny that couldn't carry your words waits for the session to stop waiting on you
/// before they're sent.
const ANSWERED_WAIT: Duration = Duration::from_secs(10);

/// Allows or denies the tool call `id` it waits on, if it's still the one waiting; or, with
/// `answers`, answers the questions it asks (`Pending::questions`). A deny with a `message`
/// tells it what to do instead: in the same answer where its harness takes words with one
/// (`Adapter::deny_saying`), else sent as a prompt once it no longer waits.
fn answer_call(engine: &Handle, c: &Call) -> Result<Value, String> {
    let said = answer_here(engine, c)?;
    // Answers to its questions, or words sent in place of the call, answer what was suggested
    // for it, as a message does (`send_prompt`).
    let p: params::Answer = c.read()?;
    let words_sent = p.message.as_deref().is_some_and(|m| !m.trim().is_empty());
    if (words_sent || p.answers.as_ref().is_some_and(|a| !a.is_null())) && !matches!(c.from, From::Command(_)) {
        forget_suggested(engine, c.session()?);
    }
    Ok(said)
}

/// Runs a command led by `!` that you ran for the session (in its box, or Run on a `!` block) in a
/// new tab of your terminal, as you, and has the session told what it printed once it ends
/// (terminal.rs `report_ended`): the `!` means it's for the session to read.
fn run_command(engine: &Handle, c: &Call) -> Result<Value, String> {
    let key = c.session()?;
    let p: params::RunCommand = c.read()?;
    let text = p.text.trim_start();
    let command = text.strip_prefix('!').unwrap_or(text).trim().to_string();
    if let Some(why) = crate::terminal::refused(&command) {
        return Err(why.into());
    }
    let (apps, lanes, title) = engine.read(|b| (b.apps.clone(), b.lanes.clone(), b.session(key).map(|(_, s)| s.title.clone())));
    let title = title.ok_or("Codealong doesn't list that session.")?;
    let (id, at) = crate::terminal::new_id();
    let asked = crate::terminal::Asked { id, ask: crate::terminal::Ask { session: key.to_string(), command, cwd: p.folder.into() }, at };
    let run = crate::terminal::start(&asked, &apps, &lanes, &title, true).map_err(words)?;
    Ok(json!(format!("Running it in a new {} tab. What it prints goes to the session when it ends.", run.terminal)))
}

/// Answers a command the session's agent asks to run in your terminal (terminal.rs): Deny refuses
/// it, with your words when you wrote some, and Allow opens its tab. Only you allow it, from a
/// window here or your phone: the command may only deny (`deny_only`), and another Mac's request
/// may come from an agent there. It's asked every time, so there's no Always.
fn answer_run(engine: &Handle, c: &Call, key: &str, id: &str, answer: api::Answer, saying: Option<&str>) -> Result<Value, String> {
    let gone = || "It isn't waiting on that any more: its agent stopped asking.".to_string();
    match answer {
        api::Answer::Deny => crate::terminal::deny(key, id, saying).map(Value::String).ok_or_else(gone),
        api::Answer::Always => Err("A command for your terminal is asked about every time: allow it once, or deny it.".into()),
        api::Answer::Allow if !matches!(c.from, From::Here | From::Phone) => Err("A command runs in your terminal only when you allow it on its Mac or from your phone.".into()),
        api::Answer::Allow => {
            let open = |a: &crate::terminal::Asked| {
                let (apps, lanes, title) = engine.read(|b| (b.apps.clone(), b.lanes.clone(), b.session(&a.ask.session).map(|(_, s)| s.title.clone()).unwrap_or_default()));
                crate::terminal::start(a, &apps, &lanes, &title, false)
            };
            let said = crate::terminal::allow(key, id, open).ok_or_else(gone)?.map_err(words)?;
            Ok(Value::String(said))
        }
    }
}

/// The answer itself, as `answer_call` says.
fn answer_here(engine: &Handle, c: &Call) -> Result<Value, String> {
    let key = c.session()?;
    let p: params::Answer = c.read()?;
    let (answer, id) = (p.answer.ok_or("The request has no answer.")?, p.id);
    let answers = p.answers.filter(|a| !a.is_null());
    let saying = p.message.as_deref().map(str::trim).filter(|m| !m.is_empty() && answer == api::Answer::Deny && answers.is_none());
    if crate::terminal::is_id(&id) {
        return answer_run(engine, c, key, &id, answer, saying);
    }
    if acp::hosts(key) {
        // A session Codealong runs takes a message while it waits, after its current turn.
        let said = acp::answer(key, &id, answer).map_err(words)?;
        return match saying {
            Some(m) => sent_after(engine, c, key, m, &said),
            None => Ok(Value::String(said)),
        };
    }
    let (cfg, board) = (cfg()?, engine.board());
    let (s, account) = harness::find(&board, &cfg, key).map_err(words)?;
    let ctx = harness::Ctx { account };
    let adapter = harness::adapter(s.harness);
    if let Some(m) = saying {
        if let Some(said) = adapter.deny_saying(&ctx, s, &id, m).map_err(words)? {
            return Ok(Value::String(said));
        }
        let said = adapter.answer(&ctx, s, &id, answer).map_err(words)?;
        return sent_after(engine, c, key, m, &said);
    }
    match answers {
        Some(a) => adapter.answer_questions(&ctx, s, &id, &a),
        None => adapter.answer(&ctx, s, &id, answer),
    }
    .map(Value::String)
    .map_err(words)
}

/// Sends `text` to the session `key` after a deny that couldn't carry it (`said` says how that
/// went), once the session no longer waits on you: typed into a prompt that's still up, a
/// Return would answer it (send.rs never types into one).
fn sent_after(engine: &Handle, c: &Call, key: &str, text: &str, said: &str) -> Result<Value, String> {
    let until = std::time::Instant::now() + ANSWERED_WAIT;
    while !acp::hosts(key) {
        let cfg = cfg()?;
        let board = engine.board();
        let (s, _) = harness::find(&board, &cfg, key).map_err(words)?;
        if !send::waits_on_you(s) {
            break;
        }
        if std::time::Instant::now() > until {
            return Err(format!("{said} It still says it's waiting on you, so what you wrote wasn't sent."));
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    let sent = send::send_here(engine, key, text, sender_of(c.from), false).map_err(|e| format!("{said} What you wrote wasn't sent: {}", words(e)))?;
    Ok(Value::String(format!("Denied it, then sent what you wrote. {}", sent.message)))
}

/// Who a message is from, as send.rs tells the session.
fn sender_of(from: &From) -> send::Sender {
    match from {
        // What you type on your phone is yours too: a phone runs no agents that could have typed it.
        From::Here | From::Phone => send::Sender::You,
        From::Device => send::Sender::Elsewhere,
        From::Command(session) => send::Sender::Agent(session.clone()),
    }
}

/// Sends it a prompt where it runs (send.rs), into its running turn at once with `now`: yours
/// from a window, an agent's from the command. From another device, one that would only open
/// with the prompt ready to send is refused: that needs someone here.
fn send_prompt(engine: &Handle, c: &Call) -> Result<Value, String> {
    let p: params::Send = c.read()?;
    let key = c.session()?;
    unarchive_to_send(engine, c, key);
    let sent = send::send_here(engine, key, &p.text, sender_of(c.from), p.now).map_err(words)?;
    if matches!(c.from, From::Device | From::Phone) && sent.opened {
        return Err(format!("It needs someone at this device: {}", sent.message));
    }
    // What its user sent answers what was suggested, in every window at once, rather than once
    // its record shows the prompt. Not what an agent sent, nor what's only on the clipboard for
    // its app to take.
    if !matches!(c.from, From::Command(_)) && !sent.opened {
        forget_suggested(engine, key);
    }
    to_value(sent)
}

/// Takes a session out of the archive as it's sent something, before it's sent: you took it up
/// again, and a harness may not go on with one it archived (Codex resumes no archived thread).
/// Not for an agent's message here, which never resumes a session you archived (send.rs
/// `wakes`), and has one that takes it in active again, which ends an archive in Codealong by
/// itself. A send goes on when this fails, saying nothing of it: the message is what was asked
/// for, and the session still shows archived.
fn unarchive_to_send(engine: &Handle, c: &Call, key: &str) {
    let archived = engine.read(|b| b.session(key).is_some_and(|(_, s)| s.archived));
    if !unarchives(c.from, archived) {
        return;
    }
    if let Err(e) = engine.archive(key, false) {
        eprintln!("Couldn't take {key} out of the archive to send it a message: {e:#}");
    }
}

/// Whether a message from `from` takes a session out of the archive (`unarchive_to_send`).
fn unarchives(from: &From, archived: bool) -> bool {
    archived && !matches!(from, From::Command(_))
}

/// Takes away the replies suggested for a session, when it has any.
fn forget_suggested(engine: &Handle, key: &str) {
    let at = crate::suggestions::current().sessions.get(key).map(|g| g.at);
    let Some(at) = at else { return };
    match crate::suggestions::forget(&[(key.to_string(), at)]) {
        Ok(true) => {
            engine.suggested(false);
        }
        Ok(false) => {}
        Err(e) => eprintln!("Couldn't forget the replies suggested for {key}: {e:#}"),
    }
}

/// Suggests replies for it, or picks choices of the questions it asks, in place of any it had,
/// or with neither takes them away (suggestions.rs). They're the session's until something is
/// sent to it or a turn of its starts. Says what was done, in a sentence.
fn suggest(engine: &Handle, c: &Call) -> Result<Value, String> {
    let key = c.session()?;
    let p: params::Suggest = c.read()?;
    let Some(session) = engine.read(|b| b.session(key).map(|(_, s)| s.clone())) else {
        return Err("Codealong doesn't list that session.".into());
    };
    let replies = crate::suggestions::tidy(p.replies).map_err(words)?;
    let note = crate::suggestions::tidy_note(p.note).map_err(words)?;
    let n = replies.len();
    // Picks go by the questions it asks now. Replies to questions would only decline them, so a
    // session asking some gets picks instead.
    let picking = p.picks.iter().any(|p| !p.trim().is_empty());
    let pending = if picking || (n > 0 && send::waits_on_you(&session)) { pending_of(engine, key)? } else { None };
    // Answered where it runs, it may have gone on to other questions in the same turn since they
    // were read, which picks for those would answer.
    if let (true, Some(read), Some(now)) = (picking, p.pending.as_deref(), pending.as_ref()) {
        if now.id != read {
            return Err("It asks other questions now than the ones you read. Read them again (`codealong session pending`).".into());
        }
    }
    let asked = pending.map(|p| p.questions).unwrap_or_default();
    if n > 0 && !asked.is_empty() {
        return Err("It asks you questions, which a reply would decline: pick among its choices instead (`--pick`; `codealong session pending` lists them).".into());
    }
    let crate::suggestions::Picked { picks, why } = if picking { crate::suggestions::picked(&asked, &p.picks, &p.why).map_err(words)? } else { crate::suggestions::Picked { picks: BTreeMap::new(), why: BTreeMap::new() } };
    // What was read is where they answer from; without it (an older Mac's agent), where it stands.
    let read = p.prompt_at.unwrap_or(session.last_prompt_at);
    let suggesting = n > 0 || !picks.is_empty();
    if suggesting {
        if let Some(why) = crate::board::suggestion_refused(&session, read) {
            return Err(why);
        }
    }
    let filled = |v: Option<String>| v.map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
    let questions = picks.len();
    let suggested = suggesting.then(|| crate::model::Suggestions { replies, picks, why, note, by: filled(p.by), by_title: filled(p.by_title), at: crate::util::now_ms(), prompt_at: Some(read) });
    let changed = crate::suggestions::set(key, suggested).map_err(words)?;
    // Waited for whoever asked, so that what they read next (an agent's `codealong sessions`)
    // has them.
    let at = if changed { engine.suggested(true) } else { None };
    if *c.from == From::Here {
        return Ok(json!(at));
    }
    Ok(json!(match (n, questions) {
        (0, 1) => "Picked a choice for its question. It shows picked in the session's question in Codealong, to answer with a click, until it's answered or a turn of its starts.".to_string(),
        (0, q) if q > 1 => format!("Picked choices for its {q} questions. They show picked in the session's questions in Codealong, to answer with a click, until they're answered or a turn of its starts."),
        (0, _) if changed => "Took its suggested replies away.".to_string(),
        (0, _) => "It had no suggested replies.".to_string(),
        (1, _) => "Suggested a reply. It shows above the session's box in Codealong until something is sent to the session or a turn of its starts.".to_string(),
        (n, _) => format!("Suggested {n} replies. They show above the session's box in Codealong until something is sent to the session or a turn of its starts."),
    }))
}

/// Tags a session with an agent's read of where it stands, in place of any tag it had, or with
/// nothing said takes the tag away (tags.rs). It's the session's until something is sent to it
/// or a turn of its starts. Says what was done, in a sentence.
fn tag(engine: &Handle, c: &Call) -> Result<Value, String> {
    let key = c.session()?;
    let p: params::Tag = c.read()?;
    let Some(session) = engine.read(|b| b.session(key).map(|(_, s)| s.clone())) else {
        return Err("Codealong doesn't list that session.".into());
    };
    let insight = crate::tags::read(&p).map_err(words)?;
    // What was read is where the tag speaks from; without it (an older Mac's agent), where it stands.
    let read = p.prompt_at.unwrap_or(session.last_prompt_at);
    if insight.is_some() {
        if let Some(why) = crate::board::read_refused(&session, read, "tag") {
            return Err(why);
        }
    }
    let tagging = insight.is_some();
    let tag = insight.map(|insight| crate::tags::Tag { insight, at: crate::util::now_ms(), prompt_at: read });
    let changed = crate::tags::set(key, tag).map_err(words)?;
    // Waited for whoever asked, so that what they read next (an agent's `codealong sessions`)
    // has it.
    let at = if changed { engine.suggested(true) } else { None };
    if *c.from == From::Here {
        return Ok(json!(at));
    }
    Ok(json!(match (tagging, changed) {
        (true, _) => "Tagged. It shows on the session in Codealong, over Jev's read of it, until something is sent to the session or a turn of its starts.".to_string(),
        (false, true) => "Took its tag away.".to_string(),
        (false, false) => "It had no tag.".to_string(),
    }))
}

/// Goes on with a session that isn't running where it says (`start::resume`), answering how it
/// went as a send does, out of the archive as a send takes it. Where its harness copied it (a
/// background job's), the copy takes its place, as a move's does.
fn resume(engine: &Handle, c: &Call) -> Result<Value, String> {
    let key = c.session()?;
    let p: params::Resume = c.read()?;
    unarchive_to_send(engine, c, key);
    let board = engine.board();
    let resumed = crate::start::resume(&board, &cfg()?, key, &p.resume, &p.prompt, &sender_of(c.from)).map_err(words)?;
    if let Some(copy) = resumed.key.as_deref().filter(|k| *k != key) {
        hand_over(engine, &board, key, copy);
    }
    if matches!(c.from, From::Device | From::Phone) && resumed.sent.opened {
        return Err(format!("It needs someone at this device: {}", resumed.sent.message));
    }
    to_value(resumed.sent)
}

/// Switches a running session's model, for it alone (`send::switch_model`). Says what happened.
fn switch_model(engine: &Handle, c: &Call) -> Result<Value, String> {
    let p: params::Model = c.read()?;
    to_value(send::switch_model(&engine.board(), &cfg()?, c.session()?, &p.model).map_err(words)?)
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
        From::Device | From::Phone | From::Command(_) => {
            engine.mark_seen(keys);
            Value::Null
        }
    })
}

/// Marks sessions unread, several at once (`keys`): each is Your turn again until it's seen or
/// prompted (`board::apply_unread`).
fn mark_unread(engine: &Handle, c: &Call) -> Result<Value, String> {
    let keys = c.read::<params::Keys>()?.keys;
    Ok(match c.from {
        From::Here => json!(engine.mark_unread_shown(keys)),
        From::Device | From::Phone | From::Command(_) => {
            engine.mark_unread(keys);
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
    to_value(crate::start::start_here(engine, &asked_by(spec_of(c.params)?, c.from)).map_err(words)?)
}

/// The start as whoever asked may have it: only the `codealong` command names the session that
/// asked for it (`StartSpec::from`), and only as the session that ran it. Another device's
/// start names none, since an agent there could otherwise start a helper of a session here and
/// have its messages to that session go unheld (send.rs `from_its_helper`).
fn asked_by(spec: crate::start::StartSpec, from: &From) -> crate::start::StartSpec {
    let ran = match from {
        From::Command(Some(key)) => spec.from.as_deref() == Some(key.as_str()),
        _ => false,
    };
    if ran { spec } else { crate::start::StartSpec { from: None, helper: false, report: false, ..spec } }
}

fn spec_of(params: &Value) -> Result<crate::start::StartSpec, String> {
    params::Start::deserialize(params).map(|p| p.spec).map_err(|_| "The request has no start.".to_string())
}

/// A start in another device's worktree, as that device knows it, with none of the todos: they
/// are this device's, and it has its own list.
fn start_there(params: Value, lane: &str) -> Result<Value, String> {
    let spec = crate::start::StartSpec { lane_id: lane.to_string(), todos: Vec::new(), from: None, helper: false, report: false, ..spec_of(&params)? };
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
/// its account's usage limit stopped is moved to go on, so the copy is told to. With a `lane`,
/// it moves to that worktree instead (`start::move_to_folder`), as itself, so its todos and its
/// place stay with it, unless its harness copied it there.
fn move_session(engine: &Handle, c: &Call) -> Result<Value, String> {
    let key = c.session()?;
    let to = c.read::<params::Move>()?;
    let cfg = cfg()?;
    let stopped = stop_to_move(engine, &cfg, key)?;
    let board = engine.board();
    if !to.lane.is_empty() {
        // It goes on as itself, unless the harness copied it.
        let mut started = crate::start::move_to_folder(&board, &cfg, key, &to.lane).map_err(words)?;
        if stopped {
            started.message = started.message.map(|m| format!("{m} Its turn was stopped first, so it goes on from where it was interrupted."));
        }
        if let Some(copy) = started.key.as_deref().filter(|k| *k != key) {
            hand_over(engine, &board, key, copy);
        }
        return to_value(started);
    }
    let limited = board.session(key).is_some_and(|(_, s)| s.limit.is_some());
    let go_on = crate::send::by_codealong(crate::start::GO_ON);
    let prompt = if limited { go_on.as_str() } else { "" };
    let mut started = crate::start::move_to(&board, &cfg, key, &to.account, prompt).map_err(words)?;
    if stopped {
        started.message = started.message.map(|m| format!("{m} Its turn was stopped first, so the copy goes on from where it was interrupted."));
    }
    match &started.key {
        Some(copy) => hand_over(engine, &board, key, copy),
        None => archive_moved(engine, key),
    }
    to_value(started)
}

/// The copy a session moved into takes its place: the todos it was on, as their primary session
/// where it was theirs, and its place among the pinned sessions. The session itself is archived.
fn hand_over(engine: &Handle, board: &crate::model::Board, key: &str, copy: &str) {
    for t in board.todos.iter().filter(|t| t.sessions.iter().any(|s| s == key)) {
        // The session stays on it too, as one that worked on it.
        let link = match t.primary.as_ref().is_some_and(|p| p.session == key) {
            true => crate::todos::TodoOp::Primary { id: t.id.clone(), session: Some(copy.to_string()) },
            false => crate::todos::TodoOp::Link { id: t.id.clone(), worktree: None, session: Some(copy.to_string()), linked: true },
        };
        if let Err(e) = engine.todo(link) {
            eprintln!("Moved {key}, but couldn't put {copy} on todo {}: {e}", t.id);
        }
    }
    // Before it's archived, which unpins it.
    if board.pinned.iter().any(|k| k == key) {
        engine.set_pinned(board.pinned.iter().map(|k| if k == key { copy.to_string() } else { k.clone() }).collect());
    }
    archive_moved(engine, key);
}

fn archive_moved(engine: &Handle, key: &str) {
    if let Err(e) = engine.archive(key, true) {
        eprintln!("Moved {key}, but couldn't archive it: {e:#}");
    }
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

/// Done with it, from its menu, its key or the strip: it ends as far as Codealong can end it
/// without ending more than it (`idle::ending`), one that holds a worktree its harness made for
/// it by its harness's own exit, which cleans that up, and is marked seen and archived, so it leaves
/// every list but Archived. Work under way holds it unless `stop` ends that too: then nothing is
/// done, and the answer's `done` is false, with what's under way. Says what happened, with the
/// time of the board that shows it, as `archive` does. From the command it goes by what an agent
/// may mark done (`done_for_agent`).
fn done(engine: &Handle, c: &Call) -> Result<Value, String> {
    let p: params::Done = c.read()?;
    let key = c.session()?;
    if let From::Command(by) = c.from {
        return done_for_agent(engine, c, by.as_deref(), key);
    }
    finish(engine, c, key, p.stop, None)
}

/// Mark as done from the command (agentdone.rs): its own session is ended once its turn ends and
/// left for its user to read, and a session it started is marked done now, as from a window but
/// never ending work under way, and saying which session did it.
fn done_for_agent(engine: &Handle, c: &Call, by: Option<&str>, key: &str) -> Result<Value, String> {
    use crate::agentdone::{may, wait_for_turn, Whose};
    let cfg = cfg()?;
    let board = engine.board();
    let (s, _) = harness::find(&board, &cfg, key).map_err(words)?;
    match may(&board, by, s)? {
        Whose::Own => {
            wait_for_turn(key);
            let message = "This session ends once this turn does: its terminal closes, and its last reply stays in Your turn for its user to read and mark done.";
            to_value(api::Done { done: false, message: message.into(), noted: false })
        }
        Whose::Started => {
            let mine = by.and_then(|k| board.session(k)).map(|(_, me)| me.title.clone()).unwrap_or_else(|| "A session".into());
            finish(engine, c, key, false, Some(&mine))
        }
    }
}

/// Ends `key` as far as `idle::ending` says, marks it seen and archives it (`done`), and its
/// helpers with it (`finish_helpers`). `by` names the session that asked for it from the
/// command, which the message says.
fn finish(engine: &Handle, c: &Call, key: &str, stop: bool, by: Option<&str>) -> Result<Value, String> {
    let cfg = cfg()?;
    let board = engine.board();
    let (s, account) = harness::find(&board, &cfg, key).map_err(words)?;
    let ends = match crate::idle::ending(&board, s, stop) {
        Ok(ends) => ends,
        Err(busy) => return to_value(api::Done { done: false, message: busy, noted: false }),
    };
    let job = matches!(ends, crate::idle::Ending::Job);
    let (ended, note) = end(s, account, ends);
    let stopped = ended.is_some();
    let title = s.title.clone();
    engine.mark_seen(vec![key.to_string()]);
    let at = engine.archive(key, true).map_err(words)?;
    let what = match ended {
        Some(what) => format!("{what} and it's archived."),
        None => "it's archived.".to_string(),
    };
    let mut message = match by {
        Some(by) => format!("“{by}” marked “{title}” done: {what}"),
        None => format!("Done with “{title}”: {what}"),
    };
    if let Some(note) = &note {
        message = format!("{message} {note}");
    }
    // Only once it's archived: a repo's own teardown can take a minute, and the session would
    // come back to the lists meanwhile.
    let worktree = job_worktree(engine, &cfg, s, job && stopped);
    if let Some((words, _)) = &worktree {
        message = format!("{message} {words}");
    }
    let (helped, later) = finish_helpers(engine, &board, &cfg, key, stop);
    let at = later.or(at);
    if let Some(words) = &helped {
        message = format!("{message} {words}");
    }
    let noted = note.is_some() || worktree.is_some_and(|(_, noted)| noted);
    let mut answer = to_value(api::Done { done: true, message, noted })?;
    answer["at"] = shown_at(c, at);
    Ok(answer)
}

/// Marks done the helpers of `key`, a session just marked done: the work they did was for it.
/// Each ends as Mark as done ends a session and is archived, but one at work (or waiting on its
/// turn to end) is left to finish unless `stop` ends that too, as it did for `key`. What to say of
/// them, and the time of the board that shows the last one archived.
fn finish_helpers(engine: &Handle, board: &crate::model::Board, cfg: &config::Config, key: &str, stop: bool) -> (Option<String>, Option<i64>) {
    let (mut done, mut left, mut at) = (0, 0, None);
    for h in crate::board::helpers_of(&board.lanes, key) {
        let Some(account) = cfg.accounts.iter().find(|a| a.name == h.account) else { continue };
        match crate::idle::ending(board, h, stop) {
            Ok(ends) => {
                // What didn't go for a helper is left unsaid: it's archived either way, and
                // its own panel says how it stands.
                let job = matches!(ends, crate::idle::Ending::Job);
                let (ended, _) = end(h, account, ends);
                engine.mark_seen(vec![h.key.clone()]);
                if let Ok(shown) = engine.archive(&h.key, true) {
                    done += 1;
                    at = shown.or(at);
                }
                let _ = job_worktree(engine, cfg, h, job && ended.is_some());
            }
            Err(_) => left += 1,
        }
    }
    (helpers_done_words(done, left), at)
}

/// What Mark as done says of a session's helpers: the ones marked done with it, and the ones it
/// left at work.
fn helpers_done_words(done: usize, left: usize) -> Option<String> {
    let done_words = match done {
        0 => None,
        1 => Some("Its helper is done too".to_string()),
        n => Some(format!("Its {n} helpers are done too")),
    };
    match (done_words, left) {
        (None, 0) => None,
        (Some(d), 0) => Some(format!("{d}.")),
        (Some(d), 1) => Some(format!("{d}; one still at work is left to finish.")),
        (Some(d), n) => Some(format!("{d}; {n} still at work are left to finish.")),
        (None, 1) => Some("Its helper still at work is left to finish.".into()),
        (None, n) => Some(format!("Its {n} helpers still at work are left to finish.")),
    }
}

/// Ends a session as `ends` says: what ended, and what's worth saying besides (why nothing did,
/// or what didn't go).
fn end(s: &crate::model::Session, account: &config::AccountCfg, ends: crate::idle::Ending) -> (Option<String>, Option<String>) {
    use crate::idle::{Ending, Exited};
    let ctx = harness::Ctx { account };
    let adapter = harness::adapter(s.harness);
    match ends {
        Ending::Terminal(pid) => match crate::launch::close_session_terminals(&[(pid, std::path::Path::new(&s.cwd))]).pop() {
            Some(Ok(())) => (Some("its terminal is closed".into()), None),
            Some(Err(e)) => (None, Some(format!("Its terminal didn't close: {}", words(e)))),
            None => (None, Some("Its terminal didn't close.".to_string())),
        },
        Ending::Exit(pid) => {
            let word = adapter.harness().word();
            let worktree = s.own_worktree.as_deref().map(|w| crate::util::file_name(std::path::Path::new(w))).unwrap_or_default();
            match crate::idle::exit(s, account, pid) {
                Ok(Exited::Ended { removed: true, closed: Ok(()) }) => (Some(format!("{word} removed its worktree, its terminal is closed")), None),
                Ok(Exited::Ended { removed: true, closed: Err(e) }) => (Some(format!("{word} removed its worktree")), Some(format!("Its terminal didn't close: {e}"))),
                Ok(Exited::Ended { removed: false, closed }) => (
                    closed.is_ok().then(|| "it exited and its terminal is closed".to_string()),
                    Some(format!("{word} kept its worktree, {worktree}.{}", closed.err().map(|e| format!(" Its terminal didn't close: {e}")).unwrap_or_default())),
                ),
                Ok(Exited::Asks) => (None, Some(format!("{word} asks in its terminal whether to keep its worktree, {worktree}: answer it there."))),
                Ok(Exited::Removing) => (None, Some(format!("{word} is still removing its worktree, {worktree}, so its terminal stays open."))),
                Err(e) => (None, Some(format!("{e} It's still open, its worktree with it."))),
            }
        }
        Ending::Job => match adapter.stop(&ctx, s) {
            Ok(_) => (Some("its background job is stopped".into()), None),
            Err(e) => (None, Some(format!("Its background job didn't stop: {}", words(e)))),
        },
        Ending::Turn => match adapter.interrupt(&ctx, s) {
            Ok(_) => (Some("its turn is stopped".into()), None),
            Err(e) => (None, Some(format!("Its turn didn't stop: {}", words(e)))),
        },
        Ending::Nothing(why) => (None, why),
    }
}

/// Ends a session that asked from the command to be ended once its turn ended (agentdone.rs), as
/// Mark as done ends one, but leaves it unseen and unarchived: its last reply is its user's to
/// read. Says what happened, or why nothing did.
pub fn end_after_turn(engine: &Handle, key: &str) -> Result<String, String> {
    let cfg = cfg()?;
    let board = engine.board();
    let (s, account) = harness::find(&board, &cfg, key).map_err(words)?;
    let ends = crate::idle::ending(&board, s, false)?;
    let job = matches!(ends, crate::idle::Ending::Job);
    let (ended, note) = end(s, account, ends);
    let worktree = job_worktree(engine, &cfg, s, job && ended.is_some()).map(|(words, _)| words);
    Ok([ended.map(|e| format!("“{}”: {e}.", s.title)), note, worktree].into_iter().flatten().collect::<Vec<_>>().join(" "))
}

/// Mark as done on a background job that holds the worktree its harness made for it, once the
/// job has stopped (`stopped`): the worktree goes too when nothing in it would be lost, the
/// job's record with it, or stays (cleanup.rs `after_job`). A terminal session's goes as its
/// harness exits it (`idle::Ending::Exit`). What to say of it, and whether that's worth saying
/// where the session leaving its list says the rest: it stayed, or not all of it went.
fn job_worktree(engine: &Handle, cfg: &config::Config, s: &crate::model::Session, stopped: bool) -> Option<(String, bool)> {
    if !stopped {
        return None;
    }
    // The board as it is when its turn comes, not as it was before the job stopped.
    Some(match crate::cleanup::after_job(&|| engine.board(), cfg, s)? {
        Ok(gone) => (gone.said, gone.partly),
        Err(words) => (words, true),
    })
}

/// Done with a session on another device that won't end it (`why`): Codealong there is from
/// before it could be asked to, or has remote control off. What any linked device may ask still
/// goes: it's marked seen and archived there, and the rest is left as it is. For one that isn't
/// running, that's all Done would have done.
fn done_anyway(engine: &Handle, peers: &Peers, device: &str, own: &str, why: &str) -> Result<Value, String> {
    let ask_for = |name: &str, params: Value| ask(peers, find(name).expect("what any linked device may ask is in the table"), device, params);
    ask_for(api::MARK_SEEN, to_value(params::Keys { keys: vec![own.to_string()] })?)?;
    ask_for(api::SESSION_ARCHIVE, to_value(params::Archive { key: own.to_string(), archived: true })?)?;
    let (title, live, mac) = engine.read(|b| match b.session(&peerfeed::remote_key(device, own)) {
        Some((lane, s)) => (s.title.clone(), s.live, lane.device.as_ref().map(|d| d.name.clone())),
        None => (String::new(), true, None),
    });
    if !live {
        return to_value(api::Done { done: true, message: format!("Done with “{title}”: it's archived."), noted: false });
    }
    let mac = mac.unwrap_or_else(|| "its Mac".into());
    let why = if why.contains(UNKNOWN) { format!("Codealong there is from before it could be asked to close it: update it on {mac}.") } else { why.to_string() };
    let message = format!("Done with “{title}”: it's archived, but still open on {mac}. {why}");
    to_value(api::Done { done: true, message, noted: true })
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

/// A PR's description, reviews, comments and changed files, as this device's `gh` reads them.
/// With it, your draft review of it, which a review would submit: the device shows it first.
fn pr_detail(_engine: &Handle, c: &Call) -> Result<Value, String> {
    let p: params::Pr = c.read()?;
    let (detail, yours) = std::thread::scope(|scope| {
        let detail = scope.spawn(|| crate::github::detail(&p.repo, p.number));
        let yours = scope.spawn(|| crate::github::your_review(&p.repo, p.number));
        (detail.join(), yours.join())
    });
    let mut detail = detail.map_err(|_| "GitHub request failed".to_string())?.map_err(words)?.for_devices();
    detail.draft = yours.ok().and_then(Result::ok).and_then(|y| y.draft).map(|d| d.for_devices());
    serde_json::to_value(detail).map_err(words)
}

/// What a PR's merge box offers you: the engine's own box, in the shape codealong-peer's
/// `pr::MergeBox` reads.
fn pr_merge_box(_engine: &Handle, c: &Call) -> Result<Value, String> {
    let p: params::Pr = c.read()?;
    serde_json::to_value(crate::prmerge::merge_box(&p.repo, p.number).map_err(words)?).map_err(words)
}

/// Reviews a PR as you, as the PR card's box does (lib.rs `review_pr`).
fn pr_review(engine: &Handle, c: &Call) -> Result<Value, String> {
    use codealong_peer::pr::Verdict;
    let p: params::PrReview = c.read()?;
    let verdict = match p.verdict {
        Verdict::Approve => crate::github::Verdict::Approve,
        Verdict::Comment => crate::github::Verdict::Comment,
        Verdict::RequestChanges => crate::github::Verdict::RequestChanges,
    };
    // It submits your draft review, comments and all: only the one the asking device showed, as
    // it was read here.
    let found = crate::github::your_review(&p.repo, p.number).map_err(|e| format!("Couldn't look for a draft review of yours on #{}, which a review would submit: {}", p.number, words(e)))?.draft;
    draft_shown(found.as_ref().map(|d| (d.stamp(), d.count as u64, d.comments.len() as u64)), p.draft.as_deref(), p.number)?;
    let reviewed = crate::github::review_with(&p.repo, p.number, verdict, &p.body, found.as_ref()).map_err(words)?;
    engine.approved(&p.repo, p.number);
    serde_json::to_value(codealong_peer::pr::Reviewed { draft: reviewed.draft.map(|n| n as u64) }).map_err(words)
}

/// Whether a review may go: it submits your draft review of the PR (`found`: its stamp, how many
/// comments it has and how many of them were read), so only when that's the one the asking device
/// showed (`shown`, its stamp), unchanged, and it was shown whole: the stamp can't see comments
/// past the ones read. A draft gone since is no matter: the review goes alone.
fn draft_shown(found: Option<(String, u64, u64)>, shown: Option<&str>, number: u64) -> Result<(), String> {
    match (found, shown) {
        (Some((_, n, _)), None) => Err(format!("You have a draft review of #{number} on GitHub with {n} {}, which this would submit. Open the PR again to read it first.", if n == 1 { "comment" } else { "comments" })),
        (Some((stamp, _, _)), Some(shown)) if stamp != shown => Err(format!("Your draft review of #{number} has changed since it was shown. Open the PR again to read it first.")),
        (Some((_, n, read)), Some(_)) if n > read => Err(format!("Your draft review of #{number} has {n} comments, more than another device is shown: finish it on GitHub.")),
        _ => Ok(()),
    }
}

/// Does what the merge box does, as you (lib.rs `merge_pr`). The op crosses in codealong-peer's
/// shape, which is the engine's own (`prmerge::Op`), as its tests check.
fn pr_merge(engine: &Handle, c: &Call) -> Result<Value, String> {
    let p: params::PrMerge = c.read()?;
    let op: crate::prmerge::Op = serde_json::from_value(serde_json::to_value(&p.op).map_err(words)?).map_err(|e| format!("The merge box's act doesn't read: {e}"))?;
    let acted = crate::prmerge::act(p.number, &p.id, &op).map_err(words)?;
    if acted.merged {
        engine.merged(&p.repo, p.number);
    } else {
        engine.pr_changed();
    }
    serde_json::to_value(codealong_peer::pr::Merged { note: acted.note, merged: acted.merged }).map_err(words)
}

/// Snoozes your review of PRs (`keys`, `owner/name#number`) until `until`, or wakes it.
fn pr_snooze(engine: &Handle, c: &Call) -> Result<Value, String> {
    let p: params::Snooze = c.read()?;
    Ok(shown_at(c, engine.snooze_reviews(p.keys, p.until)))
}

/// Keeps an image another device will send a session or start one with, as a paste into the
/// reply box does (lib.rs `save_attachment`): only an image, which the prompt refers to and
/// every harness's send turns into its way to the file (`attachments::expand`).
fn image_keep(_engine: &Handle, c: &Call) -> Result<Value, String> {
    use crate::attachments;
    let p: params::ImageKeep = c.read()?;
    let bytes = data_encoding::BASE64.decode(p.data.as_bytes()).map_err(|_| "The image didn't arrive whole.".to_string())?;
    if attachments::image_kind(&bytes).is_none() {
        return Err("That isn't an image an agent can be sent: PNG, JPEG, GIF or WebP.".into());
    }
    if bytes.len() > api::IMAGE_MAX_BYTES.min(attachments::MAX_IMAGE_BYTES) {
        return Err(format!("That image is over {} MB.", api::IMAGE_MAX_BYTES / (1024 * 1024)));
    }
    let kept = attachments::keep(&bytes, &p.name).map_err(words)?;
    serde_json::to_value(api::Kept { reference: kept.reference(), path: kept.path }).map_err(words)
}

/// An image a prompt refers to, for another device to show in the conversation: only one
/// Codealong kept (`attachments::locate`), so nothing else on this Mac can be read.
fn image_read(_engine: &Handle, c: &Call) -> Result<Value, String> {
    use crate::attachments;
    let p: params::ImageRead = c.read()?;
    let file = p.path.starts_with(attachments::IMAGES).then(|| attachments::locate(&p.path)).flatten().ok_or("That isn't one of Codealong's images.")?;
    let bytes = kept_image(&file, api::IMAGE_MAX_BYTES)?;
    serde_json::to_value(api::Image { data: data_encoding::BASE64.encode(&bytes), mime: attachments::mime(&p.path).into() }).map_err(words)
}

/// An image file's bytes, when it's a file of its own (not a link, which could lead anywhere)
/// and no bigger than `most`.
fn kept_image(file: &std::path::Path, most: usize) -> Result<Vec<u8>, String> {
    use std::io::Read;
    use std::os::unix::fs::OpenOptionsExt;
    let gone = || "That image is gone from this Mac.".to_string();
    let f = std::fs::OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW).open(file).map_err(|_| gone())?;
    let meta = f.metadata().map_err(|_| gone())?;
    if !meta.is_file() {
        return Err(gone());
    }
    if meta.len() > most as u64 {
        return Err(format!("That image is over {} MB, too big to send.", most / (1024 * 1024)));
    }
    let mut bytes = Vec::with_capacity(meta.len() as usize);
    f.take(most as u64 + 1).read_to_end(&mut bytes).map_err(|_| gone())?;
    if bytes.len() > most {
        return Err(format!("That image is over {} MB, too big to send.", most / (1024 * 1024)));
    }
    Ok(bytes)
}

/// Quiets sessions (`keys`), or makes them heard again with `quiet` false.
fn quiet(engine: &Handle, c: &Call) -> Result<Value, String> {
    let p: params::Quiet = c.read()?;
    Ok(shown_at(c, engine.quiet(p.keys, p.quiet)))
}

/// Pins sessions (`keys`) at the top of the pinned ones, or unpins them with `pinned` false.
fn pin(engine: &Handle, c: &Call) -> Result<Value, String> {
    let p: params::Pin = c.read()?;
    Ok(shown_at(c, engine.pin(p.keys, p.pinned)))
}

/// Makes helpers (`keys`) sessions of your own, for good.
fn make_own(engine: &Handle, c: &Call) -> Result<Value, String> {
    let p: params::Own = c.read()?;
    Ok(shown_at(c, engine.make_own(p.keys)))
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

/// Removes the worktree when nothing in it would be lost, or with `dry_run` only says whether it
/// can go and what would run (cleanup.rs). One that stays answers why, not an error: it's the
/// answer to the question the menu and the command ask.
fn remove_worktree(engine: &Handle, c: &Call) -> Result<Value, String> {
    let p: params::Remove = c.read()?;
    if p.dry_run {
        let plan = crate::cleanup::plan(&engine.board(), &cfg()?, c.key, None)?;
        // What would run only for one that can go: listed for one that stays, the commands read
        // as something to run by hand.
        let steps = if plan.kept.is_none() { plan.steps() } else { Vec::new() };
        let message = match (&plan.kept, &plan.skipped) {
            (Some(why), _) => format!("{} stays: {why}.", plan.name),
            (None, Some(why)) => format!("{} can go: nothing in it would be lost. The repo's WorktreeRemove hook won't run ({why}), so git removes it.", plan.name),
            (None, None) => format!("{} can go: nothing in it would be lost.", plan.name),
        };
        return to_value(api::Removal { removed: plan.kept.is_none(), message, steps, noted: false });
    }
    // What failed partway, as what stays, is said, not an error: the worktree is still there.
    let done = crate::cleanup::remove(&|| engine.board(), &cfg()?, c.key, None)?;
    to_value(match done.result {
        Ok(gone) => api::Removal { removed: true, message: gone.said, steps: done.plan.steps(), noted: gone.partly },
        Err(why) => api::Removal { removed: false, message: why, steps: Vec::new(), noted: false },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Only the command names the session that asked for a start, and only as the one that ran
    /// it: another device's start can't make a helper of a session here.
    #[test]
    fn only_the_command_names_who_asked_for_a_start() {
        let spec = crate::start::StartSpec { lane_id: "/r/app".into(), account: "codex".into(), place: crate::start::Place::Background, prompt: String::new(), model: None, effort: None, worktree: false, todos: Vec::new(), from: Some("claude:claude:boss".into()), helper: true, report: true };
        let named = |s: &crate::start::StartSpec| (s.from.clone(), s.helper, s.report);
        assert_eq!(named(&asked_by(spec.clone(), &From::Command(Some("claude:claude:boss".into())))), (Some("claude:claude:boss".into()), true, true));
        for from in [From::Command(Some("claude:claude:other".into())), From::Command(None), From::Device, From::Phone, From::Here] {
            assert_eq!(named(&asked_by(spec.clone(), &from)), (None, false, false));
        }
    }

    /// A message you send takes an archived session out of the archive, from a window, your phone
    /// or another Mac; an agent's message here leaves your archive alone.
    #[test]
    fn a_message_takes_a_session_out_of_the_archive_unless_an_agent_sent_it() {
        for from in [From::Here, From::Phone, From::Device] {
            assert!(unarchives(&from, true), "{from:?}");
            assert!(!unarchives(&from, false), "{from:?}");
        }
        for from in [From::Command(Some("claude:claude:boss".into())), From::Command(None)] {
            assert!(!unarchives(&from, true), "{from:?}");
        }
    }

    #[test]
    fn a_review_from_another_device_submits_only_the_draft_it_showed() {
        let found = |stamp: &str, n: u64| Some((stamp.to_string(), n, n));
        assert!(draft_shown(None, None, 7).is_ok());
        assert!(draft_shown(found("ab", 3), Some("ab"), 7).is_ok());
        // Gone since it was shown: the review goes alone.
        assert!(draft_shown(None, Some("ab"), 7).is_ok());
        assert!(draft_shown(found("ab", 1), None, 7).unwrap_err().contains("with 1 comment,"));
        // Edited since, as many comments as before.
        assert!(draft_shown(found("cd", 3), Some("ab"), 7).unwrap_err().contains("changed since it was shown"));
        // More comments than were read: one past them could have changed unseen.
        assert!(draft_shown(Some(("ab".into(), 101, 100)), Some("ab"), 7).unwrap_err().contains("101 comments, more than"));
    }

    #[test]
    fn an_image_is_read_only_from_a_file_of_its_own_and_not_too_big() {
        let dir = std::env::temp_dir().join(format!("codealong-kept-image-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let image = dir.join("0123456789abcdef.png");
        std::fs::write(&image, b"\x89PNG\r\n\x1a\n1234").unwrap();
        assert_eq!(kept_image(&image, 100).unwrap().len(), 12);
        assert!(kept_image(&image, 8).unwrap_err().contains("too big"));
        // A link where an image would be, leading out of the folder, isn't followed.
        let secret = dir.join("secret.txt");
        std::fs::write(&secret, b"secret").unwrap();
        let link = dir.join("fedcba9876543210.png");
        let _ = std::fs::remove_file(&link);
        std::os::unix::fs::symlink(&secret, &link).unwrap();
        assert!(kept_image(&link, 100).is_err());
        assert!(kept_image(&dir, 100).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

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
        let err = answer(&engine, FORK_TERMINAL, json!({ "key": "claude:claude:abc" }), false).unwrap_err();
        assert!(err.contains("doesn't know how to"), "{err}");
        let err = answer(&engine, "session-launch-rockets", json!({}), true).unwrap_err();
        assert!(err.contains("update it here"), "{err}");
        let err = answer(&engine, api::SESSION_PAGE, json!({}), false).unwrap_err();
        assert_eq!(err, "The request names no session.");
    }

    fn no_devices() -> (Peers, crate::fixtures::Scratch) {
        let dir = crate::fixtures::scratch("actions");
        let hooks = crate::peers::Hooks { board: Box::new(crate::model::Board::default), answer: Box::new(|_, _, _| Err("no".into())), emit: Box::new(|_| {}) };
        (Peers::new(dir.to_path_buf(), hooks), dir)
    }

    #[test]
    fn what_is_on_another_device_goes_there_unless_it_needs_someone_at_that_mac() {
        let (peers, _dir) = no_devices();
        let engine = Handle::for_tests();
        // A worktree's targets open, and its preview and reuse mark are set, on its own Mac.
        let lane = "@a1b2c3d4e5:/Users/sam/app";
        for (name, words) in [(OPEN, "it opens there"), (PREVIEW, "set its preview there"), (REUSABLE, "mark it there"), (RUN_COMMAND, "on the Mac the session runs on")] {
            let err = run(&engine, &peers, name, lane, json!({ "target": "editor", "on": true })).unwrap_err();
            assert!(err.contains(words), "{name}: {err}");
        }
        // What another device can do is asked of it: this one has none linked, so the asking fails.
        let spec = json!({ "spec": { "laneId": lane, "account": "claude", "place": "terminal", "prompt": "", "worktree": false, "todos": [] } });
        assert!(run(&engine, &peers, api::START, lane, spec).unwrap_err().contains("isn't linked"));
        assert!(run_on(&engine, &peers, api::START_OPTIONS, Some("a1b2c3d4e5"), json!({})).unwrap_err().contains("isn't linked"));
        // Another device's sessions are snoozed there: with none linked, the window hears why it
        // wasn't, and this device's own still answer.
        let snooze = |keys: &[&str]| run_keys_heard(&engine, &peers, api::SESSION_SNOOZE, keys.iter().map(|k| k.to_string()).collect(), json!({ "until": 1 }));
        assert!(snooze(&["@a1b2c3d4e5:claude:claude:abc"]).unwrap_err().contains("isn't linked"));
        assert_eq!(snooze(&["claude:claude:def"]), Ok(Some(Value::Null)));
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
        let unread = find(api::MARK_UNREAD).unwrap();
        assert_eq!((unread.there)(json!({ "keys": ["claude:claude:abc"] }), "").unwrap(), json!({ "keys": ["claude:claude:abc"] }));
        let snooze = find(api::SESSION_SNOOZE).unwrap();
        assert_eq!((snooze.there)(json!({ "keys": ["claude:claude:abc"], "until": 1 }), "").unwrap(), json!({ "keys": ["claude:claude:abc"], "until": 1 }));
        let quiet = find(api::SESSION_QUIET).unwrap();
        assert_eq!((quiet.there)(json!({ "keys": ["claude:claude:abc"], "quiet": true }), "").unwrap(), json!({ "keys": ["claude:claude:abc"], "quiet": true }));
        let pin = find(api::SESSION_PIN).unwrap();
        assert_eq!((pin.there)(json!({ "keys": ["claude:claude:abc"], "pinned": true }), "").unwrap(), json!({ "keys": ["claude:claude:abc"], "pinned": true }));
        // Suggested replies go with the session's key there, and the suggester as its own device
        // knows it.
        let suggest = find(api::SESSION_SUGGEST).unwrap();
        let asked = json!({ "replies": ["Yes"], "by": "claude:claude:sup", "byTitle": "Supervisor" });
        assert_eq!((suggest.there)(asked, "claude:claude:abc").unwrap(), json!({ "replies": ["Yes"], "by": "claude:claude:sup", "byTitle": "Supervisor", "key": "claude:claude:abc" }));
        // A start goes with the worktree as that device knows it and none of this device's todos,
        // and the session it started comes back by the key it has here.
        let start = find(api::START).unwrap();
        let spec = json!({ "laneId": "@a1b2c3d4e5:/Users/sam/app", "account": "claude", "place": "background", "prompt": "Fix the build", "model": null, "effort": null, "worktree": false, "todos": ["t-1"], "from": "claude:claude:me" });
        let there = (start.there)(json!({ "spec": spec }), "/Users/sam/app").unwrap();
        assert_eq!(there, json!({ "spec": { "laneId": "/Users/sam/app", "account": "claude", "place": "background", "prompt": "Fix the build", "model": null, "effort": null, "worktree": false, "todos": [] } }));
        let back = (start.back)(json!({ "key": "claude:claude:new", "message": "Started." }), "a1b2c3d4e5").unwrap();
        assert_eq!(back["key"], "@a1b2c3d4e5:claude:claude:new");
        // So does the copy a fork there made, for what's asked of it next to go there too; one
        // whose key isn't known yet comes back without one.
        let fork = find(api::SESSION_FORK).unwrap();
        assert_eq!((fork.there)(json!({ "prompt": "Try it the other way" }), "claude:claude:abc").unwrap(), json!({ "key": "claude:claude:abc", "prompt": "Try it the other way" }));
        let back = (fork.back)(json!({ "key": "claude:claude:copy", "message": "Forked in the background." }), "a1b2c3d4e5").unwrap();
        assert_eq!(back, json!({ "key": "@a1b2c3d4e5:claude:claude:copy", "message": "Forked in the background." }));
        assert_eq!(peerfeed::split_remote(back["key"].as_str().unwrap()), Some(("a1b2c3d4e5", "claude:claude:copy")));
        assert_eq!((fork.back)(json!({ "key": null, "message": null }), "a1b2c3d4e5").unwrap(), json!({ "key": null, "message": null }));
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
        // A command led by `!` runs in a terminal of its user's only as they ran it: an agent asks
        // with `codealong terminal run`, which they allow.
        assert!(ask(RUN_COMMAND, json!({ "text": "! make", "folder": "/tmp" })).unwrap_err().contains("`codealong terminal run`"));
        // The rest is the command's as it is a window's: these get as far as the session. Mark as
        // done goes by what an agent may mark done once it has the session (agentdone.rs).
        for name in [api::SESSION_PENDING, api::SESSION_CANCEL, api::SESSION_RENAME, api::SESSION_FORK, api::SESSION_SEND, CLOSE, api::SESSION_DONE] {
            let err = ask(name, json!({ "name": "A name", "text": "hi" })).unwrap_err();
            assert!(err.contains("isn't on the board"), "{name}: {err}");
        }
        // Nor does it resume a session, on a prompt or on none, as a message from it doesn't.
        for params in [json!({ "prompt": "Go on" }), json!({ "resume": { "place": "background" }, "prompt": "" })] {
            let err = ask(api::SESSION_RESUME, params).unwrap_err();
            assert_eq!(err, "That session isn't running, and a message from another session doesn't start it again, so nothing was sent. Its user can resume it, or `codealong start` starts a new session.");
        }
        // A window may, and so may a device: both get as far as the session.
        for from in [From::Here, From::Device] {
            let err = local(&engine, &from, api::SESSION_RESUME, "claude:claude:abc", json!({ "prompt": "Go on" })).unwrap_err();
            assert!(err.contains("isn't on the board"), "{from:?}: {err}");
        }
        // Nor may it have another device do any of that, or send a prompt there, where it would
        // arrive as the user's own.
        let deny = json!({ "id": "t1", "answer": "deny" });
        assert!(command_may_ask(api::SESSION_ANSWER, &deny).is_ok() && command_may_ask(api::SESSION_PAGE, &json!({})).is_ok());
        assert!(command_may_ask(api::SESSION_ANSWER, &json!({ "id": "t1", "answer": "allow" })).unwrap_err().contains("can only deny"));
        // There, Mark as done would be its user's, not an agent's that may mark only its own.
        assert!(command_may_ask(api::SESSION_DONE, &json!({})).unwrap_err().contains("where the command doesn't mark sessions done"));
        // A deny with words would hand the session an agent's words as its user's.
        let saying = json!({ "id": "t1", "answer": "deny", "message": "Do it this way" });
        assert!(command_may_ask(api::SESSION_ANSWER, &saying).unwrap_err().contains("not say what to do instead"));
        assert!(command_may_ask(api::SESSION_ANSWER, &json!({ "id": "t1", "answer": "deny", "message": " " })).is_ok());
        assert!(command_may_ask(api::SESSION_SEND, &json!({ "text": "hi" })).unwrap_err().contains("arrive as its user's own"));
        assert!(command_may_ask(api::SESSION_RESUME, &json!({ "prompt": "Go on" })).unwrap_err().contains("doesn't start it again"));
        // Another device's session isn't resumed for it, where a window's request goes there (and
        // finds it isn't linked).
        let (peers, _dir) = no_devices();
        let there = "@a1b2c3d4e5:claude:claude:abc";
        let err = run_as(&engine, &peers, &command, api::SESSION_RESUME, On::Key(there), json!({ "prompt": "Go on" })).unwrap_err();
        assert!(err.contains("doesn't start it again"), "{err}");
        assert!(run_as(&engine, &peers, &From::Here, api::SESSION_RESUME, On::Key(there), json!({ "prompt": "Go on" })).unwrap_err().contains("isn't linked"));
        let send = find(api::SESSION_SEND).unwrap();
        assert!(elsewhere(send, &command).is_err() && elsewhere(send, &From::Here).is_ok());
        assert!(elsewhere(find(FORK_TERMINAL).unwrap(), &From::Here).unwrap_err().contains("on the Mac it runs on"));
        // A window may allow, and so may a device, which peers.rs lets through only with remote
        // control on: both get as far as the session.
        for from in [From::Here, From::Device, From::Phone] {
            let err = local(&engine, &from, api::SESSION_ANSWER, "claude:claude:abc", json!({ "id": "t1", "answer": "allow" })).unwrap_err();
            assert!(err.contains("isn't on the board"), "{from:?}: {err}");
        }
    }

    /// What you write on your phone is yours, as what you write in a window here is: a phone runs
    /// no agents. Another Mac's request may have come from an agent there, and claims nothing.
    #[test]
    fn a_phones_messages_are_its_users_own() {
        assert_eq!(sender_of(&From::Here), send::Sender::You);
        assert_eq!(sender_of(&From::Phone), send::Sender::You);
        assert_eq!(sender_of(&From::Device), send::Sender::Elsewhere);
        assert_eq!(sender_of(&From::Command(None)), send::Sender::Agent(None));
    }

    #[test]
    fn mark_as_done_says_what_became_of_its_helpers() {
        assert_eq!(helpers_done_words(0, 0), None);
        assert_eq!(helpers_done_words(1, 0).as_deref(), Some("Its helper is done too."));
        assert_eq!(helpers_done_words(3, 0).as_deref(), Some("Its 3 helpers are done too."));
        assert_eq!(helpers_done_words(2, 1).as_deref(), Some("Its 2 helpers are done too; one still at work is left to finish."));
        assert_eq!(helpers_done_words(1, 2).as_deref(), Some("Its helper is done too; 2 still at work are left to finish."));
        assert_eq!(helpers_done_words(0, 1).as_deref(), Some("Its helper still at work is left to finish."));
        assert_eq!(helpers_done_words(0, 2).as_deref(), Some("Its 2 helpers still at work are left to finish."));
    }
}
