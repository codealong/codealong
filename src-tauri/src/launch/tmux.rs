//! tmux through its own command. It isn't an app sessions open in: its server leaves the terminal
//! it started in, and its panes show in whatever terminal a client of it runs in. So a session's
//! pane is found by its process's TTY and typed into there, and its tab is looked for by title in
//! the terminal config.toml names.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{anyhow, Result};

use super::{tty_of, HostKind, Key, Submit, Terminal, Typed, CLEAR_WAIT};
use crate::procs;

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

    fn type_session(&self, pid: u32, _cwd: &Path, text: &str, clear: bool, submit: Submit) -> Result<Typed> {
        match pane_of(pid) {
            Some(pane) => {
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
        Ok(pane_of(pid))
    }

    fn press(&self, at: &str, key: Key) -> Result<()> {
        let name = match key {
            Key::Digit1 => "1",
            Key::Escape => "Escape",
            Key::CtrlC => "C-c",
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

/// The tmux pane whose TTY is the process's.
fn pane_of(pid: u32) -> Option<String> {
    let tty = tty_of(pid)?;
    let bin = bin()?;
    let out = procs::run(&bin.to_string_lossy(), &["list-panes", "-a", "-F", "#{pane_tty} #{pane_id}"], None, Duration::from_secs(5)).ok()?;
    pane_for(&out, &tty.to_string_lossy())
}

/// From `list-panes -F '#{pane_tty} #{pane_id}'`, the pane on `tty`.
pub(super) fn pane_for(list: &str, tty: &str) -> Option<String> {
    list.lines().find_map(|l| l.split_once(' ').filter(|(t, _)| *t == tty).map(|(_, id)| id.trim().to_string()))
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
