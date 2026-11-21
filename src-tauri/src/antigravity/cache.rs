//! Coalesced database reads and settled-file caching for Antigravity conversations.
//!
//! Each open `agy` terminal holds a presence lock even when idle, so a reread triggered by another
//! conversation writing `conversation_summaries.db` used to re-read every idle terminal's 256 KB
//! transcript tail and open its `conversations/<id>.db` four separate times. Reading a
//! conversation's DB in one open (`read_db_snapshot`) and reusing cached transcript and DB results
//! once their files have settled (`ConvCache`) keeps rereads fast when many `agy` terminals sit
//! open.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use rusqlite::Connection;

use super::{ends_turn, first_request, mode_of, model_of, step_began, tail, transcript_file, turn_failed, Conversation, Failed, SessionMode, STEP_CANCELED, STEP_DONE, STEP_ENDS_TURN, STEP_ERROR, STEP_UNDER_WAY, STEP_USER_INPUT, STEP_WAITING};

/// Minimum time since a file's last modification before its size + mtime is trusted to reuse
/// cached data, avoiding missed writes within filesystem mtime granularity.
pub(super) const SETTLE_AGE: Duration = Duration::from_secs(2);

pub(super) fn db_path(dir: &Path, id: &str) -> PathBuf {
    dir.join("conversations").join(format!("{id}.db"))
}

/// Everything `Reader::reread` reads from `conversations/<id>.db`, fetched in a single open.
#[derive(Debug, Default)]
pub(super) struct DbSnapshot {
    pub mode: Option<SessionMode>,
    pub prompted_at: Option<i64>,
    /// `None` when the `steps` table has no rows.
    pub latest_step: Option<(Option<i64>, Option<Failed>)>,
    pub model: Option<String>,
    pub step_under_way: bool,
    pub waiting: bool,
    pub turn_ended: bool,
}

/// Reads prompt settings, latest step, model, and (when `want_steps_now`) active step status from
/// `conversations/<id>.db` in one open. Returns `None` if the database doesn't exist or can't be
/// opened.
pub(super) fn read_db_snapshot(dir: &Path, id: &str, written: i64, want_steps_now: bool) -> Option<DbSnapshot> {
    let db = crate::harness::open_database(&db_path(dir, id))?;
    let prompt: Option<Vec<u8>> = db.query_row("SELECT step_payload FROM steps WHERE step_type = ?1 ORDER BY idx DESC LIMIT 1", [STEP_USER_INPUT], |r| r.get(0)).ok();
    let mode = prompt.as_deref().and_then(mode_of);
    let prompted_at = prompt.as_deref().and_then(step_began);

    let latest = latest_step_row(&db);
    let latest_step = latest.as_ref().map(|(kind, status, payload)| {
        let failed = payload.as_deref().filter(|_| *kind == STEP_ERROR && *status == STEP_DONE).and_then(|p| turn_failed(p, written));
        (payload.as_deref().and_then(step_began), failed)
    });

    let (step_under_way, waiting, turn_ended) = if want_steps_now {
        let (under, wait) = active_statuses(&db);
        let ended = latest.as_ref().is_some_and(|(kind, status, payload)| *status == STEP_CANCELED || (*status == STEP_DONE && STEP_ENDS_TURN.contains(kind) && (*kind != STEP_ERROR || ends_turn(payload.as_deref()))));
        (under, wait, ended)
    } else {
        (false, false, false)
    };

    let model = db.query_row("SELECT data FROM executor_metadata ORDER BY idx DESC LIMIT 1", [], |r| r.get::<_, Vec<u8>>(0)).ok().and_then(|row| model_of(&row));

    Some(DbSnapshot { mode, prompted_at, latest_step, model, step_under_way, waiting, turn_ended })
}

/// Active step status and latest step's turn-ending state alone (for a running conversation whose
/// summary row didn't change).
pub(super) fn steps_now(dir: &Path, id: &str) -> (bool, bool, bool, Option<i64>) {
    let Some(db) = crate::harness::open_database(&db_path(dir, id)) else { return (false, false, false, None) };
    let (under, wait) = active_statuses(&db);
    let latest = latest_step_row(&db);
    let ended = latest.as_ref().is_some_and(|(kind, status, payload)| *status == STEP_CANCELED || (*status == STEP_DONE && STEP_ENDS_TURN.contains(kind) && (*kind != STEP_ERROR || ends_turn(payload.as_deref()))));
    let at = latest.as_ref().and_then(|(_, _, payload)| payload.as_deref().and_then(step_began));
    (under, wait, ended, at)
}

fn latest_step_row(db: &Connection) -> Option<(i64, i64, Option<Vec<u8>>)> {
    db.query_row("SELECT step_type, status, step_payload FROM steps ORDER BY idx DESC LIMIT 1", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .or_else(|_| db.query_row("SELECT step_type, status FROM steps ORDER BY idx DESC LIMIT 1", [], |r| Ok((r.get(0)?, r.get(1)?, None))))
        .ok()
}

fn active_statuses(db: &Connection) -> (bool, bool) {
    let under_way = STEP_UNDER_WAY.map(|s| s.to_string()).join(", ");
    let Ok(mut query) = db.prepare(&format!("SELECT DISTINCT status FROM steps WHERE status IN ({under_way})")) else { return (false, false) };
    let statuses: Vec<i64> = query.query_map([], |r| r.get(0)).map(|rows| rows.flatten().collect()).unwrap_or_default();
    (!statuses.is_empty(), statuses.contains(&STEP_WAITING))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct FileStamp {
    len: u64,
    mtime: SystemTime,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DbStamp {
    db: FileStamp,
    wal: Option<FileStamp>,
}

fn settled_stamp(path: &Path, now: SystemTime) -> Option<FileStamp> {
    let meta = std::fs::metadata(path).ok()?;
    let mtime = meta.modified().ok()?;
    now.duration_since(mtime).ok().filter(|&age| age >= SETTLE_AGE)?;
    Some(FileStamp { len: meta.len(), mtime })
}

/// Settled stamp of `conversations/<id>.db` and its WAL, or `None` when the DB is missing, has an
/// active rollback journal, or either file was modified more recently than `SETTLE_AGE`.
pub(super) fn settled_db_stamp(dir: &Path, id: &str, now: SystemTime) -> Option<DbStamp> {
    let db_file = db_path(dir, id);
    let journal = PathBuf::from(format!("{}-journal", db_file.to_string_lossy()));
    if std::fs::metadata(&journal).is_ok() {
        return None;
    }
    let db = settled_stamp(&db_file, now)?;
    let wal_file = PathBuf::from(format!("{}-wal", db_file.to_string_lossy()));
    let wal = match std::fs::metadata(&wal_file) {
        Ok(meta) => {
            let mtime = meta.modified().ok()?;
            now.duration_since(mtime).ok().filter(|&age| age >= SETTLE_AGE)?;
            Some(FileStamp { len: meta.len(), mtime })
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(_) => return None,
    };
    Some(DbStamp { db, wal })
}

pub(super) fn settled_file_stamp(path: &Path, now: SystemTime) -> Option<FileStamp> {
    settled_stamp(path, now)
}

#[derive(Debug, Default)]
struct ConvMarks {
    transcript: Option<(PathBuf, FileStamp, Option<String>)>,
    db: Option<(DbStamp, bool)>,
}

/// Per-conversation settled file marks for `Reader::reread`, keyed by `(folder, conversation_id)`.
#[derive(Debug, Default)]
pub(super) struct ConvCache {
    by_conv: HashMap<(PathBuf, String), ConvMarks>,
    #[cfg(test)]
    pub(super) transcript_hits: usize,
    #[cfg(test)]
    pub(super) db_hits: usize,
}

impl ConvCache {
    pub(super) fn clear(&mut self) {
        self.by_conv.clear();
    }

    /// Populates `c`'s transcript and per-conversation DB fields, reusing settled cached values
    /// from `old` when the underlying files haven't changed and aren't actively being written.
    pub(super) fn refresh(&mut self, c: &mut Conversation, old: Option<&Conversation>, held: bool, want_steps_now: bool, now_sys: SystemTime) {
        // An idle held conversation whose log didn't change and wasn't mid-turn can reuse settled
        // transcript and per-conversation DB reads without re-reading 256 KB tails or re-opening
        // SQLite on every reread triggered by another conversation.
        let settled_ok = old.is_some_and(|o| !c.running() && !o.idled_by_another && !o.ui_turn_open() && c.said == o.said && (held || !o.read_again));

        let record = transcript_file(&c.dir, &c.id);
        let t_stamp = record.as_deref().and_then(|p| settled_file_stamp(p, now_sys));
        let reused_transcript = match (old.filter(|_| settled_ok), record.as_deref(), t_stamp.as_ref()) {
            (Some(o), Some(path), Some(stamp)) if self.transcript_matches(&c.dir, &c.id, path, stamp, c.cwd.as_deref()) => {
                c.tail = o.tail.clone();
                c.asked = o.asked.clone();
                c.started_at = o.started_at;
                #[cfg(test)]
                {
                    self.transcript_hits += 1;
                }
                true
            }
            _ => false,
        };
        if !reused_transcript {
            c.tail = record.as_deref().map(|p| tail(p, c.cwd.as_deref())).unwrap_or_default();
            // What it was first asked, and when, don't change once written.
            if c.asked.is_none() {
                if let Some(o) = old.filter(|o| o.asked.is_some()) {
                    c.asked = o.asked.clone();
                    c.started_at = o.started_at;
                } else if let Some((asked, at)) = record.as_deref().and_then(first_request) {
                    c.asked = Some(asked);
                    c.started_at = at;
                }
            }
            self.note_transcript(&c.dir, &c.id, record.zip(t_stamp).map(|(p, s)| (p, s, c.cwd.clone())));
        }

        let db_stamp = settled_db_stamp(&c.dir, &c.id, now_sys);
        let reuse_db = old
            .filter(|o| settled_ok && (o.failed.is_none() || o.last_modified == c.last_modified))
            .filter(|_| db_stamp.as_ref().is_some_and(|s| self.db_matches(&c.dir, &c.id, s, want_steps_now)));
        if let Some(o) = reuse_db {
            c.mode = o.mode.clone();
            c.prompted_at = o.prompted_at;
            c.latest_step_at = o.latest_step_at;
            c.failed = o.failed.clone();
            c.model = o.model.clone();
            if want_steps_now {
                (c.step_under_way, c.waiting, c.turn_ended) = (o.step_under_way, o.waiting, o.turn_ended);
            }
            #[cfg(test)]
            {
                self.db_hits += 1;
            }
        } else {
            let snap = read_db_snapshot(&c.dir, &c.id, c.last_modified, want_steps_now);
            c.mode = snap.as_ref().and_then(|s| s.mode.clone());
            // Steps stay once written, so a read that finds none, or an older copy of the
            // database (`open_database`'s fallback leaves out what its WAL holds), doesn't
            // take a time back. The latest step's says when it was last written, which a
            // row another `agy` wrote back doesn't.
            c.prompted_at = snap.as_ref().and_then(|s| s.prompted_at).max(old.and_then(|o| o.prompted_at));
            // The note a failed turn ends with never reaches the transcript. Where a step
            // was read before, a read that finds none, or a latest step older than that
            // one, is none at all or an older copy: how the latest turn ended stays as it
            // was read. One with no step yet, or no database of its own, isn't behind.
            let kept = old.and_then(|o| o.latest_step_at);
            let read = snap.as_ref().and_then(|s| s.latest_step.as_ref());
            let at = read.and_then(|(at, _)| *at);
            let behind = kept.is_some_and(|kept| read.is_none() || at.is_some_and(|at| at < kept));
            c.failed = if behind { old.and_then(|o| o.failed.clone()) } else { read.and_then(|(_, failed)| failed.clone()) };
            c.latest_step_at = at.max(kept);
            c.read_again |= behind;
            c.model = snap.as_ref().and_then(|s| s.model.clone());
            if let Some(s) = snap.as_ref().filter(|_| want_steps_now) {
                (c.step_under_way, c.waiting, c.turn_ended) = (s.step_under_way, s.waiting, s.turn_ended);
            }
            self.note_db(&c.dir, &c.id, (!behind).then_some(db_stamp).flatten().map(|s| (s, want_steps_now)));
        }
    }

    fn transcript_matches(&self, dir: &Path, id: &str, path: &Path, stamp: &FileStamp, cwd: Option<&str>) -> bool {
        self.by_conv
            .get(&(dir.to_path_buf(), id.to_string()))
            .and_then(|m| m.transcript.as_ref())
            .is_some_and(|(p, s, c)| p == path && s == stamp && c.as_deref() == cwd)
    }

    fn note_transcript(&mut self, dir: &Path, id: &str, mark: Option<(PathBuf, FileStamp, Option<String>)>) {
        self.by_conv.entry((dir.to_path_buf(), id.to_string())).or_default().transcript = mark;
    }

    fn db_matches(&self, dir: &Path, id: &str, stamp: &DbStamp, want_steps_now: bool) -> bool {
        self.by_conv
            .get(&(dir.to_path_buf(), id.to_string()))
            .and_then(|m| m.db.as_ref())
            .is_some_and(|(s, had_steps)| s == stamp && (!want_steps_now || *had_steps))
    }

    fn note_db(&mut self, dir: &Path, id: &str, mark: Option<(DbStamp, bool)>) {
        self.by_conv.entry((dir.to_path_buf(), id.to_string())).or_default().db = mark;
    }

    pub(super) fn retain_keys(&mut self, keep: &HashSet<(PathBuf, String)>) {
        self.by_conv.retain(|k, _| keep.contains(k));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ModeKind;

    fn varint(mut n: u64) -> Vec<u8> {
        let mut out = Vec::new();
        loop {
            let mut b = (n & 0x7f) as u8;
            n >>= 7;
            if n != 0 {
                b |= 0x80;
            }
            out.push(b);
            if n == 0 {
                break;
            }
        }
        out
    }

    fn msg(fields: &[(u64, Vec<u8>)]) -> Vec<u8> {
        let mut out = Vec::new();
        for (f, bytes) in fields {
            out.extend(varint((f << 3) | 2));
            out.extend(varint(bytes.len() as u64));
            out.extend(bytes);
        }
        out
    }

    fn num(field: u64, value: u64) -> Vec<u8> {
        let mut out = varint(field << 3);
        out.extend(varint(value));
        out
    }

    #[test]
    fn coalesced_db_snapshot_reads_prompt_latest_step_model_and_active_statuses() {
        let dir = crate::fixtures::scratch("agy-cache-snapshot");
        std::fs::create_dir_all(dir.join("conversations")).unwrap();
        let db = Connection::open(dir.join("conversations/c1.db")).unwrap();
        db.execute_batch(
            "CREATE TABLE steps (idx INTEGER PRIMARY KEY, step_type INTEGER NOT NULL DEFAULT 0, status INTEGER NOT NULL DEFAULT 0, step_payload BLOB);
             CREATE TABLE executor_metadata (idx INTEGER PRIMARY KEY, data BLOB);",
        )
        .unwrap();

        let ts = |secs: u64| msg(&[(5, msg(&[(1, [num(1, secs), num(2, 0)].concat())]))]);
        let prompt_payload = {
            let tools = msg(&[(8, msg(&[(3, num(6, 3))]))]);
            let mut p = msg(&[(19, msg(&[(12, msg(&[(1, msg(&[(13, tools)]))]))]))]);
            p.extend(ts(1_700_000_001));
            p
        };
        let model_row = msg(&[(10, msg(&[(1, msg(&[(28, b"gemini-3.8-flash-high".to_vec())]))]))]);
        db.execute("INSERT INTO steps VALUES (0, ?1, ?2, ?3)", rusqlite::params![STEP_USER_INPUT, STEP_DONE, prompt_payload]).unwrap();
        db.execute("INSERT INTO steps VALUES (1, 15, ?1, ?2)", rusqlite::params![STEP_DONE, ts(1_700_000_005)]).unwrap();
        db.execute("INSERT INTO executor_metadata VALUES (0, ?1)", rusqlite::params![model_row]).unwrap();

        let snap = read_db_snapshot(&dir, "c1", 999, true).expect("snapshot");
        assert_eq!(snap.mode.as_ref().map(|m| m.kind), Some(ModeKind::Skips));
        assert_eq!(snap.prompted_at, Some(1_700_000_001_000));
        assert_eq!(snap.latest_step, Some((Some(1_700_000_005_000), None)));
        assert_eq!(snap.model.as_deref(), Some("gemini-3.8-flash-high"));
        assert!(!snap.step_under_way && !snap.waiting && snap.turn_ended);

        // Waiting step flips step_under_way and waiting and clears turn_ended.
        db.execute("INSERT INTO steps VALUES (2, 20, ?1, ?2)", rusqlite::params![STEP_WAITING, ts(1_700_000_009)]).unwrap();
        let snap = read_db_snapshot(&dir, "c1", 999, true).expect("snapshot with waiting");
        assert!(snap.step_under_way && snap.waiting && !snap.turn_ended);
        assert_eq!(snap.latest_step, Some((Some(1_700_000_009_000), None)));

        assert!(read_db_snapshot(&dir, "missing", 0, true).is_none());
    }

    #[test]
    fn settled_stamp_waits_for_settle_age_and_invalidates_on_wal_or_journal() {
        let dir = crate::fixtures::scratch("agy-cache-stamp");
        std::fs::create_dir_all(dir.join("conversations")).unwrap();
        let db_file = dir.join("conversations/c1.db");
        std::fs::write(&db_file, b"db").unwrap();
        let mtime = std::fs::metadata(&db_file).unwrap().modified().unwrap();

        // Unsettled immediately after write; settled once SETTLE_AGE has passed.
        assert!(settled_db_stamp(&dir, "c1", mtime).is_none());
        let later = mtime + SETTLE_AGE + Duration::from_secs(1);
        let first = settled_db_stamp(&dir, "c1", later).expect("settled db");

        // Adding a WAL changes the stamp (and requires the WAL to settle too).
        let wal = dir.join("conversations/c1.db-wal");
        std::fs::write(&wal, b"wal").unwrap();
        let wal_mtime = std::fs::metadata(&wal).unwrap().modified().unwrap();
        assert!(settled_db_stamp(&dir, "c1", wal_mtime).is_none());
        let with_wal = settled_db_stamp(&dir, "c1", wal_mtime + SETTLE_AGE + Duration::from_secs(1)).expect("settled with wal");
        assert_ne!(first, with_wal);

        // An active rollback journal prevents caching altogether.
        let journal = dir.join("conversations/c1.db-journal");
        std::fs::write(&journal, b"j").unwrap();
        assert!(settled_db_stamp(&dir, "c1", wal_mtime + Duration::from_secs(60)).is_none());
    }
}
