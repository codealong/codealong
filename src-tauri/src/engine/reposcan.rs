//! The repo scan: the checkouts under the scan roots and their worktrees, as seeds for the
//! board's lanes (`load_repos`), with a lane for every folder a session ran in that no project
//! covers (`stray_seeds`). What it reads in folders it doesn't own goes through held.rs
//! (`RepoReads`), since macOS can hold such a read for a privacy prompt: a read that's held
//! leaves out only its own folder, and the repos are read again once it lands (`rescan_repos`).

use super::*;

/// How long it waits for a scan root's walk, which reads many folders (330 ms at that load): one
/// that came in late would leave the board without the root's repos for a moment.
const ROOT_WAIT: Duration = Duration::from_secs(10);

/// Everything the repo scan reads in folders it doesn't own, one kind of read each. macOS can
/// hold any of these reads for a privacy prompt, so each holds up only its own folder (held.rs).
pub(super) struct RepoReads {
    /// The checkouts under each scan root.
    roots: Held<Vec<Checkout>>,
    /// The worktree and main checkout each session's folder is in.
    folders: Held<Option<(PathBuf, PathBuf)>>,
    /// Checkouts outside the scan roots that sessions worked in.
    checkouts: Held<Option<Checkout>>,
    /// Each worktree's `.git` file and `.gitignore`.
    worktrees: Held<WorktreeFiles>,
}

impl RepoReads {
    pub(super) fn new(tx: &Sender<Msg>) -> RepoReads {
        let landed = || {
            let tx = tx.clone();
            move || {
                let _ = tx.send(Msg::Landed);
            }
        };
        RepoReads {
            roots: Held::new("scan root", |root| repos::discover(&[root.to_path_buf()], 3), landed()),
            folders: Held::new("session folder", repos::locate, landed()),
            checkouts: Held::new("checkout", repos::load_checkout, landed()),
            worktrees: Held::new("worktree", read_worktree, landed()),
        }
    }

    /// The folders whose reads have been out at least `wait`, in order.
    pub(super) fn out_for(&self, wait: Duration) -> Vec<PathBuf> {
        let mut out = self.roots.out_for(wait);
        out.extend(self.folders.out_for(wait));
        out.extend(self.checkouts.out_for(wait));
        out.extend(self.worktrees.out_for(wait));
        out.sort();
        out.dedup();
        out
    }
}

impl Engine {
    /// A new session may have started somewhere no lane covers yet: a chat, or a folder or repo
    /// outside your projects. Its folder is only looked up here, as reading it could wait on a
    /// privacy prompt; one whose read is held is looked at again once it lands. The repos are
    /// read again when one has.
    pub(super) fn place_sessions(&mut self) {
        let unplaced = || {
            let chats = chat_roots();
            let index = board::SeedIndex::new(&self.seeds);
            self.session_cwds().into_iter().any(|cwd| {
                let p = Path::new(&cwd);
                index.best(p).is_none() && (chat_root(&chats, p).is_some() || (!self.reads.folders.is_out(p) && p.is_dir()))
            })
        };
        if unplaced() {
            self.rescan_repos();
        }
    }

    /// The checkouts and their worktrees, as seeds for lanes. Files in folders the scan doesn't
    /// own are read through `self.reads`: one macOS holds for a privacy prompt leaves out only
    /// its own repo or session folder, or shows its worktree as `git worktree list` gives it, and
    /// is filled in once the read lands.
    pub(super) fn load_repos(&mut self) {
        let h = home();
        let roots: Vec<PathBuf> = self.cfg.scan.iter().map(|s| expand(s)).collect();
        let mut walked = self.reads.roots.read(roots.clone(), ROOT_WAIT);
        let mut found: Vec<Checkout> = roots.iter().filter_map(|r| walked.remove(r)).flatten().collect();
        found.sort_by(|a, b| a.root.cmp(&b.root));
        found.dedup_by(|a, b| a.root == b.root);
        // Codealong's own source among them, in a project or not (improve.rs): one small file
        // looked for per checkout, each scan, so a clone whose files land after its `.git` is
        // found on the next.
        self.codealong_sources = found.iter().filter(|c| crate::improve::is_source(&c.root)).map(|c| (c.root.clone(), c.identity.clone())).collect();
        let known: HashSet<PathBuf> = found.iter().map(|c| c.root.clone()).collect();
        let cwds = self.session_cwds();
        let chats = chat_roots();
        // A chat's folder is never one of your repos, and the Codex app keeps its chats in
        // Documents, where a read can wait on a privacy prompt: they aren't looked up.
        let located = self.reads.folders.read(cwds.iter().map(PathBuf::from).filter(|p| chat_root(&chats, p).is_none()), HELD_WAIT);
        let extra: BTreeSet<PathBuf> = located.values().flatten().map(|(_, main)| main.clone()).filter(|main| !known.contains(main)).collect();
        let mut loaded = self.reads.checkouts.read(extra.iter().cloned(), HELD_WAIT);
        found.extend(extra.iter().filter_map(|r| loaded.remove(r).flatten()));

        self.checkouts.clear();
        for c in found {
            let Some(identity) = c.identity.clone() else { continue };
            // A project that lists the repo's folder has it, before one its remote matches: a
            // folder added for a new project stays there once its repo has a remote.
            let listed = config::project_with_folder(&self.cfg.projects, &c.root);
            if let Some(p) = listed.or_else(|| self.cfg.projects.iter().find(|p| p.repos.iter().any(|pat| repos::matches(&identity, pat)))) {
                self.checkouts.push((p.name.clone(), c));
            }
        }

        let mut seeds = Vec::new();
        self.gitdir_to_lane.clear();
        self.lane_base.clear();
        self.lane_checkout.clear();
        // `git worktree list` and the default branch for every checkout, in parallel.
        let known_bases = self.default_branch.clone();
        let listed: HashMap<PathBuf, (Vec<repos::Worktree>, Option<String>)> = parallel_map(
            self.checkouts.iter().map(|(_, c)| c.clone()).collect(),
            |c| {
                let base = known_bases.get(&c.root).cloned().unwrap_or_else(|| repos::default_branch(&c));
                Some((c.root.clone(), (repos::list_worktrees(&c), base)))
            },
        )
        .into_iter()
        .collect();
        // Each worktree's `.git` file and `.gitignore`. One whose files are held shows as `git
        // worktree list` gives it (git only looks its path up) and gets git once they land.
        let paths: Vec<PathBuf> = listed.values().flat_map(|(wts, _)| wts.iter().map(|w| w.path.clone())).collect();
        let mut files = self.reads.worktrees.read(paths.iter().cloned(), HELD_WAIT);
        self.held_worktrees = paths.into_iter().filter(|p| !files.contains_key(p)).collect();
        self.ignores.clear();
        for (project, c) in &self.checkouts {
            let Some((worktrees, base)) = listed.get(&c.root).cloned() else { continue };
            self.default_branch.insert(c.root.clone(), base.clone());
            let default_name = base.as_ref().map(|b| b.split_once('/').map(|(_, n)| n.to_string()).unwrap_or_else(|| b.clone()));
            let repo_name = c.github.as_ref().map(|(_, n)| n.clone()).unwrap_or_else(|| c.name.clone());
            for wt in worktrees {
                if let Some(f) = files.remove(&wt.path) {
                    if let Some(gitdir) = f.gitdir {
                        self.gitdir_to_lane.insert(gitdir, wt.path.clone());
                    }
                    if let Some(gi) = f.ignore {
                        self.ignores.insert(wt.path.clone(), gi);
                    }
                }
                self.lane_base.insert(wt.path.clone(), base.clone());
                self.lane_checkout.insert(wt.path.clone(), c.root.clone());
                seeds.push(LaneSeed {
                    callsign: repos::callsign(c, &wt.path, wt.branch.as_deref(), &h),
                    origin: repos::origin(c, &wt.path, &h),
                    path: wt.path,
                    project: project.clone(),
                    repo: repo_name.clone(),
                    repo_slug: c.slug(),
                    head_owners: c.github_owners.clone(),
                    branch: wt.branch,
                    head: wt.head,
                    default_branch: default_name.clone(),
                    elsewhere: false,
                    chats: false,
                    starts_chats: false,
                    exact: false,
                });
            }
        }
        for p in &self.cfg.projects {
            for f in &p.folders {
                let path = expand(f);
                if path.is_dir() && !seeds.iter().any(|s| s.path == path) {
                    let name = file_name(&path);
                    seeds.push(folder_seed(path, &p.name, &name, false));
                }
            }
        }
        // Elsewhere: every folder a session ran in outside your projects.
        let found = |p: &Path| {
            if !p.is_dir() {
                return Found::Gone;
            }
            match located.get(p) {
                Some(Some((wt, _))) => Found::Repo(wt.clone()),
                Some(None) => Found::Plain,
                // Held: looked at again once it lands.
                None => Found::Held,
            }
        };
        let strays = stray_seeds(&seeds, &cwds, &chats, &config::chats_dir(), &h, found);
        seeds.extend(strays);

        let live: HashSet<&PathBuf> = seeds.iter().map(|s| &s.path).collect();
        self.git.retain(|p, _| live.contains(p));
        self.drops.retain(|p| live.contains(p));
        self.seeds = seeds;
        if let Some(w) = &self.servers {
            w.set_roots(self.server_roots());
        }
        // An engine that makes one pass is gone before GitHub would answer.
        if !self.passing {
            self.github.ask_names(&self.seeds, Instant::now());
        }
        self.refresh_org_avatars();
    }

    /// The worktrees whose servers are looked for: those in projects. A folder outside them can
    /// be your home folder, where every app's own server would be taken for the worktree's.
    pub(super) fn server_roots(&self) -> Vec<PathBuf> {
        let h = home();
        self.seeds.iter().filter(|s| !s.elsewhere && s.path != h).map(|s| s.path.clone()).collect()
    }

    pub(super) fn git_targets(&self) -> Vec<PathBuf> {
        self.seeds.iter().filter(|s| !s.elsewhere && s.origin != Origin::Folder && !self.held_worktrees.contains(&s.path)).map(|s| s.path.clone()).collect()
    }

    pub(super) fn base_for(&self, path: &Path) -> Option<String> {
        self.lane_base.get(path).cloned().flatten()
    }

    /// Reads the repos again: worktrees came or went, a session started where no lane covers,
    /// or reads macOS held landed. Git runs in the worktrees that are new or readable now.
    pub(super) fn rescan_repos(&mut self) {
        let t = Instant::now();
        let before: HashSet<PathBuf> = self.git_targets().into_iter().collect();
        self.load_repos();
        let loaded = t.elapsed();
        let added: Vec<PathBuf> = self.git_targets().into_iter().filter(|p| !before.contains(p)).collect();
        for p in added {
            self.queue_git(p);
        }
        let w = Instant::now();
        self.watch();
        let watched = w.elapsed();
        self.refresh_visitors(false);
        self.mark_dirty("rescan repos");
        perf::took(|| format!("rescan repos ({} worktrees, loading {} ms, watching {} ms), in all", self.seeds.len(), loaded.as_millis(), watched.as_millis()), t.elapsed());
    }
}

/// A folder that's no repo's worktree, called by its name, with nothing said of a branch or a
/// remote. The made-up Mac's seeds start from it too (fixture.rs).
pub(crate) fn folder_seed(path: PathBuf, project: &str, name: &str, elsewhere: bool) -> LaneSeed {
    LaneSeed {
        path,
        project: project.to_string(),
        repo: name.to_string(),
        repo_slug: None,
        head_owners: Vec::new(),
        callsign: name.to_string(),
        origin: if elsewhere { Origin::Other } else { Origin::Folder },
        branch: None,
        head: None,
        default_branch: None,
        elsewhere,
        chats: false,
        starts_chats: false,
        exact: false,
    }
}

/// The folders apps keep their chats in, a folder for each, and the name of the lane that holds
/// them: the Claude app's Code sessions started without a folder, the Codex app's threads that
/// belong to no project, and the sessions Codealong starts in no project (`config::chats_dir`).
fn chat_roots() -> Vec<(PathBuf, &'static str)> {
    let mut roots = harness::chat_roots();
    roots.push((config::chats_dir(), CHATS));
    roots
}

/// The lane of the sessions Codealong starts in no project.
const CHATS: &str = "Chats";

/// Which of `roots` holds `path`.
fn chat_root(roots: &[(PathBuf, &'static str)], path: &Path) -> Option<usize> {
    roots.iter().position(|(root, _)| path.starts_with(root))
}

/// What a folder a session ran in turned out to be, for `stray_seeds`.
enum Found {
    /// In a repo: the root of its worktree.
    Repo(PathBuf),
    Plain,
    /// Its read is waiting on macOS.
    Held,
    Gone,
}

/// Lanes for the folders sessions ran in that no lane covers, so no session goes unlisted: one
/// for all of an app's chats (a chat whose folder the app has removed too), one for each repo
/// outside your projects, and one for each other folder, which takes in only the sessions
/// started there. A folder that's gone, or whose read is held, has none. Codealong's own chats
/// (`own`, among `chats`) have a lane with no sessions too, to start in.
fn stray_seeds(seeds: &[LaneSeed], cwds: &BTreeSet<String>, chats: &[(PathBuf, &'static str)], own: &Path, home: &Path, found: impl Fn(&Path) -> Found) -> Vec<LaneSeed> {
    let mut roots: BTreeSet<usize> = chats.iter().position(|(root, _)| root == own && !seeds.iter().any(|s| s.path == *root)).into_iter().collect();
    let mut repos = BTreeSet::new();
    let mut folders = BTreeSet::new();
    let index = board::SeedIndex::new(seeds);
    for cwd in cwds {
        let p = Path::new(cwd);
        if index.best(p).is_some() {
            continue;
        }
        if let Some(i) = chat_root(chats, p) {
            roots.insert(i);
            continue;
        }
        match found(p) {
            Found::Repo(wt) => {
                repos.insert(wt);
            }
            Found::Plain => {
                folders.insert(p.to_path_buf());
            }
            Found::Held | Found::Gone => {}
        }
    }
    let name = |p: &Path| if p == home { "home".to_string() } else { file_name(p) };
    let mut out = Vec::new();
    for i in roots {
        let (root, lane) = &chats[i];
        out.push(LaneSeed { origin: Origin::Folder, chats: true, starts_chats: root == own, ..folder_seed(root.clone(), ELSEWHERE, lane, true) });
    }
    for root in repos {
        out.push(folder_seed(root.clone(), ELSEWHERE, &name(&root), true));
    }
    for folder in folders {
        out.push(LaneSeed { origin: Origin::Folder, exact: true, ..folder_seed(folder.clone(), ELSEWHERE, &name(&folder), true) });
    }
    out
}

/// What the repo scan reads inside a worktree.
struct WorktreeFiles {
    /// Its git folder in the main checkout, for telling which worktree a change there is for.
    /// None for a main checkout.
    gitdir: Option<PathBuf>,
    ignore: Option<Gitignore>,
}

fn read_worktree(path: &Path) -> WorktreeFiles {
    // A linked worktree's `.git` is a file that names its git folder; a main checkout's is that folder.
    // Made canonical, as the watcher's paths are, so a change under it is matched to its lane.
    let gitdir = if path.join(".git").is_file() { crate::repos::linked_gitdir(path).map(|d| dunce::canonicalize(&d).unwrap_or(d)) } else { None };
    let mut b = GitignoreBuilder::new(path);
    b.add(path.join(".gitignore"));
    WorktreeFiles { gitdir, ignore: b.build().ok() }
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_session_folder_that_can_be_shown_gets_a_lane() {
        use super::*;
        let home = PathBuf::from("/Users/me");
        let scratch = "/Users/me/Library/Application Support/Claude/scratch-workspaces";
        let own = PathBuf::from("/Users/me/Library/Application Support/Codealong/chats");
        let chats = vec![(PathBuf::from(scratch), "Claude app chats"), (PathBuf::from("/Users/me/Documents/Codex"), "Codex chats"), (own.clone(), CHATS)];
        let seeds = vec![folder_seed(PathBuf::from("/Users/me/projects/app"), "App", "app", false)];
        let cwds: BTreeSet<String> = [
            "/Users/me/projects/app/src".to_string(),
            format!("{scratch}/a/b/scratch-2026-09-28-045217"),
            format!("{scratch}/a/b/scratch-2026-09-26-c8bd6b"),
            "/Users/me/Documents/Codex/2026-09-27/can-you".to_string(),
            "/Users/me/projects/other/web".to_string(),
            "/Users/me".to_string(),
            "/Users/me/Downloads".to_string(),
            "/Users/me/Documents/held".to_string(),
            "/tmp/gone".to_string(),
        ]
        .into_iter()
        .collect();
        // Chats aren't looked up, so one whose folder the app removed still has its lane.
        let found = |p: &Path| match p.to_str().unwrap() {
            "/Users/me/projects/other/web" => Found::Repo(PathBuf::from("/Users/me/projects/other")),
            "/Users/me" | "/Users/me/Downloads" => Found::Plain,
            "/Users/me/Documents/held" => Found::Held,
            "/tmp/gone" => Found::Gone,
            p => panic!("{p} was looked up"),
        };
        let mut all = seeds.clone();
        all.extend(stray_seeds(&seeds, &cwds, &chats, &own, &home, found));
        let lane = |cwd: &str| board::best_seed(&all, Path::new(cwd)).map(|i| (all[i].callsign.as_str(), all[i].chats, all[i].exact));
        // Codealong's own chats have a lane before any session has started there, to start in.
        let mine: Vec<&LaneSeed> = all.iter().filter(|s| s.starts_chats).collect();
        assert_eq!(mine.len(), 1);
        assert_eq!((mine[0].path.as_path(), mine[0].callsign.as_str(), mine[0].chats, mine[0].elsewhere), (own.as_path(), CHATS, true, true));
        assert_eq!(lane(&format!("{}/2026-10-05-a1b2c3", own.display())), Some((CHATS, true, false)));
        assert_eq!(lane("/Users/me/projects/app/src"), Some(("app", false, false)));
        assert_eq!(lane(&format!("{scratch}/a/b/scratch-2026-09-28-045217")), Some(("Claude app chats", true, false)));
        assert_eq!(lane(&format!("{scratch}/a/b/scratch-2026-09-26-c8bd6b")), Some(("Claude app chats", true, false)));
        assert_eq!(lane("/Users/me/Documents/Codex/2026-09-27/can-you"), Some(("Codex chats", true, false)));
        assert_eq!(lane("/Users/me/projects/other/web"), Some(("other", false, false)));
        assert_eq!(lane("/Users/me"), Some(("home", false, true)));
        assert_eq!(lane("/Users/me/Downloads"), Some(("Downloads", false, true)));
        assert_eq!(lane("/Users/me/Desktop"), None, "your home folder's lane doesn't take in the folders in it");
        assert_eq!((lane("/Users/me/Documents/held"), lane("/tmp/gone")), (None, None));
        assert_eq!(all.len(), 7, "one lane for each app's chats, and Codealong's");
        assert!(all[1..].iter().all(|s| s.elsewhere && s.project == ELSEWHERE));
    }

    /// macOS holds `open()` in a folder it protects until someone answers its privacy prompt,
    /// which on a Mac nobody sits at can be hours. A worktree whose files never read doesn't hold
    /// up the first scan: the board goes out with it as `git worktree list` gives it, git runs
    /// everywhere else, and a worktree whose read lands later is filled in. Runs in a copy of
    /// this test binary with a home of its own, as the scan reads and writes there.
    #[test]
    fn the_first_scan_publishes_while_a_worktree_read_is_held() {
        use super::*;

        let Some(own) = std::env::var_os("CODEALONG_TEST_HOME") else {
            let home = std::env::temp_dir().join(format!("codealong-held-scan-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&home);
            std::fs::create_dir_all(&home).unwrap();
            // As git reports paths: /private/var, not /var.
            let home = dunce::canonicalize(&home).unwrap();
            let run = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["engine::reposcan::tests::the_first_scan_publishes_while_a_worktree_read_is_held", "--exact", "--nocapture"])
                .env("HOME", &home)
                .env("CODEALONG_TEST_HOME", &home)
                .output()
                .unwrap();
            let ran = home.join("ran").exists();
            std::fs::remove_dir_all(&home).unwrap();
            assert!(run.status.success() && ran, "{}{}", String::from_utf8_lossy(&run.stdout), String::from_utf8_lossy(&run.stderr));
            return;
        };
        assert_eq!(home(), PathBuf::from(own), "never the real home folder");
        // Fails rather than hangs when the scan waits on the read that never returns.
        std::thread::spawn(|| {
            std::thread::sleep(Duration::from_secs(60));
            eprintln!("The scan waited on a read that never returns.");
            std::process::exit(1);
        });

        let git = |dir: &Path, args: &[&str]| {
            let out = std::process::Command::new("git").arg("-C").arg(dir).args(["-c", "user.name=t", "-c", "user.email=t@t"]).args(args).output().unwrap();
            assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
        };
        let projects = home().join("projects");
        let main = projects.join("repo");
        std::fs::create_dir_all(&main).unwrap();
        git(&main, &["init", "-q", "-b", "main"]);
        git(&main, &["remote", "add", "origin", "git@example.com:team/repo.git"]);
        std::fs::write(main.join(".gitignore"), "build/\n").unwrap();
        git(&main, &["add", "."]);
        git(&main, &["commit", "-qm", "init"]);
        let path = |name: &str| projects.join(format!("repo-{name}"));
        for name in ["stuck", "late", "plain"] {
            git(&main, &["worktree", "add", "-q", &path(name).to_string_lossy(), "-b", name]);
        }
        let project = crate::config::ProjectCfg { name: "Team".into(), repos: vec!["example.com/team/*".into()], folders: vec![], chrome_profile: None, default: None, decisions: None, background_prompt: None };
        config::write(&Config { scan: vec!["~/projects".into()], projects: vec![project], ..Config::default() }).unwrap();

        let boards: Arc<Mutex<Vec<Board>>> = Arc::default();
        let emitted = boards.clone();
        let (tx, rx) = channel();
        let mut engine = Engine::new(Box::new(move |b: &Board| emitted.lock().unwrap().push(b.clone())), tx.clone(), Arc::new(RwLock::new(Board::default())));
        // The stuck worktree's files never read; the late one's read once it's let go.
        let (release, gate) = channel::<()>();
        let gate = Mutex::new(gate);
        engine.reads.worktrees = Held::new(
            "worktree",
            move |p: &Path| {
                if p.ends_with("repo-stuck") {
                    loop {
                        std::thread::park();
                    }
                }
                if p.ends_with("repo-late") {
                    let _ = gate.lock().unwrap().recv();
                }
                read_worktree(p)
            },
            move || {
                let _ = tx.send(Msg::Landed);
            },
        );
        let started = Instant::now();
        engine.full_scan();
        assert!(started.elapsed() < Duration::from_secs(30), "the scan waited on the held reads");

        let first = boards.lock().unwrap().first().cloned().expect("the first scan published a board");
        assert!(!first.loading);
        let branch = |b: &Board, p: &Path| b.lanes.iter().find(|l| Path::new(&l.path) == p).map(|l| l.branch.clone());
        assert_eq!(branch(&first, &main), Some(Some("main".to_string())));
        for name in ["stuck", "late", "plain"] {
            assert_eq!(branch(&first, &path(name)), Some(Some(name.to_string())), "{name} is on the board as git worktree list gives it");
        }
        let targets = engine.git_targets();
        assert!(targets.contains(&main) && targets.contains(&path("plain")));
        assert!(!targets.contains(&path("stuck")) && !targets.contains(&path("late")), "no git where the files are held");
        assert!(engine.ignores.contains_key(&path("plain")) && !engine.ignores.contains_key(&path("late")));

        // The late read lands while the engine runs as the app's does: the next board has git there.
        std::thread::spawn(move || engine.run(&rx));
        release.send(()).unwrap();
        let has_git = |b: &Board, p: &Path| b.lanes.iter().any(|l| Path::new(&l.path) == p && l.git.is_some());
        let until = Instant::now() + Duration::from_secs(20);
        let filled = loop {
            if let Some(b) = boards.lock().unwrap().iter().find(|b| has_git(b, &path("late"))) {
                break b.clone();
            }
            assert!(Instant::now() < until, "the late worktree never got git");
            std::thread::sleep(Duration::from_millis(50));
        };
        assert!(has_git(&filled, &path("plain")) && !has_git(&filled, &path("stuck")));
        std::fs::write(home().join("ran"), "").unwrap();
    }
}
