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
    pub usage: Vec<Usage>,
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
    /// What Codealong asks Jev, from `[jev]` in config.toml.
    pub jev: crate::config::JevCfg,
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
    /// You snoozed your review from Codealong until then (`board::apply_review_snoozes`): it's
    /// out of Review requested, the floating bar's reviews and the mark until the time comes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snoozed_until: Option<i64>,
    /// Its sessions are told of what happens on it (nudges.rs): turned on for this PR, from its
    /// card or menu or by `codealong pr nudge`, and said only while it's open.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub nudge: bool,
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
}

impl PrItem {
    /// It's open, your review is asked for, and you haven't snoozed it: what the floating bar
    /// lists and the mark says needs you.
    pub fn waits_on_review(&self) -> bool {
        self.review_requested && self.state.is_open() && self.snoozed_until.is_none()
    }
}

/// A session that worked on a PR.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PrSession {
    pub key: String,
    /// The lane it's listed under: its own, or the worktree it made or worked in.
    pub lane: String,
}
