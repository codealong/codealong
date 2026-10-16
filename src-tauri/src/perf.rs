//! A timing log for finding what makes the app slow. `[debug] perf_log = true` in config.toml
//! appends one line per event to ~/Library/Logs/Codealong/perf.log: every publish (its size,
//! what changed, how long composing took), file-watcher batches, git runs, engine and
//! main-thread lag, and what each window reports through `perf_mark`. Every line carries the
//! wall-clock time and the one-minute load average, so a slow moment can be matched to what
//! the Mac was doing. A main thread stuck for a second is sampled while it's stuck
//! (`sample_stall`), into `stalls/` beside the log. Off, nothing here costs more than an atomic
//! load.

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
    crate::config::logs_dir().join("perf.log")
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

/// What a probe hands the main thread to run.
pub type Probe = Box<dyn FnOnce() + Send>;

/// One probe of the main thread: `post` hands it something to run there, and this waits for it.
/// It returns how long that waited, one line's worth however long it was, or None when it never
/// ran (the app is closing). While it waits, `stuck` is called at `stuck_after` and again at
/// `hung_after`: the moments the main thread is stuck, and a sample shows on what.
pub fn probe(post: impl FnOnce(Probe) -> bool, stuck_after: Duration, hung_after: Duration, mut stuck: impl FnMut(Duration)) -> Option<Duration> {
    use std::sync::mpsc::RecvTimeoutError;
    let t = std::time::Instant::now();
    let (tx, rx) = std::sync::mpsc::channel();
    let ran: Probe = Box::new(move || {
        let _ = tx.send(t.elapsed());
    });
    if !post(ran) {
        return None;
    }
    for at in [stuck_after, hung_after] {
        match rx.recv_timeout(at.saturating_sub(t.elapsed())) {
            Ok(waited) => return Some(waited),
            Err(RecvTimeoutError::Disconnected) => return None,
            Err(RecvTimeoutError::Timeout) => stuck(at),
        }
    }
    rx.recv().ok()
}

/// Where samples of a stuck main thread go.
pub fn stalls_dir() -> PathBuf {
    crate::config::logs_dir().join("stalls")
}

/// Samples kept; the oldest go as new ones come.
const KEEP_STALLS: usize = 20;

/// Records what every thread of the app does for two seconds, with `/usr/bin/sample`, into a
/// file in `stalls/`, and logs where the main thread spent them, after `why`. It's called while
/// the main thread is stuck, so the main thread's stack names what it's waiting for. Off the
/// calling thread. Codealong's own frames show as offsets (the release build is stripped); a
/// build of the same commit with `CARGO_PROFILE_RELEASE_STRIP=false` names them, with `atos -o
/// <that binary> -l <the report's load address> <address>`.
pub fn sample_stall(why: String) {
    if !enabled() {
        return;
    }
    std::thread::spawn(move || {
        let dir = stalls_dir();
        let _ = std::fs::create_dir_all(&dir);
        let name = format!("stall-{}.txt", file_stamp());
        let file = dir.join(&name);
        let pid = std::process::id().to_string();
        let path = file.to_string_lossy().into_owned();
        let ran = crate::procs::run("/usr/bin/sample", &[&pid, "2", "5", "-mayDie", "-file", &path], None, Duration::from_secs(120));
        let report = std::fs::read_to_string(&file).unwrap_or_default();
        let found = match (main_thread_path(&report), ran) {
            (Some(p), _) => p.describe(),
            (None, Err(e)) => format!("sample failed: {e:#}"),
            (None, Ok(_)) => "the sample has no main thread".to_string(),
        };
        log(&format!("{why}: {found}; every thread in stalls/{name}"));
        prune_stalls(&dir);
    });
}

fn prune_stalls(dir: &std::path::Path) {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("stall-"))).collect();
    // Their names start with the time, so they sort oldest first.
    files.sort();
    for old in files.iter().take(files.len().saturating_sub(KEEP_STALLS)) {
        let _ = std::fs::remove_file(old);
    }
}

/// The main thread's heaviest path in a `sample` report: from the thread down to the frame
/// most of its samples were in.
#[derive(Debug, PartialEq)]
pub struct StallPath {
    /// The main thread's samples in all.
    pub total: u32,
    /// The samples in the path's last frame.
    pub on: u32,
    /// Outermost first.
    pub frames: Vec<String>,
}

impl StallPath {
    /// Where the main thread was, innermost frame first, for the log.
    pub fn describe(&self) -> String {
        let share = if self.total > 0 { self.on * 100 / self.total } else { 0 };
        let inner: Vec<&str> = self.frames.iter().rev().take(12).map(String::as_str).collect();
        // Blocked in the event loop's own wait: nothing of Codealong's, AppKit's or WebKit's was
        // running, so the thread was waiting for an event, or for the CPU to take one.
        let idle = self.frames.last().is_some_and(|f| f.starts_with("mach_msg")) && self.frames.iter().any(|f| f.contains("nextEventMatchingMask"));
        format!("{share}% of {} samples in {}{}", self.total, inner.join(" < "), if idle { " (waiting in its event loop)" } else { "" })
    }
}

/// Finds the main thread's heaviest path in a `sample` report. The call graph lists each
/// thread's frames as a tree whose children come heaviest first, so the path is the thread's
/// first line and each line after it that sits deeper than the one before.
pub fn main_thread_path(report: &str) -> Option<StallPath> {
    let graph = report.split("Call graph:").nth(1)?;
    let mut lines = graph.lines();
    let (mut depth, total) = loop {
        let (indent, count, text) = match graph_line(lines.next()?) {
            Some(l) => l,
            None => continue,
        };
        if text.ends_with(": main") || text.contains("com.apple.main-thread") {
            break (indent, count);
        }
    };
    let mut frames = Vec::new();
    let mut on = total;
    for line in lines {
        let Some((indent, count, text)) = graph_line(line) else { break };
        if indent <= depth {
            break;
        }
        frames.push(frame_name(text));
        depth = indent;
        on = count;
    }
    Some(StallPath { total, on, frames })
}

/// A call-graph line: the column its count starts at (its depth), the count, and the frame.
/// What comes before the count is indentation and the tree's own marks.
fn graph_line(line: &str) -> Option<(usize, u32, &str)> {
    let at = line.find(|c: char| c.is_ascii_digit())?;
    if !line[..at].chars().all(|c| matches!(c, ' ' | '+' | '!' | ':' | '|')) {
        return None;
    }
    let rest = &line[at..];
    let end = rest.find(' ')?;
    Some((at, rest[..end].parse().ok()?, rest[end..].trim()))
}

/// A frame's symbol, or for one without (Codealong's own) its image and offset.
fn frame_name(text: &str) -> String {
    let (symbol, rest) = text.split_once("  (in ").unwrap_or((text, ""));
    if symbol != "???" {
        return symbol.to_string();
    }
    let image = rest.split(')').next().unwrap_or("?");
    let offset = rest.split(" + ").nth(1).and_then(|o| o.split_whitespace().next()).unwrap_or("?");
    format!("{image}+{offset}")
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
            ("notices", hash_of(&board.notices)),
            ("loading", hash_of(&board.loading)),
            ("githubAt", hash_of(&board.github_at)),
            ("usage", hash_of(&board.usage)),
            ("pinned", hash_of(&board.pinned)),
            ("pinnedWorktrees", hash_of(&board.pinned_worktrees)),
            ("pinnedPrs", hash_of(&board.pinned_prs)),
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

/// The local time for a file name, sorting as it reads: 2026-09-27-191005.
fn file_stamp() -> String {
    let (tm, _) = local_tm();
    format!("{}-{:02}{:02}{:02}", date(), tm.tm_hour, tm.tm_min, tm.tm_sec)
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

    /// A probe against a stand-in main thread the test drives: what's posted comes out of a
    /// channel, and the test runs it once the probe has said it's stuck as often as asked.
    fn probe_stuck_until(moments: usize) -> (Option<Duration>, Vec<Duration>) {
        let (stuck_after, hung_after) = (Duration::from_millis(50), Duration::from_millis(150));
        let (post_tx, posted) = std::sync::mpsc::channel::<Probe>();
        let (said_tx, said) = std::sync::mpsc::channel::<Duration>();
        let prober = std::thread::spawn(move || {
            probe(move |p| post_tx.send(p).is_ok(), stuck_after, hung_after, |after| {
                let _ = said_tx.send(after);
            })
        });
        let run = posted.recv().unwrap();
        let heard: Vec<Duration> = (0..moments).map(|_| said.recv().unwrap()).collect();
        run();
        (prober.join().unwrap(), heard)
    }

    #[test]
    fn probes_say_when_the_main_thread_is_stuck_and_for_how_long() {
        // Stuck past the first moment: that's when it's sampled, and the whole wait is logged.
        let (waited, heard) = probe_stuck_until(1);
        assert_eq!(heard, [Duration::from_millis(50)]);
        assert!(waited.unwrap() >= Duration::from_millis(50));
        // Frozen: sampled again at the second.
        let (waited, heard) = probe_stuck_until(2);
        assert_eq!(heard, [Duration::from_millis(50), Duration::from_millis(150)]);
        assert!(waited.unwrap() >= Duration::from_millis(150));
        // A probe the main thread drops (the app closing), or that can't be posted, says nothing.
        let dropped = probe(|p| {
            drop(p);
            true
        }, Duration::from_secs(5), Duration::from_secs(10), |_| panic!("dropped, not stuck"));
        assert_eq!(dropped, None);
        assert_eq!(probe(|_| false, Duration::from_secs(5), Duration::from_secs(10), |_| panic!("never posted")), None);
    }

    const BUSY: &str = "Analysis of sampling codealong (pid 1) every 5 milliseconds
Load Address:    0x10092c000

Call graph:
    400 Thread_1001: main
    + 400 start  (in dyld) + 6992  [0x184d804e4]
    +   400 ???  (in codealong)  load address 0x10092c000 + 0x1054  [0x10092d054]
    +     390 -[NSApplication run]  (in AppKit) + 368  [0x18963113c]
    +     ! 390 -[NSApplication(NSEventRouting) sendEvent:]  (in AppKit) + 964  [0x18a1d2390]
    +     !   385 +[NSWindow windowNumberAtPoint:belowWindowWithWindowNumber:]  (in AppKit) + 164  [0x1897cecac]
    +     !   : 385 SLSCopyWindowRoutingRecordsForScreenLocation  (in SkyLight) + 180  [0x18bc784b8]
    +     !   : | 385 mach_msg2_trap  (in libsystem_kernel.dylib) + 8  [0x185107c34]
    +     !   5 objc_msgSend  (in libobjc.A.dylib) + 56  [0x184ce7038]
    +     10 ???  (in codealong)  load address 0x10092c000 + 0x2000  [0x10092e000]
    400 Thread_1002: com.apple.NSEventThread
    + 400 thread_start  (in libsystem_pthread.dylib) + 8  [0x18514ab80]

Total number in stack (recursive counted multiple, when >=5):
";

    #[test]
    fn finds_where_a_stuck_main_thread_waits() {
        let p = main_thread_path(BUSY).unwrap();
        assert_eq!((p.total, p.on), (400, 385));
        assert_eq!(
            p.frames,
            ["start", "codealong+0x1054", "-[NSApplication run]", "-[NSApplication(NSEventRouting) sendEvent:]", "+[NSWindow windowNumberAtPoint:belowWindowWithWindowNumber:]", "SLSCopyWindowRoutingRecordsForScreenLocation", "mach_msg2_trap"]
        );
        assert_eq!(
            p.describe(),
            "96% of 400 samples in mach_msg2_trap < SLSCopyWindowRoutingRecordsForScreenLocation < +[NSWindow windowNumberAtPoint:belowWindowWithWindowNumber:] < -[NSApplication(NSEventRouting) sendEvent:] < -[NSApplication run] < codealong+0x1054 < start"
        );
        assert_eq!(main_thread_path("no call graph here"), None);
    }

    #[test]
    fn says_when_the_main_thread_was_only_waiting_for_events() {
        let idle = "Call graph:
    200 Thread_1: main
    + 200 -[NSApplication(NSEventRouting) nextEventMatchingMask:untilDate:inMode:dequeue:]  (in AppKit) + 72  [0x18a1d3678]
    +   200 _BlockUntilNextEventMatchingListInMode  (in HIToolbox) + 48  [0x19218014c]
    +     200 mach_msg2_trap  (in libsystem_kernel.dylib) + 8  [0x185107c34]
";
        let d = main_thread_path(idle).unwrap().describe();
        assert!(d.starts_with("100% of 200 samples in mach_msg2_trap < _BlockUntilNextEventMatchingListInMode"), "{d}");
        assert!(d.ends_with("(waiting in its event loop)"), "{d}");
    }

    /// Samples this test process with the real `sample` and reads its main thread back, so the
    /// report's format is checked against the tool itself: `cargo test samples_itself -- --ignored`.
    #[test]
    #[ignore]
    fn samples_itself() {
        let file = std::env::temp_dir().join(format!("codealong-sample-{}.txt", std::process::id()));
        let pid = std::process::id().to_string();
        crate::procs::run("/usr/bin/sample", &[&pid, "1", "5", "-mayDie", "-file", &file.to_string_lossy()], None, Duration::from_secs(120)).unwrap();
        let report = std::fs::read_to_string(&file).unwrap();
        let _ = std::fs::remove_file(&file);
        let p = main_thread_path(&report).expect("a main thread");
        assert!(p.total > 0 && !p.frames.is_empty(), "{p:?}");
        println!("{}", p.describe());
    }

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
