//! The file watcher and what it brings: which folders are watched (`watch`), the paths that
//! never matter (`is_noise`), and a batch of changed paths sorted into what each calls for
//! (`apply_fs`): one of Codealong's own files read again, a harness's records handed to its
//! reader, git in the worktree a path is in (`classify_repo_path`, with what the batch has found
//! out so far in `FsBatch`), or the repos read again. And what's done when the watcher dropped
//! events (`on_dropped`).

use super::*;

/// What one batch from the file watcher has found out so far (`Engine::apply_fs`). A build or an
/// install writes thousands of files in a few folders, and each was compared with every lane and
/// matched against every folder above it in its worktree's gitignore: up to 20 s for 30,000
/// paths, with the engine hearing nothing else meanwhile (2026-10-12). Within a batch each
/// folder's lane and gitignore are looked at once, and each lane's folder.
struct FsBatch {
    seeds: board::SeedIndex,
    /// What's known of each folder paths were in.
    dirs: HashMap<PathBuf, Folder>,
    /// Whether each lane's folder is there, by seed.
    lanes_there: HashMap<usize, bool>,
    /// Whether each other folder looked for is there.
    present: HashMap<PathBuf, bool>,
    /// The lanes git will run in for this batch.
    changed: HashSet<usize>,
}

#[derive(Clone, Copy)]
struct Folder {
    /// The lane of what's in it (`SeedIndex::holding`).
    lane: Option<usize>,
    /// Whether the lane's gitignore ignores it or a folder above it, once asked.
    ignored: Option<bool>,
}

impl FsBatch {
    fn new(seeds: &[LaneSeed]) -> FsBatch {
        FsBatch { seeds: board::SeedIndex::new(seeds), dirs: HashMap::new(), lanes_there: HashMap::new(), present: HashMap::new(), changed: HashSet::new() }
    }

    /// The seed `p` is in (`board::SeedIndex::best`).
    fn lane_of(&mut self, p: &Path) -> Option<usize> {
        if let Some(i) = self.seeds.own(p) {
            return Some(i);
        }
        match p.parent() {
            Some(dir) => self.folder(dir).lane,
            None => self.seeds.best(p),
        }
    }

    fn folder(&mut self, dir: &Path) -> &mut Folder {
        if !self.dirs.contains_key(dir) {
            let lane = self.seeds.holding(dir);
            self.dirs.insert(dir.to_path_buf(), Folder { lane, ignored: None });
        }
        self.dirs.get_mut(dir).expect("just put there")
    }

    fn lane_there(&mut self, lane: usize, folder: &Path) -> bool {
        *self.lanes_there.entry(lane).or_insert_with(|| folder.exists())
    }

    fn exists(&mut self, p: &Path) -> bool {
        if let Some(&there) = self.present.get(p) {
            return there;
        }
        let there = p.exists();
        self.present.insert(p.to_path_buf(), there);
        there
    }

    /// `gi.matched_path_or_any_parents(p, p.is_dir()).is_ignore()` for `p` in the lane whose
    /// folder is `root` and whose gitignore is `gi`: the path's own rule, else its folder's, kept
    /// for the next path there.
    fn ignored(&mut self, gi: &Gitignore, root: &Path, p: &Path) -> bool {
        let Some(dir) = p.parent().filter(|_| p != root) else {
            return gi.matched_path_or_any_parents(p, p.is_dir()).is_ignore();
        };
        // Whether it's a folder is asked of the disk only when a rule tells files and folders
        // apart (`build/`), and one says something different of each.
        let said = |m: ignore::Match<&ignore::gitignore::Glob>| (!m.is_none()).then(|| m.is_ignore());
        let (as_file, as_folder) = (said(gi.matched(p, false)), said(gi.matched(p, true)));
        let own = if as_file == as_folder || !p.is_dir() { as_file } else { as_folder };
        if let Some(ignored) = own {
            return ignored;
        }
        let folder = self.folder(dir);
        *folder.ignored.get_or_insert_with(|| gi.matched_path_or_any_parents(dir, true).is_ignore())
    }
}

enum RepoEvent {
    Lane(PathBuf),
    Checkout(PathBuf),
    Worktrees,
    Ignore,
}

impl Engine {
    /// The watcher dropped events under `roots`, the folders it watches, so what changed there
    /// isn't known; FSEvents says so for every folder of the stream at once. Git runs as often as
    /// `Drops` allows in the worktrees each holds and those of the checkout it is, whose index
    /// and HEAD are in its `.git`. What isn't a repo's folder (a harness's records, the support
    /// folder) is handed back, to be taken as a change as it always was, and so is a worktree
    /// that's gone, deleted while its own events were dropped: a path of its in the batch reads
    /// the repos again (`classify_repo_path`).
    pub(super) fn on_dropped(&mut self, roots: Vec<PathBuf>) -> Vec<PathBuf> {
        if roots.is_empty() {
            return roots;
        }
        let roots: BTreeSet<PathBuf> = roots.into_iter().collect();
        let targets = self.git_targets();
        let mut lanes = BTreeSet::new();
        let mut rest = Vec::new();
        for root in &roots {
            lanes.extend(targets.iter().filter(|p| p.starts_with(root) || self.lane_checkout.get(*p) == Some(root)).cloned());
            if !targets.contains(root) && !self.checkouts.iter().any(|(_, c)| &c.root == root) {
                rest.push(root.clone());
            }
        }
        let (lanes, gone): (BTreeSet<PathBuf>, BTreeSet<PathBuf>) = lanes.into_iter().partition(|p| p.exists());
        rest.extend(gone);
        let run = self.drops.on_dropped(lanes.iter().cloned(), Instant::now());
        if perf::enabled() {
            perf::log(&format!("watcher dropped events in {} folders: git now in {} of their {} worktrees", roots.len(), run.len(), lanes.len()));
        }
        for p in run {
            self.queue_git(p);
        }
        rest
    }

    fn classify_repo_path(&self, p: &Path, batch: &mut FsBatch) -> RepoEvent {
        for (_, c) in &self.checkouts {
            if let Ok(rel) = p.strip_prefix(&c.git_dir) {
                let mut comps = rel.components().map(|c| c.as_os_str().to_string_lossy().into_owned());
                return match comps.next().as_deref() {
                    Some("worktrees") => match comps.next() {
                        Some(name) => {
                            let admin = c.git_dir.join("worktrees").join(&name);
                            match self.gitdir_to_lane.get(&admin) {
                                // `git worktree remove` deletes the worktree's folder, then its
                                // folder here: a lane with either gone is gone, and only a rescan
                                // takes it off the board.
                                Some(lane) if batch.exists(&admin) && batch.exists(lane) => RepoEvent::Lane(lane.clone()),
                                _ => RepoEvent::Worktrees,
                            }
                        }
                        None => RepoEvent::Worktrees,
                    },
                    Some("refs") | Some("packed-refs") | Some("FETCH_HEAD") => RepoEvent::Checkout(c.root.clone()),
                    Some("HEAD") | Some("index") | Some("ORIG_HEAD") | Some("MERGE_HEAD") => RepoEvent::Lane(c.root.clone()),
                    _ => RepoEvent::Ignore,
                };
            }
        }
        let Some(i) = batch.lane_of(p) else { return RepoEvent::Ignore };
        let seed = &self.seeds[i];
        if seed.elsewhere || seed.origin == Origin::Folder {
            return RepoEvent::Ignore;
        }
        // A worktree deleted without git (`rm -rf`) leaves its folder in `.git/worktrees`, where
        // nothing changes: its own files going are the only sign.
        if !batch.lane_there(i, &seed.path) {
            return RepoEvent::Worktrees;
        }
        // Git runs there already, whatever else changed in it.
        if batch.changed.contains(&i) {
            return RepoEvent::Lane(seed.path.clone());
        }
        if p.components().any(|c| c.as_os_str() == "node_modules" || c.as_os_str() == ".git") {
            return RepoEvent::Ignore;
        }
        if let Some(gi) = self.ignores.get(&seed.path) {
            if batch.ignored(gi, &seed.path, p) {
                return RepoEvent::Ignore;
            }
        }
        batch.changed.insert(i);
        RepoEvent::Lane(seed.path.clone())
    }

    pub(super) fn apply_fs(&mut self, mut paths: Vec<PathBuf>) {
        paths.sort();
        paths.dedup();
        let config_path = config::config_path();
        let todos_path = todos::path();
        let state_path = crate::state::path();
        // Each harness's own files, by reader and account, in the order they're read.
        let accounts: Vec<Vec<AccountCfg>> = self.readers.iter().map(|r| harness::accounts_of(&self.cfg, r.harness())).collect();
        let mut owned: BTreeMap<(usize, usize), Vec<PathBuf>> = BTreeMap::new();
        let mut lanes = BTreeSet::new();
        let mut reload_repos = false;
        let mut reload_config = false;
        let (pr_links_path, reviews_path, previews_path, reusable_path) = (prlinks::path(), autoreview::path(), previews::path(), reusable::path());
        let mut batch = FsBatch::new(&self.seeds);
        let mut moved = HashSet::new();
        'paths: for p in paths {
            if p == config_path {
                reload_config = true;
                continue;
            }
            if p == todos_path {
                self.reload_todos();
                continue;
            }
            if p == state_path {
                // The engine's own writes come back here; only a file that didn't read is read
                // again, now that it's been fixed or deleted.
                if self.state_unreadable {
                    if state_path.exists() {
                        self.load_state();
                    } else {
                        // Deleted: what's in memory is the state from here on.
                        self.state_unreadable = false;
                        self.set_trouble(Trouble::State, None);
                    }
                    if !self.state_unreadable {
                        self.remember_accounts();
                        self.save_state();
                        // What's archived and pinned decides what the harnesses' readers keep.
                        self.scan_sessions();
                        self.scan_pr_marks(false);
                    }
                    self.mark_dirty("state.json");
                }
                continue;
            }
            if p == pr_links_path {
                self.load_pr_links();
                self.mark_dirty("PR links file");
                continue;
            }
            if p == reviews_path {
                self.load_reviews();
                self.mark_dirty("reviews file");
                continue;
            }
            if p == crate::schedules::path() || p == crate::schedules::runs_path() {
                self.load_schedules();
                self.mark_dirty("schedules file");
                continue;
            }
            if p == previews_path {
                self.load_previews();
                self.mark_dirty("previews file");
                continue;
            }
            if p == reusable_path {
                self.load_reusable();
                self.mark_dirty("reusable file");
                continue;
            }
            for (i, r) in self.readers.iter().enumerate() {
                if let Some(j) = accounts[i].iter().position(|a| r.owns(a, &p)) {
                    owned.entry((i, j)).or_default().push(p);
                    continue 'paths;
                }
            }
            match self.classify_repo_path(&p, &mut batch) {
                RepoEvent::Lane(l) => {
                    self.drops.on_change(l.clone(), Instant::now());
                    lanes.insert(l);
                }
                // A fetch writes a file for each ref it moves: the checkout is looked at once.
                RepoEvent::Checkout(root) if !moved.insert(root.clone()) => {}
                RepoEvent::Checkout(root) => {
                    lanes.extend(self.seeds.iter().filter(|s| self.lane_checkout.get(&s.path) == Some(&root)).map(|s| s.path.clone()));
                    // A fetch may have moved the branch a team file is read from.
                    if let Some((owner, name)) = self.checkouts.iter().find(|(_, c)| c.root == root).and_then(|(_, c)| c.github.clone()) {
                        self.team_files.on_refs_moved(&format!("{owner}/{name}"));
                    }
                }
                RepoEvent::Worktrees => reload_repos = true,
                RepoEvent::Ignore => {}
            }
        }

        if reload_config {
            self.reload();
            return;
        }
        let look = Self::look(&self.seen, &self.processes);
        let mut all = harness::Applied::default();
        let mut asked = Vec::new();
        for ((i, j), paths) in owned {
            let applied = self.readers[i].apply(&accounts[i][j], &paths, &look);
            if applied.dirty && perf::enabled() {
                asked.push(format!("files {}", accounts[i][j].name));
            }
            all.and(applied);
        }
        for a in asked {
            self.asked(|| a);
        }
        let added = self.take(all);
        if reload_repos {
            self.rescan_repos();
        } else if added {
            self.place_sessions();
        }
        for l in lanes {
            self.queue_git(l);
        }
    }

    pub(super) fn watch(&mut self) {
        let tx = self.tx.clone();
        let handler = move |res: notify::Result<notify::Event>| {
            if let Ok(ev) = res {
                if matches!(ev.kind, EventKind::Access(_)) {
                    return;
                }
                // notify says why FSEvents asks for a look again in the event's info: events
                // dropped (`rescan: user dropped`, `rescan: kernel dropped`), which it says for
                // every folder watched, or events folded into their folder, which say where.
                let dropped = ev.need_rescan() && ev.info().is_some_and(|i| i.ends_with("dropped"));
                let paths: Vec<PathBuf> = ev.paths.into_iter().filter(|p| !is_noise(p)).collect();
                if !paths.is_empty() {
                    let _ = tx.send(if dropped { Msg::Dropped(paths) } else { Msg::Fs(paths) });
                }
            }
        };
        let mut watcher = match notify::recommended_watcher(handler) {
            Ok(w) => w,
            Err(e) => {
                self.set_trouble(Trouble::Watch, Some(format!("Live updates are off: {e}")));
                return;
            }
        };
        let mut paths: Vec<(PathBuf, RecursiveMode)> = Vec::new();
        self.missing_homes = self.cfg.accounts.iter().map(|a| a.home_path()).filter(|h| !h.exists()).collect();
        let _ = std::fs::create_dir_all(config::support_dir());
        paths.push((config::support_dir(), RecursiveMode::NonRecursive));
        for r in &self.readers {
            for a in harness::accounts_of(&self.cfg, r.harness()) {
                paths.extend(r.watch(&a).into_iter().map(|(p, deep)| (p, if deep { RecursiveMode::Recursive } else { RecursiveMode::NonRecursive })));
            }
        }
        let roots: Vec<PathBuf> = self.checkouts.iter().map(|(_, c)| c.root.clone()).collect();
        for r in &roots {
            paths.push((r.clone(), RecursiveMode::Recursive));
        }
        for s in &self.seeds {
            if !s.elsewhere && s.origin != Origin::Folder && !roots.iter().any(|r| s.path.starts_with(r)) {
                paths.push((s.path.clone(), RecursiveMode::Recursive));
            }
        }
        // Added together and started once: on macOS each `watch` stops the watcher's stream and
        // starts it again with every path so far, a thread and a stream for each path.
        let mut failed = 0;
        let mut adding = watcher.paths_mut();
        for (p, mode) in paths {
            if p.exists() && adding.add(&p, mode).is_err() {
                failed += 1;
            }
        }
        // What a path's watch came to, `add` has said.
        let _ = adding.commit();
        self.set_trouble(Trouble::Watch, (failed > 0).then(|| format!("Couldn't watch {failed} folders; their lanes update every 10 seconds.")));
        self.watcher = Some(watcher);
    }
}

/// Paths that never matter, dropped before they reach the engine: databases and logs that
/// apps write constantly, dependency folders, git's object store.
pub(super) fn is_noise(p: &Path) -> bool {
    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or_default();
    // A harness's database runs in WAL mode, so while a session runs only its `-wal` file
    // changes: that change is how Codealong hears of it (`Adapter::databases`).
    if harness::all().flat_map(|a| a.databases()).any(|db| name.strip_suffix("-wal") == Some(*db)) {
        return false;
    }
    if name.ends_with("-wal") || name.ends_with("-shm") || name.ends_with("-journal") || name.ends_with(".sqlite") || name.ends_with(".log") || name.ends_with(".lock") || name == ".DS_Store" {
        return true;
    }
    p.components().any(|c| {
        let c = c.as_os_str();
        c == "node_modules" || c == "objects" || c == "target" || c == ".nx" || c == ".turbo" || c == ".next"
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_harness_database_log_is_heard_and_other_logs_are_not() {
        use super::*;
        assert!(!is_noise(Path::new("/Users/me/.local/share/opencode/opencode.db-wal")));
        assert!(!is_noise(Path::new("/Users/me/.gemini/antigravity-cli/conversation_summaries.db-wal")));
        assert!(is_noise(Path::new("/Users/me/.local/share/opencode/opencode.db-shm")));
        assert!(is_noise(Path::new("/Users/me/projects/app/data/cache.db-wal")));
    }

    /// A batch keeps what it learned of each folder, and still says what the gitignore says of
    /// each path in it, whichever paths came first: rules for files, for folders only, and ones
    /// that take a path back.
    #[test]
    fn a_batch_ignores_what_the_gitignore_ignores() {
        use super::*;
        let dir = crate::fixtures::scratch("batch-ignores");
        let root = dir.join("repo");
        for dir in ["dist/assets", "build", "src/build", "logs/keep", "src/deep/er"] {
            std::fs::create_dir_all(root.join(dir)).unwrap();
        }
        let mut gi = GitignoreBuilder::new(&root);
        for rule in ["dist", "build/", "*.log", "!keep.log", "logs/*", "!logs/keep", "/src/deep/er/*.tmp"] {
            gi.add_line(None, rule).unwrap();
        }
        let gi = gi.build().unwrap();
        let paths: Vec<PathBuf> = [
            "dist/app.js",
            "dist/assets/a.css",
            "dist",
            "build",
            "build/out.o",
            "src/build",
            "src/build/x",
            "src/main.rs",
            "src/app.log",
            "src/keep.log",
            "logs/today",
            "logs/keep",
            "logs/keep/a.log",
            "logs/keep/keep.log",
            "logs/keep/notes.txt",
            "src/deep/er/x.tmp",
            "src/deep/er/x.rs",
            "src/deep/x.tmp",
        ]
        .iter()
        .map(|p| root.join(p))
        .chain([root.clone()])
        .collect();
        let want: Vec<bool> = paths.iter().map(|p| gi.matched_path_or_any_parents(p, p.is_dir()).is_ignore()).collect();
        for order in [paths.clone(), paths.iter().rev().cloned().collect()] {
            let mut batch = FsBatch::new(&[]);
            let got: HashMap<&PathBuf, bool> = order.iter().map(|p| (p, batch.ignored(&gi, &root, p))).collect();
            for (p, want) in paths.iter().zip(&want) {
                assert_eq!(got[p], *want, "{}", p.display());
            }
        }
        assert!(want.iter().any(|&w| w) && want.iter().any(|&w| !w));
    }

    /// A batch finds each path's lane once per folder, and the lane is the one `best_seed` finds,
    /// worktrees inside worktrees and the worktrees' own folders included.
    #[test]
    fn a_batch_finds_the_lane_best_seed_finds() {
        use super::*;
        let seed = |path: &str, exact: bool| LaneSeed { exact, ..folder_seed(PathBuf::from(path), "P", "repo", false) };
        let seeds = vec![seed("/r/app", false), seed("/r/app/.claude/worktrees/fix", false), seed("/Users/me", true), seed("/r/app/.claude/worktrees/fix", false)];
        let paths = [
            "/r/app/src/a.rs",
            "/r/app/.claude/worktrees/fix",
            "/r/app/.claude/worktrees/fix/src/a.rs",
            "/r/app/.claude/worktrees/fixed/src/a.rs",
            "/r/app/.claude/worktrees",
            "/r/app/src/b.rs",
            "/Users/me",
            "/Users/me/notes.md",
            "/r/apple/x",
            "/",
        ];
        let mut batch = FsBatch::new(&seeds);
        for p in paths.iter().chain(paths.iter().rev()) {
            assert_eq!(batch.lane_of(Path::new(p)), board::best_seed(&seeds, Path::new(p)), "{p}");
        }
    }

    #[test]
    fn a_removed_worktree_rescans_the_repos() {
        use super::*;

        let dir = crate::fixtures::scratch("gone-worktree");
        let root = dir.join("repo");
        let git_dir = root.join(".git");
        let admin = git_dir.join("worktrees/topic");
        let wt = dir.join("repo-topic");
        std::fs::create_dir_all(&admin).unwrap();
        std::fs::create_dir_all(&wt).unwrap();
        let (tx, _rx) = channel();
        let mut engine = Engine::new(Box::new(|_| {}), tx, Arc::new(RwLock::new(Board::default())));
        let checkout = repos::Checkout { root: root.clone(), git_dir: git_dir.clone(), name: "repo".into(), identity: None, github: None, github_owners: Vec::new() };
        engine.checkouts = vec![("P".into(), checkout)];
        engine.gitdir_to_lane.insert(admin.clone(), wt.clone());
        let seed = |path: &Path, origin: Origin| LaneSeed { origin, ..folder_seed(path.to_path_buf(), "P", "repo", false) };
        engine.seeds = vec![seed(&root, Origin::Main), seed(&wt, Origin::Other)];
        let is_lane = |e: RepoEvent, lane: &Path| matches!(e, RepoEvent::Lane(l) if l == lane);

        // While it's there, a change in it or its folder in `.git/worktrees` runs git in it.
        assert!(is_lane(engine.classify_repo_path(&wt.join("src/main.rs"), &mut FsBatch::new(&engine.seeds)), &wt));
        assert!(is_lane(engine.classify_repo_path(&admin.join("HEAD"), &mut FsBatch::new(&engine.seeds)), &wt));
        // `git worktree remove` deletes both.
        std::fs::remove_dir_all(&wt).unwrap();
        assert!(matches!(engine.classify_repo_path(&wt.join("src/main.rs"), &mut FsBatch::new(&engine.seeds)), RepoEvent::Worktrees), "its own files going");
        assert!(matches!(engine.classify_repo_path(&admin.join("HEAD"), &mut FsBatch::new(&engine.seeds)), RepoEvent::Worktrees), "its worktree gone");
        std::fs::create_dir_all(&wt).unwrap();
        std::fs::remove_dir_all(&admin).unwrap();
        assert!(matches!(engine.classify_repo_path(&admin, &mut FsBatch::new(&engine.seeds)), RepoEvent::Worktrees), "its folder in .git gone");
        // The main checkout's own changes are still its own.
        assert!(is_lane(engine.classify_repo_path(&root.join("src/lib.rs"), &mut FsBatch::new(&engine.seeds)), &root));
    }

    /// Events the watcher dropped run git where the folder's worktrees are, the checkout's own
    /// outside it too, and no more often than `Drops` allows; a folder that's no repo's is taken
    /// as it always was, and so is a worktree that's gone, which has the repos read again.
    #[test]
    fn dropped_events_run_git_in_the_folders_worktrees_now_and_then() {
        use super::*;
        let (tx, _rx) = channel();
        let mut engine = Engine::new(Box::new(|_| {}), tx, Arc::new(RwLock::new(Board::default())));
        // No worker takes the jobs: each run stays running, to be counted.
        let (jobs, _held) = channel::<GitJob>();
        engine.git_jobs = jobs;
        let work = std::env::temp_dir().join(format!("codealong-dropped-{}", std::process::id()));
        let at = |p: &str| work.join(p);
        let (root, nested, linked, other, gone) = (at("app"), at("app/.claude/worktrees/a"), at("app-fix"), at("other"), at("app/.claude/worktrees/b"));
        for p in [&nested, &linked, &other] {
            std::fs::create_dir_all(p).unwrap();
        }
        let repo = |p: &PathBuf, origin: Origin| LaneSeed { origin, ..folder_seed(p.clone(), "P", "repo", false) };
        engine.seeds = vec![repo(&root, Origin::Main), repo(&nested, Origin::ClaudeApp), repo(&gone, Origin::ClaudeApp), repo(&linked, Origin::Main), repo(&other, Origin::Main)];
        for p in [&root, &nested, &gone, &linked] {
            engine.lane_checkout.insert(p.clone(), root.clone());
        }
        engine.lane_checkout.insert(other.clone(), other.clone());

        let rest = engine.on_dropped(vec![root.clone(), PathBuf::from("/records"), root.clone()]);
        assert_eq!(rest, vec![PathBuf::from("/records"), gone.clone()], "a harness's folder and a worktree that's gone are handed back");
        let running: BTreeSet<&PathBuf> = [&root, &nested, &gone, &linked, &other].into_iter().filter(|p| engine.git_queue.request((*p).clone(), Instant::now()) == Verdict::Running).collect();
        assert_eq!(running, [&root, &nested, &linked].into(), "not the other repo, nor where nothing is");

        // A build overflows the watcher again: nothing more yet.
        engine.git_queue = GitQueue::default();
        assert_eq!(engine.on_dropped(vec![root.clone()]), vec![gone]);
        assert_eq!(engine.git_queue.running(), 0);
        assert!(engine.drops.next_due().is_some(), "a run is owed");
        let _ = std::fs::remove_dir_all(&work);
    }

    /// A worktree deleted while the watcher dropped its events leaves the board: the drop hands it
    /// to the batch, which reads the repos again for it.
    #[test]
    fn a_worktree_gone_while_events_were_dropped_has_the_repos_read_again() {
        use super::*;
        let (tx, _rx) = channel();
        let mut engine = Engine::new(Box::new(|_| {}), tx, Arc::new(RwLock::new(Board::default())));
        let gone = PathBuf::from("/nowhere/codealong-gone-worktree");
        engine.seeds = vec![LaneSeed { origin: Origin::Main, ..folder_seed(gone.clone(), "P", "repo", false) }];
        let rest = engine.on_dropped(vec![gone.clone()]);
        assert_eq!(rest, vec![gone.clone()]);
        let mut batch = FsBatch::new(&engine.seeds);
        assert!(matches!(engine.classify_repo_path(&gone, &mut batch), RepoEvent::Worktrees));
    }
}
