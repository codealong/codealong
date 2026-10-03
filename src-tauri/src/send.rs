//! Sending a prompt to a session that already exists, the way its harness takes one in.
//!
//! Each harness says which ways reach a session, best first (`Adapter::routes`), and they're
//! tried in that order until one works. Two are the same for every harness and are done here:
//! typing the prompt into the session's terminal, where it arrives as if you'd typed it, and
//! resuming a session that isn't running in a new terminal, on the prompt. The others are a
//! harness's own (`Own`: its background service, a socket its sessions listen on, its app),
//! and its module sends by them.
//!
//! What you type in Codealong's windows here is yours, as if typed in the session. A message
//! from another device isn't known to be, and an agent's never is. Every way in is told whose
//! a message is and whether its sender skips permission prompts itself, so a harness that
//! holds another session's message for its user to allow can't be got past through Codealong
//! (claude/peer.rs).
//!
//! A session that's waiting on you for a permission prompt, or stuck mid-turn, is never typed
//! into: what's typed would answer the prompt. One whose last reply asks you something is at its
//! prompt, so what's typed is its answer. Busy sessions are fine; each harness queues the prompt
//! until the agent can take it.
//!
//! An agent's message (`codealong session send`) is never yours. It starts with who sent it, that
//! it isn't from the session's user, and how to answer, and it goes only the ways that don't read
//! as the user's own words or need someone at this Mac: a harness's own way in for other
//! sessions' messages where it has one (`Adapter::takes_messages`), which tells the model as
//! much, and for the harnesses that have none, their terminal or service. It never resumes a
//! session that isn't running, and never leaves a prompt for someone to send.

use std::borrow::Cow;
use std::path::Path;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use anyhow::{anyhow, bail, Result};

use crate::attachments::{self, Style};
use crate::config::{AccountCfg, Config};
use crate::launch;
use crate::model::{Board, Session, Status};
use crate::procs;

/// How a send went, and how one would go; they go back to other devices too, so they're in
/// codealong-peer.
pub use codealong_peer::api::{Sent, Way};

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
    /// The name its sender goes by, for a way in that says whose a message is.
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

/// An agent's message without the byline it starts with, as a note of it shows it
/// (claude/conversation.rs, `peer_item`): the byline says whose it is, which the note says
/// already.
pub fn without_byline(text: &str) -> &str {
    let t = text.trim();
    let starts = ["This message is from another agent, ", "This message was sent with `codealong session send`"];
    match t.split_once("\n\n") {
        Some((intro, rest)) if starts.iter().any(|s| intro.starts_with(s)) => rest.trim(),
        _ => t,
    }
}

/// What goes to the session: the text as it gets it, before its attachments are written out for
/// the way in, who it's from, and whether it's to go in now.
struct Message<'a> {
    text: Cow<'a, str>,
    sender: &'a Sender,
    /// Sent now: into the turn the session is taking at once, rather than once the step it's on
    /// ends (`harness::steerable`), by its harness's own way where the way in has one.
    now: bool,
    /// An agent's: its text starts with the intro.
    byline: Option<Byline>,
    /// Whether its sender skips permission prompts itself, so that the message may say so to a
    /// session that skips them too: you, or an agent whose own session does.
    bypasses: bool,
}

impl<'a> Message<'a> {
    fn new(board: &Board, cfg: &Config, text: &'a str, sender: &'a Sender, now: bool) -> Message<'a> {
        match sender {
            Sender::Agent(from) => {
                let b = byline(board, from.as_deref());
                let bypasses = from.as_deref().and_then(|k| crate::harness::find(board, cfg, k).ok()).is_some_and(|(s, account)| s.live && crate::harness::adapter(s.harness).skips_permission_prompts(&crate::harness::Ctx { account }, s));
                Message { text: Cow::Owned(format!("{}\n\n{}", b.intro, text.trim())), sender, now, byline: Some(b), bypasses }
            }
            _ => Message { text: Cow::Borrowed(text), sender, now, byline: None, bypasses: *sender == Sender::You },
        }
    }

    fn from_agent(&self) -> bool {
        matches!(self.sender, Sender::Agent(_))
    }

    /// The message as one of a harness's own ways in gets it.
    fn delivery<'b>(&'b self, session: &'b Session, account: &'b AccountCfg, untyped: Option<&'b str>) -> Delivery<'b> {
        Delivery { session, account, text: &self.text, agent: self.byline.as_ref().map(|b| b.name.as_str()), bypasses: self.bypasses, now: self.now_for(session), untyped }
    }

    /// Whether it goes in now: sending now matters only while a turn runs, since otherwise the
    /// prompt starts one either way.
    fn now_for(&self, session: &Session) -> bool {
        self.now && session.status == Status::Working
    }
}

/// How a prompt reaches a session, tried in order until one works.
#[derive(Debug, Clone)]
pub enum Route {
    /// Pasted into the session's terminal tab, then Return.
    Type { pid: u32 },
    /// Not running: a new terminal resumes it with the prompt.
    Resume,
    /// A way in of the harness's own, which its module sends by.
    Own(Arc<dyn Own>),
}

/// One of a harness's own ways in, as its adapter lists it (`Adapter::routes`): its background
/// service, a socket its sessions listen on, its app. The harness's module has the type, and
/// with it everything about how a prompt goes that way.
pub trait Own: std::fmt::Debug + Send + Sync {
    /// What it comes to, for the session's box to say before you send (`way_in`). The words
    /// go to other devices too, so they're codealong-peer's.
    fn way(&self) -> Way;

    /// Whether another agent's message may go this way: one that tells the session whose the
    /// message is, or posts it with nobody needed at this Mac. Not one where it would read as
    /// the session's user's own words, or that leaves the prompt for someone to send.
    fn takes_agents(&self) -> bool;

    /// Nothing takes the prompt this way: the harness takes none from outside, and it's only
    /// left on the clipboard for someone at this Mac to paste. An agent is told as much.
    fn leaves_it_to_paste(&self) -> bool {
        false
    }

    /// Sends the message this way, and says how it went.
    fn deliver(&self, d: &Delivery) -> Result<Sent>;
}

impl<T: Own + 'static> From<T> for Route {
    fn from(own: T) -> Route {
        Route::Own(Arc::new(own))
    }
}

/// Two ways in are the same, for a test, when they read the same.
#[cfg(test)]
impl PartialEq for Route {
    fn eq(&self, other: &Route) -> bool {
        format!("{self:?}") == format!("{other:?}")
    }
}

/// What a harness's own way in is handed to send.
pub struct Delivery<'a> {
    pub session: &'a Session,
    pub account: &'a AccountCfg,
    /// The text as the session gets it, before its attachments are written out for the way in.
    pub text: &'a str,
    /// An agent's message: the name its sender's session goes by, for a way in that says whose
    /// a message is. None for yours.
    pub agent: Option<&'a str>,
    /// Whether its sender skips permission prompts itself, so that the message may say so to a
    /// session that skips them too: you, or an agent whose own session does.
    pub bypasses: bool,
    /// Sent now, while the session's turn runs: into that turn at once, by the harness's own way
    /// to (`harness::steerable`), rather than once the step it's on ends.
    pub now: bool,
    /// Why typing into the session's terminal didn't work, when that was tried first.
    pub untyped: Option<&'a str>,
}

impl Delivery<'_> {
    pub fn from_agent(&self) -> bool {
        self.agent.is_some()
    }

    /// The session's title as a sentence names it.
    pub fn title(&self) -> String {
        quoted(&self.session.title)
    }

    /// The session is in the middle of a turn, and takes the prompt after it.
    pub fn busy(&self) -> bool {
        self.session.status == Status::Working
    }
}

/// The prompt went, where it lives.
pub fn sent(message: String) -> Sent {
    Sent { message, opened: false, fell_back: false }
}

/// The prompt is in a window that came forward, for someone to send from there.
pub fn opened(message: String) -> Sent {
    Sent { message, opened: true, fell_back: false }
}

/// Sends `text` to the session with this key; `now` sends it into the turn the session is
/// taking at once, where its way in can (`harness::steerable`), and otherwise as any send goes.
/// The same text sent to the same session while it's still on its way (the peek closed and
/// opened again, and you sent what it still showed, or another window sent it too) joins that
/// send and gets its outcome, rather than going twice.
pub fn send(board: &Board, cfg: &Config, session_key: &str, text: &str, sender: Sender, now: bool) -> Result<Sent> {
    // A message to a session that moved to another account would bring it back to life where it
    // was, beside the copy that goes on: say where it went, for the sender to send there.
    if let Some(copy) = crate::origins::current().moved_to(session_key, |k| board.session(k).is_some()) {
        bail!("{}", moved_words(board, session_key, &copy));
    }
    join(session_key, text.trim(), || deliver(board, cfg, session_key, text, &sender, now))
}

/// Where a moved session went, in a sentence that names the session to send to instead.
pub fn moved_words(board: &Board, key: &str, copy: &str) -> String {
    let title = board.session(key).map_or_else(|| key.to_string(), |(_, s)| format!("“{}”", s.title));
    let account = copy.split(':').nth(1).unwrap_or_default();
    format!("{title} moved to {account} and goes on there as {copy}. Send to that session instead.")
}

/// Sends to a session on this device, on the board the windows have and the settings as they
/// stand: what a window's reply box, another device's request and `codealong session send`
/// through the app all come to.
pub fn send_here(engine: &crate::engine::Handle, key: &str, text: &str, sender: Sender, now: bool) -> Result<Sent> {
    let cfg = crate::config::current()?;
    send(&engine.board(), &cfg, key, text, sender, now)
}

/// How a message from you would reach the session with this key, for its box to say before you
/// send it: the first of its ways in (`Adapter::routes`) that applies, as `send` tries them. None
/// for a session Codealong runs itself, which its box says in its own words.
pub fn way_in(board: &Board, cfg: &Config, key: &str) -> Result<Option<Way>> {
    let (session, account) = crate::harness::find(board, cfg, key)?;
    if session.hosted {
        return Ok(None);
    }
    let routes = crate::harness::adapter(session.harness).routes(account, session);
    let resume_in = launch::terminal_name(&cfg.apps.terminal);
    Ok(routes.iter().find_map(|r| way_of(r, &|pid| launch::terminal_kind(pid).map(|k| k.name()), &resume_in)))
}

/// What a way in comes to, with `terminal` naming the terminal a process runs in: typing goes
/// into that one, and a session in no terminal Codealong can type into passes to its next way,
/// as a send does.
fn way_of(route: &Route, terminal: &dyn Fn(u32) -> Option<&'static str>, resume_in: &str) -> Option<Way> {
    Some(match route {
        Route::Type { pid } => Way::Typed { app: terminal(*pid)?.to_string() },
        Route::Resume => Way::Resume { app: resume_in.to_string() },
        Route::Own(own) => own.way(),
    })
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
pub(crate) struct MayHaveArrived(pub(crate) String);

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

fn deliver(board: &Board, cfg: &Config, session_key: &str, text: &str, sender: &Sender, now: bool) -> Result<Sent> {
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
    let msg = Message::new(board, cfg, text, sender, now);
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
pub(crate) fn ensure_not_waiting(session: &Session, sender: &Sender) -> Result<()> {
    if session.status == Status::Needs && !session.asks {
        match sender {
            Sender::Agent(_) => bail!("{} is waiting on its user to answer a prompt, so nothing was sent. Try again once they have.", quoted(&session.title)),
            _ => bail!("{} is waiting on you; open it to answer.", quoted(&session.title)),
        }
    }
    Ok(())
}

/// The ways in an agent's message may take, of the session's own. A harness with a way in for
/// other sessions' messages (`Adapter::takes_messages`: one that tells the model a message is
/// another session's, and may hold it for the session's user) takes it only that way: typed
/// into its terminal, the message would read as its user's own words. The others take it typed
/// or by their own ways that take an agent's (`Own::takes_agents`), the byline saying whose it
/// is. A way that resumes a session or leaves the prompt for someone to send isn't for an agent.
pub(crate) fn agent_routes(session: &Session, routes: Vec<Route>) -> Result<Vec<Route>> {
    let adapter = crate::harness::adapter(session.harness);
    let left_to_send = routes.iter().any(|r| matches!(r, Route::Own(own) if own.leaves_it_to_paste()));
    let kept: Vec<Route> = routes
        .into_iter()
        .filter(|r| match r {
            Route::Own(own) => own.takes_agents(),
            Route::Type { .. } => !adapter.takes_messages(),
            Route::Resume => false,
        })
        .collect();
    if kept.is_empty() {
        let title = quoted(&session.title);
        let word = adapter.harness().word();
        bail!(if left_to_send {
            format!("{word} takes no prompts from outside; Codealong can only put one on the clipboard for someone at this Mac, so nothing was sent.")
        } else if !session.live {
            format!("{title} isn't running, and a message from another session doesn't start it again, so nothing was sent. Its user can resume it, or `codealong start` starts a new session.")
        } else if adapter.takes_messages() {
            format!("{title} doesn't take messages from other sessions (its {word} doesn't listen for them), and typing into its terminal would read as its user's words, so nothing was sent.")
        } else {
            format!("{title} runs where Codealong can't type, so nothing was sent.")
        });
    }
    Ok(kept)
}

/// One way in: it went, or nothing went and the next way is tried, or it failed. `untyped` is
/// why typing into the session's terminal didn't work, when that was tried first.
fn attempt(route: &Route, session: &Session, account: &AccountCfg, cfg: &Config, place: launch::Placement, msg: &Message, untyped: Option<&str>) -> Result<Tried> {
    let text: &str = &msg.text;
    let busy = session.status == Status::Working;
    let now = msg.now_for(session);
    let adapter = crate::harness::adapter(session.harness);
    let title = quoted(&session.title);
    match route {
        Route::Type { pid } => {
            let clear = adapter.clears_before_typing(account, session);
            // Sent now with its harness's keys where its terminal can press them; Return
            // otherwise, which sends it into the running turn already where the harness has no
            // keys of its own for that (Codex).
            let keys = adapter.sends_now_with().filter(|_| now);
            let presses = launch::terminal_kind(*pid).is_some_and(|k| k.presses_keys());
            let submit = match keys {
                Some(k) if presses => launch::Submit::Keys(k),
                _ => launch::Submit::Return,
            };
            match launch::terminal_type_session(*pid, Path::new(&session.cwd), &boxed(&attachments::expand(text, adapter.typed_attachments())), clear, submit) {
                Ok(launch::Typed::Into(app)) => Ok(Tried::Sent(sent(match (now, submit, keys) {
                    (true, launch::Submit::Keys(_), _) => format!("Typed into {title} in {app} and sent now: it reads it without waiting for its current step."),
                    (true, _, None) => format!("Typed into {title} in {app} and sent now: it takes it into the turn it's taking."),
                    (true, _, Some(_)) => format!("Typed into {title} in {app}, which can't press the keys that send it now, so it takes it after its current step."),
                    _ if busy => format!("Typed into {title} in {app}; it takes it after its current step."),
                    _ => format!("Typed into {title} in {app}."),
                }))),
                Ok(launch::Typed::Nowhere(why)) => Ok(Tried::Passed(Some(why))),
                Err(_) => Err(MayHaveArrived(format!("Codealong started typing this into {title}, and its terminal didn't confirm it went in: look there before sending it again.")).into()),
            }
        }
        Route::Resume => {
            let (input, env) = adapter.resume(account, session, text)?;
            launch::terminal_run(&cfg.apps.terminal, place, Path::new(&session.cwd), &input, &env)?;
            Ok(Tried::Sent(sent(format!("Resumed {title} in {} with this prompt.", launch::terminal_name(&cfg.apps.terminal)))))
        }
        Route::Own(own) => own.deliver(&msg.delivery(session, account, untyped)).map(Tried::Sent),
    }
}

/// A prompt as it's pasted into a session's input box, for the Return that follows to send it.
/// While the text ends in an `@` word, Claude Code and Codex both show a list to pick from
/// (files; Codex its plugins too), and Return picks from it instead of sending: the prompt
/// stays in the box unsent, its last word swapped for the pick. Claude Code's `@"<path>"` loses
/// its quotes that way, so a path with a space in it isn't attached once the prompt is sent by
/// hand. A space after the word closes the list (Claude Code leaves the space out of the
/// prompt), so a prompt that ends in one, as one with an image last does, gets a space.
pub(crate) fn boxed(text: &str) -> Cow<'_, str> {
    let t = text.trim_end();
    if ends_in_mention(t) {
        Cow::Owned(format!("{t} "))
    } else {
        Cow::Borrowed(text)
    }
}

/// Whether the text ends in an `@` word: `@"<path>"`, which may have spaces in it, or a word
/// that starts with `@`. An `@` inside a word (an email address) isn't one.
fn ends_in_mention(text: &str) -> bool {
    let starts_word = |i: usize| text[..i].chars().next_back().map_or(true, char::is_whitespace);
    if let Some(body) = text.strip_suffix('"') {
        if body.rfind("@\"").is_some_and(|i| starts_word(i) && !body[i + 2..].contains('"')) {
            return true;
        }
    }
    text.rsplit(char::is_whitespace).next().is_some_and(|word| word.starts_with('@'))
}

pub(crate) fn quoted(title: &str) -> String {
    format!("“{}”", title.trim())
}

/// Puts a prompt on the clipboard, for a way in that leaves it to someone to paste.
pub(crate) fn copy(text: &str) -> Result<()> {
    procs::run_stdin("/usr/bin/pbcopy", &[], text, Duration::from_secs(5)).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Harness, Session, Status, Surface};
    use serde_json::json;
    use std::time::Instant;

    #[test]
    fn a_pasted_prompt_never_ends_in_a_mention() {
        let image = "@\"/Users/me/Library/Application Support/Codealong/images/0123456789abcdef.png\"";
        // An image last, as the reply box writes one: Return would pick it from the list.
        assert_eq!(boxed(&format!("How do we fix these? \n{image}")), format!("How do we fix these? \n{image} "));
        assert_eq!(boxed(&format!("{image}\n")), format!("{image} "));
        assert_eq!(boxed("look at @src"), "look at @src ");
        assert_eq!(boxed("@"), "@ ");
        assert_eq!(boxed("see @\"a.png\""), "see @\"a.png\" ");
        // Text after it, or an @ that starts no word, opens no list: the prompt goes as it is.
        for sent in [format!("{image} Is there a bug there?"), format!("See {image}."), "mail gordon@example".to_string(), "he said \"no @ here\"".to_string(), "x@\"y z\"".to_string(), "plain\n".to_string(), String::new()] {
            assert_eq!(boxed(&sent), sent);
        }
    }

    fn session(harness: Harness, surface: Surface, status: Status, live: bool, pid: Option<u32>) -> Session {
        Session { harness, surface, status, live, pid, title: "Fix the build".into(), cwd: "/tmp".into(), ..Session::new(Harness::Claude, "claude", "abc") }
    }

    /// A stand-in for one of a harness's own ways in.
    #[derive(Debug)]
    struct Stand(Way);

    impl Own for Stand {
        fn way(&self) -> Way {
            self.0.clone()
        }

        fn takes_agents(&self) -> bool {
            false
        }

        fn deliver(&self, d: &Delivery) -> Result<Sent> {
            Ok(sent(format!("Stood in for {}.", d.title())))
        }
    }

    #[test]
    fn says_the_first_way_in_that_applies() {
        let in_ghostty = |pid: u32| (pid == 42).then_some("Ghostty");
        let first = |routes: &[Route]| routes.iter().find_map(|r| way_of(r, &in_ghostty, "iTerm2"));
        let message: Route = Stand(Way::Message).into();
        assert_eq!(first(&[Route::Type { pid: 42 }, message.clone()]), Some(Way::Typed { app: "Ghostty".into() }));
        // A session in no terminal Codealong can type into goes by its next way in.
        assert_eq!(first(&[Route::Type { pid: 7 }, message.clone()]), Some(Way::Message));
        assert_eq!(first(&[Route::Type { pid: 7 }]), None);
        assert_eq!(first(&[Stand(Way::Background).into(), message]), Some(Way::Background));
        assert_eq!(first(&[Route::Resume]), Some(Way::Resume { app: "iTerm2".into() }));
        assert_eq!(first(&[Stand(Way::CodexApp).into()]), Some(Way::CodexApp));
    }

    #[test]
    fn a_way_in_reads_as_the_box_takes_it() {
        assert_eq!(serde_json::to_value(Way::Typed { app: "Ghostty".into() }).unwrap(), json!({ "kind": "typed", "app": "Ghostty" }));
        assert_eq!(serde_json::to_value(Way::ClaudeApp).unwrap(), json!({ "kind": "claude-app" }));
        assert_eq!(serde_json::to_value(Way::CodexApp).unwrap(), json!({ "kind": "codex-app" }));
    }

    #[test]
    fn never_types_into_a_session_waiting_on_you() {
        let s = session(Harness::Claude, Surface::Cli, Status::Needs, true, Some(42));
        let err = ensure_not_waiting(&s, &Sender::You).unwrap_err();
        assert!(err.to_string().contains("waiting on you"), "{err}");
        // One whose reply asks you something takes the answer like any prompt.
        let asks = Session { asks: true, ..s };
        assert!(ensure_not_waiting(&asks, &Sender::You).is_ok());
    }

    /// One of a harness's own ways in is sent by the harness, with whose the message is and
    /// what its sender may claim: only you, here, are taken to skip permission prompts.
    #[test]
    fn a_way_in_of_a_harnesss_own_is_told_whose_the_message_is() {
        let account: AccountCfg = serde_json::from_value(json!({ "name": "claude", "harness": "claude", "home": "/h" })).unwrap();
        let cfg = Config::default();
        let place = launch::Placement { new_terminal: cfg.apps.new_terminal, near: &[] };
        let s = session(Harness::Claude, Surface::App, Status::Unread, true, Some(42));
        let board = Board::default();
        static AGENT: Sender = Sender::Agent(None);
        let told = |sender: &'static Sender| {
            let msg = Message::new(&board, &cfg, "hi", sender, false);
            let d = msg.delivery(&s, &account, Some("no tab"));
            (d.agent.map(String::from), d.bypasses, d.from_agent(), d.untyped.map(String::from))
        };
        assert_eq!(told(&Sender::You), (None, true, false, Some("no tab".into())));
        assert_eq!(told(&Sender::Elsewhere), (None, false, false, Some("no tab".into())));
        assert_eq!(told(&AGENT), (Some("codealong".into()), false, true, Some("no tab".into())));
        let msg = Message::new(&board, &cfg, "hi", &Sender::You, false);
        let Tried::Sent(went) = attempt(&Stand(Way::Message).into(), &s, &account, &cfg, place, &msg, None).unwrap() else { panic!("it passed") };
        assert_eq!(went.message, "Stood in for “Fix the build”.");
    }

    fn board_with(s: Session) -> Board {
        Board { lanes: vec![crate::fixtures::lane("/w", vec![s])], ..Board::default() }
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
        let msg = Message::new(&board, &Config::default(), "  Which test fails?\n", &agent, false);
        assert_eq!(msg.text, format!("{}\n\nWhich test fails?", byline(&board, Some("claude:claude2:me")).intro));
        assert_eq!(msg.byline.map(|b| b.name).as_deref(), Some("Review the parser"));
        // A note of it in the session's panel leaves the byline out, either kind.
        assert_eq!(without_byline(&msg.text), "Which test fails?");
        assert_eq!(without_byline(&format!("{}\n\nAnd the docs?", byline(&board, None).intro)), "And the docs?");
        assert_eq!(without_byline("Plain words\n\nstay whole"), "Plain words\n\nstay whole");
        assert_eq!(Message::new(&board, &Config::default(), "hi", &Sender::You, false).text, "hi");
    }

    #[test]
    fn an_agent_waits_for_a_session_waiting_on_its_user() {
        let s = session(Harness::Claude, Surface::Cli, Status::Needs, true, Some(42));
        let err = ensure_not_waiting(&s, &Sender::Agent(None)).unwrap_err();
        assert!(err.to_string().contains("waiting on its user"), "{err}");
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
}
