//! Codealong's small JSON files next to the config (todos.json, state.json, origins.json…),
//! read, changed and written one way. The app and the `codealong` command share them, at times
//! as different builds, so three things hold for every one:
//!
//! - A file that's there but doesn't parse is an error that names it, never an empty default: an
//!   older build that meets a newer file's shape must not write its own idea of empty over it.
//! - A write goes through a temporary file beside the target with a name of its own, then a
//!   rename, so a reader never sees half a file and two writers never share a temporary file.
//! - A read-modify-write (`change`) holds an advisory lock on `<file>.lock`, which keeps other
//!   processes out as well as this one's other threads.

use std::io::Write;
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::SystemTime;

use anyhow::{bail, Result};
use serde::de::DeserializeOwned;
use serde::Serialize;

/// The file's name, for messages: the folder is the same for all of them.
fn name(path: &Path) -> String {
    path.file_name().unwrap_or(path.as_os_str()).to_string_lossy().into_owned()
}

/// `<file><suffix>` beside the file.
fn beside(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(suffix);
    path.with_file_name(name)
}

/// The file's text, or none when there's no file.
pub fn text(path: &Path) -> Result<Option<String>> {
    match std::fs::read_to_string(path) {
        Ok(t) => Ok(Some(t)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => bail!("Couldn't read {}: {e}", path.display()),
    }
}

/// A file's text as a value. `what` says what the file holds ("a todo list"), for the error.
pub fn parse<T: DeserializeOwned>(path: &Path, what: &str, text: &str) -> Result<T> {
    match serde_json::from_str(text) {
        Ok(v) => Ok(v),
        Err(e) => bail!("{} doesn't read as {what} ({e}). Codealong won't change it until it does.", name(path)),
    }
}

/// The file's value, or none when there's no file. A file that doesn't parse is an error, so
/// whoever goes on to write knows not to.
pub fn read<T: DeserializeOwned>(path: &Path, what: &str) -> Result<Option<T>> {
    text(path)?.map(|t| parse(path, what, &t)).transpose()
}

/// The file's value, or the default when there's no file or it doesn't parse. For showing what
/// a file says, and for caches and the like that are fine to start over: whatever is read this
/// way and saved again replaces a file that didn't parse.
pub fn read_or_default<T: DeserializeOwned + Default>(path: &Path) -> T {
    std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

/// Writes a file in one step: a temporary file beside it, then a rename over it.
pub fn write(path: &Path, bytes: impl AsRef<[u8]>) -> Result<()> {
    put(path, bytes.as_ref(), 0o666)
}

/// `write`, for a file only you may read.
pub fn write_private(path: &Path, bytes: impl AsRef<[u8]>) -> Result<()> {
    put(path, bytes.as_ref(), 0o600)
}

/// Writes a value as indented JSON, in one step.
pub fn save<T: Serialize + ?Sized>(path: &Path, value: &T) -> Result<()> {
    write(path, serde_json::to_vec_pretty(value)?)
}

fn put(path: &Path, bytes: &[u8], mode: u32) -> Result<()> {
    // Named by process and write: two writers of one file, in one process or two, each have
    // their own, where a shared `<file>.tmp` let one rename the other's half-written text.
    static WRITES: AtomicU64 = AtomicU64::new(0);
    let tmp = beside(path, &format!(".{}.{}.tmp", std::process::id(), WRITES.fetch_add(1, Ordering::Relaxed)));
    let done = (|| {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        // One a process of the same number left behind would keep its own permissions.
        let _ = std::fs::remove_file(&tmp);
        let mut f = std::fs::OpenOptions::new().write(true).create_new(true).mode(mode).open(&tmp)?;
        f.write_all(bytes)?;
        drop(f);
        std::fs::rename(&tmp, path)
    })();
    if let Err(e) = done {
        let _ = std::fs::remove_file(&tmp);
        bail!("Couldn't write {}: {e}", path.display());
    }
    Ok(())
}

/// An advisory lock on `<file>.lock`, held through a read-modify-write of the file so no two
/// writers, here or in another process, write over each other's change. Released when dropped.
/// Whoever holds it must not ask for it again (nor call `change` on the file): each asking
/// opens the lock's file anew, and would wait on itself.
pub struct Lock(#[allow(dead_code)] std::fs::File);

pub fn lock(path: &Path) -> Result<Lock> {
    let open = || {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::OpenOptions::new().create(true).write(true).open(beside(path, ".lock"))
    };
    let f = match open() {
        Ok(f) => f,
        Err(e) => bail!("Couldn't lock {}: {e}", name(path)),
    };
    // SAFETY: flock on a descriptor this process owns; LOCK_EX waits for the other holder.
    while unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX) } != 0 {
        let e = std::io::Error::last_os_error();
        // A signal that arrives while it waits isn't a reason to give up.
        if e.kind() != std::io::ErrorKind::Interrupted {
            bail!("Couldn't lock {}: {e}", name(path));
        }
    }
    Ok(Lock(f))
}

/// Reads the file under its lock (the default when there's none yet), lets `f` change the
/// value, and saves it when `f` says it changed. Returns the value as it is afterwards. A file
/// that doesn't parse is an error and is left as it is.
pub fn change<T: DeserializeOwned + Serialize + Default>(path: &Path, what: &str, f: impl FnOnce(&mut T) -> bool) -> Result<T> {
    let _lock = lock(path)?;
    let mut value: T = read(path, what)?.unwrap_or_default();
    if f(&mut value) {
        save(path, &value)?;
    }
    Ok(value)
}

/// Changes nobody waited on that couldn't be made, by file, with the file's time and size then.
static FAILED: Mutex<BTreeMap<PathBuf, (Stamp, String)>> = Mutex::new(BTreeMap::new());

type Stamp = Option<(Option<SystemTime>, u64)>;

fn stamp(path: &Path) -> Stamp {
    std::fs::metadata(path).ok().map(|m| (m.modified().ok(), m.len()))
}

/// Notes how a change went that was made in the background, where nobody waits to hear its
/// error: a failure is kept for `problems`, which the engine says as notices, until a change to
/// the file goes through. Returns the result as it came.
pub fn noted<T>(path: &Path, done: Result<T>) -> Result<T> {
    let mut failed = FAILED.lock().unwrap_or_else(|e| e.into_inner());
    match &done {
        Ok(_) => {
            failed.remove(path);
        }
        Err(e) => {
            failed.insert(path.to_path_buf(), (stamp(path), format!("{e:#}")));
        }
    }
    done
}

/// The files in a folder whose last noted change couldn't be made, each by name with why. Only
/// while the file is as it was then: one fixed or deleted since is no problem any more.
pub fn problems(dir: &Path) -> Vec<(String, String)> {
    let failed = FAILED.lock().unwrap_or_else(|e| e.into_inner());
    failed.iter().filter(|(p, (at, _))| p.parent() == Some(dir) && stamp(p) == *at).map(|(p, (_, why))| (name(p), why.clone())).collect()
}

/// A folder of a test's own under the temporary folder, emptied.
#[cfg(test)]
pub fn test_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("codealong-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn files(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        names.sort();
        names
    }

    #[test]
    fn a_missing_file_reads_as_none_and_changes_from_the_default() {
        let dir = test_dir("store-missing");
        let path = dir.join("sub/list.json");
        assert_eq!(text(&path).unwrap(), None);
        assert_eq!(read::<Vec<String>>(&path, "a list").unwrap(), None);
        assert!(read_or_default::<Vec<String>>(&path).is_empty());
        // Nothing is written, nor the file made, when nothing changed.
        assert!(change(&path, "a list", |_: &mut Vec<String>| false).unwrap().is_empty());
        assert!(!path.exists());
        let after = change(&path, "a list", |l: &mut Vec<String>| {
            l.push("a".into());
            true
        });
        assert_eq!(after.unwrap(), ["a"]);
        assert_eq!(read::<Vec<String>>(&path, "a list").unwrap().unwrap(), ["a"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_file_that_does_not_parse_is_an_error_and_is_left_as_it_is() {
        let dir = test_dir("store-unreadable");
        let path = dir.join("list.json");
        // Another build's shape, and a write cut short.
        for bytes in [r#"{"version": 2, "items": ["a"]}"#, r#"["a", "b"#, ""] {
            std::fs::write(&path, bytes).unwrap();
            let e = read::<Vec<String>>(&path, "a list").unwrap_err().to_string();
            assert!(e.starts_with("list.json doesn't read as a list (") && e.ends_with("Codealong won't change it until it does."), "{e}");
            let mut ran = false;
            let e = change(&path, "a list", |l: &mut Vec<String>| {
                ran = true;
                l.push("c".into());
                true
            });
            assert!(e.unwrap_err().to_string().starts_with("list.json doesn't read as a list"));
            assert!(!ran, "the change isn't made to an empty stand-in");
            assert_eq!(std::fs::read_to_string(&path).unwrap(), bytes);
            // Only what's shown, or fine to start over, reads as empty.
            assert!(read_or_default::<Vec<String>>(&path).is_empty());
        }
        assert_eq!(files(&dir), ["list.json", "list.json.lock"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_write_replaces_the_file_in_one_step_and_leaves_no_temporary_file() {
        let dir = test_dir("store-write");
        let path = dir.join("list.json");
        change(&path, "a list", |l: &mut Vec<String>| {
            l.push("a".into());
            true
        })
        .unwrap();
        // Indented, as these files have always been written.
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "[\n  \"a\"\n]");
        assert_eq!(files(&dir), ["list.json", "list.json.lock"]);

        // Readable as any file this process makes is, unless it's private.
        let mode = |name: &str| std::fs::metadata(dir.join(name)).unwrap().permissions().mode() & 0o777;
        std::fs::write(dir.join("plain.json"), "[]").unwrap();
        write(&path, "[]").unwrap();
        write_private(&dir.join("private.json"), b"{}").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "[]");
        assert_eq!((mode("list.json"), mode("private.json")), (mode("plain.json"), 0o600));
        std::fs::remove_file(dir.join("plain.json")).unwrap();
        assert_eq!(files(&dir), ["list.json", "list.json.lock", "private.json"]);

        // A write that can't land says which file, and leaves nothing behind.
        std::fs::create_dir(dir.join("taken.json")).unwrap();
        let e = write(&dir.join("taken.json"), "[]").unwrap_err().to_string();
        assert!(e.starts_with("Couldn't write ") && e.contains("taken.json"), "{e}");
        assert_eq!(files(&dir), ["list.json", "list.json.lock", "private.json", "taken.json"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_background_change_that_fails_is_a_problem_until_one_lands_or_the_file_is_fixed() {
        let dir = test_dir("store-problems");
        let path = dir.join("list.json");
        let add = || {
            noted(&path, change(&path, "a list", |l: &mut Vec<String>| {
                l.push("a".into());
                true
            }))
        };
        add().unwrap();
        assert!(problems(&dir).is_empty());
        std::fs::write(&path, "[").unwrap();
        let e = add().unwrap_err().to_string();
        assert_eq!(problems(&dir), [("list.json".to_string(), e)]);
        // Another folder's files aren't this one's problems.
        assert!(problems(&dir.join("sub")).is_empty());
        // Fixed by hand: no longer a problem, before any change is made.
        std::fs::write(&path, "[\"b\", \"c\"]").unwrap();
        assert!(problems(&dir).is_empty());
        std::fs::write(&path, "{").unwrap();
        add().unwrap_err();
        assert_eq!(problems(&dir).len(), 1);
        std::fs::remove_file(&path).unwrap();
        assert!(problems(&dir).is_empty());
        add().unwrap();
        assert!(problems(&dir).is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn changes_from_two_threads_both_land() {
        let dir = test_dir("store-threads");
        let path = dir.join("list.json");
        let each = 40;
        let threads: Vec<_> = ["a", "b"]
            .into_iter()
            .map(|who| {
                let path = path.clone();
                std::thread::spawn(move || {
                    for i in 0..each {
                        change(&path, "a list", |l: &mut Vec<String>| {
                            l.push(format!("{who}{i}"));
                            true
                        })
                        .unwrap();
                    }
                })
            })
            .collect();
        for t in threads {
            t.join().unwrap();
        }
        let all: Vec<String> = read(&path, "a list").unwrap().unwrap();
        assert_eq!(all.len(), 2 * each);
        for who in ["a", "b"] {
            let of: Vec<&String> = all.iter().filter(|s| s.starts_with(who)).collect();
            assert_eq!(of.len(), each);
            assert!(of.windows(2).all(|w| w[0][1..].parse::<usize>().unwrap() < w[1][1..].parse::<usize>().unwrap()), "each thread's in its order");
        }
        assert_eq!(files(&dir), ["list.json", "list.json.lock"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
