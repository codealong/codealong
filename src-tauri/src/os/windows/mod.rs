//! The windows: the panels that float over other apps (panel.rs has what kind each is and when
//! each step happens, and re-exports these), and what's asked of any window's page beyond what
//! Tauri gives. macOS's are AppKit and WebKit through objc2, `macos.rs`; Linux's are Tauri and
//! GTK, `linux.rs`, where much of it has nothing to do.

use crate::geometry::Rect;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::*;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::*;

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
