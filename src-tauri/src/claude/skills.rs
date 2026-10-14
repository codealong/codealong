//! Where Claude Code reads skills and commands from, and how often it ran each (read from
//! 2.1.289's code and this Mac's folders, 2026-10-04): a `.claude` folder's `skills/<name>/
//! SKILL.md` and `commands/**/*.md`; an account's own in its home, the ones claude.ai syncs into
//! `skills/synced/<sign-in>/` (run as `anthropic-skills:<name>`, or by the name alone), its
//! enabled plugins' (run as `<plugin>:<name>`), and those of the plugins claude.ai syncs into
//! `plugins/synced/<sign-in>/<plugin>/` (the same, checked against 2.1.299's list of what it
//! runs, 2026-10-14); and `skillUsage` in the account's state file. What its program holds is
//! claude/builtin.rs's.

use std::collections::{BTreeMap, HashMap};
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
    let read_at = |path: PathBuf| std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()).unwrap_or(Value::Null);
    let read = |file: &str| read_at(home.join(file));
    let settings = read("settings.json");
    out.extend(plugin_roots(&read("plugins/installed_plugins.json"), &settings));
    // What claude.ai syncs is for the sign-in, and only the one Claude Code is signed in with
    // is loaded: the others' folders stay after a switch.
    let Some(bucket) = synced_bucket(&read_at(state_file(home))) else { return out };
    out.push(Root::skills(home.join("skills/synced").join(&bucket), 1).of_plugin(SYNCED));
    let plugins: Vec<PathBuf> = std::fs::read_dir(home.join("plugins/synced").join(&bucket)).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.join(".claude-plugin/plugin.json").is_file()).collect();
    out.extend(synced_plugin_roots(&plugins, &settings));
    out
}

/// The folder under `skills/synced` and `plugins/synced` that holds what claude.ai syncs for
/// the sign-in in an account's state file (`oauthAccount`): `<org>_<account>`, lowercase, or
/// `<org>_unbound` with no account (2.1.299's code). None without a claude.ai sign-in.
fn synced_bucket(state: &Value) -> Option<String> {
    let signed_in = &state["oauthAccount"];
    let org = signed_in["organizationUuid"].as_str().filter(|o| !o.is_empty())?;
    let account = signed_in["accountUuid"].as_str().filter(|a| !a.is_empty()).unwrap_or("unbound");
    Some(format!("{}_{}", org.to_lowercase(), account.to_lowercase()))
}

/// The skills and commands of the plugins claude.ai syncs into an account, a folder each under
/// its sign-in's, less the ones its settings turn off: each by `enabledPlugins`
/// (`<plugin>@synced: false`), and all of them by `syncClaudeAiPlugins: false`. An update
/// that comes while the plugin is in use goes in a folder of its own, `<plugin>~g<n>` from 2 up,
/// which Claude Code names without the `~g<n>` (2.1.299's code), so a plugin is the folder of
/// its latest one.
fn synced_plugin_roots(plugins: &[PathBuf], settings: &Value) -> Vec<Root> {
    if settings["syncClaudeAiPlugins"] == Value::Bool(false) {
        return Vec::new();
    }
    let mut latest: BTreeMap<String, (u32, &PathBuf)> = BTreeMap::new();
    for dir in plugins {
        let Some(folder) = dir.file_name().and_then(|n| n.to_str()) else { continue };
        let (name, generation) = generation_of(folder);
        if latest.get(name).is_none_or(|(had, _)| generation > *had) {
            latest.insert(name.to_string(), (generation, dir));
        }
    }
    let mut out = Vec::new();
    for (name, (_, dir)) in latest {
        if settings["enabledPlugins"][format!("{name}@synced")] == Value::Bool(false) {
            continue;
        }
        out.push(Root::skills(dir.join("skills"), 1).of_plugin(&name));
        out.push(Root::commands(dir.join("commands")).of_plugin(&name));
    }
    out
}

/// A synced folder's name and which update of it it is: `northwind~g3` is `northwind`'s third,
/// `northwind` its first.
fn generation_of(folder: &str) -> (&str, u32) {
    match folder.rsplit_once("~g") {
        Some((name, n)) if !n.starts_with('0') => match n.parse::<u32>() {
            Ok(n) if n >= 2 => (name, n),
            _ => (folder, 1),
        },
        _ => (folder, 1),
    }
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
    fn a_synced_plugin_is_its_latest_update_by_its_own_name() {
        let at = |folder: &str| PathBuf::from(format!("/b/{folder}"));
        let plugins = [at("northwind~g3"), at("northwind"), at("northwind~g2"), at("odd~g1"), at("odd~gx")];
        let roots = synced_plugin_roots(&plugins, &json!({ "enabledPlugins": { "northwind@synced": true } }));
        let named: Vec<(String, PathBuf)> = roots.iter().filter(|r| matches!(r.kind, crate::skills::Kind::Skills { .. })).map(|r| (r.prefix.clone().unwrap(), r.dir.clone())).collect();
        // `~g1` and `~gx` aren't updates: they're names of their own.
        assert_eq!(named, vec![("odd~g1".to_string(), at("odd~g1/skills")), ("odd~gx".to_string(), at("odd~gx/skills")), ("northwind".to_string(), at("northwind~g3/skills"))]);
        // Turned off by its own name, whichever update is latest.
        assert!(synced_plugin_roots(&plugins[..3], &json!({ "enabledPlugins": { "northwind@synced": false } })).is_empty());
    }

    #[test]
    fn only_the_signed_in_identitys_synced_folder_is_read() {
        let state = json!({ "oauthAccount": { "organizationUuid": "9EE3D10E-670B-4826-AD22-BF58E90552CD", "accountUuid": "8c65f1dc-8ed1-4f7b-94d3-32793843845e" } });
        assert_eq!(synced_bucket(&state).as_deref(), Some("9ee3d10e-670b-4826-ad22-bf58e90552cd_8c65f1dc-8ed1-4f7b-94d3-32793843845e"));
        assert_eq!(synced_bucket(&json!({ "oauthAccount": { "organizationUuid": "abc" } })).as_deref(), Some("abc_unbound"));
        assert_eq!(synced_bucket(&json!({})), None);
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
