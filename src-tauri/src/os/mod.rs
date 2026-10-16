//! Everything Codealong asks of the operating system it runs on, macOS or Linux, in one place.
//! What differs between them is of two kinds, kept apart so that as much as can be is compiled
//! and tested on either:
//!
//! - What only one OS has to call: AppKit and WebKit through objc2, GTK, and the kernel's own
//!   calls (`proc_pidinfo`, `sysctl` and kqueue on macOS; `/proc`, eventfd and pidfds on Linux).
//!   Each area of it here has a `macos.rs` and a `linux.rs` that give the same functions, and its
//!   `mod.rs` picks one as Codealong is built. Here, and nowhere else, `cfg(target_os)` decides
//!   what's compiled (`tests::only_os_compiles_by_os`).
//! - What differs by OS but calls nothing of it: where an app keeps its files, which program opens
//!   a link, which terminals there are, how Claude Code keeps its sign-in. That goes by
//!   `current()`, a value, so both OSes' ways compile and are tested on either, and a test can
//!   ask as if on the other (`as_if`).
//!
//! What one OS has and the other hasn't: where there's nothing to do on the other (no App Nap, no
//! Dock icon), its side does nothing and its comment says why; where something was asked for that
//! can't be done there (an app opened by its name), it fails with `only_on_macos`, in words the
//! caller passes on.

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
compile_error!("Codealong runs on macOS and Linux.");

pub mod apps;
pub mod dirs;
pub mod exits;
pub mod files;
pub mod open;
pub mod outline;
pub mod procs;
pub mod secrets;
pub mod sound;
pub mod system;
pub mod windows;

/// An operating system Codealong runs on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Os {
    MacOs,
    Linux,
}

/// The OS Codealong was built for, whatever a test asks as if on.
pub const NATIVE: Os = if cfg!(target_os = "macos") { Os::MacOs } else { Os::Linux };

/// The OS whose ways Codealong goes by: the one it runs on, or in a test the one it asks as if on.
pub fn current() -> Os {
    #[cfg(test)]
    if let Some(os) = AS_IF.with(|a| a.get()) {
        return os;
    }
    NATIVE
}

pub fn is_macos() -> bool {
    current() == Os::MacOs
}

/// What's said of something asked for that only macOS does: "Opening an app by its name is only
/// on macOS."
pub fn only_on_macos(what: &str) -> anyhow::Error {
    anyhow::anyhow!("{what} is only on macOS.")
}

#[cfg(test)]
thread_local! {
    static AS_IF: std::cell::Cell<Option<Os>> = const { std::cell::Cell::new(None) };
}

/// Runs `f` as if on `os`: what goes by `current()` goes by it, on this test's thread.
#[cfg(test)]
pub fn as_if<T>(os: Os, f: impl FnOnce() -> T) -> T {
    let _on = AsIf::new(os);
    f()
}

/// `as_if` for the rest of a scope, a whole test say: until it's dropped.
#[cfg(test)]
pub struct AsIf(Option<Os>);

#[cfg(test)]
impl AsIf {
    pub fn new(os: Os) -> AsIf {
        AsIf(AS_IF.with(|a| a.replace(Some(os))))
    }
}

#[cfg(test)]
impl Drop for AsIf {
    fn drop(&mut self) {
        AS_IF.with(|a| a.set(self.0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Only the OS's own calls are picked as the engine is built, and only here: everything else
    /// compiles on either OS and goes by `current()`. A test of what only one OS has (its sounds,
    /// its `sips`) may say so by its `#[test]`.
    #[test]
    fn only_os_compiles_by_os() {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut found = Vec::new();
        let mut files = vec![src.clone()];
        while let Some(at) = files.pop() {
            if at.is_dir() {
                if at != src.join("os") {
                    files.extend(std::fs::read_dir(&at).unwrap().flatten().map(|e| e.path()));
                }
                continue;
            }
            if at.extension().map_or(true, |e| e != "rs") {
                continue;
            }
            let text = std::fs::read_to_string(&at).unwrap();
            let lines: Vec<&str> = text.lines().collect();
            for (i, line) in lines.iter().enumerate() {
                let under_test = i > 0 && lines[i - 1].trim() == "#[test]";
                if line.contains(&format!("target_{}", "os")) && !under_test {
                    found.push(format!("{}:{}", at.strip_prefix(&src).unwrap().display(), i + 1));
                }
            }
        }
        assert!(found.is_empty(), "OS conditions outside os/: {found:?}");
    }

    #[test]
    fn a_test_can_ask_as_if_on_the_other_os() {
        let other = if NATIVE == Os::MacOs { Os::Linux } else { Os::MacOs };
        assert_eq!(current(), NATIVE);
        assert_eq!(as_if(other, || (current(), as_if(NATIVE, current), current())), (other, NATIVE, other));
        assert_eq!(current(), NATIVE);
    }
}
