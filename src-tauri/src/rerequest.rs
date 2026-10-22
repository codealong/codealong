//! Asking a reviewer to look again at a PR of yours once its fixes are pushed, where
//! `[pull_requests] rerequest_after_fixes` is on for its repo: they asked for changes, commits
//! came after their last word, and nobody has said so on it (`TurnKind::ReplyNeeded`, board.rs
//! `turn_of`). GitHub tells a reviewer of a review request, which their Codealong reviews again by
//! itself, but not of a push, so a PR whose agent pushed the fixes and said nothing waited on
//! nobody (specbook#1293 and #1290, 2026-10-22, where each request made by hand was answered by
//! the reviewer's agent within minutes).
//!
//! It's asked once the last push is `SETTLE_MS` old and none of the PR's sessions is in a turn,
//! so a run of fixes is asked about once, and a session about to say so itself can; once for each
//! reviewer and commit, and a request GitHub refuses is tried again a few times, each `RETRY_MS`
//! after the last. Asked, the PR's timeline has the request, and the PR reads as the reviewer's
//! move (`RereviewAsked`) from GitHub's next answer on.

use std::collections::HashMap;

use crate::config::PullRequestsCfg;
use crate::engine::Handle;
use crate::model::{Board, PrItem, Status, TurnKind};

/// How long after the last push a reviewer is asked: a session pushing a run of fixes is done by
/// then, or says so itself.
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
    /// It went, or it's going: the request is out.
    ok: bool,
    tries: u32,
    at: i64,
}

impl Rerequester {
    /// The reviewers to ask again now, on `board`'s PRs, with `[pull_requests]` as config.toml
    /// has it everywhere (`global`). Each is marked asked as it's handed out, so a board that
    /// comes while the request goes doesn't ask again.
    pub fn due(&mut self, board: &Board, global: &PullRequestsCfg, now: i64) -> Vec<Job> {
        self.tried.retain(|(pr, ..), _| board.prs.iter().any(|p| &p.key == pr));
        let mut jobs = Vec::new();
        for pr in board.prs.iter().filter(|p| p.mine && p.state.is_open()) {
            let Some(job) = owed(board, pr, global, now) else { continue };
            let due = match self.tried.get(&job.id()) {
                None => true,
                Some(t) => !t.ok && t.tries < TRIES && now - t.at >= RETRY_MS,
            };
            if due {
                let tries = self.tried.get(&job.id()).map_or(0, |t| t.tries) + 1;
                self.tried.insert(job.id(), Tried { ok: true, tries, at: now });
                jobs.push(job);
            }
        }
        jobs
    }

    /// How a request went: one that didn't is tried again later.
    pub fn went(&mut self, job: &Job, ok: bool, now: i64) {
        if let Some(t) = self.tried.get_mut(&job.id()) {
            t.ok = ok;
            t.at = now;
        }
    }
}

/// The PRs on `board` whose unsaid fixes have settled at `now` (`SETTLE_MS`). Nothing the windows
/// get says so, and it comes with time alone, so the board's hash takes it in (engine.rs
/// `board_hash`), as reconnect.rs `lifted` is: the board that's composed then goes out, and `due`
/// runs on it.
pub fn settled(board: &Board, now: i64) -> Vec<&str> {
    let unsaid = |p: &&PrItem| p.turn.as_ref().is_some_and(|t| t.kind == TurnKind::ReplyNeeded && t.since.is_some_and(|at| now - at >= SETTLE_MS));
    board.prs.iter().filter(unsaid).map(|p| p.key.as_str()).collect()
}

/// The reviewer `pr` owes a request to now, if any: it's yours, its fixes went unsaid
/// (`ReplyNeeded`), its repo has it on, the last push is `SETTLE_MS` old, and none of its
/// sessions is in a turn.
fn owed(board: &Board, pr: &PrItem, global: &PullRequestsCfg, now: i64) -> Option<Job> {
    let turn = pr.turn.as_ref().filter(|t| t.kind == TurnKind::ReplyNeeded)?;
    let head = pr.head_oid.clone()?;
    if !board.repo_settings.pull_requests(global, pr).rerequest_after_fixes || turn.since.is_none_or(|at| now - at < SETTLE_MS) {
        return None;
    }
    let working = pr.sessions.iter().any(|s| board.session(&s.key).is_some_and(|(_, s)| s.status == Status::Working));
    (!working).then(|| Job { pr: pr.key.clone(), repo: pr.repo_slug.clone(), number: pr.number, reviewer: turn.who.clone(), head })
}

/// Asks each job's reviewer, through `gh`, telling `rerequester` how each went, and the engine,
/// which says so in a notice when one didn't.
pub fn send_now(engine: &Handle, rerequester: &std::sync::Mutex<Rerequester>, jobs: Vec<Job>) {
    let mut failed = Vec::new();
    for job in jobs {
        let went = crate::github::request_review(&job.repo, job.number, &job.reviewer);
        rerequester.lock().unwrap_or_else(|e| e.into_inner()).went(&job, went.is_ok(), crate::util::now_ms());
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
            sessions: vec![PrSession { key: A.into(), lane: "/work".into(), pushes: 1 }],
            head_oid: Some("bbb".into()),
            authored_by: None,
            labels: Vec::new(),
            auto_review: None,
            last_review: None,
            stale_approval: None,
            turn,
        }
    }

    fn unsaid(pushed: i64) -> Option<Turn> {
        Some(Turn { kind: TurnKind::ReplyNeeded, who: "ken".into(), since: Some(pushed) })
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
        let b = board(pr(unsaid(pushed)), Status::Idle);
        let jobs = r.due(&b, &ON, NOW);
        assert_eq!(jobs, vec![Job { pr: "o/r#7".into(), repo: "o/r".into(), number: 7, reviewer: "ken".into(), head: "bbb".into() }]);
        // Once for the commit, while it goes and after.
        assert!(r.due(&b, &ON, NOW + 1).is_empty());
        r.went(&jobs[0], true, NOW + 2);
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
        let asked = Some(Turn { kind: TurnKind::RereviewAsked, who: "ken".into(), since: Some(pushed) });
        assert!(Rerequester::default().due(&board(pr(asked), Status::Idle), &ON, NOW).is_empty());
        assert!(Rerequester::default().due(&board(PrItem { mine: false, ..pr(unsaid(pushed)) }, Status::Idle), &ON, NOW).is_empty());
        assert!(Rerequester::default().due(&board(PrItem { state: PrState::Merged, ..pr(unsaid(pushed)) }, Status::Idle), &ON, NOW).is_empty());
    }

    #[test]
    fn a_request_github_refused_is_tried_again_a_few_times() {
        let mut r = Rerequester::default();
        let b = board(pr(unsaid(NOW - SETTLE_MS)), Status::Idle);
        let mut at = NOW;
        for _ in 0..TRIES {
            let jobs = r.due(&b, &ON, at);
            assert_eq!(jobs.len(), 1);
            r.went(&jobs[0], false, at);
            assert!(r.due(&b, &ON, at + RETRY_MS - 1).is_empty());
            at += RETRY_MS;
        }
        assert!(r.due(&b, &ON, at).is_empty());
        // A PR that's gone from the board is forgotten.
        r.due(&Board::default(), &ON, at);
        assert!(r.tried.is_empty());
    }
}
