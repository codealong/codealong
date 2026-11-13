//! Where things are kept: Codealong's own folders, which macOS has in the Library, Linux in the
//! XDG base folders (`$XDG_CONFIG_HOME` and the rest, else their defaults in your home) and
//! Windows in your home's AppData, and where another app keeps its data on each.

use std::path::{Path, PathBuf};

use super::Os;
use crate::util::home;

/// Codealong's own files: config.toml, the todos, the socket and what the app keeps. On Linux the
/// XDG config folder's, where config.toml is looked for; on Windows the roaming AppData's, which
/// follows you to another PC on a domain.
pub fn support_dir() -> PathBuf {
    match super::current() {
        Os::MacOs => home().join("Library/Application Support/Codealong"),
        Os::Linux => xdg_in(&home(), "XDG_CONFIG_HOME", ".config").join("codealong"),
        Os::Windows => home().join("AppData").join("Roaming").join("Codealong"),
    }
}

/// Where Codealong's logs go: macOS's place for them, on Linux the XDG state folder's, and on
/// Windows the local AppData's, which stays on this PC.
pub fn logs_dir() -> PathBuf {
    match super::current() {
        Os::MacOs => home().join("Library/Logs/Codealong"),
        Os::Linux => xdg_in(&home(), "XDG_STATE_HOME", ".local/state").join("codealong"),
        Os::Windows => home().join("AppData").join("Local").join("Codealong").join("Logs"),
    }
}

/// Caches, which the system may clear; a copy of the app with a home of its own has its own.
pub fn cache_dir() -> PathBuf {
    match super::current() {
        Os::MacOs => home().join("Library/Caches/Codealong"),
        Os::Linux => xdg_in(&home(), "XDG_CACHE_HOME", ".cache").join("codealong"),
        Os::Windows => home().join("AppData").join("Local").join("Codealong").join("Cache"),
    }
}

/// Where the desktop looks for what to start at login on Linux (XDG autostart): the XDG config
/// folder's `autostart`. macOS has login items instead, and Windows its Run key (os/login).
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn autostart_dir() -> PathBuf {
    xdg_in(&home(), "XDG_CONFIG_HOME", ".config").join("autostart")
}

/// Where an app keeps its data in the home `in_home` (yours, or the one an account's folder is
/// in): its folder in macOS's Application Support, named `macos`, on Linux in the config
/// folder, named `linux`, and on Windows in AppData, `windows` being where in it (Chrome's are
/// `Google/Chrome`, `google-chrome` and `Local/Google/Chrome/User Data`).
pub fn app_data(in_home: &Path, macos: &str, linux: &str, windows: &str) -> PathBuf {
    match super::current() {
        Os::MacOs => in_home.join("Library/Application Support").join(macos),
        Os::Linux => xdg_in(in_home, "XDG_CONFIG_HOME", ".config").join(linux),
        Os::Windows => windows.split('/').fold(in_home.join("AppData"), |p, part| p.join(part)),
    }
}

/// An XDG base folder: the one `var` names, else `default` in the home.
fn xdg_in(in_home: &Path, var: &str, default: &str) -> PathBuf {
    in_home_or(in_home, std::env::var_os(var).map(PathBuf::from), default)
}

/// The folder `set` names when it's in the home, else `default` there. So another home's folders
/// (an account's) aren't moved by your settings, and a copy of Codealong given a home of its own
/// (a test's, or one run to debug) keeps everything in it, not in the folders of the session it
/// was started from: GitHub's runners set `XDG_CONFIG_HOME`, and tests shared one config.toml.
fn in_home_or(in_home: &Path, set: Option<PathBuf>, default: &str) -> PathBuf {
    set.filter(|p| p.is_absolute() && p.starts_with(in_home)).unwrap_or_else(|| in_home.join(default))
}

/// The per-user temporary folder macOS gives apps (`getconf DARWIN_USER_TEMP_DIR`): a shell's
/// TMPDIR can be one an agent's sandbox set. Linux and Windows have none of their own.
#[cfg(target_os = "macos")]
pub fn user_temp_dir() -> Option<String> {
    let mut buf = vec![0u8; 1024];
    // SAFETY: confstr writes at most `buf.len()` bytes, its NUL included, and returns the length
    // the whole value needs.
    let n = unsafe { libc::confstr(libc::_CS_DARWIN_USER_TEMP_DIR, buf.as_mut_ptr().cast(), buf.len()) };
    (n > 1 && n <= buf.len()).then(|| String::from_utf8_lossy(&buf[..n - 1]).into_owned())
}

#[cfg(not(target_os = "macos"))]
pub fn user_temp_dir() -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::os::as_if;

    #[test]
    fn each_os_keeps_codealongs_and_other_apps_files_in_its_own_places() {
        let h = home();
        let mac = as_if(Os::MacOs, || (support_dir(), logs_dir(), cache_dir(), app_data(Path::new("/Users/me"), "Google/Chrome", "google-chrome", "Local/Google/Chrome/User Data")));
        assert_eq!(mac.0, h.join("Library/Application Support/Codealong"));
        assert_eq!(mac.1, h.join("Library/Logs/Codealong"));
        assert_eq!(mac.2, h.join("Library/Caches/Codealong"));
        assert_eq!(mac.3, PathBuf::from("/Users/me/Library/Application Support/Google/Chrome"));
        assert_eq!(as_if(Os::Linux, || app_data(Path::new("/home/other"), "Cursor", "Cursor", "Roaming/Cursor")), PathBuf::from("/home/other/.config/Cursor"));
        let win = as_if(Os::Windows, || (support_dir(), logs_dir(), app_data(Path::new("/Users/me"), "Google/Chrome", "google-chrome", "Local/Google/Chrome/User Data")));
        assert_eq!(win.0, h.join("AppData").join("Roaming").join("Codealong"));
        assert_eq!(win.1, h.join("AppData").join("Local").join("Codealong").join("Logs"));
        assert_eq!(win.2, Path::new("/Users/me").join("AppData").join("Local").join("Google").join("Chrome").join("User Data"));
        // An XDG folder counts only in the home it's for: another home's, or a copy's own home,
        // aren't moved by the settings of the session it was started from.
        let me = Path::new("/home/me");
        assert_eq!(in_home_or(me, Some("/home/me/.cfg".into()), ".config"), PathBuf::from("/home/me/.cfg"));
        assert_eq!(in_home_or(Path::new("/tmp/home"), Some("/home/me/.config".into()), ".config"), PathBuf::from("/tmp/home/.config"));
        assert_eq!(in_home_or(me, Some(".cfg".into()), ".config"), PathBuf::from("/home/me/.config"));
        assert_eq!(in_home_or(me, None, ".local/state"), PathBuf::from("/home/me/.local/state"));
        if std::env::var_os("XDG_CONFIG_HOME").is_none_or(|p| !Path::new(&p).starts_with(&h)) {
            assert_eq!(as_if(Os::Linux, support_dir), h.join(".config/codealong"));
        }
    }
}
