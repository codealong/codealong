use git2::{Commit, Delta, DiffDelta, DiffLine, Repository};

use crate::analyzed_diff::AnalyzedDiff;
use crate::config_context::ConfigContext;
use crate::error::Error;
use crate::git_blame::GitBlame;
use crate::hunk_analyzer::HunkAnalyzer;
use crate::working_config::{FileConfig, PersonConfig, WorkingConfig};

pub struct FileAnalyzer<'a> {
    repo: &'a Repository,
    commit: &'a Commit<'a>,
    result: AnalyzedDiff,
    blame: Option<GitBlame>,
    config_context: ConfigContext,
    current_hunk: Option<HunkAnalyzer<'a>>,
    ignored: bool,
}

impl<'a> FileAnalyzer<'a> {
    pub fn new(
        repo: &'a Repository,
        commit: &'a Commit<'a>,
        parent: Option<&'a Commit<'a>>,
        diff_delta: &DiffDelta,
        config: &'a WorkingConfig,
    ) -> FileAnalyzer<'a> {
        let file_config = get_file_config(config, &diff_delta);
        let author_config = get_author_config(config, commit);
        let config_context = ConfigContext::new(file_config.as_ref(), author_config.as_ref());
        let blame = get_blame(repo, diff_delta, parent, config);

        FileAnalyzer {
            repo,
            commit,
            result: AnalyzedDiff::empty(),
            config_context,
            blame,
            current_hunk: None,
            ignored: file_config.map(|c| c.ignore()).unwrap_or(false),
        }
    }

    pub fn start_hunk(&mut self) -> Result<(), Error> {
        self.finish_hunk();
        self.current_hunk.replace(HunkAnalyzer::new(
            self.repo,
            self.commit,
            self.blame.take(),
            self.config_context.weight(),
        ));
        Ok(())
    }

    pub fn analyze_line(&mut self, diff_line: &DiffLine) -> Result<(), Error> {
        if !self.ignored {
            let mut current_hunk = self.current_hunk.take().expect("no hunk started");
            current_hunk.analyze_line(diff_line)?;
            self.current_hunk.replace(current_hunk);
        }
        Ok(())
    }

    fn finish_hunk(&mut self) {
        if let Some(current_hunk) = self.current_hunk.take() {
            let (blame, hunk_result) = current_hunk.finish();
            self.blame = blame;
            self.result
                .add_stats(hunk_result, self.config_context.tags());
        }
    }

    pub fn finish(mut self) -> AnalyzedDiff {
        self.finish_hunk();
        self.result
    }
}

fn get_file_config<'a>(
    config: &'a WorkingConfig,
    diff_delta: &DiffDelta,
) -> Option<FileConfig<'a>> {
    diff_delta
        .new_file()
        .path()
        .or(diff_delta.old_file().path())
        .and_then(|path| path.to_str().and_then(|path| config.config_for_file(path)))
}

fn get_author_config<'a>(config: &'a WorkingConfig, commit: &Commit) -> Option<PersonConfig<'a>> {
    config.config_for_identity(&commit.author().into())
}

fn get_blame(
    repo: &Repository,
    diff_delta: &DiffDelta,
    parent: Option<&Commit>,
    config: &WorkingConfig,
) -> Option<GitBlame> {
    if diff_delta.status() != Delta::Modified {
        return None;
    }
    diff_delta.old_file().path().and_then(|old_path| {
        parent.and_then(|parent| {
            if let Ok(new_blame) =
                GitBlame::new(&repo, &parent.id(), &old_path, config.churn_cutoff())
            {
                Some(new_blame)
            } else {
                None
            }
        })
    })
}
