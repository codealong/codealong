//! Codex's terminal UIs on this Mac and the threads they show, the threads Codex's processes
//! have loaded, and its command as a terminal can run it.

use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// A Codex terminal UI running on this Mac.
#[derive(Debug, Clone, PartialEq)]
pub struct Terminal {
    pub pid: u32,
    pub cwd: PathBuf,
    /// The thread it was started on (`codex resume <id>`), when it was.
    pub thread: Option<String>,
    pub started_at: i64,
}

/// The rollouts Codex's processes hold open, by file name. A thread is loaded in one of them for
/// as long as its rollout is: its terminal UI (without the daemon), the daemon behind the CLI,
/// the app's app-server, an agent Codealong runs. A turn left open in a rollout none holds was
/// cut off: a terminal UI killed partway through a command wrote nothing more, and its command
/// ran on (0.157.1 in tmux, 2026-10-12). None when one of them couldn't be read, so nothing is
/// taken as let go.
///
/// Codex's processes are the ones whose executable is called `codex`, which the command line
/// can't always tell: the one Codealong runs agents through is under "Application Support".
pub fn held_rollouts(processes: &[crate::procs::PsLine]) -> Option<HashSet<OsString>> {
    // No processes at all is a `ps` that failed or ran out of time (`procs::Listing`): this one
    // at least is running.
    if processes.is_empty() {
        return None;
    }
    let mut held = HashSet::new();
    for p in processes.iter().filter(|p| p.command.contains("codex")) {
        if !crate::procs::parent_and_name(p.pid).is_some_and(|(_, name)| name == "codex") {
            continue;
        }
        match crate::procs::open_files(p.pid) {
            Some(files) => held.extend(files.iter().filter_map(|f| f.file_name()).filter(|n| is_rollout(n)).map(OsString::from)),
            // Gone since the list was made: it holds nothing now.
            None if !crate::procs::pid_alive(p.pid) => {}
            None => return None,
        }
    }
    Some(held)
}

/// Whether a file's name is a rollout's (`rollout-<time>-<thread>.jsonl`).
pub fn is_rollout(name: &std::ffi::OsStr) -> bool {
    name.to_str().is_some_and(|n| n.starts_with("rollout-") && n.ends_with(".jsonl"))
}

/// Codex's interactive terminals on this Mac. Its daemon, `exec` runs and the app's own
/// app-server aren't terminals; the CLI inside the app is one only as Codealong types it into a
/// terminal (`codex_cli`, with `TERMINAL_UI`), since whatever else the app runs it for isn't.
pub fn terminals(processes: &[crate::procs::PsLine]) -> Vec<Terminal> {
    processes
        .iter()
        .filter_map(|p| {
            let (thread, dir) = terminal_args(&p.command)?;
            let cwd = dir.or_else(|| crate::procs::cwd_of(p.pid))?;
            let cwd = std::fs::canonicalize(&cwd).unwrap_or(cwd);
            Some(Terminal { pid: p.pid, cwd, thread, started_at: crate::procs::proc_start_ms(p.pid)? })
        })
        .collect()
}

/// For a process command line that is Codex's interactive UI: the thread it resumed, and the
/// folder `-C` gave it. None for anything else. `ps` joins the arguments with spaces, so a
/// prompt reads as words; only the first word that isn't an option matters.
pub fn terminal_args(command: &str) -> Option<(Option<String>, Option<PathBuf>)> {
    let mut words = command.split_whitespace().peekable();
    let program = words.next()?;
    if program.rsplit('/').next() != Some("codex") || program.contains("/packages/") || program.contains(".app/") && !command.contains(TERMINAL_UI) {
        return None;
    }
    let mut dir = None;
    while let Some(w) = words.next() {
        match w {
            "-C" | "--cd" => dir = words.next().map(crate::util::expand),
            "-c" | "--config" | "--enable" | "--disable" | "--remote" | "--remote-auth-token-env" | "-m" | "--model" | "--local-provider" | "-p" | "--profile" | "-s" | "--sandbox" | "--add-dir" | "-a" | "--ask-for-approval" => {
                words.next();
            }
            // Images take several values, up to the next option.
            "-i" | "--image" => {
                while words.peek().is_some_and(|n| !n.starts_with('-')) {
                    words.next();
                }
            }
            "--" => return Some((None, dir)),
            _ if w.starts_with('-') => {}
            "resume" | "fork" => {
                let rest: Vec<&str> = words.collect();
                let id = rest.iter().find(|a| looks_like_uuid(a)).map(|a| a.to_string());
                let id = if w == "resume" { id } else { None };
                let dir = dir.or_else(|| rest.windows(2).find(|p| p[0] == "-C" || p[0] == "--cd").map(|p| crate::util::expand(p[1])));
                return Some((id, dir));
            }
            "exec" | "e" | "review" | "login" | "logout" | "mcp" | "plugin" | "app-server" | "remote-control" | "app" | "completion" | "update" | "doctor" | "sandbox" | "debug" | "apply" | "a" | "queue" | "archive" | "delete" | "migrate-rollouts" | "unarchive" | "cloud" | "exec-server" | "features" | "help" | "agents" | "mcp-server" | "proto" | "responses-api-proxy" => return None,
            // The prompt.
            _ => return Some((None, dir)),
        }
    }
    Some((None, dir))
}

fn looks_like_uuid(s: &str) -> bool {
    s.len() == 36 && s.chars().all(|c| c.is_ascii_hexdigit() || c == '-') && s.matches('-').count() == 4
}

/// Which terminal shows each thread. One started on a thread (`resume <id>`) shows it; the rest
/// show their folder's CLI threads that were active since they started, the newest terminal the
/// newest thread. `threads` is (id, folder, started, last activity, made in the CLI).
pub fn pair_terminals(terminals: &[Terminal], threads: &[(&str, &str, i64, i64, bool)]) -> HashMap<String, u32> {
    let mut out: HashMap<String, u32> = HashMap::new();
    for t in terminals {
        if let Some(id) = &t.thread {
            out.insert(id.clone(), t.pid);
        }
    }
    let mut folders: HashMap<&Path, Vec<&Terminal>> = HashMap::new();
    for t in terminals.iter().filter(|t| t.thread.is_none()) {
        folders.entry(t.cwd.as_path()).or_default().push(t);
    }
    for (dir, mut terms) in folders {
        terms.sort_by_key(|t| std::cmp::Reverse(t.started_at));
        let mut candidates: Vec<&(&str, &str, i64, i64, bool)> = threads.iter().filter(|th| th.4 && Path::new(th.1) == dir && !out.contains_key(th.0)).collect();
        candidates.sort_by_key(|th| std::cmp::Reverse(th.2));
        for term in terms {
            // A thread quiet since before the terminal started isn't the one it shows.
            if let Some(i) = candidates.iter().position(|th| th.3 >= term.started_at) {
                out.insert(candidates.remove(i).0.to_string(), term.pid);
            }
        }
    }
    out
}

/// Codex's newest `sessions/YYYY/MM/DD` folder, whose time is the latest new thread's.
pub(super) fn latest_day(sessions: &Path) -> Option<PathBuf> {
    let mut dir = sessions.to_path_buf();
    for _ in 0..3 {
        let mut kids: Vec<String> = std::fs::read_dir(&dir).ok()?.flatten().filter(|e| e.path().is_dir()).map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        kids.sort();
        dir = dir.join(kids.pop()?);
    }
    Some(dir)
}

/// What Codealong adds to every Codex terminal UI it opens (`terminal_ui`), which tells the CLI
/// inside the app run in a terminal from the app's own runs of it.
pub(crate) const TERMINAL_UI: &str = "-c check_for_update_on_startup=false";

/// A Codex command as a terminal can run it. Installing the Codex app doesn't put `codex` on the
/// PATH, so a plain `codex` becomes the CLI inside the app when the PATH has none; a wrapper or
/// alias of your own is left as you wrote it.
pub(crate) fn codex_cli(command: &str) -> String {
    if with_program(command, "codex", "").is_none() || crate::detect::on_user_path("codex") {
        return command.to_string();
    }
    let Some(cli) = crate::detect::codex_app_cli() else { return command.to_string() };
    let quoted = crate::util::sh_quote(&cli.to_string_lossy());
    with_program(command, "codex", &quoted).unwrap_or_else(|| command.to_string())
}

/// `command` with its program replaced, when the program (the first word past any `VAR=value`)
/// is `program`.
fn with_program(command: &str, program: &str, replacement: &str) -> Option<String> {
    let mut words: Vec<&str> = command.split_whitespace().collect();
    let assignment = |w: &str| w.split_once('=').is_some_and(|(k, _)| !k.is_empty() && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'));
    let at = words.iter().position(|w| !assignment(w)).filter(|&i| words[i] == program)?;
    words[at] = replacement;
    Some(words.join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tells_terminals_from_other_codex_processes() {
        let id = "01a0e375-0b36-77c1-8bf9-908e396b259e";
        assert_eq!(terminal_args("codex Reply with just the word pong."), Some((None, None)));
        assert_eq!(terminal_args("/opt/homebrew/bin/codex"), Some((None, None)));
        assert_eq!(terminal_args("codex -m gpt-5 -C /r/spur -i /a.png /b.png -- fix it"), Some((None, Some(PathBuf::from("/r/spur")))));
        assert_eq!(terminal_args(&format!("codex resume {id} carry on")), Some((Some(id.into()), None)));
        assert_eq!(terminal_args("codex resume --last"), Some((None, None)));
        assert_eq!(terminal_args("codex resume the work"), Some((None, None)));
        assert_eq!(terminal_args("codex exec fix it"), None);
        assert_eq!(terminal_args("/Users/me/.codex/packages/app-server-daemon/releases/0.157.1/bin/codex app-server --listen unix://"), None);
        assert_eq!(terminal_args("/Applications/Codex.app/Contents/Resources/codex app-server"), None);
        assert_eq!(terminal_args("/Applications/ChatGPT.app/Contents/Resources/codex-cli/CodexCLI.app/Contents/MacOS/codex -c features.code_mode_host=true app-server --analytics-default-enabled"), None);
        // The CLI inside the app, run in a terminal when the PATH has no `codex`.
        assert_eq!(terminal_args("/Applications/ChatGPT.app/Contents/Resources/codex-cli/CodexCLI.app/Contents/MacOS/codex -c check_for_update_on_startup=false -- Review"), Some((None, None)));
        // Anything else the app runs it for isn't one.
        assert_eq!(terminal_args("/Applications/ChatGPT.app/Contents/Resources/codex-cli/CodexCLI.app/Contents/MacOS/codex --version"), None);
    }

    #[test]
    fn puts_the_apps_cli_in_place_of_codex() {
        let cli = crate::util::sh_quote("/Applications/ChatGPT.app/Contents/Resources/codex-cli/bin/codex");
        assert_eq!(with_program("CODEX_HOME=~/.codex2 codex -c x=1", "codex", &cli).as_deref(), Some("CODEX_HOME=~/.codex2 '/Applications/ChatGPT.app/Contents/Resources/codex-cli/bin/codex' -c x=1"));
        assert_eq!(with_program("my-codex", "codex", &cli), None);
        assert_eq!(terminal_args("claude --resume x"), None);
    }

    #[test]
    fn pairs_terminals_with_their_threads() {
        let term = |pid: u32, cwd: &str, thread: Option<&str>, at: i64| Terminal { pid, cwd: PathBuf::from(cwd), thread: thread.map(String::from), started_at: at };
        let terms = vec![term(1, "/r/a", None, 100), term(2, "/r/a", None, 200), term(3, "/r/b", Some("app-thread"), 50), term(4, "/r/c", None, 500)];
        let threads = [
            // (id, folder, started, last activity, cli)
            ("old", "/r/a", 10, 20, true),
            ("first", "/r/a", 110, 150, true),
            ("second", "/r/a", 210, 250, true),
            ("app-thread", "/r/b", 5, 60, false),
            ("from-app", "/r/c", 600, 700, false),
            ("stale", "/r/c", 10, 20, true),
        ];
        let p = pair_terminals(&terms, &threads);
        assert_eq!(p.get("second"), Some(&2));
        assert_eq!(p.get("first"), Some(&1));
        assert_eq!(p.get("old"), None, "quiet since before either terminal started");
        assert_eq!(p.get("app-thread"), Some(&3), "resumed by id, even one the app made");
        assert_eq!(p.get("from-app"), None);
        assert_eq!(p.get("stale"), None);
    }

    #[test]
    fn swaps_only_the_program() {
        assert_eq!(with_program("codex", "codex", "'/A/codex'").as_deref(), Some("'/A/codex'"));
        assert_eq!(with_program("CODEX_HOME=~/.codex2 codex", "codex", "'/A/codex'").as_deref(), Some("CODEX_HOME=~/.codex2 '/A/codex'"));
        assert_eq!(with_program("codex2", "codex", "x"), None);
        assert_eq!(with_program("CODEX_HOME=codex other", "codex", "x"), None);
    }
}
