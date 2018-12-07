extern crate chrono;
extern crate codealong;
#[macro_use]
extern crate error_chain;
extern crate reqwest;
#[macro_use]
extern crate serde_derive;
extern crate serde;
extern crate serde_json;

mod client;
mod error;
mod event;

pub use crate::client::Client;
pub use crate::error::{Error, ErrorKind};
