//! The terminal on Linux, `[apps] terminal = "terminal"` there as Terminal is on macOS. Linux's
//! terminal emulators take no scripts, so a new terminal is a session in Codealong's own tmux
//! server, which Codealong types into and finds again through tmux (tmux.rs), shown in a window of
//! the first emulator found (`$TERMINAL` when it's one of `EMULATORS`, else the first of them on
//! the PATH) attached to it, and ending as that window closes. A Background start opens no window:
//! Show in attaches one later (`Tmux::attach_command`), which leaves it running as it closes. Show
//! in raises an emulator's window with `xdotool`, which only X11 has, and only where it's plainly
//! the session's: one emulator process can have many windows (GNOME Terminal's server has every
//! one) and a window many tabs, and nothing says which shows the session, so an emulator process
//! with more than one window or tab isn't taken for it. A session started by hand in
//! an emulator's own tab, outside tmux, is known to be there and no more: nothing can type into
//! it or find its tab.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{anyhow, bail, Result};

use super::{typed_line, HostKind, Placement, Submit, Terminal, TerminalHost, Typed};
use crate::procs;

/// A terminal emulator: its command, its name, the name its process has (cut to 15 characters,
/// as Linux keeps it) and what goes before a command it's to run.
struct Emulator {
    command: &'static str,
    name: &'static str,
    process: &'static str,
    runs: &'static [&'static str],
}

const EMULATORS: &[Emulator] = &[
    Emulator { command: "ghostty", name: "Ghostty", process: "ghostty", runs: &["-e"] },
    Emulator { command: "kitty", name: "kitty", process: "kitty", runs: &[] },
    Emulator { command: "wezterm", name: "WezTerm", process: "wezterm-gui", runs: &["start", "--"] },
    Emulator { command: "alacritty", name: "Alacritty", process: "alacritty", runs: &["-e"] },
    Emulator { command: "foot", name: "foot", process: "foot", runs: &[] },
    Emulator { command: "konsole", name: "Konsole", process: "konsole", runs: &["-e"] },
    Emulator { command: "gnome-terminal", name: "GNOME Terminal", process: "gnome-terminal-", runs: &["--"] },
    Emulator { command: "ptyxis", name: "Ptyxis", process: "ptyxis-agent", runs: &["-x"] },
    Emulator { command: "xfce4-terminal", name: "Xfce Terminal", process: "xfce4-terminal", runs: &["-x"] },
    Emulator { command: "xterm", name: "xterm", process: "xterm", runs: &["-e"] },
];

/// Where a program is on the user's PATH.
fn on_path(command: &str) -> Option<PathBuf> {
    #[cfg(test)]
    if let Some(staged) = super::stage::on_path(command) {
        return staged;
    }
    procs::user_path().split(':').map(|d| Path::new(d).join(command)).find(|p| p.is_file())
}

/// The emulator new windows open in: `$TERMINAL`'s, else the first on the PATH.
fn emulator() -> Option<(&'static Emulator, PathBuf)> {
    let named = std::env::var("TERMINAL").ok().and_then(|t| EMULATORS.iter().find(|e| Path::new(&t).file_name().is_some_and(|n| n == e.command)));
    named.into_iter().chain(EMULATORS.iter()).find_map(|e| Some((e, on_path(e.command)?)))
}

fn tmux() -> Option<PathBuf> {
    super::tmux::bin()
}

pub struct LinuxTerminal;

impl Terminal for LinuxTerminal {
    fn kind(&self) -> HostKind {
        HostKind::Linux
    }

    /// The emulator's name, or tmux's when it's tmux that's missing, for messages.
    fn name(&self) -> &'static str {
        match (tmux(), emulator()) {
            (None, _) => "tmux",
            (_, Some((e, _))) => e.name,
            (_, None) => "A terminal emulator",
        }
    }

    fn profile(&self) -> Option<&'static str> {
        Some("terminal")
    }

    fn installed(&self) -> bool {
        tmux().is_some() && emulator().is_some()
    }

    fn runs_as(&self, process: &str) -> bool {
        EMULATORS.iter().any(|e| e.process == process)
    }

    /// A session in Codealong's tmux server (tmux.rs `start`), shown in a window attached to it
    /// that ends it as it closes, as a tab's own session ends.
    fn run(&self, place: Placement, dir: &Path, input: &str, env: &[(String, String)]) -> Result<()> {
        if tmux().is_none() {
            bail!("tmux isn't installed, and Codealong opens terminals on Linux in it.");
        }
        let window = if place.behind { None } else { Some(emulator().ok_or_else(|| anyhow!("Codealong found no terminal emulator to open a window in."))?) };
        // A terminal opened to show a tmux session no window shows (`Tmux::attach_command`) is a
        // window attached to it: in a tmux session of its own, tmux would refuse to nest.
        if let (Some((emulator, path)), true) = (&window, super::tmux::attaches(typed_line(input))) {
            return show_in(emulator, path, &["/bin/sh", "-c", &format!("exec {}", typed_line(input))]);
        }
        let hosted = super::tmux::start(dir, input, env, place.label)?;
        if let (Some((emulator, path)), Some(attach)) = (window, hosted.attach_line(true)) {
            show_in(emulator, &path, &["/bin/sh", "-c", &format!("exec {attach}")])?;
        }
        Ok(())
    }

    /// Raises the window of the emulator the session runs in.
    fn focus_session(&self, _profile: &str, pid: u32, tty: &Path, _title: &str, _cwd: &Path) -> Result<Option<TerminalHost>> {
        let Some(emulator) = emulator_above(pid) else { return Ok(None) };
        Ok(raise(emulator).then(|| TerminalHost { terminal: &LinuxTerminal, at: tty.to_string_lossy().into_owned() }))
    }

    fn type_session(&self, _pid: u32, _cwd: &Path, _text: &str, _clear: bool, _submit: Submit, _passes: bool) -> Result<Typed> {
        Ok(Typed::Nowhere("its terminal emulator can't be typed into from outside, only a tmux session can".into()))
    }
}

/// What an emulator runs to show a tmux session in a window of its own.
fn show_command(emulator: &Emulator, path: &Path, command: &[&str]) -> Vec<String> {
    let mut args = vec![path.to_string_lossy().into_owned()];
    args.extend(emulator.runs.iter().chain(command).map(|a| a.to_string()));
    args
}

/// Opens a window of the emulator running `command`, left to run on its own.
fn show_in(emulator: &Emulator, path: &Path, command: &[&str]) -> Result<()> {
    let all = show_command(emulator, path, command);
    let args: Vec<&str> = all[1..].iter().map(String::as_str).collect();
    crate::os::open::start_detached(&all[0], &args, Duration::from_secs(10))
}

/// The nearest process above `pid` that's a terminal emulator.
fn emulator_above(pid: u32) -> Option<u32> {
    let mut at = pid;
    for _ in 0..16 {
        let (ppid, name) = super::parent_and_name(at)?;
        if LinuxTerminal.runs_as(&name) {
            return Some(at);
        }
        if ppid <= 1 {
            return None;
        }
        at = ppid;
    }
    None
}

/// Raises the window of this emulator process, through `xdotool`, when it has just one window and
/// in it one tab: each tab is a child of the emulator, its shell or the command it runs. False
/// otherwise, which would leave the session's to a guess, or without `xdotool`, as under Wayland.
fn raise(pid: u32) -> bool {
    let Ok(windows) = procs::run("xdotool", &["search", "--pid", &pid.to_string()], None, Duration::from_secs(5)) else { return false };
    let tabs = procs::run("pgrep", &["-P", &pid.to_string()], None, Duration::from_secs(5)).unwrap_or_default();
    match lone_window(&windows, &tabs) {
        Some(window) => procs::run("xdotool", &["windowactivate", window], None, Duration::from_secs(5)).is_ok(),
        None => false,
    }
}

/// The one window `xdotool search` found, when it found just one and `pgrep -P` one child.
fn lone_window<'a>(windows: &'a str, children: &str) -> Option<&'a str> {
    fn one(listed: &str) -> Option<&str> {
        let mut lines = listed.lines().map(str::trim).filter(|l| !l.is_empty());
        let first = lines.next()?;
        lines.next().is_none().then_some(first)
    }
    one(children)?;
    one(windows)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What a new terminal is sent: a session in Codealong's tmux server running the launcher, the
    /// line typed once its shell is up, and a window of the emulator attached to it that ends it
    /// as it closes; a Background one has no window.
    #[test]
    fn a_new_terminal_is_a_tmux_session_shown_in_an_emulator() {
        use super::super::stage::{play, Sent, Setup, HOSTED_CONFIG, HOSTED_SOCKET};
        use super::super::terminal_run;
        use crate::config::NewTerminal;
        let answer = |s: &Sent| Ok(if s.args.contains(&"new-session".to_string()) { "$4 %9 /dev/pts/7\n".to_string() } else { String::new() });
        let env = [("CODEALONG_PROMPT".to_string(), "Fix it".to_string())];
        let place = |behind| Placement { new_terminal: NewTerminal::Window, near: &[], behind, label: Some("app · fix") };
        let linux = || Setup { os: crate::os::Os::Linux, ..Setup::default() };
        let p = play(linux(), answer, || terminal_run("terminal", place(false), Path::new("/r/my app"), "claude --session-id x\n", &env));
        p.got.unwrap();
        let session = p.log[0].split('"').nth(21).expect("the session's name").to_string();
        assert!(session.starts_with("codealong-"), "{session}");
        let tmux = "/opt/homebrew/bin/tmux";
        let detached = crate::os::open::START_DETACHED;
        let on = format!(r#""-S", "{HOSTED_SOCKET}""#);
        assert_eq!(
            p.log,
            [
                format!(r##"{tmux} [{on}, "-f", "{HOSTED_CONFIG}", "new-session", "-d", "-P", "-F", "#{{session_id}} #{{pane_id}} #{{pane_tty}}", "-s", "{session}", "-n", "app · fix", "-c", "/r/my app", "/bin/sh 'LAUNCHER'"] (10s)"##),
                "wait for shell /dev/pts/7 after LAUNCHER".to_string(),
                format!(r#"{tmux} [{on}, "send-keys", "-t", "%9", "-l", "\u{{15}}claude --session-id x"] (5s)"#),
                format!(r#"{tmux} [{on}, "send-keys", "-t", "%9", "Enter"] (5s)"#),
                format!(r#"/bin/sh ["-c", {detached:?}, "/usr/bin/ghostty", "-e", "/bin/sh", "-c", "exec {tmux} -S '{HOSTED_SOCKET}' attach -t '%9' ';' set-option -t '$4' destroy-unattached on"] (10s)"#),
            ]
        );
        assert_eq!(p.launchers, ["rm -f 'LAUNCHER'\ncd '/r/my app' 2>/dev/null\nexport CODEALONG_PROMPT='Fix it'\nexec SHELL -l"]);
        // Behind your windows, it's the session alone; with nothing to type, nothing is waited for.
        let p = play(linux(), answer, || terminal_run("terminal", place(true), Path::new("/r/app"), "", &[]));
        p.got.unwrap();
        assert_eq!(p.log.len(), 1, "{:?}", p.log);
        // Showing a tmux session no window shows is a window attached to it, not a session of its own.
        let p = play(linux(), answer, || terminal_run("terminal", place(false), Path::new("/r/app"), &format!("{tmux} -S '{HOSTED_SOCKET}' attach -t '%3'\n"), &[]));
        p.got.unwrap();
        assert_eq!(p.log, [format!(r#"/bin/sh ["-c", {detached:?}, "/usr/bin/ghostty", "-e", "/bin/sh", "-c", "exec {tmux} -S '{HOSTED_SOCKET}' attach -t '%3'"] (10s)"#)]);
        // Without an emulator or tmux, it isn't installed.
        let p = play(Setup { installed: false, ..linux() }, answer, || terminal_run("terminal", place(false), Path::new("/r/app"), "ls\n", &[]));
        assert_eq!(p.got.unwrap_err().to_string(), "A terminal emulator isn't installed. Install it, or pick another terminal in Settings.");
    }

    #[test]
    fn an_emulator_attaches_to_its_session_as_it_takes_a_command() {
        let shown = |command: &str| show_command(EMULATORS.iter().find(|e| e.command == command).unwrap(), Path::new(&format!("/usr/bin/{command}")), &["/usr/bin/tmux", "attach", "-t", "=codealong-1-0"]);
        assert_eq!(shown("gnome-terminal"), ["/usr/bin/gnome-terminal", "--", "/usr/bin/tmux", "attach", "-t", "=codealong-1-0"]);
        assert_eq!(shown("kitty"), ["/usr/bin/kitty", "/usr/bin/tmux", "attach", "-t", "=codealong-1-0"]);
        assert_eq!(shown("wezterm"), ["/usr/bin/wezterm", "start", "--", "/usr/bin/tmux", "attach", "-t", "=codealong-1-0"]);
        assert_eq!(shown("xterm"), ["/usr/bin/xterm", "-e", "/usr/bin/tmux", "attach", "-t", "=codealong-1-0"]);
        // A process with several windows, or one window of several tabs, doesn't say which shows
        // the session.
        assert_eq!(lone_window("6291468\n", "4120\n"), Some("6291468"));
        assert_eq!(lone_window("6291468\n6291502\n", "4120\n4188\n"), None);
        assert_eq!(lone_window("6291468\n", "4120\n4188\n"), None);
        assert_eq!(lone_window("", "4120\n"), None);
        assert_eq!(lone_window("6291468\n", ""), None);
        // Linux keeps 15 characters of a process's name: GNOME Terminal's server is cut so.
        assert!(EMULATORS.iter().all(|e| e.process.len() <= 15));
        assert!(LinuxTerminal.runs_as("gnome-terminal-") && LinuxTerminal.runs_as("xterm") && !LinuxTerminal.runs_as("tmux"));
    }
}
