//! What a device tells the devices it's linked with about itself (peers.rs): its name, and its board's
//! worktrees, sessions and plan usage, cut from each board the engine publishes as barfeed.rs
//! cuts the bar's, and its PR list for a device that knows PRs (`pr_cards`). Only what changed
//! goes out: each worktree is hashed, and a feed carries the ones that changed and the ids of the
//! ones that left; the PR list goes again, whole, when it changed. What only works on this device stays here:
//! open targets, process ids, preview URLs, and resume and attach commands.
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
use codealong_peer::pr::PrCard;

use crate::model::{Board, DeviceTag, Harness, Lane, Usage};

/// What a device shares, cut from one board, with each part's hash worked out once for every
/// connection it goes to.
#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub generated_at: i64,
    pub device: DeviceInfo,
    pub lanes: Vec<Lane>,
    pub usage: Vec<Usage>,
    pub prs: Vec<PrCard>,
    lane_hashes: Vec<u64>,
    device_hash: u64,
    usage_hash: u64,
    prs_hash: u64,
    /// The same without what a device from before Cursor can't read (`reads_everywhere`), when
    /// there's any.
    older: Option<Box<Snapshot>>,
}

pub fn snapshot(board: &Board, device: DeviceInfo) -> Snapshot {
    let mut lanes: Vec<Lane> = board.lanes.iter().map(shareable).collect();
    // Each session's place among the pins, which the board keeps apart from its sessions.
    for s in lanes.iter_mut().flat_map(|l| l.sessions.iter_mut()) {
        s.pin = board.pinned.iter().position(|k| *k == s.key).map(|i| i as u32);
    }
    let usage = board.usage.clone();
    let prs = pr_cards(board);
    let prs_hash = hash_of(&prs);
    let newer = |h: Harness| !reads_everywhere(h);
    let older = (lanes.iter().any(|l| l.sessions.iter().any(|s| newer(s.harness))) || usage.iter().any(|u| newer(u.harness))).then(|| {
        let lanes: Vec<Lane> = lanes.iter().map(|l| Lane { sessions: l.sessions.iter().filter(|s| reads_everywhere(s.harness)).cloned().collect(), ..l.clone() }).collect();
        let usage: Vec<Usage> = usage.iter().filter(|u| reads_everywhere(u.harness)).cloned().collect();
        Box::new(Snapshot { generated_at: board.generated_at, lane_hashes: lanes.iter().map(hash_of).collect(), device_hash: hash_of(&device), usage_hash: hash_of(&usage), device: device.clone(), lanes, usage, prs: prs.clone(), prs_hash, older: None })
    });
    Snapshot { generated_at: board.generated_at, lane_hashes: lanes.iter().map(hash_of).collect(), device_hash: hash_of(&device), usage_hash: hash_of(&usage), device, lanes, usage, prs, prs_hash, older }
}

/// The PRs another device is told of: the open ones, and ones merged or closed in the last day,
/// as the PRs view lists them, each cut down to a card. Whether one waits on your review is the
/// board's own rule (`PrItem::waits_on_review`).
pub fn pr_cards(board: &Board) -> Vec<PrCard> {
    let since = board.generated_at - 24 * 3600 * 1000;
    board
        .prs
        .iter()
        .filter(|p| p.state.is_open() || p.merged_at.or(p.closed_at).is_some_and(|t| t >= since))
        .map(|p| PrCard {
            key: p.key.clone(),
            repo: p.repo_slug.clone(),
            project: p.project.clone(),
            number: p.number,
            title: p.title.clone(),
            url: p.url.clone(),
            state: p.state,
            author: p.author.clone(),
            mine: p.mine,
            waits: p.waits_on_review(),
            review_requested: p.review_requested,
            requested_by: p.requested_by.clone(),
            requested_at: p.requested_at,
            snoozed_until: p.snoozed_until,
            pinned: board.pinned_prs.contains(&p.key),
            head: p.head.clone(),
            base: p.base.clone(),
            checks: p.checks.clone(),
            merge: p.merge,
            additions: p.additions,
            deletions: p.deletions,
            changed_files: p.changed_files,
            created_at: p.created_at,
            updated_at: p.updated_at,
            merged_at: p.merged_at,
            closed_at: p.closed_at,
            sessions: p.sessions.iter().map(|s| s.key.clone()).collect(),
        })
        .collect()
}

/// Whether every device's build reads the harness: the ones there were before a device's hello
/// said which it knows (`proto::KNOWS`). A Cursor session or account goes only to a device that
/// says `proto::CURSOR`: in a feed or an answer to one from before, it would lose that device the
/// whole message.
pub fn reads_everywhere(h: Harness) -> bool {
    !matches!(h, Harness::Cursor)
}

/// Whether a device whose hello said `knows` reads every harness.
pub fn knows_every_harness(knows: &[String]) -> bool {
    knows.iter().any(|k| k == codealong_peer::proto::CURSOR)
}

/// An answer for a device from before Cursor, without the Cursor sessions and accounts it can't
/// read (`reads_everywhere`): whatever a list holds with a harness it doesn't know is left out.
pub fn for_older(v: &mut serde_json::Value) {
    use serde_json::Value;
    let unknown = |v: &Value| v.get("harness").and_then(Value::as_str).is_some_and(|h| crate::config::HARNESSES.contains(&h) && !reads_everywhere(Harness::from_config(h)));
    match v {
        Value::Array(items) => {
            items.retain(|item| !unknown(item));
            items.iter_mut().for_each(for_older);
        }
        Value::Object(fields) => fields.values_mut().for_each(for_older),
        _ => {}
    }
}

/// A lane as another device may see it: nothing that only works here.
pub fn shareable(lane: &Lane) -> Lane {
    let mut l = lane.clone();
    l.targets.clear();
    l.default_target = None;
    // Its URL opens here: localhost, or a name a proxy on this Mac serves.
    l.preview = None;
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
    prs: Option<u64>,
    /// The other device is from before Cursor: it gets the snapshot's `older` side.
    older: bool,
    /// The other device knows PRs (`proto::PRS`): it gets the PR list.
    knows_prs: bool,
    /// The other device draws every emblem color (`proto::EMBLEMS`); one from before gets the
    /// label colors nearest them (`emblems::for_older`).
    knows_emblems: bool,
}

impl Outbox {
    /// The outbox for a device whose hello said `knows`.
    pub fn for_device(knows: &[String]) -> Outbox {
        let says = |word: &str| knows.iter().any(|k| k == word);
        Outbox { older: !knows_every_harness(knows), knows_prs: says(codealong_peer::proto::PRS), knows_emblems: says(codealong_peer::proto::EMBLEMS), ..Outbox::default() }
    }

    /// Starts over, so the next feed carries everything, for the same device.
    pub fn restart(&mut self) {
        *self = Outbox { older: self.older, knows_prs: self.knows_prs, knows_emblems: self.knows_emblems, ..Outbox::default() };
    }

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
        let snap = match &snap.older {
            Some(older) if self.older => older,
            _ => snap,
        };
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
        // A new connection, resumed or not, starts with the whole list: its other side's copy
        // could be any age.
        if self.knows_prs && self.prs != Some(snap.prs_hash) {
            msg.prs = Some(snap.prs.clone());
            self.prs = Some(snap.prs_hash);
        }
        let mut here: HashSet<&str> = HashSet::new();
        for (lane, h) in snap.lanes.iter().zip(&snap.lane_hashes) {
            here.insert(&lane.id);
            if self.lanes.get(&lane.id) != Some(h) {
                let mut sent = lane.clone();
                if !self.knows_emblems {
                    crate::emblems::for_older(&mut sent);
                }
                msg.lanes.push(sent);
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
            if msg.device.is_none() && msg.usage.is_none() && msg.prs.is_none() && msg.lanes.is_empty() && msg.gone.is_empty() {
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
                // It came from a session on that device too, and a helper helps one there.
                if let Some(o) = &mut s.origin {
                    o.key = remote_key(short, &o.key);
                }
                if let Some(p) = s.helper.as_mut().and_then(|h| h.parent.as_mut()) {
                    *p = remote_key(short, p);
                }
            }
            l
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Session, Status, Target};

    fn session(key: &str, status: Status) -> Session {
        Session {
            status,
            pid: Some(42),
            status_since: Some(1),
            last_activity: 1,
            last_prompt_at: 1,
            resume_command: Some("claude --resume x".into()),
            app_account: true,
            attach_command: Some("claude attach j".into()),
            ..crate::fixtures::session(key)
        }
    }

    fn lane(id: &str, sessions: Vec<Session>) -> Lane {
        let terminal = Target { id: "l|terminal".into(), icon: "t".into(), label: "Terminal".into(), detail: None, kbd: None, disabled: false, group: "lane".into() };
        Lane { callsign: "c".into(), targets: vec![terminal], default_target: Some("l|terminal".into()), ..crate::fixtures::lane(id, sessions) }
    }

    fn board(lanes: Vec<Lane>) -> Board {
        Board { lanes, ..Board::default() }
    }

    fn info(name: &str) -> DeviceInfo {
        DeviceInfo { name: name.into(), app: "0.1.0".into(), remote_control: false, loading: false }
    }

    #[test]
    fn a_pinned_session_goes_out_with_its_place_among_the_pins() {
        let b = Board { pinned: vec!["s3".into(), "s1".into()], ..board(vec![lane("/a", vec![session("s1", Status::Idle), session("s2", Status::Idle)]), lane("/b", vec![session("s3", Status::Idle)])]) };
        let snap = snapshot(&b, info("Mac"));
        let pins: Vec<(&str, Option<u32>)> = snap.lanes.iter().flat_map(|l| l.sessions.iter()).map(|s| (s.key.as_str(), s.pin)).collect();
        assert_eq!(pins, [("s1", Some(1)), ("s2", None), ("s3", Some(0))]);
        // The board's own sessions, which its windows show, carry none.
        assert!(b.lanes.iter().flat_map(|l| l.sessions.iter()).all(|s| s.pin.is_none()));
    }

    #[test]
    fn what_only_works_here_stays_here() {
        let l = shareable(&lane("/a", vec![session("s1", Status::Working)]));
        assert!(l.targets.is_empty() && l.default_target.is_none());
        let s = &l.sessions[0];
        assert!(s.pid.is_none() && s.resume_command.is_none() && s.attach_command.is_none() && s.open_target.is_empty() && !s.app_account);
        assert_eq!(s.title, "Title s1");
    }

    fn pr(n: u64, state: crate::model::PrState, requested: bool, merged_at: Option<i64>) -> crate::model::PrItem {
        serde_json::from_value(serde_json::json!({
            "key": format!("o/r#{n}"), "repoSlug": "o/r", "repo": "r", "project": "P", "number": n, "title": format!("PR {n}"),
            "url": "", "state": state, "author": "ana", "mine": false, "reviewRequested": requested, "head": "h", "base": "main",
            "checks": { "passed": 1, "failed": 0, "pending": 0, "skipped": 0, "failing": [] }, "merge": "clean",
            "additions": 3, "deletions": 1, "changedFiles": 2, "createdAt": 0, "updatedAt": 5,
            "mergedAt": merged_at, "closedAt": null, "lanes": ["/a"], "sessions": [{ "key": "claude:claude:s1", "lane": "/a" }]
        }))
        .expect("a PR")
    }

    #[test]
    fn the_pr_list_goes_only_to_a_device_that_knows_prs_and_again_only_when_it_changes() {
        use crate::model::PrState;
        let now = 100 * 24 * 3600 * 1000;
        let b = |prs: Vec<crate::model::PrItem>| Board { generated_at: now, prs, pinned_prs: vec!["o/r#1".into()], ..board(vec![lane("/a", vec![])]) };
        // Open ones, and one merged in the last day; not one merged a week ago.
        let one = b(vec![pr(1, PrState::Open, true, None), pr(2, PrState::Merged, false, Some(now - 3600 * 1000)), pr(3, PrState::Merged, false, Some(now - 7 * 24 * 3600 * 1000))]);
        let snap = snapshot(&one, info("Mini"));
        assert_eq!(snap.prs.iter().map(|p| p.number).collect::<Vec<_>>(), [1, 2]);
        let first = &snap.prs[0];
        assert!(first.waits && first.pinned && first.sessions == ["claude:claude:s1"] && first.repo == "o/r");

        let mut older = Outbox::for_device(&["have".into(), "watch".into(), "cursor".into()]);
        assert!(older.next(&snap).unwrap().prs.is_none(), "a device from before PRs gets none");

        let knows: Vec<String> = codealong_peer::proto::KNOWS.iter().map(|k| k.to_string()).collect();
        let mut out = Outbox::for_device(&knows);
        let mut replica = Replica::default();
        let full = out.next(&snap).unwrap();
        assert_eq!(full.prs.as_ref().map(Vec::len), Some(2));
        replica.apply(full);
        assert!(out.next(&snapshot(&one, info("Mini"))).is_none(), "nothing changed");
        // The review is answered: the list goes again, whole.
        let two = b(vec![pr(1, PrState::Approved, false, None), pr(2, PrState::Merged, false, Some(now - 3600 * 1000))]);
        let delta = out.next(&snapshot(&two, info("Mini"))).unwrap();
        assert!(delta.lanes.is_empty() && delta.prs.is_some());
        replica.apply(delta);
        assert!(!replica.prs[0].waits && replica.prs[0].state == PrState::Approved);
        // A reconnect starts with the list, whatever the other side kept.
        out.restart();
        assert!(out.next(&snapshot(&two, info("Mini"))).unwrap().prs.is_some());
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

    /// A build from before Cursor can't read a Cursor session or account, and one in a message
    /// would lose it the whole message: it gets the board without them, and only a device that
    /// says it knows them gets them.
    #[test]
    fn a_device_from_before_cursor_gets_no_cursor_sessions() {
        let cursor = Session { harness: Harness::Cursor, ..session("cursor:cursor:c1", Status::Working) };
        let usage = |account: &str, harness| Usage { account: account.into(), harness, plan: None, meters: Vec::new(), as_of: None, error: None, needs_sign_in: false, loading: false, no_limits: false, preferred: false, rate_limited: false };
        let mut b = board(vec![lane("/a", vec![session("s1", Status::Working), cursor.clone()]), lane("/b", vec![])]);
        b.usage = vec![usage("claude", Harness::Claude), usage("cursor", Harness::Cursor)];
        let snap = snapshot(&b, info("Mini"));

        let mut older = Outbox::for_device(&["have".into(), "watch".into()]);
        let first = older.next(&snap).unwrap();
        assert_eq!(first.lanes[0].sessions.iter().map(|s| s.key.as_str()).collect::<Vec<_>>(), ["s1"]);
        assert_eq!(first.usage.as_ref().unwrap().iter().map(|u| u.account.as_str()).collect::<Vec<_>>(), ["claude"]);
        // Only the Cursor session changing is nothing to it.
        let mut b2 = b.clone();
        b2.lanes[0].sessions[1].status = Status::Unread;
        assert!(older.next(&snapshot(&b2, info("Mini"))).is_none());
        // Starting over keeps it what it is.
        older.restart();
        assert_eq!(older.next(&snap).unwrap().lanes[0].sessions.len(), 1);

        let mut newer = Outbox::for_device(&codealong_peer::proto::KNOWS.map(String::from));
        let all = newer.next(&snap).unwrap();
        assert_eq!(all.lanes[0].sessions.len(), 2);
        assert_eq!(all.usage.unwrap().len(), 2);
        assert!(newer.next(&snapshot(&b2, info("Mini"))).is_some());

        // An answer to it leaves out what's listed with Cursor's harness, wherever it's listed.
        let mut answer = serde_json::json!({ "accounts": [{ "name": "claude", "harness": "claude" }, { "name": "cursor", "harness": "cursor" }], "lane": { "sessions": [serde_json::to_value(&cursor).unwrap()] }, "harness": "cursor" });
        for_older(&mut answer);
        assert_eq!(answer, serde_json::json!({ "accounts": [{ "name": "claude", "harness": "claude" }], "lane": { "sessions": [] }, "harness": "cursor" }));
    }

    /// A device from before emblems came in more than the label colors gets the label colors
    /// nearest them, which its windows draw; one that says it knows them (`proto::EMBLEMS`) gets
    /// them as they are.
    #[test]
    fn a_device_from_before_gets_emblems_in_label_colors() {
        let emblem = |color: &str, accent: &str| crate::model::Emblem { color: color.into(), accent: accent.into(), light: 2, icon: "bug".into() };
        let s = Session { emblem: Some(emblem("rose", "sky")), ..session("s1", Status::Working) };
        let snap = snapshot(&board(vec![lane("/a", vec![s, session("s2", Status::Idle)])]), info("Mini"));
        let mut older = Outbox::for_device(&["have".into(), "watch".into(), "cursor".into(), "prs".into(), "pr-drafts".into()]);
        let sent = older.next(&snap).unwrap();
        assert_eq!(sent.lanes[0].sessions[0].emblem, Some(emblem("red", "blue")));
        assert_eq!(sent.lanes[0].sessions[1].emblem, None);
        older.restart();
        assert_eq!(older.next(&snap).unwrap().lanes[0].sessions[0].emblem, Some(emblem("red", "blue")), "starting over keeps it what it is");
        let mut newer = Outbox::for_device(&codealong_peer::proto::KNOWS.map(String::from));
        assert_eq!(newer.next(&snap).unwrap().lanes[0].sessions[0].emblem, Some(emblem("rose", "sky")));
        // Every color it could get is one its windows draw.
        assert!(crate::emblems::COLORS.iter().all(|c| ["red", "pink", "purple", "indigo", "blue", "teal", "green"].contains(&crate::emblems::label_color(c))));
    }

    #[test]
    fn windows_get_keys_that_name_their_mac() {
        let mut replica = Replica::default();
        let helper = Session { helper: Some(crate::model::Helper { parent: Some("claude:claude:9f".into()), pr: None, report: false }), ..session("codex:codex:h1", Status::Working) };
        replica.apply(FeedMsg { rev: 1, full: true, lanes: vec![lane("/Users/sam/app", vec![session("claude:claude:9f", Status::Needs), helper])], ..Default::default() });
        let tag = DeviceTag { name: "Mac mini".into(), short: "a1b2c3d4e5".into(), color: "teal".into(), online: true, last_seen: None };
        let lanes = for_windows("a1b2c3d4e5", &tag, &replica);
        assert_eq!(lanes[0].id, "@a1b2c3d4e5:/Users/sam/app");
        assert_eq!(lanes[0].sessions[0].key, "@a1b2c3d4e5:claude:claude:9f");
        // A helper helps the session on that Mac, by the key it has here.
        assert_eq!(lanes[0].sessions[1].helper.as_ref().and_then(|h| h.parent.as_deref()), Some("@a1b2c3d4e5:claude:claude:9f"));
        assert_eq!(lanes[0].path, "/Users/sam/app");
        assert_eq!(lanes[0].device.as_ref(), Some(&tag));
        assert_eq!(split_remote(&lanes[0].sessions[0].key), Some(("a1b2c3d4e5", "claude:claude:9f")));
        assert_eq!(split_remote("claude:claude:9f"), None);
        // The windows split row keys on `::` and target ids on `|`; neither may appear.
        assert!(!lanes[0].id.contains("::") && !lanes[0].sessions[0].key.contains('|'));
    }
}
