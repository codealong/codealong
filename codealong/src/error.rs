#![allow(deprecated)]
use git2;
use serde_yaml;
use std::io;

error_chain! {
    errors {
        InvalidRepo(repo: String) {
            description("invalid repo")
            display("invalid repo: '{}'", repo)
        }
        BlameError(message: String) {
            description("error running git blame")
            display("blame error: {}", message)
        }
    }

    foreign_links {
        Git2(git2::Error);
        IO(io::Error);
        Config(serde_yaml::Error);
    }
}
