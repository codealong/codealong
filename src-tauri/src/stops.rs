//! What Codealong did that ends a session's turn or cuts its step short, a line for each in
//! `~/Library/Logs/Codealong/stops.log`: Stop, the stop a move makes first, Mark as done ending
//! work under way (its turn stopped, its terminal closed or its job stopped), Send now on a
//! message the session holds where that stops the step it's on (`Adapter::steer_queued_stops`),
//! and a call denied where that ends the turn (`Adapter::deny_stops`). What it was, which session, who asked (a
//! window, the `codealong` command and the session that ran it, another device) and how it went
//! (`noted`).
//!
//! A turn cut short leaves only its harness's mark in its record, the same whoever pressed the
//! key: Claude Code's "User rejected tool use" and `[Request interrupted by user for tool use]`,
//! Codex's `turn_aborted`. So the conversation's note for it says it was Codealong's from here
//! (`attribute`), and says nothing of who when nothing here matches: a stop pressed by mistake read
//! as a call refused that nobody refused (asked 2026-10-26).
//!
//! A stop is two tab-separated lines: as it's asked, then as it went. Each says the local time,
//! what it was, the session's title, who asked, `asked` or how it went, the session's key and
//! when it was asked in epoch milliseconds; the second says when it was done too. `attribute`
//! reads them: the first is written before the key is pressed, so a conversation read while the
//! stop is still under way, as the window that asked reads it, finds it.

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

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
    /// Mark as done ending work under way, the session's own or its helpers'.
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

/// How long after a stop was done (or asked, while it's under way) its harness's mark of it may
/// come and still be taken for it: the harness reads the key once it gets a turn on a busy Mac.
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
/// as it's asked and with how it went. Not from tests, whose sessions are made up.
pub fn noted<T, E: std::fmt::Display>(what: Stop, key: &str, title: &str, who: &str, act: impl FnOnce() -> Result<T, E>) -> Result<T, E> {
    let at = crate::util::now_ms();
    let asked = Asked { what, key, title, who, at };
    if !cfg!(test) {
        append(&log_path(), &asked_line(&crate::perf::stamp(), &asked));
    }
    let got = act();
    if !cfg!(test) {
        let failed = got.as_ref().err().map(|e| e.to_string());
        append(&log_path(), &went_line(&crate::perf::stamp(), &asked, crate::util::now_ms(), failed.as_deref()));
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

/// A stop as it's logged: what, of which session, who asked, and when.
struct Asked<'a> {
    what: Stop,
    key: &'a str,
    title: &'a str,
    who: &'a str,
    at: i64,
}

fn flat(s: &str) -> String {
    s.replace(['\t', '\n', '\r'], " ")
}

/// The log's line for a stop as it's asked.
fn asked_line(stamp: &str, a: &Asked) -> String {
    format!("{stamp}\t{}\t{}\t{}\tasked\t{}\t{}\n", a.what.word(), flat(a.title), flat(a.who), flat(a.key), a.at)
}

/// The log's line for how a stop went, done at `done`, `failed` saying why it didn't go.
fn went_line(stamp: &str, a: &Asked, done: i64, failed: Option<&str>) -> String {
    let went = match failed {
        None => "done".to_string(),
        Some(why) => format!("failed: {}", flat(why)),
    };
    format!("{stamp}\t{}\t{}\t{}\t{went}\t{}\t{}\t{done}\n", a.what.word(), flat(a.title), flat(a.who), flat(a.key), a.at)
}

/// A stop read back from the log that went, or is still under way.
#[derive(Debug, PartialEq)]
struct Logged {
    what: Stop,
    who: String,
    key: String,
    at: i64,
    /// When it was done; none while it's under way.
    done: Option<i64>,
}

impl Logged {
    /// Whether a mark of a stop at `t` could be this one's.
    fn could_have_made(&self, t: i64) -> bool {
        self.at - SKEW_MS <= t && t <= self.done.unwrap_or(self.at) + LANDS_AFTER_MS
    }
}

/// The stops in the log's text that went or are under way, the earliest asked first: stops of
/// one session asked at once log how they went in the order they end. A line of another shape is
/// skipped, and so is a stop that failed.
fn logged(text: &str) -> Vec<Logged> {
    let mut stops: Vec<Logged> = Vec::new();
    let mut failed: Vec<(String, i64)> = Vec::new();
    for l in text.lines() {
        let f: Vec<&str> = l.split('\t').collect();
        let (what, who, went, key, at, done) = match f.as_slice() {
            [_, what, _, who, went, key, at] => (what, who, went, key, at, None),
            [_, what, _, who, went, key, at, done] => (what, who, went, key, at, Some(done)),
            _ => continue,
        };
        let (Some(what), Ok(at)) = (STOPS.into_iter().find(|s| s.word() == *what), at.parse::<i64>()) else { continue };
        match (*went, done) {
            ("asked", None) => stops.push(Logged { what, who: who.to_string(), key: key.to_string(), at, done: None }),
            ("done", Some(done)) => {
                let Ok(done) = done.parse() else { continue };
                match stops.iter_mut().find(|s| s.key == *key && s.at == at && s.done.is_none()) {
                    Some(s) => s.done = Some(done),
                    None => stops.push(Logged { what, who: who.to_string(), key: key.to_string(), at, done: Some(done) }),
                }
            }
            (went, Some(_)) if went.starts_with("failed") => failed.push((key.to_string(), at)),
            _ => {}
        }
    }
    stops.retain(|s| !failed.iter().any(|(k, at)| *k == s.key && *at == s.at));
    stops.sort_by_key(|s| s.at);
    stops
}

/// Which note each stop went to, by the session, the stop's time and the note's place in the
/// conversation (its cursor and number), so a page read later, of what came after, doesn't give
/// a stop to a second note. Kept for the app's run; a fresh read after a relaunch reads the
/// notes near each other on one page.
type Claims = HashMap<(String, i64), (u64, u32)>;

static CLAIMS: Mutex<Option<Claims>> = Mutex::new(None);

/// Says on each stop note in a page of the session `key`'s conversation that Codealong stopped
/// it, and who asked, where the log has a stop of that session that could have made it.
pub fn attribute(page: &mut Page, key: &str) {
    if !page.items.iter().any(|it| it.stop) {
        return;
    }
    let mut claims = CLAIMS.lock().unwrap_or_else(|e| e.into_inner());
    attribute_from(page, key, &log_path(), claims.get_or_insert_with(HashMap::new));
}

fn attribute_from(page: &mut Page, key: &str, log: &Path, claims: &mut Claims) {
    let Ok(len) = std::fs::metadata(log).map(|m| m.len()) else { return };
    let Some(text) = crate::util::read_range(log, len.saturating_sub(READ_BYTES), len) else { return };
    let stops: Vec<Logged> = logged(&text).into_iter().filter(|l| l.key == key).collect();
    for it in page.items.iter_mut().filter(|it| it.stop) {
        let note = (it.cursor, it.n);
        let claimed = |s: &Logged| claims.get(&(key.to_string(), s.at)).copied();
        // A note read before keeps its stop. Otherwise each stop makes one mark: the latest
        // asked before it, else the first asked just after, its clock read a moment late.
        let own = stops.iter().position(|s| claimed(s) == Some(note));
        let open = |i: &usize| claimed(&stops[*i]).is_none() && stops[*i].could_have_made(it.at);
        let before = || (0..stops.len()).rev().filter(open).find(|&i| stops[i].at <= it.at);
        let Some(i) = own.or_else(before).or_else(|| (0..stops.len()).find(open)) else { continue };
        claims.insert((key.to_string(), stops[i].at), note);
        it.text = format!("{} ({})", stops[i].what.said(), stops[i].who);
    }
    // Only stops the log's end still has can be claimed again.
    let oldest = stops.first().map_or(i64::MAX, |s| s.at);
    claims.retain(|(k, at), _| k != key || *at >= oldest);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcript::{stopped, Item, STOPPED};

    const KEY: &str = "claude:claude:atlas";

    const WINDOW: &str = "a window on this Mac";

    fn asked(what: Stop, key: &'static str, who: &'static str, at: i64) -> Asked<'static> {
        Asked { what, key, title: "Northwind sync", who, at }
    }

    /// A stop's two lines, done half a second after it was asked.
    fn went(what: Stop, key: &'static str, who: &'static str, at: i64) -> String {
        let a = asked(what, key, who, at);
        asked_line("t", &a) + &went_line("t", &a, at + 500, None)
    }

    #[test]
    fn a_stop_reads_back_from_its_two_lines_with_tabs_and_breaks_flattened() {
        let a = Asked { title: "Fix\tthe\nparser", ..asked(Stop::Stop, KEY, WINDOW, 1_000) };
        let (first, second) = (asked_line("2026-10-26 17:30:44.120", &a), went_line("2026-10-26 17:30:44.620", &a, 1_500, None));
        assert_eq!(first, "2026-10-26 17:30:44.120\tstop\tFix the parser\ta window on this Mac\tasked\tclaude:claude:atlas\t1000\n");
        assert_eq!(second, "2026-10-26 17:30:44.620\tstop\tFix the parser\ta window on this Mac\tdone\tclaude:claude:atlas\t1000\t1500\n");
        let back = Logged { what: Stop::Stop, who: WINDOW.into(), key: KEY.into(), at: 1_000, done: Some(1_500) };
        assert_eq!(logged(&(first.clone() + &second)), [back]);
        // Under way, it's there as asked.
        assert_eq!(logged(&first)[0].done, None);
        for what in STOPS {
            assert_eq!(logged(&went(what, KEY, "y", 5))[0].what, what);
        }
    }

    #[test]
    fn a_stop_that_failed_and_a_line_of_another_shape_say_nothing() {
        let a = asked(Stop::Stop, KEY, "y", 5);
        let failed = went_line("t", &a, 9, Some("Its turn has already ended."));
        assert!(failed.contains("\tfailed: Its turn has already ended.\t"));
        assert_eq!(logged(&format!("{}{failed}garbage\nt\tstop\tx\ty\tdone\tk\tlater\t9\n", asked_line("t", &a))), []);
    }

    /// What stop notes at `notes` (each its time and its line's place) say, from `log`, with what
    /// earlier reads claimed.
    fn said_with(log_text: &str, notes: &[(i64, u64)], claims: &mut Claims) -> Vec<String> {
        let dir = crate::fixtures::scratch("stops-attribute");
        let log = dir.join("stops.log");
        std::fs::write(&log, log_text).unwrap();
        let mut page = Page { items: notes.iter().map(|&(at, cursor)| Item { cursor, ..stopped(at) }).collect(), before: None, after: 0 };
        attribute_from(&mut page, KEY, &log, claims);
        page.items.into_iter().map(|it| it.text).collect()
    }

    fn said(log_text: &str, notes: &[i64]) -> Vec<String> {
        let notes: Vec<(i64, u64)> = notes.iter().enumerate().map(|(n, &at)| (at, n as u64 * 100)).collect();
        said_with(log_text, &notes, &mut Claims::new())
    }

    #[test]
    fn a_stop_note_says_who_stopped_it_when_codealong_did() {
        assert_eq!(said(&went(Stop::Stop, KEY, WINDOW, 100_000), &[101_000]), ["Stopped from Codealong (a window on this Mac)"]);
        // A mark a moment before the log's clock was read is still its.
        assert_eq!(said(&went(Stop::Stop, KEY, WINDOW, 100_000), &[99_000]), ["Stopped from Codealong (a window on this Mac)"]);
        let moved = went(Stop::Move, KEY, "“Kestrel lead”, with the codealong command", 100_000);
        assert_eq!(said(&moved, &[100_200]), ["Stopped from Codealong to move it (“Kestrel lead”, with the codealong command)"]);
        // Read while it's still under way, before it says how it went.
        assert_eq!(said(&asked_line("t", &asked(Stop::Stop, KEY, WINDOW, 100_000)), &[100_300]), ["Stopped from Codealong (a window on this Mac)"]);
    }

    #[test]
    fn a_stop_note_says_nothing_of_who_when_nothing_here_made_it() {
        let failed = asked(Stop::Stop, KEY, WINDOW, 100_000);
        let log = [
            went(Stop::Deny, "claude:claude:other", "your phone", 100_000),
            asked_line("t", &failed) + &went_line("t", &failed, 100_100, Some("It isn't running any more.")),
            went(Stop::Stop, KEY, WINDOW, 50_000),
        ]
        .concat();
        assert_eq!(said(&log, &[100_400]), [STOPPED]);
        // Nor of anything after it.
        assert_eq!(said(&went(Stop::Stop, KEY, WINDOW, 105_000), &[100_400]), [STOPPED]);
    }

    #[test]
    fn each_stop_makes_one_note() {
        // Two stops close together: each note is the stop asked before it.
        let log = went(Stop::Stop, KEY, WINDOW, 100_000) + &went(Stop::Deny, KEY, "your phone", 103_000);
        assert_eq!(said(&log, &[101_000, 104_000]), ["Stopped from Codealong (a window on this Mac)", "Denied from Codealong (your phone)"]);
        // Asked in that order, logged as they ended: the later ended first.
        let (first, second) = (asked(Stop::Stop, KEY, WINDOW, 100_000), asked(Stop::Deny, KEY, "your phone", 103_000));
        let log = [asked_line("t", &first), asked_line("t", &second), went_line("t", &second, 103_200, None), went_line("t", &first, 104_000, None)].concat();
        assert_eq!(said(&log, &[101_000, 104_000]), ["Stopped from Codealong (a window on this Mac)", "Denied from Codealong (your phone)"]);
        // A stop in its terminal soon after one from here isn't taken for it too.
        assert_eq!(said(&went(Stop::Stop, KEY, WINDOW, 100_000), &[100_300, 106_000]), ["Stopped from Codealong (a window on this Mac)", STOPPED]);
    }

    #[test]
    fn a_stop_keeps_its_note_across_pages() {
        // The note read first, then a page of what came after it with another, then the first again.
        let log = went(Stop::Stop, KEY, WINDOW, 100_000);
        let mut claims = Claims::new();
        assert_eq!(said_with(&log, &[(100_300, 10)], &mut claims), ["Stopped from Codealong (a window on this Mac)"]);
        assert_eq!(said_with(&log, &[(106_000, 20)], &mut claims), [STOPPED]);
        assert_eq!(said_with(&log, &[(100_300, 10), (106_000, 20)], &mut claims), ["Stopped from Codealong (a window on this Mac)", STOPPED]);
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
