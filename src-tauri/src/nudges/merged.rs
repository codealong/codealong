//! Telling whether a PR's primary session merged the PR itself, or already knows it merged, so
//! Codealong doesn't echo "It was merged." back at an agent that just landed it.
//!
//! Two layers, both deterministic and free of extra hot-path work:
//! 1. Timing (`in_turn`, `MERGE_SETTLE_MS`): while the session is in a turn when a merge arrives,
//!    `Nudger` holds the merge nudge until the turn ends and settles briefly, rather than
//!    interrupting the session mid-landing (and giving its final tool output and reply time to
//!    land on disk).
//! 2. Lazy check (`session_knows`): right before sending a merge nudge (unless you merged the PR
//!    yourself in Codealong's merge box, `Board::merged_here`), inspect what the session already
//!    shows on the board (`knows_from_session`) and, if still unsure, read the tail of its
//!    conversation (`knows_from_conversation`) for a succeeded merge command, a tool output
//!    confirming this PR merged/landed, or a post-merge reply saying so.

use std::collections::HashMap;
use std::path::Path;

use crate::engine::Handle;
use crate::model::{PrItem, Session, Status};
use crate::transcript::Item;

/// How long after a turn ends (or after recent activity) a merge nudge waits before checking the
/// session and sending, so a transcript write, a background-task wakeup or a status flip settles.
pub const MERGE_SETTLE_MS: i64 = 10_000;

/// Clock-skew and GitHub timestamp-granularity margin (GitHub timestamps are whole seconds, and a
/// merge command can finish slightly before or after GitHub stamps `mergedAt`).
const CLOCK_SKEW_MS: i64 = 30_000;

/// How long after `merged_at` a merge/land command may finish and still count as the merge itself
/// (for a script that merges and then fetches/catches up before returning).
const MERGE_AFTER_MS: i64 = 2 * 60_000;

/// How many recent conversation items to inspect lazily when deciding a merge nudge.
const RECENT_ITEMS: usize = 60;

/// Whether the session is actively in a turn (working, or waiting on an approval/question rather
/// than sitting at its prompt or stalled). Matches `rerequest.rs` and `agentdone.rs`.
pub fn in_turn(s: &Session) -> bool {
    s.status == Status::Working || (s.status == Status::Needs && !s.asks && s.stalled_since.is_none())
}

/// Whether `session_key` already merged `pr` or reported it merged/landed, checked lazily on the
/// nudger's send thread right before telling it "It was merged."
pub fn session_knows(engine: &Handle, pr: &PrItem, session_key: &str) -> bool {
    let board = engine.board();
    let Some((_, session)) = board.session(session_key) else { return false };
    if knows_from_session(pr, session) {
        return true;
    }
    let Ok(cfg) = crate::config::current() else { return false };
    let Ok(page) = crate::transcript::page(&board, &cfg, session_key, None, RECENT_ITEMS) else { return false };
    knows_from_conversation(pr, &page.items)
}

/// Fast check on what the board already holds for `session` (no disk read): a finished turn that
/// ended around or after `pr.merged_at` whose status line or insight already reports the work as
/// `landed`, unless that reply explicitly names a different PR instead of this one.
pub fn knows_from_session(pr: &PrItem, session: &Session) -> bool {
    if in_turn(session) {
        return false;
    }
    let Some(merged_at) = pr.merged_at else { return false };
    let turn_at = session.turn_ended.unwrap_or(session.last_activity);
    if turn_at < merged_at - CLOCK_SKEW_MS {
        return false;
    }
    let landed = session.insight.as_ref().and_then(|i| i.landing.as_deref()) == Some("landed")
        || crate::insights::landing_said(session) == Some("landed");
    if !landed {
        return false;
    }
    let reply = [session.reply_head.as_deref(), session.reply_tail.as_deref()].into_iter().flatten().collect::<Vec<_>>().join("\n");
    !other_pr_only(&reply, pr)
}

/// Pure check over a session's recent conversation items (`transcript::Item`, oldest first):
/// true when the session ran a succeeded command that merged `pr`, saw a succeeded tool output
/// confirming `pr` merged/landed, or wrote a post-merge reply confirming `pr` merged/landed.
pub fn knows_from_conversation(pr: &PrItem, items: &[Item]) -> bool {
    if items.is_empty() {
        return false;
    }
    let results: HashMap<&str, &Item> = items
        .iter()
        .filter(|i| i.kind == "result")
        .filter_map(|i| Some((i.id.as_deref()?, i)))
        .collect();
    let last_turn_start = items
        .iter()
        .rposition(|i| matches!(i.kind, "you" | "peer" | "codealong"))
        .map_or(0, |i| i + 1);

    for (idx, item) in items.iter().enumerate() {
        match item.kind {
            "tool" => {
                let res = item.id.as_deref().and_then(|id| results.get(id).copied());
                if item.failed || res.is_some_and(|r| r.failed) {
                    continue;
                }
                let output = res.and_then(|r| r.output.as_deref()).or(item.output.as_deref());
                // A tool call with neither an inline output nor a result item yet hasn't finished.
                if res.is_none() && item.output.is_none() {
                    continue;
                }
                let start = item.at;
                let end = res.map_or(item.at, |r| r.at);
                let in_merge_window = match pr.merged_at {
                    Some(m) => {
                        let started_ok = res.is_none() || start <= m + CLOCK_SKEW_MS;
                        started_ok && end >= m - CLOCK_SKEW_MS && end <= m + MERGE_AFTER_MS
                    }
                    None => idx >= last_turn_start,
                };
                if in_merge_window && is_merge_command(item, output, pr) {
                    return true;
                }
                let after_merge = match pr.merged_at {
                    Some(m) => end >= m - CLOCK_SKEW_MS,
                    None => idx >= last_turn_start,
                };
                if after_merge {
                    if let Some(out) = output {
                        if output_confirms_merged(out, item.cwd.as_deref(), pr, in_merge_window) {
                            return true;
                        }
                    }
                }
            }
            "agent" => {
                let after_merge = match pr.merged_at {
                    Some(m) => item.at >= m - CLOCK_SKEW_MS,
                    None => idx >= last_turn_start,
                };
                if after_merge && reply_confirms_merged(&item.text, pr) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

/// Whether a succeeded tool call executed a merge of `pr` (through `gh pr merge`, `gh api`,
/// `curl`, or a GitHub MCP merge tool), and didn't merely schedule auto-merge for later.
fn is_merge_command(call: &Item, output: Option<&str>, pr: &PrItem) -> bool {
    // Auto-merge output explicitly says it hasn't merged yet.
    if output.is_some_and(|o| o.to_ascii_lowercase().contains("will be automatically merged")) {
        return false;
    }
    let input = call.input.as_deref().unwrap_or("");
    let label = call.text.as_str();
    let lower_label = label.to_ascii_lowercase();
    let lower_input = input.to_ascii_lowercase();

    // Non-shell MCP / API tool call (e.g. GitHub MCP `merge_pull_request`).
    if !call.shell
        && (lower_label.contains("merge_pull_request")
            || lower_label.contains("mergepullrequest")
            || lower_label.contains("enqueuepullrequest")
            || lower_input.contains("merge_pull_request")
            || lower_input.contains("mergepullrequest")
            || lower_input.contains("enqueuepullrequest"))
    {
        return matches_json_or_text_pr(input, pr);
    }

    // Shell command: inspect each sub-command separated by `&&`, `||`, `;`, `|`, or newlines.
    let mut cwd_override: Option<String> = None;
    for part in split_shell_commands(input) {
        let trimmed = part.trim();
        if let Some(dir) = cd_target(trimmed) {
            cwd_override = Some(dir);
            continue;
        }
        let cwd = cwd_override.as_deref().or(call.cwd.as_deref());
        if shell_merges_pr(trimmed, cwd, pr) {
            return true;
        }
    }
    false
}

/// Splits a shell script into simple commands on top-level `&&`, `||`, `;`, `|`, and newlines
/// (ignoring separators inside single or double quotes).
fn split_shell_commands(cmd: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let bytes = cmd.as_bytes();
    let mut start = 0;
    let mut i = 0;
    let mut quote: Option<u8> = None;
    while i < bytes.len() {
        let b = bytes[i];
        if let Some(q) = quote {
            if b == b'\\' && q == b'"' && i + 1 < bytes.len() {
                i += 2;
                continue;
            }
            if b == q {
                quote = None;
            }
            i += 1;
            continue;
        }
        match b {
            b'\'' | b'"' => {
                quote = Some(b);
                i += 1;
            }
            b'\\' if i + 1 < bytes.len() => i += 2,
            b';' | b'\n' => {
                parts.push(&cmd[start..i]);
                i += 1;
                start = i;
            }
            b'&' if i + 1 < bytes.len() && bytes[i + 1] == b'&' => {
                parts.push(&cmd[start..i]);
                i += 2;
                start = i;
            }
            b'|' => {
                parts.push(&cmd[start..i]);
                i += if i + 1 < bytes.len() && bytes[i + 1] == b'|' { 2 } else { 1 };
                start = i;
            }
            _ => i += 1,
        }
    }
    parts.push(&cmd[start..]);
    parts
}

/// Extracts the target folder of a simple `cd <path>` command.
fn cd_target(part: &str) -> Option<String> {
    let mut tokens = shell_tokens(part);
    if tokens.first().map(String::as_str) != Some("cd") || tokens.len() < 2 {
        return None;
    }
    let dir = tokens.remove(1);
    (!dir.starts_with('-') && !dir.is_empty()).then_some(dir)
}

/// Lightweight whitespace/quote tokenizer for a single shell command.
fn shell_tokens(part: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    let mut chars = part.chars().peekable();
    while let Some(c) = chars.next() {
        if let Some(q) = quote {
            if c == '\\' && q == '"' {
                if let Some(next) = chars.next() {
                    cur.push(next);
                }
            } else if c == q {
                quote = None;
            } else {
                cur.push(c);
            }
            continue;
        }
        match c {
            '\'' | '"' => quote = Some(c),
            '\\' => {
                if let Some(next) = chars.next() {
                    cur.push(next);
                }
            }
            c if c.is_whitespace() => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            _ => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Whether one shell command (`gh pr merge`, `gh api .../pulls/<n>/merge`, GraphQL
/// `mergePullRequest` / `enqueuePullRequest`, or `curl .../pulls/<n>/merge`) targets `pr`.
fn shell_merges_pr(part: &str, cwd: Option<&str>, pr: &PrItem) -> bool {
    let tokens = shell_tokens(part);
    // Skip leading env assignments (`FOO=1 gh pr merge ...`) and `env`.
    let mut idx = 0;
    while idx < tokens.len() {
        let t = tokens[idx].as_str();
        if t == "env" || (t.contains('=') && !t.starts_with('-')) {
            idx += 1;
        } else {
            break;
        }
    }
    let rest = &tokens[idx..];
    let Some(prog) = rest.first().map(|s| Path::new(s).file_name().and_then(|n| n.to_str()).unwrap_or(s.as_str())) else {
        return false;
    };

    if prog == "gh" {
        if let Some(m) = gh_pr_merge_target(&rest[1..]) {
            return match m {
                MergeTarget::DisableAuto => false,
                MergeTarget::Number(n) => n == pr.number,
                MergeTarget::Branch(b) => b == pr.head,
                MergeTarget::Implicit => cwd_matches_pr(cwd, pr),
            };
        }
        // `gh api ... /pulls/<n>/merge` or GraphQL `mergePullRequest` / `enqueuePullRequest`.
        let lower = part.to_ascii_lowercase();
        if rest.get(1).map(String::as_str) == Some("api") {
            let api_merge = format!("/pulls/{}/merge", pr.number);
            let rel_merge = format!("pulls/{}/merge", pr.number);
            if lower.contains(&api_merge) || lower.contains(&rel_merge) {
                return true;
            }
            if lower.contains("mergepullrequest") || lower.contains("enqueuepullrequest") {
                return matches_json_or_text_pr(part, pr);
            }
        }
        return false;
    }

    if prog == "curl" {
        let lower = part.to_ascii_lowercase();
        let api_merge = format!("/pulls/{}/merge", pr.number);
        return lower.contains(&api_merge);
    }

    false
}

enum MergeTarget {
    DisableAuto,
    Number(u64),
    Branch(String),
    Implicit,
}

/// Parses `gh [global flags] pr merge [flags] [target]` into what it targets.
fn gh_pr_merge_target(args: &[String]) -> Option<MergeTarget> {
    let mut i = 0;
    // Skip flags before `pr` (e.g. `gh -R owner/repo pr merge`).
    while i < args.len() {
        let a = args[i].as_str();
        if a == "pr" {
            break;
        }
        if matches!(a, "-R" | "--repo") {
            i += 2;
        } else if a.starts_with('-') {
            i += 1;
        } else {
            return None;
        }
    }
    if args.get(i).map(String::as_str) != Some("pr") || args.get(i + 1).map(String::as_str) != Some("merge") {
        return None;
    }
    let mut j = i + 2;
    let mut positional: Option<&str> = None;
    while j < args.len() {
        let a = args[j].as_str();
        if a == "--disable-auto" {
            return Some(MergeTarget::DisableAuto);
        }
        if matches!(a, "-R" | "--repo" | "-t" | "--subject" | "-b" | "--body" | "-F" | "--body-file" | "--match-head-commit" | "--author-email") {
            j += 2;
            continue;
        }
        if a.starts_with('-') {
            j += 1;
            continue;
        }
        positional = Some(a);
        j += 1;
    }
    let Some(target) = positional else {
        return Some(MergeTarget::Implicit);
    };
    if let Ok(n) = target.trim_start_matches('#').parse::<u64>() {
        return Some(MergeTarget::Number(n));
    }
    if let Some(key) = crate::prlinks::from_url(target) {
        if let Some((_, num)) = key.rsplit_once('#') {
            if let Ok(n) = num.parse::<u64>() {
                return Some(MergeTarget::Number(n));
            }
        }
    }
    Some(MergeTarget::Branch(target.to_string()))
}

/// Whether `cwd` is inside one of `pr.lanes` (or either side is unknown, in which case we defer
/// to the session already being `pr`'s primary).
fn cwd_matches_pr(cwd: Option<&str>, pr: &PrItem) -> bool {
    let Some(cwd) = cwd.filter(|c| !c.is_empty()) else { return true };
    if pr.lanes.is_empty() {
        return true;
    }
    let cwd = Path::new(cwd);
    pr.lanes.iter().any(|lane| {
        let lane = Path::new(lane);
        cwd == lane || cwd.starts_with(lane) || lane.starts_with(cwd)
    })
}

/// Whether an MCP / GraphQL tool input matches `pr`: if it explicitly names PR number(s), `pr.number`
/// must be among them; if it names none (e.g. only an opaque GraphQL node ID), accept it as long as
/// no other PR number is named.
fn matches_json_or_text_pr(text: &str, pr: &PrItem) -> bool {
    if mentions_pr(text, pr) {
        return true;
    }
    if let Some(num) = json_pr_number(text) {
        return num == pr.number;
    }
    !other_pr_only(text, pr)
}

/// Extracts `"pull_number": 42` / `"pullNumber": 42` / `"number": 42` from a JSON/CLI payload.
fn json_pr_number(text: &str) -> Option<u64> {
    for key in ["\"pull_number\"", "\"pullNumber\"", "\"pr_number\"", "\"number\""] {
        let Some(pos) = text.find(key) else { continue };
        let rest = text[pos + key.len()..].trim_start();
        let Some(rest) = rest.strip_prefix(':') else { continue };
        let digits: String = rest.trim_start().chars().take_while(|c| c.is_ascii_digit()).collect();
        if let Ok(n) = digits.parse::<u64>() {
            return Some(n);
        }
    }
    None
}

/// Whether a succeeded tool output confirms that `pr` was merged or landed: covers `gh pr merge`,
/// `gh pr view`, and custom landing scripts (`scripts/land.sh`, `bin/ship`, etc.).
fn output_confirms_merged(output: &str, cwd: Option<&str>, pr: &PrItem, in_merge_window: bool) -> bool {
    let lower = output.to_ascii_lowercase();
    if lower.contains("will be automatically merged") {
        return false;
    }
    // `gh pr view` tabular or JSON output naming this PR and `MERGED`.
    if mentions_pr(output, pr)
        && (output.contains("state:\tMERGED")
            || output.contains("state: MERGED")
            || output.contains("\"state\":\"MERGED\"")
            || output.contains("\"state\": \"MERGED\""))
    {
        return true;
    }
    // Line-by-line confirmation naming this PR.
    for line in output.lines() {
        if mentions_pr(line, pr) && line_affirms_merged(line) {
            return true;
        }
    }
    // A landing script's `Landed:` summary line in the merge window, in the PR's worktree,
    // without naming a different PR.
    if in_merge_window && cwd_matches_pr(cwd, pr) && !other_pr_only(output, pr) {
        if output.lines().any(starts_with_landed) {
            return true;
        }
    }
    false
}

/// Whether an agent's post-merge reply already reports `pr` as merged or landed.
fn reply_confirms_merged(text: &str, pr: &PrItem) -> bool {
    // First line opening with `Landed:` (matching `insights::landing_said`), unless it names a
    // different PR instead of this one.
    if text.lines().next().is_some_and(starts_with_landed) && !other_pr_only(text, pr) {
        return true;
    }
    // Any line explicitly naming this PR and affirmatively stating it was merged/landed.
    text.lines().any(|line| mentions_pr(line, pr) && line_affirms_merged(line))
}

fn starts_with_landed(line: &str) -> bool {
    let head = line.trim_start_matches(|c: char| !c.is_alphanumeric());
    head.get(..7).is_some_and(|p| p.eq_ignore_ascii_case("landed:"))
}

/// Whether `line` affirmatively states something was merged/landed rather than pending, blocked,
/// negated, or merging a base branch into the PR branch.
fn line_affirms_merged(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    let has_verb = has_word(&lower, "merged") || has_word(&lower, "landed");
    if !has_verb {
        return false;
    }
    const REJECT: &[&str] = &[
        "not merged",
        "not yet merged",
        "not been merged",
        "hasn't merged",
        "haven't merged",
        "didn't merge",
        "did not merge",
        "unmerged",
        "will be merged",
        "will be automatically merged",
        "will merge",
        "to be merged",
        "ready to merge",
        "waiting to merge",
        "waiting for",
        "before merg",
        "once merged",
        "when merged",
        "if merged",
        "until merged",
        "auto-merge",
        "automerge",
        "merge conflict",
        "couldn't merge",
        "could not merge",
        "failed to merge",
        "unable to merge",
        "can't merge",
        "cannot merge",
        "merged main",
        "merged master",
        "merged origin/",
        "merged upstream/",
        "merged base",
        "without being merged",
        "closed without",
        "not landed",
        "not yet landed",
        "hasn't landed",
        "didn't land",
    ];
    !REJECT.iter().any(|p| lower.contains(p))
}

fn has_word(lower: &str, word: &str) -> bool {
    let bytes = lower.as_bytes();
    let wlen = word.len();
    let mut from = 0;
    while let Some(pos) = lower[from..].find(word) {
        let start = from + pos;
        let end = start + wlen;
        let before = start == 0 || !bytes[start - 1].is_ascii_alphanumeric();
        let after = end == bytes.len() || !bytes[end].is_ascii_alphanumeric();
        if before && after {
            return true;
        }
        from = start + 1;
    }
    false
}

/// Whether `text` explicitly names `pr` by `#<number>`, `/pull/<number>`, or `/pulls/<number>`.
fn mentions_pr(text: &str, pr: &PrItem) -> bool {
    let num = pr.number.to_string();
    let bytes = text.as_bytes();
    for needle in [format!("#{num}"), format!("/pull/{num}"), format!("/pulls/{num}")] {
        let mut from = 0;
        while let Some(pos) = text[from..].find(&needle) {
            let start = from + pos;
            let end = start + needle.len();
            let before_ok = if needle.starts_with('#') {
                start == 0 || !matches!(bytes[start - 1], b'&' | b'0'..=b'9')
            } else {
                true
            };
            let after_ok = end == bytes.len() || !(bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_');
            if before_ok && after_ok {
                return true;
            }
            from = start + 1;
        }
    }
    false
}

/// Whether `text` explicitly references at least one PR number (via `/pull/<n>`, `/pulls/<n>`,
/// `pr #<n>`, `pull request #<n>`, or `<owner>/<repo>#<n>`) and NONE of them is `pr.number`.
fn other_pr_only(text: &str, pr: &PrItem) -> bool {
    let mut found_other = false;
    for n in explicit_pr_numbers(text) {
        if n == pr.number {
            return false;
        }
        found_other = true;
    }
    found_other
}

/// Extracts explicit PR numbers from URLs (`/pull/N`, `/pulls/N`), prose (`PR #N`, `pull request
/// #N`), and repo keys (`owner/repo#N`). Leaves bare `#1` (e.g. list item numbers) alone.
fn explicit_pr_numbers(text: &str) -> Vec<u64> {
    let mut out = Vec::new();
    let lower = text.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let parse_at = |pos: usize| -> Option<u64> {
        let digits: String = lower[pos..].chars().take_while(|c| c.is_ascii_digit()).collect();
        if digits.is_empty() {
            return None;
        }
        let end = pos + digits.len();
        if end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
            return None;
        }
        digits.parse().ok()
    };
    for prefix in ["/pull/", "/pulls/", "pr #", "pull request #"] {
        let mut from = 0;
        while let Some(pos) = lower[from..].find(prefix) {
            let start = from + pos;
            let before_ok = prefix.starts_with('/') || start == 0 || !bytes[start - 1].is_ascii_alphanumeric();
            if before_ok {
                if let Some(n) = parse_at(start + prefix.len()) {
                    out.push(n);
                }
            }
            from = start + 1;
        }
    }
    // `owner/repo#N`: a `#N` immediately preceded by `a/b`.
    let mut from = 0;
    while let Some(pos) = lower[from..].find('#') {
        let hash = from + pos;
        let token_start = lower[..hash].rfind(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '/' | '-' | '_' | '.'))).map_or(0, |i| i + 1);
        let before = &lower[token_start..hash];
        if let Some((owner, repo)) = before.split_once('/') {
            if !owner.is_empty() && !repo.is_empty() && !repo.contains('/') {
                if let Some(n) = parse_at(hash + 1) {
                    out.push(n);
                }
            }
        }
        from = hash + 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Checks, Merge, PrState, SessionInsight};
    use crate::transcript::item;

    const MERGED_AT: i64 = 1_800_000_000_000;

    fn pr(number: u64) -> PrItem {
        PrItem {
            key: format!("o/r#{number}"),
            repo_slug: "o/r".into(),
            repo: "r".into(),
            project: "P".into(),
            number,
            title: "Ship feature".into(),
            url: format!("https://github.com/o/r/pull/{number}"),
            state: PrState::Merged,
            author: Some("me".into()),
            mine: true,
            review_requested: false,
            requested_by: None,
            requested_at: None,
            reviewers_asked: Vec::new(),
            snoozed_until: None,
            nudge: false,
            nudge_by_default: false,
            watched: false,
            review_first: false,
            waits_to_merge: false,
            taken_over: false,
            primary: Some("claude:claude:a".into()),
            primary_by: None,
            head: "feat".into(),
            base: Some("main".into()),
            checks: Checks::default(),
            merge: Merge::Unknown,
            additions: 1,
            deletions: 0,
            changed_files: 1,
            created_at: MERGED_AT - 600_000,
            updated_at: MERGED_AT,
            merged_at: Some(MERGED_AT),
            closed_at: None,
            lanes: vec!["/work/feat".into()],
            sessions: Vec::new(),
            head_oid: None,
            authored_by: None,
            labels: Vec::new(),
            auto_review: None,
            last_review: None,
            stale_approval: None,
            turn: None,
            shared_review: Default::default(),
        }
    }

    fn shell_call(id: &str, at: i64, cmd: &str, cwd: Option<&str>) -> Item {
        Item { id: Some(id.into()), input: Some(cmd.into()), shell: true, cwd: cwd.map(String::from), ..item(at, "tool", format!("Bash · `{cmd}`")) }
    }

    fn shell_res(id: &str, at: i64, out: &str, failed: bool) -> Item {
        Item { id: Some(id.into()), output: Some(out.into()), failed, ..item(at, "result", "") }
    }

    #[test]
    fn direct_gh_pr_merge_suppresses_nudge_including_after_cleanup_steps() {
        let p = pr(42);
        let items = vec![
            item(MERGED_AT - 20_000, "you", "Merge it and clean up."),
            shell_call("t1", MERGED_AT - 2_000, "gh pr merge 42 --squash --delete-branch", Some("/work/feat")),
            shell_res("t1", MERGED_AT + 1_000, "✓ Squashed and merged pull request #42", false),
            shell_call("t2", MERGED_AT + 10_000, "git worktree remove /work/feat", Some("/work")),
            shell_res("t2", MERGED_AT + 12_000, "", false),
            item(MERGED_AT + 15_000, "agent", "All done."),
        ];
        assert!(knows_from_conversation(&p, &items));
    }

    #[test]
    fn implicit_gh_pr_merge_matches_same_worktree_and_rejects_other_worktree() {
        let p = pr(42);
        let same = vec![
            shell_call("t1", MERGED_AT - 1_000, "gh pr merge --squash", Some("/work/feat")),
            shell_res("t1", MERGED_AT + 1_000, "", false),
        ];
        assert!(knows_from_conversation(&p, &same));

        let other_dir = vec![
            shell_call("t1", MERGED_AT - 1_000, "gh pr merge --squash", Some("/work/other")),
            shell_res("t1", MERGED_AT + 1_000, "", false),
        ];
        assert!(!knows_from_conversation(&p, &other_dir));

        let cd_into = vec![
            shell_call("t1", MERGED_AT - 1_000, "cd /work/feat && gh pr merge --squash", Some("/work")),
            shell_res("t1", MERGED_AT + 1_000, "", false),
        ];
        assert!(knows_from_conversation(&p, &cd_into));
    }

    #[test]
    fn gh_pr_merge_for_another_pr_or_failed_or_disable_auto_does_not_suppress() {
        let p = pr(42);
        // Different PR number.
        let other = vec![
            shell_call("t1", MERGED_AT - 1_000, "gh pr merge 41 --squash", Some("/work/feat")),
            shell_res("t1", MERGED_AT + 1_000, "✓ Squashed and merged pull request #41", false),
        ];
        assert!(!knows_from_conversation(&p, &other));
        // Failed merge command.
        let failed = vec![
            shell_call("t1", MERGED_AT - 1_000, "gh pr merge 42 --squash", Some("/work/feat")),
            shell_res("t1", MERGED_AT + 1_000, "Pull request #42 is not mergeable", true),
        ];
        assert!(!knows_from_conversation(&p, &failed));
        // Disabling auto-merge isn't merging.
        let disable = vec![
            shell_call("t1", MERGED_AT - 1_000, "gh pr merge 42 --disable-auto", Some("/work/feat")),
            shell_res("t1", MERGED_AT + 1_000, "Auto-merge disabled", false),
        ];
        assert!(!knows_from_conversation(&p, &disable));
    }

    #[test]
    fn auto_merge_queued_earlier_is_still_nudged_when_it_merges_later() {
        let p = pr(42);
        // Agent queued auto-merge 8 minutes earlier and stopped.
        let earlier = vec![
            shell_call("t1", MERGED_AT - 8 * 60_000, "gh pr merge 42 --auto --squash", Some("/work/feat")),
            shell_res("t1", MERGED_AT - 8 * 60_000 + 2_000, "Pull request #42 will be automatically merged", false),
            item(MERGED_AT - 8 * 60_000 + 5_000, "agent", "Queued PR #42 for auto-merge."),
        ];
        assert!(!knows_from_conversation(&p, &earlier));
        // Even if the auto-merge command ran right before merged_at, its output explicitly said it
        // only queued it.
        let queued_just_before = vec![
            shell_call("t1", MERGED_AT - 5_000, "gh pr merge 42 --auto --squash", Some("/work/feat")),
            shell_res("t1", MERGED_AT - 4_000, "Pull request #42 will be automatically merged", false),
        ];
        assert!(!knows_from_conversation(&p, &queued_just_before));
    }

    #[test]
    fn blocking_landing_script_and_codex_inline_tool_suppress_nudge() {
        let p = pr(42);
        // Custom script started 6 minutes before merged_at and returned right after with Landed:.
        let script = vec![
            shell_call("t1", MERGED_AT - 6 * 60_000, "scripts/land.sh", Some("/work/feat")),
            shell_res("t1", MERGED_AT + 4_000, "Landed: abc1234 is in origin/main.", false),
        ];
        assert!(knows_from_conversation(&p, &script));

        // Codex emits one completed tool item (no separate result item) at completion time.
        let codex = vec![Item {
            input: Some("gh -R o/r pr merge https://github.com/o/r/pull/42 --squash".into()),
            output: Some("".into()),
            shell: true,
            cwd: Some("/work/feat".into()),
            ..item(MERGED_AT + 1_000, "tool", "Shell · `gh pr merge`")
        }];
        assert!(knows_from_conversation(&p, &codex));
    }

    #[test]
    fn gh_api_curl_mcp_and_gh_pr_view_are_recognized() {
        let p = pr(42);
        let api = vec![
            shell_call("t1", MERGED_AT - 1_000, "gh api -X PUT repos/o/r/pulls/42/merge", Some("/work/feat")),
            shell_res("t1", MERGED_AT + 1_000, r#"{"merged":true}"#, false),
        ];
        assert!(knows_from_conversation(&p, &api));

        let mcp = vec![Item {
            input: Some(r#"{"owner":"o","repo":"r","pull_number":42}"#.into()),
            output: Some("merged".into()),
            ..item(MERGED_AT + 1_000, "tool", "github · merge_pull_request")
        }];
        assert!(knows_from_conversation(&p, &mcp));

        let mcp_other = vec![Item {
            input: Some(r#"{"owner":"o","repo":"r","pull_number":41}"#.into()),
            output: Some("merged".into()),
            ..item(MERGED_AT + 1_000, "tool", "github · merge_pull_request")
        }];
        assert!(!knows_from_conversation(&p, &mcp_other));

        let viewed = vec![
            shell_call("t1", MERGED_AT + 20_000, "gh pr view 42", Some("/work/feat")),
            shell_res("t1", MERGED_AT + 21_000, "title:\tShip feature\nstate:\tMERGED\nurl:\thttps://github.com/o/r/pull/42\n", false),
        ];
        assert!(knows_from_conversation(&p, &viewed));
    }

    #[test]
    fn post_merge_reply_and_session_insight_suppress_only_for_this_pr_after_merge() {
        let p = pr(42);
        // Post-merge reply confirming #42 merged.
        assert!(knows_from_conversation(&p, &[item(MERGED_AT + 5_000, "agent", "Merged PR #42 and removed the worktree.")]));
        assert!(knows_from_conversation(&p, &[item(MERGED_AT + 5_000, "agent", "Landed: abc1234 is in origin/main.")]));
        // Negative / future / base-merge phrasings must NOT suppress.
        for nope in [
            "PR #42 is ready to merge.",
            "Waiting for #42 to be merged.",
            "PR #42 has not been merged yet.",
            "Merged main into PR #42 to resolve conflicts.",
            "Queued PR #42 for auto-merge.",
            "Failed to merge #42 due to branch protection.",
            "Landed: abc1234 for PR #41.",
        ] {
            assert!(!knows_from_conversation(&p, &[item(MERGED_AT + 5_000, "agent", nope)]), "should not match: {nope}");
        }
        // Stale reply from before merged_at must NOT suppress.
        assert!(!knows_from_conversation(&p, &[item(MERGED_AT - 120_000, "agent", "Landed: abc1234 is in origin/main.")]));

        // Session-level insight / reply_head check.
        let mut s = crate::fixtures::session("claude:claude:a");
        s.status = Status::Unread;
        s.turn_ended = Some(MERGED_AT + 5_000);
        s.reply_head = Some("Landed: abc1234 is in origin/main.".into());
        assert!(knows_from_session(&p, &s));
        // Working session doesn't trust stale insight from prior turn.
        assert!(!knows_from_session(&p, &Session { status: Status::Working, ..s.clone() }));
        // Earlier turn before merged_at doesn't suppress.
        assert!(!knows_from_session(&p, &Session { turn_ended: Some(MERGED_AT - 120_000), ..s.clone() }));
        // Landing another PR explicitly doesn't suppress #42.
        assert!(!knows_from_session(&p, &Session { reply_head: Some("Landed: PR #41 is in origin/main.".into()), ..s.clone() }));
        // Decision model insight with landing == "landed" also works without `Landed:` prefix.
        let via_insight = Session {
            reply_head: Some("All set on main.".into()),
            insight: Some(SessionInsight { landing: Some("landed".into()), ..Default::default() }),
            ..s
        };
        assert!(knows_from_session(&p, &via_insight));
    }
}
