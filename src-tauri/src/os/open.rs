//! Opening things as a click would: a link or a file in the app the OS has for it, a file shown in
//! its folder, an app by its name or ID, and a program left to run by itself. macOS's `open` does
//! all but the last; Linux has `xdg-open` for links and files and an app's own command, which
//! `start_detached` runs, and opens no app by name. Each is a program run, so both OSes' ways
//! compile and are tested on either. Each caller says how long it gets and what a failure is
//! called.

use std::path::Path;
use std::time::Duration;

use anyhow::{anyhow, Result};

use super::Os;
use crate::procs;

/// An app, as macOS's `open` names one: by its name or path (`-a`), or by its bundle ID (`-b`).
#[derive(Clone, Copy, Debug)]
pub enum App<'a> {
    Named(&'a str),
    Id(&'a str),
}

impl App<'_> {
    fn args(&self) -> [&str; 2] {
        match self {
            App::Named(name) => ["-a", name],
            App::Id(id) => ["-b", id],
        }
    }
}

/// A link, or a file, in the app the OS opens it with.
pub fn url(target: &str, timeout: Duration) -> Result<()> {
    match super::current() {
        Os::MacOs => procs::run("/usr/bin/open", &[target], None, timeout).map(drop),
        Os::Linux => procs::run("xdg-open", &[target], None, timeout).map(drop),
    }
}

/// A file shown where it is: in the Finder, selected; on Linux its folder, in the file manager.
pub fn reveal(path: &Path, timeout: Duration) -> Result<()> {
    match super::current() {
        Os::MacOs => procs::run("/usr/bin/open", &["-R", &path.to_string_lossy()], None, timeout).map(drop),
        Os::Linux => url(&path.parent().unwrap_or(path).to_string_lossy(), timeout),
    }
}

/// An app brought up, with `target` (a file, a link) opened in it.
pub fn in_app(app: App, target: Option<&str>, timeout: Duration) -> Result<()> {
    match super::current() {
        Os::MacOs => {
            let mut args = app.args().to_vec();
            args.extend(target);
            procs::run("/usr/bin/open", &args, None, timeout).map(drop)
        }
        Os::Linux => Err(super::only_on_macos("Opening an app by its name")),
    }
}

/// An app started with `args`, as a copy of its own when `new_copy` (a running app only takes
/// arguments as a new copy starts: Chrome's hands them to the one running, and quits).
pub fn launch(app: App, new_copy: bool, args: &[&str], timeout: Duration) -> Result<()> {
    match super::current() {
        Os::MacOs => {
            let mut all: Vec<&str> = if new_copy { vec!["-n"] } else { Vec::new() };
            all.extend(app.args());
            all.push("--args");
            all.extend_from_slice(args);
            procs::run("/usr/bin/open", &all, None, timeout).map(drop)
        }
        Os::Linux => Err(super::only_on_macos("Opening an app by its name")),
    }
}

/// Starts a program that carries on by itself, a window's app say, and doesn't wait for it: a
/// shell starts it in a session of its own and is gone at once, so it outlives no process of
/// Codealong's and nothing has to wait for it to end. Its arguments go to it as they are. Gone,
/// the shell couldn't say the program isn't there, so it looks first and fails when it isn't, and
/// it waits a moment for one that fails as it starts. One that's quick to hand over to a copy
/// already running (GNOME Terminal's, VS Code's, Chrome's) and exits cleanly has started. One
/// that fails later than that, or out of sight, isn't seen. Linux's way to open an app: macOS's
/// is `open`, and it has no `setsid`.
pub fn start_detached(program: &str, args: &[&str], timeout: Duration) -> Result<()> {
    let mut all = vec!["-c", START_DETACHED, program];
    all.extend_from_slice(args);
    // The shell's words are about the program: they're said of it.
    procs::run("/bin/sh", &all, None, timeout).map(drop).map_err(|e| anyhow!("{program} {}", e.to_string().trim_start_matches("/bin/sh").trim()))
}

pub(crate) const START_DETACHED: &str = "command -v \"$0\" >/dev/null || { echo 'is not installed' >&2; exit 127; }; setsid \"$0\" \"$@\" </dev/null >/dev/null 2>&1 & started=$!; sleep 0.3; kill -0 \"$started\" 2>/dev/null && exit 0; wait \"$started\" || { s=$?; echo \"exited with status $s as it started\" >&2; exit $s; }";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::os::as_if;

    /// What each way of opening sends, on each OS.
    #[test]
    fn opens_go_to_each_oses_own_program() {
        let sent = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let log = sent.clone();
        procs::stand_in::set(Some(Box::new(move |program, args, _, timeout| {
            log.borrow_mut().push(format!("{program} {args:?} {}s", timeout.as_secs()));
            Ok(String::new())
        })));
        let t = Duration::from_secs(10);
        as_if(Os::MacOs, || {
            url("https://a.test", t).unwrap();
            reveal(Path::new("/r/a.md"), t).unwrap();
            in_app(App::Named("Cursor"), Some("/r"), t).unwrap();
            in_app(App::Id("com.google.Chrome"), None, t).unwrap();
            launch(App::Id("com.google.Chrome"), true, &["--profile-directory=Default", "https://a.test"], t).unwrap();
            launch(App::Named("Ghostty.app"), true, &["--x"], t).unwrap();
        });
        let linux = as_if(Os::Linux, || {
            url("https://a.test", t).unwrap();
            reveal(Path::new("/r/a.md"), t).unwrap();
            (in_app(App::Named("Cursor"), Some("/r"), t).unwrap_err().to_string(), launch(App::Id("x"), false, &[], t).is_err())
        });
        procs::stand_in::set(None);

        assert_eq!(linux, ("Opening an app by its name is only on macOS.".to_string(), true));
        assert_eq!(
            *sent.borrow(),
            [
                r#"/usr/bin/open ["https://a.test"] 10s"#,
                r#"/usr/bin/open ["-R", "/r/a.md"] 10s"#,
                r#"/usr/bin/open ["-a", "Cursor", "/r"] 10s"#,
                r#"/usr/bin/open ["-b", "com.google.Chrome"] 10s"#,
                r#"/usr/bin/open ["-n", "-b", "com.google.Chrome", "--args", "--profile-directory=Default", "https://a.test"] 10s"#,
                r#"/usr/bin/open ["-n", "-a", "Ghostty.app", "--args", "--x"] 10s"#,
                r#"xdg-open ["https://a.test"] 10s"#,
                r#"xdg-open ["/r"] 10s"#,
            ]
        );
    }

    /// Run for real: a program that's still running after the moment, or done and fine, has
    /// started; one that isn't there, or exits with an error at once, says so. Linux's, which has
    /// `setsid`.
    #[test]
    fn a_program_started_by_itself_is_looked_for_first() {
        if crate::os::NATIVE != Os::Linux {
            return;
        }
        assert!(start_detached("/bin/sleep", &["2"], Duration::from_secs(5)).is_ok());
        assert!(start_detached("/bin/true", &[], Duration::from_secs(5)).is_ok());
        let e = start_detached("no-such-program-codealong", &["x"], Duration::from_secs(5)).unwrap_err();
        assert_eq!(e.to_string(), "no-such-program-codealong is not installed");
        let e = start_detached("/bin/sh", &["-c", "exit 3"], Duration::from_secs(5)).unwrap_err();
        assert!(e.to_string().ends_with("exited with status 3 as it started"), "{e}");
    }
}
