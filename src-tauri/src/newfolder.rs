//! A folder typed into the quick panel's worktree menu (⌘T), for a session to start in that
//! isn't on the board yet: a new project's, say. `look` says what's there as it's typed, with
//! the folders in it whose names start with what's typed after the last `/`; `add` makes the
//! folder when it isn't there, puts it in a project in config.toml (one of its own, named after
//! it, or the one the panel is on), and waits for the board to have it, so the start goes on in
//! it as in any other worktree. A folder on the board already, or in a worktree there, is that
//! worktree: nothing is added for it.

use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use serde::Serialize;

use crate::config;
use crate::engine::Handle;
use crate::model::{Lane, Origin, ELSEWHERE};
use crate::os::{self, Os};
use crate::util::{expand_in, home, tilde_in};

/// The most folders `look` lists as completions.
const MOST: usize = 8;
/// How long `add` waits for the board to have the folder: the engine reads config.toml again
/// when it changes, and its projects' folders with it.
const BOARD_WAIT: Duration = Duration::from_secs(20);

/// What a folder typed in the worktree menu is.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Look {
    /// What was typed, as it was: the panel keeps an answer only while it's what's typed.
    pub typed: String,
    /// The folder it names, `~` expanded and `.` and `..` worked out.
    pub path: String,
    /// As config.toml writes it, `~` for your home folder.
    pub shown: String,
    /// Its last part: what a project of its own is named.
    pub name: String,
    /// It's there; adding one that isn't makes it.
    pub exists: bool,
    /// Why it can't be added, when it can't.
    pub refused: Option<String>,
    /// The worktree on the board it is or is in: that one is started in, and nothing is added.
    pub lane: Option<String>,
    /// The project config.toml lists it in already, where adding it leaves it.
    pub project: Option<String>,
    /// The folders in the one typed (in its parent, while a name is typed after the last `/`)
    /// whose names start with that name, as `shown` writes them.
    pub completions: Vec<String>,
}

/// What `typed` is, from the board and config.toml as they are now and the disk.
pub fn look(engine: &Handle, typed: &str) -> Look {
    let projects = config::current().map(|c| c.projects).unwrap_or_default();
    look_at(typed, &home(), |path| engine.read(|b| worktree_at(&b.lanes, path)), |path| config::project_with_folder(&projects, path).map(|p| p.name.clone()))
}

/// `look`, with `home` your home folder, `on_board` the worktree the board has for a folder, and
/// `listed` the project config.toml lists it in. The disk is only read outside the board.
fn look_at(typed: &str, home: &Path, on_board: impl FnOnce(&Path) -> Option<String>, listed: impl FnOnce(&Path) -> Option<String>) -> Look {
    let Some(path) = resolve(typed, home) else {
        let refused = Some("Type a folder's path, starting with ~/ or /.".to_string());
        return Look { typed: typed.to_string(), path: String::new(), shown: typed.trim().to_string(), name: String::new(), exists: false, refused, lane: None, project: None, completions: Vec::new() };
    };
    let shown = tilde_in(&path, home);
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    // What's typed after the last `/` is the start of a name in the folder before it, as typed:
    // a `.` there asks for hidden folders.
    let t = typed.trim();
    let (dir, start) = match t.rfind('/') {
        Some(i) => (resolve(&t[..=i], home).unwrap_or_else(|| path.clone()), &t[i + 1..]),
        None => (path.clone(), ""),
    };
    let completions = completions(&dir, start, home).into_iter().filter(|c| *c != shown).collect();
    let exists = path.is_dir();
    let lane = on_board(&path);
    let refused = match () {
        _ if lane.is_some() => None,
        _ if path == home => Some("That's your home folder; name a folder in it.".to_string()),
        _ if path.parent().is_none() => Some("That's the top of the disk; name a folder in it.".to_string()),
        _ if path.exists() && !exists => Some(format!("{shown} is a file.")),
        _ if !exists && !path.parent().is_some_and(Path::is_dir) => Some(format!("{} isn't there.", tilde_in(path.parent().unwrap_or(&path), home))),
        _ => None,
    };
    let project = if lane.is_none() { listed(&path) } else { None };
    Look { typed: typed.to_string(), path: path.to_string_lossy().into_owned(), shown, name, exists, refused, lane, project, completions }
}

/// The folder `typed` names: from your home folder (`~`) or the top of the disk (`/`), with `.`
/// and `..` worked out as written, links left as they are. None for anything else.
fn resolve(typed: &str, home: &Path) -> Option<PathBuf> {
    let t = typed.trim();
    if !(t == "~" || t.starts_with("~/") || t.starts_with('/')) {
        return None;
    }
    let mut out = PathBuf::new();
    for part in expand_in(t, home).components() {
        match part {
            Component::RootDir => out.push("/"),
            Component::Normal(p) => out.push(p),
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir | Component::Prefix(_) => {}
        }
    }
    Some(out)
}

/// The worktree on the board that's `path`, or a repo's that holds it. A folder only somewhere
/// in a folder of a project's isn't in it, nor one a session once ran in outside your projects
/// (Elsewhere): adding it puts it in a project. Another device's worktrees aren't this Mac's
/// folders.
pub fn worktree_at(lanes: &[Lane], path: &Path) -> Option<String> {
    lanes
        .iter()
        .filter(|l| l.device.is_none() && !l.starts_chats && l.project != ELSEWHERE)
        .filter(|l| Path::new(&l.path) == path || (l.origin != Origin::Folder && path.starts_with(&l.path)))
        .max_by_key(|l| l.path.len())
        .map(|l| l.id.clone())
}

/// The folders in `dir` whose names start with `start` (whatever their case), as `~` writes
/// them; hidden ones only once `start` begins with a dot. None in a folder macOS asks about.
fn completions(dir: &Path, start: &str, home: &Path) -> Vec<String> {
    if guarded(dir, home) {
        return Vec::new();
    }
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let start = start.to_lowercase();
    let mut names: Vec<String> = entries
        .flatten()
        .filter(|e| std::fs::metadata(e.path()).is_ok_and(|m| m.is_dir()))
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.to_lowercase().starts_with(&start) && (start.starts_with('.') || !n.starts_with('.')))
        .collect();
    names.sort_by_key(|n| n.to_lowercase());
    names.into_iter().take(MOST).map(|n| tilde_in(&dir.join(n), home)).collect()
}

/// Folders whose listing waits on a privacy prompt on macOS (held.rs): a completion would bring
/// one up as you type. Removable and network volumes are under /Volumes.
fn guarded(dir: &Path, home: &Path) -> bool {
    match os::current() {
        Os::MacOs => ["Desktop", "Documents", "Downloads", "Library"].iter().any(|f| dir.starts_with(home.join(f))) || dir.starts_with("/Volumes"),
        // Linux asks about none.
        Os::Linux => false,
    }
}

/// Makes the folder `typed` names when it isn't there, puts it in `project` in config.toml (in
/// a project of its own with none), and waits for the board to have it; answers its worktree's
/// id. A folder the board has already, or a worktree holds, answers that worktree.
pub fn add(engine: &Handle, typed: &str, project: Option<&str>) -> Result<String> {
    let look = look(engine, typed);
    if let Some(why) = look.refused {
        bail!(why);
    }
    if let Some(lane) = look.lane {
        return Ok(lane);
    }
    let path = PathBuf::from(&look.path);
    if !look.exists {
        std::fs::create_dir(&path).with_context(|| format!("Couldn't make {}", look.shown))?;
    }
    config::add_folder(&look.shown, project)?;
    let until = Instant::now() + BOARD_WAIT;
    loop {
        if let Some(id) = engine.read(|b| worktree_at(&b.lanes, &path)) {
            return Ok(id);
        }
        if Instant::now() >= until {
            return Err(anyhow!("{} is in config.toml now, but the board doesn't have it yet; pick it again in a moment.", look.shown));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::{lane, scratch};
    use crate::model::DeviceTag;

    fn at(typed: &str, home: &Path) -> Look {
        look_at(typed, home, |_| None, |_| None)
    }

    #[test]
    fn a_path_is_from_home_or_the_top_with_dots_worked_out() {
        let h = Path::new("/Users/me");
        assert_eq!(resolve("~/projects/kestrel", h), Some(PathBuf::from("/Users/me/projects/kestrel")));
        assert_eq!(resolve(" ~/projects/./atlas/../kestrel/ ", h), Some(PathBuf::from("/Users/me/projects/kestrel")));
        assert_eq!(resolve("/opt/northwind", h), Some(PathBuf::from("/opt/northwind")));
        assert_eq!(resolve("~", h), Some(PathBuf::from("/Users/me")));
        assert_eq!(resolve("/..", h), Some(PathBuf::from("/")));
        for words in ["kestrel", "~kestrel", "./kestrel", ""] {
            assert_eq!(resolve(words, h), None, "{words:?}");
        }
    }

    #[test]
    fn a_new_folder_goes_where_its_parent_is() {
        let h = scratch("newfolder-new");
        std::fs::create_dir_all(h.join("projects/atlas")).unwrap();
        let new = at("~/projects/kestrel", &h);
        assert_eq!((new.shown.as_str(), new.name.as_str(), new.exists, new.refused.as_deref()), ("~/projects/kestrel", "kestrel", false, None));
        assert_eq!(new.path, h.join("projects/kestrel").to_string_lossy());
        let there = at("~/projects/atlas", &h);
        assert!(there.exists && there.refused.is_none());
        assert_eq!(at("~/projets/kestrel", &h).refused.as_deref(), Some("~/projets isn't there."), "a typo above it makes nothing");
        std::fs::write(h.join("projects/notes.md"), "").unwrap();
        assert_eq!(at("~/projects/notes.md", &h).refused.as_deref(), Some("~/projects/notes.md is a file."));
        assert!(at("~", &h).refused.unwrap().contains("home folder"));
        assert!(at("/", &h).refused.unwrap().contains("top of the disk"));
        assert!(at("projects", &h).refused.unwrap().contains("~/ or /"));
    }

    #[test]
    fn completions_are_the_folders_whose_names_start_with_whats_typed() {
        let h = scratch("newfolder-complete");
        for f in ["projects/Atlas", "projects/atlas-web", "projects/kestrel", "projects/.cache"] {
            std::fs::create_dir_all(h.join(f)).unwrap();
        }
        std::fs::write(h.join("projects/atlas.md"), "").unwrap();
        assert_eq!(at("~/projects/at", &h).completions, ["~/projects/Atlas", "~/projects/atlas-web"], "folders alone, whatever their case");
        assert_eq!(at("~/projects/", &h).completions, ["~/projects/Atlas", "~/projects/atlas-web", "~/projects/kestrel"], "hidden ones left out");
        assert_eq!(at("~/projects/.", &h).completions, ["~/projects/.cache"]);
        assert_eq!(at("~/projects/kestrel", &h).completions, Vec::<String>::new(), "not the one typed whole");
        let documents = h.join("Documents");
        std::fs::create_dir_all(documents.join("notes")).unwrap();
        let mac = os::as_if(Os::MacOs, || at("~/Documents/", &h).completions);
        assert!(mac.is_empty(), "macOS would ask before listing Documents");
        assert_eq!(os::as_if(Os::Linux, || at("~/Documents/", &h).completions), ["~/Documents/notes"]);
    }

    #[test]
    fn a_folder_on_the_board_is_its_worktree() {
        let h = scratch("newfolder-board");
        let repo = lane("/Users/me/projects/atlas", Vec::new());
        let folder = Lane { origin: Origin::Folder, ..lane("/Users/me/work", Vec::new()) };
        let elsewhere = Lane { project: ELSEWHERE.into(), origin: Origin::Other, ..lane("/Users/me/scratch", Vec::new()) };
        let tag = DeviceTag { name: "Studio".into(), short: "abc12".into(), color: "teal".into(), online: true, last_seen: None };
        let other_mac = Lane { device: Some(tag), ..lane("/Users/me/projects/kestrel", Vec::new()) };
        let lanes = [repo, folder, elsewhere, other_mac];
        let on = |p: &str| worktree_at(&lanes, Path::new(p));
        assert_eq!(on("/Users/me/projects/atlas").as_deref(), Some("/Users/me/projects/atlas"));
        assert_eq!(on("/Users/me/projects/atlas/web").as_deref(), Some("/Users/me/projects/atlas"), "a repo's folder is in its worktree");
        assert_eq!(on("/Users/me/work").as_deref(), Some("/Users/me/work"));
        assert_eq!(on("/Users/me/work/kestrel"), None, "a folder in a project's folder is a folder of its own");
        assert_eq!(on("/Users/me/scratch"), None, "Elsewhere is put in a project");
        assert_eq!(on("/Users/me/projects/kestrel"), None, "another Mac's folder isn't this one's");

        let found = look_at("~/projects/atlas", &h, |_| Some("/r/atlas".into()), |_| Some("Atlas".into()));
        assert_eq!((found.lane.as_deref(), found.refused, found.project), (Some("/r/atlas"), None, None), "a worktree is started in as it is");
        let listed = look_at("~/projects/kestrel", &h, |_| None, |_| Some("Atlas".into()));
        assert_eq!(listed.project.as_deref(), Some("Atlas"));
    }
}
