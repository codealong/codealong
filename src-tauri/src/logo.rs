//! The running app's Dock icon follows the engine even when every board window is hidden.

use crate::model::LogoStatus;

#[cfg(target_os = "macos")]
pub fn update(app: &tauri::AppHandle, status: LogoStatus) -> tauri::Result<()> {
    use objc2::{AnyThread, MainThreadMarker};
    use objc2_app_kit::{NSApplication, NSImage};
    use objc2_foundation::NSData;

    let png: &'static [u8] = match status {
        LogoStatus::Error => include_bytes!("../icons/status/error.png"),
        LogoStatus::Needs => include_bytes!("../icons/status/needs.png"),
        LogoStatus::Unread => include_bytes!("../icons/status/unread.png"),
        LogoStatus::Working => include_bytes!("../icons/status/working.png"),
        LogoStatus::Idle => include_bytes!("../app-icon.png"),
    };
    app.run_on_main_thread(move || {
        let Some(mtm) = MainThreadMarker::new() else { return };
        let data = NSData::with_bytes(png);
        if let Some(image) = NSImage::initWithData(NSImage::alloc(), &data) {
            // AppKit requires the main thread and a non-null image. The bundled idle
            // icon stays neutral; only the running application's icon changes.
            unsafe { NSApplication::sharedApplication(mtm).setApplicationIconImage(Some(&image)); }
        } else {
            eprintln!("Couldn't decode the Codealong status icon: {status:?}");
        }
    })
}

#[cfg(not(target_os = "macos"))]
pub fn update(_app: &tauri::AppHandle, _status: LogoStatus) -> tauri::Result<()> {
    Ok(())
}
