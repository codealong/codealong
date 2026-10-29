//! What a device tells the devices it's linked with about itself: its name, and its board's
//! worktrees, sessions and plan usage, its PR list for a device that knows PRs, and the schedules
//! it shares with your other machines for one that keeps them. A connection starts with
//! everything, then carries only the worktrees that changed and the ids of the ones that left, and
//! the PR list and the schedules again only when they changed (the engine's peerfeed.rs cuts them
//! from each board it publishes).
//!
//! The receiving device keeps a `Replica` of each linked device's feed. Keys from another device
//! go around with that device's short id in front (`remote_key`), so an id from there is never
//! taken for one here.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::model::{Lane, Session, Usage};
use crate::pr::PrCard;
use crate::schedules::Shared;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfo {
    pub name: String,
    pub app: String,
    /// Its linked devices may send prompts, answer approvals and start sessions there.
    #[serde(default)]
    pub remote_control: bool,
    /// Its first scan hasn't finished.
    #[serde(default)]
    pub loading: bool,
    /// Where it stands among your machines for the automations they share, so that one of them
    /// runs each (a review of a PR that asks for yours): none from a phone, which runs nothing,
    /// and from machines before 2026-10-22, which share none and run their own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shares: Option<Rank>,
}

/// Where a machine stands for the automations your machines share: a backup runs one only when
/// no first choice that can is around. Machines that stand the same go by their device keys, so
/// each of them works out the same one.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Default)]
#[serde(rename_all = "lowercase")]
pub enum Rank {
    #[default]
    First,
    Backup,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct FeedMsg {
    pub rev: u64,
    /// Everything the sender has: the receiver drops what it had from it before.
    #[serde(default)]
    pub full: bool,
    #[serde(default)]
    pub generated_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device: Option<DeviceInfo>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lanes: Vec<Lane>,
    /// Every session's key, in order, for each lane in `lanes` that comes with only the sessions
    /// that changed in it: the rest are the receiver's own copies. Only to a device that says
    /// `proto::SESSIONS`.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub keys: HashMap<String, Vec<String>>,
    /// Lanes that left, by id.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gone: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<Vec<Usage>>,
    /// Each lane's hash as the sender worked it out, by id, for the lanes this message carries: the
    /// receiver keeps them and hands them back when it reconnects (`Msg::Have`).
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub hashes: HashMap<String, u64>,
    /// The sender's PR list, whole, when it changed: only to a device that says it knows PRs
    /// (`proto::PRS`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prs: Option<Vec<PrCard>>,
    /// The schedules the sender shares with your other machines, whole, when they changed: only to
    /// a device that says it keeps them (`proto::SCHEDULES`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schedules: Option<Shared>,
}

/// How this build reads a feed's lanes. A lane or a session that gains a field another device shows
/// bumps it: a build from before reads such a feed without the field and keeps the sender's hashes
/// all the same, so once it's updated, a resumed feed (`Replica::have`) would leave out every lane
/// that hasn't changed since, the field missing from them until they do. A copy kept with another
/// stamp is sent again whole instead. 1: `Session::pin`; 2: `Session::mode`; 3: `Session::effort`;
/// 4: `Session::queued_at`; 5: `Session::modes` and `SessionMode::id`; 6: `SessionLimit::continues`;
/// 7: `Session::queued`; 8: `Session::flagged`; 9: `Pr::stale_approval`; 10: `Helper::yours`;
/// 11: `SessionMode::expected`; 12: `Session::emblem` as an emoji or a picture.
pub const READS: u32 = 12;

/// What this device knows of a linked device's board, kept up to date by its feeds, and kept on disk
/// so a device that's asleep still shows as it was when last seen.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Replica {
    pub rev: u64,
    pub generated_at: i64,
    pub device: DeviceInfo,
    pub lanes: Vec<Lane>,
    pub usage: Vec<Usage>,
    /// The sender's hash of each lane, as its feeds said, for resuming (`have`).
    #[serde(default)]
    pub hashes: HashMap<String, u64>,
    /// Its PR list, as its feeds last said; none from a device that sends none.
    #[serde(default)]
    pub prs: Vec<PrCard>,
    /// The schedules it shares, as its feeds last said.
    #[serde(default)]
    pub schedules: Shared,
    /// How the build that read its last whole feed reads them (`READS`); 0 from builds before the
    /// stamp.
    #[serde(default)]
    pub reads: u32,
}

impl Replica {
    /// The lanes to tell the sender this device still has when it reconnects (`Msg::Have`): only
    /// ones whose hash it sent, so a lane from a sender that sends none is sent again. None from a
    /// copy another build read (`READS`), so it's sent again whole.
    pub fn have(&self) -> HashMap<String, u64> {
        if self.reads != READS {
            return HashMap::new();
        }
        self.lanes.iter().filter_map(|l| self.hashes.get(&l.id).map(|h| (l.id.clone(), *h))).collect()
    }

    /// The sender connected again, saying what it knows: a sender that doesn't know PRs sends no PR
    /// list, and a resumed feed (`have`) starts with none, so the one kept from before goes.
    pub fn heard_from(&mut self, knows: &[String]) {
        if !knows.iter().any(|k| k == crate::proto::PRS) {
            self.prs.clear();
        }
        if !knows.iter().any(|k| k == crate::proto::SCHEDULES) {
            self.schedules = Shared::default();
        }
    }

    pub fn apply(&mut self, msg: FeedMsg) {
        if msg.full {
            *self = Replica { reads: READS, ..Replica::default() };
        }
        for id in &msg.gone {
            self.hashes.remove(id);
        }
        for l in &msg.lanes {
            match msg.hashes.get(&l.id) {
                Some(h) => self.hashes.insert(l.id.clone(), *h),
                None => self.hashes.remove(&l.id),
            };
        }
        self.rev = msg.rev;
        self.generated_at = msg.generated_at;
        if let Some(d) = msg.device {
            self.device = d;
        }
        if let Some(u) = msg.usage {
            self.usage = u;
        }
        if let Some(prs) = msg.prs {
            self.prs = prs;
        }
        if let Some(schedules) = msg.schedules {
            self.schedules = schedules;
        }
        if !msg.gone.is_empty() {
            let gone: HashSet<&String> = msg.gone.iter().collect();
            self.lanes.retain(|l| !gone.contains(&l.id));
        }
        let mut at: HashMap<String, usize> = self.lanes.iter().enumerate().map(|(i, l)| (l.id.clone(), i)).collect();
        for mut l in msg.lanes {
            if let Some(keys) = msg.keys.get(&l.id) {
                let had = at.get(&l.id).map(|&i| std::mem::take(&mut self.lanes[i].sessions)).unwrap_or_default();
                let mut had: HashMap<String, Session> = had.into_iter().map(|s| (s.key.clone(), s)).collect();
                let mut changed: HashMap<String, Session> = std::mem::take(&mut l.sessions).into_iter().map(|s| (s.key.clone(), s)).collect();
                l.sessions = keys.iter().filter_map(|k| changed.remove(k).or_else(|| had.remove(k))).collect();
                // A session this copy doesn't have: it isn't what the sender takes it for, so the
                // lane comes whole when the feed resumes.
                if l.sessions.len() != keys.len() {
                    self.hashes.remove(&l.id);
                }
            }
            match at.get(&l.id) {
                Some(&i) => self.lanes[i] = l,
                None => {
                    at.insert(l.id.clone(), self.lanes.len());
                    self.lanes.push(l);
                }
            }
        }
    }
}

/// A key from another device, as this device's windows get it.
pub fn remote_key(short: &str, key: &str) -> String {
    format!("@{short}:{key}")
}

/// The device's short id and its own key, for a key that came from another device.
pub fn split_remote(key: &str) -> Option<(&str, &str)> {
    key.strip_prefix('@')?.split_once(':')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lane(id: &str) -> Lane {
        crate::fixtures::lane(id, Vec::new())
    }

    #[test]
    fn a_full_feed_replaces_what_was_there() {
        let mut replica = Replica::default();
        replica.apply(FeedMsg { rev: 5, full: true, lanes: vec![lane("/old")], ..Default::default() });
        replica.apply(FeedMsg { rev: 1, full: true, lanes: vec![lane("/new")], ..Default::default() });
        assert_eq!(replica.lanes.iter().map(|l| l.id.as_str()).collect::<Vec<_>>(), ["/new"]);
    }

    #[test]
    fn a_copy_an_older_build_kept_is_sent_again_whole() {
        // An older build read the feed without fields it didn't know, keeping the hashes; its
        // copy, saved without the stamp, asks for nothing to be left out.
        let older: Replica = serde_json::from_value(serde_json::json!({ "rev": 3, "generatedAt": 1, "device": { "name": "Mac", "app": "0.1.0", "remoteControl": false }, "lanes": [lane("/a")], "usage": [], "hashes": { "/a": 7 } })).unwrap();
        assert!(older.have().is_empty());
        // What this build reads whole it resumes from.
        let mut replica = older;
        replica.apply(FeedMsg { rev: 1, full: true, lanes: vec![lane("/a")], hashes: HashMap::from([("/a".to_string(), 8)]), ..Default::default() });
        assert_eq!(replica.reads, READS);
        assert_eq!(replica.have(), HashMap::from([("/a".to_string(), 8)]));
    }

    #[test]
    fn deltas_replace_and_drop_lanes() {
        let mut replica = Replica::default();
        replica.apply(FeedMsg { rev: 1, full: true, lanes: vec![lane("/a"), lane("/b")], ..Default::default() });
        let mut changed = lane("/a");
        changed.headline = "new".into();
        replica.apply(FeedMsg { rev: 2, lanes: vec![changed, lane("/c")], gone: vec!["/b".into()], ..Default::default() });
        assert_eq!(replica.lanes.iter().map(|l| (l.id.as_str(), l.headline.as_str())).collect::<Vec<_>>(), [("/a", "new"), ("/c", "")]);
        assert_eq!(replica.rev, 2);
    }

    #[test]
    fn a_lane_with_only_its_changed_sessions_keeps_the_rest() {
        let session = |key: &str, title: &str| Session { title: title.into(), ..crate::fixtures::session(key) };
        let mut replica = Replica::default();
        let first = crate::fixtures::lane("/a", vec![session("k1", "one"), session("k2", "two"), session("k3", "three")]);
        replica.apply(FeedMsg { rev: 1, full: true, lanes: vec![first], hashes: HashMap::from([("/a".to_string(), 1)]), ..Default::default() });
        // k2 changed and moved first, k3 left, k4 came; k1 is this copy's own.
        let changed = crate::fixtures::lane("/a", vec![session("k2", "two again"), session("k4", "four")]);
        let keys = HashMap::from([("/a".to_string(), vec!["k2".to_string(), "k1".into(), "k4".into()])]);
        replica.apply(FeedMsg { rev: 2, lanes: vec![changed], keys, hashes: HashMap::from([("/a".to_string(), 2)]), ..Default::default() });
        let titles: Vec<(&str, &str)> = replica.lanes[0].sessions.iter().map(|s| (s.key.as_str(), s.title.as_str())).collect();
        assert_eq!(titles, [("k2", "two again"), ("k1", "one"), ("k4", "four")]);
        assert_eq!(replica.have(), HashMap::from([("/a".to_string(), 2)]));

        // One it doesn't have: the lane is kept as far as it goes, and comes whole on resuming.
        let keys = HashMap::from([("/a".to_string(), vec!["k1".to_string(), "k9".into()])]);
        replica.apply(FeedMsg { rev: 3, lanes: vec![crate::fixtures::lane("/a", vec![])], keys, hashes: HashMap::from([("/a".to_string(), 3)]), ..Default::default() });
        assert_eq!(replica.lanes[0].sessions.iter().map(|s| s.key.as_str()).collect::<Vec<_>>(), ["k1"]);
        assert!(replica.have().is_empty());
    }

    #[test]
    fn replicas_keep_the_hashes_they_were_sent() {
        let mut replica = Replica::default();
        replica.apply(FeedMsg { rev: 1, full: true, lanes: vec![lane("/a"), lane("/b")], hashes: HashMap::from([("/a".to_string(), 1), ("/b".to_string(), 2)]), ..Default::default() });
        assert_eq!(replica.have(), HashMap::from([("/a".to_string(), 1), ("/b".to_string(), 2)]));
        // A changed lane brings its new hash; a lane that left takes its hash with it; a lane sent
        // without one isn't claimed.
        replica.apply(FeedMsg { rev: 2, lanes: vec![lane("/a"), lane("/c")], hashes: HashMap::from([("/a".to_string(), 3)]), gone: vec!["/b".into()], ..Default::default() });
        assert_eq!(replica.have(), HashMap::from([("/a".to_string(), 3)]));
        // It survives the trip to disk.
        let back: Replica = serde_json::from_value(serde_json::to_value(&replica).unwrap()).unwrap();
        assert_eq!(back.have(), replica.have());
    }

    #[test]
    fn the_pr_list_is_replaced_whole_and_kept_when_a_feed_has_none() {
        let card = |n: u64| crate::pr::PrCard {
            key: format!("o/r#{n}"),
            repo: "o/r".into(),
            project: "P".into(),
            number: n,
            title: format!("PR {n}"),
            url: String::new(),
            state: crate::model::PrState::Open,
            author: None,
            mine: false,
            waits: n == 1,
            review_requested: n == 1,
            requested_by: None,
            requested_at: None,
            snoozed_until: None,
            pinned: false,
            head: "b".into(),
            base: None,
            checks: Default::default(),
            merge: Default::default(),
            additions: 0,
            deletions: 0,
            changed_files: 0,
            created_at: 0,
            updated_at: 0,
            merged_at: None,
            closed_at: None,
            sessions: Vec::new(),
            session_lanes: Vec::new(),
            labels: Vec::new(),
            auto_review: None,
            shared_review: None,
            turn: None,
        };
        let mut replica = Replica::default();
        replica.apply(FeedMsg { rev: 1, full: true, prs: Some(vec![card(1), card(2)]), ..Default::default() });
        replica.apply(FeedMsg { rev: 2, lanes: vec![lane("/a")], ..Default::default() });
        assert_eq!(replica.prs.iter().map(|p| p.number).collect::<Vec<_>>(), [1, 2]);
        replica.apply(FeedMsg { rev: 3, prs: Some(vec![card(2)]), ..Default::default() });
        assert_eq!(replica.prs.iter().map(|p| p.number).collect::<Vec<_>>(), [2]);
        // The sender comes back as a build that doesn't know PRs: the list kept goes, though the
        // feed that resumes is no full one.
        replica.heard_from(&["have".to_string(), "watch".into(), "cursor".into()]);
        replica.apply(FeedMsg { rev: 4, lanes: vec![lane("/a")], ..Default::default() });
        assert!(replica.prs.is_empty());
        // One that knows them keeps it until its feed says otherwise.
        replica.apply(FeedMsg { rev: 5, prs: Some(vec![card(3)]), ..Default::default() });
        replica.heard_from(&crate::proto::KNOWS.map(String::from));
        assert_eq!(replica.prs.len(), 1);
        // A feed from a device that sends none leaves none behind.
        let old: FeedMsg = serde_json::from_str(r#"{"rev":6,"full":true}"#).unwrap();
        replica.apply(old);
        assert!(replica.prs.is_empty());
    }

    #[test]
    fn the_shared_schedules_are_replaced_whole_and_go_with_a_build_that_keeps_none() {
        let shared = |id: &str| Shared { list: vec![crate::schedules::SharedSchedule { id: id.into(), able: true, ..Default::default() }], gone: Vec::new() };
        let mut replica = Replica::default();
        replica.apply(FeedMsg { rev: 1, full: true, schedules: Some(shared("s-1")), ..Default::default() });
        replica.apply(FeedMsg { rev: 2, lanes: vec![lane("/a")], ..Default::default() });
        assert!(replica.schedules.get("s-1").is_some());
        replica.apply(FeedMsg { rev: 3, schedules: Some(shared("s-2")), ..Default::default() });
        assert!(replica.schedules.get("s-1").is_none() && replica.schedules.get("s-2").is_some());
        // Back as a build that keeps none: what it said before goes.
        replica.heard_from(&[crate::proto::PRS.to_string()]);
        assert_eq!(replica.schedules, Shared::default());
    }

    #[test]
    fn remote_keys_split_back() {
        let k = remote_key("a1b2c3d4e5", "claude:claude:9f");
        assert_eq!(k, "@a1b2c3d4e5:claude:claude:9f");
        assert_eq!(split_remote(&k), Some(("a1b2c3d4e5", "claude:claude:9f")));
        assert_eq!(split_remote("claude:claude:9f"), None);
    }
}
