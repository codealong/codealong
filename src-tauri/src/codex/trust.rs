//! The folders a Codex account trusts, in its config.toml, and trusting one as answering Trust at
//! its terminal UI's first screen in a folder does (0.157.1's code).
//!
//! A folder is trusted by an entry `[projects."<path>"] trust_level = "trusted"` for the folder
//! itself, or for the repo it's in, a worktree going by its repo's main checkout, each path as
//! given or with its links resolved; a folder above one doesn't count (config's
//! `get_active_project`, git-utils' `resolve_root_git_project_for_trust`). Without an entry the
//! terminal UI asks first; an `untrusted` entry is an answer too, and isn't asked again. Trust is
//! written for the repo's main checkout, else the folder, by its real path (`project_trust_key`).

use std::path::{Path, PathBuf};

/// Where Codex looks for an entry for `folder`: the folder, then its repo's main checkout, each
/// with its links resolved and as given.
fn lookup_keys(folder: &Path) -> Vec<String> {
    let main = crate::repos::locate(folder).map(|(_, main)| main);
    let mut keys = Vec::new();
    for path in std::iter::once(folder).chain(main.as_deref()) {
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

/// The folder Codex trusts for `folder`, as its own Trust writes it: its repo's main checkout,
/// else the folder, with its links resolved.
fn trust_target(folder: &Path) -> PathBuf {
    let target = crate::repos::locate(folder).map_or_else(|| folder.to_path_buf(), |(_, main)| main);
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
/// already. Everything else keeps its text, comments included.
fn with_trust(text: &str, keys: &[String], target: &str) -> anyhow::Result<Option<String>> {
    use toml_edit::{table, value, DocumentMut, Item, Table};
    let mut doc: DocumentMut = text.parse()?;
    let answered = |projects: &Table| keys.iter().any(|k| projects.get(k).and_then(|p| p.get("trust_level")).is_some());
    match doc.get("projects") {
        Some(Item::Table(projects)) if answered(projects) => return Ok(None),
        Some(Item::Table(_)) | None => {}
        Some(_) => anyhow::bail!("its `projects` isn't a table"),
    }
    let projects = doc.entry("projects").or_insert_with(|| {
        let mut t = Table::new();
        // Each project its own `[projects."…"]`, as Codex writes them.
        t.set_implicit(true);
        Item::Table(t)
    });
    let Some(projects) = projects.as_table_mut() else { anyhow::bail!("its `projects` isn't a table") };
    let mut entry = table();
    entry["trust_level"] = value("trusted");
    projects.insert(target, entry);
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

        // An answer of untrusted stands.
        let untrusted = format!("[projects.\"{}\"]\ntrust_level = \"untrusted\"\n", repo.display());
        assert_eq!(with_trust(&untrusted, &lookup_keys(&repo), &repo.to_string_lossy()).unwrap(), None);
        // A config that isn't there yet, or has no projects.
        assert_eq!(with_trust("", &["/r/a".into()], "/r/a").unwrap().as_deref(), Some("[projects.\"/r/a\"]\ntrust_level = \"trusted\"\n"));
        // Never the home folder or above, nor a file that doesn't read.
        assert!(trust_folder(&config, &home, &home).is_err());
        std::fs::write(&config, "model = ").unwrap();
        assert!(trust_folder(&config, &chat, &home).is_err());
        assert_eq!(std::fs::read_to_string(&config).unwrap(), "model = ");
    }
}
