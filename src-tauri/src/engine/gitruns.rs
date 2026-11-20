//! Git in the worktrees: the workers that run it (`spawn_git_workers`, each run answered even
//! when it falls over), asking for a run where something changed (`queue_git`, spaced per
//! worktree by throttle.rs `GitQueue`), and taking in what comes back (`on_git`).

use super::*;

const GIT_WORKERS: usize = 4;

impl Engine {
    /// Asks for git in a worktree. Runs at once when it's been quiet, otherwise once the
    /// current run ends or the spacing allows, whichever is later.
    pub(super) fn queue_git(&mut self, path: PathBuf) {
        if self.held_worktrees.contains(&path) {
            return;
        }
        if let Verdict::Run = self.git_queue.request(path.clone(), Instant::now()) {
            self.send_git(path);
        }
    }

    pub(super) fn send_git(&mut self, path: PathBuf) {
        let base = self.base_for(&path);
        // With no worker left to take it, nothing is running there.
        if let Err(unsent) = self.git_jobs.send((path, base)) {
            self.git_queue.done(&unsent.0 .0, Instant::now());
        }
    }

    /// Every worktree, the ones with the latest activity first: with many worktrees and a busy
    /// Mac the first results take a while, and those are the ones on screen.
    pub(super) fn queue_git_all(&mut self) {
        let activity: HashMap<String, (bool, i64)> = self
            .board
            .read()
            .map(|b| b.lanes.iter().map(|l| (l.id.clone(), (l.sessions.iter().any(|s| s.live), l.sessions.iter().map(|s| s.last_activity).max().unwrap_or(0)))).collect())
            .unwrap_or_default();
        let mut targets = self.git_targets();
        targets.sort_by_key(|p| std::cmp::Reverse(activity.get(&p.to_string_lossy().to_string()).copied().unwrap_or((false, 0))));
        for p in targets {
            self.queue_git(p);
        }
    }

    /// Each worktree's HEAD as git last read it (`git::last_head`), else as the worktree listing
    /// had it: a commit moves HEAD long before the listing is read again.
    pub(super) fn heads_now(&self) -> HashMap<PathBuf, String> {
        self.seeds.iter().filter_map(|s| Some((s.path.clone(), crate::git::last_head(&s.path).or_else(|| s.head.clone())?))).collect()
    }

    pub(super) fn on_git(&mut self, path: PathBuf, info: Option<GitInfo>) {
        self.perf.git_jobs += 1;
        // A run that fell over leaves what was known of the worktree as it is, and one that found
        // it as it was changes nothing to publish: most runs, after a fetch or a build.
        if let Some(info) = info.filter(|_| self.seeds.iter().any(|s| s.path == path)) {
            if self.git.get(&path) != Some(&info) {
                self.git.insert(path.clone(), info);
                self.dirty_git = true;
                self.asked(|| "git".into());
            }
        }
        if self.git_queue.done(&path, Instant::now()) {
            self.queue_git(path);
        }
    }
}

/// A worktree to run git in, and the branch its changes are counted from.
pub(super) type GitJob = (PathBuf, Option<String>);

pub(super) fn spawn_git_workers(back: Sender<Msg>) -> Sender<GitJob> {
    let (jobs, rx) = channel::<GitJob>();
    let rx = Arc::new(Mutex::new(rx));
    for i in 0..GIT_WORKERS {
        let rx = rx.clone();
        let back = back.clone();
        let _ = std::thread::Builder::new().name(format!("codealong-git-{i}")).spawn(move || {
            crate::os::system::utility_thread();
            let status = |path: &Path, base: Option<&str>| {
                let t = Instant::now();
                let info = git::status(path, base);
                if perf::enabled() {
                    perf::log(&format!("git status {} ms: {}", t.elapsed().as_millis(), path.display()));
                }
                info
            };
            git_worker(&rx, status, |path, info| back.send(Msg::GitDone(path, info)).is_ok());
        });
    }
    jobs
}

/// One git worker: runs the jobs as they come, until there are no more to come or nobody to
/// answer. Every job is answered, with none when its run panicked, and the worker goes on to
/// the next: the engine counts a worktree as running until it hears (throttle.rs), so one it
/// never heard of would get no git again, and a worker that died would take a quarter of the
/// git with it.
fn git_worker(jobs: &Mutex<Receiver<GitJob>>, run: impl Fn(&Path, Option<&str>) -> GitInfo, answer: impl Fn(PathBuf, Option<GitInfo>) -> bool) {
    loop {
        let job = match jobs.lock() {
            Ok(r) => r.recv(),
            Err(_) => return,
        };
        let Ok((path, base)) = job else { return };
        let info = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(&path, base.as_deref()))).ok();
        if !answer(path, info) {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    /// A git run that finds a worktree as it was changes nothing to publish.
    #[test]
    fn git_that_finds_nothing_new_asks_for_no_publish() {
        use super::*;
        let (tx, _rx) = channel();
        let mut engine = Engine::new(Box::new(|_| {}), tx, Arc::new(RwLock::new(Board::default())));
        let wt = PathBuf::from("/work/app");
        engine.seeds = vec![folder_seed(wt.clone(), "p", "app", false)];
        let info = GitInfo { ahead: 1, dirty: 2, ..GitInfo::default() };
        engine.on_git(wt.clone(), Some(info.clone()));
        assert!(engine.dirty_git, "the first word on it");
        engine.dirty_git = false;
        engine.on_git(wt.clone(), Some(info.clone()));
        assert!(!engine.dirty_git, "the same again");
        engine.on_git(wt.clone(), Some(GitInfo { dirty: 3, ..info }));
        assert!(engine.dirty_git, "a file changed");
        engine.dirty_git = false;
        engine.on_git(wt, None);
        assert!(!engine.dirty_git, "a run that fell over leaves it as it was");
    }

    /// Git isn't run in a worktree while a run there is out, so every run has to answer.
    #[test]
    fn a_git_run_that_falls_over_still_answers() {
        use super::*;
        let (jobs, rx) = channel::<GitJob>();
        let (answers, answered) = channel();
        for name in ["first", "bad", "next"] {
            jobs.send((PathBuf::from(name), None)).unwrap();
        }
        drop(jobs);
        let run = |path: &Path, _: Option<&str>| {
            if path == Path::new("bad") {
                // A panic, without the hook's lines on stderr.
                std::panic::resume_unwind(Box::new("git fell over"));
            }
            GitInfo::default()
        };
        // The worker returns once the jobs run out: it went on past the one that fell over.
        git_worker(&Mutex::new(rx), run, |path, info| answers.send((path, info.is_some())).is_ok());
        let all: Vec<(PathBuf, bool)> = answered.try_iter().collect();
        assert_eq!(all, [("first".into(), true), ("bad".into(), false), ("next".into(), true)]);

        // The engine has the worktree's run as over either way, and keeps what it knew of it.
        let (tx, _rx) = channel();
        let mut engine = Engine::new(Box::new(|_| {}), tx, Arc::new(RwLock::new(Board::default())));
        let wt = PathBuf::from("/x/wt");
        engine.seeds = vec![folder_seed(wt.clone(), "P", "repo", false)];
        assert_eq!(engine.git_queue.request(wt.clone(), Instant::now()), Verdict::Run);
        engine.on_git(wt.clone(), None);
        assert_eq!(engine.git_queue.running(), 0);
        assert!(!engine.git.contains_key(&wt) && !engine.dirty_git);
        engine.on_git(wt.clone(), Some(GitInfo::default()));
        assert!(engine.git.contains_key(&wt) && engine.dirty_git);

        // With no worker to take a job, nothing is left running either.
        let (gone, _) = channel::<GitJob>();
        engine.git_jobs = gone;
        engine.git_queue = GitQueue::default();
        engine.queue_git(wt.clone());
        assert_eq!(engine.git_queue.running(), 0);
    }
}
