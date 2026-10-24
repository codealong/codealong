//! Codealong's `PermissionRequest` hook in a Codex account, which hands Codealong the account's
//! approval prompts as they come up (asks.rs). It's a group of its own in `<CODEX_HOME>/hooks.json`,
//! beside any of yours, known by its command (`MARK`), and comes and goes with the skill Settings
//! › Agents puts in the account (install.rs).
//!
//! Codex runs only the hooks you've trusted, asking about a new or changed one as its terminal
//! UI starts, and keeps what's trusted in config.toml (`[hooks.state."<key>"] trusted_hash`, a
//! hash of the hook as Codex normalizes it). So the hook is trusted as it's put in, as the
//! terminal UI's own review does it: Codex's app-server lists it with its key and hash
//! (`hooks/list`), and that hash is written under `hooks.state` (`config/batchWrite`;
//! 0.162.1's `tui/src/hooks_rpc.rs`), which `appserver::trust_hook` does. One that can't be
//! trusted is taken out again: left in, every terminal UI would stop on the review as it starts,
//! a helper started out of sight with them. 0.162.1 and the app's 0.163.0-alpha hashed it alike
//! (2026-10-24), and a hook is read as a session starts, so one already running goes on without.

use std::path::Path;

use anyhow::{Context, Result};
use serde_json::{json, Value};

use crate::jsonedit::Node;

/// What the hook runs, after Codealong's command: how a group is known for Codealong's.
pub const MARK: &str = "session hook codex-permission-request";

const EVENT: &str = "PermissionRequest";

/// The hook's command: `codealong` (the plugin's shim, which finds the app wherever it is) with
/// its output thrown away, so Codex never takes anything it prints for a decision.
pub fn command(codealong: &Path) -> String {
    format!("\"{}\" {MARK} >/dev/null 2>&1 || true", codealong.display())
}

/// Puts the hook in the account at `home`, or brings it up to date, and has Codex trust it.
/// Whether anything changed. An error once it's taken out again.
pub fn install(home: &Path, codealong: &Path) -> Result<bool> {
    let file = home.join("hooks.json");
    let wanted = command(codealong);
    let written = match with_hook(&read(&file)?, &wanted)? {
        Some(text) => {
            crate::util::write_whole(&file, &text).with_context(|| format!("writing {}", file.display()))?;
            true
        }
        None => false,
    };
    match crate::codex::trust_hook(home, &file, &wanted) {
        Ok(trusted) => Ok(written || trusted),
        Err(e) => {
            remove(home)?;
            Err(e.context("Codex didn't trust Codealong's hook, so it was taken out again"))
        }
    }
}

/// Takes the hook out of the account at `home`, leaving any others.
pub fn remove(home: &Path) -> Result<()> {
    let file = home.join("hooks.json");
    if let Some(text) = without_hook(&read(&file)?)? {
        crate::util::write_whole(&file, &text)?;
    }
    Ok(())
}

/// A `hooks.json`'s text, or an empty object where there's none.
fn read(file: &Path) -> Result<String> {
    match std::fs::read_to_string(file) {
        Ok(t) if t.trim().is_empty() => Ok("{}\n".to_string()),
        Ok(t) => Ok(t),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok("{}\n".to_string()),
        Err(e) => Err(e).with_context(|| format!("reading {}", file.display())),
    }
}

/// Codealong's group: the one hook, run for every tool, with a timeout to match the file it
/// writes.
fn ours(command: &str) -> Value {
    json!({ "hooks": [{ "type": "command", "command": command, "timeout": 10 }] })
}

fn is_ours(group: &Value) -> bool {
    group.get("hooks").and_then(Value::as_array).is_some_and(|hs| hs.iter().any(|h| h.get("command").and_then(Value::as_str).is_some_and(|c| c.contains(MARK))))
}

/// `text` with Codealong's group in its `PermissionRequest` list, as the one with `command`, or
/// None where it's there already. Everything else keeps its text and its place.
fn with_hook(text: &str, command: &str) -> Result<Option<String>> {
    edit(text, |groups| {
        let want = ours(command);
        match groups.iter().position(is_ours) {
            Some(i) if groups[i] == want => false,
            Some(i) => {
                groups[i] = want;
                true
            }
            None => {
                groups.push(want);
                true
            }
        }
    })
}

/// `text` without Codealong's group, or None where it has none.
fn without_hook(text: &str) -> Result<Option<String>> {
    edit(text, |groups| {
        let before = groups.len();
        groups.retain(|g| !is_ours(g));
        groups.len() != before
    })
}

/// `text` with its `hooks.PermissionRequest` list changed by `change`, which says whether it did.
/// Codex writes nothing to this file, so it's kept as you wrote it, a list emptied going with its
/// key.
fn edit(text: &str, change: impl FnOnce(&mut Vec<Value>) -> bool) -> Result<Option<String>> {
    let mut top = Node::entries(text).context("hooks.json isn't a JSON object")?;
    let hooks = Node::object_in(&mut top, "hooks")?;
    let at = hooks.iter().rposition(|(k, _)| k == EVENT);
    let mut groups: Vec<Value> = match at.map(|i| &hooks[i].1) {
        Some(Node::Raw(raw)) if raw.get() != "null" => serde_json::from_str(raw.get()).context("hooks.json's PermissionRequest isn't a list")?,
        _ => Vec::new(),
    };
    if !change(&mut groups) {
        return Ok(None);
    }
    match (at, groups.is_empty()) {
        (Some(i), true) => {
            hooks.remove(i);
        }
        (None, true) => {}
        (at, false) => {
            // Indented as a value two levels in.
            let list = Node::Raw(serde_json::value::RawValue::from_string(serde_json::to_string_pretty(&groups)?.replace('\n', "\n    "))?);
            match at {
                Some(i) => hooks[i].1 = list,
                None => hooks.push((EVENT.to_string(), list)),
            }
        }
    }
    let mut out = serde_json::to_string_pretty(&Node::Object(top))?;
    out.push('\n');
    Ok(Some(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHIM: &str = "/Users/me/Library/Application Support/Codealong/plugin-marketplace/plugin/bin/codealong";

    #[test]
    fn the_hook_goes_in_beside_yours_and_comes_out_alone() {
        let wanted = command(Path::new(SHIM));
        assert_eq!(wanted, format!("\"{SHIM}\" session hook codex-permission-request >/dev/null 2>&1 || true"));
        // None there yet: a file of its own.
        let made = with_hook("{}\n", &wanted).unwrap().unwrap();
        let v: Value = serde_json::from_str(&made).unwrap();
        assert_eq!(v["hooks"]["PermissionRequest"][0]["hooks"][0]["command"], wanted.as_str());
        assert_eq!(with_hook(&made, &wanted).unwrap(), None, "there already");

        // Yours stay as they were, theirs first.
        let yours = r#"{
  "description": "my hooks",
  "hooks": {
    "PreToolUse": [{"matcher": "Bash", "hooks": [{"type": "command", "command": "./check.sh"}]}],
    "PermissionRequest": [{"hooks": [{"type": "command", "command": "./mine.sh"}]}]
  }
}
"#;
        let both = with_hook(yours, &wanted).unwrap().unwrap();
        let v: Value = serde_json::from_str(&both).unwrap();
        assert_eq!(v["description"], "my hooks");
        assert_eq!(v["hooks"]["PreToolUse"][0]["matcher"], "Bash");
        assert_eq!(v["hooks"]["PermissionRequest"][0]["hooks"][0]["command"], "./mine.sh");
        assert_eq!(v["hooks"]["PermissionRequest"][1]["hooks"][0]["command"], wanted.as_str());
        assert!(both.contains(r#""PreToolUse": [{"matcher": "Bash", "hooks": [{"type": "command", "command": "./check.sh"}]}]"#), "the rest keeps its text");

        // The app moved: the same group, its new command.
        let moved = with_hook(&both, &command(Path::new("/Applications/Codealong.app/x/codealong"))).unwrap().unwrap();
        let v: Value = serde_json::from_str(&moved).unwrap();
        assert_eq!(v["hooks"]["PermissionRequest"].as_array().unwrap().len(), 2);

        // Out again: yours stay; a list left empty goes.
        let out: Value = serde_json::from_str(&without_hook(&moved).unwrap().unwrap()).unwrap();
        assert_eq!(out["hooks"]["PermissionRequest"].as_array().unwrap().len(), 1);
        let alone: Value = serde_json::from_str(&without_hook(&made).unwrap().unwrap()).unwrap();
        assert_eq!(alone, json!({ "hooks": {} }));
        assert_eq!(without_hook(yours).unwrap(), None);
        assert!(with_hook("[1]", &wanted).is_err(), "not an object");
    }

    /// The real Codex trusts the hook as it goes into a scratch account, and needn't again.
    /// `cargo test a_real_codex_trusts_the_hook -- --ignored`: it runs Codex's app-server.
    #[test]
    #[ignore]
    fn a_real_codex_trusts_the_hook() {
        let home = crate::fixtures::scratch("codex-hook-home");
        assert!(install(&home, Path::new(SHIM)).unwrap(), "put in and trusted");
        assert!(!install(&home, Path::new(SHIM)).unwrap(), "trusted already");
        let config = std::fs::read_to_string(home.join("config.toml")).unwrap();
        assert!(config.contains("[hooks.state.") && config.contains("trusted_hash = \"sha256:"), "{config}");
        remove(&home).unwrap();
        let left: Value = serde_json::from_str(&std::fs::read_to_string(home.join("hooks.json")).unwrap()).unwrap();
        assert_eq!(left, json!({ "hooks": {} }));
    }
}
