//! The outline round a terminal that came forward on another screen than the pointer's
//! (flash.rs): the screens and the pointer as the window server has them, the front window of an
//! app, the pointer moved, and the outline drawn and faded. All in CoreGraphics' global
//! coordinates (`flash::Frame`).

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::*;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::*;
