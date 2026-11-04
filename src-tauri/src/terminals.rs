//! Codealong's own terminals: a shell on a pseudo-terminal of Codealong's, run by the server for
//! a session and shown in the drawer under its box (src/ui/terminal/). Unlike a tab of your
//! terminal app, Codealong holds the screen itself, so what a command printed is read exactly,
//! a full-screen program included, and the windows, the command line and an agent all see the
//! same terminal.
//!
//! Each terminal is a thread (`run`) that owns its emulator (screen.rs: Ghostty's, whose types
//! may not leave the thread that made them) and takes everything that reaches the terminal as a
//! message (`Msg`): what its process printed, read by a thread of its own (`read`); the process
//! ending, waited on by another (`wait`); a window's input and size. What goes to the process is
//! written by a third (`write`), so a program that doesn't read its input never holds up the
//! screen.
//!
//! Screens go to the windows as frames (screen.rs `Frame`), the event `terminal-frame`, only
//! while a window shows the terminal: a window that shows one says so (`watch`) and says it again
//! every few seconds while it does, so a window that went without saying stops getting them. A
//! frame goes out at most every `FRAME_EVERY` while the program prints, and at once after a quiet
//! spell, so typing is echoed without waiting. The board lists each session's terminals
//! (`glances`), which the windows show as tabs.
//!
//! The terminals outlive the server's build: a server replaced by a new build of the same app
//! hands them over (`hand_over`, serve.rs `hand_over`). It becomes the new build in place, so its
//! processes stay its children and nothing is told, and the new build takes each terminal back
//! from the descriptor it inherited and a snapshot of its screen (`take_over`). A server that ends
//! any other way (a crash, a quit) ends their shells.

mod keys;
pub mod pty;
pub mod screen;

use std::collections::{BTreeMap, HashMap};
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, OwnedFd};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

pub use pty::Size;
pub use screen::{Frame, Input, Theme};

use crate::util::now_ms;

/// How often a terminal sends a frame while its program prints.
const FRAME_EVERY: Duration = Duration::from_millis(16);
/// How long a window's word that it shows a terminal holds: it says it again before then.
const WATCH_FOR: Duration = Duration::from_secs(20);
/// How often what's in front on a terminal is looked at.
const LOOK_EVERY: Duration = Duration::from_secs(1);
/// How long a hung-up process has to end before it's killed.
const HANG_UP_WAIT: Duration = Duration::from_secs(3);
/// The size a terminal starts at when the window opening it hasn't measured.
pub const DEFAULT_SIZE: Size = Size { cols: 100, rows: 24, cell_width: 8, cell_height: 17 };

/// A terminal as the board lists it.
#[derive(Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Glance {
    pub id: String,
    /// The session it's for.
    pub session: String,
    /// The title its program set (OSC 0 or 2), if any.
    pub title: String,
    /// The shell's name (`zsh`), or a command's first word.
    pub shell: String,
    /// The command it runs, when it was opened for one rather than for a shell.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    /// Where it started, or where its shell last said it is (OSC 7).
    pub folder: String,
    pub started_at: i64,
    /// Its process hasn't ended.
    pub running: bool,
    /// How its process ended: its status, or 128 and the signal that ended it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit: Option<i32>,
    /// The program in front, when it isn't the shell itself (`vim`, `npm`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub program: Option<String>,
    /// When its program last rang the bell.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bell_at: Option<i64>,
    /// Its device (`/dev/ttys012`): a session whose process has it runs here.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tty: String,
    /// The session's own process runs in it: it's where the session lives, opened to start or
    /// resume it in Codealong's terminal (launch/codealong.rs), not a terminal beside it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hosts: bool,
}

/// What a terminal's thread is told.
enum Msg {
    Output(Vec<u8>),
    /// The terminal's other side closed: nothing more will be printed.
    Hangup,
    /// The process ended, with its status.
    Exited(i32),
    Input(Vec<Input>),
    Paste { text: String, unsafe_ok: bool, reply: Sender<bool> },
    Resize(Size),
    Theme(Theme),
    Watch { window: String, on: bool },
    Copy(Sender<Option<String>>),
    Text(Sender<String>),
    /// Bytes for the program as they are, as if typed: a message sent to the session in it, a key
    /// that answers its prompt.
    Write(Vec<u8>),
    /// The server is handing its terminals to the next (`hand_over`): the terminal as it's handed,
    /// with its screen's snapshot, after which its thread ends and leaves its process be.
    HandOver(Sender<Option<(Handed, Option<Vec<u8>>)>>),
    Close,
}

struct Entry {
    glance: Glance,
    tx: Sender<Msg>,
    /// Stops its reader (`read`): as it closes, or as it's handed over.
    stop: Arc<AtomicBool>,
}

/// A terminal as one server hands it to the next build (`hand_over`, `take_over`): what the board
/// lists of it, its side of the terminal as a descriptor the next build inherits, its process,
/// and its screen's snapshot in a file beside Codealong's.
#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Handed {
    pub glance: Glance,
    pub fd: i32,
    pub pid: u32,
    pub size: Size,
    #[serde(default)]
    pub theme: Option<Theme>,
    pub close_on_exit: bool,
    #[serde(default)]
    pub snapshot: Option<PathBuf>,
}

#[derive(Default)]
struct State {
    /// In the order they were opened.
    list: Vec<Entry>,
}

fn state() -> MutexGuard<'static, State> {
    static STATE: OnceLock<Mutex<State>> = OnceLock::new();
    STATE.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner())
}

type FrameHook = Box<dyn Fn(&Frame) + Send + Sync>;
type ChangedHook = Box<dyn Fn() + Send + Sync>;
static FRAMES: OnceLock<FrameHook> = OnceLock::new();
static CHANGED: OnceLock<ChangedHook> = OnceLock::new();
static LIVING: OnceLock<PathBuf> = OnceLock::new();

/// What the server does with a terminal's frames (to the windows), and when what the board lists
/// of the terminals changed (the board again); and the file that keeps which sessions live in
/// them (`living`).
pub fn init(frames: impl Fn(&Frame) + Send + Sync + 'static, changed: impl Fn() + Send + Sync + 'static, living: PathBuf) {
    let _ = FRAMES.set(Box::new(frames));
    let _ = CHANGED.set(Box::new(changed));
    let _ = LIVING.set(living);
}

fn changed() {
    if let Some(f) = CHANGED.get() {
        f();
    }
    note_living();
}

/// The sessions whose processes run in a terminal of Codealong's, kept in a file as they change,
/// so the next server knows which ended with this one (`living_before`). Written only when the
/// list changes.
#[derive(Serialize, Deserialize, Default, PartialEq, Clone, Debug)]
pub struct Living {
    pub sessions: Vec<String>,
}

fn note_living() {
    static LAST: Mutex<Option<Living>> = Mutex::new(None);
    let Some(path) = LIVING.get() else { return };
    let mut now = Living { sessions: state().list.iter().filter(|e| e.glance.hosts && e.glance.running && !e.glance.session.is_empty()).map(|e| e.glance.session.clone()).collect() };
    now.sessions.sort();
    let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
    if last.as_ref() == Some(&now) {
        return;
    }
    if crate::store::save(path, &now).is_ok() {
        *last = Some(now);
    }
}

/// The sessions that lived in the last server's terminals, as it last wrote them: read once as a
/// server starts, before it takes over or opens any.
pub fn living_before(path: &Path) -> Vec<String> {
    crate::store::read::<Living>(path, "the sessions in Codealong's terminals").ok().flatten().map(|l| l.sessions).unwrap_or_default()
}

/// Of the sessions that lived in the last server's terminals, those whose terminal didn't come
/// across to this one (`take_over`): they ended with that server.
pub fn ended_with_server(before: &[String]) -> Vec<String> {
    let now: Vec<String> = state().list.iter().filter(|e| e.glance.hosts).map(|e| e.glance.session.clone()).collect();
    before.iter().filter(|k| !now.contains(k)).cloned().collect()
}

/// Every terminal, by the key of the session it's for, in the order they were opened.
pub fn glances() -> BTreeMap<String, Vec<Glance>> {
    let mut by: BTreeMap<String, Vec<Glance>> = BTreeMap::new();
    for e in &state().list {
        by.entry(e.glance.session.clone()).or_default().push(e.glance.clone());
    }
    by
}

/// One terminal's glance.
#[cfg(test)]
pub fn glance(id: &str) -> Option<Glance> {
    state().list.iter().find(|e| e.glance.id == id).map(|e| e.glance.clone())
}

/// What to open: a shell, or `command` run in the shell, in `folder`, for `session`.
#[derive(Default)]
pub struct Open {
    pub session: String,
    pub folder: PathBuf,
    pub command: Option<String>,
    /// What the board says it runs, where that isn't `command` itself (a run's wrapper runs the
    /// command you asked for).
    pub shown: Option<String>,
    /// Set in its environment, beside what a terminal app sets.
    pub env: Vec<(String, String)>,
    /// The size and colors it starts with: the last a window gave a terminal, without.
    pub size: Option<Size>,
    pub theme: Option<Theme>,
    /// It closes when its process ends, as a terminal app's tab does when its shell exits. One
    /// that doesn't stays, saying how its command ended, until it's closed.
    pub close_on_exit: bool,
    /// Typed into it as it starts, before its shell reads anything, as Ghostty's `initial input`
    /// is: the line that starts or resumes a session in it.
    pub input: Option<String>,
}

/// The size and colors a window last gave a terminal, for one opened without a window's.
fn last_given() -> MutexGuard<'static, (Size, Option<Theme>)> {
    static GIVEN: OnceLock<Mutex<(Size, Option<Theme>)>> = OnceLock::new();
    GIVEN.get_or_init(|| Mutex::new((DEFAULT_SIZE, None))).lock().unwrap_or_else(|e| e.into_inner())
}

/// Opens a terminal, its shell started in its folder.
pub fn open(o: Open) -> Result<Glance, String> {
    if !o.folder.is_dir() {
        return Err(format!("{} isn't there any more.", crate::util::tilde(&o.folder)));
    }
    let shell = crate::launch::login_shell();
    let shell_name = Path::new(&shell).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "sh".into());
    // A login shell, as a terminal app starts one, which reads your profile and finds your PATH;
    // a command runs in an interactive one, so your aliases and functions are there too.
    let args: Vec<String> = match &o.command {
        Some(c) => vec!["-i".into(), "-c".into(), c.clone()],
        None => vec![],
    };
    let arg0 = format!("-{shell_name}");
    let mut env = environment();
    env.retain(|(k, _)| !o.env.iter().any(|(set, _)| set == k));
    env.extend(o.env.iter().cloned());
    let (size, theme) = {
        let given = last_given();
        (sane(o.size.unwrap_or(given.0)), o.theme.or_else(|| given.1.clone()))
    };
    let opened = pty::Pty::open(&pty::Spawn { program: Path::new(&shell), arg0: &arg0, args: &args, cwd: &o.folder, env: &env }, size).map_err(|e| format!("Couldn't start {shell_name}: {e}"))?;
    let id = new_id();
    let shown = o.shown.or(o.command);
    let glance = Glance {
        id: id.clone(),
        session: o.session,
        title: String::new(),
        shell: shown.as_deref().and_then(|c| c.split_whitespace().next()).map(|w| w.rsplit('/').next().unwrap_or(w).to_string()).unwrap_or(shell_name),
        command: shown,
        folder: o.folder.to_string_lossy().into_owned(),
        started_at: now_ms(),
        running: true,
        exit: None,
        program: None,
        bell_at: None,
        tty: opened.tty.clone(),
        hosts: false,
    };
    let pid = opened.pid();
    let pty::Pty { master, mut child, .. } = opened;
    let started = Started { glance, pid, master, size, theme, close_on_exit: o.close_on_exit, input: o.input, snapshot: None };
    spawn(started, move || child.wait().map(|s| exit_code(&s)).unwrap_or(-1))
}

/// A terminal's side and process, and how its thread starts: a new screen, or the one another
/// server handed over (`snapshot`).
struct Started {
    glance: Glance,
    pid: u32,
    master: OwnedFd,
    size: Size,
    theme: Option<Theme>,
    close_on_exit: bool,
    input: Option<String>,
    snapshot: Option<Vec<u8>>,
}

/// Starts a terminal's threads and lists it: its process waited on with `wait` (reaped however
/// the rest goes), its output read, its input written, and its screen run.
fn spawn(st: Started, wait: impl FnOnce() -> i32 + Send + 'static) -> Result<Glance, String> {
    let Started { glance, pid, master, size, theme, close_on_exit, input, snapshot } = st;
    let id = glance.id.clone();
    let (tx, rx) = mpsc::channel();
    let fds = master.try_clone().and_then(|r| master.try_clone().map(|w| (r, w)));
    let (reader, writer) = match fds {
        Ok(fds) => fds,
        Err(e) => {
            pty::hang_up(pid, true);
            reap(pid);
            return Err(format!("Couldn't start {}: {e}", glance.shell));
        }
    };
    {
        let tx = tx.clone();
        let waiting = std::thread::Builder::new().name(format!("terminal-wait-{id}")).spawn(move || {
            let _ = tx.send(Msg::Exited(wait()));
        });
        if let Err(e) = waiting {
            pty::hang_up(pid, true);
            reap(pid);
            return Err(format!("Couldn't start {}: {e}", glance.shell));
        }
    }
    let stop = Arc::new(AtomicBool::new(false));
    // Listed before the rest starts, so one whose process ends at once finds itself to close.
    state().list.push(Entry { glance: glance.clone(), tx: tx.clone(), stop: stop.clone() });
    let started = (|| -> std::io::Result<()> {
        {
            let (tx, stop) = (tx.clone(), stop.clone());
            std::thread::Builder::new().name(format!("terminal-read-{id}")).spawn(move || read(reader, tx, stop))?;
        }
        let (out_tx, out_rx) = mpsc::channel::<Vec<u8>>();
        if let Some(input) = input {
            // Waits in the terminal until the shell reads it.
            let _ = out_tx.send(input.into_bytes());
        }
        std::thread::Builder::new().name(format!("terminal-write-{id}")).spawn(move || write(writer, out_rx))?;
        let (id, stop) = (id.clone(), stop.clone());
        std::thread::Builder::new().name(format!("terminal-{id}")).spawn(move || {
            let how = How { size, theme, close_on_exit, snapshot };
            if let Err(e) = run(&id, pid, master, how, rx, out_tx, stop.clone()) {
                eprintln!("Terminal {id} stopped: {e}");
                abandon(&id, pid, &stop);
            }
        })?;
        Ok(())
    })();
    if let Err(e) = started {
        abandon(&id, pid, &stop);
        return Err(format!("Couldn't start {}: {e}", glance.shell));
    }
    changed();
    Ok(glance)
}

/// Reaps a process of this server's that won't be waited on otherwise.
fn reap(pid: u32) {
    // SAFETY: waitpid on a child of this process; one already reaped answers an error.
    unsafe { libc::waitpid(pid as libc::pid_t, std::ptr::null_mut(), 0) };
}

/// Waits for a process this server took over to end (`take_over`): its child since before the
/// server became this build, so waited on by its pid. One reaped before then (it ended as the
/// server changed) is waited on until it's gone, its status unknown.
fn wait_pid(pid: u32) -> i32 {
    loop {
        let mut status: libc::c_int = 0;
        // SAFETY: waitpid fills a status for a child of this process.
        let r = unsafe { libc::waitpid(pid as libc::pid_t, &mut status, 0) };
        if r == pid as libc::pid_t {
            if libc::WIFEXITED(status) {
                return libc::WEXITSTATUS(status);
            }
            if libc::WIFSIGNALED(status) {
                return 128 + libc::WTERMSIG(status);
            }
            continue;
        }
        if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
            continue;
        }
        while crate::procs::pid_alive(pid) {
            std::thread::sleep(Duration::from_millis(500));
        }
        return -1;
    }
}

/// Hands every running terminal to the next build of the server, which this process is about to
/// become (serve.rs `hand_over`): each reader stops first, so all it read is in its terminal's
/// queue ahead of the question, then each terminal's thread answers with the terminal, its side
/// as a descriptor that outlives the change (`pty::inheritable`), and its screen's snapshot,
/// written to a file in `dir`, and ends, leaving its process running. One that ended or doesn't
/// answer isn't handed over.
pub fn hand_over(dir: &Path) -> Vec<Handed> {
    hand_over_these(dir, |_| true)
}

/// `hand_over` for the terminals `pick` picks.
fn hand_over_these(dir: &Path, pick: impl Fn(&Glance) -> bool) -> Vec<Handed> {
    let running: Vec<(String, Sender<Msg>, Arc<AtomicBool>)> = state().list.iter().filter(|e| e.glance.running && pick(&e.glance)).map(|e| (e.glance.id.clone(), e.tx.clone(), e.stop.clone())).collect();
    if running.is_empty() {
        return Vec::new();
    }
    for (_, _, stop) in &running {
        stop.store(true, Ordering::Relaxed);
    }
    // A reader looks at its flag at least this often (`read`).
    std::thread::sleep(READ_POLL + Duration::from_millis(150));
    let _ = std::fs::create_dir_all(dir);
    running
        .into_iter()
        .filter_map(|(id, tx, _)| {
            let (reply, rx) = mpsc::channel();
            tx.send(Msg::HandOver(reply)).ok()?;
            let (mut handed, snapshot) = rx.recv_timeout(Duration::from_secs(5)).ok()??;
            if let Some(bytes) = snapshot {
                let path = dir.join(format!("{id}.snap"));
                if std::fs::write(&path, bytes).is_ok() {
                    handed.snapshot = Some(path);
                }
            }
            Some(handed)
        })
        .collect()
}

/// Whether any terminal's process is running, which a server replaced by a new build hands over.
pub fn any_running() -> bool {
    state().list.iter().any(|e| e.glance.running)
}

/// Takes back the terminals the server handed over as it became this build (`hand_over`): each
/// from the descriptor this process inherited, its screen from its snapshot (or drawn again by
/// its program, told its size changed, where the snapshot can't be read), its process waited on
/// by its pid. Each snapshot's file goes once it's read. How many it took.
pub fn take_over(handed: Vec<Handed>) -> usize {
    use std::os::fd::FromRawFd;
    let mut taken = 0;
    for h in handed {
        if h.fd < 0 {
            continue;
        }
        // SAFETY: the descriptor was handed down by the server this process was before it became
        // this build, and nothing else in this process has it.
        let master = unsafe { OwnedFd::from_raw_fd(h.fd) };
        // Not for the processes this build starts.
        let _ = pty::cloexec(master.as_raw_fd());
        let snapshot = h.snapshot.as_ref().and_then(|p| {
            let bytes = std::fs::read(p).ok();
            let _ = std::fs::remove_file(p);
            bytes
        });
        let pid = h.pid;
        let id = h.glance.id.clone();
        let started = Started { glance: h.glance, pid, master, size: sane(h.size), theme: h.theme, close_on_exit: h.close_on_exit, input: None, snapshot };
        match spawn(started, move || wait_pid(pid)) {
            Ok(_) => taken += 1,
            Err(e) => eprintln!("Couldn't take terminal {id} over: {e}"),
        }
    }
    taken
}

/// Undoes a terminal that didn't get going: no longer listed, its reader stopped, and its process
/// killed, which the thread waiting on it reaps.
fn abandon(id: &str, pid: u32, closed: &AtomicBool) {
    closed.store(true, Ordering::Relaxed);
    pty::hang_up(pid, true);
    let listed = {
        let mut st = state();
        let before = st.list.len();
        st.list.retain(|e| e.glance.id != id);
        st.list.len() != before
    };
    if listed {
        changed();
    }
}

fn exit_code(s: &std::process::ExitStatus) -> i32 {
    use std::os::unix::process::ExitStatusExt;
    s.code().unwrap_or_else(|| 128 + s.signal().unwrap_or(0))
}

/// A size no terminal is too small or too large to take.
fn sane(s: Size) -> Size {
    Size { cols: s.cols.clamp(2, 1000), rows: s.rows.clamp(1, 500), cell_width: s.cell_width.clamp(1, 200), cell_height: s.cell_height.clamp(1, 400) }
}

fn new_id() -> String {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    format!("term-{}-{}", now_ms(), NEXT.fetch_add(1, Ordering::Relaxed))
}

/// The environment a terminal's shell starts with: the server's, which is what the Dock gives an
/// app (install.rs `server_environment`), less what's an agent session's, with what a terminal
/// app sets.
fn environment() -> Vec<(String, String)> {
    let mut env: Vec<(String, String)> = std::env::vars().filter(|(k, _)| !crate::install::of_a_session(k)).filter(|(k, _)| !matches!(k.as_str(), "TERM" | "COLORTERM" | "TERM_PROGRAM" | "TERM_PROGRAM_VERSION")).collect();
    env.push(("TERM".into(), screen::TERM.into()));
    env.push(("COLORTERM".into(), "truecolor".into()));
    env.push(("TERM_PROGRAM".into(), "Codealong".into()));
    env.push(("TERM_PROGRAM_VERSION".into(), env!("CARGO_PKG_VERSION").into()));
    // An app the Dock opens has no locale set, where a terminal app sets one: without it a shell
    // takes text as ASCII and garbles anything else typed or printed.
    if !env.iter().any(|(k, v)| matches!(k.as_str(), "LANG" | "LC_ALL" | "LC_CTYPE") && !v.is_empty()) {
        env.push(("LANG".into(), "en_US.UTF-8".into()));
    }
    env
}

fn send(id: &str, msg: Msg) -> Result<(), String> {
    let tx = state().list.iter().find(|e| e.glance.id == id).map(|e| e.tx.clone()).ok_or_else(|| "That terminal has closed.".to_string())?;
    tx.send(msg).map_err(|_| "That terminal has closed.".to_string())
}

/// A window shows the terminal (`on`), and is sent a whole frame; or no longer does.
pub fn watch(id: &str, window: &str, on: bool) -> Result<(), String> {
    send(id, Msg::Watch { window: window.to_string(), on })
}

/// Input from a window, in the order it came.
pub fn input(id: &str, inputs: Vec<Input>) -> Result<(), String> {
    send(id, Msg::Input(inputs))
}

/// Pastes text: false when it could run commands as it's pasted and `unsafe_ok` wasn't given.
pub fn paste(id: &str, text: String, unsafe_ok: bool) -> Result<bool, String> {
    let (reply, rx) = mpsc::channel();
    send(id, Msg::Paste { text, unsafe_ok, reply })?;
    rx.recv_timeout(Duration::from_secs(5)).map_err(|_| "The terminal didn't answer.".to_string())
}

/// A window's size for a terminal, which one opened without a window's then starts at.
pub fn resize(id: &str, size: Size) -> Result<(), String> {
    let size = sane(size);
    last_given().0 = size;
    send(id, Msg::Resize(size))
}

/// A window's colors for a terminal, which one opened without a window's then starts in.
pub fn theme(id: &str, theme: Theme) -> Result<(), String> {
    last_given().1 = Some(theme.clone());
    send(id, Msg::Theme(theme))
}

/// The selection's text.
pub fn copy(id: &str) -> Result<Option<String>, String> {
    let (reply, rx) = mpsc::channel();
    send(id, Msg::Copy(reply))?;
    rx.recv_timeout(Duration::from_secs(5)).map_err(|_| "The terminal didn't answer.".to_string())
}

/// Everything the terminal holds as text, scrollback included.
pub fn text(id: &str) -> Result<String, String> {
    let (reply, rx) = mpsc::channel();
    send(id, Msg::Text(reply))?;
    rx.recv_timeout(Duration::from_secs(5)).map_err(|_| "The terminal didn't answer.".to_string())
}

/// Writes bytes for the program as they are, as if typed.
pub fn write_to(id: &str, bytes: Vec<u8>) -> Result<(), String> {
    send(id, Msg::Write(bytes))
}

/// Whether there's a terminal at all: until there is, nothing needs looking up.
pub fn any() -> bool {
    !state().list.is_empty()
}

/// The terminal whose device is `tty`, which a process there has as its terminal.
pub fn on_tty(tty: &str) -> Option<Glance> {
    if tty.is_empty() {
        return None;
    }
    state().list.iter().find(|e| e.glance.tty == tty).map(|e| e.glance.clone())
}

/// Every terminal, in the order they were opened.
pub fn list() -> Vec<Glance> {
    state().list.iter().map(|e| e.glance.clone()).collect()
}

/// A session's own process runs in terminal `id` (found by its device, `engine.rs`): the
/// terminal is the session's, where it lives, and goes where its terminals go.
pub fn adopt(id: &str, key: &str) {
    update(id, |g| {
        g.session = key.to_string();
        g.hosts = true;
    });
}

/// Closes a terminal: its process is hung up on, as closing a tab does, and killed if it hasn't
/// ended a few seconds later.
pub fn close(id: &str) -> Result<(), String> {
    let entry = {
        let mut st = state();
        let at = st.list.iter().position(|e| e.glance.id == id).ok_or_else(|| "That terminal has closed.".to_string())?;
        st.list.remove(at)
    };
    let _ = entry.tx.send(Msg::Close);
    changed();
    Ok(())
}

/// A session moved to another account goes on as a new session (start.rs `move_to`): its
/// terminals go on as the copy's.
pub fn follow(from: &str, to: &str) {
    let moved = {
        let mut st = state();
        let mut moved = false;
        for e in st.list.iter_mut().filter(|e| e.glance.session == from) {
            e.glance.session = to.to_string();
            moved = true;
        }
        moved
    };
    if moved {
        changed();
    }
}

/// Closes every terminal of a session.
pub fn close_session(key: &str) {
    let ids: Vec<String> = state().list.iter().filter(|e| e.glance.session == key).map(|e| e.glance.id.clone()).collect();
    for id in ids {
        let _ = close(&id);
    }
}

fn update(id: &str, f: impl FnOnce(&mut Glance)) {
    let differs = {
        let mut st = state();
        let Some(e) = st.list.iter_mut().find(|e| e.glance.id == id) else { return };
        let before = e.glance.clone();
        f(&mut e.glance);
        e.glance != before
    };
    if differs {
        changed();
    }
}

/// How often a reader looks at whether to stop.
const READ_POLL: Duration = Duration::from_millis(250);

/// Reads what the process prints, until the terminal's other side closes or the terminal does.
fn read(fd: OwnedFd, tx: Sender<Msg>, closed: Arc<AtomicBool>) {
    let raw = fd.as_raw_fd();
    let mut file = std::fs::File::from(fd);
    let mut buf = vec![0u8; 64 * 1024];
    while !closed.load(Ordering::Relaxed) {
        if !pty::readable(raw, READ_POLL.as_millis() as i32) {
            continue;
        }
        match file.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                if tx.send(Msg::Output(buf[..n].to_vec())).is_err() {
                    return;
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            // EIO once the other side has closed, on Linux.
            Err(_) => break,
        }
    }
    let _ = tx.send(Msg::Hangup);
}

/// Writes what goes to the process, in order, until the terminal goes.
fn write(fd: OwnedFd, rx: Receiver<Vec<u8>>) {
    let mut file = std::fs::File::from(fd);
    for bytes in rx {
        if file.write_all(&bytes).is_err() {
            return;
        }
    }
}

/// How a terminal's screen starts (`run`).
struct How {
    size: Size,
    theme: Option<Theme>,
    close_on_exit: bool,
    /// The screen another server handed over, as its snapshot.
    snapshot: Option<Vec<u8>>,
}

/// A terminal's thread: its emulator, fed and asked, and its frames sent while a window shows it.
fn run(id: &str, pid: u32, master: OwnedFd, how: How, rx: Receiver<Msg>, out: Sender<Vec<u8>>, closed: Arc<AtomicBool>) -> Result<(), String> {
    let How { size, theme, close_on_exit, snapshot } = how;
    let answers = out.clone();
    let answer = move |b: &[u8]| {
        let _ = answers.send(b.to_vec());
    };
    let handed = snapshot.is_some();
    let restored = snapshot.and_then(|bytes| match screen::Screen::restore(&bytes, size, answer.clone()) {
        Ok(s) => Some(s),
        Err(e) => {
            eprintln!("Terminal {id}'s screen couldn't be restored ({e:?}): its program draws it again.");
            None
        }
    });
    let restored_ok = restored.is_some();
    let mut screen = match restored {
        Some(s) => s,
        None => screen::Screen::new(size, answer).map_err(|e| format!("{e:?}"))?,
    };
    if handed && !restored_ok {
        pty::redraw(master.as_raw_fd());
    }
    let mut theme_now = theme.clone();
    if let Some(t) = &theme {
        screen.theme(t);
    }
    let mut watchers: HashMap<String, Instant> = HashMap::new();
    let mut full = true;
    let mut dirty = true;
    let mut last_frame = Instant::now() - FRAME_EVERY;
    let mut last_look = Instant::now();
    let mut running = true;
    loop {
        let now = Instant::now();
        let watched = watchers.values().any(|until| *until > now);
        // A screen held still (a redraw under way) is looked at again a frame later, not at once.
        let wait = if watched && (dirty || full) {
            if screen.held(now) { FRAME_EVERY } else { FRAME_EVERY.saturating_sub(last_frame.elapsed()) }
        } else {
            LOOK_EVERY
        };
        let first = match rx.recv_timeout(wait) {
            Ok(m) => Some(m),
            Err(RecvTimeoutError::Timeout) => None,
            Err(RecvTimeoutError::Disconnected) => break,
        };
        // Everything that's waiting, then a frame: a burst of output is drawn once.
        let mut msgs: Vec<Msg> = first.into_iter().collect();
        while msgs.len() < 256 {
            match rx.try_recv() {
                Ok(m) => msgs.push(m),
                Err(_) => break,
            }
        }
        for msg in msgs {
            match msg {
                Msg::Output(bytes) => {
                    screen.feed(&bytes);
                    dirty = true;
                }
                Msg::Hangup => {}
                Msg::Exited(code) => {
                    running = false;
                    dirty = true;
                    update(id, |g| {
                        g.running = false;
                        g.exit = Some(code);
                        g.program = None;
                    });
                    // Its Close comes back here as a message, after what's left of this batch.
                    if close_on_exit {
                        let _ = close(id);
                    }
                }
                Msg::Input(inputs) => {
                    for i in &inputs {
                        screen.input(i);
                    }
                    let bytes = screen.take_out();
                    if !bytes.is_empty() && running {
                        let _ = out.send(bytes);
                    }
                    dirty = true;
                }
                Msg::Paste { text, unsafe_ok, reply } => {
                    let pasted = running && screen.paste(&text, unsafe_ok);
                    let _ = reply.send(pasted);
                    dirty = true;
                }
                Msg::Resize(s) => {
                    if s != screen.size() {
                        screen.resize(s);
                        pty::resize(master.as_raw_fd(), s);
                        dirty = true;
                    }
                }
                Msg::Theme(t) => {
                    screen.theme(&t);
                    theme_now = Some(t);
                    full = true;
                }
                Msg::Watch { window, on } => {
                    if on {
                        watchers.insert(window, Instant::now() + WATCH_FOR);
                        full = true;
                    } else {
                        watchers.remove(&window);
                    }
                }
                Msg::Copy(reply) => {
                    let _ = reply.send(screen.selection_text());
                }
                Msg::Text(reply) => {
                    let _ = reply.send(screen.text());
                }
                Msg::HandOver(reply) => {
                    let glance = state().list.iter().find(|e| e.glance.id == id).map(|e| e.glance.clone());
                    let fd = if running { pty::inheritable(master.as_raw_fd()) } else { None };
                    let answer = glance.zip(fd).map(|(glance, fd)| (Handed { glance, fd, pid, size: screen.size(), theme: theme_now.clone(), close_on_exit, snapshot: None }, screen.snapshot()));
                    let _ = reply.send(answer);
                    // The next build has it now: this thread ends and leaves its process be.
                    return Ok(());
                }
                Msg::Write(bytes) => {
                    if running && !bytes.is_empty() {
                        screen.typed();
                        let _ = out.send(bytes);
                        dirty = true;
                    }
                }
                Msg::Close => {
                    closed.store(true, Ordering::Relaxed);
                    if running {
                        pty::hang_up(pid, false);
                        std::thread::spawn(move || {
                            std::thread::sleep(HANG_UP_WAIT);
                            if crate::procs::pid_alive(pid) {
                                pty::hang_up(pid, true);
                            }
                        });
                    }
                    return Ok(());
                }
            }
        }
        let noted = screen.take_noted();
        if noted.title || noted.bell || noted.pwd {
            let (title, pwd) = (screen.title(), screen.pwd());
            update(id, |g| {
                if noted.title {
                    g.title = title;
                }
                if noted.bell {
                    g.bell_at = Some(now_ms());
                }
                if noted.pwd {
                    if let Some(folder) = folder_of(&pwd) {
                        g.folder = folder;
                    }
                }
            });
        }
        if running && last_look.elapsed() >= LOOK_EVERY {
            last_look = Instant::now();
            let front = pty::foreground(master.as_raw_fd()).filter(|g| *g != pid);
            let program = front.and_then(|g| crate::procs::parent_and_name(g).map(|(_, name)| name));
            update(id, |g| g.program = program);
        }
        let now = Instant::now();
        watchers.retain(|_, until| *until > now);
        if !watchers.is_empty() && (dirty || full) && last_frame.elapsed() >= FRAME_EVERY && !screen.held(now) {
            if let Some(f) = screen.frame(id, full) {
                if let Some(hook) = FRAMES.get() {
                    hook(&f);
                }
            }
            full = false;
            dirty = false;
            last_frame = Instant::now();
        }
    }
    Ok(())
}

/// The folder an OSC 7 report names (`file://host/path`), when it's a folder here.
fn folder_of(pwd: &str) -> Option<String> {
    let path = pwd.strip_prefix("file://").map_or(pwd, |rest| rest.find('/').map_or("", |i| &rest[i..]));
    let path = urlencoding::decode(path).ok()?.into_owned();
    Path::new(&path).is_dir().then_some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wait_for(mut done: impl FnMut() -> bool) -> bool {
        let started = Instant::now();
        while started.elapsed() < Duration::from_secs(10) {
            if done() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(30));
        }
        false
    }

    /// A command runs in a terminal of its own, which keeps what it printed and how it ended.
    #[test]
    fn a_command_runs_in_its_own_terminal_and_its_end_is_listed() {
        let dir = std::env::temp_dir();
        let g = open(Open { session: "test:session:one".into(), folder: dir.clone(), command: Some("printf 'from the terminal\\n'; exit 3".into()), size: Some(DEFAULT_SIZE), ..Default::default() }).expect("a terminal");
        assert!(g.running);
        assert_eq!(g.command.as_deref(), Some("printf 'from the terminal\\n'; exit 3"));
        assert!(wait_for(|| glance(&g.id).is_some_and(|g| !g.running)), "it ends");
        assert_eq!(glance(&g.id).unwrap().exit, Some(3));
        assert!(wait_for(|| text(&g.id).is_ok_and(|t| t.contains("from the terminal"))), "{:?}", text(&g.id));
        assert!(glances().get("test:session:one").is_some_and(|l| l.iter().any(|t| t.id == g.id)));
        close(&g.id).unwrap();
        assert!(glance(&g.id).is_none());
        assert!(text(&g.id).is_err(), "closed");
    }

    /// Typing reaches the program, in the order it was typed.
    #[test]
    fn what_a_window_types_reaches_the_program() {
        let dir = std::env::temp_dir();
        let g = open(Open { session: "test:session:two".into(), folder: dir, command: Some("read line; echo \"got $line\"; sleep 1".into()), size: Some(DEFAULT_SIZE), ..Default::default() }).unwrap();
        watch(&g.id, "main", true).unwrap();
        for (code, text) in [("KeyH", "h"), ("KeyI", "i")] {
            input(&g.id, vec![Input::Key { code: code.into(), text: Some(text.into()), unshifted: None, mods: Default::default(), action: Default::default() }]).unwrap();
        }
        input(&g.id, vec![Input::Key { code: "Enter".into(), text: None, unshifted: None, mods: Default::default(), action: Default::default() }]).unwrap();
        assert!(wait_for(|| text(&g.id).is_ok_and(|t| t.contains("got hi"))), "{:?}", text(&g.id));
        close(&g.id).unwrap();
    }

    /// Closing a terminal hangs up on what runs in it.
    #[test]
    fn closing_a_terminal_ends_its_process() {
        let g = open(Open { session: "test:session:three".into(), folder: std::env::temp_dir(), command: Some("sleep 59.125".into()), size: Some(DEFAULT_SIZE), ..Default::default() }).unwrap();
        assert!(wait_for(|| glance(&g.id).is_some_and(|g| g.program.is_some() || g.running)));
        let pid_of = |name: &str| crate::procs::run("/usr/bin/pgrep", &["-f", name], None, Duration::from_secs(5)).ok().filter(|o| !o.trim().is_empty());
        assert!(wait_for(|| pid_of("sleep 59.125").is_some()), "it runs");
        close(&g.id).unwrap();
        assert!(wait_for(|| pid_of("sleep 59.125").is_none()), "it ended");
    }

    /// A session moved to another account takes its terminals with it; one archived closes them.
    #[test]
    fn a_sessions_terminals_follow_it_when_it_moves_and_close_with_it() {
        let g = open(Open { session: "test:moves:old".into(), folder: std::env::temp_dir(), size: Some(DEFAULT_SIZE), ..Default::default() }).unwrap();
        follow("test:moves:old", "test:moves:new");
        assert!(glances().get("test:moves:old").is_none());
        assert_eq!(glances().get("test:moves:new").map(|l| l.iter().map(|t| t.id.clone()).collect::<Vec<_>>()), Some(vec![g.id.clone()]));
        close_session("test:moves:new");
        assert!(glance(&g.id).is_none());
    }

    /// A terminal opened to start a session (launch/codealong.rs) is typed its line as it
    /// starts, is found by its device, takes bytes as if typed, and becomes the session's own once
    /// the session's process turns up in it.
    #[test]
    fn a_terminal_for_a_start_is_typed_its_line_and_found_by_its_device() {
        let g = open(Open { folder: std::env::temp_dir(), command: Some("read line; echo \"got $line\"; read more; echo \"then $more\"; sleep 1".into()), input: Some("first\n".into()), size: Some(DEFAULT_SIZE), ..Default::default() }).unwrap();
        assert!(g.session.is_empty() && !g.hosts, "no session's yet");
        assert!(g.tty.starts_with("/dev/"), "{:?}", g.tty);
        assert!(wait_for(|| text(&g.id).is_ok_and(|t| t.contains("got first"))), "{:?}", text(&g.id));
        assert_eq!(on_tty(&g.tty).map(|t| t.id), Some(g.id.clone()));
        assert!(on_tty("").is_none());
        write_to(&g.id, b"second\r".to_vec()).unwrap();
        assert!(wait_for(|| text(&g.id).is_ok_and(|t| t.contains("then second"))), "{:?}", text(&g.id));
        adopt(&g.id, "test:adopt:one");
        let mine = glances().get("test:adopt:one").cloned().unwrap_or_default();
        assert!(mine.iter().any(|t| t.id == g.id && t.hosts), "{mine:?}");
        close(&g.id).unwrap();
    }

    /// A terminal handed over (as a server does that becomes a new build) is taken back with its
    /// screen and goes on taking input, its process never having noticed.
    #[test]
    fn a_terminal_handed_over_is_taken_back_with_its_screen() {
        let g = open(Open { folder: std::env::temp_dir(), command: Some("read a; echo \"got:$a\"; read b; echo \"then:$b\"; sleep 30".into()), size: Some(DEFAULT_SIZE), ..Default::default() }).unwrap();
        write_to(&g.id, b"first\r".to_vec()).unwrap();
        assert!(wait_for(|| text(&g.id).is_ok_and(|t| t.contains("got:first"))), "{:?}", text(&g.id));
        let dir = std::env::temp_dir().join(format!("codealong-handover-test-{}", std::process::id()));
        let mut handed = hand_over_these(&dir, |t| t.id == g.id);
        assert_eq!(handed.len(), 1);
        assert!(handed[0].snapshot.as_ref().is_some_and(|p| p.exists()), "its screen is in a file");
        assert!(handed[0].fd >= 0);
        // As in the new build, where the old list is gone.
        state().list.retain(|e| e.glance.id != g.id);
        assert_eq!(take_over(std::mem::take(&mut handed)), 1);
        assert!(text(&g.id).is_ok_and(|t| t.contains("got:first")), "its screen came back: {:?}", text(&g.id));
        write_to(&g.id, b"second\r".to_vec()).unwrap();
        assert!(wait_for(|| text(&g.id).is_ok_and(|t| t.contains("then:second"))), "{:?}", text(&g.id));
        close(&g.id).unwrap();
        assert!(wait_for(|| glance(&g.id).is_none()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The sessions the last server's terminals held, as it wrote them, less those whose terminal
    /// came across, ended with it.
    #[test]
    fn sessions_whose_terminal_didnt_come_across_ended_with_the_server() {
        let dir = std::env::temp_dir().join(format!("codealong-living-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let file = dir.join("terminals.json");
        crate::store::save(&file, &Living { sessions: vec!["test:living:gone".into(), "test:living:kept".into()] }).unwrap();
        let before = living_before(&file);
        assert_eq!(before, ["test:living:gone", "test:living:kept"]);
        assert!(living_before(&dir.join("none.json")).is_empty());
        let g = open(Open { folder: std::env::temp_dir(), command: Some("sleep 5".into()), size: Some(DEFAULT_SIZE), ..Default::default() }).unwrap();
        adopt(&g.id, "test:living:kept");
        assert_eq!(ended_with_server(&before), ["test:living:gone"]);
        close(&g.id).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn reads_the_folder_a_shell_reports() {
        let dir = std::env::temp_dir().canonicalize().unwrap();
        let d = dir.to_string_lossy();
        assert_eq!(folder_of(&format!("file://somehost{d}")), Some(d.to_string()));
        assert_eq!(folder_of("file://somehost/no/such/folder/here"), None);
    }
}
