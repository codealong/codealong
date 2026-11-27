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

/// Prompt for a blind pairwise judge running in an isolated folder containing `spec.md`,
/// `A.diff`, `B.diff`, optionally `repo/` (a clean read-only snapshot of the codebase at
/// `base_sha` with no `.git` metadata), and optionally `gate.txt`.
pub fn judge_prompt(has_gate: bool, has_repo: bool, rubric: Option<&str>) -> String {
    let repo_line = if has_repo {
        "- `repo/`: read-only snapshot of the repository at the base commit (no git history) — inspect surrounding code, existing helpers, and repo rules (e.g. `AGENTS.md` / `CLAUDE.md`) to judge codebase fit\n"
    } else {
        ""
    };
    let gate_line = if has_gate {
        "- `gate.txt`: mechanical test/check gate summary for both candidates\n"
    } else {
        ""
    };
    let rubric_block = match rubric.map(str::trim).filter(|s| !s.is_empty()) {
        Some(r) => format!("\nAdditional evaluation criteria:\n{r}\n"),
        None => String::new(),
    };
    format!(
        "You are a blind code judge comparing two anonymous implementations (`Candidate A` and `Candidate B`) for the same task.\n\n\
        Read the files in this folder:\n\
        - `spec.md`: the task specification\n\
        - `A.diff`: Candidate A's complete diff from the base commit\n\
        - `B.diff`: Candidate B's complete diff from the base commit\n\
        {repo_line}{gate_line}{rubric_block}\n\
        Evaluate both candidates on correctness, completeness against `spec.md`, code quality, codebase fit (reusing existing helpers and following repo conventions), simplicity, and test coverage.\n\
        Do not try to infer which model or tool wrote either diff, and do not modify any files.\n\n\
        Give a concise comparison citing concrete files/hunks from `A.diff` and `B.diff`, then end your reply with a single final line in this exact format:\n\
        `Winner: A` or `Winner: B` or `Winner: tie`"
    )
}

/// Prompt for a blind all-at-once judge comparing 2+ anonymous candidates (`A.diff`, `B.diff`,
/// `C.diff`, ...) together against `spec.md` and `repo/` (base-commit read-only snapshot).
pub fn rank_judge_prompt(labels: &[char], has_gate: bool, has_repo: bool, rubric: Option<&str>) -> String {
    let diff_lines: String = labels
        .iter()
        .map(|l| format!("- `{l}.diff`: Candidate {l}'s complete diff from the base commit\n"))
        .collect();
    let repo_line = if has_repo {
        "- `repo/`: read-only snapshot of the repository at the base commit (no git history) — inspect surrounding code, existing helpers, and repo rules (e.g. `AGENTS.md` / `CLAUDE.md`) to judge codebase fit\n"
    } else {
        ""
    };
    let gate_line = if has_gate {
        "- `gate.txt`: mechanical test/check gate summary for every candidate\n"
    } else {
        ""
    };
    let rubric_block = match rubric.map(str::trim).filter(|s| !s.is_empty()) {
        Some(r) => format!("\nAdditional evaluation criteria:\n{r}\n"),
        None => String::new(),
    };
    let example: String = labels.iter().map(|c| c.to_string()).collect::<Vec<_>>().join(" > ");
    format!(
        "You are a blind code judge comparing {} anonymous implementations ({}) for the same task.\n\n\
        Read the files in this folder:\n\
        - `spec.md`: the task specification\n\
        {diff_lines}{repo_line}{gate_line}{rubric_block}\n\
        Evaluate all candidates on correctness, completeness against `spec.md`, code quality, codebase fit (reusing existing helpers and following repo conventions), simplicity, and test coverage.\n\
        Do not try to infer which model or tool wrote any diff, and do not modify any files.\n\n\
        Give a concise comparative review citing concrete files/hunks from each diff, then end your reply with a single final line ranking EVERY candidate from best to worst using `>` (strictly better) or `=` (genuine tie), e.g.:\n\
        `Ranking: {example}`",
        labels.len(),
        labels.iter().map(|l| format!("`Candidate {l}`")).collect::<Vec<_>>().join(", "),
    )
}

/// Parses a judge's final `Ranking: B > A = C` line into ordered tiers of candidate letters.
/// Returns `None` unless every label in `expected` appears exactly once.
pub fn parse_ranking(reply: &str, expected: &[char]) -> Option<Vec<Vec<char>>> {
    if expected.is_empty() {
        return None;
    }
    let expected_upper: std::collections::HashSet<char> = expected.iter().map(|c| c.to_ascii_uppercase()).collect();
    for line in reply.lines().rev() {
        let trimmed = line.trim().trim_matches(|c: char| c == '`' || c == '*' || c == '_');
        let lower = trimmed.to_ascii_lowercase();
        let Some(rest) = lower.strip_prefix("ranking:") else { continue };
        let body = rest.trim().trim_matches(|c: char| c == '`' || c == '*' || c == '_' || c == '.');
        let mut tiers: Vec<Vec<char>> = Vec::new();
        let mut seen: std::collections::HashSet<char> = std::collections::HashSet::new();
        let mut ok = true;
        for tier_str in body.split('>') {
            let mut tier = Vec::new();
            for item in tier_str.split('=') {
                let cleaned = item
                    .trim()
                    .trim_matches(|c: char| matches!(c, '`' | '*' | '_' | '.' | ',' | '(' | ')'))
                    .trim_start_matches("candidate")
                    .trim_start_matches(['-', ':', ' ']);
                let mut chars = cleaned.chars();
                let (Some(ch), None) = (chars.next(), chars.next()) else {
                    ok = false;
                    break;
                };
                let up = ch.to_ascii_uppercase();
                if !expected_upper.contains(&up) || !seen.insert(up) {
                    ok = false;
                    break;
                }
                tier.push(up);
            }
            if !ok || tier.is_empty() {
                ok = false;
                break;
            }
            tiers.push(tier);
        }
        if ok && seen.len() == expected_upper.len() {
            return Some(tiers);
        }
    }
    None
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

    #[test]
    fn judge_prompt_includes_repo_context_and_optional_rubric() {
        let plain = judge_prompt(false, true, None);
        assert!(plain.contains("`repo/`"), "{plain}");
        assert!(!plain.contains("`gate.txt`"), "{plain}");
        assert!(!plain.contains("Additional evaluation criteria"), "{plain}");

        let with_rubric = judge_prompt(true, true, Some("Prefer minimal diffs over new abstractions."));
        assert!(with_rubric.contains("`repo/`"), "{with_rubric}");
        assert!(with_rubric.contains("`gate.txt`"), "{with_rubric}");
        assert!(with_rubric.contains("Additional evaluation criteria:\nPrefer minimal diffs over new abstractions."), "{with_rubric}");
    }

    #[test]
    fn rank_judge_prompt_and_parse_ranking_handle_ties_and_reject_incomplete_lines() {
        let prompt = rank_judge_prompt(&['A', 'B', 'C'], false, true, Some("Reuse helpers."));
        assert!(prompt.contains("`A.diff`") && prompt.contains("`B.diff`") && prompt.contains("`C.diff`"), "{prompt}");
        assert!(prompt.contains("`repo/`") && prompt.contains("Reuse helpers."), "{prompt}");

        assert_eq!(parse_ranking("Analysis...\n\nRanking: B > A > C", &['A', 'B', 'C']), Some(vec![vec!['B'], vec!['A'], vec!['C']]));
        assert_eq!(parse_ranking("**Ranking: C > A = B.**\n", &['A', 'B', 'C']), Some(vec![vec!['C'], vec!['A', 'B']]));
        assert_eq!(
            parse_ranking("`Ranking: Candidate A = Candidate B > Candidate C`", &['A', 'B', 'C']),
            Some(vec![vec!['A', 'B'], vec!['C']])
        );
        // Rejects missing or duplicate labels.
        assert_eq!(parse_ranking("Ranking: A > B", &['A', 'B', 'C']), None);
        assert_eq!(parse_ranking("Ranking: A > A > B", &['A', 'B', 'C']), None);
    }
}
