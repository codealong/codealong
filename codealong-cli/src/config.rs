use std::path::Path;

use codealong::Config;

pub fn config_from_args(matches: &clap::ArgMatches) -> Config {
    let mut config = Config::base();

    if let Some(config_paths) = matches.values_of("config_path") {
        for config_path in config_paths {
            config.merge(Config::from_path(Path::new(config_path)).unwrap());
        }
    }

    config
}
