//! Cursor (3.x): the app's agent chats and its command line's, `cursor-agent`, as one account
//! kept in `~/.cursor`. The app keeps every chat in its global state database
//! (`~/Library/Application Support/Cursor/User/globalStorage/state.vscdb`, SQLite in WAL mode):
//! `composerHeaders` lists them, and its key-value table, `cursorDiskKV`, holds each one's
//! `composerData:<id>` (its status, and the order of its messages) and its messages,
//! `bubbleId:<chat>:<message>`. The command line keeps a chat in
//! `chats/<md5 of its folder>/<id>/store.db` (its messages as protobufs) and writes the same
//! transcript of JSON lines the app does,
//! `projects/<folder as a slug>/agent-transcripts/<id>/<id>.jsonl`, which is what one of its
//! chats' panel reads. Neither lists the other's chats. See .claude/rules/cursor.md.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use rusqlite::Connection;
use serde_json::Value;

use crate::config::AccountCfg;
use crate::harness::{Applied, Ctx, Look};
use crate::model::{Harness, ModeKind, Session, SessionMode, Status, Surface};
use crate::procs;
use crate::transcript::{clip_input, clip_output, item, kept_whole, placed, Item, Page};
use crate::util::{code, command_dir, file_name, note_work, one_line, only_looks, parse_rfc3339_ms};

/// The command line as its installer names it (it also installs it as `agent`).
pub const CLI: &str = "cursor-agent";
/// The app's bundle ID.
pub const APP_BUNDLE_ID: &str = "com.todesktop.230313mzl4w4u92";
/// The app's global state database, under its data folder.
const STATE_DB: &str = "state.vscdb";
/// A command line chat's own database.
const STORE_DB: &str = "store.db";
/// How many of a chat's latest messages are read for what it's on.
const TAIL_MESSAGES: usize = 60;
/// How much of a transcript's end is read for what a command line chat is on.
const TAIL_BYTES: u64 = 256 * 1024;
/// The folders of a workspace, and of your home, that Cursor's command line reads skills from,
/// which is where Codealong runs a skill by name (`skill_mention`). Claude Code's and Codex's
/// are its "third-party" ones, which it always reads and the app reads unless its setting is
/// turned off; the app reads Grok's `.grok/skills` too. A skill two of them have at the same
/// place is one skill (3.22.7's agent, 2026.08.11's command line).
const SKILL_FOLDERS: [&str; 4] = [".cursor/skills", ".claude/skills", ".codex/skills", ".agents/skills"];
/// The skills Cursor ships, which it keeps in your home.
const OWN_SKILLS: &str = ".cursor/skills-cursor";
/// How many folders down a skills folder Cursor looks for a `SKILL.md`.
const SKILL_DEPTH: usize = 10;

/// A folder of skills as the command line reads it (its `CustomCommands`): a skill is a command
/// named by its folder (`getSkillIdForPath`), and one kept off the menu or whose
/// `metadata.surfaces` doesn't list `cli` is left out (`parseSkillMarkdown`). It follows a link
/// out of a folder only among your personal skills (`findSkillMarkdownFiles`), not a
/// workspace's or the ones it ships.
fn skill_root(dir: PathBuf, personal: bool) -> crate::skills::Root {
    crate::skills::Root::skills(dir, SKILL_DEPTH).as_commands(crate::skills::AsCommands { surface: "cli", contained: !personal })
}

/// The app's global state database for the account kept in `home`. Only the account in
/// `~/.cursor` has the app's chats: one in a folder of its own (`CURSOR_CONFIG_DIR`) is the
/// command line's alone. The app's data folder is in the Library of the home `~/.cursor` is in.
pub fn app_db(home: &Path) -> Option<PathBuf> {
    if file_name(home) != ".cursor" {
        return None;
    }
    Some(crate::os::dirs::app_data(home.parent()?, "Cursor", "Cursor").join("User/globalStorage").join(STATE_DB))
}

/// The command that runs an account's command line: its own, when config.toml gives one.
pub fn cli_command(account: &AccountCfg) -> &str {
    account.command.as_deref().unwrap_or(CLI)
}

/// Whether `cursor-agent` is installed for the user whose home is `home`: where its installer
/// puts it, or, for you, anywhere on your PATH.
pub fn cli_installed(home: &Path) -> bool {
    crate::detect::is_executable(&home.join(".local/bin").join(CLI)) || (home == crate::util::home() && crate::detect::on_user_path(CLI))
}

/// The file in a folder's records (`projects/<slug>`) that says the command line trusts the
/// folder. Without it there, or in the records of a folder above that isn't your home or one of
/// the two at the top, the command line stops at "Workspace Trust Required" before it writes its
/// chat, for each folder it works in, one `--add-dir` adds too (2026.07.09 to 2026.10.16).
const TRUSTED: &str = ".workspace-trusted";

/// The folder the command line keeps its records of folders in (`projects`), whatever account
/// it runs as: `CURSOR_DATA_DIR`, else `~/.cursor` in `home`, where an account of its own sets
/// only `CURSOR_CONFIG_DIR` (`config_dir_of`). `env` is the environment it starts with
/// (`launch_var`).
fn data_dir(env: impl Fn(&str) -> Option<String>, home: &Path) -> PathBuf {
    env("CURSOR_DATA_DIR").map(|d| d.trim().to_string()).filter(|d| !d.is_empty()).map(PathBuf::from).unwrap_or_else(|| home.join(".cursor"))
}

/// A variable as a start of an account's command line has it: set at the start of the
/// account's own command (`CURSOR_DATA_DIR=~/data cursor-agent`), else by your login shell,
/// whose environment the terminal's command gets and Codealong, opened from the Dock, doesn't.
/// The login shell is asked once a name.
fn launch_var(command: &str, name: &str) -> Option<String> {
    static ASKED: std::sync::Mutex<Vec<(String, Option<String>)>> = std::sync::Mutex::new(Vec::new());
    if let Some(v) = assigned(command, name) {
        return Some(v);
    }
    let mut asked = ASKED.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((_, v)) = asked.iter().find(|(n, _)| n == name) {
        return v.clone();
    }
    let v = procs::login_var(name);
    asked.push((name.to_string(), v.clone()));
    v
}

/// The value `command` gives `name` in the assignments it starts with, its `~` or `$HOME`
/// expanded, as detect.rs writes an account's folder there.
fn assigned(command: &str, name: &str) -> Option<String> {
    let v = command.split_whitespace().take_while(|w| w.contains('=')).find_map(|w| w.strip_prefix(name)?.strip_prefix('='))?;
    let v = v.trim_matches(['"', '\'']);
    let v = v.strip_prefix("$HOME").map(|rest| format!("~{rest}")).unwrap_or_else(|| v.to_string());
    Some(crate::util::expand(&v).to_string_lossy().into_owned())
}

/// Trusts `folder` for the command line whose records are in `data` (`data_dir`), as Trust at
/// its prompt does: `{trustedAt, workspacePath}` in the folder's `.workspace-trusted`. By the
/// path it's given and by its real path, which the command line goes by when it starts there
/// (the shell's folder as the kernel has it). False when it trusts both already, and then
/// nothing is written. Not the home folder or one above it.
pub fn trust_folder(data: &Path, folder: &Path, user_home: &Path) -> anyhow::Result<bool> {
    if user_home.starts_with(folder) {
        anyhow::bail!("{} holds the home folder", folder.display());
    }
    let given = folder.to_string_lossy().trim_end_matches('/').to_string();
    let real = std::fs::canonicalize(folder).ok().map(|p| p.to_string_lossy().into_owned());
    let mut wrote = false;
    for path in std::iter::once(given.clone()).chain(real.filter(|r| *r != given)) {
        let records = data.join("projects").join(slug(&path));
        if records.join(TRUSTED).exists() {
            continue;
        }
        std::fs::create_dir_all(&records)?;
        let marker = serde_json::json!({ "trustedAt": crate::util::iso_utc(crate::util::now_ms()), "workspacePath": path });
        crate::util::write_whole(&records.join(TRUSTED), &serde_json::to_string_pretty(&marker)?)?;
        wrote = true;
    }
    Ok(wrote)
}

/// A chat, the app's or the command line's.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Chat {
    pub id: String,
    pub title: Option<String>,
    /// The folder it works in: a worktree the app made for it, else its window's folder.
    pub cwd: Option<String>,
    pub created_at: i64,
    pub last_updated: i64,
    /// Archived in Cursor.
    pub archived: bool,
    /// The app's word for where it stands (`composerData.status`): `none`, `generating`,
    /// `completed` or `aborted`. Empty for the command line's.
    pub status: String,
    /// Something in it waits on you: a call to allow, a question (`hasBlockingPendingActions`).
    pub blocking: bool,
    pub branch: Option<String>,
    /// The model it's set to, by Cursor's id for it (`grok-4.7`, `default` for Auto); the
    /// session says it by the name Cursor's picker does (`Reader::names`).
    pub model: Option<String>,
    /// The effort that model is set to, as Cursor writes the level (`high`, `xhigh`).
    pub effort: Option<String>,
    /// The mode it's in, as Cursor names it: the app's `unifiedMode` (`agent`, `chat` for Ask,
    /// `plan`…), the command line's `mode` (`default`, `plan`, `debug`, `search` for Ask).
    pub mode: Option<String>,
    /// Its own auto-run setting, where it has one: the app's `agentApprovalModeOverride`
    /// (`unrestricted` once you pick Run Everything in it), the command line's `approvalMode`
    /// (`allowlist`, `unrestricted`, `auto-review`).
    pub approval: Option<String>,
    /// The mode its header says (`composerHeaders.value.unifiedMode`), written up to a minute
    /// after composerData's: a switch between two modes of one length leaves composerData as long
    /// as it was, so it isn't read again, and this is how the switch shows.
    pub header_mode: Option<String>,
    /// How many messages it has.
    pub messages: i64,
    /// How long its composerData is: the app rewrites it with every message, so a change says
    /// to read it again, which a length says without parsing it.
    pub written: i64,
    /// It's the command line's: a `cursor-agent` process runs it, in a terminal or on its own.
    pub cli: bool,
    /// A command line process runs it but hasn't written it yet: the command line asks what it
    /// must (whether to trust the folder, to allow a workspace's MCP servers) before it does,
    /// and writes none of that down.
    pub unwritten: bool,
    /// The command line's chat folder (`chats/<hash>/<id>`), and its transcript, with when that
    /// was last written.
    pub dir: Option<PathBuf>,
    pub transcript: Option<PathBuf>,
    pub transcript_at: Option<i64>,
    /// What its latest messages say.
    pub tail: Tail,
}

impl Chat {
    /// When its conversation last changed, which a chat at rest has stood still since: for the
    /// command line's, its transcript's last write or its newest prompt, since its database is
    /// written for a mode switched too, which says nothing new; for the app's, its last write.
    pub fn said_at(&self) -> i64 {
        match self.transcript_at {
            Some(at) if self.cli => at.max(self.tail.last_at.unwrap_or(0)),
            _ => self.last_updated,
        }
    }

    /// The app says a turn of it is under way.
    pub fn generating(&self) -> bool {
        self.status == "generating"
    }
}

/// What a chat's latest messages say.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Tail {
    /// The latest reply's start, and its end, for Jev.
    pub last_text: Option<String>,
    pub last_text_tail: Option<String>,
    /// The latest tool call, as a line.
    pub last_tool: Option<String>,
    /// The calls since your last prompt that haven't finished: what runs now, or waits on you.
    pub open: Vec<Call>,
    /// Where its latest calls worked, oldest first.
    pub work: Vec<String>,
    /// What you first asked it, and when you last did.
    pub first_prompt: Option<String>,
    pub last_prompt_at: Option<i64>,
    /// When its latest message was written: a reply goes on long after the prompt the app's
    /// `lastUpdatedAt` is the time of.
    pub last_at: Option<i64>,
    /// Its latest message says the turn ended on an error: what it said.
    pub error: Option<String>,
    /// Its latest turn hasn't ended: for the command line's, whose record says when one does.
    pub turn_open: bool,
}

/* ---- The app's chats ---- */

fn flag(v: &Value, key: &str) -> bool {
    match v.get(key) {
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_i64().is_some_and(|n| n != 0),
        _ => false,
    }
}

/// The app's chats changed since `since_ms`, plus any in `keep` (pinned ones), newest first,
/// without what their messages say (`read_state`, `app_tail`). Subagents' chats are left out,
/// and so are the empty ones every new window makes (no name, nothing written) and drafts.
pub fn read_app_chats(db: &Connection, since_ms: i64, keep: &HashSet<String>) -> rusqlite::Result<Vec<Chat>> {
    let mut stmt = db.prepare("SELECT composerId, createdAt, lastUpdatedAt, isArchived, CAST(value AS TEXT), checkpointAt FROM composerHeaders WHERE isSubagent = 0")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<i64>>(1)?, r.get::<_, Option<i64>>(2)?, r.get::<_, Option<i64>>(3)?, r.get::<_, Option<String>>(4)?, r.get::<_, Option<i64>>(5)?)))?;
    let mut out = Vec::new();
    for (id, created, updated, archived, value, checkpoint) in rows.flatten() {
        let v: Value = value.as_deref().and_then(|t| serde_json::from_str(t).ok()).unwrap_or(Value::Null);
        let name = v.get("name").and_then(Value::as_str).map(str::trim).filter(|n| !n.is_empty()).map(str::to_string);
        if flag(&v, "isDraft") || (name.is_none() && updated.is_none()) {
            continue;
        }
        let created_at = created.unwrap_or(0);
        // `lastUpdatedAt` is when you last sent it something; a checkpoint is written as a turn
        // goes on, and the latest message's time comes with its tail (`Tail::last_at`).
        let last_updated = updated.unwrap_or(created_at).max(checkpoint.unwrap_or(0));
        if last_updated < since_ms && !keep.contains(&id) {
            continue;
        }
        let cwd = v.pointer("/workspaceIdentifier/uri/fsPath").and_then(Value::as_str).map(|p| p.trim_end_matches('/').to_string()).filter(|p| !p.is_empty());
        // The branch it last worked on, of the repos it tracked.
        let branches = v.get("trackedGitRepos").and_then(Value::as_array).into_iter().flatten().flat_map(|r| r.get("branches").and_then(Value::as_array).into_iter().flatten());
        let branch = branches.max_by_key(|b| b.get("lastInteractionAt").and_then(Value::as_i64).unwrap_or(0)).and_then(|b| b.get("branchName")?.as_str()).map(str::to_string);
        let header_mode = v.get("unifiedMode").and_then(Value::as_str).filter(|m| !m.is_empty()).map(str::to_string);
        out.push(Chat { id, title: name, cwd, created_at, last_updated, archived: archived.unwrap_or(0) != 0, blocking: flag(&v, "hasBlockingPendingActions"), branch, header_mode, ..Chat::default() });
    }
    out.sort_by(|a, b| b.last_updated.cmp(&a.last_updated));
    Ok(out)
}

/// What a chat's composerData says now. The app writes it with each message, where its header
/// can lag a minute behind; it's read only once it has been written since (`written`).
fn read_state(db: &Connection, c: &mut Chat) {
    let got = db.query_row(
        "SELECT json_extract(t, '$.status'), json_extract(t, '$.lastUpdatedAt'), json_extract(t, '$.name'), json_extract(t, '$.hasBlockingPendingActions'), \
         json_extract(t, '$.gitWorktree.worktreePath'), json_extract(t, '$.gitWorktree.branchName'), \
         json_array_length(t, '$.fullConversationHeadersOnly'), length(t), json_extract(t, '$.unifiedMode'), json_extract(t, '$.agentApprovalModeOverride') \
         FROM (SELECT CAST(value AS TEXT) AS t FROM cursorDiskKV WHERE key = ?1)",
        [format!("composerData:{}", c.id)],
        |r| {
            Ok((
                r.get::<_, Option<String>>(0)?,
                r.get::<_, Option<i64>>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<i64>>(3)?,
                r.get::<_, Option<String>>(4)?,
                r.get::<_, Option<String>>(5)?,
                r.get::<_, Option<i64>>(6)?,
                r.get::<_, Option<i64>>(7)?,
                r.get::<_, Option<String>>(8)?,
                r.get::<_, Option<String>>(9)?,
            ))
        },
    );
    let Ok((status, updated, name, blocking, worktree, branch, messages, written, mode, approval)) = got else { return };
    c.status = status.unwrap_or_default();
    c.last_updated = c.last_updated.max(updated.unwrap_or(0));
    if let Some(name) = name.map(|n| n.trim().to_string()).filter(|n| !n.is_empty()) {
        c.title = Some(name);
    }
    // The header's word may be a minute old, and an aborted chat's is often left set.
    c.blocking = blocking.is_some_and(|b| b != 0) || (c.blocking && c.generating());
    if let Some(w) = worktree.filter(|w| !w.is_empty()) {
        c.cwd = Some(w.trim_end_matches('/').to_string());
    }
    c.branch = branch.or(c.branch.take());
    c.messages = messages.unwrap_or(0);
    c.written = written.unwrap_or(0);
    c.mode = mode.filter(|m| !m.is_empty());
    c.approval = approval.filter(|a| !a.is_empty());
}

/// What a model's effort is among its parameters, by model: `reasoning_effort`, `effort`, or
/// `reasoning` (with `none` and `extra-high` among its levels). A `thinking` switch is no level.
const EFFORT_PARAMETERS: [&str; 3] = ["reasoning_effort", "effort", "reasoning"];

/// The model a chat's picker set and its effort, from composerData's `modelConfig`, as the app
/// reads it: the first of `selectedModels` (`{modelId, parameters: [{id, value}]}`), else the
/// `modelName` older chats keep alone (which can be a legacy one beside a newer pick). The effort
/// is that model's effort parameter, None where the picker left the model's own.
fn model_of(config: &Value) -> (Option<String>, Option<String>) {
    let selected = config.pointer("/selectedModels/0").unwrap_or(&Value::Null);
    let model = selected.get("modelId").or(config.get("modelName")).and_then(Value::as_str).filter(|m| !m.is_empty()).map(str::to_string);
    (model, effort_of(selected))
}

fn effort_of(selected: &Value) -> Option<String> {
    let parameters = selected.get("parameters")?.as_array()?;
    let value = |id: &str| parameters.iter().find(|p| p.get("id").and_then(Value::as_str) == Some(id))?.get("value")?.as_str().filter(|v| !v.is_empty()).map(str::to_string);
    EFFORT_PARAMETERS.iter().find_map(|id| value(id))
}

/// The app's settings for the signed-in user, one JSON row of the ItemTable (half a megabyte,
/// so SQLite walks it): the models it offers, and how it shows the account's usage.
const APPLICATION_USER: &str = "src.vs.platform.reactivestorage.browser.reactiveStorageServiceImpl.persistentStorage.applicationUser";

/// A model Cursor offers: its id (`grok-4.7`, `default` for Auto), which the command line's
/// `--model` takes too, the name its picker shows (Grok 4.7, Auto), and whether the picker shows
/// it at all.
#[derive(Debug, Clone, PartialEq)]
struct Offered {
    id: String,
    name: String,
    shown: bool,
}

/// The models Cursor offers, in the order of the list it keeps, `availableDefaultModels2` in its
/// settings (`APPLICATION_USER`). Its picker shows one that's on by default (`defaultOn`, on when
/// missing) or turned on in its Settings (`aiSettings.modelOverrideEnabled`), unless it's turned
/// off there (`modelOverrideDisabled`), as 3.22.7's `autoEnableSelectedModels` reads them. About
/// 7 ms here (2026-10-14).
fn offered_models(db: &Connection) -> Vec<Offered> {
    let read = || -> rusqlite::Result<Vec<Offered>> {
        let mut stmt = db.prepare(
            "SELECT json_extract(m.value, '$.name'), json_extract(m.value, '$.clientDisplayName'), json_extract(m.value, '$.defaultOn'), \
             json_extract(u.t, '$.aiSettings.modelOverrideEnabled'), json_extract(u.t, '$.aiSettings.modelOverrideDisabled') \
             FROM (SELECT CAST(value AS TEXT) AS t FROM ItemTable WHERE key = ?1) AS u, json_each(u.t, '$.availableDefaultModels2') AS m",
        )?;
        let rows = stmt.query_map([APPLICATION_USER], |r| Ok((r.get::<_, Option<String>>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, Option<i64>>(2)?, r.get::<_, Option<String>>(3)?, r.get::<_, Option<String>>(4)?)))?;
        let listed = |ids: Option<String>| ids.and_then(|t| serde_json::from_str::<Vec<String>>(&t).ok()).unwrap_or_default();
        Ok(rows
            .flatten()
            .filter_map(|(id, name, default_on, on, off)| {
                let (id, name) = (id.filter(|i| !i.is_empty())?, name.filter(|n| !n.trim().is_empty())?);
                let shown = !listed(off).contains(&id) && (default_on.map_or(true, |d| d != 0) || listed(on).contains(&id));
                Some(Offered { id, name, shown })
            })
            .collect())
    };
    read().unwrap_or_default()
}

/// Cursor's names for its models by id, the ones its picker hides included.
fn model_names(db: &Connection) -> HashMap<String, String> {
    offered_models(db).into_iter().map(|m| (m.id, m.name)).collect()
}

/// The models a start or a resume can ask the command line for: the ones Cursor's picker shows,
/// by the id `--model` takes, which gives a model its default settings (the command line's
/// `handleInitialModel`, 2026.08.11). Auto is one of them, `auto` to the command line where the
/// app says `default`: the command line keeps the last model it was started on as its default
/// (`cli-config.json`), and a resume goes on with the chat's own, so the account's own line isn't
/// always Auto. Read from the app's list, so an account without the app offers none.
fn start_models(home: &Path) -> Vec<crate::start::ModelOption> {
    let Some(db) = app_db(home).and_then(|p| crate::harness::open_database(&p)) else { return Vec::new() };
    let id = |m: &Offered| if m.id == "default" { "auto".to_string() } else { m.id.clone() };
    offered_models(&db).into_iter().filter(|m| m.shown && crate::start::valid_model(&m.id)).map(|m| crate::start::ModelOption { id: id(&m), label: m.name }).collect()
}

/// A model as a session says it: by Cursor's name for it, else by its id, Auto's included.
fn model_name(id: &str, names: &HashMap<String, String>) -> String {
    match names.get(id) {
        Some(name) => name.clone(),
        None if id == "default" => "Auto".to_string(),
        None => id.to_string(),
    }
}

/// The app's auto-run setting, which every chat runs by unless it has its own (`Chat::approval`):
/// the `agent` entry of `composerState.modes4` in its settings, whatever mode the chat is in, and
/// the name each mode has there (3.22.7's code). Run Everything is `autoRun` with `fullAutoRun`,
/// Auto-review `autoRun` with `smartModeAutoRun`, Allowlist `autoRun` alone, and Ask Every Time
/// no `autoRun`. `approvalMode` in the account's `permissions.json` overrides it and a chat's own
/// (`enforced`); a team's settings, a policy, or a project's `.cursor/permissions.json` can too,
/// and none of them is read.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AutoRun {
    /// None when the app's settings say nothing of it.
    pub agent: Option<&'static str>,
    pub names: HashMap<String, String>,
    /// What `permissions.json` sets for every chat: `manual` (as Ask Every Time),
    /// `allowlist` or `unrestricted`.
    pub enforced: Option<&'static str>,
}

/// The account's `permissions.json` `approvalMode`, which the app applies over its auto-run
/// settings (3.22.7's code). None when there's no such file or it sets none.
fn enforced_approval(home: &Path) -> Option<&'static str> {
    let text = std::fs::read_to_string(home.join("permissions.json")).ok()?;
    let v: Value = serde_json::from_str(&text).ok()?;
    match v.get("approvalMode")?.as_str()? {
        "manual" => Some("ask-every-time"),
        "allowlist" => Some("allowlist"),
        "unrestricted" => Some("unrestricted"),
        _ => None,
    }
}

/// The ItemTable key that holds the app's settings for its chats.
const APP_SETTINGS: &str = "src.vs.platform.reactivestorage.browser.reactiveStorageServiceImpl.persistentStorage.applicationUser";

fn read_auto_run(db: &Connection) -> AutoRun {
    let mut out = AutoRun::default();
    let Ok(mut stmt) = db.prepare(
        "SELECT json_extract(m.value, '$.id'), json_extract(m.value, '$.name'), json_extract(m.value, '$.autoRun'), json_extract(m.value, '$.fullAutoRun'), json_extract(m.value, '$.smartModeAutoRun') \
         FROM ItemTable, json_each(CAST(ItemTable.value AS TEXT), '$.composerState.modes4') AS m WHERE ItemTable.key = ?1",
    ) else {
        return out;
    };
    let on = |v: Option<i64>| v.is_some_and(|v| v != 0);
    let rows = stmt.query_map([APP_SETTINGS], |r| Ok((r.get::<_, Option<String>>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, Option<i64>>(2)?, r.get::<_, Option<i64>>(3)?, r.get::<_, Option<i64>>(4)?)));
    for (id, name, auto, full, smart) in rows.into_iter().flatten().flatten() {
        let Some(id) = id else { continue };
        if id == "agent" {
            out.agent = Some(match (on(auto), on(full), on(smart)) {
                (false, _, _) => "ask-every-time",
                (true, true, _) => "unrestricted",
                (true, false, true) => "auto-review",
                (true, false, false) => "allowlist",
            });
        }
        if let Some(name) = name.map(|n| n.trim().to_string()).filter(|n| !n.is_empty()) {
            out.names.insert(id, name);
        }
    }
    out
}

/// A chat's mode, as the app or the command line has it. Ask and Plan change nothing (the
/// command line calls Ask `search`, and once called Run Everything a mode, `auto-run`); in every
/// other mode its auto-run setting says what it does without asking: its own, else the app's.
pub fn chat_mode(c: &Chat, app: &AutoRun) -> Option<SessionMode> {
    let mode = c.mode.as_deref().unwrap_or(if c.cli { "default" } else { "agent" });
    let named = |id: &str, plain: &str| app.names.get(id).cloned().unwrap_or_else(|| plain.to_string());
    match mode {
        "chat" | "search" => return Some(SessionMode::new(ModeKind::ReadOnly, &named("chat", "Ask"), "Cursor's Ask mode: it reads and answers, and changes nothing.")),
        "plan" => return Some(SessionMode::new(ModeKind::Plan, &named("plan", "Plan"), "Cursor's Plan mode: it plans without changing anything, until you build the plan.")),
        "edit" => return Some(SessionMode::new(ModeKind::Other, &named("edit", "Manual"), "Cursor's Manual mode: it edits only the files you give it, with no tools.")),
        _ => {}
    }
    let approval = match (c.cli, c.approval.as_deref()) {
        (false, _) if app.enforced.is_some() => app.enforced,
        (_, Some(a)) => Some(a),
        (true, None) if mode == "auto-run" => Some("unrestricted"),
        (true, None) => Some("allowlist"),
        (false, None) => app.agent,
    }?;
    let (kind, run, detail) = match approval {
        "unrestricted" => (ModeKind::Skips, "Run Everything", "runs every command without asking"),
        "auto-review" => (ModeKind::Auto, "Auto-review", "a reviewer decides which commands run without asking you"),
        "allowlist" => (ModeKind::Edits, "Allowlist", "runs the commands on its allowlist without asking, and asks before the rest"),
        "ask-every-time" => (ModeKind::Asks, "Ask Every Time", "asks before it runs any command"),
        _ => return None,
    };
    // Agent is the usual mode, so its auto-run is what's said; another names itself first.
    let label = match mode {
        "agent" | "default" | "auto-run" | "background" => run.to_string(),
        "debug" => named("debug", "Debug"),
        other => named(other, other),
    };
    let detail = format!("Auto-run is {run}: it {detail}.");
    Some(SessionMode::new(kind, &label, &detail))
}

/// What each chat's composerData is now, asked of every listed chat at each read: how long it
/// is, which says whether to read the rest again without parsing it, and the model and effort its
/// picker set (`model_of`), as a switch between two of one length (`grok-4.6` to `grok-4.7`, `low`
/// to `max`) leaves the length as it was. Only `modelConfig` is pulled out, about 3 ms for all 124
/// chats here (2026-10-14).
#[derive(Debug, Clone, Default, PartialEq)]
struct Written {
    length: i64,
    model: Option<String>,
    effort: Option<String>,
}

fn written(db: &Connection, ids: &[&str]) -> HashMap<String, Written> {
    if ids.is_empty() {
        return HashMap::new();
    }
    let keys: Vec<String> = ids.iter().map(|id| format!("composerData:{id}")).collect();
    let marks = vec!["?"; keys.len()].join(",");
    let Ok(mut stmt) = db.prepare(&format!("SELECT key, length(t), json_extract(t, '$.modelConfig') FROM (SELECT key, CAST(value AS TEXT) AS t FROM cursorDiskKV WHERE key IN ({marks}))")) else { return HashMap::new() };
    let rows = stmt.query_map(rusqlite::params_from_iter(keys.iter()), |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, Option<String>>(2)?)));
    let written = |(key, length, config): (String, i64, Option<String>)| {
        let config: Value = config.as_deref().and_then(|c| serde_json::from_str(c).ok()).unwrap_or(Value::Null);
        let (model, effort) = model_of(&config);
        Some((key.strip_prefix("composerData:")?.to_string(), Written { length, model, effort }))
    };
    rows.map(|rows| rows.flatten().filter_map(written).collect()).unwrap_or_default()
}

/// The ids of a chat's messages, in order, with whether each is yours.
fn message_ids(db: &Connection, chat: &str) -> Vec<(String, bool)> {
    let Ok(mut stmt) = db.prepare(
        "SELECT json_extract(j.value, '$.bubbleId'), json_extract(j.value, '$.type') FROM cursorDiskKV, json_each(CAST(cursorDiskKV.value AS TEXT), '$.fullConversationHeadersOnly') AS j \
         WHERE cursorDiskKV.key = ?1 ORDER BY j.key",
    ) else {
        return Vec::new();
    };
    let rows = stmt.query_map([format!("composerData:{chat}")], |r| Ok((r.get::<_, Option<String>>(0)?, r.get::<_, Option<i64>>(1)?)));
    rows.map(|rows| rows.flatten().filter_map(|(id, kind)| Some((id?, kind == Some(1)))).collect()).unwrap_or_default()
}

/// A chat's messages by their ids, as JSON. One that isn't there is left out.
fn messages(db: &Connection, chat: &str, ids: &[(String, bool)]) -> HashMap<String, Value> {
    if ids.is_empty() {
        return HashMap::new();
    }
    let prefix = format!("bubbleId:{chat}:");
    let keys: Vec<String> = ids.iter().map(|(id, _)| format!("{prefix}{id}")).collect();
    let marks = vec!["?"; keys.len()].join(",");
    let Ok(mut stmt) = db.prepare(&format!("SELECT key, CAST(value AS TEXT) FROM cursorDiskKV WHERE key IN ({marks})")) else { return HashMap::new() };
    let rows = stmt.query_map(rusqlite::params_from_iter(keys.iter()), |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?)));
    rows.map(|rows| rows.flatten().filter_map(|(k, v)| Some((k.strip_prefix(&prefix)?.to_string(), serde_json::from_str(&v?).ok()?))).collect()).unwrap_or_default()
}

/// Those messages in the order asked.
fn in_order(db: &Connection, chat: &str, ids: &[(String, bool)]) -> Vec<Value> {
    let mut found = messages(db, chat, ids);
    ids.iter().filter_map(|(id, _)| found.remove(id)).collect()
}

/// What the end of an app chat's messages says, in its folder, where a command that only looks
/// still counts as work.
fn app_tail(db: &Connection, c: &Chat) -> Tail {
    let ids = message_ids(db, &c.id);
    let first = ids.iter().find(|(_, you)| *you).cloned();
    let latest = &ids[ids.len().saturating_sub(TAIL_MESSAGES)..];
    let mut tail = tail_of(&in_order(db, &c.id, latest).iter().filter_map(bubble).collect::<Vec<_>>(), c.cwd.as_deref());
    // The first prompt is long gone from the end of a long chat.
    if let Some(f) = first.filter(|f| !latest.contains(f)) {
        tail.first_prompt = in_order(db, &c.id, &[f]).iter().filter_map(bubble).find(|b| b.you).map(|b| one_line(&b.text, 140)).filter(|t| !t.is_empty());
    }
    tail
}

/// Reads an app chat's composerData and the end of its messages again, and when it last wrote.
fn refresh(db: &Connection, c: &mut Chat) {
    read_state(db, c);
    c.tail = app_tail(db, c);
    c.last_updated = c.last_updated.max(c.tail.last_at.unwrap_or(0));
}

/// One of a chat's messages: yours, the agent's words, or one of its tool calls, each a message
/// of its own.
#[derive(Debug, Clone, PartialEq)]
struct Bubble {
    you: bool,
    at: i64,
    text: String,
    call: Option<Call>,
    /// The turn ended on an error, in its words.
    error: Option<String>,
}

fn bubble(v: &Value) -> Option<Bubble> {
    let kind = v.get("type").and_then(Value::as_i64)?;
    let at = v.get("createdAt").and_then(|t| t.as_str().and_then(parse_rfc3339_ms).or_else(|| t.as_i64())).unwrap_or(0);
    let text = v.get("text").and_then(Value::as_str).unwrap_or("").trim().to_string();
    let error = v.get("errorDetails").filter(|e| e.is_object()).map(|e| {
        let said = |k: &str| e.get(k).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty());
        match (said("title"), said("message")) {
            (Some(t), Some(m)) => format!("{t}: {m}"),
            (Some(t), None) => t.to_string(),
            (None, Some(m)) => m.to_string(),
            (None, None) => "The turn stopped with an error".to_string(),
        }
    });
    Some(Bubble { you: kind == 1, at, text, call: v.get("toolFormerData").filter(|t| t.is_object()).map(call), error })
}

/// A tool call of the agent's.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Call {
    pub id: String,
    /// The call as a line, e.g. "Run · `cargo test`".
    pub label: String,
    /// All of it: the command, or the arguments.
    pub input: String,
    pub command: Option<String>,
    pub cwd: Option<String>,
    /// Where it works: a command's folder, or the file it writes. A read or a search works
    /// nowhere, as an agent's reads of a repo it isn't working in don't.
    pub work: Option<String>,
    /// Its command only looks (`util::only_looks`), which is work only in the chat's own folder.
    pub looks: bool,
    /// It hasn't finished: it runs, or waits on you.
    pub open: bool,
    /// It waits on you to allow it, or to answer it.
    pub waits: bool,
    pub output: Option<String>,
    pub failed: bool,
}

/// Arguments the app keeps as a string of JSON, or as they are.
fn args_of(t: &Value, key: &str) -> Value {
    match t.get(key) {
        Some(Value::String(s)) => serde_json::from_str(s).unwrap_or(Value::Null),
        Some(v) => v.clone(),
        None => Value::Null,
    }
}

/// A tool's name as a line says it: `run_terminal_command_v2` is "Run", and one it has no word
/// for is its own name in words.
fn tool_word(name: &str) -> String {
    let base = name.strip_suffix("_v2").unwrap_or(name);
    let word = match base {
        "run_terminal_command" => "Run",
        "edit_file" | "search_replace" | "write" => "Edit",
        "read_file" => "Read",
        "delete_file" => "Delete",
        "ripgrep_raw_search" | "grep" | "grep_search" => "Search",
        "glob_file_search" | "file_search" => "Find files",
        "semantic_search_full" | "codebase_search" => "Search the codebase",
        "list_dir" => "List",
        "web_search" => "Search the web",
        "web_fetch" => "Fetch",
        "read_lints" => "Read lints",
        "todo_write" => "Update todos",
        "task" => "Subagent",
        "ask_question" => "Ask",
        "create_plan" => "Plan",
        "switch_mode" => "Switch mode",
        other => {
            let said = other.replace('_', " ");
            let mut chars = said.chars();
            return chars.next().map(|f| f.to_uppercase().collect::<String>() + chars.as_str()).unwrap_or_else(|| "Tool".into());
        }
    };
    word.to_string()
}

/// A call as the app keeps it (`toolFormerData`): its `params` and `result` strings of JSON, its
/// `status`, your `userDecision`, and `additionalData`, which says when it waits on you.
fn call(t: &Value) -> Call {
    let text = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
    let name = t.get("name").and_then(Value::as_str).unwrap_or("tool");
    let base = name.strip_suffix("_v2").unwrap_or(name);
    let raw = args_of(t, "rawArgs");
    let params = match args_of(t, "params") {
        Value::Null => raw.clone(),
        p => p,
    };
    let result = args_of(t, "result");
    let status = t.get("status").and_then(Value::as_str).unwrap_or("");
    let review = t.pointer("/additionalData/reviewData/status").and_then(Value::as_str).unwrap_or("");
    let asking = t.pointer("/additionalData/status").and_then(Value::as_str) == Some("pending");
    let command = (base == "run_terminal_command").then(|| text(&params, "command")).flatten();
    let cwd = text(&params, "cwd").filter(|c| c.starts_with('/'));
    let file = ["targetFile", "relativeWorkspacePath", "path", "targetDirectory"].iter().find_map(|k| text(&params, k)).filter(|p| p.starts_with('/'));
    // An MCP server's tool is `mcp-<server>-<tool>`, and both may have dashes in them.
    let word = match (name.strip_prefix("mcp-"), text(&raw, "providerIdentifier"), text(&raw, "toolName")) {
        (Some(_), Some(server), Some(tool)) => format!("{server} · {tool}"),
        (Some(rest), _, _) if rest.trim_matches('-').is_empty() => "MCP tool".to_string(),
        (Some(rest), _, _) => rest.to_string(),
        (None, _, _) => tool_word(name),
    };
    let label = match (&command, base) {
        (Some(c), _) => format!("{word} · {}", code(&one_line(c, 80))),
        (None, "ask_question" | "create_plan") => match text(&params, "title") {
            Some(title) => format!("{word} · {}", one_line(&title, 80)),
            None => word,
        },
        (None, _) => {
            let what = ["pattern", "globPattern", "query", "searchTerm", "url", "description", "toModeId"].iter().find_map(|k| text(&params, k));
            match (&file, what) {
                (Some(f), _) if matches!(base, "edit_file" | "read_file" | "delete_file" | "search_replace" | "write") => format!("{word} · {}", code(&file_name(Path::new(f)))),
                (_, Some(w)) => format!("{word} · {}", one_line(&w, 80)),
                (Some(f), None) => format!("{word} · {}", code(&file_name(Path::new(f)))),
                (None, None) => word,
            }
        }
    };
    let input = command.clone().unwrap_or_else(|| match &params {
        Value::Null => String::new(),
        p => serde_json::to_string_pretty(p).unwrap_or_default(),
    });
    let output = match base {
        "run_terminal_command" => text(&result, "output"),
        "read_file" => text(&result, "contents"),
        "web_fetch" => text(&result, "markdown"),
        _ => None,
    };
    let rejected = t.get("userDecision").and_then(Value::as_str) == Some("rejected") || result.get("rejected").and_then(Value::as_bool) == Some(true);
    let work = match (&command, &cwd) {
        (Some(c), Some(dir)) => command_dir(c, dir),
        _ => file.clone().filter(|_| matches!(base, "edit_file" | "delete_file" | "search_replace" | "write")),
    };
    let open = matches!(status, "loading" | "pending" | "running");
    Call {
        id: text(t, "toolCallId").unwrap_or_default(),
        label,
        input,
        looks: command.as_deref().is_some_and(only_looks),
        command,
        cwd,
        work,
        open,
        waits: open && (review == "Requested" || asking),
        output,
        failed: matches!(status, "error" | "cancelled") || rejected,
    }
}

/// What a run of messages, oldest first, says of where the chat stands, in its folder `own`.
fn tail_of(bubbles: &[Bubble], own: Option<&str>) -> Tail {
    let mut out = Tail::default();
    for b in bubbles {
        out.last_at = out.last_at.max(Some(b.at).filter(|t| *t > 0));
        if b.you {
            out.last_prompt_at = Some(b.at).filter(|t| *t > 0).or(out.last_prompt_at);
            if out.first_prompt.is_none() {
                out.first_prompt = Some(one_line(&b.text, 140)).filter(|t| !t.is_empty());
            }
            // Calls left before a prompt of yours aren't waiting any more.
            out.open.clear();
            out.error = None;
            continue;
        }
        if let Some(c) = &b.call {
            out.last_tool = Some(c.label.clone());
            // The agent picks each command's folder: one run somewhere else only to look isn't
            // work there.
            if let Some(w) = c.work.as_ref().filter(|w| !c.looks || own.is_some_and(|own| Path::new(w).starts_with(own))) {
                note_work(&mut out.work, w.clone());
            }
            if c.open {
                out.open.push(c.clone());
            }
        } else if !b.text.is_empty() {
            out.last_text = Some(one_line(&b.text, 140)).filter(|t| !t.is_empty());
            let chars: Vec<char> = b.text.chars().collect();
            out.last_text_tail = Some(chars[chars.len().saturating_sub(600)..].iter().collect());
        }
        out.error = b.error.clone();
    }
    out
}

/// A message's items (transcript.rs): your prompt, the agent's words, or one of its calls with
/// what it gave back, which the app keeps on the call itself.
fn bubble_items(v: &Value) -> Vec<Item> {
    let Some(b) = bubble(v) else { return Vec::new() };
    let mut out = Vec::new();
    if b.you {
        if !b.text.is_empty() {
            out.push(item(b.at, "you", b.text));
        }
        return out;
    }
    if let Some(c) = &b.call {
        let shell = c.command.as_deref().is_some_and(|cmd| !cmd.trim().is_empty() && kept_whole(cmd));
        out.push(Item {
            id: Some(c.id.clone()).filter(|id| !id.is_empty()),
            input: Some(clip_input(&c.input)).filter(|i| !i.is_empty()),
            output: c.output.as_deref().map(clip_output),
            failed: c.failed,
            shell,
            cwd: c.cwd.clone().filter(|_| shell),
            ..item(b.at, "tool", c.label.clone())
        });
    } else if !b.text.is_empty() {
        out.push(item(b.at, "agent", b.text.clone()));
    }
    if let Some(e) = &b.error {
        out.push(item(b.at, "note", e.clone()));
    }
    out
}

/// A page of an app chat's conversation, back from `before` (from the newest when none). A
/// message's place is one more than its index among the chat's messages, and `before` and
/// `after` count messages; a chat still generating has its last message read again by the next
/// read on, as it may still be written.
fn app_page(db: &Connection, chat: &str, generating: bool, before: Option<u64>, limit: usize) -> Page {
    let ids = message_ids(db, chat);
    let mut lo = before.map_or(ids.len(), |b| (b as usize).min(ids.len()));
    let mut newest_first: Vec<Vec<Item>> = Vec::new();
    let mut count = 0;
    // Fifty messages at a time, newest first, until there are enough items.
    while lo > 0 && count < limit {
        let from = lo.saturating_sub(50);
        let found = messages(db, chat, &ids[from..lo]);
        for i in (from..lo).rev() {
            let its = found.get(&ids[i].0).map(|v| placed(bubble_items(v), (i + 1) as u64)).unwrap_or_default();
            count += its.len();
            newest_first.push(its);
            lo = i;
            if count >= limit {
                break;
            }
        }
    }
    let items = newest_first.into_iter().rev().flatten().collect();
    let after = if generating { ids.len().saturating_sub(1) } else { ids.len() } as u64;
    Page { items, before: (lo > 0).then_some(lo as u64), after }
}

/// What an app chat said and did after `after`.
fn app_newer(db: &Connection, chat: &str, generating: bool, after: u64) -> Page {
    let ids = message_ids(db, chat);
    let from = (after as usize).min(ids.len());
    let found = messages(db, chat, &ids[from..]);
    let items = (from..ids.len()).flat_map(|i| found.get(&ids[i].0).map(|v| placed(bubble_items(v), (i + 1) as u64)).unwrap_or_default()).collect();
    let last = if generating { ids.len().saturating_sub(1) } else { ids.len() };
    Page { items, before: None, after: (last as u64).max(after) }
}

/* ---- The command line's chats ---- */

/// What a chat's `store.db` says of it: its `meta` row's value is JSON written as hex.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Meta {
    pub name: Option<String>,
    pub created_at: Option<i64>,
    pub model: Option<String>,
    pub mode: Option<String>,
    /// `approvalMode`, else `unrestricted` where `isRunEverything` says so, as the command line
    /// reads them (2026.08.11's code).
    pub approval: Option<String>,
    /// It's a subagent's chat (`subagentInfo`, naming the chat that started it), which the
    /// command line's own list of chats leaves out. 2026.10.16 writes no `meta.json` for one.
    pub subagent: bool,
}

fn decode_hex(s: &str) -> Option<Vec<u8>> {
    let s = s.trim();
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok()).collect()
}

/// A `meta` value: hex for JSON, or the JSON itself.
pub fn parse_meta(value: &str) -> Meta {
    let json = decode_hex(value).and_then(|b| String::from_utf8(b).ok()).unwrap_or_else(|| value.to_string());
    let v: Value = serde_json::from_str(&json).unwrap_or(Value::Null);
    let text = |k: &str| v.get(k).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
    let approval = text("approvalMode").or_else(|| (v.get("isRunEverything").and_then(Value::as_bool) == Some(true)).then(|| "unrestricted".to_string()));
    let subagent = v.get("subagentInfo").is_some_and(|i| !i.is_null());
    Meta { name: text("name"), created_at: v.get("createdAt").and_then(Value::as_i64), model: text("lastUsedModel"), mode: text("mode"), approval, subagent }
}

fn read_meta(store: &Path) -> Meta {
    let Some(db) = crate::harness::open_database(store) else { return Meta::default() };
    db.query_row("SELECT value FROM meta WHERE key = '0'", [], |r| r.get::<_, String>(0)).map(|v| parse_meta(&v)).unwrap_or_default()
}

/// What the command line writes beside a chat's database as it makes it, `meta.json`
/// (`{schemaVersion, createdAtMs, updatedAtMs, hasConversation, title, cwd, isSubagent}`,
/// 2026.08.11 on): the folder it works in, which the database doesn't say, and whether it's a
/// subagent's, which its own list of chats leaves out.
#[derive(Debug, Default, PartialEq)]
pub struct Sidecar {
    pub cwd: Option<String>,
    pub subagent: bool,
}

pub fn parse_sidecar(text: &str) -> Sidecar {
    let v: Value = serde_json::from_str(text).unwrap_or(Value::Null);
    Sidecar { cwd: v.get("cwd").and_then(Value::as_str).filter(|c| !c.is_empty()).map(str::to_string), subagent: v.get("isSubagent").and_then(Value::as_bool) == Some(true) }
}

/// A folder's path as Cursor names the folder of its records for it under `projects`: each run
/// of characters that aren't letters or digits one `-`.
pub fn slug(path: &str) -> String {
    let mut out = String::new();
    for c in path.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_string()
}

/// The folder whose slug `name` is, found by walking down from `root` through the folders whose
/// own slugs lead it. Many paths make one slug; the first folder that's there wins.
pub fn unslug(root: &Path, name: &str) -> Option<PathBuf> {
    fn walk(dir: &Path, rest: &str, depth: usize) -> Option<PathBuf> {
        if depth > 24 {
            return None;
        }
        let entries = std::fs::read_dir(dir).ok()?;
        let mut kids: Vec<(String, PathBuf)> = entries.flatten().filter(|e| e.file_type().is_ok_and(|t| t.is_dir())).map(|e| (slug(&e.file_name().to_string_lossy()), e.path())).filter(|(s, _)| !s.is_empty()).collect();
        // Longer names first, so `app-two` is tried before `app`.
        kids.sort_by(|a, b| b.0.len().cmp(&a.0.len()));
        for (s, path) in kids {
            if rest == s {
                return Some(path);
            }
            if let Some(found) = rest.strip_prefix(s.as_str()).and_then(|r| r.strip_prefix('-')).and_then(|more| walk(&path, more, depth + 1)) {
                return Some(found);
            }
        }
        None
    }
    walk(root, name, 0)
}

/// Where a chat's transcript is, under the account's `projects`, whichever folder's it is.
fn find_transcript(home: &Path, id: &str) -> Option<PathBuf> {
    let projects = std::fs::read_dir(home.join("projects")).ok()?;
    projects.flatten().map(|p| p.path().join("agent-transcripts").join(id).join(format!("{id}.jsonl"))).find(|p| p.is_file())
}

/// A command line chat's transcript: under the folder its copy's `meta.json` says it works in,
/// by Cursor's name for it (`slug`, its `workspace-paths.js`), and none until that's written,
/// since another copy's (`cli_chat_dir`) is of another conversation. One without a folder (from
/// before 2026.08.11) has whichever there is.
fn cli_transcript(home: &Path, id: &str, side: &Sidecar) -> Option<PathBuf> {
    match side.cwd.as_deref() {
        Some(cwd) => Some(home.join("projects").join(slug(cwd)).join("agent-transcripts").join(id).join(format!("{id}.jsonl"))).filter(|p| p.is_file()),
        None => find_transcript(home, id),
    }
}

fn read_sidecar(dir: &Path) -> Sidecar {
    std::fs::read_to_string(dir.join("meta.json")).map(|t| parse_sidecar(&t)).unwrap_or_default()
}

/// When a command line chat's database was last written, its write-ahead log included; None
/// while it has none.
fn store_written(dir: &Path) -> Option<i64> {
    let store = dir.join(STORE_DB);
    let wal = PathBuf::from(format!("{}-wal", store.display()));
    store.is_file().then(|| [&store, &wal].into_iter().filter_map(|p| crate::util::mtime_ms(p)).max()).flatten()
}

/// The command line's chats under `home`, written to since `since_ms` or in `keep`, newest
/// first, but not subagents': each in the folder its `meta.json` says, which an older one has
/// none of (`cli_cwd`). One a chat, however many folders have a copy (`cli_chat_dir`).
pub fn read_cli_chats(home: &Path, since_ms: i64, keep: &HashSet<String>) -> Vec<Chat> {
    let Ok(folders) = std::fs::read_dir(home.join("chats")) else { return Vec::new() };
    let mut latest: HashMap<String, (i64, PathBuf)> = HashMap::new();
    for dir in folders.flatten().filter_map(|f| std::fs::read_dir(f.path()).ok()).flatten().flatten().map(|c| c.path()) {
        let Some(at) = store_written(&dir) else { continue };
        let id = file_name(&dir);
        // The copy `cli_chat_dir` takes: `max` on the pair.
        if latest.get(&id).map_or(true, |was| *was < (at, dir.clone())) {
            latest.insert(id, (at, dir));
        }
    }
    let mut out = Vec::new();
    for (id, (_, dir)) in latest {
        let store = dir.join(STORE_DB);
        let side = read_sidecar(&dir);
        let transcript = cli_transcript(home, &id, &side);
        let files = [Some(store.clone()), Some(PathBuf::from(format!("{}-wal", store.display()))), transcript.clone()];
        let written = files.into_iter().flatten().filter_map(|p| crate::util::mtime_ms(&p)).max().unwrap_or(0);
        if written < since_ms && !keep.contains(&id) {
            continue;
        }
        if side.subagent {
            continue;
        }
        let meta = read_meta(&store);
        if meta.subagent {
            continue;
        }
        let transcript_at = transcript.as_deref().and_then(crate::util::mtime_ms);
        out.push(Chat { id, title: meta.name, cwd: side.cwd, created_at: meta.created_at.unwrap_or(written), last_updated: written, model: meta.model, mode: meta.mode, approval: meta.approval, cli: true, dir: Some(dir), transcript, transcript_at, ..Chat::default() });
    }
    out.sort_by(|a, b| b.last_updated.cmp(&a.last_updated));
    out
}

/// A command line chat's folder under `home`, `chats/<hash>/<id>`, once its database is there.
/// The command line keeps a folder's chats under an MD5 of its path and takes `--resume <id>` in
/// another folder for a new chat there under the same id, with nothing of what was said before:
/// a review resumed after its worktree had gone left two (2026-10-27). Of those, the one written
/// last is the chat that goes on, and the others are no longer read.
fn cli_chat_dir(home: &Path, id: &str) -> Option<PathBuf> {
    let chats = std::fs::read_dir(home.join("chats")).ok()?;
    chats.flatten().map(|f| f.path().join(id)).filter_map(|d| Some((store_written(&d)?, d))).max().map(|(_, d)| d)
}

/// The folder a command line chat works in, from where its transcript is kept: its database's
/// folder is named for a hash of it, which can't be read back.
fn cli_cwd(c: &Chat) -> Option<String> {
    let project = c.transcript.as_deref()?.parent()?.parent()?.parent()?;
    unslug(Path::new("/"), &file_name(project)).map(|p| p.to_string_lossy().into_owned())
}

/// What you asked, and when, out of a prompt as a transcript writes it: the time as a sentence
/// in `<timestamp>`, the words in `<user_query>`.
fn prompt_of(text: &str) -> (String, Option<i64>) {
    let inner = |tag: &str| text.split_once(&format!("<{tag}>")).and_then(|(_, rest)| rest.split_once(&format!("</{tag}>"))).map(|(x, _)| x.trim());
    let asked = inner("user_query").unwrap_or(text).trim().to_string();
    (asked, inner("timestamp").and_then(prompt_time))
}

/// "Wednesday, Sep 2, 2026, 4:21 PM (UTC-7)" as epoch milliseconds.
fn prompt_time(s: &str) -> Option<i64> {
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let (when, zone) = s.split_once('(').map(|(w, z)| (w.trim(), z.trim_end_matches(')').trim())).unwrap_or((s.trim(), "UTC"));
    let parts: Vec<&str> = when.split(',').map(str::trim).collect();
    // [weekday,] "Sep 2", "2026", "4:21 PM"
    let parts = if parts.len() == 4 { &parts[1..] } else { &parts[..] };
    let [md, year, time] = parts else { return None };
    let (month, day) = md.split_once(' ')?;
    let month = MONTHS.iter().position(|m| month.starts_with(m))? + 1;
    let (clock, half) = time.split_once(' ').unwrap_or((time, ""));
    let (h, m) = clock.split_once(':')?;
    let mut hour: i64 = h.parse().ok()?;
    match half {
        "PM" if hour < 12 => hour += 12,
        "AM" if hour == 12 => hour = 0,
        _ => {}
    }
    let offset = zone.strip_prefix("UTC").or_else(|| zone.strip_prefix("GMT")).unwrap_or("");
    let sign = if offset.starts_with('-') { '-' } else { '+' };
    let unsigned = offset.trim_start_matches(['+', '-']);
    let offset = match unsigned.split_once(':') {
        _ if unsigned.is_empty() => "Z".to_string(),
        Some((h, m)) => format!("{sign}{:02}:{:02}", h.parse::<i64>().ok()?, m.parse::<i64>().ok()?),
        None => format!("{sign}{:02}:00", unsigned.parse::<i64>().ok()?),
    };
    parse_rfc3339_ms(&format!("{year}-{month:02}-{:02}T{hour:02}:{:02}:00{offset}", day.parse::<i64>().ok()?, m.parse::<i64>().ok()?))
}

/// A transcript line: yours, the agent's words and calls, or the end of a turn.
enum Line {
    You { text: String, at: Option<i64> },
    Agent { text: String, calls: Vec<Call> },
    /// `error` says why, when the turn ended on one.
    TurnEnded { error: Option<String> },
}

fn line(v: &Value) -> Option<Line> {
    if v.get("type").and_then(Value::as_str) == Some("turn_ended") {
        return Some(Line::TurnEnded { error: (v.get("status").and_then(Value::as_str) == Some("error")).then(|| turn_error(v)) });
    }
    let parts = v.pointer("/message/content").and_then(Value::as_array)?;
    let text: Vec<&str> = parts.iter().filter(|p| p.get("type").and_then(Value::as_str) == Some("text")).filter_map(|p| p.get("text").and_then(Value::as_str)).collect();
    let text = text.join("\n").trim().to_string();
    match v.get("role").and_then(Value::as_str)? {
        "user" => {
            let (asked, at) = prompt_of(&text);
            Some(Line::You { text: asked, at })
        }
        "assistant" => {
            let uses = parts.iter().filter(|p| p.get("type").and_then(Value::as_str) == Some("tool_use"));
            let calls = uses
                .map(|p| {
                    // The transcript names its tools as Claude Code does.
                    let name = match p.get("name").and_then(Value::as_str).unwrap_or("tool") {
                        "Shell" | "Bash" => "run_terminal_command",
                        "Read" => "read_file",
                        "Write" | "StrReplace" | "Edit" => "edit_file",
                        "Delete" => "delete_file",
                        "Grep" => "grep",
                        "Glob" => "glob_file_search",
                        "SemanticSearch" => "codebase_search",
                        "WebSearch" => "web_search",
                        "WebFetch" => "web_fetch",
                        "TodoWrite" => "todo_write",
                        "Task" => "task",
                        other => other,
                    };
                    call(&serde_json::json!({ "name": name, "params": p.get("input").cloned().unwrap_or(Value::Null), "status": "completed" }))
                })
                .collect();
            Some(Line::Agent { text, calls })
        }
        _ => None,
    }
}

/// Why a turn stopped, in the words of its `turn_ended` line's `error`: "[resource_exhausted]
/// Error" when Cursor's servers turned a request away (2026-10-22), while the plan had room.
fn turn_error(v: &Value) -> String {
    match v.get("error").and_then(Value::as_str).map(str::trim).filter(|e| !e.is_empty()) {
        Some(why) => format!("The turn stopped with an error: {why}"),
        None => "The turn stopped with an error".to_string(),
    }
}

/// A transcript line's items (transcript.rs). The lines carry no times of their own, only your
/// prompts say when.
fn transcript_items(v: &Value, at: i64) -> Vec<Item> {
    match line(v) {
        Some(Line::You { text, at: asked }) if !text.is_empty() => vec![item(asked.unwrap_or(at), "you", text)],
        Some(Line::Agent { text, calls }) => {
            let mut out = Vec::new();
            if !text.is_empty() {
                out.push(item(at, "agent", text));
            }
            for c in calls {
                let shell = c.command.as_deref().is_some_and(|cmd| !cmd.trim().is_empty() && kept_whole(cmd));
                out.push(Item { input: Some(clip_input(&c.input)).filter(|i| !i.is_empty()), shell, cwd: c.cwd.clone().filter(|_| shell), ..item(at, "tool", c.label) });
            }
            out
        }
        Some(Line::TurnEnded { error: Some(why) }) => vec![item(at, "note", why)],
        _ => Vec::new(),
    }
}

/// When a command line chat's items (`transcript_items`) happened, as near as its record says: it
/// keeps a time only for each prompt, the minute it was written in (`prompt_time`). What came
/// after a prompt takes that prompt's time, so a reply from a minute that also had a later prompt
/// isn't taken for that prompt's (`session wait`), and one dated so is known by it as dated by its
/// prompt (`autoreview::reply_after`). What came before the page's first prompt takes that
/// prompt's time; with no prompt in the page, the session's last prompt's where the page ends the
/// record (`to_end`), as what came after that prompt, else the session's start. In a page that
/// ends the record, its last item takes the time the record was last written (`written`) where
/// that's later: a turn can end on an error hours after anything else in it was written (a review
/// whose turn gave up after five hours of resuming, 2026-10-28).
fn timed(mut page: Page, s: &Session, to_end: bool, written: Option<i64>) -> Page {
    let mut known = match page.items.iter().map(|i| i.at).find(|t| *t > 0) {
        Some(t) => t,
        None if to_end => s.last_prompt_at,
        None => s.started_at.unwrap_or(s.last_prompt_at),
    };
    let mut last_untimed = false;
    for it in page.items.iter_mut() {
        last_untimed = it.at <= 0;
        if last_untimed {
            it.at = known;
        } else {
            known = it.at;
        }
    }
    if let (true, true, Some(at), Some(last)) = (to_end, last_untimed, written, page.items.last_mut()) {
        last.at = last.at.max(at);
    }
    page
}

/// What the end of a transcript says, in the chat's folder `own`.
fn transcript_tail(path: &Path, own: Option<&str>) -> Tail {
    let Ok(end) = crate::util::read_last(path, TAIL_BYTES) else { return Tail::default() };
    let mut bubbles = Vec::new();
    let mut ended = true;
    let mut errored = None;
    for v in end.text().lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()) {
        match line(&v) {
            Some(Line::You { text, at }) => {
                bubbles.push(Bubble { you: true, at: at.unwrap_or(0), text, call: None, error: None });
                ended = false;
                errored = None;
            }
            Some(Line::Agent { text, calls }) => {
                if !text.is_empty() {
                    bubbles.push(Bubble { you: false, at: 0, text, call: None, error: None });
                }
                bubbles.extend(calls.into_iter().map(|c| Bubble { you: false, at: 0, text: String::new(), call: Some(c), error: None }));
            }
            Some(Line::TurnEnded { error }) => {
                ended = true;
                errored = error;
            }
            None => {}
        }
    }
    let mut tail = tail_of(&bubbles, own);
    tail.turn_open = !ended;
    tail.error = errored;
    if tail.first_prompt.is_none() {
        tail.first_prompt = crate::util::read_head(path, TAIL_BYTES as usize).and_then(|head| {
            head.lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()).find_map(|v| match line(&v) {
                Some(Line::You { text, .. }) => Some(one_line(&text, 140)).filter(|t| !t.is_empty()),
                _ => None,
            })
        });
    }
    tail
}

/* ---- The processes ---- */

/// A `cursor-agent` process on this Mac.
#[derive(Debug, Clone, PartialEq)]
pub struct Process {
    pub pid: u32,
    /// The chat it has open: from its arguments, else the chat database it holds open.
    pub chat: Option<String>,
    /// It's the interactive UI in a terminal, rather than a print run.
    pub terminal: bool,
    /// The prompt it was started with, as `ps` joins its words.
    pub prompt: Option<String>,
}

/// What a `cursor-agent` process's arguments say (`cli_args`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Args {
    /// It's the interactive UI rather than a print run.
    pub interactive: bool,
    /// The chat it names.
    pub chat: Option<String>,
    /// The prompt, from its first word on.
    pub prompt: Option<String>,
}

/// What a process's command line says when it's `cursor-agent` working on a chat (`Args`). None
/// for its other commands (`login`, `models`…) and for anything else. Its launcher runs its
/// bundled node as the name it was run by (`exec -a`), so `ps` shows that name, node's options,
/// the bundle's `index.js` and the arguments, joined with spaces: a prompt reads as words, and
/// only the words before it are options.
pub fn cli_args(command: &str) -> Option<Args> {
    let mut words = command.split_whitespace().peekable();
    let program = words.next()?;
    if !matches!(program.rsplit('/').next(), Some("cursor-agent" | "agent")) {
        return None;
    }
    words.by_ref().find(|w| w.ends_with("/index.js") && w.contains("/cursor-agent/"))?;
    let mut interactive = true;
    let mut chat = None;
    let mut prompt = None;
    while let Some(w) = words.next() {
        let (flag, value) = match w.split_once('=') {
            Some((f, v)) if f.starts_with("--") => (f, Some(v.to_string())),
            _ => (w, None),
        };
        match flag {
            "-p" | "--print" => interactive = false,
            // `--resume` alone picks a chat from a list.
            "--resume" | "--new-session-id" => {
                let id = match value {
                    Some(v) => Some(v),
                    None => words.next_if(|n| !n.starts_with('-')).map(str::to_string),
                };
                chat = id.filter(|id| id.len() >= 8 && id.chars().all(|c| c.is_ascii_hexdigit() || c == '-')).or(chat);
            }
            "--api-key" | "-H" | "--header" | "-e" | "--endpoint" | "--output-format" | "--mode" | "--model" | "--sandbox" if value.is_none() => {
                words.next();
            }
            // A folder with a space in its path (`Application Support`) goes on in the words
            // after it that carry on a path.
            "--workspace" | "--add-dir" | "--plugin-dir" | "--worktree-base" if value.is_none() => {
                words.next();
                while words.next_if(|n| n.contains('/') && !n.starts_with(['/', '-'])).is_some() {}
            }
            // Its name is optional, and takes the word after it: a start puts it after the prompt.
            "-w" | "--worktree" if value.is_none() => {
                words.next_if(|n| !n.starts_with('-'));
            }
            _ if flag.starts_with('-') => {}
            "acp" | "install-shell-integration" | "uninstall-shell-integration" | "login" | "logout" | "mcp" | "plugin" | "worker" | "status" | "whoami" | "models" | "bedrock" | "about" | "update" | "create-chat" | "generate-rule" | "rule" | "ls" | "help" => return None,
            // `resume` goes on with the folder's latest chat.
            "resume" | "agent" => {}
            // A prompt.
            _ => {
                let mut said: Vec<&str> = std::iter::once(w).chain(words.by_ref()).collect();
                if matches!(said.last(), Some(&"--worktree" | &"-w")) {
                    said.pop();
                }
                prompt = Some(said.join(" "));
                break;
            }
        }
    }
    Some(Args { interactive, chat, prompt })
}

/// The folder a command line process keeps its chats in, from the environment it started with:
/// `CURSOR_CONFIG_DIR`, else `$XDG_CONFIG_HOME/cursor`, else `~/.cursor` (2026.08.11's code).
/// Its transcripts and the folders it trusts go by another, `CURSOR_DATA_DIR`, else `~/.cursor`.
fn config_dir_of(pid: u32) -> PathBuf {
    let env = procs::environment(pid).unwrap_or_default();
    let set = |k: &str| env.get(k).map(|v| v.trim()).filter(|v| !v.is_empty()).map(PathBuf::from);
    set("CURSOR_CONFIG_DIR").or_else(|| set("XDG_CONFIG_HOME").map(|x| x.join("cursor"))).unwrap_or_else(|| crate::util::home().join(".cursor"))
}

/// Whether a process's command line is the Cursor app's own.
fn is_app(command: &str) -> bool {
    let program = command.split(" --").next().unwrap_or(command).trim_end();
    program.ends_with("/Contents/MacOS/Cursor") && program.contains("Cursor.app")
}

/// The chat a command line process holds open: its `chats/<hash>/<id>/store.db` among its open
/// files.
fn held_chat(open_files: &[PathBuf]) -> Option<String> {
    open_files.iter().find_map(|p| {
        let p = p.to_str()?;
        let p = Path::new(p.strip_suffix("-wal").unwrap_or(p));
        let dir = p.parent()?;
        (file_name(p) == STORE_DB && dir.parent()?.parent().map(file_name).as_deref() == Some("chats")).then(|| file_name(dir))
    })
}

/// The command line's processes, each with the chat it has open, and whether the app is
/// running. From the list of every process, and the open files of a `cursor-agent` whose
/// arguments don't name its chat, as the kernel lists them (no `lsof`, which takes half a second
/// at a high load, on the engine's thread).
pub fn processes(listed: &[procs::PsLine]) -> (Vec<Process>, bool) {
    let mut app = false;
    let mut found = Vec::new();
    for p in listed {
        app |= is_app(&p.command);
        if let Some(args) = cli_args(&p.command) {
            let chat = args.chat.or_else(|| procs::open_files(p.pid).and_then(|files| held_chat(&files)));
            found.push(Process { pid: p.pid, chat, terminal: args.interactive && !p.tty.starts_with('?'), prompt: args.prompt });
        }
    }
    (found, app)
}

/* ---- Reading chats for the board ---- */

/// Cursor's chats for the board (`harness::Reader`): each account's chats as last read, newest
/// first, the command line's processes, and whether the app is running.
#[derive(Default)]
pub struct Reader {
    chats: HashMap<String, Vec<Chat>>,
    marks: crate::harness::DatabaseMarks,
    processes: Vec<Process>,
    app_running: bool,
    /// The app's auto-run setting, by account, as last read.
    auto_run: HashMap<String, AutoRun>,
    /// Cursor's names for its models (`model_names`), and when they were read: again only for
    /// a model they lack, and not more than once a minute.
    names: HashMap<String, String>,
    names_at: Option<Instant>,
    looked_at: Option<Instant>,
    read_at: Option<Instant>,
}

impl Reader {
    /// Reads an account's chats again: the app's list, the composerData of each that was written
    /// since and the end of its messages, and the command line's chats, with the end of each that
    /// changed or that a process has open. True when a chat is there that wasn't.
    fn reread(&mut self, a: &AccountCfg, look: &Look) -> bool {
        self.read_at = Some(Instant::now());
        let home = a.home_path();
        // In the processes the engine last listed (`Look::ps`: as of its last tick).
        if self.looked_at.map_or(true, |t| t.elapsed() >= Duration::from_secs(2)) {
            self.look_at_processes(look);
        }
        let keep = look.seen.pinned_ids("cursor", &a.name);
        let before: HashMap<String, Chat> = self.chats.remove(&a.name).unwrap_or_default().into_iter().map(|c| (c.id.clone(), c)).collect();
        let mut rows = Vec::new();
        if let Some(db) = app_db(&home).and_then(|p| crate::harness::open_database(&p)) {
            self.auto_run.insert(a.name.clone(), AutoRun { enforced: enforced_approval(&home), ..read_auto_run(&db) });
            let mut app = read_app_chats(&db, look.since, &keep).unwrap_or_default();
            let now = written(&db, &app.iter().map(|c| c.id.as_str()).collect::<Vec<_>>());
            for c in &mut app {
                let w = now.get(&c.id).cloned().unwrap_or_default();
                match before.get(&c.id) {
                    // Nothing written since: what was read stands, as the header may lag.
                    Some(old) if old.written == w.length && w.length > 0 => {
                        // A mode the header changed to since is one composerData's length hid.
                        let mode = if c.header_mode != old.header_mode { c.header_mode.clone().or(old.mode.clone()) } else { old.mode.clone() };
                        *c = Chat { archived: c.archived, header_mode: c.header_mode.clone(), mode, ..old.clone() };
                    }
                    _ => refresh(&db, c),
                }
                (c.model, c.effort) = (w.model, w.effort);
            }
            app.retain(|c| c.messages > 0 || c.generating());
            let unnamed = app.iter().filter_map(|c| c.model.as_deref()).any(|m| !self.names.contains_key(m));
            if self.names_at.map_or(true, |t| unnamed && t.elapsed() >= Duration::from_secs(60)) {
                self.names = model_names(&db);
                self.names_at = Some(Instant::now());
            }
            rows.extend(app);
        }
        for mut c in read_cli_chats(&home, look.since, &keep) {
            let process = self.held(&c.id);
            // Walking the disk for its folder is done once a chat.
            let cwd = c.cwd.clone().or_else(|| before.get(&c.id).and_then(|old| old.cwd.clone())).or_else(|| cli_cwd(&c));
            c.cwd = cwd.or_else(|| process.and_then(|p| procs::cwd_of(p.pid)).map(|d| d.to_string_lossy().into_owned()));
            match before.get(&c.id) {
                Some(old) if process.is_none() && old.last_updated == c.last_updated => c.tail = old.tail.clone(),
                _ => c.tail = c.transcript.as_deref().map(|t| transcript_tail(t, c.cwd.as_deref())).unwrap_or_default(),
            }
            rows.push(c);
        }
        // A process of the account's whose chat isn't written yet: the board knows it from the
        // start, and shows it waiting on what the command line asks first.
        let unwritten: Vec<Chat> = self
            .processes
            .iter()
            .filter(|p| p.chat.as_deref().is_some_and(|id| !rows.iter().any(|c| c.id == id) && cli_chat_dir(&home, id).is_none()) && config_dir_of(p.pid) == home)
            .map(|p| {
                let started = procs::proc_start_ms(p.pid).unwrap_or(look.now);
                let first_prompt = p.prompt.as_deref().map(|t| one_line(t, 140)).filter(|t| !t.is_empty());
                // Where it is now: it goes into a folder `--workspace` or `--worktree` names as it starts.
                let cwd = procs::cwd_of(p.pid).map(|d| d.to_string_lossy().into_owned()).or_else(|| before.get(p.chat.as_deref().unwrap_or_default()).and_then(|old| old.cwd.clone()));
                Chat { id: p.chat.clone().unwrap_or_default(), cwd, created_at: started, last_updated: started, cli: true, unwritten: true, tail: Tail { first_prompt, ..Tail::default() }, ..Chat::default() }
            })
            .collect();
        rows.extend(unwritten);
        rows.sort_by(|x, y| y.last_updated.cmp(&x.last_updated));
        let added = rows.iter().any(|c| placed_anew(before.get(&c.id), c));
        self.chats.insert(a.name.clone(), rows);
        added
    }

    /// Looks at the processes again. True when they changed.
    fn look_at_processes(&mut self, look: &Look) -> bool {
        self.looked_at = Some(Instant::now());
        let (processes, app_running) = processes(look.ps.all());
        let changed = processes != self.processes || app_running != self.app_running;
        self.processes = processes;
        self.app_running = app_running;
        changed
    }

    fn held(&self, id: &str) -> Option<&Process> {
        self.processes.iter().find(|p| p.chat.as_deref() == Some(id))
    }
}

/// Whether a chat as read now may need a lane made for it (`Applied::added`): it's new, or it's
/// in a folder it wasn't in when last read, as one known only from its process is once its own
/// record says where it works.
fn placed_anew(before: Option<&Chat>, now: &Chat) -> bool {
    before.map_or(true, |old| now.cwd.is_some() && old.cwd != now.cwd)
}

impl crate::harness::Reader for Reader {
    fn harness(&self) -> Harness {
        Harness::Cursor
    }

    fn scan(&mut self, accounts: &[AccountCfg], look: &Look) {
        self.chats.clear();
        if !accounts.is_empty() {
            self.look_at_processes(look);
        }
        for a in accounts {
            self.reread(a, look);
        }
    }

    /// The command line's chats, the transcripts both write, and the app's database's folder.
    fn watch(&self, account: &AccountCfg) -> Vec<(PathBuf, bool)> {
        let home = account.home_path();
        let mut out = vec![(home.join("chats"), true), (home.join("projects"), true)];
        out.extend(app_db(&home).and_then(|p| Some((p.parent()?.to_path_buf(), false))));
        out
    }

    fn owns(&self, account: &AccountCfg, path: &Path) -> bool {
        let home = account.home_path();
        path.starts_with(home.join("chats")) || path.starts_with(home.join("projects")) || app_db(&home).and_then(|p| Some(path.starts_with(p.parent()?))).unwrap_or(false)
    }

    fn apply(&mut self, account: &AccountCfg, paths: &[PathBuf], look: &Look) -> Applied {
        let counts = |p: &PathBuf| {
            let name = file_name(p);
            name.starts_with(STATE_DB) || name.starts_with(STORE_DB) || (name.ends_with(".jsonl") && p.components().any(|c| c.as_os_str() == "agent-transcripts"))
        };
        if !paths.iter().any(counts) {
            return Applied::default();
        }
        Applied { dirty: true, added: self.reread(account, look), ..Applied::default() }
    }

    /// A process that came may have a chat that wasn't listed, and one that went leaves its chat
    /// as it was; and a chat the app was generating when it quit says so until the app opens it
    /// again, which only the app's processes tell.
    fn processes(&mut self, accounts: &[AccountCfg], look: &Look) -> Applied {
        let mut out = Applied::default();
        if !accounts.is_empty() && self.look_at_processes(look) {
            out.dirty = true;
            for a in accounts {
                out.added |= self.reread(a, look);
            }
        }
        out
    }

    /// The app's database, which it writes all the time while it's open (its windows' state
    /// too) through a file it holds open, and the databases of the command line's chats a
    /// process has open. Read again at most every two seconds.
    fn glance(&mut self, accounts: &[AccountCfg], look: &Look) -> Applied {
        let mut out = Applied::default();
        for a in accounts {
            let home = a.home_path();
            let mut changed = app_db(&home).is_some_and(|db| self.marks.changed(&format!("{}|app", a.name), &db));
            let held: Vec<PathBuf> = self.chats.get(&a.name).into_iter().flatten().filter(|c| self.held(&c.id).is_some()).filter_map(|c| Some(c.dir.as_ref()?.join(STORE_DB))).collect();
            for db in held {
                changed |= self.marks.changed(&db.to_string_lossy(), &db);
            }
            if changed && self.read_at.map_or(true, |t| t.elapsed() >= Duration::from_secs(2)) {
                out.added |= self.reread(a, look);
                out.dirty = true;
            }
        }
        out
    }

    fn sessions(&self, account: &AccountCfg, look: &Look) -> Vec<Session> {
        let rows = self.chats.get(&account.name).map(|v| v.as_slice()).unwrap_or(&[]);
        rows.iter()
            .map(|c| {
                let key = Session::key_for(Harness::Cursor, &account.name, &c.id);
                let mut s = crate::board::cursor_session(account, c, self.held(&c.id), self.app_running, look.seen.seen_at(&key), look.now);
                s.archived |= look.archived_here(&key, s.last_activity);
                s.mode = chat_mode(c, self.auto_run.get(&account.name).unwrap_or(&AutoRun::default()));
                s.model = c.model.as_deref().map(|m| model_name(m, &self.names));
                s
            })
            .collect()
    }

    fn cwds(&self) -> Vec<String> {
        self.chats.values().flatten().filter_map(|c| c.cwd.clone()).collect()
    }
}

/* ---- Privacy Mode ---- */

/// Cursor's API, where the account's Privacy Mode is read and set, as Cursor 3.22.7's Settings
/// and `cursor-agent` 2026.08.11 do: Connect's JSON over HTTP, `{}` to `GetUserPrivacyMode` and
/// `{privacyMode}` to `SetUserPrivacyMode`. The app keeps a copy in its ItemTable
/// (`cursorai/donotchange/newPrivacyMode2`) that it reads again every five minutes; writing it
/// changes nothing on the server.
const DASHBOARD: &str = "https://api2.cursor.sh/aiserver.v1.DashboardService";
/// Where a person changes it by hand.
const PRIVACY_PAGE: &str = "https://cursor.com/dashboard?tab=settings&openPrivacy=true";
/// Privacy Mode as the Settings picker sets it: not trained on, though code may be kept for
/// features that need it (Background Agents). `PRIVACY_MODE_NO_STORAGE` (Legacy) is offered only
/// to accounts already on it.
const NO_TRAINING: &str = "PRIVACY_MODE_NO_TRAINING";

/// The sign-in to ask Cursor's API with: the app's, whose email Settings shows for the account;
/// for a folder the app doesn't use, or with the app signed out, the command line's, kept in the
/// account's folder (`auth.json`, `{accessToken, refreshToken, apiKey…}`) or the keychain. The
/// token is never logged, and Codealong never refreshes it.
fn api_token(home: &Path) -> Option<String> {
    let cli = || procs::run("/usr/bin/security", &["find-generic-password", "-a", "cursor-user", "-s", "cursor-access-token", "-w"], None, Duration::from_secs(10)).ok();
    saved_token(home).or_else(|| cli().and_then(usable_token))
}

/// The sign-in kept where Codealong reads it without asking the keychain: the app's, or the
/// command line's file. What's read every couple of minutes goes by this alone, as the command
/// line's keychain item is another program's, which macOS may ask you before handing over.
fn saved_token(home: &Path) -> Option<String> {
    let file = || {
        let v: Value = serde_json::from_str(&std::fs::read_to_string(home.join("auth.json")).ok()?).ok()?;
        v.get("accessToken").and_then(Value::as_str).map(str::to_string)
    };
    let app = || app_db(home).and_then(|p| crate::harness::open_database(&p)).and_then(|db| db.query_row("SELECT CAST(value AS TEXT) FROM ItemTable WHERE key = 'cursorAuth/accessToken'", [], |r| r.get::<_, String>(0)).ok());
    app().and_then(usable_token).or_else(|| file().and_then(usable_token))
}

fn usable_token(t: String) -> Option<String> {
    Some(t.trim().to_string()).filter(|t| !t.is_empty())
}

/// What a call says when Cursor turned its sign-in away.
const EXPIRED: &str = "the sign-in has expired: sign in to Cursor again";

fn dashboard(token: &str, method: &str, body: &Value) -> Result<Value, String> {
    let headers = [format!("Authorization: Bearer {token}"), "Connect-Protocol-Version: 1".to_string()];
    let reply = crate::http::call("POST", &format!("{DASHBOARD}/{method}"), &headers, Some(body), Duration::from_secs(15))?;
    match reply.status {
        200..=299 => reply.json().ok_or_else(|| "unexpected response".to_string()),
        401 | 403 => Err(EXPIRED.to_string()),
        other => Err(format!("HTTP {other}")),
    }
}

/// Where the account stands from `GetUserPrivacyMode`'s answer: `privacyMode`, and
/// `isEnforcedByTeam` when a team sets it. Fields at their defaults are left out.
fn privacy_training(account: &AccountCfg, v: &Value) -> crate::training::Training {
    use crate::training::{State, Training};
    let team = v.get("isEnforcedByTeam").and_then(Value::as_bool).unwrap_or(false);
    let reading = match v.get("privacyMode").and_then(Value::as_str).unwrap_or("PRIVACY_MODE_UNSPECIFIED") {
        "PRIVACY_MODE_NO_TRAINING" => Training::new(account, State::Off, "Privacy Mode is on: Cursor doesn't train on your code."),
        "PRIVACY_MODE_NO_STORAGE" => Training::new(account, State::Off, "Privacy Mode (Legacy) is on: Cursor doesn't train on or keep your code."),
        "PRIVACY_MODE_USAGE_DATA_TRAINING_ALLOWED" => Training::new(account, State::On, "Share Data is on: Cursor keeps and trains on your prompts, edits and usage.").stoppable(),
        "PRIVACY_MODE_USAGE_CODEBASE_TRAINING_ALLOWED" => Training::new(account, State::On, "Share Data is on: Cursor keeps and trains on your prompts, edits, usage and codebase.").stoppable(),
        _ => Training::new(account, State::Unset, "Cursor has no Privacy Mode set for this account. Turning training off sets it.").stoppable(),
    }
    .at(PRIVACY_PAGE);
    if team {
        // A team's setting can't be changed from a member's account.
        return Training { can_stop: false, note: format!("{} Your team sets it.", reading.note), ..reading };
    }
    reading
}

fn read_privacy(account: &AccountCfg) -> crate::training::Training {
    use crate::training::Training;
    let Some(token) = api_token(&account.home_path()) else {
        return Training::unknown(account, "Not signed in to Cursor.").at(PRIVACY_PAGE);
    };
    match dashboard(&token, "GetUserPrivacyMode", &serde_json::json!({})) {
        Ok(v) => privacy_training(account, &v),
        Err(why) => Training::unknown(account, format!("Couldn't read its Privacy Mode: {why}.")).at(PRIVACY_PAGE),
    }
}

fn stop_privacy(account: &AccountCfg) -> anyhow::Result<()> {
    let token = api_token(&account.home_path()).ok_or_else(|| anyhow::anyhow!("{} isn't signed in to Cursor.", account.name))?;
    dashboard(&token, "SetUserPrivacyMode", &serde_json::json!({ "privacyMode": NO_TRAINING })).map_err(|why| anyhow::anyhow!("Couldn't turn on Privacy Mode for {}: {why}.", account.name))?;
    Ok(())
}

/* ---- Plan usage ---- */

/// The account's usage this billing period, as Cursor's Settings and the usage line under its
/// chat box read it (3.22.7): `GetCurrentPeriodUsage` on the same API, asked `{}`, and the
/// plan's name from `GetPlanInfo` (`planInfo.planName`, `Pro+`). Blocks for a second or so;
/// call it off the engine thread.
fn read_usage(account: &AccountCfg, now: i64) -> crate::model::Usage {
    let mut u = crate::model::Usage { loading: false, ..crate::usage::loading(account) };
    let Some(token) = saved_token(&account.home_path()) else {
        u.error = Some("Not signed in to Cursor".to_string());
        u.needs_sign_in = true;
        return u;
    };
    match dashboard(&token, "GetCurrentPeriodUsage", &serde_json::json!({})) {
        Ok(v) => {
            u.as_of = Some(now);
            u.meters = usage_meters(&v, usage_display(&account.home_path()));
            if u.meters.is_empty() {
                u.error = Some("Cursor reports no usage limits for this plan.".to_string());
            }
            // Only the name, which nothing waits on: a plan read that fails leaves it out.
            u.plan = dashboard(&token, "GetPlanInfo", &serde_json::json!({})).ok().and_then(|p| p.pointer("/planInfo/planName")?.as_str().map(str::to_string)).filter(|n| !n.is_empty());
        }
        Err(why) => {
            u.needs_sign_in = why == EXPIRED;
            u.error = Some(format!("Couldn't read usage: {why}"));
        }
    }
    u
}

/// How Cursor shows an account's usage, as its Settings decides it (3.22.7's `usageDisplay`).
#[derive(Debug, Clone, Copy, PartialEq)]
enum UsageDisplay {
    /// One limit for everything the plan includes, the share of each kind of model beneath it.
    Total,
    /// A plan that spills over (Pro+ here): a limit for Cursor's own models, and one for the
    /// models you name, which a Cursor model's use spills into past its own.
    Split,
    /// The Express plan: only Cursor's own models.
    CursorModelsOnly,
}

/// The display the app's settings give the signed-in account: `membershipType` `express` is
/// Cursor's models only; `hasTieredSelfServeTeamSpillover`, or `hasAutoSpillover` on a plan other
/// than Enterprise, is split; anything else, and an account without the app's settings, total.
fn usage_display(home: &Path) -> UsageDisplay {
    let Some(db) = app_db(home).and_then(|p| crate::harness::open_database(&p)) else { return UsageDisplay::Total };
    let got = db.query_row(
        "SELECT json_extract(t, '$.membershipType'), json_extract(t, '$.hasAutoSpillover'), json_extract(t, '$.hasTieredSelfServeTeamSpillover') \
         FROM (SELECT CAST(value AS TEXT) AS t FROM ItemTable WHERE key = ?1)",
        [APPLICATION_USER],
        |r| Ok((r.get::<_, Option<String>>(0)?, r.get::<_, Option<i64>>(1)?, r.get::<_, Option<i64>>(2)?)),
    );
    let Ok((membership, auto, tiered)) = got else { return UsageDisplay::Total };
    display_of(membership.as_deref(), auto == Some(1), tiered == Some(1))
}

fn display_of(membership: Option<&str>, auto_spillover: bool, tiered_spillover: bool) -> UsageDisplay {
    match membership {
        Some("express") => UsageDisplay::CursorModelsOnly,
        _ if tiered_spillover || (auto_spillover && membership != Some("enterprise")) => UsageDisplay::Split,
        _ => UsageDisplay::Total,
    }
}

/// A meter for each limit in `GetCurrentPeriodUsage`'s answer as Cursor shows it (`display`):
/// `{billingCycleStart, billingCycleEnd, planUsage, spendLimitUsage…}`, times as strings of epoch
/// milliseconds, spends in cents, percents out of 100. Split, Cursor Models (Auto, Composer and
/// its own Grok) are `autoPercentUsed` and Other Models, the ones you name, `apiPercentUsed`,
/// as the usage line under its chat box says them; Express has the first alone. Total is
/// `totalPercentUsed`, as Settings shows it, else what's spent of what the plan includes
/// (`includedSpend` of `limit`). On-demand spend past the plan counts against a limit of its own
/// once you set one (`spendLimitUsage.individualLimit`). Each resets as the billing cycle ends.
fn usage_meters(v: &Value, display: UsageDisplay) -> Vec<crate::model::Meter> {
    let Some(plan) = v.get("planUsage") else { return Vec::new() };
    let resets_at = v.get("billingCycleEnd").and_then(|e| e.as_str().and_then(|s| s.parse().ok()).or_else(|| e.as_i64()));
    let meter = |key: &str, label: &str, window: &str, percent: f64, model: Option<&str>| crate::model::Meter {
        key: key.to_string(),
        label: label.to_string(),
        window: window.to_string(),
        percent: percent.clamp(0.0, 100.0),
        resets_at,
        severity: crate::usage::severity(None, percent),
        model: model.map(str::to_string),
    };
    let number = |v: &Value, key: &str| v.get(key).and_then(Value::as_f64);
    let mut out = Vec::new();
    let cursor_models = || meter("model:Cursor", "Cursor", "Monthly, Cursor models (Auto, Composer and Cursor Grok)", number(plan, "autoPercentUsed").unwrap_or(0.0), Some("Cursor"));
    match display {
        UsageDisplay::Split => {
            out.push(cursor_models());
            out.push(meter("model:Other", "Other", "Monthly, other models", number(plan, "apiPercentUsed").unwrap_or(0.0), Some("Other")));
        }
        UsageDisplay::CursorModelsOnly => out.push(cursor_models()),
        UsageDisplay::Total => {
            let limit = number(plan, "limit").unwrap_or(0.0);
            let spent = (limit > 0.0).then(|| number(plan, "includedSpend").unwrap_or(0.0) / limit * 100.0);
            out.extend(number(plan, "totalPercentUsed").or(spent).map(|p| meter("month", "Month", "Monthly, included usage", p, None)));
        }
    }
    if let Some(spend) = v.get("spendLimitUsage").filter(|s| number(s, "individualLimit").is_some_and(|l| l > 0.0)) {
        let (used, limit) = (number(spend, "individualUsed").unwrap_or(0.0), number(spend, "individualLimit").unwrap_or(0.0));
        out.push(meter("spend", "On-demand", "On-demand spend", used / limit * 100.0, None));
    }
    out
}

/* ---- The adapter ---- */

/// The only way in for an app chat: nothing there takes a prompt from outside (a `/prompt` link
/// makes a new chat), so the app comes forward on the chat with the prompt on the clipboard to
/// paste.
#[derive(Debug, Clone, PartialEq)]
pub struct OnClipboard {
    pub chat: String,
}

impl crate::send::Own for OnClipboard {
    fn way(&self) -> crate::send::Way {
        crate::send::Way::Cursor
    }

    fn takes_agents(&self) -> bool {
        false
    }

    fn leaves_it_to_paste(&self) -> bool {
        true
    }

    fn deliver(&self, d: &crate::send::Delivery) -> anyhow::Result<crate::send::Sent> {
        crate::send::copy(&crate::attachments::expand(d.text, crate::attachments::Style::Path))?;
        open_chat(&self.chat)?;
        Ok(crate::send::opened(format!("Cursor is open on {} and the prompt is on the clipboard. Paste it to send it.", d.title())))
    }
}

/// The link that opens an app chat: the Agents window comes forward on it. The window looks the
/// id up among the chats it lists, local ones included, and says it can't find it when it isn't
/// one of them (3.22.7's `GlassDeeplinkHandler.handleAgentOpen`, tried 2026-10-14).
fn chat_link(id: &str) -> String {
    format!("cursor://anysphere.cursor-deeplink/agent?id={id}")
}

/// Brings Cursor forward on an app chat, in its Agents window (`chat_link`); the link opens
/// Cursor when it isn't running.
pub fn open_chat(id: &str) -> anyhow::Result<()> {
    let app = crate::detect::app("Cursor.app").ok_or_else(|| anyhow::anyhow!("Cursor isn't installed in Applications."))?;
    let app = app.to_string_lossy().into_owned();
    let link = chat_link(id);
    crate::os::open::in_app(crate::os::open::App::Named(&app), Some(&link), Duration::from_secs(10)).map_err(|e| anyhow::anyhow!("Couldn't open Cursor: {e}"))
}

/// Cursor (`harness::Adapter`): the app's chats, read from its database, and the command line's,
/// which starts and resumes chats in a terminal. The app takes nothing from outside.
pub struct Cursor;

impl Cursor {
    fn chat_dir(&self, account: &AccountCfg, s: &Session) -> Option<PathBuf> {
        cli_chat_dir(&account.home_path(), &s.id)
    }

    fn transcript(&self, account: &AccountCfg, s: &Session) -> anyhow::Result<PathBuf> {
        let side = self.chat_dir(account, s).map(|d| read_sidecar(&d)).unwrap_or_default();
        cli_transcript(&account.home_path(), &s.id, &side).ok_or_else(|| anyhow::anyhow!("Cursor hasn't written this chat's transcript yet."))
    }

    fn app_database(&self, account: &AccountCfg) -> anyhow::Result<Connection> {
        app_db(&account.home_path()).and_then(|p| crate::harness::open_database(&p)).ok_or_else(|| anyhow::anyhow!("Couldn't read Cursor's database."))
    }

    /// An app chat's calls that haven't finished, from its latest messages.
    fn app_open(&self, account: &AccountCfg, s: &Session) -> Vec<Call> {
        let Ok(db) = self.app_database(account) else { return Vec::new() };
        app_tail(&db, &Chat { id: s.id.clone(), cwd: Some(s.cwd.clone()), ..Chat::default() }).open
    }
}

/// Whether a session's turn is under way, so its last message may still be written.
fn under_way(s: &Session) -> bool {
    matches!(s.status, Status::Working | Status::Needs) && s.live
}

impl crate::harness::Adapter for Cursor {
    fn harness(&self) -> Harness {
        Harness::Cursor
    }

    /// Trusts the folder for the command line (`trust_folder`), and Codealong's folders of
    /// attachments, which a start adds (`--add-dir`): in a folder it doesn't trust, an added one
    /// too, `cursor-agent` asks first and writes no chat until it's answered, so a review
    /// started behind your windows waited there unseen (2026-10-18). You chose the folder, as
    /// answering the prompt would. A start goes ahead without it, and Cursor asks.
    fn before_start(&self, account: &AccountCfg, folder: &Path) {
        for f in [folder.to_path_buf(), crate::attachments::images_dir(), crate::attachments::files_dir()] {
            if let Err(e) = trust_folder(&data_dir(|k| launch_var(cli_command(account), k), &crate::util::home()), &f, &crate::util::home()) {
                eprintln!("Couldn't trust {} for {}: {e:#}", crate::util::tilde(&f), account.name);
            }
        }
    }

    /// One account in `~/.cursor`, which the app's chats go with (`app_db`); `CURSOR_CONFIG_DIR`
    /// moves the command line's folder.
    fn spec(&self) -> &'static crate::detect::Spec {
        const SPEC: crate::detect::Spec = crate::detect::Spec { harness: "cursor", program: Some(CLI), default_command: Some(CLI), var: Some(("CURSOR_CONFIG_DIR", None)), default_dir: "~/.cursor", siblings: None };
        &SPEC
    }

    fn app(&self) -> &'static str {
        "Cursor"
    }

    /// The app's sign-in, by the email it keeps beside its tokens, or the command line's file,
    /// when it keeps one rather than the keychain. Only the email is read.
    fn signed_in(&self, home: &Path) -> (bool, Option<String>) {
        let db = app_db(home).and_then(|p| crate::harness::open_database(&p));
        let email = db.and_then(|db| db.query_row("SELECT CAST(value AS TEXT) FROM ItemTable WHERE key = 'cursorAuth/cachedEmail'", [], |r| r.get::<_, String>(0)).ok());
        let email = email.map(|e| e.trim().to_string()).filter(|e| e.contains('@'));
        (email.is_some() || home.join("auth.json").is_file(), email)
    }

    /// The command line signs in with `login`; without it, the app does from its own window.
    fn sign_in_command(&self, account: &AccountCfg) -> String {
        if self.missing_cli(account, &crate::util::home()).is_none() {
            format!("{} login", cli_command(account))
        } else {
            "open -a Cursor".to_string()
        }
    }

    fn installed(&self, home: &Path, on_path: &dyn Fn(&str) -> bool) -> bool {
        crate::detect::app("Cursor.app").is_some() || on_path(CLI) || crate::detect::is_executable(&home.join(".local/bin").join(CLI))
    }

    /// The app needs none: only its command line starts chats from Codealong.
    fn missing_cli(&self, account: &AccountCfg, home: &Path) -> Option<&'static str> {
        crate::harness::missing_program(cli_command(account), CLI, home, || cli_installed(home))
    }

    fn folder_use(&self, dir: &Path) -> crate::harness::FolderUse {
        let mut files: Vec<PathBuf> = std::fs::read_dir(dir.join("chats")).into_iter().flatten().flatten().map(|e| e.path()).collect();
        if let Some(db) = app_db(dir) {
            files.push(PathBuf::from(format!("{}-wal", db.display())));
            files.push(db);
        }
        let has_data = files.iter().any(|p| p.is_dir() || (p.is_file() && !file_name(p).ends_with("-wal")));
        crate::harness::FolderUse { has_data, last_used: files.iter().filter_map(|p| crate::util::mtime_ms(p)).max(), signed_in: dir.join("auth.json").is_file() }
    }

    /// `~/.cursor/skills`, in your home whatever folder the account is kept in: the app and the
    /// command line read personal skills from there, not from `CURSOR_CONFIG_DIR`.
    fn skills_dir(&self, _account: &AccountCfg) -> Option<PathBuf> {
        Some(crate::util::home().join(SKILL_FOLDERS[0]))
    }

    /// The session's own folder's, its workspace: Cursor reads none above it.
    fn worktree_skills(&self, folders: &[PathBuf]) -> Vec<crate::skills::Root> {
        folders.first().map(|dir| SKILL_FOLDERS.iter().map(|f| skill_root(dir.join(f), false)).collect()).unwrap_or_default()
    }

    /// The skills Cursor ships, and the same folders in your home.
    fn account_skills(&self, _account: &AccountCfg) -> Vec<crate::skills::Root> {
        let home = crate::util::home();
        std::iter::once(skill_root(home.join(OWN_SKILLS), false)).chain(SKILL_FOLDERS.iter().map(|f| skill_root(home.join(f), true))).collect()
    }

    /// The command line runs a skill a prompt names as `/name` (its `findSkillReferences`), as a
    /// first prompt too, and finds only names of letters, digits, `-` and `_` that way. The app
    /// makes a skill of a `/name` picked from its box's menu, and nothing reaches its chats but
    /// what you paste there.
    fn skill_mention(&self, name: &str) -> Option<String> {
        let found = !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
        found.then(|| format!("/{name}"))
    }

    /// The command line starts a chat in a terminal; the app only in its own window, so without
    /// the command line (`missing_cli`) there's nowhere (start.rs `places_of`).
    fn places(&self, _account: &AccountCfg, _home: &Path) -> std::result::Result<Vec<crate::start::Place>, String> {
        Ok(vec![crate::start::Place::Terminal, crate::start::Place::Background])
    }

    /// The command line's chat opens in its terminal, or a new one that resumes it
    /// (`cursor-agent --resume`); a print run has it until it ends. The app's opens in Cursor's
    /// Agents window (`chat_link`).
    fn targets(&self, s: &Session, o: &crate::harness::Opens) -> Vec<crate::model::Target> {
        let who = o.who.clone();
        match s.surface {
            Surface::Cli => {
                let busy = s.live && s.pid.is_none();
                vec![o.terminal(s, if busy { format!("{who} · a run without a terminal has it open") } else { who }, busy)]
            }
            Surface::App => vec![o.way(s, "cursor", "app", "Open in Cursor".into(), format!("{who} · the chat, in Cursor's Agents window"), true, false)],
        }
    }

    fn open(&self, s: &Session, action: &str) -> anyhow::Result<()> {
        match action {
            "cursor" => open_chat(&s.id),
            other => anyhow::bail!("Unknown action {other:?}"),
        }
    }

    fn app_bundle(&self, action: &str) -> Option<&'static str> {
        (action == "cursor").then_some(APP_BUNDLE_ID)
    }

    /// Every chat's transcript, the app's and the command line's alike.
    fn search_files(&self, account: &AccountCfg, since: i64) -> Vec<(PathBuf, String, i64)> {
        let Ok(projects) = std::fs::read_dir(account.home_path().join("projects")) else { return Vec::new() };
        let mut out = Vec::new();
        for p in projects.flatten() {
            let Ok(chats) = std::fs::read_dir(p.path().join("agent-transcripts")) else { continue };
            for chat in chats.flatten() {
                let id = file_name(&chat.path());
                let path = chat.path().join(format!("{id}.jsonl"));
                if let Some(at) = crate::util::mtime_ms(&path).filter(|at| *at >= since) {
                    out.push((path, Session::key_for(Harness::Cursor, &account.name, &id), at));
                }
            }
        }
        out
    }

    /// What you asked, and what the agent said.
    fn prose(&self, raw: &[u8]) -> Option<(i64, bool, String)> {
        if memchr::memmem::find(raw, br#""type":"text""#).is_none() {
            return None;
        }
        let v: Value = serde_json::from_slice(raw).ok()?;
        let said = match line(&v)? {
            Line::You { text, at } => (at.unwrap_or(0), true, text),
            Line::Agent { text, .. } => (0, false, text),
            Line::TurnEnded { .. } => return None,
        };
        (!said.2.is_empty()).then_some(said)
    }

    /// Codealong picks a new chat's id (`--new-session-id`, which takes a v4 UUID, as Codealong's
    /// are); the prompt goes as an argument, its attachments as paths whose folders are added
    /// (`--add-dir`, once each) so it reads them without asking. `--worktree` takes an optional
    /// name, so it goes after the prompt.
    fn start_in_terminal(&self, account: &AccountCfg, ask: &crate::harness::StartAsk) -> crate::harness::Typed {
        use crate::start::{FILES_VAR, IMAGES_VAR, PROMPT_VAR};
        let (has_images, has_files) = crate::attachments::kinds(ask.prompt);
        let prompt = crate::attachments::expand_in(ask.prompt, ask.images_dir, ask.files_dir, crate::attachments::Style::Path);
        let prompt = prompt.trim();
        let mut input = format!("{} --new-session-id {}", cli_command(account), ask.session_id);
        let mut env = Vec::new();
        for (has, var, dir) in [(has_images, IMAGES_VAR, ask.images_dir), (has_files, FILES_VAR, ask.files_dir)] {
            if has {
                input.push_str(&format!(" --add-dir \"${var}\""));
                env.push((var.to_string(), dir.to_string_lossy().to_string()));
            }
        }
        if let Some(m) = ask.model {
            input.push_str(&format!(" --model '{m}'"));
        }
        if !prompt.is_empty() {
            input.push_str(&format!(" \"${PROMPT_VAR}\""));
            env.push((PROMPT_VAR.to_string(), crate::start::as_argument(prompt)));
        }
        if ask.worktree {
            input.push_str(" --worktree");
        }
        input.push('\n');
        crate::harness::Typed { input, env, key: Some(Session::key_for(Harness::Cursor, &account.name, ask.session_id)) }
    }

    fn makes_worktrees(&self) -> bool {
        true
    }

    fn models(&self, account: &AccountCfg) -> Vec<crate::start::ModelOption> {
        start_models(&account.home_path())
    }

    /// No `model_command`: the command line's `/model <id>` saves the pick as its default for
    /// every later start (`cli-config.json`), where a switch is for its session alone.
    fn takes_model(&self) -> bool {
        true
    }

    fn session_cwds(&self, home: &Path, since: i64) -> Vec<String> {
        let app = app_db(home).and_then(|p| crate::harness::open_database(&p)).and_then(|db| read_app_chats(&db, since, &HashSet::new()).ok()).unwrap_or_default();
        let cli = read_cli_chats(home, since, &HashSet::new()).into_iter().map(|c| Chat { cwd: cli_cwd(&c), ..c });
        app.into_iter().chain(cli).filter_map(|c| c.cwd).collect()
    }

    /// Usage is asked of Cursor's servers with a sign-in kept where Codealong reads it without
    /// the keychain (`saved_token`): the app's, or the command line's file.
    fn no_limits(&self, account: &AccountCfg) -> Option<&'static str> {
        (!self.polls_usage(account)).then_some("Codealong reads Cursor's usage with the Cursor app's sign-in, which this account doesn't have.")
    }

    fn polls_usage(&self, account: &AccountCfg) -> bool {
        let home = account.home_path();
        app_db(&home).is_some_and(|db| db.is_file()) || home.join("auth.json").is_file()
    }

    fn read_usage(&self, account: &AccountCfg, now: i64) -> crate::model::Usage {
        read_usage(account, now)
    }

    /// Privacy Mode, in the account's settings on Cursor's servers.
    fn training(&self, account: &AccountCfg) -> crate::training::Training {
        read_privacy(account)
    }

    /// Turns Privacy Mode on, as Cursor's Settings does.
    fn stop_training(&self, account: &AccountCfg) -> anyhow::Result<()> {
        stop_privacy(account)
    }

    /// The servers in its folder's `mcp.json` (`mcpServers`), which the app and its command line
    /// share. A server turned off in the app's settings is kept in its state database, not there,
    /// so it's listed still.
    fn mcp(&self, account: &AccountCfg) -> crate::mcp::Mcp {
        let file = account.home_path().join("mcp.json");
        match crate::mcp::json_file(&file) {
            Ok(v) => crate::mcp::Mcp::new(account, v.map(|v| crate::mcp::servers_in(&v["mcpServers"], &crate::util::tilde(&file))).unwrap_or_default(), None),
            Err(why) => crate::mcp::Mcp::new(account, Vec::new(), Some(why)),
        }
    }

    fn records(&self, account: &AccountCfg, s: &Session) -> Vec<PathBuf> {
        match s.surface {
            Surface::App => app_db(&account.home_path()).into_iter().collect(),
            Surface::Cli => self.transcript(account, s).ok().into_iter().chain(self.chat_dir(account, s).map(|d| d.join(STORE_DB))).collect(),
        }
    }

    fn conversation(&self, ctx: &Ctx, s: &Session, before: Option<u64>, limit: usize) -> anyhow::Result<Page> {
        match s.surface {
            Surface::App => Ok(app_page(&self.app_database(ctx.account)?, &s.id, under_way(s), before, limit)),
            Surface::Cli => {
                let path = self.transcript(ctx.account, s)?;
                let page = crate::transcript::file_page(&path, transcript_items, before, limit)?;
                Ok(timed(page, s, before.is_none(), crate::util::mtime_ms(&path)))
            }
        }
    }

    fn conversation_after(&self, ctx: &Ctx, s: &Session, after: u64) -> anyhow::Result<Page> {
        match s.surface {
            Surface::App => Ok(app_newer(&self.app_database(ctx.account)?, &s.id, under_way(s), after)),
            Surface::Cli => {
                let path = self.transcript(ctx.account, s)?;
                Ok(timed(crate::transcript::file_newer(&path, transcript_items, after)?, s, true, crate::util::mtime_ms(&path)))
            }
        }
    }

    /// The command line's chat takes a prompt typed into its terminal, or a new terminal that
    /// resumes it; one a print run has open takes nothing until that ends, nor one whose chat
    /// isn't written yet (`Chat::unwritten`), whose terminal shows what the command line asks
    /// first, which a prompt typed there would answer. Nothing reaches the app's from outside:
    /// the app comes forward with the prompt on the clipboard.
    fn routes(&self, account: &AccountCfg, s: &Session) -> Vec<crate::send::Route> {
        match (s.surface, s.pid, s.live) {
            (Surface::Cli, Some(_), _) if self.chat_dir(account, s).is_none() => Vec::new(),
            (Surface::Cli, Some(pid), _) => vec![crate::send::Route::Type { pid }],
            (Surface::Cli, None, false) => vec![crate::send::Route::Resume],
            (Surface::Cli, None, true) => Vec::new(),
            _ => vec![OnClipboard { chat: s.id.clone() }.into()],
        }
    }

    /// `cursor-agent --resume <id> <prompt>` goes on with a chat in its terminal UI, on the
    /// model asked for, as a start is; without a prompt it opens on the chat and waits for one.
    fn resume(&self, account: &AccountCfg, s: &Session, ask: &crate::harness::StartAsk) -> anyhow::Result<(String, Vec<(String, String)>)> {
        use crate::start::{FILES_VAR, IMAGES_VAR, PROMPT_VAR};
        let (has_images, has_files) = crate::attachments::kinds(ask.prompt);
        let prompt = crate::attachments::expand_in(ask.prompt, ask.images_dir, ask.files_dir, crate::attachments::Style::Path).trim().to_string();
        let mut input = format!("{} --resume {}", cli_command(account), s.id);
        let mut env = Vec::new();
        for (has, var, dir) in [(has_images, IMAGES_VAR, ask.images_dir), (has_files, FILES_VAR, ask.files_dir)] {
            if has {
                input.push_str(&format!(" --add-dir \"${var}\""));
                env.push((var.to_string(), dir.to_string_lossy().to_string()));
            }
        }
        if let Some(m) = ask.model {
            input.push_str(&format!(" --model '{m}'"));
        }
        if !prompt.is_empty() {
            input.push_str(&format!(" \"${PROMPT_VAR}\""));
            env.push((PROMPT_VAR.to_string(), crate::start::as_argument(&prompt)));
        }
        input.push('\n');
        Ok((input, env))
    }

    /// A command line chat goes on in a terminal, or one behind your windows; the command line
    /// can't go on with the app's.
    fn resumes_in(&self, place: crate::start::Place) -> bool {
        use crate::start::Place;
        matches!(place, Place::Terminal | Place::Background)
    }

    /// What an app chat waits on you to allow or answer. It's answered in the app: nothing
    /// outside takes an answer, and the command line writes nothing down while its prompt is up.
    fn pending(&self, ctx: &Ctx, s: &Session) -> Option<crate::harness::Pending> {
        if s.surface != Surface::App {
            return None;
        }
        let open = self.app_open(ctx.account, s);
        let c = open.iter().find(|c| c.waits).or(open.first())?.clone();
        Some(crate::harness::Pending { id: c.id, label: c.label, input: Some(c.input).filter(|i| !i.is_empty()), cannot: Some("Cursor asks in its own window; answer it there.".into()), always: false, no_once: false, plan: false, message: false, always_rule: None, questions: Vec::new(), unwritten: false, terminal: None })
    }

    fn open_calls(&self, ctx: &Ctx, s: &Session) -> Vec<String> {
        match s.surface {
            Surface::App => self.app_open(ctx.account, s).into_iter().map(|c| c.input).collect(),
            Surface::Cli => Vec::new(),
        }
    }

    /// The app's database, and each command line chat's.
    fn databases(&self) -> &'static [&'static str] {
        &[STATE_DB, STORE_DB]
    }

    fn reader(&self, _wake: crate::harness::Wake) -> Box<dyn crate::harness::Reader> {
        Box::new(Reader::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::Adapter as _;
    use serde_json::json;

    /// The app's tables as 3.22 makes them, with a chat in each state: one generating with a
    /// call waiting to be allowed, one done in a worktree the app made, an empty one a new
    /// window made, a draft, a subagent's, and an old one.
    fn app_fixture() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE ItemTable (key TEXT UNIQUE ON CONFLICT REPLACE, value BLOB);
             CREATE TABLE cursorDiskKV (key TEXT UNIQUE ON CONFLICT REPLACE, value BLOB);
             CREATE TABLE composerHeaders (composerId TEXT PRIMARY KEY, workspaceId TEXT, createdAt INTEGER, lastUpdatedAt INTEGER, isArchived INTEGER, isSubagent INTEGER, recency INTEGER, checkpointAt INTEGER, value TEXT, subagentTypeName TEXT);",
        )
        .unwrap();
        let header = |id: &str, created: i64, updated: Option<i64>, subagent: i64, value: Value| {
            db.execute("INSERT INTO composerHeaders (composerId, workspaceId, createdAt, lastUpdatedAt, isArchived, isSubagent, value) VALUES (?1, 'w', ?2, ?3, 0, ?4, ?5)", rusqlite::params![id, created, updated, subagent, value.to_string()]).unwrap();
        };
        let folder = |p: &str| json!({ "id": "w", "uri": { "fsPath": p } });
        let repos = json!([{ "repoPath": "/Users/me/app", "branches": [{ "branchName": "old", "lastInteractionAt": 1 }, { "branchName": "fix-build", "lastInteractionAt": 9 }] }]);
        header("c1", 1_000, Some(5_000), 0, json!({ "type": "head", "name": "Fix the build", "hasBlockingPendingActions": true, "workspaceIdentifier": folder("/Users/me/app/"), "trackedGitRepos": repos }));
        header("c2", 1_000, Some(4_000), 0, json!({ "type": "head", "name": "Explain auth", "workspaceIdentifier": folder("/Users/me/api") }));
        header("empty", 3_000, None, 0, json!({ "type": "head", "workspaceIdentifier": folder("/Users/me/app") }));
        header("draft", 3_000, Some(3_500), 0, json!({ "type": "head", "name": "Half typed", "isDraft": true }));
        header("task-1", 3_000, None, 1, json!({ "type": "head", "name": "Subagent" }));
        header("old", 10, Some(20), 0, json!({ "type": "head", "name": "Old", "workspaceIdentifier": folder("/Users/me/app") }));
        let kv = |key: String, value: Value| {
            db.execute("INSERT INTO cursorDiskKV (key, value) VALUES (?1, ?2)", rusqlite::params![key, value.to_string()]).unwrap();
        };
        // Cursor's list of the models it offers, as its settings keep it.
        let models = json!({
            "availableDefaultModels2": [
                { "name": "default", "clientDisplayName": "Auto", "defaultOn": true },
                { "name": "grok-4.7", "clientDisplayName": "Grok 4.7", "defaultOn": true },
                { "name": "gpt-5.5", "clientDisplayName": "" },
                { "name": "kimi-k3", "clientDisplayName": "Kimi K3", "defaultOn": false },
                { "name": "glm-5.2", "clientDisplayName": "GLM 5.2", "defaultOn": false },
                { "name": "muse-spark-1.3", "clientDisplayName": "Muse Spark 1.3", "defaultOn": true }
            ],
            "aiSettings": { "modelOverrideEnabled": ["kimi-k3"], "modelOverrideDisabled": ["muse-spark-1.3"] }
        });
        db.execute("INSERT INTO ItemTable (key, value) VALUES ('src.vs.platform.reactivestorage.browser.reactiveStorageServiceImpl.persistentStorage.applicationUser', ?1)", [models.to_string()]).unwrap();
        let order = |ids: &[(&str, i64)]| ids.iter().map(|(id, t)| json!({ "bubbleId": id, "type": t })).collect::<Vec<_>>();
        kv("composerData:c1".into(), json!({ "status": "generating", "lastUpdatedAt": 6_000, "name": "Fix the build", "hasBlockingPendingActions": true, "modelConfig": { "modelName": "claude-4.5-sonnet", "selectedModels": [{ "modelId": "grok-4.7", "parameters": [{ "id": "context", "value": "256k" }, { "id": "reasoning_effort", "value": "high" }, { "id": "fast", "value": "true" }] }] }, "fullConversationHeadersOnly": order(&[("u1", 1), ("a1", 2), ("t1", 2), ("t2", 2)]) }));
        kv("composerData:c2".into(), json!({ "status": "completed", "lastUpdatedAt": 4_000, "modelConfig": { "modelName": "default" }, "gitWorktree": { "worktreePath": "/Users/me/.cursor/worktrees/api/sky", "branchName": "sky" }, "fullConversationHeadersOnly": order(&[("u1", 1), ("a1", 2)]) }));
        let bubble = |chat: &str, id: &str, v: Value| kv(format!("bubbleId:{chat}:{id}"), v);
        bubble("c1", "u1", json!({ "type": 1, "bubbleId": "u1", "text": "Fix the build", "createdAt": "2026-09-02T23:21:44.459Z" }));
        bubble("c1", "a1", json!({ "type": 2, "bubbleId": "a1", "text": "I'll run the tests first.", "createdAt": "2026-09-02T23:21:50.000Z" }));
        let ran = json!({ "name": "run_terminal_command_v2", "toolCallId": "tool_1", "status": "completed", "params": json!({ "command": "cd /Users/me/app && cargo test", "cwd": "/Users/me/app" }).to_string(), "result": json!({ "output": "2 failed\n", "rejected": false }).to_string(), "additionalData": { "status": "success" } });
        bubble("c1", "t1", json!({ "type": 2, "bubbleId": "t1", "createdAt": "2026-09-02T23:22:00.000Z", "toolFormerData": ran }));
        let asks = json!({ "name": "run_terminal_command_v2", "toolCallId": "tool_2", "status": "loading", "params": json!({ "command": "rm -rf target", "cwd": "/Users/me/app" }).to_string(), "additionalData": { "blockReason": "Not in allowlist: rm", "reviewData": { "status": "Requested" } } });
        bubble("c1", "t2", json!({ "type": 2, "bubbleId": "t2", "createdAt": "2026-09-02T23:23:00.000Z", "toolFormerData": asks }));
        bubble("c2", "u1", json!({ "type": 1, "bubbleId": "u1", "text": "How does auth work?", "createdAt": "2026-09-01T10:00:00.000Z" }));
        bubble("c2", "a1", json!({ "type": 2, "bubbleId": "a1", "text": "Sessions are cookies.\n\nThat's all.", "createdAt": "2026-09-01T10:00:05.000Z" }));
        db
    }

    fn read(db: &Connection) -> Vec<Chat> {
        let mut rows = read_app_chats(db, 1_000, &HashSet::new()).unwrap();
        let now = written(db, &rows.iter().map(|c| c.id.as_str()).collect::<Vec<_>>());
        for c in &mut rows {
            refresh(db, c);
            let w = &now[&c.id];
            (c.model, c.effort) = (w.model.clone(), w.effort.clone());
        }
        rows
    }

    #[test]
    fn lists_the_apps_chats_but_not_empty_ones_drafts_or_subagents() {
        let db = app_fixture();
        let rows = read_app_chats(&db, 1_000, &HashSet::new()).unwrap();
        assert_eq!(rows.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(), ["c1", "c2"]);
        let c = &rows[0];
        assert_eq!((c.title.as_deref(), c.cwd.as_deref(), c.branch.as_deref(), c.blocking), (Some("Fix the build"), Some("/Users/me/app"), Some("fix-build"), true));
        // A pinned one comes however old it is.
        let kept = read_app_chats(&db, 1_000, &HashSet::from(["old".to_string()])).unwrap();
        assert!(kept.iter().any(|c| c.id == "old"));
    }

    #[test]
    fn reads_where_a_chat_stands_from_its_composer_data() {
        let db = app_fixture();
        let rows = read(&db);
        let (c1, c2) = (&rows[0], &rows[1]);
        assert!(c1.generating() && c1.blocking);
        // Its latest message, not `lastUpdatedAt`, which is when the last prompt was sent.
        assert_eq!((c1.last_updated, c1.messages), (parse_rfc3339_ms("2026-09-02T23:23:00.000Z").unwrap(), 4));
        // The model it's on is the one its picker selected, as the app reads it, over the name an
        // older Cursor left beside it, and its effort is that model's.
        assert_eq!((c1.model.as_deref(), c1.effort.as_deref()), (Some("grok-4.7"), Some("high")));
        assert!(c1.written > 0);
        assert_eq!(written(&db, &["c1", "c2", "nope"]).get("c1").map(|w| w.length), Some(c1.written));
        assert_eq!(c1.tail.first_prompt.as_deref(), Some("Fix the build"));
        assert_eq!(c1.tail.last_text.as_deref(), Some("I'll run the tests first."));
        assert_eq!(c1.tail.last_tool.as_deref(), Some("Run · `rm -rf target`"));
        assert_eq!(c1.tail.open.len(), 1);
        assert!(c1.tail.open[0].waits && c1.tail.open[0].id == "tool_2");
        // Each command a vote for where it works.
        assert_eq!(c1.tail.work, ["/Users/me/app", "/Users/me/app"]);
        // A worktree the app made for the chat is where it works; one on Auto has no effort.
        assert_eq!((c2.status.as_str(), c2.cwd.as_deref(), c2.branch.as_deref()), ("completed", Some("/Users/me/.cursor/worktrees/api/sky"), Some("sky")));
        assert_eq!((c2.model.as_deref(), c2.effort.as_deref()), (Some("default"), None));
        assert_eq!(c2.tail.last_prompt_at, parse_rfc3339_ms("2026-09-01T10:00:00.000Z"));
        assert!(c2.tail.open.is_empty() && c2.tail.error.is_none());
    }

    #[test]
    fn says_a_model_and_its_effort_as_cursors_picker_does() {
        let db = app_fixture();
        let names = model_names(&db);
        assert_eq!(names.get("grok-4.7").map(String::as_str), Some("Grok 4.7"));
        // A model the list doesn't name, or names with nothing, goes by its id; Auto by its name.
        assert_eq!(["grok-4.7", "default", "gpt-5.5", "composer-9"].map(|m| model_name(m, &names)), ["Grok 4.7", "Auto", "gpt-5.5", "composer-9"]);
        assert_eq!(model_name("default", &HashMap::new()), "Auto");
        // Each model calls its effort by a name of its own; a thinking switch isn't one.
        let selected = |parameters: Value| json!({ "modelId": "m", "parameters": parameters });
        assert_eq!(effort_of(&selected(json!([{ "id": "thinking", "value": "true" }, { "id": "effort", "value": "max" }]))).as_deref(), Some("max"));
        assert_eq!(effort_of(&selected(json!([{ "id": "context", "value": "272k" }, { "id": "reasoning", "value": "extra-high" }]))).as_deref(), Some("extra-high"));
        assert_eq!(effort_of(&selected(json!([{ "id": "thinking", "value": "true" }, { "id": "fast", "value": "false" }]))), None);
        assert_eq!(effort_of(&selected(json!([]))), None);
        assert_eq!(effort_of(&Value::Null), None);
    }

    /// A switch between two models, or two levels, of one length leaves composerData as long as
    /// it was, so the reader's shortcut keeps what it read; what `written` says changes all the same.
    #[test]
    fn a_switch_of_model_or_effort_of_one_length_shows() {
        let db = app_fixture();
        let before = written(&db, &["c1"])["c1"].clone();
        let data: String = db.query_row("SELECT CAST(value AS TEXT) FROM cursorDiskKV WHERE key = 'composerData:c1'", [], |r| r.get(0)).unwrap();
        let switched = data.replace("\"grok-4.7\"", "\"grok-4.6\"").replace("\"high\"", "\"xlow\"");
        db.execute("UPDATE cursorDiskKV SET value = ?1 WHERE key = 'composerData:c1'", [&switched]).unwrap();
        let after = written(&db, &["c1"])["c1"].clone();
        assert_eq!(after.length, before.length);
        assert_eq!((after.model.as_deref(), after.effort.as_deref()), (Some("grok-4.6"), Some("xlow")));
    }

    /// A start offers what Cursor's picker shows, in its order: on by default or turned on, not
    /// turned off, Auto by the command line's name for it.
    #[test]
    fn a_start_offers_the_models_cursors_picker_shows() {
        let db = app_fixture();
        let offered: Vec<(String, bool)> = offered_models(&db).into_iter().map(|m| (m.id, m.shown)).collect();
        assert_eq!(offered, [("default".into(), true), ("grok-4.7".into(), true), ("kimi-k3".into(), true), ("glm-5.2".into(), false), ("muse-spark-1.3".into(), false)]);
        // The ones it hides are still named.
        assert_eq!(model_names(&db).get("glm-5.2").map(String::as_str), Some("GLM 5.2"));
        let dir = std::env::temp_dir().join(format!("codealong-cursor-models-{}", std::process::id()));
        let db_path = crate::os::dirs::app_data(&dir, "Cursor", "Cursor").join("User/globalStorage").join(STATE_DB);
        std::fs::create_dir_all(db_path.parent().unwrap()).unwrap();
        db.execute("VACUUM INTO ?1", [db_path.to_string_lossy()]).unwrap();
        let listed: Vec<(String, String)> = start_models(&dir.join(".cursor")).into_iter().map(|m| (m.id, m.label)).collect();
        assert_eq!(listed, [("auto".into(), "Auto".into()), ("grok-4.7".into(), "Grok 4.7".into()), ("kimi-k3".into(), "Kimi K3".into())]);
        assert!(Cursor.takes_model() && Cursor.model_command().is_none(), "a running chat isn't switched: /model saves a default");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_app_chat_opens_by_its_link() {
        assert_eq!(chat_link("f39bf436-feba-4354-8eeb-904485dac943"), "cursor://anysphere.cursor-deeplink/agent?id=f39bf436-feba-4354-8eeb-904485dac943");
    }

    #[test]
    fn pages_an_app_chats_conversation() {
        let db = app_fixture();
        let page = app_page(&db, "c1", true, None, 50);
        let kinds: Vec<&str> = page.items.iter().map(|i| i.kind).collect();
        assert_eq!(kinds, ["you", "agent", "tool", "tool"]);
        let run = &page.items[2];
        assert_eq!(run.text, "Run · `cd /Users/me/app && cargo test`");
        assert_eq!((run.input.as_deref(), run.output.as_deref(), run.shell, run.cwd.as_deref()), (Some("cd /Users/me/app && cargo test"), Some("2 failed"), true, Some("/Users/me/app")));
        assert_eq!(page.items[0].at, parse_rfc3339_ms("2026-09-02T23:21:44.459Z").unwrap());
        assert_eq!(page.items.iter().map(|i| i.cursor).collect::<Vec<_>>(), [1, 2, 3, 4]);
        // Still generating: its last message is read again by the next read on.
        assert_eq!((page.before, page.after), (None, 3));
        let newer = app_newer(&db, "c1", true, page.after);
        assert_eq!((newer.items.len(), newer.items[0].cursor, newer.after), (1, 4, 3));
        assert_eq!(app_newer(&db, "c1", false, 3).after, 4);
        // Back from the fourth message, two items at a time: the newest two before it.
        let back = app_page(&db, "c1", false, Some(3), 2);
        assert_eq!(back.items.iter().map(|i| i.kind).collect::<Vec<_>>(), ["agent", "tool"]);
        assert_eq!(back.before, Some(1));
        let first = app_page(&db, "c1", false, back.before, 2);
        assert_eq!((first.items[0].kind, first.before), ("you", None));
        let last = app_page(&db, "c1", false, None, 1);
        assert_eq!((last.items.len(), last.before, last.after), (1, Some(3), 4));
    }

    #[test]
    fn says_each_call_as_a_line() {
        let label = |t: Value| call(&t).label;
        assert_eq!(label(json!({ "name": "read_file_v2", "params": "{\"targetFile\":\"/a/src/main.rs\"}" })), "Read · `main.rs`");
        assert_eq!(label(json!({ "name": "edit_file_v2", "params": "{\"relativeWorkspacePath\":\"/a/src/lib.rs\"}" })), "Edit · `lib.rs`");
        assert_eq!(label(json!({ "name": "ripgrep_raw_search", "params": "{\"pattern\":\"fn main\",\"path\":\"/a\"}" })), "Search · fn main");
        assert_eq!(label(json!({ "name": "web_search", "params": "{\"searchTerm\":\"tauri panels\"}" })), "Search the web · tauri panels");
        let navigate = json!({ "name": "mcp-cursor-ide-browser-browser_navigate", "rawArgs": { "providerIdentifier": "cursor-ide-browser", "toolName": "browser_navigate", "args": { "url": "https://example.com" } } });
        assert_eq!(label(navigate), "cursor-ide-browser · browser_navigate");
        assert_eq!(label(json!({ "name": "mcp--", "rawArgs": {} })), "MCP tool");
        assert_eq!(label(json!({ "name": "set_active_branch" })), "Set active branch");
        assert_eq!(label(json!({ "name": "ask_question", "params": "{\"title\":\"Which database?\"}" })), "Ask · Which database?");
        // An edit is work where it writes; a read isn't.
        assert_eq!(call(&json!({ "name": "edit_file_v2", "params": "{\"relativeWorkspacePath\":\"/a/src/lib.rs\"}" })).work.as_deref(), Some("/a/src/lib.rs"));
        assert_eq!(call(&json!({ "name": "read_file_v2", "params": "{\"targetFile\":\"/b/x.rs\"}" })).work, None);
        // Turned down at its prompt.
        assert!(call(&json!({ "name": "switch_mode", "status": "cancelled", "userDecision": "rejected" })).failed);
        let asked = call(&json!({ "name": "ask_question", "status": "loading", "additionalData": { "status": "pending" } }));
        assert!(asked.open && asked.waits);
    }

    #[test]
    fn reads_the_transcript_both_write() {
        let dir = crate::fixtures::scratch("cursor-transcript");
        let path = dir.join("c.jsonl");
        let lines = [
            json!({ "role": "user", "message": { "content": [{ "type": "text", "text": "<timestamp>Wednesday, Sep 2, 2026, 4:21 PM (UTC-7)</timestamp>\n<user_query>\nFix the build\n</user_query>" }] } }),
            json!({ "role": "assistant", "message": { "content": [{ "type": "text", "text": "Running the tests." }, { "type": "tool_use", "name": "Shell", "input": { "command": "cargo test", "cwd": "/Users/me/app" } }, { "type": "tool_use", "name": "Read", "input": { "path": "/Users/me/app/src/lib.rs" } }] } }),
            json!({ "type": "turn_ended", "status": "success" }),
            json!({ "role": "user", "message": { "content": [{ "type": "text", "text": "<user_query>\nAnd the docs\n</user_query>" }] } }),
        ];
        std::fs::write(&path, lines.iter().map(|l| format!("{l}\n")).collect::<String>()).unwrap();
        let tail = transcript_tail(&path, Some("/Users/me/app"));
        assert_eq!(tail.first_prompt.as_deref(), Some("Fix the build"));
        assert_eq!(tail.last_tool.as_deref(), Some("Read · `lib.rs`"));
        assert!(tail.turn_open, "a prompt after the last turn ended");
        assert_eq!(tail.work, ["/Users/me/app"]);
        let page = crate::transcript::file_page(&path, transcript_items, None, 50).unwrap();
        let kinds: Vec<&str> = page.items.iter().map(|i| i.kind).collect();
        assert_eq!(kinds, ["you", "agent", "tool", "tool", "you"]);
        assert_eq!(page.items[0].text, "Fix the build");
        assert_eq!(page.items[0].at, parse_rfc3339_ms("2026-09-02T16:21:00-07:00").unwrap());
        assert_eq!(page.items[2].text, "Run · `cargo test`");
        assert!(page.items[2].shell);
        assert_eq!(Cursor.prose(lines[0].to_string().as_bytes()).map(|(_, you, t)| (you, t)), Some((true, "Fix the build".to_string())));
        assert_eq!(Cursor.prose(lines[2].to_string().as_bytes()), None);
    }

    /// The command line's transcript keeps a time only for each prompt, the minute it was written
    /// in: what came after it is dated by its prompt, never 0, so a review is read as written after
    /// it was asked for whatever second that was, and the turn before's reply isn't taken for an
    /// answer to a prompt sent later in its minute (`session wait`); the record's last line, when
    /// it was last written, as a turn that gave up hours later ended.
    #[test]
    fn dates_what_the_transcript_keeps_no_time_for() {
        let dir = crate::fixtures::scratch("cursor-times");
        let path = dir.join("c.jsonl");
        let minute = parse_rfc3339_ms("2026-10-28T19:26:00-07:00").unwrap();
        let prompt = json!({ "role": "user", "message": { "content": [{ "type": "text", "text": "<timestamp>Wednesday, Oct 28, 2026, 7:26 PM (UTC-7)</timestamp>\n<user_query>\nReview pull request #7 in kestrel/atlas\n</user_query>" }] } });
        let calls = json!({ "role": "assistant", "message": { "content": [{ "type": "text", "text": "Reading the diff." }, { "type": "tool_use", "name": "Read", "input": { "path": "/Users/me/atlas/pr-7.diff" } }] } });
        let said = |text: &str| json!({ "role": "assistant", "message": { "content": [{ "type": "text", "text": text }] } });
        let write = |lines: &[&Value]| std::fs::write(&path, lines.iter().map(|l| format!("{l}\n")).collect::<String>()).unwrap();
        let s = Session { last_prompt_at: minute, started_at: Some(minute - 3_600_000), ..crate::fixtures::session("cursor:cursor:atlas") };
        let times = |page: &Page| page.items.iter().map(|i| (i.kind, i.at)).collect::<Vec<_>>();
        let read = || crate::transcript::file_page(&path, transcript_items, None, 50).unwrap();
        let reply_after = crate::autoreview::reply_after;

        write(&[&prompt, &calls, &said("Two problems."), &said("Both in `a.rs`."), &json!({ "type": "turn_ended", "status": "success" })]);
        let page = timed(read(), &s, true, Some(minute + 50_000));
        assert_eq!(times(&page), [("you", minute), ("agent", minute), ("tool", minute), ("agent", minute), ("agent", minute + 50_000)]);
        let asked = minute + 36_166;
        assert_eq!(reply_after(&page.items, asked, s.last_prompt_at).as_deref(), Some("Two problems.\n\nBoth in `a.rs`."));
        assert_eq!(reply_after(&page.items, minute + 60_005, s.last_prompt_at), None, "an earlier turn's");
        // The turn before's, written at 19:26:20, while a prompt sent at 19:26:30 hasn't reached
        // the record: nothing of it is dated after that prompt, as `session wait` asks of a reply.
        let before = timed(read(), &s, true, Some(minute + 20_000));
        assert!(before.items.iter().all(|i| i.at < minute + 30_000), "{:?}", times(&before));
        assert_eq!(reply_after(&before.items, minute + 30_000, s.last_prompt_at), None);

        // An error hours after anything else was written.
        let five_hours = minute + 5 * 3_600_000;
        write(&[&prompt, &calls, &json!({ "type": "turn_ended", "status": "error", "error": "Agent turn stopped after repeated resume attempts made no progress" })]);
        assert_eq!(times(&timed(read(), &s, true, Some(five_hours))), [("you", minute), ("agent", minute), ("tool", minute), ("note", five_hours)]);
        assert_eq!(times(&timed(read(), &s, false, Some(five_hours))).last(), Some(&("note", minute)), "a page that doesn't end the record");

        // A page without the prompt: the session's last, where the page ends the record; else what
        // the page before its first prompt can only say, the session's start.
        let tail = crate::transcript::file_page(&path, transcript_items, None, 1).unwrap();
        assert_eq!(times(&timed(tail, &s, true, Some(five_hours))), [("note", five_hours)]);
        let calls_only = crate::transcript::file_page(&path, transcript_items, None, 3).unwrap();
        assert_eq!(times(&timed(calls_only.clone(), &s, true, Some(five_hours)))[..2], [("agent", minute), ("tool", minute)]);
        assert_eq!(times(&timed(calls_only, &s, false, None))[..2], [("agent", minute - 3_600_000), ("tool", minute - 3_600_000)]);
    }

    #[test]
    fn says_why_a_turn_stopped_in_cursors_words() {
        let dir = crate::fixtures::scratch("cursor-turn-error");
        let path = dir.join("c.jsonl");
        let prompt = json!({ "role": "user", "message": { "content": [{ "type": "text", "text": "<user_query>\nReview the PR\n</user_query>" }] } });
        let reply = json!({ "role": "assistant", "message": { "content": [{ "type": "text", "text": "Reading the diff." }] } });
        let write = |lines: &[Value]| std::fs::write(&path, lines.iter().map(|l| format!("{l}\n")).collect::<String>()).unwrap();
        write(&[prompt.clone(), reply.clone(), json!({ "type": "turn_ended", "status": "error", "error": "[resource_exhausted] Error" })]);
        let said = "The turn stopped with an error: [resource_exhausted] Error";
        assert_eq!(transcript_tail(&path, None).error.as_deref(), Some(said));
        let page = crate::transcript::file_page(&path, transcript_items, None, 50).unwrap();
        assert_eq!(page.items.last().map(|i| (i.kind, i.text.as_str())), Some(("note", said)));
        // One that says nothing more is still an error; a prompt after it is a new turn.
        write(&[prompt.clone(), reply.clone(), json!({ "type": "turn_ended", "status": "error" })]);
        assert_eq!(transcript_tail(&path, None).error.as_deref(), Some("The turn stopped with an error"));
        write(&[prompt.clone(), json!({ "type": "turn_ended", "status": "error", "error": "x" }), prompt.clone()]);
        assert_eq!(transcript_tail(&path, None).error, None);
        // An aborted turn isn't one.
        write(&[prompt, reply, json!({ "type": "turn_ended", "status": "aborted", "error": "x" })]);
        assert_eq!(transcript_tail(&path, None).error, None);
    }

    #[test]
    fn reads_a_prompts_time_as_the_transcript_says_it() {
        assert_eq!(prompt_time("Wednesday, Sep 2, 2026, 4:21 PM (UTC-7)"), parse_rfc3339_ms("2026-09-02T16:21:00-07:00"));
        assert_eq!(prompt_time("Friday, Oct 9, 2026, 12:05 AM (UTC+5:30)"), parse_rfc3339_ms("2026-10-09T00:05:00+05:30"));
        assert_eq!(prompt_time("Oct 9, 2026, 12:05 PM (UTC)"), parse_rfc3339_ms("2026-10-09T12:05:00Z"));
        assert_eq!(prompt_time("soon"), None);
    }

    #[test]
    fn reads_a_command_line_chats_meta() {
        let text = r#"{"agentId":"a","name":"Fix the build","createdAt":1790000000000,"lastUsedModel":"gpt-5.5"}"#;
        let hex: String = text.bytes().map(|b| format!("{b:02x}")).collect();
        assert_eq!(parse_meta(&hex), Meta { name: Some("Fix the build".into()), created_at: Some(1_790_000_000_000), model: Some("gpt-5.5".into()), ..Meta::default() });
        assert_eq!(parse_meta(text).name.as_deref(), Some("Fix the build"));
        let planned = parse_meta(r#"{"name":"n","mode":"plan","isRunEverything":true}"#);
        assert_eq!((planned.mode.as_deref(), planned.approval.as_deref()), (Some("plan"), Some("unrestricted")));
        assert_eq!(parse_meta(r#"{"approvalMode":"auto-review","isRunEverything":true}"#).approval.as_deref(), Some("auto-review"));
        // A subagent's, as 2026.10.16 writes one under the chat that started it.
        assert!(parse_meta(r#"{"name":"New Agent","subagentInfo":{"parentAgentId":"d1414899","rootParentAgentId":"d1414899"}}"#).subagent);
        assert!(!planned.subagent);
    }

    /// A chat as 2026.08.11 leaves one: its database, the `meta.json` beside it, and its
    /// transcript, here under a folder no lane walks to.
    #[test]
    fn a_command_line_chat_is_in_the_folder_its_meta_json_says() {
        let dir = crate::fixtures::scratch("cursor-sidecar");
        let home = dir.join(".cursor");
        let chat = |hash: &str, id: &str, side: Value| {
            let dir = home.join("chats").join(hash).join(id);
            std::fs::create_dir_all(&dir).unwrap();
            Connection::open(dir.join(STORE_DB)).unwrap().execute_batch("CREATE TABLE meta (key TEXT, value TEXT); INSERT INTO meta VALUES ('0', '{\"name\":\"Hi There\"}');").unwrap();
            std::fs::write(dir.join("meta.json"), side.to_string()).unwrap();
        };
        chat("d16e", "c1", json!({ "schemaVersion": 1, "createdAtMs": 1, "hasConversation": true, "title": "Hi There", "cwd": "/Users/me/projects/atlas-agent-restart-deferral" }));
        chat("d16e", "sub", json!({ "schemaVersion": 1, "createdAtMs": 1, "hasConversation": true, "isSubagent": true, "cwd": "/Users/me/app" }));
        // A subagent's chat with no meta.json, its database saying whose it is (2026.10.16).
        let sub = home.join("chats/d16e/sub2");
        std::fs::create_dir_all(&sub).unwrap();
        Connection::open(sub.join(STORE_DB)).unwrap().execute_batch(r#"CREATE TABLE meta (key TEXT, value TEXT); INSERT INTO meta VALUES ('0', '{"name":"New Agent","subagentInfo":{"parentAgentId":"c1"}}');"#).unwrap();
        let rows = read_cli_chats(&home, 0, &HashSet::new());
        assert_eq!(rows.iter().map(|c| (c.id.as_str(), c.title.as_deref(), c.cwd.as_deref())).collect::<Vec<_>>(), [("c1", Some("Hi There"), Some("/Users/me/projects/atlas-agent-restart-deferral"))]);
        assert_eq!(cli_chat_dir(&home, "c1"), Some(home.join("chats/d16e/c1")));
        assert_eq!(cli_chat_dir(&home, "unwritten"), None);
        assert_eq!(parse_sidecar("{}"), Sidecar::default());
    }

    /// `--resume <id>` in another folder than the chat's makes a new chat there under the same id,
    /// with a database and a transcript of its own: it's one session, the copy written last.
    #[test]
    fn a_chat_resumed_in_another_folder_is_the_copy_written_last() {
        let dir = crate::fixtures::scratch("cursor-copies");
        let home = dir.join(".cursor");
        let at = |secs: u64| std::time::UNIX_EPOCH + Duration::from_secs(1_790_000_000 + secs);
        let aged = |p: &Path, secs: u64| std::fs::File::options().write(true).open(p).unwrap().set_modified(at(secs)).unwrap();
        let copy = |hash: &str, name: &str, cwd: &str, secs: u64| {
            let dir = home.join("chats").join(hash).join("c1");
            std::fs::create_dir_all(&dir).unwrap();
            Connection::open(dir.join(STORE_DB)).unwrap().execute_batch(&format!("CREATE TABLE meta (key TEXT, value TEXT); INSERT INTO meta VALUES ('0', '{{\"name\":\"{name}\"}}');")).unwrap();
            std::fs::write(dir.join("meta.json"), json!({ "cwd": cwd }).to_string()).unwrap();
            aged(&dir.join(STORE_DB), secs);
        };
        let transcript = |cwd: &str, secs: u64| {
            let path = home.join("projects").join(slug(cwd)).join("agent-transcripts/c1/c1.jsonl");
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, "").unwrap();
            aged(&path, secs);
            path
        };
        let read = || read_cli_chats(&home, 0, &HashSet::new()).into_iter().map(|c| (c.title, c.cwd, c.transcript)).collect::<Vec<_>>();
        let row = |title: &str, cwd: &str, transcript: Option<&PathBuf>| (Some(title.to_string()), Some(cwd.to_string()), transcript.cloned());
        // Started in a worktree, then resumed in its main checkout once the worktree had gone.
        let (worktree, checkout) = ("/Users/me/projects/atlas/wt/release", "/Users/me/projects/atlas");
        copy("a1", "Review Atlas PR", worktree, 100);
        transcript(worktree, 100);
        copy("b2", "Review Request", checkout, 200);
        // The new copy's database comes before its transcript, and the old copy's isn't its.
        assert_eq!(read(), [row("Review Request", checkout, None)]);
        let resumed = transcript(checkout, 200);
        assert_eq!(read(), [row("Review Request", checkout, Some(&resumed))]);
        assert_eq!(cli_chat_dir(&home, "c1"), Some(home.join("chats/b2/c1")));
        assert_eq!(cli_transcript(&home, "c1", &read_sidecar(&home.join("chats/b2/c1"))), Some(resumed));
    }

    /// The command line asks about each folder it works in that it doesn't trust before it
    /// writes its chat, and goes by its records' `.workspace-trusted`.
    #[test]
    fn trusts_a_folder_as_its_prompt_does_and_not_the_home_folder() {
        let dir = crate::fixtures::scratch("cursor-trust");
        let root = std::fs::canonicalize(&*dir).unwrap();
        let (home, user) = (root.join("me/.cursor"), root.join("me"));
        let folder = root.join("me/projects/app two");
        std::fs::create_dir_all(&folder).unwrap();
        assert!(trust_folder(&home, &folder, &user).unwrap());
        let marker = home.join("projects").join(slug(&folder.to_string_lossy())).join(TRUSTED);
        let written: Value = serde_json::from_str(&std::fs::read_to_string(&marker).unwrap()).unwrap();
        assert_eq!(written["workspacePath"], folder.to_string_lossy().as_ref());
        assert!(written["trustedAt"].as_str().is_some_and(|t| t.ends_with('Z') && parse_rfc3339_ms(t).is_some()));
        // Once is enough.
        assert!(!trust_folder(&home, &folder, &user).unwrap());
        // By a link too, which the command line, started there, takes for the folder it leads to.
        let link = root.join("me/linked");
        std::os::unix::fs::symlink(&folder, &link).unwrap();
        assert!(trust_folder(&home, &link, &user).unwrap());
        assert!(home.join("projects").join(slug(&link.to_string_lossy())).join(TRUSTED).is_file());
        assert!(trust_folder(&home, &user, &user).is_err());
        assert!(trust_folder(&home, &root, &user).is_err());
        // An account of its own (`CURSOR_CONFIG_DIR=~/.cursor-work cursor-agent`) keeps its
        // records of folders where every other does, unless CURSOR_DATA_DIR says otherwise.
        let env = |set: Option<&'static str>| move |k: &str| (k == "CURSOR_DATA_DIR").then_some(set).flatten().map(String::from);
        assert_eq!(data_dir(env(None), &user), user.join(".cursor"));
        assert_eq!(data_dir(env(Some(" ")), &user), user.join(".cursor"));
        assert_eq!(data_dir(env(Some("/data/cursor")), &user), PathBuf::from("/data/cursor"));
        // Set by the account's own command, as its terminal runs it.
        let you = crate::util::home();
        assert_eq!(assigned("CURSOR_CONFIG_DIR=~/.cursor-work CURSOR_DATA_DIR=\"$HOME/data\" cursor-agent", "CURSOR_DATA_DIR"), Some(you.join("data").to_string_lossy().into_owned()));
        assert_eq!(assigned("CURSOR_CONFIG_DIR=~/.cursor-work cursor-agent", "CURSOR_DATA_DIR"), None);
        assert_eq!(assigned("cursor-agent CURSOR_DATA_DIR=/x", "CURSOR_DATA_DIR"), None);
    }

    /// The board makes a lane for a chat in a folder it hasn't one for only when a reader says a
    /// chat is new or has moved: one known from its process at first, then from its own record.
    #[test]
    fn a_chat_that_moved_folder_needs_placing() {
        let at = |cwd: Option<&str>| Chat { id: "c".into(), cwd: cwd.map(String::from), ..Chat::default() };
        assert!(placed_anew(None, &at(None)));
        assert!(placed_anew(Some(&at(None)), &at(Some("/r/app"))));
        assert!(placed_anew(Some(&at(Some("/r/app"))), &at(Some("/r/app/.cursor/worktrees/sky"))));
        assert!(!placed_anew(Some(&at(Some("/r/app"))), &at(Some("/r/app"))));
        assert!(!placed_anew(Some(&at(Some("/r/app"))), &at(None)));
    }

    #[test]
    fn a_switch_between_modes_of_one_length_shows_once_the_header_says_it() {
        let root = crate::fixtures::scratch("cursor-mode-switch");
        let home = root.join(".cursor");
        std::fs::create_dir_all(&home).unwrap();
        let path = app_db(&home).unwrap();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let fixture = app_fixture();
        fixture.execute_batch("UPDATE cursorDiskKV SET value = json_set(CAST(value AS TEXT), '$.unifiedMode', 'chat') WHERE key = 'composerData:c2'; UPDATE composerHeaders SET value = json_set(value, '$.unifiedMode', 'chat') WHERE composerId = 'c2';").unwrap();
        fixture.execute("VACUUM INTO ?1", [path.to_string_lossy()]).unwrap();
        let account = AccountCfg { name: "cursor".into(), harness: "cursor".into(), home: home.to_string_lossy().into_owned(), command: None, start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false };
        let seen = crate::state::SeenState::default();
        let look = Look { seen: &seen, since: 0, now: 10_000, ps: Default::default() };
        let mut reader = Reader::default();
        let mode = |reader: &Reader| reader.chats["cursor"].iter().find(|c| c.id == "c2").and_then(|c| c.mode.clone());
        reader.reread(&account, &look);
        assert_eq!(mode(&reader).as_deref(), Some("chat"));
        // Ask to Plan leaves composerData as long as it was, so it isn't read again; the header
        // says it a minute later.
        let db = Connection::open(&path).unwrap();
        db.execute_batch("UPDATE cursorDiskKV SET value = json_set(CAST(value AS TEXT), '$.unifiedMode', 'plan') WHERE key = 'composerData:c2';").unwrap();
        reader.reread(&account, &look);
        assert_eq!(mode(&reader).as_deref(), Some("chat"));
        db.execute_batch("UPDATE composerHeaders SET value = json_set(value, '$.unifiedMode', 'plan') WHERE composerId = 'c2';").unwrap();
        reader.reread(&account, &look);
        assert_eq!(mode(&reader).as_deref(), Some("plan"));
        // A header that's caught up with what composerData said leaves it.
        reader.reread(&account, &look);
        assert_eq!(mode(&reader).as_deref(), Some("plan"));
    }

    #[test]
    fn a_chats_mode_is_its_own_then_the_apps_auto_run() {
        let db = app_fixture();
        let modes = json!({ "composerState": { "modes4": [
            { "id": "agent", "name": "Agent", "autoRun": 1, "fullAutoRun": 1, "smartModeAutoRun": 0 },
            { "id": "chat", "name": "Ask", "autoRun": 0 },
            { "id": "debug", "name": "Debug" },
        ] } });
        db.execute("INSERT INTO ItemTable (key, value) VALUES (?1, ?2)", rusqlite::params![APP_SETTINGS, modes.to_string()]).unwrap();
        let app = read_auto_run(&db);
        assert_eq!(app.agent, Some("unrestricted"));
        let said = |c: &Chat, app: &AutoRun| chat_mode(c, app).map(|m| (m.kind, m.label));
        let says = |kind: ModeKind, label: &str| Some((kind, label.to_string()));
        let chat = |mode: Option<&str>, approval: Option<&str>, cli: bool| Chat { mode: mode.map(String::from), approval: approval.map(String::from), cli, ..Chat::default() };
        assert_eq!(said(&chat(Some("agent"), None, false), &app), says(ModeKind::Skips, "Run Everything"));
        // Ask changes nothing, whatever auto-run says; another mode names itself.
        assert_eq!(said(&chat(Some("chat"), None, false), &app), says(ModeKind::ReadOnly, "Ask"));
        assert_eq!(said(&chat(Some("debug"), None, false), &app), says(ModeKind::Skips, "Debug"));
        // A chat's own setting wins, and with no settings the app's chats say nothing.
        assert_eq!(said(&chat(None, Some("ask-every-time"), false), &app), says(ModeKind::Asks, "Ask Every Time"));
        assert_eq!(said(&chat(Some("agent"), None, false), &AutoRun::default()), None);
        // The account's permissions.json wins over the app's settings and the chat's own.
        let home = crate::fixtures::scratch("cursor-permissions");
        assert_eq!(enforced_approval(&home), None);
        std::fs::write(home.join("permissions.json"), r#"{"approvalMode": "manual"}"#).unwrap();
        let enforced = AutoRun { enforced: enforced_approval(&home), ..app.clone() };
        assert_eq!(said(&chat(Some("agent"), Some("unrestricted"), false), &enforced), says(ModeKind::Asks, "Ask Every Time"));
        assert_eq!(said(&chat(Some("default"), None, true), &enforced), says(ModeKind::Edits, "Allowlist"));
        // The command line's: Allowlist unless it says otherwise, `search` is Ask.
        assert_eq!(said(&chat(Some("default"), None, true), &AutoRun::default()), says(ModeKind::Edits, "Allowlist"));
        assert_eq!(said(&chat(Some("auto-run"), None, true), &AutoRun::default()), says(ModeKind::Skips, "Run Everything"));
        assert_eq!(said(&chat(Some("search"), None, true), &AutoRun::default()), says(ModeKind::ReadOnly, "Ask"));
        assert_eq!(said(&chat(Some("default"), Some("auto-review"), true), &AutoRun::default()), says(ModeKind::Auto, "Auto-review"));
    }

    #[test]
    fn finds_a_folder_from_its_slug() {
        assert_eq!(slug("/Users/me/projects/spark-infra/.claude/worktrees/taco"), "Users-me-projects-spark-infra-claude-worktrees-taco");
        let root = crate::fixtures::scratch("cursor-slug");
        for d in ["Users/me/my app/src", "Users/me/my-app-two", "Users/me/my"] {
            std::fs::create_dir_all(root.join(d)).unwrap();
        }
        assert_eq!(unslug(&root, "Users-me-my-app-src"), Some(root.join("Users/me/my app/src")));
        assert_eq!(unslug(&root, "Users-me-my-app-two"), Some(root.join("Users/me/my-app-two")));
        assert_eq!(unslug(&root, "Users-me-my"), Some(root.join("Users/me/my")));
        assert_eq!(unslug(&root, "Users-me-nowhere"), None);
    }

    #[test]
    fn knows_its_processes_by_their_arguments() {
        let bin = "/Users/me/.local/share/cursor-agent/versions/2026.08.11-e8db854/index.js";
        let ps = |rest: &str| format!("cursor-agent --use-system-ca {bin} {rest}");
        let id = "6ea31b00-0a64-4c8c-bfd0-f71170a1bf7f";
        let args = |interactive: bool, chat: Option<&str>, prompt: Option<&str>| Some(Args { interactive, chat: chat.map(String::from), prompt: prompt.map(String::from) });
        assert_eq!(cli_args(&ps("")), args(true, None, None));
        assert_eq!(cli_args(&ps(&format!("--resume {id}"))), args(true, Some(id), None));
        assert_eq!(cli_args(&ps(&format!("--new-session-id={id} --model gpt-5 fix it"))), args(true, Some(id), Some("fix it")));
        // As a start names the files it adds, then the prompt: `ps` writes a space in a folder's
        // path as it does between words, and a skill's prompt starts with a slash.
        let files = "/Users/me/Library/Application Support/Codealong/files";
        assert_eq!(cli_args(&ps(&format!("--new-session-id {id} --add-dir {files} Review pull request #7"))), args(true, Some(id), Some("Review pull request #7")));
        assert_eq!(cli_args(&ps(&format!("--add-dir {files} /review the diff"))), args(true, None, Some("/review the diff")));
        // A worktree's name is no prompt; a start puts the flag after the prompt, with none.
        assert_eq!(cli_args(&ps("--worktree review")), args(true, None, None));
        assert_eq!(cli_args(&ps("-w review --model gpt-5 fix it")), args(true, None, Some("fix it")));
        assert_eq!(cli_args(&ps(&format!("--new-session-id {id} fix it --worktree"))), args(true, Some(id), Some("fix it")));
        assert_eq!(cli_args(&ps("-p --output-format json fix it")), args(false, None, Some("fix it")));
        // A prompt that names a command is still a prompt after its first word.
        assert_eq!(cli_args(&ps("fix the login page")), args(true, None, Some("fix the login page")));
        assert_eq!(cli_args(&ps("login")), None);
        assert_eq!(cli_args(&ps("acp")), None);
        assert_eq!(cli_args(&format!("/Users/me/.local/bin/agent {bin} --resume")), args(true, None, None));
        assert_eq!(cli_args("/opt/homebrew/bin/agent run"), None);
        assert!(is_app("/Applications/Cursor.app/Contents/MacOS/Cursor"));
        assert!(!is_app("/Applications/Cursor.app/Contents/Frameworks/Cursor Helper.app/Contents/MacOS/Cursor Helper --type=gpu-process"));
        assert_eq!(held_chat(&[PathBuf::from("/Users/me/.cursor/chats/9f1c/6ea31b00-0a64/store.db-wal")]), Some("6ea31b00-0a64".into()));
        assert_eq!(held_chat(&[PathBuf::from("/Users/me/.cursor/ai-tracking/ai-code-tracking.db")]), None);
    }

    /// Every chat on this Mac, however old, as the board would show it, and the newest one's
    /// conversation, read from your own Cursor folders without writing to them:
    /// `cargo test cursor_here -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn cursor_here() {
        let home = crate::util::home().join(".cursor");
        let account = AccountCfg { name: "cursor".into(), harness: "cursor".into(), home: home.to_string_lossy().into_owned(), command: None, start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false };
        let started = Instant::now();
        let db = app_db(&home).and_then(|p| crate::harness::open_database(&p)).expect("Cursor's database");
        let mut chats = read(&db);
        chats.retain(|c| c.messages > 0);
        let app_ms = started.elapsed().as_millis();
        // What a read costs when nothing was written since: the list and how long each is.
        let started = Instant::now();
        let listed = read_app_chats(&db, 0, &HashSet::new()).unwrap();
        let lengths = written(&db, &listed.iter().map(|c| c.id.as_str()).collect::<Vec<_>>());
        println!("a read with nothing new: {} µs for {} chats' lengths and models", started.elapsed().as_micros(), lengths.len());
        chats.extend(read_cli_chats(&home, 0, &HashSet::new()).into_iter().map(|c| Chat { cwd: cli_cwd(&c), ..c }));
        let (procs, app_running) = processes(crate::procs::Listing::default().all());
        println!("{} chats ({app_ms} ms for the app's), the app {}running, {} command line processes", chats.len(), if app_running { "" } else { "not " }, procs.len());
        let now = crate::util::now_ms();
        for c in &chats {
            let s = crate::board::cursor_session(&account, c, None, app_running, 0, now);
            println!("{:8} {:3} {:40} {:50} | {} | {:?} {:?}", format!("{:?}", s.status), if c.cli { "cli" } else { "app" }, one_line(&s.title, 40), one_line(&s.cwd, 50), one_line(s.excerpt.as_deref().unwrap_or(""), 60), s.model, s.branch);
        }
        let newest = chats.iter().find(|c| !c.cli).expect("an app chat");
        let started = Instant::now();
        let page = app_page(&db, &newest.id, false, None, 30);
        println!("\n{} items of {:?} in {} ms, before {:?}, after {}", page.items.len(), newest.title, started.elapsed().as_millis(), page.before, page.after);
        for i in &page.items {
            println!("  {:6} {}{}", i.kind, one_line(&i.text, 90), i.output.as_deref().map(|o| format!("  → {}", one_line(o, 40))).unwrap_or_default());
        }
    }

    #[test]
    fn starts_and_resumes_in_a_terminal() {
        let account = AccountCfg { name: "cursor".into(), harness: "cursor".into(), home: "~/.cursor".into(), command: None, start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false };
        let id = "6ea31b00-0a64-4c8c-bfd0-f71170a1bf7f";
        let ask = |prompt: &'static str, model: Option<&'static str>, worktree: bool| crate::harness::StartAsk { prompt, session_id: id, model, effort: None, worktree, images_dir: Path::new("/i"), files_dir: Path::new("/f") };
        let typed = Cursor.start_in_terminal(&account, &ask("-v please", Some("gpt-5"), true));
        assert_eq!(typed.input, format!("cursor-agent --new-session-id {id} --model 'gpt-5' \"${}\" --worktree\n", crate::start::PROMPT_VAR));
        assert_eq!(typed.env, vec![(crate::start::PROMPT_VAR.to_string(), " -v please".to_string())]);
        assert_eq!(typed.key.as_deref(), Some("cursor:cursor:6ea31b00-0a64-4c8c-bfd0-f71170a1bf7f"));
        assert_eq!(Cursor.start_in_terminal(&account, &ask("", None, false)).input, format!("cursor-agent --new-session-id {id}\n"));
        let s = Session::new(Harness::Cursor, "cursor", "abc");
        assert_eq!(Cursor.resume(&account, &s, &ask("", None, false)).unwrap(), ("cursor-agent --resume abc\n".to_string(), Vec::new()));
    }

    #[test]
    fn reads_skills_from_its_folders_and_the_ones_it_shares_and_names_them_with_a_slash() {
        // An account kept elsewhere still reads your home's.
        let account = AccountCfg { name: "cursor".into(), harness: "cursor".into(), home: "~/elsewhere/cursor".into(), command: None, start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false };
        let home = crate::util::home();
        // Commands of the command line's, which follows a link out of a folder only among yours.
        let how = |personal: bool| Some(crate::skills::AsCommands { surface: "cli", contained: !personal });
        let roots: Vec<(PathBuf, _)> = Cursor.account_skills(&account).into_iter().map(|r| (r.dir, r.as_commands)).collect();
        assert_eq!(roots[0], (home.join(".cursor/skills-cursor"), how(false)));
        assert_eq!(roots[1..], [".cursor/skills", ".claude/skills", ".codex/skills", ".agents/skills"].map(|f| (home.join(f), how(true))));
        // The session's own folder's, not those of the folders above it.
        let roots: Vec<(PathBuf, _)> = Cursor.worktree_skills(&[PathBuf::from("/r/app"), PathBuf::from("/r")]).into_iter().map(|r| (r.dir, r.as_commands)).collect();
        assert_eq!(roots, SKILL_FOLDERS.map(|f| (Path::new("/r/app").join(f), how(false))));
        assert_eq!(Cursor.skills_dir(&account), Some(home.join(".cursor/skills")));
        assert_eq!(Cursor.skill_mention("pr-review_2").as_deref(), Some("/pr-review_2"));
        // A name the command line wouldn't find in the prompt.
        assert_eq!(Cursor.skill_mention("figma:use"), None);
        assert_eq!(Cursor.skill_mention("v1.2"), None);
    }

    /// As `GetUserPrivacyMode` answered on 2026-10-13, and the other modes Cursor 3.22.7 knows.
    #[test]
    fn reads_privacy_mode_as_whether_cursor_trains_on_the_account() {
        use crate::training::State;
        let account = AccountCfg { name: "cursor".into(), harness: "cursor".into(), home: "~/.cursor".into(), command: None, start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false };
        let read = |v: Value| privacy_training(&account, &v);
        let off = read(json!({"privacyMode": "PRIVACY_MODE_NO_TRAINING", "accountPrivacyMode": "PRIVACY_MODE_NO_TRAINING"}));
        assert_eq!((off.state, off.can_stop, off.url.as_deref()), (State::Off, false, Some(PRIVACY_PAGE)));
        assert_eq!(read(json!({"privacyMode": "PRIVACY_MODE_NO_STORAGE"})).state, State::Off);
        let on = read(json!({"privacyMode": "PRIVACY_MODE_USAGE_CODEBASE_TRAINING_ALLOWED"}));
        assert_eq!((on.state, on.can_stop), (State::On, true));
        assert_eq!(read(json!({})).state, State::Unset, "a mode left out is unspecified");
        let team = read(json!({"privacyMode": "PRIVACY_MODE_USAGE_DATA_TRAINING_ALLOWED", "isEnforcedByTeam": true}));
        assert_eq!((team.state, team.can_stop), (State::On, false));
        assert!(team.note.ends_with("Your team sets it."));
    }

    /// As `GetCurrentPeriodUsage` answered a Pro+ account on 2026-10-14, and the other shapes
    /// Cursor 3.22.7's usage line reads.
    #[test]
    fn reads_the_billing_periods_usage_as_meters() {
        use crate::model::Severity;
        let pro_plus = json!({
            "billingCycleStart": "1790621503000", "billingCycleEnd": "1793213503000",
            "planUsage": { "totalSpend": 138, "includedSpend": 138, "remaining": 6862, "limit": 7000, "autoPercentUsed": 0.05833333333333334, "apiPercentUsed": 0.6181818181818182, "totalPercentUsed": 0.10534351145038168 },
            "spendLimitUsage": { "limitType": "user" }, "enabled": true
        });
        use UsageDisplay::*;
        let shown = |v: &Value, d| usage_meters(v, d).iter().map(|m| (m.key.clone(), (m.percent * 100.0).round() / 100.0, m.resets_at, m.model.clone())).collect::<Vec<_>>();
        // A plan that spills over has a limit for Cursor's own models and one for the rest.
        let cursor = ("model:Cursor".to_string(), 0.06, Some(1_793_213_503_000), Some("Cursor".to_string()));
        assert_eq!(shown(&pro_plus, Split), [cursor.clone(), ("model:Other".into(), 0.62, Some(1_793_213_503_000), Some("Other".into()))]);
        assert_eq!(shown(&pro_plus, CursorModelsOnly), [cursor]);
        // Any other has one, the total Settings shows, though the answer has the shares too.
        assert_eq!(shown(&pro_plus, Total), [("month".into(), 0.11, Some(1_793_213_503_000), None)]);
        // Without Cursor's total, what's spent of what the plan includes.
        let month = usage_meters(&json!({ "planUsage": { "includedSpend": 1800, "limit": 2000 } }), Total);
        assert_eq!((month.len(), month[0].percent.round(), month[0].severity), (1, 90.0, Severity::Critical));
        // On-demand spend counts once you've set a limit on it.
        let spend = json!({ "planUsage": { "limit": 2000 }, "spendLimitUsage": { "individualLimit": 5000, "individualUsed": 1250 } });
        assert_eq!(usage_meters(&spend, Total).iter().map(|m| (m.key.as_str(), m.percent)).collect::<Vec<_>>(), [("month", 0.0), ("spend", 25.0)]);
        // A plan with nothing to say says nothing.
        assert!(usage_meters(&json!({ "enabled": false }), Split).is_empty());
        assert!(usage_meters(&json!({ "planUsage": {} }), Total).is_empty());
        // Which it is, as the app's settings say.
        assert_eq!(display_of(Some("pro_plus"), true, false), Split);
        assert_eq!(display_of(Some("enterprise"), true, false), Total);
        assert_eq!(display_of(Some("enterprise"), false, true), Split);
        assert_eq!(display_of(Some("express"), true, false), CursorModelsOnly);
        assert_eq!(display_of(Some("pro"), false, false), Total);
        assert_eq!(display_of(None, false, false), Total);
    }

    /// A folder the app doesn't use (not `~/.cursor`) has the command line's sign-in, from its
    /// file when it keeps one.
    #[test]
    fn a_command_line_sign_in_kept_in_a_file_is_read() {
        let dir = std::env::temp_dir().join(format!("codealong-cursor-auth-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("auth.json"), r#"{"accessToken": " tok ", "refreshToken": "r", "apiKey": null}"#).unwrap();
        assert_eq!(api_token(&dir).as_deref(), Some("tok"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
