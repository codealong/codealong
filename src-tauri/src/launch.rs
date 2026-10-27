//! Opening things where they live: the terminals and the Claude and Codex deep links. Editors
//! are in editors.rs. See `[apps]` in README.md's Settings section.
//!
//! Each terminal is a module of its own behind `Terminal` (Ghostty, iTerm2 and Terminal through
//! AppleScript, Rex and tmux through their commands, and on Linux an emulator's window showing
//! tmux). What's
//! here is what they share: where a new terminal goes, the launcher that sets up a new tab's
//! shell, waiting for that shell, a process's TTY and which terminal it runs under, marks on a
//! TTY, and focus requests taking turns.

mod ghostty;
mod iterm;
mod linux;
mod rex;
#[cfg(test)]
pub(crate) mod stage;
mod terminal_app;
mod tmux;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, TryLockError};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Result};

use crate::config::{NewTerminal, TmuxUse};
use crate::os::{self, open::App, Os};
use crate::procs;
use crate::util::{home, sh_quote};

/// A terminal Codealong drives: an app it opens sessions in, finds their tabs in, types into and
/// closes (Ghostty, iTerm2 and Terminal, each through AppleScript, and Rex through its command),
/// or tmux, which runs inside one of those and takes typing through its own command. Each has a module under `launch/` that
/// knows its scripts and its ways, with a struct that answers here for what the rest of the app
/// asks of a terminal; what one can't do has a default that says so. Nothing outside those
/// modules branches on which terminal it is: it asks `terminal_app(profile)` for the one
/// config.toml names, or `terminal_kind(pid)` for the one a session's process runs under. A new
/// terminal is a module, a variant of `HostKind` and a line in `terminals()`.
///
/// A tab is addressed by what its terminal finds it again by (`at`): Ghostty's terminal id, the
/// TTY in iTerm2, Terminal and Rex, a tmux pane.
pub trait Terminal: Sync {
    fn kind(&self) -> HostKind;

    /// Its name, the way messages say it.
    fn name(&self) -> &'static str;

    /// What `[apps] terminal` in config.toml calls it. None for one that isn't an app sessions
    /// open in (tmux).
    fn profile(&self) -> Option<&'static str> {
        None
    }

    /// The app it's in, by bundle ID, to bring forward; tmux runs in whatever terminal its client
    /// does.
    fn bundle(&self) -> Option<&'static str> {
        None
    }

    /// Whether it's on this Mac: an app wherever apps get installed.
    fn installed(&self) -> bool;

    /// Whether a process of this name is the terminal, or its own: the nearest such process
    /// above a session's says which terminal the session runs in (`kind_by`).
    fn runs_as(&self, process: &str) -> bool;

    /// Whether Codealong can talk to it when a session runs under it. An app that's running
    /// takes scripts wherever it's installed.
    fn reachable(&self) -> bool {
        true
    }

    /// Opens a new terminal at `dir` with `env` set in its shell, and types `input` (a command
    /// plus return) there. The shell expands variables in `input` from `env`.
    fn run(&self, _place: Placement, _dir: &Path, _input: &str, _env: &[(String, String)]) -> Result<()> {
        bail!("Codealong can't open a terminal in {}.", self.name())
    }

    /// Opens a new terminal at `dir`, optionally typing `input` (a command plus return).
    fn open(&self, place: Placement, dir: &Path, input: Option<&str>) -> Result<()> {
        self.run(place, dir, input.unwrap_or(""), &[])
    }

    /// Its tabs, each with what it's addressed by, its folder and its title. One that doesn't
    /// say (Terminal lists no titles or folders of its tabs) has none.
    fn tabs(&self) -> Result<Vec<Tab>> {
        Ok(Vec::new())
    }

    /// Brings forward the tab `at`. `Ok(false)` when it has no such tab.
    fn focus(&self, _at: &str) -> Result<bool> {
        Ok(false)
    }

    /// Brings forward the tab a live session's process runs in, and says which it is. None when
    /// there's no such tab. `profile` is the terminal config.toml names, for a terminal with no
    /// tabs of its own (tmux) to look in when it can't tell where it shows.
    fn focus_session(&self, profile: &str, pid: u32, tty: &Path, title: &str, cwd: &Path) -> Result<Option<TerminalHost>>;

    /// The app a session's tab is in, by bundle ID, to bring forward: its own, or for one that
    /// runs inside another (tmux), that one's. None when it can't tell.
    fn app_of(&self, _pid: u32) -> Option<&'static str> {
        self.bundle()
    }

    /// What a new terminal types to show a session's process when no tab shows it now (a tmux
    /// session no client is attached to), None while one does. An app's sessions are its tabs.
    fn attach_command(&self, _pid: u32) -> Option<String> {
        None
    }

    /// Types `text` into the tab a session's process runs in and sends it with `submit`,
    /// pressing the key that clears the input box first for `clear`, where it can press one
    /// (`terminal_type_session`). `passes`: the send has another way in after this one, so where
    /// finding the tab is a question of its own (Ghostty), a slow one gives up before anything
    /// is typed rather than waiting.
    fn type_session(&self, pid: u32, cwd: &Path, text: &str, clear: bool, submit: Submit, passes: bool) -> Result<Typed>;

    /// Whether Codealong can press a single key there. Terminal presses Return after whatever
    /// a script types, and Claude Code takes "1" and Return arriving together as typed text, not
    /// as the digit that answers its prompt.
    fn presses_keys(&self) -> bool {
        false
    }

    /// The tab a session's process runs in, found (for Ghostty, marked) now, to press keys in:
    /// what `press` takes. An error from one that can't press one key alone.
    fn key_target(&self, _pid: u32, _cwd: &Path) -> Result<Option<String>> {
        bail!("{} can't press a single key for Codealong. Open it to answer.", self.name())
    }

    /// Presses one key in the tab `at`, as if typed: no paste, no Return.
    fn press(&self, _at: &str, _key: Key) -> Result<()> {
        bail!("{} can't press a single key for Codealong.", self.name())
    }

    /// Whether it has tabs Codealong can close once the sessions in them have ended (idle.rs).
    fn closes_tabs(&self) -> bool {
        false
    }

    /// Hangs up what runs in tabs and closes them, all at once (`close_terminals`). One result
    /// per tab, in order; an error when it couldn't be asked at all.
    fn close_tabs(&self, _tabs: &[&Closing]) -> Result<Vec<Result<()>>> {
        bail!("Codealong can't close {} tabs.", self.name())
    }
}

/// Whether an app is where apps get installed, by its bundle's name.
fn app_installed(app: &str) -> bool {
    #[cfg(test)]
    if let Some(there) = stage::app_installed() {
        return there;
    }
    [PathBuf::from("/Applications"), home().join("Applications")].iter().any(|d| d.join(app).is_dir())
}

/// Every terminal on this OS: the profiles first, in the order Settings lists them and a new
/// config.toml picks the first one installed. On macOS the apps Codealong scripts; on Linux the
/// desktop's emulator through tmux. tmux sessions run in any terminal.
pub fn terminals() -> &'static [&'static dyn Terminal] {
    match os::current() {
        Os::MacOs => &[&ghostty::Ghostty, &iterm::ITerm2, &terminal_app::AppleTerminal, &rex::Rex, &tmux::Tmux],
        Os::Linux => &[&linux::LinuxTerminal, &tmux::Tmux],
    }
}

fn terminal(kind: HostKind) -> &'static dyn Terminal {
    *terminals().iter().find(|t| t.kind() == kind).expect("every kind has a terminal")
}

/// The terminals `[apps] terminal` can name, each with its profile.
pub fn profiles() -> impl Iterator<Item = (&'static str, &'static dyn Terminal)> {
    terminals().iter().filter_map(|t| Some((t.profile()?, *t)))
}

/// The terminal a profile names.
pub fn terminal_app(id: &str) -> Option<&'static dyn Terminal> {
    profiles().find(|(profile, _)| *profile == id).map(|(_, t)| t)
}

pub fn terminal_name(id: &str) -> String {
    terminal_app(id).map_or_else(|| id.to_string(), |t| t.name().to_string())
}

/// The bundle ID of a terminal profile's app, to bring it forward.
pub fn bundle_id(profile: &str) -> Option<&'static str> {
    terminal_app(profile).and_then(|t| t.bundle())
}

/// The profile a config.toml without one uses: the first terminal installed here.
pub fn default_terminal() -> &'static str {
    profiles().find(|(_, t)| t.installed()).map_or("terminal", |(profile, _)| profile)
}


/// The names of the terminals that can do something, as a sentence lists them.
fn terminals_that(can: impl Fn(&dyn Terminal) -> bool) -> String {
    crate::util::listed(&terminals().iter().filter(|t| can(**t)).map(|t| t.name().to_string()).collect::<Vec<_>>(), "or")
}

/// The profiles `[apps] terminal` takes, as config.toml writes them, for messages.
pub fn profile_names() -> String {
    crate::util::listed(&profiles().map(|(profile, _)| format!("{profile:?}")).collect::<Vec<_>>(), "or")
}

fn unknown_profile(profile: &str) -> anyhow::Error {
    anyhow!("There's no terminal profile called {profile:?}. Use {} in config.toml.", profile_names())
}

/// How long after pressing a key before a paste, so the program reads them apart.
const CLEAR_WAIT: Duration = Duration::from_millis(200);

/// How long Codealong waits for Ghostty. It can be slow to answer while it launches, while it's
/// busy, or while macOS asks whether Codealong may control it (asked again after each ad-hoc signed rebuild).
/// Stopping early doesn't stop Ghostty: it still does what it was asked, so a short wait
/// reported failures for windows that then opened.
const OSASCRIPT_TIMEOUT: Duration = Duration::from_secs(60);

/// What `osascript` says when macOS hasn't let Codealong control a terminal app.
#[derive(Debug)]
struct NotAllowed(&'static str);

impl std::fmt::Display for NotAllowed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Codealong isn't allowed to control {}. Turn it on in System Settings → Privacy & Security → Automation.", self.0)
    }
}

impl std::error::Error for NotAllowed {}

/// Runs a script that talks to `app` (its name, for what goes wrong).
pub fn osascript_in(app: &'static str, script: &str, args: &[&str]) -> Result<String> {
    osascript_within(app, script, args, OSASCRIPT_TIMEOUT)
}

/// `osascript_in` given `timeout` rather than a minute: for a script that only asks something,
/// which can be given up on without leaving anything half done.
fn osascript_within(app: &'static str, script: &str, args: &[&str], timeout: Duration) -> Result<String> {
    let mut full = vec!["-"];
    full.extend_from_slice(args);
    procs::run_stdin("/usr/bin/osascript", &full, script, timeout).map_err(|e| {
        let msg = e.to_string();
        if msg.contains("-1743") || msg.contains("Not authorized") {
            anyhow::Error::new(NotAllowed(app))
        } else if msg.contains("timed out") && timeout == OSASCRIPT_TIMEOUT {
            anyhow!("{app} didn't answer within a minute. It may still open the window; if macOS asked whether Codealong may control {app}, allow it and try again.")
        } else if msg.contains("timed out") {
            anyhow!("{app} didn't answer in time.")
        } else {
            anyhow!("{app} didn't respond: {msg}")
        }
    })
}

/// A line to type, without the newline that ends it: iTerm2 and Terminal press Return after it.
fn typed_line(input: &str) -> &str {
    input.strip_suffix('\n').unwrap_or(input)
}

/// `text` as a bracketed paste, with any Escape in it taken out, for an app whose script types
/// exactly what it's given (iTerm2, Terminal): a program that takes bracketed pastes (Claude Code,
/// Codex) gets it whole.
pub(crate) fn pasted(text: &str) -> String {
    format!("\x1b[200~{}\x1b[201~", text.replace('\x1b', ""))
}

/// A line to type into a new tab's shell, led by Ctrl-U, which clears what's on the line first in
/// zsh, bash and fish, and in a terminal with no line editor. A window that comes forward takes
/// the keyboard, and keys typed meanwhile went in ahead of the line (`ree exec /bin/sh …`).
fn fresh_line(line: &str) -> String {
    format!("\x15{line}")
}

/// The login shell, as the user database has it: what a terminal app starts in a new tab.
pub(crate) fn login_shell() -> String {
    // SAFETY: getpwuid_r fills `pw` with pointers into `buf`, which outlives their use here.
    let from_db = unsafe {
        let mut pw: libc::passwd = std::mem::zeroed();
        let mut found: *mut libc::passwd = std::ptr::null_mut();
        let mut buf = vec![0 as libc::c_char; 4096];
        let r = libc::getpwuid_r(libc::getuid(), &mut pw, buf.as_mut_ptr(), buf.len(), &mut found);
        (r == 0 && !found.is_null() && !pw.pw_shell.is_null()).then(|| std::ffi::CStr::from_ptr(pw.pw_shell).to_string_lossy().into_owned())
    };
    from_db.filter(|s| Path::new(s).is_file()).unwrap_or_else(|| "/bin/zsh".to_string())
}

/// Where the next launcher goes, one number per launch so two at once never share a file.
static LAUNCHES: AtomicU64 = AtomicU64::new(0);

/// Launchers older than this never ran (their terminal didn't open), so they're swept away.
const STALE_LAUNCHER: Duration = Duration::from_secs(600);

/// A script that sets up a new tab's shell as Ghostty's window configuration does: its folder
/// and environment, which iTerm2 and Terminal don't take from a script. /bin/sh runs it whatever
/// the user's shell, so its quoting is sh's; it deletes itself first (it can hold a prompt), then
/// becomes the login shell, which keeps the environment and reads its startup files as a new
/// tab's would. It's in the per-user temporary folder, readable only by you.
fn launcher(dir: &Path, env: &[(String, String)]) -> Result<PathBuf> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let tmp = std::env::temp_dir();
    sweep_launchers(&tmp);
    let n = LAUNCHES.fetch_add(1, Ordering::SeqCst);
    let path = tmp.join(format!("codealong-launch-{}-{}-{n}.sh", std::process::id(), crate::util::now_ms()));
    let text = launcher_text(&path, dir, env, &login_shell());
    let mut f = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&path)?;
    f.write_all(text.as_bytes())?;
    Ok(path)
}

fn launcher_text(path: &Path, dir: &Path, env: &[(String, String)], shell: &str) -> String {
    let mut s = format!("rm -f {}\ncd {} 2>/dev/null\n", sh_quote(&path.to_string_lossy()), sh_quote(&dir.to_string_lossy()));
    for (k, v) in env {
        // Names come from Codealong; anything else couldn't be exported anyway.
        if !k.is_empty() && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') && !k.starts_with(|c: char| c.is_ascii_digit()) {
            s.push_str(&format!("export {k}={}\n", sh_quote(v)));
        }
    }
    s.push_str(&format!("exec {} -l\n", sh_quote(shell)));
    s
}

fn sweep_launchers(tmp: &Path) {
    let Ok(entries) = std::fs::read_dir(tmp) else { return };
    for e in entries.flatten().filter(|e| e.file_name().to_string_lossy().starts_with("codealong-launch-")) {
        if e.metadata().and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|age| age > STALE_LAUNCHER) {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

/// Where a new terminal goes: `[apps] new_terminal`, the folders it belongs beside, nearest
/// first (`board::terminal_neighbours`), whether it stays out of sight (a Background start of a
/// harness with no background service of its own: in no tab at all where tmux can hold it,
/// `out_of_sight_in_tmux`, else behind your windows) instead of coming forward, and the name its
/// tab is given (`board::tab_label`), which a terminal that can't name a tab ignores.
#[derive(Debug, Clone, Copy)]
pub struct Placement<'a> {
    pub new_terminal: NewTerminal,
    pub near: &'a [Vec<String>],
    pub behind: bool,
    pub label: Option<&'a str>,
}

impl Placement<'_> {
    /// The word a script that opens a terminal takes for whether its app comes forward.
    fn forward(&self) -> &'static str {
        if self.behind {
            "behind"
        } else {
            "front"
        }
    }
}

/// Opens a new terminal at `path`, optionally typing `input` (a command plus return).
pub fn terminal_open(profile: &str, place: Placement, path: &Path, input: Option<&str>) -> Result<()> {
    opens_in(profile)?.open(place, path, input)
}

/// Opens a new terminal at `path` with `env` set in its shell, and types `input` (a command
/// plus return). The shell expands variables in `input` from `env`.
pub fn terminal_run(profile: &str, place: Placement, path: &Path, input: &str, env: &[(String, String)]) -> Result<()> {
    opens_in(profile)?.run(place, path, input, env)
}

/// Where a session Codealong starts in a terminal runs (`session_run`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Host {
    /// In a tab of the terminal app.
    App,
    /// In Codealong's own tmux server (tmux.rs `hosted`), which the terminal app shows by
    /// attaching to it.
    Tmux,
}

/// Why a session can't start in a terminal now.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Unhosted {
    /// The screen is locked, so the terminal app can't start one, and tmux doesn't either:
    /// `[apps] tmux` says never, or it isn't installed.
    Locked { tmux_installed: bool },
    /// `[apps] tmux` says always, and it isn't installed.
    NoTmux,
}

impl Unhosted {
    /// What a start says of it, in the words of `terminal`, the app config.toml names.
    pub fn words(self, terminal: &str) -> String {
        match self {
            Unhosted::Locked { tmux_installed: false } => format!("The screen is locked, and {terminal} can't open a terminal until you unlock it. With tmux installed (brew install tmux), Codealong starts sessions in it meanwhile."),
            Unhosted::Locked { tmux_installed: true } => format!("The screen is locked, and {terminal} can't open a terminal until you unlock it. Settings › General › Run sessions in tmux can have Codealong start them in tmux meanwhile."),
            Unhosted::NoTmux => "Settings › General says to run sessions in tmux, but tmux isn't installed where Codealong looks for it (brew install tmux).".into(),
        }
    }
}

/// Where a session Codealong starts in a terminal runs, as `[apps] tmux` says, the screen is,
/// and tmux is installed. While the screen is locked a terminal app can't be counted on to start
/// one: Ghostty's terminals don't start then, and iTerm2's and Terminal's are unchecked. Linux's
/// terminal is a tmux session already.
pub fn host_for(use_tmux: TmuxUse) -> std::result::Result<Host, Unhosted> {
    host_rule(os::current(), use_tmux, screen_locked(), tmux::bin().is_some())
}

fn host_rule(on: Os, use_tmux: TmuxUse, locked: bool, has_tmux: bool) -> std::result::Result<Host, Unhosted> {
    match (on, use_tmux, locked, has_tmux) {
        (Os::Linux, ..) => Ok(Host::App),
        (_, TmuxUse::Always, _, true) | (_, TmuxUse::WhenLocked, true, true) => Ok(Host::Tmux),
        (_, TmuxUse::Always, _, false) => Err(Unhosted::NoTmux),
        (_, _, false, _) => Ok(Host::App),
        (_, _, true, tmux_installed) => Err(Unhosted::Locked { tmux_installed }),
    }
}

/// Whether a session that's to stay out of sight (a Background start of a harness with no
/// background service of its own, `Placement::behind`) runs in Codealong's tmux server with no tab
/// at all, until Show in opens one, rather than in a tab of the terminal app behind your windows:
/// wherever tmux is installed, unless `[apps] tmux` says never. Nobody watches such a session, and
/// a tab behind your windows stays there once it's done. Linux's terminal does this already.
pub fn out_of_sight_in_tmux(use_tmux: TmuxUse) -> bool {
    out_of_sight_rule(os::current(), use_tmux, tmux::bin().is_some())
}

fn out_of_sight_rule(on: Os, use_tmux: TmuxUse, has_tmux: bool) -> bool {
    has_tmux && (on == Os::Linux || use_tmux != TmuxUse::Never)
}

/// Whether tmux is where Codealong looks for it.
pub fn tmux_installed() -> bool {
    tmux::bin().is_some()
}

/// Whether the screen is locked (os/system `screen_locked`).
pub fn screen_locked() -> bool {
    #[cfg(test)]
    if let Some(staged) = stage::screen_locked() {
        return staged;
    }
    os::system::screen_locked()
}

/// How a session Codealong started in a terminal went (`session_run`).
#[derive(Debug, Clone, PartialEq)]
pub enum Ran {
    /// In a tab of the terminal app.
    InApp,
    /// In Codealong's tmux server, which the terminal shows as `Shown` says.
    InTmux(Shown),
}

/// When a terminal shows a session started in tmux.
#[derive(Debug, Clone, PartialEq)]
pub enum Shown {
    /// It does now: a tab, in front or behind your windows as the start asked.
    Now,
    /// Once the screen is unlocked (`show_unlocked`).
    OnUnlock,
    /// When Show in asks, since the terminal didn't open, for this reason.
    Not(String),
    /// When Show in asks: it's out of sight, in no tab at all (`out_of_sight_in_tmux`).
    OnAsk,
}

impl Ran {
    /// What a start adds to what it says, for `terminal`, where the session isn't in a tab of it
    /// now.
    pub fn note(&self, terminal: &str) -> Option<String> {
        match self {
            Ran::InApp | Ran::InTmux(Shown::Now) => None,
            Ran::InTmux(Shown::OnUnlock) => Some(format!("The screen is locked, so it runs in tmux, and {terminal} shows it once you unlock.")),
            Ran::InTmux(Shown::Not(why)) => Some(format!("It runs in tmux, but {terminal} didn't open it ({why}); Show in opens it.")),
            Ran::InTmux(Shown::OnAsk) => Some(format!("No window shows it; Show in opens one in {terminal}.")),
        }
    }
}

/// Starts a session, or a command, in a new terminal as `terminal_run` does, where `host_for`
/// says: a tab of the terminal app, or a session in Codealong's tmux server with a tab of the
/// app attached to it, in front or behind your windows as `place` says, which ends it as it
/// closes, as a tab's own session ends. While the screen is locked the tab opens once it isn't.
/// One to stay out of sight runs in tmux with no tab, where tmux can hold it
/// (`out_of_sight_in_tmux`). A host of Codealong's own would take tmux's place here.
pub fn session_run(profile: &str, use_tmux: TmuxUse, place: Placement, path: &Path, input: &str, env: &[(String, String)]) -> Result<Ran> {
    if place.behind && out_of_sight_in_tmux(use_tmux) {
        tmux::start(path, input, env, place.label)?;
        return Ok(Ran::InTmux(Shown::OnAsk));
    }
    let app = opens_in(profile)?;
    match host_for(use_tmux).map_err(|why| anyhow!(why.words(app.name())))? {
        Host::App => terminal_run(profile, place, path, input, env).map(|_| Ran::InApp),
        Host::Tmux => {
            let hosted = tmux::start(path, input, env, place.label)?;
            if screen_locked() {
                return Ok(Ran::InTmux(match hosted.wait_for_tab(place.behind) {
                    Ok(()) => {
                        MARKED.store(true, Ordering::SeqCst);
                        Shown::OnUnlock
                    }
                    Err(e) => Shown::Not(e.to_string()),
                }));
            }
            let line = hosted.attach_line(true).ok_or_else(|| anyhow!("tmux isn't installed where Codealong looks for it."))?;
            // The session runs whether or not a tab shows it: Show in opens one later.
            Ok(Ran::InTmux(match app.run(place, path, &format!("exec {line}\n"), &[]) {
                Ok(()) => Shown::Now,
                Err(e) => Shown::Not(e.to_string()),
            }))
        }
    }
}

/// A session was marked as waiting for a tab in this process since the engine last asked
/// (`take_marked`).
static MARKED: AtomicBool = AtomicBool::new(false);

/// Whether a session was marked as waiting for a tab since this was last asked.
pub fn take_marked() -> bool {
    MARKED.swap(false, Ordering::SeqCst)
}

/// When the engine's tick looks for sessions that wait for a tab (`show_unlocked`): as the screen
/// is unlocked, on its first look when it isn't (after a restart, or a start the command made with
/// the app closed), and once a session was marked since (`take_marked`: a start between two ticks
/// that saw the lock, or one that saw it just before a look), never while a look is under way and
/// never while the screen is locked. What's due waits for that.
#[derive(Debug, Default)]
pub struct TabLooks {
    was_locked: Option<bool>,
    due: bool,
    looking: bool,
}

impl TabLooks {
    /// A tick, the screen `locked` and `marked` what `take_marked` said: whether to look now.
    pub fn on_tick(&mut self, locked: bool, marked: bool) -> bool {
        self.due |= marked || (!locked && self.was_locked != Some(false));
        self.was_locked = Some(locked);
        if locked || self.looking || !self.due {
            return false;
        }
        self.due = false;
        self.looking = true;
        true
    }

    /// The look ended, however it went; `deferred` when the screen was locked again before every
    /// tab opened, so another look is due.
    pub fn on_looked(&mut self, deferred: bool) {
        self.looking = false;
        self.due |= deferred;
    }
}

/// How a look for sessions waiting for a tab went (`show_unlocked`): the tabs that didn't open,
/// each with why, and whether the screen was locked again before they all had.
#[derive(Debug, Default, PartialEq)]
pub struct Looked {
    pub failed: Vec<String>,
    pub deferred: bool,
}

/// Opens a tab of the terminal `profile` names for each session that started in tmux while the
/// screen was locked and waits for one (tmux.rs `waiting_tabs`), as its start would have: beside
/// the terminals of the worktrees near it in `lanes`, in front or behind your windows. Each waits
/// no longer only once it's handled, so a restart partway leaves the rest for the next look, and
/// a lock partway (a tab can take a minute to open) leaves the rest, and the one being opened as
/// it came, for the next unlock. A session that ended meanwhile, or that a terminal shows
/// already (Show in opened it, or a look cut short opened it), gets none. Says each that didn't
/// open otherwise, whose session goes on in tmux for Show in to open.
pub fn show_unlocked(profile: &str, new_terminal: NewTerminal, lanes: &[crate::model::Lane]) -> Looked {
    let mut looked = Looked::default();
    for (w, shown) in tmux::waiting_tabs() {
        if !shown {
            if screen_locked() {
                looked.deferred = true;
                break;
            }
            let near = crate::board::terminal_neighbours(lanes, &w.dir);
            let place = Placement { new_terminal, near: &near, behind: w.behind, label: Some(w.label.as_str()) };
            let opened = w.hosted.attach_line(true).ok_or_else(|| anyhow!("tmux isn't installed where Codealong looks for it.")).and_then(|line| opens_in(profile)?.run(place, &w.dir, &format!("exec {line}\n"), &[]));
            // Locked as it opened, Ghostty's window can have come with no terminal in it and
            // still answer that it opened: it keeps its mark, and the next look finds it attached
            // if it did open.
            if screen_locked() {
                looked.deferred = true;
                break;
            }
            if let Err(e) = opened {
                looked.failed.push(format!("{}: {e}", w.label));
            }
        }
        w.hosted.handled();
    }
    looked
}

/// The terminal a profile names, to open something in. Says so when its app isn't here, rather
/// than letting AppleScript ask where it is.
fn opens_in(profile: &str) -> Result<&'static dyn Terminal> {
    match terminal_app(profile) {
        Some(app) if !app.installed() => bail!("{} isn't installed. Install it, or pick another terminal in Settings.", app.name()),
        Some(app) => Ok(app),
        None => Err(unknown_profile(profile)),
    }
}

/// The first two arguments of a script that opens a terminal: where it goes, and the tabs it
/// goes beside. For `project`, the tabs already in the nearest folders that have any; with none,
/// or the app not answering the question, a window. For `tab`, the front window, with the same
/// tabs, which a terminal that orders its tabs puts it after when they're in that window.
fn placement(place: Placement, tabs: impl FnOnce() -> Result<Vec<Tab>>) -> (&'static str, String) {
    match place.new_terminal {
        NewTerminal::Window => ("window", String::new()),
        NewTerminal::Tab if place.near.is_empty() => ("front", String::new()),
        NewTerminal::Tab => ("front", tabs().map(|ts| terminals_beside(&ts, place.near).join("\n")).unwrap_or_default()),
        NewTerminal::Project => {
            let beside = tabs().map(|ts| terminals_beside(&ts, place.near)).unwrap_or_default();
            if beside.is_empty() {
                ("window", String::new())
            } else {
                ("beside", beside.join("\n"))
            }
        }
    }
}

/// The ids of the terminals working in the nearest group of folders that has any.
fn terminals_beside(terminals: &[Tab], near: &[Vec<String>]) -> Vec<String> {
    near.iter()
        .map(|group| {
            terminals.iter().filter(|t| group.iter().any(|f| Path::new(&t.cwd).starts_with(f))).map(|t| t.id.clone()).collect::<Vec<_>>()
        })
        .find(|ids| !ids.is_empty())
        .unwrap_or_default()
}

/// A terminal as its app lists it: the id Codealong addresses it by (Ghostty's terminal id,
/// iTerm2's TTY), its folder and its title.
#[derive(Debug, Clone, PartialEq)]
pub struct Tab {
    pub id: String,
    pub cwd: String,
    pub name: String,
}

/// Focus requests take turns, and one still waiting when a newer one comes in is dropped. On a
/// busy Mac a click takes seconds to land: without turns, one that landed late brought its tab
/// forward over the tab clicked after it, and two clicks on one session each put back the
/// other's mark as the folder.
struct Turns {
    lock: Mutex<()>,
    newest: AtomicU64,
}

impl Turns {
    const fn new() -> Self {
        Turns { lock: Mutex::new(()), newest: AtomicU64::new(0) }
    }

    /// A place in line, taken when the click arrives.
    fn ticket(&self) -> u64 {
        self.newest.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// Waits for the ticket's turn. None when a newer ticket was taken meanwhile.
    fn wait(&self, ticket: u64) -> Option<MutexGuard<'_, ()>> {
        let turn = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        (self.newest.load(Ordering::SeqCst) == ticket).then_some(turn)
    }

    /// The turn without a ticket, as a send or a key press takes it, so a click waiting before
    /// it doesn't lose its own. Waited for until `deadline` when there is one: None after it.
    fn lock_by(&self, deadline: Option<Instant>) -> Option<MutexGuard<'_, ()>> {
        let Some(deadline) = deadline else { return Some(self.lock.lock().unwrap_or_else(|e| e.into_inner())) };
        loop {
            match self.lock.try_lock() {
                Ok(turn) => return Some(turn),
                Err(TryLockError::Poisoned(e)) => return Some(e.into_inner()),
                Err(TryLockError::WouldBlock) if Instant::now() >= deadline => return None,
                Err(TryLockError::WouldBlock) => std::thread::sleep(Duration::from_millis(20)),
            }
        }
    }
}

static FOCUS_TURNS: Turns = Turns::new();

/// How long a new tab's shell gets to be ready for typing (`wait_for_shell`).
const SHELL_READY_WAIT: Duration = Duration::from_secs(15);

/// Waits for a new tab's shell to take typing: its launcher gone (the launcher deletes itself
/// once it runs) and its terminal out of canonical mode, which a shell's line editor (zsh's,
/// bash's readline, fish's) switches off while it waits for a line. A line typed then arrives as
/// yours would, rather than sitting in the terminal while the shell reads its startup files, and
/// in Terminal it's how Codealong knows its setup line ran before typing a command after it.
/// False when the wait ran out.
fn wait_for_shell(tty: &str, launcher: Option<&Path>) -> bool {
    #[cfg(test)]
    if let Some(ready) = stage::shell_ready(tty, launcher) {
        return ready;
    }
    let started = std::time::Instant::now();
    loop {
        if launcher.map_or(true, |l| !l.exists()) && line_editor_waiting(Path::new(tty)) {
            return true;
        }
        if started.elapsed() > SHELL_READY_WAIT {
            return false;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Whether a program on the terminal reads it a key at a time, as a shell's line editor does
/// while it waits for you: canonical mode off. Asked of the terminal device; nothing is written.
fn line_editor_waiting(tty: &Path) -> bool {
    use std::os::fd::AsRawFd;
    use std::os::unix::fs::OpenOptionsExt;
    let Ok(f) = std::fs::OpenOptions::new().write(true).custom_flags(libc::O_NOCTTY | libc::O_NONBLOCK).open(tty) else { return false };
    // SAFETY: tcgetattr fills a plain-old-data struct for an open descriptor.
    unsafe {
        let mut t: libc::termios = std::mem::zeroed();
        libc::tcgetattr(f.as_raw_fd(), &mut t) == 0 && t.c_lflag & libc::ICANON == 0
    }
}

/// Where the unique folders Codealong reports on a session's TTY start.
const MARK_PREFIX: &str = "/tmp/codealong-";

/// Focuses the terminal tab a live session runs in, in whichever app it runs in. `Ok(false)` when
/// there's no such tab.
///
/// iTerm2 and Terminal list each tab's TTY, so the tab is the one on the session's. Ghostty has
/// no such property, so there it's found exactly another way: Codealong reports a unique folder
/// (OSC 7) on the session's TTY, finds the Ghostty terminal showing that folder, selects its tab,
/// then reports the real folder again. Claude Code rewrites tab titles as it works but never the
/// folder, so the mark holds. A tmux session's tab is its client's, found by the client's TTY the
/// same ways. Where none of that can work (a terminal Codealong doesn't script, or a tmux client
/// in one), the tab is matched by title in `profile`'s app, then by folder, but only when that
/// finds just one.
pub fn terminal_focus_session(profile: &str, pid: u32, title: &str, cwd: &Path) -> Result<bool> {
    let ticket = FOCUS_TURNS.ticket();
    let Some(tty) = tty_of(pid) else {
        bail!("This session runs in the background, so it has no terminal tab.");
    };
    let Some(_turn) = FOCUS_TURNS.wait(ticket) else {
        // Clicked again while this one waited: the newer click decides what comes forward.
        return Ok(true);
    };
    // Asked before the tab is raised: `follow` is for an app that wasn't the active one then.
    // Which app the tab is in is known only once it's found (a tmux session's is its client's).
    let behind: Vec<&str> = terminals().iter().filter_map(|t| t.bundle()).filter(|app| !app_active(app)).collect();
    let raised = match terminal_kind(pid) {
        Some(terminal) => terminal.focus_session(profile, pid, &tty, title, cwd)?,
        None => focus_by_title(profile, title, cwd)?,
    };
    let found = raised.is_some();
    if let Some(raised) = raised {
        if let Some(app) = raised.terminal.bundle().filter(|app| behind.contains(app)) {
            follow(ticket, app, raised);
        }
    }
    Ok(found)
}

/// What a new terminal types (Return left off) to show a live session's process that no tab
/// shows now: a tmux session no client is attached to. None where a tab may show it.
pub fn attach_command(pid: u32) -> Option<String> {
    terminal_kind(pid)?.attach_command(pid)
}

/// The app a live session's tab is in, by bundle ID: its terminal's, or for a session in tmux,
/// the one its client runs in. None when it can't tell.
pub fn session_app(pid: u32) -> Option<&'static str> {
    terminal_kind(pid)?.app_of(pid)
}

/// A session's tab, found: the terminal it's in and what that terminal finds it again by
/// (Ghostty's terminal id, the TTY in iTerm2 and Terminal, a tmux pane). A key is pressed in it,
/// and a tab that was brought forward is raised again by it.
pub struct TerminalHost {
    terminal: &'static dyn Terminal,
    at: String,
}

impl TerminalHost {
    fn again(&self) {
        let _ = self.terminal.focus(&self.at);
    }
}

/// How long a terminal's app gets to become the active one after its tab was raised (`follow`).
const FOLLOW_WAIT: Duration = Duration::from_secs(2);

/// Raises a focused tab once more when its app has become the active one, so a tab on another
/// desktop (a Space, a full-screen window's too) takes you there. macOS switches to a window's
/// desktop when the active app brings the window forward (unless the window is set to move to
/// the desktop you're on, by AppKit's notes on `moveToActiveSpace`). The terminals raise the
/// window first and become active after: Ghostty's `activate window` is `makeKeyAndOrderFront`,
/// then `NSApp.activate`, and after a click in the bar it's active only once Codealong asks
/// macOS (`front::bring_forward`). An app made active with one window on the desktop you're on
/// and the raised one on another needn't take you anywhere. So this waits for the app to be
/// active and raises the tab again, which moves nothing when it's in front already. A newer
/// click drops it, as it drops a focus still waiting its turn. The switch itself isn't checked
/// by hand yet (2026-10-01).
fn follow(ticket: u64, app: &'static str, raised: TerminalHost) {
    std::thread::spawn(move || {
        let newest = || FOCUS_TURNS.newest.load(Ordering::SeqCst) == ticket;
        if !wait_until(FOLLOW_WAIT, || app_active(app), || !newest()) {
            return;
        }
        if let Some(_turn) = FOCUS_TURNS.wait(ticket) {
            raised.again();
        }
    });
}

/// Whether an app is the active one, by its bundle ID.
fn app_active(app: &str) -> bool {
    #[cfg(test)]
    if let Some(active) = stage::app_active(app) {
        return active;
    }
    crate::os::apps::is_active(app)
}

/// Waits for `ready` to say yes. False when `dropped` does first, or `limit` runs out.
fn wait_until(limit: Duration, ready: impl Fn() -> bool, dropped: impl Fn() -> bool) -> bool {
    let started = std::time::Instant::now();
    loop {
        if dropped() {
            return false;
        }
        if ready() {
            return true;
        }
        if started.elapsed() >= limit {
            return false;
        }
        std::thread::sleep(Duration::from_millis(40));
    }
}

/// The one tab in `profile`'s app titled like the session (see `pick_by_title`). Terminal lists
/// no titles or folders of its tabs, so it has none.
fn focus_by_title(profile: &str, title: &str, cwd: &Path) -> Result<Option<TerminalHost>> {
    let Some(terminal) = terminal_app(profile) else { return Ok(None) };
    match pick_by_title(&terminal.tabs()?, title, cwd) {
        Some(at) => Ok(terminal.focus(&at)?.then(|| TerminalHost { terminal, at })),
        None => Ok(None),
    }
}

/// Focusing where each tab has a TTY (iTerm2, Terminal, Rex): the tab on the session's.
fn focus_on_tty(terminal: &'static dyn Terminal, tty: &Path) -> Result<Option<TerminalHost>> {
    let at = tty.to_string_lossy().into_owned();
    Ok(terminal.focus(&at)?.then(|| TerminalHost { terminal, at }))
}

/// How typing into a session's terminal went, when nothing went wrong while typing.
#[derive(Debug, Clone, PartialEq)]
pub enum Typed {
    /// Typed into the session's tab in this terminal app.
    Into(&'static str),
    /// Nothing was typed, so another way can take it: the session isn't in a terminal Codealong
    /// can type into, or its tab didn't turn up. Why, as the end of a sentence.
    Nowhere(String),
}

/// Types `text` into the terminal a session's process runs in and presses Return: Ghostty
/// (found by an OSC 7 mark, since it has no TTY or PID property), iTerm2 or Terminal (by the
/// tab's TTY), or tmux (by the pane's TTY). An error means typing started and failed, so the
/// text may be in the terminal already: the apps carry out what they were asked even after
/// Codealong stopped waiting on them.
///
/// `clear` presses Ctrl+C first, where a single key can be pressed, for an idle Claude Code
/// whose input box may hold a prompt it took back (claude/conversation.rs): with text there it clears the box
/// and keeps the text in its history; in an empty box it only says to press it again to exit.
/// Ctrl+L doesn't clear it (2.1.285 binds it to clearing the screen). The key goes a moment
/// before the paste: arriving together, Claude Code takes both for one paste.
///
/// `submit` is what sends it once it's in: Return, or keys typed in its place, which a terminal
/// that can't press keys alone (Terminal) has none of, so nothing is typed there. `passes`: the
/// send has another way in after this one (`Terminal::type_session`).
pub fn terminal_type_session(pid: u32, cwd: &Path, text: &str, clear: bool, submit: Submit, passes: bool) -> Result<Typed> {
    match terminal_kind(pid) {
        Some(terminal) if matches!(submit, Submit::Keys(_)) && !terminal.presses_keys() => Ok(Typed::Nowhere(format!("{} can't press the keys that send it now", terminal.name()))),
        Some(terminal) => terminal.type_session(pid, cwd, text, clear, submit, passes),
        None => Ok(Typed::Nowhere(format!("it isn't in a {} tab", terminals_that(|_| true)))),
    }
}

/// Typing where each tab has a TTY (iTerm2, Terminal, Rex): into the tab on the process's, by the
/// app's own way to type there. `clear` is its way to press one key, where the input box is to
/// be cleared first and it has one.
fn type_on_tty(app: &'static str, pid: u32, text: &str, clear: Option<fn(&str, Key) -> Result<()>>, type_text: impl FnOnce(&str, &str) -> Result<bool>) -> Result<Typed> {
    let Some(tty) = tty_of(pid).map(|t| t.to_string_lossy().into_owned()) else { return Ok(Typed::Nowhere(format!("Codealong couldn't find its {app} tab"))) };
    if let Some(press) = clear {
        press(&tty, Key::CtrlC)?;
        std::thread::sleep(CLEAR_WAIT);
    }
    match type_text(&tty, text) {
        Ok(true) => Ok(Typed::Into(app)),
        // The script finds the tab before typing, so a missing one got nothing.
        Ok(false) => Ok(Typed::Nowhere(format!("Codealong couldn't find its {app} tab"))),
        // macOS refused the whole script: nothing went in.
        Err(e) if e.is::<NotAllowed>() => Ok(Typed::Nowhere(not_allowed_why(app))),
        Err(e) => Err(e),
    }
}

fn not_allowed_why(app: &str) -> String {
    format!("Codealong isn't allowed to control {app} (System Settings → Privacy & Security → Automation)")
}

/// A terminal tab to close: a process on it, which finds the tab, the folder its terminal shows,
/// and the processes to hang up first. Close idle sessions hangs up the session's own process;
/// Close stale terminals what the tab's shell runs, or nothing when it's at its prompt.
pub struct Closing<'a> {
    pub pid: u32,
    pub cwd: &'a Path,
    pub hang_up: Vec<u32>,
}

/// Ends sessions' processes and closes the terminal tabs they ran in (idle.rs): `close_terminals`,
/// each session's process hung up. One result per `(pid, folder)`, in order.
pub fn close_session_terminals(sessions: &[(u32, &Path)]) -> Vec<Result<()>> {
    close_terminals(&sessions.iter().map(|&(pid, cwd)| Closing { pid, cwd, hang_up: vec![pid] }).collect::<Vec<_>>())
}

/// Hangs up what runs in tabs and closes them, all at once for each app: with dozens of tabs on a
/// busy Mac, one tab at a time took minutes. The processes get a hangup (as they would from the
/// tab closing), and the tabs whose processes all exited close in one script: the apps ask before
/// closing a tab with a program still running, and don't once only the shell is left at its
/// prompt. One result per tab, in order.
pub fn close_terminals(tabs: &[Closing]) -> Vec<Result<()>> {
    let mut out: Vec<Option<Result<()>>> = tabs.iter().map(|_| None).collect();
    let kinds: Vec<Option<HostKind>> = tabs.iter().map(|t| terminal_kind(t.pid).map(|t| t.kind())).collect();
    for terminal in terminals().iter().filter(|t| t.closes_tabs()) {
        let at: Vec<usize> = (0..tabs.len()).filter(|&i| kinds[i] == Some(terminal.kind())).collect();
        if at.is_empty() {
            continue;
        }
        let group: Vec<&Closing> = at.iter().map(|&i| &tabs[i]).collect();
        let results = terminal.close_tabs(&group);
        let results = results.unwrap_or_else(|e| group.iter().map(|_| Err(anyhow!("{e}"))).collect());
        for (i, r) in at.into_iter().zip(results) {
            out[i] = Some(r);
        }
    }
    out.into_iter().map(|r| r.unwrap_or_else(|| Err(anyhow!("It isn't in a {} tab.", closes_tabs_in())))).collect()
}

/// Closing where each tab has a TTY (iTerm2, Terminal, Rex): the tabs the app lists that hold these
/// processes, each tab's processes hung up, then the tabs whose processes exited closed in one
/// script. A tab whose last process is hung up (a program run in place of a shell) is left to its
/// terminal, whose settings say whether it closes a tab whose process ended (Terminal keeps one
/// that didn't exit cleanly): with nothing running there the script can't find it by what runs in
/// it, and its TTY is free for a new tab to take, which the script would close in its place.
fn close_tty_terminals(tabs: &[&Closing], app: &str, ttys: fn() -> Result<Vec<String>>, in_front: fn() -> Result<Vec<String>>, close: fn(&[&str]) -> Result<()>) -> Result<Vec<Result<()>>> {
    let listed = ttys()?;
    let front = in_front().unwrap_or_default();
    let mut out: Vec<Result<()>> = Vec::new();
    // (index, tty) of the ones to end.
    let mut ending = Vec::new();
    for (i, tab) in tabs.iter().enumerate() {
        match tty_of(tab.pid).map(|t| t.to_string_lossy().into_owned()) {
            None => out.push(Err(anyhow!("It has no terminal tab."))),
            Some(tty) if !listed.contains(&tty) => out.push(Err(anyhow!("Couldn't find its {app} tab."))),
            Some(tty) if front.contains(&tty) => out.push(Err(anyhow!("It's the tab you're in."))),
            Some(tty) => {
                out.push(Ok(()));
                ending.push((i, tty));
            }
        }
    }
    let last: Vec<bool> = ending.iter().map(|(i, _)| tabs[*i].hang_up.contains(&tabs[*i].pid) && shell_of(tabs[*i].pid) == Some(tabs[*i].pid)).collect();
    let stayed = hang_up_tabs(&ending.iter().map(|(i, _)| tabs[*i]).collect::<Vec<_>>());
    let mut closing = Vec::new();
    for (((i, tty), stayed), last) in ending.iter().zip(stayed).zip(last) {
        match stayed {
            None if last => out[*i] = Err(anyhow!("Its program ended; whether {app} closes the tab now is its own setting.")),
            None => closing.push(tty.as_str()),
            Some(pid) => out[*i] = Err(anyhow!("Its process (pid {pid}) didn't exit, so the tab stays.")),
        }
    }
    if !closing.is_empty() {
        // A moment for the shells to draw their prompts, which tells the app it's safe to close.
        std::thread::sleep(Duration::from_millis(500));
        if let Err(e) = close(&closing) {
            for (i, tty) in &ending {
                if closing.contains(&tty.as_str()) {
                    out[*i] = Err(anyhow!("Its process ended, but the tab didn't close: {e}"));
                }
            }
        }
    }
    Ok(out)
}

/// Hangs up every tab's processes at once, and says for each tab the first of them that didn't
/// exit, if one didn't.
fn hang_up_tabs(tabs: &[&Closing]) -> Vec<Option<u32>> {
    let mut exited = hang_up(&tabs.iter().flat_map(|t| t.hang_up.iter().copied()).collect::<Vec<_>>()).into_iter();
    tabs.iter()
        .map(|t| {
            let gone: Vec<bool> = exited.by_ref().take(t.hang_up.len()).collect();
            t.hang_up.iter().zip(gone).find(|(_, gone)| !gone).map(|(pid, _)| *pid)
        })
        .collect()
}

/// Hangs up each process, as its tab closing would, and waits up to ten seconds for them to
/// exit. Whether each one did, in order. A process that leads a process group (a job: job control
/// gives each its own) is hung up with its group, as a closing tab hangs up the group in its
/// foreground, so a wrapper that exits without passing the hangup on leaves nothing behind; one
/// of the group that ignores it outlives the tab, as it would a tab closed by hand. Each is
/// continued too, as the kernel does a stopped job it hangs up: a job stopped with ^Z takes the
/// hangup only once it runs.
fn hang_up(pids: &[u32]) -> Vec<bool> {
    #[cfg(test)]
    if let Some(exited) = stage::hang_up(pids) {
        return exited;
    }
    // SAFETY: getpgid and kill take plain numbers; a process that's gone answers an error.
    let alive = |pid: u32| unsafe { libc::kill(pid as libc::pid_t, 0) } == 0;
    for &pid in pids {
        let pid = pid as libc::pid_t;
        let target = if unsafe { libc::getpgid(pid) } == pid { -pid } else { pid };
        unsafe { libc::kill(target, libc::SIGHUP) };
        unsafe { libc::kill(target, libc::SIGCONT) };
    }
    let started = std::time::Instant::now();
    while pids.iter().any(|p| alive(*p)) && started.elapsed() < Duration::from_secs(10) {
        std::thread::sleep(Duration::from_millis(100));
    }
    pids.iter().map(|p| !alive(*p)).collect()
}

/// Which terminal: one for each `Terminal`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostKind {
    Ghostty,
    ITerm2,
    Terminal,
    Rex,
    Tmux,
    Linux,
}

/// The terminals Codealong can press keys in, for messages.
pub fn presses_keys_in() -> String {
    terminals_that(|t| t.presses_keys())
}

/// The terminals whose tabs Codealong can close, for messages.
pub fn closes_tabs_in() -> String {
    terminals_that(|t| t.closes_tabs())
}

/// The terminal a process runs in, by its parents: cheap enough to ask on every look.
pub fn terminal_kind(pid: u32) -> Option<&'static dyn Terminal> {
    kind_by(pid, parent_and_name).map(terminal).filter(|t| t.reachable())
}

/// A process's parent and name, as the kernel has them.
fn parent_and_name(pid: u32) -> Option<(u32, String)> {
    #[cfg(test)]
    if let Some(staged) = stage::parent_and_name(pid) {
        return staged;
    }
    procs::parent_and_name(pid)
}

/// The nearest of a process's ancestors that's a terminal, with `step` giving each process's
/// parent and name. Ghostty and Terminal start a login shell per tab (claude → zsh → login →
/// ghostty); iTerm2 starts them under a server of its own (`iTermServer-3.7.3`), which carries on
/// if iTerm2 restarts, and Rex under its server (claude → zsh → login → rex), which carries on
/// when Rex quits. tmux's server leaves the terminal it started in, so nothing above it is.
fn kind_by(pid: u32, step: impl Fn(u32) -> Option<(u32, String)>) -> Option<HostKind> {
    let mut at = pid;
    for _ in 0..16 {
        let (ppid, name) = step(at)?;
        let kind = terminals().iter().find(|t| t.runs_as(&name)).map(|t| t.kind());
        if kind.is_some() {
            return kind;
        }
        if ppid <= 1 {
            return None;
        }
        at = ppid;
    }
    None
}

/// The tab a session's process runs in, found (for Ghostty, marked) now, to press keys in.
/// An error for Terminal, which can't press one key alone.
pub fn terminal_host(pid: u32, cwd: &Path) -> Result<Option<TerminalHost>> {
    Ok(match terminal_kind(pid) {
        Some(terminal) => terminal.key_target(pid, cwd)?.map(|at| TerminalHost { terminal, at }),
        None => None,
    })
}

/// What sends a prompt once it's typed into a session's box: Return, or keys of the harness's
/// own typed in its place (`Adapter::sends_now_with`), which only a terminal that presses keys
/// can type (`Terminal::presses_keys`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Submit {
    Return,
    Keys(&'static str),
}

/// A key a harness's own prompt answers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// A choice's number, 1 to 9: Yes in a permission prompt, a question's choice in
    /// Antigravity's.
    Digit(u8),
    Escape,
    CtrlC,
    /// The last of a list's choices, in Claude Code's dialogs.
    End,
    /// Back to the question before, in Antigravity's questions.
    Left,
    /// Shift+Tab, which steps Claude Code's and Antigravity's terminal UIs to their next mode
    /// (cycle.rs).
    BackTab,
    Return,
    /// Keys of a harness's own typed as they are, as `Submit::Keys` types them after a prompt:
    /// Claude Code's send-now with nothing in its box (`Adapter::steer_queued`).
    Keys(&'static str),
}

const DIGITS: [&str; 9] = ["1", "2", "3", "4", "5", "6", "7", "8", "9"];

impl Key {
    /// What a terminal writes to the program for the key, without the keyboard protocols that
    /// tell more keys apart: programs that use them still take these (Claude Code, checked
    /// 2026-10-07 for End as `ESC [ F`, `ESC O F` and `ESC [ 4 ~`).
    pub fn typed(self) -> &'static str {
        match self {
            Key::Digit(n) => DIGITS[usize::from(n.clamp(1, 9)) - 1],
            Key::Escape => "\x1b",
            Key::CtrlC => "\x03",
            Key::End => "\x1b[F",
            Key::Left => "\x1b[D",
            Key::BackTab => "\x1b[Z",
            Key::Return => "\r",
            Key::Keys(keys) => keys,
        }
    }
}

/// Presses one key in a terminal, as if typed: no paste, no Return.
pub fn press_key(host: &TerminalHost, key: Key) -> Result<()> {
    host.terminal.press(&host.at, key)
}

/// The terminal device a process writes to, or None for a background process.
fn tty_of(pid: u32) -> Option<std::path::PathBuf> {
    #[cfg(test)]
    if let Some(staged) = stage::tty(pid) {
        return staged;
    }
    procs::tty(pid)
}

/// The shell of the tab a process runs in: the topmost of it and its parents on its terminal,
/// below a `login` (Ghostty and Terminal start one above the shell, and it runs with your real
/// user id, so it may be signalled, but it isn't the shell). None for a process with no terminal.
fn shell_of(pid: u32) -> Option<u32> {
    let tty = tty_of(pid)?;
    let mut at = pid;
    for _ in 0..16 {
        let (parent, _) = parent_and_name(at)?;
        match parent_and_name(parent) {
            Some((_, name)) if name != "login" && tty_of(parent).as_ref() == Some(&tty) => at = parent,
            _ => break,
        }
    }
    Some(at)
}

/// Reports `dir` as the terminal's folder. Never blocks: a full or stuck terminal just misses it.
fn write_osc7(tty: &Path, dir: &str) -> std::io::Result<()> {
    #[cfg(test)]
    if stage::osc7(tty, dir).is_some() {
        return Ok(());
    }
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut f = std::fs::OpenOptions::new().write(true).custom_flags(libc::O_NOCTTY | libc::O_NONBLOCK).open(tty)?;
    f.write_all(osc7(dir).as_bytes())
}

/// The escape sequence a shell sends to report its folder.
pub fn osc7(dir: &str) -> String {
    let path: String = dir
        .bytes()
        .map(|b| if b.is_ascii_alphanumeric() || b"/-._~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") })
        .collect();
    format!("\x1b]7;file://localhost{path}\x07")
}

/// Claude Code titles a tab with a status glyph, a space and the session title.
fn tab_title(name: &str) -> &str {
    match name.split_once(' ') {
        Some((glyph, rest)) if glyph.chars().count() == 1 && !glyph.chars().all(char::is_alphanumeric) => rest,
        _ => name,
    }
}

/// The one tab titled like the session (the one in its folder, when several are); else the one
/// tab in its folder. None when that still leaves more than one: a guess puts another agent's
/// terminal in front, and what's typed there goes to the wrong session.
pub fn pick_by_title(terminals: &[Tab], title: &str, cwd: &Path) -> Option<String> {
    let dir = cwd.to_string_lossy();
    let only = |ts: Vec<&Tab>| match ts.as_slice() {
        [t] => Some(t.id.clone()),
        _ => None,
    };
    let titled: Vec<&Tab> = terminals.iter().filter(|t| !title.is_empty() && tab_title(&t.name) == title).collect();
    if titled.is_empty() {
        return only(terminals.iter().filter(|t| t.cwd == dir).collect());
    }
    let here: Vec<&Tab> = titled.iter().copied().filter(|t| t.cwd == dir).collect();
    only(titled).or_else(|| only(here))
}

pub fn open_url(url: &str) -> Result<()> {
    os::open::url(url, Duration::from_secs(10)).map_err(|e| anyhow!("Couldn't open {url}: {e}"))
}

pub fn reveal(path: &Path) -> Result<()> {
    os::open::reveal(path, Duration::from_secs(10))
}

/// Kinds of file that opening runs rather than shows: apps and what installs into macOS, scripts,
/// which Terminal or a launcher runs, and the Finder's links to a file or a page (`.fileloc`,
/// `.webloc`, `.inetloc`), which open what they point at, a program as readily as a page.
const RUNS_WHEN_OPENED: [&str; 20] = ["app", "command", "tool", "terminal", "sh", "zsh", "bash", "py", "rb", "pl", "jar", "pkg", "mpkg", "prefpane", "saver", "workflow", "action", "fileloc", "webloc", "inetloc"];

/// What a Linux desktop runs when it opens one: a launcher (`.desktop`) and an AppImage.
const LINUX_RUNS_WHEN_OPENED: [&str; 2] = ["desktop", "appimage"];

/// Whether opening a file would run something: a folder (an app is one), a program, or a kind in
/// `RUNS_WHEN_OPENED` (and on Linux `LINUX_RUNS_WHEN_OPENED`).
fn runs_when_opened(path: &Path, folder: bool, mode: u32) -> bool {
    let kind = path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).unwrap_or_default();
    if os::current() == Os::Linux && LINUX_RUNS_WHEN_OPENED.contains(&kind.as_str()) {
        return true;
    }
    folder || mode & 0o111 != 0 || RUNS_WHEN_OPENED.contains(&kind.as_str())
}

/// A file a conversation names, as opening, showing or editing one takes it: a full path, there on
/// this Mac. Its name, to say what went wrong with, and where it really is: a link's target.
fn named_file(path: &Path) -> Result<(String, PathBuf)> {
    if !path.is_absolute() {
        bail!("{} isn't a full path.", path.display());
    }
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.display().to_string());
    match std::fs::canonicalize(path) {
        Ok(real) => Ok((name, real)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => bail!("{name} isn't on this Mac any more."),
        Err(e) => bail!("Couldn't open {name}: {e}"),
    }
}

/// Opens a file a conversation names (one an agent sent you, cites, or writes as a path) in the
/// app that opens its kind, as a double-click in the Finder would. What opening would run instead
/// (an app, a script, a program, or a link or a Finder alias, whatever it points at) is shown in
/// the Finder, so a click on a name an agent wrote runs nothing.
pub fn open_file(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let (name, real) = named_file(path)?;
    let meta = std::fs::metadata(&real).map_err(|e| anyhow!("Couldn't open {name}: {e}"))?;
    let (folder, mode) = (meta.is_dir(), meta.permissions().mode());
    // A link opens what it links to, so that's what's judged, by its own name too.
    if runs_when_opened(path, folder, mode) || runs_when_opened(&real, folder, mode) || os::files::finder_alias(&real) {
        return reveal(path);
    }
    os::open::url(&path.to_string_lossy(), Duration::from_secs(10)).map_err(|e| anyhow!("Couldn't open {name}: {e}"))
}

/// Shows a file a conversation names in the Finder, selected, as its menu's Reveal in Finder does.
pub fn reveal_file(path: &Path) -> Result<()> {
    let (name, _) = named_file(path)?;
    reveal(path).map_err(|e| anyhow!("Couldn't show {name}: {e}"))
}

/// Opens a file a conversation names in the editor `id`, at `line` where the path gave one.
pub fn edit_file(id: &str, path: &Path, line: Option<u64>) -> Result<()> {
    named_file(path)?;
    crate::editors::open_at(id, path, line)
}

/// What a path a conversation names is on this Mac.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PathKind {
    File,
    Folder,
}

/// What each of `paths` is here, `None` where nothing is or it isn't a full path (`~/` is home),
/// for the windows to make only the paths that are here open on a click. Each is only looked up
/// (stat), never read, so one in a folder macOS protects doesn't ask for permission
/// (.claude/rules/macos-privacy.md).
pub fn path_kinds(paths: &[String]) -> Vec<Option<PathKind>> {
    paths
        .iter()
        .map(|p| {
            let path = crate::util::expand(p);
            let meta = path.is_absolute().then(|| std::fs::metadata(&path).ok()).flatten()?;
            Some(if meta.is_dir() { PathKind::Folder } else { PathKind::File })
        })
        .collect()
}

/// Opens a `codex://` link in the Codex app that's running. The Codex app ships as
/// ChatGPT.app now, and an older Codex.app may still be installed with the same scheme.
pub fn open_codex(url: &str) -> Result<()> {
    for app in ["/Applications/ChatGPT.app", "/Applications/Codex.app"] {
        let exe = format!("{app}/Contents/MacOS/");
        let running = procs::run("/usr/bin/pgrep", &["-f", &exe], None, Duration::from_secs(3)).is_ok();
        if running {
            return os::open::in_app(App::Named(app), Some(url), Duration::from_secs(10)).map_err(|e| anyhow!("Couldn't open Codex: {e}"));
        }
    }
    open_url(url)
}

/// Brings Antigravity forward. It has no link to a conversation (its only deep link teleports
/// into AI Studio), so the app is as close as Codealong can get.
pub fn open_antigravity() -> Result<()> {
    let app = "/Applications/Antigravity.app";
    if !Path::new(app).is_dir() {
        bail!("Antigravity isn't installed in /Applications.");
    }
    os::open::in_app(App::Named(app), None, Duration::from_secs(10)).map_err(|e| anyhow!("Couldn't open Antigravity: {e}"))
}

pub fn codex_thread_url(thread_id: &str) -> String {
    format!("codex://threads/{}", urlencoding::encode(thread_id))
}

pub fn codex_review_url(pr_url: &str) -> String {
    format!("codex://review?pr={}", urlencoding::encode(pr_url))
}

#[cfg(test)]
mod tests {
    use super::ghostty::{osascript, terminal_of as ghostty_terminal_of, terminals as ghostty_terminals};
    use super::ghostty::GHOSTTY_FOCUS;
    use super::tmux::{client_for as tmux_client_for, pane_for as tmux_pane_for, socket_in as tmux_socket_in, waiting_in as tmux_waiting_in, Client as TmuxClient, Pane as TmuxPane, Server as TmuxServer};
    use super::*;

    /// A send waits its turn only as long as it has; a key press, or a send once the turn is
    /// free, gets it.
    #[test]
    fn a_turn_is_waited_for_until_the_deadline() {
        let turns = Turns::new();
        let held = turns.lock.lock().unwrap();
        let started = Instant::now();
        assert!(turns.lock_by(Some(started + Duration::from_millis(60))).is_none());
        assert!(started.elapsed() >= Duration::from_millis(60));
        drop(held);
        assert!(turns.lock_by(Some(Instant::now())).is_some());
        assert!(turns.lock_by(None).is_some());
    }

    #[test]
    fn opening_runs_apps_programs_and_scripts_so_those_are_shown() {
        let runs = |name: &str, mode: u32| runs_when_opened(Path::new(name), false, mode);
        assert!(!runs("/d/report.pdf", 0o644) && !runs("/d/notes", 0o644));
        assert!(runs("/d/build.sh", 0o644) && runs("/d/tool", 0o755) && runs("/d/Setup.PKG", 0o644));
        assert!(runs_when_opened(Path::new("/d/Calculator.app"), true, 0o755));
        // The Finder's links open what they point at, which can be a program.
        assert!(runs("/d/notes.fileloc", 0o644) && runs("/d/site.webloc", 0o644) && runs("/d/Page.inetloc", 0o644));
        // A Linux desktop's launchers and AppImages run there.
        assert!(os::as_if(Os::Linux, || runs("/d/app.desktop", 0o644) && runs("/d/Tool.AppImage", 0o644)));
        assert!(os::as_if(Os::MacOs, || !runs("/d/app.desktop", 0o644) && !runs("/d/Tool.AppImage", 0o644)));
    }

    /// A path a conversation names is a file, a folder or nothing here; a part of one, or one
    /// that isn't full, is nothing. Saying what's wrong with one names the file.
    #[test]
    fn paths_named_are_looked_up_and_judged() {
        let dir = std::env::temp_dir().join(format!("codealong-paths-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("plans")).unwrap();
        std::fs::write(dir.join("plans/steps.md"), "# Steps").unwrap();
        let at = |p: &str| dir.join(p).to_string_lossy().into_owned();
        let kinds = path_kinds(&[at("plans/steps.md"), at("plans"), at("plans/gone.md"), "plans/steps.md".into(), String::new()]);
        assert_eq!(kinds, [Some(PathKind::File), Some(PathKind::Folder), None, None, None]);
        assert_eq!(named_file(&dir.join("plans/gone.md")).unwrap_err().to_string(), "gone.md isn't on this Mac any more.");
        assert_eq!(named_file(Path::new("plans/steps.md")).unwrap_err().to_string(), "plans/steps.md isn't a full path.");
        assert_eq!(named_file(&dir.join("plans/steps.md")).unwrap().0, "steps.md");
        assert!(edit_file("cursor", &dir.join("plans/gone.md"), Some(3)).is_err());
        assert!(reveal_file(Path::new("steps.md")).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn builds_deep_links() {
        assert_eq!(codex_thread_url("01a0c4a6"), "codex://threads/01a0c4a6");
        assert_eq!(
            codex_review_url("https://github.com/acme/web/pull/1037"),
            "codex://review?pr=https%3A%2F%2Fgithub.com%2Facme%2Fweb%2Fpull%2F1037"
        );
    }

    /// osacompile checks the scripts against each app's dictionary without running them: on a
    /// Mac with the apps.
    #[test]
    fn terminal_scripts_compile() {
        use super::ghostty::{GHOSTTY_ACTION, GHOSTTY_CLOSE, GHOSTTY_FIND, GHOSTTY_IN_FRONT, GHOSTTY_KEY, GHOSTTY_LIST, GHOSTTY_NEW, GHOSTTY_TYPE};
        if os::NATIVE != Os::MacOs {
            return;
        }
        let ghostty = [GHOSTTY_NEW, GHOSTTY_LIST, GHOSTTY_FOCUS, GHOSTTY_FIND, GHOSTTY_TYPE, GHOSTTY_IN_FRONT, GHOSTTY_CLOSE, GHOSTTY_KEY, GHOSTTY_ACTION].map(String::from).to_vec();
        for (app, scripts) in [("ghostty", ghostty), ("iterm2", iterm::scripts()), ("terminal", terminal_app::scripts())] {
            if !terminal_app(app).unwrap().installed() {
                continue;
            }
            for (i, script) in scripts.iter().enumerate() {
                let out = std::env::temp_dir().join(format!("codealong-test-{app}-{i}-{}.scpt", std::process::id()));
                let r = procs::run_stdin("/usr/bin/osacompile", &["-o", &out.to_string_lossy()], script, Duration::from_secs(20));
                let _ = std::fs::remove_file(&out);
                assert!(r.is_ok(), "{app} script {i} failed to compile: {r:?}");
            }
        }
    }

    /// Every terminal comes back with its folder: `tab` inside a Ghostty tell once meant its tab
    /// class, the lines had no separators, and every terminal was dropped. Needs Ghostty running
    /// with a terminal open: `cargo test ghostty_lists_terminals_with_folders -- --ignored`.
    #[test]
    #[ignore]
    fn ghostty_lists_terminals_with_folders() {
        let count: usize = osascript(r#"tell application "Ghostty" to return count of every terminal"#, &[]).unwrap().trim().parse().unwrap();
        assert!(count > 0, "Ghostty has no terminals open");
        assert_eq!(ghostty_terminals().unwrap().len(), count);
    }

    /// Opens a real Ghostty window whose shell writes the prompt it was handed back to a file,
    /// then exits. Opt in: `cargo test --release ghostty_hands_over -- --ignored`.
    #[test]
    #[ignore]
    fn ghostty_hands_over_the_environment() {
        let out = std::env::temp_dir().join(format!("codealong-env-{}", std::process::id()));
        let _ = std::fs::remove_file(&out);
        let prompt = "Fix the \"flaky\" test\n\tthen run `npm test` and $(say nope) 'quoted' -v";
        let input = format!("printenv CODEALONG_PROMPT > '{}'; exit\n", out.display());
        terminal_run("ghostty", Placement { new_terminal: NewTerminal::Window, near: &[], behind: false, label: None }, &std::env::temp_dir(), &input, &[("CODEALONG_PROMPT".to_string(), prompt.to_string())]).unwrap();
        for _ in 0..100 {
            if std::fs::metadata(&out).map(|m| m.len() > 0).unwrap_or(false) {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        std::thread::sleep(Duration::from_millis(200));
        let got = std::fs::read_to_string(&out).expect("the shell wrote nothing");
        let _ = std::fs::remove_file(&out);
        assert_eq!(got.strip_suffix('\n').unwrap_or(&got), prompt);
    }

    /// Opens a real Ghostty window with a tab in one folder and a tab in another, then a third
    /// tab beside the first folder's and a fourth, in the front window, beside the second's, and
    /// reads the window's tabs back: each new one sits right after its folder's, wherever the
    /// selected tab was, and each keeps the name it was given over the title its shell sets.
    /// Then closes the window. Opt in: `cargo test ghostty_groups_and_names_tabs -- --ignored`.
    #[test]
    #[ignore]
    fn ghostty_groups_and_names_tabs() {
        let scratch = crate::fixtures::scratch("tabs");
        let dir = std::fs::canonicalize(&*scratch).unwrap();
        let (alpha, beta) = (dir.join("alpha"), dir.join("beta"));
        std::fs::create_dir_all(&alpha).unwrap();
        std::fs::create_dir_all(&beta).unwrap();
        let ids = || ghostty_terminals().unwrap().into_iter().map(|t| t.id).collect::<Vec<_>>();
        let before = ids();
        terminal_run("ghostty", Placement { new_terminal: NewTerminal::Window, near: &[], behind: false, label: Some("alpha") }, &alpha, "", &[]).unwrap();
        let retitles = "printf '\\033]0;its own title\\007'\n";
        terminal_run("ghostty", Placement { new_terminal: NewTerminal::Tab, near: &[], behind: false, label: Some("beta") }, &beta, retitles, &[]).unwrap();
        std::thread::sleep(Duration::from_secs(1));
        let near = [vec![alpha.to_string_lossy().into_owned()]];
        terminal_run("ghostty", Placement { new_terminal: NewTerminal::Project, near: &near, behind: false, label: Some("alpha · two") }, &alpha, "", &[]).unwrap();
        // The third is selected now, so Ghostty puts the fourth before the second's tab.
        let near = [vec![beta.to_string_lossy().into_owned()]];
        terminal_run("ghostty", Placement { new_terminal: NewTerminal::Tab, near: &near, behind: false, label: Some("beta · two") }, &beta, "", &[]).unwrap();
        std::thread::sleep(Duration::from_millis(1500));
        let opened: Vec<String> = ids().into_iter().filter(|t| !before.contains(t)).collect();
        assert_eq!(opened.len(), 4, "{opened:?}");
        let read = r#"
on run argv
  tell application "Ghostty"
    set wids to id of (every window whose id of every terminal contains (item 1 of argv))
    set wid to item 1 of wids
    set names to name of every tab of window id wid
  end tell
  set out to wid
  repeat with n in names
    set out to out & linefeed & n
  end repeat
  return out
end run
"#;
        let got = osascript(read, &[&opened[0]]).unwrap();
        let mut lines = got.trim_end().lines();
        let window = lines.next().unwrap().to_string();
        let names: Vec<&str> = lines.collect();
        let _ = osascript("on run argv\n  tell application \"Ghostty\" to close window (window id (item 1 of argv))\nend run", &[&window]);
        assert_eq!(names, ["alpha", "alpha · two", "beta", "beta · two"]);
    }

    /// Waits up to ten seconds for a file to have something in it, and returns what.
    fn written(path: &Path) -> Option<String> {
        for _ in 0..100 {
            if std::fs::metadata(path).map(|m| m.len() > 0).unwrap_or(false) {
                std::thread::sleep(Duration::from_millis(200));
                return std::fs::read_to_string(path).ok();
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        None
    }

    /// Opens a window in iTerm2 or Terminal whose shell writes its folder and the prompt it was
    /// handed to a file, then closes it: what a started session gets from the launcher and the
    /// line typed after it. Opt in: `cargo test tty_apps_hand_over -- --ignored`. These tests'
    /// windows come forward and take the keyboard: what you type meanwhile lands in their shells.
    #[test]
    #[ignore]
    fn tty_apps_hand_over_the_folder_and_environment() {
        let dir = std::fs::canonicalize(std::env::temp_dir()).unwrap().join(format!("codealong-hand {}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let prompt = "Fix the \"flaky\" test\n\tthen run `npm test` and $(say nope) 'quoted' -v";
        let apps: [(&str, fn() -> Result<Vec<String>>, fn(&[&str]) -> Result<()>); 2] = [("iterm2", iterm::ttys, iterm::close), ("terminal", terminal_app::ttys, terminal_app::close)];
        for (profile, ttys, close) in apps {
            let out = dir.join(format!("{profile}.txt"));
            let input = format!("pwd > '{0}'; printenv CODEALONG_PROMPT >> '{0}'\n", out.display());
            let place = Placement { new_terminal: NewTerminal::Window, near: &[], behind: false, label: None };
            let before = ttys().unwrap();
            terminal_run(profile, place, &dir, &input, &[("CODEALONG_PROMPT".to_string(), prompt.to_string())]).unwrap();
            let got = written(&out);
            let opened: Vec<String> = ttys().unwrap().into_iter().filter(|t| !before.contains(t)).collect();
            let _ = close(&opened.iter().map(String::as_str).collect::<Vec<_>>());
            assert_eq!(got.unwrap_or_else(|| panic!("{profile}'s shell wrote nothing")), format!("{}\n{prompt}\n", dir.display()), "{profile}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// For each of iTerm2 and Terminal: a window whose shell records the bytes that reach it, raw
    /// and with bracketed paste on as Claude Code runs, typed into as a send does. The text comes
    /// as a paste and then Return; in iTerm2, 1 and Escape, as answering a permission prompt does,
    /// come as typed keys. Opt in: `cargo test tty_apps_type -- --ignored`.
    #[test]
    #[ignore]
    fn tty_apps_type_as_a_send_does() {
        let dir = std::env::temp_dir();
        for profile in ["iterm2", "terminal"] {
            let out = dir.join(format!("codealong-typed-{profile}-{}", std::process::id()));
            let ready = dir.join(format!("codealong-typed-{profile}-{}.pid", std::process::id()));
            let _ = std::fs::remove_file(&out);
            let _ = std::fs::remove_file(&ready);
            let want: &[u8] = if profile == "iterm2" { b"\x1b[200~ab\ncd\x1b[201~\r1\x1b" } else { b"\x1b[200~ab\ncd\x1b[201~\r" };
            let input = format!("stty raw -echo; printf '\\033[?2004h'; echo $$ > '{}'; dd bs=1 count={} of='{}' 2>/dev/null; stty sane\n", ready.display(), want.len(), out.display());
            terminal_run(profile, Placement { new_terminal: NewTerminal::Window, near: &[], behind: false, label: None }, &dir, &input, &[]).unwrap();
            let pid: u32 = written(&ready).and_then(|p| p.trim().parse().ok()).unwrap_or_else(|| panic!("{profile}'s shell never started"));
            let kind = terminal_kind(pid);
            let typed = terminal_type_session(pid, &dir, "ab\ncd", false, Submit::Return, false).unwrap();
            if profile == "iterm2" {
                let host = terminal_host(pid, &dir).unwrap().expect("iTerm2 presses keys");
                press_key(&host, Key::Digit(1)).and_then(|_| press_key(&host, Key::Escape)).unwrap();
            } else {
                assert!(terminal_host(pid, &dir).is_err(), "Terminal can't press a key alone");
            }
            let mut got = Vec::new();
            for _ in 0..50 {
                got = std::fs::read(&out).unwrap_or_default();
                if got.len() >= want.len() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            let _ = std::fs::remove_file(&out);
            let _ = std::fs::remove_file(&ready);
            if let Some(tty) = tty_of(pid) {
                let tty = tty.to_string_lossy();
                let _ = if profile == "iterm2" { iterm::close(&[&tty]) } else { terminal_app::close(&[&tty]) };
            }
            assert_eq!(kind.and_then(|t| t.bundle()), bundle_id(profile), "{profile}");
            assert_eq!(typed, Typed::Into(terminal_app(profile).unwrap().name()), "{profile}");
            assert_eq!(got, want, "{profile}'s program got {:?}", String::from_utf8_lossy(&got));
        }
    }

    /// For each of iTerm2 and Terminal: a window running a long `sleep`, a plain window in front of
    /// it, then the first closed as Close idle terminals would (hangup, then close). Opt in:
    /// `cargo test tty_apps_close -- --ignored`.
    #[test]
    #[ignore]
    fn tty_apps_close_a_session_terminal() {
        let dir = std::env::temp_dir();
        let apps: [(&str, fn() -> Result<Vec<String>>, fn(&[&str]) -> Result<()>); 2] = [("iterm2", iterm::ttys, iterm::close), ("terminal", terminal_app::ttys, terminal_app::close)];
        for (profile, ttys, close) in apps {
            let place = Placement { new_terminal: NewTerminal::Window, near: &[], behind: false, label: None };
            let secs = 7000 + std::process::id() % 1000 + if profile == "terminal" { 1 } else { 0 };
            terminal_open(profile, place, &dir, Some(&format!("sleep {secs}\n"))).unwrap();
            let mut pid = None;
            for _ in 0..100 {
                let out = procs::run("/usr/bin/pgrep", &["-f", &format!("^sleep {secs}$")], None, Duration::from_secs(3)).unwrap_or_default();
                pid = out.trim().lines().next().and_then(|p| p.parse::<u32>().ok());
                if pid.is_some() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            let pid = pid.unwrap_or_else(|| panic!("the sleep never started in {profile}"));
            let tty = tty_of(pid).unwrap().to_string_lossy().into_owned();
            let before = ttys().unwrap();
            terminal_open(profile, place, &dir, None).unwrap();
            std::thread::sleep(Duration::from_secs(1));
            let front: Vec<String> = ttys().unwrap().into_iter().filter(|t| !before.contains(t)).collect();
            let result = close_session_terminals(&[(pid, dir.as_path())]).remove(0);
            std::thread::sleep(Duration::from_millis(500));
            let still = ttys().unwrap().contains(&tty);
            let _ = close(&front.iter().map(String::as_str).collect::<Vec<_>>());
            result.unwrap_or_else(|e| panic!("{profile}: {e}"));
            assert!(!still, "{profile}'s tab is still open");
        }
    }

    /// For each of iTerm2 and Terminal: three windows, the one furthest back focused by its TTY,
    /// and it's the one in front. It moves windows about, so it's opt in: `cargo test
    /// tty_apps_focus -- --ignored`.
    #[test]
    #[ignore]
    fn tty_apps_focus_a_window_behind_others() {
        const ITERM_FRONT: &str = r#"tell application "iTerm" to return tty of current session of current window"#;
        const TERMINAL_FRONT: &str = r#"tell application "Terminal" to return tty of selected tab of front window"#;
        let dir = std::env::temp_dir();
        let apps: [(&str, fn() -> Result<Vec<String>>, fn(&[&str]) -> Result<()>, fn(&str) -> Result<bool>, &str); 2] =
            [("iterm2", iterm::ttys, iterm::close, iterm::focus, ITERM_FRONT), ("terminal", terminal_app::ttys, terminal_app::close, terminal_app::focus, TERMINAL_FRONT)];
        for (profile, ttys, close, focus, front) in apps {
            let place = Placement { new_terminal: NewTerminal::Window, near: &[], behind: false, label: None };
            let mut opened = Vec::new();
            for _ in 0..3 {
                let before = ttys().unwrap();
                terminal_open(profile, place, &dir, None).unwrap();
                for _ in 0..50 {
                    if let Some(t) = ttys().unwrap().into_iter().find(|t| !before.contains(t)) {
                        opened.push(t);
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
            let back = opened[0].clone();
            let focused = focus(&back);
            std::thread::sleep(Duration::from_millis(500));
            let now = osascript_in("test", front, &[]).unwrap_or_default();
            let _ = close(&opened.iter().map(String::as_str).collect::<Vec<_>>());
            assert_eq!(opened.len(), 3, "{profile} didn't open three windows");
            assert!(focused.unwrap(), "{profile} didn't find {back}");
            assert_eq!(now.trim(), back, "{profile}");
        }
    }

    /// Opens a Ghostty window running a long `sleep` and a plain window in front of it, closes the
    /// first as Close idle terminals would (hangup, then close), and checks it's gone. It opens
    /// windows, so it's opt in: `cargo test ghostty_closes_a_session_terminal -- --ignored`.
    #[test]
    #[ignore]
    fn ghostty_closes_a_session_terminal() {
        const OPEN: &str = r#"
on run argv
  tell application "Ghostty"
    set seenIds to id of every terminal
    new window with configuration {initial working directory:(item 1 of argv), initial input:(item 2 of argv)}
    repeat 50 times
      set nowIds to id of every terminal
      repeat with t in nowIds
        if seenIds does not contain (contents of t) then return contents of t
      end repeat
      delay 0.1
    end repeat
  end tell
  return ""
end run
"#;
        const EXISTS: &str = r#"
on run argv
  tell application "Ghostty" to return exists terminal id (item 1 of argv)
end run
"#;
        const CLOSE: &str = r#"
on run argv
  tell application "Ghostty"
    try
      close terminal id (item 1 of argv)
    end try
  end tell
end run
"#;
        let dir = std::env::temp_dir();
        let secs = 7000 + std::process::id() % 1000;
        let sleeper = osascript(OPEN, &[&dir.to_string_lossy(), &format!("sleep {secs}\n")]).unwrap().trim().to_string();
        let front = osascript(OPEN, &[&dir.to_string_lossy(), ""]).unwrap().trim().to_string();
        let mut pid = None;
        for _ in 0..50 {
            let out = procs::run("/usr/bin/pgrep", &["-f", &format!("^sleep {secs}$")], None, Duration::from_secs(3)).unwrap_or_default();
            pid = out.trim().lines().next().and_then(|p| p.parse::<u32>().ok());
            if pid.is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let result = pid.ok_or_else(|| anyhow!("the sleep never started")).and_then(|pid| close_session_terminals(&[(pid, dir.as_path())]).remove(0));
        let still = osascript(EXISTS, &[&sleeper]).unwrap_or_default();
        let _ = osascript(CLOSE, &[&front]);
        let _ = osascript(CLOSE, &[&sleeper]);
        result.unwrap();
        assert_eq!(still.trim(), "false", "the terminal is still open");
    }

    /// Opens a Ghostty window whose program records the bytes that reach it, in raw mode with
    /// bracketed paste on as Claude Code runs, presses 1 and Escape there as answering a
    /// permission prompt does, and checks both arrived as typed keys: `send key` sent nothing
    /// for the digit. It opens a window, so it's opt in: `cargo test ghostty_presses_keys --
    /// --ignored`.
    /// Finds the Ghostty tab of a process that runs in one, as a send does before it types, and
    /// types nothing: `CODEALONG_TEST_PID=<pid> cargo test ghostty_finds_a_tab -- --ignored
    /// --nocapture`. The tab's folder changes for a moment and is put back, as a click's does.
    #[test]
    #[ignore]
    fn ghostty_finds_a_tab() {
        let pid: u32 = std::env::var("CODEALONG_TEST_PID").expect("CODEALONG_TEST_PID").parse().unwrap();
        let t = std::time::Instant::now();
        let kind = terminal_kind(pid).map(|t| t.kind());
        println!("terminal: {kind:?}, tty {:?}, in {:?}", tty_of(pid), t.elapsed());
        assert_eq!(kind, Some(HostKind::Ghostty));
        let t = std::time::Instant::now();
        let id = ghostty_terminal_of(pid, Path::new("/"), None).unwrap();
        println!("tab: {id:?}, in {:?}", t.elapsed());
        assert!(id.is_some());
    }

    #[test]
    #[ignore]
    fn ghostty_presses_keys() {
        const OPEN: &str = r#"
on run argv
  tell application "Ghostty"
    set seenIds to id of every terminal
    new window with configuration {initial working directory:(item 1 of argv), initial input:(item 2 of argv)}
    repeat 50 times
      set nowIds to id of every terminal
      repeat with t in nowIds
        if seenIds does not contain (contents of t) then return contents of t
      end repeat
      delay 0.1
    end repeat
  end tell
  return ""
end run
"#;
        const CLOSE: &str = r#"
on run argv
  tell application "Ghostty"
    try
      close terminal id (item 1 of argv)
    end try
  end tell
end run
"#;
        let dir = std::env::temp_dir();
        let out = dir.join(format!("codealong-keys-{}", std::process::id()));
        let ready = dir.join(format!("codealong-keys-{}.ready", std::process::id()));
        let _ = std::fs::remove_file(&out);
        let _ = std::fs::remove_file(&ready);
        // Once the terminal is raw and takes bracketed pastes, the next two bytes, whatever they are.
        let input = format!("stty raw -echo; printf '\\033[?2004h'; touch '{}'; dd bs=1 count=2 of='{}' 2>/dev/null; stty sane; exit\n", ready.display(), out.display());
        let id = osascript(OPEN, &[&dir.to_string_lossy(), &input]).unwrap().trim().to_string();
        for _ in 0..100 {
            if ready.exists() {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let host = TerminalHost { terminal: &ghostty::Ghostty, at: id.clone() };
        let pressed = press_key(&host, Key::Digit(1)).and_then(|_| press_key(&host, Key::Escape));
        for _ in 0..50 {
            if std::fs::metadata(&out).map(|m| m.len() >= 2).unwrap_or(false) {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let got = std::fs::read(&out).unwrap_or_default();
        let _ = osascript(CLOSE, &[&id]);
        let _ = std::fs::remove_file(&out);
        let _ = std::fs::remove_file(&ready);
        pressed.unwrap();
        assert_eq!(got, b"1\x1b", "the program got {:?}", String::from_utf8_lossy(&got));
    }

    /// Opens three Ghostty windows, focuses the terminal in the one furthest back, checks that
    /// its window is the one in front, then closes them. Held by position, the focus brought
    /// forward the window that had been in front of it instead. It moves windows about, so it's
    /// opt in: `cargo test ghostty_focuses_a_window_behind_others -- --ignored`.
    #[test]
    #[ignore]
    fn ghostty_focuses_a_window_behind_others() {
        const OPEN: &str = r#"
on run argv
  tell application "Ghostty"
    set seenIds to id of every terminal
    new window with configuration {initial working directory:(item 1 of argv)}
    repeat 50 times
      set nowIds to id of every terminal
      repeat with t in nowIds
        if seenIds does not contain (contents of t) then return contents of t
      end repeat
      delay 0.1
    end repeat
  end tell
  return ""
end run
"#;
        const FRONT: &str = r#"
on run argv
  tell application "Ghostty" to return id of every terminal of front window
end run
"#;
        const CLOSE: &str = r#"
on run argv
  tell application "Ghostty"
    repeat with t in argv
      try
        close terminal id (contents of t)
      end try
    end repeat
  end tell
end run
"#;
        struct Opened(Vec<String>);
        impl Drop for Opened {
            fn drop(&mut self) {
                let ids: Vec<&str> = self.0.iter().map(String::as_str).collect();
                let _ = osascript(CLOSE, &ids);
            }
        }
        let dir = std::env::temp_dir();
        let mut opened = Opened(Vec::new());
        for _ in 0..3 {
            let id = osascript(OPEN, &[&dir.to_string_lossy()]).unwrap().trim().to_string();
            assert!(!id.is_empty(), "the new window has no terminal");
            opened.0.push(id);
        }
        let back = opened.0[0].clone();
        assert_eq!(osascript(GHOSTTY_FOCUS, &["id", &back]).unwrap().trim(), back);
        std::thread::sleep(Duration::from_millis(500));
        let front = osascript(FRONT, &[]).unwrap();
        assert!(front.contains(&back), "the front window holds {}, not {back}", front.trim());
    }

    #[test]
    fn reports_a_folder_the_way_a_shell_does() {
        assert_eq!(osc7("/tmp/codealong-1-2"), "\x1b]7;file://localhost/tmp/codealong-1-2\x07");
        assert_eq!(osc7("/Users/me/My Repo"), "\x1b]7;file://localhost/Users/me/My%20Repo\x07");
    }

    /// Whether `pid` runs inside the app whose executable is named `exe`, with `step` giving each
    /// process's parent and name.
    fn runs_under_by(pid: u32, exe: &str, step: impl Fn(u32) -> Option<(u32, String)>) -> bool {
        let mut at = pid;
        for _ in 0..16 {
            let Some((ppid, name)) = step(at) else { return false };
            if name == exe {
                return true;
            }
            if ppid <= 1 {
                return false;
            }
            at = ppid;
        }
        false
    }

    fn runs_under(pid: u32, exe: &str) -> bool {
        runs_under_by(pid, exe, procs::parent_and_name)
    }

    #[test]
    fn knows_which_terminal_a_session_runs_in() {
        let _mac = os::AsIf::new(Os::MacOs);
        // (pid, parent, name) as the kernel has them, from this Mac.
        let table = [
            (849, 1, "ghostty"),
            (958, 849, "login"),
            (997, 958, "zsh"),
            (68277, 997, "claude"),
            (4100, 1, "tmux"),
            (4101, 4100, "zsh"),
            (4102, 4101, "claude"),
            (39227, 1, "iTerm2"),
            (39607, 39227, "iTermServer-3.7."),
            (41265, 39607, "login"),
            (41266, 41265, "zsh"),
            (41300, 41266, "2.1.284"),
            // iTerm2 restarted: its server carries on under launchd.
            (39700, 1, "iTermServer-3.7."),
            (39701, 39700, "zsh"),
            (51103, 1, "Terminal"),
            (41595, 51103, "login"),
            (41602, 41595, "zsh"),
            (41700, 41602, "codex"),
            (5000, 1, "Google Chrome"),
        ];
        let step = |pid: u32| table.iter().find(|r| r.0 == pid).map(|r| (r.1, r.2.to_string()));
        assert_eq!(kind_by(68277, step), Some(HostKind::Ghostty));
        assert_eq!(kind_by(4102, step), Some(HostKind::Tmux));
        assert_eq!(kind_by(41300, step), Some(HostKind::ITerm2));
        assert_eq!(kind_by(39701, step), Some(HostKind::ITerm2));
        assert_eq!(kind_by(41700, step), Some(HostKind::Terminal));
        assert_eq!(kind_by(5000, step), None);
        assert_eq!(kind_by(999_999, step), None);
    }

    #[test]
    fn knows_the_terminal_profiles() {
        let _mac = os::AsIf::new(Os::MacOs);
        assert_eq!(terminal_name("iterm2"), "iTerm2");
        assert_eq!(terminal_name("terminal"), "Terminal");
        assert_eq!(terminal_name("kitty"), "kitty");
        assert_eq!(bundle_id("ghostty"), Some("com.mitchellh.ghostty"));
        assert_eq!(bundle_id("kitty"), None);
        // Terminal comes with macOS, so a config.toml always has a terminal that's here.
        assert!(terminal_app("terminal").unwrap().installed());
        assert!(terminal_app(default_terminal()).unwrap().installed());
        assert!(terminal_open("kitty", Placement { new_terminal: NewTerminal::Window, near: &[], behind: false, label: None }, Path::new("/"), None).unwrap_err().to_string().contains("\"iterm2\""));
    }

    #[test]
    fn a_launcher_sets_up_the_shell_in_sh_whatever_the_prompt() {
        let text = launcher_text(
            Path::new("/tmp/codealong-launch-1.sh"),
            Path::new("/Users/me/My Repo's"),
            &[("CODEALONG_PROMPT".into(), "Fix the 'flaky' test\n$(say nope) `x`".into()), ("BAD NAME".into(), "x".into()), ("1X".into(), "x".into())],
            "/opt/homebrew/bin/fish",
        );
        assert_eq!(
            text,
            "rm -f '/tmp/codealong-launch-1.sh'\ncd '/Users/me/My Repo'\\''s' 2>/dev/null\nexport CODEALONG_PROMPT='Fix the '\\''flaky'\\'' test\n$(say nope) `x`'\nexec '/opt/homebrew/bin/fish' -l\n"
        );
        assert_eq!(typed_line("claude2 --session-id x\n"), "claude2 --session-id x");
        assert_eq!(fresh_line(" exec x"), "\u{15} exec x");
        assert_eq!(typed_line(""), "");
    }

    /// The launcher runs: its folder and environment reach the shell it becomes, and it's gone.
    #[test]
    fn a_launcher_hands_over_the_folder_and_environment() {
        let dir = std::env::temp_dir().join(format!("codealong-launch-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let prompt = "Fix the \"flaky\" test\n\tthen run `npm test` and $(say nope) 'quoted' -v";
        let path = launcher(&dir, &[("CODEALONG_PROMPT".to_string(), prompt.to_string())]).unwrap();
        // The login shell is swapped for one that prints what it was handed, then exits.
        let text = std::fs::read_to_string(&path).unwrap();
        let body = text.lines().filter(|l| !l.starts_with("exec ")).collect::<Vec<_>>().join("\n");
        std::fs::write(&path, format!("{body}\npwd; printenv CODEALONG_PROMPT\n")).unwrap();
        let out = procs::run("/bin/sh", &[&path.to_string_lossy()], None, Duration::from_secs(10)).unwrap();
        assert!(!path.exists(), "the launcher is still there");
        assert_eq!(out, format!("{}\n{prompt}\n", dir.display()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn reads_a_process_line_and_terminal_from_the_kernel() {
        // This test's own process and a child of it: named, with parents, found under it.
        let me = std::process::id();
        let (_, name) = procs::parent_and_name(me).expect("the kernel lists this process");
        assert!(!name.is_empty());
        assert!(runs_under(me, &name));
        assert!(!runs_under(me, "no-such-program"));
        let mut sleeper = std::process::Command::new("/bin/sleep").arg("30").stdin(std::process::Stdio::null()).spawn().unwrap();
        let child = sleeper.id();
        // Linux names the new program a moment after `spawn` returns, which until then has the
        // name of the thread that started it.
        let started = Instant::now();
        let mut seen = procs::parent_and_name(child);
        while seen.as_ref().is_some_and(|(_, n)| n != "sleep") && started.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(20));
            seen = procs::parent_and_name(child);
        }
        assert_eq!(seen, Some((me, "sleep".to_string())));
        assert!(runs_under(child, &name));
        // Every process `ps` lists has the parent, name and terminal it says, other users'
        // included, and is under Ghostty when `ps` puts it there: the root `login` in a Ghostty
        // tab's line is on the way, when one is open.
        let table = procs::run("/bin/ps", &["-A", "-o", "pid=,ppid=,tty=,ucomm="], None, Duration::from_secs(20)).unwrap();
        let rows: Vec<(u32, u32, &str, &str)> = table
            .lines()
            .filter_map(|line| {
                let mut f = line.split_whitespace();
                let (pid, ppid, tty) = (f.next()?.parse().ok()?, f.next()?.parse().ok()?, f.next()?);
                // A name can have spaces: the rest of the line.
                let name = line[line.find(tty)? + tty.len()..].trim();
                Some((pid, ppid, tty, name))
            })
            .collect();
        let by_ps = |pid: u32| rows.iter().find(|r| r.0 == pid).map(|r| (r.1, r.3.to_string()));
        let mut checked = 0;
        for &(pid, ppid, tty, name) in &rows {
            // A Linux kernel thread names itself for the work it's on, which changes as it goes.
            if ppid == 2 {
                continue;
            }
            // Gone since `ps` looked.
            let Some((parent, kernel_name)) = procs::parent_and_name(pid) else { continue };
            // A name cut to 16 characters can end in a space, which `ps` doesn't print.
            assert_eq!((parent, kernel_name.trim_end()), (ppid, name), "pid {pid}");
            // macOS names a tab's (`ttys004`) and not its console; Linux every terminal ps names
            // (`pts/3`, a VM's console `hvc0`), but none (`?`).
            let named = match crate::os::NATIVE {
                Os::MacOs => tty.starts_with("tty"),
                Os::Linux => !tty.starts_with('?'),
            };
            let want = named.then(|| Path::new("/dev").join(tty));
            assert_eq!(procs::tty(pid), want, "pid {pid}");
            assert_eq!(runs_under(pid, "ghostty"), runs_under_by(pid, "ghostty", by_ps), "pid {pid}");
            assert_eq!(kind_by(pid, procs::parent_and_name), kind_by(pid, by_ps), "pid {pid}");
            checked += 1;
        }
        assert!(checked > 10, "only {checked} processes checked");
        let _ = sleeper.kill();
        let _ = sleeper.wait();
        assert_eq!(procs::parent_and_name(child), None);
    }

    #[test]
    fn puts_a_new_tab_beside_the_nearest_folder_with_terminals() {
        let t = |id: &str, cwd: &str| Tab { id: id.into(), cwd: cwd.into(), name: String::new() };
        let tabs = vec![t("a", "/r/app"), t("b", "/r/app-pins/src"), t("c", "/r/site"), t("d", ""), t("e", "/r/app-pins")];
        let groups = |g: &[&[&str]]| g.iter().map(|g| g.iter().map(|f| f.to_string()).collect()).collect::<Vec<Vec<String>>>();
        assert_eq!(terminals_beside(&tabs, &groups(&[&["/r/app-pins"], &["/r/app"]])), vec!["b", "e"]);
        // Not a folder that merely starts the same way.
        assert_eq!(terminals_beside(&tabs, &groups(&[&["/r/app-p"], &["/r/app"]])), vec!["a"]);
        assert_eq!(terminals_beside(&tabs, &groups(&[&["/r/none"], &["/r/site", "/r/x"]])), vec!["c"]);
        assert!(terminals_beside(&tabs, &groups(&[&["/r/none"]])).is_empty());
    }

    #[test]
    fn matches_tabs_by_title_then_folder() {
        let t = |id: &str, cwd: &str, name: &str| Tab { id: id.into(), cwd: cwd.into(), name: name.into() };
        let tabs = vec![
            t("a", "/r/app4", "✳ RLS work and other tables"),
            t("b", "/r/app2", "✳ Cloud agent execution regressions"),
            t("c", "/r/app2", "✳ Cloud agent execution regressions ⑂"),
            t("d", "/r/app4", "…/projects/app4"),
            t("e", "/r/app3", "⠐ Fix the login redirect"),
            t("f", "/r/app1", "⠂ Fix the login redirect"),
            t("g", "/r/app5", "✳ Bump dependencies"),
            t("h", "/r/app6", "✳ Bump dependencies"),
        ];
        assert_eq!(pick_by_title(&tabs, "Cloud agent execution regressions ⑂", Path::new("/r/app2")).as_deref(), Some("c"));
        assert_eq!(pick_by_title(&tabs, "Cloud agent execution regressions", Path::new("/r/app2")).as_deref(), Some("b"));
        assert_eq!(pick_by_title(&tabs, "RLS work and other tables", Path::new("/r/none")).as_deref(), Some("a"));
        // Two tabs share the title: the one in the session's folder, and no guess without one.
        assert_eq!(pick_by_title(&tabs, "Fix the login redirect", Path::new("/r/app3")).as_deref(), Some("e"));
        assert_eq!(pick_by_title(&tabs, "Bump dependencies", Path::new("/r/app2")), None);
        // No title matches: the one tab in the folder, and no guess between two.
        assert_eq!(pick_by_title(&tabs, "Something else", Path::new("/r/app3")).as_deref(), Some("e"));
        assert_eq!(pick_by_title(&tabs, "Something else", Path::new("/r/app4")), None);
        assert_eq!(pick_by_title(&tabs, "Something else", Path::new("/r/none")), None);
    }

    #[test]
    fn a_newer_focus_goes_ahead_of_one_still_waiting() {
        let turns = Turns::new();
        let first = turns.ticket();
        let held = turns.wait(first).expect("first in line");
        let second = turns.ticket();
        let third = turns.ticket();
        drop(held);
        assert!(turns.wait(second).is_none(), "a click made while another waited is dropped");
        assert!(turns.wait(third).is_some(), "the newest click goes ahead");
    }

    #[test]
    fn a_raise_waits_for_its_app_and_gives_way_to_a_newer_click() {
        use std::cell::Cell;
        let limit = Duration::from_secs(5);
        assert!(wait_until(limit, || true, || false), "the app is active already");
        // Active on the third look, as an app brought forward a moment after its window is.
        let looks = Cell::new(0);
        let ready = || {
            looks.set(looks.get() + 1);
            looks.get() >= 3
        };
        assert!(wait_until(limit, ready, || false));
        assert_eq!(looks.get(), 3);
        assert!(!wait_until(limit, || true, || true), "a newer click drops it, active app or not");
        assert!(!wait_until(Duration::ZERO, || false, || false), "the app never came forward");
    }

    #[test]
    fn a_tmux_pane_is_found_by_its_tty() {
        let list = "/dev/ttys003 %0 @0 1 main\n/dev/ttys012 %4 @3 0 my agents\n";
        let pane = |id: &str, window: &str, shown_in: &[&str]| TmuxPane { id: id.into(), window: window.into(), shown_in: shown_in.iter().map(|s| s.to_string()).collect(), server: TmuxServer::Yours };
        assert_eq!(tmux_pane_for(list, "/dev/ttys012", TmuxServer::Yours), Some(pane("%4", "@3", &[])));
        assert_eq!(tmux_pane_for(list, "/dev/ttys003", TmuxServer::Yours), Some(pane("%0", "@0", &["main"])));
        assert_eq!(tmux_pane_for(list, "/dev/ttys01", TmuxServer::Yours), None);
        // A window in several sessions (a group's, or linked) is listed once in each: it shows
        // wherever any of them is attached, whichever comes first.
        let grouped = "/dev/ttys025 %0 @0 0 agents\n/dev/ttys029 %1 @1 0 agents\n/dev/ttys025 %0 @0 1 view\n/dev/ttys025 %0 @0 2 also\n";
        assert_eq!(tmux_pane_for(grouped, "/dev/ttys025", TmuxServer::Yours), Some(pane("%0", "@0", &["view", "also"])));
        // A pane in another server is that server's.
        let theirs = TmuxServer::Socket(PathBuf::from("/s/tmux.sock"));
        assert_eq!(tmux_pane_for(list, "/dev/ttys012", theirs.clone()).map(|p| p.server), Some(theirs));
    }

    #[test]
    fn a_panes_server_is_the_one_its_tmux_variable_names() {
        assert_eq!(tmux_socket_in("/private/tmp/tmux-501/default,1234,0"), Some(PathBuf::from("/private/tmp/tmux-501/default")));
        // A socket's path may have commas and spaces of its own.
        assert_eq!(tmux_socket_in("/Users/a,b/Library/Application Support/Codealong/tmux.sock,98760,3"), Some(PathBuf::from("/Users/a,b/Library/Application Support/Codealong/tmux.sock")));
        assert_eq!(tmux_socket_in("nonsense"), None);
        assert_eq!(tmux_socket_in(",1,2"), None);
    }

    /// The tick looks for what waits for a tab as the screen is unlocked, on its first look when
    /// it isn't locked (after a restart, or a start the command made with the app closed), and
    /// once a session was marked since; never while it's locked or a look is under way, what's
    /// due waiting for that.
    #[test]
    fn tabs_are_looked_for_as_the_screen_is_unlocked_and_once_one_is_marked() {
        // `(locked, marked, looks)` tick by tick, with a look's end after each look.
        let ticks = |steps: &[(bool, bool, bool)], ends: bool| {
            let mut looks = TabLooks::default();
            for (i, &(locked, marked, want)) in steps.iter().enumerate() {
                assert_eq!(looks.on_tick(locked, marked), want, "tick {i}: locked {locked}, marked {marked}");
                if want && ends {
                    looks.on_looked(false);
                }
            }
        };
        // Its first look, unlocked; then nothing new until a lock and an unlock.
        ticks(&[(false, false, true), (false, false, false), (true, false, false), (true, false, false), (false, false, true), (false, false, false)], true);
        // Started locked: nothing while locked, the look as it's unlocked.
        ticks(&[(true, false, false), (true, true, false), (false, false, true)], true);
        // A lock between two ticks, a session marked in it: the mark alone brings the look.
        ticks(&[(false, false, true), (false, true, true), (false, false, false)], true);
        // A look under way: nothing more until it ends, and then what came due meanwhile.
        let mut looks = TabLooks::default();
        assert!(looks.on_tick(false, false));
        assert!(!looks.on_tick(false, true), "one look at a time");
        assert!(!looks.on_tick(true, false));
        assert!(!looks.on_tick(false, false), "still looking");
        looks.on_looked(false);
        assert!(looks.on_tick(false, false), "the mark and the unlock that came meanwhile");
        assert!(!looks.on_tick(false, false));
        // A look the screen locked partway through is due again, at the next tick it isn't locked,
        // though no tick saw the lock.
        looks.on_looked(true);
        assert!(looks.on_tick(false, false));
        looks.on_looked(true);
        assert!(!looks.on_tick(true, false));
        assert!(looks.on_tick(false, false));
    }

    #[test]
    fn what_waits_for_a_tab_is_read_from_tmux() {
        let list = "$4\t%9\t0\tfront\t/r/my app\tapp · fix\n$4\t%10\t0\tfront\t/r/my app\tapp · fix\n$5\t%11\t1\tbehind\t/r/b\tb\n$6\t%12\t0\t\t/r/c\tc\nnonsense\n";
        let waiting = tmux_waiting_in(list);
        let hosted = |session: &str, pane: &str| tmux::Hosted { session: session.into(), pane: pane.into() };
        assert_eq!(
            waiting,
            [
                (tmux::Waiting { hosted: hosted("$4", "%9"), dir: PathBuf::from("/r/my app"), label: "app · fix".into(), behind: false }, false),
                (tmux::Waiting { hosted: hosted("$5", "%11"), dir: PathBuf::from("/r/b"), label: "b".into(), behind: true }, true),
            ],
            "a session once however many panes it has, and none that isn't marked"
        );
    }

    /// Where a session Codealong starts in a terminal runs: in tmux where `[apps] tmux` says,
    /// and while the screen is locked when it may and can; otherwise in the terminal app, unless
    /// the screen is locked, when nothing can run it. Linux's terminal is tmux already.
    #[test]
    fn a_session_runs_in_tmux_as_the_setting_and_the_screen_say() {
        use TmuxUse::{Always, Never, WhenLocked};
        let locked = |tmux_installed| Err(Unhosted::Locked { tmux_installed });
        for (use_tmux, locked_now, has_tmux, want) in [
            (Always, false, true, Ok(Host::Tmux)),
            (Always, true, true, Ok(Host::Tmux)),
            (Always, false, false, Err(Unhosted::NoTmux)),
            (WhenLocked, false, true, Ok(Host::App)),
            (WhenLocked, true, true, Ok(Host::Tmux)),
            (WhenLocked, false, false, Ok(Host::App)),
            (WhenLocked, true, false, locked(false)),
            (Never, false, true, Ok(Host::App)),
            (Never, true, true, locked(true)),
            (Never, true, false, locked(false)),
        ] {
            assert_eq!(host_rule(Os::MacOs, use_tmux, locked_now, has_tmux), want, "{use_tmux:?}, locked {locked_now}, tmux {has_tmux}");
            assert_eq!(host_rule(Os::Linux, use_tmux, locked_now, has_tmux), Ok(Host::App));
        }
        // What a start says of each, in the terminal's words.
        assert!(Unhosted::Locked { tmux_installed: false }.words("Ghostty").contains("brew install tmux"));
        assert!(Unhosted::Locked { tmux_installed: true }.words("Ghostty").contains("Run sessions in tmux"));
        assert!(Unhosted::NoTmux.words("Ghostty").starts_with("Settings › General says to run sessions in tmux"));
    }

    /// A session to stay out of sight runs in tmux with no tab wherever tmux is installed, unless
    /// the setting says never; Linux's terminal is tmux whatever it says.
    #[test]
    fn a_session_out_of_sight_runs_in_tmux_with_no_tab_unless_told_never() {
        use TmuxUse::{Always, Never, WhenLocked};
        for (use_tmux, has_tmux, want) in [(Always, true, true), (WhenLocked, true, true), (Never, true, false), (WhenLocked, false, false), (Always, false, false)] {
            assert_eq!(out_of_sight_rule(Os::MacOs, use_tmux, has_tmux), want, "{use_tmux:?}, tmux {has_tmux}");
            assert_eq!(out_of_sight_rule(Os::Linux, use_tmux, has_tmux), has_tmux);
        }
    }

    #[test]
    fn a_tmux_sessions_client_is_the_one_used_last() {
        let list = "1760000100 501 /dev/ttys004 my agents\n1760000900 502 /dev/ttys005 main\n1760000300 503 /dev/ttys006 my agents\n";
        let on = |names: &[&str]| tmux_client_for(list, &names.iter().map(|n| n.to_string()).collect::<Vec<_>>());
        let client = |pid: u32, tty: &str, session: &str| Some(TmuxClient { pid, tty: PathBuf::from(tty), session: session.into() });
        assert_eq!(on(&["my agents"]), client(503, "/dev/ttys006", "my agents"));
        assert_eq!(on(&["main"]), client(502, "/dev/ttys005", "main"));
        assert_eq!(on(&["my agents", "main"]), client(502, "/dev/ttys005", "main"));
        assert_eq!(on(&["my"]), None);
        assert_eq!(on(&[]), None);
        assert_eq!(tmux_client_for("", &["main".to_string()]), None);
    }
}
