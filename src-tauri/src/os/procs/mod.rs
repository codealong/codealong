//! The kernel's process table, read without starting `ps`: a process's start, parent, name,
//! terminal, folder, environment, open files and listening ports; and what's asked of a process
//! (signals), of a program as it's started, and of its pipes. procs.rs builds on these and
//! re-exports them; macOS's are `macos.rs`, Linux's (`/proc`) `linux.rs`, Windows' `windows.rs`,
//! and what macOS and Linux do alike through POSIX `posix.rs`.

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::*;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::*;
#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use windows::*;
#[cfg(unix)]
mod posix;
#[cfg(unix)]
pub use posix::*;

/// What a process is told (`signal`): POSIX's signals. Windows has none of them; there, those that
/// end a process end it, Continue does nothing (nothing is stopped there), and an interrupt
/// can't be sent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Signal {
    /// SIGHUP: its terminal went.
    HangUp,
    /// SIGINT: Ctrl-C.
    Interrupt,
    /// SIGTERM: asked to quit.
    Terminate,
    /// SIGKILL.
    Kill,
    /// SIGCONT.
    Continue,
}

/// A process as one look at the whole table finds it: its parent, the CPU it has used, when it
/// started, and its command line, its arguments joined with spaces (`table`).
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub pid: u32,
    pub ppid: u32,
    pub cpu_ms: Option<u64>,
    pub started_ms: Option<i64>,
    pub command: String,
}

/// A descriptor passed on through `exec_keeping`: POSIX's number for an open file. Windows has
/// handles instead, and never passes one on this way.
#[cfg(unix)]
pub type Fd = std::os::fd::RawFd;
#[cfg(windows)]
pub type Fd = i32;

/// How many descriptors a process holds, of each kind (`open_kinds`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Kinds {
    pub files: usize,
    pub sockets: usize,
    pub pipes: usize,
    /// kqueues, epolls, eventfds and the rest.
    pub other: usize,
}

/// A process and every one under it, as one look found them (`tree`): which they are and the CPU
/// they've used between them. Two looks a moment apart tell a command at rest, waiting on
/// something (you, the network, a timer), from one at work (terminals.rs `Asking`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tree {
    /// The process first, then those under it, nearest first.
    pub pids: Vec<u32>,
    /// Nanoseconds of CPU, their own and the kernel's for them, of the processes there now.
    pub cpu_ns: u64,
}

impl Tree {
    /// The most CPU a tree at rest uses between two looks a second apart: a runtime's timers
    /// (Go's, a Node program's) wake now and then, which comes to a few milliseconds.
    pub const AT_REST_NS: u64 = 20_000_000;

    /// Whether nothing happened between `before` and this look: no process started or ended, and
    /// together they used next to no CPU.
    pub fn rested_since(&self, before: &Tree) -> bool {
        self.pids == before.pids && self.cpu_ns.saturating_sub(before.cpu_ns) < Tree::AT_REST_NS
    }
}

/// The most processes a tree is followed to: a command's are a handful.
const TREE_MOST: usize = 256;

/// A process as `ps` lists it: its parent, short name, terminal device and the process group in
/// the foreground of that terminal.
pub(crate) struct Listed {
    pub ppid: u32,
    pub name: String,
    pub tdev: u32,
    pub tpgid: i32,
}

/// A TCP port one of your processes listens on.
#[derive(Clone, Debug, PartialEq)]
pub struct Listener {
    pub pid: u32,
    pub port: u16,
    /// Where to reach it on this Mac: the address it's bound to, or loopback when it listens on
    /// every address.
    pub addr: std::net::IpAddr,
    /// The executable's short name (`node`), as `ps -c` has it.
    pub name: String,
    /// The folder it runs in.
    pub cwd: std::path::PathBuf,
    pub started_ms: i64,
}
