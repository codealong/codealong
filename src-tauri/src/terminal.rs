//! Commands an agent asks to run in a terminal of yours (`codealong terminal run`): you allow the
//! exact command, it runs in a new tab of your terminal beside the session's, as you (your shell,
//! its settings and your sign-ins, outside any sandbox), and what it prints goes back to the agent.
//! The Claude app's Code tab does the same in its own Terminal panel (its `run_in_terminal` tool);
//! this is it for every harness, in the terminal you use. The commands you run from a session
//! (`!` in its box, Run on a block: actions.rs `run_command`, runcode.rs) run the same way, so
//! every tab Codealong opens for a command is one it can tell the state of, and close.
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
//! command's status. The line typed into the new tab `exec`s the wrapper (`TYPED`, `wrapper`), so
//! the tab is the run's until you take a shell in it with Return once it has ended, and an agent
//! may close it only till then (`close`): nothing outside a shell can tell its prompt from a
//! `read` of yours. Unless you take a shell there, the tab closes by itself a while after the
//! command ends (`[apps] close_command_tabs_after`, `wrapper`). Each run has a folder (`dir`) with the request, the command, the wrapper, its
//! processes, what it printed, and its exit status once it ends. Runs are kept a week (`prune`).
//! An agent's sandbox may keep its command from looking at your processes, so the command asks the
//! app where a run stands, and to stop or close it, when it's up (server.rs).
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
/// Made as a run's tab is claimed, once the run has ended: by the wrapper as you take a shell in
/// the tab with Return, or by an agent's close. Each makes it with one `mkdir`, which only one can
/// do, so a close never ends a shell you took the moment before (`claim_for_close`).
const CLAIM: &str = "claim";
/// In the claim, when a close made it.
const CLOSING: &str = "closing";

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
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Run {
    pub id: String,
    /// The session whose agent asked, or that you ran it for, by key, and its title then. Empty
    /// for a block you ran with Run (runcode.rs), which no session waits on or is told about.
    pub session: String,
    pub session_title: String,
    pub command: String,
    pub cwd: PathBuf,
    /// The terminal app its tab opened in.
    pub terminal: String,
    pub asked_at: i64,
    pub allowed_at: i64,
    /// You ran it yourself for this session, with `!` in its box or Run on a `!` block (actions.rs
    /// `run_command`): the session is told what it printed once it ends (`report_ended`). An
    /// agent's run has none: its agent waits on it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub reports: bool,
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
/// this, since it came. Each request's time is its own (`new_id`), and differs from the time its
/// harness says the session began waiting, so a panel showing the session asks again what it waits
/// on as one request gives way to the next, or to what its harness asks.
pub fn overlay(s: &mut Session) {
    let Some(a) = of_session(&s.key) else { return };
    s.status = Status::Needs;
    s.status_since = Some(a.at);
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

/// A request's id and the time it came, which no other request has either: runs list in order, an
/// id is never a folder's that's there, and a session's next request begins waiting at a time of
/// its own, so a panel that showed the one before asks what it waits on again.
pub fn new_id() -> (String, i64) {
    loop {
        let now = now_ms().max(1) as u64;
        let next = |last: u64| last.max(now - 1) + 1;
        let at = match LAST_ID.try_update(Ordering::SeqCst, Ordering::SeqCst, |last| Some(next(last))) {
            Ok(last) | Err(last) => next(last),
        };
        let id = format!("{PREFIX}{}", base36(at));
        if !dir(&id).exists() {
            return (id, at as i64);
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

/// Opens a tab for an allowed request, a command you ran for a session (`reports`), or one you ran
/// from a block (no session), and has it run the command there: its folder written, then a line
/// typed into the new tab's shell that runs the wrapper. The tab goes beside the terminals in its
/// folder's lanes (`board::terminal_neighbours`), named like a session's, and closes as
/// `[apps] close_command_tabs_after` says once the command ends.
pub fn start(asked: &Asked, apps: &AppNames, lanes: &[Lane], session_title: &str, reports: bool) -> Result<Run> {
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
        reports,
    };
    let d = dir(&run.id);
    let minutes = crate::config::current().map_or(crate::config::CLOSE_COMMAND_TABS_AFTER, |c| c.apps.close_command_tabs_after);
    write_folder(&d, &run, Duration::from_secs(u64::from(minutes) * 60)).with_context(|| format!("writing {}", d.display()))?;
    let near = crate::board::terminal_neighbours(lanes, cwd);
    let label = crate::board::tab_label(lanes, cwd, &format!("Run {}", one_line(&run.command, 60)));
    let place = crate::launch::Placement { new_terminal: apps.new_terminal, near: &near, behind: false, label: label.as_deref() };
    let env = [(ENV.to_string(), d.to_string_lossy().into_owned())];
    if let Err(e) = crate::launch::terminal_run(&apps.terminal_id, place, cwd, TYPED, &env) {
        let _ = std::fs::remove_dir_all(&d);
        return Err(e);
    }
    if reports {
        to_report().push(run.id.clone());
    }
    Ok(run)
}

/// What the new tab's shell gets, set in its environment: the run's folder.
const ENV: &str = "CODEALONG_RUN";
/// The line typed into the new tab's shell, which it replaces with the wrapper: the tab is then the
/// run's until you take a shell in it (`close`). The leading space keeps it out of a history that
/// skips such lines, as a session's launcher line is (launch.rs).
const TYPED: &str = " exec sh \"$CODEALONG_RUN/run.sh\"\n";

/// A run's folder, its wrapper closing the tab `closes_after` the command ends (zero: never).
fn write_folder(d: &Path, run: &Run, closes_after: Duration) -> Result<()> {
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
    write(WRAPPER, &wrapper(d, run, closes_after))?;
    Ok(())
}

/// The script the tab runs, as its shell: it notes its processes (whole, so a look meanwhile never
/// finds the file half written), shows what runs and for whom, runs the command under `script` in
/// your shell as a new tab starts it (a login shell, and interactive, for your aliases and
/// functions; `/bin/sh` with the tab's environment when your shell isn't one these words suit,
/// fish), and notes how it ended. Then it waits for Return, which gives you a login shell of your
/// own in the tab once it has claimed the tab (`CLAIM`), so the tab isn't closed under you; when a
/// close claimed it first, the tab is closing, and Return ends the wrapper. Unless `closes_after`
/// is zero, a closer started as the command ends claims the tab as a close does once that's up,
/// and hangs up the wrapper, which closes the tab as `close` does; your Return first stops it,
/// quietly (a shell says so when what it waits on is killed). It sleeps rather than `read -t`
/// timing out: macOS's `/bin/sh` can be dash, whose `read` has no `-t`. The command is sourced
/// from its file, so nothing in it is quoted twice, and runs a command at a time as typed lines
/// do: zsh parses all of an `eval`'s text first, so an alias one line defines wouldn't work on the
/// next.
pub fn wrapper(d: &Path, run: &Run, closes_after: Duration) -> String {
    let d = sh_quote(&d.to_string_lossy());
    let title = run.session_title.trim();
    let says = sh_quote(&match (run.session.is_empty(), run.reports, title.is_empty()) {
        (true, _, _) => "Codealong runs this as you asked:".to_string(),
        (false, true, true) => "Codealong runs this as you asked; what it prints goes to the session when it ends:".to_string(),
        (false, true, false) => format!("Codealong runs this for {title}, as you asked; what it prints goes to it when it ends:"),
        (false, false, true) => "Codealong runs this for an agent, as you allowed:".to_string(),
        (false, false, false) => format!("Codealong runs this for {title}, as you allowed:"),
    });
    let secs = closes_after.as_secs();
    let (ended, closer, kept) = if secs == 0 {
        (sh_quote("Press Return for a shell in this tab."), String::new(), String::new())
    } else {
        (
            sh_quote(&format!("This tab closes in {}, unless you press Return for a shell in it.", in_words(secs))),
            format!("(sleep {secs} && mkdir \"$d/{CLAIM}\" && : > \"$d/{CLAIM}/{CLOSING}\" && kill -HUP $$) 2>/dev/null &\ncloser=$!\n"),
            "  pkill -P \"$closer\" 2>/dev/null; kill \"$closer\" 2>/dev/null\n".to_string(),
        )
    };
    format!(
        r#"# A command run in your terminal, as you allowed or asked (Codealong's terminal.rs).
d={d}
printf '%s %s\n' "$$" "$PPID" > "$d/{PIDS}.new" && mv -f "$d/{PIDS}.new" "$d/{PIDS}"
printf '\n\033[2m%s\033[0m\n' {says}
sed 's/^/  /' "$d/{COMMAND}"
printf '\n'
case "${{SHELL##*/}}" in
  zsh|bash|ksh|sh) run="$SHELL" ;;
  *) run=/bin/sh ;;
esac
CODEALONG_RUN_DIR="$d" /usr/bin/script -q -F "$d/{OUTPUT}" "$run" -l -i -c 'printf %s "$$" > "$CODEALONG_RUN_DIR/{SHELL}"; . "$CODEALONG_RUN_DIR/{COMMAND}"'
status=$?
printf '%s\n' "$status" > "$d/{EXIT}"
printf '\n\033[2m%s %s. %s\033[0m ' 'It exited with status' "$status" {ended}
{closer}if read -r _ && mkdir "$d/{CLAIM}" 2>/dev/null; then
{kept}  exec "${{SHELL:-/bin/zsh}}" -l
fi
"#
    )
}

/// How long until a run's tab closes, in words: "a minute", "10 minutes", "an hour".
fn in_words(secs: u64) -> String {
    match secs {
        1 => "a second".into(),
        s if s < 60 => format!("{s} seconds"),
        60 => "a minute".into(),
        3600 => "an hour".into(),
        s if s % 3600 == 0 => format!("{} hours", s / 3600),
        s => format!("{} minutes", s.div_ceil(60)),
    }
}

/// Removes the folders of runs that ended more than a week ago.
pub fn prune(now: i64) {
    let Ok(entries) = std::fs::read_dir(runs_dir()) else { return };
    for e in entries.flatten() {
        let d = e.path();
        let Some(id) = d.file_name().and_then(|n| n.to_str()).filter(|n| is_id(n)) else { continue };
        let old = match status(id) {
            // Its wrapper may wait for Return still, in a tab left open: it hands the tab over
            // through this folder.
            Ok(s) => s.state != State::Running && s.ended_at.unwrap_or(s.run.allowed_at) < now - KEEP_MS && noted(&d, PIDS, 0, s.run.allowed_at) == Noted::Gone,
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

/// A run's folder, for an id that names one: `run-` and letters, digits and dashes only, so no id
/// reaches outside the runs' folder, and a folder there, not a link out of it. The app takes ids
/// from its socket, where an agent in a sandbox could otherwise point it at files of its own making
/// and have it signal a process of yours.
fn run_folder(id: &str) -> Result<PathBuf> {
    let plain = id.strip_prefix(PREFIX).is_some_and(|rest| !rest.is_empty() && rest.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'));
    let d = dir(id);
    match std::fs::symlink_metadata(&d) {
        Ok(m) if plain && m.is_dir() => Ok(d),
        _ => bail!("There's no run {id}."),
    }
}

pub fn record(id: &str) -> Result<Run> {
    let p = run_folder(id)?.join(RECORD);
    let text = std::fs::read_to_string(&p).with_context(|| format!("There's no run {id}."))?;
    serde_json::from_str(&text).with_context(|| format!("{} doesn't read", p.display()))
}

/// A pid noted in a run's folder, while that process is still the one noted. Each was running when
/// its file was written (the wrapper notes itself and the shell that ran it, the command's shell
/// itself), and started once the run was allowed: one that started after the file was written is
/// another that took the number since, and one from before the run is no process of the run's.
fn noted_pid(d: &Path, file: &str, field: usize, since: i64) -> Option<u32> {
    match noted(d, file, field, since) {
        Noted::Is(pid) => Some(pid),
        Noted::Unknown | Noted::Gone => None,
    }
}

/// What became of a process noted in a run's folder.
#[derive(Debug, PartialEq)]
enum Noted {
    Is(u32),
    /// A process has its number, which this one may not look at: an agent's sandbox keeps its
    /// command from looking at your processes. It isn't taken for gone.
    Unknown,
    Gone,
}

/// `noted_pid`, saying when the process can't be looked at.
fn noted(d: &Path, file: &str, field: usize, since: i64) -> Noted {
    let p = d.join(file);
    let pid = std::fs::read_to_string(&p).ok().and_then(|t| t.split_whitespace().nth(field)?.parse::<u32>().ok());
    let (Some(pid), Some(written)) = (pid, modified(&p)) else { return Noted::Gone };
    match procs::proc_start_ms(pid) {
        Some(started) if started >= since - 2_000 && started <= written => Noted::Is(pid),
        Some(_) => Noted::Gone,
        None if procs::pid_alive(pid) => Noted::Unknown,
        None => Noted::Gone,
    }
}

pub fn status(id: &str) -> Result<RunStatus> {
    let run = record(id)?;
    let d = run_folder(id)?;
    let exit = std::fs::read_to_string(d.join(EXIT)).ok().and_then(|t| t.trim().parse::<i32>().ok());
    let (state, ended_at) = match exit {
        Some(_) => (State::Exited, modified(&d.join(EXIT))),
        None if !d.join(PIDS).exists() => (State::Starting, None),
        None if noted(&d, PIDS, 0, run.allowed_at) != Noted::Gone => (State::Running, None),
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
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
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
    let p = run_folder(id)?.join(OUTPUT);
    let (bytes, cut) = match read_end(&p, READ_BYTES) {
        Ok(read) => read,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (Vec::new(), false),
        Err(e) => return Err(e).with_context(|| format!("reading {}", p.display())),
    };
    Ok(pick(render(&bytes), last, skip, grep, cut))
}

/// How much a run has printed so far, in bytes: what a wait for new output watches.
pub fn printed_bytes(id: &str) -> u64 {
    run_folder(id).ok().and_then(|d| std::fs::metadata(d.join(OUTPUT)).ok()).map_or(0, |m| m.len())
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
const RENDERED_MOST: usize = 16 << 20;
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
        // The line being drawn counts too: one with no end can be the most of it.
        if size + line.len() > RENDERED_MOST {
            out.push(line.iter().collect::<String>().trim_end().to_string());
            out.push(TOO_MUCH.to_string());
            return out;
        }
        match c {
            '\n' => {
                let done = line.iter().collect::<String>().trim_end().to_string();
                size += done.len() + 1;
                out.push(done);
                line.clear();
                col = 0;
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

// Telling the session what a run of yours printed.

/// The runs whose session is told what they printed once they end (`Run::reports`), by id.
static TO_REPORT: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn to_report() -> MutexGuard<'static, Vec<String>> {
    TO_REPORT.lock().unwrap_or_else(|e| e.into_inner())
}

/// In a run's folder once its session was told what it printed, or telling it was tried.
const REPORTED: &str = "reported";
/// How many of its last lines a session is told, and how much of each.
const REPORT_LINES: usize = 60;
const REPORT_LINE_CHARS: usize = 400;

/// Tells each run's session what it printed once it ends, for as long as the app runs: the runs
/// started since (`start`), and those that ended, or were still running, while the app was closed
/// or relaunching. `tell` sends the words to the session, as yours, once `ready` says the board has
/// the sessions: after a relaunch, its first scan may not have them yet.
pub fn report_ended(ready: impl Fn() -> bool + Send + 'static, tell: impl Fn(&RunStatus, String) + Send + 'static) {
    let left: Vec<String> = list(None).into_iter().filter(|s| s.run.reports && !dir(&s.run.id).join(REPORTED).exists()).map(|s| s.run.id).collect();
    to_report().extend(left);
    let _ = std::thread::Builder::new().name("terminal-reports".into()).spawn(move || loop {
        std::thread::sleep(Duration::from_secs(2));
        if !ready() {
            continue;
        }
        let ids = to_report().clone();
        for id in ids {
            let ended = match status(&id) {
                Ok(s) if matches!(s.state, State::Exited | State::Ended) => Some(s),
                Ok(_) => continue,
                // Its folder is gone.
                Err(_) => None,
            };
            to_report().retain(|i| *i != id);
            let Some(s) = ended else { continue };
            // Noted first, so a relaunch while it's told doesn't tell it twice.
            if std::fs::write(dir(&id).join(REPORTED), "").is_err() {
                continue;
            }
            let printed = output(&id, REPORT_LINES, 0, None).unwrap_or_default();
            tell(&s, report_text(&s, &printed));
        }
    });
}

/// What a session is told of a run you ran for it: the command, how it ended, and the end of what
/// it printed, each in a code block, and how to read the rest. It's yours, so it reads as yours.
pub fn report_text(s: &RunStatus, printed: &Output) -> String {
    let ended = match (s.state, s.exit_code) {
        (State::Exited, Some(code)) => format!("It exited with status {code}"),
        _ => "It ended without an exit status (its tab was closed, or it was killed)".to_string(),
    };
    let lines: Vec<String> = printed.lines.iter().map(|(_, l)| crate::util::truncate(l, REPORT_LINE_CHARS)).collect();
    let clipped = printed.lines.iter().any(|(_, l)| l.chars().count() > REPORT_LINE_CHARS);
    let what = match (lines.len(), printed.total) {
        (0, _) => ", printing nothing.".to_string(),
        (n, total) if n < total || printed.cut => format!(". The last {n} lines it printed:\n\n{}", fenced(&lines.join("\n"))),
        _ => format!(". What it printed:\n\n{}", fenced(&lines.join("\n"))),
    };
    let more = if lines.len() < printed.total || printed.cut || clipped { format!("\n\nAll of it: `codealong terminal read {} --lines 2000`.", s.run.id) } else { String::new() };
    format!("I ran this in my terminal:\n\n{}\n\n{ended}{what}{more}", fenced(&s.run.command))
}

/// Text in a code block whose fence no run of backticks in it closes.
fn fenced(text: &str) -> String {
    let longest = text.split(|c| c != '`').map(str::len).max().unwrap_or(0);
    let fence = "`".repeat(longest.max(2) + 1);
    format!("{fence}\n{}\n{fence}", text.trim_end())
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
    let d = run_folder(id)?;
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

/// Closes a run's tab: hangs up the wrapper, which is the tab's shell (the line typed `exec`s it),
/// and what it runs, as closing the tab would, and the terminal closes the tab as it does one whose
/// shell has ended. Only while the tab is the run's: until you take a shell in it (`CLAIM`),
/// everything in the tab is the wrapper and what it runs, and what you typed went to the command or
/// to the wrapper's own Return. Looked at, then claimed, just before the hangup, with nothing slow
/// between.
pub fn close(id: &str) -> Result<String> {
    let s = status(id)?;
    if s.state == State::Starting {
        bail!("It hasn't started in its tab yet, so its tab stays.");
    }
    let d = run_folder(id)?;
    if taken(&d) {
        bail!("{TAKEN}");
    }
    let wrapper = noted_pid(&d, PIDS, 0, s.run.allowed_at).ok_or_else(|| anyhow!("Its tab is closed already."))?;
    let tty = procs::tty(wrapper).ok_or_else(|| anyhow!("Its tab is closed already."))?;
    let tty_name = tty.to_string_lossy().trim_start_matches("/dev/").to_string();
    let listed = procs::run("ps", &["-t", &tty_name, "-o", "pid="], None, Duration::from_secs(10))?;
    let on_tty: Vec<u32> = listed.split_whitespace().filter_map(|p| p.parse().ok()).collect();
    let above = procs::ancestors(wrapper);
    if !only_the_run(&on_tty, &above, |p| procs::ancestors(p).contains(&wrapper)) {
        bail!("Something of your user's may run in its tab now, so the tab stays open: ask them to close it.");
    }
    claim_for_close(&d)?;
    // The tab's shell started its own group, which the wrapper and `script` are in; the command's
    // shell, on `script`'s terminal, is hung up as `script` ends.
    // SAFETY: kill with a negative pid signals that process group.
    if unsafe { libc::kill(-(wrapper as libc::pid_t), libc::SIGHUP) } != 0 && unsafe { libc::kill(wrapper as libc::pid_t, libc::SIGHUP) } != 0 {
        bail!("Couldn't close its tab: {}", std::io::Error::last_os_error());
    }
    let started = Instant::now();
    while procs::pid_alive(wrapper) && started.elapsed() < Duration::from_secs(5) {
        std::thread::sleep(Duration::from_millis(100));
    }
    Ok(if procs::pid_alive(wrapper) {
        "Hung up its tab, which hasn't ended yet.".to_string()
    } else {
        "Its tab's shell has ended, so the tab closes as your terminal closes one whose shell ended (a terminal set to keep such tabs keeps it).".to_string()
    })
}

/// Why a tab stays when you took a shell in it.
const TAKEN: &str = "Your user took a shell in its tab once it ended, so the tab is theirs and stays open: ask them to close it.";

/// Whether you took a shell in a run's tab: the wrapper made the claim, not a close.
fn taken(d: &Path) -> bool {
    d.join(CLAIM).is_dir() && !d.join(CLAIM).join(CLOSING).exists()
}

/// Claims a run's tab for closing, or says it's yours: the claim is one `mkdir`, as the wrapper's
/// is when you press Return, so whichever comes first has the tab. A close that claimed it before
/// may claim it again.
fn claim_for_close(d: &Path) -> Result<()> {
    match std::fs::create_dir(d.join(CLAIM)) {
        Ok(()) => {
            std::fs::write(d.join(CLAIM).join(CLOSING), "")?;
            Ok(())
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists && !taken(d) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => bail!("{TAKEN}"),
        Err(e) => Err(e.into()),
    }
}

/// Whether a run's tab is still the run's, once its shell is known to be the wrapper still (no
/// shell taken there, `CLAIM`): every process on its terminal is the wrapper, what it runs, or what
/// it runs under. Its name says nothing: macOS's `/bin/sh` runs as the shell
/// `/private/var/select/sh` names, `bash` here.
fn only_the_run(on_tty: &[u32], above: &[u32], under_the_wrapper: impl Fn(u32) -> bool) -> bool {
    on_tty.iter().all(|p| above.contains(p) || under_the_wrapper(*p))
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
    /// itself reads as it ended, the exit status is kept, `stop` interrupts a command as Ctrl-C
    /// does, `close` ends a tab still the run's, and not once you took a shell in it, and a tab
    /// closes by itself once its wait is up. `cargo test a_command_runs_in_a_tab_under_script --
    /// --ignored`.
    #[test]
    #[ignore]
    fn a_command_runs_in_a_tab_under_script_and_is_read_and_stopped() {
        let scratch = crate::fixtures::scratch("terminal-runs");
        TEST_RUNS.with(|r| *r.borrow_mut() = Some(scratch.to_path_buf()));
        let tmux = |args: &[&str]| std::process::Command::new("tmux").args(args).env_remove("TMUX").output().unwrap();
        let begin = |id: &str, command: &str| {
            let run = Run { id: id.into(), session: "k".into(), session_title: "A test".into(), command: command.into(), cwd: scratch.to_path_buf(), terminal: "tmux".into(), asked_at: now_ms(), allowed_at: now_ms(), ..Default::default() };
            write_folder(&dir(id), &run, Duration::ZERO).unwrap();
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

        // Closing: in your login zsh, the line typed into it as a new tab's shell gets it, which
        // makes the wrapper the tab's shell. A tab still the run's closes; one you took a shell in
        // once it ended stays.
        let tab_with = |id: &str, closes_after: Duration| {
            let run = Run { id: id.into(), session: "k".into(), session_title: "A test".into(), command: "echo closing".into(), cwd: scratch.to_path_buf(), terminal: "tmux".into(), asked_at: now_ms(), allowed_at: now_ms(), ..Default::default() };
            write_folder(&dir(id), &run, closes_after).unwrap();
            assert!(tmux(&["new-session", "-d", "-s", id, "-x", "100", "-y", "20", "/bin/zsh -l"]).status.success());
            let typed = format!("export {ENV}={}; {}", sh_quote(&dir(id).to_string_lossy()), TYPED.trim());
            tmux(&["send-keys", "-t", id, &typed, "Enter"]);
            let s = until(id, &|s| s.state == State::Exited);
            assert_eq!(s.exit_code, Some(0));
            noted_pid(&dir(id), PIDS, 0, run.allowed_at).unwrap()
        };
        let id = format!("run-test-close-{}", std::process::id());
        let tab = tab_with(&id, Duration::ZERO);
        let said = close(&id).unwrap();
        tmux(&["kill-session", "-t", &id]);
        assert!(said.starts_with("Its tab's shell has ended"), "{said}");
        assert!(!procs::pid_alive(tab));

        let id = format!("run-test-taken-{}", std::process::id());
        let tab = tab_with(&id, Duration::ZERO);
        tmux(&["send-keys", "-t", &id, "Enter"]);
        let start = Instant::now();
        while !dir(&id).join(CLAIM).exists() && start.elapsed() < Duration::from_secs(10) {
            std::thread::sleep(Duration::from_millis(100));
        }
        let refused = close(&id).unwrap_err().to_string();
        let alive = procs::pid_alive(tab);
        tmux(&["kill-session", "-t", &id]);
        assert!(refused.starts_with("Your user took a shell in its tab"), "{refused}");
        assert!(alive, "the shell you took there, under the same pid");

        // Left alone, the tab closes by itself once its wait is up: the tmux session, whose shell
        // the wrapper was, ends.
        let id = format!("run-test-closes-{}", std::process::id());
        let tab = tab_with(&id, Duration::from_secs(2));
        let start = Instant::now();
        while tmux(&["has-session", "-t", &id]).status.success() && start.elapsed() < Duration::from_secs(15) {
            std::thread::sleep(Duration::from_millis(100));
        }
        let open = tmux(&["has-session", "-t", &id]).status.success();
        tmux(&["kill-session", "-t", &id]);
        assert!(!open && !procs::pid_alive(tab), "closed by itself");
        assert!(dir(&id).join(CLAIM).join(CLOSING).exists());
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
        let run = Run { id: "run-test-hold-allow".into(), session: key.into(), session_title: "T".into(), command: "echo hi".into(), cwd: "/tmp".into(), terminal: "Ghostty".into(), asked_at: 7, allowed_at: 9, ..Default::default() };
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
        let threads: Vec<_> = (0..8).map(|_| std::thread::spawn(|| (0..200).map(|_| new_id()).collect::<Vec<(String, i64)>>())).collect();
        let asked: Vec<(String, i64)> = threads.into_iter().flat_map(|t| t.join().unwrap()).collect();
        let ids: std::collections::HashSet<&String> = asked.iter().map(|(id, _)| id).collect();
        let times: std::collections::HashSet<i64> = asked.iter().map(|(_, at)| *at).collect();
        assert_eq!((ids.len(), times.len()), (asked.len(), asked.len()), "nor a time, which a panel goes by");
    }

    /// Two requests from one session: answering the first, the session waits on the second from a
    /// time of its own, so a panel that showed the first asks what it waits on again.
    #[test]
    fn a_sessions_next_request_waits_from_a_time_of_its_own() {
        let key = "claude:claude:terminal-two-requests";
        let (first, at1) = new_id();
        let (second, at2) = new_id();
        let ask = Ask { session: key.into(), command: "make".into(), cwd: "/tmp".into() };
        let _a = held_for_tests(Asked { id: first.clone(), ask: ask.clone(), at: at1 });
        let _b = held_for_tests(Asked { id: second.clone(), ask, at: at2 });
        let waits = || {
            let mut s = Session { status: Status::Needs, status_since: Some(at2 + 60_000), ..crate::fixtures::session(key) };
            overlay(&mut s);
            s.status_since
        };
        assert_eq!(waits(), Some(at1), "the oldest first, at its own time, whatever its harness says");
        deny(key, &first, None);
        assert_eq!(waits(), Some(at2));
        deny(key, &second, None);
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

    /// A close and your Return at the same moment: one `mkdir` each, and the one made first has the
    /// tab. The wrapper's is `mkdir` in sh, as here.
    #[test]
    fn a_tab_goes_to_whichever_claims_it_first() {
        let return_key = |d: &Path| std::process::Command::new("/bin/sh").arg("-c").arg(format!("mkdir {}/{CLAIM} 2>/dev/null", sh_quote(&d.to_string_lossy()))).status().unwrap().success();
        let d = crate::fixtures::scratch("terminal-claim-close");
        claim_for_close(&d).unwrap();
        assert!(!return_key(&d), "closing: Return ends the wrapper");
        claim_for_close(&d).unwrap();
        let d = crate::fixtures::scratch("terminal-claim-yours");
        assert!(return_key(&d));
        assert_eq!(claim_for_close(&d).unwrap_err().to_string(), TAKEN);
        assert!(taken(&d));
    }

    /// Only an id of a run's own shape, naming a folder in the runs' folder, reaches one: an
    /// agent's sandbox could write a folder of its own elsewhere for the app to signal from.
    #[test]
    fn only_a_run_folder_is_a_runs() {
        let scratch = crate::fixtures::scratch("terminal-run-folders");
        TEST_RUNS.with(|r| *r.borrow_mut() = Some(scratch.join("runs")));
        std::fs::create_dir_all(scratch.join("runs/run-abc123")).unwrap();
        std::fs::create_dir_all(scratch.join("forged")).unwrap();
        std::os::unix::fs::symlink(scratch.join("forged"), scratch.join("runs/run-linked")).unwrap();
        assert!(run_folder("run-abc123").is_ok());
        for id in ["run-linked", "run-missing", "run-../forged", "../forged", "/tmp", "run-", "abc123", "run-a/b"] {
            assert!(run_folder(id).is_err(), "{id}");
        }
        TEST_RUNS.with(|r| *r.borrow_mut() = None);
    }

    #[test]
    fn a_tab_is_closed_only_while_it_is_the_runs() {
        let (login, wrapper, script, shell, theirs) = (100, 200, 201, 300, 400);
        let above = [wrapper, login, 1];
        let under = |p: u32| p == script || p == shell;
        assert!(only_the_run(&[login, wrapper, script], &above, under), "running, or waiting for Return");
        assert!(only_the_run(&[login, wrapper], &above, under), "waiting for Return");
        assert!(!only_the_run(&[login, wrapper, theirs], &above, under), "something else on its terminal");
    }

    #[test]
    fn a_cursor_moved_far_off_makes_no_long_line() {
        let far = render(b"\x1b[1000000000Gx\n");
        assert!(far[0].len() <= WIDEST + 1 && far[0].ends_with('x'), "{}", far[0].len());
        let many = render(format!("{}x\n", "\x1b[600C".repeat(10_000)).as_bytes());
        assert!(many[0].len() <= WIDEST + 1, "{}", many[0].len());
        let long = "y".repeat(2_000);
        assert_eq!(render(format!("{long}\x1b[5000Cz\n").as_bytes())[0], format!("{long}z"), "no further than the line's end");
        // Tabs, each eight wide, with no line's end: 8 MB of them would be 64 MB drawn.
        let tabs = render(format!("{}x", "\t".repeat(RENDERED_MOST / 4)).as_bytes());
        assert_eq!(tabs.last().map(String::as_str), Some(TOO_MUCH));
        assert!(tabs.iter().map(String::len).sum::<usize>() <= RENDERED_MOST + WIDEST + TOO_MUCH.len(), "{}", tabs.iter().map(String::len).sum::<usize>());
    }

    /// What a session is told of a command you ran for it: the command, how it ended, the end of
    /// what it printed in a block no backticks in it close, and where the rest is.
    #[test]
    fn a_session_is_told_what_a_run_of_yours_printed() {
        let run = Run { id: "run-abc".into(), command: "gcloud auth login".into(), reports: true, ..Default::default() };
        let status = |state, exit_code| RunStatus { run: run.clone(), state, exit_code, ended_at: Some(1) };
        let printed = |lines: &[&str], total: usize| Output { lines: lines.iter().enumerate().map(|(i, l)| (total - lines.len() + i + 1, l.to_string())).collect(), total, cut: false, matches: None };
        assert_eq!(
            report_text(&status(State::Exited, Some(0)), &printed(&["You are now logged in as sam@example.com."], 1)),
            "I ran this in my terminal:\n\n```\ngcloud auth login\n```\n\nIt exited with status 0. What it printed:\n\n```\nYou are now logged in as sam@example.com.\n```"
        );
        let text = report_text(&status(State::Exited, Some(1)), &printed(&["see ```this```", "error: denied"], 90));
        assert!(text.contains("It exited with status 1. The last 2 lines it printed:\n\n````\nsee ```this```\nerror: denied\n````"), "{text}");
        assert!(text.ends_with("All of it: `codealong terminal read run-abc --lines 2000`."), "{text}");
        let long = "x".repeat(REPORT_LINE_CHARS + 50);
        let text = report_text(&status(State::Exited, Some(0)), &printed(&[long.as_str()], 1));
        assert!(text.contains(". What it printed:") && text.ends_with("All of it: `codealong terminal read run-abc --lines 2000`."), "a line cut short is read whole there: {text}");
        let text = report_text(&status(State::Ended, None), &Output::default());
        assert!(text.ends_with("It ended without an exit status (its tab was closed, or it was killed), printing nothing."), "{text}");
    }

    #[test]
    fn ids_sort_by_when_they_were_asked() {
        assert_eq!(base36(0), "0");
        assert_eq!(base36(35), "z");
        assert_eq!(base36(36), "10");
        assert!(base36(1_790_000_000_000) < base36(1_790_000_000_001));
        assert!(is_id(&new_id().0));
    }

    /// The wrapper as a tab's shell runs it, in a scratch folder with a stub for `script`'s
    /// place: it notes its processes, runs the command from its file with the arguments `script`
    /// would get, and keeps the exit status.
    #[test]
    fn the_wrapper_runs_the_command_from_its_file_and_keeps_its_status() {
        let d = crate::fixtures::scratch("terminal-wrapper");
        std::fs::write(d.join(COMMAND), "printf '%s\\n' \"it's $((40 + 2))\"; shopt -q login_shell && case $- in *i*) echo 'a login shell, interactive';; esac; exit 3\n").unwrap();
        let run = Run { session: "k".into(), session_title: "Fix the 'build'".into(), ..Default::default() };
        let text = wrapper(&d, &run, Duration::ZERO).replace("/usr/bin/script -q -F", "sh -c 'shift; \"$@\"' script");
        // sh, in a home of the test's own, so none of your startup files run: a login sh reads
        // ~/.profile, an interactive one $ENV. The test's profile shows it was read.
        let home = d.join("home");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(home.join(".profile"), "echo 'read its profile'\n").unwrap();
        let out = std::process::Command::new("/bin/sh").arg("-c").arg(&text).env("SHELL", "/bin/sh").env("HOME", &home).env_remove("ENV").env_remove("BASH_ENV").output().unwrap();
        let shown = String::from_utf8_lossy(&out.stdout);
        assert!(shown.contains("read its profile"), "{shown}");
        assert!(shown.contains("Codealong runs this for Fix the 'build', as you allowed:"), "{shown}");
        assert!(shown.contains("  printf '%s\\n' \"it's $((40 + 2))\"; shopt"), "{shown}");
        assert!(shown.contains("it's 42") && shown.contains("It exited with status 3. Press Return for a shell in this tab."), "{shown}");
        assert!(shown.contains("a login shell, interactive"), "as a new tab's shell is: {shown}");
        assert!(!d.join(CLAIM).exists(), "with nothing to read, no shell is taken");
        assert_eq!(std::fs::read_to_string(d.join(EXIT)).unwrap().trim(), "3");
        let pids = std::fs::read_to_string(d.join(PIDS)).unwrap();
        assert_eq!(pids.split_whitespace().count(), 2, "{pids}");
        assert!(!d.join(format!("{PIDS}.new")).exists(), "written whole, then moved into place");
        assert!(std::fs::read_to_string(d.join(SHELL)).unwrap().parse::<u32>().is_ok());
    }

    /// In zsh, as most Macs' login shell is, the command runs a line at a time as typed lines
    /// would, so an alias one line defines works on the next (an `eval` of the whole text parses it
    /// all first). zsh with a home of the test's own, so none of your startup files run.
    #[test]
    fn a_command_runs_a_line_at_a_time_in_zsh() {
        let d = crate::fixtures::scratch("terminal-wrapper-zsh");
        std::fs::write(d.join(COMMAND), "alias greet='echo hello from an alias'\ngreet\n").unwrap();
        let run = Run { session: "k".into(), session_title: "A test".into(), ..Default::default() };
        let text = wrapper(&d, &run, Duration::ZERO).replace("/usr/bin/script -q -F", "sh -c 'shift; \"$@\"' script");
        let home = d.join("home");
        std::fs::create_dir_all(&home).unwrap();
        let out = std::process::Command::new("/bin/sh").arg("-c").arg(&text).env("SHELL", "/bin/zsh").env("HOME", &home).env("ZDOTDIR", &home).output().unwrap();
        let shown = String::from_utf8_lossy(&out.stdout);
        assert!(shown.contains("hello from an alias"), "{shown}{}", String::from_utf8_lossy(&out.stderr));
        assert_eq!(std::fs::read_to_string(d.join(EXIT)).unwrap().trim(), "0");
    }

    /// Once the command ends, the wrapper closes its tab by itself when the wait is up, having
    /// claimed it as a close does; Return before then takes a shell there, which the wait then
    /// leaves alone. In `sh`, with a stub in `script`'s place, as above, and a pipe for the tab's
    /// keyboard.
    #[test]
    fn the_wrapper_closes_its_tab_after_the_wait_unless_you_took_a_shell_there() {
        use std::os::unix::process::ExitStatusExt;
        use std::process::{Child, Command, Stdio};
        let start = |name: &str| -> (crate::fixtures::Scratch, Child) {
            let d = crate::fixtures::scratch(name);
            std::fs::write(d.join(COMMAND), "echo ran\n").unwrap();
            let run = Run { session: "k".into(), session_title: "A test".into(), ..Default::default() };
            let text = wrapper(&d, &run, Duration::from_secs(1)).replace("/usr/bin/script -q -F", "sh -c 'shift; \"$@\"' script");
            assert!(text.contains("This tab closes in a second, unless you press Return for a shell in it."), "{text}");
            let home = d.join("home");
            std::fs::create_dir_all(&home).unwrap();
            let child = Command::new("/bin/sh")
                .arg("-c")
                .arg(&text)
                .env("SHELL", "/bin/sh")
                .env("HOME", &home)
                .env_remove("ENV")
                .env_remove("BASH_ENV")
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap();
            (d, child)
        };
        let ended = |child: &mut Child, within: Duration| {
            let started = Instant::now();
            loop {
                if let Some(status) = child.try_wait().unwrap() {
                    return Some(status);
                }
                if started.elapsed() > within {
                    return None;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        };

        let (d, mut tab) = start("terminal-wrapper-closes");
        let status = ended(&mut tab, Duration::from_secs(15));
        let _ = tab.kill();
        assert_eq!(status.and_then(|s| s.signal()), Some(libc::SIGHUP), "hung up once the wait was up: {status:?}");
        assert_eq!(std::fs::read_to_string(d.join(EXIT)).unwrap().trim(), "0");
        assert!(d.join(CLAIM).join(CLOSING).exists() && !taken(&d), "claimed as a close claims it");

        let (d, mut tab) = start("terminal-wrapper-taken");
        let mut keys = tab.stdin.take().unwrap();
        keys.write_all(b"\n").unwrap();
        let early = ended(&mut tab, Duration::from_secs(3));
        assert!(early.is_none(), "the shell you took is still there past the wait: {early:?}");
        assert!(taken(&d));
        keys.write_all(b"exit 7\n").unwrap();
        drop(keys);
        let status = ended(&mut tab, Duration::from_secs(10));
        let _ = tab.kill();
        assert_eq!(status.and_then(|s| s.code()), Some(7), "your shell ended as you ended it: {status:?}");
    }

    #[test]
    fn how_long_until_a_tab_closes_reads_as_words() {
        assert_eq!([1, 30, 60, 600, 3600, 7200, 5400].map(in_words), ["a second", "30 seconds", "a minute", "10 minutes", "an hour", "2 hours", "90 minutes"]);
    }
}
