//! Deterministic and decision-model pre-filtering for PR nudges (nudges.rs):
//! - Bot filtering with an optional per-repo/global allowlist (`[pull_requests] nudge_bots`);
//! - Coalescing rapid bursts of PR updates (CI check failures, review comments, conflicts) over a
//!   short quiet window (`COALESCE_MS`, capped at `COALESCE_MAX_MS`) so one burst sends one nudge;
//! - Actionable-comment classification via `prasks.rs` (`[decisions] pull_requests`) so casual
//!   chatter ("nice!", "LGTM", "thanks") doesn't interrupt the PR's primary session.

use std::collections::{HashMap, HashSet};

use crate::config::Config;
use crate::github::PrComment;
use crate::nudges::Why;

/// Quiet window before sending a non-urgent PR nudge (comments, check failures, conflicts),
/// giving a burst of CI failures or inline review comments time to coalesce into one nudge.
pub const COALESCE_MS: i64 = 60_000;

/// Maximum time a coalesced nudge waits from its first queued update so a continuously active PR
/// still delivers promptly.
pub const COALESCE_MAX_MS: i64 = 180_000;

/// Minimum `prasks.rs` probability for a plain comment to be treated as asking for work.
/// Matches `ASKS_AT` in `src/prStatus.ts`.
pub const ACTIONABLE_AT: f64 = 0.6;

/// Strips a trailing `[bot]` suffix and trims whitespace for case-insensitive bot login comparison.
fn norm_bot(name: &str) -> &str {
    let t = name.trim();
    t.strip_suffix("[bot]").or_else(|| t.strip_suffix("[BOT]")).unwrap_or(t)
}

/// Whether an author is allowed to trigger PR nudges under `allowlist` (`[pull_requests]
/// nudge_bots`). Humans (`!bot`) are always allowed. Bots are ignored unless `"*"` or their login
/// (with or without `[bot]`, case-insensitively) appears in `allowlist`.
pub fn author_allowed(author: &str, bot: bool, allowlist: &[String]) -> bool {
    if !bot {
        return true;
    }
    let target = norm_bot(author);
    allowlist.iter().any(|entry| {
        let e = entry.trim();
        e == "*" || (!e.is_empty() && norm_bot(e).eq_ignore_ascii_case(target))
    })
}

/// Whether `why` should wait out the short coalescing window before being sent. Approvals, merges,
/// closes, and watched PRs go right away (merges still obey `merged::MERGE_SETTLE_MS`).
pub fn needs_coalesce(why: &Why, watched: bool) -> bool {
    if watched || why.ended.is_some() || why.approved {
        return false;
    }
    why.since.is_some() || why.changes || !why.failed.is_empty() || why.conflicts
}

/// Deadline (ms) until which an owed nudge first seen at `first_at` and most recently updated at
/// `last_at` should wait to coalesce with further updates.
pub fn coalesce_until(first_at: i64, last_at: i64, window_ms: i64) -> i64 {
    if window_ms <= 0 || first_at <= 0 {
        return 0;
    }
    let cap = COALESCE_MAX_MS.max(window_ms);
    (last_at + window_ms).min(first_at + cap)
}

/// Comment IDs from `comments` that scored below `ACTIONABLE_AT` in `scores` and are not
/// `CHANGES_REQUESTED` reviews (which always ask for work). Missing scores fail open (kept).
pub fn non_actionable_ids(comments: &[&PrComment], scores: &HashMap<String, f64>) -> HashSet<String> {
    comments
        .iter()
        .filter(|c| c.review.as_deref() != Some("CHANGES_REQUESTED"))
        .filter(|c| scores.get(&c.id).is_some_and(|&p| p < ACTIONABLE_AT))
        .map(|c| c.id.clone())
        .collect()
}

/// Uses `[decisions] pull_requests` (`prasks.rs`) to identify fresh non-bot comments that don't
/// ask the author for a change or answer. Returns an empty set (fail-open) when `[decisions]
/// pull_requests` is off, no decider covers the project, or the model errors.
pub fn ask_non_actionable(cfg: &Config, project: Option<&str>, pr_title: &str, comments: &[&PrComment]) -> HashSet<String> {
    if !cfg.decisions.pull_requests || comments.is_empty() {
        return HashSet::new();
    }
    let Some(decider) = crate::decisions::for_project(cfg, project) else {
        return HashSet::new();
    };
    let inputs: Vec<crate::prasks::CommentIn> = comments
        .iter()
        .filter(|c| c.review.as_deref() != Some("CHANGES_REQUESTED"))
        .map(|c| crate::prasks::CommentIn { id: c.id.clone(), author: c.author.clone(), body: c.body.clone(), path: c.path.clone() })
        .collect();
    if inputs.is_empty() {
        return HashSet::new();
    }
    match crate::prasks::asks(&decider, pr_title, &inputs) {
        Ok(scores) => non_actionable_ids(comments, &scores),
        Err(_) => HashSet::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(id: &str, review: Option<&str>) -> PrComment {
        PrComment { id: id.into(), author: "ana".into(), bot: false, body: "hi".into(), at: Some(10), url: None, path: None, line: None, before: false, outdated: false, replies: 0, resolved: false, mine: false, review: review.map(String::from), in_review: None }
    }

    #[test]
    fn bot_filtering_ignores_bots_by_default_and_matches_allowlist() {
        assert!(author_allowed("ana", false, &[]));
        assert!(!author_allowed("vercel", true, &[]));
        assert!(!author_allowed("github-actions[bot]", true, &[]));
        assert!(author_allowed("github-actions[bot]", true, &["github-actions".into()]));
        assert!(author_allowed("github-actions", true, &["GitHub-Actions[bot]".into()]));
        assert!(!author_allowed("codecov", true, &["github-actions".into()]));
        assert!(author_allowed("anything", true, &["*".into()]));
    }

    #[test]
    fn coalesce_applies_to_feedback_and_checks_but_not_approvals_merges_or_watched() {
        assert!(needs_coalesce(&Why { since: Some(100), ..Why::default() }, false));
        assert!(needs_coalesce(&Why { failed: vec!["ci".into()], ..Why::default() }, false));
        assert!(needs_coalesce(&Why { conflicts: true, ..Why::default() }, false));
        assert!(!needs_coalesce(&Why { since: Some(100), ..Why::default() }, true));
        assert!(!needs_coalesce(&Why { approved: true, since: Some(100), ..Why::default() }, false));
        assert!(!needs_coalesce(&Why { ended: Some(crate::model::PrState::Merged), ..Why::default() }, false));
        assert_eq!(coalesce_until(1_000, 1_000, COALESCE_MS), 61_000);
        assert_eq!(coalesce_until(1_000, 50_000, COALESCE_MS), 110_000);
        assert_eq!(coalesce_until(1_000, 170_000, COALESCE_MS), 181_000, "capped at COALESCE_MAX_MS from first event");
    }

    #[test]
    fn non_actionable_drops_low_scoring_comments_but_never_change_requests_or_unscored() {
        let praise = c("c1", None);
        let ask = c("c2", None);
        let req = c("c3", Some("CHANGES_REQUESTED"));
        let unscored = c("c4", None);
        let scores = HashMap::from([("c1".into(), 0.15), ("c2".into(), 0.85), ("c3".into(), 0.10)]);
        let dropped = non_actionable_ids(&[&praise, &ask, &req, &unscored], &scores);
        assert_eq!(dropped, HashSet::from(["c1".to_string()]));
    }
}
