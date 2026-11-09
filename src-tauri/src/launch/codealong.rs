//! Codealong's own terminals as a terminal sessions start in, beside Ghostty and the rest:
//! `[apps] terminal = "codealong"`. The server holds each terminal (terminals.rs), so everything
//! another terminal is asked through a script is done here directly: a start opens a terminal
//! whose shell is typed the start's line as it starts, a send pastes into it as the program asked
//! for pastes and presses Return, a key goes in as typed, and closing one hangs up its shell.
//!
//! A session's process is known to run in one by its terminal device, which the terminal records
//! as it opens (`Glance::tty`), not by the processes above it: those are the server's, which runs
//! the sessions Codealong runs itself too, and after a handover they're launchd's. The engine
//! finds each terminal's session the same way and marks the terminal as the session's own
//! (`terminals::adopt`), which the windows show in the session's panel.
//!
//! Showing one is the main window's: the server asks it to show the terminal (`views::show`, a
//! reveal of kind `terminal`), which shows the session it's for, or for one that's no session's
//! (a folder opened in a terminal, a background job attached to), a window of its own.

use std::path::Path;
use std::time::Duration;

use anyhow::{anyhow, Result};
use serde_json::json;

use super::{fresh_line, tty_of, Closing, HostKind, Key, Placement, Submit, Tab, Terminal, TerminalHost, Typed, CLEAR_WAIT};
use crate::terminals;

const NAME: &str = "Codealong";

/// How long after a paste before Return, so the program takes them apart: Claude Code reads a
/// Return that comes while it's still taking a paste in as part of it (Rex's wait, 0.3 s, which
/// Sculptor found to be about what Claude Code needs).
const PASTE_WAIT: Duration = Duration::from_millis(300);

pub struct Own;

/// The terminal a live session's process runs in, by the process's terminal device. None when
/// there's no terminal of Codealong's at all, without asking the kernel.
pub(super) fn hosting(pid: u32) -> Option<terminals::Glance> {
    if !terminals::any() {
        return None;
    }
    terminals::on_tty(&tty_of(pid)?.to_string_lossy())
}

/// Asks the windows to show terminal `id`. True when there's such a terminal. `wait`: it was
/// opened for a command, which may be a session's that the board doesn't have yet, so the
/// windows wait a moment for that before showing it as no session's (`src/terminals.ts`
/// `placeOf`).
fn show(id: &str, wait: bool) -> bool {
    if !terminals::list().iter().any(|g| g.id == id) {
        return false;
    }
    crate::views::show(json!({ "kind": "terminal", "id": id, "wait": wait }));
    true
}

impl Terminal for Own {
    fn kind(&self) -> HostKind {
        HostKind::Codealong
    }

    fn name(&self) -> &'static str {
        NAME
    }

    fn profile(&self) -> Option<&'static str> {
        Some("codealong")
    }

    /// Part of Codealong, so always there.
    fn installed(&self) -> bool {
        true
    }

    /// Found by the session's terminal device instead (`hosting`): the processes above it are the
    /// server's, which runs other things than terminals.
    fn runs_as(&self, _process: &str) -> bool {
        false
    }

    /// The server keeps its terminals running with no window showing them, and makes them with no
    /// display to draw in, so while the screen is locked too.
    fn holds_sessions(&self) -> bool {
        true
    }

    fn run(&self, place: Placement, dir: &Path, input: &str, env: &[(String, String)]) -> Result<()> {
        let opened = terminals::open(terminals::Open {
            folder: dir.to_path_buf(),
            env: env.to_vec(),
            input: Some(fresh_line(input)),
            close_on_exit: true,
            ..Default::default()
        })
        .map_err(|e| anyhow!(e))?;
        if !place.behind {
            show(&opened.id, !input.trim().is_empty());
        }
        Ok(())
    }

    fn tabs(&self) -> Result<Vec<Tab>> {
        Ok(terminals::list().into_iter().map(|g| Tab { name: if g.title.is_empty() { g.shell.clone() } else { g.title.clone() }, id: g.id, cwd: g.folder }).collect())
    }

    fn focus(&self, at: &str) -> Result<bool> {
        Ok(show(at, false))
    }

    fn focus_session(&self, _profile: &str, _pid: u32, tty: &Path, _title: &str, _cwd: &Path) -> Result<Option<TerminalHost>> {
        let Some(g) = terminals::on_tty(&tty.to_string_lossy()) else { return Ok(None) };
        Ok(show(&g.id, false).then(|| TerminalHost { terminal: &Own, at: g.id }))
    }

    /// A terminal beside the session's own, under the same session, in front.
    fn split(&self, pid: u32, dir: &Path) -> Result<bool> {
        let Some(g) = hosting(pid).filter(|g| !g.session.is_empty()) else { return Ok(false) };
        let opened = terminals::open(terminals::Open { session: g.session, folder: dir.to_path_buf(), close_on_exit: true, ..Default::default() }).map_err(|e| anyhow!(e))?;
        show(&opened.id, false);
        Ok(true)
    }

    fn type_session(&self, pid: u32, _cwd: &Path, text: &str, clear: bool, submit: Submit, _passes: bool) -> Result<Typed> {
        let Some(g) = hosting(pid) else { return Ok(Typed::Nowhere("Codealong couldn't find its terminal".into())) };
        if clear {
            terminals::write_to(&g.id, Key::CtrlC.typed().into()).map_err(|e| anyhow!(e))?;
            std::thread::sleep(CLEAR_WAIT);
        }
        // Framed as the program asked for pastes (bracketed, for Claude Code and Codex), and
        // never refused: it's a message for the program, not lines for a shell.
        terminals::paste(&g.id, text.to_string(), true).map_err(|e| anyhow!(e))?;
        std::thread::sleep(PASTE_WAIT);
        let after = match submit {
            Submit::Return => Key::Return.typed(),
            Submit::Keys(keys) => keys,
        };
        terminals::write_to(&g.id, after.into()).map_err(|e| anyhow!(e))?;
        Ok(Typed::Into(NAME))
    }

    fn presses_keys(&self) -> bool {
        true
    }

    fn key_target(&self, pid: u32, _cwd: &Path) -> Result<Option<String>> {
        Ok(hosting(pid).map(|g| g.id))
    }

    fn press(&self, at: &str, key: Key) -> Result<()> {
        terminals::write_to(at, key.typed().into()).map_err(|e| anyhow!(e))
    }

    /// The end of all its text, which its own emulator holds.
    fn screen(&self, pid: u32, _cwd: &Path) -> Result<Option<String>> {
        let Some(g) = hosting(pid) else { return Ok(None) };
        let text = terminals::text(&g.id).map_err(|e| anyhow!(e))?;
        Ok(Some(super::screen_tail(&text)))
    }

    fn closes_tabs(&self) -> bool {
        true
    }

    /// Closing one hangs up its shell's session, as closing a tab does, and kills what's left a
    /// few seconds later (terminals.rs `close`).
    fn close_tabs(&self, tabs: &[&Closing]) -> Result<Vec<Result<()>>> {
        Ok(tabs
            .iter()
            .map(|t| match hosting(t.pid) {
                Some(g) => terminals::close(&g.id).map_err(|e| anyhow!(e)),
                None => Err(anyhow!("It isn't in a terminal of Codealong's.")),
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

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

    /// A process in one of Codealong's terminals is found there by its device, typed into as a
    /// send types (a paste, then Return), and pressed keys in; closing its tab ends it.
    #[test]
    fn a_session_in_codealongs_terminal_is_typed_into_and_closed() {
        let g = terminals::open(terminals::Open { folder: std::env::temp_dir(), command: Some("read line; echo \"got:$line\"; read key; echo \"key:$key\"; sleep 30".into()), size: Some(terminals::DEFAULT_SIZE), ..Default::default() }).unwrap();
        let name = g.tty.trim_start_matches("/dev/").to_string();
        let on_it = || crate::procs::run("/bin/ps", &["-t", &name, "-o", "pid="], None, Duration::from_secs(5)).ok().and_then(|o| o.split_whitespace().next().and_then(|p| p.parse::<u32>().ok()));
        assert!(wait_for(|| on_it().is_some()), "a process on {name}");
        let pid = on_it().unwrap();
        assert_eq!(hosting(pid).map(|t| t.id), Some(g.id.clone()));
        assert_eq!(super::super::terminal_kind(pid).map(|t| t.kind()), Some(HostKind::Codealong));
        assert_eq!(Own.type_session(pid, Path::new("/"), "hello there", false, Submit::Return, false).unwrap(), Typed::Into("Codealong"));
        assert!(wait_for(|| terminals::text(&g.id).is_ok_and(|t| t.contains("got:hello there"))), "{:?}", terminals::text(&g.id));
        let at = Own.key_target(pid, Path::new("/")).unwrap().expect("its terminal");
        Own.press(&at, Key::Digit(2)).unwrap();
        Own.press(&at, Key::Return).unwrap();
        assert!(wait_for(|| terminals::text(&g.id).is_ok_and(|t| t.contains("key:2"))), "{:?}", terminals::text(&g.id));
        let closed = Own.close_tabs(&[&Closing { pid, cwd: Path::new("/"), hang_up: vec![pid], unasked: None }]).unwrap();
        assert!(closed[0].is_ok(), "{closed:?}");
        assert!(!terminals::list().iter().any(|t| t.id == g.id), "closed");
        assert_ne!(Own.type_session(pid, Path::new("/"), "again", false, Submit::Return, false).unwrap(), Typed::Into("Codealong"));
    }
}
