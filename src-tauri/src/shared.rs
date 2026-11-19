//! The automations your machines share, so that one of them runs each rather than every one: the
//! review of a PR that asks for yours (autoreview.rs `reviewer_of`), and a schedule that runs on one
//! of your machines (schedules/machines.rs `line`). Each machine works out the same one by itself,
//! from what every machine says of itself in its feed (peerfeed.rs): where it stands (`[devices]
//! automations`, a first choice or a backup), what it would run, and where what it ran stands.
//! Nothing is voted on or locked: the machine that's around, would run the thing and comes first
//! in line runs it, and the others show it as that machine's.
//!
//! A machine is around while it's connected, and for `AWAY_AFTER_MS` after it was last heard from,
//! so a connection that drops for a moment (a network that changed, a device waking) doesn't hand
//! its work to another. One asleep, closed or off for longer is passed over, and its work goes to
//! the next in line. Machines on builds that share nothing (before 2026-10-22) don't count: they
//! run their own, as before, and phones run nothing.

use codealong_peer::feed::Rank;
use codealong_peer::pr::PrCard;
use codealong_peer::schedules::Shared;

/// How long a machine that was connected counts as around after it was last heard from.
pub const AWAY_AFTER_MS: i64 = 5 * 60_000;

/// Your machines as shared automations go by them: this one, and the others that share.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Machines {
    /// This machine, while it shares (`[devices]` is on).
    pub me: Option<Me>,
    pub others: Vec<Machine>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Me {
    /// Its device key, as hex.
    pub key: String,
    pub rank: Rank,
}

/// Another machine of yours that shares, as its feed last said.
#[derive(Clone, Debug, PartialEq)]
pub struct Machine {
    /// Its device key, as hex.
    pub key: String,
    pub name: String,
    pub rank: Rank,
    pub online: bool,
    pub last_seen: Option<i64>,
    /// When its last feed came, which says what it ran since: a connection closing is no word
    /// from it.
    pub fed_at: Option<i64>,
    /// Its PR list, each with whether it would review the PR and where its own review stands.
    pub prs: Vec<PrCard>,
    /// The schedules it shares, each with whether it can run it and what it has run.
    pub schedules: Shared,
}

impl Machine {
    /// Connected, or heard from a moment ago.
    pub fn around(&self, now: i64) -> bool {
        self.online || self.last_seen.is_some_and(|t| now - t < AWAY_AFTER_MS)
    }

    /// Its card for PR `key` (`owner/name#number`), when it lists it.
    pub fn pr(&self, key: &str) -> Option<&PrCard> {
        self.prs.iter().find(|c| c.key.eq_ignore_ascii_case(key))
    }

    /// Its short id, which keys from there carry here (`feed::remote_key`).
    pub fn short(&self) -> &str {
        &self.key[..self.key.len().min(10)]
    }
}

impl Machines {
    /// Whether `other` says what this does but when each connected machine was last heard from
    /// and fed. Those move with every feed (one every second or two from a linked Mac) and are
    /// read only against minutes (a schedule's settling) while it's connected, so the engine keeps
    /// them without composing a board for them, which would come out the same (engine.rs
    /// `Msg::Machines`). An offline machine's still count: a reconnect and drop batched into one
    /// view moves only its `last_seen`, and that can make it around again (`around`).
    pub fn same_but_times(&self, other: &Machines) -> bool {
        let timeless = |m: &Machines| Machines { others: m.others.iter().map(|o| if o.online { Machine { last_seen: None, fed_at: None, ..o.clone() } } else { o.clone() }).collect(), ..m.clone() };
        timeless(self) == timeless(other)
    }
}

/// Where a machine goes in line for shared work: first choices before backups, then by device
/// key, the same on every machine.
pub fn place(rank: Rank, key: &str) -> (Rank, &str) {
    (rank, key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_machine_stays_around_for_a_while_after_it_was_last_heard_from() {
        let m = |online: bool, last_seen: Option<i64>| Machine { key: "aa".into(), name: "Mini".into(), rank: Rank::First, online, last_seen, fed_at: last_seen, prs: Vec::new(), schedules: Shared::default() };
        assert!(m(true, None).around(0));
        assert!(m(false, Some(1_000)).around(1_000 + AWAY_AFTER_MS - 1));
        assert!(!m(false, Some(1_000)).around(1_000 + AWAY_AFTER_MS));
        assert!(!m(false, None).around(0), "never heard from");
        // First choices go before backups whatever their keys, then keys settle it.
        assert!(place(Rank::First, "ff") < place(Rank::Backup, "00"));
        assert!(place(Rank::Backup, "0a") < place(Rank::Backup, "0b"));
    }

    #[test]
    fn a_feed_that_moves_only_the_times_says_the_same() {
        let m = |online: bool, at: i64| Machines { me: None, others: vec![Machine { key: "aa".into(), name: "Mini".into(), rank: Rank::First, online, last_seen: Some(at), fed_at: Some(at), prs: Vec::new(), schedules: Shared::default() }] };
        assert!(m(true, 1_000).same_but_times(&m(true, 2_000)));
        assert!(!m(true, 1_000).same_but_times(&m(false, 1_000)), "gone offline");
        assert!(!m(true, 1_000).same_but_times(&Machines::default()), "one fewer");
        // A reconnect and drop in one view: still offline, but heard from again, so around again.
        assert!(!m(false, 1_000).same_but_times(&m(false, 1_000 + AWAY_AFTER_MS)), "an offline machine heard from");
    }
}
