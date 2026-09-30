//! Antigravity (2.x): the conversation summaries its language server keeps in
//! `~/.gemini/antigravity/conversation_summaries.db`. The conversations themselves are
//! protobuf files and the app's own state, so the summary table is what Codealong reads: a
//! title, the workspace folders, when it last changed, when you last wrote, and whether it's
//! still working. See *Harness adapters*.

use std::collections::HashSet;
use std::path::Path;
use std::time::Duration;

use rusqlite::types::Value as Sql;
use rusqlite::{Connection, OpenFlags};

use crate::util::parse_rfc3339_ms;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Conversation {
    pub id: String,
    pub title: Option<String>,
    /// The summary's preview text: the conversation's opening, as the app lists it.
    pub preview: Option<String>,
    pub step_count: i64,
    /// The first workspace folder, as a path.
    pub cwd: Option<String>,
    pub workspaces: Vec<String>,
    /// The summary's status word, as the app records it, lowercased.
    pub status: String,
    pub source: String,
    /// The agent is still doing something.
    pub not_fully_idle: bool,
    pub killed: bool,
    pub last_modified: i64,
    /// When you last wrote to it.
    pub last_input: Option<i64>,
}

/// Conversations changed since `since_ms`, plus any in `keep` (pinned ones). Nested ones
/// (subagents, side questions) are left out.
pub fn read_conversations(home: &Path, since_ms: i64, keep: &HashSet<String>) -> Vec<Conversation> {
    let path = home.join("conversation_summaries.db");
    if !path.is_file() {
        return Vec::new();
    }
    let Ok(db) = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX) else { return Vec::new() };
    let _ = db.busy_timeout(Duration::from_millis(500));
    read_from(&db, since_ms, keep).unwrap_or_default()
}

fn read_from(db: &Connection, since_ms: i64, keep: &HashSet<String>) -> rusqlite::Result<Vec<Conversation>> {
    let mut stmt = db.prepare(
        "SELECT conversation_id, title, preview, step_count, last_modified_time, workspace_uris, status, source, \
         parent_conversation_id, nesting_depth, not_fully_idle, killed, last_user_input_time FROM conversation_summaries",
    )?;
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
        ))
    })?;
    let mut out = Vec::new();
    for (id, title, preview, step_count, modified, uris, status, source, parent, depth, busy, killed, input) in rows.flatten() {
        if depth > 0 || parent.map(|p| !p.is_empty()).unwrap_or(false) {
            continue;
        }
        let last_modified = timestamp(&modified).unwrap_or(0);
        if last_modified < since_ms && !keep.contains(&id) {
            continue;
        }
        let workspaces: Vec<String> = uris.as_deref().and_then(|u| serde_json::from_str::<Vec<String>>(u).ok()).unwrap_or_default().iter().filter_map(|u| uri_path(u)).collect();
        out.push(Conversation {
            cwd: workspaces.first().cloned(),
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
        });
    }
    out.sort_by(|a, b| b.last_modified.cmp(&a.last_modified));
    Ok(out)
}

/// A gorm `datetime` column: text like `2026-09-24 14:16:07.434615-07:00`, or seconds or
/// milliseconds since the epoch.
fn timestamp(v: &Sql) -> Option<i64> {
    match v {
        Sql::Text(t) => parse_rfc3339_ms(t.trim()),
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

/// Antigravity (`harness::Adapter`): its conversations are protobufs Codealong doesn't read yet,
/// so its panel shows the summary line and opens the app.
pub struct Antigravity;

impl crate::harness::Adapter for Antigravity {
    fn harness(&self) -> crate::model::Harness {
        crate::model::Harness::Antigravity
    }

    /// The sign-in is a token file beside the app data folder, under ~/.gemini; only whether
    /// it's there is read.
    fn signed_in(&self, home: &Path) -> (bool, Option<String>) {
        (home.parent().is_some_and(|p| p.join("jetski-standalone-oauth-token").is_file()), None)
    }

    /// It signs in from its own window.
    fn sign_in_command(&self, _account: &crate::config::AccountCfg) -> String {
        "open -a Antigravity".to_string()
    }

    fn installed(&self, _home: &Path, _on_path: &dyn Fn(&str) -> bool) -> bool {
        crate::detect::app("Antigravity.app").is_some()
    }

    fn folder_use(&self, dir: &Path) -> crate::harness::FolderUse {
        crate::harness::FolderUse {
            has_data: dir.join("conversation_summaries.db").is_file(),
            last_used: [dir.join("conversation_summaries.db"), dir.join("conversation_summaries.db-wal")].iter().filter_map(|p| crate::util::mtime_ms(p)).max(),
            signed_in: false,
        }
    }

    fn skills_dir(&self, _account: &crate::config::AccountCfg) -> Option<std::path::PathBuf> {
        Some(crate::util::home().join(".gemini/config/skills"))
    }

    fn places(&self, account: &crate::config::AccountCfg, _home: &Path) -> std::result::Result<Vec<crate::start::Place>, String> {
        Err(format!("Antigravity starts conversations in its own window, so {} can't start one from here.", account.name))
    }

    fn session_cwds(&self, home: &Path, since: i64) -> Vec<String> {
        read_conversations(home, since, &HashSet::new()).into_iter().filter_map(|c| c.cwd).collect()
    }

    fn no_limits(&self) -> Option<&'static str> {
        Some("Antigravity keeps its quotas in the app.")
    }

    fn records(&self, account: &crate::config::AccountCfg, _s: &crate::model::Session) -> Vec<std::path::PathBuf> {
        vec![account.home_path().join("conversation_summaries.db")]
    }

    /// Nothing takes a prompt from outside: the app comes forward with it on the clipboard.
    fn routes(&self, _account: &crate::config::AccountCfg, _s: &crate::model::Session) -> Vec<crate::send::Route> {
        vec![crate::send::Route::Antigravity]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_summary_table() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE conversation_summaries (conversation_id TEXT PRIMARY KEY, title TEXT NOT NULL DEFAULT '', preview TEXT NOT NULL DEFAULT '', \
             step_count INTEGER NOT NULL DEFAULT 0, last_modified_time DATETIME NOT NULL, workspace_uris TEXT NOT NULL, status TEXT NOT NULL DEFAULT '', \
             source TEXT NOT NULL DEFAULT '', project_id TEXT NOT NULL DEFAULT '', agent_name TEXT NOT NULL DEFAULT '', parent_conversation_id TEXT NOT NULL DEFAULT '', \
             nesting_depth INTEGER NOT NULL DEFAULT 0, not_fully_idle NUMERIC NOT NULL DEFAULT false, killed NUMERIC NOT NULL DEFAULT false, \
             last_user_input_time DATETIME NOT NULL, last_user_input_step_index INTEGER NOT NULL DEFAULT -1, app_data_dir TEXT NOT NULL DEFAULT '', raw_summary BLOB, group_id TEXT NOT NULL DEFAULT '');\
             INSERT INTO conversation_summaries (conversation_id, title, preview, step_count, last_modified_time, workspace_uris, status, not_fully_idle, last_user_input_time) VALUES \
             ('c1', 'Fix the build', 'Fix the build so tests pass', 12, '2026-09-24 14:20:07.434615-07:00', '[\"file:///Users/me/My%20Repo/\"]', 'RUNNING', 1, '2026-09-24 14:16:07.000000-07:00'), \
             ('c2', 'Old one', '', 3, '2026-08-01 10:00:00+00:00', '[]', '', 0, '2026-08-01 09:00:00+00:00'), \
             ('c3', 'Nested', '', 3, '2026-09-24 14:20:07-07:00', '[]', '', 0, '2026-09-24 14:16:07-07:00');\
             UPDATE conversation_summaries SET parent_conversation_id = 'c1', nesting_depth = 1 WHERE conversation_id = 'c3';",
        )
        .unwrap();
        let since = parse_rfc3339_ms("2026-09-01T00:00:00Z").unwrap();
        let rows = read_from(&db, since, &HashSet::new()).unwrap();
        assert_eq!(rows.len(), 1);
        let c = &rows[0];
        assert_eq!(c.id, "c1");
        assert_eq!(c.cwd.as_deref(), Some("/Users/me/My Repo"));
        assert_eq!(c.status, "running");
        assert!(c.not_fully_idle);
        assert_eq!(c.last_modified, parse_rfc3339_ms("2026-09-24T21:20:07.434Z").unwrap());
        assert_eq!(c.last_input, parse_rfc3339_ms("2026-09-24T21:16:07Z"));
        // A pinned one comes back however old.
        let keep: HashSet<String> = ["c2".to_string()].into();
        assert_eq!(read_from(&db, since, &keep).unwrap().len(), 2);
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
}
