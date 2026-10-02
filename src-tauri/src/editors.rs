//! The editors a folder opens in: the ones Codealong knows, found where Mac apps are installed,
//! and any other app `[apps] editor` names. See *Apps*.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use serde::Serialize;

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
    procs::open(&["-a", &app, &path.to_string_lossy()], Duration::from_secs(15))
        .map_err(|_| anyhow!("Couldn't open {}. Is it installed?", name(id)))
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
                    let rgba = draw_icon(&app);
                    drawn.push((app, rgba.clone()));
                    rgba
                }
            };
            Some(Icon { id: k.id.to_string(), size: ICON_PX, rgba: rgba? })
        })
        .collect()
}

/// Draws the app's icon as the Finder shows it, `ICON_PX` square.
fn draw_icon(app: &Path) -> Option<Vec<u8>> {
    use objc2::AllocAnyThread;
    use objc2_app_kit::{NSBitmapImageRep, NSCompositingOperation, NSDeviceRGBColorSpace, NSGraphicsContext, NSImageInterpolation, NSWorkspace};
    use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};
    let px = ICON_PX as isize;
    let icon = NSWorkspace::sharedWorkspace().iconForFile(&NSString::from_str(&app.to_string_lossy()));
    // 8 bits a sample, RGBA, rows packed: the layout `pixels` reads.
    // SAFETY: with no planes given AppKit allocates the pixels itself and owns them, sized by the
    // numbers here, which agree with each other (4 samples of 8 bits are the 32 bits of a pixel
    // and the `px * 4` bytes of a row); the color space's name is AppKit's constant.
    let rep = unsafe { NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bytesPerRow_bitsPerPixel(NSBitmapImageRep::alloc(), std::ptr::null_mut(), px, px, 8, 4, true, false, NSDeviceRGBColorSpace, px * 4, 32) }?;
    let ctx = NSGraphicsContext::graphicsContextWithBitmapImageRep(&rep)?;
    NSGraphicsContext::saveGraphicsState_class();
    NSGraphicsContext::setCurrentContext(Some(&ctx));
    ctx.setImageInterpolation(NSImageInterpolation::High);
    let all = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(ICON_PX as f64, ICON_PX as f64));
    // An empty source rect draws the whole image, from the representation that best fits.
    icon.drawInRect_fromRect_operation_fraction(all, NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(0.0, 0.0)), NSCompositingOperation::Copy, 1.0);
    ctx.flushGraphics();
    NSGraphicsContext::restoreGraphicsState_class();
    let data = rep.bitmapData();
    if data.is_null() || rep.bytesPerRow() != px * 4 {
        return None;
    }
    // SAFETY: the pixels aren't null and their rows are `px * 4` bytes, both just checked, and
    // there are `px` rows, as asked for above: that many bytes are the rep's, which is alive until
    // this returns and which nothing draws into again; `straight` copies them out.
    let premultiplied = unsafe { std::slice::from_raw_parts(data, ICON_PX * ICON_PX * 4) };
    Some(straight(premultiplied))
}

/// Premultiplied RGBA, as AppKit draws, made straight, as menus and canvases take it.
fn straight(rgba: &[u8]) -> Vec<u8> {
    rgba.chunks_exact(4)
        .flat_map(|p| {
            let a = p[3] as u32;
            let un = |c: u8| if a == 0 { 0 } else { ((c as u32 * 255 + a / 2) / a).min(255) as u8 };
            [un(p[0]), un(p[1]), un(p[2]), p[3]]
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
    fn names_known_editors_and_apps() {
        assert_eq!(name("vscode"), "VS Code");
        assert_eq!(name("cursor"), "Cursor");
        assert_eq!(name("TextMate"), "TextMate");
        assert_eq!(name("/Applications/CotEditor.app"), "CotEditor");
    }

    #[test]
    fn unpremultiplies_what_appkit_draws() {
        // Opaque stays, clear goes to zero, half-covered comes back to its color.
        assert_eq!(straight(&[10, 20, 30, 255, 5, 5, 5, 0, 64, 32, 0, 128]), [10, 20, 30, 255, 0, 0, 0, 0, 128, 64, 0, 128]);
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
