//! The server's socket (serve.rs), for the `codealong` command (cli.rs) and the app (view.rs):
//! JSON lines over a Unix socket next to the config, one request per line and one reply per line.
//! A request is what a window's command does, through the same engine handle, so the engine stays
//! the one writer of todos.json and a reply is the board the windows have. An app also holds a
//! connection open that the server writes events to (`Subscribe`, views.rs), and hands it the
//! windows' commands to answer (`Call`, calls.rs). The socket is mode 0600: it answers the user
//! running the server, and only them.

use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use crate::config::support_dir;
use crate::calls::Ctx;
use crate::engine::Handle;
use crate::peers::{AskView, DeviceView, DevicesView, FoundView, MeView};
use crate::start::StartSpec;
use crate::todos::TodoOp;

pub fn path() -> PathBuf {
    support_dir().join("codealong.sock")
}

/// What the command can ask. `method` picks one; the rest are its arguments.
#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "method", rename_all = "kebab-case", rename_all_fields = "camelCase")]
pub enum Request {
    /// Is the app up, and which one.
    Status,
    /// Quits, telling the apps connected to it to quit too: for an app that wants a server of its
    /// own engine, `codealong server stop`, or an app from before there were servers, which runs
    /// its own engine (`take_over`). The reply is the pid that quits.
    Quit,
    /// Quits without telling the apps, which start a server again as they find it gone: the menu
    /// bar's Restart server (serve.rs `restart`). The reply is the pid that quits.
    Restart,
    /// Tells the apps connected to the server that opened before `since` to quit, and answers once
    /// they have: for an app that's opening, which takes their place (the last one opened wins,
    /// view.rs `take_over`); with none, every app.
    TakeViews {
        #[serde(default)]
        since: Option<i64>,
    },
    /// What a window asks the engine (calls.rs), by its command's name, with its arguments as the
    /// window sent them.
    Call {
        name: String,
        #[serde(default)]
        args: Value,
        /// The build of the app that asks, which a server of another build turns away unrun
        /// (`FOREIGN_CALL`): the app sends it again once it's connected to its own.
        #[serde(default)]
        engine: Option<String>,
        #[serde(default)]
        exe: Option<String>,
    },
    /// An app connecting as a view (views.rs): the connection is kept, and the server writes the
    /// board and everything else the windows hear to it, one event a line, for as long as it's open.
    Subscribe { view: crate::views::Info },
    /// A view's answer to what the server asked it (`views::ask`).
    ViewAnswer {
        id: u64,
        #[serde(default)]
        result: Value,
        #[serde(default)]
        error: Option<String>,
    },
    /// The board as the windows have it.
    Board,
    /// A change to a todo or the labels; the reply is the time of the board that shows it.
    TodoApply { op: TodoOp },
    /// Starts a session, and links it to its todos.
    Start { spec: StartSpec },
    /// Starts an agent's review of a PR now, as Review with… does (autoreview.rs `review_now`),
    /// on `account` (an account or a routing group), else the reviewer config.toml names. The
    /// reply is where its round stands (`autoreview::Begun`).
    PrReview {
        pr: String,
        #[serde(default)]
        account: Option<String>,
    },
    /// Sessions whose prompts and replies have every word (search.rs).
    Search { words: Vec<String> },
    /// What closing idle sessions would close, and with `archive` only archive (idle.rs).
    IdleSessions {
        after_ms: i64,
        #[serde(default)]
        archive: bool,
    },
    /// Closes these sessions, each checked again first, and with `archive` archives them.
    CloseIdle {
        keys: Vec<String>,
        after_ms: i64,
        #[serde(default)]
        archive: bool,
    },
    /// Your other devices (peers.rs) as Settings › Devices shows them, with how many worktrees and
    /// sessions each has in place of the worktrees themselves.
    Devices,
    /// Asks one of your devices on your tailnet to link; the reply is the code both show.
    DeviceLink { key: String },
    /// Says yes, or no, to a device asking to link.
    DeviceAnswer { key: String, accept: bool },
    DeviceUnlink { key: String },
    /// A link code for a device that isn't on your tailnet.
    DeviceCode,
    /// Links with the device that made this link code; the reply is its name.
    DeviceUseCode { code: String },
    /// Runs a request on a linked device, as this device's windows do, by its short id: what the
    /// command may ask for (`actions::command_may_ask`).
    DeviceAsk { device: String, request: String, params: Value },
    /// One of the app's requests about a session (actions.rs `REQUESTS`: `session-send`,
    /// `session-cancel`, `session-fork`…), for the session or worktree `key` with `params`
    /// (empty for a request about neither), asked as the
    /// command (`actions::From::Command`): what a window may ask, less what is its user's alone.
    /// `from` is the session that ran the command, when it found one, which a message says it's
    /// from (send.rs).
    Run {
        name: String,
        key: String,
        #[serde(default)]
        params: Value,
        #[serde(default)]
        from: Option<String>,
    },
    /// Brings the main window forward on a session, worktree, PR or todo (`Reveal` in api.ts), as
    /// the floating bar's Show in Codealong does. The reply is the app's bundle ID, or null when
    /// it runs outside a bundle: macOS may not let an app come forward when nobody clicked in it,
    /// so the command opens it by that ID too.
    Show { reveal: Value },
    /// A change to a board (boards.rs `BoardOp`); the reply is the board as it is now.
    BoardApply { op: crate::boards::BoardOp },
    /// What a board holds, moving a card on its Kanban, or arranging its canvas (boards.rs `Ask`):
    /// the main window works it out, and the reply is its answer.
    BoardAsk { board: String, project: String, ask: crate::boards::Ask },
    /// A permission request from Claude Code's `PermissionRequest` hook, its input as the hook
    /// got it, with the session's process and account folder: the connection is held until it's
    /// answered here or where the session runs, and the reply is the decision, or null for none
    /// (claude/asks.rs).
    PermissionAsk {
        hook: Value,
        #[serde(default)]
        pid: Option<u32>,
        #[serde(default)]
        home: Option<String>,
        /// When the hook first asked, and how long the call's record was then: a hook asking
        /// again after the app it asked quit goes by its first ask (claude/asks.rs).
        #[serde(default)]
        at: Option<i64>,
        #[serde(default)]
        from: Option<u64>,
    },
    /// A batch of a reply as Claude Code shows it, from the plugin's `MessageDisplay` hook, its
    /// input as the hook got it, with the session's account folder (claude/live.rs). The reply is
    /// empty: the hook doesn't wait.
    MessageShown { hook: Value, home: String },
    /// A command an agent asks to run in a terminal of yours: the connection is held until you
    /// answer it, and the reply says how (terminal.rs `reply`). A request whose command goes first
    /// goes from the windows.
    TerminalAsk { ask: crate::terminal::Ask },
    /// Interrupts a run of `codealong terminal run`, or with `close` closes its tab, here in the
    /// app: an agent's sandbox may not let its command look at your processes or signal them.
    TerminalStop {
        id: String,
        #[serde(default)]
        close: bool,
    },
    /// Where a run of `codealong terminal run` stands, or with no id every run kept (only those
    /// `session` asked for when it names one), as the app sees them: whether a run's processes are
    /// still there is a look at yours, which an agent's sandbox may not let its command take.
    TerminalRuns {
        #[serde(default)]
        id: Option<String>,
        #[serde(default)]
        session: Option<String>,
    },
}

/// How a reply starts when the app doesn't know the request: it's older than the command.
pub const UNKNOWN_REQUEST: &str = "That isn't a request the app knows";

/// What a server answers a call from an app of another build, which it didn't run.
pub const FOREIGN_CALL: &str = "That app is another build's than this server's.";

#[derive(Serialize, Deserialize, Debug)]
pub struct Reply {
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl Reply {
    fn ok(v: impl Serialize) -> Reply {
        match serde_json::to_value(v) {
            Ok(result) => Reply { ok: true, result: Some(result), error: None },
            Err(e) => Reply::err(format!("Couldn't encode the reply: {e}")),
        }
    }
    fn err(e: impl Into<String>) -> Reply {
        Reply { ok: false, result: None, error: Some(e.into()) }
    }
}

/// What `status` says about the server.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub version: String,
    pub pid: u32,
    /// The board's time, and whether the first scan is still running.
    pub generated_at: i64,
    pub loading: bool,
    /// `server` (serve.rs `ROLE`); none from an app from before there were servers, which ran the
    /// engine itself.
    #[serde(default)]
    pub role: Option<String>,
    /// The engine it was built with (serve.rs `ENGINE`).
    #[serde(default)]
    pub engine: Option<String>,
    /// The binary it runs, which an app keeps only when it's its own (serve.rs `ensure`).
    #[serde(default)]
    pub exe: Option<String>,
    /// The apps connected to it, the latest last.
    #[serde(default)]
    pub views: Vec<crate::views::Info>,
}

/// The socket, bound for a server about to start: taken before it starts anything, so a server
/// that can't have it starts nothing and leaves (`serve` then serves it).
pub struct Bound {
    path: PathBuf,
    listener: (UnixListener, Option<(u64, u64)>),
}

/// Binds the socket, unless another server answers on it. A socket file left by a server that
/// didn't exit cleanly is taken over.
pub fn bind() -> Result<Bound, String> {
    let p = path();
    let _ = std::fs::create_dir_all(support_dir());
    if p.exists() {
        if UnixStream::connect(&p).is_ok() {
            return Err(format!("Another Codealong answers on {} already.", p.display()));
        }
        let _ = std::fs::remove_file(&p);
    }
    let listener = listen(&p).map_err(|e| format!("Couldn't open {}: {e}", p.display()))?;
    Ok(Bound { path: p, listener })
}

/// Serves the socket `bind` took for as long as the server runs.
pub fn serve(bound: Bound, ctx: Ctx) -> Result<(), String> {
    let Bound { path: p, listener } = bound;
    std::thread::Builder::new()
        .name("socket".into())
        .spawn(move || {
            accept_on(&p, listener, |stream| {
                let ctx = ctx.clone();
                let _ = std::thread::Builder::new().name("socket-client".into()).spawn(move || connection(stream, &ctx));
            })
        })
        .map(drop)
        .map_err(|e| format!("Couldn't start the socket's thread: {e}"))
}

/// The socket at `p`, mode 0600, and which socket it is (`socket_id`), told as it's bound, before
/// anything can close it.
fn listen(p: &Path) -> std::io::Result<(UnixListener, Option<(u64, u64)>)> {
    use std::os::fd::AsRawFd;
    let listener = UnixListener::bind(p)?;
    let _ = std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o600));
    let id = socket_id(listener.as_raw_fd());
    Ok((listener, id))
}

/// Hands `serve` each connection to the socket at `p`, for as long as the app runs.
///
/// A descriptor of the listener closed or replaced while this thread waits in accept leaves the
/// socket draining on macOS (`SS_DRAINING`, which lsof shows as `SS=UNKNOWN=0x4000`): the accept
/// fails with ECONNABORTED and the socket goes on taking connections, but each one it accepts
/// takes that state too, and any read or write on it that would wait fails with EBADF. So a
/// request not in yet failed at once, and a reply stopped where the socket's 8 KB buffer filled.
/// The running app's listener was found so on 2026-10-20, twice, still fd 6 and waited on (a
/// `dup2` onto it while accept waits does just that); what in the app does it isn't found yet.
/// A listener closed outright has its number freed, for the next socket or file to take.
///
/// Either way a new socket at the same path has none of it: it's bound beside the old one and
/// renamed over it, so a command never finds the path missing, and one that can't be bound
/// leaves the old. The old number is closed only while it's still the old socket's, since once
/// it was closed under it, it may be the new socket's or another part of the app's.
fn accept_on(p: &Path, (mut listener, mut id): (UnixListener, Option<(u64, u64)>), mut serve: impl FnMut(UnixStream)) {
    use std::os::fd::{AsRawFd, IntoRawFd};

    // A socket that can't be told is taken for one that isn't the listener's.
    let still = |fd, id: Option<(u64, u64)>| id.is_some() && socket_id(fd) == id;
    loop {
        // Asked before each accept, since a number closed meanwhile may be another listening
        // socket's by now, which accept would wait on and serve.
        let why = if !still(listener.as_raw_fd(), id) {
            "its descriptor isn't the socket's any more".to_string()
        } else {
            match listener.accept() {
                Ok((stream, _)) => {
                    serve(stream);
                    continue;
                }
                Err(e) if e.kind() == std::io::ErrorKind::ConnectionAborted => e.to_string(),
                // One that lasts (too many open files) would spin this thread.
                Err(_) => {
                    std::thread::sleep(Duration::from_millis(100));
                    continue;
                }
            }
        };
        eprintln!("The socket for codealong commands stopped answering ({why}); opening it again.");
        let fresh = p.with_extension("new");
        let _ = std::fs::remove_file(&fresh);
        match listen(&fresh).and_then(|l| std::fs::rename(&fresh, p).map(|_| l)) {
            Ok((l, new_id)) => {
                let old = std::mem::replace(&mut listener, l);
                if still(old.as_raw_fd(), id) {
                    drop(old);
                } else {
                    let _ = old.into_raw_fd();
                }
                id = new_id;
            }
            Err(e) => {
                eprintln!("Couldn't open {} again: {e}", p.display());
                std::thread::sleep(Duration::from_secs(1));
            }
        }
    }
}

/// Which socket a listener's descriptor `fd` is now, by its device and inode, which a copy of it
/// shares and no other socket or file has; None once the descriptor is closed.
fn socket_id(fd: std::os::fd::RawFd) -> Option<(u64, u64)> {
    // SAFETY: fstat only writes the struct it's given, and fails on a closed descriptor.
    let mut st: libc::stat = unsafe { std::mem::zeroed() };
    (unsafe { libc::fstat(fd, &mut st) } == 0).then_some((st.st_dev as u64, st.st_ino as u64))
}

fn connection(stream: UnixStream, ctx: &Ctx) {
    let result = connection_on(stream, |req, out| {
        match req {
            Ok(Request::PermissionAsk { hook, pid, home, at, from }) => {
                hold_permission_ask(&ctx.engine, &hook, pid, home.as_deref(), at, from, out);
                None
            }
            Ok(Request::TerminalAsk { ask }) => {
                hold_terminal_ask(&ctx.engine, ask, out);
                None
            }
            Ok(Request::Subscribe { view }) => {
                if let Ok(stream) = out.try_clone() {
                    let (engine, peers) = (ctx.engine.clone(), ctx.peers.clone());
                    // The board as it is and your other devices' worktrees: what a view starts from.
                    let first = move || [crate::views::line("board", &engine.board()), crate::views::line("devices", &peers.view())].into_iter().flatten().collect();
                    crate::views::serve(stream, view, first);
                }
                None
            }
            req => Some(handle_request(ctx, req)),
        }
    });
    if let Err(e) = result {
        eprintln!("Couldn't answer a codealong command: {e}");
        if e.raw_os_error() == Some(libc::EMFILE) {
            out_of_files();
        }
    }
}

/// Says once, in the server's log, what it held as it first ran out of open files, read from the
/// kernel, since what held them is gone by the time anyone looks: on 2026-11-03 servers ran out
/// within minutes, and the questions their hooks handed over went back to their terminals.
fn out_of_files() {
    use std::sync::atomic::{AtomicBool, Ordering};
    static SAID: AtomicBool = AtomicBool::new(false);
    if SAID.swap(true, Ordering::Relaxed) {
        return;
    }
    let pid = std::process::id();
    let Some(k) = crate::procs::open_kinds(pid) else {
        eprintln!("Out of open files, and what holds them didn't read.");
        return;
    };
    let mut held: std::collections::HashMap<PathBuf, usize> = std::collections::HashMap::new();
    for path in crate::procs::open_files(pid).unwrap_or_default() {
        *held.entry(path).or_default() += 1;
    }
    let mut most: Vec<(PathBuf, usize)> = held.into_iter().collect();
    most.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let most: Vec<String> = most.iter().take(5).map(|(p, n)| format!("{} ×{n}", crate::util::tilde(p))).collect();
    eprintln!("Out of open files: {} sockets, {} files, {} pipes, {} other; the files held most: {}.", k.sockets, k.files, k.pipes, k.other, most.join(", "));
}

/// The socket exchange, with the app's actions supplied by `connection`.
fn connection_on(stream: UnixStream, mut answer: impl FnMut(serde_json::Result<Request>, &mut UnixStream) -> Option<Reply>) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(60)))?;
    let mut out = stream.try_clone()?;
    let reader = BufReader::new(stream);
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let Some(reply) = answer(serde_json::from_str(&line), &mut out) else { break };
        let mut text = serde_json::to_string(&reply).unwrap_or_else(|_| r#"{"ok":false,"error":"Couldn't encode the reply."}"#.into());
        text.push('\n');
        out.write_all(text.as_bytes())?;
        out.flush()?;
    }
    Ok(())
}

fn handle_request(ctx: &Ctx, req: serde_json::Result<Request>) -> Reply {
    let (engine, peers) = (&ctx.engine, &ctx.peers);
    let req = match req {
        Ok(r) => r,
        Err(e) => return Reply::err(format!("{UNKNOWN_REQUEST}: {e}")),
    };
    match req {
        Request::Status => {
            let b = engine.board();
            Reply::ok(Status {
                version: env!("CARGO_PKG_VERSION").to_string(),
                pid: std::process::id(),
                generated_at: b.generated_at,
                loading: b.loading,
                role: Some(crate::serve::ROLE.into()),
                engine: Some(crate::serve::ENGINE.into()),
                exe: crate::serve::this_binary().map(|p| p.to_string_lossy().into_owned()),
                views: crate::views::list(),
            })
        }
        Request::Board => Reply::ok(engine.board()),
        Request::Quit => {
            // After the reply is on its way. The apps go first: one left without its server would
            // start another.
            std::thread::spawn(|| {
                std::thread::sleep(Duration::from_millis(200));
                eprintln!("Codealong's server quitting, as asked.");
                crate::views::quit_all(Duration::from_secs(5), None);
                std::process::exit(0);
            });
            Reply::ok(std::process::id())
        }
        Request::Restart => {
            std::thread::spawn(|| {
                std::thread::sleep(Duration::from_millis(200));
                eprintln!("Codealong's server quitting to be started again, as asked.");
                std::process::exit(0);
            });
            Reply::ok(std::process::id())
        }
        Request::TakeViews { since } => {
            crate::views::quit_all(Duration::from_secs(8), since);
            Reply::ok(Value::Null)
        }
        Request::Call { engine: Some(engine), exe, .. } if !crate::serve::same_build(Some(&engine), exe.as_deref()) => Reply::err(FOREIGN_CALL),
        Request::Call { name, args, .. } => match crate::calls::call(ctx, &name, args) {
            Some(answer) => answered(answer),
            None => Reply::err(format!("{UNKNOWN_REQUEST}: no call {name}")),
        },
        Request::ViewAnswer { id, result, error } => {
            crate::views::answer(id, error.map_or(Ok(result), Err));
            Reply::ok(Value::Null)
        }
        // Held on their connection instead (`connection`).
        Request::Subscribe { .. } => Reply::ok(Value::Null),
        Request::TodoApply { op } => match engine.todo_within(op, Duration::from_secs(90)) {
            Ok(at) => Reply::ok(at),
            Err(e) => Reply::err(e),
        },
        Request::Search { words } => Reply::ok(crate::search::find(&words, 1000)),
        Request::IdleSessions { after_ms, archive } => Reply::ok(crate::idle::plan_now(&engine.board(), after_ms, archive)),
        Request::CloseIdle { keys, after_ms, archive } => answered(crate::idle::close_on(engine, &keys, after_ms, archive)),
        Request::Start { spec } => answered(crate::start::start_here(engine, &spec).map_err(crate::util::words)),
        Request::PrReview { pr, account } => answered(crate::autoreview::review_now(engine, &ctx.reviews, &pr, account.as_deref()).map(|()| crate::autoreview::begun(&ctx.reviews, &pr))),
        Request::Devices => Reply::ok(DevicesSummary::of(peers.view())),
        Request::DeviceLink { key } => answered(peers.link(&key)),
        Request::DeviceAnswer { key, accept } => answered(peers.answer_link(&key, accept)),
        Request::DeviceUnlink { key } => answered(peers.unlink(&key)),
        Request::DeviceCode => answered(peers.link_code()),
        Request::DeviceUseCode { code } => answered(peers.link_with_code(&code)),
        Request::DeviceAsk { device, request, params } => answered(crate::actions::command_may_ask(&request, &params).and_then(|_| peers.ask(&device, &request, params, Duration::from_secs(40)))),
        Request::Run { name, key, params, from } => {
            let ran = crate::actions::run_as(engine, peers, &crate::actions::From::Command(from), &name, crate::actions::On::Key(&key), params);
            // A session an agent marked done leaves your lists with no window of yours having done
            // it: the main window says which session did, with its Undo (Toasts.tsx).
            if let Ok(answer) = &ran {
                if name == codealong_peer::api::SESSION_DONE && answer.get("done").and_then(Value::as_bool) == Some(true) {
                    let message = answer.get("message").and_then(Value::as_str).unwrap_or_default();
                    let title = engine.read(|b| b.session(&key).map(|(_, s)| s.title.clone())).unwrap_or_default();
                    crate::views::emit("agent-done", &serde_json::json!({ "key": key, "title": title, "message": message }));
                }
            }
            answered(ran)
        }
        // The app that's connected shows it, and says which it is; with none, the one the command
        // opens shows it as it connects.
        Request::Show { reveal } => match crate::views::show(reveal) {
            Some(bundle) => Reply::ok(bundle),
            None => Reply::ok(own_bundle()),
        },
        Request::BoardApply { op } => {
            let applied = crate::boards::apply(op, crate::util::now_ms());
            if let Ok(a) = &applied {
                crate::boards::changed(a, None);
            }
            answered(applied.map_err(crate::util::words))
        }
        Request::BoardAsk { board, project, ask } => answered(crate::views::ask("board-ask", serde_json::json!({ "board": board, "project": project, "ask": ask }), crate::boards::WAIT + Duration::from_secs(5))),
        Request::MessageShown { hook, home } => {
            message_shown(&hook, &home);
            Reply::ok(Value::Null)
        }
        // Held on their connection instead (`connection`).
        Request::PermissionAsk { .. } | Request::TerminalAsk { .. } => Reply::ok(Value::Null),
        Request::TerminalRuns { id: Some(id), .. } => answered(crate::terminal::status(&id).map_err(crate::util::words)),
        Request::TerminalRuns { id: None, session } => Reply::ok(crate::terminal::list(session.as_deref())),
        Request::TerminalStop { id, close } => answered(if close { crate::terminal::close(&id) } else { crate::terminal::stop(&id) }.map_err(crate::util::words)),
    }
}

/// Holds a hook's permission request until it's answered (claude/asks.rs). No answer comes from here
/// for a session whose process the hook didn't find, one Codealong runs itself (it asks through
/// its adapter), nor for an account config.toml doesn't list.
fn hold_permission_ask(engine: &Handle, hook: &Value, pid: Option<u32>, home: Option<&str>, at: Option<i64>, from: Option<u64>, out: &mut UnixStream) {
    use crate::claude::asks::{note_for, Side};
    let field = |k: &str| hook.get(k).and_then(Value::as_str).unwrap_or("?");
    let let_go = |out: &mut UnixStream, why: &str| {
        note_for(Side::Server, field("session_id"), field("tool_name"), &format!("let go at once: {why}"));
        crate::claude::asks::reply(out, None);
    };
    let cfg = match crate::config::current() {
        Ok(cfg) => cfg,
        Err(e) => return let_go(out, &format!("the settings don't read: {e:#}")),
    };
    let Some((asked, record)) = crate::claude::asks::parse(hook, at.unwrap_or_else(crate::util::now_ms)) else {
        return let_go(out, "not a request Codealong answers");
    };
    let Some(pid) = pid else { return let_go(out, "the hook didn't say its session's process") };
    let Some(account) = home.and_then(|h| crate::claude::asks::account_for(&cfg, std::path::Path::new(h))) else {
        return let_go(out, &format!("{} is no account config.toml lists", home.unwrap_or("its folder")));
    };
    // A side conversation's copy has nobody to ask: Claude Code refuses the call as soon as
    // the hook returns. A session Codealong runs asks through its adapter too, and only that
    // request can be answered: one held here as well showed as waiting where nothing answered it.
    if crate::side::runs(pid) || crate::acp::runs(pid) {
        return let_go(out, "a session Codealong runs itself asks through its own way in");
    }
    let mut watch = crate::claude::asks::Watch::new(account.home_path(), pid, record);
    watch.from = from.unwrap_or(watch.from);
    crate::claude::asks::hold(out, asked, watch, &|| engine.hosted_changed());
}

/// Takes a batch of a reply from the plugin's `MessageDisplay` hook for a session of an account
/// config.toml lists, and tells this Mac's windows what it's writing when that changed
/// (claude/live.rs); a linked device asks for it as it reads the session.
fn message_shown(hook: &Value, home: &str) {
    let Ok(cfg) = crate::config::current() else { return };
    let Some(account) = crate::claude::asks::account_for(&cfg, std::path::Path::new(home)) else { return };
    let Some(id) = hook.get("session_id").and_then(Value::as_str) else { return };
    let key = crate::model::Session::key_for(crate::model::Harness::Claude, &account.name, id);
    crate::claude::live::take(&key, hook, crate::util::now_ms(), &|text| {
        crate::views::emit("hosted-stream", &crate::acp::Stream { key: key.clone(), text, tool: None });
    });
}

/// The bundle id of the app this server's binary is in, for the command to open it by when no app
/// is connected; none for a binary outside a bundle (cargo run).
fn own_bundle() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    let contents = exe.parent()?.parent()?;
    let info: plist::Value = plist::from_file(contents.join("Info.plist")).ok()?;
    info.as_dictionary()?.get("CFBundleIdentifier")?.as_string().map(String::from)
}

/// Holds a command's request to run in your terminal until you answer it (terminal.rs). One that
/// can't be asked for is refused at once: a command that wouldn't show as it is, one from a session
/// the board doesn't have here, one for a folder that isn't there.
fn hold_terminal_ask(engine: &Handle, ask: crate::terminal::Ask, out: &mut UnixStream) {
    let refused = crate::terminal::refused(&ask.command)
        .map(String::from)
        .or_else(|| engine.read(|b| b.session(&ask.session).is_none()).then(|| "Codealong doesn't list the session that asks, so there's nowhere to ask its user.".to_string()))
        .or_else(|| (!ask.cwd.is_absolute() || !ask.cwd.is_dir()).then(|| format!("{} isn't a folder here.", crate::util::tilde(&ask.cwd))));
    if let Some(why) = refused {
        let mut line = serde_json::to_string(&Reply::err(why)).unwrap_or_default();
        line.push('\n');
        let _ = out.write_all(line.as_bytes()).and_then(|_| out.flush());
        return;
    }
    // Only a window of Codealong's asks you: with the app closed it opens to ask.
    if !crate::views::any() {
        if let Some(bundle) = own_bundle() {
            std::thread::spawn(move || {
                if let Err(e) = crate::os::open::in_app(crate::os::open::App::Id(&bundle), None, Duration::from_secs(10)) {
                    eprintln!("Couldn't open Codealong to ask about a command: {e:#}");
                }
            });
        }
    }
    let (id, at) = crate::terminal::new_id();
    let asked = crate::terminal::Asked { id, ask, at };
    crate::terminal::hold(out, asked, &|| engine.hosted_changed());
}

/// Hands the running app a command's request to run in your terminal and waits, however long it
/// takes, for your answer (terminal.rs `reply`). None when no app answered: none is up, or it quit
/// first.
pub fn ask_terminal(ask: &crate::terminal::Ask) -> Option<Result<Value, String>> {
    ask_within(&Request::TerminalAsk { ask: ask.clone() }, Duration::ZERO)
}

fn answered<T: Serialize>(r: std::result::Result<T, String>) -> Reply {
    match r {
        Ok(v) => Reply::ok(v),
        Err(e) => Reply::err(e),
    }
}

/// Your other devices as the command gets them (`devices`): Settings › Devices' view, with how
/// many worktrees and sessions each linked Mac has in place of the worktrees themselves, which
/// are big.
#[derive(Serialize, Deserialize, Debug, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct DevicesSummary {
    pub enabled: bool,
    pub me: MeView,
    pub devices: Vec<DeviceSummary>,
    pub phones: Vec<DeviceView>,
    pub found: Vec<FoundView>,
    pub asks: Vec<AskView>,
}

#[derive(Serialize, Deserialize, Debug, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct DeviceSummary {
    #[serde(flatten)]
    pub device: DeviceView,
    pub worktrees: usize,
    pub sessions: usize,
    /// Its sessions that wait on you, or stopped on an error.
    pub needs: usize,
}

impl DevicesSummary {
    pub fn of(view: DevicesView) -> DevicesSummary {
        let devices = view
            .devices
            .into_iter()
            .map(|device| {
                let prefix = format!("@{}:", device.short);
                let lanes: Vec<_> = view.lanes.iter().filter(|l| l.id.starts_with(&prefix)).collect();
                let sessions = lanes.iter().flat_map(|l| &l.sessions).filter(|s| s.visitor.is_none());
                let needs = sessions.clone().filter(|s| matches!(s.status, crate::model::Status::Needs | crate::model::Status::Error)).count();
                DeviceSummary { worktrees: lanes.len(), sessions: sessions.count(), needs, device }
            })
            .collect();
        DevicesSummary { enabled: view.enabled, me: view.me, devices, phones: view.phones, found: view.found, asks: view.asks }
    }
}

/// Hands the running app a hook's permission request and waits, however long it takes, for the
/// reply: the decision, or null when it let the request go unanswered or doesn't know the
/// request (an older app). None when no app replied: none is up, or it quit first.
pub fn ask_permission(hook: &Value, pid: u32, home: &std::path::Path, at: i64, from: u64) -> Option<Value> {
    let mut stream = UnixStream::connect(path()).ok()?;
    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
    let req = Request::PermissionAsk { hook: hook.clone(), pid: Some(pid), home: Some(home.to_string_lossy().into_owned()), at: Some(at), from: Some(from) };
    let mut text = serde_json::to_string(&req).ok()?;
    text.push('\n');
    stream.write_all(text.as_bytes()).ok()?;
    stream.flush().ok()?;
    let mut line = String::new();
    BufReader::new(stream).read_line(&mut line).ok()?;
    let reply: Reply = serde_json::from_str(&line).ok()?;
    Some(reply.result.filter(|_| reply.ok).unwrap_or(Value::Null))
}

/// How long the command waits for the app's answer to most requests.
pub const WAIT: Duration = Duration::from_secs(120);

/// Whoever asked on a connection the app holds (a hook's permission request, a command to run in
/// your terminal) has gone: its end is closed, its process having ended.
pub fn hung_up(stream: &mut UnixStream) -> bool {
    if stream.set_nonblocking(true).is_err() {
        return true;
    }
    let mut b = [0u8; 1];
    let gone = match stream.read(&mut b) {
        Ok(0) => true,
        Ok(_) => false,
        Err(e) => e.kind() != std::io::ErrorKind::WouldBlock,
    };
    let _ = stream.set_nonblocking(false);
    gone
}

/// One request to the running app, or None when no app answers on the socket.
pub fn ask(req: &Request) -> Option<Result<Value, String>> {
    ask_within(req, WAIT)
}

/// `ask`, waiting `wait` for the answer, or for as long as it takes with zero.
pub fn ask_within(req: &Request, wait: Duration) -> Option<Result<Value, String>> {
    ask_on(UnixStream::connect(path()).ok()?, req, wait)
}

pub(crate) fn ask_on(mut stream: UnixStream, req: &Request, wait: Duration) -> Option<Result<Value, String>> {
    let _ = stream.set_read_timeout((!wait.is_zero()).then_some(wait));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
    let mut text = serde_json::to_string(req).ok()?;
    text.push('\n');
    stream.write_all(text.as_bytes()).ok()?;
    stream.flush().ok()?;
    let mut line = String::new();
    BufReader::new(stream).read_line(&mut line).ok()?;
    if line.trim().is_empty() {
        return None;
    }
    // read_line also succeeds at EOF with an unfinished line. The newline, not a full
    // buffer or a closed socket, is the end of a reply. Don't parse a truncated response
    // or retry its request: it may already have changed something in the app.
    if !line.ends_with('\n') {
        return Some(Err(format!("The app closed the connection before finishing its reply ({} bytes received).", line.len())));
    }
    let reply: Reply = match serde_json::from_str(&line) {
        Ok(r) => r,
        Err(e) => return Some(Err(format!("The app's reply didn't read: {e}"))),
    };
    Some(if reply.ok { Ok(reply.result.unwrap_or(Value::Null)) } else { Err(reply.error.unwrap_or_else(|| "The app said no.".into())) })
}

/// Whether the app was opened as an app is (Finder, the Dock, Spotlight, `open`, a restart): no
/// arguments, or only the process serial number older macOS passes. main.rs starts the app for
/// any argument that isn't a command too, which is a typo in a terminal.
pub fn opened_plainly(args: &[String]) -> bool {
    args.iter().skip(1).all(|a| a.starts_with("-psn_"))
}

/// The last Codealong opened wins. One already answering on the socket (the one in Applications,
/// or Codealong Dev, a build of this repo) holds the socket, the shortcut and the floating bar, so
/// a copy opened after it asks it to quit and waits for it to go; one from before `quit` gets
/// SIGTERM, as relaunch.sh sends. A copy started with arguments that aren't a command leaves the
/// running one be and says why instead.
pub fn take_over(opened_plainly: bool) -> Result<(), String> {
    let Some(Ok(status)) = ask_within(&Request::Status, Duration::from_secs(3)) else { return Ok(()) };
    let Some(pid) = status.get("pid").and_then(Value::as_u64).map(|p| p as libc::pid_t) else { return Ok(()) };
    if !opened_plainly {
        return Err(format!("Codealong is already running (pid {pid}), and that isn't a codealong command: see codealong --help."));
    }
    if !matches!(ask_within(&Request::Quit, Duration::from_secs(3)), Some(Ok(_))) {
        unsafe { libc::kill(pid, libc::SIGTERM) };
    }
    // Five seconds to quit, then SIGTERM; three more, then SIGKILL: a copy that hangs on quitting
    // would otherwise keep this one from ever opening.
    for tenth in 0..100 {
        if unsafe { libc::kill(pid, 0) } != 0 {
            return Ok(());
        }
        match tenth {
            50 => unsafe { libc::kill(pid, libc::SIGTERM) },
            80 => unsafe { libc::kill(pid, libc::SIGKILL) },
            _ => 0,
        };
        std::thread::sleep(Duration::from_millis(100));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A socket `accept_on` serves in a folder of its own, answering each request with a 1 MB
    /// reply: its path, the listener's descriptor, the inode at its path, and the reply. `before`
    /// is done to the descriptor before the thread that serves it starts.
    fn served(name: &str, before: impl FnOnce(i32)) -> (PathBuf, i32, u64, String) {
        use std::os::fd::AsRawFd;
        use std::os::unix::fs::MetadataExt;

        let dir = std::env::temp_dir().join(format!("codealong-socket-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("t.sock");
        let listener = listen(&p).unwrap();
        let fd = listener.0.as_raw_fd();
        let first = std::fs::metadata(&p).unwrap().ino();
        before(fd);
        let expected = "x".repeat(1024 * 1024);
        let reply = expected.clone();
        let path = p.clone();
        std::thread::spawn(move || {
            accept_on(&path, listener, |stream| {
                let reply = reply.clone();
                std::thread::spawn(move || connection_on(stream, |_, _| Some(Reply::ok(&reply))));
            })
        });
        (p, fd, first, expected)
    }

    /// Whether another socket than the one with inode `first` is at `p` within `wait`.
    fn reopened(p: &Path, first: u64, wait: Duration) -> bool {
        use std::os::unix::fs::MetadataExt;
        let since = std::time::Instant::now();
        while since.elapsed() < wait {
            if std::fs::metadata(p).is_ok_and(|m| m.ino() != first) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        false
    }

    /// What the running app's listener went through (`accept_on`): its descriptor replaced while
    /// accept waits on it. A long reply still comes whole, from the socket opened in its place.
    #[test]
    fn a_listener_replaced_under_its_accept_still_answers_in_full() {
        let (p, fd, first, expected) = served("replaced", |_| {});
        let ours = socket_id(fd);
        // Only a replacement while accept waits drains the socket, and the thread may not be in it
        // yet: tried until it was, and only while the number is still the listener's, since once
        // it's opened again the old one is closed and the number may be another test's.
        let mut again = false;
        for _ in 0..20 {
            if socket_id(fd) != ours {
                break;
            }
            // SAFETY: dup2 puts a copy of the listener in its place in one step, so no other
            // thread's descriptor can take the number meanwhile; the copy is closed after.
            unsafe {
                let copy = libc::dup(fd);
                libc::dup2(copy, fd);
                libc::close(copy);
            }
            // Linux leaves the accept waiting on the same socket, which goes on answering.
            if !crate::os::is_macos() {
                break;
            }
            again = reopened(&p, first, Duration::from_millis(500));
            if again {
                break;
            }
        }
        assert!(again || !crate::os::is_macos(), "a new socket in its place");
        let answer = ask_on(UnixStream::connect(&p).unwrap(), &Request::Board, Duration::from_secs(5));
        assert_eq!(answer, Some(Ok(Value::String(expected))));
        let _ = std::fs::remove_dir_all(p.parent().unwrap());
    }

    /// A listener closed outright frees its number, which the socket opened in its place is likely
    /// to take: it isn't closed with the old one's, and goes on answering. Closed while accept
    /// waits on it, or before its thread starts.
    #[test]
    fn a_listener_closed_under_its_accept_is_opened_again_and_kept() {
        for closed_first in [true, false] {
            // Linux leaves an accept waiting on a closed descriptor's socket, until a command comes.
            if !closed_first && !crate::os::is_macos() {
                continue;
            }
            // SAFETY: closes the listener's descriptor, which is what's tried; `accept_on` mustn't
            // close the number again.
            let close = |fd| unsafe { libc::close(fd) };
            let (p, fd, first, expected) = served(if closed_first { "closed-first" } else { "closed" }, |fd| {
                if closed_first {
                    close(fd);
                }
            });
            if !closed_first {
                std::thread::sleep(Duration::from_millis(200));
                close(fd);
            }
            assert!(reopened(&p, first, Duration::from_secs(5)), "a new socket in its place (closed first: {closed_first})");
            for _ in 0..2 {
                let answer = ask_on(UnixStream::connect(&p).unwrap(), &Request::Board, Duration::from_secs(5));
                assert_eq!(answer, Some(Ok(Value::String(expected.clone()))));
            }
            let _ = std::fs::remove_dir_all(p.parent().unwrap());
        }
    }

    /// A listener whose number another listening socket has taken by the time it's accepted on:
    /// a new socket is opened at its path rather than the other one served, and the other's
    /// number is left to it.
    #[test]
    fn a_listener_whose_number_another_listener_took_is_opened_again() {
        use std::os::fd::AsRawFd;

        let mut other = None;
        let (p, fd, first, expected) = served("taken", |fd| {
            let dir = std::env::temp_dir().join(format!("codealong-socket-taken-{}", std::process::id()));
            let o = UnixListener::bind(dir.join("other.sock")).unwrap();
            // SAFETY: dup2 closes the listener's descriptor and puts a copy of the other listener
            // at its number in one step, so no other thread's descriptor can take it meanwhile.
            unsafe { libc::dup2(o.as_raw_fd(), fd) };
            other = Some(o);
        });
        assert!(reopened(&p, first, Duration::from_secs(5)), "a new socket in its place");
        let answer = ask_on(UnixStream::connect(&p).unwrap(), &Request::Board, Duration::from_secs(5));
        assert_eq!(answer, Some(Ok(Value::String(expected))));
        // The copy at the old number still answers for the other listener: it wasn't closed.
        let other = other.unwrap();
        assert!(socket_id(fd).is_some() && socket_id(fd) == socket_id(other.as_raw_fd()));
        // SAFETY: the copy dup2 made, which `accept_on` gave up.
        unsafe { libc::close(fd) };
        let _ = std::fs::remove_dir_all(p.parent().unwrap());
    }

    #[test]
    fn a_reply_split_across_socket_reads_is_read_through_its_newline() {
        let (mut app, command) = UnixStream::pair().unwrap();
        let expected = "a\nquoted \"reply\" … ".repeat(8192);
        let mut text = serde_json::to_string(&Reply::ok(&expected)).unwrap();
        text.push('\n');
        let server = std::thread::spawn(move || {
            let mut request = String::new();
            BufReader::new(&app).read_line(&mut request).unwrap();
            assert!(matches!(serde_json::from_str::<Request>(&request), Ok(Request::Board)));
            for chunk in text.as_bytes().chunks(8192) {
                app.write_all(chunk).unwrap();
            }
            // The reply ends at the newline, without needing the app to close its end.
            assert_eq!(app.read(&mut [0u8; 1]).unwrap(), 0);
        });
        let reply = ask_on(command, &Request::Board, Duration::from_secs(5));
        assert_eq!(reply, Some(Ok(Value::String(expected))));
        server.join().unwrap();
    }

    #[test]
    fn a_reply_closed_before_its_newline_is_a_transport_error() {
        let mut partial = serde_json::to_string(&Reply::ok("x".repeat(8192))).unwrap();
        partial.truncate(8192);
        for text in [partial, r#"{"ok":true,"result":42}"#.into()] {
            let (mut app, command) = UnixStream::pair().unwrap();
            let length = text.len();
            let server = std::thread::spawn(move || {
                let mut request = String::new();
                BufReader::new(&app).read_line(&mut request).unwrap();
                app.write_all(text.as_bytes()).unwrap();
            });
            let reply = ask_on(command, &Request::Board, Duration::from_secs(5));
            assert_eq!(reply, Some(Err(format!("The app closed the connection before finishing its reply ({length} bytes received)."))));
            server.join().unwrap();
        }
    }

    /// A call from an app of another build is turned away without being run, which that app
    /// takes for leave to send it again to its own server; its own build's, or a caller that
    /// doesn't say, is answered.
    #[test]
    fn a_call_from_another_build_is_turned_away_unrun() {
        let dir = crate::fixtures::scratch("server-call");
        let hooks = crate::peers::Hooks { board: Box::new(crate::model::Board::default), answer: Box::new(|_, _, _| Err("no".into())), emit: Box::new(|_| {}), side: Box::new(|_| {}) };
        let ctx = Ctx { engine: Handle::over(crate::model::Board::default()), peers: crate::peers::Peers::new(dir.to_path_buf(), hooks), reviews: Default::default() };
        let call = |engine: Option<&str>, exe: Option<String>| Request::Call { name: "setup_needed".into(), args: serde_json::json!({}), engine: engine.map(String::from), exe };
        let other = handle_request(&ctx, Ok(call(Some("0000000000000000"), None)));
        assert_eq!(other.error.as_deref(), Some(FOREIGN_CALL));
        let exe = crate::serve::this_binary().map(|p| p.to_string_lossy().into_owned());
        assert!(handle_request(&ctx, Ok(call(Some(crate::serve::ENGINE), exe))).ok);
        assert!(handle_request(&ctx, Ok(call(None, None))).ok);
    }

    #[test]
    fn only_a_plain_open_takes_over() {
        let args = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(opened_plainly(&args(&["/Applications/Codealong.app/Contents/MacOS/codealong"])));
        assert!(opened_plainly(&args(&["codealong", "-psn_0_1234567"])));
        assert!(!opened_plainly(&args(&["codealong", "todos"])));
        let quit: Request = serde_json::from_str(r#"{"method":"quit"}"#).unwrap();
        assert!(matches!(quit, Request::Quit));
    }

    #[test]
    fn the_devices_view_counts_each_macs_worktrees_in_place_of_them() {
        use crate::fixtures::{lane, session};
        use crate::model::{Session, Status};
        let needs = Session { status: Status::Needs, ..session("@9f8e7d6c5b:claude:claude:a") };
        let visiting = Session { visitor: Some("claude:claude:b".into()), ..session("@9f8e7d6c5b:claude:claude:v") };
        let view = DevicesView {
            enabled: true,
            devices: vec![DeviceView { key: "9f8e7d6c5b4a3928".into(), short: "9f8e7d6c5b".into(), name: "Mac mini".into(), ..Default::default() }],
            phones: vec![DeviceView { short: "b7c6d5e4f3".into(), kind: "phone".into(), ..Default::default() }],
            lanes: vec![lane("@9f8e7d6c5b:/a", vec![needs, visiting]), lane("@9f8e7d6c5b:/b", vec![session("@9f8e7d6c5b:codex:codex:c")]), lane("@0000000000:/c", vec![])],
            ..Default::default()
        };
        let summary = DevicesSummary::of(view);
        let d = &summary.devices[0];
        assert_eq!((d.worktrees, d.sessions, d.needs), (2, 2, 1), "a visitor isn't one of its sessions");
        let v = serde_json::to_value(&summary).unwrap();
        assert!(v.get("lanes").is_none() && v["phones"][0].get("worktrees").is_none(), "{v}");
        assert_eq!((v["devices"][0]["name"].as_str(), v["devices"][0]["needs"].as_u64()), (Some("Mac mini"), Some(1)));
    }

    #[test]
    fn requests_round_trip_as_json_lines() {
        let op = TodoOp::Update { id: "t-1".into(), patch: crate::todos::TodoPatch { status: Some(crate::todos::TodoStatus::Done), ..Default::default() } };
        let text = serde_json::to_string(&Request::TodoApply { op }).unwrap();
        assert!(text.contains(r#""method":"todo-apply""#) && text.contains(r#""status":"done""#), "{text}");
        let back: Request = serde_json::from_str(&text).unwrap();
        assert!(matches!(back, Request::TodoApply { op: TodoOp::Update { .. } }));
        let status: Request = serde_json::from_str(r#"{"method":"status"}"#).unwrap();
        assert!(matches!(status, Request::Status));
        let ask: Request = serde_json::from_str(r#"{"method":"device-ask","device":"a1b2c3d4e5","request":"search","params":{"words":["x"]}}"#).unwrap();
        assert!(matches!(ask, Request::DeviceAsk { ref request, .. } if request == "search"));
        let answer = serde_json::to_string(&Request::DeviceAnswer { key: "k".into(), accept: true }).unwrap();
        assert!(answer.contains(r#""method":"device-answer""#), "{answer}");
        let run = serde_json::to_string(&Request::Run { name: "session-send".into(), key: "claude:claude:abc".into(), params: serde_json::json!({ "text": "hi" }), from: None }).unwrap();
        assert_eq!(run, r#"{"method":"run","name":"session-send","key":"claude:claude:abc","params":{"text":"hi"},"from":null}"#);
        let stop: Request = serde_json::from_str(r#"{"method":"run","name":"session-cancel","key":"claude:claude:abc"}"#).unwrap();
        assert!(matches!(stop, Request::Run { ref name, ref params, from: None, .. } if name == "session-cancel" && params.is_null()));
        let show = serde_json::to_string(&Request::Show { reveal: serde_json::json!({"kind": "session", "laneId": "l", "key": "claude:claude:abc"}) }).unwrap();
        assert!(show.contains(r#""method":"show""#) && show.contains(r#""laneId":"l""#), "{show}");
        let shown = serde_json::to_string(&Request::MessageShown { hook: serde_json::json!({ "index": 0 }), home: "/h/.claude".into() }).unwrap();
        assert_eq!(shown, r#"{"method":"message-shown","hook":{"index":0},"home":"/h/.claude"}"#);
        let review = serde_json::to_string(&Request::PrReview { pr: "acme/app#12".into(), account: Some("cursor".into()) }).unwrap();
        assert_eq!(review, r#"{"method":"pr-review","pr":"acme/app#12","account":"cursor"}"#);
        assert!(matches!(serde_json::from_str(r#"{"method":"pr-review","pr":"acme/app#12"}"#).unwrap(), Request::PrReview { account: None, .. }));
        let reply: Reply = serde_json::from_str(r#"{"ok":false,"error":"no"}"#).unwrap();
        assert!(!reply.ok && reply.error.as_deref() == Some("no"));
    }
}
