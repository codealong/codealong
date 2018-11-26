#[macro_use]
extern crate clap;

extern crate codealong;
extern crate codealong_elk;
extern crate console;
extern crate git2;
extern crate indicatif;

use git2::Repository;

use indicatif::ProgressBar;

use console::{style, Emoji};

use codealong::{AnalyzedRevwalk, Config, Error};
use codealong_elk::Client;

static LOOKING_GLASS: Emoji = Emoji("🔍 ", "");
static CHART_INCREASING: Emoji = Emoji("📈 ", "");

fn main() {
    use clap::App;

    let yml = load_yaml!("cli.yml");
    let matches = App::from_yaml(yml).get_matches();

    if let Some(matches) = matches.subcommand_matches("index") {
        index_command(matches).unwrap();
    }
}

fn index_command(matches: &clap::ArgMatches) -> Result<(), Error> {
    let path = matches.value_of("repo_path").unwrap();
    let repo = Repository::discover(path).expect("unable to open repository");
    let config = Config::from_dir(repo.path())?;
    let revwalk = AnalyzedRevwalk::new(&repo, config)?;
    let client = Client::default();

    println!(
        "{} {}Calculating stats...",
        style("[1/2]").bold().dim(),
        LOOKING_GLASS
    );

    let count = revwalk.len();

    println!(
        "{} {}Analyzing commits...",
        style("[2/2]").bold().dim(),
        CHART_INCREASING
    );

    let pb = ProgressBar::new(count as u64);

    for analyzed_commit in revwalk {
        client.index(analyzed_commit?).expect("unable to index");
        pb.inc(1);
    }

    pb.finish_with_message("done");
    Ok(())
}
