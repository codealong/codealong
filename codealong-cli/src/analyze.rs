use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::thread;

use console::style;
use git2::Repository;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

use codealong::{AnalyzedRevwalk, Config};

use crate::error::Result;
use crate::repo::Repo;

const NUM_THREADS: usize = 6;

pub fn analyze(matches: &clap::ArgMatches) -> Result<()> {
    let repos = expand_repos(matches);

    println!("{} Initializing...", style("[1/4]").bold().dim());

    initialize_repos(&repos)?;

    println!("{} Analyzing...", style("[2/4]").bold().dim());

    analyze_repos(&repos)?;

    Ok(())
}

fn expand_repos(matches: &clap::ArgMatches) -> Vec<Repo> {
    let mut repos = Vec::new();

    if let Some(repo_paths) = matches.values_of("repo_path") {
        for repo_path in repo_paths {
            repos.push(Repo::Local(repo_path.to_owned()));
        }
    }

    if let Some(repo_urls) = matches.values_of("repo_url") {
        for repo_url in repo_urls {
            repos.push(Repo::Url(repo_url.to_owned()));
        }
    }

    repos
}

fn initialize_repos(repos: &Vec<Repo>) -> Result<()> {
    let m = MultiProgress::new();
    let mut tasks: VecDeque<InitializeTask> = VecDeque::new();
    for repo in repos {
        tasks.push_back(InitializeTask::new(&m, repo.clone()));
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

struct InitializeTask {
    repo: Repo,
    pb: ProgressBar,
}

impl InitializeTask {
    pub fn new(m: &MultiProgress, repo: Repo) -> InitializeTask {
        let pb = m.add(ProgressBar::new(0));
        pb.set_style(
            ProgressStyle::default_spinner()
                .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈ ")
                .template("{prefix:.bold.dim} {spinner} {wide_msg}"),
        );
        pb.set_message("initializing");

        InitializeTask { pb, repo }
    }

    pub fn start(&self) {
        self.pb.enable_steady_tick(100);
        self.repo.init().unwrap();
    }
}

fn analyze_repos(repos: &Vec<Repo>) -> Result<()> {
    let m = MultiProgress::new();
    let mut analyze_tasks: VecDeque<AnalyzeTask> = VecDeque::new();
    for repo in repos {
        analyze_tasks.push_back(AnalyzeTask::new(&m, repo.clone(), AnalyzeTaskType::Commit));
        analyze_tasks.push_back(AnalyzeTask::new(
            &m,
            repo.clone(),
            AnalyzeTaskType::PullRequest,
        ));
    }

    let mutex = Arc::new(Mutex::new(analyze_tasks));

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
    repo: Repo,
}

impl AnalyzeTask {
    pub fn new(m: &MultiProgress, repo: Repo, task_type: AnalyzeTaskType) -> AnalyzeTask {
        let pb = m.add(ProgressBar::new(0));
        pb.set_style(
            ProgressStyle::default_bar()
                .template(&format!(
                    "[{{elapsed_precise}}] {{bar:40.green/cyan}} {{pos:>7}}/{{len:7}} {:16} {{msg}}",
                    repo.display_name()
                ))
                .progress_chars("##-"),
        );
        pb.set_message("waiting");

        AnalyzeTask {
            pb,
            repo,
            task_type,
        }
    }

    pub fn start(&self) {
        self.analyze().unwrap();
    }

    fn analyze(&self) -> Result<()> {
        match self.task_type {
            AnalyzeTaskType::Commit => analyze_commits(&self.pb, &self.repo.init()?),
            AnalyzeTaskType::PullRequest => analyze_prs(&self.pb, &self.repo.init()?),
        }
    }
}

fn analyze_commits(pb: &ProgressBar, repo: &Repository) -> Result<()> {
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

fn analyze_prs(pb: &ProgressBar, repo: &Repository) -> Result<()> {
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
