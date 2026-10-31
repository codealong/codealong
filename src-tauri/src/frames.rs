//! Where an ordinary window was, for the next launch: the main window's frame in `window.json` and
//! the waiting window's in `waiting.json`, next to the config.
//!
//! A frame is noted as its window moves and sizes, and written within moments of that, not as
//! Codealong quits: only a plain Quit gets as far as `RunEvent::Exit`. A copy taking over ends the
//! process itself (`server::Request::Quit`), relaunch.sh sends SIGTERM, and `AppHandle::restart`
//! on the main thread, as an update's restart is, skips the exit events.
//!
//! What's kept is a frame: a window in full screen isn't noted, so it comes back at the frame it
//! had before it went full screen, not in full screen.
//!
//! Only macOS puts the main window back. On Linux a window's border isn't known until it's up
//! (os/windows/linux.rs `border`), so the first window up would grow by its title bar at each
//! launch, and a window manager places a window as it's shown (.claude/rules/linux.md).

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, WebviewWindow, Window, WindowEvent};

use crate::config::support_dir;
use crate::geometry::{clamp, screen_at, screen_holding, Rect, Screen};
use crate::panel;

/// How long after a move a frame is written, so a drag is a write or two rather than one a step.
const SETTLE: Duration = Duration::from_millis(500);

#[derive(Default, Serialize, Deserialize)]
struct Saved {
    /// Where it was last, in AppKit's points.
    #[serde(default)]
    frame: Option<Rect>,
}

/// Where one window was and is.
pub struct Place {
    path: PathBuf,
    frame: Mutex<Option<Rect>>,
    /// Whether a write is waiting out `SETTLE`.
    writing: AtomicBool,
}

impl Place {
    pub fn load(file: &str) -> Arc<Place> {
        Place::at(support_dir().join(file))
    }

    /// Read leniently: the file is only where the window was, which the next drag sets again, so
    /// one that doesn't read puts the window in its first place and is saved over.
    fn at(path: PathBuf) -> Arc<Place> {
        let frame = crate::store::read_or_default::<Saved>(&path).frame;
        Arc::new(Place { path, frame: Mutex::new(frame), writing: AtomicBool::new(false) })
    }

    /// Puts a window that hasn't shown yet where it was. On the main thread.
    pub fn restore(&self, w: &WebviewWindow) {
        let saved = self.frame.lock().ok().and_then(|f| *f);
        let screens = panel::screens();
        let pointer = panel::pointer().and_then(|p| screen_at(p, &screens));
        if let Some(frame) = panel::frame(w).and_then(|now| start_frame(saved, now, &screens, pointer)) {
            panel::set_frame(w, frame);
        }
    }

    /// Notes where the window is, as it moves or sizes. On the main thread.
    pub fn note(self: &Arc<Self>, w: &WebviewWindow) {
        // Hidden or minimized, it reports a frame that isn't where it shows, and in full screen
        // one it won't come back to.
        if !w.is_visible().unwrap_or(false) || w.is_minimized().unwrap_or(false) || w.is_fullscreen().unwrap_or(false) {
            return;
        }
        if let Some(frame) = panel::frame(w) {
            self.noted(frame);
        }
    }

    fn noted(self: &Arc<Self>, frame: Rect) {
        let Ok(mut slot) = self.frame.lock() else { return };
        if slot.replace(frame) == Some(frame) {
            return;
        }
        drop(slot);
        if !self.writing.swap(true, Ordering::AcqRel) {
            let place = self.clone();
            std::thread::spawn(move || {
                std::thread::sleep(SETTLE);
                // Before the write, so a frame noted during it gets one of its own.
                place.writing.store(false, Ordering::Release);
                place.save();
            });
        }
    }

    /// Writes where the window is, for the next launch.
    pub fn save(&self) {
        let frame = self.frame.lock().ok().and_then(|f| *f);
        if frame.is_some() {
            let _ = crate::store::save(&self.path, &Saved { frame });
        }
    }
}

/// Where a window opens: where it was, moved onto the screen that holds most of it if a screen it
/// was on is gone, else at its size centered on the screen with the pointer, a little above the
/// middle. None with no screens.
fn start_frame(saved: Option<Rect>, now: Rect, screens: &[Screen], pointer: Option<Screen>) -> Option<Rect> {
    if let Some(s) = saved {
        if let Some(screen) = screen_holding(s.center(), screens) {
            // One that fits goes back exactly: `clamp` keeps a margin, which would take a window
            // tiled against an edge of the screen off it.
            return Some(if screen.visible.holds(s) { s } else { clamp(s, screen.visible) });
        }
    }
    let screen = pointer.or(screens.first().copied())?.visible;
    let (w, h) = saved.map_or((now.w, now.h), |s| (s.w, s.h));
    let x = screen.x + (screen.w - w) / 2.0;
    // A third of what's left over goes above it, two thirds below, as macOS centers a window.
    let y = screen.y + (screen.h - h) * 2.0 / 3.0;
    Some(clamp(Rect { x: x.round(), y: y.round(), w, h }, screen))
}

/// The main window's place.
struct Main(Arc<Place>);

/// Puts the main window where it was and shows it: tauri.conf.json makes it hidden, so it's never
/// seen in its first place. On the main thread.
pub fn setup(app: &AppHandle) {
    if crate::os::is_macos() {
        let place = Place::load("window.json");
        if let Some(w) = app.get_webview_window("main") {
            place.restore(&w);
        }
        app.manage(Main(place));
    }
    if let Err(e) = crate::front::show_main(app, None) {
        eprintln!("Couldn't show the main window: {e:#}");
    }
}

/// Window events for the main window: where it is is kept as it moves. On the main thread.
pub fn on_window_event(window: &Window, event: &WindowEvent) {
    if window.label() != "main" || !matches!(event, WindowEvent::Moved(_) | WindowEvent::Resized(_)) {
        return;
    }
    let app = window.app_handle();
    if let (Some(state), Some(w)) = (app.try_state::<Main>(), app.get_webview_window("main")) {
        state.0.note(&w);
    }
}

/// Writes where the main window is, for a move too recent to have been written.
pub fn save(app: &AppHandle) {
    if let Some(state) = app.try_state::<Main>() {
        state.0.save();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAIN: Screen = Screen { frame: Rect { x: 0.0, y: 0.0, w: 1512.0, h: 982.0 }, visible: Rect { x: 0.0, y: 0.0, w: 1512.0, h: 945.0 } };
    const LEFT: Screen = Screen { frame: Rect { x: -1920.0, y: 0.0, w: 1920.0, h: 1080.0 }, visible: Rect { x: -1920.0, y: 0.0, w: 1920.0, h: 1055.0 } };
    const NOW: Rect = Rect { x: 0.0, y: 0.0, w: 820.0, h: 808.0 };

    #[test]
    fn opens_where_it_was() {
        let was = Rect { x: -1500.0, y: 100.0, w: 900.0, h: 700.0 };
        assert_eq!(start_frame(Some(was), NOW, &[MAIN, LEFT], Some(MAIN)), Some(was));
        // Partly off its screen, it's moved back on.
        let hanging = Rect { x: 1000.0, y: 100.0, w: 900.0, h: 700.0 };
        assert_eq!(start_frame(Some(hanging), NOW, &[MAIN], Some(MAIN)).map(|r| r.right()), Some(1512.0 - crate::geometry::EDGE));
    }

    #[test]
    fn a_window_tiled_against_the_edges_stays_against_them() {
        let half = Rect { x: 0.0, y: 0.0, w: 756.0, h: 945.0 };
        assert_eq!(start_frame(Some(half), NOW, &[MAIN], Some(MAIN)), Some(half));
        let right = Rect { x: 756.0, ..half };
        assert_eq!(start_frame(Some(right), NOW, &[MAIN], Some(MAIN)), Some(right));
        assert_eq!(start_frame(Some(MAIN.visible), NOW, &[MAIN], Some(MAIN)), Some(MAIN.visible));
    }

    #[test]
    fn opens_centered_where_the_pointer_is_the_first_time_and_when_its_screen_is_gone() {
        let first = start_frame(None, NOW, &[MAIN, LEFT], Some(LEFT)).unwrap();
        assert_eq!((first.w, first.h), (820.0, 808.0));
        assert!(LEFT.visible.contains(first.center()), "{first:?}");
        assert_eq!(first.x, -1920.0 + 550.0);
        // A little above the middle.
        assert!(first.center().1 > LEFT.visible.center().1, "{first:?}");
        // Its screen unplugged: at the size it had, on the pointer's.
        let gone = Rect { x: 3000.0, y: 100.0, w: 900.0, h: 700.0 };
        let moved = start_frame(Some(gone), NOW, &[MAIN], Some(MAIN)).unwrap();
        assert_eq!((moved.w, moved.h), (900.0, 700.0));
        assert!(MAIN.visible.contains(moved.center()), "{moved:?}");
        assert_eq!(start_frame(None, NOW, &[], None), None);
    }

    #[test]
    fn a_noted_frame_reaches_the_file_without_a_quit() {
        let path = crate::store::test_dir("frames-noted").join("window.json");
        let place = Place::at(path.clone());
        let last = Rect { x: 40.0, y: 50.0, w: 1100.0, h: 760.0 };
        for x in [10.0, 20.0, 30.0] {
            place.noted(Rect { x, ..last });
        }
        place.noted(last);
        assert!(!path.exists(), "a drag waits for the window to stop");
        let until = std::time::Instant::now() + 20 * SETTLE;
        while !path.exists() && std::time::Instant::now() < until {
            std::thread::sleep(SETTLE / 10);
        }
        assert_eq!(*Place::at(path).frame.lock().unwrap(), Some(last), "the next launch reads where it stopped");
    }

    #[test]
    fn a_frame_it_already_has_writes_nothing() {
        let frame = Rect { x: 40.0, y: 50.0, w: 1100.0, h: 760.0 };
        let path = crate::store::test_dir("frames-same").join("window.json");
        crate::store::save(&path, &Saved { frame: Some(frame) }).unwrap();
        let place = Place::at(path);
        place.noted(frame);
        assert!(!place.writing.load(Ordering::Acquire));
    }

    #[test]
    fn a_file_that_does_not_read_is_a_first_launch() {
        let path = crate::store::test_dir("frames-unread").join("window.json");
        std::fs::write(&path, "not json").unwrap();
        assert_eq!(*Place::at(path).frame.lock().unwrap(), None);
    }
}
