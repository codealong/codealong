//! The floating bar's peek: hovering a session's pill shows that session in a panel under the bar
//! (over it, when the bar sits low on the screen): its conversation as its panel in the main
//! window has it, and a box to answer it (Peek.tsx). It's the quickest way to answer a session:
//! hover, click the box, type, ⏎.
//!
//! Like the bar and the quick panel it's a non-activating panel, so it shows over full-screen apps
//! and leaves the app you're in active. Hovering never gives it the keyboard, so passing the
//! pointer over the bar can't take what you're typing elsewhere; a click in it does. Until then it
//! closes once the pointer has been off both its pill and the peek for a moment, and hovering
//! another pill switches it to that session. With the keyboard it stays on its session until you
//! send, press Escape, click somewhere else or bring up another of Codealong's windows, whatever
//! the bar does meanwhile. Should another app take the keyboard while the pointer is still on the
//! peek, it goes back to how a hover has it rather than closing under you.
//!
//! Opening is the quick panel's handshake: Codealong tells the page which session (`peek-open`),
//! the page reads the conversation and answers with its height (`peek_ready`), and only then does
//! the panel show, so it never flashes the session it showed before.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder, Window, WindowEvent};

use crate::bar::{Rect, Screen};
use crate::config::Theme;
use crate::model::{Board, Lane, Session};

pub const LABEL: &str = "peek";
/// Room around the card for its shadow: the window is this much bigger on its sides and on the
/// side away from the bar (Peek.tsx).
const MARGIN: f64 = 20.0;
/// On the side toward the bar the window is only this much bigger, so it never covers the bar's
/// pills (a window takes the clicks on its clear parts too). The shadow falls away from there.
const NEAR: f64 = 8.0;
/// The window's width: the card and its margins.
const WIDTH: f64 = 480.0 + 2.0 * MARGIN;
/// The tallest the window gets.
const MAX_HEIGHT: f64 = 580.0 + MARGIN + NEAR;
/// With less room than this under the bar, the peek opens over it when there's more room there.
const MIN_ROOM: f64 = 360.0;
/// The bar's page leaves this much room around its card (Bar.tsx), as a desktop card's page does
/// (DesktopPin.tsx).
const BAR_MARGIN: f64 = 12.0;
/// Space between the bar's card and the peek's: no less than `NEAR`, so the peek's window starts
/// where the bar's card ends.
const GAP: f64 = 8.0;
/// The pill counts this far past its edges, so the pointer can wobble on its way to the peek.
const PILL_SLACK: f64 = 6.0;
/// How long the pointer may be off the pill and the peek before the peek closes.
const LEAVE_AFTER: Duration = Duration::from_millis(350);
/// How often the pointer is looked at while the peek is up without the keyboard.
const WATCH_EVERY: Duration = Duration::from_millis(60);
/// How long an open waits for the page before showing anyway.
const READY_WAIT: Duration = Duration::from_millis(400);

/// Where the peek goes: its left edge, and the edge it keeps while its height changes (its top
/// under the bar, its bottom over it), in AppKit points (y grows up).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Place {
    pub x: f64,
    pub edge: f64,
    pub below: bool,
    /// The tallest the window can be there.
    pub room: f64,
}

impl Place {
    /// The window's frame at a height: hanging from its top edge under the bar, standing on its
    /// bottom edge over it.
    pub fn frame(&self, height: f64) -> Rect {
        let h = height.min(self.room).max(1.0);
        if self.below {
            Rect { x: self.x, y: self.edge - h, w: WIDTH, h }
        } else {
            Rect { x: self.x, y: self.edge, w: WIDTH, h }
        }
    }
}

/// A pill the page of the bar or of a desktop card measured (CSS pixels from the top left of its
/// window) on the screen.
pub fn pill_on_screen(host: Rect, pill: Rect) -> Rect {
    Rect { x: host.x + pill.x, y: host.top() - pill.y - pill.h, w: pill.w, h: pill.h }
}

/// Where the peek goes for a pill: under the bar's card (`bar` is its window, or a desktop card's),
/// centered on the pill and kept on the screen, or over it when there's too little room under it
/// and more over it.
pub fn place(bar: Rect, pill: Rect, screen: Rect) -> Place {
    let below_edge = bar.y + BAR_MARGIN - GAP + NEAR;
    let above_edge = bar.top() - BAR_MARGIN + GAP - NEAR;
    let room_below = below_edge - screen.y;
    let room_above = screen.top() - above_edge;
    let below = room_below >= MIN_ROOM || room_below >= room_above;
    let (cx, _) = pill.center();
    let x = (cx - WIDTH / 2.0).min(screen.right() - WIDTH).max(screen.x).round();
    let room = if below { room_below } else { room_above };
    Place { x, edge: if below { below_edge } else { above_edge }, below, room: room.min(MAX_HEIGHT) }
}

/// Whether the pointer is on the peek or on the pill it came from.
pub fn over(p: (f64, f64), peek: Rect, pill: Option<Rect>) -> bool {
    let near = |r: Rect| Rect { x: r.x - PILL_SLACK, y: r.y - PILL_SLACK, w: r.w + 2.0 * PILL_SLACK, h: r.h + 2.0 * PILL_SLACK };
    peek.contains(p) || pill.is_some_and(|r| near(r).contains(p))
}

/// What the bar and the page hear about the peek.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Shown {
    /// The session it shows, while it's up.
    pub key: Option<String>,
    /// It has the keyboard.
    pub engaged: bool,
}

/// What an open tells the page.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Opening {
    id: u64,
    lane: Lane,
    session: Session,
    terminal: String,
    theme: Theme,
    /// The tallest the window can be where it opens.
    room: f64,
    /// Under the bar, so the card sits at the window's top; over it, at its bottom.
    below: bool,
    /// It takes the keyboard as it shows (Reply here in the pill's menu), not on a click.
    focus: bool,
    /// `[debug] perf_log` is on, so the page reports its timings.
    perf_log: bool,
}

#[derive(Default)]
struct Peek {
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    /// The session shown, or about to be.
    key: Option<String>,
    /// The window its pill is in: the bar, or a card pinned to the desktop (desktop.rs).
    host: Option<Rect>,
    /// Its pill on the screen.
    pill: Option<Rect>,
    place: Option<Place>,
    opens: u64,
    /// The open the page hasn't answered yet.
    waiting: Option<u64>,
    /// The open waiting wants the keyboard.
    focus: bool,
    visible: bool,
    /// It has the keyboard.
    engaged: bool,
    /// Since when the pointer has been off the peek and its pill.
    away_since: Option<Instant>,
    /// Counts watches of the pointer, so an old one stops.
    watch: u64,
}

/// Makes the peek's window, hidden, at launch, so a hover never waits for a page to load.
pub fn setup(app: &AppHandle) -> Result<()> {
    app.manage(Peek::default());
    let window = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("index.html".into()))
        .title("Codealong")
        .initialization_script("window.__CODEALONG_PEEK__ = true;")
        .inner_size(WIDTH, 320.0)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .always_on_top(true)
        .visible_on_all_workspaces(true)
        .skip_taskbar(true)
        .focused(false)
        .accept_first_mouse(true)
        // Up over other apps, where WebKit would slow its timers and drawing.
        .background_throttling(tauri::utils::config::BackgroundThrottlingPolicy::Disabled)
        .visible(false)
        .build()?;
    native::make_panel(&window);
    Ok(())
}

/// A session to show, with what the page needs besides it.
pub struct Target {
    pub lane: Lane,
    pub session: Session,
    pub terminal: String,
    pub theme: Theme,
}

/// The session and its lane, as the board has them.
pub fn target(board: &Board, key: &str) -> Option<Target> {
    let (lane, session) = board.lanes.iter().find_map(|l| l.sessions.iter().find(|s| s.key == key).map(|s| (l.clone(), s.clone())))?;
    Some(Target { lane, session, terminal: board.apps.terminal.clone(), theme: board.theme })
}

/// Shows a session in the peek, for a hover over its pill (`pill`: the window it's in, the bar or
/// a card on the desktop, and the pill as that window's page measured it) or, with `focus`, ready
/// to type into. Without a pill it opens where it last was. A hover doesn't take the peek from a
/// session you're typing to. On the main thread.
pub fn open(app: &AppHandle, target: Target, pill: Option<(Rect, Rect)>, focus: bool) -> Result<()> {
    let Some(state) = app.try_state::<Peek>() else { return Ok(()) };
    let bar = crate::bar::frame(app);
    let pill = pill.map(|(host, p)| (host, pill_on_screen(host, p)));
    let key = target.session.key.clone();
    let (id, room, below) = {
        let mut s = state.inner.lock().map_err(|_| anyhow!("The peek is unavailable."))?;
        let same = s.key.as_deref() == Some(key.as_str());
        if s.visible && s.engaged && !focus && !same {
            return Ok(());
        }
        if let Some((host, p)) = pill {
            s.host = Some(host);
            s.pill = Some(p);
        }
        if s.visible && same && !focus {
            s.away_since = None;
            return Ok(());
        }
        let screens = native::screens();
        let host = s.host.or(bar);
        let anchor = s.pill.or(host);
        let screen = anchor.and_then(|a| screens.iter().find(|x| x.frame.contains(a.center())).or(screens.first()).map(|x| x.visible));
        if let (Some(h), Some(a), Some(sc)) = (host, anchor, screen) {
            s.place = Some(place(h, a, sc));
        }
        s.opens += 1;
        s.waiting = Some(s.opens);
        s.focus = focus;
        s.key = Some(key);
        s.away_since = None;
        (s.opens, s.place.map_or(MAX_HEIGHT, |p| p.room), s.place.is_none_or(|p| p.below))
    };
    let Target { lane, session, terminal, theme } = target;
    let _ = app.emit_to(LABEL, "peek-open", Opening { id, lane, session, terminal, theme, room, below, focus, perf_log: crate::perf::enabled() });
    // A page that doesn't answer doesn't keep the peek from showing.
    let handle = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(READY_WAIT);
        let app = handle.clone();
        let _ = handle.run_on_main_thread(move || ready(&app, id, None));
    });
    Ok(())
}

/// The page has laid out the open `id` at this height: show the peek there, with the keyboard if
/// that open asked for it. A late answer to an older open is ignored. On the main thread.
pub fn ready(app: &AppHandle, id: u64, height: Option<f64>) {
    let (Some(state), Some(window)) = (app.try_state::<Peek>(), app.get_webview_window(LABEL)) else { return };
    let (frame, focus, watch) = {
        let Ok(mut s) = state.inner.lock() else { return };
        if s.waiting != Some(id) {
            return;
        }
        let height = height.or_else(|| native::frame(&window).map(|f| f.h)).unwrap_or(320.0);
        // Nowhere to put it (the bar isn't there), or the pointer moved on while the page read
        // the conversation: nothing to show it for, and the page hears so.
        let away = |f: &Rect| !s.focus && !s.visible && native::pointer().is_some_and(|p| !over(p, *f, s.pill));
        let Some(frame) = s.place.map(|p| p.frame(height)).filter(|f| !away(f)) else {
            drop(s);
            hide(app);
            return;
        };
        s.waiting = None;
        s.visible = true;
        if s.focus {
            s.engaged = true;
        }
        s.watch += 1;
        (frame, s.focus, (!s.focus).then_some(s.watch))
    };
    native::set_frame(&window, frame);
    native::show(&window, focus);
    if let Some(n) = watch {
        watch_pointer(app, n);
    }
    tell(app);
}

/// The page changed size while up: the peek follows, keeping its edge by the bar. On the main
/// thread.
pub fn fit(app: &AppHandle, height: f64) {
    let (Some(state), Some(window)) = (app.try_state::<Peek>(), app.get_webview_window(LABEL)) else { return };
    let Some(frame) = state.inner.lock().ok().filter(|s| s.visible && s.waiting.is_none()).and_then(|s| s.place).map(|p| p.frame(height.ceil())) else { return };
    if native::frame(&window) != Some(frame) {
        native::set_frame(&window, frame);
    }
}

/// Gives the peek the keyboard: you clicked in it. On the main thread.
pub fn take_keys(app: &AppHandle) {
    let (Some(state), Some(window)) = (app.try_state::<Peek>(), app.get_webview_window(LABEL)) else { return };
    if state.inner.lock().is_ok_and(|s| s.visible) {
        native::take_keys(&window);
    }
}

/// Hides the peek. On the main thread.
pub fn hide(app: &AppHandle) {
    let (Some(state), Some(window)) = (app.try_state::<Peek>(), app.get_webview_window(LABEL)) else { return };
    let was = {
        let Ok(mut s) = state.inner.lock() else { return };
        let was = s.visible || s.waiting.is_some();
        s.visible = false;
        s.engaged = false;
        s.waiting = None;
        s.key = None;
        s.away_since = None;
        s.watch += 1;
        was
    };
    native::hide(&window);
    if was {
        let _ = app.emit_to(LABEL, "peek-hidden", ());
        tell(app);
    }
}

/// Tells the bar and the cards on the desktop (which mark the pill or card) and the page what the
/// peek shows.
fn tell(app: &AppHandle) {
    let Some(state) = app.try_state::<Peek>() else { return };
    let Some(shown) = state.inner.lock().ok().map(|s| Shown { key: s.key.clone().filter(|_| s.visible), engaged: s.engaged }) else { return };
    for label in crate::desktop::labels(app) {
        let _ = app.emit_to(label.as_str(), "peek", shown.clone());
    }
    let _ = app.emit_to(crate::bar::LABEL, "peek", shown.clone());
    let _ = app.emit_to(LABEL, "peek", shown);
}

/// While the peek is up without the keyboard, looks where the pointer is and closes the peek once
/// it has been off both the peek and its pill for a moment. The bar's page only knows its own
/// window, and the peek's page gets no pointer moves off itself.
fn watch_pointer(app: &AppHandle, n: u64) {
    let handle = app.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(WATCH_EVERY);
        let current = handle.try_state::<Peek>().and_then(|s| s.inner.lock().ok().map(|s| s.watch == n && s.visible && !s.engaged));
        if current != Some(true) {
            break;
        }
        let app = handle.clone();
        if handle.run_on_main_thread(move || look(&app, n)).is_err() {
            break;
        }
    });
}

fn look(app: &AppHandle, n: u64) {
    let (Some(state), Some(window)) = (app.try_state::<Peek>(), app.get_webview_window(LABEL)) else { return };
    let (Some(p), Some(frame)) = (native::pointer(), native::frame(&window)) else { return };
    let gone = {
        let Ok(mut s) = state.inner.lock() else { return };
        if s.watch != n || !s.visible || s.engaged {
            return;
        }
        if over(p, frame, s.pill) {
            s.away_since = None;
            false
        } else {
            let since = *s.away_since.get_or_insert_with(Instant::now);
            since.elapsed() >= LEAVE_AFTER
        }
    };
    if gone {
        hide(app);
    }
}

/// The peek lost the keyboard, and Codealong's window events come after the change is made, so
/// what has it now can be asked. A click somewhere else puts the peek away, as does another of
/// Codealong's windows taking the keyboard. But the keyboard can go without you going anywhere,
/// to an app that brings a window of its own forward, so while the pointer is still on the peek or
/// its pill the peek stays up as a hover has it: a click in it takes the keyboard back, and it
/// closes once the pointer has left. On the main thread.
fn lost_keys(app: &AppHandle) {
    let (Some(state), Some(window)) = (app.try_state::<Peek>(), app.get_webview_window(LABEL)) else { return };
    let n = {
        let Ok(mut s) = state.inner.lock() else { return };
        // Hiding it takes the keyboard too, and a peek up from a hover never had it.
        if !s.visible || !s.engaged {
            return;
        }
        let on = native::pointer().zip(native::frame(&window)).is_some_and(|(p, f)| over(p, f, s.pill));
        if !on || native::other_is_key(&window) {
            drop(s);
            hide(app);
            return;
        }
        s.engaged = false;
        s.away_since = None;
        s.watch += 1;
        s.watch
    };
    // Ordered in again as a hover shows it, saying it's key, so the page gets the pointer again.
    native::show(&window, false);
    watch_pointer(app, n);
    tell(app);
}

/// Window events for the peek: it has the keyboard once it's the key window, lets go of it when
/// another window or app takes it (`lost_keys`), and closing it (⌘W) only hides it. On the main
/// thread.
pub fn on_window_event(window: &Window, event: &WindowEvent) {
    if window.label() != LABEL {
        return;
    }
    let app = window.app_handle();
    match event {
        WindowEvent::Focused(true) => {
            let changed = app.try_state::<Peek>().and_then(|s| {
                let mut s = s.inner.lock().ok()?;
                let changed = s.visible && !s.engaged;
                if s.visible {
                    s.engaged = true;
                    s.watch += 1;
                }
                Some(changed)
            });
            if changed == Some(true) {
                tell(app);
            }
        }
        WindowEvent::Focused(false) => lost_keys(app),
        WindowEvent::CloseRequested { api, .. } => {
            api.prevent_close();
            hide(app);
        }
        _ => {}
    }
}

#[cfg(target_os = "macos")]
mod native {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::OnceLock;

    use objc2::runtime::{AnyClass, AnyObject, Bool, ClassBuilder, Sel};
    use objc2::{msg_send, sel, ClassType, MainThreadMarker};
    use objc2_app_kit::{NSApplication, NSEvent, NSPanel, NSResponder, NSScreen, NSStatusWindowLevel, NSWindow, NSWindowCollectionBehavior, NSWindowStyleMask};
    use objc2_foundation::{NSPoint, NSRect, NSSize};
    use tauri::WebviewWindow;

    use super::{Rect, Screen};

    /// While up without the keyboard, the panel says it's the key window anyway, as the floating
    /// bar's does: WebKit treats a page whose window isn't key as inactive, sending it no pointer
    /// moves, so nothing in it would show hover and no tooltip would appear. Taking the keyboard
    /// turns this off first, so AppKit sees the panel isn't key yet and makes it so.
    static SAYS_KEY: AtomicBool = AtomicBool::new(true);

    /// The window's NSWindow, on the main thread only.
    fn ns(w: &WebviewWindow) -> Option<&NSWindow> {
        MainThreadMarker::new()?;
        let ptr = w.ns_window().ok()? as *const NSWindow;
        // SAFETY: Tauri hands out the window's own NSWindow, which lives as long as the window,
        // and this is the main thread, where AppKit expects to be called.
        unsafe { ptr.as_ref() }
    }

    /// NSPanel that can take the keyboard but never becomes the main window, so a board window
    /// behind it stays the one menu commands are for.
    fn panel_class() -> &'static AnyClass {
        static CLASS: OnceLock<&'static AnyClass> = OnceLock::new();
        CLASS.get_or_init(|| {
            extern "C" fn yes(_: &AnyObject, _: Sel) -> Bool {
                Bool::YES
            }
            extern "C" fn no(_: &AnyObject, _: Sel) -> Bool {
                Bool::NO
            }
            extern "C" fn is_key(this: &AnyObject, _: Sel) -> Bool {
                if SAYS_KEY.load(Ordering::Relaxed) {
                    return Bool::YES;
                }
                // SAFETY: NSWindow's own `- (BOOL)isKeyWindow`, asked of the panel it was asked of.
                unsafe { msg_send![super(this, NSPanel::class()), isKeyWindow] }
            }
            match ClassBuilder::new(c"CodealongPeekPanel", NSPanel::class()) {
                Some(mut builder) => {
                    // SAFETY: the overrides match NSWindow's `- (BOOL)canBecome…Window` and
                    // `- (BOOL)isKeyWindow`.
                    unsafe {
                        builder.add_method(sel!(canBecomeKeyWindow), yes as extern "C" fn(_, _) -> _);
                        builder.add_method(sel!(canBecomeMainWindow), no as extern "C" fn(_, _) -> _);
                        builder.add_method(sel!(isKeyWindow), is_key as extern "C" fn(_, _) -> _);
                    }
                    builder.register()
                }
                None => AnyClass::get(c"CodealongPeekPanel").unwrap_or(NSPanel::class()),
            }
        })
    }

    /// Turns the window into a non-activating panel that can take the keyboard, over the floating
    /// bar, on whichever Space is in front, full-screen ones too.
    pub fn make_panel(w: &WebviewWindow) {
        let Some(n) = ns(w) else { return };
        let obj: &AnyObject = n;
        let panel = panel_class();
        // As for the floating bar (bar.rs): NSPanel adds no storage to NSWindow, so tao's window
        // has room for it; if that ever changes, the peek stays a plain window.
        if panel.instance_size() > obj.class().instance_size() {
            return;
        }
        // SAFETY: the object is tao's NSWindow subclass, at least as large as the panel class,
        // which only adds methods. tao reads its own ivar only when focusability changes, which
        // the peek never does after it's made.
        unsafe { AnyObject::set_class(obj, panel) };
        n.setStyleMask(n.styleMask() | NSWindowStyleMask::NonactivatingPanel);
        n.setHidesOnDeactivate(false);
        n.setLevel(NSStatusWindowLevel + 1);
        n.setCollectionBehavior(
            n.collectionBehavior()
                | NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::FullScreenAuxiliary
                | NSWindowCollectionBehavior::Transient
                | NSWindowCollectionBehavior::IgnoresCycle,
        );
        n.setExcludedFromWindowsMenu(true);
        n.setCanHide(false);
        // Codealong is nearly always in the background while the peek is up.
        n.setAllowsToolTipsWhenApplicationIsInactive(true);
    }

    /// Orders the peek in over every other app, without activating Codealong. With `keys` it
    /// takes the keyboard too; without, whatever app you're in keeps it.
    pub fn show(w: &WebviewWindow, keys: bool) {
        let Some(n) = ns(w) else { return };
        // WebKit reads whether the window is key as it's ordered on screen.
        SAYS_KEY.store(!keys, Ordering::Relaxed);
        n.orderFrontRegardless();
        if keys {
            take_keys(w);
        }
    }

    /// Makes the peek the key window, and its webview the first responder, so the page's keys
    /// reach what it focused.
    pub fn take_keys(w: &WebviewWindow) {
        let Some(n) = ns(w) else { return };
        SAYS_KEY.store(false, Ordering::Relaxed);
        n.makeKeyWindow();
        let window = w.clone();
        let _ = w.with_webview(move |webview| {
            let Some(n) = ns(&window) else { return };
            let ptr = webview.inner() as *const NSResponder;
            // SAFETY: on macOS `inner` is the webview's WKWebView, an NSResponder that lives as
            // long as the window; this runs on the main thread.
            if let Some(view) = unsafe { ptr.as_ref() } {
                n.makeFirstResponder(Some(view));
            }
        });
    }

    pub fn hide(w: &WebviewWindow) {
        if let Some(n) = ns(w) {
            n.orderOut(None);
        }
        SAYS_KEY.store(true, Ordering::Relaxed);
    }

    fn rect(r: NSRect) -> Rect {
        Rect { x: r.origin.x, y: r.origin.y, w: r.size.width, h: r.size.height }
    }

    pub fn frame(w: &WebviewWindow) -> Option<Rect> {
        ns(w).map(|n| rect(n.frame()))
    }

    pub fn set_frame(w: &WebviewWindow, r: Rect) {
        if let Some(n) = ns(w) {
            n.setFrame_display(NSRect::new(NSPoint::new(r.x, r.y), NSSize::new(r.w, r.h)), true);
        }
    }

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

    /// Another of Codealong's windows has the keyboard: the quick panel, a board window. While
    /// another app has it, Codealong has no key window.
    pub fn other_is_key(w: &WebviewWindow) -> bool {
        let (Some(mtm), Some(n)) = (MainThreadMarker::new(), ns(w)) else { return false };
        NSApplication::sharedApplication(mtm).keyWindow().is_some_and(|k| !std::ptr::eq(&*k, n))
    }
}

/// Elsewhere the peek is an ordinary window.
#[cfg(not(target_os = "macos"))]
mod native {
    use tauri::WebviewWindow;

    use super::{Rect, Screen};

    pub fn make_panel(_: &WebviewWindow) {}
    pub fn show(w: &WebviewWindow, keys: bool) {
        let _ = w.show();
        if keys {
            let _ = w.set_focus();
        }
    }
    pub fn take_keys(w: &WebviewWindow) {
        let _ = w.set_focus();
    }
    pub fn hide(w: &WebviewWindow) {
        let _ = w.hide();
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
    pub fn other_is_key(_: &WebviewWindow) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: Rect = Rect { x: 0.0, y: 0.0, w: 1500.0, h: 875.0 };

    fn r(x: f64, y: f64, w: f64, h: f64) -> Rect {
        Rect { x, y, w, h }
    }

    #[test]
    fn finds_the_pill_on_the_screen() {
        // A bar at the top of the screen; the page measures from its window's top left.
        let bar = r(500.0, 800.0, 600.0, 56.0);
        assert_eq!(pill_on_screen(bar, r(100.0, 17.0, 120.0, 22.0)), r(600.0, 817.0, 120.0, 22.0));
    }

    #[test]
    fn hangs_under_the_bar_centered_on_the_pill() {
        let bar = r(500.0, 800.0, 600.0, 56.0);
        let pill = r(600.0, 817.0, 120.0, 22.0);
        let p = place(bar, pill, SCREEN);
        assert!(p.below);
        // The window starts where the bar's card ends, so it never covers a pill.
        assert_eq!(p.edge, 800.0 + BAR_MARGIN);
        assert_eq!(p.x, 660.0 - WIDTH / 2.0);
        assert_eq!(p.room, MAX_HEIGHT);
        // It grows down from its top edge.
        let f = p.frame(300.0);
        assert_eq!((f.top(), f.h, f.w), (p.edge, 300.0, WIDTH));
        // It's never taller than the room it has.
        assert_eq!(p.frame(5000.0).h, MAX_HEIGHT);
    }

    #[test]
    fn stays_on_the_screen_by_a_pill_near_its_edge() {
        let bar = r(0.0, 800.0, 400.0, 56.0);
        assert_eq!(place(bar, r(20.0, 817.0, 80.0, 22.0), SCREEN).x, 0.0);
        let bar = r(1100.0, 800.0, 400.0, 56.0);
        assert_eq!(place(bar, r(1420.0, 817.0, 80.0, 22.0), SCREEN).x, 1500.0 - WIDTH);
    }

    #[test]
    fn opens_over_a_bar_that_sits_low() {
        let bar = r(500.0, 40.0, 600.0, 56.0);
        let p = place(bar, r(600.0, 57.0, 120.0, 22.0), SCREEN);
        assert!(!p.below);
        assert_eq!(p.edge, 96.0 - BAR_MARGIN);
        // It grows up from its bottom edge.
        assert_eq!(p.frame(300.0).y, p.edge);
        // A little room under it is still enough when there's less over it.
        let short = Rect { h: 500.0, ..SCREEN };
        assert!(place(r(500.0, 250.0, 600.0, 56.0), r(600.0, 267.0, 120.0, 22.0), short).below);
        assert!(!place(r(500.0, 200.0, 600.0, 56.0), r(600.0, 217.0, 120.0, 22.0), short).below);
    }

    #[test]
    fn knows_when_the_pointer_left() {
        let peek = r(400.0, 300.0, WIDTH, 500.0);
        let pill = Some(r(600.0, 817.0, 120.0, 22.0));
        assert!(over((500.0, 500.0), peek, pill));
        assert!(over((650.0, 830.0), peek, pill));
        // A wobble just off the pill still counts.
        assert!(over((597.0, 830.0), peek, pill));
        assert!(!over((580.0, 830.0), peek, pill));
        assert!(!over((200.0, 500.0), peek, None));
    }
}
