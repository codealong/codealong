//! A model or an effort level switched in a running session by its command typed into its
//! terminal (`send::switch_model`, `switch_effort`): the question Claude Code asks before it takes
//! one, and the defaults `/model` and `/effort` save beyond the session.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Result};
use serde_json::value::RawValue;
use serde_json::Value;

use crate::config::AccountCfg;
use crate::harness::Switch;
use crate::jsonedit::Node;
use crate::model::Session;

/// How long Claude Code is given to write what a switch did before its question is taken to be
/// up, and then to write it once the question is answered.
const SAID_WAIT: Duration = Duration::from_millis(1500);
const ANSWERED_WAIT: Duration = Duration::from_secs(5);

/// Where Claude Code keeps the defaults a switch saves (2.1.299, typed in its terminal): the
/// model new sessions start on (`/model opus`: "Set model to Opus 5.5 and saved as your default
/// for new sessions"), and the effort level a model starts on, under `modelSettings` (`/effort
/// high`: "Set effort level to high (saved as your default for new sessions)"), or for every
/// model at the top.
const MODEL: &str = "model";
const MODEL_SETTINGS: &str = "modelSettings";
const EFFORT_LEVEL: &str = "effortLevel";

/// How long after Claude Code saves a default it's put back. Its settings watcher (chokidar)
/// reports a change once the file has held still for a second, polling every half second, and
/// takes the first change within 5 seconds of a write of its own for that write's echo
/// (2.1.299's `awaitWriteFinish {stabilityThreshold: 1000}` and `internalWrites`, `rin`). A
/// change made sooner goes in with its own and is never read, and its next write of the settings
/// saves the default again (seen on 2026-10-14).
const ECHO_WAIT: Duration = Duration::from_secs(3);

/// Types a switch by `typ` and sees it through. While the conversation is cached for what it's
/// on, Claude Code asks first ("Switch model?", "Change effort level?", 2.1.299; checked in tmux
/// on 2026-10-14), Yes picked, and writes what it did (`<local-command-stdout>`) only once that's
/// answered; so when nothing's written in a moment, Return answers Yes, as picking the switch
/// here meant. Both commands also save what they switch to as the account's default for new
/// sessions (`/effort` only low to xhigh): what they saved is put back, and the session keeps what
/// it was switched to (checked the same day).
pub(super) fn typed(account: &AccountCfg, session: &Session, records: &[PathBuf], to: Switch, text: &str, typ: &mut dyn FnMut(&str) -> Result<bool>) -> Result<bool> {
    let transcript = records.first();
    let from = transcript.and_then(|t| std::fs::metadata(t).ok()).map_or(0, |m| m.len());
    let settings = account.home_path().join("settings.json");
    let before = settings_now(&settings);
    if !typ(text)? {
        return Ok(false);
    }
    // A session that hasn't written its transcript yet has nothing to say it took it.
    let Some(transcript) = transcript else { return Ok(true) };
    let said = match said_after(transcript, from, SAID_WAIT) {
        Some(said) => said,
        None => {
            press_return(session)?;
            said_after(transcript, from, ANSWERED_WAIT).ok_or_else(|| anyhow!("Claude Code didn't say it took the switch: it may be asking about it in its terminal."))?
        }
    };
    if let Some(before) = before {
        put_back_defaults(&settings, &before)?;
    }
    let took = match to {
        Switch::Model(_) => super::transcript::model_set(&said).is_some(),
        Switch::Effort(_) => matches!(super::transcript::effort_set(&said), Some(Some(_))),
    };
    if !took {
        let words = said.trim_start_matches("<local-command-stdout>").trim_end_matches("</local-command-stdout>");
        bail!("Claude Code didn't switch it: {}", words.trim());
    }
    Ok(true)
}

/// What Claude Code wrote of a local command's outcome in `transcript` after byte `from`, waiting
/// up to `wait` for it: the first such line's text, `<local-command-stdout>` and all.
fn said_after(transcript: &Path, from: u64, wait: Duration) -> Option<String> {
    let deadline = Instant::now() + wait;
    loop {
        if let Ok(tail) = crate::util::read_tail(transcript, from) {
            let said = tail.text().lines().find_map(|line| {
                let v: Value = serde_json::from_str(line).ok()?;
                let content = v.pointer("/message/content")?;
                let text = content.as_str().or_else(|| content.as_array()?.iter().find_map(|i| i.get("text")?.as_str()))?;
                (v.get("type")?.as_str()? == "user" && text.starts_with("<local-command-stdout>")).then(|| text.to_string())
            });
            if said.is_some() {
                return said;
            }
        }
        if Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Presses Return in the terminal a session runs in, to answer the question up there.
fn press_return(session: &Session) -> Result<()> {
    let pid = session.pid.ok_or_else(|| anyhow!("It isn't running any more."))?;
    let Some(host) = crate::launch::terminal_host(pid, Path::new(&session.cwd))? else {
        bail!("Claude Code may be asking about the switch in its terminal, where Codealong can't press keys ({}): answer it there.", crate::launch::presses_keys_in());
    };
    crate::launch::press_key(&host, crate::launch::Key::Return)
}

/// The account's settings as they are now: `{}` when it has none, None when they don't parse,
/// and then nothing is put back.
fn settings_now(settings: &Path) -> Option<Value> {
    match std::fs::read_to_string(settings) {
        Ok(text) => serde_json::from_str(&text).ok(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Some(Value::Object(Default::default())),
        Err(_) => None,
    }
}

/// Puts back the defaults the account's settings had before a switch (`before`), once Claude
/// Code reads a change to them again (`ECHO_WAIT`): the model, the effort level at the top and
/// each model's under `modelSettings`, each as it was, an entry the switch added taken out again.
/// Everything else keeps its text and its place; settings whose defaults are as they were aren't
/// waited on or written.
fn put_back_defaults(settings: &Path, before: &Value) -> Result<()> {
    let saved = |text: &str| with_defaults_of(text, before).ok().flatten().is_some();
    if !std::fs::read_to_string(settings).is_ok_and(|text| saved(&text)) {
        return Ok(());
    }
    std::thread::sleep(ECHO_WAIT);
    let Ok(text) = std::fs::read_to_string(settings) else { return Ok(()) };
    let Some(next) = with_defaults_of(&text, before)? else { return Ok(()) };
    crate::util::write_whole(settings, &next)
}

/// `text` with the defaults a switch saves put back to `before`'s, or None when they're the same.
fn with_defaults_of(text: &str, before: &Value) -> Result<Option<String>> {
    let now: Value = serde_json::from_str(text)?;
    let level = |v: &Value, model: Option<&str>| -> Option<Value> {
        match model {
            None => v.get(EFFORT_LEVEL).cloned(),
            Some(m) => v.get(MODEL_SETTINGS)?.get(m)?.get(EFFORT_LEVEL).cloned(),
        }
    };
    let models = |v: &Value| -> Vec<String> { v.get(MODEL_SETTINGS).and_then(Value::as_object).map(|o| o.keys().cloned().collect()).unwrap_or_default() };
    let mut tops: Vec<&str> = Vec::new();
    for key in [MODEL, EFFORT_LEVEL] {
        if now.get(key) != before.get(key) {
            tops.push(key);
        }
    }
    let mut changed: Vec<String> = Vec::new();
    for model in models(&now).into_iter().chain(models(before)) {
        if level(&now, Some(&model)) != level(before, Some(&model)) && !changed.contains(&model) {
            changed.push(model);
        }
    }
    if tops.is_empty() && changed.is_empty() {
        return Ok(None);
    }
    let raw = |v: &Value| -> Result<Node> { Ok(Node::Raw(RawValue::from_string(serde_json::to_string(v)?)?)) };
    let mut top = Node::entries(text)?;
    for key in tops {
        match (top.iter().rposition(|(k, _)| k == key), before.get(key)) {
            (Some(at), Some(was)) => top[at].1 = raw(was)?,
            (Some(_), None) => top.retain(|(k, _)| k != key),
            (None, Some(was)) => top.push((key.to_string(), raw(was)?)),
            (None, None) => {}
        }
    }
    for model in &changed {
        let was = level(before, Some(model));
        let had_entry = before.get(MODEL_SETTINGS).and_then(|m| m.get(model)).is_some();
        let all = Node::object_in(&mut top, MODEL_SETTINGS)?;
        let entry = Node::object_in(all, model)?;
        match &was {
            Some(was) => match entry.iter_mut().rev().find(|(k, _)| k == EFFORT_LEVEL) {
                Some((_, v)) => *v = raw(was)?,
                None => entry.push((EFFORT_LEVEL.to_string(), raw(was)?)),
            },
            None => entry.retain(|(k, _)| k != EFFORT_LEVEL),
        }
        if entry.is_empty() && !had_entry {
            all.retain(|(k, _)| k != model);
        }
    }
    if before.get(MODEL_SETTINGS).is_none() && matches!(top.iter().rev().find(|(k, _)| k == MODEL_SETTINGS), Some((_, Node::Object(o))) if o.is_empty()) {
        top.retain(|(k, _)| k != MODEL_SETTINGS);
    }
    // Claude Code writes the file as JSON.stringify(settings, null, 2) does, which serde_json's
    // pretty printer matches.
    let mut out = serde_json::to_string_pretty(&Node::Object(top))?;
    if text.ends_with('\n') {
        out.push('\n');
    }
    Ok(Some(out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const SETTINGS: &str = "{\n  \"model\": \"opus\",\n  \"effortLevel\": \"xhigh\",\n  \"modelSettings\": {\n    \"claude-opus-5-5\": {\n      \"effortLevel\": \"xhigh\"\n    }\n  },\n  \"permissions\": {\n    \"defaultMode\": \"bypassPermissions\"\n  }\n}\n";

    #[test]
    fn puts_back_the_level_effort_saved_and_leaves_the_rest_as_it_was() {
        let before: Value = serde_json::from_str(SETTINGS).unwrap();
        // As `/effort high` in a Sonnet session wrote it: an entry for its model.
        let saved = SETTINGS.replace("      \"effortLevel\": \"xhigh\"\n    }\n  },", "      \"effortLevel\": \"xhigh\"\n    },\n    \"claude-sonnet-5-5\": {\n      \"effortLevel\": \"high\"\n    }\n  },");
        assert_eq!(with_defaults_of(&saved, &before).unwrap().as_deref(), Some(SETTINGS));
        // A level it changed goes back to what it was.
        let changed = SETTINGS.replace("    \"claude-opus-5-5\": {\n      \"effortLevel\": \"xhigh\"", "    \"claude-opus-5-5\": {\n      \"effortLevel\": \"low\"");
        assert_eq!(with_defaults_of(&changed, &before).unwrap().as_deref(), Some(SETTINGS));
        let top = SETTINGS.replace("  \"effortLevel\": \"xhigh\",", "  \"effortLevel\": \"medium\",");
        assert_eq!(with_defaults_of(&top, &before).unwrap().as_deref(), Some(SETTINGS));
        // Levels as they were: nothing to write.
        assert_eq!(with_defaults_of(SETTINGS, &before).unwrap(), None);
    }

    #[test]
    fn takes_out_what_effort_added_to_settings_without_levels() {
        let before = json!({ "model": "opus" });
        let saved = "{\n  \"model\": \"opus\",\n  \"modelSettings\": {\n    \"claude-opus-5-5\": {\n      \"effortLevel\": \"high\"\n    }\n  }\n}";
        assert_eq!(with_defaults_of(saved, &before).unwrap().as_deref(), Some("{\n  \"model\": \"opus\"\n}"));
        // A top-level level it took out comes back.
        let cleared = "{\n  \"model\": \"opus\"\n}\n";
        assert_eq!(with_defaults_of(cleared, &json!({ "model": "opus", "effortLevel": "xhigh" })).unwrap().as_deref(), Some("{\n  \"model\": \"opus\",\n  \"effortLevel\": \"xhigh\"\n}\n"));
    }

    #[test]
    fn puts_back_the_model_model_saved() {
        let before: Value = serde_json::from_str(SETTINGS).unwrap();
        // As `/model sonnet` wrote it: the default for new sessions, in its place.
        let saved = SETTINGS.replace("  \"model\": \"opus\",", "  \"model\": \"sonnet\",");
        assert_eq!(with_defaults_of(&saved, &before).unwrap().as_deref(), Some(SETTINGS));
        // Settings with no model of their own lose the one it added.
        let added = "{\n  \"model\": \"sonnet\",\n  \"theme\": \"dark\"\n}\n";
        assert_eq!(with_defaults_of(added, &json!({ "theme": "dark" })).unwrap().as_deref(), Some("{\n  \"theme\": \"dark\"\n}\n"));
    }

    #[test]
    fn finds_what_a_switch_said_after_it_was_typed() {
        let dir = crate::store::test_dir("switch-said");
        let path = dir.join("t.jsonl");
        let line = |content: &str| format!("{}\n", json!({ "type": "user", "message": { "role": "user", "content": content } }));
        std::fs::write(&path, line("<local-command-stdout>Set effort level to low (this session only): Quick</local-command-stdout>")).unwrap();
        let from = std::fs::metadata(&path).unwrap().len();
        // What came before the switch isn't what it said, and nothing yet is nothing.
        assert_eq!(said_after(&path, from, Duration::ZERO), None);
        let caveat = line("<local-command-caveat>The command below was run directly in Claude Code.</local-command-caveat>");
        let said = "<local-command-stdout>Set effort level to high (saved as your default for new sessions): Comprehensive</local-command-stdout>";
        std::fs::write(&path, format!("{}{caveat}{}", std::fs::read_to_string(&path).unwrap(), line(said))).unwrap();
        assert_eq!(said_after(&path, from, Duration::ZERO).as_deref(), Some(said));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
