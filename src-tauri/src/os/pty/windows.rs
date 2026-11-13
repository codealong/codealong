//! Windows has pseudo-consoles (ConPTY) where POSIX has pseudo-terminals, which Codealong doesn't
//! drive yet: opening a terminal fails saying so. A terminal's side and the hand-over's socket
//! wrap `Infallible`, so none is ever made, and what would act on one needs no body.

use std::convert::Infallible;
use std::io::{self, Read, Write};
use std::process::ExitStatus;

use super::{Pty, Size, Spawn};
use crate::os::procs::Fd;

impl Pty {
    pub fn open(_spawn: &Spawn<'_>, _size: Size) -> io::Result<Pty> {
        Err(io::Error::new(io::ErrorKind::Unsupported, crate::os::not_on_windows("Opening one of Codealong's terminals")))
    }
}

/// The terminal's side Codealong keeps, which a pseudo-console's pipes and handle will be.
pub struct Master(Infallible);

impl Master {
    pub fn try_clone(&self) -> io::Result<Master> {
        match self.0 {}
    }

    pub fn nonblocking(&self) -> io::Result<()> {
        match self.0 {}
    }

    pub fn readable(&self, _ms: i32) -> bool {
        match self.0 {}
    }

    pub fn writable(&self, _ms: i32) -> bool {
        match self.0 {}
    }

    pub fn resize(&self, _size: Size) {
        match self.0 {}
    }

    pub fn foreground(&self) -> Option<u32> {
        match self.0 {}
    }

    pub fn redraw(&self) {
        match self.0 {}
    }

    pub fn to_hand_over(&self) -> Option<Fd> {
        match self.0 {}
    }

    /// Nothing is handed over on Windows (`HandOverSocket`).
    ///
    /// # Safety
    ///
    /// None needed: nothing is taken.
    pub unsafe fn from_handed(_fd: Fd) -> io::Result<Master> {
        Err(io::Error::new(io::ErrorKind::Unsupported, crate::os::not_on_windows("Handing terminals to a new build")))
    }
}

impl Read for Master {
    fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
        match self.0 {}
    }
}

impl Write for Master {
    fn write(&mut self, _buf: &[u8]) -> io::Result<usize> {
        match self.0 {}
    }

    fn flush(&mut self) -> io::Result<()> {
        match self.0 {}
    }
}

/// No terminal of Codealong's runs here, so there's no session on one to hang up on.
pub fn hang_up(_pid: u32, _kill: bool) {}

/// A process that ended leaves nothing on Windows for its parent to reap.
pub fn reap(_pid: u32) {}

/// Nothing is taken over from another build here, so there's no child of one to wait on by its pid.
pub fn wait_child(_pid: u32) -> Option<i32> {
    None
}

/// How a process ended: Windows always has its exit code, signals being POSIX's.
pub fn exit_code(status: &ExitStatus) -> i32 {
    status.code().unwrap_or(-1)
}

/// No descriptor is made or taken for a hand-over here.
///
/// # Safety
///
/// None needed: nothing is closed.
pub unsafe fn close(_fd: Fd) {}

/// A server can't become another build in place on Windows (`procs::exec_keeping`), so nothing
/// hands its terminals over through a socket.
pub struct HandOverSocket(Infallible);

impl HandOverSocket {
    pub fn pair(_room: usize) -> io::Result<(HandOverSocket, HandOverSocket)> {
        Err(io::Error::new(io::ErrorKind::Unsupported, crate::os::not_on_windows("Handing terminals to a new build")))
    }

    pub fn fd(&self) -> Fd {
        match self.0 {}
    }

    pub fn send(&self, _fds: &[Fd], _payload: &[u8]) -> io::Result<()> {
        match self.0 {}
    }

    pub fn recv(&self, _most: usize) -> io::Result<Option<(Vec<Fd>, Vec<u8>)>> {
        match self.0 {}
    }

    /// # Safety
    ///
    /// None needed: nothing is taken.
    pub unsafe fn from_handed(_fd: Fd) -> io::Result<HandOverSocket> {
        Err(io::Error::new(io::ErrorKind::Unsupported, crate::os::not_on_windows("Handing terminals to a new build")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn opening_a_terminal_says_it_isnt_on_windows_yet() {
        let size = Size { cols: 80, rows: 24, cell_width: 8, cell_height: 16 };
        let spawn = Spawn { program: Path::new("cmd.exe"), arg0: "cmd", args: &[], cwd: Path::new("C:\\"), env: &[] };
        let e = Pty::open(&spawn, size).err().expect("no terminal");
        assert_eq!(e.kind(), io::ErrorKind::Unsupported);
        assert_eq!(e.to_string(), "Opening one of Codealong's terminals isn't on Windows yet.");
        assert!(HandOverSocket::pair(1024).is_err());
    }
}
