//! What Codealong does with a harness's sessions, harness by harness, behind one interface.
//!
//! Each harness (Claude Code, Codex, OpenCode, Antigravity) has a module of its own that knows
//! its files and formats, and an `Adapter` that answers the questions the rest of the app asks
//! of any session: where its record is, what it said and did, what it's waiting on you for and
//! how to answer. The rest of the app asks `adapter(session.harness)` and never matches on the
//! harness itself. A new harness is a module with an `Adapter`, a line in `ADAPTERS`, and a
//! variant of `Harness`; each method a harness can't do has a default that says so.
//!
//! Reading sessions for the board (discovery, status rules, opening, starting, sending, usage)
//! still dispatches in the engine and its helpers; CLAUDE.md's "Adding a harness" lists where.
//! They move behind this interface as they're next worked on.

use std::path::PathBuf;

use anyhow::{bail, Result};

use crate::config::{AccountCfg, Config};
use crate::model::{Harness, Session};
use crate::transcript::Page;

/// What a session waits on you for, and your answer: they go between devices too, so they're in
/// codealong-peer.
pub use codealong_peer::api::{Answer, Pending, Question};

/// What an adapter gets to work with besides the session: the account it's on.
pub struct Ctx<'a> {
    pub account: &'a AccountCfg,
}

pub trait Adapter: Sync {
    fn harness(&self) -> Harness;

    /// The files or databases the harness keeps the session in, most specific first.
    fn records(&self, account: &AccountCfg, session: &Session) -> Vec<PathBuf>;

    /// The session's conversation, a page back from `before` (from the newest when none).
    fn conversation(&self, _ctx: &Ctx, _session: &Session, _before: Option<u64>, _limit: usize) -> Result<Page> {
        bail!("Codealong can't read {} conversations yet.", self.harness().word())
    }

    /// What the session said and did after `after`.
    fn conversation_after(&self, _ctx: &Ctx, _session: &Session, _after: u64) -> Result<Page> {
        bail!("Codealong can't read {} conversations yet.", self.harness().word())
    }

    /// The ways a prompt can reach the session, best first (`send.rs` tries them in order).
    /// None: Codealong can't send it anything, and says to open it.
    fn routes(&self, _account: &AccountCfg, _session: &Session) -> Vec<crate::send::Route> {
        Vec::new()
    }

    /// What the session waits on you to allow, if anything. Only asked of sessions that need you.
    fn pending(&self, _ctx: &Ctx, _session: &Session) -> Option<Pending> {
        None
    }

    /// Answers the pending request `id`, checking first that it's still the one waiting.
    /// Returns what happened, in a sentence.
    fn answer(&self, _ctx: &Ctx, _session: &Session, _id: &str, _answer: Answer) -> Result<String> {
        bail!("Answer it in {}: Codealong can't answer it from here.", self.harness().word())
    }

    /// Answers the questions of the pending request `id` (`Pending::questions`): `answers` has
    /// each answered question's text for a key, and a choice's label, several for a question
    /// that takes more, or words of your own. Returns what happened, in a sentence.
    fn answer_questions(&self, _ctx: &Ctx, _session: &Session, _id: &str, _answers: &serde_json::Value) -> Result<String> {
        bail!("Answer it in {}: Codealong can't answer it from here.", self.harness().word())
    }

    /// The whole input of each tool call the session made that has no result yet: the command
    /// it's running now, or the call waiting on you. The `codealong` command finds the session
    /// that runs it from these when no process above it is a session (cli.rs). The harness
    /// must have written the call before running it.
    fn open_calls(&self, _ctx: &Ctx, _session: &Session) -> Vec<String> {
        Vec::new()
    }

    /// Whether the commands a session runs are children of its process (`Session.pid`), so
    /// that a command without that process above it isn't the session's. Not so where tools
    /// run in a service that serves every session.
    fn runs_its_commands(&self) -> bool {
        false
    }

    /// Whether the live session skips permission prompts, as its own files say. A message it
    /// sends another session through Codealong may then claim that, as Claude Code's own
    /// messages between sessions claim their sender's permission class (send.rs). False when
    /// Codealong can't tell.
    fn skips_permission_prompts(&self, _ctx: &Ctx, _session: &Session) -> bool {
        false
    }

    /// A copy of the session's conversation as far as it's written, to go on with in a new
    /// terminal while the session itself carries on: what to type there. A harness that copies
    /// through a service has made the copy by the time this returns.
    fn fork(&self, _ctx: &Ctx, _session: &Session) -> Result<Fork> {
        bail!("{} can't fork a session.", self.harness().word())
    }

    /// Ends a live session that has no terminal of its own (a background job), keeping its
    /// conversation to resume later. A session in a terminal closes with its tab (idle.rs).
    /// Returns what happened, in a sentence.
    fn stop(&self, _ctx: &Ctx, _session: &Session) -> Result<String> {
        bail!("Codealong can't close this {} session; close it where it runs.", self.harness().word())
    }

    /// Names the session where the harness keeps its name, so the harness shows it too, and
    /// returns the files that changed, for the engine to read again at once. `name` is one line
    /// (`session_name`).
    fn rename(&self, _ctx: &Ctx, _session: &Session, _name: &str) -> Result<Vec<PathBuf>> {
        bail!("Codealong can't rename {} sessions yet.", self.harness().word())
    }

    /// Whether `interrupt` can reach the session where it runs, asked of every session whose
    /// turn is running each time the board is put together, so it only looks, cheaply.
    fn interrupts(&self, _ctx: &Ctx, _session: &Session) -> bool {
        false
    }

    /// Stops the turn the session is taking, as its own Stop or Escape would, and leaves the
    /// session running. Returns what happened, in a sentence.
    fn interrupt(&self, _ctx: &Ctx, _session: &Session) -> Result<String> {
        bail!("Codealong can't stop this {} session; open it to stop it there.", self.harness().word())
    }
}

/// The longest name a session takes: Claude Code's own limit.
const NAME_MAX: usize = 200;

/// A name as typed, made one line as Claude Code makes its names: control, line-break,
/// direction and zero-width characters become spaces, and then runs of spaces one; the ends are
/// trimmed and it's cut at 200 characters. Mirrored by `sessionName` in the UI's harness.ts.
pub fn session_name(raw: &str) -> Result<String> {
    let spaced: String = raw.chars().map(|c| if c.is_control() || matches!(c, '\u{061C}' | '\u{200B}'..='\u{200F}' | '\u{2028}'..='\u{202E}' | '\u{2060}'..='\u{206F}' | '\u{FEFF}') { ' ' } else { c }).collect();
    let name: String = spaced.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(NAME_MAX).collect();
    let name = name.trim_end();
    if name.is_empty() {
        bail!("A session's name can't be empty.");
    }
    Ok(name.to_string())
}

/// Whether a session is in the middle of a turn that a Stop would end: working, or stuck mid-turn.
pub fn turn_running(s: &Session) -> bool {
    s.live && (s.status == crate::model::Status::Working || s.stalled_since.is_some())
}

/// Whether Codealong can stop the turn a session is taking: one it runs itself, or one its
/// harness can reach.
pub fn stoppable(s: &Session, account: Option<&AccountCfg>) -> bool {
    turn_running(s) && (s.hosted || account.is_some_and(|account| adapter(s.harness).interrupts(&Ctx { account }, s)))
}

/// How a fork opens in a terminal (`Adapter::fork`).
#[derive(Debug, Clone, PartialEq)]
pub struct Fork {
    /// Typed into the new terminal, Return included.
    pub input: String,
    /// Set in the terminal's shell.
    pub env: Vec<(String, String)>,
    /// The new session's key, when it's known before the harness writes the session.
    pub key: Option<String>,
}

static ADAPTERS: [&dyn Adapter; 4] = [&crate::claude::ClaudeCode, &crate::codex::Codex, &crate::opencode::OpenCode, &crate::antigravity::Antigravity];

pub fn adapter(h: Harness) -> &'static dyn Adapter {
    *ADAPTERS.iter().find(|a| a.harness() == h).expect("every harness has an adapter")
}

/// A session on the board with its account, for an adapter.
pub fn find<'a>(board: &'a crate::model::Board, cfg: &'a Config, key: &str) -> Result<(&'a Session, &'a AccountCfg)> {
    let s = board.lanes.iter().flat_map(|l| &l.sessions).find(|s| s.key == key).ok_or_else(|| anyhow::anyhow!("That session isn't on the board any more."))?;
    let account = cfg.accounts.iter().find(|a| a.name == s.account).ok_or_else(|| anyhow::anyhow!("There's no account named {} in config.toml.", s.account))?;
    Ok((s, account))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_harness_has_its_own_adapter() {
        for h in [Harness::Claude, Harness::Codex, Harness::Opencode, Harness::Antigravity] {
            assert_eq!(adapter(h).harness(), h);
        }
    }

    #[test]
    fn names_are_one_line() {
        assert_eq!(session_name("  Fix the\nlogin   redirect\t").unwrap(), "Fix the login redirect");
        assert_eq!(session_name("a\u{200B}b\u{202E}c\u{FEFF}").unwrap(), "a b c");
        assert_eq!(session_name("Ünïcode · “quotes” and `code`").unwrap(), "Ünïcode · “quotes” and `code`");
        assert_eq!(session_name(&"x".repeat(250)).unwrap().chars().count(), 200);
        // Cut at 200, the space before the cut doesn't stay at the end.
        assert_eq!(session_name(&format!("{} y", "x".repeat(199))).unwrap(), "x".repeat(199));
        assert!(session_name(" \n\u{200B} ").is_err());
    }
}
