//! The windows on Windows. The panels (panel.rs): the borderless windows `builder` makes, kept over
//! other apps' windows and off the taskbar, through Tauri, with Win32 for what Tauri doesn't say
//! (the pointer, the mouse's buttons, Shift, the monitors, a window's opacity). Windows keeps one
//! stack of topmost windows, so as on Linux there are no levels and no notch; a window made
//! unfocusable (`Page::focusable`) is one a click doesn't activate.
//!
//! Windows counts in physical pixels from the top left of the primary monitor, y growing down,
//! each monitor with a scale of its own. Rectangles are turned into AppKit's way of counting,
//! from the bottom left of the primary screen with y growing up, in points: physical pixels over
//! the primary monitor's scale, one scale for the whole desktop so that a frame read on one
//! monitor goes back where it was on another (geometry.rs `Rect`).

use std::sync::OnceLock;

use tauri::{Manager, PhysicalPosition, PhysicalSize, WebviewWindow};
use windows_sys::Win32::Foundation::{CloseHandle, FILETIME, HWND, LPARAM, POINT, RECT};
use windows_sys::Win32::Graphics::Gdi::{EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFOEXW};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, Thread32First, Thread32Next, TH32CS_SNAPTHREAD, THREADENTRY32};
use windows_sys::Win32::System::Threading::{GetCurrentProcessId, GetCurrentThreadId, GetThreadTimes, OpenThread, THREAD_QUERY_LIMITED_INFORMATION};
use windows_sys::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON, VK_SHIFT};
use windows_sys::Win32::UI::WindowsAndMessaging::{GetCursorPos, GetWindowLongPtrW, SetLayeredWindowAttributes, SetWindowLongPtrW, GWL_EXSTYLE, LWA_ALPHA, MONITORINFOF_PRIMARY, WS_EX_LAYERED};

use super::PageView;
use crate::geometry::{Rect, Screen};
use crate::panel::{Config, Level, Top};

/// The process's first thread, which runs `main` and Tauri's event loop: the one whose creation
/// time is earliest among this process's threads, found once.
fn main_thread_id() -> Option<u32> {
    static MAIN: OnceLock<Option<u32>> = OnceLock::new();
    *MAIN.get_or_init(|| {
        let mut first: Option<(u64, u32)> = None;
        // SAFETY: the snapshot and each thread's handle are closed before returning; the entry
        // is a struct with its size set, which the calls fill.
        unsafe {
            let snap = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
            if snap.is_null() || snap == windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE {
                return None;
            }
            let me = GetCurrentProcessId();
            let mut e: THREADENTRY32 = std::mem::zeroed();
            e.dwSize = std::mem::size_of::<THREADENTRY32>() as u32;
            let mut more = Thread32First(snap, &mut e) != 0;
            while more {
                if e.th32OwnerProcessID == me {
                    let h = OpenThread(THREAD_QUERY_LIMITED_INFORMATION, 0, e.th32ThreadID);
                    if !h.is_null() {
                        let zero = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
                        let (mut created, mut a, mut b, mut c) = (zero, zero, zero, zero);
                        if GetThreadTimes(h, &mut created, &mut a, &mut b, &mut c) != 0 {
                            let at = ((created.dwHighDateTime as u64) << 32) | created.dwLowDateTime as u64;
                            if first.is_none_or(|(t, _)| at < t) {
                                first = Some((at, e.th32ThreadID));
                            }
                        }
                        CloseHandle(h);
                    }
                }
                more = Thread32Next(snap, &mut e) != 0;
            }
            CloseHandle(snap);
        }
        first.map(|(_, id)| id)
    })
}

/// Whether this is the main thread, for a caller that may be on either.
pub fn on_main_thread() -> bool {
    // SAFETY: GetCurrentThreadId can't fail.
    main_thread_id() == Some(unsafe { GetCurrentThreadId() })
}

/// What `make` hands back for `unmake`: nothing is swapped here, so nothing is given back.
pub struct Original;

/// Keeps the window over other apps' windows and off the taskbar, which `builder` asked for
/// already. Windows has one desktop's windows on every virtual desktop only for windows it
/// pins itself, so a panel stays on the desktop it was made on.
pub fn make(w: &WebviewWindow, _config: Config) -> Option<Original> {
    let _ = w.set_always_on_top(true);
    let _ = w.set_skip_taskbar(true);
    Some(Original)
}

pub fn unmake(w: &WebviewWindow, _original: Original) {
    let _ = w.hide();
}

pub fn show(w: &WebviewWindow) {
    let _ = w.show();
}

/// Shows the window, with the keyboard when `keys`. Windows lets a background app take the
/// keyboard only in a moment it allows; past that the taskbar flashes the window instead.
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

/// Nothing to do: Windows gives the keyboard back to the window that had it as this one hides,
/// or it goes where you click next.
pub fn drop_keys(_w: &WebviewWindow) {}

/// Another of Codealong's windows has the keyboard.
pub fn other_is_key(w: &WebviewWindow) -> bool {
    w.app_handle().webview_windows().values().any(|o| o.label() != w.label() && o.is_focused().unwrap_or(false))
}

/// Whether the window has the keyboard, as Windows says.
pub fn has_keys(w: &WebviewWindow) -> bool {
    w.is_focused().unwrap_or(false)
}

/// Every panel is topmost, in the one stack Windows keeps.
pub fn set_level(_w: &WebviewWindow, _level: Level) {}

pub fn set_clickable(w: &WebviewWindow, clickable: bool) {
    let _ = w.set_ignore_cursor_events(!clickable);
}

/// Windows animates a window in only where its own settings say, for every app alike.
pub fn no_animation(_w: &WebviewWindow) {}

/// How much of the window shows through: a layered window's alpha, set once it's layered.
pub fn set_alpha(w: &WebviewWindow, alpha: f64) {
    let Ok(hwnd) = w.hwnd() else { return };
    let hwnd = hwnd.0 as HWND;
    // SAFETY: the window is Codealong's own and open; the style and alpha are plain values.
    unsafe {
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        if ex & WS_EX_LAYERED as isize == 0 {
            if alpha >= 1.0 {
                return;
            }
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex | WS_EX_LAYERED as isize);
        }
        SetLayeredWindowAttributes(hwnd, 0, (alpha.clamp(0.0, 1.0) * 255.0).round() as u8, LWA_ALPHA);
    }
}

pub fn is_visible(w: &WebviewWindow) -> bool {
    w.is_visible().unwrap_or(false)
}

/// Shown and not minimized. Windows doesn't say whether one is covered.
pub fn on_screen(w: &WebviewWindow) -> bool {
    w.is_visible().unwrap_or(true) && !w.is_minimized().unwrap_or(false)
}

/// A window's frame, in screen points: any window's, not only a panel's. Its outside, with the
/// title bar and border Windows draws round a board window, as AppKit's frame is.
pub fn frame(w: &WebviewWindow) -> Option<Rect> {
    let (main, scale) = primary()?;
    let at = w.outer_position().ok()?;
    let size = w.outer_size().ok()?;
    Some(flipped((at.x as f64, at.y as f64, size.width as f64, size.height as f64), main, scale))
}

/// Puts the window's outside where `frame` reads it. Tauri places a window by its outside and
/// sizes its inside, so the border comes off the size.
pub fn set_frame(w: &WebviewWindow, r: Rect) {
    let Some((main, scale)) = primary() else { return };
    let (x, y) = unflipped(r, main, scale);
    let (bw, bh) = match (w.outer_size(), w.inner_size()) {
        (Ok(o), Ok(i)) => (o.width.saturating_sub(i.width) as f64, o.height.saturating_sub(i.height) as f64),
        _ => (0.0, 0.0),
    };
    let _ = w.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32));
    let _ = w.set_size(PhysicalSize::new((r.w * scale - bw).max(1.0).round() as u32, (r.h * scale - bh).max(1.0).round() as u32));
}

/// A monitor as Windows lists it: its rectangle and work area in physical pixels, whether it's
/// the primary one, its device's name (`\\.\DISPLAY1`) and its scale.
struct Monitor {
    frame: (f64, f64, f64, f64),
    work: (f64, f64, f64, f64),
    primary: bool,
    name: String,
    scale: f64,
}

fn monitors() -> Vec<Monitor> {
    unsafe extern "system" fn each(m: HMONITOR, _dc: HDC, _r: *mut RECT, out: LPARAM) -> i32 {
        // SAFETY: `out` is the Vec `monitors` passed, alive through the enumeration; the info is
        // a struct with its size set, which the call fills.
        let out = unsafe { &mut *(out as *mut Vec<Monitor>) };
        let mut info: MONITORINFOEXW = unsafe { std::mem::zeroed() };
        info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
        if unsafe { GetMonitorInfoW(m, (&mut info as *mut MONITORINFOEXW).cast()) } == 0 {
            return 1;
        }
        let (mut dx, mut dy) = (96u32, 96u32);
        unsafe { GetDpiForMonitor(m, MDT_EFFECTIVE_DPI, &mut dx, &mut dy) };
        let rect = |r: RECT| (r.left as f64, r.top as f64, (r.right - r.left) as f64, (r.bottom - r.top) as f64);
        let len = info.szDevice.iter().position(|&c| c == 0).unwrap_or(info.szDevice.len());
        out.push(Monitor {
            frame: rect(info.monitorInfo.rcMonitor),
            work: rect(info.monitorInfo.rcWork),
            primary: info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY != 0,
            name: String::from_utf16_lossy(&info.szDevice[..len]),
            scale: dx.max(1) as f64 / 96.0,
        });
        1
    }
    let mut out: Vec<Monitor> = Vec::new();
    // SAFETY: every monitor of the desktop, each handed to `each` with the Vec it fills.
    unsafe { EnumDisplayMonitors(std::ptr::null_mut(), std::ptr::null(), Some(each), &mut out as *mut Vec<Monitor> as LPARAM) };
    out.sort_by_key(|m| !m.primary);
    out
}

/// The primary monitor's rectangle in physical pixels, and its scale, which points are counted
/// in everywhere.
fn primary() -> Option<((f64, f64, f64, f64), f64)> {
    monitors().into_iter().next().map(|m| (m.frame, m.scale))
}

/// Every screen, the primary one first.
pub fn screens() -> Vec<Screen> {
    tops().into_iter().map(|t| t.screen).collect()
}

/// The top of every screen, the primary one first, each known by its device's name. None has a
/// notch.
pub fn tops() -> Vec<Top> {
    let all = monitors();
    let Some((main, scale)) = all.first().map(|m| (m.frame, m.scale)) else { return Vec::new() };
    all.iter().map(|m| Top { screen: Screen { frame: flipped(m.frame, main, scale), visible: flipped(m.work, main, scale) }, safe_top: 0.0, sides: (0.0, 0.0), id: m.name.clone() }).collect()
}

/// Whether Shift is down, whichever app has the keyboard.
pub fn shift_down() -> bool {
    // SAFETY: GetAsyncKeyState takes a key code; its high bit says the key is down now.
    unsafe { GetAsyncKeyState(VK_SHIFT as i32) as u16 & 0x8000 != 0 }
}

/// Where the pointer is, in screen points.
pub fn pointer() -> Option<(f64, f64)> {
    let mut p = POINT { x: 0, y: 0 };
    // SAFETY: it writes the one POINT it's given.
    if unsafe { GetCursorPos(&mut p) } == 0 {
        return None;
    }
    let (main, scale) = primary()?;
    let r = flipped((p.x as f64, p.y as f64, 0.0, 0.0), main, scale);
    Some((r.x, r.y))
}

/// The mouse's main button is down, whichever app has the click.
pub fn button_down() -> bool {
    // SAFETY: as in `shift_down`.
    unsafe { GetAsyncKeyState(VK_LBUTTON as i32) as u16 & 0x8000 != 0 }
}

/// A rectangle in Windows' physical pixels, as AppKit would count it: in points of the primary
/// monitor's `scale`, from the bottom left of the primary monitor, `main`, with y growing up.
fn flipped((x, y, w, h): (f64, f64, f64, f64), main: (f64, f64, f64, f64), scale: f64) -> Rect {
    Rect { x: (x - main.0) / scale, y: (main.1 + main.3 - y - h) / scale, w: w / scale, h: h / scale }
}

/// Where Windows puts the top left corner of a rectangle counted as AppKit does, in physical
/// pixels.
fn unflipped(r: Rect, main: (f64, f64, f64, f64), scale: f64) -> (f64, f64) {
    (r.x * scale + main.0, main.1 + main.3 - (r.y + r.h) * scale)
}

// ---- What macOS's AppKit and WebKit are asked of any window, which WebView2 isn't ----

/// WebView2 has no first responder to hand back: the page keeps what focus it has.
pub fn refocus_page(_w: &WebviewWindow, report: impl FnOnce(Option<String>) + Send + 'static) {
    report(None);
}

/// WebView2 has no drawing side's scrolling tree to read, so the probe has nothing to compare.
pub fn read_page(_w: &WebviewWindow, _read: impl FnOnce(PageView) + Send + 'static) {}

/// WebView2's pages aren't asked for pictures yet: Improve Codealong goes without them.
pub fn picture(_window: &WebviewWindow, _done: impl FnOnce(Option<Vec<u8>>) + Send + 'static) -> bool {
    false
}

/// Nor asked what they show: it's asked only to picture the terminals of the sessions they
/// show, which Windows has no way to find yet (os/outline/windows.rs).
pub fn ask_page(_window: &WebviewWindow, _script: &str, _done: impl FnOnce(Option<String>) + Send + 'static) -> bool {
    false
}

/// Windows draws a board window's title bar.
pub fn overlay_title_bar<'a, R: tauri::Runtime, M: tauri::Manager<R>>(b: tauri::WebviewWindowBuilder<'a, R, M>, _buttons: Option<(f64, f64)>) -> tauri::WebviewWindowBuilder<'a, R, M> {
    b
}

/// Windows has no Dock to reopen the app from.
pub fn reopened(_event: &tauri::RunEvent) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rectangles_are_counted_in_points_from_the_bottom_left_of_the_primary_screen() {
        // A primary monitor of 2880 by 1800 pixels at 200%.
        let main = (0.0, 0.0, 2880.0, 1800.0);
        // A window 600 by 200 pixels at its top left is 300 by 100 points at its top.
        assert_eq!(flipped((0.0, 0.0, 600.0, 200.0), main, 2.0), Rect { x: 0.0, y: 800.0, w: 300.0, h: 100.0 });
        assert_eq!(unflipped(Rect { x: 0.0, y: 800.0, w: 300.0, h: 100.0 }, main, 2.0), (0.0, 0.0));
        // A monitor left of the primary one starts at negative x.
        let left = flipped((-1920.0, 0.0, 1920.0, 1080.0), main, 2.0);
        assert_eq!(left, Rect { x: -960.0, y: 360.0, w: 960.0, h: 540.0 });
        assert_eq!(unflipped(left, main, 2.0), (-1920.0, 0.0));
    }
}
