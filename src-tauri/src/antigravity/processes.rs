//! The command line's processes: which conversation each `agy` has open and the logs it keeps,
//! whether the hub app's agent runs, and the commands a conversation's turn left running in the
//! background, which `agy` runs in shells under its process.

use std::path::PathBuf;

use super::{file_name, log_begun, Conversation, Reader, CLI};
use crate::harness::Look;
use crate::procs;

/// An `agy` process on this Mac.
#[derive(Debug, Clone, PartialEq)]
pub struct Process {
    pub pid: u32,
    /// The conversation it has open, from the presence lock it holds.
    pub conversation: Option<String>,
    /// It's the interactive UI in a terminal, rather than a print run.
    pub terminal: bool,
    /// The logs it has open: its own among them (`own_log`).
    pub logs: Vec<PathBuf>,
    /// When it began, as the kernel has it: the only time a conversation it opened and never
    /// prompted has, its row's being gorm's zero.
    pub started: Option<i64>,
}

/// For a process's command line that's `agy` running a conversation: whether it's the
/// interactive UI rather than a print run. None for its other commands (`models`, `update`…)
/// and for anything else. `ps` joins the arguments with spaces, so a prompt reads as words;
/// only the words before it matter.
pub fn cli_args(command: &str) -> Option<bool> {
    let (program, rest) = crate::util::program_and_rest(command)?;
    let mut words = rest.split_whitespace();
    if crate::util::program_name(program) != CLI {
        return None;
    }
    while let Some(w) = words.next() {
        match w.split('=').next().unwrap_or(w) {
            "-p" | "--print" | "--prompt" | "--input-format" => return Some(false),
            "-i" | "--prompt-interactive" => return Some(true),
            "--add-dir" | "--agent" | "--conversation" | "--effort" | "--json-schema" | "--log-file" | "--mode" | "--model" | "--output-format" | "--print-timeout" | "--project" if !w.contains('=') => {
                words.next();
            }
            _ if w.starts_with('-') => {}
            "acp" | "agent" | "agents" | "changelog" | "help" | "install" | "mcp" | "mic-serve" | "models" | "plugin" | "plugins" | "remote-control" | "update" => return None,
            // A prompt.
            _ => return Some(true),
        }
    }
    Some(true)
}

/// Whether a process's command line is the hub app's agent, which runs the app's conversations.
pub(super) fn is_app_agent(command: &str) -> bool {
    let program = command.split(" --").next().unwrap_or(command);
    program.ends_with("/language_server") && program.contains("Antigravity")
}

/// The conversation a command line process has open: the presence lock among its open files.
pub(super) fn held_conversation(open_files: &[PathBuf]) -> Option<String> {
    open_files.iter().find_map(|p| {
        (p.parent().map(file_name).as_deref() == Some("presence") && p.extension().and_then(|x| x.to_str()) == Some("lock")).then(|| p.file_stem().map(|s| s.to_string_lossy().into_owned())).flatten()
    })
}

/// The command line's logs a process has open (`log/cli-<time>.log`), from the same list.
pub(super) fn open_logs(open_files: &[PathBuf]) -> Vec<PathBuf> {
    let mut logs: Vec<PathBuf> = open_files.iter().filter(|p| p.parent().map(file_name).as_deref() == Some("log") && log_begun(p).is_some()).cloned().collect();
    logs.sort();
    logs.dedup();
    logs
}

/// The command line's processes, each with the conversation and the logs it has open, and
/// whether the hub app's agent is running. From the list of every process, and each `agy`'s
/// open files as the kernel lists them: an `lsof` for each took half a second at a high load,
/// on the engine's thread.
pub fn processes(listed: &[procs::PsLine]) -> (Vec<Process>, bool) {
    processes_excluding(listed, crate::side::runs)
}

pub(super) fn processes_excluding(listed: &[procs::PsLine], skip: impl Fn(u32) -> bool) -> (Vec<Process>, bool) {
    let mut app = false;
    let mut found = Vec::new();
    for p in listed {
        app |= is_app_agent(&p.command);
        if skip(p.pid) {
            continue;
        }
        if let Some(interactive) = cli_args(&p.command) {
            // One gone since the list was made, or not yours, has nothing open that's known.
            let files = procs::open_files(p.pid).unwrap_or_default();
            found.push(Process { pid: p.pid, conversation: held_conversation(&files), terminal: interactive && !p.tty.starts_with('?'), logs: open_logs(&files), started: procs::proc_start_ms(p.pid) });
        }
    }
    (found, app)
}

/// A command a conversation's turn left running in the background. `agy` puts a command there
/// once it has run for as long as its call said to wait (`WaitMsBeforeAsync`, 10 seconds as the
/// agent writes it): the call's result says it's a task (`Tool is running as a background task
/// with task id: …`), the agent ends its turn saying it waits, and the command's step runs on
/// (`Conversation::step_under_way`) under the process, in a shell (`zsh -c <command>`) or in the
/// shell's place, until its result comes back as a message that starts the agent again (1.5,
/// 2026-11-24). A command that can never end (one waiting on its input, which nothing gives it)
/// holds the conversation's row "not fully idle" for as long as the process lives: a commit
/// message in double quotes ran `agy acp` from its backquotes, which sat for a day (2026-11-23).
#[derive(Debug, Clone, PartialEq)]
pub struct Background {
    /// The command, as the process table has it (`procs::shell_script`).
    pub command: String,
    /// When its processes were last seen doing something (`procs::LeftRunning`); None until
    /// they've been looked at.
    pub active_at: Option<i64>,
}

/// The command `agy` process `pid` runs for its agent: the newest process under it, as
/// `procs::shell_script` reads it. A process holds nothing else under it while it waits at its
/// box (1.5, 2026-11-24).
fn command_under(listed: &[procs::PsLine], pid: u32) -> Option<String> {
    listed.iter().filter(|p| p.ppid == pid).last().map(|p| procs::shell_script(&p.command)).filter(|c| !c.trim().is_empty())
}

/// When a conversation's turn ended, by its latest step, which the next turn's steps move on:
/// for telling the command one turn left running from the next's.
fn left_at(c: &Conversation) -> i64 {
    c.latest_step_at.unwrap_or(c.last_modified)
}

impl Reader {
    /// Looks at what the commands conversations' turns left running in the background are
    /// doing, once a minute (`procs::LeftRunning`).
    pub(super) fn watch_background(&mut self, look: &Look) {
        if !self.background.due() {
            return;
        }
        let left: Vec<(u32, i64)> = self.conversations.values().flatten().filter_map(|c| self.left_in(c, look).map(|(pid, _)| (pid, left_at(c)))).collect();
        self.background.look(&look.ps, &left, |listed, pids| procs::command_trees(listed, pids, |_| true), look.now);
    }

    /// What `c`'s turn left running in the background, if anything, and when it was last seen
    /// doing something.
    pub(super) fn background_of(&self, c: &Conversation, look: &Look) -> Option<Background> {
        let (pid, command) = self.left_in(c, look)?;
        Some(Background { command, active_at: self.background.active_at(pid, left_at(c)) })
    }

    /// The process that has `c` and the command it runs in the background for it: its turn is
    /// over by its database's word, though a step of it runs on, and a process runs under it.
    fn left_in(&self, c: &Conversation, look: &Look) -> Option<(u32, String)> {
        if !c.cli || !c.turn_ended || !c.step_under_way || c.waiting {
            return None;
        }
        let p = self.processes.iter().find(|p| p.conversation.as_deref() == Some(c.id.as_str()))?;
        Some((p.pid, command_under(look.ps.all(), p.pid)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::Reader as _;
    use crate::model::Status;
    use crate::procs::{Listed, Listing, PsLine};

    #[test]
    fn a_turn_that_ended_with_a_command_running_under_agy_left_it_running() {
        let now = 1_800_000_000_000;
        let ended = now - 120_000;
        let account = crate::config::AccountCfg { name: "antigravity".into(), harness: "antigravity".into(), home: "/Users/me/.gemini/antigravity".into(), command: None, start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false, chrome_profile: None };
        // Past any pid a system gives, so nothing here is asked what files it has open.
        let (agy, other) = (4_200_041, 4_200_050);
        let c = Conversation { id: "c1".into(), cli: true, status: "cascade_run_status_idle".into(), not_fully_idle: true, turn_ended: true, step_under_way: true, latest_step_at: Some(ended), last_modified: ended, last_input: Some(ended - 60_000), ..Default::default() };
        let mut reader = Reader { processes: vec![Process { pid: agy, conversation: Some("c1".into()), terminal: true, logs: Vec::new(), started: None }], ..Reader::default() };
        reader.conversations.insert("antigravity".into(), vec![c.clone()]);
        let row = |pid: u32, ppid: u32, command: &str| PsLine { pid, ppid, tty: "??".into(), cpu: Some(10), command: command.into() };
        let seen = crate::state::SeenState::default();
        let look_at = |lines: Vec<PsLine>| Look { seen: &seen, since: 0, now, ps: Listing::of(std::sync::Arc::new(Listed { at: now, lines })) };
        // The newest process under the one that has it, and what its shell runs; nobody else's.
        let shell = vec![row(agy, 1, "agy -i Land the fix"), row(agy + 1, agy, "sleep 600"), row(agy + 2, agy, "/bin/zsh -c npm test && scripts/land.sh"), row(other + 1, other, "zsh -c gh run watch")];
        let look = look_at(shell);
        assert_eq!(reader.background_of(&c, &look), Some(Background { command: "npm test && scripts/land.sh".into(), active_at: None }));
        // A plain command, which the shell runs in its own place.
        let plain = look_at(vec![row(agy, 1, "agy -i Land the fix"), row(agy + 3, agy, "gh run watch 42")]);
        assert_eq!(reader.background_of(&c, &plain).map(|b| b.command), Some("gh run watch 42".into()));
        let s = &reader.sessions(&account, &look)[0];
        assert_eq!((s.status, s.excerpt.as_deref()), (Status::Working, Some("Waiting on `npm test && scripts/land.sh`")));
        // Once looked at, it's watched from when the turn ended.
        reader.watch_background(&look);
        assert_eq!(reader.background_of(&c, &look).and_then(|b| b.active_at), Some(ended));
        // Not while the turn goes on or a step waits on you, for a conversation no process has, nor
        // with nothing under the process that has it.
        assert_eq!(reader.background_of(&Conversation { turn_ended: false, ..c.clone() }, &look), None);
        assert_eq!(reader.background_of(&Conversation { waiting: true, ..c.clone() }, &look), None);
        assert_eq!(reader.background_of(&Conversation { id: "c2".into(), ..c.clone() }, &look), None);
        assert_eq!(reader.background_of(&c, &look_at(vec![row(agy, 1, "agy -i Land the fix")])), None);
    }
}
