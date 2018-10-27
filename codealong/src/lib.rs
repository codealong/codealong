extern crate chrono;
extern crate git2;
extern crate glob;
#[macro_use]
extern crate include_dir;
extern crate regex;
#[macro_use]
extern crate lazy_static;
extern crate linked_hash_map;
#[macro_use]
extern crate serde_derive;
extern crate serde;
extern crate serde_yaml;

mod analyzed_commit;
mod analyzed_diff;
mod commit_analyzer;
mod config;
mod config_context;
mod diff_analyzer;
mod error;
mod fast_blame;
mod file_analyzer;
mod hunk_analyzer;
mod line_analyzer;
mod work_stats;

use git2::{Repository, Revwalk};

pub use analyzed_commit::AnalyzedCommit;
pub use commit_analyzer::CommitAnalyzer;
pub use config::Config;
pub use error::Error;

pub fn walk<'repo>(repo: &'repo Repository) -> AnalyzedRevwalk<'repo> {
    let mut revwalk = repo.revwalk().unwrap();
    revwalk.push_head().unwrap();
    let config = Config::base();
    AnalyzedRevwalk {
        repo,
        revwalk,
        config,
    }
}

pub struct AnalyzedRevwalk<'repo> {
    repo: &'repo Repository,
    revwalk: Revwalk<'repo>,
    config: Config,
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
