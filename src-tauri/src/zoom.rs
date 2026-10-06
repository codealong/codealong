//! Zooming the board's windows as VS Code and a browser do: ⌘= (or ⌘+) bigger, ⌘- smaller, ⌘0
//! back to actual size, from the page's keys (`src/mainKeys.ts`) or the View menu. Every board
//! window (the main one and a project's) draws at one zoom, kept in config.toml as `[appearance]
//! zoom`, a percent, so it lasts and can be written by hand.
//!
//! A step reads the file, moves to the next of `ZOOMS` and writes it back (`config::step_zoom`),
//! then the engine puts it on the board, and the board's publisher sets it on the windows
//! (`follow`): so a change by hand in config.toml reaches them the same way. It's the webview's
//! own page zoom (WKWebView's `pageZoom`), as a browser's ⌘+ is: the page lays out in fewer CSS
//! pixels and everything in it agrees, events and `devicePixelRatio` too (2.5 on a Retina screen
//! at 125%, checked in a WKWebView on 2026-10-06). A spot the page hands the window in points is
//! scaled where it crosses (`src/api.ts` `popupMenu`).
//!
//! The windows that float (the bar, the notch, the peek, the quick panel, the waiting window,
//! cards on the desktop) size themselves to their pages in points, so they stay at actual size.

use std::sync::atomic::{AtomicU32, Ordering};

use serde::Deserialize;
use tauri::{Emitter, Manager};

/// Actual size, in percent.
pub const ACTUAL: u32 = 100;
/// What ⌘= and ⌘- go through, in percent: a browser's steps, fine near actual size.
pub const ZOOMS: [u32; 11] = [50, 67, 75, 80, 90, ACTUAL, 110, 125, 150, 175, 200];
pub const MIN: u32 = ZOOMS[0];
pub const MAX: u32 = ZOOMS[ZOOMS.len() - 1];

/// Which way a key or the menu zooms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Step {
    In,
    Out,
    Actual,
}

/// The zoom a step goes to from `now`: the next of `ZOOMS` up or down, so a zoom written by hand
/// between two goes to the nearer one that way, and no further than the ends.
pub fn stepped(now: u32, step: Step) -> u32 {
    match step {
        Step::Actual => ACTUAL,
        Step::In => ZOOMS.into_iter().find(|&z| z > now).unwrap_or(MAX),
        Step::Out => ZOOMS.into_iter().rev().find(|&z| z < now).unwrap_or(MIN),
    }
}

/// The zoom the board's windows draw at now; 0 before the first is set.
static SHOWN: AtomicU32 = AtomicU32::new(0);

/// Sets `percent` on every board window, when it isn't the zoom they have.
pub fn follow(app: &tauri::AppHandle, percent: u32) {
    let percent = percent.clamp(MIN, MAX);
    if SHOWN.swap(percent, Ordering::AcqRel) == percent {
        return;
    }
    for (label, w) in app.webview_windows() {
        if crate::shows_board(&label) {
            set(&w, percent);
        }
    }
}

/// Sets the board windows' zoom on a board window just made.
pub fn fit(window: &tauri::WebviewWindow) {
    let percent = SHOWN.load(Ordering::Acquire);
    if percent != 0 {
        set(window, percent);
    }
}

fn set(window: &tauri::WebviewWindow, percent: u32) {
    if let Err(e) = window.set_zoom(f64::from(percent) / 100.0) {
        eprintln!("Couldn't zoom {}: {e}", window.label());
    }
}

/// Puts Actual size (⌘0), Zoom in (⌘=) and Zoom out (⌘-) at the top of the View menu, as a
/// browser has them. Choosing one asks the front board window to zoom, as its keys do, so what
/// goes wrong shows there; with none in front there's nothing to zoom. The page has the keys
/// before the menu does: WebKit sends a key equivalent to the page first, and to the menu only
/// when the page doesn't prevent it (checked in a WKWebView on 2026-10-06). So a press zooms
/// once, and the canvas keeps ⌘= and ⌘- for its own zoom.
pub fn add_menu(app: &tauri::AppHandle) -> tauri::Result<()> {
    use tauri::menu::{MenuItem, MenuItemKind, PredefinedMenuItem};
    let Some(menu) = app.menu() else { return Ok(()) };
    let view = menu.items()?.into_iter().find_map(|item| match item {
        MenuItemKind::Submenu(s) if s.text().is_ok_and(|t| t == "View") => Some(s),
        _ => None,
    });
    let Some(view) = view else { return Ok(()) };
    let actual = MenuItem::with_id(app, MENU_ACTUAL, "Actual size", true, Some("CmdOrCtrl+0"))?;
    let bigger = MenuItem::with_id(app, MENU_IN, "Zoom in", true, Some("CmdOrCtrl+="))?;
    let smaller = MenuItem::with_id(app, MENU_OUT, "Zoom out", true, Some("CmdOrCtrl+-"))?;
    view.prepend_items(&[&actual, &bigger, &smaller, &PredefinedMenuItem::separator(app)?])?;
    app.on_menu_event(|app, event| {
        let step = match event.id().as_ref() {
            MENU_ACTUAL => "actual",
            MENU_IN => "in",
            MENU_OUT => "out",
            _ => return,
        };
        if let Some(w) = crate::front_window(app) {
            // `emit_to`, not a window's `emit`: that one reaches every window.
            let _ = app.emit_to(w.label(), "zoom", step);
        }
    });
    Ok(())
}

const MENU_ACTUAL: &str = "zoom-actual";
const MENU_IN: &str = "zoom-in";
const MENU_OUT: &str = "zoom-out";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_go_through_the_zooms_and_stop_at_the_ends() {
        assert_eq!(stepped(100, Step::In), 110);
        assert_eq!(stepped(100, Step::Out), 90);
        assert_eq!(stepped(110, Step::Actual), 100);
        assert_eq!(stepped(MAX, Step::In), MAX);
        assert_eq!(stepped(MIN, Step::Out), MIN);
    }

    #[test]
    fn a_zoom_written_by_hand_steps_to_the_nearer_one_that_way() {
        assert_eq!(stepped(115, Step::In), 125);
        assert_eq!(stepped(115, Step::Out), 110);
        assert_eq!(stepped(300, Step::Out), MAX);
        assert_eq!(stepped(10, Step::In), MIN);
    }
}
