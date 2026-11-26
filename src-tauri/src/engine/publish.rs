//! The board: composed from everything the engine holds (`compose`, on `board::compose`), and
//! published when it differs from the last one (`publish`, by `board_hash`), with what's done
//! once one is composed: starts linked to the sessions they made, spent marks forgotten, the
//! decision models asked. And what the board is given of PRs' links (`pr_links`).

use super::*;

impl Engine {
    /// A PR the board doesn't list, matched to these lanes' worktrees and sessions.
    pub(super) fn pr_card(&self, lanes: &[Lane], slug: &str, raw: &PrRaw, login: Option<&str>) -> PrItem {
        let links = self.pr_links();
        board::pr_card(&self.seeds, lanes, slug, self.github.repo_names(), raw, self.github.login().or(login), &links, &self.working_elsewhere)
    }

    /// What pr-links.json and the sessions' records say of PRs, for the board, a moved
    /// conversation's records going on in its copy (`prlinks::carry_moves`).
    pub(super) fn pr_links(&self) -> board::PrLinks {
        let origins = origins::current();
        let moves = origins.moves();
        let (mut worked, unlinked, made) = prlinks::resolve(&prlinks::carry_moves(&self.pr_marks, &moves, prlinks::add_mark), &self.pr_store);
        // By the board before this one, as sends and schedules follow a session. With none yet,
        // every copy is taken to be there, or a watch would seem to begin with the next board.
        let picked = {
            let move_index = origins.move_index();
            let board = self.board.read();
            let keys: Option<HashSet<&str>> = if move_index.is_empty() {
                None
            } else {
                board.as_ref().ok().filter(|b| !b.lanes.is_empty()).map(|b| b.lanes.iter().flat_map(|l| l.sessions.iter().filter(|s| s.visitor.is_none()).map(|s| s.key.as_str())).collect())
            };
            let exists = |k: &str| keys.as_ref().map_or(true, |set| set.contains(k));
            prlinks::picks_after_moves(&self.pr_store, |k| move_index.moved_to(k, exists), &mut worked)
        };
        board::PrLinks {
            worked,
            unlinked,
            made,
            pushed: prlinks::carry_moves(&self.pr_pushes, &moves, prlinks::add_push),
            picked,
            linked_at: self.pr_store.links.iter().filter(|l| l.linked).fold(HashMap::new(), |mut at, l| {
                let t = at.entry(prlinks::pr_key(&l.pr)).or_insert(l.at);
                *t = (*t).max(l.at);
                at
            }),
            nudged: self.pr_store.nudges.iter().cloned().collect(),
            not_nudged: self.pr_store.nudges_off.iter().cloned().collect(),
            watched: self.pr_store.watched.iter().cloned().collect(),
            nudge_by_default: self.cfg.pull_requests.nudge_by_default,
            nudge_by_default_in: self.team_files.settings(&self.cfg.repos).nudges_by_default(),
            agent_posts: self.reviews.prs.values().flat_map(|t| t.rounds.iter().filter_map(|r| r.comment.clone())).collect(),
        }
    }

    /// Writes down where headless runs came from once they've outlived their process
    /// (`origins::ended_runs`), as `with_run_origins` found it while they ran. The board shows
    /// those origins already, so it isn't composed again.
    fn keep_run_origins(&self, board: &Board) {
        // Let go before the file's written: another thread may be composing a board.
        let runs = {
            let found = self.run_origins.lock().unwrap_or_else(|e| e.into_inner());
            if found.is_empty() {
                return;
            }
            origins::ended_runs(board, &found, &origins::current())
        };
        if !runs.is_empty() {
            origins::record_runs(&runs);
        }
    }

    /// Notes where sessions came from that starts from other sessions were waiting for, and
    /// finishes handing over ownership (todos, labels, pin, terminals, schedules) for any pending
    /// cross-provider move whose target session just appeared.
    fn link_origins(&mut self, board: &Board) -> bool {
        let store = origins::current();
        if store.pending.is_empty() {
            return false;
        }
        let lanes = todos::LaneIndex::of(&board.lanes);
        let (changed, handovers) = origins::resolve_pending_handovers(&origins::candidates(board, &store.pending), &lanes, now_ms());
        for h in handovers {
            self.finish_pending_handover(board, &lanes, &h);
        }
        changed
    }

    /// Completes ownership handover once an async-ID move's target session appears on the board.
    fn finish_pending_handover(&mut self, board: &Board, lanes: &todos::LaneIndex, h: &origins::ResolvedHandover) {
        let mut todos_changed = false;
        let matching: Vec<(String, bool)> = self
            .todos
            .todos
            .iter()
            .filter(|t| t.sessions.iter().any(|s| s == &h.from))
            .map(|t| (t.id.clone(), t.primary.as_ref().is_some_and(|p| p.session == h.from)))
            .collect();
        for (id, is_primary) in matching {
            let op = if is_primary {
                todos::TodoOp::Primary { id, session: Some(h.copy.clone()) }
            } else {
                todos::TodoOp::Link { id, worktree: None, session: Some(h.copy.clone()), linked: true }
            };
            if self.todos.apply(op, lanes, now_ms()).unwrap_or(false) {
                todos_changed = true;
            }
        }
        if todos_changed {
            self.save_todos();
        }
        let mut state_changed = false;
        let label_ids: Vec<String> = self.seen.labels_of(&h.from).to_vec();
        if !label_ids.is_empty() {
            state_changed |= self.seen.label(&h.copy, &label_ids, &[]);
        }
        if h.was_pinned {
            state_changed |= self.seen.pin(&h.copy, true);
        }
        if state_changed {
            self.save_state();
        }
        crate::terminals::follow(&h.from, &h.copy);
        if let Err(e) = crate::schedules::runner::copy_takes_place(board, &h.from, &h.copy) {
            eprintln!("Moved {}, but couldn't send its schedules' runs to {}: {e:#}", h.from, h.copy);
        }
    }

    pub(super) fn compose(&self) -> Board {
        let now = now_ms();
        let apps = AppNames {
            terminal: launch::terminal_name(&self.cfg.apps.terminal),
            terminal_id: self.cfg.apps.terminal.clone(),
            editor: editors::name(&self.cfg.apps.editor),
            editor_id: self.cfg.apps.editor.clone(),
            editors: editors::installed(),
            diff_editor: editors::diff_editor(&self.cfg.apps.editor),
            new_terminal: self.cfg.apps.new_terminal,
            run_in: self.cfg.apps.run_in,
        };
        let order: Vec<String> = self.cfg.projects.iter().map(|p| p.name.clone()).collect();
        let notices = self.notices();
        let usage = self.compose_usage(now);
        let pr_links = self.pr_links();
        let repo_settings = self.team_files.settings(&self.cfg.repos);
        let sides = crate::side::glances();
        let listenings = crate::listen::glances();
        let merged = self.github.merged_heads();
        let at_merged: HashSet<PathBuf> = self.heads_now().into_iter().filter(|(_, head)| merged.contains(head)).map(|(path, _)| path).collect();
        let mut board = board::compose(board::Inputs {
            now,
            apps: &apps,
            seeds: &self.seeds,
            sessions: self.sessions(now, &usage, &sides, &listenings),
            git: &self.git,
            prs: self.github.prs(),
            requested: self.github.requested(),
            made: self.github.made(),
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
            machines: &self.machines,
            session_settings: &self.cfg.sessions,
            at_merged: &at_merged,
            working_elsewhere: &self.working_elsewhere,
        });
        // Starts whose sessions aren't here yet: a line under the session that asked for each, and
        // a notice for one in a terminal that never made its session, with what its tab shows.
        let word_of = |account: &str| self.harness_word(account);
        let waiting = origins::waiting_for(&board, now);
        let stuck = origins::stuck_notices(&waiting, &board, word_of, |at| self.screens.screen(at).map(String::from), now);
        board.notices.extend(stuck);
        board.starting = crate::starting::by_session(&waiting, &board, &self.screens, word_of, now);
        board.usage = usage;
        let signed_out = board::signed_out_notices(&board);
        board.notices.extend(signed_out);
        board.training = self.training.list(&self.cfg.accounts);
        board.resets = self.resets.list(&self.cfg.accounts);
        board.sign_ins = self.sign_ins.list(&self.cfg);
        board.mcp = self.mcp.list(&self.cfg.accounts);
        board.sides = sides;
        adopt_terminals(&board.lanes, |pid| crate::procs::tty(pid).map(|t| t.to_string_lossy().into_owned()));
        board.terminals = crate::terminals::glances();
        board.listenings = listenings;
        board.meeting = crate::meetings::offered();
        board.meeting_soon = crate::meetings::soon();
        board.load = self.load.now();
        board.github_polled = self.github.polled();
        board.github_current = self.github.current();
        board.when_limited = self.cfg.limits.when_hit;
        board.nudge_about = self.cfg.pull_requests.nudge_about;
        board.merged_here = self.merged_here.iter().filter(|(_, at)| now - **at < 15 * 60_000).map(|(k, _)| k.clone()).collect();
        board.pr_stopped = crate::nudges::stopped_primaries(&board, &self.running_elsewhere, self.cfg.sessions.resume_for_pr_news, |s| crate::send::reaches_stopped(&self.cfg, s));
        board.reviews = self.cfg.reviews.clone();
        board.repos = self.cfg.repos.clone();
        board.repo_settings = repo_settings;
        for pr in &mut board.prs {
            pr.review_first = board.repo_settings.pull_requests(&self.cfg.pull_requests, pr).reviews_first;
        }
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
        board.pinned_repos = self.seen.pinned_repos.clone();
        board.theme = self.cfg.appearance.theme;
        board.zoom = self.cfg.appearance.zoom;
        board.glass = self.cfg.appearance.glass;
        board.show_keys = self.cfg.appearance.show_keys;
        board.show_started = self.cfg.sessions.show_started;
        board.tools = self.cfg.appearance.tools;
        board.quick_shortcut = self.cfg.shortcuts.quick_panel.clone();
        board.chat_shortcut = self.cfg.shortcuts.chat.clone();
        board.waiting_shortcut = self.cfg.shortcuts.waiting.clone();
        board.improve_shortcut = self.cfg.shortcuts.improve.clone();
        board.main_window_shortcut = self.cfg.shortcuts.main_window.clone();
        let in_use = self.cfg.shortcuts_in_use();
        board.talk_shortcut = in_use.talk;
        board.talk_orchestrator_shortcut = in_use.talk_orchestrator;
        board.listen_shortcut = in_use.listen;
        // Codealong's own source's worktrees, and where a session that improves it goes.
        let sources: HashSet<PathBuf> = self.codealong_sources.iter().map(|(root, _)| root.clone()).collect();
        crate::improve::mark(&mut board.lanes, &self.lane_checkout, &sources);
        board.improve = crate::improve::Improve { installed: crate::updates::enabled(), ..crate::improve::target(&board.lanes, &self.codealong_sources) };
        board.decisions = self.cfg.decisions.clone();
        board.experimental = self.cfg.experimental.clone();
        board.backgrounds = self.cfg.backgrounds.clone();
        board.queue = self.cfg.queue.settled();
        // While `[queue]` leaves reviews or merges out, PRs of that kind are no cause for the
        // mark's color, as they're none for the bar's pills or the notch's count.
        let (reviews_on, merges_on) = (board.queue.waits("reviews"), board.queue.waits("merges"));
        if !reviews_on || !merges_on {
            board.logo_status = board::logo_status_with(&board.lanes, &board.prs, reviews_on, merges_on);
        }
        board.devices = self.cfg.devices.clone();
        board.project_rules = self.cfg.projects.clone();
        let project_owners = crate::orgavatars::project_owners(&self.cfg.projects, &self.seeds, |slug| self.github.renamed(slug).map(String::as_str));
        for p in &mut board.projects {
            p.icon = project_owners.get(&p.name).and_then(|owner| self.org_avatars.icon_for(owner)).map(str::to_string);
        }
        for lane in &mut board.lanes {
            lane.repo_slug_now = lane.repo_slug.as_deref().and_then(|slug| self.github.renamed(slug).cloned());
        }
        board.perf_log = perf::enabled();
        board.todos = todos::awake_at(self.todos.todos.clone(), now);
        board.today = crate::util::day_key(crate::util::today_local());
        board.todo_clock = todos::todo_clock(&self.todos.todos, now);
        board.todos_unreadable = self.todos_unreadable;
        board.labels = self.todos.labels.clone();
        // Once the board has the PRs and the todos a session's work is known by.
        board::settle_finished(&mut board.lanes, &board.prs, &board.todos);
        if self.cfg.decisions.todos {
            board.todo_hints = self.jev_todos.hints(&board, &self.todos.todos);
        }
        board.machines = self.machines.clone();
        board.schedules = crate::schedules::items(&self.schedules, &self.schedule_runs, &board, &self.cfg, now, &crate::schedules::rule::Local);
        board.schedules.iter_mut().for_each(|i| i.emblem = self.emblems.of_schedule(&i.schedule.id, self.cfg.emblems.kind));
        crate::schedules::orchestrate::mark(&mut board.lanes, &board.schedules);
        board.shared_schedules = crate::schedules::machines::outgoing(&self.schedules, &self.schedule_runs, &board, &self.cfg, &crate::schedules::rule::Local);
        board.shared_schedules.list.iter_mut().for_each(|x| x.emblem = self.emblems.shared(&x.id));
        board.schedules_problem = self.schedules_problem.clone();
        // Once the board has its worktrees, its todos, the day and `[queue]`.
        board.todo_queue = crate::barfeed::todo_queue(&board);
        board
    }

    /// The usage of each account that isn't turned off: one turned off has no place in the bar.
    fn compose_usage(&self, now: i64) -> Vec<Usage> {
        self.cfg
            .accounts
            .iter()
            .filter(|a| !a.disabled)
            .map(|a| {
                let adapter = harness::of(a);
                if let Some(why) = adapter.and_then(|h| h.no_limits(a)) {
                    return usage::none(a, why);
                }
                // What comes with the sessions read anyway, else the last background read.
                let read = adapter.and_then(|h| self.readers.iter().find(|r| r.harness() == h.harness())).and_then(|r| r.usage(a, now));
                read.unwrap_or_else(|| self.usage.get(&a.name).cloned().unwrap_or_else(|| usage::loading(a)))
            })
            .collect()
    }

    pub(super) fn publish(&mut self) {
        let t = Instant::now();
        self.store_warnings = crate::store::problems(&config::support_dir());
        let mut board = self.compose();
        // Rare, so the board is composed again rather than patched. Before the todos' starts take
        // their sessions, so that a helper whose start an app made is known for one and isn't
        // taken (`todos::SessionRef::helper`).
        if self.link_origins(&board) {
            board = self.compose();
        }
        if self.link_pending(&board) {
            board.todos = todos::awake_at(self.todos.todos.clone(), board.generated_at);
            board.todo_queue = crate::barfeed::todo_queue(&board);
        }
        self.keep_run_origins(&board);
        // Where each session came from, for the ledger to keep (began.rs).
        if self.cfg.experimental.analytics && crate::began::due() {
            crate::began::note(crate::began::noted_on(&board, &crate::schedules::own_sessions(&self.schedules, &self.schedule_runs, &board)));
        }
        // The board already leaves them out.
        self.forget_stale_suggestions(&board);
        self.forget_stale_tags(&board);
        self.forget_spent_agent_seen(&board);
        self.forget_spent_aside(&board);
        self.wake_called_back(&board);
        self.jev_look(&board);
        // Rare too: a schedule's emblem coming, or a session of it showing.
        if self.look_emblems(&board) {
            board = self.compose();
        }
        self.look_backgrounds(&board);
        let composed = t.elapsed();
        self.dirty = false;
        self.dirty_git = false;
        let since_last = self.last_publish.map(|t| t.elapsed());
        self.last_publish = Some(Instant::now());
        let hash = board_hash(&mut board);
        let hashed = t.elapsed() - composed;
        let asked_by = std::mem::take(&mut self.asked_by);
        if hash == self.last_hash {
            if perf::enabled() {
                let by = if asked_by.is_empty() { "something unnamed".to_string() } else { asked_by.into_iter().collect::<Vec<_>>().join(", ") };
                perf::log(&format!("publish skipped, nothing changed: compose {} ms, hash {} ms; asked for by {by}", composed.as_millis(), hashed.as_millis()));
            }
            return;
        }
        self.last_hash = hash;
        // Numbered after the hash, which leaves the number out: what the windows are sent of it
        // says which board it changes (windowfeed.rs).
        self.rev += 1;
        board.rev = self.rev;
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
        // A whole pass: read, composed and out (`run_guarded`).
        self.passed = true;
        if perf::enabled() {
            let line = self.perf.publish_line(&board, composed, hashed, emitted, since_last, self.git_queue.running());
            perf::log(&line);
        }
    }
}

/// A hash of everything the UI shows except the timestamp, so identical boards aren't resent.
/// The board is hashed as it's written out, with its timestamp set aside meanwhile: it's some
/// 2 MB, and neither a copy of it nor its text is made for this.
pub(crate) fn board_hash(board: &mut Board) -> u64 {
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
    // Neither what's read of the repos' team files, which limits have lifted long enough to be
    // carried on, nor which PRs' unsaid fixes have settled long enough to ask their reviewer about
    // is sent, but what acts on the board waits on them; the last two come with time alone, which
    // the heartbeat's board then goes out for. Nor what this machine's reviews of each PR come to,
    // which goes to your other machines in its feed (peerfeed.rs).
    let lifted = crate::reconnect::lifted(board, board.generated_at);
    let settled: Vec<String> = crate::rerequest::settled(board, board.generated_at).into_iter().map(String::from).collect();
    let shared: Vec<&codealong_peer::pr::SharedReview> = board.prs.iter().map(|p| &p.shared_review).collect();
    let at = std::mem::take(&mut board.generated_at);
    let written = serde_json::to_writer(&mut out, &*board).and_then(|()| serde_json::to_writer(&mut out, &(board.repo_settings.readiness(), lifted, settled, shared)));
    board.generated_at = at;
    // A board that can't be written out hashes as nothing at all, as it did.
    match (written, out.into_inner()) {
        (Ok(()), Ok(hashed)) => hashed.0.finish(),
        _ => std::collections::hash_map::DefaultHasher::new().finish(),
    }
}

/// A terminal of Codealong's that a live session's own process runs in is that session's
/// (launch/codealong.rs): one opened to start or resume a session belongs to none until its
/// process turns up on the board, found by the terminal device it has (`tty_of`), and one a
/// session lives in goes to whichever session runs there now, as a new conversation in the same
/// process (`/clear`, `/resume` in Claude Code) is a new session. A terminal beside a session (its
/// drawer's) stays that session's, whatever you run in it. Asked only while there's such a
/// terminal, so most boards cost nothing.
fn adopt_terminals(lanes: &[crate::model::Lane], tty_of: impl Fn(u32) -> Option<String>) {
    let open: Vec<crate::terminals::Glance> = crate::terminals::list().into_iter().filter(|g| g.running && (g.session.is_empty() || g.hosts) && !g.tty.is_empty()).collect();
    if open.is_empty() {
        return;
    }
    for s in lanes.iter().flat_map(|l| &l.sessions).filter(|s| s.live && !s.archived) {
        let Some(tty) = s.pid.and_then(&tty_of) else { continue };
        if let Some(g) = open.iter().find(|g| g.tty == tty && g.session != s.key) {
            crate::terminals::adopt(&g.id, &s.key);
        }
    }
}

#[cfg(test)]
mod tests {
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

    #[test]
    fn boards_that_differ_only_in_their_time_hash_alike() {
        use super::*;
        let board = |at: i64, loading: bool| Board { generated_at: at, loading, quick_shortcut: "Alt+Cmd+/".into(), ..Board::default() };
        let (mut first, mut later, mut other) = (board(1_000, false), board(2_000, false), board(1_000, true));
        assert_eq!(board_hash(&mut first), board_hash(&mut later));
        assert_ne!(board_hash(&mut first), board_hash(&mut other), "a real difference");
        later.notices.push(Notice { key: "github".into(), title: "GitHub didn't answer".into(), text: "GitHub: couldn't load PRs".into(), ..Default::default() });
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

    /// A limit that has lifted long enough to be carried on changes nothing the windows get, and
    /// comes with time alone, but a board saying so has to go out for reconnect.rs to hear it.
    #[test]
    fn a_limit_lifted_long_enough_to_carry_on_is_a_change_to_publish() {
        use super::*;
        let limit = SessionLimit { at: 1_000, resets_at: Some(10_000), continues: true, ..SessionLimit::default() };
        let session = Session { live: true, status: Status::Error, limit: Some(limit), ..crate::fixtures::session("claude:claude:a") };
        let board = |at: i64| Board { generated_at: at, lanes: vec![crate::fixtures::lane("/work", vec![session.clone()])], ..Board::default() };
        let (mut before, mut lifted) = (board(10_001), board(10_001 + 60 * 60_000));
        let mut later = board(10_002 + 60 * 60_000);
        assert_ne!(board_hash(&mut before), board_hash(&mut lifted));
        assert_eq!(board_hash(&mut lifted), board_hash(&mut later), "only once");
    }

    /// A terminal opened to start a session becomes that session's own once its process runs
    /// there, found by the device; a session elsewhere takes none.
    #[test]
    #[cfg_attr(windows, ignore = "Codealong's own terminals aren't on Windows yet")]
    fn a_session_running_in_a_terminal_of_codealongs_makes_it_its_own() {
        use super::*;
        use crate::terminals;
        let g = terminals::open(terminals::Open { folder: std::env::temp_dir(), command: Some("sleep 5".into()), size: Some(terminals::DEFAULT_SIZE), ..Default::default() }).unwrap();
        let here = crate::model::Session { live: true, pid: Some(4242), ..crate::fixtures::session("test:adopts:here") };
        let elsewhere = crate::model::Session { live: true, pid: Some(4343), ..crate::fixtures::session("test:adopts:elsewhere") };
        let lanes = vec![crate::fixtures::lane("/r", vec![elsewhere, here])];
        let tty = g.tty.clone();
        adopt_terminals(&lanes, |pid| match pid {
            4242 => Some(tty.clone()),
            _ => Some("/dev/ttys999".into()),
        });
        let found = terminals::glances().get("test:adopts:here").cloned().unwrap_or_default();
        assert!(found.iter().any(|t| t.id == g.id && t.hosts), "{found:?}");
        assert!(terminals::glances().get("test:adopts:elsewhere").is_none());
        // A new conversation in the same process (`/clear`): the terminal goes with it.
        let next = crate::model::Session { live: true, pid: Some(4242), ..crate::fixtures::session("test:adopts:next") };
        let tty = g.tty.clone();
        adopt_terminals(&[crate::fixtures::lane("/r", vec![next])], |pid| (pid == 4242).then(|| tty.clone()));
        assert!(terminals::glances().get("test:adopts:next").is_some_and(|l| l.iter().any(|t| t.id == g.id && t.hosts)));
        assert!(terminals::glances().get("test:adopts:here").is_none());
        terminals::close(&g.id).unwrap();
    }
}
