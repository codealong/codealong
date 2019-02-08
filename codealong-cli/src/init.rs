use std::fs::create_dir_all;
use std::fs::File;
use std::path::Path;

use slog::Logger;

use codealong::WorkspaceConfig;
use codealong_github::config_from_org;

use crate::error::Result;

pub fn init(matches: &clap::ArgMatches, logger: &Logger) -> Result<()> {
    let dir = matches.value_of("destination").unwrap_or(".");
    let dest = Path::join(Path::new(dir), "config.yml");
    info!(logger, "Initializing config at {}", dest.to_str().unwrap());
    let config = build_config(matches, logger)?;
    write_config(&dest, &config)?;
    info!(logger, "Initialized config at {}", dest.to_str().unwrap());
    Ok(())
}

fn build_config(matches: &clap::ArgMatches, logger: &Logger) -> Result<WorkspaceConfig> {
    let mut config = WorkspaceConfig::default();
    let client = codealong_github::Client::from_env();
    if let Some(github_orgs) = matches.values_of("github_org") {
        for github_org in github_orgs {
            let org_config = config_from_org(&client, github_org, logger)?;
            config.merge(org_config);
        }
    }
    Ok(config)
}

fn write_config(dest: &Path, config: &WorkspaceConfig) -> Result<()> {
    if let Some(parent) = dest.parent() {
        create_dir_all(parent)?;
    }
    let file = File::create(&dest)?;
    serde_yaml::to_writer(file, &config)?;
    Ok(())
}
