use std::fs::create_dir_all;
use std::fs::File;
use std::path::{Path, PathBuf};

use codealong::Config;
use codealong_github::config_from_org;

use crate::error::Result;

pub fn init(matches: &clap::ArgMatches) -> Result<()> {
    let config = build_config(matches)?;
    let dest = write_config(matches, &config)?;
    println!("Initialized config at {}", dest.to_str().unwrap());
    Ok(())
}

fn build_config(matches: &clap::ArgMatches) -> Result<Config> {
    let mut config = Config::default();
    if let Some(github_orgs) = matches.values_of("github_org") {
        for github_org in github_orgs {
            let org_config = config_from_org(github_org)?;
            config.merge(org_config);
        }
    }
    Ok(config)
}

fn write_config(_matches: &clap::ArgMatches, config: &Config) -> Result<PathBuf> {
    let dest = Path::new("./.codealong/config.yml");
    create_dir_all(dest.parent().unwrap())?;
    let file = File::create(&dest)?;
    serde_yaml::to_writer(file, &config)?;
    Ok(dest.to_path_buf())
}
