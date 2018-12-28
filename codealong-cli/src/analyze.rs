use console::style;

use crate::analyze_repos::analyze_repos;
use crate::config::config_from_args;
use crate::error::Result;
use crate::expand_repos::expand_repos;
use crate::initialize_repos::initialize_repos;

pub fn analyze(matches: &clap::ArgMatches) -> Result<()> {
    let config = config_from_args(matches);
    println!("{} Expanding repos...", style("[1/3]").bold().dim());
    let repos = expand_repos(matches);
    println!("{} Initializing...", style("[2/3]").bold().dim());
    initialize_repos(matches, repos.clone())?;
    println!("{} Analyzing...", style("[3/3]").bold().dim());
    analyze_repos(matches, repos.clone(), config)?;
    Ok(())
}
