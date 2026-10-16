//! Reads that macOS can hold until someone answers a privacy prompt. On a removable volume, or
//! in Documents, Desktop or Downloads, opening a file or listing a folder waits in `open()` until
//! "Codealong would like to access…" is answered, which on a Mac nobody sits at can be hours;
//! stat, realpath and a file watch there don't wait (macOS 26.6, checked 2026-09-27). macOS keeps
//! the answer for builds with the same certificate, and asks again after every build signed ad
//! hoc (relaunch.sh's on a Mac without an Apple Development certificate). The repo scan reads the
//! files of folders it doesn't own through `Held`: each read runs on a thread of its own, the
//! scan waits for them a moment and goes on without the ones still out, and each of those lands
//! when it's back, for the next scan to use.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use crate::perf;

/// Reads of one kind (a scan root's checkouts, a worktree's files), by path.
pub struct Held<T> {
    /// What the reads are of, for the perf log.
    what: &'static str,
    read: Arc<dyn Fn(&Path) -> T + Send + Sync>,
    /// Called, on the thread that read it, when a read lands after its scan went on.
    landed: Arc<dyn Fn() + Send + Sync>,
    shared: Arc<Shared<T>>,
}

struct Shared<T> {
    state: Mutex<State<T>>,
    back: Condvar,
}

struct State<T> {
    scans: u64,
    /// The scan waiting for its reads, by its number.
    waiting: Option<u64>,
    /// Reads that came back while their scan waited.
    got: Vec<(PathBuf, T)>,
    /// Reads not back yet, and when each started.
    out: HashMap<PathBuf, Instant>,
    /// Reads that came back after their scan went on.
    landed: HashMap<PathBuf, T>,
}

impl<T: Send + 'static> Held<T> {
    pub fn new(what: &'static str, read: impl Fn(&Path) -> T + Send + Sync + 'static, landed: impl Fn() + Send + Sync + 'static) -> Held<T> {
        let state = State { scans: 0, waiting: None, got: Vec::new(), out: HashMap::new(), landed: HashMap::new() };
        Held { what, read: Arc::new(read), landed: Arc::new(landed), shared: Arc::new(Shared { state: Mutex::new(state), back: Condvar::new() }) }
    }

    /// Reads each path on a thread of its own and waits up to `wait` for them. Returns what came
    /// back in time and what landed since the last scan. A path whose read is still out isn't
    /// read again, and one that doesn't come back in time is left out: it lands later.
    pub fn read(&self, paths: impl IntoIterator<Item = PathBuf>, wait: Duration) -> HashMap<PathBuf, T> {
        let paths: HashSet<PathBuf> = paths.into_iter().collect();
        let mut state = self.lock();
        // What landed is used once, and dropped when its path isn't asked about any more.
        let mut done: HashMap<PathBuf, T> = std::mem::take(&mut state.landed).into_iter().filter(|(p, _)| paths.contains(p)).collect();
        state.scans += 1;
        let scan = state.scans;
        state.waiting = Some(scan);
        let started = Instant::now();
        let mut asked = Vec::new();
        for path in paths {
            if done.contains_key(&path) || state.out.contains_key(&path) {
                continue;
            }
            // A thread that can't start leaves its path unread rather than wait here for it.
            if self.spawn(scan, path.clone()) {
                state.out.insert(path.clone(), started);
                asked.push(path);
            }
        }
        let until = started + wait;
        while state.got.len() < asked.len() {
            let now = Instant::now();
            if now >= until {
                break;
            }
            state = self.shared.back.wait_timeout(state, until - now).map(|(s, _)| s).unwrap_or_else(|e| e.into_inner().0);
        }
        state.waiting = None;
        for (path, value) in std::mem::take(&mut state.got) {
            state.out.remove(&path);
            done.insert(path, value);
        }
        let held: Vec<String> = asked.iter().filter(|p| state.out.contains_key(*p)).map(|p| p.display().to_string()).collect();
        drop(state);
        if !held.is_empty() {
            perf::log(&format!("held: {} of {} {} reads still out after {} ms: {}", held.len(), asked.len(), self.what, started.elapsed().as_millis(), held.join(", ")));
        }
        done
    }

    /// Whether a read of `path` is out: a scan went on without it.
    pub fn is_out(&self, path: &Path) -> bool {
        self.lock().out.contains_key(path)
    }

    /// The paths whose reads have been out at least `wait`, in order.
    pub fn out_for(&self, wait: Duration) -> Vec<PathBuf> {
        let mut out: Vec<PathBuf> = self.lock().out.iter().filter(|(_, since)| since.elapsed() >= wait).map(|(p, _)| p.clone()).collect();
        out.sort();
        out
    }

    fn spawn(&self, scan: u64, path: PathBuf) -> bool {
        let (what, read, landed, shared) = (self.what, self.read.clone(), self.landed.clone(), self.shared.clone());
        std::thread::Builder::new()
            .name("codealong-read".into())
            .spawn(move || {
                crate::os::system::utility_thread();
                let value = read(&path);
                let mut state = shared.state.lock().unwrap_or_else(|e| e.into_inner());
                if state.waiting == Some(scan) {
                    state.got.push((path, value));
                    shared.back.notify_all();
                    return;
                }
                let since = state.out.remove(&path);
                let line = format!("held: {what} read landed after {} ms: {}", since.map(|t| t.elapsed().as_millis()).unwrap_or(0), path.display());
                state.landed.insert(path, value);
                drop(state);
                perf::log(&line);
                landed();
            })
            .is_ok()
    }

    fn lock(&self) -> MutexGuard<'_, State<T>> {
        self.shared.state.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc::{channel, Receiver};

    /// Reads `/fast` at once, `/slow` once `release` is sent something, and never `/stuck`.
    fn reader(release: Receiver<()>, calls: Arc<AtomicUsize>) -> impl Fn(&Path) -> String + Send + Sync + 'static {
        let release = Mutex::new(release);
        move |p: &Path| {
            calls.fetch_add(1, Ordering::SeqCst);
            if p == Path::new("/stuck") {
                loop {
                    std::thread::park();
                }
            }
            if p == Path::new("/slow") {
                let _ = release.lock().unwrap().recv();
            }
            format!("read {}", p.display())
        }
    }

    #[test]
    fn a_read_still_out_holds_up_only_itself_and_lands_later() {
        let (release, gate) = channel();
        let (landed_tx, landed) = channel();
        let calls = Arc::new(AtomicUsize::new(0));
        let landed_tx = Mutex::new(landed_tx);
        let held = Held::new("test", reader(gate, calls.clone()), move || {
            let _ = landed_tx.lock().unwrap().send(());
        });
        let paths = || ["/fast", "/slow", "/stuck"].map(PathBuf::from);

        let t = Instant::now();
        let got = held.read(paths(), Duration::from_millis(300));
        assert!(t.elapsed() < Duration::from_secs(5));
        assert_eq!(got.len(), 1);
        assert_eq!(got[Path::new("/fast")], "read /fast");
        assert!(held.is_out(Path::new("/slow")) && held.is_out(Path::new("/stuck")));
        assert!(!held.is_out(Path::new("/fast")));
        assert_eq!(held.out_for(Duration::ZERO), paths()[1..].to_vec());
        assert!(held.out_for(Duration::from_secs(60)).is_empty());

        // Reads still out aren't started again.
        let got = held.read(paths(), Duration::from_millis(100));
        assert_eq!(got.keys().collect::<Vec<_>>(), vec![Path::new("/fast")]);
        assert_eq!(calls.load(Ordering::SeqCst), 4);

        // The slow one lands, and the next scan has it without reading it again.
        release.send(()).unwrap();
        landed.recv_timeout(Duration::from_secs(5)).expect("the slow read lands");
        assert!(!held.is_out(Path::new("/slow")));
        let got = held.read(paths(), Duration::from_millis(100));
        assert_eq!(got[Path::new("/slow")], "read /slow");
        assert_eq!(calls.load(Ordering::SeqCst), 5, "only /fast was read again");
        assert!(held.is_out(Path::new("/stuck")));
    }

    #[test]
    fn what_landed_for_a_path_no_longer_asked_about_is_dropped() {
        let (release, gate) = channel();
        let (landed_tx, landed) = channel();
        let calls = Arc::new(AtomicUsize::new(0));
        let landed_tx = Mutex::new(landed_tx);
        let held = Held::new("test", reader(gate, calls.clone()), move || {
            let _ = landed_tx.lock().unwrap().send(());
        });
        assert!(held.read([PathBuf::from("/slow")], Duration::from_millis(50)).is_empty());
        release.send(()).unwrap();
        landed.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(held.read([PathBuf::from("/fast")], Duration::from_secs(5)).contains_key(Path::new("/fast")));
        // Gone: asked about again, it's read afresh.
        release.send(()).unwrap();
        assert_eq!(held.read([PathBuf::from("/slow")], Duration::from_secs(5))[Path::new("/slow")], "read /slow");
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }
}
