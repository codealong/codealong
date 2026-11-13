//! An event that wakes it, and a handle to each process watched, which Windows signals once the
//! process exits: `WaitForMultipleObjects` waits on both.

use std::time::Duration;

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0};
use windows_sys::Win32::System::Threading::{CreateEventW, OpenProcess, SetEvent, WaitForMultipleObjects, WaitForSingleObject, PROCESS_SYNCHRONIZE};

/// The most handles one wait takes (`MAXIMUM_WAIT_OBJECTS`), the waker's among them; the processes
/// past it are looked at as the wait ends.
const MOST: usize = 64;

pub struct Waiter {
    wake: HANDLE,
    exits: Vec<(u32, HANDLE)>,
}

pub struct Waker(HANDLE);

// SAFETY: a handle is a kernel object's number, good on any thread of the process.
unsafe impl Send for Waiter {}
unsafe impl Send for Waker {}
unsafe impl Sync for Waker {}

impl Waiter {
    pub fn new() -> Option<Waiter> {
        // SAFETY: an unnamed auto-reset event, unsignaled; null is an error.
        let wake = unsafe { CreateEventW(std::ptr::null(), 0, 0, std::ptr::null()) };
        (!wake.is_null()).then_some(Waiter { wake, exits: Vec::new() })
    }

    pub fn waker(&self) -> Waker {
        Waker(self.wake)
    }

    pub fn watch_exit(&mut self, pid: u32) {
        // SAFETY: OpenProcess takes plain values; null is an error, as for a process that's
        // already gone, which the next look finds gone.
        let h = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
        if !h.is_null() {
            self.exits.push((pid, h));
        }
    }

    /// Waits up to `timeout`, and says which watched processes exited.
    pub fn wait(&mut self, timeout: Duration) -> Vec<u32> {
        let handles: Vec<HANDLE> = std::iter::once(self.wake).chain(self.exits.iter().map(|e| e.1)).take(MOST).collect();
        // SAFETY: the handles are ours and open; it returns which was signaled first, or a timeout.
        let n = unsafe { WaitForMultipleObjects(handles.len() as u32, handles.as_ptr(), 0, timeout.as_millis().min(u32::MAX as u128 - 1) as u32) };
        if n.wrapping_sub(WAIT_OBJECT_0) as usize >= handles.len() {
            return Vec::new();
        }
        // One wait says only the first; each is asked again without waiting.
        // SAFETY: as above, with no wait.
        let exited: Vec<usize> = self.exits.iter().enumerate().filter(|(_, e)| unsafe { WaitForSingleObject(e.1, 0) } == WAIT_OBJECT_0).map(|(i, _)| i).collect();
        let mut pids = Vec::new();
        for i in exited.into_iter().rev() {
            let (pid, h) = self.exits.remove(i);
            // SAFETY: the handle is ours and nothing uses it after this.
            unsafe { CloseHandle(h) };
            pids.push(pid);
        }
        pids
    }
}

impl Drop for Waiter {
    fn drop(&mut self) {
        for h in std::iter::once(self.wake).chain(self.exits.iter().map(|e| e.1)) {
            // SAFETY: the handles are ours and nothing uses them after this.
            unsafe { CloseHandle(h) };
        }
    }
}

impl Waker {
    pub fn wake(&self) {
        // SAFETY: signals the event; the waiting thread's wait returns and resets it.
        unsafe { SetEvent(self.0) };
    }
}
