//! What's in front: Codealong's main window brought forward, another app asked to come forward,
//! and whether an app is the active one. Most of Codealong's windows are panels that leave it
//! inactive when clicked (panel.rs), so after one of them opens something, Codealong has to ask
//! macOS for what a click in an ordinary window gets by itself.

use anyhow::{anyhow, Result};
use objc2_app_kit::{NSApplicationActivationOptions, NSRunningApplication};
use objc2_foundation::NSString;
use tauri::{AppHandle, Emitter, Manager};

/// Brings the main window forward. `reveal` (a session or PR) goes to its page, which selects it.
pub fn show_main(app: &AppHandle, reveal: Option<serde_json::Value>) -> Result<()> {
    let main = app.get_webview_window("main").ok_or_else(|| anyhow!("The main window is gone."))?;
    let _ = main.unminimize();
    main.show()?;
    main.set_focus()?;
    if let Some(r) = reveal {
        app.emit_to("main", "reveal", r)?;
    }
    Ok(())
}

/// After a panel opened something, asks macOS to bring that app forward. A click in the bar leaves
/// Codealong inactive, and macOS may refuse an inactive app; the open itself has already happened.
pub fn bring_forward(app: &AppHandle, bundle_id: &'static str) {
    let _ = app.run_on_main_thread(move || activate_app(bundle_id));
}

/// Whether the app with this bundle ID is the active one, the app whose menus are in the menu bar.
/// Asked afresh each time, from any thread: a running app's record is as it was when it was
/// fetched, so each ask fetches it again.
pub fn app_active(bundle_id: &str) -> bool {
    let apps = NSRunningApplication::runningApplicationsWithBundleIdentifier(&NSString::from_str(bundle_id));
    apps.count() > 0 && apps.objectAtIndex(0).isActive()
}

/// Asks the running app with this bundle ID to come forward.
fn activate_app(bundle_id: &str) {
    let apps = NSRunningApplication::runningApplicationsWithBundleIdentifier(&NSString::from_str(bundle_id));
    if apps.count() > 0 {
        #[allow(deprecated)]
        let _ = apps.objectAtIndex(0).activateWithOptions(NSApplicationActivationOptions::empty());
    }
}
