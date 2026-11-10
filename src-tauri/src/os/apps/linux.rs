//! Apps on Linux, which knows them by no bundle ID: none is said to be active or asked to come
//! forward (the window manager raises what a click opens), Codealong has no hiding or Dock icon
//! of its own, and an app has no one icon.

use std::path::Path;

pub fn is_active(_bundle_id: &str) -> bool {
    false
}

pub fn activate(_bundle_id: &str) {}

pub fn frontmost_other() -> Option<String> {
    None
}

/// No app is hidden whole; the caller hides its windows.
pub fn hide(_app: &tauri::AppHandle) -> bool {
    false
}

/// Never asked: no binary is in an app bundle off macOS (install.rs).
pub fn opened_by_macos(_pid: u32) -> bool {
    false
}

/// A taskbar shows the windows' own icon, which stays as it is.
pub fn set_dock_icon(_app: &tauri::AppHandle, _png: &'static [u8]) -> tauri::Result<()> {
    Ok(())
}

/// No link comes this way on Linux: a desktop opens one by starting the program a launcher
/// names for its scheme with the link as an argument, and no launcher names Codealong for one yet.
pub fn opened_links(_event: &tauri::RunEvent) -> Vec<String> {
    Vec::new()
}

/// Nothing to claim where no link is opened with Codealong (`opened_links`).
pub fn claim_links(_bundle: &Path, _scheme: &str) {}

/// The editor menus show editors by name alone.
pub fn icon(_app: &Path, _px: usize) -> Option<Vec<u8>> {
    None
}
