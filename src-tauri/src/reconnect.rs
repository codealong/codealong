//! Telling a session to carry on once what stopped its turn is over: a lost connection, unless
//! config.toml turns it off (`[sessions] continue_after_network_error`), or its account's usage
//! limit, once the limit lifts (`[limits] continue_when_lifted`). A harness retries a request that
//! can't connect for only a few minutes of the Mac being awake, and with the lid closed the Mac
//! wakes for a few seconds every minute or two without DNS: the retries run out there, and the
//! session waits for a prompt when the lid opens (claude/transcript.rs `connection_lost_at`). A
//! usage limit stops a session until it lifts, hours or days away.
//!
//! Every board the engine publishes is looked at for sessions that run here with such a turn
//! (`Session::connection_lost` and `Session::limit`, which the engine marks `continues`). Once
//! their API can be reached from here (the harness's `api_address`, connected to for a moment), or
//! a few minutes after their limit lifted, each is sent a message saying to carry on, as a message
//! of yours from a window is (actions.rs `session-send`), on a thread of its own, since connecting
//! and sending take a moment.
//!
//! Each stop is answered once. A session stopped again straight after it was answered is answered
//! again, a few times in a row at most; then it's left for you, and a notice says so. A stop long
//! ago is left as it is, and so is a session waiting on you to answer a prompt (what's typed would
//! answer it). While the API can't be reached it's tried again every half minute; a send that
//! fails is tried again a little later, a few times.

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::time::Duration;

use crate::engine::Handle;
use crate::model::{Board, Session};

/// A connection lost, or a limit lifted, longer ago than this is left as it is: you've seen it,
/// or it ended a turn that no longer matters. A night with the lid closed is well inside it.
const MAX_AGE_MS: i64 = 24 * 60 * 60_000;

/// How long after a limit lifts a session still stopped there is told to carry on. Claude Code
/// carries on by itself 30 to 90 seconds after (2.1.300's `autoContinueAtUsageLimit`, on by
/// default where Anthropic turns it on for the account; nothing in the transcript says whether
/// it will), and then has a turn under way and no limit, so it isn't told twice.
const LIFTED_GRACE_MS: i64 = 3 * 60_000;

/// A session stopped again within this long of being answered was stopped straight after.
const AGAIN_MS: i64 = 10 * 60_000;

/// How many answers in a row may each be stopped again straight after before it's left for you.
const AGAIN_MAX: u32 = 3;

/// How long after finding the API can't be reached it's tried again.
const OFFLINE_RETRY_MS: i64 = 30_000;

/// How many times a send is tried before it's given up on, and how long after a failed one it's
/// tried again: the first wait, doubled each time.
const TRIES: u32 = 4;
const RETRY_MS: i64 = 30_000;

/// How long a connection to the API is given to open.
const REACH_TIMEOUT: Duration = Duration::from_secs(5);

/// What stopped a session's turn.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Why {
    /// A lost connection (`Session::connection_lost`).
    Connection,
    /// Its account's usage limit (`Session::limit`).
    Limit,
}

impl Why {
    /// What Codealong says to a session it tells to carry on.
    pub fn message(self) -> &'static str {
        match self {
            Why::Connection => "Your last turn ended because the API couldn't be reached (the Mac slept or lost its connection). It can be reached again, so carry on where you left off.",
            Why::Limit => "Your last turn stopped at your account's usage limit, which has lifted now, so carry on where you left off.",
        }
    }

    /// What stopped it, after its title.
    fn stopped(self) -> &'static str {
        match self {
            Why::Connection => "lost its connection",
            Why::Limit => "stopped at its usage limit",
        }
    }
}

/// Telling one session to carry on from what stopped it at `at`.
#[derive(Debug, Clone, PartialEq)]
pub struct Job {
    pub session: String,
    pub at: i64,
    pub why: Why,
    /// Where its API is (`Adapter::api_address`), tried first after a lost connection.
    pub address: Option<&'static str>,
}

/// What's due on a board: sessions to tell, and the titles of those left for you just now, with
/// what stopped them.
#[derive(Debug, Default, PartialEq)]
pub struct Due {
    pub jobs: Vec<Job>,
    pub left: Vec<(String, Why)>,
}

impl Due {
    pub fn is_empty(&self) -> bool {
        self.jobs.is_empty() && self.left.is_empty()
    }
}

/// How a job went.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Went {
    Sent,
    /// The API couldn't be reached: nothing was sent.
    Offline,
    Failed,
}

/// Where one session stands.
#[derive(Debug, Default)]
struct Run {
    /// The stop last seen (its time).
    at: i64,
    /// It's been answered, or left for you.
    done: bool,
    /// A job for it is under way.
    sending: bool,
    /// Sends that failed in a row.
    failures: u32,
    /// Not tried again before this (ms).
    next_at: i64,
    /// When it was last answered.
    sent_at: Option<i64>,
    /// Answers in a row each lost again straight after.
    again: u32,
}

/// What `s` is owed at `now` (ms), when anything: what stopped it and when, and where its API is
/// to try first. A lost connection is owed an answer at once, in the day after; a limit once it's
/// lifted and Claude Code's own carrying on has had time to show (`LIFTED_GRACE_MS`), in the day
/// after that.
fn owed(s: &Session, now: i64) -> Option<(Why, i64, Option<&'static str>)> {
    if let Some(lost) = s.connection_lost.as_ref().filter(|l| l.continues && now - l.at <= MAX_AGE_MS) {
        let address = crate::harness::adapter(s.harness).api_address()?;
        return Some((Why::Connection, lost.at, Some(address)));
    }
    let limit = s.limit.as_ref().filter(|l| l.continues)?;
    let lifted = limit.resets_at? + LIFTED_GRACE_MS;
    (now >= lifted && now - lifted <= MAX_AGE_MS).then_some((Why::Limit, limit.at, None))
}

/// The sessions on `board` whose limit has lifted long enough at `now` to be carried on
/// (`owed`). Nothing the windows get says so, and it comes with time alone, so the board's hash
/// takes it in (engine.rs `board_hash`): the board that's composed then goes out, and `due` runs
/// on it.
pub fn lifted(board: &Board, now: i64) -> Vec<String> {
    board.lanes.iter().flat_map(|l| &l.sessions).filter(|s| matches!(owed(s, now), Some((Why::Limit, ..)))).map(|s| s.key.clone()).collect()
}

/// Where each session that stopped stands, so each stop is answered once.
#[derive(Default)]
pub struct Continuer {
    runs: HashMap<String, Run>,
}

impl Continuer {
    /// What's due at `now` (ms): each session that runs here, whose last turn a lost connection or
    /// a limit that has lifted stopped (`owed`) and that Codealong is to carry on, unless it's been
    /// answered, waits on you, has a job under way or a wait that isn't over. A session the board
    /// no longer lists is forgotten; one that stopped running and came back keeps its answers, so
    /// no stop is answered twice.
    pub fn due(&mut self, board: &Board, now: i64) -> Due {
        let here: Vec<&Session> = board.lanes.iter().filter(|l| l.device.is_none()).flat_map(|l| &l.sessions).filter(|s| s.visitor.is_none()).collect();
        let listed: HashSet<&str> = here.iter().map(|s| s.key.as_str()).collect();
        self.runs.retain(|k, r| r.sending || listed.contains(k.as_str()));
        let mut due = Due::default();
        for s in here {
            if !s.live || s.archived {
                continue;
            }
            let Some((why, at, address)) = owed(s, now) else { continue };
            let run = self.runs.entry(s.key.clone()).or_default();
            if run.at != at {
                // A stop not seen before: stopped again straight after the last answer, or afresh.
                run.again = if run.sent_at.is_some_and(|t| at - t <= AGAIN_MS) { run.again + 1 } else { 0 };
                *run = Run { at, sending: run.sending, sent_at: run.sent_at, again: run.again, ..Run::default() };
                if run.again >= AGAIN_MAX {
                    run.done = true;
                    due.left.push((s.title.clone(), why));
                }
            }
            if run.done || run.sending || now < run.next_at || crate::send::waits_on_you(s) {
                continue;
            }
            run.sending = true;
            due.jobs.push(Job { session: s.key.clone(), at, why, address });
        }
        due.jobs.sort_by(|a, b| a.session.cmp(&b.session));
        due
    }

    /// How a job went, at `now` (ms): sent, it's answered; the API out of reach, it's tried
    /// again shortly; a send that failed, later, unless it's failed too often. Whether it will be.
    pub fn told(&mut self, job: &Job, went: Went, now: i64) -> bool {
        let Some(run) = self.runs.get_mut(&job.session) else { return false };
        run.sending = false;
        if run.at != job.at {
            return false;
        }
        match went {
            Went::Sent => {
                run.done = true;
                run.sent_at = Some(now);
                false
            }
            Went::Offline => {
                run.next_at = now + OFFLINE_RETRY_MS;
                true
            }
            Went::Failed => {
                run.failures += 1;
                if run.failures >= TRIES {
                    run.done = true;
                    return false;
                }
                run.next_at = now + (RETRY_MS << (run.failures - 1));
                true
            }
        }
    }
}

/// Whether `address` (`host:port`) takes a connection now: found, and answering, within a few
/// seconds.
fn reachable(address: &str) -> bool {
    use std::net::ToSocketAddrs;
    let Ok(found) = address.to_socket_addrs() else { return false };
    found.into_iter().any(|a| std::net::TcpStream::connect_timeout(&a, REACH_TIMEOUT).is_ok())
}

/// Says who was left for you, then does each job: tries the API's address after a lost
/// connection (once for the jobs that share it), and sends the session the message as a window's
/// send does; tells `continuer` how each went, and the engine, which says so in a notice when
/// something didn't go.
pub fn send_now(engine: &Handle, continuer: &Mutex<Continuer>, due: Due) {
    let mut said: Vec<String> = due.left.iter().map(|(t, why)| format!("“{t}” {} again each time it was told to carry on, so it's left for you.", why.stopped())).collect();
    let mut reached: HashMap<&str, bool> = HashMap::new();
    for job in due.jobs {
        let went = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if job.address.is_some_and(|a| !*reached.entry(a).or_insert_with(|| reachable(a))) {
                return Ok(Went::Offline);
            }
            crate::send::tell(engine, &job.session, job.why.message()).map(|_| Went::Sent).map_err(|e| format!("{e:#}"))
        }))
        .unwrap_or_else(|_| Err("telling it stopped partway".to_string()));
        let again = continuer.lock().unwrap_or_else(|e| e.into_inner()).told(&job, *went.as_ref().unwrap_or(&Went::Failed), crate::util::now_ms());
        if let Err(e) = went {
            let title = engine.read(|b| b.session(&job.session).map(|(_, s)| s.title.clone())).unwrap_or_else(|| job.session.clone());
            let then = if again { " Codealong tries again in a minute or two." } else { "" };
            said.push(format!("“{title}” {}, and telling it to carry on didn't work: {e}.{then}", job.why.stopped()));
        }
    }
    engine.continued(if said.is_empty() { Ok(()) } else { Err(said.join(" ")) });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ConnectionLost, SessionLimit, Status};

    const NOW: i64 = 1_800_000_000_000;
    const A: &str = "claude:claude:a";
    const B: &str = "claude:claude:b";

    fn lost(key: &str, at: i64) -> Session {
        Session { live: true, status: Status::Unread, title: key.rsplit(':').next().unwrap().into(), connection_lost: Some(ConnectionLost { at, continues: true }), ..crate::fixtures::session(key) }
    }

    /// A session stopped at its limit at `at`, which lifts at `lifts`.
    fn limited(key: &str, at: i64, lifts: i64) -> Session {
        let limit = SessionLimit { at, resets_at: Some(lifts), continues: true, ..SessionLimit::default() };
        Session { live: true, status: Status::Error, title: key.rsplit(':').next().unwrap().into(), limit: Some(limit), ..crate::fixtures::session(key) }
    }

    fn board(sessions: Vec<Session>) -> Board {
        Board { lanes: vec![crate::fixtures::lane("/work", sessions)], ..Board::default() }
    }

    fn keys(due: &Due) -> Vec<&str> {
        due.jobs.iter().map(|j| j.session.as_str()).collect()
    }

    #[test]
    fn a_lost_connection_is_answered_once_it_can_be_reached() {
        let mut c = Continuer::default();
        let b = board(vec![lost(A, NOW - 60_000), Session { live: true, ..crate::fixtures::session(B) }]);
        let due = c.due(&b, NOW);
        assert_eq!(due.jobs, vec![Job { session: A.into(), at: NOW - 60_000, why: Why::Connection, address: Some("api.anthropic.com:443") }]);
        assert!(c.due(&b, NOW).is_empty(), "under way");
        // Out of reach: nothing sent, tried again in half a minute.
        assert!(c.told(&due.jobs[0], Went::Offline, NOW));
        assert!(c.due(&b, NOW + 1_000).is_empty());
        let again = c.due(&b, NOW + OFFLINE_RETRY_MS);
        assert_eq!(keys(&again), [A]);
        assert!(!c.told(&again.jobs[0], Went::Sent, NOW + OFFLINE_RETRY_MS));
        assert!(c.due(&b, NOW + 10 * OFFLINE_RETRY_MS).is_empty(), "answered once");
    }

    #[test]
    fn only_what_codealong_is_to_carry_on_and_runs_here_now() {
        use crate::model::DeviceTag;
        let mut c = Continuer::default();
        let off = Session { connection_lost: Some(ConnectionLost { at: NOW, continues: false }), ..lost(A, NOW) };
        let stopped = Session { live: false, ..lost(A, NOW) };
        let archived = Session { archived: true, ..lost(A, NOW) };
        let old = lost(A, NOW - MAX_AGE_MS - 1);
        let waits = Session { status: Status::Needs, ..lost(A, NOW) };
        for s in [off, stopped, archived, old, waits] {
            assert!(c.due(&board(vec![s.clone()]), NOW).is_empty(), "{s:?}");
        }
        let mut elsewhere = board(vec![lost(A, NOW)]);
        elsewhere.lanes[0].device = Some(DeviceTag { name: "Studio".into(), short: "st".into(), color: "blue".into(), online: true, last_seen: None });
        assert!(c.due(&elsewhere, NOW).is_empty());
        // A harness that doesn't say where its API is isn't carried on.
        let codex = Session { harness: crate::model::Harness::Codex, ..lost("codex:codex:c", NOW) };
        assert!(c.due(&board(vec![codex]), NOW).is_empty());
        // Answered once it's done waiting on you.
        assert_eq!(keys(&c.due(&board(vec![lost(A, NOW)]), NOW)), [A]);
    }

    #[test]
    fn a_limit_is_answered_once_it_has_lifted_and_its_agent_hasnt_gone_on_by_itself() {
        let mut c = Continuer::default();
        let lifts = NOW + 3 * 3_600_000;
        let b = board(vec![limited(A, NOW - 60_000, lifts)]);
        assert!(c.due(&b, NOW).is_empty(), "still stopped");
        // Lifted, but Claude Code may carry on by itself first.
        assert!(c.due(&b, lifts + 60_000).is_empty());
        assert!(lifted(&b, lifts + 60_000).is_empty());
        assert_eq!(lifted(&b, lifts + LIFTED_GRACE_MS), [A]);
        let due = c.due(&b, lifts + LIFTED_GRACE_MS);
        assert_eq!(due.jobs, vec![Job { session: A.into(), at: NOW - 60_000, why: Why::Limit, address: None }]);
        assert!(!c.told(&due.jobs[0], Went::Sent, lifts + LIFTED_GRACE_MS));
        assert!(c.due(&b, lifts + 2 * LIFTED_GRACE_MS).is_empty(), "answered once");
        // One that went on by itself works, and its limit is behind it.
        let mut c = Continuer::default();
        let went_on = Session { status: Status::Working, limit: None, ..limited(A, NOW - 60_000, lifts) };
        assert!(c.due(&board(vec![went_on]), lifts + LIFTED_GRACE_MS).is_empty());
        // Stopped at its limit again straight after a few answers, it's left for you.
        let mut at = lifts;
        for _ in 0..AGAIN_MAX {
            let due = c.due(&board(vec![limited(A, at, at - LIFTED_GRACE_MS)]), at);
            assert_eq!(keys(&due), [A]);
            c.told(&due.jobs[0], Went::Sent, at);
            at += 60_000;
        }
        let due = c.due(&board(vec![limited(A, at, at - LIFTED_GRACE_MS)]), at);
        assert_eq!((due.jobs.len(), due.left), (0, vec![("a".to_string(), Why::Limit)]));
    }

    #[test]
    fn only_a_limit_codealong_is_to_carry_on_that_said_when_it_lifts() {
        let mut c = Continuer::default();
        let lifted = NOW - LIFTED_GRACE_MS;
        let off = Session { limit: Some(SessionLimit { continues: false, ..limited(A, NOW - 3_600_000, lifted).limit.unwrap() }), ..limited(A, NOW - 3_600_000, lifted) };
        let unsaid = Session { limit: Some(SessionLimit { resets_at: None, ..limited(A, NOW - 3_600_000, lifted).limit.unwrap() }), ..limited(A, NOW - 3_600_000, lifted) };
        let long_ago = limited(A, NOW - 3 * MAX_AGE_MS, NOW - 2 * MAX_AGE_MS);
        let stopped = Session { live: false, ..limited(A, NOW - 3_600_000, lifted) };
        let archived = Session { archived: true, ..limited(A, NOW - 3_600_000, lifted) };
        for s in [off, unsaid, long_ago, stopped, archived] {
            assert!(c.due(&board(vec![s.clone()]), NOW).is_empty(), "{s:?}");
        }
        // Any harness: it needs no address.
        let agy = Session { harness: crate::model::Harness::Antigravity, ..limited("antigravity:antigravity:g", NOW - 3_600_000, lifted) };
        assert_eq!(keys(&c.due(&board(vec![agy]), NOW)), ["antigravity:antigravity:g"]);
    }

    #[test]
    fn lost_again_straight_after_a_few_times_is_left_for_you() {
        let mut c = Continuer::default();
        let mut at = NOW;
        for i in 0..AGAIN_MAX {
            let due = c.due(&board(vec![lost(A, at)]), at);
            assert_eq!((keys(&due), due.left.len()), (vec![A], 0), "answer {i}");
            assert!(!c.told(&due.jobs[0], Went::Sent, at));
            at += 2 * 60_000;
        }
        let due = c.due(&board(vec![lost(A, at)]), at);
        assert_eq!((due.jobs.len(), due.left), (0, vec![("a".to_string(), Why::Connection)]));
        assert!(c.due(&board(vec![lost(A, at)]), at + 60_000).is_empty(), "said once");
        // Lost long after an answer, it's answered afresh.
        let mut c = Continuer::default();
        let due = c.due(&board(vec![lost(A, NOW)]), NOW);
        c.told(&due.jobs[0], Went::Sent, NOW);
        let later = NOW + AGAIN_MS + 1;
        for i in 0..AGAIN_MAX + 1 {
            let at = later + i as i64 * 3 * AGAIN_MS;
            let due = c.due(&board(vec![lost(A, at)]), at);
            assert_eq!(keys(&due), [A], "loss {i}");
            c.told(&due.jobs[0], Went::Sent, at);
        }
    }

    #[test]
    fn a_failed_send_is_tried_again_later_a_few_times() {
        let mut c = Continuer::default();
        let b = board(vec![lost(A, NOW)]);
        let mut at = NOW;
        let mut waits = Vec::new();
        let mut job = c.due(&b, at).jobs.remove(0);
        for _ in 1..TRIES {
            assert!(c.told(&job, Went::Failed, at));
            let (next, again) = (at..).step_by(1000).find_map(|t| c.due(&b, t).jobs.pop().map(|j| (t, j))).unwrap();
            waits.push((next - at) / 1000);
            (at, job) = (next, again);
        }
        assert_eq!(waits, [30, 60, 120]);
        assert!(!c.told(&job, Went::Failed, at), "given up");
        assert!(c.due(&b, at + 60 * 60_000).is_empty());
        // A loss after that is answered.
        assert_eq!(keys(&c.due(&board(vec![lost(A, at + 1)]), at + 2)), [A]);
    }

    /// Needs the internet: `cargo test reaches_the_api -- --ignored`.
    #[test]
    #[ignore]
    fn reaches_the_api() {
        assert!(reachable("api.anthropic.com:443"));
        assert!(!reachable("no-such-host.invalid:443"));
    }

    #[test]
    fn no_loss_is_answered_twice_and_a_session_gone_from_the_board_is_forgotten() {
        let mut c = Continuer::default();
        let due = c.due(&board(vec![lost(A, NOW)]), NOW);
        c.told(&due.jobs[0], Went::Sent, NOW);
        // Stopped and running again, with the same loss: answered already.
        assert!(c.due(&board(vec![Session { live: false, ..lost(A, NOW) }]), NOW + 1).is_empty());
        assert!(c.due(&board(vec![lost(A, NOW)]), NOW + 2).is_empty());
        assert!(c.due(&board(vec![]), NOW + 3).is_empty());
        assert!(c.runs.is_empty());
        // A job under way is kept until it's told how it went.
        let due = c.due(&board(vec![lost(B, NOW)]), NOW + 4);
        c.due(&board(vec![]), NOW + 5);
        assert!(!c.told(&due.jobs[0], Went::Sent, NOW + 6));
        assert!(c.runs.contains_key(B));
    }
}
