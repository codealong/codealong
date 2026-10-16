//! What's in front: Codealong's main window brought forward, another app asked to come forward,
//! and whether an app is the active one. Most of Codealong's windows are panels that leave it
//! inactive when clicked (panel.rs), so after one of them opens something, Codealong has to ask
//! macOS for what a click in an ordinary window gets by itself. The `[shortcuts] main_window`
//! shortcut brings back the board window you were in last, and hides Codealong from there
//! (`toggle_board`).
//!
//! Apps are known by bundle ID, which only macOS has (os::apps): on Linux no other app is asked
//! to come forward or said to be active, and the window manager raises what a click opens.

use std::sync::Mutex;

use anyhow::{anyhow, Result};
use tauri::{AppHandle, Emitter, Manager, WebviewWindow, Window, WindowEvent};

/// The board window that had the keyboard last, by label: the main window or a project's.
static LAST_BOARD: Mutex<Option<String>> = Mutex::new(None);

/// Brings the main window forward. `reveal` (a session or PR) goes to its page, which selects it.
pub fn show_main(app: &AppHandle, reveal: Option<serde_json::Value>) -> Result<()> {
    let main = app.get_webview_window("main").ok_or_else(|| anyhow!("The main window is gone."))?;
    bring(&main)?;
    if let Some(r) = reveal {
        app.emit_to("main", "reveal", r)?;
    }
    Ok(())
}

/// Brings a window forward with the keyboard, out of the Dock if it was minimized, and Codealong
/// with it.
fn bring(window: &WebviewWindow) -> Result<()> {
    let _ = window.unminimize();
    window.show()?;
    window.set_focus()?;
    Ok(())
}

/// What `[shortcuts] main_window` does.
#[derive(Debug, PartialEq)]
enum Press {
    /// Hides Codealong, as ⌘H does.
    Hide,
    /// Brings this board window forward.
    Bring(String),
}

/// A board window is the one you're in: Codealong hides, and macOS gives the keyboard to the app
/// you were in before. Otherwise the board window you were in last comes forward, or the main
/// window, which closing only hides, when that one's been closed since or there's none yet.
fn on_press(board_in_front: bool, last: Option<String>, open: impl Fn(&str) -> bool) -> Press {
    if board_in_front {
        return Press::Hide;
    }
    Press::Bring(last.filter(|l| open(l)).unwrap_or_else(|| "main".to_string()))
}

/// `[shortcuts] main_window` was pressed. Hiding the app leaves the floating bar, the quick panel
/// and the cards pinned to the desktop up: panels can't hide (panel.rs `place`). On the main
/// thread.
pub fn toggle_board(app: &AppHandle) {
    let last = LAST_BOARD.lock().ok().and_then(|l| l.clone());
    match on_press(crate::front_window(app).is_some(), last, |label| app.get_webview_window(label).is_some()) {
        Press::Hide => hide(app),
        Press::Bring(label) => {
            if let Some(w) = app.get_webview_window(&label) {
                let _ = bring(&w);
            }
        }
    }
}

/// Hides Codealong, as ⌘H does; where the OS hides no app, its board windows.
fn hide(app: &AppHandle) {
    if crate::os::apps::hide(app) {
        return;
    }
    for (label, w) in app.webview_windows() {
        if crate::shows_board(&label) {
            let _ = w.hide();
        }
    }
}

/// Keeps which board window had the keyboard last, for `toggle_board`. On the main thread.
pub fn on_window_event(window: &Window, event: &WindowEvent) {
    if let (WindowEvent::Focused(true), true) = (event, crate::shows_board(window.label())) {
        if let Ok(mut last) = LAST_BOARD.lock() {
            *last = Some(window.label().to_string());
        }
    }
}

/// After a panel opened something, asks macOS to bring that app forward. A click in the bar leaves
/// Codealong inactive, and macOS may refuse an inactive app; the open itself has already happened.
pub fn bring_forward(app: &AppHandle, bundle_id: &'static str) {
    let _ = app.run_on_main_thread(move || crate::os::apps::activate(bundle_id));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shortcut_hides_codealong_or_brings_back_the_board_window_used_last() {
        let open = |label: &str| ["main", "project-kestrel-6f1c0e2a"].contains(&label);
        assert_eq!(on_press(true, Some("project-kestrel-6f1c0e2a".into()), open), Press::Hide);
        assert_eq!(on_press(false, Some("project-kestrel-6f1c0e2a".into()), open), Press::Bring("project-kestrel-6f1c0e2a".into()));
        // A project's window closed since, or none had the keyboard yet: the main window, which is
        // only ever hidden.
        assert_eq!(on_press(false, Some("project-gone-0a1b2c3d".into()), open), Press::Bring("main".into()));
        assert_eq!(on_press(false, None, open), Press::Bring("main".into()));
    }
}
