use slog::Logger;

use crate::analyze_repos::analyze_repos;
use crate::config::config_from_args;
use crate::error::Result;
use crate::expand_repos::expand_repos;
use crate::initialize_repos::initialize_repos;
use crate::logger::OutputMode;

pub fn analyze(matches: &clap::ArgMatches, logger: &Logger, output_mode: OutputMode) -> Result<()> {
    let config = config_from_args(matches);
    let repos = expand_repos(matches);
    initialize_repos(matches, repos.clone(), logger, output_mode)?;
    analyze_repos(matches, repos.clone(), config, logger, output_mode)?;
    Ok(())
}
