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

/// This machine's load, memory and swap.
#[derive(Serialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct System {
    /// The load averages over 1, 5 and 15 minutes, and how many CPUs share it.
    pub load: [f64; 3],
    pub cpus: u32,
    /// Memory in bytes, and how much of it is free as the OS counts it (macOS for memory pressure, Linux
    /// what it could give a new program without swapping).
    pub memory: u64,
    pub free_percent: Option<u32>,
    /// What macOS's compressor holds, in bytes.
    pub compressed: Option<u64>,
    pub swap_total: u64,
    pub swap_used: u64,
}

/// How many files the server may hold open: one for each window, hook and command waiting on
/// it, and a pipe or three for each program it runs, under a busy day's agents.
const OPEN_FILES: u64 = 8192;

/// Raises this process's limit on open files to `OPEN_FILES`, as far as its hard limit lets it,
/// and says what it is now. launchd starts a job with 256 (`launchctl limit maxfiles`), which the
/// server ran out of within minutes on 2026-11-03: a hook's request it held couldn't read its
/// session's registry, took the session for gone and let the question go to its terminal.
/// `getrlimit` and `setrlimit` are POSIX's, the same on macOS and Linux.
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

#[cfg(test)]
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
