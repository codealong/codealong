//! The windows on Linux. The panels (panel.rs): the borderless windows `builder` makes, kept over
//! other apps' windows and on every workspace, through Tauri and GTK. An X11 window manager keeps one stack for every
//! window that's on top and gives the keyboard to whichever window asks, so here there are no
//! levels, no panel that takes the keyboard while another app stays active, and no notch.
//! Rectangles are turned into AppKit's way of counting, from the bottom left of the primary
//! screen with y growing up (geometry.rs `Rect`), so the code that places the windows is the
//! same on every platform. GTK's coordinates, the pointer's among them, are the whole desktop's
//! on X11 only: under Wayland a window can't learn where it or the pointer is.

use ::gtk::gdk;
use ::gtk::gdk::prelude::*;
use ::gtk::glib::translate::{FromGlibPtrFull, ToGlibPtr};
use ::gtk::glib::GString;
use ::gtk::prelude::*;
use tauri::{LogicalPosition, LogicalSize, Manager, WebviewWindow};

use super::PageView;
use crate::panel::{Config, Level, Top};
use crate::geometry::{Rect, Screen};

/// GTK is the main thread's. Off it the caller does nothing, as on macOS; a debug build stops
/// instead, to say who called.
fn main_thread() -> bool {
    let on = ::gtk::is_initialized_main_thread();
    debug_assert!(on, "panel.rs was called off the main thread, where GTK isn't to be called");
    on
}

/// Whether this is the main thread, for a caller that may be on either.
pub fn on_main_thread() -> bool {
    ::gtk::is_initialized_main_thread()
}

/// What `make` hands back for `unmake`: nothing is swapped here, so nothing is given back.
pub struct Original;

/// Keeps the window over other apps' windows, on every workspace and off the taskbar, which
/// `builder` asked for already; a window manager may drop them while a window is hidden.
pub fn make(w: &WebviewWindow, _config: Config) -> Option<Original> {
    let _ = w.set_always_on_top(true);
    let _ = w.set_visible_on_all_workspaces(true);
    let _ = w.set_skip_taskbar(true);
    Some(Original)
}

pub fn unmake(w: &WebviewWindow, _original: Original) {
    let _ = w.hide();
}

pub fn show(w: &WebviewWindow) {
    let _ = w.show();
}

/// Shows the window, with the keyboard when `keys`. Without, the window manager decides whether
/// it takes it; the windows that never should are made unfocusable (`Page::focusable`).
pub fn present(w: &WebviewWindow, keys: bool) {
    let _ = w.show();
    if keys {
        take_keys(w);
    }
}

pub fn hide(w: &WebviewWindow) {
    let _ = w.hide();
}

pub fn take_keys(w: &WebviewWindow) {
    let _ = w.set_focus();
}

/// Nothing to do: there's no way to hand the keyboard back to the app that had it. It goes where
/// you click next.
pub fn drop_keys(_w: &WebviewWindow) {}

/// Another of Codealong's windows has the keyboard.
pub fn other_is_key(w: &WebviewWindow) -> bool {
    w.app_handle().webview_windows().values().any(|o| o.label() != w.label() && o.is_focused().unwrap_or(false))
}

/// Every panel is on top, in the one stack the window manager keeps.
pub fn set_level(_w: &WebviewWindow, _level: Level) {}

pub fn set_clickable(w: &WebviewWindow, clickable: bool) {
    let _ = w.set_ignore_cursor_events(!clickable);
}

/// GTK doesn't animate a window in.
pub fn no_animation(_w: &WebviewWindow) {}

/// How much of the window shows through, where a compositor runs; without one it's opaque.
pub fn set_alpha(w: &WebviewWindow, alpha: f64) {
    if let Ok(g) = w.gtk_window() {
        if g.opacity() != alpha {
            g.set_opacity(alpha);
        }
    }
}

pub fn is_visible(w: &WebviewWindow) -> bool {
    w.is_visible().unwrap_or(false)
}

/// Shown and not minimized. A window manager doesn't say whether one is covered.
pub fn on_screen(w: &WebviewWindow) -> bool {
    w.is_visible().unwrap_or(true) && !w.is_minimized().unwrap_or(false)
}

/// A window's frame, in screen points: any window's, not only a panel's. Its outside, with what
/// the window manager draws round it (a board window's title bar), as AppKit's frame is.
pub fn frame(w: &WebviewWindow) -> Option<Rect> {
    if !main_thread() {
        return None;
    }
    let scale = w.scale_factor().ok()?;
    let at = w.outer_position().ok()?.to_logical::<f64>(scale);
    let size = w.outer_size().ok()?.to_logical::<f64>(scale);
    Some(flipped((at.x, at.y, size.width, size.height), primary()?))
}

/// Puts the window's outside where `frame` reads it. Tauri places a window by its outside and
/// sizes its inside, so the border comes off the size (`border`).
pub fn set_frame(w: &WebviewWindow, r: Rect) {
    if !main_thread() {
        return;
    }
    let Some(main) = primary() else { return };
    let (x, y) = unflipped(r, main);
    let (bw, bh) = border(w);
    let _ = w.set_position(LogicalPosition::new(x, y));
    let _ = w.set_size(LogicalSize::new((r.w - bw).max(1.0), (r.h - bh).max(1.0)));
}

/// How much wider and taller the window manager makes a window than its inside: none for a panel,
/// which has no decorations. A window's own is known only once it's up, which the waiting window
/// isn't yet as its frame is put back, so till then it's another of Codealong's that's up: a
/// window manager borders its windows alike. With none up, nothing comes off, and a restored
/// window is that much bigger.
fn border(w: &WebviewWindow) -> (f64, f64) {
    if !w.is_decorated().unwrap_or(false) {
        return (0.0, 0.0);
    }
    let of = |w: &WebviewWindow| {
        if !w.is_visible().unwrap_or(false) || !w.is_decorated().unwrap_or(false) {
            return None;
        }
        let scale = w.scale_factor().ok()?;
        let (outer, inner) = (w.outer_size().ok()?.to_logical::<f64>(scale), w.inner_size().ok()?.to_logical::<f64>(scale));
        Some((outer.width - inner.width, outer.height - inner.height)).filter(|(bw, bh)| *bw >= 0.0 && *bh > 0.0)
    };
    of(w).or_else(|| w.app_handle().webview_windows().values().find_map(of)).unwrap_or((0.0, 0.0))
}

/// Every screen, the primary one first.
pub fn screens() -> Vec<Screen> {
    tops().into_iter().map(|t| t.screen).collect()
}

/// The top of every screen, the primary one first. None has a notch.
pub fn tops() -> Vec<Top> {
    if !main_thread() {
        return Vec::new();
    }
    let (Some(display), Some(main)) = (gdk::Display::default(), primary()) else { return Vec::new() };
    let mut all: Vec<(i32, gdk::Monitor)> = (0..display.n_monitors()).filter_map(|i| Some((i, display.monitor(i)?))).collect();
    all.sort_by_key(|(_, m)| !m.is_primary());
    all.iter()
        .map(|(i, m)| Top { screen: Screen { frame: flipped(of(m.geometry()), main), visible: flipped(of(m.workarea()), main) }, safe_top: 0.0, sides: (0.0, 0.0), id: monitor_id(&display, *i) })
        .collect()
}

/// The connector a monitor is plugged into (`DP-1`), which X and Wayland name it by: no two share
/// one, and it stays while the cable stays in that port, so two monitors of one model are told
/// apart however the others come and go. GTK 3 gives it only through the deprecated
/// `gdk_screen_get_monitor_plug_name` (GTK 4 has `gdk_monitor_get_connector`). Without one, where
/// the monitor is in GDK's list.
fn monitor_id(display: &gdk::Display, i: i32) -> String {
    let screen = display.default_screen();
    // SAFETY: GDK's own screen and a monitor number it listed.
    let name = unsafe { gdk::ffi::gdk_screen_get_monitor_plug_name(screen.to_glib_none().0, i) };
    if name.is_null() {
        return format!("monitor {i}");
    }
    // SAFETY: a new string GDK hands over, which the GString frees.
    unsafe { GString::from_glib_full(name) }.to_string()
}

/// Where the pointer is, in screen points.
pub fn pointer() -> Option<(f64, f64)> {
    let (root, device) = root_and_pointer()?;
    let (_, x, y, _) = root.device_position_double(&device);
    let r = flipped((x, y, 0.0, 0.0), primary()?);
    Some((r.x, r.y))
}

/// The mouse's main button is down, whichever app has the click. On the main thread.
pub fn button_down() -> bool {
    root_and_pointer().is_some_and(|(root, device)| root.device_position(&device).3.contains(gdk::ModifierType::BUTTON1_MASK))
}

fn root_and_pointer() -> Option<(gdk::Window, gdk::Device)> {
    if !main_thread() {
        return None;
    }
    let display = gdk::Display::default()?;
    let device = display.default_seat()?.pointer()?;
    Some((display.default_screen().root_window()?, device))
}

/// The primary screen as GDK places it: x right and y down from the top left of the desktop.
/// An X server that names no primary screen has its first.
fn primary() -> Option<(f64, f64, f64, f64)> {
    let display = gdk::Display::default()?;
    let m = display.primary_monitor().or_else(|| display.monitor(0))?;
    Some(of(m.geometry()))
}

fn of(r: gdk::Rectangle) -> (f64, f64, f64, f64) {
    (r.x() as f64, r.y() as f64, r.width() as f64, r.height() as f64)
}

/// A rectangle as GDK places it, as AppKit would: from the bottom left of the primary screen,
/// `main`, with y growing up.
fn flipped((x, y, w, h): (f64, f64, f64, f64), main: (f64, f64, f64, f64)) -> Rect {
    Rect { x: x - main.0, y: main.1 + main.3 - y - h, w, h }
}

/// Where GDK puts the top left corner of a rectangle counted as AppKit does.
fn unflipped(r: Rect, main: (f64, f64, f64, f64)) -> (f64, f64) {
    (r.x + main.0, main.1 + main.3 - r.y - r.h)
}

// ---- What macOS's AppKit and WebKit are asked of any window, which GTK's aren't ----

/// GTK has no first responder to hand back: the page keeps what focus it has.
pub fn refocus_page(_w: &WebviewWindow, report: impl FnOnce(Option<String>) + Send + 'static) {
    report(None);
}

/// WebKitGTK has no drawing side's scrolling tree to read, so the probe has nothing to compare.
pub fn read_page(_w: &WebviewWindow, _read: impl FnOnce(PageView) + Send + 'static) {}

/// WebKitGTK's pages aren't asked for pictures: Improve Codealong goes without them.
pub fn picture(_window: &WebviewWindow, _done: impl FnOnce(Option<Vec<u8>>) + Send + 'static) -> bool {
    false
}

/// The window manager draws a board window's title bar.
pub fn overlay_title_bar<'a, R: tauri::Runtime, M: tauri::Manager<R>>(b: tauri::WebviewWindowBuilder<'a, R, M>, _buttons: Option<(f64, f64)>) -> tauri::WebviewWindowBuilder<'a, R, M> {
    b
}

/// Linux has no Dock to reopen the app from.
pub fn reopened(_event: &tauri::RunEvent) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rectangles_are_counted_from_the_bottom_left_of_the_primary_screen() {
        let main = (0.0, 0.0, 1600.0, 1000.0);
        // A window 300 by 100 at GDK's top left is at the primary screen's top.
        assert_eq!(flipped((0.0, 0.0, 300.0, 100.0), main), Rect { x: 0.0, y: 900.0, w: 300.0, h: 100.0 });
        assert_eq!(unflipped(Rect { x: 0.0, y: 900.0, w: 300.0, h: 100.0 }, main), (0.0, 0.0));
        // A screen to the right of the primary one and lower: its bottom is below y 0.
        let right = flipped((1600.0, 200.0, 1280.0, 1024.0), main);
        assert_eq!(right, Rect { x: 1600.0, y: -224.0, w: 1280.0, h: 1024.0 });
        assert_eq!(unflipped(right, main), (1600.0, 200.0));
        // A primary screen that isn't at GDK's origin.
        let off = (1280.0, 0.0, 1600.0, 1000.0);
        assert_eq!(flipped((1280.0, 0.0, 10.0, 10.0), off), Rect { x: 0.0, y: 990.0, w: 10.0, h: 10.0 });
    }
}
