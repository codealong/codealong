//! Mark as done from the `codealong` command, which an agent may have typed. An agent can't see
//! what you see, so it may finish only what's its own: its own session, or one it started
//! (`codealong start` run from it, `Session::origin`), never one that waits on you or is working,
//! and nothing on another Mac. Anything else is yours to mark done, in Codealong (asked
//! 2026-10-07).
//!
//! A session finishing itself runs the command in the middle of its own turn, so ending it then
//! would cut its reply short. It's held (`Waiting`) until the turn ends, and then ended as Mark as
//! done ends a session (its terminal closes, or its harness exits it), but not marked seen nor
//! archived: its last reply is for you, and stays in Your turn until you mark it done yourself.
//! A session it started is marked done at once, as from the windows, and the main window says
//! which session did it, with an Undo.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use crate::model::{Board, OriginHow, Session, Status};
use crate::util::now_ms;

/// What a session asking may mark done.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Whose {
    /// Its own session: ended once its turn ends, and left for you to read.
    Own,
    /// A session it started: marked done now.
    Started,
}

/// Whether the session `by` (the one running the command, when the command found it) may mark
/// `s` done, and as which; why not when it may not.
pub fn may(board: &Board, by: Option<&str>, s: &Session) -> Result<Whose, String> {
    let Some(by) = by else {
        return Err("Only a session can mark a session done from the command, its own or one it started, and this command isn't running in one. Mark it done in Codealong.".into());
    };
    if s.key == by {
        return Ok(Whose::Own);
    }
    let started = s.origin.as_ref().is_some_and(|o| o.how == OriginHow::Started && o.key == by);
    if !started {
        let mine = board.session(by).map(|(_, me)| format!("“{}”", me.title)).unwrap_or_else(|| "this session".into());
        return Err(format!("“{}” isn't {mine} or a session it started, so it's its user's to mark done, in Codealong.", s.title));
    }
    if s.status == Status::Working {
        return Err(format!("“{}” is still working: mark it done once its turn ends.", s.title));
    }
    if s.status == Status::Needs || s.status == Status::Error {
        return Err(format!("“{}” is waiting on its user, so it's theirs to mark done.", s.title));
    }
    Ok(Whose::Started)
}

/// How long a session's own Mark as done waits for its turn to end before it's dropped.
const WAIT_MS: i64 = 2 * 60 * 60 * 1000;

/// Sessions that asked to be ended once their turn ends, with when they asked.
#[derive(Debug, Default)]
pub struct Waiting {
    asked: HashMap<String, i64>,
}

impl Waiting {
    /// `key` asked at `at` (ms) to be ended once its turn ends.
    pub fn ask(&mut self, key: &str, at: i64) {
        self.asked.insert(key.to_string(), at);
    }

    /// The sessions whose turn has ended since they asked, to end now, as `board` shows them at
    /// `now`. A turn ends when the session stops working, and not at a call it waits on you to
    /// allow, which is still the turn. One whose turn ended asking you something, or with an
    /// error, is left as it is, and so is one that's gone, isn't running, or waited too long.
    /// The board can lag a turn that just started: a status from before the ask isn't the turn's
    /// end.
    pub fn due(&mut self, board: &Board, now: i64) -> Vec<String> {
        let mut out = Vec::new();
        self.asked.retain(|key, &mut at| {
            let Some((_, s)) = board.session(key) else { return false };
            if !s.live || now - at > WAIT_MS {
                return false;
            }
            let in_turn = s.status == Status::Working || (s.status == Status::Needs && !s.asks);
            if in_turn || s.status_since.unwrap_or(s.last_activity) < at {
                return true;
            }
            if matches!(s.status, Status::Unread | Status::Idle | Status::Stopped) {
                out.push(key.clone());
            }
            false
        });
        out.sort();
        out
    }
}

fn waiting() -> &'static Mutex<Waiting> {
    static WAITING: OnceLock<Mutex<Waiting>> = OnceLock::new();
    WAITING.get_or_init(Default::default)
}

/// Holds `key` to be ended once its turn ends (actions.rs `done`).
pub fn wait_for_turn(key: &str) {
    if let Ok(mut w) = waiting().lock() {
        w.ask(key, now_ms());
    }
}

/// The sessions to end now, from each board the engine publishes (lib.rs).
pub fn due(board: &Board) -> Vec<String> {
    waiting().lock().map(|mut w| w.due(board, board.generated_at)).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::SessionOrigin;

    const NOW: i64 = 1_000_000_000;

    fn session(key: &str, status: Status) -> Session {
        Session { status, status_since: Some(NOW - 60_000), last_activity: NOW - 60_000, ..crate::fixtures::session(key) }
    }

    fn board(sessions: Vec<Session>) -> Board {
        let mut b = Board::default();
        b.lanes = vec![crate::fixtures::lane("/work", sessions)];
        b
    }

    fn origin(by: &str, how: OriginHow) -> Option<SessionOrigin> {
        Some(SessionOrigin { key: by.into(), how })
    }

    #[test]
    fn an_agent_marks_done_only_its_own_session_and_the_ones_it_started() {
        let me = session("me", Status::Working);
        let mut started = session("started", Status::Unread);
        started.origin = origin("me", OriginHow::Started);
        let mut forked = session("forked", Status::Idle);
        forked.origin = origin("me", OriginHow::Fork);
        let mut theirs = session("theirs", Status::Idle);
        theirs.origin = origin("other", OriginHow::Started);
        let b = board(vec![me.clone(), started.clone(), forked.clone(), theirs.clone()]);

        assert_eq!(may(&b, Some("me"), &me), Ok(Whose::Own));
        assert_eq!(may(&b, Some("me"), &started), Ok(Whose::Started));
        // A fork of it may be one you made, and another session's is that session's.
        for s in [&forked, &theirs] {
            assert!(may(&b, Some("me"), s).unwrap_err().contains("its user's to mark done"), "{}", s.key);
        }
        assert!(may(&b, None, &started).unwrap_err().contains("isn't running in one"));
        // Never one working or waiting on you, though it started it.
        let mut busy = started.clone();
        busy.status = Status::Working;
        assert!(may(&b, Some("me"), &busy).unwrap_err().contains("still working"));
        for status in [Status::Needs, Status::Error] {
            let mut waits = started.clone();
            waits.status = status;
            assert!(may(&b, Some("me"), &waits).unwrap_err().contains("waiting on its user"), "{status:?}");
        }
    }

    #[test]
    fn its_own_session_ends_once_its_turn_has() {
        let asked = NOW - 30_000;
        let mut w = Waiting::default();
        let ended = |status: Status, since: i64, asks: bool| {
            let mut s = session("me", status);
            s.status_since = Some(since);
            s.asks = asks;
            board(vec![s])
        };
        w.ask("me", asked);
        // Still in its turn, at a call it waits on you to allow too, or the board not caught up.
        assert!(w.due(&ended(Status::Working, NOW - 60_000, false), NOW).is_empty());
        assert!(w.due(&ended(Status::Needs, NOW - 10_000, false), NOW).is_empty());
        assert!(w.due(&ended(Status::Idle, asked - 1, false), NOW).is_empty());
        // Its turn ended: once.
        assert_eq!(w.due(&ended(Status::Unread, NOW - 10_000, false), NOW), ["me"]);
        assert!(w.due(&ended(Status::Unread, NOW - 10_000, false), NOW).is_empty());

        // A turn that ended asking you something, or failed, is left as it is, and forgotten.
        for (status, asks) in [(Status::Needs, true), (Status::Error, false)] {
            w.ask("me", asked);
            assert!(w.due(&ended(status, NOW - 10_000, asks), NOW).is_empty(), "{status:?}");
            assert!(w.due(&ended(Status::Unread, NOW - 5_000, false), NOW).is_empty(), "{status:?} is forgotten");
        }
        // As is one that waited too long, or is gone.
        w.ask("me", asked);
        assert!(w.due(&ended(Status::Working, NOW, false), asked + WAIT_MS + 1).is_empty());
        assert!(w.due(&ended(Status::Unread, NOW, false), NOW).is_empty());
        w.ask("me", asked);
        assert!(w.due(&board(vec![]), NOW).is_empty());
        assert!(w.due(&ended(Status::Unread, NOW, false), NOW).is_empty());
    }
}
