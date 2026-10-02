//! What Codealong on one device says to Codealong on another. The Mac app's engine (src-tauri)
//! and apps that only watch, such as the phone app (codealong-ios), build on the same code, so
//! the two sides can't drift apart:
//!
//! - `proto`: the messages, and each side's proof that it holds its device key.
//! - `feed`: a device's worktrees and sessions as it sends them, and the copy another keeps.
//! - `model`: the worktrees and sessions themselves, as the board shows them.
//! - `net`: link codes, and reaching a device anywhere through iroh by its key.
//! - `wire`: a connection, whatever carries it.
//! - `api`: what devices ask each other for, and what comes back.
//! - `client`: a device that only watches: it links, keeps feeds and asks, and answers nothing.
//!
//! `fixtures` is the tests' own: the sessions, lanes and scratch folders they build on, which the
//! engine's tests compile too.

pub mod api;
pub mod client;
pub mod feed;
#[cfg(test)]
pub(crate) mod fixtures;
pub mod model;
pub mod net;
pub mod proto;
pub mod wire;
