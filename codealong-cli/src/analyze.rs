use slog::Logger;

use codealong::{Repo, RepoInfo, Workspace};

use crate::analyze_repos::analyze_repos;
use crate::build_workspace::build_workspace;
use crate::error::Result;
use crate::initialize_repos::initialize_repos;
use crate::logger::OutputMode;

pub fn analyze(matches: &clap::ArgMatches, logger: &Logger, output_mode: OutputMode) -> Result<()> {
    let workspace = build_workspace(matches, logger)?;
    let repos = build_repos(&workspace, matches);
    initialize_repos(matches, repos.clone(), logger, output_mode)?;
    analyze_repos(matches, repos.clone(), logger, output_mode)?;
    Ok(())
}

fn build_repos(workspace: &Workspace, matches: &clap::ArgMatches) -> Vec<Repo> {
    let skip_forks = matches.is_present("skip_forks");
    let mut repos: Vec<Repo> = workspace
        .repos()
        .into_iter()
        .filter(|r| !skip_forks || !r.repo_info().fork)
        .collect();

    if let Some(repo_urls) = matches.values_of("repo") {
        let explicit_repos: Vec<RepoInfo> = repo_urls
            .map(|url| RepoInfo::from_url(url).unwrap())
            .collect();
        repos = repos
            .into_iter()
            .filter(|r| {
                explicit_repos
                    .iter()
                    .find(|info| r.repo_info().name == info.name)
                    .is_some()
            })
            .collect();
    }

    repos
}
