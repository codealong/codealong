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
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use anyhow::{anyhow, bail, Result};
use serde_json::json;

use crate::attachments::{self, Style};
use crate::claude::daemon::{daemon_reply, daemon_type};
use crate::claude::peer::{peer_note, peer_send};
use crate::codex::ipc::{codex_start_turn, codex_steer_turn};
use crate::config::{AccountCfg, Config};
use crate::launch;
use crate::model::{Board, Session, Status, Surface};
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
        Route::Daemon { .. } => Way::Background,
        Route::Type { pid } => Way::Typed { app: terminal(*pid)?.to_string() },
        Route::Peer { .. } => Way::Message,
        Route::Resume => Way::Resume { app: resume_in.to_string() },
        Route::ClaudeApp => Way::ClaudeApp,
        Route::CodexApp => Way::CodexApp,
        Route::Opencode => Way::Opencode,
        Route::Antigravity => Way::Antigravity,
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
fn ensure_not_waiting(session: &Session, sender: &Sender) -> Result<()> {
    if session.status == Status::Needs && !session.asks {
        match sender {
            Sender::Agent(_) => bail!("{} is waiting on its user to answer a prompt, so nothing was sent. Try again once they have.", quoted(&session.title)),
            _ => bail!("{} is waiting on you; open it to answer.", quoted(&session.title)),
        }
    }
    Ok(())
}

/// The ways in an agent's message may take, of the session's own. A harness with a way in for
/// other sessions' messages (`Adapter::takes_messages`: Claude Code's socket, which tells the
/// model a message is another session's and holds it for the user where the session skips
/// permission prompts) takes it only that way: typed into its terminal or handed to its daemon,
/// the message would read as its user's own words. The others take it typed or posted, the
/// byline saying whose it is. A way that resumes a session or leaves the prompt for someone to
/// send isn't for an agent.
fn agent_routes(session: &Session, routes: Vec<Route>) -> Result<Vec<Route>> {
    let adapter = crate::harness::adapter(session.harness);
    let left_to_send = routes.iter().any(|r| matches!(r, Route::Antigravity));
    let kept: Vec<Route> = routes
        .into_iter()
        .filter(|r| match r {
            Route::Peer { .. } | Route::CodexApp | Route::Opencode => true,
            Route::Type { .. } => !adapter.takes_messages(),
            Route::Daemon { .. } | Route::Resume | Route::ClaudeApp | Route::Antigravity => false,
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
    // Sending now matters only while a turn runs: otherwise the prompt starts one either way.
    let now = msg.now && busy;
    let adapter = crate::harness::adapter(session.harness);
    let title = quoted(&session.title);
    let sent = |message: String| Ok(Tried::Sent(Sent { message, opened: false, fell_back: false }));
    let opened = |message: String| Ok(Tried::Sent(Sent { message, opened: true, fell_back: false }));
    match route {
        Route::Daemon { socket, key_file, job } => {
            let expanded = attachments::expand(text, Style::Mention);
            let typed = boxed(&expanded);
            // `reply` pastes and presses Return; its send-now keys go in through an attach.
            if let Some(keys) = adapter.sends_now_with().filter(|_| now) {
                daemon_type(socket, key_file, job, &[launch::pasted(&typed).as_bytes(), keys.as_bytes()])?;
                return sent(format!("Sent now to {title} in the background: it reads it without waiting for its current step."));
            }
            let key = std::fs::read_to_string(key_file).map_err(|e| anyhow!("Claude Code's daemon key: {e}"))?;
            daemon_reply(socket, key.trim(), job, &typed)?;
            sent(if busy { format!("Queued for {title}; the background session takes it after its current step.") } else { format!("Sent to {title} in the background.") })
        }
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
                Ok(launch::Typed::Into(app)) => sent(match (now, submit, keys) {
                    (true, launch::Submit::Keys(_), _) => format!("Typed into {title} in {app} and sent now: it reads it without waiting for its current step."),
                    (true, _, None) => format!("Typed into {title} in {app} and sent now: it takes it into the turn it's taking."),
                    (true, _, Some(_)) => format!("Typed into {title} in {app}, which can't press the keys that send it now, so it takes it after its current step."),
                    _ if busy => format!("Typed into {title} in {app}; it takes it after its current step."),
                    _ => format!("Typed into {title} in {app}."),
                }),
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
            let (input, env) = adapter.resume(account, &session.id, text)?;
            launch::terminal_run(&cfg.apps.terminal, place, Path::new(&session.cwd), &input, &env)?;
            sent(format!("Resumed {title} in {} with this prompt.", launch::terminal_name(&cfg.apps.terminal)))
        }
        Route::ClaudeApp => {
            copy(&attachments::expand(text, Style::Path))?;
            launch::open_url(&crate::claude::app::claude_app_url(&session.id))?;
            opened(format!("{title} is open in the Claude app and the prompt is on the clipboard. Paste it there to send it."))
        }
        Route::CodexApp => {
            let prompt = attachments::expand(text, Style::Path);
            if now {
                match codex_steer_turn(&account.home_path(), &session.id, &prompt, &session.cwd) {
                    Ok(()) => return sent(format!("Sent now to {title} in Codex: it takes it into the turn it's taking.")),
                    Err(e) if e.is::<MayHaveArrived>() => return Err(e),
                    // Its turn ended meanwhile, or no window runs it: a turn starts on it, as
                    // any send does.
                    Err(_) => {}
                }
            }
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
            let body = json!({ "text": attachments::expand(text, Style::Path), "delivery": if now { "steer" } else { "queue" } });
            crate::opencode::post(&svc, &format!("/api/session/{}/prompt", urlencoding::encode(&session.id)), &body)?;
            sent(if now {
                format!("Sent now to {title} in OpenCode: it takes it into the turn it's taking.")
            } else if busy {
                format!("Queued for {title} in OpenCode; it takes it after its current step.")
            } else {
                format!("Sent to {title} in OpenCode.")
            })
        }
        Route::Antigravity => {
            copy(&attachments::expand(text, Style::Path))?;
            launch::open_antigravity()?;
            opened("Antigravity is open and the prompt is on the clipboard. Paste it there to send it.".to_string())
        }
    }
}

/// A prompt as it's pasted into a session's input box, for the Return that follows to send it.
/// While the text ends in an `@` word, Claude Code and Codex both show a list to pick from
/// (files; Codex its plugins too), and Return picks from it instead of sending: the prompt
/// stays in the box unsent, its last word swapped for the pick. Claude Code's `@"<path>"` loses
/// its quotes that way, so a path with a space in it isn't attached once the prompt is sent by
/// hand. A space after the word closes the list (Claude Code leaves the space out of the
/// prompt), so a prompt that ends in one, as one with an image last does, gets a space.
fn boxed(text: &str) -> Cow<'_, str> {
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

fn quoted(title: &str) -> String {
    format!("“{}”", title.trim())
}

fn copy(text: &str) -> Result<()> {
    procs::run_stdin("/usr/bin/pbcopy", &[], text, Duration::from_secs(5)).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::claude::routes::claude_routes;
    use crate::model::{Harness, Session, Status, Surface};
    use serde_json::Value;
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
    fn says_the_first_way_in_that_applies() {
        let in_ghostty = |pid: u32| (pid == 42).then_some("Ghostty");
        let first = |routes: &[Route]| routes.iter().find_map(|r| way_of(r, &in_ghostty, "iTerm2"));
        let peer = Route::Peer { socket: "/tmp/cc-socks/42.sock".into(), session_id: "abc".into(), mode: None };
        assert_eq!(first(&[Route::Type { pid: 42 }, peer.clone()]), Some(Way::Typed { app: "Ghostty".into() }));
        // A session in no terminal Codealong can type into gets a message on its socket instead.
        assert_eq!(first(&[Route::Type { pid: 7 }, peer.clone()]), Some(Way::Message));
        assert_eq!(first(&[Route::Type { pid: 7 }]), None);
        let daemon = Route::Daemon { socket: "/d.sock".into(), key_file: "/k".into(), job: "j".into() };
        assert_eq!(first(&[daemon, peer]), Some(Way::Background));
        assert_eq!(first(&[Route::Resume]), Some(Way::Resume { app: "iTerm2".into() }));
        assert_eq!(first(&[Route::CodexApp]), Some(Way::CodexApp));
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
        let msg = |sender: &'static Sender| Message::new(&board, &cfg, "hi", sender, false);
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
        let msg = Message::new(&Board::default(), &cfg, "hi", &agent, false);
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

    /// A folder of its own under the system's temporary one, short enough for a socket path.
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ca-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
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
        // A turn that ended with a command left running in the background shows as working, with
        // no turn to stop.
        assert!(!stoppable(&Session { background: Some("gh pr checks --watch".into()), ..hosted(Status::Working, true, None) }, None));

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
    fn offers_send_now_only_while_a_turn_runs_where_its_harness_takes_one() {
        use crate::harness::steerable;
        let home = scratch("steerable");
        let account = |harness: &str| -> AccountCfg { serde_json::from_value(json!({ "name": "a", "harness": harness, "home": home.to_string_lossy() })).unwrap() };
        // The Agent Client Protocol has no way to, and the Claude app takes messages only on its
        // socket, where none goes in sooner.
        let hosted = Session { hosted: true, ..session(Harness::Claude, Surface::Cli, Status::Working, true, None) };
        assert!(!steerable(&hosted, Some(&account("claude"))));
        assert!(!steerable(&session(Harness::Claude, Surface::App, Status::Working, true, Some(1)), Some(&account("claude"))));
        assert!(!steerable(&session(Harness::Antigravity, Surface::Cli, Status::Working, true, Some(1)), Some(&account("antigravity"))));
        // A Codex app thread takes a steer through the app's windows, while they're there to ask,
        // and only while its turn runs.
        let thread = session(Harness::Codex, Surface::App, Status::Working, true, None);
        assert!(!steerable(&thread, Some(&account("codex"))));
        std::fs::create_dir_all(home.join("ipc")).unwrap();
        std::fs::write(home.join("ipc/ipc.sock"), "").unwrap();
        assert!(steerable(&thread, Some(&account("codex"))));
        for status in [Status::Idle, Status::Unread, Status::Needs] {
            assert!(!steerable(&Session { status, ..thread.clone() }, Some(&account("codex"))));
        }
        assert!(!steerable(&Session { live: false, ..thread.clone() }, Some(&account("codex"))));
        assert!(!steerable(&thread, None));
        // A terminal session in no terminal Codealong types into.
        assert!(!steerable(&session(Harness::Codex, Surface::Cli, Status::Working, true, Some(u32::MAX)), Some(&account("codex"))));
        assert!(!steerable(&session(Harness::Claude, Surface::Cli, Status::Working, true, Some(u32::MAX)), Some(&account("claude"))));
        let _ = std::fs::remove_dir_all(&home);

        // What's typed after the prompt to send it now: Claude Code's Ctrl+X Ctrl+S; Return
        // already does for Codex's terminal UI.
        assert_eq!(crate::harness::adapter(Harness::Claude).sends_now_with(), Some("\u{18}\u{13}"));
        assert_eq!(crate::harness::adapter(Harness::Codex).sends_now_with(), None);
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
    fn resumes_with_the_prompt_in_the_environment() {
        let account = AccountCfg { name: "claude2".into(), harness: "claude".into(), home: "~/.claude2".into(), command: None, start: None, preferred: false, acp: None };
        let (input, env) = crate::harness::adapter(Harness::Claude).resume(&account, "abc", "-v please").unwrap();
        assert_eq!(input, format!("claude2 --resume abc \"${}\"\n", crate::start::PROMPT_VAR));
        assert_eq!(env, vec![(crate::start::PROMPT_VAR.to_string(), " -v please".to_string())]);
        let codex = AccountCfg { name: "codex".into(), harness: "codex".into(), home: "~/.codex".into(), command: Some("codex".into()), start: None, preferred: false, acp: None };
        let (input, env) = crate::harness::adapter(Harness::Codex).resume(&codex, "01a0e379", "-v please").unwrap();
        assert_eq!(input, format!("codex resume -- 01a0e379 \"${}\"\n", crate::start::PROMPT_VAR));
        assert_eq!(env, vec![(crate::start::PROMPT_VAR.to_string(), "-v please".to_string())]);
    }

}
