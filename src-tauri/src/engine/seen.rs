//! state.json (`SeenState`: what you've seen, pinned, archived and snoozed) as the engine holds
//! it: read, saved, and never written over when it doesn't read. And what's done to sessions'
//! marks that takes more than setting one: moving sessions to the top (`on_raise`), labelling
//! them (`on_label`), and forgetting the marks, tags and suggested replies their sessions have
//! moved past, which `publish` does each time it has composed a board, before that board is
//! compared with the last one: whether or not it then goes out.

use super::*;

impl Engine {
    /// Forgets the tags on sessions that have moved on since: something was sent to them since
    /// they were read, or a turn of theirs started since they were tagged (`tags::moved_on`),
    /// keeping which for `codealong changes` to say.
    pub(super) fn forget_stale_tags(&self, board: &Board) {
        let store = tags::current();
        if store.sessions.is_empty() {
            return;
        }
        let stale: Vec<_> = store.sessions.iter().filter_map(|(key, t)| board.session(key).and_then(|(_, s)| tags::moved_on(s, t)).map(|moved| (key.clone(), t.at, moved))).collect();
        if stale.is_empty() {
            return;
        }
        if let Err(e) = tags::forget(&stale) {
            eprintln!("Couldn't forget the tags on sessions that moved on: {e:#}");
        }
    }

    /// Forgets the marks of seen from sessions' agents whose sessions have moved on since
    /// (`board::agent_seen_spent`), so the turn a later one ends with is never taken for the one
    /// its agent marked, whatever its record keeps of how it started.
    /// Forgets sessions set aside that no longer wait on their side conversation
    /// (`board::aside_holds`): the board shows them as it shows any, and one asked on the side
    /// again later isn't set aside unless you set it aside again.
    pub(super) fn forget_spent_aside(&mut self, board: &Board) {
        let spent: Vec<String> = self.seen.aside.iter().filter(|m| board.session(&m.key).is_some_and(|(_, s)| s.aside.is_none())).map(|m| m.key.clone()).collect();
        if spent.is_empty() {
            return;
        }
        for k in &spent {
            self.seen.forget_aside(k);
        }
        self.save_state();
    }

    /// Takes away the snoozes of sessions a turn of their own has left waiting on you
    /// (`board::snooze_called_back`), which the board already shows awake: for good, so that
    /// once you've answered, the snooze doesn't take the session back off your list, nor bring a
    /// quiet one's next report back as Your turn when its time comes.
    pub(super) fn wake_called_back(&mut self, board: &Board) {
        let now = now_ms();
        let called: Vec<String> = self.seen.snoozed.iter().filter(|z| z.until > now && board.session(&z.key).is_some_and(|(_, s)| board::snooze_called_back(s, z.at))).map(|z| z.key.clone()).collect();
        if called.is_empty() {
            return;
        }
        for k in &called {
            self.seen.forget_snooze(k);
        }
        self.save_state();
    }

    /// Wakes the sessions snoozed until their accounts have room whose room is back
    /// (`board::snoozes_with_room`), as their time coming would. True when any woke, for the
    /// board to be composed again with them back.
    pub(super) fn wake_with_room(&mut self, board: &Board) -> bool {
        let now = now_ms();
        let back = board::snoozes_with_room(board, &self.seen.snoozed, now);
        if back.is_empty() {
            return false;
        }
        for k in &back {
            self.seen.wake(k, now);
        }
        self.save_state();
        true
    }

    pub(super) fn forget_spent_agent_seen(&mut self, board: &Board) {
        let spent: Vec<String> = self.seen.agent_seen.iter().filter(|m| board.session(&m.key).is_some_and(|(_, s)| board::agent_seen_spent(s, m.seen.at))).map(|m| m.key.clone()).collect();
        if spent.is_empty() {
            return;
        }
        for k in &spent {
            self.seen.forget_agent_seen(k);
        }
        self.save_state();
    }

    /// Forgets the replies suggested for sessions that have moved on since: something was sent to
    /// them since they were read, or a turn of theirs started since they were suggested
    /// (`suggestions::moved_on`), keeping which for `codealong changes` to say. A turn that
    /// started and then ended would hold them again by `board::suggestions_hold` alone, so
    /// they're forgotten as soon as it's seen.
    pub(super) fn forget_stale_suggestions(&self, board: &Board) {
        let store = suggestions::current();
        if store.sessions.is_empty() {
            return;
        }
        let stale: Vec<_> = store.sessions.iter().filter_map(|(key, g)| board.session(key).and_then(|(_, s)| suggestions::moved_on(s, g)).map(|moved| (key.clone(), g.at, moved))).collect();
        if stale.is_empty() {
            return;
        }
        if let Err(e) = suggestions::forget(&stale) {
            eprintln!("Couldn't forget the replies suggested for sessions that moved on: {e:#}");
        }
    }

    /// Moves sessions to the top, as `raise` asks (`Msg::Raise`), or back to their places. An
    /// agent's move either way is turned down, before any is made, where `board::raise_refused`
    /// says, and one of yours wakes a snooze. True when anything changed.
    pub(super) fn on_raise(&mut self, keys: &[String], raised: bool, by: Raised, now: i64) -> Result<bool, String> {
        if by.agent {
            let b = self.board.read().map_err(|_| "Codealong couldn't read its board.".to_string())?;
            for k in keys {
                let Some((_, s)) = b.session(k) else { return Err("Codealong doesn't list that session.".to_string()) };
                if let Some(why) = board::raise_refused(s, self.seen.raise_of(k), raised) {
                    return Err(why);
                }
            }
        }
        if !raised {
            return Ok(keys.iter().fold(false, |changed, k| self.seen.lower(k, now, !by.agent) | changed));
        }
        for k in keys {
            if !by.agent {
                self.seen.wake(k, now);
            }
            self.seen.raise(k, Raised { at: now, ..by.clone() });
        }
        Ok(!keys.is_empty())
    }

    /// Puts labels on sessions and takes others off, making a label of each name to put on that's
    /// none yet. True when that changed the labels or what a session wears.
    pub(super) fn on_label(&mut self, keys: &[String], add: &[String], remove: &[String]) -> std::result::Result<bool, String> {
        let mut made = false;
        let add = if add.is_empty() {
            Vec::new()
        } else {
            // Under the lock the command takes too, against the file as it is now, as a todo's
            // change is (`change_todos`).
            let lanes = self.lane_index();
            let _lock = self.lock_todos()?;
            self.reload_todos();
            if self.todos_unreadable {
                return Err(self.trouble(Trouble::Todos).cloned().unwrap_or_default());
            }
            let (labels, ids) = todos::named(&self.todos.labels, add, now_ms());
            if labels != self.todos.labels {
                made = self.todos.apply(TodoOp::Labels { labels }, &lanes, now_ms()).map_err(|e| format!("{e:#}"))?;
                self.save_todos();
            }
            ids
        };
        let remove = todos::named_ids(&self.todos.labels, remove);
        Ok(keys.iter().fold(made, |changed, k| self.seen.label(k, &add, &remove) | changed))
    }

    /// state.json, read again. One that doesn't read is left alone: the engine goes on with what
    /// it has in memory, a first run's state at launch, and says so (`notices`).
    pub(super) fn load_state(&mut self) {
        let read = self.read_or_warn(Trouble::State, SeenState::load());
        self.state_unreadable = read.is_none();
        match read {
            Some(state) => self.seen = state,
            None => {
                if self.seen.first_run == 0 {
                    self.seen = SeenState::fresh();
                }
                if let Some(why) = self.troubles.get_mut(&Trouble::State) {
                    why.push_str(" What you pin, archive, snooze and mark seen meanwhile lasts only until Codealong quits.");
                }
            }
        }
    }

    pub(super) fn save_state(&mut self) {
        // A file that didn't read is left for you to fix, not replaced with what's in memory.
        if self.state_unreadable {
            return;
        }
        let warning = self.seen.save().err().map(|e| format!("Couldn't save state.json: {e:#}"));
        if warning.as_ref() != self.trouble(Trouble::State) {
            self.set_trouble(Trouble::State, warning);
            self.mark_dirty("state.json trouble");
        }
    }
}

#[cfg(test)]
mod tests {
    /// state.json holds what you pinned, archived, snoozed and saw. One that doesn't read, as
    /// when an older build meets a newer one's shape, is never written over: the engine runs on a
    /// first run's state in memory, says so, and takes the file up again once it reads. A save
    /// that fails is said too. Runs in a copy of this test binary with a home of its own, as a
    /// scan reads and writes there.
    #[test]
    #[cfg(unix)]
    fn a_state_file_that_does_not_read_is_never_written_over() {
        use super::*;
        use std::os::unix::fs::PermissionsExt;

        let Some(own) = std::env::var_os("CODEALONG_TEST_HOME") else {
            let home = std::env::temp_dir().join(format!("codealong-state-scan-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&home);
            std::fs::create_dir_all(&home).unwrap();
            let run = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["engine::seen::tests::a_state_file_that_does_not_read_is_never_written_over", "--exact", "--nocapture"])
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

    /// A snooze a turn of the session's own called back (`board::snooze_called_back`) is taken
    /// away for good: once you've answered, the session back at work isn't snoozed again.
    #[test]
    fn a_called_back_snooze_is_taken_away() {
        use super::*;

        let Some(own) = std::env::var_os("CODEALONG_TEST_HOME") else {
            let home = std::env::temp_dir().join(format!("codealong-called-back-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&home);
            std::fs::create_dir_all(&home).unwrap();
            let run = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["engine::seen::tests::a_called_back_snooze_is_taken_away", "--exact", "--nocapture"])
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
        const MIN: i64 = 60_000;
        let now = now_ms();
        let key = "claude:claude:a";
        e.seen.snooze(key, now - 10 * MIN, now + 60 * MIN);
        // A scheduled wakeup five minutes ago, its turn ended a minute ago.
        let after_wakeup = |status| Session { status, status_since: Some(now - MIN), input_at: Some(now - 5 * MIN), ..crate::fixtures::session(key) };
        let on_board = |s| Board { lanes: vec![crate::fixtures::lane("/a", vec![s])], ..Board::default() };
        e.wake_called_back(&on_board(after_wakeup(Status::Unread)));
        assert!(e.seen.snooze_of(key).is_some(), "a report keeps it");
        e.wake_called_back(&on_board(after_wakeup(Status::Needs)));
        assert!(e.seen.snooze_of(key).is_none(), "a question takes it away");
        let mut answered = after_wakeup(Status::Working);
        board::apply_snooze(&mut answered, e.seen.snooze_of(key), 0, now);
        assert_eq!(answered.snoozed_until, None, "answered, it isn't snoozed again");
        std::fs::write(home().join("ran"), "").unwrap();
    }

    /// A label put on a session by a name that's none yet is made, with the todos' labels, and one
    /// taken off by its name comes off.
    #[test]
    fn a_session_is_labelled_by_name_and_a_new_name_made_a_label() {
        use super::*;

        let Some(own) = std::env::var_os("CODEALONG_TEST_HOME") else {
            let home = std::env::temp_dir().join(format!("codealong-session-labels-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&home);
            std::fs::create_dir_all(&home).unwrap();
            let run = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["engine::seen::tests::a_session_is_labelled_by_name_and_a_new_name_made_a_label", "--exact", "--nocapture"])
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
        e.change_todos(TodoOp::Labels { labels: vec![todos::Label { id: "l1".into(), name: "Bug".into(), color: "red".into() }] }).unwrap();
        let key = "claude:claude:a".to_string();
        assert_eq!(e.on_label(std::slice::from_ref(&key), &["bug".into(), "Release".into()], &[]), Ok(true));
        let release = e.todos.labels.iter().find(|l| l.name == "Release").expect("made").id.clone();
        assert_eq!(e.seen.labels_of(&key), ["l1".to_string(), release.clone()].as_slice());
        assert!(std::fs::read_to_string(todos::path()).unwrap().contains("Release"), "kept with the todos' labels");
        assert_eq!(e.on_label(std::slice::from_ref(&key), &["Bug".into()], &[]), Ok(false), "nothing new");
        assert_eq!(e.on_label(std::slice::from_ref(&key), &[], &["BUG".into()]), Ok(true));
        assert_eq!(e.seen.labels_of(&key), [release].as_slice());
        let s = e.with_snoozes(vec![crate::fixtures::session(&key)], &Default::default(), &Default::default(), now_ms());
        assert_eq!(s[0].labels.iter().map(|l| l.name.as_str()).collect::<Vec<_>>(), ["Release"]);
        std::fs::write(home().join("ran"), "").unwrap();
    }
}
