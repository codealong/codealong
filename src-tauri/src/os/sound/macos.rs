//! Playing a sound on macOS: NSSound, by its name.

/// Plays a sound by its name, at `volume` percent of the Mac's own volume; a sound still playing
/// starts over. Off the main thread: the first sound after launch waits on macOS's audio to get
/// going, which took 7 seconds on a busy Mac (load 170, 2026-10-12) and a tenth of one after.
/// The sound plays on after the thread that started it is gone (checked the same day).
pub fn play(name: &str, volume: u32) -> anyhow::Result<()> {
    use objc2_app_kit::NSSound;
    use objc2_foundation::NSString;
    // A sound by name is one object for the whole app, so two threads don't start it at once.
    static PLAYING: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _one = PLAYING.lock().unwrap_or_else(|e| e.into_inner());
    let Some(sound) = NSSound::soundNamed(&NSString::from_str(name.trim())) else {
        anyhow::bail!("There's no sound named {:?} in ~/Library/Sounds or macOS's sounds.", name.trim());
    };
    sound.stop();
    sound.setVolume(volume.min(100) as f32 / 100.0);
    if !sound.play() {
        anyhow::bail!("macOS didn't play the sound {:?}.", name.trim());
    }
    Ok(())
}
