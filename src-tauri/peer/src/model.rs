//! The worktrees and sessions a board shows, as every device sees them: the engine composes them
//! (board.rs), and a device's feed carries them to the devices it's linked with (feed.rs). Field
//! names serialize in camelCase to match `src/types.ts`. The board itself, and what only the Mac
//! app shows, are in the engine's model.rs.

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Harness {
    Claude,
    Codex,
    Opencode,
    Antigravity,
}

impl Harness {
    /// The harness an account's config names; anything unknown reads as Claude Code, which
    /// `config::check_accounts` never lets be saved.
    pub fn from_config(s: &str) -> Harness {
        match s {
            "codex" => Harness::Codex,
            "opencode" => Harness::Opencode,
            "antigravity" => Harness::Antigravity,
            _ => Harness::Claude,
        }
    }

    /// The harness as the UI names it.
    pub fn word(self) -> &'static str {
        match self {
            Harness::Claude => "Claude Code",
            Harness::Codex => "Codex",
            Harness::Opencode => "OpenCode",
            Harness::Antigravity => "Antigravity",
        }
    }
}

/// Where a session runs: the configured terminal app, or the harness's own app.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Surface {
    Cli,
    App,
}

/// Session states, in priority order.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
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
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Bay {
    Needs,
    Working,
    Unread,
    Review,
    Idle,
    Merged,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "kebab-case")]
pub enum Origin {
    Main,
    Script,
    Codex,
    /// In a `.claude/worktrees/` folder: made by `claude -w` or the Claude app.
    ClaudeApp,
    /// Inside a Claude session's scratchpad: made by that session.
    Scratchpad,
    Temp,
    Folder,
    Other,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
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
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
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

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    /// `claude:claude2:<id>`, `codex:codex:<id>`, `opencode:opencode:<id>` or
    /// `antigravity:antigravity:<id>`.
    pub key: String,
    pub id: String,
    pub short_id: String,
    pub harness: Harness,
    /// The account name as you type it: claude, claude2, codex, opencode, antigravity.
    pub account: String,
    pub surface: Surface,
    pub status: Status,
    /// A process is running this session right now.
    pub live: bool,
    pub pid: Option<u32>,
    pub title: String,
    /// Where it was started. Resuming and attaching happen here.
    pub cwd: String,
    /// Where its latest commands ran and edits landed, oldest first. It decides the session's
    /// lane (`board::session_seed`), so the UI doesn't need it.
    #[serde(skip)]
    pub work: Vec<String>,
    /// The end of its last reply, for Jev (jev.rs): where a question for you or a summary is.
    #[serde(skip)]
    pub reply_tail: Option<String>,
    /// The first prompt you gave it, for Jev.
    #[serde(skip)]
    pub first_prompt: Option<String>,
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
    /// The target that opens this session where it lives (focus, resume, Claude app, Codex,
    /// Antigravity).
    pub open_target: String,
    /// Why a session from another lane shows here: "made this worktree" or "worked here".
    /// Visitors don't count toward this lane's bay.
    pub visitor: Option<String>,
    /// This session's own one sentence, for rows in the agent bays.
    pub headline: String,
    pub detail: Option<String>,
    /// The branch it worked on: Claude's latest `gitBranch`, Codex's thread metadata.
    pub branch: Option<String>,
    /// Archived: from Codealong, in Codex, in OpenCode, or in the Claude app. Kept last in its
    /// lane and left out of the lane's status.
    pub archived: bool,
    /// A background job has no terminal tab; this opens it in a new one (`claude attach <job>`).
    pub attach_command: Option<String>,
    /// Set when the session says it's busy but nothing has happened for a long while: it's stuck
    /// mid-turn, usually on a command waiting for input. When it went quiet.
    pub stalled_since: Option<i64>,
    /// Set on a Claude session whose turn ended with a command left running in the background
    /// (registry status `shell`): the command, or empty when it couldn't be read.
    #[serde(skip)]
    pub background: Option<String>,
    /// Jev's read of how its turn ended (insights.rs), when `[jev] sessions` is on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub insight: Option<SessionInsight>,
    /// Needs you only because its last reply asks you something (`board::apply_insight`): its
    /// turn ended and it sits at its prompt, so what you type is the answer. A session waiting on
    /// a permission prompt isn't one: what's typed there would answer the prompt.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub asks: bool,
    /// Codealong runs it itself, over the Agent Client Protocol (acp.rs): its panel sends to it
    /// and answers what it asks, and opening it shows the panel.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hosted: bool,
    /// Snoozed from Codealong until then (`board::apply_snooze`): the floating bar and the mark
    /// leave it out of Needs you and Your turn until the time comes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snoozed_until: Option<i64>,
}

/// Jev's read of a session (insights.rs). Each part is there only when it was asked: how a turn
/// ended when one has, the command's risk while it waits on you.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionInsight {
    /// How likely it is that the reply ends asking you something.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asks_you: Option<f64>,
    /// How far the work got: landed, pushed, committed, uncommitted, blocked, none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub landing: Option<String>,
    /// How likely it is that what you asked for is done.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished: Option<f64>,
    /// How risky the command waiting on you looks, 0 (reads only) to 3 (destructive).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub risk: Option<f64>,
    /// How likely it is that the command a quiet session runs waits for typing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub waits_for_input: Option<f64>,
    /// What a command left running in the background does: polling, job, server or stalled.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background_p: Option<f64>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Checks {
    pub passed: u32,
    pub failed: u32,
    pub pending: u32,
    pub skipped: u32,
    /// Names of failing checks.
    pub failing: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
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

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
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

#[derive(Serialize, Deserialize, Clone, Debug)]
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

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Lane {
    /// The worktree path, which is also its identity.
    pub id: String,
    pub project: String,
    pub repo: String,
    /// `owner/name` on GitHub.
    pub repo_slug: Option<String>,
    /// What GitHub calls the repo now, when that isn't `repo_slug`: the repo or its owner was
    /// renamed, and the remote still has the old name, which GitHub redirects.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_slug_now: Option<String>,
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
    /// Jev reads every session here as finished, and nothing is running (`[jev] sessions`): a
    /// worktree that may be ready to clean up.
    #[serde(default)]
    pub looks_finished: bool,
    pub targets: Vec<Target>,
    pub default_target: Option<String>,
    /// The device it's on, when that's another of your devices (peers.rs); none for this
    /// device's own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device: Option<DeviceTag>,
}

/// Which of your other devices a lane is on, as the windows mark it: its name, its short id, the
/// color you gave it (or the one its key picks), and whether it's connected now.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DeviceTag {
    pub name: String,
    pub short: String,
    /// One of the label colors: gray, red, pink, purple, indigo, blue, teal, green.
    pub color: String,
    pub online: bool,
    pub last_seen: Option<i64>,
}

/// How close a limit is: the usage endpoint's own word, or ours from the percentage.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Normal,
    Warning,
    Critical,
}

/// One usage limit: the 5-hour session, the week, or a per-model week such as Fable's.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
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

/// An account's plan limits, in config order on the board.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
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
    /// Its `preferred` setting: Auto starts it over accounts without one while it has room.
    #[serde(default)]
    pub preferred: bool,
    /// The usage endpoint asked us to slow down.
    #[serde(skip)]
    pub rate_limited: bool,
}
