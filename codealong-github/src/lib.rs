extern crate regex;
#[macro_use]
extern crate lazy_static;
extern crate reqwest;
#[macro_use]
extern crate serde_derive;
extern crate serde;
extern crate serde_json;

mod client;
mod cursor;
mod pull_request;

use client::Client;
use pull_request::PullRequest;

pub fn walk(repo_and_owner: &str, token: &str) {
    let client = Client::new(token.to_owned());
    let path = format!("repos/{}/pulls", repo_and_owner);
    let res = client
        .build_request(&path)
        .query(&[("state", "all")])
        .send()
        .unwrap()
        .json::<Vec<PullRequest>>();
    println!("{:?}", res);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_walk() {
        let token = "09cf75651dc8e726894eb72a0c25830228ef073e";
        walk("getoutreach/outreach", token);
    }
}
