//! The quick panel: one input, and what you can do next below it. With nothing typed it lists
//! the sessions that need you; typing finds sessions; its last row starts a new one with what
//! you typed as the prompt. A shortcut that works in every app (`[shortcuts] quick_panel` in
//! config.toml) opens it, and so do ⌘N and New session in the board's windows, at what's
//! selected there.
//!
//! Like the floating bar it's a non-activating panel: macOS shows only those on another app's
//! full-screen Space, and opening one leaves the app you're in active, with Codealong's windows
//! where they were. Unlike the bar it takes the keyboard. It hides when it loses the keyboard,
//! as Spotlight does.
//!
//! Opening is a handshake. Codealong tells the page what it opens for (`quick-open`); the page
//! reads the board, lays itself out and answers with its height (`quick_ready`); only then does
//! the panel show, so it never flashes what it showed last time.

use std::str::FromStr;
use std::sync::Mutex;
use std::time::Duration;

use anyhow::Result;
use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder, Window, WindowEvent};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut};

use crate::bar::{Rect, Screen};
use crate::start::Place;

pub const LABEL: &str = "quick";
/// The window's width: the page's card and the room its shadow needs on each side.
const WIDTH: f64 = 760.0;
/// Its height before the page has measured itself.
const START_HEIGHT: f64 = 240.0;
/// Where its top edge sits: this far down the usable part of the screen, as Spotlight's does.
const TOP: f64 = 0.18;
/// How long an open waits for the page before showing anyway.
const READY_WAIT: Duration = Duration::from_millis(600);

#[derive(Default)]
struct Quick {
    /// Where the panel's left and top edges are while it's up, in AppKit points (y grows up). It
    /// grows and shrinks downward from the top as the list changes.
    anchor: Mutex<Option<(f64, f64)>>,
    /// Counts opens, so a late answer from the page only shows the panel for the latest one.
    opens: Mutex<u64>,
    /// The open the page hasn't answered yet.
    waiting: Mutex<Option<u64>>,
    shortcut: Mutex<ShortcutState>,
}

#[derive(Default)]
struct ShortcutState {
    /// As config.toml has it; None until it's first read.
    wanted: Option<String>,
    registered: Option<Shortcut>,
    error: Option<String>,
    /// Off while Settings records a new one, so pressing the current one records it.
    paused: bool,
}

/// The shortcut as Settings shows it.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutStatus {
    /// As config.toml has it; empty when it's off.
    pub shortcut: String,
    /// Why it isn't working, when it isn't.
    pub error: Option<String>,
}

/// Makes the panel's window, hidden. It's made at launch so that opening never waits for a page
/// to load.
pub fn setup(app: &AppHandle) -> Result<()> {
    app.manage(Quick::default());
    let window = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("index.html".into()))
        .title("Codealong")
        .initialization_script("window.__CODEALONG_QUICK__ = true;")
        .inner_size(WIDTH, START_HEIGHT)
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
        // It's up while another app is frontmost, when WebKit would slow its timers.
        .background_throttling(tauri::utils::config::BackgroundThrottlingPolicy::Disabled)
        .visible(false)
        .build()?;
    native::make_panel(&window);
    Ok(())
}

/// Opens the panel, or brings it back up, for `context`: what a board window asks it to start
/// in (`laneId`, `project`, `todo`, `prompt`, `opener`), or nothing for the shortcut. On the main
/// thread.
pub fn open(app: &AppHandle, context: Value) {
    let (Some(state), Some(window)) = (app.try_state::<Quick>(), app.get_webview_window(LABEL)) else { return };
    let id = {
        let Ok(mut opens) = state.opens.lock() else { return };
        *opens += 1;
        *opens
    };
    // It opens where you are: on the screen with the pointer, centered, near the top.
    if !native::is_visible(&window) {
        if let Some(s) = native::pointer_screen() {
            let anchor = anchor_on(s.visible);
            if let Ok(mut a) = state.anchor.lock() {
                *a = Some(anchor);
            }
        }
    }
    if let Ok(mut w) = state.waiting.lock() {
        *w = Some(id);
    }
    let _ = app.emit_to(LABEL, "quick-open", with_room(context, room(&state)));
    // A page that doesn't answer (still loading, say) doesn't keep the panel from showing.
    let handle = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(READY_WAIT);
        let app = handle.clone();
        let _ = handle.run_on_main_thread(move || {
            let waiting = app.try_state::<Quick>().and_then(|s| s.waiting.lock().ok().map(|w| *w));
            if waiting == Some(Some(id)) {
                ready(&app, None);
            }
        });
    });
}

/// Where a panel opened on this screen sits: centered, its top edge `TOP` of the way down.
fn anchor_on(visible: Rect) -> (f64, f64) {
    let x = visible.x + ((visible.w - WIDTH) / 2.0).max(0.0);
    let top = visible.y + visible.h - (visible.h * TOP).round();
    (x.round(), top)
}

/// The page is laid out for the latest open and measured itself: show the panel at that height
/// and give it the keyboard. On the main thread.
pub fn ready(app: &AppHandle, height: Option<f64>) {
    let (Some(state), Some(window)) = (app.try_state::<Quick>(), app.get_webview_window(LABEL)) else { return };
    if let Ok(mut w) = state.waiting.lock() {
        if w.is_none() {
            // Not opening: the page just changed size.
            drop(w);
            if let Some(h) = height {
                fit(app, h);
            }
            return;
        }
        *w = None;
    }
    let height = height.or_else(|| native::frame(&window).map(|f| f.h)).unwrap_or(START_HEIGHT);
    if let Some(frame) = frame_for(&state, height) {
        native::set_frame(&window, frame);
    }
    native::present(&window);
}

/// How tall the panel can be where it's anchored: from its top edge down to the bottom of the
/// screen's usable part.
fn room(state: &Quick) -> Option<f64> {
    let (x, top) = (*state.anchor.lock().ok()?)?;
    let screen = native::screens().into_iter().find(|s| s.frame.contains((x + WIDTH / 2.0, top - 1.0)))?;
    Some(top - screen.visible.y)
}

/// What an open tells the page, with the room it has. The page keeps its card within it and
/// scrolls its list, so the panel's bottom, where a new session starts, stays on a short screen.
fn with_room(context: Value, room: Option<f64>) -> Value {
    let Some(room) = room else { return context };
    let mut fields = match context {
        Value::Object(fields) => fields,
        _ => serde_json::Map::new(),
    };
    fields.insert("room".into(), room.into());
    Value::Object(fields)
}

/// The panel's frame at a height, from its anchor, kept on the screen.
fn frame_for(state: &Quick, height: f64) -> Option<Rect> {
    let (x, top) = (*state.anchor.lock().ok()?)?;
    let height = room(state).map_or(height, |r| height.min(r)).max(1.0);
    Some(Rect { x, y: top - height, w: WIDTH, h: height })
}

/// The page changed size while up: the panel follows, its top edge where it was. On the main
/// thread.
pub fn fit(app: &AppHandle, height: f64) {
    let (Some(state), Some(window)) = (app.try_state::<Quick>(), app.get_webview_window(LABEL)) else { return };
    if !native::is_visible(&window) {
        return;
    }
    if let Some(frame) = frame_for(&state, height.ceil()) {
        if native::frame(&window) != Some(frame) {
            native::set_frame(&window, frame);
        }
    }
}

/// Hides the panel. What you typed stays for next time. On the main thread.
pub fn hide(app: &AppHandle) {
    if let Some(state) = app.try_state::<Quick>() {
        if let Ok(mut w) = state.waiting.lock() {
            *w = None;
        }
    }
    if let Some(window) = app.get_webview_window(LABEL) {
        if native::is_visible(&window) {
            native::hide(&window);
            let _ = app.emit_to(LABEL, "quick-hidden", ());
        }
    }
}

/// The shortcut: opens the panel, or closes it when it's up. On the main thread.
pub fn toggle(app: &AppHandle) {
    match app.get_webview_window(LABEL) {
        Some(w) if native::is_visible(&w) => hide(app),
        Some(_) => open(app, Value::Null),
        None => {}
    }
}

/// Window events for the panel: it hides when another window or app takes the keyboard, and
/// closing it (⌘W) only hides it. On the main thread.
pub fn on_window_event(window: &Window, event: &WindowEvent) {
    if window.label() != LABEL {
        return;
    }
    match event {
        WindowEvent::Focused(false) => hide(window.app_handle()),
        WindowEvent::CloseRequested { api, .. } => {
            api.prevent_close();
            hide(window.app_handle());
        }
        _ => {}
    }
}

/// The app a start lands in, by bundle ID, so it can be asked to come forward: a click in the
/// panel leaves Codealong inactive, and the app it started in may be behind the one you were in.
/// `terminal` is the terminal's profile in config.toml.
pub fn start_app(place: Place, terminal: &str) -> Option<&'static str> {
    match place {
        Place::Terminal => (terminal == "ghostty").then_some("com.mitchellh.ghostty"),
        // Nothing opens: the session runs with no window.
        Place::Background => None,
        Place::ClaudeApp => Some("com.anthropic.claudefordesktop"),
        Place::CodexApp => Some("com.openai.codex"),
    }
}

/// A shortcut as config.toml writes it (`Option+Shift+Space`), or None when it's off (empty).
/// It needs ⌃, ⌥ or ⌘ unless it's a function key, since the panel would otherwise take a key
/// you type; and ⌘ alone with a key is a menu shortcut in every app.
pub fn parse_shortcut(text: &str) -> std::result::Result<Option<Shortcut>, String> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    let shortcut = Shortcut::from_str(text).map_err(|_| format!("\"{text}\" isn't a shortcut Codealong can read. Write it like \"Option+Shift+Space\" or \"Cmd+Ctrl+K\"."))?;
    let function_key = matches!(
        shortcut.key,
        Code::F1 | Code::F2 | Code::F3 | Code::F4 | Code::F5 | Code::F6 | Code::F7 | Code::F8 | Code::F9 | Code::F10 | Code::F11 | Code::F12
            | Code::F13 | Code::F14 | Code::F15 | Code::F16 | Code::F17 | Code::F18 | Code::F19 | Code::F20
    );
    let mods = shortcut.mods;
    if !function_key && !mods.intersects(Modifiers::CONTROL | Modifiers::ALT | Modifiers::SUPER) {
        return Err(format!("\"{text}\" needs Control, Option or Command, or it would take a key you type."));
    }
    if mods.difference(Modifiers::SHIFT) == Modifiers::SUPER && shortcut.key == Code::Tab {
        return Err("macOS keeps Cmd+Tab for switching apps.".into());
    }
    if mods == Modifiers::SUPER && !matches!(shortcut.key, Code::Space) && !function_key {
        return Err(format!("\"{text}\" is a menu shortcut in most apps. Add Control or Option."));
    }
    Ok(Some(shortcut))
}

/// Registers the shortcut config.toml asks for, in place of the one before. Off a background
/// thread only: registering waits on the main thread.
pub fn apply_shortcut(app: &AppHandle, wanted: &str) {
    let Some(state) = app.try_state::<Quick>() else { return };
    let Ok(mut s) = state.shortcut.lock() else { return };
    if s.wanted.as_deref() == Some(wanted) {
        return;
    }
    s.wanted = Some(wanted.to_string());
    register(app, &mut s);
    drop(s);
    let _ = app.emit("quick-shortcut", status(app));
}

/// Turns the shortcut off while Settings records a new one, and back on after. Off a
/// background thread only.
pub fn pause_shortcut(app: &AppHandle, paused: bool) -> ShortcutStatus {
    if let Some(state) = app.try_state::<Quick>() {
        if let Ok(mut s) = state.shortcut.lock() {
            if s.paused != paused {
                s.paused = paused;
                register(app, &mut s);
            }
        }
    }
    status(app)
}

fn register(app: &AppHandle, s: &mut ShortcutState) {
    let shortcuts = app.global_shortcut();
    if let Some(old) = s.registered.take() {
        let _ = shortcuts.unregister(old);
    }
    s.error = None;
    if s.paused {
        return;
    }
    let wanted = s.wanted.clone().unwrap_or_default();
    match parse_shortcut(&wanted) {
        Ok(None) => {}
        Ok(Some(shortcut)) => match shortcuts.register(shortcut) {
            Ok(()) => s.registered = Some(shortcut),
            Err(e) => s.error = Some(format!("macOS wouldn't take {}: {e}", wanted.trim())),
        },
        Err(e) => s.error = Some(e),
    }
}

pub fn status(app: &AppHandle) -> ShortcutStatus {
    let Some(state) = app.try_state::<Quick>() else { return ShortcutStatus { shortcut: String::new(), error: None } };
    let Ok(s) = state.shortcut.lock() else { return ShortcutStatus { shortcut: String::new(), error: None } };
    ShortcutStatus { shortcut: s.wanted.as_deref().unwrap_or_default().trim().to_string(), error: s.error.clone() }
}

#[cfg(target_os = "macos")]
mod native {
    use std::sync::OnceLock;

    use objc2::runtime::{AnyClass, AnyObject, Bool, ClassBuilder, Sel};
    use objc2::{sel, ClassType, MainThreadMarker};
    use objc2_app_kit::{NSEvent, NSPanel, NSResponder, NSScreen, NSStatusWindowLevel, NSWindow, NSWindowCollectionBehavior, NSWindowStyleMask};
    use objc2_foundation::{NSPoint, NSRect, NSSize};
    use tauri::WebviewWindow;

    use super::{Rect, Screen};

    /// The window's NSWindow, on the main thread only.
    fn ns(w: &WebviewWindow) -> Option<&NSWindow> {
        MainThreadMarker::new()?;
        let ptr = w.ns_window().ok()? as *const NSWindow;
        // SAFETY: Tauri hands out the window's own NSWindow, which lives as long as the window,
        // and this is the main thread, where AppKit expects to be called.
        unsafe { ptr.as_ref() }
    }

    /// NSPanel that takes the keyboard but never becomes the main window, so a board window
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
            match ClassBuilder::new(c"CodealongQuickPanel", NSPanel::class()) {
                Some(mut builder) => {
                    // SAFETY: both overrides match NSWindow's `- (BOOL)canBecome…Window`.
                    unsafe {
                        builder.add_method(sel!(canBecomeKeyWindow), yes as extern "C" fn(_, _) -> _);
                        builder.add_method(sel!(canBecomeMainWindow), no as extern "C" fn(_, _) -> _);
                    }
                    builder.register()
                }
                None => AnyClass::get(c"CodealongQuickPanel").unwrap_or(NSPanel::class()),
            }
        })
    }

    /// Turns the window into a non-activating panel that can take the keyboard, above other
    /// apps' windows and the floating bar, on whichever Space is in front, full-screen ones too.
    pub fn make_panel(w: &WebviewWindow) {
        let Some(n) = ns(w) else { return };
        let obj: &AnyObject = n;
        let panel = panel_class();
        // As for the floating bar (bar.rs): NSPanel adds no storage to NSWindow, so tao's window
        // has room for it; if that ever changes, the panel stays a plain window.
        if panel.instance_size() > obj.class().instance_size() {
            return;
        }
        // SAFETY: the object is tao's NSWindow subclass, at least as large as the panel class,
        // which only adds methods. tao reads its own ivar only when focusability changes, which
        // the panel never does after it's made.
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
        // Up even while Codealong is hidden (⌘H), without bringing its other windows back.
        n.setCanHide(false);
    }

    pub fn is_visible(w: &WebviewWindow) -> bool {
        ns(w).map(|n| n.isVisible()).unwrap_or(false)
    }

    /// Orders the panel in over every other app and gives it the keyboard, without activating
    /// Codealong. The page's webview takes the keys, so what the page focused gets them.
    pub fn present(w: &WebviewWindow) {
        let Some(n) = ns(w) else { return };
        n.orderFrontRegardless();
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

    /// The screen the pointer is on, else the first.
    pub fn pointer_screen() -> Option<Screen> {
        let p = NSEvent::mouseLocation();
        let all = screens();
        all.iter().find(|s| s.frame.contains((p.x, p.y))).or(all.first()).copied()
    }
}

/// Elsewhere the panel is an ordinary window.
#[cfg(not(target_os = "macos"))]
mod native {
    use tauri::WebviewWindow;

    use super::{Rect, Screen};

    pub fn make_panel(_: &WebviewWindow) {}
    pub fn is_visible(w: &WebviewWindow) -> bool {
        w.is_visible().unwrap_or(false)
    }
    pub fn present(w: &WebviewWindow) {
        let _ = w.show();
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
    pub fn pointer_screen() -> Option<Screen> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_shortcuts_as_config_toml_writes_them() {
        let s = parse_shortcut("Option+Shift+Space").unwrap().unwrap();
        assert_eq!((s.mods, s.key), (Modifiers::ALT | Modifiers::SHIFT, Code::Space));
        let s = parse_shortcut(" cmd+ctrl+k ").unwrap().unwrap();
        assert_eq!((s.mods, s.key), (Modifiers::SUPER | Modifiers::CONTROL, Code::KeyK));
        assert_eq!(parse_shortcut("Alt+`").unwrap().unwrap().key, Code::Backquote);
        assert!(parse_shortcut("").unwrap().is_none());
        assert!(parse_shortcut("  ").unwrap().is_none());
        // A function key can stand alone; Command and Space is allowed, for people who moved Spotlight.
        assert!(parse_shortcut("F18").unwrap().is_some());
        assert!(parse_shortcut("Cmd+Space").unwrap().is_some());
    }

    #[test]
    fn turns_away_shortcuts_that_would_take_typing_or_menus() {
        for bad in ["Space", "K", "Shift+K", "Shift+Space", "Cmd+K", "Cmd+Tab", "Cmd+Shift+Tab", "Option+Shift", "Option+Shift+Spacebar", "Option++Space"] {
            let err = parse_shortcut(bad).unwrap_err();
            assert!(!err.is_empty(), "{bad}");
        }
        assert!(parse_shortcut("Cmd+K").unwrap_err().contains("menu shortcut"));
        assert!(parse_shortcut("Shift+K").unwrap_err().contains("Control, Option or Command"));
    }

    #[test]
    fn opens_centered_near_the_top_of_the_screen() {
        let visible = Rect { x: 0.0, y: 0.0, w: 1512.0, h: 945.0 };
        assert_eq!(anchor_on(visible), (376.0, 945.0 - 170.0));
        // A screen to the left of the main one, in AppKit's coordinates.
        let left = Rect { x: -1920.0, y: 25.0, w: 1920.0, h: 1055.0 };
        assert_eq!(anchor_on(left), (-1340.0, 25.0 + 1055.0 - 190.0));
    }

    #[test]
    fn opens_tell_the_page_its_room() {
        let asked = serde_json::json!({ "laneId": "l1", "prompt": "fix it" });
        assert_eq!(with_room(asked.clone(), Some(620.0)), serde_json::json!({ "laneId": "l1", "prompt": "fix it", "room": 620.0 }));
        // The shortcut asks for nothing in particular.
        assert_eq!(with_room(Value::Null, Some(620.0)), serde_json::json!({ "room": 620.0 }));
        // Off any screen there's no room to tell.
        assert_eq!(with_room(asked.clone(), None), asked);
    }

    #[test]
    fn starts_bring_their_app_forward() {
        assert_eq!(start_app(Place::Terminal, "ghostty"), Some("com.mitchellh.ghostty"));
        assert_eq!(start_app(Place::Terminal, "iterm"), None);
        assert_eq!(start_app(Place::ClaudeApp, "ghostty"), Some("com.anthropic.claudefordesktop"));
        assert_eq!(start_app(Place::CodexApp, "ghostty"), Some("com.openai.codex"));
    }
}
