//! The Claude app's records of its Code sessions: one `local_<id>.json` per session under
//! `claude-code-sessions/<account>/<org>/`, naming the CLI session it runs (`cliSessionId`),
//! whether you archived it there (`isArchived`) and the permission mode you run it in
//! (`permissionMode`). The app has no way for another program to archive a session, so
//! Codealong reads these to hide what you archived in the app.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::util::{file_name, home, mtime_ms};

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
    })
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_archived_sessions_and_notices_changes() {
        let root = std::env::temp_dir().join(format!("codealong-app-records-{}", std::process::id()));
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
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn reads_the_permission_mode_the_app_runs_a_session_in() {
        let root = std::env::temp_dir().join(format!("codealong-app-modes-{}", std::process::id()));
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
        let _ = std::fs::remove_dir_all(&root);
    }
}
