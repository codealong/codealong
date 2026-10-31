//! The window that works through what waits on you: one session or PR review at a time, with its
//! conversation and the box that answers it, going on to the next as each is dealt with
//! (Waiting.tsx). Unlike the quick panel it's an ordinary window: it makes Codealong the active
//! app, takes the size you drag it to, and stays up until you close it. The `[shortcuts] waiting`
//! shortcut opens it, unless the bar is docked in the notch, which does that itself
//! (`quick::work_through`); so do Window › Waiting on you, ⇥ on a session in the quick panel, and
//! the floating bar's reviews pill.
//!
//! It has no title bar of its own: the strip along the top of what it shows is its title bar
//! (SessionPanel.tsx `Hotkeys.titleBar`), with the window's buttons where macOS puts them, at its
//! start, and it drags the window. Its title is still there, unseen, for the Window menu and
//! Mission Control.
//!
//! It's made the first time it's asked for, and closing it only hides it, so the page is up for
//! the next open. Where it was is kept for the next launch (frames.rs).

use std::sync::{Arc, Mutex};

use anyhow::Result;
use serde_json::Value;
use tauri::menu::{MenuItem, MenuItemKind, PredefinedMenuItem, WINDOW_SUBMENU_ID};
use tauri::{AppHandle, Emitter, Manager, Window, WindowEvent};

use crate::frames::Place;

pub const LABEL: &str = "waiting";
const MENU_ID: &str = "waiting";
/// Its frame the first time, before you've sized it: about the quick panel's width, and tall
/// enough for a good part of a conversation.
const START_SIZE: (f64, f64) = (820.0, 780.0);
const MIN_SIZE: (f64, f64) = (520.0, 420.0);

struct Waiting {
    /// What the latest open asked for, which a page that comes up after it reads (`asked`).
    asked: Mutex<Value>,
    /// The app that was in front when the shortcut brought the window up, by its bundle ID: the
    /// shortcut putting it away gives that app the keyboard back.
    before: Mutex<Option<String>>,
    /// Where the window is.
    place: Arc<Place>,
}

/// Adds Window › Waiting on you. The window itself is made when it's first asked for.
pub fn setup(app: &AppHandle) -> Result<()> {
    app.manage(Waiting { asked: Default::default(), before: Default::default(), place: Place::load("waiting.json") });
    let item = MenuItem::with_id(app, MENU_ID, "Waiting on you", true, None::<&str>)?;
    if let Some(MenuItemKind::Submenu(menu)) = app.menu().and_then(|m| m.get(WINDOW_SUBMENU_ID)) {
        menu.append(&PredefinedMenuItem::separator(app)?)?;
        menu.append(&item)?;
    }
    app.on_menu_event(|app, event| {
        if event.id().as_ref() == MENU_ID {
            open(app, Value::Null);
        }
    });
    Ok(())
}

/// Brings the window up for `context`: `{ "key": … }` for a session or a PR waiting on your
/// review to show first; with nothing, the session it shows if it's up, else the first that
/// waits. On the main thread.
pub fn open(app: &AppHandle, context: Value) {
    let Some(state) = app.try_state::<Waiting>() else { return };
    if let Ok(mut asked) = state.asked.lock() {
        *asked = context.clone();
    }
    match app.get_webview_window(LABEL) {
        Some(w) => {
            let _ = w.unminimize();
            let _ = w.show();
            let _ = w.set_focus();
            let _ = app.emit_to(LABEL, "waiting-open", context);
        }
        None => {
            // The page reads what was asked once it's up.
            if let Err(e) = make(app, &state) {
                eprintln!("Couldn't open the waiting window: {e:#}");
            }
        }
    }
}

fn make(app: &AppHandle, state: &Waiting) -> Result<()> {
    let w = tauri::WebviewWindowBuilder::new(app, LABEL, tauri::WebviewUrl::App("index.html".into()))
        .title("Waiting on you")
        .inner_size(START_SIZE.0, START_SIZE.1)
        .min_inner_size(MIN_SIZE.0, MIN_SIZE.1)
        .visible(false)
        .initialization_script("window.__CODEALONG_WAITING__ = true;");
    // The card's strip is its title bar.
    let w = crate::os::windows::overlay_title_bar(w, None).build()?;
    state.place.restore(&w);
    w.show()?;
    w.set_focus()?;
    Ok(())
}

/// The page asks what the latest open was for, as it comes up.
pub fn asked(app: &AppHandle) -> Value {
    app.try_state::<Waiting>().and_then(|s| s.asked.lock().ok().map(|a| a.clone())).unwrap_or(Value::Null)
}

/// Puts the window away; `give_back` hands the keyboard to the app that had it when the shortcut
/// brought the window up. On the main thread.
pub fn hide(app: &AppHandle, give_back: bool) {
    let Some(w) = app.get_webview_window(LABEL) else { return };
    let _ = w.hide();
    let _ = app.emit_to(LABEL, "waiting-hidden", ());
    save(app);
    let before = app.try_state::<Waiting>().and_then(|s| s.before.lock().ok().and_then(|mut b| b.take()));
    if let (true, Some(id)) = (give_back, before) {
        crate::os::apps::activate(&id);
    }
}

/// Writes where the window is, for the next launch.
pub fn save(app: &AppHandle) {
    if let Some(state) = app.try_state::<Waiting>() {
        state.place.save();
    }
}

/// The shortcut that goes through what waits on you, when the bar isn't docked in the notch:
/// brings the window up, or puts it away when it's the window you're in. On the main thread.
pub fn toggle(app: &AppHandle) {
    let up = app.get_webview_window(LABEL).is_some_and(|w| w.is_visible().unwrap_or(false) && w.is_focused().unwrap_or(false));
    if up {
        hide(app, true);
        return;
    }
    if let Some(state) = app.try_state::<Waiting>() {
        if let Ok(mut before) = state.before.lock() {
            *before = crate::os::apps::frontmost_other();
        }
    }
    open(app, Value::Null);
}

/// Window events for the window: closing it only hides it, and where it is is kept as it moves.
/// On the main thread.
pub fn on_window_event(window: &Window, event: &WindowEvent) {
    if window.label() != LABEL {
        return;
    }
    match event {
        WindowEvent::CloseRequested { api, .. } => {
            api.prevent_close();
            hide(window.app_handle(), false);
        }
        WindowEvent::Moved(_) | WindowEvent::Resized(_) => {
            let app = window.app_handle();
            if let (Some(state), Some(w)) = (app.try_state::<Waiting>(), app.get_webview_window(LABEL)) {
                state.place.note(&w);
            }
        }
        _ => {}
    }
}
