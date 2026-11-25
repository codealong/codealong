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
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::os::procs::{self, Fd, Wrote};

/// How many of the child's last stderr lines are kept.
const TAIL: usize = 5;

/// How long the child's last stderr lines are waited for once its stdout has ended. The two end
/// together when the process exits, but they're read on two threads, and something the child
/// started can hold its stderr open for good.
const LAST_WORDS: Duration = Duration::from_millis(250);

/// How long a line that takes no answer (`send`, `notify`, `respond`, `refuse`) may wait for the
/// child to take it. Its input is a pipe that holds 64 KB: a child that leaves that unread this
/// long has stopped reading.
const WRITE_TIMEOUT: Duration = Duration::from_secs(10);

/// How often a write the child isn't taking looks again, and looks whether the client is ending.
const TICK: Duration = Duration::from_millis(50);

/// How often the stdout reader wakes to check whether the server is handing the client over.
const READ_TICK: Duration = Duration::from_millis(25);

/// How long `Client::hand_over` waits for the stdout reader to stop and return its pipe.
const HAND_OVER_WAIT: Duration = Duration::from_secs(2);

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
    /// None came in the time given, or the child didn't take the request in it. The client
    /// carries on; an answer that comes later goes where the other lines go.
    Timeout,
    /// The child's output ended first. With its last stderr lines, as `Heard::Closed` has them.
    Closed(Vec<String>),
    /// The child answered with an error: its `error` object.
    Refused(Value),
    /// The request couldn't be written.
    Write(io::Error),
}

pub type Answer = std::result::Result<Value, Failure>;

/// How the child process is owned: spawned here, taken over from the server's previous build by
/// pid, or handed out to the next build so dropping `Client` leaves it running.
enum Owner {
    Child(Child),
    Handed(u32),
    HandedOut,
}

impl Owner {
    fn kill(&mut self) {
        match self {
            Owner::Child(c) => {
                let _ = c.kill();
            }
            Owner::Handed(pid) => {
                let _ = procs::signal(*pid, procs::Signal::Kill);
            }
            Owner::HandedOut => {}
        }
    }

    fn try_reaped(&mut self) -> bool {
        match self {
            Owner::Child(c) => matches!(c.try_wait(), Ok(Some(_))),
            Owner::Handed(pid) => procs::try_reap(*pid),
            Owner::HandedOut => true,
        }
    }

    fn wait(&mut self) {
        match self {
            Owner::Child(c) => {
                let _ = c.wait();
            }
            Owner::Handed(pid) => crate::os::pty::reap(*pid),
            Owner::HandedOut => {}
        }
    }
}

/// A running child's open pipes and JSON-RPC state, frozen for `serve.rs::hand_over` to pass
/// across an exec (`Client::hand_over`, `Client::from_handed`).
pub struct Handed {
    pub pid: u32,
    pub stdin_fd: Fd,
    pub stdout_fd: Fd,
    pub next_id: u64,
    pub pending_ids: Vec<u64>,
    pub unread: Vec<u8>,
}

/// The way to a child: what writes to it. The function a client is started with gets one, to
/// answer what the child asks.
pub struct Link {
    pid: u32,
    wire: Wire,
    /// None once the client is ended, handed over, or a line was given up on half written.
    stdin: Mutex<Option<File>>,
    child: Mutex<Owner>,
    next: AtomicU64,
    /// The requests waiting for an answer, by id. None once the child's output has ended.
    waiting: Mutex<Option<HashMap<u64, Sender<Answer>>>>,
    tail: Arc<Mutex<VecDeque<String>>>,
    ending: AtomicBool,
    handing_over: AtomicBool,
    handed_tx: Mutex<Option<Sender<(File, Vec<u8>)>>>,
}

/// A child process and the way to it. Dropping it kills the process and reaps it (unless handed
/// over to the next build via `Client::hand_over`).
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

fn spawn_reader(
    reader: Arc<Link>,
    mut stdout: File,
    initial: Vec<u8>,
    said: Option<Receiver<()>>,
    mut on: impl FnMut(&Link, Heard<'_>) + Send + 'static,
) {
    std::thread::spawn(move || {
        let mut buf = initial;
        let mut chunk = [0u8; 8192];
        let mut dispatch = |raw: &[u8]| {
            let trimmed = raw.strip_suffix(b"\r").unwrap_or(raw);
            let text = String::from_utf8_lossy(trimmed);
            let Ok(msg) = serde_json::from_str::<Value>(&text) else { return };
            match reader.asker(&msg) {
                Some(asker) => {
                    let _ = asker.send(match msg.get("error") {
                        Some(e) => Err(Failure::Refused(e.clone())),
                        None => Ok(msg.get("result").cloned().unwrap_or(Value::Null)),
                    });
                }
                None => on(&reader, Heard::Line(&msg)),
            }
        };
        let handed = loop {
            while let Some(nl) = buf.iter().position(|&b| b == b'\n') {
                let line: Vec<u8> = buf.drain(..=nl).collect();
                dispatch(&line[..line.len() - 1]);
            }
            if reader.handing_over.load(Ordering::SeqCst) {
                if let Some(tx) = lock(&reader.handed_tx).take() {
                    let _ = tx.send((stdout, buf));
                    break true;
                }
            }
            if !procs::readable_within(&stdout, READ_TICK) {
                continue;
            }
            match stdout.read(&mut chunk) {
                Ok(0) => {
                    if !buf.is_empty() {
                        dispatch(&buf);
                        buf.clear();
                    }
                    break false;
                }
                Ok(n) => buf.extend_from_slice(&chunk[..n]),
                Err(e) if e.kind() == io::ErrorKind::Interrupted || e.kind() == io::ErrorKind::WouldBlock => continue,
                Err(_) => break false,
            }
        };
        if handed {
            return;
        }
        // Drop any handover waiter immediately if EOF raced with `Client::hand_over`.
        lock(&reader.handed_tx).take();
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
}

impl Client {
    /// Starts `cmd` with its stdin and stdout as pipes and reads its lines on a thread of their
    /// own. `on` runs there for each line that isn't an answer to a `request`, in order, and a
    /// last time when the output ends: it mustn't call `request`, whose answer that thread reads.
    pub fn spawn(mut cmd: Command, stderr: Stderr, wire: Wire, on: impl FnMut(&Link, Heard<'_>) + Send + 'static) -> io::Result<Client> {
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
        let (stdin, stdout) = (procs::pipe_in_from_child(stdin), procs::pipe_out_from_child(stdout));
        let stderr = child.stderr.take();
        // Written without blocking, so a child that stops reading holds a write only as long as
        // its deadline, and never past the client's end (`Link::write`); stdout is also nonblocking
        // so the reader can stop cleanly for server hand-over.
        crate::procs::nonblocking(&stdin);
        crate::procs::nonblocking(&stdout);
        let tail = Arc::new(Mutex::new(VecDeque::new()));
        let link = Arc::new(Link {
            pid: child.id(),
            wire,
            stdin: Mutex::new(Some(stdin)),
            child: Mutex::new(Owner::Child(child)),
            next: AtomicU64::new(1),
            waiting: Mutex::new(Some(HashMap::new())),
            tail: tail.clone(),
            ending: AtomicBool::new(false),
            handing_over: AtomicBool::new(false),
            handed_tx: Mutex::new(None),
        });
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
        spawn_reader(link.clone(), stdout, Vec::new(), said, on);
        Ok(Client { link })
    }

    /// Re-attaches to a running child handed across an exec (`hand_over`): pre-registers every
    /// `pending_ids` waiter before starting the stdout reader so responses already sitting in the
    /// pipe or in `unread` are delivered to their callers.
    ///
    /// # Safety
    ///
    /// `stdin_fd` and `stdout_fd` are open pipe descriptors owned exclusively by the caller.
    #[allow(clippy::too_many_arguments)]
    pub unsafe fn from_handed(
        pid: u32,
        stdin_fd: Fd,
        stdout_fd: Fd,
        next_id: u64,
        pending_ids: &[u64],
        unread: Vec<u8>,
        wire: Wire,
        on: impl FnMut(&Link, Heard<'_>) + Send + 'static,
    ) -> io::Result<(Client, HashMap<u64, Receiver<Answer>>)> {
        // SAFETY: caller guarantees unique ownership of both descriptors.
        let stdin = unsafe { procs::pipe_from_handed(stdin_fd) }?;
        let stdout = match unsafe { procs::pipe_from_handed(stdout_fd) } {
            Ok(out) => out,
            Err(e) => {
                drop(stdin);
                return Err(e);
            }
        };
        crate::procs::nonblocking(&stdin);
        crate::procs::nonblocking(&stdout);
        let mut waiting = HashMap::with_capacity(pending_ids.len());
        let mut receivers = HashMap::with_capacity(pending_ids.len());
        for &id in pending_ids {
            let (tx, rx) = channel();
            waiting.insert(id, tx);
            receivers.insert(id, rx);
        }
        let link = Arc::new(Link {
            pid,
            wire,
            stdin: Mutex::new(Some(stdin)),
            child: Mutex::new(Owner::Handed(pid)),
            next: AtomicU64::new(next_id.max(1)),
            waiting: Mutex::new(Some(waiting)),
            tail: Arc::new(Mutex::new(VecDeque::new())),
            ending: AtomicBool::new(false),
            handing_over: AtomicBool::new(false),
            handed_tx: Mutex::new(None),
        });
        spawn_reader(link.clone(), stdout, unread, None, on);
        Ok((Client { link }, receivers))
    }

    /// Freezes this client for a server hand-over: stops the stdout reader without losing any
    /// partial line in its buffer, takes stdin so no further lines are written, dups both pipes as
    /// close-on-exec descriptors, and disarms `Drop` so the child keeps running across `exec`.
    /// Pending `waiting` senders are left parked until `exec` replaces the process.
    pub fn hand_over(&self) -> Option<Handed> {
        if self.is_closed() || self.ending.load(Ordering::SeqCst) {
            return None;
        }
        let (tx, rx) = channel();
        *lock(&self.handed_tx) = Some(tx);
        self.handing_over.store(true, Ordering::SeqCst);
        let (stdout, unread) = match rx.recv_timeout(HAND_OVER_WAIT) {
            Ok(pair) => pair,
            Err(_) => {
                self.handing_over.store(false, Ordering::SeqCst);
                lock(&self.handed_tx).take();
                return None;
            }
        };
        let mut stdin_guard = lock(&self.stdin);
        let Some(stdin) = stdin_guard.as_ref() else {
            return None;
        };
        let stdin_fd = procs::dup_pipe(stdin);
        let stdout_fd = procs::dup_pipe(&stdout);
        let (Some(stdin_fd), Some(stdout_fd)) = (stdin_fd, stdout_fd) else {
            if let Some(fd) = stdin_fd {
                // SAFETY: closing a dup'd descriptor we just made and aren't using.
                unsafe { crate::os::pty::close(fd) };
            }
            if let Some(fd) = stdout_fd {
                // SAFETY: closing a dup'd descriptor we just made and aren't using.
                unsafe { crate::os::pty::close(fd) };
            }
            return None;
        };
        // No more writes from this process; the dup'd `stdin_fd` keeps the pipe open.
        stdin_guard.take();
        drop(stdin_guard);
        drop(stdout);
        self.ending.store(true, Ordering::SeqCst);
        *lock(&self.child) = Owner::HandedOut;
        let next_id = self.next.load(Ordering::SeqCst);
        let mut pending_ids: Vec<u64> = lock(&self.waiting).as_ref().map(|w| w.keys().copied().collect()).unwrap_or_default();
        pending_ids.sort_unstable();
        Some(Handed { pid: self.pid, stdin_fd, stdout_fd, next_id, pending_ids, unread })
    }

    /// Ends the process gently: its input closes, which a process that reads until its input
    /// ends leaves on, and it's killed if it's still there after `grace` (mid-answer, say).
    pub fn end(self, grace: Duration) {
        finish(&self.link, grace);
    }

    /// The way to the child, for writing to it from a thread of its own: the process still ends
    /// with the client, and writes after that fail.
    pub fn link(&self) -> Arc<Link> {
        self.link.clone()
    }
}

/// Closes the child's input and, `grace` later if it hasn't left by itself, kills it. Only the
/// first call does anything: `Client::end` is followed by the client's drop.
fn finish(link: &Arc<Link>, grace: Duration) {
    if link.ending.swap(true, Ordering::SeqCst) {
        return;
    }
    if grace.is_zero() {
        lock(&link.child).kill();
    }
    let until = Instant::now() + grace;
    let reaper = link.clone();
    // Reaped on a thread of its own: a process stuck in the kernel dies only once the kernel
    // lets it go, and whoever drops a client may hold a lock. It's set going before the input is
    // closed, so the kill comes on time whatever a write in progress is doing.
    std::thread::spawn(move || {
        let mut child = lock(&reaper.child);
        while Instant::now() < until {
            if child.try_reaped() {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        child.kill();
        child.wait();
    });
    // A write the child isn't taking holds this lock until it sees the client ending, a tick.
    lock(&link.stdin).take();
}

impl Link {
    pub fn pid(&self) -> u32 {
        self.pid
    }

    /// Whether the child's output has ended: it stopped, or is about to, and what's asked of it
    /// from now on fails. True a moment before the function the client was started with hears
    /// `Heard::Closed`.
    pub fn is_closed(&self) -> bool {
        lock(&self.waiting).is_none()
    }

    /// Writes one line as it is.
    pub fn send(&self, line: &Value) -> io::Result<()> {
        self.write(line, Some(Instant::now() + WRITE_TIMEOUT))
    }

    fn write(&self, line: &Value, deadline: Option<Instant>) -> io::Result<()> {
        self.write_if(line, deadline, || true).map(|_| ())
    }

    /// Writes one line if `wanted()` says so, asked once no other line can go before it: false
    /// leaves it unwritten. It gives up at `deadline` (with `TimedOut`) or when the client ends.
    /// A line given up on half written closes the child's input: what it read next would be the
    /// rest of that line run into another.
    fn write_if(&self, line: &Value, deadline: Option<Instant>, wanted: impl FnOnce() -> bool) -> io::Result<bool> {
        let closed = || io::Error::new(io::ErrorKind::BrokenPipe, "its input is closed");
        let mut text = line.to_string();
        text.push('\n');
        let mut stdin = lock(&self.stdin);
        if stdin.is_none() {
            return Err(closed());
        }
        if !wanted() {
            return Ok(false);
        }
        let Some(pipe) = stdin.as_mut() else { return Err(closed()) };
        let late = || deadline.is_some_and(|d| Instant::now() >= d);
        match crate::os::procs::write_within(pipe, text.as_bytes(), TICK, &|| late() || self.ending.load(Ordering::SeqCst))? {
            Wrote::All => Ok(true),
            gave_up => {
                if gave_up == Wrote::Part {
                    stdin.take();
                }
                Err(if late() { io::Error::new(io::ErrorKind::TimedOut, "it isn't reading its input") } else { closed() })
            }
        }
    }

    /// The JSON-RPC request ids currently waiting for an answer.
    pub fn pending_ids(&self) -> Vec<u64> {
        let mut ids: Vec<u64> = lock(&self.waiting).as_ref().map(|w| w.keys().copied().collect()).unwrap_or_default();
        ids.sort_unstable();
        ids
    }

    /// Sends a request, numbered from 1 in the order they're made, and waits for its answer:
    /// its `result`. `timeout` covers writing the request as well as the answer; None waits as
    /// long as the process lives. Any thread but the reader's may ask, several at once.
    pub fn request(&self, method: &str, params: Value, timeout: Option<Duration>) -> Answer {
        self.request_if(method, params, timeout, || true).unwrap_or_else(|| unreachable!("a request that's always wanted is always made"))
    }

    /// As `request`, if `wanted()` still says so as the request is written: it's asked once no
    /// other line can go before it, so a line written once it has said yes goes after the
    /// request. None, with nothing written, when it says no.
    pub fn request_if(&self, method: &str, params: Value, timeout: Option<Duration>, wanted: impl FnOnce() -> bool) -> Option<Answer> {
        let deadline = timeout.map(|t| Instant::now() + t);
        let (id, rx) = match self.start_request_if(method, params, deadline, wanted)? {
            Ok(pair) => pair,
            Err(e) => return Some(Err(e)),
        };
        Some(self.finish_request(id, rx, deadline))
    }

    /// Writes a request if `wanted()` still says so and returns its JSON-RPC id and answer
    /// receiver without blocking on the answer yet, so the caller can record `id` (for server
    /// hand-over) before waiting in `finish_request`.
    pub fn start_request_if(&self, method: &str, params: Value, deadline: Option<Instant>, wanted: impl FnOnce() -> bool) -> Option<std::result::Result<(u64, Receiver<Answer>), Failure>> {
        let id = self.next.fetch_add(1, Ordering::SeqCst);
        let (asker, answer) = channel();
        match lock(&self.waiting).as_mut() {
            Some(waiting) => waiting.insert(id, asker),
            None => return Some(Err(Failure::Closed(self.last_words()))),
        };
        match self.write_if(&self.marked(request_line(id, method, params)), deadline, wanted) {
            Ok(true) => Some(Ok((id, answer))),
            Ok(false) => {
                self.forget(id);
                None
            }
            Err(e) => {
                self.forget(id);
                Some(Err(if e.kind() == io::ErrorKind::TimedOut { Failure::Timeout } else { Failure::Write(e) }))
            }
        }
    }

    /// Waits for an answer started by `start_request_if` (or restored by `Client::from_handed`).
    pub fn finish_request(&self, id: u64, answer: Receiver<Answer>, deadline: Option<Instant>) -> Answer {
        let got = match deadline {
            Some(d) => answer.recv_timeout(d.saturating_duration_since(Instant::now())).map_err(|e| match e {
                RecvTimeoutError::Timeout => Failure::Timeout,
                RecvTimeoutError::Disconnected => Failure::Closed(self.last_words()),
            }),
            None => answer.recv().map_err(|_| Failure::Closed(self.last_words())),
        };
        if got.is_err() {
            self.forget(id);
        }
        got.and_then(|a| a)
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
        let mut cmd = Command::new(crate::procs::posix_tool("sh"));
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
        assert!(!client.is_closed());
        match client.request("thread/fork", json!({}), None) {
            Err(Failure::Closed(last)) => assert_eq!(last, said),
            other => panic!("{other:?}"),
        }
        assert_eq!(next(&rx), (None, said.to_vec()));
        assert!(client.is_closed());
        // And what's asked after that fails at once, the same way.
        assert!(matches!(client.request("again", json!({}), None), Err(Failure::Closed(last)) if last == said));
    }

    /// A request that's no longer wanted as it's about to be written isn't, and what's written
    /// once it has said it is goes after it, even from another thread.
    #[test]
    fn a_request_goes_only_if_still_wanted_and_before_what_follows() {
        let mut cmd = Command::new(crate::procs::posix_tool("cat"));
        cmd.arg("-u");
        let (tx, rx) = channel();
        let client = Client::spawn(cmd, Stderr::Tail, Wire::Bare, move |_, heard| {
            if let Heard::Line(v) = heard {
                let _ = tx.send(v["method"].as_str().unwrap_or("").to_string());
            }
        })
        .unwrap();
        assert!(client.request_if("unwanted", json!({}), SOON, || false).is_none());
        let link = client.link.clone();
        let wanted = || {
            // Another thread writes as soon as it can, which is once the request has gone.
            let link = link.clone();
            std::thread::spawn(move || link.notify("after", Value::Null).unwrap());
            std::thread::sleep(Duration::from_millis(200));
            true
        };
        assert!(matches!(client.request_if("wanted", json!({}), Some(Duration::from_millis(300)), wanted), Some(Err(Failure::Timeout))));
        let got: Vec<String> = (0..2).map(|_| rx.recv_timeout(Duration::from_secs(10)).unwrap()).collect();
        assert_eq!(got, ["wanted", "after"]);
        assert!(rx.recv_timeout(Duration::from_millis(200)).is_err());
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
            let mut cmd = Command::new(crate::procs::posix_tool("cat"));
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

    /// More than the 64 KB a pipe holds, for a child that reads none of it.
    fn too_big() -> Value {
        json!({ "pad": "x".repeat(1 << 20) })
    }

    #[test]
    fn a_request_the_child_doesnt_take_times_out() {
        let (client, _rx) = child("exec sleep 600", Wire::Bare);
        let started = Instant::now();
        assert!(matches!(client.request("big", too_big(), Some(Duration::from_millis(200))), Err(Failure::Timeout)));
        assert!(started.elapsed() < Duration::from_secs(3), "{:?}", started.elapsed());
        // Part of it went in, so the input is closed rather than have the next line run into it.
        assert_eq!(client.notify("next", Value::Null).unwrap_err().kind(), io::ErrorKind::BrokenPipe);
    }

    #[test]
    fn ending_the_client_doesnt_wait_for_a_write_the_child_isnt_taking() {
        let (client, _rx) = child("exec sleep 600", Wire::Bare);
        let pid = client.pid();
        let link = client.link.clone();
        let writer = std::thread::spawn(move || link.request("big", too_big(), None));
        std::thread::sleep(Duration::from_millis(200));
        let started = Instant::now();
        client.end(Duration::from_millis(300));
        assert!(started.elapsed() < Duration::from_secs(1), "{:?}", started.elapsed());
        assert!(gone(pid), "the child is still there");
        assert!(writer.join().unwrap().is_err());
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

    /// Handing a client over keeps its process alive across `Client::drop`, delivers the answer
    /// to an in-flight request to the new client, continues numbering requests after `next_id`,
    /// and stitches any partial line buffered before the hand-over.
    #[cfg(unix)]
    #[test]
    fn handing_a_client_over_keeps_in_flight_requests_and_buffered_bytes() {
        let (client, _old_rx) = child(
            r#"read one; printf '{"method":"split",'; read go; printf '"params":{"ok":true}}\n'; answer "$one"; read two; answer "$two"; cat >/dev/null"#,
            Wire::JsonRpc2,
        );
        let pid = client.pid();
        let (id, _old_answer_rx) = client.start_request_if("slow", json!({}), None, || true).unwrap().unwrap();
        assert_eq!(id, 1);
        // Wait briefly so the child writes the partial first half of the notification line.
        std::thread::sleep(Duration::from_millis(150));
        let handed = client.hand_over().expect("client hands over");
        assert_eq!(handed.pid, pid);
        assert_eq!(handed.pending_ids, vec![1]);
        assert!(handed.next_id >= 2);
        // Dropping the handed-out client must not kill the child.
        drop(client);
        assert!(crate::procs::pid_alive(pid), "child must stay alive after hand_over");

        let (tx, new_rx) = channel();
        // SAFETY: `handed` owns both dup'd fds exclusively.
        let (resumed, mut waiters) = unsafe {
            Client::from_handed(handed.pid, handed.stdin_fd, handed.stdout_fd, handed.next_id, &handed.pending_ids, handed.unread, Wire::JsonRpc2, move |_, heard| {
                if let Heard::Line(v) = heard {
                    let _ = tx.send(v.clone());
                }
            })
        }
        .unwrap();
        let slow_rx = waiters.remove(&1).expect("pending id 1 restored");
        // Tell the child to finish the split line and answer request 1.
        resumed.notify("go", Value::Null).unwrap();
        let split = new_rx.recv_timeout(Duration::from_secs(10)).expect("stitched notification");
        assert_eq!(split, json!({ "method": "split", "params": { "ok": true } }));
        let slow = resumed.finish_request(1, slow_rx, Some(Instant::now() + Duration::from_secs(10))).unwrap();
        assert_eq!(slow, json!({ "method": "slow" }));
        // Subsequent requests work on the resumed client and don't reuse id 1.
        let second = resumed.request("after", json!({}), SOON).unwrap();
        assert_eq!(second, json!({ "method": "after" }));
        // Dropping the resumed client kills and reaps the child.
        drop(resumed);
        assert!(gone(pid), "resumed client drop must kill and reap the child");
    }
}
