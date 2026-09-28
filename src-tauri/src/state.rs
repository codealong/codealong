//! What you've seen, pinned and snoozed. A finished turn is "Your turn" until you open the
//! session from Codealong or mark it seen. Pinned sessions stay at the top of the Sessions view
//! and pinned worktrees at the top of the Worktrees view, each in your order. A snoozed session
//! stays off the floating bar's Needs you and Your turn until its time (`board::apply_snooze`).
//! Stored in `state.json` next to the config.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::config::support_dir;
use crate::util::{now_ms, DAY_MS};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SeenState {
    /// Turns that ended before Codealong first ran count as seen.
    #[serde(default)]
    pub first_run: i64,
    /// Session key → when you last saw it.
    #[serde(default)]
    pub seen: HashMap<String, i64>,
    /// Pinned session keys, in the order you put them.
    #[serde(default)]
    pub pinned: Vec<String>,
    /// Pinned worktrees by lane id (the worktree's path), in the order you put them.
    #[serde(default)]
    pub pinned_worktrees: Vec<String>,
    /// Sessions archived from Codealong, and when.
    #[serde(default)]
    pub archived: Vec<ArchivedSession>,
    /// Account folders Codealong has read (every one config.toml has listed) or been told to
    /// leave, as `detect::folder_key` writes them. Only others are suggested as new.
    #[serde(default)]
    pub known_accounts: Vec<String>,
    /// Sessions snoozed from Codealong, the latest snooze of each.
    #[serde(default)]
    pub snoozed: Vec<Snooze>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Snooze {
    pub key: String,
    /// When it was snoozed: a prompt sent to the session after this ends the snooze.
    pub at: i64,
    /// When it comes back.
    pub until: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ArchivedSession {
    pub key: String,
    pub at: i64,
}

impl SeenState {
    pub fn load() -> SeenState {
        let loaded = std::fs::read_to_string(path())
            .ok()
            .and_then(|t| serde_json::from_str::<SeenState>(&t).ok())
            .filter(|s| s.first_run > 0);
        loaded.unwrap_or_else(|| SeenState { first_run: now_ms(), ..SeenState::default() })
    }

    pub fn save(&mut self) {
        let cutoff = now_ms() - 30 * DAY_MS;
        self.seen.retain(|_, at| *at >= cutoff);
        self.snoozed.retain(|z| z.until >= cutoff);
        if std::fs::create_dir_all(support_dir()).is_ok() {
            if let Ok(text) = serde_json::to_string_pretty(self) {
                // Through a temporary file: Settings reads this while the engine writes it.
                let tmp = path().with_extension("json.tmp");
                if std::fs::write(&tmp, text).is_ok() {
                    let _ = std::fs::rename(&tmp, path());
                }
            }
        }
    }

    /// When the session was last seen, never earlier than the first run.
    pub fn seen_at(&self, key: &str) -> i64 {
        self.seen.get(key).copied().unwrap_or(0).max(self.first_run)
    }

    pub fn mark(&mut self, key: &str, at: i64) {
        self.seen.insert(key.to_string(), at);
    }

    /// Snoozes a session until `until`, in place of any snooze it had.
    pub fn snooze(&mut self, key: &str, at: i64, until: i64) {
        self.snoozed.retain(|z| z.key != key);
        self.snoozed.push(Snooze { key: key.to_string(), at, until });
    }

    /// Ends a session's snooze now, as if its time had come: it comes back to the bar.
    pub fn wake(&mut self, key: &str, now: i64) {
        if let Some(z) = self.snoozed.iter_mut().find(|z| z.key == key && z.until > now) {
            z.until = now;
        }
    }

    pub fn snooze_of(&self, key: &str) -> Option<&Snooze> {
        self.snoozed.iter().find(|z| z.key == key)
    }

    /// Replaces the pinned list, keeping the first of any repeats.
    pub fn set_pinned(&mut self, keys: Vec<String>) {
        self.pinned = unique(keys);
    }

    /// Replaces the pinned worktrees, keeping the first of any repeats.
    pub fn set_pinned_worktrees(&mut self, ids: Vec<String>) {
        self.pinned_worktrees = unique(ids);
    }

    /// Records an archive (or its undoing) made from Codealong.
    pub fn set_archived(&mut self, key: &str, archived: bool, at: i64) {
        self.archived.retain(|a| a.key != key);
        if archived {
            self.archived.push(ArchivedSession { key: key.to_string(), at });
            self.pinned.retain(|k| k != key);
        }
    }

    /// When the session was archived from Codealong.
    pub fn archived_at(&self, key: &str) -> Option<i64> {
        self.archived.iter().find(|a| a.key == key).map(|a| a.at)
    }

    /// Adds account folders to the known ones. True when one wasn't known before.
    pub fn know_accounts(&mut self, keys: impl IntoIterator<Item = String>) -> bool {
        let mut added = false;
        for k in keys {
            if !k.is_empty() && !self.known_accounts.contains(&k) {
                self.known_accounts.push(k);
                added = true;
            }
        }
        added
    }

    /// Session ids archived from Codealong for one account.
    pub fn archived_ids(&self, harness: &str, account: &str) -> std::collections::HashSet<String> {
        let prefix = format!("{harness}:{account}:");
        self.archived.iter().filter_map(|a| a.key.strip_prefix(&prefix).map(str::to_string)).collect()
    }

    /// Session ids pinned for one account (`<harness>:<account>:<id>` keys).
    pub fn pinned_ids(&self, harness: &str, account: &str) -> std::collections::HashSet<String> {
        let prefix = format!("{harness}:{account}:");
        self.pinned.iter().filter_map(|k| k.strip_prefix(&prefix).map(str::to_string)).collect()
    }
}

fn path() -> PathBuf {
    support_dir().join("state.json")
}

/// The keys in order, without empty ones or repeats.
fn unique(keys: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    keys.into_iter().filter(|k| !k.is_empty() && seen.insert(k.clone())).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knows_account_folders_once() {
        let mut s = SeenState::default();
        assert!(s.know_accounts(["/Users/me/.claude".to_string(), "/Users/me/.codex".to_string()]));
        assert!(!s.know_accounts(["/Users/me/.claude".to_string(), String::new()]));
        assert!(s.know_accounts(["/Users/me/.claude2".to_string()]));
        assert_eq!(s.known_accounts, vec!["/Users/me/.claude", "/Users/me/.codex", "/Users/me/.claude2"]);
    }

    #[test]
    fn first_run_counts_as_seen() {
        let mut s = SeenState { first_run: 1_000, ..SeenState::default() };
        assert_eq!(s.seen_at("claude:claude:a"), 1_000);
        s.mark("claude:claude:a", 5_000);
        assert_eq!(s.seen_at("claude:claude:a"), 5_000);
    }

    #[test]
    fn pins_keep_their_order_without_repeats() {
        let mut s = SeenState::default();
        s.set_pinned(vec!["codex:codex:b".into(), "claude:claude2:a".into(), "codex:codex:b".into(), "".into()]);
        assert_eq!(s.pinned, vec!["codex:codex:b".to_string(), "claude:claude2:a".to_string()]);
        assert_eq!(s.pinned_ids("claude", "claude2").into_iter().collect::<Vec<_>>(), vec!["a".to_string()]);
        let old: SeenState = serde_json::from_str(r#"{"firstRun":5,"seen":{}}"#).unwrap();
        assert!(old.pinned.is_empty());
    }

    #[test]
    fn pinned_worktrees_keep_their_order_apart_from_pinned_sessions() {
        let mut s = SeenState::default();
        s.set_pinned(vec!["claude:claude:a".into()]);
        s.set_pinned_worktrees(vec!["/r/app3".into(), "/r/main".into(), "/r/app3".into(), "".into()]);
        assert_eq!(s.pinned_worktrees, vec!["/r/app3".to_string(), "/r/main".to_string()]);
        assert_eq!(s.pinned, vec!["claude:claude:a".to_string()]);
        let old: SeenState = serde_json::from_str(r#"{"firstRun":5,"seen":{},"pinned":["claude:claude:a"]}"#).unwrap();
        assert!(old.pinned_worktrees.is_empty());
        let text = serde_json::to_string(&s).unwrap();
        assert!(text.contains(r#""pinnedWorktrees":["/r/app3","/r/main"]"#));
    }

    #[test]
    fn a_snooze_replaces_the_last_and_waking_brings_its_time_forward() {
        let mut s = SeenState::default();
        s.snooze("claude:claude:a", 10, 100);
        s.snooze("claude:claude:a", 20, 200);
        assert_eq!(s.snoozed, vec![Snooze { key: "claude:claude:a".into(), at: 20, until: 200 }]);
        s.wake("claude:claude:a", 50);
        assert_eq!(s.snooze_of("claude:claude:a").map(|z| (z.at, z.until)), Some((20, 50)));
        // A snooze whose time has passed stays as it ended.
        s.wake("claude:claude:a", 80);
        assert_eq!(s.snooze_of("claude:claude:a").map(|z| z.until), Some(50));
        let old: SeenState = serde_json::from_str(r#"{"firstRun":5,"seen":{}}"#).unwrap();
        assert!(old.snoozed.is_empty());
    }

    #[test]
    fn archiving_unpins_and_can_be_undone() {
        let mut s = SeenState::default();
        s.set_pinned(vec!["codex:codex:t1".into()]);
        s.set_archived("codex:codex:t1", true, 50);
        assert_eq!(s.archived_at("codex:codex:t1"), Some(50));
        assert!(s.pinned.is_empty());
        assert!(s.archived_ids("codex", "codex").contains("t1"));
        s.set_archived("codex:codex:t1", false, 60);
        assert_eq!(s.archived_at("codex:codex:t1"), None);
    }
}
