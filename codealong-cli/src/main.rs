#[macro_use]
extern crate clap;
extern crate codealong;
extern crate codealong_elk;
extern crate console;
#[macro_use]
extern crate error_chain;
extern crate git2;
extern crate indicatif;

mod error;

use error::Result;

use git2::Repository;

use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

use codealong::{AnalyzedRevwalk, Config};
use codealong_elk::Client;

use std::thread;

fn main() {
    use clap::App;

    let yml = load_yaml!("cli.yml");
    let matches = App::from_yaml(yml).get_matches();

    if let Some(matches) = matches.subcommand_matches("index") {
        index_command(matches).unwrap();
    }
}

fn index_command(matches: &clap::ArgMatches) -> Result<()> {
    let m = MultiProgress::new();
    let style = ProgressStyle::default_bar()
        .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7} {msg}")
        .progress_chars("##-");

    let pb = m.add(ProgressBar::new(0));
    pb.set_style(style.clone());

    let path = matches.value_of("repo_path").unwrap().to_owned();

    let _ = thread::spawn(move || -> Result<()> {
        let repo = Repository::discover(path).expect("unable to open repository");
        let config = Config::from_dir(repo.path())?;
        let revwalk = AnalyzedRevwalk::new(&repo, config)?;
        let client = Client::default();
        pb.set_message("calculating");
        let count = revwalk.len();
        pb.set_length(count as u64);
        pb.set_message("analyzing commits");
        for analyzed_commit in revwalk {
            client.index(analyzed_commit?)?;
            pb.inc(1);
        }
        Ok(pb.finish_with_message("Done"))
    });

    m.join_and_clear()?;

    Ok(())
}
