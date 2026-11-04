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

/// A copy of `fd` that outlives this process becoming another program (exec), for a server
/// handing its terminals to its next build (terminals.rs `hand_over`). None when there's none to
/// be had.
pub fn inheritable(fd: RawFd) -> Option<RawFd> {
    // SAFETY: dup makes a new descriptor for one we own; the copy isn't closed on exec.
    let copy = unsafe { libc::dup(fd) };
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
