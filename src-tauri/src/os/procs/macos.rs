//! The process table on macOS, from the kernel: `proc_pidinfo` for your own processes, `sysctl`
//! for any (`.claude/rules/windows.md` says what each answers).

use std::collections::HashMap;

use super::{Listed, Listener};

/// What the kernel keeps about one of your processes: its parent and start. None once it's
/// gone, and for another user's process.
fn bsd_info(pid: u32) -> Option<libc::proc_bsdinfo> {
    // SAFETY: proc_pidinfo fills a plain-old-data struct of the size we pass.
    unsafe {
        let mut info: libc::proc_bsdinfo = std::mem::zeroed();
        let size = std::mem::size_of::<libc::proc_bsdinfo>() as libc::c_int;
        let n = libc::proc_pidinfo(pid as libc::c_int, libc::PROC_PIDTBSDINFO, 0, &mut info as *mut libc::proc_bsdinfo as *mut libc::c_void, size);
        (n == size).then_some(info)
    }
}

/// Process start time in epoch milliseconds, read from the kernel.
pub fn proc_start_ms(pid: u32) -> Option<i64> {
    let info = bsd_info(pid)?;
    Some(info.pbi_start_tvsec as i64 * 1000 + info.pbi_start_tvusec as i64 / 1000)
}

/// A process's parent, from the kernel; None once it's gone.
pub fn parent_pid(pid: u32) -> Option<u32> {
    bsd_info(pid).map(|info| info.pbi_ppid).filter(|&p| p > 0)
}

/// A process's entry in the kernel's process table (`sysctl` KERN_PROC_PID), which `ps` reads
/// too, for any process: `proc_pidinfo` refuses other users', such as the root `login` between
/// a Ghostty tab's shell and Ghostty. Asking starts no process and reads nothing of the
/// process's own, so it answers at once however busy the Mac is: `ps`, asked the same on a Mac
/// that had stalled, took minutes, and a send waiting on it gave up on the session's terminal.
/// None once it's gone.
pub(crate) fn listed(pid: u32) -> Option<Listed> {
    // `struct kinfo_proc` (sys/sysctl.h) is 648 bytes on 64-bit macOS; these are the offsets of
    // the fields read, from the SDK's layout. `p_pid` is checked, so a layout that changed reads
    // as nothing rather than as nonsense.
    const SIZE: usize = 648;
    const P_PID: usize = 40;
    const P_COMM: usize = 243;
    const COMM_LEN: usize = 17;
    const E_PPID: usize = 560;
    const E_TDEV: usize = 572;
    const E_TPGID: usize = 576;
    let mut buf = [0u64; SIZE / 8];
    let mut len: libc::size_t = SIZE;
    let mut mib = [libc::CTL_KERN, libc::KERN_PROC, libc::KERN_PROC_PID, pid as libc::c_int];
    // SAFETY: sysctl writes at most `len` bytes into `buf`, which holds that many.
    let r = unsafe { libc::sysctl(mib.as_mut_ptr(), 4, buf.as_mut_ptr() as *mut libc::c_void, &mut len, std::ptr::null_mut(), 0) };
    if r != 0 || len != SIZE {
        return None;
    }
    // SAFETY: the bytes of the buffer just filled.
    let bytes = unsafe { std::slice::from_raw_parts(buf.as_ptr() as *const u8, SIZE) };
    let int = |at: usize| i32::from_ne_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]);
    if int(P_PID) != pid as i32 {
        return None;
    }
    let comm = &bytes[P_COMM..P_COMM + COMM_LEN];
    let name = String::from_utf8_lossy(&comm[..comm.iter().position(|&b| b == 0).unwrap_or(COMM_LEN)]).into_owned();
    Some(Listed { ppid: int(E_PPID) as u32, name, tdev: int(E_TDEV) as u32, tpgid: int(E_TPGID) })
}

/// The terminal a process runs in (`/dev/ttys006`), from the kernel; None for one with no
/// terminal, such as a background job, or once it's gone.
pub fn tty(pid: u32) -> Option<std::path::PathBuf> {
    extern "C" {
        fn devname_r(dev: libc::dev_t, kind: libc::mode_t, buf: *mut libc::c_char, len: libc::c_int) -> *mut libc::c_char;
    }
    // A process without a terminal has NODEV, all ones.
    let dev = listed(pid)?.tdev;
    if dev == u32::MAX {
        return None;
    }
    let mut buf = [0 as libc::c_char; 64];
    // SAFETY: devname_r writes at most `len` bytes, ending in a NUL, into `buf`, and returns it
    // (or null when it can't name the device).
    let name = unsafe {
        let p = devname_r(dev as libc::dev_t, libc::S_IFCHR, buf.as_mut_ptr(), buf.len() as libc::c_int);
        if p.is_null() {
            return None;
        }
        std::ffi::CStr::from_ptr(p).to_string_lossy().into_owned()
    };
    name.starts_with("tty").then(|| std::path::Path::new("/dev").join(name))
}

/// Every TCP port your processes listen on, from the kernel: one pass over the process table
/// and each process's sockets, starting no `lsof`. With 730 of your processes holding 1,400
/// sockets it took 4 to 6 ms, at a load of 15 to 25 (2026-09-28). The kernel doesn't answer
/// for other users' processes, so they're left out.
pub fn listeners() -> Vec<Listener> {
    // `struct socket_fdinfo` (sys/proc_info.h) is 792 bytes on 64-bit macOS; these are the
    // offsets of the fields read, from the SDK's layout. `listeners_are_found` checks them
    // against a port the test opens.
    const PROC_PIDFDSOCKETINFO: libc::c_int = 3;
    const SIZE: usize = 792;
    const SOI_KIND: usize = 256;
    const SOCKINFO_TCP: i32 = 2;
    const INSI_LPORT: usize = 268;
    const INSI_VFLAG: usize = 288;
    const INI_IPV6: u8 = 2;
    const INSI_LADDR: usize = 312;
    const TCPSI_STATE: usize = 344;
    const TSI_S_LISTEN: i32 = 1;

    // SAFETY: getuid can't fail.
    let me = unsafe { libc::getuid() };
    // SAFETY: with no buffer, proc_listallpids says how many processes there are.
    let count = unsafe { libc::proc_listallpids(std::ptr::null_mut(), 0) }.max(0) as usize;
    let mut pids = vec![0 as libc::c_int; count + 64];
    // SAFETY: proc_listallpids writes at most the buffer's size in pids, and says how many.
    let n = unsafe { libc::proc_listallpids(pids.as_mut_ptr() as *mut libc::c_void, (pids.len() * std::mem::size_of::<libc::c_int>()) as libc::c_int) }.max(0) as usize;
    pids.truncate(n.min(pids.len()));
    let mut fds: Vec<libc::proc_fdinfo> = Vec::with_capacity(256);
    let mut buf = [0u64; SIZE / 8];
    let mut out: Vec<Listener> = Vec::new();
    for pid in pids.into_iter().filter(|&p| p > 0) {
        // SAFETY: proc_pidinfo fills a plain-old-data struct of the size we pass.
        let short = unsafe {
            let mut info: libc::proc_bsdshortinfo = std::mem::zeroed();
            let size = std::mem::size_of::<libc::proc_bsdshortinfo>() as libc::c_int;
            let got = libc::proc_pidinfo(pid, libc::PROC_PIDT_SHORTBSDINFO, 0, &mut info as *mut _ as *mut libc::c_void, size);
            (got == size).then_some(info)
        };
        let Some(short) = short.filter(|i| i.pbsi_uid == me) else { continue };
        list_fds(pid, &mut fds);
        let mut ports: Vec<(u16, std::net::IpAddr)> = Vec::new();
        for fd in &fds {
            if fd.proc_fdtype as libc::c_int != libc::PROX_FDTYPE_SOCKET {
                continue;
            }
            // SAFETY: proc_pidfdinfo writes at most SIZE bytes into `buf`, which holds that many.
            let got = unsafe { libc::proc_pidfdinfo(pid, fd.proc_fd, PROC_PIDFDSOCKETINFO, buf.as_mut_ptr() as *mut libc::c_void, SIZE as libc::c_int) };
            if got as usize != SIZE {
                continue;
            }
            // SAFETY: the bytes of the buffer just filled.
            let b = unsafe { std::slice::from_raw_parts(buf.as_ptr() as *const u8, SIZE) };
            let int = |at: usize| i32::from_ne_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]]);
            if int(SOI_KIND) != SOCKINFO_TCP || int(TCPSI_STATE) != TSI_S_LISTEN {
                continue;
            }
            // The port is kept as the socket has it, in network order.
            let port = u16::from_be_bytes([b[INSI_LPORT], b[INSI_LPORT + 1]]);
            let addr: std::net::IpAddr = if b[INSI_VFLAG] & INI_IPV6 != 0 {
                let mut a = [0u8; 16];
                a.copy_from_slice(&b[INSI_LADDR..INSI_LADDR + 16]);
                let a = std::net::Ipv6Addr::from(a);
                if a.is_unspecified() { std::net::Ipv6Addr::LOCALHOST.into() } else { a.into() }
            } else {
                let a = std::net::Ipv4Addr::new(b[INSI_LADDR + 12], b[INSI_LADDR + 13], b[INSI_LADDR + 14], b[INSI_LADDR + 15]);
                if a.is_unspecified() { std::net::Ipv4Addr::LOCALHOST.into() } else { a.into() }
            };
            match ports.iter_mut().find(|(p, _)| *p == port) {
                // Listening on both families: IPv4 is reached the same way everywhere.
                Some(have) if have.1.is_ipv6() && addr.is_ipv4() => have.1 = addr,
                Some(_) => {}
                None => ports.push((port, addr)),
            }
        }
        if ports.is_empty() {
            continue;
        }
        let pid = pid as u32;
        let comm = short.pbsi_comm.iter().take_while(|&&c| c != 0).map(|&c| c as u8).collect::<Vec<u8>>();
        let name = String::from_utf8_lossy(&comm).into_owned();
        let cwd = cwd_of(pid).unwrap_or_default();
        let started_ms = proc_start_ms(pid).unwrap_or(0);
        out.extend(ports.into_iter().map(|(port, addr)| Listener { pid, port, addr, name: name.clone(), cwd: cwd.clone(), started_ms }));
    }
    out
}

/// Fills `fds` with a process's open file descriptors, from the kernel. False, and `fds` empty,
/// when the kernel doesn't answer: another user's process, or one that has gone.
fn list_fds(pid: libc::c_int, fds: &mut Vec<libc::proc_fdinfo>) -> bool {
    if fds.capacity() == 0 {
        fds.reserve(256);
    }
    loop {
        // SAFETY: proc_pidinfo writes at most the capacity's worth of `proc_fdinfo`s and says
        // how many bytes it wrote; `set_len` covers only those.
        let got = unsafe {
            let bytes = (fds.capacity() * std::mem::size_of::<libc::proc_fdinfo>()) as libc::c_int;
            let got = libc::proc_pidinfo(pid, libc::PROC_PIDLISTFDS, 0, fds.as_mut_ptr() as *mut libc::c_void, bytes);
            fds.set_len(0);
            if got <= 0 {
                return false;
            }
            got as usize / std::mem::size_of::<libc::proc_fdinfo>()
        };
        if got < fds.capacity() {
            // SAFETY: proc_pidinfo just wrote `got` entries.
            unsafe { fds.set_len(got) };
            return true;
        }
        // Full: there may be more than fit.
        fds.reserve(fds.capacity() * 2);
    }
}

/// The files one of your processes has open, from the kernel, starting no `lsof`. None when the
/// kernel doesn't answer: another user's process, or one that has gone.
pub fn open_files(pid: u32) -> Option<Vec<std::path::PathBuf>> {
    use std::os::unix::ffi::OsStrExt;
    // `struct vnode_fdinfowithpath` (sys/proc_info.h): a 24-byte `proc_fileinfo`, then the
    // vnode's details and its path. `open_files_are_found` checks the layout against a file the
    // test opens.
    const PROC_PIDFDVNODEPATHINFO: libc::c_int = 2;
    #[repr(C)]
    struct WithPath {
        file: [u8; 24],
        vnode: libc::vnode_info_path,
    }
    let mut fds = Vec::new();
    if !list_fds(pid as libc::c_int, &mut fds) {
        return None;
    }
    let mut out = Vec::new();
    for fd in fds.iter().filter(|fd| fd.proc_fdtype as libc::c_int == libc::PROX_FDTYPE_VNODE) {
        // SAFETY: all zeroes is a valid WithPath, and proc_pidfdinfo writes at most its size.
        let mut info: WithPath = unsafe { std::mem::zeroed() };
        let size = std::mem::size_of::<WithPath>() as libc::c_int;
        // SAFETY: as above; the call says how many bytes it wrote.
        let got = unsafe { libc::proc_pidfdinfo(pid as libc::c_int, fd.proc_fd, PROC_PIDFDVNODEPATHINFO, &mut info as *mut _ as *mut libc::c_void, size) };
        if got != size {
            continue;
        }
        let bytes: Vec<u8> = info.vnode.vip_path.iter().flatten().take_while(|&&c| c != 0).map(|&c| c as u8).collect();
        if !bytes.is_empty() {
            out.push(std::path::PathBuf::from(std::ffi::OsStr::from_bytes(&bytes)));
        }
    }
    Some(out)
}

/// The folder one of your processes runs in, from the kernel; None once it's gone.
pub fn cwd_of(pid: u32) -> Option<std::path::PathBuf> {
    // SAFETY: proc_pidinfo fills a plain-old-data struct of the size we pass; the path in it
    // ends in a NUL.
    unsafe {
        let mut info: libc::proc_vnodepathinfo = std::mem::zeroed();
        let size = std::mem::size_of::<libc::proc_vnodepathinfo>() as libc::c_int;
        if libc::proc_pidinfo(pid as libc::c_int, libc::PROC_PIDVNODEPATHINFO, 0, &mut info as *mut _ as *mut libc::c_void, size) != size {
            return None;
        }
        let path = std::ffi::CStr::from_ptr(info.pvi_cdir.vip_path.as_ptr() as *const libc::c_char);
        let path = path.to_string_lossy();
        (!path.is_empty()).then(|| std::path::PathBuf::from(path.as_ref()))
    }
}

/// The environment one of your processes was started with, from the kernel (`sysctl`
/// KERN_PROCARGS2, which `ps -E` reads): what it inherited, whoever its parent is now. None once
/// it's gone, and for another user's process. Apple's own programs (`/bin/sleep`, `/bin/zsh`)
/// show none but to root, which `ps` runs as: only their arguments come back (macOS 26).
pub fn environment(pid: u32) -> Option<HashMap<String, String>> {
    let mut mib = [libc::CTL_KERN, libc::KERN_PROCARGS2, pid as libc::c_int];
    let mut len: libc::size_t = 0;
    // SAFETY: with no buffer, sysctl only writes the size it needs into `len`.
    if unsafe { libc::sysctl(mib.as_mut_ptr(), 3, std::ptr::null_mut(), &mut len, std::ptr::null_mut(), 0) } != 0 {
        return None;
    }
    let mut buf = vec![0u8; len];
    // SAFETY: sysctl writes at most `len` bytes into `buf`, which holds that many, and sets
    // `len` to how many it wrote.
    if unsafe { libc::sysctl(mib.as_mut_ptr(), 3, buf.as_mut_ptr() as *mut libc::c_void, &mut len, std::ptr::null_mut(), 0) } != 0 {
        return None;
    }
    buf.truncate(len);
    procargs_environment(&buf)
}

/// The environment in KERN_PROCARGS2's bytes: the argument count, the executable's path and
/// the NULs that pad it, the arguments and then the environment, each ending in a NUL, and an
/// empty string after the last. An argument can be empty (`--tools ''`), so they're counted
/// rather than skipped; the padding is skipped as `ps` skips it.
fn procargs_environment(buf: &[u8]) -> Option<HashMap<String, String>> {
    let argc = i32::from_ne_bytes(buf.get(..4)?.try_into().ok()?);
    let rest = &buf[4..];
    let mut at = rest.iter().position(|&b| b == 0)?;
    while rest.get(at) == Some(&0) {
        at += 1;
    }
    for _ in 0..argc {
        at += rest.get(at..)?.iter().position(|&b| b == 0)? + 1;
    }
    let vars = rest.get(at..)?.split(|&b| b == 0).take_while(|v| !v.is_empty());
    Some(vars.filter_map(|v| std::str::from_utf8(v).ok()?.split_once('=')).map(|(k, v)| (k.to_string(), v.to_string())).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_environment_past_the_arguments_empty_ones_too() {
        let mut buf = 3i32.to_ne_bytes().to_vec();
        buf.extend_from_slice(b"/usr/local/bin/claude\0\0\0\0claude\0--tools\0\0HOME=/Users/me\0SESSION=a=b\0\0ptr_munge=\0");
        let env = procargs_environment(&buf).unwrap();
        // The empty argument isn't taken for the end of the arguments, and nothing past the
        // environment's end is in it.
        assert_eq!(env, HashMap::from([("HOME".to_string(), "/Users/me".to_string()), ("SESSION".to_string(), "a=b".to_string())]));
        assert_eq!(procargs_environment(&3i32.to_ne_bytes()), None);
    }
}
