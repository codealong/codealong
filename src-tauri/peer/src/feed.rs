//! What a device tells the devices it's linked with about itself: its name, and its board's
//! worktrees, sessions and plan usage, and its PR list for a device that knows PRs. A connection
//! starts with everything, then carries only the worktrees that changed and the ids of the ones
//! that left, and the PR list again only when it changed (the engine's peerfeed.rs cuts them from
//! each board it publishes).
//!
//! The receiving device keeps a `Replica` of each linked device's feed. Keys from another device
//! go around with that device's short id in front (`remote_key`), so an id from there is never
//! taken for one here.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::model::{Lane, Usage};
use crate::pr::PrCard;

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
}

/// How this build reads a feed's lanes. A lane or a session that gains a field another device shows
/// bumps it: a build from before reads such a feed without the field and keeps the sender's hashes
/// all the same, so once it's updated, a resumed feed (`Replica::have`) would leave out every lane
/// that hasn't changed since, the field missing from them until they do. A copy kept with another
/// stamp is sent again whole instead. 1: `Session::pin`; 2: `Session::mode`; 3: `Session::effort`;
/// 4: `Session::queued_at`; 5: `Session::modes` and `SessionMode::id`; 6: `SessionLimit::continues`;
/// 7: `Session::queued`; 8: `Session::flagged`.
pub const READS: u32 = 8;

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
        if !msg.gone.is_empty() {
            let gone: HashSet<&String> = msg.gone.iter().collect();
            self.lanes.retain(|l| !gone.contains(&l.id));
        }
        let mut at: HashMap<String, usize> = self.lanes.iter().enumerate().map(|(i, l)| (l.id.clone(), i)).collect();
        for l in msg.lanes {
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
            labels: Vec::new(),
            auto_review: None,
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
    fn remote_keys_split_back() {
        let k = remote_key("a1b2c3d4e5", "claude:claude:9f");
        assert_eq!(k, "@a1b2c3d4e5:claude:claude:9f");
        assert_eq!(split_remote(&k), Some(("a1b2c3d4e5", "claude:claude:9f")));
        assert_eq!(split_remote("claude:claude:9f"), None);
    }
}
