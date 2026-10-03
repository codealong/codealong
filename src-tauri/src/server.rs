//! The app's socket for the `codealong` command (cli.rs): JSON lines over a Unix socket next to
//! the config, one request per line and one reply per line. A request is what a window's Tauri
//! command does, through the same engine handle, so the engine stays the one writer of
//! todos.json and a reply is the board the windows have. The socket is mode 0600: it answers
//! the user running the app, and only them.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::support_dir;
use crate::engine::Handle;
use crate::peers::{AskView, DeviceView, DevicesView, FoundView, MeView, Peers};
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
    /// Quits, for another copy of Codealong that's opening: the last one opened wins
    /// (`take_over`). The reply is this app's pid.
    Quit,
    /// The board as the windows have it.
    Board,
    /// A change to a todo or the labels; the reply is the time of the board that shows it.
    TodoApply { op: TodoOp },
    /// Starts a session, and links it to its todos.
    Start { spec: StartSpec },
    /// Sessions whose prompts and replies have every word (search.rs).
    Search { words: Vec<String> },
    /// What closing idle terminals would close (idle.rs).
    IdleTerminals { after_ms: i64 },
    /// Closes the terminals of these sessions, each checked again first.
    CloseIdle { keys: Vec<String>, after_ms: i64 },
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
}

/// How a reply starts when the app doesn't know the request: it's older than the command.
pub const UNKNOWN_REQUEST: &str = "That isn't a request the app knows";

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

/// What `status` says about the app.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub version: String,
    pub pid: u32,
    /// The board's time, and whether the first scan is still running.
    pub generated_at: i64,
    pub loading: bool,
}

/// Listens on the socket for as long as the app runs. Errors are printed, not fatal: the
/// command falls back to the files when there's no socket.
pub fn serve(engine: Handle, peers: Peers, app: tauri::AppHandle) {
    let p = path();
    let _ = std::fs::create_dir_all(support_dir());
    // A socket file left by an app that didn't exit cleanly; a live one would have answered.
    if p.exists() && UnixStream::connect(&p).is_err() {
        let _ = std::fs::remove_file(&p);
    }
    let listener = match UnixListener::bind(&p) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Couldn't open {}: {e}. The codealong command will read the files instead.", p.display());
            return;
        }
    };
    let _ = std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600));
    std::thread::Builder::new()
        .name("socket".into())
        .spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                let (engine, peers, app) = (engine.clone(), peers.clone(), app.clone());
                let _ = std::thread::Builder::new().name("socket-client".into()).spawn(move || connection(stream, &engine, &peers, &app));
            }
        })
        .ok();
}

fn connection(stream: UnixStream, engine: &Handle, peers: &Peers, app: &tauri::AppHandle) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(60)));
    let mut out = match stream.try_clone() {
        Ok(s) => s,
        Err(_) => return,
    };
    let reader = BufReader::new(stream);
    for line in reader.lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let req = serde_json::from_str::<Request>(&line);
        if let Ok(Request::PermissionAsk { hook, pid, home, at, from }) = req {
            hold_permission_ask(engine, &hook, pid, home.as_deref(), at, from, &mut out);
            break;
        }
        let reply = handle_request(engine, peers, app, req);
        let mut text = serde_json::to_string(&reply).unwrap_or_else(|_| r#"{"ok":false,"error":"Couldn't encode the reply."}"#.into());
        text.push('\n');
        if out.write_all(text.as_bytes()).and_then(|_| out.flush()).is_err() {
            break;
        }
    }
}

fn handle_request(engine: &Handle, peers: &Peers, app: &tauri::AppHandle, req: serde_json::Result<Request>) -> Reply {
    let req = match req {
        Ok(r) => r,
        Err(e) => return Reply::err(format!("{UNKNOWN_REQUEST}: {e}")),
    };
    match req {
        Request::Status => {
            let b = engine.board();
            Reply::ok(Status { version: env!("CARGO_PKG_VERSION").to_string(), pid: std::process::id(), generated_at: b.generated_at, loading: b.loading })
        }
        Request::Board => Reply::ok(engine.board()),
        Request::Quit => {
            // After the reply is on its way.
            std::thread::spawn(|| {
                std::thread::sleep(Duration::from_millis(200));
                std::process::exit(0);
            });
            Reply::ok(std::process::id())
        }
        Request::TodoApply { op } => match engine.todo_within(op, Duration::from_secs(90)) {
            Ok(at) => Reply::ok(at),
            Err(e) => Reply::err(e),
        },
        Request::Search { words } => Reply::ok(crate::search::find(&words, 1000)),
        Request::IdleTerminals { after_ms } => Reply::ok(crate::idle::plan_now(&engine.board(), after_ms)),
        Request::CloseIdle { keys, after_ms } => Reply::ok(crate::idle::close(&engine.board(), &keys, after_ms)),
        Request::Start { spec } => answered(crate::start::start_here(engine, &spec).map_err(crate::util::words)),
        Request::Devices => Reply::ok(DevicesSummary::of(peers.view())),
        Request::DeviceLink { key } => answered(peers.link(&key)),
        Request::DeviceAnswer { key, accept } => answered(peers.answer_link(&key, accept)),
        Request::DeviceUnlink { key } => answered(peers.unlink(&key)),
        Request::DeviceCode => answered(peers.link_code()),
        Request::DeviceUseCode { code } => answered(peers.link_with_code(&code)),
        Request::DeviceAsk { device, request, params } => answered(crate::actions::command_may_ask(&request, &params).and_then(|_| peers.ask(&device, &request, params, Duration::from_secs(40)))),
        Request::Run { name, key, params, from } => answered(crate::actions::run_as(engine, peers, &crate::actions::From::Command(from), &name, crate::actions::On::Key(&key), params)),
        Request::Show { reveal } => match crate::front::show_main(app, Some(reveal)) {
            // A bare binary isn't what its ID opens: that's whichever bundle has it.
            Ok(()) => Reply::ok(std::env::current_exe().ok().and_then(|exe| crate::updates::bundle_of(&exe)).map(|_| app.config().identifier.clone())),
            Err(e) => Reply::err(crate::util::words(e)),
        },
        // Held on its connection instead (`connection`).
        Request::PermissionAsk { .. } => Reply::ok(Value::Null),
    }
}

/// Holds a hook's permission request until it's answered (claude/asks.rs). No answer comes from here
/// for a session whose process the hook didn't find (one Codealong runs itself has none: it asks
/// through its adapter), nor for an account config.toml doesn't list.
fn hold_permission_ask(engine: &Handle, hook: &Value, pid: Option<u32>, home: Option<&str>, at: Option<i64>, from: Option<u64>, out: &mut UnixStream) {
    let Ok(cfg) = crate::config::current() else {
        crate::claude::asks::reply(out, None);
        return;
    };
    let found = crate::claude::asks::parse(hook, at.unwrap_or_else(crate::util::now_ms)).zip(pid).zip(home.and_then(|h| crate::claude::asks::account_for(&cfg, std::path::Path::new(h))));
    let Some((((asked, record), pid), account)) = found else {
        crate::claude::asks::reply(out, None);
        return;
    };
    // A side conversation's copy has nobody to ask: Claude Code refuses the call as soon as
    // the hook returns.
    if crate::side::runs(pid) {
        crate::claude::asks::reply(out, None);
        return;
    }
    let mut watch = crate::claude::asks::Watch::new(account.home_path(), pid, record);
    watch.from = from.unwrap_or(watch.from);
    crate::claude::asks::hold(out, asked, watch, &|| engine.hosted_changed());
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

/// One request to the running app, or None when no app answers on the socket.
pub fn ask(req: &Request) -> Option<Result<Value, String>> {
    ask_within(req, Duration::from_secs(120))
}

fn ask_within(req: &Request, wait: Duration) -> Option<Result<Value, String>> {
    let mut stream = UnixStream::connect(path()).ok()?;
    let _ = stream.set_read_timeout(Some(wait));
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
        let reply: Reply = serde_json::from_str(r#"{"ok":false,"error":"no"}"#).unwrap();
        assert!(!reply.ok && reply.error.as_deref() == Some("no"));
    }
}
