//! `config.toml`: apps, theme, accounts, projects and scan roots. Codealong writes it on first
//! run from what it finds. After that you edit it, or the Accounts dialog rewrites the
//! `[[accounts]]` tables and the theme buttons the `theme` line, in place, leaving the rest of
//! the file, comments included, as you wrote it.

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{Context, Result};
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
}

impl Default for Config {
    fn default() -> Self {
        Config { scan: default_scan(), apps: AppsCfg::default(), appearance: AppearanceCfg::default(), accounts: Vec::new(), projects: Vec::new() }
    }
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
    /// What you type, and what chips show: claude, claude2, codex.
    pub name: String,
    /// `claude` or `codex`.
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

pub const HARNESSES: [&str; 2] = ["claude", "codex"];

/// Why a list of accounts can't be saved, in words.
pub fn check_accounts(accounts: &[AccountCfg]) -> std::result::Result<(), String> {
    let mut names = std::collections::HashSet::new();
    let mut homes = std::collections::HashSet::new();
    for a in accounts {
        if a.name.is_empty() || !a.name.chars().all(|c| c.is_ascii_alphanumeric() || "._-".contains(c)) {
            return Err(format!("\"{}\" can't be an account name. Use letters, numbers, dots, dashes or underscores.", a.name));
        }
        if !HARNESSES.contains(&a.harness.as_str()) {
            return Err(format!("{} needs a harness: claude or codex.", a.name));
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
    use toml_edit::{value, ArrayOfTables, DocumentMut, Item, Table};
    let mut doc: DocumentMut = text.parse().context("config.toml has an error; fix it before saving accounts")?;
    let lead = doc
        .get("accounts")
        .and_then(Item::as_array_of_tables)
        .and_then(|a| a.get(0))
        .and_then(|t| t.decor().prefix().and_then(|p| p.as_str().map(str::to_string)));
    let mut tables = ArrayOfTables::new();
    for (i, a) in accounts.iter().enumerate() {
        let mut t = Table::new();
        t["name"] = value(a.name.trim());
        t["harness"] = value(a.harness.trim());
        t["home"] = value(a.home.trim());
        if let Some(c) = a.command.as_deref().map(str::trim).filter(|c| !c.is_empty()) {
            t["command"] = value(c);
        }
        let prefix = if i == 0 { lead.clone().unwrap_or_else(|| "\n".to_string()) } else { "\n".to_string() };
        t.decor_mut().set_prefix(prefix);
        tables.push(t);
    }
    doc["accounts"] = Item::ArrayOfTables(tables);
    Ok(doc.to_string())
}

const APPEARANCE_COMMENT: &str = "# Light or dark. \"system\" follows macOS. The buttons at the foot of the sidebar set this too.\n";

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
    use toml_edit::{value, DocumentMut, Item, Table, Value};
    let mut doc: DocumentMut = text.parse().context("config.toml has an error; fix it before changing the theme")?;
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
    let slot = &mut doc["appearance"]["theme"];
    match slot.as_value_mut() {
        Some(v) => {
            let decor = v.decor().clone();
            *v = Value::from(theme.as_str());
            *v.decor_mut() = decor;
        }
        None => *slot = value(theme.as_str()),
    }
    Ok(doc.to_string())
}

/// Finds Claude config folders (`~/.claude*` with a `projects` folder) and Codex.
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
    s.push_str("# Where Codealong opens things. Built-in profiles: terminal \"ghostty\", editor \"cursor\".\n");
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
