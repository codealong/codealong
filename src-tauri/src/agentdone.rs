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
//! When you asked it to close (`--archive`), you've no reason to read that reply, so it's marked
//! seen and archived too, as Mark as done does, unless ending it didn't go as asked (actions.rs
//! `end_after_turn`): a terminal left open or a worktree kept is yours to see (asked 2026-11-06).
//! A session it started is marked done at once, as from the windows, and the main window says
//! which session did it, with an Undo.

use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};

use crate::actions::From;
use crate::engine::Handle;
use crate::model::{Board, OriginHow, Session, Status};
use crate::util::{now_ms, words};

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
    let started = s.origin.as_ref().is_some_and(|o| o.how == OriginHow::Started && o.key == by)
        || s.helper.as_ref().and_then(|h| h.parent.as_deref()) == Some(by);
    if !started {
        let mine = board.session(by).map(|(_, me)| format!("“{}”", me.title)).unwrap_or_else(|| "this session".into());
        return Err(format!("“{}” isn't {mine} or a session it started, so it's its user's to mark done, in Codealong.", s.title));
    }
    if s.status == Status::Working {
        return Err(format!("“{}” is still working: mark it done once its turn ends.", s.title));
    }
    if s.waits_on_its_user() {
        return Err(format!("“{}” is waiting on its user, so it's theirs to mark done.", s.title));
    }
    Ok(Whose::Started)
}

/// Whether the `codealong` command (run by `by`, when inside a session) may archive `s` on
/// `board`: reversible cleanup of idle or stopped sessions, never interrupting work under way,
/// hiding something waiting on its user, closing a live terminal in Codealong, or orphaning
/// unfinished helpers.
pub fn may_archive_from_command(board: &Board, by: Option<&str>, s: &Session) -> Result<(), String> {
    if by == Some(s.key.as_str()) {
        return Err("Use `codealong session done me --archive` to archive this session once its turn ends.".into());
    }
    if s.status == Status::Working {
        return Err(format!("“{}” is still working: archive it once its turn ends.", s.title));
    }
    if s.waits_on_its_user() {
        return Err(format!("“{}” is waiting on its user, so it's theirs to archive.", s.title));
    }
    if s.status == Status::Unread {
        return Err(format!("“{}” has an unread reply for its user, so it's theirs to read and archive.", s.title));
    }
    if s.background.is_some() {
        return Err(format!("“{}” is running a command in the background, so it stays open.", s.title));
    }
    if s.running.is_some() {
        return Err(format!("“{}” is waiting on a command in a terminal, so it stays open.", s.title));
    }
    if s.stalled_since.is_some() {
        return Err(format!("“{}” looks stuck mid-turn, so it's its user's to archive.", s.title));
    }
    if s.listening {
        return Err(format!("“{}” is listening to a meeting, so it stays open.", s.title));
    }
    if board.terminals.get(&s.key).is_some_and(|ts| ts.iter().any(|t| t.running)) {
        return Err(format!("“{}” has a running terminal in Codealong, which archiving would close; archive it in Codealong.", s.title));
    }
    if has_unfinished_helper(board, &s.key) {
        return Err(format!("“{}” has a helper still working or waiting on its user, so it stays open.", s.title));
    }
    Ok(())
}

/// Whether `key` has any unarchived descendant helper still working or waiting on its user.
fn has_unfinished_helper(board: &Board, key: &str) -> bool {
    let mut seen = HashSet::from([key.to_string()]);
    let mut stack = vec![key.to_string()];
    while let Some(parent) = stack.pop() {
        for h in crate::board::helpers_of(&board.lanes, &parent) {
            if !seen.insert(h.key.clone()) || h.archived {
                continue;
            }
            if h.status == Status::Working || h.unfinished() || h.waits_on_its_user() {
                return true;
            }
            stack.push(h.key.clone());
        }
    }
    false
}

/// One session archived or unarchived by `codealong session archive`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ArchivedSession {
    pub key: String,
    pub title: String,
    /// Why its harness still lists it, where it's archived in Codealong only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kept_in_harness: Option<String>,
}

/// One session `codealong session archive` left as it was, and why.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct KeptSession {
    pub key: String,
    pub title: String,
    pub why: String,
}

/// What `codealong session archive` changed, left already in that state, or refused to touch.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveOutcome {
    pub archived: bool,
    pub changed: Vec<ArchivedSession>,
    pub already: Vec<ArchivedSession>,
    pub kept: Vec<KeptSession>,
    pub message: String,
}

/// The line a toast shows when an agent archives (or unarchives) `titles`.
pub fn archive_toast_message(by_title: Option<&str>, archived: bool, titles: &[String]) -> String {
    let (verb, cap) = if archived { ("archived", "Archived") } else { ("unarchived", "Unarchived") };
    let what = match titles {
        [one] => format!("“{one}”"),
        many => format!("{} sessions", many.len()),
    };
    match by_title.map(str::trim).filter(|t| !t.is_empty()) {
        Some(by) => format!("“{by}” {verb} {what}."),
        None => format!("{cap} {what}."),
    }
}

/// Archives or unarchives `keys` from the `codealong` command: every qualifying session goes in
/// one batch on the engine so the board publishes once and the main window shows one Undo toast.
/// Refuses when nothing changed and nothing was already in the requested state.
pub fn archive_batch(engine: &Handle, from: &From, keys: &[String], archived: bool) -> Result<ArchiveOutcome, String> {
    if keys.is_empty() {
        return Err("Give at least one session key.".into());
    }
    let mut unique = Vec::with_capacity(keys.len());
    let mut seen = HashSet::new();
    for k in keys {
        if seen.insert(k.as_str()) {
            unique.push(k.clone());
        }
    }
    let board = engine.board();
    let by = match from {
        From::Command(by) => by.as_deref(),
        _ => None,
    };
    let by_title = by.and_then(|k| board.session(k)).map(|(_, me)| me.title.clone()).filter(|t| !t.is_empty());
    let mut ready: Vec<(String, String, Option<crate::dismissed::Leaving>)> = Vec::new();
    let mut already = Vec::new();
    let mut kept = Vec::new();
    for key in unique {
        let Some((_, s)) = board.session(&key) else {
            kept.push(KeptSession { key, title: String::new(), why: "Codealong doesn't list that session.".into() });
            continue;
        };
        if s.archived == archived {
            already.push(ArchivedSession { key, title: s.title.clone(), kept_in_harness: None });
            continue;
        }
        if archived {
            if let Err(why) = may_archive_from_command(&board, by, s) {
                kept.push(KeptSession { key, title: s.title.clone(), why });
                continue;
            }
        }
        let leaving = if archived { crate::dismissed::leaving(&board, s, crate::dismissed::What::Archive, from) } else { None };
        ready.push((key, s.title.clone(), leaving));
    }
    if ready.is_empty() && already.is_empty() {
        return Err(refusal_words(&kept));
    }
    let mut changed = Vec::new();
    if !ready.is_empty() {
        let batch_keys: Vec<String> = ready.iter().map(|(k, _, _)| k.clone()).collect();
        let results = engine.archive_batch(&batch_keys, archived);
        for ((key, title, leaving), res) in ready.into_iter().zip(results) {
            match res {
                Ok(kept_in_harness) => {
                    if let Some(l) = leaving {
                        l.note();
                    } else if !archived {
                        crate::dismissed::undone(&key);
                    }
                    changed.push(ArchivedSession { key, title, kept_in_harness });
                }
                Err(e) => kept.push(KeptSession { key, title, why: words(e) }),
            }
        }
        if changed.is_empty() && already.is_empty() {
            return Err(refusal_words(&kept));
        }
    }
    let titles: Vec<String> = changed.iter().map(|s| s.title.clone()).collect();
    let toast_message = (!changed.is_empty()).then(|| archive_toast_message(by_title.as_deref(), archived, &titles));
    let message = outcome_message(toast_message.as_deref(), archived, &already, &kept);
    Ok(ArchiveOutcome { archived, changed, already, kept, message })
}

fn refusal_words(kept: &[KeptSession]) -> String {
    match kept {
        [one] => one.why.clone(),
        many => many.iter().map(|k| k.why.as_str()).collect::<Vec<_>>().join(" "),
    }
}

fn outcome_message(toast: Option<&str>, archived: bool, already: &[ArchivedSession], kept: &[KeptSession]) -> String {
    let mut notes = Vec::new();
    if !already.is_empty() {
        let state = if archived { "already archived" } else { "not archived" };
        notes.push(format!("{} {state}", already.len()));
    }
    if !kept.is_empty() {
        notes.push(format!("{} kept", kept.len()));
    }
    if let Some(base) = toast {
        return if notes.is_empty() { base.to_string() } else { format!("{} ({}).", base.trim_end_matches('.'), notes.join(", ")) };
    }
    let already_said = match (archived, already) {
        (true, [one]) => format!("“{}” is already archived", one.title),
        (false, [one]) => format!("“{}” isn't archived", one.title),
        (true, many) => format!("{} sessions are already archived", many.len()),
        (false, many) => format!("{} sessions aren't archived", many.len()),
    };
    if kept.is_empty() {
        format!("{already_said}.")
    } else {
        format!("{already_said} ({} kept).", kept.len())
    }
}

/// How long a session's own Mark as done waits for its turn to end before it's dropped.
const WAIT_MS: i64 = 2 * 60 * 60 * 1000;

/// A session to end once its turn ends: when it asked, and whether its user asked it to close,
/// so it's archived too (`--archive`).
#[derive(Debug, Clone, Copy, PartialEq)]
struct Asked {
    at: i64,
    archive: bool,
}

/// Sessions that asked to be ended once their turn ends.
#[derive(Debug, Default)]
pub struct Waiting {
    asked: HashMap<String, Asked>,
}

/// A session whose turn has ended, to end now.
#[derive(Debug, Clone, PartialEq)]
pub struct Due {
    pub key: String,
    /// Mark it seen and archive it too, its user having asked it to close.
    pub archive: bool,
}

impl Waiting {
    /// `key` asked at `at` (ms) to be ended once its turn ends, and archived with `archive`.
    pub fn ask(&mut self, key: &str, at: i64, archive: bool) {
        self.asked.insert(key.to_string(), Asked { at, archive });
    }

    /// The sessions whose turn has ended since they asked, to end now, as `board` shows them at
    /// `now`, `jev` saying whether a decision model reads sessions (`[decisions] sessions`). A turn ends when the
    /// session stops working, and not at a call it waits on you to allow, which is still the turn.
    /// One whose turn ended asking you something, or with an error, is left as it is, unarchived,
    /// and so is one that's gone or waited too long, or isn't running and has nothing to archive.
    /// One to archive that has stopped running is archived all the same: a Codex app thread runs
    /// only while it takes a turn. One that moved on since it asked (a prompt, another session's
    /// message, a turn started after the one it asked in), as `session seen` goes by
    /// (`board::agent_seen_spent`), is forgotten: the ask was about a turn that's over. The board
    /// can lag a turn that just started: a status from before the ask isn't the turn's end.
    pub fn due(&mut self, board: &Board, now: i64, jev: bool) -> Vec<Due> {
        let mut out = Vec::new();
        self.asked.retain(|key, &mut Asked { at, archive }| {
            let Some((_, s)) = board.session(key) else { return false };
            if (!s.live && !archive) || now - at > WAIT_MS || crate::board::agent_seen_spent(s, at) {
                return false;
            }
            let in_turn = s.status == Status::Working || (s.status == Status::Needs && !s.asks);
            if in_turn || s.status_since.unwrap_or(s.last_activity) < at {
                return true;
            }
            if !matches!(s.status, Status::Unread | Status::Idle | Status::Stopped) {
                return false;
            }
            let archive = match archive.then(|| may_archive(s, jev, now)) {
                Some(None) => return true,
                may => may.flatten().unwrap_or(false),
            };
            if s.live || archive {
                out.push(Due { key: key.clone(), archive });
            }
            false
        });
        out.sort_by(|a, b| a.key.cmp(&b.key));
        out
    }
}

/// How long a turn that ended waits for Jev's read of whether its reply asks its user something
/// before it's archived or not by its question mark alone: a read takes seconds, and one that
/// failed never comes.
const READ_WAIT_MS: i64 = 2 * 60 * 1000;

/// Whether a session whose user asked it to close may be archived now that its turn has ended:
/// not when its harness cut the turn short (`Session::cut_short`), whose reply is an error, nor
/// when its reply may ask its user something (`board::reply_asks`). None while
/// Jev's read of that is still to come, `jev` saying whether Jev reads sessions.
fn may_archive(s: &Session, jev: bool, now: i64) -> Option<bool> {
    if s.cut_short() {
        return Some(false);
    }
    let read = s.insight.as_ref().is_some_and(|i| i.asks_you.is_some());
    let ended = s.status_since.unwrap_or(s.last_activity);
    if jev && !read && now - ended < READ_WAIT_MS && crate::insights::ask(s, now).is_some() {
        return None;
    }
    Some(!crate::board::reply_asks(s))
}

fn waiting() -> &'static Mutex<Waiting> {
    static WAITING: OnceLock<Mutex<Waiting>> = OnceLock::new();
    WAITING.get_or_init(Default::default)
}

/// Holds `key` to be ended once its turn ends, and archived with `archive` (actions.rs `done`).
pub fn wait_for_turn(key: &str, archive: bool) {
    if let Ok(mut w) = waiting().lock() {
        w.ask(key, now_ms(), archive);
    }
}

/// The sessions to end now, as `board` shows them at `now`.
fn due(board: &Board, now: i64) -> Vec<Due> {
    let Ok(mut w) = waiting().lock() else { return Vec::new() };
    if w.asked.is_empty() {
        return Vec::new();
    }
    // Without settings that read, a reply is taken to ask something by its question mark alone.
    let jev = crate::config::current().is_ok_and(|c| c.decisions.sessions);
    w.due(board, now, jev)
}

/// Ends the sessions due now, as `board` shows them at `now`: from each board the engine
/// publishes (serve.rs), and on `spawn`'s clock. Each on a thread of its own: closing a terminal
/// takes a moment. Taken off the queue as they're found, so the two never end one twice.
pub fn end_due(engine: &Handle, board: &Board, now: i64) {
    for Due { key, archive } in due(board, now) {
        let engine = engine.clone();
        std::thread::spawn(move || match crate::actions::end_after_turn(&engine, &key, archive) {
            Ok(said) => eprintln!("Ended {key} once its turn did, as it asked: {said}"),
            Err(e) => eprintln!("Couldn't end {key} once its turn did: {e}"),
        });
    }
}

/// How often the clock looks while a session waits to be ended.
const LOOK_EVERY: std::time::Duration = std::time::Duration::from_secs(15);

/// Looks at the sessions waiting to be ended on a clock of its own, beside the boards the engine
/// publishes: a turn waiting on Jev's read of its reply goes by its question mark once the read
/// is overdue (`READ_WAIT_MS`), and a read that failed changes nothing on the board, which isn't
/// published again until something does.
pub fn spawn(engine: Handle) {
    let started = std::thread::Builder::new().name("codealong-agentdone".into()).spawn(move || loop {
        std::thread::sleep(LOOK_EVERY);
        if waiting().lock().is_ok_and(|w| !w.asked.is_empty()) {
            end_due(&engine, &engine.board(), now_ms());
        }
    });
    if let Err(e) = started {
        eprintln!("codealong: a session waiting on Jev's read to be archived waits until the board changes: {e}");
    }
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
        let mut adopted = session("adopted", Status::Idle);
        adopted.origin = origin("old-boss", OriginHow::Started);
        adopted.helper = Some(crate::model::Helper { parent: Some("me".into()), ..Default::default() });
        let b = board(vec![me.clone(), started.clone(), forked.clone(), theirs.clone(), adopted.clone()]);

        assert_eq!(may(&b, Some("me"), &me), Ok(Whose::Own));
        assert_eq!(may(&b, Some("me"), &started), Ok(Whose::Started));
        assert_eq!(may(&b, Some("me"), &adopted), Ok(Whose::Started));
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
        // Nor one stopped on its account's sign-in, whose turn didn't end.
        let mut signed_out = started.clone();
        signed_out.status = Status::Idle;
        signed_out.signed_out = Some(crate::model::SignedOut { at: 1, said: "OAuth token revoked · Please run /login".into(), continues: true });
        assert!(may(&b, Some("me"), &signed_out).unwrap_err().contains("waiting on its user"));
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
        w.ask("me", asked, false);
        // Still in its turn, at a call it waits on you to allow too, or the board not caught up.
        assert!(w.due(&ended(Status::Working, NOW - 60_000, false), NOW, false).is_empty());
        assert!(w.due(&ended(Status::Needs, NOW - 10_000, false), NOW, false).is_empty());
        assert!(w.due(&ended(Status::Idle, asked - 1, false), NOW, false).is_empty());
        // Its turn ended: once.
        assert_eq!(w.due(&ended(Status::Unread, NOW - 10_000, false), NOW, false), [Due { key: "me".into(), archive: false }]);
        assert!(w.due(&ended(Status::Unread, NOW - 10_000, false), NOW, false).is_empty());

        // A turn that ended asking you something, or failed, is left as it is, and forgotten.
        for (status, asks) in [(Status::Needs, true), (Status::Error, false)] {
            w.ask("me", asked, false);
            assert!(w.due(&ended(status, NOW - 10_000, asks), NOW, false).is_empty(), "{status:?}");
            assert!(w.due(&ended(Status::Unread, NOW - 5_000, false), NOW, false).is_empty(), "{status:?} is forgotten");
        }
        // As is one that waited too long, or is gone.
        w.ask("me", asked, false);
        assert!(w.due(&ended(Status::Working, NOW, false), asked + WAIT_MS + 1, false).is_empty());
        assert!(w.due(&ended(Status::Unread, NOW, false), NOW, false).is_empty());
        w.ask("me", asked, false);
        assert!(w.due(&board(vec![]), NOW, false).is_empty());
        assert!(w.due(&ended(Status::Unread, NOW, false), NOW, false).is_empty());
    }

    /// A session whose turn ended at NOW - 10 s, replying `reply`, `change`d.
    fn ended_with(status: Status, reply: &str, change: impl FnOnce(&mut Session)) -> Board {
        let mut s = session("me", status);
        s.status_since = Some(NOW - 10_000);
        s.last_activity = NOW - 10_000;
        s.reply_tail = Some(reply.into());
        change(&mut s);
        board(vec![s])
    }

    fn due(key: &str, archive: bool) -> Vec<Due> {
        vec![Due { key: key.into(), archive }]
    }

    #[test]
    fn one_its_user_asked_to_close_is_archived_once_its_turn_ends_unless_it_asks_them_something() {
        let asked = NOW - 30_000;
        let done = "Deleted the branch; this session closes now.";
        let mut w = Waiting::default();
        w.ask("me", asked, true);
        // Still in the turn it asked in.
        assert!(w.due(&ended_with(Status::Working, done, |s| s.status_since = Some(asked - 60_000)), NOW, false).is_empty());
        assert_eq!(w.due(&ended_with(Status::Unread, done, |_| {}), NOW, false), due("me", true));
        // Asked again without it before its turn ends, it's left for its user to read.
        w.ask("me", asked, true);
        w.ask("me", asked, false);
        assert_eq!(w.due(&ended_with(Status::Idle, done, |_| {}), NOW, false), due("me", false));
        // A turn that ended asking its user something, or failed, is neither ended nor archived.
        for (status, asks) in [(Status::Needs, true), (Status::Error, false)] {
            w.ask("me", asked, true);
            assert!(w.due(&ended_with(status, done, |s| s.asks = asks), NOW, false).is_empty(), "{status:?}");
            assert!(w.due(&ended_with(Status::Unread, done, |_| {}), NOW, false).is_empty(), "{status:?} is forgotten");
        }
        // A reply that may ask something, or a turn its harness cut short, is ended and left
        // for its user to see.
        let cut_short: [(&str, &str, fn(&mut Session)); 4] = [
            ("asks", "Want me to remove the worktree too?", |_| {}),
            ("asks in Markdown, with more after", "**Should I keep the worktree?** It has no changes.", |_| {}),
            ("connection lost", done, |s| s.connection_lost = Some(Default::default())),
            ("cut off", done, |s| s.cut_off = Some(NOW - 10_000)),
        ];
        for (what, reply, change) in cut_short {
            w.ask("me", asked, true);
            assert_eq!(w.due(&ended_with(Status::Unread, reply, change), NOW, false), due("me", false), "{what}");
        }
    }

    /// Its user wrote to it once its turn had ended, before Jev's read came, or another turn of
    /// its began: what it asked was about a turn that's over, and the next is theirs.
    #[test]
    fn one_that_moved_on_since_it_asked_is_forgotten() {
        let asked = NOW - 30_000;
        let reply = "Deleted the branch; the worktree is next";
        let moved_on: [(&str, fn(&mut Session)); 3] = [
            ("a prompt", |s| s.last_prompt_at = NOW - 5_000),
            ("another session's message", |s| s.input_at = Some(NOW - 5_000)),
            ("a turn after the one it asked in", |s| {
                s.status = Status::Working;
                s.status_since = Some(NOW - 5_000);
            }),
        ];
        for (what, change) in moved_on {
            let mut w = Waiting::default();
            w.ask("me", asked, true);
            // Its turn ended, and Jev hasn't read it yet.
            assert!(w.due(&ended_with(Status::Unread, reply, |_| {}), NOW, true).is_empty(), "{what}");
            assert!(w.due(&ended_with(Status::Unread, reply, change), NOW, true).is_empty(), "{what}");
            // The turn that came of it ends, read as asking nothing: it isn't ended for the ask.
            let read = |s: &mut Session| s.insight = Some(crate::model::SessionInsight { asks_you: Some(0.0), ..Default::default() });
            assert!(w.due(&ended_with(Status::Unread, reply, read), NOW + 60_000, true).is_empty(), "{what}");
        }
    }

    #[test]
    fn with_jev_on_it_waits_for_jevs_read_of_the_reply_before_archiving() {
        let asked = NOW - 30_000;
        let reply = "Deleted the branch; the worktree is next";
        let read = |asks_you: f64| move |s: &mut Session| s.insight = Some(crate::model::SessionInsight { asks_you: Some(asks_you), ..Default::default() });
        let mut w = Waiting::default();
        w.ask("me", asked, true);
        // Not read yet: it waits, and goes once Jev says it asks nothing.
        assert!(w.due(&ended_with(Status::Unread, reply, |_| {}), NOW, true).is_empty());
        assert_eq!(w.due(&ended_with(Status::Unread, reply, read(0.05)), NOW, true), due("me", true));
        // Jev's read wins over the question mark both ways, at Close idle sessions' bar.
        w.ask("me", asked, true);
        assert_eq!(w.due(&ended_with(Status::Unread, reply, read(0.4)), NOW, true), due("me", false));
        w.ask("me", asked, true);
        assert_eq!(w.due(&ended_with(Status::Unread, "Done. Anything else?", read(0.1)), NOW, true), due("me", true));
        // A read that never comes: the question mark decides once the wait is over.
        w.ask("me", asked, true);
        assert!(w.due(&ended_with(Status::Unread, reply, |_| {}), NOW + READ_WAIT_MS - 10_001, true).is_empty());
        assert_eq!(w.due(&ended_with(Status::Unread, reply, |_| {}), NOW + READ_WAIT_MS, true), due("me", true));
        // Jev doesn't read a helper's reply, so there's nothing to wait for.
        w.ask("me", asked, true);
        let helps = |s: &mut Session| s.helper = Some(crate::model::Helper { parent: Some("other".into()), ..Default::default() });
        assert_eq!(w.due(&ended_with(Status::Unread, reply, helps), NOW, true), due("me", true));
    }

    #[test]
    fn one_no_longer_running_is_archived_if_asked_and_otherwise_forgotten() {
        let asked = NOW - 30_000;
        let done = "Deleted the branch; this session closes now.";
        let stopped = |s: &mut Session| s.live = false;
        let mut w = Waiting::default();
        // A Codex app thread runs only while its turn does.
        w.ask("me", asked, true);
        assert_eq!(w.due(&ended_with(Status::Unread, done, stopped), NOW, false), due("me", true));
        // Without --archive there's nothing left to do.
        w.ask("me", asked, false);
        assert!(w.due(&ended_with(Status::Unread, done, stopped), NOW, false).is_empty());
        assert!(w.due(&ended_with(Status::Unread, done, |_| {}), NOW, false).is_empty());
        // Nor when its reply asks something.
        w.ask("me", asked, true);
        assert!(w.due(&ended_with(Status::Unread, "Shall I remove it?", stopped), NOW, false).is_empty());
        assert!(w.due(&ended_with(Status::Unread, done, |_| {}), NOW, false).is_empty());
    }

    #[test]
    fn command_archive_allows_idle_and_stopped_and_refuses_active_or_waiting() {
        let mut me = session("me", Status::Working);
        me.title = "Cleanup review".into();
        let mut idle = session("idle", Status::Idle);
        idle.title = "Old cleanup 1".into();
        let mut stopped = session("stopped", Status::Stopped);
        stopped.title = "Old cleanup 2".into();
        let mut working = session("working", Status::Working);
        working.title = "Still building".into();
        let mut needs = session("needs", Status::Needs);
        needs.title = "Waiting on approve".into();
        let mut errored = session("errored", Status::Error);
        errored.title = "Failed turn".into();
        let mut unread = session("unread", Status::Unread);
        unread.title = "Unread reply".into();
        let mut signed_out = session("signed-out", Status::Idle);
        signed_out.title = "Signed out".into();
        signed_out.signed_out = Some(crate::model::SignedOut { at: 1, said: "Please run /login".into(), continues: true });
        let mut bg = session("bg", Status::Idle);
        bg.title = "Running bg".into();
        bg.background = Some("npm run dev".into());
        let mut running = session("running", Status::Idle);
        running.title = "Waiting on terminal".into();
        running.running = Some(crate::model::RunningIn { terminal: "Ghostty".into(), command: "make".into(), yours: false });
        let mut stalled = session("stalled", Status::Idle);
        stalled.title = "Stalled".into();
        stalled.stalled_since = Some(NOW - 5_000);
        let mut listening = session("listening", Status::Idle);
        listening.title = "In meeting".into();
        listening.listening = true;
        let mut with_live_term = session("live-term", Status::Idle);
        with_live_term.title = "Has live shell".into();
        let mut with_exited_term = session("exited-term", Status::Idle);
        with_exited_term.title = "Has exited shell".into();
        let mut parent_busy = session("parent-busy", Status::Idle);
        parent_busy.title = "Parent with busy helper".into();
        let mut busy_helper = session("busy-helper", Status::Working);
        busy_helper.live = true;
        busy_helper.helper = Some(crate::model::Helper { parent: Some("parent-busy".into()), ..Default::default() });
        let mut parent_done = session("parent-done", Status::Idle);
        parent_done.title = "Parent with idle helper".into();
        let mut idle_helper = session("idle-helper", Status::Idle);
        idle_helper.helper = Some(crate::model::Helper { parent: Some("parent-done".into()), ..Default::default() });

        let mut b = board(vec![
            me.clone(),
            idle.clone(),
            stopped.clone(),
            working.clone(),
            needs.clone(),
            errored.clone(),
            unread.clone(),
            signed_out.clone(),
            bg.clone(),
            running.clone(),
            stalled.clone(),
            listening.clone(),
            with_live_term.clone(),
            with_exited_term.clone(),
            parent_busy.clone(),
            busy_helper,
            parent_done.clone(),
            idle_helper,
        ]);
        let glance = |id: &str, key: &str, running: bool| crate::terminals::Glance {
            id: id.into(),
            session: key.into(),
            title: String::new(),
            shell: "zsh".into(),
            command: None,
            folder: "/work".into(),
            started_at: 0,
            running,
            exit: (!running).then_some(0),
            program: None,
            bell_at: None,
            tty: String::new(),
            hosts: false,
            asks: None,
            asks_since: None,
        };
        b.terminals.insert("live-term".into(), vec![glance("t1", "live-term", true)]);
        b.terminals.insert("exited-term".into(), vec![glance("t2", "exited-term", false)]);

        assert_eq!(may_archive_from_command(&b, Some("me"), &idle), Ok(()));
        assert_eq!(may_archive_from_command(&b, Some("me"), &stopped), Ok(()));
        assert_eq!(may_archive_from_command(&b, None, &idle), Ok(()));
        assert_eq!(may_archive_from_command(&b, Some("me"), &with_exited_term), Ok(()));
        assert_eq!(may_archive_from_command(&b, Some("me"), &parent_done), Ok(()));

        assert!(may_archive_from_command(&b, Some("me"), &me).unwrap_err().contains("session done me --archive"));
        assert!(may_archive_from_command(&b, Some("me"), &working).unwrap_err().contains("still working"));
        assert!(may_archive_from_command(&b, Some("me"), &needs).unwrap_err().contains("waiting on its user"));
        assert!(may_archive_from_command(&b, Some("me"), &errored).unwrap_err().contains("waiting on its user"));
        assert!(may_archive_from_command(&b, Some("me"), &signed_out).unwrap_err().contains("waiting on its user"));
        assert!(may_archive_from_command(&b, Some("me"), &unread).unwrap_err().contains("unread reply"));
        assert!(may_archive_from_command(&b, Some("me"), &bg).unwrap_err().contains("background"));
        assert!(may_archive_from_command(&b, Some("me"), &running).unwrap_err().contains("command in a terminal"));
        assert!(may_archive_from_command(&b, Some("me"), &stalled).unwrap_err().contains("stuck mid-turn"));
        assert!(may_archive_from_command(&b, Some("me"), &listening).unwrap_err().contains("listening to a meeting"));
        assert!(may_archive_from_command(&b, Some("me"), &with_live_term).unwrap_err().contains("running terminal in Codealong"));
        assert!(may_archive_from_command(&b, Some("me"), &parent_busy).unwrap_err().contains("helper still working"));
    }

    #[test]
    fn archive_batch_handles_mixed_batches_idempotency_and_unarchive() {
        let mut me = session("claude:a:me", Status::Working);
        me.title = "Cleanup review".into();
        let mut idle1 = session("claude:a:one", Status::Idle);
        idle1.title = "Old cleanup 1".into();
        let mut idle2 = session("claude:a:two", Status::Stopped);
        idle2.title = "Old cleanup 2".into();
        let mut already = session("claude:a:done", Status::Idle);
        already.title = "Already gone".into();
        already.archived = true;
        let mut busy = session("claude:a:busy", Status::Working);
        busy.title = "Active work".into();

        // Use an accountless harness key ("unknown:a:...") so Handle::over doesn't look up
        // config.toml accounts when archiving in tests.
        let u = |s: Session, key: &str| {
            let mut s = s;
            s.key = key.into();
            s
        };
        let me = u(me, "test:a:me");
        let idle1 = u(idle1, "test:a:one");
        let idle2 = u(idle2, "test:a:two");
        let already = u(already, "test:a:done");
        let busy = u(busy, "test:a:busy");
        let engine = Handle::over(board(vec![me, idle1, idle2, already, busy]));
        let from = From::Command(Some("test:a:me".into()));

        // Empty list is refused.
        assert!(archive_batch(&engine, &from, &[], true).unwrap_err().contains("at least one"));
        // All refused fails with the reason.
        assert!(archive_batch(&engine, &from, &["test:a:busy".into()], true).unwrap_err().contains("still working"));
        // Mixed batch archives qualifying ones, deduplicates keys, notes already-archived and kept.
        let out = archive_batch(&engine, &from, &["test:a:one".into(), "test:a:one".into(), "test:a:two".into(), "test:a:done".into(), "test:a:busy".into()], true).unwrap();
        assert!(out.archived);
        assert_eq!(out.changed.iter().map(|s| s.key.as_str()).collect::<Vec<_>>(), ["test:a:one", "test:a:two"]);
        assert_eq!(out.already.iter().map(|s| s.key.as_str()).collect::<Vec<_>>(), ["test:a:done"]);
        assert_eq!(out.kept.len(), 1);
        assert_eq!(out.kept[0].key, "test:a:busy");
        assert_eq!(out.message, "“Cleanup review” archived 2 sessions (1 already archived, 1 kept).");

        // Already-archived session alone succeeds cleanly without error.
        let same = archive_batch(&engine, &from, &["test:a:done".into()], true).unwrap();
        assert!(same.changed.is_empty());
        assert_eq!(same.already.len(), 1);
        assert_eq!(same.message, "“Already gone” is already archived.");

        // Unarchive restores archived sessions and reports unarchived ones as already.
        let back = archive_batch(&engine, &from, &["test:a:done".into(), "test:a:one".into()], false).unwrap();
        assert!(!back.archived);
        assert_eq!(back.changed.iter().map(|s| s.key.as_str()).collect::<Vec<_>>(), ["test:a:done"]);
        assert_eq!(back.already.iter().map(|s| s.key.as_str()).collect::<Vec<_>>(), ["test:a:one"]);
        assert_eq!(back.message, "“Cleanup review” unarchived “Already gone” (1 not archived).");

        // Toast line formatting with and without a calling session.
        assert_eq!(archive_toast_message(Some("Cleanup review"), true, &["Old cleanup 1".into()]), "“Cleanup review” archived “Old cleanup 1”.");
        assert_eq!(archive_toast_message(Some("Cleanup review"), true, &["a".into(), "b".into()]), "“Cleanup review” archived 2 sessions.");
        assert_eq!(archive_toast_message(None, true, &["Old cleanup 1".into()]), "Archived “Old cleanup 1”.");
        assert_eq!(archive_toast_message(None, false, &["a".into(), "b".into()]), "Unarchived 2 sessions.");
    }
}
