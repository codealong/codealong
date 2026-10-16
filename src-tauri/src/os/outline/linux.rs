//! Linux has no way to find a terminal's windows by its app, so nothing is outlined: with no
//! pointer to start from, `flash::after_focus` returns at once.

use crate::flash::Frame;

pub fn ticket() -> u64 {
    0
}

pub fn is_newest(_ticket: u64) -> bool {
    false
}

pub fn pointer() -> Option<(f64, f64)> {
    None
}

pub fn screens() -> Vec<Frame> {
    Vec::new()
}

pub fn front_window_settled(_bundle_id: &str) -> Option<Frame> {
    None
}

pub fn warp(_at: (f64, f64)) {}

pub fn outline(_app: &tauri::AppHandle, _window: Frame, _ticket: u64) {}
