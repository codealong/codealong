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
    pub accounts: Vec<AccountCfg>,
    #[serde(default)]
    pub projects: Vec<ProjectCfg>,
    #[serde(default)]
    pub debug: DebugCfg,
}

impl Default for Config {
    fn default() -> Self {
        Config { scan: default_scan(), apps: AppsCfg::default(), appearance: AppearanceCfg::default(), accounts: Vec::new(), projects: Vec::new(), debug: DebugCfg::default() }
    }
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
}

impl Default for AppsCfg {
    fn default() -> Self {
        AppsCfg { terminal: default_terminal(), editor: default_editor() }
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
    let path = config_path();
    if !path.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let cfg = toml::from_str::<Config>(&text).with_context(|| format!("{} has an error", crate::util::tilde(&path)))?;
    Ok(Some(cfg))
}

/// The theme config.toml asks for: `system` when there's no file yet or it has an error.
pub fn theme() -> Theme {
    load().ok().flatten().map(|c| c.appearance.theme).unwrap_or_default()
}

pub fn write(cfg: &Config) -> Result<()> {
    std::fs::create_dir_all(support_dir())?;
    write_atomic(&render(cfg))
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
            t
        })
        .collect();
    set_tables(doc, "accounts", rows);
}

const APPS_COMMENT: &str = "# Where Codealong opens things. Built-in profiles: terminal \"ghostty\", editor \"cursor\".\n";
const APPEARANCE_COMMENT: &str = "# Light or dark. \"system\" follows macOS. The buttons at the foot of the sidebar set this too.\n";
const DEBUG_COMMENT: &str = "# Timings for finding what makes the app slow, in ~/Library/Logs/Codealong/perf.log.\n";

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
    }
    set_value(&mut doc["appearance"]["theme"], Value::from(theme.as_str()));
}

/// A terminal or editor profile the `[apps]` table can name.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Profile {
    pub id: String,
    pub name: String,
}

/// The built-in app profiles (see launch.rs): what the Settings form offers for `[apps]`.
pub fn terminal_profiles() -> Vec<Profile> {
    vec![Profile { id: "ghostty".into(), name: "Ghostty".into() }]
}
pub fn editor_profiles() -> Vec<Profile> {
    vec![Profile { id: "cursor".into(), name: "Cursor".into() }]
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
    let (config, error) = match toml::from_str::<Config>(text) {
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
    if cfg.apps.terminal.trim().is_empty() || cfg.apps.editor.trim().is_empty() {
        return Err("The terminal and editor need a profile name.".into());
    }
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
    let cfg: Config = toml::from_str(text).map_err(|e| anyhow!("{}", e.to_string().trim()))?;
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

    set_theme(&mut doc, cfg.appearance.theme);

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

/// Finds Claude config folders (`~/.claude*` with a `projects` folder), Codex, OpenCode and
/// Antigravity, each by the files it keeps sessions in.
pub fn discover_accounts() -> Vec<AccountCfg> {
    let h = home();
    let mut claude: Vec<String> = std::fs::read_dir(&h)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let path = e.path();
            (name.starts_with(".claude") && path.is_dir() && path.join("projects").is_dir()).then_some(name)
        })
        .collect();
    claude.sort();
    let mut out: Vec<AccountCfg> = claude
        .into_iter()
        .map(|dir| {
            let name = dir.trim_start_matches('.').to_string();
            AccountCfg { name: name.clone(), harness: "claude".into(), home: format!("~/{dir}"), command: Some(name) }
        })
        .collect();
    if h.join(".codex/sessions").is_dir() {
        out.push(AccountCfg { name: "codex".into(), harness: "codex".into(), home: "~/.codex".into(), command: None });
    }
    if h.join(".local/share/opencode/opencode.db").is_file() {
        out.push(AccountCfg { name: "opencode".into(), harness: "opencode".into(), home: "~/.local/share/opencode".into(), command: Some("opencode".into()) });
    }
    if h.join(".gemini/antigravity/conversation_summaries.db").is_file() {
        out.push(AccountCfg { name: "antigravity".into(), harness: "antigravity".into(), home: "~/.gemini/antigravity".into(), command: None });
    }
    out
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
    s.push('\n');
    s.push_str(APPEARANCE_COMMENT);
    s.push_str("[appearance]\n");
    s.push_str(&format!("theme = {}\n", q(cfg.appearance.theme.as_str())));
    s.push_str("\n# Harness accounts. `name` is what you type and what the board shows.\n");
    for a in &cfg.accounts {
        s.push_str("\n[[accounts]]\n");
        s.push_str(&format!("name = {}\n", q(&a.name)));
        s.push_str(&format!("harness = {}\n", q(&a.harness)));
        s.push_str(&format!("home = {}\n", q(&a.home)));
        if let Some(c) = &a.command {
            s.push_str(&format!("command = {}\n", q(c)));
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
            accounts: vec![
                AccountCfg { name: "claude".into(), harness: "claude".into(), home: "~/.claude".into(), command: Some("claude".into()) },
                AccountCfg { name: "codex".into(), harness: "codex".into(), home: "~/.codex".into(), command: None },
            ],
            projects: vec![ProjectCfg {
                name: "Northwind".into(),
                repos: vec!["github.com/northwindai/*".into()],
                folders: vec!["~/projects/atlasai/osprey".into()],
            }],
            debug: DebugCfg::default(),
        };
        let parsed: Config = toml::from_str(&render(&cfg)).unwrap();
        assert_eq!(parsed, cfg);
    }

    #[test]
    fn saving_accounts_keeps_the_rest_of_the_file() {
        let text = render(&Config {
            accounts: vec![AccountCfg { name: "claude".into(), harness: "claude".into(), home: "~/.claude".into(), command: Some("claude".into()) }],
            projects: vec![ProjectCfg { name: "Northwind".into(), repos: vec!["github.com/northwindai/*".into()], folders: vec![] }],
            ..Config::default()
        });
        let text = text.replace("[apps]\n", "[apps]\n# my editor of choice\n");
        let accounts = vec![
            AccountCfg { name: "claude".into(), harness: "claude".into(), home: "~/.claude".into(), command: Some("claude".into()) },
            AccountCfg { name: "claude3".into(), harness: "claude".into(), home: "~/.claude3".into(), command: Some("CLAUDE_CONFIG_DIR=~/.claude3 claude".into()) },
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
        let a = |name: &str, home: &str| AccountCfg { name: name.into(), harness: "claude".into(), home: home.into(), command: None };
        assert!(check_accounts(&[a("claude", "~/.claude"), a("claude", "~/.claude2")]).is_err());
        assert!(check_accounts(&[a("claude", "~/.claude"), a("claude2", "~/.claude")]).is_err());
        assert!(check_accounts(&[a("my claude", "~/.claude")]).is_err());
        assert!(check_accounts(&[a("claude", "~/.claude"), a("claude2", "~/.claude2")]).is_ok());
    }

    #[test]
    fn fills_defaults() {
        let cfg: Config = toml::from_str("").unwrap();
        assert_eq!(cfg.apps.terminal, "ghostty");
        assert_eq!(cfg.apps.editor, "cursor");
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
            accounts: vec![AccountCfg { name: "claude".into(), harness: "claude".into(), home: "~/.claude".into(), command: Some("claude".into()) }],
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
    fn a_new_file_has_the_theme() {
        let cfg = Config { appearance: AppearanceCfg { theme: Theme::Light }, ..Config::default() };
        let text = render(&cfg);
        assert!(text.contains("[appearance]\ntheme = \"light\"\n"));
        assert_eq!(toml::from_str::<Config>(&text).unwrap(), cfg);
    }

    fn full() -> Config {
        Config {
            scan: vec!["~/projects".into(), "~/work".into()],
            apps: AppsCfg::default(),
            appearance: AppearanceCfg { theme: Theme::Dark },
            accounts: vec![
                AccountCfg { name: "claude".into(), harness: "claude".into(), home: "~/.claude".into(), command: Some("claude".into()) },
                AccountCfg { name: "codex".into(), harness: "codex".into(), home: "~/.codex".into(), command: None },
            ],
            projects: vec![
                ProjectCfg { name: "Northwind".into(), repos: vec!["github.com/northwindai/*".into()], folders: vec!["~/projects/atlasai/osprey".into()] },
                ProjectCfg { name: "Kestrel".into(), repos: vec!["github.com/kestrelhq/*".into()], folders: vec![] },
            ],
            debug: DebugCfg { perf_log: true },
        }
    }

    #[test]
    fn saving_everything_sets_each_value_in_place() {
        let text = render(&full()).replace("[apps]\n", "[apps]\n# my editor of choice\n").replace("scan = ", "# roots\nscan = ");
        let mut next = full();
        next.scan = vec!["~/code".into()];
        next.apps.editor = "zed".into();
        next.appearance.theme = Theme::Light;
        next.debug.perf_log = false;
        next.accounts.pop();
        next.projects[1].folders.push("~/extra".into());
        next.projects.push(ProjectCfg { name: "Codealong".into(), repos: vec!["github.com/codealong/*".into()], folders: vec![] });
        let out = with_config(&text, &next).unwrap();
        assert_eq!(toml::from_str::<Config>(&out).unwrap(), next);
        for kept in ["# my editor of choice", "# roots", "# Harness accounts.", "# Projects group repos", APPS_COMMENT, DEBUG_COMMENT] {
            assert!(out.contains(kept), "lost {kept:?} in:\n{out}");
        }
        // The file keeps its order: scan, apps, appearance, accounts, projects, debug.
        let at = |s: &str| out.find(s).unwrap_or_else(|| panic!("no {s:?} in:\n{out}"));
        assert!(at("scan =") < at("[apps]") && at("[apps]") < at("[appearance]") && at("[appearance]") < at("[[accounts]]"));
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
