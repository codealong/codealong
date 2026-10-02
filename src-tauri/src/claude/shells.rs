//! What a session's Bash tool is running, from the process table. Each command runs in a shell
//! that is a child of the session's process, with the command in its arguments as `eval '…'`
//! (`zsh -c source … && eval 'gcloud auth login' < /dev/null && pwd -P >| …`); its MCP servers
//! are children too, without one.

use std::collections::HashMap;

use crate::procs;

/// The command a session's Bash tool is running: the `eval '<command>'` in the arguments of its
/// shell child. For saying what a stuck session is stuck on.
pub(super) fn running_command(pid: u32) -> Option<String> {
    procs::find_child(pid, eval_command)
}

/// What the commands several sessions' Bash tools run are made of right now (`procs::CommandTree`),
/// from one look at the process table: every process under their shells.
pub(super) fn command_trees(pids: &[u32]) -> HashMap<u32, procs::CommandTree> {
    procs::command_trees(pids, is_tool_shell)
}

/// A shell the Bash tool runs a command in, by its command line.
fn is_tool_shell(command: &str) -> bool {
    command.contains("eval '")
}

/// `zsh -c source … && eval 'gcloud auth login' < /dev/null && pwd -P >| …` → `gcloud auth login`.
fn eval_command(cmdline: &str) -> Option<String> {
    let (_, rest) = cmdline.split_once("eval '")?;
    let inner = rest.rsplit_once("' < /dev/null").map(|(c, _)| c).unwrap_or(rest);
    Some(inner.replace("'\\''", "'")).filter(|c| !c.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_command_tree_is_the_shells_under_a_session_and_what_they_run() {
        let table = "\
  100     1   0:05.00 claude
  200   100   0:01.00 node mcp-server.js
  300   100   0:00.01 /bin/zsh -c source snap.sh && eval 'spruce login --host x | tail -5' < /dev/null && pwd -P
  301   300   0:00.88 node spruce login --host x
  302   300   0:00.00 tail -5
  303   301 1:02:03.45 helper
  400     1   0:00.10 other
";
        let trees = procs::command_trees_in(table, &[100, 999], is_tool_shell);
        assert_eq!(trees[&100].cpu, vec![(300, 1), (301, 88), (302, 0), (303, 372345)]);
        assert!(trees[&999].cpu.is_empty());
    }

    /// `CODEALONG_TEST_PIDS=<pid,…> cargo test command_trees_here -- --ignored --nocapture`
    /// prints what those Claude sessions' shells are running, looked at twice a minute apart.
    #[test]
    #[ignore]
    fn command_trees_here() {
        let pids: Vec<u32> = std::env::var("CODEALONG_TEST_PIDS").unwrap_or_default().split(',').filter_map(|p| p.parse().ok()).collect();
        let before = command_trees(&pids);
        std::thread::sleep(std::time::Duration::from_secs(60));
        for (pid, after) in command_trees(&pids) {
            println!("{pid}: {:?} → {:?}, active: {}", before[&pid].cpu, after.cpu, after.active_since(&before[&pid]));
        }
    }

    #[test]
    fn reads_the_command_a_shell_is_running() {
        let line = "/bin/zsh -c source /Users/me/.claude2/shell-snapshots/snapshot-zsh-1.sh 2>/dev/null || true && eval 'gcloud auth login' < /dev/null && pwd -P >| /tmp/claude-57ad-cwd";
        assert_eq!(eval_command(line).as_deref(), Some("gcloud auth login"));
        let quoted = "zsh -c eval 'echo '\\''hi'\\''' < /dev/null && pwd -P";
        assert_eq!(eval_command(quoted).as_deref(), Some("echo 'hi'"));
        assert_eq!(eval_command("node server.js"), None);
    }
}
