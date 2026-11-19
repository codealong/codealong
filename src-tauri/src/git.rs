//! Per-lane git state. Read-only: every command runs with `GIT_OPTIONAL_LOCKS=0` so it never
//! takes the index lock out from under you or an agent.

use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::model::GitInfo;
use crate::procs;

const TIMEOUT: Duration = Duration::from_secs(15);
/// How much of the end of a reflog is read at most: it keeps 90 days of entries by default, the
/// latest last, a few hundred KB for a busy worktree.
const REFLOG_TAIL: usize = 1024 * 1024;

#[cfg(test)]
thread_local! {
    /// The git processes this thread ran, for tests to count.
    static RUNS: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

fn git(path: &Path, args: &[&str]) -> Option<String> {
    let dir = path.to_string_lossy();
    #[cfg(test)]
    RUNS.with(|r| r.set(r.get() + 1));
    let mut full = vec!["-C", dir.as_ref()];
    full.extend_from_slice(args);
    procs::run_background("git", &full, None, TIMEOUT).ok()
}

/// Status, last commit, and the commits and diff against `base` (e.g. `origin/main`).
///
/// Most refreshes come from edits to the working tree, which move only what `status` and the
/// diff say. What the commits say (the last one, the counts against `base`, what's unpushed)
/// depends only on HEAD, `base` and the remote branches, so it's taken from the last refresh's
/// memo while none of those moved (`Memo`, `reusable`), and the diff too while the tree is
/// clean. A refresh after an edit runs two processes rather than five.
pub fn status(path: &Path, base: Option<&str>) -> GitInfo {
    let mut info = GitInfo::default();
    let mut head = None;
    if let Some(out) = git(path, &["status", "--porcelain=v2", "--branch"]) {
        let s = parse_status(&out);
        info.upstream = s.upstream;
        info.ahead = s.ahead;
        info.behind = s.behind;
        info.dirty = s.dirty;
        head = s.oid;
    }
    // The refs are stamped after status read HEAD: a ref that moves in between moves the stamp
    // the next refresh compares with, never one this refresh trusts.
    let key = head.and_then(|head| Some(Key { head, base: base.map(str::to_string), refs: refs_stamp(&common_dir(path)?)? }));
    let prior = memos().get(path).cloned().unwrap_or_default();
    let reuse = reusable(&prior, key.as_ref(), info.dirty, Instant::now());
    let mut memo = Memo { key: key.clone(), at: if reuse.commits { prior.at } else { Some(Instant::now()) }, reflog: prior.reflog.clone(), ..Memo::default() };

    memo.last_commit = prior.last_commit.filter(|_| reuse.commits).or_else(|| {
        let out = git(path, &["log", "-1", "--format=%s%x1f%ct"])?;
        let (subject, ts) = out.trim_end().split_once('\u{1f}')?;
        Some((subject.to_string(), ts.parse::<i64>().ok().map(|s| s * 1000)))
    });
    if let Some((subject, at)) = &memo.last_commit {
        info.last_commit = Some(subject.clone());
        info.last_commit_at = *at;
    }
    if let Some(base) = base {
        info.base = Some(base.to_string());
        // A branch that tracks the default branch has its counts from status already.
        if info.upstream.as_deref() == Some(base) {
            info.base_ahead = Some(info.ahead);
            info.base_behind = Some(info.behind);
        } else {
            memo.base_counts = prior.base_counts.filter(|_| reuse.commits).or_else(|| {
                let out = git(path, &["rev-list", "--left-right", "--count", &format!("{base}...HEAD")])?;
                let (behind, ahead) = parse_left_right(&out)?;
                Some((ahead, behind))
            });
            if let Some((ahead, behind)) = memo.base_counts {
                info.base_ahead = Some(ahead);
                info.base_behind = Some(behind);
            }
        }
        // A clean tree's diff is HEAD's commit against the base, read from the commit status
        // named rather than the files, so an edit made and undone between the two can't be kept
        // as what the clean tree says (`stat_clean`). A dirty tree's is the files'.
        let committed = key.as_ref().map(|k| k.head.clone()).filter(|_| info.dirty == 0);
        memo.stat = prior.stat.filter(|_| reuse.stat).or_else(|| {
            let stat = match &committed {
                Some(head) => git(path, &["diff", "--shortstat", &format!("{base}...{head}")])?,
                // One process finds the merge base and diffs (git 2.30+); older gits take two.
                None => git(path, &["diff", "--shortstat", "--merge-base", base])
                    .or_else(|| git(path, &["merge-base", "HEAD", base]).and_then(|mb| git(path, &["diff", "--shortstat", mb.trim()])))?,
            };
            Some(parse_shortstat(&stat))
        });
        memo.stat_clean = memo.stat.is_some() && committed.is_some();
        if let Some((added, removed)) = memo.stat {
            info.added = Some(added);
            info.removed = Some(removed);
        }
    }
    let starts = memo.new_work(path, base);
    info.reset_at = starts.last().copied();
    info.new_work_count = Some(starts.len() as u32);
    info.unpushed = if info.base_ahead == Some(0) {
        Some(0)
    } else {
        memo.unpushed = prior.unpushed.filter(|_| reuse.commits).or_else(|| unpushed(path, base, info.base_ahead));
        memo.unpushed
    };
    info.locked = locked(path);
    memos().insert(path.to_path_buf(), memo);
    info
}

/// Commits HEAD has that no remote branch has, but for those whose change is in `base` already
/// (`unpushed_by`). The default branch is a remote's (`origin/main`), so a HEAD it holds has
/// none, and git isn't run again to say so.
pub fn unpushed(path: &Path, base: Option<&str>, base_ahead: Option<u32>) -> Option<u32> {
    if base_ahead == Some(0) {
        return Some(0);
    }
    unpushed_by(&|args| git(path, args).ok_or_else(|| "git failed".to_string()), base).ok()
}

/// Commits HEAD has that no remote branch has (`git rev-list HEAD --not --remotes`), with git
/// run by `run`: they'd go with the worktree. None counts when what they change is in `base`
/// (the default branch) already (`landed`): a squash or rebase merge puts a branch's work there
/// in commits of its own and leaves the branch's on no remote. A commit that changes nothing, an
/// empty one, holds nothing either. cleanup.rs runs it with a git whose failure keeps the
/// worktree.
pub fn unpushed_by(run: &dyn Fn(&[&str]) -> Result<String, String>, base: Option<&str>) -> Result<u32, String> {
    let count = run(&["rev-list", "--count", "HEAD", "--not", "--remotes"])?.trim().parse::<u32>().map_err(|_| "its count of commits didn't read".to_string())?;
    if count > 0 && base.is_some_and(|base| landed(run, base)) {
        return Ok(0);
    }
    Ok(count)
}

/// Whether all HEAD changed since it parted from `base` is in `base` as it is in HEAD: no file
/// HEAD changed differs between the two. Trees are compared by their objects, so no merge
/// driver, filter or external diff runs. `base` having changed one of those files again since
/// says it isn't, as does histories with nothing in common or a git that fails.
fn landed(run: &dyn Fn(&[&str]) -> Result<String, String>, base: &str) -> bool {
    let changed = |from: &str| run(&["diff-tree", "-r", "-z", "--name-only", from, "HEAD"]).map(|out| out.split('\0').filter(|p| !p.is_empty()).map(String::from).collect::<std::collections::HashSet<_>>());
    let Ok(fork) = run(&["merge-base", base, "HEAD"]) else { return false };
    match (changed(fork.trim()), changed(base)) {
        (Ok(own), Ok(apart)) => own.is_disjoint(&apart),
        _ => false,
    }
}

/// What the commit facts of a worktree's last refresh were read under: they hold while HEAD,
/// the base asked about and every ref in the repository stay as they were.
#[derive(Clone, Debug, PartialEq)]
struct Key {
    /// HEAD's commit, as status says it (`# branch.oid`).
    head: String,
    base: Option<String>,
    /// The refs as `refs_stamp` found them.
    refs: u64,
}

/// A worktree's last refresh, for the next to take what can't have changed (`status`).
#[derive(Clone, Debug, Default)]
struct Memo {
    /// None when the refresh couldn't say (status failed, the git folder isn't where it's
    /// looked for): nothing of it is taken.
    key: Option<Key>,
    /// When the commit facts were read: they're read again after `MEMO_FOR` whatever the key
    /// says, in case something moved a ref without the stamp showing it.
    at: Option<Instant>,
    /// The last commit's subject and time.
    last_commit: Option<(String, Option<i64>)>,
    /// Commits ahead of and behind the base, when they took a run of their own.
    base_counts: Option<(u32, u32)>,
    /// Lines added and removed against the base's merge base.
    stat: Option<(u32, u32)>,
    /// `stat` was read with nothing changed in the tree, so it's what the commits alone say.
    stat_clean: bool,
    unpushed: Option<u32>,
    /// The new work read from HEAD's reflog, with the base it was read for and the stamp of the
    /// reflog it was read from.
    reflog: Option<(u64, Option<String>, Vec<i64>)>,
}

/// The longest the commit facts are taken from a memo before they're read again.
const MEMO_FOR: Duration = Duration::from_secs(300);

/// What a refresh may take from the last one's memo instead of running git again.
#[derive(Debug, Default, PartialEq)]
struct Reuse {
    /// The last commit, the counts against the base and what's unpushed.
    commits: bool,
    /// The diff against the base's merge base.
    stat: bool,
}

/// What can be taken from `prior` by a refresh that found `key`, with `dirty` changes in the
/// tree: the commit facts when nothing they hang on moved and they're not old, and the diff too
/// when the tree is clean and was when it was read (with edits, the diff moves with them).
fn reusable(prior: &Memo, key: Option<&Key>, dirty: u32, now: Instant) -> Reuse {
    let fresh = prior.at.is_some_and(|at| now.saturating_duration_since(at) < MEMO_FOR);
    let commits = fresh && key.is_some() && prior.key.as_ref() == key;
    Reuse { commits, stat: commits && dirty == 0 && prior.stat_clean }
}

impl Memo {
    /// `new_work`, read again only when HEAD's reflog or the base changed.
    fn new_work(&mut self, path: &Path, base: Option<&str>) -> Vec<i64> {
        let stamp = git_dir(path).and_then(|d| stamp_of(&d.join("logs/HEAD")));
        if let (Some(stamp), Some((was, was_base, starts))) = (stamp, &self.reflog) {
            if stamp == *was && was_base.as_deref() == base {
                return starts.clone();
            }
        }
        let starts = new_work(path, base);
        self.reflog = stamp.map(|s| (s, base.map(str::to_string), starts.clone()));
        starts
    }
}

fn memos() -> std::sync::MutexGuard<'static, HashMap<PathBuf, Memo>> {
    static MEMOS: OnceLock<Mutex<HashMap<PathBuf, Memo>>> = OnceLock::new();
    MEMOS.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner())
}

/// A file's or folder's size and time, hashed: what moves when it's written.
fn stamp_of(path: &Path) -> Option<u64> {
    let m = std::fs::metadata(path).ok()?;
    let mut h = DefaultHasher::new();
    (m.len(), m.modified().ok()).hash(&mut h);
    Some(h.finish())
}

/// The folder a repository's refs are kept in: `.git` in a main checkout, the one a linked
/// worktree's git folder names in its `commondir`.
fn common_dir(path: &Path) -> Option<PathBuf> {
    let dot = path.join(".git");
    if dot.is_dir() {
        return Some(dot);
    }
    let gitdir = crate::repos::linked_gitdir(path)?;
    let common = std::fs::read_to_string(gitdir.join("commondir")).ok()?;
    let common = PathBuf::from(common.trim());
    Some(if common.is_absolute() { common } else { gitdir.join(common) })
}

/// A stamp of every ref in the repository whose refs are in `common`: `packed-refs`, a reftable's
/// list of tables, the `shallow` file, and the time of each folder under `refs/`. Git writes a loose ref to a lock
/// file and renames it into place, which moves its folder's time, so a branch made, moved or
/// removed (a commit, a fetch, a push) moves the stamp. None when there's no `refs/` folder.
fn refs_stamp(common: &Path) -> Option<u64> {
    let mut h = DefaultHasher::new();
    stamp_of(&common.join("packed-refs")).hash(&mut h);
    stamp_of(&common.join("reftable/tables.list")).hash(&mut h);
    // A deepened or unshallowed clone's history changes with no ref moving.
    stamp_of(&common.join("shallow")).hash(&mut h);
    let mut dirs = vec![common.join("refs")];
    let mut seen = 0;
    while let Some(dir) = dirs.pop() {
        let entries = std::fs::read_dir(&dir).ok()?;
        dir.hash(&mut h);
        stamp_of(&dir)?.hash(&mut h);
        seen += 1;
        let mut under: Vec<PathBuf> = entries.flatten().filter(|e| e.file_type().is_ok_and(|t| t.is_dir())).map(|e| e.path()).collect();
        under.sort();
        dirs.extend(under);
    }
    (seen > 0).then(|| h.finish())
}

/// Why a linked worktree is locked (`git worktree lock`), from its git folder's `locked` file,
/// which holds the reason; empty for a lock given none. None when it isn't locked, and for a
/// main checkout, which can't be.
pub fn locked(path: &Path) -> Option<String> {
    let dir = crate::repos::linked_gitdir(path)?;
    std::fs::read_to_string(dir.join("locked")).ok().map(|reason| reason.trim().to_string())
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
    /// HEAD's commit; none on a branch with no commits yet (`(initial)`).
    pub oid: Option<String>,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub dirty: u32,
}

pub fn parse_status(out: &str) -> StatusSummary {
    let mut s = StatusSummary::default();
    for line in out.lines() {
        if let Some(oid) = line.strip_prefix("# branch.oid ") {
            s.oid = Some(oid.trim().to_string()).filter(|o| o != "(initial)");
        } else if let Some(u) = line.strip_prefix("# branch.upstream ") {
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
        let out = "# branch.oid 0ef4ed2\n# branch.head portal\n# branch.upstream gwen/portal\n# branch.ab +3 -1\n1 .M N... 100644 100644 100644 a b src/x.ts\n? new-file.ts\n? other.ts\n";
        assert_eq!(
            parse_status(out),
            StatusSummary { oid: Some("0ef4ed2".into()), upstream: Some("gwen/portal".into()), ahead: 3, behind: 1, dirty: 3 }
        );
        assert_eq!(parse_status("# branch.oid (initial)\n# branch.head main\n").oid, None);
    }

    /// The commit facts are taken from the last refresh only while HEAD, the base and the refs
    /// are as they were and the memo isn't old; the diff only while the tree is clean and was.
    #[test]
    fn a_refresh_takes_from_the_last_only_what_cant_have_moved() {
        let t0 = Instant::now();
        let key = |head: &str, base: Option<&str>, refs: u64| Key { head: head.into(), base: base.map(str::to_string), refs };
        let k = key("abc", Some("origin/main"), 7);
        let memo = |stat_clean: bool| Memo { key: Some(k.clone()), at: Some(t0), stat_clean, ..Memo::default() };
        let all = Reuse { commits: true, stat: true };
        let commits_only = Reuse { commits: true, stat: false };
        let none = Reuse::default();
        assert_eq!(reusable(&memo(true), Some(&k), 0, t0), all, "nothing moved, a clean tree");
        assert_eq!(reusable(&memo(true), Some(&k), 2, t0), commits_only, "an edit moves the diff");
        assert_eq!(reusable(&memo(false), Some(&k), 0, t0), commits_only, "the diff was read with edits, since undone");
        assert_eq!(reusable(&memo(true), Some(&key("abd", Some("origin/main"), 7)), 0, t0), none, "a commit");
        assert_eq!(reusable(&memo(true), Some(&key("abc", Some("origin/dev"), 7)), 0, t0), none, "another base");
        assert_eq!(reusable(&memo(true), Some(&key("abc", Some("origin/main"), 8)), 0, t0), none, "a ref moved: a fetch, a push");
        assert_eq!(reusable(&memo(true), None, 0, t0), none, "status couldn't say");
        assert_eq!(reusable(&Memo::default(), Some(&k), 0, t0), none, "no refresh before");
        assert_eq!(reusable(&Memo { key: None, ..memo(true) }, Some(&k), 0, t0), none);
        assert_eq!(reusable(&memo(true), Some(&k), 0, t0 + MEMO_FOR), none, "too old");
    }

    fn run_git(dir: &Path, args: &[&str]) -> String {
        let out = std::process::Command::new("git").arg("-C").arg(dir).args(["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false"]).args(args).output().unwrap();
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    /// The refs' stamp moves with every way a ref moves, from a linked worktree's folder too, and
    /// a refresh after an edit says what one that ran everything would.
    #[test]
    fn the_refs_stamp_moves_with_the_refs_and_a_memo_says_what_git_would() {
        let root = crate::fixtures::scratch("refs-stamp");
        let main = root.join("main");
        std::fs::create_dir_all(&main).unwrap();
        run_git(&main, &["init", "-q", "-b", "main"]);
        std::fs::write(main.join("a.txt"), "one\n").unwrap();
        run_git(&main, &["add", "."]);
        run_git(&main, &["commit", "-qm", "first"]);
        let head = run_git(&main, &["rev-parse", "HEAD"]);
        run_git(&main, &["update-ref", "refs/remotes/origin/main", head.trim()]);
        run_git(&main, &["worktree", "add", "-q", "-b", "topic", "../wt"]);
        let wt = root.join("wt");
        let common = common_dir(&wt).unwrap();
        assert_eq!(dunce::canonicalize(&common).unwrap(), dunce::canonicalize(main.join(".git")).unwrap());
        let stamp = || refs_stamp(&common).unwrap();
        let mut was = stamp();
        assert_eq!(stamp(), was, "nothing moved");
        let mut moved = |what: &str, args: &[&str]| {
            run_git(&wt, args);
            let now = stamp();
            assert_ne!(now, was, "{what}");
            was = now;
        };
        std::fs::write(wt.join("a.txt"), "two\n").unwrap();
        moved("a commit", &["commit", "-qam", "second"]);
        moved("a remote branch moved", &["update-ref", "refs/remotes/origin/main", "HEAD"]);
        moved("a remote branch in a folder of its own", &["update-ref", "refs/remotes/origin/team/x", "HEAD"]);
        moved("a remote branch in that folder moved", &["update-ref", "refs/remotes/origin/team/x", "HEAD~1"]);
        moved("packed", &["pack-refs", "--all"]);
        moved("a packed ref moved", &["update-ref", "refs/remotes/origin/team/x", "HEAD"]);
        moved("removed", &["update-ref", "-d", "refs/remotes/origin/team/x"]);
        // A deepened clone's history changes with no ref moving; its `shallow` file says so.
        std::fs::write(common.join("shallow"), format!("{head}")).unwrap();
        assert_ne!(stamp(), was, "a clone made shallow");
        was = stamp();
        std::fs::remove_file(common.join("shallow")).unwrap();
        assert_ne!(stamp(), was, "a clone unshallowed");

        // An edit after a full refresh: the memo's answer is the one git gives afresh.
        let base = Some("origin/main");
        let runs = |f: &mut dyn FnMut()| {
            let before = RUNS.with(|r| r.get());
            f();
            RUNS.with(|r| r.get()) - before
        };
        // Here, the first refresh: status, log, rev-list and diff (HEAD is the base's, so nothing's
        // unpushed without asking).
        assert_eq!(runs(&mut || drop(status(&wt, base))), 4);
        assert_eq!(runs(&mut || drop(status(&wt, base))), 1, "nothing moved: status alone");
        std::fs::write(wt.join("a.txt"), "three\nfour\n").unwrap();
        let mut memoed = GitInfo::default();
        assert_eq!(runs(&mut || memoed = status(&wt, base)), 2, "an edit: status and the diff");
        memos().remove(&wt);
        let fresh = status(&wt, base);
        assert_eq!(memoed, fresh);
        assert_eq!((memoed.dirty, memoed.added, memoed.last_commit.as_deref()), (1, Some(2), Some("second")));
        // And after the edit's undone, the diff is read again rather than taken.
        std::fs::write(wt.join("a.txt"), "two\n").unwrap();
        let undone = status(&wt, base);
        assert_eq!((undone.dirty, undone.added, undone.base_ahead), (0, Some(0), Some(0)));
        // A fetch that moves the base (back to the first commit) moves the counts.
        run_git(&main, &["update-ref", "refs/remotes/origin/main", "HEAD"]);
        let after = status(&wt, base);
        assert_eq!((after.base_ahead, after.unpushed, after.added), (Some(1), Some(1), Some(1)));
        memos().remove(&wt);
        assert_eq!(status(&wt, base), after);
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
        let root = crate::fixtures::scratch("reflog");
        let (wt, admin) = (root.join("app3"), root.join("main/.git/worktrees/app3"));
        std::fs::create_dir_all(admin.join("logs")).unwrap();
        std::fs::create_dir_all(&wt).unwrap();
        std::fs::write(wt.join(".git"), format!("gitdir: {}\n", admin.display())).unwrap();
        let line = |t: i64, m: &str| format!("aaa bbb A <a@b.c> {t} +0000\t{m}\n");
        let log = [line(100, "branch: Created from origin/main"), line(200, "commit: work"), line(300, "branch: Reset to origin/main"), line(301, "checkout: moving from app3 to app3"), line(400, "commit: more")].concat();
        std::fs::write(admin.join("logs/HEAD"), log).unwrap();
        assert_eq!(new_work(&wt, Some("origin/main")), vec![100_000, 300_000]);
        assert_eq!(new_work(&root.join("nowhere"), None), Vec::<i64>::new());
    }

    #[test]
    fn parses_shortstat() {
        assert_eq!(parse_shortstat(" 12 files changed, 412 insertions(+), 88 deletions(-)\n"), (412, 88));
        assert_eq!(parse_shortstat(" 1 file changed, 5 deletions(-)\n"), (0, 5));
        assert_eq!(parse_shortstat(""), (0, 0));
    }
}
