//! The engine owns every cache. It reacts to file changes (FSEvents), runs git on a small
//! worker pool and GitHub in the background, and publishes a fresh board after each change.
//!
//! One thread does all of it (`Engine::run`, kept going through a panic by engine/guard.rs).
//! Everything else reaches it by a message (`Msg`), sent through a `Handle`, and reads the board
//! it publishes. What it holds is the fields of `Engine`, here; what it does with them is in
//! parts by what they're about, a file each under engine/, most of them further `impl Engine`
//! blocks:
//!
//! - `engine/handle.rs`: `Handle`, what everything else talks to the engine through.
//! - `engine/open.rs`: opening things where they live, as a click does (`open_here`,
//!   `open_link`), over a board read once.
//! - `engine/trouble.rs`: what goes wrong in the background (`Trouble`), and the board's
//!   notices.
//! - `engine/readers.rs`: the harnesses' readers as the engine drives them, and the process
//!   table they look at.
//! - `engine/reposcan.rs`: the repo scan: checkouts and their worktrees as seeds for lanes,
//!   and the reads macOS may hold.
//! - `engine/gitruns.rs`: git in the worktrees: the workers, the queue, what comes back.
//! - `engine/watch.rs`: the file watcher, and what a batch of changed paths calls for
//!   (`apply_fs`).
//! - `engine/records.rs`: what sessions' records show besides the sessions: who worked in a
//!   worktree nobody runs in, and the PRs, pushes, pages and Chrome tabs.
//! - `engine/polls.rs`: what's read in the background on a clock of its own: usage, team
//!   files, the accounts' settings, the terminals of starts.
//! - `engine/seen.rs`: state.json, what's done to sessions' marks (`on_raise`, `on_label`),
//!   and the marks forgotten once they're spent.
//! - `engine/todofile.rs`: todos.json as the engine holds and changes it.
//! - `engine/jevasks.rs`: what's asked of the decision models, and what's done with their
//!   answers.
//! - `engine/emblems.rs`: giving sessions and schedules their emblems.
//! - `engine/sessions.rs`: the board's sessions: what the records say with all the engine
//!   knows laid over it, a stage at a time (`laid_over`).
//! - `engine/guard.rs`: the thread under a guard, and what a fall of it comes to.
//! - This file: `Msg`; the `Engine`, its fields and its making (`start`, `Engine::new`); the
//!   first scan, and what's read again when config.toml changes or after a fall (`full_scan`,
//!   `reload`, `pick_up`); the loop (`run`) and its tick; and the marks that ask for a board
//!   (`mark_dirty`, `publish_for`); and what has no part yet.
//!
//! A part starts with `use super::*`, so the imports below serve them all, and what it makes
//! `pub` is named from here by the `pub use` lines. A method this file or another part calls
//! is `pub(super)`. The fields stay on `Engine`, whichever part works on them.
//!
//! Before adding to it:
//!
//! - A subsystem is a struct of its own with `on_*` methods the engine calls, as `GhPoll`
//!   (ghpoll.rs) is, its rules tested on made-up clocks: not more fields on `Engine` and another
//!   arm's worth of logic in `run`. src/conventions.test.ts holds this file and each part to
//!   what one reading holds, so new work goes in a module of its own, or in the part it's
//!   about.
//! - Work the engine waits on from another thread goes through `throttle::background`, which
//!   tells whoever started it that it's over however it ends (with nothing when it fell over or
//!   never started): the mark set for it is taken off there. A mark that only the work's own
//!   last line clears stays for good after a panic, and what waits on it never runs again.
//! - The model's rules (a session's status, a worktree's bay, the headlines) are board's, pure
//!   functions with tests: the engine gathers what they need and calls them. A status set
//!   after them here is counted by src/conventions.test.ts, and there should be no more.
//! - What goes wrong in the background is a `Trouble`, said as a notice until it goes right
//!   again. What Codealong does to a session is a request in actions.rs.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Result};
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};

use crate::board::{self, LaneSeed, ELSEWHERE};
use crate::chrome;
use crate::autoreview;
use crate::artifacts;
use crate::chrometabs;
use crate::prlinks;
use crate::teamfile;
use crate::training;
use crate::previews;
use crate::reusable;
use crate::origins;
use crate::suggestions;
use crate::tags;
use crate::config::{self, AccountCfg, Config, Theme};
use crate::detect;
use crate::editors;
use crate::ghpoll::{self, GhPoll, Now};
use crate::git;
use crate::harness;
use crate::github::{self, PrRaw};
use crate::held::Held;
use crate::jev;
use crate::todojev;
use crate::launch;
use crate::model::*;
use crate::perf;
use crate::procs;
use crate::repos::{self, Checkout};
use crate::search;
use crate::setup;
use crate::state::SeenState;
use crate::throttle::{self, Drops, GitQueue, Verdict};
use crate::todos::{self, TodoOp, TodoStore};
use crate::attachments;
use crate::usage;
use crate::util::{expand, file_name, home, mtime_ms, now_ms, parallel_map, tilde, DAY_MS};

const PUBLISH_EVERY: Duration = Duration::from_millis(300);
/// Claude usage is read this often; Codex usage updates with every Codex turn.
const USAGE_EVERY: Duration = Duration::from_secs(2 * 60);
/// After the usage endpoint says to slow down.
const USAGE_BACKOFF: Duration = Duration::from_secs(10 * 60);
/// How long the repo scan waits for folders' files before going on without the ones still out,
/// which macOS may be holding for a privacy prompt (held.rs). At a load of 600, this Mac's 66
/// session folders and 123 worktrees were all read within 170 ms.
pub(crate) const HELD_WAIT: Duration = Duration::from_secs(1);
/// Reads out this long are said on the board: a slow disk is back by then.
const HELD_WARN_AFTER: Duration = Duration::from_secs(10);
/// Reads that land within this of each other are used by one scan.
const LANDED_QUIET: Duration = Duration::from_millis(150);

pub enum Msg {
    Fs(Vec<PathBuf>),
    /// The watcher dropped events under these folders, the ones it watches: what changed there
    /// isn't known (`Engine::on_dropped`).
    Dropped(Vec<PathBuf>),
    /// Git's word on a worktree, or none when its run fell over: the run is over either way.
    GitDone(PathBuf, Option<GitInfo>),
    /// Something GitHub was asked in the background came back (ghpoll.rs).
    Github(ghpoll::Event),
    RefreshGithub,
    /// A PR changed on GitHub at your hand: PRs are read again in full now.
    PrChanged,
    /// A PR you merged from Codealong, by its slug on the board and its number. The reply is the
    /// time of the board that shows it, published at once.
    Merged(String, u64, Sender<i64>),
    /// A PR you reviewed from Codealong, by its slug on the board and its number, with the
    /// verdict and the commit it was of where that isn't the PR's head (a draft's). The reply is
    /// the time of the board that shows it, published at once.
    Approved(String, u64, github::Verdict, Option<String>, Sender<i64>),
    /// Sessions marked seen, as of then (when it's taken, with none). With a reply, the time of
    /// the board that shows it, published at once.
    MarkSeen(Vec<String>, Option<i64>, Option<Sender<i64>>),
    /// Sessions to mark unread, and where to say when the board shows it.
    MarkUnread(Vec<String>, Option<Sender<i64>>),
    /// A session whose own agent marked the turn it's taking seen, with why (its time is the
    /// engine's). The reply is the time of the board that shows it, published at once.
    AgentSeen(String, AgentSeen, Sender<i64>),
    /// Sessions snoozed until then, or woken now with none. The reply is the time of the board
    /// that shows it, published at once.
    Snooze(Vec<String>, Option<i64>, Sender<i64>),
    /// Sessions that need you set aside until their side conversations answer. The reply is the
    /// time of the board that shows it, published at once.
    Aside(Vec<String>, Sender<i64>),
    /// Reviews asked of you snoozed until then, by PR key, or woken now with none. The reply is
    /// the time of the board that shows it, published at once.
    SnoozeReviews(Vec<String>, Option<i64>, Sender<i64>),
    /// Sessions quieted, or made heard again. The reply is the time of the board that shows it,
    /// published at once.
    Quiet(Vec<String>, bool, Sender<i64>),
    /// Labels put on sessions, by name or id, and taken off. The reply is the time of the board
    /// that shows it, published at once, or why it couldn't be done.
    Label(Vec<String>, Vec<String>, Vec<String>, Sender<std::result::Result<i64, String>>),
    /// Sessions moved to the top, or back to their places with false, and who by and why (its
    /// time is the engine's). The reply is the time of the board that shows it, published at
    /// once, or why an agent's move was turned down.
    Raise(Vec<String>, bool, Raised, Sender<Result<i64, String>>),
    Pin(Vec<String>, bool, Sender<i64>),
    /// Helpers made sessions of your own. The reply is the time of the board that shows it,
    /// published at once.
    Own(Vec<String>, Sender<i64>),
    /// A helper you sent a message to, or one an agent sent a message to (`SeenState::talking`).
    Talking(String, bool),
    /// The setup wizard was finished or skipped (setup.rs).
    SetupDone,
    /// suggestions.json just changed at a request here: the board is published again at once.
    /// The reply is the time of the board that shows it.
    Suggested(Sender<i64>),
    /// Lane path → session keys whose logs mention it, from the scan of that number
    /// (`Visitors::begin`).
    Visitors(u64, HashMap<PathBuf, Vec<String>>),
    /// Session key → the PRs its records show it made or changed, each with its first mark's
    /// time, and the branches they show it pushed to (prlinks.rs).
    PrMarks(prlinks::Marks, prlinks::Pushes),
    /// Session key → the pages on the web its records show it made or showed you, the latest
    /// first (artifacts.rs).
    Artifacts(artifacts::Made),
    /// Session key → the Chrome tabs its records show it drove, their profiles looked up, and
    /// Chrome's profiles (chrometabs.rs).
    ChromeUse(HashMap<String, chrometabs::Use>, Vec<chrome::Profile>),
    /// The profile folder a session's Chrome tabs are in, looked up for the tab group it had
    /// then, and Chrome's profiles.
    ChromeProfile(String, Option<i64>, Option<String>, Vec<chrome::Profile>),
    RefreshUsage,
    /// How something Codealong does by itself in the background went (`Handle::went`): why it
    /// didn't work if it didn't, and which didn't if it was several.
    Went(Trouble, Option<String>),
    /// The PRs another of your devices runs a session on (nudges.rs `running_elsewhere`).
    RunningElsewhere(std::collections::BTreeSet<String>),
    /// Your other machines that share automations, as their feeds said (shared.rs).
    Machines(crate::shared::Machines),
    /// Sessions started in tmux while the screen was locked shown in your terminal once it
    /// wasn't (launch.rs `show_unlocked`): which weren't and why if any, and whether the screen
    /// was locked again before they all were.
    ShownUnlocked(Option<String>, bool),
    /// A job of the automatic reviews went (autoreview.rs), and why it didn't if it didn't:
    /// reviews.json is read again at once.
    Reviewed(Option<autoreview::Said>),
    /// pr-links.json was changed from a window: read now, without waiting for the watcher.
    PrLinksChanged,
    /// The usage read is over: what it read, or none when it fell over or never started.
    UsageDone(Option<Vec<Usage>>),
    /// The tick's process table, listed off this thread (`list_processes`), or none when the
    /// listing fell over or never started.
    Processes(Option<procs::Listed>),
    /// The read of repos' team files is over (teamfile.rs): what it read, or none when it fell
    /// over or never started.
    TeamFiles(Option<Vec<teamfile::Answer>>),
    /// The read of the accounts' data settings is over (training.rs): what it read, or none when
    /// it fell over or never started.
    TrainingRead(Option<Vec<training::Training>>),
    /// The read of the boundary members' sign-ins is over (signin.rs): what it read, or none when
    /// it stopped partway.
    SignInsRead(Option<Vec<crate::signin::SignIn>>),
    /// One account's data setting, read again after Codealong turned training off, for the
    /// account the turn-off began on (`accountpoll::identity`).
    TrainingOne(String, training::Training),
    /// The read of the accounts' limit resets is over (resets.rs): what it read, or none when it
    /// fell over or never started.
    ResetsRead(Option<Vec<crate::resets::Resets>>),
    /// One account's resets, read again after Codealong used one, for the account the use began
    /// on (`accountpoll::identity`).
    ResetsOne(String, crate::resets::Resets),
    /// The read of the accounts' MCP servers is over (mcp.rs): what it read, or none when it fell
    /// over or never started.
    McpRead(Option<Vec<crate::mcp::Mcp>>),
    /// The read of the terminals of starts that wait for their session is over (starting.rs
    /// `read`): what each showed, or none when it fell over or never started.
    Screens(Option<Vec<crate::starting::Found>>),
    /// What the terminal of the start made then waited on was just answered (starting.rs
    /// `answer`): it's read again at the next look.
    StartAnswered(i64),
    /// A session's emblem was drawn, or why it wasn't (emblems.rs `draw`): the kept picture's
    /// name and what it shows.
    EmblemDrawn(String, std::result::Result<(String, String), String>),
    /// A harness's reader has something new that it read off this thread (`harness::Wake`).
    Read,
    /// The full pinned list, in order.
    SetPinned(Vec<String>),
    /// The full list of pinned worktrees (lane ids), in order.
    SetPinnedWorktrees(Vec<String>),
    /// The full list of pinned PRs (`owner/name#number`), in order.
    SetPinnedPrs(Vec<String>),
    /// The full list of pinned repos (a board's repo keys), in order.
    SetPinnedRepos(Vec<String>),
    /// Sessions archived (true) or unarchived from Codealong; Codex has already done its part,
    /// but for the ones in the set, which it wouldn't archive just then (`Adapter::archive`).
    /// The reply is the time of the board that shows them, published at once.
    SetArchived(Vec<String>, bool, HashSet<String>, Sender<i64>),
    /// The theme, already saved to config.toml.
    SetTheme(Theme),
    /// The board windows' zoom, already saved to config.toml.
    SetZoom(u32),
    /// Sent every second while the perf log is on; how late it's read is the engine's lag.
    Ping(Instant),
    /// A change to a todo or to the labels, from a window. The reply is the time of the board
    /// that shows it.
    Todo(TodoOp, Sender<std::result::Result<i64, String>>),
    /// A session started from todos (one, or several on one prompt): its worktree (none for a
    /// new one), and the session's key when Codealong chose its ID, or the start to wait for.
    TodoStarted { ids: Vec<String>, lane: Option<String>, session: Option<String>, pending: Option<todos::PendingStart> },
    /// A decision model's answer to a question the engine asked (`ask_jev`), by the id it was
    /// asked under.
    JevDone(String, std::result::Result<serde_json::Value, String>),
    /// "Looks done" was wrong for this todo, for now.
    DismissTodoHint(String),
    /// A session Codealong runs changed in a way the board shows (acp.rs).
    Hosted,
    /// A side conversation was asked, answered, ended, closed or forgotten (side.rs `glances`).
    Sides,
    /// A terminal of Codealong's own opened, closed or changed in a way the board lists
    /// (terminals.rs `glances`).
    Terminals,
    /// A listening started, ended, or has something new to say of itself (listen.rs `glances`).
    Listenings,
    /// A read of a folder's files came back after the scan that asked for it went on (held.rs).
    Landed,
    /// The servers listening in worktrees changed (previews.rs).
    Servers(Vec<previews::Server>),
    /// A worktree's preview URL was set or cleared; the reply is the time of the board that
    /// shows it.
    Previews(Sender<i64>),
    /// A worktree was marked reusable or not; the reply is the time of the board that shows it.
    Reusable(Sender<i64>),
    /// Files a window's change just wrote (a session renamed in its harness's record), read now
    /// rather than when the watcher says. The reply is the time of the board that shows it,
    /// published at once.
    Reread(Vec<PathBuf>, Sender<i64>),
    /// A PR the board doesn't list, as GitHub has it, to match to worktrees and sessions for a
    /// link to it (`board::pr_card`): the repo as the link names it, the PR, and who `gh` is.
    PrCard(String, PrRaw, Option<String>, Sender<PrItem>),
}

/// The board before the first scan has finished: loading, with the theme, zoom, glass, shortcuts
/// and experiments config.toml has, so windows open in them. The app's copy starts from it too, until
/// the server's first board comes (view.rs).
pub fn starting_board() -> Board {
    let shortcuts = config::shortcuts();
    Board {
        loading: true,
        theme: config::theme(),
        zoom: config::zoom(),
        glass: config::glass(),
        show_keys: config::show_keys(),
        experimental: config::experimental(),
        quick_shortcut: shortcuts.quick_panel,
        chat_shortcut: shortcuts.chat,
        waiting_shortcut: shortcuts.waiting,
        improve_shortcut: shortcuts.improve,
        main_window_shortcut: shortcuts.main_window,
        talk_shortcut: shortcuts.talk,
        talk_orchestrator_shortcut: shortcuts.talk_orchestrator,
        listen_shortcut: shortcuts.listen,
        ..Default::default()
    }
}

pub fn start(emit: impl Fn(&Board) + Send + 'static) -> Handle {
    let (tx, rx) = channel();
    let board = Arc::new(RwLock::new(starting_board()));
    let terminal_use = crate::stale::lists_tabs().then(|| Arc::new(Mutex::new(crate::stale::Uses::load())));
    let handle = Handle { tx: tx.clone(), board: board.clone(), terminal_use: terminal_use.clone() };
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
            // Every change waits on this thread to reach the windows, and its own work is small
            // (a few percent of a core). In the utility class it waited behind the agents' builds:
            // at a load of 30 to 130 on ten cores, 15 to 40 s, while composing a board took 66 ms
            // (2026-10-20). Git, the heavy part, keeps to its own utility threads.
            crate::os::system::user_initiated_thread();
            let back = tx.clone();
            // The engine's making is watched too: a panic in it (a thread it starts that the OS
            // has no room for) left the socket answering a board that never loads. There's no
            // engine to carry on with, so it's the host's to act on at once.
            let made = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
                let mut engine = Engine::new(Box::new(emit), tx, board);
                engine.terminal_use = terminal_use;
                engine.github.keep_heads_in(github::heads_file());
                engine.servers = previews::watch(move |servers| back.send(Msg::Servers(servers)).is_ok());
                engine
            }));
            match made {
                Ok(mut engine) => guard::run_guarded(&mut engine, &rx),
                Err(panic) => guard::give_up(&format!("it couldn't be made: {}", crate::carryon::panic_words(panic.as_ref())), false),
            }
        })
        .expect("engine thread");
    handle
}

/// One synchronous pass for `--snapshot`: everything the board needs, no watchers. Claude usage
/// is read only when asked: the usage endpoint rate-limits, and the app polls it already.
pub fn snapshot(with_github: bool, with_usage: bool) -> Board {
    snapshot_engine(with_github, with_usage).compose()
}

/// `--snapshot --pr-card`: a PR as a link to it gets it when the board doesn't list it.
pub fn snapshot_pr_card(slug: &str, number: u64) -> Result<PrItem> {
    let (raw, login) = github::one(slug, number)?;
    let e = snapshot_engine(false, false);
    let lanes = e.compose().lanes;
    Ok(e.pr_card(&lanes, slug, &raw, login.as_deref()))
}

fn snapshot_engine(with_github: bool, with_usage: bool) -> Engine {
    let (tx, _rx) = channel();
    let mut e = Engine::new(Box::new(|_| {}), tx, Arc::new(RwLock::new(Board::default())));
    e.passing = true;
    // Which worktrees are at a merged PR's last commit, as the server's engine last heard.
    e.github.read_heads_from(&github::heads_file());
    let t = Instant::now();
    let lap = |label: &str, t: &Instant| eprintln!("  {label:<10} {:>6} ms", t.elapsed().as_millis());
    e.load_config();
    e.load_state();
    e.load_todos();
    lap("config", &t);
    e.scan_sessions();
    lap("sessions", &t);
    e.load_repos();
    lap("repos", &t);
    e.refresh_visitors(true);
    lap("visitors", &t);
    e.load_pr_links();
    e.load_reviews();
    e.scan_pr_marks(true);
    lap("pr marks", &t);
    e.load_previews();
    e.load_reusable();
    e.previews.servers = previews::scan_once(&e.server_roots());
    lap("previews", &t);
    e.load.on_tick(now_ms(), crate::os::system::system);
    e.loading = false;
    let _ = e.compose();
    lap("1st paint", &t);
    let jobs: Vec<(PathBuf, Option<String>)> = e.git_targets().into_iter().map(|p| (p.clone(), e.base_for(&p))).collect();
    for (p, info) in parallel_map(jobs, |(p, base)| Some((p.clone(), git::status(&p, base.as_deref())))) {
        e.git.insert(p, info);
    }
    lap("git", &t);
    if with_usage {
        for u in usage::read_all(&e.cfg.accounts) {
            e.usage.insert(u.account.clone(), u);
        }
        lap("usage", &t);
    }
    if with_github {
        let slugs: Vec<String> = board::polled_slugs(&e.seeds).into_iter().collect();
        e.github.set_marked(board::worked_prs(&e.pr_links(), &e.compose().lanes));
        let made = e.github.made_to_read(&e.seeds);
        for answer in github::read_once(&slugs, &made) {
            e.github.take(answer, &e.seeds, Now::real());
        }
        e.github.took_all(slugs);
        lap("github", &t);
    }
    let answers = parallel_map(e.team_file_asks(), |ask| Some(teamfile::read(&ask)));
    e.team_files.on_read(Some(answers));
    lap("team files", &t);
    e
}

/// The board without git, GitHub, visitors or usage: sessions, worktrees and todos as the
/// files say, in a couple of seconds. For the `codealong` command when no server answers.
pub fn light_snapshot() -> Board {
    let (tx, _rx) = channel();
    let mut e = Engine::new(Box::new(|_| {}), tx, Arc::new(RwLock::new(Board::default())));
    e.passing = true;
    e.load_config();
    e.load_state();
    e.load_todos();
    e.scan_sessions();
    e.load_repos();
    e.load_previews();
    e.load_reusable();
    e.previews.servers = previews::scan_once(&e.server_roots());
    e.loading = false;
    e.compose()
}

/// What's done about a fall of the engine's thread: its first scan and its loop run under
/// `guard::run_guarded`, and a host says what an engine that can't get going comes to with
/// `when_spent`.
mod guard;
pub use guard::when_spent;

// The engine's parts, a file a subject (the header maps them). Each starts with `use super::*`,
// so the imports above serve them all; what a part makes `pub` is named from here, and what
// this file or another part calls of one is `pub(super)`.
mod emblems;
mod gitruns;
mod handle;
mod jevasks;
mod open;
mod polls;
mod readers;
mod records;
mod reposcan;
mod seen;
mod sessions;
mod todofile;
mod trouble;
mod watch;
use gitruns::*;
pub use handle::*;
pub use open::*;
use records::*;
pub(crate) use reposcan::*;
pub use trouble::*;
// What only tests outside its part name.
#[cfg(test)]
use watch::is_noise;

struct Engine {
    emit: Box<dyn Fn(&Board) + Send>,
    tx: Sender<Msg>,
    board: Arc<RwLock<Board>>,
    cfg: Config,
    /// config.toml as last read or written (`None` when there was none or it didn't read), so an
    /// edit the watcher didn't hear can be told from what's loaded.
    config_text: Option<String>,
    /// What's going wrong in the background, each with why, until it goes right again.
    troubles: BTreeMap<Trouble, String>,
    /// How often the engine's thread has fallen over and been carried on (`run_guarded`).
    fell: u32,
    /// A board has gone out since this was last asked (`run_guarded` asks at each fall): what
    /// tells an engine doing its work from one that can't get going.
    passed: bool,
    /// The first scan has gone through, once: this engine worked, whatever became of it after.
    got_going: bool,
    /// config.toml as read is applied whole, with everything that hangs off the settings read
    /// again where it needed to be: false from when `load_config` starts until what called it
    /// is done (`full_scan`, `reload`, `read_all_again`), and so after a fall partway through,
    /// when what's applied is neither the old settings nor the new.
    config_settled: bool,
    seen: SeenState,
    /// state.json is there but doesn't read: the engine runs on what it has in memory (a first
    /// run's state at launch) and writes nothing over the file until it's fixed or deleted.
    state_unreadable: bool,
    /// Todos and labels, as in todos.json.
    todos: TodoStore,
    /// todos.json as last read or written, so the watcher can tell someone else's edit from ours.
    todos_text: String,
    /// todos.json is there but doesn't read as a todo list: nothing is written over it, and
    /// changes are turned away, until it's fixed or deleted.
    todos_unreadable: bool,
    checkouts: Vec<(String, Checkout)>,
    seeds: Vec<LaneSeed>,
    /// `<main>/.git/worktrees/<name>` → the linked worktree's path.
    gitdir_to_lane: HashMap<PathBuf, PathBuf>,
    ignores: HashMap<PathBuf, Gitignore>,
    /// Each harness's sessions as its files last said (`harness::Reader`), in the adapters' order.
    readers: Vec<Box<dyn harness::Reader>>,
    /// The repo scan's reads of files in folders it doesn't own, which macOS can hold.
    reads: RepoReads,
    /// When a held read last landed: the repos are read again once they stop landing.
    landed_at: Option<Instant>,
    /// Worktrees whose files the last scan couldn't read. Git would wait on them too, so it
    /// runs there once a scan has read them.
    held_worktrees: HashSet<PathBuf>,
    /// Hash of the last published board, to skip publishing when nothing changed.
    last_hash: u64,
    /// The last published board's number (`Board::rev`).
    rev: u64,
    /// What asked for a publish since the last, for the perf log to say when one changed
    /// nothing. Kept only while the log is on.
    asked_by: BTreeSet<String>,
    /// Sessions from other lanes whose logs show they worked in a worktree.
    visitors: Visitors,
    /// What to look for in logs: (lane path, text that identifies it, when it was made).
    needles: Vec<(PathBuf, String, i64)>,
    /// Session key → the PRs its records show it made or changed, each with its first mark's time.
    pr_marks: prlinks::Marks,
    /// Session key → the branches its records show it pushed to, each push with its time.
    pr_pushes: prlinks::Pushes,
    /// Session key → the pages on the web its records show it made or showed you, the latest
    /// first (artifacts.rs).
    artifacts: artifacts::Made,
    /// The records read whole for those, the PRs and the Chrome tabs (`scan_pr_marks`,
    /// `read_found_logs`): from then on only their new lines are.
    logs_read: HashSet<PathBuf>,
    /// Session key → the Chrome tabs its records show it drove (chrometabs.rs).
    chrome_use: HashMap<String, chrometabs::Use>,
    /// A headless run's key → the session that started it (`with_run_origins`), found while its
    /// process runs and kept after, while it's on the board, and written to origins.json once
    /// its process has ended (`keep_run_origins`). Learned as the board is composed, which only
    /// reads the engine, so behind a lock.
    run_origins: Mutex<HashMap<String, String>>,
    /// Chrome's profiles, as of the last lookup of a session's.
    chrome_profiles: Vec<chrome::Profile>,
    /// Session key → the tab group its profile was last looked up for, and when.
    chrome_lookups: HashMap<String, (Option<i64>, Instant)>,
    /// Links to PRs made or taken away by hand (`pr-links.json`).
    pr_store: prlinks::LinkStore,
    /// Where the automatic reviews stand (`reviews.json`), for the PRs' cards; autoreview.rs
    /// keeps the file.
    reviews: autoreview::Store,
    /// Why an automatic review couldn't be started or posted, until a later one goes.
    review_warning: Option<autoreview::Said>,
    /// The schedules (`schedules.json`) and what's happened to them (`schedule-runs.json`), as
    /// last read, for the Schedules view; the scheduler (schedules/runner.rs) keeps the runs.
    schedules: crate::schedules::File,
    schedule_runs: crate::schedules::Runs,
    /// Why one of those files doesn't read, while it doesn't.
    schedules_problem: Option<String>,
    /// Worktrees' preview URLs (`previews.json`) and the servers in their folders.
    previews: board::Previews,
    /// Worktrees marked reusable or not by hand (`worktrees.json`), by path.
    reusable: HashMap<String, bool>,
    /// The PRs another of your devices runs a session on (`Handle::peer_lanes`).
    running_elsewhere: std::collections::BTreeSet<String>,
    /// Your machines that share automations (`Handle::peer_machines`).
    machines: std::sync::Arc<crate::shared::Machines>,
    /// The files changed in the background whose last change couldn't be made, by name with
    /// why (`store::problems`), as of the last board.
    store_warnings: Vec<(String, String)>,
    /// The thread that finds servers, told which worktrees to look in; none in a snapshot.
    servers: Option<previews::Watcher>,
    git: HashMap<PathBuf, GitInfo>,
    git_jobs: Sender<GitJob>,
    /// Which worktrees git runs in, and which wait their turn (see throttle.rs).
    git_queue: GitQueue<PathBuf>,
    /// What events the watcher dropped call for (throttle.rs `Drops`).
    drops: Drops<PathBuf>,
    /// Checkout root → default branch ref (`origin/main`).
    default_branch: HashMap<PathBuf, Option<String>>,
    lane_base: HashMap<PathBuf, Option<String>>,
    /// Lane path → the main checkout it belongs to.
    lane_checkout: HashMap<PathBuf, PathBuf>,
    /// The checkouts of Codealong's source the scan found, in a project or not: each one's main
    /// checkout and identity (`github.com/owner/name`).
    codealong_sources: Vec<(PathBuf, Option<String>)>,
    /// GitHub as the engine has it: the PRs read, when it's asked again, and what it calls each
    /// repo now (ghpoll.rs).
    github: GhPoll,
    /// An engine for one pass (`--snapshot`, the command without the app): it's gone before
    /// what it would ask in the background comes back.
    passing: bool,
    watcher: Option<RecommendedWatcher>,
    /// Account folders that didn't exist at the last watch: a new account before sign-in.
    missing_homes: Vec<PathBuf>,
    /// Usage by account for the harnesses whose usage is asked for in the background
    /// (`Adapter::polls_usage`), from the last read.
    usage: HashMap<String, Usage>,
    usage_inflight: bool,
    usage_last: Option<Instant>,
    usage_backoff: bool,
    /// The processes last listed (`list_processes`), for every look the readers take, so none
    /// of them runs `ps` on this thread. None until the first scan lists them.
    processes: Option<Arc<procs::Listed>>,
    /// A tick's process table is being listed.
    processes_inflight: bool,
    /// When to look for sessions that wait for a tab of your terminal (`launch::show_unlocked`).
    tab_looks: launch::TabLooks,
    /// How busy this machine is (load.rs), read every half a minute.
    load: crate::load::Reading,
    /// When each terminal tab was last used, a redraw of every tab not counted, for Close stale
    /// terminals (`stale::Uses`), shared with the handle. Only the app's engine keeps it and
    /// writes it; a snapshot's or a test's has none.
    terminal_use: Option<Arc<Mutex<crate::stale::Uses>>>,
    /// Each repo's team settings, `.codealong/config.toml` on its branches (teamfile.rs).
    team_files: teamfile::TeamFiles,
    /// Whether each account's provider may train on it (training.rs).
    training: training::TrainingPoll,
    /// The limit resets each account holds (resets.rs).
    resets: crate::resets::ResetPoll,
    /// The boundary groups' members' sign-ins (signin.rs).
    sign_ins: crate::signin::SignInPoll,
    /// The MCP servers each account's agents have (mcp.rs).
    mcp: crate::mcp::McpPoll,
    /// What the terminals of starts waiting for their session show (starting.rs).
    screens: crate::starting::Screens,
    dirty: bool,
    /// Only git results changed since the last publish; they go out in batches.
    dirty_git: bool,
    loading: bool,
    last_publish: Option<Instant>,
    perf: perf::Counters,
    /// Questions for decision models go to their own thread (decisions.rs); answers come back as
    /// `Msg::JevDone`.
    jev_jobs: Sender<crate::decisions::Job>,
    /// Ids asked and not answered yet, so nothing is asked twice at once.
    jev_pending: HashSet<String>,
    /// What Jev was asked about todos and said (todojev.rs), kept in jev-todos.json.
    jev_todos: todojev::Memory,
    /// Todo id → the signature its "looks done" question was asked at and what it's based on,
    /// while it's out.
    jev_done_asking: HashMap<String, (String, String)>,
    /// Session key → the todo ids its link question's options stand for, while it's out.
    jev_link_asking: HashMap<String, Vec<String>>,
    /// jev-todos.json has been read: the first look reads it.
    jev_todos_loaded: bool,
    /// When a todo question last failed (no key, TypeSafe down): nothing more is asked for a while.
    jev_todos_failed: Option<Instant>,
    /// Jev's reads of sessions, by the moment each was asked about (insights.rs).
    insights: crate::insights::Store,
    /// Each session's emblem: the emoji Jev picked or the picture drawn for it (emblems.rs).
    emblems: crate::emblems::Store,
}

impl Engine {
    fn new(emit: Box<dyn Fn(&Board) + Send>, tx: Sender<Msg>, board: Arc<RwLock<Board>>) -> Engine {
        let git_jobs = spawn_git_workers(tx.clone());
        let reads = RepoReads::new(&tx);
        let back = tx.clone();
        let github = GhPoll::new(Arc::new(move |event| {
            let _ = back.send(Msg::Github(event));
        }));
        let back = tx.clone();
        let jev_jobs = crate::decisions::worker(move |id, answer| {
            let _ = back.send(Msg::JevDone(id, answer));
        });
        let back = Mutex::new(tx.clone());
        let wake: harness::Wake = Arc::new(move || {
            if let Ok(back) = back.lock() {
                let _ = back.send(Msg::Read);
            }
        });
        Engine {
            emit,
            tx,
            board,
            cfg: Config::default(),
            config_text: None,
            troubles: BTreeMap::new(),
            fell: 0,
            passed: false,
            got_going: false,
            config_settled: false,
            seen: SeenState::default(),
            state_unreadable: false,
            todos: TodoStore::default(),
            todos_text: String::new(),
            todos_unreadable: false,
            checkouts: Vec::new(),
            seeds: Vec::new(),
            gitdir_to_lane: HashMap::new(),
            ignores: HashMap::new(),
            readers: harness::readers(&wake),
            reads,
            landed_at: None,
            held_worktrees: HashSet::new(),
            last_hash: 0,
            rev: 0,
            asked_by: BTreeSet::new(),
            visitors: Visitors::default(),
            needles: Vec::new(),
            pr_marks: HashMap::new(),
            pr_pushes: HashMap::new(),
            artifacts: HashMap::new(),
            logs_read: HashSet::new(),
            chrome_use: HashMap::new(),
            run_origins: Mutex::default(),
            chrome_profiles: Vec::new(),
            chrome_lookups: HashMap::new(),
            pr_store: prlinks::LinkStore::default(),
            reviews: autoreview::Store::default(),
            review_warning: None,
            schedules: crate::schedules::File::default(),
            schedule_runs: crate::schedules::Runs::default(),
            schedules_problem: None,
            previews: board::Previews::default(),
            reusable: HashMap::new(),
            running_elsewhere: Default::default(),
            machines: Default::default(),
            store_warnings: Vec::new(),
            servers: None,
            git: HashMap::new(),
            git_jobs,
            git_queue: GitQueue::default(),
            drops: Drops::default(),
            default_branch: HashMap::new(),
            lane_base: HashMap::new(),
            lane_checkout: HashMap::new(),
            codealong_sources: Vec::new(),
            github,
            passing: false,
            watcher: None,
            missing_homes: Vec::new(),
            usage: HashMap::new(),
            usage_inflight: false,
            usage_last: None,
            usage_backoff: false,
            processes: None,
            processes_inflight: false,
            tab_looks: launch::TabLooks::default(),
            load: crate::load::Reading::default(),
            terminal_use: None,
            team_files: teamfile::TeamFiles::default(),
            training: training::TrainingPoll::default(),
            resets: crate::resets::ResetPoll::default(),
            sign_ins: crate::signin::SignInPoll::default(),
            mcp: crate::mcp::McpPoll::default(),
            screens: crate::starting::Screens::default(),
            dirty: false,
            loading: true,
            dirty_git: false,
            last_publish: None,
            perf: perf::Counters::default(),
            jev_jobs,
            jev_pending: HashSet::new(),
            jev_todos: todojev::Memory::default(),
            jev_done_asking: HashMap::new(),
            jev_link_asking: HashMap::new(),
            jev_todos_loaded: false,
            jev_todos_failed: None,
            insights: crate::insights::Store::load(),
            emblems: crate::emblems::Store::load(),
        }
    }

    fn full_scan(&mut self) {
        self.load_config();
        self.load_state();
        // From the first board on, and for `keep_last_good` should the first read be turned away.
        self.usage = usage::kept(&self.cfg.accounts, now_ms());
        self.remember_accounts();
        self.save_state();
        self.load_todos();
        // Only with every todo and emblem read: a list that didn't read would make every
        // attachment look unused. Pictures drawn for emblems are kept images too.
        if let (false, Some(pictures)) = (self.todos_unreadable, self.emblems.pictures()) {
            let texts: Vec<String> = self.todos.todos.iter().map(|t| t.description.clone()).chain(pictures).collect();
            let _ = std::thread::Builder::new().name("codealong-attachments".into()).spawn(move || {
                attachments::collect_garbage(&texts.iter().map(String::as_str).collect::<Vec<_>>());
            });
        }
        self.scan_sessions();
        self.load_repos();
        // Before the first board: it says which sessions are automatic reviews (`with_origins`),
        // which their rounds carry on after a restart and reconnect.rs mustn't. Read again below,
        // once the watcher runs.
        self.load_reviews();
        self.loading = false;
        self.publish_for("first board");
        self.queue_git_all();
        self.github.poll(&self.seeds, true, Now::real());
        self.fetch_usage();
        self.poll_readers();
        self.watch();
        // The scan read config.toml and todos.json before the watcher started, and can take
        // minutes on a busy Mac: an edit made meanwhile is read now, as the watcher would have,
        // rather than when the file next changes.
        if config::read_text().ok().flatten() != self.config_text {
            self.reload();
        }
        self.reload_todos();
        self.refresh_visitors(false);
        self.load_pr_links();
        self.load_reviews();
        self.load_schedules();
        self.scan_pr_marks(false);
        self.load_previews();
        self.load_reusable();
        self.config_settled = true;
    }

    /// Everything that depends on config.toml, read again. When nothing but the theme, the
    /// shortcuts or the accounts' order changed, a new board is enough: reading everything again
    /// also asks for Claude usage, and the usage endpoint turns away calls that come often.
    fn reload(&mut self) {
        let before = self.cfg.clone();
        self.load_config();
        if self.remember_accounts() {
            self.save_state();
        }
        let mut rest = self.cfg.clone();
        rest.appearance = before.appearance.clone();
        rest.shortcuts = before.shortcuts.clone();
        rest.debug = before.debug.clone();
        // Accounts put in another order (the usage bar's chips, dragged) are the same accounts:
        // the board lists them in the new order, and nothing needs reading again.
        if config::same_accounts(&rest.accounts, &before.accounts) {
            rest.accounts = before.accounts.clone();
        }
        // The apps are read as the board is composed; nothing to rescan for them either, nor for
        // which PRs nudge by default.
        rest.apps = before.apps.clone();
        rest.pull_requests = before.pull_requests.clone();
        // Flag protection changes scheduling and delivery, not how any record is read.
        rest.sessions.block_flagged_sessions = before.sessions.block_flagged_sessions;
        // Sounds are read as each one plays (sounds.rs).
        rest.sounds = before.sounds.clone();
        // Voice is read as a key goes down and as a listening is looked at (dictation.rs,
        // listen.rs), and the experiments only show and hide what's there.
        rest.voice = before.voice.clone();
        rest.experimental = before.experimental.clone();
        if rest == before {
            self.mark_dirty("config");
            self.config_settled = true;
            return;
        }
        self.read_all_again();
    }

    /// Everything that hangs off the settings, read again: the sessions and their records, the
    /// repos, git, GitHub, usage, and what's watched.
    fn read_all_again(&mut self) {
        self.scan_sessions();
        // The readers read their records' ends again, the lines before passed over: every record
        // is read whole again, off the engine's thread, an account's added since among them.
        self.scan_pr_marks(false);
        self.load_repos();
        self.queue_git_all();
        self.github.poll(&self.seeds, true, Now::real());
        self.usage.retain(|name, _| self.cfg.accounts.iter().any(|a| &a.name == name));
        self.usage_inflight = false;
        self.training.keep_only(&self.cfg.accounts);
        self.resets.keep_only(&self.cfg.accounts);
        self.mcp.keep_only(&self.cfg.accounts);
        self.fetch_usage();
        self.watch();
        self.mark_dirty("config");
        self.config_settled = true;
    }

    /// After the thread fell over (`run_guarded`): whatever it had in hand is gone (the changed
    /// paths it hadn't read yet, a scan half done), so everything a change could have told it is
    /// read again from where it is.
    ///
    /// Codealong's own files go through `apply_fs` as if each had just changed (state.json, the
    /// todos, the PR links, the reviews, the schedules, the previews, the reusable worktrees),
    /// so an edit the lost batch held is applied by what applies it any other time. config.toml
    /// comes after them, by itself: an edit to it, or one whose applying the fall cut short
    /// (`config_settled`), is applied whole and everything that hangs off the settings read
    /// again, GitHub and usage with it. Otherwise the sessions are scanned and their records
    /// read whole again (unless a state.json mended in the lost batch has just had that done),
    /// the repos read again and the watcher set anew, and git runs in every worktree; GitHub
    /// and usage keep their own times, as their endpoints turn away calls that come often. The
    /// search for whose work an orphan worktree is starts over in the background whatever the
    /// orphans are: a line that tied a session to one may be behind the readers now.
    fn pick_up(&mut self) {
        let unread = self.state_unreadable;
        self.apply_fs(vec![crate::state::path(), todos::path(), prlinks::path(), autoreview::path(), crate::schedules::path(), crate::schedules::runs_path(), previews::path(), reusable::path()]);
        let scanned = unread && !self.state_unreadable;
        if !self.config_settled || config::read_text().ok().flatten() != self.config_text {
            self.load_config();
            if self.remember_accounts() {
                self.save_state();
            }
            self.read_all_again();
        } else {
            if !scanned {
                self.scan_sessions();
                self.scan_pr_marks(false);
            }
            // As `rescan_repos` reads them, without its own search for visitors: the one below
            // is over the repos as they are now, and a second would only take its place.
            self.load_repos();
            self.watch();
            self.queue_git_all();
        }
        self.search_visitors(false, true);
        self.mark_dirty("fell over");
    }

    /// Notes a fall for the board: how many there have been since the process started, and what
    /// the last one said.
    fn fell_over(&mut self, why: &str) {
        self.fell += 1;
        self.set_trouble(Trouble::Engine, Some(format!("Codealong stopped partway through its work ({why}) and carried on from what it had. What's shown may be behind for a moment; if this comes back, quitting Codealong and its server starts it afresh.")));
    }

    fn run(&mut self, rx: &Receiver<Msg>) {
        let mut pending: Vec<PathBuf> = Vec::new();
        let mut dropped: Vec<PathBuf> = Vec::new();
        let mut first_event: Option<Instant> = None;
        let mut last_event: Option<Instant> = None;
        let mut last_tick = Instant::now();
        let mut last_heartbeat = Instant::now();
        let mut last_databases = Instant::now();
        loop {
            let now = Instant::now();
            let mut wait = if !pending.is_empty() || !dropped.is_empty() || self.dirty || self.landed_at.is_some() { Duration::from_millis(50) } else { Duration::from_secs(2) };
            // Wake for a git batch that's due, or a held git run.
            if self.dirty_git {
                let due = self.last_publish.map(|t| (t + throttle::GIT_PUBLISH_EVERY).saturating_duration_since(now)).unwrap_or_default();
                wait = wait.min(due.max(Duration::from_millis(10)));
            }
            for at in [self.git_queue.next_due(), self.drops.next_due()].into_iter().flatten() {
                wait = wait.min(at.saturating_duration_since(now).max(Duration::from_millis(10)));
            }
            match rx.recv_timeout(wait) {
                Ok(Msg::Fs(paths)) => {
                    let now = Instant::now();
                    first_event.get_or_insert(now);
                    last_event = Some(now);
                    pending.extend(paths);
                }
                // In the same batch as the events around them: a drop comes with a burst.
                Ok(Msg::Dropped(paths)) => {
                    let now = Instant::now();
                    first_event.get_or_insert(now);
                    last_event = Some(now);
                    dropped.extend(paths);
                }
                Ok(Msg::GitDone(path, info)) => self.on_git(path, info),
                Ok(Msg::Github(event)) => self.on_github(event),
                Ok(Msg::RefreshGithub) => self.github.poll(&self.seeds, false, Now::real()),
                Ok(Msg::PrChanged) => self.github.on_changed(&self.seeds, Now::real()),
                Ok(Msg::Merged(slug, number, reply)) => {
                    let changed = self.github.on_merged(slug, number, &self.seeds, Now::real());
                    self.dirty_if(changed, "merged");
                    // Now rather than on the next tick: the window shows it merged meanwhile.
                    self.publish_for("merged");
                    let _ = reply.send(self.board.read().map(|b| b.generated_at).unwrap_or(0));
                }
                Ok(Msg::Approved(slug, number, verdict, commit, reply)) => {
                    let changed = self.github.on_approved(slug, number, verdict, commit, &self.seeds, Now::real());
                    self.dirty_if(changed, "approved");
                    // Now rather than on the next tick: the button waits for it.
                    self.publish_for("approved");
                    let _ = reply.send(self.board.read().map(|b| b.generated_at).unwrap_or(0));
                }
                Ok(Msg::Visitors(scan, found)) => {
                    let changed = self.visitors.scanned(scan, found);
                    self.dirty_if(changed, "visitors");
                }
                Ok(Msg::PrMarks(found, pushed)) => {
                    // Marks and pushes read from new lines while the scan ran are kept.
                    for (key, prs) in found {
                        let have = self.pr_marks.entry(key).or_default();
                        for (pr, at) in prs {
                            prlinks::add_mark(have, pr, at);
                        }
                    }
                    for (key, pushes) in pushed {
                        let have = self.pr_pushes.entry(key).or_default();
                        for (branch, at) in pushes {
                            prlinks::add_push(have, branch, at);
                        }
                    }
                    self.mark_dirty("PR marks scan");
                }
                Ok(Msg::Artifacts(found)) => {
                    // Lines read as they were written while the scan ran are kept, or taken
                    // over by what the scan read after them.
                    for (key, made) in found {
                        let have = self.artifacts.entry(key).or_default();
                        for a in made {
                            artifacts::add(have, a);
                        }
                    }
                    self.mark_dirty("artifacts scan");
                }
                Ok(Msg::ChromeUse(found, profiles)) => self.take_chrome_use(found, profiles),
                Ok(Msg::ChromeProfile(key, group, dir, profiles)) => {
                    self.chrome_profiles = profiles;
                    if let Some(u) = self.chrome_use.get_mut(&key).filter(|u| u.group == group) {
                        if dir.is_some() && u.profile != dir {
                            u.profile = dir;
                            self.mark_dirty("Chrome profile");
                        }
                    }
                }
                Ok(Msg::RefreshUsage) => {
                    self.fetch_usage();
                    self.training.on_refresh();
                    self.read_training();
                    self.resets.on_refresh();
                    self.read_resets();
                    self.mcp.on_refresh();
                    self.read_mcp();
                }
                Ok(Msg::Went(what, why)) => {
                    self.set_trouble(what, why);
                    self.mark_dirty("a background job went");
                }
                Ok(Msg::PrLinksChanged) => {
                    self.load_pr_links();
                    self.mark_dirty("PR links");
                }
                // Another device's board comes often; only what it runs on PRs is looked at.
                Ok(Msg::RunningElsewhere(prs)) if prs != self.running_elsewhere => {
                    self.running_elsewhere = prs;
                    self.mark_dirty("running elsewhere");
                }
                Ok(Msg::RunningElsewhere(_)) => {}
                Ok(Msg::Machines(m)) if m != *self.machines => {
                    // Kept either way; a board is composed only for more than the times, which the
                    // next one has (the heartbeat's at most 30 s on).
                    let moved = !m.same_but_times(&self.machines);
                    self.machines = std::sync::Arc::new(m);
                    self.dirty_if(moved, "machines");
                }
                Ok(Msg::Machines(_)) => {}
                Ok(Msg::ShownUnlocked(why, deferred)) => {
                    self.tab_looks.on_looked(deferred);
                    self.set_trouble(Trouble::Unlocked, why);
                    self.mark_dirty("unlocked");
                }
                Ok(Msg::Reviewed(why)) => {
                    self.review_warning = why;
                    self.load_reviews();
                    self.mark_dirty("reviews");
                }
                Ok(Msg::Hosted) => self.mark_dirty("hosted sessions"),
                Ok(Msg::Read) => self.mark_dirty("a reader's poll"),
                Ok(Msg::Sides) => self.mark_dirty("side conversations"),
                Ok(Msg::Terminals) => self.mark_dirty("Codealong's terminals"),
                Ok(Msg::Listenings) => self.mark_dirty("listenings"),
                Ok(Msg::SetArchived(keys, archived, only_here, reply)) => {
                    let now = now_ms();
                    for key in &keys {
                        self.seen.set_archived(key, archived, now, only_here.contains(key));
                    }
                    self.save_state();
                    // A harness with an archive of its own reads what's in it now.
                    let look = Self::look(&self.seen, &self.processes);
                    let harnesses: HashSet<_> = keys.iter().filter_map(|k| k.split(':').next().and_then(harness::named)).map(|a| a.harness()).collect();
                    for r in self.readers.iter_mut().filter(|r| harnesses.contains(&r.harness())) {
                        r.archives_changed(&harness::accounts_of(&self.cfg, r.harness()), &look);
                    }
                    // Now rather than on the next tick: the window shows it already, and waits for it.
                    self.publish_for("archived");
                    let _ = reply.send(self.board.read().map(|b| b.generated_at).unwrap_or(0));
                }
                Ok(Msg::SetPinned(keys)) => {
                    self.seen.set_pinned(keys);
                    self.save_state();
                    self.mark_dirty("pins");
                }
                Ok(Msg::SetPinnedWorktrees(ids)) => {
                    self.seen.set_pinned_worktrees(ids);
                    self.save_state();
                    self.mark_dirty("pinned worktrees");
                }
                Ok(Msg::SetPinnedPrs(keys)) => {
                    self.seen.set_pinned_prs(keys);
                    self.save_state();
                    self.mark_dirty("pinned PRs");
                }
                Ok(Msg::SetPinnedRepos(keys)) => {
                    self.seen.set_pinned_repos(keys);
                    self.save_state();
                    self.mark_dirty("pinned repos");
                }
                Ok(Msg::SetTheme(theme)) => {
                    // Published now, not on the next tick: the app's appearance follows the board.
                    self.cfg.appearance.theme = theme;
                    self.publish_for("theme");
                }
                Ok(Msg::SetZoom(zoom)) => {
                    self.cfg.appearance.zoom = zoom;
                    self.publish_for("zoom");
                }
                Ok(Msg::Ping(sent)) => perf::lag("engine", sent.elapsed()),
                Ok(Msg::Todo(op, reply)) => {
                    let _ = reply.send(self.change_todos(op));
                }
                Ok(Msg::JevDone(id, answer)) => self.on_jev(id, answer),
                Ok(Msg::DismissTodoHint(id)) => self.dismiss_todo_hint(id),
                // A prompt answered lands many reads at once; they're used together.
                Ok(Msg::Landed) => self.landed_at = Some(Instant::now()),
                Ok(Msg::Servers(servers)) => {
                    self.previews.servers = servers;
                    self.mark_dirty("preview servers");
                }
                Ok(Msg::Previews(reply)) => {
                    self.load_previews();
                    // Now rather than on the next tick: the window waits for it.
                    self.publish_for("previews");
                    let _ = reply.send(self.board.read().map(|b| b.generated_at).unwrap_or(0));
                }
                Ok(Msg::Reusable(reply)) => {
                    self.load_reusable();
                    self.publish_for("reusable");
                    let _ = reply.send(self.board.read().map(|b| b.generated_at).unwrap_or(0));
                }
                Ok(Msg::PrCard(slug, raw, login, reply)) => {
                    let item = match self.board.read() {
                        Ok(b) => self.pr_card(&b.lanes, &slug, &raw, login.as_deref()),
                        Err(_) => self.pr_card(&[], &slug, &raw, login.as_deref()),
                    };
                    let _ = reply.send(item);
                }
                Ok(Msg::Reread(paths, reply)) => {
                    self.apply_fs(paths);
                    // Now rather than on the next tick: the window shows it already, and waits for it.
                    self.publish_for("reread");
                    let _ = reply.send(self.board.read().map(|b| b.generated_at).unwrap_or(0));
                }
                Ok(Msg::TodoStarted { ids, lane, session, pending }) => self.todos_started(ids, lane, session, pending),
                Ok(Msg::UsageDone(list)) => self.on_usage(list),
                Ok(Msg::Processes(listed)) => self.on_processes(listed),
                Ok(Msg::TeamFiles(answers)) => {
                    let changed = self.team_files.on_read(answers);
                    self.dirty_if(changed, "team files");
                }
                Ok(Msg::TrainingRead(list)) => {
                    let changed = self.training.on_read(list);
                    self.dirty_if(changed, "training");
                }
                Ok(Msg::SignInsRead(list)) => {
                    let changed = self.sign_ins.on_read(list);
                    self.dirty_if(changed, "sign-ins");
                }
                Ok(Msg::TrainingOne(asked_as, t)) => {
                    let changed = self.training.on_changed(&asked_as, t);
                    self.dirty_if(changed, "training");
                }
                Ok(Msg::ResetsRead(list)) => {
                    let changed = self.resets.on_read(list);
                    self.dirty_if(changed, "limit resets");
                }
                Ok(Msg::ResetsOne(asked_as, r)) => {
                    let changed = self.resets.on_changed(&asked_as, r);
                    self.dirty_if(changed, "limit resets");
                }
                Ok(Msg::McpRead(list)) => {
                    let changed = self.mcp.on_read(list);
                    self.dirty_if(changed, "MCP");
                }
                Ok(Msg::Screens(found)) => {
                    let changed = self.screens.on_read(found);
                    self.dirty_if(changed, "screens");
                }
                Ok(Msg::StartAnswered(at)) => self.screens.again(at),
                Ok(Msg::EmblemDrawn(key, drawn)) => self.on_emblem_drawn(&key, drawn),
                Ok(Msg::MarkSeen(keys, at, reply)) => {
                    let now = now_ms();
                    let taken_up: Vec<String> = self.board.read().map(|b| keys.iter().filter(|k| b.session(k).is_some_and(|(_, s)| board::seen_takes_up(s))).cloned().collect()).unwrap_or_default();
                    for k in keys {
                        // As of then, never back from a later mark.
                        let at = at.map_or(now, |at| at.max(self.seen.seen.get(&k).copied().unwrap_or(0)));
                        self.seen.mark(&k, at);
                    }
                    for k in taken_up {
                        self.seen.take_up(&k);
                    }
                    self.save_state();
                    self.publish_for("taken up");
                    if let Some(reply) = reply {
                        let _ = reply.send(self.board.read().map(|b| b.generated_at).unwrap_or(0));
                    }
                }
                Ok(Msg::MarkUnread(keys, reply)) => {
                    let now = now_ms();
                    for k in keys {
                        self.seen.mark_unread(&k, now);
                    }
                    self.save_state();
                    self.publish_for("marked unread");
                    if let Some(reply) = reply {
                        let _ = reply.send(self.board.read().map(|b| b.generated_at).unwrap_or(0));
                    }
                }
                Ok(Msg::AgentSeen(key, seen, reply)) => {
                    self.seen.mark_agent_seen(&key, AgentSeen { at: now_ms(), ..seen });
                    self.save_state();
                    self.publish_for("agent seen");
                    let _ = reply.send(self.board.read().map(|b| b.generated_at).unwrap_or(0));
                }
                Ok(Msg::Snooze(keys, until, reply)) => {
                    let now = now_ms();
                    for k in keys {
                        match until {
                            // Snoozed, it's no longer first: the two are opposites.
                            Some(until) if until > now => {
                                self.seen.snooze(&k, now, until);
                                self.seen.lower(&k, now, true);
                            }
                            _ => self.seen.wake(&k, now),
                        }
                    }
                    self.save_state();
                    // Now rather than on the next tick: the window shows it already, and waits for it.
                    self.publish_for("seen");
                    let _ = reply.send(self.board.read().map(|b| b.generated_at).unwrap_or(0));
                }
                Ok(Msg::Aside(keys, reply)) => {
                    let now = now_ms();
                    for k in keys {
                        self.seen.set_aside(&k, now);
                    }
                    self.save_state();
                    self.publish_for("set aside");
                    let _ = reply.send(self.board.read().map(|b| b.generated_at).unwrap_or(0));
                }
                Ok(Msg::SnoozeReviews(keys, until, reply)) => {
                    let now = now_ms();
                    for k in keys {
                        match until {
                            Some(until) if until > now => self.seen.snooze_review(&k, now, until),
                            _ => self.seen.wake_review(&k, now),
                        }
                    }
                    self.save_state();
                    self.publish_for("snoozed");
                    let _ = reply.send(self.board.read().map(|b| b.generated_at).unwrap_or(0));
                }
                Ok(Msg::Suggested(reply)) => {
                    self.publish_for("suggested");
                    let _ = reply.send(self.board.read().map(|b| b.generated_at).unwrap_or(0));
                }
                Ok(Msg::Quiet(keys, quiet, reply)) => {
                    let mut changed = false;
                    for k in keys {
                        changed |= self.seen.set_quiet(&k, quiet);
                    }
                    if changed {
                        self.save_state();
                        self.publish_for("quiet");
                    }
                    let _ = reply.send(self.board.read().map(|b| b.generated_at).unwrap_or(0));
                }
                Ok(Msg::Label(keys, add, remove, reply)) => {
                    let answer = self.on_label(&keys, &add, &remove);
                    if answer == Ok(true) {
                        self.save_state();
                        self.publish_for("label");
                    }
                    let _ = reply.send(answer.map(|_| self.board.read().map(|b| b.generated_at).unwrap_or(0)));
                }
                Ok(Msg::Raise(keys, raised, by, reply)) => {
                    let answer = self.on_raise(&keys, raised, by, now_ms());
                    if answer == Ok(true) {
                        self.save_state();
                        self.publish_for("raised");
                    }
                    let _ = reply.send(answer.map(|_| self.board.read().map(|b| b.generated_at).unwrap_or(0)));
                }
                Ok(Msg::Pin(keys, pinned, reply)) => {
                    let keys: Vec<String> = self.board.read().map(|b| board::pinnable(&b.lanes, &keys, pinned).into_iter().cloned().collect()).unwrap_or_default();
                    let mut changed = false;
                    // In reverse, so sessions pinned together keep the order they were asked in.
                    for k in keys.iter().rev() {
                        changed |= self.seen.pin(k, pinned);
                    }
                    if changed {
                        self.save_state();
                        self.publish_for("pinned");
                    }
                    let _ = reply.send(self.board.read().map(|b| b.generated_at).unwrap_or(0));
                }
                Ok(Msg::Own(keys, reply)) => {
                    let mut changed = false;
                    for k in keys {
                        changed |= self.seen.make_own(&k);
                    }
                    if changed {
                        self.save_state();
                        self.publish_for("made own");
                    }
                    let _ = reply.send(self.board.read().map(|b| b.generated_at).unwrap_or(0));
                }
                Ok(Msg::Talking(key, yours)) => {
                    // Only a helper's is kept: what anyone says to one of your own sessions
                    // changes nothing, and an agent's message ends it for any.
                    let helper = self.board.read().is_ok_and(|b| b.session(&key).is_some_and(|(_, s)| s.helper.is_some()));
                    if (helper || !yours) && self.seen.set_talking(&key, yours) {
                        self.save_state();
                        self.publish_for("talking");
                    }
                }
                Ok(Msg::SetupDone) => {
                    if !self.seen.setup_done {
                        self.seen.setup_done = true;
                        self.save_state();
                    }
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
            if !pending.is_empty() || !dropped.is_empty() {
                let quiet = last_event.map(|t| t.elapsed() >= Duration::from_millis(150)).unwrap_or(true);
                let overdue = first_event.map(|t| t.elapsed() >= Duration::from_millis(600)).unwrap_or(false);
                if quiet || overdue {
                    let mut batch = std::mem::take(&mut pending);
                    first_event = None;
                    let t = Instant::now();
                    batch.extend(self.on_dropped(std::mem::take(&mut dropped)));
                    if !batch.is_empty() {
                        let n = batch.len();
                        let busiest = perf::busiest(&batch);
                        self.apply_fs(batch);
                        self.perf.fs(n, t.elapsed(), busiest);
                    }
                }
            }
            if self.landed_at.is_some_and(|t| t.elapsed() >= LANDED_QUIET) {
                self.landed_at = None;
                self.rescan_repos();
            }
            for path in self.git_queue.due(Instant::now()) {
                self.send_git(path);
            }
            for path in self.drops.due(Instant::now()) {
                self.queue_git(path);
            }
            // What the watcher brought goes out before the readers look about: at a high load
            // their glance and tick take a while, and a session's new line would wait for them.
            if throttle::publish_now(self.dirty, self.dirty_git, self.last_publish.map(|t| t.elapsed()), PUBLISH_EVERY) {
                self.publish();
            }
            if last_databases.elapsed() >= DATABASES_EVERY {
                last_databases = Instant::now();
                self.glance_readers();
            }
            if last_tick.elapsed() >= Duration::from_secs(10) {
                last_tick = Instant::now();
                self.tick();
            }
            if last_heartbeat.elapsed() >= Duration::from_secs(30) {
                // Codex turns go stale with time alone; republishing picks that up.
                last_heartbeat = Instant::now();
                self.mark_dirty("heartbeat");
            }
            if throttle::publish_now(self.dirty, self.dirty_git, self.last_publish.map(|t| t.elapsed()), PUBLISH_EVERY) {
                self.publish();
            }
        }
    }

    fn load_config(&mut self) {
        self.config_settled = false;
        let text = config::read_text();
        self.config_text = text.as_ref().ok().cloned().flatten();
        match text.and_then(|t| t.map(|t| config::load_text(&t)).transpose()) {
            Ok(Some(cfg)) => {
                // A sound that isn't there reads fine and would only be silent, and a queue that
                // names what isn't a kind is gone by without it, so they're said here, as Settings
                // says them when it saves.
                self.set_trouble(Trouble::Config, config::check_sounds(&cfg.sounds).and_then(|()| config::check_queue(&cfg.queue)).err());
                self.cfg = cfg;
            }
            Ok(None) => {
                let cfg = first_run_config();
                match config::write(&cfg) {
                    Ok(text) => self.config_text = Some(text),
                    Err(e) => self.set_trouble(Trouble::Config, Some(format!("Couldn't save config.toml: {e}"))),
                }
                self.cfg = cfg;
            }
            Err(e) => {
                self.set_trouble(Trouble::Config, Some(format!("{e:#}. Codealong is using the last good settings.")));
                if self.cfg.accounts.is_empty() {
                    self.cfg.accounts = detect::first_run_accounts();
                }
            }
        }
        // What the app's commands act on, so they go by what the board shows (`config::current`).
        // A one-pass engine (`--snapshot`, the command without the app) leaves the file to them,
        // so a mistake in it is said, not covered over.
        if !self.passing && !cfg!(test) {
            config::running_on(&self.cfg);
            crate::install::follow_live_replies(self.cfg.sessions.live_replies);
        }
        self.previews.chrome_profiles = self.cfg.projects.iter().filter_map(|p| Some((p.name.clone(), p.chrome_profile.clone()?))).collect();
        perf::set_enabled(self.cfg.debug.perf_log);
        search::configure(search::Scope { accounts: self.cfg.accounts.clone(), days: self.cfg.search.days });
    }

    /// Every folder config.toml lists is known from then on, so an account you take out isn't
    /// suggested again, whether Settings or an editor took it out. True when one was new.
    fn remember_accounts(&mut self) -> bool {
        let keys: Vec<String> = self.cfg.accounts.iter().map(|a| detect::folder_key(&a.home)).collect();
        self.seen.know_accounts(keys)
    }

    fn load_previews(&mut self) {
        if let Some(store) = self.read_or_warn(Trouble::Previews, previews::load()) {
            self.previews.set = store.previews;
        }
    }

    fn load_reusable(&mut self) {
        if let Some(store) = self.read_or_warn(Trouble::Reusable, reusable::load()) {
            self.reusable = store.by_path();
        }
    }

    /// Notes what asked for a publish, for the perf log's word on one that changed nothing.
    fn asked(&mut self, what: impl FnOnce() -> String) {
        if perf::enabled() {
            self.asked_by.insert(what());
        }
    }

    /// Has the board composed again, saying why: every mark names its cause, so a publish that
    /// changed nothing says in the perf log what asked for it (`publish`).
    fn mark_dirty(&mut self, why: &'static str) {
        self.dirty = true;
        self.asked(|| why.into());
    }

    /// `mark_dirty` when `changed`.
    fn dirty_if(&mut self, changed: bool, why: &'static str) {
        if changed {
            self.mark_dirty(why);
        }
    }

    /// Publishes now rather than on the next tick (a window waits for it, or shows it already),
    /// saying why as `mark_dirty` does.
    fn publish_for(&mut self, why: &'static str) {
        self.asked(|| why.into());
        self.publish();
    }

    /// Takes in what GitHub was asked in the background (ghpoll.rs).
    fn on_github(&mut self, event: ghpoll::Event) {
        if self.github.on_event(event, &self.seeds, Now::real()) {
            self.mark_dirty("github");
        }
    }

    fn tick(&mut self) {
        let t = Instant::now();
        self.tick_readers();
        // What processes came or went is looked at once they're listed (`on_processes`).
        self.list_processes();
        self.poll_readers();
        // A PR a session just made in a repo whose PRs aren't read is asked about now, not at the
        // next poll, so it shows with its session at once.
        let links = self.pr_links();
        let worked = self.board.read().map(|b| board::worked_prs(&links, &b.lanes)).unwrap_or_default();
        if self.github.set_marked(worked) {
            self.github.poll(&self.seeds, false, Now::real());
        }
        self.github.tick(&self.seeds, Now::real());
        // A worktree with commits on no remote can go once GitHub says a merged PR ends at its HEAD.
        self.github.ask_heads(&board::heads_on_no_remote(&self.seeds, &self.git, &self.heads_now()), Now::real());
        let every = if self.usage_backoff { USAGE_BACKOFF } else { USAGE_EVERY };
        if self.usage_last.map(|t| t.elapsed() >= every).unwrap_or(true) {
            self.fetch_usage();
        }
        self.read_team_files();
        self.read_training();
        self.read_resets();
        self.read_sign_ins();
        self.read_mcp();
        self.read_screens();
        if self.load.on_tick(now_ms(), crate::os::system::system) {
            self.mark_dirty("load");
        }
        // Sessions that started in tmux while the screen was locked open in your terminal once
        // it isn't, those started while Codealong was closed too (`launch::TabLooks`).
        if crate::os::is_macos() && self.tab_looks.on_tick(launch::screen_locked(), launch::take_marked()) {
            let tx = self.tx.clone();
            let (terminal, new_terminal, lanes) = self.board.read().map(|b| (b.apps.terminal_id.clone(), b.apps.new_terminal, b.lanes.clone())).unwrap_or_default();
            throttle::background("codealong-show-unlocked", move || launch::show_unlocked(&terminal, new_terminal, &lanes), move |looked| {
                let looked = looked.unwrap_or_default();
                let why = (!looked.failed.is_empty()).then(|| format!("{} It still runs in tmux; Show in opens it.", looked.failed.join("; ")));
                let _ = tx.send(Msg::ShownUnlocked(why, looked.deferred));
            });
        }
        // A new account's folder appears when it first signs in; start reading it.
        if self.missing_homes.iter().any(|p| p.exists()) {
            self.reload();
        }
        // Reads still out after a while are said on the board (held_warning).
        if !self.reads.out_for(HELD_WARN_AFTER).is_empty() {
            self.mark_dirty("reads held");
        }
        // A run whose command stopped waiting on it is no longer shown on its session, nor a run
        // of yours whose session was told what it printed, or that couldn't be.
        if crate::terminal::forget_unwaited(now_ms()) | crate::terminal::forget_told(now_ms()) {
            self.mark_dirty("terminal runs");
        }
        perf::took(|| "engine tick".into(), t.elapsed());
    }

    /// A PR the board doesn't list, matched to these lanes' worktrees and sessions.
    fn pr_card(&self, lanes: &[Lane], slug: &str, raw: &PrRaw, login: Option<&str>) -> PrItem {
        let links = self.pr_links();
        board::pr_card(&self.seeds, lanes, slug, self.github.repo_names(), raw, self.github.login().or(login), &links)
    }

    /// What pr-links.json and the sessions' records say of PRs, for the board, a moved
    /// conversation's records going on in its copy (`prlinks::carry_moves`).
    fn pr_links(&self) -> board::PrLinks {
        let origins = origins::current();
        let moves = origins.moves();
        let (mut worked, unlinked, made) = prlinks::resolve(&prlinks::carry_moves(&self.pr_marks, &moves, prlinks::add_mark), &self.pr_store);
        // By the board before this one, as sends and schedules follow a session. With none yet,
        // every copy is taken to be there, or a watch would seem to begin with the next board.
        let picked = {
            let board = self.board.read();
            let exists = |k: &str| board.as_ref().map_or(true, |b| b.lanes.is_empty() || b.session(k).is_some());
            prlinks::picks_after_moves(&self.pr_store, |k| origins.moved_to(k, exists), &mut worked)
        };
        board::PrLinks {
            worked,
            unlinked,
            made,
            pushed: prlinks::carry_moves(&self.pr_pushes, &moves, prlinks::add_push),
            picked,
            linked_at: self.pr_store.links.iter().filter(|l| l.linked).fold(HashMap::new(), |mut at, l| {
                let t = at.entry(prlinks::pr_key(&l.pr)).or_insert(l.at);
                *t = (*t).max(l.at);
                at
            }),
            nudged: self.pr_store.nudges.iter().cloned().collect(),
            not_nudged: self.pr_store.nudges_off.iter().cloned().collect(),
            watched: self.pr_store.watched.iter().cloned().collect(),
            nudge_by_default: self.cfg.pull_requests.nudge_by_default,
            nudge_by_default_in: self.team_files.settings(&self.cfg.repos).nudges_by_default(),
            agent_posts: self.reviews.prs.values().flat_map(|t| t.rounds.iter().filter_map(|r| r.comment.clone())).collect(),
        }
    }

    /// schedules.json and schedule-runs.json as they are now. One that doesn't read leaves what
    /// was read before, and says why.
    fn load_schedules(&mut self) {
        let file = crate::schedules::load().map(|f| self.schedules = f);
        let runs = crate::schedules::load_runs().map(|r| self.schedule_runs = r);
        self.schedules_problem = file.and(runs).err().map(crate::util::words);
    }

    /// reviews.json as autoreview.rs last saved it, for the PRs' cards. One that doesn't read
    /// shows no reviews; autoreview.rs says why (store.rs `load_whole`).
    fn load_reviews(&mut self) {
        self.reviews = crate::store::read_or_default(&autoreview::path());
    }

    fn load_pr_links(&mut self) {
        if let Some(store) = self.read_or_warn(Trouble::PrLinks, prlinks::load()) {
            self.pr_store = store;
        }
    }

    /// Writes down where headless runs came from once they've outlived their process
    /// (`origins::ended_runs`), as `with_run_origins` found it while they ran. The board shows
    /// those origins already, so it isn't composed again.
    fn keep_run_origins(&self, board: &Board) {
        // Let go before the file's written: another thread may be composing a board.
        let runs = {
            let found = self.run_origins.lock().unwrap_or_else(|e| e.into_inner());
            if found.is_empty() {
                return;
            }
            origins::ended_runs(board, &found, &origins::current())
        };
        if !runs.is_empty() {
            origins::record_runs(&runs);
        }
    }

    /// Notes where sessions came from that starts from other sessions were waiting for.
    fn link_origins(&self, board: &Board) -> bool {
        let store = origins::current();
        if store.pending.is_empty() {
            return false;
        }
        let lanes = todos::LaneIndex::of(&board.lanes);
        origins::resolve_pending(&origins::candidates(board, &store.pending), &lanes, now_ms())
    }

    fn compose(&self) -> Board {
        let now = now_ms();
        let apps = AppNames {
            terminal: launch::terminal_name(&self.cfg.apps.terminal),
            terminal_id: self.cfg.apps.terminal.clone(),
            editor: editors::name(&self.cfg.apps.editor),
            editor_id: self.cfg.apps.editor.clone(),
            editors: editors::installed(),
            diff_editor: editors::diff_editor(&self.cfg.apps.editor),
            new_terminal: self.cfg.apps.new_terminal,
            run_in: self.cfg.apps.run_in,
        };
        let order: Vec<String> = self.cfg.projects.iter().map(|p| p.name.clone()).collect();
        let notices = self.notices();
        let usage = self.compose_usage(now);
        let pr_links = self.pr_links();
        let repo_settings = self.team_files.settings(&self.cfg.repos);
        let sides = crate::side::glances();
        let listenings = crate::listen::glances();
        let merged = self.github.merged_heads();
        let at_merged: HashSet<PathBuf> = self.heads_now().into_iter().filter(|(_, head)| merged.contains(head)).map(|(path, _)| path).collect();
        let mut board = board::compose(board::Inputs {
            now,
            apps: &apps,
            seeds: &self.seeds,
            sessions: self.sessions(now, &usage, &sides, &listenings),
            git: &self.git,
            prs: self.github.prs(),
            requested: self.github.requested(),
            made: self.github.made(),
            project_order: &order,
            notices,
            loading: self.loading,
            github_at: self.github.at(),
            visitors: &self.visitors.by_lane,
            pinned: &self.seen.pinned,
            github_login: self.github.login(),
            pr_links: &pr_links,
            previews: &self.previews,
            reusable: &self.reusable,
            review_snoozes: &self.seen.review_snoozes,
            auto_reviews: (&self.reviews, &self.cfg.reviews, &repo_settings),
            machines: &self.machines,
            session_settings: &self.cfg.sessions,
            at_merged: &at_merged,
        });
        // Starts whose sessions aren't here yet: a line under the session that asked for each, and
        // a notice for one in a terminal that never made its session, with what its tab shows.
        let word_of = |account: &str| self.harness_word(account);
        let waiting = origins::waiting_for(&board, now);
        let stuck = origins::stuck_notices(&waiting, &board, word_of, |at| self.screens.screen(at).map(String::from), now);
        board.notices.extend(stuck);
        board.starting = crate::starting::by_session(&waiting, &board, &self.screens, word_of, now);
        board.usage = usage;
        let signed_out = board::signed_out_notices(&board);
        board.notices.extend(signed_out);
        board.training = self.training.list(&self.cfg.accounts);
        board.resets = self.resets.list(&self.cfg.accounts);
        board.sign_ins = self.sign_ins.list(&self.cfg);
        board.mcp = self.mcp.list(&self.cfg.accounts);
        board.sides = sides;
        adopt_terminals(&board.lanes, |pid| crate::procs::tty(pid).map(|t| t.to_string_lossy().into_owned()));
        board.terminals = crate::terminals::glances();
        board.listenings = listenings;
        board.meeting = crate::meetings::offered();
        board.meeting_soon = crate::meetings::soon();
        board.load = self.load.now();
        board.github_polled = self.github.polled();
        board.github_current = self.github.current();
        board.when_limited = self.cfg.limits.when_hit;
        board.nudge_about = self.cfg.pull_requests.nudge_about;
        board.pr_stopped = crate::nudges::stopped_primaries(&board, &self.running_elsewhere, self.cfg.sessions.resume_for_pr_news, |s| crate::send::reaches_stopped(&self.cfg, s));
        board.reviews = self.cfg.reviews.clone();
        board.repos = self.cfg.repos.clone();
        board.repo_settings = repo_settings;
        for pr in &mut board.prs {
            pr.review_first = board.repo_settings.pull_requests(&self.cfg.pull_requests, pr).reviews_first;
        }
        board.team_files = self.team_files.files();
        for f in &mut board.team_files {
            let Some(team) = &f.settings else { continue };
            let mine = board.repo_settings.mine_of(&f.repo);
            let reviews = config::reviews_for(&self.cfg.reviews, Some(team), mine);
            let prs = config::pull_requests_for(&self.cfg.pull_requests, Some(team), mine);
            f.offers = config::suggestions_left(&team.suggests, &reviews, &prs);
        }

        board.pinned_worktrees = self.seen.pinned_worktrees.clone();
        board.pinned_prs = self.seen.pinned_prs.clone();
        board.pinned_repos = self.seen.pinned_repos.clone();
        board.theme = self.cfg.appearance.theme;
        board.zoom = self.cfg.appearance.zoom;
        board.glass = self.cfg.appearance.glass;
        board.show_keys = self.cfg.appearance.show_keys;
        board.quick_shortcut = self.cfg.shortcuts.quick_panel.clone();
        board.chat_shortcut = self.cfg.shortcuts.chat.clone();
        board.waiting_shortcut = self.cfg.shortcuts.waiting.clone();
        board.improve_shortcut = self.cfg.shortcuts.improve.clone();
        board.main_window_shortcut = self.cfg.shortcuts.main_window.clone();
        let in_use = self.cfg.shortcuts_in_use();
        board.talk_shortcut = in_use.talk;
        board.talk_orchestrator_shortcut = in_use.talk_orchestrator;
        board.listen_shortcut = in_use.listen;
        // Codealong's own source's worktrees, and where a session that improves it goes.
        let sources: HashSet<PathBuf> = self.codealong_sources.iter().map(|(root, _)| root.clone()).collect();
        crate::improve::mark(&mut board.lanes, &self.lane_checkout, &sources);
        board.improve = crate::improve::Improve { installed: crate::updates::enabled(), ..crate::improve::target(&board.lanes, &self.codealong_sources) };
        board.decisions = self.cfg.decisions.clone();
        board.experimental = self.cfg.experimental.clone();
        board.queue = self.cfg.queue.settled();
        // While `[queue]` leaves reviews out, a PR waiting on your review is no cause for the
        // mark's color, as it's none for the bar's pills or the notch's count.
        if !board.queue.waits("reviews") {
            board.logo_status = board::logo_status(&board.lanes, &[]);
        }
        board.devices = self.cfg.devices.clone();
        board.project_rules = self.cfg.projects.clone();
        for lane in &mut board.lanes {
            lane.repo_slug_now = lane.repo_slug.as_deref().and_then(|slug| self.github.renamed(slug).cloned());
        }
        board.perf_log = perf::enabled();
        board.todos = todos::awake_at(self.todos.todos.clone(), now);
        board.today = crate::util::day_key(crate::util::today_local());
        board.todo_clock = todos::todo_clock(&self.todos.todos, now);
        board.todos_unreadable = self.todos_unreadable;
        board.labels = self.todos.labels.clone();
        // Once the board has the PRs and the todos a session's work is known by.
        board::settle_finished(&mut board.lanes, &board.prs, &board.todos);
        if self.cfg.decisions.todos {
            board.todo_hints = self.jev_todos.hints(&board, &self.todos.todos);
        }
        board.machines = self.machines.clone();
        board.schedules = crate::schedules::items(&self.schedules, &self.schedule_runs, &board, &self.cfg, now, &crate::schedules::rule::Local);
        board.schedules.iter_mut().for_each(|i| i.emblem = self.emblems.of_schedule(&i.schedule.id, self.cfg.emblems.kind));
        crate::schedules::orchestrate::mark(&mut board.lanes, &board.schedules);
        board.shared_schedules = crate::schedules::machines::outgoing(&self.schedules, &self.schedule_runs, &board, &self.cfg, &crate::schedules::rule::Local);
        board.shared_schedules.list.iter_mut().for_each(|x| x.emblem = self.emblems.shared(&x.id));
        board.schedules_problem = self.schedules_problem.clone();
        // Once the board has its worktrees, its todos, the day and `[queue]`.
        board.todo_queue = crate::barfeed::todo_queue(&board);
        board
    }

    /// The usage of each account that isn't turned off: one turned off has no place in the bar.
    fn compose_usage(&self, now: i64) -> Vec<Usage> {
        self.cfg
            .accounts
            .iter()
            .filter(|a| !a.disabled)
            .map(|a| {
                let adapter = harness::of(a);
                if let Some(why) = adapter.and_then(|h| h.no_limits(a)) {
                    return usage::none(a, why);
                }
                // What comes with the sessions read anyway, else the last background read.
                let read = adapter.and_then(|h| self.readers.iter().find(|r| r.harness() == h.harness())).and_then(|r| r.usage(a, now));
                read.unwrap_or_else(|| self.usage.get(&a.name).cloned().unwrap_or_else(|| usage::loading(a)))
            })
            .collect()
    }

    fn publish(&mut self) {
        let t = Instant::now();
        self.store_warnings = crate::store::problems(&config::support_dir());
        let mut board = self.compose();
        // Rare, so the board is composed again rather than patched. Before the todos' starts take
        // their sessions, so that a helper whose start an app made is known for one and isn't
        // taken (`todos::SessionRef::helper`).
        if self.link_origins(&board) {
            board = self.compose();
        }
        if self.link_pending(&board) {
            board.todos = todos::awake_at(self.todos.todos.clone(), board.generated_at);
            board.todo_queue = crate::barfeed::todo_queue(&board);
        }
        self.keep_run_origins(&board);
        // Where each session came from, for the ledger to keep (began.rs).
        if self.cfg.experimental.analytics && crate::began::due() {
            crate::began::note(crate::began::noted_on(&board, &crate::schedules::own_sessions(&self.schedules, &self.schedule_runs, &board)));
        }
        // The board already leaves them out.
        self.forget_stale_suggestions(&board);
        self.forget_stale_tags(&board);
        self.forget_spent_agent_seen(&board);
        self.forget_spent_aside(&board);
        self.wake_called_back(&board);
        self.jev_look(&board);
        // Rare too: a schedule's emblem coming, or a session of it showing.
        if self.look_emblems(&board) {
            board = self.compose();
        }
        let composed = t.elapsed();
        self.dirty = false;
        self.dirty_git = false;
        let since_last = self.last_publish.map(|t| t.elapsed());
        self.last_publish = Some(Instant::now());
        let hash = board_hash(&mut board);
        let hashed = t.elapsed() - composed;
        let asked_by = std::mem::take(&mut self.asked_by);
        if hash == self.last_hash {
            if perf::enabled() {
                let by = if asked_by.is_empty() { "something unnamed".to_string() } else { asked_by.into_iter().collect::<Vec<_>>().join(", ") };
                perf::log(&format!("publish skipped, nothing changed: compose {} ms, hash {} ms; asked for by {by}", composed.as_millis(), hashed.as_millis()));
            }
            return;
        }
        self.last_hash = hash;
        // Numbered after the hash, which leaves the number out: what the windows are sent of it
        // says which board it changes (windowfeed.rs).
        self.rev += 1;
        board.rev = self.rev;
        // The board `get_board` reads is this one before any window hears of it: a window that
        // reads the board as this push comes gets this board or a later one, never the one
        // before it, which nothing would correct while the board stays the same. It goes out
        // from the slot, under a read lock: only this thread writes there, and readers don't
        // wait on each other.
        match self.board.write() {
            Ok(mut slot) => *slot = board,
            Err(poisoned) => *poisoned.into_inner() = board,
        }
        let board = self.board.read().unwrap_or_else(|poisoned| poisoned.into_inner());
        let t = Instant::now();
        (self.emit)(&board);
        let emitted = t.elapsed();
        // A whole pass: read, composed and out (`run_guarded`).
        self.passed = true;
        if perf::enabled() {
            let line = self.perf.publish_line(&board, composed, hashed, emitted, since_last, self.git_queue.running());
            perf::log(&line);
        }
    }
}

/// A hash of everything the UI shows except the timestamp, so identical boards aren't resent.
/// The board is hashed as it's written out, with its timestamp set aside meanwhile: it's some
/// 2 MB, and neither a copy of it nor its text is made for this.
pub(crate) fn board_hash(board: &mut Board) -> u64 {
    use std::hash::Hasher;
    struct Hashed(std::collections::hash_map::DefaultHasher);
    impl std::io::Write for Hashed {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.write(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    // Buffered, so the hasher is handed a few kilobytes at a time, not each comma and quote.
    let mut out = std::io::BufWriter::new(Hashed(std::collections::hash_map::DefaultHasher::new()));
    // Neither what's read of the repos' team files, which limits have lifted long enough to be
    // carried on, nor which PRs' unsaid fixes have settled long enough to ask their reviewer about
    // is sent, but what acts on the board waits on them; the last two come with time alone, which
    // the heartbeat's board then goes out for. Nor what this machine's reviews of each PR come to,
    // which goes to your other machines in its feed (peerfeed.rs).
    let lifted = crate::reconnect::lifted(board, board.generated_at);
    let settled: Vec<String> = crate::rerequest::settled(board, board.generated_at).into_iter().map(String::from).collect();
    let shared: Vec<&codealong_peer::pr::SharedReview> = board.prs.iter().map(|p| &p.shared_review).collect();
    let at = std::mem::take(&mut board.generated_at);
    let written = serde_json::to_writer(&mut out, &*board).and_then(|()| serde_json::to_writer(&mut out, &(board.repo_settings.readiness(), lifted, settled, shared)));
    board.generated_at = at;
    // A board that can't be written out hashes as nothing at all, as it did.
    match (written, out.into_inner()) {
        (Ok(()), Ok(hashed)) => hashed.0.finish(),
        _ => std::collections::hash_map::DefaultHasher::new().finish(),
    }
}

/// How often the harness databases are looked at for changes (`glance_readers`).
const DATABASES_EVERY: Duration = Duration::from_millis(1500);

/// How long the tick's `ps` may take. Off the engine's thread, so a slow one only puts off the
/// readers' look at the processes; seen at 3 to 7 s at a load of 400 to 700.
const PS_WAIT: Duration = Duration::from_secs(20);

/// First run: the accounts in use or signed in, a project per GitHub owner your agents worked
/// in, the folders their repos are in to search, and an editor that's installed. The setup
/// wizard then has you confirm them (setup.rs).
fn first_run_config() -> Config {
    let accounts = detect::first_run_accounts();
    let mut per_repo = setup::worked_repos(&accounts);
    let mut defaults = Config::default();
    let scan = setup::scan_folders(per_repo.keys().map(PathBuf::as_path), &crate::util::home());
    if !scan.is_empty() {
        defaults.scan = scan.iter().map(|p| crate::util::tilde(p)).collect();
    }
    if let Some(editor) = setup::first_editor() {
        defaults.apps.editor = editor;
    }
    per_repo.retain(|_, n| *n >= 2);
    let mut loaded = Held::new("checkout", repos::load_checkout, || {}).read(per_repo.keys().cloned(), HELD_WAIT);
    let mut owners: BTreeMap<String, usize> = BTreeMap::new();
    for (root, n) in per_repo {
        if let Some((owner, _)) = loaded.remove(&root).flatten().and_then(|c| c.github) {
            *owners.entry(owner).or_default() += n;
        }
    }
    Config { accounts, projects: config::suggest_projects(&owners, github::display_name), ..defaults }
}

/// A terminal of Codealong's that a live session's own process runs in is that session's
/// (launch/codealong.rs): one opened to start or resume a session belongs to none until its
/// process turns up on the board, found by the terminal device it has (`tty_of`), and one a
/// session lives in goes to whichever session runs there now, as a new conversation in the same
/// process (`/clear`, `/resume` in Claude Code) is a new session. A terminal beside a session (its
/// drawer's) stays that session's, whatever you run in it. Asked only while there's such a
/// terminal, so most boards cost nothing.
fn adopt_terminals(lanes: &[crate::model::Lane], tty_of: impl Fn(u32) -> Option<String>) {
    let open: Vec<crate::terminals::Glance> = crate::terminals::list().into_iter().filter(|g| g.running && (g.session.is_empty() || g.hosts) && !g.tty.is_empty()).collect();
    if open.is_empty() {
        return;
    }
    for s in lanes.iter().flat_map(|l| &l.sessions).filter(|s| s.live && !s.archived) {
        let Some(tty) = s.pid.and_then(&tty_of) else { continue };
        if let Some(g) = open.iter().find(|g| g.tty == tty && g.session != s.key) {
            crate::terminals::adopt(&g.id, &s.key);
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn worktrees_carry_what_github_calls_a_renamed_repo_now() {
        use super::*;

        let (tx, _rx) = channel();
        let mut engine = Engine::new(Box::new(|_| {}), tx, Arc::new(RwLock::new(Board::default())));
        let seed = |path: &str, slug: &str| LaneSeed { repo_slug: Some(slug.into()), origin: Origin::Main, ..folder_seed(PathBuf::from(path), "P", "repo", false) };
        engine.seeds = vec![seed("/x/spark", "SparkHQ/spark"), seed("/x/spur", "spurhq/spur")];
        engine.on_github(ghpoll::Event::RepoNames(vec![("sparkhq/spark".into(), Some("sprucehq/spark".into())), ("spurhq/spur".into(), Some("spurhq/spur".into()))]));
        assert!(engine.dirty, "a rename changes the board");
        let board = engine.compose();
        let now = |path: &str| board.lanes.iter().find(|l| l.path == path).map(|l| l.repo_slug_now.clone());
        assert_eq!(now("/x/spark"), Some(Some("sprucehq/spark".to_string())));
        assert_eq!(now("/x/spur"), Some(None), "a repo that kept its name says nothing more");

        engine.dirty = false;
        engine.on_github(ghpoll::Event::RepoNames(vec![("spurhq/spur".into(), Some("spurhq/spur".into()))]));
        assert!(!engine.dirty, "a repo that kept its name changes nothing");
    }

    /// The first scan reads config.toml before the watcher starts, and can take minutes, so an
    /// edit made meanwhile is read once the watcher is up. A scan reads and writes under the
    /// home folder (config.toml, state.json), so it runs in a copy of this test binary with an
    /// empty home of its own.
    #[test]
    fn a_config_edit_during_the_first_scan_is_read() {
        use super::*;
        use std::sync::atomic::{AtomicBool, Ordering};

        let Some(own) = std::env::var_os("CODEALONG_TEST_HOME") else {
            let home = std::env::temp_dir().join(format!("codealong-first-scan-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&home);
            std::fs::create_dir_all(&home).unwrap();
            let run = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["engine::tests::a_config_edit_during_the_first_scan_is_read", "--exact", "--nocapture"])
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

        // No accounts and nothing to scan, so the scan stays in this home.
        let off = Config { scan: vec![], ..Config::default() };
        config::write(&off).unwrap();
        let on = config::render(&Config { devices: crate::config::DevicesCfg { enabled: true, ..Default::default() }, ..off });
        // The scan's first board goes out before the watcher starts: devices are turned on then.
        let edited = AtomicBool::new(false);
        let emit = move |_: &Board| {
            if !edited.swap(true, Ordering::SeqCst) {
                std::fs::write(config::config_path(), &on).unwrap();
            }
        };
        let (tx, _rx) = channel();
        let mut engine = Engine::new(Box::new(emit), tx, Arc::new(RwLock::new(Board::default())));
        engine.full_scan();
        assert!(engine.cfg.devices.enabled);
        assert!(engine.compose().devices.enabled);
        std::fs::write(home().join("ran"), "").unwrap();
    }

    #[test]
    fn boards_that_differ_only_in_their_time_hash_alike() {
        use super::*;
        let board = |at: i64, loading: bool| Board { generated_at: at, loading, quick_shortcut: "Alt+Cmd+/".into(), ..Board::default() };
        let (mut first, mut later, mut other) = (board(1_000, false), board(2_000, false), board(1_000, true));
        assert_eq!(board_hash(&mut first), board_hash(&mut later));
        assert_ne!(board_hash(&mut first), board_hash(&mut other), "a real difference");
        later.notices.push(Notice { key: "github".into(), title: "GitHub didn't answer".into(), text: "GitHub: couldn't load PRs".into(), ..Default::default() });
        assert_ne!(board_hash(&mut first), board_hash(&mut later));
        // The board goes out with the time it was made, and hashes the same each time.
        assert_eq!((first.generated_at, later.generated_at), (1_000, 2_000));
        assert_eq!(board_hash(&mut first), board_hash(&mut first.clone()));
    }

    /// A repo's team file read without a file changes nothing the windows get, but a review that
    /// waited on it goes on only once the board with it is published (autoreview.rs reads the
    /// published board), so it counts as a change.
    #[test]
    fn a_team_file_read_without_a_file_is_a_change_to_publish() {
        use super::*;
        let mut before = Board { repo_settings: crate::teamfile::RepoSettings { unread: true, ..Default::default() }, ..Board::default() };
        let mut started = Board { repo_settings: crate::teamfile::RepoSettings { here: ["o/r".to_string()].into(), ..Default::default() }, ..Board::default() };
        let mut read = started.clone();
        read.repo_settings.read.insert(("o/r".into(), "main".into()));
        assert_ne!(board_hash(&mut before), board_hash(&mut started), "the first read was asked for");
        assert_ne!(board_hash(&mut started), board_hash(&mut read), "a branch was read");
        assert_eq!(board_hash(&mut read), board_hash(&mut read.clone()));
    }

    /// A limit that has lifted long enough to be carried on changes nothing the windows get, and
    /// comes with time alone, but a board saying so has to go out for reconnect.rs to hear it.
    #[test]
    fn a_limit_lifted_long_enough_to_carry_on_is_a_change_to_publish() {
        use super::*;
        let limit = SessionLimit { at: 1_000, resets_at: Some(10_000), continues: true, ..SessionLimit::default() };
        let session = Session { live: true, status: Status::Error, limit: Some(limit), ..crate::fixtures::session("claude:claude:a") };
        let board = |at: i64| Board { generated_at: at, lanes: vec![crate::fixtures::lane("/work", vec![session.clone()])], ..Board::default() };
        let (mut before, mut lifted) = (board(10_001), board(10_001 + 60 * 60_000));
        let mut later = board(10_002 + 60 * 60_000);
        assert_ne!(board_hash(&mut before), board_hash(&mut lifted));
        assert_eq!(board_hash(&mut lifted), board_hash(&mut later), "only once");
    }

    /// What the engine's thread spends on this Mac's real data: composing the board, hashing it,
    /// each reader's tick and glance, and a big batch from the file watcher. `CODEALONG_BENCH_FS` names folders whose files go in
    /// that batch (as many as there are, up to 30,000), as a build or an install writes them.
    /// `CODEALONG_BENCH_RUNS` says how many times each is timed (20), more for a profile.
    ///
    ///   cd src-tauri && CODEALONG_BENCH_FS=~/projects/app/dist cargo test --lib bench_engine_on_real_data -- --ignored --nocapture
    #[test]
    #[ignore = "reads this Mac's sessions and repos; run by hand"]
    fn bench_engine_on_real_data() {
        use super::*;
        let mut e = snapshot_engine(false, false);
        let ms = |t: Instant| t.elapsed().as_secs_f64() * 1000.0;
        let runs: usize = std::env::var("CODEALONG_BENCH_RUNS").ok().and_then(|n| n.parse().ok()).unwrap_or(20);
        let (mut compose, mut sessions, mut hash) = (0.0, 0.0, 0.0);
        for _ in 0..runs {
            let t = Instant::now();
            let _ = e.build_sessions(now_ms());
            sessions += ms(t);
            let t = Instant::now();
            let mut board = e.compose();
            compose += ms(t);
            let t = Instant::now();
            let _ = board_hash(&mut board);
            hash += ms(t);
        }
        let n = runs as f64;
        eprintln!("compose {:.1} ms (of which building sessions {:.1} ms), hash {:.1} ms, over {runs} runs", compose / n, sessions / n, hash / n);
        // Each reader's part of the tick, its look at the processes (listed once, as the engine
        // lists them, off its thread), its glance, and its reading every session afresh, as a
        // glance does for an account whose database changed.
        let t = Instant::now();
        let listed = procs::list(PS_WAIT);
        eprintln!("ps {:.1} ms, {} processes (off the engine's thread)", ms(t), listed.lines.len());
        e.processes = Some(Arc::new(listed));
        for r in &mut e.readers {
            let accounts = harness::accounts_of(&e.cfg, r.harness());
            let (mut tick, mut processes, mut glance) = (0.0, 0.0, 0.0);
            for _ in 0..runs {
                let look = Engine::look(&e.seen, &e.processes);
                let t = Instant::now();
                r.tick(&accounts, &look);
                tick += ms(t);
                let t = Instant::now();
                r.processes(&accounts, &look);
                processes += ms(t);
                let t = Instant::now();
                r.glance(&accounts, &look);
                glance += ms(t);
            }
            let t = Instant::now();
            r.scan(&accounts, &Engine::look(&e.seen, &e.processes));
            eprintln!("{:<12} tick {:.1} ms, processes {:.1} ms, glance {:.1} ms, reading every session {:.1} ms", r.harness().config(), tick / n, processes / n, glance / n, ms(t));
        }
        let t = Instant::now();
        e.watch();
        eprintln!("watching every folder {:.1} ms", ms(t));
        let Ok(dirs) = std::env::var("CODEALONG_BENCH_FS") else { return };
        let mut paths = Vec::new();
        for dir in std::env::split_paths(&dirs) {
            let mut stack = vec![dir];
            while let Some(d) = stack.pop() {
                let Ok(entries) = std::fs::read_dir(&d) else { continue };
                for entry in entries.flatten() {
                    let p = entry.path();
                    if entry.file_type().is_ok_and(|t| t.is_dir()) {
                        stack.push(p.clone());
                    }
                    if !is_noise(&p) && paths.len() < 30_000 {
                        paths.push(p);
                    }
                }
            }
        }
        let batches = runs.max(1);
        let t = Instant::now();
        for _ in 0..batches {
            e.apply_fs(paths.clone());
        }
        eprintln!("fs batch of {} paths {:.1} ms, over {batches} runs", paths.len(), ms(t) / batches as f64);
    }

    /// A terminal opened to start a session becomes that session's own once its process runs
    /// there, found by the device; a session elsewhere takes none.
    #[test]
    #[cfg_attr(windows, ignore = "Codealong's own terminals aren't on Windows yet")]
    fn a_session_running_in_a_terminal_of_codealongs_makes_it_its_own() {
        use super::*;
        use crate::terminals;
        let g = terminals::open(terminals::Open { folder: std::env::temp_dir(), command: Some("sleep 5".into()), size: Some(terminals::DEFAULT_SIZE), ..Default::default() }).unwrap();
        let here = crate::model::Session { live: true, pid: Some(4242), ..crate::fixtures::session("test:adopts:here") };
        let elsewhere = crate::model::Session { live: true, pid: Some(4343), ..crate::fixtures::session("test:adopts:elsewhere") };
        let lanes = vec![crate::fixtures::lane("/r", vec![elsewhere, here])];
        let tty = g.tty.clone();
        adopt_terminals(&lanes, |pid| match pid {
            4242 => Some(tty.clone()),
            _ => Some("/dev/ttys999".into()),
        });
        let found = terminals::glances().get("test:adopts:here").cloned().unwrap_or_default();
        assert!(found.iter().any(|t| t.id == g.id && t.hosts), "{found:?}");
        assert!(terminals::glances().get("test:adopts:elsewhere").is_none());
        // A new conversation in the same process (`/clear`): the terminal goes with it.
        let next = crate::model::Session { live: true, pid: Some(4242), ..crate::fixtures::session("test:adopts:next") };
        let tty = g.tty.clone();
        adopt_terminals(&[crate::fixtures::lane("/r", vec![next])], |pid| (pid == 4242).then(|| tty.clone()));
        assert!(terminals::glances().get("test:adopts:next").is_some_and(|l| l.iter().any(|t| t.id == g.id && t.hosts)));
        assert!(terminals::glances().get("test:adopts:here").is_none());
        terminals::close(&g.id).unwrap();
    }
}
