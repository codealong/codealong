//! What you've seen, pinned and snoozed. A finished turn is "Your turn" until you open the
//! session from Codealong or mark it seen, and one you marked unread is Your turn again until
//! you see it or prompt it (`board::apply_unread`). Pinned sessions stay at the top of the Sessions view
//! and pinned worktrees at the top of the Worktrees view, each in your order. A snoozed session
//! stays off the floating bar's Needs you and Your turn until its time (`board::apply_snooze`),
//! and a PR whose review you snoozed stays out of Review requested, the bar's reviews and the
//! mark until its time (`board::apply_review_snoozes`). A quiet session's turns don't wait on you
//! until you make it heard again: it waits on you only when it needs you (`board::apply_quiet`).
//! Stored in `state.json` next to the config.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::config::support_dir;
use crate::store;
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
    /// Pinned PRs by key (`owner/name#number`), in the order you put them.
    #[serde(default)]
    pub pinned_prs: Vec<String>,
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
    /// Reviews asked of you that you snoozed from Codealong, by PR key (`owner/name#number`), the
    /// latest snooze of each.
    #[serde(default)]
    pub review_snoozes: Vec<Snooze>,
    /// Sessions you marked unread, and when: the latest mark of each.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unread: Vec<UnreadMark>,
    /// Sessions you quieted, by key, in the order you quieted them. One stays quiet until you make
    /// it heard again, whatever it does meanwhile: answering it, or archiving it, doesn't end it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub quiet: Vec<String>,
    /// Helpers you made sessions of your own (Make it my own), by key: no longer helpers
    /// (`Session::helper`), for good.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub own: Vec<String>,
}

/// A session marked unread from Codealong.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UnreadMark {
    pub key: String,
    pub at: i64,
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
    /// The state of a first run: nothing seen, pinned, archived or snoozed, and turns that
    /// ended before now count as seen.
    pub fn fresh() -> SeenState {
        SeenState { first_run: now_ms(), ..SeenState::default() }
    }

    /// state.json, or a first run's state when there's no file yet. A file that's there but
    /// doesn't parse is an error, told apart from a first run so that whoever holds the state
    /// knows not to save over it: the pins, archives and snoozes in it are still yours, and an
    /// older build may just not know a newer one's shape.
    pub fn load() -> Result<SeenState> {
        Self::load_from(&path())
    }

    fn load_from(path: &Path) -> Result<SeenState> {
        let Some(mut state) = store::read::<SeenState>(path, "what you've seen, pinned and snoozed")? else { return Ok(Self::fresh()) };
        // A file without a first run keeps the rest of what it says.
        if state.first_run <= 0 {
            state.first_run = now_ms();
        }
        Ok(state)
    }

    pub fn save(&mut self) -> Result<()> {
        self.save_to(&path())
    }

    fn save_to(&mut self, path: &Path) -> Result<()> {
        let cutoff = now_ms() - 30 * DAY_MS;
        self.seen.retain(|_, at| *at >= cutoff);
        self.snoozed.retain(|z| z.until >= cutoff);
        self.review_snoozes.retain(|z| z.until >= cutoff);
        self.unread.retain(|u| u.at >= cutoff);
        // Through a temporary file: Settings reads this while the engine writes it.
        store::save(path, self)
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
        snooze_in(&mut self.snoozed, key, at, until);
    }

    /// Ends a session's snooze now, as if its time had come: it comes back to the bar.
    pub fn wake(&mut self, key: &str, now: i64) {
        wake_in(&mut self.snoozed, key, now);
    }

    pub fn snooze_of(&self, key: &str) -> Option<&Snooze> {
        self.snoozed.iter().find(|z| z.key == key)
    }

    /// Marks a session unread at `at`, in place of any mark it had: it's Your turn again until
    /// it's seen or prompted after that.
    pub fn mark_unread(&mut self, key: &str, at: i64) {
        self.unread.retain(|u| u.key != key);
        self.unread.push(UnreadMark { key: key.to_string(), at });
    }

    pub fn unread_of(&self, key: &str) -> Option<&UnreadMark> {
        self.unread.iter().find(|u| u.key == key)
    }

    /// Quiets a session, or makes it heard again. True when that changed anything.
    pub fn set_quiet(&mut self, key: &str, quiet: bool) -> bool {
        let was = self.is_quiet(key);
        if quiet && !was {
            self.quiet.push(key.to_string());
        } else if !quiet && was {
            self.quiet.retain(|k| k != key);
        }
        was != quiet
    }

    pub fn is_quiet(&self, key: &str) -> bool {
        self.quiet.iter().any(|k| k == key)
    }

    /// Makes a helper a session of your own. True when it wasn't already.
    pub fn make_own(&mut self, key: &str) -> bool {
        if self.is_own(key) {
            return false;
        }
        self.own.push(key.to_string());
        true
    }

    pub fn is_own(&self, key: &str) -> bool {
        self.own.iter().any(|k| k == key)
    }

    /// Snoozes your review of a PR until `until`, in place of any snooze it had.
    pub fn snooze_review(&mut self, key: &str, at: i64, until: i64) {
        snooze_in(&mut self.review_snoozes, key, at, until);
    }

    /// Ends a review's snooze now: the PR waits on your review again.
    pub fn wake_review(&mut self, key: &str, now: i64) {
        wake_in(&mut self.review_snoozes, key, now);
    }

    /// Replaces the pinned list, keeping the first of any repeats.
    pub fn set_pinned(&mut self, keys: Vec<String>) {
        self.pinned = unique(keys);
    }

    /// Replaces the pinned worktrees, keeping the first of any repeats.
    pub fn set_pinned_worktrees(&mut self, ids: Vec<String>) {
        self.pinned_worktrees = unique(ids);
    }

    /// Replaces the pinned PRs, keeping the first of any repeats.
    pub fn set_pinned_prs(&mut self, keys: Vec<String>) {
        self.pinned_prs = unique(keys);
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

pub fn path() -> PathBuf {
    support_dir().join("state.json")
}

/// The account folders state.json knows, for those that only look: none when it doesn't read.
pub fn known_accounts() -> Vec<String> {
    SeenState::load().map(|s| s.known_accounts).unwrap_or_default()
}

fn snooze_in(list: &mut Vec<Snooze>, key: &str, at: i64, until: i64) {
    list.retain(|z| z.key != key);
    list.push(Snooze { key: key.to_string(), at, until });
}

/// A snooze woken stays, ended now, as one whose time has passed does.
fn wake_in(list: &mut [Snooze], key: &str, now: i64) {
    if let Some(z) = list.iter_mut().find(|z| z.key == key && z.until > now) {
        z.until = now;
    }
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
    fn no_file_is_a_first_run_and_what_is_saved_reads_back() {
        let dir = store::test_dir("state-first-run");
        let path = dir.join("state.json");
        let before = now_ms();
        let mut s = SeenState::load_from(&path).unwrap();
        assert!(s.first_run >= before);
        assert_eq!(s, SeenState { first_run: s.first_run, ..SeenState::default() });
        assert!(!path.exists(), "reading makes no file");
        s.set_pinned(vec!["codex:codex:b".into()]);
        s.snooze("claude:claude:a", now_ms(), now_ms() + 60_000);
        s.mark("claude:claude:a", now_ms());
        s.mark("claude:claude:old", now_ms() - 31 * DAY_MS);
        s.save_to(&path).unwrap();
        assert!(!s.seen.contains_key("claude:claude:old"), "marks older than a month are let go");
        assert_eq!(SeenState::load_from(&path).unwrap(), s);
        // A file that doesn't say when the first run was keeps the rest of what it says.
        std::fs::write(&path, r#"{"pinned":["codex:codex:b"]}"#).unwrap();
        let s = SeenState::load_from(&path).unwrap();
        assert!(s.first_run >= before);
        assert_eq!(s.pinned, ["codex:codex:b"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Codealong and Codealong Dev share state.json: a build that doesn't know the other's
    /// shape gets an error, not a first run to save over your pins, archives and snoozes.
    #[test]
    fn a_file_that_does_not_read_is_an_error_not_a_first_run() {
        let dir = store::test_dir("state-unreadable");
        let path = dir.join("state.json");
        for bytes in [r#"{"firstRun":5,"pinned":["a"],"snoozed":[{"key":"a","at":1,"until":"tomorrow"}]}"#, r#"{"firstRun":5,"pinned":["a""#, ""] {
            std::fs::write(&path, bytes).unwrap();
            let e = SeenState::load_from(&path).unwrap_err().to_string();
            assert!(e.starts_with("state.json doesn't read as what you've seen, pinned and snoozed (") && e.ends_with("Codealong won't change it until it does."), "{e}");
            assert_eq!(std::fs::read_to_string(&path).unwrap(), bytes);
        }
        // A save that can't land says so.
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        assert!(SeenState::load_from(&path).is_err());
        let e = SeenState::fresh().save_to(&path).unwrap_err().to_string();
        assert!(e.starts_with("Couldn't write "), "{e}");
        std::fs::remove_dir_all(&dir).unwrap();
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
    fn pinned_prs_are_a_list_of_their_own() {
        let mut s = SeenState::default();
        s.set_pinned_worktrees(vec!["/r/main".into()]);
        s.set_pinned_prs(vec!["o/r#12".into(), "o/r#3".into(), "o/r#12".into(), "".into()]);
        assert_eq!(s.pinned_prs, vec!["o/r#12".to_string(), "o/r#3".to_string()]);
        assert_eq!(s.pinned_worktrees, vec!["/r/main".to_string()]);
        let old: SeenState = serde_json::from_str(r#"{"firstRun":5,"seen":{},"pinnedWorktrees":["/r/main"]}"#).unwrap();
        assert!(old.pinned_prs.is_empty());
        assert!(serde_json::to_string(&s).unwrap().contains(r#""pinnedPrs":["o/r#12","o/r#3"]"#));
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
    fn review_snoozes_are_a_list_of_their_own() {
        let mut s = SeenState::default();
        s.snooze("claude:claude:a", 10, 100);
        s.snooze_review("o/r#12", 10, 100);
        s.snooze_review("o/r#12", 20, 300);
        assert_eq!(s.review_snoozes, vec![Snooze { key: "o/r#12".into(), at: 20, until: 300 }]);
        assert_eq!(s.snoozed.len(), 1);
        s.wake_review("o/r#12", 50);
        assert_eq!(s.review_snoozes[0].until, 50);
        assert_eq!(s.snooze_of("claude:claude:a").map(|z| z.until), Some(100));
        assert!(serde_json::to_string(&s).unwrap().contains(r#""reviewSnoozes":[{"key":"o/r#12","at":20,"until":50}]"#));
        let old: SeenState = serde_json::from_str(r#"{"firstRun":5,"seen":{}}"#).unwrap();
        assert!(old.review_snoozes.is_empty());
    }

    #[test]
    fn a_session_stays_quiet_until_made_heard_again() {
        let mut s = SeenState::default();
        assert!(s.set_quiet("claude:claude:a", true));
        assert!(!s.set_quiet("claude:claude:a", true), "once");
        assert!(s.set_quiet("codex:codex:b", true));
        assert_eq!(s.quiet, vec!["claude:claude:a".to_string(), "codex:codex:b".to_string()]);
        // Archiving doesn't end it.
        s.set_archived("claude:claude:a", true, 10);
        assert!(s.is_quiet("claude:claude:a"));
        assert!(serde_json::to_string(&s).unwrap().contains(r#""quiet":["claude:claude:a","codex:codex:b"]"#));
        assert!(s.set_quiet("claude:claude:a", false));
        assert!(!s.set_quiet("claude:claude:a", false));
        assert_eq!(s.quiet, vec!["codex:codex:b".to_string()]);
        // An older file has none, and a state with none says nothing of it.
        let old: SeenState = serde_json::from_str(r#"{"firstRun":5,"seen":{}}"#).unwrap();
        assert!(old.quiet.is_empty());
        assert!(!serde_json::to_string(&old).unwrap().contains("quiet"));
    }

    #[test]
    fn a_helper_made_your_own_stays_yours() {
        let mut s = SeenState::default();
        assert!(s.make_own("codex:codex:h"));
        assert!(!s.make_own("codex:codex:h"), "once");
        assert!(s.is_own("codex:codex:h") && !s.is_own("codex:codex:other"));
        assert!(serde_json::to_string(&s).unwrap().contains(r#""own":["codex:codex:h"]"#));
        let old: SeenState = serde_json::from_str(r#"{"firstRun":5,"seen":{}}"#).unwrap();
        assert!(old.own.is_empty() && !serde_json::to_string(&old).unwrap().contains("\"own\""));
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
