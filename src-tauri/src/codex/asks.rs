//! Codex's approval prompts, as its `PermissionRequest` hook hands them over (0.162.1): Codex
//! runs the hook as a call needs approving, before its reviewer agent or its own prompt has it,
//! with the thread, the turn, the tool and what it would run, and goes on to them when the hook
//! prints nothing. Codealong's hook (`codealong session hook codex-permission-request`, which
//! hook.rs puts in an account's `hooks.json`) writes that down here as a note and returns at
//! once, so the prompt comes up where the thread runs as it always does. Codex records no
//! approval in its rollout, so a note is how Codealong knows one is up: while its turn is open
//! and the call it's about has no result (`waiting`), where without one it could only guess from
//! a call left without its result for minutes (board.rs `codex_unanswered`).
//!
//! The notes are files in Codealong's own folder, one a request, written by the hook whether the
//! app is up or not, and read by the reader as the folder changes (`Notes`). A call Codex's
//! terminal UI runs as JavaScript (its `exec` tool) can run several commands, each asking on its
//! own, and stays without its result while an allowed one runs, for as long as the command takes
//! to come back (`exec_command`'s yield, ten seconds or so): a note allowed in the terminal reads
//! as waiting until then. One answered from Codealong goes at once.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::rollout::{PendingCall, ThreadInfo};
use crate::model::ModeKind;
use crate::util::one_line;

/// Codealong's folder of notes.
pub fn dir() -> PathBuf {
    crate::config::support_dir().join("codex-asks")
}

/// A note this old is gone whatever its turn: Codex never keeps a turn open that long.
const KEEP_MS: i64 = 24 * 60 * 60 * 1000;

/// A request the hook was handed, as Codex gave it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Note {
    /// The thread that asks (`session_id`), and its turn (`turn_id`).
    pub thread: String,
    pub turn: String,
    /// The tool as hooks name it: `Bash` for a command (or its use of the network), `apply_patch`
    /// for an edit, an MCP server's tool as `mcp__<server>__<tool>`.
    pub tool: String,
    /// What it would run: a command's `command` and `description` (the reason the agent gave),
    /// a patch's `command`, a tool's arguments.
    pub input: Value,
    /// When the hook ran.
    pub at: i64,
    /// Its file, which an answer takes away.
    #[serde(skip)]
    pub file: PathBuf,
}

impl Note {
    /// The command or patch, where there's one.
    pub fn command(&self) -> Option<&str> {
        self.input.get("command").and_then(Value::as_str).map(str::trim).filter(|c| !c.is_empty())
    }

    /// Why the agent asks, in its own words.
    pub fn reason(&self) -> Option<&str> {
        self.input.get("description").and_then(Value::as_str).map(str::trim).filter(|c| !c.is_empty())
    }

    /// What it would do, for the session's line: "`touch ~/notes`", "Edit files",
    /// "github · create_issue".
    pub fn what(&self) -> String {
        match (self.tool.as_str(), self.command()) {
            ("Bash", Some(c)) => format!("`{}`", one_line(c, 80)),
            ("apply_patch", _) => "Edit files".to_string(),
            (tool, _) => match tool.strip_prefix("mcp__").and_then(|t| t.split_once("__")) {
                Some((server, name)) => format!("{server} · {name}"),
                None => tool.to_string(),
            },
        }
    }

    /// The request as a line to allow or deny: "Run `touch ~/notes`", "Edit files".
    pub fn label(&self) -> String {
        match self.tool.as_str() {
            "Bash" if self.command().is_some() => format!("Run {}", self.what()),
            _ => self.what(),
        }
    }

    /// All of it, to read before answering: a command with the agent's reason above it as a
    /// comment, as Codex's own prompt shows it beside the command; a patch; else the arguments.
    pub fn whole(&self) -> String {
        match (self.tool.as_str(), self.command(), self.reason()) {
            ("Bash", Some(c), Some(why)) => format!("# {}\n{c}", one_line(why, 300)),
            (_, Some(c), _) => c.to_string(),
            _ => serde_json::to_string_pretty(&self.input).unwrap_or_default(),
        }
    }

    /// Its id, which an answer names: its file's name.
    pub fn id(&self) -> String {
        self.file.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
    }

    /// Whether Codex's terminal UI asks it with Yes first (`y`) and Escape to say no: a command
    /// (`exec_options`) and an edit (`patch_options`), 0.162.1. An MCP tool's and a request for
    /// more permissions are asked in prompts of their own.
    pub fn keyed(&self) -> bool {
        matches!(self.tool.as_str(), "Bash" | "apply_patch")
    }
}

/// The hook's input as a note, or None for one that isn't a request of a thread's.
pub fn from_hook(hook: &Value, at: i64) -> Option<Note> {
    let s = |k: &str| hook.get(k).and_then(Value::as_str).map(str::trim).filter(|v| !v.is_empty()).map(String::from);
    Some(Note { thread: s("session_id")?, turn: s("turn_id")?, tool: s("tool_name")?, input: hook.get("tool_input").cloned().unwrap_or(Value::Null), at, file: PathBuf::new() })
}

/// Writes the note for a hook's input into `dir`, whole or not at all: the hook does this and
/// nothing else, and prints nothing, so Codex goes on to its prompt.
pub fn write(dir: &Path, note: &Note) -> anyhow::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let name = format!("{}-{}-{}.json", note.thread, note.at, std::process::id());
    let file = dir.join(&name);
    let tmp = dir.join(format!(".{name}"));
    std::fs::write(&tmp, serde_json::to_vec(note)?)?;
    std::fs::rename(&tmp, &file)?;
    Ok(file)
}

/// The notes in `dir`, and the ones older than a day taken away.
pub fn read(dir: &Path, now: i64) -> Vec<Note> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut out = Vec::new();
    for path in entries.flatten().map(|e| e.path()) {
        if path.extension().and_then(|x| x.to_str()) != Some("json") || path.file_name().is_some_and(|n| n.to_string_lossy().starts_with('.')) {
            continue;
        }
        let note = std::fs::read(&path).ok().and_then(|b| serde_json::from_slice::<Note>(&b).ok());
        match note {
            Some(n) if now - n.at < KEEP_MS => out.push(Note { file: path, ..n }),
            // Old, or not one of these: of no use to anyone.
            _ => {
                let _ = std::fs::remove_file(&path);
            }
        }
    }
    out.sort_by_key(|n| n.at);
    out
}

/// Takes a note away once it's answered from here.
pub fn done(note: &Note) {
    // Gone already: the reader took it away with its turn, or another answer did.
    let _ = std::fs::remove_file(&note.file);
}

/// The note a thread waits on you for, the oldest still up, as its terminal UI asks them in
/// turn: one of its open turn whose call has no result yet. None while a reviewer agent answers
/// for you (Approve for me), which Codex asks before its own prompt.
pub fn waiting<'a>(th: &ThreadInfo, notes: &'a [Note]) -> Option<&'a Note> {
    if !th.turn_open || th.mode.as_ref().is_some_and(|m| m.kind == ModeKind::Auto) {
        return None;
    }
    notes.iter().filter(|n| n.thread == th.id && th.turn_id.as_deref() == Some(n.turn.as_str())).find(|n| asked_about(&th.pending_calls, n).is_some())
}

/// A call is written before Codex asks about it, and the hook's time is taken after; this much
/// later still counts, for the two reading the clock a moment apart.
const SLACK_MS: i64 = 200;

/// The call a note asks about: of those without a result written before it, the one whose input
/// has its command, as it is or escaped as a string (a JavaScript `exec` holds it in a string
/// literal, a `function_call` in its JSON arguments), else the latest.
fn asked_about<'a>(open: &'a [PendingCall], note: &Note) -> Option<&'a PendingCall> {
    let before = || open.iter().filter(|c| c.at <= note.at + SLACK_MS);
    let has = |c: &PendingCall, cmd: &str| c.input.contains(cmd) || serde_json::to_string(cmd).is_ok_and(|q| c.input.contains(q.trim_matches('"')));
    note.command().and_then(|cmd| before().find(|c| has(c, cmd))).or_else(|| before().max_by_key(|c| c.at))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::SessionMode;
    use serde_json::json;

    /// What 0.162.1 handed the hook for a command it asked about (in tmux, 2026-10-24).
    fn hooked() -> Value {
        json!({
            "session_id": "01a16e99-a812-7591-9707-1f6a541c1e15",
            "turn_id": "01a16e9e-f8a7-7d20-8c6c-8988b813d707",
            "transcript_path": "/Users/me/.codex/sessions/2026/10/24/rollout-2026-10-24T08-48-46-01a16e99-a812-7591-9707-1f6a541c1e15.jsonl",
            "cwd": "/Users/me/projects/northwind",
            "hook_event_name": "PermissionRequest",
            "model": "gpt-6-luna",
            "permission_mode": "default",
            "tool_name": "Bash",
            "tool_input": { "command": "touch ~/.northwind-probe", "description": "May I create the probe in your home folder?" }
        })
    }

    fn call(id: &str, at: i64, input: &str) -> PendingCall {
        PendingCall { call_id: id.into(), at, what: "exec".into(), limit_ms: 4 * 60 * 1000, input: input.into() }
    }

    fn open_turn(calls: Vec<PendingCall>) -> ThreadInfo {
        ThreadInfo { id: "01a16e99-a812-7591-9707-1f6a541c1e15".into(), turn_open: true, turn_id: Some("01a16e9e-f8a7-7d20-8c6c-8988b813d707".into()), pending_calls: calls, ..ThreadInfo::default() }
    }

    #[test]
    fn a_hooks_input_is_a_note_and_says_what_it_asks() {
        let n = from_hook(&hooked(), 5_000).unwrap();
        assert_eq!((n.tool.as_str(), n.command(), n.reason()), ("Bash", Some("touch ~/.northwind-probe"), Some("May I create the probe in your home folder?")));
        assert_eq!((n.what(), n.label()), ("`touch ~/.northwind-probe`".to_string(), "Run `touch ~/.northwind-probe`".to_string()));
        assert_eq!(n.whole(), "# May I create the probe in your home folder?\ntouch ~/.northwind-probe");
        assert!(n.keyed());
        let mcp = Note { tool: "mcp__github__create_issue".into(), input: json!({ "title": "x" }), ..n.clone() };
        assert_eq!(mcp.label(), "github · create_issue");
        assert!(!mcp.keyed());
        assert_eq!(from_hook(&json!({ "tool_name": "Bash" }), 5_000), None, "no thread");
    }

    #[test]
    fn notes_are_written_whole_and_old_ones_go() {
        let dir = crate::fixtures::scratch("codex-asks");
        let n = from_hook(&hooked(), 5_000).unwrap();
        let file = write(&dir, &n).unwrap();
        std::fs::write(dir.join("stray.json"), "{").unwrap();
        let read_back = read(&dir, 6_000);
        assert_eq!(read_back, vec![Note { file: file.clone(), ..n.clone() }]);
        assert!(!dir.join("stray.json").exists(), "what isn't a note goes");
        assert!(read(&dir, 5_000 + KEEP_MS).is_empty());
        assert!(!file.exists());
    }

    #[test]
    fn a_note_waits_while_its_call_has_no_result() {
        let n = from_hook(&hooked(), 10_000).unwrap();
        let js = r#"const r = await tools.exec_command({cmd:"touch ~/.northwind-probe", sandbox_permissions:"require_escalated"});"#;
        let notes = vec![n.clone()];
        assert_eq!(waiting(&open_turn(vec![call("c1", 9_500, js)]), &notes), Some(&n));
        // Answered: the call has its result, or the turn ended or moved on.
        assert_eq!(waiting(&open_turn(Vec::new()), &notes), None);
        assert_eq!(waiting(&ThreadInfo { turn_open: false, ..open_turn(vec![call("c1", 9_500, js)]) }, &notes), None);
        assert_eq!(waiting(&ThreadInfo { turn_id: Some("next".into()), ..open_turn(vec![call("c1", 9_500, js)]) }, &notes), None);
        // A call made after it isn't the one it asks about.
        assert_eq!(waiting(&open_turn(vec![call("c2", 12_000, "ls")]), &notes), None);
        // Its command is found escaped too, as a `function_call`'s arguments hold one with quotes.
        let quoted = Note { input: json!({ "command": r#"npm run build -- --name "north wind""# }), ..n.clone() };
        let args = r#"{"cmd":"npm run build -- --name \"north wind\"","sandbox_permissions":"require_escalated"}"#;
        let beside = vec![call("c4", 9_600, args), call("c5", 9_900, "sleep 30")];
        assert_eq!(asked_about(&beside, &quoted).map(|c| c.call_id.as_str()), Some("c4"));
        // Another call still running beside it doesn't stand in for it once its own is done.
        let beside = vec![call("c0", 8_000, "sleep 30"), call("c1", 9_500, js)];
        assert_eq!(asked_about(&beside, &n).map(|c| c.call_id.as_str()), Some("c1"));
        // A reviewer agent answers for you in Approve for me.
        let reviewed = ThreadInfo { mode: Some(SessionMode::new(ModeKind::Auto, "Approve for me", "")), ..open_turn(vec![call("c1", 9_500, js)]) };
        assert_eq!(waiting(&reviewed, &notes), None);
        // The oldest of two up is asked first.
        let later = Note { at: 11_000, input: json!({ "command": "rm ~/.northwind-probe" }), ..n.clone() };
        let both = open_turn(vec![call("c1", 9_500, js), call("c3", 10_500, "rm ~/.northwind-probe")]);
        assert_eq!(waiting(&both, &[n.clone(), later.clone()]), Some(&n));
        assert_eq!(waiting(&open_turn(vec![call("c3", 10_500, "rm ~/.northwind-probe")]), &[n, later.clone()]), Some(&later));
    }
}
