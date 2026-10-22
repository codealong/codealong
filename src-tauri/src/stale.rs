//! Closing the terminal tabs nobody has used in a while (File › Close stale terminals…): every
//! Ghostty, iTerm2 and Terminal tab, whatever runs in it, that nothing was typed or printed in
//! for the wait. Close idle sessions (idle.rs) goes by the board's sessions; this goes by tabs,
//! so a tab left at a shell, or one whose agent ended, goes too. `plan` says what would close and
//! why, and `close` closes the ones you picked, looking at every tab again first.
//!
//! A tab is found in the process table: the processes on one terminal device, under a terminal
//! app Codealong closes tabs in (`takes_tabs_of`; tmux's panes aren't tabs). Its shell is the
//! topmost of them, or the one under `login` where the terminal starts that (Ghostty and Terminal
//! do, and iTerm2's profiles by default): `login` runs as root with your real user id, so you may
//! signal it, but it isn't the shell. A shell is a program `/etc/shells` or your login shell
//! names; anything else the terminal started, or a shell `exec`ed, runs in its place. A tab was
//! used last when its device was: the kernel moves a terminal device's access time as what's
//! typed there is read, and its modification time as anything is printed, so a working agent or
//! a server printing its log keeps its tab fresh, and an agent at rest or a shell at its prompt
//! doesn't. A tab that's sat that long opened at least that long ago, so the wait is its age
//! too; a terminal that reopens its tabs as it starts (Ghostty restoring its windows) starts
//! their shells anew.
//!
//! A stale tab is ticked to close when closing it loses nothing: its shell at its prompt with no
//! jobs, or sessions Close idle sessions would close (`idle::verdict`) and nothing else. One that
//! runs anything else, in front or behind (a server, an editor stopped with ^Z, a tmux client), or
//! a session that waits on you, is listed unticked, saying what closing ends. Closing hangs up
//! what the shell runs, leaving it at its prompt, then has the terminal close the tab
//! (`launch::close_terminals`), as Close idle sessions does a session's: a shell hung up instead
//! would leave Terminal a window whose process completed. A session's conversation stays and
//! resumes like any other.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::idle::span;
use crate::model::{Board, Session, Surface};
use crate::procs::PsLine;

/// A terminal tab, as the process table has it.
#[derive(Clone, Debug, PartialEq)]
pub struct Tab {
    /// Its terminal device as `ps` names it: `ttys004`.
    pub tty: String,
    /// The terminal it's in: Ghostty, iTerm2, Terminal.
    pub terminal: &'static str,
    /// Its shell: the process its terminal started for it, under `login` where there's one.
    pub shell: u32,
    /// What closing it hangs up, each with its command line: the jobs its shell runs, the one in
    /// front first, or what runs in place of a shell.
    pub runs: Vec<(u32, String)>,
    /// Every process on its device, with its parent.
    pub procs: Vec<(u32, u32)>,
    pub cwd: String,
    pub opened: i64,
    /// When something was last typed or printed in it.
    pub used: i64,
}

impl Tab {
    /// Whether `pid` is `of` or runs under it in this tab.
    fn under(&self, pid: u32, of: u32) -> bool {
        let mut at = pid;
        for _ in 0..64 {
            if at == of {
                return true;
            }
            match self.procs.iter().find(|p| p.0 == at) {
                Some(&(_, parent)) => at = parent,
                None => return false,
            }
        }
        false
    }
}

/// What a stale tab shows.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Shows {
    /// Its shell, at its prompt.
    Shell,
    /// Sessions on the board.
    Session,
    /// Another program.
    Program,
}

/// A tab that sat unused for the wait.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Stale {
    /// What it's closed by: its device and its shell (`ttys004:4821`), so a tab that took the
    /// device since isn't closed in its place.
    pub key: String,
    pub terminal: String,
    pub shows: Shows,
    /// The session it shows, the program it runs, or its folder.
    pub title: String,
    /// The sessions it shows, newest first: one OpenCode terminal can show several.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sessions: Vec<String>,
    /// Its shell.
    pub pid: u32,
    pub cwd: String,
    pub opened: i64,
    pub used: i64,
    /// Ticked to close: closing it loses nothing.
    pub ticked: bool,
    /// Why it may close, or what closing it ends.
    pub why: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub after_ms: i64,
    /// The stale tabs, the longest unused first.
    pub tabs: Vec<Stale>,
    /// How many tabs were used since.
    pub fresh: usize,
}

/// What closing one tab did.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    pub key: String,
    pub title: String,
    pub closed: bool,
    pub message: String,
}

/// What would close: every tab unused for `after_ms`, ticked where closing it loses nothing.
pub fn plan(board: &Board, tabs: &[Tab], now: i64, after_ms: i64) -> Plan {
    let live: Vec<&Session> = board.lanes.iter().flat_map(|l| &l.sessions).filter(|s| s.live && s.visitor.is_none() && s.surface == Surface::Cli && !s.hosted).collect();
    let mut out = Plan { after_ms, ..Plan::default() };
    for tab in tabs {
        let unused = now - tab.used;
        if unused < after_ms {
            out.fresh += 1;
            continue;
        }
        let mut shown: Vec<(&Session, u32)> = Vec::new();
        for s in &live {
            if let Some(pid) = s.pid.filter(|p| tab.procs.iter().any(|q| q.0 == *p)) {
                if !shown.iter().any(|(o, _)| o.key == s.key) {
                    shown.push((s, pid));
                }
            }
        }
        shown.sort_by_key(|(s, _)| std::cmp::Reverse(s.last_activity));
        let place = crate::util::tilde(Path::new(&tab.cwd));
        let unused = format!("unused {}", span(unused));
        // What closing ends besides its sessions: what runs that's neither a session's process
        // nor one a session runs under (a wrapper that started it).
        let other = tab.runs.iter().find(|(run, _)| !shown.iter().any(|(_, pid)| tab.under(*pid, *run)));
        let (shows, title, ticked, why) = match (shown.first(), tab.runs.first()) {
            (Some((newest, _)), _) => {
                let verdicts: Vec<_> = shown.iter().map(|(s, _)| crate::idle::verdict(s, now, after_ms)).collect();
                let title = newest.title.clone();
                match (verdicts.iter().position(|v| v.is_err()), other) {
                    // One that waits on you holds the tab for the others too.
                    (Some(i), _) => {
                        let held = verdicts[i].clone().unwrap_err();
                        let why = if shown.len() > 1 { format!("“{}” {held}", shown[i].0.title) } else { held };
                        (Shows::Session, title, false, format!("{place} · {unused} · {why}"))
                    }
                    (None, Some((_, command))) => (Shows::Session, title, false, format!("{place} · {unused} · also runs `{}`", short_command(command))),
                    (None, None) => (Shows::Session, title, true, format!("{place} · {}", verdicts[0].clone().unwrap_or_default())),
                }
            }
            (None, Some((_, command))) => {
                let more = match tab.runs.len() {
                    1 => String::new(),
                    n => format!(" and {} more", n - 1),
                };
                (Shows::Program, short_command(command), false, format!("{place} · {unused} · closing ends it{more}"))
            }
            (None, None) => (Shows::Shell, place, true, format!("{unused} · a shell at its prompt")),
        };
        out.tabs.push(Stale {
            key: key_of(tab),
            terminal: tab.terminal.to_string(),
            shows,
            title,
            sessions: shown.iter().map(|(s, _)| s.key.clone()).collect(),
            pid: tab.shell,
            cwd: tab.cwd.clone(),
            opened: tab.opened,
            used: tab.used,
            ticked,
            why,
        });
    }
    out.tabs.sort_by_key(|t| t.used);
    out
}

fn key_of(tab: &Tab) -> String {
    format!("{}:{}", tab.tty, tab.shell)
}

/// A command line as a row says it: its program by name, not path (`node …/npm run dev` as
/// `npm run dev` would need the script read, so `node` it is), cut to a line.
fn short_command(command: &str) -> String {
    let rest = command.trim().split_once(' ').map_or("", |(_, rest)| rest);
    crate::util::truncate(format!("{} {rest}", program(command)).trim(), 60)
}

/// A command line's program by its file name: `/usr/bin/login -flp me` is `login`, and a login
/// shell's `-/bin/zsh` is `zsh`.
fn program(command: &str) -> &str {
    let first = command.trim().split(' ').next().unwrap_or_default();
    first.rsplit('/').next().unwrap_or(first).trim_start_matches('-')
}

/// Whether Close stale terminals lists a terminal's tabs: an app sessions open in whose tabs
/// Codealong closes. tmux closes its panes too, but they aren't tabs, and a pane of yours left
/// running is left on purpose.
fn takes_tabs_of(t: &dyn crate::launch::Terminal) -> bool {
    t.closes_tabs() && t.profile().is_some()
}

/// The tabs on this Mac now: one `ps`, and the kernel asked about each tab's shell and device.
pub fn tabs_now() -> Vec<Tab> {
    let listed = crate::procs::list(Duration::from_secs(10));
    let terminal = |pid: u32| crate::launch::terminal_kind(pid).filter(|t| takes_tabs_of(*t)).map(|t| t.name());
    let shells: Vec<String> = std::fs::read_to_string("/etc/shells")
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|l| l.starts_with('/'))
        .map(String::from)
        .chain([crate::launch::login_shell()])
        .map(|path| program(&path).to_string())
        .collect();
    let mut out = Vec::new();
    for found in tabs_in(&listed.lines, terminal, |name| shells.iter().any(|s| s == name), crate::procs::foreground_group) {
        let device = Path::new("/dev").join(&found.tty);
        let opened = crate::procs::proc_start_ms(found.shell).unwrap_or(listed.at);
        out.push(Tab {
            cwd: crate::procs::cwd_of(found.shell).map(|p| p.to_string_lossy().into_owned()).unwrap_or_default(),
            used: last_used(&device).unwrap_or(opened).max(opened),
            opened,
            ..found
        });
    }
    out
}

/// When a terminal device was last read from or written to.
fn last_used(device: &Path) -> Option<i64> {
    use std::os::unix::fs::MetadataExt;
    let m = std::fs::metadata(device).ok()?;
    Some((m.atime() * 1000 + m.atime_nsec() / 1_000_000).max(m.mtime() * 1000 + m.mtime_nsec() / 1_000_000))
}

/// The tabs in a process listing, without the kernel's times and folders: the processes on
/// each terminal device, whose topmost (the one whose parent isn't on it) runs under a terminal
/// app (`terminal`). Its shell is the topmost, or the child of a topmost `login`; what it runs is
/// its children when `is_shell` takes its program for a shell, the one `foreground` says is in
/// front first, else itself.
fn tabs_in(lines: &[PsLine], terminal: impl Fn(u32) -> Option<&'static str>, is_shell: impl Fn(&str) -> bool, foreground: impl Fn(u32) -> Option<u32>) -> Vec<Tab> {
    let mut by_tty: BTreeMap<&str, Vec<&PsLine>> = BTreeMap::new();
    for line in lines.iter().filter(|l| !matches!(l.tty.as_str(), "??" | "?" | "-" | "")) {
        by_tty.entry(line.tty.as_str()).or_default().push(line);
    }
    let mut out = Vec::new();
    for (tty, procs) in by_tty {
        let on = |pid: u32| procs.iter().any(|p| p.pid == pid);
        let Some((top, name)) = procs.iter().filter(|p| !on(p.ppid)).find_map(|p| Some((p, terminal(p.pid)?))) else { continue };
        let shell = match program(&top.command) == "login" {
            false => top,
            true => match procs.iter().filter(|p| p.ppid == top.pid).min_by_key(|p| p.pid) {
                Some(p) => p,
                None => continue,
            },
        };
        let runs = match is_shell(program(&shell.command)) {
            true => {
                let mut jobs: Vec<(u32, String)> = procs.iter().filter(|p| p.ppid == shell.pid).map(|p| (p.pid, p.command.clone())).collect();
                let front = foreground(shell.pid);
                jobs.sort_by_key(|(pid, _)| Some(*pid) != front);
                jobs
            }
            false => vec![(shell.pid, shell.command.clone())],
        };
        out.push(Tab { tty: tty.to_string(), terminal: name, shell: shell.pid, runs, procs: procs.iter().map(|p| (p.pid, p.ppid)).collect(), cwd: String::new(), opened: 0, used: 0 });
    }
    out
}

/// The plan for the tabs open now.
pub fn plan_now(board: &Board, after_ms: i64) -> Plan {
    plan(board, &tabs_now(), crate::util::now_ms(), after_ms)
}

/// Closes the tabs of `keys` that are still stale, looked at again now, whether they were
/// ticked or not: you picked them. One outcome a key, in order.
pub fn close(board: &Board, keys: &[String], after_ms: i64) -> Vec<Outcome> {
    let tabs = tabs_now();
    let fresh = plan(board, &tabs, crate::util::now_ms(), after_ms);
    let chosen: Vec<(&Stale, &Tab)> = keys.iter().filter_map(|k| Some((fresh.tabs.iter().find(|t| &t.key == k)?, tabs.iter().find(|t| &key_of(t) == k)?))).collect();
    let closing: Vec<crate::launch::Closing> = chosen.iter().map(|(_, tab)| crate::launch::Closing { pid: tab.shell, cwd: Path::new(&tab.cwd), hang_up: tab.runs.iter().map(|r| r.0).collect() }).collect();
    let ended: Vec<Result<(), String>> = crate::launch::close_terminals(&closing).into_iter().map(|r| r.map_err(crate::util::words)).collect();
    keys.iter()
        .map(|key| {
            let Some(i) = chosen.iter().position(|(t, _)| &t.key == key) else {
                return Outcome { key: key.clone(), title: String::new(), closed: false, message: "It was used since, or it's closed already.".into() };
            };
            let (closed, message) = match ended.get(i) {
                Some(Ok(())) => (true, "Closed.".to_string()),
                Some(Err(e)) => (false, e.clone()),
                None => (false, "Nothing was closed.".to_string()),
            };
            Outcome { key: key.clone(), title: chosen[i].0.title.clone(), closed, message }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Status;
    use crate::util::DAY_MS;

    const NOW: i64 = 100 * DAY_MS;

    fn ps(pid: u32, ppid: u32, tty: &str, command: &str) -> PsLine {
        PsLine { pid, ppid, tty: tty.into(), cpu: None, command: command.into() }
    }

    /// A Ghostty tab: `login` (its number one less than the shell's) above the shell, and the
    /// shell's processes, each with its parent; `runs` are what closing it hangs up.
    fn tab(tty: &str, shell: u32, procs: &[(u32, u32)], runs: &[(u32, &str)], unused: i64) -> Tab {
        Tab {
            tty: tty.into(),
            terminal: "Ghostty",
            shell,
            runs: runs.iter().map(|&(pid, command)| (pid, command.to_string())).collect(),
            procs: [(shell - 1, 1), (shell, shell - 1)].into_iter().chain(procs.iter().copied()).collect(),
            cwd: "/work/northwind".into(),
            opened: NOW - unused - DAY_MS,
            used: NOW - unused,
        }
    }

    fn session(key: &str, pid: u32, status: Status, quiet: i64) -> Session {
        Session { status, pid: Some(pid), title: format!("Session {key}"), cwd: "/work/northwind".into(), reply_tail: Some("Pushed it.".into()), last_activity: NOW - quiet, ..crate::fixtures::session(key) }
    }

    fn board(sessions: Vec<Session>) -> Board {
        let mut b = Board::default();
        b.lanes = vec![crate::fixtures::lane("/work/northwind", sessions)];
        b
    }

    #[test]
    fn finds_each_tab_under_a_terminal_its_shell_and_what_it_runs() {
        let lines = [
            ps(10, 1, "??", "/Applications/Ghostty.app/Contents/MacOS/ghostty"),
            ps(11, 10, "ttys001", "/usr/bin/login -flp me /bin/bash --noprofile --norc -c exec -l /bin/zsh"),
            ps(12, 11, "ttys001", "-/bin/zsh"),
            // A job behind and one in front, which comes first.
            ps(21, 10, "ttys002", "login -fp me"),
            ps(22, 21, "ttys002", "-zsh"),
            ps(24, 22, "ttys002", "sleep 100"),
            ps(25, 22, "ttys002", "/usr/local/bin/node /usr/local/bin/npm run dev"),
            ps(26, 25, "ttys002", "node server.js"),
            // iTerm2 started this one's shell itself, with no `login` above it.
            ps(30, 1, "??", "iTermServer-3.7.3"),
            ps(31, 30, "ttys003", "/bin/zsh -l"),
            // A tmux pane: under tmux's server, which isn't a terminal that has tabs.
            ps(40, 1, "??", "tmux new-session -d"),
            ps(41, 40, "ttys004", "-zsh"),
            // A shell that ran `exec vim`: vim is in its place.
            ps(51, 10, "ttys005", "login -fp me"),
            ps(52, 51, "ttys005", "vim notes.md"),
        ];
        let terminal = |pid: u32| match pid {
            11 | 21 | 51 => Some("Ghostty"),
            31 => Some("iTerm2"),
            _ => None,
        };
        let is_shell = |name: &str| ["zsh", "bash", "sh"].contains(&name);
        let foreground = |pid: u32| Some(if pid == 22 { 25 } else { pid });
        let tabs = tabs_in(&lines, terminal, is_shell, foreground);
        let got: Vec<(&str, &str, u32, Vec<u32>)> = tabs.iter().map(|t| (t.tty.as_str(), t.terminal, t.shell, t.runs.iter().map(|r| r.0).collect())).collect();
        assert_eq!(got, [("ttys001", "Ghostty", 12, vec![]), ("ttys002", "Ghostty", 22, vec![25, 24]), ("ttys003", "iTerm2", 31, vec![]), ("ttys005", "Ghostty", 52, vec![52])]);
        assert_eq!(tabs[1].procs, [(21, 10), (22, 21), (24, 22), (25, 22), (26, 25)]);
    }

    #[test]
    fn lists_the_tabs_of_terminal_apps_and_not_tmux_panes() {
        let taken: Vec<&str> = crate::launch::terminals().iter().filter(|t| takes_tabs_of(**t)).map(|t| t.name()).collect();
        match crate::os::current() {
            crate::os::Os::MacOs => assert_eq!(taken, ["Ghostty", "iTerm2", "Terminal"]),
            crate::os::Os::Linux => assert!(taken.is_empty(), "{taken:?}"),
        }
    }

    #[test]
    fn ticks_only_what_closing_loses_nothing_of() {
        let day = DAY_MS;
        let mut needs = session("needs", 43, Status::Needs, 3 * day);
        needs.title = "Fix login".into();
        let b = board(vec![
            session("idle", 33, Status::Idle, 3 * day),
            needs,
            session("pair-old", 53, Status::Idle, 5 * day),
            session("pair-new", 53, Status::Idle, 3 * day),
            session("wrapped", 73, Status::Idle, 3 * day),
            session("beside", 83, Status::Idle, 3 * day),
        ]);
        let tabs = [
            tab("ttys001", 12, &[], &[], 4 * day),
            tab("ttys002", 22, &[(23, 22)], &[(23, "/usr/local/bin/node /usr/local/bin/npm run dev")], 3 * day),
            // A session's own processes (caffeinate) are its.
            tab("ttys003", 32, &[(33, 32), (34, 33)], &[(33, "claude")], 3 * day),
            tab("ttys004", 42, &[(43, 42)], &[(43, "claude")], 3 * day),
            // One OpenCode terminal showing two sessions.
            tab("ttys005", 52, &[(53, 52)], &[(53, "opencode")], 3 * day),
            // Used an hour ago: not stale.
            tab("ttys006", 62, &[], &[], 3_600_000),
            // A session under a wrapper that started it.
            tab("ttys007", 72, &[(71, 70), (73, 71)], &[(71, "node /usr/local/bin/codex")], 3 * day),
            // A session with an editor stopped beside it.
            tab("ttys008", 82, &[(83, 82), (84, 82)], &[(84, "vim notes.md"), (83, "claude")], 3 * day),
            // Two jobs, and no session.
            tab("ttys009", 92, &[(93, 92), (94, 92)], &[(93, "python3 -m http.server"), (94, "sleep 100")], 3 * day),
        ];
        let p = plan(&b, &tabs, NOW, day);
        assert_eq!(p.fresh, 1);
        let row = |key: &str| {
            let t = p.tabs.iter().find(|t| t.key == key).unwrap();
            (t.shows, t.title.as_str(), t.ticked, t.why.as_str())
        };
        // The longest unused first.
        assert_eq!(p.tabs[0].key, "ttys001:12");
        assert_eq!(row("ttys001:12"), (Shows::Shell, "/work/northwind", true, "unused 4 days · a shell at its prompt"));
        assert_eq!(row("ttys002:22"), (Shows::Program, "node /usr/local/bin/npm run dev", false, "/work/northwind · unused 3 days · closing ends it"));
        assert_eq!(row("ttys003:32"), (Shows::Session, "Session idle", true, "/work/northwind · idle 3 days · last reply asks nothing"));
        assert_eq!(row("ttys004:42"), (Shows::Session, "Fix login", false, "/work/northwind · unused 3 days · needs you"));
        // A terminal of two sessions is named by the newer, and closes both.
        assert_eq!(row("ttys005:52"), (Shows::Session, "Session pair-new", true, "/work/northwind · idle 3 days · last reply asks nothing"));
        assert_eq!(p.tabs.iter().find(|t| t.key == "ttys005:52").unwrap().sessions, ["pair-new", "pair-old"]);
        assert!(row("ttys007:72").2, "a wrapper's session closes");
        assert_eq!(row("ttys008:82"), (Shows::Session, "Session beside", false, "/work/northwind · unused 3 days · also runs `vim notes.md`"));
        assert_eq!(row("ttys009:92"), (Shows::Program, "python3 -m http.server", false, "/work/northwind · unused 3 days · closing ends it and 1 more"));
    }

    #[test]
    fn a_session_that_waits_holds_its_tab_for_the_others() {
        let day = DAY_MS;
        let mut needs = session("needs", 43, Status::Needs, 3 * day);
        needs.title = "Fix login".into();
        let b = board(vec![session("idle", 43, Status::Idle, 4 * day), needs]);
        let p = plan(&b, &[tab("ttys004", 42, &[(43, 42)], &[(43, "opencode")], 3 * day)], NOW, day);
        assert_eq!((p.tabs[0].ticked, p.tabs[0].why.as_str()), (false, "/work/northwind · unused 3 days · “Fix login” needs you"));
    }

    /// Opens a Ghostty window running a long `sleep` and a plain window in front of it (the tab
    /// you're in stays open), each in a folder of its own it's found by, closes the first as the
    /// dialog does (the `sleep` hung up, then the tab with its shell at its prompt), and checks the
    /// `sleep` and the tab's shell are gone: a closed tab ends its shell. Ghostty 1.3.1 goes on
    /// listing a terminal a script closed, in a window it ordered out, so its lists can't say. It
    /// opens windows, so it's opt in: `cargo test ghostty_closes_a_stale_tab -- --ignored`.
    #[test]
    #[ignore]
    fn ghostty_closes_a_stale_tab() {
        use crate::launch::{terminal_open, Placement};
        let root = std::env::temp_dir().join(format!("codealong-stale-{}", std::process::id()));
        let (behind, front) = (root.join("behind"), root.join("front"));
        std::fs::create_dir_all(&behind).unwrap();
        std::fs::create_dir_all(&front).unwrap();
        let real = |dir: &Path| dir.canonicalize().unwrap().to_string_lossy().into_owned();
        let (behind_at, front_at) = (real(&behind), real(&front));
        let find = |at: &str, ready: &dyn Fn(&Tab) -> bool| {
            for _ in 0..100 {
                if let Some(t) = tabs_now().into_iter().find(|t| t.cwd == at && ready(t)) {
                    return Some(t);
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            None
        };
        let secs = 7000 + std::process::id() % 1000;
        let place = Placement { new_terminal: crate::config::NewTerminal::Window, near: &[], behind: true, label: None };
        terminal_open("ghostty", place, &behind, Some(&format!("sleep {secs}\n"))).unwrap();
        let tab = find(&behind_at, &|t| t.runs.first().is_some_and(|r| r.1 == format!("sleep {secs}")));
        terminal_open("ghostty", place, &front, None).unwrap();
        let in_front = find(&front_at, &|_| true);
        let out = tab.as_ref().map(|t| close(&Board::default(), &[key_of(t)], 0));
        std::thread::sleep(Duration::from_secs(1));
        let alive = |pid: u32, at: &str| crate::procs::pid_alive(pid) && crate::procs::cwd_of(pid).is_some_and(|c| c.to_string_lossy() == at);
        let shell_runs = tab.as_ref().is_some_and(|t| alive(t.shell, &behind_at));
        let sleep_runs = tab.as_ref().is_some_and(|t| crate::procs::pid_alive(t.runs[0].0));
        // Only the shells in the test's own folders are hung up, which closes their tabs.
        for t in tabs_now().into_iter().filter(|t| t.cwd == behind_at || t.cwd == front_at) {
            unsafe { libc::kill(t.shell as libc::pid_t, libc::SIGHUP) };
        }
        let _ = std::fs::remove_dir_all(&root);
        assert!(tab.is_some(), "the sleep's tab never showed");
        assert!(in_front.is_some(), "the window in front never showed");
        let out = out.unwrap();
        assert!(out[0].closed, "{}", out[0].message);
        assert!(!sleep_runs, "the sleep still runs");
        assert!(!shell_runs, "the tab is still open: its shell runs");
    }

    /// The stale tabs on this Mac now, closing nothing:
    /// `cargo test this_macs_stale_terminals -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn this_macs_stale_terminals() {
        let p = plan_now(&crate::engine::light_snapshot(), 0);
        for t in &p.tabs {
            println!("{:<8} {:<16} {:<5} {:<40} {}", t.terminal, t.key, if t.ticked { "close" } else { "keep" }, t.title.chars().take(40).collect::<String>(), t.why);
        }
    }
}
