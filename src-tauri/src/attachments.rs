//! Attachments: files pasted, dropped or picked into a todo's description or a new session's
//! prompt. Each is kept next to the config and named by its content, so the same file twice is
//! kept once.
//!
//! - Images a window can show (PNG, JPEG, GIF, WebP) are `images/<hash>.<ext>`, written
//!   `![image](images/<hash>.<ext>)`, and shown in place through the `codealong-image:` scheme
//!   (lib.rs).
//! - Any other file is `files/<hash>/<name>`, its name made safe for a path, and written
//!   `[<name>](files/<hash>/<name>)`.
//!
//! A start swaps in each one's full path, since no harness takes a file any other way when a
//! session starts:
//!
//! - Claude Code reads `@"<path>"` in its first prompt as a file mention and attaches a file of
//!   up to 256 KB (an image as an image; a PDF whatever its size); a bigger one it reads with its
//!   Read tool. `--add-dir` on the folders lets it do that without asking (start.rs).
//! - The Claude app's and Codex's new-session links take text only (their `file` parameters are
//!   dropped or go elsewhere), and OpenCode has no `--add-dir`, so the agent gets the path and
//!   opens the file itself.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::config::support_dir;
use crate::util::DAY_MS;

/// Bigger than any screenshot.
pub const MAX_IMAGE_BYTES: usize = 20 * 1024 * 1024;
/// Logs, PDFs, exports; not a disk image.
pub const MAX_FILE_BYTES: usize = 50 * 1024 * 1024;
/// What text calls the folders.
pub const IMAGES: &str = "images/";
pub const FILES: &str = "files/";

pub fn images_dir() -> PathBuf {
    support_dir().join("images")
}

pub fn files_dir() -> PathBuf {
    support_dir().join("files")
}

#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// Shown in place.
    Image,
    File,
}

/// A kept attachment, as a window writes it into text.
#[derive(Serialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Attachment {
    pub kind: Kind,
    /// How text refers to it: `images/<hash>.<ext>` or `files/<hash>/<name>`.
    pub path: String,
    /// The file's own name, to show.
    pub name: String,
}

/// The image format a window can show, from the file's first bytes: png, jpg, gif or webp.
pub fn image_kind(bytes: &[u8]) -> Option<&'static str> {
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

fn is_hash(s: &str) -> bool {
    s.len() == 16 && s.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
}

/// An image name Codealong could have made: 16 hex digits and a known extension. Nothing else
/// is ever read from or written to the folder, so no request can reach outside it.
pub fn valid_image(name: &str) -> bool {
    let Some((stem, ext)) = name.split_once('.') else { return false };
    is_hash(stem) && ["png", "jpg", "gif", "webp"].contains(&ext)
}

fn safe_char(c: char) -> bool {
    c.is_alphanumeric() || "._-".contains(c)
}

/// A file's name as it's kept: letters, digits, `.`, `_` and `-`, each run of anything else one
/// `-`, never starting with a dot, and short enough for any disk. `Crash report (1).log` is
/// kept as `Crash-report-1-.log`.
pub fn safe_name(name: &str) -> String {
    let mut out = String::new();
    for c in name.trim().chars() {
        if safe_char(c) {
            out.push(c);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let out = out.trim_start_matches(['.', '-']);
    let out: String = out.chars().take(100).collect();
    if out.trim_matches(['-', '.']).is_empty() {
        "file".into()
    } else {
        out
    }
}

/// `<hash>/<name>` as `safe_name` makes them, so nothing outside the folder can be named.
pub fn valid_file(rest: &str) -> bool {
    let Some((hash, name)) = rest.split_once('/') else { return false };
    is_hash(hash) && !name.is_empty() && !name.starts_with('.') && name.chars().count() <= 100 && name.chars().all(safe_char)
}

/// The name to show and to write between `[` and `]`: one line, no brackets.
fn shown_name(name: &str, kept: &str) -> String {
    let shown: String = name.chars().filter(|c| !matches!(c, '[' | ']' | '\n' | '\r')).take(120).collect();
    let shown = shown.trim();
    if shown.is_empty() {
        kept.to_string()
    } else {
        shown.to_string()
    }
}

fn hash16(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().take(8).map(|b| format!("{b:02x}")).collect()
}

fn write_once(path: &Path, bytes: &[u8]) -> Result<()> {
    if path.exists() {
        return Ok(());
    }
    let dir = path.parent().context("no folder")?;
    std::fs::create_dir_all(dir)?;
    let tmp = dir.join(format!(".{}.tmp", path.file_name().and_then(|n| n.to_str()).unwrap_or("file")));
    std::fs::write(&tmp, bytes).with_context(|| format!("writing {}", tmp.display()))?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// Keeps a file's bytes under `images` or `files`, by what they are, and says how to refer to it.
/// An image too big to show in place goes with the other files.
pub fn keep_in(images: &Path, files: &Path, bytes: &[u8], name: &str) -> Result<Attachment> {
    if let Some(ext) = image_kind(bytes).filter(|_| bytes.len() <= MAX_IMAGE_BYTES) {
        let kept = format!("{}.{ext}", hash16(bytes));
        write_once(&images.join(&kept), bytes)?;
        return Ok(Attachment { kind: Kind::Image, path: format!("{IMAGES}{kept}"), name: shown_name(name, "image") });
    }
    if bytes.len() > MAX_FILE_BYTES {
        bail!("That file is over {} MB. Put its path in the text instead.", MAX_FILE_BYTES / (1024 * 1024));
    }
    let kept = safe_name(name);
    let rel = format!("{}/{kept}", hash16(bytes));
    write_once(&files.join(&rel), bytes)?;
    Ok(Attachment { kind: Kind::File, path: format!("{FILES}{rel}"), name: shown_name(name, &kept) })
}

/// Keeps a pasted or picked file.
pub fn keep(bytes: &[u8], name: &str) -> Result<Attachment> {
    keep_in(&images_dir(), &files_dir(), bytes, name)
}

/// Keeps a copy of a file dropped from the Finder.
pub fn import(path: &Path) -> Result<Attachment> {
    let meta = std::fs::metadata(path).with_context(|| format!("reading {}", path.display()))?;
    if meta.is_dir() {
        bail!("{} is a folder. Attach the files in it, or put its path in the text.", path.display());
    }
    if meta.len() > MAX_FILE_BYTES as u64 {
        bail!("That file is over {} MB. Put its path in the text instead.", MAX_FILE_BYTES / (1024 * 1024));
    }
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    keep(&std::fs::read(path)?, &name)
}

/// The file behind an attachment's path, when it's one of ours.
pub fn locate_in(images: &Path, files: &Path, path: &str) -> Option<PathBuf> {
    if let Some(name) = path.strip_prefix(IMAGES) {
        return valid_image(name).then(|| images.join(name));
    }
    let rest = path.strip_prefix(FILES)?;
    valid_file(rest).then(|| files.join(rest))
}

pub fn locate(path: &str) -> Option<PathBuf> {
    locate_in(&images_dir(), &files_dir(), path)
}

/// Each reference in a text: where it starts and ends, its kind, and the part after the
/// folder's name. A reference starts a word or sits inside `(…)`, so a full path that happens
/// to end in `images/<name>` isn't one.
fn references(text: &str) -> Vec<(usize, usize, Kind, &str)> {
    let mut out = Vec::new();
    for (prefix, kind) in [(IMAGES, Kind::Image), (FILES, Kind::File)] {
        let mut from = 0;
        while let Some(i) = text[from..].find(prefix) {
            let start = from + i;
            let body = start + prefix.len();
            let len = text[body..].find(|c: char| !(safe_char(c) || c == '/')).unwrap_or(text.len() - body);
            // A reference at the end of a sentence doesn't take its full stop.
            let rest = text[body..body + len].trim_end_matches('.');
            let boundary = text[..start].chars().next_back().map(|c| c.is_whitespace() || "(<\"'[".contains(c)).unwrap_or(true);
            let valid = match kind {
                Kind::Image => valid_image(rest),
                Kind::File => valid_file(rest),
            };
            if boundary && valid {
                out.push((start, body + rest.len(), kind, rest));
            }
            from = body + len;
        }
    }
    out.sort_by_key(|r| r.0);
    out
}

/// The attachments a text refers to (`images/…` and `files/…`), in order, without repeats.
pub fn referenced(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for (start, end, _, _) in references(text) {
        let path = &text[start..end];
        if !out.iter().any(|p| p == path) {
            out.push(path.to_string());
        }
    }
    out
}

/// Which kinds a text has, for the folders a start opens to the agent.
pub fn kinds(text: &str) -> (bool, bool) {
    let refs = references(text);
    (refs.iter().any(|r| r.2 == Kind::Image), refs.iter().any(|r| r.2 == Kind::File))
}

/// How a start hands a file to the agent.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Style {
    /// `@"<path>"`, Claude Code's file mention, in place of the whole `![…](…)` or `[…](…)`.
    Mention,
    /// The full path in place of the folder and name.
    Path,
}

/// The `[label](` or `![label](` before a reference and the `)` after it, when it's written as a
/// link or an image.
fn markdown_span(text: &str, start: usize, end: usize) -> Option<(usize, usize)> {
    if !text[end..].starts_with(')') || !text[..start].ends_with("](") {
        return None;
    }
    let mut open = text[..start - 2].rfind('[')?;
    let label = &text[open + 1..start - 2];
    if label.contains(['\n', ']']) {
        return None;
    }
    if text[..open].ends_with('!') {
        open -= 1;
    }
    Some((open, end + 1))
}

/// The text with each attachment as the agent should get it.
pub fn expand_in(text: &str, images: &Path, files: &Path, style: Style) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    for (start, end, kind, rest) in references(text) {
        let full = match kind {
            Kind::Image => images.join(rest),
            Kind::File => files.join(rest),
        };
        let full = full.to_string_lossy().to_string();
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
    expand_in(text, &images_dir(), &files_dir(), style)
}

/// Removes attachments no todo refers to any more, once they've sat unused for 30 days: one cut
/// from a description, or a todo deleted and restored, is still there for a while, and so is one
/// a session was started with for as long as Claude Code keeps the session.
pub fn collect_garbage(texts: &[&str]) {
    collect_garbage_in(&images_dir(), &files_dir(), texts, std::time::SystemTime::now() - std::time::Duration::from_millis(30 * DAY_MS as u64));
}

fn collect_garbage_in(images: &Path, files: &Path, texts: &[&str], cutoff: std::time::SystemTime) {
    let keep: std::collections::HashSet<String> = texts.iter().flat_map(|t| referenced(t)).collect();
    let old = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).map(|t| t < cutoff).unwrap_or(false);
    for e in std::fs::read_dir(images).into_iter().flatten().flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if valid_image(&name) && !keep.contains(&format!("{IMAGES}{name}")) && old(&e.path()) {
            let _ = std::fs::remove_file(e.path());
        }
    }
    // A file's folder is its content; it goes when no name in it is referred to.
    for e in std::fs::read_dir(files).into_iter().flatten().flatten() {
        let hash = e.file_name().to_string_lossy().to_string();
        if !is_hash(&hash) || keep.iter().any(|k| k.starts_with(&format!("{FILES}{hash}/"))) || !old(&e.path()) {
            continue;
        }
        let _ = std::fs::remove_dir_all(e.path());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";

    fn temp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("codealong-attach-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn knows_images_by_their_bytes() {
        assert_eq!(image_kind(PNG), Some("png"));
        assert_eq!(image_kind(&[0xff, 0xd8, 0xff, 0xe0]), Some("jpg"));
        assert_eq!(image_kind(b"GIF89a...."), Some("gif"));
        assert_eq!(image_kind(b"RIFF\0\0\0\0WEBPVP8 "), Some("webp"));
        assert_eq!(image_kind(b"<svg xmlns"), None);
        assert_eq!(mime("0123456789abcdef.jpg"), "image/jpeg");
    }

    #[test]
    fn names_stay_inside_the_folders() {
        assert!(valid_image("0123456789abcdef.png"));
        for bad in ["../config.toml", "0123456789abcdef.svg", "0123456789ABCDEF.png", "0123456789abcde.png", "/etc/passwd", "0123456789abcdef.png/..", ""] {
            assert!(!valid_image(bad), "{bad}");
        }
        assert!(valid_file("0123456789abcdef/crash.log"));
        assert!(valid_file("0123456789abcdef/レポート.pdf"));
        for bad in ["0123456789abcdef/../x", "0123456789abcdef/.hidden", "0123456789abcdef/", "0123456789abcdef/a/b", "0123456789abcdeg/x", "x/y"] {
            assert!(!valid_file(bad), "{bad}");
        }
        let (images, files) = (Path::new("/i"), Path::new("/f"));
        assert_eq!(locate_in(images, files, "files/0123456789abcdef/a.log"), Some(PathBuf::from("/f/0123456789abcdef/a.log")));
        assert_eq!(locate_in(images, files, "images/0123456789abcdef.png"), Some(PathBuf::from("/i/0123456789abcdef.png")));
        assert_eq!(locate_in(images, files, "files/0123456789abcdef/../../config.toml"), None);
        assert_eq!(locate_in(images, files, "/etc/passwd"), None);
    }

    #[test]
    fn makes_names_safe_for_a_path() {
        assert_eq!(safe_name("Crash report (1).log"), "Crash-report-1-.log");
        assert_eq!(safe_name("  schema.sql "), "schema.sql");
        assert_eq!(safe_name(".env"), "env");
        assert_eq!(safe_name("../../etc/passwd"), "etc-passwd");
        assert_eq!(safe_name("レポート 2.pdf"), "レポート-2.pdf");
        assert_eq!(safe_name("???"), "file");
        assert_eq!(safe_name(""), "file");
        assert!(valid_file(&format!("0123456789abcdef/{}", safe_name(&"x".repeat(300)))));
    }

    #[test]
    fn keeps_images_to_show_and_other_files_by_name() {
        let (images, files) = (temp("i"), temp("f"));
        let shot = keep_in(&images, &files, PNG, "image.png").unwrap();
        assert_eq!(shot.kind, Kind::Image);
        assert!(shot.path.starts_with("images/") && shot.path.ends_with(".png"));
        assert_eq!(keep_in(&images, &files, PNG, "again.png").unwrap().path, shot.path);
        let log = keep_in(&images, &files, b"line 1\nline 2\n", "Crash report [1].log").unwrap();
        assert_eq!(log.kind, Kind::File);
        assert!(log.path.starts_with("files/") && log.path.ends_with("/Crash-report-1-.log"), "{}", log.path);
        assert_eq!(log.name, "Crash report 1.log");
        assert_eq!(std::fs::read(locate_in(&images, &files, &log.path).unwrap()).unwrap(), b"line 1\nline 2\n");
        // An image a window can't show is a file like any other.
        let svg = keep_in(&images, &files, b"<svg xmlns='http://www.w3.org/2000/svg'/>", "logo.svg").unwrap();
        assert_eq!((svg.kind, svg.name.as_str()), (Kind::File, "logo.svg"));
        let _ = (std::fs::remove_dir_all(&images), std::fs::remove_dir_all(&files));
    }

    #[test]
    fn imports_files_but_not_folders() {
        let dir = temp("import");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("notes.md");
        std::fs::write(&file, "# Notes").unwrap();
        let (images, files) = (dir.join("images"), dir.join("files"));
        // import() writes to the real folders, so its parts are checked here on temp ones.
        let bytes = std::fs::read(&file).unwrap();
        let a = keep_in(&images, &files, &bytes, "notes.md").unwrap();
        assert!(a.path.ends_with("/notes.md"));
        assert!(import(&dir).unwrap_err().to_string().contains("is a folder"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn finds_and_expands_references() {
        let text = "Broken on save:\n![image](images/0123456789abcdef.png)\nand ![b](images/fedcba9876543210.jpg). Again images/0123456789abcdef.png, not images/nope.png";
        assert_eq!(referenced(text), vec!["images/0123456789abcdef.png".to_string(), "images/fedcba9876543210.jpg".to_string()]);
        let (images, files) = (Path::new("/Users/me/Library/Application Support/Codealong/images"), Path::new("/f"));
        let out = expand_in(text, images, files, Style::Path);
        assert!(out.contains("![image](/Users/me/Library/Application Support/Codealong/images/0123456789abcdef.png)"));
        assert!(out.contains("Again /Users/me/Library/Application Support/Codealong/images/0123456789abcdef.png,"));
        assert!(out.contains("not images/nope.png"));
        // A full path already there stays as it is.
        let full = "/Users/me/Library/Application Support/Codealong/images/0123456789abcdef.png";
        assert_eq!(expand_in(full, Path::new("/x"), files, Style::Path), full);
        assert!(referenced(full).is_empty());
        assert_eq!(expand_in("images/0123456789abcdef.png", Path::new("/x"), files, Style::Path), "/x/0123456789abcdef.png");
        assert_eq!(expand_in("trailing images/", Path::new("/x"), files, Style::Path), "trailing images/");
        assert_eq!(expand_in("images/é images/0123456789abcdef.gif", Path::new("/x"), files, Style::Path), "images/é /x/0123456789abcdef.gif");
    }

    #[test]
    fn files_go_to_the_agent_by_path() {
        let (images, files) = (Path::new("/i"), Path::new("/Users/me/Library/Application Support/Codealong/files"));
        let text = "The export fails:\n[Crash report 1.log](files/0123456789abcdef/Crash-report-1-.log)\nand ![image](images/fedcba9876543210.png). See files/aaaaaaaaaaaaaaaa/schema.sql.";
        assert_eq!(referenced(text), vec!["files/0123456789abcdef/Crash-report-1-.log", "images/fedcba9876543210.png", "files/aaaaaaaaaaaaaaaa/schema.sql"]);
        assert_eq!(kinds(text), (true, true));
        assert_eq!(kinds("no attachments"), (false, false));
        assert_eq!(
            expand_in(text, images, files, Style::Mention),
            "The export fails:\n@\"/Users/me/Library/Application Support/Codealong/files/0123456789abcdef/Crash-report-1-.log\"\nand @\"/i/fedcba9876543210.png\". See @\"/Users/me/Library/Application Support/Codealong/files/aaaaaaaaaaaaaaaa/schema.sql\"."
        );
        assert_eq!(
            expand_in("[a.log](files/0123456789abcdef/a.log)", images, Path::new("/f"), Style::Path),
            "[a.log](/f/0123456789abcdef/a.log)"
        );
    }

    #[test]
    fn claude_code_gets_each_image_as_a_file_mention() {
        let (images, files) = (Path::new("/Users/me/Library/Application Support/Codealong/images"), Path::new("/f"));
        let text = "Fix the header:\n![image](images/0123456789abcdef.png)\nsame as images/fedcba9876543210.jpg";
        assert_eq!(
            expand_in(text, images, files, Style::Mention),
            "Fix the header:\n@\"/Users/me/Library/Application Support/Codealong/images/0123456789abcdef.png\"\nsame as @\"/Users/me/Library/Application Support/Codealong/images/fedcba9876543210.jpg\""
        );
        // Mentions need a space or the start of a line before them, so the markdown goes.
        assert_eq!(expand_in("See ![a (b)](images/0123456789abcdef.png).", Path::new("/x"), files, Style::Mention), "See @\"/x/0123456789abcdef.png\".");
        // Something that only looks like one keeps its brackets.
        assert_eq!(expand_in("See ![a]b](images/0123456789abcdef.png).", Path::new("/x"), files, Style::Mention), "See ![a]b](@\"/x/0123456789abcdef.png\").");
        assert_eq!(expand_in("See ![shot](images/0123456789abcdef.png).", Path::new("/x"), files, Style::Mention), "See @\"/x/0123456789abcdef.png\".");
    }

    #[test]
    fn clears_only_old_attachments_no_todo_keeps() {
        let (images, files) = (temp("gc-i"), temp("gc-f"));
        let kept = keep_in(&images, &files, b"kept", "kept.txt").unwrap();
        let gone = keep_in(&images, &files, b"gone", "gone.txt").unwrap();
        let shot = keep_in(&images, &files, PNG, "s.png").unwrap();
        let text = format!("[kept.txt]({})", kept.path);
        // Nothing is old enough yet.
        collect_garbage_in(&images, &files, &[&text], std::time::SystemTime::now() - std::time::Duration::from_secs(3600));
        assert!(locate_in(&images, &files, &gone.path).unwrap().exists());
        collect_garbage_in(&images, &files, &[&text], std::time::SystemTime::now() + std::time::Duration::from_secs(3600));
        assert!(locate_in(&images, &files, &kept.path).unwrap().exists());
        assert!(!locate_in(&images, &files, &gone.path).unwrap().exists());
        assert!(!locate_in(&images, &files, &shot.path).unwrap().exists());
        let _ = (std::fs::remove_dir_all(&images), std::fs::remove_dir_all(&files));
    }
}
