//! Opening things where they live: the terminal profile (Ghostty through AppleScript) and the
//! Claude and Codex deep links. Editors are in editors.rs. See *Apps*.

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use anyhow::{anyhow, bail, Result};

use crate::config::NewTerminal;
use crate::procs;
use crate::util::home;

pub fn terminal_name(id: &str) -> String {
    match id {
        "ghostty" => "Ghostty".to_string(),
        other => other.to_string(),
    }
}

/// Arguments: where it goes (`window`, `front` for a tab in the front window, or `beside` for a
/// tab in the frontmost window holding any of the terminals listed next), those terminal ids one
/// per line (or ""), the folder, the input to type (or ""), then any `KEY=VALUE` environment
/// entries. A tab with no window to go in becomes a window.
const GHOSTTY_NEW: &str = r#"
on run argv
  set placeAt to item 1 of argv
  set nearIds to paragraphs of (item 2 of argv)
  set thePath to item 3 of argv
  set theInput to item 4 of argv
  set theEnv to {}
  if (count of argv) > 4 then set theEnv to items 5 thru -1 of argv
  tell application "Ghostty"
    if theInput is "" then
      set cfg to {initial working directory:thePath}
    else if theEnv is {} then
      set cfg to {initial working directory:thePath, initial input:theInput}
    else
      set cfg to {initial working directory:thePath, initial input:theInput, environment variables:theEnv}
    end if
    set dest to missing value
    if placeAt is "front" then
      if (count of windows) > 0 then set dest to id of front window
    else if placeAt is "beside" then
      -- Windows come front to back, so the one nearest the front wins. Held by id: see GHOSTTY_FOCUS.
      set wids to id of every window
      repeat with wid in wids
        set tids to id of every terminal of window id (contents of wid)
        repeat with t in nearIds
          if tids contains (contents of t) then
            set dest to contents of wid
            exit repeat
          end if
        end repeat
        if dest is not missing value then exit repeat
      end repeat
    end if
    if dest is missing value then
      new window with configuration cfg
    else
      new tab in window id dest with configuration cfg
      try
        activate window window id dest
      end try
    end if
    activate
  end tell
  return "ok"
end run
"#;

/// Three questions give three lists, which only line up if no terminal opened or closed in
/// between, so it asks for the ids again to check, and gives up with nothing rather than pair
/// a terminal with another's folder. The separator is set outside the tell: inside it, `tab`
/// is Ghostty's tab class, and the lines came out joined by the word "tab".
const GHOSTTY_LIST: &str = r#"
on run argv
  set out to ""
  set sep to character id 9
  tell application "Ghostty"
    repeat 3 times
      set ids to id of every terminal
      set dirs to working directory of every terminal
      set names to name of every terminal
      set again to id of every terminal
      if again = ids and (count of dirs) = (count of ids) and (count of names) = (count of ids) then
        repeat with i from 1 to count of ids
          set out to out & (item i of ids) & sep & (item i of dirs) & sep & (item i of names) & linefeed
        end repeat
        return out
      end if
    end repeat
  end tell
  return out
end run
"#;

/// `tag <marker> <seconds>`: waits up to that long for the terminal whose folder contains the
/// marker and focuses it. `id <terminal id>`: focuses that terminal. Returns the focused
/// terminal's id. The wait is timed, not counted: with many terminals open each look takes
/// longer. Once the tab is selected, the rest is best effort, so a slow or refused step can't
/// turn a tab that was found into an error.
///
/// Windows, tabs and terminals are held by id, never by position. Ghostty lists windows front
/// to back, and a loop over them (`repeat with w in windows`) makes `w` "window 3", asked
/// afresh each time it's used: once selecting the tab brought its window forward, "window 3"
/// was the window that had been in front of it, and that one came forward instead.
const GHOSTTY_FOCUS: &str = r#"
on run argv
  set mode to item 1 of argv
  set want to item 2 of argv
  if mode is "tag" then
    set waitFor to (item 3 of argv) as integer
    set started to current date
    -- The clock is read outside `tell`: inside, `current date` goes to Ghostty first.
    repeat
      -- One question, so the id comes with its own folder.
      tell application "Ghostty" to set found to id of every terminal whose working directory contains want
      if found is not {} then exit repeat
      if ((current date) - started) > waitFor then return ""
      delay 0.1
    end repeat
    set want to item 1 of found
  end if
  if focusTerminal(want) then return want
  return ""
end run

on focusTerminal(want)
  tell application "Ghostty"
    set ws to every window whose id of every terminal contains want
    if ws is {} then
      -- In no window (the quick terminal): Ghostty's own focus still brings it forward.
      if not (exists terminal id want) then return false
    else
      set w to item 1 of ws
      set tbs to every tab of w whose id of every terminal contains want
      if tbs is not {} then select tab (item 1 of tbs)
      try
        activate window w
      end try
    end if
    try
      focus terminal id want
    end try
    try
      activate
    end try
  end tell
  return true
end focusTerminal
"#;

/// `<marker> <seconds>`: waits up to that long for the terminal whose folder contains the
/// marker and returns its id, or "" when none shows. Unlike GHOSTTY_FOCUS it brings nothing
/// forward: typing into a session shouldn't take you away from where you are.
const GHOSTTY_FIND: &str = r#"
on run argv
  set want to item 1 of argv
  set waitFor to (item 2 of argv) as integer
  set started to current date
  repeat
    tell application "Ghostty" to set found to id of every terminal whose working directory contains want
    if found is not {} then return item 1 of found
    if ((current date) - started) > waitFor then return ""
    delay 0.1
  end repeat
end run
"#;

/// `<terminal id> <text>`: pastes the text into that terminal, then presses Return. Ghostty
/// pastes it bracketed when the program asked for that (Claude Code and Codex do), so a
/// prompt of several lines arrives whole. Return comes a moment later, so the program has
/// taken the paste as a paste before it sees the key.
const GHOSTTY_TYPE: &str = r#"
on run argv
  set want to item 1 of argv
  set theText to item 2 of argv
  tell application "Ghostty"
    if not (exists terminal id want) then return ""
    input text theText to terminal id want
    delay 0.3
    send key "enter" to terminal id want
  end tell
  return want
end run
"#;

/// How long to wait for Ghostty to show a session's marker. It's there within a fraction of a
/// second normally; on a Mac busy enough to fall seconds behind, a short wait reported tabs as
/// missing that Ghostty then showed.
const GHOSTTY_MARK_WAIT_SECS: &str = "8";

/// How long Codealong waits for Ghostty. It can be slow to answer while it launches, while it's
/// busy, or while macOS asks whether Codealong may control it (asked again after each rebuild).
/// Stopping early doesn't stop Ghostty: it still does what it was asked, so a short wait
/// reported failures for windows that then opened.
const OSASCRIPT_TIMEOUT: Duration = Duration::from_secs(60);

/// What `osascript` says when macOS hasn't let Codealong control Ghostty.
const NOT_ALLOWED: &str = "Codealong isn't allowed to control Ghostty. Turn it on in System Settings → Privacy & Security → Automation.";

fn osascript(script: &str, args: &[&str]) -> Result<String> {
    let mut full = vec!["-"];
    full.extend_from_slice(args);
    procs::run_stdin("/usr/bin/osascript", &full, script, OSASCRIPT_TIMEOUT).map_err(|e| {
        let msg = e.to_string();
        if msg.contains("-1743") || msg.contains("Not authorized") {
            anyhow!(NOT_ALLOWED)
        } else if msg.contains("timed out") {
            anyhow!("Ghostty didn't answer within a minute. It may still open the window; if macOS asked whether Codealong may control Ghostty, allow it and try again.")
        } else {
            anyhow!("Ghostty didn't respond: {msg}")
        }
    })
}

/// Where a new terminal goes: `[apps] new_terminal`, and the folders it belongs beside, nearest
/// first (`board::terminal_neighbours`).
#[derive(Debug, Clone, Copy)]
pub struct Placement<'a> {
    pub new_terminal: NewTerminal,
    pub near: &'a [Vec<String>],
}

/// Opens a new terminal at `path`, optionally typing `input` (a command plus return).
pub fn terminal_open(profile: &str, place: Placement, path: &Path, input: Option<&str>) -> Result<()> {
    match profile {
        "ghostty" => {
            let dir = path.to_string_lossy();
            let (at, beside) = ghostty_placement(place);
            match osascript(GHOSTTY_NEW, &[at, &beside, &dir, input.unwrap_or("")]) {
                Ok(_) => Ok(()),
                Err(e) if input.is_none() => {
                    // Without Automation permission a plain window still works.
                    let arg = format!("--working-directory={dir}");
                    procs::run("/usr/bin/open", &["-na", "Ghostty.app", "--args", &arg], None, Duration::from_secs(10)).map(|_| ()).map_err(|_| e)
                }
                Err(e) => Err(e),
            }
        }
        other => bail!("There's no terminal profile called {other:?} yet. Use \"ghostty\" in config.toml."),
    }
}

/// Opens a new terminal at `path` with `env` set in its shell, and types `input` (a command
/// plus return). The shell expands variables in `input` from `env`.
pub fn terminal_run(profile: &str, place: Placement, path: &Path, input: &str, env: &[(String, String)]) -> Result<()> {
    match profile {
        "ghostty" => {
            let dir = path.to_string_lossy();
            let (at, beside) = ghostty_placement(place);
            let entries: Vec<String> = env.iter().map(|(k, v)| format!("{k}={v}")).collect();
            let mut args: Vec<&str> = vec![at, &beside, &dir, input];
            args.extend(entries.iter().map(String::as_str));
            osascript(GHOSTTY_NEW, &args).map(|_| ())
        }
        other => bail!("There's no terminal profile called {other:?} yet. Use \"ghostty\" in config.toml."),
    }
}

/// GHOSTTY_NEW's first two arguments. For `project`, the terminals already in the nearest
/// folders that have any; with none, or Ghostty not answering the question, a window.
fn ghostty_placement(place: Placement) -> (&'static str, String) {
    match place.new_terminal {
        NewTerminal::Window => ("window", String::new()),
        NewTerminal::Tab => ("front", String::new()),
        NewTerminal::Project => {
            let beside = ghostty_terminals().map(|ts| terminals_beside(&ts, place.near)).unwrap_or_default();
            if beside.is_empty() {
                ("window", String::new())
            } else {
                ("beside", beside.join("\n"))
            }
        }
    }
}

/// The ids of the terminals working in the nearest group of folders that has any.
fn terminals_beside(terminals: &[GhosttyTerminal], near: &[Vec<String>]) -> Vec<String> {
    near.iter()
        .map(|group| {
            terminals.iter().filter(|t| group.iter().any(|f| Path::new(&t.cwd).starts_with(f))).map(|t| t.id.clone()).collect::<Vec<_>>()
        })
        .find(|ids| !ids.is_empty())
        .unwrap_or_default()
}

#[derive(Debug, Clone, PartialEq)]
pub struct GhosttyTerminal {
    pub id: String,
    pub cwd: String,
    pub name: String,
}

fn ghostty_terminals() -> Result<Vec<GhosttyTerminal>> {
    Ok(osascript(GHOSTTY_LIST, &[])?
        .lines()
        .filter_map(|line| {
            let mut parts = line.splitn(3, '\t');
            Some(GhosttyTerminal { id: parts.next()?.to_string(), cwd: parts.next()?.to_string(), name: parts.next().unwrap_or("").to_string() })
        })
        .filter(|t| !t.id.is_empty())
        .collect())
}

/// Focus requests take turns, and one still waiting when a newer one comes in is dropped. On a
/// busy Mac a click takes seconds to land: without turns, one that landed late brought its tab
/// forward over the tab clicked after it, and two clicks on one session each put back the
/// other's mark as the folder.
struct Turns {
    lock: Mutex<()>,
    newest: AtomicU64,
}

impl Turns {
    const fn new() -> Self {
        Turns { lock: Mutex::new(()), newest: AtomicU64::new(0) }
    }

    /// A place in line, taken when the click arrives.
    fn ticket(&self) -> u64 {
        self.newest.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// Waits for the ticket's turn. None when a newer ticket was taken meanwhile.
    fn wait(&self, ticket: u64) -> Option<MutexGuard<'_, ()>> {
        let turn = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        (self.newest.load(Ordering::SeqCst) == ticket).then_some(turn)
    }
}

static FOCUS_TURNS: Turns = Turns::new();

/// Where the unique folders Codealong reports on a session's TTY start.
const MARK_PREFIX: &str = "/tmp/codealong-";

/// Focuses the terminal tab a live session runs in. `Ok(false)` when there's no such tab.
///
/// The tab is found exactly: Codealong reports a unique folder (OSC 7) on the session's TTY,
/// finds the Ghostty terminal showing that folder, selects its tab, then reports the real
/// folder again. Claude Code rewrites tab titles as it works but never the folder, so the mark
/// holds. Where that can't work (tmux, a session outside Ghostty), the tab is matched by title,
/// then by folder, but only when that finds just one.
pub fn terminal_focus_session(profile: &str, pid: u32, title: &str, cwd: &Path) -> Result<bool> {
    if profile != "ghostty" {
        bail!("There's no terminal profile called {profile:?} yet.");
    }
    let ticket = FOCUS_TURNS.ticket();
    let Some(tty) = tty_of(pid) else {
        bail!("This session runs in the background, so it has no terminal tab.");
    };
    let Some(_turn) = FOCUS_TURNS.wait(ticket) else {
        // Clicked again while this one waited: the newer click decides what comes forward.
        return Ok(true);
    };
    let terminals = ghostty_terminals()?;
    // A session Ghostty didn't start (tmux, another terminal) can't show the mark: don't wait.
    let in_ghostty = runs_under(pid, "ghostty");
    let mark = format!("{MARK_PREFIX}{pid}-{}", crate::util::now_ms());
    let mut failed = None;
    if in_ghostty && write_osc7(&tty, &mark).is_ok() {
        let found = osascript(GHOSTTY_FOCUS, &["tag", &mark, GHOSTTY_MARK_WAIT_SECS]);
        let id = found.as_deref().map(str::trim).unwrap_or_default().to_string();
        let real = terminals
            .iter()
            .find(|t| t.id == id)
            .map(|t| t.cwd.clone())
            // A mark left over (the app quit halfway through) isn't the folder to put back.
            .filter(|c| !c.is_empty() && !c.starts_with(MARK_PREFIX))
            .unwrap_or_else(|| cwd.to_string_lossy().into_owned());
        let _ = write_osc7(&tty, &real);
        match found {
            Ok(_) if !id.is_empty() => return Ok(true),
            Ok(_) => {}
            // Ghostty was too slow or refused: say that, unless the fallback finds the tab.
            Err(e) => failed = Some(e),
        }
    }
    match pick_by_title(&terminals, title, cwd) {
        Some(id) => Ok(!osascript(GHOSTTY_FOCUS, &["id", &id])?.trim().is_empty()),
        None => failed.map_or(Ok(false), Err),
    }
}

/// How typing into a session's terminal went, when nothing went wrong while typing.
#[derive(Debug, Clone, PartialEq)]
pub enum Typed {
    /// Typed into the session's tab in this terminal app.
    Into(&'static str),
    /// Nothing was typed, so another way can take it: the session isn't in a terminal Codealong
    /// can type into, or its tab didn't turn up. Why, as the end of a sentence.
    Nowhere(String),
}

/// Types `text` into the terminal a session's process runs in and presses Return: Ghostty
/// (found by an OSC 7 mark, since it has no TTY or PID property) or tmux (by the pane's TTY).
/// An error means typing started and failed, so the text may be in the terminal already:
/// Ghostty carries out what it was asked even after Codealong stopped waiting on it.
pub fn terminal_type_session(profile: &str, pid: u32, cwd: &Path, text: &str) -> Result<Typed> {
    match terminal_kind(pid) {
        Some(HostKind::Tmux) => match tmux_pane_of(pid) {
            Some(pane) => tmux_type(&pane, text).map(|_| Typed::Into("tmux")),
            None => Ok(Typed::Nowhere("Codealong couldn't find its tmux pane".into())),
        },
        Some(HostKind::Ghostty) if profile == "ghostty" => {
            let id = match ghostty_terminal_of(pid, cwd) {
                Ok(Some(id)) => id,
                Ok(None) => return Ok(Typed::Nowhere("Codealong couldn't find its Ghostty tab".into())),
                // Nothing was typed yet, so another way can take it; say why this one didn't.
                Err(e) if e.to_string() == NOT_ALLOWED => return Ok(Typed::Nowhere("Codealong isn't allowed to control Ghostty (System Settings → Privacy & Security → Automation)".into())),
                Err(_) => return Ok(Typed::Nowhere("Ghostty didn't answer in time".into())),
            };
            if osascript(GHOSTTY_TYPE, &[&id, text])?.trim().is_empty() {
                // Its tab closed between finding it and typing: nothing went in.
                return Ok(Typed::Nowhere("its Ghostty tab closed".into()));
            }
            Ok(Typed::Into("Ghostty"))
        }
        Some(HostKind::Ghostty) => Ok(Typed::Nowhere(format!("there's no terminal profile called {profile:?}"))),
        None => Ok(Typed::Nowhere("it isn't in a Ghostty or tmux tab".into())),
    }
}

/// The Ghostty terminal a process runs in, found by writing a unique folder report to its TTY
/// and asking Ghostty which terminal shows it; then the real folder is reported again.
fn ghostty_terminal_of(pid: u32, cwd: &Path) -> Result<Option<String>> {
    let Some(tty) = tty_of(pid) else { return Ok(None) };
    // A mark takes over the TTY's folder report, as focusing's does, so the two never run at
    // once. A send takes no ticket: it mustn't make a click waiting before it lose its turn.
    let _turn = FOCUS_TURNS.lock.lock().unwrap_or_else(|e| e.into_inner());
    let terminals = ghostty_terminals()?;
    let mark = format!("{MARK_PREFIX}{pid}-{}", crate::util::now_ms());
    write_osc7(&tty, &mark)?;
    let found = osascript(GHOSTTY_FIND, &[&mark, GHOSTTY_MARK_WAIT_SECS]);
    let id = found.as_deref().map(str::trim).unwrap_or_default().to_string();
    let real = terminals
        .iter()
        .find(|t| t.id == id)
        .map(|t| t.cwd.clone())
        .filter(|c| !c.is_empty() && !c.starts_with(MARK_PREFIX))
        .unwrap_or_else(|| cwd.to_string_lossy().into_owned());
    let _ = write_osc7(&tty, &real);
    found?;
    Ok((!id.is_empty()).then_some(id))
}

/// The terminal you're in: Ghostty's focused one in its front window while Ghostty is the app
/// in front, else "". Closing it would pull it from under you.
const GHOSTTY_IN_FRONT: &str = r#"
on run argv
  tell application "Ghostty"
    if not frontmost then return ""
    try
      return id of focused terminal of selected tab of front window
    end try
  end tell
  return ""
end run
"#;

/// `<terminal id>...`: closes those terminals, skipping any that closed by themselves already.
const GHOSTTY_CLOSE: &str = r#"
on run argv
  tell application "Ghostty"
    repeat with t in argv
      try
        if exists terminal id (contents of t) then close terminal id (contents of t)
      end try
    end repeat
  end tell
  return "ok"
end run
"#;

/// Ends sessions' processes and closes the Ghostty terminals they ran in (idle.rs), all at once:
/// with dozens of tabs on a busy Mac, one tab at a time took minutes. Every process's TTY gets
/// its own mark, one look at Ghostty finds them all, the processes get a hangup (as they would
/// from the tab closing), and the terminals whose process exited close in one script: Ghostty
/// asks before closing a terminal with a program still running, and doesn't once only the
/// shell is left at its prompt. One result per `(pid, folder)`, in order.
pub fn close_session_terminals(profile: &str, sessions: &[(u32, &Path)]) -> Vec<Result<()>> {
    if profile != "ghostty" {
        return sessions.iter().map(|_| Err(anyhow!("Closing terminals works in Ghostty only."))).collect();
    }
    match close_ghostty_terminals(sessions) {
        Ok(out) => out,
        Err(e) => sessions.iter().map(|_| Err(anyhow!("{e}"))).collect(),
    }
}

fn close_ghostty_terminals(sessions: &[(u32, &Path)]) -> Result<Vec<Result<()>>> {
    // Marks take over TTYs' folder reports, as focusing's do, so the two never run at once.
    let _turn = FOCUS_TURNS.lock.lock().unwrap_or_else(|e| e.into_inner());
    let before = ghostty_terminals()?;
    let stamp = crate::util::now_ms();
    let mut out: Vec<Result<()>> = Vec::new();
    // (index, tty, mark) for each process that has a terminal.
    let mut marked = Vec::new();
    for (i, (pid, _)) in sessions.iter().enumerate() {
        let mark = format!("{MARK_PREFIX}{pid}-{stamp}");
        match tty_of(*pid) {
            Some(tty) if write_osc7(&tty, &mark).is_ok() => {
                marked.push((i, tty, mark));
                out.push(Ok(()));
            }
            Some(_) => out.push(Err(anyhow!("Couldn't reach its terminal."))),
            None => out.push(Err(anyhow!("It has no terminal tab."))),
        }
    }
    // Wait for Ghostty to show the marks, looking at every terminal at once.
    let mut ids: Vec<Option<String>> = vec![None; marked.len()];
    let started = std::time::Instant::now();
    loop {
        let now = ghostty_terminals().unwrap_or_default();
        for (j, (_, _, mark)) in marked.iter().enumerate() {
            if ids[j].is_none() {
                ids[j] = now.iter().find(|t| t.cwd.contains(mark.as_str())).map(|t| t.id.clone());
            }
        }
        if ids.iter().all(Option::is_some) || started.elapsed() > Duration::from_secs(8) {
            break;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    // Put each terminal's real folder back.
    for (j, (i, tty, _)) in marked.iter().enumerate() {
        let real = ids[j]
            .as_ref()
            .and_then(|id| before.iter().find(|t| &t.id == id))
            .map(|t| t.cwd.clone())
            .filter(|c| !c.is_empty() && !c.starts_with(MARK_PREFIX))
            .unwrap_or_else(|| sessions[*i].1.to_string_lossy().into_owned());
        let _ = write_osc7(tty, &real);
    }
    let front = osascript(GHOSTTY_IN_FRONT, &[]).map(|s| s.trim().to_string()).unwrap_or_default();
    // (index, terminal id) of the ones to end.
    let mut ending = Vec::new();
    for (j, (i, _, _)) in marked.iter().enumerate() {
        match &ids[j] {
            None => out[*i] = Err(anyhow!("Couldn't find its Ghostty tab.")),
            Some(id) if !front.is_empty() && *id == front => out[*i] = Err(anyhow!("It's the tab you're in.")),
            Some(id) => ending.push((*i, id.clone())),
        }
    }
    let alive = |pid: u32| unsafe { libc::kill(pid as libc::pid_t, 0) } == 0;
    for (i, _) in &ending {
        unsafe { libc::kill(sessions[*i].0 as libc::pid_t, libc::SIGHUP) };
    }
    let started = std::time::Instant::now();
    while ending.iter().any(|(i, _)| alive(sessions[*i].0)) && started.elapsed() < Duration::from_secs(10) {
        std::thread::sleep(Duration::from_millis(100));
    }
    let mut closing = Vec::new();
    for (i, id) in &ending {
        let pid = sessions[*i].0;
        if alive(pid) {
            out[*i] = Err(anyhow!("Its process (pid {pid}) didn't exit, so the tab stays."));
        } else {
            closing.push(id.as_str());
        }
    }
    if !closing.is_empty() {
        // A moment for the shells to draw their prompts, which tells Ghostty it's safe to close.
        std::thread::sleep(Duration::from_millis(500));
        if let Err(e) = osascript(GHOSTTY_CLOSE, &closing) {
            for (i, id) in &ending {
                if closing.contains(&id.as_str()) {
                    out[*i] = Err(anyhow!("Its process ended, but the tab didn't close: {e}"));
                }
            }
        }
    }
    Ok(out)
}

/// Which terminal a process runs in, by its parents: cheap enough to ask on every look.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostKind {
    Ghostty,
    Tmux,
}

pub fn terminal_kind(pid: u32) -> Option<HostKind> {
    if runs_under(pid, "ghostty") {
        Some(HostKind::Ghostty)
    } else if runs_under(pid, "tmux") && tmux_bin().is_some() {
        Some(HostKind::Tmux)
    } else {
        None
    }
}

/// A terminal to press keys in: a Ghostty terminal by id, or a tmux pane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminalHost {
    Ghostty(String),
    Tmux(String),
}

/// The terminal a session's process runs in, found (for Ghostty, marked) now.
pub fn terminal_host(pid: u32, cwd: &Path) -> Result<Option<TerminalHost>> {
    Ok(match terminal_kind(pid) {
        Some(HostKind::Ghostty) => ghostty_terminal_of(pid, cwd)?.map(TerminalHost::Ghostty),
        Some(HostKind::Tmux) => tmux_pane_of(pid).map(TerminalHost::Tmux),
        None => None,
    })
}

/// A key a harness's own prompt answers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Digit1,
    Escape,
}

/// Presses one key in a terminal, as if typed: no paste, no Return.
pub fn press_key(host: &TerminalHost, key: Key) -> Result<()> {
    match host {
        TerminalHost::Ghostty(id) => {
            // `send key` makes a key event without text, and Ghostty writes nothing for a
            // character key without text: only keys with codes of their own, like Escape, get
            // through. A digit goes through Ghostty's `text:` action, which writes it to the
            // program as typing it does; `input text` would paste it, and Claude Code's prompt
            // ignores pastes.
            match key {
                Key::Digit1 => osascript(GHOSTTY_ACTION, &[id, "text:1"])?,
                Key::Escape => osascript(GHOSTTY_KEY, &[id, "escape"])?,
            };
            Ok(())
        }
        TerminalHost::Tmux(pane) => {
            let name = match key {
                Key::Digit1 => "1",
                Key::Escape => "Escape",
            };
            let bin = tmux_bin().ok_or_else(|| anyhow!("tmux isn't installed where Codealong looks for it."))?;
            procs::run(&bin.to_string_lossy(), &["send-keys", "-t", pane, name], None, Duration::from_secs(5))?;
            Ok(())
        }
    }
}

/// `<terminal id> <key name>`: presses one key in that terminal.
const GHOSTTY_KEY: &str = r#"
on run argv
  tell application "Ghostty"
    if not (exists terminal id (item 1 of argv)) then error "That terminal is gone."
    send key (item 2 of argv) to terminal id (item 1 of argv)
  end tell
  return "ok"
end run
"#;

/// `<terminal id> <action>`: performs one of Ghostty's actions (as keybindings name them) in
/// that terminal; it answers false for one it didn't perform.
const GHOSTTY_ACTION: &str = r#"
on run argv
  tell application "Ghostty"
    if not (exists terminal id (item 1 of argv)) then error "That terminal is gone."
    if not (perform action (item 2 of argv) on terminal id (item 1 of argv)) then error "Ghostty didn't take the key."
  end tell
  return "ok"
end run
"#;

fn tmux_bin() -> Option<std::path::PathBuf> {
    ["/opt/homebrew/bin/tmux", "/usr/local/bin/tmux", "/usr/bin/tmux"].iter().map(std::path::PathBuf::from).find(|p| p.is_file())
}

/// The tmux pane whose TTY is the process's.
fn tmux_pane_of(pid: u32) -> Option<String> {
    let tty = tty_of(pid)?;
    let bin = tmux_bin()?;
    let out = procs::run(&bin.to_string_lossy(), &["list-panes", "-a", "-F", "#{pane_tty} #{pane_id}"], None, Duration::from_secs(5)).ok()?;
    tmux_pane_for(&out, &tty.to_string_lossy())
}

/// From `list-panes -F '#{pane_tty} #{pane_id}'`, the pane on `tty`.
fn tmux_pane_for(list: &str, tty: &str) -> Option<String> {
    list.lines().find_map(|l| l.split_once(' ').filter(|(t, _)| *t == tty).map(|(_, id)| id.trim().to_string()))
}

/// Pastes `text` into the pane (bracketed, so a prompt of several lines arrives whole), then
/// presses Return a moment later, as `GHOSTTY_TYPE` does.
fn tmux_type(pane: &str, text: &str) -> Result<()> {
    let bin = tmux_bin().ok_or_else(|| anyhow!("tmux isn't installed where Codealong looks for it."))?.to_string_lossy().into_owned();
    procs::run_stdin(&bin, &["load-buffer", "-b", "codealong", "-"], text, Duration::from_secs(5))?;
    procs::run(&bin, &["paste-buffer", "-p", "-d", "-b", "codealong", "-t", pane], None, Duration::from_secs(5))?;
    std::thread::sleep(Duration::from_millis(300));
    procs::run(&bin, &["send-keys", "-t", pane, "Enter"], None, Duration::from_secs(5))?;
    Ok(())
}

/// Whether `pid` runs inside the app whose executable is named `exe`, asked of the kernel
/// (procs.rs). Ghostty starts a login shell per tab: claude → zsh → login → ghostty.
pub fn runs_under(pid: u32, exe: &str) -> bool {
    runs_under_by(pid, exe, procs::parent_and_name)
}

/// `runs_under`, with `step` giving each process's parent and name.
fn runs_under_by(pid: u32, exe: &str, step: impl Fn(u32) -> Option<(u32, String)>) -> bool {
    let mut at = pid;
    for _ in 0..16 {
        let Some((ppid, name)) = step(at) else { return false };
        if name == exe {
            return true;
        }
        if ppid <= 1 {
            return false;
        }
        at = ppid;
    }
    false
}

/// The terminal device a process writes to, or None for a background process.
fn tty_of(pid: u32) -> Option<std::path::PathBuf> {
    procs::tty(pid)
}

/// Reports `dir` as the terminal's folder. Never blocks: a full or stuck terminal just misses it.
fn write_osc7(tty: &Path, dir: &str) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut f = std::fs::OpenOptions::new().write(true).custom_flags(libc::O_NOCTTY | libc::O_NONBLOCK).open(tty)?;
    f.write_all(osc7(dir).as_bytes())
}

/// The escape sequence a shell sends to report its folder.
pub fn osc7(dir: &str) -> String {
    let path: String = dir
        .bytes()
        .map(|b| if b.is_ascii_alphanumeric() || b"/-._~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") })
        .collect();
    format!("\x1b]7;file://localhost{path}\x07")
}

/// Claude Code titles a tab with a status glyph, a space and the session title.
fn tab_title(name: &str) -> &str {
    match name.split_once(' ') {
        Some((glyph, rest)) if glyph.chars().count() == 1 && !glyph.chars().all(char::is_alphanumeric) => rest,
        _ => name,
    }
}

/// The one tab titled like the session (the one in its folder, when several are); else the one
/// tab in its folder. None when that still leaves more than one: a guess puts another agent's
/// terminal in front, and what's typed there goes to the wrong session.
pub fn pick_by_title(terminals: &[GhosttyTerminal], title: &str, cwd: &Path) -> Option<String> {
    let dir = cwd.to_string_lossy();
    let only = |ts: Vec<&GhosttyTerminal>| match ts.as_slice() {
        [t] => Some(t.id.clone()),
        _ => None,
    };
    let titled: Vec<&GhosttyTerminal> = terminals.iter().filter(|t| !title.is_empty() && tab_title(&t.name) == title).collect();
    if titled.is_empty() {
        return only(terminals.iter().filter(|t| t.cwd == dir).collect());
    }
    let here: Vec<&GhosttyTerminal> = titled.iter().copied().filter(|t| t.cwd == dir).collect();
    only(titled).or_else(|| only(here))
}

pub fn open_url(url: &str) -> Result<()> {
    procs::run("/usr/bin/open", &[url], None, Duration::from_secs(10)).map(|_| ()).map_err(|e| anyhow!("Couldn't open {url}: {e}"))
}

pub fn reveal(path: &Path) -> Result<()> {
    procs::run("/usr/bin/open", &["-R", &path.to_string_lossy()], None, Duration::from_secs(10)).map(|_| ())
}

/// Opens a session in the Claude app. A session the app runs is selected by the app's own ID
/// (`code/continue` takes only `local_…` IDs); any other is imported first (`resume`).
pub fn claude_app_url(cli_session_id: &str) -> String {
    claude_app_url_in(&home().join("Library/Application Support/Claude/claude-code-sessions"), cli_session_id)
}

fn claude_app_url_in(records: &Path, cli_session_id: &str) -> String {
    match claude_app_session(records, cli_session_id) {
        Some(local) => format!("claude://code/continue?session={}", urlencoding::encode(&local)),
        None => format!("claude://resume?session={}", urlencoding::encode(cli_session_id)),
    }
}

/// The Claude app's ID for the session running `cli_session_id`, from its records (newest first,
/// since that's usually the one; only files that mention the session are parsed).
fn claude_app_session(records: &Path, cli_session_id: &str) -> Option<String> {
    crate::claudeapp::record_files(records).into_iter().find_map(|(path, _)| {
        let text = std::fs::read_to_string(&path).ok()?;
        if !text.contains(cli_session_id) {
            return None;
        }
        crate::claudeapp::parse_record(&text, &path).filter(|r| r.cli_session_id == cli_session_id).map(|r| r.local_id)
    })
}

/// Opens a `codex://` link in the Codex app that's running. The Codex app ships as
/// ChatGPT.app now, and an older Codex.app may still be installed with the same scheme.
pub fn open_codex(url: &str) -> Result<()> {
    for app in ["/Applications/ChatGPT.app", "/Applications/Codex.app"] {
        let exe = format!("{app}/Contents/MacOS/");
        let running = procs::run("/usr/bin/pgrep", &["-f", &exe], None, Duration::from_secs(3)).is_ok();
        if running {
            return procs::run("/usr/bin/open", &["-a", app, url], None, Duration::from_secs(10))
                .map(|_| ())
                .map_err(|e| anyhow!("Couldn't open Codex: {e}"));
        }
    }
    open_url(url)
}

/// Brings Antigravity forward. It has no link to a conversation (its only deep link teleports
/// into AI Studio), so the app is as close as Codealong can get.
pub fn open_antigravity() -> Result<()> {
    let app = "/Applications/Antigravity.app";
    if !Path::new(app).is_dir() {
        bail!("Antigravity isn't installed in /Applications.");
    }
    procs::run("/usr/bin/open", &["-a", app], None, Duration::from_secs(10)).map(|_| ()).map_err(|e| anyhow!("Couldn't open Antigravity: {e}"))
}

pub fn codex_thread_url(thread_id: &str) -> String {
    format!("codex://threads/{}", urlencoding::encode(thread_id))
}

pub fn codex_review_url(pr_url: &str) -> String {
    format!("codex://review?pr={}", urlencoding::encode(pr_url))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_deep_links() {
        let empty = std::env::temp_dir().join("codealong-no-records");
        assert_eq!(claude_app_url_in(&empty, "87412567-2cd2"), "claude://resume?session=87412567-2cd2");
        assert_eq!(codex_thread_url("01a0c4a6"), "codex://threads/01a0c4a6");
        assert_eq!(
            codex_review_url("https://github.com/northwindai/atlas/pull/1037"),
            "codex://review?pr=https%3A%2F%2Fgithub.com%2Fnorthwindai%2Fatlas%2Fpull%2F1037"
        );
    }

    #[test]
    fn ghostty_scripts_compile() {
        // osacompile checks the scripts against Ghostty's dictionary without running them.
        if !Path::new("/Applications/Ghostty.app").exists() {
            return;
        }
        for (i, script) in [GHOSTTY_NEW, GHOSTTY_LIST, GHOSTTY_FOCUS, GHOSTTY_FIND, GHOSTTY_TYPE, GHOSTTY_IN_FRONT, GHOSTTY_CLOSE].iter().enumerate() {
            let out = std::env::temp_dir().join(format!("codealong-test-{i}.scpt"));
            let r = procs::run_stdin("/usr/bin/osacompile", &["-o", &out.to_string_lossy()], script, Duration::from_secs(20));
            assert!(r.is_ok(), "script {i} failed to compile: {r:?}");
        }
    }

    /// Every terminal comes back with its folder: `tab` inside a Ghostty tell once meant its tab
    /// class, the lines had no separators, and every terminal was dropped. Needs Ghostty running
    /// with a terminal open: `cargo test ghostty_lists_terminals_with_folders -- --ignored`.
    #[test]
    #[ignore]
    fn ghostty_lists_terminals_with_folders() {
        let count: usize = osascript(r#"tell application "Ghostty" to return count of every terminal"#, &[]).unwrap().trim().parse().unwrap();
        assert!(count > 0, "Ghostty has no terminals open");
        assert_eq!(ghostty_terminals().unwrap().len(), count);
    }

    /// Opens a real Ghostty window whose shell writes the prompt it was handed back to a file,
    /// then exits. Opt in: `cargo test --release ghostty_hands_over -- --ignored`.
    #[test]
    #[ignore]
    fn ghostty_hands_over_the_environment() {
        let out = std::env::temp_dir().join(format!("codealong-env-{}", std::process::id()));
        let _ = std::fs::remove_file(&out);
        let prompt = "Fix the \"flaky\" test\n\tthen run `npm test` and $(say nope) 'quoted' -v";
        let input = format!("printenv CODEALONG_PROMPT > '{}'; exit\n", out.display());
        terminal_run("ghostty", Placement { new_terminal: NewTerminal::Window, near: &[] }, &std::env::temp_dir(), &input, &[("CODEALONG_PROMPT".to_string(), prompt.to_string())]).unwrap();
        for _ in 0..100 {
            if std::fs::metadata(&out).map(|m| m.len() > 0).unwrap_or(false) {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        std::thread::sleep(Duration::from_millis(200));
        let got = std::fs::read_to_string(&out).expect("the shell wrote nothing");
        let _ = std::fs::remove_file(&out);
        assert_eq!(got.strip_suffix('\n').unwrap_or(&got), prompt);
    }

    /// Opens a Ghostty window running a long `sleep` and a plain window in front of it, closes the
    /// first as Close idle terminals would (hangup, then close), and checks it's gone. It opens
    /// windows, so it's opt in: `cargo test ghostty_closes_a_session_terminal -- --ignored`.
    #[test]
    #[ignore]
    fn ghostty_closes_a_session_terminal() {
        const OPEN: &str = r#"
on run argv
  tell application "Ghostty"
    set seenIds to id of every terminal
    new window with configuration {initial working directory:(item 1 of argv), initial input:(item 2 of argv)}
    repeat 50 times
      set nowIds to id of every terminal
      repeat with t in nowIds
        if seenIds does not contain (contents of t) then return contents of t
      end repeat
      delay 0.1
    end repeat
  end tell
  return ""
end run
"#;
        const EXISTS: &str = r#"
on run argv
  tell application "Ghostty" to return exists terminal id (item 1 of argv)
end run
"#;
        const CLOSE: &str = r#"
on run argv
  tell application "Ghostty"
    try
      close terminal id (item 1 of argv)
    end try
  end tell
end run
"#;
        let dir = std::env::temp_dir();
        let secs = 7000 + std::process::id() % 1000;
        let sleeper = osascript(OPEN, &[&dir.to_string_lossy(), &format!("sleep {secs}\n")]).unwrap().trim().to_string();
        let front = osascript(OPEN, &[&dir.to_string_lossy(), ""]).unwrap().trim().to_string();
        let mut pid = None;
        for _ in 0..50 {
            let out = procs::run("/usr/bin/pgrep", &["-f", &format!("^sleep {secs}$")], None, Duration::from_secs(3)).unwrap_or_default();
            pid = out.trim().lines().next().and_then(|p| p.parse::<u32>().ok());
            if pid.is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let result = pid.ok_or_else(|| anyhow!("the sleep never started")).and_then(|pid| close_session_terminals("ghostty", &[(pid, dir.as_path())]).remove(0));
        let still = osascript(EXISTS, &[&sleeper]).unwrap_or_default();
        let _ = osascript(CLOSE, &[&front]);
        let _ = osascript(CLOSE, &[&sleeper]);
        result.unwrap();
        assert_eq!(still.trim(), "false", "the terminal is still open");
    }

    /// Opens a Ghostty window whose program records the bytes that reach it, in raw mode with
    /// bracketed paste on as Claude Code runs, presses 1 and Escape there as answering a
    /// permission prompt does, and checks both arrived as typed keys: `send key` sent nothing
    /// for the digit. It opens a window, so it's opt in: `cargo test ghostty_presses_keys --
    /// --ignored`.
    /// Finds the Ghostty tab of a process that runs in one, as a send does before it types, and
    /// types nothing: `CODEALONG_TEST_PID=<pid> cargo test ghostty_finds_a_tab -- --ignored
    /// --nocapture`. The tab's folder changes for a moment and is put back, as a click's does.
    #[test]
    #[ignore]
    fn ghostty_finds_a_tab() {
        let pid: u32 = std::env::var("CODEALONG_TEST_PID").expect("CODEALONG_TEST_PID").parse().unwrap();
        let t = std::time::Instant::now();
        let kind = terminal_kind(pid);
        println!("terminal: {kind:?}, tty {:?}, in {:?}", tty_of(pid), t.elapsed());
        assert_eq!(kind, Some(HostKind::Ghostty));
        let t = std::time::Instant::now();
        let id = ghostty_terminal_of(pid, Path::new("/")).unwrap();
        println!("tab: {id:?}, in {:?}", t.elapsed());
        assert!(id.is_some());
    }

    #[test]
    #[ignore]
    fn ghostty_presses_keys() {
        const OPEN: &str = r#"
on run argv
  tell application "Ghostty"
    set seenIds to id of every terminal
    new window with configuration {initial working directory:(item 1 of argv), initial input:(item 2 of argv)}
    repeat 50 times
      set nowIds to id of every terminal
      repeat with t in nowIds
        if seenIds does not contain (contents of t) then return contents of t
      end repeat
      delay 0.1
    end repeat
  end tell
  return ""
end run
"#;
        const CLOSE: &str = r#"
on run argv
  tell application "Ghostty"
    try
      close terminal id (item 1 of argv)
    end try
  end tell
end run
"#;
        let dir = std::env::temp_dir();
        let out = dir.join(format!("codealong-keys-{}", std::process::id()));
        let ready = dir.join(format!("codealong-keys-{}.ready", std::process::id()));
        let _ = std::fs::remove_file(&out);
        let _ = std::fs::remove_file(&ready);
        // Once the terminal is raw and takes bracketed pastes, the next two bytes, whatever they are.
        let input = format!("stty raw -echo; printf '\\033[?2004h'; touch '{}'; dd bs=1 count=2 of='{}' 2>/dev/null; stty sane; exit\n", ready.display(), out.display());
        let id = osascript(OPEN, &[&dir.to_string_lossy(), &input]).unwrap().trim().to_string();
        for _ in 0..100 {
            if ready.exists() {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let host = TerminalHost::Ghostty(id.clone());
        let pressed = press_key(&host, Key::Digit1).and_then(|_| press_key(&host, Key::Escape));
        for _ in 0..50 {
            if std::fs::metadata(&out).map(|m| m.len() >= 2).unwrap_or(false) {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let got = std::fs::read(&out).unwrap_or_default();
        let _ = osascript(CLOSE, &[&id]);
        let _ = std::fs::remove_file(&out);
        let _ = std::fs::remove_file(&ready);
        pressed.unwrap();
        assert_eq!(got, b"1\x1b", "the program got {:?}", String::from_utf8_lossy(&got));
    }

    /// Opens three Ghostty windows, focuses the terminal in the one furthest back, checks that
    /// its window is the one in front, then closes them. Held by position, the focus brought
    /// forward the window that had been in front of it instead. It moves windows about, so it's
    /// opt in: `cargo test ghostty_focuses_a_window_behind_others -- --ignored`.
    #[test]
    #[ignore]
    fn ghostty_focuses_a_window_behind_others() {
        const OPEN: &str = r#"
on run argv
  tell application "Ghostty"
    set seenIds to id of every terminal
    new window with configuration {initial working directory:(item 1 of argv)}
    repeat 50 times
      set nowIds to id of every terminal
      repeat with t in nowIds
        if seenIds does not contain (contents of t) then return contents of t
      end repeat
      delay 0.1
    end repeat
  end tell
  return ""
end run
"#;
        const FRONT: &str = r#"
on run argv
  tell application "Ghostty" to return id of every terminal of front window
end run
"#;
        const CLOSE: &str = r#"
on run argv
  tell application "Ghostty"
    repeat with t in argv
      try
        close terminal id (contents of t)
      end try
    end repeat
  end tell
end run
"#;
        struct Opened(Vec<String>);
        impl Drop for Opened {
            fn drop(&mut self) {
                let ids: Vec<&str> = self.0.iter().map(String::as_str).collect();
                let _ = osascript(CLOSE, &ids);
            }
        }
        let dir = std::env::temp_dir();
        let mut opened = Opened(Vec::new());
        for _ in 0..3 {
            let id = osascript(OPEN, &[&dir.to_string_lossy()]).unwrap().trim().to_string();
            assert!(!id.is_empty(), "the new window has no terminal");
            opened.0.push(id);
        }
        let back = opened.0[0].clone();
        assert_eq!(osascript(GHOSTTY_FOCUS, &["id", &back]).unwrap().trim(), back);
        std::thread::sleep(Duration::from_millis(500));
        let front = osascript(FRONT, &[]).unwrap();
        assert!(front.contains(&back), "the front window holds {}, not {back}", front.trim());
    }

    #[test]
    fn finds_the_claude_app_session_for_a_cli_session() {
        let root = std::env::temp_dir().join(format!("codealong-records-{}", std::process::id()));
        let org = root.join("acct").join("org");
        std::fs::create_dir_all(&org).unwrap();
        std::fs::write(org.join("local_aaa.json"), r#"{"sessionId":"local_aaa","cliSessionId":"11111111-0000-0000-0000-000000000000"}"#).unwrap();
        std::fs::write(org.join("local_bbb.json"), r#"{"sessionId":"local_bbb","cliSessionId":"87412567-2cd2-4c10-8635-d144e1c53c78"}"#).unwrap();
        assert_eq!(claude_app_url_in(&root, "87412567-2cd2-4c10-8635-d144e1c53c78"), "claude://code/continue?session=local_bbb");
        assert_eq!(claude_app_url_in(&root, "22222222-0000-0000-0000-000000000000"), "claude://resume?session=22222222-0000-0000-0000-000000000000");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn reports_a_folder_the_way_a_shell_does() {
        assert_eq!(osc7("/tmp/codealong-1-2"), "\x1b]7;file://localhost/tmp/codealong-1-2\x07");
        assert_eq!(osc7("/Users/me/My Repo"), "\x1b]7;file://localhost/Users/me/My%20Repo\x07");
    }

    #[test]
    fn knows_a_session_ghostty_started() {
        // (pid, parent, name) as the kernel has them.
        let table = [(849, 1, "ghostty"), (958, 849, "login"), (997, 958, "zsh"), (68277, 997, "claude"), (4100, 1, "tmux"), (4101, 4100, "zsh"), (4102, 4101, "claude"), (5000, 1, "Google Chrome")];
        let step = |pid: u32| table.iter().find(|r| r.0 == pid).map(|r| (r.1, r.2.to_string()));
        assert!(runs_under_by(68277, "ghostty", step));
        assert!(!runs_under_by(4102, "ghostty", step));
        assert!(runs_under_by(4102, "tmux", step));
        assert!(!runs_under_by(5000, "ghostty", step));
        assert!(!runs_under_by(999_999, "ghostty", step));
    }

    #[test]
    fn reads_a_process_line_and_terminal_from_the_kernel() {
        // This test's own process and a child of it: named, with parents, found under it.
        let me = std::process::id();
        let (_, name) = procs::parent_and_name(me).expect("the kernel lists this process");
        assert!(!name.is_empty());
        assert!(runs_under(me, &name));
        assert!(!runs_under(me, "no-such-program"));
        let mut sleeper = std::process::Command::new("/bin/sleep").arg("30").stdin(std::process::Stdio::null()).spawn().unwrap();
        let child = sleeper.id();
        assert_eq!(procs::parent_and_name(child), Some((me, "sleep".to_string())));
        assert!(runs_under(child, &name));
        // Every process `ps` lists has the parent, name and terminal it says, other users'
        // included, and is under Ghostty when `ps` puts it there: the root `login` in a Ghostty
        // tab's line is on the way, when one is open.
        let table = procs::run("/bin/ps", &["-A", "-o", "pid=,ppid=,tty=,ucomm="], None, Duration::from_secs(20)).unwrap();
        let rows: Vec<(u32, u32, &str, &str)> = table
            .lines()
            .filter_map(|line| {
                let mut f = line.split_whitespace();
                let (pid, ppid, tty) = (f.next()?.parse().ok()?, f.next()?.parse().ok()?, f.next()?);
                // A name can have spaces: the rest of the line.
                let name = line[line.find(tty)? + tty.len()..].trim();
                Some((pid, ppid, tty, name))
            })
            .collect();
        let by_ps = |pid: u32| rows.iter().find(|r| r.0 == pid).map(|r| (r.1, r.3.to_string()));
        let mut checked = 0;
        for &(pid, ppid, tty, name) in &rows {
            // Gone since `ps` looked.
            let Some((parent, kernel_name)) = procs::parent_and_name(pid) else { continue };
            // A name cut to 16 characters can end in a space, which `ps` doesn't print.
            assert_eq!((parent, kernel_name.trim_end()), (ppid, name), "pid {pid}");
            let want = tty.starts_with("tty").then(|| Path::new("/dev").join(tty));
            assert_eq!(procs::tty(pid), want, "pid {pid}");
            assert_eq!(runs_under(pid, "ghostty"), runs_under_by(pid, "ghostty", by_ps), "pid {pid}");
            checked += 1;
        }
        assert!(checked > 10, "only {checked} processes checked");
        let _ = sleeper.kill();
        let _ = sleeper.wait();
        assert_eq!(procs::parent_and_name(child), None);
    }

    #[test]
    fn puts_a_new_tab_beside_the_nearest_folder_with_terminals() {
        let t = |id: &str, cwd: &str| GhosttyTerminal { id: id.into(), cwd: cwd.into(), name: String::new() };
        let tabs = vec![t("a", "/r/app"), t("b", "/r/app-pins/src"), t("c", "/r/kestrel"), t("d", ""), t("e", "/r/app-pins")];
        let groups = |g: &[&[&str]]| g.iter().map(|g| g.iter().map(|f| f.to_string()).collect()).collect::<Vec<Vec<String>>>();
        assert_eq!(terminals_beside(&tabs, &groups(&[&["/r/app-pins"], &["/r/app"]])), vec!["b", "e"]);
        // Not a folder that merely starts the same way.
        assert_eq!(terminals_beside(&tabs, &groups(&[&["/r/app-p"], &["/r/app"]])), vec!["a"]);
        assert_eq!(terminals_beside(&tabs, &groups(&[&["/r/none"], &["/r/kestrel", "/r/x"]])), vec!["c"]);
        assert!(terminals_beside(&tabs, &groups(&[&["/r/none"]])).is_empty());
    }

    #[test]
    fn matches_tabs_by_title_then_folder() {
        let t = |id: &str, cwd: &str, name: &str| GhosttyTerminal { id: id.into(), cwd: cwd.into(), name: name.into() };
        let tabs = vec![
            t("a", "/r/app4", "✳ RLS work and other tables"),
            t("b", "/r/app2", "✳ Cloud agent execution regressions"),
            t("c", "/r/app2", "✳ Cloud agent execution regressions ⑂"),
            t("d", "/r/app4", "…/projects/app4"),
            t("e", "/r/app3", "⠐ Fix the login redirect"),
            t("f", "/r/app1", "⠂ Fix the login redirect"),
            t("g", "/r/app5", "✳ Bump dependencies"),
            t("h", "/r/app6", "✳ Bump dependencies"),
        ];
        assert_eq!(pick_by_title(&tabs, "Cloud agent execution regressions ⑂", Path::new("/r/app2")).as_deref(), Some("c"));
        assert_eq!(pick_by_title(&tabs, "Cloud agent execution regressions", Path::new("/r/app2")).as_deref(), Some("b"));
        assert_eq!(pick_by_title(&tabs, "RLS work and other tables", Path::new("/r/none")).as_deref(), Some("a"));
        // Two tabs share the title: the one in the session's folder, and no guess without one.
        assert_eq!(pick_by_title(&tabs, "Fix the login redirect", Path::new("/r/app3")).as_deref(), Some("e"));
        assert_eq!(pick_by_title(&tabs, "Bump dependencies", Path::new("/r/app2")), None);
        // No title matches: the one tab in the folder, and no guess between two.
        assert_eq!(pick_by_title(&tabs, "Something else", Path::new("/r/app3")).as_deref(), Some("e"));
        assert_eq!(pick_by_title(&tabs, "Something else", Path::new("/r/app4")), None);
        assert_eq!(pick_by_title(&tabs, "Something else", Path::new("/r/none")), None);
    }

    #[test]
    fn a_newer_focus_goes_ahead_of_one_still_waiting() {
        let turns = Turns::new();
        let first = turns.ticket();
        let held = turns.wait(first).expect("first in line");
        let second = turns.ticket();
        let third = turns.ticket();
        drop(held);
        assert!(turns.wait(second).is_none(), "a click made while another waited is dropped");
        assert!(turns.wait(third).is_some(), "the newest click goes ahead");
    }

    #[test]
    fn a_tmux_pane_is_found_by_its_tty() {
        let list = "/dev/ttys003 %0\n/dev/ttys012 %4\n";
        assert_eq!(tmux_pane_for(list, "/dev/ttys012").as_deref(), Some("%4"));
        assert_eq!(tmux_pane_for(list, "/dev/ttys01"), None);
    }
}
