//! Per-lane git state. Read-only: every command runs with `GIT_OPTIONAL_LOCKS=0` so it never
//! takes the index lock out from under you or an agent.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::model::GitInfo;
use crate::procs;

const TIMEOUT: Duration = Duration::from_secs(15);
/// How much of the end of a reflog is read at most: it keeps 90 days of entries by default, the
/// latest last, a few hundred KB for a busy worktree.
const REFLOG_TAIL: usize = 1024 * 1024;

fn git(path: &Path, args: &[&str]) -> Option<String> {
    let dir = path.to_string_lossy();
    let mut full = vec!["-C", dir.as_ref()];
    full.extend_from_slice(args);
    procs::run_background("git", &full, None, TIMEOUT).ok()
}

/// Status, last commit, and the commits and diff against `base` (e.g. `origin/main`).
pub fn status(path: &Path, base: Option<&str>) -> GitInfo {
    let mut info = GitInfo::default();
    if let Some(out) = git(path, &["status", "--porcelain=v2", "--branch"]) {
        let s = parse_status(&out);
        info.upstream = s.upstream;
        info.ahead = s.ahead;
        info.behind = s.behind;
        info.dirty = s.dirty;
    }
    if let Some(out) = git(path, &["log", "-1", "--format=%s%x1f%ct"]) {
        if let Some((subject, ts)) = out.trim_end().split_once('\u{1f}') {
            info.last_commit = Some(subject.to_string());
            info.last_commit_at = ts.parse::<i64>().ok().map(|s| s * 1000);
        }
    }
    if let Some(base) = base {
        info.base = Some(base.to_string());
        // A branch that tracks the default branch has its counts from status already.
        if info.upstream.as_deref() == Some(base) {
            info.base_ahead = Some(info.ahead);
            info.base_behind = Some(info.behind);
        } else if let Some(out) = git(path, &["rev-list", "--left-right", "--count", &format!("{base}...HEAD")]) {
            if let Some((behind, ahead)) = parse_left_right(&out) {
                info.base_ahead = Some(ahead);
                info.base_behind = Some(behind);
            }
        }
        // One process finds the merge base and diffs (git 2.30+); older gits take two.
        let stat = git(path, &["diff", "--shortstat", "--merge-base", base])
            .or_else(|| git(path, &["merge-base", "HEAD", base]).and_then(|mb| git(path, &["diff", "--shortstat", mb.trim()])));
        if let Some(stat) = stat {
            let (added, removed) = parse_shortstat(&stat);
            info.added = Some(added);
            info.removed = Some(removed);
        }
    }
    let starts = new_work(path, base);
    info.reset_at = starts.last().copied();
    info.new_work_count = Some(starts.len() as u32);
    info
}

/// When the worktree at `path` took new work, oldest first, from its HEAD's reflog: each time
/// HEAD went to another branch, or its branch was reset or made afresh (`git switch -C`,
/// `checkout -B`, `reset --hard origin/main`). A file read, not a git run. Empty when the
/// reflog has none, or can't be read.
pub fn new_work(path: &Path, base: Option<&str>) -> Vec<i64> {
    let Some(log) = git_dir(path).and_then(|d| std::fs::read(d.join("logs/HEAD")).ok()) else { return Vec::new() };
    let tail = &log[log.len().saturating_sub(REFLOG_TAIL)..];
    String::from_utf8_lossy(tail).lines().filter_map(|l| new_work_at(l, base)).collect()
}

/// A worktree's git folder: `.git` in a main checkout, the folder its `.git` file names in a
/// linked worktree.
fn git_dir(path: &Path) -> Option<PathBuf> {
    let dot = path.join(".git");
    if dot.is_dir() {
        return Some(dot);
    }
    crate::repos::linked_gitdir(path)
}

/// A reflog line's time, in ms, when it starts new work. A line is `<old> <new> <who> <seconds>
/// <zone>`, a tab, then what happened. Going to the branch you're on (`switch -C` writes that
/// after its reset) and resets anywhere but the default branch (undoing a commit) don't count.
pub fn new_work_at(line: &str, base: Option<&str>) -> Option<i64> {
    let (head, message) = line.split_once('\t')?;
    let mut parts = head.rsplitn(3, ' ');
    let _zone = parts.next()?;
    let seconds: i64 = parts.next()?.parse().ok()?;
    let new_work = if let Some(rest) = message.strip_prefix("checkout: moving from ") {
        rest.split_once(" to ").is_some_and(|(from, to)| from != to)
    } else if let Some(to) = message.strip_prefix("reset: moving to ") {
        base.is_some_and(|b| to == b || b.split_once('/').is_some_and(|(_, name)| to == name))
    } else {
        message.starts_with("branch: Reset to ") || message.starts_with("branch: Created from ")
    };
    new_work.then_some(seconds * 1000)
}

#[derive(Debug, Default, PartialEq)]
pub struct StatusSummary {
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub dirty: u32,
}

pub fn parse_status(out: &str) -> StatusSummary {
    let mut s = StatusSummary::default();
    for line in out.lines() {
        if let Some(u) = line.strip_prefix("# branch.upstream ") {
            s.upstream = Some(u.to_string());
        } else if let Some(ab) = line.strip_prefix("# branch.ab ") {
            let mut parts = ab.split_whitespace();
            s.ahead = parts.next().and_then(|a| a.trim_start_matches('+').parse().ok()).unwrap_or(0);
            s.behind = parts.next().and_then(|b| b.trim_start_matches('-').parse().ok()).unwrap_or(0);
        } else if line.starts_with("1 ") || line.starts_with("2 ") || line.starts_with("u ") || line.starts_with("? ") {
            s.dirty += 1;
        }
    }
    s
}

/// `rev-list --left-right --count A...B`: commits only in A, then only in B.
pub fn parse_left_right(out: &str) -> Option<(u32, u32)> {
    let mut parts = out.split_whitespace();
    let left = parts.next()?.parse().ok()?;
    let right = parts.next()?.parse().ok()?;
    Some((left, right))
}

pub fn parse_shortstat(out: &str) -> (u32, u32) {
    let mut added = 0;
    let mut removed = 0;
    for part in out.split(',') {
        let part = part.trim();
        let n: u32 = part.split_whitespace().next().and_then(|n| n.parse().ok()).unwrap_or(0);
        if part.contains("insertion") {
            added = n;
        } else if part.contains("deletion") {
            removed = n;
        }
    }
    (added, removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_porcelain_v2() {
        let out = "# branch.oid 0ef4ed2\n# branch.head quarry\n# branch.upstream ghempton/quarry\n# branch.ab +3 -1\n1 .M N... 100644 100644 100644 a b src/x.ts\n? new-file.ts\n? other.ts\n";
        assert_eq!(
            parse_status(out),
            StatusSummary { upstream: Some("ghempton/quarry".into()), ahead: 3, behind: 1, dirty: 3 }
        );
    }

    #[test]
    fn parses_left_right_counts() {
        assert_eq!(parse_left_right("42\t3\n"), Some((42, 3)));
        assert_eq!(parse_left_right("0\t0\n"), Some((0, 0)));
        assert_eq!(parse_left_right("fatal: bad revision\n"), None);
        assert_eq!(parse_left_right(""), None);
    }

    #[test]
    fn a_reflog_line_starts_new_work_when_the_branch_changes_or_goes_back_to_the_base() {
        let at = |m: &str| new_work_at(&format!("aaa bbb Some One <one@x.y> 1790785361 -0700\t{m}"), Some("origin/main"));
        assert_eq!(at("checkout: moving from app3 to feature"), Some(1_790_785_361_000));
        assert_eq!(at("branch: Reset to origin/main"), Some(1_790_785_361_000));
        assert_eq!(at("branch: Created from origin/main"), Some(1_790_785_361_000));
        assert_eq!(at("reset: moving to origin/main"), Some(1_790_785_361_000));
        assert_eq!(at("reset: moving to main"), Some(1_790_785_361_000));
        // Going to the branch it's on, undoing a commit, committing and pulling don't.
        assert_eq!(at("checkout: moving from app3 to app3"), None);
        assert_eq!(at("reset: moving to HEAD~1"), None);
        assert_eq!(at("reset: moving to HEAD"), None);
        assert_eq!(at("commit: Fix the thing"), None);
        assert_eq!(at("pull: Fast-forward"), None);
        assert_eq!(new_work_at("no tab here", None), None);
    }

    #[test]
    fn reads_when_a_linked_worktree_last_took_new_work() {
        let root = std::env::temp_dir().join(format!("codealong-reflog-{}", std::process::id()));
        let (wt, admin) = (root.join("app3"), root.join("main/.git/worktrees/app3"));
        std::fs::create_dir_all(admin.join("logs")).unwrap();
        std::fs::create_dir_all(&wt).unwrap();
        std::fs::write(wt.join(".git"), format!("gitdir: {}\n", admin.display())).unwrap();
        let line = |t: i64, m: &str| format!("aaa bbb A <a@b.c> {t} +0000\t{m}\n");
        let log = [line(100, "branch: Created from origin/main"), line(200, "commit: work"), line(300, "branch: Reset to origin/main"), line(301, "checkout: moving from app3 to app3"), line(400, "commit: more")].concat();
        std::fs::write(admin.join("logs/HEAD"), log).unwrap();
        assert_eq!(new_work(&wt, Some("origin/main")), vec![100_000, 300_000]);
        assert_eq!(new_work(&root.join("nowhere"), None), Vec::<i64>::new());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn parses_shortstat() {
        assert_eq!(parse_shortstat(" 12 files changed, 412 insertions(+), 88 deletions(-)\n"), (412, 88));
        assert_eq!(parse_shortstat(" 1 file changed, 5 deletions(-)\n"), (0, 5));
        assert_eq!(parse_shortstat(""), (0, 0));
    }
}
