//! todos.json as the engine holds it: read, changed under the lock the `codealong` command takes
//! too (`change_todos`), saved, and never written over when it doesn't read. And the sessions
//! that starts from todos were waiting for, linked once a board has them (`link_pending`).

use super::*;

impl Engine {
    /// The lock on todos.json that the `codealong` command takes too. Without it a change here
    /// could write over one the command is making, so the change isn't made: whoever asked for
    /// it hears why, and the board says so until the lock is had again.
    pub(super) fn lock_todos(&mut self) -> std::result::Result<todos::Lock, String> {
        let lock = todos::lock().map_err(|e| format!("{e:#}"));
        let warning = lock.as_ref().err().cloned();
        if warning.as_ref() != self.trouble(Trouble::TodosLock) {
            self.set_trouble(Trouble::TodosLock, warning);
            self.mark_dirty("todos lock");
        }
        lock
    }

    /// A change to the todos from a window or the command, and the time of the board that
    /// shows it.
    pub(super) fn change_todos(&mut self, op: TodoOp) -> std::result::Result<i64, String> {
        let lanes = self.lane_index();
        // Under the lock the command takes too, and against the file as it is now, so
        // a change the command made a moment ago isn't written over.
        let lock = self.lock_todos()?;
        self.reload_todos();
        let changed = match self.todos_unreadable {
            // A change now would be lost when the file is fixed, so it's turned away.
            true => Err(self.trouble(Trouble::Todos).cloned().unwrap_or_default()),
            false => self.todos.apply(op, &lanes, now_ms()).map_err(|e| format!("{e:#}")),
        }?;
        if changed {
            self.save_todos();
            drop(lock);
            // Now rather than on the next tick: the window waits for it.
            self.publish_for("todos changed");
        }
        Ok(self.board.read().map(|b| b.generated_at).unwrap_or(0))
    }

    /// Links a session started from todos to each of them, or notes that it's still to show up.
    pub(super) fn todos_started(&mut self, ids: Vec<String>, lane: Option<String>, session: Option<String>, pending: Option<todos::PendingStart>) {
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

    pub(super) fn load_todos(&mut self) {
        let read = self.read_or_warn(Trouble::Todos, todos::load());
        self.todos_unreadable = read.is_none();
        if let Some((store, text)) = read {
            self.todos = store;
            self.todos_text = text;
        }
    }

    /// todos.json changed on disk. Our own writes come back through the watcher too; only
    /// someone else's edit is read again, or any change while the file didn't read.
    pub(super) fn reload_todos(&mut self) {
        let text = std::fs::read_to_string(todos::path()).unwrap_or_default();
        if text == self.todos_text && !self.todos_unreadable {
            return;
        }
        self.load_todos();
        self.mark_dirty("todos file");
    }

    pub(super) fn save_todos(&mut self) {
        self.mark_dirty("todos saved");
        // A file that didn't read is left for you to fix, not replaced.
        if self.todos_unreadable {
            return;
        }
        match todos::save(&self.todos) {
            Ok(text) => {
                self.todos_text = text;
                self.set_trouble(Trouble::Todos, None);
            }
            Err(e) => self.set_trouble(Trouble::Todos, Some(format!("Couldn't save todos.json: {e:#}"))),
        }
    }

    /// Each lane's project and repo, for linking todos.
    pub(super) fn lane_index(&self) -> todos::LaneIndex {
        self.board.read().map(|b| todos::LaneIndex::of(&b.lanes)).unwrap_or_default()
    }

    /// Links sessions that starts in the apps were waiting for, from the board just composed.
    pub(super) fn link_pending(&mut self, board: &Board) -> bool {
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
                    helper: s.helper.is_some(),
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
}

#[cfg(test)]
mod tests {
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
                .args(["engine::todofile::tests::a_todo_change_without_the_lock_is_turned_away", "--exact", "--nocapture"])
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
}
