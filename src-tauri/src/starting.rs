//! Starts an agent asked for whose session isn't on the board yet (origins.rs `Pending`): the
//! line each gets under the session that asked for it (`Board.starting`), and what the terminal
//! of one started in a terminal shows while it waits (`Screens`).
//!
//! A harness that names its own sessions (Codex, OpenCode, Antigravity, Cursor) makes one only
//! once its terminal UI takes the prompt, and that UI can stop first on a screen of its own:
//! Codex's review of the hooks a repo's `.codex/config.toml` adds ("Hooks need review", 2026-11-15),
//! its update offer, its question about its background server. In a terminal behind your windows
//! nobody sees it, and the session that asked had nothing under it to say a helper was on its way.
//! So its line says it's starting, and a stuck one's line, its notice (origins.rs
//! `stuck_notices`) and its session's report (helpers.rs) say what its tab shows, read as
//! `launch::screen_of` reads a session's (Rex, tmux, Codealong's own terminals; not Ghostty).
//!
//! The tab is found by its harness's process: one on a terminal in its worktree that began as the
//! start was asked for, those whose command line has the prompt's first words first (Codex takes
//! the prompt as an argument), and only when they're all on one terminal (`process_of`).

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::model::Board;
use crate::origins::{Pending, LAUNCH_MS, PENDING_SLACK_MS};
use crate::procs::PsLine;

/// How long after it's started a start in a terminal has its screen read: well before it's
/// said to be stuck (origins.rs `STUCK_MS`), so what it shows is known by then.
const READ_AFTER_MS: i64 = 60_000;
/// How often a waiting start's screen is read again.
const READ_EVERY_MS: i64 = 15_000;
/// How many lines with something on them a screen's foot keeps (`foot`): a terminal UI's
/// question, its reasons and its choices.
const FOOT_LINES: usize = 12;
/// How much of a prompt's opening its harness's command line is looked for by (`first_words`).
const MATCH_CHARS: usize = 48;
/// An opening with fewer characters than this to go by could be anyone's.
const MATCH_MIN: usize = 12;

/// A start whose session isn't on the board yet, as the windows list it under the session that
/// asked for it (`Board.starting`).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Starting {
    /// When it was started (`Pending::at`): which start it is, in the same worktree too.
    pub at: i64,
    pub account: String,
    /// Its harness's word (Codex), for its line.
    pub harness: String,
    /// The worktree it was started in (a lane id), and that worktree's name.
    pub lane: String,
    pub place: String,
    /// Started as a helper of the session (`codealong start --helper`).
    #[serde(default)]
    pub helper: bool,
    /// How its prompt opens (`origins::opening`): its line's title until its session has one.
    #[serde(default)]
    pub opens: Option<String>,
    /// The terminal it was started in (`apps.terminal`), where it's found, when it was.
    #[serde(default)]
    pub terminal: Option<String>,
    /// It's gone a few minutes without its session (`Pending::stuck`).
    #[serde(default)]
    pub stuck: bool,
    /// What its terminal shows at its foot (`foot`), when that was read.
    #[serde(default)]
    pub screen: Option<String>,
}

/// The starts still waiting for their session (`origins::waiting_for`) that a session asked for,
/// by that session's key, the latest first, each with what its terminal showed when last read.
pub fn by_session(waiting: &[Pending], board: &Board, screens: &Screens, word_of: impl Fn(&str) -> String, now: i64) -> BTreeMap<String, Vec<Starting>> {
    let mut out: BTreeMap<String, Vec<Starting>> = BTreeMap::new();
    for p in waiting.iter().filter(|p| !p.from.is_empty()) {
        out.entry(p.from.clone()).or_default().push(Starting {
            at: p.at,
            account: p.account.clone(),
            harness: word_of(&p.account),
            lane: p.lane.clone(),
            place: crate::origins::place_of(board, &p.lane),
            helper: p.helper,
            opens: p.opens.clone(),
            terminal: p.terminal.then(|| board.apps.terminal.clone()),
            stuck: p.stuck(now),
            screen: screens.screen(p.at).map(String::from),
        });
    }
    for starts in out.values_mut() {
        starts.sort_by(|a, b| b.at.cmp(&a.at));
    }
    out
}

/// What the terminals of starts waiting in one showed when last read, read off the engine's
/// thread (`read`): one read at a time, each start's at most every `READ_EVERY_MS`.
#[derive(Default)]
pub struct Screens {
    /// By the start's `at`.
    read: HashMap<i64, Read>,
    inflight: bool,
}

struct Read {
    /// When it was last asked for.
    asked: Option<i64>,
    screen: Option<String>,
}

/// A start whose terminal is to be read.
#[derive(Clone, Debug, PartialEq)]
pub struct Ask {
    pub at: i64,
    pub lane: String,
    pub opens: Option<String>,
}

impl Screens {
    /// The starts whose terminals are due to be read at `now`: waiting in one `READ_AFTER_MS` or
    /// more (`Pending::in_terminal`) and not asked for in `READ_EVERY_MS`; none while a read is on
    /// its way. What it asks for is marked asked, so a read that falls over isn't asked again at
    /// once, and the starts no longer waiting are forgotten.
    pub fn due(&mut self, waiting: &[Pending], now: i64) -> Vec<Ask> {
        self.read.retain(|at, _| waiting.iter().any(|p| p.at == *at));
        if self.inflight {
            return Vec::new();
        }
        let mut asks = Vec::new();
        for p in waiting.iter().filter(|p| p.in_terminal(now) && now - p.at >= READ_AFTER_MS) {
            let read = self.read.entry(p.at).or_insert(Read { asked: None, screen: None });
            if read.asked.is_none_or(|asked| now - asked >= READ_EVERY_MS) {
                read.asked = Some(now);
                asks.push(Ask { at: p.at, lane: p.lane.clone(), opens: p.opens.clone() });
            }
        }
        self.inflight = !asks.is_empty();
        asks
    }

    /// A read is over: each start's screen, none where it couldn't be read; or nothing when the
    /// read fell over or never ran, which leaves what was read before. Whether what a start's
    /// terminal shows changed.
    pub fn on_read(&mut self, found: Option<Vec<(i64, Option<String>)>>) -> bool {
        self.inflight = false;
        let mut changed = false;
        for (at, screen) in found.unwrap_or_default() {
            // A start gone since it was asked for is forgotten, not brought back.
            let Some(read) = self.read.get_mut(&at) else { continue };
            changed |= read.screen != screen;
            read.screen = screen;
        }
        changed
    }

    /// What the start made `at` showed when its terminal was last read.
    pub fn screen(&self, at: i64) -> Option<&str> {
        self.read.get(&at).and_then(|r| r.screen.as_deref())
    }
}

/// Reads each start's terminal, its harness's process found among those `listed`: the foot of
/// what its tab shows (`foot`), or none where its process isn't found or its terminal can't be
/// read. A read that fails says nothing more than one that can't be made: the notice goes on
/// guessing, as it did before reads.
pub fn read(asks: &[Ask], listed: &[PsLine]) -> Vec<(i64, Option<String>)> {
    asks.iter()
        .map(|a| {
            let screen = process_of(a, listed, crate::procs::proc_start_ms, crate::procs::cwd_of).and_then(|pid| crate::launch::screen_of(pid, Path::new(&a.lane)).ok().flatten()).and_then(|text| foot(&text));
            (a.at, screen)
        })
        .collect()
}

/// The process of the harness a start runs in its terminal, among the processes on a terminal
/// that began (`began`) in its worktree (`cwd`) as the start was asked for: those whose command
/// line has the prompt's first words where there are any, else all of them; the latest of those
/// to begin, when they're all on one terminal. None when nothing says which: two starts there
/// then with the same opening (one agent's two reviewers) are on two terminals, and a terminal
/// that might be the other's is never read as this one's.
fn process_of(ask: &Ask, listed: &[PsLine], began: impl Fn(u32) -> Option<i64>, cwd: impl Fn(u32) -> Option<PathBuf>) -> Option<u32> {
    let lane = Path::new(&ask.lane);
    let real = dunce::canonicalize(lane).ok();
    let here: Vec<(&PsLine, i64)> = listed
        .iter()
        .filter(|l| on_terminal(&l.tty))
        .filter_map(|l| began(l.pid).map(|t| (l, t)))
        .filter(|(_, t)| *t >= ask.at - PENDING_SLACK_MS && *t <= ask.at + LAUNCH_MS)
        .filter(|(l, _)| cwd(l.pid).is_some_and(|d| d == lane || real.as_ref() == Some(&d)))
        .collect();
    let words = ask.opens.as_deref().and_then(first_words);
    let named: Vec<&(&PsLine, i64)> = here.iter().filter(|(l, _)| words.as_ref().is_some_and(|w| collapsed(&l.command).contains(w.as_str()))).collect();
    let these = if named.is_empty() { here.iter().collect() } else { named };
    let tty = &these.first()?.0.tty;
    if these.iter().any(|(l, _)| &l.tty != tty) {
        return None;
    }
    these.iter().max_by_key(|(_, t)| *t).map(|(l, _)| l.pid)
}

/// `ps` names a terminal (`ttys004`); `??` is none.
fn on_terminal(tty: &str) -> bool {
    !tty.is_empty() && tty != "??" && tty != "-"
}

/// The first words of a prompt's opening as its harness's command line has them: up to
/// `MATCH_CHARS`, before anything `util::one_line` may have changed (a link, emphasis, where it
/// was cut). None when that's too little to tell one prompt from another.
fn first_words(opens: &str) -> Option<String> {
    let mut out = String::new();
    for w in opens.split_whitespace() {
        if w.contains(['[', ']', '*', '…']) || (!out.is_empty() && out.chars().count() + 1 + w.chars().count() > MATCH_CHARS) {
            break;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(w);
    }
    (out.chars().count() >= MATCH_MIN).then_some(out)
}

/// A command line with each run of whitespace one space, as an opening has it.
fn collapsed(command: &str) -> String {
    command.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// What a terminal UI asks at the foot of its screen: the last `FOOT_LINES` lines with something
/// on them, a run of blank lines between them kept as one, without the lines that only draw a
/// box, moved left as far as they all go. None for a screen with nothing on it.
pub fn foot(screen: &str) -> Option<String> {
    let mut kept: Vec<&str> = Vec::new();
    let mut lines = 0;
    for l in screen.lines().rev().map(str::trim_end).filter(|l| !draws_a_box(l)) {
        if l.is_empty() {
            if kept.last().is_some_and(|k| !k.is_empty()) {
                kept.push("");
            }
            continue;
        }
        if lines == FOOT_LINES {
            break;
        }
        kept.push(l);
        lines += 1;
    }
    while kept.last() == Some(&"") {
        kept.pop();
    }
    kept.reverse();
    let indent = kept.iter().filter(|l| !l.is_empty()).map(|l| l.len() - l.trim_start_matches(' ').len()).min()?;
    Some(kept.iter().map(|l| l.get(indent..).unwrap_or_default()).collect::<Vec<_>>().join("\n"))
}

/// A line of nothing but box drawing (`╭───╮`, `│ │`), which says nothing in a few lines of text.
fn draws_a_box(line: &str) -> bool {
    line.chars().any(|c| !c.is_whitespace()) && line.chars().all(|c| c.is_whitespace() || ('\u{2500}'..='\u{257F}').contains(&c))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::OriginHow;

    const AT: i64 = 1_000_000_000;

    fn pending(from: &str, at: i64) -> Pending {
        Pending { from: from.into(), how: OriginHow::Started, lane: "/r/app".into(), account: "codex".into(), at, helper: true, report: true, opens: Some("Draft the release notes for northwind from the merged PRs".into()), terminal: true }
    }

    fn ps(pid: u32, tty: &str, command: &str) -> PsLine {
        PsLine { pid, ppid: 1, tty: tty.into(), cpu: None, command: command.into() }
    }

    /// What Rex gave of a Codex helper's tab that sat on its hooks review (2026-11-15).
    const HOOKS: &str = "\n  Hooks need review\n  3 hooks are new or changed.\n  Hooks can run outside the sandbox after you trust them.\n\n\n› 1. Review hooks\n  2. Trust all and continue\n  3. Continue without trusting (hooks won't run)\n\n  enter confirm · esc skip\n";

    #[test]
    fn a_screens_foot_keeps_what_it_asks_with_its_gaps_and_without_its_boxes() {
        assert_eq!(
            foot(HOOKS).as_deref(),
            Some("  Hooks need review\n  3 hooks are new or changed.\n  Hooks can run outside the sandbox after you trust them.\n\n› 1. Review hooks\n  2. Trust all and continue\n  3. Continue without trusting (hooks won't run)\n\n  enter confirm · esc skip")
        );
        // Moved left as far as every line goes; a box's edges and the lines above the last twelve
        // go.
        let boxed = format!("╭──────╮\n│ >_ OpenAI Codex │\n╰──────╯\n{}", (1..=14).map(|i| format!("    line {i}")).collect::<Vec<_>>().join("\n"));
        let kept = foot(&boxed).unwrap();
        assert_eq!(kept.lines().next(), Some("line 3"));
        assert_eq!(kept.lines().count(), 12);
        assert_eq!(foot("\n   \n"), None);
    }

    #[test]
    fn a_starts_harness_is_found_by_its_prompt_else_alone_in_its_worktree() {
        let ask = Ask { at: AT, lane: "/r/app".into(), opens: Some("Draft the release notes for northwind from the merged PRs, with a deadline of the 24th and …".into()) };
        let began = |pid: u32| Some(match pid {
            1 => AT - 3_600_000,
            _ => AT + 1_500 + pid as i64,
        });
        let at_app = |_: u32| Some(PathBuf::from("/r/app"));
        let listed = [
            ps(1, "ttys001", "codex -- Draft the release notes for northwind from the merged PRs"),
            ps(2, "ttys010", "/bin/zsh -l"),
            ps(3, "ttys010", "codex -c check_for_update_on_startup=false -- Draft the release notes  for northwind from the merged PRs, with"),
            ps(4, "??", "codex app-server -- Draft the release notes for northwind from the merged PRs"),
        ];
        // The one started then, on a terminal, whose command has the prompt: not an older run of
        // the same prompt, nor one with no terminal.
        assert_eq!(process_of(&ask, &listed, began, at_app), Some(3));
        // An opening its command line doesn't have: the latest in the worktree, its terminal the
        // only one that began there then.
        let other = Ask { opens: Some("Something else entirely, typed in".into()), ..ask.clone() };
        assert_eq!(process_of(&other, &listed, began, at_app), Some(3));
        // Two terminals began there then: nothing says which.
        let two = [ps(2, "ttys010", "/bin/zsh -l"), ps(5, "ttys011", "/bin/zsh -l")];
        assert_eq!(process_of(&other, &two, began, at_app), None);
        // None in its worktree.
        assert_eq!(process_of(&other, &listed[1..2], began, |_| Some(PathBuf::from("/r/elsewhere"))), None);
        // The same prompt started in another worktree then is that one's, however late; and two
        // of it in this one are on two terminals, so neither is read as this start's.
        let twins = [ps(3, "ttys010", "codex -- Draft the release notes for northwind from the merged PRs"), ps(7, "ttys012", "codex -- Draft the release notes for northwind from the merged PRs")];
        let other_place = |pid: u32| Some(PathBuf::from(if pid == 7 { "/r/other" } else { "/r/app" }));
        assert_eq!(process_of(&ask, &twins, began, other_place), Some(3));
        assert_eq!(process_of(&ask, &twins, began, at_app), None);
        // Too little of an opening to go by.
        assert_eq!(first_words("Hi"), None);
        assert_eq!(first_words("Look at [the PR](https://example.com) and …"), None);
        assert_eq!(first_words("Review the **branch** now").as_deref(), None);
        assert_eq!(first_words("Review the changes on this branch, then **say**").as_deref(), Some("Review the changes on this branch, then"));
    }

    #[test]
    fn a_starts_screen_is_read_a_while_on_and_again_now_and_then() {
        let start = pending("claude:claude:boss", AT);
        let mut screens = Screens::default();
        // Not in its first minute.
        assert!(screens.due(&[start.clone()], AT + 30_000).is_empty());
        let asks = screens.due(&[start.clone()], AT + READ_AFTER_MS);
        assert_eq!(asks, [Ask { at: AT, lane: "/r/app".into(), opens: start.opens.clone() }]);
        // Not again while that read is on its way, nor soon after it.
        assert!(screens.due(&[start.clone()], AT + READ_AFTER_MS + 1_000).is_empty());
        assert!(screens.on_read(Some(vec![(AT, foot(HOOKS))])));
        assert_eq!(screens.screen(AT).map(|s| s.lines().next()), Some(Some("  Hooks need review")));
        assert!(screens.due(&[start.clone()], AT + READ_AFTER_MS + 5_000).is_empty());
        // Read again later; the same screen changes nothing, and a read that fell over keeps it.
        assert_eq!(screens.due(&[start.clone()], AT + READ_AFTER_MS + READ_EVERY_MS).len(), 1);
        assert!(!screens.on_read(Some(vec![(AT, foot(HOOKS))])));
        assert_eq!(screens.due(&[start.clone()], AT + READ_AFTER_MS + 2 * READ_EVERY_MS).len(), 1);
        assert!(!screens.on_read(None));
        assert!(screens.screen(AT).is_some());
        // Not one started outside a terminal, nor without a prompt, nor once it's no longer
        // waiting, which forgets what it showed.
        let mut fresh = Screens::default();
        assert!(fresh.due(&[Pending { terminal: false, ..start.clone() }, Pending { opens: None, ..start.clone() }], AT + READ_AFTER_MS).is_empty());
        assert!(screens.due(&[], AT + READ_AFTER_MS + 3 * READ_EVERY_MS).is_empty());
        assert_eq!(screens.screen(AT), None);
    }

    #[test]
    fn a_sessions_starts_are_listed_under_it_the_latest_first() {
        let boss = "claude:claude:boss";
        let mut board = Board { lanes: vec![crate::fixtures::lane("/r/app", vec![])], ..Board::default() };
        board.apps.terminal = "Rex".into();
        let mut screens = Screens::default();
        let (older, newer) = (pending(boss, AT), Pending { terminal: false, helper: false, ..pending(boss, AT + 60_000) });
        let waiting = [older.clone(), newer.clone(), Pending { from: String::new(), ..pending("", AT) }];
        screens.due(&waiting, AT + 4 * 60_000);
        screens.on_read(Some(vec![(AT, foot(HOOKS))]));
        let listed = by_session(&waiting, &board, &screens, |_| "Codex".into(), AT + 4 * 60_000);
        // Yours, started from no session, is under none.
        assert_eq!(listed.keys().collect::<Vec<_>>(), [boss]);
        let starts = &listed[boss];
        assert_eq!(starts.iter().map(|s| s.at).collect::<Vec<_>>(), [AT + 60_000, AT]);
        let stuck = &starts[1];
        assert_eq!((stuck.harness.as_str(), stuck.place.as_str(), stuck.terminal.as_deref(), stuck.stuck, stuck.helper), ("Codex", "r/app", Some("Rex"), true, true));
        assert!(stuck.screen.as_deref().is_some_and(|s| s.contains("Hooks need review")));
        // One started in an app waits for nobody to answer a terminal.
        assert_eq!((starts[0].terminal.as_deref(), starts[0].stuck, starts[0].screen.as_deref()), (None, false, None));
    }
}
