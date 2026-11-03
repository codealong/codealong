//! The outline on macOS: CoreGraphics' windows and pointer, and an AppKit window drawing a box;
//! and pictures of a terminal's windows, found among them.

use std::cell::RefCell;
use std::ffi::c_void;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{msg_send, MainThreadMarker};
use objc2_app_kit::{NSBackingStoreType, NSBox, NSBoxType, NSColor, NSRunningApplication, NSTitlePosition, NSWindow, NSWindowStyleMask};
use objc2_foundation::NSString;
use tauri::AppHandle;

use crate::flash::Frame;
use crate::geometry::{PageRect, Rect};
use crate::panel::{self, Level, Spaces};

#[repr(C)]
#[derive(Clone, Copy)]
struct CGPoint {
    x: f64,
    y: f64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CGRect {
    origin: CGPoint,
    size: CGPoint,
}

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventCreate(source: *const c_void) -> *mut c_void;
    fn CGEventGetLocation(event: *mut c_void) -> CGPoint;
    fn CGGetActiveDisplayList(max: u32, displays: *mut u32, count: *mut u32) -> i32;
    fn CGDisplayBounds(display: u32) -> CGRect;
    fn CGMainDisplayID() -> u32;
    fn CGWindowListCopyWindowInfo(option: u32, relative_to: u32) -> *mut AnyObject;
    fn CGWarpMouseCursorPosition(point: CGPoint) -> i32;
    fn CGAssociateMouseAndMouseCursorPosition(connected: i32) -> i32;
    fn CGPreflightScreenCaptureAccess() -> bool;
    fn CGRequestScreenCaptureAccess() -> bool;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFRelease(cf: *const c_void);
}

const ALL_WINDOWS: u32 = 0;
const ON_SCREEN_ONLY: u32 = 1 << 0;
const EXCLUDE_DESKTOP: u32 = 1 << 4;

/// How long the outline stays, then how long it takes to fade.
const HOLD: Duration = Duration::from_millis(500);
const FADE: Duration = Duration::from_millis(450);
const FADE_STEPS: u32 = 15;
/// How far the outline sits outside the window, and its line.
const MARGIN: f64 = 4.0;
const LINE: f64 = 5.0;

/// Flashes take turns like focuses do: a newer click's flash replaces an older one.
static NEWEST: AtomicU64 = AtomicU64::new(0);

pub fn ticket() -> u64 {
    NEWEST.fetch_add(1, Ordering::SeqCst) + 1
}

pub fn is_newest(ticket: u64) -> bool {
    NEWEST.load(Ordering::SeqCst) == ticket
}

pub fn pointer() -> Option<(f64, f64)> {
    // SAFETY: a null source makes an event just to read the pointer; it's released here.
    unsafe {
        let event = CGEventCreate(std::ptr::null());
        if event.is_null() {
            return None;
        }
        let p = CGEventGetLocation(event);
        CFRelease(event);
        Some((p.x, p.y))
    }
}

pub fn screens() -> Vec<Frame> {
    let mut ids = [0u32; 32];
    let mut count = 0u32;
    // SAFETY: the list has room for `max` displays and the count says how many were written.
    if unsafe { CGGetActiveDisplayList(ids.len() as u32, ids.as_mut_ptr(), &mut count) } != 0 {
        return Vec::new();
    }
    // SAFETY: a plain call that takes an ID and returns a rectangle, with the IDs the list
    // just gave; one that names no display would get an empty rectangle.
    ids[..count as usize].iter().map(|&d| frame(unsafe { CGDisplayBounds(d) })).collect()
}

fn frame(r: CGRect) -> Frame {
    Frame { x: r.origin.x, y: r.origin.y, w: r.size.x, h: r.size.y }
}

fn get(dict: &AnyObject, key: &str) -> Option<Retained<AnyObject>> {
    let key = NSString::from_str(key);
    // SAFETY: CFDictionary is toll-free bridged to NSDictionary.
    unsafe { msg_send![dict, objectForKey: &*key] }
}

fn number(obj: &AnyObject) -> f64 {
    // SAFETY: the window list's numbers are CFNumbers, bridged to NSNumber.
    unsafe { msg_send![obj, doubleValue] }
}

/// One of an app's windows as the window server lists them.
#[derive(Clone, Debug, PartialEq)]
struct Listed {
    id: i64,
    frame: Frame,
    /// Its level: 0 for an ordinary window, above that for one that floats (Ghostty's Float on
    /// Top puts a terminal's window at AppKit's floating level).
    layer: f64,
    /// Its title, which the window server tells only an app that may record the screen.
    title: Option<String>,
}

/// The windows of the app with this bundle ID at any level, front to back, among those the
/// window server lists with `options`, slivers left out. Window bounds and owners are readable
/// without Screen Recording; only titles aren't.
fn windows_of(bundle_id: &str, options: u32) -> Vec<Listed> {
    let apps = NSRunningApplication::runningApplicationsWithBundleIdentifier(&NSString::from_str(bundle_id));
    if apps.count() == 0 {
        return Vec::new();
    }
    let running = apps.objectAtIndex(0);
    // SAFETY: `- (pid_t)processIdentifier`; the typed method needs the libc feature.
    let pid: i32 = unsafe { msg_send![&*running, processIdentifier] };
    let pid = pid as f64;
    // SAFETY: the copy is ours (+1), and a CFArray is toll-free bridged to NSArray.
    let Some(list) = (unsafe { Retained::<AnyObject>::from_raw(CGWindowListCopyWindowInfo(options, 0)) }) else { return Vec::new() };
    // SAFETY: NSArray's `- (NSUInteger)count`, asked of the array just copied.
    let count: usize = unsafe { msg_send![&*list, count] };
    let mut out = Vec::new();
    for i in 0..count {
        // SAFETY: `i` is under the count just read of an array that's this function's own
        // copy, so nothing changes it meanwhile; its members are the windows' dictionaries,
        // never nil.
        let w: Retained<AnyObject> = unsafe { msg_send![&*list, objectAtIndex: i] };
        if get(&w, "kCGWindowOwnerPID").map(|n| number(&n)) != Some(pid) {
            continue;
        }
        let Some(layer) = get(&w, "kCGWindowLayer").map(|n| number(&n)) else { continue };
        let Some(b) = get(&w, "kCGWindowBounds") else { continue };
        let side = |k: &str| get(&b, k).map(|n| number(&n)).unwrap_or(0.0);
        let frame = Frame { x: side("X"), y: side("Y"), w: side("Width"), h: side("Height") };
        // Skip slivers such as a tab bar's drag image.
        if frame.w < 100.0 || frame.h < 60.0 {
            continue;
        }
        let id = get(&w, "kCGWindowNumber").map(|n| number(&n) as i64).unwrap_or(-1);
        let title = get(&w, "kCGWindowName").and_then(|t| t.downcast_ref::<NSString>().map(|s| s.to_string()));
        out.push(Listed { id, frame, layer, title });
    }
    out
}

/// The front window among an app's ordinary ones: the first of them on screen is the one it
/// just brought up.
fn front(windows: Vec<Listed>) -> Option<Listed> {
    windows.into_iter().find(|w| w.layer == 0.0)
}

/// The front window of the app with this bundle ID among ordinary windows, its number and frame.
fn front_window(bundle_id: &str) -> Option<(i64, Frame)> {
    front(windows_of(bundle_id, ON_SCREEN_ONLY | EXCLUDE_DESKTOP)).map(|w| (w.id, w.frame))
}

/// The one window whose title `titled` takes, at whatever level: None when none does, or when
/// more than one does (one floating among them too), so there's no telling which is meant.
fn titled_one(windows: Vec<Listed>, titled: impl Fn(&str) -> bool) -> Option<Listed> {
    let mut found = windows.into_iter().filter(|w| w.title.as_deref().is_some_and(&titled));
    let one = found.next()?;
    found.next().is_none().then_some(one)
}

/// How long taking a picture of a window, or making it smaller, may take.
const PICTURE_WAIT: Duration = Duration::from_secs(5);

/// A picture of the one window of the app with this bundle ID whose title `titled` takes, on
/// screen or ordered out, as PNG at one pixel a point, as the windows' own pictures are
/// (improve.rs). None when another app's windows can't be pictured, as when Codealong may not
/// record the screen, which macOS is asked here the first time (and remembers the answer); and
/// when no window takes the title, or more than one, so it can't tell which is meant.
/// `screencapture` takes the picture as Codealong (its responsible process), and `sips` makes it
/// smaller.
pub fn window_picture(bundle_id: &str, titled: impl Fn(&str) -> bool) -> Option<Vec<u8>> {
    // SAFETY: plain calls that take nothing; asking shows macOS's own prompt, once.
    if !unsafe { CGPreflightScreenCaptureAccess() } {
        unsafe { CGRequestScreenCaptureAccess() };
        return None;
    }
    let one = titled_one(windows_of(bundle_id, ALL_WINDOWS), titled)?;
    let file = std::env::temp_dir().join(format!("codealong-window-{}-{}-{}.png", std::process::id(), one.id, crate::util::now_ms()));
    let path = file.to_string_lossy();
    let (id, width) = (one.id.to_string(), (one.frame.w.round() as i64).to_string());
    let taken = crate::procs::run("/usr/sbin/screencapture", &["-x", "-o", "-l", &id, &path], None, PICTURE_WAIT)
        .and_then(|_| crate::procs::run("/usr/bin/sips", &["--resampleWidth", &width, &path], None, PICTURE_WAIT))
        .ok()
        .and_then(|_| std::fs::read(&file).ok());
    let _ = std::fs::remove_file(&file);
    taken
}

/// The app's front window once it stops changing. Ghostty raises the window on its next turn
/// after the focus script returns, so the first look can still see the one before.
pub fn front_window_settled(bundle_id: &str) -> Option<Frame> {
    let mut last = None;
    for _ in 0..8 {
        std::thread::sleep(Duration::from_millis(120));
        let now = front_window(bundle_id);
        if now.is_some() && now == last {
            return now.map(|(_, f)| f);
        }
        last = now;
    }
    last.map(|(_, f)| f)
}

pub fn warp((x, y): (f64, f64)) {
    // SAFETY: plain calls; reassociating keeps the next mouse move from lagging behind.
    unsafe {
        CGWarpMouseCursorPosition(CGPoint { x, y });
        CGAssociateMouseAndMouseCursorPosition(1);
    }
}

thread_local! {
    /// The outline showing, on the main thread, with its ticket.
    static SHOWING: RefCell<Option<(u64, Retained<NSWindow>)>> = const { RefCell::new(None) };
}

/// Outlines `window` in the system accent color, holds, fades out, and closes.
pub fn outline(app: &AppHandle, window: Frame, ticket: u64) {
    let _ = app.run_on_main_thread(move || show(window, ticket));
    std::thread::sleep(HOLD);
    for step in 1..=FADE_STEPS {
        std::thread::sleep(FADE / FADE_STEPS);
        let alpha = 1.0 - step as f64 / FADE_STEPS as f64;
        let _ = app.run_on_main_thread(move || {
            SHOWING.with(|s| {
                let mut s = s.borrow_mut();
                if let Some((t, w)) = s.as_ref() {
                    if *t == ticket {
                        if alpha <= 0.0 {
                            w.orderOut(None);
                            w.close();
                            *s = None;
                        } else {
                            w.setAlphaValue(alpha);
                        }
                    }
                }
            });
        });
    }
}

fn show(window: Frame, ticket: u64) {
    let Some(mtm) = MainThreadMarker::new() else { return };
    // AppKit's origin is the bottom left of the main screen, y growing up, and the window
    // list's is that screen's top left, y growing down: the main screen is to a window there
    // what a window is to its page.
    // SAFETY: plain calls that take and return values.
    let main = frame(unsafe { CGDisplayBounds(CGMainDisplayID()) });
    let host = Rect { x: 0.0, y: 0.0, w: main.w, h: main.h };
    let around = PageRect { x: window.x, y: window.y, w: window.w, h: window.h }.to_screen(host).outset(MARGIN);
    let rect = panel::ns_rect(around);
    // SAFETY: a borderless window made and used on the main thread, kept alive in SHOWING.
    let win = unsafe { NSWindow::initWithContentRect_styleMask_backing_defer(mtm.alloc(), rect, NSWindowStyleMask::Borderless, NSBackingStoreType::Buffered, false) };
    // SAFETY: turns off AppKit's own release of the window as it closes, which on top of the
    // `Retained` in SHOWING letting go of it would release it twice.
    unsafe { win.setReleasedWhenClosed(false) };
    win.setOpaque(false);
    win.setBackgroundColor(Some(&NSColor::clearColor()));
    win.setHasShadow(false);
    win.setIgnoresMouseEvents(true);
    win.setLevel(Level::Outline.raw());
    // On every Space as the panels are, and as both kinds of them are for Mission Control:
    // left in place, and not shown as a window of its own.
    win.setCollectionBehavior(Spaces::Stationary.behavior() | Spaces::Transient.behavior());
    win.setExcludedFromWindowsMenu(true);
    let accent = NSColor::controlAccentColor();
    let outline = NSBox::initWithFrame(mtm.alloc(), panel::ns_rect(Rect { x: 0.0, y: 0.0, w: around.w, h: around.h }));
    outline.setBoxType(NSBoxType::Custom);
    outline.setTitlePosition(NSTitlePosition::NoTitle);
    outline.setBorderColor(&accent);
    outline.setBorderWidth(LINE);
    outline.setCornerRadius(16.0 + MARGIN);
    // A light tint over the window too: an outline alone is easy to miss on a big screen.
    outline.setFillColor(&accent.colorWithAlphaComponent(0.12));
    win.setContentView(Some(&outline));
    win.orderFrontRegardless();
    SHOWING.with(|s| {
        if let Some((_, old)) = s.borrow_mut().replace((ticket, win)) {
            old.orderOut(None);
            old.close();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(id: i64, layer: f64, title: &str) -> Listed {
        Listed { id, frame: Frame { x: 0.0, y: 0.0, w: 800.0, h: 600.0 }, layer, title: Some(title.to_string()) }
    }

    #[test]
    fn a_window_is_pictured_only_when_its_title_is_its_own() {
        let is = |want: &'static str| move |t: &str| t == want;
        // A floating one (Ghostty's Float on Top) is found as an ordinary one is.
        assert_eq!(titled_one(vec![window(1, 0.0, "a"), window(2, 3.0, "b")], is("b")).map(|w| w.id), Some(2));
        // Two of one title, a floating one among them: there's no telling which.
        assert_eq!(titled_one(vec![window(1, 0.0, "a"), window(2, 3.0, "a")], is("a")), None);
        assert_eq!(titled_one(vec![window(1, 0.0, "a")], is("b")), None);
        // One without a title (no Screen Recording) is never the one.
        let untitled = Listed { title: None, ..window(3, 0.0, "") };
        assert_eq!(titled_one(vec![untitled], |_| true), None);
    }

    #[test]
    fn the_front_window_is_an_ordinary_one() {
        assert_eq!(front(vec![window(1, 3.0, "floating"), window(2, 0.0, "a"), window(3, 0.0, "b")]).map(|w| w.id), Some(2));
        assert_eq!(front(vec![window(1, 3.0, "floating")]), None);
    }
}
