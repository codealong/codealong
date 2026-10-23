//! Automatic updates (design/plans/releases.md). Tauri's updater plugin asks for the manifest
//! the latest GitHub release carries (`plugins.updater` in tauri.conf.json), downloads a newer
//! version's archive, checks its signature against the key compiled into the app, and puts the
//! new bundle in place of this one. The running app carries on as it is; the new version starts
//! at the next launch, or when you choose Restart to update.
//!
//! Only release builds update themselves: scripts/release.sh builds with `CODEALONG_RELEASE`
//! set. A build from relaunch.sh that found a release would replace itself with it.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use tauri::menu::MenuItem;
use tauri::{AppHandle, Emitter, Manager, Wry};
use tauri_plugin_updater::UpdaterExt;

use crate::util::now_ms;

/// A launch has the Mac to itself for its first scan and git runs; the first check waits them out.
const FIRST_CHECK: Duration = Duration::from_secs(120);
/// How often to look again, going by the clock, so a Mac that slept through a check catches up.
const EVERY_MS: i64 = 6 * 60 * 60 * 1000;
/// How often the loop wakes to see whether a check is due.
const TICK: Duration = Duration::from_secs(10 * 60);
/// A check, or a download, that takes longer than this has failed.
const TIMEOUT: Duration = Duration::from_secs(300);

pub const MENU_ID: &str = "check-updates";
const MENU_CHECK: &str = "Check for updates…";

/// Where updates stand, as Settings and the sidebar show it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum Status {
    /// A build that doesn't update itself: one from relaunch.sh or `tauri dev`.
    Off,
    /// Nothing asked yet since launch.
    Idle,
    Checking,
    /// The latest release is this version, as of `checkedAt`.
    #[serde(rename_all = "camelCase")]
    UpToDate { checked_at: i64 },
    /// Downloading a newer version and putting it in place.
    Installing { version: String },
    /// A newer version is in place and starts at the next launch.
    Ready { version: String, notes: Option<String> },
    /// The last check or install didn't work; the next one tries again.
    Failed { error: String, at: i64 },
}

/// What the windows get: this copy's version, and where updates stand.
#[derive(Debug, Clone, Serialize)]
pub struct View {
    pub current: String,
    #[serde(flatten)]
    pub status: Status,
}

#[derive(Default)]
pub struct Updates {
    status: Mutex<Option<Status>>,
    /// When the last check finished, by the clock (ms), for the next one's turn.
    last_check: Mutex<i64>,
    menu: Mutex<Option<MenuItem<Wry>>>,
}

/// Whether this build updates itself.
pub fn enabled() -> bool {
    option_env!("CODEALONG_RELEASE").is_some()
}

/// Puts Check for updates… under About in the app menu, and, in a release build, looks for an
/// update now and then from a thread of its own.
pub fn setup(app: &AppHandle) {
    app.manage(Updates::default());
    set(app, if enabled() { Status::Idle } else { Status::Off });
    if let Err(e) = add_menu(app) {
        eprintln!("Couldn't add Check for updates to the menu: {e:#}");
    }
    if !enabled() {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(FIRST_CHECK);
        loop {
            let last = *app.state::<Updates>().last_check.lock().unwrap();
            if now_ms() - last >= EVERY_MS && crate::config::updates_check() {
                check(&app);
            }
            std::thread::sleep(TICK);
        }
    });
}

pub fn view(app: &AppHandle) -> View {
    let status = app.state::<Updates>().status.lock().unwrap().clone().unwrap_or(Status::Off);
    View { current: app.package_info().version.to_string(), status }
}

fn set(app: &AppHandle, status: Status) {
    let updates = app.state::<Updates>();
    *updates.status.lock().unwrap() = Some(status.clone());
    // Taken out of the lock first: off the main thread `set_text` waits for it, which may be in
    // `begin` here, wanting the lock.
    let item = updates.menu.lock().unwrap().clone();
    if let Some(item) = item {
        let text = match &status {
            Status::Ready { version, .. } => format!("Restart to update to {version}"),
            _ => MENU_CHECK.to_string(),
        };
        let _ = item.set_text(text);
    }
    let _ = app.emit("update-status", view(app));
}

/// Looks for a newer release and, when there is one, puts it in place. Runs on the caller's
/// thread; one check at a time, and none once an update waits for a restart.
pub fn check(app: &AppHandle) {
    if begin(app) {
        finish(app);
    }
}

/// Starts a check on a thread of its own, for a click. It's marked as checking before this
/// returns, so a window asked next to say how it goes doesn't read the last check's outcome.
pub fn check_soon(app: &AppHandle) {
    if begin(app) {
        let app = app.clone();
        std::thread::spawn(move || finish(&app));
    }
}

/// Marks a check as under way; false when this build doesn't update, or a check, an install or
/// an update waiting for a restart is already there.
fn begin(app: &AppHandle) -> bool {
    if !enabled() {
        return false;
    }
    {
        let updates = app.state::<Updates>();
        let mut status = updates.status.lock().unwrap();
        if matches!(status.as_ref(), Some(Status::Checking | Status::Installing { .. } | Status::Ready { .. })) {
            return false;
        }
        *status = Some(Status::Checking);
    }
    set(app, Status::Checking);
    true
}

fn finish(app: &AppHandle) {
    let outcome = tauri::async_runtime::block_on(install_newer(app));
    *app.state::<Updates>().last_check.lock().unwrap() = now_ms();
    set(
        app,
        match outcome {
            Ok(None) => Status::UpToDate { checked_at: now_ms() },
            Ok(Some((version, notes))) => Status::Ready { version, notes },
            Err(error) => Status::Failed { error, at: now_ms() },
        },
    );
}

/// The newer version put in place, with its release notes; `None` when this is the latest.
async fn install_newer(app: &AppHandle) -> Result<Option<(String, Option<String>)>, String> {
    let mut builder = app.updater_builder().timeout(TIMEOUT);
    // A manifest of your own, to try an update before it's released.
    if let Ok(url) = std::env::var("CODEALONG_UPDATE_URL") {
        let url: tauri::Url = url.parse().map_err(|e| format!("CODEALONG_UPDATE_URL isn't a URL: {e}"))?;
        builder = builder.endpoints(vec![url]).map_err(|e| e.to_string())?;
    }
    let updater = builder.build().map_err(|e| format!("Couldn't check for updates: {e}"))?;
    let Some(update) = updater.check().await.map_err(|e| format!("Couldn't check for updates: {e}"))? else {
        return Ok(None);
    };
    if let Some(why) = std::env::current_exe().ok().as_deref().and_then(bundle_of).and_then(|b| cant_replace(&b)) {
        return Err(why);
    }
    set(app, Status::Installing { version: update.version.clone() });
    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(|e| format!("Couldn't install Codealong {}: {e}", update.version))?;
    Ok(Some((update.version.clone(), update.body.clone().filter(|n| !n.trim().is_empty()))))
}

/// The app bundle an executable inside it belongs to: `…/Codealong.app` for
/// `…/Codealong.app/Contents/MacOS/codealong`.
pub(crate) fn bundle_of(exe: &Path) -> Option<PathBuf> {
    let bundle = exe.parent()?.parent()?.parent()?;
    (bundle.extension()? == "app").then(|| bundle.to_path_buf())
}

/// Why an update can't take this bundle's place, when it can't: macOS runs an app opened where
/// it was downloaded from a read-only copy of it (App Translocation), and the disk image is
/// read-only too. Moved to Applications, it updates.
fn cant_replace(bundle: &Path) -> Option<String> {
    let from_download = bundle.to_string_lossy().contains("/AppTranslocation/");
    if from_download || read_only(bundle) {
        return Some("Move Codealong to your Applications folder to get updates: it's running from where it was downloaded, which can't be changed.".into());
    }
    None
}

fn read_only(path: &Path) -> bool {
    use std::os::unix::ffi::OsStrExt;
    let Ok(c) = std::ffi::CString::new(path.as_os_str().as_bytes()) else { return false };
    let mut fs: libc::statvfs = unsafe { std::mem::zeroed() };
    unsafe { libc::statvfs(c.as_ptr(), &mut fs) == 0 && fs.f_flag & libc::ST_RDONLY != 0 }
}

/// Check for updates… right under About; Restart to update once one is in place.
fn add_menu(app: &AppHandle) -> tauri::Result<()> {
    use tauri::menu::MenuItemKind;
    let Some(menu) = app.menu() else { return Ok(()) };
    let Some(MenuItemKind::Submenu(first)) = menu.items()?.into_iter().next() else { return Ok(()) };
    let item = MenuItem::with_id(app, MENU_ID, MENU_CHECK, true, None::<&str>)?;
    first.insert(&item, 1)?;
    *app.state::<Updates>().menu.lock().unwrap() = Some(item);
    app.on_menu_event(|app, event| {
        if event.id().as_ref() != MENU_ID {
            return;
        }
        let ready = matches!(view(app).status, Status::Ready { .. });
        if ready {
            app.restart();
        }
        check_soon(app);
        // The window in front says how the check goes, in a toast; Settings isn't opened for it.
        match crate::front_window(app) {
            Some(w) => {
                let _ = app.emit_to(w.label(), "check-updates", ());
            }
            None => {
                let _ = crate::front::show_main(app, None);
                let _ = app.emit_to("main", "check-updates", ());
            }
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_bundle_of_its_executable() {
        assert_eq!(bundle_of(Path::new("/Applications/Codealong.app/Contents/MacOS/codealong")), Some(PathBuf::from("/Applications/Codealong.app")));
        assert_eq!(bundle_of(Path::new("/Users/me/codealong-target/release/codealong")), None);
    }

    #[test]
    fn a_translocated_copy_cant_be_replaced() {
        let translocated = Path::new("/private/var/folders/x/T/AppTranslocation/1234/d/Codealong.app");
        assert!(cant_replace(translocated).unwrap().contains("Applications"));
        // The home folder is writable wherever this runs.
        assert_eq!(cant_replace(&crate::util::home()), None);
    }

    /// A release's update, end to end, through the updater itself: a local server hands out its
    /// latest.json and archive, the public key is tauri.conf.json's, and the update replaces a copy
    /// of an app bundle as it would the installed one. Before a release goes out:
    ///   CODEALONG_UPDATE_TEST=<release dir holding latest.json> CODEALONG_UPDATE_TEST_APP=<an .app> \
    ///     cargo test an_update_replaces_the_app -- --ignored --nocapture
    #[test]
    #[ignore]
    fn an_update_replaces_the_app() {
        use std::io::{Read, Write};
        let dist = PathBuf::from(std::env::var("CODEALONG_UPDATE_TEST").expect("CODEALONG_UPDATE_TEST: a release's dist folder"));
        let installed = PathBuf::from(std::env::var("CODEALONG_UPDATE_TEST_APP").expect("CODEALONG_UPDATE_TEST_APP: an app bundle to replace"));
        let mut manifest: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dist.join("latest.json")).unwrap()).unwrap();
        let version = manifest["version"].as_str().unwrap().to_string();

        // The manifest and archive, from a server of the test's own on loopback.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        for (_, entry) in manifest["platforms"].as_object_mut().unwrap() {
            let name = entry["url"].as_str().unwrap().rsplit('/').next().unwrap().to_string();
            entry["url"] = format!("{base}/{name}").into();
        }
        let served = serde_json::to_vec(&manifest).unwrap();
        let folder = dist.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let mut stream = stream;
                let mut request = Vec::new();
                let mut buf = [0; 4096];
                while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                    let n = stream.read(&mut buf).unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    request.extend_from_slice(&buf[..n]);
                }
                let line = String::from_utf8_lossy(&request).lines().next().unwrap_or_default().to_string();
                let path = line.split(' ').nth(1).unwrap_or("/").trim_start_matches('/').to_string();
                let body = if path == "latest.json" { served.clone() } else { std::fs::read(folder.join(&path)).unwrap_or_default() };
                let _ = write!(stream, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
                let _ = stream.write_all(&body);
            }
        });

        // A copy of an installed app, which the update replaces.
        let tmp = std::env::temp_dir().join(format!("codealong-update-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let copy = tmp.join("Codealong.app");
        assert!(std::process::Command::new("ditto").arg(&installed).arg(&copy).status().unwrap().success());

        let conf: serde_json::Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let mut context = tauri::test::mock_context(tauri::test::noop_assets());
        context.config_mut().plugins.0.insert(
            "updater".into(),
            serde_json::json!({ "pubkey": conf["plugins"]["updater"]["pubkey"], "endpoints": [format!("{base}/latest.json")], "requireSignedVersion": true }),
        );
        context.package_info_mut().version = "0.0.1".parse().unwrap();
        let app = tauri::test::mock_builder().plugin(tauri_plugin_updater::Builder::new().build()).build(context).unwrap();
        let updater = app.updater_builder().executable_path(copy.join("Contents/MacOS/codealong")).build().unwrap();
        let update = tauri::async_runtime::block_on(updater.check()).unwrap().expect("the release is newer than 0.0.1");
        assert_eq!(update.version, version);
        tauri::async_runtime::block_on(update.download_and_install(|_, _| {}, || {})).unwrap();

        let plist = std::process::Command::new("plutil").args(["-extract", "CFBundleShortVersionString", "raw"]).arg(copy.join("Contents/Info.plist")).output().unwrap();
        assert_eq!(String::from_utf8_lossy(&plist.stdout).trim(), version);
        let verify = std::process::Command::new("codesign").args(["--verify", "--deep", "--strict"]).arg(&copy).status().unwrap();
        assert!(verify.success(), "the replaced app's signature doesn't verify");
        // The copy is now the release's own app, not what was there (which may be the same version).
        let archive = manifest["platforms"].as_object().unwrap().values().next().unwrap()["url"].as_str().unwrap().rsplit('/').next().unwrap().to_string();
        let unpacked = tmp.join("unpacked");
        std::fs::create_dir_all(&unpacked).unwrap();
        assert!(std::process::Command::new("tar").arg("-xzf").arg(dist.join(&archive)).arg("-C").arg(&unpacked).status().unwrap().success());
        let binary = |app: &Path| std::fs::read(app.join("Contents/MacOS/codealong")).unwrap();
        assert!(binary(&copy) == binary(&unpacked.join("Codealong.app")), "the app in place isn't the release's");
        assert!(binary(&installed) != binary(&copy) || installed == unpacked, "the app given to replace was already this release's; give another to see a replacement");
        println!("Replaced {} with Codealong {version} from the release's own update.", copy.display());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn a_status_says_its_state_to_the_windows() {
        let ready = View { current: "0.2.0".into(), status: Status::Ready { version: "0.3.0".into(), notes: Some("Faster".into()) } };
        assert_eq!(serde_json::to_value(ready).unwrap(), serde_json::json!({"current": "0.2.0", "state": "ready", "version": "0.3.0", "notes": "Faster"}));
        let checked = View { current: "0.2.0".into(), status: Status::UpToDate { checked_at: 5 } };
        assert_eq!(serde_json::to_value(checked).unwrap(), serde_json::json!({"current": "0.2.0", "state": "upToDate", "checkedAt": 5}));
        let off = View { current: "0.2.0".into(), status: Status::Off };
        assert_eq!(serde_json::to_value(off).unwrap(), serde_json::json!({"current": "0.2.0", "state": "off"}));
    }
}
