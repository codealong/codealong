//! Starting a session from Codealong. Codealong only launches: the harness runs the session,
//! makes a worktree when you ask it to, and the session reaches the board through its adapter
//! like any other. See *Surfaces*, New session.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Result};
use serde::{Deserialize, Serialize};

use crate::codex;
use crate::config::{AccountCfg, Config};
use crate::launch;
use crate::model::{Board, Harness};
use crate::util::{home, tilde};

/// The environment variable a terminal start hands the prompt over in. The command Codealong
/// types stays short and needs no quoting, whatever the prompt holds.
pub const PROMPT_VAR: &str = "CODEALONG_PROMPT";

/// Where a new session runs.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "kebab-case")]
pub enum Place {
    /// A new terminal window running the account's command.
    Terminal,
    /// The Claude app's new-session page, with the folder and prompt filled in.
    ClaudeApp,
    /// A new thread in the Codex app, with the folder and prompt filled in.
    CodexApp,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PlaceOption {
    pub place: Place,
    /// The app's name, the way buttons say it: Ghostty, Claude app, Codex app.
    pub label: String,
}

/// An account as the new-session sheet offers it.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StartAccount {
    pub name: String,
    pub harness: Harness,
    /// Where this account can start a session, the usual place first.
    pub places: Vec<PlaceOption>,
    /// Why there's nowhere, when `places` is empty.
    pub unavailable: Option<String>,
}

#[derive(Serialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StartOptions {
    /// In config order.
    pub accounts: Vec<StartAccount>,
    /// Folders the Codex app has as projects. A thread started anywhere else adds one.
    pub codex_projects: Vec<String>,
}

/// What the sheet asks for.
#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct StartSpec {
    pub lane_id: String,
    pub account: String,
    pub place: Place,
    #[serde(default)]
    pub prompt: String,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Started {
    /// The new session's key when Codealong chose its ID, so the board can select it as soon
    /// as it shows.
    pub key: Option<String>,
    /// What to do next, when the app is waiting on you.
    pub message: Option<String>,
}

/// What a start does. `--snapshot --start-plan` prints it.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Plan {
    /// A new terminal window at `cwd`, with `env` set in its shell, typing `input`.
    Terminal { cwd: PathBuf, input: String, env: Vec<(String, String)> },
    /// A link the Claude app opens.
    ClaudeApp { url: String },
    /// A link the Codex app opens.
    CodexApp { url: String },
}

fn harness_of(a: &AccountCfg) -> Harness {
    if a.harness == "codex" {
        Harness::Codex
    } else {
        Harness::Claude
    }
}

/// Where each account can start a session. The Claude app and the Codex app each run one
/// account, the one in the default folder (`~/.claude`, `~/.codex`); the terminal runs any
/// Claude account through its command.
pub fn options(accounts: &[AccountCfg], terminal: &str, codex_projects: Vec<String>, home: &Path) -> StartOptions {
    let accounts = accounts
        .iter()
        .map(|a| {
            let harness = harness_of(a);
            let mut places = Vec::new();
            let mut unavailable = None;
            match harness {
                Harness::Claude => {
                    places.push(PlaceOption { place: Place::Terminal, label: terminal.to_string() });
                    if a.home_path() == home.join(".claude") {
                        places.push(PlaceOption { place: Place::ClaudeApp, label: "Claude app".into() });
                    }
                }
                Harness::Codex => {
                    if a.home_path() == home.join(".codex") {
                        places.push(PlaceOption { place: Place::CodexApp, label: "Codex app".into() });
                    } else {
                        unavailable = Some(format!("The Codex app runs the account in ~/.codex, so {} can't start threads from here.", a.name));
                    }
                }
            }
            StartAccount { name: a.name.clone(), harness, places, unavailable }
        })
        .collect();
    StartOptions { accounts, codex_projects }
}

/// The sheet's options for the accounts in `cfg`.
pub fn options_for(cfg: &Config) -> StartOptions {
    let codex_projects = cfg.accounts.iter().filter(|a| a.harness == "codex").flat_map(|a| codex::app_project_roots(&a.home_path())).collect();
    options(&cfg.accounts, &launch::terminal_name(&cfg.apps.terminal), codex_projects, &home())
}

/// The launch for one start. In the terminal Claude Code takes `session_id` as its own
/// (`--session-id`), so the board knows the session the moment it appears. The apps get a link
/// that opens their new-session page with the folder and prompt filled in; you send it there.
pub fn plan(account: &AccountCfg, place: Place, folder: &Path, prompt: &str, session_id: &str) -> Plan {
    let prompt = prompt.trim();
    let dir = folder.to_string_lossy();
    match place {
        Place::Terminal => {
            let mut input = format!("{} --session-id {session_id}", account.command());
            let mut env = Vec::new();
            if !prompt.is_empty() {
                input.push_str(&format!(" \"${PROMPT_VAR}\""));
                env.push((PROMPT_VAR.to_string(), as_argument(prompt)));
            }
            input.push('\n');
            Plan::Terminal { cwd: folder.to_path_buf(), input, env }
        }
        Place::ClaudeApp => Plan::ClaudeApp { url: with_param(format!("claude://code/new?folder={}", urlencoding::encode(&dir)), "q", prompt) },
        Place::CodexApp => Plan::CodexApp { url: with_param(format!("codex://threads/new?path={}", urlencoding::encode(&dir)), "prompt", prompt) },
    }
}

fn with_param(url: String, key: &str, value: &str) -> String {
    if value.is_empty() {
        url
    } else {
        format!("{url}&{key}={}", urlencoding::encode(value))
    }
}

/// Claude Code reads an argument that starts with `-` as an option, and one that is only a
/// command's name (`update`, `doctor`) as that command. A space in front of the one and after
/// the other keeps each a prompt.
fn as_argument(prompt: &str) -> String {
    let mut p = prompt.to_string();
    if p.starts_with('-') {
        p.insert(0, ' ');
    }
    if !p.trim().contains(char::is_whitespace) {
        p.push(' ');
    }
    p
}

/// A random version 4 UUID, the form `--session-id` takes.
pub fn new_session_id() -> String {
    let mut b = [0u8; 16];
    // SAFETY: arc4random_buf writes exactly the length it's given into the buffer.
    unsafe { libc::arc4random_buf(b.as_mut_ptr().cast(), b.len()) };
    uuid_v4(b)
}

fn uuid_v4(mut b: [u8; 16]) -> String {
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let hex: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!("{}-{}-{}-{}-{}", &hex[..8], &hex[8..12], &hex[12..16], &hex[16..20], &hex[20..])
}

/// Starts a session: checks the choice against the board and the config, then hands it to the
/// terminal or the app.
pub fn start(board: &Board, cfg: &Config, spec: &StartSpec) -> Result<Started> {
    let lane = board.lanes.iter().find(|l| l.id == spec.lane_id).ok_or_else(|| anyhow!("That worktree just changed. Try again."))?;
    let account = cfg.accounts.iter().find(|a| a.name == spec.account).ok_or_else(|| anyhow!("There's no account named {}.", spec.account))?;
    let folder = Path::new(&lane.path);
    if !folder.is_dir() {
        bail!("{} isn't there any more.", tilde(folder));
    }
    let mut offered = options(std::slice::from_ref(account), &launch::terminal_name(&cfg.apps.terminal), Vec::new(), &home());
    let offered = offered.accounts.remove(0);
    if !offered.places.iter().any(|p| p.place == spec.place) {
        bail!(offered.unavailable.unwrap_or_else(|| format!("{} can't start a session there.", account.name)));
    }
    let id = new_session_id();
    let prompted = !spec.prompt.trim().is_empty();
    match plan(account, spec.place, folder, &spec.prompt, &id) {
        Plan::Terminal { cwd, input, env } => {
            launch::terminal_run(&cfg.apps.terminal, &cwd, &input, &env)?;
            Ok(Started { key: Some(format!("claude:{}:{id}", account.name)), message: None })
        }
        Plan::ClaudeApp { url } => {
            launch::open_url(&url)?;
            let message = if prompted {
                "Your prompt is waiting in a new Claude app session. Send it from there.".to_string()
            } else {
                format!("A new Claude app session is open in {}.", lane.callsign)
            };
            Ok(Started { key: None, message: Some(message) })
        }
        Plan::CodexApp { url } => {
            launch::open_codex(&url)?;
            let message = if prompted {
                "Your prompt is waiting in a new Codex thread. Send it from there.".to_string()
            } else {
                format!("A new Codex thread is open in {}.", lane.callsign)
            };
            Ok(Started { key: None, message: Some(message) })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account(name: &str, harness: &str, home: &str, command: Option<&str>) -> AccountCfg {
        AccountCfg { name: name.into(), harness: harness.into(), home: home.into(), command: command.map(String::from) }
    }

    fn places(o: &StartOptions, name: &str) -> Vec<Place> {
        o.accounts.iter().find(|a| a.name == name).unwrap().places.iter().map(|p| p.place).collect()
    }

    #[test]
    fn offers_each_app_only_its_own_account() {
        let accounts = vec![
            account("claude", "claude", "/Users/me/.claude", Some("claude")),
            account("claude2", "claude", "/Users/me/.claude2", Some("claude2")),
            account("codex", "codex", "/Users/me/.codex", None),
            account("codex2", "codex", "/Users/me/.codex2", None),
        ];
        let o = options(&accounts, "Ghostty", vec!["/r/kestrel".into()], Path::new("/Users/me"));
        assert_eq!(places(&o, "claude"), vec![Place::Terminal, Place::ClaudeApp]);
        assert_eq!(places(&o, "claude2"), vec![Place::Terminal]);
        assert_eq!(places(&o, "codex"), vec![Place::CodexApp]);
        assert!(places(&o, "codex2").is_empty());
        assert!(o.accounts[3].unavailable.as_deref().unwrap_or("").contains("~/.codex"));
        assert_eq!(o.accounts[0].places[0].label, "Ghostty");
        assert_eq!(o.codex_projects, vec!["/r/kestrel".to_string()]);
    }

    #[test]
    fn hands_the_terminal_the_prompt_in_the_environment() {
        let a = account("claude2", "claude", "~/.claude2", Some("claude2"));
        let id = "0b7c2a4e-1d2f-4e3a-9b8c-7d6e5f4a3b2c";
        let prompt = "Fix the \"flaky\" test\nthen run `npm test` and $(say done)";
        let Plan::Terminal { cwd, input, env } = plan(&a, Place::Terminal, Path::new("/r/atlas-app4"), &format!("  {prompt}\n"), id) else { panic!() };
        assert_eq!(cwd, PathBuf::from("/r/atlas-app4"));
        assert_eq!(input, format!("claude2 --session-id {id} \"$CODEALONG_PROMPT\"\n"));
        assert_eq!(env, vec![(PROMPT_VAR.to_string(), prompt.to_string())]);
        let Plan::Terminal { input, env, .. } = plan(&a, Place::Terminal, Path::new("/r/x"), "  ", id) else { panic!() };
        assert_eq!(input, format!("claude2 --session-id {id}\n"));
        assert!(env.is_empty());
        let custom = account("claude3", "claude", "~/.claude3", Some("CLAUDE_CONFIG_DIR=~/.claude3 claude"));
        let Plan::Terminal { input, .. } = plan(&custom, Place::Terminal, Path::new("/r/x"), "", id) else { panic!() };
        assert_eq!(input, format!("CLAUDE_CONFIG_DIR=~/.claude3 claude --session-id {id}\n"));
    }

    #[test]
    fn keeps_a_prompt_from_reading_as_an_option_or_a_command() {
        assert_eq!(as_argument("Fix the build"), "Fix the build");
        assert_eq!(as_argument("update"), "update ");
        assert_eq!(as_argument("-v"), " -v ");
        assert_eq!(as_argument("--help with the README"), " --help with the README");
    }

    #[test]
    fn builds_the_app_links() {
        let claude = account("claude", "claude", "~/.claude", Some("claude"));
        let codex = account("codex", "codex", "~/.codex", None);
        let folder = Path::new("/Users/me/My Repo");
        assert_eq!(
            plan(&claude, Place::ClaudeApp, folder, "Fix it & ship\nnow", "id"),
            Plan::ClaudeApp { url: "claude://code/new?folder=%2FUsers%2Fme%2FMy%20Repo&q=Fix%20it%20%26%20ship%0Anow".into() }
        );
        assert_eq!(plan(&claude, Place::ClaudeApp, folder, "", "id"), Plan::ClaudeApp { url: "claude://code/new?folder=%2FUsers%2Fme%2FMy%20Repo".into() });
        assert_eq!(
            plan(&codex, Place::CodexApp, folder, "Review #12", "id"),
            Plan::CodexApp { url: "codex://threads/new?path=%2FUsers%2Fme%2FMy%20Repo&prompt=Review%20%2312".into() }
        );
    }

    #[test]
    fn makes_version_4_session_ids() {
        assert_eq!(uuid_v4([0xff; 16]), "ffffffff-ffff-4fff-bfff-ffffffffffff");
        assert_eq!(uuid_v4([0; 16]), "00000000-0000-4000-8000-000000000000");
        let a = new_session_id();
        assert_eq!(a.len(), 36);
        assert_eq!(&a[14..15], "4");
        assert_ne!(a, new_session_id());
    }
}
