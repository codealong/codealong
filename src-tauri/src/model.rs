//! The board as the UI sees it. Field names serialize in camelCase to match `src/types.ts`.

use serde::Serialize;

use crate::config::Theme;

#[derive(Serialize, Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Harness {
    Claude,
    Codex,
}

/// Where a session runs: the configured terminal app, or the harness's own app.
#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Surface {
    Cli,
    App,
}

/// Session states, in priority order.
#[derive(Serialize, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Needs,
    Error,
    Working,
    Unread,
    Idle,
    Stopped,
}

/// Board bays, in board order.
#[derive(Serialize, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Bay {
    Needs,
    Working,
    Unread,
    Review,
    Idle,
    Merged,
}

#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "kebab-case")]
pub enum Origin {
    Main,
    Script,
    Codex,
    ClaudeApp,
    /// Inside a Claude session's scratchpad: made by that session.
    Scratchpad,
    Temp,
    Folder,
    Other,
}

#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum PrState {
    Draft,
    Open,
    Approved,
    Changes,
    Merged,
    Closed,
}

impl PrState {
    pub fn is_open(self) -> bool {
        matches!(self, PrState::Draft | PrState::Open | PrState::Approved | PrState::Changes)
    }
}

/// Whether GitHub would merge an open PR now, as its merge box says (`mergeStateStatus`).
#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum Merge {
    /// Nothing stands in the way: the reviews the repo requires, passing checks, no conflicts.
    Clean,
    /// The branch conflicts with its base.
    Conflicts,
    /// The repo wants the branch up to date with its base, and it's behind.
    Behind,
    /// Held back by a review or check the repo requires, a rule, or being a draft.
    Blocked,
    /// It could merge, but checks that aren't required didn't pass.
    Unstable,
    /// GitHub hasn't worked it out yet (asking starts that), or the PR is merged or closed.
    #[default]
    Unknown,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    /// `claude:claude2:<id>` or `codex:codex:<id>`.
    pub key: String,
    pub id: String,
    pub short_id: String,
    pub harness: Harness,
    /// The account name as you type it: claude, claude2, codex.
    pub account: String,
    pub surface: Surface,
    pub status: Status,
    /// A process is running this session right now.
    pub live: bool,
    pub pid: Option<u32>,
    pub title: String,
    pub cwd: String,
    /// The registry name, which Claude Code also puts in the terminal title.
    pub name: Option<String>,
    pub started_at: Option<i64>,
    /// When the current status began (turn start while Working).
    pub status_since: Option<i64>,
    pub last_activity: i64,
    /// When you last sent it a prompt, or when it started if its log doesn't say. Unlike its
    /// last activity this holds still while it works, so lists sort by it.
    pub last_prompt_at: i64,
    /// What the session is doing or asking. Backtick spans render as code.
    pub excerpt: Option<String>,
    pub resume_command: Option<String>,
    /// The Claude app is signed into this session's account (`~/.claude`), so it can open it.
    pub app_account: bool,
    /// The target that opens this session where it lives (focus, resume, Claude app, Codex).
    pub open_target: String,
    /// Why a session from another lane shows here: "made this worktree" or "worked here".
    /// Visitors don't count toward this lane's bay.
    pub visitor: Option<String>,
    /// This session's own one sentence, for rows in the agent bays.
    pub headline: String,
    pub detail: Option<String>,
    /// The branch it worked on: Claude's latest `gitBranch`, Codex's thread metadata.
    pub branch: Option<String>,
    /// Archived: from Codealong, in Codex, or in the Claude app. Kept last in its lane and left
    /// out of the lane's status.
    pub archived: bool,
    /// A background job has no terminal tab; this opens it in a new one (`claude attach <job>`).
    pub attach_command: Option<String>,
    /// Set when the session says it's busy but nothing has happened for a long while: it's stuck
    /// mid-turn, usually on a command waiting for input. When it went quiet.
    pub stalled_since: Option<i64>,
}

#[derive(Serialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Checks {
    pub passed: u32,
    pub failed: u32,
    pub pending: u32,
    pub skipped: u32,
    /// Names of failing checks.
    pub failing: Vec<String>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Pr {
    pub number: u64,
    pub title: String,
    pub url: String,
    pub state: PrState,
    pub checks: Checks,
    pub merge: Merge,
    pub updated_at: i64,
    pub merged_at: Option<i64>,
    pub closed_at: Option<i64>,
}

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct GitInfo {
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    /// Uncommitted files, untracked included.
    pub dirty: u32,
    pub last_commit: Option<String>,
    pub last_commit_at: Option<i64>,
    /// Lines added and removed against the merge base with the default branch.
    pub added: Option<u32>,
    pub removed: Option<u32>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Target {
    pub id: String,
    pub icon: String,
    pub label: String,
    pub detail: Option<String>,
    pub kbd: Option<Vec<String>>,
    pub disabled: bool,
    /// `session`, `lane` or `pr`.
    pub group: String,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Lane {
    /// The worktree path, which is also its identity.
    pub id: String,
    pub project: String,
    pub repo: String,
    /// `owner/name` on GitHub.
    pub repo_slug: Option<String>,
    pub path: String,
    pub callsign: String,
    pub origin: Origin,
    pub branch: Option<String>,
    /// Short commit when the HEAD is detached.
    pub detached: Option<String>,
    pub bay: Bay,
    pub glyph: Status,
    pub parked: bool,
    /// Elsewhere lanes stay off the board unless they need you.
    pub hidden: bool,
    /// One sentence; backtick spans render as code, `{ago:<ms>}` and `{for:<ms>}` as live times.
    pub headline: String,
    pub detail: Option<String>,
    pub sessions: Vec<Session>,
    pub pr: Option<Pr>,
    pub git: Option<GitInfo>,
    pub last_activity: i64,
    pub targets: Vec<Target>,
    pub default_target: Option<String>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RepoSummary {
    pub name: String,
    pub lanes: usize,
    pub folder: bool,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSummary {
    pub name: String,
    pub lanes: usize,
    pub repos: Vec<RepoSummary>,
}

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct AppNames {
    pub terminal: String,
    pub terminal_id: String,
    pub editor: String,
    pub editor_id: String,
}

/// How close a limit is: the usage endpoint's own word, or ours from the percentage.
#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Normal,
    Warning,
    Critical,
}

/// One usage limit: the 5-hour session, the week, or a per-model week such as Fable's.
#[derive(Serialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Meter {
    /// `session`, `week`, or `model:<name>`.
    pub key: String,
    /// Short, for the bar: `5h`, `Week`, `Fable`.
    pub label: String,
    /// Long, for the tooltip: `5-hour session`, `Weekly, Fable only`.
    pub window: String,
    pub percent: f64,
    pub resets_at: Option<i64>,
    pub severity: Severity,
    /// Set on a per-model limit.
    pub model: Option<String>,
}

/// A configured account and whether it's ready, for the Accounts dialog.
#[derive(Serialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AccountInfo {
    pub name: String,
    pub harness: String,
    pub home: String,
    pub command: Option<String>,
    pub folder_exists: bool,
    pub signed_in: bool,
    pub email: Option<String>,
}

/// An account's plan limits, in config order on the board.
#[derive(Serialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub account: String,
    pub harness: Harness,
    /// `Max 20x`, `Pro`.
    pub plan: Option<String>,
    pub meters: Vec<Meter>,
    /// When the numbers were read: fetched (Claude), or reported by the last turn (Codex).
    pub as_of: Option<i64>,
    /// Why there are no numbers, in words.
    pub error: Option<String>,
    /// The fix for `error` is signing in again.
    pub needs_sign_in: bool,
    /// True until the first read finishes.
    pub loading: bool,
    /// The usage endpoint asked us to slow down.
    #[serde(skip)]
    pub rate_limited: bool,
}

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Board {
    pub generated_at: i64,
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
}

/// A PR in the PRs view, with the worktrees and sessions that worked on it.
#[derive(Serialize, Clone, Debug, PartialEq)]
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
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PrSession {
    pub key: String,
    /// The lane it's listed under: its own, or the worktree it made or worked in.
    pub lane: String,
}
