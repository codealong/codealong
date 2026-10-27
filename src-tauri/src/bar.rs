//! The floating bar: a small window, on top of every other and on every Space, that shows the
//! agents that need you and the ones running, PRs to review and todos ready to start. It's up
//! unless you turn it off, from the toggle at the top right of the board, the bar's own menu, or
//! the Window menu.
//!
//! It docks in the notch, where it starts on a Mac, or floats where you put it. Docked, it's at
//! the top of the screen, around the camera, as one black shape with the notch that shows the
//! mark and what needs you most, and opens down into the bar's pills when you point at it
//! (Bar.tsx). A screen without a notch gets
//! one drawn at the top center of its menu bar. Docked, the window sits above the menu bar and
//! stays at the size the shape opens to, so opening and closing are the page's animation alone:
//! a window resized as it opens shows its old picture for a frame at the wrong place, since
//! WebKit draws in a process of its own. A web page takes the clicks on its clear parts too, so
//! the window takes clicks only while the pointer is on the shape the page says it shows
//! (`notch_shape`); elsewhere they go through to the menu bar and the apps under it.
//!
//! It's a non-activating panel: macOS lets only those onto another app's full-screen Space. So a
//! click in the bar doesn't make Codealong the active app, and it doesn't take the keyboard,
//! with one exception: docked, a click on a session's pill shows the session under the pills, to
//! read and answer there, and for as long as that's up the bar has the keyboard (`set_keys`),
//! as the peek has under the floating bar. After it opens a session, Codealong asks macOS to
//! bring that app forward (`front::bring_forward`).
//!
//! It docks into the first screen with a notch, else one drawn on the screen with the menu bar,
//! unless you've dragged it to another: a press on the notch itself, off its pills, buttons and
//! session (Notch.tsx), slides it along the top of whichever screen the pointer is on, and let go
//! it docks there (`drag`). That screen is kept, and while it isn't connected the bar docks where
//! it would without it, going back once it is (`notch_on`).
//!
//! Closing the main window hides it and the app keeps running with the bar; the Dock icon and the
//! bar's mark bring it back. Whether the bar is on, where you left it, the screen its notch is on
//! and whether it stays open there are kept in `bar.json` next to the config.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tauri::menu::{CheckMenuItem, MenuItemKind, PredefinedMenuItem, WINDOW_SUBMENU_ID};
use tauri::{AppHandle, Emitter, Manager, Window, WindowEvent, Wry};

use crate::config::support_dir;
use crate::geometry::{clamp, place, screen_for, screen_holding, PageRect, Rect, Screen, CARD_MARGIN, TOP_GAP};
use crate::panel::{self, Level};

pub const LABEL: &str = "bar";
const MENU_ID: &str = "floating-bar";
const NOTCH_MENU_ID: &str = "notch-bar";
/// How often the pointer is looked at while the bar is docked, to take clicks on the shape only.
const POINTER_EVERY: Duration = Duration::from_millis(40);
/// The bar's size before its page has measured itself.
const START: (f64, f64) = (240.0, 48.0);
/// The width of the notch drawn on a screen without one: about the hardware's on a 14-inch
/// MacBook Pro (185 points).
const DRAWN_NOTCH_WIDTH: f64 = 180.0;
/// The drawn notch's height where the menu bar is hidden and says nothing of its own.
const DRAWN_NOTCH_HEIGHT: f64 = 24.0;
/// The bar's panel: it has the keyboard only while it's asked to (docked, with a session under
/// its pills) and says it's key the rest of the time, so its page gets hover and tooltips; it
/// stays where it's put, over the menu bar when it's docked; and Mission Control leaves it in
/// place. It starts at the floating bar's level, and `place_for` moves it when it docks.
const PANEL: panel::Config = panel::Config { keys: panel::Keys::WhenAsked, level: Level::FloatingBar, spaces: panel::Spaces::Stationary, unconstrained: true, tooltips: true, steps: panel::Steps::Two };

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
    /// Docked, it stays open.
    pub keep_open: bool,
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

fn notch_of_top(t: &panel::Top) -> Notch {
    notch_of(t.screen.frame, t.screen.visible, t.safe_top, Some(t.sides))
}

/// The screen the bar docks on by itself: the first with a notch, else the one with the menu bar.
fn automatic(tops: &[panel::Top]) -> Option<&panel::Top> {
    tops.iter().find(|t| notch_of_top(t).real).or(tops.first())
}

/// The notch the bar docks into: on the screen it was dragged to (`chosen`) while that's
/// connected, else on the one it picks by itself.
pub fn notch_on(tops: &[panel::Top], chosen: Option<&str>) -> Option<Notch> {
    chosen.and_then(|id| tops.iter().find(|t| t.id == id)).or_else(|| automatic(tops)).map(notch_of_top)
}

/// The screen kept for the notch once it's dropped on the one with `id`: none when that's where
/// the bar docks by itself, so a notch dragged back there follows it again (to the MacBook's own
/// screen when its lid opens, say).
pub fn chosen_by_drop(tops: &[panel::Top], id: &str) -> Option<String> {
    (automatic(tops).map(|t| t.id.as_str()) != Some(id)).then(|| id.to_string())
}

/// The docked bar's frame while it's dragged: its top at the top of the screen the pointer is on,
/// centered `center` across it, as near as keeps a notch `width` wide on that screen.
pub fn sliding(window: Rect, screen: Rect, center: f64, width: f64) -> Rect {
    let half = (width / 2.0).min(screen.w / 2.0);
    let x = center.clamp(screen.x + half, screen.right() - half);
    Rect { x: (x - window.w / 2.0).round(), y: screen.top() - window.h, w: window.w, h: window.h }
}

/// The notch the bar docks into now. On the main thread.
fn notch(app: &AppHandle) -> Option<Notch> {
    let chosen = app.try_state::<Bar>().and_then(|s| s.saved.lock().ok().and_then(|s| s.screen.clone()));
    notch_on(&panel::tops(), chosen.as_deref())
}

/// The docked bar's frame at a size: centered on the notch, its top at the top of the screen.
pub fn notch_frame(n: Notch, w: f64, h: f64) -> Rect {
    Rect { x: (n.x - w / 2.0).round(), y: n.top - h, w, h }
}

/// Where the bar starts: where you left it if that's still on a screen, else top center of the
/// first screen, just under the menu bar.
pub fn start_frame(saved: Option<Rect>, screens: &[Screen]) -> Option<Rect> {
    if let Some(r) = saved {
        if let Some(s) = screen_holding(r.center(), screens) {
            return Some(clamp(r, s.visible));
        }
    }
    let v = screens.first()?.visible;
    let (w, h) = saved.map(|r| (r.w, r.h)).unwrap_or(START);
    Some(clamp(Rect { x: v.x + (v.w - w) / 2.0, y: v.top() - TOP_GAP - h, w, h }, v))
}

/// What `bar.json` keeps.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Saved {
    /// Turned off from the bar's menu or the Window menu.
    #[serde(default)]
    off: bool,
    /// Docked in the notch rather than floating; kept while it's off, for when it's back on.
    #[serde(default = "docked_at_first")]
    notch: bool,
    /// Docked, it stays open rather than opening under the pointer.
    #[serde(default)]
    keep_open: bool,
    /// Where the bar was last.
    #[serde(default)]
    frame: Option<Rect>,
    /// The screen the notch was dragged to (`panel::Top`'s `id`); none where it docks by itself.
    #[serde(default)]
    screen: Option<String>,
}

impl Default for Saved {
    fn default() -> Saved {
        Saved { off: false, notch: docked_at_first(), keep_open: false, frame: None, screen: None }
    }
}

/// Where the bar shows before you've put it anywhere: in the notch on a Mac, and floating on
/// Linux, which has none.
fn docked_at_first() -> bool {
    crate::os::is_macos()
}

impl Saved {
    fn mode(&self) -> Mode {
        match (self.off, self.notch) {
            (true, _) => Mode::Off,
            (false, true) => Mode::Notch,
            (false, false) => Mode::Floating,
        }
    }

    /// Read leniently: bar.json is only where the bar was and how you left it (on or off, in the
    /// notch), which a drag or a click sets again, so one that doesn't read starts the bar afresh
    /// and is saved over at its next move, rather than leaving the bar unable to keep its place.
    fn load() -> Saved {
        crate::store::read_or_default(&support_dir().join("bar.json"))
    }

    fn save(&self) {
        let _ = crate::store::save(&support_dir().join("bar.json"), self);
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
    shape: Mutex<Option<PageRect>>,
    on_shape: AtomicBool,
    /// Docked, it has the keyboard, for the session it shows under its pills.
    keys: AtomicBool,
    /// The notch is being dragged to another screen: how far right of its middle the pointer
    /// went down, and which drag it is, so the look left from an earlier one stops (`drag`).
    moving: Mutex<Option<(f64, u64)>>,
    drags: AtomicU64,
}

impl Bar {
    /// How wide the notch it docks into is, which a drag keeps on the screen.
    fn notch_width(&self) -> f64 {
        self.notch.lock().ok().and_then(|n| *n).map_or(DRAWN_NOTCH_WIDTH, |n| n.width)
    }
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
        keys: AtomicBool::new(false),
        moving: Mutex::new(None),
        drags: AtomicU64::new(0),
    });

    let page = panel::Page { title: "Codealong bar", script: "window.__CODEALONG_BAR__ = true;", size: START, focusable: false, first_mouse: true };
    let window = panel::builder(app, LABEL, page).build()?;
    // The bar is never destroyed, so the class it had isn't kept.
    let _ = panel::make(&window, PANEL);
    if let Some(r) = start_frame(frame, &panel::screens()) {
        panel::set_frame(&window, r);
    }
    if mode != Mode::Off {
        place_for(app, &window, mode);
        panel::show(&window);
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
        std::thread::sleep(panel::CHECK_EVERY);
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
pub fn on_shape(p: (f64, f64), window: Rect, shape: PageRect) -> bool {
    shape.contains(window.page_point(p))
}

/// Docked: the window takes clicks while the pointer is on the shape and lets them through
/// elsewhere, and the page hears when the pointer comes onto the shape and leaves it, since a
/// window letting clicks through tells its page nothing. On the main thread.
fn watch_pointer(app: &AppHandle) {
    let (Some(bar), Some(state)) = (app.get_webview_window(LABEL), app.try_state::<Bar>()) else {
        return;
    };
    let (Some(frame), Some(p), Some(shape)) = (panel::frame(&bar), panel::pointer(), state.shape.lock().ok().and_then(|s| *s)) else {
        return;
    };
    let on = on_shape(p, frame, shape);
    // A press that began on the shape is the shape's until it's let go: text being selected in
    // the session it shows, a scroll bar being dragged.
    if !on && panel::button_down() && state.on_shape.load(Ordering::Relaxed) {
        return;
    }
    if state.on_shape.swap(on, Ordering::Relaxed) != on {
        panel::set_clickable(&bar, on);
        let _ = app.emit_to(LABEL, "notch-pointer", on);
    }
}

/// The shape the docked bar's page shows now, in the window's points from its top left.
pub fn set_shape(app: &AppHandle, shape: PageRect) {
    let Some(state) = app.try_state::<Bar>() else { return };
    if let Ok(mut s) = state.shape.lock() {
        *s = Some(shape);
    }
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || watch_pointer(&handle));
}

/// Docked, gives the bar the keyboard for the session it shows under its pills, or hands it back
/// to the app you're in once that's put away. On the main thread.
pub fn set_keys(app: &AppHandle, on: bool) {
    let (Some(bar), Some(state)) = (app.get_webview_window(LABEL), app.try_state::<Bar>()) else {
        return;
    };
    let on = on && mode_of(app) == Mode::Notch;
    if state.keys.swap(on, Ordering::Relaxed) == on {
        return;
    }
    if on {
        panel::take_keys(&bar);
    } else {
        panel::drop_keys(&bar);
    }
}

/// The docked bar lost the keyboard without being asked to give it up: a click somewhere else,
/// or another window or app taking it. Its page hears, and puts the session away unless the
/// pointer is still on it. On the main thread.
fn lost_keys(app: &AppHandle) {
    let (Some(bar), Some(state)) = (app.get_webview_window(LABEL), app.try_state::<Bar>()) else {
        return;
    };
    if !state.keys.swap(false, Ordering::Relaxed) {
        return;
    }
    panel::drop_keys(&bar);
    let _ = app.emit_to(LABEL, "notch-keys", false);
}

/// Puts the bar back up if it's on but not on screen, and writes down where it is after a move (a
/// drag moves it many times a second). On the main thread.
fn keep_up(app: &AppHandle) {
    let (Some(bar), Some(state)) = (app.get_webview_window(LABEL), app.try_state::<Bar>()) else {
        return;
    };
    let mode = state.saved.lock().map(|s| s.mode()).unwrap_or(Mode::Off);
    if mode != Mode::Off && !panel::on_screen(&bar) {
        panel::show(&bar);
    }
    // Docked, the bar follows the notch: a display plugged in or out, the lid closed, the menu
    // bar's height changed.
    if mode == Mode::Notch {
        let now = notch(app);
        let before = state.notch.lock().ok().and_then(|n| *n);
        if now != before {
            place_for(app, &bar, Mode::Notch);
            let _ = app.emit_to(LABEL, "bar-mode", ModeView { notch: now, ..mode_view(app) });
        }
    }
    if state.moved.swap(false, Ordering::Relaxed) {
        if let Ok(saved) = state.saved.lock() {
            saved.save();
        }
    }
}

/// Window events that concern the bar: closing the main window hides it instead, a moved bar is
/// remembered, and a docked one that loses the keyboard says so. On the main thread.
pub fn on_window_event(window: &Window, event: &WindowEvent) {
    match event {
        WindowEvent::CloseRequested { api, .. } if window.label() == "main" => {
            api.prevent_close();
            let _ = window.hide();
        }
        WindowEvent::Focused(false) if window.label() == LABEL => lost_keys(window.app_handle()),
        WindowEvent::Moved(_) if window.label() == LABEL => {
            let (Some(state), Some(bar)) = (window.try_state::<Bar>(), window.app_handle().get_webview_window(LABEL)) else {
                return;
            };
            // Where it floats is what's remembered; docked, it's where the notch is.
            if state.saved.lock().map(|s| s.mode() != Mode::Floating).unwrap_or(true) {
                return;
            }
            let Some(frame) = panel::frame(&bar) else { return };
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
        let Some(current) = panel::frame(&bar) else { return };
        let (w, h) = (width.ceil(), height.ceil());
        let next = match (mode_of(&handle), notch(&handle)) {
            // Being dragged, it stays under the pointer.
            (Mode::Notch, _) if dragging(&handle) => Rect { x: (current.center().0 - w / 2.0).round(), y: current.top() - h, w, h },
            (Mode::Notch, Some(n)) => notch_frame(n, w, h),
            _ => {
                let Some(screen) = screen_for(current, &panel::screens()) else { return };
                place(current, w, h, screen)
            }
        };
        if next != current {
            panel::set_frame(&bar, next);
        }
    });
}

/// Sends the bar its feed (`barfeed::feed`) after the engine publishes, if what it shows changed.
/// The bar doesn't listen for the board, so Tauri doesn't send it the half megabyte each publish.
/// The board's time changes with every publish and shows nothing, so it alone doesn't send a feed.
pub fn publish(app: &AppHandle, mut feed: crate::barfeed::BarFeed) {
    let Some(state) = app.try_state::<Bar>() else { return };
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
    panel::frame(&app.get_webview_window(LABEL)?)
}

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

/// Whether a point on the screen is on the bar: the floating bar's card, or the docked shape.
/// On the main thread.
pub fn covers(app: &AppHandle, p: (f64, f64)) -> bool {
    is_on(app) && peek_frame(app).is_some_and(|f| f.inset(CARD_MARGIN).contains(p))
}

/// A docked shape, given in its window's points from the top left, on the screen with the
/// floating card's margin around it.
pub fn shape_frame(window: Rect, shape: PageRect) -> Rect {
    shape.to_screen(window).outset(CARD_MARGIN)
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
    let keep_open = app.try_state::<Bar>().and_then(|s| s.saved.lock().ok().map(|s| s.keep_open)).unwrap_or(false);
    ModeView { mode, notch: (mode == Mode::Notch).then(|| notch(app)).flatten(), keep_open }
}

/// Keeps the docked bar open, or lets it close again once the pointer is off it, and tells every
/// window.
pub fn set_keep_open(app: &AppHandle, keep: bool) {
    let Some(state) = app.try_state::<Bar>() else { return };
    if let Ok(mut saved) = state.saved.lock() {
        saved.keep_open = keep;
        saved.save();
    }
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        let view = mode_view(&handle);
        let _ = handle.emit("bar-mode", view);
    });
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
        // Floating, the bar takes every click; docked, the pointer watch decides. Only a docked
        // bar ever has the keyboard, or is dragged.
        if let Some(state) = handle.try_state::<Bar>() {
            if state.keys.swap(false, Ordering::Relaxed) {
                panel::drop_keys(&bar);
            }
            if state.moving.lock().ok().and_then(|mut m| m.take()).is_some() {
                let _ = handle.emit_to(LABEL, "notch-drag-end", ());
            }
            state.on_shape.store(true, Ordering::Relaxed);
            if let Ok(mut s) = state.shape.lock() {
                *s = None;
            }
        }
        panel::set_clickable(&bar, true);
        match mode {
            Mode::Off => {
                let _ = bar.hide();
            }
            Mode::Floating => {
                panel::set_level(&bar, Level::FloatingBar);
                if let Some(r) = start_frame(floating_frame, &panel::screens()) {
                    panel::set_frame(&bar, r);
                }
                panel::show(&bar);
            }
            Mode::Notch => {
                place_for(&handle, &bar, Mode::Notch);
                panel::show(&bar);
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
    panel::set_level(bar, if mode == Mode::Notch { Level::DockedBar } else { Level::FloatingBar });
    if mode != Mode::Notch {
        return;
    }
    let notch = notch(app);
    if let (Some(n), Some(current)) = (notch, panel::frame(bar)) {
        panel::set_frame(bar, notch_frame(n, current.w, current.h));
    }
    if let Some(state) = app.try_state::<Bar>() {
        if let Ok(mut slot) = state.notch.lock() {
            *slot = notch;
        }
    }
}

/// You're dragging the docked bar by the notch itself, and the pointer has gone `dx` across since
/// the button went down: from here it slides along the top of whichever screen the pointer is
/// on, and once the button comes up it docks in that screen's notch, which is kept. The pointer
/// is read here until then, as it is for the peek's drag (peek.rs), rather than from the page's
/// pointer events, which come from a window that's moving under the pointer. Whether it started:
/// only a docked bar is dragged. On the main thread.
pub fn drag(app: &AppHandle, dx: f64) -> bool {
    let (Some(state), Some(bar)) = (app.try_state::<Bar>(), app.get_webview_window(LABEL)) else { return false };
    let (Some(p), Some(frame)) = (panel::pointer(), panel::frame(&bar)) else { return false };
    if mode_of(app) != Mode::Notch {
        return false;
    }
    let n = state.drags.fetch_add(1, Ordering::Relaxed) + 1;
    {
        let Ok(mut moving) = state.moving.lock() else { return false };
        // Where on the notch it was taken: as far from its middle as keeps the pointer on it.
        let half = state.notch_width() / 2.0;
        *moving = Some(((p.0 - dx - frame.center().0).clamp(-half, half), n));
    }
    follow_drag(app, n);
    let going = move |app: &AppHandle| app.try_state::<Bar>().is_some_and(|s| s.moving.lock().is_ok_and(|m| m.is_some_and(|(_, at)| at == n)));
    panel::poll(app, panel::DRAG_EVERY, going, move |app| follow_drag(app, n));
    true
}

/// Whether the docked bar is being dragged to another screen.
fn dragging(app: &AppHandle) -> bool {
    app.try_state::<Bar>().is_some_and(|s| s.moving.lock().is_ok_and(|m| m.is_some()))
}

/// Looks at the pointer for a drag of the notch: the bar slides along the top of the screen the
/// pointer is on, and with the button up docks on that screen and keeps it. On the main thread.
fn follow_drag(app: &AppHandle, n: u64) {
    let (Some(state), Some(bar)) = (app.try_state::<Bar>(), app.get_webview_window(LABEL)) else { return };
    let Some(grab) = state.moving.lock().ok().and_then(|m| *m).filter(|(_, at)| *at == n).map(|(grab, _)| grab) else { return };
    let (Some(p), Some(frame)) = (panel::pointer(), panel::frame(&bar)) else { return };
    let tops = panel::tops();
    let Some(top) = tops.iter().find(|t| t.screen.frame.contains(p)).or(tops.first()) else { return };
    if panel::button_down() {
        let next = sliding(frame, top.screen.frame, p.0 - grab, state.notch_width());
        if next != frame {
            panel::set_frame(&bar, next);
        }
        return;
    }
    if let Ok(mut moving) = state.moving.lock() {
        *moving = None;
    }
    if let Ok(mut saved) = state.saved.lock() {
        saved.screen = chosen_by_drop(&tops, &top.id);
        saved.save();
    }
    place_for(app, &bar, Mode::Notch);
    let _ = app.emit("bar-mode", mode_view(app));
    let _ = app.emit_to(LABEL, "notch-drag-end", ());
}

/// The app an open target lands in, by bundle ID: the terminal for terminal sessions, Chrome
/// for a session's tabs there, else the harness's own app. `terminal` is the terminal's profile
/// in config.toml, where new terminals open; `host` is the app a live session's terminal is in,
/// which is where focusing it goes; `own` is the app the session's harness says this way of
/// opening it lands in (`Adapter::app_bundle`).
pub fn target_app(target_id: &str, terminal: &str, host: Option<&'static str>, own: Option<&'static str>) -> Option<&'static str> {
    match target_id.rsplit('|').next()? {
        "focus" => host.or_else(|| crate::launch::bundle_id(terminal)),
        "attach" | "resume" | "terminal" => crate::launch::bundle_id(terminal),
        "chrome" => Some(crate::chrome::BUNDLE_ID),
        _ => own,
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
        let _mac = crate::os::AsIf::new(crate::os::Os::MacOs);
        assert_eq!(target_app("s|claude:claude:1|focus", "ghostty", None, None), Some("com.mitchellh.ghostty"));
        assert_eq!(target_app("s|claude:claude:1|focus", "ghostty", Some("com.googlecode.iterm2"), None), Some("com.googlecode.iterm2"));
        assert_eq!(target_app("s|claude:claude2:1|resume", "ghostty", Some("com.googlecode.iterm2"), None), Some("com.mitchellh.ghostty"));
        assert_eq!(target_app("s|claude:claude2:1|resume", "terminal", None, None), Some("com.apple.Terminal"));
        assert_eq!(target_app("s|claude:claude:1|focus", "kitty", None, None), None);
        assert_eq!(target_app("l|editor", "ghostty", None, None), None);
        // A new terminal in a session's or a worktree's folder opens in the terminal.
        assert_eq!(target_app("s|claude:claude:1|terminal", "rex", None, None), Some("com.superlogical.rex"));
        assert_eq!(target_app("l|terminal", "iterm2", None, None), Some("com.googlecode.iterm2"));
        // A harness's own way lands in its app, which its adapter names; a terminal's never does.
        let own = |h: crate::model::Harness, action: &str| crate::harness::adapter(h).app_bundle(action);
        use crate::model::Harness::{Antigravity, Claude, Codex, Opencode};
        assert_eq!(target_app("s|claude:claude:1|claude-app", "ghostty", None, own(Claude, "claude-app")), Some("com.anthropic.claudefordesktop"));
        assert_eq!(target_app("s|codex:codex:1|codex", "ghostty", None, own(Codex, "codex")), Some("com.openai.codex"));
        assert_eq!(target_app("s|antigravity:antigravity:1|antigravity", "ghostty", None, own(Antigravity, "antigravity")), Some("com.google.antigravity"));
        assert_eq!(target_app("s|claude:claude:1|resume", "ghostty", None, own(Claude, "resume")), Some("com.mitchellh.ghostty"));
        assert_eq!(target_app("s|claude:claude:1|hosted", "ghostty", None, own(Claude, "hosted")), None);
        assert_eq!(own(Opencode, "focus"), None);
    }

    #[test]
    fn saved_state_reads_old_and_partial_files() {
        let _mac = crate::os::AsIf::new(crate::os::Os::MacOs);
        // Nothing saved yet: in the notch.
        let s: Saved = serde_json::from_str("{}").unwrap();
        assert_eq!(s, Saved::default());
        assert_eq!(s.mode(), Mode::Notch);
        // Floating once it's been put there, as every bar.json written before says it was.
        let s: Saved = serde_json::from_str(r#"{"off":false,"notch":false,"keepOpen":false}"#).unwrap();
        assert_eq!(s.mode(), Mode::Floating);
        let s: Saved = serde_json::from_str(r#"{"off":true,"frame":{"x":1,"y":2,"w":3,"h":4}}"#).unwrap();
        assert!(s.off);
        assert_eq!(s.mode(), Mode::Off);
        assert_eq!(s.frame, Some(r(1.0, 2.0, 3.0, 4.0)));
        let s: Saved = serde_json::from_str(r#"{"notch":true}"#).unwrap();
        assert_eq!(s.mode(), Mode::Notch);
        assert!(!s.keep_open);
        let s: Saved = serde_json::from_str(r#"{"notch":true,"keepOpen":true}"#).unwrap();
        assert!(s.keep_open);
        assert_eq!(s.screen, None);
        let s: Saved = serde_json::from_str(r#"{"notch":true,"screen":"9C4B2D1E-0000-4000-8000-000000000001"}"#).unwrap();
        assert_eq!(s.screen.as_deref(), Some("9C4B2D1E-0000-4000-8000-000000000001"));
        assert_eq!(serde_json::to_value(Mode::Notch).unwrap(), "notch");
    }

    #[test]
    fn floats_at_first_on_linux_which_has_no_notch() {
        let _linux = crate::os::AsIf::new(crate::os::Os::Linux);
        assert_eq!(Saved::default().mode(), Mode::Floating);
        let s: Saved = serde_json::from_str("{}").unwrap();
        assert_eq!(s.mode(), Mode::Floating);
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
        let shape = PageRect { x: 225.0, y: 0.0, w: 285.0, h: 32.0 };
        assert!(on_shape((863.0, 1110.0), window, shape));
        assert!(on_shape((721.0, 1117.0), window, shape), "its top left corner");
        assert!(!on_shape((863.0, 1080.0), window, shape), "under it, in the window");
        assert!(!on_shape((600.0, 1110.0), window, shape), "beside it, over the menu bar");
    }

    #[test]
    fn the_peek_opens_by_the_shape_not_the_window() {
        // The window keeps room for the shadow under the shape; the peek goes by the shape.
        let window = r(390.0, 1007.0, 948.0, 110.0);
        let f = shape_frame(window, PageRect { x: 108.0, y: 0.0, w: 731.0, h: 66.0 });
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

    /// A screen's top, with a MacBook's notch or without one.
    fn top(id: &str, frame: Rect, notch: bool) -> panel::Top {
        let visible = Rect { h: frame.h - 32.0, ..frame };
        let side = if notch { (frame.w - 185.0) / 2.0 } else { 0.0 };
        panel::Top { screen: Screen { frame, visible }, safe_top: if notch { 32.0 } else { 0.0 }, sides: (side, side), id: id.to_string() }
    }

    #[test]
    fn docks_on_the_screen_it_was_dragged_to_while_that_one_is_connected() {
        // An external display with the menu bar, a MacBook to its left and another display right.
        let ext = top("ext", r(0.0, 0.0, 2560.0, 1440.0), false);
        let mac = top("mac", r(-1512.0, 0.0, 1512.0, 982.0), true);
        let side = top("side", r(2560.0, 0.0, 1920.0, 1080.0), false);
        let all = [ext.clone(), mac.clone(), side.clone()];
        // By itself, the MacBook's notch, wherever the menu bar is.
        assert_eq!(notch_on(&all, None), Some(notch_of_top(&mac)));
        // Dragged to the display on the right: drawn at the top center of it.
        let n = notch_on(&all, Some("side")).unwrap();
        assert_eq!((n.x, n.top, n.real), (3520.0, 1080.0, false));
        // That display unplugged: where it docks by itself, until it's back.
        assert_eq!(notch_on(&[ext.clone(), mac.clone()], Some("side")), Some(notch_of_top(&mac)));
        assert_eq!(notch_on(&[ext.clone()], Some("side")), Some(notch_of_top(&ext)), "the lid closed too");
        assert_eq!(notch_on(&all, Some("side")), Some(n), "plugged in again");
        assert_eq!(notch_on(&[], Some("side")), None);
    }

    #[test]
    fn a_notch_dropped_where_it_docks_by_itself_follows_that_again() {
        let ext = top("ext", r(0.0, 0.0, 2560.0, 1440.0), false);
        let mac = top("mac", r(-1512.0, 0.0, 1512.0, 982.0), true);
        let all = [ext.clone(), mac];
        assert_eq!(chosen_by_drop(&all, "ext"), Some("ext".to_string()));
        assert_eq!(chosen_by_drop(&all, "mac"), None);
        // The lid closed: the menu bar's screen is the one it docks on by itself.
        assert_eq!(chosen_by_drop(&[ext], "ext"), None);
    }

    #[test]
    fn a_dragged_notch_slides_along_the_top_of_the_pointer_s_screen() {
        let window = r(600.0, 900.0, 300.0, 82.0);
        let screen = r(-1920.0, 200.0, 1920.0, 1080.0);
        // Its top at the screen's, centered where the pointer takes it.
        assert_eq!(sliding(window, screen, -1000.0, 185.0), r(-1150.0, 1198.0, 300.0, 82.0));
        // No further than keeps the notch on that screen.
        assert_eq!(sliding(window, screen, -1910.0, 185.0), r(-1978.0, 1198.0, 300.0, 82.0));
        assert_eq!(sliding(window, screen, 10.0, 185.0), r(-243.0, 1198.0, 300.0, 82.0));
    }
}
