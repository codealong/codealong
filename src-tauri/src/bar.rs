//! The floating bar: a small window, on top of every other and on every Space, that shows the
//! agents that need you and the ones running, PRs to review and todos ready to start. It's up
//! unless you turn it off, from the toggle at the top right of the board, the bar's own menu, or
//! the Window menu.
//!
//! It floats where you put it, or docks in the notch: at the top of the screen, around the
//! camera, as one black shape with the notch that shows the mark and what needs you most, and
//! opens down into the bar's pills when you point at it (Bar.tsx). A screen without a notch gets
//! one drawn at the top center of its menu bar. Docked, the window sits above the menu bar and
//! stays at the size the shape opens to, so opening and closing are the page's animation alone:
//! a window resized as it opens shows its old picture for a frame at the wrong place, since
//! WebKit draws in a process of its own. A web page takes the clicks on its clear parts too, so
//! the window takes clicks only while the pointer is on the shape the page says it shows
//! (`notch_shape`); elsewhere they go through to the menu bar and the apps under it.
//!
//! It's a non-activating panel: macOS lets only those onto another app's full-screen Space. So a
//! click in the bar doesn't make Codealong the active app, and it never takes the keyboard. After
//! it opens a session, Codealong asks macOS to bring that app forward (`bring_forward`).
//!
//! Closing the main window hides it and the app keeps running with the bar; the Dock icon and the
//! bar's mark bring it back. Whether the bar is on and where you left it are kept in `bar.json`
//! next to the config.

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
const NOTCH_MENU_ID: &str = "notch-bar";
/// How often the bar checks that it's up (when on) and writes down where it was moved to.
const CHECK_EVERY: Duration = Duration::from_secs(1);
/// How often the pointer is looked at while the bar is docked, to take clicks on the shape only.
const POINTER_EVERY: Duration = Duration::from_millis(40);
/// The bar's size before its page has measured itself.
const START: (f64, f64) = (240.0, 48.0);
/// Space between a newly placed bar and the menu bar.
pub(crate) const TOP_GAP: f64 = 6.0;
/// Room kept at the sides of the screen.
pub(crate) const EDGE: f64 = 8.0;
/// The width of the notch drawn on a screen without one: about the hardware's on a 14-inch
/// MacBook Pro (185 points).
const DRAWN_NOTCH_WIDTH: f64 = 180.0;
/// The drawn notch's height where the menu bar is hidden and says nothing of its own.
const DRAWN_NOTCH_HEIGHT: f64 = 24.0;

/// Where the bar shows: floating where you put it, docked in the notch, or not at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Off,
    Floating,
    Notch,
}

/// The notch the bar docks into, in screen points: its center, the top of its screen, its size,
/// and whether the screen has one or the bar draws it.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Notch {
    pub x: f64,
    pub top: f64,
    pub width: f64,
    pub height: f64,
    pub real: bool,
}

/// What the bar's page is told: where it shows, and the notch it docks into.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModeView {
    pub mode: Mode,
    pub notch: Option<Notch>,
}

/// A screen's notch: between the parts of the menu bar AppKit gives on either side of the camera
/// (`auxiliaryTopLeftArea` and `…RightArea`), as tall as the safe area it keeps from the top. A
/// screen without one gets one drawn at the top center, as tall as its menu bar.
pub fn notch_of(frame: Rect, visible: Rect, safe_top: f64, sides: Option<(f64, f64)>) -> Notch {
    match sides.filter(|(l, r)| safe_top > 0.0 && *l > 0.0 && *r > 0.0) {
        Some((left, right)) => {
            let (from, to) = (frame.x + left, frame.right() - right);
            Notch { x: (from + to) / 2.0, top: frame.top(), width: to - from, height: safe_top, real: true }
        }
        None => {
            let menu = frame.top() - visible.top();
            let height = if menu > 5.0 { menu } else { DRAWN_NOTCH_HEIGHT };
            Notch { x: frame.x + frame.w / 2.0, top: frame.top(), width: DRAWN_NOTCH_WIDTH, height, real: false }
        }
    }
}

/// The docked bar's frame at a size: centered on the notch, its top at the top of the screen.
pub fn notch_frame(n: Notch, w: f64, h: f64) -> Rect {
    Rect { x: (n.x - w / 2.0).round(), y: n.top - h, w, h }
}

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
    pub(crate) fn top(&self) -> f64 {
        self.y + self.h
    }
    pub(crate) fn right(&self) -> f64 {
        self.x + self.w
    }
    pub(crate) fn center(&self) -> (f64, f64) {
        (self.x + self.w / 2.0, self.y + self.h / 2.0)
    }
    pub(crate) fn contains(&self, (x, y): (f64, f64)) -> bool {
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
pub(crate) fn clamp(r: Rect, screen: Rect) -> Rect {
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
pub(crate) fn screen_for(r: Rect, screens: &[Screen]) -> Option<Rect> {
    screens.iter().find(|s| s.frame.contains(r.center())).or(screens.first()).map(|s| s.visible)
}

/// What `bar.json` keeps.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Saved {
    /// Turned off from the bar's menu or the Window menu.
    #[serde(default)]
    off: bool,
    /// Docked in the notch rather than floating; kept while it's off, for when it's back on.
    #[serde(default)]
    notch: bool,
    /// Where the bar was last.
    #[serde(default)]
    frame: Option<Rect>,
}

impl Saved {
    fn mode(&self) -> Mode {
        match (self.off, self.notch) {
            (true, _) => Mode::Off,
            (false, true) => Mode::Notch,
            (false, false) => Mode::Floating,
        }
    }

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
    /// The feed last sent to the page, as sent, so an unchanged one isn't sent again.
    sent: Mutex<String>,
    /// The bar moved since `bar.json` was written.
    moved: AtomicBool,
    /// Window › Floating bar and Window › Bar in the notch.
    item: Mutex<Option<CheckMenuItem<Wry>>>,
    notch_item: Mutex<Option<CheckMenuItem<Wry>>>,
    /// The notch the page was last told of, while docked.
    notch: Mutex<Option<Notch>>,
    /// The shape the docked bar shows, in its window's points from the top left (`notch_shape`),
    /// and whether the pointer was last on it.
    shape: Mutex<Option<Rect>>,
    on_shape: AtomicBool,
}

/// Makes the bar, shows it unless it's turned off, and adds Window › Floating bar.
pub fn setup(app: &AppHandle) -> Result<()> {
    let saved = Saved::load();
    let (mode, frame) = (saved.mode(), saved.frame);
    app.manage(Bar {
        saved: Mutex::new(saved),
        sent: Mutex::new(String::new()),
        moved: AtomicBool::new(false),
        item: Mutex::new(None),
        notch_item: Mutex::new(None),
        notch: Mutex::new(None),
        shape: Mutex::new(None),
        // A new window takes clicks everywhere: the first look lets them through off the shape.
        on_shape: AtomicBool::new(true),
    });

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
        // WebKit slows a page's timers and drawing while its app isn't frontmost, which for a
        // bar that's up over other apps is nearly always: updates and hovers came late.
        .background_throttling(tauri::utils::config::BackgroundThrottlingPolicy::Disabled)
        .visible(false)
        .build()?;
    let _ = native::make_panel(&window);
    native::prepare(&window);
    if let Some(r) = start_frame(frame, &native::screens()) {
        native::set_frame(&window, r);
    }
    if mode != Mode::Off {
        place_for(app, &window, mode);
        native::show(&window);
    }

    let item = CheckMenuItem::with_id(app, MENU_ID, "Floating bar", true, mode == Mode::Floating, None::<&str>)?;
    let notch_item = CheckMenuItem::with_id(app, NOTCH_MENU_ID, "Bar in the notch", true, mode == Mode::Notch, None::<&str>)?;
    if let Some(MenuItemKind::Submenu(menu)) = app.menu().and_then(|m| m.get(WINDOW_SUBMENU_ID)) {
        menu.append(&PredefinedMenuItem::separator(app)?)?;
        menu.append(&item)?;
        menu.append(&notch_item)?;
    }
    if let Ok(mut slot) = app.state::<Bar>().item.lock() {
        *slot = Some(item);
    }
    if let Ok(mut slot) = app.state::<Bar>().notch_item.lock() {
        *slot = Some(notch_item);
    }
    app.on_menu_event(|app, event| {
        // Picking the checked place turns the bar off; picking the other moves it there.
        let picked = match event.id().as_ref() {
            MENU_ID => Mode::Floating,
            NOTCH_MENU_ID => Mode::Notch,
            _ => return,
        };
        set_mode(app, if mode_of(app) == picked { Mode::Off } else { picked });
    });

    let handle = app.clone();
    std::thread::Builder::new().name("codealong-bar".into()).spawn(move || loop {
        std::thread::sleep(CHECK_EVERY);
        let app = handle.clone();
        if handle.run_on_main_thread(move || keep_up(&app)).is_err() {
            break;
        }
    })?;
    let handle = app.clone();
    std::thread::Builder::new().name("codealong-notch".into()).spawn(move || loop {
        std::thread::sleep(POINTER_EVERY);
        if mode_of(&handle) != Mode::Notch {
            continue;
        }
        let app = handle.clone();
        if handle.run_on_main_thread(move || watch_pointer(&app)).is_err() {
            break;
        }
    })?;
    Ok(())
}

/// Whether a point on the screen is on the docked bar's shape, given where its window is and
/// the shape in the window's points from its top left.
pub fn on_shape(p: (f64, f64), window: Rect, shape: Rect) -> bool {
    let (x, y) = (p.0 - window.x, window.top() - p.1);
    x >= shape.x && x <= shape.x + shape.w && y >= shape.y && y <= shape.y + shape.h
}

/// Docked: the window takes clicks while the pointer is on the shape and lets them through
/// elsewhere, and the page hears when the pointer comes onto the shape and leaves it, since a
/// window letting clicks through tells its page nothing. On the main thread.
fn watch_pointer(app: &AppHandle) {
    let (Some(bar), Some(state)) = (app.get_webview_window(LABEL), app.try_state::<Bar>()) else {
        return;
    };
    let (Some(frame), Some(p), Some(shape)) = (native::frame(&bar), native::pointer(), state.shape.lock().ok().and_then(|s| *s)) else {
        return;
    };
    let on = on_shape(p, frame, shape);
    if state.on_shape.swap(on, Ordering::Relaxed) != on {
        native::set_clickable(&bar, on);
        let _ = app.emit_to(LABEL, "notch-pointer", on);
    }
}

/// The shape the docked bar's page shows now, in the window's points from its top left.
pub fn set_shape(app: &AppHandle, shape: Rect) {
    let Some(state) = app.try_state::<Bar>() else { return };
    if let Ok(mut s) = state.shape.lock() {
        *s = Some(shape);
    }
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || watch_pointer(&handle));
}

/// Puts the bar back up if it's on but not on screen, and writes down where it is after a move (a
/// drag moves it many times a second). On the main thread.
fn keep_up(app: &AppHandle) {
    let (Some(bar), Some(state)) = (app.get_webview_window(LABEL), app.try_state::<Bar>()) else {
        return;
    };
    let mode = state.saved.lock().map(|s| s.mode()).unwrap_or(Mode::Off);
    if mode != Mode::Off && !native::on_screen(&bar) {
        native::show(&bar);
    }
    // Docked, the bar follows the notch: a display plugged in or out, the lid closed, the menu
    // bar's height changed.
    if mode == Mode::Notch {
        let now = native::notch();
        let before = state.notch.lock().ok().and_then(|n| *n);
        if now != before {
            place_for(app, &bar, Mode::Notch);
            let _ = app.emit_to(LABEL, "bar-mode", ModeView { mode, notch: now });
        }
    }
    if state.moved.swap(false, Ordering::Relaxed) {
        if let Ok(saved) = state.saved.lock() {
            saved.save();
        }
    }
}

/// Window events that concern the bar: closing the main window hides it instead, and a moved bar
/// is remembered. On the main thread.
pub fn on_window_event(window: &Window, event: &WindowEvent) {
    match event {
        WindowEvent::CloseRequested { api, .. } if window.label() == "main" => {
            api.prevent_close();
            let _ = window.hide();
        }
        WindowEvent::Moved(_) if window.label() == LABEL => {
            let (Some(state), Some(bar)) = (window.try_state::<Bar>(), window.app_handle().get_webview_window(LABEL)) else {
                return;
            };
            // Where it floats is what's remembered; docked, it's where the notch is.
            if state.saved.lock().map(|s| s.mode() != Mode::Floating).unwrap_or(true) {
                return;
            }
            let Some(frame) = native::frame(&bar) else { return };
            if let Ok(mut saved) = state.saved.lock() {
                saved.frame = Some(frame);
                state.moved.store(true, Ordering::Relaxed);
            };
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
        let next = match (mode_of(&handle), native::notch()) {
            (Mode::Notch, Some(n)) => notch_frame(n, width.ceil(), height.ceil()),
            _ => {
                let Some(screen) = screen_for(current, &native::screens()) else { return };
                place(current, width.ceil(), height.ceil(), screen)
            }
        };
        if next != current {
            native::set_frame(&bar, next);
        }
    });
}

/// Sends the bar its feed after the engine publishes, if what it shows changed. The bar doesn't
/// listen for the board, so Tauri doesn't send it the half megabyte each publish. The board's
/// time changes with every publish and shows nothing, so it alone doesn't send a feed.
pub fn publish(app: &AppHandle, board: &crate::model::Board) {
    let Some(state) = app.try_state::<Bar>() else { return };
    let mut feed = crate::barfeed::feed(board);
    let at = std::mem::take(&mut feed.generated_at);
    let Ok(text) = serde_json::to_string(&feed) else { return };
    feed.generated_at = at;
    {
        let Ok(mut sent) = state.sent.lock() else { return };
        if *sent == text {
            return;
        }
        *sent = text;
    }
    let _ = app.emit_to(LABEL, "bar-feed", feed);
}

/// The bar's window frame, in screen points. On the main thread.
pub fn frame(app: &AppHandle) -> Option<Rect> {
    native::frame(&app.get_webview_window(LABEL)?)
}

/// The room the floating bar's page leaves around its card, for its shadow (Bar.tsx `MARGIN`).
pub(crate) const CARD_MARGIN: f64 = 12.0;

/// The frame the peek opens by (peek.rs `place`): the floating bar's window, whose card sits
/// `CARD_MARGIN` inside it; docked, the shape the page shows with that margin around it, since
/// the window keeps room for the shape's shadow as well. On the main thread.
pub fn peek_frame(app: &AppHandle) -> Option<Rect> {
    let frame = frame(app)?;
    let shape = app.try_state::<Bar>().and_then(|s| s.shape.lock().ok().and_then(|s| *s));
    match (mode_of(app), shape) {
        (Mode::Notch, Some(s)) => Some(shape_frame(frame, s)),
        _ => Some(frame),
    }
}

/// A docked shape, given in its window's points from the top left, on the screen with the
/// floating card's margin around it.
pub fn shape_frame(window: Rect, shape: Rect) -> Rect {
    Rect { x: window.x + shape.x - CARD_MARGIN, y: window.top() - shape.y - shape.h - CARD_MARGIN, w: shape.w + 2.0 * CARD_MARGIN, h: shape.h + 2.0 * CARD_MARGIN }
}

/// Whether the bar is on.
pub fn is_on(app: &AppHandle) -> bool {
    mode_of(app) != Mode::Off
}

/// Where the bar shows.
pub fn mode_of(app: &AppHandle) -> Mode {
    app.try_state::<Bar>().and_then(|s| s.saved.lock().ok().map(|s| s.mode())).unwrap_or(Mode::Off)
}

/// Where the bar shows, with the notch it docks into when it does. On the main thread.
pub fn mode_view(app: &AppHandle) -> ModeView {
    let mode = mode_of(app);
    ModeView { mode, notch: (mode == Mode::Notch).then(native::notch).flatten() }
}

/// Turns the bar on or off; on, it shows where it last did.
pub fn set_on(app: &AppHandle, on: bool) {
    let notch = app.try_state::<Bar>().and_then(|s| s.saved.lock().ok().map(|s| s.notch)).unwrap_or(false);
    set_mode(app, match (on, notch) {
        (false, _) => Mode::Off,
        (true, true) => Mode::Notch,
        (true, false) => Mode::Floating,
    });
}

/// Moves the bar to the notch or back to where it floated, or turns it off, and tells every
/// window, so the toggle at the top right and the Window menu stay in step with wherever it was
/// changed.
pub fn set_mode(app: &AppHandle, mode: Mode) {
    let Some(state) = app.try_state::<Bar>() else { return };
    let floating_frame = {
        let Ok(mut saved) = state.saved.lock() else { return };
        saved.off = mode == Mode::Off;
        if mode != Mode::Off {
            saved.notch = mode == Mode::Notch;
        }
        saved.save();
        saved.frame
    };
    for (item, checked) in [(&state.item, mode == Mode::Floating), (&state.notch_item, mode == Mode::Notch)] {
        if let Some(item) = item.lock().ok().and_then(|i| i.clone()) {
            let _ = item.set_checked(checked);
        }
    }
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        let Some(bar) = handle.get_webview_window(LABEL) else { return };
        // Floating, the bar takes every click; docked, the pointer watch decides.
        if let Some(state) = handle.try_state::<Bar>() {
            state.on_shape.store(true, Ordering::Relaxed);
            if let Ok(mut s) = state.shape.lock() {
                *s = None;
            }
        }
        native::set_clickable(&bar, true);
        match mode {
            Mode::Off => {
                let _ = bar.hide();
            }
            Mode::Floating => {
                native::set_level(&bar, false);
                if let Some(r) = start_frame(floating_frame, &native::screens()) {
                    native::set_frame(&bar, r);
                }
                native::show(&bar);
            }
            Mode::Notch => {
                place_for(&handle, &bar, Mode::Notch);
                native::show(&bar);
            }
        }
        let view = mode_view(&handle);
        let _ = handle.emit("bar-mode", view);
        let _ = handle.emit("floating-bar", mode != Mode::Off);
    });
}

/// Puts the bar at its level and place for how it shows: docked, centered on the notch at the
/// size it has (the page sizes it to its shape), and the notch remembered. On the main thread.
fn place_for(app: &AppHandle, bar: &tauri::WebviewWindow, mode: Mode) {
    native::set_level(bar, mode == Mode::Notch);
    if mode != Mode::Notch {
        return;
    }
    let notch = native::notch();
    if let (Some(n), Some(current)) = (notch, native::frame(bar)) {
        native::set_frame(bar, notch_frame(n, current.w, current.h));
    }
    if let Some(state) = app.try_state::<Bar>() {
        if let Ok(mut slot) = state.notch.lock() {
            *slot = notch;
        }
    }
}

/// The app an open target lands in, by bundle ID: the terminal for terminal sessions, else the
/// Claude or Codex app. `terminal` is the terminal's profile in config.toml.
pub fn target_app(target_id: &str, terminal: &str) -> Option<&'static str> {
    match target_id.rsplit('|').next()? {
        "focus" | "attach" | "resume" => match terminal {
            "ghostty" => Some("com.mitchellh.ghostty"),
            _ => None,
        },
        "claude-app" => Some("com.anthropic.claudefordesktop"),
        "codex" => Some("com.openai.codex"),
        _ => None,
    }
}

/// After the bar opened something, asks macOS to bring that app forward. A click in the bar leaves
/// Codealong inactive, and macOS may refuse an inactive app; the open itself has already happened.
pub fn bring_forward(app: &AppHandle, bundle_id: &'static str) {
    let _ = app.run_on_main_thread(move || native::activate_app(bundle_id));
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

/// The bar's panel, shared with the cards pinned to the desktop (desktop.rs), which float the same way.
#[cfg(target_os = "macos")]
pub(crate) mod native {
    use std::sync::OnceLock;

    use objc2::runtime::{AnyClass, AnyObject, Bool, ClassBuilder, Sel};
    use objc2::{sel, ClassType, MainThreadMarker};
    use objc2_app_kit::{NSApplicationActivationOptions, NSEvent, NSMainMenuWindowLevel, NSPanel, NSRunningApplication, NSScreen, NSStatusWindowLevel, NSWindow, NSWindowCollectionBehavior, NSWindowOcclusionState, NSWindowStyleMask};
    use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};
    use tauri::WebviewWindow;

    use super::{Notch, Rect, Screen};

    /// The window's NSWindow, on the main thread only.
    fn ns(w: &WebviewWindow) -> Option<&NSWindow> {
        MainThreadMarker::new()?;
        let ptr = w.ns_window().ok()? as *const NSWindow;
        // SAFETY: Tauri hands out the window's own NSWindow, which lives as long as the window,
        // and this is the main thread, where AppKit expects to be called.
        unsafe { ptr.as_ref() }
    }

    /// NSPanel that never becomes key or main, so the board keeps the keyboard and its place, but
    /// says it's key when asked. WebKit treats a page whose window isn't key as inactive: it sends
    /// such a page's mouse moves only to scrollbars, so pills never showed hover, `:hover` stayed on
    /// whatever was last clicked, and no tooltip ever appeared. Keys still go to the app's real key
    /// window, and menu commands skip the bar (`front_window` in lib.rs).
    fn panel_class() -> &'static AnyClass {
        static CLASS: OnceLock<&'static AnyClass> = OnceLock::new();
        CLASS.get_or_init(|| {
            extern "C" fn no(_: &AnyObject, _: Sel) -> Bool {
                Bool::NO
            }
            extern "C" fn yes(_: &AnyObject, _: Sel) -> Bool {
                Bool::YES
            }
            // Where it's put is where it goes: docked, the bar sits over the menu bar, which
            // AppKit would otherwise keep a window below.
            extern "C" fn as_asked(_: &AnyObject, _: Sel, frame: NSRect, _: *const AnyObject) -> NSRect {
                frame
            }
            match ClassBuilder::new(c"CodealongBarPanel", NSPanel::class()) {
                Some(mut builder) => {
                    // SAFETY: the overrides match NSWindow's `- (BOOL)canBecome…Window` and
                    // `- (BOOL)isKeyWindow`.
                    unsafe {
                        builder.add_method(sel!(canBecomeKeyWindow), no as extern "C" fn(_, _) -> _);
                        builder.add_method(sel!(canBecomeMainWindow), no as extern "C" fn(_, _) -> _);
                        builder.add_method(sel!(isKeyWindow), yes as extern "C" fn(_, _) -> _);
                        builder.add_method(sel!(constrainFrameRect:toScreen:), as_asked as extern "C" fn(_, _, _, _) -> _);
                    }
                    builder.register()
                }
                None => AnyClass::get(c"CodealongBarPanel").unwrap_or(NSPanel::class()),
            }
        })
    }

    /// The class a window had before `make_panel` swapped it.
    pub struct Original(&'static AnyClass);

    /// Turns the bar's window into a non-activating panel, the only kind macOS puts on another
    /// app's full-screen Space. Tauri makes windows, so the class is swapped after the fact. WebKit
    /// asks again whether the window is key each time it's ordered on screen, which `show` does
    /// after this, so the page counts as active from the bar's first showing.
    ///
    /// Returns the class it replaced, which a window must get back before it's destroyed
    /// (`unmake_panel`). The bar never is.
    pub fn make_panel(w: &WebviewWindow) -> Option<Original> {
        let n = ns(w)?;
        let obj: &AnyObject = n;
        let panel = panel_class();
        let original = obj.class();
        // NSPanel keeps no storage of its own beyond NSWindow's, so the swap needs no more room
        // than tao's window already has; if that ever changes, the bar stays a plain window.
        if panel.instance_size() > original.instance_size() {
            return None;
        }
        // SAFETY: the object is tao's NSWindow subclass, at least as large as the panel class,
        // which only adds methods. tao reads its own ivar only when focusability changes, which
        // the bar never does after it's made.
        unsafe { AnyObject::set_class(obj, panel) };
        n.setStyleMask(n.styleMask() | NSWindowStyleMask::NonactivatingPanel);
        // Panels hide when their app deactivates unless told not to.
        n.setHidesOnDeactivate(false);
        // The bar is up over other apps, so Codealong is nearly always in the background, and
        // macOS shows a background app's tooltips only in windows that allow them.
        n.setAllowsToolTipsWhenApplicationIsInactive(true);
        Some(Original(original))
    }

    /// Takes a panel off the screen and gives its window back the class `make_panel` replaced,
    /// so it can be destroyed. WebKit watches the window through key-value observing, which is
    /// itself a swap of the window's class (to `NSKVONotifying_TaoWindow`) made when the page went
    /// in; taking the page out of a window with the panel's class instead raises "Cannot remove
    /// an observer … because it is not registered", an exception that aborts the app.
    pub fn unmake_panel(w: &WebviewWindow, original: Original) {
        let Some(n) = ns(w) else { return };
        n.orderOut(None);
        n.setStyleMask(n.styleMask() & !NSWindowStyleMask::NonactivatingPanel);
        // SAFETY: the class the object had before, which make_panel checked is no larger than it.
        unsafe { AnyObject::set_class(n, original.0) };
    }

    /// Asks the running app with this bundle ID to come forward.
    pub fn activate_app(bundle_id: &str) {
        let apps = NSRunningApplication::runningApplicationsWithBundleIdentifier(&NSString::from_str(bundle_id));
        if apps.count() > 0 {
            #[allow(deprecated)]
            let _ = apps.objectAtIndex(0).activateWithOptions(NSApplicationActivationOptions::empty());
        }
    }

    fn rect(r: NSRect) -> Rect {
        Rect { x: r.origin.x, y: r.origin.y, w: r.size.width, h: r.size.height }
    }

    /// Above other apps' floating panels too (the status item level, still under menus), on every
    /// Space and over full-screen apps, left in place by Mission Control, kept out of ⌘` and the
    /// Window menu, and still up when Codealong is hidden (⌘H).
    pub fn prepare(w: &WebviewWindow) {
        if let Some(n) = ns(w) {
            n.setLevel(NSStatusWindowLevel);
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

    /// Docked in the notch the bar sits over the menu bar (level 24) and its status items (25), as
    /// the notch apps do; floating, at the status item level like other apps' floating panels.
    pub fn set_level(w: &WebviewWindow, docked: bool) {
        if let Some(n) = ns(w) {
            n.setLevel(if docked { NSMainMenuWindowLevel + 2 } else { NSStatusWindowLevel });
        }
    }

    /// Whether the window takes the pointer's clicks and moves or lets them through to what's under it.
    pub fn set_clickable(w: &WebviewWindow, clickable: bool) {
        if let Some(n) = ns(w) {
            n.setIgnoresMouseEvents(!clickable);
        }
    }

    /// The notch the bar docks into: the first screen's with one, else one drawn on the screen with
    /// the menu bar.
    pub fn notch() -> Option<Notch> {
        let mtm = MainThreadMarker::new()?;
        let all = NSScreen::screens(mtm);
        let screens: Vec<_> = (0..all.count()).map(|i| all.objectAtIndex(i)).collect();
        let of = |s: &NSScreen| {
            let (left, right) = (s.auxiliaryTopLeftArea().size.width, s.auxiliaryTopRightArea().size.width);
            super::notch_of(rect(s.frame()), rect(s.visibleFrame()), s.safeAreaInsets().top, Some((left, right)))
        };
        screens.iter().map(|s| of(s)).find(|n| n.real).or_else(|| screens.first().map(|s| of(s)))
    }

    /// Brings the bar up without making it key or activating Codealong, whichever app is in front.
    pub fn show(w: &WebviewWindow) {
        if let Some(n) = ns(w) {
            n.orderFrontRegardless();
        }
    }

    /// Ordered in and some part of it on screen. The bar sits above ordinary windows, so when it's
    /// ordered in but can't be seen, macOS kept it off the Space in front.
    pub fn on_screen(w: &WebviewWindow) -> bool {
        ns(w).map(|n| n.isVisible() && n.occlusionState().contains(NSWindowOcclusionState::Visible)).unwrap_or(true)
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

    /// Where the pointer is, in screen points.
    pub fn pointer() -> Option<(f64, f64)> {
        MainThreadMarker::new()?;
        let p = NSEvent::mouseLocation();
        Some((p.x, p.y))
    }
}

/// Elsewhere the bar is an ordinary always-on-top window, left where it opens.
#[cfg(not(target_os = "macos"))]
pub(crate) mod native {
    use tauri::WebviewWindow;

    use super::{Rect, Screen};

    pub struct Original;

    pub fn make_panel(_: &WebviewWindow) -> Option<Original> {
        None
    }
    pub fn set_level(_: &WebviewWindow, _: bool) {}
    pub fn set_clickable(_: &WebviewWindow, _: bool) {}
    pub fn notch() -> Option<super::Notch> {
        None
    }
    pub fn unmake_panel(_: &WebviewWindow, _: Original) {}
    pub fn activate_app(_: &str) {}
    pub fn prepare(_: &WebviewWindow) {}
    pub fn show(w: &WebviewWindow) {
        let _ = w.show();
    }
    pub fn on_screen(w: &WebviewWindow) -> bool {
        w.is_visible().unwrap_or(true)
    }

    pub fn frame(_: &WebviewWindow) -> Option<Rect> {
        None
    }
    pub fn set_frame(_: &WebviewWindow, _: Rect) {}
    pub fn screens() -> Vec<Screen> {
        Vec::new()
    }
    pub fn pointer() -> Option<(f64, f64)> {
        None
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
    fn knows_which_app_a_target_lands_in() {
        assert_eq!(target_app("s|claude:claude:1|focus", "ghostty"), Some("com.mitchellh.ghostty"));
        assert_eq!(target_app("s|claude:claude2:1|resume", "ghostty"), Some("com.mitchellh.ghostty"));
        assert_eq!(target_app("s|claude:claude:1|claude-app", "ghostty"), Some("com.anthropic.claudefordesktop"));
        assert_eq!(target_app("s|codex:codex:1|codex", "ghostty"), Some("com.openai.codex"));
        assert_eq!(target_app("s|claude:claude:1|focus", "iterm"), None);
        assert_eq!(target_app("l|editor", "ghostty"), None);
    }

    #[test]
    fn saved_state_reads_old_and_partial_files() {
        let s: Saved = serde_json::from_str("{}").unwrap();
        assert_eq!(s, Saved::default());
        assert_eq!(s.mode(), Mode::Floating);
        let s: Saved = serde_json::from_str(r#"{"off":true,"frame":{"x":1,"y":2,"w":3,"h":4}}"#).unwrap();
        assert!(s.off);
        assert_eq!(s.mode(), Mode::Off);
        assert_eq!(s.frame, Some(r(1.0, 2.0, 3.0, 4.0)));
        let s: Saved = serde_json::from_str(r#"{"notch":true}"#).unwrap();
        assert_eq!(s.mode(), Mode::Notch);
        assert_eq!(serde_json::to_value(Mode::Notch).unwrap(), "notch");
    }

    #[test]
    fn finds_the_notch_between_the_menu_bars_two_sides() {
        // A 14-inch MacBook Pro at its default size: 1512 by 982 points, 185 of them the notch's.
        let frame = r(0.0, 0.0, 1512.0, 982.0);
        let visible = r(0.0, 0.0, 1512.0, 950.0);
        let n = notch_of(frame, visible, 32.0, Some((663.5, 663.5)));
        assert_eq!(n, Notch { x: 756.0, top: 982.0, width: 185.0, height: 32.0, real: true });
        // A 16-inch one as it read on this Mac (2026-09-28): the notch sits half a point right
        // of the middle.
        let n16 = notch_of(r(0.0, 0.0, 1728.0, 1117.0), r(0.0, 0.0, 1728.0, 1084.0), 32.0, Some((771.0, 772.0)));
        assert_eq!(n16, Notch { x: 863.5, top: 1117.0, width: 185.0, height: 32.0, real: true });
        assert_eq!(notch_frame(n16, 285.0, 32.0), r(721.0, 1085.0, 285.0, 32.0));
        // An external display: none, so one is drawn as tall as its menu bar.
        let ext = r(1512.0, 0.0, 2560.0, 1440.0);
        let drawn = notch_of(ext, r(1512.0, 0.0, 2560.0, 1415.0), 0.0, Some((0.0, 0.0)));
        assert_eq!(drawn, Notch { x: 2792.0, top: 1440.0, width: DRAWN_NOTCH_WIDTH, height: 25.0, real: false });
        // Its menu bar hidden: the usual height.
        assert_eq!(notch_of(ext, ext, 0.0, None).height, DRAWN_NOTCH_HEIGHT);
    }

    #[test]
    fn the_pointer_is_on_the_shape_only_within_it() {
        // A window at the top of a 1117-point screen, the shape 285 by 32 centered in its 735.
        let window = r(496.0, 1039.0, 735.0, 78.0);
        let shape = r(225.0, 0.0, 285.0, 32.0);
        assert!(on_shape((863.0, 1110.0), window, shape));
        assert!(on_shape((721.0, 1117.0), window, shape), "its top left corner");
        assert!(!on_shape((863.0, 1080.0), window, shape), "under it, in the window");
        assert!(!on_shape((600.0, 1110.0), window, shape), "beside it, over the menu bar");
    }

    #[test]
    fn the_peek_opens_by_the_shape_not_the_window() {
        // The window keeps room for the shadow under the shape; the peek goes by the shape.
        let window = r(390.0, 1007.0, 948.0, 110.0);
        let f = shape_frame(window, r(108.0, 0.0, 731.0, 66.0));
        assert_eq!(f, r(486.0, 1039.0, 755.0, 90.0));
        // Its card's bottom, as peek.rs finds it, is the shape's.
        assert_eq!(f.y + CARD_MARGIN, 1117.0 - 66.0);
    }

    #[test]
    fn docks_centered_on_the_notch_at_the_top_of_the_screen() {
        let n = Notch { x: 756.0, top: 982.0, width: 185.0, height: 32.0, real: true };
        assert_eq!(notch_frame(n, 281.0, 32.0), r(616.0, 950.0, 281.0, 32.0));
        assert_eq!(notch_frame(n, 600.0, 120.0), r(456.0, 862.0, 600.0, 120.0));
    }
}
