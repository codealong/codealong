//! Antigravity's Agent Client Protocol (`agy acp`) launch and permission rules.
//!
//! Kept beside `antigravity.rs` so the main adapter stays within its file-length budget.

use std::path::Path;

use serde_json::Value;

use crate::config::AccountCfg;

/// `agy acp` is built in; `--process-per-session` gives each session its own worker process so
/// workspace roots, rules and `.agents/hooks.json` don't bleed across sessions, and its log goes
/// into Codealong's logs folder rather than `~/.gemini/antigravity-cli/log`.
pub(super) fn command(account: &AccountCfg) -> std::result::Result<(String, Vec<(String, String)>), String> {
    let log = crate::config::logs_dir().join(format!("agy-acp-{}.log", account.name));
    Ok((format!("{} acp --process-per-session --log-file {}", super::cli_command(account), crate::util::sh_quote(&log.to_string_lossy())), Vec::new()))
}

/// The command line's `settings.json`, when there's one that reads.
fn settings(home: &Path) -> Option<Value> {
    serde_json::from_str(&std::fs::read_to_string(super::cli_settings(home)?).ok()?).ok()
}

/// Whether the command line's `settings.json` puts its default mode in `always-proceed` with no
/// saved non-default `agentMode` overriding it.
fn skips_in_default_mode(home: &Path) -> bool {
    let Some(v) = settings(home) else { return false };
    let perm = v.get("toolPermission").and_then(Value::as_str) == Some("always-proceed");
    let agent_mode = v.get("agentMode").and_then(Value::as_str).unwrap_or("");
    perm && (agent_mode.is_empty() || agent_mode == "default")
}

/// `agy`'s terminal UI auto-approves waiting tool confirmations (including a `PreToolUse` hook's
/// `force_ask`) when `settings.json` has `toolPermission: "always-proceed"` and the session is in
/// its default mode (`tool_confirmation_manager.go`). `agy acp` forwards them as
/// `session/request_permission` even with `--dangerously-skip-permissions`, so Codealong matches
/// the terminal UI here. Plans (`is_plan`) are filtered before this, and `ask_question` is never
/// auto-approved.
pub(super) fn auto_allows(home: &Path, mode: Option<&str>, call: &Value) -> bool {
    mode == Some("default") && call.get("name").and_then(Value::as_str) != Some("ask_question") && skips_in_default_mode(home)
}

/// The agent mode the command line's `settings.json` starts its terminal UI in (`agentMode`:
/// `accept-edits`, `auto` or `plan`, which `/config` writes and its default removes), for the
/// sessions Codealong runs: `agy acp`'s `session/new` starts each in `default` whatever the
/// settings say (1.5.14, 2026-11-22), and in `default` again as `session/resume` picks one up.
/// None when they say none, or the default.
pub(super) fn start_mode(home: &Path) -> Option<String> {
    let mode = settings(home)?.get("agentMode")?.as_str()?.trim().to_string();
    (!mode.is_empty() && mode != "default").then_some(mode)
}

#[cfg(test)]
mod tests {
    use super::super::{Antigravity, APP_DIR, CLI_DIR};
    use crate::harness::Adapter;

    #[test]
    fn acp_auto_allows_matches_terminal_always_proceed_in_default_mode() {
        let root = crate::fixtures::scratch("agy-acp-auto-allow");
        let app_home = root.join(APP_DIR);
        let cli_home = root.join(CLI_DIR);
        std::fs::create_dir_all(&app_home).unwrap();
        std::fs::create_dir_all(&cli_home).unwrap();
        let settings = cli_home.join("settings.json");
        let cmd_call = serde_json::json!({ "name": "run_command", "kind": "execute" });
        let ask_call = serde_json::json!({ "name": "ask_question" });

        // Missing or non-always-proceed settings never auto-allow.
        assert!(!Antigravity.acp_auto_allows(&app_home, Some("default"), &cmd_call));
        for perm in ["request-review", "strict", "proceed-in-sandbox"] {
            std::fs::write(&settings, serde_json::json!({ "toolPermission": perm }).to_string()).unwrap();
            assert!(!Antigravity.acp_auto_allows(&app_home, Some("default"), &cmd_call), "{perm}");
        }
        std::fs::write(&settings, "{not json").unwrap();
        assert!(!Antigravity.acp_auto_allows(&app_home, Some("default"), &cmd_call));

        // always-proceed in default mode auto-allows tool calls, for both app and CLI homes.
        std::fs::write(&settings, serde_json::json!({ "toolPermission": "always-proceed" }).to_string()).unwrap();
        assert!(Antigravity.acp_auto_allows(&app_home, Some("default"), &cmd_call));
        assert!(Antigravity.acp_auto_allows(&cli_home, Some("default"), &cmd_call));
        // Non-default modes and ask_question stay manual.
        for mode in [None, Some("accept-edits"), Some("auto"), Some("plan")] {
            assert!(!Antigravity.acp_auto_allows(&app_home, mode, &cmd_call), "{mode:?}");
        }
        assert!(!Antigravity.acp_auto_allows(&app_home, Some("default"), &ask_call));

        // A saved non-default agentMode overrides always-proceed, while "default" keeps it.
        std::fs::write(&settings, serde_json::json!({ "toolPermission": "always-proceed", "agentMode": "accept-edits" }).to_string()).unwrap();
        assert!(!Antigravity.acp_auto_allows(&app_home, Some("default"), &cmd_call));
        std::fs::write(&settings, serde_json::json!({ "toolPermission": "always-proceed", "agentMode": "default" }).to_string()).unwrap();
        assert!(Antigravity.acp_auto_allows(&app_home, Some("default"), &cmd_call));
    }

    /// A session Codealong runs starts in the agent mode the terminal UI would, from the command
    /// line's settings, whichever of the two folders is the account's.
    #[test]
    fn a_hosted_session_starts_in_the_settings_agent_mode() {
        let root = crate::fixtures::scratch("agy-acp-start-mode");
        let app_home = root.join(APP_DIR);
        let cli_home = root.join(CLI_DIR);
        std::fs::create_dir_all(&app_home).unwrap();
        std::fs::create_dir_all(&cli_home).unwrap();
        let settings = cli_home.join("settings.json");
        assert_eq!(Antigravity.acp_start_mode(&app_home), None, "no settings");
        std::fs::write(&settings, "{not json").unwrap();
        assert_eq!(Antigravity.acp_start_mode(&app_home), None, "settings that don't read");
        for (saved, mode) in [(serde_json::json!({}), None), (serde_json::json!({ "agentMode": "" }), None), (serde_json::json!({ "agentMode": "default" }), None), (serde_json::json!({ "agentMode": "auto" }), Some("auto")), (serde_json::json!({ "agentMode": "accept-edits" }), Some("accept-edits"))] {
            std::fs::write(&settings, saved.to_string()).unwrap();
            assert_eq!(Antigravity.acp_start_mode(&app_home).as_deref(), mode, "{saved}");
            assert_eq!(Antigravity.acp_start_mode(&cli_home).as_deref(), mode, "{saved}");
        }
    }
}
