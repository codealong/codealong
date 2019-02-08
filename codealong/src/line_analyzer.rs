use git2::{Commit, DiffLine, Repository, Signature};

use crate::error::Error;
use crate::git_blame::GitBlame;
use crate::work_stats::WorkStats;

pub struct LineAnalyzer<'a> {
    repo: &'a Repository,
    commit: &'a Commit<'a>,
    diff_line: &'a DiffLine<'a>,
    blame: Option<&'a GitBlame>,
}

impl<'a> LineAnalyzer<'a> {
    pub fn new(
        repo: &'a Repository,
        commit: &'a Commit,
        diff_line: &'a DiffLine<'a>,
        blame: Option<&'a GitBlame>,
    ) -> LineAnalyzer<'a> {
        LineAnalyzer {
            repo,
            commit,
            diff_line,
            blame,
        }
    }

    pub fn analyze(&self) -> Result<WorkStats, Error> {
        match self.diff_line.origin() {
            '+' => Ok(WorkStats::new_work()),
            ' ' => self.analyze_change(),
            _ => Ok(WorkStats::other()),
        }
    }

    fn analyze_change(&self) -> Result<WorkStats, Error> {
        let blame = self.blame.expect("No blame found for change");
        if let Some(previous_commit_oid) =
            blame.get_line(self.diff_line.old_lineno().unwrap() as usize)?
        {
            let previous_commit = self.repo.find_commit(previous_commit_oid).unwrap();

            let diff_in_seconds = self.commit.committer().when().seconds()
                - previous_commit.committer().when().seconds();
            if diff_in_seconds < 60 * 60 * 24 * 7 * 3 {
                if self.compare_signatures(&previous_commit.author(), &self.commit.author()) {
                    return Ok(WorkStats::churn());
                } else {
                    return Ok(WorkStats::help_others());
                }
            }
        }
        return Ok(WorkStats::legacy_refactor());
    }

    // TODO: incorporate author config
    fn compare_signatures(&self, a: &Signature, b: &Signature) -> bool {
        if let Some(email_a) = a.email() {
            if let Some(email_b) = b.email() {
                return email_a == email_b;
            }
        }
        return false;
    }
}
