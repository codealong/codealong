//! The panels: the one place Codealong talks to AppKit about the windows that float over other
//! apps. The bar, its peek, the quick panel, the sessions kept on the desktop, the cards pinned
//! there and the lines between them are each a Tauri window turned into a non-activating NSPanel, the only kind macOS puts on another app's
//! full-screen Space, and here is how one is made, shown, given the keyboard and has it taken,
//! hidden, and given back its class before it's destroyed, with the level each floats at
//! (`Level`). A window's own module says what kind of panel it is (`Config`) and when each of
//! these happens; it never reaches AppKit itself.
//!
//! Nearly everything here is odd for a reason, found by the plain way failing, and much of it
//! could only be checked by hand in the running app: the comments say which, and when.
//!
//! All of it is the main thread's. Off it these functions do nothing and answer nothing, and a
//! debug build stops to say so (`main_thread`).

use std::ffi::CString;
use std::sync::Mutex;
use std::time::Duration;

use objc2::runtime::{AnyClass, AnyObject, Bool, ClassBuilder, Sel};
use objc2::{msg_send, sel, ClassType, MainThreadMarker};
use objc2_app_kit::{NSApplication, NSEvent, NSMainMenuWindowLevel, NSPanel, NSResponder, NSScreen, NSStatusWindowLevel, NSWindow, NSWindowAnimationBehavior, NSWindowCollectionBehavior, NSWindowLevel, NSWindowOcclusionState, NSWindowStyleMask};
use objc2_foundation::{NSPoint, NSRect, NSSize};
use tauri::{AppHandle, WebviewUrl, WebviewWindow, WebviewWindowBuilder, Wry};

use crate::geometry::{Rect, Screen};

/// How often a window that stays up checks that it is (macOS can leave one off the Space in
/// front) and writes down where it was moved to: the bar, the cards and the sessions kept on the
/// desktop.
pub const CHECK_EVERY: Duration = Duration::from_secs(1);

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
    /// The floating bar: above other apps' floating panels too (the status item level, still
    /// under menus), like other apps' floating panels.
    FloatingBar,
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

impl Level {
    /// The level as AppKit counts it.
    pub fn raw(self) -> NSWindowLevel {
        match self {
            Level::Lines => NSStatusWindowLevel - 1,
            Level::Card | Level::Kept | Level::FloatingBar => NSStatusWindowLevel,
            Level::Peek => NSStatusWindowLevel + 1,
            Level::DockedBar => NSMainMenuWindowLevel + 2,
            Level::Quick => NSMainMenuWindowLevel + 3,
        }
    }
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

impl Spaces {
    /// The collection behaviour that says so.
    pub fn behavior(self) -> NSWindowCollectionBehavior {
        let everywhere = NSWindowCollectionBehavior::CanJoinAllSpaces | NSWindowCollectionBehavior::FullScreenAuxiliary | NSWindowCollectionBehavior::IgnoresCycle;
        match self {
            Spaces::Stationary => everywhere | NSWindowCollectionBehavior::Stationary,
            Spaces::Transient => everywhere | NSWindowCollectionBehavior::Transient,
        }
    }
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

/// What kind of panel a window is made into (`make`).
#[derive(Clone, Copy, Debug)]
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

/// AppKit is the main thread's. Off it there's no marker, so the caller does nothing, as these
/// functions always have; a debug build stops instead, to say who called.
fn main_thread() -> Option<MainThreadMarker> {
    let mtm = MainThreadMarker::new();
    debug_assert!(mtm.is_some(), "panel.rs was called off the main thread, where AppKit isn't to be called");
    mtm
}

/// Whether this is the main thread, for a caller that may be on either.
pub fn on_main_thread() -> bool {
    MainThreadMarker::new().is_some()
}

/// The window's NSWindow, on the main thread only.
fn ns(w: &WebviewWindow) -> Option<&NSWindow> {
    main_thread()?;
    let ptr = w.ns_window().ok()? as *const NSWindow;
    // SAFETY: Tauri hands out the window's own NSWindow, which lives as long as the window,
    // and this is the main thread, where AppKit expects to be called.
    unsafe { ptr.as_ref() }
}

/// The panels that have the keyboard or are taking it, by their address: the docked bar, while
/// it shows a session to answer, the peek, the sessions kept on the desktop. They answer AppKit
/// truthfully about being key; the others say they are (`Keys`). The floating bar and the cards
/// on the desktop are never here.
static ENGAGED: Mutex<Vec<usize>> = Mutex::new(Vec::new());

fn engaged(this: *const AnyObject) -> bool {
    ENGAGED.lock().is_ok_and(|e| e.contains(&(this as usize)))
}

/// Whether the panel answers truthfully about being key (`on`), or says it is.
fn engage(n: &NSWindow, on: bool) {
    note(n as *const NSWindow as usize, on);
}

fn note(this: usize, on: bool) {
    if let Ok(mut e) = ENGAGED.lock() {
        e.retain(|x| *x != this);
        if on {
            e.push(this);
        }
    }
}

/// The NSPanel subclass for a way with the keyboard, made the first time it's asked for. It
/// overrides only what that way answers differently from a panel.
fn class(keys: Keys, unconstrained: bool) -> &'static AnyClass {
    extern "C" fn yes(_: &AnyObject, _: Sel) -> Bool {
        Bool::YES
    }
    extern "C" fn no(_: &AnyObject, _: Sel) -> Bool {
        Bool::NO
    }
    extern "C" fn if_engaged(this: &AnyObject, _: Sel) -> Bool {
        Bool::new(engaged(this))
    }
    extern "C" fn is_key(this: &AnyObject, _: Sel) -> Bool {
        if !engaged(this) {
            return Bool::YES;
        }
        // SAFETY: NSWindow's own `- (BOOL)isKeyWindow`, asked of the panel it was asked of.
        unsafe { msg_send![super(this, NSPanel::class()), isKeyWindow] }
    }
    extern "C" fn as_asked(_: &AnyObject, _: Sel, frame: NSRect, _: *const AnyObject) -> NSRect {
        frame
    }
    static CLASSES: Mutex<Vec<(Keys, bool, &'static AnyClass)>> = Mutex::new(Vec::new());
    let mut classes = CLASSES.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((_, _, made)) = classes.iter().find(|(k, u, _)| *k == keys && *u == unconstrained) {
        return made;
    }
    let way = match keys {
        Keys::WhenAsked => "KeyWhenAsked",
        Keys::OnClick => "KeyOnClick",
        Keys::Always => "KeyAlways",
    };
    let name = CString::new(format!("CodealongPanel{way}{}", if unconstrained { "Unconstrained" } else { "" })).expect("a class name without a NUL");
    let made = match ClassBuilder::new(&name, NSPanel::class()) {
        Some(mut builder) => {
            // SAFETY: the overrides match NSWindow's `- (BOOL)canBecome…Window`,
            // `- (BOOL)isKeyWindow` and
            // `- (NSRect)constrainFrameRect:(NSRect)frame toScreen:(NSScreen *)screen`.
            unsafe {
                match keys {
                    Keys::WhenAsked => builder.add_method(sel!(canBecomeKeyWindow), if_engaged as extern "C" fn(_, _) -> _),
                    Keys::OnClick | Keys::Always => builder.add_method(sel!(canBecomeKeyWindow), yes as extern "C" fn(_, _) -> _),
                }
                builder.add_method(sel!(canBecomeMainWindow), no as extern "C" fn(_, _) -> _);
                if keys != Keys::Always {
                    builder.add_method(sel!(isKeyWindow), is_key as extern "C" fn(_, _) -> _);
                }
                if unconstrained {
                    builder.add_method(sel!(constrainFrameRect:toScreen:), as_asked as extern "C" fn(_, _, _, _) -> _);
                }
            }
            builder.register()
        }
        None => AnyClass::get(&name).unwrap_or(NSPanel::class()),
    };
    classes.push((keys, unconstrained, made));
    made
}

/// The class a window had before `make` swapped it.
pub struct Original(&'static AnyClass);

/// Turns a window into a non-activating panel, the only kind macOS puts on another app's
/// full-screen Space. Tauri makes windows, so the class is swapped after the fact, and before
/// the window is first shown: one shown as a window stays off full-screen Spaces. WebKit asks
/// again whether the window is key each time it's ordered on screen, which `show` does after
/// this, so the page counts as active from its first showing.
///
/// Then it's put at its level, on every Space and over full-screen apps, kept out of ⌘` and the
/// Window menu, and still up when Codealong is hidden (⌘H), without bringing its other windows
/// back.
///
/// Returns the class it replaced, which a window must get back before it's destroyed
/// (`unmake`). The bar, its peek, the quick panel and the lines never are.
pub fn make(w: &WebviewWindow, config: Config) -> Option<Original> {
    let n = ns(w)?;
    let original = swap(n, config.keys, config.unconstrained);
    match (config.steps, original.is_some()) {
        (Steps::Two, made) => {
            if made && config.tooltips {
                n.setAllowsToolTipsWhenApplicationIsInactive(true);
            }
            place(n, config);
        }
        (Steps::One, true) => {
            place(n, config);
            if config.tooltips {
                n.setAllowsToolTipsWhenApplicationIsInactive(true);
            }
        }
        (Steps::One, false) => {}
    }
    original
}

/// The class, and what makes a panel of it.
fn swap(n: &NSWindow, keys: Keys, unconstrained: bool) -> Option<Original> {
    let obj: &AnyObject = n;
    let panel = class(keys, unconstrained);
    let original = obj.class();
    // NSPanel keeps no storage of its own beyond NSWindow's, so the swap needs no more room
    // than tao's window already has; if that ever changes, the window stays a plain window.
    if panel.instance_size() > original.instance_size() {
        return None;
    }
    // SAFETY: the object is tao's NSWindow subclass, at least as large as the panel class,
    // which only adds methods. tao reads its own ivar only when focusability changes, which
    // these windows never do after they're made.
    unsafe { AnyObject::set_class(obj, panel) };
    n.setStyleMask(n.styleMask() | NSWindowStyleMask::NonactivatingPanel);
    // Panels hide when their app deactivates unless told not to.
    n.setHidesOnDeactivate(false);
    Some(Original(original))
}

/// Where it floats.
fn place(n: &NSWindow, config: Config) {
    n.setLevel(config.level.raw());
    n.setCollectionBehavior(n.collectionBehavior() | config.spaces.behavior());
    n.setExcludedFromWindowsMenu(true);
    n.setCanHide(false);
}

/// Takes a panel off the screen and gives its window back the class `make` replaced, so it can
/// be destroyed. WebKit watches the window through key-value observing, which is itself a swap
/// of the window's class (to `NSKVONotifying_TaoWindow`) made when the page went in; taking the
/// page out of a window with the panel's class instead raises "Cannot remove an observer …
/// because it is not registered", an exception that aborts the app.
pub fn unmake(w: &WebviewWindow, original: Original) {
    let Some(n) = ns(w) else { return };
    n.orderOut(None);
    engage(n, false);
    n.setStyleMask(n.styleMask() & !NSWindowStyleMask::NonactivatingPanel);
    // SAFETY: the class the object had before, which `make` checked is no larger than it.
    unsafe { AnyObject::set_class(n, original.0) };
}

/// Brings the panel up without making it key or activating Codealong, whichever app is in front.
pub fn show(w: &WebviewWindow) {
    if let Some(n) = ns(w) {
        n.orderFrontRegardless();
    }
}

/// Orders the panel in over every other app, without activating Codealong. With `keys` it takes
/// the keyboard too; without, whatever app you're in keeps it, and the panel says it's key, as
/// one up from a hover does.
pub fn present(w: &WebviewWindow, keys: bool) {
    let Some(n) = ns(w) else { return };
    // WebKit reads whether the window is key as it's ordered on screen.
    engage(n, keys);
    n.orderFrontRegardless();
    if keys {
        take_keys(w);
    }
}

/// Takes the window off the screen. It has the keyboard no more, if it had.
pub fn hide(w: &WebviewWindow) {
    if let Some(n) = ns(w) {
        n.orderOut(None);
        engage(n, false);
    }
}

/// Runs `f` with the window and its page's view, on the main thread.
fn with_page(w: &WebviewWindow, f: impl FnOnce(&NSWindow, &NSResponder) + Send + 'static) {
    let window = w.clone();
    let _ = w.with_webview(move |webview| {
        let Some(n) = ns(&window) else { return };
        let ptr = webview.inner() as *const NSResponder;
        // SAFETY: on macOS `inner` is the webview's WKWebView, an NSResponder that lives as
        // long as the window; this runs on the main thread.
        if let Some(view) = unsafe { ptr.as_ref() } {
            f(n, view);
        }
    });
}

/// Makes the panel the key window, and its webview the first responder, so the page's keys
/// reach what it focused. Codealong stays inactive: the app you're in keeps its menu bar.
pub fn take_keys(w: &WebviewWindow) {
    let Some(n) = ns(w) else { return };
    engage(n, true);
    n.makeKeyWindow();
    with_page(w, |n, view| {
        n.makeFirstResponder(Some(view));
    });
}

/// Hands the keyboard back to the app you're in, with the panel still up. A key panel gives
/// the keyboard up when it's ordered out and at no other time AppKit offers, so it's ordered
/// out and in again in one turn of the run loop, which the screen shows as nothing: AppKit
/// sends a turn's window changes to the screen together. It comes back saying it's key, as
/// it does whenever it hasn't the keyboard, so its page still gets the pointer.
pub fn drop_keys(w: &WebviewWindow) {
    let Some(n) = ns(w) else { return };
    // Ordered out while it still answers truthfully, so AppKit sees a key panel going.
    if engaged(n as *const NSWindow as *const AnyObject) && n.isKeyWindow() {
        n.orderOut(None);
    }
    engage(n, false);
    // WebKit reads whether the window is key as it's ordered on screen.
    n.orderFrontRegardless();
}

/// Another of Codealong's windows has the keyboard: the quick panel, a board window. While
/// another app has it, Codealong has no key window.
pub fn other_is_key(w: &WebviewWindow) -> bool {
    let (Some(mtm), Some(n)) = (main_thread(), ns(w)) else { return false };
    NSApplication::sharedApplication(mtm).keyWindow().is_some_and(|k| !std::ptr::eq(&*k, n))
}

/// Moves the panel to another level.
pub fn set_level(w: &WebviewWindow, level: Level) {
    if let Some(n) = ns(w) {
        n.setLevel(level.raw());
    }
}

/// Whether the window takes the pointer's clicks and moves or lets them through to what's under it.
pub fn set_clickable(w: &WebviewWindow, clickable: bool) {
    if let Some(n) = ns(w) {
        n.setIgnoresMouseEvents(!clickable);
    }
}

/// Shows and hides the window at once, without AppKit's scaling it in: what it draws is placed
/// by the screen, and is off its place while the window is scaled.
pub fn no_animation(w: &WebviewWindow) {
    if let Some(n) = ns(w) {
        n.setAnimationBehavior(NSWindowAnimationBehavior::None);
    }
}

/// How much of the window shows through: 0 leaves it up, where it was, and unseen.
pub fn set_alpha(w: &WebviewWindow, alpha: f64) {
    if let Some(n) = ns(w) {
        if n.alphaValue() != alpha {
            n.setAlphaValue(alpha);
        }
    }
}

/// Ordered in, wherever it is.
pub fn is_visible(w: &WebviewWindow) -> bool {
    ns(w).map(|n| n.isVisible()).unwrap_or(false)
}

/// Ordered in and some part of it on screen. These sit above ordinary windows, so when one is
/// ordered in but can't be seen, macOS kept it off the Space in front.
pub fn on_screen(w: &WebviewWindow) -> bool {
    ns(w).map(|n| n.isVisible() && n.occlusionState().contains(NSWindowOcclusionState::Visible)).unwrap_or(true)
}

fn rect(r: NSRect) -> Rect {
    Rect { x: r.origin.x, y: r.origin.y, w: r.size.width, h: r.size.height }
}

/// A window's frame, in screen points: any window's, not only a panel's.
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
    let Some(mtm) = main_thread() else { return Vec::new() };
    let all = NSScreen::screens(mtm);
    (0..all.count()).map(|i| all.objectAtIndex(i)).map(|s| Screen { frame: rect(s.frame()), visible: rect(s.visibleFrame()) }).collect()
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

/// The top of every screen, the one with the menu bar first.
pub fn tops() -> Vec<Top> {
    let Some(mtm) = main_thread() else { return Vec::new() };
    let all = NSScreen::screens(mtm);
    (0..all.count())
        .map(|i| all.objectAtIndex(i))
        .map(|s| Top {
            screen: Screen { frame: rect(s.frame()), visible: rect(s.visibleFrame()) },
            safe_top: s.safeAreaInsets().top,
            sides: (s.auxiliaryTopLeftArea().size.width, s.auxiliaryTopRightArea().size.width),
        })
        .collect()
}

/// Where the pointer is, in screen points.
pub fn pointer() -> Option<(f64, f64)> {
    main_thread()?;
    let p = NSEvent::mouseLocation();
    Some((p.x, p.y))
}

/// The mouse's main button is down, whichever app has the click.
pub fn button_down() -> bool {
    NSEvent::pressedMouseButtons() & 1 != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_levels_stack_as_the_windows_do() {
        let at = Level::raw;
        // The lines are under the cards they join, and the cards no higher than the bar.
        assert!(at(Level::Lines) < at(Level::Card));
        assert!(at(Level::Card) <= at(Level::FloatingBar));
        assert!(at(Level::Card) < at(Level::DockedBar));
        assert_eq!(at(Level::Kept), at(Level::Card));
        // Docked, the bar is over the menu bar and its status items, and the peek is over the
        // floating bar and other apps' status items.
        assert!(at(Level::DockedBar) > NSMainMenuWindowLevel && at(Level::DockedBar) > NSStatusWindowLevel);
        assert!(at(Level::Peek) > at(Level::FloatingBar) && at(Level::Peek) > NSStatusWindowLevel);
        // The quick panel is over them all, the docked bar and the peek too.
        assert!(at(Level::Quick) > at(Level::DockedBar) && at(Level::Quick) > at(Level::Peek));
        // As AppKit counts them: the menu bar is 24 and its status items 25.
        assert_eq!([Level::Lines, Level::Card, Level::Kept, Level::FloatingBar, Level::Peek, Level::DockedBar, Level::Quick].map(at), [24, 25, 25, 25, 26, 26, 27]);
    }

    /// Which of a panel's answers a class gives itself, rather than leaving them to NSPanel.
    fn overrides(keys: Keys, unconstrained: bool) -> [bool; 4] {
        let class = class(keys, unconstrained);
        [sel!(canBecomeKeyWindow), sel!(canBecomeMainWindow), sel!(isKeyWindow), sel!(constrainFrameRect:toScreen:)].map(|s| {
            let of = |c: &AnyClass| c.instance_method(s).map(|m| m.implementation() as usize);
            of(class) != of(NSPanel::class())
        })
    }

    #[test]
    fn a_class_overrides_only_what_its_way_with_the_keyboard_answers() {
        // The bar's, the cards' and the lines': all four.
        assert_eq!(overrides(Keys::WhenAsked, true), [true, true, true, true]);
        assert_eq!(overrides(Keys::WhenAsked, false), [true, true, true, false]);
        // The peek's and a kept session's: AppKit places them as it places any window.
        assert_eq!(overrides(Keys::OnClick, false), [true, true, true, false]);
        assert!(!std::ptr::eq(class(Keys::OnClick, false), class(Keys::WhenAsked, false)));
        // The quick panel's: whether it's key is AppKit's to say.
        assert_eq!(overrides(Keys::Always, false), [true, true, false, false]);
        // Asked for again, it's the same class.
        assert!(std::ptr::eq(class(Keys::WhenAsked, true), class(Keys::WhenAsked, true)));
        assert!(!std::ptr::eq(class(Keys::WhenAsked, true), class(Keys::WhenAsked, false)));
    }

    #[test]
    fn a_panel_answers_truthfully_only_while_it_has_the_keyboard() {
        // Addresses no window has.
        let (a, b) = (0x10usize, 0x20usize);
        assert!(!engaged(a as *const AnyObject));
        note(a, true);
        note(a, true);
        note(b, true);
        assert!(engaged(a as *const AnyObject) && engaged(b as *const AnyObject));
        // Taken once, however often it was given.
        note(a, false);
        assert!(!engaged(a as *const AnyObject) && engaged(b as *const AnyObject));
        note(b, false);
        assert!(!engaged(b as *const AnyObject));
    }

    #[test]
    fn every_panel_is_on_every_space_and_out_of_the_window_cycle() {
        let everywhere = NSWindowCollectionBehavior::CanJoinAllSpaces | NSWindowCollectionBehavior::FullScreenAuxiliary | NSWindowCollectionBehavior::IgnoresCycle;
        assert_eq!(Spaces::Stationary.behavior(), everywhere | NSWindowCollectionBehavior::Stationary);
        assert_eq!(Spaces::Transient.behavior(), everywhere | NSWindowCollectionBehavior::Transient);
    }
}
