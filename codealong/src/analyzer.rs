use git2::{Commit, Repository};

use analyzed_commit::AnalyzedCommit;
use analyzed_diff::AnalyzedDiff;
use config::Config;
use error::Error;

pub trait Analyzer {
    fn config(&self) -> &Config;

    fn analyze_commit(&self, repo: &Repository, commit: &Commit) -> Result<AnalyzedCommit, Error> {
        let mut result = AnalyzedCommit::new(commit);
        let mut has_parents = false;
        for parent in commit.parents() {
            result.merge_diff(&self.analyze_diff(repo, commit, Some(&parent))?);
            has_parents = true;
        }
        // handle initial commit
        if !has_parents {
            result.merge_diff(&self.analyze_diff(repo, commit, None)?);
        }
        if let Some(ref github) = self.config().github {
            result.github_url = Some(format!(
                "https://github.com/{}/commit/{}",
                github, result.id
            ));
        }
        if let Some(ref repo_name) = self.config().repo_name {
            result.repo_name = Some(repo_name.clone());
        }
        return Ok(result);
    }

    fn analyze_diff(
        &self,
        repo: &Repository,
        commit: &Commit,
        parent: Option<&Commit>,
    ) -> Result<AnalyzedDiff, Error>;
}
