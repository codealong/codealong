//! The sounds the OS has for what comes to wait on you (sounds.rs): where it keeps them and yours,
//! which files they are, the two a new config.toml names, and playing one. On macOS the ones
//! NSSound finds by name; on Linux the freedesktop sound theme's, which desktops install, played
//! through PulseAudio or PipeWire.

use std::path::{Path, PathBuf};

use super::Os;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::play;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::play;

/// The sounds a new config.toml names for something needing you and a turn ending: on macOS a
/// pop and a click, which every Mac has; on Linux the sound theme's.
pub fn defaults() -> (&'static str, &'static str) {
    match super::current() {
        Os::MacOs => ("Pop", "Tink"),
        Os::Linux => ("message-new-instant", "complete"),
    }
}

/// Whether the defaults can be missing: a Linux without the sound theme has neither.
pub fn default_may_be_missing() -> bool {
    super::current() == Os::Linux
}

/// The folders the sounds are found in, yours first: on macOS those NSSound's `soundNamed:`
/// looks in, in its order (yours, the Mac's, macOS's); on Linux yours, then the theme's.
pub fn folders() -> Vec<PathBuf> {
    let mut out = Vec::new();
    match super::current() {
        Os::MacOs => {
            out.extend(dirs::home_dir().map(|h| h.join("Library/Sounds")));
            out.extend(["/Library/Sounds", "/Network/Library/Sounds", "/System/Library/Sounds"].map(PathBuf::from));
        }
        Os::Linux => {
            out.extend(dirs::data_dir().map(|d| d.join("sounds")));
            out.extend(["/usr/local/share/sounds", "/usr/share/sounds/freedesktop/stereo"].map(PathBuf::from));
        }
    }
    out
}

/// What config.toml says of where the sounds it can name are.
pub fn names_said() -> &'static str {
    match super::current() {
        Os::MacOs => "Each is the name of a sound in /System/Library/Sounds (\"Pop\", \"Tink\", \"Bottle\", \"Purr\"...)\n# or ~/Library/Sounds, or \"\" for none. volume: in percent of the Mac's own.",
        Os::Linux => "Each is the name of a sound in /usr/share/sounds/freedesktop/stereo (\"message-new-instant\",\n# \"complete\", \"bell\"...) or ~/.local/share/sounds, or \"\" for none. volume: in percent of the system's.",
    }
}

/// Whether a file is a sound the OS plays, by its extension.
pub fn is_sound(p: &Path) -> bool {
    let kinds: &[&str] = match super::current() {
        Os::MacOs => &["aiff", "aif", "aifc", "wav", "caf", "mp3", "m4a", "aac", "au", "snd"],
        Os::Linux => &["oga", "ogg", "wav", "flac"],
    };
    p.extension().and_then(|x| x.to_str()).is_some_and(|x| kinds.contains(&x.to_ascii_lowercase().as_str()))
}

/// The file of the sound named `name`, the first folder's that has it.
#[cfg_attr(target_os = "macos", allow(dead_code))]
fn file_in(folders: &[PathBuf], name: &str) -> Option<PathBuf> {
    folders
        .iter()
        .filter_map(|d| std::fs::read_dir(d).ok())
        .flat_map(|entries| entries.filter_map(|e| e.ok()).map(|e| e.path()))
        .find(|p| is_sound(p) && p.file_stem().and_then(|s| s.to_str()).is_some_and(|s| s.eq_ignore_ascii_case(name)))
}
