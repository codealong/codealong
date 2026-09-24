//! Images pasted or dropped into a todo's description or a new session's prompt. Each is kept
//! in `images/` next to the config, named by its content, so the same screenshot twice is one
//! file. Text refers to them as `![…](images/<name>)`; windows show them through the
//! `codealong-image:` scheme (lib.rs), and a start swaps in the full path, since no harness
//! takes an image any other way when a session starts:
//!
//! - Claude Code reads `@"<path>"` in its first prompt as a file mention and attaches an image
//!   of up to 256 KB as an image; a bigger one it reads with its Read tool. `--add-dir` on the
//!   images folder lets it do that without asking (start.rs).
//! - The Claude app's and Codex's new-session links take text only (their `file` parameters
//!   are dropped or go elsewhere), so the agent gets the path and opens the image itself.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};

use crate::config::support_dir;
use crate::util::DAY_MS;

/// Bigger than any screenshot, smaller than anything that should sit in a todo.
pub const MAX_BYTES: usize = 20 * 1024 * 1024;
/// What text calls the folder.
pub const PREFIX: &str = "images/";

pub fn dir() -> PathBuf {
    support_dir().join("images")
}

/// The image format, from the file's first bytes: png, jpg, gif or webp.
pub fn kind(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("png")
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some("jpg")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("gif")
    } else if bytes.len() > 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("webp")
    } else {
        None
    }
}

pub fn mime(name: &str) -> &'static str {
    match name.rsplit('.').next() {
        Some("png") => "image/png",
        Some("jpg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        _ => "application/octet-stream",
    }
}

/// A name `save` could have made: 16 hex digits and a known extension. Nothing else is ever
/// read from or written to the folder, so no request can reach outside it.
pub fn valid_name(name: &str) -> bool {
    let Some((stem, ext)) = name.split_once('.') else { return false };
    stem.len() == 16 && stem.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()) && ["png", "jpg", "gif", "webp"].contains(&ext)
}

fn name_for(bytes: &[u8], ext: &str) -> String {
    let hash = Sha256::digest(bytes);
    let hex: String = hash.iter().take(8).map(|b| format!("{b:02x}")).collect();
    format!("{hex}.{ext}")
}

/// Keeps an image in `into` and returns its name.
pub fn save_in(into: &Path, bytes: &[u8]) -> Result<String> {
    if bytes.len() > MAX_BYTES {
        bail!("That image is over {} MB.", MAX_BYTES / (1024 * 1024));
    }
    let Some(ext) = kind(bytes) else { bail!("Only PNG, JPEG, GIF and WebP images can go in.") };
    let name = name_for(bytes, ext);
    let path = into.join(&name);
    if !path.exists() {
        std::fs::create_dir_all(into)?;
        let tmp = into.join(format!(".{name}.tmp"));
        std::fs::write(&tmp, bytes).with_context(|| format!("writing {}", tmp.display()))?;
        std::fs::rename(&tmp, &path)?;
    }
    Ok(name)
}

pub fn save(bytes: &[u8]) -> Result<String> {
    save_in(&dir(), bytes)
}

/// Keeps a copy of an image file, for one dropped from the Finder.
pub fn import(path: &Path) -> Result<String> {
    let len = std::fs::metadata(path).with_context(|| format!("reading {}", path.display()))?.len();
    if len > MAX_BYTES as u64 {
        bail!("That image is over {} MB.", MAX_BYTES / (1024 * 1024));
    }
    save(&std::fs::read(path)?)
}

/// The file for a name, when it's one of ours.
pub fn file(name: &str) -> Option<PathBuf> {
    valid_name(name).then(|| dir().join(name))
}

/// Each reference in a text: where it starts and ends, and the image's name. A reference is
/// `images/<name>` at the start of a word or inside `(…)`, so a full path that happens to end
/// in `images/<name>` isn't one.
fn references(text: &str) -> Vec<(usize, usize, &str)> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(i) = text[from..].find(PREFIX) {
        let start = from + i;
        let body = start + PREFIX.len();
        let len = text[body..].find(|c: char| !(c.is_ascii_alphanumeric() || c == '.')).unwrap_or(text.len() - body);
        let name = text[body..body + len].trim_end_matches('.');
        let boundary = text[..start].chars().next_back().map(|c| c.is_whitespace() || "(<\"'[".contains(c)).unwrap_or(true);
        if boundary && valid_name(name) {
            out.push((start, body + name.len(), name));
        }
        from = body + len;
    }
    out
}

/// The images a text refers to, in order, without repeats.
pub fn referenced(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for (_, _, name) in references(text) {
        if !out.iter().any(|n| n == name) {
            out.push(name.to_string());
        }
    }
    out
}

/// How a start hands an image to the agent.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Style {
    /// `@"<path>"`, Claude Code's file mention, in place of the whole `![…](…)`.
    Mention,
    /// The full path in place of `images/<name>`.
    Path,
}

/// The `![alt](` before a reference and the `)` after it, when it's written as an image.
fn markdown_span(text: &str, start: usize, end: usize) -> Option<(usize, usize)> {
    if !text[end..].starts_with(')') || !text[..start].ends_with("](") {
        return None;
    }
    let open = text[..start - 2].rfind("![")?;
    let alt = &text[open + 2..start - 2];
    (!alt.contains(['\n', ']'])).then_some((open, end + 1))
}

/// The text with each image as the agent should get it.
pub fn expand_in(text: &str, dir: &Path, style: Style) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    for (start, end, name) in references(text) {
        let full = dir.join(name).to_string_lossy().to_string();
        let (from, to, with) = match (style, markdown_span(text, start, end)) {
            (Style::Mention, Some((from, to))) => (from, to, format!("@\"{full}\"")),
            (Style::Mention, None) => (start, end, format!("@\"{full}\"")),
            (Style::Path, _) => (start, end, full),
        };
        if from < last {
            continue;
        }
        out.push_str(&text[last..from]);
        out.push_str(&with);
        last = to;
    }
    out.push_str(&text[last..]);
    out
}

pub fn expand(text: &str, style: Style) -> String {
    expand_in(text, &dir(), style)
}

/// Removes images no todo refers to any more, once they've sat unused for 30 days: an image cut
/// from a description, or a todo deleted and restored, is still there for a while, and so is one
/// a session was started with for as long as Claude Code keeps the session.
pub fn collect_garbage(texts: &[&str]) {
    let keep: std::collections::HashSet<String> = texts.iter().flat_map(|t| referenced(t)).collect();
    let Ok(entries) = std::fs::read_dir(dir()) else { return };
    let cutoff = std::time::SystemTime::now() - std::time::Duration::from_millis(30 * DAY_MS as u64);
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if !valid_name(&name) || keep.contains(&name) {
            continue;
        }
        let old = e.metadata().and_then(|m| m.modified()).map(|t| t < cutoff).unwrap_or(false);
        if old {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";

    #[test]
    fn knows_images_by_their_bytes() {
        assert_eq!(kind(PNG), Some("png"));
        assert_eq!(kind(&[0xff, 0xd8, 0xff, 0xe0]), Some("jpg"));
        assert_eq!(kind(b"GIF89a...."), Some("gif"));
        assert_eq!(kind(b"RIFF\0\0\0\0WEBPVP8 "), Some("webp"));
        assert_eq!(kind(b"<svg xmlns"), None);
        assert_eq!(mime("0123456789abcdef.jpg"), "image/jpeg");
    }

    #[test]
    fn names_stay_inside_the_folder() {
        assert!(valid_name("0123456789abcdef.png"));
        for bad in ["../config.toml", "0123456789abcdef.svg", "0123456789ABCDEF.png", "0123456789abcde.png", "/etc/passwd", "0123456789abcdef.png/..", ""] {
            assert!(!valid_name(bad), "{bad}");
        }
    }

    #[test]
    fn keeps_one_file_per_image() {
        let dir = std::env::temp_dir().join(format!("codealong-images-{}", std::process::id()));
        let a = save_in(&dir, PNG).unwrap();
        assert!(valid_name(&a) && a.ends_with(".png"));
        assert_eq!(save_in(&dir, PNG).unwrap(), a);
        assert_eq!(std::fs::read(dir.join(&a)).unwrap(), PNG);
        assert!(save_in(&dir, b"plain text").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn finds_and_expands_references() {
        let text = "Broken on save:\n![image](images/0123456789abcdef.png)\nand ![b](images/fedcba9876543210.jpg). Again images/0123456789abcdef.png, not images/nope.png";
        assert_eq!(referenced(text), vec!["0123456789abcdef.png".to_string(), "fedcba9876543210.jpg".to_string()]);
        let dir = Path::new("/Users/me/Library/Application Support/Codealong/images");
        let out = expand_in(text, dir, Style::Path);
        assert!(out.contains("![image](/Users/me/Library/Application Support/Codealong/images/0123456789abcdef.png)"));
        assert!(out.contains("Again /Users/me/Library/Application Support/Codealong/images/0123456789abcdef.png,"));
        assert!(out.contains("not images/nope.png"));
        // A full path already there stays as it is.
        let full = "/Users/me/Library/Application Support/Codealong/images/0123456789abcdef.png";
        assert_eq!(expand_in(full, Path::new("/x"), Style::Path), full);
        assert!(referenced(full).is_empty());
        assert_eq!(expand_in("images/0123456789abcdef.png", Path::new("/x"), Style::Path), "/x/0123456789abcdef.png");
        assert_eq!(expand_in("trailing images/", Path::new("/x"), Style::Path), "trailing images/");
        assert_eq!(expand_in("images/é images/0123456789abcdef.gif", Path::new("/x"), Style::Path), "images/é /x/0123456789abcdef.gif");
    }

    #[test]
    fn claude_code_gets_each_image_as_a_file_mention() {
        let dir = Path::new("/Users/me/Library/Application Support/Codealong/images");
        let text = "Fix the header:\n![image](images/0123456789abcdef.png)\nsame as images/fedcba9876543210.jpg";
        assert_eq!(
            expand_in(text, dir, Style::Mention),
            "Fix the header:\n@\"/Users/me/Library/Application Support/Codealong/images/0123456789abcdef.png\"\nsame as @\"/Users/me/Library/Application Support/Codealong/images/fedcba9876543210.jpg\""
        );
        // Mentions need a space or the start of a line before them, so the markdown goes.
        assert_eq!(expand_in("See ![a (b)](images/0123456789abcdef.png).", Path::new("/x"), Style::Mention), "See @\"/x/0123456789abcdef.png\".");
        // Something that only looks like one keeps its brackets.
        assert_eq!(expand_in("See ![a]b](images/0123456789abcdef.png).", Path::new("/x"), Style::Mention), "See ![a]b](@\"/x/0123456789abcdef.png\").");
        assert_eq!(expand_in("See ![shot](images/0123456789abcdef.png).", Path::new("/x"), Style::Mention), "See @\"/x/0123456789abcdef.png\".");
    }
}
