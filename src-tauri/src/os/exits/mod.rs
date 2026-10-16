//! Waiting on processes to exit: a thread waits until one of the processes it watches exits, it's
//! woken (`Waker`), or a time runs out, and is told which exited. The dev-server watcher waits so
//! between its looks (previews.rs), so a stopped server shows at once.

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::*;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::*;
