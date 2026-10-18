//! Opening Codealong when you log in (Settings › General), so your other devices can reach it
//! without your opening it. On macOS the app is a login item, registered with ServiceManagement,
//! which System Settings › General › Login Items lists; on Linux an XDG autostart entry, a desktop
//! file in `~/.config/autostart` that the desktop starts at login. Neither is kept in config.toml:
//! either can be turned off from the system's side too, so whether it's on is asked of the system
//! each time, and nothing turns it back on by itself.

use std::path::{Path, PathBuf};

use serde::Serialize;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::*;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::*;

/// Whether the app opens at login.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum AtLogin {
    Off,
    On,
    /// Registered, but macOS waits for you to allow it in System Settings › General › Login Items.
    Approve,
}

/// The autostart entry for the app named `name`: `codealong.desktop`, or `codealong-dev.desktop`
/// for Codealong Dev, so each copy has its own.
#[cfg_attr(target_os = "macos", allow(dead_code))]
fn autostart_file(name: &str) -> PathBuf {
    super::dirs::autostart_dir().join(format!("{}.desktop", name.to_lowercase().replace(' ', "-")))
}

/// The autostart entry that opens `exe` at login.
#[cfg_attr(target_os = "macos", allow(dead_code))]
fn autostart_entry(name: &str, exe: &Path) -> String {
    format!("[Desktop Entry]\nType=Application\nName={name}\nExec={}\nX-GNOME-Autostart-enabled=true\n", exec_arg(&exe.to_string_lossy()))
}

/// A path as the one argument of a desktop entry's `Exec`, as the Desktop Entry spec has it: in
/// quotes, with `"`, `` ` ``, `$` and `\` escaped inside them and `%` doubled (it starts a field
/// code), then every backslash doubled again, as in any string value, which is read first.
#[cfg_attr(target_os = "macos", allow(dead_code))]
fn exec_arg(path: &str) -> String {
    let mut quoted = String::from("\"");
    for c in path.chars() {
        match c {
            '"' | '`' | '$' | '\\' => {
                quoted.push('\\');
                quoted.push(c);
            }
            '%' => quoted.push_str("%%"),
            _ => quoted.push(c),
        }
    }
    quoted.push('"');
    quoted.replace('\\', "\\\\")
}

/// Whether an autostart entry starts at login: one the desktop turned off (`Hidden=true`, or
/// GNOME's `X-GNOME-Autostart-enabled=false`) doesn't.
#[cfg_attr(target_os = "macos", allow(dead_code))]
fn autostart_on(entry: &str) -> bool {
    !entry.lines().filter_map(|l| l.split_once('=')).any(|(k, v)| matches!((k.trim(), v.trim()), ("Hidden", "true") | ("X-GNOME-Autostart-enabled", "false")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_autostart_entry_opens_this_binary_and_each_copy_has_its_own() {
        assert!(autostart_file("Codealong").ends_with("autostart/codealong.desktop"));
        assert!(autostart_file("Codealong Dev").ends_with("autostart/codealong-dev.desktop"));
        let entry = autostart_entry("Codealong", Path::new("/opt/Code along/codealong"));
        assert_eq!(entry, "[Desktop Entry]\nType=Application\nName=Codealong\nExec=\"/opt/Code along/codealong\"\nX-GNOME-Autostart-enabled=true\n");
        assert!(autostart_on(&entry));
    }

    #[test]
    fn a_path_is_quoted_and_escaped_as_exec_reads_it() {
        assert_eq!(exec_arg("/usr/bin/codealong"), "\"/usr/bin/codealong\"");
        // `$` is escaped once inside the quotes, and that backslash doubled as a string's are.
        assert_eq!(exec_arg("/a/$b"), r#""/a/\\$b""#);
        assert_eq!(exec_arg(r"/a\b"), r#""/a\\\\b""#);
        assert_eq!(exec_arg("/a/\"b\""), r#""/a/\\"b\\"""#);
        assert_eq!(exec_arg("/a/100%"), "\"/a/100%%\"");
    }

    #[test]
    fn an_entry_the_desktop_turned_off_is_off() {
        assert!(!autostart_on("[Desktop Entry]\nExec=codealong\nHidden=true\n"));
        assert!(!autostart_on("[Desktop Entry]\nExec=codealong\nX-GNOME-Autostart-enabled = false\n"));
        assert!(autostart_on("[Desktop Entry]\nExec=codealong\nHidden=false\n"));
    }
}
