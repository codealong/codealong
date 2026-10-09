//! Reviewing the pull requests that wait on your review without being asked, when config.toml
//! says to (`[reviews] auto`, Settings › Accounts). Every board the engine publishes is looked
//! at for open PRs that ask for your review, aren't yours and aren't drafts. Each gets a session
//! of its own, on `[reviews] account` (else the account Auto would start) in the background,
//! started in the worktree that holds the PR's branch, else the repo's main checkout, as Review
//! with… starts one. It's handed the PR as Markdown and its whole diff, written by Codealong
//! (`github::for_reading`), since an agent asking `gh` itself would stop on a permission prompt,
//! and it's told to change nothing and post nothing. The session is linked to the PR
//! (pr-links.json), so the PR's card lists it.
//!
//! Once its turn ends, its last message is the review: Codealong posts it on the PR as a comment
//! from your GitHub account (`[reviews] comment`), with a line saying an agent wrote it and which
//! commit it read, and a hidden mark (`marker`) so a review already posted for that commit, from
//! another Mac or before a restart, isn't posted twice. A comment, not a review: a review would
//! settle the request to you, which is still yours to answer. An agent that can't review it
//! says `SKIP:` and why, and nothing is posted.
//!
//! When new commits come in while the PR still waits on your review, the same session reviews it
//! again, once the branch has held still a few minutes (an author's pushes come in runs): it's
//! sent the PR as it is now and what changed since the commit it last reviewed, and asked what of
//! its earlier findings is fixed, and what's new; a session that isn't running goes on in the
//! background, and one that's gone is replaced by a new one. A review whose PR moved on before it
//! was done isn't posted: the next round covers it.
//!
//! A few reviews run at once at most; a session waiting on you doesn't hold a place. A round
//! that didn't get done isn't tried again for the same commit; the next push tries again. Where
//! each PR stands is kept in reviews.json next to the config (`Store`), which this keeps in
//! memory and saves whole, and the engine reads for the PRs' cards (`apply`).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::config::{support_dir, Config, ReviewsCfg};
use crate::engine::Handle;
use crate::model::{AutoReview, Board, PrItem, PrState, ReviewStage, Session, Status};
use crate::prlinks::pr_key;
use crate::start::{Place, ResumeIn, StartSpec};
use crate::transcript::Item;
use crate::util::{now_ms, DAY_MS};

pub fn path() -> PathBuf {
    support_dir().join("reviews.json")
}

/// What the file holds, for the error when it doesn't read as that.
const WHAT: &str = "a list of reviews";

/// How many reviews run at once, at most.
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
/// How long a PR that waits on your review is held out of what waits on you for an agent's
/// review that isn't done (`apply`): one that takes longer, or never starts, hands it to you.
const HOLD_MS: i64 = 2 * 60 * 60_000;

/// A round: one review of one commit of a PR.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Round {
    /// The commit it reviews.
    pub head: String,
    pub stage: ReviewStage,
    /// When it was asked for: what its session writes after this is its review.
    pub at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<i64>,
    /// The comment it posted, by its link.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    /// Why it was skipped or didn't get done, or wasn't posted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
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

/// One PR's reviews.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Tracked {
    /// The session that reviews it, once known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending: Option<Pending>,
    /// Oldest first: the last is where it stands.
    #[serde(default)]
    pub rounds: Vec<Round>,
}

impl Tracked {
    /// The commit its last review that got done was of, for what changed since.
    fn done_head(&self) -> Option<&str> {
        self.rounds.iter().rev().find(|r| matches!(r.stage, ReviewStage::Posted | ReviewStage::Kept)).map(|r| r.head.as_str())
    }
}

/// reviews.json: each PR's reviews, by its key (`prlinks::pr_key`).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Store {
    #[serde(default)]
    pub prs: BTreeMap<String, Tracked>,
}

/// What's due.
#[derive(Debug, Clone, PartialEq)]
pub enum Job {
    /// Review the PR at `head`: in `session` where it has one, else in a new session. `since` is
    /// the commit its last review that got done was of.
    Review { pr: PrItem, head: String, since: Option<String>, session: Option<String> },
    /// The round asked for at `at` has ended its turn in `session`: read its review and post it.
    Post { pr: PrItem, head: String, session: String, at: i64 },
}

impl Job {
    fn pr(&self) -> &PrItem {
        match self {
            Job::Review { pr, .. } | Job::Post { pr, .. } => pr,
        }
    }
}

/// How a job went.
#[derive(Debug, Clone, PartialEq)]
pub enum Went {
    /// A round is under way, asked for at `at`: in `session`, or in the one `pending` finds.
    Started { session: Option<String>, pending: Option<Pending>, at: i64 },
    /// Nothing was started, and why.
    NotStarted(String),
    /// Nothing was started, and nothing's wrong: the PR moved on since it was due.
    Later,
    /// Its review was posted, at this link (by another Mac, or before a restart, for a review
    /// that found it there).
    Posted(String),
    /// Its review is done and left in the session: posting is off, or the PR moved on or ended
    /// before it was done (`note` says which).
    Kept(Option<String>),
    /// Its agent said it couldn't review it, and why.
    Skipped(String),
    /// Its session hasn't written the review yet.
    NotYet,
    /// The round didn't get done, and why.
    Failed(String),
    /// Posting didn't go through, and why: it's tried again a few times.
    PostFailed(String),
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

    /// The jobs due now, at `now` (ms), with reviews on as `cfg` says. A round under way whose
    /// session has ended its turn is to be posted; a PR that waits on your review at a commit no
    /// round has had is to be reviewed, at once the first time and once its branch has held still
    /// after that, while fewer than `AT_ONCE` sessions are reviewing. Nothing for a PR with a job
    /// under way, or whose wait after a try isn't over.
    pub fn due(&mut self, board: &Board, cfg: &ReviewsCfg, now: i64) -> Vec<Job> {
        if !cfg.auto || board.github_login.is_none() {
            return Vec::new();
        }
        let mut changed = false;
        let listed: HashMap<String, &PrItem> = board.prs.iter().map(|p| (pr_key(&p.key), p)).collect();
        // Sessions from starts that didn't name them, once they show; a session that never does
        // ends its round.
        for (key, t) in self.store.prs.iter_mut() {
            let Some(p) = t.pending.clone() else { continue };
            let url = listed.get(key).map(|pr| pr.url.as_str()).unwrap_or_default();
            if let Some(s) = found(board, &p, url).filter(|_| !url.is_empty()) {
                t.session = Some(s);
                t.pending = None;
                changed = true;
            } else if now - p.at > SHOW_MS {
                t.pending = None;
                if let Some(r) = t.rounds.last_mut().filter(|r| r.stage == ReviewStage::Reviewing) {
                    end(r, ReviewStage::Failed, Some("Its session never showed up.".into()), now);
                }
                changed = true;
            }
        }
        // PRs that no longer wait on you are forgotten a while after their last round.
        let before = self.store.prs.len();
        self.store.prs.retain(|k, t| listed.get(k).is_some_and(|p| wants(p)) || t.rounds.last().is_some_and(|r| now - r.ended_at.unwrap_or(r.at) < KEEP_MS) || self.busy.contains(k));
        changed |= self.store.prs.len() != before;
        let working = |t: &Tracked| match &t.session {
            Some(k) => board.session(k).is_none_or(|(_, s)| s.status == Status::Working),
            None => t.pending.is_some(),
        };
        let mut running = self.busy.len() + self.store.prs.iter().filter(|(k, t)| !self.busy.contains(*k) && t.rounds.last().is_some_and(|r| r.stage == ReviewStage::Reviewing) && working(t)).count();
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
            let t = self.store.prs.get_mut(&key);
            if let Some(t) = t.filter(|t| t.rounds.last().is_some_and(|r| r.stage == ReviewStage::Reviewing)) {
                let r = t.rounds.last_mut().expect("a round under way");
                let Some(session) = t.session.clone() else { continue };
                match board.session(&session) {
                    Some((_, s)) if turn_ended(s, r.at) => {
                        self.busy.insert(key);
                        jobs.push(Job::Post { pr: pr.clone(), head: r.head.clone(), session, at: r.at });
                    }
                    Some((_, s)) if s.status != Status::Working && head != r.head && wants(pr) => {
                        // Stuck while the PR moved on: the next round takes over.
                        end(r, ReviewStage::Failed, Some("New commits came in while it waited, so it wasn't finished.".into()), now);
                        changed = true;
                    }
                    Some(_) => {}
                    None if now - r.at > SHOW_MS => {
                        end(r, ReviewStage::Failed, Some("Its session is gone.".into()), now);
                        changed = true;
                    }
                    None => {}
                }
                continue;
            }
            if !wants(pr) {
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
            jobs.push(Job::Review { pr: pr.clone(), head, since: t.and_then(|t| t.done_head()).map(String::from), session: t.and_then(|t| t.session.clone()) });
        }
        if changed {
            self.save();
        }
        jobs
    }

    /// How a job went, at `now` (ms): kept, and saved. What should be said as a notice, for a
    /// review that couldn't be started or posted; a review that didn't get done for its own
    /// reasons is said on its PR's card.
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
            (Job::Review { head, .. }, Went::Started { session: None, pending: None, at }) => {
                t.rounds.push(Round { head: head.clone(), stage: ReviewStage::Failed, at, ended_at: Some(now), comment: None, note: Some("Its session couldn't be found.".into()) });
            }
            (Job::Review { head, .. }, Went::Started { session, pending, at }) => {
                t.session = session;
                t.pending = pending;
                t.rounds.push(Round { head: head.clone(), stage: ReviewStage::Reviewing, at, ended_at: None, comment: None, note: None });
            }
            (Job::Review { head, .. }, Went::NotStarted(why)) => {
                t.rounds.push(Round { head: head.clone(), stage: ReviewStage::Failed, at: now, ended_at: Some(now), comment: None, note: Some(why.clone()) });
                said = Err(format!("Codealong couldn't start a review of {}: {why}", pr.key));
            }
            (Job::Review { head, .. }, Went::Posted(link)) => {
                // Found posted already: nothing to do for that commit.
                t.rounds.push(Round { head: head.clone(), stage: ReviewStage::Posted, at: now, ended_at: Some(now), comment: Some(link), note: None });
            }
            (Job::Review { .. }, _) => {}
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
                    Went::Kept(note) => end(r, ReviewStage::Kept, note, now),
                    Went::Skipped(why) => end(r, ReviewStage::Skipped, Some(why), now),
                    Went::Failed(why) | Went::NotStarted(why) => end(r, ReviewStage::Failed, Some(why), now),
                    _ => {}
                }
            }
        }
        self.next_at.remove(&pr_key(&pr.key));
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
/// the round to come for a PR that waits on your review; whether the PR has moved on since that
/// round, with another to come; and whether it's held out of what waits on you until the agent's
/// review is done. All of that only while reviews are `on`, and the PR is one they're for
/// (`wants`). It's held while its commit has had no round yet, for `HOLD_MS` at most from when
/// your review was asked for or the PR last changed, and while its commit's round is under way,
/// for as long from when that began; a round that's done, or didn't get done, hands it to you.
pub fn apply(prs: &mut [PrItem], store: &Store, on: bool, now: i64) {
    for pr in prs {
        let wanted = on && wants(pr);
        let t = store.prs.get(&pr_key(&pr.key));
        let head = pr.head_oid.clone().unwrap_or_default();
        let since = pr.requested_at.unwrap_or(0).max(pr.updated_at);
        let holds = wanted
            && match t.and_then(|t| t.rounds.iter().rev().find(|r| r.head == head)) {
                Some(r) => r.stage == ReviewStage::Reviewing && now - r.at < HOLD_MS,
                None => now - since < HOLD_MS,
            };
        pr.auto_review = match t.and_then(|t| t.rounds.last().map(|r| (t, r))) {
            Some((t, r)) => Some(AutoReview {
                stage: r.stage,
                head: r.head.clone(),
                session: t.session.clone(),
                at: r.ended_at.unwrap_or(r.at),
                comment: r.comment.clone().or_else(|| t.rounds.iter().rev().find_map(|r| r.comment.clone()).filter(|_| r.stage != ReviewStage::Posted)),
                note: r.note.clone(),
                rounds: t.rounds.len(),
                behind: wanted && head != r.head,
                holds,
            }),
            None if wanted => Some(AutoReview { stage: ReviewStage::Queued, head, session: None, at: since, comment: None, note: None, rounds: 0, behind: false, holds }),
            None => None,
        };
    }
}

// ---- What the session is told, and what's posted ----

/// A commit as people read it.
fn short(sha: &str) -> &str {
    &sha[..sha.len().min(7)]
}

/// The mark a posted review carries, unseen on GitHub: which PR and commit it's of.
pub fn marker(pr_key_: &str, head: &str) -> String {
    format!("<!-- codealong-review {} {head} -->", pr_key(pr_key_))
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

/// Where the review's last message goes.
fn last_message(login: &str, comment: bool) -> String {
    if comment {
        format!("Codealong posts your last message on the PR as a comment from {login}'s GitHub account, word for word, with a line saying an agent wrote it. So make that message the review and nothing else, written to the PR's author in GitHub Markdown")
    } else {
        format!("Your last message is the review, for {login} to read here; nothing is posted. Make it the review and nothing else, in Markdown")
    }
}

const SKIP: &str = "If you can't review it, make your last message `SKIP: ` and why, and nothing is posted.";
const HANDS_OFF: &str = "Don't change any files, commit, push, or post on GitHub.";

/// The first prompt of a review's own session. `holds`: its folder has the PR's branch checked
/// out. `since`: the commit an earlier review of it was of, by a session that's gone, with
/// `files.changes` saying what changed since where that follows on.
pub fn first_prompt(pr: &PrItem, head: &str, since: Option<&str>, holds: bool, login: &str, comment: bool, files: &Reading) -> String {
    let base = pr.base.as_deref().unwrap_or("its base branch");
    let by = pr.author.as_deref().map(|a| format!(", by {a}")).unwrap_or_default();
    let mut out = format!(
        "Review pull request #{} in {}, “{}” ({}){by}: it merges {} into {base}, and is at commit {}. Codealong started you to review it for {login}, whose review it waits on.\n\n",
        pr.number,
        pr.repo_slug,
        pr.title.trim(),
        pr.url,
        pr.head,
        short(head)
    );
    out += &format!("Its description, reviews, comments and changed files are in {}, and its whole diff is in {}. ", files.md, files.diff);
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
    out += &format!("Look for bugs, security and data risks, missing tests, and anything that doesn't do what the PR says it does. {HANDS_OFF}\n\n");
    out += &format!(
        "{}: a line on what you make of it, then each finding, most serious first, with its file and line and what to do about it. Leave out praise and a summary of the PR. If you find nothing worth raising, say so in a sentence. {SKIP}",
        last_message(login, comment)
    );
    out
}

/// What a review's session is sent when the PR has new commits since the one it last reviewed.
pub fn again_prompt(pr: &PrItem, head: &str, since: Option<&str>, login: &str, comment: bool, files: &Reading) -> String {
    let mut out = match since {
        Some(s) => format!("Codealong: #{} “{}” has new commits since you reviewed {}: it's at {} now. ", pr.number, pr.title.trim(), short(s), short(head)),
        None => format!("Codealong: review #{} “{}” again; it's at {} now. ", pr.number, pr.title.trim(), short(head)),
    };
    out += &format!("{} has it as it is now, with what's been said on it since, and {} its whole diff. ", files.md, files.diff);
    match (since, &files.changes) {
        (Some(s), Some(c)) => out += &format!("What changed since {} is in {c}.\n\n", short(s)),
        (Some(_), None) => out += "Its branch was rewritten since then (rebased or force-pushed), so review the whole diff again.\n\n",
        (None, _) => out += "\n\n",
    }
    let new = if comment { " as a new comment" } else { "" };
    out += &format!(
        "Review it again. {HANDS_OFF} {}{new}: start with which of your earlier findings are fixed and which aren't, then anything new, most serious first, with its file and line. If nothing needs saying, say so in a sentence. {SKIP}",
        last_message(login, comment)
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

/// Why the agent passed on a review, when its reply says `SKIP:`.
pub fn skipped(reply: &str) -> Option<String> {
    let t = reply.trim();
    let rest = t.get(..5).filter(|p| p.eq_ignore_ascii_case("skip:")).map(|_| t[5..].trim())?;
    Some(if rest.is_empty() { "Its agent couldn't review it.".to_string() } else { rest.to_string() })
}

/// The comment posted for a review: the reply, then a line saying an agent wrote it, of which
/// commit, and the unseen mark (`marker`).
pub fn comment_body(reply: &str, pr: &PrItem, head: &str, agent: &str) -> String {
    let mut text = reply.trim().to_string();
    if text.chars().count() > COMMENT_MAX {
        let cut = crate::util::truncate(&text, COMMENT_MAX);
        text = format!("{}\n\n…(cut short: the whole review is longer than a comment can be)", cut.trim_end());
    }
    format!("{text}\n\n<sub>An agent's review ({agent}) of {}, posted by Codealong.</sub>\n{}", short(head), marker(&pr.key, head))
}

// ---- Doing the jobs ----

/// The account a new review runs on, and where: `[reviews] account`, else the one Auto would
/// start, out of sight: under its harness's background service, else in Codealong, else in a
/// terminal behind your windows (Background for a harness with no service of its own).
fn reviewer_place(cfg: &Config, board: &Board) -> Result<(String, Place), String> {
    let unseen = [Place::Background, Place::Codealong];
    let home = crate::util::home();
    let name = match cfg.reviews.account.trim() {
        "" => crate::start::pick_account(cfg, &board.usage, &unseen, None, now_ms()).ok_or("no account can start a session in the background")?,
        n => n.to_string(),
    };
    let a = cfg.accounts.iter().find(|a| a.name == name).ok_or_else(|| format!("there's no account named {name}"))?;
    let offered = crate::start::ordered_places(a, &home).unwrap_or_default();
    let service = crate::harness::of(a).is_some_and(|h| h.runs_in_background());
    let order = if service { unseen } else { [Place::Codealong, Place::Background] };
    let place = order.into_iter().find(|p| offered.contains(p)).ok_or_else(|| format!("{name} can't start a session in the background"))?;
    Ok((name, place))
}

/// Links the review's session to its PR, so the PR's card lists it.
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
        diff: keep(diff, format!("pr-{}.diff", pr.number))?,
        changes: changes.map(|c| keep(c, format!("pr-{}-to-{}.diff", pr.number, short(head)))).transpose()?,
    })
}

/// Starts or asks for a round: reads the PR, then sends its session the new commits, goes on with
/// it in the background where it isn't running, or starts a new one.
fn review(engine: &Handle, pr: &PrItem, head: &str, since: Option<&str>, session: Option<&str>) -> Went {
    let board = engine.board();
    let cfg = match crate::config::current() {
        Ok(c) => c,
        Err(e) => return Went::NotStarted(format!("{e:#}")),
    };
    let Some(pr) = board.prs.iter().find(|p| p.key == pr.key) else { return Went::Later };
    if pr.head_oid.as_deref() != Some(head) || !wants(pr) {
        return Went::Later;
    }
    let login = board.github_login.clone().unwrap_or_else(|| "you".into());
    let (md, diff, detail) = match crate::github::for_reading(pr) {
        Ok(r) => r,
        Err(e) => return Went::NotStarted(format!("{e:#}")),
    };
    // Reviewed already, from another Mac or before a restart.
    let mark = marker(&pr.key, head);
    if let Some(c) = detail.comments.iter().find(|c| c.mine && c.body.contains(&mark)) {
        return Went::Posted(c.url.clone().unwrap_or_else(|| pr.url.clone()));
    }
    let changes = since.and_then(|s| crate::github::changes_since(&pr.repo_slug, s, head).ok().flatten());
    let files = match keep_files(pr, head, &md, &diff, changes.as_deref()) {
        Ok(f) => f,
        Err(e) => return Went::NotStarted(e),
    };
    let comment = cfg.reviews.comment;
    let lane_of = |key: &str| board.session(key).map(|(l, s)| (l.id.clone(), s.clone()));
    if let Some((lane, s)) = session.and_then(lane_of) {
        let text = again_prompt(pr, head, since, &login, comment, &files);
        let at = now_ms();
        if s.live {
            let params = codealong_peer::api::params::Send { key: String::new(), text, now: false };
            return match crate::actions::local(engine, &crate::actions::From::Here, codealong_peer::api::SESSION_SEND, &s.key, params) {
                Ok(_) => Went::Started { session: Some(s.key.clone()), pending: None, at },
                Err(e) => Went::NotStarted(format!("its session couldn't be sent the new commits: {e}")),
            };
        }
        let resumes = cfg.accounts.iter().find(|a| a.name == s.account).is_some_and(|a| crate::start::resume_places(a, &crate::util::home()).contains(&Place::Background));
        if resumes {
            let how = ResumeIn { place: Place::Background, model: None, effort: None };
            return match crate::start::resume(&board, &cfg, &s.key, &how, &text, &crate::send::Sender::You) {
                Ok(r) => {
                    if let Some(k) = r.key.as_deref().filter(|k| *k != s.key) {
                        link(engine, pr, k);
                    }
                    let pending = r.key.is_none().then(|| Pending { lane, account: s.account.clone(), at });
                    Went::Started { session: r.key, pending, at }
                }
                Err(e) => Went::NotStarted(format!("its session couldn't go on in the background: {e:#}")),
            };
        }
    }
    let spec = match new_start(&board, &cfg, pr, head, since, &login, &files) {
        Ok(s) => s,
        Err(e) => return Went::NotStarted(e),
    };
    let at = now_ms();
    let params = codealong_peer::api::params::Start { spec: spec.clone() };
    match crate::actions::local(engine, &crate::actions::From::Here, codealong_peer::api::START, &spec.lane_id, params).and_then(crate::actions::typed::<crate::start::Started>) {
        Ok(started) => {
            if let Some(k) = &started.key {
                link(engine, pr, k);
            }
            let pending = started.key.is_none().then(|| Pending { lane: spec.lane_id.clone(), account: spec.account.clone(), at });
            Went::Started { session: started.key, pending, at }
        }
        Err(e) => Went::NotStarted(e),
    }
}

/// A new session's start for a review: on the reviews' account, where it starts in the
/// background, in the worktree `side::pr_lane` picks, on the first prompt.
fn new_start(board: &Board, cfg: &Config, pr: &PrItem, head: &str, since: Option<&str>, login: &str, files: &Reading) -> Result<StartSpec, String> {
    let (account, place) = reviewer_place(cfg, board)?;
    let (lane, holds) = crate::side::pr_lane(board, pr).ok_or_else(|| format!("there's no checkout of {} on this Mac", pr.repo_slug))?;
    let prompt = first_prompt(pr, head, since, holds, login, cfg.reviews.comment, files);
    Ok(StartSpec { lane_id: lane.id.clone(), account, place, prompt, model: None, effort: None, worktree: false, todos: Vec::new(), from: None })
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
    let since = stands.as_ref().filter(|a| a.head != head).map(|a| a.head.clone());
    let changes = since.as_deref().and_then(|s| crate::github::changes_since(&pr.repo_slug, s, &head).ok().flatten());
    let files = keep_files(pr, &head, &md, &diff, changes.as_deref()).map_err(anyhow::Error::msg)?;
    let login = board.github_login.clone().unwrap_or_else(|| "you".into());
    let (spec, problem) = match new_start(board, cfg, pr, &head, since.as_deref(), &login, &files) {
        Ok(s) => (Some(s), None),
        Err(e) => (None, Some(e)),
    };
    let start = spec.as_ref().and_then(|s| {
        let account = cfg.accounts.iter().find(|a| a.name == s.account)?;
        let lane = board.lanes.iter().find(|l| l.id == s.lane_id)?;
        Some(crate::start::plan(account, s.place, std::path::Path::new(&lane.path), &s.prompt, &crate::start::new_session_id(), None, None, false))
    });
    Ok(Plan { wanted: wants(pr), on: cfg.reviews.auto, stands, spec, start, problem })
}

/// Reads a round's review from its session and posts it, unless posting is off, the agent passed
/// on it, or the PR moved on or ended before it was done.
fn post(engine: &Handle, pr: &PrItem, head: &str, session: &str, at: i64) -> Went {
    let board = engine.board();
    let cfg = match crate::config::current() {
        Ok(c) => c,
        Err(e) => return Went::PostFailed(format!("{e:#}")),
    };
    let Some((_, s)) = board.session(session) else { return Went::Failed("Its session is gone.".into()) };
    let items = match crate::transcript::page(&board, &cfg, session, None, 60) {
        Ok(p) => p.items,
        Err(e) => return Went::PostFailed(format!("its session's conversation couldn't be read: {e:#}")),
    };
    let Some(reply) = reply_after(&items, at) else {
        return if s.live { Went::NotYet } else { Went::Failed("Its session stopped before it wrote a review.".into()) };
    };
    if let Some(why) = skipped(&reply) {
        return Went::Skipped(why);
    }
    let pr = board.prs.iter().find(|p| p.key == pr.key).unwrap_or(pr);
    if !pr.state.is_open() {
        return Went::Kept(Some("Not posted: the PR was closed before the review was done.".into()));
    }
    if pr.head_oid.as_deref().is_some_and(|h| h != head) {
        return Went::Failed("Not posted: new commits came in before it was done.".into());
    }
    if !cfg.reviews.comment {
        return Went::Kept(None);
    }
    let harness = crate::harness::adapter(s.harness).harness().word();
    let agent = s.model.as_deref().map(|m| format!("{harness}, {m}")).unwrap_or_else(|| harness.to_string());
    match crate::github::comment(&pr.repo_slug, pr.number, &comment_body(&reply, pr, head, &agent)) {
        Ok(link) => {
            engine.pr_changed();
            Went::Posted(link)
        }
        Err(e) => Went::PostFailed(format!("{e:#}")),
    }
}

/// Does a job and tells `reviewer` how it went, and the engine, which says a failure as a notice
/// until a later job goes through.
pub fn run(engine: &Handle, reviewer: &Mutex<Reviewer>, job: Job) {
    let went = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match &job {
        Job::Review { pr, head, since, session } => review(engine, pr, head, since.as_deref(), session.as_deref()),
        Job::Post { pr, head, session, at } => post(engine, pr, head, session, *at),
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

    fn session(status: Status, last_activity: i64) -> Session {
        Session { status, last_activity, ..fixtures::session(S) }
    }

    fn kinds(jobs: &[Job]) -> Vec<String> {
        jobs.iter()
            .map(|j| match j {
                Job::Review { pr, head, since, session } => format!("review {} {} since {:?} in {:?}", pr.number, short(head), since.as_deref().map(short), session),
                Job::Post { pr, head, .. } => format!("post {} {}", pr.number, short(head)),
            })
            .collect()
    }

    #[test]
    fn a_pr_that_waits_on_you_is_reviewed_once_per_commit_and_posted_once_its_turn_ends() {
        let mut r = Reviewer::default();
        let b = board(vec![pr(7, HEAD)], vec![]);
        let jobs = r.due(&b, &on(), NOW);
        assert_eq!(kinds(&jobs), ["review 7 1111111 since None in None"]);
        assert!(r.due(&b, &on(), NOW + 1).is_empty(), "under way");
        assert_eq!(r.told(&jobs[0], Went::Started { session: Some(S.into()), pending: None, at: NOW }, NOW + 2), Ok(()));
        // Working: nothing yet.
        assert!(r.due(&board(vec![pr(7, HEAD)], vec![session(Status::Working, NOW + 5)]), &on(), NOW + 10).is_empty());
        // Its turn ended: posted.
        let done = board(vec![pr(7, HEAD)], vec![session(Status::Unread, NOW + 60_000)]);
        let jobs = r.due(&done, &on(), NOW + 61_000);
        assert_eq!(kinds(&jobs), ["post 7 1111111"]);
        // Not written yet: looked at again later.
        r.told(&jobs[0], Went::NotYet, NOW + 61_000).unwrap();
        assert!(r.due(&done, &on(), NOW + 62_000).is_empty());
        let jobs = r.due(&done, &on(), NOW + 61_000 + RETRY_MS);
        r.told(&jobs[0], Went::Posted("https://github.com/o/r/pull/7#issuecomment-1".into()), NOW + 92_000).unwrap();
        // That commit is done.
        assert!(r.due(&done, &on(), NOW + 10 * QUIET_MS).is_empty());
        let mut prs = vec![pr(7, HEAD)];
        apply(&mut prs, &r.store, true, NOW);
        let a = prs[0].auto_review.clone().unwrap();
        assert_eq!((a.stage, a.head.as_str(), a.session.as_deref(), a.comment.as_deref(), a.rounds, a.behind), (ReviewStage::Posted, HEAD, Some(S), Some("https://github.com/o/r/pull/7#issuecomment-1"), 1, false));
    }

    #[test]
    fn a_pr_is_held_out_of_what_waits_on_you_until_its_review_is_done() {
        let fresh = |head: &str| PrItem { requested_at: Some(NOW - 60_000), updated_at: NOW - 60_000, ..pr(7, head) };
        let shown = |store: &Store, p: PrItem, on: bool, at: i64| {
            let mut prs = vec![p];
            apply(&mut prs, store, on, at);
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
        r.told(&jobs[0], Went::Started { session: Some(S.into()), pending: None, at: NOW }, NOW).unwrap();
        assert!(!shown(&r.store, fresh(HEAD), true, NOW + 60_000).waits_on_review());
        assert!(shown(&r.store, fresh(HEAD), true, NOW + HOLD_MS).waits_on_review());
        // Done: it waits on you.
        let jobs = r.due(&board(vec![fresh(HEAD)], vec![session(Status::Unread, NOW + 5)]), &on(), NOW + 6);
        r.told(&jobs[0], Went::Posted("link".into()), NOW + 7).unwrap();
        assert!(shown(&r.store, fresh(HEAD), true, NOW + 8).waits_on_review());
        // New commits: held again until they're reviewed; a round that didn't get done hands it back.
        let moved = PrItem { updated_at: NOW + 9, ..fresh(NEXT) };
        let p = shown(&r.store, moved.clone(), true, NOW + 10);
        assert_eq!(p.auto_review.as_ref().map(|a| (a.stage, a.behind, a.holds)), Some((ReviewStage::Posted, true, true)));
        r.store.prs.get_mut("o/r#7").unwrap().rounds.push(Round { head: NEXT.into(), stage: ReviewStage::Failed, at: NOW + 11, ended_at: Some(NOW + 11), comment: None, note: None });
        assert!(shown(&r.store, moved, true, NOW + 12).waits_on_review());
    }

    #[test]
    fn new_commits_are_reviewed_again_in_the_same_session_once_the_branch_holds_still() {
        let mut r = Reviewer::default();
        let jobs = r.due(&board(vec![pr(7, HEAD)], vec![]), &on(), NOW);
        r.told(&jobs[0], Went::Started { session: Some(S.into()), pending: None, at: NOW }, NOW).unwrap();
        let jobs = r.due(&board(vec![pr(7, HEAD)], vec![session(Status::Idle, NOW + 1)]), &on(), NOW + 2);
        r.told(&jobs[0], Went::Posted("link".into()), NOW + 3).unwrap();
        let moved = board(vec![pr(7, NEXT)], vec![session(Status::Idle, NOW + 1)]);
        assert!(r.due(&moved, &on(), NOW + 10).is_empty(), "first seen");
        let mut prs = vec![pr(7, NEXT)];
        apply(&mut prs, &r.store, true, NOW);
        assert!(prs[0].auto_review.as_ref().unwrap().behind);
        assert!(r.due(&moved, &on(), NOW + QUIET_MS).is_empty(), "still settling");
        assert_eq!(kinds(&r.due(&moved, &on(), NOW + 10 + QUIET_MS)), [format!("review 7 2222222 since Some(\"1111111\") in Some(\"{S}\")")]);
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
        assert_eq!(kinds(&r.due(&board(vec![pr(6, HEAD)], vec![]), &on(), NOW)), ["review 6 1111111 since None in None"]);
    }

    #[test]
    fn a_few_run_at_once_and_one_waiting_on_you_holds_no_place() {
        let mut r = Reviewer::default();
        let prs = vec![pr(3, HEAD), pr(1, HEAD), pr(2, HEAD)];
        let jobs = r.due(&board(prs.clone(), vec![]), &on(), NOW);
        // The longest waiting first.
        assert_eq!(kinds(&jobs), ["review 1 1111111 since None in None", "review 2 1111111 since None in None"]);
        r.told(&jobs[0], Went::Started { session: Some(S.into()), pending: None, at: NOW }, NOW).unwrap();
        r.told(&jobs[1], Went::Started { session: Some("claude:claude:two".into()), pending: None, at: NOW }, NOW).unwrap();
        let two = Session { status: Status::Working, ..fixtures::session("claude:claude:two") };
        assert!(r.due(&board(prs.clone(), vec![session(Status::Working, NOW), two.clone()]), &on(), NOW + 1).is_empty());
        // One waits on you to allow something: the third starts.
        let waits = Session { asks: false, ..session(Status::Needs, NOW + 5) };
        assert_eq!(kinds(&r.due(&board(prs, vec![waits, two]), &on(), NOW + 6)), ["review 3 1111111 since None in None"]);
    }

    #[test]
    fn a_start_that_named_no_session_finds_it_by_its_prompt() {
        let mut r = Reviewer::default();
        let jobs = r.due(&board(vec![pr(7, HEAD)], vec![]), &on(), NOW);
        let pending = Pending { lane: "/r".into(), account: "codex".into(), at: NOW };
        r.told(&jobs[0], Went::Started { session: None, pending: Some(pending), at: NOW }, NOW).unwrap();
        let other = Session { account: "codex".into(), started_at: Some(NOW + 1), first_prompt: Some("something else".into()), ..fixtures::session("codex:codex:x") };
        let mine = Session { account: "codex".into(), started_at: Some(NOW + 2), first_prompt: Some("Review pull request #7 in o/r, “Add teams” (https://github.com/o/r/pull/7)".into()), status: Status::Working, ..fixtures::session("codex:codex:y") };
        r.due(&board(vec![pr(7, HEAD)], vec![other.clone()]), &on(), NOW + 3);
        assert_eq!(r.store.prs["o/r#7"].session, None);
        r.due(&board(vec![pr(7, HEAD)], vec![other, mine]), &on(), NOW + 4);
        assert_eq!(r.store.prs["o/r#7"].session.as_deref(), Some("codex:codex:y"));
        // One that never shows ends its round.
        let mut r = Reviewer::default();
        let jobs = r.due(&board(vec![pr(7, HEAD)], vec![]), &on(), NOW);
        r.told(&jobs[0], Went::Started { session: None, pending: Some(Pending { lane: "/r".into(), account: "codex".into(), at: NOW }), at: NOW }, NOW).unwrap();
        r.due(&board(vec![pr(7, HEAD)], vec![]), &on(), NOW + SHOW_MS + 1);
        assert_eq!(r.store.prs["o/r#7"].rounds[0].stage, ReviewStage::Failed);
    }

    #[test]
    fn a_failed_start_or_post_is_said_and_not_tried_again_for_that_commit() {
        let mut r = Reviewer::default();
        let b = board(vec![pr(7, HEAD)], vec![]);
        let jobs = r.due(&b, &on(), NOW);
        assert!(r.told(&jobs[0], Went::NotStarted("claude can't start a session in the background".into()), NOW).unwrap_err().contains("in the background"));
        assert!(r.due(&b, &on(), NOW + 10 * QUIET_MS).is_empty());
        // Posting is tried a few times, waiting longer each time.
        let mut r = Reviewer::default();
        let jobs = r.due(&b, &on(), NOW);
        r.told(&jobs[0], Went::Started { session: Some(S.into()), pending: None, at: NOW }, NOW).unwrap();
        let done = board(vec![pr(7, HEAD)], vec![session(Status::Unread, NOW + 1)]);
        let mut at = NOW + 2;
        let mut waits = Vec::new();
        for _ in 1..POST_TRIES {
            let job = r.due(&done, &on(), at).pop().unwrap();
            assert!(r.told(&job, Went::PostFailed("gh: Not Found".into()), at).is_err());
            let next = (at..).step_by(1000).find(|&t| !r.due(&done, &on(), t).is_empty()).unwrap();
            waits.push((next - at) / 1000);
            // That due started a job: say it's still failing.
            r.busy.clear();
            at = next;
        }
        assert_eq!(waits, [30, 60, 120]);
        let job = r.due(&done, &on(), at).pop().unwrap();
        assert!(r.told(&job, Went::PostFailed("gh: Not Found".into()), at).unwrap_err().contains("Not Found"));
        assert_eq!(r.store.prs["o/r#7"].rounds[0].stage, ReviewStage::Failed);
        assert!(r.due(&done, &on(), at + 10 * QUIET_MS).is_empty());
    }

    #[test]
    fn a_round_stuck_while_the_pr_moved_on_makes_way_and_a_gone_session_ends_it() {
        let mut r = Reviewer::default();
        let jobs = r.due(&board(vec![pr(7, HEAD)], vec![]), &on(), NOW);
        r.told(&jobs[0], Went::Started { session: Some(S.into()), pending: None, at: NOW }, NOW).unwrap();
        let stuck = board(vec![pr(7, NEXT)], vec![session(Status::Error, NOW + 1)]);
        r.due(&stuck, &on(), NOW + 2);
        assert_eq!(r.store.prs["o/r#7"].rounds[0].stage, ReviewStage::Failed);
        assert_eq!(kinds(&r.due(&stuck, &on(), NOW + 3 + QUIET_MS)), [format!("review 7 2222222 since None in Some(\"{S}\")")]);
        let mut r = Reviewer::default();
        let jobs = r.due(&board(vec![pr(7, HEAD)], vec![]), &on(), NOW);
        r.told(&jobs[0], Went::Started { session: Some(S.into()), pending: None, at: NOW }, NOW).unwrap();
        r.due(&board(vec![pr(7, HEAD)], vec![]), &on(), NOW + 1);
        assert_eq!(r.store.prs["o/r#7"].rounds[0].stage, ReviewStage::Reviewing, "not showing yet");
        r.due(&board(vec![pr(7, HEAD)], vec![]), &on(), NOW + SHOW_MS + 1);
        assert_eq!(r.store.prs["o/r#7"].rounds[0].stage, ReviewStage::Failed);
    }

    #[test]
    fn reviews_are_kept_across_launches_and_forgotten_long_after() {
        let dir = fixtures::scratch("autoreview");
        let path = dir.join("reviews.json");
        let mut r = Reviewer { path: Some(path.clone()), ..Reviewer::default() };
        let jobs = r.due(&board(vec![pr(7, HEAD)], vec![]), &on(), NOW);
        r.told(&jobs[0], Went::Started { session: Some(S.into()), pending: None, at: NOW }, NOW).unwrap();
        let mut again = Reviewer { store: crate::store::load_whole(&path, WHAT), path: Some(path), ..Reviewer::default() };
        assert_eq!(again.store, r.store);
        // Under way, it's followed: nothing new starts for that commit.
        assert!(again.due(&board(vec![pr(7, HEAD)], vec![session(Status::Working, NOW + 1)]), &on(), NOW + 2).is_empty());
        // Ended, and the PR no longer waits on you: forgotten a month on.
        let jobs = again.due(&board(vec![pr(7, HEAD)], vec![session(Status::Idle, NOW + 3)]), &on(), NOW + 4);
        again.told(&jobs[0], Went::Kept(None), NOW + 5).unwrap();
        again.due(&board(vec![], vec![]), &on(), NOW + 6);
        assert!(again.store.prs.contains_key("o/r#7"));
        again.due(&board(vec![], vec![]), &on(), NOW + 6 + KEEP_MS);
        assert!(again.store.prs.is_empty());
    }

    fn item(kind: &'static str, at: i64, text: &str) -> Item {
        crate::transcript::item(at, kind, text)
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
        assert_eq!(skipped("SKIP: the diff is a vendored library"), Some("the diff is a vendored library".into()));
        assert_eq!(skipped("skip:"), Some("Its agent couldn't review it.".into()));
        assert_eq!(skipped("Skipping the tests, the rest is fine."), None);
    }

    #[test]
    fn what_the_session_is_told_and_what_is_posted() {
        let files = Reading { md: "[pr-7.md](files/aa/pr-7.md)".into(), diff: "[pr-7.diff](files/bb/pr-7.diff)".into(), changes: Some("[pr-7-to-2222222.diff](files/cc/pr-7-to-2222222.diff)".into()) };
        let first = first_prompt(&pr(7, HEAD), HEAD, None, false, "gwen", true, &files);
        assert!(first.starts_with("Review pull request #7 in o/r, “Add teams” (https://github.com/o/r/pull/7), by ana: it merges teams into main, and is at commit 1111111."), "{first}");
        for part in ["[pr-7.md](files/aa/pr-7.md)", "without its branch", "Don't change any files, commit, push, or post on GitHub.", "as a comment from gwen's GitHub account", "`SKIP: `"] {
            assert!(first.contains(part), "{part}: {first}");
        }
        assert!(!first.contains("earlier review"));
        let later = first_prompt(&pr(7, NEXT), NEXT, Some(HEAD), true, "gwen", false, &files);
        for part in ["has its branch checked out", "An earlier review of commit 1111111", "[pr-7-to-2222222.diff]", "for gwen to read here; nothing is posted"] {
            assert!(later.contains(part), "{part}: {later}");
        }
        let again = again_prompt(&pr(7, NEXT), NEXT, Some(HEAD), "gwen", true, &files);
        assert!(again.starts_with("Codealong: #7 “Add teams” has new commits since you reviewed 1111111: it's at 2222222 now."), "{again}");
        assert!(again.contains("What changed since 1111111 is in [pr-7-to-2222222.diff]") && again.contains("as a new comment"), "{again}");
        let rewritten = again_prompt(&pr(7, NEXT), NEXT, Some(HEAD), "gwen", true, &Reading { changes: None, ..files });
        assert!(rewritten.contains("rewritten since then (rebased or force-pushed)"), "{rewritten}");
        let body = comment_body("  Looks right.\n", &pr(7, HEAD), HEAD, "Claude Code, claude-opus-5-5");
        assert_eq!(body, "Looks right.\n\n<sub>An agent's review (Claude Code, claude-opus-5-5) of 1111111, posted by Codealong.</sub>\n<!-- codealong-review o/r#7 1111111aaaa -->");
        let long = comment_body(&"x".repeat(COMMENT_MAX + 10), &pr(7, HEAD), HEAD, "Codex");
        assert!(long.len() < 65_536 && long.contains("cut short"));
    }
}
