extern crate chrono;
extern crate git2;
extern crate glob;
#[macro_use]
extern crate include_dir;
extern crate regex;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate serde_derive;
extern crate serde;
extern crate serde_yaml;

mod analyzed_commit;
mod analyzed_diff;
mod analyzer;
mod config;
mod default_analyzer;
mod error;
mod fast_blame;
mod work_stats;

use git2::{Repository, Revwalk};

pub use analyzed_commit::AnalyzedCommit;
pub use analyzer::Analyzer;
pub use config::Config;
pub use default_analyzer::DefaultAnalyzer;
pub use error::Error;

pub fn walk<'repo>(repo: &'repo Repository) -> AnalyzedRevwalk<'repo> {
    let mut revwalk = repo.revwalk().unwrap();
    revwalk.push_head().unwrap();
    AnalyzedRevwalk {
        analyzer: Box::new(DefaultAnalyzer::new()),
        repo,
        revwalk,
    }
}

pub struct AnalyzedRevwalk<'repo> {
    analyzer: Box<Analyzer>,
    repo: &'repo Repository,
    revwalk: Revwalk<'repo>,
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
                Some(self.analyzer.analyze_commit(self.repo, &commit))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_walk_simple_repo() {
        let repo = Repository::open(Path::new("./fixtures/repos/simple")).unwrap();
        let mut count = 0;
        for _result in walk(&repo) {
            count += 1;
        }
        assert!(count >= 5);
    }
}
