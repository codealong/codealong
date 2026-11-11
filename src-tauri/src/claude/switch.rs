//! A model or an effort level switched in a running session by its command typed into its
//! terminal, or pasted into a background job's box by its account's background service
//! (`send::switch_model`, `switch_effort`): the question Claude Code asks before it takes one, and
//! the defaults `/model` and `/effort` save beyond the session.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use anyhow::{anyhow, bail, Result};
use serde_json::value::RawValue;
use serde_json::Value;

use crate::config::AccountCfg;
use crate::harness::{Switch, Switched};
use crate::jsonedit::Node;
use crate::model::Session;

/// How long Claude Code is given to write what a switch did before its question is taken to be
/// up, and then to write it once the question is answered; a test's stand-in writes at once.
const SAID_WAIT: Duration = if cfg!(test) { Duration::from_millis(200) } else { Duration::from_millis(1500) };
const ANSWERED_WAIT: Duration = if cfg!(test) { Duration::from_millis(300) } else { Duration::from_secs(5) };

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

/// The variable that fixes a session's effort level for its whole run: while it names another
/// level than one asked for, `/effort` sets nothing in the session (it says "Not applied", or
/// "overrides this session"), though it still saves the level asked for as the default (2.1.322's
/// code).
const EFFORT_VAR: &str = "CLAUDE_CODE_EFFORT_LEVEL";

/// Types a switch by `typ` and sees it through. While the conversation is cached for what it's
/// on, Claude Code asks first ("Switch model?", "Change effort level?", 2.1.299; checked in tmux
/// on 2026-10-14), Yes picked, and writes what it did (`<local-command-stdout>`) only once that's
/// answered; so when nothing's written in a moment, Return answers Yes, as picking the switch
/// here meant, unless it waits on something else (`press_return`). Both commands also save what
/// they switch to as the account's default for new sessions (`/effort` only low to xhigh, and not
/// a level over the session's cap, which it sets to the cap): what they saved is put back, and
/// the session keeps what it was switched to (checked the same day). One switch at a time, so one
/// doesn't take another's saved default for the account's own; another asked meanwhile is refused
/// rather than kept waiting, when the session it's for may have gone on to a turn. Answers what
/// it's on now: a level over the cap is set to the cap. `records` are the session's own
/// transcript, where the command writes, even for one whose conversation another's record holds
/// until then.
///
/// Mid-turn the commands run at once too (2.1.322, in tmux on 2026-11-10): `/model` to another
/// model asks the same question under the running turn, and the turn's next step goes on what
/// they set. But what they did shows only on screen, and nothing reaches the transcript, then or
/// after the turn; so there the default they save says they took it, settings.json being written
/// again whenever one is saved, even with nothing in it changed. `/effort` asks nothing mid-turn,
/// so no Return is pressed for it, and a level that saves no default (max, or one over the cap)
/// leaves nothing that says what it set: that's `Switched::Unsaid`. A level its environment or
/// settings fix (`fixed_effort`) isn't typed mid-turn, where nothing would say it wasn't applied.
pub(super) fn typed(account: &AccountCfg, session: &Session, records: &dyn Fn() -> Vec<PathBuf>, to: Switch, text: &str, typ: &mut dyn FnMut(&str) -> Result<bool>) -> Result<Option<Switched>> {
    static SWITCHING: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _one_at_a_time = match SWITCHING.try_lock() {
        Ok(held) => held,
        Err(std::sync::TryLockError::Poisoned(e)) => e.into_inner(),
        Err(std::sync::TryLockError::WouldBlock) => bail!("Another switch is under way: try again in a few seconds."),
    };
    see_through(account, session, records, to, text, typ)
}

/// `typed`, while no other switch is under way.
fn see_through(account: &AccountCfg, session: &Session, records: &dyn Fn() -> Vec<PathBuf>, to: Switch, text: &str, typ: &mut dyn FnMut(&str) -> Result<bool>) -> Result<Option<Switched>> {
    // A session sent no prompt yet may have no transcript: the command starts one.
    let from = records().first().and_then(|t| std::fs::metadata(t).ok()).map_or(0, |m| m.len());
    let settings = account.home_path().join("settings.json");
    let before = settings_now(&settings);
    // Mid-turn the transcript says nothing, and the settings written again do.
    let mid_turn = crate::harness::turn_running(session);
    let saves = mid_turn.then(|| (settings.as_path(), written_at(&settings)));
    let asks = !(mid_turn && matches!(to, Switch::Effort(_)));
    if !asks {
        let launched = session.pid.and_then(crate::procs::environment).and_then(|mut env| env.remove(EFFORT_VAR));
        let home = account.home_path();
        let files = std::iter::once(super::trust::state_file(&home)).chain(super::mode::settings_files(&home, Path::new(&session.cwd)));
        if let Some(fixed) = fixed_effort(launched, files.filter_map(|f| std::fs::read_to_string(f).ok()), to.value()) {
            bail!("{EFFORT_VAR}={fixed}, from its environment or its settings, keeps Claude Code from switching its effort.");
        }
    }
    // A typing that failed may still have gone in (`send::MayHaveArrived`): what it said and saved
    // is looked for, and only a question it surely brought up is answered.
    let typed = typ(text);
    if matches!(typed, Ok(false)) {
        return Ok(None);
    }
    // With nothing to answer, the whole wait goes on looking.
    let mut sign = sign_after(records, from, saves, if asks { SAID_WAIT } else { SAID_WAIT + ANSWERED_WAIT });
    let mut pressed = Ok(());
    if sign.is_none() && typed.is_ok() && asks {
        pressed = press_return(account, session);
        if pressed.is_ok() {
            sign = sign_after(records, from, saves, ANSWERED_WAIT);
        }
    }
    // What it saved is put back however the switch went.
    if let (Some(before), Some(saved)) = (before, settings_now(&settings)) {
        put_back_defaults(&settings, &before, &saved, to)?;
    }
    typed?;
    pressed?;
    let said = match sign {
        Some(Sign::Said(said)) => said,
        Some(Sign::Saved) => return Ok(Some(Switched::On(to.value().to_string()))),
        None if !asks => return Ok(Some(Switched::Unsaid)),
        None => bail!("Claude Code didn't say it took the switch: it may be asking about it in its terminal."),
    };
    let now = match to {
        Switch::Model(model) => super::transcript::model_set(&said).map(|_| model.to_string()),
        Switch::Effort(_) => super::transcript::effort_set(&said).flatten(),
    };
    let Some(now) = now else {
        let words = said.trim_start_matches("<local-command-stdout>").trim_end_matches("</local-command-stdout>");
        bail!("Claude Code didn't switch it: {}", words.trim());
    };
    Ok(Some(Switched::On(now)))
}

/// The level a session's effort is fixed at, where it isn't `level`, as Claude Code takes
/// `EFFORT_VAR`: from the environment it started with (`launched`), then the `env` of its
/// account's state file and of each settings file (`texts`, lowest first), each over the one
/// before. 2.1.322's code puts those in its own environment after launch, which the kernel
/// doesn't show another process. `auto` and `unset` there fix it too; an empty value fixes none.
fn fixed_effort(launched: Option<String>, texts: impl Iterator<Item = String>, level: &str) -> Option<String> {
    let set = texts.filter_map(|t| serde_json::from_str::<Value>(&t).ok()?.get("env")?.get(EFFORT_VAR)?.as_str().map(str::to_string)).last();
    set.or(launched).map(|v| v.trim().to_string()).filter(|v| !v.is_empty() && !v.eq_ignore_ascii_case(level))
}

/// What says Claude Code took a switch.
#[derive(Debug, PartialEq)]
enum Sign {
    /// What it wrote of the command's outcome, `<local-command-stdout>` and all.
    Said(String),
    /// Mid-turn, where it writes nothing: the settings it saves the default in were written again.
    Saved,
}

/// What says Claude Code took a switch, waiting up to `wait` for it: the outcome it wrote in the
/// session's transcript (`records`' first) after byte `from`, the first such line's; or, given
/// `saves` (the settings and when they were last written before the switch), the settings
/// written since. Another write of them in those seconds would be taken for the switch's.
fn sign_after(records: &dyn Fn() -> Vec<PathBuf>, from: u64, saves: Option<(&Path, Option<SystemTime>)>, wait: Duration) -> Option<Sign> {
    let deadline = Instant::now() + wait;
    loop {
        if let Some(tail) = records().first().and_then(|t| crate::util::read_tail(t, from).ok()) {
            let said = tail.text().lines().find_map(|line| {
                let v: Value = serde_json::from_str(line).ok()?;
                let content = v.pointer("/message/content")?;
                let text = content.as_str().or_else(|| content.as_array()?.iter().find_map(|i| i.get("text")?.as_str()))?;
                (v.get("type")?.as_str()? == "user" && text.starts_with("<local-command-stdout>")).then(|| text.to_string())
            });
            if let Some(said) = said {
                return Some(Sign::Said(said));
            }
        }
        if saves.is_some_and(|(settings, was)| written_at(settings) != was) {
            return Some(Sign::Saved);
        }
        if Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// When the settings were last written, None while there are none.
fn written_at(settings: &Path) -> Option<SystemTime> {
    std::fs::metadata(settings).and_then(|m| m.modified()).ok()
}

/// Presses Return where a session runs, its tab or a background job's terminal, to answer the
/// question up there.
fn press_return(account: &AccountCfg, session: &Session) -> Result<()> {
    let home = account.home_path();
    let registry = super::session_entry(&home, session).ok_or_else(|| anyhow!("It isn't running any more."))?;
    if let Some(why) = waits_on_another(&registry) {
        bail!("Its terminal shows something else to answer ({why}), which Return would answer: answer its question about the switch there.");
    }
    super::press_in(&home, session, &registry, crate::launch::Key::Return, "Claude Code may be asking about the switch there: answer it in its terminal.")
}

/// What a session waits on that isn't a switch's question, by its registry: Claude Code marks it
/// waiting for a permission prompt, a question, a held message or another dialog of its own, never
/// for the panel a command's question shows in (2.1.322's table of them: `/model`'s has no
/// `waitingFor`). A turn going on under the switch can bring one up meanwhile, and Return would
/// answer it.
fn waits_on_another(registry: &super::RegistryEntry) -> Option<&str> {
    (registry.status == "waiting").then(|| registry.waiting_for.as_deref().unwrap_or("a prompt"))
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
/// `before` and `saved`, and for `/effort` that `saved` has at the level it set: another
/// session's level changed meanwhile isn't its) put back to `before`'s where they're still what
/// it saved, or None when there are none.
fn with_defaults_of(text: &str, before: &Value, saved: &Value, to: Switch) -> Result<Option<String>> {
    let now: Value = serde_json::from_str(text)?;
    let level = |v: &Value, model: Option<&str>| -> Option<Value> {
        match model {
            None => v.get(EFFORT_LEVEL).cloned(),
            Some(m) => v.get(MODEL_SETTINGS)?.get(m)?.get(EFFORT_LEVEL).cloned(),
        }
    };
    let models = |v: &Value| -> Vec<String> { v.get(MODEL_SETTINGS).and_then(Value::as_object).map(|o| o.keys().cloned().collect()).unwrap_or_default() };
    let set = Value::String(to.value().to_string());
    let (saves, levels) = match to {
        Switch::Model(_) => (MODEL, false),
        Switch::Effort(_) => (EFFORT_LEVEL, true),
    };
    let ours = |put: Option<&Value>| !levels || put == Some(&set);
    let mut tops: Vec<&str> = Vec::new();
    if saved.get(saves) != before.get(saves) && now.get(saves) == saved.get(saves) && ours(saved.get(saves)) {
        tops.push(saves);
    }
    let mut changed: Vec<String> = Vec::new();
    for model in models(saved).into_iter().chain(models(before)).filter(|_| levels) {
        let (was, put) = (level(before, Some(&model)), level(saved, Some(&model)));
        if put != was && ours(put.as_ref()) && level(&now, Some(&model)) == put && !changed.contains(&model) {
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
        assert_eq!(put_back(&changed, &before, Switch::Effort("low")).as_deref(), Some(SETTINGS));
        let top = SETTINGS.replace("  \"effortLevel\": \"xhigh\",", "  \"effortLevel\": \"medium\",");
        assert_eq!(put_back(&top, &before, Switch::Effort("medium")).as_deref(), Some(SETTINGS));
        // Levels as they were: nothing to write.
        assert_eq!(put_back(SETTINGS, &before, EFFORT), None);
        // Another level changed meanwhile (`/effort low` in a Sonnet session's own terminal)
        // isn't this switch's.
        let both = saved.replace("      \"effortLevel\": \"high\"", "      \"effortLevel\": \"low\"").replace("    \"claude-opus-5-5\": {\n      \"effortLevel\": \"xhigh\"", "    \"claude-opus-5-5\": {\n      \"effortLevel\": \"high\"");
        let theirs = SETTINGS.replace("      \"effortLevel\": \"xhigh\"\n    }\n  },", "      \"effortLevel\": \"xhigh\"\n    },\n    \"claude-sonnet-5-5\": {\n      \"effortLevel\": \"low\"\n    }\n  },");
        assert_eq!(put_back(&both, &before, EFFORT).as_deref(), Some(theirs.as_str()));
    }

    #[test]
    fn takes_out_what_effort_added_to_settings_without_levels() {
        let before = json!({ "model": "opus" });
        let saved = "{\n  \"model\": \"opus\",\n  \"modelSettings\": {\n    \"claude-opus-5-5\": {\n      \"effortLevel\": \"high\"\n    }\n  }\n}";
        assert_eq!(put_back(saved, &before, EFFORT).as_deref(), Some("{\n  \"model\": \"opus\"\n}"));
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
        let after_second = with_defaults_of(&second.to_string(), &first, &second, Switch::Effort("low")).unwrap().unwrap();
        let after_first = with_defaults_of(&after_second, &before, &first, MODEL_SWITCH).unwrap().unwrap();
        assert_eq!(serde_json::from_str::<Value>(&after_first).unwrap(), before);
    }

    #[test]
    fn finds_what_a_switch_said_after_it_was_typed() {
        let dir = crate::store::test_dir("switch-said");
        let path = dir.join("t.jsonl");
        let records = || vec![path.clone()].into_iter().filter(|p| p.is_file()).collect();
        // A session with no transcript yet says nothing until the command starts one.
        assert_eq!(sign_after(&records, 0, None, Duration::ZERO), None);
        let line = |content: &str| format!("{}\n", json!({ "type": "user", "message": { "role": "user", "content": content } }));
        std::fs::write(&path, line("<local-command-stdout>Set effort level to low (this session only): Quick</local-command-stdout>")).unwrap();
        let from = std::fs::metadata(&path).unwrap().len();
        // What came before the switch isn't what it said, and nothing yet is nothing.
        assert_eq!(sign_after(&records, from, None, Duration::ZERO), None);
        let caveat = line("<local-command-caveat>The command below was run directly in Claude Code.</local-command-caveat>");
        let said = "<local-command-stdout>Set effort level to high (saved as your default for new sessions): Comprehensive</local-command-stdout>";
        std::fs::write(&path, format!("{}{caveat}{}", std::fs::read_to_string(&path).unwrap(), line(said))).unwrap();
        assert_eq!(sign_after(&records, from, None, Duration::ZERO), Some(Sign::Said(said.to_string())));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Mid-turn Claude Code writes nothing of a switch in the transcript, and the settings it
    /// saves the default in being written again, with the same text or not, say it took it.
    #[test]
    fn a_switch_mid_turn_is_seen_by_the_settings_it_saves() {
        let dir = crate::store::test_dir("switch-saved");
        let (path, settings) = (dir.join("t.jsonl"), dir.join("settings.json"));
        std::fs::write(&path, "").unwrap();
        let records = || vec![path.clone()];
        std::fs::write(&settings, SETTINGS).unwrap();
        let was = written_at(&settings);
        assert!(was.is_some());
        // Nothing written since: nothing says it was taken.
        assert_eq!(sign_after(&records, 0, Some((&settings, was)), Duration::ZERO), None);
        // As Claude Code saves a default it already had: the same text, written again.
        let file = std::fs::File::options().write(true).open(&settings).unwrap();
        file.set_modified(was.unwrap() + Duration::from_secs(1)).unwrap();
        assert_eq!(sign_after(&records, 0, Some((&settings, was)), Duration::ZERO), Some(Sign::Saved));
        // Not looked at between turns, where the transcript says what it did.
        assert_eq!(sign_after(&records, 0, None, Duration::ZERO), None);
        // Settings that weren't there before and are now were written too.
        let fresh = dir.join("fresh.json");
        assert_eq!(written_at(&fresh), None);
        std::fs::write(&fresh, "{}").unwrap();
        assert_eq!(sign_after(&records, 0, Some((&fresh, None)), Duration::ZERO), Some(Sign::Saved));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Mid-turn an effort switch presses no Return, as nothing asks about one and a prompt the
    /// turn brought up would take it: one that saved its default is on its level, one that saved
    /// none (max, or a level over the cap) says nothing of what it set. A model's question is
    /// answered with Return, which here finds no session to press it in.
    #[test]
    fn mid_turn_only_a_models_question_gets_a_return() {
        use crate::model::{Harness, Status};
        let dir = crate::store::test_dir("switch-mid-turn");
        let account: AccountCfg = serde_json::from_value(json!({ "name": "claude", "harness": "claude", "home": dir })).unwrap();
        let (path, settings) = (dir.join("t.jsonl"), dir.join("settings.json"));
        std::fs::write(&path, "").unwrap();
        std::fs::write(&settings, SETTINGS).unwrap();
        let records = || vec![path.clone()];
        let working = Session { status: Status::Working, live: true, pid: None, ..Session::new(Harness::Claude, "claude", "abc") };
        // As Claude Code takes a switch that saves a default: the settings written again, the same.
        let mut saves = |_: &str| -> Result<bool> {
            std::fs::File::options().write(true).open(&settings)?.set_modified(SystemTime::now() + Duration::from_secs(5))?;
            Ok(true)
        };
        let mut quiet = |_: &str| -> Result<bool> { Ok(true) };
        let switch = |to: Switch, typ: &mut dyn FnMut(&str) -> Result<bool>| see_through(&account, &working, &records, to, "/typed", typ);
        assert_eq!(switch(Switch::Effort("high"), &mut saves).unwrap(), Some(Switched::On("high".into())));
        assert_eq!(switch(Switch::Effort("max"), &mut quiet).unwrap(), Some(Switched::Unsaid));
        assert_eq!(switch(Switch::Model("haiku"), &mut quiet).unwrap_err().to_string(), "It isn't running any more.");
        assert_eq!(switch(Switch::Model("haiku"), &mut saves).unwrap(), Some(Switched::On("haiku".into())));
        // Nothing typed: nothing switched.
        assert_eq!(switch(Switch::Effort("high"), &mut |_: &str| Ok(false)).unwrap(), None);
        assert_eq!(std::fs::read_to_string(&settings).unwrap(), SETTINGS);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A level the session's environment or settings fix isn't typed mid-turn, `auto` there
    /// included; nor is Return pressed while its terminal shows something else to answer.
    #[test]
    fn a_fixed_level_or_another_prompt_up_keeps_a_switch_out() {
        let none = std::iter::empty::<String>;
        let set = |v: &str| format!(r#"{{"env":{{"{EFFORT_VAR}":"{v}"}},"model":"opus"}}"#);
        let launched = |v: &str| Some(v.to_string());
        assert_eq!(fixed_effort(None, none(), "high"), None);
        assert_eq!(fixed_effort(launched("low"), none(), "high").as_deref(), Some("low"));
        assert_eq!(fixed_effort(launched("auto"), none(), "high").as_deref(), Some("auto"));
        assert_eq!(fixed_effort(launched("HIGH"), none(), "high"), None);
        assert_eq!(fixed_effort(launched(" "), none(), "high"), None);
        // A settings file's `env` goes over what it started with, and a later file over an earlier.
        assert_eq!(fixed_effort(None, [set("low")].into_iter(), "high").as_deref(), Some("low"));
        assert_eq!(fixed_effort(launched("high"), [set("low")].into_iter(), "high").as_deref(), Some("low"));
        assert_eq!(fixed_effort(launched("low"), [set("high")].into_iter(), "high"), None);
        assert_eq!(fixed_effort(None, [set("low"), "{}".into(), "not json".into(), set("auto")].into_iter(), "high").as_deref(), Some("auto"));
        // Set empty, it fixes none, whatever it started with.
        assert_eq!(fixed_effort(launched("low"), [set("")].into_iter(), "high"), None);
        let registry = |status: &str, waiting: Option<&str>| super::super::RegistryEntry { status: status.into(), waiting_for: waiting.map(str::to_string), ..Default::default() };
        assert_eq!(waits_on_another(&registry("busy", None)), None);
        assert_eq!(waits_on_another(&registry("waiting", Some("permission prompt"))), Some("permission prompt"));
        assert_eq!(waits_on_another(&registry("waiting", None)), Some("a prompt"));
    }
}
