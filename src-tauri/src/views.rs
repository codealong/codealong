//! The apps connected to the server (serve.rs), each a view of it: what each says of itself as it
//! connects (`Info`), everything the server tells them as it happens (`emit`: the board as the
//! engine publishes it, a reply as it streams, your other devices), and what it asks of the one
//! that connected last, which has the windows: to show a session (`show`), or to work out a board
//! (`ask`), which the main window does. A view that falls behind or goes is let go; it connects
//! again and starts from the board as it is then.
//!
//! Each view has a connection of its own to the socket (server.rs `Request::Subscribe`), which
//! only the server writes to: one JSON line an event, `{"event": …, "payload": …}`, with a ping
//! every few seconds so either end knows the other is still there. A view answers an ask with a
//! request of its own (`Request::ViewAnswer`).

use std::collections::HashMap;
use std::io::Write;
use std::os::unix::net::UnixStream;
use std::sync::mpsc::{self, RecvTimeoutError, SyncSender};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// What a view says of itself as it connects.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Info {
    pub pid: u32,
    pub version: String,
    /// The engine it was built with (`serve::ENGINE`): one built with another wants a server of
    /// its own.
    pub engine: String,
    /// Its app's bundle id, which the `codealong` command brings it forward by.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bundle: Option<String>,
    /// When its app opened, in ms since the epoch: of two apps connected, the one opened last
    /// stays (`serve`).
    pub since: i64,
    /// The binary it runs, which with `engine` says whether it's this server's build.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exe: Option<String>,
}

/// Whether a view is of another build than this server's: another app replaced its server with
/// this one meanwhile. One that doesn't say its engine (the tests') is taken as it comes.
fn foreign(info: &Info) -> bool {
    !info.engine.is_empty() && (info.engine != crate::serve::ENGINE || info.exe.as_deref().map(std::path::PathBuf::from) != crate::serve::this_binary())
}

/// How many events wait for a view before it's taken to have fallen behind and let go: a board is
/// a couple of megabytes, so few.
const QUEUE: usize = 24;

/// How often a view with nothing else to hear is sent a ping.
pub const PING: Duration = Duration::from_secs(5);

/// How long a write to a view may take before it's taken to be gone.
const WRITE_WAIT: Duration = Duration::from_secs(10);

/// How long a session asked to be shown while no view was connected waits for one.
const SHOW_WAIT: Duration = Duration::from_secs(60);

/// What a request that needs a window says when no app is connected.
pub const NO_VIEW: &str = "Codealong's app isn't open";

struct View {
    id: u64,
    info: Info,
    tx: SyncSender<Arc<str>>,
}

#[derive(Default)]
struct Views {
    next: u64,
    views: Vec<View>,
    /// Asks waiting for a view's answer, by id.
    asks: HashMap<u64, mpsc::Sender<Result<Value, String>>>,
    /// A session asked to be shown while no view was connected, for the next one to show.
    show: Option<(Instant, Value)>,
}

fn views() -> &'static Mutex<Views> {
    static VIEWS: OnceLock<Mutex<Views>> = OnceLock::new();
    VIEWS.get_or_init(Default::default)
}

/// An event as the line a view reads.
pub fn line(event: &str, payload: &impl Serialize) -> Option<Arc<str>> {
    #[derive(Serialize)]
    struct Line<'a, T> {
        event: &'a str,
        payload: &'a T,
    }
    let mut text = serde_json::to_string(&Line { event, payload }).ok()?;
    text.push('\n');
    Some(text.into())
}

/// Tells every view `event`. Nothing is written out while none is connected, so the board costs
/// nothing to publish then.
pub fn emit(event: &str, payload: &impl Serialize) {
    let Ok(mut v) = views().lock() else { return };
    if v.views.is_empty() {
        return;
    }
    let Some(line) = line(event, payload) else { return };
    // A view whose queue is full has fallen behind: it's let go, and reads the board whole as it
    // connects again.
    v.views.retain(|view| match view.tx.try_send(line.clone()) {
        Ok(()) => true,
        Err(e) => {
            eprintln!("Let a view go (pid {}): {}", view.info.pid, if matches!(e, mpsc::TrySendError::Full(_)) { "it fell behind" } else { "it's gone" });
            false
        }
    });
}

/// Whether any app is connected.
pub fn any() -> bool {
    views().lock().is_ok_and(|v| !v.views.is_empty())
}

/// The apps connected, the latest last. One whose process has gone is left out: it's let go only
/// at the next write to it, a ping away at most.
pub fn list() -> Vec<Info> {
    // SAFETY: kill with no signal only asks whether the process is there.
    let alive = |pid: u32| unsafe { libc::kill(pid as libc::pid_t, 0) } == 0;
    views().lock().map(|v| v.views.iter().filter(|view| alive(view.info.pid)).map(|view| view.info.clone()).collect()).unwrap_or_default()
}

/// Serves a view on its connection until it goes: `first` (the board as it is, and the rest of
/// what a view starts from), then whatever is emitted. `first` is worked out as the view is taken
/// on, so nothing emitted meanwhile comes before it.
pub fn serve(mut stream: UnixStream, info: Info, first: impl FnOnce() -> Vec<Arc<str>>) {
    // An app of another build is told so, and starts its own server, or gives way to the newer
    // app this one is (view.rs `follow`), rather than showing what it doesn't know.
    if foreign(&info) {
        if let Some(line) = line("foreign", &Value::Null) {
            let _ = stream.write_all(line.as_bytes());
        }
        return;
    }
    let (tx, rx) = mpsc::sync_channel(QUEUE);
    let id = {
        let Ok(mut v) = views().lock() else { return };
        for line in first() {
            let _ = tx.try_send(line);
        }
        // One app at a time, the one opened last, as an app opening asks the others to quit
        // (view.rs `take_over`): two opened at once both found none to ask. The same app
        // connecting again (its server restarted) is neither.
        let other = |view: &&View| view.info.pid != info.pid;
        if v.views.iter().filter(other).any(|view| view.info.since > info.since) {
            if let Some(line) = line("quit", &Value::Null) {
                let _ = tx.try_send(line);
            }
        } else if let Some(quit) = line("quit", &Value::Null) {
            for view in v.views.iter().filter(other).filter(|view| view.info.since < info.since) {
                eprintln!("An app opened after it: telling the one before (pid {}) to quit.", view.info.pid);
                let _ = view.tx.try_send(quit.clone());
            }
        }
        if let Some((_, reveal)) = v.show.take().filter(|(at, _)| at.elapsed() < SHOW_WAIT) {
            if let Some(line) = line("show", &reveal) {
                let _ = tx.try_send(line);
            }
        }
        let id = v.next;
        v.next += 1;
        eprintln!("A view connected: pid {}, {} ({})", info.pid, info.version, info.engine);
        v.views.push(View { id, info, tx });
        id
    };
    let ping = line("ping", &Value::Null).unwrap_or_else(|| "\n".into());
    let _ = stream.set_write_timeout(Some(WRITE_WAIT));
    loop {
        let line = match rx.recv_timeout(PING) {
            Ok(line) => line,
            Err(RecvTimeoutError::Timeout) => ping.clone(),
            Err(RecvTimeoutError::Disconnected) => break,
        };
        if stream.write_all(line.as_bytes()).and_then(|_| stream.flush()).is_err() {
            break;
        }
    }
    if let Ok(mut v) = views().lock() {
        v.views.retain(|view| view.id != id);
    }
}

/// Shows a session, worktree, PR or todo in the latest view's main window (`Reveal` in api.ts).
/// The bundle id of that view's app, for the command to bring it forward by; None when no view is
/// connected, when the next one to connect within a minute shows it.
pub fn show(reveal: Value) -> Option<Option<String>> {
    let mut v = views().lock().ok()?;
    let line = line("show", &reveal)?;
    match v.views.last() {
        Some(view) => {
            let bundle = view.info.bundle.clone();
            let _ = view.tx.try_send(line);
            Some(bundle)
        }
        None => {
            v.show = Some((Instant::now(), reveal));
            None
        }
    }
}

/// Asks the latest view `what` and waits, at most `wait`, for its answer.
pub fn ask(what: &str, payload: Value, wait: Duration) -> Result<Value, String> {
    let (tx, rx) = mpsc::channel();
    let id = {
        let mut v = views().lock().map_err(|_| "Codealong's views are stuck.".to_string())?;
        let id = v.next;
        v.next += 1;
        let line = line("ask", &json!({ "id": id, "ask": what, "payload": payload })).ok_or("Couldn't write the question out.")?;
        let view = v.views.last().ok_or_else(|| format!("{NO_VIEW}, and its main window works that out."))?;
        view.tx.try_send(line).map_err(|_| "Codealong's app isn't keeping up; ask again in a moment.".to_string())?;
        v.asks.insert(id, tx);
        id
    };
    let got = rx.recv_timeout(wait);
    if let Ok(mut v) = views().lock() {
        v.asks.remove(&id);
    }
    got.unwrap_or_else(|_| Err("Codealong's app didn't answer in time. Ask again once its main window is open.".into()))
}

/// A view's answer to ask `id`; one nobody waits for any more is dropped.
pub fn answer(id: u64, answer: Result<Value, String>) {
    if let Some(tx) = views().lock().ok().and_then(|mut v| v.asks.remove(&id)) {
        let _ = tx.send(answer);
    }
}

/// Tells every view whose app opened before `before` (every one, with none) to quit, as another app
/// takes their place or the server stops, and waits, at most `wait`, for them to go; one still
/// there then is ended.
pub fn quit_all(wait: Duration, before: Option<i64>) {
    let older = |i: &Info| before.is_none_or(|b| i.since < b);
    let pids: Vec<u32> = list().iter().filter(|i| older(i)).map(|i| i.pid).collect();
    if pids.is_empty() {
        return;
    }
    if let (Ok(v), Some(quit)) = (views().lock(), line("quit", &Value::Null)) {
        for view in v.views.iter().filter(|view| pids.contains(&view.info.pid)) {
            let _ = view.tx.try_send(quit.clone());
        }
    }
    // SAFETY: kill with no signal only asks whether the process is there.
    let alive = |pid: u32| pid != std::process::id() && unsafe { libc::kill(pid as libc::pid_t, 0) } == 0;
    let until = Instant::now() + wait;
    while pids.iter().any(|p| alive(*p)) && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(100));
    }
    for pid in pids.iter().copied().filter(|p| alive(*p)) {
        eprintln!("A view (pid {pid}) didn't quit when asked; ending it.");
        // SAFETY: as above, with the signal that ends it.
        unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM) };
    }
    // Gone, they'd be let go only at the next ping.
    if let Ok(mut v) = views().lock() {
        v.views.retain(|view| !pids.contains(&view.info.pid));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};

    /// Views are one set for the process, so the tests that connect them take turns.
    fn turn() -> std::sync::MutexGuard<'static, ()> {
        static TURN: Mutex<()> = Mutex::new(());
        TURN.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// A view connected, once the server holds it: another of the same app's may be held already.
    fn connect(info: Info, first: Vec<&'static str>) -> BufReader<UnixStream> {
        let (server, client) = UnixStream::pair().unwrap();
        let (pid, before) = (info.pid, holding(info.pid));
        std::thread::spawn(move || serve(server, info, move || first.into_iter().map(|s| Arc::from(format!("{s}\n"))).collect()));
        let until = Instant::now() + Duration::from_secs(5);
        while holding(pid) == before && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(5));
        }
        BufReader::new(client)
    }

    /// How many of an app's connections the server holds.
    fn holding(pid: u32) -> usize {
        views().lock().unwrap().views.iter().filter(|v| v.info.pid == pid).count()
    }

    fn next(r: &mut BufReader<UnixStream>) -> Value {
        loop {
            let mut line = String::new();
            r.read_line(&mut line).unwrap();
            let v: Value = serde_json::from_str(&line).unwrap();
            if v["event"] != "ping" {
                return v;
            }
        }
    }

    fn gone(pid: u32) {
        let until = Instant::now() + Duration::from_secs(5);
        while holding(pid) > 0 && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn a_view_hears_what_it_starts_from_then_what_is_emitted() {
        let _t = turn();
        let mut r = connect(Info { pid: 4_000_001, ..Default::default() }, vec![r#"{"event":"board","payload":{"rev":1}}"#]);
        emit("hosted-stream", &json!({ "key": "k", "text": "hi" }));
        assert_eq!(next(&mut r)["payload"]["rev"], 1);
        let streamed = next(&mut r);
        assert_eq!((streamed["event"].as_str(), streamed["payload"]["text"].as_str()), (Some("hosted-stream"), Some("hi")));
        drop(r);
        // Its end closed, it's let go at the next write.
        emit("ping", &Value::Null);
        gone(4_000_001);
        assert_eq!(holding(4_000_001), 0, "held, alive or not: list leaves out the dead");
    }

    /// Two apps that both found none open to ask: the one opened last stays, whichever connects
    /// first, and the same app connecting again is told nothing.
    #[test]
    fn of_two_apps_the_one_opened_last_stays() {
        let _t = turn();
        let mut older = connect(Info { pid: 4_000_011, since: 1_000, ..Default::default() }, vec![]);
        let mut newer = connect(Info { pid: 4_000_012, since: 2_000, ..Default::default() }, vec![]);
        assert_eq!(next(&mut older)["event"], "quit");
        // The older one connecting after the newer is told to quit at once.
        let mut late = connect(Info { pid: 4_000_013, since: 500, ..Default::default() }, vec![]);
        assert_eq!(next(&mut late)["event"], "quit");
        // The newer one, connecting again as itself (its server restarted), is told nothing.
        let mut again = connect(Info { pid: 4_000_012, since: 2_000, ..Default::default() }, vec![]);
        emit("hosted-stream", &json!({ "key": "k" }));
        assert_eq!(next(&mut again)["event"], "hosted-stream");
        assert_eq!(next(&mut newer)["event"], "hosted-stream");
        drop((older, newer, late, again));
        emit("ping", &Value::Null);
        for pid in [4_000_011, 4_000_012, 4_000_013] {
            gone(pid);
        }
    }

    /// An app of another build is told so, and isn't taken on.
    #[test]
    fn an_app_of_another_build_is_told_so() {
        let _t = turn();
        let (server, client) = UnixStream::pair().unwrap();
        let info = Info { pid: 4_000_021, engine: "0000000000000000".into(), ..Default::default() };
        serve(server, info, Vec::new);
        let mut r = BufReader::new(client);
        assert_eq!(next(&mut r)["event"], "foreign");
        assert_eq!(holding(4_000_021), 0);
    }

    /// An app opening asks those opened before it to quit, not one opened after it, which a slow
    /// start let in first.
    #[test]
    fn taking_over_leaves_an_app_opened_later() {
        let _t = turn();
        let mut before = connect(Info { pid: 4_000_031, since: 1_000, ..Default::default() }, vec![]);
        let mut after = connect(Info { pid: 4_000_032, since: 3_000, ..Default::default() }, vec![]);
        // The second's connecting told the first to quit already; that's what this one does too.
        assert_eq!(next(&mut before)["event"], "quit");
        quit_all(Duration::from_millis(50), Some(2_000));
        emit("hosted-stream", &json!({ "key": "k" }));
        assert_eq!(next(&mut after)["event"], "hosted-stream", "the later app wasn't told to quit");
        drop((before, after));
        emit("ping", &Value::Null);
        for pid in [4_000_031, 4_000_032] {
            gone(pid);
        }
    }

    #[test]
    fn an_ask_goes_to_the_latest_view_and_waits_for_its_answer() {
        let _t = turn();
        assert!(ask("board-ask", json!({}), Duration::from_millis(50)).unwrap_err().contains(NO_VIEW));
        let mut r = connect(Info { pid: 4_000_002, ..Default::default() }, vec![]);
        let asking = std::thread::spawn(|| ask("board-ask", json!({ "board": "b1" }), Duration::from_secs(5)));
        let asked = next(&mut r);
        assert_eq!((asked["event"].as_str(), asked["payload"]["ask"].as_str(), asked["payload"]["payload"]["board"].as_str()), (Some("ask"), Some("board-ask"), Some("b1")));
        answer(asked["payload"]["id"].as_u64().unwrap(), Ok(json!({ "cards": 3 })));
        assert_eq!(asking.join().unwrap().unwrap()["cards"], 3);
        drop(r);
        emit("ping", &Value::Null);
        gone(4_000_002);
    }

    #[test]
    fn a_show_with_no_view_waits_for_the_next_one() {
        let _t = turn();
        assert_eq!(show(json!({ "kind": "session", "key": "k1" })), None);
        let mut r = connect(Info { pid: 4_000_003, bundle: Some("app.codealong.desktop".into()), ..Default::default() }, vec![]);
        let shown = next(&mut r);
        assert_eq!((shown["event"].as_str(), shown["payload"]["key"].as_str()), (Some("show"), Some("k1")));
        // With one connected, it shows there at once, and says which app to bring forward.
        assert_eq!(show(json!({ "kind": "session", "key": "k2" })), Some(Some("app.codealong.desktop".into())));
        assert_eq!(next(&mut r)["payload"]["key"], "k2");
        drop(r);
        emit("ping", &Value::Null);
        gone(4_000_003);
    }
}
