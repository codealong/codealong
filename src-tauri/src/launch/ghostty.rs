//! Ghostty through its AppleScript dictionary. Its terminals have an id, a folder and a title,
//! but no TTY or PID, so a session's terminal is found by a mark: a unique folder reported (OSC 7)
//! on the session's TTY, which Ghostty then shows as that terminal's. See
//! .claude/rules/terminals.md.

use std::path::Path;
use std::time::Duration;

use anyhow::{anyhow, Result};

use super::{app_installed, hang_up, not_allowed_why, pick_by_title, placement, tty_of, write_osc7, HostKind, Key, NotAllowed, Placement, Submit, Tab, Terminal, TerminalHost, Typed, FOCUS_TURNS, MARK_PREFIX};
use crate::procs;

const APP: &str = "Ghostty";

pub(super) fn osascript(script: &str, args: &[&str]) -> Result<String> {
    super::osascript_in(APP, script, args)
}

pub struct Ghostty;

impl Terminal for Ghostty {
    fn kind(&self) -> HostKind {
        HostKind::Ghostty
    }

    fn name(&self) -> &'static str {
        APP
    }

    fn profile(&self) -> Option<&'static str> {
        Some("ghostty")
    }

    fn bundle(&self) -> Option<&'static str> {
        Some("com.mitchellh.ghostty")
    }

    fn installed(&self) -> bool {
        app_installed("Ghostty.app")
    }

    /// Ghostty starts a login shell per tab (claude → zsh → login → ghostty).
    fn runs_as(&self, process: &str) -> bool {
        process == "ghostty"
    }

    /// Its window configuration takes the folder, the input and the environment as they are, and
    /// the tab takes its name as an action of Ghostty's own.
    fn run(&self, place: Placement, dir: &Path, input: &str, env: &[(String, String)]) -> Result<()> {
        let dir = dir.to_string_lossy();
        let (at, beside) = placement(place, terminals);
        let entries: Vec<String> = env.iter().map(|(k, v)| format!("{k}={v}")).collect();
        let mut args: Vec<&str> = vec![at, &beside, &dir, input, place.forward(), place.label.unwrap_or("")];
        args.extend(entries.iter().map(String::as_str));
        osascript(GHOSTTY_NEW, &args).map(|_| ())
    }

    fn open(&self, place: Placement, dir: &Path, input: Option<&str>) -> Result<()> {
        match self.run(place, dir, input.unwrap_or(""), &[]) {
            Ok(()) => Ok(()),
            Err(e) if input.is_none() => {
                // Without Automation permission a plain window still works.
                let arg = format!("--working-directory={}", dir.to_string_lossy());
                procs::open(&["-na", "Ghostty.app", "--args", &arg], Duration::from_secs(10)).map_err(|_| e)
            }
            Err(e) => Err(e),
        }
    }

    fn tabs(&self) -> Result<Vec<Tab>> {
        terminals()
    }

    fn focus(&self, at: &str) -> Result<bool> {
        Ok(!osascript(GHOSTTY_FOCUS, &["id", at])?.trim().is_empty())
    }

    fn focus_session(&self, _profile: &str, pid: u32, tty: &Path, title: &str, cwd: &Path) -> Result<Option<TerminalHost>> {
        Ok(focus_session(pid, tty, title, cwd)?.map(|at| TerminalHost { terminal: &Ghostty, at }))
    }

    /// Clearing the box first is Ghostty's own action for Ctrl+C, in the script that types.
    fn type_session(&self, pid: u32, cwd: &Path, text: &str, clear: bool, submit: Submit) -> Result<Typed> {
        let id = match terminal_of(pid, cwd) {
            Ok(Some(id)) => id,
            Ok(None) => return Ok(Typed::Nowhere("Codealong couldn't find its Ghostty tab".into())),
            // Nothing was typed yet, so another way can take it; say why this one didn't.
            Err(e) if e.is::<NotAllowed>() => return Ok(Typed::Nowhere(not_allowed_why(APP))),
            Err(_) => return Ok(Typed::Nowhere("Ghostty didn't answer in time".into())),
        };
        let first = if clear { GHOSTTY_CTRL_C.to_string() } else { String::new() };
        let then = match submit {
            Submit::Return => String::new(),
            Submit::Keys(keys) => text_action(keys),
        };
        if osascript(GHOSTTY_TYPE, &[id.as_str(), text, &first, &then])?.trim().is_empty() {
            // Its tab closed between finding it and typing: nothing went in.
            return Ok(Typed::Nowhere("its Ghostty tab closed".into()));
        }
        Ok(Typed::Into(APP))
    }

    fn presses_keys(&self) -> bool {
        true
    }

    fn key_target(&self, pid: u32, cwd: &Path) -> Result<Option<String>> {
        terminal_of(pid, cwd)
    }

    fn press(&self, at: &str, key: Key) -> Result<()> {
        // `send key` makes a key event without text, and Ghostty writes nothing for a
        // character key without text: only keys with codes of their own, like Escape, get
        // through. A digit goes through Ghostty's `text:` action, which writes it to the
        // program as typing it does; `input text` would paste it, and Claude Code's prompt
        // ignores pastes.
        match key {
            Key::Digit1 => osascript(GHOSTTY_ACTION, &[at, "text:1"])?,
            Key::Escape => osascript(GHOSTTY_KEY, &[at, "escape"])?,
            Key::CtrlC => osascript(GHOSTTY_ACTION, &[at, GHOSTTY_CTRL_C])?,
            // End as the terminal sequence, which needs no key name of Ghostty's.
            Key::End => osascript(GHOSTTY_ACTION, &[at, &text_action(key.typed())])?,
            Key::Return => osascript(GHOSTTY_KEY, &[at, "enter"])?,
        };
        Ok(())
    }

    fn closes_tabs(&self) -> bool {
        true
    }

    fn close_sessions(&self, sessions: &[(u32, &Path)]) -> Result<Vec<Result<()>>> {
        close_sessions(sessions)
    }
}

/// Arguments: where it goes (`window`, `front` for a tab in the front window, or `beside` for a
/// tab in the frontmost window holding any of the terminals listed next), those terminal ids one
/// per line (or "": a front tab may have them too, to sit beside where they're in the front
/// window), the folder, the input to type (or ""), whether Ghostty comes forward
/// (`front`) or not (`behind`), the tab's name (or ""), then any `KEY=VALUE` environment
/// entries. A tab with no window to go in becomes a window. Behind, nothing here asks Ghostty
/// forward, though its own code for a new window or tab still asks to be the active app
/// (`NSApp.activate`, 1.3.1's TerminalController `newWindow` and `newTab`).
///
/// Ghostty puts a new tab after the selected one, or last (`window-new-tab-position`), so a tab
/// that goes beside others is moved to just after the last tab holding one of them
/// (`move_tab`), and a worktree's tabs sit together. Its name is `set_tab_title`, which outlasts
/// the titles the program in it sets and takes its text as it is (1.3.1, checked 2026-10-05).
/// Neither can undo the open, so a failure of either is let go.
pub(super) const GHOSTTY_NEW: &str = r#"
on run argv
  set placeAt to item 1 of argv
  set nearIds to paragraphs of (item 2 of argv)
  set thePath to item 3 of argv
  set theInput to item 4 of argv
  set raise to (item 5 of argv) is "front"
  set theLabel to item 6 of argv
  set theEnv to {}
  if (count of argv) > 6 then set theEnv to items 7 thru -1 of argv
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
      set made to new window with configuration cfg
    else
      set made to new tab in window id dest with configuration cfg
    end if
    set madeIds to {}
    try
      set madeIds to id of every terminal of made
    end try
    try
      if dest is not missing value then
        set madeTab to id of made
        set lastAt to 0
        set tabIds to id of every tab of window id dest
        repeat with tid in tabIds
          set inTab to id of every terminal of tab id (contents of tid) of window id dest
          repeat with t in nearIds
            if inTab contains (contents of t) then
              set pos to index of tab id (contents of tid) of window id dest
              if pos > lastAt then set lastAt to pos
              exit repeat
            end if
          end repeat
        end repeat
        set madeAt to index of tab id madeTab of window id dest
        if lastAt > 0 then
          if madeAt > lastAt then
            set moveBy to lastAt + 1 - madeAt
          else
            set moveBy to lastAt - madeAt
          end if
          if moveBy is not 0 then perform action ("move_tab:" & moveBy) on terminal id (item 1 of madeIds)
        end if
      end if
    end try
    try
      if theLabel is not "" then perform action ("set_tab_title:" & theLabel) on terminal id (item 1 of madeIds)
    end try
    if raise and dest is not missing value then
      try
        activate window window id dest
      end try
    end if
    if raise then activate
  end tell
  return "ok"
end run
"#;

/// Three questions give three lists, which only line up if no terminal opened or closed in
/// between, so it asks for the ids again to check, and gives up with nothing rather than pair
/// a terminal with another's folder. The separator is set outside the tell: inside it, `tab`
/// is Ghostty's tab class, and the lines came out joined by the word "tab".
pub(super) const GHOSTTY_LIST: &str = r#"
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
pub(super) const GHOSTTY_FOCUS: &str = r#"
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
pub(super) const GHOSTTY_FIND: &str = r#"
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

/// `<terminal id> <text> <first> <then>`: pastes the text into that terminal, then presses
/// Return, or performs `then` in its place (`text_action`, keys that send it otherwise). Ghostty
/// pastes it bracketed when the program asked for that (Claude Code and Codex do), so a prompt of
/// several lines arrives whole. Return comes a moment later, so the program has taken the paste
/// as a paste before it sees the key. `first`, unless it's empty, is an action performed before
/// the paste (`GHOSTTY_CTRL_C`).
pub(super) const GHOSTTY_TYPE: &str = r#"
on run argv
  set want to item 1 of argv
  set theText to item 2 of argv
  set firstAction to item 3 of argv
  set thenAction to item 4 of argv
  tell application "Ghostty"
    if not (exists terminal id want) then return ""
    if firstAction is not "" then
      perform action firstAction on terminal id want
      delay 0.2
    end if
    input text theText to terminal id want
    delay 0.3
    if thenAction is "" then
      send key "enter" to terminal id want
    else
      perform action thenAction on terminal id want
    end if
  end tell
  return want
end run
"#;

/// Ghostty's action that types Ctrl+C: `text:` takes a Zig string literal.
const GHOSTTY_CTRL_C: &str = "text:\\x03";

/// Ghostty's action that types `chars` as typing them does: `text:` and the characters as a Zig
/// string literal, every one that isn't printable ASCII (or is a quote or backslash) escaped.
fn text_action(chars: &str) -> String {
    let mut out = String::from("text:");
    for c in chars.chars() {
        match c {
            ' '..='~' if c != '\\' && c != '"' => out.push(c),
            c if (c as u32) < 0x80 => out.push_str(&format!("\\x{:02x}", c as u32)),
            c => out.push_str(&format!("\\u{{{:x}}}", c as u32)),
        }
    }
    out
}

/// How long to wait for Ghostty to show a session's marker. It's there within a fraction of a
/// second normally; on a Mac busy enough to fall seconds behind, a short wait reported tabs as
/// missing that Ghostty then showed.
const GHOSTTY_MARK_WAIT_SECS: &str = "8";

/// The terminal you're in: Ghostty's focused one in its front window while Ghostty is the app
/// in front, else "". Closing it would pull it from under you.
pub(super) const GHOSTTY_IN_FRONT: &str = r#"
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
pub(super) const GHOSTTY_CLOSE: &str = r#"
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

/// `<terminal id> <key name>`: presses one key in that terminal.
pub(super) const GHOSTTY_KEY: &str = r#"
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
pub(super) const GHOSTTY_ACTION: &str = r#"
on run argv
  tell application "Ghostty"
    if not (exists terminal id (item 1 of argv)) then error "That terminal is gone."
    if not (perform action (item 2 of argv) on terminal id (item 1 of argv)) then error "Ghostty didn't take the key."
  end tell
  return "ok"
end run
"#;

/// Its terminals, each with its id.
pub(super) fn terminals() -> Result<Vec<Tab>> {
    Ok(osascript(GHOSTTY_LIST, &[])?
        .lines()
        .filter_map(|line| {
            let mut parts = line.splitn(3, '\t');
            Some(Tab { id: parts.next()?.to_string(), cwd: parts.next()?.to_string(), name: parts.next().unwrap_or("").to_string() })
        })
        .filter(|t| !t.id.is_empty())
        .collect())
}

/// The id of the Ghostty terminal it brought forward, None when the session's tab isn't found.
fn focus_session(pid: u32, tty: &Path, title: &str, cwd: &Path) -> Result<Option<String>> {
    let terminals = terminals()?;
    let mark = format!("{MARK_PREFIX}{pid}-{}", crate::util::now_ms());
    let mut failed = None;
    if write_osc7(tty, &mark).is_ok() {
        let found = osascript(GHOSTTY_FOCUS, &["tag", &mark, GHOSTTY_MARK_WAIT_SECS]);
        let id = found.as_deref().map(str::trim).unwrap_or_default().to_string();
        let real = terminals
            .iter()
            .find(|t| t.id == id)
            .map(|t| t.cwd.clone())
            // A mark left over (the app quit halfway through) isn't the folder to put back.
            .filter(|c| !c.is_empty() && !c.starts_with(MARK_PREFIX))
            .unwrap_or_else(|| cwd.to_string_lossy().into_owned());
        let _ = write_osc7(tty, &real);
        match found {
            Ok(_) if !id.is_empty() => return Ok(Some(id)),
            Ok(_) => {}
            // Ghostty was too slow or refused: say that, unless the fallback finds the tab.
            Err(e) => failed = Some(e),
        }
    }
    match pick_by_title(&terminals, title, cwd) {
        Some(id) => Ok((!osascript(GHOSTTY_FOCUS, &["id", &id])?.trim().is_empty()).then_some(id)),
        None => failed.map_or(Ok(None), Err),
    }
}

/// The Ghostty terminal a process runs in, found by writing a unique folder report to its TTY
/// and asking Ghostty which terminal shows it; then the real folder is reported again.
pub(super) fn terminal_of(pid: u32, cwd: &Path) -> Result<Option<String>> {
    let Some(tty) = tty_of(pid) else { return Ok(None) };
    // A mark takes over the TTY's folder report, as focusing's does, so the two never run at
    // once. A send takes no ticket: it mustn't make a click waiting before it lose its turn.
    let _turn = FOCUS_TURNS.lock.lock().unwrap_or_else(|e| e.into_inner());
    let terminals = terminals()?;
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

/// Ends sessions' processes and closes their terminals: each TTY marked, the marks looked for in
/// one list of every terminal, then each process hung up and the terminals whose process exited
/// closed in one script.
fn close_sessions(sessions: &[(u32, &Path)]) -> Result<Vec<Result<()>>> {
    // Marks take over TTYs' folder reports, as focusing's do, so the two never run at once.
    let _turn = FOCUS_TURNS.lock.lock().unwrap_or_else(|e| e.into_inner());
    let before = terminals()?;
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
        let now = terminals().unwrap_or_default();
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
    let exited = hang_up(&ending.iter().map(|(i, _)| sessions[*i].0).collect::<Vec<_>>());
    let mut closing = Vec::new();
    for ((i, id), gone) in ending.iter().zip(exited) {
        if gone {
            closing.push(id.as_str());
        } else {
            out[*i] = Err(anyhow!("Its process (pid {}) didn't exit, so the tab stays.", sessions[*i].0));
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
