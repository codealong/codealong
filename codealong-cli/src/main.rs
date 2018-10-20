#[macro_use]
extern crate clap;

extern crate codealong_elk;
extern crate console;
extern crate git2;
extern crate indicatif;

use codealong_elk::index;
use git2::Repository;

use indicatif::ProgressBar;

use console::{style, Emoji};

static LOOKING_GLASS: Emoji = Emoji("🔍 ", "");
static CHART_INCREASING: Emoji = Emoji("📈 ", "");

fn main() {
    use clap::App;

    let yml = load_yaml!("cli.yml");
    let matches = App::from_yaml(yml).get_matches();

    if let Some(matches) = matches.subcommand_matches("index") {
        index_command(matches);
    }
}

fn index_command(matches: &clap::ArgMatches) {
    let path = matches.value_of("repo_path").unwrap();
    let repo = Repository::discover(path).expect("unable to open repository");

    println!(
        "{} {}Calculating stats...",
        style("[1/2]").bold().dim(),
        LOOKING_GLASS
    );

    let count = calculate_size(&repo);

    println!(
        "{} {}Analyzing commits...",
        style("[2/2]").bold().dim(),
        CHART_INCREASING
    );

    let pb = ProgressBar::new(count);

    index(
        &repo,
        Some(&|| {
            pb.inc(1);
        }),
    );

    pb.finish_with_message("done");
}

fn calculate_size(repo: &Repository) -> u64 {
    let mut revwalk = repo.revwalk().unwrap();
    revwalk.push_head().unwrap();
    revwalk.count() as u64
}
