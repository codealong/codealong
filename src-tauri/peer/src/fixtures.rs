//! What the tests build their sessions and lanes from, and the folders they write in. A test
//! says only what it's about (`Session { status: Status::Needs, ..session("a") }`) and the rest
//! comes from here, over the model's own constructors (`Session::new`, `Lane::new`), so a field
//! added to a session or a lane is given its neutral value there and no test has to say it.
//!
//! The engine's tests can't see a `#[cfg(test)]` item of this crate, so the engine compiles this
//! same file into its own tests (`#[path]` in its lib.rs). That's one set of fixtures for both
//! crates, in neither's build of the app. It names only `crate::model`, which both have.
//!
//! Everything here is made up: no real project, repo, account or person is named, and a test
//! shouldn't name one either.

// Each crate's tests use some of these, not all.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use crate::model::{Harness, Lane, Origin, Session};

/// A Claude Code session of the account `claude`, open in a terminal in `/r` and idle. Its key
/// is taken as it's given, so a test can call its sessions `a` and `b`, and its id is the key
/// too; its title and the target that opens it follow from the key. A session that should have
/// a real key's parts is `Session::new`'s.
pub fn session(key: &str) -> Session {
    Session {
        key: key.to_string(),
        live: true,
        title: format!("Title {key}"),
        cwd: "/r".to_string(),
        open_target: format!("s|{key}|focus"),
        ..Session::new(Harness::Claude, "claude", key)
    }
}

/// A repo's main checkout at `id`, in the project `P` and the repo `r`, idle, with these
/// sessions and no branch, PR or git state. Its callsign is its path without the leading slash.
pub fn lane(id: &str, sessions: Vec<Session>) -> Lane {
    Lane { callsign: id.trim_start_matches('/').to_string(), origin: Origin::Main, sessions, ..Lane::new(id, "P", "r") }
}

/// A folder of a test's own: made empty under the temp folder, and removed when the test lets
/// go of it, passing or not. It stands in wherever a path does.
pub struct Scratch(PathBuf);

/// A fresh, empty folder for a test to write in. `tag` says whose it is, for a folder left
/// behind by a run that was killed; two calls never get the same folder, whatever their tags.
pub fn scratch(tag: &str) -> Scratch {
    static N: AtomicU32 = AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!("codealong-{tag}-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a scratch folder");
    Scratch(dir)
}

impl std::ops::Deref for Scratch {
    type Target = PathBuf;
    fn deref(&self) -> &PathBuf {
        &self.0
    }
}

impl AsRef<Path> for Scratch {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
