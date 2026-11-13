//! What the OS keeps on a file beyond its bytes: its mode, which only POSIX has (on Windows a file
//! in your profile is yours alone already, and runs by its extension), links, which file it is,
//! and whether its volume can be written. And the Finder's aliases.

use std::fs::{DirBuilder, Metadata, OpenOptions};
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

/// Linux and Windows have no Finder aliases.
#[cfg(not(target_os = "macos"))]
pub fn finder_alias(_path: &Path) -> bool {
    false
}

/// Makes a file the options create have `mode` (0o600: only you may read it).
pub fn create_mode(opts: &mut OpenOptions, mode: u32) -> &mut OpenOptions {
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(opts, mode);
    #[cfg(windows)]
    let _ = mode;
    opts
}

/// Makes the folders a builder creates have `mode` (0o700: only you may look in them).
pub fn dir_mode(builder: &mut DirBuilder, mode: u32) -> &mut DirBuilder {
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(builder, mode);
    #[cfg(windows)]
    let _ = mode;
    builder
}

/// Gives the file at `path` the mode `mode`. On Windows only the owner's write bit counts: without
/// it the file is read-only.
pub fn set_mode(path: &Path, mode: u32) -> std::io::Result<()> {
    #[cfg(unix)]
    return std::fs::set_permissions(path, std::os::unix::fs::PermissionsExt::from_mode(mode));
    #[cfg(windows)]
    {
        let mut p = std::fs::metadata(path)?.permissions();
        p.set_readonly(mode & 0o200 == 0);
        std::fs::set_permissions(path, p)
    }
}

/// A file's mode, its permission bits. On Windows, what POSIX would say of it: a folder may be
/// looked in and a file read, by anyone, and written unless it's read-only; none is run by its
/// mode.
pub fn mode_of(meta: &Metadata) -> u32 {
    #[cfg(unix)]
    return std::os::unix::fs::PermissionsExt::mode(&meta.permissions()) & 0o7777;
    #[cfg(windows)]
    {
        let write = if meta.permissions().readonly() { 0 } else { 0o200 };
        if meta.is_dir() { 0o555 | write } else { 0o444 | write }
    }
}

/// Whether a file is a program you can run: by its mode where there are modes, by its extension
/// on Windows.
pub fn runs(path: &Path, meta: &Metadata) -> bool {
    if !meta.is_file() {
        return false;
    }
    #[cfg(unix)]
    return {
        let _ = path;
        mode_of(meta) & 0o111 != 0
    };
    #[cfg(windows)]
    {
        let ext = path.extension().and_then(|e| e.to_str()).map(|e| format!(".{}", e.to_ascii_uppercase()));
        let runnable = std::env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into());
        ext.is_some_and(|e| runnable.split(';').any(|r| r.eq_ignore_ascii_case(&e)))
    }
}

/// A link at `link` to `target`, which a relative target names from the link's folder. On
/// Windows a link to a folder is another kind than a link to a file, picked by what's there;
/// making one takes Developer Mode or an administrator.
pub fn symlink(target: impl AsRef<Path>, link: impl AsRef<Path>) -> std::io::Result<()> {
    let (target, link) = (target.as_ref(), link.as_ref());
    #[cfg(unix)]
    return std::os::unix::fs::symlink(target, link);
    #[cfg(windows)]
    {
        let resolved = link.parent().map_or_else(|| target.to_path_buf(), |dir| dir.join(target));
        if resolved.is_dir() {
            std::os::windows::fs::symlink_dir(target, link)
        } else {
            std::os::windows::fs::symlink_file(target, link)
        }
    }
}

/// A path a program wrote as bytes (git's `gitdir:` line): any bytes on POSIX, whose paths are
/// bytes; UTF-8 on Windows, as git writes them there.
pub fn path_from_bytes(bytes: &[u8]) -> Option<std::path::PathBuf> {
    #[cfg(unix)]
    return Some(std::path::PathBuf::from(<std::ffi::OsStr as std::os::unix::ffi::OsStrExt>::from_bytes(bytes)));
    #[cfg(windows)]
    return std::str::from_utf8(bytes).ok().map(std::path::PathBuf::from);
}

/// Which file an open file is, by its device and inode on POSIX and its volume and file index
/// on Windows: another file at its path (a log started again) has another.
pub fn id(file: &std::fs::File) -> Option<(u64, u64)> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let m = file.metadata().ok()?;
        Some((m.dev(), m.ino()))
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Storage::FileSystem::{GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION};
        // SAFETY: the handle is the open file's; the call fills the struct it's given.
        let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
        (unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } != 0).then(|| (info.dwVolumeSerialNumber as u64, ((info.nFileIndexHigh as u64) << 32) | info.nFileIndexLow as u64))
    }
}

/// Which file is at `path` now (`id`).
pub fn id_at(path: &Path) -> Option<(u64, u64)> {
    id(&std::fs::File::open(path).ok()?)
}

/// Opens a file to read, refusing a link, which could lead anywhere: POSIX refuses it as it
/// opens (`O_NOFOLLOW`); Windows has no such open for a plain file, so a link is looked for
/// first.
pub fn open_no_follow(path: &Path) -> std::io::Result<std::fs::File> {
    #[cfg(unix)]
    return std::os::unix::fs::OpenOptionsExt::custom_flags(OpenOptions::new().read(true), libc::O_NOFOLLOW).open(path);
    #[cfg(windows)]
    {
        if std::fs::symlink_metadata(path)?.file_type().is_symlink() {
            return Err(std::io::Error::other("it's a link"));
        }
        std::fs::File::open(path)
    }
}

/// Whether the volume `path` is on can't be written: a disk image the app runs from, say.
pub fn read_only_volume(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        let Ok(c) = std::ffi::CString::new(path.as_os_str().as_bytes()) else { return false };
        // SAFETY: statvfs reads the NUL-ended path and fills the struct it's given.
        let mut fs: libc::statvfs = unsafe { std::mem::zeroed() };
        unsafe { libc::statvfs(c.as_ptr(), &mut fs) == 0 && fs.f_flag & libc::ST_RDONLY != 0 }
    }
    #[cfg(windows)]
    {
        let _ = path;
        false
    }
}
