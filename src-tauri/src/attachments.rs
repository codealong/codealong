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
//! A start or send hands each attachment to the harness the way it takes one:
//!
//! - Where the protocol or command takes images itself (sessions Codealong runs over ACP,
//!   OpenCode's service, `codex -i`, Codex's app and app-server), the images are taken out of the
//!   text (`take_images_in`, `read_images_in`) and sent as image parts or options; other files stay
//!   in the text as paths.
//! - Claude Code reads `@"<path>"` in a prompt as a file mention and attaches a file of up to
//!   256 KB (an image as an image; a PDF whatever its size); a bigger one it reads with its Read
//!   tool. `--add-dir` on the folders lets it do that without asking (start.rs).
//! - The Claude app's and Codex's new-session links take text only (their `file` parameters are
//!   dropped or go elsewhere), so the agent gets the path and opens the file itself.
//!
//! A prompt for a session on another device, or a start there, takes its images to that device
//! first (`going_with_in`, actions.rs `carry_images`), which keeps each under the name it has here,
//! being named by its bytes; any other file stays here, so a prompt with one isn't sent. A
//! window shows another device's images by asking it for them (lib.rs `serve_image`).
//!
//! A session's record has a prompt as it went out, so its conversation reads each path as the
//! reference it was (`written_in`). The images a prompt came with as image parts (sent that way
//! from here, pasted into a terminal, or attached in a harness's own app) are in the record
//! itself; reading the conversation keeps those here too (`keep_image_in`, from transcript.rs),
//! so they show in place the same way.

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

impl Attachment {
    /// How text refers to it: an image as `![name](images/…)`, which shows in place, any other
    /// file as `[name](files/…)`. What's sent or started with the text gets each one's full path
    /// (`expand`).
    pub fn reference(&self) -> String {
        match self.kind {
            Kind::Image => format!("![{}]({})", self.name, self.path),
            Kind::File => format!("[{}]({})", self.name, self.path),
        }
    }
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

/// Keeps `bytes` at `path` unless a file is there already: named by its content, one that's
/// there holds the same bytes. Windows reading one conversation keep its new images at the same
/// moment, so each write goes through a temporary file of its own (`store::write`): a shared one
/// let a window rename another's half-written file, or find it gone and drop the image. The last
/// rename puts the same bytes in place, and a write that fails once the file is there anyway
/// has nothing left to do.
fn write_once(path: &Path, bytes: &[u8]) -> Result<()> {
    if path.exists() {
        return Ok(());
    }
    crate::store::write(path, bytes).or_else(|e| if path.exists() { Ok(()) } else { Err(e) })
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

/// Keeps an image a window can show, and says how text refers to it (`images/<hash>.<ext>`).
/// Anything else isn't kept: this is for the images a session's record holds (transcript.rs),
/// where what can't be shown in place is no file of yours to keep.
pub fn keep_image_in(images: &Path, bytes: &[u8]) -> Option<String> {
    let ext = image_kind(bytes).filter(|_| bytes.len() <= MAX_IMAGE_BYTES)?;
    let kept = format!("{}.{ext}", hash16(bytes));
    write_once(&images.join(&kept), bytes).ok()?;
    Some(format!("{IMAGES}{kept}"))
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

/// Whether the text opens with an attachment written as an image or a link (`![…](images/…)`,
/// `[…](files/…)`), whose `!` is the image's, not one typed to lead a shell command.
pub fn leads_with_link(text: &str) -> bool {
    references(text).iter().any(|r| markdown_span(text, r.0, r.1).is_some_and(|(from, _)| from == 0))
}

/// The text with each reference replaced as `with` says: `with(start, end, kind, rest)` gives
/// the span to replace (the reference, or its whole `![…](…)`) and what goes there, or none to
/// leave it.
fn replace(text: &str, mut with: impl FnMut(usize, usize, Kind, &str) -> Option<(usize, usize, String)>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    for (start, end, kind, rest) in references(text) {
        let Some((from, to, with)) = with(start, end, kind, rest) else { continue };
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

/// The text with each attachment as the agent should get it.
pub fn expand_in(text: &str, images: &Path, files: &Path, style: Style) -> String {
    replace(text, |start, end, kind, rest| {
        let full = match kind {
            Kind::Image => images.join(rest),
            Kind::File => files.join(rest),
        };
        let full = full.to_string_lossy().to_string();
        Some(match (style, markdown_span(text, start, end)) {
            (Style::Mention, Some((from, to))) => (from, to, format!("@\"{full}\"")),
            (Style::Mention, None) => (start, end, format!("@\"{full}\"")),
            (Style::Path, _) => (start, end, full),
        })
    })
}

/// The text with each image in it, its whole `![…](…)` when it's written so, replaced by what
/// `with` gives for the image's name (`<hash>.<ext>`). Files stay as they are.
pub fn swap_images(text: &str, mut with: impl FnMut(&str) -> String) -> String {
    replace(text, |start, end, kind, rest| {
        let (from, to) = markdown_span(text, start, end).unwrap_or((start, end));
        (kind == Kind::Image).then(|| (from, to, with(rest)))
    })
}

/// The text without its attachments, each whole `![…](…)` or `[…](…)` taken out: the words
/// typed alone, for searching by them.
pub fn without(text: &str) -> String {
    replace(text, |start, end, _, _| {
        let (from, to) = markdown_span(text, start, end).unwrap_or((start, end));
        Some((from, to, String::new()))
    })
}

pub fn expand(text: &str, style: Style) -> String {
    expand_in(text, &images_dir(), &files_dir(), style)
}

/// Strips each image reference in `text` that `take` accepts, and expands the rest according to
/// `style`. When an image sits alone on its line, the line goes with it, so taking an attached
/// image out of a prompt leaves no blank line behind.
fn strip_images_in(text: &str, images: &Path, files: &Path, style: Style, mut take: impl FnMut(&str, &Path) -> bool) -> String {
    let short = shorten_in(text, images, files);
    let stripped = replace(&short, |start, end, kind, rest| {
        if kind != Kind::Image || !take(rest, &images.join(rest)) {
            return None;
        }
        let (mut from, mut to) = markdown_span(&short, start, end).unwrap_or((start, end));
        let line_start = short[..from].rfind('\n').map_or(0, |i| i + 1);
        let line_end = short[to..].find('\n').map_or(short.len(), |i| to + i);
        let blank = |s: &str| s.bytes().all(|b| b == b' ' || b == b'\t');
        if blank(&short[line_start..from]) && blank(&short[to..line_end]) {
            from = line_start;
            to = (line_end + 1).min(short.len());
        } else if blank(&short[to..line_end]) {
            from = short[..from].trim_end_matches([' ', '\t']).len();
            to = line_end;
        } else if blank(&short[line_start..from]) {
            from = line_start;
            to = line_end - short[to..line_end].trim_start_matches([' ', '\t']).len();
        } else if short[..from].ends_with(' ') && short[to..].starts_with(' ') {
            to += 1;
        }
        Some((from, to, String::new()))
    });
    expand_in(stripped.trim_matches('\n').trim_end_matches([' ', '\t']), images, files, style)
}

/// Takes the kept images out of `text` as full paths (in order, without repeats) and expands any
/// other attachments in the remaining text according to `style`, for harnesses that take images
/// through an option or an input array of their own (`codex -i`, Codex's `localImage`).
pub fn take_images_in(text: &str, images: &Path, files: &Path, style: Style) -> (String, Vec<PathBuf>) {
    let mut paths = Vec::new();
    let out = strip_images_in(text, images, files, style, |_, full| {
        if !paths.iter().any(|p| p == full) {
            paths.push(full.to_path_buf());
        }
        true
    });
    (out, paths)
}

/// Reads the kept images in `text` from disk as `(name, mime_type, base64)` (in order, without
/// repeats), taking the ones that are there out of `text` and expanding the rest according to
/// `style`, for protocols that carry image bytes alongside the text (ACP, OpenCode's service).
pub fn read_images_in(text: &str, images: &Path, files: &Path, style: Style) -> (String, Vec<(String, String, String)>) {
    let mut read: Vec<(String, String, String)> = Vec::new();
    let out = strip_images_in(text, images, files, style, |rest, full| {
        if read.iter().any(|(name, _, _)| name == rest) {
            return true;
        }
        let Ok(bytes) = std::fs::read(full) else { return false };
        read.push((rest.to_string(), mime(rest).to_string(), data_encoding::BASE64.encode(&bytes)));
        true
    });
    (out, read)
}

/// What goes ahead of a prompt for another device (actions.rs `carry_images`): each image it
/// refers to that's kept here, by its reference (`images/<hash>.<ext>`) and its file, in order
/// and without repeats. A file kept here that isn't an image can't go (a device keeps only images
/// for another: `image-keep`), so one is the error, by its name. A reference to nothing kept here
/// is left for the device asked, which may keep it itself.
pub fn going_with_in(text: &str, images: &Path, files: &Path) -> Result<Vec<(String, PathBuf)>, String> {
    let mut going = Vec::new();
    for reference in referenced(text) {
        let Some(file) = locate_in(images, files, &reference).filter(|f| f.is_file()) else { continue };
        match reference.strip_prefix(FILES) {
            Some(rest) => return Err(rest.split_once('/').map_or(rest, |(_, name)| name).to_string()),
            None => going.push((reference, file)),
        }
    }
    Ok(going)
}

/// Text that went out with full paths (`Style::Path`) as a window writes it again, each kept
/// file by its `images/…` or `files/…` reference, so it shows as it did when you sent it. Any
/// other path under the folders (a name Codealong wouldn't have made) stays as it is.
pub fn shorten_in(text: &str, images: &Path, files: &Path) -> String {
    let mut out = text.to_string();
    for (dir, prefix, valid) in [(images, IMAGES, valid_image as fn(&str) -> bool), (files, FILES, valid_file)] {
        let dir = format!("{}/", dir.to_string_lossy().trim_end_matches('/'));
        let mut short = String::with_capacity(out.len());
        let mut rest = out.as_str();
        while let Some(i) = rest.find(&dir) {
            let body = &rest[i + dir.len()..];
            let len = body.find(|c: char| !(safe_char(c) || c == '/')).unwrap_or(body.len());
            short.push_str(&rest[..i]);
            short.push_str(if valid(body[..len].trim_end_matches('.')) { prefix } else { &dir });
            rest = body;
        }
        short.push_str(rest);
        out = short;
    }
    out
}

pub fn shorten(text: &str) -> String {
    shorten_in(text, &images_dir(), &files_dir())
}

/// How text refers to a kept file named by its path alone: an image as `![image](images/…)`,
/// which shows in place, any other file as `[<name>](files/…)`.
fn reference_to(path: &str) -> Option<String> {
    if path.strip_prefix(IMAGES).is_some_and(valid_image) {
        return Some(format!("![image]({path})"));
    }
    let rest = path.strip_prefix(FILES).filter(|r| valid_file(r))?;
    Some(format!("[{}]({path})", rest.split_once('/')?.1))
}

/// A prompt as a session's record has it, written as a window writes it: each kept file by its
/// reference again, whichever way it went out, as a full path (`shorten`) or as Claude Code's
/// file mention, `@"<path>"` (`@<path>` once Claude Code's own list of files has rewritten it).
/// So its images show in place, as they did in the box it was sent from.
pub fn written_in(text: &str, images: &Path, files: &Path) -> String {
    let text = shorten_in(text, images, files);
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    for (at, _) in text.match_indices('@') {
        // A mention starts a word.
        if at < last || !text[..at].chars().next_back().map_or(true, char::is_whitespace) {
            continue;
        }
        let quoted = text[at + 1..].starts_with('"');
        let body = at + 1 + quoted as usize;
        let len = text[body..].find(|c: char| !(safe_char(c) || c == '/')).unwrap_or(text.len() - body);
        if quoted && !text[body + len..].starts_with('"') {
            continue;
        }
        // Without quotes, one at the end of a sentence doesn't take its full stop.
        let path = if quoted { &text[body..body + len] } else { text[body..body + len].trim_end_matches('.') };
        let Some(reference) = reference_to(path) else { continue };
        out.push_str(&text[last..at]);
        out.push_str(&reference);
        last = body + path.len() + quoted as usize;
    }
    out.push_str(&text[last..]);
    out
}

/// A prompt's text with the images it came with, each by its reference (`images/…`) where the
/// text names it, `[Image #2]` (Claude Code's mark for the second image pasted into a session),
/// else on a line of its own after the text. One the text already shows isn't shown twice.
pub fn with_images(text: &str, kept: &[(Option<u32>, String)]) -> String {
    let mut out = text.to_string();
    for (number, path) in kept {
        if out.contains(&format!("({path})")) {
            continue;
        }
        let reference = format!("![image]({path})");
        match number.map(|n| format!("[Image #{n}]")).filter(|mark| out.contains(mark.as_str())) {
            Some(mark) => out = out.replacen(&mark, &reference, 1),
            None => {
                if !out.is_empty() && !out.ends_with('\n') {
                    out.push('\n');
                }
                out.push_str(&reference);
            }
        }
    }
    out
}

/// Removes attachments no todo refers to any more, once they've sat unused for 30 days: one cut
/// from a description, or a todo deleted and restored, is still there for a while, and so is one
/// a session was started with for as long as Claude Code keeps the session. An image kept from a
/// session's record goes the same way, and is kept again when its conversation is next read.
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
        let (images, files) = (crate::fixtures::scratch("attach-i"), crate::fixtures::scratch("attach-f"));
        let shot = keep_in(&images, &files, PNG, "image.png").unwrap();
        assert_eq!(shot.kind, Kind::Image);
        assert!(shot.path.starts_with("images/") && shot.path.ends_with(".png"));
        assert_eq!(keep_in(&images, &files, PNG, "again.png").unwrap().path, shot.path);
        let log = keep_in(&images, &files, b"line 1\nline 2\n", "Crash report [1].log").unwrap();
        assert_eq!(log.kind, Kind::File);
        assert!(log.path.starts_with("files/") && log.path.ends_with("/Crash-report-1-.log"), "{}", log.path);
        assert_eq!(log.name, "Crash report 1.log");
        // Text refers to each so that it's found again, and written out in full when sent.
        assert_eq!(shot.reference(), format!("![image.png]({})", shot.path));
        assert_eq!(log.reference(), format!("[Crash report 1.log]({})", log.path));
        assert_eq!(referenced(&format!("{}\n{}", shot.reference(), log.reference())), vec![shot.path.clone(), log.path.clone()]);
        assert_eq!(std::fs::read(locate_in(&images, &files, &log.path).unwrap()).unwrap(), b"line 1\nline 2\n");
        // An image a window can't show is a file like any other.
        let svg = keep_in(&images, &files, b"<svg xmlns='http://www.w3.org/2000/svg'/>", "logo.svg").unwrap();
        assert_eq!((svg.kind, svg.name.as_str()), (Kind::File, "logo.svg"));
    }

    #[test]
    fn imports_files_but_not_folders() {
        let dir = crate::fixtures::scratch("attach-import");
        let file = dir.join("notes.md");
        std::fs::write(&file, "# Notes").unwrap();
        let (images, files) = (dir.join("images"), dir.join("files"));
        // import() writes to the real folders, so its parts are checked here on temp ones.
        let bytes = std::fs::read(&file).unwrap();
        let a = keep_in(&images, &files, &bytes, "notes.md").unwrap();
        assert!(a.path.ends_with("/notes.md"));
        assert!(import(&dir).unwrap_err().to_string().contains("is a folder"));
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
    fn images_are_swapped_whole_and_files_left() {
        let text = "Fix ![image](images/0123456789abcdef.png) like images/fedcba9876543210.jpg, see [a.log](files/0123456789abcdef/a.log)";
        let mut seen = Vec::new();
        let out = swap_images(text, |name| {
            seen.push(name.to_string());
            format!("<{}>", seen.len())
        });
        assert_eq!(out, "Fix <1> like <2>, see [a.log](files/0123456789abcdef/a.log)");
        assert_eq!(seen, ["0123456789abcdef.png", "fedcba9876543210.jpg"]);
        assert_eq!(swap_images("no images here", |_| unreachable!()), "no images here");
        assert_eq!(without(text), "Fix  like , see ");
    }

    #[test]
    fn sent_text_shortens_back_to_references() {
        let (images, files) = (Path::new("/Users/me/Library/Application Support/Codealong/images"), Path::new("/Users/me/Library/Application Support/Codealong/files"));
        let text = "The export fails:\n[Crash report 1.log](files/0123456789abcdef/Crash-report-1-.log)\nand ![image](images/fedcba9876543210.png), not /Users/me/images/x.png";
        assert_eq!(shorten_in(&expand_in(text, images, files, Style::Path), images, files), text);
        // A path there that's no kept file's (a listing of the folder, say) is left whole.
        let listing = "ls /Users/me/Library/Application Support/Codealong/images/ shows /Users/me/Library/Application Support/Codealong/images/notes.txt";
        assert_eq!(shorten_in(listing, images, files), listing);
    }

    #[test]
    fn a_recorded_prompt_reads_as_it_was_written() {
        let (images, files) = (Path::new("/Users/me/Library/Application Support/Codealong/images"), Path::new("/Users/me/Library/Application Support/Codealong/files"));
        let text = "Seeing this\n![image](images/0123456789abcdef.png)\nand [Crash report 1.log](files/0123456789abcdef/Crash-report-1-.log), then images/fedcba9876543210.jpg.";
        // As Claude Code got it, each file a mention: the image shows again, the file by its kept name.
        assert_eq!(
            written_in(&expand_in(text, images, files, Style::Mention), images, files),
            "Seeing this\n![image](images/0123456789abcdef.png)\nand [Crash-report-1-.log](files/0123456789abcdef/Crash-report-1-.log), then ![image](images/fedcba9876543210.jpg)."
        );
        // As the others got it, by full path: just as it was written.
        assert_eq!(written_in(&expand_in(text, images, files, Style::Path), images, files), text);
        // A mention Claude Code's list of files rewrote has lost its quotes; a trailing space is the box's.
        assert_eq!(written_in("Look @/Users/me/Library/Application Support/Codealong/images/0123456789abcdef.png ", images, files), "Look ![image](images/0123456789abcdef.png) ");
        // Other mentions, an address, and a quote that never closes stay as they are.
        for same in ["See @src/lanes.ts and @\"my notes.md\"", "mail me@images/0123456789abcdef.png", "@\"images/0123456789abcdef.png", "@images/nope.png", "@\"/Users/me/Desktop/images/0123456789abcdef.png\""] {
            assert_eq!(written_in(same, images, files), same);
        }
    }

    #[test]
    fn images_a_prompt_came_with_show_where_it_names_them() {
        let kept = |n: Option<u32>, name: &str| (n, format!("images/{name}"));
        let (a, b) = ("0123456789abcdef.png", "fedcba9876543210.webp");
        // Claude Code numbers what's pasted into a session, and the text says where each went.
        assert_eq!(with_images("Before [Image #3] and after [Image #4]", &[kept(Some(3), a), kept(Some(4), b)]), format!("Before ![image](images/{a}) and after ![image](images/{b})"));
        // Attached in an app, the text doesn't name them: each goes after it, on its own line.
        assert_eq!(with_images("Why is this cut off?", &[kept(None, a), kept(Some(2), b)]), format!("Why is this cut off?\n![image](images/{a})\n![image](images/{b})"));
        assert_eq!(with_images("", &[kept(None, a)]), format!("![image](images/{a})"));
        // One the text already shows (Codealong sent it by path, and the harness attached it) shows once.
        let sent = format!("Look:\n![image](images/{a})");
        assert_eq!(with_images(&sent, &[kept(None, a)]), sent);
        assert_eq!(with_images("[Image #1]", &[]), "[Image #1]");
    }

    #[test]
    fn only_images_a_window_can_show_are_kept_from_a_record() {
        let (images, files) = (crate::fixtures::scratch("attach-rec-i"), crate::fixtures::scratch("attach-rec-f"));
        let path = keep_image_in(&images, PNG).unwrap();
        // The same name as when it's pasted into Codealong, so one sent from here is kept once.
        assert_eq!(path, keep_in(&images, &files, PNG, "image.png").unwrap().path);
        assert!(locate_in(&images, &files, &path).unwrap().exists());
        assert_eq!(keep_image_in(&images, b"<svg xmlns='http://www.w3.org/2000/svg'/>"), None);
        assert_eq!(keep_image_in(&images, b""), None);
        assert_eq!(std::fs::read_dir(&*files).unwrap().count(), 0, "nothing else is kept");
    }

    #[test]
    fn windows_keeping_the_same_new_image_at_once_all_get_it() {
        // Each round, a folder without the image and threads that all keep it at the same moment,
        // as windows reading one conversation with an image not seen before do.
        for _ in 0..30 {
            let images = crate::fixtures::scratch("attach-race");
            let start = std::sync::Arc::new(std::sync::Barrier::new(4));
            let threads: Vec<_> = (0..4)
                .map(|_| {
                    let (images, start) = (images.to_path_buf(), start.clone());
                    std::thread::spawn(move || {
                        start.wait();
                        keep_image_in(&images, PNG)
                    })
                })
                .collect();
            let kept: Vec<Option<String>> = threads.into_iter().map(|t| t.join().unwrap()).collect();
            assert!(kept.iter().all(|k| k.is_some()), "every window shows the image: {kept:?}");
            // The image, whole, and nothing else left in the folder.
            let names: Vec<_> = std::fs::read_dir(&*images).unwrap().map(|e| e.unwrap().file_name()).collect();
            assert_eq!(names.len(), 1, "{names:?}");
            assert_eq!(std::fs::read(images.join(&names[0])).unwrap(), PNG);
        }
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
        let (images, files) = (crate::fixtures::scratch("attach-gc-i"), crate::fixtures::scratch("attach-gc-f"));
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
    }

    #[test]
    fn takes_images_out_of_the_text_and_expands_other_files() {
        let (images, files) = (Path::new("/Users/me/Library/Application Support/Codealong/images"), Path::new("/f"));
        let a = "0123456789abcdef.png";
        let b = "fedcba9876543210.webp";
        let fh = "0123456789abcdef";
        let (text, taken) = take_images_in(&format!("Match this:\n![image](images/{a})\nand [crash.log](files/{fh}/crash.log)\n![shot](images/{b})\n![again](images/{a})"), images, files, Style::Path);
        assert_eq!(text, format!("Match this:\nand [crash.log](/f/{fh}/crash.log)"));
        assert_eq!(taken, vec![images.join(a), images.join(b)]);
        // Works the same when the text was expanded already, or has only images.
        let expanded = expand_in(&format!("Match this:\n![image](images/{a})"), images, files, Style::Path);
        assert_eq!(take_images_in(&expanded, images, files, Style::Path), ("Match this:".to_string(), vec![images.join(a)]));
        assert_eq!(take_images_in(&format!("![image](images/{a})\n![image](images/{b})"), images, files, Style::Path), (String::new(), vec![images.join(a), images.join(b)]));
        assert_eq!(take_images_in(&format!("Before ![image](images/{a}) after"), images, files, Style::Path), ("Before after".to_string(), vec![images.join(a)]));
    }

    #[test]
    fn reads_kept_images_from_disk_and_leaves_missing_ones_as_paths() {
        let (images, files) = (crate::fixtures::scratch("attach-read-i"), crate::fixtures::scratch("attach-read-f"));
        let kept = keep_in(&images, &files, PNG, "shot.png").unwrap();
        let missing = "0123456789abcdef.jpg";
        let prompt = format!("Check this:\n![image]({})\nand ![missing](images/{missing})", kept.path);
        let (text, read) = read_images_in(&prompt, &images, &files, Style::Path);
        assert_eq!(text, format!("Check this:\nand ![missing]({}/{missing})", images.display()));
        let name = kept.path.strip_prefix(IMAGES).unwrap();
        assert_eq!(read, vec![(name.to_string(), "image/png".to_string(), data_encoding::BASE64.encode(PNG))]);
    }

    #[test]
    fn a_prompt_for_another_device_takes_the_images_kept_here() {
        let (images, files) = (crate::fixtures::scratch("attach-going-i"), crate::fixtures::scratch("attach-going-f"));
        let shot = keep_in(&images, &files, PNG, "shot.png").unwrap();
        // Twice in the text, and one the device asked keeps, not this one: it goes once, alone.
        let elsewhere = "images/0123456789abcdef.jpg";
        let prompt = format!("Fix this:\n{}\nas in {elsewhere}, and ![again]({})", shot.reference(), shot.path);
        assert_eq!(going_with_in(&prompt, &images, &files), Ok(vec![(shot.path.clone(), images.join(shot.path.strip_prefix(IMAGES).unwrap()))]));
        assert_eq!(going_with_in("Nothing attached", &images, &files), Ok(vec![]));
        // Any other file kept here can't go, and says which.
        let log = keep_in(&images, &files, b"line 1\n", "Crash report.log").unwrap();
        assert_eq!(going_with_in(&format!("{}\n{}", shot.reference(), log.reference()), &images, &files), Err("Crash-report.log".to_string()));
    }
}
