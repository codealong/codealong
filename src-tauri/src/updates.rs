//! Automatic updates (design/plans/releases.md). Tauri's updater plugin asks for the manifest
//! the latest GitHub release carries (`plugins.updater` in tauri.conf.json: through
//! codealong.com, which counts the check, and from GitHub itself when that fails), downloads a newer
//! version's archive, checks its signature against the key compiled into the app, and puts the
//! new bundle in place of this one. The running app carries on as it is; the new version starts
//! at the next launch, or when you choose Restart to update.
//!
//! Only release builds update themselves: scripts/release.sh builds with `CODEALONG_RELEASE`
//! set. A build from relaunch.sh that found a release would replace itself with it. Codealong Dev,
//! built from a clean checkout of the source by relaunch.sh (`CODEALONG_COMMIT`), looks at the
//! source's origin/main instead now and then (`dev_check`), and once it has commits the build
//! hasn't, the sidebar offers a session that pulls them and relaunches it (src/agentTasks.ts
//! `devUpdateTask`).

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use tauri::menu::MenuItem;
use tauri::{AppHandle, Emitter, Manager, Runtime, Wry};
use tauri_plugin_updater::{Update, UpdaterExt};

use crate::util::now_ms;

/// A launch has the Mac to itself for its first scan and git runs; the first check waits them out.
const FIRST_CHECK: Duration = Duration::from_secs(120);
/// How often to look again, going by the clock, so a Mac that slept through a check catches up.
const EVERY_MS: i64 = 6 * 60 * 60 * 1000;
/// How often the loop wakes to see whether a check is due.
const TICK: Duration = Duration::from_secs(10 * 60);
/// A check, or a download, that takes longer than this has failed.
const TIMEOUT: Duration = Duration::from_secs(300);
/// How often a development build looks at its source's origin/main (`dev_check`).
const DEV_EVERY: Duration = Duration::from_secs(15 * 60);
/// A fetch of origin/main that takes longer than this has failed; the last one fetched is counted.
const FETCH_TIMEOUT: Duration = Duration::from_secs(60);

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

/// What origin/main has that a development build hasn't (`dev_check`).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Behind {
    /// The commit the build was built from (`built_commit`).
    pub built: String,
    /// How many commits origin/main has past it.
    pub commits: u32,
    /// The checkout of Codealong's source it was looked at in, where Improve Codealong starts its
    /// sessions (the board's `improve`), which is where the session that updates it starts.
    pub source: String,
    pub checked_at: i64,
}

/// What the windows get: this copy's version, and where updates stand.
#[derive(Debug, Clone, Serialize)]
pub struct View {
    pub current: String,
    #[serde(flatten)]
    pub status: Status,
    /// In a development build, what origin/main has that it hasn't, once there's any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub behind: Option<Behind>,
}

#[derive(Default)]
pub struct Updates {
    status: Mutex<Option<Status>>,
    behind: Mutex<Option<Behind>>,
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
        if let Some(built) = built_commit() {
            let app = app.clone();
            std::thread::spawn(move || {
                std::thread::sleep(FIRST_CHECK);
                loop {
                    if crate::config::updates_check() {
                        dev_check(&app, built);
                    }
                    std::thread::sleep(DEV_EVERY);
                }
            });
        }
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
    let updates = app.state::<Updates>();
    let status = updates.status.lock().unwrap().clone().unwrap_or(Status::Off);
    let behind = updates.behind.lock().unwrap().clone();
    View { current: app.package_info().version.to_string(), status, behind }
}

/// The commit a development build was built from: relaunch.sh sets `CODEALONG_COMMIT` when the
/// checkout it builds has no uncommitted changes. None for any other build, which has no commit
/// to compare.
fn built_commit() -> Option<&'static str> {
    option_env!("CODEALONG_COMMIT").map(str::trim).filter(|c| !c.is_empty())
}

/// Looks at origin/main in Codealong's source on this machine (the board's `improve` worktree,
/// whose id is its folder) for commits this build hasn't, and tells the windows. Fetching only
/// moves the remote's branches, as relaunch.sh's own fetch does. Nothing's said while the board
/// has no source here, or git can't tell (the commit isn't in that checkout: a fork's build).
fn dev_check(app: &AppHandle, built: &str) {
    let source = app.try_state::<crate::engine::Handle>().and_then(|e| e.read(|b| b.improve.lane_id.clone())).filter(|id| !id.starts_with('@'));
    let behind = source.and_then(|source| {
        let commits = commits_behind(Path::new(&source), built)?;
        (commits > 0).then(|| Behind { built: built.to_string(), commits, source, checked_at: now_ms() })
    });
    *app.state::<Updates>().behind.lock().unwrap() = behind;
    let _ = app.emit("update-status", view(app));
}

/// How many commits origin/main has past `built`, in the checkout at `dir`, once origin/main is
/// fetched; a fetch that fails (offline) leaves the last one fetched to count against.
fn commits_behind(dir: &Path, built: &str) -> Option<u32> {
    let git = |args: &[&str], timeout: Duration| {
        let mut full = vec!["-C", dir.to_str()?];
        full.extend_from_slice(args);
        crate::procs::run_background("git", &full, None, timeout).ok()
    };
    let _ = git(&["fetch", "--quiet", "origin", "main"], FETCH_TIMEOUT);
    git(&["rev-list", "--count", &format!("{built}..origin/main")], Duration::from_secs(15))?.trim().parse().ok()
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
    // A manifest of your own, to try an update before it's released.
    let own = match std::env::var("CODEALONG_UPDATE_URL") {
        Ok(url) => Some(url.parse().map_err(|e| format!("CODEALONG_UPDATE_URL isn't a URL: {e}"))?),
        Err(_) => None,
    };
    let Some(update) = newest(app, own).await.map_err(|e| format!("Couldn't check for updates: {e}"))? else {
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

/// A newer release, as tauri.conf.json's endpoints (or `only`) offer it. The updater asks them
/// in order and moves past one that fails, but gives up when one answers with something other
/// than JSON, so a codealong.com that's up but wrong (a parked domain) would hold back every
/// update: then the last endpoint, GitHub's own manifest, is asked alone.
async fn newest<R: Runtime>(app: &AppHandle<R>, only: Option<tauri::Url>) -> Result<Option<Update>, tauri_plugin_updater::Error> {
    let ask = |endpoints: Option<Vec<tauri::Url>>| async move {
        let mut builder = app.updater_builder().timeout(TIMEOUT);
        if let Some(endpoints) = endpoints {
            builder = builder.endpoints(endpoints)?;
        }
        builder.build()?.check().await
    };
    let retry = only.is_none();
    match ask(only.map(|url| vec![url])).await {
        Err(tauri_plugin_updater::Error::Reqwest(e)) if retry && (e.is_decode() || e.is_body()) => match last_endpoint(app) {
            Some(github) => ask(Some(vec![github])).await,
            None => Err(tauri_plugin_updater::Error::Reqwest(e)),
        },
        found => found,
    }
}

/// The endpoint tauri.conf.json lists last: GitHub's own manifest.
fn last_endpoint<R: Runtime>(app: &AppHandle<R>) -> Option<tauri::Url> {
    app.config().plugins.0.get("updater")?["endpoints"].as_array()?.last()?.as_str()?.parse().ok()
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

    #[test]
    fn a_development_build_counts_what_origin_main_has_past_its_commit() {
        let dir = std::env::temp_dir().join(format!("ca-dev-behind-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let (origin, writer, source) = (dir.join("origin.git"), dir.join("writer"), dir.join("source"));
        std::fs::create_dir_all(&dir).unwrap();
        let git = |at: &Path, args: &[&str]| {
            let mut full = vec!["-C", at.to_str().unwrap(), "-c", "user.name=Test", "-c", "user.email=test@example.com"];
            full.extend_from_slice(args);
            crate::procs::run("git", &full, None, Duration::from_secs(20)).unwrap()
        };
        git(&dir, &["init", "-q", "--bare", "-b", "main", origin.to_str().unwrap()]);
        git(&dir, &["clone", "-q", origin.to_str().unwrap(), writer.to_str().unwrap()]);
        git(&writer, &["commit", "-q", "--allow-empty", "-m", "first"]);
        git(&writer, &["push", "-q", "origin", "HEAD:main"]);
        let built = git(&writer, &["rev-parse", "HEAD"]).trim().to_string();
        git(&dir, &["clone", "-q", origin.to_str().unwrap(), source.to_str().unwrap()]);
        assert_eq!(commits_behind(&source, &built), Some(0));
        // Two more land; the source's checkout hasn't fetched them, and the check does.
        git(&writer, &["commit", "-q", "--allow-empty", "-m", "second"]);
        git(&writer, &["commit", "-q", "--allow-empty", "-m", "third"]);
        git(&writer, &["push", "-q", "origin", "HEAD:main"]);
        assert_eq!(commits_behind(&source, &built), Some(2));
        // A commit the checkout doesn't have (a fork's build) says nothing.
        assert_eq!(commits_behind(&source, "0123456789abcdef0123456789abcdef01234567"), None);
        let _ = std::fs::remove_dir_all(&dir);
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

    /// Installed copies ask codealong.com first, which counts the check and redirects to the
    /// manifest on GitHub (the website's worker/index.ts), and GitHub itself when the site fails
    /// or answers with something other than a manifest.
    #[test]
    fn copies_ask_the_site_first_and_github_when_it_fails() {
        use std::io::{Read, Write};
        let conf: serde_json::Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let endpoints: Vec<&str> = conf["plugins"]["updater"]["endpoints"].as_array().unwrap().iter().map(|e| e.as_str().unwrap()).collect();
        let (site, github) = ("https://www.codealong.com", "https://github.com/codealong/codealong/releases/latest/download");
        assert_eq!(endpoints, [format!("{site}/api/update/{{{{target}}}}/{{{{arch}}}}/{{{{current_version}}}}"), format!("{github}/latest.json")]);
        // The updater reads this binary's path even when given another, and Tauri refuses a path
        // through a link, as a worktree inside another checkout builds to (scripts/target-dir.sh).
        if let Err(e) = tauri::utils::platform::current_exe() {
            eprintln!("Not checking the updater itself here: {e}");
            return;
        }

        // The two on loopback: /up/ redirects as the site does, /down/ fails, /parked/ answers
        // with a page, and /github/ with the manifest.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let entry = serde_json::json!({ "url": format!("{base}/github/Codealong.app.tar.gz"), "signature": "unused" });
        let platforms: serde_json::Map<_, _> = ["darwin-aarch64", "darwin-x86_64", "linux-x86_64", "linux-aarch64"].iter().map(|p| (p.to_string(), entry.clone())).collect();
        let manifest = serde_json::to_vec(&serde_json::json!({ "version": "0.0.2", "platforms": platforms })).unwrap();
        let asked = std::sync::Arc::new(Mutex::new(Vec::<String>::new()));
        let (log, to) = (asked.clone(), format!("{base}/github/latest.json"));
        std::thread::spawn(move || {
            for mut stream in listener.incoming().flatten() {
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
                let path = line.split(' ').nth(1).unwrap_or("/").to_string();
                log.lock().unwrap().push(path.clone());
                let _ = if path.starts_with("/up/") {
                    write!(stream, "HTTP/1.1 302 Found\r\nLocation: {to}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                } else if path == "/github/latest.json" {
                    write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", manifest.len()).and_then(|_| stream.write_all(&manifest))
                } else if path.starts_with("/parked/") {
                    write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: 13\r\nConnection: close\r\n\r\n<p>parked</p>")
                } else {
                    write!(stream, "HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                };
            }
        });

        let found = |site_now: &str| {
            let endpoints: Vec<String> = endpoints.iter().map(|e| e.replace(site, &format!("{base}/{site_now}")).replace(github, &format!("{base}/github"))).collect();
            let mut context = tauri::test::mock_context(tauri::test::noop_assets());
            context.config_mut().plugins.0.insert(
                "updater".into(),
                serde_json::json!({ "pubkey": conf["plugins"]["updater"]["pubkey"], "endpoints": endpoints, "requireSignedVersion": true }),
            );
            context.package_info_mut().version = "0.0.1".parse().unwrap();
            let app = tauri::test::mock_builder().plugin(tauri_plugin_updater::Builder::new().build()).build(context).unwrap();
            tauri::async_runtime::block_on(newest(app.handle(), None)).unwrap().map(|u| u.version)
        };
        for site_now in ["up", "down", "parked"] {
            asked.lock().unwrap().clear();
            assert_eq!(found(site_now), Some("0.0.2".into()), "with the site {site_now}");
            let first = asked.lock().unwrap().first().cloned().unwrap();
            assert!(first.starts_with(&format!("/{site_now}/api/update/")) && first.ends_with("/0.0.1"), "the site was asked {first}");
            assert_eq!(asked.lock().unwrap()[1..], ["/github/latest.json"], "with the site {site_now}");
        }
    }

    #[test]
    fn a_status_says_its_state_to_the_windows() {
        let ready = View { current: "0.2.0".into(), status: Status::Ready { version: "0.3.0".into(), notes: Some("Faster".into()) }, behind: None };
        assert_eq!(serde_json::to_value(ready).unwrap(), serde_json::json!({"current": "0.2.0", "state": "ready", "version": "0.3.0", "notes": "Faster"}));
        let checked = View { current: "0.2.0".into(), status: Status::UpToDate { checked_at: 5 }, behind: None };
        assert_eq!(serde_json::to_value(checked).unwrap(), serde_json::json!({"current": "0.2.0", "state": "upToDate", "checkedAt": 5}));
        let off = View { current: "0.2.0".into(), status: Status::Off, behind: None };
        assert_eq!(serde_json::to_value(off).unwrap(), serde_json::json!({"current": "0.2.0", "state": "off"}));
        // A development build origin/main has moved past.
        let behind = Behind { built: "1a2b3c4d".into(), commits: 3, source: "/Users/me/projects/codealong".into(), checked_at: 7 };
        let off = View { current: "0.2.0".into(), status: Status::Off, behind: Some(behind) };
        assert_eq!(serde_json::to_value(off).unwrap(), serde_json::json!({"current": "0.2.0", "state": "off", "behind": {"built": "1a2b3c4d", "commits": 3, "source": "/Users/me/projects/codealong", "checkedAt": 7}}));
    }
}
