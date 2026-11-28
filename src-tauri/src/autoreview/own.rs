//! Reviews of the pull requests your sessions open, shared among your machines as the reviews of
//! PRs that ask for yours are (`reviewer_of`, shared.rs): one machine of yours reviews each
//! commit, and the review goes back to the machine that has the PR's primary session, which keeps
//! it and hands it to that session as it would one written there.
//!
//! The machine with the PR's primary session owns it: its `[reviews.own]` settings say whether
//! the PR is reviewed at all (a draft too, or a PR whose session is flagged) and whether the
//! review goes back to the session. Its card says so (`SharedReview::owner`). The machines that
//! would review it for that one (`would_for`: their own reviews are on and they have a checkout of
//! its repo) and the owner itself go in line for its commits by the same rules as a PR that asks
//! for your review, so the first choice for automations your machines share reviews them all while
//! it's around. A machine that reviews a commit for the owner keeps the review there, as a review
//! of one of its own PRs is kept, and puts it on its card (`handed`); the owner keeps it as a round
//! of its own once it sees it there (`import`), which isn't one of the owner's as your machines
//! weigh them (`Round::from`), so the PR stays with the machine that reviewed it. The card is kept
//! on disk with the rest of that machine's feed, so a review done while the owner was asleep
//! still reaches it.
//!
//! Machines on builds from before 2026-11-21 say nothing of owning a PR, and review their own PRs
//! themselves, as before; so does a machine that shares no automations.

use super::{claim_state, clean, has_checkout, origin, parse_reply, short, verdict_of, Part, Round, Store, Tracked, NOTHING_TO_RAISE};
use crate::config::{OwnReviewsCfg, ReviewsCfg};
use crate::github::Verdict;
use crate::model::{Harness, Lane, PartState, PrItem, PrState, ReviewStage, Session};
use crate::prlinks::pr_key;
use crate::shared::{Machine, Machines};
use codealong_peer::pr::{AutoReview, ClaimState, HandedReview, SharedReview};
use std::collections::HashMap;

/// The review of a PR of yours as its primary session reads it: who wrote it and of which
/// commit, its text, then each comment with its file and line.
pub fn review_file(pr: &PrItem, head: &str, agent: &str, review: &str) -> String {
    let (summary, comments) = parse_reply(review);
    let mut md = format!("# Review of #{} “{}” at {}, by {agent}\n\n{}\n", pr.number, pr.title.trim(), short(head), summary.trim());
    if !comments.is_empty() {
        md += "\n## On particular lines\n\n";
        for c in &comments {
            md += &format!("- `{}` line {}: {}\n", c.path, c.line, c.body.trim().replace('\n', "\n  "));
        }
    }
    md
}

/// What a PR of yours' primary session is sent with its review, kept at `file`
/// (`review_file`): who wrote it and of which commit, and to weigh it as another agent's.
pub fn back_message(pr: &PrItem, head: &str, agent: &str, login: &str, file: &str, comments: usize) -> String {
    let lines = match comments {
        0 => String::new(),
        1 => ", with a comment on a line".into(),
        n => format!(", with {n} comments on lines"),
    };
    format!(
        "{agent} reviewed your pull request #{} “{}” ({}) at {}, as {login} has another agent review the pull requests their sessions open. Its review is in {file}{lines}. It's that agent's review, not {login}'s word: fix what holds up, and say which findings you leave and why. {}",
        pr.number,
        pr.title.trim(),
        pr.url,
        short(head),
        crate::nudges::NOT_A_MERGE
    )
}

/// Why a review of a PR of yours that only left comments (`Verdict: comment`) isn't handed back
/// automatically while `[reviews.own] send_comments` is off.
pub(super) const COMMENTS_ONLY: &str = "It only left comments (no changes requested), so it isn't sent.";

/// Whether an own-PR review should be handed back to the PR's primary session automatically:
/// never when it would approve (`clean`), never for `Verdict: comment` unless `cfg.send_comments`
/// is on, and always for `Verdict: request changes` (or a review with no `Verdict:` line).
pub(super) fn should_send(review: &str, cfg: &OwnReviewsCfg) -> bool {
    if clean(review) {
        return false;
    }
    if verdict_of(review) == Some(Verdict::Comment) && !cfg.send_comments {
        return false;
    }
    true
}

/// The note kept on an own-PR round when its review isn't handed back automatically.
pub(super) fn not_sent_note(review: &str, cfg: &OwnReviewsCfg) -> Option<String> {
    if clean(review) {
        Some(NOTHING_TO_RAISE.to_string())
    } else if verdict_of(review) == Some(Verdict::Comment) && !cfg.send_comments {
        Some(COMMENTS_ONLY.to_string())
    } else {
        None
    }
}

/// Whether a PR's state allows finishing and sending an in-flight own-PR review or starting a
/// review by hand (`codealong pr review` / Review with…): open (including draft) or merged, never
/// closed without merging. Automatic reviews still start only while open (`wants_own`).
pub(super) fn reviewable_state(state: PrState) -> bool {
    state.is_open() || state == PrState::Merged
}

/// The note keeping a finished review from being sent or posted because the PR ended, or None when
/// `post` should go on as usual: an own-PR review started while the PR was open still finishes and
/// is handed back after the PR merges (not when closed without merging); a manual review of
/// someone else's merged PR is kept here rather than posted on GitHub.
pub(super) fn closed_note(pr: &PrItem, by_hand: bool) -> Option<String> {
    if pr.state.is_open() || (pr.mine && pr.state == PrState::Merged) {
        return None;
    }
    if !pr.mine && by_hand && pr.state == PrState::Merged {
        return Some("Not posted: the PR is merged, so the review is kept here.".into());
    }
    let not = if pr.mine { "Not sent" } else { "Not posted" };
    Some(format!("{not}: the PR was closed before the review was done."))
}

/// The top-level session a review of a PR of yours nests under (`nest_own`): `start` itself when
/// it has no helper parent, else walking up `Helper::parent` through local, non-visitor,
/// unarchived sessions so a review of a worker helper's PR sits beside the helper under its
/// top-level session rather than two levels down. Stops at the last reachable ancestor or a cycle.
pub(super) fn nest_parent<'a>(lanes: &'a [Lane], start: &'a Session) -> &'a str {
    let find = |k: &str| lanes.iter().filter(|l| l.device.is_none()).flat_map(|l| &l.sessions).find(|s| s.key == k && s.visitor.is_none() && !s.archived);
    let mut cur = start;
    let mut seen: Vec<&str> = vec![cur.key.as_str()];
    while let Some(parent_key) = cur.helper.as_ref().and_then(|h| h.parent.as_deref()) {
        if seen.contains(&parent_key) {
            break;
        }
        let Some(parent) = find(parent_key) else { break };
        seen.push(parent.key.as_str());
        cur = parent;
    }
    cur.key.as_str()
}

/// Puts each review of a PR of yours under the top-level session of the session it goes back to
/// (`origin`, walking up `Helper::parent` via `nest_parent`), as a helper of that session as well
/// as of the PR (`Helper::parent`): Sessions lists it under that session so a worker helper's
/// review stays visible rather than nested two levels down, whose panel lists it with its other
/// helpers, and Mark as done on that session marks it done too. A review of a PR that waits on
/// your review has no session of yours to go under, and stays the PR's alone.
pub fn nest_own(lanes: &mut [Lane], prs: &[PrItem]) {
    let back_to: HashMap<String, String> = prs.iter().filter(|p| p.mine).filter_map(|p| origin(lanes, p).map(|(_, s)| (pr_key(&p.key), nest_parent(lanes, s).to_string()))).collect();
    if back_to.is_empty() {
        return;
    }
    for s in lanes.iter_mut().flat_map(|l| l.sessions.iter_mut()) {
        let key = s.key.clone();
        let Some(h) = s.helper.as_mut().filter(|h| h.parent.is_none()) else { continue };
        if let Some(parent) = h.pr.as_deref().and_then(|pr| back_to.get(pr)).filter(|p| **p != key) {
            h.parent = Some(parent.clone());
        }
    }
}

/// Whether this machine keeps claiming ownership of PR `pr` on its card (`SharedReview::owner`):
/// while it wants a review (`owned`), or after the PR merges with `[reviews.own]` on and its
/// primary session still here, so another machine that finished reviewing it keeps publishing its
/// `handed` review for this machine to import and send (neither machine starts a new round, since
/// `would` and `would_for` stay open-only).
pub(super) fn keeps_owner(owned: bool, cfg: &ReviewsCfg, blocked: bool, lanes: &[Lane], pr: &PrItem) -> bool {
    owned || (pr.mine && pr.state == PrState::Merged && cfg.own.auto && !blocked && origin(lanes, pr).is_some())
}

/// Whether an automatic own-PR round that failed because its session showed up late can still be
/// taken up after the PR merged (`Reviewer::due` -> `Tracked::take_up`), without starting anything
/// new: both on the machine that owns the PR and on a peer machine reviewing it for the owner.
pub(super) fn recover_merged(cfg: &ReviewsCfg, lanes: &[Lane], machines: &Machines, pr: &PrItem) -> bool {
    pr.mine && pr.state == PrState::Merged && cfg.own.auto && (origin(lanes, pr).is_some() || owner(pr, machines).is_some())
}

/// Whether automatic own-PR review rounds for a PR have hit `max_rounds` (0 = unlimited).
/// Counts completed automatic rounds (`!by_hand`); manual `Review with…` rounds always bypass
/// this cap and don't count against it.
pub(super) fn round_capped(t: Option<&Tracked>, max_rounds: u32) -> bool {
    if max_rounds == 0 {
        return false;
    }
    let done = t.map_or(0, |t| t.rounds.iter().filter(|r| !r.by_hand && claim_state(r.stage, false) == ClaimState::Done).count());
    done >= max_rounds as usize
}

/// Which machine reviews a PR of yours.
#[derive(Debug, PartialEq)]
enum Reviewer<'a> {
    Here,
    /// Another, with its latest round of the PR when it has one.
    Elsewhere(&'a Machine, Option<&'a AutoReview>),
    /// None: no machine owns it, or none that would review it is around.
    Nobody,
}

/// The machine of yours that owns PR `pr` of yours, by its card, when another does: its card is
/// kept while it's asleep, so it still owns it then.
pub(super) fn owner<'a>(pr: &PrItem, machines: &'a Machines) -> Option<&'a Machine> {
    machines.others.iter().find(|m| m.pr(&pr.key).and_then(|c| c.shared_review.as_ref()).is_some_and(|s| s.owner))
}

/// Which machine reviews PR `pr` of yours (`reviewer_of`): `mine` is what this machine's reviews
/// of it come to, `owned` whether this machine owns it.
fn reviewer<'a>(pr: &PrItem, mine: &SharedReview, owned: bool, machines: &'a Machines, now: i64) -> Reviewer<'a> {
    if !owned && owner(pr, machines).is_none() {
        return Reviewer::Nobody;
    }
    match super::reviewer_of(pr, mine, machines, now) {
        Some((m, r)) => Reviewer::Elsewhere(m, r),
        None if mine.would => Reviewer::Here,
        None => Reviewer::Nobody,
    }
}

/// Whether this machine would review PR `pr` of yours for the machine that owns it: its own
/// reviews are on, it has a checkout of the PR's repo, and the PR is open at a commit it knows.
/// Not where the PR's primary session is here: this machine owns it then.
pub(super) fn would_for(cfg: &ReviewsCfg, lanes: &[Lane], pr: &PrItem) -> bool {
    pr.mine && cfg.own.auto && pr.state.is_open() && pr.head_oid.is_some() && origin(lanes, pr).is_none() && has_checkout(lanes, pr)
}

/// Whether this machine reviews PR `pr` of yours for the machine that owns it now, as the other
/// machines' cards and its own (`PrItem::shared_review`) say.
pub(super) fn reviews_here(pr: &PrItem, lanes: &[Lane], machines: &Machines, now: i64) -> bool {
    pr.mine && origin(lanes, pr).is_none() && reviewer(pr, &pr.shared_review, false, machines, now) == Reviewer::Here
}

/// The harness that wrote PR `pr` of yours, by the primary session the machine that owns it names
/// on its card (`PrCard::primary`, a key that starts with its harness): what a review here keeps
/// off, as it keeps off the harness of a primary session here (`pr_harness`).
pub(super) fn harness(pr: &PrItem, machines: &Machines) -> Option<Harness> {
    let key = owner(pr, machines)?.pr(&pr.key)?.primary.as_deref()?;
    crate::harness::named(key.split(':').next()?).map(|a| a.harness())
}

/// Another machine of yours that reviews PR `pr` of yours, at commit `head`, with its latest round
/// of the PR, for this machine's card to show: until a review of `head` is here, written here or
/// kept from another machine (`import`), which says more.
pub(super) fn elsewhere<'a>(t: Option<&Tracked>, pr: &PrItem, head: &str, owned: bool, machines: &'a Machines, now: i64) -> Option<(&'a Machine, Option<&'a AutoReview>)> {
    if t.is_some_and(|t| t.rounds.iter().any(|r| r.head == head && (r.from.is_some() || claim_state(r.stage, false) == ClaimState::Done))) {
        return None;
    }
    match reviewer(pr, &pr.shared_review, owned, machines, now) {
        Reviewer::Elsewhere(m, r) => Some((m, r)),
        Reviewer::Here | Reviewer::Nobody => None,
    }
}

/// The review this machine wrote of a PR of yours that another machine owns, for that machine to
/// keep (`import`): its latest round's that got done with a review.
pub(super) fn handed(t: Option<&Tracked>) -> Option<HandedReview> {
    let written = |p: &&Part| p.state == PartState::Done && p.review.is_some();
    let (r, p) = t?.rounds.iter().rev().filter(|r| r.from.is_none() && r.stage == ReviewStage::Kept).find_map(|r| Some((r, r.parts.iter().find(written)?)))?;
    Some(HandedReview { head: r.head.clone(), agent: p.label(), review: p.review.clone()?, at: r.ended_at.unwrap_or(r.at) })
}

/// Keeps here the reviews other machines of yours wrote of PR `pr` of yours (`key`), which this
/// machine owns, from their cards (`handed`): each as a round of its own, once for each commit,
/// and not over a review of that commit that got done here. A review of the commit the PR is at
/// is its latest round, whatever ended here meanwhile (a round of it that gave way: their clocks
/// differ, and one done while the machines couldn't reach each other is seen late); an earlier
/// commit's goes in its place among the rounds by when it was done, after any other of that
/// commit's. Not while a round here is under way, which may give way to it first (`ELSEWHERE`).
/// True when one was kept.
pub(super) fn import(store: &mut Store, key: &str, pr: &PrItem, machines: &Machines, own_cfg: &OwnReviewsCfg) -> bool {
    let reviewing = |t: &Tracked| t.rounds.last().is_some_and(|r| r.stage == ReviewStage::Reviewing);
    if store.prs.get(key).is_some_and(reviewing) {
        return false;
    }
    let mut kept = false;
    for (m, h) in machines.others.iter().filter_map(|m| Some((m, m.pr(&pr.key)?.shared_review.as_ref()?.handed.as_ref()?))) {
        let had = |t: &Tracked| t.rounds.iter().any(|r| r.head == h.head && (r.from.is_some() || claim_state(r.stage, false) == ClaimState::Done));
        if store.prs.get(key).is_some_and(had) {
            continue;
        }
        let part = Part { state: PartState::Done, review: Some(h.review.clone()), agent: Some(format!("{} on {}", h.agent, m.name)), verified: true, ..Part::asked(&h.agent, None, None, h.at) };
        // Said as a review written here that found nothing to raise says it (`post`).
        let note = not_sent_note(&h.review, own_cfg);
        let round = Round { parts: vec![part], from: Some(m.name.clone()), note, ..Round::new(&h.head, ReviewStage::Kept, h.at) };
        let t = store.prs.entry(key.to_string()).or_default();
        let at = match pr.head_oid.as_deref() == Some(h.head.as_str()) {
            true => t.rounds.len(),
            false => t.rounds.iter().position(|r| r.at > h.at && r.head != h.head).unwrap_or(t.rounds.len()),
        };
        t.rounds.insert(at, round);
        kept = true;
    }
    kept
}

/// What a round of this machine's says of a PR of yours that another machine owns: where its
/// review goes, once it's kept.
pub(super) fn note(r: &Round, pr: &PrItem, machines: &Machines) -> Option<String> {
    if r.note.is_some() || r.stage != ReviewStage::Kept {
        return r.note.clone();
    }
    Some(match owner(pr, machines) {
        Some(m) => format!("It goes back to {}, where the PR's primary session is.", m.name),
        None => "It goes back to the machine where the PR's primary session is.".into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_send_gates_comment_only_reviews_unless_opted_in() {
        let def = OwnReviewsCfg::default();
        assert!(!should_send("Looks good.\nVerdict: approve", &def));
        assert!(!should_send("Minor nit.\nVerdict: comment", &def));
        assert_eq!(not_sent_note("Minor nit.\nVerdict: comment", &def).as_deref(), Some(COMMENTS_ONLY));
        assert!(should_send("Real bug.\nVerdict: request changes", &def));
        assert!(should_send("Legacy review without a verdict line.", &def));
        let with_comments = OwnReviewsCfg { send_comments: true, ..def };
        assert!(should_send("Minor nit.\nVerdict: comment", &with_comments));
        assert_eq!(not_sent_note("Minor nit.\nVerdict: comment", &with_comments), None);
    }

    #[test]
    fn round_capped_counts_only_completed_automatic_rounds() {
        let mut t = Tracked::default();
        t.rounds.push(Round::new("111", ReviewStage::Kept, 1));
        t.rounds.push(Round { by_hand: true, ..Round::new("222", ReviewStage::Kept, 2) });
        t.rounds.push(Round::new("333", ReviewStage::Failed, 3));
        assert!(!round_capped(Some(&t), 2), "only 1 completed automatic round");
        t.rounds.push(Round::new("444", ReviewStage::Kept, 4));
        assert!(round_capped(Some(&t), 2), "2 completed automatic rounds hit cap");
        assert!(!round_capped(Some(&t), 0), "0 means unlimited");
    }

    #[test]
    fn merged_prs_finish_own_reviews_and_keep_manual_ones_while_closed_ones_stop() {
        assert!(reviewable_state(PrState::Open));
        assert!(reviewable_state(PrState::Draft));
        assert!(reviewable_state(PrState::Merged));
        assert!(!reviewable_state(PrState::Closed));

        let pr_with = |mine: bool, state: PrState| PrItem {
            key: "northwind/app#7".into(),
            repo_slug: "northwind/app".into(),
            repo: "app".into(),
            project: "Northwind".into(),
            number: 7,
            title: "Fix queue".into(),
            url: "https://github.com/northwind/app/pull/7".into(),
            state,
            author: Some("gwen".into()),
            mine,
            review_requested: !mine,
            requested_by: None,
            requested_at: None,
            reviewers_asked: Vec::new(),
            snoozed_until: None,
            nudge: false,
            nudge_by_default: false,
            watched: false,
            review_first: false,
            waits_to_merge: false,
            taken_over: false,
            primary: None,
            primary_by: None,
            head: "fix-queue".into(),
            base: Some("main".into()),
            checks: Default::default(),
            merge: Default::default(),
            additions: 1,
            deletions: 0,
            changed_files: 1,
            created_at: 0,
            updated_at: 0,
            merged_at: None,
            closed_at: None,
            lanes: Vec::new(),
            sessions: Vec::new(),
            head_oid: Some("1111111aaaa".into()),
            authored_by: None,
            labels: Vec::new(),
            auto_review: None,
            last_review: None,
            stale_approval: None,
            turn: None,
            shared_review: Default::default(),
        };

        assert_eq!(closed_note(&pr_with(true, PrState::Open), false), None);
        assert_eq!(closed_note(&pr_with(true, PrState::Merged), false), None, "own PR merged: still kept and sent");
        assert_eq!(closed_note(&pr_with(true, PrState::Merged), true), None, "own PR merged by hand: still kept and sent");
        assert_eq!(closed_note(&pr_with(true, PrState::Closed), false).as_deref(), Some("Not sent: the PR was closed before the review was done."));
        assert_eq!(closed_note(&pr_with(false, PrState::Merged), false).as_deref(), Some("Not posted: the PR was closed before the review was done."));
        assert_eq!(closed_note(&pr_with(false, PrState::Merged), true).as_deref(), Some("Not posted: the PR is merged, so the review is kept here."));
        assert_eq!(closed_note(&pr_with(false, PrState::Closed), true).as_deref(), Some("Not posted: the PR was closed before the review was done."));
    }

    #[test]
    fn nest_parent_walks_to_top_level_session_and_handles_archived_or_cycles() {
        use crate::fixtures;
        use crate::model::Helper;

        let helper = |key: &str, parent: &str| Session {
            helper: Some(Helper { parent: Some(parent.into()), ..Default::default() }),
            ..fixtures::session(key)
        };
        let lead = fixtures::session("claude:claude:lead");
        let worker = helper("claude:claude:worker", "claude:claude:lead");
        let sub = helper("codex:codex:sub", "claude:claude:worker");
        let lanes = vec![fixtures::lane("/r", vec![lead.clone(), worker.clone(), sub.clone()])];
        assert_eq!(nest_parent(&lanes, &lead), "claude:claude:lead");
        assert_eq!(nest_parent(&lanes, &worker), "claude:claude:lead");
        assert_eq!(nest_parent(&lanes, &sub), "claude:claude:lead");

        // An archived parent is not a target: the helper itself is top-level then.
        let archived_lead = Session { archived: true, ..lead };
        let lanes_archived = vec![fixtures::lane("/r", vec![archived_lead, worker.clone()])];
        assert_eq!(nest_parent(&lanes_archived, &worker), "claude:claude:worker");

        // A parent cycle terminates without looping forever.
        let a = helper("claude:claude:a", "claude:claude:b");
        let b = helper("claude:claude:b", "claude:claude:a");
        let lanes_cycle = vec![fixtures::lane("/r", vec![a.clone(), b])];
        assert_eq!(nest_parent(&lanes_cycle, &a), "claude:claude:b");
    }
}
