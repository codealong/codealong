use slog::Logger;
use std::path::Path;

use codealong::{Config, RepoInfo, Workspace};

use crate::error::*;

pub fn build_workspace(matches: &clap::ArgMatches, logger: &Logger) -> Result<Workspace> {
    let mut res = if let Some(workspace_path) = matches.value_of("workspace_path") {
        Workspace::from_dir(Path::new(workspace_path).to_path_buf())
    } else {
        Workspace::current(std::env::current_dir()?)
    }?;

    info!(
        logger,
        "Using workspace located at {}",
        &res.path().to_string_lossy()
    );

    if let Some(config_paths) = matches.values_of("config_path") {
        for config_path in config_paths {
            let config = Config::from_path(Path::new(config_path))?;
            res.add_config(config);
        }
    }

    let repos = expand_repos(matches)?;
    for repo_info in repos {
        res.add(repo_info, None)?;
    }

    Ok(res)
}

/// Given all possible repo-related arguments, expand them to a list of Repo
/// structs.
fn expand_repos(matches: &clap::ArgMatches) -> Result<Vec<RepoInfo>> {
    let mut repos = Vec::new();

    // TODO support local paths
    if let Some(repo_urls) = matches.values_of("repo") {
        for repo_url in repo_urls {
            repos.push(RepoInfo::from_url(repo_url)?);
        }
    }

    Ok(repos)
}
