//! Worktrees marked reusable or not by hand: its menu's Reuse for new work, or `codealong
//! reuse on|off`. A reusable worktree takes one piece of work after another, where others are
//! kept for one piece of work each; Jev suggests new work only in reusable ones (src/route.ts).
//! A main checkout is reusable unless it's marked otherwise (`board::reusable`).
//!
//! Kept in `worktrees.json` next to the config, by path, and marked through `mark` from the menu
//! and the command alike: the command asks the app when it's up and writes the file under a
//! lock when it isn't, and the engine watches it, as it does `previews.json`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

use crate::config::support_dir;
use crate::model::{Lane, Origin};
use crate::store;

pub fn path() -> PathBuf {
    support_dir().join("worktrees.json")
}

/// What the file holds, for the error when it doesn't read as that.
const WHAT: &str = "a list of worktrees marked reusable";

/// One worktree's mark.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Mark {
    /// The worktree's path, as the board names it.
    pub worktree: String,
    pub reusable: bool,
    pub at: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Store {
    #[serde(default)]
    pub worktrees: Vec<Mark>,
}

impl Store {
    pub fn get(&self, worktree: &str) -> Option<bool> {
        self.worktrees.iter().find(|m| m.worktree == worktree).map(|m| m.reusable)
    }

    /// Marks a worktree reusable or not. False when it was marked so already.
    pub fn set(&mut self, worktree: &str, reusable: bool, at: i64) -> bool {
        if self.get(worktree) == Some(reusable) {
            return false;
        }
        self.worktrees.retain(|m| m.worktree != worktree);
        self.worktrees.push(Mark { worktree: worktree.to_string(), reusable, at });
        true
    }

    /// The marks by path, for the board.
    pub fn by_path(&self) -> HashMap<String, bool> {
        self.worktrees.iter().map(|m| (m.worktree.clone(), m.reusable)).collect()
    }
}

/// The store, or an empty one when there's no file. A file that doesn't parse is an error, so
/// it's never overwritten.
pub fn load() -> Result<Store> {
    Ok(store::read(&path(), WHAT)?.unwrap_or_default())
}

/// Marks a worktree reusable or not, for the app and the command alike. Only a repo's worktree in
/// one of your projects can be reusable (`board::can_reuse`): a plain folder, or a repo outside
/// every project, has no work of yours to reuse it for. False when it was marked so already.
pub fn mark(lane: &Lane, on: bool) -> Result<bool> {
    if on && !may_be_reusable(lane) {
        bail!("Only a repo's worktree in one of your projects can be reusable; {} isn't one.", crate::util::tilde(Path::new(&lane.id)));
    }
    let mut changed = false;
    change(|store| {
        changed = store.set(&lane.id, on, crate::util::now_ms());
        changed
    })?;
    Ok(changed)
}

fn may_be_reusable(lane: &Lane) -> bool {
    lane.project != crate::board::ELSEWHERE && lane.origin != Origin::Folder
}

/// Reads the store under a lock on `worktrees.json.lock`, lets `f` change it, and saves it
/// through a temporary file when `f` says it changed. Marks of worktrees that are gone go with
/// it.
pub fn change(f: impl FnOnce(&mut Store) -> bool) -> Result<Store> {
    store::change(&path(), WHAT, |s: &mut Store| {
        let changed = f(s);
        if changed {
            s.worktrees.retain(|m| Path::new(&m.worktree).exists());
        }
        changed
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_repos_worktree_in_your_projects_can_be_marked_reusable() {
        use crate::model::Lane;
        let elsewhere = Lane { project: crate::board::ELSEWHERE.into(), ..crate::fixtures::lane("/tmp/x", vec![]) };
        let folder = Lane { origin: Origin::Folder, ..crate::fixtures::lane("/Users/sam/notes", vec![]) };
        for lane in [elsewhere, folder] {
            let err = mark(&lane, true).unwrap_err();
            assert!(err.to_string().contains("Only a repo's worktree"), "{err}");
        }
        assert!(may_be_reusable(&crate::fixtures::lane("/r/app", vec![])));
    }

    #[test]
    fn a_mark_replaces_the_one_before_and_says_when_nothing_changed() {
        let mut s = Store::default();
        assert!(s.set("/r/app3", true, 1));
        assert!(!s.set("/r/app3", true, 2));
        assert!(s.set("/r/app3", false, 3));
        assert_eq!(s.worktrees, vec![Mark { worktree: "/r/app3".into(), reusable: false, at: 3 }]);
        assert_eq!(s.by_path().get("/r/app3"), Some(&false));
        assert_eq!(s.get("/r/other"), None);
    }

    #[test]
    fn reads_what_it_writes() {
        let s = Store { worktrees: vec![Mark { worktree: "/r/app3".into(), reusable: true, at: 5 }] };
        let text = serde_json::to_string_pretty(&s).unwrap();
        assert!(text.contains("\"reusable\": true"));
        assert_eq!(serde_json::from_str::<Store>(&text).unwrap(), s);
        assert_eq!(serde_json::from_str::<Store>("{}").unwrap(), Store::default());
    }
}
