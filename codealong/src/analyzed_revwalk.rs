use crate::analyze_opts::AnalyzeOpts;
use crate::analyzed_commit::AnalyzedCommit;
use crate::commit_analyzer::CommitAnalyzer;
use crate::config::Config;
use crate::error::Result;
use crate::identity::Identity;

use git2::{Repository, Revwalk};

pub struct AnalyzedRevwalk<'repo> {
    repo: &'repo Repository,
    revwalk: Revwalk<'repo>,
    config: Config,
    opts: AnalyzeOpts,
}

impl<'repo> AnalyzedRevwalk<'repo> {
    pub fn new(
        repo: &'repo Repository,
        config: Config,
        opts: AnalyzeOpts,
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
        })
    }
}

impl<'repo> Iterator for AnalyzedRevwalk<'repo> {
    type Item = Result<AnalyzedCommit>;

    fn next(&mut self) -> Option<Result<AnalyzedCommit>> {
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
                        let analyzer = CommitAnalyzer::new(self.repo, &commit, &self.config);
                        break Some(analyzer.analyze());
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

    #[test]
    fn test_walk() -> Result<()> {
        let repo = Repository::open("./fixtures/repos/simple")?;
        let config = Config::default();
        let opts = AnalyzeOpts {
            ignore_unknown_authors: false,
        };
        let revwalk = AnalyzedRevwalk::new(&repo, config, opts)?;
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
        let revwalk = AnalyzedRevwalk::new(&repo, config, opts)?;
        assert_eq!(revwalk.count(), 0);
        Ok(())
    }
}
