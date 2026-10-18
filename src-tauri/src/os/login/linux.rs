//! Open at login on Linux: an XDG autostart entry for this binary, which GNOME, KDE and the other
//! desktops start when you log in.

use anyhow::Context;

use super::{autostart_entry, autostart_file, autostart_on, AtLogin};

/// The entry is named for the app's product name, so Codealong Dev has its own.
fn name(app: &tauri::AppHandle) -> String {
    app.config().product_name.clone().unwrap_or_else(|| "Codealong".into())
}

pub fn status(app: &tauri::AppHandle) -> AtLogin {
    match std::fs::read_to_string(autostart_file(&name(app))) {
        Ok(entry) if autostart_on(&entry) => AtLogin::On,
        _ => AtLogin::Off,
    }
}

/// Writes the entry for the binary running now, or takes it away; then how it stands.
pub fn set(app: &tauri::AppHandle, on: bool) -> anyhow::Result<AtLogin> {
    let name = name(app);
    let file = autostart_file(&name);
    if on {
        let exe = std::env::current_exe().context("Couldn't tell where Codealong's binary is")?;
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir).with_context(|| format!("Couldn't make {}", dir.display()))?;
        }
        std::fs::write(&file, autostart_entry(&name, &exe)).with_context(|| format!("Couldn't write {}", file.display()))?;
    } else if file.exists() {
        std::fs::remove_file(&file).with_context(|| format!("Couldn't remove {}", file.display()))?;
    }
    Ok(status(app))
}
