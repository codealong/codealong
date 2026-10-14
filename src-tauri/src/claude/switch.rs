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
/// it was switched to (checked the same day). One switch at a time, so one doesn't take another's
/// saved default for the account's own; another asked meanwhile is refused rather than kept
/// waiting, when the session it's for may have gone on to a turn. Answers what it's on now: a
/// level over the account's cap is set to the cap. `records` are the session's own transcript,
/// where the command writes, even for one whose conversation another's record holds until then.
pub(super) fn typed(account: &AccountCfg, session: &Session, records: &dyn Fn() -> Vec<PathBuf>, to: Switch, text: &str, typ: &mut dyn FnMut(&str) -> Result<bool>) -> Result<Option<String>> {
    static SWITCHING: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _one_at_a_time = match SWITCHING.try_lock() {
        Ok(held) => held,
        Err(std::sync::TryLockError::Poisoned(e)) => e.into_inner(),
        Err(std::sync::TryLockError::WouldBlock) => bail!("Another switch is under way: try again in a few seconds."),
    };
    // A session sent no prompt yet may have no transcript: the command starts one.
    let from = records().first().and_then(|t| std::fs::metadata(t).ok()).map_or(0, |m| m.len());
    let settings = account.home_path().join("settings.json");
    let before = settings_now(&settings);
    // A typing that failed may still have gone in (`send::MayHaveArrived`): what it said and saved
    // is looked for, and only a question it surely brought up is answered.
    let typed = typ(text);
    if matches!(typed, Ok(false)) {
        return Ok(None);
    }
    let mut said = said_after(records, from, SAID_WAIT);
    let mut pressed = Ok(());
    if said.is_none() && typed.is_ok() {
        pressed = press_return(session);
        if pressed.is_ok() {
            said = said_after(records, from, ANSWERED_WAIT);
        }
    }
    // What it saved is put back however the switch went.
    if let (Some(before), Some(saved)) = (before, settings_now(&settings)) {
        put_back_defaults(&settings, &before, &saved, to)?;
    }
    typed?;
    pressed?;
    let said = said.ok_or_else(|| anyhow!("Claude Code didn't say it took the switch: it may be asking about it in its terminal."))?;
    let now = match to {
        Switch::Model(model) => super::transcript::model_set(&said).map(|_| model.to_string()),
        Switch::Effort(_) => super::transcript::effort_set(&said).flatten(),
    };
    let Some(now) = now else {
        let words = said.trim_start_matches("<local-command-stdout>").trim_end_matches("</local-command-stdout>");
        bail!("Claude Code didn't switch it: {}", words.trim());
    };
    Ok(Some(now))
}

/// What Claude Code wrote of a local command's outcome in the session's transcript (`records`'
/// first) after byte `from`, waiting up to `wait` for it: the first such line's text,
/// `<local-command-stdout>` and all.
fn said_after(records: &dyn Fn() -> Vec<PathBuf>, from: u64, wait: Duration) -> Option<String> {
    let deadline = Instant::now() + wait;
    loop {
        if let Some(tail) = records().first().and_then(|t| crate::util::read_tail(t, from).ok()) {
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

/// Puts back the defaults a switch saved, from the account's settings before it (`before`) and
/// after (`saved`), once Claude Code reads a change to them again (`ECHO_WAIT`): for `/model`
/// the model, for `/effort` the level at the top and each model's under `modelSettings`, each as
/// it was, an entry the switch added taken out again. A default changed again since is left as it
/// is now, and everything else keeps its text and its place. Settings the switch saved nothing in
/// aren't waited on or written.
fn put_back_defaults(settings: &Path, before: &Value, saved: &Value, to: Switch) -> Result<()> {
    if with_defaults_of(&saved.to_string(), before, saved, to)?.is_none() {
        return Ok(());
    }
    std::thread::sleep(ECHO_WAIT);
    let Ok(text) = std::fs::read_to_string(settings) else { return Ok(()) };
    let Some(next) = with_defaults_of(&text, before, saved, to)? else { return Ok(()) };
    crate::util::write_whole(settings, &next)
}

/// `text` with the defaults a switch's command saved (those it can save that differ between
/// `before` and `saved`) put back to `before`'s where they're still what it saved, or None when
/// there are none.
fn with_defaults_of(text: &str, before: &Value, saved: &Value, to: Switch) -> Result<Option<String>> {
    let now: Value = serde_json::from_str(text)?;
    let level = |v: &Value, model: Option<&str>| -> Option<Value> {
        match model {
            None => v.get(EFFORT_LEVEL).cloned(),
            Some(m) => v.get(MODEL_SETTINGS)?.get(m)?.get(EFFORT_LEVEL).cloned(),
        }
    };
    let models = |v: &Value| -> Vec<String> { v.get(MODEL_SETTINGS).and_then(Value::as_object).map(|o| o.keys().cloned().collect()).unwrap_or_default() };
    let (saves, saves_levels) = match to {
        Switch::Model(_) => (MODEL, false),
        Switch::Effort(_) => (EFFORT_LEVEL, true),
    };
    let mut tops: Vec<&str> = Vec::new();
    if saved.get(saves) != before.get(saves) && now.get(saves) == saved.get(saves) {
        tops.push(saves);
    }
    let mut changed: Vec<String> = Vec::new();
    for model in models(saved).into_iter().chain(models(before)).filter(|_| saves_levels) {
        let (was, put) = (level(before, Some(&model)), level(saved, Some(&model)));
        if put != was && level(&now, Some(&model)) == put && !changed.contains(&model) {
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
    const EFFORT: Switch = Switch::Effort("high");
    const MODEL_SWITCH: Switch = Switch::Model("sonnet");

    /// The settings `text` once what a switch saved in them (`text` itself) is put back to
    /// `before`'s.
    fn put_back(text: &str, before: &Value, to: Switch) -> Option<String> {
        with_defaults_of(text, before, &serde_json::from_str(text).unwrap(), to).unwrap()
    }

    #[test]
    fn puts_back_the_level_effort_saved_and_leaves_the_rest_as_it_was() {
        let before: Value = serde_json::from_str(SETTINGS).unwrap();
        // As `/effort high` in a Sonnet session wrote it: an entry for its model.
        let saved = SETTINGS.replace("      \"effortLevel\": \"xhigh\"\n    }\n  },", "      \"effortLevel\": \"xhigh\"\n    },\n    \"claude-sonnet-5-5\": {\n      \"effortLevel\": \"high\"\n    }\n  },");
        assert_eq!(put_back(&saved, &before, EFFORT).as_deref(), Some(SETTINGS));
        // A level it changed goes back to what it was.
        let changed = SETTINGS.replace("    \"claude-opus-5-5\": {\n      \"effortLevel\": \"xhigh\"", "    \"claude-opus-5-5\": {\n      \"effortLevel\": \"low\"");
        assert_eq!(put_back(&changed, &before, EFFORT).as_deref(), Some(SETTINGS));
        let top = SETTINGS.replace("  \"effortLevel\": \"xhigh\",", "  \"effortLevel\": \"medium\",");
        assert_eq!(put_back(&top, &before, EFFORT).as_deref(), Some(SETTINGS));
        // Levels as they were: nothing to write.
        assert_eq!(put_back(SETTINGS, &before, EFFORT), None);
    }

    #[test]
    fn takes_out_what_effort_added_to_settings_without_levels() {
        let before = json!({ "model": "opus" });
        let saved = "{\n  \"model\": \"opus\",\n  \"modelSettings\": {\n    \"claude-opus-5-5\": {\n      \"effortLevel\": \"high\"\n    }\n  }\n}";
        assert_eq!(put_back(saved, &before, EFFORT).as_deref(), Some("{\n  \"model\": \"opus\"\n}"));
        // A top-level level it took out comes back.
        let cleared = "{\n  \"model\": \"opus\"\n}\n";
        assert_eq!(put_back(cleared, &json!({ "model": "opus", "effortLevel": "xhigh" }), EFFORT).as_deref(), Some("{\n  \"model\": \"opus\",\n  \"effortLevel\": \"xhigh\"\n}\n"));
    }

    #[test]
    fn puts_back_the_model_model_saved() {
        let before: Value = serde_json::from_str(SETTINGS).unwrap();
        // As `/model sonnet` wrote it: the default for new sessions, in its place.
        let saved = SETTINGS.replace("  \"model\": \"opus\",", "  \"model\": \"sonnet\",");
        assert_eq!(put_back(&saved, &before, MODEL_SWITCH).as_deref(), Some(SETTINGS));
        // Settings with no model of their own lose the one it added.
        let added = "{\n  \"model\": \"sonnet\",\n  \"theme\": \"dark\"\n}\n";
        assert_eq!(put_back(added, &json!({ "theme": "dark" }), MODEL_SWITCH).as_deref(), Some("{\n  \"theme\": \"dark\"\n}\n"));
    }

    #[test]
    fn leaves_a_default_the_switch_didnt_save() {
        let before: Value = serde_json::from_str(SETTINGS).unwrap();
        let saved: Value = serde_json::from_str(&SETTINGS.replace("  \"model\": \"opus\",", "  \"model\": \"sonnet\",")).unwrap();
        // You picked Haiku in its own picker meanwhile: that stays.
        let since = SETTINGS.replace("  \"model\": \"opus\",", "  \"model\": \"haiku\",");
        assert_eq!(with_defaults_of(&since, &before, &saved, MODEL_SWITCH).unwrap(), None);
        // A model changed while an effort switch waited isn't the effort switch's to put back.
        let both = SETTINGS.replace("  \"model\": \"opus\",", "  \"model\": \"haiku\",").replace("  \"effortLevel\": \"xhigh\",", "  \"effortLevel\": \"high\",");
        let kept = SETTINGS.replace("  \"model\": \"opus\",", "  \"model\": \"haiku\",");
        assert_eq!(put_back(&both, &before, EFFORT).as_deref(), Some(kept.as_str()));
        // Switches one after another each put back only what they saved, so the account's own
        // comes back whatever their order.
        let first = SETTINGS.replace("  \"model\": \"opus\",", "  \"model\": \"sonnet\",");
        let second: Value = serde_json::from_str(&first.replace("  \"effortLevel\": \"xhigh\",", "  \"effortLevel\": \"low\",")).unwrap();
        let first: Value = serde_json::from_str(&first).unwrap();
        let after_second = with_defaults_of(&second.to_string(), &first, &second, EFFORT).unwrap().unwrap();
        let after_first = with_defaults_of(&after_second, &before, &first, MODEL_SWITCH).unwrap().unwrap();
        assert_eq!(serde_json::from_str::<Value>(&after_first).unwrap(), before);
    }

    #[test]
    fn finds_what_a_switch_said_after_it_was_typed() {
        let dir = crate::store::test_dir("switch-said");
        let path = dir.join("t.jsonl");
        let records = || vec![path.clone()].into_iter().filter(|p| p.is_file()).collect();
        // A session with no transcript yet says nothing until the command starts one.
        assert_eq!(said_after(&records, 0, Duration::ZERO), None);
        let line = |content: &str| format!("{}\n", json!({ "type": "user", "message": { "role": "user", "content": content } }));
        std::fs::write(&path, line("<local-command-stdout>Set effort level to low (this session only): Quick</local-command-stdout>")).unwrap();
        let from = std::fs::metadata(&path).unwrap().len();
        // What came before the switch isn't what it said, and nothing yet is nothing.
        assert_eq!(said_after(&records, from, Duration::ZERO), None);
        let caveat = line("<local-command-caveat>The command below was run directly in Claude Code.</local-command-caveat>");
        let said = "<local-command-stdout>Set effort level to high (saved as your default for new sessions): Comprehensive</local-command-stdout>";
        std::fs::write(&path, format!("{}{caveat}{}", std::fs::read_to_string(&path).unwrap(), line(said))).unwrap();
        assert_eq!(said_after(&records, from, Duration::ZERO).as_deref(), Some(said));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
