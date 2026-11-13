//! The machine on Windows: its memory as `GlobalMemoryStatusEx` counts it, a process's commit,
//! threads' priorities, and whether the workstation is locked.

use windows_sys::Win32::Foundation::CloseHandle;
use windows_sys::Win32::Security::Cryptography::{BCryptGenRandom, BCRYPT_USE_SYSTEM_PREFERRED_RNG};
use windows_sys::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
use windows_sys::Win32::System::StationsAndDesktops::{CloseDesktop, OpenInputDesktop, DESKTOP_SWITCHDESKTOP};
use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
use windows_sys::Win32::System::Threading::{GetCurrentThread, OpenProcess, SetThreadPriority, PROCESS_QUERY_LIMITED_INFORMATION, THREAD_PRIORITY_BELOW_NORMAL, THREAD_PRIORITY_NORMAL};

use super::System;

/// Windows' memory and page file. What's free is what it says is available; the page file is
/// what its commit limit holds beyond the memory itself. It keeps no load average.
pub fn system() -> System {
    // SAFETY: a zeroed struct with its length set, which the call fills.
    let mut m: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
    m.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
    let ok = unsafe { GlobalMemoryStatusEx(&mut m) } != 0;
    let swap_total = m.ullTotalPageFile.saturating_sub(m.ullTotalPhys);
    let swap_free = m.ullAvailPageFile.saturating_sub(m.ullAvailPhys).min(swap_total);
    System {
        load: [0.0; 3],
        cpus: std::thread::available_parallelism().map_or(0, |n| n.get() as u32),
        memory: m.ullTotalPhys,
        free_percent: (ok && m.ullTotalPhys > 0).then(|| (m.ullAvailPhys * 100 / m.ullTotalPhys) as u32),
        compressed: None,
        swap_total,
        swap_used: swap_total - swap_free,
    }
}

/// What a process of yours holds of memory, paged out included: its commit charge.
pub fn footprint(pid: u32) -> Option<u64> {
    // SAFETY: the handle is closed before returning; the counters are a struct the call fills.
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            return None;
        }
        let mut c: PROCESS_MEMORY_COUNTERS = std::mem::zeroed();
        let ok = GetProcessMemoryInfo(h, &mut c, std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32) != 0;
        CloseHandle(h);
        ok.then_some(c.PagefileUsage as u64)
    }
}

/// Work nobody is waiting on, below the app's priority. A thread Windows starts runs at normal
/// priority whatever its starter's, so this holds back none it starts.
pub fn utility_thread() {
    // SAFETY: the pseudo-handle of the calling thread needs no closing.
    unsafe { SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_BELOW_NORMAL) };
}

/// Work a person waits on, at the app's priority.
pub fn user_initiated_thread() {
    // SAFETY: as above.
    unsafe { SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_NORMAL) };
}

/// Windows has no App Nap.
pub fn no_app_nap() {}

/// Whether the workstation is locked: then the desktop that takes input is the sign-in screen's,
/// which a program of yours can't open.
pub fn screen_locked() -> bool {
    // SAFETY: the desktop handle, when there is one, is closed at once.
    unsafe {
        let desk = OpenInputDesktop(0, 0, DESKTOP_SWITCHDESKTOP);
        if desk.is_null() {
            return true;
        }
        CloseDesktop(desk);
    }
    false
}

/// A moment, in epoch seconds, on this machine's clock in its time zone as it was then.
pub fn localtime(secs: i64) -> libc::tm {
    let secs = secs as libc::time_t;
    // SAFETY: localtime_s fills the struct it's given; a zeroed tm is a valid one.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::localtime_s(&mut tm, &secs) };
    tm
}

extern "C" {
    /// The C runtime's `mktime`, with a 64-bit time, which the libc crate leaves out on Windows.
    fn _mktime64(tm: *mut libc::tm) -> i64;
}

/// A time on this machine's clock, its fields normalized as `mktime` does, in epoch seconds; None
/// for one the clock can't say.
pub fn mktime(tm: &mut libc::tm) -> Option<i64> {
    // SAFETY: _mktime64 only reads and normalizes the struct it's given.
    let secs = unsafe { _mktime64(tm) };
    (secs != -1).then_some(secs)
}

/// Windows keeps no load average.
pub fn load_average() -> f64 {
    0.0
}

/// Windows holds a process to no limit on open files worth raising: some 16 million handles.
pub fn more_open_files() -> Option<u64> {
    Some(1 << 24)
}

/// Fills `buf` with random bytes from the system's generator.
pub fn random_bytes(buf: &mut [u8]) {
    for chunk in buf.chunks_mut(u32::MAX as usize) {
        // SAFETY: it writes the length it's given into the buffer.
        unsafe { BCryptGenRandom(std::ptr::null_mut(), chunk.as_mut_ptr(), chunk.len() as u32, BCRYPT_USE_SYSTEM_PREFERRED_RNG) };
    }
}
