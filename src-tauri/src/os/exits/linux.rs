//! An eventfd that wakes it, and a pidfd for each process watched, which `poll` says is readable
//! once the process exits.

use std::time::Duration;

pub struct Waiter {
    wake: i32,
    exits: Vec<(u32, i32)>,
}

pub struct Waker(i32);

impl Waiter {
    pub fn new() -> Option<Waiter> {
        // SAFETY: eventfd takes plain values; a negative result is an error.
        let wake = unsafe { libc::eventfd(0, libc::EFD_NONBLOCK | libc::EFD_CLOEXEC) };
        (wake >= 0).then_some(Waiter { wake, exits: Vec::new() })
    }

    pub fn waker(&self) -> Waker {
        Waker(self.wake)
    }

    pub fn watch_exit(&mut self, pid: u32) {
        // SAFETY: pidfd_open takes a pid and flags; a negative result is an error, as for a
        // process that's already gone, which the next look finds gone.
        let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, pid as libc::pid_t, 0) } as i32;
        if fd >= 0 {
            self.exits.push((pid, fd));
        }
    }

    /// Waits up to `timeout`, and says which watched processes exited.
    pub fn wait(&mut self, timeout: Duration) -> Vec<u32> {
        let mut fds: Vec<libc::pollfd> = std::iter::once(self.wake).chain(self.exits.iter().map(|e| e.1)).map(|fd| libc::pollfd { fd, events: libc::POLLIN, revents: 0 }).collect();
        // SAFETY: poll writes only the `revents` of the descriptors it's given.
        let n = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, timeout.as_millis() as libc::c_int) };
        if n <= 0 {
            return Vec::new();
        }
        if fds[0].revents != 0 {
            let mut count = [0u8; 8];
            // SAFETY: an eventfd's count is the 8 bytes read into a buffer of that size.
            unsafe { libc::read(self.wake, count.as_mut_ptr().cast(), count.len()) };
        }
        let exited: Vec<usize> = fds[1..].iter().enumerate().filter(|(_, f)| f.revents != 0).map(|(i, _)| i).collect();
        let mut pids = Vec::new();
        for i in exited.into_iter().rev() {
            let (pid, fd) = self.exits.remove(i);
            // SAFETY: the descriptor is ours and nothing uses it after this.
            unsafe { libc::close(fd) };
            pids.push(pid);
        }
        pids
    }
}

impl Drop for Waiter {
    fn drop(&mut self) {
        for fd in std::iter::once(self.wake).chain(self.exits.iter().map(|e| e.1)) {
            // SAFETY: the descriptors are ours and nothing uses them after this.
            unsafe { libc::close(fd) };
        }
    }
}

impl Waker {
    pub fn wake(&self) {
        let one = 1u64.to_ne_bytes();
        // SAFETY: writing 8 bytes adds to an eventfd's count.
        unsafe { libc::write(self.0, one.as_ptr().cast(), one.len()) };
    }
}
