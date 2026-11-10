//! Talking to a session: a key held while you talk (`[shortcuts] talk`, `talk_orchestrator`),
//! what you say turned into text on this Mac (ear.rs) and put where you'd have typed it. It's
//! the app's own, not the server's: the keys, the window you're in and the notch that shows what's
//! heard are all here, and so nothing is heard while the app is closed.
//!
//! Hearing starts as the key goes down, before the speech model is ready (the helper keeps what's
//! said meanwhile), and ends as it comes up: the microphone is off at once, and when the last
//! words have settled the text goes where it's for, after any said before it:
//!
//! - `talk`: to the Codealong window you're in, which types it where its caret is, or failing
//!   that into the box of the session it shows (src/talk.ts). In another app, or a window with
//!   nowhere to put it, it opens the quick panel on it, which finds the session it's for or
//!   starts one. Nothing is sent: what a machine heard is yours to read over first.
//! - `talk_orchestrator`: to the session that runs as your orchestrator (board.rs
//!   `orchestrator`), whatever you're looking at. Into its box, the main window brought forward on
//!   it; or, with `[voice] send_on_release` or Shift held as you let go, sent, with a line saying
//!   it was dictated, since nobody read it over.
//!
//! What you say is yours, as what you type is: it goes the way a window's box sends
//! (`send_to_session`). That's why only the microphone is ever heard here. What a meeting says is
//! other people's words, and listen.rs keeps those apart.
//!
//! The notch shows it as it goes (the `talk` event, src/ui/Talk.tsx): that it's listening and to
//! whom, the words as they're heard, where they went. Without the bar there's nothing to show it
//! on, and it works the same unseen.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager};

use crate::ear::{self, Ear, Event, Source, Spec};

/// How long a key may be held: one that never came up (the release was lost to another app's
/// own use of the keys) doesn't leave the microphone on.
const LONGEST: Duration = Duration::from_secs(180);

/// How often the notch is told what's heard, at most: every one is a script run in its page.
const SHOW_EVERY: Duration = Duration::from_millis(80);

/// The line that goes with what's sent unread, so the session knows how far to trust its words.
const DICTATED: &str = "(Dictated, and sent as I let go of the key: some words may be misheard.)";

/// Who's talked to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum To {
    /// Whatever you're looking at.
    Looking,
    Orchestrator,
}

/// What the notch is told of a key held to talk (the `talk` event).
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Talk {
    pub id: u64,
    /// `listening` while the key is down; then `settling` until the last words are in; then one
    /// of `typed` (it went to a window's caret or a session's box), `sent`, `asked` (the quick
    /// panel has it), `nothing` (nothing was said) or `failed`.
    pub phase: &'static str,
    /// `looking` or `orchestrator`.
    pub to: &'static str,
    /// The session it's for, by title, where one is known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// What's been heard so far.
    pub text: String,
    /// How loud the microphone is, 0 to 1.
    pub level: f64,
    /// What went wrong, or what to know: no orchestrator runs, the microphone isn't allowed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// What a window is handed to put where its caret is (the `talked` event).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Talked {
    text: String,
    /// The session whose box it's for, when it's for one whatever the window shows.
    #[serde(skip_serializing_if = "Option::is_none")]
    key: Option<String>,
}

/// What's been heard of one press.
#[derive(Default)]
struct Heard {
    settled: Vec<String>,
    unsettled: String,
    level: f64,
    failed: Option<ear::Refused>,
    /// What it waits on that passes: the speech model downloading, the first time a language
    /// is used.
    waiting: Option<String>,
    shown: Option<Instant>,
}

impl Heard {
    fn text(&self) -> String {
        words(&self.settled, &self.unsettled)
    }
}

/// What was said, as one text: the settled stretches and the one still being heard.
fn words(settled: &[String], unsettled: &str) -> String {
    settled.iter().map(String::as_str).chain(std::iter::once(unsettled)).map(str::trim).filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" ")
}

/// A key that's down.
struct Live {
    id: u64,
    to: To,
    /// The Codealong window that had the keyboard as the key went down, by label.
    window: Option<String>,
    /// None until the helper is capturing.
    ear: Option<Ear>,
    heard: Arc<Mutex<Heard>>,
    /// The key came up before the helper was capturing: it's finished as soon as it is.
    released: bool,
    /// Shift was down as the key came up.
    shift: bool,
}

static LIVE: Mutex<Option<Live>> = Mutex::new(None);
static NEXT: AtomicU64 = AtomicU64::new(1);

fn live() -> MutexGuard<'static, Option<Live>> {
    LIVE.lock().unwrap_or_else(|e| e.into_inner())
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn show(app: &AppHandle, talk: &Talk) {
    let _ = app.emit_to(crate::bar::LABEL, "talk", talk);
}

fn word(to: To) -> &'static str {
    match to {
        To::Looking => "looking",
        To::Orchestrator => "orchestrator",
    }
}

/// The Codealong window that has the keyboard, by label. On the main thread.
fn key_window(app: &AppHandle) -> Option<String> {
    app.webview_windows().into_iter().find(|(label, w)| label != crate::bar::LABEL && crate::os::windows::has_keys(w)).map(|(label, _)| label)
}

/// One of the keys held to talk went down. On the main thread.
pub fn press(app: &AppHandle, to: To) {
    let id = {
        let mut live = live();
        if live.is_some() {
            // Down again without having come up: its release was lost, and this is it.
            drop(live);
            release(app);
            return;
        }
        let id = NEXT.fetch_add(1, Ordering::SeqCst);
        *live = Some(Live { id, to, window: key_window(app), ear: None, heard: Arc::default(), released: false, shift: false });
        id
    };
    show(app, &Talk { id, phase: "listening", to: word(to), ..Talk::default() });
    let app = app.clone();
    std::thread::spawn(move || begin(&app, id, to));
}

/// Starts the helper for press `id`, off the main thread: it waits on the person where macOS has
/// yet to ask them for the microphone.
fn begin(app: &AppHandle, id: u64, to: To) {
    let cfg = crate::config::current().unwrap_or_default();
    let heard = match live().as_ref().filter(|l| l.id == id) {
        Some(l) => l.heard.clone(),
        None => return,
    };
    let spec = Spec { sources: vec![Source { name: crate::listen::YOU.into(), from: ear::From::Mic { cancel_echo: false } }], locale: cfg.voice.locale.clone(), hints: hints(&cfg) };
    let (shows, into) = (app.clone(), heard.clone());
    let started = Ear::start(&spec, move |event| {
        let talk = {
            let mut heard = lock(&into);
            match event {
                Event::Partial { text, .. } => heard.unsettled = text,
                Event::Final { text, .. } => {
                    heard.settled.push(text);
                    heard.unsettled.clear();
                }
                Event::Level { level, .. } => heard.level = level,
                Event::Waiting { message } => heard.waiting = Some(message),
                Event::Ready => heard.waiting = None,
                Event::Failed(refused) => heard.failed = Some(refused),
                Event::Lost { message, .. } => heard.failed = Some(ear::Refused { code: "lost".into(), message }),
                _ => return,
            }
            if heard.shown.is_some_and(|at| at.elapsed() < SHOW_EVERY) {
                return;
            }
            heard.shown = Some(Instant::now());
            Talk { id, phase: "listening", to: word(to), text: heard.text(), level: heard.level, message: heard.waiting.clone(), ..Talk::default() }
        };
        show(&shows, &talk);
    });
    let ear = match started {
        Ok(ear) => ear,
        Err(refused) => {
            let mut live = live();
            if live.as_ref().is_some_and(|l| l.id == id) {
                *live = None;
            }
            drop(live);
            show(app, &Talk { id, phase: "failed", to: word(to), message: Some(crate::listen::refusal(refused).to_string()), ..Talk::default() });
            return;
        }
    };
    let released = {
        let mut live = live();
        match live.as_mut().filter(|l| l.id == id) {
            Some(l) => {
                l.ear = Some(ear);
                l.released
            }
            // Gone meanwhile: the helper goes with the ear.
            None => return,
        }
    };
    if released {
        finish_in_turn(app, take(id));
        return;
    }
    // A key that never comes up is let go of for it. This thread goes as soon as the key does.
    let until = Instant::now() + LONGEST;
    while live().as_ref().is_some_and(|l| l.id == id) {
        if Instant::now() >= until {
            finish_in_turn(app, take(id));
            return;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

/// Takes press `id` out of `LIVE`, if it's still the one there: it's over, and the next can begin.
fn take(id: u64) -> Option<Live> {
    let mut live = live();
    if live.as_ref().is_some_and(|l| l.id == id) {
        live.take()
    } else {
        None
    }
}

/// Words a speech model wouldn't guess, which are likely here: what the projects are called.
fn hints(cfg: &crate::config::Config) -> Vec<String> {
    let mut hints: Vec<String> = cfg.projects.iter().map(|p| p.name.clone()).collect();
    hints.extend(["Codealong", "orchestrator", "worktree", "rebase"].map(String::from));
    hints
}

/// The key came up. On the main thread.
pub fn release(app: &AppHandle) {
    let over = {
        let mut live = live();
        let Some(l) = live.as_mut() else { return };
        l.shift = crate::os::windows::shift_down();
        if l.ear.is_none() {
            // Still starting: `begin` finishes it once it's capturing.
            l.released = true;
            return;
        }
        live.take()
    };
    finish_in_turn(app, over);
}

/// A press that's over, and what says its helper has stopped, its last words in.
type Over = (AppHandle, Live, std::sync::mpsc::Receiver<()>);

/// Finishes a press that's over. Two things, apart:
///
/// - its helper is stopped now, on a thread of its own, whatever else is going on: the
///   microphone is off as the key comes up, not when an earlier press's words have gone where
///   they're for, or what's said after the key would be heard as part of it;
/// - what was said goes where it's for after the presses before it, on the one thread that
///   does that, which waits for each one's helper in the order their keys came up. So two said
///   one after the other arrive in the order they were said.
///
/// A press is out of `LIVE` by now, so the next can begin at once.
fn finish_in_turn(app: &AppHandle, over: Option<Live>) {
    static TURNS: std::sync::OnceLock<Mutex<std::sync::mpsc::Sender<Over>>> = std::sync::OnceLock::new();
    let Some(mut over) = over else { return };
    show(app, &Talk { id: over.id, phase: "settling", to: word(over.to), text: lock(&over.heard).text(), ..Talk::default() });
    let (stopped, when_stopped) = std::sync::mpsc::channel();
    let ear = over.ear.take();
    std::thread::spawn(move || {
        if let Some(ear) = ear {
            ear.stop();
        }
        let _ = stopped.send(());
    });
    let turns = TURNS.get_or_init(|| {
        let (tx, rx) = std::sync::mpsc::channel::<Over>();
        let started = std::thread::Builder::new().name("codealong-dictation".into()).spawn(move || {
            for (app, over, when_stopped) in rx {
                // Its last words are in once its helper has stopped, or is gone.
                let _ = when_stopped.recv();
                if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| deliver(&app, over))).is_err() {
                    eprintln!("Voice: putting what was said where it's for panicked");
                }
            }
        });
        if let Err(e) = started {
            eprintln!("Voice: the thread that puts what's said where it's for didn't start: {e}");
        }
        Mutex::new(tx)
    });
    // Only a thread that didn't start leaves nobody to take it; then it's done here.
    if let Err(unsent) = lock(turns).send((app.clone(), over, when_stopped)) {
        let (app, over, when_stopped) = unsent.0;
        let _ = when_stopped.recv();
        deliver(&app, over);
    }
}

/// Puts what a press heard where it's for, its helper stopped and its last words in. Off the
/// main thread, one press at a time (`finish_in_turn`).
fn deliver(app: &AppHandle, l: Live) {
    let id = l.id;
    let to = word(l.to);
    let (text, failed, waiting) = {
        let heard = lock(&l.heard);
        (heard.text(), heard.failed.clone(), heard.waiting.is_some())
    };
    if text.is_empty() {
        let talk = match failed {
            Some(refused) => Talk { id, phase: "failed", to, message: Some(crate::listen::refusal(refused).to_string()), ..Talk::default() },
            // The model wasn't there to hear it with: what was said is lost, and saying why
            // beats saying nothing was heard.
            None if waiting => Talk { id, phase: "failed", to, message: Some("macOS is still downloading the speech model. Try again in a minute.".into()), ..Talk::default() },
            None => Talk { id, phase: "nothing", to, message: Some("Nothing heard. Hold the keys down while you talk.".into()), ..Talk::default() },
        };
        show(app, &talk);
        return;
    }
    let done = match l.to {
        To::Looking => to_window(app, l.window.as_deref(), &text),
        To::Orchestrator => to_orchestrator(app, &text, l.shift),
    };
    show(app, &Talk { id, to, text, ..done });
}

/// Hands what was said to the window that had the keyboard, or to the quick panel where no
/// window of Codealong's did.
fn to_window(app: &AppHandle, window: Option<&str>, text: &str) -> Talk {
    match window.filter(|label| app.get_webview_window(label).is_some()) {
        Some(label) => {
            let _ = app.emit_to(label, "talked", Talked { text: text.to_string(), key: None });
            Talk { phase: "typed", ..Talk::default() }
        }
        None => to_quick(app, text, None),
    }
}

/// Opens the quick panel on what was said: it finds the session it's for, or starts one.
fn to_quick(app: &AppHandle, text: &str, message: Option<String>) -> Talk {
    let (handle, prompt) = (app.clone(), text.to_string());
    let _ = app.run_on_main_thread(move || crate::quick::open(&handle, json!({ "prompt": prompt })));
    Talk { phase: "asked", message, ..Talk::default() }
}

/// What's sent unread: what was said, and that it was dictated.
fn dictated(text: &str) -> String {
    format!("{}\n\n{DICTATED}", text.trim())
}

/// Gives what was said to the orchestrator: sent, or put in its box with the main window
/// brought forward on it.
fn to_orchestrator(app: &AppHandle, text: &str, shift: bool) -> Talk {
    let found: Result<Option<(String, String)>, String> = crate::view::call_as("orchestrator", json!({}));
    let (key, title) = match found {
        Ok(Some(found)) => found,
        Ok(None) => return to_quick(app, text, Some("No orchestrator is running, so the quick panel has it. /codealong:orchestrate starts one.".into())),
        Err(e) => return to_quick(app, text, Some(e)),
    };
    let send = shift || crate::config::current().is_ok_and(|c| c.voice.send_on_release);
    if send {
        return match crate::view::call("send_to_session", json!({ "key": key, "text": dictated(text), "now": false })) {
            Ok(_) => Talk { phase: "sent", title: Some(title), ..Talk::default() },
            // Not lost: it's in its box, to send again.
            Err(e) => {
                reveal(app, &key, text);
                Talk { phase: "typed", title: Some(title), message: Some(e), ..Talk::default() }
            }
        };
    }
    reveal(app, &key, text);
    Talk { phase: "typed", title: Some(title), ..Talk::default() }
}

/// Puts `text` in a session's box, and brings the main window forward on the session.
fn reveal(app: &AppHandle, key: &str, text: &str) {
    let lane = app.try_state::<crate::view::Mirror>().and_then(|m| m.read(|b| b.session(key).map(|(lane, _)| lane.id.clone())));
    let (handle, key, text) = (app.clone(), key.to_string(), text.to_string());
    let _ = app.run_on_main_thread(move || {
        let reveal = lane.map(|lane| json!({ "kind": "session", "laneId": lane, "key": key }));
        if let Err(e) = crate::front::show_main(&handle, reveal) {
            eprintln!("Couldn't bring the main window forward for what was said: {e:#}");
        }
        let _ = handle.emit_to("main", "talked", Talked { text, key: Some(key) });
    });
}

/// Loads the speech model, so the first words of the first press aren't kept waiting on it. Off
/// the main thread, at launch and when voice is turned on.
pub fn warm() {
    let Ok(cfg) = crate::config::current() else { return };
    if !cfg.experimental.voice {
        return;
    }
    if let Err(refused) = ear::warm(&cfg.voice.locale) {
        // Said again, to the person, when they press a key.
        eprintln!("Voice: {}", refused.message);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_was_said_is_the_settled_stretches_and_the_one_still_heard() {
        assert_eq!(words(&["Hello, orchestrator.".into(), " Please check what waits on me. ".into()], "and tell"), "Hello, orchestrator. Please check what waits on me. and tell");
        assert_eq!(words(&[], "  "), "");
        assert_eq!(words(&["One.".into()], ""), "One.");
    }

    #[test]
    fn what_goes_unread_says_it_was_dictated() {
        assert_eq!(dictated(" Rebase the release branch. "), "Rebase the release branch.\n\n(Dictated, and sent as I let go of the key: some words may be misheard.)");
    }

    #[test]
    fn the_notch_is_told_in_its_own_words() {
        let talk = Talk { id: 3, phase: "sent", to: word(To::Orchestrator), title: Some("Orchestrator".into()), text: "Hi.".into(), level: 0.25, message: None };
        assert_eq!(serde_json::to_value(talk).unwrap(), json!({ "id": 3, "phase": "sent", "to": "orchestrator", "title": "Orchestrator", "text": "Hi.", "level": 0.25 }));
        assert_eq!(word(To::Looking), "looking");
    }
}
