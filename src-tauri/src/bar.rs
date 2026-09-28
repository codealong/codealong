//! The floating bar: a small window, on top of every other and on every Space, that shows the
//! agents that need you and the ones running, PRs to review and todos ready to start. It's up
//! unless you turn it off, from the toggle at the top right of the board, the bar's own menu, or
//! Window › Floating bar.
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
/// How often the bar checks that it's up (when on) and writes down where it was moved to.
const CHECK_EVERY: Duration = Duration::from_secs(1);
/// The bar's size before its page has measured itself.
const START: (f64, f64) = (240.0, 48.0);
/// Space between a newly placed bar and the menu bar.
const TOP_GAP: f64 = 6.0;
/// Room kept at the sides of the screen.
const EDGE: f64 = 8.0;

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
fn clamp(r: Rect, screen: Rect) -> Rect {
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
fn screen_for(r: Rect, screens: &[Screen]) -> Option<Rect> {
    screens.iter().find(|s| s.frame.contains(r.center())).or(screens.first()).map(|s| s.visible)
}

/// What `bar.json` keeps.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Saved {
    /// Turned off from the bar's menu or the Window menu.
    #[serde(default)]
    off: bool,
    /// Where the bar was last.
    #[serde(default)]
    frame: Option<Rect>,
}

impl Saved {
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
    /// Window › Floating bar.
    item: Mutex<Option<CheckMenuItem<Wry>>>,
}

/// Makes the bar, shows it unless it's turned off, and adds Window › Floating bar.
pub fn setup(app: &AppHandle) -> Result<()> {
    let saved = Saved::load();
    let (off, frame) = (saved.off, saved.frame);
    app.manage(Bar { saved: Mutex::new(saved), sent: Mutex::new(String::new()), moved: AtomicBool::new(false), item: Mutex::new(None) });

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
    native::make_panel(&window);
    native::prepare(&window);
    if let Some(r) = start_frame(frame, &native::screens()) {
        native::set_frame(&window, r);
    }
    if !off {
        native::show(&window);
    }

    let item = CheckMenuItem::with_id(app, MENU_ID, "Floating bar", true, !off, None::<&str>)?;
    if let Some(MenuItemKind::Submenu(menu)) = app.menu().and_then(|m| m.get(WINDOW_SUBMENU_ID)) {
        menu.append(&PredefinedMenuItem::separator(app)?)?;
        menu.append(&item)?;
    }
    if let Ok(mut slot) = app.state::<Bar>().item.lock() {
        *slot = Some(item);
    }
    app.on_menu_event(|app, event| {
        if event.id().as_ref() == MENU_ID {
            // The menu has already flipped the check mark.
            let on = app.state::<Bar>().item.lock().ok().and_then(|i| i.as_ref().and_then(|i| i.is_checked().ok())).unwrap_or(true);
            set_on(app, on);
        }
    });

    let handle = app.clone();
    std::thread::Builder::new().name("codealong-bar".into()).spawn(move || loop {
        std::thread::sleep(CHECK_EVERY);
        let app = handle.clone();
        if handle.run_on_main_thread(move || keep_up(&app)).is_err() {
            break;
        }
    })?;
    Ok(())
}

/// Puts the bar back up if it's on but not on screen, and writes down where it is after a move (a
/// drag moves it many times a second). On the main thread.
fn keep_up(app: &AppHandle) {
    let (Some(bar), Some(state)) = (app.get_webview_window(LABEL), app.try_state::<Bar>()) else {
        return;
    };
    let on = state.saved.lock().map(|s| !s.off).unwrap_or(false);
    if on && !native::on_screen(&bar) {
        native::show(&bar);
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
        let Some(screen) = screen_for(current, &native::screens()) else { return };
        let next = place(current, width.ceil(), height.ceil(), screen);
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

/// Whether the bar is on.
pub fn is_on(app: &AppHandle) -> bool {
    app.try_state::<Bar>().and_then(|s| s.saved.lock().ok().map(|s| !s.off)).unwrap_or(false)
}

/// Turns the bar on or off, and tells every window, so the toggle at the top right and Window ›
/// Floating bar stay in step with wherever it was changed.
pub fn set_on(app: &AppHandle, on: bool) {
    let Some(state) = app.try_state::<Bar>() else { return };
    if let Ok(mut saved) = state.saved.lock() {
        saved.off = !on;
        saved.save();
    }
    if let Some(item) = state.item.lock().ok().and_then(|i| i.clone()) {
        let _ = item.set_checked(on);
    }
    if let Some(bar) = app.get_webview_window(LABEL) {
        if on {
            native::show(&bar);
        } else {
            let _ = bar.hide();
        }
    }
    let _ = app.emit("floating-bar", on);
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

#[cfg(target_os = "macos")]
mod native {
    use std::sync::OnceLock;

    use objc2::runtime::{AnyClass, AnyObject, Bool, ClassBuilder, Sel};
    use objc2::{sel, ClassType, MainThreadMarker};
    use objc2_app_kit::{NSApplicationActivationOptions, NSPanel, NSRunningApplication, NSScreen, NSStatusWindowLevel, NSWindow, NSWindowCollectionBehavior, NSWindowOcclusionState, NSWindowStyleMask};
    use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};
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
            match ClassBuilder::new(c"CodealongBarPanel", NSPanel::class()) {
                Some(mut builder) => {
                    // SAFETY: the overrides match NSWindow's `- (BOOL)canBecome…Window` and
                    // `- (BOOL)isKeyWindow`.
                    unsafe {
                        builder.add_method(sel!(canBecomeKeyWindow), no as extern "C" fn(_, _) -> _);
                        builder.add_method(sel!(canBecomeMainWindow), no as extern "C" fn(_, _) -> _);
                        builder.add_method(sel!(isKeyWindow), yes as extern "C" fn(_, _) -> _);
                    }
                    builder.register()
                }
                None => AnyClass::get(c"CodealongBarPanel").unwrap_or(NSPanel::class()),
            }
        })
    }

    /// Turns the bar's window into a non-activating panel, the only kind macOS puts on another
    /// app's full-screen Space. Tauri makes windows, so the class is swapped after the fact. WebKit
    /// asks again whether the window is key each time it's ordered on screen, which `show` does
    /// after this, so the page counts as active from the bar's first showing.
    pub fn make_panel(w: &WebviewWindow) {
        let Some(n) = ns(w) else { return };
        let obj: &AnyObject = n;
        let panel = panel_class();
        // NSPanel keeps no storage of its own beyond NSWindow's, so the swap needs no more room
        // than tao's window already has; if that ever changes, the bar stays a plain window.
        if panel.instance_size() > obj.class().instance_size() {
            return;
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
}

/// Elsewhere the bar is an ordinary always-on-top window, left where it opens.
#[cfg(not(target_os = "macos"))]
mod native {
    use tauri::WebviewWindow;

    use super::{Rect, Screen};

    pub fn make_panel(_: &WebviewWindow) {}
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
        let s: Saved = serde_json::from_str(r#"{"off":true,"frame":{"x":1,"y":2,"w":3,"h":4}}"#).unwrap();
        assert!(s.off);
        assert_eq!(s.frame, Some(r(1.0, 2.0, 3.0, 4.0)));
    }
}
