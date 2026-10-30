//! Sending a prompt to a session that already exists, the way its harness takes one in.
//!
//! Each harness says which ways reach a session, best first (`Adapter::routes`), and they're
//! tried in that order until one works. Two are the same for every harness: typing the prompt
//! into the session's terminal, where it arrives as if you'd typed it, done here, and resuming a
//! session that isn't running in a new terminal, on the prompt, which is `start::resume`'s (a
//! window's box asks that for the other places it can go on in). The others are a harness's own
//! (`Own`: its background service, a socket its sessions listen on, its app), and its module
//! sends by them.
//!
//! What you type in Codealong's windows here is yours, as if typed in the session. A message
//! from another device isn't known to be, and an agent's never is. Every way in is told whose
//! a message is and whether its sender skips permission prompts itself, so a harness that
//! holds another session's message for its user to allow can't be got past through Codealong
//! (claude/peer.rs).
//!
//! A session that's waiting on you for a permission prompt is never typed into: what's typed
//! would answer the prompt. One whose last reply asks you something is at its prompt, so what's
//! typed is its answer. Busy sessions are fine, stuck ones too; each harness queues the prompt
//! until the agent can take it.
//!
//! A prompt of yours that starts with `!` is a command for the session's own shell, as Claude
//! Code's box takes one (bash mode: it runs in the session, and the agent reads what it prints).
//! It goes only the ways that put it in that box as typed, its terminal or a background job's,
//! never one that would hand the agent the words, or resume the session on them
//! (`typed_routes`). That's for one sent from another device: the box in a window here, and Run on
//! a block in the conversation, run it in a tab of yours instead and tell the session what it
//! printed (actions.rs `run_command`).
//!
//! An agent's message (`codealong session send`) is never yours. It starts with who sent it, that
//! it isn't from the session's user, and how to answer, and it goes only the ways that don't read
//! as the user's own words or need someone at this Mac: a harness's own way in for other
//! sessions' messages where it has one (`Adapter::takes_messages`), which tells the model as
//! much, and for the harnesses that have none, their terminal or service. It never leaves a
//! prompt for someone to send, and doesn't resume a session that isn't running by the ways an
//! agent's message takes: Codealong resumes it out of sight with the message, by itself, as
//! `[sessions] resume_for_any_session` has it, or only for a message along a start with just
//! `[sessions] resume_for_sessions_it_started` on (`wakes`). The session is told it had stopped,
//! and the agent that it was resumed.
//!
//! What Codealong sends by itself, as you set it up to (a PR's news, a review handed back, a
//! helper's news, carrying on after a lost connection or at another account; `tell`), isn't
//! yours either: it starts with a line saying Codealong sent it and that it isn't its user's
//! words (`FROM_CODEALONG_ITSELF`), and goes first by the ways an agent's message takes, under
//! the name Codealong, so Claude Code shows it as a message from Codealong and not as a prompt;
//! typed into its terminal only where there's no such way, the line saying whose it is. It
//! claims the session's own permission class, as what you send does, since it's your setting
//! that sends it: Claude Code doesn't hold it for you. Conversations show it as Codealong's
//! (transcript.rs `shown_in`, claude/conversation.rs `peer_item`).

use std::borrow::{Borrow, Cow};
use std::path::Path;
use std::collections::BTreeMap;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use anyhow::{anyhow, bail, Result};

use crate::attachments::{self, Style};
use crate::config::{AccountCfg, Config};
use crate::harness::Switch;
use crate::launch;
use crate::model::{Board, Session, Status};
use crate::procs;
use crate::start::Place;

/// How a send went, and how one would go; they go back to other devices too, so they're in
/// codealong-peer.
pub use codealong_peer::api::{ModeSwitched, Sent, Way};

/// Who a message is from: how the session is told, and which ways in it may take.
#[derive(Debug, Clone, PartialEq)]
pub enum Sender {
    /// You, in one of Codealong's windows on this Mac, or on your phone, which runs no agents.
    You,
    /// Another of your Macs, or the engine's `--snapshot --send`, where an agent may have
    /// written it.
    Elsewhere,
    /// An agent, through `codealong session send`: the key of the session that ran the command,
    /// when the command found one.
    Agent(Option<String>),
    /// Codealong itself, by a setting of yours: a PR's news, a review handed back, a helper's
    /// news, carrying on after a lost connection (`tell`).
    Codealong,
}

/// The line what Codealong sends by itself starts with (`Sender::Codealong`), as the session
/// reads it and as its conversation is told apart from yours. It says that what the session has
/// to say goes in its reply, as yours does (claude/peer.rs `FROM_YOU`): no session sent it.
pub const FROM_CODEALONG_ITSELF: &str = "Codealong sent this by itself, as your user set it up to; it isn't something your user wrote. No session sent it either: anything you'd say about it goes in your reply here, where your user reads it.";

/// How that line was worded until 2026-10-14, which older records start with. The line now
/// starts with it, so it's looked for second.
const FROM_CODEALONG_ITSELF_BEFORE: &str = "Codealong sent this by itself, as your user set it up to; it isn't something your user wrote.";

/// Text Codealong sends by itself as a session gets it: after the line that says so. For what goes
/// in some other way than a send (a resume's or a start's prompt).
pub fn by_codealong(text: &str) -> String {
    format!("{FROM_CODEALONG_ITSELF}\n\n{}", text.trim())
}

/// The line a message starts with where it resumed the session it's for (`wake`), ahead of the
/// agent's byline: the session had stopped and goes on for another session's message, which isn't
/// its user's. A resume's prompt reads as typed, so the line says so where a message's way in
/// would (claude/peer.rs). Recognized ahead of a byline (`past_resumed`).
pub const RESUMED_FOR_A_MESSAGE: &str = "You had stopped, and Codealong resumed you for this message from another session, as your user set it up to; your user didn't write it.";

/// Text that `by_codealong` led, without its line: what Codealong said.
pub fn from_codealong(text: &str) -> Option<&str> {
    let t = text.trim_start();
    [FROM_CODEALONG_ITSELF, FROM_CODEALONG_ITSELF_BEFORE].iter().find_map(|first| t.strip_prefix(first)).map(str::trim)
}

/// Sends a session what Codealong has to tell it by itself (`Sender::Codealong`), the way its
/// harness takes a message from outside. Every one of Codealong's own messages goes this way.
pub fn tell(engine: &crate::engine::Handle, key: &str, text: &str) -> Result<Sent> {
    send_here(engine, key, text, Sender::Codealong, false)
}

/// The line past Codealong's own (`by_codealong`) that what it tells a session by itself starts
/// with where it resumed the session to tell it (`tell_waking`).
pub const RESUMED_TO_TELL: &str = "You had stopped, and Codealong resumed you to tell you this.";

/// Tells a session what Codealong has to tell it by itself, as `tell` does, whether or not it's
/// running: one that isn't, whose harness goes on by a resume (`Route::Resume`), is resumed out of
/// sight with it (`start::resume_out_of_sight`, `resume_once`), led by the line that says it had
/// stopped (`RESUMED_TO_TELL`), as `wake` resumes one for an agent's message; one whose own way
/// in takes it while it isn't running (the Codex app, OpenCode's service) is sent it that way. For
/// a PR's news to its primary session (nudges.rs), as `[sessions] resume_for_pr_news` has it. How
/// it went, with the key it goes on under (a copy's, where its harness copied it); None where it
/// can't go on without someone at this Mac (a Claude app session goes on in its app), you archived
/// it or turned its account off: nothing is sent, as nothing was before it could be resumed.
pub fn tell_waking(engine: &crate::engine::Handle, key: &str, text: &str) -> Result<Option<crate::start::Resumed>> {
    let cfg = crate::config::current()?;
    let board = engine.board();
    let (session, account) = crate::harness::find(&board, &cfg, key)?;
    ensure_not_flagged(&cfg, session, &Sender::Codealong)?;
    let routes = || crate::harness::adapter(session.harness).routes(account, session);
    match waking(session, account, routes, || crate::start::resume_out_of_sight(account, &crate::util::home())) {
        Waking::Send => send_from(|| engine.board(), &cfg, key, text, Sender::Codealong, false).map(|sent| Some(crate::start::Resumed { key: Some(key.to_string()), sent })),
        Waking::Nowhere => Ok(None),
        Waking::Resume(place) => {
            let prompt = by_codealong(&format!("{RESUMED_TO_TELL}\n\n{}", text.trim()));
            match resume_once(&board, &cfg, session, place, &prompt)? {
                Some(resumed) => Ok(Some(resumed)),
                None => bail!("{} is being resumed for something else just now, so it wasn't told this yet.", quoted(&session.title)),
            }
        }
    }
}

/// Whether `tell_waking` would reach a session that isn't running, for nudges.rs
/// `stopped_primaries`.
pub fn reaches_stopped(cfg: &Config, session: &Session) -> bool {
    if ensure_not_flagged(cfg, session, &Sender::Codealong).is_err() {
        return false;
    }
    let Some(account) = cfg.accounts.iter().find(|a| a.name == session.account) else { return false };
    let routes = || crate::harness::adapter(session.harness).routes(account, session);
    waking(session, account, routes, || crate::start::resume_out_of_sight(account, &crate::util::home())) != Waking::Nowhere
}

/// How `tell_waking` reaches a session.
#[derive(Debug, PartialEq)]
enum Waking {
    /// Sent it as `tell` sends: it runs, or a way in of its harness's takes it while it doesn't.
    Send,
    /// Resumed with it, out of sight here.
    Resume(Place),
    /// Not at all.
    Nowhere,
}

/// How `tell_waking` reaches `session`, given its ways in (`Adapter::routes`) and where its
/// account resumes one out of sight (`start::resume_out_of_sight`), each asked only when it's
/// needed.
fn waking(session: &Session, account: &AccountCfg, routes: impl FnOnce() -> Vec<Route>, out_of_sight: impl FnOnce() -> Option<Place>) -> Waking {
    if session.live {
        return Waking::Send;
    }
    if session.archived || account.ensure_on().is_err() {
        return Waking::Nowhere;
    }
    // Codealong ran it, and goes on with it when it's sent something (acp.rs `send`).
    if session.hosted {
        return Waking::Send;
    }
    let routes = routes();
    if routes.iter().any(|r| matches!(r, Route::Resume)) {
        return out_of_sight().map_or(Waking::Nowhere, Waking::Resume);
    }
    match codealong_routes(session, routes) {
        Ok(_) => Waking::Send,
        Err(_) => Waking::Nowhere,
    }
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

/// How a Mac before 2026-10-14 began its note of another session's message, which another
/// device may still send (changes.rs `texts`, and the windows' ahead.ts `inRecord`), and which a
/// device from before still gets (peerfeed.rs `peer_items_as_notes`); a Mac since sends the
/// message whole, as a `peer` item.
pub const FROM_ANOTHER_SESSION: &str = "From another session";

/// How an agent's message begins, for each way `byline` words it.
const AGENT_BYLINES: [&str; 2] = ["This message is from another agent, ", "This message was sent with `codealong session send`"];

/// An agent's message without the byline it starts with, as the conversation shows it
/// (claude/conversation.rs `peer_item`, transcript.rs `shown`): the byline says whose it is,
/// which the conversation says already.
pub fn without_byline(text: &str) -> &str {
    let t = text.trim();
    match t.split_once("\n\n") {
        Some((intro, rest)) if AGENT_BYLINES.iter().chain([&FROM_CODEALONG_ITSELF, &FROM_CODEALONG_ITSELF_BEFORE]).any(|s| intro.starts_with(s)) => rest.trim(),
        _ => t,
    }
}

/// Text an agent sent with `codealong session send`, as a session that takes it typed in records
/// it, or that was resumed on it, without its byline: what the agent said. None for anything else.
pub fn from_an_agent(text: &str) -> Option<&str> {
    let t = past_resumed(text);
    AGENT_BYLINES.iter().any(|s| t.starts_with(s)).then(|| without_byline(t))
}

/// What a message asks, past the lines Codealong leads it with where Codealong sent it by itself
/// (`by_codealong`, `RESUMED_TO_TELL`) or another session sent it (`byline`,
/// `RESUMED_FOR_A_MESSAGE`); other text as it is. For a harness that records such messages as
/// prompts (Codex's are user messages): a session's title, excerpt and latest prompt say what was
/// asked, where those lines would say only who asked it.
pub fn past_sender_lines(text: &str) -> &str {
    match from_codealong(text) {
        Some(said) => said.strip_prefix(RESUMED_TO_TELL).map_or(said, str::trim_start),
        None => from_an_agent(text).unwrap_or(text),
    }
}

/// Text past the line that says a message resumed its session (`RESUMED_FOR_A_MESSAGE`), or the
/// text as it is.
fn past_resumed(text: &str) -> &str {
    let t = text.trim_start();
    t.strip_prefix(RESUMED_FOR_A_MESSAGE).map_or(t, str::trim_start)
}

/// The session an agent's message is from, by the title its byline quotes; None where the byline
/// names no session the board knew.
pub fn sender_of(text: &str) -> Option<&str> {
    let who = past_resumed(text).strip_prefix(AGENT_BYLINES[0])?.strip_prefix('“')?;
    who.split_once('”').map(|(title, _)| title.trim()).filter(|t| !t.is_empty())
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
            // Your setting sends it, so it claims the session's class as what you send does.
            Sender::Codealong => {
                let b = Byline { intro: FROM_CODEALONG_ITSELF.into(), name: "Codealong".into() };
                Message { text: Cow::Owned(by_codealong(text)), sender, now, byline: Some(b), bypasses: true }
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

/// Whether an agent's message comes from a helper the session it goes to started
/// (`codealong start --helper`), by a start that said which session it made
/// (`origins::Store::known_helper_of`), or from the automatic review of a PR that session opened,
/// which is handed back to it (autoreview.rs `reviews_for`). What such a helper says is the work
/// its session asked of it, which the session reads with `codealong session wait` or is sent
/// anyway, so its message claims the session's own class, as yours does, rather than being held
/// for you. Any other agent's message still claims only what its own session has: a helper found
/// only by matching its start to the next session in its worktree (`origins::resolve`), which
/// may be another one started about then. And it must still be that session's helper on the
/// board: one you made your own (`Session::helper` gone, state.rs `own`) is a session like any
/// other.
fn from_its_helper(board: &Board, cfg: &Config, origins: &crate::origins::Store, sender: &Sender, to: &str) -> bool {
    let Sender::Agent(Some(from)) = sender else { return false };
    let helps = board.session(from).is_some_and(|(_, s)| s.helper.as_ref().and_then(|h| h.parent.as_deref()) == Some(to));
    (helps && origins.known_helper_of(from) == Some(to)) || crate::autoreview::reviews_for(board, &cfg.reviews, from, to)
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

    /// What goes this way lands in the session's own box as if typed there (a background job's
    /// terminal), so a prompt that starts with `!` runs in the session's shell (`shell_command`).
    /// Not a way that hands the agent the words, or opens an app with them.
    fn as_typed(&self) -> bool {
        false
    }

    /// Nothing takes the prompt this way: the harness takes none from outside, and it's only
    /// left on the clipboard for someone at this Mac to paste. An agent is told as much.
    fn leaves_it_to_paste(&self) -> bool {
        false
    }

    /// Sends the message this way, and says how it went.
    fn deliver(&self, d: &Delivery) -> Result<Sent>;

    /// The app this way goes through sets the model and effort level the session's next turn
    /// runs on, running or not (`switch`).
    fn switches(&self) -> bool {
        false
    }

    /// Switches the session to a model or an effort level this way, for it alone, and says the
    /// one it's on now, which may be the nearest the model takes. Only where `switches`.
    fn switch(&self, session: &Session, _account: &AccountCfg, to: Switch) -> Result<String> {
        bail!("{} can't be switched to another {} this way.", quoted(&session.title), to.what())
    }
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
        mid_turn(self.session)
    }
}

/// Whether a session is in the middle of a turn, stuck in one too, so a prompt goes in once the
/// step it's on ends.
fn mid_turn(session: &Session) -> bool {
    session.status == Status::Working || crate::harness::turn_running(session)
}

/// The prompt went, where it lives.
pub fn sent(message: String) -> Sent {
    Sent { message, opened: false, fell_back: false, uncertain: false, queued: None }
}

/// The prompt is in a window that came forward, for someone to send from there.
pub fn opened(message: String) -> Sent {
    Sent { message, opened: true, fell_back: false, uncertain: false, queued: None }
}

/// Sends `text` to the session with this key; `now` sends it into the turn the session is
/// taking at once, where its way in can (`harness::steerable`), and otherwise as any send goes.
/// The same text sent to the same session while it's still on its way (the peek closed and
/// opened again, and you sent what it still showed, or another window sent it too) joins that
/// send and gets its outcome, rather than going twice.
pub fn send(board: &Board, cfg: &Config, session_key: &str, text: &str, sender: Sender, now: bool) -> Result<Sent> {
    send_from(|| board, cfg, session_key, text, sender, now)
}

/// `send` on the board `board` reads, which is read again once the session's input box is held
/// (`at_the_box`): a send that waited there behind a Remove goes by the session as it is then,
/// which may wait on you by now, not as it was when it was asked.
fn send_from<B: Borrow<Board>>(board: impl Fn() -> B, cfg: &Config, session_key: &str, text: &str, sender: Sender, now: bool) -> Result<Sent> {
    let b = board();
    if let Some((_, s)) = b.borrow().session(session_key) {
        ensure_not_flagged(cfg, s, &sender)?;
    }
    send_allowed(board, cfg, session_key, text, sender, now)
}

/// After the automation check, or a deliberate Send review click. The click changes only
/// whether a flagged session may receive it, never its author or its delivery routes.
fn send_allowed<B: Borrow<Board>>(board: impl Fn() -> B, cfg: &Config, key: &str, text: &str, sender: Sender, now: bool) -> Result<Sent> {
    ensure_not_moved(&crate::origins::current(), board().borrow(), key)?;
    join(key, text.trim(), || at_the_box(key, || deliver(board().borrow(), cfg, key, text, &sender, now)))
}

/// Each session's input box, held by whatever types into it or presses keys there for a while:
/// a send, Send now on what it holds, and Remove (`Adapter::remove_queued`), whose ↑ and
/// clearing would take a send's text with them, or a send take what ↑ gave back.
static BOXES: Mutex<BTreeMap<String, Arc<Mutex<()>>>> = Mutex::new(BTreeMap::new());

/// Runs `f` holding the input box of the session `key`, after anything else that holds it.
pub(crate) fn at_the_box<T>(key: &str, f: impl FnOnce() -> T) -> T {
    let held = {
        let mut boxes = BOXES.lock().unwrap_or_else(|e| e.into_inner());
        // The ones nothing holds or waits on go, so the map stays as small as what's under way.
        boxes.retain(|_, b| Arc::strong_count(b) > 1);
        boxes.entry(key.to_string()).or_default().clone()
    };
    let _alone = held.lock().unwrap_or_else(|e| e.into_inner());
    f()
}

/// A review you explicitly asked to send is still Codealong's message, with its byline and
/// routes. It can reach a flagged session, but can't open an app with text left to paste.
pub fn tell_by_hand(engine: &crate::engine::Handle, key: &str, text: &str) -> Result<Sent> {
    let cfg = crate::config::current()?;
    send_allowed(|| engine.board(), &cfg, key, text, Sender::Codealong, false)
}

/// A flag stops messages from Codealong and other agents before any delivery path, including
/// hosted sessions and automatic resumes. A person can still reply or resume deliberately.
pub fn ensure_not_flagged(cfg: &Config, s: &Session, sender: &Sender) -> Result<()> {
    if cfg.sessions.blocks(s) && matches!(sender, Sender::Codealong | Sender::Agent(_)) {
        bail!("“{}” was flagged by its provider. Automatic messages and resumes are blocked in Settings › Automation; nothing was sent.", s.title);
    }
    Ok(())
}

/// A session that moved to another account (start.rs `move_to`) is neither sent to nor resumed
/// (start.rs `resume`): either would bring it back to life where it was, beside the copy that
/// goes on. Says where it went, for the sender to go there instead. `origins` is where sessions
/// came from (`origins::current`), and a copy counts while the board has it.
pub fn ensure_not_moved(origins: &crate::origins::Store, board: &Board, key: &str) -> Result<()> {
    if let Some(copy) = origins.moved_to(key, |k| board.session(k).is_some()) {
        bail!("{}", moved_words(board, key, &copy));
    }
    Ok(())
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
    send_from(|| engine.board(), &cfg, key, text, sender, now)
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
/// didn't confirm: no other way is tried, or the session could get it twice, and the send counts
/// as sent, unsure (`sent_unsure`).
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
    let origins = crate::origins::current();
    let mut msg = Message::new(board, cfg, text, sender, now);
    if from_its_helper(board, cfg, &origins, sender, session_key) {
        msg.bypasses = true;
    }
    if session.hosted {
        // Codealong runs it: the prompt goes in over its adapter, after the running turn if
        // there is one, and a question it waits on is answered from its panel.
        let message = crate::acp::send(account, session_key, &attachments::expand(&msg.text, Style::Path))?;
        return Ok(sent(message));
    }
    // One that isn't running goes on when it's sent something, whichever way it goes (a resume,
    // OpenCode's service, the Codex app), which nothing does on an account turned off; one at
    // work still takes it.
    if !session.live && session.status != Status::Working {
        account.ensure_on()?;
    }
    ensure_not_waiting(session, sender)?;
    // Each harness says which ways reach the session, best first (`harness::Adapter::routes`).
    let adapter = crate::harness::adapter(session.harness);
    let mut routes = adapter.routes(account, session);
    if msg.from_agent() {
        let ctx = crate::harness::Ctx { account };
        let holds = || adapter.would_hold_messages(&ctx, session);
        let out_of_sight = || crate::start::resume_out_of_sight(account, &crate::util::home());
        if let Some(woken) = wakes(cfg, &origins, &msg, session, &routes, holds, out_of_sight)? {
            return wake(board, cfg, session, woken, &msg);
        }
        routes = agent_routes(session, routes)?;
    } else if *sender == Sender::Codealong {
        routes = codealong_routes(session, routes)?;
    } else if shell_command(&msg.text) {
        routes = typed_routes(routes);
        if routes.is_empty() {
            let title = quoted(&session.title);
            bail!(if session.live {
                format!("{title} runs a command that starts with ! only when it's typed into its own box, and Codealong can't type there, so nothing was sent. Open it and run the command there.")
            } else {
                format!("{title} isn't running, so there's no shell for the command to run in; nothing was sent.")
            });
        }
    }
    through(&routes, &session.title, |route, untyped, passes| attempt(route, board, session, account, cfg, &msg, untyped, passes))
}

/// Tries each way in, best first, until one sends it (`attempt`, given why typing didn't work
/// and whether a way comes after). One that may have put it in the session ends the send
/// (`sent_unsure`).
fn through(routes: &[Route], title: &str, mut attempt: impl FnMut(&Route, Option<&str>, bool) -> Result<Tried>) -> Result<Sent> {
    let mut last_err = None;
    // Why typing into its terminal didn't work, for the note on the way it went instead.
    let mut untyped = None;
    let last = routes.len().saturating_sub(1);
    for (i, route) in routes.iter().enumerate() {
        match attempt(route, untyped.as_deref(), i < last) {
            Ok(Tried::Sent(sent)) => return Ok(sent),
            // This way doesn't reach the session; the next one may.
            Ok(Tried::Passed(why)) => untyped = why.or(untyped),
            Err(e) => match sent_unsure(e) {
                Ok(sent) => return Ok(sent),
                Err(e) => last_err = Some(e),
            },
        }
    }
    Err(last_err.unwrap_or_else(|| anyhow!("Codealong can't reach {} to send it this. Open it and send it there.", quoted(title))))
}

/// A send that failed after it may have put the prompt in the session (`MayHaveArrived`) as sent,
/// unsure, with what it said; any other failure as it was. Told it failed, a sender puts it back
/// to send again (the windows' box, Codealong's own messages trying again later), and the session
/// had it twice (2026-10-17). Its record says soon enough whether it went in.
pub(crate) fn sent_unsure(e: anyhow::Error) -> Result<Sent> {
    e.downcast::<MayHaveArrived>().map(|m| Sent { uncertain: true, ..sent(m.0) })
}

/// Whether it waits on you to answer a prompt (a permission, a question through its harness),
/// not only asking you something in its reply, nor stuck (`Session::stalled_since`). A stuck
/// session is at no prompt: its turn has gone quiet on a command, and Claude Code queues what's
/// typed then as it does while it works, for once the step it's on ends; or its turn ended on a
/// command left running in the background (`Session::background`, which Jev reads as stuck when
/// it waits on a sign-in), which runs detached from the terminal, so the session sits at its
/// prompt (board.rs `claude_busy_since`). Either takes a message in its own terminal, so it
/// takes one from Codealong too (asked 2026-10-18).
pub(crate) fn waits_on_you(session: &Session) -> bool {
    session.status == Status::Needs && !session.asks && session.stalled_since.is_none()
}

/// A session waiting on you is never typed into: what's typed would answer its prompt. One that
/// only asks you something in its reply (`Session::asks`), or is stuck, takes what you send like
/// any prompt.
pub(crate) fn ensure_not_waiting(session: &Session, sender: &Sender) -> Result<()> {
    if waits_on_you(session) {
        match sender {
            Sender::Agent(_) => bail!("{} is waiting on its user to answer a prompt, so nothing was sent. Try again once they have.", quoted(&session.title)),
            _ => bail!("{} is waiting on you; open it to answer.", quoted(&session.title)),
        }
    }
    Ok(())
}

/// Why an agent never starts a session that isn't running again, after the session's name:
/// resumed on its words, the session would take them as its user's own prompt, and spend their
/// usage, with nobody at this Mac having asked. A send never resumes one for an agent
/// (`agent_routes`), nor does a resume (`ensure_may_resume`), and the command can't ask for one
/// (actions.rs). The one way round it is Codealong's own, by a setting of yours (`wakes`), with a
/// first line that says the session was resumed for another session's message.
pub(crate) const NOT_STARTED_AGAIN: &str = "isn't running, and a message from another session doesn't start it again, so nothing was sent. Its user can resume it, or `codealong start` starts a new session.";

/// Whether `sender` may start the session `title` names again where it isn't running (start.rs
/// `resume`): you may, from a window here or another of your devices; an agent never does.
pub(crate) fn ensure_may_resume(sender: &Sender, title: &str) -> Result<()> {
    if matches!(sender, Sender::Agent(_)) {
        bail!("{} {NOT_STARTED_AGAIN}", quoted(title));
    }
    Ok(())
}

/// How a start links a session that isn't running to the agent's that sends it a message, which
/// Codealong resumes it for (`wakes`).
#[derive(Debug, Clone, Copy, PartialEq)]
enum Link {
    /// It started the agent's session, which reports what it was asked for.
    StartedYours,
    /// The agent's session started it, and hands it more of the work it was started for.
    YoursStarted,
    /// No start links them.
    Neither,
}

/// Where an agent's message resumes the session it's for, which isn't running, and how a start
/// links them: a message from any session, with `[sessions] resume_for_any_session` on, else only
/// one along a start that said which session it made (`origins::Store::known_starter_of`), with
/// `[sessions] resume_for_sessions_it_started` on. Agents hand each other work and report on it
/// (`codealong session send`), and Claude Code's background service ends a job left idle at its
/// prompt for an hour, so the session a message is for has often stopped: a session waiting to
/// hear back would never hear, and one handed more of its work could only be replaced by a new one
/// (`codealong start`) that knows none of what it did. Codealong resumes it by itself, by your
/// setting (`wake`), out of sight (`start::resume_out_of_sight`, `out_of_sight`): the session is
/// told it had stopped and whose message it is, and the agent that it was resumed. Never one you
/// archived or marked done, one whose harness doesn't go on by a resume (a Claude app session goes
/// on in its app), nor one that skips permission prompts where the sender doesn't, or may as far as
/// Codealong can tell (`holds`, `Adapter::would_hold_messages`): running, it would hold the
/// message for you (claude/peer.rs), and as a resume's prompt it would act on it unasked. None for
/// a message from no session, which goes no further than an agent's does (`agent_routes`); an
/// error says why one from a session isn't resumed.
fn wakes(cfg: &Config, origins: &crate::origins::Store, msg: &Message, session: &Session, routes: &[Route], holds: impl FnOnce() -> bool, place: impl FnOnce() -> Option<Place>) -> Result<Option<(Place, Link)>> {
    let Sender::Agent(Some(from)) = msg.sender else { return Ok(None) };
    let resumes = routes.iter().any(|r| matches!(r, Route::Resume));
    if session.live || !resumes {
        return Ok(None);
    }
    let link = if origins.known_starter_of(from) == Some(session.key.as_str()) {
        Link::StartedYours
    } else if origins.known_starter_of(&session.key) == Some(from.as_str()) {
        Link::YoursStarted
    } else {
        Link::Neither
    };
    let title = quoted(&session.title);
    let instead = "Its user can resume it, or `codealong start` starts a new session.";
    let sessions = &cfg.sessions;
    if !sessions.resume_for_any_session {
        let whom = match link {
            Link::Neither => bail!("{title} isn't running, and its user turned off resuming a session for another session's message (`[sessions] resume_for_any_session`), so nothing was sent. {instead}"),
            _ if sessions.resume_for_sessions_it_started => None,
            Link::StartedYours => Some("one it started"),
            Link::YoursStarted => Some("the one that started it"),
        };
        if let Some(whom) = whom {
            bail!("{title} isn't running, and its user turned off resuming a session for {whom}, or for any session (`[sessions] resume_for_sessions_it_started` and `resume_for_any_session`), so nothing was sent. {instead}");
        }
    }
    if session.archived {
        bail!("{title} isn't running, and its user archived it or marked it done, so Codealong doesn't resume it and nothing was sent. {instead}");
    }
    if !msg.bypasses && holds() {
        bail!("{title} isn't running, and it skips permission prompts (or may: Codealong can't tell) where your session doesn't, so it would hold your message for its user: Codealong doesn't resume it on it, and nothing was sent. {instead}");
    }
    match place() {
        Some(place) => Ok(Some((place, link))),
        None => bail!("{title} isn't running, and its account can't resume it out of sight, so nothing was sent. {instead}"),
    }
}

/// The sessions `wake` resumed lately, each with when (epoch ms): `woken.json` next to the
/// config, which the app and the `codealong` command share, since with the app closed each
/// command sends by itself. The board goes on saying a session isn't running for a few seconds
/// after it's resumed (until the next scan finds it), and a second message then would resume it a
/// second time, beside the first.
fn woken_path() -> std::path::PathBuf {
    crate::config::support_dir().join("woken.json")
}

/// What `woken.json` holds, for the error when it doesn't read as that.
const WOKEN: &str = "a list of sessions resumed for a message";

/// How long after a session was resumed for a message another isn't resumed for: the board has
/// it running well before, or the resume came to nothing and another may try.
const WAKING_MS: i64 = 120_000;

/// Claims a resume of the session `key` for a message `now` in `woken`, which this prunes: not
/// while it was resumed for one less than `WAKING_MS` ago. Says whether it claimed it.
fn claim_wake(woken: &mut BTreeMap<String, i64>, key: &str, now: i64) -> bool {
    woken.retain(|_, at| now - *at < WAKING_MS);
    if woken.contains_key(key) {
        return false;
    }
    woken.insert(key.to_string(), now);
    true
}

/// Resumes the session `place` on `prompt` as Codealong does by itself, on the account's own
/// model, as a send from its box resumes it unless you pick another. It's claimed first
/// (`claim_wake`), so a message or news sent while another resumes it, or just after, comes again
/// once it runs rather than resuming it twice: None when it was claimed already. The claim is only
/// the file's lock long, so nothing waits on another session's resume.
fn resume_once(board: &Board, cfg: &Config, session: &Session, place: Place, prompt: &str) -> Result<Option<crate::start::Resumed>> {
    let (key, now) = (session.key.as_str(), crate::util::now_ms());
    let mut claimed = false;
    crate::store::change(&woken_path(), WOKEN, |woken: &mut BTreeMap<String, i64>| {
        let before = woken.clone();
        claimed = claim_wake(woken, key, now);
        *woken != before
    })?;
    if !claimed {
        return Ok(None);
    }
    let how = crate::start::ResumeIn { place, model: None, effort: None };
    let resumed = crate::start::resume(board, cfg, key, &how, prompt, &Sender::Codealong).inspect_err(|_| {
        // It didn't go on, so the next message may try again. A file that can't be changed now
        // only keeps that one waiting out `WAKING_MS`.
        let _ = crate::store::change(&woken_path(), WOKEN, |woken: &mut BTreeMap<String, i64>| woken.get(key) == Some(&now) && woken.remove(key).is_some());
    })?;
    Ok(Some(resumed))
}

/// Resumes the session `place` on an agent's message, as `wakes` allows (`resume_once`): as
/// Codealong resumes one by itself, the message led by the line that says it had stopped and was
/// resumed for another session's message, not its user's (`RESUMED_FOR_A_MESSAGE`), then by the
/// agent's byline, which says whose it is and how to answer; the agent is told it wasn't running.
/// What `wakes` found still holds once it's claimed. Where its harness goes on with a copy, a copy
/// resumed for the session that started it carries on that start (`origins::carry_start`), so
/// that session can resume the copy in turn.
fn wake(board: &Board, cfg: &Config, session: &Session, (place, link): (Place, Link), msg: &Message) -> Result<Sent> {
    let key = session.key.as_str();
    let prompt = format!("{RESUMED_FOR_A_MESSAGE}\n\n{}", msg.text.trim());
    let Some(resumed) = resume_once(board, cfg, session, place, &prompt)? else {
        bail!("{} is being resumed for another message sent to it just now, so yours wasn't sent: send it again in a minute, once it's running.", quoted(&session.title));
    };
    let copied = resumed.key.as_deref().filter(|k| *k != session.key);
    if let (Some(copy), Link::YoursStarted) = (copied, link) {
        crate::origins::carry_start(copy, key);
    }
    let copy = copied.map(|k| format!(" It goes on as {k}: send there from now on.")).unwrap_or_default();
    let why = match link {
        Link::StartedYours => "; it started your session",
        Link::YoursStarted => "; your session started it",
        Link::Neither => "",
    };
    Ok(sent(format!("{} wasn't running{why}, so Codealong resumed it {} with your message and told it it had stopped.{copy}", quoted(&session.title), crate::start::place_words(place, cfg))))
}

/// Whether a prompt is a command for the session's own shell: it starts with `!`, which Claude
/// Code's box takes as one (bash mode), running it in the session and handing the agent what it
/// prints. A prompt that opens with an image isn't one: the `!` is the image's, which Claude
/// Code's box gets as a mention (`Adapter::typed_attachments`), so it goes any way a prompt does.
pub fn shell_command(text: &str) -> bool {
    let text = text.trim_start();
    text.starts_with('!') && !attachments::leads_with_link(text)
}

/// The ways in a command for the session's shell may take (`shell_command`): those that put it
/// in the session's box as typed, where the `!` runs it. Any other would hand the agent the words
/// to act on as it saw fit, or resume the session on them.
pub(crate) fn typed_routes(routes: Vec<Route>) -> Vec<Route> {
    routes
        .into_iter()
        .filter(|r| match r {
            Route::Type { .. } => true,
            Route::Resume => false,
            Route::Own(own) => own.as_typed(),
        })
        .collect()
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
            format!("{title} {NOT_STARTED_AGAIN}")
        } else if adapter.takes_messages() {
            format!("{title} doesn't take messages from other sessions (its {word} doesn't listen for them), and typing into its terminal would read as its user's words, so nothing was sent.")
        } else {
            format!("{title} runs where Codealong can't type, so nothing was sent.")
        });
    }
    Ok(kept)
}

/// The ways in what Codealong sends by itself takes: first those an agent's message takes (a
/// harness's own way for other sessions' messages, which shows it as Codealong's), then the ways
/// that put it in as typed (its terminal, a background job's box), its first line saying whose it
/// is. Never one that opens an app with it to send, or leaves it on the clipboard, which would come
/// up in front of you unasked, nor a resume.
pub(crate) fn codealong_routes(session: &Session, routes: Vec<Route>) -> Result<Vec<Route>> {
    let (mut first, mut then) = (Vec::new(), Vec::new());
    for r in routes {
        match &r {
            Route::Own(own) if own.takes_agents() => first.push(r),
            Route::Own(own) if own.as_typed() => then.push(r),
            Route::Type { .. } => then.push(r),
            Route::Own(_) | Route::Resume => {}
        }
    }
    first.append(&mut then);
    if first.is_empty() {
        bail!("Codealong can't reach {}: it isn't running anywhere Codealong can send to.", quoted(&session.title));
    }
    Ok(first)
}

/// One way in: it went, or nothing went and the next way is tried, or it failed. `untyped` is
/// why typing into the session's terminal didn't work, when that was tried first; `passes`,
/// whether there's a way after this one, which typing gives up for sooner.
#[allow(clippy::too_many_arguments)]
fn attempt(route: &Route, board: &Board, session: &Session, account: &AccountCfg, cfg: &Config, msg: &Message, untyped: Option<&str>, passes: bool) -> Result<Tried> {
    let text: &str = &msg.text;
    let busy = mid_turn(session);
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
            let expanded = attachments::expand(text, adapter.typed_attachments());
            let typed = boxed(&expanded);
            let since = crate::util::now_ms();
            match launch::terminal_type_session(*pid, Path::new(&session.cwd), &typed, clear, submit, passes) {
                Ok(launch::Typed::Into(app)) => {
                    let said = sent(match (now, submit, keys) {
                        (true, launch::Submit::Keys(_), _) => format!("Typed into {title} in {app} and sent now: it reads it without waiting for its current step."),
                        (true, _, None) => format!("Typed into {title} in {app} and sent now: it takes it into the turn it's taking."),
                        (true, _, Some(_)) => format!("Typed into {title} in {app}, which can't press the keys that send it now, so it takes it after its current step."),
                        _ if busy => format!("Typed into {title} in {app}; it takes it after its current step."),
                        _ => format!("Typed into {title} in {app}."),
                    });
                    // Held for the turn, where its harness says what it holds.
                    let queued = (busy && submit == launch::Submit::Return).then(|| adapter.queued_as(account, session, &typed, since)).flatten();
                    Ok(Tried::Sent(Sent { queued, ..said }))
                }
                Ok(launch::Typed::Nowhere(why)) => Ok(Tried::Passed(Some(why))),
                Err(_) => Err(MayHaveArrived(format!("Codealong started typing this into {title}, and its terminal didn't confirm it went in: look there before sending it again.")).into()),
            }
        }
        Route::Resume => Ok(Tried::Sent(crate::start::resume(board, cfg, &session.key, &crate::start::ResumeIn::default(), text, msg.sender)?.sent)),
        Route::Own(own) => own.deliver(&msg.delivery(session, account, untyped)).map(Tried::Sent),
    }
}

/// Switches a running session to `model`, one of its account's (`Adapter::models`), for that
/// session alone (`switch`).
pub fn switch_model(board: &Board, cfg: &Config, key: &str, model: &str) -> Result<String> {
    switch(board, cfg, key, Switch::Model(model.trim()))
}

/// Switches a running session to `effort`, one of its account's levels (`Adapter::efforts`), for
/// that session alone (`switch`).
pub fn switch_effort(board: &Board, cfg: &Config, key: &str, effort: &str) -> Result<String> {
    switch(board, cfg, key, Switch::Effort(effort.trim()))
}

/// Switches a session to `mode`, one of the modes it offers (`Session::modes`, by id), for that
/// session alone. One Codealong runs through its agent (acp.rs `switch_mode`), mid-turn too and
/// while it waits on you, as Shift+Tab does in Claude Code's terminal: what it asks next goes by
/// the new mode, and what it asks now still waits for your answer. One in a terminal by Shift+Tab
/// pressed there (cycle.rs). Any other has its mode changed where it runs.
pub fn switch_mode(board: &Board, cfg: &Config, key: &str, mode: &str) -> Result<ModeSwitched> {
    let (session, account) = crate::harness::find(board, cfg, key)?;
    let title = quoted(&session.title);
    if session.modes.is_empty() {
        bail!("Codealong can't switch {title}'s mode: change it where it runs.");
    }
    let Some(to) = session.modes.iter().find(|m| m.id == mode.trim()) else {
        bail!("{:?} isn't a mode {title}'s agent offers.", mode.trim());
    };
    if !session.hosted {
        return crate::cycle::switch(account, session, to);
    }
    let now = crate::acp::switch_mode(account, key, &to.id)?;
    let named = |id: &str| session.modes.iter().find(|m| m.id == id).map_or_else(|| id.to_string(), |m| m.label.clone());
    let message = if now == to.id { format!("{title} is in {} mode now.", to.label) } else { format!("{title} is in {} mode: its agent didn't take {}.", named(&now), to.label) };
    Ok(ModeSwitched { message, mode: now })
}

/// Switches a session's model or effort level, for that session alone: one Codealong runs
/// through its agent (acp.rs `switch`), one that goes on in its harness's app through the app,
/// for its next turn (`Own::switch`), any other by its harness's command typed into its terminal
/// (`Adapter::model_command`, `effort_command`), the way a prompt is typed, and seen through as
/// its harness needs (`Adapter::type_switch`). Only typed: another way in would take the command
/// as a prompt. Not mid-turn, where it would wait as a prompt does, nor while the session waits
/// on you, where it would answer; the app takes it whenever, running or not.
fn switch(board: &Board, cfg: &Config, key: &str, to: Switch) -> Result<String> {
    let (session, account) = crate::harness::find(board, cfg, key)?;
    let adapter = crate::harness::adapter(session.harness);
    let title = quoted(&session.title);
    let (what, value) = (to.what(), to.value());
    let (taken, command) = match to {
        Switch::Model(model) => (adapter.takes_model() && crate::start::valid_model(model), adapter.model_command()),
        Switch::Effort(effort) => (adapter.efforts().iter().any(|(id, _)| *id == effort), adapter.effort_command()),
    };
    if !taken {
        bail!("{value:?} isn't {} Codealong can switch {title} to.", if what == "model" { "a model" } else { "an effort level" });
    }
    let label = |now: &str| match to {
        Switch::Model(_) => crate::start::model_label(adapter, account, now),
        Switch::Effort(_) => format!("{now} effort"),
    };
    let in_app = adapter.routes(account, session).into_iter().find_map(|r| match r {
        Route::Own(own) if own.switches() => Some(own),
        _ => None,
    });
    if let (false, Some(own)) = (session.hosted, in_app) {
        let now = own.switch(session, account, to)?;
        return Ok(format!("{title} is on {} from its next turn.", label(&now)));
    }
    if !session.live {
        bail!("{title} isn't running: pick its {what} as it resumes.");
    }
    if crate::harness::turn_running(session) {
        bail!("{title} is in the middle of a turn: switch its {what} once the turn ends.");
    }
    ensure_not_waiting(session, &Sender::You)?;
    if session.hosted {
        crate::acp::switch(account, key, to)?;
        return Ok(format!("{title} is on {} now.", label(value)));
    }
    let Some(command) = command else {
        bail!("{} has no command to switch a running session's {what}.", adapter.harness().word());
    };
    let text = format!("{command} {value}");
    let mut why = None;
    let mut typ = |text: &str| -> Result<bool> {
        let msg = Message::new(board, cfg, text, &Sender::You, false);
        for route in adapter.routes(account, session).iter().filter(|r| matches!(r, Route::Type { .. })) {
            match attempt(route, board, session, account, cfg, &msg, None, false)? {
                Tried::Sent(_) => return Ok(true),
                Tried::Passed(passed) => why = passed.or(why.take()),
            }
        }
        Ok(false)
    };
    if let Some(now) = adapter.type_switch(account, session, to, &text, &mut typ)? {
        let capped = if now == value { String::new() } else { format!(" ({value} is over the account's cap)") };
        return Ok(format!("Switched {title} to {}{capped}: `{text}` went into its terminal.", label(&now)));
    }
    bail!("Codealong can't type into {title}'s terminal{}, so its {what} wasn't switched. Run `{text}` there.", why.map(|w| format!(" ({w})")).unwrap_or_default())
}

/// Sends your prompt one of a harness's own ways in, outside a send: its app's, for a session
/// that goes on there (`start::resume`). An empty prompt opens the session there.
pub(crate) fn deliver_yours(own: &dyn Own, session: &Session, account: &AccountCfg, text: &str) -> Result<Sent> {
    own.deliver(&Delivery { session, account, text, agent: None, bypasses: true, now: false, untyped: None }).or_else(sent_unsure)
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

    /// What types into a session's box waits for what's typing there already (a send behind a
    /// Remove), and what types into another session's doesn't.
    #[test]
    fn one_thing_at_a_time_types_into_a_box() {
        let (said, heard) = std::sync::mpsc::channel();
        let (go, wait) = std::sync::mpsc::channel::<()>();
        let first = std::thread::spawn({
            let said = said.clone();
            move || {
                at_the_box("claude:claude:box-a", || {
                    said.send("removing").unwrap();
                    wait.recv().unwrap();
                    said.send("removed").unwrap();
                })
            }
        });
        assert_eq!(heard.recv().unwrap(), "removing");
        at_the_box("claude:claude:box-b", || said.send("another session's send").unwrap());
        let second = std::thread::spawn({
            let said = said.clone();
            move || at_the_box("claude:claude:box-a", || said.send("this session's send").unwrap())
        });
        go.send(()).unwrap();
        first.join().unwrap();
        second.join().unwrap();
        drop(said);
        assert_eq!(heard.iter().collect::<Vec<_>>(), ["another session's send", "removed", "this session's send"]);
    }

    /// A send that waited for a session's box (behind a Remove) goes by the session as it is once
    /// it has it: one that began waiting on you meanwhile isn't typed into, as what's typed would
    /// answer its prompt.
    #[test]
    fn a_send_that_waited_for_the_box_reads_the_session_again() {
        let cfg: Config = serde_json::from_value(json!({ "accounts": [{ "name": "claude", "harness": "claude", "home": "/h" }] })).unwrap();
        let working = session(Harness::Claude, Surface::Cli, Status::Working, true, Some(7));
        let asks = std::sync::atomic::AtomicBool::new(false);
        let (holding, held) = std::sync::mpsc::channel();
        let (go, wait) = std::sync::mpsc::channel::<()>();
        let err = std::thread::scope(|scope| {
            let (key, turns) = (&working.key, &asks);
            scope.spawn(move || {
                at_the_box(key, || {
                    holding.send(()).unwrap();
                    wait.recv().unwrap();
                    turns.store(true, std::sync::atomic::Ordering::SeqCst);
                })
            });
            held.recv().unwrap();
            go.send(()).unwrap();
            let now = || Session { status: if asks.load(std::sync::atomic::Ordering::SeqCst) { Status::Needs } else { Status::Working }, ..working.clone() };
            send_allowed(|| board_with(now()), &cfg, &working.key, "And the docs", Sender::You, false).unwrap_err()
        });
        assert_eq!(err.to_string(), "“Fix the build” is waiting on you; open it to answer.");
    }

    #[test]
    fn provider_flags_stop_automated_delivery_before_any_route() {
        let mut cfg = Config::default();
        let mut s = crate::fixtures::session("claude:claude:flagged");
        s.flagged = true;
        s.hosted = true;
        let b = Board { lanes: vec![crate::fixtures::lane("/work", vec![s.clone()])], ..Board::default() };
        for sender in [Sender::Codealong, Sender::Agent(None), Sender::Agent(Some("codex:codex:helper".into()))] {
            assert!(send(&b, &cfg, &s.key, "PR checks failed", sender.clone(), false).unwrap_err().to_string().contains("flagged"));
            assert!(ensure_not_flagged(&cfg, &s, &sender).is_err());
        }
        for sender in [Sender::You, Sender::Elsewhere] {
            assert!(ensure_not_flagged(&cfg, &s, &sender).is_ok());
        }
        // The explicit review action gets past the flag, then reaches the normal account
        // check; no real route is run. Its author and routes stay Codealong's.
        assert!(send_allowed(|| &b, &cfg, &s.key, "Review findings", Sender::Codealong, false).unwrap_err().to_string().contains("no account named"));
        let message = Message::new(&b, &cfg, "Review findings", &Sender::Codealong, false);
        assert!(message.text.starts_with(FROM_CODEALONG_ITSELF));
        assert_eq!(message.byline.as_ref().map(|b| b.name.as_str()), Some("Codealong"));
        cfg.sessions.block_flagged_sessions = false;
        assert!(ensure_not_flagged(&cfg, &s, &Sender::Codealong).is_ok());
        s.flagged = false;
        assert!(ensure_not_flagged(&Config::default(), &s, &Sender::Codealong).is_ok());
    }

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

    /// A session that isn't running, on an account turned off, isn't sent anything: OpenCode's
    /// service would take it and start a turn, as a resume would. One that runs still takes it.
    #[test]
    fn a_session_that_isnt_running_on_an_account_turned_off_isnt_sent_to() {
        let off: AccountCfg = serde_json::from_value(json!({ "name": "opencode", "harness": "opencode", "home": "/nowhere/opencode", "disabled": true })).unwrap();
        let cfg = Config { accounts: vec![off], ..Config::default() };
        let idle = Session { key: "opencode:opencode:ses_1".into(), account: "opencode".into(), ..session(Harness::Opencode, Surface::Cli, Status::Idle, false, None) };
        let board = Board { lanes: vec![crate::fixtures::lane("/tmp", vec![idle.clone()])], ..Board::default() };
        let e = deliver(&board, &cfg, &idle.key, "Go on", &Sender::You, false).unwrap_err().to_string();
        assert_eq!(e, "opencode is turned off in Settings › Accounts.");
        let working = Session { status: Status::Working, ..idle };
        let board = Board { lanes: vec![crate::fixtures::lane("/tmp", vec![working.clone()])], ..Board::default() };
        let e = deliver(&board, &cfg, &working.key, "Go on", &Sender::You, false).unwrap_err().to_string();
        assert!(!e.contains("turned off"), "{e}");
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

    /// A way in that may have put it in the session already is the last one tried, and the send
    /// counts as sent, unsure, saying what it said; any other failure goes on to the next way.
    #[test]
    fn a_way_in_that_may_have_put_it_in_counts_as_sent() {
        let routes: Vec<Route> = vec![Route::Type { pid: 42 }, Stand(Way::Message).into()];
        let mut tried = 0;
        let went = through(&routes, "Fix the build", |_, _, _| {
            tried += 1;
            Err(MayHaveArrived("Look there before sending it again.".into()).into())
        });
        assert_eq!(went.unwrap(), Sent { message: "Look there before sending it again.".into(), opened: false, fell_back: false, uncertain: true, queued: None });
        assert_eq!(tried, 1);
        let mut tried = 0;
        let went = through(&routes, "Fix the build", |_, _, _| {
            tried += 1;
            Err(anyhow!("Ghostty said no."))
        });
        assert_eq!(went.unwrap_err().to_string(), "Ghostty said no.");
        assert_eq!(tried, 2);
    }

    /// The same when its app's way is the only one, as a resume there with a prompt sends it.
    #[test]
    fn a_prompt_its_app_may_have_taken_counts_as_sent() {
        #[derive(Debug)]
        struct Unheard;
        impl Own for Unheard {
            fn way(&self) -> Way {
                Way::CodexApp
            }
            fn takes_agents(&self) -> bool {
                false
            }
            fn deliver(&self, _: &Delivery) -> Result<Sent> {
                Err(MayHaveArrived("Look there before sending it again.".into()).into())
            }
        }
        let account: AccountCfg = serde_json::from_value(json!({ "name": "codex", "harness": "codex", "home": "/nowhere/codex" })).unwrap();
        let s = session(Harness::Codex, Surface::Cli, Status::Idle, false, None);
        let sent = deliver_yours(&Unheard, &s, &account, "Go on").unwrap();
        assert_eq!((sent.message.as_str(), sent.uncertain), ("Look there before sending it again.", true));
        assert_eq!(deliver_yours(&Stand(Way::CodexApp), &s, &account, "Go on").unwrap().uncertain, false);
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
        let asks = Session { asks: true, ..s.clone() };
        assert!(ensure_not_waiting(&asks, &Sender::You).is_ok());
        // So does one whose turn ended on a command in the background that waits on a sign-in.
        let stuck = Session { background: Some("acme login --device".into()), stalled_since: Some(1), ..s.clone() };
        assert!(ensure_not_waiting(&stuck, &Sender::You).is_ok());
        assert!(ensure_not_waiting(&stuck, &Sender::Agent(None)).is_ok());
        // And one stuck mid-turn on a command: Claude Code queues what's typed until its step ends.
        let stuck = Session { stalled_since: Some(1), ..s };
        assert!(ensure_not_waiting(&stuck, &Sender::You).is_ok());
        assert!(ensure_not_waiting(&stuck, &Sender::Agent(None)).is_ok());
    }

    /// A helper's message to the session that started it claims that session's class, as yours
    /// does, so a session that skips prompts doesn't hold it; any other agent's claims only its own.
    #[test]
    fn a_helpers_message_to_its_own_session_isnt_held() {
        use crate::model::OriginHow;
        use crate::origins::{Entry, Store};
        let entry = |helper: bool, known: bool| Entry { from: "claude:claude:boss".into(), how: OriginHow::Started, at: 1, helper, report: false, known };
        let mut origins = Store::default();
        origins.sessions.insert("codex:codex:rev".into(), entry(true, true));
        // Matched to its start afterwards: it may be another session started about then.
        origins.sessions.insert("codex:codex:matched".into(), entry(true, false));
        // Started by it, not as a helper.
        origins.sessions.insert("codex:codex:started".into(), entry(false, true));
        // Each is a helper on the board, as the engine marks them (`with_origins`), the review too.
        let of = |key: &str, parent: Option<&str>| Session { key: key.into(), helper: parent.map(|p| crate::model::Helper { parent: Some(p.into()), ..Default::default() }), ..Session::new(Harness::Codex, "codex", key) };
        let helpers = ["codex:codex:rev", "codex:codex:matched", "codex:codex:started", "codex:codex:review"].map(|k| of(k, Some("claude:claude:boss")));
        let board = Board { lanes: vec![crate::fixtures::lane("/r", [vec![of("claude:claude:boss", None)], helpers.to_vec()].concat())], ..Board::default() };
        let (cfg, from) = (Config::default(), |k: &str| Sender::Agent(Some(k.into())));
        assert!(from_its_helper(&board, &cfg, &origins, &from("codex:codex:rev"), "claude:claude:boss"));
        // To any other session, or from any other agent, among them a review no round of a PR
        // asked (autoreview.rs `reviews_for` has the ones that are): as before.
        assert!(!from_its_helper(&board, &cfg, &origins, &from("codex:codex:rev"), "codex:codex:other"));
        assert!(!from_its_helper(&board, &cfg, &origins, &from("codex:codex:matched"), "claude:claude:boss"));
        assert!(!from_its_helper(&board, &cfg, &origins, &from("codex:codex:started"), "claude:claude:boss"));
        assert!(!from_its_helper(&board, &cfg, &origins, &from("codex:codex:review"), "claude:claude:boss"));
        assert!(!from_its_helper(&board, &cfg, &origins, &Sender::Agent(None), "claude:claude:boss"));
        assert!(!from_its_helper(&board, &cfg, &origins, &Sender::Elsewhere, "claude:claude:boss"));
        // Made your own, it's a helper no more, whatever its start said.
        let owned = Board { lanes: vec![crate::fixtures::lane("/r", vec![of("claude:claude:boss", None), of("codex:codex:rev", None)])], ..Board::default() };
        assert!(!from_its_helper(&owned, &cfg, &origins, &from("codex:codex:rev"), "claude:claude:boss"));
    }

    /// One of a harness's own ways in is sent by the harness, with whose the message is and
    /// what its sender may claim: only you, here, are taken to skip permission prompts.
    #[test]
    fn a_way_in_of_a_harnesss_own_is_told_whose_the_message_is() {
        let account: AccountCfg = serde_json::from_value(json!({ "name": "claude", "harness": "claude", "home": "/h" })).unwrap();
        let cfg = Config::default();
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
        let Tried::Sent(went) = attempt(&Stand(Way::Message).into(), &board, &s, &account, &cfg, &msg, None, false).unwrap() else { panic!("it passed") };
        assert_eq!(went.message, "Stood in for “Fix the build”.");
    }

    /// What Codealong sends by itself says so first, goes under its name, claims what you would,
    /// and takes another session's way in before typing; a resume never.
    #[test]
    fn what_codealong_sends_by_itself_says_so_and_goes_as_a_message_first() {
        let account: AccountCfg = serde_json::from_value(json!({ "name": "claude", "harness": "claude", "home": "/h" })).unwrap();
        let (cfg, board) = (Config::default(), Board::default());
        let s = session(Harness::Claude, Surface::Cli, Status::Unread, true, Some(42));
        static CODEALONG: Sender = Sender::Codealong;
        let msg = Message::new(&board, &cfg, " On your PR #7: ken approved it. ", &CODEALONG, false);
        assert_eq!(msg.text, format!("{FROM_CODEALONG_ITSELF}\n\nOn your PR #7: ken approved it."));
        let d = msg.delivery(&s, &account, None);
        assert_eq!((d.agent, d.bypasses), (Some("Codealong"), true));
        assert_eq!(from_codealong(&msg.text), Some("On your PR #7: ken approved it."));
        assert_eq!(from_codealong("On your PR #7"), None);
        // As it was worded before, too.
        let before = format!("{FROM_CODEALONG_ITSELF_BEFORE}\n\nOn your PR #7: ken approved it.");
        assert_eq!((from_codealong(&before), without_byline(&before)), (Some("On your PR #7: ken approved it."), "On your PR #7: ken approved it."));
        assert_eq!(without_byline(&msg.text), "On your PR #7: ken approved it.");
        assert_eq!(by_codealong(" Carry on. "), format!("{FROM_CODEALONG_ITSELF}\n\nCarry on."));

        #[derive(Debug)]
        struct Socket;
        impl Own for Socket {
            fn way(&self) -> Way {
                Way::Message
            }
            fn takes_agents(&self) -> bool {
                true
            }
            fn deliver(&self, d: &Delivery) -> Result<Sent> {
                Ok(sent(format!("Messaged {}.", d.title())))
            }
        }
        #[derive(Debug)]
        struct Job;
        impl Own for Job {
            fn way(&self) -> Way {
                Way::Background
            }
            fn takes_agents(&self) -> bool {
                false
            }
            fn as_typed(&self) -> bool {
                true
            }
            fn deliver(&self, d: &Delivery) -> Result<Sent> {
                Ok(sent(format!("Pasted into {}.", d.title())))
            }
        }
        // An app opened with it to send (the Claude app's way) isn't one, nor a resume.
        let routes = vec![Route::Type { pid: 42 }, Stand(Way::ClaudeApp).into(), Job.into(), Socket.into(), Route::Resume];
        let kept = codealong_routes(&s, routes).unwrap();
        let ways: Vec<String> = kept.iter().map(|r| format!("{r:?}")).collect();
        assert_eq!(ways.len(), 3, "{ways:?}");
        assert!(ways[0].contains("Socket") && ways[1].contains("Type") && ways[2].contains("Job"), "{ways:?}");
        assert!(codealong_routes(&s, vec![Route::Resume, Stand(Way::ClaudeApp).into()]).is_err());

        // What Codealong tells a session that may have stopped (`tell_waking`): sent where it
        // runs; resumed out of sight where its harness goes on by a resume; sent by a way of its
        // harness's that takes it while it doesn't run; else nothing, as for one you archived or
        // whose account you turned off.
        let account = crate::config::AccountCfg { name: "claude".into(), harness: "claude".into(), home: "~/.claude".into(), command: None, start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false };
        let stopped = Session { live: false, ..s.clone() };
        let way = |s: &Session, a: &crate::config::AccountCfg, routes: Vec<Route>| waking(s, a, || routes, || Some(Place::Background));
        assert_eq!(way(&s, &account, vec![]), Waking::Send);
        assert_eq!(way(&stopped, &account, vec![Route::Resume]), Waking::Resume(Place::Background));
        assert_eq!(waking(&stopped, &account, || vec![Route::Resume], || None), Waking::Nowhere, "its account can't go on out of sight");
        assert_eq!(way(&stopped, &account, vec![Socket.into()]), Waking::Send);
        assert_eq!(way(&stopped, &account, vec![Stand(Way::ClaudeApp).into()]), Waking::Nowhere, "only in its app, in front of you");
        assert_eq!(way(&Session { archived: true, ..stopped.clone() }, &account, vec![Route::Resume]), Waking::Nowhere);
        assert_eq!(way(&stopped, &crate::config::AccountCfg { disabled: true, ..account.clone() }, vec![Route::Resume]), Waking::Nowhere);
        // One Codealong ran goes on there when it's sent something, but not on an account turned off.
        let hosted = Session { hosted: true, ..stopped.clone() };
        assert_eq!(way(&hosted, &account, vec![]), Waking::Send);
        assert_eq!(way(&hosted, &crate::config::AccountCfg { disabled: true, ..account.clone() }, vec![]), Waking::Nowhere);
    }

    fn board_with(s: Session) -> Board {
        Board { lanes: vec![crate::fixtures::lane("/w", vec![s])], ..Board::default() }
    }

    /// A session that isn't running is resumed out of sight for any session's message, as the
    /// setting has it; with that off, only for one from a session it started or from the one that
    /// started it, as the other has it. Never where both are off, it's archived, its harness doesn't
    /// go on by a resume, or it would hold the message for its user, nor for a message from no
    /// session.
    #[test]
    fn a_session_that_isnt_running_is_resumed_for_another_sessions_message() {
        use crate::model::OriginHow;
        use crate::origins::{Entry, Store};
        let entry = |from: &str, how: OriginHow, known: bool| Entry { from: from.into(), how, at: 1, helper: false, report: false, known };
        let mut origins = Store::default();
        origins.sessions.insert("claude:claude2:child".into(), entry("claude:claude2:boss", OriginHow::Started, true));
        // Matched to its start afterwards, a fork of it, or another's.
        origins.sessions.insert("codex:codex:matched".into(), entry("claude:claude2:boss", OriginHow::Started, false));
        origins.sessions.insert("claude:claude2:copy".into(), entry("claude:claude2:boss", OriginHow::Fork, true));
        origins.sessions.insert("claude:claude2:other".into(), entry("claude:claude2:else", OriginHow::Started, true));
        origins.sessions.insert("claude:claude2:twin".into(), entry("claude:claude2:boss", OriginHow::Started, true));
        let boss = Session { key: "claude:claude2:boss".into(), title: "Cloud SQL CPU spike".into(), live: false, status: Status::Stopped, ..Session::new(Harness::Claude, "claude2", "boss") };
        let board = board_with(boss.clone());
        let cfg = Config::default();
        let senders = ["claude:claude2:child", "codex:codex:matched", "claude:claude2:copy", "claude:claude2:other"].map(|k| Sender::Agent(Some(k.into())));
        let resumes = [Route::Resume];
        let wakes_for = |cfg: &Config, sender: &Sender, s: &Session, routes: &[Route], bypasses: bool, holds: bool| {
            let mut msg = Message::new(&board, cfg, "PR #1199 is open.", sender, false);
            msg.bypasses = bypasses;
            wakes(cfg, &origins, &msg, s, routes, || holds, || Some(Place::Background)).map_err(|e| e.to_string())
        };
        let woken = |link| Ok(Some((Place::Background, link)));
        assert_eq!(wakes_for(&cfg, &senders[0], &boss, &resumes, false, false), woken(Link::StartedYours));
        // Any other session's message too, saying no start links them.
        for sender in &senders[1..] {
            assert_eq!(wakes_for(&cfg, sender, &boss, &resumes, false, false), woken(Link::Neither), "{sender:?}");
        }
        // From no session, to a running one, or to one that goes on in its app: no further than
        // any agent's message goes.
        for sender in [Sender::Agent(None), Sender::You, Sender::Codealong] {
            assert_eq!(wakes_for(&cfg, &sender, &boss, &resumes, false, false), Ok(None), "{sender:?}");
        }
        let running = Session { live: true, status: Status::Idle, ..boss.clone() };
        assert_eq!(wakes_for(&cfg, &senders[3], &running, &resumes, false, false), Ok(None));
        assert_eq!(wakes_for(&cfg, &senders[3], &boss, &[Stand(Way::ClaudeApp).into()], false, false), Ok(None));
        // Each of the rest says why not.
        let archived = Session { archived: true, ..boss.clone() };
        assert!(wakes_for(&cfg, &senders[3], &archived, &resumes, false, false).unwrap_err().contains("archived it or marked it done"));
        let held = wakes_for(&cfg, &senders[3], &boss, &resumes, false, true).unwrap_err();
        assert!(held.starts_with("“Cloud SQL CPU spike” isn't running, and it skips permission prompts (or may: Codealong can't tell) where your session doesn't"), "{held}");
        // One that skips them too is no hold.
        assert_eq!(wakes_for(&cfg, &senders[3], &boss, &resumes, true, true), woken(Link::Neither));
        let mut msg = Message::new(&board, &cfg, "hi", &senders[3], false);
        msg.bypasses = true;
        assert!(wakes(&cfg, &origins, &msg, &boss, &resumes, || false, || None).unwrap_err().to_string().contains("can't resume it out of sight"));

        // With any session's turned off, only along a start, either way.
        let mut starts = Config::default();
        starts.sessions.resume_for_any_session = false;
        assert_eq!(wakes_for(&starts, &senders[0], &boss, &resumes, false, false), woken(Link::StartedYours));
        for sender in &senders[1..] {
            assert!(wakes_for(&starts, sender, &boss, &resumes, false, false).unwrap_err().contains("turned off resuming a session for another session's message"), "{sender:?}");
        }
        let child = Session { key: "claude:claude2:child".into(), title: "Prescient follow-ups".into(), live: false, status: Status::Stopped, ..Session::new(Harness::Claude, "claude2", "child") };
        let from_boss = Sender::Agent(Some("claude:claude2:boss".into()));
        assert_eq!(wakes_for(&starts, &from_boss, &child, &resumes, false, false), woken(Link::YoursStarted));
        assert_eq!(wakes_for(&cfg, &from_boss, &child, &resumes, false, false), woken(Link::YoursStarted));
        // Not one only matched to its start afterwards, a fork of it, one another started, nor
        // another session the same one started.
        for key in ["codex:codex:matched", "claude:claude2:copy", "claude:claude2:other"] {
            let s = Session { key: key.into(), ..child.clone() };
            assert!(wakes_for(&starts, &from_boss, &s, &resumes, false, false).is_err(), "{key}");
        }
        assert!(wakes_for(&starts, &Sender::Agent(Some("claude:claude2:twin".into())), &child, &resumes, false, false).is_err());
        // With both off, none; with any session's on, the other is part of it.
        let mut off = starts.clone();
        off.sessions.resume_for_sessions_it_started = false;
        assert!(wakes_for(&off, &senders[0], &boss, &resumes, false, false).unwrap_err().contains("turned off resuming a session for one it started, or for any session"));
        assert!(wakes_for(&off, &from_boss, &child, &resumes, false, false).unwrap_err().contains("turned off resuming a session for the one that started it"));
        let any = Config { sessions: crate::config::SessionsCfg { resume_for_sessions_it_started: false, ..Config::default().sessions }, ..Config::default() };
        assert_eq!(wakes_for(&any, &senders[0], &boss, &resumes, false, false), woken(Link::StartedYours));
        assert!(wakes_for(&starts, &from_boss, &Session { archived: true, ..child.clone() }, &resumes, false, false).unwrap_err().contains("archived it or marked it done"));
        assert!(wakes_for(&starts, &from_boss, &child, &resumes, false, true).unwrap_err().contains("where your session doesn't"));
    }

    /// A message that resumed its session is the agent's, as the conversation shows it, after the
    /// line that says the session had stopped.
    #[test]
    fn a_message_that_resumed_its_session_is_the_agents() {
        let intro = byline(&Board::default(), Some("claude:claude2:boss")).intro;
        let resumed = format!("{RESUMED_FOR_A_MESSAGE}\n\n{intro}\n\nPR #1199 is open.");
        assert_eq!(from_an_agent(&resumed), Some("PR #1199 is open."));
        assert_eq!(from_an_agent(&format!("{RESUMED_FOR_A_MESSAGE}\n\nGo on.")), None);
        let board = board_with(Session { key: "claude:claude2:boss".into(), title: "Cloud SQL CPU spike".into(), ..Session::new(Harness::Claude, "claude2", "boss") });
        let named = format!("{RESUMED_FOR_A_MESSAGE}\n\n{}\n\nPR #1199 is open.", byline(&board, Some("claude:claude2:boss")).intro);
        assert_eq!(sender_of(&named), Some("Cloud SQL CPU spike"));
    }

    /// A title or excerpt taken from a message Codealong or another session sent says what it
    /// asks, not the lines that say who sent it: a Codex thread was titled "Codealong sent this by
    /// itself…" once a PR's news reached it.
    #[test]
    fn what_a_message_asks_is_past_the_lines_that_say_who_sent_it() {
        let news = "On your PR #7: ken approved it.";
        assert_eq!(past_sender_lines(&by_codealong(news)), news);
        assert_eq!(past_sender_lines(&format!("{FROM_CODEALONG_ITSELF_BEFORE}\n\n{news}")), news);
        assert_eq!(past_sender_lines(&by_codealong(&format!("{RESUMED_TO_TELL}\n\n{news}"))), news);
        let intro = byline(&Board::default(), Some("claude:claude2:boss")).intro;
        assert_eq!(past_sender_lines(&format!("{intro}\n\nPR #1199 is open.")), "PR #1199 is open.");
        assert_eq!(past_sender_lines(&format!("{RESUMED_FOR_A_MESSAGE}\n\n{intro}\n\nPR #1199 is open.")), "PR #1199 is open.");
        // Yours, as it is, even when it quotes Codealong further on.
        let yours = format!("Why does this say {FROM_CODEALONG_ITSELF}?");
        assert_eq!(past_sender_lines(&yours), yours);
    }

    /// A session resumed for a message isn't resumed again for another until the board has had
    /// time to find it running, and then may be, should that resume have come to nothing.
    #[test]
    fn a_session_resumed_for_a_message_isnt_resumed_again_at_once() {
        let mut woken = BTreeMap::new();
        assert!(claim_wake(&mut woken, "claude:claude2:boss", 1_000));
        assert!(!claim_wake(&mut woken, "claude:claude2:boss", 6_000));
        assert!(claim_wake(&mut woken, "codex:codex:other", 6_000));
        assert_eq!(woken.len(), 2);
        assert!(claim_wake(&mut woken, "claude:claude2:boss", 1_000 + WAKING_MS));
        assert_eq!(woken, BTreeMap::from([("claude:claude2:boss".to_string(), 1_000 + WAKING_MS), ("codex:codex:other".to_string(), 6_000)]));
        // Claimed through the file, as the app and the command each do.
        let path = crate::store::test_dir("woken").join("woken.json");
        let claim = |at: i64| {
            let mut claimed = false;
            crate::store::change(&path, WOKEN, |w: &mut BTreeMap<String, i64>| {
                claimed = claim_wake(w, "claude:claude2:boss", at);
                claimed
            })
            .unwrap();
            claimed
        };
        assert!(claim(1_000) && !claim(2_000) && claim(1_000 + WAKING_MS));
    }

    /// Switching a running session's model is refused where the command would go in as a
    /// prompt or answer what's asked, and for a model the account doesn't take.
    #[test]
    fn switches_a_model_only_between_turns() {
        let cfg: Config = serde_json::from_value(json!({ "accounts": [{ "name": "claude", "harness": "claude", "home": "/h" }, { "name": "codex", "harness": "codex", "home": "/c" }] })).unwrap();
        let refused = |s: Session, model: &str| switch_model(&board_with(s.clone()), &cfg, &s.key, model).unwrap_err().to_string();
        let idle = session(Harness::Claude, Surface::Cli, Status::Idle, true, Some(7));
        assert_eq!(refused(Session { live: false, status: Status::Stopped, ..idle.clone() }, "haiku"), "“Fix the build” isn't running: pick its model as it resumes.");
        assert_eq!(refused(Session { status: Status::Working, ..idle.clone() }, "haiku"), "“Fix the build” is in the middle of a turn: switch its model once the turn ends.");
        assert!(refused(Session { status: Status::Needs, ..idle.clone() }, "haiku").contains("is waiting on you"));
        assert!(refused(idle.clone(), "haiku; rm -rf ~").contains("isn't a model Codealong can switch"));
        // A Codex terminal is switched there: Codealong types no command into one.
        let thread = Session { key: "codex:codex:t1".into(), account: "codex".into(), harness: Harness::Codex, ..idle.clone() };
        assert_eq!(refused(thread.clone(), "gpt-6-sol"), "Codex has no command to switch a running session's model.");
        // A thread in the Codex app is switched there, from its next turn, running or not: here
        // no app is running to take it.
        let app = Session { surface: Surface::App, pid: None, live: false, status: Status::Idle, ..thread.clone() };
        assert_eq!(refused(app.clone(), "gpt-6-sol"), "The Codex app isn't running.");
        assert_eq!(refused(Session { live: true, status: Status::Working, ..app.clone() }, "gpt-6-sol"), "The Codex app isn't running.");
        assert_eq!(switch_effort(&board_with(app.clone()), &cfg, &app.key, "high").unwrap_err().to_string(), "The Codex app isn't running.");
        // An effort level the same way, and only one of the account's.
        let refused = |s: Session, effort: &str| switch_effort(&board_with(s.clone()), &cfg, &s.key, effort).unwrap_err().to_string();
        assert_eq!(refused(Session { status: Status::Working, ..idle.clone() }, "high"), "“Fix the build” is in the middle of a turn: switch its effort once the turn ends.");
        assert!(refused(idle.clone(), "turbo").contains("isn't an effort level Codealong can switch"));
        assert_eq!(refused(thread.clone(), "high"), "Codex has no command to switch a running session's effort.");
        assert!(refused(thread, "turbo").contains("isn't an effort level Codealong can switch"));
    }

    /// A mode is switched only for a session that offers modes here (one Codealong runs, or one
    /// in a terminal it presses keys in), and only to one of them; any other has its mode changed
    /// where it runs.
    #[test]
    fn switches_a_mode_only_where_codealong_can() {
        let cfg: Config = serde_json::from_value(json!({ "accounts": [{ "name": "claude", "harness": "claude", "home": "/h" }] })).unwrap();
        let refused = |s: Session, mode: &str| switch_mode(&board_with(s.clone()), &cfg, &s.key, mode).unwrap_err().to_string();
        let terminal = session(Harness::Claude, Surface::Cli, Status::Idle, true, Some(7));
        assert_eq!(refused(terminal.clone(), "auto"), "Codealong can't switch “Fix the build”'s mode: change it where it runs.");
        let manual = crate::claude::session_mode("default").unwrap().with_id("default");
        let hosted = Session { hosted: true, modes: vec![manual.clone()], ..terminal.clone() };
        assert_eq!(refused(hosted, "auto"), "\"auto\" isn't a mode “Fix the build”'s agent offers.");
        // In a terminal, a key pressed there would go to what it waits on.
        let waiting = Session { status: Status::Needs, modes: vec![manual], ..terminal };
        assert_eq!(refused(waiting, "default"), "“Fix the build” is waiting on you, and what Codealong pressed in its terminal would go to what it asks: answer it first.");
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
        // The session's conversation shows it without the byline, either kind, and from the
        // session the byline names.
        assert_eq!(without_byline(&msg.text), "Which test fails?");
        assert_eq!(without_byline(&format!("{}\n\nAnd the docs?", byline(&board, None).intro)), "And the docs?");
        assert_eq!(without_byline("Plain words\n\nstay whole"), "Plain words\n\nstay whole");
        assert_eq!(sender_of(&msg.text), Some("Review the parser"));
        assert_eq!(sender_of(&format!("{}\n\nAnd the docs?", byline(&board, None).intro)), None);
        assert_eq!(sender_of("“Review the parser” says hi"), None);
        assert_eq!(Message::new(&board, &Config::default(), "hi", &Sender::You, false).text, "hi");
    }

    /// A session that moved to another account is neither sent to nor resumed: it would come back
    /// to life beside its copy. The copy goes on as ever, and so does a session whose copy is gone.
    #[test]
    fn a_moved_session_is_neither_sent_to_nor_resumed() {
        let mut origins = crate::origins::Store::default();
        origins.sessions.insert("claude:claude2:copy".into(), crate::origins::Entry { from: "claude:claude:abc".into(), how: crate::model::OriginHow::Fork, at: 1, helper: false, report: false, known: false });
        let moved = session(Harness::Claude, Surface::Cli, Status::Stopped, false, None);
        let copy = Session { key: "claude:claude2:copy".into(), account: "claude2".into(), ..moved.clone() };
        let board = Board { lanes: vec![crate::fixtures::lane("/w", vec![moved.clone(), copy])], ..Board::default() };
        let e = ensure_not_moved(&origins, &board, "claude:claude:abc").unwrap_err().to_string();
        assert_eq!(e, "“Fix the build” moved to claude2 and goes on there as claude:claude2:copy. Send to that session instead.");
        assert!(ensure_not_moved(&origins, &board, "claude:claude2:copy").is_ok());
        assert!(ensure_not_moved(&origins, &board_with(moved), "claude:claude:abc").is_ok());
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
        let sent = |m: &str| Ok(super::sent(m.into()));
        let (started, on_way) = mpsc::channel();
        let (go, wait) = mpsc::channel::<()>();
        let first = std::thread::spawn(move || {
            join("k-once", "hello", || {
                started.send(()).unwrap();
                wait.recv().unwrap();
                Ok(super::sent("Typed.".into()))
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
