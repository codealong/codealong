//! A pseudo-terminal of Codealong's own, with a process on it: the process leads a session of
//! its own on the terminal's other side (`setsid`, then the terminal made its controlling one),
//! as a terminal app starts its shell, so the shell runs jobs, gets ^C as SIGINT and is hung up
//! on when the terminal goes. POSIX alone, the same on macOS and Linux.

use std::ffi::OsStr;
use std::fs::File;
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command, Stdio};

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

impl Size {
    fn winsize(self) -> libc::winsize {
        libc::winsize {
            ws_row: self.rows,
            ws_col: self.cols,
            ws_xpixel: self.cols.saturating_mul(self.cell_width),
            ws_ypixel: self.rows.saturating_mul(self.cell_height),
        }
    }
}

/// What runs on the terminal, and where.
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

/// The terminal's side Codealong keeps (`master`), and the process on the other.
pub struct Pty {
    pub master: OwnedFd,
    pub child: Child,
    /// The other side's device (`/dev/ttys012`), which the process and everything it starts have
    /// as their terminal: how a session's process is known to run here (launch/codealong.rs).
    pub tty: String,
}

impl Pty {
    /// Opens a terminal of `size` and starts `spawn` on it.
    pub fn open(spawn: &Spawn<'_>, size: Size) -> io::Result<Pty> {
        let (mut master, mut slave): (RawFd, RawFd) = (-1, -1);
        let mut ws = size.winsize();
        // SAFETY: openpty fills the two descriptors; the name and settings may be null, the size
        // is ours.
        if unsafe { libc::openpty(&mut master, &mut slave, std::ptr::null_mut(), std::ptr::null_mut(), &mut ws) } != 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: openpty gave both, and nothing else owns them.
        let (master, slave) = unsafe { (OwnedFd::from_raw_fd(master), OwnedFd::from_raw_fd(slave)) };
        // Neither is for the other processes Codealong starts.
        cloexec(master.as_raw_fd())?;
        cloexec(slave.as_raw_fd())?;
        let stdio = |fd: &OwnedFd| fd.try_clone().map(|fd| Stdio::from(File::from(fd)));
        let mut cmd = Command::new(spawn.program);
        cmd.arg0(OsStr::new(spawn.arg0)).args(spawn.args).current_dir(spawn.cwd).env_clear().envs(spawn.env.iter().map(|(k, v)| (k, v))).stdin(stdio(&slave)?).stdout(stdio(&slave)?).stderr(stdio(&slave)?);
        // SAFETY: only async-signal-safe calls between fork and exec. Its stdin is the terminal's
        // other side by now, which becomes the controlling terminal of the session it leads.
        unsafe {
            cmd.pre_exec(|| {
                if libc::setsid() < 0 {
                    return Err(io::Error::last_os_error());
                }
                if libc::ioctl(0, libc::TIOCSCTTY as _, 0) < 0 {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let tty = tty_name(slave.as_raw_fd());
        let child = cmd.spawn()?;
        // The process has its own copies; this one would keep the terminal open after it ends.
        drop(slave);
        Ok(Pty { master, child, tty })
    }

    pub fn pid(&self) -> u32 {
        self.child.id()
    }
}

/// The device a terminal's side is, by its descriptor; empty where the system won't say.
fn tty_name(fd: RawFd) -> String {
    let mut buf = [0 as libc::c_char; 128];
    // SAFETY: ttyname_r writes a NUL-ended name into the buffer we pass, of the length we pass.
    if unsafe { libc::ttyname_r(fd, buf.as_mut_ptr(), buf.len()) } != 0 {
        return String::new();
    }
    // SAFETY: on success the buffer holds a NUL-ended string.
    unsafe { std::ffi::CStr::from_ptr(buf.as_ptr()) }.to_string_lossy().into_owned()
}

/// Tells the terminal its new size, which signals the program in front (SIGWINCH).
pub fn resize(master: RawFd, size: Size) {
    let ws = size.winsize();
    // SAFETY: TIOCSWINSZ reads the size we pass; a closed descriptor only fails.
    unsafe {
        libc::ioctl(master, libc::TIOCSWINSZ as _, &ws);
    }
}

/// The process group in front on the terminal: the shell's own at its prompt, a program's while
/// it runs one. None when the terminal is gone.
pub fn foreground(master: RawFd) -> Option<u32> {
    // SAFETY: tcgetpgrp only reads the terminal's state.
    let group = unsafe { libc::tcgetpgrp(master) };
    u32::try_from(group).ok().filter(|g| *g > 0)
}

/// Hangs up on the session the process leads, as closing a terminal app's tab does, and with
/// `kill` ends it outright.
pub fn hang_up(pid: u32, kill: bool) {
    let Ok(group) = libc::pid_t::try_from(pid) else { return };
    // SAFETY: signalling a process group we started; one that's gone only fails.
    unsafe {
        libc::killpg(group, if kill { libc::SIGKILL } else { libc::SIGHUP });
        libc::kill(group, libc::SIGCONT);
    }
}

/// A copy of `fd` for a server handing its terminals to its next build (terminals.rs
/// `hand_over`), closed on exec like every other descriptor: it goes across through a socket
/// (`send_fds`), never inherited, so a process the server starts meanwhile never gets one. None
/// when there's none to be had.
pub fn inheritable(fd: RawFd) -> Option<RawFd> {
    // SAFETY: F_DUPFD_CLOEXEC makes a new descriptor for one we own.
    let copy = unsafe { libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 0) };
    (copy >= 0).then_some(copy)
}

/// Has the program in front draw its screen again, as a terminal resized does: for one whose
/// screen couldn't be carried over to a new build (terminals.rs `take_over`).
pub fn redraw(master: RawFd) {
    if let Some(group) = foreground(master) {
        // SAFETY: signalling the process group in front on our terminal.
        unsafe { libc::killpg(group as libc::pid_t, libc::SIGWINCH) };
    }
}

pub(crate) fn cloexec(fd: RawFd) -> io::Result<()> {
    // SAFETY: setting a flag on a descriptor we own.
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
    if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFD, flags | libc::FD_CLOEXEC) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Asks for `bytes` of room in a socket's send and receive buffers; the system may give less.
pub fn socket_room(sock: RawFd, bytes: usize) {
    let size = bytes as libc::c_int;
    for opt in [libc::SO_SNDBUF, libc::SO_RCVBUF] {
        // SAFETY: setsockopt reads the int we pass for a socket we own.
        unsafe { libc::setsockopt(sock, libc::SOL_SOCKET, opt, (&size as *const libc::c_int).cast(), std::mem::size_of::<libc::c_int>() as libc::socklen_t) };
    }
}

/// Sends descriptors over a Unix datagram socket (SCM_RIGHTS), with `payload` as the message's
/// bytes: the socket holds them for whoever reads it (`recv_fds`), who gets descriptors of their
/// own for the same open files, even once the sender has closed its own and become another
/// program (serve.rs `hand_over`).
pub fn send_fds(sock: RawFd, fds: &[RawFd], payload: &[u8]) -> io::Result<()> {
    let bytes = std::mem::size_of_val(fds) as libc::c_uint;
    // SAFETY: the message, its one buffer and its control space are ours and outlive the call;
    // the control header is written where CMSG_FIRSTHDR says, its data where CMSG_DATA says.
    unsafe {
        let space = libc::CMSG_SPACE(bytes) as usize;
        let mut control = vec![0u8; space];
        let mut iov = libc::iovec { iov_base: payload.as_ptr().cast_mut().cast(), iov_len: payload.len() };
        let mut msg: libc::msghdr = std::mem::zeroed();
        msg.msg_iov = &mut iov;
        msg.msg_iovlen = 1;
        msg.msg_control = control.as_mut_ptr().cast();
        msg.msg_controllen = space as _;
        let cmsg = libc::CMSG_FIRSTHDR(&msg);
        (*cmsg).cmsg_level = libc::SOL_SOCKET;
        (*cmsg).cmsg_type = libc::SCM_RIGHTS;
        (*cmsg).cmsg_len = libc::CMSG_LEN(bytes) as _;
        std::ptr::copy_nonoverlapping(fds.as_ptr(), libc::CMSG_DATA(cmsg).cast::<libc::c_int>(), fds.len());
        if libc::sendmsg(sock, &msg, 0) < 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

/// The next message `send_fds` sent: its descriptors, each closed on exec, and its bytes. None
/// once there's none waiting (it doesn't wait).
pub fn recv_fds(sock: RawFd, most: usize) -> io::Result<Option<(Vec<RawFd>, Vec<u8>)>> {
    let mut payload = vec![0u8; 64 * 1024];
    // SAFETY: as `send_fds`, reading instead; the descriptors are read from the control data the
    // kernel wrote, as many as its length holds.
    unsafe {
        let space = libc::CMSG_SPACE((most * std::mem::size_of::<libc::c_int>()) as libc::c_uint) as usize;
        let mut control = vec![0u8; space];
        let mut iov = libc::iovec { iov_base: payload.as_mut_ptr().cast(), iov_len: payload.len() };
        let mut msg: libc::msghdr = std::mem::zeroed();
        msg.msg_iov = &mut iov;
        msg.msg_iovlen = 1;
        msg.msg_control = control.as_mut_ptr().cast();
        msg.msg_controllen = space as _;
        let n = libc::recvmsg(sock, &mut msg, libc::MSG_DONTWAIT);
        if n < 0 {
            let e = io::Error::last_os_error();
            // Empty, or empty with the sender gone (macOS says the connection was reset then,
            // having given everything queued first).
            return if matches!(e.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::ConnectionReset) { Ok(None) } else { Err(e) };
        }
        let mut fds = Vec::new();
        let cmsg = libc::CMSG_FIRSTHDR(&msg);
        if !cmsg.is_null() && (*cmsg).cmsg_level == libc::SOL_SOCKET && (*cmsg).cmsg_type == libc::SCM_RIGHTS {
            let data = libc::CMSG_DATA(cmsg) as usize - cmsg as usize;
            let count = ((*cmsg).cmsg_len as usize).saturating_sub(data) / std::mem::size_of::<libc::c_int>();
            for i in 0..count {
                let fd = std::ptr::read_unaligned(libc::CMSG_DATA(cmsg).cast::<libc::c_int>().add(i));
                let _ = cloexec(fd);
                fds.push(fd);
            }
        }
        payload.truncate(n as usize);
        Ok(Some((fds, payload)))
    }
}

/// Waits up to `ms` for `fd` to take a write.
pub fn writable(fd: RawFd, ms: i32) -> bool {
    let mut p = libc::pollfd { fd, events: libc::POLLOUT, revents: 0 };
    // SAFETY: poll reads and writes the one pollfd we give it.
    let n = unsafe { libc::poll(&mut p, 1, ms) };
    n > 0
}

/// Makes reads and writes on `fd` return rather than wait, for every copy of it: a terminal's
/// writer then never sits in a write it can't be stopped in (terminals.rs `write`).
pub fn nonblocking(fd: RawFd) -> io::Result<()> {
    // SAFETY: setting a flag on a descriptor we own.
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Waits up to `ms` for `fd` to have something to read. False on timeout, true when it's readable
/// or hung up (a read then says which).
pub fn readable(fd: RawFd, ms: i32) -> bool {
    let mut p = libc::pollfd { fd, events: libc::POLLIN, revents: 0 };
    // SAFETY: poll reads and writes the one pollfd we give it.
    let n = unsafe { libc::poll(&mut p, 1, ms) };
    n > 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use std::time::{Duration, Instant};

    fn env() -> Vec<(String, String)> {
        vec![("PATH".into(), "/usr/bin:/bin".into()), ("TERM".into(), "xterm-256color".into())]
    }

    fn read_until(master: &OwnedFd, want: &str, wait: Duration) -> String {
        let mut file = File::from(master.try_clone().unwrap());
        let started = Instant::now();
        let mut got = Vec::new();
        let mut buf = [0u8; 4096];
        while started.elapsed() < wait && !String::from_utf8_lossy(&got).contains(want) {
            if !readable(master.as_raw_fd(), 100) {
                continue;
            }
            match file.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => got.extend_from_slice(&buf[..n]),
            }
        }
        String::from_utf8_lossy(&got).into_owned()
    }

    /// The program leads its own session on the terminal, which is its controlling terminal at
    /// the size asked for, and in front.
    #[test]
    fn starts_a_program_leading_a_session_on_a_terminal_of_the_size_asked() {
        let size = Size { cols: 91, rows: 17, cell_width: 8, cell_height: 16 };
        let args = vec!["-c".to_string(), "stty size; tty; sleep 1".to_string()];
        let env = env();
        let mut pty = Pty::open(&Spawn { program: Path::new("/bin/sh"), arg0: "sh", args: &args, cwd: Path::new("/"), env: &env }, size).expect("a terminal");
        let out = read_until(&pty.master, "/dev/", Duration::from_secs(5));
        assert!(out.contains("17 91"), "stty sees the size: {out:?}");
        assert!(out.contains("/dev/"), "it has a terminal of its own: {out:?}");
        assert!(pty.tty.starts_with("/dev/") && out.contains(&pty.tty), "the terminal's device is known: {:?} in {out:?}", pty.tty);
        assert_eq!(foreground(pty.master.as_raw_fd()), Some(pty.pid()), "the shell's group is in front");
        hang_up(pty.pid(), false);
        let status = pty.child.wait().unwrap();
        assert!(!status.success(), "hung up on");
    }

    /// Descriptors sent through a socket reach whoever reads it, closed on exec there, with the
    /// message's bytes, after the sender has closed its own; reading an empty socket doesn't wait.
    #[test]
    fn descriptors_go_through_a_socket_and_outlive_the_senders_copies() {
        use std::os::fd::{AsRawFd, FromRawFd};
        let (keep, give) = std::os::unix::net::UnixDatagram::pair().unwrap();
        let (mut a_read, a_write) = std::io::pipe().unwrap();
        let (mut b_read, b_write) = std::io::pipe().unwrap();
        send_fds(give.as_raw_fd(), &[a_write.as_raw_fd(), b_write.as_raw_fd()], b"term-a\nterm-b").unwrap();
        drop((a_write, b_write, give));
        let (fds, ids) = recv_fds(keep.as_raw_fd(), 16).unwrap().expect("a message");
        assert_eq!(ids, b"term-a\nterm-b");
        assert_eq!(fds.len(), 2);
        // SAFETY: a descriptor F_GETFD only reads.
        assert!(fds.iter().all(|fd| unsafe { libc::fcntl(*fd, libc::F_GETFD) } & libc::FD_CLOEXEC != 0), "closed on exec as it's taken");
        for (fd, (reader, word)) in fds.into_iter().zip([(&mut a_read, "one"), (&mut b_read, "two")]) {
            // SAFETY: a descriptor the socket gave this test and nothing else holds.
            let mut w = unsafe { File::from_raw_fd(fd) };
            std::io::Write::write_all(&mut w, word.as_bytes()).unwrap();
            drop(w);
            let mut got = String::new();
            reader.read_to_string(&mut got).unwrap();
            assert_eq!(got, word);
        }
        assert!(recv_fds(keep.as_raw_fd(), 16).unwrap().is_none(), "nothing more, without waiting");
    }

    /// A hand-over's socket holds every terminal's descriptor at once, hundreds of them, with its
    /// room asked for (`socket_room`): nothing reads it until the next build does.
    #[test]
    fn a_hand_over_socket_holds_hundreds_of_descriptors_at_once() {
        use std::os::fd::AsRawFd;
        let (keep, give) = std::os::unix::net::UnixDatagram::pair().unwrap();
        socket_room(keep.as_raw_fd(), 4 * 1024 * 1024);
        socket_room(give.as_raw_fd(), 4 * 1024 * 1024);
        let pipes: Vec<_> = (0..240).map(|_| std::io::pipe().unwrap()).collect();
        for (n, batch) in pipes.chunks(16).enumerate() {
            let fds: Vec<RawFd> = batch.iter().map(|(_, w)| w.as_raw_fd()).collect();
            let ids = (0..batch.len()).map(|i| format!("term-1793770000000-{}", n * 16 + i)).collect::<Vec<_>>().join("\n");
            send_fds(give.as_raw_fd(), &fds, ids.as_bytes()).unwrap_or_else(|e| panic!("batch {n}: {e}"));
        }
        drop(give);
        let mut got = 0;
        while let Some((fds, _)) = recv_fds(keep.as_raw_fd(), 16).unwrap() {
            got += fds.len();
            for fd in fds {
                // SAFETY: a descriptor this test was given and owns.
                unsafe { libc::close(fd) };
            }
        }
        assert_eq!(got, 240);
    }

    #[test]
    fn a_new_size_reaches_the_program() {
        let size = Size { cols: 80, rows: 24, cell_width: 8, cell_height: 16 };
        let args = vec!["-c".to_string(), "read _; stty size".to_string()];
        let env = env();
        let mut pty = Pty::open(&Spawn { program: Path::new("/bin/sh"), arg0: "sh", args: &args, cwd: Path::new("/"), env: &env }, size).unwrap();
        resize(pty.master.as_raw_fd(), Size { cols: 120, rows: 40, cell_width: 8, cell_height: 16 });
        let mut file = File::from(pty.master.try_clone().unwrap());
        std::io::Write::write_all(&mut file, b"\n").unwrap();
        let out = read_until(&pty.master, "40 120", Duration::from_secs(5));
        assert!(out.contains("40 120"), "{out:?}");
        let _ = pty.child.wait();
    }
}
