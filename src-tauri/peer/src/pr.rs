//! Pull requests as one device tells another of them: the Mac's PR list (its PRs view, cut down to a
//! card each), a PR's detail as its PR panel shows it, and its merge box. A Mac reads all of them
//! from GitHub through `gh`, signed in as you; a phone has no GitHub of its own, so it asks a Mac
//! (`api::PR_DETAIL`, `PR_MERGE_BOX`, `PR_REVIEW`, `PR_MERGE`, `PR_SNOOZE`).
//!
//! The engine keeps its own types for these (model.rs `PrItem`, github.rs `PrDetail`, prmerge.rs
//! `MergeBox` and `Op`), which it writes to its windows. What goes between devices is written here,
//! in the same shape, and the engine's tests read what it sends into these types, so the two can't
//! drift apart.

use serde::{Deserialize, Serialize};

use crate::model::{Checks, Merge, PrLabel, PrState};

/// A PR on a device's board, for a linked device's PR list: open ones, and ones merged or closed in
/// the last day. Only devices that say they know PRs (`proto::PRS`) are sent them (`FeedMsg::prs`).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PrCard {
    /// `owner/name#number`, as the requests about it name it with `repo` and `number`.
    pub key: String,
    /// `owner/name`.
    pub repo: String,
    /// The project its repo is in on the sending device.
    pub project: String,
    pub number: u64,
    pub title: String,
    pub url: String,
    pub state: PrState,
    pub author: Option<String>,
    /// You opened it.
    #[serde(default)]
    pub mine: bool,
    /// Your review is asked for, and it isn't snoozed or held by Codealong's own review of it: it
    /// waits on you, as the Mac's Review requested and its notch count it.
    #[serde(default)]
    pub waits: bool,
    /// Your review is asked for, snoozed or not.
    #[serde(default)]
    pub review_requested: bool,
    /// Who asked for your review, when GitHub says.
    #[serde(default)]
    pub requested_by: Option<String>,
    /// When your review was last asked for.
    #[serde(default)]
    pub requested_at: Option<i64>,
    /// You snoozed your review until then.
    #[serde(default)]
    pub snoozed_until: Option<i64>,
    /// Pinned on the sending device.
    #[serde(default)]
    pub pinned: bool,
    pub head: String,
    #[serde(default)]
    pub base: Option<String>,
    #[serde(default)]
    pub checks: Checks,
    #[serde(default)]
    pub merge: Merge,
    #[serde(default)]
    pub additions: u64,
    #[serde(default)]
    pub deletions: u64,
    #[serde(default)]
    pub changed_files: u64,
    #[serde(default)]
    pub created_at: i64,
    #[serde(default)]
    pub updated_at: i64,
    #[serde(default)]
    pub merged_at: Option<i64>,
    #[serde(default)]
    pub closed_at: Option<i64>,
    /// The sessions that worked on it, by their keys on the sending device.
    #[serde(default)]
    pub sessions: Vec<String>,
    /// Its labels on GitHub, in the order of their names. Missing from Macs before 2026-10-17.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<PrLabel>,
    /// Where Codealong's own review of it stands on the sending device, once it's had one. Missing
    /// from Macs before 2026-10-16.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_review: Option<AutoReview>,
}

/// The latest round of Codealong's automatic review of a PR (the engine's autoreview.rs), as the
/// line over a PR's review box says it: the engine's `AutoReview` cut down, in the same shape.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AutoReview {
    pub stage: ReviewStage,
    /// The commit it reviews, as GitHub names it.
    pub head: String,
    /// What its agent would do with the PR, which the review box suggests.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggests: Option<Verdict>,
    /// What its review was submitted as yours as (`[reviews] submit`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub submitted: Option<Verdict>,
}

/// Where a round of the automatic review stands.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ReviewStage {
    /// Its commit is to be reviewed, and no session has started on it yet.
    Queued,
    Reviewing,
    /// Reviewed, and posted on the PR as a comment.
    Posted,
    /// Reviewed, and put in a draft review on your account, for you to finish.
    Drafted,
    /// Reviewed, and left in its session: its Mac's settings say not to post.
    Kept,
    /// Its agent said it couldn't review it.
    Skipped,
    /// It didn't get done.
    Failed,
    /// One a later version names.
    #[serde(other)]
    Other,
}

/// A PR's description, reviews, comments and changed files with their diffs, as the Mac's PR panel
/// shows them (`api::PR_DETAIL`).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Detail {
    /// The description as written, in Markdown.
    pub body: String,
    /// `MERGEABLE`, `CONFLICTING` or `UNKNOWN`.
    pub mergeable: Option<String>,
    pub commits: u64,
    /// Each reviewer's standing review: their last approval or change request, else comment.
    pub reviews: Vec<Review>,
    /// Who's asked to review it and hasn't yet.
    pub requested: Vec<String>,
    /// Comments with something written in them, newest first.
    pub comments: Vec<Comment>,
    pub files: Vec<File>,
    /// More files than the first page holds.
    pub more_files: bool,
    /// Your review of it that you began and haven't submitted (an agent's, or one you started on
    /// GitHub). A review submits it, its comments and all, as GitHub's Finish your review does, so
    /// it's shown before one, and the review says which it showed (`params::PrReview::draft`).
    /// None when there's none, or GitHub couldn't say.
    pub draft: Option<Draft>,
}

/// Your review of a PR that you began and haven't submitted.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Draft {
    /// Its text, without Codealong's hidden marks.
    pub body: String,
    pub url: Option<String>,
    /// Its comments on lines, the first hundred.
    pub comments: Vec<DraftComment>,
    /// How many comments it has in all.
    pub count: u64,
    /// The draft as it is now, in a word: a review sends it back, and the Mac submits the draft
    /// only while it's still this one, so nothing written or edited since goes unseen.
    pub stamp: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct DraftComment {
    pub path: String,
    /// Its line in the file's new version; none for one on the file as a whole.
    pub line: Option<u64>,
    /// The PR's changes since have moved or changed its line.
    pub outdated: bool,
    pub body: String,
}

/// What a review did (`api::PR_REVIEW`).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Reviewed {
    /// It submitted your draft review, with this many comments.
    pub draft: Option<u64>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Review {
    pub author: String,
    /// `APPROVED`, `CHANGES_REQUESTED`, `COMMENTED` or `DISMISSED`.
    pub state: String,
    pub at: Option<i64>,
    /// Given by whoever the Mac's `gh` is signed in as. Missing from Macs before 2026-10-12.
    pub mine: bool,
}

/// A comment on a PR: on its conversation, a review's own text, or the first of a thread on the
/// code.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Comment {
    pub id: String,
    pub author: String,
    /// As written, in Markdown.
    pub body: String,
    pub at: Option<i64>,
    pub url: Option<String>,
    /// The file a thread on the code is about.
    pub path: Option<String>,
    /// A thread on the code: its line in the file's new version, or the old one's when `before`,
    /// none for one on the whole file or one the PR's changes since moved. Missing from Macs
    /// before 2026-10-12.
    pub line: Option<u64>,
    /// A thread on the code on the file as it was before the PR: on a line it removes, or one
    /// beside them, numbered as the file was.
    pub before: bool,
    /// A thread on the code whose line the PR's changes since moved or changed.
    pub outdated: bool,
    /// A thread on the code: how many replies follow its first comment.
    pub replies: u64,
    /// A thread on the code that's been resolved.
    pub resolved: bool,
    /// Written by whoever the Mac's `gh` is signed in as.
    pub mine: bool,
    /// A review's own text: that review's state (`APPROVED`, `CHANGES_REQUESTED`, `COMMENTED`,
    /// `DISMISSED`), which tells an earlier approval's words from a comment.
    pub review: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct File {
    pub path: String,
    /// `added`, `modified`, `removed`, `renamed`…
    pub status: String,
    pub additions: u64,
    pub deletions: u64,
    /// The unified diff; GitHub leaves it out for very large files.
    pub patch: Option<String>,
}

/// What a review of a PR says: approved, a comment, or changes asked for. A comment or a request for
/// changes needs something written; an approval's text is optional.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Verdict {
    Approve,
    Comment,
    RequestChanges,
}

/// How a PR's commits go onto its base: GitHub's three ways.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Method {
    /// A merge commit.
    Merge,
    Squash,
    Rebase,
}

/// A commit's title and message, as GitHub would write them.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct CommitText {
    pub title: String,
    pub body: String,
}

/// Auto-merge, while it's on: who turned it on and when, and how it'll merge.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AutoMerge {
    pub method: Option<Method>,
    pub by: Option<String>,
    pub at: Option<i64>,
}

/// Its place in the merge queue.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Queued {
    pub position: Option<u64>,
    /// `QUEUED`, `AWAITING_CHECKS`, `MERGEABLE`, `UNMERGEABLE` or `LOCKED`.
    pub state: Option<String>,
}

/// What a PR's merge box offers you, and where the PR stands for it, as GitHub's has it
/// (`api::PR_MERGE_BOX`; the engine's prmerge.rs reads it).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MergeBox {
    /// The PR's node id, which every act names it by.
    pub id: String,
    /// The head commit the box was read at, which a merge or an update names.
    pub head: String,
    pub branch: String,
    /// `OPEN`, `MERGED` or `CLOSED`.
    pub state: String,
    pub draft: bool,
    /// `MERGEABLE`, `CONFLICTING`, or `UNKNOWN` while GitHub works it out.
    pub mergeable: String,
    /// `mergeStateStatus`: `CLEAN`, `HAS_HOOKS`, `UNSTABLE`, `BLOCKED`, `BEHIND`, `DIRTY` or
    /// `UNKNOWN`.
    pub merge_state: String,
    /// `APPROVED`, `CHANGES_REQUESTED` or `REVIEW_REQUIRED`; None where the repo needs no review.
    pub review_decision: Option<String>,
    /// You can write to the repo, which merging needs.
    pub can_merge: bool,
    /// The methods the repo allows, in GitHub's order.
    pub methods: Vec<Method>,
    /// The one GitHub picks for you in this repo, when the repo allows it.
    pub default_method: Option<Method>,
    pub merge_text: CommitText,
    pub squash_text: CommitText,
    /// The emails a commit you make can be under, and the one GitHub picks.
    pub emails: Vec<String>,
    pub default_email: Option<String>,
    /// You can take it out of draft.
    pub can_ready: bool,
    /// You can merge it before its requirements are met, past the rules.
    pub can_bypass: bool,
    pub can_auto: bool,
    pub can_stop_auto: bool,
    pub auto: Option<AutoMerge>,
    /// Its branch is behind its base and you can bring it up to date.
    pub can_update_branch: bool,
    /// Its base branch merges through a queue.
    pub queue: bool,
    pub queued: Option<Queued>,
    /// GitHub deletes the branch itself once it's merged.
    pub deletes_branch: bool,
    /// The branch's ref, while it's there and you can push to its repo: what deleting it names.
    pub branch_ref: Option<String>,
    pub merged_by: Option<String>,
}

/// What a merge or auto-merge writes: the method, and for a merge commit or a squash the commit's
/// title, message and email (None leaves each to GitHub).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Commit {
    pub method: Method,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
}

/// One thing the merge box does (`api::PR_MERGE`). What merges or moves the branch names the head
/// commit the box was read at, so GitHub refuses it when the branch has moved since.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "op", rename_all = "kebab-case", rename_all_fields = "camelCase")]
pub enum Op {
    /// Merges it now: past the rules too, for one who may.
    Merge {
        #[serde(flatten)]
        commit: Commit,
        head: String,
        /// The branch's ref, to delete once it's merged.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        delete_branch: Option<String>,
    },
    /// Merges it once its requirements are met.
    Auto {
        #[serde(flatten)]
        commit: Commit,
        head: String,
    },
    StopAuto,
    /// Into the queue its base branch merges through, which picks the method.
    Enqueue { head: String },
    Dequeue,
    /// Brings the branch up to date with its base, by a merge commit or a rebase.
    UpdateBranch { rebase: bool, head: String },
    /// Takes it out of draft.
    Ready,
    DeleteBranch { branch_ref: String },
}

/// What a merge box's act did (`api::PR_MERGE`).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Merged {
    /// What it couldn't do after (a branch that stayed).
    pub note: Option<String>,
    /// It merged the PR, rather than queueing it, turning on auto-merge or the rest.
    pub merged: bool,
}
