//! Antigravity (2.x): the hub app and its command line, `agy`, are one sign-in with a folder
//! each under `~/.gemini` (`antigravity`, `antigravity-cli`), laid out alike.
//! `conversation_summaries.db` lists a folder's conversations; `conversations/<id>.db` holds
//! one's steps; `brain/<id>/.system_generated/logs/transcript_full.jsonl` is the same steps as
//! JSON lines, which is what a panel reads; and a command line process keeps
//! `presence/<id>.lock` open while it has a conversation, and its log, where a terminal UI says
//! what only it knows of a turn (`Said`). See .claude/rules/antigravity.md.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use rusqlite::types::Value as Sql;
use rusqlite::Connection;
use serde_json::Value;

use crate::config::AccountCfg;
use crate::harness::{Applied, Ctx, Look, Question};
use crate::launch::Key;
use crate::model::{Harness, ModeKind, Session, SessionMode, Surface};
use crate::procs;
use crate::transcript::{clip_input, clip_output, item, kept_whole, Item, Page};
use crate::util::{code, command_dir, file_name, note_work, one_line, only_looks, parse_rfc3339_ms};

/// The hub app's folder and the command line's, beside each other.
const APP_DIR: &str = "antigravity";
const CLI_DIR: &str = "antigravity-cli";
const SUMMARIES: &str = "conversation_summaries.db";
/// Where a command line conversation without a workspace runs its commands.
const SCRATCH: &str = "scratch";
/// The command line as its installer names it.
pub const CLI: &str = "agy";
/// The hub app's bundle ID.
pub const APP_BUNDLE_ID: &str = "com.google.antigravity";
/// `CORTEX_STEP_STATUS_WAITING` in a conversation's `steps` table: the step's permission prompt
/// is up (read from `agy` 1.2.14's own definitions).
const STEP_WAITING: i64 = 9;
/// The statuses of a step that isn't over, from the same definitions: pending, running,
/// generating, waiting and queued. A reply is generating from its first words; while the model
/// thinks before them, no step is under way.
const STEP_UNDER_WAY: [i64; 5] = [1, 2, 8, STEP_WAITING, 11];
/// `CORTEX_STEP_STATUS_CANCELED`: a call stopped by Escape or denied in a print run, which
/// ends its turn. A call that fails (7) doesn't: its error goes back to the model, which goes on
/// (every one of 19 on this Mac, 2026-10-14).
const STEP_CANCELED: i64 = 6;
/// `CORTEX_STEP_STATUS_DONE`.
const STEP_DONE: i64 = 3;
/// The step types that end a turn when they're a conversation's latest step and done: the
/// model's reply, and the note a turn that failed ends with ("Agent execution terminated due
/// to error"; seen on 1.3.5). A reply that makes calls is done in the same write that adds its
/// first call's step, so it's never the latest while done (watched every 4 ms on 1.3.5). Not
/// every such note ends one (`ends_turn`).
const STEP_ENDS_TURN: [i64; 2] = [15, STEP_ERROR];
/// An error's note, `CORTEX_STEP_TYPE_ERROR_MESSAGE` in the transcript.
const STEP_ERROR: i64 = 17;
/// Where an error note's payload says what went wrong in words for you. The note a failed turn
/// ends with has them ("Agent execution terminated due to error."); the notes the agent goes on
/// after carry only the error: a request it tries again ("API error (attempt 1): request
/// failed: …", one a try, eight in a row on 2026-10-14) and a stream cut off that it picks up
/// ("The stream was interrupted. Please continue…"). Every note on this Mac, 1.3.5.
const ERROR_FOR_YOU: [u64; 3] = [24, 3, 1];
/// How much of a transcript's end is read for what a conversation is on.
const TAIL: u64 = 256 * 1024;

/// One of an account's folders of conversations.
#[derive(Debug, Clone, PartialEq)]
pub struct Folder {
    pub path: PathBuf,
    /// The command line's, whose conversations run in `agy` processes.
    pub cli: bool,
}

/// The folders an account's conversations are in: its own, and with the hub app's folder the
/// command line's beside it, which the same sign-in uses. An account kept in the command
/// line's folder has only that.
pub fn folders(home: &Path) -> Vec<Folder> {
    let name = file_name(home);
    if name == CLI_DIR {
        return vec![Folder { path: home.to_path_buf(), cli: true }];
    }
    let mut out = vec![Folder { path: home.to_path_buf(), cli: false }];
    if let Some(parent) = home.parent().filter(|_| name == APP_DIR) {
        out.push(Folder { path: parent.join(CLI_DIR), cli: true });
    }
    out
}

/// The command line's settings: what its `/config` saves, the permissions you allowed for good,
/// and the folders it trusts. None when the account has no command line folder: it has never
/// run.
fn cli_settings(home: &Path) -> Option<PathBuf> {
    folders(home).into_iter().find(|f| f.cli && f.path.is_dir()).map(|f| f.path.join("settings.json"))
}

/// The folders the command line trusts, in its settings. Each is the very folder it started in:
/// one under a trusted folder, or in a trusted repo, is asked about again, and `--add-dir`
/// folders aren't asked about at all (1.3.4, tried in a home of its own on 2026-10-13). The
/// folder is the shell's `PWD`, cleaned of `.`, `..` and a trailing slash but with its links
/// kept, or the real path when there's no `PWD`; Yes at its prompt adds that, as it is. A print
/// run (`-p`) never asks.
const TRUSTED_WORKSPACES: &str = "trustedWorkspaces";

/// Trusts `folder` for the command line, as Yes at its trust prompt does: by the path it's
/// started with, and by its real path too, which it goes by when the shell has no `PWD`. False
/// when it trusts both already, and then the file isn't touched. Not the home folder or one
/// above it, which it would then read and run things in unasked wherever you started it.
pub fn trust_folder(settings: &Path, folder: &Path, user_home: &Path) -> anyhow::Result<bool> {
    if user_home.starts_with(folder) {
        anyhow::bail!("{} holds the home folder", folder.display());
    }
    let given = folder.to_str().map(|p| p.trim_end_matches('/').to_string()).filter(|p| !p.is_empty()).ok_or_else(|| anyhow::anyhow!("{} isn't a folder Antigravity can keep", folder.display()))?;
    let real = std::fs::canonicalize(folder).ok().and_then(|p| p.to_str().map(String::from));
    let paths: Vec<String> = std::iter::once(given.clone()).chain(real.filter(|r| *r != given)).collect();
    // Two starts at once would each write back the list they read, losing one folder. agy takes
    // no lock of its own on the file, so only Codealong's writes wait for each other.
    static WRITING: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _writing = WRITING.lock().unwrap_or_else(|e| e.into_inner());
    let text = match std::fs::read_to_string(settings) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => "{}\n".to_string(),
        Err(e) => return Err(e.into()),
    };
    let Some(next) = with_trusted(&text, &paths)? else { return Ok(false) };
    crate::util::write_whole(settings, &next)?;
    Ok(true)
}

/// The command line's settings, `text`, with `paths` added to the folders it trusts, or None
/// when they're all there. The rest keeps its text and its order; the command line writes the
/// file indented by two spaces, as serde_json's pretty printer does, and so the list too.
fn with_trusted(text: &str, paths: &[String]) -> anyhow::Result<Option<String>> {
    use crate::jsonedit::Node;
    let mut top = Node::entries(text)?;
    let at = top.iter().rposition(|(k, _)| k == TRUSTED_WORKSPACES);
    let mut trusted: Vec<String> = match at.map(|i| &top[i].1) {
        Some(Node::Raw(raw)) if raw.get() != "null" => serde_json::from_str(raw.get())?,
        _ => Vec::new(),
    };
    let before = trusted.len();
    for p in paths {
        if !trusted.contains(p) {
            trusted.push(p.clone());
        }
    }
    if trusted.len() == before {
        return Ok(None);
    }
    // Indented as a value one level in.
    let list = serde_json::to_string_pretty(&trusted)?.replace('\n', "\n  ");
    let list = Node::Raw(serde_json::value::RawValue::from_string(list)?);
    match at {
        Some(i) => top[i].1 = list,
        None => top.push((TRUSTED_WORKSPACES.to_string(), list)),
    }
    let mut out = serde_json::to_string_pretty(&Node::Object(top))?;
    if text.ends_with('\n') {
        out.push('\n');
    }
    Ok(Some(out))
}

/// The command that runs an account's command line: its own, when config.toml gives one.
pub fn cli_command(account: &AccountCfg) -> &str {
    account.command.as_deref().unwrap_or(CLI)
}

/// Whether `agy` is installed for the user whose home is `home`: where its installer puts it,
/// or, for you, anywhere on your PATH.
pub fn cli_installed(home: &Path) -> bool {
    crate::detect::is_executable(&home.join(".local/bin").join(CLI)) || (home == crate::util::home() && crate::detect::on_user_path(CLI))
}

/// Whether an account has a command line to run: its own command, when config.toml gives one,
/// or `agy` installed.
fn has_cli(account: &AccountCfg, home: &Path) -> bool {
    account.command.is_some() || cli_installed(home)
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Conversation {
    pub id: String,
    pub title: Option<String>,
    /// The summary's preview text: the conversation's opening, as the app lists it.
    pub preview: Option<String>,
    pub step_count: i64,
    /// The workspace folder it works in, as a path (`work_folder`).
    pub cwd: Option<String>,
    pub workspaces: Vec<String>,
    /// The summary's status, lowercased: `cascade_run_status_idle`, `_running`, `_busy` or
    /// `_canceling`.
    pub status: String,
    pub source: String,
    /// The agent is still doing something.
    pub not_fully_idle: bool,
    pub killed: bool,
    pub last_modified: i64,
    /// When you last wrote to it.
    pub last_input: Option<i64>,
    /// The folder it's kept in.
    pub dir: PathBuf,
    /// It's the command line's: an `agy` process runs it, in a terminal or on its own.
    pub cli: bool,
    /// What the end of its record says.
    pub tail: Tail,
    /// A step's permission prompt is up, as its own database says.
    pub waiting: bool,
    /// A step runs, is being written or waits on you, as its own database says.
    pub step_under_way: bool,
    /// Its latest step ends a turn (`STEP_ENDS_TURN`, `STEP_CANCELED`), as its own database says.
    pub turn_ended: bool,
    /// When its latest step began, as its own database says: later than the summary's time when
    /// another `agy` wrote back an older copy of the row (`STEP_BEGAN`).
    pub latest_step_at: Option<i64>,
    /// Its summary says idle only because another `agy` started, while the process that has it
    /// goes on with its turn (`idled_by_another`).
    pub idled_by_another: bool,
    /// The permissions its latest turn ran with (`turn_mode`).
    pub mode: Option<SessionMode>,
    /// Its latest turn ended on an error (`turn_failed`).
    pub failed: Option<Failed>,
    /// The model its latest turn ran on, as `--model` takes it (`turn_model`).
    pub model: Option<String>,
    /// What the log of the terminal UI that has it open says (`Said`).
    pub said: Said,
}

impl Conversation {
    /// Its summary says a turn is under way.
    pub fn running(&self) -> bool {
        self.not_fully_idle || ["running", "busy", "canceling"].iter().any(|w| self.status.ends_with(w))
    }

    /// The log of the terminal UI that has it open says a turn is under way: the UI holds
    /// messages until the turn ends, or it started a turn it hasn't stopped and that no step of
    /// the conversation's has ended. Neither the summary, which another `agy` writes back idle,
    /// nor a step, of which none is under way while the model thinks, says so for certain.
    pub fn ui_turn_open(&self) -> bool {
        !self.said.queued.is_empty() || (self.said.started(&self.id) && !self.turn_ended)
    }
}

/// Whether a conversation's summary says idle only because another `agy` started, from its row
/// as last read and now. Each `agy`, print runs too (the usage read among them), writes every
/// conversation it doesn't run itself back to the command line's table as idle when it starts,
/// from its own copy of the row (`jetbox_summaries_proto.pb`), which can be far older than the
/// table: its time and step count go back. The process that runs a conversation goes on with
/// its turn, writing the row as a turn starts, a step waits on you, a command starts and a turn
/// ends, and an ending writes the time and count of the latest step, later than any the turn
/// wrote before. So an idle row no newer than the one read while the turn was under way, by
/// the row or by a step that ran, is another's doing, until the row is written again or the
/// conversation's latest step ends the turn: an older copy can land on the ending's row before
/// it's read. Escape while the model thinks writes no step, only the ending's row, and an older
/// copy can land on that one too: only the terminal UI's log says it ended (`Said::stopped`),
/// which the reader asks before it takes an idle row for another's doing.
pub fn idled_by_another(before: Option<&Conversation>, now: &Conversation) -> bool {
    !now.running() && !now.turn_ended && before.is_some_and(|b| (b.running() || b.idled_by_another || b.step_under_way) && now.last_modified <= b.last_modified && now.step_count <= b.step_count)
}

/// A folder's conversations changed since `since_ms`, plus any in `keep` (pinned ones), newest
/// first, without what the end of each one's record says (`with_tails`). Nested ones (subagents,
/// side questions) are left out.
pub fn read_conversations(folder: &Folder, since_ms: i64, keep: &HashSet<String>) -> Vec<Conversation> {
    let Some(db) = crate::harness::open_database(&folder.path.join(SUMMARIES)) else { return Vec::new() };
    let mut rows = read_from(&db, &file_name(&folder.path), since_ms, keep).unwrap_or_default();
    for c in &mut rows {
        c.dir = folder.path.clone();
        c.cli = folder.cli;
        // Without a workspace, the command line's works in its own scratch folder.
        if c.cwd.is_none() && folder.cli {
            c.cwd = Some(folder.path.join(SCRATCH).to_string_lossy().into_owned());
        }
    }
    rows
}

/// The summary table's rows for the folder named `dir`. Each folder's table also lists the
/// other folders' conversations (`app_data_dir` says whose each is), without their status.
fn read_from(db: &Connection, dir: &str, since_ms: i64, keep: &HashSet<String>) -> rusqlite::Result<Vec<Conversation>> {
    const COLUMNS: &str = "conversation_id, title, preview, step_count, last_modified_time, workspace_uris, status, source, \
         parent_conversation_id, nesting_depth, not_fully_idle, killed, last_user_input_time";
    // Older tables have no `app_data_dir`: every row is the folder's own.
    let mut stmt = db
        .prepare(&format!("SELECT {COLUMNS}, app_data_dir FROM conversation_summaries"))
        .or_else(|_| db.prepare(&format!("SELECT {COLUMNS}, '' FROM conversation_summaries")))?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, Option<String>>(1)?,
            r.get::<_, Option<String>>(2)?,
            r.get::<_, Option<i64>>(3)?.unwrap_or(0),
            r.get::<_, Sql>(4)?,
            r.get::<_, Option<String>>(5)?,
            r.get::<_, Option<String>>(6)?,
            r.get::<_, Option<String>>(7)?,
            r.get::<_, Option<String>>(8)?,
            r.get::<_, Option<i64>>(9)?.unwrap_or(0),
            r.get::<_, Sql>(10)?,
            r.get::<_, Sql>(11)?,
            r.get::<_, Sql>(12)?,
            r.get::<_, Option<String>>(13)?.unwrap_or_default(),
        ))
    })?;
    let mut out = Vec::new();
    for (id, title, preview, step_count, modified, uris, status, source, parent, depth, busy, killed, input, owner) in rows.flatten() {
        if depth > 0 || parent.map(|p| !p.is_empty()).unwrap_or(false) {
            continue;
        }
        if !owner.is_empty() && owner != dir {
            continue;
        }
        let last_modified = timestamp(&modified).unwrap_or(0);
        if last_modified < since_ms && !keep.contains(&id) {
            continue;
        }
        let workspaces: Vec<String> = uris.as_deref().and_then(|u| serde_json::from_str::<Vec<String>>(u).ok()).unwrap_or_default().iter().filter_map(|u| uri_path(u)).collect();
        out.push(Conversation {
            cwd: work_folder(&workspaces, &[crate::attachments::images_dir(), crate::attachments::files_dir()]),
            workspaces,
            id,
            title: title.map(|t| t.trim().to_string()).filter(|t| !t.is_empty()),
            preview: preview.map(|t| t.trim().to_string()).filter(|t| !t.is_empty()),
            step_count,
            status: status.unwrap_or_default().trim().to_lowercase(),
            source: source.unwrap_or_default(),
            not_fully_idle: truthy(&busy),
            killed: truthy(&killed),
            last_modified,
            last_input: timestamp(&input),
            ..Default::default()
        });
    }
    out.sort_by(|a, b| b.last_modified.cmp(&a.last_modified));
    Ok(out)
}

/// The folder a conversation works in, from its row's workspaces. The row doesn't keep the
/// order `agy` took them in: one started in a repo with Codealong's images folder added
/// (`--add-dir`, for a prompt's image) lists the images folder first, though its log names the
/// repo first (`workspaceDirs=[…]`; 1.3.5, 2026-10-14). Codealong's attachments folders are
/// never where the work is, so they're passed over unless there's nothing else.
fn work_folder(workspaces: &[String], attachments: &[PathBuf]) -> Option<String> {
    workspaces.iter().find(|w| !attachments.iter().any(|a| Path::new(w) == a)).or(workspaces.first()).cloned()
}

/// A gorm `datetime` column: text like `2026-09-24 14:16:07.434615-07:00`, or seconds or
/// milliseconds since the epoch. The year 1 is gorm's "never".
fn timestamp(v: &Sql) -> Option<i64> {
    match v {
        Sql::Text(t) => parse_rfc3339_ms(t.trim()).filter(|ms| *ms > 0),
        Sql::Integer(n) if *n > 100_000_000_000 => Some(*n),
        Sql::Integer(n) if *n > 0 => Some(n * 1000),
        Sql::Real(f) if *f > 100_000_000_000.0 => Some(*f as i64),
        Sql::Real(f) if *f > 0.0 => Some((f * 1000.0) as i64),
        _ => None,
    }
}

/// gorm's `numeric` booleans: 1/0, "true"/"false", or "1"/"0".
fn truthy(v: &Sql) -> bool {
    match v {
        Sql::Integer(n) => *n != 0,
        Sql::Real(f) => *f != 0.0,
        Sql::Text(t) => matches!(t.trim().to_lowercase().as_str(), "true" | "1" | "t"),
        _ => false,
    }
}

/// `file:///Users/me/My%20Repo` → `/Users/me/My Repo`. Other URIs (remote workspaces) aren't
/// folders here.
pub fn uri_path(uri: &str) -> Option<String> {
    let rest = uri.strip_prefix("file://")?;
    let path = rest.trim_start_matches("localhost");
    let path = if path.starts_with('/') { path } else { return None };
    let decoded = urlencoding::decode(path).ok()?.into_owned();
    Some(decoded.trim_end_matches('/').to_string()).filter(|p| !p.is_empty())
}

/// Where a step's payload keeps when it began: seconds, then nanoseconds (`created_at` in the
/// transcript; every step on this Mac, 1.3.5).
const STEP_BEGAN: [u64; 2] = [5, 1];

/// Whether a step of the conversation is under way, whether one waits on you, whether its
/// latest step ends a turn, and when that step began, from its own database.
fn steps_now(dir: &Path, id: &str) -> (bool, bool, bool, Option<i64>) {
    let Some(db) = crate::harness::open_database(&dir.join("conversations").join(format!("{id}.db"))) else { return (false, false, false, None) };
    let under_way = STEP_UNDER_WAY.map(|s| s.to_string()).join(", ");
    let Ok(mut query) = db.prepare(&format!("SELECT DISTINCT status FROM steps WHERE status IN ({under_way})")) else { return (false, false, false, None) };
    let statuses: Vec<i64> = query.query_map([], |r| r.get(0)).map(|rows| rows.flatten().collect()).unwrap_or_default();
    let latest: Option<(i64, i64, i64)> = db.query_row("SELECT idx, step_type, status FROM steps ORDER BY idx DESC LIMIT 1", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).ok();
    let payload = latest.and_then(|(idx, ..)| db.query_row("SELECT step_payload FROM steps WHERE idx = ?1", [idx], |r| r.get::<_, Option<Vec<u8>>>(0)).ok().flatten());
    let ended = latest.is_some_and(|(_, kind, status)| status == STEP_CANCELED || (status == STEP_DONE && STEP_ENDS_TURN.contains(&kind) && (kind != STEP_ERROR || ends_turn(payload.as_deref()))));
    (!statuses.is_empty(), statuses.contains(&STEP_WAITING), ended, payload.as_deref().and_then(step_began))
}

/// When a step began, from its payload (`STEP_BEGAN`), as epoch milliseconds.
fn step_began(payload: &[u8]) -> Option<i64> {
    let ints = |field: u64| proto_ints(payload, &[STEP_BEGAN[0], STEP_BEGAN[1], field]).first().copied();
    let secs = i64::try_from(ints(1)?).ok()?;
    Some(secs * 1000 + ints(2).map_or(0, |nanos| (nanos / 1_000_000) as i64))
}

/// Whether an error note ends its turn, by its payload (`ERROR_FOR_YOU`). One that can't be
/// read is taken to.
fn ends_turn(payload: Option<&[u8]>) -> bool {
    payload.map_or(true, |p| !proto_at(p, &ERROR_FOR_YOU).is_empty())
}

/// Where an error note's payload has the error as the agent had it ("request failed: Post …:
/// write: broken pipe", "RESOURCE_EXHAUSTED (code 429): Individual quota reached…"), and
/// Google's details of it, one `google.rpc` detail as JSON each (`ErrorInfo` with its `reason`
/// and `metadata`, `RetryInfo`). Beside them, 24.3.7 is the HTTP status (2 for a request that got
/// none), 24.3.3 the error with its stack and 24.3.5 the response's body. Every note that ended a
/// turn on this Mac, 1.3.5, 2026-10-14 and 15.
const ERROR_SAID: [u64; 3] = [24, 3, 2];
const ERROR_DETAILS: [u64; 3] = [24, 3, 10];

/// The words a failed turn's note has for you when they say only that it failed.
const TERMINATED: &str = "Agent execution terminated due to error.";

/// How a request's error ends when it lost its connection to the API or couldn't make one, in
/// Go's words. Seen ending turns: "write: broken pipe", four turns in three conversations within
/// seven minutes on 2026-10-14, each sent after the agent had waited minutes on a permission
/// prompt or a command, on a connection the network had dropped meanwhile. "dial tcp: lookup …:
/// no such host" was seen too, which `agy` tried again itself; the rest are Go's words for the
/// same kinds of loss. A refused connection is left out, as Claude Code's is
/// (claude/transcript.rs `CONNECTION_LOST`): it doesn't come right by waiting.
const CONNECTION_LOST: &[&str] = &[
    "broken pipe",
    "connection reset by peer",
    "no such host",
    "network is unreachable",
    "network is down",
    "no route to host",
    "i/o timeout",
    "TLS handshake timeout",
    "EOF",
    "http2: client connection lost",
];

/// Why a turn ended on an error.
#[derive(Debug, Clone, PartialEq)]
pub enum FailedOn {
    /// Its request lost its connection to the API, or couldn't make one (`CONNECTION_LOST`).
    Connection,
    /// The account's quota refused it (`ErrorInfo` reason `QUOTA_EXHAUSTED`), until when, where
    /// the note says (`quotaResetTimeStamp`).
    Quota { resets_at: Option<i64> },
    /// Anything else: on 2026-10-15 a 404 for the model ("models/barium-b-agy is not found"),
    /// which the same prompt sent again got past.
    Other,
}

/// A turn that ended on an error, by the note it ended with (`turn_failed`).
#[derive(Debug, Clone, PartialEq)]
pub struct Failed {
    pub on: FailedOn,
    /// When the note was written.
    pub at: i64,
    /// What went wrong, in a line.
    pub said: String,
}

/// Why the turn an error note ended failed, and what went wrong in a line. None for a note that
/// doesn't end its turn (`ends_turn`).
fn failed_of(payload: &[u8]) -> Option<(FailedOn, String)> {
    let text = |path: &[u64]| proto_at(payload, path).first().map(|b| String::from_utf8_lossy(b).trim().to_string()).filter(|t| !t.is_empty());
    let for_you = text(&ERROR_FOR_YOU)?;
    let error = text(&ERROR_SAID).unwrap_or_default();
    let details: Vec<Value> = proto_at(payload, &ERROR_DETAILS).into_iter().filter_map(|d| serde_json::from_slice(d).ok()).collect();
    if let Some(quota) = details.iter().find(|d| d.get("reason").and_then(Value::as_str) == Some("QUOTA_EXHAUSTED")) {
        let resets_at = quota.pointer("/metadata/quotaResetTimeStamp").and_then(Value::as_str).and_then(parse_rfc3339_ms);
        // Its "Resets in 2h14m8s." counts from the note, and the limit says when it lifts.
        let said = for_you.split(" Resets in ").next().unwrap_or(&for_you);
        return Some((FailedOn::Quota { resets_at }, one_line(said, 140)));
    }
    if CONNECTION_LOST.iter().any(|w| error.ends_with(w)) {
        // Go's words for the loss are the error's last part ("broken pipe").
        let cause = error.rsplit(": ").next().unwrap_or(&error);
        return Some((FailedOn::Connection, format!("Its request to the API failed: {cause}")));
    }
    let said = match request_cause(&error) {
        Some(cause) => format!("Its request to the API failed: {cause}"),
        None if for_you == TERMINATED && !error.is_empty() => error,
        None => for_you,
    };
    Some((FailedOn::Other, one_line(&said, 140)))
}

/// What a request that failed before an answer came says went wrong, without the agent's
/// prefixes and the address it went to, which take more than a line: `agent executor error: …
/// request failed: Post "https://…": Post "https://oauth2…/token": dial tcp: lookup …: no such
/// host` is `dial tcp: lookup …: no such host`.
fn request_cause(error: &str) -> Option<&str> {
    let rest = error.split_once("request failed: ")?.1;
    Some(rest.rsplit_once("\": ").map_or(rest, |(_, cause)| cause))
}

/// How the conversation's latest turn failed, when its latest step is the error note it ended
/// with, from its own database; `written` is when the note was written when it doesn't say.
fn turn_failed(dir: &Path, id: &str, written: i64) -> Option<Failed> {
    let db = crate::harness::open_database(&dir.join("conversations").join(format!("{id}.db")))?;
    let (kind, status, payload): (i64, i64, Option<Vec<u8>>) = db.query_row("SELECT step_type, status, step_payload FROM steps ORDER BY idx DESC LIMIT 1", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).ok()?;
    let payload = payload.filter(|_| kind == STEP_ERROR && status == STEP_DONE)?;
    let (on, said) = failed_of(&payload)?;
    Some(Failed { on, at: step_began(&payload).unwrap_or(written), said })
}

/// The steps of a conversation whose permission prompt is up (`STEP_WAITING`), by their index:
/// a call's step is the one its result is written as (`Call::result_step`).
fn waiting_steps(dir: &Path, id: &str) -> Vec<i64> {
    let Some(db) = crate::harness::open_database(&dir.join("conversations").join(format!("{id}.db"))) else { return Vec::new() };
    let Ok(mut query) = db.prepare("SELECT idx FROM steps WHERE status = ?1 ORDER BY idx") else { return Vec::new() };
    query.query_map([STEP_WAITING], |r| r.get(0)).map(|rows| rows.flatten().collect()).unwrap_or_default()
}

/// A step that's your prompt (`CORTEX_STEP_TYPE_USER_INPUT`), which carries the settings its
/// turn runs with.
const STEP_USER_INPUT: i64 = 14;

/// Where a prompt's step (`gemini_coder.Step`, 1.3.4's definitions) keeps its turn's tool
/// settings: `user_input` (19) → `user_config` (12) → `planner_config` (1) → `tool_config` (13).
const TOOL_CONFIG: [u64; 4] = [19, 12, 1, 13];

/// The permissions a conversation's latest turn ran with, from its prompt's step: the tool
/// config's `auto_execution_policy` (8 → 3 → 6), and its `default_grants` (42 → 3), whose asks
/// (3) tell `strict` from `request-review`, the settings' `toolPermission`. `always-proceed`
/// and `--dangerously-skip-permissions` are both EAGER (3). `accept-edits` and `plan` (the
/// settings' `agentMode`, `--mode`, Shift+Tab) aren't written there: the command line's log
/// says them (`SetCycleMode called`), and Codealong doesn't read it.
fn turn_mode(dir: &Path, id: &str) -> Option<SessionMode> {
    let db = crate::harness::open_database(&dir.join("conversations").join(format!("{id}.db")))?;
    let step: Vec<u8> = db.query_row("SELECT step_payload FROM steps WHERE step_type = ?1 ORDER BY idx DESC LIMIT 1", [STEP_USER_INPUT], |r| r.get(0)).ok()?;
    let config = proto_at(&step, &TOOL_CONFIG).into_iter().next()?;
    let policy = proto_ints(config, &[8, 3, 6]).into_iter().next()?;
    let asks_to_read = proto_at(config, &[42, 3]).into_iter().flat_map(|grants| proto_at(grants, &[3])).any(|g| g == b"read_file(/)");
    policy_mode(policy, asks_to_read)
}

/// Where a row of a conversation's `executor_metadata` keeps the model its turn ran on, with its
/// level, as `agy models` lists it and `--model` takes it (`gemini-4-argon-high`): every
/// conversation on this Mac, the command line's and the app's (1.3.5, 2026-10-15). The rows
/// come several a turn, the latest last; `gen_metadata`'s, one a request, name the model
/// without its level.
const TURN_MODEL: [u64; 3] = [10, 1, 28];

/// The model a conversation's latest turn ran on (`TURN_MODEL`).
fn turn_model(dir: &Path, id: &str) -> Option<String> {
    let db = crate::harness::open_database(&dir.join("conversations").join(format!("{id}.db")))?;
    let row: Vec<u8> = db.query_row("SELECT data FROM executor_metadata ORDER BY idx DESC LIMIT 1", [], |r| r.get(0)).ok()?;
    let model = proto_at(&row, &TURN_MODEL).into_iter().next()?;
    std::str::from_utf8(model).ok().map(str::trim).filter(|m| crate::start::valid_model(m)).map(str::to_string)
}

/// A turn's `auto_execution_policy` as a mode: OFF (1), AUTO (2), EAGER (3) or
/// PROCEED_IN_SANDBOX (4), named as the settings name them.
fn policy_mode(policy: u64, asks_to_read: bool) -> Option<SessionMode> {
    let (kind, label, detail) = match policy {
        1 if asks_to_read => (ModeKind::Asks, "Strict", "Asks before it reads or writes files, runs commands or goes on the web."),
        1 => (ModeKind::Asks, "Request review", "Asks before it runs commands or writes files, and before it reads outside its workspace."),
        2 => (ModeKind::Auto, "Auto", "Edits files and runs the commands it takes for safe without asking, and asks about the rest."),
        3 => (ModeKind::Skips, "Always proceed", "Runs commands and edits files without asking (always-proceed, or --dangerously-skip-permissions)."),
        4 => (ModeKind::Edits, "Proceed in sandbox", "Runs commands in its sandbox without asking, and asks before one that would leave it."),
        _ => return None,
    };
    Some(SessionMode::new(kind, label, detail))
}

/// A protobuf message's fields: each number with its varint, or the bytes of a length-delimited
/// one. Stops at a field it can't read.
fn proto_fields(b: &[u8]) -> Vec<(u64, Result<u64, &[u8]>)> {
    fn varint(b: &[u8], i: &mut usize) -> Option<u64> {
        let mut n = 0u64;
        for shift in (0..64).step_by(7) {
            let c = *b.get(*i)?;
            *i += 1;
            n |= u64::from(c & 0x7f) << shift;
            if c < 0x80 {
                return Some(n);
            }
        }
        None
    }
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let Some(key) = varint(b, &mut i) else { break };
        let field = key >> 3;
        match key & 7 {
            0 => match varint(b, &mut i) {
                Some(v) => out.push((field, Ok(v))),
                None => break,
            },
            2 => {
                let Some(len) = varint(b, &mut i).and_then(|l| usize::try_from(l).ok()) else { break };
                let Some(bytes) = i.checked_add(len).and_then(|end| b.get(i..end)) else { break };
                out.push((field, Err(bytes)));
                i += len;
            }
            1 => i += 8,
            5 => i += 4,
            _ => break,
        }
    }
    out
}

/// The length-delimited fields at `path`, each step a field number, every repeat followed.
fn proto_at<'a>(b: &'a [u8], path: &[u64]) -> Vec<&'a [u8]> {
    let mut at = vec![b];
    for &field in path {
        at = at.into_iter().flat_map(|m| proto_fields(m).into_iter().filter_map(move |(f, v)| (f == field).then_some(v).and_then(Result::err))).collect();
    }
    at
}

/// The numbers at `path`: the messages along it, then the last field's varints.
fn proto_ints(b: &[u8], path: &[u64]) -> Vec<u64> {
    let Some((last, along)) = path.split_last() else { return Vec::new() };
    proto_at(b, along).into_iter().flat_map(|m| proto_fields(m).into_iter().filter_map(|(f, v)| (f == *last).then_some(v).and_then(Result::ok))).collect()
}

/* ---- A conversation's record: its steps as JSON lines ---- */

/// A conversation's steps as lines: `transcript_full.jsonl` has each call's arguments as they
/// were, `transcript.jsonl` as strings of JSON, so the first is read when it's there.
pub fn transcript_file(dir: &Path, id: &str) -> Option<PathBuf> {
    let logs = dir.join("brain").join(id).join(".system_generated/logs");
    ["transcript_full.jsonl", "transcript.jsonl"].iter().map(|f| logs.join(f)).find(|p| p.is_file())
}

/// A tool call in a planner's step.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Call {
    /// The step its result is written as: results follow their planner's step, in the calls'
    /// order.
    pub result_step: i64,
    /// The call as a line, e.g. "Run command · `cargo test`".
    pub label: String,
    /// All of it: the command, or the arguments.
    pub input: String,
    /// The command a `run_command` runs, and the folder it runs in.
    pub command: Option<String>,
    pub cwd: Option<String>,
    /// Where it works: a command's folder, or the file it writes. A view, a listing and a search
    /// work nowhere, as an agent's reads of a repo it isn't working in don't.
    pub work: Option<String>,
    /// Its command only looks (`util::only_looks`), which is work only in the conversation's
    /// own folder.
    pub looks: bool,
    /// What an `ask_question` call asks you.
    pub questions: Vec<Question>,
}

impl Call {
    /// What it waits on you for, as a line: the call, or what it asks you.
    pub fn asked(&self) -> String {
        match self.questions.first() {
            Some(q) => format!("Asks you · {}", one_line(&q.question, 160)),
            None => self.label.clone(),
        }
    }
}

/// One line of a transcript.
struct Step<'a> {
    index: i64,
    source: &'a str,
    kind: &'a str,
    status: &'a str,
    at: i64,
    content: &'a str,
    calls: Vec<Call>,
}

fn step(v: &Value) -> Option<Step<'_>> {
    let text = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("");
    let index = v.get("step_index").and_then(Value::as_i64)?;
    let calls = v.get("tool_calls").and_then(Value::as_array).map(|a| a.iter().enumerate().map(|(k, c)| call(c, index + 1 + k as i64)).collect()).unwrap_or_default();
    Some(Step { index, source: text("source"), kind: text("type"), status: text("status"), at: parse_rfc3339_ms(text("created_at")).unwrap_or(0), content: text("content"), calls })
}

/// An argument of a call as text, whichever way its transcript wrote it: as it was, or as a
/// string of JSON.
fn arg(args: &Value, key: &str) -> Option<String> {
    match args.get(key)? {
        Value::String(s) if s.starts_with('"') => Some(serde_json::from_str::<String>(s).unwrap_or_else(|_| s.clone())),
        Value::String(s) => Some(s.clone()),
        Value::Null => None,
        other => Some(other.to_string()),
    }
}

fn call(c: &Value, result_step: i64) -> Call {
    let name = c.get("name").and_then(Value::as_str).unwrap_or("tool");
    let args = c.get("args").cloned().unwrap_or(Value::Null);
    let said = name.replace('_', " ");
    let mut said = said.chars();
    let said = said.next().map(|f| f.to_uppercase().collect::<String>() + said.as_str()).unwrap_or_default();
    let command = arg(&args, "CommandLine").filter(|_| name == "run_command");
    let cwd = arg(&args, "Cwd").filter(|c| !c.is_empty());
    let file = ["TargetFile", "AbsolutePath", "DirectoryPath", "SearchPath", "SearchDirectory"].iter().find_map(|k| arg(&args, k)).filter(|p| p.starts_with('/'));
    let label = match (&command, arg(&args, "toolSummary").filter(|s| !s.trim().is_empty())) {
        (Some(c), _) => format!("{said} · {}", code(&one_line(c, 80))),
        (None, Some(summary)) => format!("{said} · {}", one_line(&summary, 80)),
        (None, None) => match &file {
            Some(f) => format!("{said} · {}", code(&file_name(Path::new(f)))),
            None => said,
        },
    };
    // The arguments without the two that only say what the call is for.
    let rest = || {
        let kept: serde_json::Map<String, Value> = args.as_object().into_iter().flatten().filter(|(k, _)| !matches!(k.as_str(), "toolAction" | "toolSummary")).map(|(k, _)| (k.clone(), Value::String(arg(&args, k).unwrap_or_default()))).collect();
        serde_json::to_string_pretty(&kept).unwrap_or_default()
    };
    let work = match (&command, &cwd) {
        (Some(c), Some(dir)) => command_dir(c, dir),
        _ => arg(&args, "TargetFile").filter(|p| p.starts_with('/')),
    };
    let looks = command.as_deref().is_some_and(only_looks);
    let questions = if name == ASK_QUESTION { questions_of(&args) } else { Vec::new() };
    Call { result_step, label, input: command.clone().unwrap_or_else(rest), command, cwd, work, looks, questions }
}

/// The tool that asks you questions, each with choices to pick from.
const ASK_QUESTION: &str = "ask_question";

/// The questions in an `ask_question` call's arguments: `questions`, each `{question, options,
/// is_multi_select}` with its options as plain labels, and no header or descriptions. The
/// transcript without `_full` writes the list as a string of JSON.
fn questions_of(args: &Value) -> Vec<Question> {
    let listed = match args.get("questions") {
        Some(Value::String(s)) => serde_json::from_str(s).unwrap_or(Value::Null),
        Some(v) => v.clone(),
        None => Value::Null,
    };
    let text = |v: &Value| v.as_str().map(str::trim).filter(|t| !t.is_empty()).map(String::from);
    let Value::Array(listed) = listed else { return Vec::new() };
    listed
        .iter()
        .filter_map(|q| {
            let options = q.get("options").and_then(Value::as_array).map(|o| o.iter().filter_map(|c| text(c).or_else(|| c.get("label").and_then(text))).map(|label| codealong_peer::api::Choice { label, description: None }).collect()).unwrap_or_default();
            Some(Question { question: q.get("question").and_then(text)?, header: None, options, multi_select: q.get("is_multi_select").and_then(Value::as_bool) == Some(true) })
        })
        .collect()
}

/// What its result says for a question you skipped.
const SKIPPED: &str = "User Skipped";

/// Your answers, out of a question call's result: one for each question in its order, `A1:
/// Green`, `A2: Mon, Wed`, your own words as you wrote them (lines and all), and `User Skipped`
/// for one you skipped, which is left out. Each is keyed by its question's text.
fn answers_of(result: &str, questions: &[Question]) -> std::collections::BTreeMap<String, Vec<String>> {
    let mut out = std::collections::BTreeMap::new();
    let Some(mut rest) = result.strip_prefix("A1: ") else { return out };
    for (k, q) in questions.iter().enumerate() {
        let (answer, after) = rest.split_once(&format!("\nA{}: ", k + 2)).unwrap_or((rest, ""));
        let answer = answer.trim_end();
        if answer != SKIPPED && !answer.is_empty() {
            out.insert(q.question.clone(), picks_of(answer, q));
        }
        rest = after;
    }
    out
}

/// An answer as the choices it picked and your own words after them: a question that takes
/// several joins the ones ticked with ", ", in their order, then what you wrote. A label with a
/// comma of its own is still found whole.
fn picks_of(answer: &str, q: &Question) -> Vec<String> {
    if !q.multi_select || q.options.iter().any(|o| o.label == answer) {
        return vec![answer.to_string()];
    }
    let mut out = Vec::new();
    let mut rest = answer;
    for o in &q.options {
        match rest.strip_prefix(o.label.as_str()) {
            Some("") => {
                out.push(o.label.clone());
                rest = "";
            }
            Some(after) if after.starts_with(", ") => {
                out.push(o.label.clone());
                rest = &after[2..];
            }
            _ => {}
        }
    }
    if !rest.is_empty() {
        out.push(rest.to_string());
    }
    out
}

/// A page with your answers on its question calls' results (`answers_of`). A result only
/// numbers them, so they're keyed by its call's questions, found on the page or, for a result
/// read without its call (one that came in after the call was read), in the record.
fn with_answers(path: &Path, mut page: Page) -> Page {
    let mut record: Option<Vec<Call>> = None;
    for i in 0..page.items.len() {
        let Some(mut numbered) = page.items[i].answers.remove(NUMBERED) else { continue };
        let id = page.items[i].id.as_deref();
        let on_page = page.items.iter().find(|c| c.kind == "tool" && c.id.as_deref() == id && !c.questions.is_empty()).map(|c| &c.questions);
        let asked = on_page.or_else(|| record.get_or_insert_with(|| asked_in(path)).iter().find(|c| Some(c.result_step.to_string().as_str()) == id).map(|c| &c.questions));
        let answers = asked.map(|q| answers_of(&numbered.pop().unwrap_or_default(), q)).unwrap_or_default();
        page.items[i].answers = answers;
    }
    page
}

/// Where `conversation_items` keeps a result's whole text when it reads as answers (`A1: …`),
/// for `with_answers` to take apart: what the result shows is clipped, and an answer of your
/// own can be long.
const NUMBERED: &str = "A1";

/// Every question call in a record.
fn asked_in(path: &Path) -> Vec<Call> {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    text.lines().filter(|l| l.contains(ASK_QUESTION)).filter_map(|l| serde_json::from_str::<Value>(l).ok()).flat_map(|v| step(&v).map(|s| s.calls).unwrap_or_default()).filter(|c| !c.questions.is_empty()).collect()
}

/// What you asked, out of a `USER_INPUT` step: Antigravity wraps it in `<USER_REQUEST>` and
/// adds the time and what settings changed after it.
fn request_of(content: &str) -> &str {
    let inner = content.split_once("<USER_REQUEST>").and_then(|(_, rest)| rest.split_once("</USER_REQUEST>")).map(|(asked, _)| asked);
    inner.unwrap_or(content).trim()
}

/// What a tool gave back, out of its result step: the text after the times it starts with, and
/// for a command its output alone when it exited with 0. With whether it failed.
fn result_of(content: &str, status: &str) -> (String, bool) {
    let body: Vec<&str> = content.lines().skip_while(|l| l.starts_with("Created At:") || l.starts_with("Completed At:") || l.trim().is_empty()).collect();
    let body = body.join("\n");
    let code = body.split_once("The command exited with code ").and_then(|(_, rest)| rest.split(|c: char| !c.is_ascii_digit() && c != '-').next()?.parse::<i32>().ok());
    let failed = code.is_some_and(|c| c != 0) || matches!(status, "ERROR" | "CANCELED");
    let text = match (code, body.split_once("Output:\n")) {
        (Some(0), Some((_, out))) => out.to_string(),
        _ => body,
    };
    (text.replace("\r\n", "\n"), failed)
}

/// The steps that are neither yours, the agent's, nor a tool's result: the app's own notes to
/// the model.
const ASIDES: [&str; 6] = ["SYSTEM_MESSAGE", "CHECKPOINT", "EPHEMERAL_MESSAGE", "CONVERSATION_HISTORY", "KNOWLEDGE_ARTIFACTS", "USER_INPUT"];

/// A transcript line's items (transcript.rs): your prompt, the agent's reply and its calls, or
/// a call's result, which is a step of its own. A call has no id in the record, so each gets
/// the number of the step its result is written as.
pub fn conversation_items(v: &Value, _at: i64) -> Vec<Item> {
    let Some(s) = step(v) else { return Vec::new() };
    match (s.source, s.kind) {
        ("USER_EXPLICIT", "USER_INPUT") => {
            let asked = request_of(s.content);
            if asked.is_empty() { Vec::new() } else { vec![item(s.at, "you", asked)] }
        }
        ("MODEL", "PLANNER_RESPONSE") => {
            let mut out = Vec::new();
            if !s.content.trim().is_empty() {
                out.push(item(s.at, "agent", s.content.trim()));
            }
            for c in &s.calls {
                let shell = c.command.as_deref().is_some_and(|cmd| !cmd.trim().is_empty() && kept_whole(cmd));
                out.push(Item { id: Some(c.result_step.to_string()), input: Some(clip_input(&c.input)), shell, cwd: c.cwd.clone().filter(|_| shell), questions: c.questions.clone(), ..item(s.at, "tool", c.label.clone()) });
            }
            out
        }
        (_, "ERROR_MESSAGE") if !s.content.trim().is_empty() => vec![item(s.at, "note", s.content.trim())],
        ("MODEL", kind) if !ASIDES.contains(&kind) => {
            let (output, failed) = result_of(s.content, s.status);
            let answers = if output.starts_with("A1: ") { [(NUMBERED.to_string(), vec![output.clone()])].into() } else { Default::default() };
            vec![Item { id: Some(s.index.to_string()), output: Some(clip_output(&output)), failed, answers, ..item(s.at, "result", "") }]
        }
        _ => Vec::new(),
    }
}

/// What the end of a conversation's record says.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Tail {
    /// The latest reply's start, and its end, for Jev.
    pub last_text: Option<String>,
    pub last_text_tail: Option<String>,
    /// The latest tool call, as a line.
    pub last_tool: Option<String>,
    /// The latest calls with no result yet: what runs now, or waits on you.
    pub open: Vec<Call>,
    /// Where its latest calls worked, oldest first.
    pub work: Vec<String>,
    /// Its latest step isn't a reply that ends a turn: your prompt, a call, or a call's result.
    pub turn_open: bool,
}

/// Reads the end of a transcript. `own` is the conversation's folder, where a command that
/// only looks still counts as work.
pub fn tail(path: &Path, own: Option<&str>) -> Tail {
    // Whole lines only: a cut drops the line it fell in, whatever it fell inside.
    crate::util::read_last(path, TAIL).map(|end| tail_of(&end.text(), own)).unwrap_or_default()
}

fn tail_of(text: &str, own: Option<&str>) -> Tail {
    let values: Vec<Value> = text.lines().filter_map(|l| serde_json::from_str(l).ok()).collect();
    // Lines aren't always in the steps' order: calls made together write their results before
    // their own step.
    let mut steps: Vec<Step> = values.iter().filter_map(step).collect();
    steps.sort_by_key(|s| s.index);
    let written: HashSet<i64> = steps.iter().map(|s| s.index).collect();
    let mut out = Tail::default();
    for s in steps.iter().filter(|s| s.source == "MODEL" && s.kind == "PLANNER_RESPONSE") {
        if !s.content.trim().is_empty() {
            out.last_text = Some(one_line(s.content, 140)).filter(|t| !t.is_empty());
            let chars: Vec<char> = s.content.trim().chars().collect();
            out.last_text_tail = Some(chars[chars.len().saturating_sub(600)..].iter().collect());
        }
        for c in &s.calls {
            out.last_tool = Some(c.label.clone());
            // The agent picks each command's folder: one run somewhere else only to look isn't
            // work there.
            if let Some(w) = c.work.as_ref().filter(|w| !c.looks || own.is_some_and(|own| Path::new(w).starts_with(own))) {
                note_work(&mut out.work, w.clone());
            }
        }
        if !s.calls.is_empty() {
            out.open = s.calls.iter().filter(|c| !written.contains(&c.result_step)).cloned().collect();
        }
    }
    // The app's own notes to the model don't say where a turn stands.
    out.turn_open = steps.iter().rev().find(|s| s.kind == "USER_INPUT" || s.source == "MODEL").is_some_and(|s| s.kind != "PLANNER_RESPONSE" || !s.calls.is_empty());
    out
}

/// The only way in for a conversation in the app: nothing there takes a prompt from outside, so
/// the app comes forward with the prompt on the clipboard to paste.
#[derive(Debug, Clone, PartialEq)]
pub struct OnClipboard;

impl crate::send::Own for OnClipboard {
    fn way(&self) -> crate::send::Way {
        crate::send::Way::Antigravity
    }

    fn takes_agents(&self) -> bool {
        false
    }

    fn leaves_it_to_paste(&self) -> bool {
        true
    }

    fn deliver(&self, d: &crate::send::Delivery) -> anyhow::Result<crate::send::Sent> {
        crate::send::copy(&crate::attachments::expand(d.text, crate::attachments::Style::Path))?;
        crate::launch::open_antigravity()?;
        Ok(crate::send::opened("Antigravity is open and the prompt is on the clipboard. Paste it there to send it.".to_string()))
    }
}

/// What a conversation was first asked, and when, from the start of its record.
fn first_request(path: &Path) -> Option<(String, i64)> {
    let text = crate::util::read_head(path, TAIL as usize)?;
    text.lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()).find_map(|v| {
        let s = step(&v).filter(|s| s.kind == "USER_INPUT")?;
        Some((one_line(request_of(s.content), 140), s.at)).filter(|(asked, _)| !asked.is_empty())
    })
}

/// A conversation a command line process has open that its folder's table doesn't list yet: a
/// print run writes its row only as it ends, though its record is written as it goes (checked
/// on 1.2.14). It runs in the process's folder.
fn started(dirs: &[Folder], p: &Process, now: i64) -> Option<Conversation> {
    let id = p.conversation.as_ref()?;
    let dir = dirs.iter().find(|f| f.cli && f.path.join("presence").join(format!("{id}.lock")).exists())?;
    let record = transcript_file(&dir.path, id);
    let (asked, at) = record.as_deref().and_then(first_request).unzip();
    let cwd = procs::cwd_of(p.pid).map(|c| c.to_string_lossy().into_owned());
    let tail = record.as_deref().map(|p| tail(p, cwd.as_deref())).unwrap_or_default();
    Some(Conversation {
        id: id.clone(),
        preview: asked,
        workspaces: cwd.iter().cloned().collect(),
        cwd,
        status: "cascade_run_status_running".into(),
        not_fully_idle: true,
        last_modified: record.as_deref().and_then(crate::util::mtime_ms).unwrap_or(now),
        last_input: at,
        dir: dir.path.clone(),
        cli: true,
        tail,
        ..Default::default()
    })
}

/* ---- The command line's processes ---- */

/// An `agy` process on this Mac.
#[derive(Debug, Clone, PartialEq)]
pub struct Process {
    pub pid: u32,
    /// The conversation it has open, from the presence lock it holds.
    pub conversation: Option<String>,
    /// It's the interactive UI in a terminal, rather than a print run.
    pub terminal: bool,
    /// The logs it has open: its own among them (`own_log`).
    pub logs: Vec<PathBuf>,
}

/// For a process's command line that's `agy` running a conversation: whether it's the
/// interactive UI rather than a print run. None for its other commands (`models`, `update`…)
/// and for anything else. `ps` joins the arguments with spaces, so a prompt reads as words;
/// only the words before it matter.
pub fn cli_args(command: &str) -> Option<bool> {
    let mut words = command.split_whitespace();
    let program = words.next()?;
    if program.rsplit('/').next() != Some(CLI) {
        return None;
    }
    while let Some(w) = words.next() {
        match w.split('=').next().unwrap_or(w) {
            "-p" | "--print" | "--prompt" | "--input-format" => return Some(false),
            "-i" | "--prompt-interactive" => return Some(true),
            "--add-dir" | "--agent" | "--conversation" | "--effort" | "--json-schema" | "--log-file" | "--mode" | "--model" | "--output-format" | "--print-timeout" | "--project" if !w.contains('=') => {
                words.next();
            }
            _ if w.starts_with('-') => {}
            "agent" | "agents" | "changelog" | "help" | "install" | "mcp" | "mic-serve" | "models" | "plugin" | "plugins" | "remote-control" | "update" => return None,
            // A prompt.
            _ => return Some(true),
        }
    }
    Some(true)
}

/// Whether a process's command line is the hub app's agent, which runs the app's conversations.
fn is_app_agent(command: &str) -> bool {
    let program = command.split(" --").next().unwrap_or(command);
    program.ends_with("/language_server") && program.contains("Antigravity")
}

/// The conversation a command line process has open: the presence lock among its open files
/// (`lsof -Fn`'s `n<path>` lines).
fn held_conversation(open_files: &str) -> Option<String> {
    open_files.lines().filter_map(|l| l.strip_prefix('n')).find_map(|p| {
        let p = Path::new(p);
        (p.parent().map(file_name).as_deref() == Some("presence") && p.extension().and_then(|x| x.to_str()) == Some("lock")).then(|| p.file_stem().map(|s| s.to_string_lossy().into_owned())).flatten()
    })
}

/// The command line's logs a process has open (`log/cli-<time>.log`), from the same list.
fn open_logs(open_files: &str) -> Vec<PathBuf> {
    let mut logs: Vec<PathBuf> = open_files
        .lines()
        .filter_map(|l| l.strip_prefix('n'))
        .map(PathBuf::from)
        .filter(|p| p.parent().map(file_name).as_deref() == Some("log") && log_begun(p).is_some())
        .collect();
    logs.sort();
    logs.dedup();
    logs
}

/// The command line's processes, each with the conversation and the logs it has open, and
/// whether the hub app's agent is running. From the list of every process, and an `lsof` for
/// each `agy`.
pub fn processes(listed: &[procs::PsLine]) -> (Vec<Process>, bool) {
    let mut app = false;
    let mut found = Vec::new();
    for p in listed {
        app |= is_app_agent(&p.command);
        if let Some(interactive) = cli_args(&p.command) {
            let files = procs::run("/usr/sbin/lsof", &["-Fn", "-p", &p.pid.to_string()], None, Duration::from_secs(3)).unwrap_or_default();
            found.push(Process { pid: p.pid, conversation: held_conversation(&files), terminal: interactive && !p.tty.starts_with('?'), logs: open_logs(&files) });
        }
    }
    (found, app)
}

/* ---- A terminal UI's log ---- */

/// Its log's line as it sends a prompt into the conversation it names: one you typed, the
/// queue sent at a turn's end, or the one it started with (`-i`). A turn starts.
const LOG_PROMPTED: &str = "Forwarding user message to conversation ";
/// Escape while a reply is on its way, in the conversation it names: the turn ends, and no
/// step says so when the model was thinking.
const LOG_STOPPED: &str = "Cancelling in-progress response for conversation ";
/// A message you typed and sent with Return, Codealong's typing too. It goes in at once, a
/// `LOG_PROMPTED` line within milliseconds of it, or, while a turn runs, waits in the UI's
/// queue until the turn ends (`Queued Messages` in `/config`, `Queue` by default). Nothing but
/// this line says it came until then: on 2026-10-14 one waited 31 minutes.
const LOG_TYPED: &str = "HandleUserInput called with text: ";
/// The queue is dropped (`Pending inputs cancelled (N regular + M shell dropped)`).
const LOG_DROPPED: &str = "Pending inputs cancelled (";
/// The queue sent at a turn's end didn't go, and is held again.
const LOG_RESTORED: &str = "Auto-flush SendUserMessage failed, restoring ";

/// What a terminal UI's log says of the conversation it has open, as far as it's been read
/// (`LogRead`). Each `agy` writes its own (`log/cli-<time>.log`, its local time to the second
/// in the name) and keeps it open, its first lines naming the process (`own_log`). Lines are
/// glog's, `I1014 17:45:37.928030 <thread> <file>:<line>] <text>`, the time this Mac's
/// without the year. Read on agy 1.3.4 and 1.3.5's logs on this Mac, 2026-10-14.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Said {
    /// The conversation it last sent a prompt into, and when.
    pub prompted: Option<(String, i64)>,
    /// When it last stopped a reply on its way.
    pub stopped: Option<i64>,
    /// When each message it holds until its turn ends came, oldest first.
    pub queued: Vec<i64>,
    /// What it held when it last sent the queue, should the send fail.
    flushed: Vec<i64>,
}

impl Said {
    /// Takes in a line of a log begun in `begun` (`log_time`).
    fn on_line(&mut self, line: &str, begun: (i64, i64)) {
        // glog's own lines only, and an event only at the start of the text after the header: a
        // message typed can quote a line of the log.
        let Some(at) = log_time(line, begun) else { return };
        let Some((_, text)) = line.split_once("] ") else { return };
        if let Some(rest) = text.strip_prefix(LOG_PROMPTED) {
            let id = rest.split_whitespace().next().unwrap_or_default();
            self.prompted = Some((id.to_string(), at));
            self.flushed = std::mem::take(&mut self.queued);
        } else if let Some(typed) = text.strip_prefix(LOG_TYPED) {
            // Text led by `/` may be a command the UI runs itself and never sends in (no log here
            // had one); taken for a message held, it would keep the session working for good.
            if !typed.starts_with("\"/") {
                self.queued.push(at);
            }
        } else if text.starts_with(LOG_STOPPED) {
            self.stopped = Some(at);
        } else if text.starts_with(LOG_DROPPED) {
            self.queued.clear();
        } else if text.starts_with(LOG_RESTORED) {
            let held = std::mem::take(&mut self.queued);
            self.queued = std::mem::take(&mut self.flushed).into_iter().chain(held).collect();
        }
    }

    /// It started a turn in conversation `id` and hasn't stopped it.
    pub fn started(&self, id: &str) -> bool {
        self.prompted.as_ref().is_some_and(|(to, at)| to == id && self.stopped.map_or(true, |s| s < *at))
    }

    /// It stopped the reply its latest prompt started.
    pub fn stopped_last(&self) -> bool {
        self.stopped.is_some_and(|s| self.prompted.as_ref().map_or(true, |(_, at)| s >= *at))
    }
}

/// The year and month a log was begun in, from its name (`cli-20261014_172324.log`).
fn log_begun(path: &Path) -> Option<(i64, i64)> {
    let name = file_name(path);
    let stamp = name.strip_prefix("cli-")?.strip_suffix(".log")?;
    if stamp.len() != 15 || stamp.as_bytes()[8] != b'_' {
        return None;
    }
    Some((stamp.get(..4)?.parse().ok()?, stamp.get(4..6)?.parse().ok()?))
}

/// When a glog line was written, from its prefix (`I1014 17:45:37.928030`), in a log begun in
/// `begun` (year, month): a month before that one is the next year's.
fn log_time(line: &str, begun: (i64, i64)) -> Option<i64> {
    let b = line.as_bytes();
    if b.len() < 21 || !matches!(b[0], b'I' | b'W' | b'E' | b'F') || b[5] != b' ' || b[8] != b':' || b[11] != b':' || b[14] != b'.' {
        return None;
    }
    let num = |from: usize, to: usize| line.get(from..to)?.parse::<i64>().ok();
    let (month, day) = (num(1, 3)?, num(3, 5)?);
    let year = if month < begun.1 { begun.0 + 1 } else { begun.0 };
    crate::util::local_ms(year, month, day, num(6, 8)?, num(9, 11)?, num(12, 14)?, num(15, 18)?)
}

/// A process's own log among those it has open: the one whose first lines name it
/// (`Starting language server process with pid N`). A UI was seen holding another `agy`'s
/// log open beside its own.
fn own_log(logs: &[PathBuf], pid: u32) -> Option<PathBuf> {
    let names = format!("Starting language server process with pid {pid}\n");
    logs.iter().find(|p| crate::util::read_head(p, 8 * 1024).is_some_and(|head| head.contains(&names))).cloned()
}

/// A terminal UI's log, read as it grows.
#[derive(Debug)]
struct LogRead {
    path: PathBuf,
    begun: (i64, i64),
    /// How far it's been read: always the start of a line.
    read_to: u64,
    said: Said,
}

impl LogRead {
    fn new(path: PathBuf) -> Option<LogRead> {
        Some(LogRead { begun: log_begun(&path)?, path, read_to: 0, said: Said::default() })
    }

    /// Reads what was written since the last read, whole lines only. True when what it says
    /// changed.
    fn read_on(&mut self) -> bool {
        let Ok(tail) = crate::util::read_tail(&self.path, self.read_to) else { return false };
        let before = self.said.clone();
        for line in tail.text().lines() {
            self.said.on_line(line, self.begun);
        }
        self.read_to = tail.end;
        self.said != before
    }
}

/* ---- Starting a conversation ---- */

/// How long a list of models is good for, and how soon a read that got none is tried again: one
/// that failed (an `agy` that didn't answer in its 30 seconds, under load) used to leave the
/// menus without Antigravity's models for the hour.
const MODELS_FOR: Duration = Duration::from_secs(60 * 60);
const MODELS_AGAIN: Duration = Duration::from_secs(60);

/// The list of models last read, kept for the next launch: without it the menus had none of
/// Antigravity's models from a launch until its first read answered, seconds and, under load,
/// longer. A cache, which macOS may clear; a copy of the app with a home of its own has its own.
fn kept_models_path() -> PathBuf {
    crate::config::cache_dir().join("agy-models.json")
}

/// The command line's list of models, and when to ask for it again.
#[derive(Debug, Default)]
struct KeptModels {
    list: Vec<crate::start::ModelOption>,
    ask_at: Option<Instant>,
}

impl KeptModels {
    /// Whether to ask now. Once asked, not again for the hour, so a slow answer isn't asked
    /// for twice; `on_answer` says when.
    fn on_wanted(&mut self, now: Instant) -> bool {
        let due = self.ask_at.map_or(true, |at| now >= at);
        if due {
            self.ask_at = Some(now + MODELS_FOR);
        }
        due
    }

    /// What the command line answered: a list, kept for the hour; or none, asked for again in a
    /// minute, the list before it kept meanwhile. True for a list.
    fn on_answer(&mut self, now: Instant, listed: Vec<crate::start::ModelOption>) -> bool {
        if listed.is_empty() {
            self.ask_at = Some(now + MODELS_AGAIN);
            return false;
        }
        self.list = listed;
        self.ask_at = Some(now + MODELS_FOR);
        true
    }
}

/// The models the command line offers (`agy models`), which asks Google for them and takes a
/// second or two: asked off the caller's thread, kept for an hour (`KeptModels`), and on disk
/// for the next launch (`kept_models_path`), which starts with the last list while it asks again.
/// The reader asks as the app starts, so the list is fresh by the time a panel wants it.
fn models(command: &str) -> Vec<crate::start::ModelOption> {
    static KEPT: std::sync::Mutex<Option<KeptModels>> = std::sync::Mutex::new(None);
    let mut lock = KEPT.lock().unwrap_or_else(|e| e.into_inner());
    let kept = lock.get_or_insert_with(|| KeptModels { list: crate::store::read_or_default(&kept_models_path()), ask_at: None });
    if kept.on_wanted(Instant::now()) {
        let command = format!("{command} models");
        std::thread::spawn(move || {
            // Through a shell: an account's command may set variables first.
            let listed = procs::run_background("/bin/sh", &["-c", &command], None, Duration::from_secs(30)).map(|out| listed_models(&out)).unwrap_or_default();
            let mut lock = KEPT.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(kept) = lock.as_mut() {
                if kept.on_answer(Instant::now(), listed) {
                    // Not worth a notice: the next launch asks again.
                    let _ = crate::store::save(&kept_models_path(), &kept.list);
                }
            }
        });
    }
    kept.list.clone()
}

/// `agy models` prints a line a model: what `--model` takes, a tab, and its name.
fn listed_models(out: &str) -> Vec<crate::start::ModelOption> {
    out.lines().filter_map(|l| l.split_once('\t')).map(|(id, label)| (id.trim(), label.trim())).filter(|(id, label)| crate::start::valid_model(id) && !label.is_empty()).map(|(id, label)| crate::start::ModelOption { id: id.to_string(), label: label.to_string() }).collect()
}

/* ---- The sign-in and plan usage ---- */

/// Where `agy` 1.3.4 keeps the sign-in it shares with the app, in the login keychain.
const KEYCHAIN_SERVICE: &str = "gemini";
const KEYCHAIN_ACCOUNT: &str = "antigravity";
/// The token file, beside the folders under ~/.gemini, that the sign-in was before `agy` moved
/// it into the keychain.
const TOKEN_FILE: &str = "jetski-standalone-oauth-token";
/// A locked keychain can hold `security` at a prompt; don't wait on it forever.
const KEYCHAIN_TIMEOUT: Duration = Duration::from_secs(10);
/// A usage read takes two seconds or so. One still going after this is stuck: signed out, `agy`
/// waits for a sign-in in the browser rather than exit.
const USAGE_TIMEOUT: Duration = Duration::from_secs(30);

/// Whether the account kept in `home` is signed in: the keychain holds the sign-in, or the token
/// file is there. Reads no secret.
fn signed_in(home: &Path) -> bool {
    home.parent().is_some_and(|p| p.join(TOKEN_FILE).is_file())
        || procs::run("/usr/bin/security", &["find-generic-password", "-s", KEYCHAIN_SERVICE, "-a", KEYCHAIN_ACCOUNT], None, KEYCHAIN_TIMEOUT).is_ok()
}

/// Where `agy` logs a usage read: one file, written over each time. Left to itself, each run
/// leaves a log of its own in the command line's folder, so a read every couple of minutes would
/// pile them up there.
fn usage_log() -> PathBuf {
    crate::config::logs_dir().join("agy-usage.log")
}

/// An account's plan usage, from the command line's `/usage` in a print run, which answers
/// without taking a turn, spends no quota and leaves no conversation behind (`agy` 1.1.11 on).
/// Blocks for a few seconds; call it off the engine thread.
pub fn read_usage(account: &AccountCfg, now: i64) -> crate::model::Usage {
    let mut u = crate::model::Usage { loading: false, ..crate::usage::loading(account) };
    let cli = cli_command(account);
    if !signed_in(&account.home_path()) {
        u.error = Some(format!("Not signed in: run {} once to sign in", code(cli)));
        u.needs_sign_in = true;
        return u;
    }
    let log = usage_log();
    let _ = std::fs::create_dir_all(log.parent().unwrap_or(Path::new("/tmp")));
    // Through a shell, as `models` asks: an account's command may set variables first.
    let command = format!("{cli} -p /usage --output-format json --log-file {}", crate::util::sh_quote(&log.to_string_lossy()));
    let answer = match procs::run_in("/bin/sh", &["-c", &command], &crate::util::home(), &[], "", USAGE_TIMEOUT) {
        Ok(out) => usage_answer(&out.out),
        Err(_) => UsageAnswer::Failed(format!("{} didn't answer in {} seconds", code(cli), USAGE_TIMEOUT.as_secs())),
    };
    match answer {
        UsageAnswer::Limits(meters) => {
            u.as_of = Some(now);
            if meters.is_empty() {
                u.error = Some("This plan doesn't report usage limits.".to_string());
            }
            u.meters = meters;
        }
        UsageAnswer::SignedOut => {
            u.error = Some(format!("Sign-in expired: run {} once to sign in again", code(cli)));
            u.needs_sign_in = true;
        }
        UsageAnswer::Failed(why) => u.error = Some(format!("Couldn't read usage: {why}")),
    }
    u
}

#[derive(Debug, PartialEq)]
enum UsageAnswer {
    Limits(Vec<crate::model::Meter>),
    SignedOut,
    Failed(String),
}

/// What a print run's `/usage` printed: `{status, error, command: {name, data: {groups}}}` on one
/// line, `status` `SUCCESS` or `ERROR`.
fn usage_answer(out: &str) -> UsageAnswer {
    // The answer is its last line; anything before it isn't the command's.
    let Some(v) = out.lines().rev().find_map(|l| serde_json::from_str::<Value>(l.trim()).ok().filter(Value::is_object)) else {
        return UsageAnswer::Failed("unexpected response".into());
    };
    if v.get("status").and_then(Value::as_str) != Some("SUCCESS") {
        let why = v.get("error").and_then(Value::as_str).unwrap_or("it failed");
        // What a run that couldn't sign in says.
        if why.contains("authentication") {
            return UsageAnswer::SignedOut;
        }
        return UsageAnswer::Failed(one_line(why, 120));
    }
    UsageAnswer::Limits(usage_meters(&v))
}

/// A meter for each of the answer's limits, `command.data.groups[].buckets[]`: each group of
/// models shares a limit, `{id, window, remaining_fraction, reset_time}`, weekly as seen.
fn usage_meters(v: &Value) -> Vec<crate::model::Meter> {
    let mut out = Vec::new();
    for g in v.pointer("/command/data/groups").and_then(Value::as_array).into_iter().flatten() {
        let name = group_label(g.get("name").and_then(Value::as_str).unwrap_or_default());
        if name.is_empty() {
            continue;
        }
        for b in g.get("buckets").and_then(Value::as_array).into_iter().flatten() {
            let Some(left) = b.get("remaining_fraction").and_then(Value::as_f64) else { continue };
            let window = b.get("window").and_then(Value::as_str).unwrap_or("weekly");
            let percent = ((1.0 - left) * 100.0).clamp(0.0, 100.0);
            out.push(crate::model::Meter {
                key: if window == "weekly" { format!("model:{name}") } else { format!("model:{name}:{window}") },
                label: name.clone(),
                window: format!("{}, {name} models", crate::usage::title(window)),
                percent,
                // An untouched limit's reset is a week from whenever it's asked, so it says nothing.
                resets_at: if left < 1.0 { b.get("reset_time").and_then(Value::as_str).and_then(parse_rfc3339_ms) } else { None },
                severity: crate::usage::severity(None, percent),
                model: Some(name.clone()),
            });
        }
    }
    out
}

/// A group's name for the bar: `Gemini Models` is `Gemini`, `Claude and GPT models` is
/// `Claude & GPT`.
fn group_label(name: &str) -> String {
    let name = name.trim();
    let cut = name.len().checked_sub(" models".len()).filter(|&i| name.get(i..).is_some_and(|end| end.eq_ignore_ascii_case(" models"))).map_or(name, |i| &name[..i]);
    cut.trim().replace(" and ", " & ")
}

/* ---- Enable Telemetry ---- */

/// Google's API behind Antigravity's account settings, where Enable Telemetry is kept: on, Google
/// collects the account's interactions "for use in evaluating, developing, and improving
/// Antigravity and models that support Antigravity", and Antigravity's terms say to opt out of
/// that in its settings. The app (2.23.0's language server) and `agy` 1.3.4 read it here as they
/// start (`fetchUserInfo`, `{}`, answered with `userSettings {telemetryEnabled,
/// marketingEmailsEnabled, userDataCollectionForceDisabled}`, fields that are false left out)
/// and change it with `setUserSettings {userSettings}`, which has no update mask, so what's
/// there is sent back with it. A running copy keeps the old value until it starts again.
const CLOUD_CODE: &str = "https://daily-cloudcode-pa.googleapis.com/v1internal";
/// Google turns away a call whose User-Agent doesn't say Antigravity (403, "The caller does not
/// have permission"), whatever its sign-in; one that names Codealong and says Antigravity is
/// let through (2026-10-13).
const TELEMETRY_AGENT: &str = concat!("User-Agent: codealong/", env!("CARGO_PKG_VERSION"), " (antigravity)");

/// The access token in the keychain's sign-in, which `agy` refreshes as it runs (Codealong
/// never does, and never writes the item): go-keyring's `go-keyring-base64:` (or `-encoded:`,
/// hex) and JSON `{token: {access_token, expiry}, …}`. None when it's missing or has expired.
fn access_token() -> Option<Result<String, &'static str>> {
    let raw = procs::run("/usr/bin/security", &["find-generic-password", "-s", KEYCHAIN_SERVICE, "-a", KEYCHAIN_ACCOUNT, "-w"], None, KEYCHAIN_TIMEOUT).ok()?;
    let raw = raw.trim();
    let bytes = match (raw.strip_prefix("go-keyring-base64:"), raw.strip_prefix("go-keyring-encoded:")) {
        (Some(b64), _) => data_encoding::BASE64.decode(b64.as_bytes()).ok(),
        (_, Some(hex)) => data_encoding::HEXLOWER_PERMISSIVE.decode(hex.as_bytes()).ok(),
        _ => Some(raw.as_bytes().to_vec()),
    };
    let v: Value = bytes.and_then(|b| serde_json::from_slice(&b).ok())?;
    let token = v.pointer("/token/access_token").and_then(Value::as_str).filter(|t| !t.is_empty())?;
    let expired = v.pointer("/token/expiry").and_then(Value::as_str).and_then(parse_rfc3339_ms).is_some_and(|t| t <= crate::util::now_ms() + 60_000);
    Some(if expired { Err("expired") } else { Ok(token.to_string()) })
}

fn cloud_code(token: &str, method: &str, body: &Value) -> Result<Value, String> {
    let headers = [format!("Authorization: Bearer {token}"), TELEMETRY_AGENT.to_string()];
    let reply = crate::http::call("POST", &format!("{CLOUD_CODE}:{method}"), &headers, Some(body), Duration::from_secs(15))?;
    match reply.status {
        200..=299 => reply.json().ok_or_else(|| "unexpected response".to_string()),
        401 => Err("the sign-in has expired".to_string()),
        other => Err(format!("HTTP {other}")),
    }
}

/// Where the account stands from `fetchUserInfo`'s answer.
fn telemetry_training(account: &AccountCfg, v: &Value) -> crate::training::Training {
    use crate::training::{State, Training};
    let set = |name: &str| v.pointer(&format!("/userSettings/{name}")).and_then(Value::as_bool) == Some(true);
    if set("userDataCollectionForceDisabled") {
        Training::new(account, State::Off, "Google keeps data collection off for this account: its organization's terms apply.")
    } else if set("telemetryEnabled") {
        Training::new(account, State::On, "Enable Telemetry is on: Google may collect this account's Antigravity interactions to improve Antigravity and the models behind it. It's in Antigravity's Settings › Account, and agy's /config.").stoppable()
    } else {
        Training::new(account, State::Off, "Enable Telemetry is off in Antigravity's account settings.")
    }
}

fn read_telemetry(account: &AccountCfg) -> crate::training::Training {
    use crate::training::Training;
    let cli = cli_command(account);
    match access_token() {
        None => Training::unknown(account, format!("Not signed in: run {} once to sign in.", code(cli))),
        Some(Err(_)) => Training::unknown(account, format!("Its sign-in needs refreshing, which {} does as it runs.", code(cli))),
        Some(Ok(token)) => match cloud_code(&token, "fetchUserInfo", &serde_json::json!({})) {
            Ok(v) => telemetry_training(account, &v),
            Err(why) => Training::unknown(account, format!("Couldn't read its account settings: {why}.")),
        },
    }
}

fn stop_telemetry(account: &AccountCfg) -> anyhow::Result<()> {
    let cli = cli_command(account);
    let token = match access_token() {
        Some(Ok(t)) => t,
        Some(Err(_)) => anyhow::bail!("{}'s sign-in needs refreshing: run {} once, then try again.", account.name, code(cli)),
        None => anyhow::bail!("{} isn't signed in: run {} once to sign in.", account.name, code(cli)),
    };
    let now = cloud_code(&token, "fetchUserInfo", &serde_json::json!({})).map_err(|why| anyhow::anyhow!("Couldn't read {}'s account settings: {why}.", account.name))?;
    let emails = now.pointer("/userSettings/marketingEmailsEnabled").and_then(Value::as_bool).unwrap_or(false);
    let body = serde_json::json!({ "userSettings": { "telemetryEnabled": false, "marketingEmailsEnabled": emails } });
    cloud_code(&token, "setUserSettings", &body).map_err(|why| anyhow::anyhow!("Couldn't turn off Enable Telemetry for {}: {why}.", account.name))?;
    Ok(())
}

/* ---- Reading conversations for the board ---- */

/// Antigravity's conversations for the board (`harness::Reader`): each account's summary tables
/// as last read, newest first, the command line's processes, and whether the app's agent runs.
#[derive(Default)]
pub struct Reader {
    conversations: HashMap<String, Vec<Conversation>>,
    marks: crate::harness::DatabaseMarks,
    processes: Vec<Process>,
    /// The log of each process that has a conversation open, by its pid, as far as it's read.
    logs: HashMap<u32, LogRead>,
    app_running: bool,
    looked_at: Option<Instant>,
}

impl Reader {
    /// Reads an account's summary tables again, and for the conversations that changed or that
    /// a process has open, the end of their records. True when a conversation is there that
    /// wasn't.
    fn reread(&mut self, a: &AccountCfg, look: &Look) -> bool {
        let dirs = folders(&a.home_path());
        // A change in the command line's folder is often a process that came or went.
        if dirs.iter().any(|f| f.cli) && self.looked_at.map_or(true, |t| t.elapsed() >= Duration::from_secs(2)) {
            self.look_at_processes(look);
        }
        let keep = look.seen.pinned_ids("antigravity", &a.name);
        let before: HashMap<String, Conversation> = self.conversations.remove(&a.name).unwrap_or_default().into_iter().map(|c| (c.id.clone(), c)).collect();
        let mut rows: Vec<Conversation> = dirs.iter().flat_map(|f| read_conversations(f, look.since, &keep)).collect();
        // The same conversation once, should two folders' tables both claim it.
        let mut listed = HashSet::new();
        rows.retain(|c| listed.insert(c.id.clone()));
        rows.extend(self.processes.iter().filter(|p| p.conversation.as_ref().is_some_and(|id| !listed.contains(id))).filter_map(|p| started(&dirs, p, look.now)));
        rows.sort_by(|x, y| y.last_modified.cmp(&x.last_modified));
        let mut added = false;
        for c in &mut rows {
            let process = self.processes.iter().find(|p| p.conversation.as_deref() == Some(c.id.as_str()));
            let held = process.is_some();
            if let Some(log) = process.and_then(|p| self.logs.get_mut(&p.pid)) {
                log.read_on();
                c.said = log.said.clone();
                // The summary's time of your last prompt can be an older copy of the row.
                if let Some((_, at)) = c.said.prompted.as_ref().filter(|(to, _)| *to == c.id) {
                    c.last_input = c.last_input.max(Some(*at));
                }
            }
            match before.get(&c.id) {
                Some(old) if !held && old.last_modified == c.last_modified && old.step_count == c.step_count => {
                    c.tail = old.tail.clone();
                    c.mode = old.mode.clone();
                    c.failed = old.failed.clone();
                    c.model = old.model.clone();
                }
                old => {
                    added |= old.is_none();
                    c.tail = transcript_file(&c.dir, &c.id).map(|p| tail(&p, c.cwd.as_deref())).unwrap_or_default();
                    c.mode = turn_mode(&c.dir, &c.id);
                    // The note a failed turn ends with never reaches the transcript.
                    c.failed = turn_failed(&c.dir, &c.id, c.last_modified);
                    c.model = turn_model(&c.dir, &c.id);
                }
            }
            if c.running() || held {
                (c.step_under_way, c.waiting, c.turn_ended, c.latest_step_at) = steps_now(&c.dir, &c.id);
            }
            // An idle row after Escape is the UI's own, whatever its time and count.
            c.idled_by_another = held && idled_by_another(before.get(&c.id), c) && !c.said.stopped_last();
        }
        self.conversations.insert(a.name.clone(), rows);
        added
    }

    /// Looks at the processes again, and finds the log of each that has a conversation open.
    /// True when they changed.
    fn look_at_processes(&mut self, look: &Look) -> bool {
        self.looked_at = Some(Instant::now());
        let (processes, app_running) = processes(look.ps.all());
        let changed = processes != self.processes || app_running != self.app_running;
        self.processes = processes;
        self.app_running = app_running;
        self.track_logs();
        changed
    }

    /// Keeps the logs of the processes that have a conversation open, finding a new one's own
    /// among those it has open once.
    fn track_logs(&mut self) {
        let holding: Vec<&Process> = self.processes.iter().filter(|p| p.conversation.is_some()).collect();
        self.logs.retain(|pid, _| holding.iter().any(|p| p.pid == *pid));
        for p in holding {
            if !self.logs.contains_key(&p.pid) {
                if let Some(log) = own_log(&p.logs, p.pid).and_then(LogRead::new) {
                    self.logs.insert(p.pid, log);
                }
            }
        }
    }
}

impl crate::harness::Reader for Reader {
    fn harness(&self) -> Harness {
        Harness::Antigravity
    }

    fn scan(&mut self, accounts: &[AccountCfg], look: &Look) {
        self.conversations.clear();
        if !accounts.is_empty() {
            self.look_at_processes(look);
        }
        for a in accounts {
            self.reread(a, look);
        }
    }

    /// Each folder's summary table, its WAL and its journal, all in the folder itself.
    fn watch(&self, account: &AccountCfg) -> Vec<(PathBuf, bool)> {
        folders(&account.home_path()).into_iter().map(|f| (f.path, false)).collect()
    }

    fn owns(&self, account: &AccountCfg, path: &Path) -> bool {
        folders(&account.home_path()).iter().any(|f| path.starts_with(&f.path))
    }

    fn apply(&mut self, account: &AccountCfg, paths: &[PathBuf], look: &Look) -> Applied {
        let dirs = folders(&account.home_path());
        if !paths.iter().any(|p| file_name(p).contains(".db") && dirs.iter().any(|f| p.parent() == Some(f.path.as_path()))) {
            return Applied::default();
        }
        Applied { dirty: true, added: self.reread(account, look), ..Applied::default() }
    }

    /// Keeps the list of models a start can ask for at hand (`models`).
    fn poll(&mut self, accounts: &[AccountCfg]) {
        for a in accounts.iter().filter(|a| a.command.is_some() || cli_installed(&crate::util::home())) {
            models(cli_command(a));
        }
    }

    /// Which conversations a command line process has open, and whether the app's agent runs.
    /// A process that came may have a conversation its table doesn't list yet, and one that
    /// went leaves its conversation as it was: either way they're read again.
    fn tick(&mut self, accounts: &[AccountCfg], look: &Look) -> Applied {
        let mut out = Applied::default();
        if !accounts.is_empty() && self.look_at_processes(look) {
            out.dirty = true;
            for a in accounts {
                out.added |= self.reread(a, look);
            }
        }
        out
    }

    /// The summary tables, and the databases of the conversations under way or open in a
    /// process, where steps are written, and one starts to wait on you, without the summary
    /// changing; and the logs of the terminal UIs, where a message you type while a turn runs
    /// is the only sign of it.
    fn glance(&mut self, accounts: &[AccountCfg], look: &Look) -> Applied {
        let mut out = Applied::default();
        let told: Vec<PathBuf> = self.logs.values_mut().filter_map(|l| l.read_on().then(|| l.path.clone())).collect();
        for a in accounts {
            let mut changed = false;
            for f in folders(&a.home_path()) {
                changed |= self.marks.changed(&format!("{}|{}", a.name, f.path.display()), &f.path.join(SUMMARIES));
                changed |= told.iter().any(|log| log.starts_with(&f.path));
            }
            let held = |c: &&Conversation| self.processes.iter().any(|p| p.conversation.as_deref() == Some(c.id.as_str()));
            let under_way: Vec<PathBuf> = self.conversations.get(&a.name).into_iter().flatten().filter(|c| c.running() || held(c)).map(|c| c.dir.join("conversations").join(format!("{}.db", c.id))).collect();
            for db in under_way {
                changed |= self.marks.changed(&db.to_string_lossy(), &db);
            }
            if changed {
                out.added |= self.reread(a, look);
                out.dirty = true;
            }
        }
        out
    }

    fn sessions(&self, account: &AccountCfg, look: &Look) -> Vec<Session> {
        let rows = self.conversations.get(&account.name).map(|v| v.as_slice()).unwrap_or(&[]);
        rows.iter()
            .map(|c| {
                let key = Session::key_for(Harness::Antigravity, &account.name, &c.id);
                let process = self.processes.iter().find(|p| p.conversation.as_deref() == Some(c.id.as_str()));
                let mut s = crate::board::antigravity_session(account, c, process, self.app_running, look.seen.seen_at(&key), look.now);
                s.archived = look.archived_here(&key, s.last_activity);
                s
            })
            .collect()
    }

    fn cwds(&self) -> Vec<String> {
        self.conversations.values().flatten().filter_map(|c| c.cwd.clone()).collect()
    }
}

/// The folder a conversation on the board is kept in: the command line's for one it runs, else
/// the first of the account's that has its record.
fn folder_of(account: &AccountCfg, s: &Session) -> PathBuf {
    let dirs = folders(&account.home_path());
    let has = |f: &&Folder| f.path.join("conversations").join(format!("{}.db", s.id)).is_file() || f.path.join("brain").join(&s.id).is_dir();
    let by_surface = dirs.iter().find(|f| f.cli == (s.surface == Surface::Cli));
    dirs.iter().find(has).or(by_surface).or(dirs.first()).map(|f| f.path.clone()).unwrap_or_else(|| account.home_path())
}

/// Antigravity (`harness::Adapter`): the hub app's conversations and the command line's, read
/// from their records. The app takes nothing from outside; the command line starts and resumes
/// conversations in a terminal.
pub struct Antigravity;

impl Antigravity {
    fn record(&self, ctx: &Ctx, s: &Session) -> anyhow::Result<PathBuf> {
        transcript_file(&folder_of(ctx.account, s), &s.id).ok_or_else(|| anyhow::anyhow!("Antigravity hasn't written this conversation's record yet."))
    }

    /// The call its one step that waits on you is, when that's the call you saw (`id`), with the
    /// process whose terminal UI takes the keys that answer it: only a command line
    /// conversation's does. Keys are pressed only while its database says so: a stray digit lands
    /// in its input box, and with several calls waiting a key would answer whichever is up.
    fn waiting_call(&self, ctx: &Ctx, s: &Session, id: &str) -> anyhow::Result<(u32, Call)> {
        let (Surface::Cli, Some(pid)) = (s.surface, s.pid) else {
            anyhow::bail!("Antigravity asks in {}; answer it there.", if s.surface == Surface::Cli { "its terminal" } else { "its own window" });
        };
        let open = tail(&self.record(ctx, s)?, None).open;
        let call = |step: i64| open.iter().find(|c| c.result_step == step).cloned().unwrap_or_else(|| Call { label: "another call".into(), ..Call::default() });
        match waiting_steps(&folder_of(ctx.account, s), &s.id).as_slice() {
            [] => anyhow::bail!("It isn't waiting on you any more."),
            [step] if step.to_string() == id => Ok((pid, call(*step))),
            [step] => anyhow::bail!("It's waiting on a different call now: {}. Look again before answering.", call(*step).label),
            _ => anyhow::bail!("It waits on more than one call; answer them in its terminal."),
        }
    }
}

/// A key to press in a terminal UI, or words to type there and send with Return.
#[derive(Debug, Clone, PartialEq)]
enum Press {
    Key(Key),
    Words(String),
}

/// The keys that pick a question's choices: 1 to 9, the last for the write-in that follows its
/// own choices.
const KEYED_CHOICES: usize = 9;

/// How long between keys: an Escape and the key after it arriving together read as Alt and that
/// key.
const PRESS_GAP: Duration = Duration::from_millis(100);

/// Presses `keys` in the terminal a session's process runs in, in order.
fn press_all(pid: u32, s: &Session, keys: &[Press]) -> anyhow::Result<()> {
    let cwd = Path::new(&s.cwd);
    let Some(host) = crate::launch::terminal_host(pid, cwd)? else {
        anyhow::bail!("It isn't in a terminal Codealong can press keys in ({}). Open it to answer.", crate::launch::presses_keys_in());
    };
    for (i, press) in keys.iter().enumerate() {
        if i > 0 {
            std::thread::sleep(PRESS_GAP);
        }
        match press {
            Press::Key(key) => crate::launch::press_key(&host, *key)?,
            Press::Words(words) => {
                if let crate::launch::Typed::Nowhere(why) = crate::launch::terminal_type_session(pid, cwd, words, false, crate::launch::Submit::Return, false)? {
                    anyhow::bail!("Your words couldn't be typed into it: {why}. Finish answering in its terminal.");
                }
            }
        }
    }
    Ok(())
}

/// Your answer to a question as `answers` gives it (`Adapter::answer_questions`): a label or words
/// of your own, or a list of them where it takes several; empty ones left out.
fn given(answers: &Value, q: &Question) -> Vec<String> {
    let words = |v: &Value| v.as_str().map(str::trim).filter(|t| !t.is_empty()).map(String::from);
    match answers.get(&q.question) {
        Some(Value::Array(list)) => list.iter().filter_map(words).collect(),
        Some(v) => words(v).into_iter().collect(),
        None => Vec::new(),
    }
}

/// The keys that answer its terminal UI's questions (1.3.5, in tmux, 2026-10-14): one question at
/// a time, its choices numbered and Write-in after them. On one that takes one answer, a choice's
/// digit picks it and goes on to the next; on one that takes several, each digit ticks its
/// choice and Return goes on. Write-in's digit opens a box for your own words (a paste of several
/// lines is taken whole), which Return sends, with what's ticked. Escape skips a question, its
/// answer "User Skipped". On the last, what goes on submits them all, and the turn goes on.
/// Left goes back a question, so it starts with enough of them to be at the first, wherever its
/// terminal was left (Left on the first does nothing). What was picked or ticked there stays
/// when you go back, and a digit toggles a tick, so each question but the last is skipped and
/// gone back to before it's answered, which clears it. The last can't be, as skipping it
/// submits them all and no key clears ticks, so a choice ticked by hand there and picked here
/// too goes unticked; the conversation shows what it took (`with_answers`). Reading that back
/// to say so was tried and left out (2026-10-14): what an answer says is shown only in a box
/// that goes as the session does, and making it stay was more than so rare a case is worth.
/// Without `answers` they're declined: nothing there turns them down, so
/// every question is skipped and one more Escape stops the turn as it goes on ("Interrupted ·
/// What should Antigravity CLI do instead?"), as a denied call's does.
fn question_keys(questions: &[Question], answers: Option<&Value>) -> anyhow::Result<Vec<Press>> {
    let digit = |i: usize| u8::try_from(i + 1).ok().filter(|_| i < KEYED_CHOICES).map(|n| Press::Key(Key::Digit(n)));
    let mut out = vec![Press::Key(Key::Left); questions.len().saturating_sub(1)];
    let mut answered = false;
    for (k, q) in questions.iter().enumerate() {
        let given = answers.map(|a| given(a, q)).unwrap_or_default();
        let too_many = || anyhow::anyhow!("“{}” has more choices than Codealong picks among with a key; answer it in its terminal.", one_line(&q.question, 80));
        if given.is_empty() {
            out.push(Press::Key(Key::Escape));
            continue;
        }
        answered = true;
        if k + 1 < questions.len() {
            out.extend([Press::Key(Key::Escape), Press::Key(Key::Left)]);
        }
        let index = |label: &str| q.options.iter().position(|o| o.label == label);
        let write_in = |words: String, out: &mut Vec<Press>| -> anyhow::Result<()> {
            out.push(digit(q.options.len()).ok_or_else(too_many)?);
            out.push(Press::Words(words));
            Ok(())
        };
        if !q.multi_select {
            match index(&given[0]) {
                Some(i) => out.push(digit(i).ok_or_else(too_many)?),
                None => write_in(given[0].clone(), &mut out)?,
            }
            continue;
        }
        let mut ticked: Vec<usize> = given.iter().filter_map(|g| index(g)).collect();
        ticked.sort_unstable();
        ticked.dedup();
        for i in ticked {
            out.push(digit(i).ok_or_else(too_many)?);
        }
        let own: Vec<&str> = given.iter().filter(|g| index(g).is_none()).map(String::as_str).collect();
        if own.is_empty() {
            out.push(Press::Key(Key::Return));
        } else {
            write_in(own.join(", "), &mut out)?;
        }
    }
    match answers {
        Some(_) if !answered => anyhow::bail!("Pick an answer first."),
        Some(_) => {}
        None => out.push(Press::Key(Key::Escape)),
    }
    Ok(out)
}

impl crate::harness::Adapter for Antigravity {
    /// Trusts the folder for the command line (`trust_folder`). In a folder it hasn't trusted,
    /// `agy` stops at "Do you trust the contents of this project?", and a prompt given with `-i`
    /// runs behind it in its `scratch` folder with no workspace, where it asks before it reads
    /// each file of the folder it was started in (two helpers' reviews, 2026-10-13). You chose
    /// the folder, as answering the prompt would. A start goes ahead without it, and agy asks.
    fn before_start(&self, account: &AccountCfg, folder: &Path) {
        let Some(settings) = cli_settings(&account.home_path()) else { return };
        if let Err(e) = trust_folder(&settings, folder, &crate::util::home()) {
            eprintln!("Couldn't trust {} for {}: {e:#}", crate::util::tilde(folder), account.name);
        }
    }

    fn harness(&self) -> Harness {
        Harness::Antigravity
    }

    /// One account, in the hub app's folder: the command line's folder goes with it (`folders`),
    /// and nothing moves either.
    fn spec(&self) -> &'static crate::detect::Spec {
        const SPEC: crate::detect::Spec = crate::detect::Spec { harness: "antigravity", program: None, default_command: None, var: None, default_dir: "~/.gemini/antigravity", siblings: None };
        &SPEC
    }

    /// The sign-in is in the keychain, or from before, a token file under ~/.gemini; only
    /// whether it's there is read.
    fn signed_in(&self, home: &Path) -> (bool, Option<String>) {
        (signed_in(home), None)
    }

    /// The command line signs in as it starts; without it, the app does from its own window.
    fn sign_in_command(&self, account: &AccountCfg) -> String {
        if has_cli(account, &crate::util::home()) {
            cli_command(account).to_string()
        } else {
            "open -a Antigravity".to_string()
        }
    }

    fn installed(&self, home: &Path, on_path: &dyn Fn(&str) -> bool) -> bool {
        crate::detect::app("Antigravity.app").is_some() || on_path(CLI) || crate::detect::is_executable(&home.join(".local/bin").join(CLI))
    }

    fn folder_use(&self, dir: &Path) -> crate::harness::FolderUse {
        let files: Vec<PathBuf> = folders(dir).iter().flat_map(|f| [f.path.join(SUMMARIES), f.path.join(format!("{SUMMARIES}-wal"))]).collect();
        crate::harness::FolderUse { has_data: files.iter().any(|p| p.is_file() && !file_name(p).ends_with("-wal")), last_used: files.iter().filter_map(|p| crate::util::mtime_ms(p)).max(), signed_in: false }
    }

    fn skills_dir(&self, _account: &AccountCfg) -> Option<PathBuf> {
        Some(crate::util::home().join(".gemini/config/skills"))
    }

    /// The command line starts a conversation in a terminal; the app only in its own window.
    fn places(&self, account: &AccountCfg, home: &Path) -> std::result::Result<Vec<crate::start::Place>, String> {
        if has_cli(account, home) {
            Ok(vec![crate::start::Place::Terminal, crate::start::Place::Background])
        } else {
            Err(format!("The Antigravity app starts conversations only in its own window. With its command line, {CLI}, installed, {} can start them in a terminal from here.", account.name))
        }
    }

    /// The command line's conversation opens in its terminal, or a new one that resumes it
    /// (`agy --conversation`); a print run has it until it ends. The app has no link to a
    /// conversation, so the app itself comes forward.
    fn targets(&self, s: &Session, o: &crate::harness::Opens) -> Vec<crate::model::Target> {
        let who = o.who.clone();
        match s.surface {
            Surface::Cli => {
                let busy = s.live && s.pid.is_none();
                vec![o.terminal(s, if busy { format!("{who} · a run without a terminal has it open") } else { who }, busy)]
            }
            Surface::App => vec![o.way(s, "antigravity", "app", "Open Antigravity".into(), format!("{who} · no link to a conversation, so the app comes forward"), true, false)],
        }
    }

    fn open(&self, _s: &Session, action: &str) -> anyhow::Result<()> {
        match action {
            "antigravity" => crate::launch::open_antigravity(),
            other => anyhow::bail!("Unknown action {other:?}"),
        }
    }

    fn app_bundle(&self, action: &str) -> Option<&'static str> {
        (action == "antigravity").then_some(APP_BUNDLE_ID)
    }

    /// Each conversation's transcript, in both folders.
    fn search_files(&self, account: &AccountCfg, since: i64) -> Vec<(PathBuf, String, i64)> {
        let rows = folders(&account.home_path()).into_iter().flat_map(|f| read_conversations(&f, since, &HashSet::new()));
        rows.filter_map(|c| Some((transcript_file(&c.dir, &c.id)?, Session::key_for(Harness::Antigravity, &account.name, &c.id), c.last_modified))).collect()
    }

    /// What you asked, and what the agent replied.
    fn prose(&self, line: &[u8]) -> Option<(i64, bool, String)> {
        let has = |needle: &[u8]| memchr::memmem::find(line, needle).is_some();
        if !has(br#""type":"USER_INPUT""#) && !has(br#""type":"PLANNER_RESPONSE""#) {
            return None;
        }
        let v: Value = serde_json::from_slice(line).ok()?;
        let s = step(&v)?;
        let text = match (s.source, s.kind) {
            ("USER_EXPLICIT", "USER_INPUT") => request_of(s.content),
            ("MODEL", "PLANNER_RESPONSE") => s.content.trim(),
            _ => "",
        };
        (!text.is_empty()).then(|| (s.at, s.kind == "USER_INPUT", text.to_string()))
    }

    /// The models `agy models` lists, which its `--model` takes.
    fn models(&self, account: &AccountCfg) -> Vec<crate::start::ModelOption> {
        models(cli_command(account))
    }

    fn takes_model(&self) -> bool {
        true
    }

    /// `agy -i <prompt>` opens its terminal UI on a prompt (checked on 1.2.14); it names the
    /// conversation itself. Attachments go as paths, their folders added (`--add-dir`, once
    /// each) so it reads them without asking.
    fn start_in_terminal(&self, account: &AccountCfg, ask: &crate::harness::StartAsk) -> crate::harness::Typed {
        use crate::start::{FILES_VAR, IMAGES_VAR, PROMPT_VAR};
        let (has_images, has_files) = crate::attachments::kinds(ask.prompt);
        let prompt = crate::attachments::expand_in(ask.prompt, ask.images_dir, ask.files_dir, crate::attachments::Style::Path);
        let prompt = prompt.trim();
        let mut input = cli_command(account).to_string();
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
            input.push_str(&format!(" -i \"${PROMPT_VAR}\""));
            env.push((PROMPT_VAR.to_string(), prompt.to_string()));
        }
        input.push('\n');
        crate::harness::Typed { input, env, key: None }
    }

    fn session_cwds(&self, home: &Path, since: i64) -> Vec<String> {
        folders(home).iter().flat_map(|f| read_conversations(f, since, &HashSet::new())).filter_map(|c| c.cwd).collect()
    }

    /// `CLOUD_CODE`'s host, where `agy` sends the model's requests (its error notes name it).
    fn api_address(&self) -> Option<&'static str> {
        Some("daily-cloudcode-pa.googleapis.com:443")
    }

    /// The quotas are read through the command line (`read_usage`); without it they're only
    /// in the app.
    fn no_limits(&self, account: &AccountCfg) -> Option<&'static str> {
        (!has_cli(account, &crate::util::home())).then_some("Antigravity shows its quotas in the app. With its command line, agy, installed, Codealong shows them here.")
    }

    fn polls_usage(&self, account: &AccountCfg) -> bool {
        has_cli(account, &crate::util::home())
    }

    fn read_usage(&self, account: &AccountCfg, now: i64) -> crate::model::Usage {
        read_usage(account, now)
    }

    /// Enable Telemetry, in the account's settings on Google's servers, which the app and `agy`
    /// share.
    fn training(&self, account: &AccountCfg) -> crate::training::Training {
        read_telemetry(account)
    }

    fn stop_training(&self, account: &AccountCfg) -> anyhow::Result<()> {
        stop_telemetry(account)
    }

    fn records(&self, account: &AccountCfg, s: &Session) -> Vec<PathBuf> {
        let dir = folder_of(account, s);
        transcript_file(&dir, &s.id).into_iter().chain([dir.join("conversations").join(format!("{}.db", s.id)), dir.join(SUMMARIES)]).collect()
    }

    fn conversation(&self, ctx: &Ctx, s: &Session, before: Option<u64>, limit: usize) -> anyhow::Result<Page> {
        let path = self.record(ctx, s)?;
        Ok(with_answers(&path, crate::transcript::file_page(&path, conversation_items, before, limit)?))
    }

    fn conversation_after(&self, ctx: &Ctx, s: &Session, after: u64) -> anyhow::Result<Page> {
        let path = self.record(ctx, s)?;
        Ok(with_answers(&path, crate::transcript::file_newer(&path, conversation_items, after)?))
    }

    /// The command line's conversation takes a prompt typed into its terminal, or a new terminal
    /// that resumes it; one a print run has open takes nothing until that ends. Nothing reaches
    /// the app's from outside: the app comes forward with the prompt on the clipboard.
    fn routes(&self, _account: &AccountCfg, s: &Session) -> Vec<crate::send::Route> {
        match (s.surface, s.pid, s.live) {
            (Surface::Cli, Some(pid), _) => vec![crate::send::Route::Type { pid }],
            (Surface::Cli, None, false) => vec![crate::send::Route::Resume],
            (Surface::Cli, None, true) => Vec::new(),
            _ => vec![OnClipboard.into()],
        }
    }

    /// `agy --conversation <id> -i <prompt>` goes on with a conversation in its terminal UI, on
    /// the prompt (checked on 1.2.14).
    /// On the model asked for, as a start is, and without a prompt it opens on the
    /// conversation and waits for one.
    fn resume(&self, account: &AccountCfg, s: &Session, ask: &crate::harness::StartAsk) -> anyhow::Result<(String, Vec<(String, String)>)> {
        use crate::start::{FILES_VAR, IMAGES_VAR, PROMPT_VAR};
        let (has_images, has_files) = crate::attachments::kinds(ask.prompt);
        let prompt = crate::attachments::expand_in(ask.prompt, ask.images_dir, ask.files_dir, crate::attachments::Style::Path).trim().to_string();
        let mut input = format!("{} --conversation {}", cli_command(account), s.id);
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
            input.push_str(&format!(" -i \"${PROMPT_VAR}\""));
            env.push((PROMPT_VAR.to_string(), prompt));
        }
        input.push('\n');
        Ok((input, env))
    }

    /// A terminal, or one behind your windows.
    fn resumes_in(&self, place: crate::start::Place) -> bool {
        use crate::start::Place;
        matches!(place, Place::Terminal | Place::Background)
    }

    /// What its step waits on you to allow, or the questions it asks. The command line's terminal
    /// UI is answered here with keys (`answer`, `answer_questions`); the app takes nothing from
    /// outside.
    fn pending(&self, ctx: &Ctx, s: &Session) -> Option<crate::harness::Pending> {
        let open = tail(&self.record(ctx, s).ok()?, None).open;
        // Calls run side by side, so the first that hasn't finished may be one that still runs:
        // the one whose step waits is the one asked about.
        let waiting = waiting_steps(&folder_of(ctx.account, s), &s.id);
        let c = open.iter().find(|c| waiting.contains(&c.result_step)).or(open.first())?.clone();
        let cannot = match (s.surface, s.pid) {
            (Surface::Cli, Some(_)) if c.questions.iter().any(|q| q.options.len() >= KEYED_CHOICES) => Some(format!("A question of its has more than {} choices, more than Codealong picks among with a key; answer it in its terminal.", KEYED_CHOICES - 1)),
            (Surface::Cli, Some(_)) => None,
            (Surface::Cli, None) => Some("Antigravity asks in its terminal; answer it there.".to_string()),
            _ => Some("Antigravity asks in its own window; answer it there.".to_string()),
        };
        // Questions are answered with your picks, so their arguments aren't shown.
        let input = c.questions.is_empty().then(|| c.input.clone());
        Some(crate::harness::Pending { id: c.result_step.to_string(), label: c.asked(), input, cannot, always: false, no_once: false, plan: false, message: false, always_rule: None, questions: c.questions, unwritten: false, terminal: None })
    }

    /// Its terminal UI's permission prompt, with a key (1.3.4, in tmux, 2026-10-13): `1`, with no
    /// Return, is the first choice of every prompt, Yes (run the command, read the file); Escape
    /// cancels it, which denies the call and ends the turn ("Interrupted · What should Antigravity
    /// CLI do instead?"). Its other choices (always, for the conversation or in its settings, and
    /// No) are numbered differently from one prompt to another, a file outside its workspace
    /// having two or three, so they're left to its terminal. Questions are declined by skipping
    /// each and stopping the turn (`question_keys`).
    fn answer(&self, ctx: &Ctx, s: &Session, id: &str, answer: crate::harness::Answer) -> anyhow::Result<String> {
        use crate::harness::Answer;
        let (pid, call) = self.waiting_call(ctx, s, id)?;
        let keys = match answer {
            Answer::Allow | Answer::Always if !call.questions.is_empty() => anyhow::bail!("It asks you questions: pick your answers."),
            Answer::Allow => vec![Press::Key(Key::Digit(1))],
            Answer::Deny if !call.questions.is_empty() => question_keys(&call.questions, None)?,
            Answer::Deny => vec![Press::Key(Key::Escape)],
            Answer::Always => anyhow::bail!("Antigravity's prompt is answered here with Allow or Deny only."),
        };
        press_all(pid, s, &keys)?;
        Ok(match answer {
            Answer::Deny if !call.questions.is_empty() => "Declined its questions; it stops and waits for you.".to_string(),
            Answer::Deny => format!("Denied {}; it stops and waits for you.", call.label),
            _ => format!("Allowed {}.", call.label),
        })
    }

    /// Its terminal UI's questions, with the keys that answer them there (`question_keys`).
    fn answer_questions(&self, ctx: &Ctx, s: &Session, id: &str, answers: &Value) -> anyhow::Result<String> {
        let (pid, call) = self.waiting_call(ctx, s, id)?;
        if call.questions.is_empty() {
            anyhow::bail!("It isn't asking you questions: allow or deny it.");
        }
        let keys = question_keys(&call.questions, Some(answers))?;
        press_all(pid, s, &keys)?;
        let said: Vec<String> = call.questions.iter().filter_map(|q| Some(format!("{} → {}", one_line(&q.question, 80), given(answers, q).join(", "))).filter(|_| !given(answers, q).is_empty())).collect();
        Ok(format!("Answered: {}.", said.join("; ")))
    }

    fn open_calls(&self, ctx: &Ctx, s: &Session) -> Vec<String> {
        self.record(ctx, s).map(|p| tail(&p, None).open.into_iter().map(|c| c.input).collect()).unwrap_or_default()
    }

    /// A conversation the command line starts with no workspace works in its folder's `scratch`.
    fn chats(&self) -> Option<(PathBuf, &'static str)> {
        Some((crate::util::home().join(".gemini").join(CLI_DIR).join(SCRATCH), "Antigravity chats"))
    }

    /// Its list of conversations, in each of its folders.
    fn databases(&self) -> &'static [&'static str] {
        &[SUMMARIES]
    }

    fn reader(&self, _wake: crate::harness::Wake) -> Box<dyn crate::harness::Reader> {
        Box::new(Reader::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCHEMA: &str = "CREATE TABLE conversation_summaries (conversation_id TEXT PRIMARY KEY, title TEXT NOT NULL DEFAULT '', preview TEXT NOT NULL DEFAULT '', \
         step_count INTEGER NOT NULL DEFAULT 0, last_modified_time DATETIME NOT NULL, workspace_uris TEXT NOT NULL, status TEXT NOT NULL DEFAULT '', \
         source TEXT NOT NULL DEFAULT '', project_id TEXT NOT NULL DEFAULT '', agent_name TEXT NOT NULL DEFAULT '', parent_conversation_id TEXT NOT NULL DEFAULT '', \
         nesting_depth INTEGER NOT NULL DEFAULT 0, not_fully_idle NUMERIC NOT NULL DEFAULT false, killed NUMERIC NOT NULL DEFAULT false, \
         last_user_input_time DATETIME NOT NULL, last_user_input_step_index INTEGER NOT NULL DEFAULT -1, app_data_dir TEXT NOT NULL DEFAULT '', raw_summary BLOB, group_id TEXT NOT NULL DEFAULT '');";

    /// A number as protobuf writes it.
    fn proto_varint(mut n: u64, out: &mut Vec<u8>) {
        while n >= 0x80 {
            out.push((n as u8) | 0x80);
            n >>= 7;
        }
        out.push(n as u8);
    }

    /// A message of `(field, bytes)` parts, each length-delimited.
    fn proto_message(parts: &[(u64, Vec<u8>)]) -> Vec<u8> {
        let mut out = Vec::new();
        for (field, bytes) in parts {
            proto_varint(field << 3 | 2, &mut out);
            proto_varint(bytes.len() as u64, &mut out);
            out.extend(bytes);
        }
        out
    }

    /// A varint field.
    fn proto_number(field: u64, n: u64) -> Vec<u8> {
        let mut out = Vec::new();
        proto_varint(field << 3, &mut out);
        proto_varint(n, &mut out);
        out
    }

    /// A step's payload that says only when it began (`STEP_BEGAN`), at epoch milliseconds `at`.
    fn began(at: i64) -> Vec<u8> {
        let time = [proto_number(1, (at / 1000) as u64), proto_number(2, (at % 1000 * 1_000_000) as u64)].concat();
        proto_message(&[(5, proto_message(&[(1, time)]))])
    }

    /// An error note's payload: what went wrong, and words for you when there are some
    /// (`ERROR_FOR_YOU`).
    fn error_note(error: &str, for_you: Option<&str>) -> Vec<u8> {
        error_note_with(error, for_you, &[])
    }

    /// An error note with Google's details of the error, each a `google.rpc` detail as JSON
    /// (`ERROR_DETAILS`).
    fn error_note_with(error: &str, for_you: Option<&str>, google: &[&str]) -> Vec<u8> {
        let mut details = for_you.map(|w| vec![(1, w.as_bytes().to_vec())]).unwrap_or_default();
        details.push((2, error.as_bytes().to_vec()));
        details.extend(google.iter().map(|d| (10, d.as_bytes().to_vec())));
        proto_message(&[(24, proto_message(&[(3, proto_message(&details))]))])
    }

    /// A terminal UI's log line at a time on 2026-10-14, as glog writes it.
    fn glog(at: &str, text: &str) -> String {
        format!("I1014 {at}   22584 input_loop.go:106] {text}")
    }

    /// When a log line at `at` (`17:45:37.928`) on 2026-10-14 was written.
    fn on_14th(at: &str) -> i64 {
        let n = |from: usize, to: usize| at[from..to].parse::<i64>().unwrap();
        crate::util::local_ms(2026, 10, 14, n(0, 2), n(3, 5), n(6, 8), n(9, 12)).unwrap()
    }

    /// A time as the summary table writes one, in UTC: a log's local time is one wherever the
    /// test runs.
    fn in_utc(ms: i64) -> String {
        let (secs, days) = (ms.div_euclid(1000), ms.div_euclid(86_400_000));
        let (y, m, d) = crate::util::civil_from_days(days);
        let t = secs.rem_euclid(86_400);
        format!("{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02}+00:00", t / 3600, t / 60 % 60, t % 60)
    }

    #[test]
    fn reads_the_summary_table() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(&format!(
            "{SCHEMA}\
             INSERT INTO conversation_summaries (conversation_id, title, preview, step_count, last_modified_time, workspace_uris, status, not_fully_idle, last_user_input_time) VALUES \
             ('c1', 'Fix the build', 'Fix the build so tests pass', 12, '2026-09-24 14:20:07.434615-07:00', '[\"file:///Users/me/My%20Repo/\"]', 'CASCADE_RUN_STATUS_RUNNING', 1, '2026-09-24 14:16:07.000000-07:00'), \
             ('c2', 'Old one', '', 3, '2026-08-01 10:00:00+00:00', '[]', '', 0, '2026-08-01 09:00:00+00:00'), \
             ('c3', 'Nested', '', 3, '2026-09-24 14:20:07-07:00', '[]', '', 0, '2026-09-24 14:16:07-07:00');\
             UPDATE conversation_summaries SET parent_conversation_id = 'c1', nesting_depth = 1 WHERE conversation_id = 'c3';"
        ))
        .unwrap();
        let since = parse_rfc3339_ms("2026-09-01T00:00:00Z").unwrap();
        let rows = read_from(&db, "antigravity", since, &HashSet::new()).unwrap();
        assert_eq!(rows.len(), 1);
        let c = &rows[0];
        assert_eq!(c.id, "c1");
        assert_eq!(c.cwd.as_deref(), Some("/Users/me/My Repo"));
        assert_eq!(c.status, "cascade_run_status_running");
        assert!(c.not_fully_idle && c.running());
        assert_eq!(c.last_modified, parse_rfc3339_ms("2026-09-24T21:20:07.434Z").unwrap());
        assert_eq!(c.last_input, parse_rfc3339_ms("2026-09-24T21:16:07Z"));
        // A pinned one comes back however old.
        let keep: HashSet<String> = ["c2".to_string()].into();
        assert_eq!(read_from(&db, "antigravity", since, &keep).unwrap().len(), 2);
    }

    /// A conversation Codealong started with an image works in its repo, though its row lists
    /// the images folder first.
    #[test]
    fn a_conversation_works_in_its_repo_not_in_codealongs_attachments() {
        let images = PathBuf::from("/Users/me/Library/Application Support/Codealong/images");
        let files = PathBuf::from("/Users/me/Library/Application Support/Codealong/files");
        let ws = |w: &[&str]| w.iter().map(|w| w.to_string()).collect::<Vec<_>>();
        let attachments = [images.clone(), files.clone()];
        let both = ws(&["/Users/me/Library/Application Support/Codealong/images", "/Users/me/app"]);
        assert_eq!(work_folder(&both, &attachments).as_deref(), Some("/Users/me/app"));
        let all = ws(&["/Users/me/Library/Application Support/Codealong/files", "/Users/me/Library/Application Support/Codealong/images", "/Users/me/app"]);
        assert_eq!(work_folder(&all, &attachments).as_deref(), Some("/Users/me/app"));
        // One started in the images folder itself works there.
        assert_eq!(work_folder(&ws(&["/Users/me/Library/Application Support/Codealong/images"]), &attachments).as_deref(), Some(images.to_str().unwrap()));
        assert_eq!(work_folder(&ws(&["/Users/me/app", "/Users/me/lib"]), &attachments).as_deref(), Some("/Users/me/app"));
        assert_eq!(work_folder(&[], &attachments), None);
        // As the row has it, a URI with a space.
        let uri = "file:///Users/me/Library/Application%20Support/Codealong/images";
        assert_eq!(work_folder(&[uri_path(uri).unwrap(), "/Users/me/app".into()], &attachments).as_deref(), Some("/Users/me/app"));
    }

    /// Each folder's table lists the other folders' conversations too (the command line's has
    /// the app's, without a status): a folder shows only its own.
    #[test]
    fn a_folder_shows_only_its_own_conversations() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(&format!(
            "{SCHEMA}\
             INSERT INTO conversation_summaries (conversation_id, last_modified_time, workspace_uris, status, last_user_input_time, app_data_dir) VALUES \
             ('app', '2026-09-27 15:21:54+00:00', '[]', '', '0001-01-01 00:00:00+00:00', 'antigravity'), \
             ('cli', '2026-10-01 16:31:04+00:00', '[]', 'CASCADE_RUN_STATUS_IDLE', '2026-10-01 16:31:02+00:00', 'antigravity-cli');"
        ))
        .unwrap();
        let ids = |dir: &str| read_from(&db, dir, 0, &HashSet::new()).unwrap().into_iter().map(|c| c.id).collect::<Vec<_>>();
        assert_eq!(ids("antigravity-cli"), ["cli"]);
        assert_eq!(ids("antigravity"), ["app"]);
        // gorm's zero time is no time.
        assert_eq!(read_from(&db, "antigravity", 0, &HashSet::new()).unwrap()[0].last_input, None);
    }

    #[test]
    fn reads_timestamps_and_booleans_however_stored() {
        assert_eq!(timestamp(&Sql::Text("2026-09-24 14:16:07.434615-07:00".into())), parse_rfc3339_ms("2026-09-24T21:16:07.434Z"));
        assert_eq!(timestamp(&Sql::Integer(1_790_285_194)), Some(1_790_285_194_000));
        assert_eq!(timestamp(&Sql::Integer(1_790_285_194_631)), Some(1_790_285_194_631));
        assert_eq!(timestamp(&Sql::Null), None);
        assert!(truthy(&Sql::Integer(1)) && truthy(&Sql::Text("true".into())));
        assert!(!truthy(&Sql::Integer(0)) && !truthy(&Sql::Text("false".into())) && !truthy(&Sql::Null));
        assert_eq!(uri_path("file:///Users/me/My%20Repo").as_deref(), Some("/Users/me/My Repo"));
        assert_eq!(uri_path("vscode-remote://ssh/x"), None);
    }

    #[test]
    fn the_command_lines_folder_goes_with_the_apps() {
        let both = folders(Path::new("/Users/me/.gemini/antigravity"));
        assert_eq!(both, [Folder { path: "/Users/me/.gemini/antigravity".into(), cli: false }, Folder { path: "/Users/me/.gemini/antigravity-cli".into(), cli: true }]);
        assert_eq!(folders(Path::new("/Users/me/.gemini/antigravity-cli")), [Folder { path: "/Users/me/.gemini/antigravity-cli".into(), cli: true }]);
        // A separately installed IDE keeps its own, and the command line's isn't read twice.
        assert_eq!(folders(Path::new("/Users/me/.gemini/antigravity-ide")).len(), 1);
    }

    // A conversation as `agy` 1.2.14 wrote it (transcript_full.jsonl), with two calls made
    // together, whose results are written before their own step.
    const LINES: &[&str] = &[
        r#"{"step_index":0,"source":"USER_EXPLICIT","type":"USER_INPUT","status":"DONE","created_at":"2026-10-01T16:55:10Z","content":"<USER_REQUEST>\nCount the lines, then fix notes.txt\n</USER_REQUEST>\n<ADDITIONAL_METADATA>\nThe current local time is: 2026-10-01T09:55:10-07:00.\n</ADDITIONAL_METADATA>"}"#,
        r#"{"step_index":1,"source":"MODEL","type":"PLANNER_RESPONSE","status":"DONE","created_at":"2026-10-01T16:55:14Z","content":"Counting first.","tool_calls":[{"name":"run_command","args":{"CommandLine":"wc -l a.txt","Cwd":"/Users/me/app","WaitMsBeforeAsync":5000,"toolAction":"Counting lines in a.txt","toolSummary":"Line count"}}]}"#,
        r#"{"step_index":2,"source":"MODEL","type":"GENERIC","status":"DONE","created_at":"2026-10-01T16:55:15Z","content":"Created At: 2026-10-01T09:55:14-07:00\nCompleted At: 2026-10-01T09:55:15-07:00\n\nThe command exited with code 0.\nOutput:\n       1 a.txt\r\n\n"}"#,
        r#"{"step_index":3,"source":"SYSTEM","type":"SYSTEM_MESSAGE","status":"DONE","created_at":"2026-10-01T16:55:15Z","content":"The following is a <SYSTEM_MESSAGE> not actually sent by the user."}"#,
        r#"{"step_index":5,"source":"MODEL","type":"GENERIC","status":"DONE","created_at":"2026-10-01T16:55:22Z","content":"Created At: 2026-10-01T09:55:22-07:00\nCompleted At: 2026-10-01T09:55:22-07:00\nThe following changes were made by the replace_file_content tool"}"#,
        r#"{"step_index":4,"source":"MODEL","type":"PLANNER_RESPONSE","status":"DONE","created_at":"2026-10-01T16:55:22Z","tool_calls":[{"name":"replace_file_content","args":{"TargetFile":"/Users/me/app-fix/notes.txt","ReplacementContent":"BETA","toolSummary":"Fix notes.txt"}},{"name":"run_command","args":{"CommandLine":"ls /definitely-not-here","Cwd":"/Users/me/app"}}]}"#,
        r#"{"step_index":6,"source":"MODEL","type":"GENERIC","status":"DONE","created_at":"2026-10-01T16:55:24Z","content":"Created At: 2026-10-01T09:55:24-07:00\nCompleted At: 2026-10-01T09:55:24-07:00\n\nThe command exited with code 1.\nOutput:\nls: /definitely-not-here: No such file or directory\n"}"#,
        r#"{"step_index":7,"source":"MODEL","type":"PLANNER_RESPONSE","status":"DONE","created_at":"2026-10-01T16:55:25Z","content":"Done: one line, and **notes.txt** is fixed."}"#,
    ];

    #[test]
    fn a_transcript_reads_as_prompts_replies_and_tools() {
        let items: Vec<Item> = LINES.iter().flat_map(|l| conversation_items(&serde_json::from_str(l).unwrap(), 0)).collect();
        let kinds: Vec<&str> = items.iter().map(|i| i.kind).collect();
        assert_eq!(kinds, ["you", "agent", "tool", "result", "result", "tool", "tool", "result", "agent"]);
        assert_eq!(items[0].text, "Count the lines, then fix notes.txt");
        assert_eq!(items[0].at, parse_rfc3339_ms("2026-10-01T16:55:10Z").unwrap());
        // A command can run again where it ran, and its output is what it printed.
        assert_eq!((items[2].text.as_str(), items[2].input.as_deref(), items[2].shell, items[2].cwd.as_deref()), ("Run command · `wc -l a.txt`", Some("wc -l a.txt"), true, Some("/Users/me/app")));
        assert_eq!((items[3].id.as_deref(), items[3].output.as_deref(), items[3].failed), (items[2].id.as_deref(), Some("       1 a.txt"), false));
        // Calls made together are answered by the steps after theirs, in their order.
        assert_eq!((items[5].text.as_str(), items[5].id.as_deref()), ("Replace file content · Fix notes.txt", Some("5")));
        assert!(items[5].input.as_deref().unwrap().contains("\"ReplacementContent\": \"BETA\"") && !items[5].input.as_deref().unwrap().contains("toolSummary"));
        assert_eq!((items[6].id.as_deref(), items[7].id.as_deref(), items[7].failed), (Some("6"), Some("6"), true));
        assert!(items[7].output.as_deref().unwrap().starts_with("The command exited with code 1."));
    }

    /// The transcript without `_full` writes each argument as a string of JSON.
    #[test]
    fn reads_arguments_written_as_json_strings() {
        let v = serde_json::json!({"step_index": 1, "source": "MODEL", "type": "PLANNER_RESPONSE", "status": "DONE", "created_at": "2026-10-01T16:31:49Z",
            "tool_calls": [{"name": "run_command", "args": {"CommandLine": "\"wc -l a.txt\"", "Cwd": "\"/Users/me/app\"", "WaitMsBeforeAsync": "5000"}}]});
        let items = conversation_items(&v, 0);
        assert_eq!((items[0].input.as_deref(), items[0].cwd.as_deref()), (Some("wc -l a.txt"), Some("/Users/me/app")));
    }

    /// An agent reads a repo it isn't working in with its file tools; only what it writes says
    /// where it works.
    #[test]
    fn looking_at_a_file_is_not_work_on_it() {
        let work = |name: &str, args: Value| call(&serde_json::json!({"name": name, "args": args}), 1).work;
        assert_eq!(work("view_file", serde_json::json!({"AbsolutePath": "/Users/me/app-fix/notes.txt"})), None);
        assert_eq!(work("list_dir", serde_json::json!({"DirectoryPath": "/Users/me/app-fix"})), None);
        assert_eq!(work("grep_search", serde_json::json!({"SearchPath": "/Users/me/app-fix", "Query": "beta"})), None);
        assert_eq!(work("write_to_file", serde_json::json!({"TargetFile": "/Users/me/app-fix/new.txt"})).as_deref(), Some("/Users/me/app-fix/new.txt"));
        // A view still says which file on its line.
        assert_eq!(call(&serde_json::json!({"name": "view_file", "args": {"AbsolutePath": "/Users/me/app-fix/notes.txt"}}), 1).label, "View file · `notes.txt`");
    }

    #[test]
    fn the_end_of_a_record_says_what_a_conversation_is_on() {
        let whole = tail_of(&LINES.join("\n"), Some("/Users/me/app"));
        assert_eq!(whole.last_text.as_deref(), Some("Done: one line, and notes.txt is fixed."));
        assert_eq!(whole.last_tool.as_deref(), Some("Run command · `ls /definitely-not-here`"));
        assert!(whole.open.is_empty(), "every call has its result");
        // Where it worked: commands in their folder, an edit in its file's.
        assert_eq!(whole.work, ["/Users/me/app", "/Users/me/app-fix/notes.txt"]);
        // Counting lines only looks, which is work only in the conversation's own folder.
        assert_eq!(tail_of(&LINES.join("\n"), Some("/Users/me/elsewhere")).work, ["/Users/me/app-fix/notes.txt"]);
        assert_eq!(tail_of(&LINES.join("\n"), None).work, ["/Users/me/app-fix/notes.txt"]);
        // Mid-turn, the call with no result yet is the one running, or waiting on you.
        let running = tail_of(&LINES[..6].join("\n"), None);
        assert_eq!(running.open.iter().map(|c| c.input.as_str()).collect::<Vec<_>>(), ["ls /definitely-not-here"]);
        assert_eq!(running.last_text.as_deref(), Some("Counting first."));
    }

    #[test]
    fn tells_the_terminal_ui_from_print_runs_and_other_commands() {
        assert_eq!(cli_args("/Users/me/.local/bin/agy"), Some(true));
        assert_eq!(cli_args("agy --model gemini-3.8-flash-low -i Run the tests models"), Some(true));
        assert_eq!(cli_args("agy --conversation 88db39ed-a2e3-4dd7-b9aa-0bf7bb66b35d"), Some(true));
        assert_eq!(cli_args("agy --model gemini-3.8-flash-low --input-format stream-json --output-format stream-json"), Some(false));
        assert_eq!(cli_args("agy --output-format=json -p Reply OK"), Some(false));
        assert_eq!(cli_args("agy models"), None);
        assert_eq!(cli_args("/Users/me/.local/bin/agy remote-control start"), None);
        assert_eq!(cli_args("/usr/bin/magyar"), None);
        assert!(is_app_agent("/Applications/Antigravity.app/Contents/Resources/bin/language_server --standalone --override_ide_name antigravity"));
        assert!(!is_app_agent("/Applications/Windsurf.app/Contents/Resources/bin/language_server --standalone"));
    }

    #[test]
    fn finds_the_conversation_a_process_has_open() {
        let files = "p61332\nfcwd\nn/Users/me/app\nf73\nn/Users/me/.gemini/antigravity-cli/conversations/0b86875e-96cc-4f1d-aebd-68814b6c553f.db\nf40\nn/Users/me/.gemini/antigravity-cli/presence\nf79\nn/Users/me/.gemini/antigravity-cli/presence/0b86875e-96cc-4f1d-aebd-68814b6c553f.lock\n";
        assert_eq!(held_conversation(files).as_deref(), Some("0b86875e-96cc-4f1d-aebd-68814b6c553f"));
        assert_eq!(held_conversation("p1\nfcwd\nn/Users/me/app\n"), None);
    }

    #[test]
    fn tells_an_idle_another_agy_wrote_from_a_turn_that_ended() {
        let running = Conversation { status: "cascade_run_status_running".into(), not_fully_idle: true, last_modified: 1_000, step_count: 5, ..Default::default() };
        let idle = Conversation { status: "cascade_run_status_idle".into(), not_fully_idle: false, ..running.clone() };
        assert!(idled_by_another(Some(&running), &idle));
        // Read again unchanged, it's still the other's doing.
        assert!(idled_by_another(Some(&Conversation { idled_by_another: true, ..idle.clone() }), &idle));
        // The other writes its own older copy of the row: the time and count go back.
        let older = Conversation { last_modified: 400, step_count: 2, ..idle.clone() };
        assert!(idled_by_another(Some(&running), &older));
        assert!(idled_by_another(Some(&Conversation { idled_by_another: true, ..idle.clone() }), &older));
        // Unless its latest step ends the turn: the copy landed on the ending's row.
        assert!(!idled_by_another(Some(&running), &Conversation { turn_ended: true, ..older.clone() }));
        // First read after the flip, a step that ran said the turn was under way: it still is
        // while the model thinks before its next step.
        assert!(idled_by_another(Some(&Conversation { step_under_way: true, ..idle.clone() }), &idle));
        // A turn that ends writes its latest step's time and count. A row that wasn't running
        // when last read, or wasn't read at all, is as it says.
        assert!(!idled_by_another(Some(&running), &Conversation { last_modified: 2_000, ..idle.clone() }));
        assert!(!idled_by_another(Some(&running), &Conversation { step_count: 6, ..idle.clone() }));
        assert!(!idled_by_another(Some(&idle), &idle));
        assert!(!idled_by_another(None, &idle));
        assert!(!idled_by_another(Some(&running), &running));
    }

    /// A folder read end to end: the summary table, a conversation's own database for the step
    /// that waits, and its transcript.
    #[test]
    fn reads_a_folder_as_the_command_line_leaves_it() {
        let root = crate::fixtures::scratch("antigravity");
        let dir = root.join("antigravity-cli");
        let logs = dir.join("brain/c1/.system_generated/logs");
        std::fs::create_dir_all(&logs).unwrap();
        std::fs::create_dir_all(dir.join("conversations")).unwrap();
        std::fs::write(logs.join("transcript_full.jsonl"), LINES[..6].join("\n") + "\n").unwrap();
        let db = Connection::open(dir.join(SUMMARIES)).unwrap();
        db.execute_batch(&format!(
            "{SCHEMA}\
             INSERT INTO conversation_summaries (conversation_id, preview, step_count, last_modified_time, workspace_uris, status, not_fully_idle, last_user_input_time, app_data_dir) VALUES \
             ('c1', 'Count the lines, then fix notes.txt', 6, '2026-10-01 16:55:24+00:00', '[\"file:///Users/me/app\"]', 'CASCADE_RUN_STATUS_RUNNING', 1, '2026-10-01 16:55:10+00:00', 'antigravity-cli');"
        ))
        .unwrap();
        drop(db);
        let steps = Connection::open(dir.join("conversations/c1.db")).unwrap();
        steps.execute_batch("CREATE TABLE steps (idx INTEGER PRIMARY KEY, step_type INTEGER NOT NULL DEFAULT 0, status INTEGER NOT NULL DEFAULT 0); INSERT INTO steps VALUES (0, 14, 3), (6, 132, 9);").unwrap();
        drop(steps);

        let account = AccountCfg { name: "antigravity".into(), harness: "antigravity".into(), home: root.join("antigravity").to_string_lossy().into_owned(), command: None, start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false };
        let seen = crate::state::SeenState::default();
        let look = Look { seen: &seen, since: 0, now: parse_rfc3339_ms("2026-10-01T16:56:00Z").unwrap(), ps: Default::default() };
        let mut reader = Reader::default();
        assert!(reader.reread(&account, &look), "a conversation that's new may need a lane");
        assert!(!reader.reread(&account, &look));
        let c = &reader.conversations["antigravity"][0];
        assert!(c.cli && c.waiting && c.dir == dir);
        assert_eq!(c.tail.open[0].input, "ls /definitely-not-here");

        // The panel reads its record, and what it waits on is the call with no result.
        let session = Session { surface: Surface::Cli, ..crate::board::antigravity_session(&account, c, None, false, 0, look.now) };
        let ctx = Ctx { account: &account };
        use crate::harness::Adapter as _;
        let page = Antigravity.conversation(&ctx, &session, None, 50).unwrap();
        assert_eq!(page.items.iter().map(|i| i.kind).collect::<Vec<_>>(), ["you", "agent", "tool", "result", "result", "tool", "tool"]);
        let waits = Antigravity.pending(&ctx, &session).unwrap();
        assert_eq!((waits.label.as_str(), waits.id.as_str(), waits.cannot.is_some()), ("Run command · `ls /definitely-not-here`", "6", true));
        // In a terminal UI it's answered here, with a key, and only the call that waits.
        let in_terminal = Session { pid: Some(999_999), ..session.clone() };
        assert_eq!(Antigravity.pending(&ctx, &in_terminal).unwrap().cannot, None);
        let answered = |id: &str, a: crate::harness::Answer| Antigravity.answer(&ctx, &in_terminal, id, a).unwrap_err().to_string();
        assert!(answered("5", crate::harness::Answer::Allow).starts_with("It's waiting on a different call now"));
        assert!(answered("6", crate::harness::Answer::Always).contains("Allow or Deny only"));
        assert!(answered("6", crate::harness::Answer::Allow).starts_with("It isn't in a terminal Codealong can press keys in"));
        assert!(Antigravity.answer(&ctx, &session, "6", crate::harness::Answer::Allow).unwrap_err().to_string().contains("answer it there"));
        // Calls run side by side: a key answers whichever prompt is up, so only the one step
        // that waits is answered, and only as the call it is.
        let steps = Connection::open(dir.join("conversations/c1.db")).unwrap();
        steps.execute_batch("INSERT INTO steps VALUES (7, 132, 9);").unwrap();
        assert!(answered("6", crate::harness::Answer::Allow).starts_with("It waits on more than one call"));
        steps.execute_batch("UPDATE steps SET status = 2 WHERE idx = 6;").unwrap();
        assert!(answered("6", crate::harness::Answer::Allow).starts_with("It's waiting on a different call now: another call."));
        steps.execute_batch("DELETE FROM steps WHERE idx = 7; UPDATE steps SET status = 9 WHERE idx = 6;").unwrap();
        drop(steps);
        assert_eq!(Antigravity.open_calls(&ctx, &session), ["ls /definitely-not-here"]);
        assert_eq!(Antigravity.records(&account, &session)[0], logs.join("transcript_full.jsonl"));

        // Its terminal UI has it when another `agy` starts and writes its row back idle, the rest
        // as it was: the step still waits on you.
        reader.processes = vec![Process { pid: 7, conversation: Some("c1".into()), terminal: true, logs: Vec::new() }];
        reader.looked_at = Some(Instant::now());
        let summaries = Connection::open(dir.join(SUMMARIES)).unwrap();
        summaries.execute("UPDATE conversation_summaries SET status = 'CASCADE_RUN_STATUS_IDLE', not_fully_idle = 0", []).unwrap();
        reader.reread(&account, &look);
        let c = &reader.conversations["antigravity"][0];
        assert!(c.idled_by_another && c.step_under_way && c.waiting);
        assert_eq!(crate::board::antigravity_session(&account, c, reader.processes.first(), false, 0, look.now).status, crate::model::Status::Needs);
        reader.reread(&account, &look);
        assert!(reader.conversations["antigravity"][0].idled_by_another);
        // You cancel the call, and the turn's end writes the time and count of its latest step.
        Connection::open(dir.join("conversations/c1.db")).unwrap().execute("UPDATE steps SET status = 6 WHERE idx = 6", []).unwrap();
        summaries.execute("UPDATE conversation_summaries SET last_modified_time = '2026-10-01 16:55:40+00:00', step_count = 7", []).unwrap();
        reader.reread(&account, &look);
        let c = &reader.conversations["antigravity"][0];
        assert!(!c.idled_by_another && !c.step_under_way && !c.waiting);
        assert_eq!(crate::board::antigravity_session(&account, c, reader.processes.first(), false, 0, look.now).status, crate::model::Status::Unread);
    }

    /// Read first after another `agy` marked it idle: a step that runs says the turn is under way,
    /// and it still is while the model thinks once the step is done, until the turn's end writes
    /// its row.
    #[test]
    fn a_turn_first_read_after_another_agy_marked_it_idle_goes_on() {
        let root = crate::fixtures::scratch("antigravity-idled");
        let dir = root.join("antigravity-cli");
        std::fs::create_dir_all(dir.join("conversations")).unwrap();
        let summaries = Connection::open(dir.join(SUMMARIES)).unwrap();
        summaries
            .execute_batch(&format!(
                "{SCHEMA}\
                 INSERT INTO conversation_summaries (conversation_id, preview, step_count, last_modified_time, workspace_uris, status, not_fully_idle, last_user_input_time, app_data_dir) VALUES \
                 ('c1', 'Fix the build', 3, '2026-10-01 16:55:24+00:00', '[]', 'CASCADE_RUN_STATUS_IDLE', 0, '2026-10-01 16:55:10+00:00', 'antigravity-cli');"
            ))
            .unwrap();
        let steps = Connection::open(dir.join("conversations/c1.db")).unwrap();
        steps.execute_batch("CREATE TABLE steps (idx INTEGER PRIMARY KEY, step_type INTEGER NOT NULL DEFAULT 0, status INTEGER NOT NULL DEFAULT 0); INSERT INTO steps VALUES (0, 14, 3), (1, 15, 3), (2, 132, 2);").unwrap();

        let account = AccountCfg { name: "antigravity".into(), harness: "antigravity".into(), home: dir.to_string_lossy().into_owned(), command: None, start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false };
        let seen = crate::state::SeenState::default();
        let look = Look { seen: &seen, since: 0, now: parse_rfc3339_ms("2026-10-01T16:56:00Z").unwrap(), ps: Default::default() };
        let mut reader = Reader { processes: vec![Process { pid: 7, conversation: Some("c1".into()), terminal: true, logs: Vec::new() }], looked_at: Some(Instant::now()), ..Reader::default() };
        let status = |reader: &Reader| crate::board::antigravity_session(&account, &reader.conversations["antigravity"][0], reader.processes.first(), false, 0, look.now).status;
        reader.reread(&account, &look);
        assert_eq!(status(&reader), crate::model::Status::Working);
        steps.execute("UPDATE steps SET status = 3 WHERE idx = 2", []).unwrap();
        reader.reread(&account, &look);
        assert!(reader.conversations["antigravity"][0].idled_by_another);
        assert_eq!(status(&reader), crate::model::Status::Working);
        // Another `agy` starts and writes back its own older copy of the row.
        summaries.execute("UPDATE conversation_summaries SET last_modified_time = '2026-10-01 16:55:12+00:00', step_count = 1", []).unwrap();
        reader.reread(&account, &look);
        assert_eq!(status(&reader), crate::model::Status::Working);
        // The turn ends with the model's reply, and the older copy lands on the ending's row
        // before it's read.
        steps.execute("INSERT INTO steps VALUES (3, 15, 3)", []).unwrap();
        reader.reread(&account, &look);
        assert!(!reader.conversations["antigravity"][0].idled_by_another);
        assert_eq!(status(&reader), crate::model::Status::Unread);
        // A call that fails doesn't end it: the model goes on with the error.
        steps.execute("INSERT INTO steps VALUES (4, 14, 3), (5, 132, 7)", []).unwrap();
        summaries.execute("UPDATE conversation_summaries SET last_modified_time = '2026-10-01 16:55:12+00:00', step_count = 1, status = 'CASCADE_RUN_STATUS_RUNNING', not_fully_idle = 1", []).unwrap();
        reader.reread(&account, &look);
        summaries.execute("UPDATE conversation_summaries SET status = 'CASCADE_RUN_STATUS_IDLE', not_fully_idle = 0", []).unwrap();
        reader.reread(&account, &look);
        assert_eq!(status(&reader), crate::model::Status::Working);
        // One that ends on an error ends with a note.
        steps.execute("INSERT INTO steps VALUES (6, 17, 3)", []).unwrap();
        reader.reread(&account, &look);
        assert_eq!(status(&reader), crate::model::Status::Unread);
        summaries.execute("UPDATE conversation_summaries SET last_modified_time = '2026-10-01 16:55:50+00:00', step_count = 7", []).unwrap();
        reader.reread(&account, &look);
        assert_eq!(status(&reader), crate::model::Status::Unread);
    }

    /// The lines of the log of the session that waited 31 minutes, 2026-10-14, the rest of them
    /// what Escape and a failed send write.
    #[test]
    fn reads_what_a_terminal_uis_log_says() {
        let mut said = Said::default();
        let read = |said: &mut Said, line: String| said.on_line(&line, (2026, 10));
        read(&mut said, glog("17:23:24.339615", "Starting language server process with pid 61216"));
        read(&mut said, glog("17:23:30.603528", "Forwarding user message to conversation b568 (items=1, media=0)"));
        assert!(said.started("b568") && !said.started("other") && said.queued.is_empty());
        // Typed while the turn runs: held.
        read(&mut said, glog("17:45:37.928030", "HandleUserInput called with text: \"We also want to take into account account limits\""));
        read(&mut said, "W1014 17:45:38.000000      56 browser.go:256] token refresh failed due to network error".to_string());
        assert_eq!(said.queued, [on_14th("17:45:37.928")]);
        // The turn ends on an error, and the UI sends what it held.
        read(&mut said, glog("18:16:39.544711", "Auto-flush: sending 1 queued input(s) (combinedLength=299, media=0, oneShotModel=\"\")"));
        read(&mut said, glog("18:16:39.545337", "Forwarding user message to conversation b568 (items=1, media=0)"));
        assert!(said.queued.is_empty());
        assert_eq!(said.prompted, Some(("b568".to_string(), on_14th("18:16:39.545"))));
        // A send that fails holds them again.
        read(&mut said, glog("18:16:39.600000", "Auto-flush SendUserMessage failed, restoring 1 pending input(s): unavailable"));
        assert_eq!(said.queued, [on_14th("17:45:37.928")]);
        read(&mut said, glog("18:17:00.000000", "Pending inputs cancelled (1 regular + 0 shell dropped)"));
        assert!(said.queued.is_empty());
        // Escape stops the turn.
        assert!(!said.stopped_last());
        read(&mut said, glog("18:53:07.619129", "Cancelling in-progress response for conversation b568"));
        assert!(said.stopped_last() && !said.started("b568"));
        // A message that quotes the log is a message, whatever line it quotes.
        read(&mut said, glog("18:53:20.000000", "HandleUserInput called with text: \"Please inspect ] Forwarding user message to conversation b568 (items=1, media=0)\""));
        read(&mut said, glog("18:53:21.000000", "HandleUserInput called with text: \"and ] Cancelling in-progress response for conversation b568\""));
        assert_eq!(said.queued, [on_14th("18:53:20.000"), on_14th("18:53:21.000")]);
        assert_eq!((said.prompted.as_ref().map(|(_, at)| *at), said.stopped), (Some(on_14th("18:16:39.545")), Some(on_14th("18:53:07.619"))));
        read(&mut said, glog("18:53:22.000000", "Pending inputs cancelled (2 regular + 0 shell dropped)"));
        // A `/` command isn't taken for a message held.
        read(&mut said, glog("18:53:30.000000", "HandleUserInput called with text: \"/usage\""));
        assert!(said.queued.is_empty());
        // A message typed now goes in at once, and starts the next.
        read(&mut said, glog("18:54:00.000000", "HandleUserInput called with text: \"Go on\""));
        read(&mut said, glog("18:54:00.004000", "Forwarding user message to conversation b568 (items=1, media=0)"));
        assert!(said.started("b568") && !said.stopped_last() && said.queued.is_empty());
        // Lines that aren't glog's tell nothing.
        let before = said.clone();
        said.on_line("2026/10/14 17:45:47 http: TLS handshake error from 127.0.0.1:57439", (2026, 10));
        said.on_line("CLI ready for user input", (2026, 10));
        assert_eq!(said, before);
    }

    #[test]
    fn reads_a_log_lines_time_on_this_macs_clock() {
        assert_eq!(log_time("I1014 17:45:37.928030   22584 input_loop.go:106] x", (2026, 10)), Some(on_14th("17:45:37.928")));
        // A log begun in December goes on into January of the next year.
        assert_eq!(log_time("E0101 00:00:01.000000 1 a.go:1] x", (2026, 12)), crate::util::local_ms(2027, 1, 1, 0, 0, 1, 0));
        assert_eq!(log_time("2026/10/14 17:45:47 http: x", (2026, 10)), None);
        assert_eq!(log_begun(Path::new("/x/log/cli-20261014_172324.log")), Some((2026, 10)));
        assert_eq!(log_begun(Path::new("/x/log/crash_61216.log")), None);
    }

    /// A UI holds another `agy`'s log open beside its own: its own is the one that names it.
    #[test]
    fn finds_the_log_a_process_writes() {
        let dir = crate::fixtures::scratch("agy-logs").join("log");
        std::fs::create_dir_all(&dir).unwrap();
        let head = |pid: u32| format!("I1014 17:23:24.338830       1 resolver.go:85] Model resolved\nI1014 17:23:24.339615      24 server.go:1633] Starting language server process with pid {pid}\n");
        std::fs::write(dir.join("cli-20261014_172324.log"), head(61216)).unwrap();
        std::fs::write(dir.join("cli-20261014_183955.log"), head(11764)).unwrap();
        let at = |name: &str| dir.join(name).to_string_lossy().into_owned();
        let files = format!(
            "p61216\nf3\nn{}\nf4\nn{}\nf5\nn{}\nf6\nn/Users/me/.gemini/antigravity-cli/crashes/crash_61216.log\nf7\nn/Users/me/.gemini/antigravity-cli/presence/b568.lock\n",
            at("cli-20261014_172324.log"),
            at("cli-20261014_183955.log"),
            at("cli-20261014_172324.log")
        );
        let logs = open_logs(&files);
        assert_eq!(logs, [dir.join("cli-20261014_172324.log"), dir.join("cli-20261014_183955.log")]);
        assert_eq!(own_log(&logs, 61216), Some(dir.join("cli-20261014_172324.log")));
        assert_eq!(own_log(&logs, 11764), Some(dir.join("cli-20261014_183955.log")));
        assert_eq!(own_log(&logs, 6121), None);
    }

    #[test]
    fn only_the_error_note_with_words_for_you_ends_a_turn() {
        assert!(ends_turn(Some(&error_note("agent executor error: request failed", Some("Agent execution terminated due to error.")))));
        assert!(!ends_turn(Some(&error_note("API error (attempt 1): request failed", None))));
        assert!(!ends_turn(Some(&error_note("The stream was interrupted. Please continue.", None))));
        assert!(ends_turn(None));
    }

    /// The notes that ended turns on this Mac (2026-10-14 and 15), with other addresses.
    #[test]
    fn a_failed_turns_note_says_why() {
        let broken = error_note("agent executor error: generating and executing: request failed: Post \"https://daily-cloudcode-pa.googleapis.com/v1internal:streamGenerateContent?alt=sse\": write tcp [2001:db8::1]:60363->[2001:db8::2]:443: write: broken pipe", Some(TERMINATED));
        assert_eq!(failed_of(&broken), Some((FailedOn::Connection, "Its request to the API failed: broken pipe".to_string())));
        let lookup = error_note("agent executor error: request failed: Post \"https://daily-cloudcode-pa.googleapis.com/v1internal:streamGenerateContent?alt=sse\": Post \"https://oauth2.googleapis.com/token\": dial tcp: lookup oauth2.googleapis.com: no such host", Some(TERMINATED));
        assert_eq!(failed_of(&lookup), Some((FailedOn::Connection, "Its request to the API failed: no such host".to_string())));
        // The quota, until the time its details give, without the countdown from when it was written.
        let said = "Individual quota reached. Please upgrade your subscription to increase your limits. Resets in 2h14m8s.";
        let quota = error_note_with(
            &format!("RESOURCE_EXHAUSTED (code 429): {said}"),
            Some(said),
            &[
                r#"{"@type":"type.googleapis.com/google.rpc.ErrorInfo","reason":"QUOTA_EXHAUSTED","domain":"cloudcode-pa.googleapis.com","metadata":{"model":"gemini-4-argon-high","quotaResetDelay":"2h14m8.934659116s","quotaResetTimeStamp":"2026-10-15T07:08:42Z","uiMessage":"true"}}"#,
                r#"{"@type":"type.googleapis.com/google.rpc.RetryInfo","retryDelay":"8048.934659116s"}"#,
            ],
        );
        let resets_at = parse_rfc3339_ms("2026-10-15T07:08:42Z");
        assert_eq!(failed_of(&quota), Some((FailedOn::Quota { resets_at }, "Individual quota reached. Please upgrade your subscription to increase your limits.".to_string())));
        // Anything else says what went wrong, where the words for you say only that it failed.
        let missing = "NOT_FOUND (code 404): models/barium-b-agy is not found for API version v1main, or is not supported for generateContent.";
        assert_eq!(failed_of(&error_note(missing, Some(TERMINATED))), Some((FailedOn::Other, missing.to_string())));
        // A refused connection doesn't come right by waiting. Its cause is said whole, where the
        // address it went to would take the line.
        let refused = error_note("agent executor error: generating and executing: request failed: Post \"https://daily-cloudcode-pa.googleapis.com/v1internal:streamGenerateContent?alt=sse\": dial tcp [2001:db8::2]:443: connect: connection refused", Some(TERMINATED));
        assert_eq!(failed_of(&refused), Some((FailedOn::Other, "Its request to the API failed: dial tcp [2001:db8::2]:443: connect: connection refused".to_string())));
        // A note the agent goes on after ends no turn.
        assert_eq!(failed_of(&error_note("API error (attempt 1): request failed: Post \"https://oauth2.googleapis.com/token\": dial tcp: lookup oauth2.googleapis.com: no such host", None)), None);
    }

    /// The note a failed turn ends with is read from the conversation's own database, which the
    /// transcript never has, and is behind it once the conversation goes on.
    #[test]
    fn reads_how_the_latest_turn_failed() {
        let root = crate::fixtures::scratch("antigravity-failed");
        let dir = root.join("antigravity-cli");
        std::fs::create_dir_all(dir.join("conversations")).unwrap();
        let summaries = Connection::open(dir.join(SUMMARIES)).unwrap();
        summaries
            .execute_batch(&format!(
                "{SCHEMA}\
                 INSERT INTO conversation_summaries (conversation_id, preview, step_count, last_modified_time, workspace_uris, status, last_user_input_time, app_data_dir) VALUES \
                 ('c1', 'Slim chips', 3, '2026-10-15 00:52:58+00:00', '[]', 'CASCADE_RUN_STATUS_IDLE', '2026-10-15 00:47:00+00:00', 'antigravity-cli');"
            ))
            .unwrap();
        let steps = Connection::open(dir.join("conversations/c1.db")).unwrap();
        steps.execute_batch("CREATE TABLE steps (idx INTEGER PRIMARY KEY, step_type INTEGER NOT NULL DEFAULT 0, status INTEGER NOT NULL DEFAULT 0, step_payload BLOB); INSERT INTO steps (idx, step_type, status) VALUES (0, 14, 3), (1, 132, 3);").unwrap();
        let at = 1_792_025_577_777;
        let note = [began(at), error_note("agent executor error: request failed: Post \"https://daily-cloudcode-pa.googleapis.com/\": write tcp [2001:db8::1]:60363->[2001:db8::2]:443: write: broken pipe", Some(TERMINATED))].concat();
        steps.execute("INSERT INTO steps VALUES (2, 17, 3, ?1)", [note]).unwrap();

        let account = AccountCfg { name: "antigravity".into(), harness: "antigravity".into(), home: dir.to_string_lossy().into_owned(), command: None, start: None, usage_cap: None, acp: None, monthly: None, per_token: false, disabled: false };
        let seen = crate::state::SeenState::default();
        let look = Look { seen: &seen, since: 0, now: at + 60_000, ps: Default::default() };
        let mut reader = Reader { processes: vec![Process { pid: 7, conversation: Some("c1".into()), terminal: true, logs: Vec::new() }], looked_at: Some(Instant::now()), ..Reader::default() };
        reader.reread(&account, &look);
        assert_eq!(reader.conversations["antigravity"][0].failed, Some(Failed { on: FailedOn::Connection, at, said: "Its request to the API failed: broken pipe".into() }));
        // Told to carry on, it goes on.
        steps.execute("INSERT INTO steps (idx, step_type, status) VALUES (3, 14, 3)", []).unwrap();
        reader.reread(&account, &look);
        assert_eq!(reader.conversations["antigravity"][0].failed, None);
    }

    /// The session that waited, read from its folder: a turn its UI started goes on, whatever the
    /// summary says, while the model thinks and while a request is tried again; a message typed
    /// meanwhile is held; and Escape ends it.
    #[test]
    fn a_terminal_uis_log_says_what_its_files_dont() {
        let root = crate::fixtures::scratch("antigravity-log");
        let dir = root.join("antigravity-cli");
        std::fs::create_dir_all(dir.join("conversations")).unwrap();
        std::fs::create_dir_all(dir.join("log")).unwrap();
        // Another `agy` wrote the row back idle from its copy of the conversation's start.
        let summaries = Connection::open(dir.join(SUMMARIES)).unwrap();
        let started = in_utc(on_14th("17:23:30.000"));
        summaries
            .execute_batch(&format!(
                "{SCHEMA}\
                 INSERT INTO conversation_summaries (conversation_id, preview, step_count, last_modified_time, workspace_uris, status, not_fully_idle, last_user_input_time, app_data_dir) VALUES \
                 ('c1', 'Review automation', 1, '{started}', '[]', 'CASCADE_RUN_STATUS_IDLE', 0, '{started}', 'antigravity-cli');"
            ))
            .unwrap();
        let steps = Connection::open(dir.join("conversations/c1.db")).unwrap();
        steps.execute_batch("CREATE TABLE steps (idx INTEGER PRIMARY KEY, step_type INTEGER NOT NULL DEFAULT 0, status INTEGER NOT NULL DEFAULT 0, step_payload BLOB); INSERT INTO steps (idx, step_type, status) VALUES (0, 14, 3), (1, 15, 3), (2, 132, 3);").unwrap();
        let log = dir.join("log/cli-20261014_172324.log");
        let lines = |l: &[String]| l.iter().map(|l| format!("{l}\n")).collect::<String>();
        std::fs::write(&log, lines(&[glog("17:23:24.339615", "Starting language server process with pid 7"), glog("17:23:30.603528", "Forwarding user message to conversation c1 (items=1, media=0)")])).unwrap();
        let append = |l: String| {
            use std::io::Write as _;
            std::fs::OpenOptions::new().append(true).open(&log).unwrap().write_all(format!("{l}\n").as_bytes()).unwrap();
        };

        let account = AccountCfg { name: "antigravity".into(), harness: "antigravity".into(), home: dir.to_string_lossy().into_owned(), command: None, start: None, usage_cap: None, acp: None, monthly: None, per_token: false, disabled: false };
        let seen = crate::state::SeenState::default();
        let look = Look { seen: &seen, since: 0, now: on_14th("17:50:00.000"), ps: Default::default() };
        let mut reader = Reader { processes: vec![Process { pid: 7, conversation: Some("c1".into()), terminal: true, logs: vec![log.clone()] }], looked_at: Some(Instant::now()), ..Reader::default() };
        reader.track_logs();
        let session = |reader: &Reader| crate::board::antigravity_session(&account, &reader.conversations["antigravity"][0], reader.processes.first(), false, 0, look.now);
        // First read with the model thinking after a call: no step runs, the row says idle, and
        // nothing was read before.
        reader.reread(&account, &look);
        assert_eq!(session(&reader).status, crate::model::Status::Working);
        // You type while it works: only the log says so, and a glance hears it.
        append(glog("17:45:37.928030", "HandleUserInput called with text: \"We also want limits\""));
        assert!(crate::harness::Reader::glance(&mut reader, std::slice::from_ref(&account), &look).dirty);
        let s = session(&reader);
        assert_eq!((s.status, s.queued_at), (crate::model::Status::Working, Some(on_14th("17:45:37.928"))));
        assert!(!crate::harness::Reader::glance(&mut reader, std::slice::from_ref(&account), &look).dirty);
        // The turn ends on an error, and the UI sends what it held: a request the agent tries
        // again leaves a note it goes on after.
        steps.execute("INSERT INTO steps VALUES (3, 17, 3, ?1)", [error_note("agent executor error", Some("Agent execution terminated due to error."))]).unwrap();
        append(glog("18:16:39.545337", "Forwarding user message to conversation c1 (items=1, media=0)"));
        steps.execute("INSERT INTO steps VALUES (4, 14, 3, NULL), (5, 17, 3, ?1)", [error_note("API error (attempt 1): request failed", None)]).unwrap();
        reader.reread(&account, &look);
        let s = session(&reader);
        assert_eq!((s.status, s.queued_at, s.last_prompt_at), (crate::model::Status::Working, None, on_14th("18:16:39.545")));
        // A command runs, and once it's done the model thinks again.
        steps.execute("INSERT INTO steps VALUES (6, 132, 2, ?1)", [began(on_14th("18:20:00.000"))]).unwrap();
        reader.reread(&account, &look);
        steps.execute("UPDATE steps SET status = 3 WHERE idx = 6", []).unwrap();
        reader.reread(&account, &look);
        assert!(reader.conversations["antigravity"][0].idled_by_another);
        assert_eq!(session(&reader).status, crate::model::Status::Working);
        // Escape while the model thinks writes no step, and the row stays idle: only the log
        // says the turn ended. What it did since your prompt is yours to read, though the row is
        // a copy from before it: its latest step says when that was.
        append(glog("18:53:07.619129", "Cancelling in-progress response for conversation c1"));
        reader.reread(&account, &look);
        assert!(!reader.conversations["antigravity"][0].idled_by_another);
        let s = session(&reader);
        assert_eq!((s.status, s.last_activity), (crate::model::Status::Unread, on_14th("18:20:00.000")));
        // A process gone takes its log with it.
        reader.processes.clear();
        reader.track_logs();
        assert!(reader.logs.is_empty());
    }

    #[test]
    fn starts_and_resumes_in_its_terminal_ui() {
        use crate::harness::Adapter as _;
        let a = AccountCfg { name: "antigravity".into(), harness: "antigravity".into(), home: "~/.gemini/antigravity".into(), command: None, start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false };
        let ask = |prompt: &'static str, model: Option<&'static str>| crate::harness::StartAsk { prompt, session_id: "unused", model, effort: None, worktree: false, images_dir: Path::new("/i"), files_dir: Path::new("/f") };
        // With nothing to say it only opens; it names the conversation itself.
        let bare = Antigravity.start_in_terminal(&a, &ask("", None));
        assert_eq!((bare.input.as_str(), bare.env.len(), bare.key), ("agy\n", 0, None));
        // The prompt goes in the environment, so nothing in it reaches the shell.
        let typed = Antigravity.start_in_terminal(&a, &ask("  -v $(say nope) `x`  ", Some("gemini-3.8-flash-low")));
        assert_eq!(typed.input, format!("agy --model 'gemini-3.8-flash-low' -i \"${}\"\n", crate::start::PROMPT_VAR));
        assert_eq!(typed.env, vec![(crate::start::PROMPT_VAR.to_string(), "-v $(say nope) `x`".to_string())]);
        // An image goes as a path, with its folder added so it's read without asking.
        let image = Antigravity.start_in_terminal(&a, &ask("look\n![image](images/0123456789abcdef.png)", None));
        assert_eq!(image.input, format!("agy --add-dir \"${}\" -i \"${}\"\n", crate::start::IMAGES_VAR, crate::start::PROMPT_VAR));
        assert!(image.env.contains(&(crate::start::IMAGES_VAR.to_string(), "/i".to_string())) && image.env.iter().any(|(k, v)| k == crate::start::PROMPT_VAR && v.contains("/i/0123456789abcdef.png")));
        // An account's own command starts and resumes it.
        let own = AccountCfg { command: Some("agy --sandbox".into()), ..a.clone() };
        assert_eq!(Antigravity.start_in_terminal(&own, &ask("", None)).input, "agy --sandbox\n");
        let conversation = Session::new(Harness::Antigravity, &own.name, "88db39ed");
        let (input, env) = Antigravity.resume(&own, &conversation, &ask("-v please", None)).unwrap();
        assert_eq!(input, format!("agy --sandbox --conversation 88db39ed -i \"${}\"\n", crate::start::PROMPT_VAR));
        assert_eq!(env, vec![(crate::start::PROMPT_VAR.to_string(), "-v please".to_string())]);
        // On another model, or with nothing to say yet: it opens on the conversation.
        assert_eq!(Antigravity.resume(&own, &conversation, &ask("", Some("gemini-3.8-flash-low"))).unwrap(), ("agy --sandbox --conversation 88db39ed --model 'gemini-3.8-flash-low'\n".to_string(), Vec::new()));
    }

    #[test]
    fn trusts_a_folder_by_its_own_path_keeping_the_rest_of_the_settings() {
        let dir = crate::fixtures::scratch("agy-trust");
        let home = dir.join("home");
        let folder = home.join("r/app");
        std::fs::create_dir_all(&folder).unwrap();
        let folder = std::fs::canonicalize(&folder).unwrap();
        let settings = dir.join("settings.json");
        // As the command line writes it.
        let written = "{\n  \"model\": \"Gemini 3.8 Flash (High)\",\n  \"permissions\": {\n    \"allow\": [\n      \"command(pwd)\"\n    ]\n  },\n  \"trustedWorkspaces\": [\n    \"/r/other\"\n  ]\n}\n";
        std::fs::write(&settings, written).unwrap();
        assert!(trust_folder(&settings, &folder, &home).unwrap());
        let text = std::fs::read_to_string(&settings).unwrap();
        let want = written.replace("\"/r/other\"\n", &format!("\"/r/other\",\n    \"{}\"\n", folder.display()));
        assert_eq!(text, want);
        // Trusted already: the file is left alone.
        assert!(!trust_folder(&settings, &folder, &home).unwrap());
        // A link to it is trusted by the path it's started with, and by where it leads.
        let link = home.join("link");
        std::os::unix::fs::symlink(&folder, &link).unwrap();
        assert!(trust_folder(&settings, &link, &home).unwrap());
        let listed: Value = serde_json::from_str(&std::fs::read_to_string(&settings).unwrap()).unwrap();
        assert_eq!(listed["trustedWorkspaces"].as_array().unwrap().len(), 3);
        // None yet, and a file that isn't there.
        assert_eq!(with_trusted("{\"model\": \"m\"}", &["/r/a".into()]).unwrap().as_deref(), Some("{\n  \"model\": \"m\",\n  \"trustedWorkspaces\": [\n    \"/r/a\"\n  ]\n}"));
        let fresh = dir.join("fresh.json");
        assert!(trust_folder(&fresh, &folder, &home).unwrap());
        assert!(std::fs::read_to_string(&fresh).unwrap().ends_with("]\n}\n"));
        // Never the home folder or above, nor a file that doesn't read.
        assert!(trust_folder(&settings, &home, &home).is_err());
        assert!(trust_folder(&settings, &dir, &home).is_err());
        std::fs::write(&settings, "{ not json").unwrap();
        assert!(trust_folder(&settings, &folder, &home).is_err());
        assert_eq!(std::fs::read_to_string(&settings).unwrap(), "{ not json");
    }

    #[test]
    fn reads_the_permissions_its_latest_turn_ran_with() {
        let message = proto_message;
        let number = proto_number;
        let step = |policy: u64, asks: &[&str]| {
            let grants = message(&asks.iter().map(|a| (3, a.as_bytes().to_vec())).collect::<Vec<_>>());
            let tools = message(&[(8, message(&[(3, number(6, policy))])), (42, message(&[(3, grants)]))]);
            message(&[(19, message(&[(12, message(&[(1, message(&[(13, tools)]))]))]))])
        };
        let dir = crate::fixtures::scratch("agy-mode");
        std::fs::create_dir_all(dir.join("conversations")).unwrap();
        let db = Connection::open(dir.join("conversations/c1.db")).unwrap();
        db.execute_batch("CREATE TABLE steps (idx INTEGER PRIMARY KEY, step_type INTEGER NOT NULL DEFAULT 0, status INTEGER NOT NULL DEFAULT 0, step_payload BLOB);").unwrap();
        let insert = |idx: i64, kind: i64, payload: Vec<u8>| db.execute("INSERT INTO steps VALUES (?1, ?2, 3, ?3)", rusqlite::params![idx, kind, payload]).unwrap();
        insert(0, STEP_USER_INPUT, step(3, &[]));
        insert(1, 132, step(1, &[]));
        let mode = || turn_mode(&dir, "c1").map(|m| (m.kind, m.label));
        assert_eq!(mode(), Some((ModeKind::Skips, "Always proceed".to_string())));
        // The latest prompt's settings are the ones that hold.
        insert(2, STEP_USER_INPUT, step(1, &["write_file(/)"]));
        assert_eq!(mode(), Some((ModeKind::Asks, "Request review".to_string())));
        insert(3, STEP_USER_INPUT, step(1, &["command(*)", "read_file(/)", "write_file(/)"]));
        assert_eq!(mode(), Some((ModeKind::Asks, "Strict".to_string())));
        assert_eq!(turn_mode(&dir, "gone"), None);
        assert_eq!(proto_fields(&[0x08]), []);
    }

    #[test]
    fn reads_the_model_its_latest_turn_ran_on() {
        let message = proto_message;
        // The model among the row's other settings, as a real row has them.
        let row = |model: &str| message(&[(2, b"/Users/sam/app".to_vec()), (10, message(&[(1, message(&[(3, b"enable-programmatic-tools".to_vec()), (28, model.as_bytes().to_vec())]))]))]);
        let dir = crate::fixtures::scratch("agy-model");
        std::fs::create_dir_all(dir.join("conversations")).unwrap();
        let db = Connection::open(dir.join("conversations/c1.db")).unwrap();
        db.execute_batch("CREATE TABLE executor_metadata (idx INTEGER PRIMARY KEY, data BLOB);").unwrap();
        assert_eq!(turn_model(&dir, "c1"), None, "no turn yet");
        let insert = |idx: i64, data: Vec<u8>| db.execute("INSERT INTO executor_metadata VALUES (?1, ?2)", rusqlite::params![idx, data]).unwrap();
        insert(0, row("gemini-3.8-flash-high"));
        insert(1, row("gemini-3.8-flash-low"));
        assert_eq!(turn_model(&dir, "c1").as_deref(), Some("gemini-3.8-flash-low"), "the latest turn's");
        insert(2, row("not a model\n"));
        assert_eq!(turn_model(&dir, "c1"), None);
        assert_eq!(turn_model(&dir, "gone"), None);
    }

    /// A list is kept for the hour; a read that got none is asked for again in a minute, the
    /// list before it kept; nothing is asked twice while an answer is awaited.
    #[test]
    fn models_are_asked_again_soon_after_a_read_that_got_none() {
        let model = |id: &str| crate::start::ModelOption { id: id.into(), label: id.into() };
        let t = Instant::now();
        let s = |n: u64| t + Duration::from_secs(n);
        let mut kept = KeptModels { list: vec![model("kept-from-last-launch")], ask_at: None };
        assert!(kept.on_wanted(t), "the first want asks");
        assert!(!kept.on_wanted(s(10)), "not again while it's asked");
        // It got none: the last launch's list stays, and it's asked again a minute on.
        assert!(!kept.on_answer(s(30), Vec::new()));
        assert_eq!(kept.list, [model("kept-from-last-launch")]);
        assert!(!kept.on_wanted(s(80)));
        assert!(kept.on_wanted(s(90)));
        assert!(kept.on_answer(s(92), vec![model("gemini")]));
        assert_eq!(kept.list, [model("gemini")]);
        assert!(!kept.on_wanted(s(92 + 3599)));
        assert!(kept.on_wanted(s(92 + 3600)), "a list is good for the hour");
    }

    #[test]
    fn reads_the_models_the_command_line_lists() {
        let out = "Fetching available models...\ngemini-3.8-flash-low\tGemini 3.8 Flash (Low)\nclaude-sonnet-4-6\tClaude Sonnet 4.6 (Thinking)\nnot a model line\nbad id;rm\tNope\n";
        let listed: Vec<(String, String)> = listed_models(out).into_iter().map(|m| (m.id, m.label)).collect();
        assert_eq!(listed, [("gemini-3.8-flash-low".to_string(), "Gemini 3.8 Flash (Low)".to_string()), ("claude-sonnet-4-6".to_string(), "Claude Sonnet 4.6 (Thinking)".to_string())]);
    }

    #[test]
    fn reads_the_limits_usage_answers() {
        use crate::model::Severity;
        // `agy -p /usage --output-format json` on 1.3.4 (2026-10-13), a weekly limit per group.
        let out = r#"{"conversation_id":"","status":"SUCCESS","response":"Gemini Models\tWeekly Limit Remaining\t97%\t2026-10-20T21:48:07Z\n","duration_seconds":0,"num_turns":0,"usage":{"input_tokens":0,"output_tokens":0,"thinking_tokens":0,"cache_read_tokens":0,"total_tokens":0},"command":{"name":"usage","data":{"description":"Within each group, models share a weekly limit.","groups":[{"name":"Gemini Models","description":"Models within this group: Gemini Flash, Gemini Pro","buckets":[{"id":"gemini-weekly","name":"Weekly Limit Remaining","window":"weekly","remaining_fraction":0.08,"reset_time":"2026-10-20T21:48:07Z"}]},{"name":"Claude and GPT models","description":"Models within this group: Claude Opus, Claude Sonnet, GPT-OSS","buckets":[{"id":"3p-weekly","name":"Weekly Limit Remaining","window":"weekly","remaining_fraction":1,"reset_time":"2026-10-20T22:14:23Z"}]}]}}}"#;
        let UsageAnswer::Limits(meters) = usage_answer(&format!("Checking for updates...\n{out}\n")) else { panic!("limits") };
        let shown: Vec<(&str, &str, &str, i64, Option<i64>, Severity, Option<&str>)> = meters.iter().map(|m| (m.key.as_str(), m.label.as_str(), m.window.as_str(), m.percent.round() as i64, m.resets_at, m.severity, m.model.as_deref())).collect();
        assert_eq!(
            shown,
            [
                ("model:Gemini", "Gemini", "Weekly, Gemini models", 92, parse_rfc3339_ms("2026-10-20T21:48:07Z"), Severity::Critical, Some("Gemini")),
                // Untouched: its reset is only a week from the asking, so it shows none.
                ("model:Claude & GPT", "Claude & GPT", "Weekly, Claude & GPT models", 0, None, Severity::Normal, Some("Claude & GPT")),
            ]
        );
        // Signed out and nobody there to sign in, it says so once it's ended.
        assert_eq!(usage_answer(r#"{"conversation_id":"","status":"ERROR","response":"","error":"authentication failed or timed out","num_turns":0}"#), UsageAnswer::SignedOut);
        assert_eq!(usage_answer(r#"{"status":"ERROR","error":"quota service unavailable"}"#), UsageAnswer::Failed("quota service unavailable".into()));
        assert_eq!(usage_answer("Authentication required. Please visit the URL to log in:"), UsageAnswer::Failed("unexpected response".into()));
        assert_eq!(group_label("Gemini Models"), "Gemini");
        assert_eq!(group_label("Pro MODELS "), "Pro");
        assert_eq!(group_label("Models"), "Models");
    }

    #[test]
    fn an_account_with_a_command_of_its_own_has_its_quotas_read() {
        use crate::harness::Adapter as _;
        let account = AccountCfg { name: "agy".into(), harness: "antigravity".into(), home: "~/.gemini/antigravity".into(), command: Some("/opt/agy/bin/agy".into()), start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false };
        assert!(Antigravity.polls_usage(&account) && Antigravity.no_limits(&account).is_none());
        // And it signs in with that command, whatever's installed.
        assert_eq!(Antigravity.sign_in_command(&account), "/opt/agy/bin/agy");
    }

    #[test]
    fn a_prompt_reaches_a_conversation_where_it_runs() {
        use crate::harness::Adapter as _;
        use crate::send::Route;
        let a = AccountCfg { name: "antigravity".into(), harness: "antigravity".into(), home: "~/.gemini/antigravity".into(), command: None, start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false };
        let c = Conversation { id: "c1".into(), cli: true, ..Default::default() };
        let ui = Process { pid: 41, conversation: Some("c1".into()), terminal: true, logs: Vec::new() };
        let session = |c: &Conversation, p: Option<&Process>| crate::board::antigravity_session(&a, c, p, true, 0, 0);
        // Typed into its terminal; a new terminal resumes one that isn't running; a print run
        // has it until it ends.
        assert_eq!(Antigravity.routes(&a, &session(&c, Some(&ui))), [Route::Type { pid: 41 }]);
        assert_eq!(Antigravity.routes(&a, &session(&c, None)), [Route::Resume]);
        assert!(Antigravity.routes(&a, &session(&c, Some(&Process { terminal: false, ..ui.clone() }))).is_empty());
        // Nothing reaches the app's from outside.
        assert_eq!(Antigravity.routes(&a, &session(&Conversation { cli: false, ..c.clone() }, None)), [OnClipboard.into()]);
    }

    /// Runs a real print run of `agy` in a scratch folder and reads it as the engine does while
    /// it works and once it's done. It needs `agy` installed and signed in, and spends a little
    /// of its quota. Opt in: `cargo test reads_a_live_run -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn reads_a_live_run() {
        use crate::harness::Reader as _;
        use crate::model::Status;
        let name = format!("codealong-agy-live-{}", std::process::id());
        let folder = std::env::temp_dir().join(&name);
        std::fs::create_dir_all(&folder).unwrap();
        let account = AccountCfg { name: "antigravity".into(), harness: "antigravity".into(), home: "~/.gemini/antigravity".into(), command: None, start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false };
        let accounts = std::slice::from_ref(&account);
        let seen = crate::state::SeenState::default();
        let look = || {
            let now = crate::util::now_ms();
            Look { seen: &seen, since: now - crate::util::DAY_MS, now, ps: Default::default() }
        };
        let mut reader = Reader::default();
        reader.scan(accounts, &look());
        let mut child = std::process::Command::new(crate::util::home().join(".local/bin").join(CLI))
            .args(["-p", "Run the shell command 'sleep 6' with your command tool, then reply with the single word DONE.", "--model", "gemini-3.8-flash-low", "--output-format", "json", "--dangerously-skip-permissions"])
            .current_dir(&folder)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("agy starts");
        let mine = |r: &Reader, look: &Look| r.sessions(&account, look).into_iter().find(|s| s.cwd.contains(&name));
        let (mut worked, mut ticks, started) = (None, 0, Instant::now());
        let done = loop {
            assert!(started.elapsed() < Duration::from_secs(120), "the run never showed as done");
            std::thread::sleep(Duration::from_millis(400));
            let l = look();
            // The engine glances every second or two, and ticks every ten: here, every two.
            reader.glance(accounts, &l);
            ticks += 1;
            if ticks % 5 == 0 {
                reader.tick(accounts, &l);
            }
            let exited = child.try_wait().unwrap().is_some();
            match mine(&reader, &l) {
                Some(s) if s.status == Status::Working => {
                    println!("working: live {} pid {:?} · {:?}", s.live, s.pid, s.excerpt);
                    worked = Some(s);
                }
                Some(s) if exited && worked.is_some() => {
                    // Its process is gone: the next look at processes says so.
                    reader.tick(accounts, &look());
                    break mine(&reader, &look()).unwrap_or(s);
                }
                _ => {}
            }
        };
        let worked = worked.expect("it showed as working while it ran");
        assert!(worked.live && worked.pid.is_none() && worked.surface == Surface::Cli, "a print run is live with no terminal to show");
        assert!(worked.excerpt.as_deref().is_some_and(|e| e.starts_with("Run command")), "{:?}", worked.excerpt);
        println!("done: {:?} live {} · {:?} · resume with {:?}", done.status, done.live, done.excerpt, done.resume_command);
        assert_eq!((done.status, done.live), (Status::Unread, false));
        assert!(done.excerpt.as_deref().is_some_and(|e| e.contains("DONE")), "{:?}", done.excerpt);
        // Its panel reads the whole of it.
        use crate::harness::Adapter as _;
        let page = Antigravity.conversation(&Ctx { account: &account }, &done, None, 50).unwrap();
        let kinds: Vec<&str> = page.items.iter().map(|i| i.kind).collect();
        println!("{kinds:?}");
        assert!(kinds.first() == Some(&"you") && kinds.contains(&"tool") && kinds.contains(&"result") && kinds.last() == Some(&"agent"));
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn search_reads_what_was_asked_and_replied() {
        use crate::harness::Adapter as _;
        let said: Vec<(bool, String)> = LINES.iter().filter_map(|l| Antigravity.prose(l.as_bytes())).map(|(_, you, text)| (you, text)).collect();
        // Your request without what Antigravity wraps it in, and the replies; no calls, results or notes.
        assert_eq!(said, [(true, "Count the lines, then fix notes.txt".to_string()), (false, "Counting first.".to_string()), (false, "Done: one line, and **notes.txt** is fixed.".to_string())]);
        assert_eq!(Antigravity.prose(LINES[0].as_bytes()).unwrap().0, parse_rfc3339_ms("2026-10-01T16:55:10Z").unwrap());
    }

    /// A record longer than what's read of it, where each cut (the start's and the end's) falls
    /// inside a two-byte character in one of the two files: only the line it fell in is lost.
    #[test]
    fn a_cut_inside_a_character_costs_only_its_line() {
        let dir = crate::fixtures::scratch("agy-cut");
        for lead in ["", "x"] {
            let path = dir.join(format!("transcript{}.jsonl", lead.len()));
            let note = format!(r#"{{"step_index":1,"source":"SYSTEM","type":"SYSTEM_MESSAGE","status":"DONE","created_at":"2026-10-01T16:55:15Z","content":"{lead}{}"}}"#, "é".repeat(200_000));
            std::fs::write(&path, format!("{}\n{note}\n{}\n", LINES[0], LINES[7])).unwrap();
            assert!(std::fs::metadata(&path).unwrap().len() > TAIL + 100_000);
            assert_eq!(first_request(&path).map(|(asked, _)| asked).as_deref(), Some("Count the lines, then fix notes.txt"), "lead {lead:?}");
            assert_eq!(tail(&path, None).last_text.as_deref(), Some("Done: one line, and notes.txt is fixed."), "lead {lead:?}");
        }
    }

    /// As `fetchUserInfo` answered on 2026-10-13, and with the switches Antigravity 2.23.0 knows.
    #[test]
    fn reads_enable_telemetry_as_whether_google_trains_on_the_account() {
        use crate::training::State;
        let account = AccountCfg { name: "antigravity".into(), harness: "antigravity".into(), home: "~/.gemini/antigravity".into(), command: None, start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false };
        let read = |v: Value| telemetry_training(&account, &v);
        let off = read(serde_json::json!({"userSettings": {}, "regionCode": "US"}));
        assert_eq!((off.state, off.can_stop), (State::Off, false));
        let on = read(serde_json::json!({"userSettings": {"telemetryEnabled": true}}));
        assert_eq!((on.state, on.can_stop), (State::On, true));
        assert_eq!(read(serde_json::json!({"userSettings": {"telemetryEnabled": true, "userDataCollectionForceDisabled": true}})).state, State::Off);
    }

    /// An `ask_question` call and its result as `agy` 1.3.5 wrote them (in tmux, 2026-10-14):
    /// the first question skipped, the second's choice ticked with words of your own on two
    /// lines. The transcript without `_full` writes the list as a string of JSON.
    const ASKED: [&str; 3] = [
        r#"{"step_index":5,"source":"MODEL","type":"PLANNER_RESPONSE","status":"DONE","created_at":"2026-10-15T02:12:32Z","tool_calls":[{"name":"ask_question","args":{"questions":[{"is_multi_select":false,"options":["Red","Green","Blue"],"question":"Which color?"},{"is_multi_select":true,"options":["Mon","Tue","Wed"],"question":"Which days?"},{"is_multi_select":false,"options":["Small","Large"],"question":"Which size?"}],"toolAction":"Asking preference questions","toolSummary":"Color, days, and size questions"}}]}"#,
        r#"{"step_index":5,"source":"MODEL","type":"PLANNER_RESPONSE","status":"DONE","created_at":"2026-10-15T02:12:32Z","tool_calls":[{"name":"ask_question","args":{"questions":"[{\"is_multi_select\":false,\"options\":[\"Red\",\"Green\",\"Blue\"],\"question\":\"Which color?\"},{\"is_multi_select\":true,\"options\":[\"Mon\",\"Tue\",\"Wed\"],\"question\":\"Which days?\"},{\"is_multi_select\":false,\"options\":[\"Small\",\"Large\"],\"question\":\"Which size?\"}]","toolAction":"\"Asking preference questions\"","toolSummary":"\"Color, days, and size questions\""}}]}"#,
        r#"{"step_index":6,"source":"MODEL","type":"GENERIC","status":"DONE","created_at":"2026-10-15T02:12:36Z","content":"Created At: 2026-10-14T19:12:36-07:00\nCompleted At: 2026-10-14T19:13:05-07:00\nA1: User Skipped\nA2: Tue, Thu too\nand Fri\nA3: Large"}"#,
    ];

    fn asked() -> Vec<Question> {
        call(&serde_json::from_str::<Value>(ASKED[0]).unwrap()["tool_calls"][0], 6).questions
    }

    #[test]
    fn reads_the_questions_it_asks_and_your_answers() {
        let full = asked();
        assert_eq!(full.iter().map(|q| (q.question.as_str(), q.options.len(), q.multi_select)).collect::<Vec<_>>(), [("Which color?", 3, false), ("Which days?", 3, true), ("Which size?", 2, false)]);
        assert_eq!(call(&serde_json::from_str::<Value>(ASKED[1]).unwrap()["tool_calls"][0], 6).questions, full);
        // On the board it asks you, and in the conversation it's the questions with your answers.
        assert_eq!(call(&serde_json::from_str::<Value>(ASKED[0]).unwrap()["tool_calls"][0], 6).asked(), "Asks you · Which color?");
        let items: Vec<Item> = ASKED[1..].iter().flat_map(|l| conversation_items(&serde_json::from_str(l).unwrap(), 0)).collect();
        assert_eq!((items[0].kind, items[0].id.as_deref(), items[0].questions.len()), ("tool", Some("6"), 3));
        let page = with_answers(Path::new("/nowhere"), Page { items, before: None, after: 0 });
        let answers = &page.items[1].answers;
        assert_eq!(answers.len(), 2, "a question you skipped has no answer");
        assert_eq!(answers["Which days?"], ["Tue", "Thu too\nand Fri"]);
        assert_eq!(answers["Which size?"], ["Large"]);
        // Words of your own longer than a result shows still leave every answer whole.
        let long = "word ".repeat(2_000);
        let result = serde_json::json!({"step_index": 6, "source": "MODEL", "type": "GENERIC", "status": "DONE", "created_at": "2026-10-15T02:12:36Z", "content": format!("Created At: 2026-10-14T19:12:36-07:00\nA1: {long}\nA2: Mon\nA3: Small")});
        let items = [conversation_items(&serde_json::from_str(ASKED[0]).unwrap(), 0), conversation_items(&result, 0)].concat();
        assert!(items[1].output.as_deref().unwrap().starts_with('…'));
        let answers = &with_answers(Path::new("/nowhere"), Page { items, before: None, after: 0 }).items[1].answers;
        assert_eq!((answers["Which color?"][0].len(), &answers["Which days?"], &answers["Which size?"]), (long.trim_end().len(), &vec!["Mon".to_string()], &vec!["Small".to_string()]));
        // A label with a comma of its own is found whole, among others or alone.
        let q = Question { question: "Which?".into(), header: None, options: ["Mon, Tue", "Wed"].map(|l| codealong_peer::api::Choice { label: l.into(), description: None }).to_vec(), multi_select: true };
        assert_eq!(picks_of("Mon, Tue, Wed", &q), ["Mon, Tue", "Wed"]);
        assert_eq!(picks_of("Mon, Tue", &q), ["Mon, Tue"]);
        assert_eq!(picks_of("Fri", &q), ["Fri"]);
    }

    /// A result read without its call, as one that came in after the call was read is: its
    /// questions are found in the record.
    #[test]
    fn answers_read_after_their_questions_are_keyed_by_them() {
        let dir = crate::fixtures::scratch("agy-answers");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("transcript_full.jsonl");
        std::fs::write(&path, format!("{}\n{}\n", ASKED[0], ASKED[2])).unwrap();
        let after = (ASKED[0].len() + 1) as u64;
        let page = with_answers(&path, crate::transcript::file_newer(&path, conversation_items, after).unwrap());
        assert_eq!((page.items.len(), page.items[0].answers["Which size?"].clone()), (1, vec!["Large".to_string()]));
        // A command's result is left as it is, whatever it printed.
        let page = with_answers(&path, Page { items: vec![Item { id: Some("9".into()), output: Some("A1: Green".into()), ..item(0, "result", "") }], before: None, after: 0 });
        assert!(page.items[0].answers.is_empty());
    }

    #[test]
    fn questions_are_answered_with_the_keys_its_terminal_takes() {
        use Press::{Key as K, Words};
        let questions = asked();
        let keys = |answers: Value| question_keys(&questions, Some(&answers));
        // Back to the first question; each but the last cleared of what its terminal had (skipped
        // and gone back to); a choice there goes on; ticks, then Return; a choice on the last
        // submits them all.
        let cleared = [K(Key::Escape), K(Key::Left)];
        assert_eq!(
            keys(serde_json::json!({"Which color?": "Green", "Which days?": ["Wed", "Mon", "Wed"], "Which size?": "Large"})).unwrap(),
            [&[K(Key::Left), K(Key::Left)][..], &cleared, &[K(Key::Digit(2))], &cleared, &[K(Key::Digit(1)), K(Key::Digit(3)), K(Key::Return), K(Key::Digit(2))]].concat()
        );
        // Your own words go in Write-in's box, with what's ticked; a question with no answer is
        // skipped.
        assert_eq!(
            keys(serde_json::json!({"Which days?": ["Tue", "Thu too"], "Which size?": "  medium  "})).unwrap(),
            [&[K(Key::Left), K(Key::Left), K(Key::Escape)][..], &cleared, &[K(Key::Digit(2)), K(Key::Digit(4)), Words("Thu too".into()), K(Key::Digit(3)), Words("medium".into())]].concat()
        );
        assert_eq!(keys(serde_json::json!({"Which color?": " "})).unwrap_err().to_string(), "Pick an answer first.");
        // Declined, every question is skipped and the turn they go on in stopped.
        assert_eq!(question_keys(&questions, None).unwrap(), [K(Key::Left), K(Key::Left), K(Key::Escape), K(Key::Escape), K(Key::Escape), K(Key::Escape)]);
        // Only nine choices have a key, Write-in among them.
        let mut long = questions[..1].to_vec();
        long[0].options = (1..=9).map(|n| codealong_peer::api::Choice { label: format!("Choice {n}"), description: None }).collect();
        assert_eq!(question_keys(&long, Some(&serde_json::json!({"Which color?": "Choice 9"}))).unwrap(), [K(Key::Digit(9))]);
        assert!(question_keys(&long, Some(&serde_json::json!({"Which color?": "Purple"}))).unwrap_err().to_string().contains("more choices than Codealong picks among"));
    }

    /// A conversation in its terminal UI waiting on its questions, end to end: they show as
    /// questions, are answered with picks only, and a key is pressed only in a terminal.
    #[test]
    fn a_question_waits_for_your_picks() {
        let root = crate::fixtures::scratch("antigravity-asks");
        let dir = root.join("antigravity-cli");
        let logs = dir.join("brain/c1/.system_generated/logs");
        std::fs::create_dir_all(&logs).unwrap();
        std::fs::create_dir_all(dir.join("conversations")).unwrap();
        std::fs::write(logs.join("transcript_full.jsonl"), format!("{}\n{}\n", LINES[0], ASKED[0])).unwrap();
        let steps = Connection::open(dir.join("conversations/c1.db")).unwrap();
        steps.execute_batch("CREATE TABLE steps (idx INTEGER PRIMARY KEY, step_type INTEGER NOT NULL DEFAULT 0, status INTEGER NOT NULL DEFAULT 0); INSERT INTO steps VALUES (0, 14, 3), (5, 15, 3), (6, 132, 9);").unwrap();
        let account = AccountCfg { name: "antigravity".into(), harness: "antigravity".into(), home: root.join("antigravity").to_string_lossy().into_owned(), command: None, start: None, usage_cap: None, acp: None, monthly: None, per_token: false, disabled: false };
        let ctx = Ctx { account: &account };
        let session = Session { surface: Surface::Cli, pid: Some(999_999), cwd: "/r/app".into(), ..Session::new(Harness::Antigravity, "antigravity", "c1") };
        use crate::harness::{Adapter as _, Answer};
        let waits = Antigravity.pending(&ctx, &session).unwrap();
        assert_eq!((waits.id.as_str(), waits.label.as_str(), waits.input.is_none(), waits.cannot.is_none()), ("6", "Asks you · Which color?", true, true));
        assert_eq!(waits.questions, asked());
        assert_eq!(Antigravity.answer(&ctx, &session, "6", Answer::Allow).unwrap_err().to_string(), "It asks you questions: pick your answers.");
        let picks = serde_json::json!({"Which color?": "Green"});
        assert!(Antigravity.answer_questions(&ctx, &session, "6", &picks).unwrap_err().to_string().starts_with("It isn't in a terminal Codealong can press keys in"));
        assert!(Antigravity.answer_questions(&ctx, &session, "5", &picks).unwrap_err().to_string().starts_with("It's waiting on a different call now"));
        let in_app = Session { pid: None, ..session.clone() };
        assert!(Antigravity.pending(&ctx, &in_app).unwrap().cannot.is_some());
        assert!(Antigravity.answer_questions(&ctx, &in_app, "6", &picks).unwrap_err().to_string().contains("answer it there"));
        steps.execute("UPDATE steps SET status = 3 WHERE idx = 6", []).unwrap();
        assert_eq!(Antigravity.answer_questions(&ctx, &session, "6", &picks).unwrap_err().to_string(), "It isn't waiting on you any more.");
    }
}
