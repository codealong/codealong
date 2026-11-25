//! A message put into the turn a session Codealong runs is taking, where its agent takes that:
//! codex-acp's `_session/steering {sessionId, prompt}` (`turn/steer` underneath), offered in
//! `initialize`'s answer as `_meta.steering.supported` (1.13.1), which answers `injected`,
//! `startedNewTurn` when the turn had ended as it came, or `failed`. Only the answers to
//! questions its agent asked aside go this way (codex/aside.rs, actions.rs `answer_aside`):
//! Codex waits on them in its turn, and its terminal UI steers them in. A prompt sent while a
//! turn runs waits for the next.

use anyhow::Result;
use serde_json::{json, Value};

use super::{check_not_handing_over, state, REQUEST_TIMEOUT};

/// Puts `text` into the turn the session `key` is taking, and says how that went; None when it
/// isn't taking one, its agent doesn't take that, or the agent couldn't, for the caller to send
/// it as its next prompt instead. A turn started with it (`startedNewTurn`) is one its agent
/// took by itself, which its record shows running (acp.rs `overlay`).
pub fn steer(key: &str, text: &str) -> Result<Option<String>> {
    check_not_handing_over()?;
    let (ag, id) = {
        let st = state();
        let Some(h) = st.sessions.get(key).filter(|h| h.running && h.loaded) else { return Ok(None) };
        let Some(ag) = st.agents.get(&h.account).cloned() else { return Ok(None) };
        (ag, h.id.clone())
    };
    if !ag.can("steer") {
        return Ok(None);
    }
    let answer = ag.request("_session/steering", json!({ "sessionId": id, "prompt": [{ "type": "text", "text": text }] }), Some(REQUEST_TIMEOUT))?;
    Ok(match answer.get("outcome").and_then(Value::as_str) {
        Some("injected") => Some("Sent into the turn it's taking.".into()),
        Some("startedNewTurn") => Some("Sent: its turn had just ended, so it starts another with it.".into()),
        _ => None,
    })
}
