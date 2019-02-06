use git2::{Repository, Revwalk};

use crate::analyze_opts::AnalyzeOpts;
use crate::commit_analyzer::CommitAnalyzer;
use crate::error::*;
use crate::identity::Identity;
use crate::repo::Repo;
use crate::repo_config::RepoConfig;
use crate::slog::Logger;
use crate::utils::convert_time;

pub struct RepoAnalyzer {
    repo: Repository,
    config: RepoConfig,
    logger: Logger,
}

impl RepoAnalyzer {
    pub fn new(repo: Repository, config: RepoConfig, parent_logger: &Logger) -> RepoAnalyzer {
        RepoAnalyzer {
            repo,
            logger: parent_logger.new(o!("repo" => config.repo.name.to_owned())),
            config,
        }
    }

    pub fn analyze(
        &self,
        opts: AnalyzeOpts,
    ) -> Result<impl Iterator<Item = Result<CommitAnalyzer>>> {
        let mut revwalk = self.repo.revwalk()?;
        for reference in &self.config.repo.refs {
            if let Ok(_) = self.repo.find_reference(reference) {
                revwalk.push_ref(&reference)?;
            } else {
                warn!(
                    self.logger,
                    "Could not find reference: {}, using HEAD", reference
                );
                revwalk.push_head()?;
            }
        }
        Ok(AnalyzedRevwalk {
            repo: &self.repo,
            revwalk,
            config: &self.config,
            opts,
            logger: self.logger.clone(),
        })
    }

    pub fn guess_len(&self, opts: AnalyzeOpts) -> Result<usize> {
        Ok(self.analyze(opts)?.count())
    }

    pub fn from_repo(repo: &Repo, logger: &Logger) -> Result<Self> {
        Ok(Self::new(repo.repository()?, repo.config(), logger))
    }
}

pub struct AnalyzedRevwalk<'repo> {
    repo: &'repo Repository,
    revwalk: Revwalk<'repo>,
    config: &'repo RepoConfig,
    opts: AnalyzeOpts,
    logger: Logger,
}

impl<'repo> Iterator for AnalyzedRevwalk<'repo> {
    type Item = Result<CommitAnalyzer<'repo>>;

    fn next(&mut self) -> Option<Result<CommitAnalyzer<'repo>>> {
        loop {
            let rev = self.revwalk.next();
            match rev {
                None => break None,
                Some(rev) => {
                    let oid = rev.unwrap();
                    let commit = self.repo.find_commit(oid).unwrap();

                    if let Some(ref since) = self.opts.since {
                        let commit_time = convert_time(&commit.author().when());
                        if since > &commit_time {
                            continue;
                        }
                    }

                    if !self.opts.ignore_unknown_authors
                        || self
                            .config
                            .config
                            .is_known(&Identity::from(commit.author()))
                    {
                        let analyzer =
                            CommitAnalyzer::new(self.repo, commit, self.config, &self.logger);
                        break Some(Ok(analyzer));
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo_info::RepoInfo;
    use crate::test::build_test_logger;

    #[test]
    fn test_analyze() -> Result<()> {
        let repo = Repository::open("./fixtures/repos/simple")?;
        let config = RepoConfig {
            repo: RepoInfo {
                refs: vec!["refs/head".to_owned()],
                ..RepoInfo::default()
            },
            ..RepoConfig::default()
        };
        let opts = AnalyzeOpts {
            ignore_unknown_authors: false,
            since: None,
        };
        let analyzer = RepoAnalyzer::new(repo, config, &build_test_logger());
        assert!(analyzer.analyze(opts)?.count() > 4);
        Ok(())
    }

    #[test]
    fn test_ignore_unknown_authors() -> Result<()> {
        let repo = Repository::open("./fixtures/repos/simple")?;
        let config = RepoConfig {
            repo: RepoInfo {
                refs: vec!["refs/head".to_owned()],
                ..RepoInfo::default()
            },
            ..RepoConfig::default()
        };
        let opts = AnalyzeOpts {
            ignore_unknown_authors: true,
            since: None,
        };
        let analyzer = RepoAnalyzer::new(repo, config, &build_test_logger());
        assert_eq!(analyzer.analyze(opts)?.count(), 0);
        Ok(())
    }
}
