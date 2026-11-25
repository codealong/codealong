//! `Handle`: what everything else talks to the engine through. It sends the engine's thread a
//! message (`Msg`) and, where a window waits to hear, waits for the time of the board that shows
//! the change (`SHOWN_WAIT`); it reads the board the engine last published; and what needs no
//! engine (saving config.toml, a harness's own archive, the terminal that signs an account in)
//! it does on the caller's thread. A handle over a board read once (`Handle::over`) has no
//! engine behind it, for `--snapshot`'s modes.

use super::*;

/// How long a window's seen or archive waits to hear which board shows it. A window shows the
/// change meanwhile; without an answer it keeps its copy until a board shows the change.
const SHOWN_WAIT: Duration = Duration::from_secs(10);

/// A session archived or taken out of the archive (`Handle::archive`).
pub struct Archived {
    /// The time of the board that shows it, as `mark_seen_shown` returns.
    pub at: Option<i64>,
    /// Why its harness still lists it, where it's archived in Codealong only (`Adapter::archive`).
    pub kept: Option<String>,
}

/// What the UI talks to. Cheap to clone.
#[derive(Clone)]
pub struct Handle {
    pub(super) tx: Sender<Msg>,
    pub(super) board: Arc<RwLock<Board>>,
    /// The app's engine's `Engine::terminal_use`.
    pub(super) terminal_use: Option<Arc<Mutex<crate::stale::Uses>>>,
    /// In-memory test handle (`for_tests_archiving`): skips disk/config side effects in archive
    /// and hand-over so tests don't depend on this Mac's settings.
    #[cfg(test)]
    pub(super) in_memory: bool,
}

impl Handle {
    /// A handle on an empty board with no engine behind it, for tests of what goes through one.
    #[cfg(test)]
    pub fn for_tests() -> Handle {
        let (tx, _) = channel();
        Handle { tx, board: Arc::new(RwLock::new(Board::default())), terminal_use: None, in_memory: false }
    }

    /// A handle on `board` for tests that observe archives: each `Msg::SetArchived` updates
    /// `board` (marking matching sessions archived and dropping archived sessions from
    /// `PrItem::primary`, as `board/prs.rs` `pr_primary` does), records `(key, archived)` in the
    /// returned list, and replies once done. Skips `archive_in_harness` so tests don't read this
    /// Mac's `config.toml`.
    #[cfg(test)]
    pub fn for_tests_archiving(board: Board) -> (Handle, Arc<Mutex<Vec<(String, bool)>>>) {
        let (tx, rx) = channel();
        let board = Arc::new(RwLock::new(board));
        let archived: Arc<Mutex<Vec<(String, bool)>>> = Arc::new(Mutex::new(Vec::new()));
        let (b, log) = (board.clone(), archived.clone());
        std::thread::spawn(move || {
            for msg in rx {
                if let Msg::SetArchived(keys, on, _, reply) = msg {
                    if let Ok(mut guard) = b.write() {
                        for key in &keys {
                            for lane in &mut guard.lanes {
                                for s in &mut lane.sessions {
                                    if s.key == *key {
                                        s.archived = on;
                                    }
                                }
                            }
                            if on {
                                for pr in &mut guard.prs {
                                    if pr.primary.as_deref() == Some(key.as_str()) {
                                        pr.primary = None;
                                    }
                                }
                            }
                        }
                    }
                    if let Ok(mut guard) = log.lock() {
                        for key in keys {
                            guard.push((key, on));
                        }
                    }
                    let _ = reply.send(0);
                }
            }
        });
        (Handle { tx, board, terminal_use: None, in_memory: true }, archived)
    }

    #[cfg(test)]
    pub(crate) fn is_in_memory(&self) -> bool {
        self.in_memory
    }

    /// A handle on a board read once (`snapshot`, `light_snapshot`), with no engine behind it,
    /// for `--snapshot`'s modes: what goes through a handle does the same over it. A message
    /// for the engine is taken and dropped, so whoever waits for the board that shows a change
    /// hears at once that none will, as a window does from an engine too busy to say.
    pub fn over(board: Board) -> Handle {
        let (tx, rx) = channel();
        std::thread::spawn(move || for _ in rx {});
        Handle {
            tx,
            board: Arc::new(RwLock::new(board)),
            terminal_use: None,
            #[cfg(test)]
            in_memory: false,
        }
    }

    pub fn board(&self) -> Board {
        self.board.read().map(|b| b.clone()).unwrap_or_default()
    }

    /// When each terminal tab was last used, as the engine's looks saw it, for Close stale
    /// terminals (`stale::Uses`); with no engine behind the handle, as the app last wrote it.
    /// The file is up to a few minutes behind the engine.
    pub fn terminal_use(&self) -> crate::stale::Uses {
        match &self.terminal_use {
            Some(uses) => uses.lock().map(|u| u.clone()).unwrap_or_default(),
            None => crate::stale::Uses::load(),
        }
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

    /// What the start made `at` waited on in its terminal was just answered: its terminal is read
    /// again at the next look, for what it shows next.
    pub fn start_answered(&self, at: i64) {
        let _ = self.tx.send(Msg::StartAnswered(at));
    }

    /// A PR you've just merged shows merged now, not at the next poll. Returns the time of the
    /// board that shows it, as `approved` does.
    pub fn merged(&self, slug: &str, number: u64) -> Option<i64> {
        let (tx, rx) = channel();
        self.tx.send(Msg::Merged(slug.to_string(), number, tx)).ok()?;
        rx.recv_timeout(SHOWN_WAIT).ok()
    }

    /// A PR you've just reviewed stops asking for your review now, not at the next poll, and
    /// shows your review: `verdict`, of `commit` where that's a draft's, else of the PR's head.
    /// Returns the time of the board that shows it, as `mark_seen_shown` does.
    pub fn approved(&self, slug: &str, number: u64, verdict: github::Verdict, commit: Option<String>) -> Option<i64> {
        let (tx, rx) = channel();
        self.tx.send(Msg::Approved(slug.to_string(), number, verdict, commit, tx)).ok()?;
        rx.recv_timeout(SHOWN_WAIT).ok()
    }

    pub fn mark_seen(&self, keys: Vec<String>) {
        let _ = self.tx.send(Msg::MarkSeen(keys, None, None));
    }

    /// Marks sessions seen as of `at`, however long the engine takes to get to it: what came
    /// after `at` (a side conversation's answer) still waits on you.
    pub fn mark_seen_at(&self, keys: Vec<String>, at: i64) {
        let _ = self.tx.send(Msg::MarkSeen(keys, Some(at), None));
    }

    /// Marks sessions seen and returns the time of the board that shows it, so a window knows
    /// when its own copy of the change can go; None when the engine is too busy to say.
    pub fn mark_seen_shown(&self, keys: Vec<String>) -> Option<i64> {
        let (tx, rx) = channel();
        self.tx.send(Msg::MarkSeen(keys, None, Some(tx))).ok()?;
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

    /// Marks the turn a session is taking seen, as its own agent asked (`board::apply_agent_seen`).
    /// Returns the time of the board that shows it, as `mark_seen_shown` does.
    pub fn agent_seen(&self, key: &str, note: Option<String>) -> Option<i64> {
        let (tx, rx) = channel();
        self.tx.send(Msg::AgentSeen(key.to_string(), AgentSeen { at: 0, note }, tx)).ok()?;
        rx.recv_timeout(SHOWN_WAIT).ok()
    }

    /// Snoozes sessions until `until`, or wakes them with none. Returns the time of the board
    /// that shows it, as `mark_seen_shown` does.
    pub fn snooze(&self, keys: Vec<String>, until: Option<i64>) -> Option<i64> {
        let (tx, rx) = channel();
        self.tx.send(Msg::Snooze(keys, until, tx)).ok()?;
        rx.recv_timeout(SHOWN_WAIT).ok()
    }

    /// Sets sessions that need you aside until their side conversations answer
    /// (`board::apply_aside`). Returns the time of the board that shows it, as `mark_seen_shown`
    /// does.
    pub fn aside(&self, keys: Vec<String>) -> Option<i64> {
        let (tx, rx) = channel();
        self.tx.send(Msg::Aside(keys, tx)).ok()?;
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

    /// Puts labels on sessions, by name or id, and takes others off (`board::apply_labels`): a
    /// name that's no label yet is made one, as `todo update --add-label` makes it. Returns the
    /// time of the board that shows it, as `quiet` does, or why it couldn't be done.
    pub fn label(&self, keys: Vec<String>, add: Vec<String>, remove: Vec<String>) -> std::result::Result<Option<i64>, String> {
        let (tx, rx) = channel();
        self.tx.send(Msg::Label(keys, add, remove, tx)).map_err(|_| "Codealong's engine stopped.".to_string())?;
        rx.recv_timeout(SHOWN_WAIT).map_or(Ok(None), |r| r.map(Some))
    }

    /// Moves sessions to the top of what waits on you, or back to their places with `raised`
    /// false (`board::apply_raise`), as `by` says who and why. Returns the time of the board that
    /// shows it, as `quiet` does, or why an agent may not (`board::raise_refused`).
    pub fn raise(&self, keys: Vec<String>, raised: bool, by: Raised) -> Result<Option<i64>, String> {
        let (tx, rx) = channel();
        if self.tx.send(Msg::Raise(keys, raised, by, tx)).is_err() {
            return Ok(None);
        }
        rx.recv_timeout(SHOWN_WAIT).ok().transpose()
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

    /// Notes that you sent the session `key` a message (`yours`), or that an agent or a review's
    /// next round did: a helper you're talking to is yours until then (`Helper::yours`). Any
    /// session is noted, as one is a helper only by what the board reads of it.
    pub fn talking(&self, key: &str, yours: bool) {
        let _ = self.tx.send(Msg::Talking(key.to_string(), yours));
    }

    /// The setup wizard was finished or skipped: it shows again only when asked for. Whether it
    /// shows by itself is read from state.json (`setup::needed`).
    pub fn finish_setup(&self) {
        let _ = self.tx.send(Msg::SetupDone);
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
    /// (claude/asks.rs); or a session was switched to another mode (cycle.rs); or a session began
    /// waiting on a command in a terminal (terminal.rs).
    pub fn hosted_changed(&self) {
        let _ = self.tx.send(Msg::Hosted);
    }

    /// What the board lists of the side conversations may have changed (side.rs `init`).
    pub fn sides_changed(&self) {
        let _ = self.tx.send(Msg::Sides);
    }

    /// What the board lists of Codealong's own terminals may have changed (terminals.rs `init`).
    pub fn terminals_changed(&self) {
        let _ = self.tx.send(Msg::Terminals);
    }

    /// What the board lists of the listenings may have changed (listen.rs).
    pub fn listenings_changed(&self) {
        let _ = self.tx.send(Msg::Listenings);
    }

    pub fn refresh_usage(&self) {
        let _ = self.tx.send(Msg::RefreshUsage);
    }

    /// Drops `key`'s kept AI background so a fresh one is drawn immediately.
    pub fn redraw_background(&self, key: String) {
        let _ = self.tx.send(Msg::RedrawBackground(key));
    }

    /// How something Codealong does by itself in the background went (`Trouble` lists them, with
    /// what does each): a failure is a notice until a later one works.
    pub fn went(&self, what: Trouble, result: std::result::Result<(), String>) {
        let _ = self.tx.send(Msg::Went(what, result.err()));
    }

    /// What your other devices run, as their latest boards say (peers.rs): the PRs a session runs
    /// on there, which a PR's primary session here isn't resumed beside (nudges.rs
    /// `stopped_primaries`), and the PRs a session is actively working on there (board/prs.rs
    /// `working_elsewhere`), which stay out of `waits_to_merge`.
    pub fn peer_lanes(&self, lanes: &[Lane], pr_sessions: &std::collections::BTreeMap<String, Vec<crate::model::PrSession>>) {
        let _ = self.tx.send(Msg::RunningElsewhere(crate::nudges::running_elsewhere(lanes), crate::board::working_elsewhere(lanes, pr_sessions)));
    }

    /// Your machines that share automations, as their feeds say (peers.rs `machines`): which of
    /// them reviews each PR that asks for yours (autoreview.rs `reviewer_of`).
    pub fn peer_machines(&self, machines: crate::shared::Machines) {
        let _ = self.tx.send(Msg::Machines(machines));
    }

    /// How a job of the automatic reviews went (autoreview.rs): a failure is a notice until a
    /// later job goes through, and the PRs' cards follow reviews.json at once.
    pub fn reviewed(&self, result: std::result::Result<(), autoreview::Said>) {
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

    pub fn set_pinned_repos(&self, keys: Vec<String>) {
        let _ = self.tx.send(Msg::SetPinnedRepos(keys));
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
    /// Codealong, where a session of any other harness, or one its harness wouldn't archive just
    /// now, stays archived until it's active again. Says when the board shows it and what its
    /// harness kept.
    pub fn archive(&self, key: &str, archived: bool) -> Result<Archived> {
        let kept = self.archive_in_harness(key, archived)?;
        let only_here = kept.iter().map(|_| key.to_string()).collect();
        Ok(Archived { at: self.set_archived(vec![key.to_string()], archived, only_here)?, kept })
    }

    /// Archives several sessions as `archive` does each, shown on one board: each key's result,
    /// in order. One its harness refused isn't archived in Codealong either.
    pub fn archive_all(&self, keys: &[String]) -> Vec<Result<()>> {
        self.archive_batch(keys, true).into_iter().map(|r| r.map(|_| ())).collect()
    }

    /// Archives or unarchives several sessions as `archive` does each, shown on one board: each
    /// key's result (with why its harness kept it, if any), in order. One its harness refused
    /// isn't changed in Codealong either.
    pub fn archive_batch(&self, keys: &[String], archived: bool) -> Vec<Result<Option<String>>> {
        let mut results: Vec<Result<Option<String>>> = keys.iter().map(|k| self.archive_in_harness(k, archived)).collect();
        let only_here = keys.iter().zip(&results).filter(|(_, r)| matches!(r, Ok(Some(_)))).map(|(k, _)| k.clone()).collect();
        let done: Vec<String> = keys.iter().zip(&results).filter(|(_, r)| r.is_ok()).map(|(k, _)| k.clone()).collect();
        if !done.is_empty() {
            if let Err(e) = self.set_archived(done, archived, only_here) {
                let e = crate::util::words(e);
                for r in results.iter_mut().filter(|r| r.is_ok()) {
                    *r = Err(anyhow!("{e}"));
                }
            }
        }
        results
    }

    fn archive_in_harness(&self, key: &str, archived: bool) -> Result<Option<String>> {
        #[cfg(test)]
        if self.in_memory {
            return Ok(None);
        }
        archive_in_harness(key, archived)
    }

    fn set_archived(&self, keys: Vec<String>, archived: bool, only_here: HashSet<String>) -> Result<Option<i64>> {
        // A session's terminals of Codealong's own go with it, however it's archived (Archive,
        // Mark as done, Close idle sessions, a helper's or a moved session's archiving), as the
        // terminal it ran in does.
        if archived {
            for key in &keys {
                crate::terminals::close_session(key);
            }
        }
        let (tx, rx) = channel();
        if self.tx.send(Msg::SetArchived(keys, archived, only_here, tx)).is_err() {
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
        Ok(board::pr_card(&[], &[], slug, &HashMap::new(), &raw, login.as_deref(), &board::PrLinks::default(), &std::collections::BTreeSet::new()))
    }

    /// The accounts in config.toml, each with whether it's signed in.
    pub fn accounts(&self) -> Result<Vec<AccountInfo>> {
        // Settings reads them again as it comes forward, after an agent installed a command line, say.
        crate::detect::forget_shell();
        self.accounts_as_kept()
    }

    /// The accounts as `accounts` gives them, with what your shell said of their command lines
    /// kept for its ten minutes (`detect::shell_finds`): for `codealong accounts`, which agents
    /// run often, and which would otherwise start a login shell each time for a command line the
    /// PATH misses.
    pub fn accounts_as_kept(&self) -> Result<Vec<AccountInfo>> {
        let cfg = config::current()?;
        Ok(cfg.accounts.iter().map(usage::describe).collect())
    }

    /// Saves the account list to config.toml. The watcher sees the change and reloads.
    pub fn save_accounts(&self, accounts: Vec<AccountCfg>) -> Result<()> {
        config::save_accounts(&accounts)
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
        let place = launch::Placement { new_terminal: cfg.apps.new_terminal, near: &crate::board::ALONE, behind: false, label: None };
        launch::terminal_open(&cfg.apps.terminal, place, &home(), Some(&format!("{}\n", usage::sign_in_command(a))))
    }

    /// Turns training off for an account at its provider, and answers where it stands after:
    /// read again, which goes on the board at once. Blocks for the calls.
    pub fn stop_training(&self, name: &str) -> Result<training::Training> {
        let cfg = config::current()?;
        let a = cfg.accounts.iter().find(|a| a.name == name).ok_or_else(|| anyhow!("There's no account named {name}."))?;
        let after = training::stop(a)?;
        let _ = self.tx.send(Msg::TrainingOne(crate::accountpoll::identity(a), after.clone()));
        Ok(after)
    }

    /// Uses one of an account's limit resets, `credit`, else the first to expire, and answers how
    /// it went: what the account holds after goes on the board at once, read again even when the
    /// use failed, as one that wasn't answered may have gone through. Blocks for the calls.
    pub fn use_reset(&self, name: &str, credit: Option<&str>) -> Result<crate::resets::ResetUsed> {
        let cfg = config::current()?;
        let a = cfg.accounts.iter().find(|a| a.name == name).ok_or_else(|| anyhow!("There's no account named {name}."))?;
        let credit = match credit.filter(|c| !c.is_empty()) {
            Some(c) => c.to_string(),
            None => crate::resets::first_to_expire(a)?,
        };
        let asked_as = crate::accountpoll::identity(a);
        crate::resets::use_one(a, &credit, |after| {
            let _ = self.tx.send(Msg::ResetsOne(asked_as, after));
        })
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

    /// Opens a lane's target where it lives (`open_here`), marking its session seen.
    pub fn open(&self, lane_id: &str, target_id: &str) -> Result<()> {
        let board = self.board.read().map(|b| b.clone()).map_err(|_| anyhow!("board unavailable"))?;
        if let Some(key) = open_here(&board, lane_id, target_id)? {
            self.mark_seen(vec![key]);
        }
        Ok(())
    }
}

/// Archives a session in its harness, or takes it out of its archive, where the harness has one
/// another program can reach (`Adapter::archive`). Says why the harness kept it, where it would
/// only be archived in Codealong.
fn archive_in_harness(key: &str, archived: bool) -> Result<Option<String>> {
    let mut parts = key.splitn(3, ':');
    let (Some(harness), Some(account), Some(id)) = (parts.next(), parts.next(), parts.next()) else {
        bail!("That isn't a session.");
    };
    if let Some(adapter) = harness::named(harness) {
        let cfg = config::current()?;
        let a = cfg.accounts.iter().find(|a| a.name == account).ok_or_else(|| anyhow!("There's no account named {account}."))?;
        return adapter.archive(a, id, archived);
    }
    Ok(None)
}
