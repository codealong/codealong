//! tmux through its own command. It isn't an app sessions open in: its server leaves the terminal
//! it started in, and its panes show in whatever terminal a client of it runs in (`tmux attach`).
//! So a session's pane is found by its process's TTY and typed into there, and its tab is the one
//! its client runs in. A session no client shows is in no tab at all, until a new one attaches.
//!
//! A pane is asked of the server it's in, which its process's `TMUX` names (`Server::of`): yours,
//! or another, Codealong's own among them. Codealong starts sessions in a server of its own
//! (`hosted`), with settings of its own (`CONFIG`), when the screen is locked and a terminal app
//! can't start one, or for every start where `[apps] tmux` says so (launch.rs `host_for`), and on
//! Linux, where a terminal is always one of its sessions (linux.rs). A terminal app shows such a
//! session by attaching to it (`Hosted::attach_line`); one started while the screen is locked
//! waits for that, marked in tmux (`WAITS_FOR_TAB`). It's where a host of Codealong's own, one
//! that keeps a session's terminal itself, would go.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use anyhow::{anyhow, bail, Result};

use super::{fresh_line, launcher, tty_of, typed_line, wait_for_shell, HostKind, Key, Submit, Terminal, TerminalHost, Typed, CLEAR_WAIT};
use crate::procs;
use crate::util::sh_quote;

const NAME: &str = "tmux";

pub struct Tmux;

impl Terminal for Tmux {
    fn kind(&self) -> HostKind {
        HostKind::Tmux
    }

    fn name(&self) -> &'static str {
        NAME
    }

    fn installed(&self) -> bool {
        bin().is_some()
    }

    /// tmux's server leaves the terminal it started in, so nothing above it is one. On Linux tmux
    /// renames its processes `tmux: server` and `tmux: client`.
    fn runs_as(&self, process: &str) -> bool {
        process == "tmux" || process.starts_with("tmux: ")
    }

    /// Everything goes through its command, which has to be where Codealong looks.
    fn reachable(&self) -> bool {
        self.installed()
    }

    /// The tab of the client that shows its session (the one used last, where several do), with
    /// its pane brought up there, found the way that client's terminal finds a TTY. A client in a
    /// terminal Codealong doesn't know, or in tmux again, leaves the tab titled like the session
    /// in `profile`'s app. A session no client shows has no tab: `attach_command` makes one.
    fn focus_session(&self, profile: &str, pid: u32, _tty: &Path, title: &str, cwd: &Path) -> Result<Option<TerminalHost>> {
        if let Some((pane, client)) = shown_by(pid) {
            show(&pane, &client.session);
            if let Some(found) = client_terminal(client.pid).map(|t| t.focus_session(profile, client.pid, &client.tty, title, cwd)).transpose()?.flatten() {
                return Ok(Some(found));
            }
        }
        super::focus_by_title(profile, title, cwd)
    }

    /// The app its client runs in. None when no client shows its session, so a new tab opens in
    /// the terminal config.toml names.
    fn app_of(&self, pid: u32) -> Option<&'static str> {
        shown_by(pid).and_then(|(_, client)| client_terminal(client.pid)?.bundle())
    }

    /// Attaching to the pane, not its session, brings up the pane's window and the pane, where a
    /// session's name would show whichever window it was on last. Closing the tab leaves the
    /// session running, as it was before the tab.
    fn attach_command(&self, pid: u32) -> Option<String> {
        let pane = pane_of(pid).filter(|p| p.shown_in.is_empty())?;
        attach_line(&pane.server, &pane.id, None)
    }

    fn type_session(&self, pid: u32, _cwd: &Path, text: &str, clear: bool, submit: Submit, _passes: bool) -> Result<Typed> {
        match pane_of(pid) {
            Some(pane) => {
                let at = pane.at();
                if clear {
                    self.press(&at, Key::CtrlC)?;
                    std::thread::sleep(CLEAR_WAIT);
                }
                type_text(&pane.server, &pane.id, text, submit).map(|_| Typed::Into(NAME))
            }
            None => Ok(Typed::Nowhere("Codealong couldn't find its tmux pane".into())),
        }
    }

    fn presses_keys(&self) -> bool {
        true
    }

    fn key_target(&self, pid: u32, _cwd: &Path) -> Result<Option<String>> {
        Ok(pane_of(pid).map(|p| p.at()))
    }

    fn press(&self, at: &str, key: Key) -> Result<()> {
        let name = match key {
            Key::Digit(_) => key.typed(),
            Key::Escape => "Escape",
            Key::CtrlC => "C-c",
            Key::End => "End",
            Key::Left => "Left",
            Key::Return => "Enter",
        };
        let (server, pane) = Server::at(at);
        server.run(&["send-keys", "-t", pane, name])?;
        Ok(())
    }

    /// A pane is closed where it is, in any server: killing it hangs up what runs there, as a
    /// tab closing does, and a session left with no pane ends, its client's tab closing as the
    /// client exits.
    fn closes_tabs(&self) -> bool {
        true
    }

    fn close_sessions(&self, sessions: &[(u32, &Path)]) -> Result<Vec<Result<()>>> {
        Ok(sessions
            .iter()
            .map(|(pid, _)| match pane_of(*pid) {
                Some(pane) => pane.server.run(&["kill-pane", "-t", &pane.id]).map(|_| ()),
                None => Err(anyhow!("Couldn't find its tmux pane.")),
            })
            .collect())
    }
}

pub(super) fn bin() -> Option<PathBuf> {
    #[cfg(test)]
    if let Some(staged) = super::stage::tmux_bin() {
        return Some(staged);
    }
    ["/opt/homebrew/bin/tmux", "/usr/local/bin/tmux", "/usr/bin/tmux"].iter().map(PathBuf::from).find(|p| p.is_file())
}

fn tmux_or_say() -> Result<String> {
    Ok(bin().ok_or_else(|| anyhow!("tmux isn't installed where Codealong looks for it."))?.to_string_lossy().into_owned())
}

/// A tmux server, by what picks it on tmux's command line.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Server {
    /// The one `tmux` finds by itself: yours.
    Yours,
    /// One by its socket's path (`-S`).
    Socket(PathBuf),
}

impl Server {
    /// The server a process runs under, from the `TMUX` tmux sets in each pane's environment
    /// (`<socket>,<server's pid>,<session>`), which what the pane runs keeps. None where it can't
    /// be read: macOS shows no other process the environment of its own programs (a pane's
    /// `/bin/zsh` read empty, 2026-10-18), where an agent's is there to read.
    fn of(pid: u32) -> Option<Server> {
        #[cfg(test)]
        if let Some(staged) = super::stage::tmux_var(pid) {
            return match staged.as_deref() {
                None => Some(Server::Yours),
                Some("") => None,
                Some(var) => socket_in(var).map(Server::Socket),
            };
        }
        procs::environment(pid).and_then(|env| socket_in(env.get("TMUX")?)).map(Server::Socket)
    }

    /// The server and the pane in a key target (`Pane::at`).
    fn at(at: &str) -> (Server, &str) {
        match at.split_once(' ') {
            Some((pane, socket)) => (Server::Socket(PathBuf::from(socket)), pane),
            None => (Server::Yours, at),
        }
    }

    /// What goes before a command to ask this server.
    fn args(&self) -> Vec<String> {
        match self {
            Server::Yours => Vec::new(),
            Server::Socket(path) => vec!["-S".into(), path.to_string_lossy().into_owned()],
        }
    }

    fn with(&self, args: &[&str]) -> Vec<String> {
        self.args().into_iter().chain(args.iter().map(|a| a.to_string())).collect()
    }

    fn run(&self, args: &[&str]) -> Result<String> {
        self.run_within(args, Duration::from_secs(5))
    }

    fn run_within(&self, args: &[&str], wait: Duration) -> Result<String> {
        let all = self.with(args);
        procs::run(&tmux_or_say()?, &all.iter().map(String::as_str).collect::<Vec<_>>(), None, wait)
    }

    fn run_stdin(&self, args: &[&str], input: &str) -> Result<String> {
        let all = self.with(args);
        procs::run_stdin(&tmux_or_say()?, &all.iter().map(String::as_str).collect::<Vec<_>>(), input, Duration::from_secs(5))
    }
}

/// The socket in a `TMUX` value. Its path may have commas, so the two numbers come off the end.
pub(super) fn socket_in(var: &str) -> Option<PathBuf> {
    let mut parts = var.rsplitn(3, ',');
    let (_, _, path) = (parts.next()?, parts.next()?, parts.next()?);
    (!path.is_empty()).then(|| PathBuf::from(path))
}

/// A pane, as `list-panes` says it: its id, its window's, the sessions it's in that a client
/// shows, and the server it's in. A window can be in several sessions (linked, or a group's), and
/// is listed once in each.
#[derive(Debug, PartialEq)]
pub(super) struct Pane {
    pub id: String,
    pub window: String,
    pub shown_in: Vec<String>,
    pub server: Server,
}

impl Pane {
    /// What `press` takes to find it again: its id, and its server's socket after a space where
    /// it isn't yours. A pane's id has no space in it.
    fn at(&self) -> String {
        match &self.server {
            Server::Socket(path) => format!("{} {}", self.id, path.to_string_lossy()),
            Server::Yours => self.id.clone(),
        }
    }
}

/// What `list-panes` says of each pane, the session's name last since it may have spaces.
const PANES: &str = "#{pane_tty} #{pane_id} #{window_id} #{session_attached} #{session_name}";

/// The tmux pane whose TTY is the process's, in the server it runs under, or where that can't be
/// told, in Codealong's or yours.
fn pane_of(pid: u32) -> Option<Pane> {
    let tty = tty_of(pid)?;
    let servers = Server::of(pid).map_or_else(|| vec![hosted(), Server::Yours], |s| vec![s]);
    servers.into_iter().find_map(|server| {
        let out = server.run(&["list-panes", "-a", "-F", PANES]).ok()?;
        pane_for(&out, &tty.to_string_lossy(), server)
    })
}

/// From `list-panes -F PANES`, the pane on `tty`, from every line that lists it.
pub(super) fn pane_for(list: &str, tty: &str, server: Server) -> Option<Pane> {
    let mut pane: Option<Pane> = None;
    for line in list.lines() {
        let mut f = line.splitn(5, ' ');
        let (Some(t), Some(id), Some(window), Some(clients), Some(session)) = (f.next(), f.next(), f.next(), f.next(), f.next()) else { continue };
        if t != tty {
            continue;
        }
        let pane = pane.get_or_insert_with(|| Pane { id: id.to_string(), window: window.to_string(), shown_in: Vec::new(), server: server.clone() });
        if clients != "0" {
            pane.shown_in.push(session.to_string());
        }
    }
    pane
}

/// A client of tmux (a terminal running `tmux attach`): its process, its TTY and the session it
/// shows.
#[derive(Debug, PartialEq)]
pub(super) struct Client {
    pub pid: u32,
    pub tty: PathBuf,
    pub session: String,
}

/// What `list-clients` says of each client, the session's name last since it may have spaces.
const CLIENTS: &str = "#{client_activity} #{client_pid} #{client_tty} #{client_session}";

/// Where a process's pane shows: the pane, and the client showing a session it's in that was
/// used last. None when no client shows it.
fn shown_by(pid: u32) -> Option<(Pane, Client)> {
    let pane = pane_of(pid).filter(|p| !p.shown_in.is_empty())?;
    let out = pane.server.run(&["list-clients", "-F", CLIENTS]).ok()?;
    let client = client_for(&out, &pane.shown_in)?;
    Some((pane, client))
}

/// From `list-clients -F CLIENTS`, the client on one of `sessions` with the latest activity.
pub(super) fn client_for(list: &str, sessions: &[String]) -> Option<Client> {
    let clients = list.lines().filter_map(|l| {
        let mut f = l.splitn(4, ' ');
        let (at, pid, tty, session) = (f.next()?.parse::<i64>().ok()?, f.next()?.parse::<u32>().ok()?, f.next()?, f.next()?);
        sessions.iter().any(|s| s == session).then(|| (at, Client { pid, tty: PathBuf::from(tty), session: session.to_string() }))
    });
    clients.max_by_key(|c| c.0).map(|(_, client)| client)
}

/// The terminal a client runs in. The client is a tmux process too, so it's the one above it.
/// One in tmux again counts as none, so a client inside its own session can't loop.
fn client_terminal(client: u32) -> Option<&'static dyn Terminal> {
    super::parent_and_name(client).and_then(|(parent, _)| super::terminal_kind(parent)).filter(|t| t.kind() != HostKind::Tmux)
}

/// Makes the pane the one `session` shows: its window the current one there, and it the active
/// pane in that window. The window is named in the session (`=` for its exact name): a pane
/// alone names the session with it that was active last, which may be one no client shows.
/// Failing leaves the client as it was, and its tab still comes forward.
fn show(pane: &Pane, session: &str) {
    let window = format!("={session}:{}", pane.window);
    for (command, target) in [("select-window", window.as_str()), ("select-pane", pane.id.as_str())] {
        let _ = pane.server.run(&[command, "-t", target]);
    }
}

/// Pastes `text` into the pane (bracketed, so a prompt of several lines arrives whole), then
/// presses Return a moment later, as `GHOSTTY_TYPE` does. Each paste has a buffer of its own:
/// a server's buffers are every session's, and two sends at once through one name could paste
/// one's text into the other's pane.
fn type_text(server: &Server, pane: &str, text: &str, submit: Submit) -> Result<()> {
    static PASTES: AtomicU64 = AtomicU64::new(0);
    let buffer = format!("codealong-{}-{}", std::process::id(), PASTES.fetch_add(1, Ordering::SeqCst));
    server.run_stdin(&["load-buffer", "-b", &buffer, "-"], text)?;
    server.run(&["paste-buffer", "-p", "-d", "-b", &buffer, "-t", pane])?;
    std::thread::sleep(Duration::from_millis(300));
    match submit {
        Submit::Return => server.run(&["send-keys", "-t", pane, "Enter"])?,
        // Each character by its code (`-H`), so a control character goes as the key it is.
        Submit::Keys(keys) => {
            let codes: Vec<String> = keys.bytes().map(|b| format!("{b:02x}")).collect();
            let mut args = vec!["send-keys", "-t", pane, "-H"];
            args.extend(codes.iter().map(String::as_str));
            server.run(&args)?
        }
    };
    Ok(())
}

/// Codealong's own server, by a socket in its support folder (beside `codealong.sock`), shared
/// by Codealong and Codealong Dev as their other files are, and out of `/tmp`, whose old files
/// macOS clears. A socket's path holds at most 103 bytes, so a home whose path is longer than
/// that allows keeps it in a folder of Codealong's own in `/tmp` instead (`start` makes it, for
/// you alone).
pub(super) fn hosted() -> Server {
    #[cfg(test)]
    if let Some(staged) = super::stage::hosted_socket() {
        return Server::Socket(staged);
    }
    let socket = support_dir().join("tmux.sock");
    if socket.as_os_str().len() < 104 {
        Server::Socket(socket)
    } else {
        // SAFETY: getuid can't fail.
        Server::Socket(PathBuf::from(format!("/tmp/codealong-{}", unsafe { libc::getuid() })).join("tmux.sock"))
    }
}

/// Makes the folder Codealong's server's socket goes in, readable by you alone where it's one
/// of its own in `/tmp`.
fn socket_folder(server: &Server) -> Result<()> {
    #[cfg(test)]
    if super::stage::hosted_socket().is_some() {
        return Ok(());
    }
    let Server::Socket(socket) = server else { return Ok(()) };
    let Some(dir) = socket.parent() else { return Ok(()) };
    std::fs::create_dir_all(dir)?;
    if dir.starts_with("/tmp") {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

/// The settings Codealong's server starts with (`-f`). No prefix key or status line, so tmux
/// stays out of sight and every key reaches the agent (Claude Code's Ctrl+B among them); the
/// mouse wheel scrolls back through what the pane printed; the keys, clipboard, links and
/// passthrough agents use get to the terminal that shows the pane, as Claude Code's own setup
/// for tmux asks; the tab takes the title the program sets; a session no terminal shows yet is
/// drawn at a size of its own until one attaches and its size takes over; colours are full.
const CONFIG: &str = "\
set -g prefix None
set -g prefix2 None
set -g status off
set -g mouse on
set -g history-limit 50000
set -s escape-time 0
set -s extended-keys on
set -as terminal-features 'xterm*:extkeys:hyperlinks:sync:RGB'
set -g allow-passthrough on
set -g set-clipboard on
set -g focus-events on
set -g set-titles on
set -g set-titles-string '#{pane_title}'
set -g default-size 200x50
set -g window-size latest
set -g aggressive-resize on
set-environment -g COLORTERM truecolor
";

#[cfg(test)]
thread_local! {
    /// The folder a real test's server keeps its socket and settings in, in place of Codealong's.
    static TEST_SUPPORT: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}

/// Where Codealong's server keeps its socket and settings: Codealong's support folder.
fn support_dir() -> PathBuf {
    #[cfg(test)]
    if let Some(dir) = TEST_SUPPORT.with(|d| d.borrow().clone()) {
        return dir;
    }
    crate::os::dirs::support_dir()
}

/// The settings' file, written as `CONFIG` says where it says something else. tmux reads it only
/// as the server starts.
fn config_file() -> Result<PathBuf> {
    #[cfg(test)]
    if let Some(staged) = super::stage::hosted_config() {
        return Ok(staged);
    }
    let path = support_dir().join("tmux.conf");
    if std::fs::read_to_string(&path).ok().as_deref() != Some(CONFIG) {
        std::fs::create_dir_all(path.parent().unwrap_or(Path::new("/")))?;
        std::fs::write(&path, CONFIG)?;
    }
    Ok(path)
}

/// A session Codealong started in its server: the session's id and its pane's.
#[derive(Debug, Clone, PartialEq)]
pub struct Hosted {
    pub session: String,
    pub pane: String,
}

impl Hosted {
    /// What a terminal runs to show it (`attach_line`), its session ending once no terminal
    /// shows it any more where `ends_with_tab`, as a tab's own session ends as the tab closes.
    pub fn attach_line(&self, ends_with_tab: bool) -> Option<String> {
        attach_line(&hosted(), &self.pane, ends_with_tab.then_some(self.session.as_str()))
    }

    /// Marks it as waiting for a tab of its terminal, `behind` your windows or not, once the
    /// screen is unlocked (`waiting_for_tabs`).
    pub fn wait_for_tab(&self, behind: bool) -> Result<()> {
        hosted().run(&["set-option", "-t", &self.session, WAITS_FOR_TAB, if behind { "behind" } else { "front" }]).map(|_| ())
    }
}

/// The session option that marks a session started while the screen was locked as waiting for a
/// tab (`front`, or `behind` your windows). It's kept in tmux, with the session, so whichever
/// process sees the unlock opens the tab: the app, after a restart too, and for a start the
/// `codealong` command made with the app closed.
const WAITS_FOR_TAB: &str = "@codealong-tab";

/// A session of Codealong's server that waits for a tab: the session, its folder, the name its
/// window (and so its tab) goes by, and whether the tab goes behind your windows.
#[derive(Debug, Clone, PartialEq)]
pub struct Waiting {
    pub hosted: Hosted,
    pub dir: PathBuf,
    pub label: String,
    pub behind: bool,
}

/// What `list-panes` says of each pane for `waiting_in`, tab-separated, since a folder or a name
/// may have spaces.
const WAITING: &str = "#{session_id}\t#{pane_id}\t#{session_attached}\t#{@codealong-tab}\t#{session_path}\t#{window_name}";

/// From `list-panes -F WAITING`, each session that waits for a tab, once, and whether a terminal
/// shows it already.
pub(super) fn waiting_in(list: &str) -> Vec<(Waiting, bool)> {
    let mut out: Vec<(Waiting, bool)> = Vec::new();
    for line in list.lines() {
        let f: Vec<&str> = line.splitn(6, '\t').collect();
        let [session, pane, attached, tab, dir, label] = f[..] else { continue };
        if tab.is_empty() || out.iter().any(|(w, _)| w.hosted.session == session) {
            continue;
        }
        let waiting = Waiting { hosted: Hosted { session: session.into(), pane: pane.into() }, dir: PathBuf::from(dir), label: label.into(), behind: tab == "behind" };
        out.push((waiting, attached != "0"));
    }
    out
}

/// The sessions of Codealong's server that wait for a tab and that no terminal shows yet, each
/// waiting no longer once listed: one Show in opened meanwhile needs none.
pub fn waiting_for_tabs() -> Vec<Waiting> {
    let server = hosted();
    let Ok(list) = server.run(&["list-panes", "-a", "-F", WAITING]) else { return Vec::new() };
    let mut out = Vec::new();
    for (waiting, shown) in waiting_in(&list) {
        if server.run(&["set-option", "-u", "-t", &waiting.hosted.session, WAITS_FOR_TAB]).is_ok() && !shown {
            out.push(waiting);
        }
    }
    out
}

/// What a terminal runs to show a pane: attaching to it. With `ends`, the session ends once no
/// client shows it, from the moment this one attaches: on a session nobody shows yet, tmux would
/// end it at once (3.7c).
fn attach_line(server: &Server, pane: &str, ends: Option<&str>) -> Option<String> {
    let mut line = bin()?.to_string_lossy().into_owned();
    for arg in server.args() {
        line.push(' ');
        line.push_str(&if arg.starts_with('-') { arg } else { sh_quote(&arg) });
    }
    line.push_str(&format!(" attach -t {}", sh_quote(pane)));
    if let Some(session) = ends {
        line.push_str(&format!(" ';' set-option -t {} destroy-unattached on", sh_quote(session)));
    }
    Some(line)
}

/// Whether a line is one `attach_line` made, which a terminal runs as it is rather than in a
/// tmux session of its own (linux.rs).
pub(super) fn attaches(line: &str) -> bool {
    bin().is_some_and(|b| line.starts_with(&format!("{} ", b.to_string_lossy()))) && line.contains(" attach -t ")
}

/// A new session's name, one per start.
fn session_name() -> String {
    static STARTED: AtomicU64 = AtomicU64::new(0);
    format!("codealong-{}-{}", std::process::id(), STARTED.fetch_add(1, Ordering::SeqCst))
}

/// Starts a session in Codealong's server at `dir`, as a new tab's would start: the launcher sets
/// its folder and `env` and becomes your login shell, which sets up the PATH and the rest as it
/// does in a terminal (Codealong's own environment is the thin one apps get), and `input` (a
/// command plus return) is typed once that shell reads keys. `label` names its window. Nothing
/// shows it until a terminal attaches.
pub fn start(dir: &Path, input: &str, env: &[(String, String)], label: Option<&str>) -> Result<Hosted> {
    let server = hosted();
    socket_folder(&server)?;
    let launcher = launcher(dir, &with_locale(env, has_locale()))?;
    let shell = format!("/bin/sh {}", sh_quote(&launcher.to_string_lossy()));
    let conf = config_file()?.to_string_lossy().into_owned();
    let (name, dir) = (session_name(), dir.to_string_lossy());
    // Starting the server as well can take a while on a busy Mac.
    let made = server.run_within(&["-f", &conf, "new-session", "-d", "-P", "-F", "#{session_id} #{pane_id} #{pane_tty}", "-s", &name, "-n", label.unwrap_or("codealong"), "-c", &dir, &shell], Duration::from_secs(10))?;
    let mut ids = made.split_whitespace();
    let (Some(session), Some(pane), Some(tty)) = (ids.next(), ids.next(), ids.next()) else { bail!("tmux didn't say which session it started: {made:?}") };
    let hosted = Hosted { session: session.to_string(), pane: pane.to_string() };
    if !input.is_empty() {
        if !wait_for_shell(tty, Some(&launcher)) {
            bail!("The new terminal's shell didn't start within 15 seconds.");
        }
        server.run(&["send-keys", "-t", pane, "-l", &fresh_line(typed_line(input))])?;
        server.run(&["send-keys", "-t", pane, "Enter"])?;
    }
    Ok(hosted)
}

/// `env` with a UTF-8 character set where Codealong's environment has no locale at all
/// (`has_locale`), as an app opened from the Dock's hasn't: a terminal app sets one for its
/// shells, and without one a program in the pane may take what it reads and prints for ASCII.
/// Linux's desktop session sets one.
fn with_locale(env: &[(String, String)], has_locale: bool) -> Vec<(String, String)> {
    let mut out = env.to_vec();
    if crate::os::is_macos() && !has_locale {
        out.push(("LC_CTYPE".into(), "UTF-8".into()));
    }
    out
}

/// Whether Codealong's own environment names a locale.
fn has_locale() -> bool {
    #[cfg(test)]
    if super::stage::tmux_bin().is_some() {
        return true;
    }
    ["LC_ALL", "LC_CTYPE", "LANG"].iter().any(|k| std::env::var(k).is_ok_and(|v| !v.is_empty()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    /// `cargo test a_session_runs_in_codealongs_own_tmux_server -- --ignored` starts a real session
    /// in a server of the test's own (your login shell, in a scratch folder), types into it the way
    /// a send does, then shows it as a tab would and closes that client: the server took
    /// Codealong's settings, what runs there names the server, a send reaches the program in the
    /// pane, and the session ends once the tab that ends it is gone. Nothing opens on screen.
    #[test]
    #[ignore]
    fn a_session_runs_in_codealongs_own_tmux_server() {
        let scratch = crate::fixtures::scratch("tmux");
        TEST_SUPPORT.with(|d| *d.borrow_mut() = Some(scratch.to_path_buf()));
        let server = hosted();
        let (named, typed) = (scratch.join("named"), scratch.join("typed"));
        let until = |what: &str, ok: &dyn Fn() -> bool| {
            let since = Instant::now();
            while !ok() {
                assert!(since.elapsed() < Duration::from_secs(20), "{what} never happened");
                std::thread::sleep(Duration::from_millis(100));
            }
        };
        let line = format!("echo \"$TMUX $CODEALONG_TEST\" > {}; cat > {}\n", sh_quote(&named.to_string_lossy()), sh_quote(&typed.to_string_lossy()));
        let hosted = start(&scratch, &line, &[("CODEALONG_TEST".into(), "it took".into())], Some("a test")).unwrap();
        until("the line ran", &|| std::fs::read_to_string(&named).is_ok_and(|n| !n.is_empty()));
        let said = std::fs::read_to_string(&named).unwrap();
        assert!(said.starts_with(&format!("{},", scratch.join("tmux.sock").to_string_lossy())), "{said}");
        assert!(said.trim_end().ends_with("it took"), "{said}");
        for (option, want) in [("prefix", "prefix None"), ("status", "status off"), ("history-limit", "history-limit 50000")] {
            assert_eq!(server.run(&["show-options", "-g", option]).unwrap().trim(), want);
        }
        // The pane's shell is found as a session's process is, and typed into as a send types.
        let shell: u32 = server.run(&["display-message", "-p", "-t", &hosted.pane, "#{pane_pid}"]).unwrap().trim().parse().unwrap();
        assert_eq!(pane_of(shell).map(|p| (p.id, p.server)), Some((hosted.pane.clone(), server.clone())));
        assert_eq!(Tmux.type_session(shell, &scratch, "hello there", false, Submit::Return, false).unwrap(), Typed::Into("tmux"));
        until("the send arrived", &|| std::fs::read_to_string(&typed).is_ok_and(|t| t == "hello there\n"));
        // Marked as waiting for a tab, it's listed once, with its folder and its window's name.
        hosted.wait_for_tab(true).unwrap();
        let waiting = waiting_for_tabs();
        assert_eq!(waiting.iter().map(|w| (&w.hosted, w.label.as_str(), w.behind)).collect::<Vec<_>>(), [(&hosted, "a test", true)]);
        assert_eq!(waiting[0].dir.canonicalize().unwrap(), scratch.canonicalize().unwrap());
        assert!(waiting_for_tabs().is_empty(), "it waits no longer once listed");
        // A tab that ends it, opened and closed.
        let shown = || server.run(&["display-message", "-p", "-t", &hosted.pane, "#{session_attached}"]).ok().map(|n| n.trim() != "0");
        let attach = hosted.attach_line(true).unwrap();
        let mut client = std::process::Command::new("/usr/bin/script").args(["-q", "/dev/null", "/bin/sh", "-c", &format!("exec {attach}")]).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).spawn().unwrap();
        until("the tab showed it", &|| shown() == Some(true));
        let _ = client.kill();
        let _ = client.wait();
        until("the session ended with its tab", &|| shown().is_none());
        let _ = server.run(&["kill-server"]);
    }
}
