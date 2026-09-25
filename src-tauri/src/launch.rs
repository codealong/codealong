//! Opening things where they live: the terminal and editor app profiles (Ghostty through
//! AppleScript, Cursor through its CLI) and the Claude and Codex deep links. See *Apps*.

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use anyhow::{anyhow, bail, Result};

use crate::procs;
use crate::util::home;

pub fn terminal_name(id: &str) -> String {
    match id {
        "ghostty" => "Ghostty".to_string(),
        other => other.to_string(),
    }
}

pub fn editor_name(id: &str) -> String {
    match id {
        "cursor" => "Cursor".to_string(),
        other => other.to_string(),
    }
}

/// Arguments: the folder, the input to type (or ""), then any `KEY=VALUE` environment entries.
const GHOSTTY_NEW_WINDOW: &str = r#"
on run argv
  set thePath to item 1 of argv
  set theInput to item 2 of argv
  set theEnv to {}
  if (count of argv) > 2 then set theEnv to items 3 thru -1 of argv
  tell application "Ghostty"
    if theInput is "" then
      set cfg to {initial working directory:thePath}
    else if theEnv is {} then
      set cfg to {initial working directory:thePath, initial input:theInput}
    else
      set cfg to {initial working directory:thePath, initial input:theInput, environment variables:theEnv}
    end if
    new window with configuration cfg
    activate
  end tell
  return "ok"
end run
"#;

/// Three questions give three lists, which only line up if no terminal opened or closed in
/// between, so it asks for the ids again to check, and gives up with nothing rather than pair
/// a terminal with another's folder.
const GHOSTTY_LIST: &str = r#"
on run argv
  set out to ""
  tell application "Ghostty"
    repeat 3 times
      set ids to id of every terminal
      set dirs to working directory of every terminal
      set names to name of every terminal
      set again to id of every terminal
      if again = ids and (count of dirs) = (count of ids) and (count of names) = (count of ids) then
        repeat with i from 1 to count of ids
          set out to out & (item i of ids) & tab & (item i of dirs) & tab & (item i of names) & linefeed
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

fn osascript(script: &str, args: &[&str]) -> Result<String> {
    let mut full = vec!["-"];
    full.extend_from_slice(args);
    procs::run_stdin("/usr/bin/osascript", &full, script, OSASCRIPT_TIMEOUT).map_err(|e| {
        let msg = e.to_string();
        if msg.contains("-1743") || msg.contains("Not authorized") {
            anyhow!("Codealong isn't allowed to control Ghostty. Turn it on in System Settings → Privacy & Security → Automation.")
        } else if msg.contains("timed out") {
            anyhow!("Ghostty didn't answer within a minute. It may still open the window; if macOS asked whether Codealong may control Ghostty, allow it and try again.")
        } else {
            anyhow!("Ghostty didn't respond: {msg}")
        }
    })
}

/// Opens a new terminal window at `path`, optionally typing `input` (a command plus return).
pub fn terminal_open(profile: &str, path: &Path, input: Option<&str>) -> Result<()> {
    match profile {
        "ghostty" => {
            let dir = path.to_string_lossy();
            match osascript(GHOSTTY_NEW_WINDOW, &[&dir, input.unwrap_or("")]) {
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

/// Opens a new terminal window at `path` with `env` set in its shell, and types `input` (a
/// command plus return). The shell expands variables in `input` from `env`.
pub fn terminal_run(profile: &str, path: &Path, input: &str, env: &[(String, String)]) -> Result<()> {
    match profile {
        "ghostty" => {
            let dir = path.to_string_lossy();
            let entries: Vec<String> = env.iter().map(|(k, v)| format!("{k}={v}")).collect();
            let mut args: Vec<&str> = vec![&dir, input];
            args.extend(entries.iter().map(String::as_str));
            osascript(GHOSTTY_NEW_WINDOW, &args).map(|_| ())
        }
        other => bail!("There's no terminal profile called {other:?} yet. Use \"ghostty\" in config.toml."),
    }
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
    let in_ghostty = procs::run("/bin/ps", &["-A", "-o", "pid=,ppid=,comm="], None, Duration::from_secs(3))
        .map(|table| runs_under(&table, pid, "ghostty"))
        .unwrap_or(true);
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

/// Types `text` into the terminal tab a live session runs in and presses Return, without
/// bringing the tab forward. `Ok(false)` when the session isn't in a Ghostty tab Codealong can
/// find exactly.
///
/// The tab is found the way focusing finds it, by a unique folder reported on the session's
/// TTY, and only that way: a tab matched by title or folder could be another agent's, and
/// typing there sends your words to the wrong session.
pub fn terminal_type_session(profile: &str, pid: u32, cwd: &Path, text: &str) -> Result<bool> {
    if profile != "ghostty" {
        bail!("There's no terminal profile called {profile:?} yet.");
    }
    let Some(tty) = tty_of(pid) else { return Ok(false) };
    let in_ghostty = procs::run("/bin/ps", &["-A", "-o", "pid=,ppid=,comm="], None, Duration::from_secs(3))
        .map(|table| runs_under(&table, pid, "ghostty"))
        .unwrap_or(false);
    if !in_ghostty {
        return Ok(false);
    }
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
    if id.is_empty() {
        return Ok(false);
    }
    Ok(!osascript(GHOSTTY_TYPE, &[&id, text])?.trim().is_empty())
}

/// Whether `pid` runs inside the app whose executable is named `exe`, from a `ps -A -o
/// pid=,ppid=,comm=` table. Ghostty starts a login shell per tab: claude → zsh → login → ghostty.
pub fn runs_under(table: &str, pid: u32, exe: &str) -> bool {
    let parents: std::collections::HashMap<u32, (u32, &str)> = table
        .lines()
        .filter_map(|line| {
            // `  849     1 /Applications/Ghostty.app/Contents/MacOS/ghostty`; names can have spaces.
            let (pid, rest) = line.trim_start().split_once(char::is_whitespace)?;
            let rest = rest.trim_start();
            let (ppid, comm) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
            Some((pid.parse().ok()?, (ppid.parse().ok()?, comm.trim())))
        })
        .collect();
    let mut at = pid;
    for _ in 0..16 {
        let Some(&(ppid, comm)) = parents.get(&at) else { return false };
        if Path::new(comm).file_name().is_some_and(|n| n == exe) {
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
    let out = procs::run("/bin/ps", &["-o", "tty=", "-p", &pid.to_string()], None, Duration::from_secs(3)).ok()?;
    let tty = out.trim();
    tty.starts_with("tty").then(|| Path::new("/dev").join(tty))
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

pub fn editor_open(profile: &str, path: &Path) -> Result<()> {
    let dir = path.to_string_lossy();
    match profile {
        "cursor" => procs::run("cursor", &[&dir], None, Duration::from_secs(15))
            .or_else(|_| procs::run("/usr/bin/open", &["-a", "Cursor", &dir], None, Duration::from_secs(15)))
            .map(|_| ())
            .map_err(|_| anyhow!("Couldn't open Cursor. Is it installed?")),
        other => bail!("There's no editor profile called {other:?} yet. Use \"cursor\" in config.toml."),
    }
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
        for (i, script) in [GHOSTTY_NEW_WINDOW, GHOSTTY_LIST, GHOSTTY_FOCUS, GHOSTTY_FIND, GHOSTTY_TYPE].iter().enumerate() {
            let out = std::env::temp_dir().join(format!("codealong-test-{i}.scpt"));
            let r = procs::run_stdin("/usr/bin/osacompile", &["-o", &out.to_string_lossy()], script, Duration::from_secs(20));
            assert!(r.is_ok(), "script {i} failed to compile: {r:?}");
        }
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
        terminal_run("ghostty", &std::env::temp_dir(), &input, &[("CODEALONG_PROMPT".to_string(), prompt.to_string())]).unwrap();
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
        let table = "  849     1 /Applications/Ghostty.app/Contents/MacOS/ghostty
  958   849 /usr/bin/login
  997   958 -/bin/zsh
68277   997 claude
 4100     1 tmux
 4101  4100 -/bin/zsh
 4102  4101 claude
 5000     1 /Applications/Google Chrome.app/Contents/MacOS/Google Chrome
";
        assert!(runs_under(table, 68277, "ghostty"));
        assert!(!runs_under(table, 4102, "ghostty"));
        assert!(!runs_under(table, 5000, "ghostty"));
        assert!(!runs_under(table, 999_999, "ghostty"));
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
}
