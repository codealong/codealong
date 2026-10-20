//! Reviewing the pull requests that wait on your review without being asked, when config.toml
//! says to (`[reviews] auto`, Settings › Automation). Every board the engine publishes is looked
//! at for open PRs that ask for your review, aren't yours and aren't drafts. Each is reviewed in a
//! session of its own for that PR, on `[reviews] account` and `model` (else the account Auto would
//! start, on its own model), where `[reviews] start` says (`review_places`: by default never in a
//! terminal, under the harness's background service, else in Codealong), started in the worktree
//! that holds the PR's branch, else the repo's main checkout, as Review with… starts one. It's
//! handed the PR as Markdown and its whole diff, written by Codealong (`github::for_reading`),
//! since an agent asking `gh` itself would stop on a permission prompt; the diff has each line's
//! number in the file's new version beside it (`numbered`), so a comment can name its line. It's
//! told what to do by `[reviews] prompt`, else Codealong's own `INSTRUCTIONS`: a prompt of yours
//! may have it start subagents of its own, on other models, and combine what they find. Either
//! way it's told which PR, where it's written out, to change nothing and post nothing, and how to
//! lay out its last message, and it's linked to the PR (pr-links.json), so the PR's card lists it.
//!
//! Its review is its session's last message once its turn ends: a summary, then its comments on
//! particular lines in a `review-comments` block (`parse_reply`). It's posted where `[reviews]
//! post` says: by default a review on your GitHub account left as a draft, which only you see,
//! its comments on their lines where the PR's diff shows them (`draft_plan`), for you to finish and
//! submit; or a comment on the PR, each line comment under a link to its line; or nowhere. Never a
//! submitted review, which would settle the request to you, unless `[reviews] submit` says to
//! (below). What's posted says an agent wrote it
//! and which commit it read, with a hidden mark (`marker`) so a review already there for that
//! commit, from another Mac or before a restart, isn't posted twice. An agent that can't review
//! it says `SKIP:` and why.
//!
//! When new commits come in while the PR still waits on your review, the same session reviews it
//! again, once the branch has held still a few minutes (an author's pushes come in runs): it's
//! sent the PR as it is now and what changed since the commit it last reviewed, and asked what of
//! its findings is fixed; a session that isn't running goes on where `[reviews] start` lets it,
//! and one that's gone, or can't go on there, is replaced by a new one, which closes it where it
//! still runs and isn't at work (`ask`). A review whose PR moved on before it was done isn't
//! posted: the next round covers it.
//!
//! A reviewer whose turn was cut off partway (`Session::cut_off`: what ran it ended, as Codealong
//! restarting ends the sessions it runs) is told to carry on where it runs or can go on, as
//! another round's prompt is handed to it (`hand`, `Job::CarryOn`), a few times at most
//! (`CARRY_ONS`). One stopped on purpose ends its round without a review.
//!
//! A start whose harness names its session itself (Antigravity, OpenCode, Codex in a terminal)
//! leaves where to look (`Pending`): the session is the next there whose first prompt opens as a
//! review's of the PR (`found`, `opening`). One that hasn't shown in ten minutes fails its round,
//! which is taken up again should it show later with the PR still at that commit
//! (`Tracked::take_up`), so a review it wrote is still read and posted.
//!
//! A few rounds run at once at most; one whose session waits on you doesn't hold a place. A round
//! that didn't get done isn't tried again for the same commit; the next push tries again, and so
//! does Try again on the PR's card and on its notice (`again`), which posts a review that was
//! written but couldn't be posted rather than have it written again (`Reviewer::repost`), and takes
//! up a round in the session it never found, wherever that showed (`Reviewer::take_up`). A start
//! that couldn't reach GitHub to read the PR (`github::unreachable`: the network back from sleep,
//! GitHub struggling) isn't a round yet: it's tried again a few times first (`START_TRIES`). Until a
//! PR's round is done the PR is out of what waits on you (`apply`, `AutoReview::holds`), an earlier
//! approval of the agent's too: what waits on you is only what you can act on. Where
//! each PR stands is kept in reviews.json next to the config (`Store`), which this keeps in memory
//! and saves whole, and the engine reads for the PRs' cards (`apply`). A round keeps its reviewer's
//! turn as a part (`Part`), as it did when several could review one PR.
//!
//! The pull requests your own sessions open are reviewed the same way when `[reviews.own] auto`
//! says to (`wants_own`): each open PR of yours whose session (`PrItem::primary`, the one that
//! opened it) is on this Mac, and a draft only when `[reviews.own] drafts` says so (another's
//! draft never), on `[reviews.own]`'s account and model, else the account Auto would start among
//! those of another harness than that session's (`Seat::avoid`), so another model reads what one
//! wrote. Nothing goes on GitHub: the review is kept here (`ReviewStage::Kept`)
//! and, with `[reviews.own] send`, handed back to the PR's session once that runs here and doesn't
//! wait on you (`Job::Send`), as a file it's told to read (`review_file`, `back_message`), so the
//! other agent's words never reach it typed in as yours. A review that found nothing to raise
//! (`clean`) isn't sent, nor one of a commit the PR has moved on from: the next round's is. The
//! PR's Review tab sends one by hand (`send_now`). A review's session is a helper of its PR
//! (`reviewers`), and of a PR of yours that session's as well (`nest_own`), so it's listed under
//! the session it goes back to, and while sending is on, what it says to that session isn't held
//! for you, as a helper's isn't (`reviews_for`).
//!
//! Every review ends on what its agent would do with the PR, a `Verdict:` line (`verdict_of`):
//! approve, ask for changes, or comment, which the PR's windows suggest beside its verdicts.
//! `[reviews] submit` says which of them goes as your review once the review is posted
//! (`to_submit`, `Job::Submit`): an approval, a request for changes, or that request as a comment,
//! which doesn't hold the PR back. Your draft review is what's submitted, unless it holds words of
//! yours; without one, a comment or a request for changes points to the review posted as a comment,
//! or carries the review with posting off (`without_draft`). A PR that waits on your review is held
//! out of what waits on you until its review is submitted, when it's to be. An agent that can't
//! tell leaves it to you. The PR's box submits it by hand, as the verdict its agent suggests, with
//! what you wrote before it (`submit_now`, through lib.rs `review_pr`).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::config::{support_dir, AccountCfg, Config, ReviewPost, ReviewStart, ReviewSubmit, ReviewsCfg};
use crate::engine::Handle;
use crate::github::{LineComment, PendingReview, Verdict};
use crate::model::{AutoReview, Board, Harness, Lane, PartState, PrItem, PrState, ReviewPart, ReviewStage, Session, Status};
use crate::prlinks::pr_key;
use crate::start::{Place, ResumeIn, StartSpec};
use crate::transcript::Item;
use crate::util::{now_ms, DAY_MS};

pub fn path() -> PathBuf {
    support_dir().join("reviews.json")
}

/// What the file holds, for the error when it doesn't read as that.
const WHAT: &str = "a list of reviews";

/// How many rounds run at once, at most.
const AT_ONCE: usize = 2;
/// New commits are reviewed once the PR's branch has held still this long.
const QUIET_MS: i64 = 5 * 60_000;
/// How long after a turn's end whose review isn't written yet it's read again, and the first
/// wait after a post that failed, doubled each time.
const RETRY_MS: i64 = 30_000;
/// How many times a post is tried before the round is given up on.
const POST_TRIES: u32 = 4;
/// How many times a round that couldn't reach GitHub to read its PR is started before it's given
/// up on, waiting `RETRY_MS` and twice as long each time after: a quarter of an hour, which a
/// network coming back after a wake, or a blip of GitHub's, is over in.
const START_TRIES: u32 = 6;
/// How many times a reviewer whose turn was cut off is told to carry on before the round is
/// given up on: Codealong restarts often while it's being worked on, and each cuts off the
/// reviews it runs.
const CARRY_ONS: usize = 4;
/// How long a session started without a key is looked for, and a known one waited for to show
/// and to be seen taking the turn it was asked for (`Part::begun`).
const SHOW_MS: i64 = 10 * 60_000;
/// Reviews of PRs that no longer wait on you are forgotten this long after their last round.
const KEEP_MS: i64 = 30 * DAY_MS;
/// GitHub takes comments of up to 65,536 characters.
const COMMENT_MAX: usize = 60_000;
/// How long a PR that waits on your review is held out of what waits on you for a review that
/// isn't done (`apply`): one that takes longer, or never starts, hands it to you.
const HOLD_MS: i64 = 2 * 60 * 60_000;
/// What the reviewer is called when config.toml names no account: the one Auto would start.
pub const AUTO: &str = "auto";
/// What a reviewer is told to do when `[reviews] prompt` doesn't say (Settings shows it to edit).
pub const INSTRUCTIONS: &str = "Look for bugs, security and data risks, missing tests, and anything that doesn't do what the PR says it does. Read the code around the changes too, and raise where they don't fit it in a way that will cost whoever works on it next: something put where the codebase doesn't keep that kind of thing, or reaching across a boundary it keeps; a way of doing things other than the one the code around it holds to, or than the repo's written rules (a CLAUDE.md, AGENTS.md or contributing guide) ask for; something the codebase already has, written again; and more machinery than the change needs. Leave out differences of taste that no rule of the repo's settles, praise and a summary of the PR, and if you find nothing worth raising, say so in a sentence.";

/// What the reviewer is told to do: `[reviews] prompt`, else `INSTRUCTIONS`.
pub fn instructions(cfg: &ReviewsCfg) -> &str {
    Some(cfg.prompt.trim()).filter(|p| !p.is_empty()).unwrap_or(INSTRUCTIONS)
}

/// What the reviewer of `pr` is told to do: `[reviews.own] prompt` for a PR of yours, else
/// `instructions`; and after it, what the repo's team adds (`.codealong/config.toml`), said as
/// theirs. `cfg` is as it goes for the PR's repo (`RepoSettings::reviews`).
fn instructions_for(cfg: &ReviewsCfg, pr: &PrItem) -> String {
    let (yours, team) = if pr.mine { (Some(cfg.own.prompt.trim()).filter(|p| !p.is_empty()).unwrap_or(INSTRUCTIONS), &cfg.own.team_instructions) } else { (instructions(cfg), &cfg.team_instructions) };
    match team.trim() {
        "" => yours.to_string(),
        team => format!("{yours}\n\n{}'s team asks this of every review there ({}):\n\n{team}", pr.repo_slug, crate::teamfile::FILE),
    }
}

/// A start that didn't say which session it made: the session is the next one of its account in
/// its worktree whose first prompt names the PR (`found`).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Pending {
    pub lane: String,
    pub account: String,
    pub at: i64,
}

/// The reviewer's turn in a round, in its session.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Part {
    /// The reviewer (`Seat::who`).
    pub who: String,
    /// Its session, once known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending: Option<Pending>,
    /// When it was asked: what its session writes after this is its review.
    pub at: i64,
    pub state: PartState,
    /// Its review once read: its session's last message.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review: Option<String>,
    /// Who wrote it, as what's posted says: its harness, and its model where that's known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    /// Why it was skipped or didn't get done.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// When its session was told to carry on, each time its turn was cut off partway.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub carried_on: Vec<i64>,
    /// When its session was first seen at work (`at_work`) since it was last asked: the turn it
    /// was asked for had begun.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub began: Option<i64>,
}

impl Part {
    pub fn asked(who: &str, session: Option<String>, pending: Option<Pending>, at: i64) -> Part {
        Part { who: who.to_string(), session, pending, at, state: PartState::Reviewing, review: None, agent: None, note: None, carried_on: Vec::new(), began: None }
    }

    pub fn failed(who: &str, why: String, at: i64) -> Part {
        Part { state: PartState::Failed, note: Some(why), ..Part::asked(who, None, None, at) }
    }

    /// Who wrote its review, as what's posted names them.
    fn label(&self) -> String {
        self.agent.clone().unwrap_or_else(|| self.who.clone())
    }

    fn reviewing(&self) -> bool {
        self.state == PartState::Reviewing
    }

    /// The model it reviews on, as its name (`Seat::who`) has it after the account.
    fn model(&self) -> Option<&str> {
        self.on().0
    }

    /// The effort it reviews at, as its name has it last (`at xhigh`).
    fn effort(&self) -> Option<&str> {
        self.on().1
    }

    /// The model and effort its name (`Seat::who`) has after the account.
    fn on(&self) -> (Option<&str>, Option<&str>) {
        let rest = self.who.split_once(' ').map_or("", |(_, r)| r);
        let (model, effort) = match rest.strip_prefix("at ") {
            Some(e) => ("", Some(e)),
            None => rest.split_once(" at ").map_or((rest, None), |(m, e)| (m, Some(e))),
        };
        (Some(model).filter(|m| !m.is_empty()), effort)
    }

    /// When it was last asked: first, or told to carry on.
    fn last_asked(&self) -> i64 {
        self.carried_on.last().copied().unwrap_or(self.at)
    }

    /// Its session has been seen at work since it was last asked. Until then a session that
    /// reads as ended may not show the turn yet: a harness's reader can see a new session before
    /// what runs it (an `agy` the last listing of the processes didn't have, 2026-10-18).
    fn begun(&self) -> bool {
        self.began.is_some_and(|at| at >= self.last_asked())
    }

    /// Its session `s` was cut off partway through its turn, since it was asked or last told to
    /// carry on, and nothing runs it.
    fn cut_off_in(&self, s: &Session) -> bool {
        !s.live && s.cut_off.is_some_and(|at| at >= self.last_asked())
    }

    /// It was asked lately, first or to carry on, and its session isn't seen at work since: the
    /// turn is starting, whatever a board from before it shows (a cut that it answers, say).
    fn starting(&self, now: i64) -> bool {
        !self.begun() && now - self.last_asked() < SHOW_MS
    }
}

fn fail(p: &mut Part, why: &str) {
    p.state = PartState::Failed;
    p.note = Some(why.to_string());
}

/// Why a part whose start named no session failed when its session didn't show in time
/// (`SHOW_MS`), which `Tracked::take_up` goes by.
const NEVER_SHOWED: &str = "Its session never showed up.";
/// Why a part failed when its session's turn ended without a review and nothing ran it, once it
/// had begun or `SHOW_MS` passed (`Went::NoReview`), which `Tracked::take_up` goes by too.
const STOPPED: &str = "Its session stopped before it wrote a review.";

/// A round: the review of one commit of a PR.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Round {
    /// The commit it reviews.
    pub head: String,
    pub stage: ReviewStage,
    /// When it began.
    pub at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<i64>,
    /// What it posted, by its link.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    /// Why it was skipped or didn't get done, or wasn't posted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// The reviewer's turn: one, once asked.
    #[serde(default)]
    pub parts: Vec<Part>,
    /// Where a review of a PR of yours went back to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sent: Option<Sent>,
    /// When this round's review was submitted as yours (`[reviews] submit`), and as what, or why
    /// it wasn't. `approved` in files from when only approvals were.
    #[serde(default, skip_serializing_if = "Option::is_none", alias = "approved")]
    pub submitted: Option<Submission>,
    /// Asked for with Review with… (`review_now`): it goes on, and is posted and submitted, with
    /// `[reviews] auto` off too.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub by_hand: bool,
}

impl Round {
    fn new(head: &str, stage: ReviewStage, at: i64) -> Round {
        let ended_at = (stage != ReviewStage::Reviewing).then_some(at);
        Round { head: head.to_string(), stage, at, ended_at, comment: None, note: None, parts: Vec::new(), sent: None, submitted: None, by_hand: false }
    }
}

/// A round's review submitted as yours (`Job::Submit`), as `verdict`, or not, and why.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Submission {
    pub at: i64,
    /// What it was submitted as; an approval in files from when only approvals were.
    #[serde(default = "approval")]
    pub verdict: Verdict,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failed: Option<String>,
}

fn approval() -> Verdict {
    Verdict::Approve
}

/// A review of a PR of yours sent to the session that opened it (`Job::Send`), or given up on
/// after a few tries (`failed`, why).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Sent {
    pub session: String,
    pub at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failed: Option<String>,
}

/// One PR's reviews.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Tracked {
    /// The session the reviewer reviews it in, by the reviewer's name (`Seat::who`), for the next
    /// round.
    #[serde(default)]
    pub sessions: BTreeMap<String, String>,
    /// Oldest first: the last is where it stands.
    #[serde(default)]
    pub rounds: Vec<Round>,
}

impl Tracked {
    /// Every session a round of its reviews asked, and the one kept for its next round.
    fn asked(&self) -> impl Iterator<Item = &str> {
        let parts = self.rounds.iter().flat_map(|r| &r.parts).filter_map(|p| p.session.as_deref());
        self.sessions.values().map(String::as_str).chain(parts)
    }

    /// The commit its last review that got done was of, for what changed since.
    fn done_head(&self) -> Option<&str> {
        self.rounds.iter().rev().find(|r| matches!(r.stage, ReviewStage::Posted | ReviewStage::Drafted | ReviewStage::Kept)).map(|r| r.head.as_str())
    }

    /// Takes its latest round up again in the session it gave up on, which went on after: the
    /// round failed for that alone, of the commit open PR `pr` is at. Its review is then read
    /// once its turn ends, and posted, as if it had gone on in time. A session it gave up looking
    /// for (`NEVER_SHOWED`) is looked for where its start was (`Pending`); with `wide`, as Try
    /// again asks, one from before that was kept is looked for in every worktree here, on every
    /// account. One it took for stopped (`STOPPED`) before it was ever seen at work is at work
    /// again; with `wide`, any that did anything since. Not one prompted since: what it's at is
    /// someone else's. True when it was.
    fn take_up(&mut self, board: &Board, pr: &PrItem, wide: bool) -> bool {
        let Some(r) = self.rounds.last_mut().filter(|r| r.stage == ReviewStage::Failed && pr.state.is_open() && pr.head_oid.as_deref() == Some(r.head.as_str())) else { return false };
        let gave_up_on = |p: &Part| p.state == PartState::Failed && p.note.as_deref() == Some(if p.session.is_none() { NEVER_SHOWED } else { STOPPED });
        if r.parts.is_empty() || !r.parts.iter().all(gave_up_on) {
            return false;
        }
        let since = r.ended_at.unwrap_or(r.at);
        let went_on = |p: &Part, s: &Session| s.last_prompt_at <= since && (at_work(s) && (wide || !p.begun()) || wide && s.last_activity > since);
        let here = || board.lanes.iter().filter(|l| l.device.is_none()).flat_map(|l| &l.sessions);
        let mut took = false;
        for p in r.parts.iter_mut() {
            let shown = match (&p.session, &p.pending) {
                (Some(k), _) => board.session(k).filter(|(_, s)| went_on(p, s)).map(|_| k.clone()),
                (None, Some(pending)) => found(board, pending, pr),
                (None, None) if wide => first_naming(here(), pr, p.at),
                (None, None) => None,
            };
            let Some(s) = shown else { continue };
            self.sessions.insert(p.who.clone(), s.clone());
            p.session = Some(s);
            p.pending = None;
            p.state = PartState::Reviewing;
            p.note = None;
            took = true;
        }
        if took {
            r.stage = ReviewStage::Reviewing;
            r.ended_at = None;
            r.note = None;
        }
        took
    }
}

/// reviews.json: each PR's reviews, by its key (`prlinks::pr_key`).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Store {
    #[serde(default)]
    pub prs: BTreeMap<String, Tracked>,
}

/// The reviewer a round asks, with its session for the PR when it has one.
#[derive(Debug, Clone, PartialEq)]
pub struct Seat {
    /// What it's called: its account and model (`claude opus`), `AUTO` for the account Auto
    /// would start. A session is kept for each, so naming another starts a new one.
    pub who: String,
    /// None: the one Auto would start.
    pub account: Option<String>,
    /// None: the account's own.
    pub model: Option<String>,
    /// None: the account's own, or the routing group's member's. In `who` as the model is, so a
    /// reviewer at another effort has a session of its own: one kept running would go on at the
    /// effort it has.
    pub effort: Option<String>,
    pub session: Option<String>,
    /// The harness Auto keeps off: the one of the session that opened a PR of yours, so another
    /// model reviews it. Auto takes one of it only when no account of another can start.
    pub avoid: Option<Harness>,
}

/// The reviewer `cfg` names, with its session for the PR in `t`.
fn seat(cfg: &ReviewsCfg, t: Option<&Tracked>) -> Seat {
    named_seat(&cfg.account, &cfg.model, &cfg.effort, t)
}

fn named_seat(account: &str, model: &str, effort: &str, t: Option<&Tracked>) -> Seat {
    let some = |s: &str| Some(s.trim()).filter(|s| !s.is_empty()).map(String::from);
    let (account, model, effort) = (some(account), some(model), some(effort));
    let at = effort.as_deref().map(|e| format!("at {e}"));
    let who = [Some(account.as_deref().unwrap_or(AUTO)), model.as_deref(), at.as_deref()].into_iter().flatten().collect::<Vec<_>>().join(" ");
    Seat { session: t.and_then(|t| t.sessions.get(&who).cloned()), who, account, model, effort, avoid: None }
}

/// The harness that wrote `pr`: the session that opened it here, else what its commit trailer or
/// description says (`PrItem::authored_by`).
pub fn pr_harness(lanes: &[Lane], pr: &PrItem) -> Option<Harness> {
    origin(lanes, pr).map(|(_, o)| o.harness).or(pr.authored_by)
}

/// The reviewer of `pr`: `[reviews.own]`'s for a PR of yours, else `seat`, keeping off the harness
/// that wrote the PR when `other_harness` is on.
fn seat_for(cfg: &ReviewsCfg, t: Option<&Tracked>, lanes: &[Lane], pr: &PrItem) -> Seat {
    let (mut s, other_harness) = if pr.mine {
        (named_seat(&cfg.own.account, &cfg.own.model, &cfg.own.effort, t), cfg.own.other_harness)
    } else {
        (seat(cfg, t), cfg.other_harness)
    };
    if s.account.is_none() && other_harness {
        s.avoid = pr_harness(lanes, pr);
    }
    s
}

/// Whether `key`'s account is at a usage limit: a session of it was refused, or its meters are at
/// its usage cap for the model it runs on.
fn session_at_limit(board: &Board, accounts: &[AccountCfg], key: &str, model: Option<&str>, now: i64) -> bool {
    let Some((_, s)) = board.session(key) else { return false };
    let all = crate::start::all_sessions(&board.lanes);
    if crate::start::refused_accounts(&all, now).contains(&s.account) {
        return true;
    }
    let cap = accounts.iter().find(|a| a.name == s.account).and_then(|a| a.usage_cap);
    let check_model = model.or(s.model.as_deref());
    board.usage.iter().any(|u| u.account == s.account && (u.needs_sign_in || crate::start::near_limit(u, check_model, cap)))
}

/// `seat_for`, starting afresh when the PR's kept session is on an account at its usage limit.
fn seat_in(cfg: &ReviewsCfg, t: Option<&Tracked>, board: &Board, pr: &PrItem, now: i64) -> Seat {
    let mut s = seat_for(cfg, t, &board.lanes, pr);
    if let Some(key) = s.session.as_deref() {
        if session_at_limit(board, &[], key, s.model.as_deref(), now) {
            s.session = None;
        }
    }
    s
}

const FLAGGED_REVIEW: &str = "Automatic reviews and follow-ups are paused because a session was flagged by its provider. You can start or send a review yourself, or change Settings › Automation.";

fn review_blocked(lanes: &[Lane], sessions: &crate::config::SessionsCfg, pr: &PrItem, seat: &Seat) -> bool {
    origin(lanes, pr).is_some_and(|(_, s)| sessions.blocks(s))
        || seat.session.as_deref().and_then(|key| crate::board::home(lanes, key)).is_some_and(|(_, s)| sessions.blocks(s))
}

/// Automatic rounds leave flagged conversations alone. A review asked for by hand can start
/// afresh without continuing the flagged reviewer or messaging the PR's flagged opener.
fn review_seat(board: &Board, pr: &PrItem, mut seat: Seat, by_hand: bool) -> Option<Seat> {
    let reviewer_blocked = seat.session.as_deref().is_some_and(|k| board.automation_blocked(k));
    if !by_hand && review_blocked(&board.lanes, &board.sessions, pr, &seat) {
        return None;
    }
    if reviewer_blocked { seat.session = None; }
    Some(seat)
}

/// What's due.
#[derive(Debug, Clone, PartialEq)]
pub enum Job {
    /// Start a round at `head`, asking `seat`. `since`: the commit the PR's last review that got
    /// done was of. `by_hand`: asked for with Review with… (`review_now`), which needs no
    /// `[reviews] auto` and reviews a commit again.
    Review { pr: PrItem, head: String, since: Option<String>, seat: Seat, by_hand: bool },
    /// The round's part `part` ended its turn in `session`: read the review it wrote after `at`.
    Read { pr: PrItem, head: String, part: usize, session: String, at: i64 },
    /// The round's part `part` was cut off partway in `session`: tell it to carry on, on `model`
    /// at `effort`.
    CarryOn { pr: PrItem, head: String, part: usize, session: String, model: Option<String>, effort: Option<String> },
    /// Post the round's review, written by `agent`. `round`: when the round began, which tells its
    /// review from another of the same commit in your draft (`draft_mark`).
    Post { pr: PrItem, head: String, review: String, agent: String, round: i64 },
    /// Hand a PR of yours' review of commit `head`, written by `agent`, to `session`, the one
    /// that opened it.
    Send { pr: PrItem, head: String, session: String, review: String, agent: String },
    /// Submit a review of a PR that waits on your review as you, as `verdict`, for its round's
    /// review of `head` (`to_submit`): your draft review, else `text` (`without_draft`; none
    /// when only the draft would do).
    Submit { pr: PrItem, head: String, verdict: Verdict, text: Option<String> },
}

impl Job {
    fn pr(&self) -> &PrItem {
        match self {
            Job::Review { pr, .. } | Job::Read { pr, .. } | Job::CarryOn { pr, .. } | Job::Post { pr, .. } | Job::Send { pr, .. } | Job::Submit { pr, .. } => pr,
        }
    }
}

/// How a job went.
#[derive(Debug, Clone, PartialEq)]
pub enum Went {
    /// A round is under way: the reviewer's part, asked or not.
    Started(Vec<Part>),
    /// Nothing was started, and why.
    NotStarted(String),
    /// Nothing was started because GitHub couldn't be reached to read the PR, and why
    /// (`github::unreachable`): it's tried again a few times (`START_TRIES`).
    Unreachable(String),
    /// Nothing was done, and nothing's wrong: the PR moved on since it was due.
    Later,
    /// A turn's review, and who wrote it.
    Read { review: String, agent: String },
    /// Its review was posted, at this link (by another Mac, or before a restart, for a review
    /// that found it there).
    Posted(String),
    /// Its review was put in a draft review on your account, seen at this link.
    Drafted(String),
    /// Its review is done and left in its session: posting is off, or the PR ended before it was
    /// done (`note` says which).
    Kept(Option<String>),
    /// The agent said it couldn't review it, and why.
    Skipped(String),
    /// The turn hasn't written its review yet.
    NotYet,
    /// Its session's turn ended without a review and nothing runs it: it stopped, unless its
    /// turn hasn't shown yet (`Part::starting`), which the round tells apart.
    NoReview,
    /// A session cut off partway was told to carry on, from `at`, before it was handed the words:
    /// the session its turn is in, or where to look for it, as `hand` says.
    CarriedOn { session: Option<String>, pending: Option<Pending>, at: i64 },
    /// It didn't get done, and why.
    Failed(String),
    /// Posting didn't go through, and why: it's tried again a few times.
    PostFailed(String),
    /// Its review was sent to this session.
    Sent(String),
    /// Its review was submitted as yours.
    Submitted,
    /// It wasn't submitted, and won't be for this round: why.
    NotSubmitted(String),
}

/// What a job that went wrong says as a notice (`Reviewer::told`), and the PR (its key as the
/// board has it) a review of which didn't start or wasn't posted, which the notice offers to try
/// again (`again`).
#[derive(Debug, Clone, PartialEq)]
pub struct Said {
    pub text: String,
    pub again: Option<String>,
}

impl From<String> for Said {
    fn from(text: String) -> Said {
        Said { text, again: None }
    }
}

/// What's next for a round under way.
enum Next {
    Job(Job),
    Changed,
    Wait,
}

/// Where each PR's reviews stand, and what's under way.
#[derive(Default)]
pub struct Reviewer {
    store: Store,
    /// Where the store is saved; none in tests.
    path: Option<PathBuf>,
    /// PRs with a job under way, by key.
    busy: HashSet<String>,
    /// Each PR's head as first seen, and when, by key: new commits wait for it to hold still.
    heads: HashMap<String, (String, i64)>,
    /// A PR isn't looked at again before this (ms), by key.
    next_at: HashMap<String, i64>,
    /// Tries that failed in a row, by key: posts, sends and submits, and starts that couldn't
    /// reach GitHub (`Went::Unreachable`). A PR has one job at a time, so one count serves them.
    post_failures: HashMap<String, u32>,
}

/// Whether Codealong reviews a PR: it's open and not a draft, it asks for your review, and it
/// isn't yours.
pub fn wants(pr: &PrItem) -> bool {
    pr.review_requested && pr.state.is_open() && pr.state != PrState::Draft && !pr.mine && pr.head_oid.is_some()
}

/// The session a PR of yours goes back to: the one that opened it (`PrItem::primary`), when it's
/// on this Mac and isn't a visitor's, a helper (a review's own session, which a PR whose opener
/// is gone could take for its primary) or archived.
fn origin<'a>(lanes: &'a [Lane], pr: &PrItem) -> Option<(&'a Lane, &'a Session)> {
    let key = pr.primary.as_deref()?;
    lanes.iter().filter(|l| l.device.is_none()).find_map(|l| l.sessions.iter().find(|s| s.key == key && s.visitor.is_none() && s.helper.is_none() && !s.archived).map(|s| (l, s)))
}

/// Whether Codealong reviews a PR of yours (`[reviews.own]`): it's open, not a draft unless
/// `[reviews.own] drafts` says to review those too, and the session that opened it is here to hand
/// the review to.
pub fn wants_own(cfg: &ReviewsCfg, lanes: &[Lane], pr: &PrItem) -> bool {
    pr.mine && pr.state.is_open() && (pr.state != PrState::Draft || cfg.own.drafts) && pr.head_oid.is_some() && origin(lanes, pr).is_some()
}

/// Whether reviews of any kind are on in `cfg`: a round under way goes on only while they are,
/// or was asked for by hand.
fn on(cfg: &ReviewsCfg) -> bool {
    cfg.auto || cfg.own.auto
}

/// Whether `cfg` has Codealong review a PR: one that waits on your review while `[reviews] auto`
/// is on, one of yours while `[reviews.own] auto` is.
fn wanted(cfg: &ReviewsCfg, lanes: &[Lane], pr: &PrItem) -> bool {
    if pr.mine {
        cfg.own.auto && wants_own(cfg, lanes, pr)
    } else {
        cfg.auto && wants(pr)
    }
}

/// A session can be handed a review now: it runs here and doesn't wait on you to answer
/// something, which what's typed would answer.
fn can_take(s: &Session) -> bool {
    s.live && !crate::send::waits_on_you(s)
}

/// A review that found nothing that needs fixing: its agent would approve the PR (`verdict_of`),
/// or, from before verdicts, said `LGTM:`. One of a PR of yours isn't handed back.
pub fn clean(review: &str) -> bool {
    verdict_of(review) == Some(Verdict::Approve) || review.trim_start().strip_prefix("LGTM").is_some_and(|rest| rest.is_empty() || rest.starts_with(':') || rest.starts_with(char::is_whitespace))
}

/// What the agent would do with the PR, from the `Verdict:` line its review ends on (`layout`):
/// the last such line, Markdown's emphasis and all.
pub fn verdict_of(review: &str) -> Option<Verdict> {
    review.lines().rev().find_map(verdict_line)
}

fn verdict_line(line: &str) -> Option<Verdict> {
    let marks = |c: char| c == '*' || c == '_' || c == '`';
    let t = line.trim().trim_matches(marks).trim();
    let rest = t.get(..8).filter(|p| p.eq_ignore_ascii_case("verdict:")).map(|_| &t[8..])?;
    match rest.trim().trim_matches(|c: char| marks(c) || c == '.').trim().to_ascii_lowercase().replace('-', " ").as_str() {
        "approve" => Some(Verdict::Approve),
        "request changes" => Some(Verdict::RequestChanges),
        "comment" => Some(Verdict::Comment),
        _ => None,
    }
}

/// A review's text without its `Verdict:` line, which isn't for the PR's people.
fn without_verdict(text: &str) -> String {
    text.lines().filter(|l| verdict_line(l).is_none()).collect::<Vec<_>>().join("\n").trim().to_string()
}

/// What a round's review says to do with the PR: its done part's verdict.
fn round_verdict(r: &Round) -> Option<Verdict> {
    r.parts.iter().find(|p| p.state == PartState::Done).and_then(|p| p.review.as_deref()).and_then(verdict_of)
}

/// What's submitted as your review for an agent's verdict, as `[reviews] submit` says: an
/// approval for one that would approve, unless approvals are left to you; for one that would ask
/// for changes, that request, or a comment, which doesn't hold the PR back; nothing for one that
/// can't tell, which is yours to judge.
pub fn to_submit(submit: ReviewSubmit, verdict: Verdict) -> Option<Verdict> {
    use ReviewSubmit as S;
    match verdict {
        Verdict::Approve => matches!(submit, S::Approve | S::ApproveOrRequestChanges | S::ApproveOrComment).then_some(Verdict::Approve),
        Verdict::RequestChanges => match submit {
            S::ApproveOrRequestChanges | S::RequestChanges => Some(Verdict::RequestChanges),
            S::ApproveOrComment | S::Comment => Some(Verdict::Comment),
            S::Never | S::Approve => None,
        },
        Verdict::Comment => None,
    }
}

/// What's to be submitted as your review for round `r`, its review of `head`, if anything: the PR
/// still waits on your review at that commit, the round's review was posted where config.toml
/// says (or kept), `[reviews] submit` takes its agent's verdict (`to_submit`), and it hasn't been
/// tried; and reviews are on, or the round was asked for by hand, which goes the same way.
fn submit_due(cfg: &ReviewsCfg, pr: &PrItem, r: &Round, head: &str) -> Option<Verdict> {
    if !((cfg.auto || r.by_hand) && wants(pr) && r.head == head && r.submitted.is_none() && matches!(r.stage, ReviewStage::Drafted | ReviewStage::Posted | ReviewStage::Kept)) {
        return None;
    }
    to_submit(cfg.submit, round_verdict(r)?).filter(|v| !cfg.never_submit.contains(v))
}

/// What a review submitted as `verdict` for round `r` says when there's no draft review of yours
/// on the PR to submit: nothing, for an approval. A comment or a request for changes needs words:
/// a link to the review, posted as a comment, or the review itself with posting off, signed as a
/// posted one is. None for a review that went into a draft, which has gone since (submitted or
/// thrown away), so there's nothing of the agent's to submit.
fn without_draft(r: &Round, pr: &PrItem, head: &str, verdict: Verdict) -> Option<String> {
    if verdict == Verdict::Approve {
        return Some(String::new());
    }
    let p = r.parts.iter().find(|p| p.state == PartState::Done)?;
    match r.stage {
        // The line that points to it says what the agent said: one that asks for changes says so,
        // one that couldn't tell (submitted as a comment by hand, `submit_now`) doesn't.
        ReviewStage::Posted => {
            let says = if round_verdict(r) == Some(Verdict::RequestChanges) { "The agent's review asks for changes" } else { "The agent's review" };
            r.comment.as_ref().map(|link| signed(&format!("{says}: {link}"), pr, head, &p.label(), "submitted"))
        }
        ReviewStage::Kept => {
            let (summary, comments) = parse_reply(p.review.as_deref()?);
            Some(review_text(&summary, &comments, pr, head, &p.label(), "submitted"))
        }
        _ => None,
    }
}

/// A round's review that's still to go back to its session: done, of commit `head`, not sent or
/// given up on, with something to raise. The review and who wrote it.
fn to_send(r: &Round, head: &str) -> Option<(String, String)> {
    if r.stage != ReviewStage::Kept || r.head != head || r.sent.is_some() {
        return None;
    }
    let p = r.parts.iter().find(|p| p.state == PartState::Done)?;
    let review = p.review.clone().filter(|v| !clean(v))?;
    Some((review, p.label()))
}

/// A session's turn ended after `at`: it's done, or waits on you only because its reply asks you
/// something. One stopped by a usage limit, a lost connection, an error or what ran it ending
/// partway (cut off), or waiting on you to allow something, isn't done.
fn turn_ended(s: &Session, at: i64) -> bool {
    let ended = match s.status {
        Status::Unread | Status::Idle | Status::Stopped => true,
        Status::Needs => s.asks,
        Status::Working | Status::Error => false,
    };
    ended && s.last_activity >= at && s.limit.is_none() && s.connection_lost.is_none() && s.cut_off.is_none()
}

/// A session at work on a turn: working, or waiting on you to allow something partway.
fn at_work(s: &Session) -> bool {
    s.status == Status::Working || s.status == Status::Needs && !s.asks
}

/// How a review's first prompt opens (`first_prompt`), naming its PR. A harness's reader keeps
/// only a prompt's first words (`util::one_line`'s 140 characters), which a PR with a long title
/// cuts before its link, so this is what a review's session is known by.
fn opening(pr: &PrItem) -> String {
    opening_of(pr.number, &pr.repo_slug)
}

fn opening_of(number: impl std::fmt::Display, slug: &str) -> String {
    format!("{OPENS}{number} in {slug},")
}

const OPENS: &str = "Review pull request #";

/// Whether a first prompt, as a reader keeps it, is a review of `pr`: it opens as one does
/// (`opens_review_of`), or it has the PR's link in brackets as one does (a bare link to #7 would
/// match #70's too).
fn names(first: &str, pr: &PrItem) -> bool {
    opens_review_of(first, &pr.repo_slug, pr.number) || first.contains(&format!("({})", pr.url))
}

/// Whether a first prompt, as a reader keeps it, opens as a review of PR `number` of repo `slug`
/// does, case aside as reviews.json keys a PR (`pr_key`), even cut short (`util::truncate`) where
/// a long repo name takes the opening past what's kept, so long as it goes past the PR's number.
/// A prompt of yours with the PR's link in it doesn't: that's taken for a review only where a
/// start's session is looked for (`names`).
fn opens_review_of(first: &str, slug: &str, number: impl std::fmt::Display) -> bool {
    let (first, slug) = (first.to_ascii_lowercase(), slug.to_ascii_lowercase());
    let opening = opening_of(number, &slug).to_ascii_lowercase();
    let past_number = opening.len() - slug.len() - 1;
    first.starts_with(&opening) || first.strip_suffix('…').is_some_and(|cut| cut.len() > past_number && opening.starts_with(cut))
}

/// The first session among `sessions` started since `at` (a minute's leeway) whose first prompt
/// is a review of `pr` (`names`).
fn first_naming<'a>(sessions: impl Iterator<Item = &'a Session>, pr: &PrItem, at: i64) -> Option<String> {
    sessions
        .filter(|s| s.visitor.is_none() && s.started_at.unwrap_or(s.last_activity) >= at - 60_000 && s.first_prompt.as_deref().is_some_and(|f| names(f, pr)))
        .min_by_key(|s| s.started_at.unwrap_or(s.last_activity))
        .map(|s| s.key.clone())
}

/// The session a start that named none made: the first of its account in its worktree started
/// since, whose first prompt is a review of `pr`.
fn found(board: &Board, p: &Pending, pr: &PrItem) -> Option<String> {
    let lane = board.lanes.iter().find(|l| l.id == p.lane && l.device.is_none())?;
    first_naming(lane.sessions.iter().filter(|s| s.account == p.account), pr, p.at)
}

/// What's next for a round under way on PR `pr`, which is at `head` now. The reviewer's turn is
/// read once its session ended it, told to carry on when it was cut off partway (a few times at
/// most), and fails once its session is gone a while; once it's settled, its review is posted,
/// or the round ends without one. A round stuck while the PR moved on ends, for the next.
fn next(r: &mut Round, board: &Board, pr: &PrItem, head: &str, wanted: bool, now: i64) -> Next {
    let working = |k: &str| board.session(k).is_some_and(|(_, s)| s.status == Status::Working);
    if head != r.head && wanted && !r.parts.iter().filter(|p| p.reviewing()).any(|p| p.pending.is_some() || p.session.as_deref().is_some_and(working)) {
        end(r, ReviewStage::Failed, Some("New commits came in before it was done, so it's left for the next round.".into()), now);
        return Next::Changed;
    }
    let mut changed = false;
    for (i, p) in r.parts.iter_mut().enumerate().filter(|(_, p)| p.reviewing()) {
        let Some(k) = p.session.clone() else { continue };
        match board.session(&k) {
            Some((_, s)) if turn_ended(s, p.at) => return Next::Job(Job::Read { pr: pr.clone(), head: r.head.clone(), part: i, session: k, at: p.at }),
            Some((_, s)) if s.status != Status::Working && board.automation_blocked(&k) => {
                fail(p, "Its provider flagged the conversation, so it wasn't told to carry on.");
                changed = true;
            }
            Some((_, s)) if p.cut_off_in(s) && p.carried_on.len() < CARRY_ONS => {
                return Next::Job(Job::CarryOn { pr: pr.clone(), head: r.head.clone(), part: i, session: k, model: p.model().map(String::from), effort: p.effort().map(String::from) });
            }
            Some((_, s)) if p.cut_off_in(s) => {
                fail(p, &format!("Its turn was cut off partway again after it was told to carry on {CARRY_ONS} times."));
                changed = true;
            }
            None if now - p.at > SHOW_MS => {
                fail(p, "Its session is gone.");
                changed = true;
            }
            _ => {}
        }
    }
    if r.parts.iter().any(Part::reviewing) {
        return if changed { Next::Changed } else { Next::Wait };
    }
    if let Some(p) = r.parts.iter().find(|p| p.state == PartState::Done) {
        return Next::Job(Job::Post { pr: pr.clone(), head: r.head.clone(), review: p.review.clone().unwrap_or_default(), agent: p.label(), round: r.at });
    }
    let skip = r.parts.iter().find(|p| p.state == PartState::Skipped).and_then(|p| p.note.clone());
    let why = r.parts.iter().filter_map(|p| p.note.as_deref()).collect::<Vec<_>>().join(" ");
    match skip {
        Some(s) => end(r, ReviewStage::Skipped, Some(s), now),
        None => end(r, ReviewStage::Failed, Some(if why.is_empty() { "No review got done.".into() } else { why }), now),
    }
    Next::Changed
}

impl Reviewer {
    /// The reviews reviews.json holds, saved back there as they change.
    pub fn load() -> Reviewer {
        let path = path();
        Reviewer { store: crate::store::load_whole(&path, WHAT), path: Some(path), ..Reviewer::default() }
    }

    fn save(&self) {
        if let Some(path) = &self.path {
            // A failure is noted, and said as a notice (store::problems).
            let _ = crate::store::save_whole(path, &self.store);
        }
    }

    /// The jobs due now, at `now` (ms), with reviews on as `cfg` says. A round under way goes on
    /// (`next`); a PR that waits on your review at a commit no round has had is to be reviewed,
    /// at once the first time and once its branch has held still after that, while fewer than
    /// `AT_ONCE` rounds are under way. Nothing for a PR with a job under way, or whose wait after
    /// a try isn't over.
    pub fn due(&mut self, board: &Board, cfg: &ReviewsCfg, now: i64) -> Vec<Job> {
        // With reviews off, only what was asked for by hand goes on. Each PR goes as its repo's
        // settings say (`RepoSettings::reviews`).
        let settings = &board.repo_settings;
        if !(settings.reviews_on(cfg) || self.store.prs.values().any(|t| t.rounds.last().is_some_and(|r| r.by_hand))) || board.github_login.is_none() {
            return Vec::new();
        }
        let on_for = |pr: Option<&PrItem>| pr.map_or(cfg.auto || cfg.own.auto, |p| on(&settings.reviews(cfg, p)));
        let mut changed = false;
        let listed: HashMap<String, &PrItem> = board.prs.iter().map(|p| (pr_key(&p.key), p)).collect();
        // Sessions from starts that didn't name them, once they show; one that doesn't in time
        // fails its part, and its round is taken up again should it show later (`take_up`). And
        // when each session is first seen at work on what it was asked (`Part::begun`).
        for (key, t) in self.store.prs.iter_mut() {
            let pr = listed.get(key).copied();
            let on = on_for(pr);
            let failed = t.rounds.last().filter(|r| r.stage == ReviewStage::Failed).map(|r| r.by_hand);
            if let (Some(by_hand), Some(pr)) = (failed, pr) {
                if by_hand || (on && wanted(&settings.reviews(cfg, pr), &board.lanes, pr)) {
                    changed |= t.take_up(board, pr, false);
                }
            }
            let Some(r) = t.rounds.last_mut().filter(|r| r.stage == ReviewStage::Reviewing && (on || r.by_hand)) else { continue };
            for p in r.parts.iter_mut().filter(|p| p.reviewing()) {
                if let Some(pending) = p.pending.clone() {
                    if let Some(s) = pr.and_then(|pr| found(board, &pending, pr)) {
                        t.sessions.insert(p.who.clone(), s.clone());
                        p.session = Some(s);
                        p.pending = None;
                        changed = true;
                    } else if now - pending.at > SHOW_MS {
                        // Where to look is kept: it may show yet.
                        fail(p, NEVER_SHOWED);
                        changed = true;
                    }
                }
                // A board from before it was asked says nothing of that turn.
                if let Some(k) = &p.session {
                    if !p.begun() && now >= p.last_asked() && board.session(k).is_some_and(|(_, s)| at_work(s)) {
                        p.began = Some(now);
                        changed = true;
                    }
                }
            }
        }
        // PRs that no longer wait on you are forgotten a while after their last round.
        let before = self.store.prs.len();
        self.store.prs.retain(|k, t| listed.get(k).is_some_and(|p| wanted(&settings.reviews(cfg, p), &board.lanes, p)) || t.rounds.last().is_some_and(|r| now - r.ended_at.unwrap_or(r.at) < KEEP_MS) || self.busy.contains(k));
        changed |= self.store.prs.len() != before;
        // A round holds a place while one of its sessions works, is starting, or was cut off
        // partway, to be told to carry on or just told.
        let holds_place = |r: &Round| {
            r.stage == ReviewStage::Reviewing
                && r.parts.iter().any(|p| p.reviewing() && match &p.session { Some(k) => board.session(k).is_none_or(|(_, s)| s.status == Status::Working || (!board.automation_blocked(k) && (p.cut_off_in(s) || p.starting(now)))), None => p.pending.is_some() })
        };
        let mut running = self.busy.len() + self.store.prs.iter().filter(|(k, t)| !self.busy.contains(*k) && t.rounds.last().is_some_and(holds_place)).count();
        let mut jobs = Vec::new();
        let mut prs: Vec<&PrItem> = board.prs.iter().collect();
        // The longest waiting first.
        prs.sort_by_key(|p| (p.requested_at.unwrap_or(p.updated_at), p.key.clone()));
        for pr in prs {
            let key = pr_key(&pr.key);
            let Some(head) = pr.head_oid.clone() else { continue };
            let seen = self.heads.entry(key.clone()).or_insert_with(|| (head.clone(), now));
            if seen.0 != head {
                *seen = (head.clone(), now);
            }
            let held_since = seen.1;
            if self.busy.contains(&key) || self.next_at.get(&key).is_some_and(|&t| now < t) {
                continue;
            }
            let cfg = settings.reviews(cfg, pr);
            let cfg = cfg.as_ref();
            let want = wanted(cfg, &board.lanes, pr);
            let on = on(cfg);
            if let Some(r) = self.store.prs.get_mut(&key).and_then(|t| t.rounds.last_mut()).filter(|r| r.stage == ReviewStage::Reviewing && (on || r.by_hand)) {
                match next(r, board, pr, &head, want, now) {
                    Next::Job(job) => {
                        self.busy.insert(key);
                        jobs.push(job);
                    }
                    Next::Changed => changed = true,
                    Next::Wait => {}
                }
                continue;
            }
            if let Some(job) = self.send_due(board, cfg, pr, &head) {
                self.busy.insert(key);
                jobs.push(job);
                continue;
            }
            // What its repo's team sets for it (words for its reviewer, verdicts never submitted)
            // isn't read yet: its review and what's submitted for you wait for it.
            if settings.pending(pr) {
                continue;
            }
            if let Some((r, verdict)) = self.store.prs.get(&key).and_then(|t| t.rounds.last()).and_then(|r| Some((r, submit_due(cfg, pr, r, &head)?))) {
                let text = without_draft(r, pr, &head, verdict);
                self.busy.insert(key);
                jobs.push(Job::Submit { pr: pr.clone(), head, verdict, text });
                continue;
            }
            if !want {
                continue;
            }
            let t = self.store.prs.get(&key);
            if review_seat(board, pr, seat_for(cfg, t, &board.lanes, pr), false).is_none() {
                continue;
            }
            if t.is_some_and(|t| t.rounds.iter().any(|r| r.head == head)) {
                continue;
            }
            let again = t.is_some_and(|t| !t.rounds.is_empty());
            if (again && now - held_since < QUIET_MS) || running >= AT_ONCE {
                continue;
            }
            running += 1;
            self.busy.insert(key);
            jobs.push(Job::Review { pr: pr.clone(), head, since: t.and_then(|t| t.done_head()).map(String::from), seat: seat_in(cfg, t, board, pr, now), by_hand: false });
        }
        if changed {
            self.save();
        }
        jobs
    }

    /// Makes way for a round of PR `key` asked for by hand: not while one is under way, but an
    /// automatic round left under way when reviews were turned off (`on` false), which goes no
    /// further (`due`), is ended for it.
    fn clear_for_hand(&mut self, key: &str, on: bool, now: i64) -> Result<(), String> {
        let last = self.store.prs.get_mut(key).and_then(|t| t.rounds.last_mut()).filter(|x| x.stage == ReviewStage::Reviewing);
        if self.busy.contains(key) || last.as_ref().is_some_and(|x| on || x.by_hand) {
            return Err("A review of it is under way: its card says how that goes.".into());
        }
        if let Some(x) = last {
            end(x, ReviewStage::Failed, Some("Automatic reviews were turned off before it was done.".into()), now);
        }
        Ok(())
    }

    /// Puts PR `key`'s latest round back under way when all it missed was being posted: its agent
    /// wrote a review of `head`, the commit the PR is at now. `due` then posts it (`next`), tried a
    /// few times as any post is, rather than a new review being written; it goes on as one asked
    /// for by hand does. False for any other round.
    fn repost(&mut self, key: &str, head: &str) -> bool {
        let Some(r) = self.store.prs.get_mut(key).and_then(|t| t.rounds.last_mut()) else { return false };
        if r.stage != ReviewStage::Failed || r.head != head || !r.parts.iter().any(|p| p.state == PartState::Done) {
            return false;
        }
        r.stage = ReviewStage::Reviewing;
        r.ended_at = None;
        r.note = None;
        r.by_hand = true;
        self.save();
        true
    }

    /// Takes PR `key`'s latest round up again in its session, which it gave up on and which went
    /// on after: one it never found in time that has shown since, looked for everywhere here, or
    /// one it took for stopped that did anything since (`Tracked::take_up`), as Try again asks,
    /// rather than have the review written again. It goes on as one asked for by hand does.
    fn take_up(&mut self, key: &str, board: &Board, pr: &PrItem) -> bool {
        let Some(t) = self.store.prs.get_mut(key) else { return false };
        if !t.take_up(board, pr, true) {
            return false;
        }
        if let Some(r) = t.rounds.last_mut() {
            r.by_hand = true;
        }
        self.save();
        true
    }

    /// The send due for a PR of yours at `head` (`to_send`): to the session that opened it, once
    /// that runs here and doesn't wait on you, while sending is on and the PR is open.
    fn send_due(&self, board: &Board, cfg: &ReviewsCfg, pr: &PrItem, head: &str) -> Option<Job> {
        if !(pr.mine && cfg.own.send && pr.state.is_open()) {
            return None;
        }
        // A round asked for by hand goes back as an automatic one would.
        let r = self.store.prs.get(&pr_key(&pr.key))?.rounds.last().filter(|r| cfg.own.auto || r.by_hand)?;
        let (review, agent) = to_send(r, head)?;
        let (_, s) = origin(&board.lanes, pr).filter(|(_, s)| can_take(s) && !board.automation_blocked(&s.key))?;
        Some(Job::Send { pr: pr.clone(), head: head.to_string(), session: s.key.clone(), review, agent })
    }

    /// How a job went, at `now` (ms): kept, and saved. What should be said as a notice, for a
    /// review that couldn't be started or posted; one that didn't get done for its own reasons is
    /// said on its PR's card. A review that didn't start, or wasn't posted, can be tried again from
    /// the notice (`again`): nothing else would review that commit, or post its review.
    pub fn told(&mut self, job: &Job, went: Went, now: i64) -> Result<(), Said> {
        let pr = job.pr();
        let key = pr_key(&pr.key);
        self.busy.remove(&key);
        // A start that couldn't reach GitHub is tried again a few times before it's given up on;
        // one that got further, or found nothing to do, counts its tries afresh.
        let unreachable = matches!(went, Went::Unreachable(_));
        if matches!(job, Job::Review { .. }) && !unreachable {
            self.post_failures.remove(&key);
        }
        if went == Went::Later {
            return Ok(());
        }
        let t = self.store.prs.entry(key.clone()).or_default();
        let mut said: Result<(), Said> = Ok(());
        let again = |text: String| Said { text, again: Some(pr.key.clone()) };
        match (job, went) {
            (Job::Review { head, by_hand, .. }, Went::Started(parts)) => {
                for p in &parts {
                    if let Some(s) = &p.session {
                        t.sessions.insert(p.who.clone(), s.clone());
                    }
                }
                let failed: Vec<String> = parts.iter().filter(|p| p.state == PartState::Failed).map(|p| p.note.clone().unwrap_or_else(|| "it didn't start".into())).collect();
                let none = failed.len() == parts.len();
                let mut r = Round::new(head, if none { ReviewStage::Failed } else { ReviewStage::Reviewing }, now);
                if none {
                    r.note = Some(failed.join(" "));
                    said = Err(again(format!("Codealong couldn't start a review of {}: {}", pr.key, failed.join("; "))));
                }
                r.parts = parts;
                r.by_hand = *by_hand;
                t.rounds.push(r);
            }
            (Job::Review { head, .. }, Went::NotStarted(why) | Went::Unreachable(why)) => {
                if unreachable {
                    let failures = self.post_failures.entry(key.clone()).or_default();
                    *failures += 1;
                    if *failures < START_TRIES {
                        // No round yet: `due` starts it again once the wait is over.
                        self.next_at.insert(key, now + (RETRY_MS << (*failures - 1)));
                        return Err(again(format!("Codealong couldn't start a review of {}: {why} It tries again in a few minutes.", pr.key)));
                    }
                    self.post_failures.remove(&key);
                }
                let mut r = Round::new(head, ReviewStage::Failed, now);
                r.note = Some(why.clone());
                t.rounds.push(r);
                said = Err(again(format!("Codealong couldn't start a review of {}: {why}", pr.key)));
            }
            (Job::Review { head, .. }, went @ (Went::Posted(_) | Went::Drafted(_))) => {
                // Found there already: nothing to do for that commit.
                let (stage, link) = match went {
                    Went::Drafted(l) => (ReviewStage::Drafted, l),
                    Went::Posted(l) => (ReviewStage::Posted, l),
                    _ => unreachable!(),
                };
                let mut r = Round::new(head, stage, now);
                r.comment = Some(link);
                t.rounds.push(r);
            }
            (Job::Review { .. }, _) => {}
            (Job::Read { part, .. }, went) => {
                let Some(r) = t.rounds.last_mut().filter(|r| r.stage == ReviewStage::Reviewing) else { return Ok(()) };
                let Some(p) = r.parts.get_mut(*part) else { return Ok(()) };
                match went {
                    Went::Read { review, agent } => {
                        p.state = PartState::Done;
                        p.review = Some(review);
                        p.agent = Some(agent);
                    }
                    Went::Skipped(why) => {
                        p.state = PartState::Skipped;
                        p.note = Some(why);
                    }
                    Went::NotYet => {
                        self.next_at.insert(key, now + RETRY_MS);
                        return Ok(());
                    }
                    // Its turn may not show yet: read again later.
                    Went::NoReview if p.starting(now) => {
                        self.next_at.insert(key, now + RETRY_MS);
                        return Ok(());
                    }
                    Went::NoReview => fail(p, STOPPED),
                    Went::Failed(why) => fail(p, &why),
                    _ => {}
                }
            }
            (Job::CarryOn { part, .. }, went) => {
                let Some(r) = t.rounds.last_mut().filter(|r| r.stage == ReviewStage::Reviewing) else { return Ok(()) };
                let Some(p) = r.parts.get_mut(*part) else { return Ok(()) };
                match went {
                    Went::CarriedOn { session, pending, at } => {
                        // A background job's copy goes on in its place.
                        if let Some(s) = &session {
                            t.sessions.insert(p.who.clone(), s.clone());
                        }
                        p.session = session;
                        p.pending = pending;
                        p.carried_on.push(at);
                    }
                    Went::Failed(why) => fail(p, &why),
                    _ => {}
                }
            }
            (Job::Send { head, .. }, Went::Sent(session)) => {
                self.post_failures.remove(&key);
                if let Some(r) = t.rounds.iter_mut().rev().find(|r| r.head == *head) {
                    r.sent = Some(Sent { session, at: now, failed: None });
                }
            }
            (Job::Send { head, session, .. }, Went::PostFailed(why)) => {
                let failures = self.post_failures.entry(key.clone()).or_default();
                *failures += 1;
                if *failures < POST_TRIES {
                    self.next_at.insert(key, now + (RETRY_MS << (*failures - 1)));
                    return Err(Said::from(format!("Codealong couldn't hand its review of {} to the session that opened it: {why} It tries again in a minute or two.", pr.key)));
                }
                self.post_failures.remove(&key);
                if let Some(r) = t.rounds.iter_mut().rev().find(|r| r.head == *head) {
                    r.sent = Some(Sent { session: session.clone(), at: now, failed: Some(why.clone()) });
                }
                said = Err(Said::from(format!("Codealong couldn't hand its review of {} to the session that opened it: {why}", pr.key)));
            }
            (Job::Send { .. }, _) => {}
            (Job::Submit { head, verdict, .. }, Went::Submitted) => {
                self.post_failures.remove(&key);
                if let Some(r) = t.rounds.iter_mut().rev().find(|r| r.head == *head) {
                    r.submitted = Some(Submission { at: now, verdict: *verdict, failed: None });
                }
            }
            (Job::Submit { head, verdict, .. }, Went::NotSubmitted(why)) => {
                if let Some(r) = t.rounds.iter_mut().rev().find(|r| r.head == *head) {
                    r.submitted = Some(Submission { at: now, verdict: *verdict, failed: Some(why) });
                }
            }
            (Job::Submit { head, verdict, .. }, Went::PostFailed(why)) => {
                let failures = self.post_failures.entry(key.clone()).or_default();
                *failures += 1;
                if *failures < POST_TRIES {
                    self.next_at.insert(key, now + (RETRY_MS << (*failures - 1)));
                    return Err(Said::from(format!("Codealong couldn't {} {} for you: {why} It tries again in a minute or two.", verdict.verb(), pr.key)));
                }
                self.post_failures.remove(&key);
                if let Some(r) = t.rounds.iter_mut().rev().find(|r| r.head == *head) {
                    r.submitted = Some(Submission { at: now, verdict: *verdict, failed: Some(why.clone()) });
                }
                said = Err(Said::from(format!("Codealong couldn't {} {} for you: {why}", verdict.verb(), pr.key)));
            }
            (Job::Submit { .. }, _) => {}
            (Job::Post { .. }, Went::NotYet) => {
                self.next_at.insert(key, now + RETRY_MS);
                return Ok(());
            }
            (Job::Post { .. }, Went::PostFailed(why)) => {
                let failures = self.post_failures.entry(key.clone()).or_default();
                *failures += 1;
                if *failures < POST_TRIES {
                    self.next_at.insert(key, now + (RETRY_MS << (*failures - 1)));
                    return Err(Said::from(format!("Codealong couldn't post its review of {}: {why} It tries again in a minute or two.", pr.key)));
                }
                self.post_failures.remove(&key);
                if let Some(r) = t.rounds.last_mut().filter(|r| r.stage == ReviewStage::Reviewing) {
                    end(r, ReviewStage::Failed, Some(format!("It couldn't be posted: {why}")), now);
                }
                // Its Try again posts the review it has (`again`): nothing else would.
                said = Err(again(format!("Codealong couldn't post its review of {}: {why}", pr.key)));
            }
            (Job::Post { .. }, went) => {
                self.post_failures.remove(&key);
                let Some(r) = t.rounds.last_mut().filter(|r| r.stage == ReviewStage::Reviewing) else { return Ok(()) };
                match went {
                    Went::Posted(link) => {
                        r.comment = Some(link);
                        end(r, ReviewStage::Posted, None, now);
                    }
                    Went::Drafted(link) => {
                        r.comment = Some(link);
                        end(r, ReviewStage::Drafted, None, now);
                    }
                    Went::Kept(note) => end(r, ReviewStage::Kept, note, now),
                    Went::Skipped(why) => end(r, ReviewStage::Skipped, Some(why), now),
                    Went::Failed(why) | Went::NotStarted(why) => end(r, ReviewStage::Failed, Some(why), now),
                    _ => {}
                }
            }
        }
        self.next_at.remove(&key);
        self.save();
        said
    }
}

fn end(r: &mut Round, stage: ReviewStage, note: Option<String>, now: i64) {
    r.stage = stage;
    r.note = note;
    r.ended_at = Some(now);
}

/// The PR each session reviews, by the session's key: every session a round of a PR's reviews
/// asked, and the one kept for its next round; and among `sessions`, one no round holds whose
/// first prompt opens as a review of a PR here does, started since its first round
/// (`opens_review_of`). That one's start named no session and it showed only after its round
/// gave up on it, or once a later round had begun, so nothing took it up (`Tracked::take_up`);
/// it's a review of the PR all the same. Each is a helper of that PR (`Session::helper`), out of
/// your lists and queues while it doesn't need you, and listed on the PR's card.
pub fn reviewers<'a>(store: &'a Store, sessions: &[Session]) -> HashMap<String, &'a str> {
    let mut out = HashMap::new();
    for (pr, t) in &store.prs {
        for key in t.asked() {
            out.insert(key.to_string(), pr.as_str());
        }
    }
    // Only a prompt that opens as a review does is matched against each PR.
    let maybe = |f: &str| f.get(..OPENS.len()).is_some_and(|o| o.eq_ignore_ascii_case(OPENS));
    for s in sessions {
        let Some(first) = s.first_prompt.as_deref().filter(|f| s.visitor.is_none() && maybe(f)) else { continue };
        if out.contains_key(&s.key) {
            continue;
        }
        let started = s.started_at.unwrap_or(s.last_activity);
        let since = |t: &Tracked| t.rounds.iter().map(|r| r.at).min().is_some_and(|at| started >= at - 60_000);
        let review_of = |pr: &str| pr.rsplit_once('#').is_some_and(|(slug, number)| opens_review_of(first, slug, number));
        // An opening cut short in a long repo name can fit more than one: then it's none of them.
        let mut of = store.prs.iter().filter(|(pr, t)| since(t) && review_of(pr));
        if let (Some((pr, _)), None) = (of.next(), of.next()) {
            out.insert(s.key.clone(), pr.as_str());
        }
    }
    out
}

/// Puts each review of a PR of yours under the session it goes back to (`origin`), as a helper
/// of that session as well as of the PR (`Helper::parent`): Sessions lists it under that session,
/// whose panel lists it with its other helpers, and Mark as done on that session marks it done
/// too. A review of a PR that waits on your review has no session of yours to go under, and stays
/// the PR's alone.
pub fn nest_own(lanes: &mut [Lane], prs: &[PrItem]) {
    let back_to: HashMap<String, String> = prs.iter().filter(|p| p.mine).filter_map(|p| origin(lanes, p).map(|(_, s)| (pr_key(&p.key), s.key.clone()))).collect();
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

/// Whether session `from` is the automatic review of a PR of yours that goes back to session `to`
/// (`origin`), with `[reviews.own] send` on for the PR's repo. What it says to that session is
/// about the work the session is handed its review of anyway, most often an answer to what the
/// session asked it, so its message claims the session's own permission class, as a helper's
/// does (send.rs `from_its_helper`), rather than being held for you. Only a session a round of
/// the PR's reviews asked counts, in any round (`Tracked::asked`): one whose start named it, or,
/// for a harness that names its sessions itself, the one found where it started, of its account
/// (`found`). And only one whose first prompt opens in Codealong's words for a review of that PR
/// (`opens_review_of`): `found` also takes one that has the PR's link in brackets, as reviews
/// asked before that opening did, which a session of yours about the PR started then could
/// have. Not one taken for a review by its first prompt alone (`reviewers`), which any session
/// could open with; and only while it's still the PR's on the board, not one you made your own.
/// reviews.json is read only for a session the board takes for a review.
pub fn reviews_for(board: &Board, global: &ReviewsCfg, from: &str, to: &str) -> bool {
    let Some((_, review)) = board.session(from).filter(|(_, s)| s.helper.as_ref().is_some_and(|h| h.pr.is_some())) else { return false };
    asked_to_review(board, &crate::store::read_or_default(&path()), global, review, to)
}

/// `reviews_for`, with reviews.json read.
fn asked_to_review(board: &Board, store: &Store, global: &ReviewsCfg, review: &Session, to: &str) -> bool {
    let Some(key) = review.helper.as_ref().and_then(|h| h.pr.as_deref()) else { return false };
    let Some(pr) = board.prs.iter().find(|p| p.mine && pr_key(&p.key) == key) else { return false };
    let asked = store.prs.get(key).is_some_and(|t| t.asked().any(|s| s == review.key));
    let opens = review.first_prompt.as_deref().is_some_and(|f| opens_review_of(f, &pr.repo_slug, pr.number));
    asked && opens && origin(&board.lanes, pr).is_some_and(|(_, s)| s.key == to) && board.repo_settings.reviews(global, pr).own.send
}

/// Each PR's automatic review, for its card, from reviews.json (`Store`): its latest round, else
/// the round to come for a PR that reviews are on for (`wanted`); whether the PR has moved on
/// since that round, with another to come; and whether it's held out of what waits on you until
/// the agent's review is done. It's held while its commit has had no round yet, for `HOLD_MS` at
/// most from when your review was asked for or the PR last changed, and while its commit's round
/// is under way, for as long from when that began; a round that's done, or didn't get done, hands
/// it to you, so what waits on you is only what you can act on: a PR the agent approved before
/// its author pushed again is held for the new commits' review like any other, its approval
/// (`approved`) kept for its card to say. A PR of yours is never held: it says where its review
/// went back to, or why it hasn't, and whether the session that opened it can be sent it now.
pub fn apply(prs: &mut [PrItem], store: &Store, global: &ReviewsCfg, sessions: &crate::config::SessionsCfg, settings: &crate::teamfile::RepoSettings, lanes: &[Lane], now: i64) {
    for pr in prs {
        let cfg = settings.reviews(global, pr);
        let cfg = cfg.as_ref();
        let t = store.prs.get(&pr_key(&pr.key));
        let blocked = review_blocked(lanes, sessions, pr, &seat_for(cfg, t, lanes, pr));
        let wants_review = wanted(cfg, lanes, pr);
        let wanted = wants_review && !blocked;
        let head = pr.head_oid.clone().unwrap_or_default();
        let since = pr.requested_at.unwrap_or(0).max(pr.updated_at);
        let own = pr.mine;
        let approves = t.filter(|_| wants(pr)).and_then(approved);
        let paused = blocked && (wants_review || (own && cfg.own.send));
        let holds = wanted
            && !own
            && match t.and_then(|t| t.rounds.iter().rev().find(|r| r.head == head)) {
                // Until its review is submitted as yours, when it's to be.
                Some(r) => (r.stage == ReviewStage::Reviewing && now - r.at < HOLD_MS) || (submit_due(cfg, pr, r, &head).is_some() && now - r.at < HOLD_MS),
                None => now - since < HOLD_MS,
            };
        pr.auto_review = match t.and_then(|t| t.rounds.last().map(|r| (t, r))) {
            Some((t, r)) => Some(AutoReview {
                stage: r.stage,
                head: r.head.clone(),
                parts: parts_of(r),
                at: r.ended_at.unwrap_or(r.at),
                comment: r.comment.clone().or_else(|| t.rounds.iter().rev().find_map(|r| r.comment.clone())),
                note: {
                    let note = if own { back_note(r, cfg, &head, paused) } else { r.submitted.as_ref().and_then(not_submitted).or_else(|| r.note.clone()) };
                    if paused { Some(note.map_or_else(|| FLAGGED_REVIEW.into(), |n| format!("{n} {FLAGGED_REVIEW}"))) } else { note }
                },
                suggests: round_verdict(r),
                approves: approves.map(str::to_string),
                submitted: r.submitted.as_ref().filter(|s| s.failed.is_none()).map(|s| s.verdict),
                submitted_at: r.submitted.as_ref().filter(|s| s.failed.is_none()).map(|s| s.at),
                rounds: t.rounds.len(),
                behind: wanted && head != r.head,
                holds,
                own,
                sent: r.sent.as_ref().filter(|s| s.failed.is_none()).map(|s| s.session.clone()),
                sent_at: r.sent.as_ref().filter(|s| s.failed.is_none()).map(|s| s.at),
                sendable: (own && r.stage == ReviewStage::Kept).then(|| origin(lanes, pr)).flatten().filter(|(_, s)| can_take(s)).map(|(_, s)| s.key.clone()),
            }),
            None if wants_review => Some(AutoReview { stage: if blocked { ReviewStage::Skipped } else { ReviewStage::Queued }, head, parts: Vec::new(), at: since, comment: None, note: blocked.then(|| FLAGGED_REVIEW.into()), rounds: 0, behind: false, holds, own, sent: None, sent_at: None, sendable: None, suggests: None, approves: None, submitted: None, submitted_at: None }),
            None => None,
        };
    }
}

/// The commit the agent's latest written review of a PR approved, when that review approves it:
/// the PR's commit, when it waits on your sign-off; an earlier one, while the commits since are
/// reviewed. A round that wrote nothing (under way, passed on, never done) doesn't count, so a
/// review of new commits still to come or cut short leaves the earlier approval standing.
fn approved(t: &Tracked) -> Option<&str> {
    let r = t.rounds.iter().rev().find(|r| r.parts.iter().any(|p| p.state == PartState::Done && p.review.is_some()))?;
    (round_verdict(r) == Some(Verdict::Approve)).then_some(r.head.as_str())
}

/// A round's reviewers, each with its session, for a PR's card and for `codealong pr review`.
fn parts_of(r: &Round) -> Vec<ReviewPart> {
    r.parts.iter().map(|p| ReviewPart { who: p.who.clone(), session: p.session.clone(), state: p.state, note: p.note.clone() }).collect()
}

/// Why a round's review wasn't submitted as yours, when it was to be and wasn't.
fn not_submitted(s: &Submission) -> Option<String> {
    let why = s.failed.as_ref()?;
    Some(match s.verdict {
        Verdict::Approve => format!("It wasn't approved for you: {why}"),
        Verdict::RequestChanges => format!("Changes weren't requested for you: {why}"),
        Verdict::Comment => format!("It wasn't submitted as a comment for you: {why}"),
    })
}

/// Why a review of a PR of yours hasn't gone back to its session, while it hasn't: what the
/// round says (it found nothing to raise, the PR ended), else that sending is off, it was given
/// up on, the PR moved on, or it waits for its session.
fn back_note(r: &Round, cfg: &ReviewsCfg, head: &str, paused: bool) -> Option<String> {
    if r.note.is_some() || r.stage != ReviewStage::Kept {
        return r.note.clone();
    }
    match &r.sent {
        Some(Sent { failed: Some(why), .. }) => Some(format!("It couldn't be handed to the session that opened the PR: {why}")),
        Some(_) => None,
        None if !cfg.own.send || !(cfg.own.auto || r.by_hand) => Some("Handing reviews back is off: send it from here.".into()),
        None if paused => None,
        None if r.head != head => Some("New commits came in since, so it isn't sent: the next review is.".into()),
        None => Some("It goes to the session that opened the PR once that runs here and doesn't wait on you.".into()),
    }
}

/// A round's review as its agent wrote it, for the PR's panel to show whole: its text, and its
/// comments on particular lines (`parse_reply`), with where it went and the session that wrote it.
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WrittenReview {
    /// The commit it reviewed.
    pub head: String,
    pub stage: ReviewStage,
    /// When it was done, or asked for while it isn't.
    pub at: i64,
    pub session: Option<String>,
    /// Who wrote it: its harness, and its model where that's known.
    pub agent: Option<String>,
    /// Where it went on GitHub: the PR's files, beside a draft of yours, or the comment.
    pub link: Option<String>,
    pub summary: String,
    pub comments: Vec<LineComment>,
    /// What its agent would do with the PR (`verdict_of`).
    pub verdict: Option<Verdict>,
}

/// The reviews a PR's rounds wrote, the latest first; a round whose agent wrote none (it's under
/// way, passed or didn't get done) has none here.
pub fn written(store: &Store, key: &str) -> Vec<WrittenReview> {
    let Some(t) = store.prs.get(&pr_key(key)) else { return Vec::new() };
    t.rounds
        .iter()
        .rev()
        .filter_map(|r| {
            let p = r.parts.iter().find(|p| p.review.is_some())?;
            let (summary, comments) = parse_reply(p.review.as_deref().unwrap_or_default());
            let verdict = verdict_of(p.review.as_deref().unwrap_or_default());
            Some(WrittenReview { head: r.head.clone(), stage: r.stage, at: r.ended_at.unwrap_or(r.at), session: p.session.clone(), agent: p.agent.clone(), link: r.comment.clone(), summary, comments, verdict })
        })
        .collect()
}

// ---- What the session is told, and what's posted ----

/// A commit as people read it.
fn short(sha: &str) -> &str {
    &sha[..sha.len().min(7)]
}

/// The mark a posted review carries, unseen on GitHub: which PR and commit it's of.
pub fn marker(pr_key_: &str, head: &str) -> String {
    format!("{}{} {head} -->", crate::github::REVIEW_MARK, pr_key(pr_key_))
}

/// What's handed to a review to read, as text refers to kept files (attachments.rs).
pub struct Reading {
    /// The PR as Markdown (`github::reading_md`).
    pub md: String,
    /// Its whole diff.
    pub diff: String,
    /// What changed since the commit its last review was of, where that follows on.
    pub changes: Option<String>,
}

/// How the review's last message is laid out, written to `to`: a summary, then the comments on
/// particular lines in a block Codealong reads (`parse_reply`).
fn layout(to: &str) -> String {
    format!("So make that message the review and nothing else, in GitHub Markdown, written to {to}: first a line on what you make of it, then the findings that aren't about particular lines, most serious first. Then put each finding about a particular line in a fenced block tagged `review-comments` holding a JSON array of objects with `path` (the file as the diff names it), `line` (the line's number in the file's new version: the diff's left column) and `body` (the comment, saying what to do about it). Only a line the diff numbers can take a comment. {VERDICT}")
}

/// The line a review ends on, saying what its agent would do with the PR (`verdict_of`).
const VERDICT: &str = "Last, on a line of its own, say what you'd do with it: `Verdict: approve` when nothing you found needs fixing before it merges (minor points at most), `Verdict: request changes` when something must be fixed first, or `Verdict: comment` when you can't tell.";

/// Where a review goes once it's done: where config.toml posts it, or back to the agent that wrote
/// a PR of yours (`[reviews.own]`), named by its session's harness and model.
#[derive(Debug, Clone, PartialEq)]
pub enum Goes {
    Post(ReviewPost),
    Back(String),
}

/// Where `pr`'s review goes.
fn goes(cfg: &ReviewsCfg, lanes: &[Lane], pr: &PrItem) -> Goes {
    if pr.mine {
        Goes::Back(origin(lanes, pr).map(|(_, s)| agent_of(s)).unwrap_or_else(|| "the agent that wrote it".into()))
    } else {
        Goes::Post(cfg.post)
    }
}

/// Who a session's agent is, as a review names its author: its harness, and its model where
/// that's known.
fn agent_of(s: &Session) -> String {
    let harness = crate::harness::adapter(s.harness).harness().word();
    s.model.as_deref().map(|m| format!("{harness}, {m}")).unwrap_or_else(|| harness.to_string())
}

/// Where the review's last message goes, and how to lay it out for that.
fn last_message(login: &str, goes: &Goes) -> String {
    match goes {
        Goes::Post(ReviewPost::Draft) => format!("Codealong puts your last message on the PR as a review on {login}'s GitHub account, left as a draft that only {login} sees until they finish it there, with each line comment on its line and a line saying an agent wrote it. {}", layout("the PR's author")),
        Goes::Post(ReviewPost::Comment) => format!("Codealong posts your last message on the PR as a comment from {login}'s GitHub account, with each line comment written in under a link to its line, and a line saying an agent wrote it. {}", layout("the PR's author")),
        Goes::Post(ReviewPost::None) => format!("Your last message is the review, for {login} to read here; nothing is posted. Make it the review and nothing else, in Markdown: a line on what you make of it, then each finding, most serious first, with its file and line and what to do about it. {VERDICT}"),
        Goes::Back(who) => format!("Codealong hands your last message to the agent that wrote the PR ({who}) to act on, unless you'd approve it, and shows it to {login}; nothing goes on GitHub. {}", layout("that agent")),
    }
}

const SKIP: &str = "If you can't review it, make your last message `SKIP: ` and why, and nothing is posted.";
const HANDS_OFF: &str = "Don't change any files, commit, push, or post on GitHub.";
/// What the numbered diffs (`numbered`) are, said where they're handed over.
const NUMBERED: &str = "Each line of a diff has its number in the file's new version in a column on its left; a removed line has none.";

/// The first prompt of a review's own session. `holds`: its folder has the PR's branch checked
/// out. `since`: the commit an earlier review of it was of, by a session that's gone, with
/// `files.changes` saying what changed since where that follows on.
#[allow(clippy::too_many_arguments)]
pub fn first_prompt(pr: &PrItem, head: &str, since: Option<&str>, holds: bool, login: &str, goes: &Goes, instructions: &str, files: &Reading) -> String {
    let base = pr.base.as_deref().unwrap_or("its base branch");
    let by = pr.author.as_deref().map(|a| format!(", by {a}")).unwrap_or_default();
    let why = match goes {
        Goes::Back(who) => format!("Codealong started you to review it for {login}, who opened it with another agent ({who}): your review goes back to that agent, to act on."),
        Goes::Post(_) if pr.review_requested => format!("Codealong started you to review it for {login}, whose review it waits on."),
        Goes::Post(_) => format!("Codealong started you to review it for {login}, who asked for a review of it."),
    };
    let mut out = format!("{} “{}” ({}){by}: it merges {} into {base}, and is at commit {}. {why}\n\n", opening(pr), pr.title.trim(), pr.url, pr.head, short(head));
    out += &format!("Its description, reviews, comments and changed files are in {}, and its whole diff is in {}. {NUMBERED} ", files.md, files.diff);
    out += if holds {
        "This folder has its branch checked out, so the code here is the PR's.\n\n".to_string()
    } else {
        format!("This folder is a checkout of {} without its branch: read the code around the changes here, and the changes themselves in the diff.\n\n", pr.repo_slug)
    }
    .as_str();
    if let Some(s) = since {
        out += &format!("An earlier review of commit {} is among the comments in the Markdown. ", short(s));
        match &files.changes {
            Some(c) => out += &format!("What changed since then is in {c}. "),
            None => out += "Its branch was rewritten since then, so review the whole diff. ",
        }
        out += "Start with which of its findings are fixed and which aren't.\n\n";
    }
    out += &format!("{}\n\n{HANDS_OFF} {} {SKIP}", instructions.trim(), last_message(login, goes));
    out
}

/// What a review's session is sent when the PR has new commits since the one it last reviewed.
pub fn again_prompt(pr: &PrItem, head: &str, since: Option<&str>, login: &str, goes: &Goes, instructions: &str, files: &Reading) -> String {
    let mut out = match since {
        Some(s) => format!("#{} “{}” has new commits since you reviewed {}: it's at {} now. ", pr.number, pr.title.trim(), short(s), short(head)),
        None => format!("Review #{} “{}” again; it's at {} now. ", pr.number, pr.title.trim(), short(head)),
    };
    out += &format!("{} has it as it is now, with what's been said on it since, and {} its whole diff. {NUMBERED} ", files.md, files.diff);
    match (since, &files.changes) {
        (Some(s), Some(c)) => out += &format!("What changed since {} is in {c}.\n\n", short(s)),
        (Some(_), None) => out += "Its branch was rewritten since then (rebased or force-pushed), so review the whole diff again.\n\n",
        (None, _) => out += "\n\n",
    }
    out += &format!(
        "Review it again, starting with which of your earlier findings are fixed and which aren't, then anything new, as you were asked to:\n\n{}\n\n{HANDS_OFF} {} {SKIP}",
        instructions.trim(),
        last_message(login, goes)
    );
    out
}

/// The review a session's turn ended on: its last run of replies, once that run came after `at`.
/// None while the turn hasn't written one, or ended on something else (a call, a prompt). A reply
/// its record keeps no time for is as new as the prompt it answers, the session's last
/// (`prompted`): Cursor's command line keeps a time only for each prompt, the minute it was written
/// in, so a prompt from the minute of `at` on will do.
pub fn reply_after(items: &[Item], at: i64, prompted: i64) -> Option<String> {
    let end = items.iter().rposition(|i| i.kind != "note")?;
    let start = items[..=end].iter().rposition(|i| i.kind != "agent").map_or(0, |i| i + 1);
    let run = &items[start..=end];
    let since = |i: &Item| if i.at > 0 { i.at >= at } else { prompted >= at - at.rem_euclid(60_000) };
    if run.is_empty() || run.iter().any(|i| i.kind != "agent" || !since(i)) {
        return None;
    }
    let text = run.iter().map(|i| i.text.trim()).filter(|t| !t.is_empty()).collect::<Vec<_>>().join("\n\n");
    (!text.is_empty()).then_some(text)
}

/// What a line of a diff is (`walk`).
#[derive(Debug, Clone, Copy, PartialEq)]
enum DiffLine<'a> {
    /// A file's header, or a hunk's.
    Header,
    /// Removed: it's in the old version only.
    Old,
    /// In the new version, added or around a change: the file and its number there.
    New(&'a str, u64),
}

/// Each line of a diff and what it is. A file is named by its `+++ b/` line, else by the part of
/// its `diff --git` line after ` b/` (GitHub's compare files have only that); a hunk's lines are
/// numbered from its `@@ -a,b +c,d @@` header's `c`.
fn walk(diff: &str) -> Vec<(&str, DiffLine<'_>)> {
    let mut out = Vec::new();
    let mut path = "";
    let mut next: Option<u64> = None;
    for line in diff.lines() {
        if let Some(rest) = line.strip_prefix("diff --git ") {
            path = rest.rsplit_once(" b/").map_or(rest, |(_, b)| b);
            next = None;
            out.push((line, DiffLine::Header));
            continue;
        }
        if line.starts_with("@@") {
            next = line.split_whitespace().find_map(|w| w.strip_prefix('+')).and_then(|w| w.split(',').next()).and_then(|n| n.parse().ok());
            out.push((line, DiffLine::Header));
            continue;
        }
        let Some(n) = next else {
            if let Some(p) = line.strip_prefix("+++ ").filter(|p| *p != "/dev/null") {
                path = p.strip_prefix("b/").unwrap_or(p);
            }
            out.push((line, DiffLine::Header));
            continue;
        };
        match line.as_bytes().first() {
            // An empty line in a hunk is a context line whose space was lost.
            Some(b'+') | Some(b' ') | None => {
                out.push((line, DiffLine::New(path, n)));
                next = Some(n + 1);
            }
            _ => out.push((line, DiffLine::Old)),
        }
    }
    out
}

/// A diff for an agent to read, each line with its number in the file's new version in a column
/// on its left, so a comment can name its line without counting from a hunk's header.
pub fn numbered(diff: &str) -> String {
    walk(diff)
        .into_iter()
        .map(|(line, kind)| match kind {
            DiffLine::Header => format!("{line}\n"),
            DiffLine::Old => format!("      {line}\n"),
            DiffLine::New(_, n) => format!("{n:>5} {line}\n"),
        })
        .collect()
}

/// The lines a review can comment on, by file: the ones the PR's diff shows in the new version.
/// GitHub refuses a comment on any other.
pub fn commentable(diff: &str) -> HashMap<String, HashSet<u64>> {
    let mut lines: HashMap<String, HashSet<u64>> = HashMap::new();
    for (_, kind) in walk(diff) {
        if let DiffLine::New(path, n) = kind {
            lines.entry(path.to_string()).or_default().insert(n);
        }
    }
    lines
}

/// The review's text and its comments on particular lines, from its last message: the
/// `review-comments` block taken out of it (`LAYOUT`). A block that doesn't read leaves the
/// message whole, so nothing it says is lost.
pub fn parse_reply(reply: &str) -> (String, Vec<LineComment>) {
    const FENCE: &str = "```review-comments";
    let whole = || (without_verdict(reply), Vec::new());
    let Some(start) = reply.find(FENCE) else { return whole() };
    let after = &reply[start + FENCE.len()..];
    let Some(end) = after.find("\n```") else { return whole() };
    let Ok(serde_json::Value::Array(items)) = serde_json::from_str::<serde_json::Value>(after[..end].trim()) else { return whole() };
    let mut comments = Vec::new();
    for item in &items {
        let text = |k: &str| item.get(k).and_then(serde_json::Value::as_str).map(str::trim).filter(|t| !t.is_empty());
        let line = item.get("line").and_then(|l| l.as_u64().or_else(|| l.as_str()?.trim().parse().ok()));
        let (Some(path), Some(line), Some(body)) = (text("path"), line, text("body")) else { return whole() };
        comments.push(LineComment { path: path.trim_start_matches("b/").to_string(), line, body: body.to_string() });
    }
    let rest = after[end + 4..].trim_start_matches(|c| c != '\n').trim();
    let summary = [reply[..start].trim(), rest].into_iter().filter(|t| !t.is_empty()).collect::<Vec<_>>().join("\n\n");
    (without_verdict(&summary), comments)
}

/// Why the agent passed on a review, when its reply says `SKIP:`.
pub fn skipped(reply: &str) -> Option<String> {
    let t = reply.trim();
    let rest = t.get(..5).filter(|p| p.eq_ignore_ascii_case("skip:")).map(|_| t[5..].trim())?;
    Some(if rest.is_empty() { "Its agent couldn't review it.".to_string() } else { rest.to_string() })
}

/// A link to a line of a file at a commit, which GitHub shows as the code itself when it's on a
/// line of its own in a comment on the same repo.
fn line_link(pr: &PrItem, head: &str, c: &LineComment) -> String {
    let path: Vec<String> = c.path.split('/').map(|p| urlencoding::encode(p).into_owned()).collect();
    format!("https://github.com/{}/blob/{head}/{}#L{}", pr.repo_slug, path.join("/"), c.line)
}

/// Comments on particular lines written into a review's text, each under a link to its line.
fn written_in(pr: &PrItem, head: &str, comments: &[LineComment]) -> String {
    comments.iter().map(|c| format!("**`{}` line {}**\n{}\n\n{}", c.path, c.line, line_link(pr, head, c), c.body.trim())).collect::<Vec<_>>().join("\n\n")
}

/// A review's text as it's posted: `text`, cut to what GitHub takes, then a line saying an agent
/// wrote it, of which commit, and the unseen mark (`marker`).
fn signed(text: &str, pr: &PrItem, head: &str, agent: &str, how: &str) -> String {
    signed_with(text, head, agent, how, &marker(&pr.key, head))
}

/// `signed`, with `mark` as its unseen mark.
fn signed_with(text: &str, head: &str, agent: &str, how: &str, mark: &str) -> String {
    let mut text = text.trim().to_string();
    if text.chars().count() > COMMENT_MAX {
        let cut = crate::util::truncate(&text, COMMENT_MAX);
        text = format!("{}\n\n…(cut short: the whole review is longer than GitHub takes)", cut.trim_end());
    }
    format!("{text}\n\n<sub>An agent's review ({agent}) of {}, {how} by Codealong.</sub>\n{mark}", short(head))
}

/// The comment posted for a review: its text, then its comments on particular lines, each under a
/// link to its line.
pub fn comment_body(summary: &str, comments: &[LineComment], pr: &PrItem, head: &str, agent: &str) -> String {
    review_text(summary, comments, pr, head, agent, "posted")
}

/// A review's text with its comments on particular lines written in, signed as `how` it went.
fn review_text(summary: &str, comments: &[LineComment], pr: &PrItem, head: &str, agent: &str, how: &str) -> String {
    let lines = written_in(pr, head, comments);
    let text = [summary.trim(), lines.as_str()].into_iter().filter(|t| !t.is_empty()).collect::<Vec<_>>().join("\n\n");
    signed(&text, pr, head, agent, how)
}

/// What a draft review does on GitHub: begins one, or adds to the draft you have (`add_to`), with
/// `body` as its text and `threads` on their lines.
#[derive(Debug, Clone, PartialEq)]
pub struct DraftPlan {
    pub add_to: Option<String>,
    pub body: String,
    pub threads: Vec<LineComment>,
}

/// How a review goes into a draft on your account. A comment goes on its line when the PR's diff
/// shows that line (`lines`, `commentable`) and the draft is at the reviewed commit; GitHub takes
/// no other, so the rest are written into its text under links to their lines. A draft you have
/// already keeps what you wrote, the review's text going after it, and one begun on another
/// commit takes every comment in its text, since its lines are numbered by that commit.
pub fn draft_plan(summary: &str, comments: Vec<LineComment>, lines: &HashMap<String, HashSet<u64>>, pending: Option<&PendingReview>, pr: &PrItem, head: &str, agent: &str, round: i64) -> DraftPlan {
    let same = pending.is_none_or(|p| p.commit.as_deref() == Some(head));
    let (threads, rest): (Vec<LineComment>, Vec<LineComment>) = comments.into_iter().partition(|c| same && lines.get(&c.path).is_some_and(|l| l.contains(&c.line)));
    let others = written_in(pr, head, &rest);
    let others = if others.is_empty() { others } else { format!("**On other lines**\n\n{others}") };
    let text = [summary.trim(), others.as_str()].into_iter().filter(|t| !t.is_empty()).collect::<Vec<_>>().join("\n\n");
    let ours = signed_with(&text, head, agent, "drafted", &draft_mark(&pr.key, head, round));
    let body = match pending.map(|p| p.body.trim()).filter(|b| !b.is_empty()) {
        Some(mine) => format!("{mine}\n\n---\n\n{ours}"),
        None => ours,
    };
    DraftPlan { add_to: pending.map(|p| p.id.clone()), body, threads }
}

/// The mark a review drafted for round `round` carries: the PR's and commit's (`marker`), which
/// tells another Mac it's reviewed, then the round's, which tells a draft that has this round's
/// review from one that has another of the same commit.
pub fn draft_mark(pr_key_: &str, head: &str, round: i64) -> String {
    format!("{} <!-- round {round} -->", marker(pr_key_, head))
}

/// Where a PR's draft review is seen: its files, beside the lines it comments on.
fn draft_link(pr: &PrItem) -> String {
    format!("{}/files", pr.url)
}

/// Puts a review into a draft on your account (`draft_plan`), reading the PR's diff and your draft
/// as they are now. A draft that has this round's review already (its mark, `draft_mark`) is left
/// as it is: a post tried again after adding part of it doesn't add it twice. Another round's
/// review of the same commit, asked for by hand, goes in after it.
fn draft(pr: &PrItem, head: &str, summary: &str, comments: Vec<LineComment>, agent: &str, round: i64) -> Went {
    let diff = match crate::github::pr_diff(&pr.repo_slug, pr.number) {
        Ok(d) => d,
        Err(e) => return Went::PostFailed(format!("its diff couldn't be read: {e:#}")),
    };
    let (id, pending) = match crate::github::pending_review(&pr.repo_slug, pr.number) {
        Ok(p) => p,
        Err(e) => return Went::PostFailed(format!("{e:#}")),
    };
    if pending.as_ref().is_some_and(|p| p.body.contains(&draft_mark(&pr.key, head, round))) {
        return Went::Drafted(draft_link(pr));
    }
    let plan = draft_plan(summary, comments, &commentable(&diff), pending.as_ref(), pr, head, agent, round);
    let done = match &plan.add_to {
        None => crate::github::draft_review(&id, head, &plan.body, &plan.threads),
        Some(review) => crate::github::add_to_draft(review, &plan.body, &plan.threads),
    };
    match done {
        Ok(()) => Went::Drafted(draft_link(pr)),
        Err(e) => Went::PostFailed(format!("{e:#}")),
    }
}

// ---- Doing the jobs ----

/// Where a review's session may be, as `[reviews] start` says, first choice first, for a harness
/// that has a background service of its own (`service`) or not. A new one starts in the first its
/// account offers (`review_place`), and one that isn't running goes on in the first its account
/// can go on in (`hand`). Only `background` and `terminal` ever open a terminal: `background` for
/// a harness with neither a service nor Codealong's adapter, as the account's Background does.
pub fn review_places(start: ReviewStart, service: bool) -> &'static [Place] {
    match start {
        ReviewStart::NoTerminal | ReviewStart::Background if service => &[Place::Background, Place::Codealong],
        ReviewStart::NoTerminal | ReviewStart::Codealong => &[Place::Codealong],
        ReviewStart::Background => &[Place::Codealong, Place::Background],
        ReviewStart::Terminal => &[Place::Terminal],
    }
}

/// The first of `review_places` that's among `can`.
fn first_of(start: ReviewStart, service: bool, can: &[Place]) -> Option<Place> {
    review_places(start, service).iter().copied().find(|p| can.contains(p))
}

fn has_service(a: &AccountCfg) -> bool {
    crate::harness::of(a).is_some_and(|h| h.runs_in_background())
}

/// Where a new review on `a` starts, as `[reviews] start` says; none when `a` can't start one
/// there.
pub fn review_place(a: &AccountCfg, start: ReviewStart) -> Option<Place> {
    first_of(start, has_service(a), &crate::start::places_of(a, &crate::util::home()).unwrap_or_default())
}

/// Where `[reviews] start` has reviews start, as "can't start a review …" says it.
fn start_words(start: ReviewStart) -> &'static str {
    match start {
        ReviewStart::NoTerminal => "without a terminal",
        ReviewStart::Codealong => "in Codealong",
        ReviewStart::Background => "in the background",
        ReviewStart::Terminal => "in a terminal",
    }
}

/// The account, model and effort a new review runs on, and where: `seat`'s account (or the one
/// Auto or the routing group it names picks among the accounts that can start one where
/// `[reviews] start` says, of another harness than `avoid` when one of them can), model and
/// effort, else the effort the group's member names.
fn reviewer_place(cfg: &Config, board: &Board, seat: &Seat, avoid: Option<Harness>) -> Result<(String, Option<String>, Option<String>, Place), String> {
    let start = cfg.reviews.start;
    let can = |a: &AccountCfg| review_place(a, start).is_some();
    let asked = seat.account.as_deref().map(str::trim).filter(|a| !a.is_empty());
    let (name, model, effort) = match asked {
        None => {
            let picked = crate::start::resolve_group(cfg, Some(board), AUTO, &[], seat.model.as_deref(), avoid, true, now_ms(), can)
                .and_then(std::result::Result::ok)
                .ok_or_else(|| format!("no account can start a review {}; Settings › Automation says where reviews start", start_words(start)))?;
            (picked.account, seat.model.clone().or(picked.model), seat.effort.clone().or(picked.effort))
        }
        Some(n) if cfg.routing.groups.iter().any(|g| g.name == n) => {
            let picked = crate::start::resolve_group(cfg, Some(board), n, &[], seat.model.as_deref(), avoid, true, now_ms(), can)
                .and_then(std::result::Result::ok)
                .ok_or_else(|| format!("no account in {n} can start a review {}; Settings › Automation says where reviews start", start_words(start)))?;
            (picked.account, seat.model.clone().or(picked.model), seat.effort.clone().or(picked.effort))
        }
        Some(n) => (n.to_string(), seat.model.clone(), seat.effort.clone()),
    };
    let a = cfg.accounts.iter().find(|a| a.name == name).ok_or_else(|| format!("there's no account named {name}"))?;
    if a.disabled {
        return Err(format!("{name} is turned off in Settings › Accounts"));
    }
    let place = review_place(a, start).ok_or_else(|| format!("{name} can't start a review {}; Settings › Automation says where reviews start", start_words(start)))?;
    Ok((name, model, effort, place))
}

/// Links a review's session to its PR, so the PR's card lists it.
fn link(engine: &Handle, pr: &PrItem, session: &str) {
    match crate::prlinks::change(|s| s.set(&pr.key, session, true, now_ms())) {
        Ok(_) => engine.pr_links_changed(),
        Err(e) => eprintln!("codealong: couldn't link the review of {} to its session: {e:#}", pr.key),
    }
}

/// Keeps a reading's files where a start or a send hands them on, and says how text refers to
/// each.
fn keep_files(pr: &PrItem, head: &str, md: &str, diff: &str, changes: Option<&str>) -> Result<Reading, String> {
    let keep = |bytes: &str, name: String| crate::attachments::keep(bytes.as_bytes(), &name).map(|a| a.reference()).map_err(|e| format!("its diff couldn't be kept for it to read: {e:#}"));
    Ok(Reading {
        md: keep(md, format!("pr-{}.md", pr.number))?,
        diff: keep(&numbered(diff), format!("pr-{}.diff", pr.number))?,
        changes: changes.map(|c| keep(&numbered(c), format!("pr-{}-to-{}.diff", pr.number, short(head)))).transpose()?,
    })
}

/// Sends a session that has a turn to take `text`: as a message where it runs, else by going on
/// with it where `[reviews] start` lets its account (`review_places`), on the reviewer's `model`
/// and `effort` (none for the account's own), which a resume would otherwise set back to the
/// account's. The
/// session the turn is in, which a background job's copy changes, or where to look for it
/// (`Pending`) when that isn't said.
#[allow(clippy::too_many_arguments)]
fn hand(engine: &Handle, board: &Board, cfg: &Config, pr: &PrItem, key: &str, model: Option<&str>, effort: Option<&str>, text: String) -> Result<(Option<String>, Option<Pending>), String> {
    let Some((lane, s)) = board.session(key) else { return Err("its session is gone".into()) };
    if s.live {
        crate::send::tell(engine, key, &text).map_err(|e| format!("its session couldn't be sent it: {e:#}"))?;
        return Ok((Some(key.to_string()), None));
    }
    let start = cfg.reviews.start;
    let place = cfg.accounts.iter().find(|a| a.name == s.account).and_then(|a| first_of(start, has_service(a), &crate::start::resume_places(a, &crate::util::home())));
    let Some(place) = place else { return Err(format!("its session isn't running, and can't go on {}", start_words(start))) };
    let at = now_ms();
    let how = ResumeIn { place, model: model.map(str::to_string), effort: effort.map(str::to_string) };
    let r = crate::start::resume(board, cfg, key, &how, &crate::send::by_codealong(&text), &crate::send::Sender::Codealong).map_err(|e| format!("its session couldn't go on {}: {e:#}", start_words(start)))?;
    if let Some(k) = r.key.as_deref().filter(|k| *k != key) {
        link(engine, pr, k);
    }
    let pending = r.key.is_none().then(|| Pending { lane: lane.id.clone(), account: s.account.clone(), at });
    Ok((r.key, pending))
}

/// Asks the reviewer for its part in a round: its session for the PR is sent the new commits,
/// else a new session starts on the first prompt.
#[allow(clippy::too_many_arguments)]
fn ask(engine: &Handle, board: &Board, cfg: &Config, pr: &PrItem, head: &str, since: Option<&str>, login: &str, files: &Reading, seat: &Seat) -> Part {
    let at = now_ms();
    // A blocked delivery mustn't fall through to a fresh reviewer if its config and the board
    // straddle a settings change. The send uses this same config.
    if let Some((_, s)) = seat.session.as_deref().and_then(|key| board.session(key)) {
        if let Err(e) = crate::send::ensure_not_flagged(cfg, s, &crate::send::Sender::Codealong) {
            return Part::failed(&seat.who, format!("{e:#}"), at);
        }
    }
    let can_switch = seat.account.as_deref().is_none_or(|n| cfg.routing.groups.iter().any(|g| g.name == n));
    if let Some(key) = seat.session.as_deref().filter(|k| board.session(k).is_some() && !(can_switch && session_at_limit(board, &cfg.accounts, k, seat.model.as_deref(), at))) {
        let reviews = board.repo_settings.reviews(&cfg.reviews, pr);
        match hand(engine, board, cfg, pr, key, seat.model.as_deref(), seat.effort.as_deref(), again_prompt(pr, head, since, login, &goes(&reviews, &board.lanes, pr), &instructions_for(&reviews, pr), files)) {
            Ok((session, pending)) => return Part::asked(&seat.who, session, pending, at),
            // A session that can't go on is replaced by a new one, and one still running but not
            // at work is closed rather than left beside it: one waiting at what its harness asks
            // before it starts takes nothing typed.
            Err(e) => {
                eprintln!("codealong: {}'s review of {} starts afresh: {e}", seat.who, pr.key);
                if board.session(key).is_some_and(|(_, s)| s.live && s.status != Status::Working) {
                    if let Err(e) = crate::idle::close_one(board, cfg, key) {
                        eprintln!("codealong: {}'s old review session of {} stays open: {e:#}", seat.who, pr.key);
                    }
                }
            }
        }
    }
    let spec = match new_start(board, cfg, pr, head, since, login, files, seat) {
        Ok(s) => s,
        Err(e) => return Part::failed(&seat.who, e, at),
    };
    let params = codealong_peer::api::params::Start { spec: spec.clone() };
    match crate::actions::local(engine, &crate::actions::From::Here, codealong_peer::api::START, &spec.lane_id, params).and_then(crate::actions::typed::<crate::start::Started>) {
        Ok(started) => {
            if let Some(k) = &started.key {
                link(engine, pr, k);
            }
            let pending = started.key.is_none().then(|| Pending { lane: spec.lane_id.clone(), account: spec.account.clone(), at });
            Part::asked(&seat.who, started.key, pending, at)
        }
        Err(e) => Part::failed(&seat.who, e, at),
    }
}

/// Starts a round: reads the PR, then asks the reviewer (`ask`). One asked for `by_hand` starts
/// whatever `[reviews] auto` says, while the PR is open, and reviews a commit reviewed already.
fn review(engine: &Handle, pr: &PrItem, head: &str, since: Option<&str>, seat: &Seat, by_hand: bool) -> Went {
    let board = engine.board();
    let cfg = match crate::config::current() {
        Ok(c) => c,
        Err(e) => return Went::NotStarted(format!("{e:#}")),
    };
    let Some(pr) = board.prs.iter().find(|p| p.key == pr.key) else { return Went::Later };
    let reviews = board.repo_settings.reviews(&cfg.reviews, pr);
    let still = if by_hand { pr.state.is_open() } else { wanted(&reviews, &board.lanes, pr) };
    if pr.head_oid.as_deref() != Some(head) || !still {
        return Went::Later;
    }
    let Some(seat) = review_seat(&board, pr, seat.clone(), by_hand) else { return Went::Later };
    let login = board.github_login.clone().unwrap_or_else(|| "you".into());
    let (md, diff, detail) = match crate::github::for_reading(pr) {
        Ok(r) => r,
        Err(e) => {
            let why = crate::util::words(e);
            return if crate::github::unreachable(&why) { Went::Unreachable(why) } else { Went::NotStarted(why) };
        }
    };
    // Reviewed already, from another Mac or before a restart.
    let mark = marker(&pr.key, head);
    if let Some(c) = detail.comments.iter().find(|c| !by_hand && !pr.mine && c.mine && c.body.contains(&mark)) {
        return match reviews.post {
            ReviewPost::Draft => Went::Drafted(draft_link(pr)),
            _ => Went::Posted(c.url.clone().unwrap_or_else(|| pr.url.clone())),
        };
    }
    let changes = since.and_then(|s| crate::github::changes_since(&pr.repo_slug, s, head).ok().flatten());
    let files = match keep_files(pr, head, &md, &diff, changes.as_deref()) {
        Ok(f) => f,
        Err(e) => return Went::NotStarted(e),
    };
    let fresh = engine.board();
    let Some(seat) = review_seat(&fresh, pr, seat, by_hand) else { return Went::Later };
    Went::Started(vec![ask(engine, &fresh, &cfg, pr, head, since, &login, &files, &seat)])
}

/// A new session's start for a review: on the reviewer's account (Auto's for none) and model,
/// where `[reviews] start` says, in the worktree `side::pr_lane` picks, on the first prompt.
#[allow(clippy::too_many_arguments)]
fn new_start(board: &Board, cfg: &Config, pr: &PrItem, head: &str, since: Option<&str>, login: &str, files: &Reading, seat: &Seat) -> Result<StartSpec, String> {
    let reviews = board.repo_settings.reviews(&cfg.reviews, pr);
    let other_harness = if pr.mine { reviews.own.other_harness } else { reviews.other_harness };
    let avoid = seat.avoid.or_else(|| other_harness.then(|| pr_harness(&board.lanes, pr)).flatten());
    let (account, model, effort, place) = reviewer_place(cfg, board, seat, avoid)?;
    let (lane, holds) = crate::side::pr_lane(board, pr).ok_or_else(|| format!("there's no checkout of {} on this Mac", pr.repo_slug))?;
    let prompt = first_prompt(pr, head, since, holds, login, &goes(&reviews, &board.lanes, pr), &instructions_for(&reviews, pr), files);
    Ok(StartSpec { lane_id: lane.id.clone(), account, place, prompt, model, effort, worktree: false, todos: Vec::new(), from: None, helper: false, report: false })
}

/// Reads the review a turn wrote after `at`: its session's last message once the turn ended.
/// No review there, with nothing running the session, is `NoReview`, which the round judges
/// (`told`).
fn read(engine: &Handle, session: &str, at: i64) -> Went {
    let board = engine.board();
    let cfg = match crate::config::current() {
        Ok(c) => c,
        Err(_) => return Went::NotYet,
    };
    let Some((_, s)) = board.session(session) else { return Went::Failed("Its session is gone.".into()) };
    // A record that doesn't read now is read again in a moment.
    let Ok(page) = crate::transcript::page(&board, &cfg, session, None, 60) else { return Went::NotYet };
    let Some(review) = reply_after(&page.items, at, s.last_prompt_at) else {
        return if s.live { Went::NotYet } else { Went::NoReview };
    };
    if let Some(why) = skipped(&review) {
        return Went::Skipped(why);
    }
    Went::Read { review, agent: agent_of(s) }
}

/// Tells a reviewer whose turn was cut off partway to carry on, as another round's prompt is
/// handed to it (`hand`): where it runs, else where `[reviews] start` lets it go on, on `model`
/// at `effort`.
fn carry_on(engine: &Handle, pr: &PrItem, session: &str, model: Option<&str>, effort: Option<&str>) -> Went {
    let board = engine.board();
    let cfg = match crate::config::current() {
        Ok(c) => c,
        Err(e) => return Went::Failed(format!("Its turn was cut off partway, and it couldn't be told to carry on: {e:#}")),
    };
    let pr = board.prs.iter().find(|p| p.key == pr.key).unwrap_or(pr);
    // Before it goes: the turn it starts may be cut off again before this is told how it went.
    let at = now_ms();
    match hand(engine, &board, &cfg, pr, session, model, effort, crate::start::CARRY_ON.to_string()) {
        Ok((session, pending)) => Went::CarriedOn { session, pending, at },
        Err(e) => Went::Failed(format!("Its turn was cut off partway, and {e}.")),
    }
}

/// Posts a round's review where config.toml says, unless the PR ended or moved on before it was
/// done.
fn post(engine: &Handle, pr: &PrItem, head: &str, review: &str, agent: &str, round: i64) -> Went {
    let board = engine.board();
    let cfg = match crate::config::current() {
        Ok(c) => c,
        Err(e) => return Went::PostFailed(format!("{e:#}")),
    };
    let pr = board.prs.iter().find(|p| p.key == pr.key).unwrap_or(pr);
    let not = if pr.mine { "Not sent" } else { "Not posted" };
    if !pr.state.is_open() {
        return Went::Kept(Some(format!("{not}: the PR was closed before the review was done.")));
    }
    if pr.head_oid.as_deref().is_some_and(|h| h != head) {
        return Went::Failed(format!("{not}: new commits came in before it was done."));
    }
    // A review of a PR of yours stays here, and goes back to its session from there
    // (`Job::Send`), never to GitHub.
    if pr.mine {
        return Went::Kept(clean(review).then(|| "It found nothing to raise, so it isn't sent.".to_string()));
    }
    let (summary, comments) = parse_reply(review);
    match board.repo_settings.reviews(&cfg.reviews, pr).post {
        ReviewPost::None => Went::Kept(None),
        ReviewPost::Draft => draft(pr, head, &summary, comments, agent, round),
        ReviewPost::Comment => match crate::github::comment(&pr.repo_slug, pr.number, &comment_body(&summary, &comments, pr, head, agent)) {
            Ok(link) => {
                engine.pr_changed();
                Went::Posted(link)
            }
            Err(e) => Went::PostFailed(format!("{e:#}")),
        },
    }
}

/// What an automatic review of a PR would start now, as `--snapshot --review-plan` prints it:
/// nothing is started or posted. The PR is read from GitHub, and its reading kept, as a review
/// does.
#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    /// Whether it waits on your review, so a review would start once reviews are on.
    pub wanted: bool,
    /// Reviews are on in config.toml.
    pub on: bool,
    /// Where its reviews stand.
    pub stands: Option<AutoReview>,
    /// What the reviewer would start, as a new session.
    pub start: PlannedStart,
}

/// The reviewer's start in a plan.
#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PlannedStart {
    pub who: String,
    pub spec: Option<StartSpec>,
    /// The start the spec comes to (start.rs `plan`).
    pub start: Option<crate::start::Plan>,
    /// Why it couldn't start.
    pub problem: Option<String>,
}

pub fn plan(board: &Board, cfg: &Config, key: &str) -> anyhow::Result<Plan> {
    let pr = board.prs.iter().find(|p| p.key == key).ok_or_else(|| anyhow::anyhow!("Codealong doesn't list {key}."))?;
    let head = pr.head_oid.clone().ok_or_else(|| anyhow::anyhow!("GitHub didn't say which commit {key} is at."))?;
    let reviews = board.repo_settings.reviews(&cfg.reviews, pr);
    let store: Store = crate::store::read(&path(), WHAT)?.unwrap_or_default();
    let seat = seat_for(&reviews, store.prs.get(&pr_key(key)), &board.lanes, pr);
    if review_blocked(&board.lanes, &cfg.sessions, pr, &seat) {
        let on = if pr.mine { reviews.own.auto } else { reviews.auto };
        return Ok(Plan { wanted: false, on, stands: pr.auto_review.clone(), start: PlannedStart { who: seat.who, spec: None, start: None, problem: Some(FLAGGED_REVIEW.into()) } });
    }
    let (md, diff, _) = crate::github::for_reading(pr)?;
    let stands = pr.auto_review.clone();
    let since = stands.as_ref().filter(|a| a.head != head && a.stage != ReviewStage::Queued).map(|a| a.head.clone());
    let changes = since.as_deref().and_then(|s| crate::github::changes_since(&pr.repo_slug, s, &head).ok().flatten());
    let files = keep_files(pr, &head, &md, &diff, changes.as_deref()).map_err(anyhow::Error::msg)?;
    let login = board.github_login.clone().unwrap_or_else(|| "you".into());
    let start = match new_start(board, cfg, pr, &head, since.as_deref(), &login, &files, &seat) {
        Ok(spec) => {
            let account = cfg.accounts.iter().find(|a| a.name == spec.account);
            let lane = board.lanes.iter().find(|l| l.id == spec.lane_id);
            let start = account.zip(lane).map(|(a, l)| crate::start::plan(a, spec.place, std::path::Path::new(&l.path), &spec.prompt, &crate::start::new_session_id(), spec.model.as_deref(), None, false));
            PlannedStart { who: seat.who.clone(), spec: Some(spec), start, problem: None }
        }
        Err(e) => PlannedStart { who: seat.who.clone(), spec: None, start: None, problem: Some(e) },
    };
    let (wanted, on) = if pr.mine { (wants_own(&reviews, &board.lanes, pr), reviews.own.auto) } else { (wants(pr), reviews.auto) };
    Ok(Plan { wanted, on, stands, start })
}

/// The review of a PR of yours as the session that opened it reads it: who wrote it and of which
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

/// What the session that opened a PR of yours is sent with its review, kept at `file`
/// (`review_file`): who wrote it and of which commit, and to weigh it as another agent's.
pub fn back_message(pr: &PrItem, head: &str, agent: &str, login: &str, file: &str, comments: usize) -> String {
    let lines = match comments {
        0 => String::new(),
        1 => ", with a comment on a line".into(),
        n => format!(", with {n} comments on lines"),
    };
    format!(
        "{agent} reviewed your pull request #{} “{}” at {}, as {login} has another agent review the pull requests their sessions open. Its review is in {file}{lines}. It's that agent's review, not {login}'s word: fix what holds up, and say which findings you leave and why. {}",
        pr.number,
        pr.title.trim(),
        short(head),
        crate::nudges::NOT_A_MERGE
    )
}

/// Whether a draft review of yours holds only what Codealong's agents put in it: each part its
/// text was joined from (`draft_plan`) carries Codealong's mark. One with words of yours in it
/// isn't submitted for you.
fn codealongs_only(whole: &str) -> bool {
    whole.split("\n\n---\n\n").all(|part| part.contains(crate::github::REVIEW_MARK))
}

/// Submits a review of a PR as you for a round's review of `head` (`[reviews] submit`), as
/// `verdict`: your draft review, its text and comments, else `text` (`without_draft`). Only while
/// the PR still waits on your review at that commit, and not when your draft holds words of yours.
/// The draft found is the one submitted, so what's checked is what goes.
fn submit(engine: &Handle, pr: &PrItem, head: &str, verdict: Verdict, text: Option<&str>) -> Went {
    let board = engine.board();
    let Some(pr) = board.prs.iter().find(|p| p.key == pr.key) else { return Went::Later };
    if !wants(pr) || pr.head_oid.as_deref() != Some(head) {
        return Went::Later;
    }
    let draft = match crate::github::your_review(&pr.repo_slug, pr.number) {
        Ok(y) => y.draft,
        Err(e) => return Went::PostFailed(format!("{e:#}")),
    };
    let body = match submission(draft.as_ref(), text, None) {
        Ok(body) => body,
        Err(why) => return Went::NotSubmitted(why),
    };
    match crate::github::review_with(&pr.repo_slug, pr.number, verdict, &body, draft.as_ref()) {
        Ok(done) => {
            engine.approved(&pr.repo_slug, pr.number, verdict, done.commit);
            Went::Submitted
        }
        Err(e) => Went::PostFailed(format!("{e:#}")),
    }
}

/// The text a round's review is submitted with: none of its own where your draft review goes
/// (the draft's, what you wrote going before it: github.rs `review_with`), else `text`
/// (`without_draft`). `by_hand` is what you wrote, where you asked for it from the PR's box
/// (`submit_now`): it goes first, and a draft with words of yours in it goes too, which isn't
/// submitted for you otherwise. Err says why nothing goes.
fn submission(draft: Option<&crate::github::Draft>, text: Option<&str>, by_hand: Option<&str>) -> Result<String, String> {
    let words = by_hand.unwrap_or_default().trim();
    match (draft, text) {
        (Some(d), _) if by_hand.is_none() && !codealongs_only(&d.whole) => Err("your draft review on it has words of yours, so it's yours to submit.".into()),
        (Some(_), _) => Ok(words.to_string()),
        (None, Some(text)) if words.is_empty() => Ok(text.to_string()),
        (None, Some(text)) if text.is_empty() => Ok(words.to_string()),
        (None, Some(text)) => Ok(format!("{words}\n\n{text}")),
        (None, None) => Err("the agent's draft review isn't on GitHub any more: it was submitted or thrown away.".into()),
    }
}

/// Submits a PR's latest agent review as yours now, as `verdict`, with what you wrote before it,
/// as the PR's box does with the verdict its agent suggests (PrBox.tsx): your draft review, the
/// agent's and whatever you added to it, else the review itself or a link to it as posted
/// (`without_draft`). The review must be of the commit the PR's at and not submitted yet. One
/// GitHub refuses is said where it was asked for, and leaves the round as it was.
pub fn submit_now(engine: &Handle, reviewer: &Mutex<Reviewer>, key: &str, verdict: Verdict, words: &str) -> Result<crate::github::Reviewed, String> {
    let board = engine.board();
    let pr = board.prs.iter().find(|p| p.key.eq_ignore_ascii_case(key)).ok_or("Codealong doesn't list that PR any more.")?;
    if !pr.state.is_open() {
        return Err("It's closed.".into());
    }
    let head = pr.head_oid.clone().ok_or("GitHub hasn't said which commit it's at yet.")?;
    let k = pr_key(&pr.key);
    let job = {
        let mut r = reviewer.lock().unwrap_or_else(|e| e.into_inner());
        if r.busy.contains(&k) {
            return Err("Its review is being read or sent: try again in a moment.".into());
        }
        let round = r.store.prs.get(&k).and_then(|t| t.rounds.last()).filter(|r| r.head == head && matches!(r.stage, ReviewStage::Drafted | ReviewStage::Posted | ReviewStage::Kept));
        let round = round.ok_or("The agent hasn't reviewed the commit it's at now.")?;
        if round.submitted.as_ref().is_some_and(|s| s.failed.is_none()) {
            return Err("The agent's review of it was submitted already.".into());
        }
        let text = without_draft(round, pr, &head, verdict);
        r.busy.insert(k.clone());
        Job::Submit { pr: pr.clone(), head, verdict, text }
    };
    let Job::Submit { text, .. } = &job else { unreachable!() };
    let sent = crate::github::your_review(&pr.repo_slug, pr.number).map_err(|e| format!("{e:#}")).and_then(|y| {
        let body = submission(y.draft.as_ref(), text.as_deref(), Some(words))?;
        crate::github::review_with(&pr.repo_slug, pr.number, verdict, &body, y.draft.as_ref()).map_err(|e| format!("{e:#}"))
    });
    let mut r = reviewer.lock().unwrap_or_else(|e| e.into_inner());
    match sent {
        Ok(done) => {
            let said = r.told(&job, Went::Submitted, now_ms());
            drop(r);
            engine.approved(&pr.repo_slug, pr.number, verdict, done.commit.clone());
            engine.reviewed(said);
            Ok(done)
        }
        Err(why) => {
            r.busy.remove(&k);
            Err(why)
        }
    }
}

/// Hands a review of a PR of yours to `session`, as a window's send does, while the session can
/// take it (`can_take`) and, unless asked for by hand (`any_head`), while the PR is open at the
/// commit reviewed. The review goes as a file the message names.
fn send(engine: &Handle, job: &Job, any_head: bool) -> Went {
    let Job::Send { pr, head, session, review, agent } = job else { return Went::Later };
    let board = engine.board();
    let pr = board.prs.iter().find(|p| p.key == pr.key).unwrap_or(pr);
    if !any_head && (!pr.state.is_open() || pr.head_oid.as_deref() != Some(head)) {
        return Went::Later;
    }
    if !board.session(session).is_some_and(|(_, s)| can_take(s)) {
        return Went::Later;
    }
    let login = board.github_login.clone().unwrap_or_else(|| "you".into());
    let file = match crate::attachments::keep(review_file(pr, head, agent, review).as_bytes(), &format!("review-{}-{}.md", pr.number, short(head))) {
        Ok(a) => a.reference(),
        Err(e) => return Went::PostFailed(format!("its review couldn't be kept for it to read: {e:#}")),
    };
    let text = back_message(pr, head, agent, &login, &file, parse_reply(review).1.len());
    let sent = if any_head { crate::send::tell_by_hand(engine, session, &text) } else { crate::send::tell(engine, session, &text) };
    match sent {
        Ok(_) => Went::Sent(session.clone()),
        Err(e) => Went::PostFailed(format!("{e:#}")),
    }
}

/// Starts a review of PR `key` now, as Review with… asks: a round of the same reviews `[reviews]
/// auto` starts (`Job::Review`), the PR written out for it, told what `[reviews] prompt` says, its
/// review posted where `[reviews] post` says and submitted as `[reviews] submit` does, but
/// whatever `[reviews] auto` says, and of a commit reviewed already too. It runs on `account`, on
/// its own model, else on the reviewer config.toml names (`seat_for`: Auto's account for none).
/// That reviewer's session for the PR is asked again when it has one; else a new one starts out
/// of sight. Not while a round of the PR is under way. One that doesn't start is said where it
/// was asked for, and leaves the PR's reviews as they were.
pub fn review_now(engine: &Handle, reviewer: &Mutex<Reviewer>, key: &str, account: Option<&str>) -> Result<(), String> {
    let board = engine.board();
    let cfg = crate::config::current().map_err(|e| format!("{e:#}"))?;
    let pr = board.prs.iter().find(|p| p.key.eq_ignore_ascii_case(key)).ok_or("Codealong reviews only the pull requests it lists, and it doesn't list this one.")?;
    if !pr.state.is_open() {
        return Err("It's closed.".into());
    }
    let head = pr.head_oid.clone().ok_or("GitHub hasn't said which commit it's at yet.")?;
    if board.repo_settings.pending(pr) {
        return Err(format!("Codealong is still reading {}'s team settings ({}): try again in a moment.", pr.repo_slug, crate::teamfile::FILE));
    }
    let k = pr_key(&pr.key);
    let job = {
        let mut r = reviewer.lock().unwrap_or_else(|e| e.into_inner());
        let reviews = board.repo_settings.reviews(&cfg.reviews, pr);
        r.clear_for_hand(&k, on(&reviews), now_ms())?;
        let t = r.store.prs.get(&k);
        let seat = match account.map(str::trim).filter(|a| !a.is_empty()) {
            Some(a) => named_seat(a, "", "", t),
            None => seat_for(&reviews, t, &board.lanes, pr),
        };
        let since = t.and_then(|t| t.done_head()).filter(|h| *h != head).map(String::from);
        let job = Job::Review { pr: pr.clone(), head, since, seat, by_hand: true };
        r.busy.insert(k.clone());
        job
    };
    let Job::Review { head, since, seat, .. } = &job else { unreachable!() };
    let went = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| review(engine, pr, head, since.as_deref(), seat, true))).unwrap_or_else(|_| Went::NotStarted("It stopped partway.".into()));
    let failed = match &went {
        Went::NotStarted(why) | Went::Unreachable(why) => Some(why.clone()),
        Went::Later => Some("It moved on as it was asked: try again.".into()),
        Went::Started(parts) if parts.iter().all(|p| p.state == PartState::Failed) => Some(parts.iter().filter_map(|p| p.note.as_deref()).collect::<Vec<_>>().join(" ")),
        _ => None,
    };
    let mut r = reviewer.lock().unwrap_or_else(|e| e.into_inner());
    match failed {
        Some(why) => {
            r.busy.remove(&k);
            r.save();
            Err(format!("Codealong couldn't start a review of #{}: {why}", pr.number))
        }
        None => {
            let said = r.told(&job, went, now_ms());
            drop(r);
            engine.reviewed(said);
            Ok(())
        }
    }
}

/// Where a PR's latest round stands, as `codealong pr review` says once it has started one: its
/// commit and its reviewers, each with its session to follow (`codealong session wait`).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Begun {
    pub stage: ReviewStage,
    pub head: String,
    pub parts: Vec<ReviewPart>,
}

/// PR `key`'s latest round, as reviews.json has it.
pub fn begun(reviewer: &Mutex<Reviewer>, key: &str) -> Option<Begun> {
    let r = reviewer.lock().unwrap_or_else(|e| e.into_inner());
    let round = r.store.prs.get(&pr_key(key))?.rounds.last()?;
    Some(Begun { stage: round.stage, head: round.head.clone(), parts: parts_of(round) })
}

/// Tries PR `key`'s review again, as Try again on its card and on its notice asks, and says what
/// it did: a review its agent wrote of the commit the PR is at, which wasn't posted, is posted
/// (`Reviewer::repost`) rather than written again, and so is one written in a session the round
/// gave up on, which went on later (`Reviewer::take_up`); else the PR is reviewed again
/// as Review with… does on the reviewer config.toml names (`review_now`), in that reviewer's
/// session for the PR while it can go on.
pub fn again(engine: &Handle, reviewer: &Mutex<Reviewer>, key: &str) -> Result<String, String> {
    let board = engine.board();
    let cfg = crate::config::current().map_err(|e| format!("{e:#}"))?;
    let pr = board.prs.iter().find(|p| p.key.eq_ignore_ascii_case(key)).ok_or("Codealong reviews only the pull requests it lists, and it doesn't list this one.")?;
    let head = pr.head_oid.clone().ok_or("GitHub hasn't said which commit it's at yet.")?;
    let k = pr_key(&pr.key);
    let mut r = reviewer.lock().unwrap_or_else(|e| e.into_inner());
    r.clear_for_hand(&k, on(&board.repo_settings.reviews(&cfg.reviews, pr)), now_ms())?;
    if r.repost(&k, &head) {
        drop(r);
        engine.reviewed(Ok(()));
        return Ok(format!("Posting the agent's review of #{} again: its card says how it goes.", pr.number));
    }
    if r.take_up(&k, &board, pr) {
        drop(r);
        engine.reviewed(Ok(()));
        return Ok(format!("The session that reviewed #{} went on after all: Codealong takes its review from there, and its card says how it goes.", pr.number));
    }
    drop(r);
    review_now(engine, reviewer, key, None)?;
    Ok(format!("Reviewing #{} again: its card says how it goes.", pr.number))
}

/// Hands a PR of yours' latest review to the session that opened it now, as its Review tab's Send
/// asks: whether or not sending is on, and one that found nothing to raise too.
pub fn send_now(engine: &Handle, reviewer: &Mutex<Reviewer>, key: &str) -> Result<(), String> {
    let board = engine.board();
    let pr = board.prs.iter().find(|p| p.key == key).ok_or("Codealong doesn't list that PR any more.")?;
    let (_, s) = origin(&board.lanes, pr).ok_or("The session that opened it isn't here any more.")?;
    if !can_take(s) {
        return Err(if s.live { "The session that opened it waits on you: answer it first." } else { "The session that opened it isn't running here." }.into());
    }
    let job = {
        let mut r = reviewer.lock().unwrap_or_else(|e| e.into_inner());
        let k = pr_key(key);
        if r.busy.contains(&k) {
            return Err("Its review is being read or sent: try again in a moment.".into());
        }
        let t = r.store.prs.get(&k).ok_or("It has no review to send.")?;
        let round = t.rounds.iter().rev().find(|r| r.parts.iter().any(|p| p.state == PartState::Done)).ok_or("It has no review to send.")?;
        let p = round.parts.iter().find(|p| p.state == PartState::Done).ok_or("It has no review to send.")?;
        let job = Job::Send { pr: pr.clone(), head: round.head.clone(), session: s.key.clone(), review: p.review.clone().unwrap_or_default(), agent: p.label() };
        r.busy.insert(k);
        job
    };
    let went = send(engine, &job, true);
    let failed = match &went {
        Went::Sent(_) => None,
        Went::PostFailed(why) => Some(why.clone()),
        _ => Some("The session that opened it can't take it now.".to_string()),
    };
    let mut r = reviewer.lock().unwrap_or_else(|e| e.into_inner());
    match failed {
        None => {
            let said = r.told(&job, went, now_ms());
            drop(r);
            engine.reviewed(said);
            Ok(())
        }
        // A send by hand that didn't go is said where it was asked for, and leaves the round as
        // it was.
        Some(why) => {
            r.busy.remove(&pr_key(key));
            Err(why)
        }
    }
}

/// Does a job and tells `reviewer` how it went, and the engine, which says a failure as a notice
/// until a later job goes through.
pub fn run(engine: &Handle, reviewer: &Mutex<Reviewer>, job: Job) {
    let went = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match &job {
        Job::Review { pr, head, since, seat, by_hand } => review(engine, pr, head, since.as_deref(), seat, *by_hand),
        Job::Read { session, at, .. } => read(engine, session, *at),
        Job::CarryOn { pr, session, model, effort, .. } => carry_on(engine, pr, session, model.as_deref(), effort.as_deref()),
        Job::Post { pr, head, review, agent, round } => post(engine, pr, head, review, agent, *round),
        Job::Send { .. } => send(engine, &job, false),
        Job::Submit { pr, head, verdict, text } => submit(engine, pr, head, *verdict, text.as_deref()),
    }))
    .unwrap_or_else(|_| Went::Failed("It stopped partway.".into()));
    let said = reviewer.lock().unwrap_or_else(|e| e.into_inner()).told(&job, went, now_ms());
    engine.reviewed(said);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;
    use crate::model::{Checks, Merge};

    const NOW: i64 = 1_800_000_000_000;
    const S: &str = "claude:claude:rev";
    const HEAD: &str = "1111111aaaa";
    const NEXT: &str = "2222222bbbb";

    fn pr(n: u64, head: &str) -> PrItem {
        PrItem {
            key: format!("o/r#{n}"),
            repo_slug: "o/r".into(),
            repo: "r".into(),
            project: "P".into(),
            number: n,
            title: "Add teams".into(),
            url: format!("https://github.com/o/r/pull/{n}"),
            state: PrState::Open,
            author: Some("ana".into()),
            mine: false,
            review_requested: true,
            requested_by: None,
            requested_at: Some(n as i64),
            reviewers_asked: Vec::new(),
            snoozed_until: None,
            nudge: false,
            nudge_by_default: false,
            primary: None,
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
            lanes: Vec::new(),
            sessions: Vec::new(),
            head_oid: Some(head.into()),
            authored_by: None,
            labels: Vec::new(),
            auto_review: None,
            last_review: None,
            stale_approval: None,
        }
    }

    fn board(prs: Vec<PrItem>, sessions: Vec<Session>) -> Board {
        Board { prs, lanes: vec![fixtures::lane("/r", sessions)], github_login: Some("gwen".into()), ..Board::default() }
    }

    fn on() -> ReviewsCfg {
        ReviewsCfg { auto: true, ..ReviewsCfg::default() }
    }

    fn at(key: &str, status: Status, last_activity: i64) -> Session {
        Session { status, last_activity, ..fixtures::session(key) }
    }

    fn session(status: Status, last_activity: i64) -> Session {
        at(S, status, last_activity)
    }

    fn started(who: &str, key: &str, when: i64) -> Part {
        Part::asked(who, Some(key.into()), None, when)
    }

    fn kinds(jobs: &[Job]) -> Vec<String> {
        jobs.iter()
            .map(|j| match j {
                Job::Review { pr, head, since, seat, by_hand } => {
                    let session = seat.session.as_deref().map(|k| format!(" in {k}")).unwrap_or_default();
                    let hand = if *by_hand { " by hand" } else { "" };
                    format!("review {} {} since {:?} by {}{session}{hand}", pr.number, short(head), since.as_deref().map(short), seat.who)
                }
                Job::Read { pr, part, session, .. } => format!("read {} part {part} in {session}", pr.number),
                Job::CarryOn { pr, part, session, model, effort, .. } => format!("carry on {} part {part} in {session} on {model:?} at {effort:?}", pr.number),
                Job::Post { pr, review, agent, .. } => format!("post {} {review:?} by {agent}", pr.number),
                Job::Send { pr, head, session, agent, .. } => format!("send {} {} by {agent} to {session}", pr.number, short(head)),
                Job::Submit { pr, head, verdict, text } => format!("{} {} {}{}", verdict.verb(), pr.number, short(head), text.as_deref().map(|t| format!(" saying {t:?}")).unwrap_or_default()),
            })
            .collect()
    }

    #[test]
    fn provider_flags_pause_reviews_for_the_opener_and_kept_reviewer() {
        let mut b = board(vec![mine(7, HEAD)], vec![Session { flagged: true, ..fixtures::session(MAKER) }]);
        let mut r = Reviewer::default();
        assert!(r.due(&b, &own(), NOW).is_empty());
        b.sessions.block_flagged_sessions = false;
        let jobs = r.due(&b, &own(), NOW + 1);
        assert_eq!(jobs.len(), 1);
        r.told(&jobs[0], Went::Started(vec![started(AUTO, S, NOW + 1)]), NOW + 2).unwrap();
        b.sessions.block_flagged_sessions = true;
        b.lanes[0].sessions.push(Session { flagged: true, ..session(Status::Unread, NOW + 60_000) });
        // Reading and keeping a completed reply doesn't send anything to either flagged session.
        read(&mut r, &b, &own(), NOW + 61_000, "A completed review.", "Claude");
        let post = r.due(&b, &own(), NOW + 62_000).pop().unwrap();
        assert!(matches!(post, Job::Post { .. }));
        r.told(&post, Went::Kept(None), NOW + 63_000).unwrap();
        assert!(r.due(&b, &own(), NOW + 64_000).is_empty(), "no handback to the flagged opener");
        assert!(r.clear_for_hand("o/r#7", true, NOW + 65_000).is_ok(), "the completed round no longer blocks Review with");
        let seat = seat_for(&own(), r.store.prs.get("o/r#7"), &b.lanes, &b.prs[0]);
        assert_eq!(seat.session.as_deref(), Some(S));
        assert!(review_seat(&b, &b.prs[0], seat.clone(), false).is_none());
        assert!(review_seat(&b, &b.prs[0], seat, true).unwrap().session.is_none(), "a manual review starts afresh");
        b.lanes[0].sessions[0].flagged = false;
        b.prs[0].head_oid = Some(NEXT.into());
        assert!(r.due(&b, &own(), NOW + 66_000).is_empty());
        assert!(r.due(&b, &own(), NOW + 66_000 + QUIET_MS).is_empty(), "no new automatic round in the flagged kept reviewer");
    }

    #[test]
    fn blocked_reviews_are_explained_without_queueing_or_holding_the_pr() {
        let mut b = board(vec![mine(7, HEAD)], vec![Session { flagged: true, ..fixtures::session(MAKER) }]);
        let mut prs = b.prs.clone();
        apply(&mut prs, &Store::default(), &own(), &b.sessions, &Default::default(), &b.lanes, NOW);
        let a = prs[0].auto_review.as_ref().unwrap();
        assert_eq!(a.stage, ReviewStage::Skipped);
        assert!(!a.holds && !a.behind);
        assert_eq!(a.note.as_deref(), Some(FLAGGED_REVIEW));
        b.sessions.block_flagged_sessions = false;
        apply(&mut prs, &Store::default(), &own(), &b.sessions, &Default::default(), &b.lanes, NOW);
        assert_eq!(prs[0].auto_review.as_ref().unwrap().stage, ReviewStage::Queued);

        let mut r = Reviewer::default();
        let mut b = board(vec![pr(7, HEAD)], vec![]);
        let job = r.due(&b, &on(), NOW).pop().unwrap();
        r.told(&job, Went::Started(vec![started(AUTO, S, NOW)]), NOW).unwrap();
        b.lanes[0].sessions.push(Session { flagged: true, ..session(Status::Unread, NOW + 60_000) });
        read(&mut r, &b, &on(), NOW + 61_000, "Done.", "Claude");
        let post = r.due(&b, &on(), NOW + 62_000).pop().unwrap();
        r.told(&post, Went::Kept(None), NOW + 63_000).unwrap();
        b.prs[0].head_oid = Some(NEXT.into());
        let mut prs = b.prs.clone();
        apply(&mut prs, &r.store, &on(), &b.sessions, &Default::default(), &b.lanes, NOW + 64_000);
        let a = prs[0].auto_review.as_ref().unwrap();
        assert!(!a.holds && !a.behind, "a paused next round must not hide the PR from its user");
        assert_eq!(a.note.as_deref(), Some(FLAGGED_REVIEW));
        r.store.prs.values_mut().next().unwrap().rounds.last_mut().unwrap().note = Some("It found nothing to raise.".into());
        apply(&mut prs, &r.store, &on(), &b.sessions, &Default::default(), &b.lanes, NOW + 64_000);
        assert_eq!(prs[0].auto_review.as_ref().unwrap().note.as_deref(), Some(format!("It found nothing to raise. {FLAGGED_REVIEW}").as_str()));
        let mut off = on();
        off.auto = false;
        apply(&mut prs, &r.store, &off, &b.sessions, &Default::default(), &b.lanes, NOW + 64_000);
        assert_eq!(prs[0].auto_review.as_ref().unwrap().note.as_deref(), Some("It found nothing to raise."));
    }

    #[test]
    fn a_flagged_cut_off_reviewer_settles_without_a_carry_on() {
        let mut r = Reviewer::default();
        let mut b = board(vec![pr(7, HEAD)], vec![]);
        let job = r.due(&b, &on(), NOW).pop().unwrap();
        r.told(&job, Went::Started(vec![started(AUTO, S, NOW)]), NOW).unwrap();
        b.lanes[0].sessions.push(Session { flagged: true, live: false, cut_off: Some(NOW + 1), ..session(Status::Error, NOW + 1) });
        assert!(r.due(&b, &on(), NOW + 60_000).is_empty());
        assert_eq!(r.store.prs["o/r#7"].rounds[0].stage, ReviewStage::Failed);
        assert!(r.clear_for_hand("o/r#7", true, NOW + 61_000).is_ok());
    }

    /// Each PR goes as its repo's settings say: your table for the repo turns reviews on there
    /// with them off everywhere else, on the account it names; its team's file adds words after
    /// the instructions and keeps a verdict from being submitted for anyone.
    #[test]
    fn a_pr_goes_as_its_repos_settings_say() {
        use crate::config::{repo_key, RepoCfg, ReviewsPatch};
        use crate::teamfile::TeamCfg;
        let off = ReviewsCfg::default();
        let elsewhere = PrItem { key: "o/s#8".into(), repo_slug: "o/s".into(), url: "https://github.com/o/s/pull/8".into(), ..pr(8, HEAD) };
        let mut b = board(vec![pr(7, HEAD), elsewhere], vec![]);
        b.repo_settings.mine.insert(repo_key("o/r"), RepoCfg { reviews: ReviewsPatch { auto: Some(true), account: Some("codex".into()), ..ReviewsPatch::default() }, ..RepoCfg::default() });
        assert_eq!(kinds(&Reviewer::default().due(&b, &off, NOW)), ["review 7 1111111 since None by codex"]);

        b.repo_settings.team.insert(("o/r".into(), "main".into()), TeamCfg { review_instructions: "Check migrations.".into(), ..TeamCfg::default() });
        let words = instructions_for(&b.repo_settings.reviews(&off, &b.prs[0]), &b.prs[0]);
        assert_eq!(words, format!("{INSTRUCTIONS}\n\no/r's team asks this of every review there (.codealong/config.toml):\n\nCheck migrations."));
        assert_eq!(instructions_for(&b.repo_settings.reviews(&off, &b.prs[1]), &b.prs[1]), INSTRUCTIONS, "another repo's team says nothing here");
        // A PR into another branch goes as that branch's file says: here, none.
        let release = PrItem { base: Some("release".into()), ..pr(7, HEAD) };
        assert_eq!(instructions_for(&b.repo_settings.reviews(&off, &release), &release), INSTRUCTIONS);

        // The team never has an approval submitted, whatever yours says; a request for changes
        // still goes as yours says.
        let approving = ReviewsCfg { auto: true, submit: ReviewSubmit::ApproveOrComment, ..ReviewsCfg::default() };
        let mut done = board(vec![pr(7, HEAD)], vec![session(Status::Unread, NOW + 60_000)]);
        done.repo_settings.team.insert(("o/r".into(), "main".into()), TeamCfg { never_submit: vec![Verdict::Approve], ..TeamCfg::default() });
        let reviewed = |review: &str| {
            let mut r = Reviewer::default();
            let job = r.due(&done, &approving, NOW).pop().unwrap();
            r.told(&job, Went::Started(vec![started(AUTO, S, NOW)]), NOW).unwrap();
            read(&mut r, &done, &approving, NOW + 61_000, review, "Codex");
            let post = r.due(&done, &approving, NOW + 62_000).pop().unwrap();
            r.told(&post, Went::Drafted("https://github.com/o/r/pull/7/files".into()), NOW + 63_000).unwrap();
            r
        };
        assert!(reviewed("Fine.\n\nVerdict: approve").due(&done, &approving, NOW + 64_000).is_empty());
        assert_eq!(kinds(&reviewed("Two problems.\n\nVerdict: request changes").due(&done, &approving, NOW + 64_000)), ["comment on 7 1111111"]);

        // While its base branch's team file isn't read, nothing starts or is submitted: what the
        // team sets would be missed.
        let mut unread = done.clone();
        unread.repo_settings.here.insert("o/r".into());
        let mut r = reviewed("Two problems.\n\nVerdict: request changes");
        assert!(r.due(&unread, &approving, NOW + 64_000).is_empty(), "no submit");
        assert!(Reviewer::default().due(&unread, &approving, NOW).is_empty(), "no review");
        unread.repo_settings.read.insert(("o/r".into(), "main".into()));
        assert_eq!(kinds(&Reviewer::default().due(&unread, &approving, NOW)).len(), 1);
    }

    fn read(r: &mut Reviewer, b: &Board, cfg: &ReviewsCfg, when: i64, review: &str, agent: &str) {
        let job = r.due(b, cfg, when).pop().expect("a read");
        assert!(matches!(job, Job::Read { .. }), "{job:?}");
        r.told(&job, Went::Read { review: review.into(), agent: agent.into() }, when).unwrap();
    }

    #[test]
    fn a_pr_that_waits_on_you_is_reviewed_once_per_commit_and_posted_once_its_turn_ends() {
        let mut r = Reviewer::default();
        let b = board(vec![pr(7, HEAD)], vec![]);
        let jobs = r.due(&b, &on(), NOW);
        assert_eq!(kinds(&jobs), ["review 7 1111111 since None by auto"]);
        assert!(r.due(&b, &on(), NOW + 1).is_empty(), "under way");
        assert_eq!(r.told(&jobs[0], Went::Started(vec![started(AUTO, S, NOW)]), NOW + 2), Ok(()));
        // Working: nothing yet.
        assert!(r.due(&board(vec![pr(7, HEAD)], vec![session(Status::Working, NOW + 5)]), &on(), NOW + 10).is_empty());
        // Its turn ended: read.
        let done = board(vec![pr(7, HEAD)], vec![session(Status::Unread, NOW + 60_000)]);
        let jobs = r.due(&done, &on(), NOW + 61_000);
        assert_eq!(kinds(&jobs), [format!("read 7 part 0 in {S}")]);
        // Not written yet: looked at again later.
        r.told(&jobs[0], Went::NotYet, NOW + 61_000).unwrap();
        assert!(r.due(&done, &on(), NOW + 62_000).is_empty());
        read(&mut r, &done, &on(), NOW + 61_000 + RETRY_MS, "Two problems.", "Claude Code, claude-opus-5-5");
        // One review: posted as it is.
        let jobs = r.due(&done, &on(), NOW + 92_000);
        assert_eq!(kinds(&jobs), ["post 7 \"Two problems.\" by Claude Code, claude-opus-5-5"]);
        r.told(&jobs[0], Went::Drafted("https://github.com/o/r/pull/7/files".into()), NOW + 93_000).unwrap();
        // That commit is done.
        assert!(r.due(&done, &on(), NOW + 10 * QUIET_MS).is_empty());
        let mut prs = vec![pr(7, HEAD)];
        apply(&mut prs, &r.store, &on(), &Default::default(), &Default::default(), &[], NOW);
        let a = prs[0].auto_review.clone().unwrap();
        assert_eq!((a.stage, a.head.as_str(), a.comment.as_deref(), a.rounds, a.behind), (ReviewStage::Drafted, HEAD, Some("https://github.com/o/r/pull/7/files"), 1, false));
        assert_eq!(a.parts, [ReviewPart { who: AUTO.into(), session: Some(S.into()), state: PartState::Done, note: None }]);
    }

    /// The session that opened PR 7 of yours.
    const MAKER: &str = "claude:claude:maker";

    /// PR `n` of yours at `head`, opened by `MAKER`.
    fn mine(n: u64, head: &str) -> PrItem {
        PrItem { mine: true, author: Some("gwen".into()), review_requested: false, primary: Some(MAKER.into()), ..pr(n, head) }
    }

    /// Reviews of your own PRs on, and nothing else.
    fn own() -> ReviewsCfg {
        ReviewsCfg { own: crate::config::OwnReviewsCfg { auto: true, ..Default::default() }, ..ReviewsCfg::default() }
    }

    #[test]
    fn a_pr_of_yours_is_reviewed_on_another_harness_and_its_review_handed_back_to_its_session() {
        let mut r = Reviewer::default();
        let maker = |status: Status| Session { status, last_activity: NOW, ..fixtures::session(MAKER) };
        let b = board(vec![mine(7, HEAD)], vec![maker(Status::Working)]);
        let jobs = r.due(&b, &own(), NOW);
        assert_eq!(kinds(&jobs), ["review 7 1111111 since None by auto"]);
        // Auto keeps off the harness of the session that opened it.
        let Job::Review { seat, .. } = &jobs[0] else { unreachable!() };
        assert_eq!(seat.avoid, Some(Harness::Claude));
        r.told(&jobs[0], Went::Started(vec![started(AUTO, S, NOW)]), NOW + 1).unwrap();
        let done = |status: Status| board(vec![mine(7, HEAD)], vec![maker(status), session(Status::Unread, NOW + 60_000)]);
        read(&mut r, &done(Status::Working), &own(), NOW + 61_000, "Two problems.\n\n```review-comments\n[{\"path\":\"src/a.rs\",\"line\":2,\"body\":\"This panics.\"}]\n```", "Codex, gpt-5.5");
        let post = r.due(&done(Status::Working), &own(), NOW + 62_000).pop().unwrap();
        assert!(matches!(post, Job::Post { .. }), "{post:?}");
        // Kept here: nothing goes on GitHub.
        r.told(&post, Went::Kept(None), NOW + 63_000).unwrap();
        assert_eq!(r.store.prs["o/r#7"].rounds[0].stage, ReviewStage::Kept);
        // Handed back to the session that opened it, while it doesn't wait on you.
        let jobs = r.due(&done(Status::Working), &own(), NOW + 64_000);
        assert_eq!(kinds(&jobs), [format!("send 7 1111111 by Codex, gpt-5.5 to {MAKER}")]);
        r.told(&jobs[0], Went::Later, NOW + 64_500).unwrap();
        let waiting = Session { asks: false, ..maker(Status::Needs) };
        assert!(r.due(&board(vec![mine(7, HEAD)], vec![waiting, session(Status::Unread, NOW + 60_000)]), &own(), NOW + 65_000).is_empty(), "waits on you");
        let stopped = Session { live: false, ..maker(Status::Idle) };
        assert!(r.due(&board(vec![mine(7, HEAD)], vec![stopped, session(Status::Unread, NOW + 60_000)]), &own(), NOW + 65_000).is_empty(), "not running");
        let jobs = r.due(&done(Status::Idle), &own(), NOW + 66_000);
        r.told(&jobs[0], Went::Sent(MAKER.into()), NOW + 67_000).unwrap();
        assert!(r.due(&done(Status::Idle), &own(), NOW + 68_000).is_empty(), "once");
        let mut prs = vec![mine(7, HEAD)];
        apply(&mut prs, &r.store, &own(), &Default::default(), &Default::default(), &done(Status::Idle).lanes, NOW + 68_000);
        let a = prs[0].auto_review.clone().unwrap();
        assert_eq!((a.stage, a.own, a.holds, a.sent.as_deref(), a.sent_at, a.note.as_deref(), a.sendable.as_deref()), (ReviewStage::Kept, true, false, Some(MAKER), Some(NOW + 67_000), None, Some(MAKER)));
    }

    /// The review of a PR of yours writes to the session that opened it as that session's helper
    /// does (send.rs `from_its_helper`), closed PR or open, while handing reviews back is on,
    /// whichever round asked it; a session taken for a review by its first prompt alone doesn't,
    /// nor one found for a round that only links to the PR.
    #[test]
    fn a_review_of_your_pr_writes_to_its_session_as_a_helper() {
        let opens = format!("{} “Teams” (https://github.com/o/r/pull/7), by gwen: it merges teams into main.", opening(&mine(7, HEAD)));
        let review = |key: &str, first: &str| Session { helper: Some(crate::model::Helper { pr: Some("o/r#7".into()), ..Default::default() }), first_prompt: Some(first.into()), ..fixtures::session(key) };
        // Its round done, and the next one's start failed: it's still the PR's review. A session of
        // yours about the PR, started just before a review was, was found for it too (`names`).
        let mut round = Round::new(HEAD, ReviewStage::Kept, NOW);
        round.parts = vec![started(AUTO, S, NOW), started(AUTO, "codex:codex:linked", NOW)];
        let mut next = Round::new(NEXT, ReviewStage::Failed, NOW + 1);
        next.parts = vec![Part::failed(AUTO, "no account".into(), NOW + 1)];
        let tracked = |sessions: BTreeMap<String, String>| Tracked { sessions, rounds: vec![round.clone(), next.clone()] };
        let store = Store { prs: [("o/r#7".to_string(), tracked(BTreeMap::new()))].into() };
        let board_with = |pr: PrItem| {
            let reviews = [review(S, &opens), review("codex:codex:kept", &opens), review("codex:codex:guessed", &opens), review("codex:codex:linked", "Investigate (https://github.com/o/r/pull/7)")];
            let mut b = board(vec![pr], [vec![fixtures::session(MAKER), fixtures::session("claude:claude:other")], reviews.to_vec()].concat());
            nest_own(&mut b.lanes, &b.prs);
            b
        };
        let asked = |b: &Board, store: &Store, cfg: &ReviewsCfg, from: &str, to: &str| b.session(from).is_some_and(|(_, s)| asked_to_review(b, store, cfg, s, to));
        let b = board_with(mine(7, HEAD));
        assert!(asked(&b, &store, &own(), S, MAKER));
        assert!(!asked(&b, &store, &own(), S, "claude:claude:other"));
        assert!(!asked(&b, &store, &own(), "codex:codex:guessed", MAKER));
        assert!(!asked(&b, &store, &own(), "codex:codex:linked", MAKER));
        assert!(!asked(&b, &store, &own(), MAKER, MAKER));
        // The session kept for its next round.
        let kept = Store { prs: [("o/r#7".to_string(), tracked([(AUTO.to_string(), "codex:codex:kept".to_string())].into()))].into() };
        assert!(asked(&b, &kept, &own(), "codex:codex:kept", MAKER));
        assert!(asked(&board_with(PrItem { state: PrState::Closed, ..mine(7, HEAD) }), &store, &own(), S, MAKER));
        // Handing reviews back off: held for you, as any agent's message.
        let quiet = ReviewsCfg { own: crate::config::OwnReviewsCfg { send: false, ..own().own }, ..own() };
        assert!(!asked(&b, &store, &quiet, S, MAKER));
        // A PR that waits on your review has no session of yours to write to.
        assert!(!asked(&board_with(pr(7, HEAD)), &store, &own(), S, MAKER));
        // Made your own, it's the PR's review no more, and reviews.json isn't read for it.
        let mut owned = board_with(mine(7, HEAD));
        owned.lanes[0].sessions.iter_mut().filter(|s| s.key == S).for_each(|s| s.helper = None);
        assert!(!reviews_for(&owned, &own(), S, MAKER));
    }

    #[test]
    fn a_review_of_yours_that_found_nothing_or_with_sending_off_stays_here() {
        let maker = Session { status: Status::Idle, ..fixtures::session(MAKER) };
        let b = board(vec![mine(7, HEAD)], vec![maker.clone(), session(Status::Unread, NOW + 60_000)]);
        let reviewed = |cfg: &ReviewsCfg, review: &str| {
            let mut r = Reviewer::default();
            let job = r.due(&b, cfg, NOW).pop().unwrap();
            r.told(&job, Went::Started(vec![started(AUTO, S, NOW)]), NOW).unwrap();
            read(&mut r, &b, cfg, NOW + 61_000, review, "Codex");
            let post = r.due(&b, cfg, NOW + 62_000).pop().unwrap();
            r.told(&post, Went::Kept(clean(review).then(|| "It found nothing to raise, so it isn't sent.".into())), NOW + 63_000).unwrap();
            r
        };
        let mut r = reviewed(&own(), "LGTM: the tests cover it.");
        assert!(r.due(&b, &own(), NOW + 64_000).is_empty());
        let mut prs = vec![mine(7, HEAD)];
        apply(&mut prs, &r.store, &own(), &Default::default(), &Default::default(), &b.lanes, NOW);
        assert_eq!(prs[0].auto_review.as_ref().unwrap().note.as_deref(), Some("It found nothing to raise, so it isn't sent."));
        let mut quiet = own();
        quiet.own.send = false;
        let mut r = reviewed(&quiet, "Two problems.");
        assert!(r.due(&b, &quiet, NOW + 64_000).is_empty());
        let mut prs = vec![mine(7, HEAD)];
        apply(&mut prs, &r.store, &quiet, &Default::default(), &Default::default(), &b.lanes, NOW);
        let a = prs[0].auto_review.clone().unwrap();
        // Sent from the Review tab instead, while its session can take it.
        assert_eq!((a.note.as_deref(), a.sendable.as_deref()), (Some("Handing reviews back is off: send it from here."), Some(MAKER)));
        // A send that doesn't go is tried a few times, then given up on, saying why.
        let mut r = reviewed(&own(), "Two problems.");
        for i in 0..POST_TRIES {
            let at = NOW + 64_000 + i64::from(i) * 10 * RETRY_MS;
            let job = r.due(&b, &own(), at).pop().expect("a send");
            let said = r.told(&job, Went::PostFailed("the terminal is gone".into()), at);
            assert!(said.unwrap_err().text.contains("couldn't hand its review of o/r#7"));
        }
        assert!(r.due(&b, &own(), NOW + 100 * RETRY_MS).is_empty());
        let mut prs = vec![mine(7, HEAD)];
        apply(&mut prs, &r.store, &own(), &Default::default(), &Default::default(), &b.lanes, NOW);
        assert_eq!(prs[0].auto_review.as_ref().unwrap().note.as_deref(), Some("It couldn't be handed to the session that opened the PR: the terminal is gone"));
    }

    #[test]
    fn a_draft_of_yours_is_reviewed_only_with_drafts_on_and_another_s_draft_never() {
        let lanes = vec![fixtures::lane("/r", vec![fixtures::session(MAKER)])];
        let drafts = ReviewsCfg { auto: true, own: crate::config::OwnReviewsCfg { auto: true, drafts: true, ..Default::default() }, ..ReviewsCfg::default() };
        let yours = PrItem { state: PrState::Draft, ..mine(7, HEAD) };
        let theirs = PrItem { state: PrState::Draft, ..pr(8, HEAD) };
        assert!(!wanted(&own(), &lanes, &yours), "drafts off");
        assert!(wanted(&drafts, &lanes, &yours));
        assert!(!wanted(&drafts, &lanes, &theirs), "asks for your review, but it's someone else's draft");
        // It's started as a PR of yours ready for review is.
        let b = board(vec![yours, theirs], vec![fixtures::session(MAKER)]);
        assert_eq!(kinds(&Reviewer::default().due(&b, &drafts, NOW)), ["review 7 1111111 since None by auto"]);
    }

    #[test]
    fn only_prs_of_yours_a_session_here_opened_and_each_kind_only_while_its_switch_is_on() {
        let maker = fixtures::session(MAKER);
        let lanes = vec![fixtures::lane("/r", vec![maker.clone()])];
        assert!(wants_own(&own(), &lanes, &mine(7, HEAD)));
        assert!(!wants_own(&own(), &lanes, &PrItem { primary: None, ..mine(7, HEAD) }), "no session opened it");
        assert!(!wants_own(&own(), &[fixtures::lane("/r", vec![Session { archived: true, ..maker.clone() }])], &mine(7, HEAD)), "archived");
        assert!(!wants_own(&own(), &lanes, &PrItem { state: PrState::Draft, ..mine(7, HEAD) }));
        assert!(!wants_own(&own(), &lanes, &pr(8, HEAD)), "not yours");
        let both = board(vec![mine(7, HEAD), pr(8, HEAD)], vec![maker]);
        let mut r = Reviewer::default();
        assert_eq!(kinds(&r.due(&both, &own(), NOW)), ["review 7 1111111 since None by auto"]);
        let mut r2 = Reviewer::default();
        assert_eq!(kinds(&r2.due(&both, &on(), NOW)), ["review 8 1111111 since None by auto"]);
        // [reviews.own]'s reviewer, named: no harness kept off.
        let named = ReviewsCfg { own: crate::config::OwnReviewsCfg { auto: true, account: "codex".into(), model: "gpt-5.5".into(), ..Default::default() }, ..ReviewsCfg::default() };
        let s = seat_for(&named, None, &lanes, &mine(7, HEAD));
        assert_eq!((s.who.as_str(), s.avoid), ("codex gpt-5.5", None));
        assert_eq!(instructions_for(&ReviewsCfg { own: crate::config::OwnReviewsCfg { prompt: " Mind the tests. ".into(), ..Default::default() }, ..ReviewsCfg::default() }, &mine(7, HEAD)), "Mind the tests.");
        assert_eq!(instructions_for(&own(), &mine(7, HEAD)), INSTRUCTIONS);
    }

    #[test]
    fn what_the_session_that_opened_it_is_told_and_given_to_read() {
        assert!(clean("LGTM: the tests cover it.") && clean("  LGTM") && !clean("LGTMs aside, two problems.") && !clean("Two problems. LGTM otherwise."));
        let review = "Two problems.\n\n```review-comments\n[{\"path\":\"src/a.rs\",\"line\":2,\"body\":\"This panics\\non empty input.\"}]\n```";
        let file = review_file(&mine(7, HEAD), HEAD, "Codex, gpt-5.5", review);
        assert_eq!(file, "# Review of #7 “Add teams” at 1111111, by Codex, gpt-5.5\n\nTwo problems.\n\n## On particular lines\n\n- `src/a.rs` line 2: This panics\n  on empty input.\n");
        let said = back_message(&mine(7, HEAD), HEAD, "Codex, gpt-5.5", "gwen", "[review-7.md](files/ab/review-7.md)", 1);
        assert_eq!(said, format!("Codex, gpt-5.5 reviewed your pull request #7 “Add teams” at 1111111, as gwen has another agent review the pull requests their sessions open. Its review is in [review-7.md](files/ab/review-7.md), with a comment on a line. It's that agent's review, not gwen's word: fix what holds up, and say which findings you leave and why. {}", crate::nudges::NOT_A_MERGE));
        let files = Reading { md: "[pr-7.md](files/a/pr-7.md)".into(), diff: "[pr-7.diff](files/b/pr-7.diff)".into(), changes: None };
        let first = first_prompt(&mine(7, HEAD), HEAD, None, true, "gwen", &Goes::Back("Claude Code, opus".into()), INSTRUCTIONS, &files);
        assert!(first.contains("for gwen, who opened it with another agent (Claude Code, opus): your review goes back to that agent, to act on."), "{first}");
        assert!(first.contains("hands your last message to the agent that wrote the PR (Claude Code, opus) to act on, unless you'd approve it, and shows it to gwen; nothing goes on GitHub.") && first.contains("written to that agent") && first.contains("`Verdict: approve`"), "{first}");
        assert!(!first.contains("whose review it waits on") && !first.contains("draft"), "{first}");
    }

    #[test]
    fn a_review_ends_on_what_its_agent_would_do_which_isnt_posted() {
        assert_eq!(verdict_of("Two problems.\n\nVerdict: request changes"), Some(Verdict::RequestChanges));
        assert_eq!(verdict_of("Fine.\n**Verdict:** approve."), Some(Verdict::Approve));
        assert_eq!(verdict_of("Fine.\n`Verdict: Request-Changes`"), Some(Verdict::RequestChanges));
        assert_eq!(verdict_of("Hm.\nverdict: comment"), Some(Verdict::Comment));
        assert_eq!(verdict_of("No verdict here.\nVerdict: maybe"), None);
        // The last one says.
        assert_eq!(verdict_of("Verdict: approve\nOn second thought:\nVerdict: request changes"), Some(Verdict::RequestChanges));
        let reply = "Two problems.\n\n```review-comments\n[{\"path\":\"a.rs\",\"line\":2,\"body\":\"x\"}]\n```\n\nVerdict: request changes";
        assert_eq!(parse_reply(reply).0, "Two problems.");
        assert_eq!(parse_reply("Nothing to raise.\n\nVerdict: approve").0, "Nothing to raise.");
        assert!(clean("Nothing to raise.\n\nVerdict: approve") && clean("LGTM: fine") && !clean("Two problems.\nVerdict: comment"));
        let files = Reading { md: "m".into(), diff: "d".into(), changes: None };
        for goes in [Goes::Post(ReviewPost::Draft), Goes::Post(ReviewPost::None), Goes::Back("Codex".into())] {
            assert!(first_prompt(&pr(7, HEAD), HEAD, None, true, "gwen", &goes, INSTRUCTIONS, &files).contains("`Verdict: approve` when nothing you found needs fixing"), "{goes:?}");
        }
    }

    #[test]
    fn the_agents_verdict_is_submitted_as_yours_as_config_toml_says() {
        let cfg = |submit: ReviewSubmit| ReviewsCfg { submit, ..on() };
        let approving = cfg(ReviewSubmit::Approve);
        let done = board(vec![pr(7, HEAD)], vec![session(Status::Unread, NOW + 60_000)]);
        let drafted = || Went::Drafted("https://github.com/o/r/pull/7/files".into());
        let reviewed = |cfg: &ReviewsCfg, review: &str, posted: Went| {
            let mut r = Reviewer::default();
            let job = r.due(&done, cfg, NOW).pop().unwrap();
            r.told(&job, Went::Started(vec![started(AUTO, S, NOW)]), NOW).unwrap();
            read(&mut r, &done, cfg, NOW + 61_000, review, "Codex");
            let post = r.due(&done, cfg, NOW + 62_000).pop().unwrap();
            r.told(&post, posted, NOW + 63_000).unwrap();
            r
        };
        let mut r = reviewed(&approving, "Fine.\n\nVerdict: approve", drafted());
        // Held out of what waits on you until it's approved.
        let mut prs = vec![PrItem { requested_at: Some(NOW), ..pr(7, HEAD) }];
        apply(&mut prs, &r.store, &approving, &Default::default(), &Default::default(), &[], NOW + 63_500);
        let a = prs[0].auto_review.clone().unwrap();
        assert_eq!((a.suggests, a.holds, a.submitted), (Some(Verdict::Approve), true, None));
        let jobs = r.due(&done, &approving, NOW + 64_000);
        assert_eq!(kinds(&jobs), ["approve 7 1111111 saying \"\""]);
        r.told(&jobs[0], Went::Submitted, NOW + 65_000).unwrap();
        assert!(r.due(&done, &approving, NOW + 66_000).is_empty(), "once");
        let mut prs = vec![PrItem { requested_at: Some(NOW), ..pr(7, HEAD) }];
        apply(&mut prs, &r.store, &approving, &Default::default(), &Default::default(), &[], NOW + 66_000);
        let a = prs[0].auto_review.clone().unwrap();
        assert_eq!((a.submitted, a.submitted_at, a.holds), (Some(Verdict::Approve), Some(NOW + 65_000), false));
        // Never, or an agent that wouldn't approve it while only approvals are: suggested, not done.
        let mut r = reviewed(&on(), "Fine.\n\nVerdict: approve", drafted());
        assert!(r.due(&done, &on(), NOW + 64_000).is_empty());
        let mut r = reviewed(&approving, "Two problems.\n\nVerdict: request changes", drafted());
        assert!(r.due(&done, &approving, NOW + 64_000).is_empty());
        let mut prs = vec![pr(7, HEAD)];
        apply(&mut prs, &r.store, &approving, &Default::default(), &Default::default(), &[], NOW + 64_000);
        assert_eq!(prs[0].auto_review.as_ref().unwrap().suggests, Some(Verdict::RequestChanges));
        // A request for changes goes as one, or as a comment; the draft holds what it says.
        for (submit, verb, approves) in [
            (ReviewSubmit::ApproveOrRequestChanges, "request changes on", true),
            (ReviewSubmit::ApproveOrComment, "comment on", true),
            (ReviewSubmit::RequestChanges, "request changes on", false),
            (ReviewSubmit::Comment, "comment on", false),
        ] {
            let mut r = reviewed(&cfg(submit), "Two problems.\n\nVerdict: request changes", drafted());
            assert_eq!(kinds(&r.due(&done, &cfg(submit), NOW + 64_000)), [format!("{verb} 7 1111111")], "{submit:?}");
            // An approval goes too, unless approvals are left to you: then the PR waits on you
            // once its review is done, the approval suggested.
            let mut r = reviewed(&cfg(submit), "Fine.\n\nVerdict: approve", drafted());
            let jobs = r.due(&done, &cfg(submit), NOW + 64_000);
            if approves {
                assert_eq!(kinds(&jobs), ["approve 7 1111111 saying \"\""], "{submit:?}");
            } else {
                assert!(jobs.is_empty(), "{submit:?}");
                let mut prs = vec![PrItem { requested_at: Some(NOW), ..pr(7, HEAD) }];
                apply(&mut prs, &r.store, &cfg(submit), &Default::default(), &Default::default(), &[], NOW + 64_000);
                let a = prs[0].auto_review.clone().unwrap();
                assert_eq!((a.suggests, a.holds, a.submitted), (Some(Verdict::Approve), false, None), "{submit:?}");
            }
            // An agent that can't tell leaves it to you.
            let mut r = reviewed(&cfg(submit), "Hm.\n\nVerdict: comment", drafted());
            assert!(r.due(&done, &cfg(submit), NOW + 64_000).is_empty(), "{submit:?}");
        }
        // Without a draft, it points to the review posted as a comment, or with posting off
        // carries it.
        let changes = cfg(ReviewSubmit::ApproveOrRequestChanges);
        let mut r = reviewed(&changes, "Two problems.\n\nVerdict: request changes", Went::Posted("https://github.com/o/r/pull/7#issuecomment-1".into()));
        let Some(Job::Submit { verdict, text: Some(text), .. }) = r.due(&done, &changes, NOW + 64_000).pop() else { panic!("a submit") };
        assert_eq!(verdict, Verdict::RequestChanges);
        assert!(text.starts_with("The agent's review asks for changes: https://github.com/o/r/pull/7#issuecomment-1\n\n<sub>An agent's review (Codex) of 1111111, submitted by Codealong.</sub>"), "{text}");
        let mut r = reviewed(&changes, "Two problems.\n\nVerdict: request changes", Went::Kept(None));
        let Some(Job::Submit { text: Some(text), .. }) = r.due(&done, &changes, NOW + 64_000).pop() else { panic!("a submit") };
        assert!(text.starts_with("Two problems.\n\n<sub>An agent's review (Codex) of 1111111, submitted by Codealong.</sub>"), "{text}");
        // One whose agent couldn't tell, submitted as a comment by hand, doesn't say it asks for
        // changes.
        let r = reviewed(&changes, "Hm.\n\nVerdict: comment", Went::Posted("https://github.com/o/r/pull/7#issuecomment-1".into()));
        let round = r.store.prs.values().next().and_then(|t| t.rounds.last()).unwrap();
        let text = without_draft(round, &pr(7, HEAD), HEAD, Verdict::Comment).unwrap();
        assert!(text.starts_with("The agent's review: https://github.com/o/r/pull/7#issuecomment-1\n\n"), "{text}");
        // One that isn't for you to submit now says why, and isn't tried again.
        for (submit, verdict, says) in [
            (ReviewSubmit::Approve, "approve", "It wasn't approved for you"),
            (ReviewSubmit::ApproveOrRequestChanges, "request changes", "Changes weren't requested for you"),
            (ReviewSubmit::ApproveOrComment, "request changes", "It wasn't submitted as a comment for you"),
        ] {
            let mut r = reviewed(&cfg(submit), &format!("Fine.\nVerdict: {verdict}"), drafted());
            let job = r.due(&done, &cfg(submit), NOW + 64_000).pop().unwrap();
            r.told(&job, Went::NotSubmitted("your draft review on it has words of yours, so it's yours to submit.".into()), NOW + 65_000).unwrap();
            assert!(r.due(&done, &cfg(submit), NOW + 66_000).is_empty());
            let mut prs = vec![pr(7, HEAD)];
            apply(&mut prs, &r.store, &cfg(submit), &Default::default(), &Default::default(), &[], NOW + 66_000);
            let a = prs[0].auto_review.clone().unwrap();
            assert_eq!((a.submitted, a.note), (None, Some(format!("{says}: your draft review on it has words of yours, so it's yours to submit."))));
        }
        // What reviews.json said when only approvals were reads as an approval.
        let old: Round = serde_json::from_str(r#"{"head":"1111111","stage":"drafted","at":1,"approved":{"at":5}}"#).unwrap();
        assert_eq!(old.submitted, Some(Submission { at: 5, verdict: Verdict::Approve, failed: None }));
        // A draft of only Codealong's rounds is submitted; one with words of yours isn't.
        assert!(codealongs_only("Fine.\n<!-- codealong-review o/r#7 abc -->"));
        assert!(codealongs_only("Round one.\n<!-- codealong-review o/r#7 abc -->\n\n---\n\nRound two.\n<!-- codealong-review o/r#7 def -->"));
        assert!(!codealongs_only("My own notes.\n\n---\n\nFine.\n<!-- codealong-review o/r#7 abc -->"));
    }

    #[test]
    fn a_review_asked_for_by_hand_goes_on_with_reviews_off_and_is_posted_and_submitted_the_same_way() {
        // Reviews off, submitting approvals: only the round asked for by hand goes on.
        let off = ReviewsCfg { submit: ReviewSubmit::Approve, ..ReviewsCfg::default() };
        let done = board(vec![pr(7, HEAD)], vec![session(Status::Unread, NOW + 60_000)]);
        let mut r = Reviewer::default();
        assert!(r.due(&done, &off, NOW).is_empty(), "nothing starts by itself");
        let seat = named_seat("claude", "", "", None);
        let job = Job::Review { pr: pr(7, HEAD), head: HEAD.into(), since: None, seat: seat.clone(), by_hand: true };
        r.told(&job, Went::Started(vec![started("claude", S, NOW)]), NOW).unwrap();
        assert!(r.store.prs["o/r#7"].rounds[0].by_hand);
        read(&mut r, &done, &off, NOW + 61_000, "Fine.\n\nVerdict: approve", "Claude Code");
        let post = r.due(&done, &off, NOW + 62_000).pop().unwrap();
        assert!(matches!(post, Job::Post { .. }), "{post:?}");
        r.told(&post, Went::Drafted("https://github.com/o/r/pull/7/files".into()), NOW + 63_000).unwrap();
        assert_eq!(kinds(&r.due(&done, &off, NOW + 64_000)), ["approve 7 1111111 saying \"\""]);
        // Its card has it, as an automatic one's.
        let mut prs = vec![pr(7, HEAD)];
        apply(&mut prs, &r.store, &off, &Default::default(), &Default::default(), &[], NOW + 64_000);
        assert_eq!(prs[0].auto_review.as_ref().map(|a| (a.stage, a.suggests)), Some((ReviewStage::Drafted, Some(Verdict::Approve))));
        // A round reviews turned off left under way stays as it is.
        let mut r = Reviewer::default();
        let auto = Job::Review { pr: pr(7, HEAD), head: HEAD.into(), since: None, seat, by_hand: false };
        r.told(&auto, Went::Started(vec![started(AUTO, S, NOW)]), NOW).unwrap();
        assert!(r.due(&done, &off, NOW + 61_000).is_empty());
        let jobs = r.due(&done, &on(), NOW + 61_000);
        assert_eq!(kinds(&jobs), [format!("read 7 part 0 in {S}")]);
        r.told(&jobs[0], Went::NotYet, NOW + 61_500).unwrap();
        // It doesn't hold up one asked for by hand, which ends it; one going on does.
        assert!(r.clear_for_hand("o/r#7", true, NOW + 62_000).is_err());
        assert_eq!(r.clear_for_hand("o/r#7", false, NOW + 62_000), Ok(()));
        let ended = &r.store.prs["o/r#7"].rounds[0];
        assert_eq!((ended.stage, ended.note.as_deref()), (ReviewStage::Failed, Some("Automatic reviews were turned off before it was done.")));
        let hand = Job::Review { pr: pr(7, HEAD), head: HEAD.into(), since: None, seat: named_seat("claude", "", "", None), by_hand: true };
        r.told(&hand, Went::Started(vec![started("claude", S, NOW + 63_000)]), NOW + 63_000).unwrap();
        assert!(r.clear_for_hand("o/r#7", false, NOW + 64_000).is_err(), "one asked for by hand goes on");
        // A drafted review's mark names its round, after the commit's: another Mac still finds the
        // commit reviewed, a post tried again finds its round's review there, and another round's
        // review of the same commit, the same words or not, isn't taken for it.
        let plan = draft_plan("Fine.", vec![], &HashMap::new(), None, &pr(7, HEAD), HEAD, "Claude Code", NOW);
        assert!(plan.body.ends_with(&draft_mark("o/r#7", HEAD, NOW)) && plan.body.contains(&marker("o/r#7", HEAD)), "{}", plan.body);
        assert!(!plan.body.contains(&draft_mark("o/r#7", HEAD, NOW + 1)));
        let last = plan.body.lines().last().unwrap();
        assert!(last.starts_with(crate::github::REVIEW_MARK) && last.ends_with("-->"), "a mark line, which an edit keeps hidden: {last}");
    }

    #[test]
    fn a_review_of_a_pr_of_yours_asked_for_by_hand_goes_back_with_its_reviews_off() {
        let mut r = Reviewer::default();
        let maker = Session { status: Status::Idle, last_activity: NOW, live: true, ..fixtures::session(MAKER) };
        let b = board(vec![mine(7, HEAD)], vec![maker, session(Status::Unread, NOW + 60_000)]);
        let off = ReviewsCfg::default();
        assert!(off.own.send && !off.own.auto);
        let job = Job::Review { pr: mine(7, HEAD), head: HEAD.into(), since: None, seat: named_seat("codex", "", "", None), by_hand: true };
        r.told(&job, Went::Started(vec![started("codex", S, NOW)]), NOW).unwrap();
        read(&mut r, &b, &off, NOW + 61_000, "Two problems.", "Codex");
        let post = r.due(&b, &off, NOW + 62_000).pop().unwrap();
        r.told(&post, Went::Kept(None), NOW + 63_000).unwrap();
        assert_eq!(kinds(&r.due(&b, &off, NOW + 64_000)), [format!("send 7 1111111 by Codex to {MAKER}")]);
        // An automatic one isn't, with reviews of your PRs off, and says so.
        let mut r = Reviewer::default();
        let auto = Job::Review { pr: mine(7, HEAD), head: HEAD.into(), since: None, seat: named_seat("codex", "", "", None), by_hand: false };
        r.told(&auto, Went::Started(vec![started("codex", S, NOW)]), NOW).unwrap();
        read(&mut r, &b, &own(), NOW + 61_000, "Two problems.", "Codex");
        let post = r.due(&b, &own(), NOW + 62_000).pop().unwrap();
        r.told(&post, Went::Kept(None), NOW + 63_000).unwrap();
        assert!(r.due(&b, &off, NOW + 64_000).is_empty());
        let mut prs = vec![mine(7, HEAD)];
        apply(&mut prs, &r.store, &off, &Default::default(), &Default::default(), &b.lanes, NOW + 64_000);
        assert_eq!(prs[0].auto_review.as_ref().and_then(|a| a.note.as_deref()), Some("Handing reviews back is off: send it from here."));
    }

    #[test]
    fn a_review_submitted_by_hand_has_what_you_wrote_first() {
        let draft = |whole: &str| crate::github::Draft { id: "R".into(), body: whole.into(), commit: None, url: None, comments: Vec::new(), count: 1, whole: whole.into() };
        let agents = draft("Fix it.\n<!-- codealong-review o/r#7 abc -->");
        let yours = draft("My own notes.");
        // For you: the agent's draft goes as it is; one with words of yours is yours to submit.
        assert_eq!(submission(Some(&agents), None, None), Ok(String::new()));
        assert!(submission(Some(&yours), None, None).is_err());
        // By hand: your draft goes, words of yours in it or not, with what you wrote before its text.
        assert_eq!(submission(Some(&yours), None, Some("  Also this. ")), Ok("Also this.".into()));
        // With no draft, the review's own text, after what you wrote.
        assert_eq!(submission(None, Some("The agent's review"), Some("")), Ok("The agent's review".into()));
        assert_eq!(submission(None, Some("The agent's review"), Some("Also this.")), Ok("Also this.\n\nThe agent's review".into()));
        assert_eq!(submission(None, Some(""), Some("Looks good.")), Ok("Looks good.".into()));
        assert!(submission(None, None, Some("Also this.")).is_err());
    }

    #[test]
    fn the_reviewer_is_an_account_and_model_and_keeps_its_session_for_the_pr() {
        let cfg = |account: &str, model: &str| ReviewsCfg { auto: true, account: account.into(), model: model.into(), ..ReviewsCfg::default() };
        let mut t = Tracked::default();
        t.sessions.insert("claude opus".into(), S.into());
        let named = seat(&cfg(" claude ", "opus"), Some(&t));
        assert_eq!(named, Seat { who: "claude opus".into(), account: Some("claude".into()), model: Some("opus".into()), effort: None, session: Some(S.into()), avoid: None });
        // Another reviewer starts afresh; Auto is named for what it is.
        assert_eq!(seat(&cfg("claude", ""), Some(&t)).session, None);
        assert_eq!(seat(&cfg("", ""), None), Seat { who: AUTO.into(), account: None, model: None, effort: None, session: None, avoid: None });
        assert_eq!(seat(&cfg("", "opus"), None).who, "auto opus");
        // Its effort is in its name, as its model is: another effort is another session.
        let at = seat(&ReviewsCfg { effort: " xhigh ".into(), ..cfg("claude", "opus") }, Some(&t));
        assert_eq!((at.who.as_str(), at.effort.as_deref(), at.session.as_deref()), ("claude opus at xhigh", Some("xhigh"), None));
        assert_eq!(seat(&ReviewsCfg { effort: "high".into(), ..cfg("", "") }, None).who, "auto at high");
        // And its part reads them back for a turn cut off partway.
        let part = |who: &str| Part::asked(who, None, None, 0);
        assert_eq!(part("claude opus at xhigh").on(), (Some("opus"), Some("xhigh")));
        assert_eq!(part("claude at high").on(), (None, Some("high")));
        assert_eq!(part("auto opus").on(), (Some("opus"), None));
        assert_eq!(part("claude").on(), (None, None));
        // Its instructions are config.toml's, else Codealong's.
        assert_eq!(instructions(&cfg("", "")), INSTRUCTIONS);
        assert_eq!(instructions(&ReviewsCfg { prompt: "  Start two subagents.  ".into(), ..cfg("", "") }), "Start two subagents.");
        // A PR whose commit trailer says which harness wrote it keeps Auto off that harness when
        // other_harness is on, and starts afresh when its kept session hit a usage limit.
        let trailer_pr = PrItem { authored_by: Some(Harness::Claude), ..pr(7, HEAD) };
        assert_eq!(seat_for(&on(), None, &[], &trailer_pr).avoid, Some(Harness::Claude));
        assert_eq!(seat_for(&ReviewsCfg { other_harness: false, ..on() }, None, &[], &trailer_pr).avoid, None);
        let mut t = Tracked::default();
        t.sessions.insert(AUTO.into(), S.into());
        let limited = Session {
            limit: Some(crate::model::SessionLimit { at: NOW - 1_000, resets_at: Some(NOW + 60_000), ..Default::default() }),
            ..session(Status::Error, NOW - 1_000)
        };
        let b = board(vec![trailer_pr.clone()], vec![limited]);
        assert_eq!(seat_in(&on(), Some(&t), &b, &trailer_pr, NOW).session, None);
    }

    #[test]
    fn a_reviewer_that_doesnt_start_or_passes_ends_the_round_saying_why() {
        let mut r = Reviewer::default();
        let jobs = r.due(&board(vec![pr(7, HEAD)], vec![]), &on(), NOW);
        let said = r.told(&jobs[0], Went::Started(vec![Part::failed(AUTO, "codex can't start a session in the background".into(), NOW)]), NOW);
        assert!(said.unwrap_err().text.contains("codex can't start"));
        assert_eq!(r.store.prs["o/r#7"].rounds[0].stage, ReviewStage::Failed);
        let mut r = Reviewer::default();
        let jobs = r.due(&board(vec![pr(7, HEAD)], vec![]), &on(), NOW);
        r.told(&jobs[0], Went::Started(vec![started(AUTO, S, NOW)]), NOW).unwrap();
        let b = board(vec![pr(7, HEAD)], vec![session(Status::Unread, NOW + 1)]);
        let job = r.due(&b, &on(), NOW + 2).pop().unwrap();
        r.told(&job, Went::Skipped("It's generated code.".into()), NOW + 2).unwrap();
        assert!(r.due(&b, &on(), NOW + 3).is_empty());
        let round = &r.store.prs["o/r#7"].rounds[0];
        assert_eq!((round.stage, round.note.as_deref()), (ReviewStage::Skipped, Some("It's generated code.")));
    }

    #[test]
    fn a_pr_is_held_out_of_what_waits_on_you_until_its_review_is_done() {
        let fresh = |head: &str| PrItem { requested_at: Some(NOW - 60_000), updated_at: NOW - 60_000, ..pr(7, head) };
        let shown = |store: &Store, p: PrItem, on: bool, at: i64| {
            let mut prs = vec![p];
            apply(&mut prs, store, &ReviewsCfg { auto: on, ..ReviewsCfg::default() }, &Default::default(), &Default::default(), &[], at);
            prs.remove(0)
        };
        let mut r = Reviewer::default();
        // Asked for a minute ago, with no round yet: held, and its card says one is to come.
        let p = shown(&r.store, fresh(HEAD), true, NOW);
        assert_eq!(p.auto_review.as_ref().map(|a| (a.stage, a.head.as_str(), a.holds)), Some((ReviewStage::Queued, HEAD, true)));
        assert!(!p.waits_on_review());
        // Reviews off: nothing said, and it waits on you.
        let p = shown(&r.store, fresh(HEAD), false, NOW);
        assert!(p.auto_review.is_none() && p.waits_on_review());
        // Asked for long ago, with no round yet: not held.
        let old = PrItem { requested_at: Some(NOW - HOLD_MS), updated_at: NOW - HOLD_MS, ..pr(7, HEAD) };
        assert!(shown(&r.store, old, true, NOW).waits_on_review());
        // Under way: held, for so long at most.
        let jobs = r.due(&board(vec![fresh(HEAD)], vec![]), &on(), NOW);
        r.told(&jobs[0], Went::Started(vec![started(AUTO, S, NOW)]), NOW).unwrap();
        assert!(!shown(&r.store, fresh(HEAD), true, NOW + 60_000).waits_on_review());
        assert!(shown(&r.store, fresh(HEAD), true, NOW + HOLD_MS).waits_on_review());
        // Done: it waits on you.
        let b = board(vec![fresh(HEAD)], vec![session(Status::Unread, NOW + 5)]);
        read(&mut r, &b, &on(), NOW + 6, "A.", "Claude Code");
        let job = r.due(&b, &on(), NOW + 7).pop().unwrap();
        r.told(&job, Went::Posted("link".into()), NOW + 7).unwrap();
        assert!(shown(&r.store, fresh(HEAD), true, NOW + 8).waits_on_review());
        // New commits: held again until they're reviewed; a round that didn't get done hands it back.
        let moved = PrItem { updated_at: NOW + 9, ..fresh(NEXT) };
        let p = shown(&r.store, moved.clone(), true, NOW + 10);
        assert_eq!(p.auto_review.as_ref().map(|a| (a.stage, a.behind, a.holds)), Some((ReviewStage::Posted, true, true)));
        r.store.prs.get_mut("o/r#7").unwrap().rounds.push(Round::new(NEXT, ReviewStage::Failed, NOW + 11));
        assert!(shown(&r.store, moved, true, NOW + 12).waits_on_review());
    }

    /// Agents go back and forth on a PR before you sign it off: once the agent's latest review
    /// approves the PR's commit, it waits on your sign-off. The author's commits after that hold it
    /// out of what waits on you until they're reviewed, as any new commits do, so only what you
    /// can act on waits on you; the earlier approval is kept for its card to say.
    #[test]
    fn an_approved_pr_waits_on_your_sign_off_once_its_latest_commit_is_approved() {
        let fresh = |head: &str| PrItem { requested_at: Some(NOW - 60_000), updated_at: NOW - 60_000, ..pr(7, head) };
        let shown = |store: &Store, p: PrItem, cfg: &ReviewsCfg, at: i64| {
            let mut prs = vec![p];
            apply(&mut prs, store, cfg, &Default::default(), &Default::default(), &[], at);
            prs.remove(0)
        };
        let round = |head: &str, stage: ReviewStage, review: Option<&str>, at: i64| {
            let state = if review.is_some() { PartState::Done } else { PartState::Reviewing };
            Round { parts: vec![Part { state, review: review.map(str::to_string), ..started(AUTO, S, at) }], ..Round::new(head, stage, at) }
        };
        let mut store = Store::default();
        fn rounds(store: &mut Store) -> &mut Vec<Round> {
            &mut store.prs.entry("o/r#7".into()).or_default().rounds
        }
        rounds(&mut store).push(round(HEAD, ReviewStage::Posted, Some("Looks right.\n\nVerdict: approve"), NOW));
        let p = shown(&store, fresh(HEAD), &on(), NOW + 1);
        assert!(p.waits_on_review());
        assert_eq!(p.auto_review.as_ref().and_then(|a| a.approves.as_deref()), Some(HEAD));
        // The author pushes again: held while the commits settle and are read, its approval kept.
        let moved = PrItem { updated_at: NOW + 2, ..fresh(NEXT) };
        let p = shown(&store, moved.clone(), &on(), NOW + 3);
        assert!(!p.waits_on_review(), "nothing to sign off until the new commits are read");
        assert_eq!(p.auto_review.as_ref().map(|a| (a.behind, a.holds, a.approves.as_deref())), Some((true, true, Some(HEAD))));
        rounds(&mut store).push(round(NEXT, ReviewStage::Reviewing, None, NOW + 4));
        let p = shown(&store, moved.clone(), &on(), NOW + 5);
        assert!(!p.waits_on_review());
        assert_eq!(p.auto_review.as_ref().and_then(|a| a.approves.as_deref()), Some(HEAD));
        // They're approved too: the sign-off is of the new commit.
        rounds(&mut store).last_mut().unwrap().parts[0] = Part { state: PartState::Done, review: Some("Verdict: approve".into()), ..started(AUTO, S, NOW + 4) };
        rounds(&mut store).last_mut().unwrap().stage = ReviewStage::Posted;
        let p = shown(&store, moved.clone(), &on(), NOW + 6);
        assert!(p.waits_on_review());
        assert_eq!(p.auto_review.as_ref().and_then(|a| a.approves.as_deref()), Some(NEXT));
        // A round that asks for changes ends it: submitted as yours, the PR is held until it's gone.
        rounds(&mut store).last_mut().unwrap().parts[0].review = Some("Verdict: request changes".into());
        let p = shown(&store, moved.clone(), &on(), NOW + 7);
        assert!(p.waits_on_review() && p.auto_review.as_ref().unwrap().approves.is_none(), "changes are left to you");
        let submitting = ReviewsCfg { submit: ReviewSubmit::RequestChanges, ..on() };
        assert!(!shown(&store, moved.clone(), &submitting, NOW + 7).waits_on_review());
        // And the commits after a request for changes are held for their review again.
        let again = PrItem { updated_at: NOW + 8, ..fresh("3333333cccc") };
        assert!(!shown(&store, again, &on(), NOW + 9).waits_on_review());
        // A PR of yours says nothing of a sign-off: you can't approve it.
        let mut store = Store::default();
        rounds(&mut store).push(round(HEAD, ReviewStage::Kept, Some("Verdict: approve"), NOW));
        let p = shown(&store, PrItem { mine: true, ..fresh(HEAD) }, &on(), NOW + 1);
        assert!(p.auto_review.is_some_and(|a| a.approves.is_none()));
        // Nor one that no longer waits on your review: you reviewed it, or it ended.
        for p in [PrItem { review_requested: false, ..fresh(HEAD) }, PrItem { state: PrState::Merged, ..fresh(HEAD) }] {
            assert!(shown(&store, p, &on(), NOW + 1).auto_review.is_some_and(|a| a.approves.is_none()));
        }
    }

    #[test]
    fn new_commits_are_reviewed_again_in_the_same_session_once_the_branch_holds_still() {
        let mut r = Reviewer::default();
        let jobs = r.due(&board(vec![pr(7, HEAD)], vec![]), &on(), NOW);
        r.told(&jobs[0], Went::Started(vec![started(AUTO, S, NOW)]), NOW).unwrap();
        let b = board(vec![pr(7, HEAD)], vec![session(Status::Idle, NOW + 1)]);
        read(&mut r, &b, &on(), NOW + 2, "A.", "Claude Code");
        let job = r.due(&b, &on(), NOW + 2).pop().unwrap();
        r.told(&job, Went::Posted("link".into()), NOW + 3).unwrap();
        let moved = board(vec![pr(7, NEXT)], vec![session(Status::Idle, NOW + 1)]);
        assert!(r.due(&moved, &on(), NOW + 10).is_empty(), "first seen");
        let mut prs = vec![pr(7, NEXT)];
        apply(&mut prs, &r.store, &on(), &Default::default(), &Default::default(), &[], NOW);
        assert!(prs[0].auto_review.as_ref().unwrap().behind);
        assert!(r.due(&moved, &on(), NOW + QUIET_MS).is_empty(), "still settling");
        assert_eq!(kinds(&r.due(&moved, &on(), NOW + 10 + QUIET_MS)), [format!("review 7 2222222 since Some(\"1111111\") by auto in {S}")]);
    }

    #[test]
    fn only_open_prs_that_wait_on_your_review_and_only_while_reviews_are_on() {
        let mut r = Reviewer::default();
        let mine = PrItem { mine: true, ..pr(1, HEAD) };
        let draft = PrItem { state: PrState::Draft, ..pr(2, HEAD) };
        let not_asked = PrItem { review_requested: false, ..pr(3, HEAD) };
        let merged = PrItem { state: PrState::Merged, ..pr(4, HEAD) };
        let no_head = PrItem { head_oid: None, ..pr(5, HEAD) };
        assert!(r.due(&board(vec![mine, draft, not_asked, merged, no_head], vec![]), &on(), NOW).is_empty());
        assert!(r.due(&board(vec![pr(6, HEAD)], vec![]), &ReviewsCfg::default(), NOW).is_empty(), "off");
        let unread = Board { github_login: None, ..board(vec![pr(6, HEAD)], vec![]) };
        assert!(r.due(&unread, &on(), NOW).is_empty(), "GitHub not read yet");
        assert_eq!(kinds(&r.due(&board(vec![pr(6, HEAD)], vec![]), &on(), NOW)), ["review 6 1111111 since None by auto"]);
    }

    #[test]
    fn a_few_rounds_run_at_once_and_one_waiting_on_you_holds_no_place() {
        let mut r = Reviewer::default();
        let prs = vec![pr(3, HEAD), pr(1, HEAD), pr(2, HEAD)];
        let jobs = r.due(&board(prs.clone(), vec![]), &on(), NOW);
        // The longest waiting first.
        assert_eq!(kinds(&jobs), ["review 1 1111111 since None by auto", "review 2 1111111 since None by auto"]);
        r.told(&jobs[0], Went::Started(vec![started(AUTO, S, NOW)]), NOW).unwrap();
        r.told(&jobs[1], Went::Started(vec![started(AUTO, "claude:claude:two", NOW)]), NOW).unwrap();
        let two = at("claude:claude:two", Status::Working, NOW);
        assert!(r.due(&board(prs.clone(), vec![session(Status::Working, NOW), two.clone()]), &on(), NOW + 1).is_empty());
        // One waits on you to allow something: the third starts.
        let waits = Session { asks: false, ..session(Status::Needs, NOW + 5) };
        assert_eq!(kinds(&r.due(&board(prs, vec![waits, two]), &on(), NOW + 6)), ["review 3 1111111 since None by auto"]);
    }

    #[test]
    fn a_start_that_named_no_session_finds_it_by_its_prompt() {
        let mut r = Reviewer::default();
        let jobs = r.due(&board(vec![pr(7, HEAD)], vec![]), &on(), NOW);
        let pending = Pending { lane: "/r".into(), account: "codex".into(), at: NOW };
        r.told(&jobs[0], Went::Started(vec![Part::asked(AUTO, None, Some(pending.clone()), NOW)]), NOW).unwrap();
        let other = Session { account: "codex".into(), started_at: Some(NOW + 1), first_prompt: Some("something else".into()), ..fixtures::session("codex:codex:x") };
        let mine = Session { account: "codex".into(), started_at: Some(NOW + 2), first_prompt: Some("Review pull request #7 in o/r, “Add teams” (https://github.com/o/r/pull/7)".into()), status: Status::Working, ..fixtures::session("codex:codex:y") };
        r.due(&board(vec![pr(7, HEAD)], vec![other.clone()]), &on(), NOW + 3);
        assert_eq!(r.store.prs["o/r#7"].rounds[0].parts[0].session, None);
        r.due(&board(vec![pr(7, HEAD)], vec![other, mine]), &on(), NOW + 4);
        assert_eq!(r.store.prs["o/r#7"].rounds[0].parts[0].session.as_deref(), Some("codex:codex:y"));
        assert_eq!(r.store.prs["o/r#7"].sessions[AUTO], "codex:codex:y");
        // Found at work: its turn has begun, should it end before the next board.
        assert_eq!(r.store.prs["o/r#7"].rounds[0].parts[0].began, Some(NOW + 4));
        // One that never shows fails its part, and so the round.
        let mut r = Reviewer::default();
        let jobs = r.due(&board(vec![pr(7, HEAD)], vec![]), &on(), NOW);
        r.told(&jobs[0], Went::Started(vec![Part::asked(AUTO, None, Some(pending), NOW)]), NOW).unwrap();
        r.due(&board(vec![pr(7, HEAD)], vec![]), &on(), NOW + SHOW_MS + 1);
        r.due(&board(vec![pr(7, HEAD)], vec![]), &on(), NOW + SHOW_MS + 2);
        let round = &r.store.prs["o/r#7"].rounds[0];
        assert_eq!((round.stage, round.note.as_deref()), (ReviewStage::Failed, Some("Its session never showed up.")));
    }

    /// A reader keeps a prompt's first words only, which a long title cuts before the PR's link
    /// (an Antigravity review of a real PR, 2026-10-18): the session is known by how a review's
    /// prompt opens. One that shows only after its round gave up on it is still read and
    /// posted, and a round from before where to look was kept is taken up by Try again.
    #[test]
    fn a_review_session_is_found_by_its_opening_however_late_it_shows() {
        let long = PrItem { title: "Replace CMMC Level 2 and CUI marketing with the Level 1 status line, on every page".into(), ..pr(7, HEAD) };
        let files = Reading { md: "m".into(), diff: "d".into(), changes: None };
        let asked = crate::util::one_line(&first_prompt(&long, HEAD, None, false, "gwen", &Goes::Post(ReviewPost::Draft), INSTRUCTIONS, &files), 140);
        assert!(!asked.contains(&long.url), "the link is cut: {asked}");
        // A repo name long enough takes the opening itself past what's kept: what's left of it
        // still names the PR, once it's past the number, and no other.
        let far = PrItem { repo_slug: format!("{}/{}", "o".repeat(39), "r".repeat(100)), ..long.clone() };
        let kept = crate::util::one_line(&first_prompt(&far, HEAD, None, false, "gwen", &Goes::Post(ReviewPost::Draft), INSTRUCTIONS, &files), 140);
        assert!(!kept.starts_with(&opening(&far)) && names(&kept, &far), "{kept}");
        assert!(!names(&kept, &PrItem { number: 70, ..far.clone() }));
        assert!(!names("Review pull request #7 in…", &far) && !names("…", &far));
        let agy = |key: &str, first: &str, activity: i64| Session { account: "antigravity".into(), status: Status::Unread, last_activity: activity, first_prompt: Some(first.into()), ..fixtures::session(key) };
        let mine = agy("antigravity:antigravity:late", &asked, NOW + SHOW_MS + 60_000);
        // Another PR's review, whose number starts the same, isn't it.
        let other = agy("antigravity:antigravity:other", "Review pull request #70 in o/r, “Something else” (https://github.com/o/r/pull/70)", NOW + 5);
        let pending = Pending { lane: "/r".into(), account: "antigravity".into(), at: NOW };
        let start = |r: &mut Reviewer| {
            let jobs = r.due(&board(vec![long.clone()], vec![]), &on(), NOW);
            r.told(&jobs[0], Went::Started(vec![Part::asked(AUTO, None, Some(pending.clone()), NOW)]), NOW).unwrap();
        };

        let mut r = Reviewer::default();
        start(&mut r);
        r.due(&board(vec![long.clone()], vec![other.clone(), Session { status: Status::Working, ..mine.clone() }]), &on(), NOW + 3);
        assert_eq!(r.store.prs["o/r#7"].rounds[0].parts[0].session.as_deref(), Some("antigravity:antigravity:late"));

        // It doesn't show in time: the round fails, keeping where to look.
        let mut r = Reviewer::default();
        start(&mut r);
        let none = board(vec![long.clone()], vec![other.clone()]);
        r.due(&none, &on(), NOW + SHOW_MS + 1);
        r.due(&none, &on(), NOW + SHOW_MS + 2);
        assert_eq!(r.store.prs["o/r#7"].rounds[0].stage, ReviewStage::Failed);
        // It shows, its review written: the round is under way again, and its review is read and
        // posted.
        let shown = board(vec![long.clone()], vec![other.clone(), mine.clone()]);
        let jobs = r.due(&shown, &on(), NOW + SHOW_MS + 70_000);
        assert_eq!(kinds(&jobs), ["read 7 part 0 in antigravity:antigravity:late"]);
        let round = &r.store.prs["o/r#7"].rounds[0];
        assert_eq!((round.stage, round.parts[0].state, round.ended_at, round.note.as_deref()), (ReviewStage::Reviewing, PartState::Reviewing, None, None));
        assert_eq!(r.store.prs["o/r#7"].sessions[AUTO], "antigravity:antigravity:late");
        r.told(&jobs[0], Went::Read { review: "Two problems.".into(), agent: "Antigravity".into() }, NOW + SHOW_MS + 80_000).unwrap();
        assert_eq!(kinds(&r.due(&shown, &on(), NOW + SHOW_MS + 90_000)), ["post 7 \"Two problems.\" by Antigravity"]);

        // Not once the PR has moved on, or with reviews off.
        for (pr, cfg) in [(PrItem { head_oid: Some(NEXT.into()), ..long.clone() }, on()), (long.clone(), ReviewsCfg::default())] {
            let mut r = Reviewer::default();
            start(&mut r);
            r.due(&none, &on(), NOW + SHOW_MS + 1);
            r.due(&board(vec![pr.clone()], vec![mine.clone()]), &cfg, NOW + SHOW_MS + 70_000);
            assert_eq!(r.store.prs["o/r#7"].rounds[0].stage, ReviewStage::Failed, "{pr:?}");
        }

        // A round failed before where to look was kept goes on only by hand, found anywhere here.
        let mut r = Reviewer::default();
        start(&mut r);
        r.due(&none, &on(), NOW + SHOW_MS + 1);
        r.store.prs.get_mut("o/r#7").unwrap().rounds[0].parts[0].pending = None;
        r.due(&shown, &on(), NOW + SHOW_MS + 70_000);
        assert_eq!(r.store.prs["o/r#7"].rounds[0].stage, ReviewStage::Failed);
        let elsewhere = Board { lanes: vec![fixtures::lane("/r", vec![other]), fixtures::lane("/r-wt", vec![mine])], ..shown };
        assert!(r.take_up("o/r#7", &elsewhere, &long));
        let round = &r.store.prs["o/r#7"].rounds[0];
        assert_eq!((round.stage, round.by_hand, round.parts[0].session.as_deref()), (ReviewStage::Reviewing, true, Some("antigravity:antigravity:late")));
        // Nothing else is taken up: a round under way, or one that failed for another reason.
        assert!(!r.take_up("o/r#7", &elsewhere, &long));
    }

    /// A session can read as ended, with nothing running it, before its harness shows the turn
    /// it was asked for: an Antigravity review's row was read before the `agy` running it was
    /// listed, and its round failed ten seconds in while it went on to write its review
    /// (2026-10-18). Its turn isn't over until it's been seen at work, or ten minutes pass; it
    /// holds its place meanwhile.
    #[test]
    fn a_review_session_isnt_taken_for_stopped_before_its_turn_shows() {
        let asked = || {
            let mut r = Reviewer::default();
            let jobs = r.due(&board(vec![pr(7, HEAD)], vec![]), &on(), NOW);
            r.told(&jobs[0], Went::Started(vec![started(AUTO, S, NOW)]), NOW).unwrap();
            r
        };
        let seen_idle = board(vec![pr(7, HEAD)], vec![Session { live: false, ..session(Status::Idle, NOW + 3_000) }]);
        let mut r = asked();
        let jobs = r.due(&seen_idle, &on(), NOW + 10_000);
        assert_eq!(kinds(&jobs), [format!("read 7 part 0 in {S}")]);
        r.told(&jobs[0], Went::NoReview, NOW + 10_500).unwrap();
        let round = &r.store.prs["o/r#7"].rounds[0];
        assert_eq!((round.stage, round.parts[0].state), (ReviewStage::Reviewing, PartState::Reviewing));
        let more = board(vec![pr(7, HEAD), pr(8, HEAD), pr(9, HEAD)], seen_idle.lanes[0].sessions.clone());
        assert_eq!(kinds(&r.due(&more, &on(), NOW + 11_000)), ["review 8 1111111 since None by auto"], "its place is kept while it starts");
        // Its turn shows, and ends with its review: read and posted.
        assert!(r.due(&board(vec![pr(7, HEAD)], vec![session(Status::Working, NOW + 40_000)]), &on(), NOW + 41_000).is_empty());
        let done = board(vec![pr(7, HEAD)], vec![session(Status::Unread, NOW + 400_000)]);
        read(&mut r, &done, &on(), NOW + 401_000, "Two problems.", "Antigravity");
        assert_eq!(kinds(&r.due(&done, &on(), NOW + 402_000)), ["post 7 \"Two problems.\" by Antigravity"]);

        // Seen at work, then ended without a review: it stopped.
        let mut r = asked();
        r.due(&board(vec![pr(7, HEAD)], vec![session(Status::Working, NOW + 5_000)]), &on(), NOW + 6_000);
        let job = r.due(&seen_idle, &on(), NOW + 10_000).pop().unwrap();
        r.told(&job, Went::NoReview, NOW + 10_500).unwrap();
        r.due(&seen_idle, &on(), NOW + 11_000);
        let round = &r.store.prs["o/r#7"].rounds[0];
        assert_eq!((round.stage, round.note.as_deref()), (ReviewStage::Failed, Some(STOPPED)));
        // Never seen at work in ten minutes: so did this one.
        let mut r = asked();
        let job = r.due(&seen_idle, &on(), NOW + SHOW_MS + 1).pop().unwrap();
        r.told(&job, Went::NoReview, NOW + SHOW_MS + 2).unwrap();
        r.due(&seen_idle, &on(), NOW + SHOW_MS + 3);
        assert_eq!(r.store.prs["o/r#7"].rounds[0].stage, ReviewStage::Failed);
        // Told to carry on, it's starting again until it's seen at work on that.
        let mut r = asked();
        r.due(&board(vec![pr(7, HEAD)], vec![session(Status::Working, NOW + 5_000)]), &on(), NOW + 6_000);
        r.store.prs.get_mut("o/r#7").unwrap().rounds[0].parts[0].carried_on.push(NOW + 60_000);
        assert!(r.store.prs["o/r#7"].rounds[0].parts[0].starting(NOW + 61_000));
    }

    /// A round that took its session for stopped, of the commit the PR is still at, is taken up
    /// again as that session goes on: by itself once it's at work again, if it was never seen at
    /// work before (as it wasn't, read too early); by Try again once it did anything since. Not
    /// once someone else has prompted it.
    #[test]
    fn a_review_session_taken_for_stopped_is_taken_up_as_it_goes_on() {
        let gave_up = NOW + 10_000;
        let failed = |began: Option<i64>| {
            let mut round = Round::new(HEAD, ReviewStage::Failed, NOW);
            round.ended_at = Some(gave_up);
            round.note = Some(STOPPED.into());
            round.parts = vec![Part { state: PartState::Failed, note: Some(STOPPED.into()), began, ..started(AUTO, S, NOW) }];
            let mut r = Reviewer::default();
            r.store.prs.insert("o/r#7".into(), Tracked { rounds: vec![round], ..Tracked::default() });
            r
        };
        let stage = |r: &Reviewer| r.store.prs["o/r#7"].rounds[0].stage;
        let prompted = |at: i64| Session { last_prompt_at: at, ..session(Status::Working, NOW + 30_000) };
        let working = board(vec![pr(7, HEAD)], vec![prompted(NOW + 2_000)]);
        let mut r = failed(None);
        assert!(r.due(&working, &on(), NOW + 31_000).is_empty());
        let round = &r.store.prs["o/r#7"].rounds[0];
        assert_eq!((round.stage, round.parts[0].state, round.ended_at, round.note.as_deref(), round.by_hand), (ReviewStage::Reviewing, PartState::Reviewing, None, None, false));
        let done = board(vec![pr(7, HEAD)], vec![Session { last_prompt_at: NOW + 2_000, ..session(Status::Unread, NOW + 400_000) }]);
        read(&mut r, &done, &on(), NOW + 401_000, "Two problems.", "Antigravity");
        assert_eq!(kinds(&r.due(&done, &on(), NOW + 402_000)), ["post 7 \"Two problems.\" by Antigravity"]);

        // Ended already, its review written: only Try again takes it up.
        let mut r = failed(None);
        r.due(&done, &on(), NOW + 401_000);
        assert_eq!(stage(&r), ReviewStage::Failed);
        assert!(r.take_up("o/r#7", &done, &pr(7, HEAD)));
        assert_eq!((stage(&r), r.store.prs["o/r#7"].rounds[0].by_hand), (ReviewStage::Reviewing, true));
        assert_eq!(kinds(&r.due(&done, &on(), NOW + 402_000)), [format!("read 7 part 0 in {S}")]);
        // Nothing since it was given up on.
        let still = board(vec![pr(7, HEAD)], vec![Session { last_prompt_at: NOW + 2_000, ..session(Status::Idle, NOW + 5_000) }]);
        assert!(!failed(None).take_up("o/r#7", &still, &pr(7, HEAD)));
        // Seen at work before it stopped: you stopped it, say, so it goes on only by Try again.
        let mut r = failed(Some(NOW + 5_000));
        r.due(&working, &on(), NOW + 31_000);
        assert_eq!(stage(&r), ReviewStage::Failed);
        assert!(r.take_up("o/r#7", &working, &pr(7, HEAD)));
        // Prompted since: what it's at isn't the review.
        let other = board(vec![pr(7, HEAD)], vec![prompted(gave_up + 1)]);
        let mut r = failed(None);
        r.due(&other, &on(), NOW + 31_000);
        assert_eq!(stage(&r), ReviewStage::Failed);
        assert!(!r.take_up("o/r#7", &other, &pr(7, HEAD)));
        // Nor once the PR has moved on.
        let moved = board(vec![pr(7, NEXT)], vec![prompted(NOW + 2_000)]);
        assert!(!failed(None).take_up("o/r#7", &moved, &pr(7, NEXT)));
    }

    #[test]
    fn a_failed_start_or_post_is_said_and_not_tried_again_for_that_commit() {
        let mut r = Reviewer::default();
        let b = board(vec![pr(7, HEAD)], vec![]);
        let jobs = r.due(&b, &on(), NOW);
        let said = r.told(&jobs[0], Went::NotStarted("GitHub: Not Found".into()), NOW).unwrap_err();
        assert!(said.text.contains("Not Found"));
        // Its notice offers to start it again: nothing else would.
        assert_eq!(said.again.as_deref(), Some("o/r#7"));
        assert!(r.due(&b, &on(), NOW + 10 * QUIET_MS).is_empty());
        let jobs = r.due(&board(vec![pr(8, HEAD)], vec![]), &on(), NOW);
        let said = r.told(&jobs[0], Went::Started(vec![Part::failed(AUTO, "claude can't start a session in the background".into(), NOW)]), NOW).unwrap_err();
        assert!(said.text.contains("in the background"));
        assert_eq!(said.again.as_deref(), Some("o/r#8"));
        assert_eq!(r.store.prs["o/r#8"].rounds[0].stage, ReviewStage::Failed);
        // Posting is tried a few times, waiting longer each time.
        let mut r = Reviewer::default();
        let jobs = r.due(&b, &on(), NOW);
        r.told(&jobs[0], Went::Started(vec![started(AUTO, S, NOW)]), NOW).unwrap();
        let done = board(vec![pr(7, HEAD)], vec![session(Status::Unread, NOW + 1)]);
        read(&mut r, &done, &on(), NOW + 2, "A.", "Claude Code");
        let mut when = NOW + 2;
        let mut waits = Vec::new();
        for _ in 1..POST_TRIES {
            let job = r.due(&done, &on(), when).pop().unwrap();
            assert!(r.told(&job, Went::PostFailed("gh: Not Found".into()), when).is_err());
            let next = (when..).step_by(1000).find(|&t| !r.due(&done, &on(), t).is_empty()).unwrap();
            waits.push((next - when) / 1000);
            // That due started a job: say it's still failing.
            r.busy.clear();
            when = next;
        }
        assert_eq!(waits, [30, 60, 120]);
        let job = r.due(&done, &on(), when).pop().unwrap();
        let said = r.told(&job, Went::PostFailed("gh: Not Found".into()), when).unwrap_err();
        assert!(said.text.contains("Not Found"));
        // Its Try again posts the review there is (`again`): nothing else would.
        assert_eq!(said.again.as_deref(), Some("o/r#7"));
        assert_eq!(r.store.prs["o/r#7"].rounds[0].stage, ReviewStage::Failed);
        assert!(r.due(&done, &on(), when + 10 * QUIET_MS).is_empty());
    }

    /// A start that couldn't reach GitHub to read the PR (the network coming back after a wake,
    /// GitHub struggling) isn't a round yet: it's started again, waiting longer each time, its
    /// notice saying so and offering Try again, and only after a quarter of an hour is it a round
    /// that didn't start. One that gets through counts its tries afresh.
    #[test]
    fn a_start_that_couldnt_reach_github_is_tried_again_a_few_times() {
        let b = board(vec![pr(7, HEAD)], vec![]);
        let offline = || Went::Unreachable("GitHub: gh error connecting to api.github.com".into());
        let mut r = Reviewer::default();
        let mut when = NOW;
        let mut waits = Vec::new();
        for _ in 1..START_TRIES {
            let job = r.due(&b, &on(), when).pop().expect("a start");
            let said = r.told(&job, offline(), when).unwrap_err();
            assert!(said.text.ends_with("It tries again in a few minutes."), "{}", said.text);
            assert_eq!(said.again.as_deref(), Some("o/r#7"));
            assert!(r.store.prs.get("o/r#7").is_none_or(|t| t.rounds.is_empty()), "no round yet");
            let next = (when..).step_by(1000).find(|&t| !r.due(&b, &on(), t).is_empty()).unwrap();
            waits.push((next - when) / 1000);
            // That due started a job: say it's still failing.
            r.busy.clear();
            when = next;
        }
        assert_eq!(waits, [30, 60, 120, 240, 480]);
        let job = r.due(&b, &on(), when).pop().unwrap();
        let said = r.told(&job, offline(), when).unwrap_err();
        assert!(!said.text.contains("tries again"), "{}", said.text);
        assert_eq!(said.again.as_deref(), Some("o/r#7"));
        let round = &r.store.prs["o/r#7"].rounds[0];
        assert_eq!((round.stage, round.parts.len(), round.note.as_deref()), (ReviewStage::Failed, 0, Some("GitHub: gh error connecting to api.github.com")));
        assert!(r.due(&b, &on(), when + 10 * QUIET_MS).is_empty(), "the next push tries again");

        let mut r = Reviewer::default();
        let job = r.due(&b, &on(), NOW).pop().unwrap();
        r.told(&job, offline(), NOW).unwrap_err();
        let job = r.due(&b, &on(), NOW + RETRY_MS).pop().unwrap();
        r.told(&job, Went::Started(vec![started(AUTO, S, NOW + RETRY_MS)]), NOW + RETRY_MS).unwrap();
        assert!(r.post_failures.is_empty());
    }

    /// Try again on a round whose agent wrote its review but couldn't post it posts that review,
    /// reviews off or on, rather than have it written again; a round with no review, or of a
    /// commit the PR has moved on from, is left for a review instead.
    #[test]
    fn try_again_posts_a_review_that_wasnt_posted() {
        let mut failed = Round::new(HEAD, ReviewStage::Failed, NOW);
        failed.note = Some("It couldn't be posted: gh: HTTP 422".into());
        failed.parts = vec![Part { state: PartState::Done, review: Some("A.".into()), agent: Some("Claude Code".into()), ..started(AUTO, S, NOW) }];
        let with = |round: Round| {
            let mut r = Reviewer::default();
            r.store.prs.insert("o/r#7".into(), Tracked { rounds: vec![round], ..Tracked::default() });
            r
        };
        let mut r = with(failed.clone());
        assert!(!r.repost("o/r#7", NEXT), "the PR moved on");
        assert!(r.repost("o/r#7", HEAD));
        let round = &r.store.prs["o/r#7"].rounds[0];
        assert_eq!((round.stage, round.note.as_deref(), round.ended_at, round.by_hand), (ReviewStage::Reviewing, None, None, true));
        assert_eq!(kinds(&r.due(&board(vec![pr(7, HEAD)], vec![]), &ReviewsCfg::default(), NOW + 1)), ["post 7 \"A.\" by Claude Code"]);
        assert!(!with(Round { parts: Vec::new(), ..failed.clone() }).repost("o/r#7", HEAD), "nothing to post");
        assert!(!with(Round { stage: ReviewStage::Drafted, ..failed }).repost("o/r#7", HEAD), "posted already");
    }

    #[test]
    fn a_review_cut_off_partway_isnt_done_even_once_seen() {
        assert!(turn_ended(&session(Status::Stopped, NOW + 1), NOW));
        let cut = Session { cut_off: Some(NOW + 1), ..session(Status::Stopped, NOW + 1) };
        assert!(!turn_ended(&cut, NOW));
    }

    /// A reviewer whose turn was cut off partway (Codealong restarting ends the sessions it runs)
    /// is told to carry on, on its model, and read once that turn ends. The board it was cut off
    /// on, seen again, doesn't have it told twice; cut off again, even before the carry-on said
    /// how it went, it's told again, a few times at most, and then the round is given up on. It
    /// holds its place among the reviews that run at once meanwhile. A copy that goes on in its
    /// place is kept for the PR's next round.
    #[test]
    fn a_reviewer_cut_off_partway_is_told_to_carry_on() {
        let cfg = ReviewsCfg { model: "opus".into(), effort: "high".into(), ..on() };
        let cut = |at: i64| Session { live: false, cut_off: Some(at), ..session(Status::Error, at) };
        let asked = || {
            let mut r = Reviewer::default();
            let jobs = r.due(&board(vec![pr(7, HEAD)], vec![]), &cfg, NOW);
            r.told(&jobs[0], Went::Started(vec![started("auto opus at high", S, NOW)]), NOW).unwrap();
            r
        };
        // Told to carry on from a moment after it was cut off at `when`, and heard back from later.
        let carried = |r: &mut Reviewer, when: i64, to: &str| {
            let job = r.due(&board(vec![pr(7, HEAD)], vec![cut(when)]), &cfg, when + 1).pop().expect("a carry-on");
            assert_eq!(kinds(std::slice::from_ref(&job)), [format!("carry on 7 part 0 in {S} on Some(\"opus\") at Some(\"high\")")]);
            r.told(&job, Went::CarriedOn { session: Some(to.into()), pending: None, at: when + 1 }, when + 30_000).unwrap();
        };
        let mut r = asked();
        carried(&mut r, NOW + 60_000, S);
        assert!(r.due(&board(vec![pr(7, HEAD)], vec![cut(NOW + 60_000)]), &cfg, NOW + 90_001).is_empty(), "told already");
        let more = board(vec![pr(7, HEAD), pr(8, HEAD), pr(9, HEAD)], vec![cut(NOW + 60_000)]);
        assert_eq!(kinds(&r.due(&more, &cfg, NOW + 90_002)), ["review 8 1111111 since None by auto opus at high"], "its place is kept while it starts");
        let done = board(vec![pr(7, HEAD)], vec![session(Status::Unread, NOW + 120_000)]);
        assert_eq!(kinds(&r.due(&done, &cfg, NOW + 120_001)), [format!("read 7 part 0 in {S}")]);

        let mut r = asked();
        carried(&mut r, NOW + 60_000, S);
        let again = r.due(&board(vec![pr(7, HEAD)], vec![cut(NOW + 60_010)]), &cfg, NOW + 90_001);
        assert_eq!(kinds(&again), [format!("carry on 7 part 0 in {S} on Some(\"opus\") at Some(\"high\")")], "cut off before the carry-on was heard from");

        let mut r = asked();
        let jobs = r.due(&board(vec![pr(7, HEAD), pr(8, HEAD), pr(9, HEAD)], vec![cut(NOW + 60_000)]), &cfg, NOW + 60_001);
        assert_eq!(kinds(&jobs), [format!("carry on 7 part 0 in {S} on Some(\"opus\") at Some(\"high\")"), "review 8 1111111 since None by auto opus at high".into()]);

        let mut r = asked();
        let mut when = NOW;
        for _ in 0..CARRY_ONS {
            when += 60_000;
            carried(&mut r, when, S);
        }
        assert_eq!(r.store.prs["o/r#7"].rounds[0].parts[0].carried_on.len(), CARRY_ONS);
        r.due(&board(vec![pr(7, HEAD)], vec![cut(when + 60_000)]), &cfg, when + 60_001);
        let round = &r.store.prs["o/r#7"].rounds[0];
        assert_eq!((round.stage, round.note.as_deref()), (ReviewStage::Failed, Some("Its turn was cut off partway again after it was told to carry on 4 times.")));

        let mut r = asked();
        carried(&mut r, NOW + 60_000, "claude:claude:copy");
        let t = &r.store.prs["o/r#7"];
        assert_eq!((t.sessions["auto opus at high"].as_str(), t.rounds[0].parts[0].session.as_deref()), ("claude:claude:copy", Some("claude:claude:copy")));

        // One that can't go on ends the round, saying why.
        let mut r = asked();
        let job = r.due(&board(vec![pr(7, HEAD)], vec![cut(NOW + 60_000)]), &cfg, NOW + 60_001).pop().unwrap();
        r.told(&job, Went::Failed("Its turn was cut off partway, and its session isn't running, and can't go on without a terminal.".into()), NOW + 60_002).unwrap();
        r.due(&board(vec![pr(7, HEAD)], vec![cut(NOW + 60_000)]), &cfg, NOW + 60_003);
        assert_eq!(r.store.prs["o/r#7"].rounds[0].stage, ReviewStage::Failed);
    }

    #[test]
    fn a_round_stuck_while_the_pr_moved_on_makes_way_and_a_gone_session_ends_it() {
        let mut r = Reviewer::default();
        let jobs = r.due(&board(vec![pr(7, HEAD)], vec![]), &on(), NOW);
        r.told(&jobs[0], Went::Started(vec![started(AUTO, S, NOW)]), NOW).unwrap();
        let stuck = board(vec![pr(7, NEXT)], vec![session(Status::Error, NOW + 1)]);
        r.due(&stuck, &on(), NOW + 2);
        assert_eq!(r.store.prs["o/r#7"].rounds[0].stage, ReviewStage::Failed);
        assert_eq!(kinds(&r.due(&stuck, &on(), NOW + 3 + QUIET_MS)), [format!("review 7 2222222 since None by auto in {S}")]);
        let mut r = Reviewer::default();
        let jobs = r.due(&board(vec![pr(7, HEAD)], vec![]), &on(), NOW);
        r.told(&jobs[0], Went::Started(vec![started(AUTO, S, NOW)]), NOW).unwrap();
        r.due(&board(vec![pr(7, HEAD)], vec![]), &on(), NOW + 1);
        assert_eq!(r.store.prs["o/r#7"].rounds[0].stage, ReviewStage::Reviewing, "not showing yet");
        r.due(&board(vec![pr(7, HEAD)], vec![]), &on(), NOW + SHOW_MS + 1);
        r.due(&board(vec![pr(7, HEAD)], vec![]), &on(), NOW + SHOW_MS + 2);
        assert_eq!(r.store.prs["o/r#7"].rounds[0].stage, ReviewStage::Failed);
    }

    #[test]
    fn reviews_are_kept_across_launches_and_forgotten_long_after() {
        let dir = fixtures::scratch("autoreview");
        let path = dir.join("reviews.json");
        let mut r = Reviewer { path: Some(path.clone()), ..Reviewer::default() };
        let jobs = r.due(&board(vec![pr(7, HEAD)], vec![]), &on(), NOW);
        r.told(&jobs[0], Went::Started(vec![started(AUTO, S, NOW)]), NOW).unwrap();
        let mut again = Reviewer { store: crate::store::load_whole(&path, WHAT), path: Some(path), ..Reviewer::default() };
        assert_eq!(again.store, r.store);
        // Under way, it's followed: nothing new starts for that commit.
        assert!(again.due(&board(vec![pr(7, HEAD)], vec![session(Status::Working, NOW + 1)]), &on(), NOW + 2).is_empty());
        // Ended, and the PR no longer waits on you: forgotten a month on.
        let b = board(vec![pr(7, HEAD)], vec![session(Status::Idle, NOW + 3)]);
        read(&mut again, &b, &on(), NOW + 4, "A.", "Claude Code");
        let job = again.due(&b, &on(), NOW + 4).pop().unwrap();
        again.told(&job, Went::Kept(None), NOW + 5).unwrap();
        again.due(&board(vec![], vec![]), &on(), NOW + 6);
        assert!(again.store.prs.contains_key("o/r#7"));
        again.due(&board(vec![], vec![]), &on(), NOW + 6 + KEEP_MS);
        assert!(again.store.prs.is_empty());
    }

    fn item(kind: &'static str, at: i64, text: &str) -> Item {
        crate::transcript::item(at, kind, text)
    }

    #[test]
    fn a_prs_written_reviews_are_listed_latest_first_with_their_comments() {
        let mut r = Reviewer::default();
        let jobs = r.due(&board(vec![pr(7, HEAD)], vec![]), &on(), NOW);
        r.told(&jobs[0], Went::Started(vec![started(AUTO, S, NOW)]), NOW).unwrap();
        assert!(written(&r.store, "o/r#7").is_empty(), "under way: nothing written yet");
        let b = board(vec![pr(7, HEAD)], vec![session(Status::Unread, NOW + 1)]);
        let review = "Two problems.\n\n```review-comments\n[{\"path\": \"src/a.rs\", \"line\": 2, \"body\": \"This panics.\"}]\n```";
        read(&mut r, &b, &on(), NOW + 2, review, "Claude Code, claude-opus-5-5");
        let job = r.due(&b, &on(), NOW + 3).pop().unwrap();
        r.told(&job, Went::Drafted("https://github.com/o/r/pull/7/files".into()), NOW + 4).unwrap();
        let got = written(&r.store, "O/R#7");
        assert_eq!(got.len(), 1);
        let w = &got[0];
        assert_eq!((w.head.as_str(), w.stage, w.at, w.session.as_deref(), w.agent.as_deref(), w.link.as_deref()), (HEAD, ReviewStage::Drafted, NOW + 4, Some(S), Some("Claude Code, claude-opus-5-5"), Some("https://github.com/o/r/pull/7/files")));
        assert_eq!((w.summary.as_str(), w.comments.clone()), ("Two problems.", vec![LineComment { path: "src/a.rs".into(), line: 2, body: "This panics.".into() }]));
        // A later round comes first; one that wrote nothing isn't listed.
        r.store.prs.get_mut("o/r#7").unwrap().rounds.push(Round::new(NEXT, ReviewStage::Failed, NOW + 5));
        let mut later = Round::new("3333333cccc", ReviewStage::Posted, NOW + 6);
        later.parts = vec![Part { state: PartState::Done, review: Some("All fixed.".into()), ..started(AUTO, S, NOW + 5) }];
        r.store.prs.get_mut("o/r#7").unwrap().rounds.push(later);
        let heads: Vec<String> = written(&r.store, "o/r#7").into_iter().map(|w| w.head).collect();
        assert_eq!(heads, ["3333333cccc", HEAD]);
        assert!(written(&r.store, "o/r#8").is_empty());
    }

    #[test]
    fn the_review_is_the_last_run_of_replies_after_it_was_asked_for() {
        let items = vec![item("you", NOW, "Review it"), item("agent", NOW + 1, "Let me read the diff."), item("tool", NOW + 2, "Read · pr.diff"), item("agent", NOW + 3, "Two problems."), item("agent", NOW + 4, "1. `a.rs:3` panics."), item("note", NOW + 5, "compacted")];
        assert_eq!(reply_after(&items, NOW, NOW).as_deref(), Some("Two problems.\n\n1. `a.rs:3` panics."));
        // Not after it was asked for: an earlier turn's.
        assert_eq!(reply_after(&items, NOW + 3_500, NOW), None);
        // Ended on a call, or a prompt.
        assert_eq!(reply_after(&items[..3], NOW, NOW), None);
        assert_eq!(reply_after(&[item("agent", NOW, "x"), item("you", NOW + 1, "again")], NOW, NOW + 1), None);
        assert_eq!(reply_after(&[], NOW, NOW), None);
    }

    #[test]
    fn a_review_its_record_keeps_no_time_for_is_as_new_as_its_prompt() {
        // As Cursor's command line records a turn: a time for the prompt alone, the minute it was
        // written in, so one asked for at 13:11:36 says 13:11.
        let at = 1_792_527_096_166;
        let minute = 1_792_527_060_000;
        let items = vec![item("you", minute, "Review it"), item("agent", 0, "Let me read the diff."), item("tool", 0, "Read · pr.diff"), item("agent", 0, "Two problems.")];
        assert_eq!(reply_after(&items, at, minute).as_deref(), Some("Two problems."));
        assert_eq!(reply_after(&items, at, minute + 60_000).as_deref(), Some("Two problems."));
        // The turn before's, while the prompt that asked for this one hasn't reached the record:
        // its prompt is from a minute before.
        assert_eq!(reply_after(&items, at, minute - 1), None);
        assert_eq!(reply_after(&items, at, at - 10 * 60_000), None);
    }

    #[test]
    fn what_the_session_is_told() {
        let files = Reading { md: "[pr-7.md](files/aa/pr-7.md)".into(), diff: "[pr-7.diff](files/bb/pr-7.diff)".into(), changes: Some("[pr-7-to-2222222.diff](files/cc/pr-7-to-2222222.diff)".into()) };
        let first = first_prompt(&pr(7, HEAD), HEAD, None, false, "gwen", &Goes::Post(ReviewPost::Draft), INSTRUCTIONS, &files);
        assert!(first.starts_with("Review pull request #7 in o/r, “Add teams” (https://github.com/o/r/pull/7), by ana: it merges teams into main, and is at commit 1111111."), "{first}");
        for part in ["[pr-7.md](files/aa/pr-7.md)", NUMBERED, "without its branch", INSTRUCTIONS, "Don't change any files, commit, push, or post on GitHub.", "left as a draft that only gwen sees", "`review-comments`", "`SKIP: `"] {
            assert!(first.contains(part), "{part}: {first}");
        }
        assert!(!first.contains("earlier review"));
        // Instructions of your own take the place of Codealong's; the rest is said either way.
        let mine = "Start two subagents, one on Opus and one on Sonnet, and combine what they find.";
        let later = first_prompt(&pr(7, NEXT), NEXT, Some(HEAD), true, "gwen", &Goes::Post(ReviewPost::None), mine, &files);
        assert!(later.contains(mine) && !later.contains(INSTRUCTIONS), "{later}");
        for part in ["has its branch checked out", "An earlier review of commit 1111111", "[pr-7-to-2222222.diff]", "for gwen to read here; nothing is posted", "Don't change any files"] {
            assert!(later.contains(part), "{part}: {later}");
        }
        assert!(!later.contains("review-comments"), "nothing posted, nothing to lay out: {later}");
        let again = again_prompt(&pr(7, NEXT), NEXT, Some(HEAD), "gwen", &Goes::Post(ReviewPost::Comment), mine, &files);
        assert!(again.starts_with("#7 “Add teams” has new commits since you reviewed 1111111: it's at 2222222 now."), "{again}");
        assert!(again.contains("What changed since 1111111 is in [pr-7-to-2222222.diff]") && again.contains("as a comment from gwen's GitHub account") && again.contains(mine), "{again}");
        let rewritten = again_prompt(&pr(7, NEXT), NEXT, Some(HEAD), "gwen", &Goes::Post(ReviewPost::Draft), INSTRUCTIONS, &Reading { changes: None, ..files });
        assert!(rewritten.contains("rewritten since then (rebased or force-pushed)"), "{rewritten}");
    }

    /// Two files: one changed in two places, one new.
    const DIFF: &str = "diff --git a/src/a.rs b/src/a.rs\nindex 1..2 100644\n--- a/src/a.rs\n+++ b/src/a.rs\n@@ -1,3 +1,4 @@\n fn a() {\n-    old();\n+    new();\n+    more();\n }\n\n@@ -10,2 +11,2 @@ fn b() {\n-    x\n+    y\n }\ndiff --git a/docs/new file.md b/docs/new file.md\nnew file mode 100644\n--- /dev/null\n+++ b/docs/new file.md\n@@ -0,0 +1,2 @@\n+# New\n++++ not a header\n";

    #[test]
    fn a_diff_is_numbered_by_each_lines_place_in_the_new_version() {
        let n = numbered(DIFF);
        let lines: Vec<&str> = n.lines().collect();
        assert_eq!(lines[4], "@@ -1,3 +1,4 @@");
        assert_eq!(&lines[5..10], ["    1  fn a() {", "      -    old();", "    2 +    new();", "    3 +    more();", "    4  }"]);
        // An empty context line keeps its number.
        assert_eq!(lines[10], "    5 ");
        assert_eq!(&lines[12..15], ["      -    x", "   11 +    y", "   12  }"]);
        assert_eq!(lines.last(), Some(&"    2 ++++ not a header"));
        let can = commentable(DIFF);
        let mut a: Vec<u64> = can["src/a.rs"].iter().copied().collect();
        a.sort();
        assert_eq!(a, [1, 2, 3, 4, 5, 11, 12]);
        assert_eq!(can["docs/new file.md"].len(), 2);
        // GitHub's compare files have no +++ line: the name comes from the diff line.
        let compare = "diff --git a/b.rs b/b.rs\n@@ -5,1 +5,2 @@\n x\n+y\n";
        assert_eq!(commentable(compare)["b.rs"], [5, 6].into_iter().collect());
    }

    #[test]
    fn the_last_message_is_a_summary_and_comments_on_lines() {
        let reply = "Two problems, one serious.\n\n```review-comments\n[{\"path\": \"src/a.rs\", \"line\": 2, \"body\": \"This panics on empty input.\"}, {\"path\": \"b/src/a.rs\", \"line\": \"11\", \"body\": \"Off by one.\"}]\n```\n\nOtherwise fine.";
        let (summary, comments) = parse_reply(reply);
        assert_eq!(summary, "Two problems, one serious.\n\nOtherwise fine.");
        assert_eq!(comments, [c("src/a.rs", 2, "This panics on empty input."), c("src/a.rs", 11, "Off by one.")]);
        // No block, or one that doesn't read: the message whole.
        assert_eq!(parse_reply("Looks right."), ("Looks right.".to_string(), vec![]));
        let broken = "Two problems.\n```review-comments\n[{\"path\": \"a.rs\"}]\n```";
        assert_eq!(parse_reply(broken), (broken.to_string(), vec![]));
        assert_eq!(skipped("SKIP: the diff is a vendored library"), Some("the diff is a vendored library".into()));
        assert_eq!(skipped("skip:"), Some("Its agent couldn't review it.".into()));
        assert_eq!(skipped("Skipping the tests, the rest is fine."), None);
    }

    fn c(path: &str, line: u64, body: &str) -> LineComment {
        LineComment { path: path.into(), line, body: body.into() }
    }

    #[test]
    fn a_draft_review_puts_each_comment_on_its_line_where_github_takes_it() {
        let lines = commentable(DIFF);
        let comments = vec![c("src/a.rs", 2, "This panics."), c("src/a.rs", 7, "Not in the diff."), c("docs/new file.md", 1, "Typo.")];
        let plan = draft_plan("Two problems.", comments.clone(), &lines, None, &pr(7, HEAD), HEAD, "Claude Code", NOW);
        assert_eq!(plan.add_to, None);
        assert_eq!(plan.threads, [c("src/a.rs", 2, "This panics."), c("docs/new file.md", 1, "Typo.")]);
        assert!(plan.body.starts_with("Two problems.\n\n**On other lines**\n\n**`src/a.rs` line 7**\nhttps://github.com/o/r/blob/1111111aaaa/src/a.rs#L7\n\nNot in the diff.\n\n<sub>An agent's review (Claude Code) of 1111111, drafted by Codealong.</sub>"), "{}", plan.body);
        assert!(plan.body.ends_with(&draft_mark("o/r#7", HEAD, NOW)));
        // Your draft at the same commit: added to, after what you wrote.
        let mine = PendingReview { id: "R_1".into(), body: "My notes.".into(), commit: Some(HEAD.into()) };
        let plan = draft_plan("Two problems.", comments.clone(), &lines, Some(&mine), &pr(7, HEAD), HEAD, "Claude Code", NOW);
        assert_eq!((plan.add_to.as_deref(), plan.threads.len()), (Some("R_1"), 2));
        assert!(plan.body.starts_with("My notes.\n\n---\n\nTwo problems."), "{}", plan.body);
        // Begun on another commit: every comment in its text, linked at the reviewed commit.
        let older = PendingReview { commit: Some(NEXT.into()), body: String::new(), ..mine };
        let plan = draft_plan("Two problems.", comments, &lines, Some(&older), &pr(7, HEAD), HEAD, "Claude Code", NOW);
        assert!(plan.threads.is_empty());
        assert!(plan.body.starts_with("Two problems.\n\n**On other lines**") && plan.body.contains("docs/new%20file.md#L1"), "{}", plan.body);
    }

    #[test]
    fn a_comment_has_its_line_comments_written_in_under_links() {
        let body = comment_body("  Looks right.\n", &[], &pr(7, HEAD), HEAD, "Claude Code, claude-opus-5-5");
        assert_eq!(body, "Looks right.\n\n<sub>An agent's review (Claude Code, claude-opus-5-5) of 1111111, posted by Codealong.</sub>\n<!-- codealong-review o/r#7 1111111aaaa -->");
        let body = comment_body("One thing.", &[c("src/a.rs", 2, "This panics.")], &pr(7, HEAD), HEAD, "Codex");
        assert!(body.starts_with("One thing.\n\n**`src/a.rs` line 2**\nhttps://github.com/o/r/blob/1111111aaaa/src/a.rs#L2\n\nThis panics.\n\n<sub>"), "{body}");
        let long = comment_body(&"x".repeat(COMMENT_MAX + 10), &[], &pr(7, HEAD), HEAD, "Codex");
        assert!(long.len() < 65_536 && long.contains("cut short"));
    }

    #[test]
    fn a_review_of_your_pr_goes_under_the_session_it_goes_back_to() {
        let review = |key: &str, pr: &str| Session { helper: Some(crate::model::Helper { pr: Some(pr.into()), ..Default::default() }), ..fixtures::session(key) };
        let lanes_with = |opener: Session| vec![fixtures::lane("/r", vec![opener, review("codex:codex:own", "o/r#7"), review("codex:codex:theirs", "o/r#8")])];
        let prs = [PrItem { mine: true, primary: Some("claude:claude:opener".into()), ..pr(7, HEAD) }, pr(8, HEAD)];
        let parent = |lanes: &[Lane], k: &str| lanes[0].sessions.iter().find(|s| s.key == k).and_then(|s| s.helper.as_ref()?.parent.clone());
        let mut lanes = lanes_with(fixtures::session("claude:claude:opener"));
        nest_own(&mut lanes, &prs);
        assert_eq!(parent(&lanes, "codex:codex:own").as_deref(), Some("claude:claude:opener"));
        // A PR that waits on your review has no session of yours to go under.
        assert_eq!(parent(&lanes, "codex:codex:theirs"), None);
        // Nor one whose session is done with.
        let mut lanes = lanes_with(Session { archived: true, ..fixtures::session("claude:claude:opener") });
        nest_own(&mut lanes, &prs);
        assert_eq!(parent(&lanes, "codex:codex:own"), None);
    }

    #[test]
    fn every_session_a_review_asked_is_a_helper_of_its_pr() {
        let mut round = Round::new(HEAD, ReviewStage::Posted, NOW);
        // The first round's session, gone on in a copy the next round asked.
        round.parts = vec![started(AUTO, "claude:claude:first", NOW)];
        let mut next = Round::new(NEXT, ReviewStage::Reviewing, NOW + 1);
        next.parts = vec![started(AUTO, S, NOW + 1), Part::failed(AUTO, "no account".into(), NOW + 1)];
        let tracked = Tracked { sessions: [(AUTO.to_string(), S.to_string())].into(), rounds: vec![round, next] };
        let store = Store { prs: [("o/r#7".to_string(), tracked), ("o/r#8".to_string(), Tracked::default())].into() };
        let got = reviewers(&store, &[]);
        assert_eq!(got.len(), 2);
        assert_eq!((got.get("claude:claude:first"), got.get(S)), (Some(&"o/r#7"), Some(&"o/r#7")));

        // A session no round holds whose first prompt is a review of a PR here, started since its
        // first round: one that showed after its round gave up on it, before a later round.
        let files = Reading { md: "m".into(), diff: "d".into(), changes: None };
        let asked = |pr: &PrItem| crate::util::one_line(&first_prompt(pr, HEAD, None, false, "gwen", &Goes::Post(ReviewPost::Draft), INSTRUCTIONS, &files), 140);
        let stray = |key: &str, first: &str, at: i64| Session { started_at: Some(at), first_prompt: Some(first.into()), ..fixtures::session(key) };
        let sessions = [
            stray("antigravity:antigravity:late", &asked(&pr(7, HEAD)), NOW + 5),
            // Case aside, as reviews.json keeps a PR's key.
            stray("codex:codex:cased", &asked(&PrItem { key: "O/R#7".into(), repo_slug: "O/R".into(), ..pr(7, HEAD) }), NOW + 5),
            // Not one from before its reviews began, nor one of a PR with no round here or none
            // here at all, nor another lane's copy of one shown as a visitor.
            stray("codex:codex:before", &asked(&pr(7, HEAD)), NOW - 2 * 60_000),
            stray("codex:codex:eight", &asked(&pr(8, HEAD)), NOW + 5),
            stray("codex:codex:seventy", &asked(&pr(70, HEAD)), NOW + 5),
            Session { visitor: Some("worked here".into()), ..stray("codex:codex:far", &asked(&pr(7, HEAD)), NOW + 5) },
            // Nor a session of yours about the PR, its link in brackets as a review's is.
            stray("claude:claude:mine", "Fix what the review of #7 (https://github.com/o/r/pull/7) raised", NOW + 5),
            // Nor what a round asked, which stays where the round has it.
            stray(S, &asked(&pr(8, HEAD)), NOW + 5),
        ];
        let got = reviewers(&store, &sessions);
        let of = |k: &str| got.get(k).copied();
        assert_eq!((of("antigravity:antigravity:late"), of("codex:codex:cased"), of(S)), (Some("o/r#7"), Some("o/r#7"), Some("o/r#7")));
        assert_eq!([of("codex:codex:before"), of("codex:codex:eight"), of("codex:codex:seventy"), of("codex:codex:far"), of("claude:claude:mine")], [None; 5]);

        // An opening cut short in a long repo name is the one PR here it fits, and none when two
        // repos here begin alike past what's kept.
        let long = |end: &str| format!("{}/{}{end}", "o".repeat(39), "r".repeat(100));
        let cut = asked(&PrItem { repo_slug: long("a"), ..pr(7, HEAD) });
        assert!(cut.ends_with('…'), "{cut}");
        let tracked = || Tracked { sessions: BTreeMap::new(), rounds: vec![Round::new(HEAD, ReviewStage::Failed, NOW)] };
        let store_of = |ends: &[&str]| Store { prs: ends.iter().map(|end| (format!("{}#7", long(end)), tracked())).collect() };
        let late = [stray("codex:codex:cut", &cut, NOW + 5)];
        assert_eq!(reviewers(&store_of(&["a"]), &late).get("codex:codex:cut").copied(), Some(format!("{}#7", long("a")).as_str()));
        assert!(reviewers(&store_of(&["a", "b"]), &late).is_empty());
    }

    #[test]
    fn reviews_open_no_terminal_unless_settings_say_so() {
        use Place::{Background as Bg, CodexApp, Codealong as Ca, Terminal as T};
        // Claude Code (a background service of its own), Codex, and Antigravity (neither a service
        // nor Codealong's adapter), each with the places it offers.
        let (claude, codex, agy) = ((true, vec![T, Bg, Ca]), (false, vec![CodexApp, T, Bg, Ca]), (false, vec![T, Bg]));
        let at = |start: ReviewStart| [&claude, &codex, &agy].map(|(service, can)| first_of(start, *service, can));
        assert_eq!(ReviewStart::default(), ReviewStart::NoTerminal);
        // Background for Codex and Antigravity is a terminal behind your windows: kept out.
        assert_eq!(at(ReviewStart::NoTerminal), [Some(Bg), Some(Ca), None]);
        assert_eq!(at(ReviewStart::Codealong), [Some(Ca), Some(Ca), None]);
        assert_eq!(at(ReviewStart::Background), [Some(Bg), Some(Ca), Some(Bg)]);
        assert_eq!(at(ReviewStart::Terminal), [Some(T), Some(T), Some(T)]);
        // A session that isn't running goes on only where it may: Codex in Codealong, never in a
        // terminal behind your windows, which is where it went on before.
        assert_eq!(first_of(ReviewStart::NoTerminal, false, &[T, Bg, CodexApp, Ca]), Some(Ca));
        assert_eq!(first_of(ReviewStart::NoTerminal, false, &[T, Bg]), None);
    }
}
