#![allow(deprecated)]
use codealong;
use codealong_elk;
use codealong_github;

error_chain! {
    errors {
        InvalidRepo(repo: String) {
            description("invalid repo")
            display("invalid repo: '{}'", repo)
        }
    }

    foreign_links {
        IO(std::io::Error);
        Git2(git2::Error);
        Url(url::ParseError);
        ArgParse(std::num::ParseIntError);
        Config(serde_yaml::Error);
        DurationParse(humantime::DurationError);
    }

    links {
        Core(codealong::Error, codealong::ErrorKind);
        Elk(codealong_elk::Error, codealong_elk::ErrorKind);
        Github(codealong_github::Error, codealong_github::ErrorKind);
    }
}
