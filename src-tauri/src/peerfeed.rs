//! What a device tells the devices it's linked with about itself (peers.rs): its name, and its board's
//! worktrees, sessions and plan usage, cut from each board the engine publishes as barfeed.rs
//! cuts the bar's. Only what changed goes out: each worktree is hashed, and a feed carries the
//! ones that changed and the ids of the ones that left. What only works on this device stays here:
//! open targets, process ids, and resume and attach commands.
//!
//! The receiving device keeps a `Replica` of each linked device's feed, and hands its windows the
//! worktrees with `@<short>:` in front of every lane id and session key (`for_windows`), so a
//! click on one goes back to the device it's on and no id from there is taken for one here.
//!
//! The feed's messages and the replica are in codealong-peer (peer/src/feed.rs), which apps that
//! only watch share; this file has the sending side.

use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

use serde::Serialize;

pub use codealong_peer::feed::{remote_key, split_remote, DeviceInfo, FeedMsg, Replica};

use crate::model::{Board, DeviceTag, Lane, Usage};

/// What a device shares, cut from one board, with each part's hash worked out once for every
/// connection it goes to.
#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub generated_at: i64,
    pub device: DeviceInfo,
    pub lanes: Vec<Lane>,
    pub usage: Vec<Usage>,
    lane_hashes: Vec<u64>,
    device_hash: u64,
    usage_hash: u64,
}

pub fn snapshot(board: &Board, device: DeviceInfo) -> Snapshot {
    let lanes: Vec<Lane> = board.lanes.iter().map(shareable).collect();
    Snapshot {
        generated_at: board.generated_at,
        lane_hashes: lanes.iter().map(hash_of).collect(),
        device_hash: hash_of(&device),
        usage_hash: hash_of(&board.usage),
        device,
        lanes,
        usage: board.usage.clone(),
    }
}

/// A lane as another device may see it: nothing that only works here.
pub fn shareable(lane: &Lane) -> Lane {
    let mut l = lane.clone();
    l.targets.clear();
    l.default_target = None;
    for s in &mut l.sessions {
        s.pid = None;
        s.resume_command = None;
        s.attach_command = None;
        s.open_target = String::new();
        s.app_account = false;
    }
    l
}

fn hash_of<T: Serialize>(v: &T) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    serde_json::to_vec(v).unwrap_or_default().hash(&mut h);
    h.finish()
}

/// Sends a device's snapshots to one peer, a change at a time. There's one per connection, so a
/// new connection starts with everything.
#[derive(Default)]
pub struct Outbox {
    rev: u64,
    lanes: HashMap<String, u64>,
    device: Option<u64>,
    usage: Option<u64>,
}

impl Outbox {
    /// Picks up where the other device's copy leaves off (`Msg::Have`): the lanes it has, by the
    /// hashes this device sent it, so the next feed carries only the lanes that changed since and
    /// the ids of those that left, not everything. Only before anything went out on this
    /// connection: after that, the other device's copy is this outbox's own.
    pub fn resume(&mut self, have: HashMap<String, u64>) {
        if self.rev == 0 {
            self.lanes = have;
            self.rev = 1;
        }
    }

    /// What to send for `snap`, or None when nothing changed since the last feed.
    pub fn next(&mut self, snap: &Snapshot) -> Option<FeedMsg> {
        let full = self.rev == 0;
        let mut msg = FeedMsg { full, generated_at: snap.generated_at, ..Default::default() };
        if full || self.device != Some(snap.device_hash) {
            msg.device = Some(snap.device.clone());
            self.device = Some(snap.device_hash);
        }
        if full || self.usage != Some(snap.usage_hash) {
            msg.usage = Some(snap.usage.clone());
            self.usage = Some(snap.usage_hash);
        }
        let mut here: HashSet<&str> = HashSet::new();
        for (lane, h) in snap.lanes.iter().zip(&snap.lane_hashes) {
            here.insert(&lane.id);
            if self.lanes.get(&lane.id) != Some(h) {
                msg.lanes.push(lane.clone());
                msg.hashes.insert(lane.id.clone(), *h);
                self.lanes.insert(lane.id.clone(), *h);
            }
        }
        let gone: Vec<String> = self.lanes.keys().filter(|id| !here.contains(id.as_str())).cloned().collect();
        for id in &gone {
            self.lanes.remove(id);
        }
        if !full {
            msg.gone = gone;
            if msg.device.is_none() && msg.usage.is_none() && msg.lanes.is_empty() && msg.gone.is_empty() {
                return None;
            }
        }
        self.rev += 1;
        msg.rev = self.rev;
        Some(msg)
    }
}

/// A linked device's worktrees as this device's windows get them: every lane id and session key with
/// the device's short id in front, and each lane tagged with the device.
pub fn for_windows(short: &str, tag: &DeviceTag, replica: &Replica) -> Vec<Lane> {
    replica
        .lanes
        .iter()
        .map(|l| {
            let mut l = l.clone();
            l.id = remote_key(short, &l.id);
            l.device = Some(tag.clone());
            for s in &mut l.sessions {
                s.key = remote_key(short, &s.key);
            }
            l
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Session, Status};

    fn session(key: &str, status: Status) -> Session {
        serde_json::from_value(serde_json::json!({
            "key": key, "id": key, "shortId": key, "harness": "claude", "account": "claude", "surface": "cli",
            "status": status, "live": true, "pid": 42, "title": format!("Title {key}"), "cwd": "/r", "work": [],
            "name": null, "startedAt": null, "statusSince": 1, "lastActivity": 1, "lastPromptAt": 1, "excerpt": null,
            "resumeCommand": "claude --resume x", "appAccount": true, "openTarget": format!("s|{key}|focus"), "visitor": null,
            "headline": "", "detail": null, "branch": null, "archived": false, "attachCommand": "claude attach j", "stalledSince": null
        }))
        .expect("a session")
    }

    fn lane(id: &str, sessions: Vec<Session>) -> Lane {
        let mut l: Lane = serde_json::from_value(serde_json::json!({
            "id": id, "project": "P", "repo": "r", "repoSlug": null, "path": id, "callsign": "c",
            "origin": "main", "branch": null, "detached": null, "bay": "idle", "glyph": "idle", "parked": false,
            "hidden": false, "headline": "", "detail": null, "sessions": [], "pr": null, "git": null,
            "lastActivity": 0, "targets": [{"id": "l|terminal", "icon": "t", "label": "Terminal", "detail": null, "kbd": null, "disabled": false, "group": "lane"}],
            "defaultTarget": "l|terminal"
        }))
        .expect("a lane");
        l.sessions = sessions;
        l
    }

    fn board(lanes: Vec<Lane>) -> Board {
        Board { lanes, ..Board::default() }
    }

    fn info(name: &str) -> DeviceInfo {
        DeviceInfo { name: name.into(), app: "0.1.0".into(), remote_control: false, loading: false }
    }

    #[test]
    fn what_only_works_here_stays_here() {
        let l = shareable(&lane("/a", vec![session("s1", Status::Working)]));
        assert!(l.targets.is_empty() && l.default_target.is_none());
        let s = &l.sessions[0];
        assert!(s.pid.is_none() && s.resume_command.is_none() && s.attach_command.is_none() && s.open_target.is_empty() && !s.app_account);
        assert_eq!(s.title, "Title s1");
    }

    #[test]
    fn sends_everything_first_then_only_what_changed() {
        let mut out = Outbox::default();
        let mut replica = Replica::default();
        let b = board(vec![lane("/a", vec![session("s1", Status::Working)]), lane("/b", vec![])]);

        let first = out.next(&snapshot(&b, info("Mini"))).unwrap();
        assert!(first.full && first.lanes.len() == 2 && first.device.is_some() && first.usage.is_some());
        replica.apply(first);
        assert!(out.next(&snapshot(&b, info("Mini"))).is_none(), "nothing changed");

        // One session's status changes, /b leaves, /c arrives.
        let b2 = board(vec![lane("/a", vec![session("s1", Status::Unread)]), lane("/c", vec![])]);
        let delta = out.next(&snapshot(&b2, info("Mini"))).unwrap();
        assert!(!delta.full && delta.device.is_none() && delta.usage.is_none());
        assert_eq!(delta.lanes.iter().map(|l| l.id.as_str()).collect::<Vec<_>>(), ["/a", "/c"]);
        assert_eq!(delta.gone, ["/b"]);
        replica.apply(delta);
        let json = |l: &Vec<Lane>| serde_json::to_value(l).unwrap();
        assert_eq!(json(&replica.lanes), json(&snapshot(&b2, info("Mini")).lanes));

        // A new name goes out without the lanes.
        let rename = out.next(&snapshot(&b2, info("Studio"))).unwrap();
        assert!(rename.lanes.is_empty() && rename.device.as_ref().is_some_and(|d| d.name == "Studio"));
        replica.apply(rename);
        assert_eq!(replica.device.name, "Studio");
        assert_eq!(replica.rev, 3);
    }

    #[test]
    fn a_reconnect_picks_up_where_the_last_feed_left_off() {
        let b = board(vec![lane("/a", vec![session("s1", Status::Working)]), lane("/b", vec![]), lane("/c", vec![])]);
        let mut first = Outbox::default();
        let mut replica = Replica::default();
        replica.apply(first.next(&snapshot(&b, info("Mini"))).unwrap());

        // While apart: /a changes and /b leaves.
        let b2 = board(vec![lane("/a", vec![session("s1", Status::Unread)]), lane("/c", vec![])]);
        let mut again = Outbox::default();
        again.resume(replica.have());
        let msg = again.next(&snapshot(&b2, info("Mini"))).unwrap();
        assert!(!msg.full, "only what changed");
        assert_eq!(msg.lanes.iter().map(|l| l.id.as_str()).collect::<Vec<_>>(), ["/a"]);
        assert_eq!(msg.gone, ["/b"]);
        replica.apply(msg);
        let json = |l: &Vec<Lane>| serde_json::to_value(l).unwrap();
        assert_eq!(json(&replica.lanes), json(&snapshot(&b2, info("Mini")).lanes));

        // A resume after the first feed changes nothing: the outbox already knows what went out.
        again.resume(std::collections::HashMap::new());
        assert!(again.next(&snapshot(&b2, info("Mini"))).is_none());
    }

    #[test]
    fn windows_get_keys_that_name_their_mac() {
        let mut replica = Replica::default();
        replica.apply(FeedMsg { rev: 1, full: true, lanes: vec![lane("/Users/sam/app", vec![session("claude:claude:9f", Status::Needs)])], ..Default::default() });
        let tag = DeviceTag { name: "Mac mini".into(), short: "a1b2c3d4e5".into(), color: "teal".into(), online: true, last_seen: None };
        let lanes = for_windows("a1b2c3d4e5", &tag, &replica);
        assert_eq!(lanes[0].id, "@a1b2c3d4e5:/Users/sam/app");
        assert_eq!(lanes[0].sessions[0].key, "@a1b2c3d4e5:claude:claude:9f");
        assert_eq!(lanes[0].path, "/Users/sam/app");
        assert_eq!(lanes[0].device.as_ref(), Some(&tag));
        assert_eq!(split_remote(&lanes[0].sessions[0].key), Some(("a1b2c3d4e5", "claude:claude:9f")));
        assert_eq!(split_remote("claude:claude:9f"), None);
        // The windows split row keys on `::` and target ids on `|`; neither may appear.
        assert!(!lanes[0].id.contains("::") && !lanes[0].sessions[0].key.contains('|'));
    }
}
