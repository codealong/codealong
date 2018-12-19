use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::thread;

use git2::Repository;

use codealong::{AnalyzedRevwalk, Config};

use crate::error::Result;
use crate::repo::Repo;
use crate::ui::{NamedProgressBar, ProgressPool};

/// Clone and/or fetch all repos
pub fn analyze_repos(matches: &clap::ArgMatches, repos: Vec<Repo>) -> Result<()> {
    let num_threads = matches
        .value_of("concurrency")
        .unwrap_or_else(|| "6")
        .parse::<i32>()?;
    let tasks = expand_tasks(&matches, repos);
    let m = Arc::new(ProgressPool::new(tasks.len() as u64));
    let tasks = Arc::new(Mutex::new(tasks));
    m.set_message("Data sources analyzed");
    for _ in 0..num_threads {
        let tasks = tasks.clone();
        let m = m.clone();
        let mut pb = m.add();
        thread::spawn(move || loop {
            let task = {
                let mut tasks = tasks.lock().unwrap();
                tasks.pop_front()
            };
            if let Some(task) = task {
                pb.reset(task.display_name().to_owned());
                task.analyze(&pb)
                    .unwrap_or_else(|_err| pb.set_message("error"));
                m.inc(1);
            } else {
                pb.finish();
                break;
            }
        });
    }
    m.join_and_clear()?;
    Ok(())
}

fn expand_tasks(matches: &clap::ArgMatches, repos: Vec<Repo>) -> VecDeque<AnalyzeTask> {
    let mut tasks: VecDeque<AnalyzeTask> = VecDeque::new();
    for repo in repos {
        if !matches.is_present("skip_commits") {
            tasks.push_back(AnalyzeTask {
                repo: repo.clone(),
                task_type: AnalyzeTaskType::Commit,
            });
        }
        if !matches.is_present("skip_pull_requests") {
            tasks.push_back(AnalyzeTask {
                repo: repo.clone(),
                task_type: AnalyzeTaskType::PullRequest,
            });
        }
    }
    tasks
}

enum AnalyzeTaskType {
    Commit,
    PullRequest,
}

struct AnalyzeTask {
    task_type: AnalyzeTaskType,
    repo: Repo,
}

impl AnalyzeTask {
    fn analyze(&self, pb: &NamedProgressBar) -> Result<()> {
        match self.task_type {
            AnalyzeTaskType::Commit => analyze_commits(pb, &self.repo.init()?),
            AnalyzeTaskType::PullRequest => analyze_prs(pb, &self.repo.init()?),
        }
    }

    fn display_name(&self) -> &str {
        self.repo.display_name()
    }
}

fn analyze_commits(pb: &NamedProgressBar, repo: &Repository) -> Result<()> {
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
    Ok(pb.finish())
}

fn analyze_prs(pb: &NamedProgressBar, repo: &Repository) -> Result<()> {
    let config = Config::from_repo(&repo)?;
    let github_client = codealong_github::Client::from_env();
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
    Ok(pb.finish())
}
