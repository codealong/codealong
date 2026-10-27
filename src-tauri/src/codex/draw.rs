//! Pictures drawn for Codealong (`Adapter::draw`, for session emblems): `codex exec` with
//! `--ephemeral`, read-only, on the account's home. Codex's image tool (gpt-image-2, on the
//! plan's usage, no API key) writes what it draws to `<CODEX_HOME>/generated_images/<thread>/`
//! whatever the sandbox, and `--json` says the thread (`thread.started`) and what the agent
//! answered (`agent_message` items), but nothing of the picture (0.162.1, 2026-10-27). The run
//! keeps no rollout, so nothing else knows of its folder, which is removed once read. One picture
//! took 35 to 65 seconds, and left alone the agent read the image skill's instructions first (71k
//! tokens in, most of them cached), which the prompt asks it not to.

use std::path::Path;
use std::time::Duration;

use anyhow::{anyhow, bail, Result};
use serde_json::Value;

use crate::harness::Drawn;

/// How long a drawing may take before it's given up on.
const TIMEOUT: Duration = Duration::from_secs(300);

/// What a run said on its way, from `--json`'s lines.
#[derive(Debug, Default, PartialEq)]
struct Ran {
    thread: Option<String>,
    /// The agent's last message.
    said: String,
    /// Why it failed, when it says so.
    failed: Option<String>,
}

fn read_events(jsonl: &str) -> Ran {
    let mut ran = Ran::default();
    for v in jsonl.lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()) {
        let text = |at: &str| v.pointer(at).and_then(Value::as_str).map(String::from);
        match v.get("type").and_then(Value::as_str) {
            Some("thread.started") => ran.thread = text("/thread_id"),
            Some("item.completed") if v.pointer("/item/type").and_then(Value::as_str) == Some("agent_message") => ran.said = text("/item/text").unwrap_or_default(),
            Some("turn.failed") => ran.failed = text("/error/message").or(Some("The turn failed.".into())),
            Some("error") => ran.failed = text("/message").or(ran.failed.take()),
            _ => {}
        }
    }
    ran
}

pub fn draw(home: &Path, cwd: &Path, prompt: &str) -> Result<Drawn> {
    let codex = super::appserver::codex_binary();
    let args = ["exec", "--ephemeral", "--json", "--skip-git-repo-check", "--sandbox", "read-only", prompt];
    let env = [("CODEX_HOME".to_string(), home.to_string_lossy().into_owned())];
    let out = crate::procs::run_in(&codex.to_string_lossy(), &args, cwd, &env, "", TIMEOUT)?;
    let ran = read_events(&out.out);
    let Some(thread) = ran.thread else {
        let why = ran.failed.unwrap_or_else(|| out.err.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("it said nothing").trim().to_string());
        bail!("Codex didn't start drawing: {why}");
    };
    let folder = home.join("generated_images").join(&thread);
    let picture = std::fs::read_dir(&folder)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x.eq_ignore_ascii_case("png")))
        .max_by_key(|e| e.metadata().and_then(|m| m.modified()).ok())
        .map(|e| std::fs::read(e.path()));
    // A run that keeps no record leaves nothing else that knows of these.
    let _ = std::fs::remove_dir_all(&folder);
    match picture {
        Some(Ok(image)) => Ok(Drawn { image, said: ran.said }),
        Some(Err(e)) => Err(anyhow!("Codex drew a picture Codealong couldn't read: {e}")),
        None => bail!("Codex drew nothing: {}", ran.failed.unwrap_or_else(|| if ran.said.trim().is_empty() { "it said nothing".into() } else { crate::util::one_line(&ran.said, 200) })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The lines `codex exec --json` printed for a picture (0.162.1, 2026-10-27).
    #[test]
    fn reads_the_thread_and_the_answer() {
        let lines = r#"{"type":"thread.started","thread_id":"01a17e89-ce25-7e22-ae43-3944b2f1d719"}
{"type":"turn.started"}
{"type":"item.completed","item":{"id":"item_0","type":"agent_message","text":"I’ll use the image generation skill to draw the cactus.\n"}}
{"type":"item.started","item":{"id":"item_1","type":"command_execution","command":"/bin/zsh -lc 'cat SKILL.md'","aggregated_output":"","exit_code":null,"status":"in_progress"}}
{"type":"item.completed","item":{"id":"item_2","type":"agent_message","text":"Cactus"}}
{"type":"turn.completed","usage":{"input_tokens":71292,"cached_input_tokens":57600,"output_tokens":176}}"#;
        assert_eq!(read_events(lines), Ran { thread: Some("01a17e89-ce25-7e22-ae43-3944b2f1d719".into()), said: "Cactus".into(), failed: None });
        let failed = read_events("{\"type\":\"thread.started\",\"thread_id\":\"t\"}\n{\"type\":\"turn.failed\",\"error\":{\"message\":\"You've hit your usage limit.\"}}\nnot json");
        assert_eq!(failed.failed.as_deref(), Some("You've hit your usage limit."));
    }
}
