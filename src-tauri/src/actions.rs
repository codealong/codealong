//! What a device does about its sessions when asked, in one table (`REQUESTS`): each request's
//! name (what one device asks another: codealong-peer's `api`), what it does on this device, and,
//! for the few that need someone at the Mac a session runs on, why another device can't ask for
//! it. A window's command about a session goes through `run`, which does it here or asks the
//! device the session is on; a linked device's request goes through `answer`, which does it here.
//! So a request does the same whether a window here, another Mac or a phone asks, and who may
//! ask for it from elsewhere is `api::REQUESTS`'s to say, once.

use std::time::Duration;

use codealong_peer::api::{self, Reach};
use serde_json::{json, Value};

use crate::engine::Handle;
use crate::peers::Peers;
use crate::{acp, config, harness, peerfeed, send, transcript};

/// What only happens on the device a session runs on: another device can't ask for them, so
/// they have names here but none in `api`.
pub const FORK_TERMINAL: &str = "session-fork-terminal";
pub const MOVE: &str = "session-move";
pub const CLOSE: &str = "session-close";
pub const SIDE_ASK: &str = "session-side-ask";
pub const SIDE_CLOSE: &str = "session-side-close";

/// Who's asking.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum From {
    /// A window on this device: you, at this Mac.
    Here,
    /// A linked device, whose request peers.rs has let through (`api::reach`).
    Device,
}

/// A request as a function gets it: `key` names the session on this device (empty for a
/// request about none), `params` the rest.
pub struct Call<'a> {
    pub key: &'a str,
    pub params: &'a Value,
    pub from: From,
}

impl Call<'_> {
    /// The session it's about, for a request that must name one.
    fn session(&self) -> Result<&str, String> {
        Some(self.key).filter(|k| !k.is_empty()).ok_or_else(|| "The request names no session.".to_string())
    }

    fn text(&self, name: &str) -> String {
        self.params.get(name).and_then(Value::as_str).unwrap_or_default().to_string()
    }

    fn flag(&self, name: &str) -> bool {
        self.params.get(name).and_then(Value::as_bool).unwrap_or(false)
    }

    fn u64(&self, name: &str) -> Option<u64> {
        self.params.get(name).and_then(Value::as_u64)
    }
}

struct Request {
    name: &'static str,
    /// Why another device can't ask for it; None when one can (`api::reach` says which).
    only_here: Option<&'static str>,
    here: fn(&Handle, &Call) -> Result<Value, String>,
}

const REQUESTS: &[Request] = &[
    Request { name: api::SESSION_PAGE, only_here: None, here: page },
    Request { name: api::SESSION_NEWER, only_here: None, here: newer },
    Request { name: api::SESSION_PENDING, only_here: None, here: pending },
    Request { name: api::SESSION_WAY, only_here: None, here: way },
    Request { name: api::SESSION_STREAM, only_here: None, here: stream },
    Request { name: api::SESSION_ANSWER, only_here: None, here: answer_call },
    Request { name: api::SESSION_SEND, only_here: None, here: send_prompt },
    Request { name: api::SESSION_CANCEL, only_here: None, here: cancel },
    Request { name: api::SESSION_ARCHIVE, only_here: None, here: archive },
    Request { name: api::SESSION_RENAME, only_here: None, here: rename },
    Request { name: api::MARK_SEEN, only_here: None, here: mark_seen },
    Request { name: api::SEARCH, only_here: None, here: search },
    Request { name: api::ROUTE, only_here: None, here: route },
    Request { name: api::START_OPTIONS, only_here: None, here: start_options },
    Request { name: api::START, only_here: None, here: start },
    Request { name: api::SESSION_FORK, only_here: None, here: fork },
    // A fork into a terminal opens one, and closing one ends it: both need someone at that Mac.
    Request { name: FORK_TERMINAL, only_here: Some("Fork it on the Mac it runs on."), here: fork_terminal },
    // A move opens a terminal too, and closes the session's own.
    Request { name: MOVE, only_here: Some("Move it on the Mac it runs on."), here: move_session },
    Request { name: CLOSE, only_here: Some("Close it on the Mac it runs on."), here: close },
    // A side conversation's copy runs on the session's Mac, and its answer goes to the windows there.
    Request { name: SIDE_ASK, only_here: Some("Ask it on the side on the Mac it runs on."), here: side_ask },
    Request { name: SIDE_CLOSE, only_here: Some("Its side conversation is on the Mac it runs on."), here: side_close },
];

fn find(name: &str) -> Option<&'static Request> {
    REQUESTS.iter().find(|r| r.name == name)
}

/// Does `name` for the session `key` from a window here: on this device, or, for a session on
/// another device, by asking that device (with the key it knows the session by).
pub fn run(engine: &Handle, peers: &Peers, name: &str, key: &str, params: Value) -> Result<Value, String> {
    let req = find(name).ok_or_else(|| format!("Codealong doesn't know how to {name}."))?;
    match peerfeed::split_remote(key) {
        Some((device, own)) => match req.only_here {
            Some(why) => Err(why.to_string()),
            None => {
                let mut params = params;
                if let Value::Object(m) = &mut params {
                    m.insert("key".into(), json!(own));
                }
                ask(peers, device, name, params)
            }
        },
        None => (req.here)(engine, &Call { key, params: &params, from: From::Here }),
    }
}

/// Asks another device for `name`, waiting as long as its kind takes: a read briefly, an action
/// longer (it may start an app, or type into a terminal).
pub fn ask(peers: &Peers, device: &str, name: &str, params: Value) -> Result<Value, String> {
    let wait = if api::reach(name) == Some(Reach::Read) { Duration::from_secs(15) } else { Duration::from_secs(40) };
    peers.ask(device, name, params, wait)
}

/// Answers a linked device's request (peers.rs has checked it may make it) as a window's would be.
pub fn answer(engine: &Handle, name: &str, params: Value) -> Result<Value, String> {
    let req = find(name).filter(|r| r.only_here.is_none()).ok_or_else(|| format!("Codealong on this device doesn't know how to {name} yet; update it here."))?;
    let key = params.get("key").and_then(Value::as_str).unwrap_or_default().to_string();
    (req.here)(engine, &Call { key: &key, params: &params, from: From::Device })
}

fn cfg() -> Result<config::Config, String> {
    Ok(config::load().map_err(|e| format!("{e:#}"))?.unwrap_or_default())
}

fn to_value<T: serde::Serialize>(v: T) -> Result<Value, String> {
    serde_json::to_value(v).map_err(|e| e.to_string())
}

fn words(e: anyhow::Error) -> String {
    format!("{e:#}")
}

/// A page of the conversation, from its own record (transcript.rs): the newest without
/// `before`, else what came before it.
fn page(engine: &Handle, c: &Call) -> Result<Value, String> {
    to_value(transcript::page(&engine.board(), &cfg()?, c.session()?, c.u64("before"), 60).map_err(words)?)
}

/// What it said and did after `after`, as it goes on.
fn newer(engine: &Handle, c: &Call) -> Result<Value, String> {
    to_value(transcript::newer(&engine.board(), &cfg()?, c.session()?, c.u64("after").unwrap_or(0)).map_err(words)?)
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
    let answer: harness::Answer = c.params.get("answer").cloned().and_then(|a| serde_json::from_value(a).ok()).ok_or("The request has no answer.")?;
    let id = c.text("id");
    if acp::hosts(key) {
        return acp::answer(key, &id, answer).map(Value::String).map_err(words);
    }
    let (cfg, board) = (cfg()?, engine.board());
    let (s, account) = harness::find(&board, &cfg, key).map_err(words)?;
    let ctx = harness::Ctx { account };
    match c.params.get("answers").filter(|a| !a.is_null()) {
        Some(a) => harness::adapter(s.harness).answer_questions(&ctx, s, &id, a),
        None => harness::adapter(s.harness).answer(&ctx, s, &id, answer),
    }
    .map(Value::String)
    .map_err(words)
}

/// Sends it a prompt where it runs (send.rs), into its running turn at once with `now`. From
/// another device, one that would only open with the prompt ready to send is refused: that
/// needs someone here.
fn send_prompt(engine: &Handle, c: &Call) -> Result<Value, String> {
    let sender = if c.from == From::Here { send::Sender::You } else { send::Sender::Elsewhere };
    let sent = send::send_here(engine, c.session()?, &c.text("text"), sender, c.flag("now")).map_err(words)?;
    if c.from == From::Device && sent.opened {
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
    if c.from == From::Here { json!(at) } else { Value::Null }
}

/// Archives it, or unarchives it with `archived: false` (Codex threads in Codex itself).
fn archive(engine: &Handle, c: &Call) -> Result<Value, String> {
    let archived = c.params.get("archived").and_then(Value::as_bool).unwrap_or(true);
    let at = engine.archive(c.session()?, archived).map_err(words)?;
    Ok(shown_at(c, at))
}

/// Names it where its harness keeps its name, so the harness shows it too.
fn rename(engine: &Handle, c: &Call) -> Result<Value, String> {
    let at = engine.rename(c.session()?, &c.text("name")).map_err(words)?;
    Ok(shown_at(c, at))
}

/// Marks finished turns seen, several sessions at once (`keys`).
fn mark_seen(engine: &Handle, c: &Call) -> Result<Value, String> {
    let keys: Vec<String> = c.params.get("keys").cloned().and_then(|k| serde_json::from_value(k).ok()).unwrap_or_default();
    Ok(match c.from {
        From::Here => json!(engine.mark_seen_shown(keys)),
        From::Device => {
            engine.mark_seen(keys);
            Value::Null
        }
    })
}

/// What was said in this device's sessions.
fn search(_: &Handle, c: &Call) -> Result<Value, String> {
    let words: Vec<String> = c.params.get("words").cloned().and_then(|w| serde_json::from_value(w).ok()).unwrap_or_default();
    to_value(crate::search::typed(&words))
}

/// Where typed text goes, as the quick panel suggests it (route.rs).
fn route(engine: &Handle, c: &Call) -> Result<Value, String> {
    let project = c.params.get("project").and_then(Value::as_str).filter(|p| !p.is_empty()).unwrap_or(crate::route::ALL_PROJECTS);
    to_value(crate::route::suggest(&engine.board(), project, &c.text("text")).map_err(words)?)
}

/// Where each account here can start a session.
fn start_options(_: &Handle, _: &Call) -> Result<Value, String> {
    to_value(crate::start::options_here().map_err(words)?)
}

/// Starts a session in a worktree here and links it to its todos.
fn start(engine: &Handle, c: &Call) -> Result<Value, String> {
    let spec: crate::start::StartSpec = c.params.get("spec").cloned().and_then(|s| serde_json::from_value(s).ok()).ok_or("The request has no start.")?;
    to_value(crate::start::start_here(engine, &spec).map_err(words)?)
}

/// Forks it into a copy that goes on without a window, on `prompt` when there's one
/// (`start::fork_background`).
fn fork(engine: &Handle, c: &Call) -> Result<Value, String> {
    to_value(crate::start::fork_background(&engine.board(), &cfg()?, c.session()?, &c.text("prompt")).map_err(words)?)
}

/// Forks it into a new terminal in its folder (`start::fork`).
fn fork_terminal(engine: &Handle, c: &Call) -> Result<Value, String> {
    to_value(crate::start::fork(&engine.board(), &cfg()?, c.session()?).map_err(words)?)
}

/// Moves it to another account of its harness, `account` (`start::move_to`): the copy goes on
/// in a new terminal, and the session is closed and archived: from here on the conversation is
/// the copy's, and so are the todos it was on and its place among the pinned sessions.
fn move_session(engine: &Handle, c: &Call) -> Result<Value, String> {
    let key = c.session()?;
    let cfg = cfg()?;
    let stopped = stop_to_move(engine, &cfg, key)?;
    let board = engine.board();
    let mut started = crate::start::move_to(&board, &cfg, key, &c.text("account")).map_err(words)?;
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
    to_value(crate::side::ask(&engine.board(), &cfg()?, c.session()?, &c.text("text")).map_err(words)?)
}

/// Ends its side conversation: the copy stops, and what was said is kept to read (`side::past`).
fn side_close(_: &Handle, c: &Call) -> Result<Value, String> {
    crate::side::close(c.session()?);
    Ok(Value::Null)
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
}
