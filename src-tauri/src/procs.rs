//! Processes: liveness checks without spawning, the user's login-shell PATH, and a command
//! runner with a timeout so a stuck `git` or `gh` can never wedge the engine.

use std::collections::HashMap;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};

use crate::util::home;

pub fn pid_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    // SAFETY: kill with signal 0 only checks for existence and permission.
    let r = unsafe { libc::kill(pid as libc::pid_t, 0) };
    r == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

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

/// A process as `ps` lists it: its parent, short name and terminal device.
struct Listed {
    ppid: u32,
    name: String,
    tdev: u32,
}

/// A process's entry in the kernel's process table (`sysctl` KERN_PROC_PID), which `ps` reads
/// too, for any process: `proc_pidinfo` refuses other users', such as the root `login` between
/// a Ghostty tab's shell and Ghostty. Asking starts no process and reads nothing of the
/// process's own, so it answers at once however busy the Mac is: `ps`, asked the same on a Mac
/// that had stalled, took minutes, and a send waiting on it gave up on the session's terminal.
/// None once it's gone.
fn listed(pid: u32) -> Option<Listed> {
    // `struct kinfo_proc` (sys/sysctl.h) is 648 bytes on 64-bit macOS; these are the offsets of
    // the fields read, from the SDK's layout. `p_pid` is checked, so a layout that changed reads
    // as nothing rather than as nonsense.
    const SIZE: usize = 648;
    const P_PID: usize = 40;
    const P_COMM: usize = 243;
    const COMM_LEN: usize = 17;
    const E_PPID: usize = 560;
    const E_TDEV: usize = 572;
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
    Some(Listed { ppid: int(E_PPID) as u32, name, tdev: int(E_TDEV) as u32 })
}

/// A process's parent and name as `ps -c` shows them (its executable's file name, cut to 16
/// characters), for any process. None once it's gone.
pub fn parent_and_name(pid: u32) -> Option<(u32, String)> {
    listed(pid).map(|p| (p.ppid, p.name))
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
    name.starts_with("tty").then(|| Path::new("/dev").join(name))
}

/// A process and its ancestors up to launchd, nearest first.
pub fn ancestors(pid: u32) -> Vec<u32> {
    let mut out = vec![pid];
    let mut p = pid;
    while let Some(parent) = parent_pid(p).filter(|&q| q > 1 && !out.contains(&q)) {
        out.push(parent);
        p = parent;
        if out.len() > 64 {
            break;
        }
    }
    out
}

/// A TCP port one of your processes listens on.
#[derive(Clone, Debug, PartialEq)]
pub struct Listener {
    pub pid: u32,
    pub port: u16,
    /// Where to reach it on this Mac: the address it's bound to, or loopback when it listens on
    /// every address.
    pub addr: std::net::IpAddr,
    /// The executable's short name (`node`), as `ps -c` has it.
    pub name: String,
    /// The folder it runs in.
    pub cwd: std::path::PathBuf,
    pub started_ms: i64,
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

/// The PATH as the login shell sets it, with none of Codealong's own additions: what a
/// command typed in a terminal can find. None when the shell didn't answer.
pub fn login_path() -> Option<&'static str> {
    static PATH: OnceLock<Option<String>> = OnceLock::new();
    PATH.get_or_init(|| {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
        run_with_path(&shell, &["-lc", "printf %s \"$PATH\""], None, Duration::from_secs(4), "/usr/bin:/bin", false).ok().filter(|p| p.contains('/'))
    })
    .as_deref()
}

/// The PATH of the user's login shell. Apps launched from Finder get a minimal PATH, so
/// `git` and `gh` would otherwise not be found.
pub fn user_path() -> &'static str {
    static PATH: OnceLock<String> = OnceLock::new();
    PATH.get_or_init(|| {
        let h = home();
        let mut parts: Vec<String> = login_path().map(|p| p.split(':').map(String::from).collect()).unwrap_or_default();
        for extra in [
            h.join(".local/bin").display().to_string(),
            "/opt/homebrew/bin".to_string(),
            "/usr/local/bin".to_string(),
            "/usr/bin".to_string(),
            "/bin".to_string(),
            "/usr/sbin".to_string(),
            "/sbin".to_string(),
        ] {
            if !parts.contains(&extra) {
                parts.push(extra);
            }
        }
        parts.join(":")
    })
}

/// The first child of `pid` whose command line `pick` makes something of, asking for each child's
/// command line in turn until one does.
pub fn find_child<T>(pid: u32, mut pick: impl FnMut(&str) -> Option<T>) -> Option<T> {
    let kids = run("pgrep", &["-P", &pid.to_string()], None, Duration::from_secs(2)).ok()?;
    kids.split_whitespace().find_map(|kid| run("ps", &["-ww", "-o", "command=", "-p", kid], None, Duration::from_secs(2)).ok().and_then(|c| pick(&c)))
}

/// What the commands a session runs are made of right now: every process under the children of
/// the session's process that run them (`root`, by command line; a harness says which) with the
/// CPU each has used, in hundredths of a second. For telling a command that polls or builds from
/// one left sitting there.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CommandTree {
    pub cpu: Vec<(u32, u64)>,
}

impl CommandTree {
    /// Whether anything happened between two looks: a process started, or one that was
    /// already there used a tenth of a second of CPU or more. A process that exited took its
    /// CPU with it, so totals can't be compared.
    pub fn active_since(&self, before: &CommandTree) -> bool {
        self.cpu.iter().any(|(pid, cs)| match before.cpu.iter().find(|(p, _)| p == pid) {
            Some((_, was)) => cs.saturating_sub(*was) >= 10,
            None => true,
        })
    }
}

/// The command trees of several sessions' processes, from one look at the process table.
pub fn command_trees(pids: &[u32], root: fn(&str) -> bool) -> HashMap<u32, CommandTree> {
    match run_background("ps", &["-axww", "-o", "pid=,ppid=,time=,command="], None, Duration::from_secs(4)) {
        Ok(table) => command_trees_in(&table, pids, root),
        Err(_) => HashMap::new(),
    }
}

pub(crate) fn command_trees_in(table: &str, pids: &[u32], root: fn(&str) -> bool) -> HashMap<u32, CommandTree> {
    let rows: Vec<(u32, u32, u64, &str)> = table
        .lines()
        .filter_map(|l| {
            let mut f = l.split_whitespace();
            let pid = f.next()?.parse().ok()?;
            let ppid = f.next()?.parse().ok()?;
            let cpu = cpu_hundredths(f.next()?)?;
            let rest = f.next().map(|first| &l[l.find(first).unwrap_or(0)..]).unwrap_or("");
            Some((pid, ppid, cpu, rest))
        })
        .collect();
    pids.iter()
        .map(|&session| {
            let mut tree: Vec<(u32, u64)> = rows.iter().filter(|r| r.1 == session && root(r.3)).map(|r| (r.0, r.2)).collect();
            let mut i = 0;
            while i < tree.len() && tree.len() < 512 {
                let parent = tree[i].0;
                tree.extend(rows.iter().filter(|r| r.1 == parent).map(|r| (r.0, r.2)));
                i += 1;
            }
            (session, CommandTree { cpu: tree })
        })
        .collect()
}

/// `ps`'s `time`: `0:00.88`, `12:34.56` or `1:02:03.45`, in hundredths of a second.
fn cpu_hundredths(t: &str) -> Option<u64> {
    let (whole, frac) = t.split_once('.').unwrap_or((t, "0"));
    let secs = whole.split(':').try_fold(0u64, |acc, part| part.parse::<u64>().ok().map(|n| acc * 60 + n))?;
    let frac: u64 = format!("{:0<2}", frac).get(..2)?.parse().ok()?;
    Some(secs * 100 + frac)
}

/// A process as `ps` lists it.
#[derive(Debug, Clone, PartialEq)]
pub struct PsLine {
    pub pid: u32,
    /// Its terminal as `ps` names it (`ttys004`), or `??` for none.
    pub tty: String,
    /// Its command line: `ps` joins the arguments with spaces.
    pub command: String,
}

/// Every process on this Mac, from one `ps` run the first time it's asked for and kept. The
/// harnesses' readers each look for their own terminals in it, so a tick that asks them all
/// (`harness::Look`) runs `ps` once.
#[derive(Default)]
pub struct Listing(OnceLock<Vec<PsLine>>);

impl Listing {
    pub fn all(&self) -> &[PsLine] {
        self.0.get_or_init(|| run("/bin/ps", &["-axo", "pid=,tty=,command="], None, Duration::from_secs(3)).map(|out| ps_lines(&out)).unwrap_or_default())
    }
}

/// `ps -o pid=,tty=,command=` as processes.
fn ps_lines(out: &str) -> Vec<PsLine> {
    out.lines()
        .filter_map(|line| {
            let (pid, rest) = line.trim_start().split_once(char::is_whitespace)?;
            let (tty, command) = rest.trim_start().split_once(char::is_whitespace)?;
            Some(PsLine { pid: pid.parse().ok()?, tty: tty.to_string(), command: command.trim().to_string() })
        })
        .collect()
}

/// Runs a program with the user's PATH and returns stdout. Fails on a non-zero exit (with
/// stderr in the message) or when `timeout` passes.
pub fn run(program: &str, args: &[&str], cwd: Option<&Path>, timeout: Duration) -> Result<String> {
    #[cfg(test)]
    if let Some(answer) = stand_in::run(program, args, None, timeout) {
        return answer;
    }
    run_with_path(program, args, cwd, timeout, user_path(), false)
}

/// `run`, with `unset` left out of the program's environment.
pub fn run_without(program: &str, args: &[&str], unset: &[&str], timeout: Duration) -> Result<String> {
    #[cfg(test)]
    if let Some(answer) = stand_in::run(program, args, None, timeout) {
        return answer;
    }
    let mut cmd = Command::new(program);
    cmd.args(args).env("PATH", user_path());
    for name in unset {
        cmd.env_remove(name);
    }
    let o = drive(cmd, program, None, timeout)?;
    if o.ok {
        Ok(o.out)
    } else {
        Err(anyhow!("{} {}", program, o.err.trim()))
    }
}

/// Runs macOS's `open` with `args`: a link, a file, or an app by its name, path or bundle ID. Each
/// caller says how long it gets and what a failure is called.
pub fn open(args: &[&str], timeout: Duration) -> Result<()> {
    run("/usr/bin/open", args, None, timeout).map(|_| ())
}

/// `run`, for work nobody is waiting on: the process gets a lower priority (nice 10), so
/// under load it yields to the agents and to the windows.
pub fn run_background(program: &str, args: &[&str], cwd: Option<&Path>, timeout: Duration) -> Result<String> {
    run_with_path(program, args, cwd, timeout, user_path(), true)
}

/// Puts the calling thread in macOS's utility class, for the same reason.
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
    #[cfg(target_os = "macos")]
    {
        extern "C" {
            fn pthread_set_qos_class_self_np(qos_class: u32, relative_priority: i32) -> i32;
        }
        // SAFETY: affects the calling thread only, with constant arguments.
        unsafe {
            pthread_set_qos_class_self_np(class, 0);
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = class;
}

/// Keeps macOS from napping the app for as long as it runs. App Nap takes an app whose windows
/// aren't in front for one that's idle: every thread of it ran at the lowest priority there is
/// (4, where the agents' work runs at 31 and up) and its timers were put off, so a phone or
/// another Mac waited on it whenever this Mac was busy, and the bar's one-second timers ran a
/// second late. The Mac can still sleep when idle.
pub fn no_app_nap() {
    #[cfg(target_os = "macos")]
    {
        use objc2_foundation::{NSActivityOptions, NSProcessInfo, NSString};
        let reason = NSString::from_str("Codealong keeps your board live and answers your other devices");
        let activity = NSProcessInfo::processInfo().beginActivityWithOptions_reason(NSActivityOptions::UserInitiatedAllowingIdleSystemSleep, &reason);
        // Held until the app quits: ending it lets macOS nap the app again.
        std::mem::forget(activity);
    }
}

/// What a program printed, whether or not it succeeded.
pub struct Output {
    pub ok: bool,
    pub out: String,
    pub err: String,
}

/// `run`, for a program whose output is worth reading when it fails: `gh api graphql` exits
/// with an error when any part of a query failed, and still prints the parts that didn't.
pub fn run_output(program: &str, args: &[&str], cwd: Option<&Path>, timeout: Duration) -> Result<Output> {
    output_with_path(program, args, cwd, timeout, user_path(), false)
}

fn run_with_path(program: &str, args: &[&str], cwd: Option<&Path>, timeout: Duration, path: &str, background: bool) -> Result<String> {
    let o = output_with_path(program, args, cwd, timeout, path, background)?;
    if o.ok {
        Ok(o.out)
    } else {
        Err(anyhow!("{} {}", program, o.err.trim()))
    }
}

fn output_with_path(program: &str, args: &[&str], cwd: Option<&Path>, timeout: Duration, path: &str, background: bool) -> Result<Output> {
    let mut cmd = Command::new(program);
    if background {
        use std::os::unix::process::CommandExt;
        // SAFETY: runs in the child between fork and exec; setpriority is async-signal-safe
        // and touches only the child.
        unsafe {
            cmd.pre_exec(|| {
                libc::setpriority(libc::PRIO_PROCESS, 0, 10);
                Ok(())
            });
        }
    }
    cmd.args(args).env("PATH", path).env("GIT_OPTIONAL_LOCKS", "0").env("GIT_TERMINAL_PROMPT", "0");
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    drive(cmd, program, None, timeout)
}

/// Runs a program with input on stdin (used for AppleScript).
pub fn run_stdin(program: &str, args: &[&str], input: &str, timeout: Duration) -> Result<String> {
    #[cfg(test)]
    if let Some(answer) = stand_in::run(program, args, Some(input), timeout) {
        return answer;
    }
    let mut cmd = Command::new(program);
    cmd.args(args).env("PATH", user_path());
    let o = drive(cmd, program, Some(input.as_bytes()), timeout)?;
    if o.ok {
        Ok(o.out)
    } else {
        Err(anyhow!("{}", o.err.trim()))
    }
}

/// How long the output pipes are read after the program exits, for the last of what it wrote
/// to reach them. A program that leaves something running in the background hands it its
/// stdout, which then stays open as long as that runs; what the program itself printed is all
/// in the pipe by the time it exits, so this needn't be long.
const LINGER: Duration = Duration::from_millis(100);

/// Starts `cmd` and sees it through under one deadline: `input` goes in on stdin while stdout
/// and stderr are read, all without blocking, so a program that prints more than a pipe holds
/// before it reads its input, or prints a lot before it exits, never waits on Codealong. The
/// deadline covers the writing, the reading and the exit; once the program has exited its
/// pipes get `LINGER` more, whoever else holds them.
fn drive(cmd: Command, program: &str, input: Option<&[u8]>, timeout: Duration) -> Result<Output> {
    let raw = drive_raw(cmd, program, input, timeout)?;
    Ok(Output { ok: raw.ok, out: String::from_utf8_lossy(&raw.out).into_owned(), err: String::from_utf8_lossy(&raw.err).into_owned() })
}

/// What a program wrote, as it wrote it.
struct Raw {
    ok: bool,
    out: Vec<u8>,
    err: Vec<u8>,
}

/// Runs a program with the user's PATH and returns its stdout as the bytes it wrote, for what
/// isn't necessarily text (a file read from GitHub). Fails as `run` does.
pub fn run_bytes(program: &str, args: &[&str], timeout: Duration) -> Result<Vec<u8>> {
    #[cfg(test)]
    if let Some(answer) = stand_in::run(program, args, None, timeout) {
        return answer.map(String::into_bytes);
    }
    let mut cmd = Command::new(program);
    cmd.args(args).env("PATH", user_path());
    let raw = drive_raw(cmd, program, None, timeout)?;
    if raw.ok {
        Ok(raw.out)
    } else {
        Err(anyhow!("{} {}", program, String::from_utf8_lossy(&raw.err).trim()))
    }
}

fn drive_raw(mut cmd: Command, program: &str, input: Option<&[u8]>, timeout: Duration) -> Result<Raw> {
    use std::io::{ErrorKind, Write};
    use std::os::fd::AsRawFd;

    let deadline = Instant::now() + timeout;
    cmd.stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() }).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| anyhow!("couldn't start {program}: {e}"))?;
    let mut stdin = child.stdin.take().inspect(|w| nonblocking(w.as_raw_fd()));
    let mut pending = input.unwrap_or_default();
    let mut out = Drain::new(child.stdout.take());
    let mut err = Drain::new(child.stderr.take());
    let mut exited: Option<(std::process::ExitStatus, Instant)> = None;
    loop {
        if let Some(w) = stdin.as_mut() {
            loop {
                if pending.is_empty() {
                    // Closed, so the program sees the end of its input.
                    stdin = None;
                    break;
                }
                match w.write(pending) {
                    Ok(n) => pending = &pending[n..],
                    Err(e) if e.kind() == ErrorKind::Interrupted => {}
                    Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                    // The program closed its stdin without reading it all, which is its call.
                    Err(_) => {
                        stdin = None;
                        break;
                    }
                }
            }
        }
        if exited.is_none() {
            if let Some(status) = child.try_wait()? {
                exited = Some((status, Instant::now()));
                stdin = None;
            }
        }
        // Read after the exit is seen, so everything the program wrote is in the pipes by then.
        out.read();
        err.read();
        let now = Instant::now();
        match exited {
            Some((status, at)) => {
                if (!out.open() && !err.open()) || now >= (at + LINGER).min(deadline) {
                    return Ok(Raw { ok: status.success(), out: out.got, err: err.got });
                }
            }
            None if now >= deadline => {
                let _ = child.kill();
                // A process stuck in the kernel dies only once the kernel lets it go, minutes later
                // on a Mac that has stalled: it's reaped out of the way, so the timeout holds.
                std::thread::spawn(move || child.wait());
                return Err(anyhow!("{program} timed out after {}s", timeout.as_secs()));
            }
            None => {}
        }
        // Waits for a pipe to be ready, or a few milliseconds for the exit, which no pipe says.
        let mut fds: Vec<libc::pollfd> = [out.fd(), err.fd()].into_iter().flatten().map(|fd| libc::pollfd { fd, events: libc::POLLIN, revents: 0 }).collect();
        if let Some(w) = stdin.as_ref() {
            fds.push(libc::pollfd { fd: w.as_raw_fd(), events: libc::POLLOUT, revents: 0 });
        }
        let wait = deadline.saturating_duration_since(now).min(Duration::from_millis(5));
        // SAFETY: `fds` is a live array of pollfd of the length passed.
        unsafe {
            libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, wait.as_millis().max(1) as libc::c_int);
        }
    }
}

/// Makes reads and writes on `fd` answer at once, with `WouldBlock` where they'd wait.
pub fn nonblocking(fd: std::os::fd::RawFd) {
    // SAFETY: fcntl on a descriptor this process owns, changing only its status flags.
    unsafe {
        let flags = libc::fcntl(fd, libc::F_GETFL);
        if flags >= 0 {
            libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
        }
    }
}

/// One of a child's output pipes, read without blocking until it ends.
struct Drain<R> {
    pipe: Option<R>,
    got: Vec<u8>,
}

impl<R: Read + std::os::fd::AsRawFd> Drain<R> {
    fn new(pipe: Option<R>) -> Self {
        if let Some(p) = &pipe {
            nonblocking(p.as_raw_fd());
        }
        Drain { pipe, got: Vec::new() }
    }

    fn open(&self) -> bool {
        self.pipe.is_some()
    }

    fn fd(&self) -> Option<std::os::fd::RawFd> {
        self.pipe.as_ref().map(|p| p.as_raw_fd())
    }

    /// Takes what's in the pipe now. At most a megabyte a call, far more than a pipe holds, so
    /// something writing as fast as it's read can't keep the deadline from being looked at.
    fn read(&mut self) {
        let Some(p) = self.pipe.as_mut() else { return };
        let mut buf = [0u8; 16 * 1024];
        for _ in 0..64 {
            match p.read(&mut buf) {
                Ok(0) => {
                    self.pipe = None;
                    return;
                }
                Ok(n) => self.got.extend_from_slice(&buf[..n]),
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => return,
                Err(_) => {
                    self.pipe = None;
                    return;
                }
            }
        }
    }
}

/// A test's stand-in for the programs `run` and `run_stdin` start: while one is set on a thread,
/// a program run there is handed to it (its arguments, what goes in on stdin, how long it would
/// get) and its answer is what comes back, with nothing started. launch.rs's tests see what the
/// terminal apps would be sent this way (`launch/stage.rs`).
#[cfg(test)]
pub mod stand_in {
    use std::cell::RefCell;
    use std::time::Duration;

    use anyhow::Result;

    pub type Runner = Box<dyn FnMut(&str, &[&str], Option<&str>, Duration) -> Result<String>>;

    thread_local! {
        static RUNNER: RefCell<Option<Runner>> = const { RefCell::new(None) };
    }

    pub fn set(runner: Option<Runner>) {
        RUNNER.with(|r| *r.borrow_mut() = runner);
    }

    pub(super) fn run(program: &str, args: &[&str], input: Option<&str>, timeout: Duration) -> Option<Result<String>> {
        RUNNER.with(|r| r.borrow_mut().as_mut().map(|run| run(program, args, input, timeout)))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn reads_what_ps_lists_as_processes() {
        use super::{ps_lines, PsLine};
        let out = "  501 ttys004  /opt/homebrew/bin/codex resume 01a0e375 \n88123 ??       /Applications/Antigravity.app/Contents/Resources/bin/language_server --csrf x\nnot a process\n";
        assert_eq!(
            ps_lines(out),
            vec![
                PsLine { pid: 501, tty: "ttys004".into(), command: "/opt/homebrew/bin/codex resume 01a0e375".into() },
                PsLine { pid: 88123, tty: "??".into(), command: "/Applications/Antigravity.app/Contents/Resources/bin/language_server --csrf x".into() },
            ]
        );
    }

    #[test]
    fn a_command_tree_is_active_when_it_starts_processes_or_uses_cpu() {
        let tree = |cpu: &[(u32, u64)]| super::CommandTree { cpu: cpu.to_vec() };
        let before = tree(&[(300, 1), (301, 88)]);
        assert!(!tree(&[(300, 1), (301, 90)]).active_since(&before));
        assert!(!tree(&[(300, 1)]).active_since(&before));
        assert!(tree(&[(300, 1), (301, 98)]).active_since(&before));
        assert!(tree(&[(300, 1), (301, 88), (310, 0)]).active_since(&before));
    }

    use super::*;

    #[test]
    fn own_process_is_alive_and_has_a_start_time() {
        let pid = std::process::id();
        assert!(pid_alive(pid));
        let start = proc_start_ms(pid).expect("start time");
        assert!(start > 1_700_000_000_000);
        assert!(!pid_alive(0));
    }

    #[test]
    fn listeners_are_found() {
        let v4 = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let v6 = std::net::TcpListener::bind("[::1]:0").unwrap();
        let any = std::net::TcpListener::bind("0.0.0.0:0").unwrap();
        let found = listeners();
        let mine = |port: u16| found.iter().find(|l| l.pid == std::process::id() && l.port == port).cloned();
        let l = mine(v4.local_addr().unwrap().port()).expect("the IPv4 port");
        assert_eq!(l.addr, std::net::IpAddr::from([127, 0, 0, 1]));
        assert_eq!(l.cwd, std::env::current_dir().unwrap());
        assert!(!l.name.is_empty());
        assert!(l.started_ms > 1_700_000_000_000);
        assert_eq!(mine(v6.local_addr().unwrap().port()).expect("the IPv6 port").addr, std::net::IpAddr::from(std::net::Ipv6Addr::LOCALHOST));
        // Every address is reached through loopback.
        assert_eq!(mine(any.local_addr().unwrap().port()).expect("the port on every address").addr, std::net::IpAddr::from([127, 0, 0, 1]));
    }

    #[test]
    fn open_files_are_found() {
        let dir = std::env::temp_dir().join(format!("codealong-open-files-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("rollout-test.jsonl");
        let held = std::fs::File::create(&path).unwrap();
        // The kernel names a file by its real path: /private/var/folders/… for $TMPDIR.
        let real = std::fs::canonicalize(&path).unwrap();
        assert!(open_files(std::process::id()).unwrap().contains(&real));
        drop(held);
        assert!(!open_files(std::process::id()).unwrap().contains(&real));
        let _ = std::fs::remove_dir_all(&dir);
        // Launchd's files aren't yours to read.
        assert_eq!(open_files(1), None);
    }

    #[test]
    fn runs_commands_with_timeouts() {
        assert_eq!(run("/bin/echo", &["hi"], None, Duration::from_secs(2)).unwrap(), "hi\n");
        assert!(run("/bin/sleep", &["3"], None, Duration::from_millis(200)).is_err());
        assert!(run("/usr/bin/false", &[], None, Duration::from_secs(2)).is_err());
    }

    #[test]
    fn output_and_input_bigger_than_a_pipe_go_through_at_once() {
        // The shell prints 200 KB before it reads a byte of its 200 KB of input: neither fits
        // in a pipe, so the input has to go in while the output is read.
        let input = "x".repeat(200_000);
        let out = run_stdin("/bin/sh", &["-c", "head -c 200000 /dev/zero | tr '\\0' a; wc -c"], &input, Duration::from_secs(10)).unwrap();
        assert_eq!(out.len() - out.trim_start_matches('a').len(), 200_000);
        assert_eq!(out.trim_start_matches('a').trim(), "200000");
    }

    #[test]
    fn input_nobody_reads_is_cut_off_by_the_deadline() {
        let started = Instant::now();
        let e = run_stdin("/bin/sleep", &["10"], &"x".repeat(1 << 20), Duration::from_millis(300)).unwrap_err();
        assert!(e.to_string().contains("timed out"), "{e}");
        assert!(started.elapsed() < Duration::from_secs(3), "{:?}", started.elapsed());
    }

    #[test]
    fn something_left_running_with_the_output_doesnt_hold_the_answer() {
        // The background sleep keeps the shell's stdout and stderr open after the shell exits.
        let started = Instant::now();
        assert_eq!(run("/bin/sh", &["-c", "echo hi; sleep 10 &"], None, Duration::from_secs(8)).unwrap(), "hi\n");
        assert_eq!(run_stdin("/bin/sh", &["-c", "cat; sleep 10 &"], "typed", Duration::from_secs(8)).unwrap(), "typed");
        assert!(started.elapsed() < Duration::from_secs(3), "{:?}", started.elapsed());
    }
}
