//! Telling a session to carry on once a lost connection has ended its turn, unless config.toml
//! turns it off (`[sessions] continue_after_network_error`). A harness retries a request that
//! can't connect for only a few minutes of the Mac being awake, and with the lid closed the Mac
//! wakes for a few seconds every minute or two without DNS: the retries run out there, and the
//! session waits for a prompt when the lid opens (claude/transcript.rs `connection_lost_at`).
//!
//! Every board the engine publishes is looked at for sessions that run here with such a turn
//! (`Session::connection_lost`, which the engine marks `continues`). Once their API can be
//! reached from here (the harness's `api_address`, connected to for a moment), each is sent a
//! message saying to carry on, as a message of yours from a window is (actions.rs
//! `session-send`), on a thread of its own, since connecting and sending take a moment.
//!
//! Each lost connection is answered once. A session that loses it again straight after it was
//! answered is answered again, a few times in a row at most; then it's left for you, and a notice
//! says so. A connection lost long ago is left as it is, and so is a session waiting on you to
//! answer a prompt (what's typed would answer it). While the API can't be reached it's tried again
//! every half minute; a send that fails is tried again a little later, a few times.

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::time::Duration;

use crate::engine::Handle;
use crate::model::{Board, Session};

/// A connection lost longer ago than this is left as it is: you've seen it, or it ended a turn
/// that no longer matters. A night with the lid closed is well inside it.
const MAX_AGE_MS: i64 = 24 * 60 * 60_000;

/// A connection lost again within this long of being answered was lost straight after.
const AGAIN_MS: i64 = 10 * 60_000;

/// How many answers in a row may each be lost again straight after before it's left for you.
const AGAIN_MAX: u32 = 3;

/// How long after finding the API can't be reached it's tried again.
const OFFLINE_RETRY_MS: i64 = 30_000;

/// How many times a send is tried before it's given up on, and how long after a failed one it's
/// tried again: the first wait, doubled each time.
const TRIES: u32 = 4;
const RETRY_MS: i64 = 30_000;

/// How long a connection to the API is given to open.
const REACH_TIMEOUT: Duration = Duration::from_secs(5);

/// What Codealong says to a session it tells to carry on.
pub const MESSAGE: &str = "Your last turn ended because the API couldn't be reached (the Mac slept or lost its connection). It can be reached again, so carry on where you left off.";

/// Telling one session to carry on from the connection lost at `at`.
#[derive(Debug, Clone, PartialEq)]
pub struct Job {
    pub session: String,
    pub at: i64,
    /// Where its API is (`Adapter::api_address`), tried first.
    pub address: &'static str,
}

/// What's due on a board: sessions to tell, and the titles of those left for you just now.
#[derive(Debug, Default, PartialEq)]
pub struct Due {
    pub jobs: Vec<Job>,
    pub left: Vec<String>,
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
    /// The lost connection last seen (its time).
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

/// Where each session that lost its connection stands, so each loss is answered once.
#[derive(Default)]
pub struct Continuer {
    runs: HashMap<String, Run>,
}

impl Continuer {
    /// What's due at `now` (ms): each session that runs here, whose last turn a lost connection
    /// ended in the last day and that Codealong is to carry on, unless it's been answered, waits on
    /// you, has a job under way or a wait that isn't over. A session the board no longer lists is
    /// forgotten; one that stopped running and came back keeps its answers, so no loss is answered
    /// twice.
    pub fn due(&mut self, board: &Board, now: i64) -> Due {
        let here: Vec<&Session> = board.lanes.iter().filter(|l| l.device.is_none()).flat_map(|l| &l.sessions).filter(|s| s.visitor.is_none()).collect();
        let listed: HashSet<&str> = here.iter().map(|s| s.key.as_str()).collect();
        self.runs.retain(|k, r| r.sending || listed.contains(k.as_str()));
        let mut due = Due::default();
        for s in here {
            if !s.live || s.archived {
                continue;
            }
            let Some(lost) = s.connection_lost.as_ref().filter(|l| l.continues && now - l.at <= MAX_AGE_MS) else { continue };
            let Some(address) = crate::harness::adapter(s.harness).api_address() else { continue };
            let run = self.runs.entry(s.key.clone()).or_default();
            if run.at != lost.at {
                // A loss not seen before: lost again straight after the last answer, or afresh.
                run.again = if run.sent_at.is_some_and(|t| lost.at - t <= AGAIN_MS) { run.again + 1 } else { 0 };
                *run = Run { at: lost.at, sending: run.sending, sent_at: run.sent_at, again: run.again, ..Run::default() };
                if run.again >= AGAIN_MAX {
                    run.done = true;
                    due.left.push(s.title.clone());
                }
            }
            if run.done || run.sending || now < run.next_at || crate::send::waits_on_you(s) {
                continue;
            }
            run.sending = true;
            due.jobs.push(Job { session: s.key.clone(), at: lost.at, address });
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

/// Says who was left for you, then does each job: tries the API's address (once for the jobs that
/// share it), and sends the session the message as a window's send does; tells `continuer` how
/// each went, and the engine, which says so in a notice when something didn't go.
pub fn send_now(engine: &Handle, continuer: &Mutex<Continuer>, due: Due) {
    let mut said: Vec<String> = due.left.iter().map(|t| format!("“{t}” lost its connection again each time it was told to carry on, so it's left for you.")).collect();
    let mut reached: HashMap<&str, bool> = HashMap::new();
    for job in due.jobs {
        let went = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if !*reached.entry(job.address).or_insert_with(|| reachable(job.address)) {
                return Ok(Went::Offline);
            }
            crate::send::tell(engine, &job.session, MESSAGE).map(|_| Went::Sent).map_err(|e| format!("{e:#}"))
        }))
        .unwrap_or_else(|_| Err("telling it stopped partway".to_string()));
        let again = continuer.lock().unwrap_or_else(|e| e.into_inner()).told(&job, *went.as_ref().unwrap_or(&Went::Failed), crate::util::now_ms());
        if let Err(e) = went {
            let title = engine.read(|b| b.session(&job.session).map(|(_, s)| s.title.clone())).unwrap_or_else(|| job.session.clone());
            let then = if again { " Codealong tries again in a minute or two." } else { "" };
            said.push(format!("“{title}” lost its connection, and telling it to carry on didn't work: {e}.{then}"));
        }
    }
    engine.continued(if said.is_empty() { Ok(()) } else { Err(said.join(" ")) });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ConnectionLost, Status};

    const NOW: i64 = 1_800_000_000_000;
    const A: &str = "claude:claude:a";
    const B: &str = "claude:claude:b";

    fn lost(key: &str, at: i64) -> Session {
        Session { live: true, status: Status::Unread, title: key.rsplit(':').next().unwrap().into(), connection_lost: Some(ConnectionLost { at, continues: true }), ..crate::fixtures::session(key) }
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
        assert_eq!(due.jobs, vec![Job { session: A.into(), at: NOW - 60_000, address: "api.anthropic.com:443" }]);
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
        assert_eq!((due.jobs.len(), due.left), (0, vec!["a".to_string()]));
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
