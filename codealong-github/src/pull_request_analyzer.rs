use git2::{Oid, Repository};
use slog::Logger;

use codealong::{with_authentication, DiffAnalyzer, RepoInfo, WorkingConfig};

use crate::analyzed_pull_request::AnalyzedPullRequest;
use crate::error::{Error, Result};
use crate::pull_request::{PullRequest, Ref};

pub struct PullRequestAnalyzer<'a> {
    repo: &'a Repository,
    config: &'a WorkingConfig,
    pr: PullRequest,
    logger: Logger,
}

impl<'a> PullRequestAnalyzer<'a> {
    pub fn new(
        repo: &'a Repository,
        pr: PullRequest,
        config: &'a WorkingConfig,
        _repo_info: &'a RepoInfo,
        parent_logger: &Logger,
    ) -> PullRequestAnalyzer<'a> {
        let logger = parent_logger.new(o!("pull_request_id" => pr.id));
        PullRequestAnalyzer {
            repo,
            pr,
            config,
            logger,
        }
    }

    pub fn analyze(self) -> Result<AnalyzedPullRequest> {
        debug!(self.logger, "Analyzing pull_request"; "updated_at" => &self.pr.updated_at.to_rfc2822(), "user" => &self.pr.user.login, "title" => &self.pr.title);
        self.fetch_remote(&self.pr.base)?;
        self.fetch_remote(&self.pr.head)?;

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

        let normalized_author = self.config.person_for_github_login(&self.pr.user.login);
        debug!(self.logger, "Done analyzing");
        Ok(AnalyzedPullRequest::new(self.pr, diff, normalized_author))
    }

    pub fn is_author_known(&self) -> bool {
        self.config.is_github_login_known(&self.pr.user.login)
    }

    fn fetch_remote(&self, reference: &Ref) -> Result<()> {
        if let Some(ref repo) = reference.repo {
            let git_config = git2::Config::open_default()?;
            let url = &repo.ssh_url;
            with_authentication(url, &git_config, |f| {
                let mut rcb = git2::RemoteCallbacks::new();
                rcb.credentials(f);
                let mut fo = git2::FetchOptions::new();
                fo.remote_callbacks(rcb);

                Ok(self.repo.remote_anonymous(url).and_then(|mut base| {
                    base.fetch(&[&reference.reference], Some(&mut fo), None)
                })?)
            })?;
        }
        Ok(())
    }
}
