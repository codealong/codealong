//! The folders a Codex account trusts, in its config.toml, and trusting one as answering Trust at
//! its terminal UI's first screen in a folder does (0.157.1's code).
//!
//! A folder is trusted by an entry `[projects."<path>"] trust_level = "trusted"` for the folder
//! itself, or for the repo it's in (`trust_root`: a worktree goes by its main checkout, a bare
//! repository's by itself), each path as given or with its links resolved; a folder above one
//! doesn't count (config's `get_active_project`, git-utils' `resolve_root_git_project_for_trust`). Without an entry the
//! terminal UI asks first; an `untrusted` entry is an answer too, and isn't asked again. Trust is
//! written for the repo's main checkout, else the folder, by its real path (`project_trust_key`).

use std::path::{Path, PathBuf};

/// The repo Codex trusts `folder` by, found as git-utils' `resolve_root_git_project_for_trust`
/// (0.157.1) finds it, check for check: the nearest folder above it with a `.git` (passing over a
/// `.git` folder with no HEAD), that folder itself when its `.git` is a folder, and for a
/// worktree its main checkout, spelled as the worktree's `.git` names it, only when everything
/// agrees: its git folder is under a `worktrees` folder whose `gitdir` names this very checkout
/// and whose `commondir` is the folder above it, and the main checkout's own `.git` is that common
/// folder. A bare repository's worktree, a copied one, or one whose main checkout keeps its git
/// folder elsewhere has none, and goes by the folder itself.
fn trust_root(folder: &Path) -> Option<PathBuf> {
    let real = |p: &Path| std::fs::canonicalize(p).ok();
    let mut dir = if folder.is_dir() { folder.to_path_buf() } else { folder.parent()?.to_path_buf() };
    let root = loop {
        let found = dir.ancestors().find(|d| d.join(".git").exists())?.to_path_buf();
        let dot_git = found.join(".git");
        if !dot_git.is_dir() || dot_git.join("HEAD").exists() {
            break found;
        }
        dir = found.parent()?.to_path_buf();
    };
    let dot_git = root.join(".git");
    if dot_git.is_dir() {
        return Some(root);
    }
    let git_dir = gitdir_named_in(&dot_git)?;
    if !git_dir.is_dir() || std::fs::symlink_metadata(&git_dir).ok()?.file_type().is_symlink() {
        return None;
    }
    let real_git_dir = real(&git_dir)?;
    let worktrees = real_git_dir.parent()?;
    if worktrees.file_name()? != "worktrees" {
        return None;
    }
    let common = worktrees.parent()?;
    let named = |file: &str| small_file(&real_git_dir.join(file)).map(|t| t.trim().to_string()).filter(|t| !t.is_empty());
    let backlink = real_git_dir.join(named("gitdir")?);
    if backlink.file_name()? != ".git" || real(backlink.parent()?)? != real(&root)? || real(&real_git_dir.join(named("commondir")?))? != common {
        return None;
    }
    let main = git_dir.parent()?.parent()?.parent()?.to_path_buf();
    let main_dot_git = main.join(".git");
    let main_git_dir = if main_dot_git.is_dir() { main_dot_git } else { gitdir_named_in(&main_dot_git)? };
    (real(&main_git_dir)? == common).then_some(main)
}

/// A file of git's own, read only when it's a small plain file, as Codex reads them.
fn small_file(path: &Path) -> Option<String> {
    let meta = std::fs::symlink_metadata(path).ok()?;
    (meta.is_file() && meta.len() <= 64 * 1024).then(|| std::fs::read_to_string(path).ok()).flatten()
}

/// The git folder a `.git` file names (`gitdir: <path>`), from the folder it's in.
fn gitdir_named_in(dot_git: &Path) -> Option<PathBuf> {
    let text = small_file(dot_git)?;
    let target = text.trim().strip_prefix("gitdir:")?.trim();
    (!target.is_empty()).then(|| dot_git.parent().map(|p| p.join(target))).flatten()
}

/// Where Codex looks for an entry for `folder`: the folder, then the repo it trusts it by, each
/// with its links resolved and as given.
fn lookup_keys(folder: &Path) -> Vec<String> {
    let root = trust_root(folder);
    let mut keys = Vec::new();
    for path in std::iter::once(folder).chain(root.as_deref()) {
        let real = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        for p in [real.as_path(), path] {
            let key = p.to_string_lossy().into_owned();
            if !keys.contains(&key) {
                keys.push(key);
            }
        }
    }
    keys
}

/// The folder Codex trusts for `folder`, as its own Trust writes it: the repo it trusts it by
/// (`trust_root`), else the folder, with its links resolved.
fn trust_target(folder: &Path) -> PathBuf {
    let target = trust_root(folder).unwrap_or_else(|| folder.to_path_buf());
    std::fs::canonicalize(&target).unwrap_or(target)
}

/// Trusts `folder` for the account whose config is `config` (`<CODEX_HOME>/config.toml`), so a
/// session Codealong starts there in a terminal isn't stopped at the trust screen. False when
/// Codex has an answer for it already, trusted or not, and then the file isn't touched. Not the
/// home folder or one above it, which would trust its own `.codex`.
pub fn trust_folder(config: &Path, folder: &Path, user_home: &Path) -> anyhow::Result<bool> {
    let target = trust_target(folder);
    if user_home.starts_with(&target) {
        anyhow::bail!("{} holds the home folder", target.display());
    }
    // Two starts at once would each write back the file they read. Codex writes its own with no
    // lock either, so only Codealong's writes wait for each other.
    static WRITING: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _writing = WRITING.lock().unwrap_or_else(|e| e.into_inner());
    let text = match std::fs::read_to_string(config) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e.into()),
    };
    let Some(next) = with_trust(&text, &lookup_keys(folder), &target.to_string_lossy())? else { return Ok(false) };
    crate::util::write_whole(config, &next)?;
    Ok(true)
}

/// `text`, a Codex config.toml, with `target` trusted, or None when one of `keys` has an answer
/// already. Everything else keeps its text, comments included. `projects` may be written as
/// tables or inline (`projects = { "/r" = { trust_level = "trusted" } }`), which Codex takes too,
/// and an entry is added in the same form.
fn with_trust(text: &str, keys: &[String], target: &str) -> anyhow::Result<Option<String>> {
    use toml_edit::{table, value, DocumentMut, InlineTable, Item, Table, Value};
    let mut doc: DocumentMut = text.parse()?;
    let projects = doc.entry("projects").or_insert_with(|| {
        let mut t = Table::new();
        // Each project its own `[projects."…"]`, as Codex writes them.
        t.set_implicit(true);
        Item::Table(t)
    });
    let Some(listed) = projects.as_table_like() else { anyhow::bail!("its `projects` isn't a table") };
    if keys.iter().any(|k| listed.get(k).and_then(|p| p.get("trust_level")).is_some()) {
        return Ok(None);
    }
    match projects {
        Item::Value(Value::InlineTable(inline)) => {
            let mut entry = InlineTable::new();
            entry.insert("trust_level", "trusted".into());
            inline.insert(target, Value::InlineTable(entry));
        }
        _ => {
            let Some(projects) = projects.as_table_mut() else { anyhow::bail!("its `projects` isn't a table") };
            let mut entry = table();
            entry["trust_level"] = value("trusted");
            projects.insert(target, entry);
        }
    }
    Ok(Some(doc.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trusts_a_folder_by_its_repo_unless_codex_has_an_answer() {
        let dir = std::fs::canonicalize(crate::fixtures::scratch("codex-trust")).unwrap();
        let home = dir.join("home");
        let repo = home.join("r/app");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        std::fs::write(repo.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
        let chat = home.join("chats/2026-10-13-ab12");
        std::fs::create_dir_all(&chat).unwrap();
        let config = dir.join("config.toml");
        // As Codex writes it.
        let written = "model = \"gpt-5.5\"\n\n# mine\n[projects.\"/r/other\"]\ntrust_level = \"trusted\"\n";
        std::fs::write(&config, written).unwrap();

        // A folder in a repo goes by the repo.
        assert!(trust_folder(&config, &repo.join("src"), &home).unwrap());
        let text = std::fs::read_to_string(&config).unwrap();
        assert_eq!(text, format!("{written}\n[projects.\"{}\"]\ntrust_level = \"trusted\"\n", repo.display()));
        assert!(!trust_folder(&config, &repo, &home).unwrap());
        // A folder outside one by itself, its parent's entry not counting.
        assert!(trust_folder(&config, &chat, &home).unwrap());
        assert!(std::fs::read_to_string(&config).unwrap().contains(&format!("[projects.\"{}\"]", chat.display())));
        let other = home.join("chats/2026-10-13-cd34");
        std::fs::create_dir_all(&other).unwrap();
        assert!(trust_folder(&config, &other, &home).unwrap());

        // A worktree goes by its main checkout, as its `.git` spells it; a bare repository's, or a
        // copy whose git folder names another checkout, by itself.
        let worktree = |admin: &Path, wt: &Path, named: &Path| {
            std::fs::create_dir_all(admin).unwrap();
            std::fs::create_dir_all(wt).unwrap();
            std::fs::write(wt.join(".git"), format!("gitdir: {}\n", admin.display())).unwrap();
            std::fs::write(admin.join("commondir"), "../..\n").unwrap();
            std::fs::write(admin.join("gitdir"), format!("{}\n", named.join(".git").display())).unwrap();
            std::fs::write(admin.join("HEAD"), "ref: refs/heads/wt\n").unwrap();
        };
        let main = home.join("r/main");
        std::fs::create_dir_all(main.join(".git")).unwrap();
        std::fs::write(main.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
        let wt = home.join("r/wt");
        worktree(&main.join(".git/worktrees/wt"), &wt, &wt);
        assert_eq!(trust_target(&wt), main);
        let link = home.join("r/alias");
        std::os::unix::fs::symlink(&main, &link).unwrap();
        let by_link = home.join("r/wt-link");
        worktree(&link.join(".git/worktrees/wt-link"), &by_link, &by_link);
        assert_eq!(trust_root(&by_link), Some(link.clone()));
        let copy = home.join("r/wt-copy");
        std::fs::create_dir_all(&copy).unwrap();
        std::fs::copy(wt.join(".git"), copy.join(".git")).unwrap();
        assert_eq!(trust_target(&copy), copy);
        let bare = home.join("repos/project.git");
        let feature = home.join("repos/feature");
        worktree(&bare.join("worktrees/feature"), &feature, &feature);
        assert_eq!(trust_target(&feature), feature);
        // An answer kept under the link's spelling is found.
        let untrusted_link = format!("[projects.\"{}\"]\ntrust_level = \"untrusted\"\n", link.display());
        assert_eq!(with_trust(&untrusted_link, &lookup_keys(&by_link), &trust_target(&by_link).to_string_lossy()).unwrap(), None);

        // An answer of untrusted stands.
        let untrusted = format!("[projects.\"{}\"]\ntrust_level = \"untrusted\"\n", repo.display());
        assert_eq!(with_trust(&untrusted, &lookup_keys(&repo), &repo.to_string_lossy()).unwrap(), None);
        // Projects written inline get one more the same way.
        let inline = "projects = { \"/r/a\" = { trust_level = \"trusted\" } }\n";
        assert_eq!(with_trust(inline, &["/r/a".into()], "/r/a").unwrap(), None);
        let added: toml::Table = with_trust(inline, &["/r/b".into()], "/r/b").unwrap().unwrap().parse().unwrap();
        assert_eq!(added["projects"]["/r/a"]["trust_level"].as_str(), Some("trusted"));
        assert_eq!(added["projects"]["/r/b"]["trust_level"].as_str(), Some("trusted"));
        assert!(with_trust("projects = 3\n", &["/r/b".into()], "/r/b").is_err());
        // A config that isn't there yet, or has no projects.
        assert_eq!(with_trust("", &["/r/a".into()], "/r/a").unwrap().as_deref(), Some("[projects.\"/r/a\"]\ntrust_level = \"trusted\"\n"));
        // Never the home folder or above, nor a file that doesn't read.
        assert!(trust_folder(&config, &home, &home).is_err());
        std::fs::write(&config, "model = ").unwrap();
        assert!(trust_folder(&config, &chat, &home).is_err());
        assert_eq!(std::fs::read_to_string(&config).unwrap(), "model = ");
    }
}
