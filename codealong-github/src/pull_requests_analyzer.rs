use git2::Repository;
use slog::Logger;

use codealong::{AnalyzeOpts, Repo, RepoConfig, RepoInfo, WorkingConfig};

use crate::client::Client;
use crate::cursor::Cursor;
use crate::error::*;
use crate::pull_request::PullRequest;
use crate::pull_request_analyzer::PullRequestAnalyzer;

pub struct PullRequestsAnalyzer<'client> {
    repo: Repository,
    config: RepoConfig,
    working_config: WorkingConfig,
    client: &'client Client,
    logger: Logger,
}

impl<'client> PullRequestsAnalyzer<'client> {
    pub fn new(
        repo: Repository,
        config: RepoConfig,
        client: &'client Client,
        parent_logger: &Logger,
    ) -> PullRequestsAnalyzer<'client> {
        let working_config = WorkingConfig::new(config.config.clone());
        PullRequestsAnalyzer {
            repo,
            working_config,
            logger: parent_logger.new(o!("repo" => config.repo.name.to_owned())),
            config,
            client,
        }
    }

    pub fn analyze(
        &self,
        opts: AnalyzeOpts,
    ) -> Result<impl Iterator<Item = Result<PullRequestAnalyzer>>> {
        let cursor = self.build_cursor(opts.clone());
        Ok(PullRequestsCursor {
            repo: &self.repo,
            cursor,
            config: &self.working_config,
            repo_info: &self.config.repo,
            opts,
            logger: self.logger.clone(),
        })
    }

    pub fn guess_len(&self, opts: AnalyzeOpts) -> Result<usize> {
        if opts.since.is_some() {
            Ok(self.analyze(opts)?.count())
        } else {
            Ok(self
                .build_cursor(opts)
                .guess_len()
                .ok_or("error estimating count of pull requests")?)
        }
    }

    pub fn from_repo(repo: &Repo, client: &'client Client, logger: &Logger) -> Result<Self> {
        Ok(Self::new(repo.repository()?, repo.config(), client, logger))
    }

    fn build_cursor(&self, _opts: AnalyzeOpts) -> Cursor<PullRequest> {
        let url = format!(
            "https://api.github.com/repos/{}/pulls?state=all",
            self.config.repo.github_name.as_ref().unwrap()
        );
        Cursor::new(&self.client, &url, &self.logger)
    }
}

struct PullRequestsCursor<'client> {
    repo: &'client Repository,
    config: &'client WorkingConfig,
    repo_info: &'client RepoInfo,
    cursor: Cursor<'client, PullRequest>,
    opts: AnalyzeOpts,
    logger: Logger,
}

impl<'client> Iterator for PullRequestsCursor<'client> {
    type Item = Result<PullRequestAnalyzer<'client>>;

    fn next(&mut self) -> Option<Result<PullRequestAnalyzer<'client>>> {
        loop {
            let pr = self.cursor.next();
            match pr {
                None => break None,
                Some(pr) => {
                    if let Some(ref since) = self.opts.since {
                        if since > &pr.updated_at {
                            break None;
                        }
                    }

                    if !self.opts.ignore_unknown_authors
                        || self.config.is_github_login_known(&pr.user.login)
                    {
                        let analyzer = PullRequestAnalyzer::new(
                            &self.repo,
                            pr,
                            &self.config,
                            &self.repo_info,
                            &self.logger,
                        );
                        break Some(Ok(analyzer));
                    }
                }
            }
        }
    }
}
