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
use crate::antigravity;
use crate::appserver;
use crate::claude::{self, RegistryEntry, TranscriptInfo};
use crate::claudeapp;
use crate::codex::{self, ThreadInfo};
use crate::opencode;
use crate::config::{self, AccountCfg, Config, Theme};
use crate::detect;
use crate::git;
use crate::github::{self, PrRaw};
use crate::jev;
use crate::launch;
use crate::model::*;
use crate::perf;
use crate::procs;
use crate::repos::{self, Checkout};
use crate::state::SeenState;
use crate::throttle::{self, GitQueue, Verdict};
use crate::todos::{self, TodoOp, TodoStore};
use crate::attachments;
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
    GithubDone(String, std::result::Result<github::RepoPrs, String>),
    RefreshGithub,
    MarkSeen(Vec<String>),
    /// Lane path → session keys whose logs mention it.
    Visitors(HashMap<PathBuf, Vec<String>>),
    RefreshUsage,
    UsageDone(Vec<Usage>),
    /// What each OpenCode account's background service said, or None where it isn't running.
    OpencodeLive(Vec<(String, Option<opencode::Live>)>),
    /// The full pinned list, in order.
    SetPinned(Vec<String>),
    /// The full list of pinned worktrees (lane ids), in order.
    SetPinnedWorktrees(Vec<String>),
    /// A session archived (true) or unarchived from Codealong; Codex has already done its part.
    SetArchived(String, bool),
    /// The theme, already saved to config.toml.
    SetTheme(Theme),
    /// Sent every second while the perf log is on; how late it's read is the engine's lag.
    Ping(Instant),
    /// A change to a todo or to the labels, from a window. The reply is the time of the board
    /// that shows it.
    Todo(TodoOp, Sender<std::result::Result<i64, String>>),
    /// A session started from a todo: its worktree (none for a new one), and the session's key
    /// when Codealong chose its ID, or the start to wait for.
    TodoStarted { id: String, lane: Option<String>, session: Option<String>, pending: Option<todos::PendingStart> },
    /// Jev's answer to a question the engine asked (`ask_jev`), by the id it was asked under.
    JevDone(String, std::result::Result<serde_json::Value, String>),
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

    pub fn set_pinned(&self, keys: Vec<String>) {
        let _ = self.tx.send(Msg::SetPinned(keys));
    }

    pub fn set_pinned_worktrees(&self, ids: Vec<String>) {
        let _ = self.tx.send(Msg::SetPinnedWorktrees(ids));
    }

    /// A change to a todo or the labels. Returns once a board that shows it has gone out, with
    /// that board's time, so a window knows when its own copy of the change can go.
    pub fn todo(&self, op: TodoOp) -> std::result::Result<i64, String> {
        self.todo_within(op, Duration::from_secs(20))
    }

    /// `todo`, waiting as long as the caller can: the socket (server.rs) waits longer than a
    /// window, since a command has nothing else to show meanwhile and the engine answers late
    /// while it scans at launch.
    pub fn todo_within(&self, op: TodoOp, wait: Duration) -> std::result::Result<i64, String> {
        let (tx, rx) = channel();
        self.tx.send(Msg::Todo(op, tx)).map_err(|_| "Codealong's engine stopped.".to_string())?;
        rx.recv_timeout(wait).map_err(|_| "Codealong is busy; the change will show when it catches up.".to_string())?
    }

    /// Links a session started from a todo, or waits for it to show up.
    pub fn todo_started(&self, id: String, lane: Option<String>, session: Option<String>, pending: Option<todos::PendingStart>) {
        let _ = self.tx.send(Msg::TodoStarted { id, lane, session, pending });
    }

    /// Archives or unarchives a session. Codex threads are archived in Codex itself, so the
    /// Codex app shows it too; neither Claude Code nor the Claude app lets another program
    /// archive, so a Claude session is archived in Codealong until it's active again.
    pub fn archive(&self, key: &str, archived: bool) -> Result<()> {
        let mut parts = key.splitn(3, ':');
        let (Some(harness), Some(account), Some(id)) = (parts.next(), parts.next(), parts.next()) else {
            bail!("That isn't a session.");
        };
        if harness == "codex" {
            let cfg = config::load()?.unwrap_or_default();
            let a = cfg.accounts.iter().find(|a| a.name == account).ok_or_else(|| anyhow!("There's no account named {account}."))?;
            appserver::set_archived(&a.home_path(), id, archived)?;
        }
        let _ = self.tx.send(Msg::SetArchived(key.to_string(), archived));
        Ok(())
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

    /// Saves every setting to config.toml, in place. The watcher sees the change and reloads;
    /// the theme is published at once, as the theme buttons do.
    pub fn save_config(&self, cfg: &config::Config) -> Result<()> {
        config::save(cfg)?;
        let _ = self.tx.send(Msg::SetTheme(cfg.appearance.theme));
        Ok(())
    }

    /// Saves config.toml as typed in Settings, once it parses.
    pub fn save_config_text(&self, text: &str) -> Result<()> {
        config::save_text(text)?;
        if let Ok(cfg) = config::parse(text) {
            let _ = self.tx.send(Msg::SetTheme(cfg.appearance.theme));
        }
        Ok(())
    }

    /// Saves the theme to config.toml and publishes it at once, rather than when the watcher
    /// notices the file.
    pub fn set_theme(&self, theme: Theme) -> Result<()> {
        config::save_theme(theme)?;
        let _ = self.tx.send(Msg::SetTheme(theme));
        Ok(())
    }

    /// Opens a terminal window that signs the account in.
    pub fn sign_in(&self, name: &str) -> Result<()> {
        let cfg = config::load()?.unwrap_or_default();
        let a = cfg.accounts.iter().find(|a| a.name == name).ok_or_else(|| anyhow!("There's no account named {name}."))?;
        let place = launch::Placement { new_terminal: cfg.apps.new_terminal, near: &[] };
        launch::terminal_open(&cfg.apps.terminal, place, &home(), Some(&format!("{}\n", usage::sign_in_command(a))))
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
    let near = board::terminal_neighbours(&board.lanes, &path);
    let place = launch::Placement { new_terminal: apps.new_terminal, near: &near };
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
                    launch::terminal_open(&apps.terminal_id, place, &cwd, Some(&format!("{cmd}\n")))?;
                }
                "resume" => {
                    let cmd = s.resume_command.clone().ok_or_else(|| anyhow!("This session can't be resumed in a terminal."))?;
                    launch::terminal_open(&apps.terminal_id, place, &cwd, Some(&format!("{cmd}\n")))?;
                }
                "claude-app" => launch::open_url(&launch::claude_app_url(&s.id))?,
                "codex" => launch::open_codex(&launch::codex_thread_url(&s.id))?,
                "antigravity" => launch::open_antigravity()?,
                other => bail!("Unknown action {other:?}"),
            }
            return Ok(Some(s.key.clone()));
        }
        ["l", "terminal"] => launch::terminal_open(&apps.terminal_id, place, &path, None)?,
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
    // The theme is on the board before the first scan finishes, so windows open in it.
    let board = Arc::new(RwLock::new(Board { loading: true, theme: config::theme(), quick_shortcut: config::quick_shortcut(), ..Default::default() }));
    let handle = Handle { tx: tx.clone(), board: board.clone() };
    let ping = tx.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(1));
        if perf::enabled() && ping.send(Msg::Ping(Instant::now())).is_err() {
            return;
        }
    });
    std::thread::Builder::new()
        .name("codealong-engine".into())
        .spawn(move || {
            procs::utility_thread();
            let mut engine = Engine::new(Box::new(emit), tx, board);
            engine.full_scan();
            engine.run(rx);
        })
        .expect("engine thread");
    handle
}

/// One synchronous pass for `--snapshot`: everything the board needs, no watchers. Claude usage
/// is read only when asked: the usage endpoint rate-limits, and the app polls it already.
pub fn snapshot(with_github: bool, with_usage: bool) -> Board {
    let (tx, _rx) = channel();
    let mut e = Engine::new(Box::new(|_| {}), tx, Arc::new(RwLock::new(Board::default())));
    let t = Instant::now();
    let lap = |label: &str, t: &Instant| eprintln!("  {label:<10} {:>6} ms", t.elapsed().as_millis());
    e.load_config();
    e.seen = SeenState::load();
    e.load_todos();
    e.app_records.refresh(&claudeapp::records_dir());
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
    if with_usage {
        for u in usage::claude_all(&e.cfg.accounts) {
            e.usage.insert(u.account.clone(), u);
        }
        lap("usage", &t);
    }
    if with_github {
        let slugs: Vec<String> = e.github_slugs().into_iter().collect();
        let results = parallel_map(slugs, |slug| {
            let (owner, name) = slug.split_once('/')?;
            Some((slug.clone(), github::fetch(owner, name)))
        });
        for (slug, result) in results {
            match result {
                Ok(repo) => {
                    e.prs.insert(slug, repo.prs);
                    if repo.login.is_some() {
                        e.gh_login = repo.login;
                    }
                    e.gh_at = Some(now_ms());
                }
                Err(err) => e.gh_warning = Some(err.to_string()),
            }
        }
        lap("github", &t);
    }
    e.compose()
}

/// The board without git, GitHub, visitors or usage: sessions, worktrees and todos as the
/// files say, in a couple of seconds. For the `codealong` command when the app isn't running.
pub fn light_snapshot() -> Board {
    let (tx, _rx) = channel();
    let mut e = Engine::new(Box::new(|_| {}), tx, Arc::new(RwLock::new(Board::default())));
    e.load_config();
    e.seen = SeenState::load();
    e.load_todos();
    e.app_records.refresh(&claudeapp::records_dir());
    e.scan_sessions();
    e.load_repos();
    e.loading = false;
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
    /// Todos and labels, as in todos.json.
    todos: TodoStore,
    /// todos.json as last read or written, so the watcher can tell someone else's edit from ours.
    todos_text: String,
    /// Why todos.json couldn't be read or saved.
    todos_warning: Option<String>,
    /// todos.json is there but doesn't read as a todo list: nothing is written over it, and
    /// changes are turned away, until it's fixed or deleted.
    todos_unreadable: bool,
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
    /// OpenCode sessions by account, newest first.
    opencode_sessions: HashMap<String, Vec<opencode::SessionRow>>,
    /// What each OpenCode account's background service said last, while it was running.
    opencode_live: HashMap<String, opencode::Live>,
    /// OpenCode's interactive terminals, refreshed on the tick.
    opencode_terminals: Vec<opencode::Terminal>,
    opencode_inflight: bool,
    opencode_last: Option<Instant>,
    /// Antigravity conversations by account, newest first.
    antigravity: HashMap<String, Vec<antigravity::Conversation>>,
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
    /// Which worktrees git runs in, and which wait their turn (see throttle.rs).
    git_queue: GitQueue<PathBuf>,
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
    gh_login: Option<String>,
    watcher: Option<RecommendedWatcher>,
    watch_warning: Option<String>,
    /// Account folders that didn't exist at the last watch: a new account before sign-in.
    missing_homes: Vec<PathBuf>,
    /// Claude usage by account, from the last background read.
    usage: HashMap<String, Usage>,
    /// The Claude app's session records, for what you archived there.
    app_records: claudeapp::Records,
    app_records_at: Option<Instant>,
    /// (pid, status time) → the command a stuck Claude session's shell is running.
    stuck: Mutex<HashMap<(u32, i64), Option<String>>>,
    usage_inflight: bool,
    usage_last: Option<Instant>,
    usage_backoff: bool,
    dirty: bool,
    /// Only git results changed since the last publish; they go out in batches.
    dirty_git: bool,
    loading: bool,
    last_publish: Option<Instant>,
    perf: perf::Counters,
    /// Questions for Jev go to its own thread (jev.rs); answers come back as `Msg::JevDone`.
    jev_jobs: Sender<jev::Job>,
    /// Ids asked and not answered yet, so nothing is asked twice at once.
    jev_pending: HashSet<String>,
}

impl Engine {
    fn new(emit: Box<dyn Fn(&Board) + Send>, tx: Sender<Msg>, board: Arc<RwLock<Board>>) -> Engine {
        let git_jobs = spawn_git_workers(tx.clone());
        let back = tx.clone();
        let jev_jobs = jev::worker(move |id, answer| {
            let _ = back.send(Msg::JevDone(id, answer));
        });
        Engine {
            emit,
            tx,
            board,
            cfg: Config::default(),
            config_warning: None,
            seen: SeenState::default(),
            todos: TodoStore::default(),
            todos_text: String::new(),
            todos_warning: None,
            todos_unreadable: false,
            checkouts: Vec::new(),
            seeds: Vec::new(),
            gitdir_to_lane: HashMap::new(),
            ignores: HashMap::new(),
            registry: HashMap::new(),
            transcripts: HashMap::new(),
            codex_index: HashMap::new(),
            rollouts: HashMap::new(),
            opencode_sessions: HashMap::new(),
            opencode_live: HashMap::new(),
            opencode_terminals: Vec::new(),
            opencode_inflight: false,
            opencode_last: None,
            antigravity: HashMap::new(),
            not_repos: HashSet::new(),
            last_hash: 0,
            visitors: HashMap::new(),
            needles: Vec::new(),
            git: HashMap::new(),
            git_jobs,
            git_queue: GitQueue::default(),
            default_branch: HashMap::new(),
            lane_base: HashMap::new(),
            lane_checkout: HashMap::new(),
            prs: HashMap::new(),
            gh_warning: None,
            gh_inflight: false,
            gh_last: None,
            gh_at: None,
            gh_login: None,
            watcher: None,
            watch_warning: None,
            missing_homes: Vec::new(),
            usage: HashMap::new(),
            app_records: claudeapp::Records::default(),
            app_records_at: None,
            stuck: Mutex::new(HashMap::new()),
            usage_inflight: false,
            usage_last: None,
            usage_backoff: false,
            dirty: false,
            loading: true,
            dirty_git: false,
            last_publish: None,
            perf: perf::Counters::default(),
            jev_jobs,
            jev_pending: HashSet::new(),
        }
    }

    /// Asks Jev in the background. The answer comes back to `on_jev` under the same id; an id
    /// already waiting for its answer isn't asked again.
    fn ask_jev(&mut self, id: String, body: serde_json::Value) {
        if self.jev_pending.insert(id.clone()) {
            let _ = self.jev_jobs.send(jev::Job { id, body });
        }
    }

    /// An answer from Jev. The id's first part, up to `:`, says which feature asked.
    fn on_jev(&mut self, id: String, answer: std::result::Result<serde_json::Value, String>) {
        self.jev_pending.remove(&id);
        let (feature, _rest) = id.split_once(':').unwrap_or((id.as_str(), ""));
        match feature {
            _ => {
                let _ = answer;
            }
        }
    }

    /// What to ask Jev about the board just composed: each feature that's on looks for what's
    /// changed since it last asked.
    fn jev_look(&mut self, board: &Board) {
        if !self.cfg.jev.any() {
            return;
        }
        let _ = board;
    }

    fn full_scan(&mut self) {
        self.load_config();
        self.seen = SeenState::load();
        self.remember_accounts();
        self.seen.save();
        self.load_todos();
        // Only with every todo read: a list that didn't read would make every attachment look unused.
        if !self.todos_unreadable {
            let texts: Vec<String> = self.todos.todos.iter().map(|t| t.description.clone()).collect();
            let _ = std::thread::Builder::new().name("codealong-attachments".into()).spawn(move || {
                attachments::collect_garbage(&texts.iter().map(String::as_str).collect::<Vec<_>>());
            });
        }
        self.app_records.refresh(&claudeapp::records_dir());
        self.app_records_at = Some(Instant::now());
        self.scan_sessions();
        self.load_repos();
        self.loading = false;
        self.publish();
        self.queue_git_all();
        self.fetch_github(true);
        self.fetch_usage();
        self.poll_opencode();
        self.watch();
        self.refresh_visitors(false);
    }

    /// Asks each OpenCode account's background service what's running and what it's waiting
    /// on, on a background thread: one poll at a time, and not more than one every 2 seconds.
    fn poll_opencode(&mut self) {
        if self.opencode_inflight || self.opencode_last.map(|t| t.elapsed() < Duration::from_secs(2)).unwrap_or(false) {
            return;
        }
        let accounts: Vec<AccountCfg> = self.cfg.accounts.iter().filter(|a| a.harness == "opencode").cloned().collect();
        if accounts.is_empty() {
            return;
        }
        self.opencode_inflight = true;
        self.opencode_last = Some(Instant::now());
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            procs::utility_thread();
            let now = now_ms();
            let results = accounts.into_iter().map(|a| (a.name.clone(), opencode::service(&a.home_path()).and_then(|svc| opencode::poll(&svc, now)))).collect();
            let _ = tx.send(Msg::OpencodeLive(results));
        });
    }

    /// OpenCode's interactive terminals: which sessions are live, and where each shows.
    fn refresh_opencode_terminals(&mut self) -> bool {
        if !self.cfg.accounts.iter().any(|a| a.harness == "opencode") {
            return false;
        }
        let fresh = opencode::terminals();
        let changed = fresh != self.opencode_terminals;
        self.opencode_terminals = fresh;
        changed
    }

    /// Reads an OpenCode or Antigravity account's database again after it changed. True when a
    /// session appeared that wasn't there before.
    fn reread_database(&mut self, account: &str) -> bool {
        let Some(a) = self.account(account).cloned() else { return false };
        let since = now_ms() - WINDOW_MS;
        match a.harness.as_str() {
            "opencode" => {
                let rows = opencode::read_sessions(&a.home_path(), since, &self.seen.pinned_ids("opencode", &a.name));
                let before: HashSet<String> = self.opencode_sessions.get(account).map(|v| v.iter().map(|r| r.id.clone()).collect()).unwrap_or_default();
                let added = rows.iter().any(|r| !before.contains(&r.id));
                self.opencode_sessions.insert(a.name.clone(), rows);
                // A write means a run may have started or asked something: ask the service.
                self.poll_opencode();
                added
            }
            "antigravity" => {
                let rows = antigravity::read_conversations(&a.home_path(), since, &self.seen.pinned_ids("antigravity", &a.name));
                let before: HashSet<String> = self.antigravity.get(account).map(|v| v.iter().map(|c| c.id.clone()).collect()).unwrap_or_default();
                let added = rows.iter().any(|c| !before.contains(&c.id));
                self.antigravity.insert(a.name.clone(), rows);
                added
            }
            _ => false,
        }
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

    /// Everything that depends on config.toml, read again. When nothing but the theme or the
    /// shortcuts changed, a new board is enough: reading everything again also asks for Claude usage, and the usage
    /// endpoint turns away calls that come often.
    fn reload(&mut self) {
        let before = self.cfg.clone();
        self.load_config();
        if self.remember_accounts() {
            self.seen.save();
        }
        let mut rest = self.cfg.clone();
        rest.appearance = before.appearance.clone();
        rest.shortcuts = before.shortcuts.clone();
        rest.debug = before.debug.clone();
        if rest == before {
            self.dirty = true;
            return;
        }
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
            let now = Instant::now();
            let mut wait = if !pending.is_empty() || self.dirty { Duration::from_millis(50) } else { Duration::from_secs(2) };
            // Wake for a git batch that's due, or a held git run.
            if self.dirty_git {
                let due = self.last_publish.map(|t| (t + throttle::GIT_PUBLISH_EVERY).saturating_duration_since(now)).unwrap_or_default();
                wait = wait.min(due.max(Duration::from_millis(10)));
            }
            if let Some(at) = self.git_queue.next_due() {
                wait = wait.min(at.saturating_duration_since(now).max(Duration::from_millis(10)));
            }
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
                Ok(Msg::OpencodeLive(results)) => {
                    self.opencode_inflight = false;
                    for (account, live) in results {
                        match live {
                            Some(l) => {
                                self.opencode_live.insert(account, l);
                            }
                            None => {
                                self.opencode_live.remove(&account);
                            }
                        }
                    }
                    self.dirty = true;
                }
                Ok(Msg::SetArchived(key, archived)) => {
                    self.seen.set_archived(&key, archived, now_ms());
                    self.seen.save();
                    if key.starts_with("codex:") {
                        self.refresh_archived_rollouts();
                    }
                    self.dirty = true;
                }
                Ok(Msg::SetPinned(keys)) => {
                    self.seen.set_pinned(keys);
                    self.seen.save();
                    self.dirty = true;
                }
                Ok(Msg::SetPinnedWorktrees(ids)) => {
                    self.seen.set_pinned_worktrees(ids);
                    self.seen.save();
                    self.dirty = true;
                }
                Ok(Msg::SetTheme(theme)) => {
                    // Published now, not on the next tick: the app's appearance follows the board.
                    self.cfg.appearance.theme = theme;
                    self.publish();
                }
                Ok(Msg::Ping(sent)) => perf::lag("engine", sent.elapsed()),
                Ok(Msg::Todo(op, reply)) => {
                    let lanes = self.lane_index();
                    // Under the lock the command takes too, and against the file as it is now, so
                    // a change the command made a moment ago isn't written over.
                    let lock = todos::lock().map_err(|e| eprintln!("{e:#}")).ok();
                    self.reload_todos();
                    let result = match self.todos_unreadable {
                        // A change now would be lost when the file is fixed, so it's turned away.
                        true => Err(self.todos_warning.clone().unwrap_or_default()),
                        false => self.todos.apply(op, &lanes, now_ms()).map_err(|e| format!("{e:#}")),
                    };
                    let result = match result {
                        Ok(changed) => {
                            if changed {
                                self.save_todos();
                                drop(lock);
                                // Now rather than on the next tick: the window waits for it.
                                self.publish();
                            }
                            Ok(self.board.read().map(|b| b.generated_at).unwrap_or(0))
                        }
                        Err(e) => Err(e),
                    };
                    let _ = reply.send(result);
                }
                Ok(Msg::JevDone(id, answer)) => self.on_jev(id, answer),
                Ok(Msg::TodoStarted { id, lane, session, pending }) => {
                    let lanes = self.lane_index();
                    let _lock = todos::lock().map_err(|e| eprintln!("{e:#}")).ok();
                    self.reload_todos();
                    if !self.todos_unreadable && self.todos.started(&id, lane.as_deref(), session.as_deref(), pending, &lanes, now_ms()) {
                        self.save_todos();
                    }
                }
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
                    let (n, t) = (batch.len(), Instant::now());
                    self.apply_fs(batch);
                    self.perf.fs(n, t.elapsed());
                }
            }
            for path in self.git_queue.due(Instant::now()) {
                self.send_git(path);
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
            if throttle::publish_now(self.dirty, self.dirty_git, self.last_publish.map(|t| t.elapsed()), PUBLISH_EVERY) {
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
                    self.cfg.accounts = detect::first_run_accounts();
                }
            }
        }
        perf::set_enabled(self.cfg.debug.perf_log);
    }

    /// Every folder config.toml lists is known from then on, so an account you take out isn't
    /// suggested again, whether Settings or an editor took it out. True when one was new.
    fn remember_accounts(&mut self) -> bool {
        let keys: Vec<String> = self.cfg.accounts.iter().map(|a| detect::folder_key(&a.home)).collect();
        self.seen.know_accounts(keys)
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
        self.opencode_sessions.clear();
        self.antigravity.clear();
        for a in self.cfg.accounts.clone() {
            let home = a.home_path();
            match a.harness.as_str() {
                "claude" => {
                    self.registry.insert(a.name.clone(), claude::read_registry(&home));
                    let files = claude::transcript_files(&home, since, &self.seen.pinned_ids("claude", &a.name));
                    for (path, parsed) in parallel_map(files, |(p, m)| claude::parse_transcript(&p, m, None).map(|r| (p, r))) {
                        self.transcripts.insert(path, (a.name.clone(), parsed));
                    }
                }
                "codex" => {
                    self.codex_index.insert(a.name.clone(), codex::read_index(&home));
                    let files = codex::rollout_files(&home, since, &self.seen.pinned_ids("codex", &a.name));
                    for (path, parsed) in parallel_map(files, |(p, m)| {
                        let parsed = codex::parse_rollout(&p, m, None);
                        Some((p, parsed))
                    }) {
                        self.rollouts.insert(path, (a.name.clone(), parsed));
                    }
                }
                "opencode" => {
                    let rows = opencode::read_sessions(&home, since, &self.seen.pinned_ids("opencode", &a.name));
                    self.opencode_sessions.insert(a.name.clone(), rows);
                }
                "antigravity" => {
                    let rows = antigravity::read_conversations(&home, since, &self.seen.pinned_ids("antigravity", &a.name));
                    self.antigravity.insert(a.name.clone(), rows);
                }
                _ => {}
            }
        }
        self.refresh_opencode_terminals();
        self.refresh_archived_rollouts();
    }

    /// Loads the Codex threads archived from Codealong from `archived_sessions`, so its Archived
    /// list can show them, and drops ones that were unarchived or are no longer listed.
    fn refresh_archived_rollouts(&mut self) {
        for a in self.cfg.accounts.clone().iter().filter(|a| a.harness == "codex") {
            let dir = a.home_path().join("archived_sessions");
            let ids = self.seen.archived_ids("codex", &a.name);
            self.rollouts.retain(|path, (acct, _)| {
                acct != &a.name || !path.starts_with(&dir) || (path.exists() && codex::thread_id_of(path).map(|id| ids.contains(&id)).unwrap_or(false))
            });
            for (path, m) in codex::archived_rollout_files(&a.home_path(), &ids) {
                if !self.rollouts.contains_key(&path) {
                    let parsed = codex::parse_rollout(&path, m, None);
                    self.rollouts.insert(path, (a.name.clone(), parsed));
                }
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
        out.extend(self.opencode_sessions.values().flatten().map(|r| r.directory.clone()));
        out.extend(self.antigravity.values().flatten().filter_map(|c| c.cwd.clone()));
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

    /// Asks for git in a worktree. Runs at once when it's been quiet, otherwise once the
    /// current run ends or the spacing allows, whichever is later.
    fn queue_git(&mut self, path: PathBuf) {
        if let Verdict::Run = self.git_queue.request(path.clone(), Instant::now()) {
            self.send_git(path);
        }
    }

    fn send_git(&mut self, path: PathBuf) {
        let base = self.base_for(&path);
        let _ = self.git_jobs.send((path, base));
    }

    /// Every worktree, the ones with the latest activity first: with many worktrees and a busy
    /// Mac the first results take a while, and those are the ones on screen.
    fn queue_git_all(&mut self) {
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

    fn on_git(&mut self, path: PathBuf, info: GitInfo) {
        self.perf.git_jobs += 1;
        if self.seeds.iter().any(|s| s.path == path) {
            self.git.insert(path.clone(), info);
            self.dirty_git = true;
        }
        if self.git_queue.done(&path, Instant::now()) {
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
            let _ = tx.send(Msg::GithubDone(String::new(), Ok(github::RepoPrs::default())));
        });
    }

    fn on_github(&mut self, slug: String, result: std::result::Result<github::RepoPrs, String>) {
        if slug.is_empty() {
            self.gh_inflight = false;
            return;
        }
        match result {
            Ok(repo) => {
                self.prs.insert(slug, repo.prs);
                if repo.login.is_some() {
                    self.gh_login = repo.login;
                }
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

    /// FSEvents can hold back appends until a writer closes its file. Codex keeps rollouts
    /// open across turns, so check their lengths too and read only bytes we haven't consumed.
    fn poll_codex(&mut self) {
        let mut changed = Vec::new();
        for a in self.cfg.accounts.iter().filter(|a| a.harness == "codex") {
            let home = a.home_path();
            let sessions = home.join("sessions");
            let mut paths: BTreeSet<PathBuf> = codex::rollout_files(&home, now_ms() - WINDOW_MS, &self.seen.pinned_ids("codex", &a.name))
                .into_iter().map(|(p, _)| p).collect();
            // Include known files so a removal or truncation is noticed as well.
            paths.extend(self.rollouts.iter().filter(|(p, (acct, _))| acct == &a.name && p.starts_with(&sessions)).map(|(p, _)| p.clone()));
            for p in paths {
                let unread = match self.rollouts.get(&p) {
                    Some((_, Some((_, offset)))) => std::fs::metadata(&p).map(|m| m.len() != *offset).unwrap_or(true),
                    // Guardian reviews and subagents still don't need to be parsed.
                    Some((_, None)) => false,
                    None => true,
                };
                if unread {
                    changed.push(p);
                }
            }
            let index = codex::read_index(&home);
            if self.codex_index.get(&a.name) != Some(&index) {
                self.codex_index.insert(a.name.clone(), index);
                self.dirty = true;
            }
        }
        if !changed.is_empty() {
            self.apply_fs(changed);
        }
    }

    fn tick(&mut self) {
        let claude_accounts: Vec<String> = self.cfg.accounts.iter().filter(|a| a.harness == "claude").map(|a| a.name.clone()).collect();
        for a in claude_accounts {
            if self.refresh_registry(&a) {
                self.dirty = true;
            }
        }
        self.poll_codex();
        if self.refresh_opencode_terminals() {
            self.dirty = true;
        }
        self.poll_opencode();
        if self.gh_last.map(|t| t.elapsed() >= GITHUB_EVERY).unwrap_or(true) {
            self.fetch_github(true);
        }
        let every = if self.usage_backoff { USAGE_BACKOFF } else { USAGE_EVERY };
        if self.usage_last.map(|t| t.elapsed() >= every).unwrap_or(true) {
            self.fetch_usage();
        }
        if self.app_records_at.map(|t| t.elapsed() >= Duration::from_secs(30)).unwrap_or(true) {
            self.app_records_at = Some(Instant::now());
            if self.app_records.refresh(&claudeapp::records_dir()) {
                self.dirty = true;
            }
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
        let todos_path = todos::path();
        let accounts = self.cfg.accounts.clone();
        let mut registries = BTreeSet::new();
        let mut indexes = BTreeSet::new();
        let mut transcripts = Vec::new();
        let mut rollouts = Vec::new();
        let mut databases = BTreeSet::new();
        let mut lanes = BTreeSet::new();
        let mut reload_repos = false;
        let mut reload_config = false;
        'paths: for p in paths {
            if p == config_path {
                reload_config = true;
                continue;
            }
            if p == todos_path {
                self.reload_todos();
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
                    "opencode" | "antigravity" => {
                        if p.starts_with(&home) {
                            if p.parent() == Some(home.as_path()) && file_name(&p).contains(".db") {
                                databases.insert(a.name.clone());
                            }
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
        for a in databases {
            sessions_changed |= self.reread_database(&a);
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
                // The database, its WAL and its journal, all in the folder itself.
                "opencode" | "antigravity" => paths.push((home.clone(), RecursiveMode::NonRecursive)),
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
                    let app_archived = self.app_records.archived();
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
                        if let Some(mut s) = board::claude_session(a, reg, info, self.seen.seen_at(&key), now, &quiet) {
                            // Archived here until it's active again, or archived in the Claude app.
                            let here = self.seen.archived_at(&key).map(|at| s.last_activity <= at).unwrap_or(false);
                            s.archived = here || app_archived.contains(id);
                            out.push(s);
                        }
                    }
                }
                "codex" => {
                    let index = self.codex_index.get(&a.name);
                    // Codex decides what's archived: its archived threads live in `archived_sessions`.
                    let archived_dir = a.home_path().join("archived_sessions");
                    for (path, (acct, info)) in &self.rollouts {
                        let (true, Some((th, _))) = (acct == &a.name, info.as_ref()) else { continue };
                        let key = format!("codex:{}:{}", a.name, th.id);
                        let name = index.and_then(|i| i.get(&th.id)).map(|(n, _)| n.as_str());
                        let mut s = board::codex_session(a, th, name, self.seen.seen_at(&key), now);
                        s.archived = path.starts_with(&archived_dir);
                        out.push(s);
                    }
                }
                "opencode" => {
                    let live = self.opencode_live.get(&a.name);
                    let rows = self.opencode_sessions.get(&a.name).map(|v| v.as_slice()).unwrap_or(&[]);
                    // A terminal in a folder shows the folder's latest session (rows are newest first).
                    let mut latest: HashMap<&str, &str> = HashMap::new();
                    for r in rows {
                        latest.entry(r.directory.as_str()).or_insert(r.id.as_str());
                    }
                    for r in rows {
                        let key = format!("opencode:{}:{}", a.name, r.id);
                        let latest_here = latest.get(r.directory.as_str()) == Some(&r.id.as_str());
                        let terminal = opencode::terminal_for(&self.opencode_terminals, r, latest_here).map(|t| t.pid);
                        let mut s = board::opencode_session(a, r, live, terminal, self.seen.seen_at(&key), now);
                        // Archived in OpenCode, or here until it's active again.
                        s.archived |= self.seen.archived_at(&key).map(|at| s.last_activity <= at).unwrap_or(false);
                        out.push(s);
                    }
                }
                "antigravity" => {
                    for c in self.antigravity.get(&a.name).map(|v| v.as_slice()).unwrap_or(&[]) {
                        let key = format!("antigravity:{}:{}", a.name, c.id);
                        let mut s = board::antigravity_session(a, c, self.seen.seen_at(&key), now);
                        s.archived = self.seen.archived_at(&key).map(|at| s.last_activity <= at).unwrap_or(false);
                        out.push(s);
                    }
                }
                _ => {}
            }
        }
        out
    }

    fn load_todos(&mut self) {
        match todos::load() {
            Ok((store, text)) => {
                self.todos = store;
                self.todos_text = text;
                self.todos_warning = None;
                self.todos_unreadable = false;
            }
            Err(e) => {
                self.todos_warning = Some(format!("{e:#}"));
                self.todos_unreadable = true;
            }
        }
    }

    /// todos.json changed on disk. Our own writes come back through the watcher too; only
    /// someone else's edit is read again, or any change while the file didn't read.
    fn reload_todos(&mut self) {
        let text = std::fs::read_to_string(todos::path()).unwrap_or_default();
        if text == self.todos_text && !self.todos_unreadable {
            return;
        }
        self.load_todos();
        self.dirty = true;
    }

    fn save_todos(&mut self) {
        self.dirty = true;
        // A file that didn't read is left for you to fix, not replaced.
        if self.todos_unreadable {
            return;
        }
        match todos::save(&self.todos) {
            Ok(text) => {
                self.todos_text = text;
                self.todos_warning = None;
            }
            Err(e) => self.todos_warning = Some(format!("Couldn't save todos.json: {e:#}")),
        }
    }

    /// Each lane's project and repo, for linking todos.
    fn lane_index(&self) -> LaneIndex {
        let board = self.board.read().map(|b| b.lanes.iter().map(|l| (l.id.clone(), (l.project.clone(), repo_identity(l)))).collect()).unwrap_or_default();
        LaneIndex(board)
    }

    /// Links sessions that starts in the apps were waiting for, from the board just composed.
    fn link_pending(&mut self, board: &Board) -> bool {
        if self.todos.todos.iter().all(|t| t.pending.is_empty()) {
            return false;
        }
        let harness = |h: Harness| if h == Harness::Codex { "codex" } else { "claude" };
        let sessions: Vec<todos::SessionRef> = board
            .lanes
            .iter()
            .flat_map(|l| {
                l.sessions.iter().filter(|s| s.visitor.is_none()).map(move |s| todos::SessionRef {
                    key: &s.key,
                    lane: &l.id,
                    harness: harness(s.harness),
                    account: &s.account,
                    started: s.started_at.unwrap_or(s.last_activity),
                })
            })
            .collect();
        let lanes = LaneIndex(board.lanes.iter().map(|l| (l.id.clone(), (l.project.clone(), repo_identity(l)))).collect());
        let changed = self.todos.resolve_pending(&sessions, &lanes, now_ms());
        if changed {
            self.save_todos();
        }
        changed
    }

    fn compose(&self) -> Board {
        let now = now_ms();
        let apps = AppNames {
            terminal: launch::terminal_name(&self.cfg.apps.terminal),
            terminal_id: self.cfg.apps.terminal.clone(),
            editor: launch::editor_name(&self.cfg.apps.editor),
            editor_id: self.cfg.apps.editor.clone(),
            new_terminal: self.cfg.apps.new_terminal,
        };
        let order: Vec<String> = self.cfg.projects.iter().map(|p| p.name.clone()).collect();
        let warnings = [&self.config_warning, &self.gh_warning, &self.watch_warning, &self.todos_warning].into_iter().flatten().cloned().collect();
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
            pinned: &self.seen.pinned,
            github_login: self.gh_login.as_deref(),
        });
        board.usage = self.compose_usage(now);
        board.pinned_worktrees = self.seen.pinned_worktrees.clone();
        board.theme = self.cfg.appearance.theme;
        board.quick_shortcut = self.cfg.shortcuts.quick_panel.clone();
        board.jev = self.cfg.jev.clone();
        board.perf_log = perf::enabled();
        board.todos = self.todos.todos.clone();
        board.labels = self.todos.labels.clone();
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
                "opencode" => usage::none(a, "OpenCode bills the provider you bring, so it has no plan limits to show."),
                "antigravity" => usage::none(a, "Antigravity keeps its quotas in the app."),
                _ => self.usage.get(&a.name).cloned().unwrap_or_else(|| usage::loading(a)),
            })
            .collect()
    }

    fn publish(&mut self) {
        let t = Instant::now();
        let mut board = self.compose();
        if self.link_pending(&board) {
            board.todos = self.todos.todos.clone();
        }
        self.jev_look(&board);
        let composed = t.elapsed();
        self.dirty = false;
        self.dirty_git = false;
        let since_last = self.last_publish.map(|t| t.elapsed());
        self.last_publish = Some(Instant::now());
        let hash = board_hash(&board);
        let hashed = t.elapsed() - composed;
        if hash == self.last_hash {
            perf::log(&format!("publish skipped, nothing changed: compose {} ms, hash {} ms", composed.as_millis(), hashed.as_millis()));
            return;
        }
        self.last_hash = hash;
        let t = Instant::now();
        (self.emit)(&board);
        let emitted = t.elapsed();
        if perf::enabled() {
            let line = self.perf.publish_line(&board, composed, hashed, emitted, since_last, self.git_queue.running());
            perf::log(&line);
        }
        if let Ok(mut slot) = self.board.write() {
            *slot = board;
        }
    }
}

/// Each lane's project and repo identity, by lane id.
struct LaneIndex(HashMap<String, (String, String)>);

impl todos::Lanes for LaneIndex {
    fn project_of(&self, lane: &str) -> Option<String> {
        self.0.get(lane).map(|(p, _)| p.clone()).filter(|p| p != ELSEWHERE)
    }
    fn repo_of(&self, lane: &str) -> Option<String> {
        self.0.get(lane).map(|(_, r)| r.clone())
    }
}

/// What every worktree of a repo shares: its GitHub slug, else its project and name.
fn repo_identity(l: &Lane) -> String {
    l.repo_slug.clone().unwrap_or_else(|| format!("{}/{}", l.project, l.repo))
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
        let _ = std::thread::Builder::new().name(format!("codealong-git-{i}")).spawn(move || {
            procs::utility_thread();
            loop {
            let job = match rx.lock() {
                Ok(r) => r.recv(),
                Err(_) => return,
            };
            let Ok((path, base)) = job else { return };
            let t = Instant::now();
            let info = git::status(&path, base.as_deref());
            if perf::enabled() {
                perf::log(&format!("git status {} ms: {}", t.elapsed().as_millis(), path.display()));
            }
            if back.send(Msg::GitDone(path, info)).is_err() {
                return;
            }
            }
        });
    }
    jobs
}

/// First run: the accounts in use or signed in, then a project per GitHub owner your agents
/// worked in.
fn first_run_config() -> Config {
    let accounts = detect::first_run_accounts();
    let since = now_ms() - 30 * DAY_MS;
    let mut cwds: Vec<String> = Vec::new();
    for a in &accounts {
        let home = a.home_path();
        match a.harness.as_str() {
            "claude" => {
                let files = claude::transcript_files(&home, since, &HashSet::new());
                cwds.extend(parallel_map(files, |(p, _)| claude::transcript_cwd(&p)));
            }
            "codex" => {
                let files = codex::rollout_files(&home, since, &HashSet::new());
                cwds.extend(parallel_map(files, |(p, _)| {
                    read_first_line(&p, 4 * 1024 * 1024).and_then(|l| codex::parse_meta(&l)).map(|m| m.cwd)
                }));
            }
            "opencode" => cwds.extend(opencode::read_sessions(&home, since, &HashSet::new()).into_iter().map(|r| r.directory)),
            "antigravity" => cwds.extend(antigravity::read_conversations(&home, since, &HashSet::new()).into_iter().filter_map(|c| c.cwd)),
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
    #[test]
    fn polls_codex_turns_while_the_rollout_stays_open() {
        use std::io::Write;
        use super::*;

        let home = std::env::temp_dir().join(format!("codealong-codex-poll-{}", std::process::id()));
        let dir = home.join("sessions/2026/09/24");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("rollout-2026-09-24T18-00-00-01a0d54c-7f02-7623-95b3-a3110c0f9672.jsonl");
        let mut file = std::fs::File::create(&path).unwrap();
        writeln!(file, r#"{{"type":"session_meta","payload":{{"id":"thread","cwd":"/codealong-test-missing-repo","thread_source":"user"}}}}"#).unwrap();
        writeln!(file, r#"{{"timestamp":"2026-09-24T18:00:00.000Z","type":"event_msg","payload":{{"type":"task_complete","last_agent_message":"Earlier result"}}}}"#).unwrap();
        let (tx, _rx) = channel();
        let mut engine = Engine::new(Box::new(|_| {}), tx, Arc::new(RwLock::new(Board::default())));
        engine.cfg.accounts = vec![AccountCfg { name: "studio".into(), harness: "codex".into(), home: home.to_string_lossy().into_owned(), command: None }];
        // A startup scan has the previous completed turn. No filesystem events follow.
        engine.rollouts.insert(path.clone(), ("studio".into(), codex::parse_rollout(&path, 0, None)));
        engine.codex_index.insert("studio".into(), HashMap::new());
        let now = crate::util::parse_rfc3339_ms("2026-09-24T18:35:00.000Z").unwrap();
        assert_eq!(engine.build_sessions(now)[0].status, Status::Unread);

        writeln!(file, r#"{{"timestamp":"2026-09-24T18:34:00.000Z","type":"event_msg","payload":{{"type":"task_started"}}}}"#).unwrap();
        writeln!(file, r#"{{"timestamp":"2026-09-24T18:34:01.000Z","type":"event_msg","payload":{{"type":"user_message","message":"Continue the work"}}}}"#).unwrap();
        engine.poll_codex();
        let session = &engine.build_sessions(now)[0];
        assert_eq!(session.status, Status::Working);
        assert!(session.live);
        assert_eq!(session.excerpt.as_deref(), Some("Continue the work"));
        assert!(engine.dirty);

        engine.dirty = false;
        engine.poll_codex();
        assert!(!engine.dirty, "unchanged files don't need another update");

        // A poll between writes must leave the incomplete line for the next one.
        write!(file, r#"{{"timestamp":"2026-09-24T18:34:59.000Z","type":"event_msg","payload":{{"type":"task_complete","last_agent_message":"Done"}}}}"#).unwrap();
        engine.poll_codex();
        assert_eq!(engine.build_sessions(now)[0].status, Status::Working);
        writeln!(file).unwrap();
        engine.poll_codex();
        let session = &engine.build_sessions(now)[0];
        assert_eq!(session.status, Status::Unread);
        assert!(!session.live);
        assert_eq!(session.excerpt.as_deref(), Some("Done"));

        drop(file);
        std::fs::remove_file(&path).unwrap();
        engine.poll_codex();
        assert!(engine.build_sessions(now).is_empty());

        // Discovery also works without a create event, including files from older day folders.
        std::fs::write(&path, "{\"type\":\"session_meta\",\"payload\":{\"id\":\"new-thread\",\"cwd\":\"/codealong-test-missing-repo\"}}\n").unwrap();
        std::fs::write(home.join("session_index.jsonl"), "{\"id\":\"new-thread\",\"thread_name\":\"New work\"}\n").unwrap();
        engine.poll_codex();
        assert_eq!(engine.build_sessions(now)[0].title, "New work");

        // Hidden subagent logs stay ignored even if they grow.
        engine.rollouts.insert(path.clone(), ("studio".into(), None));
        let mut hidden = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
        writeln!(hidden, "not a user thread").unwrap();
        engine.dirty = false;
        engine.poll_codex();
        assert!(!engine.dirty);
        assert!(engine.build_sessions(now).is_empty());
        drop(hidden);
        std::fs::remove_dir_all(&home).unwrap();
    }

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
