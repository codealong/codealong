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
    pub warnings: Vec<String>,
    /// True until the first full scan has finished.
    pub loading: bool,
    /// When PRs were last loaded from GitHub.
    pub github_at: Option<i64>,
    pub usage: Vec<Usage>,
    /// Pinned session keys, in order.
    pub pinned: Vec<String>,
    /// Pinned worktrees (lane ids), in order.
    pub pinned_worktrees: Vec<String>,
    /// Every PR in the repos the projects cover, newest activity first.
    pub prs: Vec<PrItem>,
    /// Who `gh` is signed in as.
    pub github_login: Option<String>,
    /// Light, dark, or following macOS, from config.toml.
    pub theme: Theme,
    /// True while `[debug] perf_log` is on, so windows report their timings too.
    pub perf_log: bool,
    /// Every todo, in your order.
    pub todos: Vec<crate::todos::Todo>,
    /// Todo labels, in your order.
    pub labels: Vec<crate::todos::Label>,
    /// The quick panel's shortcut, from config.toml; empty when it's off.
    pub quick_shortcut: String,
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

/// A session that worked on a PR.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PrSession {
    pub key: String,
    /// The lane it's listed under: its own, or the worktree it made or worked in.
    pub lane: String,
}
