//! Pictures drawn for Codealong (`Adapter::draw`, for session emblems) via
//! Antigravity's CLI (`agy -p`): runs a single print turn invoking the built-in `image-generator`
//! subagent, which writes a `.jpg` (or `.png`) into `<cli_dir>/brain/<conversation_id>/`.
//!
//! Each print run still writes a summary row when it ends, so its `conversation_id` is immediately
//! recorded in `antigravity-sides.json` (`super::side::hide`) to keep it off the board, out of
//! search, and out of the token ledger. Checked on `agy` 1.5.16 (2026-11-24): one drawing took
//! ~56 seconds (`num_turns: 1`).

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{anyhow, bail, Result};
use serde_json::Value;

use crate::config::AccountCfg;
use crate::harness::Drawn;

/// How long a drawing may take before it's given up on.
const TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Debug, Default, PartialEq)]
struct Ran {
    conversation_id: Option<String>,
    response: String,
    failed: Option<String>,
}

/// Parses `agy -p --output-format json`'s stdout: `{conversation_id, status, response, ...}`.
fn parse_output(stdout: &str) -> Ran {
    let trimmed = stdout.trim();
    // Some CLI versions may print diagnostic lines ahead of the JSON line; take the last valid JSON object.
    let parsed = serde_json::from_str::<Value>(trimmed)
        .ok()
        .or_else(|| trimmed.lines().rev().find_map(|l| serde_json::from_str::<Value>(l.trim()).ok()));
    let Some(v) = parsed else {
        return Ran { failed: (!trimmed.is_empty()).then(|| crate::util::one_line(trimmed, 200)), ..Ran::default() };
    };
    let text = |k: &str| v.get(k).and_then(Value::as_str).map(String::from);
    let status = text("status");
    let failed = match status.as_deref() {
        Some("SUCCESS") | Some("success") | None => text("error"),
        Some(other) => text("error").or_else(|| Some(format!("Antigravity returned status {other}."))),
    };
    Ran {
        conversation_id: text("conversation_id"),
        response: text("response").unwrap_or_default(),
        failed,
    }
}

/// The folder where `agy` stores its `brain/<conversation_id>/` artifacts.
fn cli_brain_root(account: &AccountCfg) -> PathBuf {
    let home = account.home_path();
    super::folders(&home)
        .into_iter()
        .find(|f| f.cli)
        .map(|f| f.path)
        .unwrap_or_else(|| crate::util::home().join(".gemini").join("antigravity-cli"))
        .join("brain")
}

/// Finds the newest generated image (`.png` or `.jpg`/`.jpeg`) directly inside `folder`.
fn newest_image(folder: &Path) -> Option<PathBuf> {
    let entries = std::fs::read_dir(folder).ok()?;
    entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.extension()
                    .and_then(|x| x.to_str())
                    .is_some_and(|x| x.eq_ignore_ascii_case("png") || x.eq_ignore_ascii_case("jpg") || x.eq_ignore_ascii_case("jpeg"))
        })
        .max_by_key(|p| p.metadata().and_then(|m| m.modified()).ok())
}

fn draw_log() -> PathBuf {
    crate::config::logs_dir().join("agy-draw.log")
}

pub fn draw(account: &AccountCfg, cwd: &Path, prompt: &str) -> Result<Drawn> {
    if let Some(missing) = super::missing_cli(account, &crate::util::home()) {
        bail!("Antigravity's command line, {missing}, isn't installed.");
    }
    let log = draw_log();
    if let Some(parent) = log.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let wrapped = format!(
        "Use the image-generator subagent to draw the image directly without reading any files or skills, and without writing code.\n\n{prompt}"
    );
    let log_str = log.to_string_lossy().into_owned();
    let args = [
        "-p",
        wrapped.as_str(),
        "--output-format",
        "json",
        "--disable-slash-commands",
        "--log-file",
        log_str.as_str(),
    ];
    let cli = super::cli_command(account);
    super::kept_mode::keep(&account.home_path());
    let out = crate::procs::run_in(cli, &args, cwd, &[], "", TIMEOUT)?;
    let ran = parse_output(&out.out);
    let Some(id) = ran.conversation_id else {
        let why = ran
            .failed
            .unwrap_or_else(|| out.err.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("it said nothing").trim().to_string());
        bail!("Antigravity didn't start drawing: {why}");
    };
    super::side::hide(&id);
    let folder = cli_brain_root(account).join(&id);
    let found = newest_image(&folder);
    let image = found.as_deref().map(std::fs::read);
    let _ = std::fs::remove_dir_all(&folder);
    match image {
        Some(Ok(bytes)) => Ok(Drawn { image: bytes, said: ran.response }),
        Some(Err(e)) => Err(anyhow!("Antigravity drew a picture Codealong couldn't read: {e}")),
        None => bail!(
            "Antigravity drew nothing: {}",
            ran.failed.unwrap_or_else(|| if ran.response.trim().is_empty() { "it said nothing".into() } else { crate::util::one_line(&ran.response, 200) })
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Real `agy -p --output-format json` line from 1.5.16 (2026-11-24).
    #[test]
    fn parses_real_agy_json_output() {
        let line = r#"{"conversation_id":"da0cc64c-d4fc-4948-b2db-e18c1b1f9927","status":"SUCCESS","response":"Generating the sage leaf image...\nSage leaf\n","duration_seconds":55.914466,"num_turns":1}"#;
        assert_eq!(
            parse_output(line),
            Ran {
                conversation_id: Some("da0cc64c-d4fc-4948-b2db-e18c1b1f9927".into()),
                response: "Generating the sage leaf image...\nSage leaf\n".into(),
                failed: None,
            }
        );

        let err = parse_output(r#"{"conversation_id":"abc","status":"ERROR","error":"quota exceeded"}"#);
        assert_eq!(err.failed.as_deref(), Some("quota exceeded"));
    }

    #[test]
    fn picks_newest_image_in_brain_folder() {
        let dir = crate::fixtures::scratch("agy-draw");
        std::fs::write(dir.join("notes.md"), "ignored").unwrap();
        let a = dir.join("first.jpg");
        std::fs::write(&a, b"first").unwrap();
        let found = newest_image(&dir);
        assert_eq!(found, Some(a));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
