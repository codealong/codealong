//! Moving a session its account's usage limit stopped, when config.toml says to (`[limits]
//! when_hit = "move"`). Every board the engine publishes is looked at for sessions that have just
//! stopped at a limit and have an account to go on in (`SessionLimit::move_to`, from
//! `start::limit_moves`), and each is moved there once, as Move to in its menu moves it
//! (actions.rs `session-move`), on a thread of its own: a move opens a terminal, which can take a
//! minute. Left at `suggest`, nothing here moves anything, and the windows offer the move.

use std::collections::HashSet;

use crate::config::WhenLimited;
use crate::engine::Handle;
use crate::model::Board;

/// A limit hit longer ago than this was hit before Codealong was watching, or was offered and
/// left as it was: it's offered, not moved.
const FRESH_MS: i64 = 10 * 60_000;

/// What has been moved, so each limit is moved once.
#[derive(Default)]
pub struct Mover {
    /// Each limit moved or tried, as its session and when it was hit: a move that failed isn't
    /// tried again, and a copy stopped at the account it went to is a limit of its own.
    tried: HashSet<(String, i64)>,
}

impl Mover {
    /// The sessions to move now, with the account each goes to: those running here that a limit
    /// stopped in the last few minutes, with an account to go to, not moved before. One that was
    /// moved here from another account is only offered a move: each copy is a session of its
    /// own, so two accounts that each look to have room again could pass it back and forth.
    pub fn due(&mut self, board: &Board, now: i64) -> Vec<(String, String)> {
        if board.when_limited != WhenLimited::Move {
            return Vec::new();
        }
        let moved_here = |s: &crate::model::Session| s.origin.as_ref().is_some_and(|o| o.key.split(':').nth(1) != Some(s.account.as_str()));
        let mut out = Vec::new();
        for s in board.lanes.iter().flat_map(|l| &l.sessions).filter(|s| s.visitor.is_none() && !s.archived && s.live && !moved_here(s)) {
            if board.automation_blocked(&s.key) { continue; }
            let Some(limit) = &s.limit else { continue };
            let Some(to) = &limit.move_to else { continue };
            if now - limit.at <= FRESH_MS && self.tried.insert((s.key.clone(), limit.at)) {
                out.push((s.key.clone(), to.clone()));
            }
        }
        out
    }
}

/// Moves one session, as Move to does from a window, and tells the engine how it went, which
/// says so in a notice when it didn't.
pub fn move_now(engine: &Handle, key: &str, to: &str) {
    if engine.read(|b| b.automation_blocked(key)) { return; }
    let moved = crate::actions::local(engine, &crate::actions::From::Here, crate::actions::MOVE, key, serde_json::json!({ "account": to }));
    let title = engine.read(|b| b.session(key).map(|(_, s)| s.title.clone())).unwrap_or_else(|| key.to_string());
    engine.limit_moved(moved.map(|_| ()).map_err(|e| format!("“{title}” stopped at its usage limit, and moving it to {to} didn't work: {e}")));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::SessionLimit;

    const NOW: i64 = 1_000_000_000;

    fn board(when: WhenLimited, sessions: Vec<crate::model::Session>) -> Board {
        Board { when_limited: when, lanes: vec![crate::fixtures::lane("/work", sessions)], ..Board::default() }
    }

    fn limited(key: &str, at: i64, to: Option<&str>) -> crate::model::Session {
        let limit = SessionLimit { at, move_to: to.map(str::to_string), ..SessionLimit::default() };
        crate::model::Session { live: true, limit: Some(limit), ..crate::fixtures::session(key) }
    }

    #[test]
    fn provider_flags_block_automatic_account_moves() {
        let mut s = limited("claude:claude:a", NOW - 1_000, Some("claude2"));
        s.flagged = true;
        let mut b = board(WhenLimited::Move, vec![s]);
        let mut mover = Mover::default();
        assert!(mover.due(&b, NOW).is_empty());
        b.sessions.block_flagged_sessions = false;
        assert_eq!(mover.due(&b, NOW).len(), 1);
    }

    #[test]
    fn moves_a_session_just_stopped_once_and_only_when_asked_to() {
        let sessions = vec![limited("claude:claude:a", NOW - 60_000, Some("claude2")), limited("claude:claude:b", NOW - 60_000, None)];
        assert!(Mover::default().due(&board(WhenLimited::Suggest, sessions.clone()), NOW).is_empty());
        let mut mover = Mover::default();
        let b = board(WhenLimited::Move, sessions);
        // b has nowhere to go.
        assert_eq!(mover.due(&b, NOW), vec![("claude:claude:a".to_string(), "claude2".to_string())]);
        assert!(mover.due(&b, NOW).is_empty(), "moved once");
        // Stopped again later, it's a new limit.
        let again = board(WhenLimited::Move, vec![limited("claude:claude:a", NOW, Some("claude2"))]);
        assert_eq!(mover.due(&again, NOW).len(), 1);
    }

    #[test]
    fn leaves_a_limit_from_before_and_a_session_that_isnt_running() {
        let mut mover = Mover::default();
        let old = limited("claude:claude:a", NOW - FRESH_MS - 1, Some("claude2"));
        let closed = crate::model::Session { live: false, ..limited("claude:claude:b", NOW, Some("claude2")) };
        let archived = crate::model::Session { archived: true, ..limited("claude:claude:c", NOW, Some("claude2")) };
        assert!(mover.due(&board(WhenLimited::Move, vec![old, closed, archived]), NOW).is_empty());
    }

    #[test]
    fn a_copy_moved_here_is_offered_a_move_not_moved_again() {
        use crate::model::{OriginHow, SessionOrigin};
        let copy = |from: &str| crate::model::Session { origin: Some(SessionOrigin { key: from.into(), how: OriginHow::Fork }), ..limited("claude:claude:copy", NOW, Some("claude2")) };
        assert!(Mover::default().due(&board(WhenLimited::Move, vec![copy("claude:claude2:orig")]), NOW).is_empty());
        // A fork on its own account is no move.
        assert_eq!(Mover::default().due(&board(WhenLimited::Move, vec![copy("claude:claude:orig")]), NOW).len(), 1);
    }
}
