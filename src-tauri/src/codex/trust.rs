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

/// The repo Codex trusts `folder` by: the checkout whose `.git` folder it's in, or for a
/// worktree its main checkout, but only when that checkout's own `.git` is the worktree's common
/// folder (git-utils' `resolve_root_git_project_for_trust`). A bare repository's worktree, or one
/// whose main checkout keeps its git folder elsewhere, has none, and goes by the folder itself.
fn trust_root(folder: &Path) -> Option<PathBuf> {
    let (root, main) = crate::repos::locate(folder)?;
    if root == main {
        return Some(root);
    }
    let gitdir = crate::repos::linked_gitdir(&root)?;
    let common = gitdir.join(std::fs::read_to_string(gitdir.join("commondir")).ok()?.trim());
    let main_git = main.join(".git");
    let main_git = if main_git.is_dir() { main_git } else { crate::repos::linked_gitdir(&main)? };
    (std::fs::canonicalize(main_git).ok()? == std::fs::canonicalize(common).ok()?).then_some(main)
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

        // A worktree goes by its main checkout; a bare repository's by itself.
        let main = home.join("r/main");
        std::fs::create_dir_all(main.join(".git/worktrees/wt")).unwrap();
        let wt = home.join("r/wt");
        std::fs::create_dir_all(&wt).unwrap();
        std::fs::write(wt.join(".git"), format!("gitdir: {}\n", main.join(".git/worktrees/wt").display())).unwrap();
        std::fs::write(main.join(".git/worktrees/wt/commondir"), "../..\n").unwrap();
        assert_eq!(trust_target(&wt), main);
        let bare = home.join("repos/project.git");
        std::fs::create_dir_all(bare.join("worktrees/feature")).unwrap();
        let feature = home.join("repos/feature");
        std::fs::create_dir_all(&feature).unwrap();
        std::fs::write(feature.join(".git"), format!("gitdir: {}\n", bare.join("worktrees/feature").display())).unwrap();
        std::fs::write(bare.join("worktrees/feature/commondir"), "../..\n").unwrap();
        assert_eq!(trust_target(&feature), feature);

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
