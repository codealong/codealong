use git2::{Commit, DiffDelta, DiffLine, DiffOptions, Repository, Signature};

use std::collections::HashMap;

use analyzed_diff::AnalyzedDiff;
use analyzer::Analyzer;
use config::{AuthorConfig, Config, FileConfig};
use error::Error;
use fast_blame::FastBlame;
use work_stats::WorkStats;

use std::cell::RefCell;

pub struct DefaultAnalyzer {
    config: Config,
}

impl Analyzer for DefaultAnalyzer {
    fn config(&self) -> &Config {
        &self.config
    }

    fn analyze_diff(
        &self,
        repo: &Repository,
        commit: &Commit,
        parent: Option<&Commit>,
    ) -> Result<AnalyzedDiff, Error> {
        let mut diff_opts = DiffOptions::new();
        diff_opts.ignore_whitespace(true);
        let diff =
            repo.diff_tree_to_tree(
                parent.map(|p| p.tree().unwrap()).as_ref(),
                Some(&commit.tree()?),
                Some(&mut diff_opts),
            ).unwrap();
        let blame: RefCell<Option<FastBlame>> = RefCell::new(None);
        let author_config: Option<&AuthorConfig> = self.get_author_config(repo, commit);
        let file_config: RefCell<Option<&FileConfig>> = RefCell::new(None);
        let mut result = AnalyzedDiff {
            tag_stats: HashMap::new(),
            stats: WorkStats::empty(),
        };
        diff.foreach(
            &mut |diff_delta, _| {
                blame.replace(self.get_blame(repo, &diff_delta, parent));
                file_config.replace(self.get_file_config(repo, &diff_delta));
                true
            },
            None,
            None,
            Some(&mut |diff_delta, _, diff_line| {
                let mut tags: Vec<String> = vec![];
                file_config
                    .borrow()
                    .and_then(|c| Some(tags.extend(c.tags.clone())));
                author_config.and_then(|c| Some(tags.extend(c.tags.clone())));
                let line_stats =
                    self.analyze_line_diff(
                        &repo,
                        &commit,
                        parent,
                        &diff_delta,
                        &diff_line,
                        blame.borrow().as_ref(),
                    ).unwrap();
                result.add_stats(line_stats, &tags);
                true
            }),
        )?;
        Ok(result)
    }
}

impl DefaultAnalyzer {
    pub fn new() -> DefaultAnalyzer {
        DefaultAnalyzer {
            config: Config::default(),
        }
    }

    pub fn with_config(config: Config) -> DefaultAnalyzer {
        DefaultAnalyzer { config: config }
    }

    fn get_blame(
        &self,
        repo: &Repository,
        diff_delta: &DiffDelta,
        parent: Option<&Commit>,
    ) -> Option<FastBlame> {
        diff_delta.old_file().path().and_then(|old_path| {
            parent.and_then(|parent| {
                if let Ok(new_blame) = FastBlame::new(&repo, &parent.id(), &old_path) {
                    Some(new_blame)
                } else {
                    None
                }
            })
        })
    }

    fn get_file_config(&self, _repo: &Repository, diff_delta: &DiffDelta) -> Option<&FileConfig> {
        diff_delta
            .new_file()
            .path()
            .or(diff_delta.old_file().path())
            .and_then(|path| {
                path.to_str()
                    .and_then(|path| self.config.config_for_file(path))
            })
    }

    fn get_author_config(&self, _repo: &Repository, _commit: &Commit) -> Option<&AuthorConfig> {
        // TODO
        None
    }

    fn analyze_line_diff(
        &self,
        repo: &Repository,
        commit: &Commit,
        parent: Option<&Commit>,
        diff_delta: &DiffDelta,
        diff_line: &DiffLine,
        blame: Option<&FastBlame>,
    ) -> Result<WorkStats, Error> {
        match diff_line.origin() {
            '+' => Ok(WorkStats::new_work()),
            ' ' => self.analyze_line_change(
                repo,
                commit,
                parent,
                diff_delta,
                diff_line,
                blame.unwrap(),
            ),
            _ => Ok(WorkStats::other()),
        }
    }

    fn analyze_line_change(
        &self,
        repo: &Repository,
        commit: &Commit,
        _parent: Option<&Commit>,
        _diff_delta: &DiffDelta,
        diff_line: &DiffLine,
        blame: &FastBlame,
    ) -> Result<WorkStats, Error> {
        if let Some(previous_commit_oid) = blame.get_line(diff_line.old_lineno().unwrap() as usize)
        {
            let previous_commit = repo.find_commit(previous_commit_oid).unwrap();

            let diff_in_seconds =
                commit.committer().when().seconds() - previous_commit.committer().when().seconds();
            if diff_in_seconds < 60 * 60 * 24 * 7 * 3 {
                if self.compare_signatures(&previous_commit.author(), &commit.author()) {
                    return Ok(WorkStats::churn());
                } else {
                    return Ok(WorkStats::help_others());
                }
            }
        }
        return Ok(WorkStats::legacy_refactor());
    }

    fn compare_signatures(&self, a: &Signature, b: &Signature) -> bool {
        if let Some(email_a) = a.email() {
            if let Some(email_b) = b.email() {
                return email_a == email_b;
            }
        }
        return false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use git2::Oid;

    #[test]
    fn test_initial_commit() {
        let repo = Repository::open("./fixtures/repos/simple").unwrap();
        let commit = repo
            .find_commit(Oid::from_str("86d242301830075e93ff039a4d1e88673a4a3020").unwrap())
            .unwrap();
        let analyzer = DefaultAnalyzer::new();
        let res = analyzer.analyze_commit(&repo, &commit).unwrap();
        assert_eq!(res.diff.stats.new_work, 1);
    }

    #[test]
    fn test_with_config() {
        let repo = Repository::open("./fixtures/repos/simple").unwrap();
        let commit = repo
            .find_commit(Oid::from_str("86d242301830075e93ff039a4d1e88673a4a3020").unwrap())
            .unwrap();
        let analyzer = DefaultAnalyzer::with_config(
            Config::from_file("./fixtures/configs/simple.yml").unwrap(),
        );
        let res = analyzer.analyze_commit(&repo, &commit).unwrap();
        assert_eq!(res.github_url, Some("https://github.com/ghempton/codealong/commit/86d242301830075e93ff039a4d1e88673a4a3020".to_string()));
        assert_eq!(res.diff.tag_stats.get("docs").unwrap().new_work, 1);
    }
}
