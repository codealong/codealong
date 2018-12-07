use crate::analyzed_commit::AnalyzedCommit;
use crate::commit_analyzer::CommitAnalyzer;
use crate::config::Config;
use crate::error::Error;

use git2::{Repository, Revwalk};

pub struct AnalyzedRevwalk<'repo> {
    repo: &'repo Repository,
    revwalk: Revwalk<'repo>,
    config: Config,
}

impl<'repo> AnalyzedRevwalk<'repo> {
    pub fn new(repo: &'repo Repository, config: Config) -> Result<AnalyzedRevwalk<'repo>, Error> {
        let mut revwalk = repo.revwalk()?;
        revwalk.push_head()?;
        Ok(AnalyzedRevwalk {
            repo,
            config,
            revwalk,
        })
    }
}

impl<'repo> Iterator for AnalyzedRevwalk<'repo> {
    type Item = Result<AnalyzedCommit, Error>;

    fn next(&mut self) -> Option<Result<AnalyzedCommit, Error>> {
        let rev = self.revwalk.next();
        match rev {
            None => None,
            Some(rev) => {
                let oid = rev.unwrap();
                let commit = self.repo.find_commit(oid).unwrap();
                let analyzer = CommitAnalyzer::new(self.repo, &commit, &self.config);
                Some(analyzer.analyze())
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
