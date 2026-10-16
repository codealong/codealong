//! Playing a sound on Linux: its file, through `paplay`.

/// Plays the sound's file through PulseAudio or PipeWire, which both answer to `paplay`, at
/// `volume` percent, and waits for it to end. Off the main thread.
pub fn play(name: &str, volume: u32) -> anyhow::Result<()> {
    let Some(file) = super::file_in(&super::folders(), name.trim()) else {
        anyhow::bail!("There's no sound named {:?} in ~/.local/share/sounds or the desktop's sounds.", name.trim());
    };
    let volume = format!("--volume={}", volume.min(100) * 65536 / 100);
    crate::procs::run("paplay", &[&volume, &file.to_string_lossy()], None, std::time::Duration::from_secs(10)).map(|_| ())
}
