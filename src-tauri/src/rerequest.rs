//! Asking a reviewer to look again at a PR of yours once its fixes are pushed, where
//! `[pull_requests] rerequest_after_fixes` is on for its repo: they asked for changes, commits
//! came after their last word, and nobody has said so on it (`TurnKind::ReplyNeeded`, board.rs
//! `turn_of`). GitHub tells a reviewer of a review request, which their Codealong reviews again by
//! itself, but not of a push, so a PR whose agent pushed the fixes and said nothing waited on
//! nobody (two PRs on 2026-10-22, where each request made by hand was answered by
//! the reviewer's agent within minutes).
//!
//! It's asked once the PR has been quiet for `SETTLE_MS` (its last update, which a push is, and its
//! last commit) and none of its sessions is in a turn, so a run of fixes is asked about once, and a
//! session about to say so itself can; once for each reviewer and commit, and a request GitHub
//! refuses is tried again a few times, each `RETRY_MS` after the last. The board can be behind
//! GitHub (a repo no longer read keeps its last answer, a restart reads a cached one), so the PR is
//! read afresh before anyone is asked, and they're asked only if that still owes them a word and
//! has been quiet as long (`still_owed`), and the board and settings, looked at again after the
//! read, still let it go (`allowed`); else it's looked at again `RETRY_MS` later.
//! Asked, the PR's timeline has the request, and the PR reads as the reviewer's move
//! (`RereviewAsked`) from GitHub's next answer on.

use std::collections::HashMap;

use crate::config::PullRequestsCfg;
use crate::engine::Handle;
use crate::github::PrRaw;
use crate::model::{Board, PrItem, Status, TurnKind};

/// How long a PR is quiet before its reviewer is asked: a session pushing a run of fixes is done
/// by then, or says so itself.
pub const SETTLE_MS: i64 = 10 * 60_000;
/// How long after a request GitHub refused it's tried again, and how many times it's tried.
const RETRY_MS: i64 = 15 * 60_000;
const TRIES: u32 = 3;

/// A reviewer to ask again.
#[derive(Debug, Clone, PartialEq)]
pub struct Job {
    /// The PR's `owner/name#number`.
    pub pr: String,
    pub repo: String,
    pub number: u64,
    pub reviewer: String,
    /// The commit they're asked to review.
    pub head: String,
}

impl Job {
    fn id(&self) -> (String, String, String) {
        (self.pr.clone(), self.reviewer.to_lowercase(), self.head.clone())
    }
}

/// What's been asked of whom, by PR, reviewer and commit.
#[derive(Default)]
pub struct Rerequester {
    tried: HashMap<(String, String, String), Tried>,
}

#[derive(Clone, Copy)]
struct Tried {
    went: Went,
    /// How many times GitHub refused it.
    refused: u32,
    at: i64,
}

/// How a request went.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Went {
    /// It went, or it's going.
    Sent,
    /// GitHub refused it, or couldn't be read first: tried again a few times.
    Failed,
    /// Not asked after all: GitHub or the board, looked at again, said not yet or not any more.
    /// Looked at again later, while it's still owed.
    Skipped,
}

impl Rerequester {
    /// The reviewers to ask again now, on `board`'s PRs, with `[pull_requests]` as config.toml
    /// has it everywhere (`global`). Each is marked asked as it's handed out, so a board that
    /// comes while the request goes doesn't ask again.
    pub fn due(&mut self, board: &Board, global: &PullRequestsCfg, now: i64) -> Vec<Job> {
        self.tried.retain(|(pr, ..), _| board.prs.iter().any(|p| &p.key == pr));
        let mut jobs = Vec::new();
        for job in board.prs.iter().filter(|p| p.mine && p.state.is_open()).flat_map(|pr| owed(board, pr, global, now)) {
            let due = match self.tried.get(&job.id()) {
                None => true,
                Some(t) => match t.went {
                    Went::Sent => false,
                    Went::Failed => t.refused < TRIES && now - t.at >= RETRY_MS,
                    Went::Skipped => now - t.at >= RETRY_MS,
                },
            };
            if due {
                let refused = self.tried.get(&job.id()).map_or(0, |t| t.refused);
                self.tried.insert(job.id(), Tried { went: Went::Sent, refused, at: now });
                jobs.push(job);
            }
        }
        jobs
    }

    /// How a request went: one that didn't is tried again later.
    pub fn went(&mut self, job: &Job, went: Went, now: i64) {
        if let Some(t) = self.tried.get_mut(&job.id()) {
            t.went = went;
            t.at = now;
            t.refused += u32::from(went == Went::Failed);
        }
    }
}

/// Since when `pr`'s unsaid fixes have been quiet: its last update (a push is one, and so is
/// anything said) or its last commit, whichever came later. None unless its fixes went unsaid.
fn quiet_since(pr: &PrItem) -> Option<i64> {
    let turn = pr.turn.as_ref().filter(|t| t.kind == TurnKind::ReplyNeeded)?;
    Some(turn.since?.max(pr.updated_at))
}

/// The PRs on `board` whose unsaid fixes have been quiet `SETTLE_MS` at `now`. Nothing the windows
/// get says so, and it comes with time alone, so the board's hash takes it in (engine.rs
/// `board_hash`), as reconnect.rs `lifted` is: the board that's composed then goes out, and `due`
/// runs on it.
pub fn settled(board: &Board, now: i64) -> Vec<&str> {
    board.prs.iter().filter(|p| quiet_since(p).is_some_and(|at| now - at >= SETTLE_MS)).map(|p| p.key.as_str()).collect()
}

/// Whether `board` lets a request for `pr` go: its repo has it on, and none of its sessions is in
/// a turn, working or waiting on you to allow something.
fn allowed(board: &Board, pr: &PrItem, global: &PullRequestsCfg) -> bool {
    let in_turn = |s: &crate::model::Session| s.status == Status::Working || (s.status == Status::Needs && !s.asks);
    board.repo_settings.pull_requests(global, pr).rerequest_after_fixes && !pr.sessions.iter().any(|s| board.session(&s.key).is_some_and(|(_, s)| in_turn(s)))
}

/// The reviewers `pr` owes a request to now: it's yours, its fixes went unsaid (`ReplyNeeded`, each
/// reviewer it holds for), it's been quiet `SETTLE_MS`, and `allowed`.
fn owed(board: &Board, pr: &PrItem, global: &PullRequestsCfg, now: i64) -> Vec<Job> {
    let (Some(turn), Some(head), Some(quiet)) = (pr.turn.as_ref(), pr.head_oid.as_ref(), quiet_since(pr)) else { return Vec::new() };
    if now - quiet < SETTLE_MS || !allowed(board, pr, global) {
        return Vec::new();
    }
    let job = |reviewer: &String| Job { pr: pr.key.clone(), repo: pr.repo_slug.clone(), number: pr.number, reviewer: reviewer.clone(), head: head.clone() };
    std::iter::once(&turn.who).chain(&turn.also).map(job).collect()
}

/// Whether `job`'s reviewer is still owed a request at `now` by its PR as GitHub has it (`raw`,
/// read afresh as `login`): still open at the job's commit, its fixes still unsaid to them, and
/// quiet `SETTLE_MS` as the board's copy was (`quiet_since`).
fn still_owed(raw: &PrRaw, login: Option<&str>, job: &Job, now: i64) -> bool {
    let mine = login.is_some() && raw.author.as_deref() == login;
    let Some(t) = crate::board::turn_of(raw, mine, login, &Default::default()).filter(|t| t.kind == TurnKind::ReplyNeeded) else { return false };
    let quiet = t.since.map(|at| at.max(raw.updated_at)).is_some_and(|at| now - at >= SETTLE_MS);
    let theirs = std::iter::once(&t.who).chain(&t.also).any(|w| w.eq_ignore_ascii_case(&job.reviewer));
    raw.state.is_open() && raw.head_oid.as_deref() == Some(job.head.as_str()) && theirs && quiet
}

/// Asks each job's reviewer, through `gh`, once its PR read afresh says it still holds and the
/// board and settings as they are then still let it go, telling `rerequester` how each went, and
/// the engine, which says so in a notice when one didn't.
pub fn send_now(engine: &Handle, rerequester: &std::sync::Mutex<Rerequester>, jobs: Vec<Job>) {
    let mut read: HashMap<String, Result<(PrRaw, Option<String>), String>> = HashMap::new();
    let mut failed = Vec::new();
    for job in jobs {
        let fresh = read.entry(job.pr.clone()).or_insert_with(|| crate::github::one(&job.repo, job.number).map_err(|e| e.to_string()));
        let now = crate::util::now_ms();
        // Reading GitHub, and asking for the jobs before, took a while: a session may have begun a
        // turn on it, or the setting been turned off, since `due`.
        let goes = || {
            let global = crate::config::current().map(|c| c.pull_requests.clone()).unwrap_or_default();
            engine.read(|b| b.prs.iter().find(|p| p.key == job.pr).is_some_and(|pr| allowed(b, pr, &global)))
        };
        let went = match fresh {
            Ok((raw, login)) if still_owed(raw, login.as_deref(), &job, now) && goes() => crate::github::request_review(&job.repo, job.number, &job.reviewer).map(|()| Went::Sent).map_err(|e| e.to_string()),
            Ok(_) => Ok(Went::Skipped),
            Err(e) => Err(format!("GitHub couldn't be asked how it stands: {e}")),
        };
        rerequester.lock().unwrap_or_else(|e| e.into_inner()).went(&job, *went.as_ref().unwrap_or(&Went::Failed), crate::util::now_ms());
        if let Err(e) = went {
            failed.push(format!("{} wasn't asked to review {} again after its fixes: {e}", job.reviewer, job.pr));
        }
    }
    engine.rerequested(if failed.is_empty() { Ok(()) } else { Err(failed.join(" ")) });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Checks, Merge, PrSession, PrState, Session, Turn};

    const NOW: i64 = 1_800_000_000_000;
    const A: &str = "claude:claude:a";

    fn pr(turn: Option<Turn>) -> PrItem {
        PrItem {
            key: "o/r#7".into(),
            repo_slug: "o/r".into(),
            repo: "r".into(),
            project: "P".into(),
            number: 7,
            title: "Add teams".into(),
            url: "https://github.com/o/r/pull/7".into(),
            state: PrState::Changes,
            author: Some("me".into()),
            mine: true,
            review_requested: false,
            requested_by: None,
            requested_at: None,
            reviewers_asked: Vec::new(),
            snoozed_until: None,
            nudge: false,
            nudge_by_default: false,
            primary: Some(A.into()),
            primary_by: None,
            head: "teams".into(),
            base: Some("main".into()),
            checks: Checks::default(),
            merge: Merge::Unknown,
            additions: 1,
            deletions: 0,
            changed_files: 1,
            created_at: 0,
            updated_at: 0,
            merged_at: None,
            closed_at: None,
            lanes: vec!["/work".into()],
            sessions: vec![PrSession { key: A.into(), lane: "/work".into(), pushes: 1, primary: None }],
            head_oid: Some("bbb".into()),
            authored_by: None,
            labels: Vec::new(),
            auto_review: None,
            last_review: None,
            stale_approval: None,
            turn,
            shared_review: Default::default(),
        }
    }

    fn unsaid(pushed: i64) -> Option<Turn> {
        Some(Turn { kind: TurnKind::ReplyNeeded, who: "ken".into(), also: Vec::new(), since: Some(pushed) })
    }

    fn board(p: PrItem, status: Status) -> Board {
        Board { prs: vec![p], lanes: vec![crate::fixtures::lane("/work", vec![Session { status, ..crate::fixtures::session(A) }])], ..Board::default() }
    }

    const ON: PullRequestsCfg = PullRequestsCfg { nudge_by_default: false, nudge_about: crate::config::NudgeAbout::Everything, rerequest_after_fixes: true };

    #[test]
    fn asks_the_reviewer_once_the_fixes_have_settled_and_nobody_works_on_them() {
        let mut r = Rerequester::default();
        let pushed = NOW - SETTLE_MS;
        // Too soon after the push, or while its session is still in a turn: not yet.
        assert!(r.due(&board(pr(unsaid(NOW - 60_000)), Status::Idle), &ON, NOW).is_empty());
        assert!(r.due(&board(pr(unsaid(pushed)), Status::Working), &ON, NOW).is_empty());
        // Waiting on you to allow something is still its turn; asking you something isn't.
        assert!(r.due(&board(pr(unsaid(pushed)), Status::Needs), &ON, NOW).is_empty());
        // A commit made long before it was pushed: the push moved the PR's update, which is waited on.
        assert!(r.due(&board(PrItem { updated_at: NOW - 60_000, ..pr(unsaid(pushed)) }, Status::Idle), &ON, NOW).is_empty());
        let b = board(pr(unsaid(pushed)), Status::Idle);
        let jobs = r.due(&b, &ON, NOW);
        assert_eq!(jobs, vec![Job { pr: "o/r#7".into(), repo: "o/r".into(), number: 7, reviewer: "ken".into(), head: "bbb".into() }]);
        // Once for the commit, while it goes and after.
        assert!(r.due(&b, &ON, NOW + 1).is_empty());
        r.went(&jobs[0], Went::Sent, NOW + 2);
        assert!(r.due(&b, &ON, NOW + RETRY_MS * 10).is_empty());
        // A later push is asked about again.
        let later = board(PrItem { head_oid: Some("ccc".into()), ..pr(unsaid(pushed)) }, Status::Idle);
        assert_eq!(r.due(&later, &ON, NOW).len(), 1);
    }

    #[test]
    fn a_push_settling_changes_the_boards_hash_so_its_asked_about_without_anything_else_changing() {
        let b = board(pr(unsaid(NOW - SETTLE_MS + 1)), Status::Idle);
        assert!(settled(&b, NOW).is_empty());
        assert_eq!(settled(&b, NOW + 1), vec!["o/r#7"]);
    }

    #[test]
    fn asks_only_where_its_on_and_the_fixes_went_unsaid() {
        let mut r = Rerequester::default();
        let pushed = NOW - SETTLE_MS;
        let off = PullRequestsCfg { rerequest_after_fixes: false, ..ON };
        assert!(r.due(&board(pr(unsaid(pushed)), Status::Idle), &off, NOW).is_empty());
        // Your table for the repo turns it on there.
        let mut on_here = board(pr(unsaid(pushed)), Status::Idle);
        on_here.repo_settings.mine.insert(crate::config::repo_key("o/r"), crate::config::RepoCfg { pull_requests: crate::config::PullRequestsPatch { rerequest_after_fixes: Some(true), ..Default::default() }, ..Default::default() });
        assert_eq!(r.due(&on_here, &off, NOW).len(), 1);
        // Asked already, or someone else's PR: nothing to ask.
        let asked = Some(Turn { kind: TurnKind::RereviewAsked, who: "ken".into(), also: Vec::new(), since: Some(pushed) });
        assert!(Rerequester::default().due(&board(pr(asked), Status::Idle), &ON, NOW).is_empty());
        assert!(Rerequester::default().due(&board(PrItem { mine: false, ..pr(unsaid(pushed)) }, Status::Idle), &ON, NOW).is_empty());
        assert!(Rerequester::default().due(&board(PrItem { state: PrState::Merged, ..pr(unsaid(pushed)) }, Status::Idle), &ON, NOW).is_empty());
    }

    #[test]
    fn asks_each_reviewer_the_fixes_went_unsaid_to() {
        let two = Some(Turn { also: vec!["ana".into()], ..unsaid(NOW - SETTLE_MS).unwrap() });
        let jobs = Rerequester::default().due(&board(pr(two), Status::Idle), &ON, NOW);
        assert_eq!(jobs.iter().map(|j| j.reviewer.as_str()).collect::<Vec<_>>(), vec!["ken", "ana"]);
    }

    #[test]
    fn asks_only_while_github_still_says_the_fixes_went_unsaid() {
        let job = Job { pr: "o/r#7".into(), repo: "o/r".into(), number: 7, reviewer: "Ken".into(), head: "bbb".into() };
        let said = |s: Vec<crate::github::Said>| PrRaw {
            number: 7,
            title: String::new(),
            url: String::new(),
            state: PrState::Changes,
            head_ref: "teams".into(),
            head_owner: None,
            head_oid: Some("bbb".into()),
            updated_at: 0,
            merged_at: None,
            closed_at: None,
            checks: Checks::default(),
            merge: Merge::Unknown,
            author: Some("me".into()),
            base_ref: None,
            created_at: 0,
            additions: 0,
            deletions: 0,
            changed_files: 0,
            review_requested: false,
            requested_by: None,
            requested_at: None,
            reviewers_asked: Vec::new(),
            assigned: None,
            authored_by: None,
            labels: Vec::new(),
            last_review: None,
            stale_approval: None,
            head_at: Some(110),
            said: s,
            said_from: None,
            changes_asked: vec![crate::github::ChangesAsked { who: "ken".into(), at: 100, commit: Some("aaa".into()) }],
        };
        let ken = |at| crate::github::Said { who: "ken".into(), at, asked: None, url: None };
        // Pushed at 110, nothing on it since.
        let now = 110 + SETTLE_MS;
        assert!(still_owed(&said(vec![ken(100)]), Some("me"), &job, now));
        // Not quiet long enough yet, as GitHub has it now: a push, or anything said, moved it.
        assert!(!still_owed(&said(vec![ken(100)]), Some("me"), &job, now - 1));
        assert!(!still_owed(&PrRaw { updated_at: now - 60_000, ..said(vec![ken(100)]) }, Some("me"), &job, now));
        // Said since, by anyone of yours, or asked again: nothing to ask.
        assert!(!still_owed(&said(vec![ken(100), crate::github::Said { who: "me".into(), at: 120, asked: None, url: None }]), Some("me"), &job, now));
        assert!(!still_owed(&said(vec![ken(100), crate::github::Said { who: "me".into(), at: 120, asked: Some("ken".into()), url: None }]), Some("me"), &job, now));
        // Approved, merged, or at another commit than the one it was due on.
        assert!(!still_owed(&PrRaw { changes_asked: Vec::new(), ..said(vec![ken(100)]) }, Some("me"), &job, now));
        assert!(!still_owed(&PrRaw { state: PrState::Merged, ..said(vec![ken(100)]) }, Some("me"), &job, now));
        assert!(!still_owed(&PrRaw { head_oid: Some("ccc".into()), ..said(vec![ken(100)]) }, Some("me"), &job, now));
        assert!(!still_owed(&PrRaw { state: PrState::Draft, ..said(vec![ken(100)]) }, Some("me"), &job, now));
    }

    #[test]
    fn one_not_asked_after_all_is_looked_at_again_later() {
        let mut r = Rerequester::default();
        let b = board(pr(unsaid(NOW - SETTLE_MS)), Status::Idle);
        let jobs = r.due(&b, &ON, NOW);
        // GitHub, read afresh, had it a draft for a moment, or the board then had a session in a turn.
        r.went(&jobs[0], Went::Skipped, NOW);
        assert!(r.due(&b, &ON, NOW + RETRY_MS - 1).is_empty());
        // Still owed later, it's handed out again, however many times that happens.
        for i in 1..=TRIES + 1 {
            let at = NOW + RETRY_MS * i64::from(i);
            let again = r.due(&b, &ON, at);
            assert_eq!(again.len(), 1);
            r.went(&again[0], Went::Skipped, at);
        }
    }

    #[test]
    fn a_request_github_refused_is_tried_again_a_few_times() {
        let mut r = Rerequester::default();
        let b = board(pr(unsaid(NOW - SETTLE_MS)), Status::Idle);
        let mut at = NOW;
        for _ in 0..TRIES {
            let jobs = r.due(&b, &ON, at);
            assert_eq!(jobs.len(), 1);
            r.went(&jobs[0], Went::Failed, at);
            assert!(r.due(&b, &ON, at + RETRY_MS - 1).is_empty());
            at += RETRY_MS;
        }
        assert!(r.due(&b, &ON, at).is_empty());
        // A PR that's gone from the board is forgotten.
        r.due(&Board::default(), &ON, at);
        assert!(r.tried.is_empty());
    }
}
