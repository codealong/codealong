//! The process table on Windows: a Toolhelp snapshot for each process's parent and name,
//! `GetProcessTimes` for its start and CPU, the TCP table for what listens, and a process's own
//! memory (its PEB's process parameters) for its folder and environment, which Windows keeps
//! nowhere else. A process has no terminal device there, and only yours (in your session) are
//! read, as macOS answers.

use std::collections::HashMap;
use std::ffi::c_void;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ACCESS_DENIED, FILETIME, HANDLE, INVALID_HANDLE_VALUE, NO_ERROR, STILL_ACTIVE};
use windows_sys::Win32::NetworkManagement::IpHelper::{GetExtendedTcpTable, MIB_TCP6ROW_OWNER_PID, MIB_TCPROW_OWNER_PID, TCP_TABLE_OWNER_PID_LISTENER};
use windows_sys::Win32::Networking::WinSock::{AF_INET, AF_INET6};
use windows_sys::Win32::System::Diagnostics::Debug::ReadProcessMemory;
use windows_sys::Win32::System::Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS};
use windows_sys::Win32::System::RemoteDesktop::ProcessIdToSessionId;
use windows_sys::Win32::System::Threading::{GetCurrentProcessId, GetExitCodeProcess, GetProcessTimes, IsWow64Process, OpenProcess, SetPriorityClass, TerminateProcess, BELOW_NORMAL_PRIORITY_CLASS, PROCESS_QUERY_INFORMATION, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SET_INFORMATION, PROCESS_TERMINATE, PROCESS_VM_READ};

use super::{Listed, Listener, Signal};

/// A process as the snapshot lists it.
#[derive(Clone)]
struct Entry {
    pid: u32,
    ppid: u32,
    name: String,
}

/// How long one snapshot answers for: a walk up a process's parents, or down its tree, asks
/// for many in a moment, and a snapshot of every process costs a millisecond or two.
const FRESH: Duration = Duration::from_millis(250);

fn snapshot() -> Arc<Vec<Entry>> {
    static LAST: Mutex<Option<(Instant, Arc<Vec<Entry>>)>> = Mutex::new(None);
    let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((at, list)) = last.as_ref().filter(|(at, _)| at.elapsed() < FRESH) {
        let _ = at;
        return list.clone();
    }
    let list = Arc::new(take_snapshot());
    *last = Some((Instant::now(), list.clone()));
    list
}

fn take_snapshot() -> Vec<Entry> {
    let mut out = Vec::new();
    // SAFETY: the snapshot handle is closed before returning; each entry is a struct with its
    // size set, which the calls fill.
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == INVALID_HANDLE_VALUE {
            return out;
        }
        let mut e: PROCESSENTRY32W = std::mem::zeroed();
        e.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut more = Process32FirstW(snap, &mut e) != 0;
        while more {
            let len = e.szExeFile.iter().position(|&c| c == 0).unwrap_or(e.szExeFile.len());
            let exe = String::from_utf16_lossy(&e.szExeFile[..len]);
            // `ps -c` names a program without its extension, as macOS and Linux have none.
            let name = exe.strip_suffix(".exe").or_else(|| exe.strip_suffix(".EXE")).unwrap_or(&exe).to_string();
            out.push(Entry { pid: e.th32ProcessID, ppid: e.th32ParentProcessID, name });
            more = Process32NextW(snap, &mut e) != 0;
        }
        CloseHandle(snap);
    }
    out
}

fn entry(pid: u32) -> Option<Entry> {
    snapshot().iter().find(|e| e.pid == pid).cloned()
}

/// A process opened to ask about, closed when dropped.
struct Process(HANDLE);

impl Process {
    fn open(pid: u32, access: u32) -> Option<Process> {
        // SAFETY: OpenProcess takes plain values; null is an error (gone, or not ours to open).
        let h = unsafe { OpenProcess(access, 0, pid) };
        // Built only once it's open: a Process drops by closing its handle.
        if h.is_null() {
            None
        } else {
            Some(Process(h))
        }
    }

    /// When it started, and the CPU it has used, its own and the kernel's for it, in 100 ns.
    fn times(&self) -> Option<(FILETIME, u64)> {
        let zero = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
        let (mut created, mut exited, mut kernel, mut user) = (zero, zero, zero, zero);
        // SAFETY: the handle is open; the four structs are filled by the call.
        let ok = unsafe { GetProcessTimes(self.0, &mut created, &mut exited, &mut kernel, &mut user) } != 0;
        ok.then(|| (created, ticks(kernel) + ticks(user)))
    }

    /// `len` bytes of its memory at `at`.
    fn read(&self, at: usize, len: usize) -> Option<Vec<u8>> {
        let mut buf = vec![0u8; len];
        let mut got = 0usize;
        // SAFETY: the buffer holds `len` bytes; the call says how many it copied.
        let ok = unsafe { ReadProcessMemory(self.0, at as *const c_void, buf.as_mut_ptr().cast(), len, &mut got) } != 0;
        (ok && got == len).then_some(buf)
    }

    fn read_usize(&self, at: usize) -> Option<usize> {
        Some(usize::from_ne_bytes(self.read(at, std::mem::size_of::<usize>())?.try_into().ok()?))
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        // SAFETY: the handle is ours and nothing uses it after this.
        unsafe { CloseHandle(self.0) };
    }
}

fn ticks(t: FILETIME) -> u64 {
    ((t.dwHighDateTime as u64) << 32) | t.dwLowDateTime as u64
}

/// 100 ns intervals from 1601 to 1970, where a FILETIME's count starts and an epoch's.
const EPOCH_TICKS: u64 = 116_444_736_000_000_000;

pub fn proc_start_ms(pid: u32) -> Option<i64> {
    let (created, _) = Process::open(pid, PROCESS_QUERY_LIMITED_INFORMATION)?.times()?;
    Some((ticks(created).checked_sub(EPOCH_TICKS)? / 10_000) as i64)
}

/// Its parent, when that's still the process that started it: Windows keeps the parent's pid
/// after the parent exits, and gives the pid to the next process, which started after its
/// "child".
pub fn parent_pid(pid: u32) -> Option<u32> {
    let ppid = entry(pid)?.ppid;
    if ppid == 0 {
        return None;
    }
    match (proc_start_ms(pid), proc_start_ms(ppid)) {
        (Some(child), Some(parent)) if parent > child => None,
        _ => Some(ppid),
    }
}

/// As macOS's process table has it, a process without a terminal on NODEV (all ones): none has
/// one on Windows.
pub(crate) fn listed(pid: u32) -> Option<Listed> {
    let e = entry(pid)?;
    Some(Listed { ppid: e.ppid, name: e.name, tdev: u32::MAX, tpgid: -1 })
}

/// Windows has no terminal devices: a console is no file a session is known by.
pub fn tty(_pid: u32) -> Option<PathBuf> {
    None
}

/// Whether a process runs in your session, which is how far "yours" is told on Windows without
/// reading each one's owner.
fn in_my_session(pid: u32) -> bool {
    static MINE: std::sync::OnceLock<Option<u32>> = std::sync::OnceLock::new();
    let session = |pid: u32| {
        let mut s = 0u32;
        // SAFETY: it writes the session's id into the one u32 it's given.
        (unsafe { ProcessIdToSessionId(pid, &mut s) } != 0).then_some(s)
    };
    // SAFETY: GetCurrentProcessId can't fail.
    let mine = *MINE.get_or_init(|| session(unsafe { GetCurrentProcessId() }));
    mine.is_some() && session(pid) == mine
}

/// A TCP table of one family's listening sockets, with their owners: its rows as bytes, sized
/// as the call asks.
fn tcp_table(family: u16) -> Vec<u8> {
    let mut len = 0u32;
    // SAFETY: the first call asks only for the size; the second fills a buffer of it, and is
    // asked again if the table grew in between.
    unsafe {
        GetExtendedTcpTable(std::ptr::null_mut(), &mut len, 0, family as u32, TCP_TABLE_OWNER_PID_LISTENER, 0);
        for _ in 0..3 {
            let mut buf = vec![0u8; len as usize + 1024];
            len = buf.len() as u32;
            if GetExtendedTcpTable(buf.as_mut_ptr().cast(), &mut len, 0, family as u32, TCP_TABLE_OWNER_PID_LISTENER, 0) == NO_ERROR {
                buf.truncate(len as usize);
                return buf;
            }
        }
    }
    Vec::new()
}

/// The rows of a table the call wrote: a u32 count, then the rows, aligned as `T` is.
fn rows<T: Copy>(table: &[u8]) -> Vec<T> {
    if table.len() < 4 {
        return Vec::new();
    }
    let n = u32::from_ne_bytes(table[..4].try_into().unwrap()) as usize;
    let start = 4usize.next_multiple_of(std::mem::align_of::<T>());
    (0..n)
        .filter_map(|i| {
            let at = start + i * std::mem::size_of::<T>();
            let bytes = table.get(at..at + std::mem::size_of::<T>())?;
            // SAFETY: the bytes are a whole row of the table's kind, read unaligned.
            Some(unsafe { std::ptr::read_unaligned(bytes.as_ptr().cast::<T>()) })
        })
        .collect()
}

pub fn listeners() -> Vec<Listener> {
    let mut by_pid: HashMap<u32, Vec<(u16, std::net::IpAddr)>> = HashMap::new();
    let mut add = |pid: u32, port: u16, addr: std::net::IpAddr| {
        let ports = by_pid.entry(pid).or_default();
        match ports.iter_mut().find(|(p, _)| *p == port) {
            // Listening on both families: IPv4 is reached the same way everywhere.
            Some(have) if have.1.is_ipv6() && addr.is_ipv4() => have.1 = addr,
            Some(_) => {}
            None => ports.push((port, addr)),
        }
    };
    for r in rows::<MIB_TCPROW_OWNER_PID>(&tcp_table(AF_INET)) {
        let a = std::net::Ipv4Addr::from(u32::from_be(r.dwLocalAddr));
        let a = if a.is_unspecified() { std::net::Ipv4Addr::LOCALHOST } else { a };
        add(r.dwOwningPid, u16::from_be(r.dwLocalPort as u16), a.into());
    }
    for r in rows::<MIB_TCP6ROW_OWNER_PID>(&tcp_table(AF_INET6)) {
        let a = std::net::Ipv6Addr::from(r.ucLocalAddr);
        let a = if a.is_unspecified() { std::net::Ipv6Addr::LOCALHOST } else { a };
        add(r.dwOwningPid, u16::from_be(r.dwLocalPort as u16), a.into());
    }
    let mut out = Vec::new();
    for (pid, ports) in by_pid {
        if pid == 0 || !in_my_session(pid) {
            continue;
        }
        let name = entry(pid).map(|e| e.name).unwrap_or_default();
        let cwd = cwd_of(pid).unwrap_or_default();
        let started_ms = proc_start_ms(pid).unwrap_or(0);
        out.extend(ports.into_iter().map(|(port, addr)| Listener { pid, port, addr, name: name.clone(), cwd: cwd.clone(), started_ms }));
    }
    out
}

/// A process and every one under it, with the CPU they've used: each one's children from the
/// snapshot (those that started after it, `parent_pid`'s rule), and its CPU from
/// `GetProcessTimes`. None when the process has gone.
pub fn tree(pid: u32) -> Option<super::Tree> {
    let list = snapshot();
    let mut pids = vec![pid];
    let mut i = 0;
    while i < pids.len() && pids.len() < super::TREE_MOST {
        let parent = pids[i];
        let started = proc_start_ms(parent);
        for e in list.iter().filter(|e| e.ppid == parent && e.pid != parent) {
            let after = match (started, proc_start_ms(e.pid)) {
                (Some(p), Some(c)) => c >= p,
                _ => true,
            };
            if after && !pids.contains(&e.pid) {
                pids.push(e.pid);
            }
        }
        i += 1;
    }
    pids.truncate(super::TREE_MOST);
    let mut cpu: u64 = 0;
    for (at, &p) in pids.iter().enumerate() {
        match Process::open(p, PROCESS_QUERY_LIMITED_INFORMATION).and_then(|h| h.times()) {
            Some((_, t)) => cpu = cpu.saturating_add(t),
            None if at == 0 => return None,
            // One that ended between the listing and now: the next look won't list it.
            None => {}
        }
    }
    Some(super::Tree { pids, cpu_ns: cpu.saturating_mul(100) })
}

/// Windows lists a process's handles only to a debugger's way of asking: not read.
pub fn open_kinds(_pid: u32) -> Option<super::Kinds> {
    None
}

/// Nor the files it has open.
pub fn open_files(_pid: u32) -> Option<Vec<PathBuf>> {
    None
}

/// What `NtQueryInformationProcess` says of a process: where its PEB is.
#[repr(C)]
struct BasicInformation {
    exit_status: i32,
    peb: usize,
    affinity: usize,
    priority: i32,
    pid: usize,
    ppid: usize,
}

#[link(name = "ntdll")]
extern "system" {
    fn NtQueryInformationProcess(process: HANDLE, class: u32, info: *mut c_void, len: u32, returned: *mut u32) -> i32;
}

/// Where a 64-bit process's PEB keeps its process parameters, and where those keep its folder
/// (a UNICODE_STRING), its environment block and the block's size. Windows doesn't document
/// these, but every 64-bit Windows since 8 has them here, x64 and ARM64 alike.
const PEB_PARAMETERS: usize = 0x20;
const PARAMETERS_CWD: usize = 0x38;
const PARAMETERS_COMMAND_LINE: usize = 0x70;
const PARAMETERS_ENVIRONMENT: usize = 0x80;
const PARAMETERS_ENVIRONMENT_SIZE: usize = 0x3F0;

/// A process's parameters, opened to read: none for a 32-bit process, whose PEB is laid out
/// otherwise.
fn parameters(pid: u32) -> Option<(Process, usize)> {
    let p = Process::open(pid, PROCESS_QUERY_INFORMATION | PROCESS_VM_READ)?;
    let mut wow = 0;
    // SAFETY: the handle is open; it writes one BOOL.
    if unsafe { IsWow64Process(p.0, &mut wow) } == 0 || wow != 0 {
        return None;
    }
    // SAFETY: class 0 is ProcessBasicInformation, which fills the struct it's given.
    let mut info: BasicInformation = unsafe { std::mem::zeroed() };
    let status = unsafe { NtQueryInformationProcess(p.0, 0, (&mut info as *mut BasicInformation).cast(), std::mem::size_of::<BasicInformation>() as u32, std::ptr::null_mut()) };
    if status < 0 || info.peb == 0 {
        return None;
    }
    let params = p.read_usize(info.peb + PEB_PARAMETERS)?;
    Some((p, params))
}

/// A UNICODE_STRING in another process: its length in bytes, then (aligned) where its text is.
fn unicode_string(p: &Process, at: usize) -> Option<String> {
    let len = u16::from_ne_bytes(p.read(at, 2)?.try_into().ok()?) as usize;
    let text = p.read_usize(at + 8)?;
    let bytes = p.read(text, len)?;
    let wide: Vec<u16> = bytes.chunks_exact(2).map(|c| u16::from_ne_bytes([c[0], c[1]])).collect();
    Some(String::from_utf16_lossy(&wide))
}

pub fn cwd_of(pid: u32) -> Option<PathBuf> {
    let (p, params) = parameters(pid)?;
    let dir = unicode_string(&p, params + PARAMETERS_CWD)?;
    // `C:\work\` keeps its last backslash there; a drive's root keeps it.
    let dir = if dir.len() > 3 { dir.trim_end_matches('\\') } else { dir.as_str() };
    (!dir.is_empty()).then(|| PathBuf::from(dir))
}

/// A process's command line as it was started, which Windows keeps as one string.
pub fn command_line(pid: u32) -> Option<String> {
    let (p, params) = parameters(pid)?;
    unicode_string(&p, params + PARAMETERS_COMMAND_LINE).filter(|c| !c.is_empty())
}

/// Every process, as `ps` would list them: the snapshot's, each with its times and its command
/// line, or its program's name where Windows won't let it be read (another user's, an elevated
/// one's).
pub fn table() -> Option<Vec<super::Row>> {
    let rows = snapshot()
        .iter()
        .filter(|e| e.pid != 0)
        .map(|e| {
            let times = Process::open(e.pid, PROCESS_QUERY_LIMITED_INFORMATION).and_then(|p| p.times());
            super::Row {
                pid: e.pid,
                ppid: e.ppid,
                cpu_ms: times.map(|(_, cpu)| cpu / 10_000),
                started_ms: times.and_then(|(created, _)| ticks(created).checked_sub(EPOCH_TICKS)).map(|t| (t / 10_000) as i64),
                command: command_line(e.pid).unwrap_or_else(|| e.name.clone()),
            }
        })
        .collect();
    Some(rows)
}

pub fn environment(pid: u32) -> Option<HashMap<String, String>> {
    let (p, params) = parameters(pid)?;
    let block = p.read_usize(params + PARAMETERS_ENVIRONMENT)?;
    let size = p.read_usize(params + PARAMETERS_ENVIRONMENT_SIZE)?.min(1 << 20);
    let bytes = p.read(block, size)?;
    let wide: Vec<u16> = bytes.chunks_exact(2).map(|c| u16::from_ne_bytes([c[0], c[1]])).collect();
    Some(environ(&wide))
}

/// An environment block: `NAME=value` wide strings, each ending in a NUL, the block in another.
/// Windows' own entries for each drive's folder (`=C:=C:\work`) start with `=`, and aren't
/// variables.
fn environ(wide: &[u16]) -> HashMap<String, String> {
    wide.split(|&c| c == 0)
        .take_while(|v| !v.is_empty())
        .filter_map(|v| {
            let s = String::from_utf16_lossy(v);
            let (k, v) = s.split_once('=')?;
            (!k.is_empty()).then(|| (k.to_string(), v.to_string()))
        })
        .collect()
}

/// Windows can't become another program in place: a process starts a new one.
pub fn exec_keeping(_exe: &std::path::Path, _args: &[std::ffi::OsString], _env: &[(std::ffi::OsString, std::ffi::OsString)], _keep: &[super::Fd]) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::Unsupported, "Windows can't replace a running program in place")
}

/// Whether a process is there, yours or not: one Windows won't open for you is someone's.
pub fn alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    let Some(p) = Process::open(pid, PROCESS_QUERY_LIMITED_INFORMATION) else {
        // SAFETY: reads this thread's last error, which the failed open just set.
        return unsafe { GetLastError() } == ERROR_ACCESS_DENIED;
    };
    let mut code = 0u32;
    // SAFETY: the handle is open; it writes one u32. A process that has exited has its exit
    // code there, and one still running STILL_ACTIVE.
    (unsafe { GetExitCodeProcess(p.0, &mut code) }) != 0 && code == STILL_ACTIVE as u32
}

/// Tells the process `pid`: those that end it end it at once, as SIGKILL does.
pub fn signal(pid: u32, sig: Signal) -> std::io::Result<()> {
    match sig {
        Signal::Continue => Ok(()),
        Signal::Interrupt => Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "Windows can't interrupt another console's program")),
        Signal::HangUp | Signal::Terminate | Signal::Kill => {
            let p = Process::open(pid, PROCESS_TERMINATE).ok_or_else(std::io::Error::last_os_error)?;
            // SAFETY: the handle is open with the right to end it.
            match unsafe { TerminateProcess(p.0, 1) } {
                0 => Err(std::io::Error::last_os_error()),
                _ => Ok(()),
            }
        }
    }
}

/// Windows has no process groups: what a process started is its tree, which is told from the
/// last started first, so none is left without its parent for long.
pub fn signal_group(group: u32, sig: Signal) -> std::io::Result<()> {
    let pids = tree(group).map(|t| t.pids).unwrap_or_else(|| vec![group]);
    let mut first = Ok(());
    for pid in pids.into_iter().rev() {
        if let Err(e) = signal(pid, sig) {
            if pid == group {
                first = Err(e);
            }
        }
    }
    first
}

/// The group that `signal_group` tells: the process itself, whose tree it is.
pub fn group_of(pid: u32) -> Option<u32> {
    alive(pid).then_some(pid)
}

/// A program on Windows is told its name in its command line, which `Command` writes from the
/// program as given.
pub fn set_arg0(_cmd: &mut std::process::Command, _name: &std::ffi::OsStr) {}

/// Whether a file runs: Windows runs a file by its extension, which a name found with
/// `names_to_find` has.
pub fn may_run(at: &std::path::Path) -> bool {
    at.is_file()
}

/// A program's names to look for in a folder of a PATH: as given when it has an extension, else
/// with each extension Windows runs (`PATHEXT`: `.COM;.EXE;.BAT;.CMD` and the rest), so that a
/// `.cmd` npm installs for a command (`claude.cmd`) is found as `claude`.
pub fn names_to_find(program: &std::ffi::OsStr) -> Vec<std::ffi::OsString> {
    if std::path::Path::new(program).extension().is_some() {
        return vec![program.to_os_string()];
    }
    let exts = std::env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into());
    exts.split(';')
        .filter(|e| !e.is_empty())
        .map(|e| {
            let mut name = program.to_os_string();
            name.push(e.to_ascii_lowercase());
            name
        })
        .collect()
}

/// Lowers a process just started below the app's priority, so under load it yields to the
/// agents and to the windows.
pub fn lower_priority(pid: u32) {
    if let Some(p) = Process::open(pid, PROCESS_SET_INFORMATION) {
        // SAFETY: the handle is open with the right to change its priority.
        unsafe { SetPriorityClass(p.0, BELOW_NORMAL_PRIORITY_CLASS) };
    }
}

/// An anonymous pipe on Windows can't answer at once instead of waiting (std takes a pipe set
/// not to wait for a broken one), so writes to one wait until the child reads.
pub fn nonblocking<P>(_pipe: &P) {}

/// Never reached on Windows, where writes wait (`nonblocking`): a moment's sleep.
pub fn wait_writable<P>(_pipe: &P, wait: Duration) {
    std::thread::sleep(wait);
}

/// A child's stdin, stdout and stderr, driven without the caller's thread waiting on them: a
/// thread of its own writes its input and one reads each output, handing what it reads over a
/// channel, since Windows' anonymous pipes can't be read or written without waiting. A process
/// the child left behind can hold its pipes open past it, so the threads are stopped as the
/// command is done with (`Drop`).
pub struct Pipes {
    got: std::sync::mpsc::Receiver<(bool, Option<Vec<u8>>)>,
    out: (Vec<u8>, bool),
    err: (Vec<u8>, bool),
    workers: Option<Workers>,
}

/// The threads that read and write a child's pipes, and what stops them. std reads and writes a
/// child's pipes as overlapped I/O, which only `CancelIoEx` on the pipe's handle cancels: copies
/// of the handles are kept for that, which keep the pipes open, so a number is never another's by
/// the time it's cancelled. The input's copy goes as soon as the input is written, since the child
/// reads the end of its input only once every handle that writes to it is closed.
struct Workers {
    threads: Vec<std::thread::JoinHandle<()>>,
    reads: Vec<std::os::windows::io::OwnedHandle>,
    input: std::sync::Arc<std::sync::Mutex<Option<std::os::windows::io::OwnedHandle>>>,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl Workers {
    fn done(&self) -> bool {
        self.threads.iter().all(|t| t.is_finished())
    }

    /// Cancels whatever the threads wait on now.
    fn cancel(&self) {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::System::IO::CancelIoEx;
        let input = self.input.lock().unwrap_or_else(|e| e.into_inner());
        for h in self.reads.iter().chain(input.as_ref()) {
            // SAFETY: a handle of this struct's own, open while it's held (the input's while its
            // lock is); a pipe with nothing waiting on it just answers there was nothing to cancel.
            unsafe { CancelIoEx(h.as_raw_handle(), std::ptr::null()) };
        }
    }
}

impl Pipes {
    /// Takes the child's pipes, with `input` to write to it, each to a thread of its own.
    pub fn new(child: &mut std::process::Child, input: &[u8]) -> Pipes {
        use std::io::{Read, Write};
        use std::os::windows::io::AsHandle;
        use std::sync::atomic::Ordering;
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let mut threads = Vec::new();
        let copy = |pipe: &dyn AsHandle| pipe.as_handle().try_clone_to_owned().ok();
        let reads: Vec<_> = [child.stdout.as_ref().and_then(|p| copy(p)), child.stderr.as_ref().and_then(|p| copy(p))].into_iter().flatten().collect();
        let held = std::sync::Arc::new(std::sync::Mutex::new(child.stdin.as_ref().and_then(|p| copy(p))));
        if let Some(mut stdin) = child.stdin.take() {
            let (input, held) = (input.to_vec(), held.clone());
            // Closed once it's all written, its copy first, so the program sees the end of its
            // input; a program that closes it without reading it all ends the write, which is its
            // call.
            threads.push(std::thread::spawn(move || {
                let _ = stdin.write_all(&input);
                held.lock().unwrap_or_else(|e| e.into_inner()).take();
            }));
        }
        let (tx, got) = std::sync::mpsc::channel();
        let reader = |mut pipe: Box<dyn Read + Send>, is_err: bool, tx: std::sync::mpsc::Sender<(bool, Option<Vec<u8>>)>| {
            let stop = stop.clone();
            std::thread::spawn(move || {
                let mut buf = [0u8; 16 * 1024];
                while !stop.load(Ordering::SeqCst) {
                    match pipe.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            if tx.send((is_err, Some(buf[..n].to_vec()))).is_err() {
                                return;
                            }
                        }
                    }
                }
                let _ = tx.send((is_err, None));
            })
        };
        let out = child.stdout.take().map(|p| threads.push(reader(Box::new(p), false, tx.clone()))).is_some();
        let err = child.stderr.take().map(|p| threads.push(reader(Box::new(p), true, tx))).is_some();
        Pipes { got, out: (Vec::new(), out), err: (Vec::new(), err), workers: Some(Workers { threads, reads, input: held, stop }) }
    }

    /// The input goes on its thread.
    pub fn write(&mut self) {}

    /// The input's thread ends as the child does, or as this is dropped.
    pub fn close_input(&mut self) {}

    fn take(&mut self, (is_err, chunk): (bool, Option<Vec<u8>>)) {
        let (got, open) = if is_err { &mut self.err } else { &mut self.out };
        match chunk {
            Some(c) => got.extend_from_slice(&c),
            None => *open = false,
        }
    }

    /// Takes what the readers have read by now.
    pub fn read(&mut self) {
        while let Ok(m) = self.got.try_recv() {
            self.take(m);
        }
    }

    /// Whether either of its outputs is still open.
    pub fn open(&self) -> bool {
        self.out.1 || self.err.1
    }

    /// Waits up to `wait` for a reader to have read something.
    pub fn wait(&mut self, wait: Duration) {
        if !self.open() {
            std::thread::sleep(wait);
        } else if let Ok(m) = self.got.recv_timeout(wait) {
            self.take(m);
        }
    }

    /// What it wrote to stdout and stderr.
    pub fn output(mut self) -> (Vec<u8>, Vec<u8>) {
        (std::mem::take(&mut self.out.0), std::mem::take(&mut self.err.0))
    }
}

impl Drop for Pipes {
    /// Stops the threads still waiting on a pipe, as when a process the child left behind holds
    /// one: each is told to stop and what it waits on is cancelled. A thread can start another
    /// read or write just after that, so one still going is left to a thread of its own, which
    /// cancels again every 50 ms until each has ended. A command whose pipes all closed has none.
    fn drop(&mut self) {
        let Some(workers) = self.workers.take() else { return };
        workers.stop.store(true, std::sync::atomic::Ordering::SeqCst);
        if workers.done() {
            return;
        }
        std::thread::spawn(move || {
            while !workers.done() {
                workers.cancel();
                std::thread::sleep(Duration::from_millis(50));
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_an_environment_block_without_the_drives_folders() {
        let block: Vec<u16> = "=C:=C:\\work\0HOME=C:\\Users\\me\0EMPTY=\0EQ=a=b\0\0".encode_utf16().collect();
        let env = environ(&block);
        assert_eq!(env.len(), 3);
        assert_eq!(env["HOME"], "C:\\Users\\me");
        assert_eq!(env["EMPTY"], "");
        assert_eq!(env["EQ"], "a=b");
    }

    #[test]
    fn reads_this_processs_own_start_folder_and_environment() {
        let me = std::process::id();
        let started = proc_start_ms(me).unwrap();
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64;
        assert!(started <= now && now - started < 3_600_000, "{started} {now}");
        assert_eq!(cwd_of(me).unwrap(), std::env::current_dir().unwrap());
        assert_eq!(environment(me).unwrap().get("PATH").cloned(), std::env::var("PATH").ok());
        let tree = tree(me).unwrap();
        assert_eq!(tree.pids[0], me);
    }
}
