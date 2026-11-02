//! The windows: the panels that float over other apps (panel.rs has what kind each is and when
//! each step happens, and re-exports these), and what's asked of any window's page beyond what
//! Tauri gives. macOS's are AppKit and WebKit through objc2, `macos.rs`; Linux's are Tauri and
//! GTK, `linux.rs`, where much of it has nothing to do.

use std::time::Duration;

use super::Os;
use crate::geometry::Rect;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::*;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::*;

/// Whether the desktop shows the items apps put in its menu bar or tray (menubar.rs). A Mac's
/// menu bar always does. On Linux an item is a StatusNotifierItem, which only shows while
/// something on the session bus takes them (KDE's panel, GNOME's with its AppIndicator
/// extension, most others), asked of D-Bus with `gdbus`, which GLib has wherever GTK is. A
/// desktop with only an old XEmbed tray is taken for one without.
pub fn tray_shown() -> bool {
    match super::current() {
        Os::MacOs => true,
        Os::Linux => {
            let asked = ["call", "--session", "--dest", "org.freedesktop.DBus", "--object-path", "/org/freedesktop/DBus", "--method", "org.freedesktop.DBus.NameHasOwner", "org.kde.StatusNotifierWatcher"];
            crate::procs::run("gdbus", &asked, None, Duration::from_secs(1)).is_ok_and(|answer| has_owner(&answer))
        }
    }
}

/// `gdbus`'s answer to NameHasOwner: `(true,)` or `(false,)`.
fn has_owner(answer: &str) -> bool {
    answer.trim() == "(true,)"
}

/// What the OS and its webview's drawing side say of a page, as the pointer probe compares them
/// (pointer.rs): the page's box on the screen, where the pointer is, the drawing side's scrolling
/// tree as text, the window's frame, and whether Codealong is active and the window key.
pub struct PageView {
    pub host: Rect,
    pub pointer: (f64, f64),
    pub scrolling_tree: Option<String>,
    pub window: Rect,
    pub active: bool,
    pub key: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tray_is_there_when_its_watcher_has_an_owner() {
        assert!(has_owner("(true,)\n"));
        assert!(!has_owner("(false,)\n"));
        assert!(!has_owner(""));
        let _mac = crate::os::AsIf::new(Os::MacOs);
        assert!(tray_shown());
    }
}
