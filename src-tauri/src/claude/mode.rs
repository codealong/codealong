//! The permission mode a Claude Code session runs in, as its files have it.

use std::path::PathBuf;

use serde_json::Value;

use super::registry::RegistryEntry;
use crate::model::{ModeKind, SessionMode};
use crate::util::scan_lines_back;

/// How far back a transcript is read for the mode its latest prompt went in.
const MODE_SCAN: u64 = 8 * 1024 * 1024;

/// The permission mode a live session runs in, as its files have it: a Claude app session's is
/// in the app's record of it (`hostSessionId` names it), where the app keeps the mode you pick;
/// otherwise it's the mode its latest prompt went in, which its transcript writes on that line.
pub(super) fn permission_mode(registry: Option<&RegistryEntry>, transcripts: &[PathBuf]) -> Option<String> {
    let app = registry.and_then(|r| r.host_session.as_deref());
    if let Some(mode) = app.and_then(|id| super::app::permission_mode(&super::app::records_dir(), id)) {
        return Some(mode);
    }
    let path = transcripts.first()?;
    let end = std::fs::metadata(path).ok()?.len();
    let mut mode = None;
    scan_lines_back(path, end.saturating_sub(MODE_SCAN), end, |line| {
        mode = line_mode(line);
        mode.is_some()
    });
    mode
}

/// A mode as Claude Code writes it (`permissionMode`), named as its own terminal and the Claude
/// app name it (2.1.296's table of modes: `default` is Manual, and `manual` is taken for it),
/// Bypass Permissions shortened to Bypass for the line under the reply box. None for a mode it
/// didn't have then.
pub fn session_mode(raw: &str) -> Option<SessionMode> {
    let (kind, label, detail) = match raw {
        "default" | "manual" => (ModeKind::Asks, "Manual", "Asks before it edits files or runs commands that do more than look."),
        "plan" => (ModeKind::Plan, "Plan", "Plans without changing anything, until you take the plan."),
        "acceptEdits" => (ModeKind::Edits, "Accept edits", "Edits files without asking, and asks before other commands."),
        "auto" => (ModeKind::Auto, "Auto", "A classifier allows what looks safe and asks you about the rest."),
        "dontAsk" => (ModeKind::Other, "Don't Ask", "Does only what its settings allow, and refuses the rest without asking."),
        "bypassPermissions" => (ModeKind::Skips, "Bypass", "Bypasses permissions: does everything without asking."),
        _ => return None,
    };
    Some(SessionMode::new(kind, label, detail))
}

/// A transcript line's own `permissionMode`. A quote inside a string is escaped, so the key
/// unescaped is never something a tool read or printed.
fn line_mode(line: &str) -> Option<String> {
    if !line.contains("\"permissionMode\":\"") {
        return None;
    }
    serde_json::from_str::<Value>(line).ok()?.get("permissionMode")?.as_str().map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_mode_the_latest_prompt_went_in() {
        let dir = crate::fixtures::scratch("mode");
        let t = dir.join("s.jsonl");
        let lines = [
            r#"{"type":"user","permissionMode":"default","message":{"role":"user","content":"one"}}"#,
            r#"{"type":"user","permissionMode":"bypassPermissions","message":{"role":"user","content":"two"}}"#,
            // A tool's output that has the key in it is a string, its quotes escaped.
            r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","content":"\"permissionMode\":\"default\""}]}}"#,
        ];
        std::fs::write(&t, lines.join("\n") + "\n").unwrap();
        assert_eq!(permission_mode(None, std::slice::from_ref(&t)).as_deref(), Some("bypassPermissions"));
        assert_eq!(line_mode(lines[2]), None);
        assert_eq!(permission_mode(None, &[]), None);
    }

    #[test]
    fn names_each_mode_as_claude_code_does() {
        let mode = |raw: &str| session_mode(raw).map(|m| (m.kind, m.label));
        assert_eq!(mode("default"), Some((ModeKind::Asks, "Manual".to_string())));
        assert_eq!(mode("manual"), mode("default"));
        assert_eq!(mode("bypassPermissions"), Some((ModeKind::Skips, "Bypass".to_string())));
        assert_eq!(mode("acceptEdits").map(|m| m.0), Some(ModeKind::Edits));
        assert_eq!(mode("plan").map(|m| m.0), Some(ModeKind::Plan));
        assert_eq!(mode("auto").map(|m| m.0), Some(ModeKind::Auto));
        assert_eq!(mode("someday"), None);
    }
}
