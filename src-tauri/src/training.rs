//! Whether each account's provider may train its models on what the account sends, and turning
//! that off. Where the setting is kept and how it's changed is each harness's own
//! (`Adapter::training`, `Adapter::stop_training`): Claude's "Help improve Claude" in its account's
//! privacy settings (claude/training.rs), and so on for the others. The engine reads every
//! account's at launch and every hour (`TrainingPoll`), again when the usage bar's refresh is
//! clicked, and once more right after Codealong turns one off, to see that it took. The usage bar
//! flags an account whose provider may train on it (src/ui/UsageBar.tsx), and Settings ›
//! Accounts says where each one stands.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

use crate::config::AccountCfg;
use crate::util::now_ms;

/// How often every account's setting is read again: it changes only when someone changes it.
pub const EVERY: Duration = Duration::from_secs(60 * 60);
/// How soon a refresh may read them all again.
const AGAIN_AFTER: Duration = Duration::from_secs(60);

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum State {
    /// The provider may train on what the account sends.
    On,
    /// It may not: the setting is off, or the plan is one the provider never trains on.
    Off,
    /// The account hasn't been asked yet, so nothing is chosen: the provider asks the next time
    /// it's used, and turning it off now settles it.
    Unset,
    /// Codealong can't tell: `note` says why.
    Unknown,
}

/// Where one account stands, for the board.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Training {
    pub account: String,
    pub state: State,
    /// Why, in words: what the setting is called and where it is, or why the plan is never
    /// trained on, or why it couldn't be read.
    pub note: String,
    /// Codealong can turn training off itself (`Adapter::stop_training`).
    pub can_stop: bool,
    /// The provider's page where a person changes it by hand.
    pub url: Option<String>,
    /// When it was read. A read that failed keeps the last answer, and its time.
    pub as_of: Option<i64>,
}

impl Training {
    pub fn new(account: &AccountCfg, state: State, note: impl Into<String>) -> Training {
        Training { account: account.name.clone(), state, note: note.into(), can_stop: false, url: None, as_of: Some(now_ms()) }
    }

    pub fn unknown(account: &AccountCfg, note: impl Into<String>) -> Training {
        Training { as_of: None, ..Training::new(account, State::Unknown, note) }
    }

    pub fn at(self, url: &str) -> Training {
        Training { url: Some(url.to_string()), ..self }
    }

    pub fn stoppable(self) -> Training {
        Training { can_stop: true, ..self }
    }
}

/// Reads one account's setting where its harness keeps it. It can take seconds.
pub fn read(account: &AccountCfg) -> Training {
    match crate::harness::of(account) {
        Some(h) => h.training(account),
        None => Training::unknown(account, "Codealong doesn't know this account's harness."),
    }
}

/// Reads every account's, one after another: for the engine's background read and the snapshot.
pub fn read_all(accounts: &[AccountCfg]) -> Vec<Training> {
    accounts.iter().map(read).collect()
}

/// Turns training off for an account, then reads it again to see that it took.
pub fn stop(account: &AccountCfg) -> Result<Training> {
    let h = crate::harness::of(account).ok_or_else(|| anyhow!("Codealong doesn't know {}'s harness.", account.name))?;
    h.stop_training(account)?;
    Ok(h.training(account))
}

/// Every account's last reading, and when to read them again: the engine's.
#[derive(Default)]
pub struct TrainingPoll {
    by_account: BTreeMap<String, Training>,
    running: bool,
    last: Option<Instant>,
    /// A refresh asked for a read sooner than `EVERY`.
    asked: bool,
}

impl TrainingPoll {
    /// The accounts to read now, at `now`: every one at first, once `EVERY` has passed or a
    /// refresh asked (at most every `AGAIN_AFTER`), and an account not read yet (one just added);
    /// none while a read is under way. Marks a read under way when it hands one out; `on_read`
    /// takes the mark off.
    pub fn due(&mut self, accounts: &[AccountCfg], now: Instant) -> Vec<AccountCfg> {
        if self.running {
            return Vec::new();
        }
        let since = self.last.map(|t| now.duration_since(t));
        let all = since.is_none_or(|s| s >= EVERY || (self.asked && s >= AGAIN_AFTER));
        let asks: Vec<AccountCfg> = accounts.iter().filter(|a| all || !self.by_account.contains_key(&a.name)).cloned().collect();
        if asks.is_empty() {
            return asks;
        }
        if all {
            self.last = Some(now);
            self.asked = false;
        }
        self.running = true;
        asks
    }

    /// A refresh: read them all again at the next `due`, unless they were just read.
    pub fn on_refresh(&mut self) {
        self.asked = true;
    }

    /// The background read is over, however it ended. A reading that couldn't tell keeps the
    /// last one that could. Says whether anything changed.
    pub fn on_read(&mut self, list: Option<Vec<Training>>) -> bool {
        self.running = false;
        list.into_iter().flatten().fold(false, |changed, t| self.take(t) || changed)
    }

    /// One account's reading, read on its own (after Codealong turned it off). Says whether it
    /// changed.
    pub fn take(&mut self, fresh: Training) -> bool {
        let prev = self.by_account.get(&fresh.account);
        let fresh = match prev {
            Some(p) if fresh.state == State::Unknown && p.state != State::Unknown => p.clone(),
            _ => fresh,
        };
        if prev == Some(&fresh) {
            return false;
        }
        self.by_account.insert(fresh.account.clone(), fresh);
        true
    }

    /// Accounts config.toml no longer lists are forgotten.
    pub fn keep_only(&mut self, accounts: &[AccountCfg]) {
        self.by_account.retain(|name, _| accounts.iter().any(|a| &a.name == name));
    }

    /// The readings, in config order; an account not read yet has none.
    pub fn list(&self, accounts: &[AccountCfg]) -> Vec<Training> {
        accounts.iter().filter_map(|a| self.by_account.get(&a.name).cloned()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account(name: &str) -> AccountCfg {
        AccountCfg { name: name.into(), harness: "claude".into(), home: format!("~/.{name}"), command: None, start: None, preferred: false, acp: None, monthly: None, per_token: false }
    }

    fn reading(name: &str, state: State) -> Training {
        Training { account: name.into(), state, note: String::new(), can_stop: false, url: None, as_of: Some(1) }
    }

    #[test]
    fn every_account_is_read_at_first_then_hourly_and_a_new_one_at_once() {
        let mut poll = TrainingPoll::default();
        let accounts = vec![account("claude"), account("codex")];
        let t0 = Instant::now();
        let names = |asks: Vec<AccountCfg>| asks.into_iter().map(|a| a.name).collect::<Vec<_>>();
        assert_eq!(names(poll.due(&accounts, t0)), ["claude", "codex"]);
        assert!(poll.due(&accounts, t0).is_empty(), "nothing while a read is out");
        assert!(poll.on_read(Some(vec![reading("claude", State::On), reading("codex", State::Off)])));
        assert!(poll.due(&accounts, t0 + Duration::from_secs(60)).is_empty());
        let more = vec![account("claude"), account("codex"), account("claude2")];
        assert_eq!(names(poll.due(&more, t0 + Duration::from_secs(60))), ["claude2"], "an account just added");
        poll.on_read(None);
        assert_eq!(names(poll.due(&accounts, t0 + EVERY)), ["claude", "codex"]);
    }

    #[test]
    fn a_refresh_reads_again_but_not_twice_in_a_minute() {
        let mut poll = TrainingPoll::default();
        let accounts = vec![account("claude")];
        let t0 = Instant::now();
        poll.due(&accounts, t0);
        poll.on_read(Some(vec![reading("claude", State::On)]));
        poll.on_refresh();
        assert!(poll.due(&accounts, t0 + Duration::from_secs(10)).is_empty(), "just read");
        assert_eq!(poll.due(&accounts, t0 + AGAIN_AFTER).len(), 1);
        poll.on_read(None);
        assert!(poll.due(&accounts, t0 + AGAIN_AFTER * 2).is_empty(), "the refresh was used");
    }

    #[test]
    fn a_read_that_cant_tell_keeps_the_last_that_could() {
        let mut poll = TrainingPoll::default();
        poll.take(reading("claude", State::On));
        assert!(!poll.take(reading("claude", State::Unknown)));
        assert_eq!(poll.list(&[account("claude")])[0].state, State::On);
        assert!(poll.take(reading("claude", State::Off)));
        assert_eq!(poll.list(&[account("claude")])[0].state, State::Off);
        assert!(poll.take(reading("codex", State::Unknown)), "nothing to keep: it says it can't tell");
    }

    #[test]
    fn readings_follow_config_order_and_removed_accounts_go() {
        let mut poll = TrainingPoll::default();
        poll.take(reading("b", State::Off));
        poll.take(reading("a", State::On));
        let accounts = vec![account("b"), account("a"), account("c")];
        assert_eq!(poll.list(&accounts).iter().map(|t| t.account.as_str()).collect::<Vec<_>>(), ["b", "a"]);
        poll.keep_only(&[account("a")]);
        assert_eq!(poll.list(&accounts).len(), 1);
    }
}
