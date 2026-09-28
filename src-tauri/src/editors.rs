//! The editors a folder opens in: the ones Codealong knows, found where Mac apps are installed,
//! and any other app `[apps] editor` names. See *Apps*.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};

use crate::config::Profile;
use crate::procs;
use crate::util::home;

/// An editor Codealong knows, by an id of its own.
struct Known {
    id: &'static str,
    name: &'static str,
    /// Its app's names, the first found wins: some come in editions (Community, beta).
    apps: &'static [&'static str],
}

/// In the order the Open in menus list them.
const KNOWN: &[Known] = &[
    Known { id: "vscode", name: "VS Code", apps: &["Visual Studio Code"] },
    Known { id: "vscode-insiders", name: "VS Code Insiders", apps: &["Visual Studio Code - Insiders"] },
    Known { id: "cursor", name: "Cursor", apps: &["Cursor"] },
    Known { id: "windsurf", name: "Windsurf", apps: &["Windsurf"] },
    Known { id: "zed", name: "Zed", apps: &["Zed"] },
    Known { id: "zed-preview", name: "Zed Preview", apps: &["Zed Preview"] },
    Known { id: "kiro", name: "Kiro", apps: &["Kiro"] },
    Known { id: "sublime", name: "Sublime Text", apps: &["Sublime Text"] },
    Known { id: "nova", name: "Nova", apps: &["Nova"] },
    Known { id: "bbedit", name: "BBEdit", apps: &["BBEdit"] },
    Known { id: "xcode", name: "Xcode", apps: &["Xcode", "Xcode-beta"] },
    Known { id: "intellij", name: "IntelliJ IDEA", apps: &["IntelliJ IDEA", "IntelliJ IDEA Ultimate", "IntelliJ IDEA CE", "IntelliJ IDEA Community Edition"] },
    Known { id: "webstorm", name: "WebStorm", apps: &["WebStorm"] },
    Known { id: "pycharm", name: "PyCharm", apps: &["PyCharm", "PyCharm Professional Edition", "PyCharm CE", "PyCharm Community Edition"] },
    Known { id: "goland", name: "GoLand", apps: &["GoLand"] },
    Known { id: "rustrover", name: "RustRover", apps: &["RustRover"] },
    Known { id: "clion", name: "CLion", apps: &["CLion"] },
    Known { id: "android-studio", name: "Android Studio", apps: &["Android Studio"] },
];

/// Where Mac apps are installed: by hand or Homebrew, for one user (JetBrains Toolbox puts its
/// IDEs there, or in a folder of its own there), and by Setapp.
fn app_dirs() -> Vec<PathBuf> {
    let h = home();
    vec![PathBuf::from("/Applications"), h.join("Applications"), h.join("Applications/JetBrains Toolbox"), PathBuf::from("/Applications/Setapp")]
}

fn find_in(dirs: &[PathBuf], k: &Known) -> Option<PathBuf> {
    k.apps.iter().flat_map(|app| dirs.iter().map(move |d| d.join(format!("{app}.app")))).find(|p| p.is_dir())
}

fn installed_in(dirs: &[PathBuf]) -> Vec<Profile> {
    KNOWN.iter().filter(|k| find_in(dirs, k).is_some()).map(|k| Profile { id: k.id.into(), name: k.name.into() }).collect()
}

/// The editors installed on this Mac, in menu order. Looked for at most once a minute, so one
/// installed while Codealong runs shows up without a restart.
pub fn installed() -> Vec<Profile> {
    static FOUND: Mutex<Option<(Instant, Vec<Profile>)>> = Mutex::new(None);
    let mut found = FOUND.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((at, list)) = found.as_ref() {
        if at.elapsed() < Duration::from_secs(60) {
            return list.clone();
        }
    }
    let list = installed_in(&app_dirs());
    *found = Some((Instant::now(), list.clone()));
    list
}

/// What menus call the editor `id` names: a known editor's name, else the app's.
pub fn name(id: &str) -> String {
    match KNOWN.iter().find(|k| k.id == id) {
        Some(k) => k.name.to_string(),
        None => Path::new(id).file_stem().map_or_else(|| id.to_string(), |s| s.to_string_lossy().into_owned()),
    }
}

/// What `open -a` is given for `id`: a known editor's app where it's installed (its first name
/// when it isn't found, for Launch Services to look for), else `id` itself, an app's name or path.
fn app_for(dirs: &[PathBuf], id: &str) -> String {
    match KNOWN.iter().find(|k| k.id == id) {
        Some(k) => find_in(dirs, k).map_or_else(|| k.apps[0].to_string(), |p| p.to_string_lossy().into_owned()),
        None => id.to_string(),
    }
}

/// Opens `path`, a folder or a file, in the editor `id` names.
pub fn open(id: &str, path: &Path) -> Result<()> {
    let app = app_for(&app_dirs(), id);
    procs::run("/usr/bin/open", &["-a", &app, &path.to_string_lossy()], None, Duration::from_secs(15))
        .map(|_| ())
        .map_err(|_| anyhow!("Couldn't open {}. Is it installed?", name(id)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apps_in(root: &Path, names: &[&str]) -> Vec<PathBuf> {
        let dirs = vec![root.join("Applications"), root.join("home/Applications")];
        for n in names {
            std::fs::create_dir_all(root.join(n)).unwrap();
        }
        dirs
    }

    #[test]
    fn finds_editors_where_apps_are_installed_in_menu_order() {
        let root = std::env::temp_dir().join(format!("codealong-editors-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let dirs = apps_in(&root, &["Applications/Zed.app", "Applications/Visual Studio Code.app", "home/Applications/PyCharm CE.app", "Applications/Notes.app"]);
        let ids: Vec<String> = installed_in(&dirs).into_iter().map(|p| p.id).collect();
        assert_eq!(ids, ["vscode", "zed", "pycharm"]);
        // A known editor opens from where it was found, and one that wasn't by its app's name.
        assert_eq!(app_for(&dirs, "pycharm"), root.join("home/Applications/PyCharm CE.app").to_string_lossy());
        assert_eq!(app_for(&dirs, "cursor"), "Cursor");
        // Anything else is an app's name or path, for `open -a`.
        assert_eq!(app_for(&dirs, "TextMate"), "TextMate");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn names_known_editors_and_apps() {
        assert_eq!(name("vscode"), "VS Code");
        assert_eq!(name("cursor"), "Cursor");
        assert_eq!(name("TextMate"), "TextMate");
        assert_eq!(name("/Applications/CotEditor.app"), "CotEditor");
    }

    #[test]
    fn ids_are_unique() {
        let mut ids: Vec<&str> = KNOWN.iter().map(|k| k.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), KNOWN.len());
    }
}
