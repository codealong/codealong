//! The app's side of Codealong's server (serve.rs): the app is a view of it. As it opens it takes
//! the place of an app already open (`take_over`), finds the server or starts one, replacing one
//! built with another engine (`serve::ensure`), and connects as a view (`follow`), which it does
//! again whenever the server goes (relaunched for a new engine, crashed, stopped): the windows go
//! on showing the last board meanwhile. What the server publishes it keeps a copy of (`Mirror`),
//! which the app's own commands read (the floating bar's feed, the peek, the Dock icon), and hands
//! on to the windows as they've always heard it. A window's command that reads or changes what the
//! server holds goes to it (`call`, lib.rs `forward`).
//!
//! The board's `rev` is the app's own, counted up with every board it takes, so a window's board
//! is never taken for newer than a new server's, whose count starts again.
//!
//! A server that can't be started as a job of its own (launchd turning it away) is run in the app
//! instead, as the engine always was, and goes when the app does.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use serde::Deserialize;
use serde_json::value::RawValue;
use serde_json::Value;

use crate::model::{Board, Lane};
use crate::server::{self, Request};
use crate::{serve, views};

/// How long a window's command may wait on the server: a worktree's teardown or a review's start
/// takes minutes at most.
const CALL_WAIT: Duration = Duration::from_secs(600);

/// A server that sends nothing for this long, not even a ping, is taken to be gone.
const SILENT: Duration = Duration::from_secs(20);

/// How many starts in a row may fail before the server is run in the app.
const STARTS: u32 = 3;

/// The board as the server last published it.
#[derive(Clone)]
pub struct Mirror(Arc<RwLock<Board>>);

impl Mirror {
    pub fn new() -> Mirror {
        Mirror(Arc::new(RwLock::new(crate::engine::starting_board())))
    }

    pub fn board(&self) -> Board {
        self.0.read().map(|b| b.clone()).unwrap_or_default()
    }

    pub fn read<T>(&self, f: impl FnOnce(&Board) -> T) -> T {
        match self.0.read() {
            Ok(b) => f(&b),
            Err(_) => f(&Board::default()),
        }
    }
}

/// What the app does with what the server tells it.
pub struct Hooks {
    /// A board, once it's in the mirror.
    pub board: Box<dyn Fn(&Board) + Send>,
    /// Your other devices, as Settings › Devices and the views show them, with their worktrees.
    pub devices: Box<dyn Fn(&[Lane], Value) + Send>,
    /// A session, worktree, PR or todo to show in the main window.
    pub show: Box<dyn Fn(Value) + Send>,
    /// A question for the windows (a board to work out), answered on a thread of its own.
    pub ask: Arc<dyn Fn(&str, Value) -> Result<Value, String> + Send + Sync>,
    /// Anything else, for the windows as it came.
    pub event: Box<dyn Fn(&str, Value) + Send>,
    /// Another app opened, or the server was stopped: this one quits.
    pub quit: Box<dyn Fn() + Send>,
}

/// One line from the server.
#[derive(Deserialize)]
struct Event<'a> {
    event: String,
    #[serde(borrow)]
    payload: &'a RawValue,
}

/// What this app says of itself as it connects. `bundle` is its bundle's id when it runs from one,
/// which the `codealong` command brings it forward by; `since` when it opened.
pub fn info(bundle: Option<String>, since: i64) -> views::Info {
    views::Info { pid: std::process::id(), version: env!("CARGO_PKG_VERSION").to_string(), engine: serve::ENGINE.to_string(), bundle, since }
}

/// The last app opened wins, as it always has: one connected to the server is asked to quit, and
/// one from before there were servers, which runs its own engine, quits as it did
/// (`server::take_over`). An app opened with arguments that aren't a command leaves the one open
/// be and says why.
pub fn take_over(opened_plainly: bool) -> Result<(), String> {
    let Some(status) = serve::running() else { return Ok(()) };
    if !serve::is_server(&status) {
        return server::take_over(opened_plainly);
    }
    let Some(open) = status.views.last() else { return Ok(()) };
    if !opened_plainly {
        return Err(format!("Codealong is already running (pid {}), and that isn't a codealong command: see codealong --help.", open.pid));
    }
    match server::ask_within(&Request::TakeViews, Duration::from_secs(15)) {
        Some(Ok(_)) => Ok(()),
        // A server too busy to answer: the apps it has are ended here instead.
        _ => {
            for view in &status.views {
                // SAFETY: kill only signals a process; one gone by now is refused.
                unsafe { libc::kill(view.pid as libc::pid_t, libc::SIGTERM) };
            }
            Ok(())
        }
    }
}

/// Follows the server for as long as the app runs, on a thread of its own: starting it when none
/// answers, and connecting again whenever it goes.
pub fn follow(mirror: Mirror, me: views::Info, hooks: Hooks) {
    let _ = std::thread::Builder::new().name("server".into()).spawn(move || {
        let mut rev = 0u64;
        let mut failed = 0u32;
        let mut here = false;
        loop {
            // An app opened after this one, of another build, may have replaced the server: this
            // one, the older, gives way rather than replacing it back.
            if serve::running().is_some_and(|s| serve::is_server(&s) && s.views.iter().any(|v| v.pid != me.pid && v.since > me.since)) {
                (hooks.quit)();
                return;
            }
            let found = serve::ensure(true);
            // An app from before there were servers, opened after this one, quits this one as it
            // takes the socket; one that answers here anyway (it opened as this one did) runs its
            // own engine and takes no views, so this one, the last opened, takes over from it.
            if let Some(s) = found.as_ref().ok().filter(|s| !serve::is_server(s)) {
                eprintln!("An older Codealong (pid {}) answers on the socket; taking over from it.", s.pid);
                if let Err(e) = server::take_over(true) {
                    eprintln!("Couldn't take over from it: {e}");
                }
                std::thread::sleep(Duration::from_millis(500));
                continue;
            }
            if let Err(e) = found {
                failed += 1;
                eprintln!("Couldn't start Codealong's server ({failed}): {e}");
                // Tried once: a start that failed partway may have left some of it running.
                if failed >= STARTS && !here {
                    here = true;
                    eprintln!("Running Codealong's server in the app instead: it goes when the app does.");
                    if let Err(e) = serve::start() {
                        eprintln!("Couldn't run Codealong's server in the app either: {e}");
                    }
                }
                std::thread::sleep(Duration::from_secs(2));
                continue;
            }
            failed = 0;
            match listen(&mirror, &me, &hooks, &mut rev) {
                Ok(()) => eprintln!("Codealong's server closed the connection; connecting again."),
                Err(e) => eprintln!("Lost Codealong's server ({e}); connecting again."),
            }
            std::thread::sleep(Duration::from_millis(300));
        }
    });
}

/// Connects as a view and takes what the server sends until it stops.
fn listen(mirror: &Mirror, me: &views::Info, hooks: &Hooks, rev: &mut u64) -> std::io::Result<()> {
    let mut stream = UnixStream::connect(server::path())?;
    stream.set_read_timeout(Some(SILENT))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;
    let mut text = serde_json::to_string(&Request::Subscribe { view: me.clone() }).map_err(std::io::Error::other)?;
    text.push('\n');
    stream.write_all(text.as_bytes())?;
    stream.flush()?;
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            return Ok(());
        }
        let Ok(e) = serde_json::from_str::<Event>(&line) else {
            eprintln!("A line from Codealong's server didn't read: {}", crate::util::truncate(&line, 120));
            continue;
        };
        take(mirror, hooks, rev, &e.event, e.payload);
    }
}

/// What the app does with one event.
fn take(mirror: &Mirror, hooks: &Hooks, rev: &mut u64, event: &str, payload: &RawValue) {
    match event {
        "ping" => {}
        "board" => {
            let mut board: Board = match serde_json::from_str(payload.get()) {
                Ok(b) => b,
                Err(e) => return eprintln!("The server's board didn't read: {e}"),
            };
            *rev += 1;
            board.rev = *rev;
            // What the app couldn't write of its own files (its windows' places, the desktop's
            // cards) is said beside the server's, which it can't see.
            for (file, why) in crate::store::problems(&crate::config::support_dir()) {
                if !board.notices.iter().any(|n| n.key == file) {
                    board.notices.push(crate::engine::store_notice(&file, &why));
                }
            }
            if let Ok(mut held) = mirror.0.write() {
                *held = board;
            }
            if let Ok(held) = mirror.0.read() {
                (hooks.board)(&held);
            }
        }
        "devices" => {
            #[derive(Deserialize, Default)]
            #[serde(default)]
            struct Lanes {
                lanes: Vec<Lane>,
            }
            let lanes: Lanes = serde_json::from_str(payload.get()).unwrap_or_default();
            if let Ok(view) = serde_json::from_str(payload.get()) {
                (hooks.devices)(&lanes.lanes, view);
            }
        }
        "show" => {
            if let Ok(reveal) = serde_json::from_str(payload.get()) {
                (hooks.show)(reveal);
            }
        }
        "ask" => {
            #[derive(Deserialize)]
            struct Ask {
                id: u64,
                ask: String,
                payload: Value,
            }
            let Ok(a) = serde_json::from_str::<Ask>(payload.get()) else { return };
            let answer = hooks.ask.clone();
            std::thread::spawn(move || {
                let got = answer(&a.ask, a.payload);
                let (result, error) = match got {
                    Ok(v) => (v, None),
                    Err(e) => (Value::Null, Some(e)),
                };
                let _ = server::ask_within(&Request::ViewAnswer { id: a.id, result, error }, Duration::from_secs(10));
            });
        }
        "quit" => (hooks.quit)(),
        other => {
            if let Ok(v) = serde_json::from_str(payload.get()) {
                (hooks.event)(other, v);
            }
        }
    }
}

/// How long a window's command waits for a server to come up: one starting as the app opens, or
/// being replaced, answers within seconds.
const UP_WAIT: Duration = Duration::from_secs(30);

/// A window's command, answered by the server (calls.rs). One asked while the server is starting
/// or being replaced waits for it, as the windows ask theirs as they open.
pub fn call(name: &str, args: Value) -> Result<Value, String> {
    match server::ask_once_up(&Request::Call { name: name.to_string(), args }, CALL_WAIT, UP_WAIT) {
        Some(answer) => answer,
        None => Err("Codealong's server isn't answering; it's being started again. Try again in a moment.".into()),
    }
}

/// `call`, its answer read as `T`.
pub fn call_as<T: serde::de::DeserializeOwned>(name: &str, args: Value) -> Result<T, String> {
    serde_json::from_value(call(name, args)?).map_err(|e| format!("The server's answer to {name} didn't read: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    fn hooks(heard: Arc<Mutex<Vec<String>>>) -> Hooks {
        let (b, d, s, e) = (heard.clone(), heard.clone(), heard.clone(), heard.clone());
        Hooks {
            board: Box::new(move |board| b.lock().unwrap().push(format!("board {} {}", board.rev, board.loading))),
            devices: Box::new(move |lanes, _| d.lock().unwrap().push(format!("devices {}", lanes.len()))),
            show: Box::new(move |r| s.lock().unwrap().push(format!("show {}", r["key"]))),
            ask: Arc::new(|_, _| Ok(Value::Null)),
            event: Box::new(move |name, v| e.lock().unwrap().push(format!("{name} {v}"))),
            quit: Box::new(|| {}),
        }
    }

    fn raw(text: &str) -> Box<RawValue> {
        RawValue::from_string(text.to_string()).unwrap()
    }

    #[test]
    fn each_board_taken_counts_on_from_the_last_whatever_the_server_counts() {
        let heard = Arc::new(Mutex::new(Vec::new()));
        let (mirror, h, mut rev) = (Mirror::new(), hooks(heard.clone()), 0);
        // The server's count, and a new server's starting again, are the app's own count up.
        for server_rev in [40, 41, 1] {
            let board = Board { rev: server_rev, loading: false, ..Default::default() };
            take(&mirror, &h, &mut rev, "board", &raw(&serde_json::to_string(&board).unwrap()));
        }
        assert_eq!(*heard.lock().unwrap(), ["board 1 false", "board 2 false", "board 3 false"]);
        assert_eq!(mirror.board().rev, 3);
    }

    #[test]
    fn what_the_windows_hear_goes_to_them_as_it_came() {
        let heard = Arc::new(Mutex::new(Vec::new()));
        let (mirror, h, mut rev) = (Mirror::new(), hooks(heard.clone()), 0);
        take(&mirror, &h, &mut rev, "devices", &raw(r#"{"enabled":true,"lanes":[]}"#));
        take(&mirror, &h, &mut rev, "show", &raw(r#"{"kind":"session","key":"k"}"#));
        take(&mirror, &h, &mut rev, "hosted-stream", &raw(r#"{"key":"k","text":"hi"}"#));
        take(&mirror, &h, &mut rev, "ping", &raw("null"));
        assert_eq!(*heard.lock().unwrap(), ["devices 0", r#"show "k""#, r#"hosted-stream {"key":"k","text":"hi"}"#]);
    }

    #[test]
    fn the_mirror_starts_loading_in_the_settings_theme() {
        let b = Mirror::new().board();
        assert!(b.loading && b.rev == 0);
    }
}
