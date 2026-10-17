//! Processes: liveness checks without spawning, the user's login-shell PATH, and a command
//! runner with a timeout so a stuck `git` or `gh` can never wedge the engine.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};

use crate::util::{home, now_ms};

// The kernel's process table (os/procs).
use crate::os::procs::listed;
pub use crate::os::procs::{cwd_of, environment, listeners, open_files, parent_pid, proc_start_ms, tty};

pub fn pid_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    // SAFETY: kill with signal 0 only checks for existence and permission.
    let r = unsafe { libc::kill(pid as libc::pid_t, 0) };
    r == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

/// A process's parent and name as `ps -c` shows them (its executable's file name, cut to 16
/// characters), for any process. None once it's gone.
pub fn parent_and_name(pid: u32) -> Option<(u32, String)> {
    listed(pid).map(|p| (p.ppid, p.name))
}

/// The process group in the foreground of the terminal a process runs in: the one Ctrl-C there
/// would interrupt. None for a process with no terminal, or once it's gone.
pub fn foreground_group(pid: u32) -> Option<u32> {
    let p = listed(pid)?;
    (p.tdev != u32::MAX && p.tpgid > 0).then_some(p.tpgid as u32)
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
/// `git` and `gh` would otherwise not be found. A program run with it is started through
/// `command`.
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

/// A command that runs `program` with `path` as its PATH, started without forking Codealong.
/// Rust starts a program with `posix_spawn` unless the command changes PATH and names its
/// program without a folder, or runs code of its own before the program does (`pre_exec`):
/// then it forks, and the copy, which has all of the app's memory and none of its threads but
/// the one that forked, runs the program. On macOS 27 the allocator's own check in that copy
/// sometimes fails and kills it before the program starts (2026-10-16), and macOS reports a
/// crash of Codealong. So a name without a folder is looked up in `path` here, in the order
/// `exec` would look, and the program started from where it's found, still named as it was
/// given (its `argv[0]`). A name `path` doesn't have is looked up in the PATH Codealong was
/// started with, as a command that leaves PATH alone is, which for the app is a part of
/// `path`: it fails to start as it did.
pub fn command(program: impl AsRef<OsStr>, path: &str) -> Command {
    use std::os::unix::process::CommandExt;
    let program = program.as_ref();
    if program.as_encoded_bytes().contains(&b'/') {
        let mut cmd = Command::new(program);
        cmd.env("PATH", path);
        return cmd;
    }
    match found_in(program, path) {
        Some(at) => {
            let mut cmd = Command::new(at);
            cmd.arg0(program).env("PATH", path);
            cmd
        }
        None => Command::new(program),
    }
}

/// Where `exec` would find `program` in `path`: the first of its folders with an executable
/// file of that name. Folders named from wherever it runs (`.`, or none) are passed over.
fn found_in(program: &OsStr, path: &str) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    path.split(':')
        .filter(|dir| dir.starts_with('/'))
        .map(|dir| Path::new(dir).join(program))
        .find(|at| std::fs::metadata(at).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0))
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

/// The command trees of several sessions' processes, from one look at the process table
/// (`Listing`). A process whose CPU time `ps` didn't give isn't in them.
pub fn command_trees(listed: &[PsLine], pids: &[u32], root: fn(&str) -> bool) -> HashMap<u32, CommandTree> {
    let rows: Vec<(u32, u32, u64, &str)> = listed.iter().filter_map(|p| Some((p.pid, p.ppid, p.cpu?, p.command.as_str()))).collect();
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
    pub ppid: u32,
    /// Its terminal as `ps` names it (`ttys004`), or `??` for none.
    pub tty: String,
    /// The CPU time it has used, in hundredths of a second, when `ps` gave one that reads.
    pub cpu: Option<u64>,
    /// Its command line: `ps` joins the arguments with spaces.
    pub command: String,
}

/// The processes one `ps` listed, and when.
#[derive(Debug, Default, PartialEq)]
pub struct Listed {
    /// When `ps` began: a process that started since may not be in it, so what's written after
    /// is no sign that nothing runs it.
    pub at: i64,
    /// None when `ps` failed or ran out of time.
    pub lines: Vec<PsLine>,
}

/// Every process on this Mac, from one `ps` run, kept. The harnesses' readers each look for
/// their own processes in it, so a tick that asks them all (`harness::Look`) runs `ps` once.
/// The engine lists them off its thread (`list`), as `ps` takes seconds at a high load, and
/// hands every look the latest it has; one made empty runs `ps` the first time it's asked.
#[derive(Default)]
pub struct Listing(OnceLock<Arc<Listed>>);

impl Listing {
    /// A listing made already.
    pub fn of(listed: Arc<Listed>) -> Listing {
        Listing(OnceLock::from(listed))
    }

    pub fn all(&self) -> &[PsLine] {
        &self.0.get_or_init(|| Arc::new(list(Duration::from_secs(3)))).lines
    }

    /// When the processes were listed; none until they are.
    pub fn at(&self) -> Option<i64> {
        self.0.get().map(|l| l.at)
    }

    /// The listing, once it's made.
    pub fn made(&self) -> Option<Arc<Listed>> {
        self.0.get().cloned()
    }
}

/// Every process, from one `ps` run; none when it fails or `timeout` passes. Full command lines
/// (`-ww`), with each process's parent and CPU time for its command trees.
pub fn list(timeout: Duration) -> Listed {
    let at = now_ms();
    let lines = run("/bin/ps", &["-axww", "-o", "pid=,ppid=,tty=,time=,command="], None, timeout).map(|out| ps_lines(&out)).unwrap_or_default();
    Listed { at, lines }
}

/// `ps -o pid=,ppid=,tty=,time=,command=` as processes.
fn ps_lines(out: &str) -> Vec<PsLine> {
    out.lines()
        .filter_map(|line| {
            let mut rest = line.trim_start();
            let mut field = || {
                let (f, r) = rest.split_once(char::is_whitespace)?;
                rest = r.trim_start();
                Some(f)
            };
            let (pid, ppid, tty, time) = (field()?, field()?, field()?, field()?);
            Some(PsLine { pid: pid.parse().ok()?, ppid: ppid.parse().ok()?, tty: tty.to_string(), cpu: cpu_hundredths(time), command: rest.trim().to_string() })
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
    let mut cmd = command(program, user_path());
    cmd.args(args);
    for name in unset {
        cmd.env_remove(name);
    }
    let o = drive(cmd, program, None, timeout, false)?;
    if o.ok {
        Ok(o.out)
    } else {
        Err(anyhow!("{} {}", program, o.err.trim()))
    }
}

/// `run`, for work nobody is waiting on: the process gets a lower priority (nice 10), so
/// under load it yields to the agents and to the windows.
pub fn run_background(program: &str, args: &[&str], cwd: Option<&Path>, timeout: Duration) -> Result<String> {
    run_with_path(program, args, cwd, timeout, user_path(), true)
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
    let mut cmd = command(program, path);
    cmd.args(args).env("GIT_OPTIONAL_LOCKS", "0").env("GIT_TERMINAL_PROMPT", "0");
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    drive(cmd, program, None, timeout, background)
}

/// Runs a program in `cwd` with `env` added and `input` on stdin, as a repo's hook is run, and
/// returns what it printed whether or not it succeeded. Fails only when it can't start or
/// `timeout` passes.
pub fn run_in(program: &str, args: &[&str], cwd: &Path, env: &[(String, String)], input: &str, timeout: Duration) -> Result<Output> {
    #[cfg(test)]
    if let Some(answer) = stand_in::run(program, args, Some(input), timeout) {
        return answer.map(|out| Output { ok: true, out, err: String::new() });
    }
    let mut cmd = command(program, user_path());
    cmd.args(args).current_dir(cwd);
    for (name, value) in env {
        cmd.env(name, value);
    }
    drive(cmd, program, Some(input.as_bytes()), timeout, false)
}

/// Runs a program with input on stdin (used for AppleScript).
pub fn run_stdin(program: &str, args: &[&str], input: &str, timeout: Duration) -> Result<String> {
    #[cfg(test)]
    if let Some(answer) = stand_in::run(program, args, Some(input), timeout) {
        return answer;
    }
    let mut cmd = command(program, user_path());
    cmd.args(args);
    let o = drive(cmd, program, Some(input.as_bytes()), timeout, false)?;
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
/// pipes get `LINGER` more, whoever else holds them. A `background` program runs at a lower
/// priority (nice 10), so under load it yields to the agents and to the windows.
fn drive(cmd: Command, program: &str, input: Option<&[u8]>, timeout: Duration, background: bool) -> Result<Output> {
    let raw = drive_raw(cmd, program, input, timeout, background)?;
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
    let mut cmd = command(program, user_path());
    cmd.args(args);
    let raw = drive_raw(cmd, program, None, timeout, false)?;
    if raw.ok {
        Ok(raw.out)
    } else {
        Err(anyhow!("{} {}", program, String::from_utf8_lossy(&raw.err).trim()))
    }
}

fn drive_raw(mut cmd: Command, program: &str, input: Option<&[u8]>, timeout: Duration, background: bool) -> Result<Raw> {
    use std::io::{ErrorKind, Write};
    use std::os::fd::AsRawFd;

    let deadline = Instant::now() + timeout;
    cmd.stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() }).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| anyhow!("couldn't start {program}: {e}"))?;
    if background {
        // Lowered once it's started: lowering it in the child, before its program runs, takes a
        // fork (`command`). The moment between goes to loading the program.
        // SAFETY: setpriority on the process just started, which fails harmlessly if it's gone.
        unsafe {
            libc::setpriority(libc::PRIO_PROCESS, child.id() as libc::id_t, 10);
        }
    }
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
        let out = "  501   400 ttys004    0:01.50 /opt/homebrew/bin/codex resume 01a0e375 \n88123     1 ??       1:02:03.45 /Applications/Antigravity.app/Contents/Resources/bin/language_server --csrf x\n  700     1 ??       12-01:02:03 long  spaced   args\nnot a process\n";
        assert_eq!(
            ps_lines(out),
            vec![
                PsLine { pid: 501, ppid: 400, tty: "ttys004".into(), cpu: Some(150), command: "/opt/homebrew/bin/codex resume 01a0e375".into() },
                PsLine { pid: 88123, ppid: 1, tty: "??".into(), cpu: Some(372345), command: "/Applications/Antigravity.app/Contents/Resources/bin/language_server --csrf x".into() },
                // A time that doesn't read leaves the process listed, without its CPU.
                PsLine { pid: 700, ppid: 1, tty: "??".into(), cpu: None, command: "long  spaced   args".into() },
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

    /// The kernel shows no one but root the environment of Apple's own programs (`/bin/sleep`
    /// gave the arguments alone; `ps` is setuid root), so the process read is this test run again,
    /// waiting to be read.
    #[test]
    fn reads_a_running_process_environment_as_it_was_started() {
        const NAME: &str = "procs::tests::reads_a_running_process_environment_as_it_was_started";
        if std::env::var_os("CODEALONG_TEST_VAR").is_some() {
            std::thread::sleep(Duration::from_secs(10));
            return;
        }
        let mut child = Command::new(std::env::current_exe().unwrap()).args([NAME, "--exact"]).env("CODEALONG_TEST_VAR", "from the parent").stdout(Stdio::null()).stderr(Stdio::null()).spawn().expect("this test");
        // Linux lets `spawn` return a moment before the kernel has said where the new program's
        // environment is, and reads none, or the starter's, meanwhile.
        let started = Instant::now();
        let mut env = environment(child.id());
        while env.as_ref().is_some_and(|e| !e.contains_key("CODEALONG_TEST_VAR")) && started.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(20));
            env = environment(child.id());
        }
        let _ = child.kill();
        let _ = child.wait();
        assert_eq!(env.unwrap().get("CODEALONG_TEST_VAR").map(String::as_str), Some("from the parent"));
    }

    /// A process made the leader of a new session on a terminal of its own is that terminal's
    /// foreground group, as a shell in a tab is at its prompt.
    #[test]
    fn reads_the_foreground_group_of_a_process_terminal() {
        use std::os::unix::process::CommandExt;
        let (mut master, mut slave) = (0, 0);
        // SAFETY: openpty fills the two descriptors; the name, settings and size may be null.
        let opened = unsafe { libc::openpty(&mut master, &mut slave, std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut()) };
        assert_eq!(opened, 0, "a pseudo-terminal");
        let mut cmd = Command::new("/bin/sleep");
        cmd.arg("10").stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        // SAFETY: only async-signal-safe calls between fork and exec.
        unsafe {
            cmd.pre_exec(move || {
                libc::setsid();
                libc::ioctl(slave, libc::TIOCSCTTY as _, 0);
                Ok(())
            });
        }
        let mut child = cmd.spawn().expect("sleep");
        let started = Instant::now();
        let mut group = None;
        while started.elapsed() < Duration::from_secs(5) && group != Some(child.id()) {
            group = foreground_group(child.id());
            std::thread::sleep(Duration::from_millis(20));
        }
        let _ = child.kill();
        let _ = child.wait();
        // SAFETY: closing the descriptors openpty gave.
        unsafe {
            libc::close(master);
            libc::close(slave);
        }
        assert_eq!(group, Some(child.id()));
        assert_eq!(foreground_group(u32::MAX - 1), None, "no such process");
    }

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

    thread_local! {
        static FORKS: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
    }

    /// How many times this thread forks while `f` runs. `pthread_atfork`'s first handler runs in
    /// the thread that forks, before it does, so other tests' forks aren't counted.
    fn forks_in(f: impl FnOnce()) -> u32 {
        static COUNTING: std::sync::Once = std::sync::Once::new();
        extern "C" fn forking() {
            FORKS.with(|n| n.set(n.get() + 1));
        }
        // SAFETY: registers a handler that only counts, once for the test run.
        COUNTING.call_once(|| unsafe {
            libc::pthread_atfork(Some(forking as unsafe extern "C" fn()), None, None);
        });
        let before = FORKS.with(|n| n.get());
        f();
        FORKS.with(|n| n.get()) - before
    }

    #[test]
    fn starts_programs_without_forking() {
        let forks = forks_in(|| {
            // Named without a folder, found in the user's PATH, and still called by its name.
            assert_eq!(run("sh", &["-c", "echo $0"], None, Duration::from_secs(5)).unwrap(), "sh\n");
            assert_eq!(run_in("sh", &["-c", "echo $0 $X; cat"], Path::new("/"), &[("X".into(), "x".into())], "typed", Duration::from_secs(5)).unwrap().out, "sh x\ntyped");
            // In the background, at the lower priority it's given once it has started.
            assert_eq!(run_background("sh", &["-c", "sleep 0.3; ps -o nice= -p $$"], None, Duration::from_secs(5)).unwrap().trim(), "10");
            // A program that isn't there fails to start as before, which the GitHub view reads.
            let e = run("codealong-no-such-program", &[], None, Duration::from_secs(5)).unwrap_err().to_string();
            assert!(e.starts_with("couldn't start codealong-no-such-program: No such file or directory"), "{e}");
        });
        assert_eq!(forks, 0);
        // As a command that does fork is counted.
        let forked = forks_in(|| {
            use std::os::unix::process::CommandExt;
            let mut cmd = Command::new("/usr/bin/true");
            // SAFETY: nothing runs between the fork and the exec.
            unsafe {
                cmd.pre_exec(|| Ok(()));
            }
            assert!(cmd.status().unwrap().success());
        });
        assert_eq!(forked, 1);
    }

    #[test]
    fn finds_a_program_where_exec_would() {
        let dir = std::env::temp_dir().join(format!("codealong-found-in-{}", std::process::id()));
        let (plain, folder, runs) = (dir.join("plain"), dir.join("folder"), dir.join("runs"));
        for d in [&plain, &folder, &runs] {
            std::fs::create_dir_all(d).unwrap();
        }
        std::fs::write(plain.join("tool"), "").unwrap();
        std::fs::create_dir_all(folder.join("tool")).unwrap();
        std::fs::write(runs.join("tool"), "#!/bin/sh\n").unwrap();
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(runs.join("tool"), std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        // A file it couldn't run and a folder of that name are passed over, as is a folder
        // named from wherever it runs.
        let path = format!(".::/nowhere:{}:{}:{}", plain.display(), folder.display(), runs.display());
        assert_eq!(found_in(OsStr::new("tool"), &path), Some(runs.join("tool")));
        assert_eq!(found_in(OsStr::new("tool"), &format!("{}:{}", plain.display(), folder.display())), None);
        let _ = std::fs::remove_dir_all(&dir);
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
