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
use serde::Serialize;

use crate::config::{AccountCfg, Config};
use crate::model::{Harness, Session};
use crate::transcript::Page;

/// What a session waits on you for, when it's a tool call you can allow or deny.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Pending {
    /// The request's own id: the tool call's (Claude Code) or the permission request's (OpenCode).
    /// An answer names it, so a request that changed meanwhile isn't answered by mistake.
    pub id: String,
    /// The tool call as a line, e.g. "Bash · `rm -rf build`".
    pub label: String,
    /// All of it: the command, the file and the change.
    pub input: Option<String>,
    /// Why Codealong can't answer it here, when it can't; the panel then offers to open it.
    pub cannot: Option<String>,
}

/// Your answer to a pending request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Answer {
    Allow,
    Deny,
}

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
}

static ADAPTERS: [&dyn Adapter; 4] = [&crate::claude::ClaudeCode, &crate::codex::Codex, &crate::opencode::OpenCode, &crate::antigravity::Antigravity];

pub fn adapter(h: Harness) -> &'static dyn Adapter {
    *ADAPTERS.iter().find(|a| a.harness() == h).expect("every harness has an adapter")
}

impl Harness {
    /// The harness as the UI names it.
    pub fn word(self) -> &'static str {
        match self {
            Harness::Claude => "Claude Code",
            Harness::Codex => "Codex",
            Harness::Opencode => "OpenCode",
            Harness::Antigravity => "Antigravity",
        }
    }
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
}
