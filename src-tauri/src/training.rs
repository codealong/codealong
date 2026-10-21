//! Whether each account's provider may train its models on what the account sends, and turning
//! that off. Where the setting is kept and how it's changed is each harness's own
//! (`Adapter::training`, `Adapter::stop_training`): Claude's "Help improve Claude" in its account's
//! privacy settings (claude/training.rs), and so on for the others. The engine reads every
//! account's as accountpoll.rs says (`TrainingPoll`), and once more right after Codealong turns
//! one off, to see that it took. The usage bar
//! flags an account whose provider may train on it (src/ui/UsageBar.tsx), and Settings ›
//! Accounts says where each one stands.

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

use crate::config::AccountCfg;
use crate::util::now_ms;

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
    /// When it was read. A read that couldn't tell keeps the last answer that could, and its
    /// time, with why in `stale`.
    pub as_of: Option<i64>,
    /// Why the latest read couldn't tell, when this is an older answer kept.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stale: Option<String>,
}

impl Training {
    pub fn new(account: &AccountCfg, state: State, note: impl Into<String>) -> Training {
        Training { account: account.name.clone(), state, note: note.into(), can_stop: false, url: None, as_of: Some(now_ms()), stale: None }
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

/// Reads every account's but those turned off, one after another: for the engine's background
/// read and the snapshot.
pub fn read_all(accounts: &[AccountCfg]) -> Vec<Training> {
    accounts.iter().filter(|a| !a.disabled).map(read).collect()
}

/// Turns training off for an account, then reads it again to see that it took.
pub fn stop(account: &AccountCfg) -> Result<Training> {
    let h = crate::harness::of(account).ok_or_else(|| anyhow!("Codealong doesn't know {}'s harness.", account.name))?;
    h.stop_training(account)?;
    Ok(h.training(account))
}

impl crate::accountpoll::Reading for Training {
    fn account(&self) -> &str {
        &self.account
    }

    fn keeping(&self, prev: &Training) -> Option<Training> {
        (self.state == State::Unknown && prev.state != State::Unknown).then(|| Training { stale: Some(self.note.clone()), ..prev.clone() })
    }
}

/// Every account's last reading, and when to read them again: the engine's.
pub type TrainingPoll = crate::accountpoll::AccountPoll<Training>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accountpoll::{identity, AGAIN_AFTER, EVERY};
    use std::time::{Duration, Instant};

    fn account(name: &str) -> AccountCfg {
        AccountCfg { name: name.into(), harness: "claude".into(), home: format!("~/.{name}"), command: None, start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false }
    }

    fn reading(name: &str, state: State) -> Training {
        Training { account: name.into(), state, note: String::new(), can_stop: false, url: None, as_of: Some(1), stale: None }
    }

    /// An account turned off isn't asked, by the engine's poll or by the snapshot.
    #[test]
    fn an_account_turned_off_is_never_asked() {
        let off = AccountCfg { disabled: true, ..account("claude") };
        assert!(read_all(std::slice::from_ref(&off)).is_empty());
        assert!(TrainingPoll::default().due(&[off], Instant::now()).is_empty());
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
    fn a_read_that_cant_tell_keeps_the_last_that_could_and_says_why() {
        let mut poll = TrainingPoll::default();
        let failed = Training { note: "Sign-in expired.".into(), as_of: None, ..reading("claude", State::Unknown) };
        poll.take(reading("claude", State::Off));
        assert!(poll.take(failed.clone()));
        let kept = &poll.list(&[account("claude")])[0];
        assert_eq!((kept.state, kept.as_of, kept.stale.as_deref()), (State::Off, Some(1), Some("Sign-in expired.")));
        assert!(!poll.take(failed), "the same failure again changes nothing");
        assert!(poll.take(reading("claude", State::On)));
        assert_eq!(poll.list(&[account("claude")])[0].stale, None, "a read that could tell is fresh");
        assert!(poll.take(reading("codex", State::Unknown)), "nothing to keep: it says it can't tell");
    }

    #[test]
    fn another_folder_or_harness_under_the_same_name_is_read_afresh() {
        let mut poll = TrainingPoll::default();
        let accounts = vec![account("claude"), account("codex")];
        let t0 = Instant::now();
        poll.due(&accounts, t0);
        poll.on_read(Some(vec![reading("claude", State::Off), reading("codex", State::Off)]));
        let moved = vec![AccountCfg { home: "~/.elsewhere".into(), ..account("claude") }, account("codex")];
        poll.keep_only(&moved);
        assert_eq!(poll.list(&moved).iter().map(|t| t.account.as_str()).collect::<Vec<_>>(), ["codex"], "its old answer goes");
        let asks: Vec<String> = poll.due(&moved, t0 + Duration::from_secs(60)).into_iter().map(|a| a.name).collect();
        assert_eq!(asks, ["claude"]);
        // A read out when it changed answers for the old folder, and is passed over.
        poll.keep_only(&[AccountCfg { harness: "codex".into(), ..moved[0].clone() }, account("codex")]);
        poll.on_read(Some(vec![reading("claude", State::Off)]));
        assert!(poll.list(&moved).iter().all(|t| t.account != "claude"));
    }

    #[test]
    fn a_read_out_during_a_turn_off_doesnt_undo_it() {
        let mut poll = TrainingPoll::default();
        let accounts = vec![account("claude"), account("codex")];
        poll.due(&accounts, Instant::now());
        assert!(poll.on_changed(&identity(&accounts[0]), reading("claude", State::Off)));
        poll.on_read(Some(vec![reading("claude", State::On), reading("codex", State::On)]));
        let states: Vec<State> = poll.list(&accounts).iter().map(|t| t.state).collect();
        assert_eq!(states, [State::Off, State::On], "the read began before the turn-off");
        poll.due(&accounts, Instant::now() + EVERY);
        poll.on_read(Some(vec![reading("claude", State::On)]));
        assert_eq!(poll.list(&accounts)[0].state, State::On, "a later read counts again");
    }

    #[test]
    fn a_turn_off_for_an_account_changed_meanwhile_is_dropped() {
        let mut poll = TrainingPoll::default();
        let accounts = vec![account("claude")];
        poll.due(&accounts, Instant::now());
        poll.on_read(Some(vec![reading("claude", State::On)]));
        let before = identity(&accounts[0]);
        let moved = vec![AccountCfg { home: "~/.elsewhere".into(), ..account("claude") }];
        poll.keep_only(&moved);
        poll.due(&moved, Instant::now());
        assert!(!poll.on_changed(&before, reading("claude", State::Off)), "it was for the folder before");
        poll.on_read(Some(vec![reading("claude", State::On)]));
        assert_eq!(poll.list(&moved)[0].state, State::On, "the new folder's read isn't passed over");
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
