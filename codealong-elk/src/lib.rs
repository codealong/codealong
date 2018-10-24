extern crate chrono;
extern crate codealong;
extern crate git2;
extern crate reqwest;
#[macro_use]
extern crate serde_derive;
extern crate serde;
extern crate serde_json;

mod client;
mod event;

use client::Client;
use codealong::walk;
use git2::Repository;

use event::Event;

pub fn index(repo: &Repository, cb: Option<&Fn()>) {
    let client = Client::new("http://localhost:9200");
    for result in walk(&repo) {
        let commit = result.unwrap();
        let event = Event::new(commit);
        client.index(&event).expect("unable to index");
        if let Some(cb) = cb {
            cb()
        }
    }
}
