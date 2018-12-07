use git2::{Commit, Diff, DiffOptions, Repository};

use std::cell::RefCell;
use std::collections::HashMap;

use crate::analyzed_diff::AnalyzedDiff;
use crate::config::Config;
use crate::error::Error;
use crate::file_analyzer::FileAnalyzer;
use crate::work_stats::WorkStats;

pub struct DiffAnalyzer<'a> {
    repo: &'a Repository,
    commit: &'a Commit<'a>,
    parent: Option<&'a Commit<'a>>,
    config: &'a Config,
}

impl<'a> DiffAnalyzer<'a> {
    pub fn new(
        repo: &'a Repository,
        commit: &'a Commit,
        parent: Option<&'a Commit>,
        config: &'a Config,
    ) -> DiffAnalyzer<'a> {
        DiffAnalyzer {
            repo,
            commit,
            parent,
            config,
        }
    }

    pub fn analyze(&self) -> Result<AnalyzedDiff, Error> {
        let mut result = AnalyzedDiff {
            tag_stats: HashMap::new(),
            stats: WorkStats::empty(),
        };
        let file_analyzer: RefCell<Option<FileAnalyzer>> = RefCell::new(None);
        let diff = self.build_diff()?;
        diff.foreach(
            &mut |diff_delta, _| {
                if let Some(file_analyzer) = file_analyzer.borrow_mut().take() {
                    result += file_analyzer.finish();
                }
                file_analyzer.replace(Some(FileAnalyzer::new(
                    self.repo,
                    self.commit,
                    self.parent,
                    &diff_delta,
                    self.config,
                )));
                true
            },
            None,
            Some(&mut |_diff_delta, _diff_hunk| {
                let mut inner = file_analyzer.borrow_mut().take().unwrap();
                inner.start_hunk().expect("unable to start hunk");
                file_analyzer.replace(Some(inner));
                true
            }),
            Some(&mut |_diff_delta, _diff_hunk, diff_line| {
                // TODO: figure out case where diff_hunk is none
                let mut inner = file_analyzer.borrow_mut().take().unwrap();
                inner
                    .analyze_line(&diff_line)
                    .expect("unable to analyze line");
                file_analyzer.replace(Some(inner));
                true
            }),
        )?;
        if let Some(file_analyzer) = file_analyzer.borrow_mut().take() {
            result += file_analyzer.finish();
        }
        Ok(result)
    }

    fn build_diff(&self) -> Result<Diff, Error> {
        let mut diff_opts = DiffOptions::new();
        diff_opts.ignore_whitespace(true);
        Ok(self.repo.diff_tree_to_tree(
            self.parent.map(|p| p.tree().unwrap()).as_ref(),
            Some(&self.commit.tree()?),
            Some(&mut diff_opts),
        )?)
    }
}
