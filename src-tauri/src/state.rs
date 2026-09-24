//! What you've seen. A finished turn is "Your turn" until you open the session from Codealong
//! or mark it seen. Stored in `state.json` next to the config.

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
}

impl SeenState {
    pub fn load() -> SeenState {
        let loaded = std::fs::read_to_string(path())
            .ok()
            .and_then(|t| serde_json::from_str::<SeenState>(&t).ok())
            .filter(|s| s.first_run > 0);
        loaded.unwrap_or_else(|| SeenState { first_run: now_ms(), seen: HashMap::new() })
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
}

fn path() -> PathBuf {
    support_dir().join("state.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_run_counts_as_seen() {
        let mut s = SeenState { first_run: 1_000, seen: HashMap::new() };
        assert_eq!(s.seen_at("claude:claude:a"), 1_000);
        s.mark("claude:claude:a", 5_000);
        assert_eq!(s.seen_at("claude:claude:a"), 5_000);
    }
}
