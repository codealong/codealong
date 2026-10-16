//! The panels: the one place Codealong talks to the window system about the windows that float
//! over other apps. The bar, its peek, the quick panel, the sessions kept on the desktop, the
//! cards pinned there and the lines between them are each a Tauri window made into a panel here:
//! on macOS a non-activating NSPanel, the only kind macOS puts on another app's full-screen Space
//! (`appkit`), and on Linux a borderless window kept on top through Tauri and GTK (`gtk`). Here is
//! how one is made, shown, given the keyboard and has it taken, hidden, and given back its class
//! before it's destroyed, with the level each floats at (`Level`), which the outline flash.rs
//! draws goes by too. A window's own module says what kind of panel it is (`Config`) and when
//! each of these happens; it never reaches AppKit or GTK itself.
//!
//! Nearly everything in `appkit` is odd for a reason, found by the plain way failing, and much of
//! it can only be checked by hand in the running app: `.claude/rules/windows.md` says what was,
//! when, and what hasn't been yet.
//!
//! All of it is the main thread's. Off it these functions do nothing and answer nothing, and a
//! debug build stops to say so (`main_thread`).

#[cfg(target_os = "macos")]
mod appkit;
#[cfg(target_os = "macos")]
pub use appkit::*;
#[cfg(not(target_os = "macos"))]
mod gtk;
#[cfg(not(target_os = "macos"))]
pub use self::gtk::*;

use std::time::Duration;

use tauri::{AppHandle, WebviewUrl, WebviewWindowBuilder, Wry};

use crate::geometry::Screen;

/// How often a window that stays up checks that it is (macOS can leave one off the Space in
/// front) and writes down where it was moved to: the bar, the cards and the sessions kept on the
/// desktop.
pub const CHECK_EVERY: Duration = Duration::from_secs(1);
/// How often a window dragged from here is put where the pointer has taken it: a card dragged
/// onto the desktop, the peek dragged by its header.
pub const DRAG_EVERY: Duration = Duration::from_millis(16);

/// The level a window floats at, lowest first. Two windows at one level go by which was ordered
/// front last.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    /// The lines between cards: just under the cards pinned to the desktop (and the floating
    /// bar), over other apps' windows.
    Lines,
    /// A card pinned to the desktop: the status item level, as the floating bar.
    Card,
    /// A session kept on the desktop: with the cards there.
    Kept,
    /// The floating bar: the status item level, still under menus. `always_on_top` alone is
    /// AppKit's floating level, where other apps' floating panels can cover a window.
    FloatingBar,
    /// The outline round a terminal that came forward on another screen (flash.rs): with the
    /// floating bar. Only macOS draws one.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    Outline,
    /// The bar's peek: over the floating bar it opens by.
    Peek,
    /// The bar docked in the notch: over the menu bar (level 24) and its status items (25), as
    /// the notch apps do.
    DockedBar,
    /// The quick panel: above other apps' windows and the bar, floating or docked. The docked
    /// bar's window reaches well down the screen while it shows a session, so the panel is one
    /// above that: opened from a session in the notch, it's over it.
    Quick,
}


/// How a panel answers AppKit about the keyboard. None ever becomes the main window, so a board
/// window behind one stays the one menu commands are for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Keys {
    /// Key only while it's asked to be (`take_keys`), so the board keeps the keyboard and its
    /// place, but it says it's key when asked. WebKit treats a page whose window isn't key as
    /// inactive: it sends such a page's mouse moves only to scrollbars, so pills never showed
    /// hover, `:hover` stayed on whatever was last clicked, and no tooltip ever appeared. Keys
    /// still go to the app's real key window, and menu commands skip these windows
    /// (`front_window` in lib.rs). While it has the keyboard or is taking it, it answers
    /// truthfully, so AppKit sees a panel that isn't key yet and makes it so. The bar is asked
    /// only docked, while it shows a session to answer; floating, it never is, nor are the cards
    /// on the desktop and the lines between them.
    WhenAsked,
    /// Key whenever AppKit offers, which a click in it does, and while it isn't it says it is, as
    /// the floating bar's does and for the same reason: its page would otherwise get no pointer
    /// moves, so nothing in it would show hover and no tooltip would appear. Taking the keyboard
    /// (`take_keys`, `present`) turns that off first, so AppKit sees the panel isn't key yet and
    /// makes it so. The peek and the sessions kept on the desktop.
    OnClick,
    /// Key whenever AppKit offers, and truthful about it always: the quick panel, which takes
    /// the keyboard as it shows and hides when it loses it.
    Always,
}

/// Which Spaces a panel is on. All are on every Space and over full-screen apps, and kept out
/// of ⌘`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spaces {
    /// Left in place by Mission Control: the windows that stay up.
    Stationary,
    /// Coming and going: Mission Control doesn't show it as a window of its own.
    Transient,
}


/// The steps `make` takes. The panels were made by copies of one piece of code before they
/// shared this one, and two of the copies told AppKit the same things in different orders.
/// Nobody could check the windows by hand when they were folded together (2026-10-02), so each
/// window is still made exactly as it was; either order should do for any of them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Steps {
    /// As bar.rs made the bar, the cards and the lines: first the panel, which says at once that
    /// it allows tooltips, then its level and Spaces, which a window that couldn't be made a
    /// panel gets too, as the plain window it is.
    Two,
    /// As peek.rs made the peek and the sessions kept on the desktop, and quick.rs the quick
    /// panel: all of it in one, with tooltips allowed last, and none of it for a window that
    /// couldn't be made a panel.
    One,
}

/// What kind of panel a window is made into (`make`). GTK's windows take none of it: every one is
/// on top, and the window manager says which has the keyboard (panel/gtk.rs).
#[derive(Clone, Copy, Debug)]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub struct Config {
    pub keys: Keys,
    /// The level it starts at (`set_level` moves it).
    pub level: Level,
    pub spaces: Spaces,
    /// Where it's put is where it goes: AppKit doesn't move its frame onto the screen. Docked,
    /// the bar sits over the menu bar, which AppKit would otherwise keep a window below.
    pub unconstrained: bool,
    /// Tooltips show in it while Codealong is in the background, which is nearly always while
    /// one of these is up: macOS shows a background app's tooltips only in windows that allow
    /// them.
    pub tooltips: bool,
    pub steps: Steps,
}

/// What a window's page is, and what a click does to it, for `builder`.
pub struct Page<'a> {
    pub title: &'a str,
    /// Run before the page's own scripts: tells index.html which window it's in.
    pub script: &'a str,
    /// The window's size before its page has measured itself.
    pub size: (f64, f64),
    /// A click may give it the keyboard, as tao sees it: false for the windows that never take
    /// it on a click.
    pub focusable: bool,
    /// The click that would bring it forward counts as a click in the page.
    pub first_mouse: bool,
}

/// The window every panel starts as: borderless, clear, no shadow of its own, fixed in size, on
/// top and on every Space, not focused and not yet visible. It's shown only once it's a panel
/// (`make`): a window once shown as a window stays off full-screen Spaces after the swap.
pub fn builder<'a>(app: &'a AppHandle, label: &str, page: Page<'_>) -> WebviewWindowBuilder<'a, Wry, AppHandle> {
    let mut builder = WebviewWindowBuilder::new(app, label, WebviewUrl::App("index.html".into()))
        .title(page.title)
        .initialization_script(page.script)
        .inner_size(page.size.0, page.size.1)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .always_on_top(true)
        .visible_on_all_workspaces(true)
        .skip_taskbar(true);
    if !page.focusable {
        builder = builder.focusable(false);
    }
    builder = builder.focused(false);
    if page.first_mouse {
        builder = builder.accept_first_mouse(true);
    }
    builder
        // WebKit slows a page's timers and drawing while its app isn't frontmost, which for a
        // window that's up over other apps is nearly always: updates and hovers came late.
        .background_throttling(tauri::utils::config::BackgroundThrottlingPolicy::Disabled)
        .visible(false)
}

/// Looks at something every `every` for as long as `going` says to: the pointer, while a window
/// follows it or waits for it to leave. The waiting is on a thread of its own and each look,
/// `step`, on the main thread, where the pointer and the windows are read. The first look is one
/// wait from now. It ends when `going` says no, which is asked before each look, or once the app
/// takes nothing more for its main thread.
pub fn poll(app: &AppHandle, every: Duration, going: impl Fn(&AppHandle) -> bool + Send + 'static, step: impl Fn(&AppHandle) + Send + Copy + 'static) {
    let handle = app.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(every);
        if !going(&handle) {
            break;
        }
        let app = handle.clone();
        if handle.run_on_main_thread(move || step(&app)).is_err() {
            break;
        }
    });
}

/// The top of a screen, as a notch is found from it: the safe area the screen keeps from its
/// top, and how wide the menu bar is on either side of the camera (`auxiliaryTopLeftArea` and
/// `…RightArea`). A screen without a notch has no safe area and empty sides.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Top {
    pub screen: Screen,
    pub safe_top: f64,
    pub sides: (f64, f64),
}
