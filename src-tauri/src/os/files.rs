//! What the OS keeps on a file beyond its bytes and mode.

use std::path::Path;

/// Whether a file is a Finder alias, which `open` follows to what it points at as it does a link,
/// though it's a plain file with any name: the alias flag (`kIsAlias`, 0x8000) in the Finder
/// flags of its `com.apple.FinderInfo`, bytes 8 and 9 (one to an app is type `fapa`, creator
/// `MACS`, as Foundation's bookmark files are written on macOS 26).
#[cfg(target_os = "macos")]
pub fn finder_alias(path: &Path) -> bool {
    use std::os::unix::ffi::OsStrExt;
    let Ok(c) = std::ffi::CString::new(path.as_os_str().as_bytes()) else { return false };
    let mut info = [0u8; 32];
    let n = unsafe { libc::getxattr(c.as_ptr(), c"com.apple.FinderInfo".as_ptr(), info.as_mut_ptr().cast(), info.len(), 0, 0) };
    n >= 10 && u16::from_be_bytes([info[8], info[9]]) & 0x8000 != 0
}

/// Linux has no Finder aliases.
#[cfg(target_os = "linux")]
pub fn finder_alias(_path: &Path) -> bool {
    false
}
