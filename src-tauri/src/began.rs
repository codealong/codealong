//! Where each session came from, kept in the ledger (`sessions.began`, with `[experimental]
//! analytics` on): you started it, it's a copy of another, another session's agent started it
//! (as its helper or not), Codealong did (a schedule's run, an automatic review), or it's a
//! headless run. The board says so of each session while it lists it (`Session::origin`,
//! `helper`, `headless`, and the schedules' own sessions), but only for as long as it does, and
//! origins.json forgets after 90 days; a year's report needs it for as long as the calls are kept.
//!
//! The engine hands over what it sees as it publishes a board (`note`), and the ledger's thread
//! writes what changed (`apply`) on its next pass, so the engine never waits on the file.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use anyhow::Result;
use rusqlite::{params, Connection};

use crate::model::{Board, OriginHow, Session};
use crate::turns::Source;

/// How a session began.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Began {
    /// You started it, in a terminal, an app or Codealong.
    You,
    /// A copy of another session's conversation, going on by itself.
    Fork,
    /// Another session's agent started it, for itself to read (`codealong start --helper`).
    Helper,
    /// Another session's agent started it as a session of its own.
    Agent,
    /// Codealong's automatic review of a pull request.
    Review,
    /// A schedule's run.
    Schedule,
    /// A headless run (`claude -p`, `codex exec`), a script's or an agent's.
    Headless,
}

pub const ALL: [Began; 7] = [Began::You, Began::Fork, Began::Helper, Began::Agent, Began::Review, Began::Schedule, Began::Headless];

impl Began {
    /// Its word in the ledger.
    pub fn word(self) -> &'static str {
        match self {
            Began::You => "you",
            Began::Fork => "fork",
            Began::Helper => "helper",
            Began::Agent => "agent",
            Began::Review => "review",
            Began::Schedule => "schedule",
            Began::Headless => "headless",
        }
    }

    pub fn from_word(word: &str) -> Option<Began> {
        ALL.into_iter().find(|b| b.word() == word)
    }

    /// Who its first prompt is from, whatever its record says of it: a session an agent or
    /// Codealong started opens with their words, typed in as yours would be. A headless run's is
    /// its starter's when a session ran it (`by`), and otherwise a script of yours.
    pub fn opens_as(self, by: Option<&str>) -> Source {
        match self {
            Began::You | Began::Fork => Source::You,
            Began::Helper | Began::Agent => Source::Agent,
            Began::Review | Began::Schedule => Source::Codealong,
            Began::Headless if by.is_some() => Source::Agent,
            Began::Headless => Source::You,
        }
    }

}

/// How a session began and what it came from: the session it copies or that started it, the PR
/// it reviews, the schedule that ran it (`scheduled`, by session key). What a helper is for comes
/// first: a review is started by Codealong however its session came about.
pub fn began_of(s: &Session, scheduled: &HashMap<&str, &str>) -> (Began, Option<String>) {
    if let Some(h) = &s.helper {
        if let Some(pr) = &h.pr {
            return (Began::Review, Some(pr.clone()));
        }
        return (Began::Helper, h.parent.clone().or_else(|| s.origin.as_ref().map(|o| o.key.clone())));
    }
    if let Some(id) = scheduled.get(s.key.as_str()) {
        return (Began::Schedule, Some(id.to_string()));
    }
    let from = s.origin.as_ref().map(|o| o.key.clone());
    if s.headless {
        return (Began::Headless, s.origin.as_ref().filter(|o| o.how == OriginHow::Started).map(|o| o.key.clone()));
    }
    match s.origin.as_ref().map(|o| o.how) {
        Some(OriginHow::Started) => (Began::Agent, from),
        Some(OriginHow::Fork) if from.as_deref().is_some_and(|f| moved(f, &s.key)) => (Began::You, None),
        Some(OriginHow::Fork) => (Began::Fork, from),
        None => (Began::You, None),
    }
}

/// A session moved into another account is noted as a fork of the one it left (start.rs
/// `move_to`), but it's the same work carried on, so it counts as one you began, not as a fork
/// (asked 2026-11-12). It's told by the accounts alone, as the windows tell it (lineage.ts
/// `kinHow`), so a copy in the same account (a move to another folder, a background job Claude
/// Code copies as it's resumed, `/background`) still counts as a fork.
fn moved(from: &str, to: &str) -> bool {
    crate::origins::is_move(from, to)
}

/// What's kept of a session: how it began, and what a list needs to name it once it's gone
/// from the board.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Noted {
    pub key: String,
    pub account: String,
    pub folder: String,
    pub began: Began,
    pub by: Option<String>,
    pub title: String,
    pub started: Option<i64>,
}

#[derive(Default)]
struct Notes {
    /// What was last handed over of each session, so only a change is written.
    seen: HashMap<String, Noted>,
    /// What the ledger's thread has yet to write.
    waiting: Vec<Noted>,
}

fn notes() -> &'static Mutex<Notes> {
    static NOTES: OnceLock<Mutex<Notes>> = OnceLock::new();
    NOTES.get_or_init(Default::default)
}

/// What a board says of each of this machine's sessions, with the schedules' own sessions
/// (`schedules::own_sessions`: a schedule's id and its sessions' keys).
pub fn noted_on(board: &Board, schedules: &std::collections::BTreeMap<String, Vec<String>>) -> Vec<Noted> {
    let scheduled: HashMap<&str, &str> = schedules.iter().flat_map(|(id, keys)| keys.iter().map(move |k| (k.as_str(), id.as_str()))).collect();
    board
        .lanes
        .iter()
        .filter(|l| l.device.is_none())
        .flat_map(|l| &l.sessions)
        .filter(|s| s.visitor.is_none())
        .map(|s| {
            let (began, by) = began_of(s, &scheduled);
            Noted { key: s.key.clone(), account: s.account.clone(), folder: s.cwd.clone(), began, by, title: s.title.clone(), started: s.started_at }
        })
        .collect()
}

/// How often the engine hands over what its board says: it publishes far more often than a
/// session's beginning changes.
const EVERY_MS: i64 = 20_000;

/// Whether it's time for the engine to hand over what its board says (`note`), which it then
/// does.
pub fn due() -> bool {
    static LAST: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);
    let now = crate::util::now_ms();
    let last = LAST.load(std::sync::atomic::Ordering::Relaxed);
    now - last >= EVERY_MS && LAST.compare_exchange(last, now, std::sync::atomic::Ordering::Relaxed, std::sync::atomic::Ordering::Relaxed).is_ok()
}

/// Takes what a board says (`noted_on`), keeping what's new or changed for the ledger's thread.
/// Called as the engine publishes, so it only compares.
pub fn note(list: Vec<Noted>) {
    let mut notes = notes().lock().unwrap_or_else(|e| e.into_inner());
    for n in list {
        if notes.seen.get(&n.key) != Some(&n) {
            notes.seen.insert(n.key.clone(), n.clone());
            notes.waiting.push(n);
        }
    }
}

/// Writes what's waiting into the ledger: each session's row, made if it has none (a session
/// whose harness records no tokens has no calls to make it). Says which sessions' beginnings
/// changed, by their row ids, for what's worked out from them (turns.rs `recount`).
pub fn apply(db: &Connection) -> Result<Vec<i64>> {
    let waiting = std::mem::take(&mut notes().lock().unwrap_or_else(|e| e.into_inner()).waiting);
    put(db, &waiting)
}

fn put(db: &Connection, list: &[Noted]) -> Result<Vec<i64>> {
    if list.is_empty() {
        return Ok(Vec::new());
    }
    let tx = db.unchecked_transaction()?;
    let mut changed = Vec::new();
    for n in list {
        let id = crate::ledger::session_row(&tx, &n.key, &n.account, "")?;
        let was: (Option<String>, Option<String>) = tx.query_row("SELECT began, began_by FROM sessions WHERE id = ?1", [id], |r| Ok((r.get(0)?, r.get(1)?)))?;
        if was.0.as_deref() != Some(n.began.word()) || was.1 != n.by {
            changed.push(id);
        }
        // The folder a session works in is its calls' to say; the board's is only for one with none.
        tx.execute(
            "UPDATE sessions SET began = ?2, began_by = ?3, title = ?4, started = coalesce(?5, started), folder = CASE WHEN folder = '' THEN ?6 ELSE folder END WHERE id = ?1",
            params![id, n.began.word(), n.by, n.title, n.started, n.folder],
        )?;
    }
    tx.commit()?;
    Ok(changed)
}

/// Sessions the ledger has calls of and nothing noted, from origins.json, which remembers 90
/// days of forks and starts: once, the first time analytics are on, for the sessions the board
/// no longer lists. One origins.json says nothing of stays unknown rather than yours: a
/// schedule's run or a review from before then would read as yours otherwise.
pub fn backfill(db: &Connection, origins: &crate::origins::Store) -> Result<()> {
    if crate::ledger::meta(db, "began")?.is_some() {
        return Ok(());
    }
    let tx = db.unchecked_transaction()?;
    for (key, entry) in &origins.sessions {
        let (began, by) = match (entry.helper, entry.how) {
            (true, _) => (Began::Helper, Some(&entry.from)),
            (false, OriginHow::Started) => (Began::Agent, Some(&entry.from)),
            (false, OriginHow::Fork) if moved(&entry.from, key) => (Began::You, None),
            (false, OriginHow::Fork) => (Began::Fork, Some(&entry.from)),
        };
        tx.execute("UPDATE sessions SET began = ?2, began_by = ?3 WHERE key = ?1 AND began IS NULL", params![key, began.word(), by])?;
    }
    crate::ledger::set_meta(&tx, "began", "1")?;
    tx.commit()?;
    Ok(())
}

/// Sessions moved into another account that were noted as forks before a move counted as yours
/// (`moved`): once. The board's sessions are noted again as it lists them; these are the rest.
/// Their turns needn't be counted again, since a fork's first prompt is yours as well
/// (`Began::opens_as`).
pub fn moves_are_yours(db: &Connection) -> Result<()> {
    if crate::ledger::meta(db, "began-moves")?.is_some() {
        return Ok(());
    }
    let tx = db.unchecked_transaction()?;
    let forks: Vec<(i64, String, String)> = tx
        .prepare("SELECT id, key, began_by FROM sessions WHERE began = 'fork' AND began_by IS NOT NULL")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<rusqlite::Result<_>>()?;
    for (id, key, by) in forks {
        if moved(&by, &key) {
            tx.execute("UPDATE sessions SET began = 'you', began_by = NULL WHERE id = ?1", [id])?;
        }
    }
    crate::ledger::set_meta(&tx, "began-moves", "1")?;
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Helper, SessionOrigin};

    fn session(key: &str) -> Session {
        Session { key: key.into(), account: "claude".into(), title: "Fix the parser".into(), cwd: "/work/app".into(), ..crate::fixtures::session(key) }
    }

    #[test]
    fn a_session_began_as_the_board_says() {
        let none = HashMap::new();
        let from = |how| Some(SessionOrigin { key: "claude:claude:parent".into(), how });
        assert_eq!(began_of(&session("a"), &none), (Began::You, None));
        assert_eq!(began_of(&Session { origin: from(OriginHow::Fork), ..session("a") }, &none), (Began::Fork, Some("claude:claude:parent".into())));
        assert_eq!(began_of(&Session { origin: from(OriginHow::Started), ..session("a") }, &none), (Began::Agent, Some("claude:claude:parent".into())));
        // Moved into another account it's yours; a copy in the same account is a fork.
        let copy = |key: &str, parent: &str| Session { origin: Some(SessionOrigin { key: parent.into(), how: OriginHow::Fork }), ..session(key) };
        assert_eq!(began_of(&copy("claude:claude2:b", "claude:claude:a"), &none), (Began::You, None));
        assert_eq!(began_of(&copy("claude:claude:b", "claude:claude:a"), &none), (Began::Fork, Some("claude:claude:a".into())));
        let helper = Session { origin: from(OriginHow::Started), helper: Some(Helper { parent: Some("claude:claude:parent".into()), ..Default::default() }), ..session("a") };
        assert_eq!(began_of(&helper, &none), (Began::Helper, Some("claude:claude:parent".into())));
        // A review is Codealong's, though a helper too; so is a schedule's run.
        let review = Session { helper: Some(Helper { pr: Some("northwind/app#7".into()), ..Default::default() }), ..session("a") };
        assert_eq!(began_of(&review, &none), (Began::Review, Some("northwind/app#7".into())));
        let key = session("a").key;
        assert_eq!(began_of(&session("a"), &HashMap::from([(key.as_str(), "s-1")])), (Began::Schedule, Some("s-1".into())));
        // A headless run is its starter's when a session ran it, a script of yours otherwise.
        let run = Session { headless: true, origin: from(OriginHow::Started), ..session("a") };
        assert_eq!(began_of(&run, &none), (Began::Headless, Some("claude:claude:parent".into())));
        assert_eq!(began_of(&Session { headless: true, ..session("a") }, &none), (Began::Headless, None));
        assert_eq!(Began::Headless.opens_as(Some("x")), Source::Agent);
        assert_eq!(Began::Headless.opens_as(None), Source::You);
        for b in ALL {
            assert_eq!(Began::from_word(b.word()), Some(b));
        }
    }

    #[test]
    fn what_changed_of_a_session_is_written_once() {
        let db = crate::ledger::in_memory();
        let noted = |began, by: Option<&str>| Noted { key: "claude:claude:a".into(), account: "claude".into(), folder: "/work/app".into(), began, by: by.map(String::from), title: "Fix the parser".into(), started: Some(5) };
        // A session with no calls yet gets its row, and says its beginning changed.
        let changed = put(&db, &[noted(Began::You, None)]).unwrap();
        assert_eq!(changed.len(), 1);
        let row = |db: &Connection| db.query_row("SELECT began, began_by, title, started, folder FROM sessions", [], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, String>(2)?, r.get::<_, i64>(3)?, r.get::<_, String>(4)?))).unwrap();
        assert_eq!(row(&db), ("you".into(), None, "Fix the parser".into(), 5, "/work/app".into()));
        // The same again changes nothing; found to be an agent's, it does.
        assert!(put(&db, &[noted(Began::You, None)]).unwrap().is_empty());
        assert_eq!(put(&db, &[noted(Began::Agent, Some("claude:claude:p"))]).unwrap(), changed);
        assert_eq!(row(&db).0, "agent");
        // origins.json fills in only what nothing was noted of, once.
        db.execute("INSERT INTO sessions (key, account) VALUES ('claude:claude:old', 'claude')", []).unwrap();
        let mut origins = crate::origins::Store::default();
        for key in ["claude:claude:a", "claude:claude:old"] {
            origins.sessions.insert(key.into(), crate::origins::Entry { from: "claude:claude:p".into(), how: OriginHow::Fork, at: 1, helper: false, report: false, known: false });
        }
        // One moved into another account is yours.
        db.execute("INSERT INTO sessions (key, account) VALUES ('claude:claude2:moved', 'claude2')", []).unwrap();
        origins.sessions.insert("claude:claude2:moved".into(), crate::origins::Entry { from: "claude:claude:p".into(), how: OriginHow::Fork, at: 1, helper: false, report: false, known: false });
        backfill(&db, &origins).unwrap();
        let began = |key: &str| db.query_row("SELECT began, began_by FROM sessions WHERE key = ?1", [key], |r| Ok((r.get::<_, Option<String>>(0)?, r.get::<_, Option<String>>(1)?))).unwrap();
        assert_eq!(began("claude:claude:a").0, Some("agent".into()));
        assert_eq!(began("claude:claude:old"), (Some("fork".into()), Some("claude:claude:p".into())));
        assert_eq!(began("claude:claude2:moved"), (Some("you".into()), None));
    }

    #[test]
    fn moves_noted_as_forks_before_are_yours_once() {
        let db = crate::ledger::in_memory();
        for (key, by) in [("claude:claude2:moved", "claude:claude:p"), ("claude:claude:fork", "claude:claude:p"), ("codex:codex:fork", "claude:claude:p")] {
            db.execute("INSERT INTO sessions (key, account, began, began_by) VALUES (?1, '', 'fork', ?2)", [key, by]).unwrap();
        }
        moves_are_yours(&db).unwrap();
        let began = |key: &str| db.query_row("SELECT began, began_by FROM sessions WHERE key = ?1", [key], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))).unwrap();
        assert_eq!(began("claude:claude2:moved"), ("you".into(), None));
        // A fork in the same account, or from another harness, stays one.
        assert_eq!(began("claude:claude:fork"), ("fork".into(), Some("claude:claude:p".into())));
        assert_eq!(began("codex:codex:fork").0, "fork");
        // It runs once: a later fork noted by another rule isn't touched.
        db.execute("UPDATE sessions SET began = 'fork', began_by = 'claude:claude:p' WHERE key = 'claude:claude2:moved'", []).unwrap();
        moves_are_yours(&db).unwrap();
        assert_eq!(began("claude:claude2:moved").0, "fork");
    }
}
