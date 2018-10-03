extern crate chrono;
extern crate git2;

mod analyzed_commit;
mod analyzed_diff;
mod analyzer;
mod default_analyzer;
mod error;
mod work_stats;

use git2::{Repository, Revwalk};

pub use analyzed_commit::AnalyzedCommit;
pub use analyzer::Analyzer;
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
    fn simple_repo() {
        let repo = Repository::open(Path::new("./fixtures/simple")).unwrap();
        for result in walk(&repo) {
            println!("analyzed: {:#?}", result);
        }
    }
}
