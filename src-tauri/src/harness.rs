//! What Codealong does with a harness's sessions, harness by harness, behind one interface.
//!
//! Each harness (Claude Code, Codex, OpenCode, Antigravity) has a module of its own that knows
//! its files and formats, and an `Adapter` that answers the questions the rest of the app asks
//! of any session: where its record is, what it said and did, what it's waiting on you for and
//! how to answer. The rest of the app asks `adapter(session.harness)` and never matches on the
//! harness itself. A new harness is a module with an `Adapter`, a line in `ADAPTERS`, and a
//! variant of `Harness`; each method a harness can't do has a default that says so.
//!
//! It answers for an account too: whether it's installed and signed in, how it signs in, what
//! its folders hold, where its skills go, how Codealong runs it itself, where its sessions can
//! start, and whether it has plan limits (`harness::of(account)`).
//!
//! Reading sessions for the board is a `Reader`, one a harness, which the engine keeps and
//! never looks inside: it holds what the harness's files said at the last read, says which
//! folders to watch, takes in the files that change, and puts the sessions together with the
//! `board::*_session` status rules.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use anyhow::{bail, Result};

use crate::config::{AccountCfg, Config};
use crate::model::{Harness, Session, Usage};
use crate::state::SeenState;
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

    /// A copy of the session's conversation as far as it's written that goes on with no window
    /// coming forward: a background job, or a session a service or Codealong runs. It starts in
    /// `cwd` on `prompt`, when that isn't empty (attachments not yet expanded), and otherwise
    /// waits for one. Returns the copy's key, when the harness has said it.
    fn fork_background(&self, _ctx: &Ctx, _session: &Session, _cwd: &Path, _prompt: &str) -> Result<Option<String>> {
        bail!("{} can't fork a session in the background.", self.harness().word())
    }

    /// A throwaway copy of the session's conversation as far as it's written, to ask things of
    /// on the side while the session carries on, unaware: the process that runs the copy in
    /// `cwd` and how to talk to it (side.rs). The copy may do what the session itself may
    /// without asking, and no more: it has nobody to ask, so what would ask is refused. The
    /// harness keeps no record of it.
    fn side(&self, _ctx: &Ctx, _session: &Session, _cwd: &Path) -> Result<crate::side::Recipe> {
        bail!("{} has no side conversations.", self.harness().word())
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

    // ---- The account, besides its sessions ----

    /// Whether an account kept in `home` is signed in, and the email it's for when its files say
    /// (Settings › Accounts). Reads no secret.
    fn signed_in(&self, _home: &Path) -> (bool, Option<String>) {
        (false, None)
    }

    /// What signs an account in, typed into a new terminal.
    fn sign_in_command(&self, account: &AccountCfg) -> String {
        account.command().to_string()
    }

    /// Whether it's on this Mac: `on_path` says whether a command is on the PATH, and `home` is
    /// yours, where installers put their own copies.
    fn installed(&self, _home: &Path, _on_path: &dyn Fn(&str) -> bool) -> bool {
        false
    }

    /// What its folder `dir` holds, for finding its accounts on this Mac (detect.rs).
    fn folder_use(&self, _dir: &Path) -> FolderUse {
        FolderUse::default()
    }

    /// Where an account reads personal skills, where install.rs puts Codealong's.
    fn skills_dir(&self, _account: &AccountCfg) -> Option<PathBuf> {
        None
    }

    /// How Codealong runs its sessions itself (acp.rs): the adapter's command and what goes in its
    /// environment, or why it can't. An account's own `acp` in config.toml comes before this.
    fn acp_command(&self, _account: &AccountCfg) -> std::result::Result<(String, Vec<(String, String)>), String> {
        Err(format!("{} has no Agent Client Protocol adapter.", self.harness().word()))
    }

    /// Where an account can start a session, the usual place first, or why it can't (start.rs).
    /// `home` is yours: an app runs the account in the harness's default folder.
    fn places(&self, account: &AccountCfg, _home: &Path) -> std::result::Result<Vec<crate::start::Place>, String> {
        Err(format!("{} can't start sessions from Codealong.", account.name))
    }

    /// The folders an account's sessions worked in since `since`, for the projects a first run
    /// sets up.
    fn session_cwds(&self, _home: &Path, _since: i64) -> Vec<String> {
        Vec::new()
    }

    /// Why it has no plan limits to show, when it keeps none Codealong can read.
    fn no_limits(&self) -> Option<&'static str> {
        None
    }

    /// Whether an account's plan usage is somewhere slow to ask (`read_usage`), which the
    /// engine asks every couple of minutes off its thread. Usage that comes with the sessions
    /// is the reader's to say (`Reader::usage`).
    fn polls_usage(&self) -> bool {
        false
    }

    /// Reads an account's plan usage where `polls_usage` says it is. It can take seconds.
    fn read_usage(&self, account: &AccountCfg, _now: i64) -> Usage {
        crate::usage::loading(account)
    }

    /// Archives a session in the harness itself, or takes it out of its archive, where the
    /// harness has one another program can reach. Elsewhere a session is archived in Codealong
    /// only, until it's active again.
    fn archive(&self, _account: &AccountCfg, _id: &str, _archived: bool) -> Result<()> {
        Ok(())
    }

    /// What reads its sessions for the board. `wake` tells the engine that something it read
    /// off the engine's thread changed what the board shows.
    fn reader(&self, wake: Wake) -> Box<dyn Reader>;
}

/// Tells the engine a reader has something new that no file change will say (`Adapter::reader`).
pub type Wake = Arc<dyn Fn() + Send + Sync>;

/// What a reader goes by as it reads: how far back sessions go, and what you've seen, pinned
/// and archived in Codealong.
pub struct Look<'a> {
    pub seen: &'a SeenState,
    /// Sessions quiet since before then aren't read, unless they're pinned.
    pub since: i64,
    pub now: i64,
}

impl Look<'_> {
    /// Whether a session was archived in Codealong and hasn't been active since.
    pub fn archived_here(&self, key: &str, last_activity: i64) -> bool {
        self.seen.archived_at(key).is_some_and(|at| last_activity <= at)
    }
}

/// What a read came to, for the engine to act on.
#[derive(Default)]
pub struct Applied {
    /// Something the board shows changed.
    pub dirty: bool,
    /// A session is there that wasn't before: its folder may have no lane yet.
    pub added: bool,
    /// Lines sessions wrote since the last read, which the engine checks for the PRs, Chrome
    /// tabs and worktrees they worked on.
    pub lines: Vec<NewLines>,
}

impl Applied {
    pub fn and(&mut self, other: Applied) {
        self.dirty |= other.dirty;
        self.added |= other.added;
        self.lines.extend(other.lines);
    }
}

/// A stretch of a session's record written since it was last read.
pub struct NewLines {
    pub path: PathBuf,
    pub from: u64,
    pub to: u64,
    /// The session's key.
    pub key: String,
}

/// A harness's sessions on this Mac, read and kept for the board (`Adapter::reader`). Every
/// method gets the accounts config.toml has for the harness, or one of them.
pub trait Reader: Send {
    fn harness(&self) -> Harness;

    /// Reads every account's sessions afresh: at launch, and when config.toml changes.
    fn scan(&mut self, accounts: &[AccountCfg], look: &Look);

    /// The folders to watch for an account's changes, each with whether everything under it
    /// counts.
    fn watch(&self, account: &AccountCfg) -> Vec<(PathBuf, bool)>;

    /// Whether a changed path is the account's own, to hand to `apply`, rather than a file in
    /// some worktree.
    fn owns(&self, account: &AccountCfg, path: &Path) -> bool;

    /// Takes in an account's files that changed.
    fn apply(&mut self, account: &AccountCfg, paths: &[PathBuf], look: &Look) -> Applied;

    /// Every ten seconds: what the watcher doesn't say (a file a writer holds open, a process
    /// that came or went).
    fn tick(&mut self, _accounts: &[AccountCfg], _look: &Look) -> Applied {
        Applied::default()
    }

    /// Every second or two, so it only looks, cheaply: a database whose writes the watcher
    /// never hears.
    fn glance(&mut self, _accounts: &[AccountCfg], _look: &Look) -> Applied {
        Applied::default()
    }

    /// Asks what only a running service knows, off the engine's thread, and wakes the engine
    /// when it has answered. Asked at launch and on each tick.
    fn poll(&mut self, _accounts: &[AccountCfg]) {}

    /// Archives made or undone in Codealong changed what's to be read.
    fn archives_changed(&mut self, _accounts: &[AccountCfg], _look: &Look) {}

    /// An account's sessions for the board, as of `look.now`.
    fn sessions(&self, account: &AccountCfg, look: &Look) -> Vec<Session>;

    /// The folders sessions ran in, for the lanes that show them.
    fn cwds(&self) -> Vec<String>;

    /// Each session's record that's a file of lines, with the session's key: the engine
    /// searches them for the worktrees and PRs a session worked on. A subagent's log counts for
    /// its session.
    fn logs(&self) -> Vec<(PathBuf, String)> {
        Vec::new()
    }

    /// An account's plan usage, when it comes with what's read anyway.
    fn usage(&self, _account: &AccountCfg, _now: i64) -> Option<Usage> {
        None
    }
}

/// A reader for each harness, in the adapters' order.
pub fn readers(wake: &Wake) -> Vec<Box<dyn Reader>> {
    ADAPTERS.iter().map(|a| a.reader(wake.clone())).collect()
}

/// The accounts config.toml has for a harness.
pub fn accounts_of(cfg: &Config, h: Harness) -> Vec<AccountCfg> {
    cfg.accounts.iter().filter(|a| of(a).is_some_and(|x| x.harness() == h)).cloned().collect()
}

/// A database's size and time and its WAL's, by a name, at the last look. A running session
/// writes only to the WAL, through a file SQLite holds open, and FSEvents doesn't report those
/// writes (checked on this Mac), so watching the folder doesn't hear them: a stat of two files
/// every couple of seconds does (`Reader::glance`).
#[derive(Default)]
pub struct DatabaseMarks(HashMap<String, [Option<(u64, Option<SystemTime>)>; 2]>);

impl DatabaseMarks {
    /// Whether the database changed since the last look under this name. The first look only
    /// notes how it stands.
    pub fn changed(&mut self, name: &str, db: &Path) -> bool {
        let wal = PathBuf::from(format!("{}-wal", db.to_string_lossy()));
        let mark = [db, wal.as_path()].map(|p| std::fs::metadata(p).ok().map(|m| (m.len(), m.modified().ok())));
        let changed = self.0.get(name).is_some_and(|before| *before != mark);
        self.0.insert(name.to_string(), mark);
        changed
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

/// What one of a harness's folders holds (`Adapter::folder_use`).
#[derive(Debug, Default, Clone, PartialEq)]
pub struct FolderUse {
    /// It holds sessions or a sign-in: the harness has run here.
    pub has_data: bool,
    pub last_used: Option<i64>,
    /// A sign-in kept in the folder itself, besides where `signed_in` looks.
    pub signed_in: bool,
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

/// The adapter for a harness as config.toml names it (`claude`, `codex`…); None for a name
/// Codealong doesn't know, which `config::check_accounts` keeps out of a saved file.
pub fn named(name: &str) -> Option<&'static dyn Adapter> {
    crate::config::HARNESSES.contains(&name).then(|| adapter(Harness::from_config(name)))
}

/// The adapter for an account's harness (`named`).
pub fn of(account: &AccountCfg) -> Option<&'static dyn Adapter> {
    named(&account.harness)
}

/// A session on the board with its account, for an adapter.
pub fn find<'a>(board: &'a crate::model::Board, cfg: &'a Config, key: &str) -> Result<(&'a Session, &'a AccountCfg)> {
    let (_, s) = board.session(key).ok_or_else(|| anyhow::anyhow!("That session isn't on the board any more."))?;
    let account = cfg.accounts.iter().find(|a| a.name == s.account).ok_or_else(|| anyhow::anyhow!("There's no account named {} in config.toml.", s.account))?;
    Ok((s, account))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_account_names_its_adapter_by_its_config_string() {
        assert_eq!(named("codex").map(|a| a.harness()), Some(Harness::Codex));
        assert_eq!(named("antigravity").map(|a| a.harness()), Some(Harness::Antigravity));
        // Harness::from_config reads anything unknown as Claude Code; an account's harness
        // doesn't, so nothing is asked of Claude Code's adapter for it.
        assert!(named("gemini").is_none());
    }

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
