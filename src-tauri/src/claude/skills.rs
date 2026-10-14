//! Where Claude Code reads skills and commands from, and how often it ran each (read from
//! 2.1.289's code and this Mac's folders, 2026-10-04): a `.claude` folder's `skills/<name>/
//! SKILL.md` and `commands/**/*.md`; an account's own in its home, the ones claude.ai syncs into
//! `skills/synced/<id>/` (run as `anthropic-skills:<name>`, or by the name alone), its enabled
//! plugins' (run as `<plugin>:<name>`), and those of the plugins claude.ai syncs into
//! `plugins/synced/<id>/<plugin>/` (the same, checked against 2.1.299's list of what it runs,
//! 2026-10-14); and `skillUsage` in the account's state file. What its program holds is
//! claude/builtin.rs's.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::trust::state_file;
use crate::skills::Root;

/// A `.claude` folder's skills and commands: a worktree's, or an account's home.
pub fn in_folder(claude: &Path) -> Vec<Root> {
    vec![Root::skills(claude.join("skills"), 1), Root::commands(claude.join("commands"))]
}

/// What Claude Code calls the skills claude.ai syncs into an account.
const SYNCED: &str = "anthropic-skills";

/// An account's own skills: its home's, the synced ones, its enabled plugins' and the enabled
/// ones of those claude.ai syncs.
pub fn of_account(home: &Path) -> Vec<Root> {
    let mut out = in_folder(home);
    let folders = |dir: PathBuf| std::fs::read_dir(dir).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect::<Vec<_>>();
    out.extend(folders(home.join("skills/synced")).into_iter().map(|p| Root::skills(p, 1).of_plugin(SYNCED)));
    let read = |file: &str| std::fs::read_to_string(home.join(file)).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()).unwrap_or(Value::Null);
    let settings = read("settings.json");
    out.extend(plugin_roots(&read("plugins/installed_plugins.json"), &settings));
    let synced: Vec<PathBuf> = folders(home.join("plugins/synced")).into_iter().flat_map(folders).filter(|p| p.join(".claude-plugin/plugin.json").is_file()).collect();
    out.extend(synced_plugin_roots(&synced, &settings));
    out
}

/// The skills and commands of the plugins claude.ai syncs into an account, a folder each under
/// its organization's (`plugins/synced/<org>_<account>/<plugin>/`, named by the folder), less
/// the ones its settings turn off: each by `enabledPlugins` (`<plugin>@synced: false`), and all
/// of them by `syncClaudeAiPlugins: false`.
fn synced_plugin_roots(plugins: &[PathBuf], settings: &Value) -> Vec<Root> {
    if settings["syncClaudeAiPlugins"] == Value::Bool(false) {
        return Vec::new();
    }
    let mut out = Vec::new();
    for dir in plugins {
        let Some(name) = dir.file_name().and_then(|n| n.to_str()) else { continue };
        if settings["enabledPlugins"][format!("{name}@synced")] == Value::Bool(false) {
            continue;
        }
        out.push(Root::skills(dir.join("skills"), 1).of_plugin(name));
        out.push(Root::commands(dir.join("commands")).of_plugin(name));
    }
    out
}

/// The skills and commands of the plugins an account has for every folder (`scope: "user"`),
/// from its `installed_plugins.json`, less the ones its settings turn off.
fn plugin_roots(installed: &Value, settings: &Value) -> Vec<Root> {
    let mut out = Vec::new();
    for (id, installs) in installed["plugins"].as_object().into_iter().flatten() {
        if settings["enabledPlugins"][id] == Value::Bool(false) {
            continue;
        }
        let name = id.split('@').next().unwrap_or(id);
        for install in installs.as_array().into_iter().flatten().filter(|i| i["scope"] == "user") {
            let Some(path) = install["installPath"].as_str().map(Path::new) else { continue };
            out.push(Root::skills(path.join("skills"), 1).of_plugin(name));
            out.push(Root::commands(path.join("commands")).of_plugin(name));
        }
    }
    out
}

/// How often an account ran each skill, and when last: `skillUsage` in its state file,
/// `{<name>: {usageCount, lastUsedAt}}`, counting the agent's own uses with yours.
pub fn uses(home: &Path) -> HashMap<String, (u32, i64)> {
    let state: Value = std::fs::read_to_string(state_file(home)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(Value::Null);
    uses_in(&state)
}

fn uses_in(state: &Value) -> HashMap<String, (u32, i64)> {
    state["skillUsage"].as_object().into_iter().flatten().map(|(name, u)| (name.clone(), (u["usageCount"].as_u64().unwrap_or(0) as u32, u["lastUsedAt"].as_i64().unwrap_or(0)))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::path::PathBuf;

    #[test]
    fn an_accounts_plugins_give_their_skills_unless_turned_off() {
        let installed = json!({ "version": 2, "plugins": {
            "figma@official": [{ "scope": "user", "installPath": "/cache/figma/2" }],
            "off@official": [{ "scope": "user", "installPath": "/cache/off/1" }],
            "here@official": [{ "scope": "project", "installPath": "/cache/here/1", "projectPath": "/repo" }],
        } });
        let settings = json!({ "enabledPlugins": { "figma@official": true, "off@official": false } });
        let roots = plugin_roots(&installed, &settings);
        assert_eq!(roots, vec![Root::skills(PathBuf::from("/cache/figma/2/skills"), 1).of_plugin("figma"), Root::commands(PathBuf::from("/cache/figma/2/commands")).of_plugin("figma")]);
        assert_eq!(plugin_roots(&Value::Null, &Value::Null), vec![]);
    }

    #[test]
    fn the_plugins_claude_ai_syncs_give_their_skills_unless_turned_off() {
        let plugins = [PathBuf::from("/home/plugins/synced/org_me/northwind"), PathBuf::from("/home/plugins/synced/org_me/codex")];
        let settings = json!({ "enabledPlugins": { "codex@synced": false } });
        assert_eq!(synced_plugin_roots(&plugins, &settings), vec![Root::skills(PathBuf::from("/home/plugins/synced/org_me/northwind/skills"), 1).of_plugin("northwind"), Root::commands(PathBuf::from("/home/plugins/synced/org_me/northwind/commands")).of_plugin("northwind")]);
        assert_eq!(synced_plugin_roots(&plugins, &json!({ "syncClaudeAiPlugins": false })), vec![]);
        assert_eq!(synced_plugin_roots(&plugins, &Value::Null).len(), 4);
    }

    #[test]
    fn skill_usage_is_read_from_the_state_file() {
        let state = json!({ "skillUsage": { "ship": { "usageCount": 58, "lastUsedAt": 1790000000000i64 }, "odd": {} } });
        let uses = uses_in(&state);
        assert_eq!(uses["ship"], (58, 1790000000000));
        assert_eq!(uses["odd"], (0, 0));
        assert!(uses_in(&json!({})).is_empty());
    }
}
