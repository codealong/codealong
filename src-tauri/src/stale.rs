//! Closing the terminal tabs nobody has used in a while (File › Close stale terminals…): every
//! Ghostty, iTerm2, Terminal and Rex tab, whatever runs in it, that nothing was typed or printed in
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
//! doesn't. But a redraw touches every tab at once: opening the lid or changing screens resizes
//! every window, and each shell redraws its prompt and reads again, so on a laptop every tab
//! looked used each morning. The engine looks at the devices every tick (`Uses`), and a move
//! that many tabs quiet a while made within seconds of one another is a redraw, not use: what
//! it saw used before stands. A tab that's sat that long opened at least that long ago, so the
//! wait is its age too, and one no look saw used since a redraw counts from its opening; a
//! terminal that reopens its tabs as it starts (Ghostty restoring its windows) starts their
//! shells anew.
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
    /// The terminal it's in: Ghostty, iTerm2, Terminal, Rex.
    pub terminal: &'static str,
    /// Its shell: the process its terminal started for it, under `login` where there's one.
    pub shell: u32,
    /// What closing it hangs up, each with its command line: the jobs its shell runs, the one in
    /// front first, or what runs in place of a shell.
    pub runs: Vec<(u32, String)>,
    /// Every process on its device, with its parent and command line.
    pub procs: Vec<(u32, u32, String)>,
    pub cwd: String,
    pub opened: i64,
    /// When something was last typed or printed in it, a redraw of every tab not counted.
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
                Some(p) => at = p.1,
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
        // What closing ends besides its sessions: anything it runs, or that runs under that,
        // that's neither a session's, nor under one, nor one a session runs under (a wrapper or a
        // shell that started it), so an editor beside a session under a nested shell counts.
        let other = tab.procs.iter().find(|(pid, _, _)| tab.runs.iter().any(|(run, _)| tab.under(*pid, *run)) && !shown.iter().any(|(_, s)| tab.under(*pid, *s) || tab.under(*s, *pid)));
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
                    (None, Some((_, _, command))) => (Shows::Session, title, false, format!("{place} · {unused} · also runs `{}`", short_command(command))),
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

/// Whether any terminal here has tabs to list, so the engine's looks (`Uses`) are worth taking.
pub fn lists_tabs() -> bool {
    crate::launch::terminals().iter().any(|t| takes_tabs_of(*t))
}

/// How many tabs moving together make a redraw, how close together, and how long each must
/// have sat still before. A redraw moves every tab within a few seconds; you type in one tab at a
/// time, and the working agents that print together printed a moment before too.
const REDRAW_TABS: usize = 3;
const REDRAW_WITHIN_MS: i64 = 10_000;
const QUIET_MS: i64 = 10 * 60_000;
/// How often the engine writes what it saw when nothing was redrawn: no more than a look
/// might change, and no less than `QUIET_MS`, so what moved since the file was written and
/// moves on after a restart counts as moving on, not as sitting still.
const SAVE_EVERY_MS: i64 = 5 * 60_000;

/// When each terminal device was last used, as the engine's looks saw it (`on_look`, each
/// tick's process listing): its time but for a redraw, which leaves the use before it standing.
/// The app's engine keeps it, and the plan takes a look of its own over the engine's
/// (`engine::Handle::terminal_use`); written to terminal-use.json, which outlives a restart
/// and is all `--snapshot` has. Only a cache: one that doesn't read starts over.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Uses {
    /// By device (`ttys004`). A device a new tab takes keeps its record, which its tab's
    /// opening comes after.
    seen: BTreeMap<String, Seen>,
    #[serde(skip)]
    saved_at: i64,
    /// Something changed since the file was written.
    #[serde(skip)]
    unsaved: bool,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
struct Seen {
    /// The device's time at the last look.
    at: i64,
    /// When it was used last: `at`, or `still_before` once its move is known for a redraw's.
    /// 0 when no look saw it used, so its tab counts from its opening.
    used: i64,
    /// When its last move came after it sat still for `QUIET_MS`, its use before that move,
    /// which stands if the move turns out to be a redraw's. A look can come partway through a
    /// redraw, so whether it was is asked again at every look until it's known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    still_before: Option<i64>,
}

impl Uses {
    fn path() -> std::path::PathBuf {
        crate::config::support_dir().join("terminal-use.json")
    }

    pub fn load() -> Uses {
        crate::store::read_or_default(&Self::path())
    }

    pub fn save(&mut self, now: i64) {
        self.written(now);
        let _ = crate::store::save(&Self::path(), self);
    }

    fn written(&mut self, now: i64) {
        self.saved_at = now;
        self.unsaved = false;
    }

    /// A look at every device's time `now`: whether to write the file now, because a redraw
    /// was found, or because something changed and the file is older than `SAVE_EVERY_MS`.
    ///
    /// A device's move is a redraw's when `REDRAW_TABS` or more devices that sat still for
    /// `QUIET_MS` before their last move made it within `REDRAW_WITHIN_MS` of one another,
    /// counting those already known for a redraw's, so one whose tab has closed since still
    /// counts. A device no look saw before (Codealong just started, or its tab is new) has only
    /// its time, which counts as such a move when it's `QUIET_MS` old, with no use before it.
    pub fn on_look(&mut self, now: i64, devices: &[(String, i64)]) -> bool {
        let mut seen: BTreeMap<String, Seen> = devices
            .iter()
            .map(|(tty, at)| {
                let next = match self.seen.get(tty) {
                    Some(s) if *at <= s.at => *s,
                    Some(s) => Seen { at: *at, used: *at, still_before: (*at - s.at >= QUIET_MS).then_some(s.used) },
                    None => Seen { at: *at, used: *at, still_before: (now - *at >= QUIET_MS).then_some(0) },
                };
                (tty.clone(), next)
            })
            .collect();
        let moves: Vec<(&str, i64)> = seen.iter().filter(|(_, s)| s.still_before.is_some()).map(|(tty, s)| (tty.as_str(), s.at)).collect();
        let redrawn: Vec<String> = moved_together(moves).into_iter().map(String::from).collect();
        let mut found = false;
        for tty in redrawn {
            if let Some(s) = seen.get_mut(&tty) {
                found |= s.used == s.at;
                s.used = s.still_before.unwrap_or(s.used);
            }
        }
        self.unsaved |= seen != self.seen;
        self.seen = seen;
        found || (self.unsaved && now - self.saved_at >= SAVE_EVERY_MS)
    }

    /// When `tty` was used last, as the last look saw it; 0 when no look saw it used.
    fn used(&self, tty: &str) -> i64 {
        self.seen.get(tty).map_or(0, |s| s.used)
    }
}

/// The devices of `moves` that moved together: `REDRAW_TABS` or more within `REDRAW_WITHIN_MS`
/// of one another, in any window that long.
fn moved_together(mut moves: Vec<(&str, i64)>) -> std::collections::HashSet<&str> {
    moves.sort_by_key(|m| m.1);
    let mut out = std::collections::HashSet::new();
    let mut to = 0;
    for from in 0..moves.len() {
        to = to.max(from);
        while to + 1 < moves.len() && moves[to + 1].1 - moves[from].1 <= REDRAW_WITHIN_MS {
            to += 1;
        }
        if to + 1 - from >= REDRAW_TABS {
            out.extend(moves[from..=to].iter().map(|m| m.0));
        }
    }
    out
}

/// The time of each terminal device a process in `lines` is on: when it was last read from or
/// written to.
pub fn device_times(lines: &[PsLine]) -> Vec<(String, i64)> {
    let ttys: std::collections::BTreeSet<&str> = lines.iter().map(|l| l.tty.as_str()).filter(|t| !matches!(*t, "??" | "?" | "-" | "")).collect();
    ttys.into_iter().filter_map(|tty| Some((tty.to_string(), last_used(&Path::new("/dev").join(tty))?))).collect()
}

/// The tabs on this Mac now: one `ps`, and the kernel asked about each tab's shell and device,
/// with what the engine's looks saw of them before (`uses`), which this look adds to.
pub fn tabs_now(mut uses: Uses) -> Vec<Tab> {
    let listed = crate::procs::list(Duration::from_secs(10));
    uses.on_look(crate::util::now_ms(), &device_times(&listed.lines));
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
        let opened = crate::procs::proc_start_ms(found.shell).unwrap_or(listed.at);
        out.push(Tab {
            cwd: crate::procs::cwd_of(found.shell).map(|p| p.to_string_lossy().into_owned()).unwrap_or_default(),
            used: uses.used(&found.tty).max(opened),
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
        out.push(Tab { tty: tty.to_string(), terminal: name, shell: shell.pid, runs, procs: procs.iter().map(|p| (p.pid, p.ppid, p.command.clone())).collect(), cwd: String::new(), opened: 0, used: 0 });
    }
    out
}

/// The plan for the tabs open now, with what the engine saw of them (`Uses`).
pub fn plan_now(board: &Board, uses: Uses, after_ms: i64) -> Plan {
    plan(board, &tabs_now(uses), crate::util::now_ms(), after_ms)
}

/// Closes the tabs of `keys` that are still stale, looked at again now, whether they were
/// ticked or not: you picked them. One outcome a key, in order.
pub fn close(board: &Board, uses: Uses, keys: &[String], after_ms: i64) -> Vec<Outcome> {
    let tabs = tabs_now(uses);
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
    /// shell's processes, each with its parent and command line; `runs` are what closing it
    /// hangs up, by number.
    fn tab(tty: &str, shell: u32, procs: &[(u32, u32, &str)], runs: &[u32], unused: i64) -> Tab {
        let procs: Vec<(u32, u32, String)> = [(shell - 1, 1, "login"), (shell, shell - 1, "-zsh")].iter().chain(procs).map(|&(pid, parent, command)| (pid, parent, command.to_string())).collect();
        Tab {
            tty: tty.into(),
            terminal: "Ghostty",
            shell,
            runs: runs.iter().map(|pid| (*pid, procs.iter().find(|p| p.0 == *pid).unwrap().2.clone())).collect(),
            procs,
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
        assert_eq!(tabs[1].procs.iter().map(|p| (p.0, p.1)).collect::<Vec<_>>(), [(21, 10), (22, 21), (24, 22), (25, 22), (26, 25)]);
    }

    #[test]
    fn lists_the_tabs_of_terminal_apps_and_not_tmux_panes() {
        let taken: Vec<&str> = crate::launch::terminals().iter().filter(|t| takes_tabs_of(**t)).map(|t| t.name()).collect();
        match crate::os::current() {
            crate::os::Os::MacOs => assert_eq!(taken, ["Ghostty", "iTerm2", "Terminal", "Rex"]),
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
            session("nested", 104, Status::Idle, 3 * day),
        ]);
        let tabs = [
            tab("ttys001", 12, &[], &[], 4 * day),
            tab("ttys002", 22, &[(23, 22, "/usr/local/bin/node /usr/local/bin/npm run dev")], &[23], 3 * day),
            // A session's own processes (caffeinate) are its.
            tab("ttys003", 32, &[(33, 32, "claude"), (34, 33, "caffeinate -i")], &[33], 3 * day),
            tab("ttys004", 42, &[(43, 42, "claude")], &[43], 3 * day),
            // One OpenCode terminal showing two sessions.
            tab("ttys005", 52, &[(53, 52, "opencode")], &[53], 3 * day),
            // Used an hour ago: not stale.
            tab("ttys006", 62, &[], &[], 3_600_000),
            // A session under a wrapper that started it.
            tab("ttys007", 72, &[(74, 72, "node /usr/local/bin/codex"), (73, 74, "codex")], &[74], 3 * day),
            // A session with an editor stopped beside it.
            tab("ttys008", 82, &[(83, 82, "claude"), (84, 82, "vim notes.md")], &[84, 83], 3 * day),
            // Two jobs, and no session.
            tab("ttys009", 92, &[(93, 92, "python3 -m http.server"), (94, 92, "sleep 100")], &[93, 94], 3 * day),
            // A session in a nested shell with an editor beside it there.
            tab("ttys010", 102, &[(103, 102, "bash"), (104, 103, "claude"), (105, 103, "vim notes.md")], &[103], 3 * day),
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
        assert_eq!(row("ttys010:102"), (Shows::Session, "Session nested", false, "/work/northwind · unused 3 days · also runs `vim notes.md`"));
    }

    fn look(uses: &mut Uses, now: i64, devices: &[(&str, i64)]) -> bool {
        uses.on_look(now, &devices.iter().map(|(tty, at)| (tty.to_string(), *at)).collect::<Vec<_>>())
    }

    #[test]
    fn a_redraw_of_every_tab_is_not_use() {
        let hour = 3_600_000;
        let mut uses = Uses::default();
        // Monday: shells used at their own times, an agent printing as the look comes.
        let monday = [("ttys001", NOW - 50 * hour), ("ttys002", NOW - 40 * hour), ("ttys003", NOW - 30 * hour), ("ttys004", NOW - 20 * hour), ("ttys005", NOW - 12 * hour - 5_000)];
        look(&mut uses, NOW - 12 * hour, &monday);
        assert_eq!(uses.used("ttys001"), NOW - 50 * hour);
        // The lid opens: every shell redraws within seconds, and so does the agent, which then
        // works on, printing; one shell is typed in a minute later.
        let wake = NOW - 2 * hour;
        let opened = [("ttys001", wake + 100), ("ttys002", wake + 900), ("ttys003", wake + 2_500), ("ttys004", wake + 60_000), ("ttys005", wake + 1_200)];
        assert!(look(&mut uses, wake + 70_000, &opened), "a redraw is written at once");
        assert_eq!(uses.used("ttys001"), NOW - 50 * hour);
        assert_eq!(uses.used("ttys003"), NOW - 30 * hour);
        assert_eq!(uses.used("ttys004"), wake + 60_000, "a tab typed in alone is used");
        let working = [("ttys001", wake + 100), ("ttys002", wake + 900), ("ttys003", wake + 2_500), ("ttys004", wake + 60_000), ("ttys005", wake + 75_000)];
        look(&mut uses, wake + 80_000, &working);
        assert_eq!(uses.used("ttys005"), wake + 75_000, "the agent printing on is used");
        // Nothing moved: what was seen stands; a tab closed is forgotten.
        look(&mut uses, NOW, &working[..4]);
        assert_eq!((uses.used("ttys002"), uses.used("ttys005")), (NOW - 40 * hour, 0));
    }

    #[test]
    fn agents_printing_together_are_used() {
        let mut uses = Uses::default();
        let before: Vec<(&str, i64)> = ["ttys001", "ttys002", "ttys003", "ttys004"].into_iter().map(|t| (t, NOW - 9_000)).collect();
        assert!(look(&mut uses, NOW - 8_000, &before), "what's new is written");
        uses.written(NOW - 8_000);
        let after: Vec<(&str, i64)> = ["ttys001", "ttys002", "ttys003", "ttys004"].into_iter().map(|t| (t, NOW - 500)).collect();
        assert!(!look(&mut uses, NOW, &after), "written only every few minutes");
        assert!(after.iter().all(|(t, at)| uses.used(t) == *at));
    }

    #[test]
    fn a_redraw_two_looks_saw_is_one() {
        let mut uses = Uses::default();
        let old = NOW - DAY_MS;
        let hour = 3_600_000;
        look(&mut uses, NOW - 60_000, &[("ttys001", old), ("ttys002", old - hour), ("ttys003", old - 2 * hour), ("ttys004", old - 3 * hour)]);
        assert_eq!(uses.used("ttys004"), old - 3 * hour);
        look(&mut uses, NOW - 2_000, &[("ttys001", NOW - 4_000), ("ttys002", NOW - 3_500), ("ttys003", NOW - 3_000), ("ttys004", old - 3 * hour)]);
        look(&mut uses, NOW, &[("ttys001", NOW - 4_000), ("ttys002", NOW - 3_500), ("ttys003", NOW - 3_000), ("ttys004", NOW - 1_000)]);
        assert_eq!((uses.used("ttys001"), uses.used("ttys004")), (old, old - 3 * hour), "the last of the redraw, after a look");
    }

    #[test]
    fn a_first_look_counts_a_redraw_left_from_before_from_the_tabs_opening() {
        let mut uses = Uses::default();
        let wake = NOW - 6 * 3_600_000;
        look(&mut uses, NOW, &[("ttys001", wake + 100), ("ttys002", wake + 2_000), ("ttys003", wake + 3_000), ("ttys004", NOW - 2 * DAY_MS), ("ttys005", NOW - 500), ("ttys006", NOW - 400), ("ttys007", NOW - 300)]);
        assert_eq!([uses.used("ttys001"), uses.used("ttys002"), uses.used("ttys003")], [0, 0, 0]);
        assert_eq!(uses.used("ttys004"), NOW - 2 * DAY_MS, "a tab used alone");
        assert_eq!(uses.used("ttys006"), NOW - 400, "agents printing now");
    }

    #[test]
    fn a_redraw_split_two_then_one_is_one() {
        let mut uses = Uses::default();
        let hour = 3_600_000;
        let old = [("ttys001", NOW - 9 * hour), ("ttys002", NOW - 8 * hour), ("ttys003", NOW - 7 * hour)];
        look(&mut uses, NOW - hour, &old);
        uses.written(NOW - 3_000);
        assert!(!look(&mut uses, NOW - 2_000, &[("ttys001", NOW - 4_000), ("ttys002", NOW - 3_500), old[2]]), "two alone aren't a redraw yet");
        assert_eq!(uses.used("ttys001"), NOW - 4_000);
        assert!(look(&mut uses, NOW, &[("ttys001", NOW - 4_000), ("ttys002", NOW - 3_500), ("ttys003", NOW - 1_000)]), "the third makes it one, written at once");
        assert_eq!([uses.used("ttys001"), uses.used("ttys002"), uses.used("ttys003")], [NOW - 9 * hour, NOW - 8 * hour, NOW - 7 * hour]);
        // A tab of it closing later leaves the others' redraw as it was.
        look(&mut uses, NOW + 60_000, &[("ttys001", NOW - 4_000), ("ttys002", NOW - 3_500)]);
        assert_eq!(uses.used("ttys001"), NOW - 9 * hour);
    }

    #[test]
    fn a_redraw_is_any_ten_seconds_of_moves() {
        let mut uses = Uses::default();
        let hour = 3_600_000;
        let devices = ["ttys001", "ttys002", "ttys003", "ttys004"];
        look(&mut uses, NOW - hour, &devices.iter().enumerate().map(|(i, t)| (*t, NOW - 9 * hour - i as i64 * hour)).collect::<Vec<_>>());
        // A move of one, then three together nine to twelve seconds after it.
        look(&mut uses, NOW, &[("ttys001", NOW - 20_000), ("ttys002", NOW - 11_000), ("ttys003", NOW - 9_000), ("ttys004", NOW - 8_000)]);
        assert_eq!(uses.used("ttys001"), NOW - 20_000);
        assert_eq!([uses.used("ttys002"), uses.used("ttys003"), uses.used("ttys004")], [NOW - 10 * hour, NOW - 11 * hour, NOW - 12 * hour]);
    }

    #[test]
    fn writes_a_change_within_a_few_minutes() {
        let mut uses = Uses::default();
        assert!(look(&mut uses, NOW, &[("ttys001", NOW - 100)]));
        uses.written(NOW);
        assert!(!look(&mut uses, NOW + 60_000, &[("ttys001", NOW + 59_000)]), "written a minute ago");
        assert!(look(&mut uses, NOW + SAVE_EVERY_MS, &[("ttys001", NOW + 59_000)]), "what changed since is still to write");
        uses.written(NOW + SAVE_EVERY_MS);
        assert!(!look(&mut uses, NOW + 2 * SAVE_EVERY_MS, &[("ttys001", NOW + 59_000)]), "nothing changed");
    }

    #[test]
    fn a_session_that_waits_holds_its_tab_for_the_others() {
        let day = DAY_MS;
        let mut needs = session("needs", 43, Status::Needs, 3 * day);
        needs.title = "Fix login".into();
        let b = board(vec![session("idle", 43, Status::Idle, 4 * day), needs]);
        let p = plan(&b, &[tab("ttys004", 42, &[(43, 42, "opencode")], &[43], 3 * day)], NOW, day);
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
                if let Some(t) = tabs_now(Uses::default()).into_iter().find(|t| t.cwd == at && ready(t)) {
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
        let out = tab.as_ref().map(|t| close(&Board::default(), Uses::default(), &[key_of(t)], 0));
        std::thread::sleep(Duration::from_secs(1));
        let alive = |pid: u32, at: &str| crate::procs::pid_alive(pid) && crate::procs::cwd_of(pid).is_some_and(|c| c.to_string_lossy() == at);
        let shell_runs = tab.as_ref().is_some_and(|t| alive(t.shell, &behind_at));
        let sleep_runs = tab.as_ref().is_some_and(|t| crate::procs::pid_alive(t.runs[0].0));
        // Only the shells in the test's own folders are hung up, which closes their tabs.
        for t in tabs_now(Uses::default()).into_iter().filter(|t| t.cwd == behind_at || t.cwd == front_at) {
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
        let p = plan_now(&crate::engine::light_snapshot(), Uses::load(), 0);
        for t in &p.tabs {
            println!("{:<8} {:<16} {:<5} {:<40} {}", t.terminal, t.key, if t.ticked { "close" } else { "keep" }, t.title.chars().take(40).collect::<String>(), t.why);
        }
    }
}
