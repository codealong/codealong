//! Terminal devices, which macOS and Linux have alike (POSIX's) and Windows hasn't: a console
//! there is no file a program opens by its path, and none is a session's. A session's terminal
//! written to without becoming the writer's own, asked whether its program reads it a key at a
//! time, this process's own terminal put in raw mode and asked its size
//! (`codealong terminal attach`), and the login shell the user database names.

use std::path::Path;

/// Opens a terminal device to write to it, without making it this process's controlling terminal
/// and without waiting on it.
#[cfg(unix)]
pub fn open_to_write(tty: &Path) -> std::io::Result<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new().write(true).custom_flags(libc::O_NOCTTY | libc::O_NONBLOCK).open(tty)
}

#[cfg(windows)]
pub fn open_to_write(_tty: &Path) -> std::io::Result<std::fs::File> {
    Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "Windows has no terminal devices"))
}

/// Whether a program on the terminal reads it a key at a time, as a shell's line editor does
/// while it waits for you: canonical mode off. Asked of the terminal device; nothing is written.
#[cfg(unix)]
pub fn reads_keys(tty: &Path) -> bool {
    use std::os::fd::AsRawFd;
    let Ok(f) = open_to_write(tty) else { return false };
    // SAFETY: tcgetattr fills a plain-old-data struct for an open descriptor.
    unsafe {
        let mut t: libc::termios = std::mem::zeroed();
        libc::tcgetattr(f.as_raw_fd(), &mut t) == 0 && t.c_lflag & libc::ICANON == 0
    }
}

#[cfg(windows)]
pub fn reads_keys(_tty: &Path) -> bool {
    false
}

/// The login shell, as the user database has it: what a terminal app starts in a new tab. None on
/// Windows, which has no login shell.
#[cfg(unix)]
pub fn login_shell() -> Option<String> {
    // SAFETY: getpwuid_r fills `pw` with pointers into `buf`, which outlives their use here.
    unsafe {
        let mut pw: libc::passwd = std::mem::zeroed();
        let mut found: *mut libc::passwd = std::ptr::null_mut();
        let mut buf = vec![0 as libc::c_char; 4096];
        let r = libc::getpwuid_r(libc::getuid(), &mut pw, buf.as_mut_ptr(), buf.len(), &mut found);
        (r == 0 && !found.is_null() && !pw.pw_shell.is_null()).then(|| std::ffi::CStr::from_ptr(pw.pw_shell).to_string_lossy().into_owned())
    }
}

#[cfg(windows)]
pub fn login_shell() -> Option<String> {
    None
}

/// This user's number, which POSIX names some of its folders by (`/tmp/tmux-<uid>`, launchd's
/// `gui/<uid>`). Windows numbers no user: 0 there, where nothing is named by one.
#[cfg(unix)]
pub fn user_id() -> u32 {
    // SAFETY: getuid only reads this process's user id.
    unsafe { libc::getuid() }
}

#[cfg(windows)]
pub fn user_id() -> u32 {
    0
}

/// This process's terminal's settings before `raw` changed them, put back by `restore`.
#[cfg(unix)]
static SAVED: std::sync::OnceLock<libc::termios> = std::sync::OnceLock::new();

/// Puts this process's terminal (its stdin) in raw mode: every key reaches it as it's typed,
/// nothing echoed or taken by the terminal's line discipline (Ctrl+C included).
#[cfg(unix)]
pub fn raw() -> anyhow::Result<()> {
    // SAFETY: tcgetattr fills a plain struct for an open descriptor; cfmakeraw and tcsetattr take it.
    unsafe {
        let mut t: libc::termios = std::mem::zeroed();
        if libc::tcgetattr(0, &mut t) != 0 {
            anyhow::bail!("Couldn't read this terminal's settings: {}", std::io::Error::last_os_error());
        }
        let _ = SAVED.set(t);
        libc::cfmakeraw(&mut t);
        if libc::tcsetattr(0, libc::TCSANOW, &t) != 0 {
            anyhow::bail!("Couldn't put this terminal in raw mode: {}", std::io::Error::last_os_error());
        }
    }
    Ok(())
}

/// Codealong's own terminals aren't on Windows yet, so nothing attaches to one there.
#[cfg(windows)]
pub fn raw() -> anyhow::Result<()> {
    Err(super::not_on_windows("Attaching to one of Codealong's terminals"))
}

/// Puts this process's terminal back as it was before `raw`.
#[cfg(unix)]
pub fn restore() {
    if let Some(saved) = SAVED.get() {
        // SAFETY: tcsetattr with settings tcgetattr gave for the same descriptor.
        unsafe { libc::tcsetattr(0, libc::TCSANOW, saved) };
    }
}

#[cfg(windows)]
pub fn restore() {}

/// This process's terminal's size in cells, by its stdout.
#[cfg(unix)]
pub fn size() -> Option<(u16, u16)> {
    // SAFETY: TIOCGWINSZ fills a plain struct for an open descriptor.
    unsafe {
        let mut ws: libc::winsize = std::mem::zeroed();
        (libc::ioctl(1, libc::TIOCGWINSZ, &mut ws) == 0 && ws.ws_col > 0 && ws.ws_row > 0).then_some((ws.ws_col, ws.ws_row))
    }
}

#[cfg(windows)]
pub fn size() -> Option<(u16, u16)> {
    None
}

/// Whether this process's stdin has something to read within `ms` milliseconds.
#[cfg(unix)]
pub fn stdin_ready(ms: i32) -> bool {
    let mut p = libc::pollfd { fd: 0, events: libc::POLLIN, revents: 0 };
    // SAFETY: one live pollfd, and a count of one.
    unsafe { libc::poll(&mut p, 1, ms) > 0 }
}

#[cfg(windows)]
pub fn stdin_ready(_ms: i32) -> bool {
    true
}

/// Reads what this process's stdin has, straight from its descriptor rather than through the
/// standard library's buffer, so a wait for more (`stdin_ready`) sees everything that came.
#[cfg(unix)]
pub fn read_stdin(buf: &mut [u8]) -> std::io::Result<usize> {
    // SAFETY: read writes at most `buf.len()` bytes into it.
    let n = unsafe { libc::read(0, buf.as_mut_ptr().cast(), buf.len()) };
    if n < 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(n as usize)
    }
}

#[cfg(windows)]
pub fn read_stdin(buf: &mut [u8]) -> std::io::Result<usize> {
    use std::io::Read;
    std::io::stdin().read(buf)
}
