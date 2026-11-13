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
//!   that into the box of the session it shows (src/talk.ts). In another app the quick panel
//!   opens as the key goes down, to hear it: what you say goes into its box, and it finds the
//!   session it's for or starts one. Nothing is sent: what a machine heard is yours to read
//!   over first, unless Shift is down as you let go into the quick panel, which starts a
//!   session with it at once (as ⌘⏎ would), with a line saying it was dictated. The quick
//!   panel's own microphone button is the same press and release (`talk`, lib.rs), without the
//!   key held, and so is a session's box's microphone (src/ui/SessionReply.tsx `TalkButton`).
//! - `talk_orchestrator`: to the session that runs as your orchestrator (board.rs
//!   `orchestrator`), whatever you're looking at. Into its box, the main window brought forward on
//!   it; or, with `[voice] send_on_release` or Shift held as you let go, sent, with a line saying
//!   it was dictated, since nobody read it over.
//!
//! What you say is yours, as what you type is: it goes the way a window's box sends
//! (`send_to_session`). That's why only the microphone is ever heard here. What a meeting says is
//! other people's words, and listen.rs keeps those apart.
//!
//! What's heard shows as it goes (the `talk` event): on the bar (the notch's card or the floating
//! bar's line, src/ui/Said.tsx), with to whom and where it went; in the quick panel's box, when
//! that's where the words go (src/ui/Quick.tsx); and, where there's no bar to show it (it's in
//! the menu bar, or off), at the foot of the window the words are for (src/talk.ts `hear`). So
//! there's something to see whatever the bar is set to (`show`). That window is told whatever
//! the bar does, so a session's box whose words they are shows its microphone listening.

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
    /// Told to the window the words are for, where there's no bar to show them: it says them at
    /// its foot instead (src/talk.ts `hear`).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub stands_in: bool,
}

/// What a window is handed to put where its caret is (the `talked` event).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct Talked {
    /// The press it was said in (`Talk::id`): the quick panel puts it where that press was
    /// heard, and starts nothing for a press it didn't show.
    id: u64,
    text: String,
    /// The session whose box it's for, when it's for one whatever the window shows.
    #[serde(skip_serializing_if = "Option::is_none")]
    key: Option<String>,
    /// For the quick panel: Shift was down as the key came up, so it starts a session with what
    /// was said at once, as ⌘⏎ would. The text says it was dictated (`dictated`).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    start: bool,
}

/// Where what was said to whatever you're looking at goes (`To::Looking`).
#[derive(Clone, Debug, PartialEq)]
enum Lands {
    /// Handed to a window, to put where its caret is.
    Window(Talked),
    /// The quick panel is opened on it: no window of Codealong's had the keyboard, or the quick
    /// panel did and was put away before the key came up.
    Quick,
}

/// Where what press `id` heard (`text`) goes: to the window that had the keyboard as the key
/// went down (`window`, by its label, when it's still there), which for the quick panel is only
/// while it's up (`quick_up`), and which only the quick panel takes as something to start
/// (`shift`).
fn lands(id: u64, window: Option<&str>, quick_up: bool, shift: bool, text: &str) -> Lands {
    match window {
        Some(label) if label == crate::quick::LABEL && !quick_up => Lands::Quick,
        Some(label) if label == crate::quick::LABEL && shift => Lands::Window(Talked { id, text: dictated(text), key: None, start: true }),
        Some(_) => Lands::Window(Talked { id, text: text.to_string(), key: None, start: false }),
        None => Lands::Quick,
    }
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
    /// It was dropped (`drop_press`): what it heard goes nowhere.
    dropped: bool,
}

static LIVE: Mutex<Option<Live>> = Mutex::new(None);
static NEXT: AtomicU64 = AtomicU64::new(1);
/// The presses dropped once their keys had come up (`drop_press`), by id: each is put nowhere
/// when its turn to be delivered comes (`was_dropped`).
static DROPPED: Mutex<Vec<u64>> = Mutex::new(Vec::new());

fn live() -> MutexGuard<'static, Option<Live>> {
    LIVE.lock().unwrap_or_else(|e| e.into_inner())
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Says what's heard of a press: to the bar, and to the window its words are for (`window`,
/// the one that had the keyboard as the key went down, or the quick panel opened to hear it),
/// as `told` has it.
fn show(app: &AppHandle, window: Option<&str>, talk: &Talk) {
    let _ = app.emit_to(crate::bar::LABEL, "talk", talk);
    if let Some((label, stands_in)) = told(window, crate::bar::mode_of(app).has_window()) {
        let _ = app.emit_to(label, "talk", Talk { stands_in, ..talk.clone() });
    }
}

/// The window, beside the bar, told what's heard of a press whose words are for `window`, and
/// whether it says it at its foot, standing in for the bar where there's none (`bar_up`: it
/// floats or is docked, not in the menu bar or off; src/talk.ts `hear`). Told whatever the bar
/// does: the quick panel shows the words in its box as they're heard, and a session's box its
/// microphone listening (src/ui/SessionReply.tsx `TalkButton`).
fn told(window: Option<&str>, bar_up: bool) -> Option<(&str, bool)> {
    window.filter(|label| *label != crate::bar::LABEL).map(|label| (label, !bar_up))
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
    if live().is_some() {
        // Down again without having come up: its release was lost, and this is it.
        release(app);
        return;
    }
    // With no window of Codealong's to type into (you're in another app), the quick panel is
    // opened to hear it, and is where the words go: there as you say them, whatever the bar
    // shows or doesn't, where it would otherwise open only once the key came up.
    let window = match (key_window(app), to) {
        (None, To::Looking) => {
            crate::quick::open(app, json!({ "talk": true }));
            Some(crate::quick::LABEL.to_string())
        }
        (window, _) => window,
    };
    let id = NEXT.fetch_add(1, Ordering::SeqCst);
    // On the main thread, as every press is: none has begun since the look above.
    *live() = Some(Live { id, to, window: window.clone(), ear: None, heard: Arc::default(), released: false, shift: false, dropped: false });
    show(app, window.as_deref(), &Talk { id, phase: "listening", to: word(to), ..Talk::default() });
    let app = app.clone();
    std::thread::spawn(move || begin(&app, id, to));
}

/// Starts the helper for press `id`, off the main thread: it waits on the person where macOS has
/// yet to ask them for the microphone.
fn begin(app: &AppHandle, id: u64, to: To) {
    let cfg = crate::config::current().unwrap_or_default();
    let (heard, window) = match live().as_ref().filter(|l| l.id == id) {
        Some(l) => (l.heard.clone(), l.window.clone()),
        None => return,
    };
    let spec = Spec { sources: vec![Source { name: crate::listen::YOU.into(), from: ear::From::Mic { cancel_echo: false } }], locale: cfg.voice.locale.clone(), hints: hints(&cfg) };
    let (shows, shown_in, into) = (app.clone(), window.clone(), heard.clone());
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
        show(&shows, shown_in.as_deref(), &talk);
    });
    let ear = match started {
        Ok(ear) => ear,
        Err(refused) => {
            let mut live = live();
            if live.as_ref().is_some_and(|l| l.id == id) {
                *live = None;
            }
            drop(live);
            show(app, window.as_deref(), &Talk { id, phase: "failed", to: word(to), message: Some(crate::listen::refusal(refused).to_string()), ..Talk::default() });
            return;
        }
    };
    {
        let mut live = live();
        match live.as_mut().filter(|l| l.id == id) {
            Some(l) => {
                l.ear = Some(ear);
                if l.released {
                    over(app, &mut live);
                    return;
                }
            }
            // Gone meanwhile: the helper goes with the ear.
            None => return,
        }
    }
    // A key that never comes up is let go of for it. This thread goes as soon as the key does.
    let until = Instant::now() + LONGEST;
    loop {
        {
            let mut live = live();
            if !live.as_ref().is_some_and(|l| l.id == id) {
                return;
            }
            if Instant::now() >= until {
                over(app, &mut live);
                return;
            }
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

/// The press in `LIVE` is over: out of it, so the next can begin, and into the line of presses
/// to finish (`finish_in_turn`), both under `LIVE`'s lock. A next press can only begin, and so
/// come up, once this lock is let go, by when this one has its place in the line ahead of it.
fn over(app: &AppHandle, live: &mut MutexGuard<'static, Option<Live>>) {
    if let Some(over) = live.take() {
        finish_in_turn(app, over);
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
    let mut live = live();
    let Some(l) = live.as_mut() else { return };
    l.shift = crate::os::windows::shift_down();
    if l.ear.is_none() {
        // Still starting: `begin` finishes it once it's capturing.
        l.released = true;
        return;
    }
    over(app, &mut live);
}

/// Drops press `id`: what it heard is put nowhere, and if it's still going its microphone is
/// off as when its key comes up. The quick panel asks it of the press it was hearing as it's
/// put away (Escape, a click elsewhere): a panel that's gone shouldn't come back with what was
/// being said to it, nor leave a press begun with its microphone listening on. By id, since the
/// press that's going by then may be another (one for your orchestrator, begun since), and the
/// one meant may have had its key come up and be waiting its turn (`finish_in_turn`), where it's
/// noted for `deliver` to find. On the main thread.
pub fn drop_press(app: &AppHandle, id: u64) {
    let mut live = live();
    let Some(l) = live.as_mut().filter(|l| l.id == id) else {
        lock(&DROPPED).push(id);
        return;
    };
    l.dropped = true;
    if l.ear.is_none() {
        // Still starting: `begin` finishes it once it's capturing.
        l.released = true;
        return;
    }
    over(app, &mut live);
}

/// Whether press `id`, whose turn to be delivered has come, was dropped after its key came up
/// (`noted`, the ids `drop_press` kept). Presses are delivered in the order they began, so this
/// one's note and any older are done with.
fn was_dropped(noted: &mut Vec<u64>, id: u64) -> bool {
    let was = noted.contains(&id);
    noted.retain(|other| *other > id);
    was
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
/// Called with `LIVE`'s lock held (`over`), which is what gives the presses their order here.
/// Nothing in it waits: it starts a thread and hands the press on.
fn finish_in_turn(app: &AppHandle, mut over: Live) {
    static TURNS: std::sync::OnceLock<Mutex<std::sync::mpsc::Sender<Over>>> = std::sync::OnceLock::new();
    let (stopped, when_stopped) = std::sync::mpsc::channel();
    let ear = over.ear.take();
    // Said from the thread that stops it, not from here: nothing that might wait on the main
    // thread is done under the lock the main thread takes as a key goes down.
    let settling = (app.clone(), over.window.clone(), Talk { id: over.id, phase: "settling", to: word(over.to), text: lock(&over.heard).text(), ..Talk::default() });
    std::thread::spawn(move || {
        show(&settling.0, settling.1.as_deref(), &settling.2);
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
    // Only a thread that didn't start leaves nobody to take it; then it's done on one of its
    // own, since whoever called holds a lock.
    if let Err(unsent) = lock(turns).send((app.clone(), over, when_stopped)) {
        let (app, over, when_stopped) = unsent.0;
        std::thread::spawn(move || {
            let _ = when_stopped.recv();
            deliver(&app, over);
        });
    }
}

/// Puts what a press heard where it's for, its helper stopped and its last words in. Off the
/// main thread, one press at a time (`finish_in_turn`).
fn deliver(app: &AppHandle, l: Live) {
    let id = l.id;
    let to = word(l.to);
    if was_dropped(&mut lock(&DROPPED), id) || l.dropped {
        // Over, with nothing to say of it: whoever dropped it knows.
        show(app, l.window.as_deref(), &Talk { id, phase: "nothing", to, ..Talk::default() });
        return;
    }
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
        show(app, l.window.as_deref(), &talk);
        return;
    }
    let done = match l.to {
        To::Looking => to_window(app, id, l.window.as_deref(), &text, l.shift),
        To::Orchestrator => to_orchestrator(app, &text, l.shift),
    };
    show(app, l.window.as_deref(), &Talk { id, to, text, ..done });
}

/// Hands what was said to the window that had the keyboard, or to the quick panel where no
/// window of Codealong's did (`lands`). The quick panel starts a session with it when Shift
/// was down as the key came up.
fn to_window(app: &AppHandle, id: u64, window: Option<&str>, text: &str, shift: bool) -> Talk {
    let window = window.filter(|label| app.get_webview_window(label).is_some());
    match lands(id, window, quick_up(app), shift, text) {
        Lands::Window(talked) => {
            let phase = match (talked.start, window == Some(crate::quick::LABEL)) {
                (true, _) => "started",
                (false, true) => "asked",
                (false, false) => "typed",
            };
            if let Some(label) = window {
                let _ = app.emit_to(label, "talked", talked);
            }
            Talk { phase, ..Talk::default() }
        }
        Lands::Quick => to_quick(app, text, None),
    }
}

/// Whether the quick panel is up, asked of the main thread, which is where its window is looked
/// at. Not, when the main thread doesn't say in time.
fn quick_up(app: &AppHandle) -> bool {
    let (said, heard) = std::sync::mpsc::channel();
    let handle = app.clone();
    let asked = app.run_on_main_thread(move || {
        let _ = said.send(crate::quick::is_up(&handle));
    });
    asked.is_ok() && heard.recv_timeout(Duration::from_secs(2)).unwrap_or(false)
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
        let _ = handle.emit_to("main", "talked", Talked { id: 0, text, key: Some(key), start: false });
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
    fn what_was_said_goes_to_the_window_that_had_the_keyboard_or_the_quick_panel() {
        let typed = |text: &str| Lands::Window(Talked { id: 4, text: text.into(), key: None, start: false });
        // A window of Codealong's had the keyboard: it types it where its caret is, Shift or not.
        assert_eq!(lands(4, Some("main"), false, false, "Fix the login redirect."), typed("Fix the login redirect."));
        assert_eq!(lands(4, Some("main"), true, true, "Fix the login redirect."), typed("Fix the login redirect."));
        // None did (another app was in front, and the key's own open of the panel didn't come
        // up): the quick panel is opened on it.
        assert_eq!(lands(4, None, false, false, "Fix it."), Lands::Quick);
        // The quick panel had it, opened to hear this or open already: into its box.
        assert_eq!(lands(4, Some(crate::quick::LABEL), true, false, "Fix it."), typed("Fix it."));
        // With Shift down as the key came up, it starts a session with it, said to be dictated.
        assert_eq!(
            lands(4, Some(crate::quick::LABEL), true, true, "Fix it."),
            Lands::Window(Talked { id: 4, text: "Fix it.\n\n(Dictated, and sent as I let go of the key: some words may be misheard.)".into(), key: None, start: true })
        );
        // Put away before the key came up: it's opened again on what was said, nothing started.
        assert_eq!(lands(4, Some(crate::quick::LABEL), false, true, "Fix it."), Lands::Quick);
        // As a window reads it: the press it was said in, and nothing of starting unless it's to.
        assert_eq!(serde_json::to_value(Talked { id: 4, text: "Hi.".into(), key: None, start: false }).unwrap(), json!({ "id": 4, "text": "Hi." }));
        assert_eq!(serde_json::to_value(Talked { id: 4, text: "Hi.".into(), key: Some("k".into()), start: true }).unwrap(), json!({ "id": 4, "text": "Hi.", "key": "k", "start": true }));
    }

    #[test]
    fn the_window_the_words_are_for_is_told_and_says_them_only_for_want_of_a_bar() {
        // The quick panel is told, bar or no bar: it shows the words in its box.
        assert_eq!(told(Some(crate::quick::LABEL), true), Some((crate::quick::LABEL, false)));
        assert_eq!(told(Some(crate::quick::LABEL), false), Some((crate::quick::LABEL, true)));
        // Another window is told too, for its box's microphone, and says what's heard at its foot
        // only where there's no bar: in the menu bar, or off.
        assert_eq!(told(Some("main"), true), Some(("main", false)));
        assert_eq!(told(Some("main"), false), Some(("main", true)));
        // The bar is told as the bar, once; and with no window the words are for, nobody else is.
        assert_eq!(told(Some(crate::bar::LABEL), false), None);
        assert_eq!(told(None, false), None);
        // As a window standing in is told it; the bar is told nothing of it (`the_notch_is_told_in_its_own_words`).
        assert_eq!(serde_json::to_value(Talk { stands_in: true, ..Talk::default() }).unwrap()["standsIn"], json!(true));
    }

    #[test]
    fn a_press_dropped_after_its_key_came_up_is_put_nowhere_when_its_turn_comes() {
        // Noted while it waited its turn: found, once, and older notes go with it.
        let mut noted = vec![3, 5];
        assert!(!was_dropped(&mut noted, 4));
        assert_eq!(noted, [5]);
        assert!(was_dropped(&mut noted, 5));
        assert!(noted.is_empty());
        // One that wasn't, and a note for a press delivered long ago, which goes at the next.
        assert!(!was_dropped(&mut noted, 6));
        noted.push(2);
        assert!(!was_dropped(&mut noted, 7));
        assert!(noted.is_empty());
    }

    #[test]
    fn what_goes_unread_says_it_was_dictated() {
        assert_eq!(dictated(" Rebase the release branch. "), "Rebase the release branch.\n\n(Dictated, and sent as I let go of the key: some words may be misheard.)");
    }

    #[test]
    fn the_notch_is_told_in_its_own_words() {
        let talk = Talk { id: 3, phase: "sent", to: word(To::Orchestrator), title: Some("Orchestrator".into()), text: "Hi.".into(), level: 0.25, message: None, stands_in: false };
        assert_eq!(serde_json::to_value(talk).unwrap(), json!({ "id": 3, "phase": "sent", "to": "orchestrator", "title": "Orchestrator", "text": "Hi.", "level": 0.25 }));
        assert_eq!(word(To::Looking), "looking");
    }
}
