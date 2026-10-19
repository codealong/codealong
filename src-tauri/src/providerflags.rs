//! The checked prefix of each append-only provider record. Cold readers still inspect their
//! tails; this cache keeps old unflagged history from being read again at every launch or
//! settings reload. Reader scans flush together through store.rs, merging with other processes.
//! A changed file identity, truncation or same-length rewrite starts over. Bump the filename
//! when the harnesses' flag rules change, so an older negative result cannot hide a new signal.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;
use serde::{Deserialize, Serialize};

const WHAT: &str = "checked provider records";
type Entries = BTreeMap<PathBuf, Entry>;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
struct Stamp {
    len: u64,
    modified: Option<SystemTime>,
    created: Option<SystemTime>,
}

impl Stamp {
    fn read(path: &Path) -> std::io::Result<Self> {
        let m = std::fs::metadata(path)?;
        Ok(Self { len: m.len(), modified: m.modified().ok(), created: m.created().ok() })
    }

    fn follows(&self, before: &Self) -> bool {
        self.created == before.created && (self == before || self.len > before.len)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
struct Entry {
    stamp: Stamp,
    end: u64,
    flagged: bool,
}

/// Only trust a result for the same prefix, or one extended by an append. Failed reads are
/// never cached as unflagged. `floor` may skip bytes the incremental parser already checked.
fn checked(before: Option<&Entry>, stamp: Stamp, floor: u64, end: u64, scan: impl FnOnce(u64, u64) -> std::io::Result<bool>) -> std::io::Result<Entry> {
    let before = before.filter(|p| stamp.follows(&p.stamp) && p.end <= end);
    let from = before.map_or(floor, |p| p.end.max(floor));
    let flagged = before.is_some_and(|p| p.flagged) || (from < end && scan(from, end)?);
    Ok(Entry { stamp, end, flagged })
}

#[derive(Default)]
struct Cache {
    entries: Entries,
    pending: Entries,
}

fn path() -> PathBuf { crate::config::support_dir().join("provider-flags-v1.json") }

fn cache() -> &'static Mutex<Cache> {
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(Cache { entries: crate::store::load_whole(&path(), WHAT), ..Cache::default() }))
}

pub fn before(path: &Path, floor: u64, end: u64, markers: &[&str], flagged: impl Fn(&serde_json::Value) -> bool) -> bool {
    // Parser fixtures mustn't read or change this Mac's cache; the cache has its own tests.
    if cfg!(test) { return crate::util::flagged_before(path, floor, end, markers, flagged).unwrap_or(false); }
    let Ok(stamp) = Stamp::read(path) else { return false };
    let prior = cache().lock().unwrap_or_else(|e| e.into_inner()).entries.get(path).cloned();
    // Don't hold the cache lock over file reads: the harness readers parse in parallel.
    let Ok(entry) = checked(prior.as_ref(), stamp.clone(), floor, end, |from, to| crate::util::flagged_before(path, from, to, markers, flagged)) else { return false };
    let found = entry.flagged;
    if prior.as_ref() != Some(&entry) && Stamp::read(path).ok().as_ref() == Some(&stamp) {
        let mut c = cache().lock().unwrap_or_else(|e| e.into_inner());
        c.entries.insert(path.to_path_buf(), entry.clone());
        c.pending.insert(path.to_path_buf(), entry);
    }
    found
}

fn save(path: &Path, pending: &Entries) -> anyhow::Result<()> {
    crate::store::change(path, WHAT, |entries: &mut Entries| {
        entries.extend(pending.clone());
        true
    }).map(|_| ())
}

/// One write after a reader's scan, including when a snapshot exits immediately afterwards.
/// Unreadable cache files stay untouched and produce the usual store notice.
pub fn flush() {
    if cfg!(test) { return; }
    let mut c = cache().lock().unwrap_or_else(|e| e.into_inner());
    if c.pending.is_empty() { return; }
    let p = path();
    if crate::store::noted(&p, save(&p, &c.pending)).is_ok() { c.pending.clear(); }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stamp(len: u64, tick: u64) -> Stamp {
        Stamp { len, modified: Some(SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(tick)), created: Some(SystemTime::UNIX_EPOCH) }
    }

    #[test]
    fn restarts_and_appends_reuse_only_the_checked_prefix() {
        let dir = crate::store::test_dir("provider-flag-cache");
        let path = dir.join("flags.json");
        let key = PathBuf::from("/atlas/transcript.jsonl");
        let first = checked(None, stamp(100, 1), 0, 80, |from, to| { assert_eq!((from, to), (0, 80)); Ok(false) }).unwrap();
        save(&path, &Entries::from([(key.clone(), first)])).unwrap();
        let entries: Entries = crate::store::read(&path, WHAT).unwrap().unwrap();
        let same = checked(entries.get(&key), stamp(100, 1), 0, 80, |_, _| panic!("unchanged history was read again")).unwrap();
        let more = checked(Some(&same), stamp(200, 2), 0, 180, |from, to| { assert_eq!((from, to), (80, 180)); Ok(true) }).unwrap();
        assert!(more.flagged);
        assert!(checked(Some(&more), stamp(300, 3), 0, 280, |_, _| panic!("known flag was scanned again")).unwrap().flagged);
        for changed in [stamp(90, 2), stamp(100, 2), Stamp { created: None, ..stamp(200, 2) }] {
            let read = checked(Some(&same), changed, 0, 70, |from, _| { assert_eq!(from, 0); Ok(true) }).unwrap();
            assert!(read.flagged);
        }
        assert!(checked(Some(&same), stamp(200, 2), 0, 180, |_, _| Err(std::io::ErrorKind::UnexpectedEof.into())).is_err());
        // A snapshot and the app merge their entries; a broken file is never overwritten.
        save(&path, &Entries::from([(PathBuf::from("/kestrel/record.jsonl"), more)])).unwrap();
        assert_eq!(crate::store::read::<Entries>(&path, WHAT).unwrap().unwrap().len(), 2);
        std::fs::write(&path, "{").unwrap();
        assert!(save(&path, &entries).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
