//! The quick panel: one input, and what you can do next below it. With nothing typed it lists
//! the sessions that need you; typing finds sessions; its last row starts a new one with what
//! you typed as the prompt. A shortcut that works in every app (`[shortcuts] quick_panel` in
//! config.toml) opens it, and so do ⌘N and New session in the board's windows, at what's
//! selected there; `[shortcuts] chat` opens it set to start a chat in no project. Answering what
//! waits on you, one after another, is the waiting window's (waiting.rs), which another shortcut
//! (`[shortcuts] waiting`) brings up, unless the bar is docked in the notch: then the notch does
//! that (`work_through`). Every shortcut that works in any app is kept here (`Keys`), whatever it
//! opens.
//!
//! Like the floating bar it's a non-activating panel: macOS shows only those on another app's
//! full-screen Space, and opening one leaves the app you're in active, with Codealong's windows
//! where they were. Unlike the bar it takes the keyboard. It hides when it loses the keyboard,
//! as Spotlight does.
//!
//! Opening is one script. Codealong runs `window.__quickOpen` in the page with what it opens for
//! and the board; the page lays itself out and returns its height before the script ends
//! (Quick.tsx); only then does the panel show, at that height, so it never flashes what it
//! showed last time. It all happens in the script because WebKit runs a hidden page at
//! background priority except while a script from the app runs in it: when the page read the
//! board itself and drew after the script, a busy Mac held an open back for seconds (2.5 to 25 s
//! at a load of 300, in a WKWebView set up like this one).

use std::str::FromStr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, Window, WindowEvent};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut};

use crate::geometry::{screen_holding, Rect, Screen};
use crate::start::Place;
use crate::{engine, panel, perf};

pub const LABEL: &str = "quick";
/// The window's width: the page's card and the room its shadow needs on each side.
const WIDTH: f64 = 760.0;
/// Its height before the page has measured itself.
const START_HEIGHT: f64 = 240.0;
/// Where its top edge sits: this far down the usable part of the screen, as Spotlight's does.
const TOP: f64 = 0.18;
/// How long an open waits for the page before showing anyway.
const READY_WAIT: Duration = Duration::from_millis(600);
/// The panel takes the keyboard as it shows and says truthfully whether it has it. It's above
/// other apps' windows and the bar, floating or docked in the notch, on whichever Space is in
/// front, full-screen ones too, coming and going. Unlike the others it allows no tooltips while
/// Codealong is in the background: it never has.
const PANEL: panel::Config = panel::Config { keys: panel::Keys::Always, level: panel::Level::Quick, spaces: panel::Spaces::Transient, unconstrained: false, tooltips: false, steps: panel::Steps::One };

#[derive(Default)]
struct Quick {
    /// Where the panel's left and top edges are while it's up, in AppKit points (y grows up). It
    /// grows and shrinks downward from the top as the list changes.
    anchor: Mutex<Option<(f64, f64)>>,
    /// Counts opens, so a late answer from the page only shows the panel for the latest one.
    opens: Mutex<u64>,
    /// The open the page hasn't answered yet.
    waiting: Mutex<Option<u64>>,
    shortcut: Mutex<ShortcutState>,
    chat_shortcut: Mutex<ShortcutState>,
    waiting_shortcut: Mutex<ShortcutState>,
    improve_shortcut: Mutex<ShortcutState>,
    main_shortcut: Mutex<ShortcutState>,
}

/// Codealong's shortcuts that work in every app: the quick panel's, the one that opens it set to a
/// chat in no project, the one that goes through what waits on you, Improve Codealong's
/// (improve.rs), and the one that brings the board's window forward (front.rs `toggle_board`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Keys {
    Quick,
    Chat,
    Waiting,
    Improve,
    Main,
}

impl Keys {
    pub const ALL: [Keys; 5] = [Keys::Quick, Keys::Chat, Keys::Waiting, Keys::Improve, Keys::Main];

    /// Its shortcut as `[shortcuts]` in config.toml has it.
    pub fn in_config(self, cfg: &crate::config::ShortcutsCfg) -> &str {
        match self {
            Keys::Quick => &cfg.quick_panel,
            Keys::Chat => &cfg.chat,
            Keys::Waiting => &cfg.waiting,
            Keys::Improve => &cfg.improve,
            Keys::Main => &cfg.main_window,
        }
    }

    /// Its shortcut as the board has it, which follows config.toml as it's edited.
    pub fn on_board(self, board: &crate::model::Board) -> &str {
        match self {
            Keys::Quick => &board.quick_shortcut,
            Keys::Chat => &board.chat_shortcut,
            Keys::Waiting => &board.waiting_shortcut,
            Keys::Improve => &board.improve_shortcut,
            Keys::Main => &board.main_window_shortcut,
        }
    }
}

impl Quick {
    fn slot(&self, which: Keys) -> &Mutex<ShortcutState> {
        match which {
            Keys::Quick => &self.shortcut,
            Keys::Chat => &self.chat_shortcut,
            Keys::Waiting => &self.waiting_shortcut,
            Keys::Improve => &self.improve_shortcut,
            Keys::Main => &self.main_shortcut,
        }
    }
}

#[derive(Default)]
struct ShortcutState {
    /// As config.toml has it; None until it's first read.
    wanted: Option<String>,
    registered: Option<Shortcut>,
    error: Option<String>,
    /// Off while Settings records a new one, so pressing the current one records it.
    paused: bool,
}

/// A shortcut as Settings shows it.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutStatus {
    pub which: Keys,
    /// As config.toml has it; empty when it's off.
    pub shortcut: String,
    /// Why it isn't working, when it isn't.
    pub error: Option<String>,
}

/// Makes the panel's window, hidden. It's made at launch so that opening never waits for a page
/// to load.
pub fn setup(app: &AppHandle) -> Result<()> {
    app.manage(Quick::default());
    let page = panel::Page { title: "Codealong", script: "window.__CODEALONG_QUICK__ = true;", size: (WIDTH, START_HEIGHT), focusable: true, first_mouse: true };
    let window = panel::builder(app, LABEL, page).build()?;
    // The panel is never destroyed, so the class it had isn't kept.
    let _ = panel::make(&window, PANEL);
    Ok(())
}

/// Opens the panel, or brings it back up, for `context`: what a board window asks it to start
/// in (`laneId`, `project`, `todo`, `prompt`, `opener`), or nothing for the shortcut. On the main
/// thread.
pub fn open(app: &AppHandle, context: Value) {
    let (Some(state), Some(window)) = (app.try_state::<Quick>(), app.get_webview_window(LABEL)) else { return };
    let id = {
        let Ok(mut opens) = state.opens.lock() else { return };
        *opens += 1;
        *opens
    };
    let t = Instant::now();
    // It opens where you are: on the screen with the pointer, centered, near the top.
    if !panel::is_visible(&window) {
        if let Some(s) = pointer_screen() {
            let anchor = anchor_on(s.visible);
            if let Ok(mut a) = state.anchor.lock() {
                *a = Some(anchor);
            }
        }
    }
    if let Ok(mut w) = state.waiting.lock() {
        *w = Some(id);
    }
    let board = app.try_state::<engine::Handle>().and_then(|e| e.read(|b| serde_json::to_string(b).ok())).unwrap_or_else(|| "null".into());
    if perf::enabled() {
        perf::log(&format!("quick: open {id}, the board's {} KB written in {} ms", board.len() / 1024, t.elapsed().as_millis()));
    }
    let handle = app.clone();
    let asked = window.eval_with_callback(open_script(&with_room(context, room(&state)), &board), move |answer| {
        let height = answer.trim().parse::<f64>().ok().filter(|h| h.is_finite() && *h > 0.0);
        let app = handle.clone();
        on_main(&handle, move || answered(&app, id, height, t));
    });
    if let Err(e) = asked {
        eprintln!("Couldn't open the quick panel's page: {e}");
    }
    // A page that doesn't answer (still loading, say) doesn't keep the panel from showing.
    let handle = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(READY_WAIT);
        let app = handle.clone();
        let _ = handle.run_on_main_thread(move || {
            let waiting = app.try_state::<Quick>().and_then(|s| s.waiting.lock().ok().map(|w| *w));
            if waiting == Some(Some(id)) {
                if perf::enabled() {
                    perf::log(&format!("quick: open {id} had no answer after {} ms; shown as it was", t.elapsed().as_millis()));
                }
                ready(&app, None);
            }
        });
    });
}

/// The script that opens the page: `__quickOpen` with what for and the board, which returns the
/// page's height once it's laid out. A page that isn't up yet keeps both for when it is, and
/// sends its height then (`quick_ready`).
fn open_script(context: &Value, board: &str) -> String {
    format!(
        "(function (context, board) {{ \
           if (!window.__quickOpen) {{ window.__quickPending = [context, board]; return null; }} \
           try {{ var h = window.__quickOpen(context, board); return typeof h === 'number' && isFinite(h) ? h : null; }} catch (e) {{ return null; }} \
         }})({context}, {board})"
    )
}

/// The page answered the script that opened it: with its height, or with nothing when it wasn't
/// up yet or failed, and then it sends its height itself or the wait runs out. On the main thread.
fn answered(app: &AppHandle, id: u64, height: Option<f64>, since: Instant) {
    let Some(state) = app.try_state::<Quick>() else { return };
    if perf::enabled() {
        let tall = height.map_or_else(|| "no height".to_string(), |h| format!("{h} px"));
        perf::log(&format!("quick: open {id} answered in {} ms ({tall})", since.elapsed().as_millis()));
    }
    let latest = state.waiting.lock().ok().and_then(|w| *w) == Some(id);
    // A later open has the page now, or the panel was closed meanwhile.
    if latest && height.is_some() {
        ready(app, height);
    }
}

/// Runs `f` on the main thread: right away when this is it.
fn on_main(app: &AppHandle, f: impl FnOnce() + Send + 'static) {
    if panel::on_main_thread() {
        f();
    } else {
        let _ = app.run_on_main_thread(f);
    }
}

/// The screen the pointer is on, else the first. On the main thread.
fn pointer_screen() -> Option<Screen> {
    let p = panel::pointer();
    let screens = panel::screens();
    p.and_then(|p| screen_holding(p, &screens)).or(screens.first().copied())
}

/// Where a panel opened on this screen sits: centered, its top edge `TOP` of the way down.
fn anchor_on(visible: Rect) -> (f64, f64) {
    let x = visible.x + ((visible.w - WIDTH) / 2.0).max(0.0);
    let top = visible.y + visible.h - (visible.h * TOP).round();
    (x.round(), top)
}

/// The page is laid out for the latest open and measured itself: show the panel at that height
/// and give it the keyboard. On the main thread.
pub fn ready(app: &AppHandle, height: Option<f64>) {
    let (Some(state), Some(window)) = (app.try_state::<Quick>(), app.get_webview_window(LABEL)) else { return };
    if let Ok(mut w) = state.waiting.lock() {
        if w.is_none() {
            // Not opening: the page just changed size.
            drop(w);
            if let Some(h) = height {
                fit(app, h);
            }
            return;
        }
        *w = None;
    }
    let height = height.or_else(|| panel::frame(&window).map(|f| f.h)).unwrap_or(START_HEIGHT);
    if let Some(frame) = frame_for(&state, height) {
        panel::set_frame(&window, frame);
    }
    // It takes the keyboard as it shows, and its webview the keys, so what the page focused
    // gets them.
    panel::present(&window, true);
}

/// How tall the panel can be where it's anchored: from its top edge down to the bottom of the
/// screen's usable part.
fn room(state: &Quick) -> Option<f64> {
    let (x, top) = (*state.anchor.lock().ok()?)?;
    let screen = screen_holding((x + WIDTH / 2.0, top - 1.0), &panel::screens())?;
    Some(top - screen.visible.y)
}

/// What an open tells the page, with the room it has. The page keeps its card within it and
/// scrolls its list, so the panel's bottom, where a new session starts, stays on a short screen.
fn with_room(context: Value, room: Option<f64>) -> Value {
    let Some(room) = room else { return context };
    let mut fields = match context {
        Value::Object(fields) => fields,
        _ => serde_json::Map::new(),
    };
    fields.insert("room".into(), room.into());
    Value::Object(fields)
}

/// The panel's frame at a height, from its anchor, kept on the screen.
fn frame_for(state: &Quick, height: f64) -> Option<Rect> {
    let (x, top) = (*state.anchor.lock().ok()?)?;
    let height = room(state).map_or(height, |r| height.min(r)).max(1.0);
    Some(Rect { x, y: top - height, w: WIDTH, h: height })
}

/// The page changed size while up: the panel follows, its top edge where it was. On the main
/// thread.
pub fn fit(app: &AppHandle, height: f64) {
    let (Some(state), Some(window)) = (app.try_state::<Quick>(), app.get_webview_window(LABEL)) else { return };
    if !panel::is_visible(&window) {
        return;
    }
    if let Some(frame) = frame_for(&state, height.ceil()) {
        if panel::frame(&window) != Some(frame) {
            panel::set_frame(&window, frame);
        }
    }
}

/// Hides the panel. What you typed stays for next time. On the main thread.
pub fn hide(app: &AppHandle) {
    if let Some(state) = app.try_state::<Quick>() {
        if let Ok(mut w) = state.waiting.lock() {
            *w = None;
        }
    }
    if let Some(window) = app.get_webview_window(LABEL) {
        if panel::is_visible(&window) {
            panel::hide(&window);
            let _ = app.emit_to(LABEL, "quick-hidden", ());
        }
    }
}

/// The shortcut: opens the panel, or closes it when it's up. On the main thread.
pub fn toggle(app: &AppHandle) {
    toggle_for(app, Value::Null);
}

/// The chat shortcut: opens the panel set to start a chat of its own, in no project, or closes
/// the panel when it's up. On the main thread.
pub fn toggle_chat(app: &AppHandle) {
    toggle_for(app, serde_json::json!({ "chat": true }));
}

/// Opens the panel for `context`, or hides it if it's up.
fn toggle_for(app: &AppHandle, context: Value) {
    match app.get_webview_window(LABEL) {
        Some(w) if panel::is_visible(&w) => hide(app),
        Some(_) => open(app, context),
        None => {}
    }
}

/// The shortcut that goes through what waits on you. Docked in the notch, the bar's page opens
/// the notch on the session that waits (or puts it away), as a click on the notch does; otherwise
/// the waiting window comes up (waiting.rs), or goes away when it's the window you're in. On the
/// main thread.
pub fn work_through(app: &AppHandle) {
    if crate::bar::mode_of(app) == crate::bar::Mode::Notch && app.get_webview_window(crate::bar::LABEL).is_some() {
        let _ = app.emit_to(crate::bar::LABEL, "waiting-shortcut", ());
        return;
    }
    crate::waiting::toggle(app);
}

/// One of Codealong's shortcuts was pressed: each opens what it's for, or puts it away when it's
/// up and in front. On the main thread.
pub fn pressed(app: &AppHandle, shortcut: &Shortcut) {
    let Some(state) = app.try_state::<Quick>() else { return };
    let is = |which: Keys| state.slot(which).lock().ok().is_some_and(|s| s.registered.as_ref() == Some(shortcut));
    if is(Keys::Waiting) {
        work_through(app);
    } else if is(Keys::Quick) {
        toggle(app);
    } else if is(Keys::Chat) {
        toggle_chat(app);
    } else if is(Keys::Improve) {
        crate::improve::toggle(app);
    } else if is(Keys::Main) {
        crate::front::toggle_board(app);
    }
}

/// Whether the panel is up.
pub fn is_up(app: &AppHandle) -> bool {
    app.get_webview_window(LABEL).is_some_and(|w| panel::is_visible(&w))
}

/// Window events for the panel: it hides when another window or app takes the keyboard, and
/// closing it (⌘W) only hides it. On the main thread.
pub fn on_window_event(window: &Window, event: &WindowEvent) {
    if window.label() != LABEL {
        return;
    }
    match event {
        WindowEvent::Focused(false) => hide(window.app_handle()),
        WindowEvent::CloseRequested { api, .. } => {
            api.prevent_close();
            hide(window.app_handle());
        }
        _ => {}
    }
}

/// The app a start lands in, by bundle ID, so it can be asked to come forward: a click in the
/// panel leaves Codealong inactive, and the app it started in may be behind the one you were in.
/// `terminal` is the terminal's profile in config.toml.
pub fn start_app(place: Place, terminal: &str) -> Option<&'static str> {
    match place {
        Place::Terminal => crate::launch::bundle_id(terminal),
        // Nothing opens: the session runs with no window.
        Place::Background => None,
        Place::ClaudeApp => Some(crate::claude::app::BUNDLE_ID),
        Place::CodexApp => Some(crate::codex::APP_BUNDLE_ID),
        // It runs here, and shows on the board.
        Place::Codealong => None,
    }
}

/// A shortcut as config.toml writes it (`Option+Cmd+/`), or None when it's off (empty).
/// It needs ⌃, ⌥ or ⌘ unless it's a function key, since the panel would otherwise take a key
/// you type; and ⌘ alone with a key is a menu shortcut in every app.
pub fn parse_shortcut(text: &str) -> std::result::Result<Option<Shortcut>, String> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    let shortcut = Shortcut::from_str(text).map_err(|_| format!("\"{text}\" isn't a shortcut Codealong can read. Write it like \"Option+Cmd+/\" or \"Cmd+Ctrl+K\"."))?;
    let function_key = matches!(
        shortcut.key,
        Code::F1 | Code::F2 | Code::F3 | Code::F4 | Code::F5 | Code::F6 | Code::F7 | Code::F8 | Code::F9 | Code::F10 | Code::F11 | Code::F12
            | Code::F13 | Code::F14 | Code::F15 | Code::F16 | Code::F17 | Code::F18 | Code::F19 | Code::F20
    );
    let mods = shortcut.mods;
    if !function_key && !mods.intersects(Modifiers::CONTROL | Modifiers::ALT | Modifiers::SUPER) {
        return Err(format!("\"{text}\" needs Control, Option or Command, or it would take a key you type."));
    }
    if mods.difference(Modifiers::SHIFT) == Modifiers::SUPER && shortcut.key == Code::Tab {
        return Err("macOS keeps Cmd+Tab for switching apps.".into());
    }
    if mods == Modifiers::SUPER && !matches!(shortcut.key, Code::Space) && !function_key {
        return Err(format!("\"{text}\" is a menu shortcut in most apps. Add Control or Option."));
    }
    Ok(Some(shortcut))
}

/// Registers a shortcut config.toml asks for, in place of the one before. Off a background
/// thread only: registering waits on the main thread.
pub fn apply_shortcut(app: &AppHandle, which: Keys, wanted: &str) {
    let Some(state) = app.try_state::<Quick>() else { return };
    let Ok(mut s) = state.slot(which).lock() else { return };
    if s.wanted.as_deref() == Some(wanted) {
        return;
    }
    s.wanted = Some(wanted.to_string());
    register(app, &mut s);
    drop(s);
    let _ = app.emit("quick-shortcut", status(app, which));
}

/// Turns the shortcuts off while Settings records a new one, so pressing any records it, and
/// back on after. Off a background thread only.
pub fn pause_shortcuts(app: &AppHandle, paused: bool) {
    let Some(state) = app.try_state::<Quick>() else { return };
    for which in Keys::ALL {
        if let Ok(mut s) = state.slot(which).lock() {
            if s.paused != paused {
                s.paused = paused;
                register(app, &mut s);
            }
        }
    }
}

fn register(app: &AppHandle, s: &mut ShortcutState) {
    let shortcuts = app.global_shortcut();
    if let Some(old) = s.registered.take() {
        let _ = shortcuts.unregister(old);
    }
    s.error = None;
    if s.paused {
        return;
    }
    let wanted = s.wanted.clone().unwrap_or_default();
    match parse_shortcut(&wanted) {
        Ok(None) => {}
        Ok(Some(shortcut)) => match shortcuts.register(shortcut) {
            Ok(()) => s.registered = Some(shortcut),
            Err(e) => s.error = Some(format!("macOS wouldn't take {}: {e}", wanted.trim())),
        },
        Err(e) => s.error = Some(e),
    }
}

pub fn status(app: &AppHandle, which: Keys) -> ShortcutStatus {
    let none = ShortcutStatus { which, shortcut: String::new(), error: None };
    let Some(state) = app.try_state::<Quick>() else { return none };
    let Ok(s) = state.slot(which).lock() else { return none };
    ShortcutStatus { which, shortcut: s.wanted.as_deref().unwrap_or_default().trim().to_string(), error: s.error.clone() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_shortcuts_as_config_toml_writes_them() {
        let s = parse_shortcut("Option+Shift+Space").unwrap().unwrap();
        assert_eq!((s.mods, s.key), (Modifiers::ALT | Modifiers::SHIFT, Code::Space));
        let s = parse_shortcut(" cmd+ctrl+k ").unwrap().unwrap();
        assert_eq!((s.mods, s.key), (Modifiers::SUPER | Modifiers::CONTROL, Code::KeyK));
        assert_eq!(parse_shortcut("Alt+`").unwrap().unwrap().key, Code::Backquote);
        let s = parse_shortcut(crate::config::DEFAULT_QUICK_PANEL).unwrap().unwrap();
        assert_eq!((s.mods, s.key), (Modifiers::ALT | Modifiers::SUPER, Code::Slash));
        let s = parse_shortcut(crate::config::DEFAULT_WAITING).unwrap().unwrap();
        assert_eq!((s.mods, s.key), (Modifiers::ALT | Modifiers::SUPER, Code::Period));
        let s = parse_shortcut(crate::config::DEFAULT_MAIN_WINDOW).unwrap().unwrap();
        assert_eq!((s.mods, s.key), (Modifiers::ALT | Modifiers::SUPER, Code::Semicolon));
        let s = parse_shortcut(crate::config::DEFAULT_CHAT).unwrap().unwrap();
        assert_eq!((s.mods, s.key), (Modifiers::ALT | Modifiers::SUPER, Code::Quote));
        assert!(parse_shortcut("").unwrap().is_none());
        assert!(parse_shortcut("  ").unwrap().is_none());
        // A function key can stand alone; Command and Space is allowed, for people who moved Spotlight.
        assert!(parse_shortcut("F18").unwrap().is_some());
        assert!(parse_shortcut("Cmd+Space").unwrap().is_some());
    }

    #[test]
    fn turns_away_shortcuts_that_would_take_typing_or_menus() {
        for bad in ["Space", "K", "Shift+K", "Shift+Space", "Cmd+K", "Cmd+Tab", "Cmd+Shift+Tab", "Option+Shift", "Option+Shift+Spacebar", "Option++Space"] {
            let err = parse_shortcut(bad).unwrap_err();
            assert!(!err.is_empty(), "{bad}");
        }
        assert!(parse_shortcut("Cmd+K").unwrap_err().contains("menu shortcut"));
        assert!(parse_shortcut("Shift+K").unwrap_err().contains("Control, Option or Command"));
    }

    #[test]
    fn opens_centered_near_the_top_of_the_screen() {
        let visible = Rect { x: 0.0, y: 0.0, w: 1512.0, h: 945.0 };
        assert_eq!(anchor_on(visible), (376.0, 945.0 - 170.0));
        // A screen to the left of the main one, in AppKit's coordinates.
        let left = Rect { x: -1920.0, y: 25.0, w: 1920.0, h: 1055.0 };
        assert_eq!(anchor_on(left), (-1340.0, 25.0 + 1055.0 - 190.0));
    }

    #[test]
    fn opens_in_one_script_with_the_board() {
        let script = open_script(&serde_json::json!({ "room": 620.0 }), r#"{"lanes":[]}"#);
        // The board is in the script once, with what the open is for.
        assert!(script.ends_with(r#"({"room":620.0}, {"lanes":[]})"#), "{script}");
        assert_eq!(script.matches("lanes").count(), 1);
        // The page returns its height; a page that isn't up keeps the open for when it is.
        assert!(script.contains("return typeof h === 'number' && isFinite(h) ? h : null;"));
        assert!(script.contains("window.__quickPending = [context, board]; return null;"));
    }

    #[test]
    fn opens_tell_the_page_its_room() {
        let asked = serde_json::json!({ "laneId": "l1", "prompt": "fix it" });
        assert_eq!(with_room(asked.clone(), Some(620.0)), serde_json::json!({ "laneId": "l1", "prompt": "fix it", "room": 620.0 }));
        // The shortcut asks for nothing in particular.
        assert_eq!(with_room(Value::Null, Some(620.0)), serde_json::json!({ "room": 620.0 }));
        // Off any screen there's no room to tell.
        assert_eq!(with_room(asked.clone(), None), asked);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn starts_bring_their_app_forward() {
        assert_eq!(start_app(Place::Terminal, "ghostty"), Some("com.mitchellh.ghostty"));
        assert_eq!(start_app(Place::Terminal, "iterm2"), Some("com.googlecode.iterm2"));
        assert_eq!(start_app(Place::Terminal, "kitty"), None);
        assert_eq!(start_app(Place::ClaudeApp, "ghostty"), Some("com.anthropic.claudefordesktop"));
        assert_eq!(start_app(Place::CodexApp, "ghostty"), Some("com.openai.codex"));
    }
}
