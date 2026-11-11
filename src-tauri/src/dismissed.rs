//! How the sessions you were done with stood as you left them (with `[experimental] analytics`
//! on): a row in the ledger for each Mark as done, Archive and Close idle sessions, saying who
//! asked and where the session's work stood at that moment (`board::Standing`): finished, or
//! still working, asking you something, not landed. So a report can say which sessions you
//! dismissed unfinished, and a window can say so as you do it.
//!
//! The standing is read from the board before anything is ended (`leaving`): Mark as done stops
//! a turn and marks its reply seen on the way, and what's wanted is how it stood when you chose
//! to be done with it. It's written once the session is archived (`Leaving::note`), since a Mark
//! as done that work under way holds back archives nothing. A helper's or a headless run's work
//! is its starter's, so they aren't noted.
//!
//! A session taken out of the archive is marked so (`undone`), and one you prompted again after
//! marking it done counts as reopened in a report, by its turns.

use std::collections::BTreeMap;

use anyhow::Result;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::actions::From;
use crate::board::{self, Standing};
use crate::model::{Board, Session, Status};
use crate::util::{now_ms, DAY_MS};

/// What was done to the session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum What {
    /// Mark as done: ended as far as Codealong can end it, seen and archived.
    Done,
    /// Archive, from its menu.
    Archive,
    /// Close idle sessions, with Also archive them.
    CloseIdle,
}

impl What {
    fn word(self) -> &'static str {
        match self {
            What::Done => "done",
            What::Archive => "archive",
            What::CloseIdle => "close-idle",
        }
    }
}

/// A session about to be left, as it stood.
#[derive(Clone, Debug, PartialEq)]
pub struct Leaving {
    pub key: String,
    pub title: String,
    pub account: String,
    pub folder: String,
    pub what: What,
    /// `you`, `agent` (the command, with its session's key in `by_key` when it found one) or
    /// `device` (another machine of yours, where an agent may have asked).
    pub by: &'static str,
    pub by_key: Option<String>,
    /// `Standing::word`, and why in a few words when finished.
    pub standing: &'static str,
    pub why: String,
    /// You hadn't seen its last reply.
    pub unread: bool,
}

/// Who asked, as the ledger keeps it.
fn by_of(from: &From) -> (&'static str, Option<String>) {
    match from {
        From::Here | From::Phone => ("you", None),
        From::Device => ("device", None),
        From::Command(by) => ("agent", by.clone()),
    }
}

/// How the session `s` stands on `board` as it's about to be left, for `Leaving::note` once it
/// has been. None while analytics are off, and for a session whose work isn't yours to weigh.
pub fn leaving(board: &Board, s: &Session, what: What, from: &From) -> Option<Leaving> {
    if !crate::config::current().is_ok_and(|c| c.experimental.analytics) {
        return None;
    }
    leaving_on(board, s, what, from)
}

fn leaving_on(board: &Board, s: &Session, what: What, from: &From) -> Option<Leaving> {
    let standing = board::standing_on(&board.lanes, &board.prs, &board.todos, &s.key)?;
    if standing == Standing::NotYours {
        return None;
    }
    let why = match &standing {
        Standing::Finished(f) => f.why.join(", "),
        _ => String::new(),
    };
    let (by, by_key) = by_of(from);
    Some(Leaving { key: s.key.clone(), title: s.title.clone(), account: s.account.clone(), folder: s.cwd.clone(), what, by, by_key, standing: standing.word(), why, unread: s.status == Status::Unread })
}

impl Leaving {
    /// Notes it in the ledger: the session was archived. A failure is said and nothing more: the
    /// session is done with either way.
    pub fn note(&self) {
        if let Err(e) = crate::ledger::with_db(|db| put(db, self, now_ms())) {
            eprintln!("Couldn't note how {} stood in the ledger: {e:#}", self.key);
        }
    }

    /// What a window says as you mark an unfinished session done: how it stood, or nothing for
    /// one that was finished or that nothing is known of. Nor for work that was under way: Mark
    /// as done asks before it ends that, and says what it ended.
    pub fn hint(&self) -> Option<&'static str> {
        (Standing::unfinished(self.standing) && !matches!(self.standing, "working" | "to-come")).then(|| Standing::said(self.standing))
    }
}

fn put(db: &Connection, l: &Leaving, at: i64) -> Result<()> {
    db.execute(
        "INSERT INTO dismissals (at, session, title, account, folder, what, by, by_key, standing, why, unread) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![at, l.key, l.title, l.account, l.folder, l.what.word(), l.by, l.by_key, l.standing, l.why, l.unread as i64],
    )?;
    Ok(())
}

/// A session taken out of the archive: its latest dismissal is undone.
pub fn undone(key: &str) {
    if !crate::config::current().is_ok_and(|c| c.experimental.analytics) {
        return;
    }
    let done = crate::ledger::with_db(|db| {
        db.execute("UPDATE dismissals SET undone = ?2 WHERE id = (SELECT id FROM dismissals WHERE session = ?1 AND undone IS NULL ORDER BY at DESC LIMIT 1)", params![key, now_ms()])?;
        Ok(())
    });
    if let Err(e) = done {
        eprintln!("Couldn't note in the ledger that {key} came out of the archive: {e:#}");
    }
}

/* ---- Reports ---- */

/// A session you left, for a list.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Left {
    pub at: i64,
    pub session: String,
    pub title: String,
    pub account: String,
    pub folder: String,
    /// `done`, `archive` or `close-idle`.
    pub what: String,
    /// `Standing::word`, and what a sentence says of it.
    pub standing: String,
    pub said: String,
    /// You hadn't seen its last reply.
    pub unread: bool,
    /// You took it out of the archive, or prompted it again, after.
    pub reopened: bool,
}

/// A session that's still open with work left undone, untouched for days.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hanging {
    pub session: String,
    pub title: String,
    pub account: String,
    pub folder: String,
    pub standing: String,
    pub said: String,
    pub last_activity: i64,
}

/// How sessions began and how you left them, over some days.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Outcomes {
    /// Sessions active in the days, by how each began (`Began::word`), an empty key for the ones
    /// nothing was noted of.
    pub began: Vec<crate::turns::Count>,
    /// The sessions you marked done or archived: how many in all, how many stood finished
    /// (for sure, or by the read of their last reply), unfinished, and with nothing known.
    pub left: u64,
    pub finished: u64,
    pub looked_finished: u64,
    pub unfinished: u64,
    pub unknown: u64,
    /// Of them, the ones you took up again after.
    pub reopened: u64,
    /// By standing (`Standing::word`).
    pub standings: Vec<crate::turns::Count>,
    /// The ones left unfinished, the newest first, at most `LISTED`.
    pub list: Vec<Left>,
    /// Sessions an agent marked done from the command, which aren't counted above.
    pub by_agents: u64,
    /// Open sessions of yours with work left undone that nothing has touched for `HANGING_DAYS`.
    pub hanging: Vec<Hanging>,
    /// The earliest dismissal kept: before it, nothing is known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since: Option<i64>,
}

const LISTED: usize = 200;
/// How long a session with work left undone goes untouched before it's listed as hanging, and
/// how far back the list looks.
pub const HANGING_DAYS: i64 = 3;
const HANGING_BACK_DAYS: i64 = 30;

/// The open sessions of yours on `board` whose work isn't finished and that nothing has touched
/// for `HANGING_DAYS`: asking you something, stopped on an error, cut short, not landed. Not one
/// that's working or snoozed, which is going on; nor one nothing is known of.
pub fn hanging(board: &Board, account: Option<&str>, now: i64) -> Vec<Hanging> {
    let works = board::works(&board.lanes, &board.prs, &board.todos);
    let mut out: Vec<Hanging> = board
        .lanes
        .iter()
        .filter(|l| l.device.is_none())
        .flat_map(|l| &l.sessions)
        .filter(|s| s.visitor.is_none() && !s.archived && account.is_none_or(|a| a == s.account))
        .filter(|s| now - s.last_activity >= HANGING_DAYS * DAY_MS && now - s.last_activity <= HANGING_BACK_DAYS * DAY_MS)
        .filter_map(|s| {
            let standing = board::standing_of(s, works.get(&s.key)?);
            let word = standing.word();
            (Standing::unfinished(word) && !matches!(standing, Standing::Working | Standing::ToCome)).then(|| Hanging {
                session: s.key.clone(),
                title: s.title.clone(),
                account: s.account.clone(),
                folder: s.cwd.clone(),
                standing: word.to_string(),
                said: said_now(word).to_string(),
                last_activity: s.last_activity,
            })
        })
        .collect();
    out.sort_by(|a, b| b.last_activity.cmp(&a.last_activity));
    out
}

/// A standing of a session that's still open, as a sentence says it now.
fn said_now(word: &str) -> &'static str {
    match word {
        "needs" => "It waits on you to allow a call or answer it",
        "error" => "It stopped on an error",
        "asks" => "Its last reply asks you something",
        "cut-short" => "Its turn was cut short",
        "step" => "Its reply leaves you a next step",
        "not-landed" => "Its change hasn't landed",
        "not-done" => "Its reply reads as not done",
        "open-pr" => "A PR it worked on is still open",
        "open-todo" => "A todo linked to it isn't done",
        "unsaved" => "Its worktree holds work that isn't saved",
        other => Standing::said(other),
    }
}

/// How sessions began and how you left them over the last `days` days, for one account or all.
pub fn outcomes_in(db: &Connection, board: Option<&Board>, days: u32, account: Option<&str>, now: i64) -> Result<Outcomes> {
    let from: i64 = db.query_row("SELECT CAST(strftime('%s', date('now', 'localtime', ?1), 'utc') AS INTEGER) * 1000", [format!("-{} days", days.max(1) - 1)], |r| r.get(0))?;
    let mut out = Outcomes::default();
    let mut began: BTreeMap<String, u64> = BTreeMap::new();
    let mut stmt = db.prepare(
        "SELECT coalesce(s.began, ''), count(*) FROM sessions s
         WHERE (?2 IS NULL OR s.account = ?2)
           AND (s.started >= ?1 OR EXISTS (SELECT 1 FROM calls c WHERE c.session = s.id AND c.at >= ?1) OR EXISTS (SELECT 1 FROM turns t WHERE t.session = s.id AND t.at >= ?1))
         GROUP BY 1",
    )?;
    for row in stmt.query_map(params![from, account], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))? {
        let (word, n) = row?;
        began.insert(word, n as u64);
    }
    let mut v: Vec<crate::turns::Count> = began.into_iter().map(|(key, n)| crate::turns::Count { key, n }).collect();
    v.sort_by(|a, b| b.n.cmp(&a.n).then(a.key.cmp(&b.key)));
    out.began = v;

    let mut standings: BTreeMap<String, u64> = BTreeMap::new();
    let mut stmt = db.prepare(
        "SELECT d.at, d.session, d.title, d.account, d.folder, d.what, d.by, d.standing, d.unread,
                d.undone IS NOT NULL OR EXISTS (SELECT 1 FROM turns t JOIN sessions s ON s.id = t.session WHERE s.key = d.session AND t.source = 'you' AND t.at > d.at)
         FROM dismissals d WHERE d.at >= ?1 AND (?2 IS NULL OR d.account = ?2) ORDER BY d.at DESC",
    )?;
    let mut rows = stmt.query(params![from, account])?;
    // A session left more than once in the days counts by its latest leaving.
    let mut seen = std::collections::HashSet::new();
    while let Some(r) = rows.next()? {
        let by: String = r.get(6)?;
        let key: String = r.get(1)?;
        if !seen.insert(key.clone()) {
            continue;
        }
        if by == "agent" {
            out.by_agents += 1;
            continue;
        }
        let standing: String = r.get(7)?;
        let reopened = r.get::<_, i64>(9)? != 0;
        out.left += 1;
        out.reopened += reopened as u64;
        *standings.entry(standing.clone()).or_default() += 1;
        match standing.as_str() {
            "finished" => out.finished += 1,
            "looks-finished" => out.looked_finished += 1,
            "unknown" => out.unknown += 1,
            _ => {
                out.unfinished += 1;
                if out.list.len() < LISTED {
                    out.list.push(Left { at: r.get(0)?, session: key, title: r.get(2)?, account: r.get(3)?, folder: r.get(4)?, what: r.get(5)?, said: Standing::said(&standing).to_string(), standing, unread: r.get::<_, i64>(8)? != 0, reopened });
                }
            }
        }
    }
    let mut v: Vec<crate::turns::Count> = standings.into_iter().map(|(key, n)| crate::turns::Count { key, n }).collect();
    v.sort_by(|a, b| b.n.cmp(&a.n).then(a.key.cmp(&b.key)));
    out.standings = v;
    out.since = db.query_row("SELECT min(at) FROM dismissals", [], |r| r.get(0))?;
    if let Some(board) = board {
        out.hanging = hanging(board, account, now);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::SessionInsight;

    fn session(id: &str, status: Status) -> Session {
        Session { title: format!("Session {id}"), account: "claude".into(), cwd: "/work/app".into(), status, ..crate::fixtures::session(id) }
    }

    fn board(sessions: Vec<Session>) -> Board {
        Board { lanes: vec![crate::fixtures::lane("/work/app", sessions)], ..Default::default() }
    }

    fn db() -> Connection {
        crate::ledger::in_memory()
    }

    #[test]
    fn a_session_is_noted_as_it_stood_when_you_left_it() {
        let working = session("w", Status::Working);
        let asks = Session { asks: true, ..session("q", Status::Needs) };
        let read_done = Session { insight: Some(SessionInsight { finished: Some(0.97), ending: Some("report".into()), ..Default::default() }), ..session("d", Status::Unread) };
        let helper = Session { helper: Some(crate::model::Helper { parent: Some("x".into()), ..Default::default() }), ..session("h", Status::Idle) };
        let b = board(vec![working.clone(), asks.clone(), read_done.clone(), helper.clone()]);
        let left = |s: &Session, from: &From| leaving_on(&b, s, What::Done, from);
        let l = left(&working, &From::Here).unwrap();
        assert_eq!((l.standing, l.by, l.unread), ("working", "you", false));
        assert_eq!(l.hint(), None, "Mark as done says what it stopped");
        assert_eq!(left(&asks, &From::Phone).unwrap().hint(), Some("Its last reply asked you something"));
        let done = left(&read_done, &From::Command(Some("claude:claude:p".into()))).unwrap();
        assert_eq!((done.standing, done.by, done.by_key.as_deref(), done.unread, done.hint()), ("looks-finished", "agent", Some("claude:claude:p"), true, None));
        assert_eq!(done.why, "its reply reads as done");
        // A helper's work is its starter's, and a session the board doesn't list has no standing.
        assert_eq!(left(&helper, &From::Here), None);
        assert_eq!(left(&session("gone", Status::Idle), &From::Here), None);
        // Every standing has its sentence, and only work left undone is a hint.
        for word in board::STANDINGS {
            assert!(!Standing::said(word).is_empty());
        }
        assert!(Standing::unfinished("asks") && !Standing::unfinished("unknown") && !Standing::unfinished("looks-finished"));
    }

    #[test]
    fn a_report_counts_what_you_left_and_lists_the_unfinished() {
        let db = db();
        let now = now_ms();
        let l = |key: &str, standing: &'static str, by: &'static str| Leaving { key: key.into(), title: format!("Session {key}"), account: "claude".into(), folder: "/work/app".into(), what: What::Done, by, by_key: None, standing, why: String::new(), unread: false };
        put(&db, &l("a", "finished", "you"), now - 5000).unwrap();
        put(&db, &l("b", "asks", "you"), now - 4000).unwrap();
        put(&db, &l("c", "working", "you"), now - 3000).unwrap();
        put(&db, &l("d", "unknown", "you"), now - 2000).unwrap();
        put(&db, &l("e", "looks-finished", "agent"), now - 1000).unwrap();
        // Left twice, a session counts as it stood the last time.
        put(&db, &l("b", "not-landed", "you"), now - 500).unwrap();
        // You prompted one of them again after.
        let sid = crate::ledger::session_row(&db, "c", "claude", "/work/app").unwrap();
        db.execute("INSERT INTO turns (id, session, at, source, kind) VALUES (1, ?1, ?2, 'you', 'prompt')", params![sid, now - 100]).unwrap();
        db.execute("UPDATE sessions SET began = 'you', started = ?1", [now - 9000]).unwrap();
        let o = outcomes_in(&db, None, 7, None, now).unwrap();
        assert_eq!((o.left, o.finished, o.looked_finished, o.unfinished, o.unknown, o.reopened, o.by_agents), (4, 1, 0, 2, 1, 1, 1));
        let listed: Vec<(&str, &str, bool)> = o.list.iter().map(|l| (l.session.as_str(), l.standing.as_str(), l.reopened)).collect();
        assert_eq!(listed, [("b", "not-landed", false), ("c", "working", true)]);
        assert_eq!(o.list[0].said, "Its change hadn't landed");
        assert_eq!(o.began, vec![crate::turns::Count { key: "you".into(), n: 1 }]);
        // Another account's has none of it.
        assert_eq!(outcomes_in(&db, None, 7, Some("codex"), now).unwrap().left, 0);
    }

    #[test]
    fn a_session_left_open_with_work_undone_hangs() {
        let now = now_ms();
        let old = now - 5 * DAY_MS;
        let asks = Session { asks: true, last_activity: old, ..session("q", Status::Needs) };
        let fresh = Session { asks: true, last_activity: now - DAY_MS, ..session("f", Status::Needs) };
        let working = Session { last_activity: old, ..session("w", Status::Working) };
        let nothing = Session { last_activity: old, ..session("n", Status::Idle) };
        let archived = Session { asks: true, archived: true, last_activity: old, ..session("a", Status::Needs) };
        let h = hanging(&board(vec![asks, fresh, working, nothing, archived]), None, now);
        assert_eq!(h.len(), 1);
        assert_eq!((h[0].standing.as_str(), h[0].said.as_str()), ("asks", "Its last reply asks you something"));
    }
}
