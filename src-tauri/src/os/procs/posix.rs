//! What macOS and Linux do alike with processes, POSIX's own calls: signals and process groups, a
//! program's name as it's started, whether a file may be run, priorities, and a child's pipes
//! read and written without blocking (`Pipes`), with `poll` waiting on them.

use std::io::{ErrorKind, Read, Write};
use std::os::fd::{AsRawFd, RawFd};
use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command};
use std::time::Duration;

use super::Signal;

fn number(sig: Signal) -> libc::c_int {
    match sig {
        Signal::HangUp => libc::SIGHUP,
        Signal::Interrupt => libc::SIGINT,
        Signal::Terminate => libc::SIGTERM,
        Signal::Kill => libc::SIGKILL,
        Signal::Continue => libc::SIGCONT,
    }
}

/// Whether a process is there, yours or not: a signal of 0 checks without sending one.
pub fn alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    // SAFETY: kill with signal 0 only checks for existence and permission.
    let r = unsafe { libc::kill(pid as libc::pid_t, 0) };
    r == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

/// macOS and Linux have `ps`, which procs.rs runs for the whole table and parses: there's no table
/// of the OS's own to read here.
pub fn table() -> Option<Vec<super::Row>> {
    None
}

/// Nor a process's command line, which `ps` says.
pub fn command_line(_pid: u32) -> Option<String> {
    None
}

/// Sends `sig` to the process `pid`.
pub fn signal(pid: u32, sig: Signal) -> std::io::Result<()> {
    // SAFETY: kill only sends a signal; a pid that's gone or not ours fails harmlessly.
    match unsafe { libc::kill(pid as libc::pid_t, number(sig)) } {
        0 => Ok(()),
        _ => Err(std::io::Error::last_os_error()),
    }
}

/// Sends `sig` to every process in the process group `group`.
pub fn signal_group(group: u32, sig: Signal) -> std::io::Result<()> {
    // SAFETY: kill with a negative pid signals that process group.
    match unsafe { libc::kill(-(group as libc::pid_t), number(sig)) } {
        0 => Ok(()),
        _ => Err(std::io::Error::last_os_error()),
    }
}

/// The process group `pid` is in.
pub fn group_of(pid: u32) -> Option<u32> {
    // SAFETY: getpgid takes a plain number; a process that's gone answers an error.
    let g = unsafe { libc::getpgid(pid as libc::pid_t) };
    (g > 0).then_some(g as u32)
}

/// The name `cmd`'s program sees itself run by (its `argv[0]`).
pub fn set_arg0(cmd: &mut Command, name: &std::ffi::OsStr) {
    use std::os::unix::process::CommandExt;
    cmd.arg0(name);
}

/// Whether this user may run the file at `at`, as the kernel decides it.
pub fn may_run(at: &std::path::Path) -> bool {
    use std::os::unix::ffi::OsStrExt;
    let Ok(c) = std::ffi::CString::new(at.as_os_str().as_bytes()) else {
        return false;
    };
    // SAFETY: access reads the NUL-terminated path it's given.
    unsafe { libc::access(c.as_ptr(), libc::X_OK) == 0 }
}

/// A program's own names to look for in a folder of a PATH: just its name.
pub fn names_to_find(program: &std::ffi::OsStr) -> Vec<std::ffi::OsString> {
    vec![program.to_os_string()]
}

/// Lowers a process just started below the app's priority (nice 10), so under load it yields to
/// the agents and to the windows.
pub fn lower_priority(pid: u32) {
    // SAFETY: setpriority on the process just started, which fails harmlessly if it's gone.
    unsafe {
        libc::setpriority(libc::PRIO_PROCESS, pid as libc::id_t, 10);
    }
}

/// Makes reads and writes on `pipe` answer at once, with `WouldBlock` where they'd wait.
pub fn nonblocking(pipe: &impl AsRawFd) {
    set_nonblocking(pipe.as_raw_fd());
}

fn set_nonblocking(fd: RawFd) {
    // SAFETY: fcntl on a descriptor this process owns, changing only its status flags.
    unsafe {
        let flags = libc::fcntl(fd, libc::F_GETFL);
        if flags >= 0 {
            libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
        }
    }
}

/// Waits up to `wait` for a nonblocking pipe that refused a write to take more.
pub fn wait_writable(pipe: &impl AsRawFd, wait: Duration) {
    let mut ready = libc::pollfd { fd: pipe.as_raw_fd(), events: libc::POLLOUT, revents: 0 };
    // SAFETY: one live pollfd, and a count of one.
    unsafe {
        libc::poll(&mut ready, 1, wait.as_millis().max(1) as libc::c_int);
    }
}

/// A child's stdin, stdout and stderr, driven from one thread without blocking: its input written
/// as it takes it, and what it writes read as it comes.
pub struct Pipes {
    stdin: Option<ChildStdin>,
    pending: Vec<u8>,
    out: Drain<ChildStdout>,
    err: Drain<ChildStderr>,
}

impl Pipes {
    /// Takes the child's pipes, with `input` to write to it.
    pub fn new(child: &mut Child, input: &[u8]) -> Pipes {
        let stdin = child.stdin.take().inspect(nonblocking);
        Pipes { stdin, pending: input.to_vec(), out: Drain::new(child.stdout.take()), err: Drain::new(child.stderr.take()) }
    }

    /// Writes what the child takes of its input now. Once it's all written, or the child closes
    /// its stdin without reading it all (which is its call), stdin is closed, so the program sees
    /// the end of its input.
    pub fn write(&mut self) {
        let Some(w) = self.stdin.as_mut() else { return };
        loop {
            if self.pending.is_empty() {
                self.stdin = None;
                return;
            }
            match w.write(&self.pending) {
                Ok(n) => drop(self.pending.drain(..n)),
                Err(e) if e.kind() == ErrorKind::Interrupted => {}
                Err(e) if e.kind() == ErrorKind::WouldBlock => return,
                Err(_) => {
                    self.stdin = None;
                    return;
                }
            }
        }
    }

    /// Closes its stdin, as when it has exited.
    pub fn close_input(&mut self) {
        self.stdin = None;
    }

    /// Takes what's in its stdout and stderr now.
    pub fn read(&mut self) {
        self.out.read();
        self.err.read();
    }

    /// Whether either of its outputs is still open.
    pub fn open(&self) -> bool {
        self.out.open() || self.err.open()
    }

    /// Waits up to `wait` for a pipe to be ready.
    pub fn wait(&mut self, wait: Duration) {
        let mut fds: Vec<libc::pollfd> = [self.out.fd(), self.err.fd()].into_iter().flatten().map(|fd| libc::pollfd { fd, events: libc::POLLIN, revents: 0 }).collect();
        if let Some(w) = self.stdin.as_ref() {
            fds.push(libc::pollfd { fd: w.as_raw_fd(), events: libc::POLLOUT, revents: 0 });
        }
        // SAFETY: `fds` is a live array of pollfd of the length passed.
        unsafe {
            libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, wait.as_millis().max(1) as libc::c_int);
        }
    }

    /// What it wrote to stdout and stderr.
    pub fn output(self) -> (Vec<u8>, Vec<u8>) {
        (self.out.got, self.err.got)
    }
}

/// One of a child's output pipes, read without blocking until it ends.
struct Drain<R> {
    pipe: Option<R>,
    got: Vec<u8>,
}

impl<R: Read + AsRawFd> Drain<R> {
    fn new(pipe: Option<R>) -> Self {
        if let Some(p) = &pipe {
            nonblocking(p);
        }
        Drain { pipe, got: Vec::new() }
    }

    fn open(&self) -> bool {
        self.pipe.is_some()
    }

    fn fd(&self) -> Option<RawFd> {
        self.pipe.as_ref().map(|p| p.as_raw_fd())
    }

    /// Takes what's in the pipe now. At most a megabyte a call, far more than a pipe holds, so
    /// something writing as fast as it's read can't keep the deadline from being looked at.
    fn read(&mut self) {
        let Some(p) = self.pipe.as_mut() else { return };
        let mut buf = [0u8; 16 * 1024];
        for _ in 0..64 {
            match p.read(&mut buf) {
                Ok(0) => {
                    self.pipe = None;
                    return;
                }
                Ok(n) => self.got.extend_from_slice(&buf[..n]),
                Err(e) if e.kind() == ErrorKind::Interrupted => {}
                Err(e) if e.kind() == ErrorKind::WouldBlock => return,
                Err(_) => {
                    self.pipe = None;
                    return;
                }
            }
        }
    }
}
