//! The Claude app's records of its Code sessions: one `local_<id>.json` per session under
//! `claude-code-sessions/<account>/<org>/`, naming the CLI session it runs (`cliSessionId`),
//! whether you archived it there (`isArchived`), the permission mode you run it in
//! (`permissionMode`) and the session it came from (`came_from`). The app has no way for another
//! program to archive a session, so Codealong reads these to hide what you archived in the app,
//! to open a session there by the app's own id, and to link a fork to the session it copied.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde_json::Value;

use crate::attachments::{self, Style};
use crate::model::OriginHow;
use crate::send::{copy, opened, Delivery, Own, Sent, Way};
use crate::util::{file_name, home, mtime_ms};

/// The Claude app's bundle ID.
pub const BUNDLE_ID: &str = "com.anthropic.claudefordesktop";

pub fn records_dir() -> PathBuf {
    home().join("Library/Application Support/Claude/claude-code-sessions")
}

/// Where the app makes a folder for each Code session started without one
/// (`<account>/<org>/scratch-<date>-<hex>`), removed once the session is.
pub fn scratch_dir() -> PathBuf {
    home().join("Library/Application Support/Claude/scratch-workspaces")
}

#[derive(Debug, Clone, PartialEq)]
pub struct Record {
    /// The app's own ID, `local_…`.
    pub local_id: String,
    pub cli_session_id: String,
    pub archived: bool,
    /// The session this one came from, by the app's ID, and how.
    pub came_from: Option<(String, OriginHow)>,
}

fn subdirs(p: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(p).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect()
}

/// Every record file with its modification time, newest first.
pub fn record_files(root: &Path) -> Vec<(PathBuf, i64)> {
    let mut files: Vec<(PathBuf, i64)> = subdirs(root)
        .iter()
        .flat_map(|account| subdirs(account))
        .flat_map(|org| std::fs::read_dir(org).into_iter().flatten().flatten().map(|e| e.path()))
        .filter(|p| file_name(p).starts_with("local_") && p.extension().and_then(|x| x.to_str()) == Some("json"))
        .map(|p| {
            let m = mtime_ms(&p).unwrap_or(0);
            (p, m)
        })
        .collect();
    files.sort_by(|a, b| b.1.cmp(&a.1));
    files
}

/// The permission mode the app runs one of its sessions in, from its record, where the app keeps
/// the mode you pick. `local_id` is the app's own ID, which a live session's registry entry
/// names (`hostSessionId`).
pub fn permission_mode(root: &Path, local_id: &str) -> Option<String> {
    let id = local_id.strip_prefix("local_")?;
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return None;
    }
    let name = format!("{local_id}.json");
    let path = subdirs(root).iter().flat_map(|account| subdirs(account)).map(|org| org.join(&name)).find(|p| p.is_file())?;
    let v: Value = serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()?;
    v.get("permissionMode").and_then(Value::as_str).map(str::to_string)
}

pub fn read_record(path: &Path) -> Option<Record> {
    parse_record(&std::fs::read_to_string(path).ok()?, path)
}

pub fn parse_record(text: &str, path: &Path) -> Option<Record> {
    let v: Value = serde_json::from_str(text).ok()?;
    let local_id = v.get("sessionId").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| file_name(path).trim_end_matches(".json").to_string());
    Some(Record {
        local_id,
        cli_session_id: v.get("cliSessionId").and_then(Value::as_str)?.to_string(),
        archived: v.get("isArchived").and_then(Value::as_bool).unwrap_or(false),
        came_from: came_from(&v),
    })
}

/// Where a session of the app's came from, as its record says. `forkedFromSessionId` names
/// the session it forked: the app's Fork, and its `btw`, a side question asked in a fork of the
/// conversation (`spawnedFrom.btw`, with `sideSession` and `userFork`, and a "btw: " title).
/// `spawnedFrom.sessionId` alone names the session whose agent offered it as a task to start
/// (the `spawn_task` tool's chip), and `spawnedFrom.taskId` that task (2.31226, 2026-10-09).
fn came_from(v: &Value) -> Option<(String, OriginHow)> {
    let id = |p: &str| v.pointer(p).and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_string);
    id("/forkedFromSessionId").map(|f| (f, OriginHow::Fork)).or_else(|| id("/spawnedFrom/sessionId").map(|s| (s, OriginHow::Started)))
}

/// The records, re-read only when a file changes.
#[derive(Default)]
pub struct Records {
    cache: HashMap<PathBuf, (i64, Option<Record>)>,
}

impl Records {
    /// Picks up new, changed and removed records. True when anything changed.
    pub fn refresh(&mut self, root: &Path) -> bool {
        let files = record_files(root);
        let mut changed = files.len() != self.cache.len();
        let mut next = HashMap::with_capacity(files.len());
        for (path, mtime) in files {
            let entry = match self.cache.remove(&path) {
                Some((m, rec)) if m == mtime => (m, rec),
                old => {
                    let rec = read_record(&path);
                    changed |= old.map(|(_, r)| r != rec).unwrap_or(true);
                    (mtime, rec)
                }
            };
            next.insert(path, entry);
        }
        self.cache = next;
        changed
    }

    /// CLI session IDs you archived in the Claude app.
    pub fn archived(&self) -> HashSet<String> {
        self.cache.values().filter_map(|(_, r)| r.as_ref()).filter(|r| r.archived).map(|r| r.cli_session_id.clone()).collect()
    }

    /// By CLI session ID, the CLI session each of the app's sessions came from and how, for the
    /// ones whose record says and whose source the app still has a record of.
    pub fn origins(&self) -> HashMap<String, (String, OriginHow)> {
        let records = || self.cache.values().filter_map(|(_, r)| r.as_ref());
        let cli_of: HashMap<&str, &str> = records().map(|r| (r.local_id.as_str(), r.cli_session_id.as_str())).collect();
        records()
            .filter_map(|r| {
                let (from, how) = r.came_from.as_ref()?;
                let from = cli_of.get(from.as_str()).filter(|f| **f != r.cli_session_id)?;
                Some((r.cli_session_id.clone(), (from.to_string(), *how)))
            })
            .collect()
    }
}

/// Opens a session in the Claude app. A session the app runs is selected by the app's own ID
/// (`code/continue` takes only `local_…` IDs); any other is imported first (`resume`).
pub fn claude_app_url(cli_session_id: &str) -> String {
    claude_app_url_in(&records_dir(), cli_session_id)
}

fn claude_app_url_in(records: &Path, cli_session_id: &str) -> String {
    match claude_app_session(records, cli_session_id) {
        Some(local) => format!("claude://code/continue?session={}", urlencoding::encode(&local)),
        None => format!("claude://resume?session={}", urlencoding::encode(cli_session_id)),
    }
}

/// The Claude app's ID for the session running `cli_session_id`, from its records (newest first,
/// since that's usually the one; only files that mention the session are parsed).
fn claude_app_session(records: &Path, cli_session_id: &str) -> Option<String> {
    record_files(records).into_iter().find_map(|(path, _)| {
        let text = std::fs::read_to_string(&path).ok()?;
        if !text.contains(cli_session_id) {
            return None;
        }
        parse_record(&text, &path).filter(|r| r.cli_session_id == cli_session_id).map(|r| r.local_id)
    })
}

/// The way in for one of the app's sessions that isn't running, or one the command line started
/// that goes on there (`Adapter::app_way`): the app opens it, with the prompt on the clipboard
/// for you to paste, when there's one.
#[derive(Debug, Clone, PartialEq)]
pub struct InApp;

impl Own for InApp {
    fn way(&self) -> Way {
        Way::ClaudeApp
    }

    fn takes_agents(&self) -> bool {
        false
    }

    fn deliver(&self, d: &Delivery) -> Result<Sent> {
        let prompt = attachments::expand(d.text, Style::Path);
        if prompt.trim().is_empty() {
            crate::launch::open_url(&claude_app_url(&d.session.id))?;
            return Ok(opened(format!("{} is open in the Claude app.", d.title())));
        }
        copy(&prompt)?;
        crate::launch::open_url(&claude_app_url(&d.session.id))?;
        Ok(opened(format!("{} is open in the Claude app and the prompt is on the clipboard. Paste it there to send it.", d.title())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_archived_sessions_and_notices_changes() {
        let root = crate::fixtures::scratch("app-records");
        let org = root.join("acct").join("org");
        std::fs::create_dir_all(&org).unwrap();
        std::fs::write(org.join("local_a.json"), r#"{"sessionId":"local_a","cliSessionId":"cli-a","isArchived":true}"#).unwrap();
        std::fs::write(org.join("local_b.json"), r#"{"sessionId":"local_b","cliSessionId":"cli-b","isArchived":false}"#).unwrap();
        let mut records = Records::default();
        assert!(records.refresh(&root));
        assert_eq!(records.archived(), HashSet::from(["cli-a".to_string()]));
        assert!(!records.refresh(&root));
        std::fs::remove_file(org.join("local_a.json")).unwrap();
        assert!(records.refresh(&root));
        assert!(records.archived().is_empty());
    }

    #[test]
    fn reads_the_permission_mode_the_app_runs_a_session_in() {
        let root = crate::fixtures::scratch("app-modes");
        let org = root.join("acct").join("org");
        std::fs::create_dir_all(&org).unwrap();
        std::fs::write(org.join("local_7a-b2.json"), r#"{"sessionId":"local_7a-b2","cliSessionId":"cli","permissionMode":"bypassPermissions"}"#).unwrap();
        std::fs::write(org.join("local_c3.json"), r#"{"sessionId":"local_c3","cliSessionId":"cli2"}"#).unwrap();
        assert_eq!(permission_mode(&root, "local_7a-b2").as_deref(), Some("bypassPermissions"));
        assert_eq!(permission_mode(&root, "local_c3"), None);
        assert_eq!(permission_mode(&root, "local_gone"), None);
        // Only the app's own IDs name a file: nothing reaches outside the records.
        assert_eq!(permission_mode(&root, "local_../../x"), None);
        assert_eq!(permission_mode(&root, "7a-b2"), None);
    }

    #[test]
    fn reads_what_a_session_was_forked_from_or_started_by() {
        let root = crate::fixtures::scratch("app-origins");
        let org = root.join("acct").join("org");
        std::fs::create_dir_all(&org).unwrap();
        let write = |id: &str, rest: &str| std::fs::write(org.join(format!("local_{id}.json")), format!(r#"{{"sessionId":"local_{id}","cliSessionId":"cli-{id}"{rest}}}"#)).unwrap();
        write("p", "");
        // As the app (2.31226) records a btw: a fork that's also a side session.
        write("btw", r#","forkedFromSessionId":"local_p","spawnedFrom":{"sessionId":"local_p","taskId":"task_1","sideSession":true,"userFork":true,"btw":true}"#);
        write("task", r#","forkedFromSessionId":null,"spawnedFrom":{"sessionId":"local_p","taskId":"task_2","title":"Write the spec"}"#);
        write("gone", r#","forkedFromSessionId":"local_removed""#);
        let mut records = Records::default();
        records.refresh(&root);
        let origins = records.origins();
        assert_eq!(origins.get("cli-btw"), Some(&("cli-p".to_string(), OriginHow::Fork)));
        assert_eq!(origins.get("cli-task"), Some(&("cli-p".to_string(), OriginHow::Started)));
        assert_eq!(origins.len(), 2, "one the app has no record of left out: {origins:?}");
    }

    #[test]
    fn finds_the_claude_app_session_for_a_cli_session() {
        let root = std::env::temp_dir().join(format!("codealong-records-{}", std::process::id()));
        let org = root.join("acct").join("org");
        std::fs::create_dir_all(&org).unwrap();
        std::fs::write(org.join("local_aaa.json"), r#"{"sessionId":"local_aaa","cliSessionId":"11111111-0000-0000-0000-000000000000"}"#).unwrap();
        std::fs::write(org.join("local_bbb.json"), r#"{"sessionId":"local_bbb","cliSessionId":"87412567-2cd2-4c10-8635-d144e1c53c78"}"#).unwrap();
        assert_eq!(claude_app_url_in(&root, "87412567-2cd2-4c10-8635-d144e1c53c78"), "claude://code/continue?session=local_bbb");
        assert_eq!(claude_app_url_in(&root, "22222222-0000-0000-0000-000000000000"), "claude://resume?session=22222222-0000-0000-0000-000000000000");
        let _ = std::fs::remove_dir_all(&root);
        // With no records at all, the app imports the session.
        let empty = std::env::temp_dir().join("codealong-no-records");
        assert_eq!(claude_app_url_in(&empty, "87412567-2cd2"), "claude://resume?session=87412567-2cd2");
    }
}
