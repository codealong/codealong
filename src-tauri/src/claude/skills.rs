//! Where Claude Code reads skills and commands from, and how often it ran each (read from
//! 2.1.289's code and this Mac's folders, 2026-10-04): a `.claude` folder's `skills/<name>/
//! SKILL.md` and `commands/**/*.md`; an account's own in its home, the ones claude.ai syncs into
//! `skills/synced/<id>/` (run as `anthropic-skills:<name>`), and its enabled plugins' (run as
//! `<plugin>:<name>`); and `skillUsage` in the account's state file.

use std::collections::HashMap;
use std::path::Path;

use serde_json::Value;

use super::trust::state_file;
use crate::skills::Root;

/// A `.claude` folder's skills and commands: a worktree's, or an account's home.
pub fn in_folder(claude: &Path) -> Vec<Root> {
    vec![Root::skills(claude.join("skills"), 1), Root::commands(claude.join("commands"))]
}

/// What Claude Code calls the skills claude.ai syncs into an account.
const SYNCED: &str = "anthropic-skills";

/// An account's own skills: its home's, the synced ones, and its enabled plugins'.
pub fn of_account(home: &Path) -> Vec<Root> {
    let mut out = in_folder(home);
    let synced = home.join("skills/synced");
    out.extend(std::fs::read_dir(&synced).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.is_dir()).map(|p| Root::skills(p, 1).of_plugin(SYNCED)));
    let read = |file: &str| std::fs::read_to_string(home.join(file)).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()).unwrap_or(Value::Null);
    out.extend(plugin_roots(&read("plugins/installed_plugins.json"), &read("settings.json")));
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
    fn skill_usage_is_read_from_the_state_file() {
        let state = json!({ "skillUsage": { "ship": { "usageCount": 58, "lastUsedAt": 1790000000000i64 }, "odd": {} } });
        let uses = uses_in(&state);
        assert_eq!(uses["ship"], (58, 1790000000000));
        assert_eq!(uses["odd"], (0, 0));
        assert!(uses_in(&json!({})).is_empty());
    }
}
