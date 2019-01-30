use git2;
use serde_yaml;
use std::io;

error_chain! {
    errors {
        InvalidRepo(repo: String) {
            description("invalid repo")
            display("invalid repo: '{}'", repo)
        }
    }

    foreign_links {
        Git2(git2::Error);
        IO(io::Error);
        Config(serde_yaml::Error);
    }
}
