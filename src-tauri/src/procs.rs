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
/// `git`, `gh` and `cursor` would otherwise not be found.
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

/// The command a Claude Code session's Bash tool is running: the `eval '<command>'` in the
/// arguments of its shell child. For saying what a stuck session is stuck on.
pub fn running_command(pid: u32) -> Option<String> {
    let kids = run("pgrep", &["-P", &pid.to_string()], None, Duration::from_secs(2)).ok()?;
    kids.split_whitespace()
        .find_map(|kid| run("ps", &["-ww", "-o", "command=", "-p", kid], None, Duration::from_secs(2)).ok().and_then(|c| eval_command(&c)))
}

/// What the commands a Claude Code session's Bash tool runs are made of right now: every
/// process under its shells (the `eval '…'` children of the session's process; its MCP servers
/// are left out) with the CPU each has used, in hundredths of a second. For telling a command
/// that polls or builds from one left sitting there.
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
pub fn command_trees(pids: &[u32]) -> HashMap<u32, CommandTree> {
    match run_background("ps", &["-axww", "-o", "pid=,ppid=,time=,command="], None, Duration::from_secs(4)) {
        Ok(table) => command_trees_in(&table, pids),
        Err(_) => HashMap::new(),
    }
}

fn command_trees_in(table: &str, pids: &[u32]) -> HashMap<u32, CommandTree> {
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
            let mut tree: Vec<(u32, u64)> = rows.iter().filter(|r| r.1 == session && r.3.contains("eval '")).map(|r| (r.0, r.2)).collect();
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

/// `zsh -c source … && eval 'gcloud auth login' < /dev/null && pwd -P >| …` → `gcloud auth login`.
pub fn eval_command(cmdline: &str) -> Option<String> {
    let (_, rest) = cmdline.split_once("eval '")?;
    let inner = rest.rsplit_once("' < /dev/null").map(|(c, _)| c).unwrap_or(rest);
    Some(inner.replace("'\\''", "'")).filter(|c| !c.trim().is_empty())
}

/// Runs a program with the user's PATH and returns stdout. Fails on a non-zero exit (with
/// stderr in the message) or when `timeout` passes.
pub fn run(program: &str, args: &[&str], cwd: Option<&Path>, timeout: Duration) -> Result<String> {
    run_with_path(program, args, cwd, timeout, user_path(), false)
}

/// `run`, for work nobody is waiting on: the process gets a lower priority (nice 10), so
/// under load it yields to the agents and to the windows.
pub fn run_background(program: &str, args: &[&str], cwd: Option<&Path>, timeout: Duration) -> Result<String> {
    run_with_path(program, args, cwd, timeout, user_path(), true)
}

/// Puts the calling thread in macOS's utility class, for the same reason.
pub fn utility_thread() {
    #[cfg(target_os = "macos")]
    {
        extern "C" {
            fn pthread_set_qos_class_self_np(qos_class: u32, relative_priority: i32) -> i32;
        }
        const QOS_CLASS_UTILITY: u32 = 0x11;
        // SAFETY: affects the calling thread only, with constant arguments.
        unsafe {
            pthread_set_qos_class_self_np(QOS_CLASS_UTILITY, 0);
        }
    }
}

fn run_with_path(program: &str, args: &[&str], cwd: Option<&Path>, timeout: Duration, path: &str, background: bool) -> Result<String> {
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
    cmd.args(args)
        .env("PATH", path)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    let mut child = cmd.spawn().map_err(|e| anyhow!("couldn't start {program}: {e}"))?;
    let mut stdout = child.stdout.take().expect("piped stdout");
    let mut stderr = child.stderr.take().expect("piped stderr");
    let out_reader = std::thread::spawn(move || {
        let mut s = Vec::new();
        let _ = stdout.read_to_end(&mut s);
        s
    });
    let err_reader = std::thread::spawn(move || {
        let mut s = Vec::new();
        let _ = stderr.read_to_end(&mut s);
        s
    });
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if started.elapsed() > timeout {
            let _ = child.kill();
            // A process stuck in the kernel dies only once the kernel lets it go, minutes later
            // on a Mac that has stalled: it's reaped out of the way, so the timeout holds.
            std::thread::spawn(move || child.wait());
            return Err(anyhow!("{program} timed out after {}s", timeout.as_secs()));
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    let out = String::from_utf8_lossy(&out_reader.join().unwrap_or_default()).into_owned();
    let err = String::from_utf8_lossy(&err_reader.join().unwrap_or_default()).into_owned();
    if status.success() {
        Ok(out)
    } else {
        Err(anyhow!("{} {}", program, err.trim()))
    }
}

/// Runs a program with input on stdin (used for AppleScript).
pub fn run_stdin(program: &str, args: &[&str], input: &str, timeout: Duration) -> Result<String> {
    use std::io::Write;
    let mut child = Command::new(program)
        .args(args)
        .env("PATH", user_path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| anyhow!("couldn't start {program}: {e}"))?;
    child.stdin.take().expect("piped stdin").write_all(input.as_bytes())?;
    let started = Instant::now();
    loop {
        if child.try_wait()?.is_some() {
            break;
        }
        if started.elapsed() > timeout {
            let _ = child.kill();
            std::thread::spawn(move || child.wait());
            return Err(anyhow!("{program} timed out"));
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let output = child.wait_with_output()?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Err(anyhow!("{}", String::from_utf8_lossy(&output.stderr).trim()))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_command_tree_is_the_shells_under_a_session_and_what_they_run() {
        let table = "\
  100     1   0:05.00 claude
  200   100   0:01.00 node mcp-server.js
  300   100   0:00.01 /bin/zsh -c source snap.sh && eval 'northwind login --host x | tail -5' < /dev/null && pwd -P
  301   300   0:00.88 node northwind login --host x
  302   300   0:00.00 tail -5
  303   301 1:02:03.45 helper
  400     1   0:00.10 other
";
        let trees = super::command_trees_in(table, &[100, 999]);
        assert_eq!(trees[&100].cpu, vec![(300, 1), (301, 88), (302, 0), (303, 372345)]);
        assert!(trees[&999].cpu.is_empty());
    }

    /// `CODEALONG_TEST_PIDS=<pid,…> cargo test command_trees_here -- --ignored --nocapture`
    /// prints what those Claude sessions' shells are running, looked at twice a minute apart.
    #[test]
    #[ignore]
    fn command_trees_here() {
        let pids: Vec<u32> = std::env::var("CODEALONG_TEST_PIDS").unwrap_or_default().split(',').filter_map(|p| p.parse().ok()).collect();
        let before = super::command_trees(&pids);
        std::thread::sleep(std::time::Duration::from_secs(60));
        for (pid, after) in super::command_trees(&pids) {
            println!("{pid}: {:?} → {:?}, active: {}", before[&pid].cpu, after.cpu, after.active_since(&before[&pid]));
        }
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

    #[test]
    fn reads_the_command_a_shell_is_running() {
        let line = "/bin/zsh -c source /Users/me/.claude2/shell-snapshots/snapshot-zsh-1.sh 2>/dev/null || true && eval 'gcloud auth login' < /dev/null && pwd -P >| /tmp/claude-57ad-cwd";
        assert_eq!(super::eval_command(line).as_deref(), Some("gcloud auth login"));
        let quoted = "zsh -c eval 'echo '\\''hi'\\''' < /dev/null && pwd -P";
        assert_eq!(super::eval_command(quoted).as_deref(), Some("echo 'hi'"));
        assert_eq!(super::eval_command("node server.js"), None);
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
    fn runs_commands_with_timeouts() {
        assert_eq!(run("/bin/echo", &["hi"], None, Duration::from_secs(2)).unwrap(), "hi\n");
        assert!(run("/bin/sleep", &["3"], None, Duration::from_millis(200)).is_err());
        assert!(run("/usr/bin/false", &[], None, Duration::from_secs(2)).is_err());
    }
}
