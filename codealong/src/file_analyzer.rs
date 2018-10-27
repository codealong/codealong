use git2::{Commit, Delta, DiffDelta, DiffLine, Repository};

use analyzed_diff::AnalyzedDiff;
use config::{AuthorConfig, Config, FileConfig};
use config_context::ConfigContext;
use error::Error;
use fast_blame::FastBlame;
use hunk_analyzer::HunkAnalyzer;

pub struct FileAnalyzer<'a> {
    repo: &'a Repository,
    commit: &'a Commit<'a>,
    result: AnalyzedDiff,
    blame: Option<FastBlame>,
    config_context: ConfigContext,
    current_hunk: Option<HunkAnalyzer<'a>>,
}

impl<'a> FileAnalyzer<'a> {
    pub fn new(
        repo: &'a Repository,
        commit: &'a Commit<'a>,
        parent: Option<&'a Commit<'a>>,
        diff_delta: &DiffDelta,
        config: &'a Config,
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
        }
    }

    pub fn start_hunk(&mut self) -> Result<(), Error> {
        if let Some(current_hunk) = self.current_hunk.take() {
            let hunk_result = current_hunk.finish();
            self.result
                .add_stats(hunk_result, self.config_context.tags())
        }
        self.current_hunk.replace(HunkAnalyzer::new(
            self.repo,
            self.commit,
            self.blame.take(),
            self.config_context.weight(),
        ));
        Ok(())
    }

    pub fn analyze_line(&mut self, diff_line: &DiffLine) -> Result<(), Error> {
        let mut current_hunk = self.current_hunk.take().expect("no hunk started");
        current_hunk.analyze_line(diff_line)?;
        self.current_hunk.replace(current_hunk);
        Ok(())
    }

    pub fn finish(mut self) -> AnalyzedDiff {
        if let Some(current_hunk) = self.current_hunk.take() {
            let hunk_result = current_hunk.finish();
            self.result
                .add_stats(hunk_result, self.config_context.tags())
        }
        self.result
    }
}

fn get_file_config<'a>(config: &'a Config, diff_delta: &DiffDelta) -> Option<FileConfig<'a>> {
    diff_delta
        .new_file()
        .path()
        .or(diff_delta.old_file().path())
        .and_then(|path| path.to_str().and_then(|path| config.config_for_file(path)))
}

fn get_author_config(_config: &Config, _commit: &Commit) -> Option<AuthorConfig> {
    // TODO
    None
}

fn get_blame(
    repo: &Repository,
    diff_delta: &DiffDelta,
    parent: Option<&Commit>,
    config: &Config,
) -> Option<FastBlame> {
    if diff_delta.status() != Delta::Modified {
        return None;
    }
    diff_delta.old_file().path().and_then(|old_path| {
        parent.and_then(|parent| {
            if let Ok(new_blame) =
                FastBlame::new(&repo, &parent.id(), &old_path, config.churn_cutoff)
            {
                Some(new_blame)
            } else {
                None
            }
        })
    })
}
