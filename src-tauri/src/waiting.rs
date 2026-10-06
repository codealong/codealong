//! The window that works through what waits on you: one session or PR review at a time, with its
//! conversation and the box that answers it, going on to the next as each is dealt with
//! (Waiting.tsx). Unlike the quick panel it's an ordinary window: it makes Codealong the active
//! app, takes the size you drag it to, and stays up until you close it. The `[shortcuts] waiting`
//! shortcut opens it, unless the bar is docked in the notch, which does that itself
//! (`quick::work_through`); so do Window › Waiting on you, ⇥ on a session in the quick panel, and
//! the floating bar's reviews pill.
//!
//! It's made the first time it's asked for, and closing it only hides it, so the page is up for
//! the next open. Where it was is kept in `waiting.json` next to the config, for the next launch.

use std::sync::Mutex;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::menu::{MenuItem, MenuItemKind, PredefinedMenuItem, WINDOW_SUBMENU_ID};
use tauri::{AppHandle, Emitter, Manager, Window, WindowEvent};

use crate::config::support_dir;
use crate::geometry::{clamp, screen_at, screen_holding, Rect, Screen};
use crate::{front, panel};

pub const LABEL: &str = "waiting";
const MENU_ID: &str = "waiting";
/// Its frame the first time, before you've sized it: about the quick panel's width, and tall
/// enough for a good part of a conversation.
const START_SIZE: (f64, f64) = (820.0, 780.0);
const MIN_SIZE: (f64, f64) = (520.0, 420.0);

#[derive(Default, Serialize, Deserialize)]
struct Saved {
    /// Where it was last, in AppKit's points.
    #[serde(default)]
    frame: Option<Rect>,
}

impl Saved {
    /// Read leniently: waiting.json is only where the window was, which the next drag sets
    /// again, so one that doesn't read puts the window in its first place and is saved over.
    fn load() -> Saved {
        crate::store::read_or_default(&support_dir().join("waiting.json"))
    }

    fn save(&self) {
        let _ = crate::store::save(&support_dir().join("waiting.json"), self);
    }
}

#[derive(Default)]
struct Waiting {
    /// What the latest open asked for, which a page that comes up after it reads (`asked`).
    asked: Mutex<Value>,
    /// The app that was in front when the shortcut brought the window up, by its bundle ID: the
    /// shortcut putting it away gives that app the keyboard back.
    before: Mutex<Option<String>>,
    /// Where the window is, kept as it moves and sizes, and written when it's put away or
    /// Codealong quits rather than on every step of a drag.
    frame: Mutex<Option<Rect>>,
}

/// Adds Window › Waiting on you. The window itself is made when it's first asked for.
pub fn setup(app: &AppHandle) -> Result<()> {
    app.manage(Waiting { frame: Mutex::new(Saved::load().frame), ..Default::default() });
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
        .initialization_script("window.__CODEALONG_WAITING__ = true;")
        .build()?;
    let saved = state.frame.lock().ok().and_then(|f| *f);
    let pointer = panel::pointer().and_then(|p| screen_at(p, &panel::screens()));
    if let Some(frame) = panel::frame(&w).and_then(|now| start_frame(saved, now, &panel::screens(), pointer)) {
        panel::set_frame(&w, frame);
    }
    w.show()?;
    w.set_focus()?;
    Ok(())
}

/// Where the window opens: where it was, moved onto the screen that holds most of it if a screen
/// it was on is gone, else at its size centered on the screen with the pointer, a little above
/// the middle. None with no screens.
fn start_frame(saved: Option<Rect>, now: Rect, screens: &[Screen], pointer: Option<Screen>) -> Option<Rect> {
    if let Some(s) = saved {
        if let Some(screen) = screen_holding(s.center(), screens) {
            return Some(clamp(s, screen.visible));
        }
    }
    let screen = pointer.or(screens.first().copied())?.visible;
    let (w, h) = saved.map_or((now.w, now.h), |s| (s.w, s.h));
    let x = screen.x + (screen.w - w) / 2.0;
    // A third of what's left over goes above it, two thirds below, as macOS centers a window.
    let y = screen.y + (screen.h - h) * 2.0 / 3.0;
    Some(clamp(Rect { x: x.round(), y: y.round(), w, h }, screen))
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
        front::activate(&id);
    }
}

/// Writes where the window is, for the next launch.
pub fn save(app: &AppHandle) {
    let Some(state) = app.try_state::<Waiting>() else { return };
    let frame = state.frame.lock().ok().and_then(|f| *f);
    if frame.is_some() {
        Saved { frame }.save();
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
            *before = front::frontmost_other();
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
            let (Some(state), Some(w)) = (app.try_state::<Waiting>(), app.get_webview_window(LABEL)) else { return };
            // Hidden or minimized, it reports a frame that isn't where it shows.
            if !w.is_visible().unwrap_or(false) || w.is_minimized().unwrap_or(false) {
                return;
            }
            let Some(frame) = panel::frame(&w) else { return };
            if let Ok(mut slot) = state.frame.lock() {
                *slot = Some(frame);
            };
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAIN: Screen = Screen { frame: Rect { x: 0.0, y: 0.0, w: 1512.0, h: 982.0 }, visible: Rect { x: 0.0, y: 0.0, w: 1512.0, h: 945.0 } };
    const LEFT: Screen = Screen { frame: Rect { x: -1920.0, y: 0.0, w: 1920.0, h: 1080.0 }, visible: Rect { x: -1920.0, y: 0.0, w: 1920.0, h: 1055.0 } };
    const NOW: Rect = Rect { x: 0.0, y: 0.0, w: 820.0, h: 808.0 };

    #[test]
    fn opens_where_it_was() {
        let was = Rect { x: -1500.0, y: 100.0, w: 900.0, h: 700.0 };
        assert_eq!(start_frame(Some(was), NOW, &[MAIN, LEFT], Some(MAIN)), Some(was));
        // Partly off its screen, it's moved back on.
        let hanging = Rect { x: 1000.0, y: 100.0, w: 900.0, h: 700.0 };
        assert_eq!(start_frame(Some(hanging), NOW, &[MAIN], Some(MAIN)).map(|r| r.right()), Some(1512.0 - crate::geometry::EDGE));
    }

    #[test]
    fn opens_centered_where_the_pointer_is_the_first_time_and_when_its_screen_is_gone() {
        let first = start_frame(None, NOW, &[MAIN, LEFT], Some(LEFT)).unwrap();
        assert_eq!((first.w, first.h), (820.0, 808.0));
        assert!(LEFT.visible.contains(first.center()), "{first:?}");
        assert_eq!(first.x, -1920.0 + 550.0);
        // A little above the middle.
        assert!(first.center().1 > LEFT.visible.center().1, "{first:?}");
        // Its screen unplugged: at the size it had, on the pointer's.
        let gone = Rect { x: 3000.0, y: 100.0, w: 900.0, h: 700.0 };
        let moved = start_frame(Some(gone), NOW, &[MAIN], Some(MAIN)).unwrap();
        assert_eq!((moved.w, moved.h), (900.0, 700.0));
        assert!(MAIN.visible.contains(moved.center()), "{moved:?}");
        assert_eq!(start_frame(None, NOW, &[], None), None);
    }
}
