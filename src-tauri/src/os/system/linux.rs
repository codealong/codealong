//! The machine on Linux: `/proc`, and threads' niceness.

use super::System;

/// Linux's load, memory and swap: `/proc/meminfo`, where what's free is what the kernel says it
/// could give a new program without swapping (`MemAvailable`).
pub fn system() -> System {
    let mut load = [0f64; 3];
    // SAFETY: getloadavg fills at most three doubles.
    unsafe { libc::getloadavg(load.as_mut_ptr(), 3) };
    let info = std::fs::read_to_string("/proc/meminfo").unwrap_or_default();
    let kb = |name: &str| meminfo_kb(&info, name);
    let memory = kb("MemTotal").unwrap_or(0) * 1024;
    let available = kb("MemAvailable").map(|a| a * 1024);
    let (swap_total, swap_free) = (kb("SwapTotal").unwrap_or(0) * 1024, kb("SwapFree").unwrap_or(0) * 1024);
    System {
        load,
        cpus: std::thread::available_parallelism().map_or(0, |n| n.get() as u32),
        memory,
        free_percent: available.filter(|_| memory > 0).map(|a| (a * 100 / memory) as u32),
        compressed: None,
        swap_total,
        swap_used: swap_total.saturating_sub(swap_free),
    }
}

/// A `Name: <n> kB` line of `/proc/meminfo` or `/proc/<pid>/status`, in kB.
fn meminfo_kb(text: &str, name: &str) -> Option<u64> {
    text.lines().find_map(|l| l.strip_prefix(name)?.strip_prefix(':')?.split_whitespace().next()?.parse().ok())
}

/// What a process of yours holds of memory, swapped included: what's resident and what's in
/// swap, from `/proc/<pid>/status`.
pub fn footprint(pid: u32) -> Option<u64> {
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    Some((meminfo_kb(&status, "VmRSS")? + meminfo_kb(&status, "VmSwap").unwrap_or(0)) * 1024)
}

/// Work nobody is waiting on, left at the app's own priority. Linux keeps a nice value for each
/// thread and a thread takes its starter's, but only a privileged process can lower one again: a
/// utility thread's nice 10 would hold back for good every thread it starts, the peers' request
/// handlers among them, which `user_initiated_thread` couldn't raise as macOS's classes are.
pub fn utility_thread() {}

/// Work a person waits on: the engine's thread, and another device's requests. A thread can't be
/// raised above the app's own priority without privileges, so it's left at that.
pub fn user_initiated_thread() {}

/// Linux has no App Nap.
pub fn no_app_nap() {}

/// Never locked, as far as Codealong goes: each desktop's locker says so in its own way, and a
/// terminal there is a tmux session already (launch/linux.rs), which a locked screen doesn't stop.
pub fn screen_locked() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A thread started from utility work runs as the app does, so one a person waits on isn't
    /// held back.
    #[test]
    fn threads_started_from_utility_work_keep_the_apps_priority() {
        // SAFETY: getpriority with `who` 0 reads the calling thread's nice value.
        let nice = || unsafe { libc::getpriority(libc::PRIO_PROCESS, 0) };
        let app = nice();
        let theirs = std::thread::spawn(move || {
            utility_thread();
            std::thread::spawn(move || {
                user_initiated_thread();
                nice()
            })
            .join()
            .unwrap()
        })
        .join()
        .unwrap();
        assert_eq!(theirs, app);
    }

    #[test]
    fn linux_memory_reads_from_its_kb_lines() {
        let info = "MemTotal:       16364112 kB\nMemFree:         9813420 kB\nMemAvailable:   14512340 kB\nSwapTotal:             0 kB\n";
        assert_eq!(meminfo_kb(info, "MemTotal"), Some(16364112));
        assert_eq!(meminfo_kb(info, "MemAvailable"), Some(14512340));
        assert_eq!(meminfo_kb(info, "SwapTotal"), Some(0));
        // A name is a whole name: MemFree isn't Mem.
        assert_eq!(meminfo_kb(info, "Mem"), None);
        assert_eq!(meminfo_kb("Name:\tclaude\nVmRSS:\t  412300 kB\n", "VmRSS"), Some(412300));
    }
}

/// Fills `buf` with random bytes from the kernel.
pub fn random_bytes(buf: &mut [u8]) {
    // SAFETY: it writes at most the length it's given into the buffer.
    unsafe { libc::getrandom(buf.as_mut_ptr().cast(), buf.len(), 0) };
}
