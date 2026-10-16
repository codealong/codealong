//! Codealong's own secrets (Jev's API key, this device's key), by service name: in the login
//! keychain on macOS, through `security`, and on Linux in the desktop's Secret Service (GNOME
//! Keyring, KWallet), through libsecret's `secret-tool`, which a desktop without one makes fail.
//! A secret never goes on a command line: `security -i` and `secret-tool store` read it from
//! stdin.

use std::time::Duration;

use anyhow::Result;

/// A locked keychain can hold `security` at a prompt; don't wait on it forever.
const TIMEOUT: Duration = Duration::from_secs(15);

/// The account the items are filed under: your user name.
pub fn account() -> String {
    std::env::var("USER").ok().filter(|u| !u.is_empty() && u.chars().all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))).unwrap_or_else(|| "codealong".into())
}

/// The secret kept for `service`, if there's one.
#[cfg(target_os = "macos")]
pub fn read(service: &str) -> Option<String> {
    let out = crate::procs::run("/usr/bin/security", &["find-generic-password", "-a", &account(), "-s", service, "-w"], None, TIMEOUT).ok()?;
    Some(out.trim().to_string()).filter(|s| !s.is_empty())
}

/// Whether there's one, reading no secret.
#[cfg(target_os = "macos")]
pub fn has(service: &str) -> bool {
    crate::procs::run("/usr/bin/security", &["find-generic-password", "-a", &account(), "-s", service], None, TIMEOUT).is_ok()
}

/// Keeps `secret` for `service`, in place of what was there. The caller checks it can be written
/// in `security`'s quotes.
#[cfg(target_os = "macos")]
pub fn save(service: &str, secret: &str) -> Result<()> {
    let command = format!("add-generic-password -U -a \"{}\" -s \"{service}\" -l \"{service}\" -w \"{secret}\"\n", account());
    let out = crate::procs::run_stdin("/usr/bin/security", &["-i"], &command, TIMEOUT)?;
    // `security -i` exits cleanly whatever its command did; errors come back as text.
    if out.to_lowercase().contains("error") {
        anyhow::bail!("The keychain didn't take it.");
    }
    Ok(())
}

/// Takes away what's kept for `service`; nothing there is no error.
#[cfg(target_os = "macos")]
pub fn delete(service: &str) {
    let _ = crate::procs::run("/usr/bin/security", &["delete-generic-password", "-a", &account(), "-s", service], None, TIMEOUT);
}

#[cfg(not(target_os = "macos"))]
pub fn read(service: &str) -> Option<String> {
    let out = crate::procs::run("secret-tool", &["lookup", "service", service, "account", &account()], None, TIMEOUT).ok()?;
    Some(out.trim().to_string()).filter(|s| !s.is_empty())
}

/// The Secret Service gives no answer short of the secret.
#[cfg(not(target_os = "macos"))]
pub fn has(service: &str) -> bool {
    read(service).is_some()
}

#[cfg(not(target_os = "macos"))]
pub fn save(service: &str, secret: &str) -> Result<()> {
    crate::procs::run_stdin("secret-tool", &["store", &format!("--label={service}"), "service", service, "account", &account()], secret, TIMEOUT).map(|_| ())
}

#[cfg(not(target_os = "macos"))]
pub fn delete(service: &str) {
    let _ = crate::procs::run("secret-tool", &["clear", "service", service, "account", &account()], None, TIMEOUT);
}
