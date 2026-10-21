//! The MCP servers each account's agents have, as its harness keeps them (`Adapter::mcp`): the
//! connectors its provider connects online (claude.ai's, ChatGPT's apps), the servers its own
//! settings add, and its plugins'. Settings › Accounts lists them, and Jev starts work that needs
//! one on an account that has it (route.rs). Servers a project adds for its own folder (a repo's
//! `.mcp.json`) aren't an account's, and are left out. The engine reads them as accountpoll.rs
//! says (`McpPoll`); nothing here starts a server or asks one what it does.

use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::AccountCfg;
use crate::util::now_ms;

/// One server an account's agents can call.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Server {
    /// As its harness names it: "Gmail", "figma".
    pub name: String,
    /// Where it comes from, in words: "claude.ai connector", "figma plugin", "~/.cursor/mcp.json".
    pub from: String,
    /// Its tools' names, where the harness lists them without starting it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<String>,
}

impl Server {
    pub fn new(name: impl Into<String>, from: impl Into<String>) -> Server {
        Server { name: name.into(), from: from.into(), tools: Vec::new() }
    }
}

/// One account's servers, for the board.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Mcp {
    pub account: String,
    /// Connectors first, then its settings' servers, then its plugins', each once.
    pub servers: Vec<Server>,
    /// Why some or all of them couldn't be read: the connectors when the sign-in expired, a
    /// settings file that doesn't parse.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// When it was read. A read that couldn't tell keeps the last answer that could, and its
    /// time, with why in `stale`.
    pub as_of: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stale: Option<String>,
}

impl Mcp {
    /// What was read, each service once: a connector and a plugin's server for one service
    /// (claude.ai's Figma and the figma plugin) are the same to the agent.
    pub fn new(account: &AccountCfg, servers: Vec<Server>, note: Option<String>) -> Mcp {
        let mut seen = std::collections::HashSet::new();
        let servers = servers.into_iter().filter(|s| seen.insert(key(&s.name))).collect();
        Mcp { account: account.name.clone(), servers, note, as_of: Some(now_ms()), stale: None }
    }

    pub fn unknown(account: &AccountCfg, note: impl Into<String>) -> Mcp {
        Mcp { account: account.name.clone(), servers: Vec::new(), note: Some(note.into()), as_of: None, stale: None }
    }
}

impl crate::accountpoll::Reading for Mcp {
    fn account(&self) -> &str {
        &self.account
    }

    /// A read that missed some of them (the connectors, say) keeps the last whole one.
    fn keeping(&self, prev: &Mcp) -> Option<Mcp> {
        (self.note.is_some() && prev.note.is_none()).then(|| Mcp { stale: self.note.clone(), ..prev.clone() })
    }
}

/// Every account's last reading, and when to read them again: the engine's.
pub type McpPoll = crate::accountpoll::AccountPoll<Mcp>;

/// What a service is called whoever names it: "Google Calendar" and `google-calendar` are one.
pub fn key(name: &str) -> String {
    name.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

/// Reads one account's servers where its harness keeps them. It can take seconds.
pub fn read(account: &AccountCfg) -> Mcp {
    match crate::harness::of(account) {
        Some(h) => h.mcp(account),
        None => Mcp::unknown(account, "Codealong doesn't know this account's harness."),
    }
}

/// Reads every account's but those turned off, one after another: for the engine's background
/// read and the snapshot.
pub fn read_all(accounts: &[AccountCfg]) -> Vec<Mcp> {
    accounts.iter().filter(|a| !a.disabled).map(read).collect()
}

/// A JSON settings file: none when it's missing or empty, and why when it doesn't parse.
pub fn json_file(path: &Path) -> Result<Option<Value>, String> {
    match std::fs::read_to_string(path) {
        Ok(text) if text.trim().is_empty() => Ok(None),
        Ok(text) => serde_json::from_str(&text).map(Some).map_err(|_| format!("{} doesn't parse", crate::util::tilde(path))),
        Err(_) => Ok(None),
    }
}

/// The servers a map of them names (`mcpServers` in most harnesses' files), in its order, less
/// those turned off there: `disabled: true` (Antigravity, Cursor) or `enabled: false` (Codex,
/// OpenCode).
pub fn servers_in(map: &Value, from: &str) -> Vec<Server> {
    map.as_object()
        .into_iter()
        .flatten()
        .filter(|(_, v)| v["disabled"] != Value::Bool(true) && v["enabled"] != Value::Bool(false))
        .map(|(name, _)| Server::new(name, from))
        .collect()
}

/// A plugin's MCP file: `{"mcpServers": {…}}`, or the servers alone, each an object saying how it
/// runs (a `command`, a `url` or a `type`), as Claude Code and Codex take it.
pub fn plugin_servers(file: &Value, from: &str) -> Vec<Server> {
    if file.get("mcpServers").is_some() {
        return servers_in(&file["mcpServers"], from);
    }
    let runs = |v: &Value| ["command", "url", "type"].iter().any(|k| v.get(k).is_some());
    match file.as_object() {
        Some(m) if !m.is_empty() && m.values().all(runs) => servers_in(file, from),
        _ => Vec::new(),
    }
}

/// A plugin's servers, in its folder `dir`: its `.mcp.json`'s, and those its manifest
/// (`<manifest>/plugin.json`: `.claude-plugin` for Claude Code, `.codex-plugin` for Codex) names as
/// `mcpServers` (a file in the plugin, a list of them, or the servers themselves), which Claude
/// Code adds to the first file's (its plugins reference, "how each key combines with its default
/// location"). A server named twice is listed once (`Mcp::new`).
pub fn plugin_folder(dir: &Path, manifest: &str, plugin: &str) -> Vec<Server> {
    let from = format!("{plugin} plugin");
    let named = json_file(&dir.join(manifest).join("plugin.json")).ok().flatten().map(|m| m["mcpServers"].clone()).unwrap_or(Value::Null);
    let at = |file: &str| json_file(&dir.join(file)).ok().flatten().map(|f| plugin_servers(&f, &from)).unwrap_or_default();
    let mut out = at(".mcp.json");
    out.extend(match &named {
        Value::String(file) => at(file),
        Value::Array(items) => items.iter().flat_map(|i| i.as_str().map_or_else(|| plugin_servers(i, &from), at)).collect(),
        Value::Object(_) => plugin_servers(&named, &from),
        _ => Vec::new(),
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accountpoll::Reading;
    use serde_json::json;

    fn account(name: &str) -> AccountCfg {
        AccountCfg { name: name.into(), harness: "claude".into(), home: format!("~/.{name}"), command: None, start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false }
    }

    #[test]
    fn a_service_named_two_ways_shows_once() {
        let m = Mcp::new(&account("claude"), vec![Server::new("Figma", "claude.ai connector"), Server::new("Google Calendar", "claude.ai connector"), Server::new("figma", "figma plugin"), Server::new("google-calendar", "~/.claude.json")], None);
        assert_eq!(m.servers.iter().map(|s| (s.name.as_str(), s.from.as_str())).collect::<Vec<_>>(), [("Figma", "claude.ai connector"), ("Google Calendar", "claude.ai connector")]);
        assert_eq!(key("Google Calendar"), key("google_calendar"));
    }

    #[test]
    fn servers_turned_off_are_left_out() {
        let map = json!({ "on": { "command": "x" }, "off": { "command": "y", "disabled": true }, "no": { "url": "z", "enabled": false }, "yes": { "url": "w", "enabled": true } });
        assert_eq!(servers_in(&map, "f").iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["on", "yes"]);
        assert!(servers_in(&Value::Null, "f").is_empty());
    }

    #[test]
    fn a_plugins_file_names_its_servers_either_way() {
        let wrapped = json!({ "mcpServers": { "figma": { "type": "http", "url": "https://mcp.example.com" } } });
        let bare = json!({ "acme": { "command": "acme", "args": ["mcp"] } });
        assert_eq!(plugin_servers(&wrapped, "figma plugin"), vec![Server::new("figma", "figma plugin")]);
        assert_eq!(plugin_servers(&bare, "acme plugin"), vec![Server::new("acme", "acme plugin")]);
        assert!(plugin_servers(&json!({ "name": "not servers" }), "x").is_empty());
        assert!(plugin_servers(&json!({}), "x").is_empty());
    }

    #[test]
    fn a_plugins_servers_are_in_its_mcp_file_and_where_its_manifest_says() {
        let dir = std::env::temp_dir().join(format!("codealong-mcp-plugins-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let write = |path: &str, text: &str| {
            let p = dir.join(path);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, text).unwrap();
        };
        write("plain/.mcp.json", r#"{"mcpServers":{"figma":{"type":"http","url":"https://mcp.example.com"}}}"#);
        write("named/.codex-plugin/plugin.json", r#"{"name":"acme","mcpServers":"./servers.json"}"#);
        write("named/servers.json", r#"{"acme":{"command":"acme","args":["mcp"]}}"#);
        write("named/.mcp.json", r#"{"mcpServers":{"default":{"command":"x"}}}"#);
        write("inline/.claude-plugin/plugin.json", r#"{"mcpServers":{"kestrel":{"command":"kestrel"}}}"#);
        write("none/.claude-plugin/plugin.json", r#"{"name":"none"}"#);
        let names = |sub: &str, manifest: &str| plugin_folder(&dir.join(sub), manifest, sub).into_iter().map(|s| (s.name, s.from)).collect::<Vec<_>>();
        assert_eq!(names("plain", ".claude-plugin"), [("figma".to_string(), "plain plugin".to_string())]);
        assert_eq!(names("named", ".codex-plugin"), [("default".to_string(), "named plugin".to_string()), ("acme".to_string(), "named plugin".to_string())]);
        assert_eq!(names("inline", ".claude-plugin"), [("kestrel".to_string(), "inline plugin".to_string())]);
        assert!(names("none", ".claude-plugin").is_empty());
        assert!(names("missing", ".claude-plugin").is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_read_that_missed_some_keeps_the_last_whole_one() {
        let whole = Mcp { as_of: Some(1), ..Mcp::new(&account("claude"), vec![Server::new("Gmail", "claude.ai connector")], None) };
        let part = Mcp::new(&account("claude"), vec![], Some("Sign-in expired".into()));
        let kept = part.keeping(&whole).unwrap();
        assert_eq!((kept.servers.len(), kept.as_of, kept.stale.as_deref()), (1, Some(1), Some("Sign-in expired")));
        assert!(whole.keeping(&part).is_none(), "a whole read stands");
        assert!(part.keeping(&part).is_none(), "nothing whole to keep");
    }
}
