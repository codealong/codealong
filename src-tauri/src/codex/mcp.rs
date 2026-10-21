//! A Codex account's MCP servers (mcp.rs): the ChatGPT apps it has (Gmail, Google Drive,
//! GitHub…), the servers its `config.toml` adds, and its plugins'.
//!
//! The apps are what its app-server's `app/installed` lists, `{apps: [{id, runtimeName, enabled,
//! callable}]}`, answered at once from what the account keeps (0.160.1, 2026-10-20); `app/list`
//! would name every app ChatGPT has, 5,029 of them in 22 seconds. A server in `config.toml` is
//! `[mcp_servers.<name>]`, which `enabled = false` turns off. A plugin is
//! `[plugins."<name>@<marketplace>"]` with `enabled = true`, its files in
//! `plugins/cache/<marketplace>/<name>/<version>/`, where `.codex-plugin/plugin.json` names its
//! MCP file (`"mcpServers": "./.mcp.json"`) and `.app.json` its apps, which `app/installed`
//! already lists. The app-server's `mcpServerStatus/list` would name all of them, but it starts
//! every server to ask it (1.9 seconds here, one failing to start), so Codealong reads the files.

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::config::AccountCfg;
use crate::mcp::{plugin_folder, servers_in, Mcp, Server};
use crate::util::tilde;

pub fn read(account: &AccountCfg) -> Mcp {
    let home = account.home_path();
    let mut notes = Vec::new();
    let mut servers = apps(&home).unwrap_or_else(|why| {
        notes.push(why);
        Vec::new()
    });
    let file = home.join("config.toml");
    match config(&file) {
        Ok(config) => {
            servers.extend(servers_in(&config["mcp_servers"], &tilde(&file)));
            for (name, marketplace) in plugins(&config) {
                if let Some(dir) = plugin_dir(&home, &marketplace, &name) {
                    servers.extend(plugin_folder(&dir, ".codex-plugin", &name));
                }
            }
        }
        Err(why) => notes.push(why),
    }
    Mcp::new(account, servers, (!notes.is_empty()).then(|| notes.join("; ")))
}

/// The ChatGPT apps the account can call.
fn apps(home: &Path) -> Result<Vec<Server>, String> {
    let answer = super::appserver::call(home, "app/installed", json!({})).map_err(|e| format!("Couldn't ask Codex for its apps: {e}"))?;
    Ok(installed(&answer))
}

/// The apps the answer has turned on and callable, by name.
fn installed(answer: &Value) -> Vec<Server> {
    let mut out: Vec<Server> = answer["apps"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|a| a["enabled"] == Value::Bool(true) && a["callable"] == Value::Bool(true))
        .filter_map(|a| a["runtimeName"].as_str().filter(|n| !n.trim().is_empty()).map(|n| Server::new(n, "ChatGPT app")))
        .collect();
    out.sort_by_key(|s| s.name.to_lowercase());
    out
}

/// The account's config.toml as JSON: empty when there's none, and why when it doesn't parse.
fn config(file: &Path) -> Result<Value, String> {
    let Ok(text) = std::fs::read_to_string(file) else { return Ok(Value::Null) };
    let table: toml::Table = toml::from_str(&text).map_err(|_| format!("{} doesn't parse", tilde(file)))?;
    Ok(serde_json::to_value(table).unwrap_or(Value::Null))
}

/// The plugins config.toml turns on, each `(name, marketplace)`.
fn plugins(config: &Value) -> Vec<(String, String)> {
    config["plugins"]
        .as_object()
        .into_iter()
        .flatten()
        .filter(|(_, p)| p["enabled"] == Value::Bool(true))
        .filter_map(|(id, _)| id.split_once('@'))
        .filter(|(name, marketplace)| ![name, marketplace].iter().any(|p| p.is_empty() || p.contains(['/', '\\']) || **p == ".."))
        .map(|(name, marketplace)| (name.to_string(), marketplace.to_string()))
        .collect()
}

/// The version of a plugin Codex has latest: the one of its folders last written.
fn plugin_dir(home: &Path, marketplace: &str, name: &str) -> Option<PathBuf> {
    let versions = std::fs::read_dir(home.join("plugins/cache").join(marketplace).join(name)).ok()?;
    versions
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.join(".codex-plugin/plugin.json").is_file())
        .max_by_key(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_apps_it_can_call_are_listed() {
        // As 0.160.1's app-server answered, cut short.
        let answer = json!({ "apps": [
            { "id": "connector_0001", "runtimeName": "Gmail", "enabled": true, "callable": true },
            { "id": "asdk_app_0002", "runtimeName": "Kestrel", "enabled": false, "callable": true },
            { "id": "connector_0003", "runtimeName": "Drafts", "enabled": true, "callable": false },
        ] });
        assert_eq!(installed(&answer), vec![Server::new("Gmail", "ChatGPT app")]);
        assert!(installed(&json!({})).is_empty());
    }

    #[test]
    fn config_toml_gives_its_servers_and_the_plugins_it_turns_on() {
        let dir = std::env::temp_dir().join(format!("codealong-codex-mcp-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("config.toml");
        std::fs::write(
            &file,
            "model = \"gpt-6\"\n\n[plugins.\"atlas@acme-plugins\"]\nenabled = true\n\n[plugins.\"off@acme-plugins\"]\nenabled = false\n\n[plugins.\"../x@acme\"]\nenabled = true\n\n[mcp_servers.northwind]\ncommand = \"northwind\"\n\n[mcp_servers.northwind.env]\nKEY = \"1\"\n\n[mcp_servers.parked]\nurl = \"https://mcp.example.com\"\nenabled = false\n",
        )
        .unwrap();
        let read = config(&file).unwrap();
        assert_eq!(servers_in(&read["mcp_servers"], "f").iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["northwind"]);
        assert_eq!(plugins(&read), [("atlas".to_string(), "acme-plugins".to_string())]);
        assert_eq!(config(&dir.join("missing.toml")), Ok(Value::Null));
        std::fs::write(&file, "[[[").unwrap();
        assert!(config(&file).unwrap_err().ends_with("doesn't parse"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
