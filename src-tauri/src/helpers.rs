//! Telling a session what its helpers do, for the helpers it started with `codealong start
//! --helper --report` (origins.json, `Helper::report`): when a helper's turn ends, and when it
//! starts waiting on its user (a call to allow, a question, an error), its session is sent a
//! message saying so, as Codealong's own and not yours (send.rs `tell`), on a thread of its own,
//! as PR nudges are (nudges.rs). The message says which helper it is and how to
//! read what it wrote (`codealong session wait`), and not what it wrote: a helper's words reach
//! its session as that command's output, never typed in as if you'd written them, and a session
//! that skips permission prompts takes a message from Codealong without holding it for you.
//!
//! Every board the engine publishes is looked at. A helper first seen is only noted, so a restart
//! doesn't tell a turn that ended before it; after that, a turn that ends or a wait on you that
//! begins is owed to its session while that session runs on this Mac. A session waiting on you to
//! answer a prompt isn't sent anything (what's typed would answer it) and is told once it isn't;
//! what's owed to one session goes in one message; a send that fails is tried again a little
//! later, a few times.
//!
//! A helper started in a terminal whose session hasn't shown a few minutes on is told of too,
//! once (`origins::Pending::stuck`): its terminal UI likely sits on a screen of its own, and the
//! session that started it, which has no key to wait on, would otherwise wait on it unseen.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Mutex;

use crate::engine::Handle;
use crate::model::{Board, Session, Status};
use crate::origins::Pending;

/// How many times a send is tried before it's given up on, and how long after a failed one it's
/// tried again: the first wait, doubled each time.
const TRIES: u32 = 4;
const RETRY_MS: i64 = 30_000;

/// Where a helper stands, as a board shows it.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Stand {
    Working,
    /// Waiting on its user, since then.
    OnUser(i64),
    /// Not in a turn, its last prompt then.
    Still(i64),
}

fn stand(s: &Session) -> Stand {
    match s.status {
        Status::Working => Stand::Working,
        Status::Needs | Status::Error => Stand::OnUser(s.status_since.unwrap_or(s.last_activity)),
        Status::Unread | Status::Idle | Status::Stopped => Stand::Still(s.last_prompt_at),
    }
}

/// What a helper did, to tell its session.
#[derive(Debug, Clone, PartialEq)]
pub enum News {
    /// Its turn ended.
    Ended,
    /// It waits on its user: what for, as the board says, when it does.
    OnUser(Option<String>),
    /// Started in a terminal some minutes ago, it hasn't made its session yet
    /// (`origins::Pending::stuck`): its terminal likely waits on its user.
    Unstarted { account: String, place: String, terminal: String, minutes: i64 },
}

/// News of one helper.
#[derive(Debug, Clone, PartialEq)]
pub struct Note {
    pub helper: String,
    pub title: String,
    pub news: News,
}

/// What a session is still to be told.
#[derive(Debug, Default)]
struct Owed {
    /// By helper: the latest news of each.
    notes: BTreeMap<String, Note>,
    /// What's gone to a send that hasn't said how it went.
    sending: Option<Vec<Note>>,
    /// Sends that failed in a row.
    failures: u32,
    /// Not tried again before this (ms).
    next_at: i64,
}

/// A message to send a session: what its helpers did.
#[derive(Debug, Clone, PartialEq)]
pub struct Job {
    pub session: String,
    pub notes: Vec<Note>,
}

/// Where each reporting helper stood on the last board, so each change is noticed once, and what
/// each session is still to be told.
#[derive(Default)]
pub struct Reporter {
    seen: HashMap<String, Stand>,
    /// By the session told.
    owed: HashMap<String, Owed>,
    /// The stuck starts told of, by the session that asked and when it did.
    told_stuck: HashSet<(String, i64)>,
}

impl Reporter {
    /// The messages due now, at `now` (ms). A reporting helper's turn ends when the last board
    /// had it working or waiting on its user and this one has it still, or when it's still on
    /// both with a newer prompt it has answered since (a turn quicker than a board apart); a
    /// wait on its user begins when this board has it waiting and the last didn't, or not since
    /// then. A reporting helper started in a terminal that's stuck before it made its session
    /// (`pending`, `Pending::stuck`) is told of once. Each is owed to the helper's session while
    /// that runs on this Mac.
    pub fn due(&mut self, pending: &[Pending], board: &Board, now: i64) -> Vec<Job> {
        let runs_here = |k: &str| board.session(k).is_some_and(|(lane, s)| lane.device.is_none() && s.live && !s.archived);
        self.told_stuck.retain(|(from, at)| pending.iter().any(|p| &p.from == from && p.at == *at));
        // One not told yet whose start found its session (or stopped waiting) has nothing to say:
        // its session was waiting on you, say, while the helper started and ran.
        for (from, o) in self.owed.iter_mut() {
            o.notes.retain(|_, n| !matches!(n.news, News::Unstarted { .. }) || pending.iter().any(|p| &p.from == from && n.helper == format!("start@{}", p.at) && p.stuck(now)));
        }
        for p in pending.iter().filter(|p| p.report && p.stuck(now) && runs_here(&p.from)) {
            if !self.told_stuck.insert((p.from.clone(), p.at)) {
                continue;
            }
            let place = board.lanes.iter().find(|l| l.id == p.lane).map(|l| l.callsign.clone()).unwrap_or_else(|| crate::util::tilde(std::path::Path::new(&p.lane)));
            let news = News::Unstarted { account: p.account.clone(), place, terminal: board.apps.terminal.clone(), minutes: p.minutes(now) };
            let key = format!("start@{}", p.at);
            self.owed.entry(p.from.clone()).or_default().notes.insert(key.clone(), Note { helper: key, title: String::new(), news });
        }
        let mut seen_now = HashMap::new();
        for lane in board.lanes.iter().filter(|l| l.device.is_none()) {
            for s in lane.sessions.iter().filter(|s| s.visitor.is_none() && !s.archived) {
                let Some(parent) = s.helper.as_ref().filter(|h| h.report).and_then(|h| h.parent.as_deref()) else { continue };
                let at = stand(s);
                seen_now.insert(s.key.clone(), at);
                let Some(was) = self.seen.get(&s.key).copied() else { continue };
                let news = match (was, at) {
                    (Stand::Working | Stand::OnUser(_), Stand::Still(_)) => Some(News::Ended),
                    (Stand::Still(before), Stand::Still(prompt)) if prompt > before && s.last_activity > prompt => Some(News::Ended),
                    (was, Stand::OnUser(_)) if was != at => Some(News::OnUser(s.excerpt.clone())),
                    _ => None,
                };
                if let Some(news) = news.filter(|_| runs_here(parent)) {
                    let owed = self.owed.entry(parent.to_string()).or_default();
                    owed.notes.insert(s.key.clone(), Note { helper: s.key.clone(), title: s.title.clone(), news });
                }
            }
        }
        self.seen = seen_now;
        // A session that stopped is told nothing more.
        self.owed.retain(|k, o| o.sending.is_some() || runs_here(k));
        let waits = |k: &str| board.automation_blocked(k) || board.session(k).is_some_and(|(_, s)| crate::send::waits_on_you(s));
        let mut out = Vec::new();
        for (k, o) in self.owed.iter_mut() {
            if o.sending.is_some() || o.notes.is_empty() || now < o.next_at || waits(k) {
                continue;
            }
            let notes: Vec<Note> = std::mem::take(&mut o.notes).into_values().collect();
            o.sending = Some(notes.clone());
            out.push(Job { session: k.clone(), notes });
        }
        out.sort_by(|a, b| a.session.cmp(&b.session));
        out
    }

    /// How a job went, at `now` (ms). What was told is owed no longer; what wasn't is owed again,
    /// less what's newer about the same helper, and tried again later. Whether it will be.
    pub fn told(&mut self, job: &Job, ok: bool, now: i64) -> bool {
        let Some(o) = self.owed.get_mut(&job.session) else { return false };
        let sent = o.sending.take().unwrap_or_default();
        let again = !ok && o.failures + 1 < TRIES;
        if again {
            o.failures += 1;
            o.next_at = now + (RETRY_MS << (o.failures - 1).min(10));
            for n in sent {
                o.notes.entry(n.helper.clone()).or_insert(n);
            }
        } else {
            // Told, or given up on: what's come in since starts afresh.
            o.failures = 0;
            o.next_at = 0;
            if o.notes.is_empty() {
                self.owed.remove(&job.session);
            }
        }
        again
    }
}

/// One helper's news, as the message has it: what it's called, then what happened.
fn line(n: &Note) -> String {
    let (title, key) = (n.title.trim(), &n.helper);
    match &n.news {
        News::Ended => format!("“{title}” ({key}) ended its turn: `codealong session wait {key}` prints its reply, and `codealong session done {key}` ends it once you have what you need."),
        News::OnUser(what) => {
            let what = what.as_deref().map(|w| format!(": {}", w.trim())).unwrap_or_default();
            format!("“{title}” ({key}) waits on your user{what}. They answer it in Codealong, so you may want to tell them; `codealong session wait {key} --until-done` waits on to the end of its turn.")
        }
        News::Unstarted { account, place, terminal, minutes } => format!(
            "you started on {account} in {terminal} {minutes} minutes ago, in {place}, hasn't made its session yet: its terminal may be waiting on your user on a screen of its own before it starts, such as an update offer or a question about its background server, which Codealong can't answer. Tell them to answer it in that tab. Once it starts, you're told when its turn ends."
        ),
    }
}

/// The message for a session's helpers' news.
pub fn words(notes: &[Note]) -> String {
    let named = |n: &Note| match n.news {
        News::Unstarted { .. } => format!("The helper {}", line(n)),
        _ => line(n),
    };
    match notes {
        [one] => format!("Your helper {}", line(one)),
        _ => format!("About your helpers:\n{}", notes.iter().map(|n| format!("- {}", named(n))).collect::<Vec<_>>().join("\n")),
    }
}

/// Sends each job's message to its session as a window's send does, and tells `reporter` how each
/// went, and the engine, which says so in a notice when one didn't.
pub fn send_now(engine: &Handle, reporter: &Mutex<Reporter>, jobs: Vec<Job>) {
    let mut failed = Vec::new();
    for job in jobs {
        // One that falls over is told as failed, so its session isn't left waiting on a send
        // that never says how it went.
        let went = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            crate::send::tell(engine, &job.session, &words(&job.notes)).map(|_| ()).map_err(|e| format!("{e:#}"))
        }))
        .unwrap_or_else(|_| Err("telling it stopped partway".to_string()));
        let again = reporter.lock().unwrap_or_else(|e| e.into_inner()).told(&job, went.is_ok(), crate::util::now_ms());
        if let Err(e) = went {
            let title = engine.read(|b| b.session(&job.session).map(|(_, s)| s.title.clone())).unwrap_or_else(|| job.session.clone());
            let then = if again { " Codealong tries again in a minute or two." } else { "" };
            failed.push(format!("“{title}” wasn't told what its helpers did: {e}.{then}"));
        }
    }
    engine.reported(if failed.is_empty() { Ok(()) } else { Err(failed.join(" ")) });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Helper;

    const NOW: i64 = 1_800_000_000_000;
    const BOSS: &str = "claude:claude:boss";
    const REV: &str = "codex:codex:rev";

    fn boss(status: Status) -> Session {
        Session { status, live: true, ..crate::fixtures::session(BOSS) }
    }

    fn rev(status: Status, prompt: i64, activity: i64) -> Session {
        let helper = Some(Helper { parent: Some(BOSS.into()), pr: None, report: true });
        Session { status, live: true, title: "Review the diff".into(), helper, last_prompt_at: prompt, last_activity: activity, status_since: Some(activity), excerpt: Some("Bash · `gh pr diff 12`".into()), ..crate::fixtures::session(REV) }
    }

    fn board(sessions: Vec<Session>) -> Board {
        Board { lanes: vec![crate::fixtures::lane("/r/app", sessions)], ..Board::default() }
    }

    fn news(jobs: &[Job]) -> Vec<(String, News)> {
        jobs.iter().flat_map(|j| j.notes.iter().map(move |n| (j.session.clone(), n.news.clone()))).collect()
    }

    #[test]
    fn a_turn_that_ends_is_told_to_its_session_once() {
        let mut r = Reporter::default();
        // First seen working: only noted.
        assert!(r.due(&[], &board(vec![boss(Status::Idle), rev(Status::Working, 10, 20)]), NOW).is_empty());
        assert!(r.due(&[], &board(vec![boss(Status::Idle), rev(Status::Working, 10, 30)]), NOW).is_empty());
        let jobs = r.due(&[], &board(vec![boss(Status::Idle), rev(Status::Idle, 10, 40)]), NOW);
        assert_eq!(news(&jobs), [(BOSS.to_string(), News::Ended)]);
        assert!(words(&jobs[0].notes).starts_with("Your helper “Review the diff” (codex:codex:rev) ended its turn: `codealong session wait codex:codex:rev` prints its reply"));
        assert!(!r.told(&jobs[0], true, NOW));
        assert!(r.due(&[], &board(vec![boss(Status::Idle), rev(Status::Idle, 10, 40)]), NOW).is_empty(), "once");
        // A turn quicker than a board apart: a newer prompt it has answered.
        let jobs = r.due(&[], &board(vec![boss(Status::Idle), rev(Status::Idle, 50, 60)]), NOW);
        assert_eq!(news(&jobs), [(BOSS.to_string(), News::Ended)]);
        r.told(&jobs[0], true, NOW);
        // A prompt it hasn't answered yet isn't a turn's end.
        assert!(r.due(&[], &board(vec![boss(Status::Idle), rev(Status::Idle, 70, 70)]), NOW).is_empty());
    }

    #[test]
    fn a_wait_on_its_user_is_told_and_a_helper_first_seen_isnt() {
        let mut r = Reporter::default();
        // Ended already when first seen, as after a restart: nothing.
        assert!(r.due(&[], &board(vec![boss(Status::Idle), rev(Status::Idle, 10, 40)]), NOW).is_empty());
        let jobs = r.due(&[], &board(vec![boss(Status::Working), rev(Status::Needs, 50, 60)]), NOW);
        assert_eq!(news(&jobs), [(BOSS.to_string(), News::OnUser(Some("Bash · `gh pr diff 12`".into())))]);
        assert!(words(&jobs[0].notes).contains("waits on your user: Bash · `gh pr diff 12`. They answer it in Codealong"));
        r.told(&jobs[0], true, NOW);
        // Still waiting: not again.
        assert!(r.due(&[], &board(vec![boss(Status::Working), rev(Status::Needs, 50, 60)]), NOW).is_empty());
    }

    #[test]
    fn only_a_reporting_helpers_running_session_is_told_and_not_while_it_waits_on_you() {
        let quiet = |s: Session| Session { helper: s.helper.map(|h| Helper { report: false, ..h }), ..s };
        let mut r = Reporter::default();
        r.due(&[], &board(vec![boss(Status::Idle), quiet(rev(Status::Working, 10, 20))]), NOW);
        assert!(r.due(&[], &board(vec![boss(Status::Idle), quiet(rev(Status::Idle, 10, 40))]), NOW).is_empty(), "started without --report");
        // Its session stopped: nobody to tell.
        let mut r = Reporter::default();
        let stopped = Session { live: false, ..boss(Status::Idle) };
        r.due(&[], &board(vec![stopped.clone(), rev(Status::Working, 10, 20)]), NOW);
        assert!(r.due(&[], &board(vec![stopped, rev(Status::Idle, 10, 40)]), NOW).is_empty());
        // Its session waits on you to answer a prompt: told once it doesn't.
        let mut r = Reporter::default();
        r.due(&[], &board(vec![boss(Status::Needs), rev(Status::Working, 10, 20)]), NOW);
        assert!(r.due(&[], &board(vec![boss(Status::Needs), rev(Status::Idle, 10, 40)]), NOW).is_empty());
        assert_eq!(news(&r.due(&[], &board(vec![boss(Status::Idle), rev(Status::Idle, 10, 40)]), NOW)), [(BOSS.to_string(), News::Ended)]);
    }

    /// A Codex helper started in a terminal sat an hour on a screen of its own before it made its
    /// session, and the session that started it was never told (2026-10-14).
    #[test]
    fn a_reporting_helper_stuck_before_its_session_is_told_of_once() {
        let start = Pending {
            from: BOSS.into(),
            how: crate::model::OriginHow::Started,
            lane: "/r/app".into(),
            account: "codex".into(),
            at: NOW - 4 * 60_000,
            helper: true,
            report: true,
            opens: Some("Review the branch".into()),
            terminal: true,
        };
        let mut b = board(vec![boss(Status::Idle)]);
        b.apps.terminal = "Ghostty".into();
        let mut r = Reporter::default();
        // Not yet stuck a minute after it was started.
        assert!(r.due(&[Pending { at: NOW - 60_000, ..start.clone() }], &b, NOW).is_empty());
        let jobs = r.due(&[start.clone()], &b, NOW);
        let place = b.lanes[0].callsign.clone();
        assert_eq!(news(&jobs), [(BOSS.to_string(), News::Unstarted { account: "codex".into(), place: place.clone(), terminal: "Ghostty".into(), minutes: 4 })]);
        let text = words(&jobs[0].notes);
        assert!(text.starts_with(&format!("Your helper you started on codex in Ghostty 4 minutes ago, in {place}, hasn't made its session yet: its terminal may be waiting on your user")), "{text}");
        r.told(&jobs[0], true, NOW);
        assert!(r.due(&[start.clone()], &b, NOW + 60_000).is_empty(), "once");
        // Started without --report, or by a session that isn't running: nobody to tell.
        let mut r = Reporter::default();
        assert!(r.due(&[Pending { report: false, ..start.clone() }], &b, NOW).is_empty());
        assert!(r.due(&[start.clone()], &board(vec![Session { live: false, ..boss(Status::Idle) }]), NOW).is_empty());
        // Due while its session waited on you to answer a prompt, and the helper started meanwhile:
        // nothing's left to say.
        let mut r = Reporter::default();
        let waiting = board(vec![boss(Status::Needs)]);
        assert!(r.due(&[start.clone()], &waiting, NOW).is_empty());
        assert!(r.due(&[], &b, NOW + 60_000).is_empty());
        // With another helper's news it's a line of the one message.
        let mut r = Reporter::default();
        r.due(&[], &board(vec![boss(Status::Idle), rev(Status::Working, 10, 20)]), NOW);
        let jobs = r.due(&[start], &board(vec![boss(Status::Idle), rev(Status::Idle, 10, 40)]), NOW);
        let text = words(&jobs[0].notes);
        assert!(text.contains("\n- The helper you started on codex in") && text.contains("\n- “Review the diff” (codex:codex:rev) ended its turn"), "{text}");
    }

    #[test]
    fn a_send_that_fails_is_tried_again_later_and_several_go_in_one_message() {
        let mut r = Reporter::default();
        let other = |status, activity| Session { title: "Check the migration".into(), ..Session { key: "codex:codex:two".into(), ..rev(status, 10, activity) } };
        r.due(&[], &board(vec![boss(Status::Idle), rev(Status::Working, 10, 20), other(Status::Working, 20)]), NOW);
        let jobs = r.due(&[], &board(vec![boss(Status::Idle), rev(Status::Idle, 10, 40), other(Status::Error, 40)]), NOW);
        assert_eq!(jobs.len(), 1);
        let text = words(&jobs[0].notes);
        assert!(text.starts_with("About your helpers:\n- “Review the diff” (codex:codex:rev) ended its turn") && text.contains("\n- “Check the migration” (codex:codex:two) waits on your user"), "{text}");
        assert!(r.told(&jobs[0], false, NOW));
        assert!(r.due(&[], &board(vec![boss(Status::Idle), rev(Status::Idle, 10, 40), other(Status::Error, 40)]), NOW + 1).is_empty(), "not yet");
        let again = r.due(&[], &board(vec![boss(Status::Idle), rev(Status::Idle, 10, 40), other(Status::Error, 40)]), NOW + RETRY_MS);
        assert_eq!(again.len(), 1);
        assert_eq!(again[0].notes.len(), 2);
        // Given up on after a few.
        let mut job = again[0].clone();
        let mut at = NOW + RETRY_MS;
        for _ in 0..TRIES {
            if !r.told(&job, false, at) {
                break;
            }
            at += RETRY_MS << 10;
            job = r.due(&[], &board(vec![boss(Status::Idle), rev(Status::Idle, 10, 40), other(Status::Error, 40)]), at).pop().expect("tried again");
        }
        assert!(r.due(&[], &board(vec![boss(Status::Idle), rev(Status::Idle, 10, 40), other(Status::Error, 40)]), at + (RETRY_MS << 10)).is_empty());
    }
}
