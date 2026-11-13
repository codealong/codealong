//! Open at login on Windows: a value under `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`
//! naming this binary, which Windows starts when you log in. Task Manager's Startup apps turns one
//! off without taking it away, by a value of the same name under Explorer's `StartupApproved\Run`
//! whose first byte is odd; one turned off there is off here too.

use std::os::windows::ffi::OsStrExt;

use anyhow::Context;
use windows_sys::Win32::Foundation::ERROR_SUCCESS;
use windows_sys::Win32::System::Registry::{RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_BINARY, RRF_RT_REG_SZ};

use super::AtLogin;

const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const APPROVED: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";

fn wide(s: &str) -> Vec<u16> {
    std::ffi::OsStr::new(s).encode_wide().chain([0]).collect()
}

/// The value is named for the app's product name, so Codealong Dev has its own.
fn name(app: &tauri::AppHandle) -> String {
    app.config().product_name.clone().unwrap_or_else(|| "Codealong".into())
}

/// A value's bytes under HKCU, of the kinds `flags` allows.
fn read(key: &str, name: &str, flags: u32) -> Option<Vec<u8>> {
    let (key, name) = (wide(key), wide(name));
    let mut len = 0u32;
    // SAFETY: the first call asks only for the size, the second fills a buffer of that size.
    unsafe {
        if RegGetValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr(), flags, std::ptr::null_mut(), std::ptr::null_mut(), &mut len) != ERROR_SUCCESS {
            return None;
        }
        let mut buf = vec![0u8; len as usize];
        (RegGetValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr(), flags, std::ptr::null_mut(), buf.as_mut_ptr().cast(), &mut len) == ERROR_SUCCESS).then(|| {
            buf.truncate(len as usize);
            buf
        })
    }
}

pub fn status(app: &tauri::AppHandle) -> AtLogin {
    let name = name(app);
    if read(RUN, &name, RRF_RT_REG_SZ).is_none() {
        return AtLogin::Off;
    }
    match read(APPROVED, &name, RRF_RT_REG_BINARY) {
        Some(b) if b.first().is_some_and(|f| f % 2 == 1) => AtLogin::Off,
        _ => AtLogin::On,
    }
}

/// Writes the value for the binary running now, or takes it away; then how it stands. Turning it
/// on takes away Task Manager's turning it off, as turning it on there would.
pub fn set(app: &tauri::AppHandle, on: bool) -> anyhow::Result<AtLogin> {
    let name = wide(&name(app));
    if on {
        let exe = std::env::current_exe().context("Couldn't tell where Codealong's binary is")?;
        let line = wide(&format!("\"{}\"", exe.display()));
        // SAFETY: the strings are NUL-ended wide strings that outlive the call; the data's size
        // counts its NUL, as REG_SZ's must.
        let set = unsafe { RegSetKeyValueW(HKEY_CURRENT_USER, wide(RUN).as_ptr(), name.as_ptr(), REG_SZ, line.as_ptr().cast(), (line.len() * 2) as u32) };
        anyhow::ensure!(set == ERROR_SUCCESS, "Couldn't write Codealong into the registry's Run key (error {set})");
        // SAFETY: as above; a value that isn't there is no matter.
        unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, wide(APPROVED).as_ptr(), name.as_ptr()) };
    } else {
        // SAFETY: as above.
        let gone = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, wide(RUN).as_ptr(), name.as_ptr()) };
        anyhow::ensure!(gone == ERROR_SUCCESS || read(RUN, &String::from_utf16_lossy(&name[..name.len() - 1]), RRF_RT_REG_SZ).is_none(), "Couldn't take Codealong out of the registry's Run key (error {gone})");
    }
    Ok(status(app))
}
