//! The editors a folder opens in: the ones Codealong knows, found where Mac apps are installed,
//! and any other app `[apps] editor` names. See README.md's Settings section. The ones with a
//! command line inside their app (`Cli`) also open a file at a line and two files as a diff,
//! which a PR's changed files use (prdiff.rs); the rest open a file as a folder opens. On Linux
//! an editor is its command on the PATH (`Known::commands`), which is its command line too, and
//! `[apps] editor` can name any other command.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use serde::Serialize;

use crate::config::Profile;
use crate::os::{self, open::App, Os};
use crate::procs;
use crate::util::home;

/// An editor Codealong knows, by an id of its own.
struct Known {
    id: &'static str,
    name: &'static str,
    /// Its app's names, the first found wins: some come in editions (Community, beta).
    apps: &'static [&'static str],
    /// Its command line, inside its app.
    cli: Option<Cli>,
    /// Its commands on Linux, the first found on the PATH wins; none for a Mac-only one.
    commands: &'static [&'static str],
}

/// An editor's command line: where it is in its app, and how it's told a line. Both kinds take
/// `--diff <old> <new>`: VS Code's and its forks' (`code --help`) and Zed's (`cli --help`).
#[derive(Clone, Copy)]
struct Cli {
    /// A name in VS Code's `Contents/Resources/app/bin`, or a path in the app.
    path: &'static str,
    goto: Goto,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Goto {
    /// `--goto <file>:<line>`: VS Code and its forks.
    Flag,
    /// `<file>:<line>`: Zed.
    Suffix,
}

/// VS Code's command line, as each of its forks names it.
const fn code(name: &'static str) -> Option<Cli> {
    Some(Cli { path: name, goto: Goto::Flag })
}

const ZED: Option<Cli> = Some(Cli { path: "Contents/MacOS/cli", goto: Goto::Suffix });

/// In the order the Open in menus list them.
const KNOWN: &[Known] = &[
    Known { id: "vscode", name: "VS Code", apps: &["Visual Studio Code"], cli: code("code"), commands: &["code"] },
    Known { id: "vscode-insiders", name: "VS Code Insiders", apps: &["Visual Studio Code - Insiders"], cli: code("code-insiders"), commands: &["code-insiders"] },
    Known { id: "cursor", name: "Cursor", apps: &["Cursor"], cli: code("cursor"), commands: &["cursor"] },
    Known { id: "windsurf", name: "Windsurf", apps: &["Windsurf"], cli: code("windsurf"), commands: &["windsurf"] },
    Known { id: "zed", name: "Zed", apps: &["Zed"], cli: ZED, commands: &["zed", "zeditor"] },
    Known { id: "zed-preview", name: "Zed Preview", apps: &["Zed Preview"], cli: ZED, commands: &[] },
    Known { id: "kiro", name: "Kiro", apps: &["Kiro"], cli: code("kiro"), commands: &["kiro"] },
    Known { id: "sublime", name: "Sublime Text", apps: &["Sublime Text"], cli: None, commands: &["subl"] },
    Known { id: "nova", name: "Nova", apps: &["Nova"], cli: None, commands: &[] },
    Known { id: "bbedit", name: "BBEdit", apps: &["BBEdit"], cli: None, commands: &[] },
    Known { id: "xcode", name: "Xcode", apps: &["Xcode", "Xcode-beta"], cli: None, commands: &[] },
    Known { id: "intellij", name: "IntelliJ IDEA", apps: &["IntelliJ IDEA", "IntelliJ IDEA Ultimate", "IntelliJ IDEA CE", "IntelliJ IDEA Community Edition"], cli: None, commands: &["idea", "intellij-idea-ultimate", "intellij-idea-community"] },
    Known { id: "webstorm", name: "WebStorm", apps: &["WebStorm"], cli: None, commands: &["webstorm"] },
    Known { id: "pycharm", name: "PyCharm", apps: &["PyCharm", "PyCharm Professional Edition", "PyCharm CE", "PyCharm Community Edition"], cli: None, commands: &["pycharm", "pycharm-professional", "pycharm-community"] },
    Known { id: "goland", name: "GoLand", apps: &["GoLand"], cli: None, commands: &["goland"] },
    Known { id: "rustrover", name: "RustRover", apps: &["RustRover"], cli: None, commands: &["rustrover"] },
    Known { id: "clion", name: "CLion", apps: &["CLion"], cli: None, commands: &["clion"] },
    Known { id: "android-studio", name: "Android Studio", apps: &["Android Studio"], cli: None, commands: &["android-studio", "studio"] },
];

/// Where editors are looked for: on macOS where apps are installed, by hand or Homebrew, for one
/// user (JetBrains Toolbox puts its IDEs there, or in a folder of its own there), and by Setapp;
/// on Linux the folders on your PATH.
fn app_dirs() -> Vec<PathBuf> {
    match os::current() {
        Os::MacOs => {
            let h = home();
            vec![PathBuf::from("/Applications"), h.join("Applications"), h.join("Applications/JetBrains Toolbox"), PathBuf::from("/Applications/Setapp")]
        }
        Os::Linux => procs::user_path().split(':').filter(|d| !d.is_empty()).map(PathBuf::from).collect(),
    }
}

/// Where a known editor is: its app on macOS, its command on Linux.
fn find_in(dirs: &[PathBuf], k: &Known) -> Option<PathBuf> {
    match os::current() {
        Os::MacOs => k.apps.iter().flat_map(|app| dirs.iter().map(move |d| d.join(format!("{app}.app")))).find(|p| p.is_dir()),
        Os::Linux => k.commands.iter().flat_map(|c| dirs.iter().map(move |d| d.join(c))).find(|p| p.is_file()),
    }
}

/// The known editors installed, in menu order, with where each one's app is.
fn found_in(dirs: &[PathBuf]) -> Vec<(&'static Known, PathBuf)> {
    KNOWN.iter().filter_map(|k| find_in(dirs, k).map(|p| (k, p))).collect()
}

fn installed_in(dirs: &[PathBuf]) -> Vec<Profile> {
    found_in(dirs).into_iter().map(|(k, _)| Profile { id: k.id.into(), name: k.name.into() }).collect()
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

/// What `open -a` is given for `id` on macOS: a known editor's app where it's installed (its
/// first name when it isn't found, for Launch Services to look for), else `id` itself, an app's
/// name or path.
fn app_for(dirs: &[PathBuf], id: &str) -> String {
    match KNOWN.iter().find(|k| k.id == id) {
        Some(k) => find_in(dirs, k).map_or_else(|| k.apps[0].to_string(), |p| p.to_string_lossy().into_owned()),
        None => id.to_string(),
    }
}

/// What runs the editor `id` on Linux: a known editor's command where it's found, else `id`
/// itself, a command on the PATH or a path.
fn command_for(dirs: &[PathBuf], id: &str) -> Option<PathBuf> {
    match KNOWN.iter().find(|k| k.id == id) {
        Some(k) => find_in(dirs, k),
        None if id.contains('/') => Some(crate::util::expand(id)).filter(|p| p.is_file()),
        None => dirs.iter().map(|d| d.join(id)).find(|p| p.is_file()),
    }
}

/// Opens `path`, a folder or a file, in the editor `id` names: its app on macOS; on Linux its
/// command, left to run by itself, since some (JetBrains') stay until you quit.
pub fn open(id: &str, path: &Path) -> Result<()> {
    let dirs = app_dirs();
    let path = path.to_string_lossy();
    match os::current() {
        Os::MacOs => os::open::in_app(App::Named(&app_for(&dirs, id)), Some(&path), Duration::from_secs(15))
            .map_err(|_| anyhow!("Couldn't open {}. Is it installed?", name(id))),
        Os::Linux => {
            let program = command_for(&dirs, id).ok_or_else(|| anyhow!("Couldn't open {}. Is it installed?", name(id)))?;
            os::open::start_detached(&program.to_string_lossy(), &[&path], Duration::from_secs(15)).map_err(|e| anyhow!("Couldn't open {}: {e}", name(id)))
        }
    }
}

/// The command line of the editor `id` where it's installed: the program, and how it takes a
/// line. On macOS it's the one inside the app, so nobody has to have put it on their PATH; on
/// Linux an editor's command is its command line.
fn cli_in(dirs: &[PathBuf], id: &str) -> Option<(PathBuf, Goto)> {
    let k = KNOWN.iter().find(|k| k.id == id)?;
    let cli = k.cli?;
    let found = find_in(dirs, k)?;
    let program = match os::current() {
        Os::MacOs if cli.path.contains('/') => found.join(cli.path),
        Os::MacOs => found.join("Contents/Resources/app/bin").join(cli.path),
        Os::Linux => found,
    };
    program.is_file().then_some((program, cli.goto))
}

/// The editor a diff opens in: yours (`[apps] editor`) when its command line takes one, else the
/// first installed that does, in menu order; none when none does. Looked for at most once a
/// minute for the same editor of yours, as `installed` is.
pub fn diff_editor(yours: &str) -> Option<Profile> {
    static FOUND: Mutex<Option<(Instant, String, Option<Profile>)>> = Mutex::new(None);
    let mut found = FOUND.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((at, of, editor)) = found.as_ref() {
        if of == yours && at.elapsed() < Duration::from_secs(60) {
            return editor.clone();
        }
    }
    let editor = diff_editor_in(&app_dirs(), yours);
    *found = Some((Instant::now(), yours.to_string(), editor.clone()));
    editor
}

fn diff_editor_in(dirs: &[PathBuf], yours: &str) -> Option<Profile> {
    std::iter::once(yours).chain(KNOWN.iter().map(|k| k.id)).find(|id| cli_in(dirs, id).is_some()).map(|id| Profile { id: id.into(), name: name(id) })
}

/// What an editor's command line is given to open `file` at `line`.
fn goto_args(goto: Goto, file: &Path, line: Option<u64>) -> Vec<String> {
    let file = file.to_string_lossy();
    match (goto, line) {
        (_, None) => vec![file.into_owned()],
        (Goto::Flag, Some(n)) => vec!["--goto".into(), format!("{file}:{n}")],
        (Goto::Suffix, Some(n)) => vec![format!("{file}:{n}")],
    }
}

/// What an editor's own terminal sets to hand its command line to that window: left out, so what
/// Codealong opens goes to the app as a click there would.
const EDITOR_HANDOFF: &[&str] = &["VSCODE_IPC_HOOK_CLI", "ELECTRON_RUN_AS_NODE"];

fn run_cli(program: &Path, args: &[String], id: &str) -> Result<()> {
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    procs::run_without(&program.to_string_lossy(), &args, EDITOR_HANDOFF, Duration::from_secs(20)).map(|_| ()).map_err(|e| anyhow!("Couldn't open {}: {e}", name(id)))
}

/// Opens `file` in the editor `id`, at `line`: on its command line where it has one, else as a
/// file opens (`open`), at its top.
pub fn open_at(id: &str, file: &Path, line: Option<u64>) -> Result<()> {
    match cli_in(&app_dirs(), id) {
        Some((program, goto)) => run_cli(&program, &goto_args(goto, file, line), id),
        None => open(id, file),
    }
}

/// Opens `old` and `new` side by side as a diff in the editor `id`, whose command line must take
/// one (`diff_editor`).
pub fn diff(id: &str, old: &Path, new: &Path) -> Result<()> {
    let (program, _) = cli_in(&app_dirs(), id).ok_or_else(|| anyhow!("{} can't open a diff from Codealong.", name(id)))?;
    run_cli(&program, &["--diff".into(), old.to_string_lossy().into_owned(), new.to_string_lossy().into_owned()], id)
}

/// The side editors' icons are drawn at, in pixels: menus show an item's image 18 points high
/// (muda), and pages at 16, so this is sharp at 2x in both.
const ICON_PX: usize = 36;

/// An installed editor's app icon, `size` pixels square: straight (not premultiplied) RGBA, top
/// row first, as native menus take it and a page's canvas does.
#[derive(Debug, Clone, Serialize)]
pub struct Icon {
    pub id: String,
    pub size: usize,
    pub rgba: Vec<u8>,
}

/// The icons of the editors installed here, in menu order. Each app's is drawn once a run.
/// AppKit draws them, into a bitmap of their own, which it does on any thread.
pub fn icons() -> Vec<Icon> {
    static DRAWN: Mutex<Vec<(PathBuf, Option<Vec<u8>>)>> = Mutex::new(Vec::new());
    let mut drawn = DRAWN.lock().unwrap_or_else(|e| e.into_inner());
    found_in(&app_dirs())
        .into_iter()
        .filter_map(|(k, app)| {
            let rgba = match drawn.iter().find(|(p, _)| *p == app) {
                Some((_, rgba)) => rgba.clone(),
                None => {
                    let rgba = crate::os::apps::icon(&app, ICON_PX);
                    drawn.push((app, rgba.clone()));
                    rgba
                }
            };
            Some(Icon { id: k.id.to_string(), size: ICON_PX, rgba: rgba? })
        })
        .collect()
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
        let _mac = os::AsIf::new(Os::MacOs);
        let root = crate::fixtures::scratch("editors");
        let dirs = apps_in(&root, &["Applications/Zed.app", "Applications/Visual Studio Code.app", "home/Applications/PyCharm CE.app", "Applications/Notes.app"]);
        let ids: Vec<String> = installed_in(&dirs).into_iter().map(|p| p.id).collect();
        assert_eq!(ids, ["vscode", "zed", "pycharm"]);
        // A known editor opens from where it was found, and one that wasn't by its app's name.
        assert_eq!(app_for(&dirs, "pycharm"), root.join("home/Applications/PyCharm CE.app").to_string_lossy());
        assert_eq!(app_for(&dirs, "cursor"), "Cursor");
        // Anything else is an app's name or path, for `open -a`.
        assert_eq!(app_for(&dirs, "TextMate"), "TextMate");
    }

    #[test]
    fn a_diff_opens_in_yours_when_it_can_else_the_first_that_can() {
        let _mac = os::AsIf::new(Os::MacOs);
        let root = crate::fixtures::scratch("editors-diff");
        let dirs = apps_in(&root, &["Applications/Zed.app/Contents/MacOS", "Applications/Cursor.app/Contents/Resources/app/bin", "Applications/Sublime Text.app"]);
        std::fs::write(root.join("Applications/Zed.app/Contents/MacOS/cli"), "").unwrap();
        // Cursor without its command line takes no diff.
        assert_eq!(diff_editor_in(&dirs, "cursor").map(|p| p.id).as_deref(), Some("zed"));
        std::fs::write(root.join("Applications/Cursor.app/Contents/Resources/app/bin/cursor"), "").unwrap();
        assert_eq!(diff_editor_in(&dirs, "cursor").map(|p| p.id).as_deref(), Some("cursor"));
        assert_eq!(diff_editor_in(&dirs, "zed").map(|p| p.name).as_deref(), Some("Zed"));
        // Yours can't, so the first that can, in menu order.
        assert_eq!(diff_editor_in(&dirs, "sublime").map(|p| p.id).as_deref(), Some("cursor"));
        assert_eq!(diff_editor_in(&dirs, "TextMate").map(|p| p.id).as_deref(), Some("cursor"));
        assert_eq!(cli_in(&dirs, "cursor").map(|(p, _)| p), Some(root.join("Applications/Cursor.app/Contents/Resources/app/bin/cursor")));
        assert_eq!(cli_in(&dirs, "zed").map(|(_, g)| g), Some(Goto::Suffix));
        assert_eq!(diff_editor_in(&[root.join("nowhere")], "cursor"), None);
    }

    /// On Linux an editor is its command on the PATH, which takes a line and a diff when the Mac
    /// app's command line does.
    #[test]
    fn finds_linux_editors_by_their_commands_in_menu_order() {
        let _linux = os::AsIf::new(Os::Linux);
        let root = crate::fixtures::scratch("editors-linux");
        let dirs = vec![root.join("bin"), root.join("local/bin")];
        for (d, c) in [("local/bin", "zeditor"), ("bin", "code"), ("bin", "pycharm-community"), ("bin", "subl"), ("bin", "ls")] {
            std::fs::create_dir_all(root.join(d)).unwrap();
            std::fs::write(root.join(d).join(c), "").unwrap();
        }
        let ids: Vec<String> = installed_in(&dirs).into_iter().map(|p| p.id).collect();
        assert_eq!(ids, ["vscode", "zed", "sublime", "pycharm"]);
        assert_eq!(cli_in(&dirs, "vscode"), Some((root.join("bin/code"), Goto::Flag)));
        assert_eq!(cli_in(&dirs, "zed"), Some((root.join("local/bin/zeditor"), Goto::Suffix)));
        // Sublime takes no diff, so a diff opens in the first that does.
        assert_eq!(cli_in(&dirs, "sublime"), None);
        assert_eq!(diff_editor_in(&dirs, "sublime").map(|p| p.id).as_deref(), Some("vscode"));
        assert_eq!(diff_editor_in(&[root.join("nowhere")], "vscode"), None);
        // Anything else is a command on the PATH, or a path.
        assert_eq!(command_for(&dirs, "ls"), Some(root.join("bin/ls")));
        assert_eq!(command_for(&dirs, "nano"), None);
    }

    #[test]
    fn says_a_line_as_each_command_line_takes_it() {
        let file = Path::new("/w/src/a b.ts");
        assert_eq!(goto_args(Goto::Flag, file, Some(12)), ["--goto", "/w/src/a b.ts:12"]);
        assert_eq!(goto_args(Goto::Suffix, file, Some(12)), ["/w/src/a b.ts:12"]);
        assert_eq!(goto_args(Goto::Flag, file, None), ["/w/src/a b.ts"]);
    }

    #[test]
    fn names_known_editors_and_apps() {
        assert_eq!(name("vscode"), "VS Code");
        assert_eq!(name("cursor"), "Cursor");
        assert_eq!(name("TextMate"), "TextMate");
        assert_eq!(name("/Applications/CotEditor.app"), "CotEditor");
    }


    /// Draws the icons of the editors on this Mac and says how much of each is covered, which
    /// is none for an icon that didn't draw. `cargo test editor_icons_draw -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn editor_icons_draw() {
        for icon in icons() {
            let covered = icon.rgba.chunks(4).filter(|p| p[3] > 0).count();
            println!("{:<16} {covered} of {} pixels", icon.id, icon.size * icon.size);
            assert!(covered > icon.size * icon.size / 2, "{} barely drew", icon.id);
        }
    }

    #[test]
    fn menus_take_the_icons_as_pages_send_them() {
        // contextMenu.ts sends an item's icon as {rgba, width, height}: Tauri reads that as raw
        // RGBA, which needs none of its image features.
        let icon = Icon { id: "zed".into(), size: 2, rgba: vec![0; 16] };
        let sent = serde_json::json!({ "rgba": icon.rgba, "width": icon.size, "height": icon.size });
        assert!(matches!(serde_json::from_value::<tauri::image::JsImage>(sent), Ok(tauri::image::JsImage::Rgba { .. })));
    }

    #[test]
    fn ids_are_unique() {
        let mut ids: Vec<&str> = KNOWN.iter().map(|k| k.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), KNOWN.len());
    }
}
