use error::Result;

use git2::Repository;

use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

use codealong::{AnalyzedRevwalk, Config};

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::thread;

const NUM_THREADS: usize = 6;

pub fn analyze(matches: &clap::ArgMatches) -> Result<()> {
    let m = MultiProgress::new();

    let mut tasks: VecDeque<AnalyzeTask> = VecDeque::new();
    for repo_path in matches.values_of("repo_path").unwrap() {
        tasks.push_back(AnalyzeTask::new(&m, repo_path, AnalyzeTaskType::Commit));
        tasks.push_back(AnalyzeTask::new(
            &m,
            repo_path,
            AnalyzeTaskType::PullRequest,
        ));
    }

    let mutex = Arc::new(Mutex::new(tasks));

    for _ in 0..NUM_THREADS {
        let mutex = mutex.clone();
        thread::spawn(move || loop {
            let task = {
                let mut tasks = mutex.lock().unwrap();
                tasks.pop_front()
            };
            if let Some(task) = task {
                task.start();
            } else {
                break;
            }
        });
    }

    m.join_and_clear()?;

    Ok(())
}

enum AnalyzeTaskType {
    Commit,
    PullRequest,
}

struct AnalyzeTask {
    pb: ProgressBar,
    task_type: AnalyzeTaskType,
    repo_path: String,
}

impl AnalyzeTask {
    pub fn new(m: &MultiProgress, repo_path: &str, task_type: AnalyzeTaskType) -> AnalyzeTask {
        let pb = m.add(ProgressBar::new(0));
        pb.set_style(
            ProgressStyle::default_bar()
                .template(&format!(
                    "[{{elapsed_precise}}] {{bar:40.green/cyan}} {{pos:>7}}/{{len:7}} {:16} {{msg}}",
                    repo_path
                ))
                .progress_chars("##-"),
        );
        pb.set_message("waiting");

        AnalyzeTask {
            pb,
            repo_path: repo_path.to_string(),
            task_type,
        }
    }

    pub fn start(&self) {
        self.analyze().unwrap();
    }

    fn analyze(&self) -> Result<()> {
        match self.task_type {
            AnalyzeTaskType::Commit => analyze_commits(&self.pb, &self.repo_path),
            AnalyzeTaskType::PullRequest => analyze_prs(&self.pb, &self.repo_path),
        }
    }
}

fn analyze_commits(pb: &ProgressBar, path: &str) -> Result<()> {
    let repo = Repository::discover(path).expect("unable to open repository");
    let config = Config::from_repo(&repo)?;
    let revwalk = AnalyzedRevwalk::new(&repo, config)?;
    let client = codealong_elk::Client::default();
    pb.set_message("calculating");
    let count = revwalk.len();
    pb.set_length(count as u64);
    pb.set_message("analyzing commits");
    for analyzed_commit in revwalk {
        client.index(analyzed_commit?)?;
        pb.inc(1);
    }
    Ok(pb.finish_with_message("done"))
}

fn analyze_prs(pb: &ProgressBar, path: &str) -> Result<()> {
    let repo = Repository::discover(path).expect("unable to open repository");
    let config = Config::from_repo(&repo)?;
    let github_client = codealong_github::Client::public();
    let client = codealong_elk::Client::default();
    pb.set_message("calculating");
    let url = format!(
        "https://api.github.com/repos/{}/pulls?state=all",
        config.github.clone().unwrap()
    );
    let mut cursor: codealong_github::Cursor<codealong_github::PullRequest> =
        codealong_github::Cursor::new(&github_client, &url);
    let count = cursor.guess_len();
    if let Some(count) = count {
        pb.set_length(count as u64);
    }
    pb.set_message("analyzing pull requests");
    for pr in cursor {
        let analyzer = codealong_github::PullRequestAnalyzer::new(&repo, pr, &config);
        client.index(analyzer.analyze()?)?;
        pb.inc(1);
    }
    Ok(pb.finish_with_message("done"))
}
