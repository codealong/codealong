//! Renaming Antigravity conversations where Antigravity keeps its titles.
//!
//! `agy`'s `/rename` (checked on 1.5.15, 2026-11-23) writes a conversation's title to three places
//! in its folder (`~/.gemini/antigravity-cli` or `~/.gemini/antigravity`):
//! - `annotations/<id>.pbtxt` (`exa.jetski_cortex_pb.ConversationAnnotations` in protobuf text
//!   format, top-level `title:"…"`), preserving every other annotation field;
//! - `conversation_summaries.db` (`title`, `preview`, and `raw_summary`, a
//!   `CascadeTrajectorySummary` proto whose field 1 is `summary` and field 15 is `annotations`
//!   with field 1 `title`), leaving `last_modified_time` untouched;
//! - `jetbox_summaries_proto.pb` (CLI) or `agyhub_summaries_proto.pb` (hub app), a
//!   `SummariesState` proto (`map<string, CascadeTrajectorySummary> summaries = 1`) that every
//!   starting `agy` reconciles back into `conversation_summaries.db`.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{bail, Context, Result};
use rusqlite::{params, Connection, OptionalExtension};

use super::SUMMARIES;

const ANNOTATIONS: &str = "annotations";
const SUMMARY_PROTOS: &[&str] = &["jetbox_summaries_proto.pb", "agyhub_summaries_proto.pb"];

/// Renames `id` in `dir` across Antigravity's summary database, annotation file and summary
/// snapshot, returning the database path so the engine rereads it at once.
pub(super) fn rename(dir: &Path, id: &str, current_title: &str, name: &str) -> Result<Vec<PathBuf>> {
    if id.is_empty() || id.contains('/') || id.contains('\\') || id.contains("..") {
        bail!("Invalid Antigravity conversation ID.");
    }
    let db_path = dir.join(SUMMARIES);
    update_summary_db(&db_path, id, current_title, name)?;
    update_annotations_file(&dir.join(ANNOTATIONS).join(format!("{id}.pbtxt")), name)?;
    for file in SUMMARY_PROTOS {
        let proto_path = dir.join(file);
        if proto_path.is_file() {
            update_summaries_proto(&proto_path, id, name)?;
        }
    }
    Ok(vec![db_path])
}

/// The custom title kept in `annotations/<id>.pbtxt`, when the file exists and carries one.
/// Reading it alongside `conversation_summaries.db` keeps a rename made here from reverting when
/// an already-running `agy` writes its older in-memory summary back at the end of a turn.
pub(super) fn annotation_title(dir: &Path, id: &str) -> Option<String> {
    if id.is_empty() || id.contains('/') || id.contains('\\') || id.contains("..") {
        return None;
    }
    let text = std::fs::read_to_string(dir.join(ANNOTATIONS).join(format!("{id}.pbtxt"))).ok()?;
    parse_pbtxt_title(&text).map(|t| t.trim().to_string()).filter(|t| !t.is_empty())
}

fn update_summary_db(db_path: &Path, id: &str, current_title: &str, name: &str) -> Result<()> {
    let missing = || anyhow::anyhow!("“{current_title}” has no summary row yet to keep a name in: Antigravity writes one at its first prompt.");
    if !db_path.is_file() {
        return Err(missing());
    }
    let db = Connection::open(db_path).with_context(|| format!("Couldn't open {}", db_path.display()))?;
    let _ = db.busy_timeout(Duration::from_secs(5));

    let raw: Option<Vec<u8>> = match db.query_row("SELECT raw_summary FROM conversation_summaries WHERE conversation_id = ?1", [id], |r| r.get::<_, Option<Vec<u8>>>(0)).optional() {
        Ok(Some(raw)) => raw,
        Ok(None) => return Err(missing()),
        Err(_) => {
            // Older summary tables have no `raw_summary` column.
            let exists: Option<i64> = db.query_row("SELECT 1 FROM conversation_summaries WHERE conversation_id = ?1", [id], |r| r.get(0)).optional()?;
            if exists.is_none() {
                return Err(missing());
            }
            None
        }
    };

    let updated_raw = raw.as_deref().filter(|b| !b.is_empty()).and_then(|b| set_summary_title(b, name.as_bytes()));
    let changed = if let Some(new_raw) = updated_raw {
        db.execute("UPDATE conversation_summaries SET title = ?1, preview = ?1, raw_summary = ?2 WHERE conversation_id = ?3", params![name, new_raw, id])?
    } else {
        db.execute("UPDATE conversation_summaries SET title = ?1, preview = ?1 WHERE conversation_id = ?2", params![name, id])?
    };
    if changed == 0 {
        return Err(missing());
    }
    Ok(())
}

fn update_annotations_file(path: &Path, name: &str) -> Result<()> {
    let existing = crate::store::text(path)?;
    let updated = update_pbtxt_title(existing.as_deref().unwrap_or(""), name);
    crate::store::write_private(path, updated.as_bytes())
}

fn update_summaries_proto(path: &Path, id: &str, name: &str) -> Result<()> {
    let bytes = std::fs::read(path).with_context(|| format!("Couldn't read {}", path.display()))?;
    if let Some(updated) = set_summaries_state_title(&bytes, id, name.as_bytes()) {
        crate::store::write_private(path, &updated)?;
    }
    Ok(())
}

/* ---- Protobuf text format (`annotations/<id>.pbtxt`) ---- */

fn escape_pbtxt(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 || c == '\x7f' => {
                for b in c.to_string().as_bytes() {
                    out.push_str(&format!("\\x{b:02x}"));
                }
            }
            c => out.push(c),
        }
    }
    out
}

/// Replaces the top-level `title` field in a `ConversationAnnotations` textproto, or adds one
/// while preserving every other field verbatim.
fn update_pbtxt_title(text: &str, name: &str) -> String {
    let field = format!("title:\"{}\"", escape_pbtxt(name));
    if let Some((start, end, _)) = find_top_level_title(text) {
        let mut out = String::with_capacity(text.len() + field.len());
        out.push_str(&text[..start]);
        out.push_str(&field);
        out.push_str(&text[end..]);
        return out;
    }
    if text.trim().is_empty() {
        return field;
    }
    let mut out = text.to_string();
    if !out.ends_with('\n') && !out.ends_with(' ') {
        out.push('\n');
    }
    out.push_str(&field);
    if text.ends_with('\n') {
        out.push('\n');
    }
    out
}

/// Extracts the decoded top-level `title` string from a `ConversationAnnotations` textproto.
fn parse_pbtxt_title(text: &str) -> Option<String> {
    find_top_level_title(text).map(|(_, _, value)| value)
}

/// Scans a protobuf text format message for a top-level `title` string field, returning its byte
/// span `[start..end)` (from `title` through the end of its string literal(s)) and decoded value.
fn find_top_level_title(text: &str) -> Option<(usize, usize, String)> {
    let b = text.as_bytes();
    let mut i = 0;
    let mut depth: usize = 0;
    while i < b.len() {
        match b[i] {
            b'#' => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
            }
            b'"' | b'\'' => {
                let (_, next) = parse_pbtxt_string(b, i)?;
                i = next;
            }
            b'{' | b'<' => {
                depth += 1;
                i += 1;
            }
            b'}' | b'>' => {
                depth = depth.saturating_sub(1);
                i += 1;
            }
            c if is_ident_start(c) => {
                let start = i;
                while i < b.len() && is_ident_cont(b[i]) {
                    i += 1;
                }
                let ident = &text[start..i];
                if depth == 0 && ident == "title" {
                    let mut j = skip_ws_and_comments(b, i);
                    if j < b.len() && b[j] == b':' {
                        j = skip_ws_and_comments(b, j + 1);
                        if j < b.len() && (b[j] == b'"' || b[j] == b'\'') {
                            let mut decoded = Vec::new();
                            let mut end = j;
                            while j < b.len() && (b[j] == b'"' || b[j] == b'\'') {
                                let (chunk, next) = parse_pbtxt_string(b, j)?;
                                decoded.extend_from_slice(&chunk);
                                end = next;
                                j = skip_ws_and_comments(b, next);
                            }
                            return Some((start, end, String::from_utf8_lossy(&decoded).into_owned()));
                        }
                    }
                }
            }
            _ => i += 1,
        }
    }
    None
}

fn is_ident_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'_'
}

fn is_ident_cont(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

fn skip_ws_and_comments(b: &[u8], mut i: usize) -> usize {
    while i < b.len() {
        if b[i].is_ascii_whitespace() {
            i += 1;
        } else if b[i] == b'#' {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
        } else {
            break;
        }
    }
    i
}

/// Parses one protobuf text format string literal (`"..."` or `'...'`), decoding C/Go prototext
/// escapes (`\\`, `\"`, `\'`, `\n`, `\r`, `\t`, `\NNN` octal bytes, `\xHH` hex bytes, `\uHHHH`
/// and `\UHHHHHHHH`).
fn parse_pbtxt_string(b: &[u8], start: usize) -> Option<(Vec<u8>, usize)> {
    let quote = *b.get(start)?;
    let mut i = start + 1;
    let mut out = Vec::new();
    while i < b.len() {
        let c = b[i];
        if c == quote {
            return Some((out, i + 1));
        }
        if c != b'\\' {
            out.push(c);
            i += 1;
            continue;
        }
        i += 1;
        let esc = *b.get(i)?;
        match esc {
            b'\\' | b'"' | b'\'' | b'?' => {
                out.push(esc);
                i += 1;
            }
            b'a' => {
                out.push(0x07);
                i += 1;
            }
            b'b' => {
                out.push(0x08);
                i += 1;
            }
            b'f' => {
                out.push(0x0c);
                i += 1;
            }
            b'n' => {
                out.push(b'\n');
                i += 1;
            }
            b'r' => {
                out.push(b'\r');
                i += 1;
            }
            b't' => {
                out.push(b'\t');
                i += 1;
            }
            b'v' => {
                out.push(0x0b);
                i += 1;
            }
            b'0'..=b'7' => {
                let mut val = (esc - b'0') as u32;
                i += 1;
                for _ in 0..2 {
                    if let Some(&d @ b'0'..=b'7') = b.get(i) {
                        val = (val << 3) | (d - b'0') as u32;
                        i += 1;
                    } else {
                        break;
                    }
                }
                out.push((val & 0xff) as u8);
            }
            b'x' | b'X' => {
                i += 1;
                let mut val = 0u32;
                let mut digits = 0;
                while digits < 2 {
                    if let Some(d) = b.get(i).and_then(|&c| hex_digit(c)) {
                        val = (val << 4) | d;
                        i += 1;
                        digits += 1;
                    } else {
                        break;
                    }
                }
                if digits == 0 {
                    return None;
                }
                out.push((val & 0xff) as u8);
            }
            b'u' | b'U' => {
                let want = if esc == b'u' { 4 } else { 8 };
                i += 1;
                let mut val = 0u32;
                for _ in 0..want {
                    let d = hex_digit(*b.get(i)?)?;
                    val = (val << 4) | d;
                    i += 1;
                }
                let ch = char::from_u32(val)?;
                let mut buf = [0u8; 4];
                out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
            }
            other => {
                out.push(other);
                i += 1;
            }
        }
    }
    None
}

fn hex_digit(c: u8) -> Option<u32> {
    match c {
        b'0'..=b'9' => Some((c - b'0') as u32),
        b'a'..=b'f' => Some((c - b'a' + 10) as u32),
        b'A'..=b'F' => Some((c - b'A' + 10) as u32),
        _ => None,
    }
}

/* ---- Protobuf wire-format editing (`raw_summary` and `*_summaries_proto.pb`) ---- */

fn push_varint(out: &mut Vec<u8>, mut n: u64) {
    loop {
        let byte = (n & 0x7f) as u8;
        n >>= 7;
        if n != 0 {
            out.push(byte | 0x80);
        } else {
            out.push(byte);
            break;
        }
    }
}

fn push_bytes_field(out: &mut Vec<u8>, field: u64, value: &[u8]) {
    push_varint(out, (field << 3) | 2);
    push_varint(out, value.len() as u64);
    out.extend_from_slice(value);
}

fn read_varint(b: &[u8], i: &mut usize) -> Option<u64> {
    let mut n = 0u64;
    let mut shift = 0u32;
    while *i < b.len() && shift < 64 {
        let byte = b[*i];
        *i += 1;
        n |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Some(n);
        }
        shift += 7;
    }
    None
}

/// One raw protobuf field from a wire-format message: `(field_number, wire_type, raw_bytes,
/// length_delimited_payload)`.
struct RawField<'a> {
    field: u64,
    wire: u64,
    raw: &'a [u8],
    bytes: Option<&'a [u8]>,
}

/// Parses every wire-format field of `b`, returning `None` on truncated or unsupported wire types
/// so unknown data is never corrupted.
fn parse_raw_fields(b: &[u8]) -> Option<Vec<RawField<'_>>> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let start = i;
        let tag = read_varint(b, &mut i)?;
        let (field, wire) = (tag >> 3, tag & 7);
        if field == 0 {
            return None;
        }
        let bytes = match wire {
            0 => {
                read_varint(b, &mut i)?;
                None
            }
            1 => {
                let end = i.checked_add(8)?;
                if end > b.len() {
                    return None;
                }
                i = end;
                None
            }
            2 => {
                let len = read_varint(b, &mut i)? as usize;
                let end = i.checked_add(len)?;
                let slice = b.get(i..end)?;
                i = end;
                Some(slice)
            }
            5 => {
                let end = i.checked_add(4)?;
                if end > b.len() {
                    return None;
                }
                i = end;
                None
            }
            _ => return None,
        };
        out.push(RawField { field, wire, raw: &b[start..i], bytes });
    }
    Some(out)
}

/// Sets field 1 (`title`) in a `ConversationAnnotations` proto, preserving every other field.
fn set_annotations_title(ann: &[u8], title: &[u8]) -> Option<Vec<u8>> {
    let fields = parse_raw_fields(ann)?;
    let mut out = Vec::with_capacity(ann.len() + title.len() + 8);
    let mut replaced = false;
    for f in fields {
        if f.field == 1 {
            if !replaced {
                push_bytes_field(&mut out, 1, title);
                replaced = true;
            }
        } else {
            out.extend_from_slice(f.raw);
        }
    }
    if !replaced {
        push_bytes_field(&mut out, 1, title);
    }
    Some(out)
}

/// Sets field 1 (`summary`) and field 15 (`annotations.title`) in a `CascadeTrajectorySummary`
/// proto, preserving every other field verbatim.
fn set_summary_title(summary: &[u8], title: &[u8]) -> Option<Vec<u8>> {
    let fields = parse_raw_fields(summary)?;
    let mut out = Vec::with_capacity(summary.len() + title.len() * 2 + 16);
    let mut got_summary = false;
    let mut got_annotations = false;
    for f in fields {
        if f.field == 1 {
            if !got_summary {
                push_bytes_field(&mut out, 1, title);
                got_summary = true;
            }
        } else if f.field == 15 && f.wire == 2 {
            if !got_annotations {
                let updated = set_annotations_title(f.bytes?, title)?;
                push_bytes_field(&mut out, 15, &updated);
                got_annotations = true;
            }
        } else {
            out.extend_from_slice(f.raw);
        }
    }
    if !got_summary {
        push_bytes_field(&mut out, 1, title);
    }
    if !got_annotations {
        let mut ann = Vec::with_capacity(title.len() + 4);
        push_bytes_field(&mut ann, 1, title);
        push_bytes_field(&mut out, 15, &ann);
    }
    Some(out)
}

/// Updates the `summaries[id]` entry in a `SummariesState` proto (`map<string,
/// CascadeTrajectorySummary> summaries = 1`). Returns `None` when the file doesn't parse or has
/// no entry for `id`.
fn set_summaries_state_title(state: &[u8], id: &str, title: &[u8]) -> Option<Vec<u8>> {
    let fields = parse_raw_fields(state)?;
    let mut out = Vec::with_capacity(state.len() + title.len() * 2 + 32);
    let mut found = false;
    for f in fields {
        if f.field == 1 && f.wire == 2 {
            let entry_fields = parse_raw_fields(f.bytes?)?;
            let is_target = entry_fields.iter().any(|ef| ef.field == 1 && ef.bytes == Some(id.as_bytes()));
            if is_target {
                let mut rebuilt = Vec::with_capacity(f.bytes?.len() + title.len() * 2 + 16);
                let mut updated_value = false;
                for ef in entry_fields {
                    if ef.field == 2 && ef.wire == 2 {
                        let summary = set_summary_title(ef.bytes?, title)?;
                        push_bytes_field(&mut rebuilt, 2, &summary);
                        updated_value = true;
                    } else {
                        rebuilt.extend_from_slice(ef.raw);
                    }
                }
                if !updated_value {
                    return None;
                }
                push_bytes_field(&mut out, 1, &rebuilt);
                found = true;
                continue;
            }
        }
        out.extend_from_slice(f.raw);
    }
    found.then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::antigravity::{proto_at, Antigravity, Reader};
    use crate::config::AccountCfg;
    use crate::harness::{Adapter, Ctx, Look, Reader as HarnessReader};
    use crate::model::{Harness, Session, Surface};

    fn make_summary(summary: &str, step_count: u64, ann: Option<&[u8]>) -> Vec<u8> {
        let mut out = Vec::new();
        push_bytes_field(&mut out, 1, summary.as_bytes());
        push_varint(&mut out, (2 << 3) | 0);
        push_varint(&mut out, step_count);
        if let Some(a) = ann {
            push_bytes_field(&mut out, 15, a);
        }
        out
    }

    fn make_summaries_state(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out = Vec::new();
        for (k, v) in entries {
            let mut entry = Vec::new();
            push_bytes_field(&mut entry, 1, k.as_bytes());
            push_bytes_field(&mut entry, 2, v);
            push_bytes_field(&mut out, 1, &entry);
        }
        out
    }

    fn setup_db(dir: &Path, rows: &[(&str, &str, &str, &str, &[u8])]) {
        let db = Connection::open(dir.join(SUMMARIES)).unwrap();
        db.execute_batch(
            "CREATE TABLE conversation_summaries (
                conversation_id TEXT PRIMARY KEY,
                title TEXT NOT NULL DEFAULT '',
                preview TEXT NOT NULL DEFAULT '',
                step_count INTEGER NOT NULL DEFAULT 0,
                last_modified_time TEXT NOT NULL,
                workspace_uris TEXT NOT NULL DEFAULT '[]',
                status TEXT NOT NULL DEFAULT 'CASCADE_RUN_STATUS_IDLE',
                source TEXT NOT NULL DEFAULT '',
                parent_conversation_id TEXT NOT NULL DEFAULT '',
                nesting_depth INTEGER NOT NULL DEFAULT 0,
                not_fully_idle INTEGER NOT NULL DEFAULT 0,
                killed INTEGER NOT NULL DEFAULT 0,
                last_user_input_time TEXT NOT NULL DEFAULT '',
                app_data_dir TEXT NOT NULL DEFAULT '',
                raw_summary BLOB
            );",
        )
        .unwrap();
        for (id, title, preview, modified, raw) in rows {
            db.execute(
                "INSERT INTO conversation_summaries (conversation_id, title, preview, step_count, last_modified_time, workspace_uris, raw_summary) VALUES (?1, ?2, ?3, 5, ?4, '[\"file:///repo\"]', ?5)",
                params![id, title, preview, modified, raw],
            )
            .unwrap();
        }
    }

    #[test]
    fn pbtxt_updates_and_preserves_other_fields_and_escapes() {
        // Fresh annotation file with quotes, backslash and UTF-8.
        let fresh = update_pbtxt_title("", "Fix \"auth\" — path\\1");
        assert_eq!(fresh, "title:\"Fix \\\"auth\\\" — path\\\\1\"");
        assert_eq!(parse_pbtxt_title(&fresh).as_deref(), Some("Fix \"auth\" — path\\1"));

        // Preserves other fields (such as hub app's last_user_view_time) and replaces existing title.
        let existing = "last_user_view_time:{seconds:1791929382 nanos:458000000}\ntitle:\"Old title\"\npinned:true\n";
        let updated = update_pbtxt_title(existing, "New title");
        assert_eq!(updated, "last_user_view_time:{seconds:1791929382 nanos:458000000}\ntitle:\"New title\"\npinned:true\n");
        assert_eq!(parse_pbtxt_title(&updated).as_deref(), Some("New title"));

        // Appends when other fields exist without a title, and ignores nested `title` fields.
        let nested = "nested:{title:\"Inner\"}\nlast_user_view_time:{seconds:1}\n";
        let appended = update_pbtxt_title(nested, "Outer");
        assert_eq!(appended, "nested:{title:\"Inner\"}\nlast_user_view_time:{seconds:1}\ntitle:\"Outer\"\n");
        assert_eq!(parse_pbtxt_title(&appended).as_deref(), Some("Outer"));

        // Decodes Go prototext octal-escaped UTF-8 and adjacent string literals.
        assert_eq!(parse_pbtxt_title("title:\"a\\342\\200\\224b\" \" c\"").as_deref(), Some("a—b c"));
    }

    #[test]
    fn renames_db_annotations_and_summary_proto_without_touching_mtime_or_neighbors() {
        let root = crate::fixtures::scratch("antigravity-rename");
        let cli = root.join("antigravity-cli");
        std::fs::create_dir_all(&cli).unwrap();
        let modified = "2026-11-23 10:00:00+00:00";

        let mut ann_with_pin = Vec::new();
        push_bytes_field(&mut ann_with_pin, 1, b"Old Name");
        // Field 5 = starred (bool varint = 1)
        push_varint(&mut ann_with_pin, (5 << 3) | 0);
        push_varint(&mut ann_with_pin, 1);

        let raw_a = make_summary("Old Name", 12, Some(&ann_with_pin));
        let raw_b = make_summary("Other Session", 4, None);
        setup_db(&cli, &[("c1", "Old Name", "Old Name", modified, &raw_a), ("c2", "Other Session", "Other Session", modified, &raw_b)]);
        std::fs::write(cli.join("jetbox_summaries_proto.pb"), make_summaries_state(&[("c1", &raw_a), ("c2", &raw_b)])).unwrap();

        let changed = rename(&cli, "c1", "Old Name", "Renamed Session").unwrap();
        assert_eq!(changed, vec![cli.join(SUMMARIES)]);

        // 1. annotations/<id>.pbtxt written.
        assert_eq!(std::fs::read_to_string(cli.join("annotations/c1.pbtxt")).unwrap(), "title:\"Renamed Session\"");

        // 2. conversation_summaries.db updated (title, preview, raw_summary), last_modified_time unchanged, c2 untouched.
        let db = Connection::open(cli.join(SUMMARIES)).unwrap();
        let (title, preview, ts, raw): (String, String, String, Vec<u8>) =
            db.query_row("SELECT title, preview, last_modified_time, raw_summary FROM conversation_summaries WHERE conversation_id = 'c1'", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))).unwrap();
        assert_eq!((title.as_str(), preview.as_str(), ts.as_str()), ("Renamed Session", "Renamed Session", modified));
        assert_eq!(proto_at(&raw, &[1]), vec![b"Renamed Session".as_slice()]);
        assert_eq!(proto_at(&raw, &[15, 1]), vec![b"Renamed Session".as_slice()]);
        // Other annotation fields (field 5 = starred) stayed intact.
        let ann_bytes = proto_at(&raw, &[15])[0];
        assert!(parse_raw_fields(ann_bytes).unwrap().iter().any(|f| f.field == 5 && f.wire == 0));

        let other: String = db.query_row("SELECT title FROM conversation_summaries WHERE conversation_id = 'c2'", [], |r| r.get(0)).unwrap();
        assert_eq!(other, "Other Session");

        // 3. jetbox_summaries_proto.pb updated for c1 and kept c2 byte-for-byte.
        let pb = std::fs::read(cli.join("jetbox_summaries_proto.pb")).unwrap();
        let entries = parse_raw_fields(&pb).unwrap();
        let mut got_c1 = None;
        let mut got_c2 = None;
        for f in entries {
            let inner = parse_raw_fields(f.bytes.unwrap()).unwrap();
            let k = inner.iter().find(|x| x.field == 1).unwrap().bytes.unwrap();
            let v = inner.iter().find(|x| x.field == 2).unwrap().bytes.unwrap();
            if k == b"c1" {
                got_c1 = Some(v.to_vec());
            } else if k == b"c2" {
                got_c2 = Some(v.to_vec());
            }
        }
        let got_c1 = got_c1.unwrap();
        assert_eq!(proto_at(&got_c1, &[1]), vec![b"Renamed Session".as_slice()]);
        assert_eq!(proto_at(&got_c1, &[15, 1]), vec![b"Renamed Session".as_slice()]);
        assert_eq!(got_c2.unwrap(), raw_b);
    }

    #[test]
    fn adapter_rename_rereads_and_annotation_survives_stale_db_overwrite() {
        let root = crate::fixtures::scratch("antigravity-rename-reread");
        let app_dir = root.join("antigravity");
        let cli_dir = root.join("antigravity-cli");
        std::fs::create_dir_all(&app_dir).unwrap();
        std::fs::create_dir_all(&cli_dir).unwrap();

        let raw = make_summary("Original", 3, None);
        setup_db(&cli_dir, &[("c1", "Original", "Original", "2026-11-23 10:00:00+00:00", &raw)]);
        std::fs::write(cli_dir.join("jetbox_summaries_proto.pb"), make_summaries_state(&[("c1", &raw)])).unwrap();

        let account = AccountCfg { name: "antigravity".into(), harness: "antigravity".into(), home: app_dir.to_string_lossy().into_owned(), command: None, start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false, chrome_profile: None };
        let mut session = Session::new(Harness::Antigravity, "antigravity", "c1");
        session.surface = Surface::Cli;
        session.title = "Original".into();

        let seen = crate::state::SeenState::default();
        let look = Look { seen: &seen, since: 0, now: 1_795_400_000_000, ps: Default::default() };
        let mut reader = Reader::default();
        reader.reread(&account, &look);
        assert_eq!(reader.sessions(&account, &look)[0].title, "Original");

        let changed = Antigravity.rename(&Ctx { account: &account }, &session, "Custom Title").unwrap();
        let applied = reader.apply(&account, &changed, &look);
        assert!(applied.dirty);
        assert_eq!(reader.sessions(&account, &look)[0].title, "Custom Title");

        // Simulate a running `agy` writing its stale in-memory title back to the summary table.
        let db = Connection::open(cli_dir.join(SUMMARIES)).unwrap();
        db.execute("UPDATE conversation_summaries SET title = 'Original', preview = 'Original' WHERE conversation_id = 'c1'", []).unwrap();
        reader.apply(&account, &changed, &look);
        assert_eq!(reader.sessions(&account, &look)[0].title, "Custom Title");
    }

    #[test]
    fn rename_errors_when_summary_row_missing_and_writes_no_annotation() {
        let root = crate::fixtures::scratch("antigravity-rename-missing");
        setup_db(&root, &[]);
        let err = rename(&root, "missing", "Untitled", "New Name").unwrap_err();
        assert!(err.to_string().contains("no summary row yet"));
        assert!(!root.join("annotations/missing.pbtxt").exists());
    }

    #[test]
    fn rename_updates_older_table_without_raw_summary_column_and_hub_app_proto() {
        let root = crate::fixtures::scratch("antigravity-rename-hub");
        let db = Connection::open(root.join(SUMMARIES)).unwrap();
        db.execute_batch(
            "CREATE TABLE conversation_summaries (
                conversation_id TEXT PRIMARY KEY,
                title TEXT,
                preview TEXT
            );
            INSERT INTO conversation_summaries VALUES ('h1', '', 'Hello');",
        )
        .unwrap();
        std::fs::create_dir_all(root.join("annotations")).unwrap();
        std::fs::write(root.join("annotations/h1.pbtxt"), "last_user_view_time:{seconds:123}").unwrap();
        let raw = make_summary("Hello", 2, None);
        std::fs::write(root.join("agyhub_summaries_proto.pb"), make_summaries_state(&[("h1", &raw)])).unwrap();

        rename(&root, "h1", "Hello", "Hub Title").unwrap();

        let (title, preview): (String, String) = db.query_row("SELECT title, preview FROM conversation_summaries WHERE conversation_id = 'h1'", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        assert_eq!((title.as_str(), preview.as_str()), ("Hub Title", "Hub Title"));
        let ann = std::fs::read_to_string(root.join("annotations/h1.pbtxt")).unwrap();
        assert!(ann.contains("last_user_view_time:{seconds:123}"));
        assert_eq!(parse_pbtxt_title(&ann).as_deref(), Some("Hub Title"));
        let pb = std::fs::read(root.join("agyhub_summaries_proto.pb")).unwrap();
        assert!(pb.windows(b"Hub Title".len()).any(|w| w == b"Hub Title"));
    }
}
