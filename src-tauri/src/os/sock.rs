//! The local sockets Codealong's server listens on, and its app and the `codealong` command
//! connect to (server.rs): AF_UNIX sockets, each a path in the folder of Codealong's files. std
//! has them on macOS and Linux; Windows has had them since Windows 10 (1803), through
//! uds_windows, with std's calls. What harnesses keep on their own sockets (Claude Code's, the
//! Codex app's) is connected to the same way, where they have one.

#[cfg(unix)]
pub use std::os::unix::net::{UnixListener, UnixStream};
#[cfg(windows)]
pub use uds_windows::{UnixListener, UnixStream};

/// Which socket a listener's descriptor is now, by its device and inode, which a copy of it
/// shares and no other socket or file has; None once the descriptor is closed (server.rs
/// `accept_on`).
#[cfg(unix)]
pub fn id(listener: &UnixListener) -> Option<(u64, u64)> {
    use std::os::fd::AsRawFd;
    // SAFETY: fstat only writes the struct it's given, and fails on a closed descriptor.
    let mut st: libc::stat = unsafe { std::mem::zeroed() };
    (unsafe { libc::fstat(listener.as_raw_fd(), &mut st) } == 0).then_some((st.st_dev as u64, st.st_ino as u64))
}

/// A socket's handle stays its own on Windows, where nothing closes one under another part of the
/// app: its number is its id.
#[cfg(windows)]
pub fn id(listener: &UnixListener) -> Option<(u64, u64)> {
    use std::os::windows::io::AsRawSocket;
    Some((0, listener.as_raw_socket()))
}
