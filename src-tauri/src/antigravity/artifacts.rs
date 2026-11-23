//! User-facing artifacts an Antigravity tool call writes (`write_file` / `edit_file` with
//! `ArtifactMetadata: { UserFacing: true, Summary: "…" }`), turned into `SentFiles` so a
//! conversation keeps the call out of folds, shows its summary and links the artifact — along with
//! any local images the artifact embeds (`![alt](/abs/path.png)` or `file:///…`) so they render in
//! place beside it (src/ui/SentFiles.tsx).

use std::path::Path;

use serde_json::Value;

use crate::harness::SentFiles;

/// Full path a call writes to, used to identify which worktree a session is working in (`Call.work`).
/// Reads (`read_file`, etc.) return `None`.
pub(super) fn write_path(name: &str, args: &Value) -> Option<String> {
    super::arg(args, "TargetFile")
        .or_else(|| matches!(name, "write_file" | "edit_file").then(|| super::arg(args, "Path")).flatten())
        .filter(|p| p.starts_with('/'))
}

/// What `call` hands to `SentFiles` when a tool call carries `ArtifactMetadata` with
/// `UserFacing: true`. `ArtifactMetadata` is written either as a JSON object (in `transcript_full`)
/// or as a JSON-encoded string (in `transcript.jsonl`), as `ask_question`'s `questions` are.
pub(super) fn sent_of(args: &Value) -> Option<SentFiles> {
    let raw = args.get("ArtifactMetadata")?;
    let meta = match raw {
        Value::String(s) => serde_json::from_str::<Value>(s).ok()?,
        Value::Object(_) => raw.clone(),
        _ => return None,
    };
    if meta.get("UserFacing").and_then(Value::as_bool) != Some(true) {
        return None;
    }
    let path = ["Path", "TargetFile", "AbsolutePath"]
        .iter()
        .find_map(|k| super::arg(args, k))
        .filter(|p| !p.trim().is_empty())?;
    let caption = meta.get("Summary").and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).map(String::from);

    let mut files = vec![path.clone()];
    for key in ["Content", "Replacement", "ReplacementContent"] {
        if let Some(body) = super::arg(args, key) {
            for img in embedded_images(&body) {
                if !files.contains(&img) {
                    files.push(img);
                }
            }
        }
    }
    Some(SentFiles { files, caption })
}

/// Local image paths embedded in a Markdown document (`![alt](/abs/path.png)` or
/// `![alt](file:///abs/path.png)`), in order of appearance and without duplicates. Only files
/// `sentfiles::mime` serves as images are kept; web URLs and relative paths are skipped.
pub(super) fn embedded_images(markdown: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes = markdown.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b'!' && bytes[i + 1] == b'[' {
            if let Some(close_bracket) = markdown[i + 2..].find(']').map(|k| i + 2 + k) {
                // Alt text doesn't span lines.
                if !markdown[i + 2..close_bracket].contains('\n') && bytes.get(close_bracket + 1) == Some(&b'(') {
                    if let Some(close_paren) = markdown[close_bracket + 2..].find(')').map(|k| close_bracket + 2 + k) {
                        let raw = markdown[close_bracket + 2..close_paren].trim();
                        if !raw.contains('\n') {
                            // Strip an optional title after whitespace only when the URL isn't a raw
                            // path with spaces; an angle-bracketed target `<...>` holds spaces whole.
                            let target = raw.strip_prefix('<').and_then(|s| s.strip_suffix('>')).unwrap_or(raw);
                            if let Some(path) = local_image_path(target) {
                                if !out.contains(&path) {
                                    out.push(path);
                                }
                            }
                        }
                        i = close_paren + 1;
                        continue;
                    }
                }
            }
        }
        i += 1;
    }
    out
}

fn local_image_path(target: &str) -> Option<String> {
    let path = if target.starts_with("file://") {
        let stripped = target.split(['?', '#']).next()?;
        let p = super::uri_path(stripped)?;
        // Windows `file:///C:/...` -> `C:/...`.
        if p.len() >= 3 && p.as_bytes()[0] == b'/' && p.as_bytes()[1].is_ascii_alphabetic() && p.as_bytes()[2] == b':' {
            p[1..].to_string()
        } else {
            p
        }
    } else if target.starts_with('/') && !target.starts_with("//") {
        target.to_string()
    } else if target.len() >= 3 && target.as_bytes()[0].is_ascii_alphabetic() && target.as_bytes()[1] == b':' && matches!(target.as_bytes()[2], b'/' | b'\\') {
        target.to_string()
    } else {
        return None;
    };
    let mime = crate::sentfiles::mime(Path::new(&path))?;
    mime.starts_with("image/").then_some(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn user_facing_write_file_becomes_sent_with_embedded_images() {
        let content = concat!(
            "# Mocks\n\n",
            "````carousel\n",
            "![Light](/Users/me/.gemini/antigravity-cli/brain/id/light.png)\n",
            "<!-- slide -->\n",
            "![Dark](file:///Users/me/.gemini/antigravity-cli/brain/id/dark%20mode.png)\n",
            "````\n\n",
            "![Dup](/Users/me/.gemini/antigravity-cli/brain/id/light.png)\n",
            "![Web](https://example.com/a.png)\n",
            "![Rel](images/0123456789abcdef.png)\n",
            "![NotImage](/Users/me/notes.md)\n",
        );
        // Compact transcript.jsonl encodes nested args as JSON strings.
        let args = json!({
            "Path": "\"/Users/me/.gemini/antigravity-cli/brain/id/mocks.md\"",
            "Content": serde_json::to_string(content).unwrap(),
            "ArtifactMetadata": "{\"UserFacing\":true,\"Summary\":\"Light and dark visual mocks.\",\"RequestFeedback\":false}"
        });
        let sent = sent_of(&args).expect("user-facing artifact");
        assert_eq!(sent.caption.as_deref(), Some("Light and dark visual mocks."));
        assert_eq!(
            sent.files,
            vec![
                "/Users/me/.gemini/antigravity-cli/brain/id/mocks.md",
                "/Users/me/.gemini/antigravity-cli/brain/id/light.png",
                "/Users/me/.gemini/antigravity-cli/brain/id/dark mode.png",
            ]
        );
    }

    #[test]
    fn non_user_facing_or_plain_write_file_is_not_sent() {
        let internal = json!({
            "Path": "/Users/me/.gemini/antigravity-cli/brain/id/scratch.md",
            "Content": "# Scratch",
            "ArtifactMetadata": { "UserFacing": false, "Summary": "Internal notes" }
        });
        assert_eq!(sent_of(&internal), None);
        let plain = json!({ "Path": "/Users/me/app/src/main.rs", "Content": "fn main() {}" });
        assert_eq!(sent_of(&plain), None);
    }

    #[test]
    fn object_metadata_and_edit_file_replacement_are_read() {
        let args = json!({
            "Path": "/Users/me/brain/plan.md",
            "Replacement": "Updated preview:\n![After](/Users/me/brain/after.webp)",
            "ArtifactMetadata": { "UserFacing": true, "Summary": "   " }
        });
        let sent = sent_of(&args).unwrap();
        assert_eq!(sent.caption, None);
        assert_eq!(sent.files, vec!["/Users/me/brain/plan.md", "/Users/me/brain/after.webp"]);
    }
}
