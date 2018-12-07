#[macro_use]
extern crate clap;
extern crate codealong;
extern crate codealong_elk;
extern crate codealong_github;
extern crate console;
#[macro_use]
extern crate error_chain;
extern crate git2;
extern crate indicatif;

mod analyze;
mod error;

use crate::analyze::analyze;

fn main() {
    use clap::App;

    let yml = load_yaml!("cli.yml");
    let matches = App::from_yaml(yml).get_matches();

    if let Some(matches) = matches.subcommand_matches("analyze") {
        analyze(matches).unwrap();
    }
}
