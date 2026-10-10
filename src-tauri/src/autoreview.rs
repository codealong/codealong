//! Reviewing the pull requests that wait on your review without being asked, when config.toml
//! says to (`[reviews] auto`, Settings › Automation). Every board the engine publishes is looked
//! at for open PRs that ask for your review, aren't yours and aren't drafts. Each is reviewed in a
//! session of its own for that PR, on `[reviews] account` and `model` (else the account Auto would
//! start, on its own model), out of sight, started in the worktree that holds the PR's branch,
//! else the repo's main checkout, as Review with… starts one. It's handed the PR as Markdown and
//! its whole diff, written by Codealong (`github::for_reading`), since an agent asking `gh` itself
//! would stop on a permission prompt; the diff has each line's number in the file's new version
//! beside it (`numbered`), so a comment can name its line. It's told what to do by
//! `[reviews] prompt`, else Codealong's own `INSTRUCTIONS`: a prompt of yours may have it start
//! subagents of its own, on other models, and combine what they find. Either way it's told which
//! PR, where it's written out, to change nothing and post nothing, and how to lay out its last
//! message, and it's linked to the PR (pr-links.json), so the PR's card lists it.
//!
//! Its review is its session's last message once its turn ends: a summary, then its comments on
//! particular lines in a `review-comments` block (`parse_reply`). It's posted where `[reviews]
//! post` says: by default a review on your GitHub account left as a draft, which only you see,
//! its comments on their lines where the PR's diff shows them (`draft_plan`), for you to finish and
//! submit; or a comment on the PR, each line comment under a link to its line; or nowhere. Never a
//! submitted review, which would settle the request to you. What's posted says an agent wrote it
//! and which commit it read, with a hidden mark (`marker`) so a review already there for that
//! commit, from another Mac or before a restart, isn't posted twice. An agent that can't review
//! it says `SKIP:` and why.
//!
//! When new commits come in while the PR still waits on your review, the same session reviews it
//! again, once the branch has held still a few minutes (an author's pushes come in runs): it's
//! sent the PR as it is now and what changed since the commit it last reviewed, and asked what of
//! its findings is fixed; a session that isn't running goes on in the background, and one that's
//! gone is replaced by a new one. A review whose PR moved on before it was done isn't posted: the
//! next round covers it.
//!
//! A few rounds run at once at most; one whose session waits on you doesn't hold a place. A round
//! that didn't get done isn't tried again for the same commit; the next push tries again. Until a
//! PR's round is done the PR is out of what waits on you (`apply`, `AutoReview::holds`). Where
//! each PR stands is kept in reviews.json next to the config (`Store`), which this keeps in memory
//! and saves whole, and the engine reads for the PRs' cards (`apply`). A round keeps its reviewer's
//! turn as a part (`Part`), as it did when several could review one PR.
//!
//! The pull requests your own sessions open are reviewed the same way when `[reviews.own] auto`
//! says to (`wants_own`): each open PR of yours whose session (`PrItem::primary`, the one that
//! opened it) is on this Mac, on `[reviews.own]`'s account and model, else the account Auto would
//! start among those of another harness than that session's (`Seat::avoid`), so another model
//! reads what one wrote. Nothing goes on GitHub: the review is kept here (`ReviewStage::Kept`)
//! and, with `[reviews.own] send`, handed back to the PR's session once that runs here and doesn't
//! wait on you (`Job::Send`), as a file it's told to read (`review_file`, `back_message`), so the
//! other agent's words never reach it typed in as yours. A review that found nothing to raise
//! (`clean`) isn't sent, nor one of a commit the PR has moved on from: the next round's is. The
//! PR's Review tab sends one by hand (`send_now`).
//!
//! Every review ends on what its agent would do with the PR, a `Verdict:` line (`verdict_of`):
//! approve, ask for changes, or comment, which the PR's windows suggest beside its verdicts. With
//! `[reviews] auto_approve`, a PR that waits on your review whose agent would approve it is
//! approved as you once its review is posted (`Job::Approve`), your draft review submitted with it,
//! unless that draft holds words of yours; it's held out of what waits on you until then.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::config::{support_dir, Config, ReviewPost, ReviewsCfg};
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
/// How long a session started without a key is looked for, and a known one waited for to show.
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
pub const INSTRUCTIONS: &str = "Look for bugs, security and data risks, missing tests, and anything that doesn't do what the PR says it does. Leave out praise and a summary of the PR, and if you find nothing worth raising, say so in a sentence.";

/// What the reviewer is told to do: `[reviews] prompt`, else `INSTRUCTIONS`.
pub fn instructions(cfg: &ReviewsCfg) -> &str {
    Some(cfg.prompt.trim()).filter(|p| !p.is_empty()).unwrap_or(INSTRUCTIONS)
}

/// What the reviewer of `pr` is told to do: `[reviews.own] prompt` for a PR of yours, else
/// `instructions`.
fn instructions_for<'a>(cfg: &'a ReviewsCfg, pr: &PrItem) -> &'a str {
    if !pr.mine {
        return instructions(cfg);
    }
    Some(cfg.own.prompt.trim()).filter(|p| !p.is_empty()).unwrap_or(INSTRUCTIONS)
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
}

impl Part {
    pub fn asked(who: &str, session: Option<String>, pending: Option<Pending>, at: i64) -> Part {
        Part { who: who.to_string(), session, pending, at, state: PartState::Reviewing, review: None, agent: None, note: None }
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
}

fn fail(p: &mut Part, why: &str) {
    p.state = PartState::Failed;
    p.note = Some(why.to_string());
}

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
    /// When the PR was approved as you for this round's review (`[reviews] auto_approve`), or why
    /// it wasn't.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approved: Option<Approval>,
}

impl Round {
    fn new(head: &str, stage: ReviewStage, at: i64) -> Round {
        let ended_at = (stage != ReviewStage::Reviewing).then_some(at);
        Round { head: head.to_string(), stage, at, ended_at, comment: None, note: None, parts: Vec::new(), sent: None, approved: None }
    }
}

/// A PR approved as you for its round's review (`Job::Approve`), or not, and why.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Approval {
    pub at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failed: Option<String>,
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
    /// The commit its last review that got done was of, for what changed since.
    fn done_head(&self) -> Option<&str> {
        self.rounds.iter().rev().find(|r| matches!(r.stage, ReviewStage::Posted | ReviewStage::Drafted | ReviewStage::Kept)).map(|r| r.head.as_str())
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
    pub session: Option<String>,
    /// The harness Auto keeps off: the one of the session that opened a PR of yours, so another
    /// model reviews it. Auto takes one of it only when no account of another can start.
    pub avoid: Option<Harness>,
}

/// The reviewer `cfg` names, with its session for the PR in `t`.
fn seat(cfg: &ReviewsCfg, t: Option<&Tracked>) -> Seat {
    named_seat(&cfg.account, &cfg.model, t)
}

fn named_seat(account: &str, model: &str, t: Option<&Tracked>) -> Seat {
    let some = |s: &str| Some(s.trim()).filter(|s| !s.is_empty()).map(String::from);
    let (account, model) = (some(account), some(model));
    let who = [account.as_deref().unwrap_or(AUTO), model.as_deref().unwrap_or("")].join(" ").trim().to_string();
    Seat { session: t.and_then(|t| t.sessions.get(&who).cloned()), who, account, model, avoid: None }
}

/// The reviewer of `pr`: `[reviews.own]`'s for a PR of yours, whose Auto keeps off the harness of
/// the session that opened it, else `seat`.
fn seat_for(cfg: &ReviewsCfg, t: Option<&Tracked>, lanes: &[Lane], pr: &PrItem) -> Seat {
    if !pr.mine {
        return seat(cfg, t);
    }
    let mut s = named_seat(&cfg.own.account, &cfg.own.model, t);
    if s.account.is_none() {
        s.avoid = origin(lanes, pr).map(|(_, o)| o.harness);
    }
    s
}

/// What's due.
#[derive(Debug, Clone, PartialEq)]
pub enum Job {
    /// Start a round at `head`, asking `seat`. `since`: the commit the PR's last review that got
    /// done was of.
    Review { pr: PrItem, head: String, since: Option<String>, seat: Seat },
    /// The round's part `part` ended its turn in `session`: read the review it wrote after `at`.
    Read { pr: PrItem, head: String, part: usize, session: String, at: i64 },
    /// Post the round's review, written by `agent`.
    Post { pr: PrItem, head: String, review: String, agent: String },
    /// Hand a PR of yours' review of commit `head`, written by `agent`, to `session`, the one
    /// that opened it.
    Send { pr: PrItem, head: String, session: String, review: String, agent: String },
    /// Approve a PR that waits on your review as you, its round's review of `head` saying to.
    Approve { pr: PrItem, head: String },
}

impl Job {
    fn pr(&self) -> &PrItem {
        match self {
            Job::Review { pr, .. } | Job::Read { pr, .. } | Job::Post { pr, .. } | Job::Send { pr, .. } | Job::Approve { pr, .. } => pr,
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
    /// It didn't get done, and why.
    Failed(String),
    /// Posting didn't go through, and why: it's tried again a few times.
    PostFailed(String),
    /// Its review was sent to this session.
    Sent(String),
    /// The PR was approved as you.
    Approved,
    /// It wasn't approved, and won't be for this round: why.
    NotApproved(String),
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
    /// Posts that failed in a row, by key.
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

/// Whether Codealong reviews a PR of yours (`[reviews.own]`): it's open and not a draft, and the
/// session that opened it is here to hand the review to.
pub fn wants_own(lanes: &[Lane], pr: &PrItem) -> bool {
    pr.mine && pr.state.is_open() && pr.state != PrState::Draft && pr.head_oid.is_some() && origin(lanes, pr).is_some()
}

/// Whether `cfg` has Codealong review a PR: one that waits on your review while `[reviews] auto`
/// is on, one of yours while `[reviews.own] auto` is.
fn wanted(cfg: &ReviewsCfg, lanes: &[Lane], pr: &PrItem) -> bool {
    if pr.mine {
        cfg.own.auto && wants_own(lanes, pr)
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

/// Whether the PR is to be approved as you for round `r`, its review of `head`: auto-approve is
/// on, the PR still waits on your review at that commit, the round's review was posted where
/// config.toml says (or kept), its agent would approve it, and it hasn't been tried.
fn approve_due(cfg: &ReviewsCfg, pr: &PrItem, r: &Round, head: &str) -> bool {
    cfg.auto && cfg.auto_approve && wants(pr) && r.head == head && r.approved.is_none() && matches!(r.stage, ReviewStage::Drafted | ReviewStage::Posted | ReviewStage::Kept) && round_verdict(r) == Some(Verdict::Approve)
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
/// something. One stopped by a usage limit or an error, or waiting on you to allow something,
/// isn't done.
fn turn_ended(s: &Session, at: i64) -> bool {
    let ended = match s.status {
        Status::Unread | Status::Idle | Status::Stopped => true,
        Status::Needs => s.asks,
        Status::Working | Status::Error => false,
    };
    ended && s.last_activity >= at && s.limit.is_none() && s.connection_lost.is_none()
}

/// The session a start that named none made: the first of its account in its worktree started
/// since, whose first prompt has the PR's link.
fn found(board: &Board, p: &Pending, url: &str) -> Option<String> {
    let lane = board.lanes.iter().find(|l| l.id == p.lane && l.device.is_none())?;
    lane.sessions
        .iter()
        .filter(|s| s.visitor.is_none() && s.account == p.account && s.started_at.unwrap_or(s.last_activity) >= p.at - 60_000 && s.first_prompt.as_deref().is_some_and(|f| f.contains(url)))
        .min_by_key(|s| s.started_at.unwrap_or(s.last_activity))
        .map(|s| s.key.clone())
}

/// What's next for a round under way on PR `pr`, which is at `head` now. The reviewer's turn is
/// read once its session ended it, and fails once its session is gone a while; once it's settled,
/// its review is posted, or the round ends without one. A round stuck while the PR moved on ends,
/// for the next.
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
        return Next::Job(Job::Post { pr: pr.clone(), head: r.head.clone(), review: p.review.clone().unwrap_or_default(), agent: p.label() });
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
        if !(cfg.auto || cfg.own.auto) || board.github_login.is_none() {
            return Vec::new();
        }
        let mut changed = false;
        let listed: HashMap<String, &PrItem> = board.prs.iter().map(|p| (pr_key(&p.key), p)).collect();
        // Sessions from starts that didn't name them, once they show; one that never does fails
        // its part.
        for (key, t) in self.store.prs.iter_mut() {
            let url = listed.get(key).map(|pr| pr.url.as_str()).unwrap_or_default();
            let Some(r) = t.rounds.last_mut().filter(|r| r.stage == ReviewStage::Reviewing) else { continue };
            for p in r.parts.iter_mut().filter(|p| p.reviewing()) {
                let Some(pending) = p.pending.clone() else { continue };
                if let Some(s) = found(board, &pending, url).filter(|_| !url.is_empty()) {
                    t.sessions.insert(p.who.clone(), s.clone());
                    p.session = Some(s);
                    p.pending = None;
                    changed = true;
                } else if now - pending.at > SHOW_MS {
                    p.pending = None;
                    fail(p, "Its session never showed up.");
                    changed = true;
                }
            }
        }
        // PRs that no longer wait on you are forgotten a while after their last round.
        let before = self.store.prs.len();
        self.store.prs.retain(|k, t| listed.get(k).is_some_and(|p| wanted(cfg, &board.lanes, p)) || t.rounds.last().is_some_and(|r| now - r.ended_at.unwrap_or(r.at) < KEEP_MS) || self.busy.contains(k));
        changed |= self.store.prs.len() != before;
        // A round holds a place while one of its sessions works, or is starting.
        let holds_place = |r: &Round| {
            r.stage == ReviewStage::Reviewing && r.parts.iter().any(|p| p.reviewing() && match &p.session { Some(k) => board.session(k).is_none_or(|(_, s)| s.status == Status::Working), None => p.pending.is_some() })
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
            let want = wanted(cfg, &board.lanes, pr);
            if let Some(r) = self.store.prs.get_mut(&key).and_then(|t| t.rounds.last_mut()).filter(|r| r.stage == ReviewStage::Reviewing) {
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
            if self.store.prs.get(&key).and_then(|t| t.rounds.last()).is_some_and(|r| approve_due(cfg, pr, r, &head)) {
                self.busy.insert(key);
                jobs.push(Job::Approve { pr: pr.clone(), head });
                continue;
            }
            if !want {
                continue;
            }
            let t = self.store.prs.get(&key);
            if t.is_some_and(|t| t.rounds.iter().any(|r| r.head == head)) {
                continue;
            }
            let again = t.is_some_and(|t| !t.rounds.is_empty());
            if (again && now - held_since < QUIET_MS) || running >= AT_ONCE {
                continue;
            }
            running += 1;
            self.busy.insert(key);
            jobs.push(Job::Review { pr: pr.clone(), head, since: t.and_then(|t| t.done_head()).map(String::from), seat: seat_for(cfg, t, &board.lanes, pr) });
        }
        if changed {
            self.save();
        }
        jobs
    }

    /// The send due for a PR of yours at `head` (`to_send`): to the session that opened it, once
    /// that runs here and doesn't wait on you, while sending is on and the PR is open.
    fn send_due(&self, board: &Board, cfg: &ReviewsCfg, pr: &PrItem, head: &str) -> Option<Job> {
        if !(pr.mine && cfg.own.auto && cfg.own.send && pr.state.is_open()) {
            return None;
        }
        let (review, agent) = to_send(self.store.prs.get(&pr_key(&pr.key))?.rounds.last()?, head)?;
        let (_, s) = origin(&board.lanes, pr).filter(|(_, s)| can_take(s))?;
        Some(Job::Send { pr: pr.clone(), head: head.to_string(), session: s.key.clone(), review, agent })
    }

    /// How a job went, at `now` (ms): kept, and saved. What should be said as a notice, for a
    /// review that couldn't be started or posted; one that didn't get done for its own reasons is
    /// said on its PR's card.
    pub fn told(&mut self, job: &Job, went: Went, now: i64) -> Result<(), String> {
        let pr = job.pr();
        let key = pr_key(&pr.key);
        self.busy.remove(&key);
        if went == Went::Later {
            return Ok(());
        }
        let t = self.store.prs.entry(key.clone()).or_default();
        let mut said = Ok(());
        match (job, went) {
            (Job::Review { head, .. }, Went::Started(parts)) => {
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
                    said = Err(format!("Codealong couldn't start a review of {}: {}", pr.key, failed.join("; ")));
                }
                r.parts = parts;
                t.rounds.push(r);
            }
            (Job::Review { head, .. }, Went::NotStarted(why)) => {
                let mut r = Round::new(head, ReviewStage::Failed, now);
                r.note = Some(why.clone());
                t.rounds.push(r);
                said = Err(format!("Codealong couldn't start a review of {}: {why}", pr.key));
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
                    return Err(format!("Codealong couldn't hand its review of {} to the session that opened it: {why} It tries again in a minute or two.", pr.key));
                }
                self.post_failures.remove(&key);
                if let Some(r) = t.rounds.iter_mut().rev().find(|r| r.head == *head) {
                    r.sent = Some(Sent { session: session.clone(), at: now, failed: Some(why.clone()) });
                }
                said = Err(format!("Codealong couldn't hand its review of {} to the session that opened it: {why}", pr.key));
            }
            (Job::Send { .. }, _) => {}
            (Job::Approve { head, .. }, Went::Approved) => {
                self.post_failures.remove(&key);
                if let Some(r) = t.rounds.iter_mut().rev().find(|r| r.head == *head) {
                    r.approved = Some(Approval { at: now, failed: None });
                }
            }
            (Job::Approve { head, .. }, Went::NotApproved(why)) => {
                if let Some(r) = t.rounds.iter_mut().rev().find(|r| r.head == *head) {
                    r.approved = Some(Approval { at: now, failed: Some(why) });
                }
            }
            (Job::Approve { head, .. }, Went::PostFailed(why)) => {
                let failures = self.post_failures.entry(key.clone()).or_default();
                *failures += 1;
                if *failures < POST_TRIES {
                    self.next_at.insert(key, now + (RETRY_MS << (*failures - 1)));
                    return Err(format!("Codealong couldn't approve {} for you: {why} It tries again in a minute or two.", pr.key));
                }
                self.post_failures.remove(&key);
                if let Some(r) = t.rounds.iter_mut().rev().find(|r| r.head == *head) {
                    r.approved = Some(Approval { at: now, failed: Some(why.clone()) });
                }
                said = Err(format!("Codealong couldn't approve {} for you: {why}", pr.key));
            }
            (Job::Approve { .. }, _) => {}
            (Job::Post { .. }, Went::NotYet) => {
                self.next_at.insert(key, now + RETRY_MS);
                return Ok(());
            }
            (Job::Post { .. }, Went::PostFailed(why)) => {
                let failures = self.post_failures.entry(key.clone()).or_default();
                *failures += 1;
                if *failures < POST_TRIES {
                    self.next_at.insert(key, now + (RETRY_MS << (*failures - 1)));
                    return Err(format!("Codealong couldn't post its review of {}: {why} It tries again in a minute or two.", pr.key));
                }
                self.post_failures.remove(&key);
                if let Some(r) = t.rounds.last_mut().filter(|r| r.stage == ReviewStage::Reviewing) {
                    end(r, ReviewStage::Failed, Some(format!("It couldn't be posted: {why}")), now);
                }
                said = Err(format!("Codealong couldn't post its review of {}: {why}", pr.key));
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

/// Each PR's automatic review, for its card, from reviews.json (`Store`): its latest round, else
/// the round to come for a PR that reviews are on for (`wanted`); whether the PR has moved on
/// since that round, with another to come; and whether it's held out of what waits on you until
/// the agent's review is done. It's held while its commit has had no round yet, for `HOLD_MS` at
/// most from when your review was asked for or the PR last changed, and while its commit's round
/// is under way, for as long from when that began; a round that's done, or didn't get done, hands
/// it to you. A PR of yours is never held: it says where its review went back to, or why it
/// hasn't, and whether the session that opened it can be sent it now.
/// The PR each session reviews, by the session's key: every session a round of a PR's reviews
/// asked, and the one kept for its next round. Each is a helper of that PR (`Session::helper`),
/// out of your lists and queues while it doesn't need you, and listed on the PR's card.
pub fn reviewers(store: &Store) -> HashMap<&str, &str> {
    let mut out = HashMap::new();
    for (pr, t) in &store.prs {
        let asked = t.rounds.iter().flat_map(|r| &r.parts).filter_map(|p| p.session.as_deref());
        for key in t.sessions.values().map(String::as_str).chain(asked) {
            out.insert(key, pr.as_str());
        }
    }
    out
}

pub fn apply(prs: &mut [PrItem], store: &Store, cfg: &ReviewsCfg, lanes: &[Lane], now: i64) {
    for pr in prs {
        let wanted = wanted(cfg, lanes, pr);
        let t = store.prs.get(&pr_key(&pr.key));
        let head = pr.head_oid.clone().unwrap_or_default();
        let since = pr.requested_at.unwrap_or(0).max(pr.updated_at);
        let own = pr.mine;
        let holds = wanted
            && !own
            && match t.and_then(|t| t.rounds.iter().rev().find(|r| r.head == head)) {
                // Until it's approved as you, when it's to be.
                Some(r) => (r.stage == ReviewStage::Reviewing && now - r.at < HOLD_MS) || (approve_due(cfg, pr, r, &head) && now - r.at < HOLD_MS),
                None => now - since < HOLD_MS,
            };
        pr.auto_review = match t.and_then(|t| t.rounds.last().map(|r| (t, r))) {
            Some((t, r)) => Some(AutoReview {
                stage: r.stage,
                head: r.head.clone(),
                parts: r.parts.iter().map(|p| ReviewPart { who: p.who.clone(), session: p.session.clone(), state: p.state, note: p.note.clone() }).collect(),
                at: r.ended_at.unwrap_or(r.at),
                comment: r.comment.clone().or_else(|| t.rounds.iter().rev().find_map(|r| r.comment.clone())),
                note: if own { back_note(r, cfg, &head) } else { r.approved.as_ref().and_then(|a| a.failed.as_ref()).map(|why| format!("It wasn't approved for you: {why}")).or_else(|| r.note.clone()) },
                suggests: round_verdict(r),
                approved: r.approved.as_ref().filter(|a| a.failed.is_none()).map(|a| a.at),
                rounds: t.rounds.len(),
                behind: wanted && head != r.head,
                holds,
                own,
                sent: r.sent.as_ref().filter(|s| s.failed.is_none()).map(|s| s.session.clone()),
                sent_at: r.sent.as_ref().filter(|s| s.failed.is_none()).map(|s| s.at),
                sendable: (own && r.stage == ReviewStage::Kept).then(|| origin(lanes, pr)).flatten().filter(|(_, s)| can_take(s)).map(|(_, s)| s.key.clone()),
            }),
            None if wanted => Some(AutoReview { stage: ReviewStage::Queued, head, parts: Vec::new(), at: since, comment: None, note: None, rounds: 0, behind: false, holds, own, sent: None, sent_at: None, sendable: None, suggests: None, approved: None }),
            None => None,
        };
    }
}

/// Why a review of a PR of yours hasn't gone back to its session, while it hasn't: what the
/// round says (it found nothing to raise, the PR ended), else that sending is off, it was given
/// up on, the PR moved on, or it waits for its session.
fn back_note(r: &Round, cfg: &ReviewsCfg, head: &str) -> Option<String> {
    if r.note.is_some() || r.stage != ReviewStage::Kept {
        return r.note.clone();
    }
    match &r.sent {
        Some(Sent { failed: Some(why), .. }) => Some(format!("It couldn't be handed to the session that opened the PR: {why}")),
        Some(_) => None,
        None if !cfg.own.send => Some("Handing reviews back is off: send it from here.".into()),
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
        Goes::Post(_) => format!("Codealong started you to review it for {login}, whose review it waits on."),
    };
    let mut out = format!(
        "Review pull request #{} in {}, “{}” ({}){by}: it merges {} into {base}, and is at commit {}. {why}\n\n",
        pr.number,
        pr.repo_slug,
        pr.title.trim(),
        pr.url,
        pr.head,
        short(head)
    );
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
        Some(s) => format!("Codealong: #{} “{}” has new commits since you reviewed {}: it's at {} now. ", pr.number, pr.title.trim(), short(s), short(head)),
        None => format!("Codealong: review #{} “{}” again; it's at {} now. ", pr.number, pr.title.trim(), short(head)),
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
/// None while the turn hasn't written one, or ended on something else (a call, a prompt).
pub fn reply_after(items: &[Item], at: i64) -> Option<String> {
    let end = items.iter().rposition(|i| i.kind != "note")?;
    let start = items[..=end].iter().rposition(|i| i.kind != "agent").map_or(0, |i| i + 1);
    let run = &items[start..=end];
    if run.is_empty() || run.iter().any(|i| i.kind != "agent" || i.at < at) {
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
    let mut text = text.trim().to_string();
    if text.chars().count() > COMMENT_MAX {
        let cut = crate::util::truncate(&text, COMMENT_MAX);
        text = format!("{}\n\n…(cut short: the whole review is longer than GitHub takes)", cut.trim_end());
    }
    format!("{text}\n\n<sub>An agent's review ({agent}) of {}, {how} by Codealong.</sub>\n{}", short(head), marker(&pr.key, head))
}

/// The comment posted for a review: its text, then its comments on particular lines, each under a
/// link to its line.
pub fn comment_body(summary: &str, comments: &[LineComment], pr: &PrItem, head: &str, agent: &str) -> String {
    let lines = written_in(pr, head, comments);
    let text = [summary.trim(), lines.as_str()].into_iter().filter(|t| !t.is_empty()).collect::<Vec<_>>().join("\n\n");
    signed(&text, pr, head, agent, "posted")
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
pub fn draft_plan(summary: &str, comments: Vec<LineComment>, lines: &HashMap<String, HashSet<u64>>, pending: Option<&PendingReview>, pr: &PrItem, head: &str, agent: &str) -> DraftPlan {
    let same = pending.is_none_or(|p| p.commit.as_deref() == Some(head));
    let (threads, rest): (Vec<LineComment>, Vec<LineComment>) = comments.into_iter().partition(|c| same && lines.get(&c.path).is_some_and(|l| l.contains(&c.line)));
    let others = written_in(pr, head, &rest);
    let others = if others.is_empty() { others } else { format!("**On other lines**\n\n{others}") };
    let text = [summary.trim(), others.as_str()].into_iter().filter(|t| !t.is_empty()).collect::<Vec<_>>().join("\n\n");
    let mut body = signed(&text, pr, head, agent, "drafted");
    if let Some(mine) = pending.map(|p| p.body.trim()).filter(|b| !b.is_empty()) {
        body = format!("{mine}\n\n---\n\n{body}");
    }
    DraftPlan { add_to: pending.map(|p| p.id.clone()), body, threads }
}

/// Where a PR's draft review is seen: its files, beside the lines it comments on.
fn draft_link(pr: &PrItem) -> String {
    format!("{}/files", pr.url)
}

/// Puts a review into a draft on your account (`draft_plan`), reading the PR's diff and your draft
/// as they are now. A draft that has the review already (its mark) is left as it is: a post tried
/// again after adding part of it doesn't add it twice.
fn draft(pr: &PrItem, head: &str, summary: &str, comments: Vec<LineComment>, agent: &str) -> Went {
    let diff = match crate::github::pr_diff(&pr.repo_slug, pr.number) {
        Ok(d) => d,
        Err(e) => return Went::PostFailed(format!("its diff couldn't be read: {e:#}")),
    };
    let (id, pending) = match crate::github::pending_review(&pr.repo_slug, pr.number) {
        Ok(p) => p,
        Err(e) => return Went::PostFailed(format!("{e:#}")),
    };
    if pending.as_ref().is_some_and(|p| p.body.contains(&marker(&pr.key, head))) {
        return Went::Drafted(draft_link(pr));
    }
    let plan = draft_plan(summary, comments, &commentable(&diff), pending.as_ref(), pr, head, agent);
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

/// The account a new review runs on, and where: `account`, else the one Auto would start, out
/// of sight (`start::out_of_sight`), among the accounts of another harness than `avoid` when one
/// of them can.
fn reviewer_place(cfg: &Config, board: &Board, account: Option<&str>, avoid: Option<Harness>) -> Result<(String, Place), String> {
    let name = match account.map(str::trim).filter(|a| !a.is_empty()) {
        None => {
            let other = |a: &crate::config::AccountCfg| crate::harness::of(a).map(|h| h.harness()) != avoid;
            avoid
                .and_then(|_| crate::start::pick_account_where(cfg, &board.usage, &crate::start::OUT_OF_SIGHT, None, now_ms(), other))
                .or_else(|| crate::start::pick_account(cfg, &board.usage, &crate::start::OUT_OF_SIGHT, None, now_ms()))
                .ok_or("no account can start a session in the background")?
        }
        Some(n) => n.to_string(),
    };
    let a = cfg.accounts.iter().find(|a| a.name == name).ok_or_else(|| format!("there's no account named {name}"))?;
    let place = crate::start::out_of_sight(a, &crate::util::home()).ok_or_else(|| format!("{name} can't start a session in the background"))?;
    Ok((name, place))
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
/// with it in the background where its account can. The session the turn is in, which a
/// background job's copy changes, or where to look for it (`Pending`) when that isn't said.
fn hand(engine: &Handle, board: &Board, cfg: &Config, pr: &PrItem, key: &str, text: String) -> Result<(Option<String>, Option<Pending>), String> {
    let Some((lane, s)) = board.session(key) else { return Err("its session is gone".into()) };
    if s.live {
        let params = codealong_peer::api::params::Send { key: String::new(), text, now: false };
        crate::actions::local(engine, &crate::actions::From::Here, codealong_peer::api::SESSION_SEND, key, params).map_err(|e| format!("its session couldn't be sent it: {e}"))?;
        return Ok((Some(key.to_string()), None));
    }
    let resumes = cfg.accounts.iter().find(|a| a.name == s.account).is_some_and(|a| crate::start::resume_places(a, &crate::util::home()).contains(&Place::Background));
    if !resumes {
        return Err("its session isn't running, and can't go on in the background".into());
    }
    let at = now_ms();
    let how = ResumeIn { place: Place::Background, model: None, effort: None };
    let r = crate::start::resume(board, cfg, key, &how, &text, &crate::send::Sender::You).map_err(|e| format!("its session couldn't go on in the background: {e:#}"))?;
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
    if let Some(key) = seat.session.as_deref().filter(|k| board.session(k).is_some()) {
        match hand(engine, board, cfg, pr, key, again_prompt(pr, head, since, login, &goes(&cfg.reviews, &board.lanes, pr), instructions_for(&cfg.reviews, pr), files)) {
            Ok((session, pending)) => return Part::asked(&seat.who, session, pending, at),
            // A session that can't go on is replaced by a new one.
            Err(e) => eprintln!("codealong: {}'s review of {} starts afresh: {e}", seat.who, pr.key),
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

/// Starts a round: reads the PR, then asks the reviewer (`ask`).
fn review(engine: &Handle, pr: &PrItem, head: &str, since: Option<&str>, seat: &Seat) -> Went {
    let board = engine.board();
    let cfg = match crate::config::current() {
        Ok(c) => c,
        Err(e) => return Went::NotStarted(format!("{e:#}")),
    };
    let Some(pr) = board.prs.iter().find(|p| p.key == pr.key) else { return Went::Later };
    if pr.head_oid.as_deref() != Some(head) || !wanted(&cfg.reviews, &board.lanes, pr) {
        return Went::Later;
    }
    let login = board.github_login.clone().unwrap_or_else(|| "you".into());
    let (md, diff, detail) = match crate::github::for_reading(pr) {
        Ok(r) => r,
        Err(e) => return Went::NotStarted(format!("{e:#}")),
    };
    // Reviewed already, from another Mac or before a restart.
    let mark = marker(&pr.key, head);
    if let Some(c) = detail.comments.iter().find(|c| !pr.mine && c.mine && c.body.contains(&mark)) {
        return match cfg.reviews.post {
            ReviewPost::Draft => Went::Drafted(draft_link(pr)),
            _ => Went::Posted(c.url.clone().unwrap_or_else(|| pr.url.clone())),
        };
    }
    let changes = since.and_then(|s| crate::github::changes_since(&pr.repo_slug, s, head).ok().flatten());
    let files = match keep_files(pr, head, &md, &diff, changes.as_deref()) {
        Ok(f) => f,
        Err(e) => return Went::NotStarted(e),
    };
    Went::Started(vec![ask(engine, &board, &cfg, pr, head, since, &login, &files, seat)])
}

/// A new session's start for a review: on the reviewer's account (Auto's for none) and model,
/// where it starts out of sight, in the worktree `side::pr_lane` picks, on the first prompt.
#[allow(clippy::too_many_arguments)]
fn new_start(board: &Board, cfg: &Config, pr: &PrItem, head: &str, since: Option<&str>, login: &str, files: &Reading, seat: &Seat) -> Result<StartSpec, String> {
    let (account, place) = reviewer_place(cfg, board, seat.account.as_deref(), seat.avoid)?;
    let (lane, holds) = crate::side::pr_lane(board, pr).ok_or_else(|| format!("there's no checkout of {} on this Mac", pr.repo_slug))?;
    let prompt = first_prompt(pr, head, since, holds, login, &goes(&cfg.reviews, &board.lanes, pr), instructions_for(&cfg.reviews, pr), files);
    Ok(StartSpec { lane_id: lane.id.clone(), account, place, prompt, model: seat.model.clone(), effort: None, worktree: false, todos: Vec::new(), from: None, helper: false, report: false })
}

/// Reads the review a turn wrote after `at`: its session's last message once the turn ended.
fn read(engine: &Handle, session: &str, at: i64) -> Went {
    let board = engine.board();
    let cfg = match crate::config::current() {
        Ok(c) => c,
        Err(_) => return Went::NotYet,
    };
    let Some((_, s)) = board.session(session) else { return Went::Failed("Its session is gone.".into()) };
    // A record that doesn't read now is read again in a moment.
    let Ok(page) = crate::transcript::page(&board, &cfg, session, None, 60) else { return Went::NotYet };
    let Some(review) = reply_after(&page.items, at) else {
        return if s.live { Went::NotYet } else { Went::Failed("Its session stopped before it wrote a review.".into()) };
    };
    if let Some(why) = skipped(&review) {
        return Went::Skipped(why);
    }
    Went::Read { review, agent: agent_of(s) }
}

/// Posts a round's review where config.toml says, unless the PR ended or moved on before it was
/// done.
fn post(engine: &Handle, pr: &PrItem, head: &str, review: &str, agent: &str) -> Went {
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
    match cfg.reviews.post {
        ReviewPost::None => Went::Kept(None),
        ReviewPost::Draft => draft(pr, head, &summary, comments, agent),
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
    let (md, diff, _) = crate::github::for_reading(pr)?;
    let stands = pr.auto_review.clone();
    let since = stands.as_ref().filter(|a| a.head != head && a.stage != ReviewStage::Queued).map(|a| a.head.clone());
    let changes = since.as_deref().and_then(|s| crate::github::changes_since(&pr.repo_slug, s, &head).ok().flatten());
    let files = keep_files(pr, &head, &md, &diff, changes.as_deref()).map_err(anyhow::Error::msg)?;
    let login = board.github_login.clone().unwrap_or_else(|| "you".into());
    let seat = seat_for(&cfg.reviews, None, &board.lanes, pr);
    let start = match new_start(board, cfg, pr, &head, since.as_deref(), &login, &files, &seat) {
        Ok(spec) => {
            let account = cfg.accounts.iter().find(|a| a.name == spec.account);
            let lane = board.lanes.iter().find(|l| l.id == spec.lane_id);
            let start = account.zip(lane).map(|(a, l)| crate::start::plan(a, spec.place, std::path::Path::new(&l.path), &spec.prompt, &crate::start::new_session_id(), spec.model.as_deref(), None, false));
            PlannedStart { who: seat.who.clone(), spec: Some(spec), start, problem: None }
        }
        Err(e) => PlannedStart { who: seat.who.clone(), spec: None, start: None, problem: Some(e) },
    };
    let (wanted, on) = if pr.mine { (wants_own(&board.lanes, pr), cfg.reviews.own.auto) } else { (wants(pr), cfg.reviews.auto) };
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
        "Codealong: {agent} reviewed your pull request #{} “{}” at {}, as {login} has another agent review the pull requests their sessions open. Its review is in {file}{lines}. It's that agent's review, not {login}'s word: fix what holds up, and say which findings you leave and why. {}",
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

/// Approves a PR as you for a round's review of `head` (`[reviews] auto_approve`), submitting your
/// draft review with it: while it still waits on your review at that commit, and unless your draft
/// holds words of yours.
fn approve(engine: &Handle, pr: &PrItem, head: &str) -> Went {
    let board = engine.board();
    let Some(pr) = board.prs.iter().find(|p| p.key == pr.key) else { return Went::Later };
    if !wants(pr) || pr.head_oid.as_deref() != Some(head) {
        return Went::Later;
    }
    match crate::github::your_review(&pr.repo_slug, pr.number) {
        Ok(y) if y.draft.as_ref().is_some_and(|d| !codealongs_only(&d.whole)) => return Went::NotApproved("your draft review on it has words of yours, so it's yours to submit.".into()),
        Ok(_) => {}
        Err(e) => return Went::PostFailed(format!("{e:#}")),
    }
    match crate::github::review(&pr.repo_slug, pr.number, Verdict::Approve, "") {
        Ok(_) => {
            engine.approved(&pr.repo_slug, pr.number);
            Went::Approved
        }
        Err(e) => Went::PostFailed(format!("{e:#}")),
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
    let params = codealong_peer::api::params::Send { key: String::new(), text, now: false };
    match crate::actions::local(engine, &crate::actions::From::Here, codealong_peer::api::SESSION_SEND, session, params) {
        Ok(_) => Went::Sent(session.clone()),
        Err(e) => Went::PostFailed(e.to_string()),
    }
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
        Job::Review { pr, head, since, seat } => review(engine, pr, head, since.as_deref(), seat),
        Job::Read { session, at, .. } => read(engine, session, *at),
        Job::Post { pr, head, review, agent } => post(engine, pr, head, review, agent),
        Job::Send { .. } => send(engine, &job, false),
        Job::Approve { pr, head } => approve(engine, pr, head),
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
            snoozed_until: None,
            nudge: false,
            nudge_by_default: false,
            primary: None,
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
            auto_review: None,
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
                Job::Review { pr, head, since, seat } => {
                    let session = seat.session.as_deref().map(|k| format!(" in {k}")).unwrap_or_default();
                    format!("review {} {} since {:?} by {}{session}", pr.number, short(head), since.as_deref().map(short), seat.who)
                }
                Job::Read { pr, part, session, .. } => format!("read {} part {part} in {session}", pr.number),
                Job::Post { pr, review, agent, .. } => format!("post {} {review:?} by {agent}", pr.number),
                Job::Send { pr, head, session, agent, .. } => format!("send {} {} by {agent} to {session}", pr.number, short(head)),
                Job::Approve { pr, head } => format!("approve {} {}", pr.number, short(head)),
            })
            .collect()
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
        apply(&mut prs, &r.store, &on(), &[], NOW);
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
        apply(&mut prs, &r.store, &own(), &done(Status::Idle).lanes, NOW + 68_000);
        let a = prs[0].auto_review.clone().unwrap();
        assert_eq!((a.stage, a.own, a.holds, a.sent.as_deref(), a.sent_at, a.note.as_deref(), a.sendable.as_deref()), (ReviewStage::Kept, true, false, Some(MAKER), Some(NOW + 67_000), None, Some(MAKER)));
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
        apply(&mut prs, &r.store, &own(), &b.lanes, NOW);
        assert_eq!(prs[0].auto_review.as_ref().unwrap().note.as_deref(), Some("It found nothing to raise, so it isn't sent."));
        let mut quiet = own();
        quiet.own.send = false;
        let mut r = reviewed(&quiet, "Two problems.");
        assert!(r.due(&b, &quiet, NOW + 64_000).is_empty());
        let mut prs = vec![mine(7, HEAD)];
        apply(&mut prs, &r.store, &quiet, &b.lanes, NOW);
        let a = prs[0].auto_review.clone().unwrap();
        // Sent from the Review tab instead, while its session can take it.
        assert_eq!((a.note.as_deref(), a.sendable.as_deref()), (Some("Handing reviews back is off: send it from here."), Some(MAKER)));
        // A send that doesn't go is tried a few times, then given up on, saying why.
        let mut r = reviewed(&own(), "Two problems.");
        for i in 0..POST_TRIES {
            let at = NOW + 64_000 + i64::from(i) * 10 * RETRY_MS;
            let job = r.due(&b, &own(), at).pop().expect("a send");
            let said = r.told(&job, Went::PostFailed("the terminal is gone".into()), at);
            assert!(said.unwrap_err().contains("couldn't hand its review of o/r#7"));
        }
        assert!(r.due(&b, &own(), NOW + 100 * RETRY_MS).is_empty());
        let mut prs = vec![mine(7, HEAD)];
        apply(&mut prs, &r.store, &own(), &b.lanes, NOW);
        assert_eq!(prs[0].auto_review.as_ref().unwrap().note.as_deref(), Some("It couldn't be handed to the session that opened the PR: the terminal is gone"));
    }

    #[test]
    fn only_prs_of_yours_a_session_here_opened_and_each_kind_only_while_its_switch_is_on() {
        let maker = fixtures::session(MAKER);
        let lanes = vec![fixtures::lane("/r", vec![maker.clone()])];
        assert!(wants_own(&lanes, &mine(7, HEAD)));
        assert!(!wants_own(&lanes, &PrItem { primary: None, ..mine(7, HEAD) }), "no session opened it");
        assert!(!wants_own(&[fixtures::lane("/r", vec![Session { archived: true, ..maker.clone() }])], &mine(7, HEAD)), "archived");
        assert!(!wants_own(&lanes, &PrItem { state: PrState::Draft, ..mine(7, HEAD) }));
        assert!(!wants_own(&lanes, &pr(8, HEAD)), "not yours");
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
        assert_eq!(said, format!("Codealong: Codex, gpt-5.5 reviewed your pull request #7 “Add teams” at 1111111, as gwen has another agent review the pull requests their sessions open. Its review is in [review-7.md](files/ab/review-7.md), with a comment on a line. It's that agent's review, not gwen's word: fix what holds up, and say which findings you leave and why. {}", crate::nudges::NOT_A_MERGE));
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
    fn a_pr_whose_agent_would_approve_it_is_approved_as_you_when_thats_on() {
        let cfg = ReviewsCfg { auto_approve: true, ..on() };
        let done = board(vec![pr(7, HEAD)], vec![session(Status::Unread, NOW + 60_000)]);
        let reviewed = |cfg: &ReviewsCfg, review: &str| {
            let mut r = Reviewer::default();
            let job = r.due(&done, cfg, NOW).pop().unwrap();
            r.told(&job, Went::Started(vec![started(AUTO, S, NOW)]), NOW).unwrap();
            read(&mut r, &done, cfg, NOW + 61_000, review, "Codex");
            let post = r.due(&done, cfg, NOW + 62_000).pop().unwrap();
            r.told(&post, Went::Drafted("https://github.com/o/r/pull/7/files".into()), NOW + 63_000).unwrap();
            r
        };
        let mut r = reviewed(&cfg, "Fine.\n\nVerdict: approve");
        // Held out of what waits on you until it's approved.
        let mut prs = vec![PrItem { requested_at: Some(NOW), ..pr(7, HEAD) }];
        apply(&mut prs, &r.store, &cfg, &[], NOW + 63_500);
        let a = prs[0].auto_review.clone().unwrap();
        assert_eq!((a.suggests, a.holds, a.approved), (Some(Verdict::Approve), true, None));
        let jobs = r.due(&done, &cfg, NOW + 64_000);
        assert_eq!(kinds(&jobs), ["approve 7 1111111"]);
        r.told(&jobs[0], Went::Approved, NOW + 65_000).unwrap();
        assert!(r.due(&done, &cfg, NOW + 66_000).is_empty(), "once");
        let mut prs = vec![PrItem { requested_at: Some(NOW), ..pr(7, HEAD) }];
        apply(&mut prs, &r.store, &cfg, &[], NOW + 66_000);
        let a = prs[0].auto_review.clone().unwrap();
        assert_eq!((a.approved, a.holds), (Some(NOW + 65_000), false));
        // Off, or an agent that wouldn't approve it: suggested, not done.
        let mut r = reviewed(&on(), "Fine.\n\nVerdict: approve");
        assert!(r.due(&done, &on(), NOW + 64_000).is_empty());
        let mut r = reviewed(&cfg, "Two problems.\n\nVerdict: request changes");
        assert!(r.due(&done, &cfg, NOW + 64_000).is_empty());
        let mut prs = vec![pr(7, HEAD)];
        apply(&mut prs, &r.store, &cfg, &[], NOW + 64_000);
        assert_eq!(prs[0].auto_review.as_ref().unwrap().suggests, Some(Verdict::RequestChanges));
        // One that isn't for you to approve now says why, and isn't tried again.
        let mut r = reviewed(&cfg, "Fine.\nVerdict: approve");
        let job = r.due(&done, &cfg, NOW + 64_000).pop().unwrap();
        r.told(&job, Went::NotApproved("your draft review on it has words of yours, so it's yours to submit.".into()), NOW + 65_000).unwrap();
        assert!(r.due(&done, &cfg, NOW + 66_000).is_empty());
        let mut prs = vec![pr(7, HEAD)];
        apply(&mut prs, &r.store, &cfg, &[], NOW + 66_000);
        let a = prs[0].auto_review.clone().unwrap();
        assert_eq!((a.approved, a.note.as_deref()), (None, Some("It wasn't approved for you: your draft review on it has words of yours, so it's yours to submit.")));
        // A draft of only Codealong's rounds is submitted; one with words of yours isn't.
        assert!(codealongs_only("Fine.\n<!-- codealong-review o/r#7 abc -->"));
        assert!(codealongs_only("Round one.\n<!-- codealong-review o/r#7 abc -->\n\n---\n\nRound two.\n<!-- codealong-review o/r#7 def -->"));
        assert!(!codealongs_only("My own notes.\n\n---\n\nFine.\n<!-- codealong-review o/r#7 abc -->"));
    }

    #[test]
    fn the_reviewer_is_an_account_and_model_and_keeps_its_session_for_the_pr() {
        let cfg = |account: &str, model: &str| ReviewsCfg { auto: true, account: account.into(), model: model.into(), ..ReviewsCfg::default() };
        let mut t = Tracked::default();
        t.sessions.insert("claude opus".into(), S.into());
        let named = seat(&cfg(" claude ", "opus"), Some(&t));
        assert_eq!(named, Seat { who: "claude opus".into(), account: Some("claude".into()), model: Some("opus".into()), session: Some(S.into()), avoid: None });
        // Another reviewer starts afresh; Auto is named for what it is.
        assert_eq!(seat(&cfg("claude", ""), Some(&t)).session, None);
        assert_eq!(seat(&cfg("", ""), None), Seat { who: AUTO.into(), account: None, model: None, session: None, avoid: None });
        assert_eq!(seat(&cfg("", "opus"), None).who, "auto opus");
        // Its instructions are config.toml's, else Codealong's.
        assert_eq!(instructions(&cfg("", "")), INSTRUCTIONS);
        assert_eq!(instructions(&ReviewsCfg { prompt: "  Start two subagents.  ".into(), ..cfg("", "") }), "Start two subagents.");
    }

    #[test]
    fn a_reviewer_that_doesnt_start_or_passes_ends_the_round_saying_why() {
        let mut r = Reviewer::default();
        let jobs = r.due(&board(vec![pr(7, HEAD)], vec![]), &on(), NOW);
        let said = r.told(&jobs[0], Went::Started(vec![Part::failed(AUTO, "codex can't start a session in the background".into(), NOW)]), NOW);
        assert!(said.unwrap_err().contains("codex can't start"));
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
            apply(&mut prs, store, &ReviewsCfg { auto: on, ..ReviewsCfg::default() }, &[], at);
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
        apply(&mut prs, &r.store, &on(), &[], NOW);
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
        // One that never shows fails its part, and so the round.
        let mut r = Reviewer::default();
        let jobs = r.due(&board(vec![pr(7, HEAD)], vec![]), &on(), NOW);
        r.told(&jobs[0], Went::Started(vec![Part::asked(AUTO, None, Some(pending), NOW)]), NOW).unwrap();
        r.due(&board(vec![pr(7, HEAD)], vec![]), &on(), NOW + SHOW_MS + 1);
        r.due(&board(vec![pr(7, HEAD)], vec![]), &on(), NOW + SHOW_MS + 2);
        let round = &r.store.prs["o/r#7"].rounds[0];
        assert_eq!((round.stage, round.note.as_deref()), (ReviewStage::Failed, Some("Its session never showed up.")));
    }

    #[test]
    fn a_failed_start_or_post_is_said_and_not_tried_again_for_that_commit() {
        let mut r = Reviewer::default();
        let b = board(vec![pr(7, HEAD)], vec![]);
        let jobs = r.due(&b, &on(), NOW);
        assert!(r.told(&jobs[0], Went::NotStarted("GitHub: Not Found".into()), NOW).unwrap_err().contains("Not Found"));
        assert!(r.due(&b, &on(), NOW + 10 * QUIET_MS).is_empty());
        let jobs = r.due(&board(vec![pr(8, HEAD)], vec![]), &on(), NOW);
        assert!(r.told(&jobs[0], Went::Started(vec![Part::failed(AUTO, "claude can't start a session in the background".into(), NOW)]), NOW).unwrap_err().contains("in the background"));
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
        assert!(r.told(&job, Went::PostFailed("gh: Not Found".into()), when).unwrap_err().contains("Not Found"));
        assert_eq!(r.store.prs["o/r#7"].rounds[0].stage, ReviewStage::Failed);
        assert!(r.due(&done, &on(), when + 10 * QUIET_MS).is_empty());
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
        assert_eq!(reply_after(&items, NOW).as_deref(), Some("Two problems.\n\n1. `a.rs:3` panics."));
        // Not after it was asked for: an earlier turn's.
        assert_eq!(reply_after(&items, NOW + 3_500), None);
        // Ended on a call, or a prompt.
        assert_eq!(reply_after(&items[..3], NOW), None);
        assert_eq!(reply_after(&[item("agent", NOW, "x"), item("you", NOW + 1, "again")], NOW), None);
        assert_eq!(reply_after(&[], NOW), None);
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
        assert!(again.starts_with("Codealong: #7 “Add teams” has new commits since you reviewed 1111111: it's at 2222222 now."), "{again}");
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
        let plan = draft_plan("Two problems.", comments.clone(), &lines, None, &pr(7, HEAD), HEAD, "Claude Code");
        assert_eq!(plan.add_to, None);
        assert_eq!(plan.threads, [c("src/a.rs", 2, "This panics."), c("docs/new file.md", 1, "Typo.")]);
        assert!(plan.body.starts_with("Two problems.\n\n**On other lines**\n\n**`src/a.rs` line 7**\nhttps://github.com/o/r/blob/1111111aaaa/src/a.rs#L7\n\nNot in the diff.\n\n<sub>An agent's review (Claude Code) of 1111111, drafted by Codealong.</sub>"), "{}", plan.body);
        assert!(plan.body.ends_with(&marker("o/r#7", HEAD)));
        // Your draft at the same commit: added to, after what you wrote.
        let mine = PendingReview { id: "R_1".into(), body: "My notes.".into(), commit: Some(HEAD.into()) };
        let plan = draft_plan("Two problems.", comments.clone(), &lines, Some(&mine), &pr(7, HEAD), HEAD, "Claude Code");
        assert_eq!((plan.add_to.as_deref(), plan.threads.len()), (Some("R_1"), 2));
        assert!(plan.body.starts_with("My notes.\n\n---\n\nTwo problems."), "{}", plan.body);
        // Begun on another commit: every comment in its text, linked at the reviewed commit.
        let older = PendingReview { commit: Some(NEXT.into()), body: String::new(), ..mine };
        let plan = draft_plan("Two problems.", comments, &lines, Some(&older), &pr(7, HEAD), HEAD, "Claude Code");
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
    fn every_session_a_review_asked_is_a_helper_of_its_pr() {
        let mut round = Round::new(HEAD, ReviewStage::Posted, NOW);
        // The first round's session, gone on in a copy the next round asked.
        round.parts = vec![started(AUTO, "claude:claude:first", NOW)];
        let mut next = Round::new(NEXT, ReviewStage::Reviewing, NOW + 1);
        next.parts = vec![started(AUTO, S, NOW + 1), Part::failed(AUTO, "no account".into(), NOW + 1)];
        let tracked = Tracked { sessions: [(AUTO.to_string(), S.to_string())].into(), rounds: vec![round, next] };
        let store = Store { prs: [("o/r#7".to_string(), tracked), ("o/r#8".to_string(), Tracked::default())].into() };
        let got = reviewers(&store);
        assert_eq!(got.len(), 2);
        assert_eq!((got.get("claude:claude:first"), got.get(S)), (Some(&"o/r#7"), Some(&"o/r#7")));
    }
}
