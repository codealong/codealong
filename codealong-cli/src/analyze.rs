use slog::Logger;

use codealong::Repo;

use crate::analyze_repos::analyze_repos;
use crate::build_workspace::build_workspace;
use crate::error::Result;
use crate::initialize_repos::initialize_repos;
use crate::logger::OutputMode;

pub fn analyze(matches: &clap::ArgMatches, logger: &Logger, output_mode: OutputMode) -> Result<()> {
    let workspace = build_workspace(matches, logger)?;
    let skip_forks = matches.is_present("skip_forks");
    let repos: Vec<Repo> = workspace
        .repos()
        .into_iter()
        .filter(|r| !skip_forks || !r.repo_info().fork)
        .collect();
    initialize_repos(matches, repos.clone(), logger, output_mode)?;
    analyze_repos(matches, repos.clone(), logger, output_mode)?;
    Ok(())
}
