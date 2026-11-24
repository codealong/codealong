//! Recent peer-to-peer messages between sessions (`codealong session send`), kept so Codealong
//! can warn agents when two peer sessions fall into a back-and-forth loop, surface active peer
//! chatter on each session (`Session::peers`), and flag ping-pong pairs in `codealong changes`.
//!
//! Parent↔helper exchanges (`Session::helper`, `origins::Store::helper_of`) and automatic PR
//! review helpers are left out: hub-and-spoke delegation to a helper is expected, whereas two
//! top-level peers messaging each other back and forth usually means unclear task ownership.
//!
//! Kept in `chatter.json` next to the config (through store.rs), pruned to the last day so a
//! server restart doesn't lose active threads and the file stays small.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};

use crate::model::{Board, PeerTalk, Session};
use crate::origins;
use crate::send::quoted;

/// How far back peer messages count toward chatter warnings and UI badges (2 hours).
pub const WINDOW_MS: i64 = 2 * 60 * 60 * 1000;
/// How long entries are kept in `chatter.json` before pruning (24 hours).
const KEEP_MS: i64 = 24 * 60 * 60 * 1000;
/// Maximum recent notes kept in `chatter.json`.
const MAX_NOTES: usize = 500;
/// Peer chatter threshold for warning the sender and flagging in `codealong changes`: at least
/// this many messages in `WINDOW_MS` with at least `WARN_FLIPS` direction changes.
pub const WARN_MESSAGES: u32 = 4;
pub const WARN_FLIPS: u32 = 2;

/// One peer-to-peer message sent from `from` to `to` at `at` (epoch ms).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Note {
    pub from: String,
    pub to: String,
    pub at: i64,
}

/// Summary of messages exchanged between two sessions inside `WINDOW_MS`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PairStats {
    pub count: u32,
    pub a_to_b: u32,
    pub b_to_a: u32,
    pub flips: u32,
    pub last_at: i64,
}

impl PairStats {
    /// Whether this pair has crossed the bidirectional ping-pong warning threshold.
    pub fn is_chatter(self) -> bool {
        self.count >= WARN_MESSAGES && self.a_to_b >= 1 && self.b_to_a >= 1 && self.flips >= WARN_FLIPS
    }

    /// Whether this pair is worth showing on a session's header as active peer messaging:
    /// bidirectional back-and-forth (at least 2 messages and a reply) or 3+ one-way messages.
    pub fn shown(self) -> bool {
        (self.count >= 2 && self.flips >= 1) || self.count >= 3
    }
}

/// Recent peer-to-peer sends, oldest first.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Log {
    notes: Vec<Note>,
}

impl Log {
    pub fn new(notes: Vec<Note>) -> Log {
        Log { notes }
    }

    /// Records a message from `from` to `to` at `now` (ms) and prunes stale entries.
    pub fn record(&mut self, from: &str, to: &str, now: i64) {
        if from.is_empty() || to.is_empty() || from == to {
            return;
        }
        self.notes.push(Note { from: from.to_string(), to: to.to_string(), at: now });
        self.prune(now);
    }

    fn prune(&mut self, now: i64) {
        let cutoff = now - KEEP_MS;
        self.notes.retain(|n| n.at >= cutoff);
        if self.notes.len() > MAX_NOTES {
            let drop = self.notes.len() - MAX_NOTES;
            self.notes.drain(..drop);
        }
    }

    /// Stats for messages between `a` and `b` within `WINDOW_MS` of `now`.
    pub fn pair_stats(&self, a: &str, b: &str, now: i64) -> PairStats {
        let cutoff = now - WINDOW_MS;
        let mut stats = PairStats::default();
        let mut last_dir: Option<bool> = None;
        for n in &self.notes {
            if n.at < cutoff {
                continue;
            }
            let dir = if n.from == a && n.to == b {
                true
            } else if n.from == b && n.to == a {
                false
            } else {
                continue;
            };
            stats.count += 1;
            if dir {
                stats.a_to_b += 1;
            } else {
                stats.b_to_a += 1;
            }
            if let Some(prev) = last_dir {
                if prev != dir {
                    stats.flips += 1;
                }
            }
            last_dir = Some(dir);
            if n.at > stats.last_at {
                stats.last_at = n.at;
            }
        }
        stats
    }

    /// A warning to return to `from` after messaging `to`, when their recent exchange has become
    /// bidirectional ping-pong chatter.
    pub fn warning(&self, from: &str, to: &str, to_title: &str, now: i64) -> Option<String> {
        let stats = self.pair_stats(from, to, now);
        if !stats.is_chatter() {
            return None;
        }
        let name = if to_title.trim().is_empty() { to.to_string() } else { quoted(to_title) };
        Some(format!(
            "You and {name} have exchanged {} messages in the last 2 hours. Keep one owner per branch, PR or deliverable: consolidate under one session or stop messaging unless a decision is blocked.",
            stats.count
        ))
    }

    /// Active peer exchanges for each non-archived session in `sessions`, excluding parent↔helper
    /// and PR-review-helper links. Sorted most messages first, then most recent.
    pub fn peers_for(&self, sessions: &[Session], origins: &origins::Store, now: i64) -> HashMap<String, Vec<PeerTalk>> {
        let cutoff = now - WINDOW_MS;
        let by_key: HashMap<&str, &Session> = sessions.iter().map(|s| (s.key.as_str(), s)).collect();
        let mut pairs: BTreeMap<(&str, &str), Vec<bool>> = BTreeMap::new();
        let mut last_at: HashMap<(&str, &str), i64> = HashMap::new();
        for n in &self.notes {
            if n.at < cutoff {
                continue;
            }
            let (Some(sf), Some(st)) = (by_key.get(n.from.as_str()), by_key.get(n.to.as_str())) else {
                continue;
            };
            if sf.archived || st.archived || is_helper_pair(sf, st, origins) {
                continue;
            }
            let (a, b, dir) = if sf.key <= st.key { (sf.key.as_str(), st.key.as_str(), true) } else { (st.key.as_str(), sf.key.as_str(), false) };
            pairs.entry((a, b)).or_default().push(dir);
            let entry = last_at.entry((a, b)).or_insert(0);
            if n.at > *entry {
                *entry = n.at;
            }
        }
        let mut out: HashMap<String, Vec<PeerTalk>> = HashMap::new();
        for ((a, b), dirs) in pairs {
            let mut stats = PairStats { last_at: last_at[&(a, b)], ..PairStats::default() };
            let mut prev = None;
            for dir in dirs {
                stats.count += 1;
                if dir {
                    stats.a_to_b += 1;
                } else {
                    stats.b_to_a += 1;
                }
                if let Some(p) = prev {
                    if p != dir {
                        stats.flips += 1;
                    }
                }
                prev = Some(dir);
            }
            if !stats.shown() {
                continue;
            }
            let (sa, sb) = (by_key[a], by_key[b]);
            out.entry(a.to_string()).or_default().push(PeerTalk { key: b.to_string(), title: sb.title.clone(), count: stats.count, flips: stats.flips, last_at: stats.last_at });
            out.entry(b.to_string()).or_default().push(PeerTalk { key: a.to_string(), title: sa.title.clone(), count: stats.count, flips: stats.flips, last_at: stats.last_at });
        }
        for list in out.values_mut() {
            list.sort_by(|x, y| y.count.cmp(&x.count).then_with(|| y.last_at.cmp(&x.last_at)).then_with(|| x.key.cmp(&y.key)));
        }
        out
    }
}

/// Whether `a` and `b` have a helper relationship (parent↔helper, or either is an automatic PR
/// review helper): those are expected hub-and-spoke exchanges, not peer chatter.
pub fn is_helper_pair(a: &Session, b: &Session, origins: &origins::Store) -> bool {
    let helps = |child: &Session, parent: &Session| {
        child.helper.as_ref().is_some_and(|h| h.parent.as_deref() == Some(parent.key.as_str()) || h.pr.is_some())
            || origins.helper_of(&child.key).is_some_and(|(p, _)| p == parent.key.as_str())
    };
    helps(a, b) || helps(b, a)
}

fn path() -> PathBuf {
    crate::config::support_dir().join("chatter.json")
}

fn log() -> &'static Mutex<Log> {
    static LOG: OnceLock<Mutex<Log>> = OnceLock::new();
    LOG.get_or_init(|| Mutex::new(Log::new(crate::store::load_whole(&path(), "peer chatter"))))
}

fn save(l: &Log) {
    if let Err(e) = crate::store::save_whole(&path(), &l.notes) {
        eprintln!("Couldn't save peer chatter log: {e:#}");
    }
}

/// Records an agent-to-agent send on this Mac and returns a ping-pong warning if `from` and `to`
/// are non-helper peers that have crossed the chatter threshold.
pub fn record_and_warn(board: &Board, from: &str, to: &str) -> Option<String> {
    let origins = origins::current();
    let sf = board.session(from).map(|(_, s)| s);
    let st = board.session(to).map(|(_, s)| s);
    if let (Some(a), Some(b)) = (sf, st) {
        if is_helper_pair(a, b, &origins) {
            return None;
        }
    }
    let now = crate::util::now_ms();
    let title = st.map(|s| s.title.as_str()).unwrap_or("");
    let mut guard = log().lock().unwrap_or_else(|e| e.into_inner());
    guard.record(from, to, now);
    save(&guard);
    guard.warning(from, to, title, now)
}

/// Active peer exchanges for every session in `sessions`, read from the server's chatter log.
pub fn peers_for(sessions: &[Session], origins: &origins::Store, now: i64) -> HashMap<String, Vec<PeerTalk>> {
    let guard = log().lock().unwrap_or_else(|e| e.into_inner());
    guard.peers_for(sessions, origins, now)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Helper, OriginHow};

    const NOW: i64 = 1_800_000_000_000;

    fn sess(key: &str, title: &str) -> Session {
        let mut s = crate::fixtures::session(key);
        s.key = key.into();
        s.title = title.into();
        s
    }

    #[test]
    fn bidirectional_peer_ping_pong_triggers_warning_and_peers_for() {
        let mut log = Log::default();
        let a = "claude:claude:alpha";
        let b = "claude:claude:beta";
        // Three messages in one direction don't trigger ping-pong warning yet.
        log.record(a, b, NOW - 30_000);
        log.record(a, b, NOW - 20_000);
        log.record(a, b, NOW - 10_000);
        assert_eq!(log.warning(a, b, "Beta", NOW), None);
        // Back-and-forth flips cross threshold on the 4th+ message with >=2 flips.
        let mut ping = Log::default();
        ping.record(a, b, NOW - 40_000);
        ping.record(b, a, NOW - 30_000);
        ping.record(a, b, NOW - 20_000);
        assert_eq!(ping.warning(a, b, "Beta", NOW), None, "only 3 messages so far");
        ping.record(b, a, NOW - 10_000);
        let warn = ping.warning(b, a, "Alpha", NOW).expect("4 messages and 3 flips warn");
        assert!(warn.contains("“Alpha”") && warn.contains("4 messages"), "{warn}");

        let sessions = vec![sess(a, "Alpha"), sess(b, "Beta")];
        let map = ping.peers_for(&sessions, &origins::Store::default(), NOW);
        assert_eq!(map[a], vec![PeerTalk { key: b.into(), title: "Beta".into(), count: 4, flips: 3, last_at: NOW - 10_000 }]);
        assert_eq!(map[b], vec![PeerTalk { key: a.into(), title: "Alpha".into(), count: 4, flips: 3, last_at: NOW - 10_000 }]);
    }

    #[test]
    fn helper_pairs_and_stale_messages_are_excluded() {
        let mut log = Log::default();
        let boss = "claude:claude:boss";
        let helper = "codex:codex:helper";
        for i in 0..6 {
            let (f, t) = if i % 2 == 0 { (boss, helper) } else { (helper, boss) };
            log.record(f, t, NOW - (6 - i) * 1000);
        }
        let mut sh = sess(helper, "Review helper");
        sh.helper = Some(Helper { parent: Some(boss.into()), ..Default::default() });
        let sessions = vec![sess(boss, "Boss"), sh];
        assert!(log.peers_for(&sessions, &origins::Store::default(), NOW).is_empty());

        // Also excluded when origins knows the helper link even before board helper is set.
        let mut origins = origins::Store::default();
        origins.sessions.insert(helper.into(), crate::origins::Entry { from: boss.into(), how: OriginHow::Started, at: NOW, helper: true, report: true, known: true });
        let plain = vec![sess(boss, "Boss"), sess(helper, "Review helper")];
        assert!(log.peers_for(&plain, &origins, NOW).is_empty());

        // Messages older than WINDOW_MS fall out of stats.
        let mut old = Log::default();
        let x = "claude:claude:x";
        let y = "claude:claude:y";
        old.record(x, y, NOW - WINDOW_MS - 1000);
        old.record(y, x, NOW - WINDOW_MS - 500);
        old.record(x, y, NOW - WINDOW_MS - 200);
        old.record(y, x, NOW - WINDOW_MS - 100);
        assert_eq!(old.pair_stats(x, y, NOW).count, 0);
        assert_eq!(old.warning(x, y, "Y", NOW), None);
    }
}
