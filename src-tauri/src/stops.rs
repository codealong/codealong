//! What Codealong did that ends a session's turn or cuts its step short, a line for each in
//! `~/Library/Logs/Codealong/stops.log`: Stop, the stop a move and Mark as done make first, Send
//! now on a message the session holds (Claude Code stops the step it's on for it) and a call
//! denied. What it was, which session, who asked (a window, the `codealong` command and the
//! session that ran it, another device) and how it went (`noted`).
//!
//! A turn cut short leaves only its harness's mark in its record, the same whoever pressed the
//! key: Claude Code's "User rejected tool use" and `[Request interrupted by user for tool use]`,
//! Codex's `turn_aborted`. So the conversation's note for it says it was Codealong's from here
//! (`attribute`), and says nothing of who when nothing here matches: a stop pressed by mistake read
//! as a call refused that nobody refused (asked 2026-10-26).
//!
//! A line is tab-separated: the local time, what it was, the session's title, who asked, how it
//! went, the session's key, and the time again in epoch milliseconds, which `attribute` reads.

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
    /// The stop Mark as done makes first, the session's own or its helpers'.
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

/// How long after Codealong asked its harness's mark of the stop may come and still be taken for
/// Codealong's: a key pressed through AppleScript under a heavy load took seconds to land.
const LANDS_WITHIN_MS: i64 = 30_000;
/// How long before Codealong asked: the mark's time and the log's come from different clocks
/// read at different moments.
const SKEW_MS: i64 = 5_000;
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
        append(&log_path(), &line(&crate::perf::stamp(), what, key, title, who, at, failed.as_deref()));
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

/// The log's line for a stop, `failed` saying why it didn't go.
fn line(stamp: &str, what: Stop, key: &str, title: &str, who: &str, at: i64, failed: Option<&str>) -> String {
    let flat = |s: &str| s.replace(['\t', '\n', '\r'], " ");
    let went = match failed {
        None => "done".to_string(),
        Some(why) => format!("failed: {}", flat(why)),
    };
    format!("{stamp}\t{}\t{}\t{}\t{went}\t{}\t{at}\n", what.word(), flat(title), flat(who), flat(key))
}

/// A stop that went, read back from the log.
#[derive(Debug, PartialEq)]
struct Logged {
    what: Stop,
    who: String,
    key: String,
    at: i64,
}

/// The stops that went in the log's text, oldest first. A line of another shape is skipped.
fn logged(text: &str) -> Vec<Logged> {
    text.lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split('\t').collect();
            let [_, what, _, who, went, key, at] = f.as_slice() else { return None };
            let what = STOPS.into_iter().find(|s| s.word() == *what)?;
            (*went == "done").then_some(())?;
            Some(Logged { what, who: who.to_string(), key: key.to_string(), at: at.parse().ok()? })
        })
        .collect()
}

/// Says on each stop note in a page of the session `key`'s conversation that Codealong stopped
/// it, and who asked, where the log has a stop of that session just before.
pub fn attribute(page: &mut Page, key: &str) {
    if page.items.iter().any(|it| it.stop) {
        attribute_from(page, key, &log_path());
    }
}

fn attribute_from(page: &mut Page, key: &str, log: &Path) {
    let Ok(len) = std::fs::metadata(log).map(|m| m.len()) else { return };
    let Some(text) = crate::util::read_range(log, len.saturating_sub(READ_BYTES), len) else { return };
    let stops: Vec<Logged> = logged(&text).into_iter().filter(|l| l.key == key).collect();
    for it in page.items.iter_mut().filter(|it| it.stop) {
        // The latest that could have made it: one asked again after a stop that didn't land.
        if let Some(l) = stops.iter().rev().find(|l| l.at - SKEW_MS <= it.at && it.at <= l.at + LANDS_WITHIN_MS) {
            it.text = format!("{} ({})", l.what.said(), l.who);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcript::{item, stopped, STOPPED};

    const KEY: &str = "claude:claude:atlas";

    #[test]
    fn a_line_reads_back_with_its_tabs_and_breaks_flattened() {
        let l = line("2026-10-26 17:30:44.120", Stop::Stop, KEY, "Fix\tthe\nparser", "a window on this Mac", 1_000, None);
        assert_eq!(l, "2026-10-26 17:30:44.120\tstop\tFix the parser\ta window on this Mac\tdone\tclaude:claude:atlas\t1000\n");
        assert_eq!(logged(&l), [Logged { what: Stop::Stop, who: "a window on this Mac".into(), key: KEY.into(), at: 1_000 }]);
        for what in STOPS {
            assert_eq!(logged(&line("t", what, KEY, "x", "y", 5, None))[0].what, what);
        }
    }

    #[test]
    fn a_stop_that_failed_and_a_line_of_another_shape_say_nothing() {
        let failed = line("t", Stop::Stop, KEY, "x", "y", 5, Some("Its turn has already ended."));
        assert!(failed.contains("\tfailed: Its turn has already ended.\t"));
        assert_eq!(logged(&format!("{failed}garbage\nt\tstop\tx\ty\tdone\tk\tlater\n")), []);
    }

    #[test]
    fn a_stop_note_says_who_stopped_it_when_codealong_did_just_before() {
        let dir = crate::fixtures::scratch("stops-attribute");
        let log = dir.join("stops.log");
        let text = [
            line("t", Stop::Stop, KEY, "Northwind sync", "a window on this Mac", 100_000, None),
            line("t", Stop::Deny, "claude:claude:other", "Other", "your phone", 200_000, None),
            line("t", Stop::Stop, KEY, "Northwind sync", "a window on this Mac", 300_000, Some("It isn't running any more.")),
            line("t", Stop::Move, KEY, "Northwind sync", "“Kestrel lead”, with the codealong command", 400_000, None),
        ]
        .concat();
        std::fs::write(&log, text).unwrap();
        let mut page = Page { items: vec![stopped(103_000), item(103_000, "note", "The conversation was compacted here"), stopped(200_500), stopped(301_000), stopped(399_000), stopped(500_000)], before: None, after: 0 };
        attribute_from(&mut page, KEY, &log);
        let said: Vec<&str> = page.items.iter().map(|it| it.text.as_str()).collect();
        assert_eq!(
            said,
            [
                "Stopped from Codealong (a window on this Mac)",
                "The conversation was compacted here",
                // Another session's stop, one that failed and one long before say nothing of who.
                STOPPED,
                STOPPED,
                "Stopped from Codealong to move it (“Kestrel lead”, with the codealong command)",
                STOPPED,
            ]
        );
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
