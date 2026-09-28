//! `config.toml`: apps, theme, accounts, projects and scan roots. Codealong writes it on first
//! run from what it finds. After that you edit it, or Settings does: its form sets each value
//! in place, leaving the rest of the file, comments included, as you wrote it; its config.toml
//! tab saves the text as typed, once it parses.

use std::collections::BTreeMap;
use std::path::PathBuf;

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

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct DebugCfg {
    /// Appends timings to ~/Library/Logs/Codealong/perf.log (see perf.rs).
    #[serde(default)]
    pub perf_log: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct AppearanceCfg {
    #[serde(default)]
    pub theme: Theme,
}

/// Keyboard shortcuts that work whichever app is in front. An empty one is off.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ShortcutsCfg {
    /// Opens the quick panel: find a session, or start one.
    #[serde(default = "default_quick_panel")]
    pub quick_panel: String,
}

impl Default for ShortcutsCfg {
    fn default() -> Self {
        ShortcutsCfg { quick_panel: default_quick_panel() }
    }
}

/// ⌥⌘/, / for search as in many apps. The other / keys are taken: ⌘/ comments out lines in
/// editors, ⌘? searches the Help menu, ⌃/ is undo in shells (terminals send it as ^_), and ⌥/
/// and ⌥⇧/ type ÷ and ¿. Written as Settings records it, so its "Use …" button knows it.
pub const DEFAULT_QUICK_PANEL: &str = "Option+Cmd+/";

fn default_quick_panel() -> String {
    DEFAULT_QUICK_PANEL.to_string()
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

/// Where a terminal Codealong opens goes: a window of its own, a tab in the front window, or a
/// tab in a window already showing the same worktree, repo or project (a window of its own when
/// none does).
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
}

fn default_scan() -> Vec<String> {
    vec!["~/projects".to_string()]
}
fn default_terminal() -> String {
    "ghostty".to_string()
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

/// Loads the config, or `None` when there isn't one yet.
pub fn load() -> Result<Option<Config>> {
    read_text()?.map(|text| load_text(&text)).transpose()
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

/// The quick panel's shortcut config.toml asks for: the default when there's no file yet or it
/// has an error.
pub fn quick_shortcut() -> String {
    load().ok().flatten().map(|c| c.shortcuts.quick_panel).unwrap_or_else(default_quick_panel)
}

/// Writes config.toml afresh. Returns the text written.
pub fn write(cfg: &Config) -> Result<String> {
    std::fs::create_dir_all(support_dir())?;
    let text = render(cfg);
    write_atomic(&text)?;
    Ok(text)
}

/// Writes config.toml through a temporary file, so the watcher never reads half a file.
fn write_atomic(text: &str) -> Result<()> {
    let path = config_path();
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, text).with_context(|| format!("writing {}", tmp.display()))?;
    std::fs::rename(&tmp, &path).with_context(|| format!("saving {}", path.display()))?;
    Ok(())
}

pub const HARNESSES: [&str; 4] = ["claude", "codex", "opencode", "antigravity"];

/// Why a list of accounts can't be saved, in words.
pub fn check_accounts(accounts: &[AccountCfg]) -> std::result::Result<(), String> {
    let mut names = std::collections::HashSet::new();
    let mut homes = std::collections::HashSet::new();
    for a in accounts {
        if a.name.is_empty() || !a.name.chars().all(|c| c.is_ascii_alphanumeric() || "._-".contains(c)) {
            return Err(format!("\"{}\" can't be an account name. Use letters, numbers, dots, dashes or underscores.", a.name));
        }
        if !HARNESSES.contains(&a.harness.as_str()) {
            return Err(format!("{} needs a harness: claude, codex, opencode or antigravity.", a.name));
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
    std::fs::create_dir_all(support_dir())?;
    let path = config_path();
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(_) => render(&Config::default()),
    };
    write_atomic(&with_accounts(&text, accounts)?)
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

const APPS_COMMENT: &str = "# Where Codealong opens things. terminal: \"ghostty\". editor: \"vscode\", \"cursor\", \"zed\", \"xcode\" and the others\n# Settings lists, or any app's name. Picking another editor from an Open in menu sets it here.\n# new_terminal: \"window\", \"tab\" (in the front window), or \"project\" (a tab beside the same repo or project).\n# move_pointer: move the pointer to a terminal Codealong brings forward on another screen.\n";
const APPEARANCE_COMMENT: &str = "# Light or dark. \"system\" follows macOS. The buttons at the foot of the sidebar set this too.\n";
const JEV_COMMENT: &str = "# What Codealong asks TypeSafe's Jev, feature by feature (Settings › Jev says what each one sends).\n# The API key is kept in the login keychain; set it in Settings.\n";
const SEARCH_COMMENT: &str = "# The quick panel and `codealong sessions find` search what was said in sessions active in the\n# last `days` days, not just their titles. 0 turns it off; 14, the board's reach, is the most.\n";
const DEVICES_COMMENT: &str = "# Your other devices: Codealong on each shows the others' sessions and can read and search them.\n# tailscale finds them on your tailnet; iroh reaches devices you link with a code, anywhere.\n# remote_control lets your linked devices send prompts, answer approvals and start sessions here.\n";
const DEBUG_COMMENT: &str = "# Timings for finding what makes the app slow, in ~/Library/Logs/Codealong/perf.log.\n";
const SHORTCUTS_COMMENT: &str = "# Shortcuts that work in any app, such as \"Option+Cmd+/\" or \"Cmd+Ctrl+K\". \"\" turns one off.\n# quick_panel opens the quick panel: find a session, or start one.\n";

/// Sets `[apps] editor` in config.toml and keeps everything else as written: an editor picked
/// from an Open in menu is the one Open in <editor> uses from then on.
pub fn save_editor(id: &str) -> Result<()> {
    std::fs::create_dir_all(support_dir())?;
    let text = match std::fs::read_to_string(config_path()) {
        Ok(t) => t,
        Err(_) => render(&Config::default()),
    };
    write_atomic(&with_editor(&text, id)?)
}

pub fn with_editor(text: &str, id: &str) -> Result<String> {
    let mut doc: toml_edit::DocumentMut = text.parse().context("config.toml has an error; fix it before changing the editor")?;
    ensure_table(&mut doc, "apps", APPS_COMMENT);
    set_value(&mut doc["apps"]["editor"], toml_edit::Value::from(id));
    Ok(doc.to_string())
}

/// Sets `[appearance] theme` in config.toml and keeps everything else as written.
pub fn save_theme(theme: Theme) -> Result<()> {
    std::fs::create_dir_all(support_dir())?;
    let text = match std::fs::read_to_string(config_path()) {
        Ok(t) => t,
        Err(_) => render(&Config::default()),
    };
    write_atomic(&with_theme(&text, theme)?)
}

/// `text` with its theme set. A file from before there was a theme gets an `[appearance]`
/// table after `[apps]`, commented as a new file would be; a comment after the value stays.
pub fn with_theme(text: &str, theme: Theme) -> Result<String> {
    let mut doc: toml_edit::DocumentMut = text.parse().context("config.toml has an error; fix it before changing the theme")?;
    set_theme(&mut doc, theme);
    Ok(doc.to_string())
}

fn set_theme(doc: &mut toml_edit::DocumentMut, theme: Theme) {
    use toml_edit::{Item, Table, Value};
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
    set_value(&mut doc["appearance"]["theme"], Value::from(theme.as_str()));
}

/// Sets `[shortcuts] quick_panel`. A file from before there were shortcuts gets the table after
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

/// What the Settings form offers for `[apps]`: the built-in terminal profiles (see launch.rs),
/// and the editors installed here (editors.rs).
pub fn terminal_profiles() -> Vec<Profile> {
    vec![Profile { id: "ghostty".into(), name: "Ghostty".into() }]
}
pub fn editor_profiles() -> Vec<Profile> {
    crate::editors::installed()
}

/// What Settings works from: the settings as parsed, the file as written, and why it couldn't
/// be parsed, if it couldn't (then `config` is the defaults and the form stays read-only).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigView {
    pub config: Config,
    pub text: String,
    pub path: String,
    pub error: Option<String>,
    pub terminals: Vec<Profile>,
    pub editors: Vec<Profile>,
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
    ConfigView { config, text: text.to_string(), path: path.to_string(), error, terminals: terminal_profiles(), editors: editor_profiles() }
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
    if cfg.devices.port < 1024 {
        return Err(format!("[devices] port is {}; use one from 1024 up ({DEFAULT_DEVICES_PORT} is the default).", cfg.devices.port));
    }
    if cfg.devices.name.chars().count() > 64 || cfg.devices.name.chars().any(char::is_control) {
        return Err("[devices] name is too long or has control characters.".into());
    }
    if cfg.apps.terminal.trim().is_empty() || cfg.apps.editor.trim().is_empty() {
        return Err("The terminal and editor need a profile name.".into());
    }
    crate::quick::parse_shortcut(&cfg.shortcuts.quick_panel).map_err(|e| format!("[shortcuts] quick_panel: {e}"))?;
    Ok(())
}

/// Saves every setting in config.toml, in place. The watcher sees the change and reloads.
pub fn save(cfg: &Config) -> Result<()> {
    check(cfg).map_err(anyhow::Error::msg)?;
    std::fs::create_dir_all(support_dir())?;
    let text = match std::fs::read_to_string(config_path()) {
        Ok(t) => t,
        Err(_) => render(&Config::default()),
    };
    write_atomic(&with_config(&text, cfg)?)
}

/// Saves config.toml as typed, once it parses as settings.
pub fn save_text(text: &str) -> Result<()> {
    parse(text)?;
    std::fs::create_dir_all(support_dir())?;
    write_atomic(text)
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
    s.push('\n');
    s.push_str(SHORTCUTS_COMMENT);
    s.push_str("[shortcuts]\n");
    s.push_str(&format!("quick_panel = {}\n", q(&cfg.shortcuts.quick_panel)));
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
    s.push_str("\n# Harness accounts. `name` is what you type and what the board shows.\n# `start` is where new sessions start: \"terminal\", \"background\", \"claude-app\", \"codex-app\" or \"codealong\".\n# `acp`: the command that runs the account's Agent Client Protocol adapter, for sessions Codealong runs itself.\n# `preferred = true`: Auto starts this account over ones without it while it has room.\n");
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

    #[test]
    fn renders_and_parses_back() {
        let cfg = Config {
            scan: vec!["~/projects".into()],
            apps: AppsCfg::default(),
            appearance: AppearanceCfg { theme: Theme::Dark },
            shortcuts: ShortcutsCfg { quick_panel: "Cmd+Ctrl+K".into() },
            accounts: vec![
                AccountCfg { name: "claude".into(), harness: "claude".into(), home: "~/.claude".into(), command: Some("claude".into()), start: None, preferred: false, acp: None },
                AccountCfg { name: "codex".into(), harness: "codex".into(), home: "~/.codex".into(), command: None, start: None, preferred: false, acp: None },
            ],
            projects: vec![ProjectCfg {
                name: "Northwind".into(),
                repos: vec!["github.com/northwindai/*".into()],
                folders: vec!["~/projects/atlasai/osprey".into()],
            }],
            jev: JevCfg { model: "jev-1.13.0".into(), quick_panel: true, sessions: false, todos: true, pull_requests: false },
            router: None,
            search: SearchCfg { days: 7 },
            devices: DevicesCfg { enabled: true, name: "Studio".into(), tailscale: true, iroh: false, remote_control: true, port: 47000 },
            debug: DebugCfg::default(),
        };
        let parsed: Config = toml::from_str(&render(&cfg)).unwrap();
        assert_eq!(parsed, cfg);
    }

    #[test]
    fn saving_accounts_keeps_the_rest_of_the_file() {
        let text = render(&Config {
            accounts: vec![AccountCfg { name: "claude".into(), harness: "claude".into(), home: "~/.claude".into(), command: Some("claude".into()), start: None, preferred: false, acp: None }],
            projects: vec![ProjectCfg { name: "Northwind".into(), repos: vec!["github.com/northwindai/*".into()], folders: vec![] }],
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
        assert_eq!(cfg.apps.terminal, "ghostty");
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
            projects: vec![ProjectCfg { name: "Northwind".into(), repos: vec!["github.com/northwindai/*".into()], folders: vec![] }],
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
    fn a_new_file_has_the_theme() {
        let cfg = Config { appearance: AppearanceCfg { theme: Theme::Light }, ..Config::default() };
        let text = render(&cfg);
        assert!(text.contains("[appearance]\ntheme = \"light\"\n"));
        assert_eq!(toml::from_str::<Config>(&text).unwrap(), cfg);
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
            appearance: AppearanceCfg { theme: Theme::Dark },
            shortcuts: ShortcutsCfg::default(),
            accounts: vec![
                AccountCfg { name: "claude".into(), harness: "claude".into(), home: "~/.claude".into(), command: Some("claude".into()), start: None, preferred: false, acp: None },
                AccountCfg { name: "codex".into(), harness: "codex".into(), home: "~/.codex".into(), command: None, start: None, preferred: false, acp: None },
            ],
            projects: vec![
                ProjectCfg { name: "Northwind".into(), repos: vec!["github.com/northwindai/*".into()], folders: vec!["~/projects/atlasai/osprey".into()] },
                ProjectCfg { name: "Kestrel".into(), repos: vec!["github.com/kestrelhq/*".into()], folders: vec![] },
            ],
            jev: JevCfg::default(),
            router: None,
            search: SearchCfg::default(),
            devices: DevicesCfg::default(),
            debug: DebugCfg { perf_log: true },
        }
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
        next.projects.push(ProjectCfg { name: "Codealong".into(), repos: vec!["github.com/codealong/*".into()], folders: vec![] });
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
        bad.projects[1].name = "Northwind".into();
        assert!(check(&bad).unwrap_err().contains("Two projects"));
        let mut bad = full();
        bad.projects[0].repos.push(" ".into());
        assert!(check(&bad).unwrap_err().contains("empty"));
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
        owners.insert("kestrelhq".to_string(), 3);
        owners.insert("northwindai".to_string(), 9);
        owners.insert("ghempton".to_string(), 2);
        let names: BTreeMap<&str, &str> = [("northwindai", "Northwind"), ("kestrelhq", "Kestrel")].into();
        let p = suggest_projects(&owners, |o| names.get(o).map(|n| n.to_string()));
        assert_eq!(p[0].name, "ghempton");
        assert_eq!(p[1].name, "Northwind");
        assert_eq!(p[2].name, "Kestrel");
        assert_eq!(p[2].repos, vec!["github.com/kestrelhq/*".to_string()]);
    }
}
