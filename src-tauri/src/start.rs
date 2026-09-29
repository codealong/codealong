//! Starting a session from Codealong. Codealong only launches: the harness runs the session,
//! makes a worktree when you ask it to, and the session reaches the board through its adapter
//! like any other. See *Surfaces*, New session.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Result};
use serde::Serialize;

use crate::codex;
use crate::config::{AccountCfg, Config};
use crate::attachments::{self, Style};
use crate::launch;
use crate::model::{Board, Harness, Meter, Severity, Usage};
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

/// The effort levels a Claude Code start can ask for, as `--effort` takes them, named the way
/// Claude Code's own picker names them (2.1.284: `xhigh` is Extra). A model that doesn't take
/// a level gets the nearest it does (`max` and `xhigh` become `high`), and Haiku takes none.
const CLAUDE_EFFORTS: [(&str, &str); 5] = [("low", "Low"), ("medium", "Medium"), ("high", "High"), ("xhigh", "Extra"), ("max", "Max")];

/// What a start asks for and gets back goes between devices too (a phone starts sessions on your
/// Macs), so those types are in codealong-peer.
pub use codealong_peer::api::{ModelOption, Place, PlaceOption, StartAccount, StartOptions, StartSpec, Started};

/// What a start does. `--snapshot --start-plan` prints it.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Plan {
    /// A new terminal window at `cwd`, with `env` set in its shell, typing `input`.
    Terminal { cwd: PathBuf, input: String, env: Vec<(String, String)> },
    /// `command` run by the login shell at `cwd`: a Claude Code background session. Its
    /// arguments are quoted in the command itself: the service the session runs under may start
    /// from this shell and keep its environment, so nothing is handed over in that.
    Background { cwd: PathBuf, command: String },
    /// A link the Claude app opens.
    ClaudeApp { url: String },
    /// A link the Codex app opens.
    CodexApp { url: String },
    /// A session Codealong runs over the account's ACP adapter, on `prompt`.
    Hosted { account: String, cwd: PathBuf, prompt: String, model: Option<String>, effort: Option<String> },
}

fn harness_of(a: &AccountCfg) -> Harness {
    Harness::from_config(&a.harness)
}


/// Where an account can start a session, the usual place first, or why it can't. The Claude
/// app and the Codex app each run one account, the one in the default folder (`~/.claude`,
/// `~/.codex`); the terminal runs any Claude Code, Codex or OpenCode account through its command.
/// Antigravity starts conversations only in its own window.
pub fn places_of(a: &AccountCfg, home: &Path) -> std::result::Result<Vec<Place>, String> {
    match harness_of(a) {
        Harness::Claude if a.home_path() == home.join(".claude") => Ok(vec![Place::Terminal, Place::Background, Place::ClaudeApp, Place::Codealong]),
        Harness::Claude => Ok(vec![Place::Terminal, Place::Background, Place::Codealong]),
        Harness::Codex if a.home_path() == home.join(".codex") => Ok(vec![Place::CodexApp, Place::Terminal, Place::Codealong]),
        Harness::Codex => Ok(vec![Place::Terminal, Place::Codealong]),
        Harness::Opencode => Ok(vec![Place::Terminal, Place::Codealong]),
        Harness::Antigravity => Err(format!("Antigravity starts conversations in its own window, so {} can't start one from here.", a.name)),
    }
}

/// Why an account's `start` can't be where its sessions start, in words.
pub fn check_start(a: &AccountCfg, place: &str, home: &Path) -> std::result::Result<(), String> {
    let offered = places_of(a, home).unwrap_or_default();
    let words = |ps: &[Place]| ps.iter().map(|p| format!("\"{}\"", p.word())).collect::<Vec<_>>().join(", ");
    match Place::from_word(place) {
        _ if offered.is_empty() => Err(format!("{} can't start sessions from Codealong, so it takes no start.", a.name)),
        Some(p) if offered.contains(&p) => Ok(()),
        _ => Err(format!("{} can start sessions in {}; \"{}\" isn't one of them.", a.name, words(&offered), place.trim())),
    }
}

/// The places an account offers, with the one its `start` names first.
pub fn ordered_places(a: &AccountCfg, home: &Path) -> std::result::Result<Vec<Place>, String> {
    let mut places = places_of(a, home)?;
    if let Some(i) = a.start.as_deref().and_then(Place::from_word).and_then(|p| places.iter().position(|&q| q == p)) {
        let p = places.remove(i);
        places.insert(0, p);
    }
    Ok(places)
}

/// Where each account can start a session, as the sheet offers it. `default_model` reads an
/// account's default model.
pub fn options(accounts: &[AccountCfg], terminal: &str, codex_projects: Vec<String>, home: &Path, default_model: impl Fn(&AccountCfg) -> Option<String>) -> StartOptions {
    let accounts = accounts
        .iter()
        .map(|a| {
            let harness = harness_of(a);
            let (places, unavailable) = match ordered_places(a, home) {
                Ok(ps) => (ps, None),
                Err(why) => (Vec::new(), Some(why)),
            };
            let places = places
                .into_iter()
                .map(|place| {
                    let label = match place {
                        Place::Terminal => terminal.to_string(),
                        Place::Background => "Background".into(),
                        Place::ClaudeApp => "Claude app".into(),
                        Place::CodexApp => "Codex app".into(),
                        Place::Codealong => "Codealong".into(),
                    };
                    PlaceOption { place, label }
                })
                .collect();
            let chosen = a.start.as_deref().and_then(Place::from_word);
            let default_model = if harness == Harness::Claude { default_model(a) } else { None };
            StartAccount { name: a.name.clone(), harness, places, chosen, unavailable, default_model, preferred: a.preferred }
        })
        .collect();
    let listed = |list: &[(&str, &str)]| list.iter().map(|(id, label)| ModelOption { id: id.to_string(), label: label.to_string() }).collect();
    StartOptions { accounts, codex_projects, claude_models: listed(&CLAUDE_MODELS), claude_efforts: listed(&CLAUDE_EFFORTS) }
}

/// Why Auto starts the account it does: its weekly limit resets soonest, so its room is the
/// first to be lost (`reset`); it has the most room (`room`); or there are no numbers to go by
/// (`first`).
#[derive(Serialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum AutoReason {
    Reset,
    Room,
    First,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AutoPick {
    pub account: String,
    pub reason: AutoReason,
    /// The weekly reset, for `reset`.
    pub resets_at: Option<i64>,
}

/// An account Auto could start: whether it's preferred (see `preferred_names`) and the model
/// it would start on, None when that could be any.
pub struct Candidate<'a> {
    pub name: &'a str,
    pub preferred: bool,
    pub model: Option<&'a str>,
}

/// An account this close to any limit could stall a new session.
const NEAR_LIMIT: f64 = 90.0;

/// The accounts Auto prefers among those that can start sessions (`startable`, in config
/// order): the ones config.toml marks `preferred`, else those of the first account's harness,
/// so that without a star Auto keeps to one agent until its accounts are out of room.
/// `src/start.ts`'s `preferredNames` has the same rule.
pub fn preferred_names<'a>(startable: &[&'a AccountCfg]) -> Vec<&'a str> {
    if startable.iter().any(|a| a.preferred) {
        return startable.iter().filter(|a| a.preferred).map(|a| a.name.as_str()).collect();
    }
    let first = startable.first().map(|a| a.harness.as_str());
    startable.iter().filter(|a| Some(a.harness.as_str()) == first).map(|a| a.name.as_str()).collect()
}

/// Whether a session on `model` spends a limit for `limit_model` (None: every model). A model
/// that names no family could be any of them, so it spends every limit; `src/start.ts`'s
/// `metersFor` has the same rule.
pub fn spends(model: Option<&str>, limit_model: Option<&str>) -> bool {
    let m = model.unwrap_or("").to_lowercase();
    match limit_model {
        None => true,
        Some(_) if !["fable", "opus", "sonnet", "haiku"].iter().any(|f| m.contains(f)) => true,
        Some(l) => m.contains(&l.to_lowercase()),
    }
}

/// The account Auto starts, from `candidates` in config order. Only the limits each one's model
/// spends count, and an account at 90% of one of them has no room. Preferred accounts with room
/// go first, then a preferred one without numbers yet, then the others the same way, and only
/// when all are near a limit the least full of them. Among accounts with room, the one whose
/// weekly limit resets soonest wins, so no week's room goes unused; without a reset under way,
/// the one whose fullest limit is least full. Signed-out accounts are passed over.
/// `src/start.ts`'s `autoPick` has the same rule.
pub fn auto_pick(candidates: &[Candidate], usages: &[Usage], now: i64) -> Option<AutoPick> {
    struct Measured<'a> {
        name: &'a str,
        preferred: bool,
        full: f64,
        near: bool,
        reset: Option<i64>,
    }
    let near = |m: &Meter| m.severity == Severity::Critical || m.percent >= NEAR_LIMIT;
    let mut measured: Vec<Measured> = Vec::new();
    let mut unknown: Vec<&Candidate> = Vec::new();
    for c in candidates {
        let u = usages.iter().find(|u| u.account == c.name);
        if u.is_some_and(|u| u.needs_sign_in) {
            continue;
        }
        let meters: Vec<&Meter> = u.map(|u| u.meters.iter().filter(|m| spends(c.model, m.model.as_deref())).collect()).unwrap_or_default();
        if meters.is_empty() {
            unknown.push(c);
            continue;
        }
        measured.push(Measured {
            name: c.name,
            preferred: c.preferred,
            full: meters.iter().map(|m| m.percent).fold(0.0, f64::max),
            near: meters.iter().any(|m| near(m)),
            reset: meters.iter().filter(|m| m.key == "week" || m.key.starts_with("model:")).filter_map(|m| m.resets_at).filter(|&at| at > now).min(),
        });
    }
    let least_full = |pool: &[&Measured]| pool.iter().min_by(|a, b| a.full.total_cmp(&b.full)).map(|m| AutoPick { account: m.name.to_string(), reason: AutoReason::Room, resets_at: None });
    let best = |pool: Vec<&Measured>| {
        let soonest = pool.iter().filter(|m| m.reset.is_some()).min_by(|a, b| a.reset.cmp(&b.reset).then(a.full.total_cmp(&b.full)));
        match soonest {
            Some(m) => Some(AutoPick { account: m.name.to_string(), reason: AutoReason::Reset, resets_at: m.reset }),
            None => least_full(&pool),
        }
    };
    let first = |c: &Candidate| AutoPick { account: c.name.to_string(), reason: AutoReason::First, resets_at: None };
    for preferred in [true, false] {
        let roomy: Vec<&Measured> = measured.iter().filter(|m| m.preferred == preferred && !m.near).collect();
        if !roomy.is_empty() {
            return best(roomy);
        }
        if let Some(c) = unknown.iter().find(|c| c.preferred == preferred) {
            return Some(first(c));
        }
    }
    let all: Vec<&Measured> = measured.iter().collect();
    least_full(&all).or_else(|| candidates.iter().find(|c| c.preferred).or(candidates.first()).map(first))
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

/// Whether `--effort` takes this level.
pub fn valid_effort(e: &str) -> bool {
    effort_levels().any(|id| id == e)
}

/// The levels `--effort` takes, lowest first.
pub fn effort_levels() -> impl Iterator<Item = &'static str> {
    CLAUDE_EFFORTS.iter().map(|(id, _)| *id)
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
///
/// `model` and `effort` go only to Claude Code (`--model`, `--effort`, or its adapter's options);
/// the apps and the other agents pick their own.
#[allow(clippy::too_many_arguments)]
pub fn plan(account: &AccountCfg, place: Place, folder: &Path, prompt: &str, session_id: &str, model: Option<&str>, effort: Option<&str>, worktree: bool) -> Plan {
    plan_in(account, place, folder, prompt, session_id, model, effort, worktree, (&attachments::images_dir(), &attachments::files_dir()))
}

#[allow(clippy::too_many_arguments)]
fn plan_in(account: &AccountCfg, place: Place, folder: &Path, prompt: &str, session_id: &str, model: Option<&str>, effort: Option<&str>, worktree: bool, (images_dir, files_dir): (&Path, &Path)) -> Plan {
    let effort = effort.filter(|e| valid_effort(e) && harness_of(account) == Harness::Claude);
    let (has_images, has_files) = attachments::kinds(prompt);
    let prompt_in = prompt;
    // OpenCode and Codex have no --add-dir; they get plain paths.
    let style = if matches!(harness_of(account), Harness::Opencode | Harness::Codex) { Style::Path } else { Style::Mention };
    let prompt = attachments::expand_in(prompt, images_dir, files_dir, style);
    let prompt = prompt.trim();
    let dir = folder.to_string_lossy();
    match place {
        Place::Terminal if harness_of(account) == Harness::Codex => {
            // Codex picks its own thread id. Its images go with -i, which takes several, so `--`
            // ends them before the prompt; other files are paths it can read.
            let mut input = account.command().to_string();
            let mut env = Vec::new();
            for rel in attachments::referenced(prompt_in).iter().filter(|r| r.starts_with("images/")) {
                if let Some(name) = rel.strip_prefix("images/") {
                    input.push_str(&format!(" -i {}", sh_quote(&images_dir.join(name).to_string_lossy())));
                }
            }
            if !prompt.is_empty() {
                input.push_str(&format!(" -- \"${PROMPT_VAR}\""));
                env.push((PROMPT_VAR.to_string(), prompt.to_string()));
            }
            input.push('\n');
            Plan::Terminal { cwd: folder.to_path_buf(), input, env }
        }
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
            if let Some(e) = effort {
                input.push_str(&format!(" --effort {e}"));
            }
            if !prompt.is_empty() {
                input.push_str(if opencode { " --prompt" } else { "" });
                input.push_str(&format!(" \"${PROMPT_VAR}\""));
                env.push((PROMPT_VAR.to_string(), as_argument(prompt)));
            }
            input.push('\n');
            Plan::Terminal { cwd: folder.to_path_buf(), input, env }
        }
        Place::Background => {
            // `--bg` picks the session's ID itself and ignores `--session-id`.
            let mut command = account.command().to_string();
            if worktree {
                command.push_str(" --worktree");
            }
            for (has, dir) in [(has_images, images_dir), (has_files, files_dir)] {
                if has {
                    command.push_str(&format!(" --add-dir {}", sh_quote(&dir.to_string_lossy())));
                }
            }
            command.push_str(" --bg");
            if let Some(m) = model.filter(|m| valid_model(m)) {
                command.push_str(&format!(" --model '{m}'"));
            }
            if let Some(e) = effort {
                command.push_str(&format!(" --effort {e}"));
            }
            if !prompt.is_empty() {
                command.push_str(&format!(" {}", sh_quote(&as_argument(prompt))));
            }
            Plan::Background { cwd: folder.to_path_buf(), command }
        }
        Place::ClaudeApp => Plan::ClaudeApp { url: with_param(format!("claude://code/new?folder={}", urlencoding::encode(&dir)), "q", prompt) },
        Place::CodexApp => Plan::CodexApp { url: with_param(format!("codex://threads/new?path={}", urlencoding::encode(&dir)), "prompt", prompt) },
        // The agents take the prompt as text, so attachments go as paths they can read.
        Place::Codealong => Plan::Hosted {
            account: account.name.clone(),
            cwd: folder.to_path_buf(),
            prompt: attachments::expand_in(prompt_in, images_dir, files_dir, Style::Path).trim().to_string(),
            model: model.filter(|m| valid_model(m)).map(String::from),
            effort: effort.map(String::from),
        },
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

/// `s` in single quotes for the shell, each quote in it closed, escaped and reopened.
fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// The job ID `--bg` prints (`backgrounded · 34b864ad`), which is also where the session's own
/// ID begins.
pub fn background_job(output: &str) -> Option<String> {
    output.lines().find_map(|l| {
        let rest = l.trim().strip_prefix("backgrounded")?;
        let job: String = rest.trim_start_matches(|c: char| !c.is_ascii_hexdigit()).chars().take_while(char::is_ascii_hexdigit).collect();
        (job.len() >= 8).then_some(job)
    })
}

/// A background job's session ID, from the service's roster (`<home>/daemon/roster.json`,
/// `workers.<job>.sessionId`), where it's written as the job starts.
pub fn background_session(roster: &serde_json::Value, job: &str) -> Option<String> {
    roster.get("workers")?.get(job)?.get("sessionId")?.as_str().filter(|id| id.starts_with(job)).map(str::to_string)
}

/// Runs a background start in the login shell, which knows the account's command even when it's
/// an alias, and returns the job's session ID once the service has it. Output goes to a file,
/// not a pipe: a service the command starts would hold a pipe open long after it returns.
fn start_background(account: &AccountCfg, cwd: &Path, command: &str) -> Result<(String, Option<String>)> {
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
    let out_path = std::env::temp_dir().join(format!("codealong-bg-{}-{}.txt", std::process::id(), new_session_id()));
    let out = std::fs::File::create(&out_path)?;
    let mut child = Command::new(&shell)
        .args(["-ilc", command])
        .current_dir(cwd)
        .env_remove(SESSION_VAR)
        .stdin(Stdio::null())
        .stdout(out.try_clone()?)
        .stderr(out)
        .spawn()
        .map_err(|e| anyhow!("couldn't start {shell}: {e}"))?;
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if started.elapsed() > Duration::from_secs(60) {
            let _ = child.kill();
            let _ = std::fs::remove_file(&out_path);
            bail!("{} didn't start in the background within a minute.", account.name);
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let output = std::fs::read_to_string(&out_path).unwrap_or_default();
    let _ = std::fs::remove_file(&out_path);
    let Some(job) = background_job(&output) else {
        // Claude Code says why: an untrusted folder, a bad option, not signed in.
        let why = output.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with("Starting background service")).last().unwrap_or("").to_string();
        bail!(if why.is_empty() { format!("{} didn't start in the background (exit {}).", account.name, status.code().unwrap_or(-1)) } else { why });
    };
    let roster = account.home_path().join("daemon/roster.json");
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let id = std::fs::read(&roster).ok().and_then(|b| serde_json::from_slice(&b).ok()).and_then(|v| background_session(&v, &job));
        if id.is_some() || Instant::now() > deadline {
            return Ok((job, id));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
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

/// Whether a start could go into a new worktree: only Claude Code makes one, started in a
/// terminal or in the background.
pub fn offers_worktree(cfg: &Config, spec: &StartSpec) -> bool {
    matches!(spec.place, Place::Terminal | Place::Background) && cfg.accounts.iter().any(|a| a.name == spec.account && harness_of(a) == Harness::Claude)
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
    let claude_terminal = matches!(spec.place, Place::Terminal | Place::Background) && harness_of(account) == Harness::Claude;
    // Codealong hands a Claude Code session's model to its adapter, which takes the same names.
    let claude_hosted = spec.place == Place::Codealong && harness_of(account) == Harness::Claude;
    let model = spec.model.as_deref().map(str::trim).filter(|m| !m.is_empty() && (claude_terminal || claude_hosted));
    let effort = spec.effort.as_deref().map(str::trim).filter(|e| !e.is_empty() && (claude_terminal || claude_hosted));
    let worktree = spec.worktree && claude_terminal;
    if let Some(m) = model.filter(|m| !valid_model(m)) {
        bail!("{m:?} isn't a model name Codealong can pass to Claude Code.");
    }
    if let Some(e) = effort.filter(|e| !valid_effort(e)) {
        bail!("{e:?} isn't an effort level; Claude Code takes {}.", effort_levels().collect::<Vec<_>>().join(", "));
    }
    let id = new_session_id();
    let prompted = !spec.prompt.trim().is_empty();
    match plan(account, spec.place, folder, &spec.prompt, &id, model, effort, worktree) {
        Plan::Terminal { cwd, input, env } => {
            let near = crate::board::terminal_neighbours(&board.lanes, folder);
            launch::terminal_run(&cfg.apps.terminal, launch::Placement { new_terminal: cfg.apps.new_terminal, near: &near }, &cwd, &input, &env)?;
            // OpenCode and Codex name their own sessions; one shows on the board once it's written.
            match harness_of(account) {
                Harness::Opencode => Ok(Started { key: None, message: Some(format!("OpenCode is starting in {}.", lane.callsign)) }),
                Harness::Codex => Ok(Started { key: None, message: Some(format!("Codex is starting in {}.", lane.callsign)) }),
                _ => Ok(Started { key: Some(format!("claude:{}:{id}", account.name)), message: None }),
            }
        }
        Plan::Background { cwd, command } => {
            let (job, id) = start_background(account, &cwd, &command)?;
            match id {
                Some(id) => Ok(Started { key: Some(format!("claude:{}:{id}", account.name)), message: None }),
                None => Ok(Started { key: None, message: Some(format!("{} is starting in the background as {job}.", account.name)) }),
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
        Plan::Hosted { cwd, prompt, model, effort, .. } => {
            let key = crate::acp::start(account, &cwd, &prompt, model.as_deref(), effort.as_deref())?;
            Ok(Started { key: Some(key), message: None })
        }
        Plan::CodexApp { url } => {
            if prompted {
                return codex_app_start(account, folder, &spec.prompt, &url);
            }
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

/// Forks a session into a new terminal in its folder (`Adapter::fork`): the copy goes on from
/// what the session had written, and the session itself carries on as it was.
pub fn fork(board: &Board, cfg: &Config, key: &str) -> Result<Started> {
    let (s, account) = crate::harness::find(board, cfg, key)?;
    // The id is typed into a shell, and the harnesses' ids are letters, digits, - and _.
    if s.id.is_empty() || !s.id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        bail!("Codealong won't type {}'s id into a shell: {:?}.", s.title, s.id);
    }
    // Its own worktree, not one it only visits.
    let own = |l: &&crate::model::Lane| l.sessions.iter().any(|x| x.key == key && x.visitor.is_none());
    let holds = |l: &&crate::model::Lane| l.sessions.iter().any(|x| x.key == key);
    let lane = board.lanes.iter().find(own).or_else(|| board.lanes.iter().find(holds)).ok_or_else(|| anyhow!("That session isn't on the board any more."))?;
    let cwd = if Path::new(&s.cwd).is_dir() { PathBuf::from(&s.cwd) } else { PathBuf::from(&lane.path) };
    let fork = crate::harness::adapter(s.harness).fork(&crate::harness::Ctx { account }, s)?;
    let near = crate::board::terminal_neighbours(&board.lanes, &cwd);
    launch::terminal_run(&cfg.apps.terminal, launch::Placement { new_terminal: cfg.apps.new_terminal, near: &near }, &cwd, &fork.input, &fork.env)?;
    // One whose harness names the copy shows on the board once it's written.
    let message = fork.key.is_none().then(|| format!("{} is opening a fork of {} in {}.", s.harness.word(), s.title, lane.callsign));
    Ok(Started { key: fork.key, message })
}

/// Starts a Codex app thread on a prompt. The app's new-thread link only fills the prompt in, so
/// Codealong makes the thread itself, and the app, once it has the thread open, runs the first
/// turn as if you'd sent it there. When that fails the link still fills the prompt in: in the
/// thread just made if there is one, else on the new-thread page.
fn codex_app_start(account: &AccountCfg, folder: &Path, prompt: &str, link: &str) -> Result<Started> {
    let home = account.home_path();
    let text = attachments::expand(prompt.trim(), Style::Path);
    let thread = match crate::appserver::new_thread(&home, folder, None) {
        Ok(t) => t,
        Err(_) => {
            launch::open_codex(link)?;
            return Ok(Started { key: None, message: Some("Your prompt is waiting in a new Codex thread. Send it from there.".into()) });
        }
    };
    let model = thread.model.as_deref().map(|m| (m, thread.effort.as_deref()));
    match crate::send::codex_start_turn(&home, &thread.id, &text, model) {
        Ok(()) => Ok(Started { key: Some(format!("codex:{}:{}", account.name, thread.id)), message: None }),
        Err(_) => {
            launch::open_codex(&format!("{}?prompt={}", launch::codex_thread_url(&thread.id), urlencoding::encode(&text)))?;
            Ok(Started { key: Some(format!("codex:{}:{}", account.name, thread.id)), message: Some("Your prompt is waiting in a new Codex thread. Send it from there.".into()) })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account(name: &str, harness: &str, home: &str, command: Option<&str>) -> AccountCfg {
        AccountCfg { name: name.into(), harness: harness.into(), home: home.into(), command: command.map(String::from), start: None, preferred: false, acp: None }
    }

    fn meter(key: &str, percent: f64, resets_at: Option<i64>) -> Meter {
        let severity = if percent >= 90.0 { Severity::Critical } else { Severity::Normal };
        Meter { key: key.into(), label: key.into(), window: key.into(), percent, resets_at, severity, model: None }
    }

    const NOW: i64 = 1_790_200_000_000;
    const HOUR: i64 = 3_600_000;

    /// An account's 5-hour and weekly use, and in how many hours its week resets.
    fn usage(account: &str, session: f64, week: f64, week_resets_in: Option<i64>) -> Usage {
        Usage {
            account: account.into(),
            harness: Harness::Claude,
            plan: None,
            meters: vec![meter("session", session, Some(NOW + 2 * HOUR)), meter("week", week, week_resets_in.map(|h| NOW + h * HOUR))],
            as_of: Some(NOW),
            error: None,
            needs_sign_in: false,
            loading: false,
            preferred: false,
            rate_limited: false,
        }
    }

    fn cand(name: &str, preferred: bool) -> Candidate<'_> {
        Candidate { name, preferred, model: None }
    }

    fn pick(cs: &[Candidate], us: &[Usage]) -> (String, AutoReason) {
        let p = auto_pick(cs, us, NOW).unwrap();
        (p.account, p.reason)
    }

    #[test]
    fn auto_spends_the_week_that_resets_soonest() {
        let cs = [cand("claude", true), cand("claude2", true)];
        // claude2 has less room, but its week resets first: its room is the first to be lost.
        assert_eq!(pick(&cs, &[usage("claude", 10.0, 20.0, Some(100)), usage("claude2", 30.0, 70.0, Some(90))]), ("claude2".into(), AutoReason::Reset));
        // A week that hasn't started has nothing to lose.
        assert_eq!(pick(&cs, &[usage("claude", 10.0, 20.0, Some(100)), usage("claude2", 0.0, 0.0, None)]).0, "claude");
        // Without a reset under way, the most room.
        assert_eq!(pick(&cs, &[usage("claude", 38.0, 20.0, None), usage("claude2", 12.0, 45.0, None)]), ("claude".into(), AutoReason::Room));
        // Near a limit is no room, while another has some.
        assert_eq!(pick(&cs, &[usage("claude", 10.0, 20.0, Some(100)), usage("claude2", 95.0, 70.0, Some(6))]).0, "claude");
        // All near: the least full.
        assert_eq!(pick(&cs, &[usage("claude", 92.0, 20.0, Some(100)), usage("claude2", 95.0, 70.0, Some(6))]), ("claude".into(), AutoReason::Room));
    }

    #[test]
    fn auto_keeps_to_preferred_accounts_while_they_have_room() {
        let cs = [cand("claude", true), cand("codex", false)];
        let codex_soon = usage("codex", 0.0, 10.0, Some(3));
        assert_eq!(pick(&cs, &[usage("claude", 50.0, 80.0, Some(100)), codex_soon.clone()]).0, "claude");
        // A preferred account without numbers yet still goes first.
        assert_eq!(pick(&cs, &[codex_soon.clone()]), ("claude".into(), AutoReason::First));
        // Out of room: the others.
        assert_eq!(pick(&cs, &[usage("claude", 50.0, 95.0, Some(100)), codex_soon.clone()]), ("codex".into(), AutoReason::Reset));
        // Signed out counts as out.
        let signed_out = Usage { needs_sign_in: true, ..usage("claude", 0.0, 0.0, None) };
        assert_eq!(pick(&cs, &[signed_out, codex_soon]).0, "codex");
        assert_eq!(pick(&[cand("codex", false), cand("claude", true)], &[]).0, "claude");
    }

    #[test]
    fn auto_counts_only_the_models_limits() {
        assert!(spends(Some("opus"), None));
        assert!(!spends(Some("opus[1m]"), Some("Fable")));
        assert!(spends(Some("claude-opus-5-5"), Some("Opus")));
        assert!(spends(Some("fable"), Some("Fable")));
        assert!(spends(Some("best"), Some("Fable")));
        assert!(spends(None, Some("Fable")));
        let mut u = usage("claude", 10.0, 20.0, Some(20));
        u.meters.push(Meter { model: Some("Fable".into()), ..meter("model:Fable", 100.0, Some(NOW + 20 * HOUR)) });
        let us = [u, usage("claude2", 20.0, 50.0, None)];
        let on = |m| [Candidate { model: Some(m), ..cand("claude", true) }, Candidate { model: Some(m), ..cand("claude2", true) }];
        assert_eq!(pick(&on("opus"), &us).0, "claude");
        assert_eq!(pick(&on("fable"), &us).0, "claude2");
    }

    #[test]
    fn prefers_starred_accounts_else_the_first_ones_agent() {
        let a = |name: &str, harness: &str, preferred: bool| AccountCfg { preferred, ..account(name, harness, &format!("~/.{name}"), None) };
        let (claude, claude2, codex) = (a("claude", "claude", false), a("claude2", "claude", false), a("codex", "codex", false));
        assert_eq!(preferred_names(&[&codex, &claude, &claude2]), vec!["codex"]);
        assert_eq!(preferred_names(&[&claude, &codex, &claude2]), vec!["claude", "claude2"]);
        let starred = a("codex", "codex", true);
        assert_eq!(preferred_names(&[&claude, &starred, &claude2]), vec!["codex"]);
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
        assert_eq!(places(&o, "claude"), vec![Place::Terminal, Place::Background, Place::ClaudeApp, Place::Codealong]);
        assert_eq!(places(&o, "claude2"), vec![Place::Terminal, Place::Background, Place::Codealong]);
        assert_eq!(places(&o, "codex"), vec![Place::CodexApp, Place::Terminal, Place::Codealong]);
        assert_eq!(places(&o, "codex2"), vec![Place::Terminal, Place::Codealong]);
        assert_eq!(places(&o, "opencode"), vec![Place::Terminal, Place::Codealong]);
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
    fn an_accounts_start_setting_goes_first() {
        let home = Path::new("/Users/me");
        let with = |name: &str, h: &str, dir: &str, start: &str| AccountCfg { start: Some(start.into()), ..account(name, h, dir, None) };
        let accounts = vec![with("claude", "claude", "/Users/me/.claude", "claude-app"), with("claude2", "claude", "/Users/me/.claude2", "background"), account("claude3", "claude", "/Users/me/.claude3", None)];
        let o = options(&accounts, "Ghostty", vec![], home, |_| None);
        assert_eq!(places(&o, "claude"), vec![Place::ClaudeApp, Place::Terminal, Place::Background, Place::Codealong]);
        assert_eq!(o.accounts[0].chosen, Some(Place::ClaudeApp));
        assert_eq!(places(&o, "claude2"), vec![Place::Background, Place::Terminal, Place::Codealong]);
        assert_eq!(places(&o, "claude3"), vec![Place::Terminal, Place::Background, Place::Codealong]);
        assert_eq!(o.accounts[2].chosen, None);

        assert!(check_start(&accounts[0], "claude-app", home).is_ok());
        let err = check_start(&accounts[1], "claude-app", home).unwrap_err();
        assert!(err.contains("\"terminal\", \"background\", \"codealong\""), "{err}");
        assert!(check_start(&account("codex", "codex", "/Users/me/.codex", None), "terminal", home).is_ok());
        assert!(check_start(&account("codex2", "codex", "/Users/me/.codex2", None), "codex-app", home).is_err());
        assert!(check_start(&account("ag", "antigravity", "/Users/me/.gemini/antigravity", None), "terminal", home).unwrap_err().contains("takes no start"));
        assert!(check_start(&accounts[1], "somewhere", home).is_err());
    }

    #[test]
    fn reads_the_todos_a_start_is_for_old_and_new() {
        let read = |json: &str| serde_json::from_str::<StartSpec>(json).unwrap().todos;
        let base = r#""laneId":"/r/x","account":"claude","place":"terminal""#;
        assert_eq!(read(&format!(r#"{{{base},"todos":["t-1","t-2"]}}"#)), vec!["t-1", "t-2"]);
        // As windows and the command sent it before a start could be for several.
        assert_eq!(read(&format!(r#"{{{base},"todo":"t-1"}}"#)), vec!["t-1"]);
        assert!(read(&format!(r#"{{{base},"todo":null}}"#)).is_empty());
        assert!(read(&format!("{{{base}}}")).is_empty());
    }

    #[test]
    fn asks_claude_code_for_a_model_in_quotes() {
        let a = account("claude2", "claude", "~/.claude2", Some("claude2"));
        let id = "0b7c2a4e-1d2f-4e3a-9b8c-7d6e5f4a3b2c";
        let Plan::Terminal { input, .. } = plan(&a, Place::Terminal, Path::new("/r/x"), "Fix it", id, Some("fable[1m]"), None, false) else { panic!() };
        assert_eq!(input, format!("claude2 --session-id {id} --model 'fable[1m]' \"$CODEALONG_PROMPT\"\n"));
        // Nothing that could leave the quotes is ever typed.
        let Plan::Terminal { input, .. } = plan(&a, Place::Terminal, Path::new("/r/x"), "", id, Some("opus'; rm -rf ~ '"), None, false) else { panic!() };
        assert_eq!(input, format!("claude2 --session-id {id}\n"));
        assert!(valid_model("claude-opus-5-5") && valid_model("sonnet[1m]") && valid_model("opusplan"));
        assert!(!valid_model("") && !valid_model("opus fable") && !valid_model("$(x)") && !valid_model("a'b"));
        // The apps take no model.
        let claude = account("claude", "claude", "~/.claude", Some("claude"));
        assert_eq!(plan(&claude, Place::ClaudeApp, Path::new("/r/x"), "", id, Some("opus"), None, false), Plan::ClaudeApp { url: "claude://code/new?folder=%2Fr%2Fx".into() });
    }

    #[test]
    fn asks_claude_code_for_an_effort_level_it_knows() {
        let a = account("claude2", "claude", "~/.claude2", Some("claude2"));
        let id = "0b7c2a4e-1d2f-4e3a-9b8c-7d6e5f4a3b2c";
        let Plan::Terminal { input, .. } = plan(&a, Place::Terminal, Path::new("/r/x"), "Fix it", id, Some("opus"), Some("xhigh"), false) else { panic!() };
        assert_eq!(input, format!("claude2 --session-id {id} --model 'opus' --effort xhigh \"$CODEALONG_PROMPT\"\n"));
        let Plan::Background { command, .. } = plan(&a, Place::Background, Path::new("/r/x"), "", id, None, Some("low"), false) else { panic!() };
        assert_eq!(command, "claude2 --bg --effort low");
        let Plan::Hosted { effort, .. } = plan(&a, Place::Codealong, Path::new("/r/x"), "Fix it", id, None, Some("max"), false) else { panic!() };
        assert_eq!(effort.as_deref(), Some("max"));
        // Only the levels Claude Code takes are typed.
        let Plan::Terminal { input, .. } = plan(&a, Place::Terminal, Path::new("/r/x"), "", id, None, Some("high; rm -rf ~"), false) else { panic!() };
        assert_eq!(input, format!("claude2 --session-id {id}\n"));
        assert!(valid_effort("medium") && !valid_effort("ultracode") && !valid_effort(""));
        // The other agents choose their own.
        let o = account("opencode", "opencode", "~/.local/share/opencode", Some("opencode"));
        let Plan::Terminal { input, .. } = plan(&o, Place::Terminal, Path::new("/r/x"), "", id, None, Some("high"), false) else { panic!() };
        assert_eq!(input, "opencode\n");
        let cx = account("codex", "codex", "~/.codex", Some("codex"));
        let Plan::Terminal { input, .. } = plan(&cx, Place::Terminal, Path::new("/r/x"), "", id, None, Some("high"), false) else { panic!() };
        assert_eq!(input, "codex\n");
        let o = options(&[a], "Ghostty", Vec::new(), Path::new("/Users/me"), |_| None);
        assert_eq!(o.claude_efforts.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(), ["low", "medium", "high", "xhigh", "max"]);
    }

    #[test]
    fn hands_the_terminal_the_prompt_in_the_environment() {
        let a = account("claude2", "claude", "~/.claude2", Some("claude2"));
        let id = "0b7c2a4e-1d2f-4e3a-9b8c-7d6e5f4a3b2c";
        let prompt = "Fix the \"flaky\" test\nthen run `npm test` and $(say done)";
        let Plan::Terminal { cwd, input, env } = plan(&a, Place::Terminal, Path::new("/r/atlas-app4"), &format!("  {prompt}\n"), id, None, None, false) else { panic!() };
        assert_eq!(cwd, PathBuf::from("/r/atlas-app4"));
        assert_eq!(input, format!("claude2 --session-id {id} \"$CODEALONG_PROMPT\"\n"));
        let session = (SESSION_VAR.to_string(), format!("claude:claude2:{id}"));
        assert_eq!(env, vec![session.clone(), (PROMPT_VAR.to_string(), prompt.to_string())]);
        let Plan::Terminal { input, env, .. } = plan(&a, Place::Terminal, Path::new("/r/x"), "  ", id, None, None, false) else { panic!() };
        assert_eq!(input, format!("claude2 --session-id {id}\n"));
        assert_eq!(env, vec![session]);
        let custom = account("claude3", "claude", "~/.claude3", Some("CLAUDE_CONFIG_DIR=~/.claude3 claude"));
        let Plan::Terminal { input, .. } = plan(&custom, Place::Terminal, Path::new("/r/x"), "", id, None, None, false) else { panic!() };
        assert_eq!(input, format!("CLAUDE_CONFIG_DIR=~/.claude3 claude --session-id {id}\n"));
    }

    #[test]
    fn starts_in_the_background_with_everything_quoted_in_the_command() {
        let a = account("claude2", "claude", "~/.claude2", Some("claude2"));
        let id = "0b7c2a4e-1d2f-4e3a-9b8c-7d6e5f4a3b2c";
        let (images, files) = (Path::new("/Users/me/Library/Application Support/Codealong/images"), Path::new("/f"));
        let prompt = "Don't break $HOME or `this`\n![image](images/0123456789abcdef.png)";
        let Plan::Background { cwd, command } = plan_in(&a, Place::Background, Path::new("/r/x"), prompt, id, Some("fable[1m]"), None, true, (images, files)) else { panic!() };
        assert_eq!(cwd, PathBuf::from("/r/x"));
        // --worktree first (it takes an optional name), --add-dir before --bg (it takes several
        // folders), and no --session-id, which --bg ignores.
        assert_eq!(
            command,
            format!("claude2 --worktree --add-dir '{}' --bg --model 'fable[1m]' 'Don'\\''t break $HOME or `this`\n@\"{}/0123456789abcdef.png\"'", images.display(), images.display())
        );
        let Plan::Background { command, .. } = plan(&a, Place::Background, Path::new("/r/x"), "  ", id, None, None, false) else { panic!() };
        assert_eq!(command, "claude2 --bg");
        // A one-word prompt keeps its padding, so it isn't read as a command.
        let Plan::Background { command, .. } = plan(&a, Place::Background, Path::new("/r/x"), "update", id, None, None, false) else { panic!() };
        assert_eq!(command, "claude2 --bg 'update '");
    }

    #[test]
    fn finds_the_background_session_it_started() {
        let out = "backgrounded · 34b864ad\n  claude agents             list sessions\n  claude attach 34b864ad    open in this terminal\n";
        assert_eq!(background_job(out).as_deref(), Some("34b864ad"));
        assert_eq!(background_job("backgrounded · 3a6096ed (idle — send a prompt to start)\n").as_deref(), Some("3a6096ed"));
        assert_eq!(background_job("Workspace not trusted. Run `claude` in /r/x once and accept the trust prompt, then retry.\n"), None);
        let roster = serde_json::json!({"workers": {"34b864ad": {"sessionId": "34b864ad-b397-4f98-96fe-b06f41bb2653"}, "11111111": {"sessionId": "22222222-0000-4000-8000-000000000000"}}});
        assert_eq!(background_session(&roster, "34b864ad").as_deref(), Some("34b864ad-b397-4f98-96fe-b06f41bb2653"));
        assert_eq!(background_session(&roster, "11111111"), None);
        assert_eq!(background_session(&roster, "99999999"), None);
    }

    /// Starts a real background session on Haiku in a folder Claude Code trusts, checks that its
    /// transcript has the prompt exactly as given, then stops and removes it. Opt in:
    /// `CODEALONG_TEST_FOLDER=<trusted git folder> cargo test background_round_trip -- --ignored`.
    #[test]
    #[ignore]
    fn background_round_trip() {
        let folder = PathBuf::from(std::env::var("CODEALONG_TEST_FOLDER").expect("CODEALONG_TEST_FOLDER"));
        let a = account("claude", "claude", "~/.claude", Some("claude"));
        let prompt = "Reply with just ok. Don't expand $HOME or `this` or $(say nope) -v";
        let Plan::Background { cwd, command } = plan(&a, Place::Background, &folder, prompt, "unused", Some("haiku"), None, false) else { panic!() };
        let (job, id) = start_background(&a, &cwd, &command).unwrap();
        let id = id.expect("the roster names the session");
        assert!(id.starts_with(&job));
        let mut seen = false;
        for _ in 0..100 {
            let found = crate::util::home().join(".claude/projects").read_dir().unwrap().flatten().map(|d| d.path().join(format!("{id}.jsonl"))).find(|p| p.exists());
            if let Some(text) = found.and_then(|p| std::fs::read_to_string(p).ok()) {
                // Nothing in it needs escaping in JSON.
                if text.contains(prompt) {
                    seen = true;
                    break;
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
        let _ = crate::procs::run("claude", &["stop", &job], Some(&folder), std::time::Duration::from_secs(20));
        let _ = crate::procs::run("claude", &["rm", &job], Some(&folder), std::time::Duration::from_secs(20));
        assert!(seen, "the transcript never showed the prompt");
    }

    #[test]
    fn starts_opencode_on_its_own_terms() {
        let a = account("opencode", "opencode", "~/.local/share/opencode", Some("opencode"));
        let id = "0b7c2a4e-1d2f-4e3a-9b8c-7d6e5f4a3b2c";
        let Plan::Terminal { input, env, .. } = plan(&a, Place::Terminal, Path::new("/r/x"), "Fix it", id, Some("opus"), None, true) else { panic!() };
        assert_eq!(input, "opencode --prompt \"$CODEALONG_PROMPT\"\n");
        assert_eq!(env, vec![(PROMPT_VAR.to_string(), "Fix it".to_string())]);
        let Plan::Terminal { input, env, .. } = plan(&a, Place::Terminal, Path::new("/r/x"), "", id, None, None, false) else { panic!() };
        assert_eq!(input, "opencode\n");
        assert!(env.is_empty());
    }

    #[test]
    fn hands_images_over_as_paths_claude_code_may_read() {
        let a = account("claude2", "claude", "~/.claude2", Some("claude2"));
        let id = "0b7c2a4e-1d2f-4e3a-9b8c-7d6e5f4a3b2c";
        let dir = Path::new("/Users/me/Library/Application Support/Codealong/images");
        let prompt = "Match this:\n![image](images/0123456789abcdef.png)";
        let Plan::Terminal { input, env, .. } = plan_in(&a, Place::Terminal, Path::new("/r/x"), prompt, id, Some("opus"), None, false, (dir, Path::new("/f"))) else { panic!() };
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
        let Plan::Terminal { input, .. } = plan_in(&a, Place::Terminal, Path::new("/r/x"), "Fix it", id, None, None, false, (dir, Path::new("/f"))) else { panic!() };
        assert_eq!(input, format!("claude2 --session-id {id} \"$CODEALONG_PROMPT\"\n"));
        // The Codex CLI gets the image with -i as well as its path, `--` before the prompt.
        let cx2 = account("codex2", "codex", "~/.codex2", Some("CODEX_HOME=~/.codex2 codex"));
        let Plan::Terminal { input, env, .. } = plan_in(&cx2, Place::Terminal, Path::new("/r/x"), prompt, id, Some("opus"), None, true, (dir, Path::new("/f"))) else { panic!() };
        assert_eq!(input, format!("CODEX_HOME=~/.codex2 codex -i '{}/0123456789abcdef.png' -- \"$CODEALONG_PROMPT\"\n", dir.display()));
        assert_eq!(env, vec![(PROMPT_VAR.to_string(), format!("Match this:\n![image]({}/0123456789abcdef.png)", dir.display()))]);
        let Plan::Terminal { input, env, .. } = plan_in(&cx2, Place::Terminal, Path::new("/r/x"), "", id, None, None, false, (dir, Path::new("/f"))) else { panic!() };
        assert_eq!((input.as_str(), env.len()), ("CODEX_HOME=~/.codex2 codex\n", 0));
        // Codex opens the path; the Claude app gets the same mention the terminal does.
        let codex = account("codex", "codex", "~/.codex", None);
        let Plan::CodexApp { url } = plan_in(&codex, Place::CodexApp, Path::new("/r/x"), prompt, id, None, None, false, (dir, Path::new("/f"))) else { panic!() };
        assert!(url.ends_with(&urlencoding::encode(&format!("Match this:\n![image]({}/0123456789abcdef.png)", dir.display())).to_string()));
        let claude = account("claude", "claude", "~/.claude", Some("claude"));
        let Plan::ClaudeApp { url } = plan_in(&claude, Place::ClaudeApp, Path::new("/r/x"), prompt, id, None, None, false, (dir, Path::new("/f"))) else { panic!() };
        assert!(url.contains(&urlencoding::encode("@\"/Users/me/Library").to_string()));
        // In a new worktree, the images folder comes after --worktree, which takes an optional name.
        let Plan::Terminal { input, .. } = plan_in(&a, Place::Terminal, Path::new("/r/x"), prompt, id, None, None, true, (dir, Path::new("/f"))) else { panic!() };
        assert_eq!(input, format!("claude2 --worktree --add-dir \"$CODEALONG_IMAGES\" --session-id {id} \"$CODEALONG_PROMPT\"\n"));
    }

    #[test]
    fn hands_other_files_over_from_their_own_folder() {
        let a = account("claude2", "claude", "~/.claude2", Some("claude2"));
        let id = "0b7c2a4e-1d2f-4e3a-9b8c-7d6e5f4a3b2c";
        let (images, files) = (Path::new("/i"), Path::new("/Users/me/Library/Application Support/Codealong/files"));
        let prompt = "Why does this fail?\n[crash.log](files/0123456789abcdef/crash.log)";
        let Plan::Terminal { input, env, .. } = plan_in(&a, Place::Terminal, Path::new("/r/x"), prompt, id, None, None, false, (images, files)) else { panic!() };
        assert_eq!(input, format!("claude2 --add-dir \"$CODEALONG_FILES\" --session-id {id} \"$CODEALONG_PROMPT\"\n"));
        assert_eq!(env[0], (FILES_VAR.to_string(), files.to_string_lossy().to_string()));
        assert_eq!(env[2].1, format!("Why does this fail?\n@\"{}/0123456789abcdef/crash.log\"", files.display()));
        // With an image too, both folders, each its own --add-dir.
        let both = format!("{prompt}\n![image](images/fedcba9876543210.png)");
        let Plan::Terminal { input, .. } = plan_in(&a, Place::Terminal, Path::new("/r/x"), &both, id, None, None, false, (images, files)) else { panic!() };
        assert_eq!(input, format!("claude2 --add-dir \"$CODEALONG_IMAGES\" --add-dir \"$CODEALONG_FILES\" --session-id {id} \"$CODEALONG_PROMPT\"\n"));
        // OpenCode gets the path, and no folders it has no option for.
        let o = account("opencode", "opencode", "~/.local/share/opencode", Some("opencode"));
        let Plan::Terminal { input, env, .. } = plan_in(&o, Place::Terminal, Path::new("/r/x"), prompt, id, None, None, false, (images, files)) else { panic!() };
        assert_eq!(input, "opencode --prompt \"$CODEALONG_PROMPT\"\n");
        assert_eq!(env[0].1, format!("Why does this fail?\n[crash.log]({}/0123456789abcdef/crash.log)", files.display()));
    }

    #[test]
    fn asks_claude_code_for_a_new_worktree_before_anything_it_could_take_as_its_name() {
        let a = account("claude2", "claude", "~/.claude2", Some("claude2"));
        let id = "0b7c2a4e-1d2f-4e3a-9b8c-7d6e5f4a3b2c";
        let Plan::Terminal { cwd, input, .. } = plan(&a, Place::Terminal, Path::new("/r/atlas"), "Fix it", id, Some("opus"), None, true) else { panic!() };
        assert_eq!(cwd, PathBuf::from("/r/atlas"));
        assert_eq!(input, format!("claude2 --worktree --session-id {id} --model 'opus' \"$CODEALONG_PROMPT\"\n"));
        // The apps choose their own worktrees.
        let claude = account("claude", "claude", "~/.claude", Some("claude"));
        assert_eq!(plan(&claude, Place::ClaudeApp, Path::new("/r/x"), "", id, None, None, true), Plan::ClaudeApp { url: "claude://code/new?folder=%2Fr%2Fx".into() });
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
            plan(&claude, Place::ClaudeApp, folder, "Fix it & ship\nnow", "id", None, None, false),
            Plan::ClaudeApp { url: "claude://code/new?folder=%2FUsers%2Fme%2FMy%20Repo&q=Fix%20it%20%26%20ship%0Anow".into() }
        );
        assert_eq!(plan(&claude, Place::ClaudeApp, folder, "", "id", None, None, false), Plan::ClaudeApp { url: "claude://code/new?folder=%2FUsers%2Fme%2FMy%20Repo".into() });
        assert_eq!(
            plan(&codex, Place::CodexApp, folder, "Review #12", "id", None, None, false),
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
