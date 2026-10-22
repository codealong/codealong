//! The permission mode a Claude Code session runs in, as its files have it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

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

/// The modes Shift+Tab steps a terminal session through, in Claude Code's order, each with its
/// id (2.1.308's `y8t`, checked in a terminal on 2026-10-22: Auto, Manual, Accept edits, Plan
/// and Auto again): Manual, Accept edits and Plan, then Bypass where the session offers it, then
/// Auto where it does, and round to Manual. Don't Ask steps to Manual. Claude Code writes the
/// mode it's switched to only with the next prompt, so what a press leads to is worked out from
/// this (cycle.rs). `recorded` is the mode its record says it's in, `command` its process's
/// command line, `home` the account's folder.
pub fn cycled_modes(home: &Path, recorded: Option<&str>, command: Option<&str>) -> Vec<SessionMode> {
    let offers = offers(home);
    let started = started_in(command.unwrap_or_default());
    let starts = started.mode.as_deref().or(offers.default_mode.as_deref());
    // Offered to a session that started bypassing, or was let to (`isBypassPermissionsModeAvailable`).
    let bypass = !offers.no_bypass && (starts == Some("bypassPermissions") || started.may_bypass || recorded == Some("bypassPermissions"));
    // Offered unless settings or Anthropic's flag turn it off (`q_`, whose default is on), or
    // the flag leaves it to an opt-in this session shows it took. Whether the model takes it
    // isn't known here.
    let auto = !offers.no_auto && match offers.auto_flag.as_str() {
        "disabled" => false,
        "opt-in" => starts == Some("auto") || recorded == Some("auto"),
        _ => true,
    };
    let ids = ["default", "acceptEdits", "plan"].into_iter().chain(bypass.then_some("bypassPermissions")).chain(auto.then_some("auto"));
    ids.filter_map(|id| session_mode(id).map(|m| m.with_id(id))).collect()
}

/// The id `cycled_modes` gives a mode as Claude Code writes it: Manual is written `manual` too.
pub fn mode_id(raw: &str) -> &str {
    if raw == "manual" {
        "default"
    } else {
        raw
    }
}

/// What a session's command line says of its modes: `--permission-mode`, and whether
/// `--dangerously-skip-permissions` started it bypassing or `--allow-dangerously-skip-permissions`
/// let it. A word of the prompt can't be told from an argument in `ps`'s line, so a prompt that
/// is one of these words alone counts too.
#[derive(Debug, Default, PartialEq)]
struct Started {
    mode: Option<String>,
    may_bypass: bool,
}

fn started_in(command: &str) -> Started {
    let mut out = Started::default();
    let mut words = command.split_whitespace();
    while let Some(w) = words.next() {
        match w.split_once('=') {
            Some(("--permission-mode", mode)) => out.mode = Some(mode.to_string()),
            _ if w == "--permission-mode" => out.mode = words.next().map(str::to_string),
            _ if w == "--dangerously-skip-permissions" => out.mode = Some("bypassPermissions".into()),
            _ if w == "--allow-dangerously-skip-permissions" => out.may_bypass = true,
            _ => {}
        }
    }
    out
}

/// What an account's settings.json and the flags Claude Code keeps in its state file say of the
/// modes it offers (2.1.308's code). A repo's own settings aren't read.
#[derive(Debug, Clone, Default, PartialEq)]
struct Offers {
    /// `permissions.defaultMode`: the mode a session starts in without `--permission-mode`.
    default_mode: Option<String>,
    /// `permissions.disableBypassPermissionsMode: "disable"`.
    no_bypass: bool,
    /// `disableAutoMode: "disable"`, at the top or under `permissions`.
    no_auto: bool,
    /// `cachedGrowthBookFeatures.tengu_auto_mode_config.enabled`: `enabled` (also where it's
    /// missing, as Claude Code takes it), `disabled` or `opt-in`.
    auto_flag: String,
}

/// An account's `Offers`, read again only when one of its files changed: the state file is
/// tens of kilobytes, and this is asked of each running session whenever the board is put
/// together.
fn offers(home: &Path) -> Offers {
    type Stamp = Option<(std::time::SystemTime, u64)>;
    static KEPT: Mutex<Option<HashMap<PathBuf, ((Stamp, Stamp), Offers)>>> = Mutex::new(None);
    let (settings, state) = (home.join("settings.json"), super::trust::state_file(home));
    let stamp_of = |p: &Path| std::fs::metadata(p).ok().and_then(|m| Some((m.modified().ok()?, m.len())));
    let stamp = (stamp_of(&settings), stamp_of(&state));
    let mut kept = KEPT.lock().unwrap_or_else(|e| e.into_inner());
    let kept = kept.get_or_insert_with(HashMap::new);
    if let Some((at, offers)) = kept.get(home) {
        if *at == stamp {
            return offers.clone();
        }
    }
    let read = |p: &Path| std::fs::read(p).ok().and_then(|b| serde_json::from_slice::<Value>(&b).ok()).unwrap_or(Value::Null);
    let offers = offers_in(&read(&settings), &read(&state));
    kept.insert(home.to_path_buf(), (stamp, offers.clone()));
    offers
}

fn offers_in(settings: &Value, state: &Value) -> Offers {
    let said = |v: &Value, path: &str| v.pointer(path).and_then(Value::as_str).map(str::to_string);
    Offers {
        default_mode: said(settings, "/permissions/defaultMode"),
        no_bypass: said(settings, "/permissions/disableBypassPermissionsMode").as_deref() == Some("disable"),
        no_auto: ["/disableAutoMode", "/permissions/disableAutoMode"].iter().any(|p| said(settings, p).as_deref() == Some("disable")),
        auto_flag: said(state, "/cachedGrowthBookFeatures/tengu_auto_mode_config/enabled").filter(|f| ["disabled", "opt-in"].contains(&f.as_str())).unwrap_or_else(|| "enabled".into()),
    }
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
    fn steps_through_the_modes_its_session_offers_in_claude_codes_order() {
        let ids = |modes: Vec<SessionMode>| modes.into_iter().map(|m| m.id).collect::<Vec<_>>();
        let home = crate::fixtures::scratch("cycled");
        // Nothing set: Auto is on by Anthropic's default, and nothing started it bypassing.
        assert_eq!(ids(cycled_modes(&home, Some("default"), Some("claude --session-id 1 hi"))), ["default", "acceptEdits", "plan", "auto"]);
        // Started bypassing, or let to: Bypass before Auto.
        assert_eq!(ids(cycled_modes(&home, Some("acceptEdits"), Some("claude --dangerously-skip-permissions"))), ["default", "acceptEdits", "plan", "bypassPermissions", "auto"]);
        assert_eq!(ids(cycled_modes(&home, None, Some("claude --allow-dangerously-skip-permissions --resume x"))), ["default", "acceptEdits", "plan", "bypassPermissions", "auto"]);
        assert_eq!(ids(cycled_modes(&home, None, Some("claude --permission-mode=bypassPermissions"))).len(), 5);
        assert_eq!(ids(cycled_modes(&home, Some("bypassPermissions"), None)).len(), 5, "it's in it, so it was offered");
        // Settings that turn them off, and a default mode that starts it bypassing.
        std::fs::write(home.join("settings.json"), r#"{"permissions":{"defaultMode":"bypassPermissions","disableAutoMode":"disable"}}"#).unwrap();
        assert_eq!(ids(cycled_modes(&home, Some("default"), None)), ["default", "acceptEdits", "plan", "bypassPermissions"]);
        assert_eq!(ids(cycled_modes(&home, None, Some("claude --permission-mode plan"))), ["default", "acceptEdits", "plan"], "the flag wins over the default");
        std::fs::write(home.join("settings.json"), r#"{"permissions":{"disableBypassPermissionsMode":"disable"}}"#).unwrap();
        assert_eq!(ids(cycled_modes(&home, None, Some("claude --dangerously-skip-permissions"))), ["default", "acceptEdits", "plan", "auto"]);
        // Anthropic's flag, as the state file keeps it.
        std::fs::write(home.join("settings.json"), "{}").unwrap();
        std::fs::write(super::super::trust::state_file(&home), r#"{"cachedGrowthBookFeatures":{"tengu_auto_mode_config":{"enabled":"opt-in"}}}"#).unwrap();
        assert_eq!(ids(cycled_modes(&home, Some("plan"), None)), ["default", "acceptEdits", "plan"]);
        assert_eq!(ids(cycled_modes(&home, Some("auto"), None)), ["default", "acceptEdits", "plan", "auto"], "one in Auto took the opt-in");
        assert_eq!(cycled_modes(&home, Some("auto"), None)[3].label, "Auto");
        assert_eq!(mode_id("manual"), "default");
        assert_eq!(started_in("claude --permission-mode"), Started::default());
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
