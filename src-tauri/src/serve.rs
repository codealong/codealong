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

use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::calls::Ctx;
use crate::engine::Handle;
use crate::server::{self, Request, Status};
use crate::util::words;
use crate::{acp, actions, agentdone, api, autoreview, config, engine, helpers, hibernate, ledger, limits, listen, meetings, nudges, origins, os, outbox, peers, perf, reconnect, rerequest, reviewdone, schedules, search, send, side, store, terminal, terminals, util, views};

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

/// How long an app waits for a server it asked to hand over to come back as its build.
const HAND_OVER_WAIT: Duration = Duration::from_secs(60);

/// Set for the build a server becomes in place (`hand_over`): the file that lists the terminals
/// it handed over, and the socket their descriptors wait in.
const HANDOVER_VAR: &str = "CODEALONG_HANDOVER";
const HANDOVER_FDS_VAR: &str = "CODEALONG_HANDOVER_FDS";

/// How many descriptors go in one message of the hand-over's socket.
const FDS_A_MESSAGE: usize = 16;

/// The hand-over socket's buffers: all its messages wait in it at once. macOS's default held nine
/// messages of sixteen (Codex's probe, 2026-11-04); this holds thousands.
const HAND_OVER_ROOM: usize = 4 * 1024 * 1024;

/// What the build a server became in place takes over as it starts (`take_handover`).
static HANDED: Mutex<Option<Handover>> = Mutex::new(None);

/// What a server hands the build it becomes: its terminals, and the processes of its own it
/// started that aren't theirs, which this build reaps as they end.
#[derive(serde::Serialize, serde::Deserialize, Default)]
struct Handover {
    version: u32,
    terminals: Vec<terminals::Handed>,
}

/// `codealong serve`: runs the server here until it's asked to quit.
pub fn run() -> i32 {
    // One server for the files: one already answering keeps them, and this one leaves without
    // an error, so launchd doesn't start it again.
    if let Some(s) = running() {
        eprintln!("Codealong's server is already running (pid {}).", s.pid);
        return 0;
    }
    // Before anything else runs: the variable goes from the environment before there are threads
    // to read it, and the processes the server started before it became this build are known
    // before this one starts any.
    take_handover();
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
    // Here rather than in `run`, so the app's own server, when it runs one, gets it too.
    if os::system::more_open_files().is_none_or(|n| n < 1024) {
        eprintln!("Codealong's server couldn't raise its limit on open files, which may run out under many agents.");
    }
    // The files' one server first, then the socket: a server that can't have both starts nothing,
    // so no second engine runs. The lock is kept only once the socket is: a start that couldn't
    // bind (in the app, say) leaves it for the next.
    let run = run_lock(&config::support_dir())?;
    let bound = server::bind()?;
    keep_run_lock(run);
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
    let holds_with = engine_slot.clone();
    let (continuer, continues_with) = (Arc::new(Mutex::new(reconnect::Continuer::default())), engine_slot.clone());
    let cut_by_us = continuer.clone();
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
        // Sessions that asked from the command to be ended once their turn ended (agentdone.rs),
        // off this thread.
        if let Some(engine) = ends_with.get() {
            agentdone::end_due(engine, board, board.generated_at);
        }
        // And messages held for sessions that waited on their user to answer a prompt, once they
        // don't (outbox.rs): one thread for the batch.
        if let Some(engine) = holds_with.get() {
            outbox::send_due(engine, board, util::now_ms());
        }
    });
    let _ = engine_slot.set(engine.clone());
    // Agents' messages to sessions waiting on their user are held from now on, before the socket
    // answers; what a server before this one held goes as the first boards allow, and a clock
    // retries and drops them while the board sits still.
    outbox::start(engine.clone());
    // Schedules (schedules/runner.rs): what's due, every few seconds.
    schedules::runner::spawn(engine.clone(), peers.clone());
    // Automatic reviews' sessions once they're done with (reviewdone.rs).
    reviewdone::spawn(engine.clone());
    // Sessions left idle long enough to hibernate, where config.toml says to (hibernate.rs), on a
    // clock of their own: a board that sits still is what crossing the wait looks like.
    hibernate::spawn(engine.clone());
    // Sessions waiting to be ended once their turn has, between the boards it publishes.
    agentdone::spawn(engine.clone());
    // The sessions that listen (listen.rs): the ones that were listening are taken up again, and
    // each is told what's new as it's heard.
    listen::spawn(engine.clone());
    // The meetings in your calendar (meetings.rs): one about to start is offered, to take its notes.
    meetings::spawn(engine.clone());
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
    // Codealong's own terminals: their screens for the windows showing them, the board again when
    // what it lists of them changes.
    let living = config::support_dir().join("terminals.json");
    let lived = terminals::living_before(&living);
    {
        let engine = engine.clone();
        terminals::init(|frame| views::emit("terminal-frame", frame), move || engine.terminals_changed(), living);
    }
    // The terminals the server handed over as it became this build, before the socket answers, so
    // the first board a window gets has them.
    if let Some(handed) = HANDED.lock().ok().and_then(|mut h| h.take()) {
        let n = terminals::take_over(handed.terminals);
        eprintln!("Took over {n} of Codealong's terminals from the server's last build.");
    }
    // Sessions that lived in the last server's terminals and didn't come across ended with it (a
    // crash, a quit, a server of another binary replaced): one cut off partway is carried on, as a
    // session Codealong runs itself is after a restart, in a terminal of Codealong's again.
    let ended = terminals::ended_with_server(&lived);
    if !ended.is_empty() {
        eprintln!("{} session(s) ended with the last server's terminals.", ended.len());
        if let Ok(mut c) = cut_by_us.lock() {
            c.ended_with_server(ended, util::now_ms());
        }
    }
    let ctx = Ctx { engine: engine.clone(), peers: peers.clone(), reviews };
    server::serve(bound, ctx.clone())?;
    // What a command you ran for a session printed goes to that session once it ends, as your
    // message: the `!` you ran it with is for the session to read it.
    {
        let board = engine.clone();
        terminal::report_ended(move || board.read(|b| !b.loading), move |run, text| {
            let command = util::one_line(&run.run.command, 60);
            let told = tell_run(&engine, &peers, &run.run, text).map_err(|why| format!("What `{command}` printed didn't reach “{}”: {why} `codealong terminal read {}` has it.", run.run.session_title, run.run.id));
            engine.went(engine::Trouble::TerminalRuns, told);
        });
    }
    // After the first scan and the first git runs, which have the Mac to themselves.
    search::spawn_warmer(Duration::from_secs(20));
    // The token ledger reads every record once, so it waits for the search index's first read.
    ledger::spawn(Duration::from_secs(45));
    std::thread::spawn(acp::refresh_adapters);
    Ok(ctx)
}

/// Hands this server's terminals to the build now at its binary's path by becoming it, for an app
/// of that build (`Request::HandOver`): the process stays, so the programs in its terminals stay
/// its children (they're waited on as before), launchd keeps its job, and macOS's privacy grants,
/// which go by the process that started them, hold. Each terminal's screen goes across as a
/// snapshot, listed in a file the new build reads (`take_handover`), and its side as a descriptor
/// sent through a socket (`pty::send_fds`): the descriptors are never inheritable, so a process
/// another thread starts as this one changes gets none, and only the socket's other end is passed
/// through the exec. Returns only when it couldn't: why.
pub fn hand_over() -> String {
    use std::os::fd::AsRawFd;
    let dir = config::support_dir().join("handover");
    let mut handed = Handover { version: 1, terminals: terminals::hand_over(&dir) };
    let (keep, give) = match std::os::unix::net::UnixDatagram::pair() {
        Ok(pair) => pair,
        Err(e) => return format!("couldn't make the socket to hand the terminals over in: {e}"),
    };
    // Room for every terminal's message at once, as nothing reads them until the new build does.
    terminals::pty::socket_room(keep.as_raw_fd(), HAND_OVER_ROOM);
    terminals::pty::socket_room(give.as_raw_fd(), HAND_OVER_ROOM);
    let mut sent = 0;
    for batch in handed.terminals.chunks(FDS_A_MESSAGE) {
        let fds: Vec<i32> = batch.iter().map(|h| h.fd).collect();
        let ids = batch.iter().map(|h| h.glance.id.as_str()).collect::<Vec<_>>().join("\n");
        if let Err(e) = terminals::pty::send_fds(give.as_raw_fd(), &fds, ids.as_bytes()) {
            // The ones that didn't fit end with this build, as without a hand-over; the rest
            // still go.
            eprintln!("Only {sent} of {} terminals could be handed over: {e}", handed.terminals.len());
            break;
        }
        sent += batch.len();
    }
    // The socket holds them now: this process's copies go.
    for h in &mut handed.terminals {
        // SAFETY: closing a copy this process made for the hand-over and owns alone.
        unsafe { libc::close(h.fd) };
        h.fd = -1;
    }
    drop(give);
    let file = dir.join("terminals.json");
    let written = serde_json::to_vec(&handed).map_err(|e| e.to_string()).and_then(|b| std::fs::write(&file, b).map_err(|e| e.to_string()));
    if let Err(e) = written {
        return format!("couldn't write {}: {e}", file.display());
    }
    let Ok(exe) = std::env::current_exe() else { return "couldn't find its binary".into() };
    let args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    let mut env: Vec<(std::ffi::OsString, std::ffi::OsString)> = std::env::vars_os().filter(|(k, _)| k != HANDOVER_VAR && k != HANDOVER_FDS_VAR).collect();
    env.push((HANDOVER_VAR.into(), file.clone().into_os_string()));
    env.push((HANDOVER_FDS_VAR.into(), keep.as_raw_fd().to_string().into()));
    let err = os::procs::exec_keeping(&exe, &args, &env, &[keep.as_raw_fd()]);
    format!("couldn't run {}: {err}", exe.display())
}

/// What the server this process was handed over (`hand_over`), read once at its start, while it's
/// the only thread: its file read and removed, and the variable that named it taken out, so what
/// this server starts doesn't see it. The processes the server started that aren't its terminals'
/// (sessions Codealong runs, commands it was waiting on) lost their way to it as it changed, and
/// are reaped as they end.
fn take_handover() {
    let Some(file) = std::env::var_os(HANDOVER_VAR) else { return };
    let socket = std::env::var(HANDOVER_FDS_VAR).ok().and_then(|s| s.parse::<i32>().ok());
    std::env::remove_var(HANDOVER_VAR);
    std::env::remove_var(HANDOVER_FDS_VAR);
    // The descriptors waiting in the socket, by their terminals' ids, each closed on exec as it's
    // taken, before anything here starts a process.
    let mut fds: std::collections::HashMap<String, i32> = std::collections::HashMap::new();
    if let Some(sock) = socket {
        let _ = terminals::pty::cloexec(sock);
        while let Ok(Some((got, ids))) = terminals::pty::recv_fds(sock, FDS_A_MESSAGE) {
            let ids = String::from_utf8_lossy(&ids).into_owned();
            for (id, fd) in ids.split('\n').zip(got) {
                fds.insert(id.to_string(), fd);
            }
        }
        // SAFETY: the socket's end this process was handed, read to its end.
        unsafe { libc::close(sock) };
    }
    let read = std::fs::read(&file).ok().and_then(|b| serde_json::from_slice::<Handover>(&b).ok());
    let _ = std::fs::remove_file(&file);
    let Some(mut handed) = read else {
        eprintln!("The terminals the server's last build handed over couldn't be read from {}.", PathBuf::from(&file).display());
        for fd in fds.into_values() {
            // SAFETY: a descriptor this process was handed and nothing will use.
            unsafe { libc::close(fd) };
        }
        return;
    };
    for h in &mut handed.terminals {
        h.fd = fds.remove(&h.glance.id).unwrap_or(-1);
    }
    for fd in fds.into_values() {
        // SAFETY: as above: handed over with no terminal listed for it.
        unsafe { libc::close(fd) };
    }
    let theirs: Vec<u32> = handed.terminals.iter().map(|h| h.pid).collect();
    let me = std::process::id();
    let strays: Vec<u32> = crate::procs::list(Duration::from_secs(5)).lines.iter().filter(|p| p.ppid == me && !theirs.contains(&p.pid)).map(|p| p.pid).collect();
    for pid in strays {
        std::thread::spawn(move || {
            // SAFETY: waitpid on a child this process started before it became this build.
            unsafe { libc::waitpid(pid as libc::pid_t, std::ptr::null_mut(), 0) };
        });
    }
    if let Ok(mut h) = HANDED.lock() {
        *h = Some(handed);
    }
}

/// Asks a server of another build of this app, at this binary's path, to become this build in
/// place (`Request::HandOver`), so the terminals it holds carry on. The server once it answers as
/// this build; None when it had none to hand over (it quits then, as `Quit` has it) or didn't
/// come back as this build in time.
fn hand_over_from(s: &Status) -> Option<Status> {
    let asked = server::ask_within(&Request::HandOver, Duration::from_secs(3))?.ok()?;
    if !asked.get("handsOver").and_then(serde_json::Value::as_bool).unwrap_or(false) {
        return None;
    }
    eprintln!("Codealong's server (pid {}) becomes this build, handing its terminals over.", s.pid);
    // Its apps quit first, then each terminal is handed over once what it read and what it was
    // writing are in (terminals.rs `hand_over`): seconds at most on a busy Mac.
    let until = Instant::now() + HAND_OVER_WAIT;
    while Instant::now() < until {
        if let Some(now) = running_within(Duration::from_secs(2)).filter(|n| n.pid == s.pid && n.engine.as_deref() == Some(ENGINE)) {
            return Some(now);
        }
        if !crate::procs::pid_alive(s.pid) {
            return None;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    None
}

/// Keeps, for the life of the process, the lock one server for a folder of Codealong's files holds
/// beside them, however it was started (launchd, by hand, or in the app): two starting together,
/// with a socket left by a crash, would each take the other's new socket for that one and remove
/// it. The system lets it go as the process ends, crashed or not.
fn keep_run_lock(file: std::fs::File) {
    static HELD: OnceLock<std::fs::File> = OnceLock::new();
    let _ = HELD.set(file);
}

/// The run lock for the files in `dir`, held for as long as the file is open.
fn run_lock(dir: &std::path::Path) -> Result<std::fs::File, String> {
    use std::os::fd::AsRawFd;
    let _ = std::fs::create_dir_all(dir);
    let path = dir.join("server.run.lock");
    let file = std::fs::OpenOptions::new().create(true).truncate(false).write(true).open(&path).map_err(|e| format!("Couldn't open {}: {e}", path.display()))?;
    // SAFETY: flock on a descriptor this process owns. Each open is a lock of its own, so a second
    // start in this process is refused as another's is.
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        return Err("Another of Codealong's servers is running for these files.".into());
    }
    Ok(file)
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

/// This process's binary, as its path reads once links are followed.
pub fn this_binary() -> Option<PathBuf> {
    std::env::current_exe().and_then(std::fs::canonicalize).ok()
}

/// Whether a server is this build's own: the same binary, built with the same engine. Codealong
/// Dev relaunched with only its windows changed is: its bundle is replaced where it was. A server
/// another binary runs isn't, even of the same engine (Codealong beside Codealong Dev, a debug
/// build in a worktree), since what it installs and starts comes from where it runs.
pub fn ours(s: &Status) -> bool {
    is_server(s) && same_build(s.engine.as_deref(), s.exe.as_deref())
}

/// Whether a process that says it runs `exe`, built with `engine`, is this build.
pub fn same_build(engine: Option<&str>, exe: Option<&str>) -> bool {
    engine == Some(ENGINE) && exe.map(PathBuf::from) == this_binary()
}

/// What `ensure` says to an app when the server that isn't its build's has an app on it opened
/// after this one: that one stays, and this one gives way rather than replacing its server.
pub const GIVE_WAY: &str = "A Codealong opened after this one runs on that server.";

/// The server, started when none answers. With `own`, a server that isn't this build's (`ours`)
/// is replaced, as an app wants (view.rs), unless an app opened after `since` is on it
/// (`GIVE_WAY`); without, whichever answers will do, as the command takes it. Starts are one at a
/// time for the folder of files, so a command and an app starting together start one between
/// them, and two apps opening together decide here which of them stays.
pub fn ensure(own: bool, since: Option<i64>) -> Result<Status, String> {
    ensure_held(own, since).map(|(s, _)| s)
}

/// `ensure`, with the start lock still held, for an app to connect to the server under it: an
/// older app's start, waiting on the lock, then finds the newer one connected and gives way, where
/// in between the two it would have found none and replaced the newer one's server.
pub fn ensure_held(own: bool, since: Option<i64>) -> Result<(Status, store::Lock), String> {
    let one = store::lock(&config::support_dir().join("server")).map_err(words)?;
    let found = ensure_under(own, since)?;
    Ok((found, one))
}

fn ensure_under(own: bool, since: Option<i64>) -> Result<Status, String> {
    match running() {
        Some(s) if !is_server(&s) || !own || ours(&s) => return Ok(s),
        Some(s) if since.is_some_and(|me| s.views.iter().any(|v| v.since > me)) => return Err(GIVE_WAY.into()),
        Some(s) => {
            // One at this binary's path (the app updated where it is) becomes this build, so its
            // terminals carry on; else it's replaced, ending them.
            if s.hand_over && s.exe.as_deref().map(PathBuf::from) == this_binary() {
                if let Some(now) = hand_over_from(&s) {
                    return Ok(now);
                }
            }
            eprintln!("Replacing Codealong's server (pid {}, engine {}, {}) with this build's.", s.pid, s.engine.as_deref().unwrap_or("?"), s.exe.as_deref().unwrap_or("?"));
            stop_it(&s, Request::Quit);
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
    let exe = this_binary().ok_or("Couldn't find this program.")?;
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
            stop_it(&s, Request::Quit);
            true
        }
        None => {
            // A job left loaded that runs nothing is taken out all the same.
            os::background::unload(&config::support_dir());
            false
        }
    }
}

/// Starts the server again, for the app's Restart server (menubar.rs): it quits without its apps,
/// which find it gone and start one of their own build as they do when it ends any other way
/// (view.rs `follow`). The start lock is held until the one that quit is gone and its job taken
/// out, so the start waits for that. Whether one was running and quit; one that runs in this
/// process (the app's, after its starts failed) is left be, since it would take the app with it.
pub fn restart() -> bool {
    let _one = store::lock(&config::support_dir().join("server"));
    match running() {
        Some(s) if s.pid == std::process::id() => false,
        Some(s) => {
            stop_it(&s, Request::Restart);
            true
        }
        None => false,
    }
}

/// Asks `s` to quit (`asked`: with its apps, or for them to start it again), and once it's gone
/// or the time is up, takes its job out of launchd, which ends it if it's still there.
fn stop_it(s: &Status, asked: Request) {
    // One too stuck to answer is ended at once rather than waited on.
    if !matches!(server::ask_within(&asked, Duration::from_secs(3)), Some(Ok(_))) {
        // SAFETY: kill only signals a process; one gone by now is refused.
        unsafe { libc::kill(s.pid as libc::pid_t, libc::SIGTERM) };
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// An app keeps a server only when it's this build's: its binary and its engine.
    #[test]
    fn a_server_is_ours_only_with_our_binary_and_engine() {
        let exe = this_binary().map(|p| p.to_string_lossy().into_owned());
        let ours_now = Status { role: Some(ROLE.into()), engine: Some(ENGINE.into()), exe: exe.clone(), ..Default::default() };
        assert!(ours(&ours_now));
        assert!(!ours(&Status { engine: Some("0000000000000000".into()), ..ours_now.clone() }), "another engine");
        assert!(!ours(&Status { exe: Some("/Applications/Codealong.app/Contents/MacOS/codealong".into()), ..ours_now.clone() }), "another binary");
        assert!(!ours(&Status { role: None, ..ours_now.clone() }), "an app from before there were servers");
        assert!(!ours(&Status { exe: None, ..ours_now }), "a server that doesn't say");
    }

    /// One server for a folder of files: a second start is refused while the first holds the
    /// lock, in this process or another, and takes it once the first has gone.
    #[test]
    fn one_server_holds_a_folders_run_lock() {
        let dir = crate::fixtures::scratch("run-lock");
        let first = run_lock(&dir).unwrap();
        assert!(run_lock(&dir).unwrap_err().contains("Another of Codealong's servers"));
        drop(first);
        assert!(run_lock(&dir).is_ok());
    }
}
