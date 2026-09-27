//! A timing log for finding what makes the app slow. `[debug] perf_log = true` in config.toml
//! appends one line per event to ~/Library/Logs/Codealong/perf.log: every publish (its size,
//! what changed, how long composing took), file-watcher batches, git runs, engine and
//! main-thread lag, and what each window reports through `perf_mark`. Every line carries the
//! wall-clock time and the one-minute load average, so a slow moment can be matched to what
//! the Mac was doing. Off, nothing here costs more than an atomic load.

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::model::Board;
use crate::util::home;

static ARMED: AtomicBool = AtomicBool::new(false);
static ON: AtomicBool = AtomicBool::new(false);
static FILE: Mutex<Option<File>> = Mutex::new(None);

/// Lag below this isn't worth a line.
pub const LAG_FLOOR: Duration = Duration::from_millis(50);

pub fn enabled() -> bool {
    ON.load(Ordering::Relaxed)
}

pub fn log_path() -> PathBuf {
    home().join("Library/Logs/Codealong/perf.log")
}

/// Only the app logs; `--snapshot` runs load the same config and must not.
pub fn arm() {
    ARMED.store(true, Ordering::Relaxed);
}

/// Follows `debug.perf_log` in config.toml. Turning it on opens the file and writes a header.
pub fn set_enabled(on: bool) {
    if !ARMED.load(Ordering::Relaxed) || on == enabled() {
        return;
    }
    let mut slot = FILE.lock().unwrap_or_else(|e| e.into_inner());
    if !on {
        ON.store(false, Ordering::Relaxed);
        *slot = None;
        return;
    }
    let path = log_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    match OpenOptions::new().create(true).append(true).open(&path) {
        Ok(f) => {
            *slot = Some(f);
            drop(slot);
            ON.store(true, Ordering::Relaxed);
            log(&format!("perf log on: pid {}, Codealong {}, {}", std::process::id(), env!("CARGO_PKG_VERSION"), date()));
        }
        Err(e) => eprintln!("Couldn't open {}: {e}", path.display()),
    }
}

/// Appends a line, stamped with the time of day and the load average.
pub fn log(line: &str) {
    if !enabled() {
        return;
    }
    let stamp = format!("{} load={:.1} {line}", clock(), loadavg());
    if let Ok(mut slot) = FILE.lock() {
        if let Some(f) = slot.as_mut() {
            let _ = writeln!(f, "{stamp}");
        }
    }
}

/// Logs how long something waited, when it's long enough to matter.
pub fn lag(what: &str, waited: Duration) {
    if waited >= LAG_FLOOR {
        log(&format!("{what} lag {} ms", waited.as_millis()));
    }
}

/// What the engine did between two publishes, and what each publish changed.
#[derive(Default)]
pub struct Counters {
    pub fs_batches: u32,
    pub fs_paths: u32,
    pub fs_ms: u128,
    pub git_jobs: u32,
    lanes: HashMap<String, u64>,
    sessions: HashMap<String, u64>,
    prs: HashMap<String, u64>,
    /// The rest of the board, part by part, to name what else made a publish differ.
    rest: HashMap<String, u64>,
}

impl Counters {
    /// A file-watcher batch: how many paths, how long they took, and for a big one where they
    /// were (`busiest`).
    pub fn fs(&mut self, paths: usize, took: Duration, busiest: Option<String>) {
        self.fs_batches += 1;
        self.fs_paths += paths as u32;
        self.fs_ms += took.as_millis();
        if took >= LAG_FLOOR {
            let at = busiest.map(|b| format!(", most in {b}")).unwrap_or_default();
            log(&format!("fs batch: {paths} paths, {} ms{at}", took.as_millis()));
        }
    }

    /// One line for a publish, and the counters start over.
    pub fn publish_line(&mut self, board: &Board, compose: Duration, hash: Duration, emit: Duration, since_last: Option<Duration>, git_running: usize) -> String {
        let bytes = serde_json::to_string(board).map(|s| s.len()).unwrap_or(0);
        let lanes = diff(&mut self.lanes, board.lanes.iter().map(|l| (l.id.clone(), hash_of(&l))));
        let sessions = diff(&mut self.sessions, board.lanes.iter().flat_map(|l| l.sessions.iter()).map(|s| (s.key.clone(), hash_of(&s))));
        let prs = diff(&mut self.prs, board.prs.iter().map(|p| (p.key.clone(), hash_of(&p))));
        let parts = [
            ("apps", hash_of(&board.apps)),
            ("projects", hash_of(&board.projects)),
            ("warnings", hash_of(&board.warnings)),
            ("loading", hash_of(&board.loading)),
            ("githubAt", hash_of(&board.github_at)),
            ("usage", hash_of(&board.usage)),
            ("pinned", hash_of(&board.pinned)),
            ("pinnedWorktrees", hash_of(&board.pinned_worktrees)),
            ("githubLogin", hash_of(&board.github_login)),
            ("theme", hash_of(&board.theme)),
        ];
        let mut rest: Vec<&str> = parts.iter().filter(|(k, h)| self.rest.get(*k) != Some(h)).map(|(k, _)| *k).collect();
        self.rest = parts.iter().map(|(k, h)| (k.to_string(), *h)).collect();
        if rest.is_empty() {
            rest.push("nothing else");
        }
        let since = since_last.map(|d| format!("{} ms", d.as_millis())).unwrap_or_else(|| "first".into());
        let line = format!(
            "publish {} KB after {since}: changed {lanes} of {} lanes, {sessions} of {} sessions, {prs} of {} prs, {}; compose {} ms, hash {} ms, emit {} ms; since last: {} fs batches ({} paths, {} ms), {} git runs, {git_running} running",
            bytes / 1024,
            board.lanes.len(),
            self.sessions.len(),
            board.prs.len(),
            rest.join(", "),
            compose.as_millis(),
            hash.as_millis(),
            emit.as_millis(),
            self.fs_batches,
            self.fs_paths,
            self.fs_ms,
            self.git_jobs,
        );
        self.fs_batches = 0;
        self.fs_paths = 0;
        self.fs_ms = 0;
        self.git_jobs = 0;
        line
    }
}

/// Replaces `seen` with `fresh` and counts entries that are new or different.
fn diff(seen: &mut HashMap<String, u64>, fresh: impl Iterator<Item = (String, u64)>) -> usize {
    let fresh: HashMap<String, u64> = fresh.collect();
    let changed = fresh.iter().filter(|(k, h)| seen.get(*k) != Some(h)).count();
    *seen = fresh;
    changed
}

fn hash_of<T: serde::Serialize>(v: &T) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    serde_json::to_string(v).unwrap_or_default().hash(&mut h);
    h.finish()
}

fn loadavg() -> f64 {
    let mut l = [0f64; 3];
    // SAFETY: getloadavg writes at most the one value asked for into a buffer of three.
    let n = unsafe { libc::getloadavg(l.as_mut_ptr(), 1) };
    if n >= 1 { l[0] } else { 0.0 }
}

fn local_tm() -> (libc::tm, u32) {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    let secs = now.as_secs() as libc::time_t;
    // SAFETY: localtime_r fills the struct it's given; a zeroed tm is a valid one.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::localtime_r(&secs, &mut tm) };
    (tm, now.subsec_millis())
}

fn clock() -> String {
    let (tm, ms) = local_tm();
    format!("{:02}:{:02}:{:02}.{ms:03}", tm.tm_hour, tm.tm_min, tm.tm_sec)
}

/// The local date and time, for other logs.
pub(crate) fn stamp() -> String {
    format!("{} {}", date(), clock())
}

fn date() -> String {
    let (tm, _) = local_tm();
    format!("{}-{:02}-{:02}", tm.tm_year + 1900, tm.tm_mon + 1, tm.tm_mday)
}

/// A batch this big says where its paths were.
const BIG_BATCH: usize = 1000;

/// For a big batch of changed paths, the folders most of them were in (three levels under the
/// home folder, four elsewhere), with counts: what a build or an install was writing to.
pub fn busiest(paths: &[PathBuf]) -> Option<String> {
    if !enabled() || paths.len() < BIG_BATCH {
        return None;
    }
    Some(busiest_in(paths, &home()))
}

fn busiest_in(paths: &[PathBuf], home: &std::path::Path) -> String {
    let mut counts: HashMap<PathBuf, usize> = HashMap::new();
    for p in paths {
        let folder: PathBuf = match p.strip_prefix(home) {
            Ok(rel) => home.join(rel.components().take(3).collect::<PathBuf>()),
            Err(_) => p.components().take(5).collect(),
        };
        *counts.entry(folder).or_default() += 1;
    }
    let mut top: Vec<(PathBuf, usize)> = counts.into_iter().collect();
    top.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    top.iter().take(3).map(|(f, n)| format!("{} ({n})", f.display())).collect::<Vec<_>>().join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_counts_new_and_changed_entries_and_forgets_gone_ones() {
        let mut seen = HashMap::new();
        assert_eq!(diff(&mut seen, vec![("a".into(), 1), ("b".into(), 2)].into_iter()), 2);
        assert_eq!(diff(&mut seen, vec![("a".into(), 1), ("b".into(), 3)].into_iter()), 1);
        assert_eq!(diff(&mut seen, vec![("b".into(), 3)].into_iter()), 0);
        assert_eq!(seen.len(), 1);
    }

    #[test]
    fn logging_is_off_until_armed_and_enabled() {
        assert!(!enabled());
        set_enabled(true);
        assert!(!enabled(), "an unarmed process (a --snapshot run) never logs");
    }

    #[test]
    fn busiest_names_the_folders_a_big_batch_was_in() {
        let home = std::path::Path::new("/Users/me");
        let mut paths: Vec<PathBuf> = (0..5).map(|i| home.join(format!("code/app/node_modules/pkg{i}/index.js"))).collect();
        paths.extend((0..2).map(|i| PathBuf::from(format!("/private/tmp/build/out/file{i}"))));
        paths.push(home.join("code/app/src/main.ts"));
        assert_eq!(busiest_in(&paths, home), "/Users/me/code/app/node_modules (5), /private/tmp/build/out (2), /Users/me/code/app/src (1)");
    }
}
