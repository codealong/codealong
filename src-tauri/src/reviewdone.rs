//! When an automatic review's session is done with (autoreview.rs starts them, each a helper of
//! its PR, `Helper::pr`). Nobody marks one done: it works for its PR, and nothing it says waits
//! on you. So every `TICK` the board is looked at, and:
//!
//! - once its PR is merged or closed, as the board lists it, it's marked done as Mark as done
//!   does: ended where it runs (its terminal or tmux pane closed, its background job stopped) and
//!   archived. No round of it comes again.
//! - once the board no longer lists its PR at all (a repo whose PRs aren't read hears only of the
//!   ones that wait on your review, so a merged one just goes) and it has sat unused for
//!   `IDLE_MS`, it's marked done the same way: no round of a PR the board doesn't list starts.
//! - while its PR is open, one that has sat unused for `IDLE_MS`, with no round of its PR under way
//!   or due (new commits it hasn't reviewed), is ended and kept: the next round goes on with it
//!   where `[reviews] start` says. One that runs in Codealong, which holds no process of its own,
//!   is left as it is.
//! - once another review of its PR has started on this Mac since its turn stopped on an error
//!   (autoreview.rs reviews the commit again in a new session, `past_error`), it's marked done the
//!   same way, whatever its PR: nothing in it waits on you, and its PR's card says what went wrong.
//!
//! None touches one you've taken up (`Helper::yours`; one made your own is no helper), one at
//! work or with a command left running, or one waiting on you, and each is judged again on the
//! board as it is just before it's ended. One asked to end that didn't (something held it) is
//! asked again a while later. It goes by the clock as well as the board, so it has a thread of
//! its own rather than following what the engine publishes, which a board that hasn't changed
//! doesn't.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use crate::actions::{self, From};
use crate::engine::Handle;
use crate::idle::Ending;
use crate::model::{Board, PrItem, ReviewStage, Session, Status};
use crate::prlinks::pr_key;
use crate::util::now_ms;
use codealong_peer::api;

/// How long a review's session sits unused, with no round of its PR under way or due, before it's
/// ended. Of 579 later rounds over 13 days on one Mac (2026-10-23), 69% came within an hour of the
/// round before and 85% within six hours, each going on in the session as it ran; one later than
/// this resumes it.
const IDLE_MS: i64 = 6 * 60 * 60 * 1000;

/// How long before a session asked to end, and still due, is asked again: a turn it went back to,
/// or a command it left running, can have held it.
const AGAIN_MS: i64 = 10 * 60 * 1000;

/// How often the board is looked at.
const TICK: Duration = Duration::from_secs(60);

/// Why a review's session is done with.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Why {
    /// Its PR is merged or closed: marked done.
    PrEnded,
    /// The board no longer lists its PR, and it sat unused: marked done.
    Unlisted,
    /// It sat unused while its PR is open: ended, and kept for its PR's next round.
    Idle,
    /// Its turn stopped on an error, and its review went on in another session: marked done.
    Replaced,
}

impl Why {
    /// Marked done (ended and archived), rather than only ended.
    fn done(self) -> bool {
        self != Why::Idle
    }
}

/// A session to finish, and why.
#[derive(Debug, Clone, PartialEq)]
pub struct Due {
    pub key: String,
    pub why: Why,
}

/// What a review's PR says of its sessions, as the board lists it.
#[derive(Debug, Clone, Copy, PartialEq)]
struct PrNow {
    ended: bool,
    /// A round of it is under way, or due for commits its review hasn't seen.
    round: bool,
}

fn pr_now(p: &PrItem) -> PrNow {
    PrNow { ended: !p.state.is_open(), round: p.auto_review.as_ref().is_some_and(|a| a.stage == ReviewStage::Reviewing || a.behind) }
}

/// The sessions last asked to end, with when, so one is asked once until `AGAIN_MS` is up.
#[derive(Debug, Default)]
pub struct Finisher {
    asked: HashMap<String, i64>,
}

impl Finisher {
    /// The review sessions to finish now, at `now` (ms).
    pub fn due(&mut self, board: &Board, now: i64) -> Vec<Due> {
        let prs: HashMap<String, PrNow> = board.prs.iter().map(|p| (pr_key(&p.key), pr_now(p))).collect();
        self.due_among(board, &prs, board.github_polled, now)
    }

    /// `due`, with the board's PRs as `prs`, all of them once GitHub's first poll is over (`read`).
    fn due_among(&mut self, board: &Board, prs: &HashMap<String, PrNow>, read: bool, now: i64) -> Vec<Due> {
        let mut due = Vec::new();
        let mut still = HashSet::new();
        for s in board.lanes.iter().filter(|l| l.device.is_none()).flat_map(|l| &l.sessions) {
            let Some(pr) = s.helper.as_ref().and_then(|h| h.pr.as_deref()) else { continue };
            let Some(why) = why(board, s, prs.get(pr).copied(), read, now) else { continue };
            still.insert(s.key.clone());
            if self.asked.get(&s.key).is_some_and(|&at| now - at < AGAIN_MS) {
                continue;
            }
            self.asked.insert(s.key.clone(), now);
            due.push(Due { key: s.key.clone(), why });
        }
        self.asked.retain(|k, _| still.contains(k));
        due
    }
}

/// Why `s`, a review of a PR (`pr`, as the board lists it, None when it doesn't, which says the
/// PR is gone only once `read`), is done with now, if it is.
fn why(board: &Board, s: &Session, pr: Option<PrNow>, read: bool, now: i64) -> Option<Why> {
    let h = s.helper.as_ref()?;
    let replaced = s.status == Status::Error && replaced(board, s);
    // What's past for one that no longer runs (an error its last turn ended on) doesn't hold it,
    // nor an error whose review went on elsewhere.
    let busy = s.live && (matches!(s.status, Status::Working | Status::Needs) || (s.status == Status::Error && !replaced) || s.background.is_some());
    if h.yours || s.visitor.is_some() || busy {
        return None;
    }
    // Something of it Mark as done would end: Codealong's own sessions hold no process of their own,
    // and an app's can't be closed from outside.
    let ends = s.live && matches!(crate::idle::ending(board, s, false), Ok(Ending::Terminal(_) | Ending::Exit(_) | Ending::Job));
    let unused = now - s.last_activity >= IDLE_MS;
    let done = !s.archived || ends;
    match pr {
        Some(p) if p.ended => done.then_some(Why::PrEnded),
        _ if replaced => done.then_some(Why::Replaced),
        None if read => (done && unused).then_some(Why::Unlisted),
        p => (ends && unused && !p.is_some_and(|p| p.round)).then_some(Why::Idle),
    }
}

/// Another review of `s`'s PR on this Mac started after `s` last did anything: the round went on
/// there, as autoreview.rs has it do in a new session after a turn that stopped on an error.
fn replaced(board: &Board, s: &Session) -> bool {
    fn pr(s: &Session) -> Option<&str> {
        s.helper.as_ref()?.pr.as_deref()
    }
    let Some(mine) = pr(s) else { return false };
    board.lanes.iter().filter(|l| l.device.is_none()).flat_map(|l| &l.sessions).any(|o| o.key != s.key && pr(o) == Some(mine) && o.started_at.unwrap_or(o.last_prompt_at) > s.last_activity)
}

/// Why the review `key` is done with on `board` as it is now, if it still is.
fn due_now(board: &Board, key: &str, now: i64) -> Option<Why> {
    let (lane, s) = board.session(key)?;
    let pr = s.helper.as_ref()?.pr.as_deref().filter(|_| lane.device.is_none())?;
    let listed = board.prs.iter().find(|p| pr_key(&p.key) == pr).map(pr_now);
    why(board, s, listed, board.github_polled, now)
}

/// Finishes the sessions `due` says, one at a time, as the app's own requests do: Mark as done for
/// one whose PR ended or is gone, which archives one that no longer runs, its helpers too, and
/// Close for one that sat unused. Each is judged again on the board as it is just before: a round
/// may have come for it, or you may have taken it up, while the ones before it ended (archiving a
/// Codex thread alone can take seconds). What didn't go is logged, and one still due is asked
/// again later.
pub fn run(engine: &Handle, due: Vec<Due>) {
    for d in due {
        // The board as it is now: what came before took a while.
        if due_now(&engine.board(), &d.key, now_ms()) != Some(d.why) {
            continue;
        }
        let went = if d.why.done() {
            actions::local(engine, &From::Here, api::SESSION_DONE, &d.key, api::params::Done::default())
        } else {
            actions::local(engine, &From::Here, actions::CLOSE, &d.key, serde_json::json!({}))
        };
        match went {
            Ok(said) => eprintln!("codealong: the review {} ({:?}): {}", d.key, d.why, said.get("message").and_then(|m| m.as_str()).or(said.as_str()).unwrap_or_default()),
            Err(e) => eprintln!("codealong: the review {} wasn't ended: {e}", d.key),
        }
    }
}

/// Looks at the board every `TICK`, for the life of the app, and finishes what's due.
pub fn spawn(engine: Handle) {
    let started = std::thread::Builder::new().name("codealong-reviews-done".into()).spawn(move || {
        let mut finisher = Finisher::default();
        loop {
            std::thread::sleep(TICK);
            let board = engine.board();
            // Until the first scan is over, sessions and PRs are only what's come so far.
            if board.loading {
                continue;
            }
            let due = finisher.due(&board, now_ms());
            if !due.is_empty() {
                run(&engine, due);
            }
        }
    });
    if let Err(e) = started {
        eprintln!("codealong: reviews' sessions won't be finished by themselves: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;
    use crate::model::{Helper, Surface};

    const NOW: i64 = 1_800_000_000_000;
    const PR: &str = "o/r#7";

    /// A review of PR 7 alone in its terminal, last at work `ago` before `NOW`. Its process is
    /// named for its key's last letter, so each has its own.
    fn review(key: &str, ago: i64) -> Session {
        Session { surface: Surface::Cli, pid: key.chars().last().map(|c| c as u32), status: Status::Idle, last_activity: NOW - ago, helper: Some(Helper { pr: Some(PR.into()), ..Default::default() }), ..fixtures::session(key) }
    }

    fn board(sessions: Vec<Session>) -> Board {
        Board { lanes: vec![fixtures::lane("/r", sessions)], ..Board::default() }
    }

    fn prs(ended: bool, round: bool) -> HashMap<String, PrNow> {
        HashMap::from([(PR.to_string(), PrNow { ended, round })])
    }

    fn due(b: &Board, prs: &HashMap<String, PrNow>) -> Vec<(String, Why)> {
        Finisher::default().due_among(b, prs, true, NOW).into_iter().map(|d| (d.key, d.why)).collect()
    }

    fn keys(want: &[(&str, Why)]) -> Vec<(String, Why)> {
        want.iter().map(|(k, w)| (k.to_string(), *w)).collect()
    }

    /// Its PR merged or closed, a review is marked done: one that runs, and one that doesn't but
    /// isn't archived yet. One archived with nothing of it running is done with already.
    #[test]
    fn a_review_is_done_once_its_pr_ends() {
        let stopped = Session { live: false, ..review("rev-b", 60_000) };
        let archived = Session { key: "rev-c".into(), archived: true, ..stopped.clone() };
        let b = board(vec![review("rev-a", 60_000), stopped, archived]);
        assert_eq!(due(&b, &prs(true, false)), keys(&[("rev-a", Why::PrEnded), ("rev-b", Why::PrEnded)]));
    }

    /// Once the board no longer lists its PR (merged in a repo whose PRs aren't read), a review is
    /// marked done after it's sat unused, but not before GitHub's first poll says what's listed.
    #[test]
    fn a_review_whose_pr_is_gone_is_done_once_unused() {
        let b = board(vec![review("rev-a", IDLE_MS), Session { live: false, ..review("rev-b", IDLE_MS) }, review("rev-c", IDLE_MS - 1)]);
        let none = HashMap::new();
        assert_eq!(due(&b, &none), keys(&[("rev-a", Why::Unlisted), ("rev-b", Why::Unlisted)]));
        let unread: Vec<_> = Finisher::default().due_among(&b, &none, false, NOW).into_iter().map(|d| (d.key, d.why)).collect();
        assert_eq!(unread, keys(&[("rev-a", Why::Idle)]), "before GitHub's first poll, only ended as it sat unused");
    }

    /// While its PR is open, a review that runs is ended once it's sat unused for `IDLE_MS`, unless
    /// a round of its PR is under way or due.
    #[test]
    fn a_review_that_sat_unused_is_ended_unless_a_round_comes() {
        let b = board(vec![review("rev-a", IDLE_MS), review("rev-b", IDLE_MS - 1)]);
        assert_eq!(due(&b, &prs(false, false)), keys(&[("rev-a", Why::Idle)]));
        assert!(due(&b, &prs(false, true)).is_empty(), "a round is under way or due");
        // Nothing runs it: nothing to end.
        let stopped = board(vec![Session { live: false, ..review("rev-a", IDLE_MS) }]);
        assert!(due(&stopped, &prs(false, false)).is_empty());
    }

    /// Neither touches what you've taken up, work under way, or what waits on you; nor ends one
    /// Codealong runs or one sharing its terminal, though one whose PR ended is still archived.
    #[test]
    fn what_you_took_up_work_under_way_and_what_waits_on_you_stay() {
        let changed = |f: fn(&mut Session)| {
            let mut s = review("rev-a", IDLE_MS);
            f(&mut s);
            board(vec![s])
        };
        for (b, what) in [
            (changed(|s| s.helper.as_mut().unwrap().yours = true), "yours"),
            (changed(|s| s.helper = None), "made your own"),
            (changed(|s| s.status = Status::Working), "at work"),
            (changed(|s| s.status = Status::Needs), "waiting on you"),
            (changed(|s| s.background = Some("npm test".into())), "a command left running"),
        ] {
            assert!(due(&b, &prs(true, false)).is_empty(), "{what}, its PR ended");
            assert!(due(&b, &prs(false, false)).is_empty(), "{what}, unused");
            assert!(due(&b, &HashMap::new()).is_empty(), "{what}, its PR gone");
        }
        // One whose last turn ended on an error and that no longer runs is done with all the same.
        let errored = changed(|s| (s.live, s.status) = (false, Status::Error));
        assert_eq!(due(&errored, &prs(true, false)), keys(&[("rev-a", Why::PrEnded)]));
        let hosted = changed(|s| s.hosted = true);
        assert!(due(&hosted, &prs(false, false)).is_empty(), "Codealong runs it");
        assert_eq!(due(&hosted, &prs(true, false)), keys(&[("rev-a", Why::PrEnded)]), "archived all the same");
        let beside = Session { pid: review("rev-a", 0).pid, ..fixtures::session("opencode:opencode:other") };
        let shared = board(vec![review("rev-a", IDLE_MS), beside]);
        assert!(due(&shared, &prs(false, false)).is_empty(), "its terminal shows another session");
    }

    /// One whose turn stopped on an error is marked done once another review of its PR started
    /// after it, which its round went on in, whether it still runs or not and whatever its PR:
    /// nothing in it waits on you. Not while nothing took it over, nor for a review of another PR,
    /// nor once you've taken it up or with a command it left running.
    #[test]
    fn an_errored_review_is_done_once_another_took_its_round() {
        let errored = Session { status: Status::Error, ..review("rev-a", 60_000) };
        let after = |key: &str, by: i64| Session { started_at: Some(errored.last_activity + by), ..review(key, 0) };
        let with = |s: Session, other: Session| board(vec![s, other]);
        assert!(due(&board(vec![errored.clone()]), &prs(false, true)).is_empty(), "nothing took it over");
        assert!(due(&with(errored.clone(), after("rev-b", -1)), &prs(false, true)).is_empty(), "one that started before its error");
        let elsewhere = Session { helper: Some(Helper { pr: Some("o/r#8".into()), ..Default::default() }), ..after("rev-b", 1) };
        assert!(due(&with(errored.clone(), elsewhere), &prs(false, true)).is_empty(), "a review of another PR");

        let replaced = |s: Session| due(&with(s, after("rev-b", 1)), &prs(false, true));
        assert_eq!(replaced(errored.clone()), keys(&[("rev-a", Why::Replaced)]));
        assert_eq!(replaced(Session { live: false, ..errored.clone() }), keys(&[("rev-a", Why::Replaced)]), "no longer running");
        assert_eq!(due(&with(errored.clone(), after("rev-b", 1)), &HashMap::new()), keys(&[("rev-a", Why::Replaced)]), "its PR no longer listed");
        let mut yours = errored.clone();
        yours.helper.as_mut().unwrap().yours = true;
        assert!(replaced(yours).is_empty(), "taken up");
        assert!(replaced(Session { background: Some("npm test".into()), ..errored.clone() }).is_empty(), "a command left running");
        assert!(replaced(Session { status: Status::Idle, ..errored }).is_empty(), "no error: the usual rules");
    }

    /// One asked to end is asked once, and again only once `AGAIN_MS` is up while it's still due;
    /// one no longer due is forgotten.
    #[test]
    fn a_review_is_asked_once_and_again_only_while_still_due() {
        let mut f = Finisher::default();
        let b = board(vec![review("rev-a", IDLE_MS)]);
        assert_eq!(f.due_among(&b, &prs(false, false), true, NOW).len(), 1);
        assert!(f.due_among(&b, &prs(false, false), true, NOW + AGAIN_MS - 1).is_empty());
        assert_eq!(f.due_among(&b, &prs(false, false), true, NOW + AGAIN_MS).len(), 1);
        assert!(f.due_among(&b, &prs(false, true), true, NOW + 2 * AGAIN_MS).is_empty());
        assert!(f.asked.is_empty(), "forgotten once a round came");
    }

    /// Just before it's ended, a review is judged again on the board as it is then: one a round
    /// went back to, or that you took up, meanwhile isn't.
    #[test]
    fn a_review_is_judged_again_just_before_its_end() {
        let mut b = Board { github_polled: true, ..board(vec![review("rev-a", IDLE_MS)]) };
        assert_eq!(due_now(&b, "rev-a", NOW), Some(Why::Unlisted));
        b.lanes[0].sessions[0].helper.as_mut().unwrap().yours = true;
        assert_eq!(due_now(&b, "rev-a", NOW), None, "taken up meanwhile");
        b.lanes[0].sessions[0].helper.as_mut().unwrap().yours = false;
        b.lanes[0].sessions[0].status = Status::Working;
        assert_eq!(due_now(&b, "rev-a", NOW), None, "at work again");
        assert_eq!(due_now(&b, "gone", NOW), None);
    }
}
