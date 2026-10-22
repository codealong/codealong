//! Closing the terminal tabs nobody has used in a while (File › Close stale terminals…): every
//! Ghostty, iTerm2 and Terminal tab, whatever runs in it, that nothing was typed or printed in
//! for the wait. Close idle sessions (idle.rs) goes by the board's sessions; this goes by tabs,
//! so a tab left at a shell, or one whose agent ended, goes too. `plan` says what would close and
//! why, and `close` closes the ones you kept ticked, looking at every tab again first.
//!
//! A tab is found in the process table: the processes on one terminal device, under a terminal
//! app whose tabs Codealong can close (`launch::terminal_kind`). Its shell is the topmost of them,
//! or the one under `login` where the terminal starts that (Ghostty and Terminal do, and iTerm2's
//! profiles by default): `login` runs as root with your real user id, so you may signal it, and
//! hanging it up isn't hanging up the shell. It was used last when its
//! device was: the kernel moves a terminal device's access time as what's typed there is read,
//! and its modification time as anything is printed, so a working agent or a server printing
//! its log keeps its tab fresh, and an agent at rest or a shell at its prompt doesn't. A tab
//! that's sat that long opened at least that long ago, so the wait is its age too; a terminal
//! that reopens its tabs as it starts (Ghostty restoring its windows) starts their shells anew.
//!
//! A stale tab is ticked to close when closing it loses nothing: its shell at its prompt, or
//! sessions Close idle sessions would close (`idle::verdict`). One with something else in its
//! foreground (a server, an editor, a tmux client), or a session that waits on you, is listed
//! unticked, saying what closing ends. Closing hangs up the shell, which hangs up what runs in
//! it (zsh, checked 2026-10-22), then closes the tab (`launch::close_session_terminals`); a
//! session's conversation stays and resumes like any other.

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
    /// Its shell, which closing it hangs up.
    pub shell: u32,
    /// Every process on its device.
    pub pids: Vec<u32>,
    /// The command line in its foreground, when that isn't its shell.
    pub foreground: Option<String>,
    pub cwd: String,
    pub opened: i64,
    /// When something was last typed or printed in it.
    pub used: i64,
}

/// What a stale tab shows.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Shows {
    /// Its shell, at its prompt.
    Shell,
    /// Sessions on the board.
    Session,
    /// Another program, in its foreground.
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
    /// The session it shows, the program in its foreground, or its folder.
    pub title: String,
    /// The sessions it shows, newest first: one OpenCode terminal can show several.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sessions: Vec<String>,
    /// Its shell, which closing it hangs up.
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
        let mut shown: Vec<&Session> = Vec::new();
        for s in live.iter().filter(|s| s.pid.is_some_and(|p| tab.pids.contains(&p))) {
            if !shown.iter().any(|o| o.key == s.key) {
                shown.push(s);
            }
        }
        shown.sort_by_key(|s| std::cmp::Reverse(s.last_activity));
        let place = crate::util::tilde(Path::new(&tab.cwd));
        let unused = format!("unused {}", span(unused));
        let (shows, title, ticked, why) = match (shown.first(), &tab.foreground) {
            (Some(newest), _) => {
                let verdicts: Vec<_> = shown.iter().map(|s| crate::idle::verdict(s, now, after_ms)).collect();
                match verdicts.iter().position(|v| v.is_err()) {
                    // One that waits on you holds the tab for the others too.
                    Some(i) => {
                        let held = verdicts[i].clone().unwrap_err();
                        let why = if shown.len() > 1 { format!("“{}” {held}", shown[i].title) } else { held };
                        (Shows::Session, newest.title.clone(), false, format!("{place} · {unused} · {why}"))
                    }
                    None => (Shows::Session, newest.title.clone(), true, format!("{place} · {}", verdicts[0].clone().unwrap_or_default())),
                }
            }
            (None, Some(command)) => (Shows::Program, short_command(command), false, format!("{place} · {unused} · closing ends it")),
            (None, None) => (Shows::Shell, place, true, format!("{unused} · a shell at its prompt")),
        };
        out.tabs.push(Stale {
            key: format!("{}:{}", tab.tty, tab.shell),
            terminal: tab.terminal.to_string(),
            shows,
            title,
            sessions: shown.iter().map(|s| s.key.clone()).collect(),
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

/// The tabs on this Mac now: one `ps`, and the kernel asked about each tab's shell and device.
pub fn tabs_now() -> Vec<Tab> {
    let listed = crate::procs::list(Duration::from_secs(10));
    let terminal = |pid: u32| crate::launch::terminal_kind(pid).filter(|t| t.closes_tabs()).map(|t| t.name());
    let mut out = Vec::new();
    for found in tabs_in(&listed.lines, terminal, crate::procs::foreground_group) {
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
/// app (`terminal`). Its shell is the topmost, or the child of a topmost `login`. `foreground`
/// gives the process group in front on a process's terminal.
fn tabs_in(lines: &[PsLine], terminal: impl Fn(u32) -> Option<&'static str>, foreground: impl Fn(u32) -> Option<u32>) -> Vec<Tab> {
    let mut by_tty: BTreeMap<&str, Vec<&PsLine>> = BTreeMap::new();
    for line in lines.iter().filter(|l| !matches!(l.tty.as_str(), "??" | "?" | "-" | "")) {
        by_tty.entry(line.tty.as_str()).or_default().push(line);
    }
    let mut out = Vec::new();
    for (tty, procs) in by_tty {
        let on = |pid: u32| procs.iter().any(|p| p.pid == pid);
        let Some((top, name)) = procs.iter().filter(|p| !on(p.ppid)).find_map(|p| Some((p, terminal(p.pid)?))) else { continue };
        let shell = match program(&top.command) == "login" {
            false => top.pid,
            true => match procs.iter().filter(|p| p.ppid == top.pid).map(|p| p.pid).min() {
                Some(pid) => pid,
                None => continue,
            },
        };
        let foreground = foreground(shell).filter(|&g| g != shell && g != top.pid).map(|g| match procs.iter().find(|p| p.pid == g) {
            Some(p) => p.command.clone(),
            None => "a program".to_string(),
        });
        out.push(Tab { tty: tty.to_string(), terminal: name, shell, pids: procs.iter().map(|p| p.pid).collect(), foreground, cwd: String::new(), opened: 0, used: 0 });
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
    let fresh = plan_now(board, after_ms);
    let chosen: Vec<&Stale> = keys.iter().filter_map(|k| fresh.tabs.iter().find(|t| &t.key == k)).collect();
    let targets: Vec<(u32, &Path)> = chosen.iter().map(|t| (t.pid, Path::new(t.cwd.as_str()))).collect();
    let ended: Vec<Result<(), String>> = crate::launch::close_session_terminals(&targets).into_iter().map(|r| r.map_err(crate::util::words)).collect();
    keys.iter()
        .map(|key| {
            let Some(i) = chosen.iter().position(|t| &t.key == key) else {
                return Outcome { key: key.clone(), title: String::new(), closed: false, message: "It was used since, or it's closed already.".into() };
            };
            let (closed, message) = match ended.get(i) {
                Some(Ok(())) => (true, "Closed.".to_string()),
                Some(Err(e)) => (false, e.clone()),
                None => (false, "Nothing was closed.".to_string()),
            };
            Outcome { key: key.clone(), title: chosen[i].title.clone(), closed, message }
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

    fn tab(tty: &str, shell: u32, pids: &[u32], foreground: Option<&str>, unused: i64) -> Tab {
        Tab {
            tty: tty.into(),
            terminal: "Ghostty",
            shell,
            pids: pids.to_vec(),
            foreground: foreground.map(String::from),
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
    fn finds_each_tab_under_a_terminal_and_its_shell() {
        let lines = [
            ps(10, 1, "??", "/Applications/Ghostty.app/Contents/MacOS/ghostty"),
            ps(11, 10, "ttys001", "/usr/bin/login -flp me /bin/bash --noprofile --norc -c exec -l /bin/zsh"),
            ps(12, 11, "ttys001", "-/bin/zsh"),
            ps(21, 10, "ttys002", "login -fp me"),
            ps(22, 21, "ttys002", "-zsh"),
            ps(23, 22, "ttys002", "/usr/local/bin/node /usr/local/bin/npm run dev"),
            // iTerm2 started this one's shell itself, with no `login` above it.
            ps(30, 1, "??", "iTermServer-3.7.3"),
            ps(31, 30, "ttys003", "/bin/zsh -l"),
            // A tmux pane: under tmux's server, which no terminal is above.
            ps(40, 1, "??", "tmux new-session -d"),
            ps(41, 40, "ttys004", "-zsh"),
        ];
        let terminal = |pid: u32| match pid {
            11 | 21 => Some("Ghostty"),
            31 => Some("iTerm2"),
            _ => None,
        };
        let foreground = |pid: u32| Some(if pid == 22 { 23 } else { pid });
        let tabs = tabs_in(&lines, terminal, foreground);
        let got: Vec<(&str, &str, u32, Vec<u32>, Option<&str>)> = tabs.iter().map(|t| (t.tty.as_str(), t.terminal, t.shell, t.pids.clone(), t.foreground.as_deref())).collect();
        assert_eq!(
            got,
            [
                ("ttys001", "Ghostty", 12, vec![11, 12], None),
                ("ttys002", "Ghostty", 22, vec![21, 22, 23], Some("/usr/local/bin/node /usr/local/bin/npm run dev")),
                ("ttys003", "iTerm2", 31, vec![31], None),
            ]
        );
    }

    #[test]
    fn ticks_only_what_closing_loses_nothing_of() {
        let day = DAY_MS;
        let mut needs = session("needs", 43, Status::Needs, 3 * day);
        needs.title = "Fix login".into();
        let b = board(vec![session("idle", 33, Status::Idle, 3 * day), needs, session("pair-old", 53, Status::Idle, 5 * day), session("pair-new", 54, Status::Idle, 3 * day)]);
        let tabs = [
            tab("ttys001", 12, &[11, 12], None, 4 * day),
            tab("ttys002", 22, &[21, 22, 23], Some("/usr/local/bin/node /usr/local/bin/npm run dev"), 3 * day),
            tab("ttys003", 32, &[31, 32, 33], Some("claude"), 3 * day),
            tab("ttys004", 42, &[41, 42, 43], Some("claude"), 3 * day),
            tab("ttys005", 52, &[51, 52, 53, 54], Some("opencode"), 3 * day),
            // Used an hour ago: not stale.
            tab("ttys006", 62, &[61, 62], None, 3_600_000),
        ];
        let p = plan(&b, &tabs, NOW, day);
        assert_eq!(p.fresh, 1);
        let got: Vec<(&str, Shows, &str, bool, &str)> = p.tabs.iter().map(|t| (t.key.as_str(), t.shows, t.title.as_str(), t.ticked, t.why.as_str())).collect();
        // The longest unused first.
        assert_eq!(got[0], ("ttys001:12", Shows::Shell, "/work/northwind", true, "unused 4 days · a shell at its prompt"));
        assert_eq!(got[1..].iter().filter(|g| g.3).map(|g| g.0).collect::<Vec<_>>(), ["ttys003:32", "ttys005:52"]);
        let row = |key: &str| p.tabs.iter().find(|t| t.key == key).unwrap();
        assert_eq!((row("ttys002:22").shows, row("ttys002:22").title.as_str(), row("ttys002:22").why.as_str()), (Shows::Program, "node /usr/local/bin/npm run dev", "/work/northwind · unused 3 days · closing ends it"));
        assert_eq!((row("ttys003:32").title.as_str(), row("ttys003:32").why.as_str()), ("Session idle", "/work/northwind · idle 3 days · last reply asks nothing"));
        assert_eq!((row("ttys004:42").title.as_str(), row("ttys004:42").ticked, row("ttys004:42").why.as_str()), ("Fix login", false, "/work/northwind · unused 3 days · needs you"));
        // A terminal of two sessions is named by the newer, and closes both.
        assert_eq!((row("ttys005:52").title.as_str(), row("ttys005:52").sessions.clone()), ("Session pair-new", vec!["pair-new".to_string(), "pair-old".to_string()]));
    }

    #[test]
    fn a_session_that_waits_holds_its_tab_for_the_others() {
        let day = DAY_MS;
        let mut needs = session("needs", 43, Status::Needs, 3 * day);
        needs.title = "Fix login".into();
        let b = board(vec![session("idle", 42, Status::Idle, 4 * day), needs]);
        let p = plan(&b, &[tab("ttys004", 41, &[40, 41, 42, 43], Some("opencode"), 3 * day)], NOW, day);
        assert_eq!((p.tabs[0].ticked, p.tabs[0].why.as_str()), (false, "/work/northwind · unused 3 days · “Fix login” needs you"));
    }

    /// Opens a Ghostty window running a long `sleep` and a plain window in front of it (the tab
    /// you're in stays open), finds the first among the tabs, closes it as the dialog does, and
    /// checks the tab and the `sleep` are gone. It opens windows, so it's opt in: `cargo test
    /// ghostty_closes_a_stale_tab -- --ignored`.
    #[test]
    #[ignore]
    fn ghostty_closes_a_stale_tab() {
        use crate::launch::{terminal_app, terminal_open, Placement};
        let ghostty = terminal_app("ghostty").unwrap();
        let ids = || ghostty.tabs().unwrap().into_iter().map(|t| t.id).collect::<Vec<_>>();
        let before = ids();
        let secs = 7000 + std::process::id() % 1000;
        let place = Placement { new_terminal: crate::config::NewTerminal::Window, near: &[], behind: true, label: None };
        let dir = std::env::temp_dir();
        terminal_open("ghostty", place, &dir, Some(&format!("sleep {secs}\n"))).unwrap();
        let mut found = None;
        for _ in 0..100 {
            found = tabs_now().into_iter().find(|t| t.foreground.as_deref() == Some(format!("sleep {secs}").as_str()));
            if found.is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let tab = found.expect("the sleep's tab never showed");
        let opened: Vec<String> = ids().into_iter().filter(|id| !before.contains(id)).collect();
        let ttys: Vec<String> = tabs_now().into_iter().map(|t| t.tty).collect();
        terminal_open("ghostty", place, &dir, None).unwrap();
        let mut front = None;
        for _ in 0..50 {
            front = tabs_now().into_iter().find(|t| !ttys.contains(&t.tty));
            if front.is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let sleeper = crate::procs::foreground_group(tab.shell).unwrap();
        let out = close(&Board::default(), &[format!("{}:{}", tab.tty, tab.shell)], 0);
        std::thread::sleep(Duration::from_millis(500));
        let left: Vec<String> = ids().into_iter().filter(|id| opened.contains(id)).collect();
        let alive = crate::procs::pid_alive(sleeper);
        // Hanging up a shell closes its Ghostty tab.
        for shell in front.iter().map(|t| t.shell).chain((!left.is_empty()).then_some(tab.shell)) {
            unsafe { libc::kill(shell as libc::pid_t, libc::SIGHUP) };
        }
        assert_eq!(opened.len(), 1, "one terminal opened");
        assert!(front.is_some(), "the window in front never showed");
        assert!(out[0].closed, "{}", out[0].message);
        assert!(left.is_empty(), "the tab is still open");
        assert!(!alive, "the sleep still runs");
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
