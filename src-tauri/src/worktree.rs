//! Making a new git worktree for a session when its harness doesn't make one itself (Antigravity,
//! Codex, OpenCode, and hosted Codealong sessions). Claude Code (`-w`) and Cursor (`--worktree`)
//! keep making their own on the command line (`Adapter::makes_worktrees`).
//!
//! A worktree Codealong makes goes in `<main>/.codealong/worktrees/<slug>` on branch
//! `worktree-<slug>`, based on the repo's default remote branch (`repos::default_branch`, falling
//! back to `HEAD`) with `--no-track`, with `.codealong/worktrees/` added to the repo's
//! `info/exclude` so the main checkout's git status stays clean without hiding `.codealong/`
//! team settings. When a repo declares a `WorktreeCreate` hook that an account trusts
//! (`Adapter::setup_worktree`), that hook runs in git's place, matching how `cleanup.rs` runs
//! `WorktreeRemove` hooks on teardown.
//!
//! Every worktree Codealong creates is remembered in `made-worktrees.json` so sessions running in
//! it carry `Session::own_worktree` (`attach_own`) and Mark as done cleans it up via
//! `cleanup::after_end` when nothing in it would be lost.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime};

use anyhow::{anyhow, bail, Result};
use serde::{Deserialize, Serialize};

use crate::cleanup::Hook;
use crate::config::{support_dir, Config};
use crate::harness::{self, Ctx};
use crate::model::Session;
use crate::procs;
use crate::store;
use crate::util::{file_name, now_ms, tilde};

/// How long `git worktree add` gets: a large checkout can take tens of seconds on a busy Mac.
const GIT_WAIT: Duration = Duration::from_secs(120);

/// How long quick git checks (`rev-parse`) get.
const CHECK_WAIT: Duration = Duration::from_secs(10);

const WHAT: &str = "a list of worktrees Codealong made";

const ADJECTIVES: &[&str] = &[
    "amber", "brisk", "calm", "cedar", "clear", "coral", "crisp", "dusk", "fern", "flint",
    "fresh", "golden", "harbor", "hazel", "ivory", "jade", "kind", "lively", "maple", "meadow",
    "misty", "mossy", "noble", "olive", "pine", "quiet", "river", "sage", "silver", "steady",
    "sunny", "swift",
];

const NOUNS: &[&str] = &[
    "badger", "birch", "brook", "cairn", "cedar", "cliff", "cove", "crane", "creek", "finch",
    "fox", "glade", "grove", "harbor", "heron", "knoll", "lark", "lynx", "marsh", "otter",
    "owl", "peak", "plover", "quail", "ridge", "robin", "seal", "spruce", "swift", "trail",
    "vale", "wren",
];

/// What creating a worktree for a start would do, without touching git (`--start-plan` /
/// `--dry-run`).
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub main: PathBuf,
    pub base: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub hooks: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skipped: Option<String>,
}

/// A worktree Codealong just created for a session.
#[derive(Clone, Debug, PartialEq)]
pub struct Created {
    pub path: PathBuf,
    pub main: PathBuf,
    pub name: String,
    pub branch: Option<String>,
    pub by_hook: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
struct Entry {
    path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    branch: Option<String>,
    at: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
struct Store {
    #[serde(default)]
    worktrees: Vec<Entry>,
}

/// What creating a new worktree from `folder` would do, without modifying the repository.
pub fn plan(cfg: &Config, folder: &Path) -> Result<Plan> {
    let (_, main) = crate::repos::locate(folder).ok_or_else(|| anyhow!("{} isn't in a git repository, so Codealong can't make a worktree there.", tilde(folder)))?;
    let base = base_ref(&main);
    let (hooks, skipped) = setup_hooks(cfg, &main, "<slug>");
    Ok(Plan { main, base, hooks: hooks.into_iter().map(|h| h.command).collect(), skipped })
}

/// Creates a new worktree for the repo containing `folder`: runs the repo's `WorktreeCreate` hook
/// when one is declared and trusted, else `git worktree add` in `<main>/.codealong/worktrees/<slug>`.
pub fn create(cfg: &Config, folder: &Path) -> Result<Created> {
    let (_, main) = crate::repos::locate(folder).ok_or_else(|| anyhow!("{} isn't in a git repository, so Codealong can't make a worktree there.", tilde(folder)))?;
    let slug = pick_slug(&main);
    create_with_slug(cfg, &main, &slug)
}

fn create_with_slug(cfg: &Config, main: &Path, slug: &str) -> Result<Created> {
    let (hooks, skipped) = setup_hooks(cfg, main, slug);
    if hooks.is_empty() {
        if let Some(why) = skipped {
            bail!("{why}.");
        }
        return create_with_git(main, slug);
    }
    create_with_hooks(main, &hooks)
}

fn create_with_git(main: &Path, slug: &str) -> Result<Created> {
    ensure_excluded(main)?;
    let dir = main.join(".codealong").join("worktrees");
    std::fs::create_dir_all(&dir).map_err(|e| anyhow!("Couldn't make {}: {e}", tilde(&dir)))?;
    let path = dir.join(slug);
    let branch = format!("worktree-{slug}");
    let base = base_ref(main);
    let main_s = main.to_string_lossy().into_owned();
    let path_s = path.to_string_lossy().into_owned();
    procs::run("git", &["-C", &main_s, "worktree", "add", "--no-track", "-b", &branch, &path_s, &base], None, GIT_WAIT)
        .map_err(|e| anyhow!("Git couldn't make a worktree in {}: {}", tilde(main), last_line(&e.to_string())))?;
    let path = dunce::canonicalize(&path).unwrap_or(path);
    Ok(Created { path, main: main.to_path_buf(), name: slug.to_string(), branch: Some(branch), by_hook: false })
}

fn create_with_hooks(main: &Path, hooks: &[Hook]) -> Result<Created> {
    let mut created_path: Option<PathBuf> = None;
    for h in hooks {
        let out = procs::sh()
            .and_then(|sh| procs::run_in(&sh, &["-c", &h.command], &h.cwd, &h.env, &h.input, h.timeout))
            .map_err(|e| anyhow!("The repo's WorktreeCreate hook failed: {}", last_line(&e.to_string())))?;
        if !out.ok {
            let why = last_line(&out.err);
            let why = if why.is_empty() { last_line(&out.out) } else { why };
            bail!("The repo's WorktreeCreate hook failed: {why}");
        }
        if let Some(line) = out.out.lines().rev().map(str::trim).find(|l| !l.is_empty()) {
            let p = PathBuf::from(line);
            created_path = Some(if p.is_absolute() { p } else { main.join(p) });
        }
    }
    let path = created_path.ok_or_else(|| anyhow!("The repo's WorktreeCreate hook didn't print a worktree path."))?;
    if !path.is_dir() {
        bail!("The repo's WorktreeCreate hook printed {}, which isn't a folder.", tilde(&path));
    }
    let path = dunce::canonicalize(&path).unwrap_or(path);
    let name = file_name(&path);
    Ok(Created { path, main: main.to_path_buf(), name, branch: None, by_hook: true })
}

/// Cleans up a git-created worktree when launching its session fails right after creation.
/// Hook-made worktrees are left tracked (not torn down blindly here) so Clean up worktree or
/// WorktreeRemove can handle their repo-specific teardown safely.
pub fn rollback(created: &Created) {
    if created.by_hook {
        return;
    }
    forget(&created.path);
    let main = created.main.to_string_lossy();
    let path = created.path.to_string_lossy();
    let _ = procs::run("git", &["-C", &main, "worktree", "remove", &path], None, GIT_WAIT);
    if let Some(branch) = &created.branch {
        let _ = procs::run("git", &["-C", &main, "branch", "-d", branch], None, CHECK_WAIT);
    }
}

/// The ref a new worktree starts from: the repo's default remote branch (`origin/main`, etc.),
/// falling back to `HEAD` when there is no remote default branch.
fn base_ref(main: &Path) -> String {
    crate::repos::load_checkout(main).and_then(|c| crate::repos::default_branch(&c)).unwrap_or_else(|| "HEAD".into())
}

/// The repo's `WorktreeCreate` hooks from the first account whose harness would run them, and why
/// they won't run when hooks are declared that none would.
fn setup_hooks(cfg: &Config, main: &Path, name: &str) -> (Vec<Hook>, Option<String>) {
    let mut skipped = None;
    for account in &cfg.accounts {
        let Some(adapter) = harness::of(account) else { continue };
        match adapter.setup_worktree(&Ctx { account }, main, name) {
            Ok(hooks) if !hooks.is_empty() => return (hooks, None),
            Ok(_) => {}
            Err(why) => skipped = skipped.or(Some(why)),
        }
    }
    (Vec::new(), skipped)
}

/// Adds `.codealong/worktrees/` to the repo's `info/exclude` when not already listed, so worktrees
/// under `.codealong/worktrees/` never show as untracked files while `.codealong/config.toml` and
/// other committed team files remain visible to git.
fn ensure_excluded(main: &Path) -> Result<()> {
    let main_s = main.to_string_lossy();
    let git_dir = procs::run("git", &["-C", &main_s, "rev-parse", "--git-common-dir"], None, CHECK_WAIT)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .map(|s| {
            let p = PathBuf::from(s);
            if p.is_absolute() { p } else { main.join(p) }
        })
        .unwrap_or_else(|| main.join(".git"));
    ensure_excluded_in(&git_dir.join("info").join("exclude"))
}

fn ensure_excluded_in(exclude: &Path) -> Result<()> {
    let existing = std::fs::read_to_string(exclude).unwrap_or_default();
    if existing.lines().any(|l| matches!(l.trim(), ".codealong/worktrees/" | ".codealong/worktrees" | "/.codealong/worktrees/" | "/.codealong/worktrees")) {
        return Ok(());
    }
    if let Some(parent) = exclude.parent() {
        std::fs::create_dir_all(parent).map_err(|e| anyhow!("Couldn't prepare {}: {e}", tilde(parent)))?;
    }
    let mut next = existing;
    if !next.is_empty() && !next.ends_with('\n') {
        next.push('\n');
    }
    next.push_str(".codealong/worktrees/\n");
    std::fs::write(exclude, next).map_err(|e| anyhow!("Couldn't update {}: {e}", tilde(exclude)))
}

/// Picks a short, readable, unused slug for a worktree and branch in `main`.
fn pick_slug(main: &Path) -> String {
    let dir = main.join(".codealong").join("worktrees");
    let main_s = main.to_string_lossy().into_owned();
    let taken = |slug: &str| {
        if dir.join(slug).exists() {
            return true;
        }
        let branch = format!("refs/heads/worktree-{slug}");
        procs::run("git", &["-C", &main_s, "rev-parse", "--verify", "--quiet", &branch], None, CHECK_WAIT).is_ok()
    };
    let mut b = [0u8; 4];
    for attempt in 0..8 {
        crate::os::system::random_bytes(&mut b);
        let adj = ADJECTIVES[b[0] as usize % ADJECTIVES.len()];
        let noun = NOUNS[b[1] as usize % NOUNS.len()];
        let candidate = if attempt < 4 { format!("{adj}-{noun}") } else { format!("{adj}-{noun}-{:02x}{:02x}", b[2], b[3]) };
        if !taken(&candidate) {
            return candidate;
        }
    }
    format!("wt-{}", &crate::start::new_session_id()[..8])
}

fn last_line(s: &str) -> &str {
    s.lines().rev().map(str::trim).find(|l| !l.is_empty()).unwrap_or("")
}

// ── Tracking Codealong-created worktrees (made-worktrees.json) ───────────────────────────────

fn store_path() -> PathBuf {
    support_dir().join("made-worktrees.json")
}

struct Cache {
    mtime: Option<SystemTime>,
    paths: Vec<PathBuf>,
}

static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();

fn cache() -> &'static Mutex<Cache> {
    CACHE.get_or_init(|| Mutex::new(Cache { mtime: None, paths: Vec::new() }))
}

fn tracked_paths() -> Vec<PathBuf> {
    let path = store_path();
    let mtime = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
    let mut guard = cache().lock().unwrap_or_else(|e| e.into_inner());
    if guard.mtime == mtime && mtime.is_some() {
        return guard.paths.clone();
    }
    let loaded: Store = store::read(&path, WHAT).ok().flatten().unwrap_or_default();
    let paths: Vec<PathBuf> = loaded.worktrees.into_iter().map(|e| PathBuf::from(e.path)).collect();
    guard.mtime = mtime;
    guard.paths = paths.clone();
    paths
}

fn invalidate_cache() {
    if let Ok(mut guard) = cache().lock() {
        guard.mtime = None;
    }
}

/// Records `created` in `made-worktrees.json`, pruning entries whose folders are gone.
pub fn remember(created: &Created) {
    let entry_path = created.path.to_string_lossy().into_owned();
    if let Err(e) = store::change(&store_path(), WHAT, |s: &mut Store| {
        s.worktrees.retain(|e| e.path != entry_path && Path::new(&e.path).exists());
        s.worktrees.push(Entry { path: entry_path.clone(), branch: created.branch.clone(), at: now_ms() });
        true
    }) {
        eprintln!("codealong: couldn't save made-worktrees.json: {e:#}");
    }
    invalidate_cache();
}

/// Removes `path` from `made-worktrees.json` once the worktree is gone.
pub fn forget(path: &Path) {
    let real = dunce::canonicalize(path).ok();
    if let Err(e) = store::change(&store_path(), WHAT, |s: &mut Store| {
        let before = s.worktrees.len();
        s.worktrees.retain(|e| {
            let ep = Path::new(&e.path);
            ep != path && real.as_deref().is_none_or(|r| ep != r && dunce::canonicalize(ep).ok().as_deref() != Some(r))
        });
        s.worktrees.len() != before
    }) {
        eprintln!("codealong: couldn't update made-worktrees.json: {e:#}");
    }
    invalidate_cache();
}

/// Whether `path` is a worktree Codealong created (either under `.codealong/worktrees/` or
/// recorded in `made-worktrees.json`, e.g. from a `WorktreeCreate` hook).
pub fn is_tracked(path: &str) -> bool {
    let p = Path::new(path);
    if crate::repos::made_by_codealong(p) {
        return true;
    }
    let tracked = tracked_paths();
    tracked.iter().any(|t| t == p)
}

/// Attaches `Session::own_worktree` to any session without one whose `cwd` sits inside a worktree
/// Codealong created, so Mark as done cleans it up through `cleanup::after_end`.
pub fn attach_own(sessions: &mut [Session]) {
    let tracked = tracked_paths();
    attach_own_from(sessions, &tracked);
}

fn attach_own_from(sessions: &mut [Session], tracked: &[PathBuf]) {
    let tracked_set: HashSet<&Path> = tracked.iter().map(PathBuf::as_path).collect();
    for s in sessions {
        if s.own_worktree.is_some() {
            continue;
        }
        let cwd = Path::new(&s.cwd);
        // Deepest tracked worktree that holds `cwd`.
        let matched = tracked_set
            .iter()
            .filter(|w| cwd == **w || cwd.starts_with(**w))
            .max_by_key(|w| w.as_os_str().len())
            .map(|w| w.to_path_buf())
            .or_else(|| codealong_worktree_ancestor(cwd));
        if let Some(wt) = matched {
            s.own_worktree = Some(wt.to_string_lossy().into_owned());
        }
    }
}

/// The `.codealong/worktrees/<slug>` ancestor of `cwd`, if `cwd` sits inside one.
fn codealong_worktree_ancestor(cwd: &Path) -> Option<PathBuf> {
    let mut cur = Some(cwd);
    while let Some(d) = cur {
        if crate::repos::made_by_codealong(d) {
            return Some(d.to_path_buf());
        }
        cur = d.parent();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::session;

    fn git(dir: &Path, args: &[&str]) -> String {
        procs::run("git", &[&["-C", &dir.to_string_lossy()], args].concat(), None, CHECK_WAIT).expect("git command failed")
    }

    fn init_repo(name: &str) -> crate::fixtures::Scratch {
        let dir = crate::fixtures::scratch(name);
        git(&dir, &["init", "-b", "main"]);
        git(&dir, &["config", "user.email", "test@example.com"]);
        git(&dir, &["config", "user.name", "Test"]);
        std::fs::write(dir.join("README.md"), "hello\n").unwrap();
        git(&dir, &["add", "README.md"]);
        git(&dir, &["commit", "-m", "init"]);
        dir
    }

    #[test]
    fn creates_worktree_under_dot_codealong_and_excludes_it() {
        let main = init_repo("wt-create-git");
        let cfg = Config::default();
        let created = create_with_slug(&cfg, &main, "calm-otter").unwrap();
        assert!(!created.by_hook);
        assert_eq!(created.name, "calm-otter");
        assert_eq!(created.branch.as_deref(), Some("worktree-calm-otter"));
        assert!(created.path.ends_with(".codealong/worktrees/calm-otter"));
        assert!(created.path.is_dir());
        assert!(crate::repos::made_by_codealong(&created.path));

        // Only `.codealong/worktrees/` is excluded, so main's git status stays clean while
        // committed team settings under `.codealong/config.toml` remain visible to git.
        let exclude = std::fs::read_to_string(main.join(".git/info/exclude")).unwrap();
        assert!(exclude.lines().any(|l| l.trim() == ".codealong/worktrees/"));
        assert!(git(&main, &["status", "--porcelain"]).trim().is_empty());
        std::fs::write(main.join(".codealong/config.toml"), "review = true\n").unwrap();
        let st = git(&main, &["status", "--porcelain", "-uall"]);
        assert!(st.contains(".codealong/config.toml"), "expected config.toml in status, got: {st}");
        assert!(!st.contains(".codealong/worktrees"), "worktrees should be excluded, got: {st}");
        std::fs::remove_file(main.join(".codealong/config.toml")).unwrap();

        // Calling ensure_excluded again doesn't duplicate the entry.
        ensure_excluded(&main).unwrap();
        let exclude2 = std::fs::read_to_string(main.join(".git/info/exclude")).unwrap();
        assert_eq!(exclude2.lines().filter(|l| l.trim() == ".codealong/worktrees/").count(), 1);

        // Rollback removes both the worktree folder and its branch.
        rollback(&created);
        assert!(!created.path.exists());
        assert!(procs::run("git", &["-C", &main.to_string_lossy(), "rev-parse", "--verify", "refs/heads/worktree-calm-otter"], None, CHECK_WAIT).is_err());
    }

    #[test]
    fn bases_new_worktree_on_origin_default_branch_when_present() {
        let root = crate::fixtures::scratch("wt-create-remote");
        let bare = root.join("origin.git");
        std::fs::create_dir_all(&bare).unwrap();
        git(&bare, &["init", "--bare", "-b", "main"]);

        let main = root.join("app");
        std::fs::create_dir_all(&main).unwrap();
        git(&main, &["init", "-b", "main"]);
        git(&main, &["config", "user.email", "test@example.com"]);
        git(&main, &["config", "user.name", "Test"]);
        std::fs::write(main.join("a.txt"), "v1\n").unwrap();
        git(&main, &["add", "a.txt"]);
        git(&main, &["commit", "-m", "v1"]);
        git(&main, &["remote", "add", "origin", &bare.to_string_lossy()]);
        git(&main, &["push", "-u", "origin", "main"]);
        let remote_head = git(&main, &["rev-parse", "origin/main"]).trim().to_string();

        // Advance local main ahead of origin/main; new worktree should still start from origin/main.
        std::fs::write(main.join("a.txt"), "v2\n").unwrap();
        git(&main, &["add", "a.txt"]);
        git(&main, &["commit", "-m", "local only"]);

        let p = plan(&Config::default(), &main).unwrap();
        assert_eq!(p.base, "origin/main");

        let created = create_with_slug(&Config::default(), &main, "sage-fox").unwrap();
        let wt_head = git(&created.path, &["rev-parse", "HEAD"]).trim().to_string();
        assert_eq!(wt_head, remote_head);
        // `--no-track` keeps the new branch from tracking `origin/main`.
        assert!(procs::run("git", &["-C", &main.to_string_lossy(), "config", "branch.worktree-sage-fox.remote"], None, CHECK_WAIT).is_err());
    }

    #[test]
    fn runs_worktree_create_hook_when_trusted_and_refuses_when_untrusted() {
        let main = init_repo("wt-create-hook");
        let home = crate::fixtures::scratch("wt-create-hook-home");
        std::fs::create_dir_all(main.join(".claude")).unwrap();
        let target = main.join(".codealong/hook-made");
        let script = format!("mkdir -p '{}' && echo '{}'", target.display(), target.display());
        std::fs::write(
            main.join(".claude/settings.json"),
            serde_json::json!({
                "hooks": {
                    "WorktreeCreate": [{
                        "hooks": [{ "type": "command", "command": script }]
                    }]
                }
            })
            .to_string(),
        )
        .unwrap();

        let cfg: Config = toml::from_str(&format!("[[accounts]]\nname = \"claude\"\nharness = \"claude\"\nhome = {:?}\n", home.to_string_lossy())).unwrap();

        // Untrusted account refuses to run the hook.
        let err = create_with_slug(&cfg, &main, "brisk-lark").unwrap_err().to_string();
        assert!(err.contains("doesn't trust"), "expected trust error, got: {err}");
        assert!(!target.exists());

        // Plan reports skipped without touching disk.
        let p = plan(&cfg, &main).unwrap();
        assert!(p.hooks.is_empty());
        assert!(p.skipped.as_deref().is_some_and(|s| s.contains("doesn't trust")));

        // Trust the repo for the Claude account and retry: hook runs and returns the path.
        crate::harness::of(&cfg.accounts[0]).unwrap().before_start(&cfg.accounts[0], &main);
        let p2 = plan(&cfg, &main).unwrap();
        assert_eq!(p2.hooks.len(), 1);
        assert!(p2.skipped.is_none());

        let created = create_with_slug(&cfg, &main, "brisk-lark").unwrap();
        assert!(created.by_hook);
        assert_eq!(created.path, dunce::canonicalize(&target).unwrap());
    }

    #[test]
    fn attaches_own_worktree_for_tracked_and_codealong_paths() {
        let wt1 = PathBuf::from("/r/app/.codealong/worktrees/calm-otter");
        let wt2 = PathBuf::from("/tmp/hook-wt");
        let mut s1 = session("antigravity:ag:1");
        s1.cwd = "/r/app/.codealong/worktrees/calm-otter/sub".into();
        let mut s2 = session("codex:cx:2");
        s2.cwd = "/tmp/hook-wt".into();
        let mut s3 = session("claude:cl:3");
        s3.cwd = "/r/app/.codealong/worktrees/calm-otter".into();
        s3.own_worktree = Some("/r/app/.claude/worktrees/existing".into());
        let mut s4 = session("antigravity:ag:4");
        s4.cwd = "/r/app".into();

        let mut sessions = vec![s1, s2, s3, s4];
        attach_own_from(&mut sessions, &[wt2.clone()]);
        assert_eq!(sessions[0].own_worktree.as_deref(), Some(wt1.to_str().unwrap()));
        assert_eq!(sessions[1].own_worktree.as_deref(), Some("/tmp/hook-wt"));
        // Existing own_worktree is never overwritten.
        assert_eq!(sessions[2].own_worktree.as_deref(), Some("/r/app/.claude/worktrees/existing"));
        assert_eq!(sessions[3].own_worktree, None);
    }
}
