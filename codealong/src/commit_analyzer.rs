use git2::{Commit, Repository};
use slog::Logger;

use crate::analyzed_commit::AnalyzedCommit;
use crate::config::Config;
use crate::diff_analyzer::DiffAnalyzer;
use crate::error::Error;

pub struct CommitAnalyzer<'a> {
    repo: &'a Repository,
    commit: Commit<'a>,
    config: &'a Config,
    logger: Logger,
}

impl<'a> CommitAnalyzer<'a> {
    pub fn new(
        repo: &'a Repository,
        commit: Commit<'a>,
        config: &'a Config,
        parent_logger: &Logger,
    ) -> CommitAnalyzer<'a> {
        let logger = parent_logger.new(o!("commit_id" => commit.id().to_string()));
        CommitAnalyzer {
            repo,
            commit,
            config,
            logger,
        }
    }

    pub fn analyze(&self) -> Result<AnalyzedCommit, Error> {
        let mut result = AnalyzedCommit::new(&self.commit);
        info!(self.logger, "Analyzing commit"; "commit_time" => &result.authored_at.to_rfc2822(), "commit_author" => &result.author.to_string(), "commit_summary" => &result.summary);
        // TODO: deal with merge commits
        let mut has_parents = false;
        for parent in self.commit.parents() {
            let diff_analyzer =
                DiffAnalyzer::new(self.repo, &self.commit, Some(&parent), self.config);
            result.merge_diff(&diff_analyzer.analyze()?);
            has_parents = true;
        }
        // handle initial commit
        if !has_parents {
            let diff_analyzer = DiffAnalyzer::new(self.repo, &self.commit, None, self.config);
            result.merge_diff(&diff_analyzer.analyze()?);
        }
        if let Some(ref github_name) = self.config.repo.github_name {
            result.github_url = Some(format!(
                "https://github.com/{}/commit/{}",
                github_name, result.id
            ));
        }
        result.repo = Some(self.config.repo.partial());
        result.normalized_author = Some(self.config.person_for_identity(&result.author));
        result.normalized_committer = Some(self.config.person_for_identity(&result.committer));
        info!(self.logger, "Done analyzing");
        return Ok(result);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test::build_test_logger;
    use crate::work_stats::WorkStats;
    use git2::Oid;
    use std::path::Path;

    #[test]
    fn test_initial_commit() {
        let repo = Repository::open("./fixtures/repos/simple").unwrap();
        let commit = repo
            .find_commit(Oid::from_str("86d242301830075e93ff039a4d1e88673a4a3020").unwrap())
            .unwrap();
        let config = Config::default();
        let analyzer = CommitAnalyzer::new(&repo, commit, &config, &build_test_logger());
        let res = analyzer.analyze().unwrap();
        assert_eq!(res.diff.stats.new_work, 1);
    }

    #[test]
    fn test_merge_commit() {
        let repo = Repository::open("./fixtures/repos/simple").unwrap();
        let commit = repo
            .find_commit(Oid::from_str("301dfdc07a8c0770d3a352b6f6c2d8ff8159a9e3").unwrap())
            .unwrap();
        let config = Config::default();
        let analyzer = CommitAnalyzer::new(&repo, commit, &config, &build_test_logger());
        let res = analyzer.analyze().unwrap();
        assert_eq!(
            res.diff.stats,
            // XXX: tweak this
            WorkStats {
                new_work: 8,
                legacy_refactor: 7,
                churn: 1,
                help_others: 0,
                other: 0,
                impact: 8
            }
        );
    }

    #[test]
    fn test_with_config() {
        let repo = Repository::open("./fixtures/repos/simple").unwrap();
        let commit = repo
            .find_commit(Oid::from_str("86d242301830075e93ff039a4d1e88673a4a3020").unwrap())
            .unwrap();
        let config = Config::from_path(Path::new("./fixtures/configs/simple.yml")).unwrap();
        dbg!(&config);
        let analyzer = CommitAnalyzer::new(&repo, commit, &config, &build_test_logger());
        let res = analyzer.analyze().unwrap();
        assert_eq!(res.github_url, Some("https://github.com/ghempton/codealong/commit/86d242301830075e93ff039a4d1e88673a4a3020".to_string()));
        assert_eq!(res.diff.tag_stats.get("docs").unwrap().new_work, 1);
    }
}
