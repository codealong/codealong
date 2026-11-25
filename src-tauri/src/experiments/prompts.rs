//! Prompt builders and reply parsers for experiment workers, local reviewers and blind judges.
//!
//! Nothing here assumes any specific repository or workflow script.

use std::path::Path;

/// Wraps the user's task prompt with unattended trial guardrails.
pub fn worker_prompt(task: &str) -> String {
    format!(
        "{}\n\n---\n\
        Experiment trial instructions:\n\
        - Work only inside this worktree.\n\
        - Run the repository's relevant checks and tests before finishing.\n\
        - Commit your changes locally on this branch when done.\n\
        - Do NOT push, open a pull request, merge, or land your changes.\n\
        - Do NOT stop to ask clarifying questions; if a small detail is ambiguous, make the best judgment consistent with the codebase and note your assumption in your final reply.",
        task.trim()
    )
}

/// Prompt for a 1-round read-only local reviewer examining a worker's diff.
pub fn reviewer_prompt(spec_path: &Path, diff_path: &Path) -> String {
    format!(
        "Review the implementation in this worktree against the task specification.\n\n\
        - Task specification: `{}`\n\
        - Numbered diff from base: `{}`\n\n\
        Rules:\n\
        - Read-only review: do NOT modify, create, or delete any files, and do NOT run git commit or git push.\n\
        - Focus on correctness, missing edge cases, regressions, and adherence to the specification.\n\
        - Cite concrete file paths and line numbers for every finding.\n\
        - End your reply with a final standalone line:\n\
          `Verdict: approve` (if solid with no material issues) or\n\
          `Verdict: request changes` (if issues should be fixed before acceptance).",
        spec_path.display(),
        diff_path.display()
    )
}

/// Prompt handed back to the worker when its reviewer requested changes.
pub fn fix_prompt(review_path: &Path) -> String {
    format!(
        "An independent reviewer examined your diff and requested changes. Read their review at `{}`.\n\n\
        Weigh each finding against the code and specification yourself: fix every valid issue, re-run the relevant tests/checks, and commit your updates locally on this branch.\n\
        Do NOT push, open a pull request, or land your changes.",
        review_path.display()
    )
}

/// Prompt for a blind pairwise judge running in an isolated folder containing ONLY `spec.md`,
/// `A.diff`, `B.diff`, and optionally `gate.txt`.
pub fn judge_prompt(has_gate: bool) -> String {
    let gate_line = if has_gate {
        "- `gate.txt`: mechanical test/check gate summary for both candidates\n"
    } else {
        ""
    };
    format!(
        "You are a blind code judge comparing two anonymous implementations (`Candidate A` and `Candidate B`) for the same task.\n\n\
        Read the files in this folder:\n\
        - `spec.md`: the task specification\n\
        - `A.diff`: Candidate A's complete diff from the base commit\n\
        - `B.diff`: Candidate B's complete diff from the base commit\n\
        {gate_line}\n\
        Evaluate both candidates purely on correctness, completeness against `spec.md`, code quality, simplicity, and test coverage.\n\
        Do not try to infer which model or tool wrote either diff, and do not modify any files.\n\n\
        Give a concise comparison citing concrete files/hunks from `A.diff` and `B.diff`, then end your reply with a single final line in this exact format:\n\
        `Winner: A` or `Winner: B` or `Winner: tie`"
    )
}

/// Parses the reviewer's final `Verdict:` line, accepting `request changes`, `request-changes`,
/// and `request_changes` (underscores aren't normalized by `autoreview::verdict_of`).
pub fn parse_review_verdict(reply: &str) -> Option<crate::github::Verdict> {
    let normalized: String = reply
        .lines()
        .map(|l| if l.to_ascii_lowercase().contains("verdict:") { l.replace('_', "-") } else { l.to_string() })
        .collect::<Vec<_>>()
        .join("\n");
    crate::autoreview::verdict_of(&normalized)
}

/// Parses the judge's final `Winner: A|B|tie` line (matching `autoreview::verdict_of`'s style).
pub fn parse_winner(reply: &str) -> Option<&'static str> {
    for line in reply.lines().rev() {
        let trimmed = line.trim().trim_matches(|c: char| c == '`' || c == '*' || c == '_');
        let lower = trimmed.to_ascii_lowercase();
        let Some(rest) = lower.strip_prefix("winner:") else { continue };
        let token = rest.trim().trim_matches(|c: char| c == '`' || c == '*' || c == '_' || c == '.');
        let first = token.split_whitespace().next().unwrap_or("");
        return match first {
            "a" | "candidate-a" => Some("A"),
            "b" | "candidate-b" => Some("B"),
            "tie" | "draw" | "neither" => Some("tie"),
            "candidate" => match token.split_whitespace().nth(1) {
                Some("a") => Some("A"),
                Some("b") => Some("B"),
                _ => None,
            },
            _ => None,
        };
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_winner_reads_last_winner_line() {
        assert_eq!(parse_winner("A is cleaner.\n\nWinner: A"), Some("A"));
        assert_eq!(parse_winner("Thoughts...\n**Winner: B.**\n"), Some("B"));
        assert_eq!(parse_winner("Close call.\n`Winner: tie`"), Some("tie"));
        assert_eq!(parse_winner("Winner: Candidate B"), Some("B"));
        assert_eq!(parse_winner("No verdict line here."), None);
    }

    #[test]
    fn parse_review_verdict_accepts_underscores_hyphens_and_spaces() {
        use crate::github::Verdict;
        assert_eq!(parse_review_verdict("Looks solid.\n\nVerdict: approve"), Some(Verdict::Approve));
        assert_eq!(parse_review_verdict("Two bugs.\n\nVerdict: request_changes"), Some(Verdict::RequestChanges));
        assert_eq!(parse_review_verdict("Two bugs.\n\nVerdict: request-changes"), Some(Verdict::RequestChanges));
        assert_eq!(parse_review_verdict("Two bugs.\n\nVerdict: request changes"), Some(Verdict::RequestChanges));
        assert_eq!(parse_review_verdict("Minor nit.\n\nVerdict: comment"), Some(Verdict::Comment));
        assert_eq!(parse_review_verdict("No verdict line."), None);
    }

    #[test]
    fn worker_prompt_is_repo_agnostic() {
        let p = worker_prompt("Fix the parser.");
        assert!(!p.contains("scripts/land.sh"));
        assert!(p.contains("Do NOT push"));
    }
}
