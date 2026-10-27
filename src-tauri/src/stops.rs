//! What Codealong did that ends a session's turn or cuts its step short, a line for each in
//! `~/Library/Logs/Codealong/stops.log`: Stop, the stop a move makes first, Mark as done ending
//! a session whose turn runs (its turn stopped, its terminal closed or its job stopped), Send now
//! on a message the session holds where that stops the step it's on
//! (`Adapter::steer_queued_stops`), and a call denied. What it was, which session, who asked (a
//! window, the `codealong` command and the session that ran it, another device) and how it went
//! (`noted`).
//!
//! A turn cut short leaves only its harness's mark in its record, the same whoever pressed the
//! key: Claude Code's "User rejected tool use" and `[Request interrupted by user for tool use]`,
//! Codex's `turn_aborted`. So the conversation's note for it says it was Codealong's from here
//! (`attribute`), and says nothing of who when nothing here matches: a stop pressed by mistake read
//! as a call refused that nobody refused (asked 2026-10-26).
//!
//! A line is tab-separated: the local time, what it was, the session's title, who asked, how it
//! went, the session's key, and when it was asked and when it was done in epoch milliseconds,
//! which `attribute` reads.

use std::io::Write;
use std::path::{Path, PathBuf};

use crate::actions::From;
use crate::model::Board;
use crate::transcript::Page;

/// What Codealong did to the session's turn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Stop {
    /// Stop: the box's button, ⌘., the menu's Stop turn, `codealong session stop`.
    Stop,
    /// The stop a move makes first.
    Move,
    /// Mark as done ending a session whose turn runs, the session's own or its helpers'.
    Done,
    /// Send now on a message the session holds.
    SendNow,
    /// A call denied.
    Deny,
}

const STOPS: [Stop; 5] = [Stop::Stop, Stop::Move, Stop::Done, Stop::SendNow, Stop::Deny];

impl Stop {
    /// Its word in the log.
    fn word(self) -> &'static str {
        match self {
            Stop::Stop => "stop",
            Stop::Move => "move",
            Stop::Done => "done",
            Stop::SendNow => "send-now",
            Stop::Deny => "deny",
        }
    }

    /// What the conversation's note says it was, ahead of who asked.
    fn said(self) -> &'static str {
        match self {
            Stop::Stop => "Stopped from Codealong",
            Stop::Move => "Stopped from Codealong to move it",
            Stop::Done => "Stopped from Codealong as it was marked done",
            Stop::SendNow => "Stopped from Codealong by Send now",
            Stop::Deny => "Denied from Codealong",
        }
    }
}

/// How long after a stop was done its harness's mark of it may come and still be taken for it:
/// the harness reads the key once it gets a turn on a busy Mac. A stop of another kind a few
/// seconds later, after a new turn has begun, is too soon to be told apart.
const LANDS_AFTER_MS: i64 = 10_000;
/// How long before a stop was asked: the mark's time and the log's are read at different moments.
const SKEW_MS: i64 = 2_000;
/// How much of the log's end `attribute` reads: thousands of lines.
const READ_BYTES: u64 = 512 * 1024;

fn log_path() -> PathBuf {
    crate::config::logs_dir().join("stops.log")
}

/// Who asked, as the log and the note say it: `board` names the session that ran the command.
pub fn who(board: &Board, from: &From) -> String {
    match from {
        From::Here => "a window on this Mac".into(),
        From::Phone => "your phone".into(),
        From::Device => "another device".into(),
        From::Command(Some(key)) => match board.session(key) {
            Some((_, s)) => format!("“{}”, with the codealong command", s.title),
            None => format!("{key}, with the codealong command"),
        },
        From::Command(None) => "the codealong command".into(),
    }
}

/// Does `act`, which ends the turn of the session `key` (titled `title`) as `what`, and logs it
/// with how it went. Not from tests, whose sessions are made up.
pub fn noted<T, E: std::fmt::Display>(what: Stop, key: &str, title: &str, who: &str, act: impl FnOnce() -> Result<T, E>) -> Result<T, E> {
    let at = crate::util::now_ms();
    let got = act();
    if !cfg!(test) {
        let failed = got.as_ref().err().map(|e| e.to_string());
        let asked = Asked { what, key, title, who, at, done: crate::util::now_ms() };
        append(&log_path(), &line(&crate::perf::stamp(), &asked, failed.as_deref()));
    }
    got
}

fn append(path: &Path, line: &str) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    // A stop that can't be logged still happened: the log is for finding out afterwards.
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = f.write_all(line.as_bytes());
    }
}

/// A stop as it's logged: what, of which session, who asked, and when it was asked and done.
struct Asked<'a> {
    what: Stop,
    key: &'a str,
    title: &'a str,
    who: &'a str,
    at: i64,
    done: i64,
}

/// The log's line for a stop, `failed` saying why it didn't go.
fn line(stamp: &str, a: &Asked, failed: Option<&str>) -> String {
    let flat = |s: &str| s.replace(['\t', '\n', '\r'], " ");
    let went = match failed {
        None => "done".to_string(),
        Some(why) => format!("failed: {}", flat(why)),
    };
    format!("{stamp}\t{}\t{}\t{}\t{went}\t{}\t{}\t{}\n", a.what.word(), flat(a.title), flat(a.who), flat(a.key), a.at, a.done)
}

/// A stop that went, read back from the log.
#[derive(Debug, PartialEq)]
struct Logged {
    what: Stop,
    who: String,
    key: String,
    at: i64,
    done: i64,
}

impl Logged {
    /// Whether a mark of a stop at `t` could be this one's.
    fn could_have_made(&self, t: i64) -> bool {
        self.at - SKEW_MS <= t && t <= self.done + LANDS_AFTER_MS
    }
}

/// The stops that went in the log's text, oldest first. A line of another shape is skipped.
fn logged(text: &str) -> Vec<Logged> {
    text.lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split('\t').collect();
            let [_, what, _, who, went, key, at, done] = f.as_slice() else { return None };
            let what = STOPS.into_iter().find(|s| s.word() == *what)?;
            (*went == "done").then_some(())?;
            Some(Logged { what, who: who.to_string(), key: key.to_string(), at: at.parse().ok()?, done: done.parse().ok()? })
        })
        .collect()
}

/// Says on each stop note in a page of the session `key`'s conversation that Codealong stopped
/// it, and who asked, where the log has a stop of that session that could have made it.
pub fn attribute(page: &mut Page, key: &str) {
    if page.items.iter().any(|it| it.stop) {
        attribute_from(page, key, &log_path());
    }
}

fn attribute_from(page: &mut Page, key: &str, log: &Path) {
    let Ok(len) = std::fs::metadata(log).map(|m| m.len()) else { return };
    let Some(text) = crate::util::read_range(log, len.saturating_sub(READ_BYTES), len) else { return };
    let stops: Vec<Logged> = logged(&text).into_iter().filter(|l| l.key == key).collect();
    let mut used = vec![false; stops.len()];
    for it in page.items.iter_mut().filter(|it| it.stop) {
        // Each stop makes one mark: the latest asked before it, else the first asked just after,
        // its clock read a moment late.
        let open = |i: &usize| !used[*i] && stops[*i].could_have_made(it.at);
        let before = (0..stops.len()).rev().filter(open).find(|&i| stops[i].at <= it.at);
        let Some(i) = before.or_else(|| (0..stops.len()).find(open)) else { continue };
        used[i] = true;
        it.text = format!("{} ({})", stops[i].what.said(), stops[i].who);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcript::{stopped, STOPPED};

    const KEY: &str = "claude:claude:atlas";

    fn asked(what: Stop, key: &'static str, who: &'static str, at: i64) -> Asked<'static> {
        Asked { what, key, title: "Northwind sync", who, at, done: at + 500 }
    }

    #[test]
    fn a_line_reads_back_with_its_tabs_and_breaks_flattened() {
        let a = Asked { title: "Fix\tthe\nparser", ..asked(Stop::Stop, KEY, "a window on this Mac", 1_000) };
        let l = line("2026-10-26 17:30:44.120", &a, None);
        assert_eq!(l, "2026-10-26 17:30:44.120\tstop\tFix the parser\ta window on this Mac\tdone\tclaude:claude:atlas\t1000\t1500\n");
        assert_eq!(logged(&l), [Logged { what: Stop::Stop, who: "a window on this Mac".into(), key: KEY.into(), at: 1_000, done: 1_500 }]);
        for what in STOPS {
            assert_eq!(logged(&line("t", &asked(what, KEY, "y", 5), None))[0].what, what);
        }
    }

    #[test]
    fn a_stop_that_failed_and_a_line_of_another_shape_say_nothing() {
        let failed = line("t", &asked(Stop::Stop, KEY, "y", 5), Some("Its turn has already ended."));
        assert!(failed.contains("\tfailed: Its turn has already ended.\t"));
        assert_eq!(logged(&format!("{failed}garbage\nt\tstop\tx\ty\tdone\tk\tlater\t9\n")), []);
    }

    /// What stop notes at `notes` say, from a log of `stops` (each with why it failed, if it did).
    fn said(stops: &[(Asked, Option<&str>)], notes: &[i64]) -> Vec<String> {
        let dir = crate::fixtures::scratch("stops-attribute");
        let log = dir.join("stops.log");
        std::fs::write(&log, stops.iter().map(|(a, failed)| line("t", a, *failed)).collect::<String>()).unwrap();
        let mut page = Page { items: notes.iter().map(|&at| stopped(at)).collect(), before: None, after: 0 };
        attribute_from(&mut page, KEY, &log);
        page.items.into_iter().map(|it| it.text).collect()
    }

    #[test]
    fn a_stop_note_says_who_stopped_it_when_codealong_did() {
        let window = asked(Stop::Stop, KEY, "a window on this Mac", 100_000);
        assert_eq!(said(&[(window, None)], &[101_000]), ["Stopped from Codealong (a window on this Mac)"]);
        // A mark a moment before the log's clock was read is still its.
        let window = asked(Stop::Stop, KEY, "a window on this Mac", 100_000);
        assert_eq!(said(&[(window, None)], &[99_000]), ["Stopped from Codealong (a window on this Mac)"]);
        let moved = asked(Stop::Move, KEY, "“Kestrel lead”, with the codealong command", 100_000);
        assert_eq!(said(&[(moved, None)], &[100_200]), ["Stopped from Codealong to move it (“Kestrel lead”, with the codealong command)"]);
    }

    #[test]
    fn a_stop_note_says_nothing_of_who_when_nothing_here_made_it() {
        let other = asked(Stop::Deny, "claude:claude:other", "your phone", 100_000);
        let failed = asked(Stop::Stop, KEY, "a window on this Mac", 100_000);
        let long_before = asked(Stop::Stop, KEY, "a window on this Mac", 50_000);
        assert_eq!(said(&[(other, None), (failed, Some("It isn't running any more.")), (long_before, None)], &[100_400]), [STOPPED]);
        // Nor of anything after it.
        let after = asked(Stop::Stop, KEY, "a window on this Mac", 105_000);
        assert_eq!(said(&[(after, None)], &[100_400]), [STOPPED]);
    }

    #[test]
    fn each_stop_makes_one_note() {
        // Two stops close together: each note is the stop asked before it.
        let first = asked(Stop::Stop, KEY, "a window on this Mac", 100_000);
        let second = asked(Stop::Deny, KEY, "your phone", 103_000);
        assert_eq!(said(&[(first, None), (second, None)], &[101_000, 104_000]), ["Stopped from Codealong (a window on this Mac)", "Denied from Codealong (your phone)"]);
        // A stop in its terminal soon after one from here isn't taken for it too.
        let once = asked(Stop::Stop, KEY, "a window on this Mac", 100_000);
        assert_eq!(said(&[(once, None)], &[100_300, 106_000]), ["Stopped from Codealong (a window on this Mac)", STOPPED]);
    }

    #[test]
    fn who_asked_names_the_session_that_ran_the_command() {
        let s = crate::model::Session { title: "Kestrel lead".into(), ..crate::fixtures::session("claude:claude:boss") };
        let board = Board { lanes: vec![crate::fixtures::lane("/r", vec![s])], ..Board::default() };
        assert_eq!(who(&board, &From::Command(Some("claude:claude:boss".into()))), "“Kestrel lead”, with the codealong command");
        assert_eq!(who(&board, &From::Command(Some("claude:claude:gone".into()))), "claude:claude:gone, with the codealong command");
        assert_eq!(who(&board, &From::Here), "a window on this Mac");
        assert_eq!(who(&board, &From::Phone), "your phone");
    }
}
