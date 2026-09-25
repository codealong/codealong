//! Starting a session from Codealong. Codealong only launches: the harness runs the session,
//! makes a worktree when you ask it to, and the session reaches the board through its adapter
//! like any other. See *Surfaces*, New session.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Result};
use serde::{Deserialize, Serialize};

use crate::codex;
use crate::config::{AccountCfg, Config};
use crate::attachments::{self, Style};
use crate::launch;
use crate::model::{Board, Harness};
use crate::util::{home, tilde};

/// The environment variable a terminal start hands the prompt over in. The command Codealong
/// types stays short and needs no quoting, whatever the prompt holds.
pub const PROMPT_VAR: &str = "CODEALONG_PROMPT";
/// The session's key (`claude:<account>:<id>`), set in a terminal start's environment so the
/// `codealong` command can tell which session runs it (`whoami`).
pub const SESSION_VAR: &str = "CODEALONG_SESSION";
/// The images folder, for `--add-dir`, when the prompt has images. Its path has a space in it.
pub const IMAGES_VAR: &str = "CODEALONG_IMAGES";
/// The attached files' folder, for `--add-dir`, when the prompt has other files.
pub const FILES_VAR: &str = "CODEALONG_FILES";

/// The models a Claude Code start can ask for, as `--model` takes them: aliases for the newest
/// model of each family, the way Claude Code's own `/model` picker offers them (2.1.282 knows
/// sonnet, opus, haiku, fable, their `[1m]` variants, best and opusplan). The apps' new-session
/// links take no model; you pick it in the app.
const CLAUDE_MODELS: [(&str, &str); 8] = [
    ("fable", "Fable"),
    ("fable[1m]", "Fable (1M context)"),
    ("opus", "Opus"),
    ("opus[1m]", "Opus (1M context)"),
    ("sonnet", "Sonnet"),
    ("sonnet[1m]", "Sonnet (1M context)"),
    ("haiku", "Haiku"),
    ("opusplan", "Opus Plan Mode: Opus to plan, Sonnet to build"),
];

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
pub struct ModelOption {
    /// What `--model` gets.
    pub id: String,
    pub label: String,
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
    /// The model a session starts on without `--model`: the account's settings.json `model`.
    pub default_model: Option<String>,
}

#[derive(Serialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StartOptions {
    /// In config order.
    pub accounts: Vec<StartAccount>,
    /// Folders the Codex app has as projects. A thread started anywhere else adds one.
    pub codex_projects: Vec<String>,
    /// The models a Claude Code start in the terminal can ask for.
    pub claude_models: Vec<ModelOption>,
}

/// What the sheet asks for.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct StartSpec {
    pub lane_id: String,
    pub account: String,
    pub place: Place,
    #[serde(default)]
    pub prompt: String,
    /// `--model` for a terminal start; None starts on the account's default.
    #[serde(default)]
    pub model: Option<String>,
    /// A terminal start in a new worktree Claude Code makes (`--worktree`). The apps have
    /// their own worktree choice on their new-session pages.
    #[serde(default)]
    pub worktree: bool,
    /// The todo this session is for, which it's linked to once it starts.
    #[serde(default)]
    pub todo: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
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
    Harness::from_config(&a.harness)
}

/// Where each account can start a session. The Claude app and the Codex app each run one
/// account, the one in the default folder (`~/.claude`, `~/.codex`); the terminal runs any
/// Claude Code or OpenCode account through its command. Antigravity starts conversations only
/// in its own window. `default_model` reads an account's default model.
pub fn options(accounts: &[AccountCfg], terminal: &str, codex_projects: Vec<String>, home: &Path, default_model: impl Fn(&AccountCfg) -> Option<String>) -> StartOptions {
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
                Harness::Opencode => places.push(PlaceOption { place: Place::Terminal, label: terminal.to_string() }),
                Harness::Antigravity => {
                    unavailable = Some(format!("Antigravity starts conversations in its own window, so {} can't start one from here.", a.name));
                }
            }
            let default_model = if harness == Harness::Claude { default_model(a) } else { None };
            StartAccount { name: a.name.clone(), harness, places, unavailable, default_model }
        })
        .collect();
    let claude_models = CLAUDE_MODELS.iter().map(|(id, label)| ModelOption { id: id.to_string(), label: label.to_string() }).collect();
    StartOptions { accounts, codex_projects, claude_models }
}

/// The `model` in a Claude account's settings.json, which Claude Code starts on without
/// `--model`. A repo's own `.claude/settings.json` can still override it.
pub fn claude_default_model(a: &AccountCfg) -> Option<String> {
    let bytes = std::fs::read(a.home_path().join("settings.json")).ok()?;
    let v: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    v.get("model").and_then(serde_json::Value::as_str).map(str::trim).filter(|m| !m.is_empty()).map(str::to_string)
}

/// A model name safe to type into the shell inside single quotes: letters, digits and `-._:[]`.
/// The quotes matter: zsh would read `fable[1m]` as a file pattern.
pub fn valid_model(m: &str) -> bool {
    !m.is_empty() && m.len() <= 80 && m.chars().all(|c| c.is_ascii_alphanumeric() || "-._:[]".contains(c))
}

/// The sheet's options for the accounts in `cfg`.
pub fn options_for(cfg: &Config) -> StartOptions {
    let codex_projects = cfg.accounts.iter().filter(|a| a.harness == "codex").flat_map(|a| codex::app_project_roots(&a.home_path())).collect();
    options(&cfg.accounts, &launch::terminal_name(&cfg.apps.terminal), codex_projects, &home(), claude_default_model)
}

/// The launch for one start. In the terminal Claude Code takes `session_id` as its own
/// (`--session-id`), so the board knows the session the moment it appears. The apps get a link
/// that opens their new-session page with the folder and prompt filled in; you send it there.
///
/// With `worktree`, Claude Code makes a new worktree for the session and chooses its folder,
/// branch and base itself (or a repo's WorktreeCreate hook does). `--worktree` takes an
/// optional name, so it goes first: followed by the prompt it would take the prompt as a name.
///
/// Attachments in the prompt (`images/…`, `files/…`) go to Claude Code as file mentions, with
/// their folders added so it can read them without asking; `--add-dir` takes several folders,
/// so each comes before `--session-id`, which ends it. The apps get the paths.
pub fn plan(account: &AccountCfg, place: Place, folder: &Path, prompt: &str, session_id: &str, model: Option<&str>, worktree: bool) -> Plan {
    plan_in(account, place, folder, prompt, session_id, model, worktree, (&attachments::images_dir(), &attachments::files_dir()))
}

#[allow(clippy::too_many_arguments)]
fn plan_in(account: &AccountCfg, place: Place, folder: &Path, prompt: &str, session_id: &str, model: Option<&str>, worktree: bool, (images_dir, files_dir): (&Path, &Path)) -> Plan {
    let (has_images, has_files) = attachments::kinds(prompt);
    // OpenCode has no --add-dir; it gets plain paths, as Codex does.
    let style = if place == Place::CodexApp || harness_of(account) == Harness::Opencode { Style::Path } else { Style::Mention };
    let prompt = attachments::expand_in(prompt, images_dir, files_dir, style);
    let prompt = prompt.trim();
    let dir = folder.to_string_lossy();
    match place {
        Place::Terminal => {
            // OpenCode picks its own session id, takes the prompt as an option, and has no
            // --worktree, --add-dir or --model.
            let opencode = harness_of(account) == Harness::Opencode;
            let mut input = account.command().to_string();
            let mut env = Vec::new();
            if worktree && !opencode {
                input.push_str(" --worktree");
            }
            for (has, var, dir) in [(has_images, IMAGES_VAR, images_dir), (has_files, FILES_VAR, files_dir)] {
                if has && !opencode {
                    input.push_str(&format!(" --add-dir \"${var}\""));
                    env.push((var.to_string(), dir.to_string_lossy().to_string()));
                }
            }
            if !opencode {
                input.push_str(&format!(" --session-id {session_id}"));
                env.push((SESSION_VAR.to_string(), format!("claude:{}:{session_id}", account.name)));
            }
            if let Some(m) = model.filter(|m| valid_model(m) && !opencode) {
                input.push_str(&format!(" --model '{m}'"));
            }
            if !prompt.is_empty() {
                input.push_str(if opencode { " --prompt" } else { "" });
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
pub fn as_argument(prompt: &str) -> String {
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
    let mut offered = options(std::slice::from_ref(account), &launch::terminal_name(&cfg.apps.terminal), Vec::new(), &home(), |_| None);
    let offered = offered.accounts.remove(0);
    if !offered.places.iter().any(|p| p.place == spec.place) {
        bail!(offered.unavailable.unwrap_or_else(|| format!("{} can't start a session there.", account.name)));
    }
    // The apps pick their model and worktree themselves, and only Claude Code takes them on the
    // command line, so they go only with a terminal start of a Claude Code account.
    let claude_terminal = spec.place == Place::Terminal && harness_of(account) == Harness::Claude;
    let model = spec.model.as_deref().map(str::trim).filter(|m| !m.is_empty() && claude_terminal);
    let worktree = spec.worktree && claude_terminal;
    if let Some(m) = model.filter(|m| !valid_model(m)) {
        bail!("{m:?} isn't a model name Codealong can pass to Claude Code.");
    }
    let id = new_session_id();
    let prompted = !spec.prompt.trim().is_empty();
    match plan(account, spec.place, folder, &spec.prompt, &id, model, worktree) {
        Plan::Terminal { cwd, input, env } => {
            launch::terminal_run(&cfg.apps.terminal, &cwd, &input, &env)?;
            // OpenCode names its own session; it shows on the board once it's written.
            match harness_of(account) {
                Harness::Opencode => Ok(Started { key: None, message: Some(format!("OpenCode is starting in {}.", lane.callsign)) }),
                _ => Ok(Started { key: Some(format!("claude:{}:{id}", account.name)), message: None }),
            }
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
            account("opencode", "opencode", "/Users/me/.local/share/opencode", Some("opencode")),
            account("antigravity", "antigravity", "/Users/me/.gemini/antigravity", None),
        ];
        let o = options(&accounts, "Ghostty", vec!["/r/kestrel".into()], Path::new("/Users/me"), |a| (a.name == "claude2").then(|| "claude-fable-5-1".to_string()));
        assert_eq!(places(&o, "claude"), vec![Place::Terminal, Place::ClaudeApp]);
        assert_eq!(places(&o, "claude2"), vec![Place::Terminal]);
        assert_eq!(places(&o, "codex"), vec![Place::CodexApp]);
        assert!(places(&o, "codex2").is_empty());
        assert!(o.accounts[3].unavailable.as_deref().unwrap_or("").contains("~/.codex"));
        assert_eq!(places(&o, "opencode"), vec![Place::Terminal]);
        assert!(places(&o, "antigravity").is_empty());
        assert!(o.accounts[5].unavailable.as_deref().unwrap_or("").contains("its own window"));
        assert_eq!(o.accounts[4].default_model, None);
        assert_eq!(o.accounts[0].places[0].label, "Ghostty");
        assert_eq!(o.codex_projects, vec!["/r/kestrel".to_string()]);
        assert_eq!(o.accounts[1].default_model.as_deref(), Some("claude-fable-5-1"));
        assert_eq!(o.accounts[0].default_model, None);
        assert!(o.claude_models.iter().any(|m| m.id == "opus[1m]"));
    }

    #[test]
    fn asks_claude_code_for_a_model_in_quotes() {
        let a = account("claude2", "claude", "~/.claude2", Some("claude2"));
        let id = "0b7c2a4e-1d2f-4e3a-9b8c-7d6e5f4a3b2c";
        let Plan::Terminal { input, .. } = plan(&a, Place::Terminal, Path::new("/r/x"), "Fix it", id, Some("fable[1m]"), false) else { panic!() };
        assert_eq!(input, format!("claude2 --session-id {id} --model 'fable[1m]' \"$CODEALONG_PROMPT\"\n"));
        // Nothing that could leave the quotes is ever typed.
        let Plan::Terminal { input, .. } = plan(&a, Place::Terminal, Path::new("/r/x"), "", id, Some("opus'; rm -rf ~ '"), false) else { panic!() };
        assert_eq!(input, format!("claude2 --session-id {id}\n"));
        assert!(valid_model("claude-opus-5-5") && valid_model("sonnet[1m]") && valid_model("opusplan"));
        assert!(!valid_model("") && !valid_model("opus fable") && !valid_model("$(x)") && !valid_model("a'b"));
        // The apps take no model.
        let claude = account("claude", "claude", "~/.claude", Some("claude"));
        assert_eq!(plan(&claude, Place::ClaudeApp, Path::new("/r/x"), "", id, Some("opus"), false), Plan::ClaudeApp { url: "claude://code/new?folder=%2Fr%2Fx".into() });
    }

    #[test]
    fn hands_the_terminal_the_prompt_in_the_environment() {
        let a = account("claude2", "claude", "~/.claude2", Some("claude2"));
        let id = "0b7c2a4e-1d2f-4e3a-9b8c-7d6e5f4a3b2c";
        let prompt = "Fix the \"flaky\" test\nthen run `npm test` and $(say done)";
        let Plan::Terminal { cwd, input, env } = plan(&a, Place::Terminal, Path::new("/r/atlas-app4"), &format!("  {prompt}\n"), id, None, false) else { panic!() };
        assert_eq!(cwd, PathBuf::from("/r/atlas-app4"));
        assert_eq!(input, format!("claude2 --session-id {id} \"$CODEALONG_PROMPT\"\n"));
        let session = (SESSION_VAR.to_string(), format!("claude:claude2:{id}"));
        assert_eq!(env, vec![session.clone(), (PROMPT_VAR.to_string(), prompt.to_string())]);
        let Plan::Terminal { input, env, .. } = plan(&a, Place::Terminal, Path::new("/r/x"), "  ", id, None, false) else { panic!() };
        assert_eq!(input, format!("claude2 --session-id {id}\n"));
        assert_eq!(env, vec![session]);
        let custom = account("claude3", "claude", "~/.claude3", Some("CLAUDE_CONFIG_DIR=~/.claude3 claude"));
        let Plan::Terminal { input, .. } = plan(&custom, Place::Terminal, Path::new("/r/x"), "", id, None, false) else { panic!() };
        assert_eq!(input, format!("CLAUDE_CONFIG_DIR=~/.claude3 claude --session-id {id}\n"));
    }

    #[test]
    fn starts_opencode_on_its_own_terms() {
        let a = account("opencode", "opencode", "~/.local/share/opencode", Some("opencode"));
        let id = "0b7c2a4e-1d2f-4e3a-9b8c-7d6e5f4a3b2c";
        let Plan::Terminal { input, env, .. } = plan(&a, Place::Terminal, Path::new("/r/x"), "Fix it", id, Some("opus"), true) else { panic!() };
        assert_eq!(input, "opencode --prompt \"$CODEALONG_PROMPT\"\n");
        assert_eq!(env, vec![(PROMPT_VAR.to_string(), "Fix it".to_string())]);
        let Plan::Terminal { input, env, .. } = plan(&a, Place::Terminal, Path::new("/r/x"), "", id, None, false) else { panic!() };
        assert_eq!(input, "opencode\n");
        assert!(env.is_empty());
    }

    #[test]
    fn hands_images_over_as_paths_claude_code_may_read() {
        let a = account("claude2", "claude", "~/.claude2", Some("claude2"));
        let id = "0b7c2a4e-1d2f-4e3a-9b8c-7d6e5f4a3b2c";
        let dir = Path::new("/Users/me/Library/Application Support/Codealong/images");
        let prompt = "Match this:\n![image](images/0123456789abcdef.png)";
        let Plan::Terminal { input, env, .. } = plan_in(&a, Place::Terminal, Path::new("/r/x"), prompt, id, Some("opus"), false, (dir, Path::new("/f"))) else { panic!() };
        assert_eq!(input, format!("claude2 --add-dir \"$CODEALONG_IMAGES\" --session-id {id} --model 'opus' \"$CODEALONG_PROMPT\"\n"));
        assert_eq!(
            env,
            vec![
                (IMAGES_VAR.to_string(), dir.to_string_lossy().to_string()),
                (SESSION_VAR.to_string(), format!("claude:claude2:{id}")),
                (PROMPT_VAR.to_string(), format!("Match this:\n@\"{}/0123456789abcdef.png\"", dir.display())),
            ]
        );
        // Without images, nothing changes.
        let Plan::Terminal { input, .. } = plan_in(&a, Place::Terminal, Path::new("/r/x"), "Fix it", id, None, false, (dir, Path::new("/f"))) else { panic!() };
        assert_eq!(input, format!("claude2 --session-id {id} \"$CODEALONG_PROMPT\"\n"));
        // Codex opens the path; the Claude app gets the same mention the terminal does.
        let codex = account("codex", "codex", "~/.codex", None);
        let Plan::CodexApp { url } = plan_in(&codex, Place::CodexApp, Path::new("/r/x"), prompt, id, None, false, (dir, Path::new("/f"))) else { panic!() };
        assert!(url.ends_with(&urlencoding::encode(&format!("Match this:\n![image]({}/0123456789abcdef.png)", dir.display())).to_string()));
        let claude = account("claude", "claude", "~/.claude", Some("claude"));
        let Plan::ClaudeApp { url } = plan_in(&claude, Place::ClaudeApp, Path::new("/r/x"), prompt, id, None, false, (dir, Path::new("/f"))) else { panic!() };
        assert!(url.contains(&urlencoding::encode("@\"/Users/me/Library").to_string()));
        // In a new worktree, the images folder comes after --worktree, which takes an optional name.
        let Plan::Terminal { input, .. } = plan_in(&a, Place::Terminal, Path::new("/r/x"), prompt, id, None, true, (dir, Path::new("/f"))) else { panic!() };
        assert_eq!(input, format!("claude2 --worktree --add-dir \"$CODEALONG_IMAGES\" --session-id {id} \"$CODEALONG_PROMPT\"\n"));
    }

    #[test]
    fn hands_other_files_over_from_their_own_folder() {
        let a = account("claude2", "claude", "~/.claude2", Some("claude2"));
        let id = "0b7c2a4e-1d2f-4e3a-9b8c-7d6e5f4a3b2c";
        let (images, files) = (Path::new("/i"), Path::new("/Users/me/Library/Application Support/Codealong/files"));
        let prompt = "Why does this fail?\n[crash.log](files/0123456789abcdef/crash.log)";
        let Plan::Terminal { input, env, .. } = plan_in(&a, Place::Terminal, Path::new("/r/x"), prompt, id, None, false, (images, files)) else { panic!() };
        assert_eq!(input, format!("claude2 --add-dir \"$CODEALONG_FILES\" --session-id {id} \"$CODEALONG_PROMPT\"\n"));
        assert_eq!(env[0], (FILES_VAR.to_string(), files.to_string_lossy().to_string()));
        assert_eq!(env[2].1, format!("Why does this fail?\n@\"{}/0123456789abcdef/crash.log\"", files.display()));
        // With an image too, both folders, each its own --add-dir.
        let both = format!("{prompt}\n![image](images/fedcba9876543210.png)");
        let Plan::Terminal { input, .. } = plan_in(&a, Place::Terminal, Path::new("/r/x"), &both, id, None, false, (images, files)) else { panic!() };
        assert_eq!(input, format!("claude2 --add-dir \"$CODEALONG_IMAGES\" --add-dir \"$CODEALONG_FILES\" --session-id {id} \"$CODEALONG_PROMPT\"\n"));
        // OpenCode gets the path, and no folders it has no option for.
        let o = account("opencode", "opencode", "~/.local/share/opencode", Some("opencode"));
        let Plan::Terminal { input, env, .. } = plan_in(&o, Place::Terminal, Path::new("/r/x"), prompt, id, None, false, (images, files)) else { panic!() };
        assert_eq!(input, "opencode --prompt \"$CODEALONG_PROMPT\"\n");
        assert_eq!(env[0].1, format!("Why does this fail?\n[crash.log]({}/0123456789abcdef/crash.log)", files.display()));
    }

    #[test]
    fn asks_claude_code_for_a_new_worktree_before_anything_it_could_take_as_its_name() {
        let a = account("claude2", "claude", "~/.claude2", Some("claude2"));
        let id = "0b7c2a4e-1d2f-4e3a-9b8c-7d6e5f4a3b2c";
        let Plan::Terminal { cwd, input, .. } = plan(&a, Place::Terminal, Path::new("/r/atlas"), "Fix it", id, Some("opus"), true) else { panic!() };
        assert_eq!(cwd, PathBuf::from("/r/atlas"));
        assert_eq!(input, format!("claude2 --worktree --session-id {id} --model 'opus' \"$CODEALONG_PROMPT\"\n"));
        // The apps choose their own worktrees.
        let claude = account("claude", "claude", "~/.claude", Some("claude"));
        assert_eq!(plan(&claude, Place::ClaudeApp, Path::new("/r/x"), "", id, None, true), Plan::ClaudeApp { url: "claude://code/new?folder=%2Fr%2Fx".into() });
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
            plan(&claude, Place::ClaudeApp, folder, "Fix it & ship\nnow", "id", None, false),
            Plan::ClaudeApp { url: "claude://code/new?folder=%2FUsers%2Fme%2FMy%20Repo&q=Fix%20it%20%26%20ship%0Anow".into() }
        );
        assert_eq!(plan(&claude, Place::ClaudeApp, folder, "", "id", None, false), Plan::ClaudeApp { url: "claude://code/new?folder=%2FUsers%2Fme%2FMy%20Repo".into() });
        assert_eq!(
            plan(&codex, Place::CodexApp, folder, "Review #12", "id", None, false),
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
