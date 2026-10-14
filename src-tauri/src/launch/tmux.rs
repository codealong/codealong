//! tmux through its own command. It isn't an app sessions open in: its server leaves the terminal
//! it started in, and its panes show in whatever terminal a client of it runs in (`tmux attach`).
//! So a session's pane is found by its process's TTY and typed into there, and its tab is the one
//! its client runs in. A session no client shows is in no tab at all, until a new one attaches.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{anyhow, Result};

use super::{tty_of, HostKind, Key, Submit, Terminal, TerminalHost, Typed, CLEAR_WAIT};
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

    /// tmux's server leaves the terminal it started in, so nothing above it is one.
    fn runs_as(&self, process: &str) -> bool {
        process == "tmux"
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
        if let Some((pane, (client, tty))) = pane_of(pid).filter(|p| p.shown).and_then(|p| Some((p.id, client_of(&p.session)?))) {
            show(&pane);
            // The client is a tmux process too, so its terminal is the one above it. One in
            // another tmux is left to its title, so a client inside its own session can't loop.
            let terminal = super::parent_and_name(client).and_then(|(parent, _)| super::terminal_kind(parent)).filter(|t| t.kind() != HostKind::Tmux);
            if let Some(found) = terminal.map(|t| t.focus_session(profile, client, &tty, title, cwd)).transpose()?.flatten() {
                return Ok(Some(found));
            }
        }
        super::focus_by_title(profile, title, cwd)
    }

    fn attach_command(&self, pid: u32) -> Option<String> {
        let pane = pane_of(pid).filter(|p| !p.shown)?;
        Some(format!("{} attach -t {}", bin()?.to_string_lossy(), sh_quote(&pane.session)))
    }

    fn type_session(&self, pid: u32, _cwd: &Path, text: &str, clear: bool, submit: Submit) -> Result<Typed> {
        match pane_of(pid) {
            Some(Pane { id: pane, .. }) => {
                if clear {
                    self.press(&pane, Key::CtrlC)?;
                    std::thread::sleep(CLEAR_WAIT);
                }
                type_text(&pane, text, submit).map(|_| Typed::Into(NAME))
            }
            None => Ok(Typed::Nowhere("Codealong couldn't find its tmux pane".into())),
        }
    }

    fn presses_keys(&self) -> bool {
        true
    }

    fn key_target(&self, pid: u32, _cwd: &Path) -> Result<Option<String>> {
        Ok(pane_of(pid).map(|p| p.id))
    }

    fn press(&self, at: &str, key: Key) -> Result<()> {
        let name = match key {
            Key::Digit1 => "1",
            Key::Escape => "Escape",
            Key::CtrlC => "C-c",
            Key::End => "End",
            Key::Return => "Enter",
        };
        let bin = bin().ok_or_else(|| anyhow!("tmux isn't installed where Codealong looks for it."))?;
        procs::run(&bin.to_string_lossy(), &["send-keys", "-t", at, name], None, Duration::from_secs(5))?;
        Ok(())
    }
}

fn bin() -> Option<PathBuf> {
    #[cfg(test)]
    if let Some(staged) = super::stage::tmux_bin() {
        return Some(staged);
    }
    ["/opt/homebrew/bin/tmux", "/usr/local/bin/tmux", "/usr/bin/tmux"].iter().map(PathBuf::from).find(|p| p.is_file())
}

/// A pane, as `list-panes` says it: its id, and the session it's in, by name, and whether any
/// client shows that session.
#[derive(Debug, PartialEq)]
pub(super) struct Pane {
    pub id: String,
    pub session: String,
    pub shown: bool,
}

/// What `list-panes` says of each pane, the session's name last since it may have spaces.
const PANES: &str = "#{pane_tty} #{pane_id} #{session_attached} #{session_name}";

/// The tmux pane whose TTY is the process's.
fn pane_of(pid: u32) -> Option<Pane> {
    let tty = tty_of(pid)?;
    let bin = bin()?;
    let out = procs::run(&bin.to_string_lossy(), &["list-panes", "-a", "-F", PANES], None, Duration::from_secs(5)).ok()?;
    pane_for(&out, &tty.to_string_lossy())
}

/// From `list-panes -F PANES`, the pane on `tty`.
pub(super) fn pane_for(list: &str, tty: &str) -> Option<Pane> {
    list.lines().find_map(|l| {
        let mut f = l.splitn(4, ' ');
        if f.next()? != tty {
            return None;
        }
        let (id, clients, session) = (f.next()?, f.next()?, f.next()?);
        Some(Pane { id: id.to_string(), session: session.to_string(), shown: clients != "0" })
    })
}

/// What `list-clients` says of each client, the session's name last since it may have spaces.
const CLIENTS: &str = "#{client_activity} #{client_pid} #{client_tty} #{client_session}";

/// The client showing `session` that was used last: its process and TTY.
fn client_of(session: &str) -> Option<(u32, PathBuf)> {
    let bin = bin()?;
    let out = procs::run(&bin.to_string_lossy(), &["list-clients", "-F", CLIENTS], None, Duration::from_secs(5)).ok()?;
    client_for(&out, session)
}

/// From `list-clients -F CLIENTS`, the client on `session` with the latest activity.
pub(super) fn client_for(list: &str, session: &str) -> Option<(u32, PathBuf)> {
    let clients = list.lines().filter_map(|l| {
        let mut f = l.splitn(4, ' ');
        let (at, pid, tty) = (f.next()?.parse::<i64>().ok()?, f.next()?.parse::<u32>().ok()?, f.next()?);
        (f.next()? == session).then(|| (at, pid, PathBuf::from(tty)))
    });
    clients.max_by_key(|c| c.0).map(|(_, pid, tty)| (pid, tty))
}

/// Makes the pane the one its session shows: its window the current one, and it the active pane
/// there. Failing leaves the client as it was, and its tab still comes forward.
fn show(pane: &str) {
    let Some(bin) = bin() else { return };
    for command in ["select-window", "select-pane"] {
        let _ = procs::run(&bin.to_string_lossy(), &[command, "-t", pane], None, Duration::from_secs(5));
    }
}

/// Pastes `text` into the pane (bracketed, so a prompt of several lines arrives whole), then
/// presses Return a moment later, as `GHOSTTY_TYPE` does.
fn type_text(pane: &str, text: &str, submit: Submit) -> Result<()> {
    let bin = bin().ok_or_else(|| anyhow!("tmux isn't installed where Codealong looks for it."))?.to_string_lossy().into_owned();
    procs::run_stdin(&bin, &["load-buffer", "-b", "codealong", "-"], text, Duration::from_secs(5))?;
    procs::run(&bin, &["paste-buffer", "-p", "-d", "-b", "codealong", "-t", pane], None, Duration::from_secs(5))?;
    std::thread::sleep(Duration::from_millis(300));
    match submit {
        Submit::Return => procs::run(&bin, &["send-keys", "-t", pane, "Enter"], None, Duration::from_secs(5))?,
        // Each character by its code (`-H`), so a control character goes as the key it is.
        Submit::Keys(keys) => {
            let codes: Vec<String> = keys.bytes().map(|b| format!("{b:02x}")).collect();
            let mut args = vec!["send-keys", "-t", pane, "-H"];
            args.extend(codes.iter().map(String::as_str));
            procs::run(&bin, &args, None, Duration::from_secs(5))?
        }
    };
    Ok(())
}
