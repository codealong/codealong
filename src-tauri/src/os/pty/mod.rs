//! A pseudo-terminal of Codealong's own, with a process on it (terminals.rs): the process leads a
//! session of its own on the terminal's other side, as a terminal app starts its shell, so the
//! shell runs jobs, gets ^C as an interrupt and is hung up on when the terminal goes. And how a
//! server hands its terminals' sides to the build it becomes in place (serve.rs `hand_over`):
//! through a socket kept across the exec, which holds them until the new build reads it.
//!
//! POSIX's, the same on macOS and Linux, is `posix.rs`. Windows has pseudo-consoles (ConPTY)
//! instead, which Codealong doesn't drive yet: `windows.rs` gives the same names, opening a
//! terminal there fails saying so, and a terminal's side (`Master`) and the hand-over's socket are
//! types nothing can make, so what acts on one is never reached.

#[cfg(unix)]
mod posix;
#[cfg(unix)]
pub use posix::*;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::*;

use std::path::Path;
use std::process::Child;

/// A terminal's size: in cells, and the cell's size in pixels, which programs that draw pictures
/// ask for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Size {
    pub cols: u16,
    pub rows: u16,
    pub cell_width: u16,
    pub cell_height: u16,
}

/// What runs on the terminal, and where. Windows' side starts nothing yet, so reads none of it.
#[cfg_attr(windows, allow(dead_code))]
pub struct Spawn<'a> {
    pub program: &'a Path,
    /// The name the program sees itself run by: a shell whose name starts with `-` is a login
    /// shell, as a terminal app's is.
    pub arg0: &'a str,
    pub args: &'a [String],
    pub cwd: &'a Path,
    /// Its whole environment.
    pub env: &'a [(String, String)],
}

/// The terminal's side Codealong keeps (`master`), and the process on the other (`Pty::open`).
pub struct Pty {
    pub master: Master,
    pub child: Child,
    /// The other side's device (`/dev/ttys012`), which the process and everything it starts have
    /// as their terminal: how a session's process is known to run here (launch/codealong.rs).
    pub tty: String,
}

impl Pty {
    pub fn pid(&self) -> u32 {
        self.child.id()
    }
}
