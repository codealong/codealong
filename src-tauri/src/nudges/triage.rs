//! Side-fork PR triage helper (`[pull_requests] triage_nudges`): runs a throwaway read-only copy
//! of the PR's primary session (`side::run_session_once`) before interrupting or waking it on PR
//! nudges (CI failures, review comments, conflicts) or automatic own-PR review handbacks.
//!
//! Because the side copy inherits the main session's conversation history, it knows why the code
//! was written the way it was, can inspect failing CI logs or the current branch on disk, and
//! decides either `ACTION: IGNORE` (suppressing the wakeup while keeping a quiet record in
//! `sides.json`) or `ACTION: NOTIFY` (delivering a distilled, high-signal digest). Fails open
//! (delivers the original message) if the harness doesn't support side forks, times out, or
//! returns unparseable output.

use std::time::Duration;

use crate::config::NudgeAbout;
use crate::engine::Handle;
use crate::model::PrItem;
use crate::nudges::{left_out, Why, NOT_A_MERGE};

/// Maximum time a side-fork triage helper may run before failing open.
pub const TIMEOUT: Duration = Duration::from_secs(180);

/// Outcome of a side-fork triage pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// Suppress interrupting/waking the main session, with a short explanation kept for inspection.
    Ignore(String),
    /// Notify the main session with this distilled, actionable digest.
    Notify(String),
}

/// Whether a PR nudge should go through the side-fork triage helper before reaching the main
/// session. Watched PRs and bare approvals/merges/closes bypass triage.
pub fn needs_triage(why: &Why, watched: bool) -> bool {
    if watched {
        return false;
    }
    why.since.is_some() || why.changes || !why.failed.is_empty() || why.conflicts
}

/// Builds the read-only triage prompt for an incoming PR nudge.
pub fn nudge_prompt(pr: &PrItem, why: &Why, raw_message: &str) -> String {
    let mut checks = Vec::new();
    if !why.failed.is_empty() {
        checks.push(format!(
            "- Failing checks ({}): inspect the CI logs (`gh pr checks {}` / `gh run view --log-failed`) to tell a real regression caused by this PR from a flaky test, infra/runner blip, or failure already fixed on the branch. If real, extract the exact failing test/command, file/line, and concise root cause.",
            why.failed.join(", "),
            pr.number
        ));
    }
    if why.since.is_some() || why.changes {
        checks.push("- Review comments / feedback: check each comment against the current code on the branch and the main session's prior decisions. Drop comments that are already addressed, false positives, casual non-actionable chatter, or subjective nits.".to_string());
    }
    if why.conflicts {
        checks.push("- Merge conflicts: verify whether the branch still conflicts with its base.".to_string());
    }
    format!(
        "You are triaging an incoming GitHub update on PR #{} “{}” ({}) BEFORE deciding whether to interrupt or wake the main session.\n\
         Stay strictly read-only: don't edit files, commit, push, comment on GitHub, or call `codealong session send`. Codealong delivers your result if you ask it to notify.\n\n\
         What to verify:\n{}\n\n\
         Raw incoming update:\n<pr-update>\n{}\n</pr-update>\n\n\
         Respond with your decision on the FIRST line, and nothing before it:\n\
         - `ACTION: IGNORE` followed by 1–2 sentences explaining why the main session doesn't need to be interrupted.\n\
         - `ACTION: NOTIFY` followed by a tight, actionable bullet list (one `- ` line per real item) containing ONLY what genuinely needs action from the main session, with exact failing tests, file paths, lines, and root causes where helpful.",
        pr.number,
        pr.title.trim(),
        pr.url,
        checks.join("\n"),
        raw_message.trim()
    )
}

/// Builds the read-only triage prompt for an automatic own-PR review handback (`[reviews.own]`).
pub fn own_review_prompt(pr: &PrItem, head: &str, agent: &str, review: &str) -> String {
    let short = &head[..head.len().min(7)];
    format!(
        "You are triaging an automated review of PR #{} “{}” ({}) at commit {short} written by {agent}, BEFORE deciding whether to interrupt the main session.\n\
         Stay strictly read-only: don't edit files, commit, push, comment on GitHub, or call `codealong session send`.\n\n\
         Check EVERY finding against the current branch code and the main session's prior design decisions and user instructions:\n\
         - DROP false positives, issues already fixed on the branch, intentional tradeoffs already decided in this conversation, and low-value style nits.\n\
         - KEEP only genuine bugs, security/data risks, missing critical tests, or broken requirements that actually need action.\n\n\
         <agent-review>\n{}\n</agent-review>\n\n\
         Respond with your decision on the FIRST line, and nothing before it:\n\
         - `ACTION: IGNORE` followed by 1–2 sentences explaining why nothing in the review holds up or needs action.\n\
         - `ACTION: NOTIFY` followed by a tight bullet list of ONLY the surviving findings that actually hold up, with file paths, lines, and what to fix.",
        pr.number,
        pr.title.trim(),
        pr.url,
        review.trim()
    )
}

/// Parses a triage helper's reply into `Decision::Ignore` or `Decision::Notify`. Returns `None`
/// (fail-open) if the reply doesn't begin with a recognizable `ACTION:` directive or if `NOTIFY`
/// has an empty body.
pub fn parse_decision(reply: &str) -> Option<Decision> {
    let mut lines = reply.lines().skip_while(|l| l.trim().is_empty());
    let first = lines.next()?.trim();
    let clean_first = first.trim_matches(|c: char| matches!(c, '*' | '_' | '`' | '#' | ' '));
    let upper = clean_first.to_ascii_uppercase();
    let rest_lines: Vec<&str> = lines.collect();
    let rest_block = rest_lines.join("\n");
    let rest_block = rest_block.trim();

    fn strip_sep(s: &str) -> &str {
        s.trim_start_matches(|c: char| matches!(c, '*' | '_' | '`' | ':' | '-' | '—' | '–' | ' ')).trim()
    }
    if let Some(after) = upper.strip_prefix("ACTION: IGNORE").or_else(|| upper.strip_prefix("ACTION:IGNORE")) {
        let inline = strip_sep(&clean_first[clean_first.len() - after.len()..]);
        let reason = match (inline.is_empty(), rest_block.is_empty()) {
            (false, false) => format!("{inline} {rest_block}"),
            (false, true) => inline.to_string(),
            (true, false) => rest_block.to_string(),
            (true, true) => "No actionable items after triage.".to_string(),
        };
        return Some(Decision::Ignore(crate::util::one_line(&reason, 240)));
    }
    if let Some(after) = upper.strip_prefix("ACTION: NOTIFY").or_else(|| upper.strip_prefix("ACTION:NOTIFY")) {
        let inline = strip_sep(&clean_first[clean_first.len() - after.len()..]);
        let body = match (inline.is_empty(), rest_block.is_empty()) {
            (false, false) => format!("{inline}\n{rest_block}"),
            (false, true) => inline.to_string(),
            (true, false) => rest_block.to_string(),
            (true, true) => return None,
        };
        return Some(Decision::Notify(body));
    }
    None
}

/// Wraps a triaged PR nudge digest in Codealong's standard PR nudge envelope so `nudges::glance`
/// still renders the PR card cleanly in the conversation.
pub fn format_triaged_nudge(pr: &PrItem, digest: &str, runs_about: NudgeAbout) -> String {
    let whose = if pr.mine { "your PR" } else { "PR" };
    let items: Vec<String> = digest
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(|l| l.strip_prefix("- ").or_else(|| l.strip_prefix("* ")).unwrap_or(l))
        .map(crate::glance::item)
        .collect();
    let body = items.join("\n");
    let tail = match left_out(runs_about).filter(|_| pr.state.is_open()) {
        Some(what) => format!("\nThis PR's {what} aren't passed on to you; if you're waiting on one, run `codealong pr watch {}`.", pr.key),
        None => String::new(),
    };
    format!("On {whose} #{} “{}” ({}):\n{body}\nTriaged by a helper before notifying you. Take it from there, unless you were told to leave it to someone else. {NOT_A_MERGE}{tail}", pr.number, pr.title.trim(), pr.url)
}

/// Wraps a triaged own-PR review digest after `autoreview::back_message` so `autoreview::glance`
/// still recognizes the card while giving the main session only the verified findings inline.
pub fn format_triaged_review(base_message: &str, digest: &str) -> String {
    format!("{base_message}\n\nTriaged findings (verified against this session's context):\n{}", digest.trim())
}

/// Runs the side-fork triage helper for a PR nudge job when `[pull_requests] triage_nudges` is on
/// and `needs_triage` holds. Returns `None` if suppressed (`ACTION: IGNORE`), or `Some(message)`
/// (either the distilled digest or the original `raw_message` on fail-open).
pub fn triage_nudge(engine: &Handle, job: &crate::nudges::Job, raw_message: String) -> Option<String> {
    if !needs_triage(&job.why, job.pr.watched) {
        return Some(raw_message);
    }
    let Ok(cfg) = crate::config::current() else { return Some(raw_message) };
    let board = engine.board();
    if !board.repo_settings.pull_requests(&cfg.pull_requests, &job.pr).triage_nudges {
        return Some(raw_message);
    }
    let prompt = nudge_prompt(&job.pr, &job.why, &raw_message);
    let label = format!("Triage: PR #{} {}", job.pr.number, job.pr.title.trim());
    let Ok(reply) = crate::side::run_session_once(&board, &cfg, &job.session, &prompt, Some(&label), TIMEOUT) else {
        return Some(raw_message);
    };
    match parse_decision(&reply) {
        Some(Decision::Ignore(_)) => None,
        Some(Decision::Notify(digest)) => Some(format_triaged_nudge(&job.pr, &digest, job.runs_about)),
        None => Some(raw_message),
    }
}

/// Runs the side-fork triage helper for an automatic own-PR review handback (`[reviews.own]`).
/// Returns `Some(Decision)` when triage ran and produced a valid decision, or `None` to fail open.
pub fn triage_own_review(engine: &Handle, pr: &PrItem, head: &str, session: &str, agent: &str, review: &str) -> Option<Decision> {
    let cfg = crate::config::current().ok()?;
    let board = engine.board();
    if !board.repo_settings.pull_requests(&cfg.pull_requests, pr).triage_nudges {
        return None;
    }
    let prompt = own_review_prompt(pr, head, agent, review);
    let label = format!("Triage: {agent} review of PR #{}", pr.number);
    let reply = crate::side::run_session_once(&board, &cfg, session, &prompt, Some(&label), TIMEOUT).ok()?;
    parse_decision(&reply)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::PrState;

    fn sample_pr() -> PrItem {
        PrItem {
            key: "acme/web#12".into(),
            repo_slug: "acme/web".into(),
            repo: "web".into(),
            project: "Acme".into(),
            number: 12,
            title: "Add rate limiter".into(),
            url: "https://github.com/acme/web/pull/12".into(),
            state: PrState::Open,
            author: Some("gwen".into()),
            mine: true,
            review_requested: false,
            requested_by: None,
            requested_at: None,
            reviewers_asked: Vec::new(),
            snoozed_until: None,
            nudge: true,
            nudge_by_default: false,
            watched: false,
            review_first: false,
            waits_to_merge: false,
            taken_over: false,
            primary: None,
            primary_by: None,
            head: "rate-limit".into(),
            base: Some("main".into()),
            checks: Default::default(),
            merge: crate::model::Merge::Unknown,
            additions: 10,
            deletions: 2,
            changed_files: 1,
            created_at: 0,
            updated_at: 0,
            merged_at: None,
            closed_at: None,
            lanes: Vec::new(),
            sessions: Vec::new(),
            head_oid: Some("1234567".into()),
            authored_by: None,
            labels: Vec::new(),
            auto_review: None,
            last_review: None,
            stale_approval: None,
            turn: None,
            shared_review: Default::default(),
        }
    }

    #[test]
    fn needs_triage_only_for_unwatched_work_items() {
        assert!(needs_triage(&Why { failed: vec!["ci".into()], ..Why::default() }, false));
        assert!(needs_triage(&Why { since: Some(10), ..Why::default() }, false));
        assert!(needs_triage(&Why { conflicts: true, ..Why::default() }, false));
        assert!(!needs_triage(&Why { failed: vec!["ci".into()], ..Why::default() }, true));
        assert!(!needs_triage(&Why { approved: true, ..Why::default() }, false));
        assert!(!needs_triage(&Why { ended: Some(PrState::Merged), ..Why::default() }, false));
    }

    #[test]
    fn parse_decision_handles_ignore_notify_markdown_and_malformed_fail_open() {
        assert_eq!(parse_decision("ACTION: IGNORE\nFlaky runner timeout unrelated to PR."), Some(Decision::Ignore("Flaky runner timeout unrelated to PR.".into())));
        assert_eq!(parse_decision("**ACTION: IGNORE** — Already fixed in the latest commit."), Some(Decision::Ignore("Already fixed in the latest commit.".into())));
        assert_eq!(
            parse_decision("`ACTION: NOTIFY`\n- `test_burst` fails at src/limit.rs:42 due to off-by-one."),
            Some(Decision::Notify("- `test_burst` fails at src/limit.rs:42 due to off-by-one.".into()))
        );
        assert_eq!(parse_decision("ACTION: NOTIFY\n   "), None, "empty notify fails open");
        assert_eq!(parse_decision("I'm not sure what to do."), None, "unstructured reply fails open");
    }

    #[test]
    fn triaged_nudge_keeps_glance_compatible_header() {
        let pr = sample_pr();
        let text = format_triaged_nudge(&pr, "- `test_burst` failed in src/limit.rs:42", NudgeAbout::FeedbackAndChecks);
        let g = crate::nudges::glance(&text).expect("glance must parse triaged nudge");
        assert_eq!(g.pr.as_ref().map(|p| p.number), Some(12));
        assert_eq!(g.parts.len(), 1);
        assert!(text.contains(NOT_A_MERGE));
        assert!(text.contains("codealong pr watch acme/web#12"));
    }
}
