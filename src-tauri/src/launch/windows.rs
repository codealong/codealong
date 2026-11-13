//! The terminal on Windows, `[apps] terminal = "terminal"` there as on Linux: Windows Terminal
//! (`wt.exe`, which Windows 11 has). It takes no scripts and no keys from outside, so a new
//! terminal is a new window running what Codealong would have typed, which Codealong can't type
//! into, find or bring forward afterwards. What Codealong types is a POSIX shell's (sh's quoting,
//! the `$VAR`s of the environment it sets), so the window runs it in Git Bash's bash, which Claude
//! Code needs on Windows too, from a script beside the others' launchers (launch.rs `launcher`),
//! since Windows Terminal takes a `;` on its command line for one of its own.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{anyhow, Result};

use super::{typed_line, HostKind, Placement, Submit, Terminal, TerminalHost, Typed};
use crate::util::sh_quote;

/// The names Windows Terminal's processes have: the window's, and the console host each tab
/// runs its program under.
const PROCESSES: [&str; 2] = ["WindowsTerminal", "OpenConsole"];

/// Where a program is on the user's PATH, by its names with Windows' extensions.
fn on_path(command: &str) -> Option<PathBuf> {
    #[cfg(test)]
    if let Some(staged) = super::stage::on_path(command) {
        return staged;
    }
    let names = crate::os::procs::names_to_find(std::ffi::OsStr::new(command));
    std::env::split_paths(crate::procs::user_path()).flat_map(|d| names.iter().map(move |n| d.join(n)).collect::<Vec<_>>()).find(|p| p.is_file())
}

fn wt() -> Option<PathBuf> {
    on_path("wt")
}

/// Git Bash's bash: on the PATH, but not Windows' own `bash.exe` in System32, which is WSL's and
/// runs in Linux; else where Git for Windows installs it.
fn bash() -> Option<PathBuf> {
    let system = std::env::var_os("SystemRoot").map(PathBuf::from);
    on_path("bash").filter(|p| system.as_ref().is_none_or(|s| !p.starts_with(s))).or_else(|| {
        ["ProgramFiles", "ProgramW6432", "LOCALAPPDATA"]
            .iter()
            .filter_map(std::env::var_os)
            .map(|d| if Path::new(&d).ends_with("Local") { PathBuf::from(d).join("Programs/Git/bin/bash.exe") } else { PathBuf::from(d).join("Git/bin/bash.exe") })
            .find(|p| p.is_file())
    })
}

/// The script a new window's bash runs: it deletes itself, goes to `dir`, sets `env`, runs `line`
/// and stays as a login shell, as a new tab's shell would once the line was typed.
fn script_text(path: &Path, dir: &Path, env: &[(String, String)], line: &str) -> String {
    let mut s = format!("rm -f {}\ncd {} 2>/dev/null\n", sh_quote(&path.to_string_lossy()), sh_quote(&dir.to_string_lossy()));
    for (k, v) in env {
        // Names come from Codealong; anything else couldn't be exported anyway.
        if !k.is_empty() && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') && !k.starts_with(|c: char| c.is_ascii_digit()) {
            s.push_str(&format!("export {k}={}\n", sh_quote(v)));
        }
    }
    if !line.is_empty() {
        s.push_str(line);
        s.push('\n');
    }
    s.push_str("exec bash -l\n");
    s
}

/// What starts the window: Windows Terminal, in a window of its own at `dir`, named `label`,
/// running bash on the script.
fn window_args(wt: &Path, dir: &Path, label: Option<&str>, bash: &Path, script: &Path) -> Vec<String> {
    let mut args = vec![wt.to_string_lossy().into_owned(), "-w".into(), "new".into(), "new-tab".into(), "-d".into(), dir.to_string_lossy().into_owned()];
    if let Some(label) = label {
        args.extend(["--title".into(), label.replace(';', ",")]);
    }
    args.extend([bash.to_string_lossy().into_owned(), "-l".into(), script.to_string_lossy().into_owned()]);
    args
}

pub struct WindowsTerminal;

impl Terminal for WindowsTerminal {
    fn kind(&self) -> HostKind {
        HostKind::Windows
    }

    fn name(&self) -> &'static str {
        "Windows Terminal"
    }

    fn profile(&self) -> Option<&'static str> {
        Some("terminal")
    }

    fn installed(&self) -> bool {
        wt().is_some()
    }

    fn runs_as(&self, process: &str) -> bool {
        PROCESSES.contains(&process)
    }

    fn run(&self, place: Placement, dir: &Path, input: &str, env: &[(String, String)]) -> Result<()> {
        let wt = wt().ok_or_else(|| anyhow!("Windows Terminal isn't installed, and Codealong opens terminals on Windows in it."))?;
        let bash = bash().ok_or_else(|| anyhow!("Git Bash isn't installed: Codealong runs a new terminal's commands in its bash, as Claude Code needs it on Windows. Install Git for Windows."))?;
        let script = std::env::temp_dir().join(format!("codealong-launch-{}-{}.sh", std::process::id(), crate::util::now_ms()));
        crate::store::write_private(&script, script_text(&script, dir, env, typed_line(input)))?;
        let all = window_args(&wt, dir, place.label, &bash, &script);
        let args: Vec<&str> = all[1..].iter().map(String::as_str).collect();
        crate::os::open::start_detached(&all[0], &args, Duration::from_secs(10))
    }

    fn focus_session(&self, _profile: &str, _pid: u32, _tty: &Path, _title: &str, _cwd: &Path) -> Result<Option<TerminalHost>> {
        Ok(None)
    }

    fn type_session(&self, _pid: u32, _cwd: &Path, _text: &str, _clear: bool, _submit: Submit, _passes: bool) -> Result<Typed> {
        Ok(Typed::Nowhere("Windows Terminal can't be typed into from outside".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_window_runs_what_would_be_typed_in_bash_from_a_script() {
        let script = Path::new("C:/Temp/codealong-launch-1-2.sh");
        let text = script_text(script, Path::new("C:/work/my app"), &[("CODEALONG_PROMPT".into(), "Fix it's bug".into()), ("BAD NAME".into(), "x".into())], "claude --session-id x");
        assert_eq!(text, "rm -f 'C:/Temp/codealong-launch-1-2.sh'\ncd 'C:/work/my app' 2>/dev/null\nexport CODEALONG_PROMPT='Fix it'\\''s bug'\nclaude --session-id x\nexec bash -l\n");
        let args = window_args(Path::new("C:/wt.exe"), Path::new("C:/work"), Some("app; fix"), Path::new("C:/Git/bin/bash.exe"), script);
        assert_eq!(args, ["C:/wt.exe", "-w", "new", "new-tab", "-d", "C:/work", "--title", "app, fix", "C:/Git/bin/bash.exe", "-l", "C:/Temp/codealong-launch-1-2.sh"]);
    }
}
