//! Per-lane git state. Read-only: every command runs with `GIT_OPTIONAL_LOCKS=0` so it never
//! takes the index lock out from under you or an agent.

use std::path::Path;
use std::time::Duration;

use crate::model::GitInfo;
use crate::procs;

const TIMEOUT: Duration = Duration::from_secs(15);

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
    info
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
    fn parses_shortstat() {
        assert_eq!(parse_shortstat(" 12 files changed, 412 insertions(+), 88 deletions(-)\n"), (412, 88));
        assert_eq!(parse_shortstat(" 1 file changed, 5 deletions(-)\n"), (0, 5));
        assert_eq!(parse_shortstat(""), (0, 0));
    }
}
