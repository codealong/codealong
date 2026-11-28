//! PR marks, branch pushes, and incremental transcript log tracking for Antigravity sessions
//! (`engine/records.rs`, `prlinks.rs`).
//!
//! Antigravity writes tool calls (`PLANNER_RESPONSE`) and their outputs (`GENERIC`, or
//! `SYSTEM_MESSAGE` for background tasks) as separate lines in
//! `brain/<id>/.system_generated/logs/transcript_full.jsonl` (or `transcript.jsonl`). Because a
//! command's stdout can print another session's records (for example while debugging transcripts),
//! each candidate `GENERIC` or `SYSTEM_MESSAGE` output line is paired back to its originating
//! `PLANNER_RESPONSE` `run_command` step and counted only when that shell command actually ran
//! `gh pr create` or `git push`.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::io::BufRead;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::harness::NewLines;
use crate::model::{Harness, Session};
use crate::prlinks::Source;
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

/// Substrings checked before decoding a log line for Antigravity PR marks (`pr_marks_on`) or
/// branch pushes (`pushes_on`).
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

/// The step index of the `run_command` tool result (`PLANNER_RESPONSE.step_index + 1 + call_idx`):
/// `GENERIC` carries it as its own `step_index`, and a background task's `SYSTEM_MESSAGE` names it
/// as `Task id "<conv>/task-<step>" finished with result:`.
fn result_step(v: &Value) -> Option<i64> {
    match v.get("type").and_then(Value::as_str)? {
        "GENERIC" => v.get("step_index").and_then(Value::as_i64),
        "SYSTEM_MESSAGE" => {
            let content = v.get("content").and_then(Value::as_str)?;
            let before = content.split_once("\" finished with result:\n")?.0;
            let num = before.rsplit_once("/task-")?.1;
            num.parse::<i64>().ok()
        }
        _ => None,
    }
}

/// Extracts `CommandLine` if `line` is the `PLANNER_RESPONSE` whose `run_command` call produced
/// `target_step` (`step_index + 1 + idx == target_step`).
fn planner_command(line: &str, target_step: i64) -> Option<String> {
    if !line.contains("\"PLANNER_RESPONSE\"") || !line.contains("\"run_command\"") {
        return None;
    }
    let v: Value = serde_json::from_str(line).ok()?;
    if v.get("type").and_then(Value::as_str) != Some("PLANNER_RESPONSE") {
        return None;
    }
    let step = v.get("step_index").and_then(Value::as_i64)?;
    let calls = v.get("tool_calls").and_then(Value::as_array)?;
    if target_step <= step || target_step > step + calls.len() as i64 {
        return None;
    }
    let call = &calls[(target_step - step - 1) as usize];
    if call.get("name").and_then(Value::as_str) != Some("run_command") {
        return None;
    }
    super::arg(call.get("args")?, "CommandLine")
}

/// Looks up the `CommandLine` of the `PLANNER_RESPONSE` `run_command` call paired with
/// `target_step` in `source` (handles out-of-order steps and background task completions whose
/// call sits earlier in the transcript).
fn command_for_step(source: &Source<'_>, target_step: i64) -> Option<String> {
    match source {
        Source::Text(text) => text.lines().find_map(|l| planner_command(l, target_step)),
        Source::File(path) => {
            let file = std::fs::File::open(path).ok()?;
            let mut reader = std::io::BufReader::with_capacity(1 << 20, file);
            let finder = memchr::memmem::Finder::new(b"\"run_command\"");
            let mut buf = Vec::new();
            loop {
                buf.clear();
                match reader.read_until(b'\n', &mut buf) {
                    Ok(0) | Err(_) => return None,
                    Ok(_) => {}
                }
                if finder.find(&buf).is_some() {
                    let line = String::from_utf8_lossy(&buf);
                    if let Some(cmd) = planner_command(line.trim_end(), target_step) {
                        return Some(cmd);
                    }
                }
            }
        }
    }
}

/// Splits a shell command string into simple commands (lists of argv words), respecting single
/// quotes, double quotes with backslash escapes, unquoted backslash escapes, `#` comments,
/// heredoc bodies (`<<` / `<<-`), command separators (`&&`, `||`, `;`, `|`, `&`, `\n`, `(`, `)`,
/// `$(`, standalone `{`/`}`), and redirections.
fn shell_commands(cmd: &str) -> Vec<Vec<String>> {
    let chars: Vec<char> = cmd.chars().collect();
    let mut cmds: Vec<Vec<String>> = Vec::new();
    let mut cur_cmd: Vec<String> = Vec::new();
    let mut cur_word = String::new();
    let mut in_word = false;
    let mut heredocs: Vec<(String, bool)> = Vec::new();

    let finish_word = |in_word: &mut bool, cur_word: &mut String, cur_cmd: &mut Vec<String>| {
        if *in_word {
            cur_cmd.push(std::mem::take(cur_word));
            *in_word = false;
        }
    };
    let finish_cmd = |in_word: &mut bool, cur_word: &mut String, cur_cmd: &mut Vec<String>, cmds: &mut Vec<Vec<String>>| {
        finish_word(in_word, cur_word, cur_cmd);
        if !cur_cmd.is_empty() {
            cmds.push(std::mem::take(cur_cmd));
        }
    };

    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '\n' => {
                finish_cmd(&mut in_word, &mut cur_word, &mut cur_cmd, &mut cmds);
                i += 1;
                // Skip any heredoc bodies queued on the line that just ended.
                for (delim, strip_tabs) in heredocs.drain(..) {
                    while i < chars.len() {
                        let start = i;
                        while i < chars.len() && chars[i] != '\n' {
                            i += 1;
                        }
                        let mut line: String = chars[start..i].iter().collect();
                        if line.ends_with('\r') {
                            line.pop();
                        }
                        if i < chars.len() && chars[i] == '\n' {
                            i += 1;
                        }
                        let matched = if strip_tabs { line.trim_start_matches('\t') == delim } else { line == delim };
                        if matched {
                            break;
                        }
                    }
                }
            }
            ' ' | '\t' | '\r' => {
                finish_word(&mut in_word, &mut cur_word, &mut cur_cmd);
                i += 1;
            }
            '#' if !in_word => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
            }
            '\'' => {
                in_word = true;
                i += 1;
                while i < chars.len() && chars[i] != '\'' {
                    cur_word.push(chars[i]);
                    i += 1;
                }
                if i < chars.len() {
                    i += 1;
                }
            }
            '"' => {
                in_word = true;
                i += 1;
                while i < chars.len() && chars[i] != '"' {
                    if chars[i] == '\\' && i + 1 < chars.len() {
                        let next = chars[i + 1];
                        if matches!(next, '"' | '\\' | '$' | '`' | '\n') {
                            if next != '\n' {
                                cur_word.push(next);
                            }
                            i += 2;
                            continue;
                        }
                    }
                    cur_word.push(chars[i]);
                    i += 1;
                }
                if i < chars.len() {
                    i += 1;
                }
            }
            '\\' => {
                if i + 1 < chars.len() && chars[i + 1] == '\n' {
                    i += 2;
                } else if i + 1 < chars.len() {
                    in_word = true;
                    cur_word.push(chars[i + 1]);
                    i += 2;
                } else {
                    i += 1;
                }
            }
            '$' if chars.get(i + 1) == Some(&'(') => {
                // Unquoted command substitution `$(...)` runs the inner command(s).
                finish_cmd(&mut in_word, &mut cur_word, &mut cur_cmd, &mut cmds);
                i += 2;
            }
            ';' | '(' | ')' => {
                finish_cmd(&mut in_word, &mut cur_word, &mut cur_cmd, &mut cmds);
                i += 1;
            }
            '{' | '}' if !in_word => {
                finish_cmd(&mut in_word, &mut cur_word, &mut cur_cmd, &mut cmds);
                i += 1;
            }
            '&' | '|' => {
                finish_cmd(&mut in_word, &mut cur_word, &mut cur_cmd, &mut cmds);
                i += if chars.get(i + 1) == Some(&c) { 2 } else { 1 };
            }
            '<' if chars.get(i + 1) == Some(&'<') && chars.get(i + 2) != Some(&'<') => {
                finish_word(&mut in_word, &mut cur_word, &mut cur_cmd);
                i += 2;
                let strip_tabs = chars.get(i) == Some(&'-');
                if strip_tabs {
                    i += 1;
                }
                while i < chars.len() && matches!(chars[i], ' ' | '\t') {
                    i += 1;
                }
                let mut delim = String::new();
                while i < chars.len() && !matches!(chars[i], ' ' | '\t' | '\n' | '\r' | ';' | '&' | '|' | '(' | ')') {
                    match chars[i] {
                        '\'' | '"' => i += 1,
                        '\\' if i + 1 < chars.len() => {
                            delim.push(chars[i + 1]);
                            i += 2;
                        }
                        ch => {
                            delim.push(ch);
                            i += 1;
                        }
                    }
                }
                if !delim.is_empty() {
                    heredocs.push((delim, strip_tabs));
                }
            }
            '>' | '<' => {
                // Drop numeric fd prefix glued to redirection (`2>/dev/null`) rather than keeping
                // it as an argument word.
                if in_word && cur_word.chars().all(|ch| ch.is_ascii_digit()) {
                    cur_word.clear();
                    in_word = false;
                } else {
                    finish_word(&mut in_word, &mut cur_word, &mut cur_cmd);
                }
                i += 1;
                if matches!(chars.get(i), Some('>' | '<' | '&')) {
                    i += 1;
                    // `>&1` / `>&2` / `>&-`: skip the target fd too.
                    while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '-') {
                        i += 1;
                    }
                } else {
                    // Skip the redirected path token so it isn't mistaken for a command/subcommand.
                    while i < chars.len() && matches!(chars[i], ' ' | '\t') {
                        i += 1;
                    }
                    while i < chars.len() && !matches!(chars[i], ' ' | '\t' | '\n' | '\r' | ';' | '&' | '|' | '(' | ')') {
                        if chars[i] == '\'' {
                            i += 1;
                            while i < chars.len() && chars[i] != '\'' {
                                i += 1;
                            }
                            if i < chars.len() {
                                i += 1;
                            }
                        } else if chars[i] == '"' {
                            i += 1;
                            while i < chars.len() && chars[i] != '"' {
                                i += if chars[i] == '\\' && i + 1 < chars.len() { 2 } else { 1 };
                            }
                            if i < chars.len() {
                                i += 1;
                            }
                        } else {
                            i += 1;
                        }
                    }
                }
            }
            _ => {
                in_word = true;
                cur_word.push(c);
                i += 1;
            }
        }
    }
    finish_cmd(&mut in_word, &mut cur_word, &mut cur_cmd, &mut cmds);
    cmds
}

fn is_env_assignment(word: &str) -> bool {
    let Some((name, _)) = word.split_once('=') else { return false };
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_') && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Skips leading environment assignments and transparent command wrappers (`env`, `command`,
/// `time`, `noglob`, `nohup`, `nice`, `exec`), returning the invoked program's basename and its
/// argument slice.
fn program_and_args(words: &[String]) -> Option<(&str, &[String])> {
    let mut i = 0;
    while i < words.len() {
        let w = words[i].as_str();
        if is_env_assignment(w) {
            i += 1;
            continue;
        }
        let base = Path::new(w).file_name().and_then(|s| s.to_str()).unwrap_or(w);
        match base {
            "command" | "time" | "noglob" | "nohup" | "exec" => {
                i += 1;
                while i < words.len() && words[i].starts_with('-') {
                    i += 1;
                }
            }
            "env" => {
                i += 1;
                while i < words.len() {
                    if words[i] == "-u" || words[i] == "--unset" || words[i] == "-C" || words[i] == "--chdir" {
                        i += 2;
                    } else if words[i].starts_with('-') || is_env_assignment(&words[i]) {
                        i += 1;
                    } else {
                        break;
                    }
                }
            }
            "nice" => {
                i += 1;
                while i < words.len() {
                    if words[i] == "-n" {
                        i += 2;
                    } else if words[i].starts_with('-') {
                        i += 1;
                    } else {
                        break;
                    }
                }
            }
            _ => return Some((base, &words[i + 1..])),
        }
    }
    None
}

/// True when `cmd` actually executes `gh pr create` as a shell command (not merely mentioning it in
/// a script argument, commit message, heredoc, or search pattern).
fn runs_gh_pr_create(cmd: &str) -> bool {
    shell_commands(cmd).iter().any(|words| {
        let Some(("gh", args)) = program_and_args(words) else { return false };
        if args.first().map(String::as_str) != Some("pr") {
            return false;
        }
        let mut i = 1;
        while i < args.len() {
            let a = args[i].as_str();
            if matches!(a, "-R" | "--repo") {
                i += 2;
            } else if a.starts_with('-') {
                i += 1;
            } else {
                break;
            }
        }
        if args.get(i).map(String::as_str) != Some("create") {
            return false;
        }
        !args.iter().any(|a| a == "-h" || a == "--help")
    })
}

/// True when `cmd` actually executes `git push` (not `--dry-run`, `--delete`, `--help`, or a
/// mention inside another command's arguments).
fn runs_git_push(cmd: &str) -> bool {
    shell_commands(cmd).iter().any(|words| {
        let Some(("git", args)) = program_and_args(words) else { return false };
        let mut i = 0;
        while i < args.len() {
            let a = args[i].as_str();
            if matches!(a, "-C" | "-c" | "--git-dir" | "--work-tree" | "--namespace" | "--super-prefix" | "--config-env" | "--exec-path") {
                i += 2;
            } else if a.starts_with('-') {
                i += 1;
            } else {
                break;
            }
        }
        if args.get(i).map(String::as_str) != Some("push") {
            return false;
        }
        let rest = &args[i + 1..];
        !rest.iter().any(|a| matches!(a.as_str(), "-n" | "--dry-run" | "-h" | "--help" | "-d" | "--delete"))
    })
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

/// All PRs an Antigravity transcript line says the session created (`gh pr create` in
/// `run_command`), by URL and timestamp, verified against the paired `PLANNER_RESPONSE` command in
/// `source`.
pub(super) fn pr_marks_on(line: &str, source: &Source<'_>) -> Vec<(String, i64)> {
    if !(line.contains("Creating pull request for ") || line.contains("Creating draft pull request for ")) {
        return Vec::new();
    }
    if !line.contains("/pull/") || !is_command_line(line) {
        return Vec::new();
    }
    let Ok(v) = serde_json::from_str::<Value>(line) else { return Vec::new() };
    let Some(out) = command_output(&v) else { return Vec::new() };
    let at = created_at(&v);
    let lines: Vec<&str> = out.lines().collect();
    let mut found: Vec<(String, i64)> = Vec::new();
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
                        if !found.iter().any(|(u, _)| u == seg) {
                            found.push((seg.to_string(), at));
                        }
                        break;
                    }
                }
            }
        }
    }
    if found.is_empty() {
        return found;
    }
    let Some(step) = result_step(&v) else { return Vec::new() };
    let Some(cmd) = command_for_step(source, step) else { return Vec::new() };
    if runs_gh_pr_create(&cmd) { found } else { Vec::new() }
}

/// All remote branches an Antigravity transcript line says the session pushed to (`git push` in
/// `run_command`), with timestamp, verified against the paired `PLANNER_RESPONSE` command in
/// `source`.
pub(super) fn pushes_on(line: &str, source: &Source<'_>) -> Vec<(String, i64)> {
    if !line.contains(" -> ") || !is_command_line(line) {
        return Vec::new();
    }
    if !(line.contains("To github.com:") || line.contains("To https://") || line.contains("To ssh://") || line.contains("To git@")) {
        return Vec::new();
    }
    let Ok(v) = serde_json::from_str::<Value>(line) else { return Vec::new() };
    let Some(out) = command_output(&v) else { return Vec::new() };
    let at = created_at(&v);
    let mut in_push = false;
    let mut found: Vec<(String, i64)> = Vec::new();
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
            found.push((dst.to_string(), at));
        }
    }
    if found.is_empty() {
        return found;
    }
    let Some(step) = result_step(&v) else { return Vec::new() };
    let Some(cmd) = command_for_step(source, step) else { return Vec::new() };
    if runs_git_push(&cmd) { found } else { Vec::new() }
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

    fn planner(step: i64, cmd: &str) -> String {
        json!({
            "step_index": step,
            "source": "MODEL",
            "type": "PLANNER_RESPONSE",
            "status": "DONE",
            "created_at": "1970-01-01T00:00:01Z",
            "tool_calls": [{"name": "run_command", "args": {"CommandLine": cmd}}]
        })
        .to_string()
    }

    fn generic(step: i64, ts: &str, content: &str) -> String {
        json!({"step_index": step, "source": "MODEL", "type": "GENERIC", "status": "DONE", "created_at": ts, "content": content}).to_string()
    }

    fn system_msg(step: i64, ts: &str, content: &str) -> String {
        json!({"step_index": step, "source": "SYSTEM", "type": "SYSTEM_MESSAGE", "status": "DONE", "created_at": ts, "content": content}).to_string()
    }

    #[test]
    fn finds_prs_and_pushes_from_verified_commands_including_background_and_multi_pr() {
        // Real PTY output with git push + spinner frames + gh pr create.
        let p_sync = planner(
            59,
            "git push alice 65d3cbc54:refs/heads/fix-readme-env-pull-ref && gh pr create --repo acme/app --base main --head alice:fix-readme-env-pull-ref --title \"Fix\" --body \"$(cat <<'EOF'\nSummary\nEOF\n)\"",
        );
        let sync = generic(
            60,
            "1970-01-01T00:00:05Z",
            "Created At: 2026-11-27T15:25:49-08:00\nCompleted At: 2026-11-27T15:26:00-08:00\nCommand exited with code 0.\n\nCommand output:\nTo github.com:alice/app.git\r\n * [new branch]          65d3cbc54 -> fix-readme-env-pull-ref\r\n\r\r⣾\r\r⣽\r\nCreating pull request for alice:fix-readme-env-pull-ref into main in acme/app\r\n\r\n\r\r⣾\r\r⣽\r\r⣻\rhttps://github.com/acme/app/pull/1616\r\n## app...alice/app\r\n",
        );
        let text = format!("{p_sync}\n{sync}");
        let src = Source::Text(&text);
        assert_eq!(pr_marks_on(&sync, &src), vec![("https://github.com/acme/app/pull/1616".to_string(), 5000)]);
        assert_eq!(pushes_on(&sync, &src), vec![("fix-readme-env-pull-ref".to_string(), 5000)]);

        // Draft PR with ANSI color codes, written out of order (GENERIC before PLANNER_RESPONSE).
        let p_draft = planner(10, "gh pr create --draft --title x");
        let draft = generic(
            11,
            "1970-01-01T00:00:06Z",
            "Created At: 2026-11-27T15:25:49-08:00\nCommand exited with code 0.\n\nCommand output:\n\x1b[1mCreating draft pull request for feat into main in Org/Repo\x1b[0m\r\n\r\n\x1b[32mhttps://github.com/org/repo/pull/42\x1b[0m\r\n",
        );
        let out_of_order = format!("{draft}\n{p_draft}");
        assert_eq!(pr_marks_on(&draft, &Source::Text(&out_of_order)), vec![("https://github.com/org/repo/pull/42".to_string(), 6000)]);

        // Background task completion (SYSTEM_MESSAGE) opening two PRs in one chained command.
        let p_bg = planner(
            130,
            "(cd ~/work/repo-a && git push -u origin topic-a && gh pr create -R acme/repo-a --title a) && (cd ~/work/repo-b && git push -u origin topic-b && gh pr create -R acme/repo-b --title b)",
        );
        let bg = system_msg(
            141,
            "1970-01-01T00:00:07Z",
            "<SYSTEM_MESSAGE>\n[Message] sender=abc/task-131 content=Task id \"abc/task-131\" finished with result:\nCommand exited with code 0.\n\nCommand output:\nTo github.com:acme/repo-a.git\r\n * [new branch]        topic-a -> topic-a\r\nCreating pull request for topic-a into main in acme/repo-a\r\n\r\n\r\r⣾\rhttps://github.com/acme/repo-a/pull/446\r\n---\r\nTo github.com:acme/repo-b.git\r\n * [new branch]        topic-b -> topic-b\r\nCreating pull request for topic-b into main in acme/repo-b\r\n\r\n\r\r⣾\rhttps://github.com/acme/repo-b/pull/325\r\n",
        );
        let bg_text = format!("{p_bg}\n{bg}");
        let bg_src = Source::Text(&bg_text);
        assert_eq!(
            pr_marks_on(&bg, &bg_src),
            vec![
                ("https://github.com/acme/repo-a/pull/446".to_string(), 7000),
                ("https://github.com/acme/repo-b/pull/325".to_string(), 7000),
            ]
        );
        assert_eq!(pushes_on(&bg, &bg_src), vec![("topic-a".to_string(), 7000), ("topic-b".to_string(), 7000)]);

        // Older agy format (`The command exited with code 0.\nOutput:\n`) and `git -C` + `Source::File`.
        let p_older = planner(20, "git -C /tmp/work push origin HEAD:main");
        let older = generic(
            21,
            "1970-01-01T00:00:08Z",
            "Created At: 2026-10-14T20:29:09-07:00\n\nThe command exited with code 0.\nOutput:\nTo https://github.com/acme/repo.git\r\n   f9b6ed09..135355fd  HEAD -> main\r\n",
        );
        let dir = crate::fixtures::scratch("agy-pr-file-source");
        let file = dir.join("transcript_full.jsonl");
        std::fs::write(&file, format!("{p_older}\n{older}\n")).unwrap();
        assert_eq!(pushes_on(&older, &Source::File(&file)), vec![("main".to_string(), 8000)]);
    }

    #[test]
    fn ignores_printed_records_from_debugging_commands_and_non_push_or_create_commands() {
        // A debugging script (even one whose Python source mentions `gh pr create` and `git push`)
        // printing another session's real push and PR creation output must NOT be attributed here.
        let printed_out = "Created At: 2026-11-27T16:05:41-08:00\nCompleted At: 2026-11-27T16:05:42-08:00\nCommand exited with code 0.\n\nCommand output:\n=== Line 142 step=141 source=SYSTEM type=SYSTEM_MESSAGE ===\nTo github.com:acme/infra.git\r\n * [new branch]        session-rules -> session-rules\r\nCreating pull request for session-rules into main in acme/infra\r\n\r\n\r\r⣾\rhttps://github.com/acme/infra/pull/446\r\n";

        for (i, debug_cmd) in [
            "python3 -c 'import json\nfor l in open(\"other.jsonl\"):\n  if \"gh pr create\" in l or \"git push\" in l: print(json.loads(l)[\"content\"])'",
            "python3 - <<'EOF'\nprint('git push && gh pr create')\nEOF",
            "cat /tmp/other-session.log",
            "rg -n \"Creating pull request for|git push\" /tmp/logs",
            "git commit -m \"Only count git push and gh pr create commands\"",
            "git push --dry-run origin session-rules && gh pr view 446",
        ]
        .into_iter()
        .enumerate()
        {
            let step = (i as i64 + 1) * 10;
            let p = planner(step, debug_cmd);
            let g = generic(step + 1, "1970-01-01T00:00:09Z", printed_out);
            let text = format!("{p}\n{g}");
            let src = Source::Text(&text);
            assert!(pr_marks_on(&g, &src).is_empty(), "false PR positive for: {debug_cmd}");
            assert!(pushes_on(&g, &src).is_empty(), "false push positive for: {debug_cmd}");
        }

        // Output with no paired PLANNER_RESPONSE (or paired with a non-run_command tool) is ignored.
        let orphan = generic(99, "1970-01-01T00:00:09Z", printed_out);
        assert!(pr_marks_on(&orphan, &Source::Text(&orphan)).is_empty());
        assert!(pushes_on(&orphan, &Source::Text(&orphan)).is_empty());

        // read_file output reading a log that mentions gh pr create and git push.
        let read_file = generic(
            2,
            "1970-01-01T00:00:01Z",
            "Created At: 2026-11-27T15:25:49-08:00\nShowing lines 1-42 of 42\n36: To github.com:o/r.git\n37:  * [new branch] feat -> feat\n39: Creating pull request for feat into main in o/r\n41: https://github.com/o/r/pull/325\n",
        );
        let p_rf = planner(1, "git push origin feat && gh pr create");
        let rf_text = format!("{p_rf}\n{read_file}");
        assert!(pr_marks_on(&read_file, &Source::Text(&rf_text)).is_empty());
        assert!(pushes_on(&read_file, &Source::Text(&rf_text)).is_empty());

        // Rejected push, deleted branch, and new tag don't count even from a real `git push`.
        let p_push = planner(40, "git push origin feat");
        let non_pushes = generic(
            41,
            "1970-01-01T00:00:05Z",
            "Created At: 2026-11-27T15:25:49-08:00\nCommand exited with code 1.\n\nCommand output:\nTo https://github.com/o/r.git\r\n ! [remote rejected]   feat -> feat (refused)\r\n - [deleted]           old -> old\r\n * [new tag]           v1.0 -> v1.0\r\n",
        );
        let np_text = format!("{p_push}\n{non_pushes}");
        assert!(pushes_on(&non_pushes, &Source::Text(&np_text)).is_empty());

        // Force push strips `(forced update)` and `refs/heads/`.
        let p_force = planner(50, "FOO=1 /usr/bin/git push --force-with-lease origin local:refs/heads/topic");
        let forced = generic(
            51,
            "1970-01-01T00:00:06Z",
            "Created At: 2026-11-27T15:25:49-08:00\nCommand exited with code 0.\n\nCommand output:\nTo ssh://git@github.com/o/r.git\r\n + abc1234...def5678  local -> refs/heads/topic (forced update)\r\n",
        );
        let f_text = format!("{p_force}\n{forced}");
        assert_eq!(pushes_on(&forced, &Source::Text(&f_text)), vec![("topic".to_string(), 6000)]);
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

        // Appending to the parent transcript triggers changed_for and emits NewLines.
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
