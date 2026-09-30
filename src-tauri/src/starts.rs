//! How you start sessions from the quick panel: for each repo, whether your latest starts went
//! into a new worktree or an existing one. Kept in `starts.json` next to the config. route.rs
//! weighs Jev's guess at a new worktree against this, since the wording of a prompt says little
//! about how you like to work (most prompts read as a feature or a fix).
//!
//! Only starts where a new worktree was on offer count: a Claude Code account starting in a
//! terminal or in the background. The apps choose their own worktrees.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::config::support_dir;

/// Starts kept per repo, the latest last: habits change, so old ones drop off.
const KEEP: usize = 20;

static WRITING: Mutex<()> = Mutex::new(());

#[derive(Serialize, Deserialize, Default, Debug, PartialEq)]
struct File {
    /// By `<project>/<repo>`: true for a start in a new worktree.
    #[serde(default)]
    repos: BTreeMap<String, Vec<bool>>,
}

pub fn path() -> PathBuf {
    support_dir().join("starts.json")
}

fn read() -> File {
    std::fs::read_to_string(path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

/// Notes a start in a repo, in a new worktree or not.
pub fn record(repo: &str, new_worktree: bool) -> Result<()> {
    let _held = WRITING.lock().unwrap_or_else(|e| e.into_inner());
    let mut file = read();
    push(&mut file, repo, new_worktree);
    std::fs::create_dir_all(support_dir())?;
    let p = path();
    let tmp = p.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(&file)?)?;
    std::fs::rename(&tmp, &p)?;
    Ok(())
}

fn push(file: &mut File, repo: &str, new_worktree: bool) {
    let list = file.repos.entry(repo.to_string()).or_default();
    list.push(new_worktree);
    let extra = list.len().saturating_sub(KEEP);
    list.drain(..extra);
}

pub use codealong_peer::api::Habit;

pub fn habit(repo: &str) -> Habit {
    habit_in(&read(), repo)
}

fn habit_in(file: &File, repo: &str) -> Habit {
    let list = file.repos.get(repo).map(Vec::as_slice).unwrap_or_default();
    Habit { new: list.iter().filter(|b| **b).count(), of: list.len() }
}

/// Starts in a repo that make a habit of new worktrees: at least this many, and at least half of
/// the latest starts there.
const FREQUENT: usize = 3;

/// Whether you often start new work in a new worktree in a repo. Only then does the quick panel
/// suggest one: elsewhere new work goes to a reusable worktree (src/route.ts), and the panel's
/// New worktree box is there for the time you want one.
pub fn frequent(h: Habit) -> bool {
    h.new >= FREQUENT && h.new * 2 >= h.of
}

/// Jev's probability that new work wants a worktree of its own, weighed by the habit: its odds
/// times the odds of your rate, the rate counted as if you'd also made one start each way, so a
/// repo you've never started in leaves Jev's answer as it is and a few starts move it a little.
pub fn weigh(p: f64, h: Habit) -> f64 {
    let p = p.clamp(0.001, 0.999);
    let rate = (h.new as f64 + 1.0) / (h.of as f64 + 2.0);
    let odds = p / (1.0 - p) * (rate / (1.0 - rate));
    odds / (1.0 + odds)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_the_latest_starts_per_repo() {
        let mut f = File::default();
        push(&mut f, "p/web", true);
        for _ in 0..KEEP {
            push(&mut f, "p/web", false);
        }
        push(&mut f, "p/api", true);
        assert_eq!(habit_in(&f, "p/web"), Habit { new: 0, of: KEEP });
        assert_eq!(habit_in(&f, "p/api"), Habit { new: 1, of: 1 });
        assert_eq!(habit_in(&f, "p/none"), Habit::default());
    }

    #[test]
    fn a_habit_is_three_new_worktrees_and_half_the_starts() {
        assert!(!frequent(Habit::default()));
        assert!(!frequent(Habit { new: 2, of: 2 }));
        assert!(frequent(Habit { new: 3, of: 5 }));
        assert!(frequent(Habit { new: 3, of: 6 }));
        assert!(!frequent(Habit { new: 3, of: 7 }));
    }

    #[test]
    fn no_history_leaves_jev_alone_and_a_habit_moves_it() {
        assert!((weigh(0.8, Habit::default()) - 0.8).abs() < 1e-9);
        // Nineteen of twenty in an existing worktree: a confident yes becomes a no.
        assert!(weigh(0.9, Habit { new: 1, of: 20 }) < 0.5);
        // Nearly always a new worktree: an unsure answer becomes a yes.
        assert!(weigh(0.5, Habit { new: 18, of: 20 }) > 0.8);
        // Two starts only nudge it.
        let w = weigh(0.8, Habit { new: 0, of: 2 });
        assert!(w < 0.8 && w > 0.5);
    }
}
