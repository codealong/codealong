//! Questions a session Codealong runs asked without waiting on them (`Adapter::asked_aside`,
//! Codex's `request_user_input_async`, codex/aside.rs): what its record says it asks, and the
//! message that answers them, which goes into the turn it's taking where its agent takes that
//! (`acp::steer`), else as its next prompt.

use codealong_peer::api;
use serde_json::Value;

use super::{cfg, sender_of, Call};
use crate::engine::Handle;
use crate::util::words;
use crate::{acp, harness, send};

/// What the session `key`, which Codealong runs, waits on you for: what its agent asks in the
/// process (`acp::pending`), else what it asked without waiting on it, from its record.
pub(super) fn hosted_pending(engine: &Handle, key: &str) -> Result<Option<api::Pending>, String> {
    if let Some(p) = acp::pending(key) {
        return Ok(Some(p));
    }
    let (cfg, board) = (cfg()?, engine.board());
    let Ok((s, account)) = harness::find(&board, &cfg, key) else { return Ok(None) };
    Ok(harness::adapter(s.harness).asked_aside(&harness::Ctx { account }, s))
}

/// Answers the questions `id` with `answers`, or declines them (a deny without them), or sends
/// what you wrote with a deny (`saying`), which answers them too. None when `id` names nothing
/// asked aside, or it's allowed without answers.
pub(super) fn answer(engine: &Handle, c: &Call, key: &str, id: &str, answer: api::Answer, answers: Option<&Value>, saying: Option<&str>) -> Result<Option<Value>, String> {
    if answers.is_none() && answer != api::Answer::Deny {
        return Ok(None);
    }
    let (cfg, board) = (cfg()?, engine.board());
    let (s, account) = harness::find(&board, &cfg, key).map_err(words)?;
    let Some(text) = harness::adapter(s.harness).aside_reply(&harness::Ctx { account }, s, id, answers) else { return Ok(None) };
    let text = match saying {
        Some(yours) => yours.to_string(),
        None => text.map_err(words)?,
    };
    let said = match acp::steer(key, &text).map_err(words)? {
        Some(said) => said,
        None => send::send_here(engine, key, &text, sender_of(c.from), false).map_err(words)?.message,
    };
    Ok(Some(Value::String(said)))
}
