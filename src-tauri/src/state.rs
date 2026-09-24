//! What you've seen and pinned. A finished turn is "Your turn" until you open the session from
//! Codealong or mark it seen. Pinned sessions stay at the top of the Sessions view, in your
//! order. Stored in `state.json` next to the config.

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
        if std::fs::create_dir_all(support_dir()).is_ok() {
            if let Ok(text) = serde_json::to_string_pretty(self) {
                let _ = std::fs::write(path(), text);
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

    /// Replaces the pinned list, keeping the first of any repeats.
    pub fn set_pinned(&mut self, keys: Vec<String>) {
        let mut seen = std::collections::HashSet::new();
        self.pinned = keys.into_iter().filter(|k| !k.is_empty() && seen.insert(k.clone())).collect();
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
