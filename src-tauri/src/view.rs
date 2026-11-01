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
use std::sync::{Arc, Condvar, Mutex, RwLock};
use std::time::Duration;

use serde::Deserialize;
use serde_json::value::RawValue;
use serde_json::Value;

use crate::model::{Board, Lane, Notice};
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
    let exe = serve::this_binary().map(|p| p.to_string_lossy().into_owned());
    views::Info { pid: std::process::id(), version: env!("CARGO_PKG_VERSION").to_string(), engine: serve::ENGINE.to_string(), bundle, since, exe }
}

/// The last app opened wins, as it always has: one connected to the server is asked to quit, and
/// one from before there were servers, which runs its own engine, quits as it did
/// (`server::take_over`). One opened after this one (`since`), which a slow start can let in
/// first, is left be: it's this one that goes (`follow`). An app opened with arguments that aren't
/// a command leaves the one open be and says why.
pub fn take_over(opened_plainly: bool, since: i64) -> Result<(), String> {
    let Some(status) = serve::running() else { return Ok(()) };
    if !serve::is_server(&status) {
        return server::take_over(opened_plainly);
    }
    // A copy started with arguments that aren't a command (a typo, a debug build run by hand)
    // leaves the server be, app or none: it would otherwise replace it with its own build.
    if !opened_plainly {
        let pid = status.views.last().map_or(status.pid, |v| v.pid);
        return Err(format!("Codealong is already running (pid {pid}), and that isn't a codealong command: see codealong --help."));
    }
    if status.views.is_empty() {
        return Ok(());
    }
    match server::ask_within(&Request::TakeViews { since: Some(since) }, Duration::from_secs(15)) {
        Some(Ok(_)) => Ok(()),
        // A server too busy to answer: the apps it has are ended here instead.
        _ => {
            for view in status.views.iter().filter(|v| v.since < since) {
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
        let mut taken = Taken::default();
        let mut failed = 0u32;
        let mut here = false;
        loop {
            let found = serve::ensure_held(true, Some(me.since));
            // An app opened after this one, of another build, replaced the server: this one, the
            // older, gives way rather than replacing it back.
            if found.as_ref().is_err_and(|e| e == serve::GIVE_WAY) {
                (hooks.quit)();
                return;
            }
            // An app from before there were servers, opened after this one, quits this one as it
            // takes the socket; one that answers here anyway (it opened as this one did) runs its
            // own engine and takes no views, so this one, the last opened, takes over from it.
            if let Some((s, _)) = found.as_ref().ok().filter(|(s, _)| !serve::is_server(s)) {
                eprintln!("An older Codealong (pid {}) answers on the socket; taking over from it.", s.pid);
                if let Err(e) = server::take_over(true) {
                    eprintln!("Couldn't take over from it: {e}");
                }
                std::thread::sleep(Duration::from_millis(500));
                continue;
            }
            let one = match found {
                Ok((_, one)) => one,
                Err(e) => {
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
            };
            failed = 0;
            let listened = listen(&mirror, &me, &hooks, &mut taken, one);
            set_connected(false);
            match listened {
                Ok(()) => eprintln!("Codealong's server closed the connection; connecting again."),
                Err(e) => eprintln!("Lost Codealong's server ({e}); connecting again."),
            }
            std::thread::sleep(Duration::from_millis(300));
        }
    });
}

/// Whether the app is connected to its own server, which a window's command waits for (`call`).
fn connection() -> &'static (Mutex<bool>, Condvar) {
    static CONNECTED: (Mutex<bool>, Condvar) = (Mutex::new(false), Condvar::new());
    &CONNECTED
}

fn set_connected(on: bool) {
    let (held, changed) = connection();
    if let Ok(mut c) = held.lock() {
        *c = on;
        changed.notify_all();
    }
}

/// Waits, at most `within`, for the app to be connected to its own server.
fn wait_connected(within: Duration) -> bool {
    let (held, changed) = connection();
    let Ok(c) = held.lock() else { return false };
    changed.wait_timeout_while(c, within, |on| !*on).is_ok_and(|(on, _)| *on)
}

/// Connects as a view and takes what the server sends until it stops. The server's first board
/// says it took this app as its view; one of another build says so instead (`views::serve`). The
/// start lock (`one`) is let go once it has, so this app is on the server before any other app
/// starting decides whether to replace it.
fn listen(mirror: &Mirror, me: &views::Info, hooks: &Hooks, taken: &mut Taken, one: crate::store::Lock) -> std::io::Result<()> {
    let mut one = Some(one);
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
        match e.event.as_str() {
            "foreign" => return Err(std::io::Error::other("the server is another build's")),
            "board" => {
                set_connected(true);
                one.take();
            }
            _ => {}
        }
        take(mirror, hooks, taken, &e.event, e.payload);
    }
}

/// What the app keeps of what it took: the count it gives each board (`rev`), the server's own
/// notices on the last one, and its own files' problems it said beside them.
#[derive(Default)]
struct Taken {
    rev: u64,
    notices: Vec<Notice>,
    local: Vec<(String, String)>,
}

impl Taken {
    /// The server's notices, and what the app couldn't write of its own files (its windows'
    /// places, the desktop's cards), which the server can't see.
    fn notices(&self) -> Vec<Notice> {
        let mine = self.local.iter().filter(|(file, _)| !self.notices.iter().any(|n| n.key == *file)).map(|(file, why)| crate::engine::store_notice(file, why));
        self.notices.iter().cloned().chain(mine).collect()
    }

    /// Puts `board` in the mirror, counted on from the last, and hands it on.
    fn hand_on(&mut self, mirror: &Mirror, hooks: &Hooks, mut board: Board) {
        self.rev += 1;
        board.rev = self.rev;
        board.notices = self.notices();
        if let Ok(mut held) = mirror.0.write() {
            *held = board;
        }
        if let Ok(held) = mirror.0.read() {
            (hooks.board)(&held);
        }
    }
}

/// The app's own files' problems now (`store::problems`).
fn local_problems() -> Vec<(String, String)> {
    let mut found = crate::store::problems(&crate::config::support_dir());
    found.sort();
    found
}

/// What the app does with one event.
fn take(mirror: &Mirror, hooks: &Hooks, taken: &mut Taken, event: &str, payload: &RawValue) {
    match event {
        // A ping comes every few seconds: one of the app's own files going wrong, or right again,
        // is said then, where an idle server publishes no board to say it with.
        "ping" => {
            let now = local_problems();
            if now != taken.local {
                taken.local = now;
                taken.hand_on(mirror, hooks, mirror.board());
            }
        }
        "board" => {
            // A board follows a change to config.toml within moments: the app reads it then too.
            crate::config::follow_file();
            let board: Board = match serde_json::from_str(payload.get()) {
                Ok(b) => b,
                Err(e) => return eprintln!("The server's board didn't read: {e}"),
            };
            taken.notices = board.notices.clone();
            taken.local = local_problems();
            taken.hand_on(mirror, hooks, board);
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

/// How long a window's command waits for the app to be connected to its own server: one starting
/// as the app opens, or being replaced, is there within seconds, and three starts that fail
/// before the app runs one itself take a minute.
const UP_WAIT: Duration = Duration::from_secs(90);

/// A window's command, answered by the server (calls.rs). One asked while the app isn't connected
/// to its own server (it's starting, being replaced, or another build's that this app replaces)
/// waits until it is, as the windows ask theirs as they open; one that reached a server is never
/// sent again.
pub fn call(name: &str, args: Value) -> Result<Value, String> {
    let until = std::time::Instant::now() + UP_WAIT;
    let req = Request::Call { name: name.to_string(), args, engine: Some(serve::ENGINE.to_string()), exe: serve::this_binary().map(|p| p.to_string_lossy().into_owned()) };
    loop {
        let left = until.saturating_duration_since(std::time::Instant::now());
        if left.is_zero() || !wait_connected(left) {
            return Err("Codealong's server isn't answering; it's being started again. Try again in a moment.".into());
        }
        // Not connected after all (it went since), or another build's, which didn't run it: nothing
        // happened, so it's sent again once this app is on its own server.
        let Ok(stream) = UnixStream::connect(server::path()) else {
            std::thread::sleep(Duration::from_millis(100));
            continue;
        };
        match server::ask_on(stream, &req, CALL_WAIT) {
            Some(Err(e)) if e == server::FOREIGN_CALL => std::thread::sleep(Duration::from_millis(200)),
            answer => return finish(answer),
        }
    }
}

/// What a call that reached the server came to. One whose connection went before an answer may
/// have done something, so it's never sent again.
fn finish(answer: Option<Result<Value, String>>) -> Result<Value, String> {
    match answer {
        Some(answer) => answer,
        None => Err("Codealong's server went before it answered: what you asked may or may not have been done.".into()),
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
        let (mirror, h, mut taken) = (Mirror::new(), hooks(heard.clone()), Taken::default());
        // The server's count, and a new server's starting again, are the app's own count up.
        for server_rev in [40, 41, 1] {
            let board = Board { rev: server_rev, loading: false, ..Default::default() };
            take(&mirror, &h, &mut taken, "board", &raw(&serde_json::to_string(&board).unwrap()));
        }
        assert_eq!(*heard.lock().unwrap(), ["board 1 false", "board 2 false", "board 3 false"]);
        assert_eq!(mirror.board().rev, 3);
    }

    #[test]
    fn what_the_windows_hear_goes_to_them_as_it_came() {
        let heard = Arc::new(Mutex::new(Vec::new()));
        let (mirror, h, mut taken) = (Mirror::new(), hooks(heard.clone()), Taken::default());
        take(&mirror, &h, &mut taken, "devices", &raw(r#"{"enabled":true,"lanes":[]}"#));
        take(&mirror, &h, &mut taken, "show", &raw(r#"{"kind":"session","key":"k"}"#));
        take(&mirror, &h, &mut taken, "hosted-stream", &raw(r#"{"key":"k","text":"hi"}"#));
        take(&mirror, &h, &mut taken, "ping", &raw("null"));
        assert_eq!(*heard.lock().unwrap(), ["devices 0", r#"show "k""#, r#"hosted-stream {"key":"k","text":"hi"}"#]);
    }

    /// A window's command waits for the app to be connected to its server, and goes once it is.
    #[test]
    fn a_call_waits_for_the_app_to_be_connected() {
        set_connected(false);
        assert!(!wait_connected(Duration::from_millis(20)));
        let connecting = std::thread::spawn(|| {
            std::thread::sleep(Duration::from_millis(50));
            set_connected(true);
        });
        assert!(wait_connected(Duration::from_secs(5)));
        connecting.join().unwrap();
        set_connected(false);
    }

    /// The app's own files' problems go beside the server's notices, but not twice for one file.
    #[test]
    fn the_apps_own_problems_go_beside_the_servers_notices() {
        let server = crate::engine::store_notice("todos.json", "todos.json doesn't read.");
        let taken = Taken { rev: 0, notices: vec![server.clone()], local: vec![("desktop.json".into(), "Couldn't write desktop.json.".into()), ("todos.json".into(), "here too".into())] };
        let keys: Vec<String> = taken.notices().into_iter().map(|n| n.key).collect();
        assert_eq!(keys, ["todos.json", "desktop.json"]);
    }

    #[test]
    fn the_mirror_starts_loading_in_the_settings_theme() {
        let b = Mirror::new().board();
        assert!(b.loading && b.rev == 0);
    }
}
