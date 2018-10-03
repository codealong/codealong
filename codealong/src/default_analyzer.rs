use git2::{
    Blame, BlameOptions, Commit, DiffDelta, DiffLine, DiffOptions, Oid, Repository, Signature,
};

use std::collections::HashMap;
use std::path::Path;
use std::str;

use analyzed_diff::AnalyzedDiff;
use analyzer::Analyzer;
use error::Error;
use work_stats::WorkStats;

use std::cell::RefCell;

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
        let mut blame_opts = BlameOptions::new();
        blame_opts.newest_commit(parent.id());
        blame_opts.track_copies_any_commit_copies(false);
        blame_opts.track_copies_same_file(false);
        blame_opts.track_copies_same_commit_copies(false);
        blame_opts.track_copies_same_commit_moves(false);
        blame_opts.first_parent(true);
        let blame: RefCell<Option<Blame>> = RefCell::new(None);
        diff.foreach(
            &mut |diff_delta, _| {
                if let Some(new_path) = diff_delta.new_file().path() {
                    if let Ok(new_blame) = repo.blame_file(new_path, Some(&mut blame_opts)) {
                        blame.replace(Some(new_blame));
                    } else {
                        blame.replace(None);
                    }
                }
                true
            },
            None,
            None,
            Some(&mut |diff_delta, _, diff_line| {
                println!(
                    "line: {:#?} {:#?} {:#?}",
                    &diff_delta.status(),
                    &diff_line.origin(),
                    str::from_utf8(&diff_line.content())
                );
                let analyzed_delta =
                    self.analyze_line_diff(
                        &repo,
                        &commit,
                        &parent,
                        &diff_delta,
                        &diff_line,
                        blame.borrow().as_ref(),
                    ).unwrap();
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
        blame: Option<&Blame>,
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
        parent: &Commit,
        diff_delta: &DiffDelta,
        diff_line: &DiffLine,
        blame: &Blame,
    ) -> Result<WorkStats, Error> {
        let new_path = diff_delta
            .new_file()
            .path()
            .expect("No file path available");
        if let Some(blame_hunk) = blame.get_line(diff_line.old_lineno().unwrap() as usize) {
            let previous_commit = repo.find_commit(blame_hunk.orig_commit_id()).unwrap();

            println!("Via libgit2: #{:?}", previous_commit.id());
            blame_line(
                &repo,
                &parent.id(),
                new_path,
                diff_line.old_lineno().unwrap(),
            );

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

use std::process::Command;

// libgit2 has an extremely slow blame implementation:
// https://github.com/libgit2/libgit2/issues/3027
// so we use a git binary
fn blame_line(repo: &Repository, parent: &Oid, new_path: &Path, old_lineno: u32) -> Oid {
    let output = Command::new("git")
        .current_dir(repo.path())
        .arg("blame")
        .arg(format!("-L {},{}", old_lineno, old_lineno))
        .arg(parent.to_string())
        .arg("-s")
        .arg("-l")
        .arg("--")
        .arg(new_path)
        .output()
        .expect("failed to execute process")
        .stdout;

    let raw = String::from_utf8_lossy(&output);
    let mut split = raw.splitn(2, ' ');
    Oid::from_str(split.next().unwrap()).unwrap()
}
