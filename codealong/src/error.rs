use git2;
use serde_yaml;
use std::io;

error_chain!{
    foreign_links {
        Git2(git2::Error);
        IO(io::Error);
        Config(serde_yaml::Error);
    }
}
