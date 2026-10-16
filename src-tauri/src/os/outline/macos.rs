//! The outline on macOS: CoreGraphics' windows and pointer, and an AppKit window drawing a box.

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
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFRelease(cf: *const c_void);
}

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

/// The front window of the app with this bundle ID among ordinary windows, its number and
/// frame. Window bounds and owners are readable without Screen Recording; only titles aren't.
fn front_window(bundle_id: &str) -> Option<(i64, Frame)> {
    let apps = NSRunningApplication::runningApplicationsWithBundleIdentifier(&NSString::from_str(bundle_id));
    if apps.count() == 0 {
        return None;
    }
    let running = apps.objectAtIndex(0);
    // SAFETY: `- (pid_t)processIdentifier`; the typed method needs the libc feature.
    let pid: i32 = unsafe { msg_send![&*running, processIdentifier] };
    let pid = pid as f64;
    // SAFETY: the copy is ours (+1), and a CFArray is toll-free bridged to NSArray.
    let list: Retained<AnyObject> = unsafe { Retained::from_raw(CGWindowListCopyWindowInfo(ON_SCREEN_ONLY | EXCLUDE_DESKTOP, 0))? };
    // SAFETY: NSArray's `- (NSUInteger)count`, asked of the array just copied.
    let count: usize = unsafe { msg_send![&*list, count] };
    // Front to back: the first of the app's normal windows is the one it just brought up.
    for i in 0..count {
        // SAFETY: `i` is under the count just read of an array that's this function's own
        // copy, so nothing changes it meanwhile; its members are the windows' dictionaries,
        // never nil.
        let w: Retained<AnyObject> = unsafe { msg_send![&*list, objectAtIndex: i] };
        if get(&w, "kCGWindowOwnerPID").map(|n| number(&n)) != Some(pid) {
            continue;
        }
        if get(&w, "kCGWindowLayer").map(|n| number(&n)) != Some(0.0) {
            continue;
        }
        let Some(b) = get(&w, "kCGWindowBounds") else { continue };
        let side = |k: &str| get(&b, k).map(|n| number(&n)).unwrap_or(0.0);
        let f = Frame { x: side("X"), y: side("Y"), w: side("Width"), h: side("Height") };
        // Skip slivers such as a tab bar's drag image.
        if f.w < 100.0 || f.h < 60.0 {
            continue;
        }
        let id = get(&w, "kCGWindowNumber").map(|n| number(&n) as i64).unwrap_or(-1);
        return Some((id, f));
    }
    None
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
