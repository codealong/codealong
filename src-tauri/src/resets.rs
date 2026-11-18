//! Limit resets an account holds, and using one: a reset puts a plan's usage limits back to empty
//! before they'd reset by themselves. Which providers grant them, how they're read and how one is
//! used are each harness's own (`Adapter::resets`, `Adapter::use_reset`): Codex's reset credits
//! through its app-server (codex/resets.rs). Claude Code has resets too (`/limit-reset`), but
//! Anthropic offers them only to Claude Code itself, so they aren't read here (claude-code.md).
//! The engine reads every account's as accountpoll.rs says (`ResetPoll`), and once more right
//! after Codealong uses one. The usage bar marks an account that holds one, and its card lists
//! them with Use a reset (src/ui/UsageBar.tsx).

use std::collections::BTreeSet;
use std::sync::Mutex;

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

use crate::config::AccountCfg;

/// The resets one account holds, for the board.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Resets {
    pub account: String,
    /// How many it holds. The provider may list fewer of them in `credits`.
    pub available: u32,
    /// The ones listed, the first to expire first.
    pub credits: Vec<ResetCredit>,
    /// Codealong can use one itself (`Adapter::use_reset`).
    pub can_use: bool,
    /// Why there's nothing to show: the harness grants none Codealong can read, or the read
    /// failed, in words.
    pub note: Option<String>,
    /// When it was read; none when it couldn't be. A read that couldn't tell keeps the last
    /// answer that could, and its time, with why in `stale`.
    pub as_of: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stale: Option<String>,
}

/// One reset, as its provider describes it.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ResetCredit {
    /// The provider's own id for it, which using one can name.
    pub id: String,
    /// `Full reset`.
    pub title: Option<String>,
    /// Why it was granted, in the provider's words.
    pub description: Option<String>,
    pub granted_at: Option<i64>,
    /// When it can no longer be used; none when it doesn't expire.
    pub expires_at: Option<i64>,
}

/// How using one went, in words for the window's toast.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ResetUsed {
    pub account: String,
    /// A reset was spent and the limits are back to empty.
    pub reset: bool,
    pub message: String,
    /// What the account holds after, read again.
    pub after: Resets,
}

/// What the provider said to a use.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Outcome {
    /// One was spent and the limits it covers were reset.
    Reset,
    /// No limit was far enough along to reset, and nothing was spent.
    NothingToReset,
    /// The account had none left.
    NoCredit,
    /// The same attempt had already gone through.
    AlreadyRedeemed,
}

impl Resets {
    /// An account read just now.
    pub fn held(account: &AccountCfg, credits: Vec<ResetCredit>, available: u32, now: i64) -> Resets {
        let mut credits = credits;
        credits.sort_by_key(|c| c.expires_at.unwrap_or(i64::MAX));
        Resets { account: account.name.clone(), available: available.max(credits.len() as u32), credits, can_use: true, note: None, as_of: Some(now), stale: None }
    }

    /// An account with nothing Codealong can read, and why.
    pub fn none(account: &AccountCfg, note: impl Into<String>) -> Resets {
        Resets { account: account.name.clone(), available: 0, credits: Vec::new(), can_use: false, note: Some(note.into()), as_of: None, stale: None }
    }

    /// An account whose harness grants them, whose read failed, and why: it keeps the last read
    /// that worked.
    pub fn failed(account: &AccountCfg, why: impl Into<String>) -> Resets {
        Resets { can_use: true, ..Resets::none(account, why) }
    }
}

/// Reads one account's resets where its harness keeps them. It can take seconds.
pub fn read(account: &AccountCfg) -> Resets {
    match crate::harness::of(account) {
        Some(h) => h.resets(account),
        None => Resets::none(account, "Codealong doesn't know this account's harness."),
    }
}

/// Reads every account's but those turned off, one after another: for the engine's background
/// read and the snapshot.
pub fn read_all(accounts: &[AccountCfg]) -> Vec<Resets> {
    accounts.iter().filter(|a| !a.disabled).map(read).collect()
}

/// Uses one of the account's resets, `credit` when it names one, else the one its provider picks,
/// then reads what it holds after. A second use of the account's while one is under way is turned
/// away.
pub fn use_one(account: &AccountCfg, credit: Option<&str>) -> Result<ResetUsed> {
    let h = crate::harness::of(account).ok_or_else(|| anyhow!("Codealong doesn't know {}'s harness.", account.name))?;
    let _using = Using::begin(account)?;
    let outcome = h.use_reset(account, credit)?;
    let after = after_use(outcome, h.resets(account));
    Ok(ResetUsed { account: account.name.clone(), reset: outcome == Outcome::Reset, message: said(&account.name, outcome, &after), after })
}

/// The accounts a use is under way for, by `accountpoll::identity`.
static USING: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());

/// A use under way, until it's dropped.
struct Using(String);

/// A use turned away because another of the account's is under way: nothing was asked, and the
/// one under way says what's left.
#[derive(Debug)]
pub struct Busy(String);

impl std::fmt::Display for Busy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "A reset of {}'s is being used already.", self.0)
    }
}

impl std::error::Error for Busy {}

impl Using {
    fn begin(account: &AccountCfg) -> Result<Using> {
        let id = crate::accountpoll::identity(account);
        let mut using = USING.lock().map_err(|_| anyhow!("Codealong couldn't tell whether a reset of {}'s is being used.", account.name))?;
        if !using.insert(id.clone()) {
            return Err(Busy(account.name.clone()).into());
        }
        Ok(Using(id))
    }
}

impl Drop for Using {
    fn drop(&mut self) {
        if let Ok(mut using) = USING.lock() {
            using.remove(&self.0);
        }
    }
}

/// What the account holds after a use that failed, read again: it may have gone through
/// unanswered, so a read that fails offers nothing, as after one spent (`after_use`).
pub fn read_after_failure(account: &AccountCfg) -> Resets {
    after_use(Outcome::Reset, read(account))
}

/// What the account holds after a use, as read: when one was spent, or the provider said there
/// were none, and that read failed, nothing Codealong can use until a read works, rather than the
/// count from before it (which a failed read would keep, `keeping`) offering a reset that's gone.
fn after_use(outcome: Outcome, read: Resets) -> Resets {
    let fewer = matches!(outcome, Outcome::Reset | Outcome::AlreadyRedeemed | Outcome::NoCredit);
    if fewer && read.as_of.is_none() {
        return Resets { can_use: false, ..read };
    }
    read
}

/// What the toast says after a use.
fn said(account: &str, outcome: Outcome, after: &Resets) -> String {
    let left = match after.as_of.map(|_| after.available) {
        Some(0) => " None left.".to_string(),
        Some(1) => " 1 left.".to_string(),
        Some(n) => format!(" {n} left."),
        None => String::new(),
    };
    match outcome {
        Outcome::Reset => format!("{account}'s limits are reset.{left}"),
        Outcome::NothingToReset => format!("{account} has no limit to reset yet, so no reset was used."),
        Outcome::NoCredit => format!("{account} has no resets left."),
        Outcome::AlreadyRedeemed => format!("That reset of {account}'s had already gone through.{left}"),
    }
}

impl crate::accountpoll::Reading for Resets {
    fn account(&self) -> &str {
        &self.account
    }

    fn keeping(&self, prev: &Resets) -> Option<Resets> {
        (self.as_of.is_none() && self.can_use && prev.as_of.is_some()).then(|| Resets { stale: self.note.clone(), ..prev.clone() })
    }
}

/// Every account's last reading, and when to read them again: the engine's.
pub type ResetPoll = crate::accountpoll::AccountPoll<Resets>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accountpoll::identity;
    use std::time::Instant;

    fn account(name: &str) -> AccountCfg {
        AccountCfg { name: name.into(), harness: "codex".into(), home: format!("~/.{name}"), command: None, start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false, chrome_profile: None }
    }

    fn credit(id: &str, expires_at: Option<i64>) -> ResetCredit {
        ResetCredit { id: id.into(), title: Some("Full reset".into()), description: None, granted_at: Some(1), expires_at }
    }

    /// The first to expire comes first, one that never expires last, and a count the provider
    /// gives beyond its list stands.
    #[test]
    fn credits_go_by_when_they_expire() {
        let r = Resets::held(&account("codex"), vec![credit("b", Some(20)), credit("never", None), credit("a", Some(10))], 5, 100);
        assert_eq!(r.credits.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(), ["a", "b", "never"]);
        assert_eq!(r.available, 5);
        let listed_more = Resets::held(&account("codex"), vec![credit("a", None)], 0, 100);
        assert_eq!(listed_more.available, 1, "a count short of the list it came with is the list's");
    }

    #[test]
    fn a_read_that_fails_keeps_the_last_that_worked() {
        let mut poll = ResetPoll::default();
        let a = account("codex");
        poll.take(Resets::held(&a, vec![credit("a", Some(10))], 1, 100));
        let failed = Resets::failed(&a, "Codex didn't answer in time.");
        assert!(poll.take(failed));
        let kept = &poll.list(std::slice::from_ref(&a))[0];
        assert_eq!((kept.available, kept.as_of, kept.stale.as_deref()), (1, Some(100), Some("Codex didn't answer in time.")));
        // A harness that grants none has nothing to keep.
        let mut poll = ResetPoll::default();
        poll.take(Resets::none(&a, "none"));
        assert_eq!(poll.list(std::slice::from_ref(&a))[0].stale, None);
    }

    #[test]
    fn a_use_reads_again_and_overrides_a_read_already_out() {
        let mut poll = ResetPoll::default();
        let accounts = vec![account("codex")];
        poll.due(&accounts, Instant::now());
        let after = Resets::held(&accounts[0], vec![], 0, 200);
        assert!(poll.on_changed(&identity(&accounts[0]), after));
        poll.on_read(Some(vec![Resets::held(&accounts[0], vec![credit("a", None)], 1, 150)]));
        assert_eq!(poll.list(&accounts)[0].available, 0, "the read began before the use");
    }

    /// A spent reset doesn't come back on the card because the read after it failed.
    #[test]
    fn a_failed_read_after_a_use_offers_nothing_until_one_works() {
        let a = account("codex");
        let mut poll = ResetPoll::default();
        let accounts = vec![a.clone()];
        poll.due(&accounts, Instant::now());
        poll.on_read(Some(vec![Resets::held(&a, vec![credit("a", None)], 1, 100)]));
        let after = after_use(Outcome::Reset, Resets::failed(&a, "Codex didn't answer in time."));
        assert!(!after.can_use);
        poll.on_changed(&identity(&a), after);
        let shown = &poll.list(&accounts)[0];
        assert_eq!((shown.can_use, shown.available), (false, 0), "not the one from before");
        assert!(!after_use(Outcome::NoCredit, Resets::failed(&a, "x")).can_use, "none left, as the provider said");
        // Nothing spent, or a read that worked: as read.
        assert!(after_use(Outcome::NothingToReset, Resets::failed(&a, "x")).can_use);
        assert_eq!(after_use(Outcome::Reset, Resets::held(&a, vec![], 0, 200)).as_of, Some(200));
    }

    #[test]
    fn the_toast_says_how_it_went_and_what_is_left() {
        let a = account("codex");
        let two = Resets::held(&a, vec![credit("a", None), credit("b", None)], 2, 1);
        assert_eq!(said("codex", Outcome::Reset, &two), "codex's limits are reset. 2 left.");
        assert_eq!(said("codex", Outcome::Reset, &Resets::held(&a, vec![], 0, 1)), "codex's limits are reset. None left.");
        assert_eq!(said("codex", Outcome::Reset, &Resets::none(&a, "failed")), "codex's limits are reset.", "unread after: no count");
        assert_eq!(said("codex", Outcome::NothingToReset, &two), "codex has no limit to reset yet, so no reset was used.");
    }

    #[test]
    fn a_second_use_while_one_is_under_way_is_turned_away() {
        let a = AccountCfg { home: "~/.codex-using-test".into(), ..account("codex") };
        let first = Using::begin(&a).unwrap();
        let busy = Using::begin(&a).err().unwrap();
        assert!(busy.downcast_ref::<Busy>().is_some(), "the engine tells it from a failed use by this: {busy}");
        assert!(Using::begin(&AccountCfg { home: "~/.codex-using-test2".into(), ..account("codex2") }).is_ok(), "another account's is its own");
        drop(first);
        assert!(Using::begin(&a).is_ok(), "once it's over");
    }

    #[test]
    fn an_account_turned_off_is_never_asked() {
        let off = AccountCfg { disabled: true, ..account("codex") };
        assert!(read_all(std::slice::from_ref(&off)).is_empty());
    }
}
