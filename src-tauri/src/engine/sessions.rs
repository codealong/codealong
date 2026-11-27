//! The board's sessions: what the harnesses' records say (`build_sessions`), with all the engine
//! knows besides laid over each, a stage at a time (`laid_over`: each stage is a `with_*` here,
//! most of them calling a rule of board/overlays.rs), and what's worked out from the whole
//! settled last (`settled`). The order of the stages is the rule where two speak of one thing.

use super::*;

/// What terminal.rs knows a session waits on, over Jev's read of it (`with_insights`), which can
/// only guess from a command gone quiet: a command it asks you to allow (`overlay`, laid on as
/// the sessions are built too, for Jev to read them with it), a run you allowed that its agent
/// waits on (`overlay_running`), and a command you ran for it, whose output it's sent when it ends
/// (`overlay_yours`), which Jev reads as a turn that ended; and either of those runs stopped at a
/// question in Codealong's terminal (`overlay_input`), which waits for you to type there.
fn with_runs(mut sessions: Vec<Session>, now: i64) -> Vec<Session> {
    for s in &mut sessions {
        crate::terminal::overlay(s);
        crate::terminal::overlay_running(s, now);
        crate::terminal::overlay_yours(s, now);
        crate::terminal::overlay_input(s, now, crate::terminals::asking);
    }
    sessions
}

/// Each live session's terminal app (`Session::terminal`): the one its process runs under, asked
/// of the process table on every compose as `launch::terminal_kind` is meant to be. tmux, which
/// shows in its client's app, is none.
fn with_terminals(mut sessions: Vec<Session>) -> Vec<Session> {
    for s in sessions.iter_mut().filter(|s| s.live) {
        s.terminal = s.pid.and_then(launch::terminal_kind).filter(|t| t.profile().is_some()).map(|t| t.name().to_string());
    }
    sessions
}

impl Engine {
    /// Sessions with their emblems, of the kind `[emblems]` has on.
    fn with_emblems(&self, mut sessions: Vec<Session>) -> Vec<Session> {
        for s in &mut sessions {
            s.emblem = self.emblems.emblem_of(s, self.cfg.emblems.kind);
        }
        sessions
    }

    /// Sessions with the read of their moment applied (board::apply_insight): Jev's, and the
    /// landing their reply's status line says, when `[decisions] sessions` is on; an agent's tag of
    /// them over both, whether or not it is (tags.rs, `insights::merged`).
    /// Each session that asked for a start that's stuck in its terminal (`origins::Pending::stuck`)
    /// needs you for it once it isn't taking a turn (`starting::overlay`), as what its terminal
    /// shows was last read: its helper can't start until you answer that. The starts are as the
    /// last board left them (`link_origins`), as `with_origins` has them.
    fn with_starts(&self, mut sessions: Vec<Session>, now: i64) -> Vec<Session> {
        let store = origins::current();
        if store.pending.is_empty() {
            return sessions;
        }
        let waits = crate::starting::waiting(&store.pending, &self.screens, |account| self.harness_word(account), &launch::terminal_name(&self.cfg.apps.terminal), now);
        for s in &mut sessions {
            crate::starting::overlay(s, waits.get(&s.key));
        }
        sessions
    }

    /// An account's harness by its word (Codex), or the account's name where config.toml has no
    /// such account.
    pub(super) fn harness_word(&self, account: &str) -> String {
        self.cfg.accounts.iter().find(|a| a.name == account).and_then(harness::of).map_or_else(|| account.to_string(), |h| h.harness().word().to_string())
    }

    /// Marks each orchestrator's session as its own (`Session::orchestrator`), from the schedules'
    /// runs, followed to where it moved (schedules/orchestrate.rs `sessions`).
    fn with_orchestrators(&self, mut sessions: Vec<Session>) -> Vec<Session> {
        let by = crate::schedules::orchestrate::sessions(&self.schedules, &self.schedule_runs);
        if by.is_empty() {
            return sessions;
        }
        let origins = crate::origins::current();
        let moves = origins.move_index();
        let by: HashMap<String, crate::model::Orchestrator> = if moves.is_empty() {
            by
        } else {
            let keys: HashSet<&str> = sessions.iter().map(|s| s.key.as_str()).collect();
            by.into_iter().map(|(k, o)| (moves.moved_to(&k, |m| keys.contains(m)).unwrap_or(k), o)).collect()
        };
        for s in &mut sessions {
            if let Some(o) = by.get(&s.key) {
                // Called Orchestrator, not its schedule's name, which names its project.
                let title = crate::schedules::orchestrate::session_title(&s.title, o.project.as_deref());
                if title != s.title {
                    s.title = title.to_string();
                }
                s.orchestrator = Some(o.clone());
            }
        }
        sessions
    }

    fn with_insights(&self, mut sessions: Vec<Session>, now: i64) -> Vec<Session> {
        let tags = tags::current();
        if !self.cfg.decisions.sessions && tags.sessions.is_empty() {
            return sessions;
        }
        for s in &mut sessions {
            let tag = tags.sessions.get(&s.key).filter(|t| tags::holds(s, t)).map(|t| &t.insight);
            // A turn too long ago for its read to stand keeps the time its reply put the work off
            // to, and nothing else of the read (`Store::put_off`).
            let put_off;
            let (jev, said) = if self.cfg.decisions.sessions {
                let asked = crate::insights::ask(s, now);
                put_off = match asked {
                    Some(_) => None,
                    None => self.insights.put_off(s, now).map(|until| SessionInsight { waits_until: Some(until), ..Default::default() }),
                };
                let jev = asked.and_then(|(_, signature)| self.insights.for_session(&s.key, &signature)).or(put_off.as_ref());
                (jev, crate::insights::landing_said(s))
            } else {
                (None, None)
            };
            if let Some(i) = crate::insights::merged(jev, said, tag) {
                board::apply_insight(s, &i);
                board::settle_background_insight(s, &i, self.seen.seen_at(&s.key), now);
            }
        }
        sessions
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

/// Where sessions came from by Codealong's own record (origins.rs): started by an agent's
    /// `codealong start`, or forked from Codealong where the harness doesn't say. What the
    /// harness recorded comes first. With it, what each helper helps (`board::helper_of`): the
    /// session that started it as one (origins.json), or the PR whose automatic review it is
    /// (reviews.json, or its first prompt: `autoreview::reviewers`), else the session whose
    /// scratch folder it runs in (`board::scratch_helpers`), unless you made it your own
    /// (`SeenState::own`); whether you're talking to it (`SeenState::talking`); and the copy a
    /// session that moved goes on as (`Session::moved_to`).
    fn with_origins(&self, mut sessions: Vec<Session>) -> Vec<Session> {
        let store = origins::current();
        let reviewers = autoreview::reviewers(&self.reviews, &sessions);
        if !store.sessions.is_empty() || !reviewers.is_empty() {
            let keys: HashSet<String> = sessions.iter().map(|s| s.key.clone()).collect();
            for s in sessions.iter_mut() {
                if s.origin.is_none() {
                    s.origin = store.origin_of(&s.key);
                }
                if !self.seen.is_own(&s.key) {
                    s.helper = board::helper_of(store.helper_of(&s.key), |k| keys.contains(k), reviewers.get(s.key.as_str()).copied());
                }
            }
        }
        store.mark_moves(&mut sessions);
        board::scratch_helpers(&mut sessions, crate::harness::scratch_owner, |k| self.seen.is_own(k));
        if !self.seen.talking.is_empty() {
            for s in sessions.iter_mut() {
                if let Some(h) = s.helper.as_mut() {
                    h.yours = self.seen.is_talking(&s.key);
                }
            }
        }
        sessions
    }

    /// Recent peer-to-peer exchanges between non-helper sessions (chatter.rs), attached after
    /// `with_origins` so helper links are excluded.
    fn with_chatter(&self, mut sessions: Vec<Session>, now: i64) -> Vec<Session> {
        let store = origins::current();
        let mut by_key = crate::chatter::peers_for(&sessions, &store, now);
        if by_key.is_empty() {
            return sessions;
        }
        for s in &mut sessions {
            if let Some(peers) = by_key.remove(&s.key) {
                s.peers = peers;
            }
        }
        sessions
    }

    /// Each headless run with the session that started it as its origin, where nothing else
    /// said where it came from: the nearest of its process's parents that's a running session's
    /// own, else the session its process's environment names (`board::run_started_by`). Looked
    /// up while it runs, a run's ancestry and environment stay the same, and it's kept once the
    /// run ends and has no process to look at, while it's on the board; one that's still listed
    /// once its process has ended is written down for when Codealong starts again
    /// (`keep_run_origins`).
    fn with_run_origins(&self, mut sessions: Vec<Session>) -> Vec<Session> {
        let mut known = self.run_origins.lock().unwrap_or_else(|e| e.into_inner());
        if !sessions.iter().any(|s| s.headless) {
            known.clear();
            return sessions;
        }
        let by_pid: HashMap<u32, &str> = sessions.iter().filter(|s| s.live && !s.headless).filter_map(|s| Some((s.pid?, s.key.as_str()))).collect();
        let by_id: HashMap<(Harness, &str), &str> = sessions.iter().filter(|s| !s.headless).map(|s| ((s.harness, s.id.as_str()), s.key.as_str())).collect();
        let under = |pid: u32| procs::environment(pid).map(|env| harness::started_under(&env)).unwrap_or_default();
        let found: Vec<(String, String)> = sessions
            .iter()
            .filter(|s| s.headless && s.live && s.origin.is_none() && !known.contains_key(&s.key))
            .filter_map(|s| Some((s.key.clone(), board::run_started_by(s.pid?, procs::parent_pid, under, &by_pid, &by_id)?.to_string())))
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
    /// (`start::limit_moves`), and whether Codealong tells it to carry on once the limit lifts
    /// (reconnect.rs): when config.toml says to, it's still open where it stopped
    /// (`reconnect::open_where_it_stopped`), and its harness said when it lifts.
    fn with_limits(&self, mut sessions: Vec<Session>, usage: &[Usage], now: i64) -> Vec<Session> {
        if sessions.iter().any(|s| s.limit.is_some()) {
            // Out of a boundary group only where the sign-in it would go to qualifies too.
            let cfg = &self.cfg;
            let qualifies = |name: &str| cfg.accounts.iter().find(|a| a.name == name).is_some_and(|a| crate::signin::current(a, now, crate::signin::Ask::Never).qualifies());
            let home = crate::util::home();
            crate::start::limit_moves(&mut sessions, &cfg.accounts, cfg.routing.buffer, usage, |a| crate::start::places_of(a, &home).is_ok(), |from, to| crate::boundary::may_send(cfg, from, to, qualifies(to)), now);
        }
        let on = self.cfg.limits.continue_when_lifted;
        for s in &mut sessions {
            let open = crate::reconnect::open_where_it_stopped(s) && !self.cfg.sessions.blocks(s);
            if let Some(limit) = s.limit.as_mut() {
                limit.continues = on && open && limit.resets_at.is_some();
            }
        }
        sessions
    }

    /// Sessions a lost connection stopped, each saying whether Codealong tells it to carry on
    /// once its API can be reached (reconnect.rs): when config.toml says to, it runs, and its
    /// harness says where its API is.
    fn with_connections(&self, mut sessions: Vec<Session>) -> Vec<Session> {
        let on = self.cfg.sessions.continue_after_network_error;
        for s in &mut sessions {
            let live = s.live && !self.cfg.sessions.blocks(s);
            if let Some(lost) = s.connection_lost.as_mut() {
                lost.continues = on && live && harness::adapter(s.harness).api_address().is_some();
            }
        }
        sessions
    }

    /// Sessions whose account's sign-in was turned down, each saying whether Codealong tells it to
    /// carry on once the account reads signed in again (reconnect.rs): when config.toml says to,
    /// it's still open where it stopped (a process that takes up the new sign-in by itself), the
    /// refusal is recent enough to (`reconnect::MAX_AGE_MS`), and the account is on, its usage
    /// read. One at work again (Codealong's own, whose turn shows before its transcript has the
    /// prompt) is past it.
    fn with_sign_ins(&self, mut sessions: Vec<Session>, now: i64) -> Vec<Session> {
        let on = self.cfg.sessions.continue_after_sign_in;
        for s in &mut sessions {
            if s.status == Status::Working {
                s.signed_out = None;
            }
            let open = crate::reconnect::open_where_it_stopped(s) && !self.cfg.sessions.blocks(s);
            let read = self.cfg.accounts.iter().any(|a| a.name == s.account && !a.disabled);
            if let Some(out) = s.signed_out.as_mut() {
                out.continues = on && open && read && now - out.at <= crate::reconnect::MAX_AGE_MS;
            }
        }
        sessions
    }

    /// Sessions with quiet, helpers', automatic reviews' and headless runs' rules, their agents'
    /// marks of seen, what their side conversations answered, whether they're set aside for them,
    /// their marks of unread, their snoozes, their moves to the top and their labels applied
    /// (board::apply_quiet, board::apply_helper, board::apply_round_to_come,
    /// board::apply_headless, board::apply_agent_seen,
    /// board::apply_side_answer, board::apply_aside, board::apply_unread, board::apply_snooze,
    /// board::apply_raise, board::apply_labels). A snooze's time coming changes nothing on disk: the 30-second
    /// heartbeat's publish brings it back.
    pub(super) fn with_snoozes(&self, mut sessions: Vec<Session>, sides: &std::collections::BTreeMap<String, Vec<crate::side::Glance>>, listenings: &std::collections::BTreeMap<String, crate::listen::Glance>, now: i64) -> Vec<Session> {
        let unattended = sessions.iter().any(|s| s.helper.is_some() || s.headless) || !listenings.is_empty();
        if self.seen.snoozed.is_empty() && self.seen.unread.is_empty() && self.seen.quiet.is_empty() && self.seen.raised.is_empty() && self.seen.agent_seen.is_empty() && self.seen.labels.is_empty() && sides.is_empty() && !unattended {
            return sessions;
        }
        // The commits of the PRs whose next round here is of newer commits than their last
        // round's, as the board last published says (`AutoReview::behind`, worked out with the
        // PRs and their settings). A round makes way by that board, so it says so by then; a
        // one-shot snapshot publishes none, and shows such a review waiting on you.
        let behind: HashMap<String, String> = self.board.read().map(|b| b.prs.iter().filter(|p| p.auto_review.as_ref().is_some_and(|a| a.behind && a.machine.is_none())).filter_map(|p| Some((crate::prlinks::pr_key(&p.key), p.head_oid.clone()?))).collect()).unwrap_or_default();
        let to_come = autoreview::to_come(&self.reviews, |pr| behind.get(pr).cloned());
        for s in &mut sessions {
            // Quiet and a helper's or headless run's rule first, so that what you do to it
            // yourself after (a side conversation you asked, a mark of unread, a snooze that
            // ends) still brings it back.
            board::apply_quiet(s, self.seen.is_quiet(&s.key));
            board::apply_helper(s);
            board::apply_round_to_come(s, to_come.contains(s.key.as_str()));
            board::apply_headless(s);
            board::apply_listening(s, listenings.contains_key(&s.key));
            board::apply_agent_seen(s, self.seen.agent_seen_of(&s.key));
            board::apply_side_answer(s, sides.get(&s.key).map_or(&[], Vec::as_slice), self.seen.seen_at(&s.key));
            board::apply_aside(s, self.seen.aside_of(&s.key), sides.get(&s.key).map_or(&[], Vec::as_slice));
            // A mark of unread next: a snooze keeps a session marked unread off the bar too.
            board::apply_unread(s, self.seen.unread_of(&s.key), self.seen.seen_at(&s.key));
            board::apply_snooze(s, self.seen.snooze_of(&s.key), self.seen.seen_at(&s.key), now);
            board::apply_raise(s, self.seen.raise_of(&s.key));
            board::settle_agent_seen(s);
            board::apply_labels(s, self.seen.labels_of(&s.key), &self.todos.labels);
        }
        sessions
    }

    pub(super) fn build_sessions(&self, now: i64) -> Vec<Session> {
        let look = harness::Look { now, ..Self::look(&self.seen, &self.processes) };
        let mut out = Vec::new();
        // In config.toml's order of accounts, each read by its harness's reader.
        for a in &self.cfg.accounts {
            let Some(h) = harness::of(a).map(|x| x.harness()) else { continue };
            if let Some(r) = self.readers.iter().find(|r| r.harness() == h) {
                out.extend(r.sessions(a, &look));
            }
        }
        // What only the process knows: about the sessions Codealong runs, the modes it pressed
        // sessions in terminals to (cycle.rs), the permission requests Claude Code's hook holds
        // here, and the commands agents ask to run in your terminal (terminal.rs).
        for s in &mut out {
            crate::acp::overlay(s, self.seen.seen_at(&s.key));
            crate::cycle::overlay(s);
            crate::claude::asks::overlay(s);
            crate::terminal::overlay(s);
        }
        out
    }

    /// The board's sessions: what the harnesses' records say (`build_sessions`), with all the
    /// engine knows besides laid over it (`laid_over`).
    pub(super) fn sessions(&self, now: i64, usage: &[Usage], sides: &std::collections::BTreeMap<String, Vec<crate::side::Glance>>, listenings: &std::collections::BTreeMap<String, crate::listen::Glance>) -> Vec<Session> {
        self.laid_over(self.build_sessions(now), now, usage, sides, listenings)
    }

    /// `built` with what the engine knows of each session laid over what its records say, a
    /// stage at a time in this order, and what's worked out from the whole settled last
    /// (`settled`). The order is the rule where two stages speak of one thing: a later one has
    /// the last word. So what Jev reads of a session comes before what terminal.rs knows it
    /// waits on, which Jev can only guess at; your own marks (seen, unread, snoozed, raised)
    /// come after both, over whatever a turn is doing; and a usage limit, a lost connection and a
    /// sign-in turned down are read off where all of that left the session.
    fn laid_over(&self, built: Vec<Session>, now: i64, usage: &[Usage], sides: &std::collections::BTreeMap<String, Vec<crate::side::Glance>>, listenings: &std::collections::BTreeMap<String, crate::listen::Glance>) -> Vec<Session> {
        let mut sessions = with_terminals(built);
        crate::worktree::attach_own(&mut sessions);
        let sessions = self.with_origins(sessions);
        let sessions = self.with_chatter(sessions, now);
        let sessions = self.with_run_origins(sessions);
        let sessions = self.with_chrome(sessions);
        let sessions = self.with_artifacts(sessions);
        let sessions = self.with_emblems(sessions);
        // An orchestrator's session is known as one before Jev reads it, and quieted after
        // (board.rs `quiet_orchestrator`), so its lane and the board go by what it waits on.
        let sessions = self.with_orchestrators(sessions);
        let mut sessions = self.with_insights(sessions, now);
        sessions.iter_mut().for_each(board::quiet_orchestrator);
        let sessions = with_runs(sessions, now);
        let sessions = self.with_starts(sessions, now);
        let sessions = self.with_suggestions(sessions);
        let sessions = self.with_snoozes(sessions, sides, listenings, now);
        let sessions = self.with_limits(sessions, usage, now);
        let sessions = self.with_connections(sessions);
        let sessions = self.with_sign_ins(sessions, now);
        self.settled(sessions)
    }

    /// What's worked out from a session as a whole, once nothing more is laid over it: why it
    /// needs you (`Session::settle_need`), and whether the turn it's taking can be stopped or
    /// take a message now (harness.rs `stoppable`, `steerable`). Settled any earlier, these
    /// spoke of the session a later stage went on to change: one Jev read as working on a
    /// command gone quiet, not stuck, showed as working and took no message.
    fn settled(&self, mut sessions: Vec<Session>) -> Vec<Session> {
        for s in &mut sessions {
            s.settle_need();
            let account = self.cfg.accounts.iter().find(|a| a.name == s.account);
            s.stoppable = crate::harness::stoppable(s, account);
            s.steerable = crate::harness::steerable(s, account);
        }
        sessions
    }
}

#[cfg(test)]
mod tests {
    /// A session its sign-in stopped is carried on only where reconnect.rs will: with the setting
    /// on, open where it stopped, on an account that's on (its usage read), in the day after; and
    /// one at work again is past it.
    #[test]
    fn a_sign_in_is_carried_on_only_where_codealong_will() {
        use super::*;
        use crate::model::{SignedOut, Status};
        const NOW: i64 = 1_800_000_000_000;
        let (tx, _rx) = channel();
        let mut e = Engine::new(Box::new(|_| {}), tx, Arc::new(RwLock::new(Board::default())));
        let account = |disabled: bool| config::AccountCfg { name: "claude".into(), harness: "claude".into(), home: "~/.claude".into(), command: None, start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled, chrome_profile: None };
        e.cfg.accounts = vec![account(false)];
        let out = |at: i64| crate::model::Session { status: Status::Unread, signed_out: Some(SignedOut { at, said: "OAuth token revoked · Please run /login".into(), continues: false }), ..crate::fixtures::session("a") };
        let continues = |e: &Engine, s: crate::model::Session| e.with_sign_ins(vec![s], NOW).remove(0).signed_out.map(|o| o.continues);
        assert_eq!(continues(&e, out(NOW - 60_000)), Some(true));
        assert_eq!(continues(&e, out(NOW - crate::reconnect::MAX_AGE_MS - 1)), Some(false), "too long ago");
        assert_eq!(continues(&e, crate::model::Session { live: false, ..out(NOW - 60_000) }), Some(false), "its terminal closed");
        assert_eq!(continues(&e, crate::model::Session { status: Status::Working, ..out(NOW - 60_000) }), None, "at work again");
        e.cfg.accounts = vec![account(true)];
        assert_eq!(continues(&e, out(NOW - 60_000)), Some(false), "its account is off");
        e.cfg.accounts = vec![account(false)];
        e.cfg.sessions.continue_after_sign_in = false;
        assert_eq!(continues(&e, out(NOW - 60_000)), Some(false), "the setting is off");
    }

    /// What's worked out from a session is worked out from how every stage left it. A session
    /// gone quiet on a command is stuck as its records have it, and takes no message; once Jev
    /// reads the command as one that waits on nobody, it's working, and takes one as any working
    /// session does. Settled as the sessions were built, it showed as working and took none.
    #[test]
    fn what_is_worked_out_from_a_session_is_settled_after_all_thats_laid_over_it() {
        use super::*;
        use crate::model::{Need, SessionInsight, Surface};

        // A Codex app thread takes a message mid-turn while the app's socket is there to ask.
        let home = crate::fixtures::scratch("settled");
        std::fs::create_dir_all(home.join("ipc")).unwrap();
        std::fs::write(home.join("ipc/ipc.sock"), "").unwrap();
        let (tx, _rx) = channel();
        let mut engine = Engine::new(Box::new(|_| {}), tx, Arc::new(RwLock::new(Board::default())));
        engine.cfg.accounts = vec![serde_json::from_value(serde_json::json!({ "name": "kestrel", "harness": "codex", "home": home.to_string_lossy() })).unwrap()];
        engine.cfg.decisions.sessions = true;
        let now = now_ms();
        let stuck = Session {
            surface: Surface::App,
            live: true,
            status: Status::Needs,
            status_since: Some(now - 600_000),
            stalled_since: Some(now - 600_000),
            excerpt: Some("Stuck on `cargo build` for {for:1}".into()),
            last_activity: now - 600_000,
            ..Session::new(Harness::Codex, "kestrel", "t1")
        };
        let laid = |e: &Engine| e.laid_over(vec![stuck.clone()], now, &[], &Default::default(), &Default::default()).remove(0);
        let as_recorded = laid(&engine);
        assert_eq!((as_recorded.status, as_recorded.need, as_recorded.steerable), (Status::Needs, Some(Need::Stuck), false));

        let (_, signature) = crate::insights::ask(&stuck, now).expect("something to ask Jev of it");
        engine.insights.answered(stuck.key.clone(), signature, SessionInsight { waits_for_input: Some(0.0), ..Default::default() }, now);
        let as_read = laid(&engine);
        assert_eq!((as_read.status, as_read.need, as_read.stalled_since), (Status::Working, None, None));
        assert!(as_read.steerable, "worked out after Jev's read of it, not before");
    }
}
