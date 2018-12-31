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
extern crate lazy_static;

mod analyze;
mod analyze_repos;
mod config;
mod error;
mod expand_repos;
mod init;
mod initialize_repos;
mod repo;
mod ui;
mod utils;

use crate::analyze::analyze;
use crate::init::init;

fn main() {
    use clap::App;

    let yml = load_yaml!("cli.yml");
    let matches = App::from_yaml(yml).get_matches();

    if let Some(matches) = matches.subcommand_matches("analyze") {
        analyze(matches).unwrap();
    }

    if let Some(matches) = matches.subcommand_matches("init") {
        init(matches).unwrap();
    }
}
