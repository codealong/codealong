//! Opening things where they live: the terminal and editor app profiles (Ghostty through
//! AppleScript, Cursor through its CLI) and the Claude and Codex deep links. See *Apps*.

use std::path::Path;
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

const GHOSTTY_LIST: &str = r#"
on run argv
  set out to ""
  tell application "Ghostty"
    set ids to id of every terminal
    set dirs to working directory of every terminal
    set names to name of every terminal
    repeat with i from 1 to count of ids
      set out to out & (item i of ids) & tab & (item i of dirs) & tab & (item i of names) & linefeed
    end repeat
  end tell
  return out
end run
"#;

/// `tag <marker>`: waits two to three seconds for a terminal whose folder contains the marker
/// and focuses it. `id <terminal id>`: focuses that terminal. Returns the focused terminal's id.
/// The wait is timed, not counted: with many terminals open each look takes longer.
const GHOSTTY_FOCUS: &str = r#"
on run argv
  set mode to item 1 of argv
  set want to item 2 of argv
  tell application "Ghostty"
    if mode is "id" then
      if my focusTerminal(want) then return want
      return ""
    end if
    set started to current date
    repeat
      set ids to id of every terminal
      set dirs to working directory of every terminal
      repeat with i from 1 to count of ids
        if (item i of dirs) contains want then
          set found to item i of ids
          if my focusTerminal(found) then return found
        end if
      end repeat
      if ((current date) - started) > 2 then exit repeat
      delay 0.05
    end repeat
  end tell
  return ""
end run

on focusTerminal(want)
  tell application "Ghostty"
    repeat with w in windows
      repeat with tb in tabs of w
        if (id of every terminal of tb) contains want then
          select tab tb
          activate window w
          focus (first terminal of tb whose id is want)
          activate
          return true
        end if
      end repeat
    end repeat
  end tell
  return false
end focusTerminal
"#;

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

/// Focuses the terminal tab a live session runs in. `Ok(false)` when there's no such tab.
///
/// The tab is found exactly: Codealong reports a unique folder (OSC 7) on the session's TTY,
/// finds the Ghostty terminal showing that folder, selects its tab, then reports the real
/// folder again. Claude Code rewrites tab titles as it works but never the folder, so the mark
/// holds. Where that can't work (tmux, a session outside Ghostty), tabs are matched by title,
/// then by folder.
pub fn terminal_focus_session(profile: &str, pid: u32, title: &str, cwd: &Path) -> Result<bool> {
    if profile != "ghostty" {
        bail!("There's no terminal profile called {profile:?} yet.");
    }
    let Some(tty) = tty_of(pid) else {
        bail!("This session runs in the background, so it has no terminal tab.");
    };
    let terminals = ghostty_terminals()?;
    let mark = format!("/tmp/codealong-{pid}-{}", crate::util::now_ms());
    if write_osc7(&tty, &mark).is_ok() {
        let found = osascript(GHOSTTY_FOCUS, &["tag", &mark]).unwrap_or_default();
        let found = found.trim();
        let real = terminals
            .iter()
            .find(|t| t.id == found)
            .map(|t| t.cwd.clone())
            .filter(|c| !c.is_empty())
            .unwrap_or_else(|| cwd.to_string_lossy().into_owned());
        let _ = write_osc7(&tty, &real);
        if !found.is_empty() {
            return Ok(true);
        }
    }
    match pick_by_title(&terminals, title, cwd) {
        Some(id) => Ok(!osascript(GHOSTTY_FOCUS, &["id", &id])?.trim().is_empty()),
        None => Ok(false),
    }
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

/// A tab titled like the session, preferring one in its folder; else any tab in its folder.
pub fn pick_by_title(terminals: &[GhosttyTerminal], title: &str, cwd: &Path) -> Option<String> {
    let dir = cwd.to_string_lossy();
    let titled: Vec<&GhosttyTerminal> = terminals.iter().filter(|t| !title.is_empty() && tab_title(&t.name) == title).collect();
    titled
        .iter()
        .find(|t| t.cwd == dir)
        .or_else(|| titled.first())
        .copied()
        .or_else(|| terminals.iter().find(|t| t.cwd == dir))
        .map(|t| t.id.clone())
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
        for (i, script) in [GHOSTTY_NEW_WINDOW, GHOSTTY_LIST, GHOSTTY_FOCUS].iter().enumerate() {
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
    fn matches_tabs_by_title_then_folder() {
        let t = |id: &str, cwd: &str, name: &str| GhosttyTerminal { id: id.into(), cwd: cwd.into(), name: name.into() };
        let tabs = vec![
            t("a", "/r/app4", "✳ RLS work and other tables"),
            t("b", "/r/app2", "✳ Cloud agent execution regressions"),
            t("c", "/r/app2", "✳ Cloud agent execution regressions ⑂"),
            t("d", "/r/app4", "…/projects/app4"),
        ];
        assert_eq!(pick_by_title(&tabs, "Cloud agent execution regressions ⑂", Path::new("/r/app2")).as_deref(), Some("c"));
        assert_eq!(pick_by_title(&tabs, "Cloud agent execution regressions", Path::new("/r/app2")).as_deref(), Some("b"));
        assert_eq!(pick_by_title(&tabs, "Something else", Path::new("/r/app4")).as_deref(), Some("a"));
        assert_eq!(pick_by_title(&tabs, "Something else", Path::new("/r/none")), None);
    }
}
