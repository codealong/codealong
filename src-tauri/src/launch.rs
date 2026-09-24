//! Opening things where they live: the terminal and editor app profiles (Ghostty through
//! AppleScript, Cursor through its CLI) and the Claude and Codex deep links. See *Apps*.

use std::path::Path;
use std::time::Duration;

use anyhow::{anyhow, bail, Result};

use crate::procs;

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

const GHOSTTY_NEW_WINDOW: &str = r#"
on run argv
  set thePath to item 1 of argv
  set theInput to item 2 of argv
  tell application "Ghostty"
    if theInput is "" then
      set cfg to {initial working directory:thePath}
    else
      set cfg to {initial working directory:thePath, initial input:theInput}
    end if
    new window with configuration cfg
    activate
  end tell
  return "ok"
end run
"#;

const GHOSTTY_FOCUS: &str = r#"
on run argv
  set wantName to item 1 of argv
  set wantDir to item 2 of argv
  tell application "Ghostty"
    if wantName is not "" then
      repeat with t in terminals
        if (name of t) contains wantName then
          focus t
          activate
          return "ok"
        end if
      end repeat
    end if
    repeat with t in terminals
      if (working directory of t) is wantDir then
        focus t
        activate
        return "ok"
      end if
    end repeat
  end tell
  return "none"
end run
"#;

fn osascript(script: &str, args: &[&str]) -> Result<String> {
    let mut full = vec!["-"];
    full.extend_from_slice(args);
    procs::run_stdin("/usr/bin/osascript", &full, script, Duration::from_secs(10)).map_err(|e| {
        let msg = e.to_string();
        if msg.contains("-1743") || msg.contains("Not authorized") {
            anyhow!("Codealong isn't allowed to control Ghostty. Turn it on in System Settings → Privacy & Security → Automation.")
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

/// Focuses the terminal a live session runs in. `Ok(false)` when it can't be found.
pub fn terminal_focus(profile: &str, name: Option<&str>, cwd: &Path) -> Result<bool> {
    match profile {
        "ghostty" => {
            let out = osascript(GHOSTTY_FOCUS, &[name.unwrap_or(""), &cwd.to_string_lossy()])?;
            Ok(out.trim() == "ok")
        }
        other => bail!("There's no terminal profile called {other:?} yet."),
    }
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

pub fn claude_app_url(session_id: &str) -> String {
    format!("claude://code/continue?session={}", urlencoding::encode(session_id))
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
        assert_eq!(claude_app_url("87412567-2cd2"), "claude://code/continue?session=87412567-2cd2");
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
        for (i, script) in [GHOSTTY_NEW_WINDOW, GHOSTTY_FOCUS].iter().enumerate() {
            let out = std::env::temp_dir().join(format!("codealong-test-{i}.scpt"));
            let r = procs::run_stdin("/usr/bin/osacompile", &["-o", &out.to_string_lossy()], script, Duration::from_secs(20));
            assert!(r.is_ok(), "script {i} failed to compile: {r:?}");
        }
    }
}
