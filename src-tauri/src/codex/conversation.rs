//! A rollout as the session panel's conversation, and the calls at its end with no output
//! yet.

use std::path::Path;

use serde_json::Value;

use super::rollout::{file_path_of, is_turn_event, prose_of};
use crate::transcript::{clip_input, clip_output, item, kept_whole, text_of, Image, Item, Page, IMAGES};
use crate::util::{code, one_line, without_cd};

/// One rollout line as conversation items (transcript.rs): prompts and replies, and completed
/// commands, file changes, MCP calls and searches with what they gave back.
pub fn conversation_items(v: &Value, at: i64) -> Vec<Item> {
    if let Some((you, t)) = prose_of(v) {
        let images = if you { prompt_images(v) } else { Vec::new() };
        return vec![Item { images, ..item(at, if you { "you" } else { "agent" }, t) }];
    }
    let sent = sent_images(v);
    if !sent.is_empty() {
        return vec![Item { images: sent, ..item(at, IMAGES, "") }];
    }
    let Some(p) = (v.get("type").and_then(Value::as_str) == Some("event_msg")).then(|| v.get("payload")).flatten() else { return Vec::new() };
    match p.get("type").and_then(Value::as_str) {
        Some("turn_aborted") => vec![item(at, "note", "The turn was stopped")],
        Some("item_completed") => {
            let Some(it) = p.get("item") else { return Vec::new() };
            let at = p.get("completed_at_ms").and_then(Value::as_i64).unwrap_or(at);
            let s = |k: &str| it.get(k).and_then(Value::as_str).unwrap_or("").to_string();
            match it.get("type").and_then(Value::as_str) {
                Some("CommandExecution") => {
                    // Usually `[shell, "-lc", script]`.
                    let script = match it.get("command") {
                        Some(Value::Array(a)) if a.len() >= 3 => a[2].as_str().unwrap_or("").to_string(),
                        Some(Value::Array(a)) => a.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(" "),
                        Some(Value::String(c)) => c.clone(),
                        _ => String::new(),
                    };
                    // A script a shell ran, not words joined from some other program's arguments.
                    let scripted = match it.get("command") {
                        Some(Value::Array(a)) => a.len() >= 3 && matches!(a[1].as_str(), Some("-c" | "-lc")),
                        Some(Value::String(_)) => true,
                        _ => false,
                    };
                    let shell = scripted && !script.trim().is_empty() && kept_whole(&script);
                    let exit = it.get("exit_code").and_then(Value::as_i64);
                    vec![Item {
                        input: Some(clip_input(&script)),
                        output: Some(clip_output(&s("aggregated_output"))),
                        failed: exit.is_some_and(|c| c != 0),
                        shell,
                        // The folder the agent chose for it, often another worktree than the thread's.
                        cwd: shell.then(|| it.get("cwd").and_then(Value::as_str).map(file_path_of)).flatten(),
                        ..item(at, "tool", format!("Shell · {}", code(&one_line(without_cd(&script), 80))))
                    }]
                }
                Some("FileChange") => {
                    let paths: Vec<String> = it.get("changes").and_then(Value::as_object).map(|m| m.keys().cloned().collect()).unwrap_or_default();
                    let label = match paths.as_slice() {
                        [one] => format!("Edit · {}", code(&crate::util::file_name(Path::new(one)))),
                        many => format!("Edit · {} files", many.len()),
                    };
                    vec![Item { input: Some(paths.join("\n")), failed: s("status") == "failed", ..item(at, "tool", label) }]
                }
                Some("McpToolCall") => {
                    let result = it.get("result");
                    vec![Item {
                        input: it.get("arguments").map(|a| clip_input(&serde_json::to_string_pretty(a).unwrap_or_default())),
                        output: result.and_then(|r| r.get("content")).map(|c| clip_output(&text_of(c))),
                        failed: result.and_then(|r| r.get("isError")).and_then(Value::as_bool) == Some(true),
                        ..item(at, "tool", format!("{} · {}", s("server"), s("tool")))
                    }]
                }
                Some("Extension") => {
                    let kind = s("kind");
                    let label = if kind == "web.search" { format!("Web search · {}", one_line(&s("query"), 80)) } else { kind };
                    vec![item(at, "tool", label)]
                }
                // A prompt with no words (`prose_of` has the others): only its images, when it
                // came with any.
                Some("UserMessage") => {
                    let parts = it.get("content").and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default();
                    let pictured = parts.iter().any(|p| matches!(p.get("type").and_then(Value::as_str), Some("image" | "local_image")));
                    if pictured && text_of(it.get("content").unwrap_or(&Value::Null)).trim().is_empty() {
                        vec![Item { images: prompt_images(v), ..item(at, "you", "") }]
                    } else {
                        Vec::new()
                    }
                }
                Some("ImageView") => vec![item(at, "tool", format!("Viewed an image{}", it.get("path").and_then(Value::as_str).map(|p| format!(" · {}", code(&crate::util::file_name(Path::new(p))))).unwrap_or_default()))],
                Some("ContextCompaction") => vec![item(at, "note", "The conversation was compacted here")],
                _ => Vec::new(),
            }
        }
        _ => Vec::new(),
    }
}

// A prompt's images are in the rollout twice. The `UserMessage` item names each as you gave it:
// an `image` part with the image itself (a `data:` URL), or a `local_image` part with a path,
// which for one pasted in is a temporary file that's gone within days (none of 232 here were
// still there). The line before it (a `response_item` `message` with role `user`, at most one
// line between them, none that's part of the conversation) has what the model got: an
// `input_image` part for each, always the image itself. So a prompt takes its images from the
// line before it (transcript.rs, `shown`), and from its own `image` parts without one. The
// same kind of line carries an image a tool hands the model; what follows that isn't a prompt,
// and it's dropped. Checked on 2,900 rollouts here, 2026-10-02.

/// The images in a prompt's own `UserMessage` item.
fn prompt_images(v: &Value) -> Vec<Image> {
    let parts = v.pointer("/payload/item/content").and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default();
    parts.iter().filter(|p| p.get("type").and_then(Value::as_str) == Some("image")).filter_map(|p| Image::from_data_url(p.get("image_url")?.as_str()?)).collect()
}

/// The images on a line that hands the model a user message, which a prompt's are.
fn sent_images(v: &Value) -> Vec<Image> {
    let Some(p) = v.get("payload").filter(|_| v.get("type").and_then(Value::as_str) == Some("response_item")) else { return Vec::new() };
    if p.get("type").and_then(Value::as_str) != Some("message") || p.get("role").and_then(Value::as_str) != Some("user") {
        return Vec::new();
    }
    let parts = p.get("content").and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default();
    parts.iter().filter(|p| p.get("type").and_then(Value::as_str) == Some("input_image")).filter_map(|p| Image::from_data_url(p.get("image_url")?.as_str()?)).collect()
}

/// A page with the images of the prompt it starts with: they're on the line before the prompt,
/// which the page before this one has, or the read before this one took.
pub fn with_images_before(rollout: &Path, mut page: Page) -> Page {
    let Some(first) = page.items.first().filter(|it| it.kind == "you").map(|it| it.cursor) else { return page };
    let before = crate::transcript::file_page(rollout, conversation_items, Some(first), 1).ok().and_then(|p| p.items.into_iter().last());
    if let Some(images) = before.filter(|it| it.kind == IMAGES) {
        page.items.insert(0, images);
    }
    page
}

/// The inputs of the calls at the end of a rollout with no output yet: a call is written there
/// before it runs, and waits there while the app asks you to approve it.
pub(super) fn open_calls(path: &Path) -> Vec<String> {
    let Ok(page) = crate::transcript::file_page(path, call_items, None, 30) else { return Vec::new() };
    // A turn that ended or was stopped leaves no call running.
    let turn = page.items.iter().rposition(|i| i.kind == "note").map_or(0, |i| i + 1);
    crate::transcript::open_tools(&page.items[turn..]).into_iter().filter_map(|i| i.input.clone()).collect()
}

/// A rollout line as the tool calls in it: a call (`tool`, its code or arguments whole as the
/// input), its output (`result`), or a turn starting or ending (`note`).
fn call_items(v: &Value, at: i64) -> Vec<Item> {
    if is_turn_event(v) {
        return vec![item(at, "note", "")];
    }
    let Some(p) = (v.get("type").and_then(Value::as_str) == Some("response_item")).then(|| v.get("payload")).flatten() else { return Vec::new() };
    let id = p.get("call_id").and_then(Value::as_str).map(String::from);
    match p.get("type").and_then(Value::as_str) {
        Some("function_call" | "custom_tool_call") => {
            let input = p.get("input").or_else(|| p.get("arguments")).and_then(Value::as_str).map(String::from);
            vec![Item { id, input, ..item(at, "tool", "") }]
        }
        Some("function_call_output" | "custom_tool_call_output") => vec![Item { id, ..item(at, "result", "") }],
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_calls_still_running_at_the_end_of_a_rollout() {
        let dir = crate::fixtures::scratch("codex-open-calls");
        let path = dir.join("rollout.jsonl");
        let line = |kind: &str, payload: &str| format!(r#"{{"timestamp":"2026-09-27T16:00:00.000Z","type":"{kind}","payload":{payload}}}"#);
        let exec = r#"text(await tools.exec_command({cmd:\"codealong whoami\"}));"#;
        let lines = [
            line("event_msg", r#"{"type":"task_started"}"#),
            line("response_item", &serde_json::json!({ "type": "custom_tool_call", "call_id": "a", "name": "exec", "input": exec }).to_string()),
            line("response_item", r#"{"type":"function_call","call_id":"b","name":"exec_command","arguments":"{\"cmd\":\"ls\"}"}"#),
            line("response_item", r#"{"type":"function_call_output","call_id":"b","output":"x"}"#),
        ];
        std::fs::write(&path, lines.join("\n") + "\n").unwrap();
        assert_eq!(open_calls(&path), vec![exec.to_string()]);
        // Once the turn ends, nothing is running, whatever went unanswered.
        std::fs::write(&path, [lines.join("\n"), line("event_msg", r#"{"type":"turn_aborted"}"#)].join("\n") + "\n").unwrap();
        assert!(open_calls(&path).is_empty());
    }
}
