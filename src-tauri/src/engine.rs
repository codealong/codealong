//! The engine owns every cache. It reacts to file changes (FSEvents), runs git on a small
//! worker pool and GitHub in the background, and publishes a fresh board after each change.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Result};
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};

use crate::board::{self, LaneSeed, ELSEWHERE};
use crate::claude::{self, RegistryEntry, TranscriptInfo};
use crate::codex::{self, ThreadInfo};
use crate::config::{self, AccountCfg, Config};
use crate::git;
use crate::github::{self, PrRaw};
use crate::launch;
use crate::model::*;
use crate::repos::{self, Checkout};
use crate::state::SeenState;
use crate::usage;
use crate::util::{expand, file_name, home, mtime_ms, now_ms, read_first_line, DAY_MS};

/// How far back sessions are read.
const WINDOW_MS: i64 = 14 * DAY_MS;
const GIT_WORKERS: usize = 4;
const PUBLISH_EVERY: Duration = Duration::from_millis(300);
const GITHUB_EVERY: Duration = Duration::from_secs(60);
/// Claude usage is read this often; Codex usage updates with every Codex turn.
const USAGE_EVERY: Duration = Duration::from_secs(2 * 60);
/// After the usage endpoint says to slow down.
const USAGE_BACKOFF: Duration = Duration::from_secs(10 * 60);

pub enum Msg {
    Fs(Vec<PathBuf>),
    GitDone(PathBuf, GitInfo),
    /// An empty slug marks the end of a GitHub batch.
    GithubDone(String, std::result::Result<Vec<PrRaw>, String>),
    RefreshGithub,
    MarkSeen(Vec<String>),
    /// Lane path → session keys whose logs mention it.
    Visitors(HashMap<PathBuf, Vec<String>>),
    RefreshUsage,
    UsageDone(Vec<Usage>),
}

/// What the UI talks to. Cheap to clone.
#[derive(Clone)]
pub struct Handle {
    tx: Sender<Msg>,
    board: Arc<RwLock<Board>>,
}

impl Handle {
    pub fn board(&self) -> Board {
        self.board.read().map(|b| b.clone()).unwrap_or_default()
    }

    pub fn refresh_github(&self) {
        let _ = self.tx.send(Msg::RefreshGithub);
    }

    pub fn mark_seen(&self, keys: Vec<String>) {
        let _ = self.tx.send(Msg::MarkSeen(keys));
    }

    pub fn refresh_usage(&self) {
        let _ = self.tx.send(Msg::RefreshUsage);
    }

    /// The accounts in config.toml, each with whether it's signed in.
    pub fn accounts(&self) -> Result<Vec<AccountInfo>> {
        let cfg = config::load()?.unwrap_or_default();
        Ok(cfg.accounts.iter().map(usage::describe).collect())
    }

    /// Saves the account list to config.toml. The watcher sees the change and reloads.
    pub fn save_accounts(&self, accounts: Vec<AccountCfg>) -> Result<()> {
        config::save_accounts(&accounts)
    }

    /// Opens a terminal window that signs the account in.
    pub fn sign_in(&self, name: &str) -> Result<()> {
        let cfg = config::load()?.unwrap_or_default();
        let a = cfg.accounts.iter().find(|a| a.name == name).ok_or_else(|| anyhow!("There's no account named {name}."))?;
        launch::terminal_open(&cfg.apps.terminal, &home(), Some(&format!("{}\n", usage::sign_in_command(a))))
    }

    /// Opens config.toml in the configured editor.
    pub fn open_config(&self) -> Result<()> {
        let editor = self.board.read().map(|b| b.apps.editor_id.clone()).unwrap_or_default();
        let editor = if editor.is_empty() { "cursor".to_string() } else { editor };
        launch::editor_open(&editor, &config::config_path())
    }

    /// Opens a lane's target where it lives. See *Surfaces*, Open where it lives.
    pub fn open(&self, lane_id: &str, target_id: &str) -> Result<()> {
        let board = self.board.read().map(|b| b.clone()).map_err(|_| anyhow!("board unavailable"))?;
        if let Some(key) = open_on(&board, lane_id, target_id)? {
            self.mark_seen(vec![key]);
        }
        Ok(())
    }
}

/// Opens a lane's target on `board`. Returns the key of the session it opened, if any.
pub fn open_on(board: &Board, lane_id: &str, target_id: &str) -> Result<Option<String>> {
    let lane = board.lanes.iter().find(|l| l.id == lane_id).cloned().ok_or_else(|| anyhow!("That lane just changed. Try again."))?;
    let apps = board.apps.clone();
    let path = PathBuf::from(&lane.path);
    let parts: Vec<&str> = target_id.splitn(3, '|').collect();
    match parts.as_slice() {
        ["s", key, action] => {
            let s = lane.sessions.iter().find(|s| s.key == *key).ok_or_else(|| anyhow!("That session is gone."))?;
            let cwd = if Path::new(&s.cwd).is_dir() { PathBuf::from(&s.cwd) } else { path.clone() };
            match *action {
                "focus" => {
                    let pid = s.pid.ok_or_else(|| anyhow!("That session isn't running any more."))?;
                    if !launch::terminal_focus_session(&apps.terminal_id, pid, &s.title, &cwd)? {
                        bail!("Couldn't find {}'s terminal in {}. It's running as pid {pid}.", s.account, apps.terminal);
                    }
                }
                "attach" => {
                    let cmd = s.attach_command.clone().ok_or_else(|| anyhow!("This session isn't a background job."))?;
                    launch::terminal_open(&apps.terminal_id, &cwd, Some(&format!("{cmd}\n")))?;
                }
                "resume" => {
                    let cmd = s.resume_command.clone().ok_or_else(|| anyhow!("This session can't be resumed in a terminal."))?;
                    launch::terminal_open(&apps.terminal_id, &cwd, Some(&format!("{cmd}\n")))?;
                }
                "claude-app" => launch::open_url(&launch::claude_app_url(&s.id))?,
                "codex" => launch::open_codex(&launch::codex_thread_url(&s.id))?,
                other => bail!("Unknown action {other:?}"),
            }
            return Ok(Some(s.key.clone()));
        }
        ["l", "terminal"] => launch::terminal_open(&apps.terminal_id, &path, None)?,
        ["l", "editor"] => launch::editor_open(&apps.editor_id, &path)?,
        ["l", "finder"] => launch::reveal(&path)?,
        ["p", which] => {
            let pr = lane.pr.as_ref().ok_or_else(|| anyhow!("This lane has no PR."))?;
            match *which {
                "github" => launch::open_url(&pr.url)?,
                "codex-review" => launch::open_codex(&launch::codex_review_url(&pr.url))?,
                other => bail!("Unknown PR action {other:?}"),
            }
        }
        _ => bail!("Unknown target {target_id:?}"),
    }
    Ok(None)
}

pub fn start(emit: impl Fn(&Board) + Send + 'static) -> Handle {
    let (tx, rx) = channel();
    let board = Arc::new(RwLock::new(Board { loading: true, ..Default::default() }));
    let handle = Handle { tx: tx.clone(), board: board.clone() };
    std::thread::Builder::new()
        .name("codealong-engine".into())
        .spawn(move || {
            let mut engine = Engine::new(Box::new(emit), tx, board);
            engine.full_scan();
            engine.run(rx);
        })
        .expect("engine thread");
    handle
}

/// One synchronous pass for `--snapshot`: everything the board needs, no watchers.
pub fn snapshot(with_github: bool) -> Board {
    let (tx, _rx) = channel();
    let mut e = Engine::new(Box::new(|_| {}), tx, Arc::new(RwLock::new(Board::default())));
    let t = Instant::now();
    let lap = |label: &str, t: &Instant| eprintln!("  {label:<10} {:>6} ms", t.elapsed().as_millis());
    e.load_config();
    e.seen = SeenState::load();
    lap("config", &t);
    e.scan_sessions();
    lap("sessions", &t);
    e.load_repos();
    lap("repos", &t);
    e.refresh_visitors(true);
    lap("visitors", &t);
    e.loading = false;
    let _ = e.compose();
    lap("1st paint", &t);
    let jobs: Vec<(PathBuf, Option<String>)> = e.git_targets().into_iter().map(|p| (p.clone(), e.base_for(&p))).collect();
    for (p, info) in parallel_map(jobs, |(p, base)| Some((p.clone(), git::status(&p, base.as_deref())))) {
        e.git.insert(p, info);
    }
    lap("git", &t);
    for u in usage::claude_all(&e.cfg.accounts) {
        e.usage.insert(u.account.clone(), u);
    }
    lap("usage", &t);
    if with_github {
        let slugs: Vec<String> = e.github_slugs().into_iter().collect();
        let results = parallel_map(slugs, |slug| {
            let (owner, name) = slug.split_once('/')?;
            Some((slug.clone(), github::fetch(owner, name)))
        });
        for (slug, result) in results {
            match result {
                Ok(list) => {
                    e.prs.insert(slug, list);
                    e.gh_at = Some(now_ms());
                }
                Err(err) => e.gh_warning = Some(err.to_string()),
            }
        }
        lap("github", &t);
    }
    e.compose()
}

/// The newest write to a Claude session's subagent logs (`<transcript dir>/<id>/subagents`).
fn subagent_activity(transcript: &Path, session_id: &str) -> Option<i64> {
    let dir = transcript.parent()?.join(session_id).join("subagents");
    std::fs::read_dir(dir).ok()?.flatten().filter_map(|e| mtime_ms(&e.path())).max()
}

/// A read that failed for a reason other than sign-in keeps the last good numbers; its error
/// says why they aren't fresh and `as_of` says how old they are.
fn keep_last_good(prev: Option<&Usage>, mut fresh: Usage) -> Usage {
    if fresh.error.is_some() && !fresh.needs_sign_in {
        if let Some(prev) = prev.filter(|p| !p.meters.is_empty()) {
            fresh.meters = prev.meters.clone();
            fresh.plan = fresh.plan.or_else(|| prev.plan.clone());
            fresh.as_of = prev.as_of;
        }
    }
    fresh
}

/// Maps `f` over `items` on a few threads.
fn parallel_map<T: Send, R: Send>(items: Vec<T>, f: impl Fn(T) -> Option<R> + Sync) -> Vec<R> {
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(8);
    let chunk = items.len().div_ceil(threads).max(1);
    let mut buckets: Vec<Vec<T>> = Vec::new();
    let mut items = items.into_iter().peekable();
    while items.peek().is_some() {
        buckets.push(items.by_ref().take(chunk).collect());
    }
    let f = &f;
    std::thread::scope(|scope| {
        let handles: Vec<_> = buckets.into_iter().map(|b| scope.spawn(move || b.into_iter().filter_map(f).collect::<Vec<R>>())).collect();
        handles.into_iter().flat_map(|h| h.join().unwrap_or_default()).collect()
    })
}

enum RepoEvent {
    Lane(PathBuf),
    Checkout(PathBuf),
    Worktrees,
    Ignore,
}

struct Engine {
    emit: Box<dyn Fn(&Board) + Send>,
    tx: Sender<Msg>,
    board: Arc<RwLock<Board>>,
    cfg: Config,
    config_warning: Option<String>,
    seen: SeenState,
    checkouts: Vec<(String, Checkout)>,
    seeds: Vec<LaneSeed>,
    /// `<main>/.git/worktrees/<name>` → the linked worktree's path.
    gitdir_to_lane: HashMap<PathBuf, PathBuf>,
    ignores: HashMap<PathBuf, Gitignore>,
    registry: HashMap<String, Vec<RegistryEntry>>,
    transcripts: HashMap<PathBuf, (String, claude::Parsed)>,
    codex_index: HashMap<String, HashMap<String, (String, i64)>>,
    /// `None` marks a thread that's never shown (a guardian review, a subagent), so its
    /// constant writes are ignored.
    rollouts: HashMap<PathBuf, (String, Option<(ThreadInfo, u64)>)>,
    /// Session folders that aren't in any repo, so they're not looked up again.
    not_repos: HashSet<PathBuf>,
    /// Hash of the last published board, to skip publishing when nothing changed.
    last_hash: u64,
    /// Lane path → sessions from other lanes whose logs show they worked there.
    visitors: HashMap<PathBuf, Vec<String>>,
    /// What to look for in logs: (lane path, text that identifies it).
    needles: Vec<(PathBuf, String)>,
    git: HashMap<PathBuf, GitInfo>,
    git_jobs: Sender<(PathBuf, Option<String>)>,
    git_inflight: HashSet<PathBuf>,
    git_again: HashSet<PathBuf>,
    /// Checkout root → default branch ref (`origin/main`).
    default_branch: HashMap<PathBuf, Option<String>>,
    lane_base: HashMap<PathBuf, Option<String>>,
    /// Lane path → the main checkout it belongs to.
    lane_checkout: HashMap<PathBuf, PathBuf>,
    prs: HashMap<String, Vec<PrRaw>>,
    gh_warning: Option<String>,
    gh_inflight: bool,
    gh_last: Option<Instant>,
    gh_at: Option<i64>,
    watcher: Option<RecommendedWatcher>,
    watch_warning: Option<String>,
    /// Account folders that didn't exist at the last watch: a new account before sign-in.
    missing_homes: Vec<PathBuf>,
    /// Claude usage by account, from the last background read.
    usage: HashMap<String, Usage>,
    /// (pid, status time) → the command a stuck Claude session's shell is running.
    stuck: Mutex<HashMap<(u32, i64), Option<String>>>,
    usage_inflight: bool,
    usage_last: Option<Instant>,
    usage_backoff: bool,
    dirty: bool,
    loading: bool,
    last_publish: Option<Instant>,
}

impl Engine {
    fn new(emit: Box<dyn Fn(&Board) + Send>, tx: Sender<Msg>, board: Arc<RwLock<Board>>) -> Engine {
        let git_jobs = spawn_git_workers(tx.clone());
        Engine {
            emit,
            tx,
            board,
            cfg: Config::default(),
            config_warning: None,
            seen: SeenState::default(),
            checkouts: Vec::new(),
            seeds: Vec::new(),
            gitdir_to_lane: HashMap::new(),
            ignores: HashMap::new(),
            registry: HashMap::new(),
            transcripts: HashMap::new(),
            codex_index: HashMap::new(),
            rollouts: HashMap::new(),
            not_repos: HashSet::new(),
            last_hash: 0,
            visitors: HashMap::new(),
            needles: Vec::new(),
            git: HashMap::new(),
            git_jobs,
            git_inflight: HashSet::new(),
            git_again: HashSet::new(),
            default_branch: HashMap::new(),
            lane_base: HashMap::new(),
            lane_checkout: HashMap::new(),
            prs: HashMap::new(),
            gh_warning: None,
            gh_inflight: false,
            gh_last: None,
            gh_at: None,
            watcher: None,
            watch_warning: None,
            missing_homes: Vec::new(),
            usage: HashMap::new(),
            stuck: Mutex::new(HashMap::new()),
            usage_inflight: false,
            usage_last: None,
            usage_backoff: false,
            dirty: false,
            loading: true,
            last_publish: None,
        }
    }

    fn full_scan(&mut self) {
        self.load_config();
        self.seen = SeenState::load();
        self.seen.save();
        self.scan_sessions();
        self.load_repos();
        self.loading = false;
        self.publish();
        self.queue_git_all();
        self.fetch_github(true);
        self.fetch_usage();
        self.watch();
        self.refresh_visitors(false);
    }

    /// Reads Claude usage on a background thread; Codex usage comes from the rollouts.
    fn fetch_usage(&mut self) {
        if self.usage_inflight {
            return;
        }
        let accounts: Vec<AccountCfg> = self.cfg.accounts.iter().filter(|a| a.harness == "claude").cloned().collect();
        self.usage_last = Some(Instant::now());
        if accounts.is_empty() {
            return;
        }
        self.usage_inflight = true;
        let tx = self.tx.clone();
        let _ = std::thread::Builder::new().name("codealong-usage".into()).spawn(move || {
            let _ = tx.send(Msg::UsageDone(usage::claude_all(&accounts)));
        });
    }

    /// Everything that depends on config.toml, read again.
    fn reload(&mut self) {
        self.load_config();
        self.scan_sessions();
        self.load_repos();
        self.queue_git_all();
        self.fetch_github(true);
        self.usage.retain(|name, _| self.cfg.accounts.iter().any(|a| &a.name == name));
        self.usage_inflight = false;
        self.fetch_usage();
        self.watch();
        self.dirty = true;
    }

    fn run(&mut self, rx: Receiver<Msg>) {
        let mut pending: Vec<PathBuf> = Vec::new();
        let mut first_event: Option<Instant> = None;
        let mut last_event: Option<Instant> = None;
        let mut last_tick = Instant::now();
        let mut last_heartbeat = Instant::now();
        loop {
            let wait = if !pending.is_empty() || self.dirty { Duration::from_millis(50) } else { Duration::from_secs(2) };
            match rx.recv_timeout(wait) {
                Ok(Msg::Fs(paths)) => {
                    let now = Instant::now();
                    first_event.get_or_insert(now);
                    last_event = Some(now);
                    pending.extend(paths);
                }
                Ok(Msg::GitDone(path, info)) => self.on_git(path, info),
                Ok(Msg::GithubDone(slug, result)) => self.on_github(slug, result),
                Ok(Msg::RefreshGithub) => self.fetch_github(false),
                Ok(Msg::Visitors(found)) => {
                    self.visitors = found;
                    self.dirty = true;
                }
                Ok(Msg::RefreshUsage) => self.fetch_usage(),
                Ok(Msg::UsageDone(list)) => {
                    self.usage_inflight = false;
                    self.usage_backoff = list.iter().any(|u| u.rate_limited);
                    for u in list {
                        let u = keep_last_good(self.usage.get(&u.account), u);
                        self.usage.insert(u.account.clone(), u);
                    }
                    self.dirty = true;
                }
                Ok(Msg::MarkSeen(keys)) => {
                    let now = now_ms();
                    for k in keys {
                        self.seen.mark(&k, now);
                    }
                    self.seen.save();
                    self.dirty = true;
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
            if !pending.is_empty() {
                let quiet = last_event.map(|t| t.elapsed() >= Duration::from_millis(150)).unwrap_or(true);
                let overdue = first_event.map(|t| t.elapsed() >= Duration::from_millis(600)).unwrap_or(false);
                if quiet || overdue {
                    let batch = std::mem::take(&mut pending);
                    first_event = None;
                    self.apply_fs(batch);
                }
            }
            if last_tick.elapsed() >= Duration::from_secs(10) {
                last_tick = Instant::now();
                self.tick();
            }
            if last_heartbeat.elapsed() >= Duration::from_secs(30) {
                // Codex turns go stale with time alone; republishing picks that up.
                last_heartbeat = Instant::now();
                self.dirty = true;
            }
            let due = self.last_publish.map(|t| t.elapsed() >= PUBLISH_EVERY).unwrap_or(true);
            if self.dirty && due {
                self.publish();
            }
        }
    }

    fn load_config(&mut self) {
        match config::load() {
            Ok(Some(cfg)) => {
                self.cfg = cfg;
                self.config_warning = None;
            }
            Ok(None) => {
                let cfg = first_run_config();
                if let Err(e) = config::write(&cfg) {
                    self.config_warning = Some(format!("Couldn't save config.toml: {e}"));
                }
                self.cfg = cfg;
            }
            Err(e) => {
                self.config_warning = Some(format!("{e:#}. Codealong is using the last good settings."));
                if self.cfg.accounts.is_empty() {
                    self.cfg.accounts = config::discover_accounts();
                }
            }
        }
    }

    fn account(&self, name: &str) -> Option<&AccountCfg> {
        self.cfg.accounts.iter().find(|a| a.name == name)
    }

    fn scan_sessions(&mut self) {
        let since = now_ms() - WINDOW_MS;
        self.registry.clear();
        self.transcripts.clear();
        self.codex_index.clear();
        self.rollouts.clear();
        for a in self.cfg.accounts.clone() {
            let home = a.home_path();
            match a.harness.as_str() {
                "claude" => {
                    self.registry.insert(a.name.clone(), claude::read_registry(&home));
                    let files = claude::transcript_files(&home, since);
                    for (path, parsed) in parallel_map(files, |(p, m)| claude::parse_transcript(&p, m, None).map(|r| (p, r))) {
                        self.transcripts.insert(path, (a.name.clone(), parsed));
                    }
                }
                "codex" => {
                    self.codex_index.insert(a.name.clone(), codex::read_index(&home));
                    let files = codex::rollout_files(&home, since);
                    for (path, parsed) in parallel_map(files, |(p, m)| {
                        let parsed = codex::parse_rollout(&p, m, None);
                        Some((p, parsed))
                    }) {
                        self.rollouts.insert(path, (a.name.clone(), parsed));
                    }
                }
                _ => {}
            }
        }
    }

    fn session_cwds(&self) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        for entries in self.registry.values() {
            out.extend(entries.iter().map(|e| e.cwd.clone()));
        }
        out.extend(self.transcripts.values().map(|(_, (_, t, _))| t.cwd.clone()));
        out.extend(self.rollouts.values().filter_map(|(_, t)| t.as_ref().map(|(t, _)| t.cwd.clone())));
        out
    }

    fn load_repos(&mut self) {
        let h = home();
        let roots: Vec<PathBuf> = self.cfg.scan.iter().map(|s| expand(s)).collect();
        let mut found = repos::discover(&roots, 3);
        let known: HashSet<PathBuf> = found.iter().map(|c| c.root.clone()).collect();
        let mut extra = BTreeSet::new();
        for cwd in self.session_cwds() {
            if let Some((_, main)) = repos::locate(Path::new(&cwd)) {
                if !known.contains(&main) {
                    extra.insert(main);
                }
            }
        }
        found.extend(extra.iter().filter_map(|r| repos::load_checkout(r)));

        self.checkouts.clear();
        for c in found {
            let Some(identity) = c.identity.clone() else { continue };
            if let Some(p) = self.cfg.projects.iter().find(|p| p.repos.iter().any(|pat| repos::matches(&identity, pat))) {
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
        for (project, c) in &self.checkouts {
            let Some((worktrees, base)) = listed.get(&c.root).cloned() else { continue };
            self.default_branch.insert(c.root.clone(), base.clone());
            let default_name = base.as_ref().map(|b| b.split_once('/').map(|(_, n)| n.to_string()).unwrap_or_else(|| b.clone()));
            let repo_name = c.github.as_ref().map(|(_, n)| n.clone()).unwrap_or_else(|| c.name.clone());
            for wt in worktrees {
                if wt.path != c.root {
                    if let Some(gitdir) = linked_gitdir(&wt.path) {
                        self.gitdir_to_lane.insert(gitdir, wt.path.clone());
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
        // Elsewhere: repos your agents worked in that belong to no project. Plain folders
        // (Codex chat folders, app threads) aren't lanes.
        let mut elsewhere = BTreeSet::new();
        for cwd in self.session_cwds() {
            let p = Path::new(&cwd);
            if self.not_repos.contains(p) || board::best_seed(&seeds, p).is_some() || !p.is_dir() {
                continue;
            }
            match repos::locate(p) {
                Some((wt, _)) => {
                    elsewhere.insert(wt);
                }
                None => {
                    self.not_repos.insert(p.to_path_buf());
                }
            }
        }
        for root in elsewhere {
            let name = if root == h { "home".to_string() } else { file_name(&root) };
            seeds.push(folder_seed(root, ELSEWHERE, &name, true));
        }

        self.ignores.clear();
        for s in &seeds {
            if s.origin != Origin::Folder && !s.elsewhere {
                let mut b = GitignoreBuilder::new(&s.path);
                b.add(s.path.join(".gitignore"));
                if let Ok(gi) = b.build() {
                    self.ignores.insert(s.path.clone(), gi);
                }
            }
        }
        let live: HashSet<&PathBuf> = seeds.iter().map(|s| &s.path).collect();
        self.git.retain(|p, _| live.contains(p));
        self.seeds = seeds;
    }

    fn git_targets(&self) -> Vec<PathBuf> {
        self.seeds.iter().filter(|s| !s.elsewhere && s.origin != Origin::Folder).map(|s| s.path.clone()).collect()
    }

    fn base_for(&self, path: &Path) -> Option<String> {
        self.lane_base.get(path).cloned().flatten()
    }

    fn queue_git(&mut self, path: PathBuf) {
        if self.git_inflight.contains(&path) {
            self.git_again.insert(path);
            return;
        }
        let base = self.base_for(&path);
        self.git_inflight.insert(path.clone());
        let _ = self.git_jobs.send((path, base));
    }

    fn queue_git_all(&mut self) {
        for p in self.git_targets() {
            self.queue_git(p);
        }
    }

    fn on_git(&mut self, path: PathBuf, info: GitInfo) {
        self.git_inflight.remove(&path);
        if self.seeds.iter().any(|s| s.path == path) {
            self.git.insert(path.clone(), info);
            self.dirty = true;
        }
        if self.git_again.remove(&path) {
            self.queue_git(path);
        }
    }

    fn github_slugs(&self) -> BTreeSet<String> {
        self.seeds
            .iter()
            .filter(|s| s.branch.is_some() && s.branch != s.default_branch)
            .filter_map(|s| s.repo_slug.clone())
            .collect()
    }

    fn fetch_github(&mut self, force: bool) {
        if self.gh_inflight {
            return;
        }
        if !force && self.gh_last.map(|t| t.elapsed() < Duration::from_secs(20)).unwrap_or(false) {
            return;
        }
        let slugs = self.github_slugs();
        if slugs.is_empty() {
            return;
        }
        self.gh_inflight = true;
        self.gh_last = Some(Instant::now());
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            // One request per repo, all at once; each result is applied as it lands.
            std::thread::scope(|scope| {
                for slug in &slugs {
                    let tx = tx.clone();
                    scope.spawn(move || {
                        let Some((owner, name)) = slug.split_once('/') else { return };
                        let result = github::fetch(owner, name).map_err(|e| e.to_string());
                        let _ = tx.send(Msg::GithubDone(slug.clone(), result));
                    });
                }
            });
            let _ = tx.send(Msg::GithubDone(String::new(), Ok(Vec::new())));
        });
    }

    fn on_github(&mut self, slug: String, result: std::result::Result<Vec<PrRaw>, String>) {
        if slug.is_empty() {
            self.gh_inflight = false;
            return;
        }
        match result {
            Ok(list) => {
                self.prs.insert(slug, list);
                self.gh_warning = None;
                self.gh_at = Some(now_ms());
            }
            Err(e) => self.gh_warning = Some(e),
        }
        self.dirty = true;
    }

    fn refresh_registry(&mut self, account: &str) -> bool {
        let Some(a) = self.account(account) else { return false };
        let fresh = claude::read_registry(&a.home_path());
        let changed = self.registry.get(account) != Some(&fresh);
        self.registry.insert(account.to_string(), fresh);
        changed
    }

    fn tick(&mut self) {
        let claude_accounts: Vec<String> = self.cfg.accounts.iter().filter(|a| a.harness == "claude").map(|a| a.name.clone()).collect();
        for a in claude_accounts {
            if self.refresh_registry(&a) {
                self.dirty = true;
            }
        }
        if self.gh_last.map(|t| t.elapsed() >= GITHUB_EVERY).unwrap_or(true) {
            self.fetch_github(true);
        }
        let every = if self.usage_backoff { USAGE_BACKOFF } else { USAGE_EVERY };
        if self.usage_last.map(|t| t.elapsed() >= every).unwrap_or(true) {
            self.fetch_usage();
        }
        // A new account's folder appears when it first signs in; start reading it.
        if self.missing_homes.iter().any(|p| p.exists()) {
            self.reload();
        }
    }

    fn classify_repo_path(&self, p: &Path) -> RepoEvent {
        for (_, c) in &self.checkouts {
            if let Ok(rel) = p.strip_prefix(&c.git_dir) {
                let mut comps = rel.components().map(|c| c.as_os_str().to_string_lossy().into_owned());
                return match comps.next().as_deref() {
                    Some("worktrees") => match comps.next() {
                        Some(name) => match self.gitdir_to_lane.get(&c.git_dir.join("worktrees").join(&name)) {
                            Some(lane) => RepoEvent::Lane(lane.clone()),
                            None => RepoEvent::Worktrees,
                        },
                        None => RepoEvent::Worktrees,
                    },
                    Some("refs") | Some("packed-refs") | Some("FETCH_HEAD") => RepoEvent::Checkout(c.root.clone()),
                    Some("HEAD") | Some("index") | Some("ORIG_HEAD") | Some("MERGE_HEAD") => RepoEvent::Lane(c.root.clone()),
                    _ => RepoEvent::Ignore,
                };
            }
        }
        let Some(i) = board::best_seed(&self.seeds, p) else { return RepoEvent::Ignore };
        let seed = &self.seeds[i];
        if seed.elsewhere || seed.origin == Origin::Folder {
            return RepoEvent::Ignore;
        }
        if p.components().any(|c| c.as_os_str() == "node_modules" || c.as_os_str() == ".git") {
            return RepoEvent::Ignore;
        }
        if let Some(gi) = self.ignores.get(&seed.path) {
            if gi.matched_path_or_any_parents(p, p.is_dir()).is_ignore() {
                return RepoEvent::Ignore;
            }
        }
        RepoEvent::Lane(seed.path.clone())
    }

    fn apply_fs(&mut self, mut paths: Vec<PathBuf>) {
        paths.sort();
        paths.dedup();
        let config_path = config::config_path();
        let accounts = self.cfg.accounts.clone();
        let mut registries = BTreeSet::new();
        let mut indexes = BTreeSet::new();
        let mut transcripts = Vec::new();
        let mut rollouts = Vec::new();
        let mut lanes = BTreeSet::new();
        let mut reload_repos = false;
        let mut reload_config = false;
        'paths: for p in paths {
            if p == config_path {
                reload_config = true;
                continue;
            }
            for a in &accounts {
                let home = a.home_path();
                let jsonl = p.extension().and_then(|x| x.to_str()) == Some("jsonl");
                match a.harness.as_str() {
                    "claude" => {
                        if p.starts_with(home.join("sessions")) {
                            registries.insert(a.name.clone());
                            continue 'paths;
                        }
                        let projects = home.join("projects");
                        if p.starts_with(&projects) {
                            if jsonl && p.parent().and_then(Path::parent) == Some(projects.as_path()) {
                                transcripts.push((a.name.clone(), p));
                            }
                            continue 'paths;
                        }
                    }
                    "codex" => {
                        if p == home.join("session_index.jsonl") {
                            indexes.insert(a.name.clone());
                            continue 'paths;
                        }
                        if p.starts_with(home.join("sessions")) {
                            if jsonl {
                                rollouts.push((a.name.clone(), p));
                            }
                            continue 'paths;
                        }
                        if p.starts_with(&home) {
                            continue 'paths;
                        }
                    }
                    _ => {}
                }
            }
            match self.classify_repo_path(&p) {
                RepoEvent::Lane(l) => {
                    lanes.insert(l);
                }
                RepoEvent::Checkout(root) => {
                    lanes.extend(self.seeds.iter().filter(|s| self.lane_checkout.get(&s.path) == Some(&root)).map(|s| s.path.clone()));
                }
                RepoEvent::Worktrees => reload_repos = true,
                RepoEvent::Ignore => {}
            }
        }

        if reload_config {
            self.reload();
            return;
        }
        let mut registry_changed = false;
        for a in registries {
            if self.refresh_registry(&a) {
                self.dirty = true;
                registry_changed = true;
            }
        }
        let mut sessions_changed = registry_changed;
        for (a, p) in transcripts {
            let prev = self.transcripts.remove(&p).map(|(_, parsed)| parsed);
            let is_new = prev.is_none();
            let from = prev.as_ref().map(|(_, _, st)| st.offset);
            if let Some(parsed) = mtime_ms(&p).and_then(|m| claude::parse_transcript(&p, m, prev)) {
                if let Some(from) = from {
                    let key = format!("claude:{}:{}", a, parsed.1.session_id);
                    self.check_new_lines(&p, from, parsed.2.offset, &key);
                }
                self.transcripts.insert(p, (a, parsed));
            }
            sessions_changed |= is_new;
            self.dirty = true;
        }
        for a in indexes {
            if let Some(acct) = self.account(&a) {
                let index = codex::read_index(&acct.home_path());
                self.codex_index.insert(a, index);
                self.dirty = true;
            }
        }
        for (a, p) in rollouts {
            let prev = match self.rollouts.get(&p) {
                // A thread that's never shown: skip its writes.
                Some((_, None)) => continue,
                Some((_, Some(parsed))) => Some(parsed.clone()),
                None => None,
            };
            let is_new = prev.is_none();
            let from = prev.as_ref().map(|(_, off)| *off);
            match mtime_ms(&p) {
                Some(m) => {
                    let parsed = codex::parse_rollout(&p, m, prev);
                    sessions_changed |= is_new && parsed.is_some();
                    if let (Some(from), Some((th, to))) = (from, parsed.as_ref()) {
                        let key = format!("codex:{}:{}", a, th.id);
                        self.check_new_lines(&p, from, *to, &key);
                    }
                    self.rollouts.insert(p, (a, parsed));
                }
                None => {
                    self.rollouts.remove(&p);
                }
            }
            self.dirty = true;
        }
        // A new session may have started somewhere no lane covers yet.
        let unplaced = sessions_changed
            && self.session_cwds().into_iter().any(|cwd| {
                let p = Path::new(&cwd);
                !self.not_repos.contains(p) && board::best_seed(&self.seeds, p).is_none() && p.is_dir() && repos::locate(p).is_some()
            });
        if reload_repos || unplaced {
            let before: HashSet<PathBuf> = self.seeds.iter().map(|s| s.path.clone()).collect();
            self.load_repos();
            let added: Vec<PathBuf> = self.git_targets().into_iter().filter(|p| !before.contains(p)).collect();
            for p in added {
                self.queue_git(p);
            }
            self.watch();
            self.refresh_visitors(false);
            self.dirty = true;
        }
        for l in lanes {
            self.queue_git(l);
        }
    }

    /// Worktrees an agent made from somewhere else (temporary folders, Codex worktrees) that
    /// no session runs in, and the text that identifies each in a log: its folder name, or
    /// its path under `~/.codex/worktrees`.
    fn orphan_needles(&self) -> Vec<(PathBuf, String)> {
        let cwds: Vec<String> = self.session_cwds().into_iter().collect();
        let h = home();
        self.seeds
            .iter()
            .filter(|s| matches!(s.origin, Origin::Temp | Origin::Codex | Origin::Other))
            .filter(|s| !cwds.iter().any(|c| board::best_seed(&self.seeds, Path::new(c)).map(|i| self.seeds[i].path == s.path).unwrap_or(false)))
            .filter_map(|s| {
                let needle = match s.path.strip_prefix(&h) {
                    Ok(rel) if s.origin == Origin::Codex => rel.to_string_lossy().into_owned(),
                    _ => file_name(&s.path),
                };
                (needle.len() >= 12).then(|| (s.path.clone(), needle))
            })
            .collect()
    }

    /// Searches session logs for orphan worktrees. Only files written since the oldest orphan
    /// was made are read, in the background unless `sync`.
    fn refresh_visitors(&mut self, sync: bool) {
        let needles = self.orphan_needles();
        if needles == self.needles && !sync {
            return;
        }
        self.needles = needles.clone();
        if needles.is_empty() {
            self.visitors.clear();
            return;
        }
        let since = needles
            .iter()
            .filter_map(|(p, _)| std::fs::metadata(p).and_then(|m| m.created()).ok())
            .filter_map(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as i64)
            .min()
            .unwrap_or(0)
            - 3_600_000;
        let mut files: Vec<(PathBuf, String)> = Vec::new();
        for (p, (a, (_, info, _))) in &self.transcripts {
            files.push((p.clone(), format!("claude:{}:{}", a, info.session_id)));
        }
        for (p, (a, parsed)) in &self.rollouts {
            if let Some((th, _)) = parsed {
                files.push((p.clone(), format!("codex:{}:{}", a, th.id)));
            }
        }
        files.retain(|(p, _)| mtime_ms(p).map(|m| m >= since).unwrap_or(false));
        let scan = move || -> HashMap<PathBuf, Vec<String>> {
            let found = parallel_map(files, |(path, key)| {
                let hits = file_mentions(&path, &needles);
                (!hits.is_empty()).then(|| (key, hits))
            });
            // Earliest work first: usually the session that made the worktree.
            let mut timed: HashMap<PathBuf, Vec<(i64, String)>> = HashMap::new();
            for (key, lanes) in found {
                for (lane, at) in lanes {
                    timed.entry(lane).or_default().push((at, key.clone()));
                }
            }
            timed
                .into_iter()
                .map(|(lane, mut v)| {
                    v.sort();
                    (lane, v.into_iter().map(|(_, k)| k).collect())
                })
                .collect()
        };
        if sync {
            self.visitors = scan();
        } else {
            let tx = self.tx.clone();
            std::thread::spawn(move || {
                let _ = tx.send(Msg::Visitors(scan()));
            });
        }
    }

    /// Checks lines a session just wrote for orphan worktrees.
    fn check_new_lines(&mut self, path: &Path, from: u64, to: u64, key: &str) {
        if self.needles.is_empty() || to <= from {
            return;
        }
        let Some(text) = read_range_lossy(path, from, to) else { return };
        for (lane, needle) in &self.needles {
            if worked_in(&text, needle) {
                let keys = self.visitors.entry(lane.clone()).or_default();
                if !keys.iter().any(|k| k == key) {
                    keys.push(key.to_string());
                    self.dirty = true;
                }
            }
        }
    }

    fn watch(&mut self) {
        let tx = self.tx.clone();
        let handler = move |res: notify::Result<notify::Event>| {
            if let Ok(ev) = res {
                if matches!(ev.kind, EventKind::Access(_)) {
                    return;
                }
                let paths: Vec<PathBuf> = ev.paths.into_iter().filter(|p| !is_noise(p)).collect();
                if !paths.is_empty() {
                    let _ = tx.send(Msg::Fs(paths));
                }
            }
        };
        let mut watcher = match notify::recommended_watcher(handler) {
            Ok(w) => w,
            Err(e) => {
                self.watch_warning = Some(format!("Live updates are off: {e}"));
                return;
            }
        };
        let mut paths: Vec<(PathBuf, RecursiveMode)> = Vec::new();
        self.missing_homes = self.cfg.accounts.iter().map(|a| a.home_path()).filter(|h| !h.exists()).collect();
        let _ = std::fs::create_dir_all(config::support_dir());
        paths.push((config::support_dir(), RecursiveMode::NonRecursive));
        for a in &self.cfg.accounts {
            let home = a.home_path();
            match a.harness.as_str() {
                "claude" => {
                    paths.push((home.join("sessions"), RecursiveMode::NonRecursive));
                    paths.push((home.join("projects"), RecursiveMode::Recursive));
                }
                "codex" => {
                    paths.push((home.clone(), RecursiveMode::NonRecursive));
                    paths.push((home.join("sessions"), RecursiveMode::Recursive));
                }
                _ => {}
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
        let mut failed = 0;
        for (p, mode) in paths {
            if p.exists() && watcher.watch(&p, mode).is_err() {
                failed += 1;
            }
        }
        self.watch_warning = (failed > 0).then(|| format!("Couldn't watch {failed} folders; their lanes update every 10 seconds."));
        self.watcher = Some(watcher);
    }

    /// What a stuck session's shell is running, read once per stall.
    fn stuck_command(&self, r: &RegistryEntry) -> Option<String> {
        let mut cache = self.stuck.lock().ok()?;
        cache.entry((r.pid, r.status_updated_at.unwrap_or(0))).or_insert_with(|| crate::procs::running_command(r.pid)).clone()
    }

    fn build_sessions(&self, now: i64) -> Vec<Session> {
        let mut out = Vec::new();
        for a in &self.cfg.accounts {
            match a.harness.as_str() {
                "claude" => {
                    let mut by_id: HashMap<&str, (Option<&RegistryEntry>, Option<(&Path, &TranscriptInfo)>)> = HashMap::new();
                    for r in self.registry.get(&a.name).map(|v| v.as_slice()).unwrap_or(&[]) {
                        by_id.entry(r.session_id.as_str()).or_default().0 = Some(r);
                    }
                    for (path, (acct, (_, t, _))) in &self.transcripts {
                        if acct == &a.name {
                            let slot = by_id.entry(t.session_id.as_str()).or_default();
                            if slot.1.map(|(_, old)| old.last_ts < t.last_ts).unwrap_or(true) {
                                slot.1 = Some((path.as_path(), t));
                            }
                        }
                    }
                    for (id, (reg, tr)) in by_id {
                        let key = format!("claude:{}:{}", a.name, id);
                        let info = tr.map(|(_, t)| t);
                        // Only a busy session that's gone quiet costs a look outside its transcript.
                        let (subagent_at, running) = match board::claude_busy_since(reg, info) {
                            Some(since) if now - since > board::CLAUDE_STALL_MS => {
                                let sub = tr.and_then(|(p, _)| subagent_activity(p, id));
                                let quiet = now - since.max(sub.unwrap_or(0)) > board::CLAUDE_STALL_MS;
                                (sub, if quiet { reg.and_then(|r| self.stuck_command(r)) } else { None })
                            }
                            _ => (None, None),
                        };
                        let quiet = board::Quiet { subagent_at, running: running.as_deref() };
                        if let Some(s) = board::claude_session(a, reg, info, self.seen.seen_at(&key), now, &quiet) {
                            out.push(s);
                        }
                    }
                }
                "codex" => {
                    let index = self.codex_index.get(&a.name);
                    for (acct, info) in self.rollouts.values() {
                        let (true, Some((th, _))) = (acct == &a.name, info.as_ref()) else { continue };
                        let key = format!("codex:{}:{}", a.name, th.id);
                        let name = index.and_then(|i| i.get(&th.id)).map(|(n, _)| n.as_str());
                        out.push(board::codex_session(a, th, name, self.seen.seen_at(&key), now));
                    }
                }
                _ => {}
            }
        }
        out
    }

    fn compose(&self) -> Board {
        let now = now_ms();
        let apps = AppNames {
            terminal: launch::terminal_name(&self.cfg.apps.terminal),
            terminal_id: self.cfg.apps.terminal.clone(),
            editor: launch::editor_name(&self.cfg.apps.editor),
            editor_id: self.cfg.apps.editor.clone(),
        };
        let order: Vec<String> = self.cfg.projects.iter().map(|p| p.name.clone()).collect();
        let warnings = [&self.config_warning, &self.gh_warning, &self.watch_warning].into_iter().flatten().cloned().collect();
        let mut board = board::compose(board::Inputs {
            now,
            apps: &apps,
            seeds: &self.seeds,
            sessions: self.build_sessions(now),
            git: &self.git,
            prs: &self.prs,
            project_order: &order,
            warnings,
            loading: self.loading,
            github_at: self.gh_at,
            visitors: &self.visitors,
        });
        board.usage = self.compose_usage(now);
        board
    }

    fn compose_usage(&self, now: i64) -> Vec<Usage> {
        self.cfg
            .accounts
            .iter()
            .map(|a| match a.harness.as_str() {
                "codex" => {
                    let reports = self.rollouts.values().filter(|(acct, _)| acct == &a.name).filter_map(|(_, p)| p.as_ref().map(|(th, _)| &th.rate_limits));
                    usage::codex(a, reports, now)
                }
                _ => self.usage.get(&a.name).cloned().unwrap_or_else(|| usage::loading(a)),
            })
            .collect()
    }

    fn publish(&mut self) {
        let board = self.compose();
        self.dirty = false;
        self.last_publish = Some(Instant::now());
        let hash = board_hash(&board);
        if hash == self.last_hash {
            return;
        }
        self.last_hash = hash;
        (self.emit)(&board);
        if let Ok(mut slot) = self.board.write() {
            *slot = board;
        }
    }
}

/// Which lanes a file shows work in, each with when that work first appears.
fn file_mentions(path: &Path, needles: &[(PathBuf, String)]) -> Vec<(PathBuf, i64)> {
    use std::io::Read;
    let Ok(mut f) = std::fs::File::open(path) else { return vec![] };
    let fallback = mtime_ms(path).unwrap_or(0);
    let overlap = needles.iter().map(|(_, n)| n.len()).max().unwrap_or(0);
    let mut hits: Vec<(PathBuf, i64)> = Vec::new();
    let mut carry: Vec<u8> = Vec::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = match f.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        carry.extend_from_slice(&buf[..n]);
        let text = String::from_utf8_lossy(&carry);
        for (lane, needle) in needles {
            if !hits.iter().any(|(l, _)| l == lane) {
                if let Some(line) = work_line(&text, needle) {
                    hits.push((lane.clone(), line_time(line).unwrap_or(fallback)));
                }
            }
        }
        if hits.len() == needles.len() {
            break;
        }
        let keep = carry.len().saturating_sub(overlap);
        carry.drain(..keep);
    }
    hits
}

/// True when some line in `text` shows work in the folder `needle` names: creating it with
/// `git worktree add`, or running commands in it (a `workdir`/`cwd` field, or `cd` into it).
/// A passing mention, like a search or a listing, doesn't count.
pub(crate) fn worked_in(text: &str, needle: &str) -> bool {
    work_line(text, needle).is_some()
}

/// The first line in `text` that shows work in the folder `needle` names.
fn work_line<'a>(text: &'a str, needle: &str) -> Option<&'a str> {
    const MARKERS: [&str; 6] = ["\"workdir\":\"", "\\\"workdir\\\":\\\"", "\"cwd\":\"", "\\\"cwd\\\":\\\"", "cd ", "worktree add "];
    let mut from = 0;
    while let Some(i) = text[from..].find(needle) {
        let at = from + i;
        let line_start = text[..at].rfind('\n').map(|j| j + 1).unwrap_or(0);
        let line_end = text[at..].find('\n').map(|j| at + j).unwrap_or(text.len());
        let line = &text[line_start..line_end];
        for marker in MARKERS {
            let mut m = 0;
            while let Some(k) = line[m..].find(marker) {
                let path_start = m + k + marker.len();
                let path: String = line[path_start..].chars().take_while(|c| !matches!(c, '"' | ' ' | '\\' | '&' | ';' | '\'' | '|')).collect();
                let path = path.trim_start_matches("-b ").to_string();
                if path.contains(needle) {
                    return Some(line);
                }
                // `git worktree add -b <branch> <path>`: the path comes after the branch.
                if marker == "worktree add " && line[path_start..].split_whitespace().take(4).any(|w| w.contains(needle)) {
                    return Some(line);
                }
                m = path_start;
            }
        }
        from = at + needle.len();
    }
    None
}

/// The `timestamp` of a log line, if it has one.
fn line_time(line: &str) -> Option<i64> {
    let start = line.find("\"timestamp\":\"")? + "\"timestamp\":\"".len();
    let end = line[start..].find('"')? + start;
    crate::util::parse_rfc3339_ms(&line[start..end])
}

fn read_range_lossy(path: &Path, from: u64, to: u64) -> Option<String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(path).ok()?;
    f.seek(SeekFrom::Start(from)).ok()?;
    let mut buf = Vec::new();
    f.take(to - from).read_to_end(&mut buf).ok()?;
    Some(String::from_utf8_lossy(&buf).into_owned())
}

/// A hash of everything the UI shows except the timestamp, so identical boards aren't resent.
fn board_hash(board: &Board) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    let mut copy = board.clone();
    copy.generated_at = 0;
    serde_json::to_string(&copy).unwrap_or_default().hash(&mut h);
    h.finish()
}

/// Paths that never matter, dropped before they reach the engine: databases and logs that
/// apps write constantly, dependency folders, git's object store.
fn is_noise(p: &Path) -> bool {
    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or_default();
    if name.ends_with("-wal") || name.ends_with("-shm") || name.ends_with("-journal") || name.ends_with(".sqlite") || name.ends_with(".log") || name.ends_with(".lock") || name == ".DS_Store" {
        return true;
    }
    p.components().any(|c| {
        let c = c.as_os_str();
        c == "node_modules" || c == "objects" || c == "target" || c == ".nx" || c == ".turbo" || c == ".next"
    })
}

fn folder_seed(path: PathBuf, project: &str, name: &str, elsewhere: bool) -> LaneSeed {
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
    }
}

/// `<main>/.git/worktrees/<name>` for a linked worktree, from its `.git` file.
fn linked_gitdir(worktree: &Path) -> Option<PathBuf> {
    let text = std::fs::read_to_string(worktree.join(".git")).ok()?;
    let gitdir = PathBuf::from(text.trim().strip_prefix("gitdir:")?.trim());
    Some(std::fs::canonicalize(&gitdir).unwrap_or(gitdir))
}

fn spawn_git_workers(back: Sender<Msg>) -> Sender<(PathBuf, Option<String>)> {
    let (jobs, rx) = channel::<(PathBuf, Option<String>)>();
    let rx = Arc::new(Mutex::new(rx));
    for i in 0..GIT_WORKERS {
        let rx = rx.clone();
        let back = back.clone();
        let _ = std::thread::Builder::new().name(format!("codealong-git-{i}")).spawn(move || loop {
            let job = match rx.lock() {
                Ok(r) => r.recv(),
                Err(_) => return,
            };
            let Ok((path, base)) = job else { return };
            let info = git::status(&path, base.as_deref());
            if back.send(Msg::GitDone(path, info)).is_err() {
                return;
            }
        });
    }
    jobs
}

/// First run: find accounts, then suggest a project per GitHub owner your agents worked in.
fn first_run_config() -> Config {
    let accounts = config::discover_accounts();
    let since = now_ms() - 30 * DAY_MS;
    let mut cwds: Vec<String> = Vec::new();
    for a in &accounts {
        let home = a.home_path();
        match a.harness.as_str() {
            "claude" => {
                let files = claude::transcript_files(&home, since);
                cwds.extend(parallel_map(files, |(p, _)| claude::transcript_cwd(&p)));
            }
            "codex" => {
                let files = codex::rollout_files(&home, since);
                cwds.extend(parallel_map(files, |(p, _)| {
                    read_first_line(&p, 4 * 1024 * 1024).and_then(|l| codex::parse_meta(&l)).map(|m| m.cwd)
                }));
            }
            _ => {}
        }
    }
    let mut per_repo: BTreeMap<PathBuf, usize> = BTreeMap::new();
    for cwd in cwds {
        if let Some((_, main)) = repos::locate(Path::new(&cwd)) {
            *per_repo.entry(main).or_default() += 1;
        }
    }
    let mut owners: BTreeMap<String, usize> = BTreeMap::new();
    for (root, n) in per_repo {
        if n < 2 {
            continue;
        }
        if let Some((owner, _)) = repos::load_checkout(&root).and_then(|c| c.github) {
            *owners.entry(owner).or_default() += n;
        }
    }
    Config { accounts, projects: config::suggest_projects(&owners, github::display_name), ..Config::default() }
}

#[cfg(test)]
mod tests {
    fn usage(error: Option<&str>, needs_sign_in: bool, percent: f64) -> Usage {
        Usage {
            account: "claude".into(),
            harness: Harness::Claude,
            plan: Some("Max 20x".into()),
            meters: if error.is_some() {
                vec![]
            } else {
                vec![Meter { key: "week".into(), label: "Week".into(), window: "Weekly".into(), percent, resets_at: None, severity: Severity::Normal, model: None }]
            },
            as_of: if error.is_some() { None } else { Some(1) },
            error: error.map(str::to_string),
            needs_sign_in,
            loading: false,
            rate_limited: false,
        }
    }

    #[test]
    fn a_failed_usage_read_keeps_the_last_numbers() {
        let good = usage(None, false, 47.0);
        let kept = keep_last_good(Some(&good), usage(Some("Couldn't read usage: HTTP 500"), false, 0.0));
        assert_eq!(kept.meters, good.meters);
        assert_eq!(kept.as_of, Some(1));
        assert!(kept.error.is_some());
        // Signing out isn't papered over.
        let signed_out = keep_last_good(Some(&good), usage(Some("Sign-in expired"), true, 0.0));
        assert!(signed_out.meters.is_empty());
    }

    use super::{keep_last_good, worked_in};
    use crate::model::{Harness, Meter, Severity, Usage};

    #[test]
    fn counts_real_work_not_mentions() {
        let n = "atlas-validate-ghostscript-20260923";
        assert!(worked_in(r#"{"cmd":"git worktree add /tmp/atlas-validate-ghostscript-20260923 origin/main"}"#, n));
        assert!(worked_in(r#"{"cmd":"git worktree add -b codex/x /tmp/atlas-validate-ghostscript-20260923"}"#, n));
        assert!(worked_in(r#"{"arguments":"{\"cmd\":\"yarn test\",\"workdir\":\"/private/tmp/atlas-validate-ghostscript-20260923\"}"}"#, n));
        assert!(worked_in(r#"{"command":"cd /private/tmp/atlas-validate-ghostscript-20260923 && git status"}"#, n));
        assert!(!worked_in(r#"{"command":"cd ~/projects/codealong && grep -F atlas-validate-ghostscript-20260923 logs"}"#, n));
        assert!(!worked_in(r#"{"content":"/private/tmp/atlas-validate-ghostscript-20260923  aad4df0 [codex/x]"}"#, n));
        assert_eq!(super::line_time(r#"{"timestamp":"1970-01-01T00:00:01.000Z","cmd":"x"}"#), Some(1000));
    }
}
