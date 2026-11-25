//! The command line's agent mode, kept through the runs of `agy` Codealong starts. `agy` 1.5.16
//! writes its settings.json back without `agentMode` as it reads it in any run that isn't its
//! terminal UI or `agy acp`: five `-p /usage` runs in a row did, a prompt's print run, and three
//! `agy models` (2026-11-24), each about 0.16 s in, every other key as it was, and none wrote
//! again after it was put back. The run goes on in that mode (its log says `SetCycleMode called:
//! auto`), but the next `agy` to start finds none, a terminal UI or a session Codealong runs
//! (acp/start_mode.rs) alike, and Codealong starts such runs every few minutes (`read_usage`,
//! `models`, a side conversation, an emblem drawn). So before each it watches the file for a
//! while, and puts back the mode it had.

use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::value::RawValue;

use crate::jsonedit::Node;

const AGENT_MODE: &str = "agentMode";

/// How long after a run starts the file is watched: its write came 0.16 s in on an idle Mac, and
/// later on a busy one. A mode taken out on purpose meanwhile (`/config`'s default) is put back
/// too.
const WATCH: Duration = Duration::from_secs(20);
const EVERY: Duration = Duration::from_millis(50);

/// Codealong's writes to the command line's settings, one at a time (`trust_folder` too). agy
/// takes no lock of its own on the file, so only Codealong's wait for each other.
pub(super) static WRITES: Mutex<()> = Mutex::new(());

/// Watches the command line's settings in `home` for the next `WATCH`, off the caller's thread,
/// and puts back the `agentMode` they have now if the run Codealong is about to start deletes it.
/// Nothing when they have none.
pub(super) fn keep(home: &Path) {
    let Some(settings) = super::cli_settings(home) else { return };
    let Some(mode) = std::fs::read_to_string(&settings).ok().as_deref().and_then(mode_in) else { return };
    std::thread::spawn(move || watch(&settings, &mode, WATCH));
}

/// Puts `mode` back the first time `settings` has none, for as long as `watch_for`. Says whether
/// it did.
fn watch(settings: &Path, mode: &RawValue, watch_for: Duration) -> bool {
    let until = Instant::now() + watch_for;
    loop {
        if put_back(settings, mode) {
            return true;
        }
        if Instant::now() >= until {
            return false;
        }
        std::thread::sleep(EVERY);
    }
}

/// The settings' `agentMode` as written, when they have one.
fn mode_in(text: &str) -> Option<Box<RawValue>> {
    let (_, mode) = Node::entries(text).ok()?.into_iter().rev().find(|(k, _)| k == AGENT_MODE)?;
    match mode {
        Node::Raw(raw) => Some(raw),
        Node::Object(_) => None,
    }
}

/// Puts `mode` back as the settings' `agentMode` when they have none, and says whether it did. A
/// file that doesn't read or parse is left as it is; one that doesn't write is tried again until
/// the watch ends, and the next run's watch tries again.
fn put_back(settings: &Path, mode: &RawValue) -> bool {
    let _writing = WRITES.lock().unwrap_or_else(|e| e.into_inner());
    let Ok(text) = std::fs::read_to_string(settings) else { return false };
    let Ok(Some(next)) = with_mode(&text, mode) else { return false };
    crate::util::write_whole(settings, &next).is_ok()
}

/// The settings' text, `text`, with `mode` as its `agentMode`, first, where agy's own write puts
/// it (its keys in alphabetical order), and the rest as it was; None when it has one.
fn with_mode(text: &str, mode: &RawValue) -> anyhow::Result<Option<String>> {
    let mut top = Node::entries(text)?;
    if top.iter().any(|(k, _)| k == AGENT_MODE) {
        return Ok(None);
    }
    top.insert(0, (AGENT_MODE.to_string(), Node::Raw(mode.to_owned())));
    let mut out = serde_json::to_string_pretty(&Node::Object(top))?;
    if text.ends_with('\n') {
        out.push('\n');
    }
    Ok(Some(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// As agy writes the file, with Go's escapes.
    const WRITTEN: &str = "{\n  \"model\": \"Gemini 4 Argon (High)\",\n  \"permissions\": {\n    \"allow\": [\n      \"command(a \\u003e b \\u0026\\u0026 c)\"\n    ]\n  },\n  \"trustedWorkspaces\": [\n    \"/Users/me\"\n  ]\n}\n";

    #[test]
    fn the_mode_goes_back_first_and_the_rest_as_agy_wrote_it() {
        let with = WRITTEN.replacen("{\n", "{\n  \"agentMode\": \"auto\",\n", 1);
        let mode = mode_in(&with).unwrap();
        assert_eq!(mode.get(), "\"auto\"");
        assert_eq!(with_mode(WRITTEN, &mode).unwrap().as_deref(), Some(with.as_str()));
        // One that's there, whatever it is, is left: it's the settings' own.
        assert_eq!(with_mode(&with.replace("\"auto\"", "\"plan\""), &mode).unwrap(), None);
        assert!(mode_in(WRITTEN).is_none());
        assert!(with_mode("{not json", &mode).is_err());
    }

    /// A run that takes the mode out has it put back, and a file that still has it is left as it is.
    #[test]
    fn a_run_that_deletes_the_mode_has_it_put_back() {
        let home = crate::fixtures::scratch("agy-kept-mode");
        let cli = home.join(super::super::CLI_DIR);
        std::fs::create_dir_all(&cli).unwrap();
        let settings = cli.join("settings.json");
        let with = WRITTEN.replacen("{\n", "{\n  \"agentMode\": \"auto\",\n", 1);
        let mode = mode_in(&with).unwrap();

        std::fs::write(&settings, WRITTEN).unwrap();
        assert!(watch(&settings, &mode, Duration::from_secs(10)));
        assert_eq!(std::fs::read_to_string(&settings).unwrap(), with);

        assert!(!watch(&settings, &mode, Duration::ZERO));
        assert_eq!(std::fs::read_to_string(&settings).unwrap(), with);

        // Settings that don't parse aren't written over.
        std::fs::write(&settings, "{not json").unwrap();
        assert!(!watch(&settings, &mode, Duration::ZERO));
        assert_eq!(std::fs::read_to_string(&settings).unwrap(), "{not json");
        let _ = std::fs::remove_dir_all(&home);
    }
}
