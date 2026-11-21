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

use super::{claim_state, has_checkout, origin, Part, Round, Store, Tracked};
use crate::config::ReviewsCfg;
use crate::model::{Board, Lane, PartState, PrItem, ReviewStage};
use crate::shared::{Machine, Machines};
use codealong_peer::pr::{AutoReview, ClaimState, HandedReview, SharedReview};

/// Which machine reviews a PR of yours.
#[derive(Debug, PartialEq)]
enum Reviewer<'a> {
    Here,
    /// Another, with its latest round of the PR when it has one.
    Elsewhere(&'a Machine, Option<&'a AutoReview>),
    /// None: no machine owns it, or none that would review it is around.
    Nobody,
}

/// The machine of yours that owns PR `pr` of yours, by its card, when another does.
fn owner<'a>(pr: &PrItem, machines: &'a Machines) -> Option<&'a Machine> {
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

/// Whether this machine reviews PR `pr` of yours for the machine that owns it now, as `board`'s
/// cards and its own (`PrItem::shared_review`) say.
pub(super) fn reviews_here(pr: &PrItem, board: &Board, now: i64) -> bool {
    pr.mine && origin(&board.lanes, pr).is_none() && reviewer(pr, &pr.shared_review, false, &board.machines, now) == Reviewer::Here
}

/// Another machine of yours that reviews PR `pr` of yours, at commit `head`, with its latest round
/// of the PR, for this machine's card to show: until a review of `head` that another machine wrote
/// is kept here (`import`), which says more.
pub(super) fn elsewhere<'a>(t: Option<&Tracked>, pr: &PrItem, head: &str, owned: bool, machines: &'a Machines, now: i64) -> Option<(&'a Machine, Option<&'a AutoReview>)> {
    if t.is_some_and(|t| t.rounds.iter().any(|r| r.head == head && r.from.is_some())) {
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
/// machine owns, from their cards (`handed`): each as a round of its own, in its place among the
/// PR's rounds by when it was done, once for each commit, and not over a review of that commit
/// that got done here. Not while a round here is under way, which may give way to it first
/// (`ELSEWHERE`). True when one was kept.
pub(super) fn import(store: &mut Store, key: &str, pr: &PrItem, machines: &Machines) -> bool {
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
        let part = Part { state: PartState::Done, review: Some(h.review.clone()), agent: Some(format!("{} on {}", h.agent, m.name)), ..Part::asked(&h.agent, None, None, h.at) };
        let round = Round { parts: vec![part], from: Some(m.name.clone()), ..Round::new(&h.head, ReviewStage::Kept, h.at) };
        let t = store.prs.entry(key.to_string()).or_default();
        let at = t.rounds.iter().position(|r| r.at > h.at).unwrap_or(t.rounds.len());
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
