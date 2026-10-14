//! The engine owns every cache. It reacts to file changes (FSEvents), runs git on a small
//! worker pool and GitHub in the background, and publishes a fresh board after each change.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Result};
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};

use crate::board::{self, LaneSeed, ELSEWHERE};
use crate::chrome;
use crate::autoreview;
use crate::artifacts;
use crate::chrometabs;
use crate::prlinks;
use crate::teamfile;
use crate::training;
use crate::previews;
use crate::reusable;
use crate::origins;
use crate::suggestions;
use crate::tags;
use crate::config::{self, AccountCfg, Config, Theme};
use crate::detect;
use crate::editors;
use crate::ghpoll::{self, GhPoll, Now};
use crate::git;
use crate::harness;
use crate::github::{self, PrRaw};
use crate::held::Held;
use crate::jev;
use crate::todojev;
use crate::launch;
use crate::model::*;
use crate::perf;
use crate::procs;
use crate::repos::{self, Checkout};
use crate::search;
use crate::state::SeenState;
use crate::throttle::{self, GitQueue, Verdict};
use crate::todos::{self, TodoOp, TodoStore};
use crate::attachments;
use crate::usage;
use crate::util::{expand, file_name, home, mtime_ms, now_ms, parallel_map, tilde, DAY_MS};

/// How far back sessions are read.
const WINDOW_MS: i64 = 14 * DAY_MS;
const GIT_WORKERS: usize = 4;
const PUBLISH_EVERY: Duration = Duration::from_millis(300);
/// Claude usage is read this often; Codex usage updates with every Codex turn.
const USAGE_EVERY: Duration = Duration::from_secs(2 * 60);
/// After a question to Jev fails (no key, TypeSafe down), how long before more are asked.
const JEV_RETRY: Duration = Duration::from_secs(5 * 60);
/// After the usage endpoint says to slow down.
const USAGE_BACKOFF: Duration = Duration::from_secs(10 * 60);
/// How long the repo scan waits for folders' files before going on without the ones still out,
/// which macOS may be holding for a privacy prompt (held.rs). At a load of 600, this Mac's 66
/// session folders and 123 worktrees were all read within 170 ms.
const HELD_WAIT: Duration = Duration::from_secs(1);
/// How long it waits for a scan root's walk, which reads many folders (330 ms at that load): one
/// that came in late would leave the board without the root's repos for a moment.
const ROOT_WAIT: Duration = Duration::from_secs(10);
/// Reads out this long are said on the board: a slow disk is back by then.
const HELD_WARN_AFTER: Duration = Duration::from_secs(10);
/// Reads that land within this of each other are used by one scan.
const LANDED_QUIET: Duration = Duration::from_millis(150);
/// How long a window's seen or archive waits to hear which board shows it. A window shows the
/// change meanwhile; without an answer it keeps its copy until a board shows the change.
const SHOWN_WAIT: Duration = Duration::from_secs(10);

pub enum Msg {
    Fs(Vec<PathBuf>),
    /// Git's word on a worktree, or none when its run fell over: the run is over either way.
    GitDone(PathBuf, Option<GitInfo>),
    /// Something GitHub was asked in the background came back (ghpoll.rs).
    Github(ghpoll::Event),
    RefreshGithub,
    /// A PR changed on GitHub at your hand: PRs are read again in full now.
    PrChanged,
    /// A PR you merged from Codealong, by its slug on the board and its number. The reply is the
    /// time of the board that shows it, published at once.
    Merged(String, u64, Sender<i64>),
    /// A PR you approved from Codealong, by its slug on the board and its number. The reply is
    /// the time of the board that shows it, published at once.
    Approved(String, u64, Sender<i64>),
    /// Sessions marked seen. With a reply, the time of the board that shows it, published at once.
    MarkSeen(Vec<String>, Option<Sender<i64>>),
    /// Sessions to mark unread, and where to say when the board shows it.
    MarkUnread(Vec<String>, Option<Sender<i64>>),
    /// Sessions snoozed until then, or woken now with none. The reply is the time of the board
    /// that shows it, published at once.
    Snooze(Vec<String>, Option<i64>, Sender<i64>),
    /// Reviews asked of you snoozed until then, by PR key, or woken now with none. The reply is
    /// the time of the board that shows it, published at once.
    SnoozeReviews(Vec<String>, Option<i64>, Sender<i64>),
    /// Sessions quieted, or made heard again. The reply is the time of the board that shows it,
    /// published at once.
    Quiet(Vec<String>, bool, Sender<i64>),
    Pin(Vec<String>, bool, Sender<i64>),
    /// Helpers made sessions of your own. The reply is the time of the board that shows it,
    /// published at once.
    Own(Vec<String>, Sender<i64>),
    /// suggestions.json just changed at a request here: the board is published again at once.
    /// The reply is the time of the board that shows it.
    Suggested(Sender<i64>),
    /// Lane path → session keys whose logs mention it, from the scan of that number
    /// (`Visitors::begin`).
    Visitors(u64, HashMap<PathBuf, Vec<String>>),
    /// Session key → the PRs its records show it made or changed, each with its first mark's
    /// time, and the branches they show it pushed to (prlinks.rs).
    PrMarks(prlinks::Marks, prlinks::Pushes),
    /// Session key → the pages on the web its records show it made or showed you, the latest
    /// first (artifacts.rs).
    Artifacts(artifacts::Made),
    /// Session key → the Chrome tabs its records show it drove, their profiles looked up, and
    /// Chrome's profiles (chrometabs.rs).
    ChromeUse(HashMap<String, chrometabs::Use>, Vec<chrome::Profile>),
    /// The profile folder a session's Chrome tabs are in, looked up for the tab group it had
    /// then, and Chrome's profiles.
    ChromeProfile(String, Option<i64>, Option<String>, Vec<chrome::Profile>),
    RefreshUsage,
    /// A move of a session a usage limit stopped, and why it didn't work if it didn't.
    LimitMoved(Option<String>),
    /// Sessions told their PR was approved (nudges.rs), and which weren't and why if any.
    Nudged(Option<String>),
    /// A session told what a command you ran for it printed (terminal.rs `report_ended`), and why
    /// it wasn't if it wasn't.
    RunReported(Option<String>),
    /// Sessions told to carry on after a lost connection (reconnect.rs), and which weren't and
    /// why if any.
    Continued(Option<String>),
    /// Sessions told what their helpers did (helpers.rs), and which weren't and why if any.
    Reported(Option<String>),
    /// A job of the automatic reviews went (autoreview.rs), and why it didn't if it didn't:
    /// reviews.json is read again at once.
    Reviewed(Option<String>),
    /// pr-links.json was changed from a window: read now, without waiting for the watcher.
    PrLinksChanged,
    /// The usage read is over: what it read, or none when it fell over or never started.
    UsageDone(Option<Vec<Usage>>),
    /// The read of repos' team files is over (teamfile.rs): what it read, or none when it fell
    /// over or never started.
    TeamFiles(Option<Vec<teamfile::Answer>>),
    /// The read of the accounts' data settings is over (training.rs): what it read, or none when
    /// it fell over or never started.
    TrainingRead(Option<Vec<training::Training>>),
    /// One account's data setting, read again after Codealong turned training off, for the
    /// account the turn-off began on (`training::identity`).
    TrainingOne(String, training::Training),
    /// A harness's reader has something new that it read off this thread (`harness::Wake`).
    Read,
    /// The full pinned list, in order.
    SetPinned(Vec<String>),
    /// The full list of pinned worktrees (lane ids), in order.
    SetPinnedWorktrees(Vec<String>),
    /// The full list of pinned PRs (`owner/name#number`), in order.
    SetPinnedPrs(Vec<String>),
    /// Sessions archived (true) or unarchived from Codealong; Codex has already done its part.
    /// The reply is the time of the board that shows them, published at once.
    SetArchived(Vec<String>, bool, Sender<i64>),
    /// The theme, already saved to config.toml.
    SetTheme(Theme),
    /// The board windows' zoom, already saved to config.toml.
    SetZoom(u32),
    /// An account's star, already saved to config.toml.
    SetPreferred(String, bool),
    /// Sent every second while the perf log is on; how late it's read is the engine's lag.
    Ping(Instant),
    /// A change to a todo or to the labels, from a window. The reply is the time of the board
    /// that shows it.
    Todo(TodoOp, Sender<std::result::Result<i64, String>>),
    /// A session started from todos (one, or several on one prompt): its worktree (none for a
    /// new one), and the session's key when Codealong chose its ID, or the start to wait for.
    TodoStarted { ids: Vec<String>, lane: Option<String>, session: Option<String>, pending: Option<todos::PendingStart> },
    /// Jev's answer to a question the engine asked (`ask_jev`), by the id it was asked under.
    JevDone(String, std::result::Result<serde_json::Value, String>),
    /// "Looks done" was wrong for this todo, for now.
    DismissTodoHint(String),
    /// A session Codealong runs changed in a way the board shows (acp.rs).
    Hosted,
    /// A read of a folder's files came back after the scan that asked for it went on (held.rs).
    Landed,
    /// The servers listening in worktrees changed (previews.rs).
    Servers(Vec<previews::Server>),
    /// A worktree's preview URL was set or cleared; the reply is the time of the board that
    /// shows it.
    Previews(Sender<i64>),
    /// A worktree was marked reusable or not; the reply is the time of the board that shows it.
    Reusable(Sender<i64>),
    /// Files a window's change just wrote (a session renamed in its harness's record), read now
    /// rather than when the watcher says. The reply is the time of the board that shows it,
    /// published at once.
    Reread(Vec<PathBuf>, Sender<i64>),
    /// A PR the board doesn't list, as GitHub has it, to match to worktrees and sessions for a
    /// link to it (`board::pr_card`): the repo as the link names it, the PR, and who `gh` is.
    PrCard(String, PrRaw, Option<String>, Sender<PrItem>),
}

/// What the UI talks to. Cheap to clone.
#[derive(Clone)]
pub struct Handle {
    tx: Sender<Msg>,
    board: Arc<RwLock<Board>>,
}

impl Handle {
    /// A handle on an empty board with no engine behind it, for tests of what goes through one.
    #[cfg(test)]
    pub fn for_tests() -> Handle {
        let (tx, _) = channel();
        Handle { tx, board: Arc::new(RwLock::new(Board::default())) }
    }

    /// A handle on a board read once (`snapshot`, `light_snapshot`), with no engine behind it,
    /// for `--snapshot`'s modes: what goes through a handle does the same over it. A message
    /// for the engine is taken and dropped, so whoever waits for the board that shows a change
    /// hears at once that none will, as a window does from an engine too busy to say.
    pub fn over(board: Board) -> Handle {
        let (tx, rx) = channel();
        std::thread::spawn(move || for _ in rx {});
        Handle { tx, board: Arc::new(RwLock::new(board)) }
    }

    pub fn board(&self) -> Board {
        self.board.read().map(|b| b.clone()).unwrap_or_default()
    }

    /// Reads the board in place, for a caller that needs only a piece of it.
    pub fn read<T>(&self, f: impl FnOnce(&Board) -> T) -> T {
        match self.board.read() {
            Ok(b) => f(&b),
            Err(_) => f(&Board::default()),
        }
    }

    pub fn refresh_github(&self) {
        let _ = self.tx.send(Msg::RefreshGithub);
    }

    /// A PR has just changed on GitHub at your hand (merged, queued, its branch updated): PRs are
    /// read again now, not at the next poll.
    pub fn pr_changed(&self) {
        let _ = self.tx.send(Msg::PrChanged);
    }

    /// A PR you've just merged shows merged now, not at the next poll. Returns the time of the
    /// board that shows it, as `approved` does.
    pub fn merged(&self, slug: &str, number: u64) -> Option<i64> {
        let (tx, rx) = channel();
        self.tx.send(Msg::Merged(slug.to_string(), number, tx)).ok()?;
        rx.recv_timeout(SHOWN_WAIT).ok()
    }

    /// A PR you've just approved stops asking for your review now, not at the next poll. Returns
    /// the time of the board that shows it, as `mark_seen_shown` does.
    pub fn approved(&self, slug: &str, number: u64) -> Option<i64> {
        let (tx, rx) = channel();
        self.tx.send(Msg::Approved(slug.to_string(), number, tx)).ok()?;
        rx.recv_timeout(SHOWN_WAIT).ok()
    }

    pub fn mark_seen(&self, keys: Vec<String>) {
        let _ = self.tx.send(Msg::MarkSeen(keys, None));
    }

    /// Marks sessions seen and returns the time of the board that shows it, so a window knows
    /// when its own copy of the change can go; None when the engine is too busy to say.
    pub fn mark_seen_shown(&self, keys: Vec<String>) -> Option<i64> {
        let (tx, rx) = channel();
        self.tx.send(Msg::MarkSeen(keys, Some(tx))).ok()?;
        rx.recv_timeout(SHOWN_WAIT).ok()
    }

    pub fn mark_unread(&self, keys: Vec<String>) {
        let _ = self.tx.send(Msg::MarkUnread(keys, None));
    }

    /// Marks sessions unread: each is Your turn again until it's seen or prompted. Returns the
    /// time of the board that shows it, as `mark_seen_shown` does.
    pub fn mark_unread_shown(&self, keys: Vec<String>) -> Option<i64> {
        let (tx, rx) = channel();
        self.tx.send(Msg::MarkUnread(keys, Some(tx))).ok()?;
        rx.recv_timeout(SHOWN_WAIT).ok()
    }

    /// Snoozes sessions until `until`, or wakes them with none. Returns the time of the board
    /// that shows it, as `mark_seen_shown` does.
    pub fn snooze(&self, keys: Vec<String>, until: Option<i64>) -> Option<i64> {
        let (tx, rx) = channel();
        self.tx.send(Msg::Snooze(keys, until, tx)).ok()?;
        rx.recv_timeout(SHOWN_WAIT).ok()
    }

    /// Snoozes your review of PRs (`owner/name#number`) until `until`, or wakes them with none.
    /// Returns the time of the board that shows it, as `mark_seen_shown` does.
    pub fn snooze_reviews(&self, keys: Vec<String>, until: Option<i64>) -> Option<i64> {
        let (tx, rx) = channel();
        self.tx.send(Msg::SnoozeReviews(keys, until, tx)).ok()?;
        rx.recv_timeout(SHOWN_WAIT).ok()
    }

    /// Quiets sessions, or makes them heard again (`board::apply_quiet`). Returns the time of the
    /// board that shows it, as `mark_seen_shown` does.
    pub fn quiet(&self, keys: Vec<String>, quiet: bool) -> Option<i64> {
        let (tx, rx) = channel();
        self.tx.send(Msg::Quiet(keys, quiet, tx)).ok()?;
        rx.recv_timeout(SHOWN_WAIT).ok()
    }

    /// Pins sessions at the top of the pinned ones, or unpins them (`SeenState::pin`). Returns the
    /// time of the board that shows it, as `quiet` does.
    pub fn pin(&self, keys: Vec<String>, pinned: bool) -> Option<i64> {
        let (tx, rx) = channel();
        self.tx.send(Msg::Pin(keys, pinned, tx)).ok()?;
        rx.recv_timeout(SHOWN_WAIT).ok()
    }

    /// Makes helpers sessions of your own, for good (`SeenState::own`). Returns the time of the
    /// board that shows it, as `quiet` does.
    pub fn make_own(&self, keys: Vec<String>) -> Option<i64> {
        let (tx, rx) = channel();
        self.tx.send(Msg::Own(keys, tx)).ok()?;
        rx.recv_timeout(SHOWN_WAIT).ok()
    }

    /// suggestions.json or tags.json was just changed here (suggestions.rs, tags.rs): the board
    /// shows it now rather than with the next change it hears of. Returns the time of the board
    /// that shows it, as `mark_seen_shown` does, waiting for it only with `wait`.
    pub fn suggested(&self, wait: bool) -> Option<i64> {
        let (tx, rx) = channel();
        self.tx.send(Msg::Suggested(tx)).ok()?;
        if !wait {
            return None;
        }
        rx.recv_timeout(SHOWN_WAIT).ok()
    }

    /// A session Codealong runs started or ended a turn, or asked or stopped asking something;
    /// or a Claude Code session's hook handed the app a permission request, or took it back
    /// (claude/asks.rs).
    pub fn hosted_changed(&self) {
        let _ = self.tx.send(Msg::Hosted);
    }

    pub fn refresh_usage(&self) {
        let _ = self.tx.send(Msg::RefreshUsage);
    }

    /// How moving a session a usage limit stopped went, when Codealong moved it itself
    /// (limits.rs): a failure is a notice until a later move works.
    pub fn limit_moved(&self, result: std::result::Result<(), String>) {
        let _ = self.tx.send(Msg::LimitMoved(result.err()));
    }

    /// How telling sessions their PR was approved went (nudges.rs): a failure is a notice until
    /// a later nudge works.
    pub fn nudged(&self, result: std::result::Result<(), String>) {
        let _ = self.tx.send(Msg::Nudged(result.err()));
    }

    /// How telling a session what a command you ran for it printed went (terminal.rs
    /// `report_ended`): a failure is a notice until a later one works.
    pub fn run_reported(&self, result: std::result::Result<(), String>) {
        let _ = self.tx.send(Msg::RunReported(result.err()));
    }

    /// How telling sessions to carry on after a lost connection went (reconnect.rs): a failure
    /// is a notice until a later one works.
    pub fn continued(&self, result: std::result::Result<(), String>) {
        let _ = self.tx.send(Msg::Continued(result.err()));
    }

    /// How telling sessions what their helpers did went (helpers.rs): a failure is a notice
    /// until a later one works.
    pub fn reported(&self, result: std::result::Result<(), String>) {
        let _ = self.tx.send(Msg::Reported(result.err()));
    }

    /// How a job of the automatic reviews went (autoreview.rs): a failure is a notice until a
    /// later job goes through, and the PRs' cards follow reviews.json at once.
    pub fn reviewed(&self, result: std::result::Result<(), String>) {
        let _ = self.tx.send(Msg::Reviewed(result.err()));
    }

    /// pr-links.json was just changed here (a PR's nudge turned on or off): the board follows
    /// at once, where the file's watcher would take a moment.
    pub fn pr_links_changed(&self) {
        let _ = self.tx.send(Msg::PrLinksChanged);
    }

    pub fn set_pinned(&self, keys: Vec<String>) {
        let _ = self.tx.send(Msg::SetPinned(keys));
    }

    pub fn set_pinned_worktrees(&self, ids: Vec<String>) {
        let _ = self.tx.send(Msg::SetPinnedWorktrees(ids));
    }

    pub fn set_pinned_prs(&self, keys: Vec<String>) {
        let _ = self.tx.send(Msg::SetPinnedPrs(keys));
    }

    /// A change to a todo or the labels. Returns once a board that shows it has gone out, with
    /// that board's time, so a window knows when its own copy of the change can go.
    pub fn todo(&self, op: TodoOp) -> std::result::Result<i64, String> {
        self.todo_within(op, Duration::from_secs(20))
    }

    /// `todo`, waiting as long as the caller can: the socket (server.rs) waits longer than a
    /// window, since a command has nothing else to show meanwhile and the engine answers late
    /// while it scans at launch.
    pub fn todo_within(&self, op: TodoOp, wait: Duration) -> std::result::Result<i64, String> {
        let (tx, rx) = channel();
        self.tx.send(Msg::Todo(op, tx)).map_err(|_| "Codealong's engine stopped.".to_string())?;
        rx.recv_timeout(wait).map_err(|_| "Codealong is busy; the change will show when it catches up.".to_string())?
    }

    /// Changes a worktree's preview (`previews::set`): its URL, which of its servers is the app
    /// when more than one serves a page, and the Chrome profile it opens in over its project's.
    /// Returns what changed, and the time of the board that shows it, as `mark_seen_shown` does.
    pub fn set_preview(&self, lane_id: &str, change: &previews::Change) -> Result<(previews::Set, Option<i64>)> {
        if !self.read(|b| b.lanes.iter().any(|l| l.id == lane_id && l.device.is_none())) {
            bail!("That worktree just changed. Try again.");
        }
        let set = previews::set(lane_id, change)?;
        let (tx, rx) = channel();
        if self.tx.send(Msg::Previews(tx)).is_err() {
            bail!("Codealong's engine stopped.");
        }
        Ok((set, rx.recv_timeout(SHOWN_WAIT).ok()))
    }

    /// Marks a worktree reusable or not (`reusable::mark`). Returns whether that changed it, and
    /// the time of the board that shows it, as `set_preview` does.
    pub fn set_reusable(&self, lane_id: &str, on: bool) -> Result<(bool, Option<i64>)> {
        let lane = self.read(|b| b.lanes.iter().find(|l| l.id == lane_id && l.device.is_none()).cloned()).ok_or_else(|| anyhow!("That worktree just changed. Try again."))?;
        let changed = reusable::mark(&lane, on)?;
        let (tx, rx) = channel();
        if self.tx.send(Msg::Reusable(tx)).is_err() {
            bail!("Codealong's engine stopped.");
        }
        Ok((changed, rx.recv_timeout(SHOWN_WAIT).ok()))
    }

    /// "Looks done" was wrong for this todo, for now.
    pub fn dismiss_todo_hint(&self, id: String) {
        let _ = self.tx.send(Msg::DismissTodoHint(id));
    }

    /// Links a session started from todos to each of them, or waits for it to show up.
    pub fn todo_started(&self, ids: Vec<String>, lane: Option<String>, session: Option<String>, pending: Option<todos::PendingStart>) {
        let _ = self.tx.send(Msg::TodoStarted { ids, lane, session, pending });
    }

    /// Archives or unarchives a session: in its harness where that has an archive another
    /// program can reach (`Adapter::archive`: Codex, so the Codex app shows it too), and in
    /// Codealong, where a session of any other harness stays archived until it's active again.
    /// Returns the time of the board that shows it, as `mark_seen_shown` does.
    pub fn archive(&self, key: &str, archived: bool) -> Result<Option<i64>> {
        archive_in_harness(key, archived)?;
        self.set_archived(vec![key.to_string()], archived)
    }

    /// Archives several sessions as `archive` does each, shown on one board: each key's result,
    /// in order. One its harness wouldn't archive isn't archived in Codealong either.
    pub fn archive_all(&self, keys: &[String]) -> Vec<Result<()>> {
        let mut results: Vec<Result<()>> = keys.iter().map(|k| archive_in_harness(k, true)).collect();
        let done: Vec<String> = keys.iter().zip(&results).filter(|(_, r)| r.is_ok()).map(|(k, _)| k.clone()).collect();
        if !done.is_empty() {
            if let Err(e) = self.set_archived(done, true) {
                let e = crate::util::words(e);
                for r in results.iter_mut().filter(|r| r.is_ok()) {
                    *r = Err(anyhow!("{e}"));
                }
            }
        }
        results
    }

    fn set_archived(&self, keys: Vec<String>, archived: bool) -> Result<Option<i64>> {
        let (tx, rx) = channel();
        if self.tx.send(Msg::SetArchived(keys, archived, tx)).is_err() {
            bail!("Codealong's engine stopped.");
        }
        Ok(rx.recv_timeout(SHOWN_WAIT).ok())
    }

    /// Names a session where its harness keeps its name (`Adapter::rename`), so the harness
    /// shows it too, and reads what that changed at once. Returns the time of the board that
    /// shows it, as `mark_seen_shown` does.
    pub fn rename(&self, key: &str, name: &str) -> Result<Option<i64>> {
        let name = harness::session_name(name)?;
        let cfg = config::current()?;
        let (session, account) = self.read(|b| harness::find(b, &cfg, key).map(|(s, a)| (s.clone(), a.clone())))?;
        let changed = harness::adapter(session.harness).rename(&harness::Ctx { account: &account }, &session, &name)?;
        let (tx, rx) = channel();
        if self.tx.send(Msg::Reread(changed, tx)).is_err() {
            bail!("Codealong's engine stopped.");
        }
        Ok(rx.recv_timeout(SHOWN_WAIT).ok())
    }

    /// A PR the board doesn't list, for a link to it in a conversation: GitHub's facts, with the
    /// worktrees and sessions here that worked on it. The GitHub request runs on the caller's
    /// thread; the engine only matches it. When the engine is too busy to, it comes with none.
    pub fn pr_card(&self, slug: &str, number: u64) -> Result<PrItem> {
        let (raw, login) = github::one(slug, number)?;
        let (tx, rx) = channel();
        if self.tx.send(Msg::PrCard(slug.to_string(), raw.clone(), login.clone(), tx)).is_ok() {
            if let Ok(item) = rx.recv_timeout(Duration::from_secs(3)) {
                return Ok(item);
            }
        }
        Ok(board::pr_card(&[], &[], slug, &HashMap::new(), &raw, login.as_deref(), &board::PrLinks::default()))
    }

    /// The accounts in config.toml, each with whether it's signed in.
    pub fn accounts(&self) -> Result<Vec<AccountInfo>> {
        let cfg = config::current()?;
        Ok(cfg.accounts.iter().map(usage::describe).collect())
    }

    /// Saves the account list to config.toml. The watcher sees the change and reloads.
    pub fn save_accounts(&self, accounts: Vec<AccountCfg>) -> Result<()> {
        config::save_accounts(&accounts)
    }

    /// Stars an account, or takes its star away, in config.toml, and publishes it at once
    /// rather than when the watcher notices the file.
    pub fn set_preferred(&self, account: &str, preferred: bool) -> Result<()> {
        let mut accounts = config::current()?.accounts;
        let a = accounts.iter_mut().find(|a| a.name == account).ok_or_else(|| anyhow::anyhow!("There's no account named {account}."))?;
        a.preferred = preferred;
        config::save_accounts(&accounts)?;
        let _ = self.tx.send(Msg::SetPreferred(account.to_string(), preferred));
        Ok(())
    }

    /// Saves every setting to config.toml, in place. The watcher sees the change and reloads;
    /// the theme is published at once, as the theme buttons do.
    pub fn save_config(&self, cfg: &config::Config) -> Result<()> {
        config::save(cfg)?;
        let _ = self.tx.send(Msg::SetTheme(cfg.appearance.theme));
        Ok(())
    }

    /// Saves config.toml as typed in Settings, once it parses.
    pub fn save_config_text(&self, text: &str) -> Result<()> {
        config::save_text(text)?;
        if let Ok(cfg) = config::parse(text) {
            let _ = self.tx.send(Msg::SetTheme(cfg.appearance.theme));
        }
        Ok(())
    }

    /// Saves the theme to config.toml and publishes it at once, rather than when the watcher
    /// notices the file.
    pub fn set_theme(&self, theme: Theme) -> Result<()> {
        config::save_theme(theme)?;
        let _ = self.tx.send(Msg::SetTheme(theme));
        Ok(())
    }

    /// Steps the board windows' zoom in config.toml and publishes it at once, as the theme is;
    /// answers the zoom it's at.
    pub fn zoom(&self, step: crate::zoom::Step) -> Result<u32> {
        let zoom = config::step_zoom(step)?;
        let _ = self.tx.send(Msg::SetZoom(zoom));
        Ok(zoom)
    }

    /// Opens a terminal window that signs the account in.
    pub fn sign_in(&self, name: &str) -> Result<()> {
        let cfg = config::current()?;
        let a = cfg.accounts.iter().find(|a| a.name == name).ok_or_else(|| anyhow!("There's no account named {name}."))?;
        let place = launch::Placement { new_terminal: cfg.apps.new_terminal, near: &[], behind: false, label: None };
        launch::terminal_open(&cfg.apps.terminal, place, &home(), Some(&format!("{}\n", usage::sign_in_command(a))))
    }

    /// Turns training off for an account at its provider, and answers where it stands after:
    /// read again, which goes on the board at once. Blocks for the calls.
    pub fn stop_training(&self, name: &str) -> Result<training::Training> {
        let cfg = config::current()?;
        let a = cfg.accounts.iter().find(|a| a.name == name).ok_or_else(|| anyhow!("There's no account named {name}."))?;
        let after = training::stop(a)?;
        let _ = self.tx.send(Msg::TrainingOne(training::identity(a), after.clone()));
        Ok(after)
    }

    /// Opens config.toml in the configured editor.
    pub fn open_config(&self) -> Result<()> {
        let editor = self.board.read().map(|b| b.apps.editor_id.clone()).unwrap_or_default();
        let editor = if editor.is_empty() { "cursor".to_string() } else { editor };
        editors::open(&editor, &config::config_path())
    }

    /// Takes what repo `slug`'s team suggests in its file on `branch` and you don't have yet (the
    /// file's `offers` on the board), writing it into your table for that repo in config.toml.
    pub fn take_team_settings(&self, slug: &str, branch: &str) -> Result<()> {
        let offers = self.read(|b| b.team_files.iter().find(|f| f.repo == slug && f.branch == branch).map(|f| f.offers.clone()));
        let offers = offers.ok_or_else(|| anyhow!("{slug} has no team settings on {branch} to take."))?;
        if offers.is_empty() {
            return Ok(());
        }
        config::take_suggestions(slug, &offers)
    }

    /// Opens repo `slug`'s team file as its main checkout here has it, in the editor.
    pub fn open_team_file(&self, slug: &str) -> Result<()> {
        let (editor, root) = self.read(|b| {
            let lanes = b.lanes.iter().filter(|l| l.device.is_none() && l.repo_slug.as_deref() == Some(slug));
            let root = lanes.clone().find(|l| l.origin == Origin::Main).or_else(|| lanes.clone().next()).map(|l| PathBuf::from(&l.path));
            (b.apps.editor_id.clone(), root)
        });
        let root = root.ok_or_else(|| anyhow!("There's no checkout of {slug} on this Mac."))?;
        let file = root.join(teamfile::FILE);
        if !file.is_file() {
            bail!("{} has no {}.", tilde(&root), teamfile::FILE);
        }
        let editor = if editor.is_empty() { "cursor".to_string() } else { editor };
        editors::open(&editor, &file)
    }

    /// Opens a web link: one into a worktree's app where its preview opens, so the app has that
    /// Chrome profile's sign-in (`board::app_of_link`), any other in the default browser. A
    /// profile Chrome no longer has opens the link in the default browser rather than not at
    /// all: a link's click has nowhere to say why, and the preview's own Open says it.
    pub fn open_link(&self, url: &str) -> Result<()> {
        match self.read(|b| link_opens_as(b, url)) {
            Some(lane) => open_as_preview(url, &lane).or_else(|e| {
                eprintln!("Opening {url} in the default browser: {e:#}");
                launch::open_url(url)
            }),
            None => launch::open_url(url),
        }
    }

    /// Opens a lane's target where it lives. See *Surfaces*, Open where it lives.
    pub fn open(&self, lane_id: &str, target_id: &str) -> Result<()> {
        let board = self.board.read().map(|b| b.clone()).map_err(|_| anyhow!("board unavailable"))?;
        if let Some(key) = open_on(&board, lane_id, target_id)? {
            self.mark_seen(vec![key]);
        }
        // Another editor picked from an Open in menu is the one Open in <editor> uses from then
        // on, as Codex's Open button remembers its last. The watcher publishes the change.
        if let Some(id) = target_id.strip_prefix("l|editor|").filter(|id| *id != board.apps.editor_id) {
            if let Err(e) = config::save_editor(id) {
                eprintln!("Couldn't save the editor: {e:#}");
            }
        }
        Ok(())
    }
}

/// Archives a session in its harness, or takes it out of its archive, where the harness has one
/// another program can reach (`Adapter::archive`).
fn archive_in_harness(key: &str, archived: bool) -> Result<()> {
    let mut parts = key.splitn(3, ':');
    let (Some(harness), Some(account), Some(id)) = (parts.next(), parts.next(), parts.next()) else {
        bail!("That isn't a session.");
    };
    if let Some(adapter) = harness::named(harness) {
        let cfg = config::current()?;
        let a = cfg.accounts.iter().find(|a| a.name == account).ok_or_else(|| anyhow!("There's no account named {account}."))?;
        adapter.archive(a, id, archived)?;
    }
    Ok(())
}

/// Opens a lane's target on `board`. Returns the key of the session it opened, if any.
pub fn open_on(board: &Board, lane_id: &str, target_id: &str) -> Result<Option<String>> {
    let lane = board.lanes.iter().find(|l| l.id == lane_id).cloned().ok_or_else(|| anyhow!("That lane just changed. Try again."))?;
    let apps = board.apps.clone();
    let path = PathBuf::from(&lane.path);
    let near = board::terminal_neighbours(&board.lanes, &path);
    let place = launch::Placement { new_terminal: apps.new_terminal, near: &near, behind: false, label: None };
    let parts: Vec<&str> = target_id.splitn(3, '|').collect();
    match parts.as_slice() {
        ["s", key, action] => {
            let s = lane.sessions.iter().find(|s| s.key == *key).ok_or_else(|| anyhow!("That session is gone."))?;
            let cwd = if Path::new(&s.cwd).is_dir() { PathBuf::from(&s.cwd) } else { path.clone() };
            // The session's own tab says which worktree it's in; a plain terminal keeps its shell's titles.
            let label = board::tab_label(&board.lanes, &path, &s.title);
            let named = launch::Placement { label: label.as_deref(), ..place };
            match *action {
                "focus" => {
                    let pid = s.pid.ok_or_else(|| anyhow!("That session isn't running any more."))?;
                    // No tab shows it (a tmux session nothing is attached to): a new one does.
                    if let Some(cmd) = launch::attach_command(pid) {
                        launch::terminal_open(&apps.terminal_id, named, &cwd, Some(&format!("{cmd}\n")))?;
                    } else if !launch::terminal_focus_session(&apps.terminal_id, pid, &s.title, &cwd)? {
                        // It's looked for in the app it runs under, whatever config.toml names.
                        let app = launch::terminal_kind(pid).filter(|k| k.bundle().is_some()).map_or(apps.terminal.as_str(), |k| k.name());
                        bail!("Couldn't find {}'s terminal in {app}. It's running as pid {pid}.", s.account);
                    }
                }
                "attach" => {
                    let cmd = s.attach_command.clone().ok_or_else(|| anyhow!("This session isn't a background job."))?;
                    launch::terminal_open(&apps.terminal_id, named, &cwd, Some(&format!("{cmd}\n")))?;
                }
                "resume" => {
                    let cmd = s.resume_command.clone().ok_or_else(|| anyhow!("This session can't be resumed in a terminal."))?;
                    launch::terminal_open(&apps.terminal_id, named, &cwd, Some(&format!("{cmd}\n")))?;
                }
                // A new terminal in the folder it works in, beside the terminals already there
                // (`place`). Nothing about the session itself was looked at.
                "terminal" => {
                    let folder = PathBuf::from(board::session_folder(&lane.path, &s.cwd));
                    launch::terminal_open(&apps.terminal_id, place, if folder.is_dir() { &folder } else { &path }, None)?;
                    return Ok(None);
                }
                "chrome" => chrometabs::show(s.chrome.as_ref().ok_or_else(|| anyhow!("That session hasn't used Chrome."))?)?,
                // A window shows it in its own panel; nothing opens elsewhere.
                "hosted" => {}
                // A way of its harness's own: its app.
                other => harness::adapter(s.harness).open(s, other)?,
            }
            return Ok(Some(s.key.clone()));
        }
        ["l", "terminal"] => launch::terminal_open(&apps.terminal_id, place, &path, None)?,
        ["l", "editor"] => editors::open(&apps.editor_id, &path)?,
        ["l", "editor", id] if opens_in(&apps, id) => editors::open(id, &path)?,
        ["l", "finder"] => launch::reveal(&path)?,
        ["l", "preview"] => {
            let preview = lane.preview.as_ref().ok_or_else(|| anyhow!("This worktree has no preview URL."))?;
            // Only a web page: the URL was checked when it was set, and a found one is localhost.
            if !preview.url.starts_with("http://") && !preview.url.starts_with("https://") {
                bail!("{} isn't a web address.", preview.url);
            }
            open_as_preview(&preview.url, &lane)?
        }
        ["p", which] => {
            let pr = lane.pr.as_ref().ok_or_else(|| anyhow!("This lane has no PR."))?;
            match *which {
                "github" => launch::open_url(&pr.url)?,
                "codex-review" => launch::open_codex(&launch::codex_review_url(&pr.url))?,
                other => bail!("Unknown PR action {other:?}"),
            }
        }
        _ => bail!("Unknown target {target_id:?}"),
    }
    Ok(None)
}

/// Whether a target may open a folder in the editor `id`: yours, or another found installed, not
/// any app a target could name.
fn opens_in(apps: &AppNames, id: &str) -> bool {
    id == apps.editor_id || apps.editors.iter().any(|e| e.id == id)
}

/// Opens `url` where `lane`'s preview opens: in its Chrome profile, or with none, the default
/// browser.
fn open_as_preview(url: &str, lane: &Lane) -> Result<()> {
    let Some((preview, name)) = lane.preview.as_ref().and_then(|p| Some((p, p.chrome_profile.as_ref()?))) else {
        return launch::open_url(url);
    };
    let whose = if preview.chrome_profile_set { "this worktree".to_string() } else { format!("{} in config.toml", lane.project) };
    let profile = chrome::resolve(name).map_err(|e| anyhow!("{e} It's set for {whose}."))?;
    chrome::open(url, &profile)
}

/// The worktree whose preview a web link opens as (`Handle::open_link`), or None when it opens
/// in the default browser.
pub fn link_opens_as(board: &Board, url: &str) -> Option<Lane> {
    board::app_of_link(&board.lanes, url).cloned()
}

pub fn start(emit: impl Fn(&Board) + Send + 'static) -> Handle {
    let (tx, rx) = channel();
    // The theme and zoom are on the board before the first scan finishes, so windows open in them.
    let shortcuts = config::shortcuts();
    let board = Arc::new(RwLock::new(Board { loading: true, theme: config::theme(), zoom: config::zoom(), quick_shortcut: shortcuts.quick_panel, waiting_shortcut: shortcuts.waiting, ..Default::default() }));
    let handle = Handle { tx: tx.clone(), board: board.clone() };
    let ping = tx.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(1));
        if perf::enabled() && ping.send(Msg::Ping(Instant::now())).is_err() {
            return;
        }
    });
    std::thread::Builder::new()
        .name("codealong-engine".into())
        .spawn(move || {
            procs::utility_thread();
            let back = tx.clone();
            let mut engine = Engine::new(Box::new(emit), tx, board);
            engine.servers = previews::watch(move |servers| back.send(Msg::Servers(servers)).is_ok());
            engine.full_scan();
            engine.run(rx);
        })
        .expect("engine thread");
    handle
}

/// One synchronous pass for `--snapshot`: everything the board needs, no watchers. Claude usage
/// is read only when asked: the usage endpoint rate-limits, and the app polls it already.
pub fn snapshot(with_github: bool, with_usage: bool) -> Board {
    snapshot_engine(with_github, with_usage).compose()
}

/// `--snapshot --pr-card`: a PR as a link to it gets it when the board doesn't list it.
pub fn snapshot_pr_card(slug: &str, number: u64) -> Result<PrItem> {
    let (raw, login) = github::one(slug, number)?;
    let e = snapshot_engine(false, false);
    let lanes = e.compose().lanes;
    Ok(e.pr_card(&lanes, slug, &raw, login.as_deref()))
}

fn snapshot_engine(with_github: bool, with_usage: bool) -> Engine {
    let (tx, _rx) = channel();
    let mut e = Engine::new(Box::new(|_| {}), tx, Arc::new(RwLock::new(Board::default())));
    e.passing = true;
    let t = Instant::now();
    let lap = |label: &str, t: &Instant| eprintln!("  {label:<10} {:>6} ms", t.elapsed().as_millis());
    e.load_config();
    e.load_state();
    e.load_todos();
    lap("config", &t);
    e.scan_sessions();
    lap("sessions", &t);
    e.load_repos();
    lap("repos", &t);
    e.refresh_visitors(true);
    lap("visitors", &t);
    e.load_pr_links();
    e.load_reviews();
    e.scan_pr_marks(true);
    lap("pr marks", &t);
    e.load_previews();
    e.load_reusable();
    e.previews.servers = previews::scan_once(&e.server_roots());
    lap("previews", &t);
    e.loading = false;
    let _ = e.compose();
    lap("1st paint", &t);
    let jobs: Vec<(PathBuf, Option<String>)> = e.git_targets().into_iter().map(|p| (p.clone(), e.base_for(&p))).collect();
    for (p, info) in parallel_map(jobs, |(p, base)| Some((p.clone(), git::status(&p, base.as_deref())))) {
        e.git.insert(p, info);
    }
    lap("git", &t);
    if with_usage {
        for u in usage::read_all(&e.cfg.accounts) {
            e.usage.insert(u.account.clone(), u);
        }
        lap("usage", &t);
    }
    if with_github {
        let slugs: Vec<String> = board::polled_slugs(&e.seeds).into_iter().collect();
        for answer in github::read_once(&slugs) {
            e.github.take(answer, &e.seeds, Now::real());
        }
        lap("github", &t);
    }
    let answers = parallel_map(e.team_file_asks(), |ask| Some(teamfile::read(&ask)));
    e.team_files.on_read(Some(answers));
    lap("team files", &t);
    e
}

/// The board without git, GitHub, visitors or usage: sessions, worktrees and todos as the
/// files say, in a couple of seconds. For the `codealong` command when the app isn't running.
pub fn light_snapshot() -> Board {
    let (tx, _rx) = channel();
    let mut e = Engine::new(Box::new(|_| {}), tx, Arc::new(RwLock::new(Board::default())));
    e.passing = true;
    e.load_config();
    e.load_state();
    e.load_todos();
    e.scan_sessions();
    e.load_repos();
    e.load_previews();
    e.load_reusable();
    e.previews.servers = previews::scan_once(&e.server_roots());
    e.loading = false;
    e.compose()
}

/// A read that failed for a reason other than sign-in keeps the last good numbers; its error
/// says why they aren't fresh and `as_of` says how old they are.
fn keep_last_good(prev: Option<&Usage>, mut fresh: Usage) -> Usage {
    if fresh.error.is_some() && !fresh.needs_sign_in {
        if let Some(prev) = prev.filter(|p| !p.meters.is_empty()) {
            fresh.meters = prev.meters.clone();
            fresh.plan = fresh.plan.or_else(|| prev.plan.clone());
            fresh.as_of = prev.as_of;
        }
    }
    fresh
}

/// What one batch from the file watcher has found out so far (`Engine::apply_fs`). A build or an
/// install writes thousands of files in a few folders, and each was compared with every lane and
/// matched against every folder above it in its worktree's gitignore: up to 20 s for 30,000
/// paths, with the engine hearing nothing else meanwhile (2026-10-12). Within a batch each
/// folder's lane and gitignore are looked at once, and each lane's folder.
struct FsBatch {
    seeds: board::SeedIndex,
    /// What's known of each folder paths were in.
    dirs: HashMap<PathBuf, Folder>,
    /// Whether each lane's folder is there, by seed.
    lanes_there: HashMap<usize, bool>,
    /// Whether each other folder looked for is there.
    present: HashMap<PathBuf, bool>,
    /// The lanes git will run in for this batch.
    changed: HashSet<usize>,
}

#[derive(Clone, Copy)]
struct Folder {
    /// The lane of what's in it (`SeedIndex::holding`).
    lane: Option<usize>,
    /// Whether the lane's gitignore ignores it or a folder above it, once asked.
    ignored: Option<bool>,
}

impl FsBatch {
    fn new(seeds: &[LaneSeed]) -> FsBatch {
        FsBatch { seeds: board::SeedIndex::new(seeds), dirs: HashMap::new(), lanes_there: HashMap::new(), present: HashMap::new(), changed: HashSet::new() }
    }

    /// The seed `p` is in (`board::SeedIndex::best`).
    fn lane_of(&mut self, p: &Path) -> Option<usize> {
        if let Some(i) = self.seeds.own(p) {
            return Some(i);
        }
        match p.parent() {
            Some(dir) => self.folder(dir).lane,
            None => self.seeds.best(p),
        }
    }

    fn folder(&mut self, dir: &Path) -> &mut Folder {
        if !self.dirs.contains_key(dir) {
            let lane = self.seeds.holding(dir);
            self.dirs.insert(dir.to_path_buf(), Folder { lane, ignored: None });
        }
        self.dirs.get_mut(dir).expect("just put there")
    }

    fn lane_there(&mut self, lane: usize, folder: &Path) -> bool {
        *self.lanes_there.entry(lane).or_insert_with(|| folder.exists())
    }

    fn exists(&mut self, p: &Path) -> bool {
        if let Some(&there) = self.present.get(p) {
            return there;
        }
        let there = p.exists();
        self.present.insert(p.to_path_buf(), there);
        there
    }

    /// `gi.matched_path_or_any_parents(p, p.is_dir()).is_ignore()` for `p` in the lane whose
    /// folder is `root` and whose gitignore is `gi`: the path's own rule, else its folder's, kept
    /// for the next path there.
    fn ignored(&mut self, gi: &Gitignore, root: &Path, p: &Path) -> bool {
        let Some(dir) = p.parent().filter(|_| p != root) else {
            return gi.matched_path_or_any_parents(p, p.is_dir()).is_ignore();
        };
        // Whether it's a folder is asked of the disk only when a rule tells files and folders
        // apart (`build/`), and one says something different of each.
        let said = |m: ignore::Match<&ignore::gitignore::Glob>| (!m.is_none()).then(|| m.is_ignore());
        let (as_file, as_folder) = (said(gi.matched(p, false)), said(gi.matched(p, true)));
        let own = if as_file == as_folder || !p.is_dir() { as_file } else { as_folder };
        if let Some(ignored) = own {
            return ignored;
        }
        let folder = self.folder(dir);
        *folder.ignored.get_or_insert_with(|| gi.matched_path_or_any_parents(dir, true).is_ignore())
    }
}

enum RepoEvent {
    Lane(PathBuf),
    Checkout(PathBuf),
    Worktrees,
    Ignore,
}

/// Everything the repo scan reads in folders it doesn't own, one kind of read each. macOS can
/// hold any of these reads for a privacy prompt, so each holds up only its own folder (held.rs).
struct RepoReads {
    /// The checkouts under each scan root.
    roots: Held<Vec<Checkout>>,
    /// The worktree and main checkout each session's folder is in.
    folders: Held<Option<(PathBuf, PathBuf)>>,
    /// Checkouts outside the scan roots that sessions worked in.
    checkouts: Held<Option<Checkout>>,
    /// Each worktree's `.git` file and `.gitignore`.
    worktrees: Held<WorktreeFiles>,
}

impl RepoReads {
    fn new(tx: &Sender<Msg>) -> RepoReads {
        let landed = || {
            let tx = tx.clone();
            move || {
                let _ = tx.send(Msg::Landed);
            }
        };
        RepoReads {
            roots: Held::new("scan root", |root| repos::discover(&[root.to_path_buf()], 3), landed()),
            folders: Held::new("session folder", repos::locate, landed()),
            checkouts: Held::new("checkout", repos::load_checkout, landed()),
            worktrees: Held::new("worktree", read_worktree, landed()),
        }
    }

    /// The folders whose reads have been out at least `wait`, in order.
    fn out_for(&self, wait: Duration) -> Vec<PathBuf> {
        let mut out = self.roots.out_for(wait);
        out.extend(self.folders.out_for(wait));
        out.extend(self.checkouts.out_for(wait));
        out.extend(self.worktrees.out_for(wait));
        out.sort();
        out.dedup();
        out
    }
}

struct Engine {
    emit: Box<dyn Fn(&Board) + Send>,
    tx: Sender<Msg>,
    board: Arc<RwLock<Board>>,
    cfg: Config,
    /// config.toml as last read or written (`None` when there was none or it didn't read), so an
    /// edit the watcher didn't hear can be told from what's loaded.
    config_text: Option<String>,
    config_warning: Option<String>,
    seen: SeenState,
    /// Why state.json couldn't be read or saved.
    state_warning: Option<String>,
    /// state.json is there but doesn't read: the engine runs on what it has in memory (a first
    /// run's state at launch) and writes nothing over the file until it's fixed or deleted.
    state_unreadable: bool,
    /// Todos and labels, as in todos.json.
    todos: TodoStore,
    /// todos.json as last read or written, so the watcher can tell someone else's edit from ours.
    todos_text: String,
    /// Why todos.json couldn't be read or saved.
    todos_warning: Option<String>,
    /// todos.json is there but doesn't read as a todo list: nothing is written over it, and
    /// changes are turned away, until it's fixed or deleted.
    todos_unreadable: bool,
    /// Why the lock on todos.json couldn't be had, when a change was turned away for it.
    todos_lock_warning: Option<String>,
    checkouts: Vec<(String, Checkout)>,
    seeds: Vec<LaneSeed>,
    /// `<main>/.git/worktrees/<name>` → the linked worktree's path.
    gitdir_to_lane: HashMap<PathBuf, PathBuf>,
    ignores: HashMap<PathBuf, Gitignore>,
    /// Each harness's sessions as its files last said (`harness::Reader`), in the adapters' order.
    readers: Vec<Box<dyn harness::Reader>>,
    /// The repo scan's reads of files in folders it doesn't own, which macOS can hold.
    reads: RepoReads,
    /// When a held read last landed: the repos are read again once they stop landing.
    landed_at: Option<Instant>,
    /// Worktrees whose files the last scan couldn't read. Git would wait on them too, so it
    /// runs there once a scan has read them.
    held_worktrees: HashSet<PathBuf>,
    /// Hash of the last published board, to skip publishing when nothing changed.
    last_hash: u64,
    /// Sessions from other lanes whose logs show they worked in a worktree.
    visitors: Visitors,
    /// What to look for in logs: (lane path, text that identifies it, when it was made).
    needles: Vec<(PathBuf, String, i64)>,
    /// Session key → the PRs its records show it made or changed, each with its first mark's time.
    pr_marks: prlinks::Marks,
    /// Session key → the branches its records show it pushed to, each push with its time.
    pr_pushes: prlinks::Pushes,
    /// Session key → the pages on the web its records show it made or showed you, the latest
    /// first (artifacts.rs).
    artifacts: artifacts::Made,
    /// The records read whole for those, the PRs and the Chrome tabs (`scan_pr_marks`,
    /// `read_found_logs`): from then on only their new lines are.
    logs_read: HashSet<PathBuf>,
    /// Session key → the Chrome tabs its records show it drove (chrometabs.rs).
    chrome_use: HashMap<String, chrometabs::Use>,
    /// A headless run's key → the session that started it (`with_run_origins`), found while its
    /// process runs and kept after, while it's on the board. Learned as the board is composed,
    /// which only reads the engine, so behind a lock.
    run_origins: Mutex<HashMap<String, String>>,
    /// Chrome's profiles, as of the last lookup of a session's.
    chrome_profiles: Vec<chrome::Profile>,
    /// Session key → the tab group its profile was last looked up for, and when.
    chrome_lookups: HashMap<String, (Option<i64>, Instant)>,
    /// Links to PRs made or taken away by hand (`pr-links.json`).
    pr_store: prlinks::LinkStore,
    pr_links_warning: Option<String>,
    /// Where the automatic reviews stand (`reviews.json`), for the PRs' cards; autoreview.rs
    /// keeps the file.
    reviews: autoreview::Store,
    /// Why an automatic review couldn't be started or posted, until a later one goes.
    review_warning: Option<String>,
    /// Worktrees' preview URLs (`previews.json`) and the servers in their folders.
    previews: board::Previews,
    previews_warning: Option<String>,
    /// Worktrees marked reusable or not by hand (`worktrees.json`), by path.
    reusable: HashMap<String, bool>,
    reusable_warning: Option<String>,
    /// Why moving a session a usage limit stopped didn't work, when Codealong moved it itself.
    limit_move_warning: Option<String>,
    /// Why a session wasn't told its PR was approved, when Codealong sends that itself.
    nudge_warning: Option<String>,
    /// Why a session wasn't told what a command you ran for it printed.
    run_report_warning: Option<String>,
    continue_warning: Option<String>,
    report_warning: Option<String>,
    /// The files changed in the background whose last change couldn't be made, by name with
    /// why (`store::problems`), as of the last board.
    store_warnings: Vec<(String, String)>,
    /// The thread that finds servers, told which worktrees to look in; none in a snapshot.
    servers: Option<previews::Watcher>,
    git: HashMap<PathBuf, GitInfo>,
    git_jobs: Sender<GitJob>,
    /// Which worktrees git runs in, and which wait their turn (see throttle.rs).
    git_queue: GitQueue<PathBuf>,
    /// Checkout root → default branch ref (`origin/main`).
    default_branch: HashMap<PathBuf, Option<String>>,
    lane_base: HashMap<PathBuf, Option<String>>,
    /// Lane path → the main checkout it belongs to.
    lane_checkout: HashMap<PathBuf, PathBuf>,
    /// The checkouts of Codealong's source the scan found, in a project or not: each one's main
    /// checkout and identity (`github.com/owner/name`).
    codealong_sources: Vec<(PathBuf, Option<String>)>,
    /// GitHub as the engine has it: the PRs read, when it's asked again, and what it calls each
    /// repo now (ghpoll.rs).
    github: GhPoll,
    /// An engine for one pass (`--snapshot`, the command without the app): it's gone before
    /// what it would ask in the background comes back.
    passing: bool,
    watcher: Option<RecommendedWatcher>,
    watch_warning: Option<String>,
    /// Account folders that didn't exist at the last watch: a new account before sign-in.
    missing_homes: Vec<PathBuf>,
    /// Usage by account for the harnesses whose usage is asked for in the background
    /// (`Adapter::polls_usage`), from the last read.
    usage: HashMap<String, Usage>,
    usage_inflight: bool,
    usage_last: Option<Instant>,
    usage_backoff: bool,
    /// Each repo's team settings, `.codealong/config.toml` on its branches (teamfile.rs).
    team_files: teamfile::TeamFiles,
    /// Whether each account's provider may train on it (training.rs).
    training: training::TrainingPoll,
    dirty: bool,
    /// Only git results changed since the last publish; they go out in batches.
    dirty_git: bool,
    loading: bool,
    last_publish: Option<Instant>,
    perf: perf::Counters,
    /// Questions for Jev go to its own thread (jev.rs); answers come back as `Msg::JevDone`.
    jev_jobs: Sender<jev::Job>,
    /// Ids asked and not answered yet, so nothing is asked twice at once.
    jev_pending: HashSet<String>,
    /// What Jev was asked about todos and said (todojev.rs), kept in jev-todos.json.
    jev_todos: todojev::Memory,
    /// Todo id → the signature its "looks done" question was asked at and what it's based on,
    /// while it's out.
    jev_done_asking: HashMap<String, (String, String)>,
    /// Session key → the todo ids its link question's options stand for, while it's out.
    jev_link_asking: HashMap<String, Vec<String>>,
    /// jev-todos.json has been read: the first look reads it.
    jev_todos_loaded: bool,
    /// When a todo question last failed (no key, TypeSafe down): nothing more is asked for a while.
    jev_todos_failed: Option<Instant>,
    /// Jev's reads of sessions, by the moment each was asked about (insights.rs).
    insights: crate::insights::Store,
    /// Each session's emblem: the color it was given and the icon Jev picked (emblems.rs).
    emblems: crate::emblems::Store,
}

impl Engine {
    fn new(emit: Box<dyn Fn(&Board) + Send>, tx: Sender<Msg>, board: Arc<RwLock<Board>>) -> Engine {
        let git_jobs = spawn_git_workers(tx.clone());
        let reads = RepoReads::new(&tx);
        let back = tx.clone();
        let github = GhPoll::new(Arc::new(move |event| {
            let _ = back.send(Msg::Github(event));
        }));
        let back = tx.clone();
        let jev_jobs = jev::worker(move |id, answer| {
            let _ = back.send(Msg::JevDone(id, answer));
        });
        let back = Mutex::new(tx.clone());
        let wake: harness::Wake = Arc::new(move || {
            if let Ok(back) = back.lock() {
                let _ = back.send(Msg::Read);
            }
        });
        Engine {
            emit,
            tx,
            board,
            cfg: Config::default(),
            config_text: None,
            config_warning: None,
            seen: SeenState::default(),
            state_warning: None,
            state_unreadable: false,
            todos: TodoStore::default(),
            todos_text: String::new(),
            todos_warning: None,
            todos_unreadable: false,
            todos_lock_warning: None,
            checkouts: Vec::new(),
            seeds: Vec::new(),
            gitdir_to_lane: HashMap::new(),
            ignores: HashMap::new(),
            readers: harness::readers(&wake),
            reads,
            landed_at: None,
            held_worktrees: HashSet::new(),
            last_hash: 0,
            visitors: Visitors::default(),
            needles: Vec::new(),
            pr_marks: HashMap::new(),
            pr_pushes: HashMap::new(),
            artifacts: HashMap::new(),
            logs_read: HashSet::new(),
            chrome_use: HashMap::new(),
            run_origins: Mutex::default(),
            chrome_profiles: Vec::new(),
            chrome_lookups: HashMap::new(),
            pr_store: prlinks::LinkStore::default(),
            pr_links_warning: None,
            reviews: autoreview::Store::default(),
            review_warning: None,
            previews: board::Previews::default(),
            previews_warning: None,
            reusable: HashMap::new(),
            reusable_warning: None,
            limit_move_warning: None,
            nudge_warning: None,
            run_report_warning: None,
            continue_warning: None,
            report_warning: None,
            store_warnings: Vec::new(),
            servers: None,
            git: HashMap::new(),
            git_jobs,
            git_queue: GitQueue::default(),
            default_branch: HashMap::new(),
            lane_base: HashMap::new(),
            lane_checkout: HashMap::new(),
            codealong_sources: Vec::new(),
            github,
            passing: false,
            watcher: None,
            watch_warning: None,
            missing_homes: Vec::new(),
            usage: HashMap::new(),
            usage_inflight: false,
            usage_last: None,
            usage_backoff: false,
            team_files: teamfile::TeamFiles::default(),
            training: training::TrainingPoll::default(),
            dirty: false,
            loading: true,
            dirty_git: false,
            last_publish: None,
            perf: perf::Counters::default(),
            jev_jobs,
            jev_pending: HashSet::new(),
            jev_todos: todojev::Memory::default(),
            jev_done_asking: HashMap::new(),
            jev_link_asking: HashMap::new(),
            jev_todos_loaded: false,
            jev_todos_failed: None,
            insights: crate::insights::Store::load(),
            emblems: crate::emblems::Store::load(),
        }
    }

    /// Asks Jev in the background. The answer comes back to `on_jev` under the same id; an id
    /// already waiting for its answer isn't asked again.
    fn ask_jev(&mut self, id: String, body: serde_json::Value) {
        self.ask_jev_filled(id, body, None);
    }

    /// Asks with a body the worker finishes on its own thread first (`jev::Job::fill`).
    fn ask_jev_filled(&mut self, id: String, body: serde_json::Value, fill: Option<Box<dyn FnOnce(&mut serde_json::Value) + Send>>) {
        if self.jev_pending.insert(id.clone()) {
            let _ = self.jev_jobs.send(jev::Job { id, body, fill });
        }
    }

    /// An answer from Jev. The id's first part, up to `:`, says which feature asked.
    fn on_jev(&mut self, id: String, answer: std::result::Result<serde_json::Value, String>) {
        self.jev_pending.remove(&id);
        let (feature, rest) = id.split_once(':').unwrap_or((id.as_str(), ""));
        match feature {
            "todolink" => self.on_todo_link(rest.to_string(), answer),
            "tododone" => self.on_todo_done(rest.to_string(), answer),
            "insight" => self.on_insight(rest.to_string(), answer),
            "emblem" => self.on_emblem(rest, answer),
            _ => {
                let _ = answer;
            }
        }
    }

    /// Jev's answer on which todo a new session is for: linked when it's sure.
    fn on_todo_link(&mut self, key: String, answer: std::result::Result<serde_json::Value, String>) {
        let Some(ids) = self.jev_link_asking.remove(&key) else { return };
        let Ok(response) = answer else {
            // Asked again on a later look, unless it's too old by then.
            self.jev_todos.asked.remove(&key);
            self.jev_todos_failed = Some(Instant::now());
            return;
        };
        self.jev_todos.save();
        let Some(id) = todojev::link_pick(&response, &ids) else { return };
        let lanes = self.lane_index();
        let Ok(_lock) = self.lock_todos() else { return };
        self.reload_todos();
        if self.todos_unreadable || self.todos.todos.iter().any(|t| t.sessions.contains(&key)) {
            return;
        }
        if !self.todos.todos.iter().any(|t| t.id == id && t.status != todos::TodoStatus::Done) {
            return;
        }
        let op = TodoOp::Link { id, worktree: None, session: Some(key), linked: true };
        if self.todos.apply(op, &lanes, now_ms()).unwrap_or(false) {
            self.save_todos();
        }
    }

    /// Jev's answer on whether a todo's work looks done.
    fn on_todo_done(&mut self, id: String, answer: std::result::Result<serde_json::Value, String>) {
        let Some((sig, why)) = self.jev_done_asking.remove(&id) else { return };
        let Ok(response) = answer else {
            self.jev_todos_failed = Some(Instant::now());
            return;
        };
        let Some(p) = jev::noul(&response, "done") else { return };
        self.jev_todos.done.insert(id, todojev::DoneAnswer { sig, p, why });
        self.jev_todos.save();
        self.dirty = true;
    }

    /// Says a todo doesn't look done after all, until something linked to it moves on.
    fn dismiss_todo_hint(&mut self, id: String) {
        if let Some(a) = self.jev_todos.done.get(&id) {
            self.jev_todos.dismissed.insert(id, a.sig.clone());
            self.jev_todos.save();
            self.dirty = true;
        }
    }

    /// New sessions to link, and todos whose linked work moved on, go to Jev.
    fn jev_look_todos(&mut self, board: &Board) {
        if !self.jev_todos_loaded {
            self.jev_todos = todojev::Memory::load();
            self.jev_todos_loaded = true;
        }
        if self.jev_todos_failed.is_some_and(|t| t.elapsed() < JEV_RETRY) {
            return;
        }
        let model = self.cfg.jev.model.trim().to_string();
        let now = now_ms();
        let asks: Vec<(String, serde_json::Value, Vec<String>)> = todojev::link_candidates(board, &self.todos.todos, &self.jev_todos.asked, now)
            .into_iter()
            .map(|(lane, s)| {
                let (body, ids) = todojev::link_request(&model, board, lane, s, &self.todos.todos);
                (s.key.clone(), body, ids)
            })
            .collect();
        for (key, body, ids) in asks {
            self.jev_todos.asked.insert(key.clone());
            self.jev_link_asking.insert(key.clone(), ids);
            self.ask_jev(format!("todolink:{key}"), body);
        }
        let done: Vec<(String, todojev::DoneInput)> = self
            .todos
            .todos
            .iter()
            .filter_map(|t| todojev::done_input(board, t).map(|i| (t.id.clone(), i)))
            // One question per todo at a time; a newer state is asked about once it's answered.
            .filter(|(id, i)| self.jev_todos.done.get(id).is_none_or(|a| a.sig != i.sig) && !self.jev_done_asking.contains_key(id))
            .collect();
        for (id, input) in done {
            self.jev_done_asking.insert(id.clone(), (input.sig.clone(), input.why.clone()));
            self.ask_jev(format!("tododone:{id}"), todojev::done_request(&model, &input));
        }
    }

    /// What to ask Jev about the board just composed: each feature that's on looks for what's
    /// changed since it last asked.
    fn jev_look(&mut self, board: &Board) {
        if !self.cfg.jev.any() {
            return;
        }
        if self.cfg.jev.todos {
            self.jev_look_todos(board);
        }
        if self.cfg.jev.sessions {
            self.look_insights(board);
        }
        if self.cfg.jev.emblems {
            self.look_emblems(board);
        }
    }

    /// Gives a color to each session seen for the first time and asks Jev for the icon of each
    /// that has none yet (emblems.rs). At most a few new questions per board, as for insights.
    /// Another Mac's sessions come with the emblems that Mac gave them.
    fn look_emblems(&mut self, board: &Board) {
        const PER_BOARD: usize = 20;
        let now = now_ms();
        let sessions = board.lanes.iter().filter(|l| l.device.is_none()).flat_map(|l| l.sessions.iter().filter(|s| s.visitor.is_none()).map(move |s| (l.project.as_str(), s)));
        let mut asked = 0;
        for key in self.emblems.look(sessions, now) {
            let id = format!("emblem:{key}");
            if asked >= PER_BOARD {
                break;
            }
            let Some((_, s)) = board.session(&key).filter(|_| !self.jev_pending.contains(&id)) else { continue };
            self.ask_jev(id, crate::emblems::request(s, &self.cfg.jev.model));
            asked += 1;
        }
        self.emblems.save(now);
    }

    /// The icon Jev picked for a session's emblem.
    fn on_emblem(&mut self, key: &str, answer: std::result::Result<serde_json::Value, String>) {
        let now = now_ms();
        match answer.ok().as_ref().and_then(crate::emblems::read) {
            Some(icon) => {
                self.emblems.answered(key, icon, now);
                self.emblems.save(now);
                self.dirty = true;
            }
            None => self.emblems.failed(key, now),
        }
    }

    /// Sessions with their emblems, when `[jev] emblems` is on.
    fn with_emblems(&self, mut sessions: Vec<Session>) -> Vec<Session> {
        if !self.cfg.jev.emblems {
            return sessions;
        }
        for s in &mut sessions {
            s.emblem = self.emblems.emblem_of(s);
        }
        sessions
    }

    /// Asks Jev about sessions whose moment it hasn't read yet (insights.rs). At most a few new
    /// questions per board, so the first run after turning it on doesn't ask about every session
    /// at once. What a session shows may be an agent's tag or its own status line (`with_insights`),
    /// so whether Jev has answered is the store's to say, not the board's.
    fn look_insights(&mut self, board: &Board) {
        const PER_BOARD: usize = 20;
        let now = now_ms();
        let mut asks = Vec::new();
        for s in board.lanes.iter().flat_map(|l| &l.sessions).filter(|s| s.visitor.is_none()) {
            self.insights.seen(&s.key, now);
            let Some((kind, signature)) = crate::insights::ask(s, now) else { continue };
            let id = format!("insight:{signature}|{}", s.key);
            if asks.len() < PER_BOARD && self.insights.wants(&s.key, &signature, now) && !self.jev_pending.contains(&id) {
                let body = crate::insights::request(s, kind, &self.cfg.jev.model, None);
                // An approval is judged by the whole call, which its harness may take a slow read
                // to show (OpenCode's over HTTP): read on the worker's thread, never on this one.
                let fill: Option<Box<dyn FnOnce(&mut serde_json::Value) + Send>> = match kind {
                    crate::insights::Kind::Approval => {
                        let (s, account, model) = (s.clone(), self.cfg.accounts.iter().find(|a| a.name == s.account).cloned(), self.cfg.jev.model.clone());
                        Some(Box::new(move |body: &mut serde_json::Value| {
                            if let Some(waiting) = Self::waiting_call(&s, account.as_ref()) {
                                *body = crate::insights::request(&s, crate::insights::Kind::Approval, &model, Some(&waiting));
                            }
                        }))
                    }
                    _ => None,
                };
                asks.push((id, body, fill));
            }
        }
        for (id, body, fill) in asks {
            self.ask_jev_filled(id, body, fill);
        }
        self.insights.save(now);
    }

    /// Jev's read of a session, kept under the moment it was asked about.
    fn on_insight(&mut self, rest: String, answer: std::result::Result<serde_json::Value, String>) {
        let Some((signature, key)) = rest.split_once('|') else { return };
        let now = now_ms();
        match (answer, crate::insights::kind_of(signature)) {
            (Ok(v), Some(kind)) => {
                self.insights.answered(key.to_string(), signature.to_string(), crate::insights::read(kind, &v), now);
                self.insights.save(now);
                self.dirty = true;
            }
            _ => self.insights.failed(signature.to_string(), now),
        }
    }

    /// Sessions with the read of their moment applied (board::apply_insight): Jev's, and the
    /// landing their reply's status line says, when `[jev] sessions` is on; an agent's tag of
    /// them over both, whether or not it is (tags.rs, `insights::merged`).
    fn with_insights(&self, mut sessions: Vec<Session>, now: i64) -> Vec<Session> {
        let tags = tags::current();
        if !self.cfg.jev.sessions && tags.sessions.is_empty() {
            return sessions;
        }
        for s in &mut sessions {
            let tag = tags.sessions.get(&s.key).filter(|t| tags::holds(s, t)).map(|t| &t.insight);
            let (jev, said) = if self.cfg.jev.sessions {
                let jev = crate::insights::ask(s, now).and_then(|(_, signature)| self.insights.for_session(&s.key, &signature));
                (jev, crate::insights::landing_said(s))
            } else {
                (None, None)
            };
            if let Some(i) = crate::insights::merged(jev, said, tag) {
                board::apply_insight(s, &i);
                // A turn that left a server running is over: Your turn until you've seen it.
                if s.status == Status::Idle && s.background.is_some() && s.status_since.unwrap_or(0) > self.seen.seen_at(&s.key) {
                    s.status = Status::Unread;
                }
            }
        }
        sessions
    }

    /// Forgets the tags on sessions that have moved on since: something was sent to them since
    /// they were read, or a turn of theirs started since they were tagged (`tags::holds`).
    fn forget_stale_tags(&self, board: &Board) {
        let store = tags::current();
        if store.sessions.is_empty() {
            return;
        }
        let stale: Vec<(String, i64)> = store.sessions.iter().filter(|(key, t)| board.session(key).is_some_and(|(_, s)| !tags::holds(s, t))).map(|(key, t)| (key.clone(), t.at)).collect();
        if stale.is_empty() {
            return;
        }
        if let Err(e) = tags::forget(&stale) {
            eprintln!("Couldn't forget the tags on sessions that moved on: {e:#}");
        }
    }

    /// The replies suggested for each session, while they still answer where it stands
    /// (suggestions.rs, `board::apply_suggestions`). After Jev's read of it, before a snooze or a
    /// mark of unread moves when its status began.
    fn with_suggestions(&self, mut sessions: Vec<Session>) -> Vec<Session> {
        let store = suggestions::current();
        if store.sessions.is_empty() {
            return sessions;
        }
        for s in &mut sessions {
            let suggested = store.sessions.get(&s.key);
            board::apply_suggestions(s, suggested);
        }
        sessions
    }

    /// Forgets the replies suggested for sessions that have moved on since: something was sent to
    /// them since they were read, or a turn of theirs started since they were suggested. A turn that started and then ended would hold them again
    /// by `board::suggestions_hold` alone, so they're forgotten as soon as it's seen.
    fn forget_stale_suggestions(&self, board: &Board) {
        let store = suggestions::current();
        if store.sessions.is_empty() {
            return;
        }
        let stale: Vec<(String, i64)> = store.sessions.iter().filter(|(key, g)| board.session(key).is_some_and(|(_, s)| !board::suggestions_hold(s, g))).map(|(key, g)| (key.clone(), g.at)).collect();
        if stale.is_empty() {
            return;
        }
        if let Err(e) = suggestions::forget(&stale) {
            eprintln!("Couldn't forget the replies suggested for sessions that moved on: {e:#}");
        }
    }

    /// The whole call a session waits on you to allow, as its harness shows it, with its name
/// (`insights::waiting_words`), for Jev to judge its risk by: the arguments, not just the line the
/// excerpt has. A read that may be slow: run on Jev's thread, not the engine's. A command its agent
/// asks to run in your terminal comes first, whole, as `session-pending` gives it (actions.rs).
fn waiting_call(s: &Session, account: Option<&config::AccountCfg>) -> Option<String> {
    if let Some(asked) = crate::terminal::of_session(&s.key) {
        return crate::insights::waiting_words(&crate::terminal::pending(&asked, "your terminal"));
    }
    let pending = if crate::acp::hosts(&s.key) { crate::acp::pending(&s.key) } else { crate::harness::adapter(s.harness).pending(&crate::harness::Ctx { account: account? }, s) };
    crate::insights::waiting_words(&pending?)
}

/// Where sessions came from by Codealong's own record (origins.rs): started by an agent's
    /// `codealong start`, or forked from Codealong where the harness doesn't say. What the
    /// harness recorded comes first. With it, what each helper helps (`board::helper_of`): the
    /// session that started it as one (origins.json), or the PR whose automatic review it is
    /// (reviews.json), unless you made it your own (`SeenState::own`).
    fn with_origins(&self, mut sessions: Vec<Session>) -> Vec<Session> {
        let store = origins::current();
        let reviewers = autoreview::reviewers(&self.reviews);
        if store.sessions.is_empty() && reviewers.is_empty() {
            return sessions;
        }
        let keys: HashSet<String> = sessions.iter().map(|s| s.key.clone()).collect();
        for s in sessions.iter_mut() {
            if s.origin.is_none() {
                s.origin = store.origin_of(&s.key);
            }
            if !self.seen.is_own(&s.key) {
                s.helper = board::helper_of(store.helper_of(&s.key), |k| keys.contains(k), reviewers.get(s.key.as_str()).copied());
            }
        }
        sessions
    }

    /// Each headless run with the session that started it as its origin, where nothing else
    /// said where it came from: the nearest of its process's parents that's a running session's
    /// own (`board::started_by`). Looked up while it runs, a run's ancestry stays the same, and
    /// it's kept once the run ends and has no process to climb from, until it leaves the board
    /// or Codealong quits.
    fn with_run_origins(&self, mut sessions: Vec<Session>) -> Vec<Session> {
        let mut known = self.run_origins.lock().unwrap_or_else(|e| e.into_inner());
        if !sessions.iter().any(|s| s.headless) {
            known.clear();
            return sessions;
        }
        let by_pid: HashMap<u32, &str> = sessions.iter().filter(|s| s.live && !s.headless).filter_map(|s| Some((s.pid?, s.key.as_str()))).collect();
        let found: Vec<(String, String)> = sessions
            .iter()
            .filter(|s| s.headless && s.live && s.origin.is_none() && !known.contains_key(&s.key))
            .filter_map(|s| Some((s.key.clone(), board::started_by(s.pid?, procs::parent_pid, &by_pid)?.to_string())))
            .collect();
        known.extend(found);
        let on_board: HashSet<&str> = sessions.iter().filter(|s| s.headless).map(|s| s.key.as_str()).collect();
        known.retain(|k, _| on_board.contains(k.as_str()));
        for s in sessions.iter_mut().filter(|s| s.headless && s.origin.is_none()) {
            s.origin = known.get(&s.key).map(|from| SessionOrigin { key: from.clone(), how: OriginHow::Started });
        }
        sessions
    }

    /// The pages on the web each session made or showed you, for its header.
    fn with_artifacts(&self, mut sessions: Vec<Session>) -> Vec<Session> {
        for s in &mut sessions {
            if let Some(made) = self.artifacts.get(&s.key) {
                s.artifacts = made.clone();
            }
        }
        sessions
    }

    /// The Chrome tabs each session drove, to bring back from its menu.
    fn with_chrome(&self, mut sessions: Vec<Session>) -> Vec<Session> {
        if self.chrome_use.is_empty() {
            return sessions;
        }
        for s in &mut sessions {
            if let Some(u) = self.chrome_use.get(&s.key) {
                s.chrome = u.for_board(&self.chrome_profiles);
            }
        }
        sessions
    }

    /// Sessions a usage limit stopped, each with the account it could move to and go on
    /// (`start::limit_moves`).
    fn with_limits(&self, mut sessions: Vec<Session>, usage: &[Usage], now: i64) -> Vec<Session> {
        if sessions.iter().any(|s| s.limit.is_some()) {
            crate::start::limit_moves(&mut sessions, &self.cfg.accounts, usage, |h| harness::adapter(h).moves_accounts(), now);
        }
        sessions
    }

    /// Sessions a lost connection stopped, each saying whether Codealong tells it to carry on
    /// once its API can be reached (reconnect.rs): when config.toml says to, it runs, and its
    /// harness says where its API is.
    fn with_connections(&self, mut sessions: Vec<Session>) -> Vec<Session> {
        let on = self.cfg.sessions.continue_after_network_error;
        for s in &mut sessions {
            if let Some(lost) = s.connection_lost.as_mut() {
                lost.continues = on && s.live && harness::adapter(s.harness).api_address().is_some();
            }
        }
        sessions
    }

    /// Sessions with quiet, helpers' and headless runs' rules, their marks of unread and their
    /// snoozes applied (board::apply_quiet, board::apply_helper, board::apply_headless,
    /// board::apply_unread, board::apply_snooze). A snooze's time coming changes nothing on disk:
    /// the 30-second heartbeat's publish brings it back.
    fn with_snoozes(&self, mut sessions: Vec<Session>, now: i64) -> Vec<Session> {
        let unattended = sessions.iter().any(|s| s.helper.is_some() || s.headless);
        if self.seen.snoozed.is_empty() && self.seen.unread.is_empty() && self.seen.quiet.is_empty() && !unattended {
            return sessions;
        }
        for s in &mut sessions {
            // Quiet and a helper's or headless run's rule first, so that what you do to it
            // yourself after (a mark of unread, a snooze that ends) still brings it back.
            board::apply_quiet(s, self.seen.is_quiet(&s.key));
            board::apply_helper(s);
            board::apply_headless(s);
            // A mark of unread next: a snooze keeps a session marked unread off the bar too.
            board::apply_unread(s, self.seen.unread_of(&s.key), self.seen.seen_at(&s.key));
            board::apply_snooze(s, self.seen.snooze_of(&s.key), self.seen.seen_at(&s.key), now);
        }
        sessions
    }

    fn full_scan(&mut self) {
        self.load_config();
        self.load_state();
        self.remember_accounts();
        self.save_state();
        self.load_todos();
        // Only with every todo read: a list that didn't read would make every attachment look unused.
        if !self.todos_unreadable {
            let texts: Vec<String> = self.todos.todos.iter().map(|t| t.description.clone()).collect();
            let _ = std::thread::Builder::new().name("codealong-attachments".into()).spawn(move || {
                attachments::collect_garbage(&texts.iter().map(String::as_str).collect::<Vec<_>>());
            });
        }
        self.scan_sessions();
        self.load_repos();
        self.loading = false;
        self.publish();
        self.queue_git_all();
        self.github.poll(&self.seeds, true, Now::real());
        self.fetch_usage();
        self.poll_readers();
        self.watch();
        // The scan read config.toml and todos.json before the watcher started, and can take
        // minutes on a busy Mac: an edit made meanwhile is read now, as the watcher would have,
        // rather than when the file next changes.
        if config::read_text().ok().flatten() != self.config_text {
            self.reload();
        }
        self.reload_todos();
        self.refresh_visitors(false);
        self.load_pr_links();
        self.load_reviews();
        self.scan_pr_marks(false);
        self.load_previews();
        self.load_reusable();
    }

    /// What a reader goes by as it reads (`harness::Look`). Borrows only what you've seen, so
    /// the readers can be changed meanwhile.
    fn look(seen: &SeenState) -> harness::Look<'_> {
        let now = now_ms();
        harness::Look { seen, since: now - WINDOW_MS, now, ps: Default::default() }
    }

    /// Takes in what a read came to: the board is to be published again, and the lines sessions
    /// wrote are checked. True when a session appeared, which may need a lane (`place_sessions`).
    fn take(&mut self, applied: harness::Applied) -> bool {
        self.dirty |= applied.dirty;
        for l in applied.lines {
            self.check_new_lines(&l.path, l.from, l.to, &l.key);
        }
        self.read_found_logs(applied.found);
        applied.added
    }

    /// Has each reader ask what only a running service knows (`Reader::poll`).
    fn poll_readers(&mut self) {
        for r in &mut self.readers {
            r.poll(&harness::accounts_of(&self.cfg, r.harness()));
        }
    }

    /// Has each reader look at what the watcher never hears of (`Reader::glance`).
    fn glance_readers(&mut self) {
        let look = Self::look(&self.seen);
        let mut all = harness::Applied::default();
        for r in &mut self.readers {
            all.and(r.glance(&harness::accounts_of(&self.cfg, r.harness()), &look));
        }
        if self.take(all) {
            self.place_sessions();
        }
    }

    /// A new session may have started somewhere no lane covers yet: a chat, or a folder or repo
    /// outside your projects. Its folder is only looked up here, as reading it could wait on a
    /// privacy prompt; one whose read is held is looked at again once it lands. The repos are
    /// read again when one has.
    fn place_sessions(&mut self) {
        let unplaced = || {
            let chats = chat_roots();
            let index = board::SeedIndex::new(&self.seeds);
            self.session_cwds().into_iter().any(|cwd| {
                let p = Path::new(&cwd);
                index.best(p).is_none() && (chat_root(&chats, p).is_some() || (!self.reads.folders.is_out(p) && p.is_dir()))
            })
        };
        if unplaced() {
            self.rescan_repos();
        }
    }

    /// Reads usage on a background thread for the accounts whose harness keeps it somewhere slow
    /// to ask (`Adapter::polls_usage`: Claude Code's endpoint). Usage that comes with the
    /// sessions is the reader's (`Reader::usage`: Codex's, in its rollouts).
    fn fetch_usage(&mut self) {
        if self.usage_inflight {
            return;
        }
        let accounts: Vec<AccountCfg> = self.cfg.accounts.iter().filter(|a| harness::of(a).is_some_and(|h| h.polls_usage(a))).cloned().collect();
        self.usage_last = Some(Instant::now());
        if accounts.is_empty() {
            return;
        }
        self.usage_inflight = true;
        let tx = self.tx.clone();
        // `on_usage` hears that the read is over however it ends, so usage is read again.
        throttle::background("codealong-usage", move || usage::read_all(&accounts), move |list| {
            let _ = tx.send(Msg::UsageDone(list));
        });
    }

    /// Reads the team files of the repos checked out here that are due (`TeamFiles::due`), on a
    /// background thread: from each repo's default branch and the branches its open PRs merge
    /// into.
    fn read_team_files(&mut self) {
        let asks = self.team_file_asks();
        if asks.is_empty() {
            return;
        }
        let tx = self.tx.clone();
        // `on_read` hears that the read is over however it ends, so the next one can go.
        throttle::background("codealong-team-files", move || asks.iter().map(teamfile::read).collect(), move |answers| {
            let _ = tx.send(Msg::TeamFiles(answers));
        });
    }

    /// Reads the data settings of the accounts that are due (`TrainingPoll::due`), on a
    /// background thread.
    fn read_training(&mut self) {
        if self.passing {
            return;
        }
        let asks = self.training.due(&self.cfg.accounts, Instant::now());
        if asks.is_empty() {
            return;
        }
        let tx = self.tx.clone();
        // `on_read` hears that the read is over however it ends, so the next one can go.
        throttle::background("codealong-training", move || training::read_all(&asks), move |list| {
            let _ = tx.send(Msg::TrainingRead(list));
        });
    }

    /// The team files due to be read now (`TeamFiles::due`), marked under way.
    fn team_file_asks(&mut self) -> Vec<teamfile::Ask> {
        let repos: Vec<teamfile::Repo> = self
            .checkouts
            .iter()
            .filter_map(|(_, c)| {
                let (owner, name) = c.github.clone()?;
                let default = self.default_branch.get(&c.root).cloned().flatten().and_then(|b| b.strip_prefix("origin/").map(String::from));
                Some(teamfile::Repo { slug: format!("{owner}/{name}"), root: c.root.clone(), default })
            })
            .collect();
        let open = self.github.prs().iter().chain(self.github.requested()).flat_map(|(slug, prs)| prs.iter().filter(|p| p.state.is_open()).map(move |p| (slug.as_str(), p.base_ref.as_deref())));
        let bases = teamfile::bases(open);
        self.team_files.due(&repos, &bases, Instant::now())
    }

    /// The usage read is over. One that fell over, or never started, read nothing: the numbers
    /// and the backoff stay as they were, and the next read goes at its usual time.
    fn on_usage(&mut self, list: Option<Vec<Usage>>) {
        self.usage_inflight = false;
        let Some(list) = list else { return };
        self.usage_backoff = list.iter().any(|u| u.rate_limited);
        for u in list {
            let u = keep_last_good(self.usage.get(&u.account), u);
            self.usage.insert(u.account.clone(), u);
        }
        self.dirty = true;
    }

    /// Everything that depends on config.toml, read again. When nothing but the theme, the
    /// shortcuts or an account's star changed, a new board is enough: reading everything again also asks for Claude usage, and the usage
    /// endpoint turns away calls that come often.
    fn reload(&mut self) {
        let before = self.cfg.clone();
        self.load_config();
        if self.remember_accounts() {
            self.save_state();
        }
        let mut rest = self.cfg.clone();
        rest.appearance = before.appearance.clone();
        rest.shortcuts = before.shortcuts.clone();
        rest.debug = before.debug.clone();
        // The apps are read as the board is composed; nothing to rescan for them either, nor for
        // which PRs nudge by default.
        rest.apps = before.apps.clone();
        rest.pull_requests = before.pull_requests.clone();
        // Sounds are read as each one plays (sounds.rs).
        rest.sounds = before.sounds.clone();
        // A star only changes which account Auto picks; nothing to rescan for it.
        for a in &mut rest.accounts {
            if let Some(b) = before.accounts.iter().find(|b| b.name == a.name) {
                a.preferred = b.preferred;
            }
        }
        if rest == before {
            self.dirty = true;
            return;
        }
        self.scan_sessions();
        // The readers read their records' ends again, the lines before passed over: every record
        // is read whole again, off the engine's thread, an account's added since among them.
        self.scan_pr_marks(false);
        self.load_repos();
        self.queue_git_all();
        self.github.poll(&self.seeds, true, Now::real());
        self.usage.retain(|name, _| self.cfg.accounts.iter().any(|a| &a.name == name));
        self.usage_inflight = false;
        self.training.keep_only(&self.cfg.accounts);
        self.fetch_usage();
        self.watch();
        self.dirty = true;
    }

    fn run(&mut self, rx: Receiver<Msg>) {
        let mut pending: Vec<PathBuf> = Vec::new();
        let mut first_event: Option<Instant> = None;
        let mut last_event: Option<Instant> = None;
        let mut last_tick = Instant::now();
        let mut last_heartbeat = Instant::now();
        let mut last_databases = Instant::now();
        loop {
            let now = Instant::now();
            let mut wait = if !pending.is_empty() || self.dirty || self.landed_at.is_some() { Duration::from_millis(50) } else { Duration::from_secs(2) };
            // Wake for a git batch that's due, or a held git run.
            if self.dirty_git {
                let due = self.last_publish.map(|t| (t + throttle::GIT_PUBLISH_EVERY).saturating_duration_since(now)).unwrap_or_default();
                wait = wait.min(due.max(Duration::from_millis(10)));
            }
            if let Some(at) = self.git_queue.next_due() {
                wait = wait.min(at.saturating_duration_since(now).max(Duration::from_millis(10)));
            }
            match rx.recv_timeout(wait) {
                Ok(Msg::Fs(paths)) => {
                    let now = Instant::now();
                    first_event.get_or_insert(now);
                    last_event = Some(now);
                    pending.extend(paths);
                }
                Ok(Msg::GitDone(path, info)) => self.on_git(path, info),
                Ok(Msg::Github(event)) => self.on_github(event),
                Ok(Msg::RefreshGithub) => self.github.poll(&self.seeds, false, Now::real()),
                Ok(Msg::PrChanged) => self.github.on_changed(&self.seeds, Now::real()),
                Ok(Msg::Merged(slug, number, reply)) => {
                    self.dirty |= self.github.on_merged(slug, number, &self.seeds, Now::real());
                    // Now rather than on the next tick: the window shows it merged meanwhile.
                    self.publish();
                    let _ = reply.send(self.board.read().map(|b| b.generated_at).unwrap_or(0));
                }
                Ok(Msg::Approved(slug, number, reply)) => {
                    self.dirty |= self.github.on_approved(slug, number, &self.seeds, Now::real());
                    // Now rather than on the next tick: the button waits for it.
                    self.publish();
                    let _ = reply.send(self.board.read().map(|b| b.generated_at).unwrap_or(0));
                }
                Ok(Msg::Visitors(scan, found)) => self.dirty |= self.visitors.scanned(scan, found),
                Ok(Msg::PrMarks(found, pushed)) => {
                    // Marks and pushes read from new lines while the scan ran are kept.
                    for (key, prs) in found {
                        let have = self.pr_marks.entry(key).or_default();
                        for (pr, at) in prs {
                            prlinks::add_mark(have, pr, at);
                        }
                    }
                    for (key, pushes) in pushed {
                        let have = self.pr_pushes.entry(key).or_default();
                        for (branch, at) in pushes {
                            prlinks::add_push(have, branch, at);
                        }
                    }
                    self.dirty = true;
                }
                Ok(Msg::Artifacts(found)) => {
                    // Lines read as they were written while the scan ran are kept, or taken
                    // over by what the scan read after them.
                    for (key, made) in found {
                        let have = self.artifacts.entry(key).or_default();
                        for a in made {
                            artifacts::add(have, a);
                        }
                    }
                    self.dirty = true;
                }
                Ok(Msg::ChromeUse(found, profiles)) => self.take_chrome_use(found, profiles),
                Ok(Msg::ChromeProfile(key, group, dir, profiles)) => {
                    self.chrome_profiles = profiles;
                    if let Some(u) = self.chrome_use.get_mut(&key).filter(|u| u.group == group) {
                        if dir.is_some() && u.profile != dir {
                            u.profile = dir;
                            self.dirty = true;
                        }
                    }
                }
                Ok(Msg::RefreshUsage) => {
                    self.fetch_usage();
                    self.training.on_refresh();
                    self.read_training();
                }
                Ok(Msg::LimitMoved(why)) => {
                    self.limit_move_warning = why;
                    self.dirty = true;
                }
                Ok(Msg::PrLinksChanged) => {
                    self.load_pr_links();
                    self.dirty = true;
                }
                Ok(Msg::Nudged(why)) => {
                    self.nudge_warning = why;
                    self.dirty = true;
                }
                Ok(Msg::RunReported(why)) => {
                    self.run_report_warning = why;
                    self.dirty = true;
                }
                Ok(Msg::Continued(why)) => {
                    self.continue_warning = why;
                    self.dirty = true;
                }
                Ok(Msg::Reported(why)) => {
                    self.report_warning = why;
                    self.dirty = true;
                }
                Ok(Msg::Reviewed(why)) => {
                    self.review_warning = why;
                    self.load_reviews();
                    self.dirty = true;
                }
                Ok(Msg::Hosted) | Ok(Msg::Read) => self.dirty = true,
                Ok(Msg::SetArchived(keys, archived, reply)) => {
                    let now = now_ms();
                    for key in &keys {
                        self.seen.set_archived(key, archived, now);
                    }
                    self.save_state();
                    // A harness with an archive of its own reads what's in it now.
                    let look = Self::look(&self.seen);
                    let harnesses: HashSet<_> = keys.iter().filter_map(|k| k.split(':').next().and_then(harness::named)).map(|a| a.harness()).collect();
                    for r in self.readers.iter_mut().filter(|r| harnesses.contains(&r.harness())) {
                        r.archives_changed(&harness::accounts_of(&self.cfg, r.harness()), &look);
                    }
                    // Now rather than on the next tick: the window shows it already, and waits for it.
                    self.publish();
                    let _ = reply.send(self.board.read().map(|b| b.generated_at).unwrap_or(0));
                }
                Ok(Msg::SetPinned(keys)) => {
                    self.seen.set_pinned(keys);
                    self.save_state();
                    self.dirty = true;
                }
                Ok(Msg::SetPinnedWorktrees(ids)) => {
                    self.seen.set_pinned_worktrees(ids);
                    self.save_state();
                    self.dirty = true;
                }
                Ok(Msg::SetPinnedPrs(keys)) => {
                    self.seen.set_pinned_prs(keys);
                    self.save_state();
                    self.dirty = true;
                }
                Ok(Msg::SetTheme(theme)) => {
                    // Published now, not on the next tick: the app's appearance follows the board.
                    self.cfg.appearance.theme = theme;
                    self.publish();
                }
                Ok(Msg::SetZoom(zoom)) => {
                    self.cfg.appearance.zoom = zoom;
                    self.publish();
                }
                Ok(Msg::SetPreferred(account, preferred)) => {
                    // Published now: the quick panel's Auto takes stars from the board, and the
                    // watcher's reload comes later.
                    if let Some(a) = self.cfg.accounts.iter_mut().find(|a| a.name == account) {
                        a.preferred = preferred;
                    }
                    self.publish();
                }
                Ok(Msg::Ping(sent)) => perf::lag("engine", sent.elapsed()),
                Ok(Msg::Todo(op, reply)) => {
                    let _ = reply.send(self.change_todos(op));
                }
                Ok(Msg::JevDone(id, answer)) => self.on_jev(id, answer),
                Ok(Msg::DismissTodoHint(id)) => self.dismiss_todo_hint(id),
                // A prompt answered lands many reads at once; they're used together.
                Ok(Msg::Landed) => self.landed_at = Some(Instant::now()),
                Ok(Msg::Servers(servers)) => {
                    self.previews.servers = servers;
                    self.dirty = true;
                }
                Ok(Msg::Previews(reply)) => {
                    self.load_previews();
                    // Now rather than on the next tick: the window waits for it.
                    self.publish();
                    let _ = reply.send(self.board.read().map(|b| b.generated_at).unwrap_or(0));
                }
                Ok(Msg::Reusable(reply)) => {
                    self.load_reusable();
                    self.publish();
                    let _ = reply.send(self.board.read().map(|b| b.generated_at).unwrap_or(0));
                }
                Ok(Msg::PrCard(slug, raw, login, reply)) => {
                    let item = match self.board.read() {
                        Ok(b) => self.pr_card(&b.lanes, &slug, &raw, login.as_deref()),
                        Err(_) => self.pr_card(&[], &slug, &raw, login.as_deref()),
                    };
                    let _ = reply.send(item);
                }
                Ok(Msg::Reread(paths, reply)) => {
                    self.apply_fs(paths);
                    // Now rather than on the next tick: the window shows it already, and waits for it.
                    self.publish();
                    let _ = reply.send(self.board.read().map(|b| b.generated_at).unwrap_or(0));
                }
                Ok(Msg::TodoStarted { ids, lane, session, pending }) => self.todos_started(ids, lane, session, pending),
                Ok(Msg::UsageDone(list)) => self.on_usage(list),
                Ok(Msg::TeamFiles(answers)) => self.dirty |= self.team_files.on_read(answers),
                Ok(Msg::TrainingRead(list)) => self.dirty |= self.training.on_read(list),
                Ok(Msg::TrainingOne(asked_as, t)) => self.dirty |= self.training.on_stopped(&asked_as, t),
                Ok(Msg::MarkSeen(keys, reply)) => {
                    let now = now_ms();
                    for k in keys {
                        self.seen.mark(&k, now);
                    }
                    self.save_state();
                    self.publish();
                    if let Some(reply) = reply {
                        let _ = reply.send(self.board.read().map(|b| b.generated_at).unwrap_or(0));
                    }
                }
                Ok(Msg::MarkUnread(keys, reply)) => {
                    let now = now_ms();
                    for k in keys {
                        self.seen.mark_unread(&k, now);
                    }
                    self.save_state();
                    self.publish();
                    if let Some(reply) = reply {
                        let _ = reply.send(self.board.read().map(|b| b.generated_at).unwrap_or(0));
                    }
                }
                Ok(Msg::Snooze(keys, until, reply)) => {
                    let now = now_ms();
                    for k in keys {
                        match until {
                            Some(until) if until > now => self.seen.snooze(&k, now, until),
                            _ => self.seen.wake(&k, now),
                        }
                    }
                    self.save_state();
                    // Now rather than on the next tick: the window shows it already, and waits for it.
                    self.publish();
                    let _ = reply.send(self.board.read().map(|b| b.generated_at).unwrap_or(0));
                }
                Ok(Msg::SnoozeReviews(keys, until, reply)) => {
                    let now = now_ms();
                    for k in keys {
                        match until {
                            Some(until) if until > now => self.seen.snooze_review(&k, now, until),
                            _ => self.seen.wake_review(&k, now),
                        }
                    }
                    self.save_state();
                    self.publish();
                    let _ = reply.send(self.board.read().map(|b| b.generated_at).unwrap_or(0));
                }
                Ok(Msg::Suggested(reply)) => {
                    self.publish();
                    let _ = reply.send(self.board.read().map(|b| b.generated_at).unwrap_or(0));
                }
                Ok(Msg::Quiet(keys, quiet, reply)) => {
                    let mut changed = false;
                    for k in keys {
                        changed |= self.seen.set_quiet(&k, quiet);
                    }
                    if changed {
                        self.save_state();
                        self.publish();
                    }
                    let _ = reply.send(self.board.read().map(|b| b.generated_at).unwrap_or(0));
                }
                Ok(Msg::Pin(keys, pinned, reply)) => {
                    let keys: Vec<String> = self.board.read().map(|b| board::pinnable(&b.lanes, &keys, pinned).into_iter().cloned().collect()).unwrap_or_default();
                    let mut changed = false;
                    // In reverse, so sessions pinned together keep the order they were asked in.
                    for k in keys.iter().rev() {
                        changed |= self.seen.pin(k, pinned);
                    }
                    if changed {
                        self.save_state();
                        self.publish();
                    }
                    let _ = reply.send(self.board.read().map(|b| b.generated_at).unwrap_or(0));
                }
                Ok(Msg::Own(keys, reply)) => {
                    let mut changed = false;
                    for k in keys {
                        changed |= self.seen.make_own(&k);
                    }
                    if changed {
                        self.save_state();
                        self.publish();
                    }
                    let _ = reply.send(self.board.read().map(|b| b.generated_at).unwrap_or(0));
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
            if !pending.is_empty() {
                let quiet = last_event.map(|t| t.elapsed() >= Duration::from_millis(150)).unwrap_or(true);
                let overdue = first_event.map(|t| t.elapsed() >= Duration::from_millis(600)).unwrap_or(false);
                if quiet || overdue {
                    let batch = std::mem::take(&mut pending);
                    first_event = None;
                    let (n, t) = (batch.len(), Instant::now());
                    let busiest = perf::busiest(&batch);
                    self.apply_fs(batch);
                    self.perf.fs(n, t.elapsed(), busiest);
                }
            }
            if self.landed_at.is_some_and(|t| t.elapsed() >= LANDED_QUIET) {
                self.landed_at = None;
                self.rescan_repos();
            }
            for path in self.git_queue.due(Instant::now()) {
                self.send_git(path);
            }
            if last_databases.elapsed() >= DATABASES_EVERY {
                last_databases = Instant::now();
                self.glance_readers();
            }
            if last_tick.elapsed() >= Duration::from_secs(10) {
                last_tick = Instant::now();
                self.tick();
            }
            if last_heartbeat.elapsed() >= Duration::from_secs(30) {
                // Codex turns go stale with time alone; republishing picks that up.
                last_heartbeat = Instant::now();
                self.dirty = true;
            }
            if throttle::publish_now(self.dirty, self.dirty_git, self.last_publish.map(|t| t.elapsed()), PUBLISH_EVERY) {
                self.publish();
            }
        }
    }

    fn load_config(&mut self) {
        let text = config::read_text();
        self.config_text = text.as_ref().ok().cloned().flatten();
        match text.and_then(|t| t.map(|t| config::load_text(&t)).transpose()) {
            Ok(Some(cfg)) => {
                // A sound that isn't there reads fine and would only be silent, so it's said here,
                // as Settings says it when it saves.
                self.config_warning = config::check_sounds(&cfg.sounds).err();
                self.cfg = cfg;
            }
            Ok(None) => {
                let cfg = first_run_config();
                match config::write(&cfg) {
                    Ok(text) => self.config_text = Some(text),
                    Err(e) => self.config_warning = Some(format!("Couldn't save config.toml: {e}")),
                }
                self.cfg = cfg;
            }
            Err(e) => {
                self.config_warning = Some(format!("{e:#}. Codealong is using the last good settings."));
                if self.cfg.accounts.is_empty() {
                    self.cfg.accounts = detect::first_run_accounts();
                }
            }
        }
        // What the app's commands act on, so they go by what the board shows (`config::current`).
        // A one-pass engine (`--snapshot`, the command without the app) leaves the file to them,
        // so a mistake in it is said, not covered over.
        if !self.passing && !cfg!(test) {
            config::running_on(&self.cfg);
        }
        self.previews.chrome_profiles = self.cfg.projects.iter().filter_map(|p| Some((p.name.clone(), p.chrome_profile.clone()?))).collect();
        perf::set_enabled(self.cfg.debug.perf_log);
        search::configure(search::Scope { accounts: self.cfg.accounts.clone(), days: self.cfg.search.days });
    }

    /// Every folder config.toml lists is known from then on, so an account you take out isn't
    /// suggested again, whether Settings or an editor took it out. True when one was new.
    fn remember_accounts(&mut self) -> bool {
        let keys: Vec<String> = self.cfg.accounts.iter().map(|a| detect::folder_key(&a.home)).collect();
        self.seen.know_accounts(keys)
    }

    fn scan_sessions(&mut self) {
        let look = Self::look(&self.seen);
        for r in &mut self.readers {
            r.scan(&harness::accounts_of(&self.cfg, r.harness()), &look);
        }
    }

    fn session_cwds(&self) -> BTreeSet<String> {
        self.readers.iter().flat_map(|r| r.cwds()).collect()
    }

    /// Every session's record that's a file of lines, with the session's key (`Reader::logs`).
    fn session_logs(&self) -> Vec<(PathBuf, String)> {
        self.readers.iter().flat_map(|r| r.logs()).collect()
    }

    /// The checkouts and their worktrees, as seeds for lanes. Files in folders the scan doesn't
    /// own are read through `self.reads`: one macOS holds for a privacy prompt leaves out only
    /// its own repo or session folder, or shows its worktree as `git worktree list` gives it, and
    /// is filled in once the read lands.
    fn load_repos(&mut self) {
        let h = home();
        let roots: Vec<PathBuf> = self.cfg.scan.iter().map(|s| expand(s)).collect();
        let mut walked = self.reads.roots.read(roots.clone(), ROOT_WAIT);
        let mut found: Vec<Checkout> = roots.iter().filter_map(|r| walked.remove(r)).flatten().collect();
        found.sort_by(|a, b| a.root.cmp(&b.root));
        found.dedup_by(|a, b| a.root == b.root);
        // Codealong's own source among them, in a project or not (improve.rs): one small file
        // looked for per checkout, each scan, so a clone whose files land after its `.git` is
        // found on the next.
        self.codealong_sources = found.iter().filter(|c| crate::improve::is_source(&c.root)).map(|c| (c.root.clone(), c.identity.clone())).collect();
        let known: HashSet<PathBuf> = found.iter().map(|c| c.root.clone()).collect();
        let cwds = self.session_cwds();
        let chats = chat_roots();
        // A chat's folder is never one of your repos, and the Codex app keeps its chats in
        // Documents, where a read can wait on a privacy prompt: they aren't looked up.
        let located = self.reads.folders.read(cwds.iter().map(PathBuf::from).filter(|p| chat_root(&chats, p).is_none()), HELD_WAIT);
        let extra: BTreeSet<PathBuf> = located.values().flatten().map(|(_, main)| main.clone()).filter(|main| !known.contains(main)).collect();
        let mut loaded = self.reads.checkouts.read(extra.iter().cloned(), HELD_WAIT);
        found.extend(extra.iter().filter_map(|r| loaded.remove(r).flatten()));

        self.checkouts.clear();
        for c in found {
            let Some(identity) = c.identity.clone() else { continue };
            if let Some(p) = self.cfg.projects.iter().find(|p| p.repos.iter().any(|pat| repos::matches(&identity, pat))) {
                self.checkouts.push((p.name.clone(), c));
            }
        }

        let mut seeds = Vec::new();
        self.gitdir_to_lane.clear();
        self.lane_base.clear();
        self.lane_checkout.clear();
        // `git worktree list` and the default branch for every checkout, in parallel.
        let known_bases = self.default_branch.clone();
        let listed: HashMap<PathBuf, (Vec<repos::Worktree>, Option<String>)> = parallel_map(
            self.checkouts.iter().map(|(_, c)| c.clone()).collect(),
            |c| {
                let base = known_bases.get(&c.root).cloned().unwrap_or_else(|| repos::default_branch(&c));
                Some((c.root.clone(), (repos::list_worktrees(&c), base)))
            },
        )
        .into_iter()
        .collect();
        // Each worktree's `.git` file and `.gitignore`. One whose files are held shows as `git
        // worktree list` gives it (git only looks its path up) and gets git once they land.
        let paths: Vec<PathBuf> = listed.values().flat_map(|(wts, _)| wts.iter().map(|w| w.path.clone())).collect();
        let mut files = self.reads.worktrees.read(paths.iter().cloned(), HELD_WAIT);
        self.held_worktrees = paths.into_iter().filter(|p| !files.contains_key(p)).collect();
        self.ignores.clear();
        for (project, c) in &self.checkouts {
            let Some((worktrees, base)) = listed.get(&c.root).cloned() else { continue };
            self.default_branch.insert(c.root.clone(), base.clone());
            let default_name = base.as_ref().map(|b| b.split_once('/').map(|(_, n)| n.to_string()).unwrap_or_else(|| b.clone()));
            let repo_name = c.github.as_ref().map(|(_, n)| n.clone()).unwrap_or_else(|| c.name.clone());
            for wt in worktrees {
                if let Some(f) = files.remove(&wt.path) {
                    if let Some(gitdir) = f.gitdir {
                        self.gitdir_to_lane.insert(gitdir, wt.path.clone());
                    }
                    if let Some(gi) = f.ignore {
                        self.ignores.insert(wt.path.clone(), gi);
                    }
                }
                self.lane_base.insert(wt.path.clone(), base.clone());
                self.lane_checkout.insert(wt.path.clone(), c.root.clone());
                seeds.push(LaneSeed {
                    callsign: repos::callsign(c, &wt.path, wt.branch.as_deref(), &h),
                    origin: repos::origin(c, &wt.path, &h),
                    path: wt.path,
                    project: project.clone(),
                    repo: repo_name.clone(),
                    repo_slug: c.slug(),
                    head_owners: c.github_owners.clone(),
                    branch: wt.branch,
                    head: wt.head,
                    default_branch: default_name.clone(),
                    elsewhere: false,
                    chats: false,
                    starts_chats: false,
                    exact: false,
                });
            }
        }
        for p in &self.cfg.projects {
            for f in &p.folders {
                let path = expand(f);
                if path.is_dir() && !seeds.iter().any(|s| s.path == path) {
                    let name = file_name(&path);
                    seeds.push(folder_seed(path, &p.name, &name, false));
                }
            }
        }
        // Elsewhere: every folder a session ran in outside your projects.
        let found = |p: &Path| {
            if !p.is_dir() {
                return Found::Gone;
            }
            match located.get(p) {
                Some(Some((wt, _))) => Found::Repo(wt.clone()),
                Some(None) => Found::Plain,
                // Held: looked at again once it lands.
                None => Found::Held,
            }
        };
        let strays = stray_seeds(&seeds, &cwds, &chats, &config::chats_dir(), &h, found);
        seeds.extend(strays);

        let live: HashSet<&PathBuf> = seeds.iter().map(|s| &s.path).collect();
        self.git.retain(|p, _| live.contains(p));
        self.seeds = seeds;
        if let Some(w) = &self.servers {
            w.set_roots(self.server_roots());
        }
        // An engine that makes one pass is gone before GitHub would answer.
        if !self.passing {
            self.github.ask_names(&self.seeds, Instant::now());
        }
    }

    /// The worktrees whose servers are looked for: those in projects. A folder outside them can
    /// be your home folder, where every app's own server would be taken for the worktree's.
    fn server_roots(&self) -> Vec<PathBuf> {
        let h = home();
        self.seeds.iter().filter(|s| !s.elsewhere && s.path != h).map(|s| s.path.clone()).collect()
    }

    fn load_previews(&mut self) {
        if let Some(store) = read_or_warn(previews::load(), &mut self.previews_warning) {
            self.previews.set = store.previews;
        }
    }

    fn load_reusable(&mut self) {
        if let Some(store) = read_or_warn(reusable::load(), &mut self.reusable_warning) {
            self.reusable = store.by_path();
        }
    }

    fn git_targets(&self) -> Vec<PathBuf> {
        self.seeds.iter().filter(|s| !s.elsewhere && s.origin != Origin::Folder && !self.held_worktrees.contains(&s.path)).map(|s| s.path.clone()).collect()
    }

    fn base_for(&self, path: &Path) -> Option<String> {
        self.lane_base.get(path).cloned().flatten()
    }

    /// Asks for git in a worktree. Runs at once when it's been quiet, otherwise once the
    /// current run ends or the spacing allows, whichever is later.
    fn queue_git(&mut self, path: PathBuf) {
        if self.held_worktrees.contains(&path) {
            return;
        }
        if let Verdict::Run = self.git_queue.request(path.clone(), Instant::now()) {
            self.send_git(path);
        }
    }

    fn send_git(&mut self, path: PathBuf) {
        let base = self.base_for(&path);
        // With no worker left to take it, nothing is running there.
        if let Err(unsent) = self.git_jobs.send((path, base)) {
            self.git_queue.done(&unsent.0 .0, Instant::now());
        }
    }

    /// Every worktree, the ones with the latest activity first: with many worktrees and a busy
    /// Mac the first results take a while, and those are the ones on screen.
    fn queue_git_all(&mut self) {
        let activity: HashMap<String, (bool, i64)> = self
            .board
            .read()
            .map(|b| b.lanes.iter().map(|l| (l.id.clone(), (l.sessions.iter().any(|s| s.live), l.sessions.iter().map(|s| s.last_activity).max().unwrap_or(0)))).collect())
            .unwrap_or_default();
        let mut targets = self.git_targets();
        targets.sort_by_key(|p| std::cmp::Reverse(activity.get(&p.to_string_lossy().to_string()).copied().unwrap_or((false, 0))));
        for p in targets {
            self.queue_git(p);
        }
    }

    fn on_git(&mut self, path: PathBuf, info: Option<GitInfo>) {
        self.perf.git_jobs += 1;
        // A run that fell over leaves what was known of the worktree as it is.
        if let Some(info) = info.filter(|_| self.seeds.iter().any(|s| s.path == path)) {
            self.git.insert(path.clone(), info);
            self.dirty_git = true;
        }
        if self.git_queue.done(&path, Instant::now()) {
            self.queue_git(path);
        }
    }

    /// Takes in what GitHub was asked in the background (ghpoll.rs).
    fn on_github(&mut self, event: ghpoll::Event) {
        self.dirty |= self.github.on_event(event, &self.seeds, Now::real());
    }

    /// What isn't working in the background, as the windows say it: a few words for each, and
    /// the whole of it.
    fn notices(&self) -> Vec<Notice> {
        let held = held_warning(&self.reads.out_for(HELD_WARN_AFTER));
        let team_files = self.team_files.notice();
        let ledger = crate::ledger::problem();
        let said = |key: &str, title: &str, text: Option<&String>| Some(Notice { key: key.into(), title: title.into(), text: text?.clone(), until: None });
        [
            said("config", "config.toml has a problem", self.config_warning.as_ref()),
            self.github.notice(),
            said("watch", "Live updates are off", self.watch_warning.as_ref()),
            said("todos", "todos.json has a problem", self.todos_warning.as_ref()),
            said("todos-lock", "Todos can't be changed", self.todos_lock_warning.as_ref()),
            said("state", "state.json has a problem", self.state_warning.as_ref()),
            said("pr-links", "PR links weren't read", self.pr_links_warning.as_ref()),
            said("previews", "Previews weren't read", self.previews_warning.as_ref()),
            said("reusable", "Reusable worktrees weren't read", self.reusable_warning.as_ref()),
            said("held", "Waiting to open a folder", held.as_ref()),
            said("limits", "A session didn't move", self.limit_move_warning.as_ref()),
            said("nudges", "A session wasn't told its PR was approved", self.nudge_warning.as_ref()),
            said("terminal-runs", "A session wasn't told what a command printed", self.run_report_warning.as_ref()),
            said("reconnect", "A session didn't carry on after its connection came back", self.continue_warning.as_ref()),
            said("helpers", "A session wasn't told what its helper did", self.report_warning.as_ref()),
            said("reviews", "A pull request wasn't reviewed", self.review_warning.as_ref()),
            said("team-files", "A repo's team settings aren't used", team_files.as_ref()),
            said("ledger", "Usage is missing some sessions", ledger.as_ref()),

        ]
        .into_iter()
        .flatten()
        .chain(self.store_warnings.iter().map(|(file, why)| Notice { key: file.clone(), title: format!("{file} has a problem"), text: why.clone(), until: None }))
        .collect()
    }

    fn tick(&mut self) {
        // What the watcher doesn't say: a file its writer holds open, a process that came or went.
        let look = Self::look(&self.seen);
        let mut all = harness::Applied::default();
        for r in &mut self.readers {
            all.and(r.tick(&harness::accounts_of(&self.cfg, r.harness()), &look));
        }
        if self.take(all) {
            self.place_sessions();
        }
        self.poll_readers();
        self.github.tick(&self.seeds, Now::real());
        let every = if self.usage_backoff { USAGE_BACKOFF } else { USAGE_EVERY };
        if self.usage_last.map(|t| t.elapsed() >= every).unwrap_or(true) {
            self.fetch_usage();
        }
        self.read_team_files();
        self.read_training();
        // A new account's folder appears when it first signs in; start reading it.
        if self.missing_homes.iter().any(|p| p.exists()) {
            self.reload();
        }
        // Reads still out after a while are said on the board (held_warning).
        if !self.reads.out_for(HELD_WARN_AFTER).is_empty() {
            self.dirty = true;
        }
    }

    fn classify_repo_path(&self, p: &Path, batch: &mut FsBatch) -> RepoEvent {
        for (_, c) in &self.checkouts {
            if let Ok(rel) = p.strip_prefix(&c.git_dir) {
                let mut comps = rel.components().map(|c| c.as_os_str().to_string_lossy().into_owned());
                return match comps.next().as_deref() {
                    Some("worktrees") => match comps.next() {
                        Some(name) => {
                            let admin = c.git_dir.join("worktrees").join(&name);
                            match self.gitdir_to_lane.get(&admin) {
                                // `git worktree remove` deletes the worktree's folder, then its
                                // folder here: a lane with either gone is gone, and only a rescan
                                // takes it off the board.
                                Some(lane) if batch.exists(&admin) && batch.exists(lane) => RepoEvent::Lane(lane.clone()),
                                _ => RepoEvent::Worktrees,
                            }
                        }
                        None => RepoEvent::Worktrees,
                    },
                    Some("refs") | Some("packed-refs") | Some("FETCH_HEAD") => RepoEvent::Checkout(c.root.clone()),
                    Some("HEAD") | Some("index") | Some("ORIG_HEAD") | Some("MERGE_HEAD") => RepoEvent::Lane(c.root.clone()),
                    _ => RepoEvent::Ignore,
                };
            }
        }
        let Some(i) = batch.lane_of(p) else { return RepoEvent::Ignore };
        let seed = &self.seeds[i];
        if seed.elsewhere || seed.origin == Origin::Folder {
            return RepoEvent::Ignore;
        }
        // A worktree deleted without git (`rm -rf`) leaves its folder in `.git/worktrees`, where
        // nothing changes: its own files going are the only sign.
        if !batch.lane_there(i, &seed.path) {
            return RepoEvent::Worktrees;
        }
        // Git runs there already, whatever else changed in it.
        if batch.changed.contains(&i) {
            return RepoEvent::Lane(seed.path.clone());
        }
        if p.components().any(|c| c.as_os_str() == "node_modules" || c.as_os_str() == ".git") {
            return RepoEvent::Ignore;
        }
        if let Some(gi) = self.ignores.get(&seed.path) {
            if batch.ignored(gi, &seed.path, p) {
                return RepoEvent::Ignore;
            }
        }
        batch.changed.insert(i);
        RepoEvent::Lane(seed.path.clone())
    }

    fn apply_fs(&mut self, mut paths: Vec<PathBuf>) {
        paths.sort();
        paths.dedup();
        let config_path = config::config_path();
        let todos_path = todos::path();
        let state_path = crate::state::path();
        // Each harness's own files, by reader and account, in the order they're read.
        let accounts: Vec<Vec<AccountCfg>> = self.readers.iter().map(|r| harness::accounts_of(&self.cfg, r.harness())).collect();
        let mut owned: BTreeMap<(usize, usize), Vec<PathBuf>> = BTreeMap::new();
        let mut lanes = BTreeSet::new();
        let mut reload_repos = false;
        let mut reload_config = false;
        let (pr_links_path, reviews_path, previews_path, reusable_path) = (prlinks::path(), autoreview::path(), previews::path(), reusable::path());
        let mut batch = FsBatch::new(&self.seeds);
        let mut moved = HashSet::new();
        'paths: for p in paths {
            if p == config_path {
                reload_config = true;
                continue;
            }
            if p == todos_path {
                self.reload_todos();
                continue;
            }
            if p == state_path {
                // The engine's own writes come back here; only a file that didn't read is read
                // again, now that it's been fixed or deleted.
                if self.state_unreadable {
                    if state_path.exists() {
                        self.load_state();
                    } else {
                        // Deleted: what's in memory is the state from here on.
                        self.state_unreadable = false;
                        self.state_warning = None;
                    }
                    if !self.state_unreadable {
                        self.remember_accounts();
                        self.save_state();
                        // What's archived and pinned decides what the harnesses' readers keep.
                        self.scan_sessions();
                        self.scan_pr_marks(false);
                    }
                    self.dirty = true;
                }
                continue;
            }
            if p == pr_links_path {
                self.load_pr_links();
                self.dirty = true;
                continue;
            }
            if p == reviews_path {
                self.load_reviews();
                self.dirty = true;
                continue;
            }
            if p == previews_path {
                self.load_previews();
                self.dirty = true;
                continue;
            }
            if p == reusable_path {
                self.load_reusable();
                self.dirty = true;
                continue;
            }
            for (i, r) in self.readers.iter().enumerate() {
                if let Some(j) = accounts[i].iter().position(|a| r.owns(a, &p)) {
                    owned.entry((i, j)).or_default().push(p);
                    continue 'paths;
                }
            }
            match self.classify_repo_path(&p, &mut batch) {
                RepoEvent::Lane(l) => {
                    lanes.insert(l);
                }
                // A fetch writes a file for each ref it moves: the checkout is looked at once.
                RepoEvent::Checkout(root) if !moved.insert(root.clone()) => {}
                RepoEvent::Checkout(root) => {
                    lanes.extend(self.seeds.iter().filter(|s| self.lane_checkout.get(&s.path) == Some(&root)).map(|s| s.path.clone()));
                    // A fetch may have moved the branch a team file is read from.
                    if let Some((owner, name)) = self.checkouts.iter().find(|(_, c)| c.root == root).and_then(|(_, c)| c.github.clone()) {
                        self.team_files.on_refs_moved(&format!("{owner}/{name}"));
                    }
                }
                RepoEvent::Worktrees => reload_repos = true,
                RepoEvent::Ignore => {}
            }
        }

        if reload_config {
            self.reload();
            return;
        }
        let look = Self::look(&self.seen);
        let mut all = harness::Applied::default();
        for ((i, j), paths) in owned {
            all.and(self.readers[i].apply(&accounts[i][j], &paths, &look));
        }
        let added = self.take(all);
        if reload_repos {
            self.rescan_repos();
        } else if added {
            self.place_sessions();
        }
        for l in lanes {
            self.queue_git(l);
        }
    }

    /// Reads the repos again: worktrees came or went, a session started where no lane covers,
    /// or reads macOS held landed. Git runs in the worktrees that are new or readable now.
    fn rescan_repos(&mut self) {
        let before: HashSet<PathBuf> = self.git_targets().into_iter().collect();
        self.load_repos();
        let added: Vec<PathBuf> = self.git_targets().into_iter().filter(|p| !before.contains(p)).collect();
        for p in added {
            self.queue_git(p);
        }
        self.watch();
        self.refresh_visitors(false);
        self.dirty = true;
    }

    /// Worktrees an agent made from somewhere else (temporary folders, Codex worktrees, a
    /// `git worktree add` next to the checkout) that no session runs in, the text that
    /// identifies each in a log (its folder name, or its path under `~/.codex/worktrees`), and
    /// when it was made.
    fn orphan_needles(&self) -> Vec<(PathBuf, String, i64)> {
        let index = board::SeedIndex::new(&self.seeds);
        // The folders of the lanes sessions run in.
        let lived: HashSet<&Path> = self.session_cwds().iter().filter_map(|c| index.best(Path::new(c))).map(|i| self.seeds[i].path.as_path()).collect();
        let h = home();
        self.seeds
            .iter()
            .filter(|s| matches!(s.origin, Origin::Temp | Origin::Codex | Origin::Script | Origin::Other))
            .filter(|s| !lived.contains(s.path.as_path()))
            .filter_map(|s| {
                let needle = match s.path.strip_prefix(&h) {
                    Ok(rel) if s.origin == Origin::Codex => rel.to_string_lossy().into_owned(),
                    _ => file_name(&s.path),
                };
                let made = std::fs::metadata(&s.path).and_then(|m| m.created()).ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis() as i64).unwrap_or(0);
                (needle.len() >= 12).then(|| (s.path.clone(), needle, made))
            })
            .collect()
    }

    /// Searches session logs, subagents' included, for orphan worktrees. A file is read only
    /// for the orphans made before its last write (give or take an hour), in the background
    /// unless `sync`.
    fn refresh_visitors(&mut self, sync: bool) {
        let needles = self.orphan_needles();
        if needles == self.needles && !sync {
            return;
        }
        self.needles = needles.clone();
        if needles.is_empty() {
            self.visitors.clear();
            return;
        }
        let number = self.visitors.begin();
        let files = self.session_logs();
        let scan = move || -> HashMap<PathBuf, Vec<String>> {
            let found = parallel_map(files, |(path, key)| {
                let written = mtime_ms(&path)?;
                let could: Vec<(PathBuf, String)> = needles.iter().filter(|(_, _, made)| written >= made - 3_600_000).map(|(l, n, _)| (l.clone(), n.clone())).collect();
                if could.is_empty() {
                    return None;
                }
                let hits = file_mentions(&path, &could);
                (!hits.is_empty()).then(|| (key, hits))
            });
            // Earliest work first: usually the session that made the worktree.
            let mut timed: HashMap<PathBuf, Vec<(i64, String)>> = HashMap::new();
            for (key, lanes) in found {
                for (lane, at) in lanes {
                    timed.entry(lane).or_default().push((at, key.clone()));
                }
            }
            timed
                .into_iter()
                .map(|(lane, mut v)| {
                    v.sort();
                    (lane, v.into_iter().map(|(_, k)| k).collect())
                })
                .collect()
        };
        if sync {
            self.visitors.scanned(number, scan());
        } else {
            let tx = self.tx.clone();
            std::thread::spawn(move || {
                let _ = tx.send(Msg::Visitors(number, scan()));
            });
        }
    }

    /// Reads the PRs each session's records show it made or changed, the pages on the web it
    /// made (artifacts.rs) and the Chrome tabs it drove, subagents' logs included, in the
    /// background unless `sync`. New lines are read as they come (`check_new_lines`), and records
    /// found after this as they're found (`read_found_logs`). Again after the readers scan again,
    /// which hands no lines on.
    fn scan_pr_marks(&mut self, sync: bool) {
        let files = self.session_logs();
        self.logs_read.extend(files.iter().map(|(p, _)| p.clone()));
        if sync {
            let ((prs, pushes), made, used, profiles) = read_logs(files);
            self.pr_marks = prs;
            self.pr_pushes = pushes;
            self.artifacts = made;
            self.take_chrome_use(used, profiles);
        } else {
            self.read_logs_off_thread(files);
        }
    }

    /// Reads records the full scan didn't, as `scan_pr_marks` reads every one: a transcript a
    /// reader came on after it (a copy of a session moved here, one resumed from before the
    /// window) and its subagents' logs so far. Each is read once; what they write after comes as
    /// new lines.
    fn read_found_logs(&mut self, found: Vec<(PathBuf, String)>) {
        let files: Vec<(PathBuf, String)> = found.into_iter().filter(|(p, _)| self.logs_read.insert(p.clone())).collect();
        if !files.is_empty() {
            self.read_logs_off_thread(files);
        }
    }

    /// `read_logs` on a thread of its own, its findings sent back to be added to what's known.
    fn read_logs_off_thread(&self, files: Vec<(PathBuf, String)>) {
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let ((prs, pushes), made, used, profiles) = read_logs(files);
            let _ = tx.send(Msg::PrMarks(prs, pushes));
            let _ = tx.send(Msg::Artifacts(made));
            let _ = tx.send(Msg::ChromeUse(used, profiles));
        });
    }

    /// A PR the board doesn't list, matched to these lanes' worktrees and sessions.
    fn pr_card(&self, lanes: &[Lane], slug: &str, raw: &PrRaw, login: Option<&str>) -> PrItem {
        let links = self.pr_links();
        board::pr_card(&self.seeds, lanes, slug, self.github.repo_names(), raw, self.github.login().or(login), &links)
    }

    /// Takes in the full scan's Chrome use. A session's lines read while it ran are newer and
    /// kept, with the group and profile the scan found for it when they don't say.
    fn take_chrome_use(&mut self, found: HashMap<String, chrometabs::Use>, profiles: Vec<chrome::Profile>) {
        self.chrome_profiles = profiles;
        for (key, u) in found {
            self.chrome_lookups.insert(key.clone(), (u.group, Instant::now()));
            match self.chrome_use.get_mut(&key) {
                Some(have) if have.at > u.at => {
                    if have.group.is_none() || have.group == u.group {
                        have.group = have.group.or(u.group);
                        have.profile = have.profile.clone().or(u.profile);
                    }
                }
                _ => {
                    self.chrome_use.insert(key, u);
                }
            }
        }
        self.dirty = true;
    }

    /// Looks up which Chrome profile a session's tabs are in, off the engine's thread: when its
    /// tab group is new, or again a minute on when the extension hadn't written it down yet.
    fn look_up_chrome_profile(&mut self, key: &str) {
        let Some(u) = self.chrome_use.get(key).filter(|u| !u.is_empty()) else { return };
        if let Some((group, at)) = self.chrome_lookups.get(key) {
            if *group == u.group && (u.profile.is_some() || at.elapsed() < Duration::from_secs(60)) {
                return;
            }
        }
        self.chrome_lookups.insert(key.to_string(), (u.group, Instant::now()));
        let (u, key, tx) = (u.clone(), key.to_string(), self.tx.clone());
        std::thread::spawn(move || {
            let dir = chrometabs::Clues::read().profile_of(&u);
            let _ = tx.send(Msg::ChromeProfile(key, u.group, dir, chrome::profiles()));
        });
    }

    /// What pr-links.json and the sessions' records say of PRs, for the board.
    fn pr_links(&self) -> board::PrLinks {
        let (worked, unlinked, made) = prlinks::resolve(&self.pr_marks, &self.pr_store);
        let picked = self.pr_store.links.iter().filter(|l| l.linked && l.primary).map(|l| (prlinks::pr_key(&l.pr), l.session.clone())).collect();
        board::PrLinks {
            worked,
            unlinked,
            made,
            pushed: self.pr_pushes.clone(),
            picked,
            nudged: self.pr_store.nudges.iter().cloned().collect(),
            not_nudged: self.pr_store.nudges_off.iter().cloned().collect(),
            nudge_by_default: self.cfg.pull_requests.nudge_by_default,
            nudge_by_default_in: self.team_files.settings(&self.cfg.repos).nudges_by_default(),
        }
    }

    /// reviews.json as autoreview.rs last saved it, for the PRs' cards. One that doesn't read
    /// shows no reviews; autoreview.rs says why (store.rs `load_whole`).
    fn load_reviews(&mut self) {
        self.reviews = crate::store::read_or_default(&autoreview::path());
    }

    fn load_pr_links(&mut self) {
        if let Some(store) = read_or_warn(prlinks::load(), &mut self.pr_links_warning) {
            self.pr_store = store;
        }
    }

    /// Checks lines a session just wrote for orphan worktrees and PRs it made or changed.
    fn check_new_lines(&mut self, path: &Path, from: u64, to: u64, key: &str) {
        if to <= from {
            return;
        }
        let Some(text) = crate::util::read_range(path, from, to) else { return };
        for (pr, at) in prlinks::pr_marks(&text) {
            let have = self.pr_marks.entry(key.to_string()).or_default();
            self.dirty |= prlinks::add_mark(have, pr, at);
        }
        for (branch, at) in text.lines().filter_map(prlinks::line_push) {
            let have = self.pr_pushes.entry(key.to_string()).or_default();
            self.dirty |= prlinks::add_push(have, branch, at);
        }
        for a in artifacts::in_text(&text) {
            let have = self.artifacts.entry(key.to_string()).or_default();
            self.dirty |= artifacts::add(have, a);
        }
        let mut chrome = self.chrome_use.get(key).cloned().unwrap_or_default();
        if chrometabs::fold_text(&mut chrome, &text) {
            self.chrome_use.insert(key.to_string(), chrome);
            self.look_up_chrome_profile(key);
            self.dirty = true;
        }
        for (lane, needle, _) in &self.needles {
            if worked_in(&text, needle) && self.visitors.saw(lane, key) {
                self.dirty = true;
            }
        }
    }

    fn watch(&mut self) {
        let tx = self.tx.clone();
        let handler = move |res: notify::Result<notify::Event>| {
            if let Ok(ev) = res {
                if matches!(ev.kind, EventKind::Access(_)) {
                    return;
                }
                let paths: Vec<PathBuf> = ev.paths.into_iter().filter(|p| !is_noise(p)).collect();
                if !paths.is_empty() {
                    let _ = tx.send(Msg::Fs(paths));
                }
            }
        };
        let mut watcher = match notify::recommended_watcher(handler) {
            Ok(w) => w,
            Err(e) => {
                self.watch_warning = Some(format!("Live updates are off: {e}"));
                return;
            }
        };
        let mut paths: Vec<(PathBuf, RecursiveMode)> = Vec::new();
        self.missing_homes = self.cfg.accounts.iter().map(|a| a.home_path()).filter(|h| !h.exists()).collect();
        let _ = std::fs::create_dir_all(config::support_dir());
        paths.push((config::support_dir(), RecursiveMode::NonRecursive));
        for r in &self.readers {
            for a in harness::accounts_of(&self.cfg, r.harness()) {
                paths.extend(r.watch(&a).into_iter().map(|(p, deep)| (p, if deep { RecursiveMode::Recursive } else { RecursiveMode::NonRecursive })));
            }
        }
        let roots: Vec<PathBuf> = self.checkouts.iter().map(|(_, c)| c.root.clone()).collect();
        for r in &roots {
            paths.push((r.clone(), RecursiveMode::Recursive));
        }
        for s in &self.seeds {
            if !s.elsewhere && s.origin != Origin::Folder && !roots.iter().any(|r| s.path.starts_with(r)) {
                paths.push((s.path.clone(), RecursiveMode::Recursive));
            }
        }
        let mut failed = 0;
        for (p, mode) in paths {
            if p.exists() && watcher.watch(&p, mode).is_err() {
                failed += 1;
            }
        }
        self.watch_warning = (failed > 0).then(|| format!("Couldn't watch {failed} folders; their lanes update every 10 seconds."));
        self.watcher = Some(watcher);
    }

    fn build_sessions(&self, now: i64) -> Vec<Session> {
        let look = harness::Look { now, ..Self::look(&self.seen) };
        let mut out = Vec::new();
        // In config.toml's order of accounts, each read by its harness's reader.
        for a in &self.cfg.accounts {
            let Some(h) = harness::of(a).map(|x| x.harness()) else { continue };
            if let Some(r) = self.readers.iter().find(|r| r.harness() == h) {
                out.extend(r.sessions(a, &look));
            }
        }
        // What only the process knows: about the sessions Codealong runs, the permission
        // requests Claude Code's hook holds here, and the commands agents ask to run in your
        // terminal (terminal.rs).
        for s in &mut out {
            crate::acp::overlay(s, self.seen.seen_at(&s.key));
            crate::claude::asks::overlay(s);
            crate::terminal::overlay(s);
            let account = self.cfg.accounts.iter().find(|a| a.name == s.account);
            s.stoppable = crate::harness::stoppable(s, account);
            s.steerable = crate::harness::steerable(s, account);
        }
        out
    }

    /// state.json, read again. One that doesn't read is left alone: the engine goes on with what
    /// it has in memory, a first run's state at launch, and says so (`notices`).
    fn load_state(&mut self) {
        let read = read_or_warn(SeenState::load(), &mut self.state_warning);
        self.state_unreadable = read.is_none();
        match read {
            Some(state) => self.seen = state,
            None => {
                if self.seen.first_run == 0 {
                    self.seen = SeenState::fresh();
                }
                if let Some(why) = &mut self.state_warning {
                    why.push_str(" What you pin, archive, snooze and mark seen meanwhile lasts only until Codealong quits.");
                }
            }
        }
    }

    fn save_state(&mut self) {
        // A file that didn't read is left for you to fix, not replaced with what's in memory.
        if self.state_unreadable {
            return;
        }
        let warning = self.seen.save().err().map(|e| format!("Couldn't save state.json: {e:#}"));
        if warning != self.state_warning {
            self.state_warning = warning;
            self.dirty = true;
        }
    }

    /// The lock on todos.json that the `codealong` command takes too. Without it a change here
    /// could write over one the command is making, so the change isn't made: whoever asked for
    /// it hears why, and the board says so until the lock is had again.
    fn lock_todos(&mut self) -> std::result::Result<todos::Lock, String> {
        let lock = todos::lock().map_err(|e| format!("{e:#}"));
        let warning = lock.as_ref().err().cloned();
        if warning != self.todos_lock_warning {
            self.todos_lock_warning = warning;
            self.dirty = true;
        }
        lock
    }

    /// A change to the todos from a window or the command, and the time of the board that
    /// shows it.
    fn change_todos(&mut self, op: TodoOp) -> std::result::Result<i64, String> {
        let lanes = self.lane_index();
        // Under the lock the command takes too, and against the file as it is now, so
        // a change the command made a moment ago isn't written over.
        let lock = self.lock_todos()?;
        self.reload_todos();
        let changed = match self.todos_unreadable {
            // A change now would be lost when the file is fixed, so it's turned away.
            true => Err(self.todos_warning.clone().unwrap_or_default()),
            false => self.todos.apply(op, &lanes, now_ms()).map_err(|e| format!("{e:#}")),
        }?;
        if changed {
            self.save_todos();
            drop(lock);
            // Now rather than on the next tick: the window waits for it.
            self.publish();
        }
        Ok(self.board.read().map(|b| b.generated_at).unwrap_or(0))
    }

    /// Links a session started from todos to each of them, or notes that it's still to show up.
    fn todos_started(&mut self, ids: Vec<String>, lane: Option<String>, session: Option<String>, pending: Option<todos::PendingStart>) {
        let lanes = self.lane_index();
        let Ok(_lock) = self.lock_todos() else { return };
        self.reload_todos();
        if !self.todos_unreadable {
            let now = now_ms();
            let mut changed = false;
            for id in &ids {
                changed |= self.todos.started(id, lane.as_deref(), session.as_deref(), pending.clone(), &lanes, now);
            }
            if changed {
                self.save_todos();
            }
        }
    }

    fn load_todos(&mut self) {
        let read = read_or_warn(todos::load(), &mut self.todos_warning);
        self.todos_unreadable = read.is_none();
        if let Some((store, text)) = read {
            self.todos = store;
            self.todos_text = text;
        }
    }

    /// todos.json changed on disk. Our own writes come back through the watcher too; only
    /// someone else's edit is read again, or any change while the file didn't read.
    fn reload_todos(&mut self) {
        let text = std::fs::read_to_string(todos::path()).unwrap_or_default();
        if text == self.todos_text && !self.todos_unreadable {
            return;
        }
        self.load_todos();
        self.dirty = true;
    }

    fn save_todos(&mut self) {
        self.dirty = true;
        // A file that didn't read is left for you to fix, not replaced.
        if self.todos_unreadable {
            return;
        }
        match todos::save(&self.todos) {
            Ok(text) => {
                self.todos_text = text;
                self.todos_warning = None;
            }
            Err(e) => self.todos_warning = Some(format!("Couldn't save todos.json: {e:#}")),
        }
    }

    /// Each lane's project and repo, for linking todos.
    fn lane_index(&self) -> todos::LaneIndex {
        self.board.read().map(|b| todos::LaneIndex::of(&b.lanes)).unwrap_or_default()
    }

    /// Links sessions that starts in the apps were waiting for, from the board just composed.
    fn link_pending(&mut self, board: &Board) -> bool {
        if self.todos.todos.iter().all(|t| t.pending.is_empty()) {
            return false;
        }
        let sessions: Vec<todos::SessionRef> = board
            .lanes
            .iter()
            .flat_map(|l| {
                l.sessions.iter().filter(|s| s.visitor.is_none()).map(move |s| todos::SessionRef {
                    key: &s.key,
                    lane: &l.id,
                    harness: s.harness.config(),
                    account: &s.account,
                    started: s.started_at.unwrap_or(s.last_activity),
                })
            })
            .collect();
        let lanes = todos::LaneIndex::of(&board.lanes);
        let changed = self.todos.resolve_pending(&sessions, &lanes, now_ms());
        if changed {
            self.save_todos();
        }
        changed
    }

    /// Notes where sessions came from that starts from other sessions were waiting for.
    fn link_origins(&self, board: &Board) -> bool {
        if origins::current().pending.is_empty() {
            return false;
        }
        let sessions: Vec<origins::Candidate> = board
            .lanes
            .iter()
            .flat_map(|l| {
                l.sessions.iter().filter(|s| s.visitor.is_none()).map(move |s| origins::Candidate { key: &s.key, lane: &l.id, account: &s.account, started: s.started_at.unwrap_or(s.last_activity) })
            })
            .collect();
        let lanes = todos::LaneIndex::of(&board.lanes);
        origins::resolve_pending(&sessions, &lanes, now_ms())
    }

    fn compose(&self) -> Board {
        let now = now_ms();
        let apps = AppNames {
            terminal: launch::terminal_name(&self.cfg.apps.terminal),
            terminal_id: self.cfg.apps.terminal.clone(),
            editor: editors::name(&self.cfg.apps.editor),
            editor_id: self.cfg.apps.editor.clone(),
            editors: editors::installed(),
            diff_editor: editors::diff_editor(&self.cfg.apps.editor),
            new_terminal: self.cfg.apps.new_terminal,
        };
        let order: Vec<String> = self.cfg.projects.iter().map(|p| p.name.clone()).collect();
        let notices = self.notices();
        let usage = self.compose_usage(now);
        let pr_links = self.pr_links();
        let repo_settings = self.team_files.settings(&self.cfg.repos);
        let mut board = board::compose(board::Inputs {
            now,
            apps: &apps,
            seeds: &self.seeds,
            sessions: self.with_connections(self.with_limits(self.with_snoozes(self.with_suggestions(self.with_insights(self.with_emblems(self.with_artifacts(self.with_chrome(self.with_run_origins(self.with_origins(self.build_sessions(now)))))), now)), now), &usage, now)),
            git: &self.git,
            prs: self.github.prs(),
            requested: self.github.requested(),
            project_order: &order,
            notices,
            loading: self.loading,
            github_at: self.github.at(),
            visitors: &self.visitors.by_lane,
            pinned: &self.seen.pinned,
            github_login: self.github.login(),
            pr_links: &pr_links,
            previews: &self.previews,
            reusable: &self.reusable,
            review_snoozes: &self.seen.review_snoozes,
            auto_reviews: (&self.reviews, &self.cfg.reviews, &repo_settings),
        });
        board.usage = usage;
        board.training = self.training.list(&self.cfg.accounts);
        board.github_polled = self.github.polled();
        board.github_current = self.github.current();
        board.when_limited = self.cfg.limits.when_hit;
        board.nudge_about = self.cfg.pull_requests.nudge_about;
        board.reviews = self.cfg.reviews.clone();
        board.repos = self.cfg.repos.clone();
        board.repo_settings = repo_settings;
        board.team_files = self.team_files.files();
        for f in &mut board.team_files {
            let Some(team) = &f.settings else { continue };
            let mine = board.repo_settings.mine_of(&f.repo);
            let reviews = config::reviews_for(&self.cfg.reviews, Some(team), mine);
            let prs = config::pull_requests_for(&self.cfg.pull_requests, Some(team), mine);
            f.offers = config::suggestions_left(&team.suggests, &reviews, &prs);
        }

        board.pinned_worktrees = self.seen.pinned_worktrees.clone();
        board.pinned_prs = self.seen.pinned_prs.clone();
        board.theme = self.cfg.appearance.theme;
        board.zoom = self.cfg.appearance.zoom;
        board.quick_shortcut = self.cfg.shortcuts.quick_panel.clone();
        board.waiting_shortcut = self.cfg.shortcuts.waiting.clone();
        board.improve_shortcut = self.cfg.shortcuts.improve.clone();
        // Codealong's own source's worktrees, and where a session that improves it goes.
        let sources: HashSet<PathBuf> = self.codealong_sources.iter().map(|(root, _)| root.clone()).collect();
        crate::improve::mark(&mut board.lanes, &self.lane_checkout, &sources);
        board.improve = crate::improve::target(&board.lanes, &self.codealong_sources);
        board.jev = self.cfg.jev.clone();
        board.devices = self.cfg.devices.clone();
        board.project_rules = self.cfg.projects.clone();
        for lane in &mut board.lanes {
            lane.repo_slug_now = lane.repo_slug.as_deref().and_then(|slug| self.github.renamed(slug).cloned());
        }
        board.perf_log = perf::enabled();
        board.todos = self.todos.todos.clone();
        board.todos_unreadable = self.todos_unreadable;
        board.labels = self.todos.labels.clone();
        if self.cfg.jev.todos {
            board.todo_hints = self.jev_todos.hints(&board, &self.todos.todos);
        }
        board
    }

    fn compose_usage(&self, now: i64) -> Vec<Usage> {
        self.cfg
            .accounts
            .iter()
            .map(|a| {
                let adapter = harness::of(a);
                if let Some(why) = adapter.and_then(|h| h.no_limits(a)) {
                    return usage::none(a, why);
                }
                // What comes with the sessions read anyway, else the last background read.
                let read = adapter.and_then(|h| self.readers.iter().find(|r| r.harness() == h.harness())).and_then(|r| r.usage(a, now));
                read.unwrap_or_else(|| self.usage.get(&a.name).cloned().unwrap_or_else(|| usage::loading(a)))
            })
            // A read made before the star changed still carries the old one.
            .zip(&self.cfg.accounts)
            .map(|(u, a)| Usage { preferred: a.preferred, ..u })
            .collect()
    }

    fn publish(&mut self) {
        let t = Instant::now();
        self.store_warnings = crate::store::problems(&config::support_dir());
        let mut board = self.compose();
        if self.link_pending(&board) {
            board.todos = self.todos.todos.clone();
        }
        // Rare, so the board is composed again rather than patched.
        if self.link_origins(&board) {
            board = self.compose();
        }
        // The board already leaves them out.
        self.forget_stale_suggestions(&board);
        self.forget_stale_tags(&board);
        self.jev_look(&board);
        let composed = t.elapsed();
        self.dirty = false;
        self.dirty_git = false;
        let since_last = self.last_publish.map(|t| t.elapsed());
        self.last_publish = Some(Instant::now());
        let hash = board_hash(&mut board);
        let hashed = t.elapsed() - composed;
        if hash == self.last_hash {
            perf::log(&format!("publish skipped, nothing changed: compose {} ms, hash {} ms", composed.as_millis(), hashed.as_millis()));
            return;
        }
        self.last_hash = hash;
        // The board `get_board` reads is this one before any window hears of it: a window that
        // reads the board as this push comes gets this board or a later one, never the one
        // before it, which nothing would correct while the board stays the same. It goes out
        // from the slot, under a read lock: only this thread writes there, and readers don't
        // wait on each other.
        match self.board.write() {
            Ok(mut slot) => *slot = board,
            Err(poisoned) => *poisoned.into_inner() = board,
        }
        let board = self.board.read().unwrap_or_else(|poisoned| poisoned.into_inner());
        let t = Instant::now();
        (self.emit)(&board);
        let emitted = t.elapsed();
        if perf::enabled() {
            let line = self.perf.publish_line(&board, composed, hashed, emitted, since_last, self.git_queue.running());
            perf::log(&line);
        }
    }
}

/// Which sessions from other lanes worked in each worktree that no session runs in, by what
/// their logs show: every log is scanned in the background when the worktrees to look for
/// change, and the lines sessions write are checked as they come (`Engine::check_new_lines`).
#[derive(Default)]
struct Visitors {
    /// Lane path → sessions from other lanes whose logs show they worked there.
    by_lane: HashMap<PathBuf, Vec<String>>,
    /// The number of the latest scan begun. A scan looks for the worktrees there were when it
    /// began, so one that comes back after a later one began is dropped.
    scan: u64,
    /// What new lines have shown since that scan began, until it's back: it may have read a
    /// log before they were written, so they're added to what it found.
    since: Option<HashMap<PathBuf, Vec<String>>>,
}

impl Visitors {
    /// A scan begins, for other worktrees than the last looked for. Its number comes back with
    /// what it finds (`scanned`).
    fn begin(&mut self) -> u64 {
        self.scan += 1;
        self.since = Some(HashMap::new());
        self.scan
    }

    /// There's nothing to look for any more: a scan still out looked for worktrees that were.
    fn clear(&mut self) {
        self.scan += 1;
        self.since = None;
        self.by_lane.clear();
    }

    /// A session's new lines show it worked in a lane. True when that's news.
    fn saw(&mut self, lane: &Path, key: &str) -> bool {
        let add = |keys: &mut Vec<String>| {
            let new = !keys.iter().any(|k| k == key);
            if new {
                keys.push(key.to_string());
            }
            new
        };
        if let Some(since) = &mut self.since {
            add(since.entry(lane.to_path_buf()).or_default());
        }
        add(self.by_lane.entry(lane.to_path_buf()).or_default())
    }

    /// A scan is back with what it found, the earliest work in each lane first. What new lines
    /// showed while it ran comes after. False when a later scan has begun, or there's nothing
    /// to look for any more: this one changes nothing.
    fn scanned(&mut self, scan: u64, mut found: HashMap<PathBuf, Vec<String>>) -> bool {
        if scan != self.scan {
            return false;
        }
        for (lane, keys) in self.since.take().unwrap_or_default() {
            let have = found.entry(lane).or_default();
            for key in keys {
                if !have.contains(&key) {
                    have.push(key);
                }
            }
        }
        self.by_lane = found;
        true
    }
}

/// What one of the stores' files said, or none, with why as its warning for the board
/// (`Engine::notices`): what was read before stays as it is until the file reads again.
fn read_or_warn<T>(read: Result<T>, warning: &mut Option<String>) -> Option<T> {
    *warning = read.as_ref().err().map(|e| format!("{e:#}"));
    read.ok()
}

/// Says which folders the scan has been waiting a while to open: most likely macOS is asking
/// whether Codealong may, and nobody has answered yet.
fn held_warning(paths: &[PathBuf]) -> Option<String> {
    let first = tilde(paths.first()?);
    let (what, them) = match paths.len() {
        1 => (first, "it"),
        2 => (format!("{first} and 1 more folder"), "them"),
        n => (format!("{first} and {} more folders", n - 1), "them"),
    };
    Some(format!("Waiting to open {what}. If macOS is asking whether Codealong may access {them}, answer on this Mac."))
}

/// Which lanes a file shows work in, each with when that work first appears.
fn file_mentions(path: &Path, needles: &[(PathBuf, String)]) -> Vec<(PathBuf, i64)> {
    use std::io::Read;
    let Ok(mut f) = std::fs::File::open(path) else { return vec![] };
    let fallback = mtime_ms(path).unwrap_or(0);
    let overlap = needles.iter().map(|(_, n)| n.len()).max().unwrap_or(0);
    let mut hits: Vec<(PathBuf, i64)> = Vec::new();
    let mut carry: Vec<u8> = Vec::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = match f.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        carry.extend_from_slice(&buf[..n]);
        let text = String::from_utf8_lossy(&carry);
        for (lane, needle) in needles {
            if !hits.iter().any(|(l, _)| l == lane) {
                if let Some(line) = work_line(&text, needle) {
                    hits.push((lane.clone(), line_time(line).unwrap_or(fallback)));
                }
            }
        }
        if hits.len() == needles.len() {
            break;
        }
        let keep = carry.len().saturating_sub(overlap);
        carry.drain(..keep);
    }
    hits
}

/// True when some line in `text` shows work in the folder `needle` names: creating it with
/// `git worktree add`, or running commands in it (a `workdir`/`cwd` field, or `cd` into it).
/// A passing mention, like a search or a listing, doesn't count.
pub(crate) fn worked_in(text: &str, needle: &str) -> bool {
    work_line(text, needle).is_some()
}

/// What marks a log line as a tool's output, and what comes right before a folder a command ran
/// in: every harness's own (`Adapter::output_marks`, `Adapter::folder_marks`), and `cd` and `git
/// worktree add`, which say it whatever the harness. Gathered once.
fn record_marks() -> &'static (Vec<&'static str>, Vec<&'static str>) {
    static MARKS: std::sync::OnceLock<(Vec<&'static str>, Vec<&'static str>)> = std::sync::OnceLock::new();
    MARKS.get_or_init(|| {
        let mut output: Vec<&'static str> = harness::all().flat_map(|a| a.output_marks().iter().copied()).collect();
        let mut folder: Vec<&'static str> = harness::all().flat_map(|a| a.folder_marks().iter().copied()).chain(["cd ", "worktree add "]).collect();
        // Two harnesses can share one (`"cwd":"`): a line needn't be looked at twice for it.
        for marks in [&mut output, &mut folder] {
            let mut seen = HashSet::new();
            marks.retain(|m| seen.insert(*m));
        }
        (output, folder)
    })
}

/// The first line in `text` that shows work in the folder `needle` names.
fn work_line<'a>(text: &'a str, needle: &str) -> Option<&'a str> {
    let (tool_output, markers) = record_marks();
    let mut from = 0;
    while let Some(i) = text[from..].find(needle) {
        let at = from + i;
        let line_start = text[..at].rfind('\n').map(|j| j + 1).unwrap_or(0);
        let line_end = text[at..].find('\n').map(|j| at + j).unwrap_or(text.len());
        let line = &text[line_start..line_end];
        // A command's output can name any folder: a listing, another session's log.
        if tool_output.iter().any(|t| line.contains(t)) {
            from = at + needle.len();
            continue;
        }
        for &marker in markers {
            let mut m = 0;
            while let Some(k) = line[m..].find(marker) {
                let path_start = m + k + marker.len();
                let path_end = |c: &char| !matches!(c, '"' | ' ' | '\\' | '&' | ';' | '\'' | '|');
                let path: String = line[path_start..].chars().take_while(path_end).collect();
                let path = path.trim_start_matches("-b ").to_string();
                if names_folder(&path, needle) {
                    return Some(line);
                }
                // `git worktree add -b <branch> <path>`: the path comes after the branch.
                if marker == "worktree add " && line[path_start..].split_whitespace().take(4).any(|w| names_folder(&w.chars().take_while(path_end).collect::<String>(), needle)) {
                    return Some(line);
                }
                m = path_start;
            }
        }
        from = at + needle.len();
    }
    None
}

/// True when `path` is the folder `needle` names or a path inside it, so `codealong-jev`
/// doesn't match `codealong-jevtodos`.
fn names_folder(path: &str, needle: &str) -> bool {
    path.match_indices(needle).any(|(i, _)| {
        let before = path[..i].chars().next_back();
        let after = path[i + needle.len()..].chars().next();
        matches!(before, None | Some('/')) && matches!(after, None | Some('/'))
    })
}

/// The `timestamp` of a log line, if it has one.
fn line_time(line: &str) -> Option<i64> {
    let start = line.find("\"timestamp\":\"")? + "\"timestamp\":\"".len();
    let end = line[start..].find('"')? + start;
    crate::util::parse_rfc3339_ms(&line[start..end])
}

/// The PRs sessions' records show they made or changed and the branches they pushed to, the
/// pages on the web they made and the Chrome tabs they drove, by session key, from one read of
/// each record (`file_marks`), with Chrome's profiles.
fn read_logs(files: Vec<(PathBuf, String)>) -> ((prlinks::Marks, prlinks::Pushes), artifacts::Made, HashMap<String, chrometabs::Use>, Vec<chrome::Profile>) {
    let mut out: prlinks::Marks = HashMap::new();
    let mut pushed: prlinks::Pushes = HashMap::new();
    let mut made: artifacts::Made = HashMap::new();
    let mut used: HashMap<String, chrometabs::Use> = HashMap::new();
    for (key, marks, pushes, pages, chrome) in parallel_map(files, |(path, key)| {
        let (marks, pushes, pages, chrome) = file_marks(&path);
        (!marks.is_empty() || !pushes.is_empty() || !pages.is_empty() || !chrome.is_empty()).then(|| (key, marks, pushes, pages, chrome))
    }) {
        let have = out.entry(key.clone()).or_default();
        for (pr, at) in marks {
            prlinks::add_mark(have, pr, at);
        }
        if !pushes.is_empty() {
            let have = pushed.entry(key.clone()).or_default();
            for (branch, at) in pushes {
                prlinks::add_push(have, branch, at);
            }
        }
        if !pages.is_empty() {
            let have = made.entry(key.clone()).or_default();
            for a in pages {
                artifacts::add(have, a);
            }
        }
        // A session's own log and its subagents' each have their own: the latest counts.
        if !chrome.is_empty() && used.get(&key).map_or(true, |u| chrome.at > u.at) {
            used.insert(key, chrome);
        }
    }
    out.retain(|_, prs| !prs.is_empty());
    // Which profile each session's tabs are in, from one read of the extension's storage.
    if !used.is_empty() {
        let mut clues = chrometabs::Clues::read();
        for u in used.values_mut() {
            u.profile = clues.profile_of(u);
        }
    }
    ((out, pushed), made, used, chrome::profiles())
}

/// A log's PR marks and pushes, the pages on the web it made (in the order its lines name them)
/// and the Chrome tabs it drove, from one read of it.
fn file_marks(path: &Path) -> (Vec<(String, i64)>, Vec<(String, i64)>, Vec<crate::model::Artifact>, chrometabs::Use) {
    let needles: Vec<&str> = prlinks::needles().chain(artifacts::needles()).chain(chrometabs::NEEDLES.iter().copied()).collect();
    let mut marks: Vec<(String, i64)> = Vec::new();
    let mut pushes = Vec::new();
    let mut made = Vec::new();
    let mut chrome = chrometabs::Use::default();
    crate::util::each_line_with(path, &needles, |line| {
        if let Some(m) = prlinks::line_mark(line) {
            if !marks.iter().any(|(k, _)| k == &m.0) {
                marks.push(m);
            }
        }
        pushes.extend(prlinks::line_push(line));
        made.extend(artifacts::line_artifact(line));
        chrometabs::fold(&mut chrome, line);
    });
    (marks, pushes, made, chrome)
}

/// A hash of everything the UI shows except the timestamp, so identical boards aren't resent.
/// The board is hashed as it's written out, with its timestamp set aside meanwhile: it's some
/// 450 KB, and neither a copy of it nor its text is made for this.
fn board_hash(board: &mut Board) -> u64 {
    use std::hash::Hasher;
    struct Hashed(std::collections::hash_map::DefaultHasher);
    impl std::io::Write for Hashed {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.write(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    // Buffered, so the hasher is handed a few kilobytes at a time, not each comma and quote.
    let mut out = std::io::BufWriter::new(Hashed(std::collections::hash_map::DefaultHasher::new()));
    let at = std::mem::take(&mut board.generated_at);
    // What's read of the repos' team files isn't sent, but what acts on the board waits on it.
    let written = serde_json::to_writer(&mut out, board).and_then(|()| serde_json::to_writer(&mut out, &board.repo_settings.readiness()));
    board.generated_at = at;
    // A board that can't be written out hashes as nothing at all, as it did.
    match (written, out.into_inner()) {
        (Ok(()), Ok(hashed)) => hashed.0.finish(),
        _ => std::collections::hash_map::DefaultHasher::new().finish(),
    }
}

/// How often the harness databases are looked at for changes (`glance_readers`).
const DATABASES_EVERY: Duration = Duration::from_millis(1500);

/// Paths that never matter, dropped before they reach the engine: databases and logs that
/// apps write constantly, dependency folders, git's object store.
fn is_noise(p: &Path) -> bool {
    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or_default();
    // A harness's database runs in WAL mode, so while a session runs only its `-wal` file
    // changes: that change is how Codealong hears of it (`Adapter::databases`).
    if harness::all().flat_map(|a| a.databases()).any(|db| name.strip_suffix("-wal") == Some(*db)) {
        return false;
    }
    if name.ends_with("-wal") || name.ends_with("-shm") || name.ends_with("-journal") || name.ends_with(".sqlite") || name.ends_with(".log") || name.ends_with(".lock") || name == ".DS_Store" {
        return true;
    }
    p.components().any(|c| {
        let c = c.as_os_str();
        c == "node_modules" || c == "objects" || c == "target" || c == ".nx" || c == ".turbo" || c == ".next"
    })
}

fn folder_seed(path: PathBuf, project: &str, name: &str, elsewhere: bool) -> LaneSeed {
    LaneSeed {
        path,
        project: project.to_string(),
        repo: name.to_string(),
        repo_slug: None,
        head_owners: Vec::new(),
        callsign: name.to_string(),
        origin: if elsewhere { Origin::Other } else { Origin::Folder },
        branch: None,
        head: None,
        default_branch: None,
        elsewhere,
        chats: false,
        starts_chats: false,
        exact: false,
    }
}

/// The folders apps keep their chats in, a folder for each, and the name of the lane that holds
/// them: the Claude app's Code sessions started without a folder, the Codex app's threads that
/// belong to no project, and the sessions Codealong starts in no project (`config::chats_dir`).
fn chat_roots() -> Vec<(PathBuf, &'static str)> {
    let mut roots = harness::chat_roots();
    roots.push((config::chats_dir(), CHATS));
    roots
}

/// The lane of the sessions Codealong starts in no project.
const CHATS: &str = "Chats";

/// Which of `roots` holds `path`.
fn chat_root(roots: &[(PathBuf, &'static str)], path: &Path) -> Option<usize> {
    roots.iter().position(|(root, _)| path.starts_with(root))
}

/// What a folder a session ran in turned out to be, for `stray_seeds`.
enum Found {
    /// In a repo: the root of its worktree.
    Repo(PathBuf),
    Plain,
    /// Its read is waiting on macOS.
    Held,
    Gone,
}

/// Lanes for the folders sessions ran in that no lane covers, so no session goes unlisted: one
/// for all of an app's chats (a chat whose folder the app has removed too), one for each repo
/// outside your projects, and one for each other folder, which takes in only the sessions
/// started there. A folder that's gone, or whose read is held, has none. Codealong's own chats
/// (`own`, among `chats`) have a lane with no sessions too, to start in.
fn stray_seeds(seeds: &[LaneSeed], cwds: &BTreeSet<String>, chats: &[(PathBuf, &'static str)], own: &Path, home: &Path, found: impl Fn(&Path) -> Found) -> Vec<LaneSeed> {
    let mut roots: BTreeSet<usize> = chats.iter().position(|(root, _)| root == own && !seeds.iter().any(|s| s.path == *root)).into_iter().collect();
    let mut repos = BTreeSet::new();
    let mut folders = BTreeSet::new();
    let index = board::SeedIndex::new(seeds);
    for cwd in cwds {
        let p = Path::new(cwd);
        if index.best(p).is_some() {
            continue;
        }
        if let Some(i) = chat_root(chats, p) {
            roots.insert(i);
            continue;
        }
        match found(p) {
            Found::Repo(wt) => {
                repos.insert(wt);
            }
            Found::Plain => {
                folders.insert(p.to_path_buf());
            }
            Found::Held | Found::Gone => {}
        }
    }
    let name = |p: &Path| if p == home { "home".to_string() } else { file_name(p) };
    let mut out = Vec::new();
    for i in roots {
        let (root, lane) = &chats[i];
        out.push(LaneSeed { origin: Origin::Folder, chats: true, starts_chats: root == own, ..folder_seed(root.clone(), ELSEWHERE, lane, true) });
    }
    for root in repos {
        out.push(folder_seed(root.clone(), ELSEWHERE, &name(&root), true));
    }
    for folder in folders {
        out.push(LaneSeed { origin: Origin::Folder, exact: true, ..folder_seed(folder.clone(), ELSEWHERE, &name(&folder), true) });
    }
    out
}

/// What the repo scan reads inside a worktree.
struct WorktreeFiles {
    /// Its git folder in the main checkout, for telling which worktree a change there is for.
    /// None for a main checkout.
    gitdir: Option<PathBuf>,
    ignore: Option<Gitignore>,
}

fn read_worktree(path: &Path) -> WorktreeFiles {
    // A linked worktree's `.git` is a file that names its git folder; a main checkout's is that folder.
    // Made canonical, as the watcher's paths are, so a change under it is matched to its lane.
    let gitdir = if path.join(".git").is_file() { crate::repos::linked_gitdir(path).map(|d| std::fs::canonicalize(&d).unwrap_or(d)) } else { None };
    let mut b = GitignoreBuilder::new(path);
    b.add(path.join(".gitignore"));
    WorktreeFiles { gitdir, ignore: b.build().ok() }
}

/// A worktree to run git in, and the branch its changes are counted from.
type GitJob = (PathBuf, Option<String>);

fn spawn_git_workers(back: Sender<Msg>) -> Sender<GitJob> {
    let (jobs, rx) = channel::<GitJob>();
    let rx = Arc::new(Mutex::new(rx));
    for i in 0..GIT_WORKERS {
        let rx = rx.clone();
        let back = back.clone();
        let _ = std::thread::Builder::new().name(format!("codealong-git-{i}")).spawn(move || {
            procs::utility_thread();
            let status = |path: &Path, base: Option<&str>| {
                let t = Instant::now();
                let info = git::status(path, base);
                if perf::enabled() {
                    perf::log(&format!("git status {} ms: {}", t.elapsed().as_millis(), path.display()));
                }
                info
            };
            git_worker(&rx, status, |path, info| back.send(Msg::GitDone(path, info)).is_ok());
        });
    }
    jobs
}

/// One git worker: runs the jobs as they come, until there are no more to come or nobody to
/// answer. Every job is answered, with none when its run panicked, and the worker goes on to
/// the next: the engine counts a worktree as running until it hears (throttle.rs), so one it
/// never heard of would get no git again, and a worker that died would take a quarter of the
/// git with it.
fn git_worker(jobs: &Mutex<Receiver<GitJob>>, run: impl Fn(&Path, Option<&str>) -> GitInfo, answer: impl Fn(PathBuf, Option<GitInfo>) -> bool) {
    loop {
        let job = match jobs.lock() {
            Ok(r) => r.recv(),
            Err(_) => return,
        };
        let Ok((path, base)) = job else { return };
        let info = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(&path, base.as_deref()))).ok();
        if !answer(path, info) {
            return;
        }
    }
}

/// First run: the accounts in use or signed in, then a project per GitHub owner your agents
/// worked in.
fn first_run_config() -> Config {
    let accounts = detect::first_run_accounts();
    let since = now_ms() - 30 * DAY_MS;
    let mut cwds: Vec<String> = Vec::new();
    for a in &accounts {
        if let Some(h) = harness::of(a) {
            cwds.extend(h.session_cwds(&a.home_path(), since));
        }
    }
    // A folder macOS holds for a privacy prompt is left out rather than waited on (held.rs).
    let located = Held::new("session folder", repos::locate, || {}).read(cwds.iter().map(PathBuf::from), HELD_WAIT);
    let mut per_repo: BTreeMap<PathBuf, usize> = BTreeMap::new();
    for cwd in &cwds {
        if let Some(Some((_, main))) = located.get(Path::new(cwd)) {
            *per_repo.entry(main.clone()).or_default() += 1;
        }
    }
    per_repo.retain(|_, n| *n >= 2);
    let mut loaded = Held::new("checkout", repos::load_checkout, || {}).read(per_repo.keys().cloned(), HELD_WAIT);
    let mut owners: BTreeMap<String, usize> = BTreeMap::new();
    for (root, n) in per_repo {
        if let Some((owner, _)) = loaded.remove(&root).flatten().and_then(|c| c.github) {
            *owners.entry(owner).or_default() += n;
        }
    }
    Config { accounts, projects: config::suggest_projects(&owners, github::display_name), ..Config::default() }
}

#[cfg(test)]
mod tests {
    /// Jev judges the risk of a command an agent asks to run in your terminal from all of it, not
    /// the first line the session's excerpt has.
    #[test]
    fn jev_reads_a_command_for_your_terminal_whole() {
        use super::*;
        let key = "codex:codex:engine-terminal-risk";
        let asked = crate::terminal::Asked { id: "run-engine-risk".into(), ask: crate::terminal::Ask { session: key.into(), command: "echo tidying up\nrm -rf ~/projects".into(), cwd: "/tmp".into() }, at: 1 };
        let _held = crate::terminal::held_for_tests(asked);
        let words = Engine::waiting_call(&crate::fixtures::session(key), None).unwrap();
        assert!(words.starts_with("Run in your terminal") && words.ends_with("echo tidying up\nrm -rf ~/projects"), "{words}");
        crate::terminal::deny(key, "run-engine-risk", None);
    }

    #[test]
    fn a_harness_database_log_is_heard_and_other_logs_are_not() {
        use super::*;
        assert!(!is_noise(Path::new("/Users/me/.local/share/opencode/opencode.db-wal")));
        assert!(!is_noise(Path::new("/Users/me/.gemini/antigravity-cli/conversation_summaries.db-wal")));
        assert!(is_noise(Path::new("/Users/me/.local/share/opencode/opencode.db-shm")));
        assert!(is_noise(Path::new("/Users/me/projects/app/data/cache.db-wal")));
    }

    #[test]
    fn every_session_folder_that_can_be_shown_gets_a_lane() {
        use super::*;
        let home = PathBuf::from("/Users/me");
        let scratch = "/Users/me/Library/Application Support/Claude/scratch-workspaces";
        let own = PathBuf::from("/Users/me/Library/Application Support/Codealong/chats");
        let chats = vec![(PathBuf::from(scratch), "Claude app chats"), (PathBuf::from("/Users/me/Documents/Codex"), "Codex chats"), (own.clone(), CHATS)];
        let seeds = vec![folder_seed(PathBuf::from("/Users/me/projects/app"), "App", "app", false)];
        let cwds: BTreeSet<String> = [
            "/Users/me/projects/app/src".to_string(),
            format!("{scratch}/a/b/scratch-2026-09-28-045217"),
            format!("{scratch}/a/b/scratch-2026-09-26-c8bd6b"),
            "/Users/me/Documents/Codex/2026-09-27/can-you".to_string(),
            "/Users/me/projects/other/web".to_string(),
            "/Users/me".to_string(),
            "/Users/me/Downloads".to_string(),
            "/Users/me/Documents/held".to_string(),
            "/tmp/gone".to_string(),
        ]
        .into_iter()
        .collect();
        // Chats aren't looked up, so one whose folder the app removed still has its lane.
        let found = |p: &Path| match p.to_str().unwrap() {
            "/Users/me/projects/other/web" => Found::Repo(PathBuf::from("/Users/me/projects/other")),
            "/Users/me" | "/Users/me/Downloads" => Found::Plain,
            "/Users/me/Documents/held" => Found::Held,
            "/tmp/gone" => Found::Gone,
            p => panic!("{p} was looked up"),
        };
        let mut all = seeds.clone();
        all.extend(stray_seeds(&seeds, &cwds, &chats, &own, &home, found));
        let lane = |cwd: &str| board::best_seed(&all, Path::new(cwd)).map(|i| (all[i].callsign.as_str(), all[i].chats, all[i].exact));
        // Codealong's own chats have a lane before any session has started there, to start in.
        let mine: Vec<&LaneSeed> = all.iter().filter(|s| s.starts_chats).collect();
        assert_eq!(mine.len(), 1);
        assert_eq!((mine[0].path.as_path(), mine[0].callsign.as_str(), mine[0].chats, mine[0].elsewhere), (own.as_path(), CHATS, true, true));
        assert_eq!(lane(&format!("{}/2026-10-05-a1b2c3", own.display())), Some((CHATS, true, false)));
        assert_eq!(lane("/Users/me/projects/app/src"), Some(("app", false, false)));
        assert_eq!(lane(&format!("{scratch}/a/b/scratch-2026-09-28-045217")), Some(("Claude app chats", true, false)));
        assert_eq!(lane(&format!("{scratch}/a/b/scratch-2026-09-26-c8bd6b")), Some(("Claude app chats", true, false)));
        assert_eq!(lane("/Users/me/Documents/Codex/2026-09-27/can-you"), Some(("Codex chats", true, false)));
        assert_eq!(lane("/Users/me/projects/other/web"), Some(("other", false, false)));
        assert_eq!(lane("/Users/me"), Some(("home", false, true)));
        assert_eq!(lane("/Users/me/Downloads"), Some(("Downloads", false, true)));
        assert_eq!(lane("/Users/me/Desktop"), None, "your home folder's lane doesn't take in the folders in it");
        assert_eq!((lane("/Users/me/Documents/held"), lane("/tmp/gone")), (None, None));
        assert_eq!(all.len(), 7, "one lane for each app's chats, and Codealong's");
        assert!(all[1..].iter().all(|s| s.elsewhere && s.project == ELSEWHERE));
    }

    #[test]
    fn opens_folders_in_your_editor_or_one_installed() {
        use super::*;
        let apps = AppNames { editor_id: "cursor".into(), editors: vec![config::Profile { id: "zed".into(), name: "Zed".into() }], ..Default::default() };
        assert!(opens_in(&apps, "cursor"));
        assert!(opens_in(&apps, "zed"));
        assert!(!opens_in(&apps, "Terminal"));
    }

    /// A batch keeps what it learned of each folder, and still says what the gitignore says of
    /// each path in it, whichever paths came first: rules for files, for folders only, and ones
    /// that take a path back.
    #[test]
    fn a_batch_ignores_what_the_gitignore_ignores() {
        use super::*;
        let dir = crate::fixtures::scratch("batch-ignores");
        let root = dir.join("repo");
        for dir in ["dist/assets", "build", "src/build", "logs/keep", "src/deep/er"] {
            std::fs::create_dir_all(root.join(dir)).unwrap();
        }
        let mut gi = GitignoreBuilder::new(&root);
        for rule in ["dist", "build/", "*.log", "!keep.log", "logs/*", "!logs/keep", "/src/deep/er/*.tmp"] {
            gi.add_line(None, rule).unwrap();
        }
        let gi = gi.build().unwrap();
        let paths: Vec<PathBuf> = [
            "dist/app.js",
            "dist/assets/a.css",
            "dist",
            "build",
            "build/out.o",
            "src/build",
            "src/build/x",
            "src/main.rs",
            "src/app.log",
            "src/keep.log",
            "logs/today",
            "logs/keep",
            "logs/keep/a.log",
            "logs/keep/keep.log",
            "logs/keep/notes.txt",
            "src/deep/er/x.tmp",
            "src/deep/er/x.rs",
            "src/deep/x.tmp",
        ]
        .iter()
        .map(|p| root.join(p))
        .chain([root.clone()])
        .collect();
        let want: Vec<bool> = paths.iter().map(|p| gi.matched_path_or_any_parents(p, p.is_dir()).is_ignore()).collect();
        for order in [paths.clone(), paths.iter().rev().cloned().collect()] {
            let mut batch = FsBatch::new(&[]);
            let got: HashMap<&PathBuf, bool> = order.iter().map(|p| (p, batch.ignored(&gi, &root, p))).collect();
            for (p, want) in paths.iter().zip(&want) {
                assert_eq!(got[p], *want, "{}", p.display());
            }
        }
        assert!(want.iter().any(|&w| w) && want.iter().any(|&w| !w));
    }

    /// A batch finds each path's lane once per folder, and the lane is the one `best_seed` finds,
    /// worktrees inside worktrees and the worktrees' own folders included.
    #[test]
    fn a_batch_finds_the_lane_best_seed_finds() {
        use super::*;
        let seed = |path: &str, exact: bool| LaneSeed { exact, ..folder_seed(PathBuf::from(path), "P", "repo", false) };
        let seeds = vec![seed("/r/app", false), seed("/r/app/.claude/worktrees/fix", false), seed("/Users/me", true), seed("/r/app/.claude/worktrees/fix", false)];
        let paths = [
            "/r/app/src/a.rs",
            "/r/app/.claude/worktrees/fix",
            "/r/app/.claude/worktrees/fix/src/a.rs",
            "/r/app/.claude/worktrees/fixed/src/a.rs",
            "/r/app/.claude/worktrees",
            "/r/app/src/b.rs",
            "/Users/me",
            "/Users/me/notes.md",
            "/r/apple/x",
            "/",
        ];
        let mut batch = FsBatch::new(&seeds);
        for p in paths.iter().chain(paths.iter().rev()) {
            assert_eq!(batch.lane_of(Path::new(p)), board::best_seed(&seeds, Path::new(p)), "{p}");
        }
    }

    #[test]
    fn a_removed_worktree_rescans_the_repos() {
        use super::*;

        let dir = crate::fixtures::scratch("gone-worktree");
        let root = dir.join("repo");
        let git_dir = root.join(".git");
        let admin = git_dir.join("worktrees/topic");
        let wt = dir.join("repo-topic");
        std::fs::create_dir_all(&admin).unwrap();
        std::fs::create_dir_all(&wt).unwrap();
        let (tx, _rx) = channel();
        let mut engine = Engine::new(Box::new(|_| {}), tx, Arc::new(RwLock::new(Board::default())));
        let checkout = repos::Checkout { root: root.clone(), git_dir: git_dir.clone(), name: "repo".into(), identity: None, github: None, github_owners: Vec::new() };
        engine.checkouts = vec![("P".into(), checkout)];
        engine.gitdir_to_lane.insert(admin.clone(), wt.clone());
        let seed = |path: &Path, origin: Origin| LaneSeed { origin, ..folder_seed(path.to_path_buf(), "P", "repo", false) };
        engine.seeds = vec![seed(&root, Origin::Main), seed(&wt, Origin::Other)];
        let is_lane = |e: RepoEvent, lane: &Path| matches!(e, RepoEvent::Lane(l) if l == lane);

        // While it's there, a change in it or its folder in `.git/worktrees` runs git in it.
        assert!(is_lane(engine.classify_repo_path(&wt.join("src/main.rs"), &mut FsBatch::new(&engine.seeds)), &wt));
        assert!(is_lane(engine.classify_repo_path(&admin.join("HEAD"), &mut FsBatch::new(&engine.seeds)), &wt));
        // `git worktree remove` deletes both.
        std::fs::remove_dir_all(&wt).unwrap();
        assert!(matches!(engine.classify_repo_path(&wt.join("src/main.rs"), &mut FsBatch::new(&engine.seeds)), RepoEvent::Worktrees), "its own files going");
        assert!(matches!(engine.classify_repo_path(&admin.join("HEAD"), &mut FsBatch::new(&engine.seeds)), RepoEvent::Worktrees), "its worktree gone");
        std::fs::create_dir_all(&wt).unwrap();
        std::fs::remove_dir_all(&admin).unwrap();
        assert!(matches!(engine.classify_repo_path(&admin, &mut FsBatch::new(&engine.seeds)), RepoEvent::Worktrees), "its folder in .git gone");
        // The main checkout's own changes are still its own.
        assert!(is_lane(engine.classify_repo_path(&root.join("src/lib.rs"), &mut FsBatch::new(&engine.seeds)), &root));
    }

    /// GitHub's notice is one of the board's, in the place it always had: after config.toml's.
    #[test]
    fn what_went_wrong_with_github_is_a_notice() {
        use super::*;

        let (tx, _rx) = channel();
        let mut engine = Engine::new(Box::new(|_| {}), tx, Arc::new(RwLock::new(Board::default())));
        engine.config_warning = Some("config.toml doesn't read.".into());
        engine.watch_warning = Some("Couldn't watch 2 folders; their lanes update every 10 seconds.".into());
        let keys = |e: &Engine| e.notices().into_iter().map(|n| n.key).collect::<Vec<_>>();
        assert_eq!(keys(&engine), ["config", "watch"]);

        engine.on_github(ghpoll::Event::Glance(Err(github::Failure::RateLimited { resets: None })));
        assert!(engine.dirty, "the board says so");
        assert_eq!(keys(&engine), ["config", "github", "watch"]);
        let said = engine.notices().remove(1);
        assert_eq!((said.title.as_str(), said.until), ("GitHub rate limit reached", None));
        assert_eq!(said.text, "GitHub: your account's rate limit is used up, so PRs wait until it resets. `gh` and your agents draw on the same limit.");
        assert!(engine.compose().notices.contains(&said));

        // The next glance that's answered ends it.
        engine.on_github(ghpoll::Event::Glance(Ok(github::Glance::default())));
        assert_eq!(keys(&engine), ["config", "watch"]);
    }

    /// A file noted in the background (where a session came from, a side conversation) has
    /// nobody waiting to hear that it couldn't be: the board says so.
    #[test]
    fn a_background_change_that_could_not_be_made_is_a_notice() {
        use super::*;

        let (tx, _rx) = channel();
        let mut engine = Engine::new(Box::new(|_| {}), tx, Arc::new(RwLock::new(Board::default())));
        assert!(engine.notices().is_empty());
        engine.store_warnings = vec![("origins.json".into(), "origins.json doesn't read as a list of where sessions came from.".into())];
        let said = engine.notices();
        assert_eq!(said.len(), 1);
        assert_eq!((said[0].key.as_str(), said[0].title.as_str(), said[0].until), ("origins.json", "origins.json has a problem", None));
        assert_eq!(said[0].text, "origins.json doesn't read as a list of where sessions came from.");
    }

    #[test]
    fn worktrees_carry_what_github_calls_a_renamed_repo_now() {
        use super::*;

        let (tx, _rx) = channel();
        let mut engine = Engine::new(Box::new(|_| {}), tx, Arc::new(RwLock::new(Board::default())));
        let seed = |path: &str, slug: &str| LaneSeed { repo_slug: Some(slug.into()), origin: Origin::Main, ..folder_seed(PathBuf::from(path), "P", "repo", false) };
        engine.seeds = vec![seed("/x/spark", "SparkHQ/spark"), seed("/x/spur", "spurhq/spur")];
        engine.on_github(ghpoll::Event::RepoNames(vec![("sparkhq/spark".into(), Some("sprucehq/spark".into())), ("spurhq/spur".into(), Some("spurhq/spur".into()))]));
        assert!(engine.dirty, "a rename changes the board");
        let board = engine.compose();
        let now = |path: &str| board.lanes.iter().find(|l| l.path == path).map(|l| l.repo_slug_now.clone());
        assert_eq!(now("/x/spark"), Some(Some("sprucehq/spark".to_string())));
        assert_eq!(now("/x/spur"), Some(None), "a repo that kept its name says nothing more");

        engine.dirty = false;
        engine.on_github(ghpoll::Event::RepoNames(vec![("spurhq/spur".into(), Some("spurhq/spur".into()))]));
        assert!(!engine.dirty, "a repo that kept its name changes nothing");
    }

    /// The first scan reads config.toml before the watcher starts, and can take minutes, so an
    /// edit made meanwhile is read once the watcher is up. A scan reads and writes under the
    /// home folder (config.toml, state.json), so it runs in a copy of this test binary with an
    /// empty home of its own.
    #[test]
    fn a_config_edit_during_the_first_scan_is_read() {
        use super::*;
        use std::sync::atomic::{AtomicBool, Ordering};

        let Some(own) = std::env::var_os("CODEALONG_TEST_HOME") else {
            let home = std::env::temp_dir().join(format!("codealong-first-scan-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&home);
            std::fs::create_dir_all(&home).unwrap();
            let run = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["engine::tests::a_config_edit_during_the_first_scan_is_read", "--exact", "--nocapture"])
                .env("HOME", &home)
                .env("CODEALONG_TEST_HOME", &home)
                .output()
                .unwrap();
            let ran = home.join("ran").exists();
            std::fs::remove_dir_all(&home).unwrap();
            assert!(run.status.success() && ran, "{}{}", String::from_utf8_lossy(&run.stdout), String::from_utf8_lossy(&run.stderr));
            return;
        };
        assert_eq!(home(), PathBuf::from(own), "never the real home folder");

        // No accounts and nothing to scan, so the scan stays in this home.
        let off = Config { scan: vec![], ..Config::default() };
        config::write(&off).unwrap();
        let on = config::render(&Config { devices: crate::config::DevicesCfg { enabled: true, ..Default::default() }, ..off });
        // The scan's first board goes out before the watcher starts: devices are turned on then.
        let edited = AtomicBool::new(false);
        let emit = move |_: &Board| {
            if !edited.swap(true, Ordering::SeqCst) {
                std::fs::write(config::config_path(), &on).unwrap();
            }
        };
        let (tx, _rx) = channel();
        let mut engine = Engine::new(Box::new(emit), tx, Arc::new(RwLock::new(Board::default())));
        engine.full_scan();
        assert!(engine.cfg.devices.enabled);
        assert!(engine.compose().devices.enabled);
        std::fs::write(home().join("ran"), "").unwrap();
    }

    /// state.json holds what you pinned, archived, snoozed and saw. One that doesn't read, as
    /// when an older build meets a newer one's shape, is never written over: the engine runs on a
    /// first run's state in memory, says so, and takes the file up again once it reads. A save
    /// that fails is said too. Runs in a copy of this test binary with a home of its own, as a
    /// scan reads and writes there.
    #[test]
    fn a_state_file_that_does_not_read_is_never_written_over() {
        use super::*;
        use std::os::unix::fs::PermissionsExt;

        let Some(own) = std::env::var_os("CODEALONG_TEST_HOME") else {
            let home = std::env::temp_dir().join(format!("codealong-state-scan-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&home);
            std::fs::create_dir_all(&home).unwrap();
            let run = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["engine::tests::a_state_file_that_does_not_read_is_never_written_over", "--exact", "--nocapture"])
                .env("HOME", &home)
                .env("CODEALONG_TEST_HOME", &home)
                .output()
                .unwrap();
            let ran = home.join("ran").exists();
            std::fs::remove_dir_all(&home).unwrap();
            assert!(run.status.success() && ran, "{}{}", String::from_utf8_lossy(&run.stdout), String::from_utf8_lossy(&run.stderr));
            return;
        };
        assert_eq!(home(), PathBuf::from(own), "never the real home folder");

        // No accounts and nothing to scan, so the scan stays in this home.
        config::write(&Config { scan: vec![], ..Config::default() }).unwrap();
        let path = crate::state::path();
        let said = |e: &Engine| e.notices().into_iter().find(|n| n.key == "state").map(|n| n.text);
        let engine = || {
            let (tx, _rx) = channel();
            Engine::new(Box::new(|_| {}), tx, Arc::new(RwLock::new(Board::default())))
        };

        // A snooze as a later build might write it.
        let newer = r#"{"firstRun":5,"pinned":["claude:claude:a"],"snoozed":[{"key":"claude:claude:a","at":1,"until":"tomorrow"}]}"#;
        std::fs::write(&path, newer).unwrap();
        let mut e = engine();
        e.full_scan();
        assert!(said(&e).is_some_and(|t| t.starts_with("state.json doesn't read as what you've seen, pinned and snoozed")), "{:?}", said(&e));
        assert!(e.compose().notices.iter().any(|n| n.key == "state" && n.title == "state.json has a problem"));
        assert!(e.seen.first_run > 5 && e.seen.pinned.is_empty(), "a first run's state, in memory");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), newer, "the scan's own save left it alone");
        // What's done meanwhile stays in memory.
        e.seen.set_pinned(vec!["codex:codex:b".into()]);
        e.save_state();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), newer);
        assert_eq!(e.seen.pinned, ["codex:codex:b"]);

        // Fixed by hand: the file is the state again, and is saved to from then on.
        std::fs::write(&path, r#"{"firstRun":5,"pinned":["claude:claude:a"]}"#).unwrap();
        e.apply_fs(vec![path.clone()]);
        assert_eq!(said(&e), None);
        assert_eq!((e.seen.first_run, e.seen.pinned.clone()), (5, vec!["claude:claude:a".to_string()]));
        e.seen.set_pinned_prs(vec!["o/r#12".into()]);
        e.save_state();
        let kept = std::fs::read_to_string(&path).unwrap();
        assert!(kept.contains("claude:claude:a") && kept.contains("o/r#12"), "{kept}");

        // A save that can't land is said, until one does.
        let dir = config::support_dir();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555)).unwrap();
        e.seen.set_pinned_prs(vec![]);
        e.save_state();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(said(&e).is_some_and(|t| t.starts_with("Couldn't save state.json: Couldn't write ")), "{:?}", said(&e));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), kept);
        e.save_state();
        assert_eq!(said(&e), None);
        assert!(!std::fs::read_to_string(&path).unwrap().contains("o/r#12"));

        // Deleted while it didn't read: what's in memory is the state from then on.
        std::fs::write(&path, "{").unwrap();
        let mut e = engine();
        e.load_config();
        e.load_state();
        assert!(said(&e).is_some());
        e.seen.set_pinned(vec!["codex:codex:c".into()]);
        e.save_state();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{");
        std::fs::remove_file(&path).unwrap();
        e.apply_fs(vec![path.clone()]);
        assert_eq!(said(&e), None);
        assert!(std::fs::read_to_string(&path).unwrap().contains("codex:codex:c"));
        std::fs::write(home().join("ran"), "").unwrap();
    }

    /// The app and the `codealong` command change todos.json under one lock. When the lock can't
    /// be had, a change isn't made without it, where it could write over the command's: it's
    /// turned away with the reason, and the board says so until the lock is had again. Runs in a
    /// copy of this test binary with a home of its own.
    #[test]
    fn a_todo_change_without_the_lock_is_turned_away() {
        use super::*;

        let Some(own) = std::env::var_os("CODEALONG_TEST_HOME") else {
            let home = std::env::temp_dir().join(format!("codealong-todos-lock-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&home);
            std::fs::create_dir_all(&home).unwrap();
            let run = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["engine::tests::a_todo_change_without_the_lock_is_turned_away", "--exact", "--nocapture"])
                .env("HOME", &home)
                .env("CODEALONG_TEST_HOME", &home)
                .output()
                .unwrap();
            let ran = home.join("ran").exists();
            std::fs::remove_dir_all(&home).unwrap();
            assert!(run.status.success() && ran, "{}{}", String::from_utf8_lossy(&run.stdout), String::from_utf8_lossy(&run.stderr));
            return;
        };
        assert_eq!(home(), PathBuf::from(own), "never the real home folder");

        config::write(&Config { scan: vec![], ..Config::default() }).unwrap();
        let (tx, _rx) = channel();
        let mut e = Engine::new(Box::new(|_| {}), tx, Arc::new(RwLock::new(Board::default())));
        e.load_config();
        e.load_todos();
        let said = |e: &Engine| e.notices().into_iter().find(|n| n.key == "todos-lock").map(|n| n.text);
        let labels = |name: &str| TodoOp::Labels { labels: vec![todos::Label { id: "l1".into(), name: name.into(), color: "blue".into() }] };
        e.change_todos(labels("first")).unwrap();
        let before = std::fs::read_to_string(todos::path()).unwrap();
        assert!(before.contains("first") && said(&e).is_none());

        // The lock's file can't be opened: a folder is in its place.
        let lock = todos::path().with_extension("json.lock");
        std::fs::remove_file(&lock).unwrap();
        std::fs::create_dir(&lock).unwrap();
        let why = e.change_todos(labels("second")).unwrap_err();
        assert!(why.starts_with("Couldn't lock todos.json: "), "{why}");
        assert_eq!(said(&e), Some(why));
        e.todos_started(vec!["t1".into()], None, Some("claude:claude:a".into()), None);
        assert_eq!(std::fs::read_to_string(todos::path()).unwrap(), before);
        assert_eq!(e.todos.labels[0].name, "first");

        // With the lock back, changes go through and the notice goes.
        std::fs::remove_dir(&lock).unwrap();
        e.change_todos(labels("second")).unwrap();
        assert!(std::fs::read_to_string(todos::path()).unwrap().contains("second"));
        assert_eq!(said(&e), None);
        std::fs::write(home().join("ran"), "").unwrap();
    }

    /// macOS holds `open()` in a folder it protects until someone answers its privacy prompt,
    /// which on a Mac nobody sits at can be hours. A worktree whose files never read doesn't hold
    /// up the first scan: the board goes out with it as `git worktree list` gives it, git runs
    /// everywhere else, and a worktree whose read lands later is filled in. Runs in a copy of
    /// this test binary with a home of its own, as the scan reads and writes there.
    #[test]
    fn the_first_scan_publishes_while_a_worktree_read_is_held() {
        use super::*;

        let Some(own) = std::env::var_os("CODEALONG_TEST_HOME") else {
            let home = std::env::temp_dir().join(format!("codealong-held-scan-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&home);
            std::fs::create_dir_all(&home).unwrap();
            // As git reports paths: /private/var, not /var.
            let home = std::fs::canonicalize(&home).unwrap();
            let run = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["engine::tests::the_first_scan_publishes_while_a_worktree_read_is_held", "--exact", "--nocapture"])
                .env("HOME", &home)
                .env("CODEALONG_TEST_HOME", &home)
                .output()
                .unwrap();
            let ran = home.join("ran").exists();
            std::fs::remove_dir_all(&home).unwrap();
            assert!(run.status.success() && ran, "{}{}", String::from_utf8_lossy(&run.stdout), String::from_utf8_lossy(&run.stderr));
            return;
        };
        assert_eq!(home(), PathBuf::from(own), "never the real home folder");
        // Fails rather than hangs when the scan waits on the read that never returns.
        std::thread::spawn(|| {
            std::thread::sleep(Duration::from_secs(60));
            eprintln!("The scan waited on a read that never returns.");
            std::process::exit(1);
        });

        let git = |dir: &Path, args: &[&str]| {
            let out = std::process::Command::new("git").arg("-C").arg(dir).args(["-c", "user.name=t", "-c", "user.email=t@t"]).args(args).output().unwrap();
            assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
        };
        let projects = home().join("projects");
        let main = projects.join("repo");
        std::fs::create_dir_all(&main).unwrap();
        git(&main, &["init", "-q", "-b", "main"]);
        git(&main, &["remote", "add", "origin", "git@example.com:team/repo.git"]);
        std::fs::write(main.join(".gitignore"), "build/\n").unwrap();
        git(&main, &["add", "."]);
        git(&main, &["commit", "-qm", "init"]);
        let path = |name: &str| projects.join(format!("repo-{name}"));
        for name in ["stuck", "late", "plain"] {
            git(&main, &["worktree", "add", "-q", &path(name).to_string_lossy(), "-b", name]);
        }
        let project = crate::config::ProjectCfg { name: "Team".into(), repos: vec!["example.com/team/*".into()], folders: vec![], chrome_profile: None };
        config::write(&Config { scan: vec!["~/projects".into()], projects: vec![project], ..Config::default() }).unwrap();

        let boards: Arc<Mutex<Vec<Board>>> = Arc::default();
        let emitted = boards.clone();
        let (tx, rx) = channel();
        let mut engine = Engine::new(Box::new(move |b: &Board| emitted.lock().unwrap().push(b.clone())), tx.clone(), Arc::new(RwLock::new(Board::default())));
        // The stuck worktree's files never read; the late one's read once it's let go.
        let (release, gate) = channel::<()>();
        let gate = Mutex::new(gate);
        engine.reads.worktrees = Held::new(
            "worktree",
            move |p: &Path| {
                if p.ends_with("repo-stuck") {
                    loop {
                        std::thread::park();
                    }
                }
                if p.ends_with("repo-late") {
                    let _ = gate.lock().unwrap().recv();
                }
                read_worktree(p)
            },
            move || {
                let _ = tx.send(Msg::Landed);
            },
        );
        let started = Instant::now();
        engine.full_scan();
        assert!(started.elapsed() < Duration::from_secs(30), "the scan waited on the held reads");

        let first = boards.lock().unwrap().first().cloned().expect("the first scan published a board");
        assert!(!first.loading);
        let branch = |b: &Board, p: &Path| b.lanes.iter().find(|l| Path::new(&l.path) == p).map(|l| l.branch.clone());
        assert_eq!(branch(&first, &main), Some(Some("main".to_string())));
        for name in ["stuck", "late", "plain"] {
            assert_eq!(branch(&first, &path(name)), Some(Some(name.to_string())), "{name} is on the board as git worktree list gives it");
        }
        let targets = engine.git_targets();
        assert!(targets.contains(&main) && targets.contains(&path("plain")));
        assert!(!targets.contains(&path("stuck")) && !targets.contains(&path("late")), "no git where the files are held");
        assert!(engine.ignores.contains_key(&path("plain")) && !engine.ignores.contains_key(&path("late")));

        // The late read lands while the engine runs as the app's does: the next board has git there.
        std::thread::spawn(move || engine.run(rx));
        release.send(()).unwrap();
        let has_git = |b: &Board, p: &Path| b.lanes.iter().any(|l| Path::new(&l.path) == p && l.git.is_some());
        let until = Instant::now() + Duration::from_secs(20);
        let filled = loop {
            if let Some(b) = boards.lock().unwrap().iter().find(|b| has_git(b, &path("late"))) {
                break b.clone();
            }
            assert!(Instant::now() < until, "the late worktree never got git");
            std::thread::sleep(Duration::from_millis(50));
        };
        assert!(has_git(&filled, &path("plain")) && !has_git(&filled, &path("stuck")));
        std::fs::write(home().join("ran"), "").unwrap();
    }

    /// The logs are scanned on a thread of its own, which takes a while, and sessions go on
    /// writing meanwhile: the scan's answer doesn't take away what their new lines showed, and
    /// an older scan that comes back late doesn't replace a newer one's.
    #[test]
    fn a_visitors_scan_keeps_what_new_lines_showed_and_an_older_one_is_dropped() {
        use super::*;
        let (a, b) = (PathBuf::from("/tmp/spark-a"), PathBuf::from("/tmp/spark-b"));
        let found = |lane: &PathBuf, keys: &[&str]| HashMap::from([(lane.clone(), keys.iter().map(|k| k.to_string()).collect::<Vec<_>>())]);
        let mut v = Visitors::default();

        // While a scan runs, a session's new lines show it worked in a worktree.
        let first = v.begin();
        assert!(v.saw(&a, "s2"));
        assert!(!v.saw(&a, "s2"), "said once");
        assert!(v.saw(&a, "s1"));
        // The scan read s2's log before those lines were written. What it found comes first,
        // the earliest work in the lane, and what the new lines showed is kept after it.
        assert!(v.scanned(first, found(&a, &["s0", "s1"])));
        assert_eq!(v.by_lane[&a], ["s0", "s1", "s2"]);
        // Lines that come once it's back add to it as before.
        assert!(v.saw(&a, "s3") && !v.saw(&a, "s0"));
        assert_eq!(v.by_lane[&a], ["s0", "s1", "s2", "s3"]);

        // The worktrees to look for change, and again before the scan for the first change is
        // back: that one looked for worktrees that aren't the ones looked for now.
        let second = v.begin();
        let third = v.begin();
        assert!(v.saw(&b, "s4"));
        assert!(!v.scanned(second, found(&a, &["stale"])), "the older scan changes nothing");
        assert_eq!((v.by_lane[&a].len(), v.by_lane[&b].len()), (4, 1));
        assert!(v.scanned(third, found(&b, &["s5"])));
        assert_eq!(v.by_lane.get(&a), None, "what the latest scan didn't look for is gone");
        assert_eq!(v.by_lane[&b], ["s5", "s4"]);

        // Nothing to look for any more: a scan still out is dropped when it's back.
        let fourth = v.begin();
        v.clear();
        assert!(!v.scanned(fourth, found(&a, &["late"])));
        assert!(v.by_lane.is_empty());
    }

    #[test]
    fn boards_that_differ_only_in_their_time_hash_alike() {
        use super::*;
        let board = |at: i64, loading: bool| Board { generated_at: at, loading, quick_shortcut: "Alt+Cmd+/".into(), ..Board::default() };
        let (mut first, mut later, mut other) = (board(1_000, false), board(2_000, false), board(1_000, true));
        assert_eq!(board_hash(&mut first), board_hash(&mut later));
        assert_ne!(board_hash(&mut first), board_hash(&mut other), "a real difference");
        later.notices.push(Notice { key: "github".into(), title: "GitHub didn't answer".into(), text: "GitHub: couldn't load PRs".into(), until: None });
        assert_ne!(board_hash(&mut first), board_hash(&mut later));
        // The board goes out with the time it was made, and hashes the same each time.
        assert_eq!((first.generated_at, later.generated_at), (1_000, 2_000));
        assert_eq!(board_hash(&mut first), board_hash(&mut first.clone()));
    }

    /// A repo's team file read without a file changes nothing the windows get, but a review that
    /// waited on it goes on only once the board with it is published (autoreview.rs reads the
    /// published board), so it counts as a change.
    #[test]
    fn a_team_file_read_without_a_file_is_a_change_to_publish() {
        use super::*;
        let mut before = Board { repo_settings: crate::teamfile::RepoSettings { unread: true, ..Default::default() }, ..Board::default() };
        let mut started = Board { repo_settings: crate::teamfile::RepoSettings { here: ["o/r".to_string()].into(), ..Default::default() }, ..Board::default() };
        let mut read = started.clone();
        read.repo_settings.read.insert(("o/r".into(), "main".into()));
        assert_ne!(board_hash(&mut before), board_hash(&mut started), "the first read was asked for");
        assert_ne!(board_hash(&mut started), board_hash(&mut read), "a branch was read");
        assert_eq!(board_hash(&mut read), board_hash(&mut read.clone()));
    }

    #[test]
    fn the_board_says_which_folders_it_waits_to_open() {
        use super::*;
        assert_eq!(held_warning(&[]), None);
        assert_eq!(
            held_warning(&[PathBuf::from("/Volumes/T7/wt")]).as_deref(),
            Some("Waiting to open /Volumes/T7/wt. If macOS is asking whether Codealong may access it, answer on this Mac.")
        );
        let three = held_warning(&["/Volumes/T7/a", "/Volumes/T7/b", "/Volumes/T7/c"].map(PathBuf::from)).unwrap();
        assert!(three.starts_with("Waiting to open /Volumes/T7/a and 2 more folders. "), "{three}");
    }

    fn usage(error: Option<&str>, needs_sign_in: bool, percent: f64) -> Usage {
        Usage {
            account: "claude".into(),
            harness: Harness::Claude,
            plan: Some("Max 20x".into()),
            meters: if error.is_some() {
                vec![]
            } else {
                vec![Meter { key: "week".into(), label: "Week".into(), window: "Weekly".into(), percent, resets_at: None, severity: Severity::Normal, model: None }]
            },
            as_of: if error.is_some() { None } else { Some(1) },
            error: error.map(str::to_string),
            needs_sign_in,
            loading: false,
            no_limits: false,
            preferred: false,
            rate_limited: false,
        }
    }

    /// Usage is read on a thread nobody watches, and isn't read again while a read is on its
    /// way: one that fell over has to be over too.
    #[test]
    fn a_usage_read_that_falls_over_is_over() {
        use super::*;
        let (tx, _rx) = channel();
        let mut engine = Engine::new(Box::new(|_| {}), tx, Arc::new(RwLock::new(Board::default())));
        engine.usage.insert("claude".into(), usage(None, false, 47.0));
        (engine.usage_inflight, engine.usage_backoff) = (true, true);
        engine.on_usage(None);
        assert!(!engine.usage_inflight, "the next read can go");
        assert!(engine.usage_backoff && !engine.dirty, "nothing was read: nothing else changes");
        assert_eq!(engine.usage["claude"].meters[0].percent, 47.0);

        engine.usage_inflight = true;
        engine.on_usage(Some(vec![usage(None, false, 50.0)]));
        assert!(!engine.usage_inflight && !engine.usage_backoff && engine.dirty);
        assert_eq!(engine.usage["claude"].meters[0].percent, 50.0);
    }

    /// Git isn't run in a worktree while a run there is out, so every run has to answer.
    #[test]
    fn a_git_run_that_falls_over_still_answers() {
        use super::*;
        let (jobs, rx) = channel::<GitJob>();
        let (answers, answered) = channel();
        for name in ["first", "bad", "next"] {
            jobs.send((PathBuf::from(name), None)).unwrap();
        }
        drop(jobs);
        let run = |path: &Path, _: Option<&str>| {
            if path == Path::new("bad") {
                // A panic, without the hook's lines on stderr.
                std::panic::resume_unwind(Box::new("git fell over"));
            }
            GitInfo::default()
        };
        // The worker returns once the jobs run out: it went on past the one that fell over.
        git_worker(&Mutex::new(rx), run, |path, info| answers.send((path, info.is_some())).is_ok());
        let all: Vec<(PathBuf, bool)> = answered.try_iter().collect();
        assert_eq!(all, [("first".into(), true), ("bad".into(), false), ("next".into(), true)]);

        // The engine has the worktree's run as over either way, and keeps what it knew of it.
        let (tx, _rx) = channel();
        let mut engine = Engine::new(Box::new(|_| {}), tx, Arc::new(RwLock::new(Board::default())));
        let wt = PathBuf::from("/x/wt");
        engine.seeds = vec![folder_seed(wt.clone(), "P", "repo", false)];
        assert_eq!(engine.git_queue.request(wt.clone(), Instant::now()), Verdict::Run);
        engine.on_git(wt.clone(), None);
        assert_eq!(engine.git_queue.running(), 0);
        assert!(!engine.git.contains_key(&wt) && !engine.dirty_git);
        engine.on_git(wt.clone(), Some(GitInfo::default()));
        assert!(engine.git.contains_key(&wt) && engine.dirty_git);

        // With no worker to take a job, nothing is left running either.
        let (gone, _) = channel::<GitJob>();
        engine.git_jobs = gone;
        engine.git_queue = GitQueue::default();
        engine.queue_git(wt.clone());
        assert_eq!(engine.git_queue.running(), 0);
    }

    #[test]
    fn a_failed_usage_read_keeps_the_last_numbers() {
        let good = usage(None, false, 47.0);
        let kept = keep_last_good(Some(&good), usage(Some("Couldn't read usage: HTTP 500"), false, 0.0));
        assert_eq!(kept.meters, good.meters);
        assert_eq!(kept.as_of, Some(1));
        assert!(kept.error.is_some());
        // Signing out isn't papered over.
        let signed_out = keep_last_good(Some(&good), usage(Some("Sign-in expired"), true, 0.0));
        assert!(signed_out.meters.is_empty());
    }

    use super::{keep_last_good, worked_in};
    use crate::model::{Harness, Meter, Severity, Usage};

    #[test]
    fn counts_real_work_not_mentions() {
        let n = "spark-validate-ghostscript-20260923";
        assert!(worked_in(r#"{"cmd":"git worktree add /tmp/spark-validate-ghostscript-20260923 origin/main"}"#, n));
        assert!(worked_in(r#"{"cmd":"git worktree add -b codex/x /tmp/spark-validate-ghostscript-20260923"}"#, n));
        assert!(worked_in(r#"{"arguments":"{\"cmd\":\"yarn test\",\"workdir\":\"/private/tmp/spark-validate-ghostscript-20260923\"}"}"#, n));
        assert!(worked_in(r#"{"command":"cd /private/tmp/spark-validate-ghostscript-20260923 && git status"}"#, n));
        assert!(!worked_in(r#"{"command":"cd ~/projects/codealong && grep -F spark-validate-ghostscript-20260923 logs"}"#, n));
        assert!(!worked_in(r#"{"content":"/private/tmp/spark-validate-ghostscript-20260923  aad4df0 [codex/x]"}"#, n));
        assert_eq!(super::line_time(r#"{"timestamp":"1970-01-01T00:00:01.000Z","cmd":"x"}"#), Some(1000));
    }

    /// The marks the harnesses hand the engine are the ones it looked for when it knew them
    /// itself: Claude Code's tool results and line folders, Codex's call outputs, workdirs and
    /// folders, and the shell's own.
    #[test]
    fn looks_for_every_harnesss_marks_and_the_shells() {
        let (output, folder) = super::record_marks();
        let sorted = |v: &[&'static str]| {
            let mut v = v.to_vec();
            v.sort();
            v
        };
        assert_eq!(sorted(output), sorted(&["\"type\":\"tool_result\"", "\"type\":\"function_call_output\"", "\"type\":\"custom_tool_call_output\""]));
        assert_eq!(sorted(folder), sorted(&["\"workdir\":\"", "\\\"workdir\\\":\\\"", "\"cwd\":\"", "\\\"cwd\\\":\\\"", "cd ", "worktree add "]));
    }

    #[test]
    fn a_worktree_next_to_the_checkout_counts_only_by_its_whole_name() {
        let n = "spark-org-redis-keys";
        assert!(worked_in(r#"{"command":"cd /Users/me/projects/spark && git worktree add -q -b me/org-redis-keys ../spark-org-redis-keys origin/main"}"#, n));
        assert!(worked_in(r#"{"command":"cd /Users/me/projects/spark-org-redis-keys/apps/server && npx vitest run"}"#, n));
        assert!(!worked_in(r#"{"command":"cd /Users/me/projects/spark-org-redis-keys-v2 && git status"}"#, n));
        assert!(!worked_in(r#"{"command":"cd /Users/me/projects/old-spark-org-redis-keys && git status"}"#, n));
        assert!(!worked_in(r#"{"type":"user","message":{"content":[{"type":"tool_result","content":"Bash cd /Users/me/projects/spark-org-redis-keys && ls"}]}}"#, n));
    }

    /// What the engine's thread spends on this Mac's real data: composing the board, hashing it,
    /// and a big batch from the file watcher. `CODEALONG_BENCH_FS` names folders whose files go in
    /// that batch (as many as there are, up to 30,000), as a build or an install writes them.
    /// `CODEALONG_BENCH_RUNS` says how many times each is timed (20), more for a profile.
    ///
    ///   cd src-tauri && CODEALONG_BENCH_FS=~/projects/app/dist cargo test --lib bench_engine_on_real_data -- --ignored --nocapture
    #[test]
    #[ignore = "reads this Mac's sessions and repos; run by hand"]
    fn bench_engine_on_real_data() {
        use super::*;
        let mut e = snapshot_engine(false, false);
        let ms = |t: Instant| t.elapsed().as_secs_f64() * 1000.0;
        let runs: usize = std::env::var("CODEALONG_BENCH_RUNS").ok().and_then(|n| n.parse().ok()).unwrap_or(20);
        let (mut compose, mut sessions, mut hash) = (0.0, 0.0, 0.0);
        for _ in 0..runs {
            let t = Instant::now();
            let _ = e.build_sessions(now_ms());
            sessions += ms(t);
            let t = Instant::now();
            let mut board = e.compose();
            compose += ms(t);
            let t = Instant::now();
            let _ = board_hash(&mut board);
            hash += ms(t);
        }
        let n = runs as f64;
        eprintln!("compose {:.1} ms (of which building sessions {:.1} ms), hash {:.1} ms, over {runs} runs", compose / n, sessions / n, hash / n);
        let Ok(dirs) = std::env::var("CODEALONG_BENCH_FS") else { return };
        let mut paths = Vec::new();
        for dir in std::env::split_paths(&dirs) {
            let mut stack = vec![dir];
            while let Some(d) = stack.pop() {
                let Ok(entries) = std::fs::read_dir(&d) else { continue };
                for entry in entries.flatten() {
                    let p = entry.path();
                    if entry.file_type().is_ok_and(|t| t.is_dir()) {
                        stack.push(p.clone());
                    }
                    if !is_noise(&p) && paths.len() < 30_000 {
                        paths.push(p);
                    }
                }
            }
        }
        let batches = runs.max(1);
        let t = Instant::now();
        for _ in 0..batches {
            e.apply_fs(paths.clone());
        }
        eprintln!("fs batch of {} paths {:.1} ms, over {batches} runs", paths.len(), ms(t) / batches as f64);
    }
}
