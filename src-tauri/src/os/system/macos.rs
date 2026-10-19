//! The machine on macOS: `sysctl` and `proc_pid_rusage`, threads' quality-of-service classes, and
//! App Nap.

use super::System;

fn sysctl<T: Copy>(name: &str) -> Option<T> {
    let name = std::ffi::CString::new(name).ok()?;
    // SAFETY: sysctlbyname writes at most `size` bytes into a zeroed plain-old-data value, and
    // the size it reports back is checked against the value's.
    unsafe {
        let mut value: T = std::mem::zeroed();
        let mut size = std::mem::size_of::<T>();
        let ok = libc::sysctlbyname(name.as_ptr(), &mut value as *mut T as *mut libc::c_void, &mut size, std::ptr::null_mut(), 0) == 0;
        (ok && size == std::mem::size_of::<T>()).then_some(value)
    }
}

pub fn system() -> System {
    let mut load = [0f64; 3];
    // SAFETY: getloadavg fills at most three doubles.
    unsafe { libc::getloadavg(load.as_mut_ptr(), 3) };
    let swap: Option<libc::xsw_usage> = sysctl("vm.swapusage");
    System {
        load,
        cpus: sysctl::<i32>("hw.ncpu").unwrap_or(0).max(0) as u32,
        memory: sysctl::<u64>("hw.memsize").unwrap_or(0),
        free_percent: sysctl::<i32>("kern.memorystatus_level").map(|l| l.max(0) as u32),
        compressed: sysctl::<u64>("vm.compressor_bytes_used"),
        swap_total: swap.map_or(0, |s| s.xsu_total),
        swap_used: swap.map_or(0, |s| s.xsu_used),
    }
}

/// What a process of yours holds of memory, compressed and swapped included: Activity
/// Monitor's Memory column. None for one that's gone or another user's.
pub fn footprint(pid: u32) -> Option<u64> {
    // SAFETY: proc_pid_rusage fills a plain-old-data struct of the flavor asked for.
    unsafe {
        let mut info: libc::rusage_info_v4 = std::mem::zeroed();
        let ok = libc::proc_pid_rusage(pid as libc::c_int, libc::RUSAGE_INFO_V4, &mut info as *mut libc::rusage_info_v4 as *mut libc::rusage_info_t) == 0;
        ok.then_some(info.ri_phys_footprint)
    }
}

/// Puts the calling thread in macOS's utility class: work nobody is waiting on, which under load
/// yields to the agents and to the windows.
pub fn utility_thread() {
    thread_qos(0x11);
}

/// Puts the calling thread in macOS's user-initiated class, for work a person is waiting on from
/// another device (their phone opening a session, a Mac asking for a page): it goes ahead of the
/// agents' builds rather than behind them, as the engine's own scanning does.
pub fn user_initiated_thread() {
    thread_qos(0x19);
}

fn thread_qos(class: u32) {
    extern "C" {
        fn pthread_set_qos_class_self_np(qos_class: u32, relative_priority: i32) -> i32;
    }
    // SAFETY: affects the calling thread only, with constant arguments.
    unsafe {
        pthread_set_qos_class_self_np(class, 0);
    }
}

/// Keeps macOS from napping the app for as long as it runs. App Nap takes an app whose windows
/// aren't in front for one that's idle: every thread of it ran at the lowest priority there is
/// (4, where the agents' work runs at 31 and up) and its timers were put off, so a phone or
/// another Mac waited on it whenever this Mac was busy, and the bar's one-second timers ran a
/// second late. The Mac can still sleep when idle.
pub fn no_app_nap() {
    use objc2_foundation::{NSActivityOptions, NSProcessInfo, NSString};
    let reason = NSString::from_str("Codealong keeps your board live and answers your other devices");
    let activity = NSProcessInfo::processInfo().beginActivityWithOptions_reason(NSActivityOptions::UserInitiatedAllowingIdleSystemSleep, &reason);
    // Held until the app quits: ending it lets macOS nap the app again.
    std::mem::forget(activity);
}

/// Whether the screen is locked, as the window server's login session says. Programs carry on
/// then, but an app can't make a working window: Ghostty's new terminals fail to start
/// (".claude/rules/terminals.md"). False for a process outside a login session (over SSH), which
/// has no session to ask.
pub fn screen_locked() -> bool {
    use objc2::msg_send;
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2_foundation::NSString;
    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGSessionCopyCurrentDictionary() -> *mut AnyObject;
    }
    // SAFETY: the copy is ours (+1), or null outside a login session; a CFDictionary is toll-free
    // bridged to NSDictionary.
    let Some(session) = (unsafe { Retained::from_raw(CGSessionCopyCurrentDictionary()) }) else { return false };
    let key = NSString::from_str("CGSSessionScreenIsLocked");
    // SAFETY: NSDictionary's `objectForKey:`, which answers nil for a key it hasn't: the key is
    // there only while the screen is locked.
    let locked: Option<Retained<AnyObject>> = unsafe { msg_send![&*session, objectForKey: &*key] };
    // SAFETY: the value is a CFBoolean, toll-free bridged to NSNumber, which answers `boolValue`.
    locked.is_some_and(|v| unsafe { msg_send![&*v, boolValue] })
}

/// Fills `buf` with random bytes from the kernel.
pub fn random_bytes(buf: &mut [u8]) {
    // SAFETY: it writes the length it's given into the buffer.
    unsafe { libc::arc4random_buf(buf.as_mut_ptr().cast(), buf.len()) };
}
