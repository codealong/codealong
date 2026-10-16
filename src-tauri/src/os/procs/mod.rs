//! The kernel's process table, read without starting `ps`: a process's start, parent, name,
//! terminal, folder, environment, open files and listening ports. procs.rs builds on these and
//! re-exports them; macOS's are `macos.rs`, Linux's (`/proc`) `linux.rs`.

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::*;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::*;

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
