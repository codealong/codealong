//! Sessions kept on the desktop: the floating bar's peek, kept. The peek shows a session while the
//! pointer is on its pill and goes when it leaves (peek.rs); its Keep button, dragging its edge
//! to make it bigger, or dragging it by its header leaves the session up where it is, in a window
//! of its own: its conversation and the box that answers it (Peek.tsx draws it, as it draws the
//! peek). You drag it by its header, size it by its edges, and close it with its ×. It floats over
//! every other window, on every Space and over full-screen apps, as the cards pinned to the
//! desktop do (desktop.rs).
//!
//! Each is a non-activating panel that takes the keyboard on a click, like the peek's, so a click
//! in one leaves the app you're in active. What's kept and where each window was left are in
//! `kept.json` next to the config, and they come back at launch.

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::time::Duration;

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, Window, WindowEvent};

use crate::geometry::{screen_for, Rect, Screen};
use crate::config::support_dir;
use crate::panel;

/// The windows are `kept-1`, `kept-2`… in the order they were made since launch.
pub const PREFIX: &str = "kept-";
/// A kept session's panel is the peek's, but with the cards on the desktop: at their level, and
/// left in place by Mission Control.
const PANEL: panel::Config = panel::Config { keys: panel::Keys::OnClick, level: panel::Level::Kept, spaces: panel::Spaces::Stationary, unconstrained: false, tooltips: true, steps: panel::Steps::One };
/// How long a new window waits for its page before showing anyway: a page takes longer to load
/// than the peek's, which is there already, takes to read a conversation.
const READY_WAIT: Duration = Duration::from_millis(2500);

/// A session kept on the desktop, as `kept.json` has it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
struct Kept {
    /// The session's key.
    key: String,
    /// Its window, where it was left.
    frame: Rect,
}

#[derive(Default, Serialize, Deserialize, Debug, PartialEq)]
struct Saved {
    #[serde(default)]
    sessions: Vec<Kept>,
}

impl Saved {
    fn load() -> Saved {
        crate::store::read_or_default(&support_dir().join("kept.json"))
    }

    fn save(&self) {
        let _ = crate::store::save(&support_dir().join("kept.json"), self);
    }
}

#[derive(Default)]
struct State {
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    /// Each window and the session it shows, in the order they were kept.
    windows: Vec<(String, Kept)>,
    /// The windows made into panels, with the class each had before, which it gets back before
    /// it's destroyed.
    panels: HashMap<String, Option<panel::Original>>,
    /// The windows that are up: their pages have laid out their sessions.
    shown: HashSet<String>,
    /// The windows that take the keyboard as they show: kept from a peek that had it.
    keys: HashSet<String>,
    /// Numbers the windows.
    made: u64,
    /// A window moved or changed size since `kept.json` was written.
    dirty: bool,
}

impl Inner {
    fn save(&mut self) {
        Saved { sessions: self.windows.iter().map(|(_, k)| k.clone()).collect() }.save();
        self.dirty = false;
    }

    fn label_of(&self, key: &str) -> Option<String> {
        self.windows.iter().find(|(_, k)| k.key == key).map(|(l, _)| l.clone())
    }
}

/// Makes a window for each session kept before, and starts the loop that keeps them up.
pub fn setup(app: &AppHandle) -> Result<()> {
    app.manage(State::default());
    for kept in Saved::load().sessions {
        let label = add(app, kept, false);
        if let Err(e) = make_window(app, &label) {
            eprintln!("Couldn't put a kept session back on the desktop: {e:#}");
        }
    }
    let handle = app.clone();
    std::thread::Builder::new().name("codealong-kept".into()).spawn(move || loop {
        std::thread::sleep(panel::CHECK_EVERY);
        let app = handle.clone();
        if handle.run_on_main_thread(move || keep_up(&app)).is_err() {
            break;
        }
    })?;
    Ok(())
}

/// Takes a session into the list and names its window.
fn add(app: &AppHandle, kept: Kept, keys: bool) -> String {
    let Some(state) = app.try_state::<State>() else { return String::new() };
    let Ok(mut inner) = state.inner.lock() else { return String::new() };
    inner.made += 1;
    let label = format!("{PREFIX}{}", inner.made);
    if keys {
        inner.keys.insert(label.clone());
    }
    inner.windows.push((label.clone(), kept));
    label
}

/// Makes a session's window, hidden, then on the main thread turns it into a panel where it was
/// left. It shows once its page has laid the session out (`ready`), or after a moment without.
/// Off the main thread, which making a window waits on.
fn make_window(app: &AppHandle, label: &str) -> Result<()> {
    let page = panel::Page { title: "Codealong", script: "window.__CODEALONG_KEPT__ = true;", size: crate::peek::MIN_SIZE, focusable: true, first_mouse: true };
    panel::builder(app, label, page).build()?;
    let (handle, placed) = (app.clone(), label.to_string());
    app.run_on_main_thread(move || place(&handle, &placed))?;
    let (handle, waited) = (app.clone(), label.to_string());
    std::thread::spawn(move || {
        std::thread::sleep(READY_WAIT);
        let app = handle.clone();
        let _ = handle.run_on_main_thread(move || ready(&app, &waited));
    });
    Ok(())
}

/// A frame kept on a screen that's there: where it was left if its middle is still on one, else
/// on the first screen. No smaller than a kept window gets.
pub fn on_a_screen(frame: Rect, screens: &[Screen]) -> Rect {
    let frame = Rect { w: frame.w.max(crate::peek::MIN_SIZE.0), h: frame.h.max(crate::peek::MIN_SIZE.1), ..frame };
    let Some(v) = screen_for(frame, screens) else { return frame };
    // Whole on the screen, moved no further than that takes: the window keeps room around its
    // card for the shadow, so it may touch the screen's edge.
    let (w, h) = (frame.w.min(v.w), frame.h.min(v.h));
    Rect { x: frame.x.min(v.right() - w).max(v.x), y: frame.y.min(v.top() - h).max(v.y), w, h }
}

/// A panel before it's first shown: one shown as a window stays off full-screen Spaces. On the
/// main thread.
fn place(app: &AppHandle, label: &str) {
    let (Some(state), Some(window)) = (app.try_state::<State>(), app.get_webview_window(label)) else { return };
    let original = panel::make(&window, PANEL);
    let frame = {
        let Ok(mut inner) = state.inner.lock() else { return };
        let Some((_, kept)) = inner.windows.iter_mut().find(|(l, _)| l == label) else { return };
        kept.frame = on_a_screen(kept.frame, &panel::screens());
        let frame = kept.frame;
        inner.panels.insert(label.to_string(), original);
        inner.save();
        frame
    };
    panel::set_frame(&window, frame);
}

/// The window's page has laid its session out: the window shows, and the peek it was kept from
/// goes. On the main thread.
pub fn ready(app: &AppHandle, label: &str) {
    let (Some(state), Some(window)) = (app.try_state::<State>(), app.get_webview_window(label)) else { return };
    let (key, keys) = {
        let Ok(mut inner) = state.inner.lock() else { return };
        // Not a panel yet, or up already.
        if !inner.panels.contains_key(label) || !inner.shown.insert(label.to_string()) {
            return;
        }
        let keys = inner.keys.remove(label);
        (inner.windows.iter().find(|(l, _)| l == label).map(|(_, k)| k.key.clone()), keys)
    };
    panel::present(&window, keys);
    if crate::peek::showing(app).map(|(k, _)| k) == key {
        crate::peek::hide(app);
    }
}

/// Keeps the session the peek shows on the desktop, where the peek's card is. On the main thread.
pub fn keep_peeked(app: &AppHandle) -> Result<()> {
    let (key, card) = crate::peek::showing(app).ok_or_else(|| anyhow!("The peek isn't showing a session."))?;
    // One kept already has no new window whose showing puts the peek away.
    if keep(app, key, card, crate::peek::engaged(app))? {
        crate::peek::hide(app);
    }
    Ok(())
}

/// Keeps a session on the desktop in a window of its own, its card at `card` on the screen: the
/// peek's, or the one the bar docked in the notch shows under its pills. With `keys` it takes the
/// keyboard as it shows. One kept already comes forward where it is, and the answer is true. On
/// the main thread.
pub fn keep(app: &AppHandle, key: String, card: Rect, keys: bool) -> Result<bool> {
    let state = app.try_state::<State>().ok_or_else(|| anyhow!("Kept sessions aren't ready yet."))?;
    let already = state.inner.lock().map_err(|_| anyhow!("Kept sessions are unavailable."))?.label_of(&key);
    if let Some(label) = already {
        raise(app, &label, keys);
        return Ok(true);
    }
    let label = add(app, Kept { key, frame: crate::peek::window_for(card) }, keys);
    open(app, label);
    Ok(false)
}

/// Keeps `to` where session `from` is kept: a session moved to another account goes on as a new
/// session (start.rs `move_to`), and its window goes with it. A page reads the session it keeps
/// as it loads, so the copy gets a window of its own where the old one was, and the old one
/// closes. Nothing changes when `to` is kept already.
pub fn follow(app: &AppHandle, from: &str, to: &str) {
    let Some(state) = app.try_state::<State>() else { return };
    let (old, frame) = {
        let Ok(inner) = state.inner.lock() else { return };
        let Some((label, kept)) = inner.windows.iter().find(|(_, k)| k.key == from).filter(|_| inner.label_of(to).is_none()) else { return };
        (label.clone(), kept.frame)
    };
    let label = add(app, Kept { key: to.to_string(), frame }, false);
    open(app, label);
    close(app, &old);
}

/// Makes a new kept window, off the main thread, which making a window waits on.
fn open(app: &AppHandle, label: String) {
    let handle = app.clone();
    std::thread::spawn(move || {
        if let Err(e) = make_window(&handle, &label) {
            eprintln!("Couldn't keep the session on the desktop: {e:#}");
            if let Some(state) = handle.try_state::<State>() {
                if let Ok(mut inner) = state.inner.lock() {
                    inner.windows.retain(|(l, _)| l != &label);
                }
            }
        }
    });
}

/// Brings a kept session's window up, with the keyboard if `keys`. On the main thread.
fn raise(app: &AppHandle, label: &str, keys: bool) {
    let Some(state) = app.try_state::<State>() else { return };
    let up = state.inner.lock().is_ok_and(|i| i.shown.contains(label));
    if let (true, Some(window)) = (up, app.get_webview_window(label)) {
        if keys {
            panel::present(&window, true);
        } else if !panel::on_screen(&window) {
            panel::present(&window, false);
        }
    }
}

/// Brings up the window a session is kept in, if it's kept: the peek isn't shown for a session
/// that's up already. On the main thread.
pub fn raise_session(app: &AppHandle, key: &str, keys: bool) -> bool {
    let Some(label) = app.try_state::<State>().and_then(|s| s.inner.lock().ok().and_then(|i| i.label_of(key))) else { return false };
    raise(app, &label, keys);
    true
}

/// The session a window shows, for its page.
pub fn session(app: &AppHandle, label: &str) -> Option<String> {
    let state = app.try_state::<State>()?;
    let inner = state.inner.lock().ok()?;
    inner.windows.iter().find(|(l, _)| l == label).map(|(_, k)| k.key.clone())
}

/// The window's frame at a new size: it keeps its top left corner, no smaller than a kept window
/// gets and no bigger than its screen.
pub fn sized(frame: Rect, width: f64, height: f64, screen: Option<Rect>) -> Rect {
    let (min_w, min_h) = crate::peek::MIN_SIZE;
    let (max_w, max_h) = screen.map_or((f64::MAX, f64::MAX), |s| (s.w, s.h));
    let (w, h) = (width.max(min_w).min(max_w), height.max(min_h).min(max_h));
    Rect { x: frame.x, y: frame.top() - h, w, h }
}

/// You're dragging the window's edge: it takes that size, keeping its top left corner. On the
/// main thread.
pub fn resize(app: &AppHandle, label: &str, width: f64, height: f64) {
    let (Some(state), Some(window)) = (app.try_state::<State>(), app.get_webview_window(label)) else { return };
    let Some(current) = panel::frame(&window) else { return };
    let screen = screen_for(current, &panel::screens());
    let next = sized(current, width.ceil(), height.ceil(), screen);
    if next == current {
        return;
    }
    panel::set_frame(&window, next);
    if let Ok(mut inner) = state.inner.lock() {
        if let Some((_, kept)) = inner.windows.iter_mut().find(|(l, _)| l == label) {
            kept.frame = next;
            inner.dirty = true;
        }
    };
}

/// A click in the window: it takes the keyboard. On the main thread.
pub fn take_keys(app: &AppHandle, label: &str) {
    let Some(state) = app.try_state::<State>() else { return };
    let up = state.inner.lock().is_ok_and(|i| i.shown.contains(label));
    if let (true, Some(window)) = (up, app.get_webview_window(label)) {
        panel::take_keys(&window);
    }
}

/// Takes a session off the desktop: its window goes back to being a window, then away.
pub fn close(app: &AppHandle, label: &str) {
    let Some(state) = app.try_state::<State>() else { return };
    let original = {
        let Ok(mut inner) = state.inner.lock() else { return };
        if !inner.windows.iter().any(|(l, _)| l == label) {
            return;
        }
        inner.windows.retain(|(l, _)| l != label);
        inner.shown.remove(label);
        inner.keys.remove(label);
        let original = inner.panels.remove(label).flatten();
        inner.save();
        original
    };
    let (handle, label) = (app.clone(), label.to_string());
    let _ = app.run_on_main_thread(move || {
        let Some(w) = handle.get_webview_window(&label) else { return };
        if let Some(original) = original {
            panel::unmake(&w, original);
        }
        let _ = w.destroy();
    });
}

/// Writes down where a window is now. On the main thread.
fn remember_frame(app: &AppHandle, label: &str) {
    let (Some(state), Some(window)) = (app.try_state::<State>(), app.get_webview_window(label)) else { return };
    let Some(frame) = panel::frame(&window) else { return };
    if let Ok(mut inner) = state.inner.lock() {
        // Before it's placed, the frame is the builder's, not where it was left.
        if !inner.panels.contains_key(label) {
            return;
        }
        if let Some((_, kept)) = inner.windows.iter_mut().find(|(l, _)| l == label) {
            if kept.frame != frame {
                kept.frame = frame;
                inner.dirty = true;
            }
        }
    };
}

/// Whether two rectangles overlap.
fn overlap(a: Rect, b: Rect) -> bool {
    a.x < b.right() && b.x < a.right() && a.y < b.top() && b.y < a.top()
}

/// Puts back up a window macOS left off the Space in front, unless another kept one covers it
/// (showing it each time would swap them every second), and writes down where they were moved
/// to. On the main thread.
fn keep_up(app: &AppHandle) {
    let Some(state) = app.try_state::<State>() else { return };
    let up: Vec<(String, Rect)> = {
        let Ok(mut inner) = state.inner.lock() else { return };
        if inner.dirty {
            inner.save();
        }
        inner.windows.iter().filter(|(l, _)| inner.shown.contains(l)).map(|(l, k)| (l.clone(), k.frame)).collect()
    };
    for (label, frame) in &up {
        let Some(window) = app.get_webview_window(label) else { continue };
        let covered = up.iter().any(|(l, o)| l != label && overlap(*frame, *o));
        if !covered && !panel::on_screen(&window) {
            panel::present(&window, false);
        }
    }
}

/// Window events for the kept sessions: a moved one is remembered; one that lost the keyboard
/// goes back to saying it's key, as the peek does up from a hover, so its page still gets the
/// pointer; and closing one (⌘W) takes it off the desktop. On the main thread.
pub fn on_window_event(window: &Window, event: &WindowEvent) {
    let label = window.label();
    if !label.starts_with(PREFIX) {
        return;
    }
    let app = window.app_handle();
    match event {
        WindowEvent::Moved(_) => remember_frame(app, label),
        WindowEvent::Focused(true) => {
            let _ = app.emit_to(label, "kept-keys", true);
        }
        WindowEvent::Focused(false) => {
            let up = app.try_state::<State>().is_some_and(|s| s.inner.lock().is_ok_and(|i| i.shown.contains(label)));
            if let (true, Some(w)) = (up, app.get_webview_window(label)) {
                panel::present(&w, false);
            }
            let _ = app.emit_to(label, "kept-keys", false);
        }
        WindowEvent::CloseRequested { api, .. } => {
            api.prevent_close();
            close(app, label);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(x: f64, y: f64, w: f64, h: f64) -> Rect {
        Rect { x, y, w, h }
    }

    const SCREEN: Screen = Screen { frame: Rect { x: 0.0, y: 0.0, w: 1500.0, h: 900.0 }, visible: Rect { x: 0.0, y: 60.0, w: 1500.0, h: 815.0 } };

    #[test]
    fn a_window_keeps_its_top_left_corner_as_its_size_changes() {
        let f = r(100.0, 200.0, 520.0, 400.0);
        let next = sized(f, 700.0, 500.0, Some(SCREEN.visible));
        assert_eq!((next.x, next.top(), next.w, next.h), (100.0, 600.0, 700.0, 500.0));
        // No smaller than a kept window gets, no bigger than its screen.
        let (min_w, min_h) = crate::peek::MIN_SIZE;
        let small = sized(f, 10.0, 10.0, Some(SCREEN.visible));
        assert_eq!((small.w, small.h, small.top()), (min_w, min_h, 600.0));
        let big = sized(f, 9000.0, 9000.0, Some(SCREEN.visible));
        assert_eq!((big.w, big.h), (1500.0, 815.0));
    }

    #[test]
    fn a_window_comes_back_where_it_was_left_if_that_screen_is_still_there() {
        let f = r(100.0, 200.0, 520.0, 400.0);
        assert_eq!(on_a_screen(f, &[SCREEN]), f);
        // Left on a screen that's gone: onto the first one, whole.
        let gone = on_a_screen(r(3000.0, 200.0, 520.0, 400.0), &[SCREEN]);
        assert!(SCREEN.visible.contains((gone.x, gone.y)) && SCREEN.visible.contains((gone.right(), gone.top())), "{gone:?}");
        // Bigger than the screen it's on now: cut to fit.
        let cut = on_a_screen(r(0.0, 0.0, 2000.0, 1200.0), &[SCREEN]);
        assert_eq!(cut, SCREEN.visible);
        // At the screen's edge, where the peek opens by a pill near it, it stays put.
        let edge = r(0.0, 300.0, 520.0, 400.0);
        assert_eq!(on_a_screen(edge, &[SCREEN]), edge);
    }

    #[test]
    fn the_peek_s_card_stays_where_it_is_when_kept() {
        // A peek under the bar: its window, and the card in it.
        let card = r(120.0, 232.0, 480.0, 560.0);
        let window = crate::peek::window_for(card);
        // The page lays out as the peek's does under the bar: the shadow's room under the card.
        assert_eq!((window.x, window.w), (100.0, 520.0));
        assert!(window.y < card.y && window.top() > card.top());
        assert_eq!(window.h - card.h, (card.y - window.y) + (window.top() - card.top()));
    }

    #[test]
    fn saved_sessions_read_old_and_partial_files() {
        let s: Saved = serde_json::from_str("{}").unwrap();
        assert_eq!(s, Saved::default());
        let s: Saved = serde_json::from_str(r#"{"sessions":[{"key":"claude:claude:1","frame":{"x":1,"y":2,"w":3,"h":4}}]}"#).unwrap();
        assert_eq!(s.sessions, [Kept { key: "claude:claude:1".into(), frame: r(1.0, 2.0, 3.0, 4.0) }]);
    }
}
