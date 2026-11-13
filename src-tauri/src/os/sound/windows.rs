//! Playing a sound on Windows: its file, through `PlaySound`.

use std::os::windows::ffi::OsStrExt;

use windows_sys::Win32::Media::Audio::{waveOutSetVolume, PlaySoundW, SND_FILENAME, SND_NODEFAULT, SND_SYNC};

/// Plays the sound's file at `volume` percent of the app's own volume in the mixer, and waits for
/// it to end. Off the main thread.
pub fn play(name: &str, volume: u32) -> anyhow::Result<()> {
    let Some(file) = super::file_in(&super::folders(), name.trim()) else {
        anyhow::bail!("There's no sound named {:?} in Windows' sounds (C:\\Windows\\Media).", name.trim());
    };
    let wide: Vec<u16> = file.as_os_str().encode_wide().chain([0]).collect();
    // Both channels, each a 16-bit share of full volume.
    let level = volume.min(100) * 0xFFFF / 100;
    // SAFETY: the null device is this app's wave output; the path is a NUL-ended wide string that
    // outlives the call, which plays the file to its end.
    unsafe {
        waveOutSetVolume(std::ptr::null_mut(), level | (level << 16));
        if PlaySoundW(wide.as_ptr(), std::ptr::null_mut(), SND_FILENAME | SND_SYNC | SND_NODEFAULT) == 0 {
            anyhow::bail!("Windows couldn't play {}.", file.display());
        }
    }
    Ok(())
}
