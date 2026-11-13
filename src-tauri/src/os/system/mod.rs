//! The machine as a whole: its load, memory and swap, what a process holds of memory (for
//! `codealong hygiene`), how the OS schedules Codealong's threads and the app, and how many files
//! the server may hold open.

use serde::Serialize;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::*;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::*;
#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use windows::*;

/// This machine's load, memory and swap.
#[derive(Serialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct System {
    /// The load averages over 1, 5 and 15 minutes, and how many CPUs share it. Windows keeps no
    /// load average: zeros there.
    pub load: [f64; 3],
    pub cpus: u32,
    /// Memory in bytes, and how much of it is free as the OS counts it (macOS for memory pressure, Linux
    /// what it could give a new program without swapping, Windows what's available).
    pub memory: u64,
    pub free_percent: Option<u32>,
    /// What macOS's compressor holds, in bytes.
    pub compressed: Option<u64>,
    pub swap_total: u64,
    pub swap_used: u64,
}

/// How many files the server may hold open: one for each window, hook and command waiting on
/// it, a pipe or three for each program it runs, under a busy day's agents, and the bursts of UDP
/// sockets iroh opens as the network changes (81 at once, `.claude/rules/devices.md`).
#[cfg(unix)]
const OPEN_FILES: u64 = 8192;

/// Raises this process's limit on open files to `OPEN_FILES`, as far as its hard limit lets it,
/// and says what it is now. launchd starts a job with 256 (`launchctl limit maxfiles`), which the
/// server ran out of within minutes on 2026-11-03: a hook's request it held couldn't read its
/// session's registry, took the session for gone and let the question go to its terminal.
/// `getrlimit` and `setrlimit` are POSIX's, the same on macOS and Linux; Windows has its own.
#[cfg(unix)]
pub fn more_open_files() -> Option<u64> {
    let mut lim = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
    // SAFETY: both read and write only the struct they're given.
    unsafe {
        if libc::getrlimit(libc::RLIMIT_NOFILE, &mut lim) != 0 {
            return None;
        }
        let want = OPEN_FILES.min(lim.rlim_max as u64);
        if lim.rlim_cur as u64 >= want {
            return Some(lim.rlim_cur as u64);
        }
        lim.rlim_cur = want as libc::rlim_t;
        (libc::setrlimit(libc::RLIMIT_NOFILE, &lim) == 0).then_some(want)
    }
}

/// A moment, in epoch seconds, on this machine's clock in its time zone as it was then.
/// `localtime_r` is POSIX's, the same on macOS and Linux; Windows has its own.
#[cfg(unix)]
pub fn localtime(secs: i64) -> libc::tm {
    let secs = secs as libc::time_t;
    // SAFETY: localtime_r fills the struct it's given; a zeroed tm is a valid one.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::localtime_r(&secs, &mut tm) };
    tm
}

/// A time on this machine's clock, its fields normalized as `mktime` does, in epoch seconds; None
/// for one the clock can't say. Whether summer time applies is `tm_isdst`'s to say, -1 for the
/// zone's.
#[cfg(unix)]
pub fn mktime(tm: &mut libc::tm) -> Option<i64> {
    // SAFETY: mktime only reads and normalizes the struct it's given.
    let secs = unsafe { libc::mktime(tm) };
    (secs != -1).then_some(secs as i64)
}

/// The load average over the last minute. POSIX's `getloadavg`, the same on macOS and Linux.
#[cfg(unix)]
pub fn load_average() -> f64 {
    let mut l = [0f64; 3];
    // SAFETY: getloadavg writes at most the one value asked for into a buffer of three.
    let n = unsafe { libc::getloadavg(l.as_mut_ptr(), 1) };
    if n >= 1 { l[0] } else { 0.0 }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn the_open_files_limit_is_raised_as_far_as_the_hard_limit_lets_it() {
        let mut lim = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
        // SAFETY: getrlimit writes only the struct it's given.
        assert_eq!(unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut lim) }, 0);
        let now = more_open_files().unwrap();
        assert!(now >= OPEN_FILES.min(lim.rlim_max as u64), "{now}");
        // Asked again, it's already there.
        assert_eq!(more_open_files(), Some(now));
    }
}
