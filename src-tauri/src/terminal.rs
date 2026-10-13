//! Commands an agent asks to run in a terminal of yours (`codealong terminal run`): you allow the
//! exact command, it runs in a new tab of your terminal beside the session's, as you (your shell,
//! its settings and your sign-ins, outside any sandbox), and what it prints goes back to the agent.
//! The Claude app's Code tab does the same in its own Terminal panel (its `run_in_terminal` tool);
//! this is it for every harness, in the terminal you use.
//!
//! Asking. The command hands the app the request over its socket and waits there (server.rs), as
//! Claude Code's permission hook does (claude/asks.rs): the connection's thread holds it here until
//! you answer. Meanwhile the session that asked waits on you for it (`overlay`), and the request is
//! what `session-pending` gives for it and `session-answer` answers (actions.rs), so the panel, the
//! notch and the waiting window show it with Allow and Deny. It's asked every time: there's no
//! Always. Only you allow it, at this Mac or from your phone, never the command or another Mac,
//! where an agent may have asked: the tab has your environment, not the agent's, so what an agent's
//! setup keeps from it (a credential, an identity a repo's scripts won't run under) the tab has. A
//! request lives while the command that asked waits; one whose command was stopped (an agent's tool
//! timing out) goes from the windows.
//!
//! Running. Codealong doesn't own the terminal, and Ghostty gives a script no way to read a tab's
//! text, so the command runs under `script(1)` in the tab: on a terminal of its own, which `script`
//! relays to the tab and writes to a file as it shows (`-F`). What you type isn't kept (no `-k`), so
//! a password typed at a prompt that doesn't echo it stays out of the file. `script` exits with the
//! command's status. Each run has a folder (`dir`) with the request, the command, the wrapper the
//! tab runs (`wrapper`), its processes, what it printed, and its exit status once it ends. Runs are
//! kept a week (`prune`).
//!
//! Reading. What it printed is a terminal's bytes, colors and a progress line redrawn with a
//! carriage return among them; `render` turns them into the lines a screen ends up showing, one
//! line at a time. A program that draws the whole screen (an editor, `top`) comes out garbled, as
//! `script` keeps it.

use std::io::Write as _;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use codealong_peer::api;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::config::support_dir;
use crate::model::{AppNames, Lane, Session, Status};
use crate::procs;
use crate::util::{code, now_ms, one_line, sh_quote, tilde, words, DAY_MS};

/// How a run's id starts, and the id of the request it comes from.
const PREFIX: &str = "run-";

/// The files in a run's folder.
const RECORD: &str = "run.json";
const COMMAND: &str = "command";
const WRAPPER: &str = "run.sh";
/// The wrapper's pid and its parent's, the tab's shell, as the wrapper starts.
const PIDS: &str = "pids";
/// The pid of the shell the command runs in, on `script`'s terminal.
const SHELL: &str = "shell";
const OUTPUT: &str = "output";
const EXIT: &str = "exit";

/// How long a run's folder is kept once it has ended.
const KEEP_MS: i64 = 7 * DAY_MS;
/// The longest command taken.
pub const LONGEST: usize = 16 * 1024;
/// How much of the end of what a run printed is read at most.
const READ_BYTES: u64 = 8 << 20;

pub fn is_id(id: &str) -> bool {
    id.starts_with(PREFIX)
}

pub fn runs_dir() -> PathBuf {
    #[cfg(test)]
    if let Some(d) = TEST_RUNS.with(|r| r.borrow().clone()) {
        return d;
    }
    support_dir().join("terminal-runs")
}

#[cfg(test)]
thread_local! {
    /// Where a test's runs go instead of the support folder.
    static TEST_RUNS: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}

pub fn dir(id: &str) -> PathBuf {
    runs_dir().join(id)
}

/// What the command asks for.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Ask {
    /// The key of the session whose agent asks.
    pub session: String,
    pub command: String,
    /// Where it runs: the folder the command was run in, unless it named another.
    pub cwd: PathBuf,
}

/// A request held for you to answer.
#[derive(Clone, Debug, PartialEq)]
pub struct Asked {
    /// The id of the run it starts, which an answer names.
    pub id: String,
    pub ask: Ask,
    pub at: i64,
}

/// Why a command can't be asked for, when it can't: nothing in it, too long, or with characters
/// that don't show where you'd read it (runcode.rs `unseen`), so what ran wouldn't be what you saw.
pub fn refused(command: &str) -> Option<&'static str> {
    if command.trim().is_empty() {
        Some("There's no command to run.")
    } else if command.len() > LONGEST {
        Some("That command is too long to ask for: put it in a script file and run the file.")
    } else if crate::runcode::unseen(command) {
        Some("That command has characters that don't show where its user reads it, so it isn't asked for: what ran wouldn't be what they saw.")
    } else {
        None
    }
}

/// A run, as its folder records it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Run {
    pub id: String,
    /// The session whose agent asked, by key, and its title then.
    pub session: String,
    pub session_title: String,
    pub command: String,
    pub cwd: PathBuf,
    /// The terminal app its tab opened in.
    pub terminal: String,
    pub asked_at: i64,
    pub allowed_at: i64,
}

/// Where a run stands.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum State {
    /// Its tab is open, and its shell hasn't run it yet.
    Starting,
    Running,
    /// It ended, with its exit status in `exit`.
    Exited,
    /// It ended without saying how: its tab was closed, or its processes were killed.
    Ended,
}

/// A run with where it stands, as the command prints it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RunStatus {
    #[serde(flatten)]
    pub run: Run,
    pub state: State,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    /// When it ended, when it has.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<i64>,
}


// What the app holds while you answer.

/// How a request was answered, for the command that waits on it.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    Started(Run),
    /// No, with what to do instead when you said.
    Denied(Option<String>),
    /// Allowed, and its tab couldn't be opened.
    Failed(String),
}

struct Held {
    asked: Asked,
    tx: Sender<Outcome>,
}

static HELD: Mutex<Vec<Held>> = Mutex::new(Vec::new());

fn held() -> MutexGuard<'static, Vec<Held>> {
    HELD.lock().unwrap_or_else(|e| e.into_inner())
}

/// The request a session waits on you for, the oldest when it has several.
pub fn of_session(key: &str) -> Option<Asked> {
    held().iter().map(|h| &h.asked).filter(|a| a.ask.session == key).min_by_key(|a| a.at).cloned()
}

/// The line the windows show for a request, as they show a tool call's: "Bash · `rm -rf build`".
pub fn label(a: &Asked) -> String {
    format!("Run in your terminal · {}", code(&one_line(&a.ask.command, 160)))
}

/// A request as what the session waits on you for, its tab to open in `terminal`.
pub fn pending(a: &Asked, terminal: &str) -> api::Pending {
    api::Pending {
        id: a.id.clone(),
        label: label(a),
        input: Some(a.ask.command.clone()),
        terminal: Some(api::TerminalRun { cwd: tilde(&a.ask.cwd), app: terminal.to_string() }),
        ..Default::default()
    }
}

/// Lays a held request over the session that asked, as the engine reads it: it needs you, for
/// this. The time changes with each new request, so a panel showing the session asks again what
/// it waits on.
pub fn overlay(s: &mut Session) {
    let Some(a) = of_session(&s.key) else { return };
    let since = if s.status == Status::Needs { s.status_since.unwrap_or(0).max(a.at) } else { a.at };
    s.status = Status::Needs;
    s.status_since = Some(since);
    s.excerpt = Some(label(&a));
    s.stalled_since = None;
    s.asks = false;
}

/// Holds a request as a connection would, for other modules' tests; the receiver hears its answer.
#[cfg(test)]
pub fn held_for_tests(asked: Asked) -> std::sync::mpsc::Receiver<Outcome> {
    let (tx, rx) = channel();
    held().push(Held { asked, tx });
    rx
}

/// Takes a request out of the held ones, for one answer to have it: the request `id` of the
/// session `key`, or with no key, whoever's it is.
fn claim(id: &str, key: Option<&str>) -> Option<Held> {
    let mut held = held();
    let at = held.iter().position(|h| h.asked.id == id && key.is_none_or(|k| h.asked.ask.session == k))?;
    Some(held.remove(at))
}

/// Says no to the session `key`'s request `id`, with what to do instead when there are words.
/// None when no such request is held.
pub fn deny(key: &str, id: &str, words: Option<&str>) -> Option<String> {
    let h = claim(id, Some(key))?;
    let _ = h.tx.send(Outcome::Denied(words.map(str::trim).filter(|w| !w.is_empty()).map(String::from)));
    Some("Denied. Its agent is told no.".to_string())
}

/// Allows the session `key`'s request `id`: `open` opens its tab and starts it, here, so a tab
/// that won't open is said where you allowed it. None when no such request is held.
pub fn allow(key: &str, id: &str, open: impl FnOnce(&Asked) -> Result<Run>) -> Option<Result<String>> {
    let h = claim(id, Some(key))?;
    Some(match open(&h.asked) {
        Ok(run) => {
            let said = format!("Opened a {} tab and ran it there. What it prints goes back to its agent.", run.terminal);
            let _ = h.tx.send(Outcome::Started(run));
            Ok(said)
        }
        Err(e) => {
            let _ = h.tx.send(Outcome::Failed(words(anyhow!("{e:#}"))));
            Err(e)
        }
    })
}

/// Holds a request on this connection until it's answered, then writes the reply line the command
/// waits for (`reply`). One whose command went first goes. `changed` tells the engine the board
/// should show it, or stop.
pub fn hold(stream: &mut UnixStream, asked: Asked, changed: &dyn Fn()) {
    let (tx, rx) = channel();
    let id = asked.id.clone();
    held().push(Held { asked, tx });
    changed();
    let outcome = loop {
        match rx.recv_timeout(Duration::from_millis(400)) {
            Ok(o) => break Some(o),
            Err(RecvTimeoutError::Disconnected) => break None,
            Err(RecvTimeoutError::Timeout) => {}
        }
        if crate::server::hung_up(stream) {
            // Gone unanswered, unless an answer took it meanwhile: then its tab may be opening,
            // and the run goes on without the command, which can find it with `terminal list`.
            if claim(&id, None).is_some() {
                break None;
            }
        }
    };
    held().retain(|h| h.asked.id != id);
    changed();
    if let Some(o) = outcome {
        let _ = write_reply(stream, &reply(&o));
    }
}

/// What the command is told of an answer.
pub fn reply(o: &Outcome) -> Value {
    match o {
        Outcome::Started(run) => json!({ "answer": "allowed", "run": run }),
        Outcome::Denied(words) => json!({ "answer": "denied", "message": words }),
        Outcome::Failed(why) => json!({ "answer": "failed", "message": why }),
    }
}

fn write_reply(stream: &mut UnixStream, result: &Value) -> std::io::Result<()> {
    let mut line = json!({ "ok": true, "result": result }).to_string();
    line.push('\n');
    stream.write_all(line.as_bytes())?;
    stream.flush()
}

/// The time the last id given stands for. Ids go up from it, so two requests in one millisecond,
/// each on a connection's thread of its own, never share one: an answer to one would run the other.
static LAST_ID: AtomicU64 = AtomicU64::new(0);

/// A request's id: the time it came, so runs list in order, and never a folder's that's there.
pub fn new_id() -> String {
    loop {
        let now = now_ms().max(1) as u64;
        let next = |last: u64| last.max(now - 1) + 1;
        let at = match LAST_ID.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |last| Some(next(last))) {
            Ok(last) | Err(last) => next(last),
        };
        let id = format!("{PREFIX}{}", base36(at));
        if !dir(&id).exists() {
            return id;
        }
    }
}

fn base36(mut n: u64) -> String {
    const DIGITS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut out = Vec::new();
    loop {
        out.push(DIGITS[(n % 36) as usize]);
        n /= 36;
        if n == 0 {
            break;
        }
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

// Starting a run.

/// Opens a tab for an allowed request and has it run the command there: its folder written, then
/// a line typed into the new tab's shell that runs the wrapper. The tab goes beside the terminals
/// in its folder's lanes (`board::terminal_neighbours`), named like a session's.
pub fn start(asked: &Asked, apps: &AppNames, lanes: &[Lane], session_title: &str) -> Result<Run> {
    let cwd = &asked.ask.cwd;
    if !cwd.is_absolute() || !cwd.is_dir() {
        bail!("It would run in {}, which isn't there any more.", tilde(cwd));
    }
    prune(now_ms());
    let run = Run {
        id: asked.id.clone(),
        session: asked.ask.session.clone(),
        session_title: session_title.to_string(),
        command: asked.ask.command.clone(),
        cwd: cwd.clone(),
        terminal: crate::launch::terminal_name(&apps.terminal_id),
        asked_at: asked.at,
        allowed_at: now_ms(),
    };
    let d = dir(&run.id);
    write_folder(&d, &run).with_context(|| format!("writing {}", d.display()))?;
    let near = crate::board::terminal_neighbours(lanes, cwd);
    let label = crate::board::tab_label(lanes, cwd, &format!("Run {}", one_line(&run.command, 60)));
    let place = crate::launch::Placement { new_terminal: apps.new_terminal, near: &near, behind: false, label: label.as_deref() };
    let env = [(ENV.to_string(), d.to_string_lossy().into_owned())];
    if let Err(e) = crate::launch::terminal_run(&apps.terminal_id, place, cwd, TYPED, &env) {
        let _ = std::fs::remove_dir_all(&d);
        return Err(e);
    }
    Ok(run)
}

/// What the new tab's shell gets, set in its environment: the run's folder.
const ENV: &str = "CODEALONG_RUN";
/// The line typed into the new tab's shell. The leading space keeps it out of a history that
/// skips such lines, as a session's launcher line is (launch.rs).
const TYPED: &str = " sh \"$CODEALONG_RUN/run.sh\"\n";

fn write_folder(d: &Path, run: &Run) -> Result<()> {
    std::fs::create_dir_all(runs_dir())?;
    let _ = std::fs::set_permissions(runs_dir(), std::fs::Permissions::from_mode(0o700));
    std::fs::create_dir(d)?;
    std::fs::set_permissions(d, std::fs::Permissions::from_mode(0o700))?;
    let write = |name: &str, text: &str| -> Result<()> {
        let mut f = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(d.join(name))?;
        f.write_all(text.as_bytes())?;
        Ok(())
    };
    write(RECORD, &serde_json::to_string_pretty(run)?)?;
    write(COMMAND, &format!("{}\n", run.command.trim_end_matches('\n')))?;
    write(WRAPPER, &wrapper(d, &run.session_title))?;
    Ok(())
}

/// The script the tab runs: it notes its processes (whole, so a look meanwhile never finds the
/// file half written), shows what runs and for whom, runs the command under `script` in your shell
/// as a new tab starts it (a login shell, and interactive, for your aliases and functions;
/// `/bin/sh` with the tab's environment when your shell isn't one these words suit, fish), and
/// notes how it ended. The command is read from its file, so nothing in it is quoted twice.
pub fn wrapper(d: &Path, session_title: &str) -> String {
    let d = sh_quote(&d.to_string_lossy());
    let says = sh_quote(&format!("Codealong runs this for {}, as you allowed:", if session_title.trim().is_empty() { "an agent" } else { session_title.trim() }));
    format!(
        r#"# A command an agent asked to run in your terminal, as you allowed it (Codealong's terminal.rs).
d={d}
printf '%s %s\n' "$$" "$PPID" > "$d/{PIDS}.new" && mv -f "$d/{PIDS}.new" "$d/{PIDS}"
printf '\n\033[2m%s\033[0m\n' {says}
sed 's/^/  /' "$d/{COMMAND}"
printf '\n'
case "${{SHELL##*/}}" in
  zsh|bash|ksh|sh) run="$SHELL" ;;
  *) run=/bin/sh ;;
esac
CODEALONG_RUN_DIR="$d" /usr/bin/script -q -F "$d/{OUTPUT}" "$run" -l -i -c 'printf %s "$$" > "$CODEALONG_RUN_DIR/{SHELL}"; eval "$(cat "$CODEALONG_RUN_DIR/{COMMAND}")"'
status=$?
printf '%s\n' "$status" > "$d/{EXIT}"
printf '\n\033[2m%s %s. %s\033[0m\n' 'It exited with status' "$status" 'This tab is yours again.'
"#
    )
}

/// Removes the folders of runs that ended more than a week ago.
pub fn prune(now: i64) {
    let Ok(entries) = std::fs::read_dir(runs_dir()) else { return };
    for e in entries.flatten() {
        let d = e.path();
        let Some(id) = d.file_name().and_then(|n| n.to_str()).filter(|n| is_id(n)) else { continue };
        let old = match status(id) {
            Ok(s) => s.state != State::Running && s.ended_at.unwrap_or(s.run.allowed_at) < now - KEEP_MS,
            // A folder its record never made it into.
            Err(_) => modified(&d).is_some_and(|m| m < now - KEEP_MS),
        };
        if old {
            let _ = std::fs::remove_dir_all(&d);
        }
    }
}

fn modified(p: &Path) -> Option<i64> {
    let t = std::fs::metadata(p).ok()?.modified().ok()?;
    Some(t.duration_since(std::time::UNIX_EPOCH).ok()?.as_millis() as i64)
}

// Where a run stands.

pub fn record(id: &str) -> Result<Run> {
    let p = dir(id).join(RECORD);
    let text = std::fs::read_to_string(&p).with_context(|| format!("There's no run {id}."))?;
    serde_json::from_str(&text).with_context(|| format!("{} doesn't read", p.display()))
}

/// A pid noted in a run's folder, while that process is still the one noted. Each was running when
/// its file was written (the wrapper notes itself and the shell that ran it, the command's shell
/// itself), and started once the run was allowed: one that started after the file was written is
/// another that took the number since, and one from before the run is no process of the run's.
fn noted_pid(d: &Path, file: &str, field: usize, since: i64) -> Option<u32> {
    let p = d.join(file);
    let text = std::fs::read_to_string(&p).ok()?;
    let pid: u32 = text.split_whitespace().nth(field)?.parse().ok()?;
    let written = modified(&p)?;
    let started = procs::proc_start_ms(pid)?;
    (started >= since - 2_000 && started <= written).then_some(pid)
}

pub fn status(id: &str) -> Result<RunStatus> {
    let run = record(id)?;
    let d = dir(id);
    let exit = std::fs::read_to_string(d.join(EXIT)).ok().and_then(|t| t.trim().parse::<i32>().ok());
    let (state, ended_at) = match exit {
        Some(_) => (State::Exited, modified(&d.join(EXIT))),
        None if !d.join(PIDS).exists() => (State::Starting, None),
        None if noted_pid(&d, PIDS, 0, run.allowed_at).is_some() => (State::Running, None),
        // It may have ended between the two looks.
        None => match std::fs::read_to_string(d.join(EXIT)).ok().and_then(|t| t.trim().parse::<i32>().ok()) {
            Some(code) => return Ok(RunStatus { run, state: State::Exited, exit_code: Some(code), ended_at: modified(&d.join(EXIT)) }),
            None => (State::Ended, modified(&d.join(OUTPUT)).or_else(|| modified(&d.join(PIDS)))),
        },
    };
    Ok(RunStatus { run, state, exit_code: exit, ended_at })
}

/// Every run kept, the newest first; only those `session` asked for when it names one.
pub fn list(session: Option<&str>) -> Vec<RunStatus> {
    let Ok(entries) = std::fs::read_dir(runs_dir()) else { return Vec::new() };
    let mut out: Vec<RunStatus> = entries
        .flatten()
        .filter_map(|e| e.file_name().to_str().filter(|n| is_id(n)).map(String::from))
        .filter_map(|id| status(&id).ok())
        .filter(|s| session.is_none_or(|k| s.run.session == k))
        .collect();
    out.sort_by_key(|s| std::cmp::Reverse(s.run.allowed_at));
    out
}

/// The run an id or a unique start of one names, with or without its `run-`.
pub fn resolve(given: &str) -> Result<String> {
    let given = given.trim();
    let want = if is_id(given) { given.to_string() } else { format!("{PREFIX}{given}") };
    let ids: Vec<String> = std::fs::read_dir(runs_dir())
        .map(|es| es.flatten().filter_map(|e| e.file_name().to_str().map(String::from)).filter(|n| is_id(n)).collect())
        .unwrap_or_default();
    if ids.contains(&want) {
        return Ok(want);
    }
    let hits: Vec<&String> = ids.iter().filter(|i| i.starts_with(&want)).collect();
    match hits.as_slice() {
        [one] => Ok((*one).clone()),
        [] => bail!("There's no run {given}: `codealong terminal list` lists them."),
        _ => bail!("{given} could be more than one run: give more of its id."),
    }
}

// Reading what it printed.

/// What a run printed, as lines a screen ends up showing.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Output {
    /// The lines, each with its number among them all, oldest first.
    pub lines: Vec<(usize, String)>,
    /// How many lines it printed in all, as far as was read.
    pub total: usize,
    /// Only the end was read, being long: what came before is left out.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub cut: bool,
    /// With `grep`: how many lines matched, of which `lines` are the last.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub matches: Option<usize>,
}

/// The `last` lines a run printed before its newest `skip`, or with `grep` the last `last` lines
/// that contain it (case aside) among those.
pub fn output(id: &str, last: usize, skip: usize, grep: Option<&str>) -> Result<Output> {
    let p = dir(id).join(OUTPUT);
    let (bytes, cut) = match read_end(&p, READ_BYTES) {
        Ok(read) => read,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (Vec::new(), false),
        Err(e) => return Err(e).with_context(|| format!("reading {}", p.display())),
    };
    Ok(pick(render(&bytes), last, skip, grep, cut))
}

/// How much a run has printed so far, in bytes: what a wait for new output watches.
pub fn printed_bytes(id: &str) -> u64 {
    std::fs::metadata(dir(id).join(OUTPUT)).map(|m| m.len()).unwrap_or(0)
}

/// `output`'s choice among rendered lines.
fn pick(lines: Vec<String>, last: usize, skip: usize, grep: Option<&str>, cut: bool) -> Output {
    let total = lines.len();
    let numbered = lines.into_iter().enumerate().map(|(i, l)| (i + 1, l)).take(total.saturating_sub(skip));
    let (lines, matches): (Vec<(usize, String)>, Option<usize>) = match grep.map(str::to_lowercase).filter(|g| !g.is_empty()) {
        Some(g) => {
            let hits: Vec<(usize, String)> = numbered.filter(|(_, l)| l.to_lowercase().contains(&g)).collect();
            let n = hits.len();
            (hits.into_iter().skip(n.saturating_sub(last)).collect(), Some(n))
        }
        None => {
            let all: Vec<(usize, String)> = numbered.collect();
            let n = all.len();
            (all.into_iter().skip(n.saturating_sub(last)).collect(), None)
        }
    };
    Output { lines, total, cut, matches }
}

/// The last `most` bytes of a file, from the first whole line in them, and whether that left any
/// out.
fn read_end(p: &Path, most: u64) -> std::io::Result<(Vec<u8>, bool)> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(p)?;
    let len = f.metadata()?.len();
    if len <= most {
        let mut out = Vec::new();
        f.read_to_end(&mut out)?;
        return Ok((out, false));
    }
    f.seek(SeekFrom::Start(len - most))?;
    let mut out = Vec::new();
    f.read_to_end(&mut out)?;
    let start = out.iter().position(|&b| b == b'\n').map_or(0, |i| i + 1);
    Ok((out.split_off(start), true))
}

/// The widest a screen is taken to be, for where a cursor can be moved.
const WIDEST: usize = 512;
/// The most `render` makes of what it reads.
const RENDERED_MOST: usize = 32 << 20;
/// The line that ends a rendering cut short at `RENDERED_MOST`.
const TOO_MUCH: &str = "[… the rest is left out: drawn as a screen shows it, it's too long]";

/// The lines a terminal would end up showing for what a program printed, a line at a time: text
/// and its colors' escapes, a carriage return going back to the line's start (a progress line
/// drawn over itself), erasing to the line's end, backspaces and tabs. Other escapes, which move
/// about the whole screen, are left out.
pub fn render(bytes: &[u8]) -> Vec<String> {
    let text = String::from_utf8_lossy(bytes);
    let mut out = Vec::new();
    // What's been rendered so far, which spaces a cursor moved past can make far more than what
    // was read: past `RENDERED_MOST`, the rest is left out, and a line says so.
    let mut size = 0usize;
    let mut line: Vec<char> = Vec::new();
    let mut col = 0usize;
    let mut chars = text.chars().peekable();
    let put = |line: &mut Vec<char>, col: &mut usize, c: char| {
        if *col < line.len() {
            line[*col] = c;
        } else {
            line.resize(*col, ' ');
            line.push(c);
        }
        *col += 1;
    };
    while let Some(c) = chars.next() {
        match c {
            '\n' => {
                let done = line.iter().collect::<String>().trim_end().to_string();
                size += done.len() + 1;
                out.push(done);
                line.clear();
                col = 0;
                if size > RENDERED_MOST {
                    out.push(TOO_MUCH.to_string());
                    return out;
                }
            }
            '\r' => col = 0,
            '\x08' => col = col.saturating_sub(1),
            '\t' => {
                put(&mut line, &mut col, ' ');
                while col % 8 != 0 {
                    put(&mut line, &mut col, ' ');
                }
            }
            '\x1b' => match chars.next() {
                // CSI: parameters, intermediates, then the final byte that says what it does.
                Some('[') => {
                    let mut params = String::new();
                    let mut last = None;
                    for c in chars.by_ref() {
                        if ('\x40'..='\x7e').contains(&c) {
                            last = Some(c);
                            break;
                        }
                        params.push(c);
                    }
                    let n = |default: usize| params.trim_start_matches('?').split(';').next().and_then(|p| p.parse::<usize>().ok()).unwrap_or(default);
                    // A move never takes the cursor past a wide screen's edge, or the line's end
                    // when it's longer: `ESC[1000000000G` and a character after it would make a
                    // line of a billion spaces.
                    let reach = line.len().max(WIDEST);
                    match last {
                        Some('K') => match n(0) {
                            0 => line.truncate(col),
                            1 => line.iter_mut().take(col + 1).for_each(|c| *c = ' '),
                            _ => line.clear(),
                        },
                        Some('G') => col = n(1).saturating_sub(1).min(reach),
                        Some('C') => col = col.saturating_add(n(1).max(1)).min(reach),
                        Some('D') => col = col.saturating_sub(n(1).max(1)),
                        _ => {}
                    }
                }
                // OSC: up to BEL or ESC \.
                Some(']') => {
                    while let Some(c) = chars.next() {
                        if c == '\x07' || (c == '\x1b' && chars.next_if_eq(&'\\').is_some()) {
                            break;
                        }
                    }
                }
                // DCS, SOS, PM, APC: up to ESC \.
                Some('P' | 'X' | '^' | '_') => {
                    while let Some(c) = chars.next() {
                        if c == '\x1b' && chars.next_if_eq(&'\\').is_some() {
                            break;
                        }
                    }
                }
                // A character set: one more character.
                Some('(' | ')' | '*' | '+' | '#' | '%') => {
                    chars.next();
                }
                _ => {}
            },
            c if c.is_control() => {}
            c => put(&mut line, &mut col, c),
        }
    }
    if !line.is_empty() {
        out.push(line.iter().collect::<String>().trim_end().to_string());
    }
    out
}

// Stopping it.

/// Interrupts a running command as Ctrl-C in its tab would: the process group in the foreground
/// of `script`'s terminal gets SIGINT. Says how that went, after a moment for it to end.
pub fn stop(id: &str) -> Result<String> {
    let s = status(id)?;
    match s.state {
        State::Starting => bail!("It hasn't started in its tab yet, so there's nothing to stop."),
        State::Exited | State::Ended => return Ok(ended_words(&s)),
        State::Running => {}
    }
    let d = dir(id);
    let shell = noted_pid(&d, SHELL, 0, s.run.allowed_at).ok_or_else(|| anyhow!("It's still starting in its tab; try again in a moment."))?;
    let group = procs::foreground_group(shell).unwrap_or(shell);
    // SAFETY: kill with a negative pid signals that process group.
    if unsafe { libc::kill(-(group as libc::pid_t), libc::SIGINT) } != 0 {
        bail!("Couldn't interrupt it: {}", std::io::Error::last_os_error());
    }
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(5) {
        std::thread::sleep(Duration::from_millis(100));
        let now = status(id)?;
        if now.state != State::Running {
            return Ok(format!("Sent it Ctrl-C. {}", ended_words(&now)));
        }
    }
    Ok(format!("Sent it Ctrl-C, and it's still running: it may take a while to stop, or ignore Ctrl-C. `codealong terminal stop {id} --close` closes its tab, which ends it."))
}

fn ended_words(s: &RunStatus) -> String {
    match (s.state, s.exit_code) {
        (State::Exited, Some(code)) => format!("It exited with status {code}."),
        (State::Ended, _) => "It ended without an exit status: its tab was closed, or it was killed.".to_string(),
        _ => "It has ended.".to_string(),
    }
}

/// Closes a run's tab, ending what runs in it, as the tab's close button does: here in the app,
/// which scripts the terminal. Only while the tab runs nothing of yours (`only_the_run`), looked at
/// just before it's closed.
pub fn close(id: &str) -> Result<String> {
    let s = status(id)?;
    let d = dir(id);
    let tab = noted_pid(&d, PIDS, 1, s.run.allowed_at).ok_or_else(|| anyhow!("Its tab is closed already."))?;
    let wrapper = noted_pid(&d, PIDS, 0, s.run.allowed_at);
    let tty = procs::tty(tab).ok_or_else(|| anyhow!("Its tab is closed already."))?;
    let tty_name = tty.to_string_lossy().trim_start_matches("/dev/").to_string();
    let listed = procs::run("ps", &["-t", &tty_name, "-o", "pid="], None, Duration::from_secs(10))?;
    let on_tty: Vec<u32> = listed.split_whitespace().filter_map(|p| p.parse().ok()).collect();
    let name = procs::parent_and_name(tab).map(|(_, n)| n).unwrap_or_default();
    let the_runs = |p: u32| wrapper.is_some_and(|w| procs::ancestors(p).contains(&w));
    if !only_the_run(&name, &on_tty, &procs::ancestors(tab), the_runs) {
        bail!("Something else runs in its tab now, so it stays open: its user may have started it there.");
    }
    match crate::launch::close_session_terminals(&[(tab, s.run.cwd.as_path())]).into_iter().next() {
        Some(Ok(())) => Ok("Closed its tab.".to_string()),
        Some(Err(e)) => Err(e),
        None => bail!("Couldn't close its tab."),
    }
}

/// The shells a tab runs at its prompt.
const SHELLS: &[&str] = &["zsh", "bash", "sh", "ksh", "dash", "fish", "tcsh", "csh", "nu", "xonsh", "elvish"];

/// Whether a run's tab runs nothing of yours: its shell is still a shell (`exec vim` there would
/// make it vim), and every process on its terminal is that shell, what it runs under, or the run's
/// wrapper and what that runs. A job you left running in the background there is on the terminal
/// too.
fn only_the_run(tab_name: &str, on_tty: &[u32], tab_line: &[u32], the_runs: impl Fn(u32) -> bool) -> bool {
    SHELLS.contains(&tab_name.trim_start_matches('-')) && on_tty.iter().all(|p| tab_line.contains(p) || the_runs(*p))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_progress_line_drawn_over_itself_reads_as_it_ends() {
        let printed = b"\x1b[1m$ build\x1b[0m\r\nfetching\r\n\rprogress 1/3\rprogress 2/3\rprogress 3/3\r\ndone\x1b[K\n\x1b]0;title\x07after\ttab\nshort\x1b[1Gx\n";
        assert_eq!(render(printed), vec!["$ build", "fetching", "progress 3/3", "done", "after   tab", "xhort"]);
    }

    #[test]
    fn erasing_and_moving_on_a_line_keeps_what_shows() {
        assert_eq!(render(b"50% \r\x1b[K100%\n"), vec!["100%"]);
        assert_eq!(render(b"abcdef\x1b[3D\x1b[K\n"), vec!["abc"]);
        assert_eq!(render(b"abc\x1b[2Kx\n"), vec!["   x"], "a whole line erased keeps where the cursor was");
        assert_eq!(render(b"no newline at the end"), vec!["no newline at the end"]);
        assert_eq!(render(b"\x1bP1;2|data\x1b\\kept\x1b(Bmore\n"), vec!["keptmore"]);
        assert_eq!(render(b"bell\x07 and nul\x00\n"), vec!["bell and nul"]);
    }

    #[test]
    fn lines_are_picked_from_the_end_and_by_what_they_say() {
        let lines: Vec<String> = (1..=10).map(|i| format!("line {i}")).collect();
        let o = pick(lines.clone(), 3, 0, None, false);
        assert_eq!(o.lines, vec![(8, "line 8".into()), (9, "line 9".into()), (10, "line 10".into())]);
        assert_eq!((o.total, o.matches), (10, None));
        let o = pick(lines.clone(), 2, 5, None, false);
        assert_eq!(o.lines, vec![(4, "line 4".into()), (5, "line 5".into())], "skipping the newest five");
        let o = pick(lines.clone(), 5, 0, Some("LINE 1"), false);
        assert_eq!(o.lines, vec![(1, "line 1".into()), (10, "line 10".into())]);
        assert_eq!(o.matches, Some(2));
        let o = pick(lines, 1, 0, Some("line 1"), false);
        assert_eq!(o.lines, vec![(10, "line 10".into())], "the last of the matches");
        assert_eq!(o.matches, Some(2));
    }

    #[test]
    fn only_a_command_that_shows_as_it_runs_is_asked_for() {
        assert!(refused("gcloud auth login").is_none());
        assert!(refused("cd app &&\nnpm run dev").is_none());
        assert!(refused("  ").is_some());
        assert!(refused("ls\u{1b}[2J").is_some());
        assert!(refused("echo \u{202E}txt.exe").is_some());
        assert!(refused(&"x".repeat(LONGEST + 1)).is_some());
    }

    #[test]
    fn a_request_shows_as_what_its_session_waits_on() {
        let a = Asked { id: "run-abc".into(), ask: Ask { session: "codex:codex:1".into(), command: "gcloud auth login\n# then".into(), cwd: crate::util::home().join("app") }, at: 5 };
        let p = pending(&a, "Ghostty");
        assert_eq!(p.label, "Run in your terminal · `gcloud auth login`");
        assert_eq!(p.input.as_deref(), Some("gcloud auth login\n# then"));
        assert_eq!(p.terminal, Some(api::TerminalRun { cwd: "~/app".into(), app: "Ghostty".into() }));
        assert!(!p.always && !p.no_once && p.cannot.is_none());
        let v = serde_json::to_value(&p).unwrap();
        assert_eq!(v["terminal"]["app"], "Ghostty");
        assert!(v.get("always").is_none(), "{v}");
    }

    #[test]
    fn a_held_request_makes_its_session_wait_on_you_until_answered() {
        let key = "claude:claude:terminal-overlay-test";
        let mut s = Session { status: Status::Working, status_since: Some(1), excerpt: Some("working".into()), ..crate::fixtures::session(key) };
        let (tx, rx) = channel();
        let id = "run-test-overlay".to_string();
        held().push(Held { asked: Asked { id: id.clone(), ask: Ask { session: key.into(), command: "make deploy".into(), cwd: "/tmp".into() }, at: 50 }, tx });
        overlay(&mut s);
        assert_eq!((s.status, s.status_since, s.excerpt.as_deref()), (Status::Needs, Some(50), Some("Run in your terminal · `make deploy`")));
        assert!(deny("claude:claude:another", &id, None).is_none(), "another session's key doesn't answer it");
        assert_eq!(deny(key, &id, Some("  use the staging target ")).as_deref(), Some("Denied. Its agent is told no."));
        assert_eq!(rx.recv().unwrap(), Outcome::Denied(Some("use the staging target".into())));
        assert!(deny(key, &id, None).is_none(), "answered once");
        let mut again = Session { status: Status::Working, ..crate::fixtures::session(key) };
        overlay(&mut again);
        assert_eq!(again.status, Status::Working);
    }

    #[test]
    fn a_tab_that_wont_open_is_said_where_it_was_allowed() {
        let (tx, rx) = channel();
        let id = "run-test-fails".to_string();
        held().push(Held { asked: Asked { id: id.clone(), ask: Ask { session: "k".into(), command: "x".into(), cwd: "/tmp".into() }, at: 1 }, tx });
        let r = allow("k", &id, |_| Err(anyhow!("Ghostty isn't installed."))).unwrap();
        assert_eq!(r.unwrap_err().to_string(), "Ghostty isn't installed.");
        assert_eq!(rx.recv().unwrap(), Outcome::Failed("Ghostty isn't installed.".into()));
        assert_eq!(reply(&Outcome::Failed("no".into())), json!({ "answer": "failed", "message": "no" }));
    }

    /// A real run, opt-in: the wrapper as a new tab's shell runs it, in a tmux session of its own
    /// that nobody sees, with `script` and your login shell (its startup files run). A password
    /// typed at a prompt that doesn't echo it stays out of what's kept, a progress line drawn over
    /// itself reads as it ended, the exit status is kept, and `stop` interrupts a command as Ctrl-C
    /// does. `cargo test a_command_runs_in_a_tab_under_script -- --ignored`.
    #[test]
    #[ignore]
    fn a_command_runs_in_a_tab_under_script_and_is_read_and_stopped() {
        let scratch = crate::fixtures::scratch("terminal-runs");
        TEST_RUNS.with(|r| *r.borrow_mut() = Some(scratch.to_path_buf()));
        let tmux = |args: &[&str]| std::process::Command::new("tmux").args(args).env_remove("TMUX").output().unwrap();
        let begin = |id: &str, command: &str| {
            let run = Run { id: id.into(), session: "k".into(), session_title: "A test".into(), command: command.into(), cwd: scratch.to_path_buf(), terminal: "tmux".into(), asked_at: now_ms(), allowed_at: now_ms() };
            write_folder(&dir(id), &run).unwrap();
            // As the tab has it: the folder in the shell's environment, then the line typed.
            let line = format!("export {ENV}={}; {}; sleep 60", sh_quote(&dir(id).to_string_lossy()), TYPED.trim());
            assert!(tmux(&["new-session", "-d", "-s", id, "-x", "100", "-y", "20", &line]).status.success());
        };
        let until = |id: &str, want: &dyn Fn(&RunStatus) -> bool| {
            let start = Instant::now();
            loop {
                let s = status(id).unwrap();
                if want(&s) || start.elapsed() > Duration::from_secs(30) {
                    return s;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        };

        let id = format!("run-test-real-{}", std::process::id());
        begin(&id, "printf 'secret: '; read -rs x; echo; echo \"got ${#x} letters\"; for i in 1 2 3; do printf '\\rstep %s/3' $i; sleep 0.1; done; echo; echo done; exit 4");
        until(&id, &|_| std::fs::read_to_string(dir(&id).join(OUTPUT)).is_ok_and(|o| o.contains("secret:")));
        tmux(&["send-keys", "-t", &id, "hunter2", "Enter"]);
        let s = until(&id, &|s| s.state == State::Exited);
        tmux(&["kill-session", "-t", &id]);
        assert_eq!((s.state, s.exit_code), (State::Exited, Some(4)));
        let o = output(&id, 50, 0, None).unwrap();
        let text: Vec<&str> = o.lines.iter().map(|(_, l)| l.as_str()).collect();
        assert!(text.contains(&"got 7 letters") && text.contains(&"step 3/3") && text.contains(&"done"), "{text:?}");
        assert!(!std::fs::read_to_string(dir(&id).join(OUTPUT)).unwrap().contains("hunter2"), "what was typed isn't kept");

        let id = format!("run-test-stop-{}", std::process::id());
        begin(&id, "echo waiting; sleep 30; echo never");
        // Once `sleep` is in front, as it would be when you pressed Ctrl-C.
        let in_front = |s: &RunStatus| {
            let shell = std::fs::read_to_string(dir(&s.run.id).join(SHELL)).ok().and_then(|p| p.trim().parse::<u32>().ok());
            shell.is_some_and(|p| procs::foreground_group(p).is_some_and(|g| g != p))
        };
        until(&id, &|s| s.state == State::Running && in_front(s));
        let said = stop(&id).unwrap();
        // A loaded Mac may take longer than `stop` waits.
        let s = until(&id, &|s| s.state != State::Running);
        tmux(&["kill-session", "-t", &id]);
        assert!(said.starts_with("Sent it Ctrl-C."), "{said}");
        assert_eq!((s.state, s.exit_code), (State::Exited, Some(130)), "as zsh and bash end on Ctrl-C");
        assert!(!output(&id, 50, 0, None).unwrap().lines.iter().any(|(_, l)| l == "never"));
        TEST_RUNS.with(|r| *r.borrow_mut() = None);
    }

    /// A request held on a connection as the app holds it: the reply line the command reads once
    /// it's answered, and a request whose command went first gone from the session.
    #[test]
    fn a_held_request_replies_to_the_command_that_asked_or_goes_with_it() {
        use std::io::{BufRead, BufReader};
        let key = "claude:claude:terminal-hold-test";
        let ask = |id: &str| Asked { id: id.into(), ask: Ask { session: key.into(), command: "echo hi".into(), cwd: "/tmp".into() }, at: 7 };
        let wait_held = |id: &str| {
            let start = Instant::now();
            while !held().iter().any(|h| h.asked.id == id) && start.elapsed() < Duration::from_secs(5) {
                std::thread::sleep(Duration::from_millis(10));
            }
        };
        let changes = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));

        let (mut app, command) = UnixStream::pair().unwrap();
        let counted = changes.clone();
        let holding = std::thread::spawn(move || hold(&mut app, ask("run-test-hold-deny"), &|| {
            counted.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }));
        wait_held("run-test-hold-deny");
        assert!(deny(key, "run-test-hold-deny", Some("not now")).is_some());
        let mut line = String::new();
        BufReader::new(&command).read_line(&mut line).unwrap();
        holding.join().unwrap();
        let reply: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(reply, json!({ "ok": true, "result": { "answer": "denied", "message": "not now" } }));
        assert_eq!(changes.load(std::sync::atomic::Ordering::SeqCst), 2, "shown, then taken away");

        let (mut app, command) = UnixStream::pair().unwrap();
        let holding = std::thread::spawn(move || hold(&mut app, ask("run-test-hold-allow"), &|| {}));
        wait_held("run-test-hold-allow");
        let run = Run { id: "run-test-hold-allow".into(), session: key.into(), session_title: "T".into(), command: "echo hi".into(), cwd: "/tmp".into(), terminal: "Ghostty".into(), asked_at: 7, allowed_at: 9 };
        let opened = run.clone();
        assert_eq!(allow(key, "run-test-hold-allow", move |_| Ok(opened)).unwrap().unwrap(), "Opened a Ghostty tab and ran it there. What it prints goes back to its agent.");
        let mut line = String::new();
        BufReader::new(&command).read_line(&mut line).unwrap();
        holding.join().unwrap();
        let reply: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(reply["result"]["answer"], "allowed");
        assert_eq!(serde_json::from_value::<Run>(reply["result"]["run"].clone()).unwrap(), run);

        let (mut app, command) = UnixStream::pair().unwrap();
        let holding = std::thread::spawn(move || hold(&mut app, ask("run-test-hold-gone"), &|| {}));
        wait_held("run-test-hold-gone");
        drop(command);
        holding.join().unwrap();
        assert!(of_session(key).is_none(), "withdrawn with its command");
        assert!(deny(key, "run-test-hold-gone", None).is_none());
    }

    #[test]
    fn requests_asked_at_once_never_share_an_id() {
        let threads: Vec<_> = (0..8).map(|_| std::thread::spawn(|| (0..200).map(|_| new_id()).collect::<Vec<_>>())).collect();
        let ids: Vec<String> = threads.into_iter().flat_map(|t| t.join().unwrap()).collect();
        let unique: std::collections::HashSet<&String> = ids.iter().collect();
        assert_eq!(unique.len(), ids.len());
    }

    #[test]
    fn a_noted_pid_counts_only_while_it_is_the_process_that_was_noted() {
        let d = crate::fixtures::scratch("terminal-pids");
        let me = std::process::id();
        let started = procs::proc_start_ms(me).unwrap();
        std::fs::write(d.join(PIDS), format!("{me} 1\n")).unwrap();
        assert_eq!(noted_pid(&d, PIDS, 0, started), Some(me));
        assert_eq!(noted_pid(&d, PIDS, 0, started + 60_000), None, "one that started before the run isn't the run's");
        // A file written before the process started was about another that had the number.
        let f = std::fs::File::options().write(true).open(d.join(PIDS)).unwrap();
        f.set_modified(std::time::UNIX_EPOCH + Duration::from_millis((started - 60_000) as u64)).unwrap();
        assert_eq!(noted_pid(&d, PIDS, 0, started - 120_000), None);
    }

    #[test]
    fn a_tab_is_closed_only_while_it_runs_nothing_of_yours() {
        let (login, tab, wrapper, script, theirs) = (100, 200, 300, 301, 400);
        let line = [tab, login, 1];
        let runs = |p: u32| p == wrapper || p == script;
        assert!(only_the_run("zsh", &[login, tab], &line, runs), "at its prompt");
        assert!(only_the_run("-zsh", &[login, tab, wrapper, script], &line, runs), "running the run");
        assert!(!only_the_run("zsh", &[login, tab, theirs], &line, runs), "a job of yours there");
        assert!(!only_the_run("vim", &[login, tab], &line, runs), "`exec vim` in it");
    }

    #[test]
    fn a_cursor_moved_far_off_makes_no_long_line() {
        let far = render(b"\x1b[1000000000Gx\n");
        assert!(far[0].len() <= WIDEST + 1 && far[0].ends_with('x'), "{}", far[0].len());
        let many = render(format!("{}x\n", "\x1b[600C".repeat(10_000)).as_bytes());
        assert!(many[0].len() <= WIDEST + 1, "{}", many[0].len());
        let long = "y".repeat(2_000);
        assert_eq!(render(format!("{long}\x1b[5000Cz\n").as_bytes())[0], format!("{long}z"), "no further than the line's end");
    }

    #[test]
    fn ids_sort_by_when_they_were_asked() {
        assert_eq!(base36(0), "0");
        assert_eq!(base36(35), "z");
        assert_eq!(base36(36), "10");
        assert!(base36(1_790_000_000_000) < base36(1_790_000_000_001));
        assert!(is_id(&new_id()));
    }

    /// The wrapper as a tab's shell runs it, in a scratch folder with a stub for `script`'s
    /// place: it notes its processes, runs the command from its file with the arguments `script`
    /// would get, and keeps the exit status.
    #[test]
    fn the_wrapper_runs_the_command_from_its_file_and_keeps_its_status() {
        let d = crate::fixtures::scratch("terminal-wrapper");
        std::fs::write(d.join(COMMAND), "printf '%s\\n' \"it's $((40 + 2))\"; shopt -q login_shell && case $- in *i*) echo 'a login shell, interactive';; esac; exit 3\n").unwrap();
        let text = wrapper(&d, "Fix the 'build'").replace("/usr/bin/script -q -F", "sh -c 'shift; \"$@\"' script");
        // sh, so the test doesn't run your own shell's startup files.
        let out = std::process::Command::new("/bin/sh").arg("-c").arg(&text).env("SHELL", "/bin/sh").output().unwrap();
        let shown = String::from_utf8_lossy(&out.stdout);
        assert!(shown.contains("Codealong runs this for Fix the 'build', as you allowed:"), "{shown}");
        assert!(shown.contains("  printf '%s\\n' \"it's $((40 + 2))\"; shopt"), "{shown}");
        assert!(shown.contains("it's 42") && shown.contains("It exited with status 3."), "{shown}");
        assert!(shown.contains("a login shell, interactive"), "as a new tab's shell is: {shown}");
        assert_eq!(std::fs::read_to_string(d.join(EXIT)).unwrap().trim(), "3");
        let pids = std::fs::read_to_string(d.join(PIDS)).unwrap();
        assert_eq!(pids.split_whitespace().count(), 2, "{pids}");
        assert!(!d.join(format!("{PIDS}.new")).exists(), "written whole, then moved into place");
        assert!(std::fs::read_to_string(d.join(SHELL)).unwrap().parse::<u32>().is_ok());
    }
}
