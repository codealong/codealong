//! iTerm2 through its AppleScript dictionary. Every session has a `tty`, so a session's tab is
//! found by its process's TTY, with no marks as Ghostty needs. See *Harness facts*, iTerm2.

use std::path::Path;

use anyhow::Result;

use super::{fresh_line, launcher, pasted, placement, sh_quote, typed_line, wait_for_shell, Key, Placement, Tab};

const APP: &str = "iTerm2";

fn osascript(script: &str, args: &[&str]) -> Result<String> {
    super::osascript_in(APP, script, args)
}

/// Finds the session on a TTY: `{window id, tab index, session index}`, or missing value.
/// Windows are held by id (see GHOSTTY_FOCUS), and one question per window gets every session's
/// TTY. Lists are walked by index: `contents of` inside the tell would ask iTerm2 for a
/// session's contents.
const SESSION_ON: &str = r#"
on whereIs(want)
  tell application "iTerm"
    set wids to id of every window whose visible is true or miniaturized is true
    repeat with k from 1 to count of wids
      set wid to item k of wids
      set ttys to tty of every session of every tab of window id wid
      repeat with i from 1 to count of ttys
        set tabTtys to item i of ttys
        repeat with j from 1 to count of tabTtys
          if item j of tabTtys is want then return {wid, i, j}
        end repeat
      end repeat
    end repeat
  end tell
  return missing value
end whereIs

on sessionOn(want)
  set loc to whereIs(want)
  if loc is missing value then return missing value
  tell application "iTerm" to return session (item 3 of loc) of tab (item 2 of loc) of window id (item 1 of loc)
end sessionOn
"#;

fn with_session_on(body: &str) -> String {
    format!("{body}\n{SESSION_ON}")
}

/// Arguments: where it goes (`window`, `front`, or `beside` the sessions whose TTYs are listed
/// next, one per line), and the command the session runs. Returns the new session's TTY.
/// Windows come front to back, so the first holding any of those sessions is the nearest.
const NEW: &str = r#"
on run argv
  set placeAt to item 1 of argv
  set nearTtys to paragraphs of (item 2 of argv)
  set theCommand to item 3 of argv
  tell application "iTerm"
    set dest to missing value
    if placeAt is "front" then
      try
        if visible of current window then set dest to id of current window
      end try
    else if placeAt is "beside" then
      set wids to id of every window whose visible is true or miniaturized is true
      repeat with k from 1 to count of wids
        set wid to item k of wids
        set ttys to tty of every session of every tab of window id wid
        repeat with i from 1 to count of ttys
          set tabTtys to item i of ttys
          repeat with j from 1 to count of tabTtys
            if nearTtys contains (item j of tabTtys) then set dest to wid
          end repeat
        end repeat
        if dest is not missing value then exit repeat
      end repeat
    end if
    if dest is missing value then
      set s to current session of (create window with default profile command theCommand)
    else
      tell window id dest to set t to (create tab with default profile command theCommand)
      set s to current session of t
      try
        select window id dest
      end try
    end if
    activate
    return tty of s
  end tell
end run
"#;

/// `<tty> <line>`: types the line into that session and presses Return. Returns the TTY, or ""
/// when no session has it.
const TYPE_LINE: &str = r#"
on run argv
  set s to sessionOn(item 1 of argv)
  if s is missing value then return ""
  tell application "iTerm" to tell s to write text (item 2 of argv)
  return item 1 of argv
end run
"#;

/// Every session: its TTY, folder and name, a tab-separated line each. The separator is set
/// outside the tell, where `tab` is iTerm2's tab class.
const LIST: &str = r#"
on run argv
  set out to ""
  set sep to character id 9
  tell application "iTerm"
    set wids to id of every window whose visible is true or miniaturized is true
    repeat with k from 1 to count of wids
      set w to window id (item k of wids)
      repeat with i from 1 to count of tabs of w
        set ss to sessions of tab i of w
        repeat with j from 1 to count of ss
          set s to item j of ss
          set here to ""
          try
            set here to variable of s named "session.path"
          end try
          set out to out & (tty of s) & sep & here & sep & (name of s) & linefeed
        end repeat
      end repeat
    end repeat
  end tell
  return out
end run
"#;

/// `<tty>`: selects that session, its tab and its window, and brings iTerm2 forward. Returns the
/// TTY, or "" when no session has it.
const FOCUS: &str = r#"
on run argv
  set loc to whereIs(item 1 of argv)
  if loc is missing value then return ""
  set wid to item 1 of loc
  tell application "iTerm"
    try
      select tab (item 2 of loc) of window id wid
    end try
    try
      select session (item 3 of loc) of tab (item 2 of loc) of window id wid
    end try
    try
      set miniaturized of window id wid to false
    end try
    try
      select window id wid
    end try
    try
      activate
    end try
  end tell
  return item 1 of argv
end run
"#;

/// `<tty> <text>`: writes the text into that session, then presses Return a moment later, as
/// GHOSTTY_TYPE does. `write text` types exactly what it's given (plus a Return unless told
/// not to), so the text comes wrapped as a bracketed paste already. Returns the TTY, or "" when
/// no session has it, in which case nothing was typed.
const TYPE: &str = r#"
on run argv
  set s to sessionOn(item 1 of argv)
  if s is missing value then return ""
  tell application "iTerm"
    tell s to write text (item 2 of argv) newline no
    delay 0.3
    tell s to write text ""
  end tell
  return item 1 of argv
end run
"#;

/// `<tty> <characters>`: types them into that session with no Return.
const PRESS: &str = r#"
on run argv
  set s to sessionOn(item 1 of argv)
  if s is missing value then error "That session is gone."
  tell application "iTerm"
    tell s to write text (item 2 of argv) newline no
  end tell
  return "ok"
end run
"#;

/// The TTY of the session you're in: iTerm2's current one while iTerm2 is in front, else "".
const IN_FRONT: &str = r#"
on run argv
  tell application "iTerm"
    if not frontmost then return ""
    try
      if visible of current window then return tty of current session of current window
    end try
  end tell
  return ""
end run
"#;

/// Every session's TTY, one per line.
const TTYS: &str = r#"
on run argv
  set out to ""
  tell application "iTerm"
    set wids to id of every window whose visible is true or miniaturized is true
    repeat with k from 1 to count of wids
      set ttys to tty of every session of every tab of window id (item k of wids)
      repeat with i from 1 to count of ttys
        set tabTtys to item i of ttys
        repeat with j from 1 to count of tabTtys
          set out to out & (item j of tabTtys) & linefeed
        end repeat
      end repeat
    end repeat
  end tell
  return out
end run
"#;

/// `<tty>...`: closes those sessions, skipping any that closed by themselves already. iTerm2
/// doesn't ask before a script closes a session.
const CLOSE: &str = r#"
on run argv
  repeat with n from 1 to count of argv
    set s to sessionOn(item n of argv)
    if s is not missing value then
      tell application "iTerm"
        try
          close s
        end try
      end tell
    end if
  end repeat
  return "ok"
end run
"#;

/// Opens a session running a login shell in `dir` with `env` set, then types `input` there once
/// the shell is ready for it. iTerm2 runs the command it's given in place of the profile's
/// shell: `launcher` is that command, and becomes the login shell once it has set things up.
pub fn open(place: Placement, dir: &Path, input: &str, env: &[(String, String)]) -> Result<()> {
    let (at, beside) = placement(place, sessions);
    let script = launcher(dir, env)?;
    let command = format!("/bin/sh {}", sh_quote(&script.to_string_lossy()));
    let tty = match osascript(NEW, &[at, &beside, &command]) {
        Ok(tty) => tty.trim().to_string(),
        Err(e) => {
            // It never ran, so it's still there holding the prompt.
            let _ = std::fs::remove_file(&script);
            return Err(e);
        }
    };
    let line = typed_line(input);
    if line.is_empty() {
        return Ok(());
    }
    if !wait_for_shell(&tty, Some(&script)) && script.exists() {
        let _ = std::fs::remove_file(&script);
        anyhow::bail!("iTerm2's new tab didn't start its shell within 15 seconds, so nothing was typed there.");
    }
    if osascript(&with_session_on(TYPE_LINE), &[&tty, &fresh_line(line)])?.trim().is_empty() {
        anyhow::bail!("iTerm2's new tab closed before Codealong could type into it.");
    }
    Ok(())
}

/// Every session, with its TTY as its id.
pub fn sessions() -> Result<Vec<Tab>> {
    Ok(osascript(LIST, &[])?
        .lines()
        .filter_map(|line| {
            let mut parts = line.splitn(3, '\t');
            Some(Tab { id: parts.next()?.to_string(), cwd: parts.next()?.to_string(), name: parts.next().unwrap_or("").to_string() })
        })
        .filter(|t| !t.id.is_empty())
        .collect())
}

/// Brings forward the session on `tty`. `Ok(false)` when iTerm2 has no session there.
pub fn focus(tty: &str) -> Result<bool> {
    Ok(!osascript(&with_session_on(FOCUS), &[tty])?.trim().is_empty())
}

/// Types `text` into the session on `tty` as a paste, then Return. `Ok(false)` when there's no
/// such session, so nothing was typed.
pub fn type_text(tty: &str, text: &str) -> Result<bool> {
    Ok(!osascript(&with_session_on(TYPE), &[tty, &pasted(text)])?.trim().is_empty())
}

/// Presses one key in the session on `tty`, as if typed: `write text` sends it as it is, so a
/// digit arrives as the plain character, which Claude Code's prompt takes.
pub fn press(tty: &str, key: Key) -> Result<()> {
    let chars = match key {
        Key::Digit1 => "1",
        Key::Escape => "\x1b",
        Key::CtrlC => "\x03",
    };
    osascript(&with_session_on(PRESS), &[tty, chars]).map(|_| ())
}

pub fn ttys() -> Result<Vec<String>> {
    Ok(osascript(TTYS, &[])?.lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect())
}

pub fn in_front() -> Result<String> {
    Ok(osascript(IN_FRONT, &[])?.trim().to_string())
}

pub fn close(ttys: &[&str]) -> Result<()> {
    osascript(&with_session_on(CLOSE), ttys).map(|_| ())
}

#[cfg(test)]
pub(super) fn scripts() -> Vec<String> {
    let mut all: Vec<String> = [NEW, LIST, IN_FRONT, TTYS].iter().map(|s| s.to_string()).collect();
    all.extend([FOCUS, TYPE, TYPE_LINE, PRESS, CLOSE].iter().map(|s| with_session_on(s)));
    all
}
