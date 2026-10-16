//! Apps: the ones running, by bundle ID on macOS (whether one is active, asking it to come
//! forward, which is in front), Codealong's own (hiding it, its Dock icon, whether macOS opened
//! it), and an app's icon for a menu.

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::*;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::*;
