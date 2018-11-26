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

pub use client::Client;
