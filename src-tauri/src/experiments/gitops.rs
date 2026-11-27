//! Git snapshots, diff blinding and gate command execution for experiment trials.
//!
//! Snapshots use a temporary `GIT_INDEX_FILE` so uncommitted and untracked files are captured
//! against `base_sha` without touching the worktree's real index or running repository commit
//! hooks. Each final snapshot is also written as an unattached commit under
//! `refs/codealong/experiments/<run_id>/<trial_id>` so discarding a worktree never loses work.

use std::path::Path;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};

use crate::procs;

const GIT_WAIT: Duration = Duration::from_secs(60);

/// Result of snapshotting a trial worktree against `base_sha`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub tree: String,
    pub diff: String,
    pub git_ref: Option<String>,
}

/// Result of running a mechanical gate command in a worktree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GateResult {
    pub passed: bool,
    pub exit_code: i32,
    pub output: String,
}

/// Resolves `git_ref` (e.g. `HEAD`, `origin/main`, a commit sha) to a full 40-char commit SHA.
pub fn resolve_sha(repo: &Path, git_ref: &str) -> Result<String> {
    let repo_s = repo.to_string_lossy();
    let out = procs::run("git", &["-C", &repo_s, "rev-parse", "--verify", &format!("{git_ref}^{{commit}}")], None, GIT_WAIT)
        .map_err(|e| anyhow!("Couldn't resolve {git_ref:?} in {}: {e}", repo.display()))?;
    Ok(out.trim().to_string())
}

/// Computes the git tree SHA of the entire working tree (tracked + untracked, honoring `.gitignore`)
/// using a temporary index so the worktree's real index is never touched.
pub fn tree_hash(worktree: &Path) -> Result<String> {
    let (tree, _) = with_temp_index(worktree, None, None, None)?;
    Ok(tree)
}

/// Snapshots the working tree against `base_sha`, returning its tree SHA, unified diff, and
/// (when `save_ref` is given) a git ref `refs/codealong/experiments/<run_id>/<trial_id>` pointing
/// to a commit of that tree on top of `base_sha`.
pub fn snapshot(worktree: &Path, base_sha: &str, save_ref: Option<(&str, &str)>) -> Result<Snapshot> {
    let (tree, diff) = with_temp_index(worktree, Some(base_sha), None, None)?;
    let git_ref = match save_ref {
        Some((run_id, trial_id)) => Some(save_commit_ref(worktree, base_sha, &tree, run_id, trial_id)?),
        None => None,
    };
    Ok(Snapshot { tree, diff, git_ref })
}

fn with_temp_index(worktree: &Path, diff_against: Option<&str>, _run_id: Option<&str>, _trial_id: Option<&str>) -> Result<(String, String)> {
    let mut b = [0u8; 6];
    crate::os::system::random_bytes(&mut b);
    let idx = std::env::temp_dir().join(format!("codealong-exp-idx-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}", b[0], b[1], b[2], b[3], b[4], b[5]));
    let idx_s = idx.to_string_lossy().into_owned();
    let env = vec![("GIT_INDEX_FILE".to_string(), idx_s)];
    let run_git = |args: &[&str]| -> Result<String> {
        let out = procs::run_in("git", args, worktree, &env, "", GIT_WAIT)?;
        if !out.ok {
            let msg = out.err.trim();
            anyhow::bail!("git {} failed: {}", args.join(" "), if msg.is_empty() { out.out.trim() } else { msg });
        }
        Ok(out.out)
    };
    let result = (|| -> Result<(String, String)> {
        // Seed temp index from HEAD when present, then stage all tracked + untracked files.
        let _ = run_git(&["read-tree", "HEAD"]);
        run_git(&["add", "-A"])?;
        let tree = run_git(&["write-tree"])?.trim().to_string();
        let diff = match diff_against {
            Some(base) => run_git(&["diff", "--cached", "--binary", base])?,
            None => String::new(),
        };
        Ok((tree, diff))
    })();
    let _ = std::fs::remove_file(&idx);
    result
}

fn save_commit_ref(worktree: &Path, base_sha: &str, tree: &str, run_id: &str, trial_id: &str) -> Result<String> {
    let env = vec![
        ("GIT_AUTHOR_NAME".into(), "Codealong Experiment".into()),
        ("GIT_AUTHOR_EMAIL".into(), "experiment@codealong.local".into()),
        ("GIT_COMMITTER_NAME".into(), "Codealong Experiment".into()),
        ("GIT_COMMITTER_EMAIL".into(), "experiment@codealong.local".into()),
    ];
    let msg = format!("Codealong experiment snapshot {run_id}/{trial_id}");
    let out = procs::run_in("git", &["commit-tree", tree, "-p", base_sha, "-m", &msg], worktree, &env, "", GIT_WAIT)?;
    if !out.ok {
        anyhow::bail!("git commit-tree failed: {}", out.err.trim());
    }
    let commit = out.out.trim();
    let ref_name = format!("refs/codealong/experiments/{run_id}/{trial_id}");
    let upd = procs::run_in("git", &["update-ref", &ref_name, commit], worktree, &[], "", GIT_WAIT)?;
    if !upd.ok {
        anyhow::bail!("git update-ref failed: {}", upd.err.trim());
    }
    Ok(ref_name)
}

/// Exports the tracked repository tree at `base_sha` into `dest` using a temporary
/// `GIT_INDEX_FILE` (`git read-tree` + `git checkout-index`).
///
/// Leaves NO `.git` directory in `dest`, touches neither the repository's real index nor its
/// working tree, and exposes zero branch names, trial refs, or commit history — giving a blind
/// judge full read-only codebase context at `base_sha`.
pub fn export_tree(repo: &Path, base_sha: &str, dest: &Path) -> Result<()> {
    if dest.exists() {
        std::fs::remove_dir_all(dest).with_context(|| format!("clearing {}", dest.display()))?;
    }
    std::fs::create_dir_all(dest).with_context(|| format!("creating {}", dest.display()))?;
    let abs = dunce::canonicalize(dest).unwrap_or_else(|_| dest.to_path_buf());
    let mut prefix = abs.to_string_lossy().replace('\\', "/");
    if !prefix.ends_with('/') {
        prefix.push('/');
    }
    let mut b = [0u8; 6];
    crate::os::system::random_bytes(&mut b);
    let idx = std::env::temp_dir().join(format!(
        "codealong-exp-export-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        b[0], b[1], b[2], b[3], b[4], b[5]
    ));
    let env = vec![("GIT_INDEX_FILE".to_string(), idx.to_string_lossy().into_owned())];
    let prefix_arg = format!("--prefix={prefix}");
    let result = (|| -> Result<()> {
        let rt = procs::run_in("git", &["read-tree", base_sha], repo, &env, "", GIT_WAIT)?;
        if !rt.ok {
            let msg = rt.err.trim();
            anyhow::bail!("git read-tree {base_sha} failed: {}", if msg.is_empty() { rt.out.trim() } else { msg });
        }
        let co = procs::run_in("git", &["checkout-index", "-a", "-f", &prefix_arg], repo, &env, "", GIT_WAIT)?;
        if !co.ok {
            let msg = co.err.trim();
            anyhow::bail!("git checkout-index failed: {}", if msg.is_empty() { co.out.trim() } else { msg });
        }
        Ok(())
    })();
    let _ = std::fs::remove_file(&idx);
    result
}

/// Removes worktree-path clues and harness markers from a diff before handing it to a blind judge.
pub fn blind_diff(diff: &str, worktree_path: Option<&str>) -> String {
    let mut out = diff.to_string();
    if let Some(wt) = worktree_path.filter(|s| !s.is_empty()) {
        out = out.replace(wt, "<worktree>");
        if let Some(slug) = Path::new(wt).file_name().and_then(|n| n.to_str()).filter(|s| !s.is_empty()) {
            out = out.replace(&format!(".codealong/worktrees/{slug}"), "<worktree>");
            out = out.replace(&format!(".claude/worktrees/{slug}"), "<worktree>");
        }
    }
    out
}

/// Runs a mechanical gate command through the user's login shell in `worktree`.
pub fn run_gate(worktree: &Path, cmd: &str, timeout_secs: u64) -> Result<GateResult> {
    let sh = procs::sh().context("finding shell for gate command")?;
    let wait = Duration::from_secs(timeout_secs.max(10));
    let out = procs::run_in(&sh, &["-c", cmd], worktree, &[], "", wait)?;
    let mut combined = String::new();
    if !out.out.trim().is_empty() {
        combined.push_str(out.out.trim_end());
    }
    if !out.err.trim().is_empty() {
        if !combined.is_empty() {
            combined.push_str("\n--- stderr ---\n");
        }
        combined.push_str(out.err.trim_end());
    }
    // Keep the tail bounded so artifacts and judge notes stay readable.
    const MAX_BYTES: usize = 32 * 1024;
    if combined.len() > MAX_BYTES {
        let cut = combined.floor_char_boundary(combined.len() - MAX_BYTES);
        combined = format!("…(truncated)\n{}", &combined[cut..]);
    }
    let exit_code = if out.ok { 0 } else { 1 };
    Ok(GateResult { passed: out.ok, exit_code, output: combined })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn init_repo(dir: &Path) -> String {
        let s = dir.to_string_lossy().into_owned();
        procs::run("git", &["init", "-b", "main", &s], None, GIT_WAIT).unwrap();
        procs::run("git", &["-C", &s, "config", "user.name", "Test"], None, GIT_WAIT).unwrap();
        procs::run("git", &["-C", &s, "config", "user.email", "test@example.com"], None, GIT_WAIT).unwrap();
        std::fs::write(dir.join("a.txt"), "hello\n").unwrap();
        procs::run("git", &["-C", &s, "add", "a.txt"], None, GIT_WAIT).unwrap();
        procs::run("git", &["-C", &s, "commit", "-m", "init"], None, GIT_WAIT).unwrap();
        resolve_sha(dir, "HEAD").unwrap()
    }

    #[test]
    fn snapshot_captures_uncommitted_and_untracked_without_touching_index() {
        let dir = crate::fixtures::scratch("exp-gitops");
        let base = init_repo(&dir);
        std::fs::write(dir.join("a.txt"), "hello world\n").unwrap();
        std::fs::write(dir.join("new.txt"), "brand new\n").unwrap();

        let snap = snapshot(&dir, &base, Some(("exp-1", "t1"))).unwrap();
        assert!(snap.diff.contains("hello world"), "{}", snap.diff);
        assert!(snap.diff.contains("brand new"), "{}", snap.diff);
        assert_eq!(snap.git_ref.as_deref(), Some("refs/codealong/experiments/exp-1/t1"));

        // Real git index stays unstaged.
        let staged = procs::run("git", &["-C", &dir.to_string_lossy(), "diff", "--cached", "--name-only"], None, GIT_WAIT).unwrap();
        assert!(staged.trim().is_empty(), "real index was modified: {staged}");

        // Saved ref resolves to a real commit with our tree.
        let ref_sha = resolve_sha(&dir, "refs/codealong/experiments/exp-1/t1").unwrap();
        assert_ne!(ref_sha, base);

        // Tree hash matches and detects subsequent edits.
        assert_eq!(tree_hash(&dir).unwrap(), snap.tree);
        std::fs::write(dir.join("a.txt"), "modified by reviewer\n").unwrap();
        assert_ne!(tree_hash(&dir).unwrap(), snap.tree);
    }

    #[test]
    fn blind_diff_scrubs_worktree_paths() {
        let raw = "path: /tmp/repo/.codealong/worktrees/amber-fox/src/a.rs\nother: .claude/worktrees/amber-fox/b.rs";
        let blinded = blind_diff(raw, Some("/tmp/repo/.codealong/worktrees/amber-fox"));
        assert!(!blinded.contains("amber-fox"), "{blinded}");
        assert!(blinded.contains("<worktree>"), "{blinded}");
    }

    #[test]
    fn export_tree_extracts_base_commit_without_git_metadata_or_later_changes() {
        let dir = crate::fixtures::scratch("exp-export");
        let repo = dir.join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let s = repo.to_string_lossy().into_owned();
        procs::run("git", &["init", "-b", "main", &s], None, GIT_WAIT).unwrap();
        procs::run("git", &["-C", &s, "config", "user.name", "Test"], None, GIT_WAIT).unwrap();
        procs::run("git", &["-C", &s, "config", "user.email", "test@example.com"], None, GIT_WAIT).unwrap();
        std::fs::create_dir_all(repo.join("sub")).unwrap();
        std::fs::write(repo.join("a.txt"), "base-a\n").unwrap();
        std::fs::write(repo.join("sub/b.txt"), "base-b\n").unwrap();
        procs::run("git", &["-C", &s, "add", "."], None, GIT_WAIT).unwrap();
        procs::run("git", &["-C", &s, "commit", "-m", "base"], None, GIT_WAIT).unwrap();
        let base = resolve_sha(&repo, "HEAD").unwrap();

        // Add a later commit, uncommitted edit, untracked file, and experiment ref.
        std::fs::write(repo.join("a.txt"), "later-a\n").unwrap();
        std::fs::write(repo.join("later.txt"), "secret\n").unwrap();
        procs::run("git", &["-C", &s, "add", "."], None, GIT_WAIT).unwrap();
        procs::run("git", &["-C", &s, "commit", "-m", "later"], None, GIT_WAIT).unwrap();
        std::fs::write(repo.join("untracked.txt"), "dirty\n").unwrap();
        let _ = snapshot(&repo, &base, Some(("exp-1", "t1"))).unwrap();

        let dest = dir.join("judge/p0-fwd/repo");
        export_tree(&repo, &base, &dest).unwrap();

        assert_eq!(std::fs::read_to_string(dest.join("a.txt")).unwrap(), "base-a\n");
        assert_eq!(std::fs::read_to_string(dest.join("sub/b.txt")).unwrap(), "base-b\n");
        assert!(!dest.join(".git").exists(), "exported tree must have no .git metadata");
        assert!(!dest.join("later.txt").exists(), "later commit files must not leak");
        assert!(!dest.join("untracked.txt").exists(), "untracked files must not leak");

        // Real repo index stays clean (still at `later` commit).
        let staged = procs::run("git", &["-C", &s, "diff", "--cached", "--name-only"], None, GIT_WAIT).unwrap();
        assert!(staged.trim().is_empty(), "real index was modified: {staged}");
    }
}
