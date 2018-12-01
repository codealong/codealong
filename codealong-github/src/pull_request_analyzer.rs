use git2::{Oid, Repository};

use analyzed_pull_request::AnalyzedPullRequest;
use error::{Error, Result};
use pull_request::PullRequest;

use codealong::{Config, DiffAnalyzer};

pub struct PullRequestAnalyzer<'a> {
    repo: &'a Repository,
    config: &'a Config,
    pr: PullRequest,
}

impl<'a> PullRequestAnalyzer<'a> {
    pub fn new(
        repo: &'a Repository,
        pr: PullRequest,
        config: &'a Config,
    ) -> PullRequestAnalyzer<'a> {
        PullRequestAnalyzer { repo, pr, config }
    }

    pub fn analyze(self) -> Result<AnalyzedPullRequest> {
        if let Some(ref repo) = self.pr.base.repo {
            self.repo
                .remote_anonymous(&repo.html_url)
                .and_then(|mut base| base.fetch(&[&self.pr.base.reference], None, None))?
        }

        if let Some(ref repo) = self.pr.head.repo {
            self.repo
                .remote_anonymous(&repo.html_url)
                .and_then(|mut head| head.fetch(&[&self.pr.head.reference], None, None))?;
        }

        let diff = self
            .repo
            .find_commit(Oid::from_str(&self.pr.base.sha)?)
            .map_err::<Error, _>(|e| e.into())
            .and_then(|parent| {
                self.repo
                    .find_commit(Oid::from_str(&self.pr.head.sha)?)
                    .map_err::<Error, _>(|e| e.into())
                    .and_then(|commit| {
                        Ok(
                            DiffAnalyzer::new(&self.repo, &commit, Some(&parent), &self.config)
                                .analyze()?,
                        )
                    })
            })
            .ok();

        Ok(AnalyzedPullRequest::new(self.pr, diff))
    }
}
