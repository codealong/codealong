//! The Codex app's own logs, in `~/Library/Logs/com.openai.codex/<year>/<month>/<day>/` by UTC
//! date: a file a day for each of the app's processes, named with its launch and pid
//! (`codex-desktop-<launch>-<pid>-t<n>-i1-<hhmmss>-<part>.log`), each line starting with its
//! time (`2026-10-19T21:50:50.371Z info [mac-system-shutdown] …`). Whether a window took a
//! thread is read from them too (ipc.rs `taken_in_log`).
//!
//! When one of the app's processes ended is the one way to tell a turn the app stopped as it
//! quit from one you stopped: Codex writes `turn_aborted` with `interrupted` for both (its
//! session's `shutdown` aborts every turn as `interrupt_task` does, codex-rs of 2026-04), so the
//! turn isn't left open for `codex_session`'s other cut. As macOS restarted on 2026-10-19 the app
//! logged `[mac-system-shutdown] macOS system shutdown requested; allowing app quit`, its last
//! line 0.36 s later, and the three turns it had under way were aborted 0.12 to 0.18 s after that.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::procs::PsLine;
use crate::util::{civil_from_days, parse_rfc3339_ms, read_last};

/// Where the app logs.
pub fn dir() -> PathBuf {
    crate::util::home().join("Library/Logs/com.openai.codex")
}

/// How far from a process's last line a turn it stopped as it quit is: the app-server under it
/// writes the turn's end as it's stopped, around when the app logs its last.
const NEAR_MS: i64 = 10_000;

const DAY_MS: i64 = 86_400_000;

/// How much of the end of a log is read for its last line, which can be a long stack.
const TAIL: u64 = 64 * 1024;

/// When the app's processes ended, as far as the turns stopped need it: a day's logs are read the
/// first time a turn stopped that day is asked about, and again once the app's processes that
/// run have changed.
#[derive(Default)]
pub struct Quits {
    /// The app's processes at the last look; None while the processes couldn't be listed.
    running: Option<HashSet<u32>>,
    /// By the start of a UTC day, when each of the app's processes that logged that day and had
    /// ended by the last look logged last (`ended_in`).
    ended: HashMap<i64, Vec<i64>>,
}

impl Quits {
    /// Takes in the processes listed now. None at all is a `ps` that failed (`procs::Listing`),
    /// and nothing is taken as ended then.
    pub fn look(&mut self, processes: &[PsLine]) {
        let running = (!processes.is_empty()).then(|| app_pids(processes));
        if running != self.running {
            self.ended.clear();
            self.running = running;
        }
    }

    /// Whether a turn stopped at `at` was stopped as one of the app's processes quit, by the
    /// logs in `dir`.
    pub fn quit_at(&mut self, dir: &Path, at: i64) -> bool {
        let Quits { running, ended } = self;
        let Some(running) = running else { return false };
        // The day of the earliest a process could have ended for it, read with the next.
        let day = (at - NEAR_MS).div_euclid(DAY_MS) * DAY_MS;
        ended.entry(day).or_insert_with(|| ended_in(dir, day, running)).iter().any(|end| (at - end).abs() <= NEAR_MS)
    }
}

/// The app's processes: its executable, which launches the rest. The app is ChatGPT.app, known
/// by its bundle id (`APP_BUNDLE_ID`), and was Codex.app before.
fn app_pids(processes: &[PsLine]) -> HashSet<u32> {
    processes.iter().filter(|p| ["/ChatGPT.app/Contents/MacOS/", "/Codex.app/Contents/MacOS/"].iter().any(|exe| p.command.contains(exe))).map(|p| p.pid).collect()
}

/// The folder of the UTC day `at` falls on.
fn day_dir(dir: &Path, at: i64) -> PathBuf {
    let (y, m, d) = civil_from_days(at.div_euclid(DAY_MS));
    dir.join(format!("{y:04}/{m:02}/{d:02}"))
}

/// When each of the app's processes that logged on the day starting at `day` or the next, and
/// isn't one of `running`, logged last on them. The two days are read together: a process that
/// ran on into the next day can have a last line on the first at a turn's time though it didn't
/// quit then, and one started just after midnight has none on the first. One that ran on into a
/// third day has its last line on the second near that midnight, far from any turn the two days
/// are read for.
fn ended_in(dir: &Path, day: i64, running: &HashSet<u32>) -> Vec<i64> {
    let mut last: HashMap<String, (u32, i64)> = HashMap::new();
    for folder in [day_dir(dir, day), day_dir(dir, day + DAY_MS)] {
        let Ok(entries) = std::fs::read_dir(folder) else { continue };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some((process, pid)) = name.to_str().and_then(process_of) else { continue };
            let Some(at) = last_line_at(&entry.path()) else { continue };
            let slot = last.entry(process.to_string()).or_insert((pid, at));
            slot.1 = slot.1.max(at);
        }
    }
    last.into_values().filter(|(pid, _)| !running.contains(pid)).map(|(_, at)| at).collect()
}

/// The process a log file is of, as `<launch>-<pid>`, and its pid.
fn process_of(name: &str) -> Option<(&str, u32)> {
    let rest = name.strip_prefix("codex-desktop-")?;
    // The launch is a UUID: 36 characters, dashes and all.
    let pid = rest.get(37..)?.split('-').next()?;
    Some((rest.get(..37 + pid.len())?, pid.parse().ok()?))
}

/// The time of a log's last line that has one.
fn last_line_at(path: &Path) -> Option<i64> {
    let tail = read_last(path, TAIL).ok()?;
    tail.text().lines().rev().find_map(|line| parse_rfc3339_ms(line.split(' ').next()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    const LAUNCH: &str = "0b8f3c2e-5d41-4a7e-9c11-2f6a8e4d7b90";

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("codealong-applog-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn write(dir: &Path, day: &str, file: &str, lines: &[&str]) {
        let folder = dir.join(day);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join(file), lines.iter().map(|l| format!("{l}\n")).collect::<String>()).unwrap();
    }

    fn at(s: &str) -> i64 {
        parse_rfc3339_ms(s).unwrap()
    }

    /// The processes, with the app's executable at each of `pids`.
    fn ps(pids: &[u32]) -> Vec<PsLine> {
        let line = |pid: u32, command: &str| PsLine { pid, ppid: 1, tty: "??".into(), cpu: None, command: command.into() };
        let mut out: Vec<PsLine> = pids.iter().map(|&pid| line(pid, "/Applications/ChatGPT.app/Contents/MacOS/ChatGPT")).collect();
        out.push(line(1, "/sbin/launchd"));
        out
    }

    #[test]
    fn reads_a_log_files_process() {
        assert_eq!(process_of(&format!("codex-desktop-{LAUNCH}-45931-t0-i1-000006-0.log")), Some((&*format!("{LAUNCH}-45931"), 45931)));
        assert_eq!(process_of("codex-desktop-short-45931-t0-i1-000006-0.log"), None);
        assert_eq!(process_of("main.log"), None);
    }

    #[test]
    fn a_turn_stopped_as_the_apps_process_quit_was_its_quit() {
        let logs = scratch("quit");
        // The app's process as macOS restarted, and its second file, which ended earlier.
        write(&logs, "2026/10/19", &format!("codex-desktop-{LAUNCH}-45931-t0-i1-000006-0.log"), &[
            "2026-10-19T21:50:47.470Z info [electron-message-handler] Reasoning summary item completed",
            "2026-10-19T21:50:50.016Z info [mac-system-shutdown] macOS system shutdown requested; allowing app quit",
            "2026-10-19T21:50:50.371Z warning [IpcClient] Received broadcast but no handler is configured method=ipc-connection-reset",
            "    at a stack line with no time",
        ]);
        write(&logs, "2026/10/19", &format!("codex-desktop-{LAUNCH}-45931-t1-i1-023632-0.log"), &["2026-10-19T21:36:59.116Z warning [git] git.command.complete"]);
        // The one opened after, still running.
        write(&logs, "2026/10/19", "codex-desktop-7e2d9a14-c3b8-4f06-8d5e-91a7c4f0e2b3-6469-t0-i1-220008-0.log", &["2026-10-19T22:11:02.252Z info [electron-message-handler] x"]);
        let mut quits = Quits::default();
        quits.look(&ps(&[6469]));
        assert!(quits.quit_at(&logs, at("2026-10-19T21:50:50.554Z")));
        // Stopped by you a minute and a half before, or while the app that's running ran it.
        assert!(!quits.quit_at(&logs, at("2026-10-19T21:49:16.384Z")));
        assert!(!quits.quit_at(&logs, at("2026-10-19T22:11:01.000Z")));
        // With no processes listed, nothing is known to have ended.
        let mut blind = Quits::default();
        blind.look(&[]);
        assert!(!blind.quit_at(&logs, at("2026-10-19T21:50:50.554Z")));
        let _ = std::fs::remove_dir_all(&logs);
    }

    #[test]
    fn a_process_still_running_or_running_on_into_the_next_day_didnt_quit() {
        let logs = scratch("days");
        let file = |started: &str| format!("codex-desktop-{LAUNCH}-45931-t0-i1-{started}-0.log");
        write(&logs, "2026/10/18", &file("000002"), &["2026-10-18T23:59:58.000Z info [x] last of the day"]);
        let stop = at("2026-10-18T23:59:57.000Z");
        let mut quits = Quits::default();
        quits.look(&ps(&[45931]));
        assert!(!quits.quit_at(&logs, stop), "still running");
        quits.look(&ps(&[6469]));
        assert!(quits.quit_at(&logs, stop), "it ended as the day did");
        // It went on the next day: its end is there.
        write(&logs, "2026/10/19", &file("000006"), &["2026-10-19T08:00:00.000Z info [x] later"]);
        quits.look(&ps(&[]));
        quits.look(&ps(&[6469]));
        assert!(!quits.quit_at(&logs, stop));
        // Its last line just after midnight, for a turn stopped just before.
        write(&logs, "2026/10/19", &file("000006"), &["2026-10-19T00:00:01.000Z info [x] quitting"]);
        quits.look(&ps(&[]));
        quits.look(&ps(&[6469]));
        assert!(quits.quit_at(&logs, stop));
        let _ = std::fs::remove_dir_all(&logs);
        // One started just after midnight that quit seconds later, with nothing the day before.
        let logs = scratch("midnight");
        let late = "codex-desktop-7e2d9a14-c3b8-4f06-8d5e-91a7c4f0e2b3-7001-t0-i1-000001-0.log";
        write(&logs, "2026/10/19", late, &["2026-10-19T00:00:01.000Z info [x] up", "2026-10-19T00:00:05.000Z info [x] quitting"]);
        let mut quits = Quits::default();
        quits.look(&ps(&[6469]));
        assert!(quits.quit_at(&logs, at("2026-10-19T00:00:06.000Z")));
        assert!(!quits.quit_at(&logs, at("2026-10-19T00:01:00.000Z")));
        let _ = std::fs::remove_dir_all(&logs);
    }

    #[test]
    fn day_folders_are_utc_dates() {
        let logs = Path::new("/logs");
        assert_eq!(day_dir(logs, at("2026-10-19T00:00:06.039Z")), Path::new("/logs/2026/10/19"));
        assert_eq!(day_dir(logs, at("2026-10-05T23:59:59.999Z")), Path::new("/logs/2026/10/05"));
    }
}
