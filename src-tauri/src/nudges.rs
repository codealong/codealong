//! Telling sessions their PR was approved, when config.toml says to (`[pull_requests]
//! when_approved = "nudge"`). Every board the engine publishes is looked at for PRs of yours
//! that have just become approved, and each session that worked on one (`PrItem::sessions`, as
//! the PRs view lists them) and still runs here is sent a message saying so, as a message of
//! yours from a window is (actions.rs `session-send`: typed into its terminal, or its harness's
//! own way in), on a thread of its own, since a send can take a moment. Left at `nothing`,
//! nothing here sends anything, and the board shows the approval as it does anyway.

use std::collections::HashMap;

use crate::config::WhenApproved;
use crate::engine::Handle;
use crate::model::{Board, Merge, PrItem, PrState};

/// A message due to a session: that this PR of yours was approved.
#[derive(Debug, Clone, PartialEq)]
pub struct Nudge {
    pub session: String,
    pub pr: String,
    pub text: String,
}

/// What each PR on the board stood at when last seen, so an approval is told once, as it
/// happens.
#[derive(Default)]
pub struct Nudger {
    seen: HashMap<String, PrState>,
}

impl Nudger {
    /// The nudges due now: for each PR of yours that the last board showed open and not approved
    /// and this one shows approved, one for each session that worked on it and runs on this Mac.
    /// A PR first seen approved was approved before Codealong looked (before a launch, or while
    /// the setting was off), so it's only noted; approved again after changes were asked for, it's
    /// told again. PRs the board no longer lists are forgotten.
    pub fn due(&mut self, board: &Board) -> Vec<Nudge> {
        let mut out = Vec::new();
        let mut now = HashMap::with_capacity(board.prs.len());
        for pr in &board.prs {
            let key = crate::prlinks::pr_key(&pr.key);
            let was = self.seen.get(&key).copied();
            now.insert(key, pr.state);
            let fresh = was.is_some_and(|s| s.is_open() && s != PrState::Approved) && pr.state == PrState::Approved;
            if !(fresh && pr.mine && board.when_pr_approved == WhenApproved::Nudge) {
                continue;
            }
            let text = words(pr);
            for s in &pr.sessions {
                let runs_here = board.session(&s.key).is_some_and(|(lane, session)| lane.device.is_none() && session.live && !session.archived);
                if runs_here && !out.iter().any(|n: &Nudge| n.session == s.key && n.pr == pr.key) {
                    out.push(Nudge { session: s.key.clone(), pr: pr.key.clone(), text: text.clone() });
                }
            }
        }
        self.seen = now;
        out
    }
}

/// What the session is told: the PR, where it stands, and that it's the session's to take on.
pub fn words(pr: &PrItem) -> String {
    let standing = match (pr.merge, &pr.checks) {
        (Merge::Clean, _) => " GitHub would merge it now.",
        (Merge::Conflicts, _) => " Its branch conflicts with its base.",
        (Merge::Behind, _) => " Its branch is behind its base, which the repo wants brought up to date first.",
        (_, c) if c.failed > 0 => " Some of its checks failed.",
        (_, c) if c.pending > 0 => " Its checks are still running.",
        _ => "",
    };
    format!(
        "Codealong: your PR #{} “{}” was approved on GitHub.{standing} {}\nTake it from here, as this repo does with an approved PR (merging it, say), unless you were told to leave that to someone else.",
        pr.number,
        pr.title.trim(),
        pr.url
    )
}

/// Sends each nudge in turn, as a window's send does, and tells the engine how it went, which
/// says so in a notice when one didn't go.
pub fn send_now(engine: &Handle, nudges: Vec<Nudge>) {
    let mut failed = Vec::new();
    for n in nudges {
        let params = codealong_peer::api::params::Send { key: String::new(), text: n.text, now: false };
        if let Err(e) = crate::actions::local(engine, &crate::actions::From::Here, codealong_peer::api::SESSION_SEND, &n.session, params) {
            let title = engine.read(|b| b.session(&n.session).map(|(_, s)| s.title.clone())).unwrap_or_else(|| n.session.clone());
            failed.push(format!("“{title}” wasn't told {} was approved: {e}", n.pr));
        }
    }
    engine.nudged(if failed.is_empty() { Ok(()) } else { Err(failed.join(" ")) });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Checks, PrSession, Session};

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
            head: "photos".into(),
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
            sessions: sessions.iter().map(|k| PrSession { key: (*k).into(), lane: "/work".into() }).collect(),
        }
    }

    fn live(key: &str) -> Session {
        Session { live: true, ..crate::fixtures::session(key) }
    }

    fn board(when: WhenApproved, prs: Vec<PrItem>, sessions: Vec<Session>) -> Board {
        Board { when_pr_approved: when, prs, lanes: vec![crate::fixtures::lane("/work", sessions)], ..Board::default() }
    }

    const A: &str = "claude:claude:a";
    const B: &str = "claude:claude:b";

    #[test]
    fn tells_the_sessions_that_run_here_once_as_the_pr_becomes_approved() {
        let mut nudger = Nudger::default();
        let sessions = vec![live(A), Session { live: false, ..crate::fixtures::session(B) }];
        // First seen open: noted. Then approved: told, once, and only the session that runs.
        assert!(nudger.due(&board(WhenApproved::Nudge, vec![pr("o/r#1", PrState::Open, &[A, B])], sessions.clone())).is_empty());
        let due = nudger.due(&board(WhenApproved::Nudge, vec![pr("o/r#1", PrState::Approved, &[A, B])], sessions.clone()));
        assert_eq!(due.len(), 1);
        assert_eq!((due[0].session.as_str(), due[0].pr.as_str()), (A, "o/r#1"));
        assert!(due[0].text.starts_with("Codealong: your PR #1 “Let the agent add photos” was approved on GitHub. https://github.com/o/r/pull/1\n"), "{}", due[0].text);
        assert!(nudger.due(&board(WhenApproved::Nudge, vec![pr("o/r#1", PrState::Approved, &[A, B])], sessions.clone())).is_empty(), "told once");
        // Changes asked for, then approved again: told again.
        assert!(nudger.due(&board(WhenApproved::Nudge, vec![pr("o/r#1", PrState::Changes, &[A, B])], sessions.clone())).is_empty());
        assert_eq!(nudger.due(&board(WhenApproved::Nudge, vec![pr("o/r#1", PrState::Approved, &[A, B])], sessions)).len(), 1);
    }

    #[test]
    fn a_pr_first_seen_approved_and_one_that_isnt_yours_are_left_alone() {
        let mut nudger = Nudger::default();
        assert!(nudger.due(&board(WhenApproved::Nudge, vec![pr("o/r#1", PrState::Approved, &[A])], vec![live(A)])).is_empty());
        // Gone from the board and back approved: first sight again.
        assert!(nudger.due(&board(WhenApproved::Nudge, vec![], vec![live(A)])).is_empty());
        assert!(nudger.due(&board(WhenApproved::Nudge, vec![pr("o/r#1", PrState::Approved, &[A])], vec![live(A)])).is_empty());
        let mut nudger = Nudger::default();
        let theirs = PrItem { mine: false, ..pr("o/r#2", PrState::Open, &[A]) };
        assert!(nudger.due(&board(WhenApproved::Nudge, vec![theirs.clone()], vec![live(A)])).is_empty());
        assert!(nudger.due(&board(WhenApproved::Nudge, vec![PrItem { state: PrState::Approved, ..theirs }], vec![live(A)])).is_empty());
    }

    #[test]
    fn nothing_is_sent_unless_asked_for_but_the_states_are_still_kept() {
        let mut nudger = Nudger::default();
        assert!(nudger.due(&board(WhenApproved::Nothing, vec![pr("o/r#1", PrState::Open, &[A])], vec![live(A)])).is_empty());
        assert!(nudger.due(&board(WhenApproved::Nothing, vec![pr("o/r#1", PrState::Approved, &[A])], vec![live(A)])).is_empty());
        // Turned on after the approval: nothing to tell; the next approval is told.
        assert!(nudger.due(&board(WhenApproved::Nudge, vec![pr("o/r#1", PrState::Approved, &[A])], vec![live(A)])).is_empty());
        assert!(nudger.due(&board(WhenApproved::Nudge, vec![pr("o/r#1", PrState::Open, &[A])], vec![live(A)])).is_empty());
        assert_eq!(nudger.due(&board(WhenApproved::Nudge, vec![pr("o/r#1", PrState::Approved, &[A])], vec![live(A)])).len(), 1);
    }

    #[test]
    fn a_session_on_two_prs_is_told_of_each_and_one_on_another_device_not_at_all() {

        use crate::model::DeviceTag;
        let mut nudger = Nudger::default();
        let two = |state| vec![pr("o/r#1", state, &[A]), pr("o/r#2", state, &[A])];
        nudger.due(&board(WhenApproved::Nudge, two(PrState::Open), vec![live(A)]));
        let due = nudger.due(&board(WhenApproved::Nudge, two(PrState::Approved), vec![live(A)]));
        assert_eq!(due.iter().map(|n| (n.session.as_str(), n.pr.as_str())).collect::<Vec<_>>(), vec![(A, "o/r#1"), (A, "o/r#2")]);
        let mut nudger = Nudger::default();
        let elsewhere = |state| {
            let mut b = board(WhenApproved::Nudge, vec![pr("o/r#1", state, &[A])], vec![live(A)]);
            b.lanes[0].device = Some(DeviceTag { name: "Studio".into(), short: "st".into(), color: "blue".into(), online: true, last_seen: None });

            b
        };
        nudger.due(&elsewhere(PrState::Open));
        assert!(nudger.due(&elsewhere(PrState::Approved)).is_empty());
    }

    #[test]
    fn says_where_the_pr_stands() {
        let standing = |merge, failed, pending| {
            let p = PrItem { merge, checks: Checks { failed, pending, ..Checks::default() }, ..pr("o/r#1", PrState::Approved, &[]) };
            words(&p).lines().next().unwrap().trim_start_matches("Codealong: your PR #1 “Let the agent add photos” was approved on GitHub.").trim_end_matches(" https://github.com/o/r/pull/1").to_string()
        };
        assert_eq!(standing(Merge::Clean, 0, 0), " GitHub would merge it now.");
        assert_eq!(standing(Merge::Conflicts, 0, 2), " Its branch conflicts with its base.");
        assert_eq!(standing(Merge::Blocked, 1, 2), " Some of its checks failed.");
        assert_eq!(standing(Merge::Unknown, 0, 2), " Its checks are still running.");
        assert_eq!(standing(Merge::Unknown, 0, 0), "");
    }
}
