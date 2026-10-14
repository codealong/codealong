//! What Claude Code has built in: the skills and commands its program holds rather than a
//! folder (`/design`, `/slides`, `/loop`, `/code-review`, `/compact`), which no file shows, so
//! only Claude Code can list them. It lists them to a program that runs it over stream-json and
//! asks (`control_request` `initialize`, the Agent SDK's `supportedCommands`), before any prompt,
//! so asking spends no usage: each command a session can be told to run by name, with `name`,
//! `description`, `argumentHint`, and `builtin: true` for the ones its program holds (2.1.299,
//! 2026-10-14). Those are its skills and its commands alike, and nothing in the answer tells them
//! apart, so both are listed, as Claude Code's own `/` menu lists both. The rest of the answer
//! (the account's own skills, its plugins', the ones claude.ai syncs) Codealong reads from their
//! folders (claude/skills.rs), fresher than a list kept here.
//!
//! The list is a headless run's, which isn't quite a terminal session's. A terminal and the
//! Claude app have Artifacts, and a headless run doesn't: it leaves out `/slides` and
//! `/artifact-diagramming`, and has `/design` as the command that grants Claude access to your
//! Design projects rather than the skill that makes a Design (a terminal's `/` menu says "Make a
//! new Design artifact from a brief"). Told it's the Claude app (`CLAUDE_CODE_ENTRYPOINT=
//! claude-desktop`), it lists them as a session has them, and nothing else changed between the
//! two lists. The run loads no MCP server (their prompts aren't skills, and some start
//! processes), runs no hook, and writes no transcript. A terminal's menu also hides a few that
//! the answer has (`heapdump`, `extra-usage`) and has a few of its own (`/help`, `/workflows`).
//!
//! Asking takes a second or more (Claude Code starts, reads its settings and plugins, and asks
//! claude.ai what the account may use), so each account's list is kept, read again in the
//! background once it's an hour old, and read as Codealong starts (skills.rs `warm`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::channel;
use std::sync::{Condvar, LazyLock, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::rpc;
use crate::skills::Builtin;
use crate::throttle;

/// How long a list is used before it's read again: Claude Code updates itself, and claude.ai
/// changes what an account may use.
const FRESH: Duration = Duration::from_secs(60 * 60);
/// How long after a list couldn't be read it's tried again: Claude Code may not be installed.
const RETRY: Duration = Duration::from_secs(10 * 60);
/// How long Claude Code is given to answer.
const ANSWER_WITHIN: Duration = Duration::from_secs(30);
/// The id Codealong's request goes by.
const ASK: &str = "codealong-commands";

/// How it's run: headless over stream-json, as the Claude app runs it, with nothing it would
/// start besides itself.
const ARGS: &[&str] = &["-p", "--input-format", "stream-json", "--output-format", "stream-json", "--verbose", "--no-session-persistence", "--strict-mcp-config", "--mcp-config", r#"{"mcpServers":{}}"#, "--settings", r#"{"disableAllHooks":true}"#];

/// The lists read, by the account's folder.
#[derive(Default)]
struct Lists {
    by_home: HashMap<PathBuf, List>,
}

#[derive(Default)]
struct List {
    commands: Option<Vec<Builtin>>,
    /// When it was last read, or last failed to be.
    read_at: Option<Instant>,
    failed: bool,
    reading: bool,
}

impl Lists {
    /// Whether `home`'s list is to be read now, marked under way when it is: one never read, one
    /// an hour old, one that failed ten minutes ago; never two reads at once.
    fn due(&mut self, home: &Path, now: Instant) -> bool {
        let list = self.by_home.entry(home.to_path_buf()).or_default();
        let due = !list.reading && list.read_at.is_none_or(|at| now.duration_since(at) >= if list.failed { RETRY } else { FRESH });
        list.reading |= due;
        due
    }

    /// A read is over: what it read, or None. A failed read keeps the last list.
    fn on_read(&mut self, home: &Path, now: Instant, read: Option<Vec<Builtin>>) {
        let list = self.by_home.entry(home.to_path_buf()).or_default();
        list.reading = false;
        list.read_at = Some(now);
        list.failed = read.is_none();
        if read.is_some() {
            list.commands = read;
        }
    }

    /// Whether a read of a list there's none of yet is out, which is worth waiting for.
    fn awaited(&self, home: &Path) -> bool {
        self.by_home.get(home).is_some_and(|l| l.reading && l.commands.is_none())
    }

    fn kept(&self, home: &Path) -> Vec<Builtin> {
        self.by_home.get(home).and_then(|l| l.commands.clone()).unwrap_or_default()
    }
}

static LISTS: LazyLock<(Mutex<Lists>, Condvar)> = LazyLock::new(Default::default);

fn lock() -> MutexGuard<'static, Lists> {
    LISTS.0.lock().unwrap_or_else(|e| e.into_inner())
}

/// What the account in `home` has built in, as last read; read in the background when it's due,
/// and waited for up to `wait` when there's none yet.
pub fn listed(home: &Path, wait: Duration) -> Vec<Builtin> {
    let due = lock().due(home, Instant::now());
    if due {
        // Started with the lock let go: `ended` runs on this thread when the work's can't start.
        let (at, back) = (home.to_path_buf(), home.to_path_buf());
        throttle::background("codealong-claude-commands", move || read(&at), move |read| {
            lock().on_read(&back, Instant::now(), read.flatten());
            LISTS.1.notify_all();
        });
    }
    let until = Instant::now() + wait;
    let mut lists = lock();
    while lists.awaited(home) {
        let now = Instant::now();
        if now >= until {
            break;
        }
        lists = LISTS.1.wait_timeout(lists, until - now).unwrap_or_else(|e| e.into_inner()).0;
    }
    lists.kept(home)
}

/// Asks the account's Claude Code for its commands. None when it can't be run or doesn't answer;
/// the lists go on without them, as they did before Codealong asked, so nobody is told.
fn read(home: &Path) -> Option<Vec<Builtin>> {
    let mut cmd = Command::new("claude");
    // A folder of no project, so no project's settings or skills come into it.
    cmd.args(ARGS).current_dir(std::env::temp_dir()).env("PATH", crate::procs::user_path()).env("CLAUDE_CODE_ENTRYPOINT", "claude-desktop");
    let (env, unset) = super::headless_env(home);
    for (k, v) in env {
        cmd.env(k, v);
    }
    for k in unset {
        cmd.env_remove(k);
    }
    let (tx, rx) = channel();
    let client = rpc::Client::spawn(cmd, rpc::Stderr::Tail, rpc::Wire::Bare, move |_, heard| match heard {
        rpc::Heard::Line(line) if line["type"] == "control_response" && line["response"]["request_id"] == ASK => {
            let _ = tx.send(Some(line.clone()));
        }
        rpc::Heard::Line(_) => {}
        rpc::Heard::Closed(_) => {
            let _ = tx.send(None);
        }
    })
    .ok()?;
    client.send(&json!({ "type": "control_request", "request_id": ASK, "request": { "subtype": "initialize" } })).ok()?;
    let answer = rx.recv_timeout(ANSWER_WITHIN).ok()??;
    // Dropping the client ends the process: it has nothing more to do.
    drop(client);
    builtins_in(&answer)
}

/// The commands Claude Code's program holds, from its answer to `initialize`. None for an answer
/// that isn't one.
fn builtins_in(answer: &Value) -> Option<Vec<Builtin>> {
    let commands = answer["response"]["response"]["commands"].as_array()?;
    let text = |c: &Value, k: &str| c[k].as_str().unwrap_or_default().to_string();
    Some(
        commands
            .iter()
            .filter(|c| c["builtin"] == true)
            .map(|c| Builtin { name: text(c, "name"), description: text(c, "description"), hint: Some(text(c, "argumentHint")).filter(|h| !h.is_empty()) })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn builtin(name: &str) -> Builtin {
        Builtin { name: name.into(), description: String::new(), hint: None }
    }

    #[test]
    fn only_what_its_program_holds_is_taken_from_the_answer() {
        // As 2.1.299 answers, cut short: a bundled skill, a command, a plugin's skill and a
        // synced one.
        let answer = json!({ "type": "control_response", "response": { "subtype": "success", "request_id": ASK, "response": { "commands": [
            { "name": "design", "description": "Make a new Design artifact from a brief", "argumentHint": "[what to design]", "builtin": true },
            { "name": "compact", "description": "Free up context by summarizing the conversation so far", "argumentHint": "", "builtin": true },
            { "name": "figma:figma-use", "description": "(figma) Use Figma", "argumentHint": "" },
            { "name": "pdf", "description": "PDF files", "argumentHint": "", "aliases": ["anthropic-skills:pdf"] },
        ], "models": [] } } });
        let got = builtins_in(&answer).unwrap();
        assert_eq!(
            got,
            vec![
                Builtin { name: "design".into(), description: "Make a new Design artifact from a brief".into(), hint: Some("[what to design]".into()) },
                Builtin { name: "compact".into(), description: "Free up context by summarizing the conversation so far".into(), hint: None },
            ]
        );
        // An error, or anything else, is no list.
        assert_eq!(builtins_in(&json!({ "type": "control_response", "response": { "subtype": "error", "error": "nope" } })), None);
    }

    #[test]
    fn a_list_is_read_when_due_and_kept_between_reads() {
        let home = Path::new("/home/.claude");
        let t0 = Instant::now();
        let mut lists = Lists::default();
        assert!(lists.due(home, t0));
        // One read at a time, waited for while there's no list yet.
        assert!(!lists.due(home, t0));
        assert!(lists.awaited(home));
        lists.on_read(home, t0, Some(vec![builtin("loop")]));
        assert!(!lists.awaited(home));
        assert_eq!(lists.kept(home), [builtin("loop")]);
        // Kept for an hour, then read again in the background, the old list used meanwhile.
        assert!(!lists.due(home, t0 + FRESH - Duration::from_secs(1)));
        assert!(lists.due(home, t0 + FRESH));
        assert!(!lists.awaited(home));
        // A failed read keeps the list, and is tried again sooner.
        let t1 = t0 + FRESH;
        lists.on_read(home, t1, None);
        assert_eq!(lists.kept(home), [builtin("loop")]);
        assert!(!lists.due(home, t1 + RETRY - Duration::from_secs(1)));
        assert!(lists.due(home, t1 + RETRY));
        // Another account's is its own.
        assert!(lists.kept(Path::new("/home/.claude2")).is_empty());
    }
}
