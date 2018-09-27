use git2::{BlameOptions, Commit, DiffDelta, DiffLine, DiffOptions, Repository, Signature};

use std::collections::HashMap;
use std::str;

use analyzed_diff::AnalyzedDiff;
use analyzer::Analyzer;
use error::Error;
use work_stats::WorkStats;

pub struct DefaultAnalyzer {}

impl Analyzer for DefaultAnalyzer {
    fn analyze_diff(
        &self,
        repo: &Repository,
        commit: &Commit,
        parent: &Commit,
    ) -> Result<AnalyzedDiff, Error> {
        let mut diff_opts = DiffOptions::new();
        diff_opts.ignore_whitespace(true);
        let diff =
            repo.diff_tree_to_tree(
                Some(&parent.tree()?),
                Some(&commit.tree()?),
                Some(&mut diff_opts),
            ).unwrap();
        let mut stats = WorkStats::empty();
        diff.foreach(
            &mut |d, _| {
                println!("file");
                true
            },
            Some(&mut |d, _| {
                println!("binary");
                true
            }),
            Some(&mut |d, _| {
                println!("hunk");
                true
            }),
            Some(&mut |diff_delta, _, diff_line| {
                println!(
                    "line: {:#?} {:#?} {:#?}",
                    &diff_delta.status(),
                    &diff_line.origin(),
                    str::from_utf8(&diff_line.content())
                );
                let analyzed_delta = self
                    .analyze_line_diff(&repo, &commit, &parent, &diff_delta, &diff_line)
                    .unwrap();
                println!("analyzed: {:#?}", analyzed_delta);
                stats += analyzed_delta;
                true
            }),
        )?;
        let analyzed_diff = AnalyzedDiff {
            tag_stats: HashMap::new(),
            stats,
        };
        Ok(analyzed_diff)
    }
}

impl DefaultAnalyzer {
    pub fn new() -> DefaultAnalyzer {
        DefaultAnalyzer {}
    }

    fn analyze_line_diff(
        &self,
        repo: &Repository,
        commit: &Commit,
        parent: &Commit,
        diff_delta: &DiffDelta,
        diff_line: &DiffLine,
    ) -> Result<WorkStats, Error> {
        match diff_line.origin() {
            '+' => Ok(WorkStats::new_work()),
            ' ' => self.analyze_line_change(repo, commit, parent, diff_delta, diff_line),
            _ => Ok(WorkStats::other()),
        }
    }

    fn analyze_line_change(
        &self,
        repo: &Repository,
        commit: &Commit,
        parent: &Commit,
        diff_delta: &DiffDelta,
        diff_line: &DiffLine,
    ) -> Result<WorkStats, Error> {
        let new_path = diff_delta
            .new_file()
            .path()
            .expect("No file path available");
        let mut blame_opts = BlameOptions::new();
        blame_opts.newest_commit(parent.id());
        let blame = repo.blame_file(new_path, Some(&mut blame_opts)).unwrap();
        if let Some(blame_hunk) = blame.get_line(diff_line.old_lineno().unwrap() as usize) {
            let previous_commit = repo.find_commit(blame_hunk.orig_commit_id()).unwrap();
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
