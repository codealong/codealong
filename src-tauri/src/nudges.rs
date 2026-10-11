//! Telling a PR's own session what happens on it, for the PRs it's turned on for: by you, from
//! the PR's card or menu, or by an agent with `codealong pr nudge` (both write pr-links.json,
//! prlinks.rs), or for every PR of yours by `[pull_requests] nudge_by_default`, unless turned off
//! for that one; a PR says so while it's open (`PrItem::nudge`). Every board the engine publishes
//! is looked at for such PRs with something new on GitHub: approved, changes asked for, reviewed
//! or commented on by someone else (read from the PR's detail once its `updatedAt` moves), merged
//! or closed, or with checks that just failed. The PR's primary session (`PrItem::primary`: the
//! one picked, else the one that opened it), when it runs here, is sent one message saying what
//! happened, as a message of yours from a window is (actions.rs `session-send`: typed into its
//! terminal, or its harness's own way in), on a thread of its own, since reading GitHub and a send
//! take a moment. The PR's other sessions (helpers, reviewers, ones that came to its branch) are
//! told nothing. What you did to the PR yourself, which is what its agent did through `gh` too, is
//! never told back. `[pull_requests] nudge_about` can keep it to what asks for work (`keep`):
//! comments and reviews that ask for changes or say something, and failing checks if it says so,
//! never an approval, a merge or a close.
//!
//! What's seen on the board and what's been told are kept apart: an update is owed to each
//! session from the board it shows on until a send to that session goes through. A session
//! waiting on you to answer a prompt isn't sent anything (what's typed would answer it), and is
//! told once it isn't; a send that fails, or a PR whose news can't be read from GitHub, is tried
//! again a little later, each time later than the last, a few times before it's given up on.

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use crate::config::NudgeAbout;
use crate::engine::Handle;
use crate::github::PrDetail;
use crate::model::{Board, Merge, PrItem, PrState};

/// What a PR on the board stood at when last seen, for telling only what's new.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Seen {
    on: bool,
    updated_at: i64,
    state: PrState,
    failed: bool,
}

/// What's new on a PR since the last board, or since a session was last told, to tell it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Why {
    /// Its `updatedAt` moved: read what others did on it since this time (ms) from its detail.
    pub since: Option<i64>,
    /// It's approved now and wasn't told so: it became approved, or the nudge was turned on for
    /// a PR approved already.
    pub approved: bool,
    /// Changes were asked for, by its review decision.
    pub changes: bool,
    /// It was merged or closed.
    pub ended: Option<PrState>,
    /// Checks that just failed, by name.
    pub failed: Vec<String>,
}

impl Why {
    /// Adds what's new since to what's still to tell: news from the earliest time, and every
    /// change of state.
    fn add(&mut self, more: Why) {
        self.since = match (self.since, more.since) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        self.approved |= more.approved;
        self.changes |= more.changes;
        self.ended = more.ended.or(self.ended);
        for check in more.failed {
            if !self.failed.contains(&check) {
                self.failed.push(check);
            }
        }
    }
}

/// What to tell one session about one PR.
#[derive(Debug, Clone, PartialEq)]
pub struct Job {
    pub pr: PrItem,
    pub why: Why,
    pub session: String,
    /// The last try: when the PR's news can't be read from GitHub, what the board knows is told
    /// rather than nothing.
    pub last: bool,
    /// What it's told of (`[pull_requests] nudge_about`), as the board said when it was due.
    pub about: NudgeAbout,
}

/// What of `why` a nudge tells of, as `[pull_requests] nudge_about` says: everything; or only
/// what asks for work, the reviews and comments its detail has (`since`, which `words` sorts) and
/// changes asked for, with failing checks for `feedback-and-checks`. Never then an approval, a
/// merge or a close, nor anything about a PR that's no longer `open`.
fn keep(about: NudgeAbout, open: bool, why: Why) -> Why {
    match about {
        NudgeAbout::Everything => why,
        _ if !open => Why::default(),
        _ => Why { approved: false, ended: None, failed: if about == NudgeAbout::FeedbackAndChecks { why.failed } else { Vec::new() }, ..why },
    }
}

/// How many times a send is tried before it's given up on.
const TRIES: u32 = 6;

/// How long after a failed send it's tried again: the first wait, doubled each time, up to the
/// longest. Six tries span about a quarter of an hour.
const RETRY_MS: i64 = 30_000;
const RETRY_MAX_MS: i64 = 10 * 60_000;

/// What a session is still to be told about a PR.
#[derive(Debug)]
struct Owed {
    /// The PR as the board last listed it.
    pr: PrItem,
    /// What hasn't gone to a send yet.
    why: Why,
    /// What's gone to a send that hasn't said how it went.
    sending: Option<Why>,
    /// Sends that failed in a row.
    failures: u32,
    /// Not tried again before this (ms).
    next_at: i64,
}

/// What each PR on the board stood at when last seen, so each update is noticed once, as it
/// happens, and what each session is still to be told.
#[derive(Default)]
pub struct Nudger {
    seen: HashMap<String, Seen>,
    /// By PR (`prlinks::pr_key`) and session.
    owed: HashMap<(String, String), Owed>,
}

impl Nudger {
    /// The jobs due now, at `now` (ms). For each PR with its nudge on that the last board showed
    /// otherwise, what's new is owed to its primary session when that runs on this Mac; then each
    /// session owed something is sent it, unless it waits on you, a send to it is under way, or a
    /// failed one's wait isn't over. A PR first seen is only noted, so turning the nudge on tells
    /// of what happens from then on (and of an approval it has already, when turned on for that
    /// PR: turning `nudge_by_default` on doesn't tell every approved PR of yours to merge); a PR
    /// the board no longer lists is forgotten, though what's owed for it is still told.
    pub fn due(&mut self, board: &Board, now: i64) -> Vec<Job> {
        let runs_here = |k: &str| board.session(k).is_some_and(|(lane, s)| lane.device.is_none() && s.live && !s.archived);
        let mut seen_now = HashMap::with_capacity(board.prs.len());
        for pr in &board.prs {
            let key = crate::prlinks::pr_key(&pr.key);
            let seen = Seen { on: pr.nudge, updated_at: pr.updated_at, state: pr.state, failed: pr.checks.failed > 0 };
            let was = self.seen.get(&key).copied();
            seen_now.insert(key.clone(), seen);
            for ((pr_key, _), owed) in self.owed.iter_mut() {
                if *pr_key == key {
                    owed.pr = pr.clone();
                }
            }
            let Some(was) = was else { continue };
            // The board says the nudge only while a PR is open: one that just ended is still
            // told of that.
            let ended = was.on && was.state.is_open() && !pr.state.is_open();
            if !(pr.nudge || ended) {
                continue;
            }
            let why = Why {
                since: (was.on && pr.updated_at > was.updated_at).then_some(was.updated_at),
                approved: pr.state == PrState::Approved && (was.state != PrState::Approved || (!was.on && !pr.nudge_by_default)),
                changes: was.on && pr.state == PrState::Changes && was.state != PrState::Changes,
                ended: ended.then_some(pr.state),
                failed: if was.on && seen.failed && !was.failed { pr.checks.failing.clone() } else { Vec::new() },
            };
            let why = keep(board.nudge_about, pr.state.is_open(), why);
            if why == Why::default() {
                continue;
            }
            if let Some(s) = pr.primary.as_ref().filter(|k| runs_here(k)) {
                let owed = self.owed.entry((key.clone(), s.clone())).or_insert_with(|| Owed { pr: pr.clone(), why: Why::default(), sending: None, failures: 0, next_at: 0 });
                owed.why.add(why);
            }
        }
        self.seen = seen_now;
        // A session that stopped is told nothing more, nor one whose PR had its nudge turned off,
        // nor one that's no longer its PR's primary.
        let off: HashSet<String> = board.prs.iter().filter(|p| p.state.is_open() && !p.nudge).map(|p| crate::prlinks::pr_key(&p.key)).collect();
        let primary: HashMap<String, Option<&str>> = board.prs.iter().map(|p| (crate::prlinks::pr_key(&p.key), p.primary.as_deref())).collect();
        self.owed.retain(|(pr, s), o| o.sending.is_some() || (runs_here(s) && !off.contains(pr) && primary.get(pr).map_or(true, |p| *p == Some(s.as_str()))));
        // A send already under way to a session is let finish first: two typing into one terminal
        // at once would interleave.
        let busy: HashSet<String> = self.owed.iter().filter(|(_, o)| o.sending.is_some()).map(|((_, s), _)| s.clone()).collect();
        let waits = |k: &str| board.session(k).is_some_and(|(_, s)| crate::send::waits_on_you(s));
        let mut out = Vec::new();
        for ((_, s), o) in self.owed.iter_mut() {
            if o.sending.is_some() || o.why == Why::default() || now < o.next_at || busy.contains(s) || waits(s) {
                continue;
            }
            let why = std::mem::take(&mut o.why);
            o.sending = Some(why.clone());
            out.push(Job { pr: o.pr.clone(), why, session: s.clone(), last: o.failures + 1 >= TRIES, about: board.nudge_about });
        }
        out.sort_by(|a, b| (&a.pr.key, &a.session).cmp(&(&b.pr.key, &b.session)));
        out
    }

    /// How a job went, at `now` (ms). What was told is owed no longer; what wasn't is owed again,
    /// with what's come in since, and tried again later. Whether it will be.
    pub fn told(&mut self, job: &Job, ok: bool, now: i64) -> bool {
        let key = (crate::prlinks::pr_key(&job.pr.key), job.session.clone());
        let Some(o) = self.owed.get_mut(&key) else { return false };
        let sent = o.sending.take().unwrap_or_default();
        let again = !ok && o.failures + 1 < TRIES;
        if again {
            o.failures += 1;
            o.next_at = now + (RETRY_MS << (o.failures - 1).min(10)).min(RETRY_MAX_MS);
            let mut why = sent;
            why.add(std::mem::take(&mut o.why));
            o.why = why;
        } else {
            // Told, or given up on: what's come in since starts afresh.
            o.failures = 0;
            o.next_at = 0;
            if o.why == Why::default() {
                self.owed.remove(&key);
            }
        }
        again
    }
}

/// How much of what someone wrote goes in.
const BODY_CHARS: usize = 600;

fn excerpt(body: &str) -> String {
    let body = body.trim();
    if body.chars().count() <= BODY_CHARS {
        return body.to_string();
    }
    let cut: String = body.chars().take(BODY_CHARS).collect();
    format!("{}…", cut.trim_end())
}

/// What an open PR's message ends with: the news is no go-ahead to merge.
pub const NOT_A_MERGE: &str = "This message doesn't ask you to merge it: merge it only if you were asked to.";

/// What the session is told: the PR, each new thing on it, where it stands, and that it's the
/// session's to take on, but not to merge: a message that came by itself, as one of yours, must
/// never read as your go-ahead to merge (asked 2026-10-09), so an open PR's always says it
/// doesn't ask that. None when nothing new was by someone else (the PR's own agent pushed, say). `detail` is the PR as GitHub has it now, read when `why.since` says to; `login` is who
/// `gh` is signed in as, whose reviews aren't news. Only what `about` tells of is said (`keep`):
/// with feedback only, an approval or a dismissal among the reviews isn't either.
pub fn words(pr: &PrItem, why: &Why, detail: Option<&PrDetail>, login: Option<&str>, about: NudgeAbout) -> Option<String> {
    let why = &keep(about, pr.state.is_open(), why.clone());
    let mut items: Vec<String> = Vec::new();
    let (mut approved_said, mut changes_said) = (false, false);
    if let (Some(since), Some(d)) = (why.since, detail) {
        let fresh = |at: Option<i64>| at.is_some_and(|t| t > since);
        // A review's own words come as a comment at the same time by the same author.
        let mut said: Vec<&str> = Vec::new();
        for r in d.reviews.iter().filter(|r| fresh(r.at) && Some(r.author.as_str()) != login) {
            let body = d.comments.iter().find(|c| c.author == r.author && c.at == r.at && c.path.is_none()).map(|c| c.id.as_str());
            said.extend(body);
            let words = match r.state.as_str() {
                "APPROVED" => {
                    approved_said = true;
                    "approved it"
                }
                "CHANGES_REQUESTED" => {
                    changes_said = true;
                    "asked for changes"
                }
                "DISMISSED" => "had a review dismissed",
                _ => "reviewed it",
            };
            // Its words are left out with it, not told as a comment of their own.
            if about != NudgeAbout::Everything && matches!(r.state.as_str(), "APPROVED" | "DISMISSED") {
                continue;
            }
            match body.and_then(|id| d.comments.iter().find(|c| c.id == id)) {
                Some(c) => items.push(format!("{} {words}: “{}”", r.author, excerpt(&c.body))),
                None => items.push(format!("{} {words}.", r.author)),
            }
        }
        let mut comments: Vec<_> = d.comments.iter().filter(|c| fresh(c.at) && !c.mine && !said.contains(&c.id.as_str())).collect();
        comments.sort_by_key(|c| c.at);
        for c in comments {
            match &c.path {
                Some(path) => items.push(format!("{} commented on {path}: “{}”", c.author, excerpt(&c.body))),
                None => items.push(format!("{} commented: “{}”", c.author, excerpt(&c.body))),
            }
        }
    }
    if why.approved && !approved_said {
        items.push("It was approved.".into());
    }
    if why.changes && !changes_said {
        items.push("Changes were asked for.".into());
    }
    if !why.failed.is_empty() {
        items.push(format!("Its checks failed: {}.", why.failed.join(", ")));
    }
    match why.ended {
        Some(PrState::Merged) => items.push("It was merged.".into()),
        Some(_) => items.push("It was closed without being merged.".into()),
        None => {}
    }
    if items.is_empty() {
        return None;
    }
    let whose = if pr.mine { "your PR" } else { "PR" };
    let standing = if pr.state.is_open() { standing(pr) } else { "" };
    let next = if why.ended.is_some() {
        "Nothing more is wanted on it, unless you were told otherwise.".to_string()
    } else if pr.state == PrState::Approved && !changes_said && !why.changes && why.failed.is_empty() {
        NOT_A_MERGE.to_string()
    } else {
        format!("Take it from there, unless you were told to leave it to someone else. {NOT_A_MERGE}")
    };
    let list: Vec<String> = items.iter().map(|i| format!("- {i}")).collect();
    Some(format!("On {whose} #{} “{}” ({}):\n{}\n{}{next}", pr.number, pr.title.trim(), pr.url, list.join("\n"), standing))
}

/// Where an open PR stands, for the message's last line; empty when there's nothing to say.
fn standing(pr: &PrItem) -> &'static str {
    match (pr.merge, &pr.checks) {
        (Merge::Clean, _) => "It's ready to merge. ",
        (Merge::Conflicts, _) => "Its branch conflicts with its base. ",
        (Merge::Behind, _) => "Its branch is behind its base, which the repo wants brought up to date first. ",
        (_, c) if c.failed > 0 => "Some of its checks failed. ",
        (_, c) if c.pending > 0 => "Its checks are still running. ",
        _ => "",
    }
}

/// The message for a job, given how reading the PR's detail went (None when it wasn't read):
/// None when there's nothing to say, and an error when the detail couldn't be read and this
/// isn't the last try, to try again rather than leave out what others wrote.
fn message(job: &Job, detail: Option<Result<&PrDetail, &str>>, login: Option<&str>) -> Result<Option<String>, String> {
    let detail = match detail {
        Some(Err(e)) if !job.last => return Err(format!("What's new on {} couldn't be read: {e}", job.pr.key)),
        Some(Ok(d)) => Some(d),
        _ => None,
    };
    Ok(words(&job.pr, &job.why, detail, login, job.about))
}

/// Does each job in turn: reads the PR's detail from GitHub where its `updatedAt` moved (once
/// for the jobs that share it), works out the message, and sends it to the session as a window's
/// send does; tells `nudger` how each went, and the engine, which says so in a notice when
/// something didn't go.
pub fn send_now(engine: &Handle, nudger: &Mutex<Nudger>, jobs: Vec<Job>) {
    let login = engine.read(|b| b.github_login.clone());
    let mut details: HashMap<(String, i64), Result<PrDetail, String>> = HashMap::new();
    let mut failed = Vec::new();
    for job in jobs {
        // One that falls over is told as failed, so its session isn't left waiting on a send
        // that never says how it went.
        let went = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let detail = job.why.since.map(|since| details.entry((job.pr.key.clone(), since)).or_insert_with(|| crate::github::detail(&job.pr.repo_slug, job.pr.number).map_err(|e| e.to_string())).as_ref().map_err(String::as_str));
            message(&job, detail, login.as_deref()).and_then(|text| {
                let Some(text) = text else { return Ok(()) };
                crate::send::tell(engine, &job.session, &text).map(|_| ()).map_err(|e| {
                    let title = engine.read(|b| b.session(&job.session).map(|(_, s)| s.title.clone())).unwrap_or_else(|| job.session.clone());
                    format!("“{title}” wasn't told what's new on {}: {e}", job.pr.key)
                })
            })
        }))
        .unwrap_or_else(|_| Err(format!("Telling a session what's new on {} stopped partway.", job.pr.key)));
        let again = nudger.lock().unwrap_or_else(|e| e.into_inner()).told(&job, went.is_ok(), crate::util::now_ms());
        if let Err(e) = went {
            failed.push(if again { format!("{e} Codealong tries again in a few minutes.") } else { e });
        }
    }
    engine.nudged(if failed.is_empty() { Ok(()) } else { Err(failed.join(" ")) });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::github::{PrComment, PrReview};
    use crate::model::{Checks, PrSession, Session};

    /// An open PR of yours with its nudge on, last updated at `UPDATED`, whose primary session is
    /// the first of `sessions`.
    fn pr(key: &str, state: PrState, sessions: &[&str]) -> PrItem {
        PrItem {
            key: key.into(),
            repo_slug: key.split('#').next().unwrap().into(),
            repo: "spark".into(),
            project: "Spruce".into(),
            number: key.rsplit('#').next().unwrap().parse().unwrap(),
            title: "Let the agent add photos".into(),
            url: format!("https://github.com/{}/pull/{}", key.split('#').next().unwrap(), key.rsplit('#').next().unwrap()),
            state,
            author: Some("me".into()),
            mine: true,
            review_requested: false,
            requested_by: None,
            requested_at: None,
            snoozed_until: None,
            nudge: state.is_open(),
            nudge_by_default: false,
            primary: sessions.first().map(|k| k.to_string()),
            head: "photos".into(),
            base: Some("main".into()),
            checks: Checks::default(),
            merge: Merge::Unknown,
            additions: 1,
            deletions: 0,
            changed_files: 1,
            created_at: 0,
            updated_at: UPDATED,
            merged_at: None,
            closed_at: None,
            lanes: vec!["/work".into()],
            sessions: sessions.iter().map(|k| PrSession { key: (*k).into(), lane: "/work".into() }).collect(),
            head_oid: None,
            auto_review: None,
        }
    }

    const UPDATED: i64 = 1_000_000;

    fn off(p: PrItem) -> PrItem {
        PrItem { nudge: false, ..p }
    }

    fn live(key: &str) -> Session {
        Session { live: true, ..crate::fixtures::session(key) }
    }

    fn board(prs: Vec<PrItem>, sessions: Vec<Session>) -> Board {
        Board { prs, lanes: vec![crate::fixtures::lane("/work", sessions)], ..Board::default() }
    }

    const A: &str = "claude:claude:a";
    const B: &str = "claude:claude:b";

    fn comment(id: &str, author: &str, body: &str, at: i64, path: Option<&str>, mine: bool) -> PrComment {
        PrComment { id: id.into(), author: author.into(), body: body.into(), at: Some(at), url: None, path: path.map(String::from), resolved: false, mine }
    }

    fn detail(reviews: Vec<PrReview>, comments: Vec<PrComment>) -> PrDetail {
        PrDetail { body: String::new(), body_html: String::new(), mergeable: None, commits: 1, reviews, requested: vec![], comments, files: vec![], more_files: false }
    }

    /// The jobs due on `b` at `NOW`, each told as sent.
    fn sent(nudger: &mut Nudger, b: &Board) -> Vec<Job> {
        let jobs = nudger.due(b, NOW);
        for job in &jobs {
            assert!(!nudger.told(job, true, NOW));
        }
        jobs
    }

    const NOW: i64 = 1_800_000_000_000;

    #[test]
    fn an_update_on_a_pr_its_on_for_is_a_job_for_the_sessions_that_run_here_once() {
        let mut nudger = Nudger::default();
        let sessions = vec![live(A), Session { live: false, ..crate::fixtures::session(B) }];
        // First seen: noted. Then updated on GitHub: a job to read what's new since, for the
        // session that runs.
        assert!(sent(&mut nudger, &board(vec![pr("o/r#1", PrState::Open, &[A, B])], sessions.clone())).is_empty());
        let moved = PrItem { updated_at: UPDATED + 5, ..pr("o/r#1", PrState::Open, &[A, B]) };
        let jobs = sent(&mut nudger, &board(vec![moved.clone()], sessions.clone()));
        assert_eq!(jobs.len(), 1);
        assert_eq!((jobs[0].session.as_str(), jobs[0].why.clone()), (A, Why { since: Some(UPDATED), ..Why::default() }));
        assert!(sent(&mut nudger, &board(vec![moved.clone()], sessions.clone())).is_empty(), "told once");
        // Approved, with the update that brought it; then changes asked for; then merged.
        let approved = PrItem { state: PrState::Approved, updated_at: UPDATED + 9, ..moved.clone() };
        assert_eq!(sent(&mut nudger, &board(vec![approved.clone()], sessions.clone()))[0].why, Why { since: Some(UPDATED + 5), approved: true, ..Why::default() });
        let changes = PrItem { state: PrState::Changes, ..approved.clone() };
        assert_eq!(sent(&mut nudger, &board(vec![changes], sessions.clone()))[0].why, Why { changes: true, ..Why::default() });
        let merged = off(PrItem { state: PrState::Merged, ..approved });
        assert_eq!(sent(&mut nudger, &board(vec![merged.clone()], sessions.clone()))[0].why, Why { ended: Some(PrState::Merged), ..Why::default() });
        assert!(sent(&mut nudger, &board(vec![merged], sessions)).is_empty(), "and nothing after");
    }

    #[test]
    fn checks_failing_is_told_once_and_the_approval_it_had_when_turned_on() {
        let mut nudger = Nudger::default();
        let failing = |n: u32| PrItem { checks: Checks { failed: n, failing: vec!["lint".into()], ..Checks::default() }, ..pr("o/r#1", PrState::Open, &[A]) };
        sent(&mut nudger, &board(vec![failing(0)], vec![live(A)]));
        assert_eq!(sent(&mut nudger, &board(vec![failing(1)], vec![live(A)]))[0].why, Why { failed: vec!["lint".into()], ..Why::default() });
        assert!(sent(&mut nudger, &board(vec![failing(2)], vec![live(A)])).is_empty(), "still failing is no news");
        // Off for a PR that's approved; turned on: told of the approval it has, and not of what
        // happened while it was off.
        let mut nudger = Nudger::default();
        sent(&mut nudger, &board(vec![off(pr("o/r#2", PrState::Open, &[A]))], vec![live(A)]));
        assert!(sent(&mut nudger, &board(vec![off(PrItem { updated_at: UPDATED + 1, ..pr("o/r#2", PrState::Approved, &[A]) })], vec![live(A)])).is_empty());
        let on = PrItem { updated_at: UPDATED + 2, ..pr("o/r#2", PrState::Approved, &[A]) };
        assert_eq!(sent(&mut nudger, &board(vec![on], vec![live(A)]))[0].why, Why { approved: true, ..Why::default() });
    }

    #[test]
    fn only_the_prs_primary_session_is_told_and_only_while_it_runs() {
        let mut nudger = Nudger::default();
        let both = vec![live(A), live(B)];
        let at = |updated: i64, primary: Option<&str>| PrItem { updated_at: updated, primary: primary.map(String::from), ..pr("o/r#1", PrState::Open, &[A, B]) };
        sent(&mut nudger, &board(vec![at(UPDATED, Some(A))], both.clone()));
        let jobs = sent(&mut nudger, &board(vec![at(UPDATED + 1, Some(A))], both.clone()));
        assert_eq!(jobs.iter().map(|j| j.session.as_str()).collect::<Vec<_>>(), [A]);
        // Another picked: it's told from then on.
        let jobs = sent(&mut nudger, &board(vec![at(UPDATED + 2, Some(B))], both.clone()));
        assert_eq!(jobs.iter().map(|j| j.session.as_str()).collect::<Vec<_>>(), [B]);
        // The primary stopped: nobody, though another of its sessions runs.
        assert!(sent(&mut nudger, &board(vec![at(UPDATED + 3, Some(B))], vec![live(A), Session { live: false, ..crate::fixtures::session(B) }])).is_empty());
        assert!(sent(&mut nudger, &board(vec![at(UPDATED + 4, None)], both)).is_empty());
    }

    #[test]
    fn what_a_session_was_owed_goes_when_it_is_no_longer_the_primary() {
        use crate::model::Status;
        let mut nudger = Nudger::default();
        let waiting = Session { status: Status::Needs, ..live(A) };
        let at = |updated: i64, primary: &str| PrItem { updated_at: updated, primary: Some(primary.into()), ..pr("o/r#1", PrState::Open, &[A, B]) };
        sent(&mut nudger, &board(vec![at(UPDATED, A)], vec![waiting.clone(), live(B)]));
        assert!(sent(&mut nudger, &board(vec![at(UPDATED + 1, A)], vec![waiting, live(B)])).is_empty(), "A waits on you");
        assert!(sent(&mut nudger, &board(vec![at(UPDATED + 1, B)], vec![live(A), live(B)])).is_empty(), "B became the primary after it");
    }

    #[test]
    fn turning_the_default_on_tells_no_approval_a_pr_had_already() {
        let mut nudger = Nudger::default();
        let approved = |nudge: bool, by_default: bool, updated: i64| PrItem { nudge, nudge_by_default: by_default, updated_at: updated, ..pr("o/r#1", PrState::Approved, &[A]) };
        sent(&mut nudger, &board(vec![approved(false, false, UPDATED)], vec![live(A)]));
        assert!(sent(&mut nudger, &board(vec![approved(true, true, UPDATED)], vec![live(A)])).is_empty());
        // What happens on it from then on is told.
        let merged = PrItem { state: PrState::Merged, nudge: false, nudge_by_default: false, ..approved(true, true, UPDATED) };
        assert_eq!(sent(&mut nudger, &board(vec![merged], vec![live(A)]))[0].why, Why { ended: Some(PrState::Merged), ..Why::default() });
    }

    #[test]
    fn a_pr_first_seen_or_on_another_device_tells_nobody() {
        use crate::model::DeviceTag;
        let mut nudger = Nudger::default();
        assert!(sent(&mut nudger, &board(vec![pr("o/r#1", PrState::Approved, &[A])], vec![live(A)])).is_empty());
        // Gone from the board and back, changed: first sight again.
        assert!(sent(&mut nudger, &board(vec![], vec![live(A)])).is_empty());
        assert!(sent(&mut nudger, &board(vec![PrItem { updated_at: UPDATED + 1, ..pr("o/r#1", PrState::Approved, &[A]) }], vec![live(A)])).is_empty());
        let mut nudger = Nudger::default();
        let elsewhere = |at: i64| {
            let mut b = board(vec![PrItem { updated_at: at, ..pr("o/r#1", PrState::Open, &[A]) }], vec![live(A)]);
            b.lanes[0].device = Some(DeviceTag { name: "Studio".into(), short: "st".into(), color: "blue".into(), online: true, last_seen: None });
            b
        };
        sent(&mut nudger, &elsewhere(UPDATED));
        assert!(sent(&mut nudger, &elsewhere(UPDATED + 1)).is_empty());
    }

    #[test]
    fn a_session_waiting_on_you_is_told_once_it_isnt() {
        use crate::model::Status;
        let mut nudger = Nudger::default();
        let waiting = Session { status: Status::Needs, ..live(A) };
        sent(&mut nudger, &board(vec![pr("o/r#1", PrState::Open, &[A])], vec![waiting.clone()]));
        let approved = PrItem { updated_at: UPDATED + 5, ..pr("o/r#1", PrState::Approved, &[A]) };
        assert!(sent(&mut nudger, &board(vec![approved.clone()], vec![waiting])).is_empty(), "what's typed would answer its prompt");
        // Answered: told, though the PR hasn't changed since.
        let jobs = sent(&mut nudger, &board(vec![approved.clone()], vec![live(A)]));
        assert_eq!(jobs[0].why, Why { since: Some(UPDATED), approved: true, ..Why::default() });
        assert!(sent(&mut nudger, &board(vec![approved], vec![live(A)])).is_empty());
    }

    #[test]
    fn a_refused_send_is_tried_again_later_with_what_came_since() {
        let mut nudger = Nudger::default();
        let at = |updated: i64, state| board(vec![PrItem { updated_at: updated, ..pr("o/r#1", state, &[A]) }], vec![live(A)]);
        nudger.due(&at(UPDATED, PrState::Open), NOW);
        let job = nudger.due(&at(UPDATED + 5, PrState::Approved), NOW).remove(0);
        // While the send is under way, more comes in: it waits for that send.
        assert!(nudger.due(&at(UPDATED + 9, PrState::Changes), NOW).is_empty());
        assert!(nudger.told(&job, false, NOW), "refused: tried again");
        assert!(nudger.due(&at(UPDATED + 9, PrState::Changes), NOW + 1000).is_empty(), "not straight away");
        let again = nudger.due(&at(UPDATED + 9, PrState::Changes), NOW + RETRY_MS).remove(0);
        // From the first update on, with both changes of state.
        assert_eq!(again.why, Why { since: Some(UPDATED), approved: true, changes: true, ..Why::default() });
        assert!(!nudger.told(&again, true, NOW + RETRY_MS));
        assert!(nudger.due(&at(UPDATED + 9, PrState::Changes), NOW + 10 * RETRY_MAX_MS).is_empty(), "told: owed no longer");
    }

    #[test]
    fn a_send_that_keeps_failing_waits_longer_each_time_and_is_given_up_on() {
        let mut nudger = Nudger::default();
        let b = |updated: i64| board(vec![PrItem { updated_at: updated, ..pr("o/r#1", PrState::Open, &[A]) }], vec![live(A)]);
        nudger.due(&b(UPDATED), NOW);
        let mut at = NOW;
        let mut job = nudger.due(&b(UPDATED + 5), at).remove(0);
        let mut waits = Vec::new();
        for _ in 1..TRIES {
            assert!(!job.last);
            assert!(nudger.told(&job, false, at));
            // The first second it's handed out again.
            let (next, again) = (at..).step_by(1000).find_map(|t| nudger.due(&b(UPDATED + 5), t).pop().map(|j| (t, j))).unwrap();
            waits.push((next - at) / 1000);
            (at, job) = (next, again);
        }
        assert_eq!(waits, [30, 60, 120, 240, 480]);
        // The last try tells what the board knows when GitHub can't be read, and isn't retried.
        assert!(job.last);
        assert!(!nudger.told(&job, false, at));
        assert!(nudger.due(&b(UPDATED + 5), at + 10 * RETRY_MAX_MS).is_empty());
    }

    #[test]
    fn news_that_cant_be_read_is_tried_again_before_its_left_out() {
        let p = pr("o/r#1", PrState::Approved, &[A]);
        let job = Job { pr: p.clone(), why: Why { since: Some(UPDATED), approved: true, ..Why::default() }, session: A.into(), last: false, about: NudgeAbout::Everything };
        let e = message(&job, Some(Err("gh timed out after 30s")), Some("me")).unwrap_err();
        assert_eq!(e, "What's new on o/r#1 couldn't be read: gh timed out after 30s");
        // The last try says what the board knows.
        let text = message(&Job { last: true, ..job.clone() }, Some(Err("gh timed out after 30s")), Some("me")).unwrap().unwrap();
        assert!(text.contains("- It was approved.\n"), "{text}");
        // Read: what others wrote.
        let d = detail(vec![PrReview { author: "ken".into(), state: "APPROVED".into(), at: Some(UPDATED + 1) }], vec![]);
        assert!(message(&job, Some(Ok(&d)), Some("me")).unwrap().unwrap().contains("- ken approved it.\n"));
    }

    #[test]
    fn says_what_others_did_since_and_nothing_for_what_was_yours() {
        let p = pr("o/r#1", PrState::Changes, &[A]);
        let d = detail(
            vec![
                PrReview { author: "ana".into(), state: "CHANGES_REQUESTED".into(), at: Some(UPDATED + 10) },
                PrReview { author: "me".into(), state: "COMMENTED".into(), at: Some(UPDATED + 20) },
                PrReview { author: "old".into(), state: "APPROVED".into(), at: Some(UPDATED - 1) },
            ],
            vec![
                comment("c1", "ana", "Please add a test.", UPDATED + 10, None, false),
                comment("c2", "ana", "This can be a map.", UPDATED + 11, Some("src/a.ts"), false),
                comment("c3", "me", "Done.", UPDATED + 20, None, true),
                comment("c4", "ken", "Before all that.", UPDATED - 5, None, false),
            ],
        );
        let why = Why { since: Some(UPDATED), changes: true, ..Why::default() };
        let text = words(&p, &why, Some(&d), Some("me"), NudgeAbout::Everything).unwrap();
        assert_eq!(
            text,
            "On your PR #1 “Let the agent add photos” (https://github.com/o/r/pull/1):\n- ana asked for changes: “Please add a test.”\n- ana commented on src/a.ts: “This can be a map.”\nTake it from there, unless you were told to leave it to someone else. This message doesn't ask you to merge it: merge it only if you were asked to."
        );
        // Only your own push moved it: nothing to say.
        assert_eq!(words(&p, &Why { since: Some(UPDATED + 30), ..Why::default() }, Some(&d), Some("me"), NudgeAbout::Everything), None);
        // The detail couldn't be read: what the board knows is said.
        assert!(words(&p, &why, None, Some("me"), NudgeAbout::Everything).unwrap().contains("- Changes were asked for.\n"));
    }

    #[test]
    fn says_an_approval_a_merge_and_failing_checks() {
        let approved = PrItem { merge: Merge::Clean, ..pr("o/r#1", PrState::Approved, &[A]) };
        let d = detail(vec![PrReview { author: "ken".into(), state: "APPROVED".into(), at: Some(UPDATED + 1) }], vec![]);
        let text = words(&approved, &Why { since: Some(UPDATED), approved: true, ..Why::default() }, Some(&d), Some("me"), NudgeAbout::Everything).unwrap();
        // The approval is news, never a go-ahead to merge.
        assert_eq!(text, "On your PR #1 “Let the agent add photos” (https://github.com/o/r/pull/1):\n- ken approved it.\nIt's ready to merge. This message doesn't ask you to merge it: merge it only if you were asked to.");
        // Turned on for one approved already, with nothing read: said in the board's words.
        assert!(words(&approved, &Why { approved: true, ..Why::default() }, None, None, NudgeAbout::Everything).unwrap().contains("- It was approved.\n"));
        let merged = off(PrItem { state: PrState::Merged, mine: false, ..pr("o/r#1", PrState::Open, &[A]) });
        assert_eq!(
            words(&merged, &Why { ended: Some(PrState::Merged), ..Why::default() }, None, None, NudgeAbout::Everything).unwrap(),
            "On PR #1 “Let the agent add photos” (https://github.com/o/r/pull/1):\n- It was merged.\nNothing more is wanted on it, unless you were told otherwise."
        );
        let failing = PrItem { checks: Checks { failed: 2, failing: vec!["lint".into(), "test".into()], ..Checks::default() }, ..pr("o/r#1", PrState::Open, &[A]) };
        let text = words(&failing, &Why { failed: vec!["lint".into(), "test".into()], ..Why::default() }, None, None, NudgeAbout::Everything).unwrap();
        assert!(text.contains("- Its checks failed: lint, test.\nSome of its checks failed. Take it from there"), "{text}");
        // No open PR's message reads as a go-ahead to merge, whatever came in.
        let open = PrItem { merge: Merge::Clean, ..pr("o/r#1", PrState::Approved, &[A]) };
        for why in [Why { approved: true, ..Why::default() }, Why { changes: true, ..Why::default() }, Why { failed: vec!["lint".into()], ..Why::default() }] {
            let text = words(&open, &why, None, None, NudgeAbout::Everything).unwrap();
            assert!(text.ends_with(NOT_A_MERGE) && !text.contains("merging it"), "{text}");
        }
    }

    #[test]
    fn with_feedback_only_it_tells_of_comments_and_changes_asked_for_never_an_approval_or_a_merge() {
        let mut nudger = Nudger::default();
        let feedback = |prs: Vec<PrItem>, about: NudgeAbout| Board { nudge_about: about, ..board(prs, vec![live(A)]) };
        let open = pr("o/r#1", PrState::Open, &[A]);
        sent(&mut nudger, &feedback(vec![open.clone()], NudgeAbout::Feedback));
        // Approved, merged, or checks failing: nothing, unless checks are asked for too.
        let approved = PrItem { state: PrState::Approved, ..open.clone() };
        assert!(sent(&mut nudger, &feedback(vec![approved.clone()], NudgeAbout::Feedback)).is_empty());
        let failing = PrItem { checks: Checks { failed: 1, failing: vec!["lint".into()], ..Checks::default() }, ..approved.clone() };
        assert!(sent(&mut nudger, &feedback(vec![failing.clone()], NudgeAbout::Feedback)).is_empty());
        let passing = PrItem { checks: Checks::default(), ..failing.clone() };
        sent(&mut nudger, &feedback(vec![passing], NudgeAbout::FeedbackAndChecks));
        let jobs = sent(&mut nudger, &feedback(vec![failing.clone()], NudgeAbout::FeedbackAndChecks));
        assert_eq!((jobs[0].why.clone(), jobs[0].about), (Why { failed: vec!["lint".into()], ..Why::default() }, NudgeAbout::FeedbackAndChecks));
        // Changes asked for are told, and so is what moved the PR, for its comments.
        let changes = PrItem { state: PrState::Changes, updated_at: UPDATED + 5, ..failing.clone() };
        assert_eq!(sent(&mut nudger, &feedback(vec![changes.clone()], NudgeAbout::Feedback))[0].why, Why { since: Some(UPDATED), changes: true, ..Why::default() });
        let merged = off(PrItem { state: PrState::Merged, updated_at: UPDATED + 9, ..changes });
        assert!(sent(&mut nudger, &feedback(vec![merged.clone()], NudgeAbout::Feedback)).is_empty(), "a merge isn't feedback");
        // Its words: an approval and a dismissal are left out, their text with them; the rest is
        // said.
        let d = detail(
            vec![
                PrReview { author: "ken".into(), state: "APPROVED".into(), at: Some(UPDATED + 1) },
                PrReview { author: "ana".into(), state: "CHANGES_REQUESTED".into(), at: Some(UPDATED + 2) },
                PrReview { author: "bo".into(), state: "DISMISSED".into(), at: Some(UPDATED + 3) },
            ],
            vec![comment("c1", "ken", "Looks good.", UPDATED + 1, None, false), comment("c2", "ken", "Nit: a map.", UPDATED + 1, Some("src/a.ts"), false), comment("c3", "ana", "Add a test.", UPDATED + 2, None, false)],
        );
        let why = Why { since: Some(UPDATED), approved: true, ..Why::default() };
        let text = words(&approved, &why, Some(&d), Some("me"), NudgeAbout::Feedback).unwrap();
        assert_eq!(
            text,
            "On your PR #1 “Let the agent add photos” (https://github.com/o/r/pull/1):\n- ana asked for changes: “Add a test.”\n- ken commented on src/a.ts: “Nit: a map.”\nTake it from there, unless you were told to leave it to someone else. This message doesn't ask you to merge it: merge it only if you were asked to."
        );
        assert!(words(&approved, &why, Some(&d), Some("me"), NudgeAbout::Everything).unwrap().contains("- ken approved it: “Looks good.”\n"));
        // Only an approval: nothing to say. Nor anything about a PR that ended.
        let only = detail(vec![PrReview { author: "ken".into(), state: "APPROVED".into(), at: Some(UPDATED + 1) }], vec![]);
        assert_eq!(words(&approved, &why, Some(&only), Some("me"), NudgeAbout::Feedback), None);
        assert_eq!(words(&merged, &Why { since: Some(UPDATED), ended: Some(PrState::Merged), ..Why::default() }, Some(&d), Some("me"), NudgeAbout::FeedbackAndChecks), None);
    }

    #[test]
    fn says_where_the_pr_stands() {
        let at = |merge, failed, pending| standing(&PrItem { merge, checks: Checks { failed, pending, ..Checks::default() }, ..pr("o/r#1", PrState::Approved, &[]) });
        assert_eq!(at(Merge::Clean, 0, 0), "It's ready to merge. ");
        assert_eq!(at(Merge::Conflicts, 0, 2), "Its branch conflicts with its base. ");
        assert_eq!(at(Merge::Blocked, 1, 2), "Some of its checks failed. ");
        assert_eq!(at(Merge::Unknown, 0, 2), "Its checks are still running. ");
        assert_eq!(at(Merge::Unknown, 0, 0), "");
    }
}
