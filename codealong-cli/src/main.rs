#[macro_use]
extern crate clap;
extern crate codealong;
extern crate codealong_elk;
extern crate codealong_github;
extern crate console;
extern crate dirs;
#[macro_use]
extern crate error_chain;
extern crate git2;
extern crate indicatif;
#[macro_use]
extern crate slog;
extern crate sloggers;

mod analyze;
mod analyze_repos;
mod build_workspace;
mod error;
mod init;
mod initialize_repos;
mod logger;
mod ui;

use error_chain::ChainedError;

use crate::analyze::analyze;
use crate::init::init;
use crate::logger::build_logger;

fn main() {
    use clap::App;

    let yml = load_yaml!("cli.yml");
    let matches = App::from_yaml(yml).get_matches();

    let logger = build_logger(&matches);

    if let Some(matches) = matches.subcommand_matches("analyze") {
        analyze(matches, &logger).map_err(|e| {
            error!(logger, "error invoking analyze subcommand"; "error" => e.display_chain().to_string());
            e
        }).unwrap();
    }

    if let Some(matches) = matches.subcommand_matches("init") {
        init(matches, &logger).map_err(|e| {
            error!(logger, "error invoking init subcommand"; "error" => e.display_chain().to_string());
            e
        }).unwrap();
    }
}
