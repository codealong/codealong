//! What Claude Code has built in: the skills its program holds rather than a folder (`/design`,
//! `/slides`, `/loop`, `/code-review`), which no file shows, so only Claude Code can list them.
//! Its commands (`/compact`, `/model`) aren't skills, and aren't listed (asked 2026-10-14).
//!
//! A program that runs it over stream-json gets two lists, neither of which spends usage (2.1.299
//! and 2.1.300, 2026-10-14). `control_request` `initialize` (the Agent SDK's `supportedCommands`),
//! answered before any prompt, has every command a session can be told to run by name, its
//! skills and its commands alike, with `name`, `description`, `argumentHint`, and `builtin: true`
//! for the ones its program holds; nothing there tells a skill from a command. The `system`/`init`
//! line has `skills` beside `slash_commands`, but comes only once a message is taken in. So a
//! message goes that's one of its own commands, `/color default`: it runs without the model
//! (`num_turns: 0`, `duration_api_ms: 0`, `total_cost_usd: 0`) and changes nothing that's kept,
//! the run being one that writes no transcript. It goes only when `initialize` lists `color`: any
//! other text would be a prompt, and `--max-turns 0` doesn't stop one from being answered. What's
//! listed is what `initialize` describes and `init` calls a skill. The rest of `initialize`'s
//! answer (the account's own skills, its plugins', the ones claude.ai syncs) Codealong reads from
//! their folders (claude/skills.rs), fresher than a list kept here.
//!
//! The lists are a headless run's, which isn't quite a terminal session's. A terminal and the
//! Claude app have Artifacts, and a headless run doesn't: it leaves out `/slides` and
//! `/artifact-diagramming`, and has `/design` as the command that grants Claude access to your
//! Design projects rather than the skill that makes a Design (a terminal's `/` menu says "Make a
//! new Design artifact from a brief"). Told it's the Claude app (`CLAUDE_CODE_ENTRYPOINT=
//! claude-desktop`), it lists them as a session has them, and nothing else changed between the
//! two lists. The run loads no MCP server (their prompts aren't skills, and some start
//! processes) and runs no hook. While it runs it lists itself among the live sessions
//! (`sessions/<pid>.json`, `entrypoint: claude-desktop`, `kind: interactive`), so it runs in a
//! folder of its own (`FOLDER`), which claude/registry.rs passes over; it takes its record away
//! as it ends once its input closes, and a killed one leaves it behind.
//!
//! Asking takes a second or more (Claude Code starts, reads its settings and plugins, and asks
//! claude.ai what the account may use), so each account's list is kept, read again in the
//! background once it's an hour old, and read as Codealong starts (skills.rs `warm`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
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
    skills: Option<Vec<Builtin>>,
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
            list.skills = read;
        }
    }

    /// Whether a read of a list there's none of yet is out, which is worth waiting for.
    fn awaited(&self, home: &Path) -> bool {
        self.by_home.get(home).is_some_and(|l| l.reading && l.skills.is_none())
    }

    fn kept(&self, home: &Path) -> Vec<Builtin> {
        self.by_home.get(home).and_then(|l| l.skills.clone()).unwrap_or_default()
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

/// The folder the lists are asked in: one of no project, so no project's settings or skills come
/// into them, and Codealong's own, so the run's record among the live sessions is known for what
/// it is (`runs_in`).
static FOLDER: LazyLock<PathBuf> = LazyLock::new(|| {
    let dir = std::env::temp_dir().join("codealong-claude-skills");
    let _ = std::fs::create_dir_all(&dir);
    std::fs::canonicalize(&dir).unwrap_or(dir)
});

/// Whether a live session in `cwd` is the run that lists the skills, which is no session.
pub fn runs_in(cwd: &str) -> bool {
    Path::new(cwd) == FOLDER.as_path()
}

/// The command Codealong sends to have the `init` line written: one that runs without the model.
const COLOR: &str = "color";
/// What it sends for it, which puts back the color a session starts with.
const SAID: &str = "/color default";

/// How long the run is given to leave by itself once its input closes, taking its record with it.
const LEAVE_WITHIN: Duration = Duration::from_secs(5);

/// Asks the account's Claude Code for its skills. None when it can't be run, doesn't answer, or
/// wouldn't take `/color` as its own; the lists go on without them, as they did before Codealong
/// asked, so nobody is told.
fn read(home: &Path) -> Option<Vec<Builtin>> {
    let mut cmd = crate::procs::command("claude", crate::procs::user_path());
    cmd.args(ARGS).current_dir(FOLDER.as_path()).env("CLAUDE_CODE_ENTRYPOINT", "claude-desktop");
    let (env, unset) = super::headless_env(home);
    for (k, v) in env {
        cmd.env(k, v);
    }
    for k in unset {
        cmd.env_remove(k);
    }
    let (tx, rx) = channel();
    let client = rpc::Client::spawn(cmd, rpc::Stderr::Tail, rpc::Wire::Bare, move |_, heard| match heard {
        rpc::Heard::Line(line) if (line["type"] == "control_response" && line["response"]["request_id"] == ASK) || (line["type"] == "system" && line["subtype"] == "init") => {
            let _ = tx.send(Some(line.clone()));
        }
        rpc::Heard::Line(_) => {}
        rpc::Heard::Closed(_) => {
            let _ = tx.send(None);
        }
    })
    .ok()?;
    let listed = (|| {
        client.send(&json!({ "type": "control_request", "request_id": ASK, "request": { "subtype": "initialize" } })).ok()?;
        let answer = rx.recv_timeout(ANSWER_WITHIN).ok()??;
        let commands = answer["response"]["response"]["commands"].as_array()?;
        if !commands.iter().any(|c| c["name"] == COLOR && c["builtin"] == true) {
            return None;
        }
        client.send(&json!({ "type": "user", "message": { "role": "user", "content": SAID } })).ok()?;
        let init = rx.recv_timeout(ANSWER_WITHIN).ok()??;
        skills_in(&answer, &init)
    })();
    // Its input closed, it leaves by itself, and takes its record among the live sessions away.
    client.end(LEAVE_WITHIN);
    listed
}

/// The skills Claude Code's program holds: the commands its answer to `initialize` describes as
/// built in that its `init` line calls skills. None for lines that aren't those.
fn skills_in(answer: &Value, init: &Value) -> Option<Vec<Builtin>> {
    let commands = answer["response"]["response"]["commands"].as_array()?;
    let skills: Vec<&str> = init["skills"].as_array()?.iter().filter_map(Value::as_str).collect();
    let text = |c: &Value, k: &str| c[k].as_str().unwrap_or_default().to_string();
    Some(
        commands
            .iter()
            .filter(|c| c["builtin"] == true && c["name"].as_str().is_some_and(|n| skills.contains(&n)))
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
    fn only_the_skills_its_program_holds_are_taken() {
        // As 2.1.300 answers, cut short: two bundled skills, two commands, a plugin's skill and a
        // synced one.
        let answer = json!({ "type": "control_response", "response": { "subtype": "success", "request_id": ASK, "response": { "commands": [
            { "name": "design", "description": "Make a new Design artifact from a brief", "argumentHint": "[what to design]", "builtin": true },
            { "name": "loop", "description": "Run a prompt or slash command on a recurring interval", "argumentHint": "[interval] [prompt]", "builtin": true },
            { "name": "compact", "description": "Free up context by summarizing the conversation so far", "argumentHint": "", "builtin": true },
            { "name": "color", "description": "Set the prompt bar color for this session", "argumentHint": "[red|blue|green|yellow|purple|orange|pink|cyan|default]", "builtin": true },
            { "name": "figma:figma-use", "description": "(figma) Use Figma", "argumentHint": "" },
            { "name": "pdf", "description": "PDF files", "argumentHint": "", "aliases": ["anthropic-skills:pdf"] },
        ], "models": [] } } });
        let init = json!({ "type": "system", "subtype": "init", "slash_commands": ["design", "loop", "compact", "color", "figma:figma-use", "pdf"], "skills": ["figma:figma-use", "design", "loop", "anthropic-skills:pdf"] });
        assert_eq!(
            skills_in(&answer, &init).unwrap(),
            vec![
                Builtin { name: "design".into(), description: "Make a new Design artifact from a brief".into(), hint: Some("[what to design]".into()) },
                Builtin { name: "loop".into(), description: "Run a prompt or slash command on a recurring interval".into(), hint: Some("[interval] [prompt]".into()) },
            ]
        );
        // An error, or a line that isn't `init`, is no list.
        assert_eq!(skills_in(&json!({ "type": "control_response", "response": { "subtype": "error", "error": "nope" } }), &init), None);
        assert_eq!(skills_in(&answer, &json!({ "type": "system", "subtype": "init" })), None);
    }

    #[test]
    fn its_own_run_is_known_by_its_folder() {
        assert!(runs_in(&FOLDER.to_string_lossy()));
        assert!(!runs_in("/Users/me/repo"));
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
