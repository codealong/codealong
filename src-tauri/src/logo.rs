//! The running app's Dock icon follows the engine even when every board window is hidden.

use crate::model::LogoStatus;

pub fn update(app: &tauri::AppHandle, status: LogoStatus) -> tauri::Result<()> {
    // Codealong Dev (tauri.dev.conf.json, what relaunch.sh builds) keeps its light tile in every
    // state, so it never passes for the app in Applications.
    let png: &'static [u8] = match (crate::is_dev_build(app), status) {
        (false, LogoStatus::Error) => include_bytes!("../icons/status/error.png"),
        (false, LogoStatus::Needs) => include_bytes!("../icons/status/needs.png"),
        (false, LogoStatus::Unread) => include_bytes!("../icons/status/unread.png"),
        (false, LogoStatus::Working) => include_bytes!("../icons/status/working.png"),
        (false, LogoStatus::Idle) => include_bytes!("../app-icon.png"),
        (true, LogoStatus::Error) => include_bytes!("../icons/dev/status/error.png"),
        (true, LogoStatus::Needs) => include_bytes!("../icons/dev/status/needs.png"),
        (true, LogoStatus::Unread) => include_bytes!("../icons/dev/status/unread.png"),
        (true, LogoStatus::Working) => include_bytes!("../icons/dev/status/working.png"),
        (true, LogoStatus::Idle) => include_bytes!("../app-icon-dev.png"),
    };
    crate::os::apps::set_dock_icon(app, png)
}
