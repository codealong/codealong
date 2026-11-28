//! Unified session handover when a session moves to another account, harness, or worktree
//! (`Msg::HandOver` for synchronous moves and `finish_pending_handover` for async-ID handoffs).
//!
//! Every session-keyed store is transferred in one place so a move never strands state on the
//! archived source: todos (links + primary), `SeenState` (pin slot order, labels, quiet, own,
//! talking, active snooze, archive), side conversations (`sides.json`), Kanban/Canvas cards
//! (`boards.json`), held outbox messages (`held-messages.json`, fenced against permission
//! escalation), Codealong terminals (`terminals.rs`), and schedules (`schedules/runner.rs`).

use super::*;

/// Whether moving from `from` to `copy` escalates permissions from non-bypass to bypass
/// (`ModeKind::Skips`), used to fence pre-move untrusted agent messages in the outbox.
pub(super) fn escalates_permissions(board: &Board, from: &str, copy: &str) -> bool {
    let skips = |key: &str| board.session(key).and_then(|(_, s)| s.mode.as_ref()).is_some_and(|m| m.kind == ModeKind::Skips);
    !skips(from) && skips(copy)
}

impl Engine {
    /// Transfers all in-engine state (todos, `SeenState`, archive) from `from` to `copy`. Pure
    /// with respect to global file stores so unit tests can verify handover without touching disk.
    pub(super) fn hand_over_engine_state(&mut self, lanes: &todos::LaneIndex, from: &str, copy: &str, was_pinned: bool, only_here: bool, now: i64) -> (bool, bool) {
        let mut todos_changed = false;
        let matching: Vec<(String, bool)> = self
            .todos
            .todos
            .iter()
            .filter(|t| t.sessions.iter().any(|s| s == from))
            .map(|t| (t.id.clone(), t.primary.as_ref().is_some_and(|p| p.session == from)))
            .collect();
        for (id, is_primary) in matching {
            let op = if is_primary {
                todos::TodoOp::Primary { id, session: Some(copy.to_string()) }
            } else {
                todos::TodoOp::Link { id, worktree: None, session: Some(copy.to_string()), linked: true }
            };
            if self.todos.apply(op, lanes, now).unwrap_or(false) {
                todos_changed = true;
            }
        }
        let _ = self.seen.hand_over(from, copy, was_pinned, now);
        self.seen.set_archived(from, true, now, only_here);
        (todos_changed, true)
    }

    /// Single authoritative handover for both synchronous moves (`Handle::hand_over`) and async
    /// resolved handoffs (`finish_pending_handover`): moves every session-keyed store from `from`
    /// to `copy`, records the move leg in `origins.json`, and archives `from`.
    pub(super) fn hand_over_session(&mut self, board: &Board, lanes: &todos::LaneIndex, from: &str, copy: &str, was_pinned: bool, only_here: bool) {
        if from.is_empty() || copy.is_empty() || from == copy {
            return;
        }
        let now = now_ms();
        let seen_through = board.session(from).map(|(_, s)| s.last_activity);
        let escalates = escalates_permissions(board, from, copy);
        crate::origins::record_move(from, copy, seen_through);
        let (todos_changed, state_changed) = self.hand_over_engine_state(lanes, from, copy, was_pinned, only_here, now);
        if todos_changed {
            self.save_todos();
        }
        if state_changed {
            self.save_state();
        }
        // Terminals must follow before any close of `from`.
        crate::terminals::follow(from, copy);
        crate::side::follow(from, copy);
        crate::boards::follow(from, copy);
        crate::outbox::follow(from, copy, escalates);
        if let Err(e) = crate::schedules::runner::copy_takes_place(board, from, copy) {
            eprintln!("Moved {from}, but couldn't send its schedules' runs to {copy}: {e:#}");
        }

        // Refresh archive cache on the source harness if it tracks archives natively.
        let look = Self::look(&self.seen, &self.processes);
        if let Some(h) = from.split(':').next().and_then(harness::named).map(|a| a.harness()) {
            for r in self.readers.iter_mut().filter(|r| r.harness() == h) {
                r.archives_changed(&harness::accounts_of(&self.cfg, r.harness()), &look);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ModeKind, SessionMode};

    #[test]
    fn hand_over_engine_state_transfers_todos_and_all_seen_marks_and_archives_source() {
        let (tx, _rx) = channel();
        let mut engine = Engine::new(Box::new(|_| {}), tx, Arc::new(RwLock::new(Board::default())));
        let from = "claude:claude2:old";
        let copy = "claude:claude:new";
        let now = 1_800_000_000_000;

        // Populate every per-session mark on `from` plus a todo owned by `from`.
        engine.seen.set_pinned(vec!["claude:claude:first".into(), from.into(), "claude:claude:last".into()]);
        engine.seen.label(from, &["l-release".into()], &[]);
        engine.seen.set_quiet(from, true);
        engine.seen.make_own(from);
        engine.seen.set_talking(from, true);
        engine.seen.snooze(from, now - 10_000, now + 60_000);
        let todo: crate::todos::Todo = serde_json::from_value(serde_json::json!({
            "id": "t-1",
            "project": "Northwind",
            "title": "Ship release",
            "sessions": [from],
            "primary": { "session": from, "since": now - 5_000 }
        }))
        .unwrap();
        engine.todos.todos.push(todo);

        let lanes = todos::LaneIndex::of(&[]);
        let (todos_changed, state_changed) = engine.hand_over_engine_state(&lanes, from, copy, false, false, now);
        assert!(todos_changed && state_changed);
        // Pin slot order is preserved in place and `from` is archived.
        assert_eq!(engine.seen.pinned, vec!["claude:claude:first", copy, "claude:claude:last"]);
        assert!(engine.seen.archived_at(from).is_some());
        assert_eq!(engine.seen.labels_of(copy), &["l-release".to_string()]);
        assert!(engine.seen.is_quiet(copy));
        assert!(engine.seen.is_own(copy));
        assert!(engine.seen.is_talking(copy));
        assert!(engine.seen.snoozed.iter().any(|z| z.key == copy && z.until == now + 60_000));
        // Todo primary ownership transferred to `copy`, keeping `from` in history.
        let t = &engine.todos.todos[0];
        assert_eq!(t.primary.as_ref().map(|p| p.session.as_str()), Some(copy));
        assert!(t.sessions.iter().any(|s| s == copy) && t.sessions.iter().any(|s| s == from));
    }

    #[test]
    fn escalates_permissions_detects_non_bypass_to_bypass_transitions() {
        let with_mode = |key: &str, kind: ModeKind| Session {
            key: key.into(),
            mode: Some(SessionMode { kind, label: String::new(), detail: String::new(), id: String::new(), expected: false }),
            ..Session::new(Harness::Claude, "claude", key)
        };
        let board = Board {
            lanes: vec![crate::fixtures::lane(
                "/r",
                vec![
                    with_mode("a", ModeKind::Asks),
                    with_mode("b", ModeKind::Skips),
                    with_mode("c", ModeKind::Skips),
                    with_mode("d", ModeKind::Edits),
                ],
            )],
            ..Board::default()
        };
        assert!(escalates_permissions(&board, "a", "b"));
        assert!(!escalates_permissions(&board, "b", "c"));
        assert!(!escalates_permissions(&board, "b", "d"));
        assert!(!escalates_permissions(&board, "a", "d"));
    }
}
