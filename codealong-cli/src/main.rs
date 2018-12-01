#[macro_use]
extern crate clap;
extern crate codealong;
extern crate codealong_elk;
extern crate codealong_github;
extern crate console;
#[macro_use]
extern crate error_chain;
extern crate git2;
extern crate indicatif;

mod error;

use error::Result;

use git2::Repository;

use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

use codealong::{AnalyzedRevwalk, Config};

use std::thread;

fn main() {
    use clap::App;

    let yml = load_yaml!("cli.yml");
    let matches = App::from_yaml(yml).get_matches();

    if let Some(matches) = matches.subcommand_matches("index") {
        index_command(matches).unwrap();
    }
}

fn index_command(matches: &clap::ArgMatches) -> Result<()> {
    let m = MultiProgress::new();
    let style = ProgressStyle::default_bar()
        .template("[{elapsed_precise}] {bar:40.green/cyan} {pos:>7}/{len:7} {msg}")
        .progress_chars("##-");

    let path = matches.value_of("repo_path").unwrap().to_owned();

    let pb = m.add(ProgressBar::new(0));
    pb.set_style(style.clone());
    let p = path.clone();
    let _ = thread::spawn(move || index_commits(pb, p).unwrap());

    let pb = m.add(ProgressBar::new(0));
    pb.set_style(style.clone());
    let p = path.clone();
    let _ = thread::spawn(move || index_prs(pb, p).unwrap());

    m.join_and_clear()?;

    Ok(())
}

fn index_commits(pb: ProgressBar, path: String) -> Result<()> {
    let repo = Repository::discover(&path).expect("unable to open repository");
    let config = Config::from_repo(&repo)?;
    let repo_name = config.repo_name.clone().unwrap();
    let revwalk = AnalyzedRevwalk::new(&repo, config)?;
    let client = codealong_elk::Client::default();
    pb.set_message(&format!("{}: calculating", repo_name));
    let count = revwalk.len();
    pb.set_length(count as u64);
    pb.set_message(&format!("{}: analyzing commits", repo_name));
    for analyzed_commit in revwalk {
        client.index(analyzed_commit?)?;
        pb.inc(1);
    }
    Ok(pb.finish_with_message(&format!("{}: done", repo_name)))
}

fn index_prs(pb: ProgressBar, path: String) -> Result<()> {
    let repo = Repository::discover(&path).expect("unable to open repository");
    let config = Config::from_repo(&repo)?;
    let repo_name = config.repo_name.clone().unwrap();
    let github_client = codealong_github::Client::public();
    let client = codealong_elk::Client::default();
    pb.set_message(&format!("{}: calculating", repo_name));
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
    pb.set_message(&format!("{}: analyzing pull requests", repo_name));
    for pr in cursor {
        let analyzer = codealong_github::PullRequestAnalyzer::new(&repo, pr, &config);
        client.index(analyzer.analyze()?)?;
        pb.inc(1);
    }
    Ok(pb.finish_with_message(&format!("{}: done", repo_name)))
}
