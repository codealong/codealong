//! The folders a Claude Code account trusts, in its state file, and trusting one as answering
//! Yes at its trust prompt does.

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::jsonedit::Node;
use crate::util::write_whole;

/// The file Claude Code keeps its own state in, the folders it trusts among it (2.1.285's code):
/// `~/.claude.json` for the default folder and `<home>/.claude.json` for any other
/// (`CLAUDE_CONFIG_DIR`), or an old install's `<home>/.config.json` while that's still there.
pub fn state_file(home: &Path) -> PathBuf {
    let legacy = home.join(".config.json");
    if legacy.is_file() {
        return legacy;
    }
    let user_home = crate::util::home();
    if home == user_home.join(".claude") {
        user_home.join(".claude.json")
    } else {
        home.join(".claude.json")
    }
}

const TRUSTED: &str = "hasTrustDialogAccepted";

/// Trusts `folder` for the account, as answering Yes to Claude Code's trust prompt there does,
/// so a session Codealong starts there isn't stopped by the prompt (whose default answer quits)
/// and `--bg` doesn't refuse it. False when Claude Code trusts it already, and then the file
/// isn't touched. Claude Code rewrites the file all the time, so the change is made under its
/// own lock and written whole, and it only ever adds the one entry.
pub fn trust_folder(home: &Path, folder: &Path) -> anyhow::Result<bool> {
    trust_folder_in(&state_file(home), folder, &crate::util::home())
}

/// Whether the account trusts `folder`, as Claude Code tells before it runs what a repo declares
/// there (its hooks). Only reads the file.
pub fn trusts(home: &Path, folder: &Path) -> bool {
    trusts_in(&state_file(home), folder)
}

fn trusts_in(file: &Path, folder: &Path) -> bool {
    let Ok(folder) = std::fs::canonicalize(folder) else { return false };
    let Ok(keys) = trust_keys(&folder) else { return false };
    std::fs::read_to_string(file).is_ok_and(|text| matches!(with_trust(&text, &keys), Ok(None)))
}

fn trust_folder_in(file: &Path, folder: &Path, user_home: &Path) -> anyhow::Result<bool> {
    // Claude Code goes by its working folder, which the system gives with links resolved.
    let folder = std::fs::canonicalize(folder)?;
    let user_home = std::fs::canonicalize(user_home).unwrap_or_else(|_| user_home.to_path_buf());
    // It trusts the home folder one session at a time, and every folder outside a repo goes by
    // the folders above it, so trusting home or above would trust nearly everything.
    if user_home.starts_with(&folder) {
        anyhow::bail!("{} holds the home folder, which Claude Code trusts one session at a time", folder.display());
    }
    let keys = trust_keys(&folder)?;
    // Most folders are trusted already (a worktree goes by its repo's main checkout), so the
    // lock is taken only to add an entry.
    let trusted = |text: &str| matches!(with_trust(text, &keys), Ok(None));
    if std::fs::read_to_string(file).is_ok_and(|t| trusted(&t)) {
        return Ok(false);
    }
    let _lock = StateLock::take(file)?;
    let text = match std::fs::read_to_string(file) {
        Ok(t) => t,
        // An account that hasn't run yet: Claude Code starts from whatever the file holds.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => "{}".to_string(),
        Err(e) => return Err(e.into()),
    };
    let Some(next) = with_trust(&text, &keys)? else { return Ok(false) };
    write_whole(file, &next)?;
    Ok(true)
}

/// The `projects` entries Claude Code looks at to tell whether it trusts `folder` (2.1.285's
/// code): its own first, the one a trust adds, then each folder's above it, up to its worktree's
/// root in a repo and all the way up outside one, and its repo's main checkout's.
fn trust_keys(folder: &Path) -> anyhow::Result<Vec<String>> {
    let located = crate::repos::locate(folder);
    let mut keys: Vec<&Path> = Vec::new();
    for dir in folder.ancestors() {
        keys.push(dir);
        if located.as_ref().is_some_and(|(root, _)| dir == root) {
            break;
        }
    }
    keys.extend(located.as_ref().map(|(_, main)| main.as_path()));
    // A main checkout is its own.
    keys.dedup();
    let own = keys[0].to_str().ok_or_else(|| anyhow::anyhow!("{} isn't a name Claude Code can keep", folder.display()))?;
    Ok(std::iter::once(own.to_string()).chain(keys[1..].iter().filter_map(|k| k.to_str()).map(String::from)).collect())
}

/// `text`, Claude Code's state file, with the entry for `keys[0]` (the folder's own, see
/// `trust_keys`) marked trusted, or None when an entry of `keys` is trusted already. Only the
/// entry it adds or changes is written anew: everything else keeps its text and its order.
fn with_trust(text: &str, keys: &[String]) -> anyhow::Result<Option<String>> {
    use serde_json::value::RawValue;
    let own = keys.first().ok_or_else(|| anyhow::anyhow!("no folder to trust"))?;
    let mut top = Node::entries(text)?;
    let projects = Node::object_in(&mut top, "projects")?;
    for key in keys {
        let entry = projects.iter().rev().find(|(k, _)| k == key);
        if entry.is_some_and(|(_, v)| is_trusted(v)) {
            return Ok(None);
        }
    }
    let entry = Node::object_in(projects, own)?;
    let yes = RawValue::from_string("true".into())?;
    match entry.iter_mut().rev().find(|(k, _)| k == TRUSTED) {
        Some((_, v)) => *v = Node::Raw(yes),
        None => entry.push((TRUSTED.to_string(), Node::Raw(yes))),
    }
    // Claude Code writes the file as JSON.stringify(config, null, 2) does, which serde_json's
    // pretty printer matches.
    let mut out = serde_json::to_string_pretty(&Node::Object(top))?;
    if text.ends_with('\n') {
        out.push('\n');
    }
    Ok(Some(out))
}

/// Whether a project's entry says Claude Code trusts it.
fn is_trusted(entry: &Node) -> bool {
    match entry {
        Node::Raw(raw) => serde_json::from_str::<Value>(raw.get()).is_ok_and(|v| v.get(TRUSTED) == Some(&Value::Bool(true))),
        Node::Object(entries) => entries.iter().rev().find(|(k, _)| k == TRUSTED).is_some_and(|(_, v)| matches!(v, Node::Raw(r) if r.get() == "true")),
    }
}

/// Claude Code's lock on its state file, taken as it takes it (proper-lockfile): a folder beside
/// the file, `<file>.lock`, which only one process can make. Claude Code holds it while it reads
/// the file again and writes it, and freshens its time every 5 seconds; one untouched for 10 is
/// a crashed process's, and is taken over.
struct StateLock(PathBuf);

impl StateLock {
    fn take(file: &Path) -> anyhow::Result<StateLock> {
        use std::time::{Duration, Instant};
        let mut path = file.as_os_str().to_owned();
        path.push(".lock");
        let path = PathBuf::from(path);
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match std::fs::create_dir(&path) {
                Ok(()) => return Ok(StateLock(path)),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(e.into()),
            }
            match std::fs::metadata(&path).and_then(|m| m.modified()) {
                // Let go of in the meantime.
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Ok(at) if at.elapsed().is_ok_and(|age| age > Duration::from_secs(10)) => {
                    let _ = std::fs::remove_dir(&path);
                    continue;
                }
                _ => {}
            }
            if Instant::now() > deadline {
                anyhow::bail!("Claude Code has held {} for more than 5 seconds", path.display());
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

impl Drop for StateLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A state file as Claude Code 2.1.285 writes it: JSON.stringify(config, null, 2).
    const STATE: &str = r#"{
  "numStartups": 5,
  "projects": {
    "/r/b": {
      "allowedTools": [],
      "mcpServers": {},
      "hasTrustDialogAccepted": false
    },
    "/r/c": {
      "hasTrustDialogAccepted": true
    }
  },
  "oauthAccount": {
    "emailAddress": "a@b.c"
  }
}"#;

    fn keys(ks: &[&str]) -> Vec<String> {
        ks.iter().map(|k| k.to_string()).collect()
    }

    #[test]
    fn trusts_a_folder_by_adding_its_entry_and_nothing_else() {
        let out = with_trust(STATE, &keys(&["/r/a"])).unwrap().unwrap();
        let expected = STATE.replace("      \"hasTrustDialogAccepted\": true\n    }\n  },", "      \"hasTrustDialogAccepted\": true\n    },\n    \"/r/a\": {\n      \"hasTrustDialogAccepted\": true\n    }\n  },");
        assert_eq!(out, expected);
    }

    #[test]
    fn trusts_a_folder_it_has_an_entry_for_keeping_the_rest_of_it() {
        let out = with_trust(STATE, &keys(&["/r/b"])).unwrap().unwrap();
        assert_eq!(out, STATE.replacen("\"hasTrustDialogAccepted\": false", "\"hasTrustDialogAccepted\": true", 1));
        // A file that ends with a newline keeps it.
        assert!(with_trust(&format!("{STATE}\n"), &keys(&["/r/b"])).unwrap().unwrap().ends_with("}\n}\n"));
    }

    #[test]
    fn leaves_a_folder_claude_code_trusts_already_alone() {
        // Its own entry, one above it, or its repo's main checkout.
        assert_eq!(with_trust(STATE, &keys(&["/r/c"])).unwrap(), None);
        assert_eq!(with_trust(STATE, &keys(&["/r/c/sub", "/r/c"])).unwrap(), None);
        assert_eq!(with_trust(STATE, &keys(&["/w/x", "/r/c"])).unwrap(), None);
        assert!(with_trust(STATE, &keys(&["/r/b/sub", "/r/b"])).unwrap().is_some());
    }

    #[test]
    fn makes_the_projects_it_needs_and_refuses_a_file_it_cant_read() {
        let trusted = "{\n  \"projects\": {\n    \"/r/a\": {\n      \"hasTrustDialogAccepted\": true\n    }\n  }\n}";
        assert_eq!(with_trust("{}", &keys(&["/r/a"])).unwrap().unwrap(), trusted);
        assert_eq!(with_trust("{\"projects\": null}", &keys(&["/r/a"])).unwrap().unwrap(), trusted);
        assert!(with_trust("", &keys(&["/r/a"])).is_err());
        assert!(with_trust("{\"projects\": {", &keys(&["/r/a"])).is_err());
        assert!(with_trust("[]", &keys(&["/r/a"])).is_err());
    }

    fn trust_scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("codealong-trust-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::canonicalize(&dir).unwrap()
    }

    #[test]
    fn looks_at_the_folders_claude_code_looks_at() {
        let dir = trust_scratch("keys");
        let main = dir.join("main");
        let wt = dir.join("wt");
        std::fs::create_dir_all(main.join(".git/worktrees/wt")).unwrap();
        std::fs::write(main.join(".git/worktrees/wt/commondir"), "../..\n").unwrap();
        std::fs::create_dir_all(wt.join("sub")).unwrap();
        std::fs::write(wt.join(".git"), format!("gitdir: {}\n", main.join(".git/worktrees/wt").display())).unwrap();
        let s = |p: &Path| p.to_str().unwrap().to_string();
        assert_eq!(trust_keys(&wt.join("sub")).unwrap(), vec![s(&wt.join("sub")), s(&wt), s(&main)]);
        assert_eq!(trust_keys(&main).unwrap(), vec![s(&main)]);
        // Outside a repo, every folder up to the top.
        let loose = dir.join("loose");
        std::fs::create_dir_all(&loose).unwrap();
        let got = trust_keys(&loose).unwrap();
        assert_eq!(got.first(), Some(&s(&loose)));
        assert_eq!(got.last().map(String::as_str), Some("/"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn trusts_a_folder_in_the_file_under_claude_codes_lock() {
        let dir = trust_scratch("file");
        let file = dir.join(".claude.json");
        let folder = dir.join("proj");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(&file, STATE).unwrap();
        let home = dir.join("home");
        assert!(trust_folder_in(&file, &folder, &home).unwrap());
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(v["projects"][folder.to_str().unwrap()][TRUSTED], Value::Bool(true));
        assert_eq!(v["oauthAccount"]["emailAddress"], "a@b.c");
        assert!(!dir.join(".claude.json.lock").exists());
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(&file).unwrap().permissions().mode() & 0o777, 0o600);
        // Once trusted, the file isn't written again.
        let before = std::fs::read_to_string(&file).unwrap();
        assert!(!trust_folder_in(&file, &folder, &home).unwrap());
        assert_eq!(std::fs::read_to_string(&file).unwrap(), before);
        // Never home or a folder above it.
        assert!(trust_folder_in(&file, &dir, &folder).is_err());
        assert!(trust_folder_in(&file, &folder, &folder).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn says_whether_it_trusts_a_folder_writing_nothing() {
        let dir = trust_scratch("trusts");
        let file = dir.join(".claude.json");
        let folder = dir.join("proj");
        std::fs::create_dir_all(&folder).unwrap();
        let text = format!("{{\"projects\": {{\"{}\": {{\"{TRUSTED}\": true}}}}}}", folder.display());
        std::fs::write(&file, &text).unwrap();
        assert!(trusts_in(&file, &folder));
        assert!(trusts_in(&file, &folder.join(".")));
        // The folder above isn't trusted by one below it, nor anything by a file that isn't there.
        assert!(!trusts_in(&file, &dir));
        assert!(!trusts_in(&dir.join("missing.json"), &folder));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), text);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Reads this Mac's Claude Code state files, writing nothing, and checks that trusting a new
    /// folder would only add its entry: `cargo test trust_keeps_real_state -- --ignored`.
    #[test]
    #[ignore]
    fn trust_keeps_real_state() {
        let user_home = crate::util::home();
        for home in [user_home.join(".claude"), user_home.join(".claude2")] {
            let Ok(text) = std::fs::read_to_string(state_file(&home)) else { continue };
            let key = "/nowhere/codealong-trust-check";
            let out = with_trust(&text, &keys(&[key])).unwrap().unwrap();
            let same = text.bytes().zip(out.bytes()).take_while(|(a, b)| a == b).count();
            let tail = text.bytes().rev().zip(out.bytes().rev()).take_while(|(a, b)| a == b).count().min(text.len() - same);
            assert_eq!(same + tail, text.len(), "{}: more than an insertion", home.display());
            let added = &out[same..out.len() - tail];
            assert_eq!(added, format!(",\n    \"{key}\": {{\n      \"{TRUSTED}\": true\n    }}"), "{}", home.display());
            let mut before: Value = serde_json::from_str(&text).unwrap();
            before["projects"][key] = serde_json::json!({ TRUSTED: true });
            assert_eq!(serde_json::from_str::<Value>(&out).unwrap(), before);
            println!("{}: {} bytes, {} projects, adds {} bytes", home.display(), text.len(), before["projects"].as_object().unwrap().len(), added.len());
        }
    }

    #[test]
    fn waits_for_claude_codes_lock_and_takes_over_a_dead_ones() {
        use std::time::{Duration, Instant, SystemTime};
        let dir = trust_scratch("lock");
        let file = dir.join(".claude.json");
        let lock = dir.join(".claude.json.lock");
        let home = dir.join("home");
        std::fs::write(&file, "{}").unwrap();
        // Held: the write waits until it's let go of.
        std::fs::create_dir(&lock).unwrap();
        let held = lock.clone();
        let letting_go = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(300));
            std::fs::remove_dir(&held).unwrap();
        });
        let started = Instant::now();
        std::fs::create_dir_all(dir.join("a")).unwrap();
        assert!(trust_folder_in(&file, &dir.join("a"), &home).unwrap());
        assert!(started.elapsed() >= Duration::from_millis(300));
        letting_go.join().unwrap();
        // Untouched for longer than proper-lockfile's 10 seconds: left by a process that's gone.
        std::fs::create_dir(&lock).unwrap();
        std::fs::File::open(&lock).unwrap().set_modified(SystemTime::now() - Duration::from_secs(20)).unwrap();
        std::fs::create_dir_all(dir.join("b")).unwrap();
        assert!(trust_folder_in(&file, &dir.join("b"), &home).unwrap());
        assert!(!lock.exists());
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(v["projects"].as_object().unwrap().len(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
