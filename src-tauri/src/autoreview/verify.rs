//! Adversarial verification pass over an automatic review's draft findings (`[reviews] verify`).
//!
//! Runs a read-only `Adapter::fresh_side` helper over the PR's diff, surrounding checkout, and the
//! reviewer's draft output to falsify false positives, already-handled edge cases, drive-by
//! complaints outside the diff, duplicates, and pure style nits before the review is drafted on
//! GitHub or handed back to the PR's primary session. Fails open (keeps the original review) if no
//! account supports `fresh_side`, or if the verifier errors, times out, or returns unparseable
//! output.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::config::{AccountCfg, Config, ReviewsCfg};
use crate::engine::Handle;
use crate::model::{Board, PrItem};
use crate::side::RunsOn;

use super::{clean, numbered, short, skipped, verdict_of, AUTO};

/// Maximum time a verification pass may run before failing open and keeping the original review.
pub(super) const TIMEOUT: Duration = Duration::from_secs(180);

/// Picks the account and optional model for the verifier pass: `[reviews] verify_account` (or
/// routing group) when set, else a roomy enabled account whose harness supports `fresh_side`.
pub(super) fn pick_account<'a>(cfg: &'a Config, board: &Board, reviews: &ReviewsCfg, project: Option<&str>) -> Option<(&'a AccountCfg, Option<String>)> {
    let can = |a: &AccountCfg| !a.disabled && crate::harness::of(a).is_some_and(|h| h.supports_fresh_side());
    let model = Some(reviews.verify_model.trim()).filter(|m| !m.is_empty()).map(String::from);
    let asked = Some(reviews.verify_account.trim()).filter(|a| !a.is_empty());
    let (name, picked_model) = match asked {
        Some(n) if cfg.routing.groups.iter().any(|g| g.name == n) => {
            let p = crate::start::resolve_group(cfg, Some(board), n, project, &[], model.as_deref(), None, true, crate::util::now_ms(), &can)?.ok()?;
            (p.account, model.or(p.model))
        }
        Some(n) => (n.to_string(), model),
        None => {
            if let Some(Ok(p)) = crate::start::resolve_group(cfg, Some(board), AUTO, project, &[], model.as_deref(), None, true, crate::util::now_ms(), &can) {
                (p.account, model.or(p.model))
            } else {
                let first = cfg.accounts.iter().find(|a| can(a))?;
                (first.name.clone(), model)
            }
        }
    };
    let account = cfg.accounts.iter().find(|a| a.name == name && can(a))?;
    Some((account, picked_model))
}

/// System rules for the read-only verifier helper: inspect the diff and surrounding code, try to
/// disprove each finding, drop false positives and style nits, and preserve Codealong's exact
/// review format.
pub(super) fn verifier_rules(pr: &PrItem, holds: bool, reads: &Path) -> String {
    let reads = reads.display();
    let base = pr.base.as_deref().unwrap_or("its base branch");
    let checkout = if holds {
        "The folder you're in has the PR's branch checked out.".to_string()
    } else {
        format!("The folder you're in is a checkout of {} without the PR's branch; the PR's changes are in {reads}/pr.diff.", pr.repo_slug)
    };
    format!(
        "You are a strict, adversarial code review verifier for pull request #{} in {}, “{}” ({}), merging {} into {base}. \
         Its description, discussion and changed files are in {reads}/pr.md, and its numbered diff is in {reads}/pr.diff (left column = new-file line number). {checkout}\n\n\
         Your job is to eliminate review noise and false positives before the review is delivered. For EVERY finding in the draft review:\n\
         1. Verify it directly against the diff, surrounding code, callers, tests, and any repo rules (AGENTS.md, CLAUDE.md, lint configs).\n\
         2. DROP the finding if it is a false positive, already handled elsewhere in the file/callers, about unchanged legacy code outside the diff, speculative without a concrete failure path, redundant with another finding, or a subjective style/taste nitpick not backed by a repo rule.\n\
         3. KEEP only high-confidence, actionable bugs, security/data risks, broken requirements, missing critical tests, or clear violations of written repo rules. Merge duplicates.\n\n\
         Don't edit files, commit, push, or comment on GitHub. Don't start subagents.\n\
         Output ONLY the final verified review in the exact format Codealong expects:\n\
         - A concise Markdown summary of surviving findings (or a single sentence saying nothing holds up if all findings were dropped).\n\
         - If any surviving findings target specific numbered lines in the diff, a fenced ```review-comments JSON array of objects with `path`, `line`, and `body`.\n\
         - End on a line of its own with `Verdict: approve` (if nothing needs fixing), `Verdict: request changes` (if a verified issue must be fixed before merging), or `Verdict: comment` (only for an uncertain non-blocking concern).",
        pr.number, pr.repo_slug, pr.title.trim(), pr.url, pr.head
    )
}

/// Prompt handed to the verifier helper containing the draft review of commit `head`.
pub(super) fn verifier_prompt(pr: &PrItem, head: &str, raw_review: &str) -> String {
    format!(
        "Verify the following draft review of PR #{} at commit {}. Check every finding against the code and diff, drop false positives and nits, and return the cleaned review:\n\n<draft-review>\n{}\n</draft-review>",
        pr.number,
        short(head),
        raw_review.trim()
    )
}

/// Validates a verifier's reply against the original review. Returns `Some(cleaned)` only when the
/// reply is non-empty, not `SKIP:`, and ends with a valid `Verdict:` line; otherwise returns
/// `None` so callers fail open and keep the original review.
pub(super) fn parse_verified(candidate: &str) -> Option<String> {
    let trimmed = candidate.trim();
    if trimmed.is_empty() || skipped(trimmed).is_some() {
        return None;
    }
    verdict_of(trimmed)?;
    Some(trimmed.to_string())
}

fn verify_dir(pr: &PrItem, head: &str) -> PathBuf {
    crate::config::support_dir().join("pr-verify").join(format!("{}+{}+{}", pr.repo_slug.replace('/', "+"), pr.number, short(head)))
}

/// Runs the verification pass on `raw_review` for `pr` at `head`. Returns `Some(verified_review)`
/// when verification succeeds and produces a valid review, or `None` to fail open and keep
/// `raw_review` untouched.
pub(super) fn run_verify(engine: &Handle, pr: &PrItem, head: &str, raw_review: &str) -> Option<String> {
    if clean(raw_review) || skipped(raw_review).is_some() {
        return None;
    }
    let board = engine.board();
    let cfg = crate::config::current().ok()?;
    let pr = board.prs.iter().find(|p| p.key == pr.key).unwrap_or(pr);
    let reviews = board.repo_settings.reviews(&cfg.reviews, pr);
    if !reviews.verify {
        return None;
    }
    let lane = crate::side::pr_lane(&board, pr);
    let project = lane.as_ref().map(|(l, _)| l.project.as_str()).filter(|p| !p.is_empty());
    let (account, model) = pick_account(&cfg, &board, &reviews, project)?;
    let (md, diff, _) = crate::github::for_reading(pr).ok()?;
    let reads = verify_dir(pr, head);
    std::fs::create_dir_all(&reads).ok()?;
    std::fs::write(reads.join("pr.md"), md).ok()?;
    std::fs::write(reads.join("pr.diff"), numbered(&diff)).ok()?;
    let (cwd, holds) = match lane {
        Some((l, holds)) => (PathBuf::from(&l.path), holds),
        None => (reads.clone(), false),
    };
    let rules = verifier_rules(pr, holds, &reads);
    let prompt = verifier_prompt(pr, head, raw_review);
    let on = RunsOn { model: model.as_deref(), effort: None };
    let out = crate::side::run_fresh_once(account, &cwd, &rules, &reads, on, &prompt, TIMEOUT).ok();
    let _ = std::fs::remove_dir_all(&reads);
    out.and_then(|reply| parse_verified(&reply))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::github::Verdict;
    use crate::model::PrState;

    fn sample_pr() -> PrItem {
        PrItem {
            key: "acme/web#42".into(),
            repo_slug: "acme/web".into(),
            repo: "web".into(),
            project: "Acme".into(),
            number: 42,
            title: "Fix cache invalidation".into(),
            url: "https://github.com/acme/web/pull/42".into(),
            state: PrState::Open,
            author: Some("gwen".into()),
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
            primary: None,
            primary_by: None,
            head: "fix-cache".into(),
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
            head_oid: Some("abcdef1234567".into()),
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
    fn parse_verified_accepts_valid_review_and_fails_open_on_garbage_or_skip() {
        assert_eq!(parse_verified(""), None);
        assert_eq!(parse_verified("SKIP: couldn't read diff"), None);
        assert_eq!(parse_verified("Looks fine without a verdict line."), None);

        let cleaned = "All candidate findings were false positives; nothing needs fixing.\n\nVerdict: approve";
        let ok = parse_verified(cleaned).unwrap();
        assert_eq!(verdict_of(&ok), Some(Verdict::Approve));
        assert!(clean(&ok));

        let with_comment = "One real bug survives.\n\n```review-comments\n[{\"path\":\"src/cache.rs\",\"line\":12,\"body\":\"Missing lock.\"}]\n```\n\nVerdict: request changes";
        let ok = parse_verified(with_comment).unwrap();
        assert_eq!(verdict_of(&ok), Some(Verdict::RequestChanges));
        assert_eq!(super::super::parse_reply(&ok).1.len(), 1);
    }

    #[test]
    fn verifier_prompt_and_rules_include_pr_context_and_draft_review() {
        let pr = sample_pr();
        let rules = verifier_rules(&pr, true, Path::new("/tmp/reads"));
        assert!(rules.contains("/tmp/reads/pr.diff"));
        assert!(rules.contains("DROP the finding"));
        assert!(rules.contains("Verdict: approve"));
        let prompt = verifier_prompt(&pr, "abcdef1234567", "Draft finding\nVerdict: comment");
        assert!(prompt.contains("abcdef1"));
        assert!(prompt.contains("<draft-review>"));
        assert!(prompt.contains("Draft finding"));
    }
}
