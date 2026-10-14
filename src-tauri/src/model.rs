//! The board as the UI sees it. Field names serialize in camelCase to match `src/types.ts`.
//! Worktrees, sessions and plan usage go to your other devices too, so they're defined in
//! codealong-peer (peer/src/model.rs) and used from here; this file has the rest.

use serde::{Deserialize, Serialize};

use crate::config::Theme;
pub use codealong_peer::model::*;

/// The app-wide mark: attention wins over unread results, then running sessions.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum LogoStatus {
    Error,
    Needs,
    Unread,
    Working,
    #[default]
    Idle,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RepoSummary {
    pub name: String,
    pub lanes: usize,
    pub folder: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSummary {
    pub name: String,
    pub lanes: usize,
    pub repos: Vec<RepoSummary>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct AppNames {
    pub terminal: String,
    pub terminal_id: String,
    pub editor: String,
    pub editor_id: String,
    /// The editors installed here, in menu order: the Open in menus list the ones besides
    /// `editor_id`, and a folder opens in one with the target `l|editor|<id>`.
    #[serde(default)]
    pub editors: Vec<crate::config::Profile>,
    /// The editor a PR's changed file opens in as a diff (editors.rs `diff_editor`): yours when it
    /// can, else the first installed that can; none when none can.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diff_editor: Option<crate::config::Profile>,
    /// `[apps] new_terminal`: where a terminal Codealong opens goes.
    pub new_terminal: crate::config::NewTerminal,
}

/// A configured account and whether it's ready, for the Accounts dialog.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AccountInfo {
    pub name: String,
    pub harness: String,
    pub home: String,
    pub command: Option<String>,
    /// Where its sessions start unless you pick another (config.toml's `start`).
    pub start: Option<String>,
    /// Auto starts it over accounts that aren't preferred (config.toml's `preferred`).
    pub preferred: bool,
    /// config.toml's `acp`, `monthly` and `per_token`, named as there: Settings saves the
    /// accounts whole, so it hands back what it was given.
    pub acp: Option<String>,
    pub monthly: Option<f64>,
    #[serde(rename = "per_token")]
    pub per_token: bool,
    pub folder_exists: bool,
    pub signed_in: bool,
    pub email: Option<String>,
}

/// Something in the background that isn't working: a file that doesn't read, GitHub not
/// answering. It's said in the windows until it's put right or you dismiss it. What goes wrong
/// with something you just did is never one of these: the window you did it in says so.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Notice {
    /// What it's about (`config`, `github`, `watch`, `todos`, `pr-links`, `previews`,
    /// `reusable`, `held`, `limits`), one of each at most. A window knows one it has dismissed by this
    /// and its title.
    pub key: String,
    /// A few words, for where there's room for no more.
    pub title: String,
    /// The whole of it.
    pub text: String,
    /// When what it tells of ends, where that's known: the hour of a rate limit.
    #[serde(default)]
    pub until: Option<i64>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Board {
    pub generated_at: i64,
    /// Shared by every window and the Dock, independent of view and project filters.
    #[serde(default)]
    pub logo_status: LogoStatus,
    pub apps: AppNames,
    pub projects: Vec<ProjectSummary>,
    pub lanes: Vec<Lane>,
    /// What isn't working in the background, for the windows to say until it's put right.
    pub notices: Vec<Notice>,
    /// True until the first full scan has finished.
    pub loading: bool,
    /// When PRs were last loaded from GitHub.
    pub github_at: Option<i64>,
    /// GitHub's first poll has ended: until then, the PRs here, and the reviews asked of you
    /// among them, are only what's come so far (`GhPoll::polled`, sounds.rs).
    #[serde(default)]
    pub github_polled: bool,
    /// When each repo's PRs here were last as GitHub had them, by slug (`GhPoll::current`): a
    /// repo whose read fails, or is still on its way, keeps its earlier time, and one polled but
    /// never read has none.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub github_current: std::collections::BTreeMap<String, Option<i64>>,
    pub usage: Vec<Usage>,
    /// Whether each account's provider may train on what it sends (training.rs), for the
    /// accounts read so far, in config order. This Mac's own: not sent to other devices.
    #[serde(default)]
    pub training: Vec<crate::training::Training>,
    /// Pinned session keys, in order.
    pub pinned: Vec<String>,
    /// Pinned worktrees (lane ids), in order.
    pub pinned_worktrees: Vec<String>,
    /// Pinned PRs (`owner/name#number`), in order.
    #[serde(default)]
    pub pinned_prs: Vec<String>,
    /// Every PR in the repos the projects cover, newest activity first.
    pub prs: Vec<PrItem>,
    /// Who `gh` is signed in as.
    pub github_login: Option<String>,
    /// Light, dark, or following macOS, from config.toml.
    pub theme: Theme,
    /// How big the board's windows draw, in percent, from config.toml (zoom.rs).
    #[serde(default)]
    pub zoom: u32,
    /// True while `[debug] perf_log` is on, so windows report their timings too.
    pub perf_log: bool,
    /// Every todo, in your order.
    pub todos: Vec<crate::todos::Todo>,
    /// Todo labels, in your order.
    pub labels: Vec<crate::todos::Label>,
    /// The quick panel's shortcut, from config.toml; empty when it's off.
    pub quick_shortcut: String,
    /// The shortcut that goes through what waits on you, from config.toml; empty when it's off.
    pub waiting_shortcut: String,
    /// Improve Codealong's shortcut, from config.toml; empty when it's off.
    #[serde(default)]
    pub improve_shortcut: String,
    /// Where a session that improves Codealong goes: its project and worktree, or its source in
    /// no project yet (improve.rs).
    #[serde(default)]
    pub improve: crate::improve::Improve,
    /// What Codealong asks Jev, from `[jev]` in config.toml.
    pub jev: crate::config::JevCfg,
    /// `[reviews]` from config.toml: whether PRs that wait on your review are reviewed without
    /// being asked (autoreview.rs), on which account, and whether each review is posted.
    #[serde(default)]
    pub reviews: crate::config::ReviewsCfg,
    /// Open todos whose work Jev says looks done, by todo id (todojev.rs).
    pub todo_hints: std::collections::BTreeMap<String, TodoHint>,
    /// `[devices]` from config.toml: your other devices (peers.rs), which follows it from here.
    #[serde(default)]
    pub devices: crate::config::DevicesCfg,
    /// `[[projects]]` as config.toml has them, for putting other devices' worktrees into this
    /// device's projects (peers.rs). The windows get the projects as `projects`.
    #[serde(skip)]
    pub project_rules: Vec<crate::config::ProjectCfg>,
    /// `[limits] when_hit`: whether a session a usage limit stopped is moved by itself
    /// (limits.rs) or only offered a move.
    #[serde(default)]
    pub when_limited: crate::config::WhenLimited,
    /// `[pull_requests] nudge_about`: what a PR whose nudge is on tells its session about
    /// (nudges.rs), which its card says.
    #[serde(default)]
    pub nudge_about: crate::config::NudgeAbout,
    /// Your `[repos."github.com/…"]` tables from config.toml, by key: your settings for particular
    /// repos' pull requests.
    #[serde(default)]
    pub repos: std::collections::BTreeMap<String, crate::config::RepoCfg>,
    /// The team settings read from repos' `.codealong/config.toml` (teamfile.rs), by repo and
    /// branch, each with what it offers you.
    #[serde(default)]
    pub team_files: Vec<crate::teamfile::TeamFile>,
    /// Each repo's settings where they differ from yours everywhere, for what acts on its pull
    /// requests (autoreview.rs, nudges.rs). Not sent: the windows have `repos` and `team_files`.
    #[serde(skip)]
    pub repo_settings: crate::teamfile::RepoSettings,
}


impl Board {
    /// A session and the lane it's listed under as its own, else one it visits (`board::home`).
    pub fn session(&self, key: &str) -> Option<(&Lane, &Session)> {
        crate::board::home(&self.lanes, key)
    }
}

/// Why a todo looks done: how sure Jev is, and what showed it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TodoHint {
    pub p: f64,
    pub why: String,
}

/// A PR in the PRs view, with the worktrees and sessions that worked on it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PrItem {
    /// `owner/name#number`.
    pub key: String,
    pub repo_slug: String,
    pub repo: String,
    pub project: String,
    pub number: u64,
    pub title: String,
    pub url: String,
    pub state: PrState,
    pub author: Option<String>,
    /// You opened it.
    pub mine: bool,
    /// Your review is requested.
    pub review_requested: bool,
    /// Who asked for it, when GitHub says; often not the author.
    #[serde(default)]
    pub requested_by: Option<String>,
    /// When your review was last asked for, when GitHub says: what the review waits on you since.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requested_at: Option<i64>,
    /// Whose reviews it still waits on besides yours (`review_requested`), as its review requests
    /// list them: people by their login, teams by their name. Only an open PR's, and only one the
    /// board reads (github.rs `asked_of_whom`); its card says them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reviewers_asked: Vec<String>,
    /// You snoozed your review from Codealong until then (`board::apply_review_snoozes`): it's
    /// out of Review requested, the floating bar's reviews and the mark until the time comes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snoozed_until: Option<i64>,
    /// Its primary session is told of what happens on it (nudges.rs): turned on for this PR, from
    /// its card or menu or by `codealong pr nudge`, or for every PR of yours by `[pull_requests]
    /// nudge_by_default` unless turned off for this one. Said only while it's open.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub nudge: bool,
    /// The nudge is on through `[pull_requests] nudge_by_default`, not turned on for this PR.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub nudge_by_default: bool,
    /// The session that's the PR's own, of `sessions`, by key: the one picked as its primary,
    /// else the one that pushed to its branch most, the one that opened it winning a tie, else
    /// the one that started first (board.rs `pr_primary`). Only it is nudged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary: Option<String>,
    /// Why `primary` is the one it is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary_by: Option<PrimaryBy>,
    pub head: String,
    pub base: Option<String>,
    pub checks: Checks,
    pub merge: Merge,
    pub additions: u64,
    pub deletions: u64,
    pub changed_files: u64,
    pub created_at: i64,
    pub updated_at: i64,
    pub merged_at: Option<i64>,
    pub closed_at: Option<i64>,
    /// Worktrees that hold the PR: the ones the Worktrees view shows it on.
    pub lanes: Vec<String>,
    /// The sessions that worked on the PR, each with the worktree it's listed under.
    pub sessions: Vec<PrSession>,
    /// The commit its branch is at, when GitHub says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head_oid: Option<String>,
    /// Where Codealong's own review of it stands, once it's had one (autoreview.rs).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_review: Option<AutoReview>,
}

/// Where a round of Codealong's automatic review of a PR stands (autoreview.rs).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ReviewStage {
    /// Its commit is to be reviewed, and no session has started on it yet: it's waiting for its
    /// branch to hold still, or for another review to finish. Only ever shown, never kept.
    Queued,
    /// Its session is reviewing it.
    Reviewing,
    /// Reviewed, and posted on the PR as a comment.
    Posted,
    /// Reviewed, and put in a draft review on your account, for you to finish.
    Drafted,
    /// Reviewed, and left in its session: config.toml says not to post.
    Kept,
    /// Its agent said it couldn't review it.
    Skipped,
    /// It didn't get done.
    Failed,
}

/// Codealong's automatic review of a PR, for its card: the latest round.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AutoReview {
    pub stage: ReviewStage,
    /// The commit it reviews, as GitHub names it.
    pub head: String,
    /// The reviewer's part in it, with its session.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parts: Vec<ReviewPart>,
    /// When the round began, or ended once it has.
    pub at: i64,
    /// The latest review it posted, by its link: this round's, else an earlier one's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    /// Why it was skipped or didn't get done.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// How many rounds the PR has had, this one with them.
    pub rounds: usize,
    /// The PR has new commits since this round's: another follows while it waits on your review.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub behind: bool,
    /// Its review of the PR's commit isn't done yet, so the PR is held out of what waits on you
    /// (`PrItem::waits_on_review`) until it is: you review it once the agent has.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub holds: bool,
    /// It's a review of a PR of yours (`[reviews.own]`), which goes back to the session that
    /// opened it rather than to GitHub.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub own: bool,
    /// The session this round's review was sent to, and when.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sent_at: Option<i64>,
    /// The session it goes back to, which can be sent it now: it runs here and doesn't wait on
    /// you, for the Review tab's Send.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sendable: Option<String>,
    /// What this round's agent would do with the PR, which its windows suggest.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggests: Option<crate::github::Verdict>,
    /// What its review was submitted as yours as (`[reviews] submit`), and when.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub submitted: Option<crate::github::Verdict>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub submitted_at: Option<i64>,
}

/// Where the reviewer's part in a round stands (autoreview.rs).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PartState {
    /// Its session is reviewing.
    Reviewing,
    /// Its review is written.
    Done,
    /// Its agent said it couldn't review it.
    Skipped,
    /// It didn't get done.
    Failed,
}

/// The reviewer's part in a round, for a PR's card.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ReviewPart {
    /// The reviewer, by its account and model (`claude opus`), or `auto`.
    pub who: String,
    /// Its session, once known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
    pub state: PartState,
    /// Why it was skipped or didn't get done.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

impl PrItem {
    /// It's open, your review is asked for, you haven't snoozed it, and Codealong's agent isn't
    /// still reviewing it (`AutoReview::holds`): what the floating bar lists and the mark says
    /// needs you.
    pub fn waits_on_review(&self) -> bool {
        self.review_requested && self.state.is_open() && self.snoozed_until.is_none() && !self.auto_review.as_ref().is_some_and(|a| a.holds)
    }
}

/// A session that worked on a PR.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PrSession {
    pub key: String,
    /// The lane it's listed under: its own, or the worktree it made or worked in.
    pub lane: String,
    /// How often its records show it pushed to the PR's branch before the PR ended.
    #[serde(default)]
    pub pushes: u32,
}

/// Why a PR's primary session is the one it is (board.rs `pr_primary`).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PrimaryBy {
    /// It was picked (`codealong pr nudge --session`, the PR's menu).
    Picked,
    /// It pushed to the PR's branch more often than the session that opened it, or, with no
    /// session known to have opened it, more often than the rest.
    Pushed,
    /// It opened the PR, and no other session pushed to it more.
    Opened,
    /// No session's records show it opened the PR or pushed to it: it started first.
    Started,
}
