//! PR marks, branch pushes, and incremental transcript log tracking for Antigravity sessions
//! (`engine/records.rs`, `prlinks.rs`).
//!
//! Antigravity writes tool calls (`PLANNER_RESPONSE`) and their outputs (`GENERIC`, or
//! `SYSTEM_MESSAGE` for background tasks) as separate lines in
//! `brain/<id>/.system_generated/logs/transcript_full.jsonl` (or `transcript.jsonl`). Because
//! `run_command` runs in a PTY, `gh pr create` prints its banner
//! (`Creating [draft ]pull request for <head> into <base> in <owner/name>`) followed by the new
//! PR's URL on the same result line, and `git push` prints `To <remote>` followed by its
//! ref-status table.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use serde_json::Value;

use crate::harness::NewLines;
use crate::model::{Harness, Session};
use crate::util::parse_rfc3339_ms;

use super::{read_conversations_in, transcript_file, Conversation, Folder};

/// Top-level conversations and nested subagents `(dir, id, parent_id)` from one read of each
/// folder's `conversation_summaries.db`.
pub(super) fn read_split(dirs: &[Folder], since_ms: i64, keep: &HashSet<String>) -> (Vec<Conversation>, Vec<(PathBuf, String, String)>) {
    let mut top = Vec::new();
    let mut subs = Vec::new();
    for f in dirs {
        for c in read_conversations_in(f, since_ms, keep, true) {
            if let Some(parent) = c.parent {
                subs.push((c.dir, c.id, parent));
            } else {
                top.push(c);
            }
        }
    }
    (top, subs)
}

/// Marks a transcript line as a tool's or background task's output so `engine/records.rs`
/// `worked_in` ignores passing mentions of orphan worktrees in file reads or command output.
pub(super) const OUTPUT_MARKS: &[&str] = &["\"type\":\"GENERIC\"", "\"type\":\"SYSTEM_MESSAGE\""];

/// Marks a command's explicit working directory in `PLANNER_RESPONSE` tool calls (object form in
/// `transcript_full.jsonl`, JSON-encoded string form in `transcript.jsonl`).
pub(super) const FOLDER_MARKS: &[&str] = &["\"Cwd\":\"", "\\\"Cwd\\\":\\\""];

/// Substrings checked before decoding a log line for Antigravity PR marks (`pr_made`) or branch
/// pushes (`pushed`).
pub(super) const PR_NEEDLES: &[&str] = &[
    "Creating pull request for ",
    "Creating draft pull request for ",
    "To github.com:",
    "To https://",
    "To ssh://",
    "To git@",
];

/// Fast pre-check before JSON decoding: only `GENERIC` and `SYSTEM_MESSAGE` command-result lines
/// can carry `run_command` output.
fn is_command_line(line: &str) -> bool {
    (line.contains("\"type\":\"GENERIC\"") || line.contains("\"type\":\"SYSTEM_MESSAGE\""))
        && (line.contains("Command output:\\n") || line.contains("Output:\\n"))
}

/// Extracts the command output section of an Antigravity `run_command` result (`GENERIC` for a
/// synchronous command, `SYSTEM_MESSAGE` for a background task's completion). Rejects `read_file`
/// results and unrelated system messages so file contents or logs printed in another tool never
/// count as commands this session ran.
fn command_output(v: &Value) -> Option<&str> {
    let kind = v.get("type").and_then(Value::as_str)?;
    let content = v.get("content").and_then(Value::as_str)?;
    let (header, output) = if let Some((h, o)) = content.split_once("\nCommand output:\n") {
        (h, o)
    } else if let Some((h, o)) = content.split_once("\nOutput:\n") {
        (h, o)
    } else {
        return None;
    };
    let exited = header.contains("Command exited with code ") || header.contains("The command exited with code ");
    if !exited {
        return None;
    }
    match kind {
        "GENERIC" => (content.starts_with("Created At: ") || content.starts_with("The tool's output was truncated")).then_some(output),
        "SYSTEM_MESSAGE" => (header.contains("finished with result:\nCommand exited with code ") || header.contains("finished with result:\nThe command exited with code ")).then_some(output),
        _ => None,
    }
}

fn created_at(v: &Value) -> i64 {
    v.get("created_at").and_then(Value::as_str).and_then(parse_rfc3339_ms).unwrap_or(0)
}

/// Strips ANSI CSI escape sequences (`\x1b[…<final byte>`) without allocating when none are
/// present.
fn strip_ansi(s: &str) -> Cow<'_, str> {
    if !s.contains('\x1b') {
        return Cow::Borrowed(s);
    }
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == 0x1b && bytes.get(i + 1) == Some(&b'[') {
            i += 2;
            while i < bytes.len() && !(0x40..=0x7e).contains(&bytes[i]) {
                i += 1;
            }
            if i < bytes.len() {
                i += 1;
            }
            continue;
        }
        let ch = s[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    Cow::Owned(out)
}

/// The terminal-visible tail of a PTY line: after the final carriage return (overwriting spinner
/// frames like `\r\r⣾\r…\rhttps://…`), with any ANSI escapes stripped.
fn terminal_line(line: &str) -> Cow<'_, str> {
    let raw = line.trim_end_matches('\r');
    let tail = raw.rsplit('\r').next().unwrap_or(raw);
    strip_ansi(tail)
}

/// `owner/name` from a `gh pr create` banner line (`Creating [draft ]pull request for <head> into
/// <base> in <owner/name>`).
fn create_banner_repo(line: &str) -> Option<&str> {
    let rest = line.strip_prefix("Creating pull request for ").or_else(|| line.strip_prefix("Creating draft pull request for "))?;
    let (_, after_into) = rest.split_once(" into ")?;
    let (_, repo) = after_into.rsplit_once(" in ")?;
    let repo = repo.trim();
    let (owner, name) = repo.split_once('/')?;
    (!owner.is_empty() && !name.is_empty() && !repo.contains(char::is_whitespace)).then_some(repo)
}

/// True for a line that is only whitespace or braille spinner characters (`⠀`–`⣿`), which `gh`
/// emits between its creation banner and the PR URL.
fn is_spinner_or_blank(s: &str) -> bool {
    s.chars().all(|c| c.is_whitespace() || ('\u{2800}'..='\u{28ff}').contains(&c))
}

/// The PR an Antigravity transcript line says the session created (`gh pr create` in `run_command`),
/// by its URL, with the line's timestamp.
pub(super) fn pr_made(line: &str) -> Option<(String, i64)> {
    if !(line.contains("Creating pull request for ") || line.contains("Creating draft pull request for ")) {
        return None;
    }
    if !line.contains("/pull/") || !is_command_line(line) {
        return None;
    }
    let v: Value = serde_json::from_str(line).ok()?;
    let out = command_output(&v)?;
    let at = created_at(&v);
    let lines: Vec<&str> = out.lines().collect();
    let mut found: Option<String> = None;
    for (i, raw) in lines.iter().enumerate() {
        let banner = terminal_line(raw);
        let Some(repo) = create_banner_repo(banner.trim()) else { continue };
        for next in lines.iter().skip(i + 1).take(16) {
            let vis = terminal_line(next);
            let seg = vis.trim().trim_start_matches(|c: char| ('\u{2800}'..='\u{28ff}').contains(&c)).trim();
            if is_spinner_or_blank(seg) {
                continue;
            }
            if create_banner_repo(seg).is_some() {
                break;
            }
            if (seg.starts_with("https://github.com/") || seg.starts_with("http://github.com/")) && !seg.contains(char::is_whitespace) {
                if let Some(key) = crate::prlinks::from_url(seg) {
                    if key.split_once('#').is_some_and(|(slug, _)| slug.eq_ignore_ascii_case(repo)) {
                        found = Some(seg.to_string());
                        break;
                    }
                }
            }
        }
    }
    Some((found?, at))
}

/// The remote branch an Antigravity transcript line says the session pushed to (`git push` in
/// `run_command`), with the line's timestamp. Parses git's `To <remote>` header followed by
/// `print_ref_status` lines, accepting fast-forward (`   `), forced (` + `), and new-branch
/// (` * [new branch]`) updates while ignoring rejected (` ! `), deleted (` - `), up-to-date
/// (` = `), and tag pushes (`[new tag]`).
pub(super) fn pushed(line: &str) -> Option<(String, i64)> {
    if !line.contains(" -> ") || !is_command_line(line) {
        return None;
    }
    if !(line.contains("To github.com:") || line.contains("To https://") || line.contains("To ssh://") || line.contains("To git@")) {
        return None;
    }
    let v: Value = serde_json::from_str(line).ok()?;
    let out = command_output(&v)?;
    let at = created_at(&v);
    let mut in_push = false;
    let mut found: Option<String> = None;
    for raw in out.lines() {
        let clean = terminal_line(raw);
        if let Some(remote) = clean.strip_prefix("To ") {
            let remote = remote.trim();
            in_push = !remote.is_empty() && !remote.contains(char::is_whitespace) && (remote.contains(':') || remote.contains('/'));
            continue;
        }
        if !in_push {
            continue;
        }
        if !clean.starts_with(' ') {
            in_push = false;
            continue;
        }
        let Some(rest) = clean.strip_prefix("   ").or_else(|| clean.strip_prefix(" + ")).or_else(|| clean.strip_prefix(" * ")) else {
            continue;
        };
        if clean.starts_with(" * ") && rest.trim_start().starts_with("[new tag]") {
            continue;
        }
        let Some((_, dst)) = rest.split_once(" -> ") else { continue };
        let dst = dst.trim();
        let dst = dst.strip_suffix(" (forced update)").unwrap_or(dst).trim();
        let dst = dst.strip_prefix("refs/heads/").unwrap_or(dst);
        if !dst.is_empty() && !dst.contains(char::is_whitespace) && !dst.contains('"') && !dst.contains('\'') {
            found = Some(dst.to_string());
        }
    }
    Some((found?, at))
}

/// Tracks Antigravity transcripts (top-level and subagents) and their last-read byte offsets so
/// `Reader::logs` lists every active transcript for `scan_pr_marks` and `search_visitors`, while
/// incremental rereads emit `Applied::found` on first appearance and `Applied::lines` as new steps
/// are appended.
#[derive(Default)]
pub(super) struct LogTracker {
    /// Account name → subagent `(transcript path, root parent session key)`.
    subagents: HashMap<String, Vec<(PathBuf, String)>>,
    /// Every tracked transcript's last-seen byte length.
    offsets: HashMap<PathBuf, u64>,
}

impl LogTracker {
    pub(super) fn clear(&mut self) {
        self.subagents.clear();
        self.offsets.clear();
    }

    /// Convenience wrapper for `Reader::reread_at`: advances offsets for `account`'s newly read
    /// conversations and subagents, prunes offsets no longer on the board (accounting for other
    /// accounts in `others`), and returns `(lines, found)`.
    pub(super) fn record_and_prune(
        &mut self,
        account: &str,
        convs: &[Conversation],
        nested: &[(PathBuf, String, String)],
        others: &HashMap<String, Vec<Conversation>>,
    ) -> (Vec<NewLines>, Vec<(PathBuf, String)>) {
        let (mut lines, mut found) = (Vec::new(), Vec::new());
        self.note_read(account, convs, nested, &mut lines, &mut found);
        let mut keep: HashSet<PathBuf> = convs.iter().filter_map(|c| transcript_file(&c.dir, &c.id)).collect();
        keep.extend(others.iter().filter(|(a, _)| a.as_str() != account).flat_map(|(_, rows)| rows.iter().filter_map(|c| transcript_file(&c.dir, &c.id))));
        keep.extend(self.subagents.values().flatten().map(|(p, _)| p.clone()));
        self.offsets.retain(|p, _| keep.contains(p));
        (lines, found)
    }

    /// Updates subagent mappings for `account`, advances byte offsets for all transcripts of
    /// `convs` and their subagents, and appends newly discovered files to `found` and appended
    /// byte ranges to `lines`.
    pub(super) fn note_read(
        &mut self,
        account: &str,
        convs: &[Conversation],
        nested: &[(PathBuf, String, String)],
        lines: &mut Vec<NewLines>,
        found: &mut Vec<(PathBuf, String)>,
    ) {
        let subs = resolve_subagents(account, convs, nested);
        let mut seen = HashSet::new();
        for c in convs {
            let Some(path) = transcript_file(&c.dir, &c.id) else { continue };
            if seen.insert(path.clone()) {
                let key = Session::key_for(Harness::Antigravity, account, &c.id);
                self.advance(path, key, lines, found);
            }
        }
        for (path, key) in &subs {
            if seen.insert(path.clone()) {
                self.advance(path.clone(), key.clone(), lines, found);
            }
        }
        self.subagents.insert(account.to_string(), subs);
    }

    fn advance(&mut self, path: PathBuf, key: String, lines: &mut Vec<NewLines>, found: &mut Vec<(PathBuf, String)>) {
        let Ok(len) = std::fs::metadata(&path).map(|m| m.len()) else { return };
        match self.offsets.get(&path).copied() {
            Some(from) if len > from => {
                lines.push(NewLines { path: path.clone(), from, to: len, key });
                self.offsets.insert(path, len);
            }
            Some(from) if len < from => {
                lines.push(NewLines { path: path.clone(), from: 0, to: len, key });
                self.offsets.insert(path, len);
            }
            Some(_) => {}
            None => {
                found.push((path.clone(), key));
                self.offsets.insert(path, len);
            }
        }
    }

    /// Every top-level and subagent transcript currently tracked, paired with its session key.
    pub(super) fn logs(&self, convs: &HashMap<String, Vec<Conversation>>) -> Vec<(PathBuf, String)> {
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        for (account, rows) in convs {
            for c in rows {
                let Some(path) = transcript_file(&c.dir, &c.id) else { continue };
                if seen.insert(path.clone()) {
                    out.push((path, Session::key_for(Harness::Antigravity, account, &c.id)));
                }
            }
            if let Some(subs) = self.subagents.get(account) {
                for (path, key) in subs {
                    if seen.insert(path.clone()) {
                        out.push((path.clone(), key.clone()));
                    }
                }
            }
        }
        out
    }

    /// True if any running/held conversation's transcript (or one of its subagents' transcripts)
    /// appeared or changed size since the last read, catching flushes that land right after the DB.
    pub(super) fn changed_for(&self, account: &str, active: &[&Conversation]) -> bool {
        let mut keys = HashSet::new();
        for c in active {
            let Some(path) = transcript_file(&c.dir, &c.id) else { continue };
            let len = std::fs::metadata(&path).map(|m| m.len()).ok();
            if len.is_some() && self.offsets.get(&path).copied() != len {
                return true;
            }
            keys.insert(Session::key_for(Harness::Antigravity, account, &c.id));
        }
        if let Some(subs) = self.subagents.get(account) {
            for (path, key) in subs {
                if keys.contains(key) {
                    let len = std::fs::metadata(path).map(|m| m.len()).ok();
                    if len.is_some() && self.offsets.get(path).copied() != len {
                        return true;
                    }
                }
            }
        }
        false
    }
}

/// Maps nested subagent conversations `(dir, id, parent_id)` up to their root top-level session's
/// key on `account`.
fn resolve_subagents(account: &str, convs: &[Conversation], nested: &[(PathBuf, String, String)]) -> Vec<(PathBuf, String)> {
    let top: HashSet<&str> = convs.iter().map(|c| c.id.as_str()).collect();
    let parent_of: HashMap<&str, &str> = nested.iter().map(|(_, id, parent)| (id.as_str(), parent.as_str())).collect();
    let mut out = Vec::new();
    for (dir, id, parent) in nested {
        let mut cur = parent.as_str();
        let mut hops = 0;
        let root = loop {
            if top.contains(cur) {
                break Some(cur);
            }
            hops += 1;
            if hops > 16 {
                break None;
            }
            match parent_of.get(cur) {
                Some(&next) if next != cur => cur = next,
                _ => break None,
            }
        };
        if let Some(root) = root {
            if let Some(path) = transcript_file(dir, id) {
                out.push((path, Session::key_for(Harness::Antigravity, account, root)));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn generic(ts: &str, content: &str) -> String {
        json!({"step_index": 60, "source": "MODEL", "type": "GENERIC", "status": "DONE", "created_at": ts, "content": content}).to_string()
    }

    fn system_msg(ts: &str, content: &str) -> String {
        json!({"step_index": 141, "source": "SYSTEM", "type": "SYSTEM_MESSAGE", "status": "DONE", "created_at": ts, "content": content}).to_string()
    }

    #[test]
    fn finds_prs_from_synchronous_draft_and_background_commands() {
        // Real PTY output with git push + spinner frames + gh pr create.
        let sync = generic(
            "1970-01-01T00:00:05Z",
            "Created At: 2026-11-27T15:25:49-08:00\nCompleted At: 2026-11-27T15:26:00-08:00\nCommand exited with code 0.\n\nCommand output:\nTo github.com:alice/app.git\r\n * [new branch]          65d3cbc54 -> fix-readme-env-pull-ref\r\n\r\r⣾\r\r⣽\r\nCreating pull request for alice:fix-readme-env-pull-ref into main in acme/app\r\n\r\n\r\r⣾\r\r⣽\r\r⣻\rhttps://github.com/acme/app/pull/1616\r\n## app...alice/app\r\n",
        );
        assert_eq!(pr_made(&sync), Some(("https://github.com/acme/app/pull/1616".to_string(), 5000)));
        assert_eq!(pushed(&sync), Some(("fix-readme-env-pull-ref".to_string(), 5000)));

        // Draft PR with ANSI color codes.
        let draft = generic(
            "1970-01-01T00:00:06Z",
            "Created At: 2026-11-27T15:25:49-08:00\nCommand exited with code 0.\n\nCommand output:\n\x1b[1mCreating draft pull request for feat into main in Org/Repo\x1b[0m\r\n\r\n\x1b[32mhttps://github.com/org/repo/pull/42\x1b[0m\r\n",
        );
        assert_eq!(pr_made(&draft), Some(("https://github.com/org/repo/pull/42".to_string(), 6000)));

        // Background task completion (SYSTEM_MESSAGE).
        let bg = system_msg(
            "1970-01-01T00:00:07Z",
            "<SYSTEM_MESSAGE>\n[Message] sender=abc/task-1 content=Task id \"abc/task-1\" finished with result:\nCommand exited with code 0.\n\nCommand output:\nTo github.com:o/r.git\r\n * [new branch]        feat -> feat\r\nCreating pull request for feat into main in o/r\r\n\r\n\r\r⣾\rhttps://github.com/o/r/pull/9\r\n",
        );
        assert_eq!(pr_made(&bg), Some(("https://github.com/o/r/pull/9".to_string(), 7000)));
        assert_eq!(pushed(&bg), Some(("feat".to_string(), 7000)));

        // Older agy format (`The command exited with code 0.\nOutput:\n`).
        let older = generic(
            "1970-01-01T00:00:08Z",
            "Created At: 2026-10-14T20:29:09-07:00\n\nThe command exited with code 0.\nOutput:\nTo https://github.com/o/r.git\r\n   f9b6ed09..135355fd  HEAD -> main\r\n",
        );
        assert_eq!(pushed(&older), Some(("main".to_string(), 8000)));
    }

    #[test]
    fn ignores_read_file_json_dumps_pr_views_and_failed_pushes() {
        // read_file output reading a log that mentions gh pr create and git push.
        let read_file = generic(
            "1970-01-01T00:00:01Z",
            "Created At: 2026-11-27T15:25:49-08:00\nShowing lines 1-42 of 42\n36: To github.com:o/r.git\n37:  * [new branch] feat -> feat\n39: Creating pull request for feat into main in o/r\n41: https://github.com/o/r/pull/325\n",
        );
        assert_eq!(pr_made(&read_file), None);
        assert_eq!(pushed(&read_file), None);

        // Command output printing another session's JSONL transcript (escaped \r\n inside JSON).
        let json_dump = generic(
            "1970-01-01T00:00:02Z",
            r#"Created At: 2026-11-27T15:25:49-08:00
Command exited with code 0.

Command output:
{"type":"GENERIC","content":"To github.com:o/r.git\r\n * [new branch] a -> feat\r\nCreating pull request for feat into main in o/r\r\nhttps://github.com/o/r/pull/1616\r\n"}
"#,
        );
        assert_eq!(pr_made(&json_dump), None);
        assert_eq!(pushed(&json_dump), None);

        // gh pr view prints a URL without a creation banner.
        let view = generic(
            "1970-01-01T00:00:03Z",
            "Created At: 2026-11-27T15:25:49-08:00\nCommand exited with code 0.\n\nCommand output:\nhttps://github.com/o/r/pull/58\r\n",
        );
        assert_eq!(pr_made(&view), None);

        // Banner whose repo doesn't match the printed URL.
        let mismatch = generic(
            "1970-01-01T00:00:04Z",
            "Created At: 2026-11-27T15:25:49-08:00\nCommand exited with code 1.\n\nCommand output:\nCreating pull request for feat into main in o/r\r\nSee https://github.com/other/repo/pull/1\r\n",
        );
        assert_eq!(pr_made(&mismatch), None);

        // Rejected push, deleted branch, and new tag don't count as pushed branches.
        let non_pushes = generic(
            "1970-01-01T00:00:05Z",
            "Created At: 2026-11-27T15:25:49-08:00\nCommand exited with code 1.\n\nCommand output:\nTo https://github.com/o/r.git\r\n ! [remote rejected]   feat -> feat (refused)\r\n - [deleted]           old -> old\r\n * [new tag]           v1.0 -> v1.0\r\n",
        );
        assert_eq!(pushed(&non_pushes), None);

        // Force push strips `(forced update)` and `refs/heads/`.
        let forced = generic(
            "1970-01-01T00:00:06Z",
            "Created At: 2026-11-27T15:25:49-08:00\nCommand exited with code 0.\n\nCommand output:\nTo ssh://git@github.com/o/r.git\r\n + abc1234...def5678  local -> refs/heads/topic (forced update)\r\n",
        );
        assert_eq!(pushed(&forced), Some(("topic".to_string(), 6000)));
    }

    #[test]
    fn tracker_lists_subagent_logs_and_emits_found_then_new_lines() {
        let dir = crate::fixtures::scratch("agy-log-tracker");
        let write_log = |id: &str, body: &str| {
            let p = dir.join("brain").join(id).join(".system_generated/logs/transcript_full.jsonl");
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, body).unwrap();
            p
        };
        let parent_log = write_log("p1", "{\"type\":\"USER_INPUT\"}\n");
        let sub_log = write_log("s1", "{\"type\":\"USER_INPUT\"}\n");
        let nested_sub = write_log("s2", "{\"type\":\"USER_INPUT\"}\n");

        let convs = vec![Conversation { id: "p1".into(), dir: dir.clone(), ..Default::default() }];
        let nested = vec![
            (dir.clone(), "s1".into(), "p1".into()),
            (dir.clone(), "s2".into(), "s1".into()),
            (dir.clone(), "orphan".into(), "missing".into()),
        ];

        let mut tracker = LogTracker::default();
        let (mut lines, mut found) = (Vec::new(), Vec::new());
        tracker.note_read("antigravity", &convs, &nested, &mut lines, &mut found);
        assert!(lines.is_empty());
        assert_eq!(found.len(), 3);
        let key = "antigravity:antigravity:p1";
        assert!(found.contains(&(parent_log.clone(), key.to_string())));
        assert!(found.contains(&(sub_log.clone(), key.to_string())));
        assert!(found.contains(&(nested_sub.clone(), key.to_string())));

        let map = HashMap::from([("antigravity".to_string(), convs.clone())]);
        let mut all = tracker.logs(&map);
        all.sort();
        let mut expected = vec![(parent_log.clone(), key.to_string()), (sub_log.clone(), key.to_string()), (nested_sub.clone(), key.to_string())];
        expected.sort();
        assert_eq!(all, expected);

        // No growth yet: changed_for is false and reread emits nothing.
        let active: Vec<&Conversation> = convs.iter().collect();
        assert!(!tracker.changed_for("antigravity", &active));
        lines.clear();
        found.clear();
        tracker.note_read("antigravity", &convs, &nested, &mut lines, &mut found);
        assert!(lines.is_empty() && found.is_empty());

        // Appending to the parent or subagent transcript triggers changed_for and emits NewLines.
        let before = std::fs::metadata(&parent_log).unwrap().len();
        use std::io::Write;
        writeln!(std::fs::OpenOptions::new().append(true).open(&parent_log).unwrap(), "{{\"type\":\"GENERIC\"}}").unwrap();
        assert!(tracker.changed_for("antigravity", &active));
        tracker.note_read("antigravity", &convs, &nested, &mut lines, &mut found);
        assert!(found.is_empty());
        assert_eq!(lines.len(), 1);
        assert_eq!((&lines[0].path, lines[0].from, lines[0].to, lines[0].key.as_str()), (&parent_log, before, std::fs::metadata(&parent_log).unwrap().len(), key));
    }
}
