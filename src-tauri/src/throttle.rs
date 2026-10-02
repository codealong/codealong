//! When git runs, and when its results reach the board. Git is the engine's biggest cost and
//! starts during load by construction: agents editing files is what asks for it. So runs in
//! one worktree are spaced out and coalesced, and results alone republish the board in
//! batches, not once per worktree. Pure, so the rules can be tested with made-up clocks.
//!
//! Also how work is sent to the background so that whoever waits on it always hears it's over
//! (`background`).

use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::time::{Duration, Instant};

/// The least time between two git runs in the same worktree.
pub const GIT_SPACING: Duration = Duration::from_secs(2);
/// Git results alone republish the board this often. A burst of them (launch, a fetch that
/// moves every branch) becomes a few pushes instead of one per worktree.
pub const GIT_PUBLISH_EVERY: Duration = Duration::from_secs(2);

#[derive(Debug, PartialEq)]
pub enum Verdict {
    /// Start it now; it's marked running.
    Run,
    /// Held until spacing allows; `due` hands it out then.
    Later,
    /// One is running; it runs again when that one finishes.
    Running,
}

/// Which worktrees git is running in, which are held, and when each last finished.
pub struct GitQueue<K> {
    running: HashSet<K>,
    again: HashSet<K>,
    waiting: HashMap<K, Instant>,
    last: HashMap<K, Instant>,
}

impl<K> Default for GitQueue<K> {
    fn default() -> Self {
        GitQueue { running: HashSet::new(), again: HashSet::new(), waiting: HashMap::new(), last: HashMap::new() }
    }
}

impl<K: Eq + Hash + Clone> GitQueue<K> {
    pub fn request(&mut self, key: K, now: Instant) -> Verdict {
        if self.running.contains(&key) {
            self.again.insert(key);
            return Verdict::Running;
        }
        if let Some(last) = self.last.get(&key) {
            if now.duration_since(*last) < GIT_SPACING {
                self.waiting.entry(key).or_insert(*last + GIT_SPACING);
                return Verdict::Later;
            }
        }
        self.waiting.remove(&key);
        self.running.insert(key);
        Verdict::Run
    }

    /// A run finished. True when changes came in meanwhile, so it should be requested again.
    pub fn done(&mut self, key: &K, now: Instant) -> bool {
        self.running.remove(key);
        self.last.insert(key.clone(), now);
        self.again.remove(key)
    }

    /// Held runs whose time has come, now marked running.
    pub fn due(&mut self, now: Instant) -> Vec<K> {
        let due: Vec<K> = self.waiting.iter().filter(|(_, at)| **at <= now).map(|(k, _)| k.clone()).collect();
        for k in &due {
            self.waiting.remove(k);
            self.running.insert(k.clone());
        }
        due
    }

    /// When the earliest held run is due, for the loop's wait.
    pub fn next_due(&self) -> Option<Instant> {
        self.waiting.values().min().copied()
    }

    pub fn running(&self) -> usize {
        self.running.len()
    }
}

/// Whether to publish now. Changes to what the board shows go out after `publish_every`; git
/// results alone wait `GIT_PUBLISH_EVERY`. The first publish goes at once either way.
pub fn publish_now(dirty: bool, dirty_git: bool, since_last: Option<Duration>, publish_every: Duration) -> bool {
    match since_last {
        None => dirty || dirty_git,
        Some(d) => (dirty && d >= publish_every) || (dirty_git && d >= GIT_PUBLISH_EVERY),
    }
}

/// Runs `work` on a thread of its own, named `name`, and hands what it returned to `ended`
/// once it's over, however that comes: None when the work panicked, and when its thread
/// couldn't be started. Whoever marks work as on its way takes the mark off in `ended` (by a
/// message to itself, usually). A mark that only the work's own last message took off would
/// stay for good after a panic, and what waits on it would never run again. `ended` runs on
/// the work's thread, or on the caller's when that thread never started.
pub fn background<T: Send + 'static>(name: &str, work: impl FnOnce() -> T + Send + 'static, ended: impl FnOnce(Option<T>) + Send + 'static) {
    let report = Report { result: None, ended: Some(ended) };
    let spawned = std::thread::Builder::new().name(name.to_string()).spawn(move || {
        let mut report = report;
        report.result = Some(work());
    });
    // A thread that couldn't start drops what it was to run, and the report in it: `ended` has
    // been told already.
    if let Err(e) = spawned {
        eprintln!("Couldn't start {name}: {e}");
    }
}

/// What `background` work came to, said when this is dropped: at the end of the work, as a
/// panic unwinds through it, or with a closure that never ran.
struct Report<T, F: FnOnce(Option<T>)> {
    result: Option<T>,
    ended: Option<F>,
}

impl<T, F: FnOnce(Option<T>)> Drop for Report<T, F> {
    fn drop(&mut self) {
        if let Some(ended) = self.ended.take() {
            ended(self.result.take());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clock() -> Instant {
        Instant::now()
    }

    #[test]
    fn a_first_request_runs_and_a_second_waits_for_it() {
        let mut q = GitQueue::default();
        let t = clock();
        assert_eq!(q.request("a", t), Verdict::Run);
        assert_eq!(q.request("a", t), Verdict::Running);
        assert_eq!(q.running(), 1);
        assert!(q.done(&"a", t + Duration::from_millis(500)), "changes came while it ran");
        assert_eq!(q.running(), 0);
    }

    #[test]
    fn runs_in_one_worktree_are_spaced_out() {
        let mut q = GitQueue::default();
        let t = clock();
        assert_eq!(q.request("a", t), Verdict::Run);
        assert!(!q.done(&"a", t + Duration::from_secs(1)));
        assert_eq!(q.request("a", t + Duration::from_millis(1500)), Verdict::Later);
        assert_eq!(q.request("a", t + Duration::from_millis(1600)), Verdict::Later, "asking twice holds one run");
        assert_eq!(q.next_due(), Some(t + Duration::from_secs(3)));
        assert!(q.due(t + Duration::from_millis(2900)).is_empty());
        assert_eq!(q.due(t + Duration::from_secs(3)), vec!["a"]);
        assert_eq!(q.request("a", t + Duration::from_secs(3)), Verdict::Running);
        assert_eq!(q.next_due(), None);
    }

    #[test]
    fn a_quiet_worktree_runs_at_once() {
        let mut q = GitQueue::default();
        let t = clock();
        assert_eq!(q.request("a", t), Verdict::Run);
        q.done(&"a", t);
        assert_eq!(q.request("a", t + GIT_SPACING), Verdict::Run);
        assert_eq!(q.request("b", t), Verdict::Run, "other worktrees aren't held by this one");
    }

    #[test]
    fn git_results_alone_publish_in_batches() {
        let every = Duration::from_millis(300);
        assert!(publish_now(false, true, None, every), "the first board goes out at once");
        assert!(!publish_now(false, true, Some(Duration::from_millis(500)), every));
        assert!(publish_now(false, true, Some(GIT_PUBLISH_EVERY), every));
        assert!(publish_now(true, false, Some(every), every), "a session change doesn't wait");
        assert!(!publish_now(true, false, Some(Duration::from_millis(100)), every));
        assert!(!publish_now(false, false, Some(Duration::from_secs(9)), every));
    }

    #[test]
    fn background_work_says_it_is_over_however_it_ends() {
        use std::sync::mpsc::{channel, Sender};
        let (tx, rx) = channel::<Option<u32>>();
        let said = |tx: &Sender<Option<u32>>| {
            let tx = tx.clone();
            move |result| {
                let _ = tx.send(result);
            }
        };
        let wait = Duration::from_secs(30);

        background("test-returns", || 7, said(&tx));
        assert_eq!(rx.recv_timeout(wait), Ok(Some(7)));
        // A panic unwinds through the report. (`resume_unwind` panics without the hook's lines
        // on stderr.)
        background("test-panics", || -> u32 { std::panic::resume_unwind(Box::new("fell over")) }, said(&tx));
        assert_eq!(rx.recv_timeout(wait), Ok(None));
        // A thread that can't start drops the closure it was to run, and the report in it.
        drop(Report { result: None::<u32>, ended: Some(said(&tx)) });
        assert_eq!(rx.recv_timeout(wait), Ok(None));
        // Each said so once.
        drop(tx);
        assert!(rx.recv_timeout(wait).is_err());
    }
}
