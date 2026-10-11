//! `config.toml`: apps, theme, accounts, projects and scan roots. Codealong writes it on first
//! run from what it finds. After that you edit it, or Settings does: its form sets each value
//! in place, leaving the rest of the file, comments included, as you wrote it; its config.toml
//! tab saves the text as typed, once it parses.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};

use crate::util::{expand, home};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Config {
    #[serde(default = "default_scan")]
    pub scan: Vec<String>,
    #[serde(default)]
    pub apps: AppsCfg,
    #[serde(default)]
    pub appearance: AppearanceCfg,
    #[serde(default)]
    pub shortcuts: ShortcutsCfg,
    #[serde(default)]
    pub accounts: Vec<AccountCfg>,
    #[serde(default)]
    pub projects: Vec<ProjectCfg>,
    #[serde(default)]
    pub jev: JevCfg,
    /// `[router]` from before `[jev]`: its `enabled` turns the quick panel's suggestions on.
    /// Read, folded into `jev`, and never written back.
    #[serde(default, skip_serializing)]
    pub router: Option<LegacyRouter>,
    #[serde(default)]
    pub search: SearchCfg,
    #[serde(default)]
    pub devices: DevicesCfg,
    #[serde(default)]
    pub updates: UpdatesCfg,
    #[serde(default)]
    pub limits: LimitsCfg,
    #[serde(default)]
    pub sessions: SessionsCfg,
    #[serde(default)]
    pub reviews: ReviewsCfg,
    #[serde(default)]
    pub pull_requests: PullRequestsCfg,
    #[serde(default)]
    pub debug: DebugCfg,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            scan: default_scan(),
            apps: AppsCfg::default(),
            appearance: AppearanceCfg::default(),
            shortcuts: ShortcutsCfg::default(),
            accounts: Vec::new(),
            projects: Vec::new(),
            jev: JevCfg::default(),
            router: None,
            search: SearchCfg::default(),
            devices: DevicesCfg::default(),
            updates: UpdatesCfg::default(),
            limits: LimitsCfg::default(),
            sessions: SessionsCfg::default(),
            reviews: ReviewsCfg::default(),
            pull_requests: PullRequestsCfg::default(),
            debug: DebugCfg::default(),
        }
    }
}

/// What Codealong asks TypeSafe's Jev (jev.rs), feature by feature. Everything is off until you
/// turn it on; the API key lives in the login keychain, not here.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JevCfg {
    /// The model every request names: `jev-latest`, or a version such as `jev-1.13.0`.
    #[serde(default = "default_jev_model")]
    pub model: String,
    /// The quick panel suggests where what you type goes, warns about work already under way and
    /// suggests a model for it.
    #[serde(default)]
    pub quick_panel: bool,
    /// Reads how each session's turn ended: a question for you, stuck or just slow, how far the
    /// work landed, whether it's finished, how risky a waiting command looks.
    #[serde(default)]
    pub sessions: bool,
    /// Links new sessions to the todo they're for, says when a todo looks done, and suggests a
    /// new todo's project, labels and what it waits on.
    #[serde(default)]
    pub todos: bool,
    /// Marks the review comments on a pull request that ask for a change.
    #[serde(default)]
    pub pull_requests: bool,
}

impl Default for JevCfg {
    fn default() -> Self {
        JevCfg { model: default_jev_model(), quick_panel: false, sessions: false, todos: false, pull_requests: false }
    }
}

impl JevCfg {
    pub fn any(&self) -> bool {
        self.quick_panel || self.sessions || self.todos || self.pull_requests
    }
}

fn default_jev_model() -> String {
    "jev-latest".to_string()
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct LegacyRouter {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub model: Option<String>,
}

/// Settings from config.toml's text, with an old `[router]` folded into `[jev]`.
fn from_text(text: &str) -> std::result::Result<Config, toml::de::Error> {
    let mut cfg: Config = toml::from_str(text)?;
    if let Some(first) = cfg.reviews.reviewers.first().cloned() {
        let doc = text.parse::<toml_edit::DocumentMut>().ok();
        let has = |key: &str| doc.as_ref().is_some_and(|d| d.get("reviews").and_then(|r| r.get(key)).is_some());
        if !has("account") {
            cfg.reviews.account = first.account;
        }
        if !has("model") {
            cfg.reviews.model = first.model.unwrap_or_default();
        }
    }
    cfg.reviews.reviewers.clear();
    if cfg.reviews.auto_approve && cfg.reviews.submit == ReviewSubmit::Never {
        let doc = text.parse::<toml_edit::DocumentMut>().ok();
        if !doc.as_ref().is_some_and(|d| d.get("reviews").and_then(|r| r.get("submit")).is_some()) {
            cfg.reviews.submit = ReviewSubmit::Approve;
        }
    }
    cfg.reviews.auto_approve = false;
    if let Some(old) = cfg.router.take() {
        let has_jev = text.parse::<toml_edit::DocumentMut>().is_ok_and(|d| d.get("jev").is_some());
        if !has_jev {
            cfg.jev.quick_panel = old.enabled;
            if let Some(m) = old.model.filter(|m| !m.trim().is_empty()) {
                cfg.jev.model = m;
            }
        }
    }
    Ok(cfg)
}

/// Searching what was said in sessions (search.rs), not just their titles.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SearchCfg {
    /// Sessions active in this many days have their prompts and replies searched; 0 turns it
    /// off. The board goes back 14 days, so that's the most.
    #[serde(default = "default_search_days")]
    pub days: u32,
}

impl Default for SearchCfg {
    fn default() -> Self {
        SearchCfg { days: default_search_days() }
    }
}

pub const MAX_SEARCH_DAYS: u32 = 14;

fn default_search_days() -> u32 {
    MAX_SEARCH_DAYS
}

/// Your other devices (peers.rs): Codealong on each finds the others, shows their sessions here,
/// and reads or acts on them on request. Off until you turn it on.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DevicesCfg {
    #[serde(default)]
    pub enabled: bool,
    /// This device's name on your other devices; empty is the computer's name.
    #[serde(default)]
    pub name: String,
    /// Find your devices on your tailnet, when Tailscale runs here.
    #[serde(default = "yes")]
    pub tailscale: bool,
    /// Reach linked devices that aren't on your tailnet, through iroh (encrypted end to end; its
    /// relays only pass packets along).
    #[serde(default = "yes")]
    pub iroh: bool,
    /// Let your linked devices send prompts to sessions here, answer what they wait on you to
    /// allow, and start sessions. Reading and searching don't need it.
    #[serde(default)]
    pub remote_control: bool,
    /// The TCP port Codealong listens on, on this device's tailnet address only.
    #[serde(default = "default_devices_port")]
    pub port: u16,
}

impl Default for DevicesCfg {
    fn default() -> Self {
        DevicesCfg { enabled: false, name: String::new(), tailscale: true, iroh: true, remote_control: false, port: default_devices_port() }
    }
}

pub const DEFAULT_DEVICES_PORT: u16 = 47471;

fn default_devices_port() -> u16 {
    DEFAULT_DEVICES_PORT
}

fn yes() -> bool {
    true
}

/// What Codealong does about a session its account's usage limit stopped (`start::limit_moves`).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct LimitsCfg {
    #[serde(default)]
    pub when_hit: WhenLimited,
}

/// `suggest`: the session says which account has room and offers to move it there. `move`:
/// Codealong moves it there itself, as soon as it stops, and it carries on.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum WhenLimited {
    #[default]
    Suggest,
    Move,
}

impl WhenLimited {
    pub fn as_str(self) -> &'static str {
        match self {
            WhenLimited::Suggest => "suggest",
            WhenLimited::Move => "move",
        }
    }
}

/// What Codealong does for sessions by itself.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionsCfg {
    /// A session whose turn ended because its agent couldn't reach its API (the Mac slept, or
    /// the network dropped) is told to carry on once the API can be reached again (reconnect.rs).
    #[serde(default = "yes")]
    pub continue_after_network_error: bool,
}

impl Default for SessionsCfg {
    fn default() -> Self {
        SessionsCfg { continue_after_network_error: true }
    }
}

/// Pull requests that wait on your review, reviewed by an agent of yours without being asked
/// (autoreview.rs). Off until you turn it on.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReviewsCfg {
    /// Start a review session for each open PR that waits on your review, and have it review the
    /// PR again when new commits come in while it still waits.
    #[serde(default)]
    pub auto: bool,
    /// The account the reviews run on; empty for the one Auto would start. Apart from the
    /// account new sessions start on.
    #[serde(default)]
    pub account: String,
    /// The model they start on, as a start's `--model` takes it; empty for the account's own.
    #[serde(default)]
    pub model: String,
    /// What the reviewer is told to do, in place of Codealong's own instructions
    /// (autoreview.rs `INSTRUCTIONS`): what to look for and how, and whether to start subagents
    /// of its own. Empty for Codealong's. Which PR, where it's written out, that nothing is to
    /// change and how the last message is laid out are said either way.
    #[serde(default)]
    pub prompt: String,
    /// Where each review goes once it's done.
    #[serde(default)]
    pub post: ReviewPost,
    /// Which of the agents' verdicts are submitted as your review once its review is posted.
    #[serde(default)]
    pub submit: ReviewSubmit,
    /// `auto_approve = true`, from before `submit` (2026-10-11): `submit = "approve"` where the
    /// file has no `submit` (`from_text`). Read and never written.
    #[serde(default, skip_serializing)]
    pub auto_approve: bool,
    /// `reviewers = [{ account, model }]`, from the day several could review (2026-10-09): the
    /// first becomes `account` and `model` where those aren't set (`from_text`). Read and never
    /// written; `combine` from then is dropped as unknown.
    #[serde(default, skip_serializing)]
    pub reviewers: Vec<ReviewerCfg>,
    /// Your own pull requests, `[reviews.own]`.
    #[serde(default)]
    pub own: OwnReviewsCfg,
}

impl Default for ReviewsCfg {
    fn default() -> Self {
        ReviewsCfg { auto: false, account: String::new(), model: String::new(), prompt: String::new(), post: ReviewPost::Draft, submit: ReviewSubmit::Never, auto_approve: false, reviewers: Vec::new(), own: OwnReviewsCfg::default() }
    }
}

/// The pull requests your sessions open, each reviewed by an agent on another model or provider
/// than the session that opened it, its review kept here and handed back to that session
/// (autoreview.rs). Nothing goes on GitHub. Off until you turn it on.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OwnReviewsCfg {
    /// Review each open PR of yours that a session here opened, and again when new commits come in.
    #[serde(default)]
    pub auto: bool,
    /// The account the reviews run on; empty for the one Auto would start among the accounts of
    /// another harness than the PR's session, else Auto's.
    #[serde(default)]
    pub account: String,
    /// The model they start on; empty for the account's own.
    #[serde(default)]
    pub model: String,
    /// What the reviewer is told to do, as `[reviews] prompt` is; empty for Codealong's own.
    #[serde(default)]
    pub prompt: String,
    /// Send each review to the session that opened the PR, once it runs here and doesn't wait on
    /// you; else it's kept here, to send from the PR's Review tab.
    #[serde(default = "yes")]
    pub send: bool,
}

impl Default for OwnReviewsCfg {
    fn default() -> Self {
        OwnReviewsCfg { auto: false, account: String::new(), model: String::new(), prompt: String::new(), send: true }
    }
}

/// A reviewer as `[reviews] reviewers` named one for a day.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewerCfg {
    pub account: String,
    #[serde(default)]
    pub model: Option<String>,
}

/// Where an automatic review goes (autoreview.rs). `draft`: a review on your GitHub account left
/// as a draft, with its comments on the lines they're about, which only you see until you finish
/// it on GitHub. `comment`: a comment on the PR from your account, each line it's about linked.
/// `none`: nowhere; it stays in its session for you to read.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ReviewPost {
    #[default]
    Draft,
    Comment,
    None,
}

impl ReviewPost {
    pub fn as_str(self) -> &'static str {
        match self {
            ReviewPost::Draft => "draft",
            ReviewPost::Comment => "comment",
            ReviewPost::None => "none",
        }
    }
}

/// Which of an automatic review's verdicts are submitted as your review once it's posted
/// (autoreview.rs `to_submit`). `never`: each is left for you. `approve`: a PR its agent would
/// approve is approved. `approve-or-request-changes`: that, and one it would ask for changes on gets
/// a request for changes from you. `approve-or-comment`: that, but the request for changes goes
/// as a comment, which doesn't hold the PR back. An agent that can't tell (`Verdict: comment`)
/// leaves it to you whatever this says.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ReviewSubmit {
    #[default]
    Never,
    Approve,
    ApproveOrRequestChanges,
    ApproveOrComment,
}

impl ReviewSubmit {
    pub fn as_str(self) -> &'static str {
        match self {
            ReviewSubmit::Never => "never",
            ReviewSubmit::Approve => "approve",
            ReviewSubmit::ApproveOrRequestChanges => "approve-or-request-changes",
            ReviewSubmit::ApproveOrComment => "approve-or-comment",
        }
    }
}

/// Your PRs (nudges.rs). A PR's own switch, on its card, its menu or `codealong pr nudge`, wins
/// over this either way.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct PullRequestsCfg {
    /// Every open PR of yours tells its primary session, the one that opened it, what happens on
    /// it. Off, a PR tells nobody until turned on for it.
    #[serde(default)]
    pub nudge_by_default: bool,
    /// What a PR whose nudge is on tells its session about.
    #[serde(default)]
    pub nudge_about: NudgeAbout,
}

/// What a nudge tells a PR's session about (nudges.rs `NudgeAbout::keep`). `everything`: reviews,
/// comments, an approval, a merge or close, and failing checks. `feedback`: only what asks for
/// work: comments, and reviews that ask for changes or say something, never an approval, a merge
/// or a close. `feedback-and-checks`: that, and failing checks.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum NudgeAbout {
    #[default]
    Everything,
    Feedback,
    FeedbackAndChecks,
}

impl NudgeAbout {
    pub fn as_str(self) -> &'static str {
        match self {
            NudgeAbout::Everything => "everything",
            NudgeAbout::Feedback => "feedback",
            NudgeAbout::FeedbackAndChecks => "feedback-and-checks",
        }
    }
}

/// Automatic updates (updates.rs). Only release builds update themselves.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UpdatesCfg {
    /// Look for a newer release now and then, and put it in place for the next launch. Asking
    /// (the app menu's Check for updates…) works either way.
    #[serde(default = "yes")]
    pub check: bool,
}

impl Default for UpdatesCfg {
    fn default() -> Self {
        UpdatesCfg { check: true }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct DebugCfg {
    /// Appends timings to ~/Library/Logs/Codealong/perf.log (see perf.rs).
    #[serde(default)]
    pub perf_log: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AppearanceCfg {
    #[serde(default)]
    pub theme: Theme,
    /// How big the board's windows draw, in percent; ⌘= and ⌘- step it (zoom.rs).
    #[serde(default = "default_zoom")]
    pub zoom: u32,
}

impl Default for AppearanceCfg {
    fn default() -> Self {
        AppearanceCfg { theme: Theme::default(), zoom: default_zoom() }
    }
}

fn default_zoom() -> u32 {
    crate::zoom::ACTUAL
}

/// Keyboard shortcuts that work whichever app is in front. An empty one is off.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ShortcutsCfg {
    /// Opens the quick panel: find a session, or start one.
    #[serde(default = "default_quick_panel")]
    pub quick_panel: String,
    /// Opens the first session that waits on you, to answer one after another: in the notch when
    /// the bar is docked there, else in the waiting window.
    #[serde(default = "default_waiting")]
    pub waiting: String,
}

impl Default for ShortcutsCfg {
    fn default() -> Self {
        ShortcutsCfg { quick_panel: default_quick_panel(), waiting: default_waiting() }
    }
}

/// ⌥⌘/, / for search as in many apps. The other / keys are taken: ⌘/ comments out lines in
/// editors, ⌘? searches the Help menu, ⌃/ is undo in shells (terminals send it as ^_), and ⌥/
/// and ⌥⇧/ type ÷ and ¿. Written as Settings records it, so its "Use …" button knows it.
pub const DEFAULT_QUICK_PANEL: &str = "Option+Cmd+/";

fn default_quick_panel() -> String {
    DEFAULT_QUICK_PANEL.to_string()
}

/// ⌥⌘., beside the quick panel's ⌥⌘/ with the same keys held, so the two sit together; no
/// macOS shortcut or common app's uses it. Written as Settings records it.
pub const DEFAULT_WAITING: &str = "Option+Cmd+.";

fn default_waiting() -> String {
    DEFAULT_WAITING.to_string()
}

/// Light or dark. `System` follows macOS, and changes when macOS does.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

impl Theme {
    pub fn as_str(self) -> &'static str {
        match self {
            Theme::System => "system",
            Theme::Light => "light",
            Theme::Dark => "dark",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AppsCfg {
    #[serde(default = "default_terminal")]
    pub terminal: String,
    #[serde(default = "default_editor")]
    pub editor: String,
    #[serde(default)]
    pub new_terminal: NewTerminal,
    /// After bringing a session's terminal forward on another screen, moves the pointer to it
    /// (flash.rs). Off by default: the terminal is outlined either way.
    #[serde(default)]
    pub move_pointer: bool,
}

impl Default for AppsCfg {
    fn default() -> Self {
        AppsCfg { terminal: default_terminal(), editor: default_editor(), new_terminal: NewTerminal::default(), move_pointer: false }
    }
}

/// Where a terminal Codealong opens goes: a window of its own, a tab in the front window (after
/// its worktree's, repo's or project's tabs there, where the terminal orders tabs), or a tab in a
/// window already showing the same worktree, repo or project (a window of its own when none does).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum NewTerminal {
    Window,
    Tab,
    #[default]
    Project,
}

impl NewTerminal {
    pub fn as_str(self) -> &'static str {
        match self {
            NewTerminal::Window => "window",
            NewTerminal::Tab => "tab",
            NewTerminal::Project => "project",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AccountCfg {
    /// What you type, and what chips show: claude, claude2, codex, opencode, antigravity.
    pub name: String,
    /// `claude`, `codex`, `opencode` or `antigravity`.
    pub harness: String,
    pub home: String,
    /// The command that starts this account, used for `<command> --resume <id>`.
    #[serde(default)]
    pub command: Option<String>,
    /// Where a new session on this account starts unless you pick another: `terminal`,
    /// `background`, `claude-app`, `codex-app` or `codealong`. None leaves it to the usual place.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<String>,
    /// Auto starts preferred accounts while one has room for the model, and the others only
    /// when they're all near a limit.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub preferred: bool,
    /// The command that runs this account's Agent Client Protocol adapter, for sessions
    /// Codealong runs itself (acp.rs). None uses the one `codealong install` adds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acp: Option<String>,
}

impl AccountCfg {
    pub fn home_path(&self) -> PathBuf {
        expand(&self.home)
    }
    pub fn command(&self) -> &str {
        self.command.as_deref().unwrap_or(&self.name)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProjectCfg {
    pub name: String,
    /// Repo patterns such as `github.com/northwindai/*`, or exact `github.com/owner/name`.
    #[serde(default)]
    pub repos: Vec<String>,
    /// Folders that aren't repos but belong to the project.
    #[serde(default)]
    pub folders: Vec<String>,
    /// The Chrome profile its worktrees' previews open in (chrome.rs): the name Chrome's
    /// profile picker shows, the account signed in to it, or its folder. None opens them in the
    /// default browser.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chrome_profile: Option<String>,
}

fn default_scan() -> Vec<String> {
    vec!["~/projects".to_string()]
}
fn default_terminal() -> String {
    crate::launch::default_terminal().to_string()
}
fn default_editor() -> String {
    "cursor".to_string()
}

pub fn support_dir() -> PathBuf {
    home().join("Library/Application Support/Codealong")
}

pub fn config_path() -> PathBuf {
    support_dir().join("config.toml")
}

/// Where a session that belongs to none of your projects starts: each in a folder of its own
/// in here (start.rs), all of them in one lane on the board (`engine::chat_roots`).
pub fn chats_dir() -> PathBuf {
    support_dir().join("chats")
}

/// Loads the config, or `None` when there isn't one yet.
pub fn load() -> Result<Option<Config>> {
    read_text()?.map(|text| load_text(&text)).transpose()
}

/// The settings the app runs on, which its engine sets each time it reads config.toml: while the
/// file doesn't parse, the last good ones, as its notice says. None in a process with no app
/// engine (the `codealong` command, `--snapshot`), which reads the file instead.
static RUNNING: std::sync::RwLock<Option<Config>> = std::sync::RwLock::new(None);

/// Says which settings the app runs on (the engine, each time it reads config.toml).
pub fn running_on(cfg: &Config) {
    *RUNNING.write().unwrap_or_else(|e| e.into_inner()) = Some(cfg.clone());
}

/// The settings to act on. In the app, the ones it runs on, so a mistake in config.toml never
/// leaves it half working: everything goes on with the last good settings and the notice says
/// what's wrong. Elsewhere config.toml itself (the defaults when there's none yet), and when it
/// doesn't read or parse, an error that says so and where to look, never the defaults in its
/// place: those have no accounts, and whatever acted on them would act on settings that aren't
/// yours.
pub fn current() -> Result<Config> {
    if let Some(cfg) = RUNNING.read().unwrap_or_else(|e| e.into_inner()).clone() {
        return Ok(cfg);
    }
    from_file(load())
}

/// The settings `load` read from the file: the defaults when there's no file yet, and when it
/// doesn't read or parse, an error that says where to look.
fn from_file(loaded: Result<Option<Config>>) -> Result<Config> {
    match loaded {
        Ok(cfg) => Ok(cfg.unwrap_or_default()),
        Err(e) => Err(anyhow!("{e:#}. `codealong config check` says what's wrong with it.")),
    }
}

/// config.toml's text, or `None` when there isn't one yet.
pub fn read_text() -> Result<Option<String>> {
    let path = config_path();
    if !path.exists() {
        return Ok(None);
    }
    std::fs::read_to_string(&path).map(Some).with_context(|| format!("reading {}", path.display()))
}

/// The settings in config.toml's text, as `load` reads them.
pub fn load_text(text: &str) -> Result<Config> {
    from_text(text).with_context(|| format!("{} has an error", crate::util::tilde(&config_path())))
}

/// The theme config.toml asks for: `system` when there's no file yet or it has an error.
pub fn theme() -> Theme {
    load().ok().flatten().map(|c| c.appearance.theme).unwrap_or_default()
}

/// The zoom config.toml asks for: actual size when there's no file yet or it has an error.
pub fn zoom() -> u32 {
    load().ok().flatten().map_or(crate::zoom::ACTUAL, |c| c.appearance.zoom)
}

/// The shortcuts config.toml asks for: the defaults when there's no file yet or it has an error,
/// as the theme is, since the app registers them before its engine has read the settings.
pub fn shortcuts() -> ShortcutsCfg {
    load().ok().flatten().map(|c| c.shortcuts).unwrap_or_default()
}

/// Whether config.toml lets Codealong look for updates by itself: yes when there's no file yet or
/// it has an error.
pub fn updates_check() -> bool {
    load().ok().flatten().map(|c| c.updates.check).unwrap_or(true)
}

/// Writes a new config.toml, for the first launch. Returns the text written, or the file's own
/// when one appeared meanwhile, which is left as it is.
pub fn write(cfg: &Config) -> Result<String> {
    let path = config_path();
    let _lock = crate::store::lock(&path)?;
    if let Some(there) = crate::store::text(&path)? {
        return Ok(there);
    }
    let text = render(cfg);
    crate::store::write(&path, &text)?;
    Ok(text)
}

/// Changes config.toml's text with `change` and writes it back, keeping everything `change`
/// leaves as written; returns what `change` answers alongside. See `update_at`.
fn update<R>(change: impl FnMut(&str) -> Result<(String, R)>) -> Result<R> {
    update_at(&config_path(), change)
}

/// The file's text as a change starts from: a new file's when there's none yet. One that's
/// there and can't be read (its permissions, text that isn't UTF-8) is an error, never the
/// defaults, which would be written over it with every account and project lost.
fn text_to_change(path: &Path) -> Result<String> {
    Ok(crate::store::text(path)?.unwrap_or_else(|| render(&Config::default())))
}

/// How every change Codealong makes to config.toml is made: the theme and zoom buttons, an Open
/// in menu, Settings, the account list. Each holds the one lock on `config.toml.lock`
/// (`store::lock`) from reading the file to writing it, so two of them, in this process or the
/// `codealong` command, never write over each other's change. An agent or an editor saving the
/// file by hand takes no lock, so the file is read again just before it's replaced, and a
/// change saved meanwhile is changed in turn rather than written over (a file that changes
/// under every try is left alone, with an error); only one saved in the moment between that
/// read and the write can be lost. Written through a temporary file (`store::write`), so the
/// watcher never reads half a file.
fn update_at<R>(path: &Path, mut change: impl FnMut(&str) -> Result<(String, R)>) -> Result<R> {
    let _lock = crate::store::lock(path)?;
    let mut text = text_to_change(path)?;
    for _ in 0..3 {
        let (changed, answer) = change(&text)?;
        let now = text_to_change(path)?;
        if now == text {
            crate::store::write(path, changed)?;
            return Ok(answer);
        }
        text = now;
    }
    Err(anyhow!("{} kept changing while Codealong saved it, so this change wasn't saved; try it again.", crate::util::tilde(path)))
}

pub const HARNESSES: [&str; 5] = ["claude", "codex", "opencode", "antigravity", "cursor"];

/// Why a list of accounts can't be saved, in words.
pub fn check_accounts(accounts: &[AccountCfg]) -> std::result::Result<(), String> {
    let mut names = std::collections::HashSet::new();
    let mut homes = std::collections::HashSet::new();
    for a in accounts {
        if a.name.is_empty() || !a.name.chars().all(|c| c.is_ascii_alphanumeric() || "._-".contains(c)) {
            return Err(format!("\"{}\" can't be an account name. Use letters, numbers, dots, dashes or underscores.", a.name));
        }
        if !HARNESSES.contains(&a.harness.as_str()) {
            return Err(format!("{} needs a harness: claude, codex, opencode, antigravity or cursor.", a.name));
        }
        if a.home.trim().is_empty() {
            return Err(format!("{} needs a config folder.", a.name));
        }
        if !names.insert(a.name.as_str()) {
            return Err(format!("Two accounts are named {}.", a.name));
        }
        if !homes.insert(expand(a.home.trim())) {
            return Err(format!("{} uses a config folder another account already uses.", a.name));
        }
        if let Some(place) = a.start.as_deref() {
            crate::start::check_start(a, place, &crate::util::home())?;
        }
    }
    Ok(())
}

/// Replaces the `[[accounts]]` tables in config.toml and keeps everything else as written.
pub fn save_accounts(accounts: &[AccountCfg]) -> Result<()> {
    check_accounts(accounts).map_err(anyhow::Error::msg)?;
    update(|text| Ok((with_accounts(text, accounts)?, ())))
}

/// Sets `start` on the named accounts in config.toml and keeps everything else as written: a
/// start from the quick panel keeps its place there (`start::keep_place`).
pub fn save_start(names: &[String], place: &str) -> Result<()> {
    update(|text| Ok((with_start(text, names, place)?, ())))
}

/// `text` with `start` set on the named `[[accounts]]` tables; a comment after the value stays.
pub fn with_start(text: &str, names: &[String], place: &str) -> Result<String> {
    let mut doc: toml_edit::DocumentMut = text.parse().context("config.toml has an error; fix it before saving where sessions start")?;
    if let Some(tables) = doc.get_mut("accounts").and_then(toml_edit::Item::as_array_of_tables_mut) {
        for t in tables.iter_mut().filter(|t| t.get("name").and_then(toml_edit::Item::as_str).is_some_and(|n| names.iter().any(|x| x == n))) {
            set_value(&mut t["start"], toml_edit::Value::from(place));
        }
    }
    Ok(doc.to_string())
}

/// `text` with its `[[accounts]]` tables replaced. The comment above the first one stays.
pub fn with_accounts(text: &str, accounts: &[AccountCfg]) -> Result<String> {
    let mut doc: toml_edit::DocumentMut = text.parse().context("config.toml has an error; fix it before saving accounts")?;
    set_accounts(&mut doc, accounts);
    Ok(doc.to_string())
}

fn set_accounts(doc: &mut toml_edit::DocumentMut, accounts: &[AccountCfg]) {
    use toml_edit::{value, Table};
    let rows = accounts
        .iter()
        .map(|a| {
            let mut t = Table::new();
            t["name"] = value(a.name.trim());
            t["harness"] = value(a.harness.trim());
            t["home"] = value(a.home.trim());
            if let Some(c) = a.command.as_deref().map(str::trim).filter(|c| !c.is_empty()) {
                t["command"] = value(c);
            }
            if let Some(p) = a.start.as_deref().map(str::trim).filter(|p| !p.is_empty()) {
                t["start"] = value(p);
            }
            if a.preferred {
                t["preferred"] = value(true);
            }
            if let Some(c) = a.acp.as_deref().map(str::trim).filter(|c| !c.is_empty()) {
                t["acp"] = value(c);
            }
            t
        })
        .collect();
    set_tables(doc, "accounts", rows);
}

const APPS_COMMENT: &str = "# Where Codealong opens things. terminal: \"ghostty\", \"iterm2\" or \"terminal\". editor: \"vscode\", \"cursor\", \"zed\", \"xcode\" and the others\n# Settings lists, or any app's name. Picking another editor from an Open in menu sets it here.\n# new_terminal: \"window\", \"tab\" (in the front window), or \"project\" (a tab beside the same repo or project).\n# move_pointer: move the pointer to a terminal Codealong brings forward on another screen.\n";
const APPEARANCE_COMMENT: &str = "# Light or dark. \"system\" follows macOS. The buttons at the foot of the sidebar set this too.\n# zoom: how big the windows draw, in percent, 50 to 200. View › Zoom in and Zoom out (⌘= and ⌘-) set it.\n";
const JEV_COMMENT: &str = "# What Codealong asks TypeSafe's Jev, feature by feature (Settings › Jev says what each one sends).\n# The API key is kept in the login keychain; set it in Settings.\n";
const SEARCH_COMMENT: &str = "# The quick panel and `codealong sessions find` search what was said in sessions active in the\n# last `days` days, not just their titles. 0 turns it off; 14, the board's reach, is the most.\n";
const DEVICES_COMMENT: &str = "# Your other devices: Codealong on each shows the others' sessions and can read and search them.\n# tailscale finds them on your tailnet; iroh reaches devices you link with a code, anywhere.\n# remote_control lets your linked devices send prompts, answer approvals and start sessions here.\n";
const UPDATES_COMMENT: &str = "# check: look for a newer Codealong now and then, and put it in place for the next launch.\n# The app menu's Check for updates… works either way.\n";
const LIMITS_COMMENT: &str = "# When a session stops at its account's usage limit. when_hit: \"suggest\" offers to move it to an account\n# of its harness that has room; \"move\" moves it there by itself, and it carries on where it stopped.\n";
const SESSIONS_COMMENT: &str = "# continue_after_network_error: when a session's turn ends because its agent couldn't reach its API\n# (the Mac slept, or the network dropped), tell it to carry on once the API can be reached again.\n";
const PULL_REQUESTS_COMMENT: &str = "# nudge_by_default: every open PR of yours tells the session that opened it what happens on it (reviews,\n# comments, an approval, a merge, failing checks), as a message of yours. A PR's own switch wins either way.\n# nudge_about: what a nudge tells of: \"everything\"; \"feedback\", only comments and reviews that ask for changes\n# or say something, never an approval, a merge or a close; or \"feedback-and-checks\", that and failing checks.\n";
const REVIEWS_COMMENT: &str = "# auto: an agent reviews each open pull request that waits on your review, and again when new commits come
# in while it still waits. account and model: what it runs on; \"\" for the account Auto would start, on its own
# model. prompt: what it's told to do (what to look for, whether to start subagents of its own); \"\" for
# Codealong's own. post: where each review goes: \"draft\", a draft review on your GitHub account with its
# comments on their lines, for you to finish; \"comment\", a comment on the PR from your account; \"none\",
# it stays in the review's session. submit: which of the agent's verdicts goes as your review once it's posted,
# your draft review submitted with it: \"never\"; \"approve\", when it would approve (nothing that needs fixing
# before it merges); \"approve-or-request-changes\", and a request for changes when it would ask for them; or
# \"approve-or-comment\", that request sent as a comment instead. When it can't tell, it's left for you.
";
const OWN_REVIEWS_COMMENT: &str = "# [reviews.own] auto: an agent reviews each open pull request a session of yours opened here, and again when new
# commits come in. account and model: what it runs on; \"\" for the account Auto would start among those of another
# harness than the PR's session. prompt: what it's told to do; \"\" for Codealong's own. send: hand each review to
# the session that opened the PR, once it runs and doesn't wait on you. Nothing goes on GitHub.
";
const DEBUG_COMMENT: &str = "# Timings for finding what makes the app slow, in ~/Library/Logs/Codealong/perf.log.\n";
const SHORTCUTS_COMMENT: &str = "# Shortcuts that work in any app, such as \"Option+Cmd+/\" or \"Cmd+Ctrl+K\". \"\" turns one off.\n# quick_panel opens the quick panel: find a session, or start one.\n# waiting opens the first session that waits on you, to answer one after another: in the notch\n# when the bar is docked there, else in the waiting window.\n";

/// Sets `[apps] editor` in config.toml and keeps everything else as written: an editor picked
/// from an Open in menu is the one Open in <editor> uses from then on.
pub fn save_editor(id: &str) -> Result<()> {
    update(|text| Ok((with_editor(text, id)?, ())))
}

pub fn with_editor(text: &str, id: &str) -> Result<String> {
    let mut doc: toml_edit::DocumentMut = text.parse().context("config.toml has an error; fix it before changing the editor")?;
    ensure_table(&mut doc, "apps", APPS_COMMENT);
    set_value(&mut doc["apps"]["editor"], toml_edit::Value::from(id));
    Ok(doc.to_string())
}

/// Sets `[appearance] theme` in config.toml and keeps everything else as written.
pub fn save_theme(theme: Theme) -> Result<()> {
    update(|text| Ok((with_theme(text, theme)?, ())))
}

/// `text` with its theme set. A file from before there was a theme gets an `[appearance]`
/// table after `[apps]`, commented as a new file would be; a comment after the value stays.
pub fn with_theme(text: &str, theme: Theme) -> Result<String> {
    let mut doc: toml_edit::DocumentMut = text.parse().context("config.toml has an error; fix it before changing the theme")?;
    set_theme(&mut doc, theme);
    Ok(doc.to_string())
}

fn set_theme(doc: &mut toml_edit::DocumentMut, theme: Theme) {
    ensure_appearance(doc);
    set_value(&mut doc["appearance"]["theme"], toml_edit::Value::from(theme.as_str()));
}

/// Steps `[appearance] zoom` in config.toml from what it says now (zoom.rs `stepped`), keeping
/// everything else as written, and answers the zoom it's at. It reads the file each time, not
/// the settings the app runs on, which catch up a moment later: so presses quicker than that
/// each step from the one before, one at a time under the file's lock (`update`).
pub fn step_zoom(step: crate::zoom::Step) -> Result<u32> {
    update(|text| with_zoom_step(text, step))
}

/// `text` with its zoom stepped, and the zoom it's at. Only a file that zooms, or has the key
/// already, gets it; a comment after the value stays.
pub fn with_zoom_step(text: &str, step: crate::zoom::Step) -> Result<(String, u32)> {
    use crate::zoom::{stepped, ACTUAL};
    let mut doc: toml_edit::DocumentMut = text.parse().context("config.toml has an error; fix it before zooming")?;
    let said = doc.get("appearance").and_then(|a| a.get("zoom"));
    let now = said.and_then(toml_edit::Item::as_integer).map_or(ACTUAL, |z| u32::try_from(z.max(0)).unwrap_or(u32::MAX));
    let zoom = stepped(now, step);
    if zoom != ACTUAL || said.is_some() {
        ensure_appearance(&mut doc);
        set_value(&mut doc["appearance"]["zoom"], toml_edit::Value::from(i64::from(zoom)));
    }
    Ok((doc.to_string(), zoom))
}

/// Gives a file from before there was an `[appearance]` table one after `[apps]`, commented as a
/// new file would be.
fn ensure_appearance(doc: &mut toml_edit::DocumentMut) {
    use toml_edit::{Item, Table};
    if !doc.get("appearance").is_some_and(Item::is_table_like) {
        let mut t = Table::new();
        t.decor_mut().set_prefix(format!("\n{APPEARANCE_COMMENT}"));
        // Tables made here have no place in the file, so they'd land at the end; this one goes
        // with [apps], the other setting you'd look for at the top.
        if let Some(at) = doc.get("apps").and_then(Item::as_table).and_then(|a| a.position()) {
            t.set_position(at);
        }
        doc.insert("appearance", Item::Table(t));
        place_after(doc, "appearance", "apps");
    }
}

/// Sets `[shortcuts]`. A file from before there were shortcuts gets the table after
/// `[appearance]`, commented as a new file would be.
fn set_shortcuts(doc: &mut toml_edit::DocumentMut, shortcuts: &ShortcutsCfg) {
    use toml_edit::{Item, Table, Value};
    if !doc.get("shortcuts").is_some_and(Item::is_table_like) {
        let mut t = Table::new();
        t.decor_mut().set_prefix(format!("\n{SHORTCUTS_COMMENT}"));
        // Like [appearance], it goes with the settings at the top rather than at the end.
        let after = ["appearance", "apps"].into_iter().find_map(|name| Some((name, doc.get(name).and_then(Item::as_table).and_then(|t| t.position())?)));
        if let Some((_, at)) = after {
            t.set_position(at);
        }
        doc.insert("shortcuts", Item::Table(t));
        if let Some((name, _)) = after {
            place_after(doc, "shortcuts", name);
        }
    }
    set_value(&mut doc["shortcuts"]["quick_panel"], Value::from(shortcuts.quick_panel.trim()));
    set_value(&mut doc["shortcuts"]["waiting"], Value::from(shortcuts.waiting.trim()));
}

/// Moves a table just added (it goes at the end of the file's map) to right after `after`, so
/// it's written there. The tables Settings rewrites whole, accounts and projects, have no place
/// in the file of their own and follow whatever comes before them in the map.
fn place_after(doc: &mut toml_edit::DocumentMut, name: &str, after: &str) {
    let order: Vec<String> = doc.as_table().iter().map(|(k, _)| k.to_string()).collect();
    let Some(anchor) = order.iter().position(|k| k == after) else { return };
    let rank = |k: &str| if k == name { (anchor, 1) } else { (order.iter().position(|x| x == k).unwrap_or(usize::MAX), 0) };
    doc.as_table_mut().sort_values_by(|a, _, b, _| rank(a.get()).cmp(&rank(b.get())));
}

/// A terminal or editor profile the `[apps]` table can name.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Profile {
    pub id: String,
    pub name: String,
}

/// What the Settings form offers for `[apps]`: the terminal profiles installed here (see
/// launch.rs), and the editors installed here (editors.rs).
pub fn terminal_profiles() -> Vec<Profile> {
    crate::launch::profiles().filter(|(_, t)| t.installed()).map(|(id, t)| Profile { id: id.into(), name: t.name().into() }).collect()
}
pub fn editor_profiles() -> Vec<Profile> {
    crate::editors::installed()
}

/// What Settings works from: the settings as parsed, the file as written, and why it couldn't
/// be parsed, if it couldn't (then `config` is the defaults and the form stays read-only), and
/// the profiles the pickers offer.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigView {
    pub config: Config,
    pub text: String,
    pub path: String,
    pub error: Option<String>,
    pub terminals: Vec<Profile>,
    pub editors: Vec<Profile>,
    /// Chrome's profiles, for a project's `chrome_profile` and a worktree's preview.
    pub chrome_profiles: Vec<crate::chrome::Profile>,
    /// What a reviewer is told to do while `[reviews] prompt` is empty, for Settings to show and
    /// edit (autoreview.rs `INSTRUCTIONS`).
    pub review_prompt: String,
}

pub fn view() -> ConfigView {
    let path = config_path();
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    view_of(&text, &crate::util::tilde(&path))
}

fn view_of(text: &str, path: &str) -> ConfigView {
    let (config, error) = match from_text(text) {
        Ok(c) => (c, None),
        Err(e) => (Config::default(), Some(e.to_string().trim().to_string())),
    };
    ConfigView { config, text: text.to_string(), path: path.to_string(), error, terminals: terminal_profiles(), editors: editor_profiles(), chrome_profiles: crate::chrome::profiles(), review_prompt: crate::autoreview::INSTRUCTIONS.to_string() }
}

/// Why a list of projects can't be saved, in words.
pub fn check_projects(projects: &[ProjectCfg]) -> std::result::Result<(), String> {
    let mut names = std::collections::HashSet::new();
    for p in projects {
        let name = p.name.trim();
        if name.is_empty() {
            return Err("Every project needs a name.".into());
        }
        if !names.insert(name) {
            return Err(format!("Two projects are named {name}."));
        }
        if p.repos.iter().chain(&p.folders).any(|s| s.trim().is_empty()) {
            return Err(format!("{name} has an empty repo or folder entry."));
        }
        if p.chrome_profile.as_deref().is_some_and(|c| c.trim().is_empty()) {
            return Err(format!("{name}'s chrome_profile is empty; take the line out to open its previews in the default browser."));
        }
    }
    Ok(())
}

/// Why the whole config can't be saved, in words.
pub fn check(cfg: &Config) -> std::result::Result<(), String> {
    check_accounts(&cfg.accounts)?;
    check_projects(&cfg.projects)?;
    if cfg.scan.iter().any(|s| s.trim().is_empty()) {
        return Err("A scan folder is empty.".into());
    }
    if cfg.search.days > MAX_SEARCH_DAYS {
        return Err(format!("[search] days is {}; the board goes back {MAX_SEARCH_DAYS} days, so that's the most.", cfg.search.days));
    }
    if cfg.jev.model.trim().is_empty() {
        return Err("[jev] model is empty; use \"jev-latest\".".into());
    }
    let reviewer = cfg.reviews.account.trim();
    if !reviewer.is_empty() && !cfg.accounts.iter().any(|a| a.name == reviewer) {
        return Err(format!("[reviews] account is {reviewer:?}, which isn't one of the [[accounts]]; use one of them, or \"\" for Auto."));
    }
    let model = cfg.reviews.model.trim();
    if !model.is_empty() && !crate::start::valid_model(model) {
        return Err(format!("[reviews] model is {model:?}, which isn't a model name Codealong can start a session on."));
    }
    let reviewer = cfg.reviews.own.account.trim();
    if !reviewer.is_empty() && !cfg.accounts.iter().any(|a| a.name == reviewer) {
        return Err(format!("[reviews.own] account is {reviewer:?}, which isn't one of the [[accounts]]; use one of them, or \"\" for Auto."));
    }
    let model = cfg.reviews.own.model.trim();
    if !model.is_empty() && !crate::start::valid_model(model) {
        return Err(format!("[reviews.own] model is {model:?}, which isn't a model name Codealong can start a session on."));
    }
    if cfg.devices.port < 1024 {
        return Err(format!("[devices] port is {}; use one from 1024 up ({DEFAULT_DEVICES_PORT} is the default).", cfg.devices.port));
    }
    if cfg.devices.name.chars().count() > 64 || cfg.devices.name.chars().any(char::is_control) {
        return Err("[devices] name is too long or has control characters.".into());
    }
    if !(crate::zoom::MIN..=crate::zoom::MAX).contains(&cfg.appearance.zoom) {
        return Err(format!("[appearance] zoom is {}; it goes from {} to {} (percent).", cfg.appearance.zoom, crate::zoom::MIN, crate::zoom::MAX));
    }
    if cfg.apps.terminal.trim().is_empty() || cfg.apps.editor.trim().is_empty() {
        return Err("The terminal and editor need a profile name.".into());
    }
    if crate::launch::terminal_app(&cfg.apps.terminal).is_none() {
        return Err(format!("[apps] terminal is {:?}; use {}.", cfg.apps.terminal, crate::launch::profile_names()));
    }
    let quick = crate::quick::parse_shortcut(&cfg.shortcuts.quick_panel).map_err(|e| format!("[shortcuts] quick_panel: {e}"))?;
    let waiting = crate::quick::parse_shortcut(&cfg.shortcuts.waiting).map_err(|e| format!("[shortcuts] waiting: {e}"))?;
    if quick.is_some() && quick == waiting {
        return Err("[shortcuts] quick_panel and waiting are the same keys; one press can only do one of them.".into());
    }
    Ok(())
}

/// Saves every setting in config.toml, in place. The watcher sees the change and reloads.
pub fn save(cfg: &Config) -> Result<()> {
    check(cfg).map_err(anyhow::Error::msg)?;
    update(|text| Ok((with_config(text, cfg)?, ())))
}

/// Saves config.toml as typed, once it parses as settings: the whole of it, as you see it in
/// Settings, but after any other change Codealong is making to it (`update`).
pub fn save_text(text: &str) -> Result<()> {
    parse(text)?;
    update(|_| Ok((text.to_string(), ())))
}

/// The settings in `text`, or why it can't be saved.
pub fn parse(text: &str) -> Result<Config> {
    let cfg: Config = from_text(text).map_err(|e| anyhow!("{}", e.to_string().trim()))?;
    check(&cfg).map_err(anyhow::Error::msg)?;
    Ok(cfg)
}

/// `text` with every setting set to `cfg`'s, each in its place; tables the file doesn't have
/// yet are added, commented as a new file would be.
pub fn with_config(text: &str, cfg: &Config) -> Result<String> {
    use toml_edit::{Array, DocumentMut, Item, Table, Value};
    let mut doc: DocumentMut = text.parse().context("config.toml has an error; fix it before saving settings")?;
    let strings = |items: &[String]| Value::Array(Array::from_iter(items.iter().map(|s| s.trim())));

    set_value(&mut doc["scan"], strings(&cfg.scan));

    ensure_table(&mut doc, "apps", APPS_COMMENT);
    set_value(&mut doc["apps"]["terminal"], Value::from(cfg.apps.terminal.trim()));
    set_value(&mut doc["apps"]["editor"], Value::from(cfg.apps.editor.trim()));
    set_value(&mut doc["apps"]["new_terminal"], Value::from(cfg.apps.new_terminal.as_str()));
    // Opt-in: only a file that turns it on, or has it already, gets the key.
    if cfg.apps.move_pointer || doc["apps"].get("move_pointer").is_some() {
        set_value(&mut doc["apps"]["move_pointer"], Value::from(cfg.apps.move_pointer));
    }

    set_theme(&mut doc, cfg.appearance.theme);
    set_shortcuts(&mut doc, &cfg.shortcuts);

    // Only a file that has the table, or turns something on, gets it: it's opt-in. An old
    // `[router]` gives way to it.
    if cfg.jev != JevCfg::default() || doc.get("jev").is_some() || doc.get("router").is_some() {
        place_jev(&mut doc);
        doc.remove("router");
        set_value(&mut doc["jev"]["model"], Value::from(cfg.jev.model.trim()));
        set_value(&mut doc["jev"]["quick_panel"], Value::from(cfg.jev.quick_panel));
        set_value(&mut doc["jev"]["sessions"], Value::from(cfg.jev.sessions));
        set_value(&mut doc["jev"]["todos"], Value::from(cfg.jev.todos));
        set_value(&mut doc["jev"]["pull_requests"], Value::from(cfg.jev.pull_requests));
    }

    // Like [jev], only a file that has the table or changes the default gets it.
    if cfg.search != SearchCfg::default() || doc.get("search").is_some() {
        if !doc.get("search").is_some_and(toml_edit::Item::is_table_like) {
            let mut t = Table::new();
            t.decor_mut().set_prefix(format!("\n{SEARCH_COMMENT}"));
            doc.insert("search", Item::Table(t));
            let after = if doc.get("jev").is_some() { "jev" } else { "shortcuts" };
            place_after(&mut doc, "search", after);
        }
        set_value(&mut doc["search"]["days"], Value::from(cfg.search.days as i64));
    }

    // Opt-in like [search]: only a file that has the table or changes a default gets it.
    if cfg.devices != DevicesCfg::default() || doc.get("devices").is_some() {
        if !doc.get("devices").is_some_and(toml_edit::Item::is_table_like) {
            let mut t = Table::new();
            t.decor_mut().set_prefix(format!("\n{DEVICES_COMMENT}"));
            doc.insert("devices", Item::Table(t));
            let after = ["search", "jev", "shortcuts"].into_iter().find(|k| doc.get(k).is_some()).unwrap_or("shortcuts");
            place_after(&mut doc, "devices", after);
        }
        let d = &cfg.devices;
        set_value(&mut doc["devices"]["enabled"], Value::from(d.enabled));
        if !d.name.trim().is_empty() || doc["devices"].get("name").is_some() {
            set_value(&mut doc["devices"]["name"], Value::from(d.name.trim()));
        }
        set_value(&mut doc["devices"]["tailscale"], Value::from(d.tailscale));
        set_value(&mut doc["devices"]["iroh"], Value::from(d.iroh));
        set_value(&mut doc["devices"]["remote_control"], Value::from(d.remote_control));
        if d.port != DEFAULT_DEVICES_PORT || doc["devices"].get("port").is_some() {
            set_value(&mut doc["devices"]["port"], Value::from(d.port as i64));
        }
    }

    // Like [search]: only a file that has the table or turns checking off gets it.
    if cfg.updates != UpdatesCfg::default() || doc.get("updates").is_some() {
        if !doc.get("updates").is_some_and(toml_edit::Item::is_table_like) {
            let mut t = Table::new();
            t.decor_mut().set_prefix(format!("\n{UPDATES_COMMENT}"));
            doc.insert("updates", Item::Table(t));
            let after = ["devices", "search", "jev", "shortcuts"].into_iter().find(|k| doc.get(k).is_some()).unwrap_or("shortcuts");
            place_after(&mut doc, "updates", after);
        }
        set_value(&mut doc["updates"]["check"], Value::from(cfg.updates.check));
    }

    // Like [updates]: only a file that has the table or moves by itself gets it.
    if cfg.limits != LimitsCfg::default() || doc.get("limits").is_some() {
        if !doc.get("limits").is_some_and(toml_edit::Item::is_table_like) {
            let mut t = Table::new();
            t.decor_mut().set_prefix(format!("\n{LIMITS_COMMENT}"));
            doc.insert("limits", Item::Table(t));
            let after = ["updates", "devices", "search", "jev", "shortcuts"].into_iter().find(|k| doc.get(k).is_some()).unwrap_or("shortcuts");
            place_after(&mut doc, "limits", after);
        }
        set_value(&mut doc["limits"]["when_hit"], Value::from(cfg.limits.when_hit.as_str()));
    }

    // Like [updates]: only a file that has the table or turns continuing off gets it.
    if cfg.sessions != SessionsCfg::default() || doc.get("sessions").is_some() {
        if !doc.get("sessions").is_some_and(toml_edit::Item::is_table_like) {
            let mut t = Table::new();
            t.decor_mut().set_prefix(format!("\n{SESSIONS_COMMENT}"));
            doc.insert("sessions", Item::Table(t));
            let after = ["limits", "updates", "devices", "search", "jev", "shortcuts"].into_iter().find(|k| doc.get(k).is_some()).unwrap_or("shortcuts");
            place_after(&mut doc, "sessions", after);
        }
        set_value(&mut doc["sessions"]["continue_after_network_error"], Value::from(cfg.sessions.continue_after_network_error));
    }

    // The same: only a file that has the table or turns reviews on gets it.
    if cfg.reviews != ReviewsCfg::default() || doc.get("reviews").is_some() {
        if !doc.get("reviews").is_some_and(toml_edit::Item::is_table_like) {
            let mut t = Table::new();
            t.decor_mut().set_prefix(format!("\n{REVIEWS_COMMENT}"));
            doc.insert("reviews", Item::Table(t));
            let after = ["sessions", "limits", "updates", "devices", "search", "jev", "shortcuts"].into_iter().find(|k| doc.get(k).is_some()).unwrap_or("shortcuts");
            place_after(&mut doc, "reviews", after);
        }
        set_value(&mut doc["reviews"]["auto"], Value::from(cfg.reviews.auto));
        set_value(&mut doc["reviews"]["account"], Value::from(cfg.reviews.account.trim()));
        set_value(&mut doc["reviews"]["model"], Value::from(cfg.reviews.model.trim()));
        set_value(&mut doc["reviews"]["prompt"], Value::from(cfg.reviews.prompt.trim()));
        set_value(&mut doc["reviews"]["post"], Value::from(cfg.reviews.post.as_str()));
        set_value(&mut doc["reviews"]["submit"], Value::from(cfg.reviews.submit.as_str()));
        // What said who reviewed and where it went for a day (2026-10-09): `comment`, then
        // `reviewers` (folded into `account` and `model` as it's read) and `combine`; and
        // `auto_approve`, folded into `submit` (2026-10-11).
        if let Some(t) = doc.get_mut("reviews").and_then(Item::as_table_like_mut) {
            for old in ["comment", "reviewers", "combine", "auto_approve"] {
                t.remove(old);
            }
        }
        // [reviews.own] the same way: only once it's turned on or the file has it. A table left
        // without a place of its own is written right after [reviews].
        let own = &cfg.reviews.own;
        if *own != OwnReviewsCfg::default() || doc["reviews"].get("own").is_some() {
            if !doc["reviews"].get("own").is_some_and(toml_edit::Item::is_table_like) {
                let mut t = Table::new();
                t.decor_mut().set_prefix(format!("\n{OWN_REVIEWS_COMMENT}"));
                if let Some(r) = doc["reviews"].as_table_like_mut() {
                    r.insert("own", Item::Table(t));
                }
            }
            set_value(&mut doc["reviews"]["own"]["auto"], Value::from(own.auto));
            set_value(&mut doc["reviews"]["own"]["account"], Value::from(own.account.trim()));
            set_value(&mut doc["reviews"]["own"]["model"], Value::from(own.model.trim()));
            set_value(&mut doc["reviews"]["own"]["prompt"], Value::from(own.prompt.trim()));
            set_value(&mut doc["reviews"]["own"]["send"], Value::from(own.send));
        }
    }

    // `[pull_requests] when_approved` was a setting for a day (2026-10-05), then nudges were
    // turned on PR by PR (nudges.rs), so a file that still has it loses it, and the table with
    // it unless the table has something else.
    if let Some(t) = doc.get_mut("pull_requests").and_then(Item::as_table_like_mut) {
        t.remove("when_approved");
        if t.is_empty() && cfg.pull_requests == PullRequestsCfg::default() {
            doc.remove("pull_requests");
        }
    }
    // Then as [reviews]: only a file that has the table or nudges by default gets it.
    if cfg.pull_requests != PullRequestsCfg::default() || doc.get("pull_requests").is_some() {
        if !doc.get("pull_requests").is_some_and(toml_edit::Item::is_table_like) {
            let mut t = Table::new();
            t.decor_mut().set_prefix(format!("\n{PULL_REQUESTS_COMMENT}"));
            doc.insert("pull_requests", Item::Table(t));
            let after = ["reviews", "sessions", "limits", "updates", "devices", "search", "jev", "shortcuts"].into_iter().find(|k| doc.get(k).is_some()).unwrap_or("shortcuts");
            place_after(&mut doc, "pull_requests", after);
        }
        set_value(&mut doc["pull_requests"]["nudge_by_default"], Value::from(cfg.pull_requests.nudge_by_default));
        set_value(&mut doc["pull_requests"]["nudge_about"], Value::from(cfg.pull_requests.nudge_about.as_str()));
    }

    ensure_table(&mut doc, "debug", DEBUG_COMMENT);
    set_value(&mut doc["debug"]["perf_log"], Value::from(cfg.debug.perf_log));

    set_accounts(&mut doc, &cfg.accounts);

    let projects: Vec<Table> = cfg
        .projects
        .iter()
        .map(|p| {
            let mut t = Table::new();
            t["name"] = Item::Value(Value::from(p.name.trim()));
            t["repos"] = Item::Value(strings(&p.repos));
            t["folders"] = Item::Value(strings(&p.folders));
            if let Some(c) = p.chrome_profile.as_deref().map(str::trim).filter(|c| !c.is_empty()) {
                t["chrome_profile"] = Item::Value(Value::from(c));
            }
            t
        })
        .collect();
    set_tables(&mut doc, "projects", projects);
    Ok(doc.to_string())
}

/// Adds `[jev]` with its comment when the file has none, after `[shortcuts]`.
fn place_jev(doc: &mut toml_edit::DocumentMut) {
    use toml_edit::{Item, Table};
    if doc.get("jev").is_some_and(Item::is_table_like) {
        return;
    }
    let mut t = Table::new();
    t.decor_mut().set_prefix(format!("\n{JEV_COMMENT}"));
    doc.insert("jev", Item::Table(t));
    place_after(doc, "jev", "shortcuts");
}

/// Sets a value and keeps the comments and spacing around the old one.
fn set_value(slot: &mut toml_edit::Item, new: toml_edit::Value) {
    match slot.as_value_mut() {
        Some(v) => {
            let decor = v.decor().clone();
            *v = new;
            *v.decor_mut() = decor;
        }
        None => *slot = toml_edit::Item::Value(new),
    }
}

/// Adds an empty `[name]` table with its comment when the file has none. It lands at the end,
/// which is where a new file puts `[debug]` too.
fn ensure_table(doc: &mut toml_edit::DocumentMut, name: &str, comment: &str) {
    use toml_edit::{Item, Table};
    if doc.get(name).is_some_and(Item::is_table_like) {
        return;
    }
    let mut t = Table::new();
    t.decor_mut().set_prefix(format!("\n{comment}"));
    doc.insert(name, Item::Table(t));
}

/// Replaces the `[[name]]` tables. The comment above the first one stays.
fn set_tables(doc: &mut toml_edit::DocumentMut, name: &str, rows: Vec<toml_edit::Table>) {
    use toml_edit::{ArrayOfTables, Item};
    let lead = doc
        .get(name)
        .and_then(Item::as_array_of_tables)
        .and_then(|a| a.get(0))
        .and_then(|t| t.decor().prefix().and_then(|p| p.as_str().map(str::to_string)));
    let mut tables = ArrayOfTables::new();
    for (i, mut t) in rows.into_iter().enumerate() {
        let prefix = if i == 0 { lead.clone().unwrap_or_else(|| "\n".to_string()) } else { "\n".to_string() };
        t.decor_mut().set_prefix(prefix);
        tables.push(t);
    }
    doc[name] = Item::ArrayOfTables(tables);
}

/// Suggests one project per GitHub owner your agents worked in, named the way GitHub names
/// the owner (`display_name`), or by the owner's login when it has no name.
pub fn suggest_projects(owners: &BTreeMap<String, usize>, display_name: impl Fn(&str) -> Option<String>) -> Vec<ProjectCfg> {
    owners
        .keys()
        .map(|owner| ProjectCfg {
            name: display_name(owner).map(|n| n.trim().to_string()).filter(|n| !n.is_empty()).unwrap_or_else(|| owner.clone()),
            repos: vec![format!("github.com/{owner}/*")],
            folders: vec![],
            chrome_profile: None,
        })
        .collect()
}

fn q(s: &str) -> String {
    toml::Value::String(s.to_string()).to_string()
}

fn list(items: &[String]) -> String {
    format!("[{}]", items.iter().map(|s| q(s)).collect::<Vec<_>>().join(", "))
}

/// Renders the config with comments, so the file explains itself.
pub fn render(cfg: &Config) -> String {
    let mut s = String::new();
    s.push_str("# Codealong settings. Codealong watches this file and applies changes when you save it.\n\n");
    s.push_str("# Folders searched for repos, three levels deep.\n");
    s.push_str(&format!("scan = {}\n\n", list(&cfg.scan)));
    s.push_str(APPS_COMMENT);
    s.push_str("[apps]\n");
    s.push_str(&format!("terminal = {}\n", q(&cfg.apps.terminal)));
    s.push_str(&format!("editor = {}\n", q(&cfg.apps.editor)));
    s.push_str(&format!("new_terminal = {}\n", q(cfg.apps.new_terminal.as_str())));
    if cfg.apps.move_pointer {
        s.push_str("move_pointer = true\n");
    }
    s.push('\n');
    s.push_str(APPEARANCE_COMMENT);
    s.push_str("[appearance]\n");
    s.push_str(&format!("theme = {}\n", q(cfg.appearance.theme.as_str())));
    if cfg.appearance.zoom != crate::zoom::ACTUAL {
        s.push_str(&format!("zoom = {}\n", cfg.appearance.zoom));
    }
    s.push('\n');
    s.push_str(SHORTCUTS_COMMENT);
    s.push_str("[shortcuts]\n");
    s.push_str(&format!("quick_panel = {}\n", q(&cfg.shortcuts.quick_panel)));
    s.push_str(&format!("waiting = {}\n", q(&cfg.shortcuts.waiting)));
    if cfg.jev != JevCfg::default() {
        s.push('\n');
        s.push_str(JEV_COMMENT);
        s.push_str("[jev]\n");
        s.push_str(&format!("model = {}\n", q(&cfg.jev.model)));
        s.push_str(&format!("quick_panel = {}\n", cfg.jev.quick_panel));
        s.push_str(&format!("sessions = {}\n", cfg.jev.sessions));
        s.push_str(&format!("todos = {}\n", cfg.jev.todos));
        s.push_str(&format!("pull_requests = {}\n", cfg.jev.pull_requests));
    }
    s.push('\n');
    s.push_str(SEARCH_COMMENT);
    s.push_str("[search]\n");
    s.push_str(&format!("days = {}\n", cfg.search.days));
    if cfg.devices != DevicesCfg::default() {
        let d = &cfg.devices;
        s.push('\n');
        s.push_str(DEVICES_COMMENT);
        s.push_str("[devices]\n");
        s.push_str(&format!("enabled = {}\n", d.enabled));
        if !d.name.is_empty() {
            s.push_str(&format!("name = {}\n", q(&d.name)));
        }
        s.push_str(&format!("tailscale = {}\n", d.tailscale));
        s.push_str(&format!("iroh = {}\n", d.iroh));
        s.push_str(&format!("remote_control = {}\n", d.remote_control));
        if d.port != DEFAULT_DEVICES_PORT {
            s.push_str(&format!("port = {}\n", d.port));
        }
    }
    if cfg.updates != UpdatesCfg::default() {
        s.push('\n');
        s.push_str(UPDATES_COMMENT);
        s.push_str("[updates]\n");
        s.push_str(&format!("check = {}\n", cfg.updates.check));
    }
    if cfg.limits != LimitsCfg::default() {
        s.push('\n');
        s.push_str(LIMITS_COMMENT);
        s.push_str("[limits]\n");
        s.push_str(&format!("when_hit = {}\n", q(cfg.limits.when_hit.as_str())));
    }
    if cfg.sessions != SessionsCfg::default() {
        s.push('\n');
        s.push_str(SESSIONS_COMMENT);
        s.push_str("[sessions]\n");
        s.push_str(&format!("continue_after_network_error = {}\n", cfg.sessions.continue_after_network_error));
    }
    if cfg.reviews != ReviewsCfg::default() {
        s.push('\n');
        s.push_str(REVIEWS_COMMENT);
        s.push_str("[reviews]\n");
        s.push_str(&format!("auto = {}\n", cfg.reviews.auto));
        s.push_str(&format!("account = {}\n", q(cfg.reviews.account.trim())));
        s.push_str(&format!("model = {}\n", q(cfg.reviews.model.trim())));
        s.push_str(&format!("prompt = {}\n", toml_edit::Value::from(cfg.reviews.prompt.trim())));
        s.push_str(&format!("post = {}\n", q(cfg.reviews.post.as_str())));
        s.push_str(&format!("submit = {}\n", q(cfg.reviews.submit.as_str())));
        let own = &cfg.reviews.own;
        if *own != OwnReviewsCfg::default() {
            s.push('\n');
            s.push_str(OWN_REVIEWS_COMMENT);
            s.push_str("[reviews.own]\n");
            s.push_str(&format!("auto = {}\n", own.auto));
            s.push_str(&format!("account = {}\n", q(own.account.trim())));
            s.push_str(&format!("model = {}\n", q(own.model.trim())));
            s.push_str(&format!("prompt = {}\n", toml_edit::Value::from(own.prompt.trim())));
            s.push_str(&format!("send = {}\n", own.send));
        }
    }
    if cfg.pull_requests != PullRequestsCfg::default() {
        s.push('\n');
        s.push_str(PULL_REQUESTS_COMMENT);
        s.push_str("[pull_requests]\n");
        s.push_str(&format!("nudge_by_default = {}\n", cfg.pull_requests.nudge_by_default));
        s.push_str(&format!("nudge_about = {}\n", q(cfg.pull_requests.nudge_about.as_str())));
    }
    s.push_str("\n# Harness accounts. `name` is what you type and what the board shows.\n# `start` is where new sessions start: \"terminal\", \"background\", \"claude-app\", \"codex-app\" or \"codealong\". A start from the quick panel sets it.\n# `acp`: the command that runs the account's Agent Client Protocol adapter, for sessions Codealong runs itself.\n# `preferred = true`: Auto starts this account over ones without it while it has room.\n");
    for a in &cfg.accounts {
        s.push_str("\n[[accounts]]\n");
        s.push_str(&format!("name = {}\n", q(&a.name)));
        s.push_str(&format!("harness = {}\n", q(&a.harness)));
        s.push_str(&format!("home = {}\n", q(&a.home)));
        if let Some(c) = &a.command {
            s.push_str(&format!("command = {}\n", q(c)));
        }
        if let Some(p) = &a.start {
            s.push_str(&format!("start = {}\n", q(p)));
        }
        if a.preferred {
            s.push_str("preferred = true\n");
        }
        if let Some(c) = &a.acp {
            s.push_str(&format!("acp = {}\n", q(c)));
        }
    }
    s.push_str("\n# Projects group repos (by pattern) and plain folders.\n");
    for p in &cfg.projects {
        s.push_str("\n[[projects]]\n");
        s.push_str(&format!("name = {}\n", q(&p.name)));
        s.push_str(&format!("repos = {}\n", list(&p.repos)));
        s.push_str(&format!("folders = {}\n", list(&p.folders)));
        if let Some(c) = &p.chrome_profile {
            s.push_str(&format!("chrome_profile = {}\n", q(c)));
        }
    }
    s.push('\n');
    s.push_str(DEBUG_COMMENT);
    s.push_str("[debug]\n");
    s.push_str(&format!("perf_log = {}\n", cfg.debug.perf_log));
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A config.toml that doesn't parse is an error that says where to look, never the
    /// defaults, which have no accounts: what acted on those would act on settings that aren't
    /// yours. No file yet is the defaults.
    #[test]
    fn settings_that_dont_parse_are_an_error_not_the_defaults() {
        let broken = load_text("[[accounts]\nname = ");
        let e = from_file(broken.map(Some)).unwrap_err().to_string();
        assert!(e.ends_with("`codealong config check` says what's wrong with it."), "{e}");
        assert_eq!(from_file(Ok(None)).unwrap(), Config::default());
        let cfg = load_text("[[accounts]]\nname = \"work\"\nharness = \"claude\"\nhome = \"~/.claude\"\n").unwrap();
        assert_eq!(from_file(Ok(Some(cfg.clone()))).unwrap(), cfg);
    }

    #[test]
    fn renders_and_parses_back() {
        let cfg = Config {
            scan: vec!["~/projects".into()],
            apps: AppsCfg::default(),
            appearance: AppearanceCfg { theme: Theme::Dark, ..AppearanceCfg::default() },
            shortcuts: ShortcutsCfg { quick_panel: "Cmd+Ctrl+K".into(), waiting: "Ctrl+Option+N".into() },
            accounts: vec![
                AccountCfg { name: "claude".into(), harness: "claude".into(), home: "~/.claude".into(), command: Some("claude".into()), start: None, preferred: false, acp: None },
                AccountCfg { name: "codex".into(), harness: "codex".into(), home: "~/.codex".into(), command: None, start: None, preferred: false, acp: None },
            ],
            projects: vec![ProjectCfg {
                name: "Spruce".into(),
                repos: vec!["github.com/sprucehq/*".into()],
                folders: vec!["~/projects/sparkhq/vault".into()],
                chrome_profile: Some("Work".into()),
            }],
            jev: JevCfg { model: "jev-1.13.0".into(), quick_panel: true, sessions: false, todos: true, pull_requests: false },
            router: None,
            search: SearchCfg { days: 7 },
            devices: DevicesCfg { enabled: true, name: "Studio".into(), tailscale: true, iroh: false, remote_control: true, port: 47000 },
            updates: UpdatesCfg { check: false },
            limits: LimitsCfg { when_hit: WhenLimited::Move },
            sessions: SessionsCfg { continue_after_network_error: false },
            reviews: ReviewsCfg { auto: true, account: "claude".into(), model: "opus".into(), prompt: "Start two subagents.\nThen combine their findings.".into(), post: ReviewPost::None, submit: ReviewSubmit::Approve, auto_approve: false, reviewers: Vec::new(), own: OwnReviewsCfg::default() },
            pull_requests: PullRequestsCfg { nudge_by_default: true, nudge_about: NudgeAbout::FeedbackAndChecks },
            debug: DebugCfg::default(),
        };
        let parsed: Config = toml::from_str(&render(&cfg)).unwrap();

        assert_eq!(parsed, cfg);
    }

    #[test]
    fn saving_accounts_keeps_the_rest_of_the_file() {
        let text = render(&Config {
            accounts: vec![AccountCfg { name: "claude".into(), harness: "claude".into(), home: "~/.claude".into(), command: Some("claude".into()), start: None, preferred: false, acp: None }],
            projects: vec![ProjectCfg { name: "Spruce".into(), repos: vec!["github.com/sprucehq/*".into()], folders: vec![], chrome_profile: None }],
            ..Config::default()
        });
        let text = text.replace("[apps]\n", "[apps]\n# my editor of choice\n");
        let accounts = vec![
            AccountCfg { name: "claude".into(), harness: "claude".into(), home: "~/.claude".into(), command: Some("claude".into()), start: None, preferred: false, acp: None },
            AccountCfg { name: "claude3".into(), harness: "claude".into(), home: "~/.claude3".into(), command: Some("CLAUDE_CONFIG_DIR=~/.claude3 claude".into()), start: Some("background".into()), preferred: true, acp: None },
        ];
        let out = with_accounts(&text, &accounts).unwrap();
        assert!(out.contains("# my editor of choice"));
        assert!(out.contains("# Harness accounts."));
        assert!(out.find("claude3").unwrap() < out.find("[[projects]]").unwrap());
        let parsed: Config = toml::from_str(&out).unwrap();
        assert_eq!(parsed.accounts, accounts);
        assert_eq!(parsed.projects.len(), 1);
    }

    #[test]
    fn rejects_bad_accounts() {
        let a = |name: &str, home: &str| AccountCfg { name: name.into(), harness: "claude".into(), home: home.into(), command: None, start: None, preferred: false, acp: None };
        assert!(check_accounts(&[a("claude", "~/.claude"), a("claude", "~/.claude2")]).is_err());
        assert!(check_accounts(&[a("claude", "~/.claude"), a("claude2", "~/.claude")]).is_err());
        assert!(check_accounts(&[a("my claude", "~/.claude")]).is_err());
        assert!(check_accounts(&[a("claude", "~/.claude"), a("claude2", "~/.claude2")]).is_ok());
        let start = |place: &str| AccountCfg { start: Some(place.into()), ..a("claude2", "~/.claude2") };
        assert!(check_accounts(&[start("background")]).is_ok());
        assert!(check_accounts(&[start("claude-app")]).is_err(), "the Claude app runs ~/.claude only");
        assert!(check_accounts(&[start("the moon")]).is_err());
    }

    #[test]
    fn fills_defaults() {
        let cfg: Config = toml::from_str("").unwrap();
        // The first terminal installed here; Terminal comes with macOS.
        assert_eq!(cfg.apps.terminal, crate::launch::default_terminal());
        assert_eq!(cfg.apps.editor, "cursor");
        assert_eq!(cfg.apps.new_terminal, NewTerminal::Project);
        assert_eq!(cfg.scan, vec!["~/projects".to_string()]);
        assert_eq!(cfg.appearance.theme, Theme::System);
    }

    #[test]
    fn reads_the_theme() {
        let cfg: Config = toml::from_str("[appearance]\ntheme = \"dark\"\n").unwrap();
        assert_eq!(cfg.appearance.theme, Theme::Dark);
        let err = toml::from_str::<Config>("[appearance]\ntheme = \"auto\"\n").unwrap_err().to_string();
        assert!(err.contains("system") && err.contains("light") && err.contains("dark"), "{err}");
    }

    #[test]
    fn setting_the_theme_adds_appearance_after_apps() {
        // A file written before there was a theme: no [appearance] table.
        let text = render(&Config {
            accounts: vec![AccountCfg { name: "claude".into(), harness: "claude".into(), home: "~/.claude".into(), command: Some("claude".into()), start: None, preferred: false, acp: None }],
            projects: vec![ProjectCfg { name: "Spruce".into(), repos: vec!["github.com/sprucehq/*".into()], folders: vec![], chrome_profile: None }],
            ..Config::default()
        });
        let start = text.find(APPEARANCE_COMMENT).unwrap();
        let end = text.find("\n# Harness accounts.").unwrap();
        let old = format!("{}{}", &text[..start], &text[end + 1..]);
        assert!(!old.contains("[appearance]"));

        let out = with_theme(&old, Theme::Dark).unwrap();
        let parsed: Config = toml::from_str(&out).unwrap();
        assert_eq!(parsed.appearance.theme, Theme::Dark);
        assert_eq!(parsed.accounts.len(), 1);
        assert_eq!(parsed.projects.len(), 1);
        assert!(out.find("[apps]").unwrap() < out.find("[appearance]").unwrap());
        assert!(out.find("[appearance]").unwrap() < out.find("[[accounts]]").unwrap());
        assert!(out.contains(&format!("{APPEARANCE_COMMENT}[appearance]\ntheme = \"dark\"\n")));
        // Saving again changes the one value and nothing else.
        assert_eq!(with_theme(&out, Theme::Dark).unwrap(), out);
        assert_eq!(with_theme(&out, Theme::Light).unwrap(), out.replace("theme = \"dark\"", "theme = \"light\""));
    }

    #[test]
    fn setting_the_theme_keeps_comments() {
        let text = "scan = [\"~/code\"]\n\n[appearance]\n# mine\ntheme = \"light\" # for daytime\n\n[apps]\neditor = \"zed\"\n";
        let out = with_theme(text, Theme::System).unwrap();
        assert_eq!(out, text.replace("\"light\"", "\"system\""));
        assert!(with_theme("scan = [", Theme::Dark).is_err());
    }

    #[test]
    fn picking_an_editor_changes_only_the_editor() {
        let text = "scan = [\"~/code\"]\n\n[apps]\n# mine\neditor = \"cursor\" # for now\nterminal = \"ghostty\"\n";
        let out = with_editor(text, "vscode").unwrap();
        assert_eq!(out, text.replace("\"cursor\"", "\"vscode\""));
        // A file without [apps] gets one, commented as a new file's is.
        let out = with_editor("scan = [\"~/code\"]\n", "zed").unwrap();
        assert!(out.contains(&format!("{APPS_COMMENT}[apps]\neditor = \"zed\"\n")), "{out}");
        assert_eq!(toml::from_str::<Config>(&out).unwrap().apps.editor, "zed");
        assert!(with_editor("scan = [", "zed").is_err());
    }

    #[test]
    fn a_start_from_the_quick_panel_sets_only_its_accounts_start() {
        let text = "scan = [\"~/code\"]\n\n# Harness accounts.\n[[accounts]]\nname = \"claude\"\nharness = \"claude\"\nhome = \"~/.claude\"\nstart = \"terminal\" # for now\n\n[[accounts]]\nname = \"claude2\"\nharness = \"claude\"\nhome = \"~/.claude2\"\npreferred = true\n\n[[accounts]]\nname = \"codex\"\nharness = \"codex\"\nhome = \"~/.codex\"\n";
        let out = with_start(text, &["claude".into(), "claude2".into()], "background").unwrap();
        let want = text.replace("\"terminal\" # for now", "\"background\" # for now").replace("preferred = true\n", "preferred = true\nstart = \"background\"\n");
        assert_eq!(out, want);
        let starts: Vec<Option<String>> = toml::from_str::<Config>(&out).unwrap().accounts.into_iter().map(|a| a.start).collect();
        assert_eq!(starts, vec![Some("background".into()), Some("background".into()), None]);
        assert!(with_start("scan = [", &["claude".into()], "terminal").is_err());
    }

    #[test]
    fn a_new_file_has_the_theme() {
        let cfg = Config { appearance: AppearanceCfg { theme: Theme::Light, ..AppearanceCfg::default() }, ..Config::default() };
        let text = render(&cfg);
        assert!(text.contains("[appearance]\ntheme = \"light\"\n"));
        assert_eq!(toml::from_str::<Config>(&text).unwrap(), cfg);
    }

    /// `text` with `item` added to its `scan` list, as a change that keeps everything else would.
    fn scanning(text: &str, item: &str) -> Result<(String, ())> {
        let mut doc: toml_edit::DocumentMut = text.parse()?;
        doc["scan"].as_array_mut().unwrap().push(item);
        Ok((doc.to_string(), ()))
    }

    fn scanned(path: &Path) -> Vec<String> {
        from_text(&std::fs::read_to_string(path).unwrap()).unwrap().scan
    }

    #[test]
    fn changes_to_the_settings_from_two_places_at_once_both_land() {
        let dir = crate::fixtures::scratch("config-update");
        let path = dir.join("config.toml");
        // No file yet: a change starts from a new one's text.
        update_at(&path, |t| with_theme(t, Theme::Dark).map(|t| (t, ()))).unwrap();
        let first = std::fs::read_to_string(&path).unwrap();
        assert_eq!(from_text(&first).unwrap(), Config { appearance: AppearanceCfg { theme: Theme::Dark, ..AppearanceCfg::default() }, ..Config::default() });
        // Settings and the theme buttons, say, each changing the file as the other does.
        let each = 25;
        let threads: Vec<_> = ["a", "b"]
            .into_iter()
            .map(|who| {
                let path = path.clone();
                std::thread::spawn(move || {
                    for i in 0..each {
                        update_at(&path, |t| scanning(t, &format!("{who}{i}"))).unwrap();
                    }
                })
            })
            .collect();
        for t in threads {
            t.join().unwrap();
        }
        let scan = scanned(&path);
        for who in ["a", "b"] {
            assert_eq!(scan.iter().filter(|s| s.starts_with(who)).count(), each, "{scan:?}");
        }
        assert_eq!(from_text(&std::fs::read_to_string(&path).unwrap()).unwrap().appearance.theme, Theme::Dark, "the first change is kept too");
    }

    #[test]
    fn a_change_saved_by_hand_meanwhile_is_changed_in_turn() {
        let dir = crate::fixtures::scratch("config-by-hand");
        let path = dir.join("config.toml");
        std::fs::write(&path, "scan = [\"~/projects\"]\n").unwrap();
        // An agent saves the file, taking no lock, while the theme is being set.
        let mut tries = 0;
        update_at(&path, |t| {
            tries += 1;
            if tries == 1 {
                std::fs::write(&path, "scan = [\"~/projects\", \"~/work\"]\n").unwrap();
            }
            scanning(t, "~/src")
        })
        .unwrap();
        assert_eq!(tries, 2);
        assert_eq!(scanned(&path), ["~/projects", "~/work", "~/src"]);
        // One that changes under every try is left as it is.
        let mut n = 0;
        let e = update_at(&path, |t| {
            n += 1;
            std::fs::write(&path, format!("scan = [\"{n}\"]\n")).unwrap();
            scanning(t, "~/src")
        })
        .unwrap_err();
        assert!(e.to_string().contains("kept changing while Codealong saved it"), "{e}");
        assert_eq!(scanned(&path), ["3"]);
    }

    #[test]
    fn a_config_that_cannot_be_read_is_never_replaced_by_the_defaults() {
        let dir = crate::fixtures::scratch("config-unreadable");
        let path = dir.join("config.toml");
        // Not UTF-8: read_to_string fails, as it does for a file without permission to read it.
        let bytes = b"[[accounts]]\nname = \"w\xffrk\"\n".to_vec();
        std::fs::write(&path, &bytes).unwrap();
        let e = update_at(&path, |t| with_theme(t, Theme::Light).map(|t| (t, ()))).unwrap_err().to_string();
        assert!(e.starts_with("Couldn't read ") && e.contains("config.toml"), "{e}");
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        // A change that can't be made leaves the file as it is too.
        std::fs::write(&path, "scan = [").unwrap();
        assert!(update_at(&path, |t| with_theme(t, Theme::Light).map(|t| (t, ()))).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "scan = [");
    }

    #[test]
    fn zooming_steps_the_file_in_place_and_only_a_zoomed_one_says_it() {
        use crate::zoom::Step;
        let text = render(&Config::default());
        assert!(!text.contains("zoom ="), "actual size isn't written");
        let (bigger, zoom) = with_zoom_step(&text, Step::In).unwrap();
        assert_eq!(zoom, 110);
        assert!(bigger.contains("[appearance]\ntheme = \"system\"\nzoom = 110\n"), "{bigger}");
        assert_eq!(parse(&bigger).unwrap().appearance.zoom, 110);
        // Back to actual size, a file that says its zoom keeps saying it.
        let (back, zoom) = with_zoom_step(&bigger, Step::Actual).unwrap();
        assert_eq!((zoom, back.contains("zoom = 100\n")), (100, true));
        assert_eq!(with_zoom_step(&text, Step::Actual).unwrap(), (text.clone(), 100));
        // A zoom written by hand steps to the next one, with its comment kept; a file without
        // [appearance] gets the table.
        let (out, zoom) = with_zoom_step("[appearance]\nzoom = 115 # big screen\n", Step::Out).unwrap();
        assert_eq!((out.as_str(), zoom), ("[appearance]\nzoom = 110 # big screen\n", 110));
        let (out, _) = with_zoom_step("scan = []\n\n[apps]\nterminal = \"ghostty\"\n", Step::Out).unwrap();
        assert!(out.contains(&format!("{APPEARANCE_COMMENT}[appearance]\nzoom = 90\n")), "{out}");
        assert!(with_zoom_step("scan = [", Step::In).is_err());
        assert!(parse("[appearance]\nzoom = 400\n").unwrap_err().to_string().contains("zoom is 400"));
    }

    #[test]
    fn devices_are_opt_in_and_saved_in_place() {
        // A file from before [devices] doesn't get the table until something in it changes.
        let text = render(&full());
        assert!(!text.contains("[devices]"));
        assert!(!with_config(&text, &full()).unwrap().contains("[devices]"));

        let on = Config { devices: DevicesCfg { enabled: true, remote_control: true, ..DevicesCfg::default() }, ..full() };
        let out = with_config(&text, &on).unwrap();
        assert!(out.contains(DEVICES_COMMENT), "{out}");
        assert!(out.contains("[devices]\nenabled = true\ntailscale = true\niroh = true\nremote_control = true\n"), "{out}");
        assert!(out.find("[devices]").unwrap() > out.find("[search]").unwrap_or(0));
        assert!(!out.contains("port ="), "the default port isn't written");
        assert_eq!(parse(&out).unwrap().devices, on.devices);

        // Turning it back off keeps the table, with the value changed in place.
        let off = with_config(&out, &full()).unwrap();
        assert!(off.contains("[devices]\nenabled = false"));
        assert!(parse(&"[devices]\nport = 80\n".to_string()).is_err(), "ports under 1024 are refused");
    }

    fn full() -> Config {
        Config {
            scan: vec!["~/projects".into(), "~/work".into()],
            apps: AppsCfg::default(),
            appearance: AppearanceCfg { theme: Theme::Dark, ..AppearanceCfg::default() },
            shortcuts: ShortcutsCfg::default(),
            accounts: vec![
                AccountCfg { name: "claude".into(), harness: "claude".into(), home: "~/.claude".into(), command: Some("claude".into()), start: None, preferred: false, acp: None },
                AccountCfg { name: "codex".into(), harness: "codex".into(), home: "~/.codex".into(), command: None, start: None, preferred: false, acp: None },
            ],
            projects: vec![
                ProjectCfg { name: "Spruce".into(), repos: vec!["github.com/sprucehq/*".into()], folders: vec!["~/projects/sparkhq/vault".into()], chrome_profile: None },
                ProjectCfg { name: "Spur".into(), repos: vec!["github.com/spurhq/*".into()], folders: vec![], chrome_profile: None },
            ],
            jev: JevCfg::default(),
            router: None,
            search: SearchCfg::default(),
            devices: DevicesCfg::default(),
            updates: UpdatesCfg::default(),
            limits: LimitsCfg::default(),
            sessions: SessionsCfg::default(),
            reviews: ReviewsCfg::default(),
            pull_requests: PullRequestsCfg::default(),
            debug: DebugCfg { perf_log: true },
        }
    }

    #[test]
    fn a_limit_is_suggested_unless_the_file_says_to_move() {
        assert_eq!(toml::from_str::<Config>("").unwrap().limits.when_hit, WhenLimited::Suggest);
        let old = render(&full());
        assert!(!old.contains("[limits]"), "{old}");
        assert!(!with_config(&old, &full()).unwrap().contains("[limits]"));
        let moving = Config { limits: LimitsCfg { when_hit: WhenLimited::Move }, ..full() };
        let out = with_config(&old, &moving).unwrap();
        assert!(out.contains("[limits]\nwhen_hit = \"move\"\n"), "{out}");
        assert_eq!(parse(&out).unwrap().limits.when_hit, WhenLimited::Move);
        // Back to suggesting keeps the table, the value changed in place.
        assert!(with_config(&out, &full()).unwrap().contains("[limits]\nwhen_hit = \"suggest\""));
        assert!(parse(&"[limits]\nwhen_hit = \"wait\"\n".to_string()).is_err());
    }

    #[test]
    fn a_file_from_when_nudging_was_a_setting_still_reads_and_loses_it_when_saved() {
        let old = format!("{}\n[pull_requests]\nwhen_approved = \"nudge\"\n", render(&full()));
        assert_eq!(parse(&old).unwrap(), full());
        let out = with_config(&old, &full()).unwrap();
        assert!(!out.contains("pull_requests]") && !out.contains("when_approved"), "{out}");
        // A table someone keeps other things in stays, without the key.
        let kept = with_config(&format!("{old}theirs = 1\n"), &full()).unwrap();
        assert!(kept.contains("[pull_requests]\ntheirs = 1") && !kept.contains("when_approved"), "{kept}");
        // Nudging by default takes its place.
        let on = Config { pull_requests: PullRequestsCfg { nudge_by_default: true, ..Default::default() }, ..full() };
        let out = with_config(&old, &on).unwrap();
        assert!(out.contains("[pull_requests]\nnudge_by_default = true\n") && !out.contains("when_approved"), "{out}");
    }

    #[test]
    fn nudging_by_default_is_off_until_the_file_turns_it_on() {
        assert!(!toml::from_str::<Config>("").unwrap().pull_requests.nudge_by_default);
        let old = render(&full());
        assert!(!old.contains("[pull_requests]"), "{old}");
        assert!(!with_config(&old, &full()).unwrap().contains("[pull_requests]"));
        let on = Config { pull_requests: PullRequestsCfg { nudge_by_default: true, ..Default::default() }, reviews: ReviewsCfg { auto: true, ..ReviewsCfg::default() }, ..full() };
        let out = with_config(&old, &on).unwrap();
        assert!(out.contains(&format!("{PULL_REQUESTS_COMMENT}[pull_requests]\nnudge_by_default = true\n")), "{out}");
        assert!(out.find("[reviews]").unwrap() < out.find("[pull_requests]").unwrap(), "{out}");
        assert!(out.find("[pull_requests]").unwrap() < out.find("[[accounts]]").unwrap(), "{out}");
        assert_eq!(parse(&out).unwrap(), on);
        assert_eq!(parse(&render(&on)).unwrap(), on);
        // Turned off again, the table stays, saying so.
        let off = with_config(&out, &Config { reviews: on.reviews.clone(), ..full() }).unwrap();
        assert!(off.contains("[pull_requests]\nnudge_by_default = false"), "{off}");
        // What a nudge tells of is everything until the file says otherwise, with or without
        // nudging by default.
        assert_eq!(toml::from_str::<Config>("").unwrap().pull_requests.nudge_about, NudgeAbout::Everything);
        let feedback = Config { pull_requests: PullRequestsCfg { nudge_by_default: false, nudge_about: NudgeAbout::Feedback }, ..full() };
        let out = with_config(&old, &feedback).unwrap();
        assert!(out.contains("[pull_requests]\nnudge_by_default = false\nnudge_about = \"feedback\"\n"), "{out}");
        assert_eq!(parse(&out).unwrap(), feedback);
        assert_eq!(parse(&render(&feedback)).unwrap(), feedback);
    }


    #[test]
    fn updates_are_on_unless_the_file_turns_them_off() {
        assert!(toml::from_str::<Config>("").unwrap().updates.check);
        let old = render(&full());
        assert!(!old.contains("[updates]"), "{old}");
        assert!(!with_config(&old, &full()).unwrap().contains("[updates]"));
        let mut off = full();
        off.updates.check = false;
        let out = with_config(&old, &off).unwrap();
        assert!(out.contains(&format!("{UPDATES_COMMENT}[updates]\ncheck = false")), "{out}");
        assert!(out.find("[updates]").unwrap() < out.find("[[accounts]]").unwrap(), "{out}");
        assert!(!toml::from_str::<Config>(&out).unwrap().updates.check);
        // Turned back on, the table stays, saying so.
        let on = with_config(&out, &full()).unwrap();
        assert!(on.contains("[updates]\ncheck = true"), "{on}");
    }

    #[test]
    fn continuing_after_a_network_error_is_on_unless_the_file_turns_it_off() {
        assert!(toml::from_str::<Config>("").unwrap().sessions.continue_after_network_error);
        let old = render(&full());
        assert!(!old.contains("[sessions]"), "{old}");
        assert!(!with_config(&old, &full()).unwrap().contains("[sessions]"));
        let mut off = full();
        off.sessions.continue_after_network_error = false;
        let out = with_config(&old, &off).unwrap();
        assert!(out.contains(&format!("{SESSIONS_COMMENT}[sessions]\ncontinue_after_network_error = false")), "{out}");
        assert!(out.find("[sessions]").unwrap() < out.find("[[accounts]]").unwrap(), "{out}");
        assert_eq!(parse(&out).unwrap(), off);
        assert_eq!(parse(&render(&off)).unwrap(), off);
        // Turned back on, the table stays, saying so.
        let on = with_config(&out, &full()).unwrap();
        assert!(on.contains("[sessions]\ncontinue_after_network_error = true"), "{on}");
    }

    #[test]
    fn reviews_are_off_until_the_file_turns_them_on_and_name_an_account_it_has() {
        let none = toml::from_str::<Config>("").unwrap().reviews;
        assert_eq!(none, ReviewsCfg::default());
        assert_eq!((none.auto, none.account.as_str(), none.model.as_str(), none.prompt.as_str(), none.post), (false, "", "", "", ReviewPost::Draft));
        let old = render(&full());
        assert!(!old.contains("[reviews]"), "{old}");
        assert!(!with_config(&old, &full()).unwrap().contains("[reviews]"));
        let mut on = full();
        on.reviews = ReviewsCfg { auto: true, account: "claude".into(), model: "opus".into(), prompt: "Start two subagents, on Opus and Sonnet.\nCombine what they find.".into(), post: ReviewPost::Comment, submit: ReviewSubmit::ApproveOrComment, auto_approve: false, reviewers: Vec::new(), own: OwnReviewsCfg::default() };
        let out = with_config(&old, &on).unwrap();
        assert!(out.contains(&format!("{REVIEWS_COMMENT}[reviews]\nauto = true\naccount = \"claude\"\nmodel = \"opus\"\nprompt = ")), "{out}");
        assert!(out.find("[reviews]").unwrap() < out.find("[[accounts]]").unwrap(), "{out}");
        assert_eq!(parse(&out).unwrap(), on);
        assert_eq!(parse(&render(&on)).unwrap(), on);
        // Turned off again, the table stays, saying so.
        let off = with_config(&out, &full()).unwrap();
        assert!(off.contains("[reviews]\nauto = false\naccount = \"\"\nmodel = \"\"\nprompt = \"\"\npost = \"draft\""), "{off}");
        // A file from the day several reviewers could be named: the first is the reviewer, and
        // what's gone is dropped as it's saved.
        let then = format!("{old}\n[reviews]\nauto = true\nreviewers = [{{ account = \"codex\", model = \"gpt-5.5\" }}, {{ account = \"claude\" }}]\ncombine = true\ncomment = false\n");
        let read = parse(&then).unwrap().reviews;
        assert_eq!((read.account.as_str(), read.model.as_str(), read.reviewers.len()), ("codex", "gpt-5.5", 0));
        let saved = with_config(&then, &parse(&then).unwrap()).unwrap();
        assert!(!saved.contains("reviewers =") && !saved.contains("combine =") && !saved.contains("comment =") && saved.contains("account = \"codex\""), "{saved}");
        // A file from before `submit`: auto-approving is `submit = "approve"`, written so as it's
        // saved, unless the file says `submit` itself.
        let then = format!("{old}\n[reviews]\nauto = true\nauto_approve = true\n");
        assert_eq!(parse(&then).unwrap().reviews.submit, ReviewSubmit::Approve);
        let saved = with_config(&then, &parse(&then).unwrap()).unwrap();
        assert!(!saved.contains("auto_approve") && saved.contains("submit = \"approve\""), "{saved}");
        assert_eq!(parse(&format!("{old}\n[reviews]\nauto_approve = false\n")).unwrap().reviews.submit, ReviewSubmit::Never);
        assert_eq!(parse(&format!("{old}\n[reviews]\nauto_approve = true\nsubmit = \"never\"\n")).unwrap().reviews.submit, ReviewSubmit::Never);
        assert_eq!(parse(&format!("{old}\n[reviews]\nsubmit = \"approve-or-request-changes\"\n")).unwrap().reviews.submit, ReviewSubmit::ApproveOrRequestChanges);
        // An account that isn't listed can't review, nor a model name that can't be typed.
        assert_eq!(check(&on), Ok(()));
        let mut bad = on.clone();
        bad.reviews.account = "claude9".into();
        assert!(check(&bad).unwrap_err().contains("claude9"));
        let mut bad = on.clone();
        bad.reviews.model = "opus; rm".into();
        assert!(check(&bad).unwrap_err().contains("isn't a model name"));
    }

    #[test]
    fn reviews_of_your_own_prs_are_a_table_of_their_own_under_reviews() {
        let none = toml::from_str::<Config>("").unwrap().reviews.own;
        assert_eq!(none, OwnReviewsCfg::default());
        // Each review goes back to the PR's session unless that's turned off.
        assert!(none.send && !none.auto);
        let old = render(&full());
        let mut on = full();
        on.reviews.own = OwnReviewsCfg { auto: true, account: "codex".into(), model: "gpt-5.5".into(), prompt: "Look hard at the tests.".into(), send: true };
        let out = with_config(&old, &on).unwrap();
        assert!(out.contains(&format!("{OWN_REVIEWS_COMMENT}[reviews.own]\nauto = true\naccount = \"codex\"\nmodel = \"gpt-5.5\"\nprompt = \"Look hard at the tests.\"\nsend = true")), "{out}");
        // Right after [reviews], whose own lines stay as they were.
        assert!(out.find("[reviews]\nauto = false").unwrap() < out.find("[reviews.own]").unwrap() && out.find("[reviews.own]").unwrap() < out.find("[[accounts]]").unwrap(), "{out}");
        assert_eq!(parse(&out).unwrap(), on);
        assert_eq!(parse(&render(&on)).unwrap(), on);
        // Kept as it's changed, sending turned off.
        let mut quiet = on.clone();
        quiet.reviews.own.send = false;
        let again = with_config(&out, &quiet).unwrap();
        assert_eq!(again.matches("\n[reviews.own]\n").count(), 1, "{again}");
        assert_eq!(parse(&again).unwrap(), quiet);
        let mut bad = on.clone();
        bad.reviews.own.account = "codex9".into();
        assert!(check(&bad).unwrap_err().contains("[reviews.own] account"));
    }

    #[test]
    fn search_days_are_added_when_changed_and_capped() {
        let old = render(&full()).replace(&format!("\n{SEARCH_COMMENT}[search]\ndays = 14\n"), "");
        assert!(!old.contains("[search]"), "{old}");
        assert!(!with_config(&old, &full()).unwrap().contains("[search]"));
        let mut week = full();
        week.search.days = 7;
        let out = with_config(&old, &week).unwrap();
        assert!(out.contains("[search]\ndays = 7"), "{out}");
        assert!(out.find("[search]").unwrap() > out.find("[shortcuts]").unwrap() && out.find("[search]").unwrap() < out.find("[[accounts]]").unwrap(), "{out}");
        assert_eq!(toml::from_str::<Config>(&out).unwrap().search.days, 7);
        assert_eq!(toml::from_str::<Config>("").unwrap().search.days, 14);
        week.search.days = 30;
        assert!(check(&week).unwrap_err().contains("[search] days"));
    }

    #[test]
    fn jev_is_added_after_the_shortcuts_only_once_something_is_on() {
        let text = render(&full());
        assert!(!text.contains("[jev]"), "{text}");
        assert!(!with_config(&text, &full()).unwrap().contains("[jev]"));
        let mut on = full();
        on.jev.sessions = true;
        let out = with_config(&text, &on).unwrap();
        let shortcuts = out.find("[shortcuts]").unwrap();
        let jev = out
            .find(&format!("{JEV_COMMENT}[jev]\nmodel = \"jev-latest\"\nquick_panel = false\nsessions = true\ntodos = false\npull_requests = false\n"))
            .unwrap_or_else(|| panic!("{out}"));
        assert!(jev > shortcuts && jev < out.find("[[accounts]]").unwrap(), "{out}");
        // Turned off again, the table stays and says so.
        assert!(with_config(&out, &full()).unwrap().contains("sessions = false\n"));
        assert_eq!(parse(&out).unwrap().jev, on.jev);
    }

    #[test]
    fn an_old_router_table_turns_the_quick_panel_on_and_gives_way_to_jev() {
        let old = format!("{}\n[router]\nenabled = true\nmodel = \"jev-1.13.0\"\n", render(&full()));
        let cfg = parse(&old).unwrap();
        assert!(cfg.jev.quick_panel && !cfg.jev.sessions);
        assert_eq!(cfg.jev.model, "jev-1.13.0");
        let out = with_config(&old, &cfg).unwrap();
        assert!(!out.contains("[router]") && out.contains("[jev]\nmodel = \"jev-1.13.0\"\nquick_panel = true\n"), "{out}");
    }

    #[test]
    fn saving_everything_sets_each_value_in_place() {
        let text = render(&full()).replace("[apps]\n", "[apps]\n# my editor of choice\n").replace("scan = ", "# roots\nscan = ");
        let mut next = full();
        next.scan = vec!["~/code".into()];
        next.apps.editor = "zed".into();
        next.appearance.theme = Theme::Light;
        next.shortcuts.quick_panel = "Cmd+Ctrl+K".into();
        next.debug.perf_log = false;
        next.accounts.pop();
        next.projects[1].folders.push("~/extra".into());
        next.projects[0].chrome_profile = Some("Work".into());
        next.projects.push(ProjectCfg { name: "Codealong".into(), repos: vec!["github.com/codealong/*".into()], folders: vec![], chrome_profile: None });
        let out = with_config(&text, &next).unwrap();
        assert_eq!(toml::from_str::<Config>(&out).unwrap(), next);
        for kept in ["# my editor of choice", "# roots", "# Harness accounts.", "# Projects group repos", APPS_COMMENT, DEBUG_COMMENT] {
            assert!(out.contains(kept), "lost {kept:?} in:\n{out}");
        }
        // The file keeps its order: scan, apps, appearance, shortcuts, accounts, projects, debug.
        let at = |s: &str| out.find(s).unwrap_or_else(|| panic!("no {s:?} in:\n{out}"));
        assert!(at("scan =") < at("[apps]") && at("[apps]") < at("[appearance]") && at("[appearance]") < at("[shortcuts]") && at("[shortcuts]") < at("[[accounts]]"));
        assert!(at("[[accounts]]") < at("[[projects]]") && at("[[projects]]") < at("[debug]"));
        // Saving the same settings again changes nothing.
        assert_eq!(with_config(&out, &next).unwrap(), out);
    }

    #[test]
    fn saving_everything_adds_what_a_bare_file_lacks() {
        let out = with_config("scan = [\"~/code\"]\n", &full()).unwrap();
        assert_eq!(toml::from_str::<Config>(&out).unwrap(), full());
        assert!(out.contains(&format!("{APPS_COMMENT}[apps]\n")), "{out}");
        assert!(out.contains(&format!("{DEBUG_COMMENT}[debug]\nperf_log = true\n")), "{out}");
        assert!(with_config("scan = [", &full()).is_err());
    }

    #[test]
    fn checks_what_the_form_and_the_text_can_save() {
        assert!(check(&full()).is_ok());
        let mut bad = full();
        bad.projects[1].name = "Spruce".into();
        assert!(check(&bad).unwrap_err().contains("Two projects"));
        let mut bad = full();
        bad.projects[0].repos.push(" ".into());
        assert!(check(&bad).unwrap_err().contains("empty"));
        let mut bad = full();
        bad.projects[0].chrome_profile = Some(" ".into());
        assert!(check(&bad).unwrap_err().contains("chrome_profile is empty"));
        let mut bad = full();
        bad.scan.push(String::new());
        assert!(check(&bad).is_err());
        assert!(parse("scan = [").unwrap_err().to_string().contains("expected"));
        assert!(parse("[[accounts]]\nname = \"a b\"\nharness = \"claude\"\nhome = \"~/.claude\"\n").is_err());
        assert_eq!(parse(&render(&full())).unwrap(), full());
    }

    #[test]
    fn a_file_without_shortcuts_gets_them_after_appearance() {
        // A file written before there were shortcuts.
        let text = render(&full());
        let start = text.find(SHORTCUTS_COMMENT).unwrap();
        let end = text.find("\n# Harness accounts.").unwrap();
        let old = format!("{}{}", &text[..start], &text[end + 1..]);
        assert!(!old.contains("[shortcuts]"));
        assert_eq!(toml::from_str::<Config>(&old).unwrap().shortcuts.quick_panel, DEFAULT_QUICK_PANEL);

        let mut next = full();
        next.shortcuts.quick_panel = "Ctrl+Alt+Space".into();
        let out = with_config(&old, &next).unwrap();
        assert_eq!(toml::from_str::<Config>(&out).unwrap(), next);
        assert!(out.contains(&format!("{SHORTCUTS_COMMENT}[shortcuts]\nquick_panel = \"Ctrl+Alt+Space\"\n")), "{out}");
        assert!(out.find("[appearance]").unwrap() < out.find("[shortcuts]").unwrap());
        assert!(out.find("[shortcuts]").unwrap() < out.find("[[accounts]]").unwrap());
        // Turned off, it stays in the file as an empty string.
        next.shortcuts.quick_panel = String::new();
        let off = with_config(&out, &next).unwrap();
        assert!(off.contains("quick_panel = \"\"\n"), "{off}");
        assert_eq!(toml::from_str::<Config>(&off).unwrap().shortcuts.quick_panel, "");
    }

    #[test]
    fn checks_the_shortcut() {
        let mut cfg = full();
        for ok in ["Option+Cmd+/", "Option+Shift+Space", "Cmd+Ctrl+K", "ctrl+alt+space", "F18", ""] {
            cfg.shortcuts.quick_panel = ok.into();
            assert!(check(&cfg).is_ok(), "{ok:?}: {:?}", check(&cfg));
        }
        for bad in ["Space", "Shift+K", "Cmd+Shift", "Option+Shift+Spacebar", "Cmd+Tab"] {
            cfg.shortcuts.quick_panel = bad.into();
            let err = check(&cfg).unwrap_err();
            assert!(err.starts_with("[shortcuts] quick_panel: "), "{bad:?}: {err}");
        }
        cfg.shortcuts.quick_panel = DEFAULT_QUICK_PANEL.into();
        cfg.shortcuts.waiting = "Cmd+K".into();
        assert!(check(&cfg).unwrap_err().starts_with("[shortcuts] waiting: "));
        // One press can't do both, however it's written; both off is fine.
        cfg.shortcuts.waiting = "cmd+option+/".into();
        assert!(check(&cfg).unwrap_err().contains("same keys"));
        cfg.shortcuts = ShortcutsCfg { quick_panel: String::new(), waiting: String::new() };
        assert!(check(&cfg).is_ok());
    }

    #[test]
    fn a_file_from_before_the_waiting_shortcut_has_its_default() {
        let cfg: Config = toml::from_str("[shortcuts]\nquick_panel = \"Cmd+Ctrl+K\"\n").unwrap();
        assert_eq!(cfg.shortcuts, ShortcutsCfg { quick_panel: "Cmd+Ctrl+K".into(), waiting: DEFAULT_WAITING.into() });
        let text = render(&Config::default());
        assert!(text.contains("[shortcuts]\nquick_panel = \"Option+Cmd+/\"\nwaiting = \"Option+Cmd+.\"\n"), "{text}");
    }

    #[test]
    fn a_new_file_has_the_debug_table() {
        let text = render(&Config::default());
        assert!(text.ends_with("[debug]\nperf_log = false\n"), "{text}");
        assert_eq!(view_of(&text, "").error, None);
        let broken = view_of("scan = [", "~/x/config.toml");
        assert!(broken.error.is_some() && broken.config == Config::default() && broken.text == "scan = [");
    }

    #[test]
    fn suggests_projects_by_owner() {
        let mut owners = BTreeMap::new();
        owners.insert("spurhq".to_string(), 3);
        owners.insert("sprucehq".to_string(), 9);
        owners.insert("gwen".to_string(), 2);
        let names: BTreeMap<&str, &str> = [("sprucehq", "Spruce"), ("spurhq", "Spur")].into();
        let p = suggest_projects(&owners, |o| names.get(o).map(|n| n.to_string()));
        assert_eq!(p[0].name, "gwen");
        assert_eq!(p[1].name, "Spruce");
        assert_eq!(p[2].name, "Spur");
        assert_eq!(p[2].repos, vec!["github.com/spurhq/*".to_string()]);
    }
}
