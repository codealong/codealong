//! The mode a session Codealong runs is put in as it starts and as it's picked up again, where
//! its agent starts every session in a default of its own whatever the account's settings start
//! its terminal UI in (`Adapter::acp_start_mode`: Antigravity's `agentMode`, which `agy acp`'s
//! `session/new` passes over and `session/resume` puts back to `default`).

use anyhow::Result;
use serde_json::Value;

use super::{current_option, offered_option, set_config, Agent};

/// Puts session `id` in the mode its account's settings start a session in, as its agent offers
/// it and when it isn't in it already. Returns the agent's answer about the session (`answer`,
/// from `session/new` or a pick-up) with the options it lists after.
pub(super) fn start_mode(ag: &Agent, id: &str, mut answer: Value) -> Result<Value> {
    let Some((harness, home)) = &ag.who else { return Ok(answer) };
    let Some(mode) = crate::harness::adapter(*harness).acp_start_mode(home) else { return Ok(answer) };
    let options = answer.get("configOptions").cloned().unwrap_or(Value::Null);
    if current_option(&options, "mode").as_deref() == Some(mode.as_str()) {
        return Ok(answer);
    }
    let Some(config_id) = offered_option(&options, "mode", &mode) else { return Ok(answer) };
    let options = set_config(ag, id, options, &config_id, &mode)?;
    if let Some(o) = answer.as_object_mut() {
        o.insert("configOptions".into(), options);
    }
    Ok(answer)
}
