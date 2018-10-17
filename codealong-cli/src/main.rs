#[macro_use]
extern crate clap;

extern crate codealong_elk;
extern crate git2;

use codealong_elk::index;
use git2::Repository;

fn main() {
    use clap::App;

    let yml = load_yaml!("cli.yml");
    let matches = App::from_yaml(yml).get_matches();

    if let Some(matches) = matches.subcommand_matches("index") {
        let path = matches.value_of("repo_path").unwrap();
        let repo = Repository::discover(path).expect("unable to open repository");
        index(&repo);
    }
}
