//! Processes: liveness checks without spawning, the user's login-shell PATH, and a command
//! runner with a timeout so a stuck `git` or `gh` can never wedge the engine.

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

/// Process start time in epoch milliseconds, read from the kernel.
pub fn proc_start_ms(pid: u32) -> Option<i64> {
    // SAFETY: proc_pidinfo fills a plain-old-data struct of the size we pass.
    unsafe {
        let mut info: libc::proc_bsdinfo = std::mem::zeroed();
        let size = std::mem::size_of::<libc::proc_bsdinfo>() as libc::c_int;
        let n = libc::proc_pidinfo(
            pid as libc::c_int,
            libc::PROC_PIDTBSDINFO,
            0,
            &mut info as *mut libc::proc_bsdinfo as *mut libc::c_void,
            size,
        );
        if n != size {
            return None;
        }
        Some(info.pbi_start_tvsec as i64 * 1000 + info.pbi_start_tvusec as i64 / 1000)
    }
}

/// A process's parent, from the kernel; None once it's gone.
pub fn parent_pid(pid: u32) -> Option<u32> {
    // SAFETY: proc_pidinfo fills a plain-old-data struct of the size we pass.
    unsafe {
        let mut info: libc::proc_bsdinfo = std::mem::zeroed();
        let size = std::mem::size_of::<libc::proc_bsdinfo>() as libc::c_int;
        let n = libc::proc_pidinfo(pid as libc::c_int, libc::PROC_PIDTBSDINFO, 0, &mut info as *mut libc::proc_bsdinfo as *mut libc::c_void, size);
        (n == size && info.pbi_ppid > 0).then_some(info.pbi_ppid)
    }
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
            let _ = child.wait();
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
