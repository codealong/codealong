//! A reading the engine takes of every account now and then, off its thread: whether its provider
//! may train on it (training.rs) and the MCP servers its agents have (mcp.rs). Each changes only
//! when someone changes it, so every account is read at launch and every hour, again when the
//! usage bar's refresh is clicked, and an account just added at once. A read that couldn't tell
//! keeps the last one that could.

use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

use crate::config::AccountCfg;

/// How often every account is read again.
pub const EVERY: Duration = Duration::from_secs(60 * 60);
/// How soon a refresh may read them all again.
pub const AGAIN_AFTER: Duration = Duration::from_secs(60);

/// One account's reading.
pub trait Reading: Clone + PartialEq {
    fn account(&self) -> &str;

    /// `prev`, kept and marked as not fresh, when this reading couldn't tell and `prev` could;
    /// none when this one stands.
    fn keeping(&self, prev: &Self) -> Option<Self>;
}

/// Which account a name stands for: its harness and folder.
pub fn identity(a: &AccountCfg) -> String {
    format!("{}\n{}", a.harness, a.home)
}

/// Every account's last reading, and when to read them again: the engine's.
pub struct AccountPoll<T> {
    by_account: BTreeMap<String, T>,
    running: bool,
    last: Option<Instant>,
    /// A refresh asked for a read sooner than `EVERY`.
    asked: bool,
    /// What each account was last asked as (`identity`), by name: one that changes under the
    /// same name is another account, read afresh.
    asked_as: BTreeMap<String, String>,
    /// Accounts whose answer from the read that's out is from before: read again after Codealong
    /// changed it, or another account under the name since. It's passed over.
    pass_over: BTreeSet<String>,
}

impl<T> Default for AccountPoll<T> {
    fn default() -> Self {
        AccountPoll { by_account: BTreeMap::new(), running: false, last: None, asked: false, asked_as: BTreeMap::new(), pass_over: BTreeSet::new() }
    }
}

impl<T: Reading> AccountPoll<T> {
    /// The accounts to read now, at `now`: every one at first, once `EVERY` has passed or a
    /// refresh asked (at most every `AGAIN_AFTER`), and an account not read yet (one just added);
    /// none while a read is under way, nor one turned off. Marks a read under way when it hands one out; `on_read`
    /// takes the mark off.
    pub fn due(&mut self, accounts: &[AccountCfg], now: Instant) -> Vec<AccountCfg> {
        if self.running {
            return Vec::new();
        }
        let since = self.last.map(|t| now.duration_since(t));
        let all = since.is_none_or(|s| s >= EVERY || (self.asked && s >= AGAIN_AFTER));
        let asks: Vec<AccountCfg> = accounts.iter().filter(|a| !a.disabled && (all || !self.by_account.contains_key(&a.name))).cloned().collect();
        if asks.is_empty() {
            return asks;
        }
        if all {
            self.last = Some(now);
            self.asked = false;
        }
        for a in &asks {
            self.asked_as.insert(a.name.clone(), identity(a));
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
    pub fn on_read(&mut self, list: Option<Vec<T>>) -> bool {
        self.running = false;
        let before = std::mem::take(&mut self.pass_over);
        list.into_iter().flatten().filter(|t| !before.contains(t.account())).fold(false, |changed, t| self.take(t) || changed)
    }

    /// An account's reading right after Codealong changed it, for the account `asked_as`
    /// (`identity`) when the change began. One whose name stands for another account by now,
    /// or for none, is dropped. Says whether it changed.
    pub fn on_changed(&mut self, asked_as: &str, fresh: T) -> bool {
        if self.asked_as.get(fresh.account()).map(String::as_str) != Some(asked_as) {
            return false;
        }
        if self.running {
            self.pass_over.insert(fresh.account().to_string());
        }
        self.take(fresh)
    }

    /// A reading that couldn't tell keeps the last one that could, saying why it's not fresh.
    pub(crate) fn take(&mut self, fresh: T) -> bool {
        let prev = self.by_account.get(fresh.account());
        let fresh = prev.and_then(|p| fresh.keeping(p)).unwrap_or(fresh);
        if prev == Some(&fresh) {
            return false;
        }
        self.by_account.insert(fresh.account().to_string(), fresh);
        true
    }

    /// Accounts config.toml no longer lists are forgotten, and so is a name that now stands for
    /// another harness or folder, which is read afresh at the next `due`.
    pub fn keep_only(&mut self, accounts: &[AccountCfg]) {
        let same = |name: &str, asked: &str| accounts.iter().any(|a| a.name == name && identity(a) == asked);
        let gone: Vec<String> = self.asked_as.iter().filter(|(name, asked)| !same(name, asked)).map(|(name, _)| name.clone()).collect();
        for name in gone {
            self.asked_as.remove(&name);
            self.by_account.remove(&name);
            if self.running {
                self.pass_over.insert(name);
            }
        }
        self.by_account.retain(|name, _| accounts.iter().any(|a| &a.name == name));
    }

    /// The readings, in config order; an account not read yet, or turned off, has none.
    pub fn list(&self, accounts: &[AccountCfg]) -> Vec<T> {
        accounts.iter().filter(|a| !a.disabled).filter_map(|a| self.by_account.get(&a.name).cloned()).collect()
    }
}
