//! What Claude Code knows of removing a worktree, for Codealong's own removal (cleanup.rs): the
//! repo's `WorktreeRemove` hooks, run as Claude Code runs them, and the background jobs whose
//! records name the worktree, which `claude rm <job>` drops once it's gone. Checked on 2.1.296
//! (2026-10-12) in a scratch repo; see .claude/rules/claude-code.md.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{json, Value};

use crate::cleanup::Hook;

/// How long a hook that names no `timeout` of its own runs before it's stopped, as Claude Code
/// gives a command hook.
const HOOK_TIMEOUT: Duration = Duration::from_secs(60);

/// The settings files of a project whose hooks Claude Code runs, in the project's folder.
const SETTINGS: [&str; 2] = [".claude/settings.json", ".claude/settings.local.json"];

/// The `WorktreeRemove` hooks a repo's main checkout declares, each run in it as Claude Code runs
/// one as it removes a worktree its `WorktreeCreate` hook made: through a shell, with
/// `CLAUDE_PROJECT_DIR` set to the checkout and the event on stdin. Both settings files count, and
/// `disableAllHooks` in either turns them all off. Only `command` hooks run outside Claude Code.
pub fn remove_hooks(main: &Path, worktree: &Path) -> Vec<Hook> {
    let settings: Vec<Value> = SETTINGS.iter().filter_map(|f| std::fs::read(main.join(f)).ok()).filter_map(|b| serde_json::from_slice(&b).ok()).collect();
    if settings.iter().any(|s| s.get("disableAllHooks").and_then(Value::as_bool) == Some(true)) {
        return Vec::new();
    }
    let input = json!({ "hook_event_name": "WorktreeRemove", "worktree_path": worktree, "cwd": main }).to_string();
    settings
        .iter()
        .flat_map(|s| s.pointer("/hooks/WorktreeRemove").and_then(Value::as_array).cloned().unwrap_or_default())
        .flat_map(|group| group.get("hooks").and_then(Value::as_array).cloned().unwrap_or_default())
        .filter(|h| h.get("type").and_then(Value::as_str).unwrap_or("command") == "command")
        .filter_map(|h| {
            let command = h.get("command").and_then(Value::as_str).filter(|c| !c.trim().is_empty())?.to_string();
            let timeout = h.get("timeout").and_then(Value::as_f64).filter(|t| *t > 0.0).map_or(HOOK_TIMEOUT, Duration::from_secs_f64);
            let env = vec![("CLAUDE_PROJECT_DIR".to_string(), main.to_string_lossy().into_owned())];
            Some(Hook { command, cwd: main.to_path_buf(), env, input: input.clone(), timeout })
        })
        .collect()
}

/// The background jobs whose records (`<home>/jobs/<job>/state.json`, kept after a job ends)
/// say they made or hold the worktree at `path` (`worktreePath`), by their short ids.
pub fn jobs_in(home: &Path, path: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(home.join("jobs")) else { return Vec::new() };
    let real = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let mut jobs: Vec<String> = entries
        .flatten()
        .filter(|e| {
            let Ok(bytes) = std::fs::read(e.path().join("state.json")) else { return false };
            let Some(held) = worktree_of(&bytes) else { return false };
            held == path || std::fs::canonicalize(&held).is_ok_and(|h| h == real)
        })
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|job| is_job_id(job))
        .collect();
    jobs.sort();
    jobs
}

/// The worktree a job's record says it holds.
fn worktree_of(bytes: &[u8]) -> Option<PathBuf> {
    let v: Value = serde_json::from_slice(bytes).ok()?;
    v.get("worktreePath").and_then(Value::as_str).filter(|p| !p.is_empty()).map(PathBuf::from)
}

/// A job's short id as Claude Code makes them, safe to put in a command line unquoted.
fn is_job_id(job: &str) -> bool {
    !job.is_empty() && job.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("codealong-teardown-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".claude")).unwrap();
        std::fs::canonicalize(&dir).unwrap()
    }

    #[test]
    fn reads_a_repos_worktree_remove_hooks_as_claude_code_runs_them() {
        let main = scratch("hooks");
        let wt = main.join(".claude/worktrees/brave-otter");
        assert!(remove_hooks(&main, &wt).is_empty());
        std::fs::write(
            main.join(".claude/settings.json"),
            r#"{"hooks":{"WorktreeCreate":[{"hooks":[{"type":"command","command":"make-it"}]}],"WorktreeRemove":[{"hooks":[{"type":"command","command":"\"$CLAUDE_PROJECT_DIR\"/scripts/remove.sh","timeout":120},{"type":"prompt","prompt":"say bye"}]}]}}"#,
        )
        .unwrap();
        std::fs::write(main.join(".claude/settings.local.json"), r#"{"hooks":{"WorktreeRemove":[{"matcher":"","hooks":[{"command":"echo local"}]}]}}"#).unwrap();
        let hooks = remove_hooks(&main, &wt);
        let commands: Vec<(&str, u64)> = hooks.iter().map(|h| (h.command.as_str(), h.timeout.as_secs())).collect();
        assert_eq!(commands, [("\"$CLAUDE_PROJECT_DIR\"/scripts/remove.sh", 120), ("echo local", 60)]);
        let h = &hooks[0];
        assert_eq!(h.cwd, main);
        assert_eq!(h.env, [("CLAUDE_PROJECT_DIR".to_string(), main.to_string_lossy().into_owned())]);
        let input: Value = serde_json::from_str(&h.input).unwrap();
        assert_eq!(input, json!({ "hook_event_name": "WorktreeRemove", "worktree_path": wt, "cwd": main }));
        // Turned off in either file, none run.
        std::fs::write(main.join(".claude/settings.local.json"), r#"{"disableAllHooks":true}"#).unwrap();
        assert!(remove_hooks(&main, &wt).is_empty());
        let _ = std::fs::remove_dir_all(&main);
    }

    #[test]
    fn finds_the_jobs_that_held_a_worktree() {
        let home = scratch("jobs");
        let wt = home.join("repo/.claude/worktrees/brave-otter");
        std::fs::create_dir_all(&wt).unwrap();
        let job = |id: &str, state: &str| {
            std::fs::create_dir_all(home.join("jobs").join(id)).unwrap();
            std::fs::write(home.join("jobs").join(id).join("state.json"), state).unwrap();
        };
        job("1df9ebbd", &json!({ "sessionId": "s1", "worktreePath": wt, "worktreeBranch": "worktree-brave-otter" }).to_string());
        job("0e342c51", &json!({ "sessionId": "s2", "worktreePath": home.join("repo/.claude/worktrees/other") }).to_string());
        job("0b82f9df", r#"{"sessionId":"s3","cwd":"/r"}"#);
        job("bad id", &json!({ "worktreePath": wt }).to_string());
        assert_eq!(jobs_in(&home, &wt), ["1df9ebbd"]);
        assert!(jobs_in(&home.join("nowhere"), &wt).is_empty());
        let _ = std::fs::remove_dir_all(&home);
    }
}
