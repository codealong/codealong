//! Glass (`[appearance] glass`, Settings › Appearance, on by default): the board's windows (the main
//! one and a project's) show what's behind them through macOS's own material, as the Finder's
//! sidebar does. It's Tauri's window effects (window-vibrancy): Liquid Glass (NSGlassEffectView)
//! from macOS 26, and before it, where that isn't there, the sidebar's vibrancy
//! (NSVisualEffectView). Either is a view put behind the page, which shows only where the window
//! and the page draw nothing: so the window is made see-through, as Tauri's `transparent` makes
//! one at its start, and the page draws what was gray see-through with the board's `glass` (the
//! `is-glass` class, `src/glass.ts`).
//!
//! As the zoom is (zoom.rs), it follows config.toml: the engine puts it on the board, the board's
//! publisher sets it on the windows (`follow`), and a project window just made gets it (`fit`). A
//! window that never had it is never touched, so with it off nothing is as it was before it.
//! macOS only: Linux has no material behind a window, and Windows' Mica hasn't been tried, so the
//! page never draws see-through there either.
//!
//! The quick panel has glass too, only under its card, as its window has room round the card for
//! the card's shadow that the glass mustn't frost (`panel::set_glass_under`, quick.rs
//! `fit_glass`); its page lets it through at the bar along the card's foot.

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::window::{Color, Effect, EffectsBuilder};
use tauri::Manager;

use crate::os::{self, Os};

/// Whether the board's windows have glass now.
static SHOWN: AtomicBool = AtomicBool::new(false);

/// Puts glass on every board window, and under the quick panel's card, or takes it off, when they
/// don't have it that way.
pub fn follow(app: &tauri::AppHandle, on: bool) {
    if SHOWN.swap(on, Ordering::AcqRel) == on {
        return;
    }
    for (label, w) in app.webview_windows() {
        if crate::shows_board(&label) {
            set(&w, on);
        }
    }
    crate::quick::fit_glass(app);
}

/// Whether the windows have glass now.
pub fn shown() -> bool {
    SHOWN.load(Ordering::Acquire)
}

/// Gives a board window just made the glass the others have.
pub fn fit(window: &tauri::WebviewWindow) {
    if SHOWN.load(Ordering::Acquire) {
        set(window, true);
    }
}

fn set(window: &tauri::WebviewWindow, on: bool) {
    match os::current() {
        Os::MacOs => {
            // The window's background and the webview's: clear, or back as they were. Tauri sets
            // both, the webview's through WebKit's `drawsBackground`.
            if let Err(e) = window.set_background_color(on.then_some(Color(0, 0, 0, 0))) {
                eprintln!("Couldn't make {} see-through: {e}", window.label());
            }
            os::windows::set_opaque(window, !on);
            // The first of the two that this macOS has: Liquid Glass falls back to the vibrancy.
            let effects = on.then(|| EffectsBuilder::new().effects([Effect::LiquidGlassRegular, Effect::Sidebar]).build());
            if let Err(e) = window.set_effects(effects) {
                eprintln!("Couldn't {} {}: {e}", if on { "put glass on" } else { "take glass off" }, window.label());
            }
        }
        // See the module's header: the page stays opaque there too (`src/glass.ts`).
        Os::Linux | Os::Windows => {}
    }
}
