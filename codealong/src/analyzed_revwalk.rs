use crate::analyze_opts::AnalyzeOpts;
use crate::commit_analyzer::CommitAnalyzer;
use crate::config::Config;
use crate::error::Result;
use crate::identity::Identity;
use crate::slog::Logger;

use git2::{Repository, Revwalk};

pub struct AnalyzedRevwalk<'repo> {
    repo: &'repo Repository,
    revwalk: Revwalk<'repo>,
    config: &'repo Config,
    opts: AnalyzeOpts,
    logger: Logger,
}

impl<'repo> AnalyzedRevwalk<'repo> {
    pub fn new(
        repo: &'repo Repository,
        config: &'repo Config,
        opts: AnalyzeOpts,
        parent_logger: &Logger,
    ) -> Result<AnalyzedRevwalk<'repo>> {
        let mut revwalk = repo.revwalk()?;
        if let Ok(remote) = repo.find_remote("origin") {
            revwalk.push_ref("refs/remotes/origin/master")?;
        } else {
            revwalk.push_head()?;
        }
        Ok(AnalyzedRevwalk {
            repo,
            config,
            revwalk,
            opts,
            logger: parent_logger.clone(),
        })
    }
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
                    if !self.opts.ignore_unknown_authors
                        || self.config.is_known(&Identity::from(commit.author()))
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

impl<'repo> ExactSizeIterator for AnalyzedRevwalk<'repo> {
    fn len(&self) -> usize {
        let mut revwalk = self.repo.revwalk().unwrap();
        revwalk.push_head().unwrap();
        revwalk.count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test::build_test_logger;

    #[test]
    fn test_walk() -> Result<()> {
        let repo = Repository::open("./fixtures/repos/simple")?;
        let config = Config::default();
        let opts = AnalyzeOpts {
            ignore_unknown_authors: false,
        };
        let revwalk = AnalyzedRevwalk::new(&repo, &config, opts, &build_test_logger())?;
        assert!(revwalk.count() > 4);
        Ok(())
    }

    #[test]
    fn test_ignore_unknown_authors() -> Result<()> {
        let repo = Repository::open("./fixtures/repos/simple")?;
        let config = Config::default();
        let opts = AnalyzeOpts {
            ignore_unknown_authors: true,
        };
        let revwalk = AnalyzedRevwalk::new(&repo, &config, opts, &build_test_logger())?;
        assert_eq!(revwalk.count(), 0);
        Ok(())
    }
}
