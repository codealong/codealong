//! Telling sessions what happens on their PR, for the PRs it's turned on for. It's off for every
//! PR until someone turns it on for that one: you, from the PR's card or menu, or an agent with
//! `codealong pr nudge` (both write pr-links.json, prlinks.rs), and a PR says so while it's open
//! (`PrItem::nudge`). Every board the engine publishes is looked at for such PRs with something
//! new on GitHub: approved, changes asked for, reviewed or commented on by someone else (read from
//! the PR's detail once its `updatedAt` moves), merged or closed, or with checks that just failed.
//! Each session that worked on the PR (`PrItem::sessions`, as the PRs view lists them) and still
//! runs here is sent one message saying what happened, as a message of yours from a window is
//! (actions.rs `session-send`: typed into its terminal, or its harness's own way in), on a thread
//! of its own, since reading GitHub and a send take a moment. What you did to the PR yourself,
//! which is what its agent did through `gh` too, is never told back.

use std::collections::HashMap;

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

/// What's new on a PR since the last board, to tell its sessions.
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

/// A PR with something new to tell, and the sessions to tell.
#[derive(Debug, Clone, PartialEq)]
pub struct Job {
    pub pr: PrItem,
    pub why: Why,
    pub sessions: Vec<String>,
}

/// What each PR on the board stood at when last seen, so each update is told once, as it happens.
#[derive(Default)]
pub struct Nudger {
    seen: HashMap<String, Seen>,
}

impl Nudger {
    /// The jobs due now: for each PR with its nudge on that the last board showed otherwise, one
    /// with what's new and the sessions that worked on it and run on this Mac. A PR first seen is
    /// only noted, so turning the nudge on tells of what happens from then on (and of an approval
    /// it has already); a PR the board no longer lists is forgotten.
    pub fn due(&mut self, board: &Board) -> Vec<Job> {
        let mut out = Vec::new();
        let mut now = HashMap::with_capacity(board.prs.len());
        for pr in &board.prs {
            let key = crate::prlinks::pr_key(&pr.key);
            let seen = Seen { on: pr.nudge, updated_at: pr.updated_at, state: pr.state, failed: pr.checks.failed > 0 };
            let was = self.seen.get(&key).copied();
            now.insert(key, seen);
            let Some(was) = was else { continue };
            // The board says the nudge only while a PR is open: one that just ended is still
            // told of that.
            let ended = was.on && was.state.is_open() && !pr.state.is_open();
            if !(pr.nudge || ended) {
                continue;
            }
            let why = Why {
                since: (was.on && pr.updated_at > was.updated_at).then_some(was.updated_at),
                approved: pr.state == PrState::Approved && (was.state != PrState::Approved || !was.on),
                changes: was.on && pr.state == PrState::Changes && was.state != PrState::Changes,
                ended: ended.then_some(pr.state),
                failed: if was.on && seen.failed && !was.failed { pr.checks.failing.clone() } else { Vec::new() },
            };
            if why == Why::default() {
                continue;
            }
            let runs_here = |k: &str| board.session(k).is_some_and(|(lane, s)| lane.device.is_none() && s.live && !s.archived);
            let mut sessions: Vec<String> = Vec::new();
            for s in &pr.sessions {
                if runs_here(&s.key) && !sessions.contains(&s.key) {
                    sessions.push(s.key.clone());
                }
            }
            if !sessions.is_empty() {
                out.push(Job { pr: pr.clone(), why, sessions });
            }
        }
        self.seen = now;
        out
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

/// What the session is told: the PR, each new thing on it, where it stands, and that it's the
/// session's to take on. None when nothing new was by someone else (the PR's own agent pushed,
/// say). `detail` is the PR as GitHub has it now, read when `why.since` says to; `login` is who
/// `gh` is signed in as, whose reviews aren't news.
pub fn words(pr: &PrItem, why: &Why, detail: Option<&PrDetail>, login: Option<&str>) -> Option<String> {
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
        "Nothing more is wanted on it, unless you were told otherwise."
    } else if pr.state == PrState::Approved && !changes_said && !why.changes {
        "Do what this repo does next with an approved PR, such as merging it, unless you were told to leave that to someone else."
    } else {
        "Take it from there, unless you were told to leave it to someone else."
    };
    let list: Vec<String> = items.iter().map(|i| format!("- {i}")).collect();
    Some(format!("Codealong: on {whose} #{} “{}” ({}):\n{}\n{}{next}", pr.number, pr.title.trim(), pr.url, list.join("\n"), standing))
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

/// Does each job in turn: reads the PR's detail from GitHub where its `updatedAt` moved, works
/// out the message, and sends it to each session as a window's send does; then tells the engine
/// how it went, which says so in a notice when something didn't go.
pub fn send_now(engine: &Handle, jobs: Vec<Job>) {
    let login = engine.read(|b| b.github_login.clone());
    let mut failed = Vec::new();
    for job in jobs {
        let detail = match job.why.since {
            Some(_) => match crate::github::detail(&job.pr.repo_slug, job.pr.number) {
                Ok(d) => Some(d),
                Err(e) => {
                    failed.push(format!("What's new on {} couldn't be read: {e}", job.pr.key));
                    None
                }
            },
            None => None,
        };
        let Some(text) = words(&job.pr, &job.why, detail.as_ref(), login.as_deref()) else { continue };
        for key in &job.sessions {
            let params = codealong_peer::api::params::Send { key: String::new(), text: text.clone(), now: false };
            if let Err(e) = crate::actions::local(engine, &crate::actions::From::Here, codealong_peer::api::SESSION_SEND, key, params) {
                let title = engine.read(|b| b.session(key).map(|(_, s)| s.title.clone())).unwrap_or_else(|| key.to_string());
                failed.push(format!("“{title}” wasn't told what's new on {}: {e}", job.pr.key));
            }
        }
    }
    engine.nudged(if failed.is_empty() { Ok(()) } else { Err(failed.join(" ")) });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::github::{PrComment, PrReview};
    use crate::model::{Checks, PrSession, Session};

    /// An open PR of yours with its nudge on, last updated at `UPDATED`.
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
        PrDetail { body_html: String::new(), mergeable: None, commits: 1, reviews, requested: vec![], comments, files: vec![], more_files: false }
    }

    #[test]
    fn an_update_on_a_pr_its_on_for_is_a_job_for_the_sessions_that_run_here_once() {
        let mut nudger = Nudger::default();
        let sessions = vec![live(A), Session { live: false, ..crate::fixtures::session(B) }];
        // First seen: noted. Then updated on GitHub: a job to read what's new since, for the
        // session that runs.
        assert!(nudger.due(&board(vec![pr("o/r#1", PrState::Open, &[A, B])], sessions.clone())).is_empty());
        let moved = PrItem { updated_at: UPDATED + 5, ..pr("o/r#1", PrState::Open, &[A, B]) };
        let jobs = nudger.due(&board(vec![moved.clone()], sessions.clone()));
        assert_eq!(jobs.len(), 1);
        assert_eq!((jobs[0].sessions.clone(), jobs[0].why.clone()), (vec![A.to_string()], Why { since: Some(UPDATED), ..Why::default() }));
        assert!(nudger.due(&board(vec![moved.clone()], sessions.clone())).is_empty(), "told once");
        // Approved, with the update that brought it; then changes asked for; then merged.
        let approved = PrItem { state: PrState::Approved, updated_at: UPDATED + 9, ..moved.clone() };
        assert_eq!(nudger.due(&board(vec![approved.clone()], sessions.clone()))[0].why, Why { since: Some(UPDATED + 5), approved: true, ..Why::default() });
        let changes = PrItem { state: PrState::Changes, ..approved.clone() };
        assert_eq!(nudger.due(&board(vec![changes], sessions.clone()))[0].why, Why { changes: true, ..Why::default() });
        let merged = off(PrItem { state: PrState::Merged, ..approved });
        assert_eq!(nudger.due(&board(vec![merged.clone()], sessions.clone()))[0].why, Why { ended: Some(PrState::Merged), ..Why::default() });
        assert!(nudger.due(&board(vec![merged], sessions)).is_empty(), "and nothing after");
    }

    #[test]
    fn checks_failing_is_told_once_and_the_approval_it_had_when_turned_on() {
        let mut nudger = Nudger::default();
        let failing = |n: u32| PrItem { checks: Checks { failed: n, failing: vec!["lint".into()], ..Checks::default() }, ..pr("o/r#1", PrState::Open, &[A]) };
        nudger.due(&board(vec![failing(0)], vec![live(A)]));
        assert_eq!(nudger.due(&board(vec![failing(1)], vec![live(A)]))[0].why, Why { failed: vec!["lint".into()], ..Why::default() });
        assert!(nudger.due(&board(vec![failing(2)], vec![live(A)])).is_empty(), "still failing is no news");
        // Off for a PR that's approved; turned on: told of the approval it has, and not of what
        // happened while it was off.
        let mut nudger = Nudger::default();
        nudger.due(&board(vec![off(pr("o/r#2", PrState::Open, &[A]))], vec![live(A)]));
        assert!(nudger.due(&board(vec![off(PrItem { updated_at: UPDATED + 1, ..pr("o/r#2", PrState::Approved, &[A]) })], vec![live(A)])).is_empty());
        let on = PrItem { updated_at: UPDATED + 2, ..pr("o/r#2", PrState::Approved, &[A]) };
        assert_eq!(nudger.due(&board(vec![on], vec![live(A)]))[0].why, Why { approved: true, ..Why::default() });
    }

    #[test]
    fn a_pr_first_seen_or_on_another_device_tells_nobody() {
        use crate::model::DeviceTag;
        let mut nudger = Nudger::default();
        assert!(nudger.due(&board(vec![pr("o/r#1", PrState::Approved, &[A])], vec![live(A)])).is_empty());
        // Gone from the board and back, changed: first sight again.
        assert!(nudger.due(&board(vec![], vec![live(A)])).is_empty());
        assert!(nudger.due(&board(vec![PrItem { updated_at: UPDATED + 1, ..pr("o/r#1", PrState::Approved, &[A]) }], vec![live(A)])).is_empty());
        let mut nudger = Nudger::default();
        let elsewhere = |at: i64| {
            let mut b = board(vec![PrItem { updated_at: at, ..pr("o/r#1", PrState::Open, &[A]) }], vec![live(A)]);
            b.lanes[0].device = Some(DeviceTag { name: "Studio".into(), short: "st".into(), color: "blue".into(), online: true, last_seen: None });
            b
        };
        nudger.due(&elsewhere(UPDATED));
        assert!(nudger.due(&elsewhere(UPDATED + 1)).is_empty());
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
        let text = words(&p, &why, Some(&d), Some("me")).unwrap();
        assert_eq!(
            text,
            "Codealong: on your PR #1 “Let the agent add photos” (https://github.com/o/r/pull/1):\n- ana asked for changes: “Please add a test.”\n- ana commented on src/a.ts: “This can be a map.”\nTake it from there, unless you were told to leave it to someone else."
        );
        // Only your own push moved it: nothing to say.
        assert_eq!(words(&p, &Why { since: Some(UPDATED + 30), ..Why::default() }, Some(&d), Some("me")), None);
        // The detail couldn't be read: what the board knows is said.
        assert!(words(&p, &why, None, Some("me")).unwrap().contains("- Changes were asked for.\n"));
    }

    #[test]
    fn says_an_approval_a_merge_and_failing_checks() {
        let approved = PrItem { merge: Merge::Clean, ..pr("o/r#1", PrState::Approved, &[A]) };
        let d = detail(vec![PrReview { author: "ken".into(), state: "APPROVED".into(), at: Some(UPDATED + 1) }], vec![]);
        let text = words(&approved, &Why { since: Some(UPDATED), approved: true, ..Why::default() }, Some(&d), Some("me")).unwrap();
        assert_eq!(text, "Codealong: on your PR #1 “Let the agent add photos” (https://github.com/o/r/pull/1):\n- ken approved it.\nIt's ready to merge. Do what this repo does next with an approved PR, such as merging it, unless you were told to leave that to someone else.");
        // Turned on for one approved already, with nothing read: said in the board's words.
        assert!(words(&approved, &Why { approved: true, ..Why::default() }, None, None).unwrap().contains("- It was approved.\n"));
        let merged = off(PrItem { state: PrState::Merged, mine: false, ..pr("o/r#1", PrState::Open, &[A]) });
        assert_eq!(
            words(&merged, &Why { ended: Some(PrState::Merged), ..Why::default() }, None, None).unwrap(),
            "Codealong: on PR #1 “Let the agent add photos” (https://github.com/o/r/pull/1):\n- It was merged.\nNothing more is wanted on it, unless you were told otherwise."
        );
        let failing = PrItem { checks: Checks { failed: 2, failing: vec!["lint".into(), "test".into()], ..Checks::default() }, ..pr("o/r#1", PrState::Open, &[A]) };
        let text = words(&failing, &Why { failed: vec!["lint".into(), "test".into()], ..Why::default() }, None, None).unwrap();
        assert!(text.contains("- Its checks failed: lint, test.\nSome of its checks failed. Take it from there"), "{text}");
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
