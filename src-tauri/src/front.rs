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

/// The scheme of Codealong's own links (`codealong://session/<key>`, src/codealongLinks.ts), which
/// the app's Info.plist claims.
pub const LINK_SCHEME: &str = "codealong";

/// The link macOS last asked Codealong to open, until the main window's page takes it.
static OPENED: Mutex<Option<String>> = Mutex::new(None);

/// The one of `urls` to show: the last that's Codealong's own and no longer than a link is.
fn link_among(urls: &[String]) -> Option<&String> {
    let ours = |u: &&String| u.len() <= 2048 && u.get(..LINK_SCHEME.len() + 3).is_some_and(|s| s.eq_ignore_ascii_case(&format!("{LINK_SCHEME}://")));
    urls.iter().rev().find(ours)
}

/// macOS asked Codealong to open these links (os::apps `opened_links`): a click on one in a
/// terminal or another app. The main window comes forward and its page is told there's one to
/// take (`take_opened_link`), which it also asks as it comes up, since a click that launched
/// the app comes before there's a page to tell. The page shows what the link names, as a click
/// on one in a conversation does: it knows the sessions and todos, here and on your other
/// devices. A link only ever shows something.
pub fn open_links(app: &AppHandle, urls: &[String]) {
    if !keep_link(urls) {
        return;
    }
    // No main window yet when the click is what launched the app: its page asks as it comes up.
    let _ = show_main(app, Some(serde_json::json!({ "kind": "link" })));
}

/// Keeps the one of `urls` to show for the main window to take; false when none is Codealong's.
fn keep_link(urls: &[String]) -> bool {
    let Some(link) = link_among(urls) else { return false };
    if let Ok(mut opened) = OPENED.lock() {
        *opened = Some(link.clone());
    }
    true
}

/// The link macOS asked for, once, for the page of the window labeled `window` to show: the main
/// window's alone, which is the one told of it. A project's window runs the same page, and would
/// take the link from under the main window as it opens.
pub fn take_opened_link(window: &str) -> Option<String> {
    if window != "main" {
        return None;
    }
    OPENED.lock().ok()?.take()
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
        let open = |label: &str| ["main", "project-acme-37036cd8"].contains(&label);
        assert_eq!(on_press(true, Some("project-acme-37036cd8".into()), open), Press::Hide);
        assert_eq!(on_press(false, Some("project-acme-37036cd8".into()), open), Press::Bring("project-acme-37036cd8".into()));
        // A project's window closed since, or none had the keyboard yet: the main window, which is
        // only ever hidden.
        assert_eq!(on_press(false, Some("project-gone-0a1b2c3d".into()), open), Press::Bring("main".into()));
        assert_eq!(on_press(false, None, open), Press::Bring("main".into()));
    }

    #[test]
    fn the_link_to_show_is_the_last_of_codealongs_own() {
        let urls = |list: &[&str]| list.iter().map(|u| u.to_string()).collect::<Vec<_>>();
        let both = urls(&["codealong://session/claude:claude:a", "file:///tmp/x", "Codealong://todo/t-1", "https://example.com/codealong://session/x"]);
        assert_eq!(link_among(&both).map(String::as_str), Some("Codealong://todo/t-1"));
        assert_eq!(link_among(&urls(&["https://example.com", "codealong:session/x", "codealong"])), None);
        // One too long to be a link is passed over.
        assert_eq!(link_among(&[format!("codealong://session/{}", "k".repeat(2048))]), None);
    }

    #[test]
    fn the_main_window_alone_takes_an_opened_link_once() {
        assert!(!keep_link(&["https://example.com".to_string()]));
        assert_eq!(take_opened_link("main"), None);
        assert!(keep_link(&["codealong://session/claude:claude:a".to_string()]));
        // A project's window asks as it opens too, and gets none.
        assert_eq!(take_opened_link("project-acme-37036cd8"), None);
        assert_eq!(take_opened_link("main").as_deref(), Some("codealong://session/claude:claude:a"));
        assert_eq!(take_opened_link("main"), None);
        // A later click replaces one not taken yet.
        assert!(keep_link(&["codealong://todo/t-1".to_string()]) && keep_link(&["codealong://todo/t-2".to_string()]));
        assert_eq!(take_opened_link("main").as_deref(), Some("codealong://todo/t-2"));
    }
}
