//! Shows where a focused terminal went. With several screens, a click can bring a session's tab
//! forward on a screen you aren't looking at, so when the terminal's window comes up on another
//! screen than the pointer's, Codealong outlines it for a moment, and with `[apps] move_pointer`
//! moves the pointer to it. On the pointer's own screen nothing happens: you can see it there.
//!
//! Everything here is in CoreGraphics' global coordinates (points, origin at the top left of the
//! main screen, y growing down), the ones window bounds and the pointer come in; only the outline
//! window converts to AppKit's.

use tauri::AppHandle;

/// A rectangle in global display coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Frame {
    fn contains(&self, (x, y): (f64, f64)) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }

    fn overlap(&self, o: &Frame) -> f64 {
        let w = (self.x + self.w).min(o.x + o.w) - self.x.max(o.x);
        let h = (self.y + self.h).min(o.y + o.h) - self.y.max(o.y);
        w.max(0.0) * h.max(0.0)
    }

    pub fn center(&self) -> (f64, f64) {
        (self.x + self.w / 2.0, self.y + self.h / 2.0)
    }
}

/// The screen showing most of `window`.
fn screen_of(window: &Frame, screens: &[Frame]) -> Option<usize> {
    screens
        .iter()
        .enumerate()
        .map(|(i, s)| (i, s.overlap(window)))
        .filter(|(_, a)| *a > 0.0)
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(i, _)| i)
}

/// Whether a window that came forward is on another screen than the pointer was.
pub fn elsewhere(pointer: (f64, f64), window: &Frame, screens: &[Frame]) -> bool {
    let at = screens.iter().position(|s| s.contains(pointer));
    match (at, screen_of(window, screens)) {
        (Some(a), Some(b)) => a != b,
        _ => false,
    }
}

/// Where the pointer is now, to compare with where the terminal lands.
pub fn pointer() -> Option<(f64, f64)> {
    crate::os::outline::pointer()
}

/// After a click brought a session's terminal forward: waits for its window to come up, then
/// outlines it when it's on another screen than `from`, the pointer at the click. Runs on a
/// thread of its own; the click has already returned.
pub fn after_focus(app: &AppHandle, from: Option<(f64, f64)>, bundle_id: &'static str) {
    let Some(from) = from else { return };
    let ticket = crate::os::outline::ticket();
    let app = app.clone();
    std::thread::spawn(move || {
        let Some(window) = crate::os::outline::front_window_settled(bundle_id) else { return };
        if !crate::os::outline::is_newest(ticket) || !elsewhere(from, &window, &crate::os::outline::screens()) {
            return;
        }
        let move_pointer = crate::config::current().is_ok_and(|c| c.apps.move_pointer);
        if move_pointer {
            crate::os::outline::warp(window.center());
        }
        crate::os::outline::outline(&app, window, ticket);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(x: f64, y: f64, w: f64, h: f64) -> Frame {
        Frame { x, y, w, h }
    }

    #[test]
    fn flashes_only_when_the_window_lands_on_another_screen() {
        // A laptop screen with a bigger one to its right, top edges level.
        let screens = [f(0.0, 0.0, 1512.0, 982.0), f(1512.0, 0.0, 2560.0, 1440.0)];
        let on_left = f(100.0, 100.0, 800.0, 600.0);
        let on_right = f(2000.0, 200.0, 1200.0, 800.0);
        assert!(!elsewhere((500.0, 500.0), &on_left, &screens));
        assert!(elsewhere((500.0, 500.0), &on_right, &screens));
        assert!(elsewhere((3000.0, 900.0), &on_left, &screens));
        // Straddling the edge, it's on the screen that shows most of it.
        let straddling = f(1300.0, 100.0, 1000.0, 600.0);
        assert!(!elsewhere((2000.0, 500.0), &straddling, &screens));
        assert!(elsewhere((200.0, 500.0), &straddling, &screens));
    }

    #[test]
    fn stays_quiet_when_it_cant_tell() {
        let screens = [f(0.0, 0.0, 1512.0, 982.0)];
        assert!(!elsewhere((500.0, 500.0), &f(5000.0, 0.0, 100.0, 100.0), &screens));
        assert!(!elsewhere((500.0, 500.0), &f(0.0, 0.0, 100.0, 100.0), &[]));
    }
}

/// Reads this Mac's screens, the pointer and Ghostty's front window, and says whether a focus now
/// would outline it. Changes nothing: `cargo test ghosttys_front_window -- --ignored --nocapture`.
#[cfg(test)]
mod live {
    #[test]
    #[ignore]
    fn ghosttys_front_window() {
        let screens = crate::os::outline::screens();
        let pointer = super::pointer();
        let window = crate::os::outline::front_window_settled("com.mitchellh.ghostty");
        println!("screens {screens:?}\npointer {pointer:?}\nghostty {window:?}");
        assert!(!screens.is_empty());
        if let (Some(p), Some(w)) = (pointer, window) {
            println!("outline: {}", super::elsewhere(p, &w, &screens));
        }
    }
}
