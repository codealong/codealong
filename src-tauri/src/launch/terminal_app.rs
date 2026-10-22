//! Apple's Terminal through its AppleScript dictionary. Every tab has a `tty`, so a session's tab
//! is found by its process's TTY. Its scripting is thin: `do script` types a line and always
//! presses Return after it, and it can't open a tab or close one. See .claude/rules/terminals.md.

use std::path::Path;

use anyhow::Result;

use super::{close_tty_terminals, Closing, focus_on_tty, fresh_line, launcher, pasted, sh_quote, type_on_tty, typed_line, wait_for_shell, HostKind, Placement, Submit, Terminal, TerminalHost, Typed};

const APP: &str = "Terminal";

fn osascript(script: &str, args: &[&str]) -> Result<String> {
    super::osascript_in(APP, script, args)
}

pub struct AppleTerminal;

impl Terminal for AppleTerminal {
    fn kind(&self) -> HostKind {
        HostKind::Terminal
    }

    fn name(&self) -> &'static str {
        APP
    }

    fn profile(&self) -> Option<&'static str> {
        Some("terminal")
    }

    fn bundle(&self) -> Option<&'static str> {
        Some("com.apple.Terminal")
    }

    /// Terminal comes with macOS.
    /// Every Mac has it; nothing else does.
    fn installed(&self) -> bool {
        crate::os::is_macos()
    }

    /// Terminal starts a login shell per tab, as Ghostty does.
    fn runs_as(&self, process: &str) -> bool {
        process == "Terminal"
    }

    /// Terminal can't open a tab from a script: always a window.
    fn run(&self, place: Placement, dir: &Path, input: &str, env: &[(String, String)]) -> Result<()> {
        open(place, dir, input, env)
    }

    fn focus(&self, at: &str) -> Result<bool> {
        focus(at)
    }

    fn focus_session(&self, _profile: &str, _pid: u32, tty: &Path, _title: &str, _cwd: &Path) -> Result<Option<TerminalHost>> {
        focus_on_tty(&AppleTerminal, tty)
    }

    /// Terminal can't press a key without Return, so there the box stays as it is.
    /// Terminal presses Return after whatever its script types, so it sends with nothing else
    /// (`terminal_type_session` asks it for nothing else).
    fn type_session(&self, pid: u32, _cwd: &Path, text: &str, _clear: bool, _submit: Submit, _passes: bool) -> Result<Typed> {
        type_on_tty(APP, pid, text, None, type_text)
    }

    fn closes_tabs(&self) -> bool {
        true
    }

    fn close_tabs(&self, tabs: &[&Closing]) -> Result<Vec<Result<()>>> {
        close_tty_terminals(tabs, APP, ttys, in_front, close)
    }
}

/// Finds the tab on a TTY: `{window id, tab index}`, or missing value. Windows are held by id
/// (see GHOSTTY_FOCUS), and one question per window gets every tab's TTY. Terminal goes on
/// listing a closed window, hidden, and a tab whose shell exited (kept open, as a profile can
/// say) keeps its TTY with no processes on it: a new tab can have that TTY, so only tabs with
/// processes in windows you can see count.
const TAB_ON: &str = r#"
on whereIs(want)
  tell application "Terminal"
    set wids to id of every window whose visible is true or miniaturized is true
    repeat with k from 1 to count of wids
      set wid to item k of wids
      set ttys to tty of every tab of window id wid
      repeat with i from 1 to count of ttys
        if item i of ttys is want then
          if (count of (processes of tab i of window id wid)) > 0 then return {wid, i}
        end if
      end repeat
    end repeat
  end tell
  return missing value
end whereIs
"#;

fn with_tab_on(body: &str) -> String {
    format!("{body}\n{TAB_ON}")
}

/// A new window, and its TTY. `do script` with no tab to run in opens a window of its own, and
/// types what it's given there: an empty line, which a shell just prompts again for. Argument:
/// whether Terminal comes forward (`front`) or not (`behind`).
const NEW: &str = r#"
on run argv
  tell application "Terminal"
    set t to do script ""
    if (item 1 of argv) is "front" then activate
    return tty of t
  end tell
end run
"#;

/// `<tty> <line>`: types the line into that tab; `do script` presses Return after it. Returns
/// the TTY, or "" when no tab has it.
const TYPE_LINE: &str = r#"
on run argv
  set loc to whereIs(item 1 of argv)
  if loc is missing value then return ""
  tell application "Terminal" to do script (item 2 of argv) in tab (item 2 of loc) of window id (item 1 of loc)
  return item 1 of argv
end run
"#;

/// `<tty>`: selects that tab, brings its window to the front and Terminal forward. Returns the
/// TTY, or "" when no tab has it.
const FOCUS: &str = r#"
on run argv
  set loc to whereIs(item 1 of argv)
  if loc is missing value then return ""
  set wid to item 1 of loc
  tell application "Terminal"
    set selected of tab (item 2 of loc) of window id wid to true
    try
      set miniaturized of window id wid to false
    end try
    try
      set index of window id wid to 1
    end try
    try
      activate
    end try
  end tell
  return item 1 of argv
end run
"#;

/// `<tty> <text>`: types the text into that tab; `do script` presses Return right after it.
/// Returns the TTY, or "" when no tab has it, in which case nothing was typed.
const TYPE: &str = r#"
on run argv
  set loc to whereIs(item 1 of argv)
  if loc is missing value then return ""
  tell application "Terminal" to do script (item 2 of argv) in tab (item 2 of loc) of window id (item 1 of loc)
  return item 1 of argv
end run
"#;

/// The TTY of the tab you're in: the front window's selected tab while Terminal is in front,
/// else "".
const IN_FRONT: &str = r#"
on run argv
  tell application "Terminal"
    if not frontmost then return ""
    try
      return tty of selected tab of (first window whose visible is true)
    end try
  end tell
  return ""
end run
"#;

/// Every live tab's TTY, one per line (see TAB_ON).
const TTYS: &str = r#"
on run argv
  set out to ""
  tell application "Terminal"
    set wids to id of every window whose visible is true or miniaturized is true
    repeat with k from 1 to count of wids
      set wid to item k of wids
      set ttys to tty of every tab of window id wid
      repeat with i from 1 to count of ttys
        if (count of (processes of tab i of window id wid)) > 0 then set out to out & (item i of ttys) & linefeed
      end repeat
    end repeat
  end tell
  return out
end run
"#;

/// `<tty>...`: closes those tabs. Terminal can close a window but not a tab, so a tab with others
/// beside it is closed by its shell exiting, which closes it unless its profile keeps a window
/// whose shell exited (Settings › Profiles › Shell › When the shell exits).
const CLOSE: &str = r#"
on run argv
  repeat with n from 1 to count of argv
    set loc to whereIs(item n of argv)
    if loc is not missing value then
      tell application "Terminal"
        try
          if (count of tabs of window id (item 1 of loc)) is 1 then
            close window id (item 1 of loc)
          else
            do script "exit" in tab (item 2 of loc) of window id (item 1 of loc)
          end if
        end try
      end tell
    end if
  end repeat
  return "ok"
end run
"#;

/// Opens a window whose shell `launcher` sets up (Terminal takes no folder or environment from a
/// script), then types `input` there. Each line waits for the shell that reads it to be ready
/// (see `wait_for_shell`). The line that runs the launcher starts with a space, which keeps it
/// out of the history of shells set up to skip such lines. Terminal opens only windows, so of
/// `place` only whether it comes forward counts.
pub fn open(place: Placement, dir: &Path, input: &str, env: &[(String, String)]) -> Result<()> {
    let script = launcher(dir, env)?;
    let result = open_with(&script, typed_line(input), place.forward());
    if result.is_err() {
        // It may never have run, holding the prompt.
        let _ = std::fs::remove_file(&script);
    }
    result
}

fn open_with(script: &Path, input: &str, forward: &str) -> Result<()> {
    let type_line = |tty: &str, line: &str| -> Result<()> {
        if osascript(&with_tab_on(TYPE_LINE), &[tty, &fresh_line(line)])?.trim().is_empty() {
            anyhow::bail!("Terminal's new window closed before Codealong could type into it.");
        }
        Ok(())
    };
    let tty = osascript(NEW, &[forward])?.trim().to_string();
    // A shell without a line editor never says it's ready, and still reads a line typed ahead.
    wait_for_shell(&tty, None);
    type_line(&tty, &format!(" exec /bin/sh {}", sh_quote(&script.to_string_lossy())))?;
    if !wait_for_shell(&tty, Some(script)) && script.exists() {
        anyhow::bail!("Terminal's new window didn't run Codealong's setup within 15 seconds, so nothing was typed there.");
    }
    if !input.is_empty() {
        type_line(&tty, input)?;
    }
    Ok(())
}

/// Brings forward the tab on `tty`. `Ok(false)` when Terminal has no tab there.
pub fn focus(tty: &str) -> Result<bool> {
    Ok(!osascript(&with_tab_on(FOCUS), &[tty])?.trim().is_empty())
}

/// Types `text` into the tab on `tty` as a paste, with the Return `do script` adds: a program
/// that takes bracketed pastes (Claude Code, Codex) gets it whole, then the key.
pub fn type_text(tty: &str, text: &str) -> Result<bool> {
    Ok(!osascript(&with_tab_on(TYPE), &[tty, &pasted(text)])?.trim().is_empty())
}

pub fn ttys() -> Result<Vec<String>> {
    Ok(osascript(TTYS, &[])?.lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect())
}

pub fn in_front() -> Result<String> {
    Ok(osascript(IN_FRONT, &[])?.trim().to_string())
}

pub fn close(ttys: &[&str]) -> Result<()> {
    osascript(&with_tab_on(CLOSE), ttys).map(|_| ())
}

#[cfg(test)]
pub(super) fn scripts() -> Vec<String> {
    let mut all: Vec<String> = [NEW, IN_FRONT, TTYS].iter().map(|s| s.to_string()).collect();
    all.extend([FOCUS, TYPE, TYPE_LINE, CLOSE].iter().map(|s| with_tab_on(s)));
    all
}
