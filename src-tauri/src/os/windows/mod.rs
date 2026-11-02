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
/// menu bar always does. On Linux an item is a StatusNotifierItem, shown by a host (KDE's panel,
/// GNOME's with its AppIndicator extension, most others) that has told the session bus's
/// StatusNotifierWatcher it's there; a watcher can run with no host, which shows nothing. Asked of
/// the watcher with `gdbus`, which GLib has wherever GTK is, so it waits on a program: off the
/// main thread. A desktop with only an old XEmbed tray is taken for one without.
pub fn tray_shown() -> bool {
    match super::current() {
        Os::MacOs => true,
        Os::Linux => {
            let asked = [
                "call",
                "--session",
                "--dest",
                "org.kde.StatusNotifierWatcher",
                "--object-path",
                "/StatusNotifierWatcher",
                "--method",
                "org.freedesktop.DBus.Properties.Get",
                "org.kde.StatusNotifierWatcher",
                "IsStatusNotifierHostRegistered",
            ];
            crate::procs::run("gdbus", &asked, None, Duration::from_secs(2)).is_ok_and(|answer| host_registered(&answer))
        }
    }
}

/// `gdbus`'s answer for the watcher's IsStatusNotifierHostRegistered: `(<true>,)` or
/// `(<false>,)`. With no watcher the call fails, and there's no answer to read.
fn host_registered(answer: &str) -> bool {
    answer.trim() == "(<true>,)"
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
    fn a_tray_is_there_when_a_host_is_registered_with_its_watcher() {
        assert!(host_registered("(<true>,)\n"));
        assert!(!host_registered("(<false>,)\n"));
        assert!(!host_registered(""));
        let _mac = crate::os::AsIf::new(Os::MacOs);
        assert!(tray_shown());
    }
}
