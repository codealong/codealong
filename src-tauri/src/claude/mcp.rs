//! A Claude account's MCP servers (mcp.rs): the claude.ai connectors its sign-in has connected,
//! the servers added for every folder (`claude mcp add --scope user`, its state file's
//! `mcpServers`), and its plugins'.
//!
//! The connectors are what Claude Code 2.1.306 asks for as a session starts: `GET /v1/mcp_servers`
//! with the account's sign-in (usage.rs) and the `mcp-servers-2025-12-04` beta, answering
//! `{data: [{display_name, eligible, eligibility_reason, tools: [{name}] | null, …}], next_page}`.
//! It loads the eligible ones: `connected`, or `not_an_org_connector` for the built-in Claude
//! Docs. One added to the claude.ai account but never connected (`never_connected_no_auto_connect`)
//! isn't, nor shown here (two accounts, 2026-10-20). A plugin's server is `plugin:<plugin>:<name>`
//! to Claude Code, read from the plugin's `.mcp.json` or what its `plugin.json` names.

use serde_json::Value;

use super::trust::state_file;
use super::usage::{read_sign_in, request_with, Fetch};
use crate::config::AccountCfg;
use crate::mcp::{json_file, plugin_folder, servers_in, Mcp, Server};
use crate::util::{code, now_ms, tilde};

const CONNECTORS_URL: &str = "https://api.anthropic.com/v1/mcp_servers?limit=1000&include_additional_installs=true";
const CONNECTOR: &str = "claude.ai connector";

pub fn read(account: &AccountCfg) -> Mcp {
    let home = account.home_path();
    let mut notes = Vec::new();
    // An API key has no claude.ai account to connect anything to.
    let mut servers = if account.per_token { Vec::new() } else { connectors(account).unwrap_or_else(|why| { notes.push(why); Vec::new() }) };
    let state = state_file(&home);
    match json_file(&state) {
        Ok(v) => servers.extend(v.map(|v| servers_in(&v["mcpServers"], &tilde(&state))).unwrap_or_default()),
        Err(why) => notes.push(why),
    }
    for (name, dir) in super::skills::plugins(&home) {
        servers.extend(plugin_folder(&dir, ".claude-plugin", &name));
    }
    Mcp::new(account, servers, (!notes.is_empty()).then(|| notes.join("; ")))
}

/// The claude.ai connectors the account has connected, or why they couldn't be read.
fn connectors(account: &AccountCfg) -> Result<Vec<Server>, String> {
    let fix = format!("run {} once to sign in again", code(account.command()));
    let sign_in = read_sign_in(&account.home_path()).ok_or_else(|| format!("Couldn't read its claude.ai connectors: not signed in, {fix}"))?;
    if sign_in.expires_at.is_some_and(|t| t <= now_ms() + 60_000) {
        return Err(format!("Couldn't read its claude.ai connectors: sign-in expired, {fix}"));
    }
    match request_with(&sign_in.token, "GET", CONNECTORS_URL, None, &["anthropic-beta: mcp-servers-2025-12-04", "anthropic-version: 2023-06-01"]) {
        Ok(v) => Ok(connected(&v)),
        Err(Fetch::SignedOut) => Err(format!("Couldn't read its claude.ai connectors: sign-in expired, {fix}")),
        Err(Fetch::RateLimited) => Err("Couldn't read its claude.ai connectors: rate limited".into()),
        Err(Fetch::Failed(why)) => Err(format!("Couldn't read its claude.ai connectors: {why}")),
    }
}

/// The connectors Claude Code loads from the answer, the eligible ones, by name.
fn connected(answer: &Value) -> Vec<Server> {
    let mut out: Vec<Server> = answer["data"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|c| c["eligible"] == Value::Bool(true))
        .filter_map(|c| {
            let name = c["display_name"].as_str().filter(|n| !n.trim().is_empty())?;
            let tools = c["tools"].as_array().into_iter().flatten().filter_map(|t| t["name"].as_str().map(str::to_string)).collect();
            Some(Server { tools, ..Server::new(name, CONNECTOR) })
        })
        .collect();
    out.sort_by_key(|s| s.name.to_lowercase());
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn only_the_connectors_claude_code_loads_are_listed() {
        // As 2.1.306's endpoint answered, cut short.
        let answer = json!({ "data": [
            { "type": "mcp_server", "display_name": "Claude Docs", "tools": null, "eligible": true, "eligibility_reason": "not_an_org_connector" },
            { "type": "mcp_server", "display_name": "Gmail", "tools": [{ "name": "search_threads", "effective_max_permission": "allow" }, { "name": "get_thread" }], "eligible": true, "eligibility_reason": "connected" },
            { "type": "mcp_server", "display_name": "Google Drive", "tools": null, "eligible": false, "eligibility_reason": "never_connected_no_auto_connect" },
        ], "next_page": null });
        let got = connected(&answer);
        assert_eq!(got.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["Claude Docs", "Gmail"]);
        assert_eq!(got[1].tools, ["search_threads", "get_thread"]);
        let unsorted = json!({ "data": [{ "display_name": "Slack", "eligible": true }, { "display_name": "atlas", "eligible": true }] });
        assert_eq!(connected(&unsorted).iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["atlas", "Slack"], "by name, whatever the case");
        assert!(connected(&json!({})).is_empty());
    }
}
