use git2::{Commit, DiffDelta, DiffLine, DiffOptions, Repository, Signature};

use std::collections::HashMap;

use analyzed_diff::AnalyzedDiff;
use analyzer::Analyzer;
use error::Error;
use fast_blame::FastBlame;
use work_stats::WorkStats;

use std::cell::RefCell;

pub struct DefaultAnalyzer {}

impl Analyzer for DefaultAnalyzer {
    fn analyze_diff(
        &self,
        repo: &Repository,
        commit: &Commit,
        parent: Option<&Commit>,
    ) -> Result<AnalyzedDiff, Error> {
        let mut diff_opts = DiffOptions::new();
        diff_opts.ignore_whitespace(true);
        let diff =
            repo.diff_tree_to_tree(
                parent.map(|p| p.tree().unwrap()).as_ref(),
                Some(&commit.tree()?),
                Some(&mut diff_opts),
            ).unwrap();
        let mut stats = WorkStats::empty();
        let blame: RefCell<Option<FastBlame>> = RefCell::new(None);
        diff.foreach(
            &mut |diff_delta, _| {
                if let Some(old_path) = diff_delta.old_file().path() {
                    if let Some(parent) = parent {
                        if let Ok(new_blame) = FastBlame::new(&repo, &parent.id(), &old_path) {
                            blame.replace(Some(new_blame));
                        } else {
                            blame.replace(None);
                        }
                    }
                }
                true
            },
            None,
            None,
            Some(&mut |diff_delta, _, diff_line| {
                let analyzed_delta =
                    self.analyze_line_diff(
                        &repo,
                        &commit,
                        parent,
                        &diff_delta,
                        &diff_line,
                        blame.borrow().as_ref(),
                    ).unwrap();
                stats += analyzed_delta;
                true
            }),
        )?;
        let analyzed_diff = AnalyzedDiff {
            tag_stats: HashMap::new(),
            stats,
        };
        Ok(analyzed_diff)
    }
}

impl DefaultAnalyzer {
    pub fn new() -> DefaultAnalyzer {
        DefaultAnalyzer {}
    }

    fn analyze_line_diff(
        &self,
        repo: &Repository,
        commit: &Commit,
        parent: Option<&Commit>,
        diff_delta: &DiffDelta,
        diff_line: &DiffLine,
        blame: Option<&FastBlame>,
    ) -> Result<WorkStats, Error> {
        match diff_line.origin() {
            '+' => Ok(WorkStats::new_work()),
            ' ' => self.analyze_line_change(
                repo,
                commit,
                parent,
                diff_delta,
                diff_line,
                blame.unwrap(),
            ),
            _ => Ok(WorkStats::other()),
        }
    }

    fn analyze_line_change(
        &self,
        repo: &Repository,
        commit: &Commit,
        _parent: Option<&Commit>,
        _diff_delta: &DiffDelta,
        diff_line: &DiffLine,
        blame: &FastBlame,
    ) -> Result<WorkStats, Error> {
        if let Some(previous_commit_oid) = blame.get_line(diff_line.old_lineno().unwrap() as usize)
        {
            let previous_commit = repo.find_commit(previous_commit_oid).unwrap();

            let diff_in_seconds =
                commit.committer().when().seconds() - previous_commit.committer().when().seconds();
            if diff_in_seconds < 60 * 60 * 24 * 7 * 3 {
                if self.compare_signatures(&previous_commit.author(), &commit.author()) {
                    return Ok(WorkStats::churn());
                } else {
                    return Ok(WorkStats::help_others());
                }
            }
        }
        return Ok(WorkStats::legacy_refactor());
    }

    fn compare_signatures(&self, a: &Signature, b: &Signature) -> bool {
        if let Some(email_a) = a.email() {
            if let Some(email_b) = b.email() {
                return email_a == email_b;
            }
        }
        return false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use git2::Oid;

    fn analyze_against_parent(repo_path: &str, commit_id: &str) -> Result<AnalyzedDiff, Error> {
        let repo = Repository::open(repo_path).unwrap();
        let commit = repo.find_commit(Oid::from_str(commit_id).unwrap()).unwrap();
        let parent = match commit.parent(0) {
            Ok(parent) => Some(parent),
            Err(_) => None,
        };
        let analyzer = DefaultAnalyzer::new();
        analyzer.analyze_diff(&repo, &commit, parent.as_ref())
    }

    #[test]
    fn it_works_on_initial_commit() {
        let res = analyze_against_parent(
            "./fixtures/simple",
            "86d242301830075e93ff039a4d1e88673a4a3020",
        ).unwrap();
        assert!(res.stats.new_work == 1);
    }
}
