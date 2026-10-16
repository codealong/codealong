//! A kqueue, with a user event that wakes it and a process event for each process watched.

use std::time::Duration;

pub struct Waiter(i32);

/// What wakes it from another thread.
pub struct Waker(i32);

/// The identifier of the user event that wakes it.
const WAKE: usize = 1;

fn kevent(ident: usize, filter: i16, flags: u16, fflags: u32) -> libc::kevent {
    libc::kevent { ident, filter, flags, fflags, data: 0, udata: std::ptr::null_mut() }
}

impl Waiter {
    pub fn new() -> Option<Waiter> {
        // SAFETY: kqueue takes no arguments; a negative result is an error.
        let kq = unsafe { libc::kqueue() };
        if kq < 0 {
            return None;
        }
        let wake = kevent(WAKE, libc::EVFILT_USER, libc::EV_ADD | libc::EV_CLEAR, 0);
        // SAFETY: kevent reads one change from a struct we own and returns no events.
        unsafe { libc::kevent(kq, &wake, 1, std::ptr::null_mut(), 0, std::ptr::null()) };
        Some(Waiter(kq))
    }

    pub fn waker(&self) -> Waker {
        Waker(self.0)
    }

    pub fn watch_exit(&mut self, pid: u32) {
        let exit = kevent(pid as usize, libc::EVFILT_PROC, libc::EV_ADD | libc::EV_ONESHOT, libc::NOTE_EXIT);
        // SAFETY: as above. A process that's already gone is refused, and the next look finds
        // it gone.
        unsafe { libc::kevent(self.0, &exit, 1, std::ptr::null_mut(), 0, std::ptr::null()) };
    }

    /// Waits up to `timeout`, and says which watched processes exited.
    pub fn wait(&mut self, timeout: Duration) -> Vec<u32> {
        let mut events: [libc::kevent; 16] = [kevent(0, 0, 0, 0); 16];
        let timeout = libc::timespec { tv_sec: timeout.as_secs() as libc::time_t, tv_nsec: 0 };
        // SAFETY: kevent writes at most `events.len()` events into the array.
        let n = unsafe { libc::kevent(self.0, std::ptr::null(), 0, events.as_mut_ptr(), events.len() as libc::c_int, &timeout) };
        events.iter().take(n.max(0) as usize).filter(|e| e.filter == libc::EVFILT_PROC).map(|e| e.ident as u32).collect()
    }
}

impl Drop for Waiter {
    fn drop(&mut self) {
        // SAFETY: the descriptor is ours and nothing uses it after this.
        unsafe { libc::close(self.0) };
    }
}

impl Waker {
    pub fn wake(&self) {
        let wake = kevent(WAKE, libc::EVFILT_USER, 0, libc::NOTE_TRIGGER);
        // SAFETY: kevent reads one change from a struct we own and returns no events.
        unsafe { libc::kevent(self.0, &wake, 1, std::ptr::null_mut(), 0, std::ptr::null()) };
    }
}
