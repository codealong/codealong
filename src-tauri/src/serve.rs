//! Codealong's server, `codealong serve`: the process that runs the engine and everything that
//! goes on whether a window is open or not. The engine and its board, your other devices
//! (peers.rs), the sessions Codealong runs itself (acp.rs) and their side conversations
//! (side.rs), the schedules, the automatic reviews and the rest of what acts on a board as it's
//! published, the token ledger and the socket the `codealong` command and the app talk to
//! (server.rs). The app is a view of it (view.rs): it shows the board the server publishes and
//! asks it for everything else (calls.rs), and quitting the app leaves the server, and the agents
//! it runs, going. The command starts one when none answers.
//!
//! It's started as a job of its own, launchd's on macOS (os/background.rs): never as the child of
//! whoever asked for it, whose sandbox and environment it would take, and whose end would be its
//! own. One runs for a folder of Codealong's files: a second finds the first answering and leaves.
//! An app keeps the server it finds when both were built from the same engine (`ENGINE`), so a
//! build that changed only the windows leaves the agents running, and replaces it otherwise.

use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::calls::Ctx;
use crate::engine::Handle;
use crate::server::{self, Request, Status};
use crate::util::words;
use crate::{acp, actions, agentdone, api, autoreview, config, engine, helpers, ledger, limits, nudges, origins, os, peers, perf, reconnect, rerequest, reviewdone, schedules, search, send, side, store, terminal, util, views};

/// What the engine was built from (build.rs): a hash of the Rust that goes into it, so two builds
/// that differ only in their windows have the same.
pub const ENGINE: &str = env!("CODEALONG_ENGINE");

/// What `Status::role` says of a server; an app from before there were servers says nothing.
pub const ROLE: &str = "server";

/// How long a server just started has to answer on its socket.
const START_WAIT: Duration = Duration::from_secs(20);

/// How long a server asked to quit has to go before its job is taken out of launchd, which ends it.
const QUIT_WAIT: Duration = Duration::from_secs(10);

/// The server's log is cut back to nothing at its start once it's grown past this.
const LOG_MAX: u64 = 20 * 1024 * 1024;

/// `codealong serve`: runs the server here until it's asked to quit.
pub fn run() -> i32 {
    // One server for the files: one already answering keeps them, and this one leaves without
    // an error, so launchd doesn't start it again.
    if let Some(s) = running() {
        eprintln!("Codealong's server is already running (pid {}).", s.pid);
        return 0;
    }
    trim_log();
    perf::arm();
    os::system::no_app_nap();
    eprintln!("Codealong's server {} starting: pid {}, engine {ENGINE}, files in {}", env!("CARGO_PKG_VERSION"), std::process::id(), util::tilde(&config::support_dir()));
    match start() {
        Ok(_ctx) => loop {
            std::thread::park();
        },
        Err(e) => {
            eprintln!("Codealong's server couldn't start: {e}");
            1
        }
    }
}

/// What launchd writes the server's output to grows for as long as it's kept, so a long one is
/// emptied as a server starts: launchd opened it to append, so what's written next goes at its
/// start.
fn trim_log() {
    let path = os::background::log_path();
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > LOG_MAX) {
        if let Ok(f) = std::fs::OpenOptions::new().write(true).open(&path) {
            let _ = f.set_len(0);
        }
    }
}

/// Starts everything the server runs, and the socket last, once there's an engine to answer
/// with. What's started here runs for the life of the process.
pub fn start() -> Result<Ctx, String> {
    // Your other devices (peers.rs). They come before the engine, which publishes to them, and ask
    // it for the board and for answers once it's there.
    let engine_slot: Arc<OnceLock<Handle>> = Default::default();
    let peers = {
        let (boards, answers, runs) = (engine_slot.clone(), engine_slot.clone(), engine_slot.clone());
        peers::Peers::new(
            config::support_dir(),
            peers::Hooks {
                board: Box::new(move || boards.get().map(|e| e.board()).unwrap_or_default()),
                answer: Box::new(move |method, params, watching| match answers.get() {
                    Some(e) => actions::answer(e, method, params, watching),
                    None => Err("Codealong is still starting on that device.".into()),
                }),
                emit: Box::new(move |view| {
                    // The engine goes by what they run too (nudges.rs `running_elsewhere`), and the
                    // apps show them beside the board.
                    if let Some(engine) = runs.get() {
                        engine.peer_lanes(&view.lanes);
                        engine.peer_machines(view.machines.clone());
                    }
                    views::emit("devices", view);
                }),
                side: Box::new(|s| views::emit("side", &s)),
            },
        )
    };
    let publishes = peers.clone();
    let (mover, moves_with) = (Mutex::new(limits::Mover::default()), engine_slot.clone());
    let (nudger, nudges_with) = (Arc::new(Mutex::new(nudges::Nudger::default())), engine_slot.clone());
    let ends_with = engine_slot.clone();
    let (continuer, continues_with) = (Arc::new(Mutex::new(reconnect::Continuer::default())), engine_slot.clone());
    let (rerequester, rerequests_with) = (Arc::new(Mutex::new(rerequest::Rerequester::default())), engine_slot.clone());
    let (reporter, reports_with) = (Arc::new(Mutex::new(helpers::Reporter::default())), engine_slot.clone());
    let (reviewer, reviews_with) = (Arc::new(Mutex::new(autoreview::Reviewer::load())), engine_slot.clone());
    let reviews = reviewer.clone();
    let engine = engine::start(move |board| {
        // The apps first: they put together what they show from it (view.rs).
        views::emit("board", board);
        publishes.publish(board);
        // Only once there's a handle to move them with, so a board before that leaves them due.
        if let (Some(engine), Ok(mut mover)) = (moves_with.get(), mover.lock()) {
            for (key, to) in mover.due(board, board.generated_at) {
                let engine = engine.clone();
                std::thread::spawn(move || limits::move_now(&engine, &key, &to));
            }
        }
        // The same for news on PRs with their nudge on: one thread for the batch, since two sends
        // typing into one terminal at once would interleave, and it says how each went, so what
        // wasn't told is tried again.
        if let Some(engine) = nudges_with.get() {
            let due = nudger.lock().map(|mut n| n.due(board, board.generated_at)).unwrap_or_default();
            if !due.is_empty() {
                let (engine, nudger) = (engine.clone(), nudger.clone());
                std::thread::spawn(move || nudges::send_now(&engine, &nudger, due));
            }
        }
        // And sessions a lost connection stopped, once their API can be reached, a usage limit,
        // once it has lifted, or Codealong quitting, at once: one thread for the batch, for the
        // same reason, which tries the API first.
        if let Some(engine) = continues_with.get() {
            let cut_offs = config::current().is_ok_and(|c| c.sessions.continue_after_restart);
            let due = continuer.lock().map(|mut c| c.due(board, board.generated_at, cut_offs)).unwrap_or_default();
            if !due.is_empty() {
                let (engine, continuer) = (engine.clone(), continuer.clone());
                std::thread::spawn(move || reconnect::send_now(&engine, &continuer, due));
            }
        }
        // And reviewers of PRs of yours whose fixes went unsaid, asked to look again where
        // config.toml says to (rerequest.rs): one thread for the batch, as GitHub is asked.
        if let (Some(engine), Ok(cfg)) = (rerequests_with.get(), config::current()) {
            let due = rerequester.lock().map(|mut r| r.due(board, &cfg.pull_requests, board.generated_at)).unwrap_or_default();
            if !due.is_empty() {
                let (engine, rerequester) = (engine.clone(), rerequester.clone());
                std::thread::spawn(move || rerequest::send_now(&engine, &rerequester, due));
            }
        }
        // And sessions whose helpers report to them (helpers.rs): a turn that ended, a wait on you
        // that began, or a start stuck before its session. One thread for the batch.
        if let Some(engine) = reports_with.get() {
            let pending = &origins::current().pending;
            let due = reporter.lock().map(|mut r| r.due(pending, board, board.generated_at)).unwrap_or_default();
            if !due.is_empty() {
                let (engine, reporter) = (engine.clone(), reporter.clone());
                std::thread::spawn(move || helpers::send_now(&engine, &reporter, due));
            }
        }
        // PRs that wait on your review, reviewed without being asked when config.toml says to
        // (autoreview.rs): a thread for each job; one PR's jobs never overlap.
        if let Some(engine) = reviews_with.get() {
            let due = reviewer.lock().map(|mut r| r.due(board, &board.reviews, board.generated_at)).unwrap_or_default();
            for job in due {
                let (engine, reviewer) = (engine.clone(), reviewer.clone());
                std::thread::spawn(move || autoreview::run(&engine, &reviewer, job));
            }
        }
        // Sessions that asked from the command to be ended once their turn ended (agentdone.rs);
        // closing a terminal takes a moment, so off this thread.
        if let Some(engine) = ends_with.get() {
            for key in agentdone::due(board) {
                let engine = engine.clone();
                std::thread::spawn(move || match actions::end_after_turn(&engine, &key) {
                    Ok(said) => eprintln!("Ended {key} once its turn did, as it asked: {said}"),
                    Err(e) => eprintln!("Couldn't end {key} once its turn did: {e}"),
                });
            }
        }
    });
    let _ = engine_slot.set(engine.clone());
    // Schedules (schedules/runner.rs): what's due, every few seconds.
    schedules::runner::spawn(engine.clone());
    // Automatic reviews' sessions once they're done with (reviewdone.rs).
    reviewdone::spawn(engine.clone());
    // Sessions Codealong runs itself: a turn's start and end recompose the board, and the reply as
    // it streams goes to the apps, which hand it to whichever windows show the session.
    {
        let (engine, watchers) = (engine.clone(), peers.clone());
        acp::init(move |change| match change {
            acp::Change::Board => engine.hosted_changed(),
            acp::Change::Stream(s) => {
                // Linked devices watching the session get it too (a phone showing it).
                watchers.stream(s.clone());
                views::emit("hosted-stream", &s);
            }
        });
    }
    // A side conversation's answer, as its copy writes it, for the windows showing its session and
    // the linked devices that asked about it; the board again when what it lists of them changes.
    {
        let (engine, askers) = (engine.clone(), peers.clone());
        side::init(
            move |s| {
                askers.side(&s);
                views::emit("side", &s);
            },
            move || engine.sides_changed(),
        );
    }
    let ctx = Ctx { engine: engine.clone(), peers: peers.clone(), reviews };
    server::serve(ctx.clone())?;
    // What a command you ran for a session printed goes to that session once it ends, as your
    // message: the `!` you ran it with is for the session to read it.
    {
        let board = engine.clone();
        terminal::report_ended(move || board.read(|b| !b.loading), move |run, text| {
            let command = util::one_line(&run.run.command, 60);
            let told = tell_run(&engine, &peers, &run.run, text).map_err(|why| format!("What `{command}` printed didn't reach “{}”: {why} `codealong terminal read {}` has it.", run.run.session_title, run.run.id));
            engine.run_reported(told);
        });
    }
    // After the first scan and the first git runs, which have the Mac to themselves.
    search::spawn_warmer(Duration::from_secs(20));
    // The token ledger reads every record once, so it waits for the search index's first read.
    ledger::spawn(Duration::from_secs(45));
    std::thread::spawn(acp::refresh_adapters);
    Ok(ctx)
}

/// Sends a session what a command you ran for it printed, as yours (terminal.rs `report_ended`):
/// not to one that takes a message only pasted into its app, which would come forward from the
/// background with it on the clipboard and nobody there to paste it.
fn tell_run(engine: &Handle, peers: &peers::Peers, run: &terminal::Run, text: String) -> Result<(), String> {
    let cfg = config::current().map_err(words)?;
    let way = send::way_in(&engine.board(), &cfg, &run.session).map_err(words)?;
    if matches!(way, Some(api::Way::ClaudeApp | api::Way::Antigravity | api::Way::Cursor)) {
        return Err("it takes a message only pasted into its app, so it wasn't sent.".into());
    }
    let sent = actions::run(engine, peers, api::SESSION_SEND, &run.session, actions::params::Send { text, ..Default::default() })?;
    match sent.get("opened").and_then(serde_json::Value::as_bool) {
        Some(true) => Err("its app opened with it to paste, which nobody did.".into()),
        _ => Ok(()),
    }
}

/// Whatever answers on the socket now: a server, or an app from before there were servers.
pub fn running() -> Option<Status> {
    running_within(Duration::from_secs(5))
}

fn running_within(wait: Duration) -> Option<Status> {
    match server::ask_within(&Request::Status, wait) {
        Some(Ok(v)) => serde_json::from_value(v).ok(),
        _ => None,
    }
}

/// Whether a status is a server's, not an app's from before there were servers.
pub fn is_server(s: &Status) -> bool {
    s.role.as_deref() == Some(ROLE)
}

/// The server, started when none answers. With `engine`, one built with another engine is
/// replaced, as an app wants (view.rs); without, whichever answers will do, as the command takes
/// it. Starts are one at a time for the folder of files, so a command and an app starting
/// together start one between them.
pub fn ensure(engine: Option<&str>) -> Result<Status, String> {
    let _one = store::lock(&config::support_dir().join("server")).map_err(words)?;
    match running() {
        Some(s) if !is_server(&s) || engine.is_none_or(|e| s.engine.as_deref() == Some(e)) => return Ok(s),
        Some(s) => {
            eprintln!("Replacing Codealong's server (pid {}, engine {}) with one of engine {}.", s.pid, s.engine.as_deref().unwrap_or("?"), engine.unwrap_or_default());
            stop_it(&s);
        }
        None => {}
    }
    launch()?;
    let until = Instant::now() + START_WAIT;
    while Instant::now() < until {
        if let Some(s) = running_within(Duration::from_secs(2)).filter(is_server) {
            return Ok(s);
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Err(format!("Codealong's server didn't start: {} may say why.", util::tilde(&os::background::log_path())))
}

/// Starts a server of this binary's, as a job of its own.
fn launch() -> Result<(), String> {
    let exe = std::env::current_exe().and_then(std::fs::canonicalize).map_err(|e| format!("Couldn't find this program: {e}"))?;
    let env = crate::install::server_environment();
    let log = os::background::log_path();
    if let Some(dir) = log.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let job = os::background::Job { exe: &exe, env: &env, log: &log };
    os::background::start(&job, &config::support_dir()).map_err(words)
}

/// Stops the server that answers, telling its apps to quit too, and waits for it to go. Whether
/// one was running.
pub fn stop() -> bool {
    let _one = store::lock(&config::support_dir().join("server"));
    match running() {
        Some(s) => {
            stop_it(&s);
            true
        }
        None => {
            // A job left loaded that runs nothing is taken out all the same.
            os::background::unload(&config::support_dir());
            false
        }
    }
}

/// Asks `s` to quit, and once it's gone or the time is up, takes its job out of launchd, which
/// ends it if it's still there.
fn stop_it(s: &Status) {
    let _ = server::ask_within(&Request::Quit, Duration::from_secs(3));
    let until = Instant::now() + QUIT_WAIT;
    // SAFETY: kill with no signal only asks whether the process is there.
    while unsafe { libc::kill(s.pid as libc::pid_t, 0) } == 0 && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(100));
    }
    os::background::unload(&config::support_dir());
    if unsafe { libc::kill(s.pid as libc::pid_t, 0) } == 0 {
        // Not launchd's (one started by hand, or on Linux): ended outright.
        unsafe { libc::kill(s.pid as libc::pid_t, libc::SIGKILL) };
    }
}
