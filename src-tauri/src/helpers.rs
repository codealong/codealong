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
//! what's owed to one session goes in one message, gathered for up to `GATHER_MS` while another of
//! its reporting helpers is at work (serve.rs wakes it then, though no board comes); a wait on its
//! user that's over by the time it goes is no news; a send that fails is tried again a little
//! later, a few times.
//!
//! A helper started in a terminal whose session hasn't shown a few minutes on is told of too,
//! once (`origins::Pending::stuck`): its terminal UI likely sits on a screen of its own, and the
//! session that started it, which has no key to wait on, would otherwise wait on it unseen. What
//! that screen shows goes with it where its tab could be read (`Board.starting`, starting.rs).
//! So is one whose start knew its session's key (a background job's, say) and whose session
//! hasn't shown `UNSEEN_MS` on (`News::Unseen`): its start likely failed before it made one, and
//! its session would wait on it for nothing.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Mutex;

use crate::engine::{Handle, Trouble};
use crate::model::{Board, OriginHow, Session, Status};
use crate::origins::{Entry, Pending, Store};

/// How many times a send is tried before it's given up on, and how long after a failed one it's
/// tried again: the first wait, doubled each time.
const TRIES: u32 = 4;
const RETRY_MS: i64 = 30_000;
/// How long a stuck start's report waits for what its terminal shows (`Board.starting`) from when
/// this reporter first finds it stuck: a server that starts with it stuck already reads its
/// terminal only afterwards, a few reads' time (starting.rs `READ_EVERY_MS`). A terminal that
/// can't be read is told of without.
const SCREEN_WAIT_MS: i64 = 60_000;
/// How long news owed to a session waits while another of its reporting helpers is still at work,
/// from the first of it: a session coordinating several (skills/codealong/references/coordinate.md)
/// hears of the ones that end close together in one message, rather than a turn for each.
const GATHER_MS: i64 = 60_000;
/// How long a reporting helper whose start knew its session's key (`origins::Entry::known`) may
/// go without that session on the board before its session is told: a start in the background
/// that failed before it made one (a worktree its repo's hook couldn't make, a harness that
/// quit) leaves nothing else to say so. Long enough for a slow worktree hook's few minutes.
const UNSEEN_MS: i64 = 10 * 60_000;
/// How long after `UNSEEN_MS` such a start is still looked for: a restart finds older ones in
/// origins.json, whose sessions may be gone from the board for other reasons by now.
const UNSEEN_LOOK_MS: i64 = 60 * 60_000;

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
        _ if s.waits_on_its_user() => Stand::OnUser(s.status_since.unwrap_or(s.last_activity)),
        _ => Stand::Still(s.last_prompt_at),
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
    /// (`origins::Pending::stuck`): its terminal likely waits on its user, on what it shows
    /// (`screen`) where that was read.
    Unstarted { account: String, place: String, terminal: String, minutes: i64, screen: Option<String> },
    /// Started with its session's key known some minutes ago, that session hasn't shown up
    /// (`UNSEEN_MS`): its start likely failed.
    Unseen { account: String, minutes: i64 },
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
    /// When the first of `notes` came (ms), for gathering (`GATHER_MS`).
    since: i64,
}

impl Owed {
    /// Owes a helper's latest news, noting when the first came.
    fn owe(&mut self, note: Note, now: i64) {
        if self.notes.is_empty() {
            self.since = now;
        }
        self.notes.insert(note.helper.clone(), note);
    }
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
    /// When each stuck start was first found stuck here, by the session that asked and when it
    /// did, for the wait for its screen.
    stuck_seen: HashMap<(String, i64), i64>,
    /// The helpers whose sessions hadn't shown told of (`News::Unseen`), by key.
    told_unseen: HashSet<String>,
    /// When to look again though no board comes, and the time a wake was asked for (`wake`), so
    /// one is asked for each: the news gathered on the last board is due (`GATHER_MS`), or a
    /// helper's session that hasn't shown has had its time (`UNSEEN_MS`).
    look_again: Option<i64>,
    woken_for: Option<i64>,
    /// The latest board looked at (`Board::rev`): a wake's board older than it would undo what
    /// that one found.
    rev: u64,
}

impl Reporter {
    /// The messages due now, at `now` (ms). A reporting helper's turn ends when the last board
    /// had it working or waiting on its user and this one has it still, or when it's still on
    /// both with a newer prompt it has answered since (a turn quicker than a board apart); a
    /// wait on its user begins when this board has it waiting and the last didn't, or not since
    /// then. A reporting helper started in a terminal that's stuck before it made its session
    /// (`pending`, `Pending::stuck`) is told of once, with what its terminal shows once that's
    /// read, or once `SCREEN_WAIT_MS` has gone without it. One started with its session's key
    /// known whose session still isn't on the board `UNSEEN_MS` on is told of once too. Each is
    /// owed to the helper's session while that runs on this Mac. `starts` is origins.json.
    pub fn due(&mut self, starts: &Store, board: &Board, now: i64) -> Vec<Job> {
        let pending = &starts.pending[..];
        self.rev = self.rev.max(board.rev);
        let runs_here = |k: &str| board.session(k).is_some_and(|(lane, s)| lane.device.is_none() && s.live && !s.archived);
        let screen_of = |p: &Pending| board.starting.get(&p.from).and_then(|starts| starts.iter().find(|s| s.at == p.at)).and_then(|s| s.screen.clone());
        self.told_stuck.retain(|(from, at)| pending.iter().any(|p| &p.from == from && p.at == *at));
        self.stuck_seen.retain(|(from, at), _| pending.iter().any(|p| &p.from == from && p.at == *at));
        // One not told yet whose start found its session (or stopped waiting) has nothing to say:
        // its session was waiting on you, say, while the helper started and ran. One still stuck
        // says what its terminal shows now.
        for (from, o) in self.owed.iter_mut() {
            o.notes.retain(|_, n| !matches!(n.news, News::Unstarted { .. }) || pending.iter().any(|p| &p.from == from && n.helper == format!("start@{}", p.at) && p.stuck(now)));
            for n in o.notes.values_mut() {
                let Some(p) = pending.iter().find(|p| &p.from == from && n.helper == format!("start@{}", p.at)) else { continue };
                if let (News::Unstarted { screen, .. }, Some(read)) = (&mut n.news, screen_of(p)) {
                    *screen = Some(read);
                }
            }
        }
        for p in pending.iter().filter(|p| p.report && p.stuck(now) && runs_here(&p.from)) {
            let screen = screen_of(p);
            // Without its screen yet, it waits a little for one from when it was first found stuck.
            let seen = *self.stuck_seen.entry((p.from.clone(), p.at)).or_insert(now);
            if (screen.is_none() && now - seen < SCREEN_WAIT_MS) || !self.told_stuck.insert((p.from.clone(), p.at)) {
                continue;
            }
            let place = crate::origins::place_of(board, &p.lane);
            let news = News::Unstarted { account: p.account.clone(), place, terminal: board.apps.terminal.clone(), minutes: p.minutes(now), screen };
            self.owed.entry(p.from.clone()).or_default().owe(Note { helper: format!("start@{}", p.at), title: String::new(), news }, now);
        }
        // A start that knew its session's key, whose session hasn't shown up: one this Mac's
        // board would have by now, the helper of a session that runs here.
        let mut unseen_due = None;
        let looked_for = |e: &Entry| e.helper && e.report && e.known && e.how == OriginHow::Started && now - e.at <= UNSEEN_MS + UNSEEN_LOOK_MS;
        self.told_unseen.retain(|k| starts.sessions.get(k).is_some_and(|e| looked_for(e)));
        for (key, e) in starts.sessions.iter().filter(|(k, e)| looked_for(e) && board.session(k).is_none() && runs_here(&e.from)) {
            if now - e.at < UNSEEN_MS {
                unseen_due = Some(unseen_due.map_or(e.at + UNSEEN_MS, |d: i64| d.min(e.at + UNSEEN_MS)));
                continue;
            }
            if !self.told_unseen.insert(key.clone()) {
                continue;
            }
            let account = key.split(':').nth(1).unwrap_or_default().to_string();
            let news = News::Unseen { account, minutes: (now - e.at) / 60_000 };
            self.owed.entry(e.from.clone()).or_default().owe(Note { helper: key.clone(), title: String::new(), news }, now);
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
                    self.owed.entry(parent.to_string()).or_default().owe(Note { helper: s.key.clone(), title: s.title.clone(), news }, now);
                }
            }
        }
        self.seen = seen_now;
        // A session that stopped is told nothing more.
        self.owed.retain(|k, o| o.sending.is_some() || runs_here(k));
        let waits = |k: &str| board.automation_blocked(k) || board.session(k).is_some_and(|(_, s)| crate::send::waits_on_you(s));
        let at_work = |k: &str| {
            board.lanes.iter().filter(|l| l.device.is_none()).flat_map(|l| &l.sessions).any(|s| !s.archived && s.status == Status::Working && s.helper.as_ref().is_some_and(|h| h.report && h.parent.as_deref() == Some(k)))
        };
        let seen = &self.seen;
        let mut gathering = None;
        let mut out = Vec::new();
        for (k, o) in self.owed.iter_mut() {
            // A wait on its user answered while its news was held (gathered, or its session
            // waiting on you) is no news: it would have its session tell your user of a question
            // they've answered.
            o.notes.retain(|_, n| !matches!(n.news, News::OnUser(_)) || matches!(seen.get(&n.helper), Some(Stand::OnUser(_))));
            if o.sending.is_some() || o.notes.is_empty() || now < o.next_at || waits(k) {
                continue;
            }
            if now - o.since < GATHER_MS && at_work(k) {
                gathering = Some(gathering.map_or(o.since + GATHER_MS, |g: i64| g.min(o.since + GATHER_MS)));
                continue;
            }
            let notes: Vec<Note> = std::mem::take(&mut o.notes).into_values().collect();
            o.sending = Some(notes.clone());
            out.push(Job { session: k.clone(), notes });
        }
        self.look_again = [gathering, unseen_due].into_iter().flatten().min();
        out.sort_by(|a, b| a.session.cmp(&b.session));
        out
    }

    /// When to look again for news gathered on the last board (`due`), or for a helper's session
    /// that hasn't shown, though no board comes by then: a helper's silent command can keep the
    /// board as it is for long after. Once for each time: none while one asked for comes no
    /// later.
    pub fn wake(&mut self) -> Option<i64> {
        let at = self.look_again?;
        if self.woken_for.is_some_and(|w| w <= at) {
            return None;
        }
        self.woken_for = Some(at);
        Some(at)
    }

    /// The wake asked for at `at` came, with the board as it is then: what's due on it, unless a
    /// later board was looked at meanwhile, which had it due then, and when to wake next.
    pub fn woke(&mut self, at: i64, starts: &Store, board: &Board, now: i64) -> (Vec<Job>, Option<i64>) {
        if self.woken_for == Some(at) {
            self.woken_for = None;
        }
        let due = if board.rev >= self.rev { self.due(starts, board, now) } else { Vec::new() };
        (due, self.wake())
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
        News::Unstarted { account, place, terminal, minutes, screen: None } => format!(
            "you started on {account} in {terminal} {minutes} minutes ago, in {place}, hasn't made its session yet: its terminal may be waiting on your user on a screen of its own before it starts, such as an update offer or a question about its background server. Your session shows in Codealong as waiting on them for it, and its line under your session's title shows its tab, or stops it if you no longer need it: tell them. Once it starts, you're told when its turn ends."
        ),
        // Set in, so it stays within a list's item when there's other news.
        News::Unstarted { account, place, terminal, minutes, screen: Some(screen) } => format!(
            "you started on {account} in {terminal} {minutes} minutes ago, in {place}, hasn't made its session yet: its terminal waits on your user to answer what it shows before it starts. It shows:\n\n{}\n\nYour session shows in Codealong as waiting on them for it, and its line under your session's title answers it, shows its tab, or stops it if you no longer need it: tell them what it asks. Once it starts, you're told when its turn ends.",
            screen.lines().map(|l| if l.is_empty() { String::new() } else { format!("    {l}") }).collect::<Vec<_>>().join("\n")
        ),
        News::Unseen { account, minutes } => format!(
            "{key}, which you started on {account} {minutes} minutes ago, hasn't shown up in Codealong: its start likely failed before it made its session (a worktree that couldn't be made, say), or its terminal waits on a screen of its own. `codealong session show {key}` finds nothing of it. Start the part again, or tell your user if it fails again."
        ),
    }
}

/// The message for a session's helpers' news.
pub fn words(notes: &[Note]) -> String {
    let named = |n: &Note| match n.news {
        News::Unstarted { .. } | News::Unseen { .. } => format!("The helper {}", line(n)),
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
    engine.went(Trouble::Helpers, if failed.is_empty() { Ok(()) } else { Err(failed.join(" ")) });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Helper;

    /// origins.json with `pending` its starts waiting for their sessions, and no session noted.
    fn starts(pending: &[Pending]) -> Store {
        Store { pending: pending.to_vec(), ..Store::default() }
    }

    const NOW: i64 = 1_800_000_000_000;
    const BOSS: &str = "claude:claude:boss";
    const REV: &str = "codex:codex:rev";

    fn boss(status: Status) -> Session {
        Session { status, live: true, ..crate::fixtures::session(BOSS) }
    }

    fn rev(status: Status, prompt: i64, activity: i64) -> Session {
        let helper = Some(Helper { parent: Some(BOSS.into()), pr: None, report: true, yours: false });
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
        assert!(r.due(&starts(&[]), &board(vec![boss(Status::Idle), rev(Status::Working, 10, 20)]), NOW).is_empty());
        assert!(r.due(&starts(&[]), &board(vec![boss(Status::Idle), rev(Status::Working, 10, 30)]), NOW).is_empty());
        let jobs = r.due(&starts(&[]), &board(vec![boss(Status::Idle), rev(Status::Idle, 10, 40)]), NOW);
        assert_eq!(news(&jobs), [(BOSS.to_string(), News::Ended)]);
        assert!(words(&jobs[0].notes).starts_with("Your helper “Review the diff” (codex:codex:rev) ended its turn: `codealong session wait codex:codex:rev` prints its reply"));
        assert!(!r.told(&jobs[0], true, NOW));
        assert!(r.due(&starts(&[]), &board(vec![boss(Status::Idle), rev(Status::Idle, 10, 40)]), NOW).is_empty(), "once");
        // A turn quicker than a board apart: a newer prompt it has answered.
        let jobs = r.due(&starts(&[]), &board(vec![boss(Status::Idle), rev(Status::Idle, 50, 60)]), NOW);
        assert_eq!(news(&jobs), [(BOSS.to_string(), News::Ended)]);
        r.told(&jobs[0], true, NOW);
        // A prompt it hasn't answered yet isn't a turn's end.
        assert!(r.due(&starts(&[]), &board(vec![boss(Status::Idle), rev(Status::Idle, 70, 70)]), NOW).is_empty());
    }

    /// A turn its account's sign-in stopped didn't end: its session hears it waits on its user,
    /// and that its turn ended only once it's carried on and done.
    #[test]
    fn a_helper_stopped_on_its_sign_in_waits_on_its_user() {
        let mut r = Reporter::default();
        let out = |activity| Session { signed_out: Some(crate::model::SignedOut { at: activity, said: "OAuth token revoked · Please run /login".into(), continues: true }), ..rev(Status::Idle, 10, activity) };
        assert!(r.due(&[], &board(vec![boss(Status::Idle), rev(Status::Working, 10, 20)]), NOW).is_empty());
        let jobs = r.due(&[], &board(vec![boss(Status::Idle), out(30)]), NOW);
        assert!(matches!(news(&jobs)[..], [(_, News::OnUser(_))]), "{:?}", news(&jobs));
        r.told(&jobs[0], true, NOW);
        assert!(r.due(&[], &board(vec![boss(Status::Idle), rev(Status::Working, 10, 40)]), NOW).is_empty());
        let jobs = r.due(&[], &board(vec![boss(Status::Idle), rev(Status::Idle, 10, 50)]), NOW);
        assert_eq!(news(&jobs), [(BOSS.to_string(), News::Ended)]);
    }

    #[test]
    fn a_turn_that_ends_while_another_helper_works_waits_a_little_to_go_with_its_news() {
        let mut r = Reporter::default();
        let other = |status: Status| Session { key: "codex:codex:other".into(), title: "Fix the docs".into(), ..rev(status, 10, 20) };
        r.due(&starts(&[]), &board(vec![boss(Status::Idle), rev(Status::Working, 10, 20), other(Status::Working)]), NOW);
        // One ends while the other works: it waits.
        assert!(r.due(&starts(&[]), &board(vec![boss(Status::Idle), rev(Status::Idle, 10, 40), other(Status::Working)]), NOW).is_empty());
        // The other ends within the minute: both go in one message.
        let jobs = r.due(&starts(&[]), &board(vec![boss(Status::Idle), rev(Status::Idle, 10, 40), other(Status::Idle)]), NOW + 20_000);
        assert_eq!(news(&jobs), [(BOSS.to_string(), News::Ended), (BOSS.to_string(), News::Ended)]);
        r.told(&jobs[0], true, NOW + 20_000);
        // One that ends while the other works on past the minute goes then, alone.
        let mut r = Reporter::default();
        r.due(&starts(&[]), &board(vec![boss(Status::Idle), rev(Status::Working, 10, 20), other(Status::Working)]), NOW);
        assert!(r.due(&starts(&[]), &board(vec![boss(Status::Idle), rev(Status::Idle, 10, 40), other(Status::Working)]), NOW).is_empty());
        assert!(r.due(&starts(&[]), &board(vec![boss(Status::Idle), rev(Status::Idle, 10, 40), other(Status::Working)]), NOW + GATHER_MS - 1).is_empty());
        assert_eq!(news(&r.due(&starts(&[]), &board(vec![boss(Status::Idle), rev(Status::Idle, 10, 40), other(Status::Working)]), NOW + GATHER_MS)).len(), 1);
    }

    #[test]
    fn what_it_gathers_asks_to_be_looked_at_again_once_at_its_time() {
        let mut r = Reporter::default();
        let other = |status: Status| Session { key: "codex:codex:other".into(), ..rev(status, 10, 20) };
        r.due(&starts(&[]), &board(vec![boss(Status::Idle), rev(Status::Working, 10, 20), other(Status::Working)]), NOW);
        assert_eq!(r.wake(), None, "nothing gathered");
        let held = board(vec![boss(Status::Idle), rev(Status::Idle, 10, 40), other(Status::Working)]);
        assert!(r.due(&starts(&[]), &held, NOW).is_empty());
        assert_eq!(r.wake(), Some(NOW + GATHER_MS));
        // Asked for once, however many boards come meanwhile.
        assert!(r.due(&starts(&[]), &held, NOW + 5_000).is_empty());
        assert_eq!(r.wake(), None);
        // When it comes, the same board, looked at again, has it due.
        let (jobs, next) = r.woke(NOW + GATHER_MS, &starts(&[]), &held, NOW + GATHER_MS);
        assert_eq!((news(&jobs).len(), next), (1, None));
    }

    #[test]
    fn a_wake_asks_for_the_next_and_never_looks_at_an_older_board() {
        let mut r = Reporter::default();
        let other = |status: Status| Session { key: "codex:codex:other".into(), ..rev(status, 10, 20) };
        let third = |status: Status| Session { key: "codex:codex:third".into(), ..rev(status, 10, 20) };
        let at = |rev_: u64, a: Status, b: Status, c: Status| Board { rev: rev_, ..board(vec![boss(Status::Idle), rev(a, 10, 40), other(b), third(c)]) };
        use Status::{Idle, Needs, Working};
        r.due(&starts(&[]), &at(1, Working, Working, Working), NOW);
        // It waits on your user while the others work: a wake for a minute on.
        assert!(r.due(&starts(&[]), &at(2, Needs, Working, Working), NOW).is_empty());
        assert_eq!(r.wake(), Some(NOW + GATHER_MS));
        // Answered; then the third ends 20 seconds on, while the other works: its time is later
        // than the wake asked for, which comes first.
        assert!(r.due(&starts(&[]), &at(3, Working, Working, Working), NOW + 10_000).is_empty());
        assert!(r.due(&starts(&[]), &at(4, Working, Working, Idle), NOW + 20_000).is_empty());
        assert_eq!(r.wake(), None);
        // The wake comes with a board older than the last looked at: it looks at nothing, and
        // asks for the next, the third's time.
        let (jobs, next) = r.woke(NOW + GATHER_MS, &starts(&[]), &at(3, Working, Working, Working), NOW + GATHER_MS);
        assert!(jobs.is_empty());
        assert_eq!(next, Some(NOW + 20_000 + GATHER_MS));
        // Then the third's news goes, and only it: the wait on your user was answered.
        let (jobs, next) = r.woke(NOW + 20_000 + GATHER_MS, &starts(&[]), &at(4, Working, Working, Idle), NOW + 20_000 + GATHER_MS);
        assert_eq!((news(&jobs), next), (vec![(BOSS.to_string(), News::Ended)], None));
    }

    #[test]
    fn a_wait_on_its_user_answered_while_its_news_was_held_isnt_told() {
        let mut r = Reporter::default();
        let other = |status: Status| Session { key: "codex:codex:other".into(), ..rev(status, 10, 20) };
        r.due(&starts(&[]), &board(vec![boss(Status::Idle), rev(Status::Working, 10, 20), other(Status::Working)]), NOW);
        // It waits on your user while the other works: held.
        assert!(r.due(&starts(&[]), &board(vec![boss(Status::Idle), rev(Status::Needs, 10, 30), other(Status::Working)]), NOW).is_empty());
        // Answered, it works again: nothing to tell, then or once the other ends.
        assert!(r.due(&starts(&[]), &board(vec![boss(Status::Idle), rev(Status::Working, 10, 35), other(Status::Working)]), NOW + 10_000).is_empty());
        assert!(r.due(&starts(&[]), &board(vec![boss(Status::Idle), rev(Status::Working, 10, 35), other(Status::Working)]), NOW + GATHER_MS).is_empty());
    }

    #[test]
    fn a_wait_on_its_user_is_told_and_a_helper_first_seen_isnt() {
        let mut r = Reporter::default();
        // Ended already when first seen, as after a restart: nothing.
        assert!(r.due(&starts(&[]), &board(vec![boss(Status::Idle), rev(Status::Idle, 10, 40)]), NOW).is_empty());
        let jobs = r.due(&starts(&[]), &board(vec![boss(Status::Working), rev(Status::Needs, 50, 60)]), NOW);
        assert_eq!(news(&jobs), [(BOSS.to_string(), News::OnUser(Some("Bash · `gh pr diff 12`".into())))]);
        assert!(words(&jobs[0].notes).contains("waits on your user: Bash · `gh pr diff 12`. They answer it in Codealong"));
        r.told(&jobs[0], true, NOW);
        // Still waiting: not again.
        assert!(r.due(&starts(&[]), &board(vec![boss(Status::Working), rev(Status::Needs, 50, 60)]), NOW).is_empty());
    }

    #[test]
    fn only_a_reporting_helpers_running_session_is_told_and_not_while_it_waits_on_you() {
        let quiet = |s: Session| Session { helper: s.helper.map(|h| Helper { report: false, ..h }), ..s };
        let mut r = Reporter::default();
        r.due(&starts(&[]), &board(vec![boss(Status::Idle), quiet(rev(Status::Working, 10, 20))]), NOW);
        assert!(r.due(&starts(&[]), &board(vec![boss(Status::Idle), quiet(rev(Status::Idle, 10, 40))]), NOW).is_empty(), "started without --report");
        // Its session stopped: nobody to tell.
        let mut r = Reporter::default();
        let stopped = Session { live: false, ..boss(Status::Idle) };
        r.due(&starts(&[]), &board(vec![stopped.clone(), rev(Status::Working, 10, 20)]), NOW);
        assert!(r.due(&starts(&[]), &board(vec![stopped, rev(Status::Idle, 10, 40)]), NOW).is_empty());
        // Its session waits on you to answer a prompt: told once it doesn't.
        let mut r = Reporter::default();
        r.due(&starts(&[]), &board(vec![boss(Status::Needs), rev(Status::Working, 10, 20)]), NOW);
        assert!(r.due(&starts(&[]), &board(vec![boss(Status::Needs), rev(Status::Idle, 10, 40)]), NOW).is_empty());
        assert_eq!(news(&r.due(&starts(&[]), &board(vec![boss(Status::Idle), rev(Status::Idle, 10, 40)]), NOW)), [(BOSS.to_string(), News::Ended)]);
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
        // Not yet stuck a minute after it was started; found stuck, it waits a while for its screen,
        // and is told of without one when none comes.
        assert!(r.due(&starts(&[Pending { at: NOW - 60_000, ..start.clone() }]), &b, NOW).is_empty());
        assert!(r.due(&starts(&[start.clone()]), &b, NOW - SCREEN_WAIT_MS).is_empty());
        let jobs = r.due(&starts(&[start.clone()]), &b, NOW);
        let place = b.lanes[0].callsign.clone();
        assert_eq!(news(&jobs), [(BOSS.to_string(), News::Unstarted { account: "codex".into(), place: place.clone(), terminal: "Ghostty".into(), minutes: 4, screen: None })]);
        let text = words(&jobs[0].notes);
        assert!(text.starts_with(&format!("Your helper you started on codex in Ghostty 4 minutes ago, in {place}, hasn't made its session yet: its terminal may be waiting on your user")), "{text}");
        r.told(&jobs[0], true, NOW);
        assert!(r.due(&starts(&[start.clone()]), &b, NOW + 60_000).is_empty(), "once");
        // Where its tab was read (`Board.starting`), what it shows goes with it, set in
        // (2026-11-15: Codex's review of a repo's hooks).
        let mut read = b.clone();
        let screen = "Hooks need review\n\n› 1. Review hooks".to_string();
        read.starting.insert(BOSS.into(), vec![crate::starting::Starting { at: start.at, screen: Some(screen.clone()), ..Default::default() }]);
        let jobs = Reporter::default().due(&starts(&[start.clone()]), &read, NOW);
        assert_eq!(news(&jobs), [(BOSS.to_string(), News::Unstarted { account: "codex".into(), place: place.clone(), terminal: "Ghostty".into(), minutes: 4, screen: Some(screen.clone()) })]);
        let text = words(&jobs[0].notes);
        assert!(text.contains("hasn't made its session yet: its terminal waits on your user to answer what it shows before it starts. It shows:\n\n    Hooks need review\n\n    › 1. Review hooks\n\nYour session shows in Codealong as waiting on them for it, and its line under your session's title answers it"), "{text}");
        // Found stuck long after it was started (a server started meanwhile, which has read no
        // screen yet), it waits from then: told as soon as its screen is read, else a minute on
        // without it.
        let old = Pending { at: NOW - 10 * 60_000, ..start.clone() };
        let mut read_old = b.clone();
        read_old.starting.insert(BOSS.into(), vec![crate::starting::Starting { at: old.at, screen: Some(screen.clone()), ..Default::default() }]);
        let mut r = Reporter::default();
        assert!(r.due(&starts(&[old.clone()]), &b, NOW).is_empty());
        assert!(matches!(&news(&r.due(&starts(&[old.clone()]), &read_old, NOW + 15_000))[..], [(_, News::Unstarted { screen: Some(_), .. })]));
        let mut r = Reporter::default();
        assert!(r.due(&starts(&[old.clone()]), &b, NOW).is_empty());
        assert!(matches!(&news(&r.due(&starts(&[old.clone()]), &b, NOW + SCREEN_WAIT_MS))[..], [(_, News::Unstarted { screen: None, .. })]));
        // Held while its session waited on you, it says what was read meanwhile.
        let mut r = Reporter::default();
        let needs = board(vec![boss(Status::Needs)]);
        assert!(r.due(&starts(&[start.clone()]), &needs, NOW - SCREEN_WAIT_MS).is_empty());
        assert!(r.due(&starts(&[start.clone()]), &needs, NOW).is_empty());
        assert!(matches!(&news(&r.due(&starts(&[start.clone()]), &read, NOW + 30_000))[..], [(_, News::Unstarted { screen: Some(_), .. })]));
        // Started without --report, or by a session that isn't running: nobody to tell.
        let mut r = Reporter::default();
        assert!(r.due(&starts(&[Pending { report: false, ..start.clone() }]), &b, NOW).is_empty());
        assert!(r.due(&starts(&[start.clone()]), &board(vec![Session { live: false, ..boss(Status::Idle) }]), NOW).is_empty());
        // Due while its session waited on you to answer a prompt, and the helper started meanwhile:
        // nothing's left to say.
        let mut r = Reporter::default();
        let waiting = board(vec![boss(Status::Needs)]);
        assert!(r.due(&starts(&[start.clone()]), &waiting, NOW).is_empty());
        assert!(r.due(&starts(&[]), &b, NOW + 60_000).is_empty());
        // With another helper's news it's a line of the one message.
        let mut r = Reporter::default();
        r.due(&starts(&[start.clone()]), &board(vec![boss(Status::Idle), rev(Status::Working, 10, 20)]), NOW - SCREEN_WAIT_MS);
        let jobs = r.due(&starts(&[start]), &board(vec![boss(Status::Idle), rev(Status::Idle, 10, 40)]), NOW);
        let text = words(&jobs[0].notes);
        assert!(text.contains("\n- The helper you started on codex in") && text.contains("\n- “Review the diff” (codex:codex:rev) ended its turn"), "{text}");
    }

    #[test]
    fn a_send_that_fails_is_tried_again_later_and_several_go_in_one_message() {
        let mut r = Reporter::default();
        let other = |status, activity| Session { title: "Check the migration".into(), ..Session { key: "codex:codex:two".into(), ..rev(status, 10, activity) } };
        r.due(&starts(&[]), &board(vec![boss(Status::Idle), rev(Status::Working, 10, 20), other(Status::Working, 20)]), NOW);
        let jobs = r.due(&starts(&[]), &board(vec![boss(Status::Idle), rev(Status::Idle, 10, 40), other(Status::Error, 40)]), NOW);
        assert_eq!(jobs.len(), 1);
        let text = words(&jobs[0].notes);
        assert!(text.starts_with("About your helpers:\n- “Review the diff” (codex:codex:rev) ended its turn") && text.contains("\n- “Check the migration” (codex:codex:two) waits on your user"), "{text}");
        assert!(r.told(&jobs[0], false, NOW));
        assert!(r.due(&starts(&[]), &board(vec![boss(Status::Idle), rev(Status::Idle, 10, 40), other(Status::Error, 40)]), NOW + 1).is_empty(), "not yet");
        let again = r.due(&starts(&[]), &board(vec![boss(Status::Idle), rev(Status::Idle, 10, 40), other(Status::Error, 40)]), NOW + RETRY_MS);
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
            job = r.due(&starts(&[]), &board(vec![boss(Status::Idle), rev(Status::Idle, 10, 40), other(Status::Error, 40)]), at).pop().expect("tried again");
        }
        assert!(r.due(&starts(&[]), &board(vec![boss(Status::Idle), rev(Status::Idle, 10, 40), other(Status::Error, 40)]), at + (RETRY_MS << 10)).is_empty());
    }

    /// A helper started with its session's key known (in the background, say) whose session never
    /// shows: its session is told once its time is up, woken for it though no board comes, and
    /// only for a reporting helper of a session that runs here, started lately.
    #[test]
    fn a_helper_whose_session_never_shows_is_told_of_once_its_time_is_up() {
        let entry = |at: i64, report: bool| Entry { from: BOSS.into(), how: OriginHow::Started, at, helper: true, report, known: true };
        let mut store = Store::default();
        store.sessions.insert("claude:claude2:gone".into(), entry(NOW, true));
        let b = board(vec![boss(Status::Idle)]);
        let mut r = Reporter::default();
        assert!(r.due(&store, &b, NOW + 60_000).is_empty());
        assert_eq!(r.wake(), Some(NOW + UNSEEN_MS));
        let jobs = r.due(&store, &b, NOW + UNSEEN_MS);
        assert_eq!(news(&jobs), [(BOSS.to_string(), News::Unseen { account: "claude2".into(), minutes: 10 })]);
        assert!(words(&jobs[0].notes).starts_with("Your helper claude:claude2:gone, which you started on claude2 10 minutes ago, hasn't shown up"), "{}", words(&jobs[0].notes));
        assert!(!r.told(&jobs[0], true, NOW + UNSEEN_MS));
        assert!(r.due(&store, &b, NOW + UNSEEN_MS + 60_000).is_empty(), "once");

        // Its session shown, a helper that doesn't report, one whose session its harness picked
        // (only likely), and one started long before: nothing.
        let mut others = Store::default();
        others.sessions.insert(REV.into(), entry(NOW, true));
        others.sessions.insert("claude:claude:quiet".into(), entry(NOW, false));
        others.sessions.insert("claude:claude:guessed".into(), Entry { known: false, ..entry(NOW, true) });
        others.sessions.insert("claude:claude:old".into(), entry(NOW - UNSEEN_LOOK_MS - 60_000, true));
        let shown = board(vec![boss(Status::Idle), rev(Status::Working, 10, 20)]);
        assert!(Reporter::default().due(&others, &shown, NOW + UNSEEN_MS).is_empty());
        // Nor while the session that started it isn't running.
        let stopped = board(vec![Session { live: false, ..boss(Status::Idle) }]);
        assert!(Reporter::default().due(&store, &stopped, NOW + UNSEEN_MS).is_empty());
    }
}
