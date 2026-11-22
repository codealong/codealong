//! What a session's Bash tool is running, from the process table. Each command runs in a shell
//! that is a child of the session's process, with the command in its arguments as `eval '…'`
//! (`zsh -c source … && eval 'gcloud auth login' < /dev/null && pwd -P >| …`); its MCP servers
//! are children too, without one.

use std::cmp::Reverse;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use serde_json::Value;

use crate::procs;
use crate::util::{mtime_ms, parse_rfc3339_ms};

/// How close a finished subagent's Bash/Monitor call timestamp must be to a child shell's process
/// start time for the two to match.
const SUBAGENT_CALL_SLOP_MS: i64 = 120_000;
/// How close an unnamed main-session background shell must be to a child shell's start time to
/// keep it from being treated as a finished subagent's orphan.
const UNNAMED_MAIN_SLOP_MS: i64 = 10 * 60_000;

/// A Bash tool's running shell child of a session's process, with when it started.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Running {
    pub pid: u32,
    pub command: String,
    pub started_ms: Option<i64>,
}

/// Orders running shells newest first (by start time, breaking ties by later position in `shells`),
/// so an older leftover shell doesn't hide a newer command the session actually waits on.
fn newest_first(shells: Vec<Running>) -> Vec<Running> {
    let mut indexed: Vec<(usize, Running)> = shells.into_iter().enumerate().collect();
    indexed.sort_by_key(|(idx, s)| Reverse((s.started_ms, *idx)));
    indexed.into_iter().map(|(_, s)| s).collect()
}

/// Every Bash tool shell running directly under `pid`, newest first.
pub(super) fn all_running(pid: u32) -> Vec<Running> {
    let found = procs::child_commands(pid)
        .into_iter()
        .filter_map(|(child, started_ms, cmd)| Some(Running { pid: child, command: eval_command(&cmd)?, started_ms }))
        .collect();
    newest_first(found)
}

/// Every Bash tool shell running directly under `pid` in `listed`, newest first.
pub(super) fn all_in(listed: &[procs::PsLine], pid: u32, started: impl Fn(u32) -> Option<i64>) -> Vec<Running> {
    let found = listed
        .iter()
        .filter(|p| p.ppid == pid)
        .filter_map(|p| Some(Running { pid: p.pid, command: eval_command(&p.command)?, started_ms: started(p.pid) }))
        .collect();
    newest_first(found)
}

/// The newest command a session's Bash tool is running, from a listing of the processes this tick
/// (`procs::Listing`): for whether one still runs, which a reading kept from an earlier look
/// can't say.
pub(super) fn running_in(listed: &[procs::PsLine], pid: u32) -> Option<String> {
    all_in(listed, pid, procs::proc_start_ms).into_iter().next().map(|r| r.command)
}

/// Whether every running shell of a Claude session came from a subagent that has already finished
/// (`<session>/subagents/agent-*.meta.json` says `finished: true`), and none belongs to the main
/// session's own open background tasks.
///
/// When a subagent runs a Bash command that moves to the background (past its timeout or with
/// `run_in_background`) and then finishes, Claude Code keeps that child shell under the parent
/// session's process and writes `status: "shell"` after every later turn even though the subagent
/// is gone.
pub(super) fn all_from_finished_subagents(transcript: &Path, session_id: &str, running: &[Running], main_open: &[super::Shell]) -> bool {
    if running.is_empty() {
        return false;
    }
    let open: Vec<&super::Shell> = main_open.iter().filter(|s| s.ended.is_none()).collect();
    for r in running {
        let cmd = r.command.trim();
        let owned_by_main = open.iter().any(|s| {
            let main_cmd = s.command.trim();
            if !main_cmd.is_empty() {
                main_cmd == cmd
            } else {
                r.started_ms.is_none_or(|started| (s.at - started).abs() <= UNNAMED_MAIN_SLOP_MS)
            }
        });
        if owned_by_main {
            return false;
        }
    }
    let Some(dir) = transcript.parent().map(|d| d.join(session_id).join("subagents")) else { return false };
    let Ok(entries) = std::fs::read_dir(&dir) else { return false };
    let oldest_started = running.iter().filter_map(|r| r.started_ms).min();
    let mut logs = Vec::new();
    for e in entries.flatten() {
        let path = e.path();
        let name = e.file_name().to_string_lossy().into_owned();
        let Some(stem) = name.strip_suffix(".meta.json") else { continue };
        let Ok(bytes) = std::fs::read(&path) else { continue };
        let Ok(meta) = serde_json::from_slice::<Value>(&bytes) else { continue };
        if meta.get("finished").and_then(Value::as_bool) != Some(true) {
            continue;
        }
        let log = dir.join(format!("{stem}.jsonl"));
        if !log.is_file() {
            continue;
        }
        // A subagent whose log was last written before the oldest running shell started couldn't
        // have spawned it.
        if let (Some(oldest), Some(mtime)) = (oldest_started, mtime_ms(&log)) {
            if mtime < oldest - 60_000 {
                continue;
            }
        }
        logs.push(log);
    }
    if logs.is_empty() {
        return false;
    }
    let mut matched = vec![false; running.len()];
    for log in logs {
        let Ok(file) = File::open(&log) else { continue };
        let mut reader = BufReader::new(file);
        let mut line = String::new();
        while reader.read_line(&mut line).unwrap_or(0) > 0 {
            if line.contains("\"assistant\"") && line.contains("\"tool_use\"") && !line.contains("api-request-blob") {
                if let Ok(v) = serde_json::from_str::<Value>(&line) {
                    if v.get("type").and_then(Value::as_str) == Some("assistant") {
                        let call_at = v.get("timestamp").and_then(Value::as_str).and_then(parse_rfc3339_ms);
                        for item in v.pointer("/message/content").and_then(Value::as_array).into_iter().flatten() {
                            if item.get("type").and_then(Value::as_str) != Some("tool_use") {
                                continue;
                            }
                            if !matches!(item.get("name").and_then(Value::as_str), Some("Bash" | "Monitor")) {
                                continue;
                            }
                            let Some(cmd) = item.pointer("/input/command").and_then(Value::as_str).map(str::trim).filter(|c| !c.is_empty()) else { continue };
                            for (i, r) in running.iter().enumerate() {
                                if !matched[i] && r.command.trim() == cmd && r.started_ms.zip(call_at).is_none_or(|(started, at)| (at - started).abs() <= SUBAGENT_CALL_SLOP_MS) {
                                    matched[i] = true;
                                }
                            }
                        }
                    }
                }
            }
            line.clear();
            if matched.iter().all(|&m| m) {
                return true;
            }
        }
    }
    matched.iter().all(|&m| m)
}

/// What the commands several sessions' Bash tools run are made of right now (`procs::CommandTree`),
/// from one look at the process table: every process under their shells.
pub(super) fn command_trees(listed: &[procs::PsLine], pids: &[u32]) -> HashMap<u32, procs::CommandTree> {
    procs::command_trees(listed, pids, is_tool_shell)
}

/// A shell the Bash tool runs a command in, by its command line.
fn is_tool_shell(command: &str) -> bool {
    command.contains("eval '")
}

/// `zsh -c source … && eval 'gcloud auth login' < /dev/null && pwd -P >| …` → `gcloud auth login`.
/// Unescapes both POSIX single-quote forms inside `eval '…'`: `'\''` and `'"'"'`, which Claude
/// Code writes when a command contains single quotes.
pub(super) fn eval_command(cmdline: &str) -> Option<String> {
    let (_, rest) = cmdline.split_once("eval '")?;
    let inner = rest.rsplit_once("' < /dev/null").map(|(c, _)| c).unwrap_or(rest);
    Some(inner.replace("'\"'\"'", "'").replace("'\\''", "'")).filter(|c| !c.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_command_tree_is_the_shells_under_a_session_and_what_they_run() {
        let row = |pid: u32, ppid: u32, cpu: u64, command: &str| procs::PsLine { pid, ppid, tty: "??".into(), cpu: Some(cpu), command: command.into() };
        let listed = [
            row(100, 1, 500, "claude"),
            row(200, 100, 100, "node mcp-server.js"),
            row(300, 100, 1, "/bin/zsh -c source snap.sh && eval 'spruce login --host x | tail -5' < /dev/null && pwd -P"),
            row(301, 300, 88, "node spruce login --host x"),
            row(302, 300, 0, "tail -5"),
            row(303, 301, 372345, "helper"),
            row(400, 1, 10, "other"),
        ];
        let trees = command_trees(&listed, &[100, 999]);
        assert_eq!(trees[&100].cpu, vec![(300, 1), (301, 88), (302, 0), (303, 372345)]);
        assert!(trees[&999].cpu.is_empty());
        // What its shell runs, and nothing for a session whose children run none.
        assert_eq!(running_in(&listed, 100).as_deref(), Some("spruce login --host x | tail -5"));
        assert_eq!((running_in(&listed, 300), running_in(&listed, 999)), (None, None));
    }

    #[test]
    fn picks_the_newest_running_shell_over_an_older_leftover() {
        let row = |pid: u32, command: &str| procs::PsLine { pid, ppid: 100, tty: "??".into(), cpu: Some(1), command: command.into() };
        let listed = [
            row(300, "/bin/zsh -c eval 'until grep -q exit out; do sleep 5; done' < /dev/null"),
            row(400, "/bin/zsh -c eval 'scripts/relaunch.sh' < /dev/null"),
            row(250, "/bin/zsh -c eval 'npm test' < /dev/null"),
        ];
        let started = |pid: u32| match pid {
            300 => Some(1_000),
            400 => Some(2_000),
            250 => Some(3_000),
            _ => None,
        };
        let ordered = all_in(&listed, 100, started);
        assert_eq!(ordered.iter().map(|r| (r.pid, r.command.as_str())).collect::<Vec<_>>(), [(250, "npm test"), (400, "scripts/relaunch.sh"), (300, "until grep -q exit out; do sleep 5; done")]);
        // Without start times, later rows still win deterministically.
        let untimed = all_in(&listed, 100, |_| None);
        assert_eq!(untimed[0].command, "npm test");
    }

    #[test]
    fn detects_shells_left_behind_by_a_finished_subagent() {
        let dir = crate::fixtures::scratch("subagent-orphan-shells");
        let id = "438d6313-e11d-4f89-afcc-cc80f042cefb";
        let transcript = dir.join(format!("{id}.jsonl"));
        let subs = dir.join(id).join("subagents");
        std::fs::create_dir_all(&subs).unwrap();
        std::fs::write(&transcript, "").unwrap();
        let poll = "until grep -q \"^exit\" out; do sleep 5; done";
        let log_ts = "2026-11-20T18:24:01.862Z";
        let started_ms = parse_rfc3339_ms(log_ts).unwrap();
        std::fs::write(subs.join("agent-a1.meta.json"), r#"{"agentType":"general-purpose","finished":true}"#).unwrap();
        let call = serde_json::json!({
            "type": "assistant",
            "timestamp": log_ts,
            "message": { "role": "assistant", "content": [{ "type": "tool_use", "id": "toolu_1", "name": "Bash", "input": { "command": poll } }] }
        });
        std::fs::write(subs.join("agent-a1.jsonl"), format!("{call}\n")).unwrap();

        let running = vec![Running { pid: 48892, command: poll.into(), started_ms: Some(started_ms + 500) }];
        assert!(all_from_finished_subagents(&transcript, id, &running, &[]));

        // If the main session itself also has that shell open, it isn't orphaned.
        let main = vec![super::super::Shell { id: "b1".into(), at: started_ms, command: poll.into(), ..Default::default() }];
        assert!(!all_from_finished_subagents(&transcript, id, &running, &main));
        // An ended main shell doesn't claim the running orphan.
        let main_ended = vec![super::super::Shell { id: "b1".into(), at: started_ms, command: poll.into(), ended: Some(super::super::ShellEnd { at: started_ms + 1000, status: "killed".into(), exit_code: None }), ..Default::default() }];
        assert!(all_from_finished_subagents(&transcript, id, &running, &main_ended));

        // Another running shell that didn't come from a finished subagent keeps the session working.
        let mixed = vec![
            Running { pid: 50000, command: "scripts/relaunch.sh".into(), started_ms: Some(started_ms + 60_000) },
            running[0].clone(),
        ];
        assert!(!all_from_finished_subagents(&transcript, id, &mixed, &[]));

        // A shell started hours later with the same command text isn't matched to an earlier subagent call.
        let later = vec![Running { pid: 50001, command: poll.into(), started_ms: Some(started_ms + 3_600_000) }];
        assert!(!all_from_finished_subagents(&transcript, id, &later, &[]));

        // A subagent that hasn't finished yet isn't an orphan.
        std::fs::write(subs.join("agent-a1.meta.json"), r#"{"agentType":"general-purpose","finished":false}"#).unwrap();
        assert!(!all_from_finished_subagents(&transcript, id, &running, &[]));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `CODEALONG_TEST_PIDS=<pid,…> cargo test command_trees_here -- --ignored --nocapture`
    /// prints what those Claude sessions' shells are running, looked at twice a minute apart, and
    /// what each waits behind for a lock and whether that was active.
    #[test]
    #[ignore]
    fn command_trees_here() {
        let pids: Vec<u32> = std::env::var("CODEALONG_TEST_PIDS").unwrap_or_default().split(',').filter_map(|p| p.parse().ok()).collect();
        let look = || {
            let listed = procs::list(std::time::Duration::from_secs(30)).lines;
            command_trees(&listed, &pids).into_iter().map(|(pid, tree)| (pid, (tree.waits_behind(&listed, procs::open_files, procs::held_exclusively), tree))).collect::<HashMap<_, _>>()
        };
        let before = look();
        std::thread::sleep(std::time::Duration::from_secs(60));
        for (pid, (behind, after)) in look() {
            let (was_behind, was) = &before[&pid];
            println!("{pid}: {:?} → {:?}, active: {}", was.cpu, after.cpu, after.active_since(was));
            println!("  behind {:?} → {:?}, active: {}", was_behind.cpu, behind.cpu, !was_behind.cpu.is_empty() && behind.active_since(was_behind));
        }
    }

    #[test]
    fn reads_the_command_a_shell_is_running() {
        let line = "/bin/zsh -c source /Users/me/.claude2/shell-snapshots/snapshot-zsh-1.sh 2>/dev/null || true && eval 'gcloud auth login' < /dev/null && pwd -P >| /tmp/claude-57ad-cwd";
        assert_eq!(eval_command(line).as_deref(), Some("gcloud auth login"));
        let quoted = "zsh -c eval 'echo '\\''hi'\\''' < /dev/null && pwd -P";
        assert_eq!(eval_command(quoted).as_deref(), Some("echo 'hi'"));
        let claude_quoted = r#"zsh -c eval 'grep '"'"'^== '"'"' log' < /dev/null && pwd -P"#;
        assert_eq!(eval_command(claude_quoted).as_deref(), Some("grep '^== ' log"));
        assert_eq!(eval_command("node server.js"), None);
    }
}
