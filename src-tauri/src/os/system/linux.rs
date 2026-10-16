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

/// Work nobody is waiting on: the calling thread gets a lower priority (nice 10), which Linux
/// keeps for each thread, so under load it yields to the agents and to the windows.
pub fn utility_thread() {
    // SAFETY: setpriority with `who` 0 changes the calling thread only.
    unsafe { libc::setpriority(libc::PRIO_PROCESS, 0, 10) };
}

/// Work a person waits on from another device. A thread can't be raised above the app's own
/// priority without privileges, so it's left at that.
pub fn user_initiated_thread() {}

/// Linux has no App Nap.
pub fn no_app_nap() {}

#[cfg(test)]
mod tests {
    use super::*;

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
