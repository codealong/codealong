//! A PR's changed file opened in your editor, from its Changes tab (src/ui/PrFiles.tsx): as a diff,
//! or the file itself at a line.
//!
//! The diff is the file where the PR's branch left its base (their merge base, as GitHub's own
//! diff has it), or at the commit you last reviewed while Changes shows what changed since
//! (`Ask::since`), beside the file as the PR has it. When a worktree on this Mac holds the PR
//! (`PrItem.lanes`), that side is the worktree's own file, so what you change there is the real
//! thing, and it has whatever you haven't pushed yet; otherwise it's the file at the PR's head.
//! What's read from GitHub (`gh api …/contents`) goes into files of its own under the system's
//! temp folder, one folder a commit, read-only, so they're read once and can't be taken for the
//! working copy; folders older than a week go as new ones come. Opening the file itself needs a
//! worktree that holds the PR.
//!
//! The editor a diff opens in is yours when its command line takes one, else the first installed
//! that does (editors.rs `diff_editor`); the file opens in yours, at its line where it can.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};

use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;

use crate::model::Board;
use crate::{editors, procs};

/// What a window asks to open.
#[derive(Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Ask {
    /// The PR, `owner/name#number`.
    pub pr: String,
    pub path: String,
    /// Its path before a rename.
    pub previous: Option<String>,
    /// GitHub's word for what the PR did to it: `added`, `removed`, `renamed`, `modified`…
    pub status: String,
    /// `diff`, or `file`.
    pub how: String,
    /// The line a file opens at.
    pub line: Option<u64>,
    /// The commit a diff's side before is read at in place of the merge base: the one your last
    /// review was of, where Changes shows what changed since (`path`, `previous` and `status` are
    /// then the comparison's, so a file the PR added before it reads as there).
    pub since: Option<String>,
}

/// A path the PR names, made safe to put under a folder: relative, without `..`.
fn relative(path: &str) -> Result<PathBuf> {
    let p = Path::new(path);
    if path.is_empty() || !p.components().all(|c| matches!(c, Component::Normal(_))) {
        bail!("{path:?} isn't a path in the repo.");
    }
    Ok(p.to_path_buf())
}

/// A path in the repo, for a URL: each part percent-encoded, the slashes between kept.
fn url_path(path: &str) -> String {
    path.split('/')
        .map(|part| {
            part.bytes()
                .map(|b| match b {
                    b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => (b as char).to_string(),
                    _ => format!("%{b:02X}"),
                })
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// Where the PR is read from and against: its head commit and its merge base with its base.
#[derive(Clone, Debug, PartialEq)]
struct Ends {
    head: String,
    base: String,
}

/// What `gh` said of a PR's ends lately, by PR, so the next file of the same PR opens at once. A
/// head given by the board that differs from the one kept reads them again.
static ENDS: Mutex<Option<HashMap<String, (Ends, Instant)>>> = Mutex::new(None);
const ENDS_FOR: Duration = Duration::from_secs(120);

fn ends(slug: &str, number: u64, head: Option<&str>) -> Result<Ends> {
    let key = format!("{slug}#{number}");
    if let Some((e, at)) = ENDS.lock().unwrap_or_else(|e| e.into_inner()).as_ref().and_then(|m| m.get(&key)) {
        if at.elapsed() < ENDS_FOR && head.is_none_or(|h| e.head.starts_with(h) || h.starts_with(&e.head)) {
            return Ok(e.clone());
        }
    }
    let pulls = procs::run("gh", &["api", &format!("repos/{slug}/pulls/{number}"), "--jq", "[.head.sha, .base.sha] | join(\" \")"], None, Duration::from_secs(20)).context("Reading the PR from GitHub")?;
    let (head, base) = pulls.trim().split_once(' ').ok_or_else(|| anyhow!("GitHub didn't say where the PR's branch is."))?;
    let merge_base = procs::run("gh", &["api", &format!("repos/{slug}/compare/{base}...{head}"), "--jq", ".merge_base_commit.sha"], None, Duration::from_secs(20)).context("Comparing the PR with its base on GitHub")?;
    let e = Ends { head: head.to_string(), base: merge_base.trim().to_string() };
    if e.base.is_empty() {
        bail!("GitHub didn't say where the PR's branch left its base.");
    }
    ENDS.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashMap::new).insert(key, (e.clone(), Instant::now()));
    Ok(e)
}

/// The folder a PR's files read from GitHub go in.
fn store() -> PathBuf {
    std::env::temp_dir().join("codealong-diffs")
}

/// Folders of files read from GitHub that haven't been written to in a week go.
fn clear_old(store: &Path, now: SystemTime) {
    const WEEK: Duration = Duration::from_secs(7 * 24 * 3600);
    let Ok(prs) = std::fs::read_dir(store) else { return };
    for commit in prs.flatten().filter_map(|pr| std::fs::read_dir(pr.path()).ok()).flatten().flatten() {
        let old = commit.metadata().and_then(|m| m.modified()).is_ok_and(|at| now.duration_since(at).is_ok_and(|age| age > WEEK));
        if old {
            make_writable(&commit.path());
            let _ = std::fs::remove_dir_all(commit.path());
        }
    }
}

/// Gives a folder of read-only files back their write bit, so it can be removed.
fn make_writable(dir: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            make_writable(&p);
        } else {
            let _ = std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o644));
        }
    }
}

/// The folder of PR `slug#number`'s files.
fn pr_folder(store: &Path, slug: &str, number: u64) -> PathBuf {
    store.join(format!("{}-{number}", slug.replace('/', "-")))
}

/// A path as a file of its own under a folder: its name, in a folder named for the whole path as
/// written (FNV-1a, the same every run). So two paths the Mac's disk would take for one (`Foo.ts`
/// and `foo.ts`, which differ only in case), or a file at one and a folder at the other (`lib`,
/// `lib/util.ts`), never meet, and the editor still has the file's name and kind.
fn named(path: &Path) -> PathBuf {
    let hash = path.to_string_lossy().bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ b as u64).wrapping_mul(0x0100_0000_01b3));
    PathBuf::from(format!("{hash:016x}")).join(path.file_name().unwrap_or_default())
}

/// Where the file at `path` as of commit `sha` is kept, for PR `slug#number`.
fn kept_at(store: &Path, slug: &str, number: u64, sha: &str, path: &Path) -> PathBuf {
    let short: String = sha.chars().take(7).collect();
    pr_folder(store, slug, number).join(short).join(named(path))
}

/// The empty file that stands in for `path` on a side with no file.
fn empty_at(store: &Path, slug: &str, number: u64, path: &Path) -> PathBuf {
    pr_folder(store, slug, number).join("none").join(named(path))
}

/// Writes `bytes` at `file`, read-only, unless it's there already: a file at a commit never
/// changes.
fn keep(file: &Path, bytes: impl FnOnce() -> Result<Vec<u8>>) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    if file.is_file() {
        return Ok(());
    }
    let bytes = bytes()?;
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(file, bytes)?;
    std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o444))?;
    Ok(())
}

/// The file at `path` as of commit `sha`, as GitHub has it: its bytes as they are, text or not.
fn from_github(slug: &str, path: &str, sha: &str) -> Result<Vec<u8>> {
    procs::run_bytes("gh", &["api", "-H", "Accept: application/vnd.github.raw", &format!("repos/{slug}/contents/{}?ref={sha}", url_path(path))], Duration::from_secs(30)).with_context(|| format!("Reading {path} from GitHub"))
}

/// The file at `path` in `worktree`, if it's a file there and inside it: a link in the repo to
/// somewhere else isn't followed out of it.
fn in_worktree(worktree: &Path, path: &Path) -> Option<PathBuf> {
    let root = worktree.canonicalize().ok()?;
    let file = worktree.join(path).canonicalize().ok()?;
    (file.starts_with(&root) && file.is_file()).then_some(file)
}

/// The two sides of a file's diff, given where its PR is read from (`ends`), the worktree that
/// holds the PR, if one does, and how the file is read from GitHub (`read`, by path and commit).
/// A side the PR has no file on (added, removed) is an empty file.
fn sides(ask: &Ask, slug: &str, number: u64, e: &Ends, worktree: Option<&Path>, store: &Path, read: &dyn Fn(&str, &str) -> Result<Vec<u8>>) -> Result<(PathBuf, PathBuf)> {
    let path = relative(&ask.path)?;
    let before = match ask.previous.as_deref().filter(|p| !p.is_empty()) {
        Some(p) => p.to_string(),
        None => ask.path.clone(),
    };
    let from = ask.since.as_deref().filter(|c| !c.is_empty()).unwrap_or(&e.base);
    let old = if ask.status == "added" {
        let file = empty_at(store, slug, number, &path);
        keep(&file, || Ok(Vec::new()))?;
        file
    } else {
        let file = kept_at(store, slug, number, from, &relative(&before)?);
        keep(&file, || read(&before, from))?;
        file
    };
    let new = match worktree.and_then(|w| in_worktree(w, &path)) {
        Some(file) => file,
        None if ask.status == "removed" => {
            let file = empty_at(store, slug, number, &path);
            keep(&file, || Ok(Vec::new()))?;
            file
        }
        None => {
            let file = kept_at(store, slug, number, &e.head, &path);
            keep(&file, || read(&ask.path, &e.head))?;
            file
        }
    };
    Ok((old, new))
}

/// Where a PR is on this Mac, as the board has it: the worktree that holds it, if one does, and
/// the commit its branch is at.
pub struct Place {
    worktree: Option<PathBuf>,
    head: Option<String>,
}

/// Where the PR `key` is on `board`.
pub fn place(board: &Board, key: &str) -> Result<Place> {
    let pr = board.prs.iter().find(|p| p.key.eq_ignore_ascii_case(key)).ok_or_else(|| anyhow!("That PR isn't on the board any more."))?;
    let worktree = pr.lanes.iter().filter_map(|id| board.lanes.iter().find(|l| &l.id == id && l.device.is_none())).map(|l| PathBuf::from(&l.path)).find(|p| p.is_dir());
    Ok(Place { worktree, head: pr.head_oid.clone() })
}

/// Opens what `ask` says of a PR, which is at `place` on this Mac, in `yours` (`[apps] editor`)
/// or the editor a diff opens in. It can wait on GitHub, so it's called with no board held.
pub fn open(place: Place, yours: &str, ask: &Ask) -> Result<()> {
    let (slug, number) = ask.pr.rsplit_once('#').and_then(|(s, n)| Some((s, n.parse::<u64>().ok()?))).ok_or_else(|| anyhow!("{:?} isn't a PR.", ask.pr))?;
    let Place { worktree, head } = place;
    match ask.how.as_str() {
        "file" => {
            let worktree = worktree.ok_or_else(|| anyhow!("No worktree on this Mac holds #{number}, so there's no file of it to open."))?;
            let file = in_worktree(&worktree, &relative(&ask.path)?).ok_or_else(|| anyhow!("{} isn't in {} now.", ask.path, worktree.display()))?;
            editors::open_at(yours, &file, ask.line)
        }
        "diff" => {
            let editor = editors::diff_editor(yours).ok_or_else(|| anyhow!("None of your editors opens a diff from Codealong: VS Code, Cursor, Windsurf, Kiro and Zed do."))?;
            let e = ends(slug, number, head.as_deref())?;
            let store = store();
            clear_old(&store, SystemTime::now());
            let (old, new) = sides(ask, slug, number, &e, worktree.as_deref(), &store, &|path, sha| from_github(slug, path, sha))?;
            editors::diff(&editor.id, &old, &new)
        }
        other => bail!("Unknown way to open a file: {other:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[test]
    fn takes_only_paths_inside_the_repo() {
        assert_eq!(relative("src/a.ts").unwrap(), PathBuf::from("src/a.ts"));
        for bad in ["", "/etc/passwd", "../x", "src/../../x", "./a"] {
            assert!(relative(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn encodes_a_path_for_the_contents_api() {
        assert_eq!(url_path("src/a b/c#d?.ts"), "src/a%20b/c%23d%3F.ts");
        assert_eq!(url_path("docs/naïve.md"), "docs/na%C3%AFve.md");
    }

    fn ask(path: &str, status: &str, previous: Option<&str>) -> Ask {
        Ask { pr: "acme/app#7".into(), path: path.into(), previous: previous.map(String::from), status: status.into(), how: "diff".into(), line: None, since: None }
    }

    #[test]
    fn what_changed_since_your_review_is_read_from_the_commit_you_reviewed() {
        let store = crate::fixtures::scratch("prdiff-since");
        let e = Ends { head: "bbbbbbb2".into(), base: "aaaaaaa2".into() };
        let asked = RefCell::new(Vec::new());
        let read = |path: &str, sha: &str| {
            asked.borrow_mut().push(format!("{path}@{sha}"));
            Ok(Vec::new())
        };
        // A file the PR added before your review and changed after it is "modified" since, and
        // there at the commit you reviewed, which the merge base doesn't have.
        let since = Ask { since: Some("ccccccc2".into()), ..ask("src/new.ts", "modified", None) };
        let (old, _) = sides(&since, "acme/app", 7, &e, None, &store, &read).unwrap();
        assert_eq!(old, kept_at(&store, "acme/app", 7, "ccccccc2", Path::new("src/new.ts")));
        assert_eq!(*asked.borrow(), ["src/new.ts@ccccccc2", "src/new.ts@bbbbbbb2"]);
    }

    #[test]
    fn reads_each_side_once_at_its_commit_and_keeps_it_read_only() {
        let store = crate::fixtures::scratch("prdiff-sides");
        let e = Ends { head: "bbbbbbb1".into(), base: "aaaaaaa1".into() };
        let asked = RefCell::new(Vec::new());
        let read = |path: &str, sha: &str| {
            asked.borrow_mut().push(format!("{path}@{sha}"));
            Ok(format!("{path} at {sha}\n").into_bytes())
        };
        let (old, new) = sides(&ask("src/a.ts", "modified", None), "acme/app", 7, &e, None, &store, &read).unwrap();
        assert_eq!(old, kept_at(&store, "acme/app", 7, "aaaaaaa1", Path::new("src/a.ts")));
        assert_eq!(new, kept_at(&store, "acme/app", 7, "bbbbbbb1", Path::new("src/a.ts")));
        assert_eq!(std::fs::read_to_string(&new).unwrap(), "src/a.ts at bbbbbbb1\n");
        assert!(std::fs::metadata(&old).unwrap().permissions().readonly());
        // Again, nothing's read: a file at a commit never changes.
        sides(&ask("src/a.ts", "modified", None), "acme/app", 7, &e, None, &store, &read).unwrap();
        assert_eq!(*asked.borrow(), ["src/a.ts@aaaaaaa1", "src/a.ts@bbbbbbb1"]);
        // Each under its PR and commit, by its name, in a folder of its own.
        assert!(old.starts_with(store.join("acme-app-7/aaaaaaa")) && old.file_name().unwrap() == "a.ts", "{}", old.display());
        make_writable(&store);
    }

    #[test]
    fn two_paths_that_differ_only_in_case_are_two_files() {
        let store = crate::fixtures::scratch("prdiff-case");
        let e = Ends { head: "bbbbbbb1".into(), base: "aaaaaaa1".into() };
        let read = |path: &str, sha: &str| Ok(format!("{path} at {sha}\n").into_bytes());
        let (upper, _) = sides(&ask("src/Foo.ts", "modified", None), "acme/app", 7, &e, None, &store, &read).unwrap();
        let (lower, _) = sides(&ask("src/foo.ts", "modified", None), "acme/app", 7, &e, None, &store, &read).unwrap();
        assert_eq!(std::fs::read_to_string(upper).unwrap(), "src/Foo.ts at aaaaaaa1\n");
        assert_eq!(std::fs::read_to_string(lower).unwrap(), "src/foo.ts at aaaaaaa1\n");
        make_writable(&store);
    }

    #[test]
    fn a_worktree_that_holds_the_pr_is_the_new_side() {
        let store = crate::fixtures::scratch("prdiff-worktree");
        let wt = crate::fixtures::scratch("prdiff-wt");
        std::fs::create_dir_all(wt.join("src")).unwrap();
        std::fs::write(wt.join("src/a.ts"), "mine\n").unwrap();
        let e = Ends { head: "bbbbbbb1".into(), base: "aaaaaaa1".into() };
        let read = |path: &str, sha: &str| Ok(format!("{path} at {sha}\n").into_bytes());
        let (_, new) = sides(&ask("src/a.ts", "modified", None), "acme/app", 7, &e, Some(&wt), &store, &read).unwrap();
        assert_eq!(new, wt.join("src/a.ts").canonicalize().unwrap());
        // A file the worktree doesn't have (removed there, say) is read from the PR's head.
        let (_, gone) = sides(&ask("src/b.ts", "modified", None), "acme/app", 7, &e, Some(&wt), &store, &read).unwrap();
        assert_eq!(gone, kept_at(&store, "acme/app", 7, "bbbbbbb1", Path::new("src/b.ts")));
        // Nor is a link in the repo followed out of the worktree.
        let outside = crate::fixtures::scratch("prdiff-outside");
        std::fs::write(outside.join("secret"), "theirs\n").unwrap();
        std::os::unix::fs::symlink(outside.join("secret"), wt.join("src/link.ts")).unwrap();
        let (_, linked) = sides(&ask("src/link.ts", "modified", None), "acme/app", 7, &e, Some(&wt), &store, &read).unwrap();
        assert_eq!(linked, kept_at(&store, "acme/app", 7, "bbbbbbb1", Path::new("src/link.ts")));
        assert!(in_worktree(&wt, Path::new("src/link.ts")).is_none());
        make_writable(&store);
    }

    #[test]
    fn keeps_a_files_bytes_as_they_are() {
        let store = crate::fixtures::scratch("prdiff-bytes");
        let e = Ends { head: "bbbbbbb1".into(), base: "aaaaaaa1".into() };
        let png = vec![0x89, b'P', b'N', b'G', 0xff, 0x00, 0xfe];
        let read = |_: &str, _: &str| Ok(png.clone());
        let (old, _) = sides(&ask("logo.png", "modified", None), "acme/app", 7, &e, None, &store, &read).unwrap();
        assert_eq!(std::fs::read(old).unwrap(), png);
        make_writable(&store);
    }

    #[test]
    fn an_added_file_has_nothing_before_and_a_renamed_one_its_old_name() {
        let store = crate::fixtures::scratch("prdiff-status");
        let e = Ends { head: "bbbbbbb1".into(), base: "aaaaaaa1".into() };
        let read = |path: &str, sha: &str| Ok(format!("{path} at {sha}\n").into_bytes());
        let (old, _) = sides(&ask("src/new.ts", "added", None), "acme/app", 7, &e, None, &store, &read).unwrap();
        assert_eq!(std::fs::read_to_string(old).unwrap(), "");
        let (_, new) = sides(&ask("src/gone.ts", "removed", None), "acme/app", 7, &e, None, &store, &read).unwrap();
        assert_eq!(std::fs::read_to_string(new).unwrap(), "");
        let (old, new) = sides(&ask("src/to.ts", "renamed", Some("src/from.ts")), "acme/app", 7, &e, None, &store, &read).unwrap();
        assert_eq!(std::fs::read_to_string(old).unwrap(), "src/from.ts at aaaaaaa1\n");
        assert_eq!(std::fs::read_to_string(new).unwrap(), "src/to.ts at bbbbbbb1\n");
        assert!(sides(&ask("../x", "modified", None), "acme/app", 7, &e, None, &store, &read).is_err());
        // A file the PR makes a folder: each empty side is out of the other's way.
        let (none, util) = sides(&ask("lib/util.ts", "added", None), "acme/app", 7, &e, None, &store, &read).unwrap();
        let (lib, gone) = sides(&ask("lib", "removed", None), "acme/app", 7, &e, None, &store, &read).unwrap();
        assert!([&none, &util, &lib, &gone].iter().all(|f| f.is_file()), "{} {} {} {}", none.display(), util.display(), lib.display(), gone.display());
        assert_eq!(std::fs::read_to_string(&lib).unwrap(), "lib at aaaaaaa1\n");
        make_writable(&store);
    }

    #[test]
    fn clears_what_was_read_more_than_a_week_ago() {
        let store = crate::fixtures::scratch("prdiff-clear");
        let e = Ends { head: "bbbbbbb1".into(), base: "aaaaaaa1".into() };
        let read = |path: &str, sha: &str| Ok(format!("{path} at {sha}\n").into_bytes());
        let (old, new) = sides(&ask("src/a.ts", "modified", None), "acme/app", 7, &e, None, &store, &read).unwrap();
        clear_old(&store, SystemTime::now() + Duration::from_secs(3600));
        assert!(old.is_file() && new.is_file());
        clear_old(&store, SystemTime::now() + Duration::from_secs(8 * 24 * 3600));
        assert!(!old.exists() && !new.exists());
    }

    /// Reads a real PR file's two sides from GitHub as a diff would, without opening an editor:
    /// `PRDIFF_PR=owner/name#N PRDIFF_PATH=<a file it changes> cargo test prdiff_reads_github -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn prdiff_reads_github() {
        let pr = std::env::var("PRDIFF_PR").expect("PRDIFF_PR");
        let path = std::env::var("PRDIFF_PATH").expect("PRDIFF_PATH");
        let (slug, n) = pr.rsplit_once('#').unwrap();
        let e = ends(slug, n.parse().unwrap(), None).unwrap();
        println!("head {} merge base {}", e.head, e.base);
        let store = crate::fixtures::scratch("prdiff-real");
        let (old, new) = sides(&Ask { pr: pr.clone(), path, status: "modified".into(), how: "diff".into(), ..Ask::default() }, slug, n.parse().unwrap(), &e, None, &store, &|p, sha| from_github(slug, p, sha)).unwrap();
        let (a, b) = (std::fs::read_to_string(&old).unwrap(), std::fs::read_to_string(&new).unwrap());
        println!("{} lines before, {} after\n{}\n{}", a.lines().count(), b.lines().count(), old.display(), new.display());
        assert!(!a.is_empty() && !b.is_empty() && a != b);
        make_writable(&store);
    }

    #[test]
    fn reads_where_the_pr_is_from_github_once_a_while() {
        let calls = std::sync::Arc::new(Mutex::new(Vec::<String>::new()));
        let seen = calls.clone();
        procs::stand_in::set(Some(Box::new(move |_, args, _, _| {
            let url = args[1].to_string();
            seen.lock().unwrap().push(url.clone());
            Ok(if url.contains("/pulls/") { "head123 base456\n".into() } else { "merge789\n".into() })
        })));
        let e = ends("acme/ends", 9, Some("head123")).unwrap();
        assert_eq!(e, Ends { head: "head123".into(), base: "merge789".into() });
        ends("acme/ends", 9, Some("head123")).unwrap();
        assert_eq!(calls.lock().unwrap().len(), 2, "the second is kept");
        assert_eq!(calls.lock().unwrap()[1], "repos/acme/ends/compare/base456...head123");
        // The board says the branch moved: read again.
        ends("acme/ends", 9, Some("newhead")).unwrap();
        assert_eq!(calls.lock().unwrap().len(), 4);
        procs::stand_in::set(None);
    }
}
