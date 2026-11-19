//! Codealong itself: `status` (whether its server runs, where its data is, what's installed in
//! each account), `server start|stop`, `config` (config.toml as the app reads it, and a file
//! checked) and `install` (the command on PATH, the plugin and the skill in each account).
//!
//! No server is started for these as it is for other commands (`starts_server` in cli.rs):
//! `server start` is what starts one. `status` and `config` still answer when config.toml
//! doesn't parse, and say so. Settings are changed by editing the file, never through a command.

use std::path::Path;

use anyhow::{anyhow, bail, Context, Result};
use serde::Serialize;

use crate::config::{self, Config};
use crate::server;
use crate::todos;
use crate::util::{expand, now_ms, tilde};

use super::*;

pub(super) fn server_cmd(cmd: ServerCmd, json: bool) -> Result<()> {
    match cmd {
        ServerCmd::Start => {
            let s = crate::serve::ensure(false, None).map_err(|e| anyhow!(e))?;
            if json {
                return print_json(&s);
            }
            println!("{}", running_words(&s));
        }
        ServerCmd::Stop => {
            let stopped = crate::serve::stop();
            if json {
                return print_json(&serde_json::json!({ "stopped": stopped }));
            }
            println!("{}", if stopped { "Stopped Codealong's server, and the app with it." } else { "Codealong's server wasn't running." });
        }
    }
    Ok(())
}

/// How the server that answered stands, in a sentence: its version and pid, whether its first
/// scan is done, and whether the app is open on it.
fn running_words(s: &server::Status) -> String {
    let scanning = if s.loading { ", still scanning" } else { "" };
    if !crate::serve::is_server(s) {
        return format!("Codealong {} is running (pid {}){scanning}.", s.version, s.pid);
    }
    let app = match s.views.last() {
        Some(v) => format!("the app is open (pid {})", v.pid),
        None => "the app isn't open".into(),
    };
    format!("Codealong's server {} is running (pid {}){scanning}; {app}.", s.version, s.pid)
}

// ---- status and accounts ----

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StatusOut {
    app_running: bool,
    app: Option<server::Status>,
    socket: String,
    version: String,
    data_dir: String,
    config: String,
    todos: String,
    accounts: usize,
    /// Why config.toml's settings can't be read, when they can't.
    #[serde(skip_serializing_if = "Option::is_none")]
    config_error: Option<String>,
    /// The symlink `install` made, when it's in place.
    command_link: Option<String>,
    /// Where the skill is installed, per account.
    skills: Vec<SkillStatus>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SkillStatus {
    account: String,
    path: Option<String>,
    installed: bool,
    current: bool,
    edited: bool,
    /// A Claude Code account's plugin, which is what Codealong installs there in place of the skill.
    #[serde(skip_serializing_if = "Option::is_none")]
    plugin: Option<crate::install::PluginState>,
}

pub(super) fn status(c: &mut Client, json: bool) -> Result<()> {
    // Status still answers with settings that don't parse, and says that first.
    let (cfg, config_error) = match c.cfg() {
        Ok(cfg) => (cfg.clone(), None),
        Err(e) => (Config::default(), Some(format!("{e:#}"))),
    };
    let out = StatusOut {
        app_running: c.app.is_some(),
        app: c.app.clone(),
        socket: server::path().to_string_lossy().into(),
        version: env!("CARGO_PKG_VERSION").into(),
        data_dir: config::support_dir().to_string_lossy().into(),
        config: config::config_path().to_string_lossy().into(),
        todos: todos::path().to_string_lossy().into(),
        accounts: cfg.accounts.len(),
        config_error,
        command_link: crate::install::command_link().map(|p| p.to_string_lossy().into_owned()),
        skills: {
            let source = crate::install::skill_source();
            cfg.accounts
                .iter()
                .map(|a| crate::install::account_skill(source.as_deref(), a))
                .map(|s| SkillStatus { account: s.name, path: s.path, installed: s.installed, current: s.current, edited: s.edited, plugin: s.plugin })
                .collect()
        },
    };
    if json {
        return print_json(&out);
    }
    match &out.app {
        Some(a) => println!("{}", running_words(a)),
        None => println!("Codealong's server isn't running; the files are read directly, and a command that needs it starts it."),
    }
    println!("Data: {}", tilde(Path::new(&out.data_dir)));
    if let Some(e) = &out.config_error {
        println!("Config: {e}");
    }
    println!("Accounts: {}", out.accounts);
    match &out.command_link {
        Some(l) => println!("Command: {}", tilde(Path::new(l))),
        None => println!("Command: not on PATH (codealong install --command)"),
    }
    let shipped = crate::install::shipped_version();
    for s in &out.skills {
        match &s.plugin {
            Some(p) => println!("Plugin for {}: {}", s.account, plugin_words(p, shipped.as_deref(), s.installed.then_some(s.path.as_deref()).flatten())),
            None => println!("Skill for {}: {}", s.account, match (s.installed, s.current, s.edited, &s.path) { (true, _, true, Some(p)) => format!("edited at {}, so updates leave it alone", tilde(Path::new(p))), (true, true, _, Some(p)) => format!("installed at {}", tilde(Path::new(p))), (true, false, _, Some(p)) => format!("out of date at {}", tilde(Path::new(p))), (_, _, _, Some(_)) => "not installed".into(), _ => "no skills folder known".into() }),
        }
    }
    Ok(())
}

/// A Claude Code account's plugin in a few words, as Settings › Agents says it (`pluginState`
/// in AgentsSection.tsx): against the version this build carries, and with the skill copy
/// Codealong made there before the plugin, when it's still in the skills folder.
fn plugin_words(p: &crate::install::PluginState, shipped: Option<&str>, old_copy: Option<&str>) -> String {
    let state = match (&p.version, &p.from) {
        (None, _) => "not installed".to_string(),
        (Some(_), _) if p.disabled => "turned off".to_string(),
        (Some(v), Some(from)) => format!("{v}, installed from {from}"),
        (Some(v), None) if shipped.is_some_and(|s| s != v) => format!("{v}, out of date"),
        (Some(v), None) => format!("installed, {v}"),
    };
    match old_copy {
        Some(path) => format!("{state}; an older copy of the skill is still at {}", tilde(Path::new(path))),
        None => state,
    }
}

// ---- config ----

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ConfigOut {
    #[serde(flatten)]
    view: config::ConfigView,
    /// Accounts found on this Mac that config.toml doesn't have yet.
    discovered: Vec<crate::detect::Found>,
}

fn config_view() -> ConfigOut {
    // The view says what's wrong with a config.toml that doesn't parse; nothing is offered
    // against it meanwhile.
    ConfigOut { view: config::view(), discovered: crate::detect::unlisted().unwrap_or_default() }
}

pub(super) fn config_cmd(cmd: ConfigCmd, json: bool) -> Result<()> {
    match cmd {
        ConfigCmd::Show => {
            let out = config_view();
            if json {
                return print_json(&out);
            }
            println!("# {}", tilde(Path::new(&out.view.path)));
            if let Some(e) = &out.view.error {
                println!("# Doesn't parse: {e}");
            }
            print!("{}", out.view.text);
            if !out.view.text.ends_with('\n') {
                println!();
            }
            for d in &out.discovered {
                let started = d.command.as_deref().map(|c| format!(", started with `{c}`")).unwrap_or_default();
                println!("# Found but not configured: {} ({} at {}{started})", d.name, d.harness, d.home);
            }
            Ok(())
        }
        ConfigCmd::Check { repo: Some(folder), .. } => {
            let path = expand(&folder).join(crate::teamfile::FILE);
            let Some(parsed) = crate::teamfile::read_folder(&expand(&folder)) else { bail!("{} has no {}.", tilde(&expand(&folder)), crate::teamfile::FILE) };
            if json {
                let (settings, skipped, error) = match &parsed {
                    Ok((cfg, skipped)) => (Some(cfg), skipped.clone(), None),
                    Err(e) => (None, Vec::new(), Some(e.clone())),
                };
                return print_json(&serde_json::json!({ "path": path, "ok": error.is_none(), "error": error, "settings": settings, "skipped": skipped })).and_then(|_| parsed.map(|_| ()).map_err(|e| anyhow!("{e}")));
            }
            match parsed {
                Ok((_, skipped)) => {
                    println!("{} reads as team settings.", tilde(&path));
                    if !skipped.is_empty() {
                        println!("Passed over, as this Codealong doesn't know them: {}.", skipped.join(", "));
                    }
                }
                Err(e) => bail!("{}: {e}", tilde(&path)),
            }
            Ok(())
        }
        ConfigCmd::Check { file, .. } => {
            let (path, text) = match file {
                Some(f) => (expand(&f), std::fs::read_to_string(expand(&f)).with_context(|| format!("reading {f}"))?),
                None => (config::config_path(), std::fs::read_to_string(config::config_path()).unwrap_or_default()),
            };
            let parsed = config::parse(&text).map_err(|e| format!("{e:#}"));
            // Boundary members' sign-ins, asked of their providers: one that doesn't qualify is
            // outside its group until it does (boundary.rs).
            let warnings = parsed.as_ref().map(|cfg| crate::boundary::warnings(cfg, |a| crate::signin::current(a, now_ms(), crate::signin::Ask::IfStale))).unwrap_or_default();
            let result = parsed.map(|_| ());
            if json {
                return print_json(&serde_json::json!({ "path": path, "ok": result.is_ok(), "error": result.clone().err(), "warnings": warnings })).and_then(|_| result.map_err(|e| anyhow!("{e}")));
            }
            match result {
                Ok(()) => println!("{} reads as settings.", tilde(&path)),
                Err(e) => bail!("{}: {e}", tilde(&path)),
            }
            for w in &warnings {
                println!("warning: {w}");
            }
            Ok(())
        }
    }
}

// ---- install ----

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Installed {
    command: Option<String>,
    plugins: Vec<PluginInstall>,
    skills: Vec<SkillInstall>,
    acp: Option<String>,
    warnings: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PluginInstall {
    account: String,
    version: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SkillInstall {
    account: String,
    path: String,
}

pub(super) fn install(c: &mut Client, a: &InstallArgs, json: bool) -> Result<()> {
    let both = !a.command && !a.skill && !a.acp;
    let mut out = Installed { command: None, plugins: Vec::new(), skills: Vec::new(), acp: None, warnings: Vec::new() };
    if a.acp || both {
        match crate::acp::install_adapters() {
            Ok(done) => out.acp = Some(done),
            Err(e) => out.warnings.push(format!("{e:#}")),
        }
    }
    let accounts: Vec<String> = match a.skill || both {
        true => {
            let names: Vec<String> = c.cfg()?.accounts.iter().filter(|x| a.account.is_empty() || a.account.contains(&x.name)).map(|x| x.name.clone()).collect();
            if names.is_empty() {
                bail!("No account matches. `codealong accounts` lists them.");
            }
            if crate::install::skill_source().is_none() {
                bail!("The skill folder isn't next to this binary. Run from the app bundle, or from the repo.");
            }
            names
        }
        false => Vec::new(),
    };
    // Installing for every account leaves an edited skill copy alone; naming the account replaces it.
    let ask = crate::install::Ask { command: a.command || both, bin: a.bin.as_deref().map(expand), accounts, keep_edited: a.account.is_empty() };
    let done = crate::install::run(&ask);
    out.command = done.command.map(|p| p.to_string_lossy().into_owned());
    out.warnings.extend(done.errors);
    let status = crate::install::status();
    for name in &done.accounts {
        let Some(acct) = status.accounts.iter().find(|s| &s.name == name) else { continue };
        match &acct.plugin {
            Some(plugin) => out.plugins.push(PluginInstall { account: name.clone(), version: plugin.version.clone() }),
            None => out.skills.push(SkillInstall { account: name.clone(), path: acct.path.clone().unwrap_or_default() }),
        }
    }
    if json {
        return print_json(&out);
    }
    if let Some(p) = &out.command {
        println!("Command: {}", tilde(Path::new(p)));
    }
    for p in &out.plugins {
        println!("Plugin for {}: codealong {}", p.account, p.version.as_deref().unwrap_or("(version unknown)"));
    }
    for s in &out.skills {
        println!("Skill for {}: {}", s.account, tilde(Path::new(&s.path)));
    }
    if let Some(done) = &out.acp {
        println!("{done}");
    }
    for w in &out.warnings {
        println!("! {w}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_says_a_claude_accounts_plugin() {
        use crate::install::PluginState;
        let p = |version: Option<&str>, disabled: bool, from: Option<&str>| PluginState { version: version.map(String::from), disabled, from: from.map(String::from) };
        assert_eq!(plugin_words(&p(None, false, None), Some("0.2.1+aaaa"), None), "not installed");
        assert_eq!(plugin_words(&p(Some("0.2.1+aaaa"), false, None), Some("0.2.1+aaaa"), None), "installed, 0.2.1+aaaa");
        assert_eq!(plugin_words(&p(Some("0.2.0+bbbb"), false, None), Some("0.2.1+aaaa"), None), "0.2.0+bbbb, out of date");
        assert_eq!(plugin_words(&p(Some("0.2.1+aaaa"), true, None), Some("0.2.1+aaaa"), None), "turned off");
        assert_eq!(plugin_words(&p(Some("0.2.1"), false, Some("someone/plugins")), Some("0.2.1+aaaa"), None), "0.2.1, installed from someone/plugins");
        // A plugin installed with no build to compare against isn't called out of date.
        assert_eq!(plugin_words(&p(Some("0.2.1+aaaa"), false, None), None, None), "installed, 0.2.1+aaaa");
        assert!(plugin_words(&p(Some("0.2.1+aaaa"), false, None), None, Some("/x/skills/codealong")).ends_with("; an older copy of the skill is still at /x/skills/codealong"));
    }
}
