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

/// The process at the other end of a connection, as the kernel says it: who asks, which it
/// can't make up (server.rs `AccountCheck`). None where the OS doesn't say.
#[cfg(target_os = "macos")]
pub fn peer_pid(stream: &UnixStream) -> Option<u32> {
    use std::os::fd::AsRawFd;
    let mut pid: libc::pid_t = 0;
    let mut len = std::mem::size_of::<libc::pid_t>() as libc::socklen_t;
    // SAFETY: getsockopt writes at most `len` bytes into the pid it's given.
    let ok = unsafe { libc::getsockopt(stream.as_raw_fd(), libc::SOL_LOCAL, libc::LOCAL_PEERPID, (&mut pid as *mut libc::pid_t).cast(), &mut len) } == 0;
    (ok && pid > 0).then_some(pid as u32)
}

#[cfg(target_os = "linux")]
pub fn peer_pid(stream: &UnixStream) -> Option<u32> {
    use std::os::fd::AsRawFd;
    let mut cred: libc::ucred = unsafe { std::mem::zeroed() };
    let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    // SAFETY: getsockopt writes at most `len` bytes into the struct it's given.
    let ok = unsafe { libc::getsockopt(stream.as_raw_fd(), libc::SOL_SOCKET, libc::SO_PEERCRED, (&mut cred as *mut libc::ucred).cast(), &mut len) } == 0;
    (ok && cred.pid > 0).then_some(cred.pid as u32)
}

/// Windows's AF_UNIX sockets don't say who's at the other end: the asker's own word stands.
#[cfg(windows)]
pub fn peer_pid(_stream: &UnixStream) -> Option<u32> {
    None
}
