//! Repos and their worktrees: discovery under the scan roots, identity from the `origin`
//! remote, `git worktree list`, and the callsign and origin rules from *The model*.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::model::Origin;
use crate::procs;
use crate::util::file_name;

#[derive(Debug, Clone)]
pub struct Checkout {
    /// The main worktree.
    pub root: PathBuf,
    pub git_dir: PathBuf,
    /// Folder name of the main worktree, e.g. `atlas`.
    pub name: String,
    /// Normalized `origin` URL, e.g. `github.com/northwindai/atlas`.
    pub identity: Option<String>,
    /// `(owner, name)` when `origin` is on GitHub.
    pub github: Option<(String, String)>,
    /// Every GitHub owner that hosts this repo among the remotes (forks included), origin first.
    pub github_owners: Vec<String>,
}

impl Checkout {
    pub fn slug(&self) -> Option<String> {
        self.github.as_ref().map(|(o, n)| format!("{o}/{n}"))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Worktree {
    pub path: PathBuf,
    pub head: Option<String>,
    pub branch: Option<String>,
}

pub fn load_checkout(root: &Path) -> Option<Checkout> {
    let git_dir = root.join(".git");
    if !git_dir.is_dir() {
        return None;
    }
    let config = std::fs::read_to_string(git_dir.join("config")).unwrap_or_default();
    let remotes = parse_remotes(&config);
    let origin = remotes.iter().find(|(n, _)| n == "origin").or_else(|| remotes.first());
    let identity = origin.and_then(|(_, url)| normalize_url(url));
    let github = identity.as_deref().and_then(github_of);
    let mut github_owners = Vec::new();
    if let Some((owner, name)) = &github {
        github_owners.push(owner.clone());
        for (_, url) in &remotes {
            if let Some((o, n)) = normalize_url(url).as_deref().and_then(github_of) {
                if &n == name && !github_owners.contains(&o) {
                    github_owners.push(o);
                }
            }
        }
    }
    Some(Checkout { root: root.to_path_buf(), git_dir, name: file_name(root), identity, github, github_owners })
}

/// Finds main checkouts (folders with a `.git` directory) up to `depth` levels below each root.
pub fn discover(roots: &[PathBuf], depth: usize) -> Vec<Checkout> {
    fn walk(dir: &Path, depth: usize, out: &mut Vec<Checkout>) {
        let git = dir.join(".git");
        if git.is_dir() {
            if let Some(c) = load_checkout(dir) {
                out.push(c);
            }
            return;
        }
        if git.exists() || depth == 0 {
            return;
        }
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for e in entries.flatten() {
            let name = e.file_name();
            let name = name.to_string_lossy();
            if name.starts_with('.') || name == "node_modules" || name == "target" {
                continue;
            }
            if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                walk(&e.path(), depth - 1, out);
            }
        }
    }
    let mut out = Vec::new();
    for root in roots {
        walk(root, depth, &mut out);
    }
    out.sort_by(|a, b| a.root.cmp(&b.root));
    out.dedup_by(|a, b| a.root == b.root);
    out
}

/// The worktree root containing `path` and the main checkout it belongs to.
pub fn locate(path: &Path) -> Option<(PathBuf, PathBuf)> {
    let mut dir = Some(path);
    while let Some(d) = dir {
        let git = d.join(".git");
        if git.is_dir() {
            return Some((d.to_path_buf(), d.to_path_buf()));
        }
        if git.is_file() {
            let text = std::fs::read_to_string(&git).ok()?;
            let gitdir = PathBuf::from(text.trim().strip_prefix("gitdir:")?.trim());
            let gitdir = if gitdir.is_absolute() { gitdir } else { d.join(gitdir) };
            let common = std::fs::read_to_string(gitdir.join("commondir")).ok()?;
            let common = gitdir.join(common.trim());
            let common = std::fs::canonicalize(&common).unwrap_or(common);
            let main = common.parent()?.to_path_buf();
            return Some((d.to_path_buf(), main));
        }
        dir = d.parent();
    }
    None
}

pub fn parse_remotes(config: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut current: Option<String> = None;
    for line in config.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            current = t.strip_prefix("[remote \"").and_then(|r| r.strip_suffix("\"]")).map(String::from);
            continue;
        }
        if let Some(name) = &current {
            if let Some(rest) = t.strip_prefix("url") {
                if let Some(v) = rest.trim_start().strip_prefix('=') {
                    out.push((name.clone(), v.trim().to_string()));
                }
            }
        }
    }
    out
}

/// `git@github.com:owner/repo.git`, `https://github.com/owner/repo` and
/// `ssh://git@github.com/owner/repo` all become `github.com/owner/repo`.
pub fn normalize_url(url: &str) -> Option<String> {
    let u = url.trim().trim_end_matches('/');
    let (scheme_less, had_scheme) = match u.split_once("://") {
        Some((_, rest)) => (rest, true),
        None => (u, false),
    };
    let without_user = scheme_less.rsplit_once('@').map(|(_, r)| r).unwrap_or(scheme_less);
    let joined = if had_scheme {
        without_user.to_string()
    } else {
        // scp-like: host:owner/repo
        match without_user.split_once(':') {
            Some((host, path)) if !host.contains('/') => format!("{host}/{path}"),
            _ => without_user.to_string(),
        }
    };
    let joined = joined.strip_suffix(".git").unwrap_or(&joined).to_string();
    let (host, path) = joined.split_once('/')?;
    if host.is_empty() || path.is_empty() {
        return None;
    }
    Some(format!("{}/{}", host.to_lowercase(), path))
}

pub fn github_of(identity: &str) -> Option<(String, String)> {
    let mut parts = identity.split('/');
    if parts.next()? != "github.com" {
        return None;
    }
    let owner = parts.next()?.to_string();
    let name = parts.next()?.to_string();
    if parts.next().is_some() {
        return None;
    }
    Some((owner, name))
}

/// `github.com/northwindai/*` matches every repo of that owner; anything else must match exactly.
pub fn matches(identity: &str, pattern: &str) -> bool {
    let id = identity.to_lowercase();
    let pat = pattern.trim().to_lowercase();
    match pat.strip_suffix("/*") {
        Some(prefix) => id.starts_with(&format!("{prefix}/")),
        None => id == pat,
    }
}

pub fn list_worktrees(checkout: &Checkout) -> Vec<Worktree> {
    let out = procs::run(
        "git",
        &["-C", &checkout.root.to_string_lossy(), "worktree", "list", "--porcelain"],
        None,
        Duration::from_secs(10),
    );
    match out {
        Ok(text) => parse_worktrees(&text),
        Err(_) => vec![Worktree { path: checkout.root.clone(), head: None, branch: None }],
    }
}

pub fn parse_worktrees(text: &str) -> Vec<Worktree> {
    let mut out = Vec::new();
    for block in text.split("\n\n") {
        let mut path = None;
        let mut head = None;
        let mut branch = None;
        let mut skip = false;
        for line in block.lines() {
            if let Some(p) = line.strip_prefix("worktree ") {
                path = Some(PathBuf::from(p));
            } else if let Some(h) = line.strip_prefix("HEAD ") {
                head = Some(h.to_string());
            } else if let Some(b) = line.strip_prefix("branch ") {
                branch = Some(b.strip_prefix("refs/heads/").unwrap_or(b).to_string());
            } else if line == "bare" || line.starts_with("prunable") {
                skip = true;
            }
        }
        if let (Some(path), false) = (path, skip) {
            if path.exists() {
                out.push(Worktree { path, head, branch });
            }
        }
    }
    out
}

/// The default branch as a remote ref, e.g. `origin/main`.
pub fn default_branch(checkout: &Checkout) -> Option<String> {
    let text = std::fs::read_to_string(checkout.git_dir.join("refs/remotes/origin/HEAD")).ok();
    if let Some(t) = text {
        if let Some(r) = t.trim().strip_prefix("ref: refs/remotes/") {
            return Some(r.to_string());
        }
    }
    for candidate in ["origin/main", "origin/master"] {
        let exists = procs::run(
            "git",
            &["-C", &checkout.root.to_string_lossy(), "rev-parse", "--verify", "--quiet", candidate],
            None,
            Duration::from_secs(5),
        )
        .is_ok();
        if exists {
            return Some(candidate.to_string());
        }
    }
    None
}

fn strip_hash_suffix(name: &str) -> String {
    match name.rsplit_once('-') {
        Some((head, tail)) if tail.len() == 6 && tail.chars().all(|c| c.is_ascii_hexdigit()) && !head.is_empty() => head.to_string(),
        _ => name.to_string(),
    }
}

/// The Claude session that owns a scratchpad path such as
/// `/private/tmp/claude-501/<encoded-cwd>/<session-id>/scratchpad/atlas-infra-nx-cache`.
pub fn scratchpad_owner(path: &Path) -> Option<String> {
    let s = path.to_string_lossy();
    let rest = s.strip_prefix("/private/tmp/").or_else(|| s.strip_prefix("/tmp/"))?;
    let mut parts = rest.split('/');
    if !parts.next()?.starts_with("claude-") {
        return None;
    }
    let _cwd = parts.next()?;
    let id = parts.next()?;
    let is_uuid = id.len() == 36 && id.chars().filter(|c| *c == '-').count() == 4;
    (parts.next()? == "scratchpad" && is_uuid).then(|| id.to_string())
}

fn is_temp(path: &Path) -> bool {
    let tmpdir = std::env::var("TMPDIR").ok().map(PathBuf::from);
    path.starts_with("/private/tmp")
        || path.starts_with("/tmp")
        || path.starts_with("/private/var/folders")
        || tmpdir.map(|t| path.starts_with(t)).unwrap_or(false)
}

/// The short name a lane goes by. See *The model*, Callsigns.
pub fn callsign(checkout: &Checkout, wt: &Path, branch: Option<&str>, home: &Path) -> String {
    if wt == checkout.root {
        return checkout.name.clone();
    }
    if let Ok(rest) = wt.strip_prefix(home.join(".codex/worktrees")) {
        if let Some(b) = branch {
            return b.strip_prefix("codex/").unwrap_or(b).to_string();
        }
        let id = rest.components().next().map(|c| c.as_os_str().to_string_lossy().into_owned()).unwrap_or_default();
        return format!("codex-{id}");
    }
    if made_by_claude(wt) {
        return strip_hash_suffix(&file_name(wt));
    }
    let folder = file_name(wt);
    match folder.strip_prefix(&format!("{}-", checkout.name)) {
        Some(slug) if !slug.is_empty() => slug.to_string(),
        _ => folder,
    }
}

/// A worktree Claude made: `claude -w` and the Claude app put theirs in `.claude/worktrees/`
/// under whichever checkout they ran in, not only the main one.
fn made_by_claude(wt: &Path) -> bool {
    wt.parent().is_some_and(|p| p.ends_with(".claude/worktrees"))
}

/// Who made the lane. See *The model*, Where a lane came from.
pub fn origin(checkout: &Checkout, wt: &Path, home: &Path) -> Origin {
    if wt == checkout.root {
        Origin::Main
    } else if wt.starts_with(home.join(".codex/worktrees")) {
        Origin::Codex
    } else if made_by_claude(wt) {
        Origin::ClaudeApp
    } else if scratchpad_owner(wt).is_some() {
        Origin::Scratchpad
    } else if is_temp(wt) {
        Origin::Temp
    } else if wt.parent() == checkout.root.parent() && file_name(wt).starts_with(&format!("{}-", checkout.name)) {
        Origin::Script
    } else {
        Origin::Other
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checkout(root: &str) -> Checkout {
        Checkout {
            root: PathBuf::from(root),
            git_dir: PathBuf::from(root).join(".git"),
            name: file_name(Path::new(root)),
            identity: Some("github.com/northwindai/atlas".into()),
            github: Some(("northwindai".into(), "atlas".into())),
            github_owners: vec!["northwindai".into(), "ghempton".into()],
        }
    }

    #[test]
    fn normalizes_remote_urls() {
        assert_eq!(normalize_url("git@github.com:northwindai/atlas.git").as_deref(), Some("github.com/northwindai/atlas"));
        assert_eq!(normalize_url("https://github.com/kestrelhq/kestrel.git/").as_deref(), Some("github.com/kestrelhq/kestrel"));
        assert_eq!(normalize_url("ssh://git@GitHub.com/o/r").as_deref(), Some("github.com/o/r"));
        assert_eq!(normalize_url("https://user:tok@github.com/o/r").as_deref(), Some("github.com/o/r"));
        assert_eq!(normalize_url("nonsense"), None);
    }

    #[test]
    fn reads_remotes_and_github_slugs() {
        let cfg = "[core]\n\tbare = false\n[remote \"ghempton\"]\n\turl = git@github.com:ghempton/atlas.git\n[remote \"origin\"]\n\turl = git@github.com:northwindai/atlas.git\n\tfetch = +refs/heads/*:refs/remotes/origin/*\n";
        let remotes = parse_remotes(cfg);
        assert_eq!(remotes.len(), 2);
        assert_eq!(remotes[1], ("origin".to_string(), "git@github.com:northwindai/atlas.git".to_string()));
        assert_eq!(github_of("github.com/northwindai/atlas"), Some(("northwindai".into(), "atlas".into())));
        assert_eq!(github_of("gitlab.com/a/b"), None);
    }

    #[test]
    fn matches_patterns() {
        assert!(matches("github.com/northwindai/atlas", "github.com/northwindai/*"));
        assert!(!matches("github.com/northwindaix/atlas", "github.com/northwindai/*"));
        assert!(matches("github.com/kestrelhq/kestrel", "github.com/kestrelhq/kestrel"));
    }

    #[test]
    fn parses_worktree_lists() {
        let text = "worktree /\nHEAD abc\nbranch refs/heads/app\n\nworktree /nope/missing\nHEAD def\ndetached\n\nworktree /tmp\nHEAD 123\ndetached\nprunable gitdir file points to non-existent location\n\n";
        let wts = parse_worktrees(text);
        assert_eq!(wts.len(), 1);
        assert_eq!(wts[0].branch.as_deref(), Some("app"));
    }

    #[test]
    fn derives_callsigns_and_origins() {
        let home = PathBuf::from("/Users/me");
        let c = checkout("/Users/me/projects/atlasai/atlas");
        let cs = |p: &str, b: Option<&str>| callsign(&c, Path::new(p), b, &home);
        assert_eq!(cs("/Users/me/projects/atlasai/atlas", Some("app")), "atlas");
        assert_eq!(cs("/Users/me/projects/atlasai/atlas-app4", Some("app4")), "app4");
        assert_eq!(cs("/Users/me/.codex/worktrees/e8e5/atlas", Some("codex/class-c-secure-configuration")), "class-c-secure-configuration");
        assert_eq!(cs("/Users/me/.codex/worktrees/5941/atlas", None), "codex-5941");
        assert_eq!(cs("/Users/me/projects/atlasai/atlas/.claude/worktrees/bold-fermi-1ac421", Some("claude/bold-fermi-1ac421")), "bold-fermi");
        assert_eq!(cs("/Users/me/projects/atlasai/atlas-app4/.claude/worktrees/keen-fox", Some("worktree-keen-fox")), "keen-fox");
        assert_eq!(cs("/private/tmp/atlas-fix-vulns-2026-09-21", Some("codex/fix-vulns")), "fix-vulns-2026-09-21");
        assert_eq!(cs("/Users/me/projects/kestrelhq/kestrel-worktrees/lumen-diag", Some("lumen-diag")), "lumen-diag");

        let or = |p: &str| origin(&c, Path::new(p), &home);
        assert_eq!(or("/Users/me/projects/atlasai/atlas"), Origin::Main);
        assert_eq!(or("/Users/me/projects/atlasai/atlas-app4"), Origin::Script);
        assert_eq!(or("/Users/me/.codex/worktrees/e8e5/atlas"), Origin::Codex);
        assert_eq!(or("/Users/me/projects/atlasai/atlas/.claude/worktrees/bold-fermi-1ac421"), Origin::ClaudeApp);
        assert_eq!(or("/Users/me/projects/atlasai/atlas-app4/.claude/worktrees/keen-fox"), Origin::ClaudeApp);
        assert_eq!(or("/private/tmp/atlas-fix-vulns-2026-09-21"), Origin::Temp);
        let scratch = "/private/tmp/claude-501/-Users-me-projects-osprey/59530ddd-e0c9-4a13-8eed-6b14db725855/scratchpad/atlas-nx-cache";
        assert_eq!(or(scratch), Origin::Scratchpad);
        assert_eq!(scratchpad_owner(Path::new(scratch)).as_deref(), Some("59530ddd-e0c9-4a13-8eed-6b14db725855"));
        assert_eq!(cs(scratch, Some("nx-cache")), "nx-cache");
        assert_eq!(scratchpad_owner(Path::new("/private/tmp/atlas-x")), None);
    }
}
