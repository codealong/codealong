use git2::{Commit, Repository};

use analyzed_commit::AnalyzedCommit;
use analyzed_diff::AnalyzedDiff;
use error::Error;

pub trait Analyzer {
    fn analyze_commit(&self, repo: &Repository, commit: &Commit) -> Result<AnalyzedCommit, Error> {
        let mut result = AnalyzedCommit::new(commit);
        for parent in commit.parents() {
            result.merge_diff(&self.analyze_diff(repo, commit, &parent)?)
        }
        return Ok(result);
    }

    fn analyze_diff(
        &self,
        repo: &Repository,
        commit: &Commit,
        parent: &Commit,
    ) -> Result<AnalyzedDiff, Error>;
}
