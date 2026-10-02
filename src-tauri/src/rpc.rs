//! A child process that speaks JSON lines: one JSON message a line on its stdin, one a line on
//! its stdout. Codex's app-server (codex/appserver.rs), the Agent Client Protocol adapters (acp.rs)
//! and a side conversation's copy of a session (side.rs) are such processes, and each goes
//! through here: one way to start the process, read its lines on a thread, give an answer to
//! the request that asked for it, say what its stderr last said when it stops, and kill and
//! reap it.
//!
//! Most of them speak JSON-RPC (`request`, `notify`, `respond`, `refuse`). Claude Code's
//! stream-json doesn't, so a line can also be written as it is (`send`), and every line that
//! isn't the answer to a request made here goes to the function the client was started with:
//! notifications, what the child asks of us, and the answers to requests a caller numbered
//! itself (a side conversation's `Dialect` is lines in and lines out, so that it's tested
//! without a process).

use std::collections::{HashMap, VecDeque};
use std::io::{self, BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

/// How many of the child's last stderr lines are kept.
const TAIL: usize = 5;

/// How long the child's last stderr lines are waited for once its stdout has ended. The two end
/// together when the process exits, but they're read on two threads, and something the child
/// started can hold its stderr open for good.
const LAST_WORDS: Duration = Duration::from_millis(250);

/// Who Codealong says it is to a child that asks (`clientInfo`, in Codex's app-server protocol
/// and the Agent Client Protocol alike).
pub fn client_info() -> Value {
    json!({ "name": "codealong", "title": "Codealong", "version": env!("CARGO_PKG_VERSION") })
}

/// A request as a line: what `Link::request` writes, for a caller that numbers its own.
pub fn request_line(id: impl Into<Value>, method: &str, params: Value) -> Value {
    let mut line = note_line(method, params);
    line["id"] = id.into();
    line
}

/// A notification as a line. One with nothing to say (null) has no `params`.
pub fn note_line(method: &str, params: Value) -> Value {
    match params {
        Value::Null => json!({ "method": method }),
        params => json!({ "method": method, "params": params }),
    }
}

/// Where the child's stderr goes.
pub enum Stderr {
    /// Appended to a file.
    Log(std::fs::File),
    /// Read, and its last few lines kept to say why the child stopped.
    Tail,
}

/// How the lines written by `request`, `notify`, `respond` and `refuse` are marked.
#[derive(Clone, Copy, PartialEq)]
pub enum Wire {
    /// As they are: Codex's app-server leaves the version out.
    Bare,
    /// With `"jsonrpc": "2.0"`, as the Agent Client Protocol has them.
    JsonRpc2,
}

/// What the child wrote that isn't the answer to a `request`.
pub enum Heard<'a> {
    /// A line of JSON: a notification, something it asks of us (answer with `respond` or
    /// `refuse`), or anything else.
    Line(&'a Value),
    /// Its output ended: the process stopped, or is about to. With its last stderr lines, the
    /// latest last, where they were kept (`Stderr::Tail`).
    Closed(&'a [String]),
}

/// Why a request has no answer.
#[derive(Debug)]
pub enum Failure {
    /// None came in the time given. The client carries on; an answer that comes later goes
    /// where the other lines go.
    Timeout,
    /// The child's output ended first. With its last stderr lines, as `Heard::Closed` has them.
    Closed(Vec<String>),
    /// The child answered with an error: its `error` object.
    Refused(Value),
    /// The request couldn't be written.
    Write(io::Error),
}

type Answer = std::result::Result<Value, Failure>;

/// The way to a child: what writes to it. The function a client is started with gets one, to
/// answer what the child asks.
pub struct Link {
    pid: u32,
    wire: Wire,
    /// None once the client is ended.
    stdin: Mutex<Option<ChildStdin>>,
    child: Mutex<Child>,
    next: AtomicU64,
    /// The requests waiting for an answer, by id. None once the child's output has ended.
    waiting: Mutex<Option<HashMap<u64, Sender<Answer>>>>,
    tail: Arc<Mutex<VecDeque<String>>>,
    ending: AtomicBool,
}

/// A child process and the way to it. Dropping it kills the process and reaps it.
pub struct Client {
    link: Arc<Link>,
}

impl std::ops::Deref for Client {
    type Target = Link;
    fn deref(&self) -> &Link {
        &self.link
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        finish(&self.link, Duration::ZERO);
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Client {
    /// Starts `cmd` with its stdin and stdout as pipes and reads its lines on a thread of their
    /// own. `on` runs there for each line that isn't an answer to a `request`, in order, and a
    /// last time when the output ends: it mustn't call `request`, whose answer that thread reads.
    pub fn spawn(mut cmd: Command, stderr: Stderr, wire: Wire, mut on: impl FnMut(&Link, Heard<'_>) + Send + 'static) -> io::Result<Client> {
        cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(match stderr {
            Stderr::Log(file) => Stdio::from(file),
            Stderr::Tail => Stdio::piped(),
        });
        let mut child = cmd.spawn()?;
        let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
            let _ = child.kill();
            let _ = child.wait();
            return Err(io::Error::other("the process has no pipes"));
        };
        let stderr = child.stderr.take();
        let tail = Arc::new(Mutex::new(VecDeque::new()));
        let link = Arc::new(Link { pid: child.id(), wire, stdin: Mutex::new(Some(stdin)), child: Mutex::new(child), next: AtomicU64::new(1), waiting: Mutex::new(Some(HashMap::new())), tail: tail.clone(), ending: AtomicBool::new(false) });
        // Ends when stderr does, which the reader below waits for.
        let said = stderr.map(|stderr| {
            let (done, said) = channel::<()>();
            std::thread::spawn(move || {
                for line in BufReader::new(stderr).split(b'\n').map_while(std::result::Result::ok) {
                    let line = String::from_utf8_lossy(&line);
                    if line.trim().is_empty() {
                        continue;
                    }
                    let mut tail = lock(&tail);
                    if tail.len() == TAIL {
                        tail.pop_front();
                    }
                    tail.push_back(line.trim().to_string());
                }
                drop(done);
            });
            said
        });
        let reader = link.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(std::result::Result::ok) {
                let Ok(msg) = serde_json::from_str::<Value>(&line) else { continue };
                match reader.asker(&msg) {
                    Some(asker) => {
                        let _ = asker.send(match msg.get("error") {
                            Some(e) => Err(Failure::Refused(e.clone())),
                            None => Ok(msg.get("result").cloned().unwrap_or(Value::Null)),
                        });
                    }
                    None => on(&reader, Heard::Line(&msg)),
                }
            }
            if let Some(said) = said {
                let _ = said.recv_timeout(LAST_WORDS);
            }
            let last = reader.last_words();
            // Taken under the lock a request starts waiting under, so none waits from here on
            // for an answer that can't come.
            let waiting = lock(&reader.waiting).take();
            for asker in waiting.into_iter().flat_map(HashMap::into_values) {
                let _ = asker.send(Err(Failure::Closed(last.clone())));
            }
            on(&reader, Heard::Closed(&last));
        });
        Ok(Client { link })
    }

    /// Ends the process gently: its input closes, which a process that reads until its input
    /// ends leaves on, and it's killed if it's still there after `grace` (mid-answer, say).
    pub fn end(self, grace: Duration) {
        finish(&self.link, grace);
    }
}

/// Closes the child's input and, `grace` later if it hasn't left by itself, kills it. Only the
/// first call does anything: `Client::end` is followed by the client's drop.
fn finish(link: &Arc<Link>, grace: Duration) {
    if link.ending.swap(true, Ordering::SeqCst) {
        return;
    }
    // Before its input is taken: a write the child isn't reading holds that lock until the
    // child is gone.
    if grace.is_zero() {
        let _ = lock(&link.child).kill();
    }
    lock(&link.stdin).take();
    let link = link.clone();
    // Reaped on a thread of its own: a process stuck in the kernel dies only once the kernel
    // lets it go, and whoever drops a client may hold a lock.
    std::thread::spawn(move || {
        let mut child = lock(&link.child);
        let until = Instant::now() + grace;
        while Instant::now() < until {
            if matches!(child.try_wait(), Ok(Some(_))) {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let _ = child.kill();
        let _ = child.wait();
    });
}

impl Link {
    pub fn pid(&self) -> u32 {
        self.pid
    }

    /// Writes one line as it is.
    pub fn send(&self, line: &Value) -> io::Result<()> {
        let mut text = line.to_string();
        text.push('\n');
        let mut stdin = lock(&self.stdin);
        let Some(stdin) = stdin.as_mut() else { return Err(io::Error::new(io::ErrorKind::BrokenPipe, "its input is closed")) };
        stdin.write_all(text.as_bytes())?;
        stdin.flush()
    }

    /// Sends a request, numbered from 1 in the order they're made, and waits for its answer:
    /// its `result`. None waits as long as the process lives. Any thread but the reader's may
    /// ask, several at once.
    pub fn request(&self, method: &str, params: Value, timeout: Option<Duration>) -> Answer {
        let id = self.next.fetch_add(1, Ordering::SeqCst);
        let (asker, answer) = channel();
        match lock(&self.waiting).as_mut() {
            Some(waiting) => waiting.insert(id, asker),
            None => return Err(Failure::Closed(self.last_words())),
        };
        if let Err(e) = self.send(&self.marked(request_line(id, method, params))) {
            self.forget(id);
            return Err(Failure::Write(e));
        }
        let got = match timeout {
            Some(t) => answer.recv_timeout(t).map_err(|e| match e {
                RecvTimeoutError::Timeout => Failure::Timeout,
                RecvTimeoutError::Disconnected => Failure::Closed(self.last_words()),
            }),
            None => answer.recv().map_err(|_| Failure::Closed(self.last_words())),
        };
        if got.is_err() {
            self.forget(id);
        }
        got?
    }

    /// Tells the child something that takes no answer.
    pub fn notify(&self, method: &str, params: Value) -> io::Result<()> {
        self.send(&self.marked(note_line(method, params)))
    }

    /// Answers request `id` of the child's.
    pub fn respond(&self, id: &Value, result: Value) -> io::Result<()> {
        self.send(&self.marked(json!({ "id": id, "result": result })))
    }

    /// Answers request `id` of the child's with an error.
    pub fn refuse(&self, id: &Value, code: i64, message: &str) -> io::Result<()> {
        self.send(&self.marked(json!({ "id": id, "error": { "code": code, "message": message } })))
    }

    fn marked(&self, mut line: Value) -> Value {
        if self.wire == Wire::JsonRpc2 {
            line["jsonrpc"] = json!("2.0");
        }
        line
    }

    /// Who waits for `msg`, when it's the answer to a request made here: it has no method, and
    /// its id is one that's waiting.
    fn asker(&self, msg: &Value) -> Option<Sender<Answer>> {
        if msg.get("method").and_then(Value::as_str).is_some() {
            return None;
        }
        let id = msg.get("id").and_then(Value::as_u64)?;
        lock(&self.waiting).as_mut()?.remove(&id)
    }

    fn forget(&self, id: u64) {
        if let Some(waiting) = lock(&self.waiting).as_mut() {
            waiting.remove(&id);
        }
    }

    fn last_words(&self) -> Vec<String> {
        lock(&self.tail).iter().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::Receiver;

    /// A shell function for the stand-in children: answers the request in `$1` with its own
    /// method, by its id. The lines written here have their keys in order, the id first.
    const ANSWER: &str = r#"answer() { id=${1#*\"id\":}; id=${id%%,*}; method=${1#*\"method\":\"}; method=${method%%\"*}; printf '{"id":%s,"result":{"method":"%s"}}\n' "$id" "$method"; }; "#;

    /// A child that runs `script` in `/bin/sh`, and what it wrote that wasn't an answer: each
    /// line, then `None` when its output ended, with its last stderr lines.
    fn child(script: &str, wire: Wire) -> (Client, Receiver<(Option<Value>, Vec<String>)>) {
        let mut cmd = Command::new("/bin/sh");
        cmd.args(["-c", &format!("{ANSWER}{script}")]);
        let (tx, rx) = channel();
        let client = Client::spawn(cmd, Stderr::Tail, wire, move |link, heard| match heard {
            Heard::Line(v) => {
                // What the child asks is answered from here, as acp.rs answers a permission
                // request it can't place.
                if v["method"] == "ask" {
                    link.respond(&v["id"], json!({ "ok": true })).unwrap();
                } else if v["method"] == "ask-too-much" {
                    link.refuse(&v["id"], -32601, "Not offered.").unwrap();
                }
                let _ = tx.send((Some(v.clone()), Vec::new()));
            }
            Heard::Closed(last) => {
                let _ = tx.send((None, last.to_vec()));
            }
        })
        .unwrap();
        (client, rx)
    }

    fn next(rx: &Receiver<(Option<Value>, Vec<String>)>) -> (Option<Value>, Vec<String>) {
        rx.recv_timeout(Duration::from_secs(10)).expect("the child wrote nothing more")
    }

    const SOON: Option<Duration> = Some(Duration::from_secs(10));

    #[test]
    fn an_answer_goes_to_its_request_whatever_the_order() {
        // The second request read is answered first.
        let (client, _rx) = child("read one; read two; answer \"$two\"; answer \"$one\"; cat >/dev/null", Wire::Bare);
        let client = Arc::new(client);
        let asks: Vec<_> = ["first", "second"]
            .into_iter()
            .map(|method| {
                let client = client.clone();
                std::thread::spawn(move || client.request(method, json!({}), SOON).unwrap())
            })
            .collect();
        let got: Vec<Value> = asks.into_iter().map(|t| t.join().unwrap()).collect();
        assert_eq!(got, [json!({ "method": "first" }), json!({ "method": "second" })]);
    }

    #[test]
    fn what_is_not_an_answer_goes_to_the_function() {
        // A notification, a line that isn't JSON (dropped), then the answer.
        let (client, rx) = child(r#"read one; echo '{"method":"tick","params":{"n":1}}'; echo 'not json'; answer "$one"; cat >/dev/null"#, Wire::Bare);
        assert_eq!(client.request("ping", json!({}), SOON).unwrap(), json!({ "method": "ping" }));
        assert_eq!(next(&rx).0, Some(json!({ "method": "tick", "params": { "n": 1 } })));
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn what_the_child_asks_is_answered_from_the_function() {
        // The child says back what it was answered, so the answers can be read here.
        let script = r#"echo '{"id":7,"method":"ask","params":{}}'; read a; echo "{\"method\":\"got\",\"params\":$a}"; echo '{"id":"x","method":"ask-too-much"}'; read b; echo "{\"method\":\"got\",\"params\":$b}"; cat >/dev/null"#;
        let (_client, rx) = child(script, Wire::JsonRpc2);
        assert_eq!(next(&rx).0.unwrap()["method"], "ask");
        assert_eq!(next(&rx).0.unwrap()["params"], json!({ "jsonrpc": "2.0", "id": 7, "result": { "ok": true } }));
        assert_eq!(next(&rx).0.unwrap()["method"], "ask-too-much");
        assert_eq!(next(&rx).0.unwrap()["params"], json!({ "jsonrpc": "2.0", "id": "x", "error": { "code": -32601, "message": "Not offered." } }));
    }

    #[test]
    fn a_request_that_times_out_leaves_the_client_working() {
        let (client, rx) = child("read one; read two; answer \"$two\"; read go; answer \"$one\"; cat >/dev/null", Wire::Bare);
        assert!(matches!(client.request("slow", json!({}), Some(Duration::from_millis(100))), Err(Failure::Timeout)));
        assert_eq!(client.request("quick", json!({}), SOON).unwrap(), json!({ "method": "quick" }));
        // The late answer is nobody's now: it goes where the other lines go.
        client.notify("go", Value::Null).unwrap();
        assert_eq!(next(&rx).0, Some(json!({ "id": 1, "result": { "method": "slow" } })));
    }

    #[test]
    fn a_child_that_stops_fails_what_waits_with_what_it_last_said() {
        let (client, rx) = child("read one; echo 'warming up' >&2; echo >&2; echo '  No conversation found ' >&2; exit 3", Wire::Bare);
        let said = ["warming up".to_string(), "No conversation found".to_string()];
        match client.request("thread/fork", json!({}), None) {
            Err(Failure::Closed(last)) => assert_eq!(last, said),
            other => panic!("{other:?}"),
        }
        assert_eq!(next(&rx), (None, said.to_vec()));
        // And what's asked after that fails at once, the same way.
        assert!(matches!(client.request("again", json!({}), None), Err(Failure::Closed(last)) if last == said));
    }

    #[test]
    fn an_error_answer_is_the_requests_failure() {
        let (client, _rx) = child(r#"read one; echo '{"id":1,"error":{"code":-32600,"message":"no rollout found"}}'; cat >/dev/null"#, Wire::Bare);
        match client.request("thread/fork", json!({}), SOON) {
            Err(Failure::Refused(e)) => assert_eq!(e["message"], "no rollout found"),
            other => panic!("{other:?}"),
        }
    }

    /// What's written, read back from `cat`: a request of ours comes back with its method, so
    /// it reads as something the child asks and goes to the function.
    #[test]
    fn lines_are_written_as_each_protocol_has_them() {
        let wrote = |wire: Wire| {
            let mut cmd = Command::new("/bin/cat");
            cmd.arg("-u");
            let (tx, rx) = channel();
            let client = Client::spawn(cmd, Stderr::Tail, wire, move |_, heard| {
                if let Heard::Line(v) = heard {
                    let _ = tx.send(v.to_string());
                }
            })
            .unwrap();
            assert!(matches!(client.request("initialize", json!({ "clientInfo": client_info() }), Some(Duration::from_millis(50))), Err(Failure::Timeout)));
            client.notify("initialized", Value::Null).unwrap();
            client.notify("session/cancel", json!({ "sessionId": "s" })).unwrap();
            assert!(matches!(client.request("thread/archive", json!({ "threadId": "t" }), Some(Duration::from_millis(50))), Err(Failure::Timeout)));
            // Not JSON-RPC at all: Claude Code's stream-json.
            client.send(&json!({ "type": "user", "message": { "role": "user", "content": "Why?" } })).unwrap();
            (0..5).map(|_| rx.recv_timeout(Duration::from_secs(10)).unwrap()).collect::<Vec<_>>()
        };
        let version = env!("CARGO_PKG_VERSION");
        assert_eq!(
            wrote(Wire::Bare),
            [
                format!(r#"{{"id":1,"method":"initialize","params":{{"clientInfo":{{"name":"codealong","title":"Codealong","version":"{version}"}}}}}}"#),
                r#"{"method":"initialized"}"#.to_string(),
                r#"{"method":"session/cancel","params":{"sessionId":"s"}}"#.to_string(),
                r#"{"id":2,"method":"thread/archive","params":{"threadId":"t"}}"#.to_string(),
                r#"{"message":{"content":"Why?","role":"user"},"type":"user"}"#.to_string(),
            ]
        );
        assert_eq!(
            wrote(Wire::JsonRpc2),
            [
                format!(r#"{{"id":1,"jsonrpc":"2.0","method":"initialize","params":{{"clientInfo":{{"name":"codealong","title":"Codealong","version":"{version}"}}}}}}"#),
                r#"{"jsonrpc":"2.0","method":"initialized"}"#.to_string(),
                r#"{"jsonrpc":"2.0","method":"session/cancel","params":{"sessionId":"s"}}"#.to_string(),
                r#"{"id":2,"jsonrpc":"2.0","method":"thread/archive","params":{"threadId":"t"}}"#.to_string(),
                r#"{"message":{"content":"Why?","role":"user"},"type":"user"}"#.to_string(),
            ]
        );
        assert_eq!(request_line(4, "turn/start", json!({ "threadId": "c" })), json!({ "id": 4, "method": "turn/start", "params": { "threadId": "c" } }));
    }

    fn gone(pid: u32) -> bool {
        for _ in 0..100 {
            if !crate::procs::pid_alive(pid) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        false
    }

    #[test]
    fn dropping_the_client_kills_the_child() {
        // It would outlast the test: closing its input doesn't end it.
        let (client, rx) = child("exec sleep 600", Wire::Bare);
        let pid = client.pid();
        assert!(crate::procs::pid_alive(pid));
        drop(client);
        assert!(gone(pid), "the child is still there");
        assert_eq!(next(&rx), (None, Vec::new()));
    }

    #[test]
    fn ending_the_client_lets_the_child_leave_first() {
        // One that leaves when its input closes isn't killed: it says goodbye.
        let (client, rx) = child("cat >/dev/null; echo '{\"method\":\"bye\"}'", Wire::Bare);
        let pid = client.pid();
        client.end(Duration::from_secs(10));
        assert_eq!(next(&rx).0, Some(json!({ "method": "bye" })));
        assert!(gone(pid));
        // One that doesn't is killed once its time is up.
        let (client, _rx) = child("exec sleep 600", Wire::Bare);
        let pid = client.pid();
        client.end(Duration::from_millis(200));
        assert!(crate::procs::pid_alive(pid));
        assert!(gone(pid), "the child is still there");
    }
}
