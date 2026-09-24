//! The floating bar: a small window above every other that shows the agents that need you and the
//! ones running, whenever no board window is on screen: closed, minimized, on another Space or
//! covered by other windows. macOS keeps track of whether any part of a window can be seen (its
//! occlusion state); Tauri doesn't pass that on, so the bar reads it from AppKit.
//!
//! The bar never takes the keyboard, so the board keeps it. Clicking the bar still makes Codealong
//! the active app, which it has to be for Ghostty or Codex to come forward.
//!
//! Closing the main window hides it, so the bar has something to stand in for; the Dock icon and
//! the bar bring it back. Whether you turned the bar off and where you left it are kept in
//! `bar.json` next to the config.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use tauri::menu::{CheckMenuItem, MenuItemKind, PredefinedMenuItem, WINDOW_SUBMENU_ID};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder, Window, WindowEvent, Wry};

use crate::config::support_dir;

pub const LABEL: &str = "bar";
const MENU_ID: &str = "floating-bar";
/// How often the bar checks whether a board window can be seen. There's no event for it that
/// Tauri passes on, and the check is a property read.
const POLL: Duration = Duration::from_millis(400);
/// The bar's size before its page has measured itself.
const START: (f64, f64) = (240.0, 48.0);
/// Space between a newly placed bar and the menu bar.
const TOP_GAP: f64 = 6.0;
/// Room kept at the sides of the screen.
const EDGE: f64 = 8.0;

/// A rectangle in screen points as AppKit gives them: the origin at the bottom left of the main
/// screen, y growing up.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    fn top(&self) -> f64 {
        self.y + self.h
    }
    fn right(&self) -> f64 {
        self.x + self.w
    }
    fn center(&self) -> (f64, f64) {
        (self.x + self.w / 2.0, self.y + self.h / 2.0)
    }
    fn contains(&self, (x, y): (f64, f64)) -> bool {
        x >= self.x && x <= self.right() && y >= self.y && y <= self.top()
    }
}

/// A screen: all of it, and the part the menu bar and Dock leave free.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Screen {
    pub frame: Rect,
    pub visible: Rect,
}

/// Where the bar goes at a new size. It keeps its top edge and the side it's nearest: a bar in
/// the left third of the screen grows to the right, one in the right third grows to the left, and
/// one in the middle stays centered. It stays on the screen.
pub fn place(current: Rect, w: f64, h: f64, screen: Rect) -> Rect {
    let (cx, _) = current.center();
    let third = (cx - screen.x) / screen.w;
    let x = if third < 1.0 / 3.0 {
        current.x
    } else if third > 2.0 / 3.0 {
        current.right() - w
    } else {
        cx - w / 2.0
    };
    clamp(Rect { x, y: current.top() - h, w, h }, screen)
}

/// Moves the rectangle onto the screen, narrowing it only when it's wider than the screen.
fn clamp(r: Rect, screen: Rect) -> Rect {
    let w = r.w.min(screen.w - 2.0 * EDGE).max(1.0);
    let h = r.h.min(screen.h).max(1.0);
    let x = r.x.min(screen.right() - EDGE - w).max(screen.x + EDGE);
    let y = r.y.min(screen.top() - h).max(screen.y);
    Rect { x, y, w, h }
}

/// Where the bar starts: where you left it if that's still on a screen, else top center of the
/// first screen, just under the menu bar.
pub fn start_frame(saved: Option<Rect>, screens: &[Screen]) -> Option<Rect> {
    if let Some(r) = saved {
        if let Some(s) = screens.iter().find(|s| s.frame.contains(r.center())) {
            return Some(clamp(r, s.visible));
        }
    }
    let v = screens.first()?.visible;
    let (w, h) = saved.map(|r| (r.w, r.h)).unwrap_or(START);
    Some(clamp(Rect { x: v.x + (v.w - w) / 2.0, y: v.top() - TOP_GAP - h, w, h }, v))
}

/// The usable area of the screen a rectangle is on: the one holding its center, else the first.
fn screen_for(r: Rect, screens: &[Screen]) -> Option<Rect> {
    screens.iter().find(|s| s.frame.contains(r.center())).or(screens.first()).map(|s| s.visible)
}

/// What `bar.json` keeps.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Saved {
    /// Turned off from the bar's menu or the Window menu.
    #[serde(default)]
    off: bool,
    /// Where the bar was last.
    #[serde(default)]
    frame: Option<Rect>,
}

impl Saved {
    fn load() -> Saved {
        std::fs::read_to_string(support_dir().join("bar.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    }

    fn save(&self) {
        if std::fs::create_dir_all(support_dir()).is_ok() {
            if let Ok(text) = serde_json::to_string_pretty(self) {
                let _ = std::fs::write(support_dir().join("bar.json"), text);
            }
        }
    }
}

struct Bar {
    saved: Mutex<Saved>,
    /// The bar moved since `bar.json` was written.
    moved: AtomicBool,
    /// Window › Floating bar.
    item: Mutex<Option<CheckMenuItem<Wry>>>,
}

/// Makes the bar (hidden), adds Window › Floating bar, and starts watching the board windows.
pub fn setup(app: &AppHandle) -> Result<()> {
    let saved = Saved::load();
    let (off, frame) = (saved.off, saved.frame);
    app.manage(Bar { saved: Mutex::new(saved), moved: AtomicBool::new(false), item: Mutex::new(None) });

    let window = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("index.html".into()))
        .title("Codealong bar")
        .initialization_script("window.__CODEALONG_BAR__ = true;")
        .inner_size(START.0, START.1)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .always_on_top(true)
        .visible_on_all_workspaces(true)
        .skip_taskbar(true)
        .focusable(false)
        .focused(false)
        .accept_first_mouse(true)
        .visible(false)
        .build()?;
    native::prepare(&window);
    if let Some(r) = start_frame(frame, &native::screens()) {
        native::set_frame(&window, r);
    }

    let item = CheckMenuItem::with_id(app, MENU_ID, "Floating bar", true, !off, None::<&str>)?;
    if let Some(MenuItemKind::Submenu(menu)) = app.menu().and_then(|m| m.get(WINDOW_SUBMENU_ID)) {
        menu.append(&PredefinedMenuItem::separator(app)?)?;
        menu.append(&item)?;
    }
    if let Ok(mut slot) = app.state::<Bar>().item.lock() {
        *slot = Some(item);
    }
    app.on_menu_event(|app, event| {
        if event.id().as_ref() == MENU_ID {
            // The menu has already flipped the check mark.
            let on = app.state::<Bar>().item.lock().ok().and_then(|i| i.as_ref().and_then(|i| i.is_checked().ok())).unwrap_or(true);
            set_on(app, on);
        }
    });

    let handle = app.clone();
    std::thread::Builder::new().name("codealong-bar".into()).spawn(move || loop {
        std::thread::sleep(POLL);
        let app = handle.clone();
        if handle.run_on_main_thread(move || tick(&app)).is_err() {
            break;
        }
    })?;
    Ok(())
}

fn is_board(label: &str) -> bool {
    label == "main" || label.starts_with("project-")
}

/// Shows the bar when no board window can be seen, hides it when one can, and writes down where
/// the bar is after it moves. On the main thread.
fn tick(app: &AppHandle) {
    let (Some(bar), Some(state)) = (app.get_webview_window(LABEL), app.try_state::<Bar>()) else {
        return;
    };
    let off = state.saved.lock().map(|s| s.off).unwrap_or(false);
    let board_seen = app.webview_windows().iter().any(|(label, w)| is_board(label) && native::on_screen(w));
    let want = !off && !board_seen;
    if want != bar.is_visible().unwrap_or(false) {
        let _ = if want { bar.show() } else { bar.hide() };
    }
    if state.moved.swap(false, Ordering::Relaxed) {
        if let (Some(frame), Ok(mut saved)) = (native::frame(&bar), state.saved.lock()) {
            saved.frame = Some(frame);
            saved.save();
        }
    }
}

/// Window events that concern the bar: closing the main window hides it instead, and a moved bar
/// is remembered.
pub fn on_window_event(window: &Window, event: &WindowEvent) {
    match event {
        WindowEvent::CloseRequested { api, .. } if window.label() == "main" => {
            api.prevent_close();
            let _ = window.hide();
        }
        WindowEvent::Moved(_) if window.label() == LABEL => {
            if let Some(state) = window.try_state::<Bar>() {
                state.moved.store(true, Ordering::Relaxed);
            }
        }
        _ => {}
    }
}

/// Sizes the bar to its page, which measures itself whenever what it shows changes.
pub fn fit(app: &AppHandle, width: f64, height: f64) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        let Some(bar) = handle.get_webview_window(LABEL) else { return };
        let Some(current) = native::frame(&bar) else { return };
        let Some(screen) = screen_for(current, &native::screens()) else { return };
        let next = place(current, width.ceil(), height.ceil(), screen);
        if next != current {
            native::set_frame(&bar, next);
        }
    });
}

/// Turns the bar on or off, and keeps Window › Floating bar in step.
pub fn set_on(app: &AppHandle, on: bool) {
    let Some(state) = app.try_state::<Bar>() else { return };
    if let Ok(mut saved) = state.saved.lock() {
        saved.off = !on;
        saved.save();
    }
    if let Some(item) = state.item.lock().ok().and_then(|i| i.clone()) {
        let _ = item.set_checked(on);
    }
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || tick(&handle));
}

/// Brings the main window forward. `reveal` (a session or PR) goes to its page, which selects it.
pub fn show_main(app: &AppHandle, reveal: Option<serde_json::Value>) -> Result<()> {
    let main = app.get_webview_window("main").ok_or_else(|| anyhow!("The main window is gone."))?;
    let _ = main.unminimize();
    main.show()?;
    main.set_focus()?;
    if let Some(r) = reveal {
        app.emit_to("main", "reveal", r)?;
    }
    Ok(())
}

#[cfg(target_os = "macos")]
mod native {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSScreen, NSWindow, NSWindowCollectionBehavior, NSWindowOcclusionState};
    use objc2_foundation::{NSPoint, NSRect, NSSize};
    use tauri::WebviewWindow;

    use super::{Rect, Screen};

    /// The window's NSWindow, on the main thread only.
    fn ns(w: &WebviewWindow) -> Option<&NSWindow> {
        MainThreadMarker::new()?;
        let ptr = w.ns_window().ok()? as *const NSWindow;
        // SAFETY: Tauri hands out the window's own NSWindow, which lives as long as the window,
        // and this is the main thread, where AppKit expects to be called.
        unsafe { ptr.as_ref() }
    }

    fn rect(r: NSRect) -> Rect {
        Rect { x: r.origin.x, y: r.origin.y, w: r.size.width, h: r.size.height }
    }

    /// Some part of the window can be seen: it's open, not minimized, on this Space and not
    /// entirely covered. When that can't be told, it counts as seen, so the bar stays out of the
    /// way.
    pub fn on_screen(w: &WebviewWindow) -> bool {
        ns(w).map(|n| n.isVisible() && n.occlusionState().contains(NSWindowOcclusionState::Visible)).unwrap_or(true)
    }

    /// On every Space and over full-screen apps, left in place by Mission Control, kept out of ⌘`
    /// and the Window menu, and still up when Codealong is hidden (⌘H), which hides the board.
    pub fn prepare(w: &WebviewWindow) {
        if let Some(n) = ns(w) {
            n.setCollectionBehavior(
                n.collectionBehavior()
                    | NSWindowCollectionBehavior::CanJoinAllSpaces
                    | NSWindowCollectionBehavior::Stationary
                    | NSWindowCollectionBehavior::IgnoresCycle
                    | NSWindowCollectionBehavior::FullScreenAuxiliary,
            );
            n.setExcludedFromWindowsMenu(true);
            n.setCanHide(false);
        }
    }

    pub fn frame(w: &WebviewWindow) -> Option<Rect> {
        ns(w).map(|n| rect(n.frame()))
    }

    pub fn set_frame(w: &WebviewWindow, r: Rect) {
        if let Some(n) = ns(w) {
            n.setFrame_display(NSRect::new(NSPoint::new(r.x, r.y), NSSize::new(r.w, r.h)), true);
        }
    }

    /// Every screen, the one with the menu bar first.
    pub fn screens() -> Vec<Screen> {
        let Some(mtm) = MainThreadMarker::new() else { return Vec::new() };
        let all = NSScreen::screens(mtm);
        (0..all.count()).map(|i| all.objectAtIndex(i)).map(|s| Screen { frame: rect(s.frame()), visible: rect(s.visibleFrame()) }).collect()
    }
}

/// Elsewhere there's no bar: board windows always count as seen.
#[cfg(not(target_os = "macos"))]
mod native {
    use tauri::WebviewWindow;

    use super::{Rect, Screen};

    pub fn on_screen(_: &WebviewWindow) -> bool {
        true
    }
    pub fn prepare(_: &WebviewWindow) {}
    pub fn frame(_: &WebviewWindow) -> Option<Rect> {
        None
    }
    pub fn set_frame(_: &WebviewWindow, _: Rect) {}
    pub fn screens() -> Vec<Screen> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: Rect = Rect { x: 0.0, y: 0.0, w: 1500.0, h: 900.0 };

    fn r(x: f64, y: f64, w: f64, h: f64) -> Rect {
        Rect { x, y, w, h }
    }

    #[test]
    fn grows_from_the_side_it_is_nearest() {
        // Middle third: stays centered.
        assert_eq!(place(r(650.0, 800.0, 200.0, 40.0), 400.0, 40.0, SCREEN), r(550.0, 800.0, 400.0, 40.0));
        // Left third: keeps its left edge.
        assert_eq!(place(r(100.0, 800.0, 200.0, 40.0), 400.0, 40.0, SCREEN), r(100.0, 800.0, 400.0, 40.0));
        // Right third: keeps its right edge.
        assert_eq!(place(r(1200.0, 800.0, 200.0, 40.0), 400.0, 40.0, SCREEN), r(1000.0, 800.0, 400.0, 40.0));
    }

    #[test]
    fn keeps_its_top_edge_and_stays_on_screen() {
        assert_eq!(place(r(650.0, 800.0, 200.0, 40.0), 200.0, 50.0, SCREEN), r(650.0, 790.0, 200.0, 50.0));
        // Near the left edge a centered bar would spill off; it's pushed back on.
        assert_eq!(place(r(10.0, 100.0, 40.0, 40.0), 300.0, 40.0, SCREEN).x, 10.0);
        assert_eq!(place(r(600.0, 100.0, 300.0, 40.0), 2000.0, 40.0, SCREEN), r(8.0, 100.0, 1484.0, 40.0));
        // Dragged partly above the usable area, it comes back under it.
        assert_eq!(place(r(600.0, 880.0, 300.0, 40.0), 300.0, 40.0, SCREEN).y, 860.0);
    }

    #[test]
    fn starts_where_it_was_left_if_that_screen_is_still_there() {
        let main = Screen { frame: SCREEN, visible: r(0.0, 0.0, 1500.0, 875.0) };
        let left = Screen { frame: r(-1920.0, 0.0, 1920.0, 1080.0), visible: r(-1920.0, 0.0, 1920.0, 1055.0) };
        // Nothing saved: top center of the first screen, under the menu bar.
        assert_eq!(start_frame(None, &[main, left]), Some(r(630.0, 821.0, 240.0, 48.0)));
        // Saved on the other screen.
        let there = r(-1000.0, 500.0, 300.0, 48.0);
        assert_eq!(start_frame(Some(there), &[main, left]), Some(there));
        // That screen is gone: back to the default place, at the saved size.
        assert_eq!(start_frame(Some(there), &[main]), Some(r(600.0, 821.0, 300.0, 48.0)));
        assert_eq!(start_frame(None, &[]), None);
    }

    #[test]
    fn saved_state_reads_old_and_partial_files() {
        let s: Saved = serde_json::from_str("{}").unwrap();
        assert_eq!(s, Saved::default());
        let s: Saved = serde_json::from_str(r#"{"off":true,"frame":{"x":1,"y":2,"w":3,"h":4}}"#).unwrap();
        assert!(s.off);
        assert_eq!(s.frame, Some(r(1.0, 2.0, 3.0, 4.0)));
    }
}
