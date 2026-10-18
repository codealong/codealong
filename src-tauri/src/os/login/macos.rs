//! Open at login on macOS: the app as a login item, through ServiceManagement's `SMAppService`
//! (macOS 13 and later). macOS opens the bundle this copy runs from at login, as the Dock would,
//! and lists it under System Settings › General › Login Items, where it can be turned off too.

use objc2_service_management::{SMAppService, SMAppServiceStatus};

use super::AtLogin;

/// The bundle is the one this app runs from, so Codealong and Codealong Dev are items of their own.
pub fn status(_app: &tauri::AppHandle) -> AtLogin {
    // SAFETY: the app's own service, asked for its status; neither takes arguments.
    let status = unsafe { SMAppService::mainAppService().status() };
    match status {
        SMAppServiceStatus::Enabled => AtLogin::On,
        SMAppServiceStatus::RequiresApproval => AtLogin::Approve,
        // Not registered, or nothing macOS can find to register (a binary outside a bundle):
        // turning it on says why that fails.
        _ => AtLogin::Off,
    }
}

/// Registers the app as a login item, or unregisters it; then how it stands. Each fails on one
/// that's already so, so it's asked only of one that isn't.
pub fn set(app: &tauri::AppHandle, on: bool) -> anyhow::Result<AtLogin> {
    let now = status(app);
    // SAFETY: the app's own service; registering and unregistering take no arguments.
    let done = unsafe {
        let service = SMAppService::mainAppService();
        match (on, now) {
            (true, AtLogin::Off) => service.registerAndReturnError(),
            (false, AtLogin::On | AtLogin::Approve) => service.unregisterAndReturnError(),
            _ => Ok(()),
        }
    };
    if let Err(e) = done {
        let what = if on { "add Codealong to" } else { "take Codealong out of" };
        anyhow::bail!("macOS didn't {what} your login items: {}", e.localizedDescription());
    }
    Ok(status(app))
}
