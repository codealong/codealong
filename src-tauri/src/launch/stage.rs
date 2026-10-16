//! A stand-in for the Mac, for tests of what the terminals are sent. While a test plays on it,
//! every program launch.rs would run (`osascript`, `tmux`, `open`) is written down with its
//! arguments and answered by the test in place of being run, and the processes, their terminal
//! devices, the folder reports written to them and the waits are the stage's. So a test says
//! exactly what Ghostty, iTerm2, Terminal and tmux would have been asked, in order, for each
//! thing Codealong does with a terminal, and no window opens and nothing is typed.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

/// A process as a test lists it: its pid, its parent, its name, and its terminal device.
pub type Proc = (u32, u32, &'static str, Option<&'static str>);

/// What a play starts from.
pub struct Setup {
    pub procs: &'static [Proc],
    /// Whether the terminal apps are on this Mac (Terminal always is).
    pub installed: bool,
    /// Whether a new tab's shell gets ready for typing in time.
    pub shell_ready: bool,
    /// The processes that don't exit when hung up.
    pub stays: &'static [u32],
    /// Whether macOS may nap Ghostty (App Nap).
    pub ghostty_naps: bool,
}

impl Default for Setup {
    fn default() -> Self {
        Setup { procs: &[], installed: true, shell_ready: true, stays: &[], ghostty_naps: false }
    }
}

/// Something a program was sent: an AppleScript by its name in `SCRIPTS` (or `script#<hash>` for
/// one that isn't there), or another program by its path, with the arguments as they were, and
/// how long it was given.
pub struct Sent {
    pub what: String,
    pub args: Vec<String>,
    pub timeout: Duration,
}

struct Stage {
    setup: Setup,
    log: Vec<String>,
    marks: Vec<String>,
    launchers: Vec<PathBuf>,
}

thread_local! {
    static STAGE: RefCell<Option<Stage>> = const { RefCell::new(None) };
}

/// One play at a time: focusing takes turns through a lock every test shares.
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

/// Every AppleScript by the hash of its text, so a test names the script that was sent and a
/// script whose text changed stops being known. Terminal's TYPE and TYPE_LINE are the same text.
const SCRIPTS: &[(&str, u64)] = &[
    ("GHOSTTY_NEW", 0xcfd07ea6d9f9e85b),
    ("GHOSTTY_LIST", 0xb5f37a7f6c1d405c),
    ("GHOSTTY_FOCUS", 0x2060d52fdb69a8e4),
    ("GHOSTTY_FIND", 0x18c5641264cf22da),
    ("GHOSTTY_TYPE", 0x1564f2adc9419537),
    ("GHOSTTY_IN_FRONT", 0x27cafb0b698ccc18),
    ("GHOSTTY_CLOSE", 0x838b3e2c07f8f8ef),
    ("GHOSTTY_KEY", 0xe38cd6b6c3f7bf31),
    ("GHOSTTY_ACTION", 0x7b0d78d41a2bbe02),
    ("ITERM_NEW", 0xa921c82270e903da),
    ("ITERM_LIST", 0x5062105823ca2b42),
    ("ITERM_IN_FRONT", 0x77aa4e86c8f0c7b0),
    ("ITERM_TTYS", 0x7ea1519eae6b1ee2),
    ("ITERM_FOCUS+SESSION_ON", 0xb050549c0c27a8b5),
    ("ITERM_TYPE+SESSION_ON", 0x53024d4295554970),
    ("ITERM_TYPE_LINE+SESSION_ON", 0x9a72e6983148ca8d),
    ("ITERM_PRESS+SESSION_ON", 0xdd3dee147c2951f2),
    ("ITERM_CLOSE+SESSION_ON", 0xd63cd2b0deb962da),
    ("TERMINAL_NEW", 0x6694c693b39996f1),
    ("TERMINAL_IN_FRONT", 0x45e9e0062546e5c4),
    ("TERMINAL_TTYS", 0x8ee9c7bb3dfcb130),
    ("TERMINAL_FOCUS+TAB_ON", 0x7ae29d2776b9d20b),
    ("TERMINAL_TYPE+TAB_ON", 0xa7c5c56b1a8acc66),
    ("TERMINAL_CLOSE+TAB_ON", 0x395c0be9fa50e975),
];

fn fnv(text: &str) -> u64 {
    text.bytes().fold(0xcbf29ce484222325, |h, b| (h ^ b as u64).wrapping_mul(0x100000001b3))
}

fn script_name(text: &str) -> String {
    let hash = fnv(text);
    SCRIPTS.iter().find(|(_, h)| *h == hash).map_or_else(|| format!("script#{hash:016x}"), |(name, _)| name.to_string())
}

/// What a play did: what the act returned, everything sent and noted, in order, and the
/// launchers written, each as its text.
pub struct Played<T> {
    pub got: T,
    pub log: Vec<String>,
    pub launchers: Vec<String>,
}

/// Runs `act` on the stage. `answer` is each program's output, or what it failed with.
pub fn play<T>(setup: Setup, answer: impl Fn(&Sent) -> Result<String, String> + 'static, act: impl FnOnce() -> T) -> Played<T> {
    let _one = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    STAGE.with(|s| *s.borrow_mut() = Some(Stage { setup, log: Vec::new(), marks: Vec::new(), launchers: Vec::new() }));
    crate::procs::stand_in::set(Some(Box::new(move |program, args, input, timeout| {
        let sent = if program == "/usr/bin/osascript" {
            assert_eq!(args.first(), Some(&"-"), "the script goes in on stdin");
            let what = script_name(input.expect("a script"));
            // A terminal app gets a minute to answer, but for finding a session's Ghostty tab to
            // type a send into, which has another way to go (ghostty.rs `FIND_WAIT`).
            let finding = matches!(what.as_str(), "GHOSTTY_LIST" | "GHOSTTY_FIND") && !timeout.is_zero() && timeout <= super::ghostty::FIND_WAIT;
            assert!(timeout == Duration::from_secs(60) || finding, "{what} was given {timeout:?}");
            Sent { what, args: args[1..].iter().map(|a| a.to_string()).collect(), timeout }
        } else {
            Sent { what: program.to_string(), args: args.iter().map(|a| a.to_string()).collect(), timeout }
        };
        let shown: Vec<String> = sent.args.iter().map(|a| tidy(a)).collect();
        let mut line = format!("{} {shown:?}", sent.what);
        if program != "/usr/bin/osascript" {
            if let Some(input) = input {
                line.push_str(&format!(" < {input:?}"));
            }
            line.push_str(&format!(" ({}s)", timeout.as_secs()));
        }
        note(line);
        answer(&sent).map_err(|e| anyhow::anyhow!("{e}"))
    })));
    let got = act();
    crate::procs::stand_in::set(None);
    let stage = STAGE.with(|s| s.borrow_mut().take()).expect("the stage");
    let launchers = stage
        .launchers
        .iter()
        .filter_map(|path| {
            let text = std::fs::read_to_string(path).ok()?;
            let _ = std::fs::remove_file(path);
            // The login shell is this Mac's; everything else is the launcher's own.
            Some(text.lines().map(|l| if l.starts_with("exec ") { "exec SHELL -l".to_string() } else { tidy(l) }).collect::<Vec<_>>().join("\n"))
        })
        .collect();
    Played { got, log: stage.log, launchers }
}

/// A line without what differs from run to run: the time in a mark, and a launcher's path.
fn tidy(text: &str) -> String {
    let mut out = text.to_string();
    for mark in marks() {
        if let Some((stem, _)) = mark.rsplit_once('-') {
            out = out.replace(&mark, &format!("{stem}-NOW"));
        }
    }
    let tmp = std::env::temp_dir().join("codealong-launch-").to_string_lossy().into_owned();
    while let Some(at) = out.find(&tmp) {
        let end = out[at..].find(".sh").map_or(out.len(), |n| at + n + 3);
        let path = PathBuf::from(&out[at..end]);
        STAGE.with(|s| {
            if let Some(stage) = s.borrow_mut().as_mut() {
                if !stage.launchers.contains(&path) {
                    stage.launchers.push(path);
                }
            }
        });
        out.replace_range(at..end, "LAUNCHER");
    }
    out
}

fn note(line: String) {
    STAGE.with(|s| {
        if let Some(stage) = s.borrow_mut().as_mut() {
            stage.log.push(line);
        }
    });
}

fn staged<T>(f: impl FnOnce(&Stage) -> T) -> Option<T> {
    STAGE.with(|s| s.borrow().as_ref().map(f))
}

/// The marks reported on terminals so far, as they were written: a test answers Ghostty's
/// questions about them.
pub fn marks() -> Vec<String> {
    staged(|s| s.marks.clone()).unwrap_or_default()
}

// What launch.rs asks the stage in place of the Mac. Each is None when no play is on.

pub fn app_installed() -> Option<bool> {
    staged(|s| s.setup.installed)
}

pub fn parent_and_name(pid: u32) -> Option<Option<(u32, String)>> {
    staged(|s| s.setup.procs.iter().find(|p| p.0 == pid).map(|p| (p.1, p.2.to_string())))
}

pub fn tty(pid: u32) -> Option<Option<PathBuf>> {
    staged(|s| s.setup.procs.iter().find(|p| p.0 == pid).and_then(|p| p.3).map(PathBuf::from))
}

pub fn tmux_bin() -> Option<PathBuf> {
    staged(|_| PathBuf::from("/opt/homebrew/bin/tmux"))
}

/// Every app is the active one, so no focus waits on a thread of its own to raise a tab again.
pub fn app_active(_app: &str) -> Option<bool> {
    staged(|_| true)
}

pub fn ghostty_naps() -> Option<bool> {
    staged(|s| s.setup.ghostty_naps)
}

pub fn osc7(tty: &Path, dir: &str) -> Option<()> {
    staged(|_| ())?;
    if dir.starts_with(super::MARK_PREFIX) {
        STAGE.with(|s| s.borrow_mut().as_mut().expect("the stage").marks.push(dir.to_string()));
    }
    note(format!("osc7 {} {}", tty.display(), tidy(dir)));
    Some(())
}

pub fn shell_ready(tty: &str, launcher: Option<&Path>) -> Option<bool> {
    let ready = staged(|s| s.setup.shell_ready)?;
    note(match launcher {
        Some(l) => format!("wait for shell {tty} after {}", tidy(&l.to_string_lossy())),
        None => format!("wait for shell {tty}"),
    });
    Some(ready)
}

pub fn hang_up(pids: &[u32]) -> Option<Vec<bool>> {
    let stays = staged(|s| s.setup.stays)?;
    note(format!("hang up {pids:?}"));
    Some(pids.iter().map(|p| !stays.contains(p)).collect())
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::path::Path;
    use std::rc::Rc;

    use super::super::*;
    use super::{marks, play, Played, Proc, Sent, Setup};
    use crate::config::NewTerminal;

    /// Sessions in each terminal, one in the background and one in a terminal Codealong doesn't
    /// know. The pids are past the highest macOS gives out, so none is a real process.
    const PROCS: &[Proc] = &[
        (100_001, 1, "ghostty", None),
        (100_002, 100_001, "login", None),
        (100_003, 100_002, "zsh", Some("/dev/ttys101")),
        (100_010, 100_003, "claude", Some("/dev/ttys101")),
        (100_012, 100_001, "login", None),
        (100_013, 100_012, "zsh", Some("/dev/ttys102")),
        (100_020, 100_013, "claude", Some("/dev/ttys102")),
        (200_001, 1, "iTerm2", None),
        (200_002, 200_001, "iTermServer-3.7.", None),
        (200_003, 200_002, "zsh", Some("/dev/ttys201")),
        (200_010, 200_003, "claude", Some("/dev/ttys201")),
        (200_013, 200_002, "zsh", Some("/dev/ttys202")),
        (200_020, 200_013, "codex", Some("/dev/ttys202")),
        (300_001, 1, "Terminal", None),
        (300_002, 300_001, "login", None),
        (300_003, 300_002, "zsh", Some("/dev/ttys301")),
        (300_010, 300_003, "claude", Some("/dev/ttys301")),
        (400_001, 1, "tmux", None),
        (400_002, 400_001, "zsh", Some("/dev/ttys401")),
        (400_010, 400_002, "claude", Some("/dev/ttys401")),
        (500_010, 1, "claude", None),
        (600_001, 1, "kitty", None),
        (600_002, 600_001, "zsh", Some("/dev/ttys601")),
        (600_010, 600_002, "claude", Some("/dev/ttys601")),
        // Clients of tmux (`tmux attach`): in Ghostty, iTerm2 and kitty, and in a pane of tmux.
        (100_032, 100_001, "login", None),
        (100_033, 100_032, "zsh", Some("/dev/ttys103")),
        (100_030, 100_033, "tmux", Some("/dev/ttys103")),
        (200_023, 200_002, "zsh", Some("/dev/ttys203")),
        (200_030, 200_023, "tmux", Some("/dev/ttys203")),
        (600_013, 600_001, "zsh", Some("/dev/ttys602")),
        (600_030, 600_013, "tmux", Some("/dev/ttys602")),
        (400_013, 400_001, "zsh", Some("/dev/ttys402")),
        (400_030, 400_013, "tmux", Some("/dev/ttys402")),
    ];
    const IN_GHOSTTY: u32 = 100_010;
    const ALSO_IN_GHOSTTY: u32 = 100_020;
    const IN_ITERM: u32 = 200_010;
    const ALSO_IN_ITERM: u32 = 200_020;
    const IN_TERMINAL: u32 = 300_010;
    const IN_TMUX: u32 = 400_010;
    const IN_BACKGROUND: u32 = 500_010;
    const IN_KITTY: u32 = 600_010;

    fn setup() -> Setup {
        Setup { procs: PROCS, ..Setup::default() }
    }

    const NOT_ALLOWED: &str = "execution error: Not authorized to send Apple events to the app. (-1743)";

    /// Ghostty's terminals as GHOSTTY_LIST prints them: T1 is the first session's tab.
    const GHOSTTY_TABS: &str = "T1\t/r/app\t✳ Fix the login\nT2\t/r/other\t✳ Bump dependencies\nT3\t/r/app\tzsh\n";
    /// iTerm2's sessions as its LIST prints them.
    const ITERM_TABS: &str = "/dev/ttys201\t/r/app\t✳ Fix the login\n/dev/ttys202\t/r/other\tcodex\n";

    fn window() -> Placement<'static> {
        Placement { new_terminal: NewTerminal::Window, near: &[], behind: false, label: None }
    }

    fn err<T>(r: anyhow::Result<T>) -> String {
        r.err().map(|e| e.to_string()).expect("an error")
    }

    fn played<T>(answer: impl Fn(&Sent) -> Result<String, String> + 'static, act: impl FnOnce() -> T) -> Played<T> {
        play(setup(), answer, act)
    }

    #[test]
    fn ghostty_is_sent_a_new_window_or_tab_with_its_input_and_environment() {
        let env = [("CODEALONG_PROMPT".to_string(), "Fix the 'flaky' test".to_string()), ("CODEALONG_FILES".to_string(), "/tmp/files".to_string())];
        let p = played(|_| Ok("ok\n".into()), || terminal_run("ghostty", window(), Path::new("/r/my app"), "claude2 --session-id x \"$CODEALONG_PROMPT\"\n", &env));
        p.got.unwrap();
        assert_eq!(p.log, [r#"GHOSTTY_NEW ["window", "", "/r/my app", "claude2 --session-id x \"$CODEALONG_PROMPT\"\n", "front", "", "CODEALONG_PROMPT=Fix the 'flaky' test", "CODEALONG_FILES=/tmp/files"]"#]);
        assert!(p.launchers.is_empty(), "Ghostty takes the folder and environment itself");

        let tab = Placement { new_terminal: NewTerminal::Tab, near: &[], behind: false, label: None };
        let p = played(|_| Ok("ok\n".into()), move || terminal_run("ghostty", tab, Path::new("/r/app"), "ls\n", &[]));
        p.got.unwrap();
        assert_eq!(p.log, [r#"GHOSTTY_NEW ["front", "", "/r/app", "ls\n", "front", ""]"#]);
        // A Background start: Ghostty isn't asked to come forward.
        let behind = Placement { behind: true, ..tab };
        let p = played(|_| Ok("ok\n".into()), move || terminal_run("ghostty", behind, Path::new("/r/app"), "ls\n", &[("A".to_string(), "1".to_string())]));
        p.got.unwrap();
        assert_eq!(p.log, [r#"GHOSTTY_NEW ["front", "", "/r/app", "ls\n", "behind", "", "A=1"]"#]);

        // Beside the tabs of the nearest folders that have any: Ghostty is asked what it has. The
        // tab's name goes as it is, for the script to give the tab.
        let near = [vec!["/r/app-pins".to_string()], vec!["/r/app".to_string(), "/r/else".to_string()]];
        let answer = |s: &Sent| Ok(if s.what == "GHOSTTY_LIST" { GHOSTTY_TABS.to_string() } else { "ok\n".into() });
        let named = Placement { new_terminal: NewTerminal::Project, near: &near, behind: false, label: Some("app · Fix the \"login\": now") };
        let p = played(answer, || terminal_run("ghostty", named, Path::new("/r/app"), "ls\n", &[]));
        p.got.unwrap();
        assert_eq!(p.log, ["GHOSTTY_LIST []", r#"GHOSTTY_NEW ["beside", "T1\nT3", "/r/app", "ls\n", "front", "app · Fix the \"login\": now"]"#]);
        // A tab in the front window is handed the same tabs, to sit beside where they're there;
        // with none near, or Ghostty not saying, it still goes in the front window.
        let p = played(answer, || terminal_run("ghostty", Placement { new_terminal: NewTerminal::Tab, ..named }, Path::new("/r/app"), "ls\n", &[]));
        p.got.unwrap();
        assert_eq!(p.log, ["GHOSTTY_LIST []", r#"GHOSTTY_NEW ["front", "T1\nT3", "/r/app", "ls\n", "front", "app · Fix the \"login\": now"]"#]);
        let far = [vec!["/r/none".to_string()]];
        let p = played(|_| Err("boom".into()), || terminal_run("ghostty", Placement { new_terminal: NewTerminal::Tab, near: &far, behind: false, label: None }, Path::new("/r/app"), "ls\n", &[]));
        assert_eq!(p.log[0], "GHOSTTY_LIST []");
        assert_eq!(p.log[1], r#"GHOSTTY_NEW ["front", "", "/r/app", "ls\n", "front", ""]"#);

        // No tab near, or Ghostty not saying: a window.
        let far = [vec!["/r/none".to_string()]];
        let p = played(answer, || terminal_run("ghostty", Placement { new_terminal: NewTerminal::Project, near: &far, behind: false, label: None }, Path::new("/r/app"), "ls\n", &[]));
        p.got.unwrap();
        assert_eq!(p.log, ["GHOSTTY_LIST []", r#"GHOSTTY_NEW ["window", "", "/r/app", "ls\n", "front", ""]"#]);
        let p = played(|s| if s.what == "GHOSTTY_LIST" { Err("boom".into()) } else { Ok("ok\n".into()) }, || terminal_run("ghostty", Placement { new_terminal: NewTerminal::Project, near: &near, behind: false, label: None }, Path::new("/r/app"), "ls\n", &[]));
        p.got.unwrap();
        assert_eq!(p.log, ["GHOSTTY_LIST []", r#"GHOSTTY_NEW ["window", "", "/r/app", "ls\n", "front", ""]"#]);

        // What goes wrong is said in Ghostty's name.
        let p = played(|_| Err(NOT_ALLOWED.into()), || terminal_run("ghostty", window(), Path::new("/r/app"), "ls\n", &[]));
        assert_eq!(err(p.got), "Codealong isn't allowed to control Ghostty. Turn it on in System Settings → Privacy & Security → Automation.");
        let p = played(|_| Err("/usr/bin/osascript timed out".into()), || terminal_run("ghostty", window(), Path::new("/r/app"), "ls\n", &[]));
        assert_eq!(err(p.got), "Ghostty didn't answer within a minute. It may still open the window; if macOS asked whether Codealong may control Ghostty, allow it and try again.");
        let p = played(|_| Err("boom".into()), || terminal_run("ghostty", window(), Path::new("/r/app"), "", &[]));
        assert_eq!(err(p.got), "Ghostty didn't respond: boom");
        assert_eq!(p.log, [r#"GHOSTTY_NEW ["window", "", "/r/app", "", "front", ""]"#], "a run has no other way to open");
    }

    #[test]
    fn ghostty_opens_a_plain_window_another_way_when_its_script_fails() {
        let p = played(|_| Ok("ok\n".into()), || terminal_open("ghostty", window(), Path::new("/r/app"), None));
        p.got.unwrap();
        assert_eq!(p.log, [r#"GHOSTTY_NEW ["window", "", "/r/app", "", "front", ""]"#]);

        let p = played(|s| if s.what == "GHOSTTY_NEW" { Err(NOT_ALLOWED.into()) } else { Ok(String::new()) }, || terminal_open("ghostty", window(), Path::new("/r/app"), None));
        p.got.unwrap();
        assert_eq!(p.log, [r#"GHOSTTY_NEW ["window", "", "/r/app", "", "front", ""]"#, r#"/usr/bin/open ["-na", "Ghostty.app", "--args", "--working-directory=/r/app"] (10s)"#]);

        // That failing too, the script's own failure is the one said.
        let p = played(|s| Err(if s.what == "GHOSTTY_NEW" { "boom" } else { "no such app" }.into()), || terminal_open("ghostty", window(), Path::new("/r/app"), None));
        assert_eq!(err(p.got), "Ghostty didn't respond: boom");
        assert_eq!(p.log.len(), 2);

        // With something to type there's no other way.
        let p = played(|_| Err("boom".into()), || terminal_open("ghostty", window(), Path::new("/r/app"), Some("claude --resume x\n")));
        assert_eq!(err(p.got), "Ghostty didn't respond: boom");
        assert_eq!(p.log, [r#"GHOSTTY_NEW ["window", "", "/r/app", "claude --resume x\n", "front", ""]"#]);
        let p = played(|_| Ok("ok\n".into()), || terminal_open("ghostty", Placement { new_terminal: NewTerminal::Tab, near: &[], behind: false, label: None }, Path::new("/r/app"), Some("claude --resume x\n")));
        p.got.unwrap();
        assert_eq!(p.log, [r#"GHOSTTY_NEW ["front", "", "/r/app", "claude --resume x\n", "front", ""]"#]);
    }

    #[test]
    fn iterm_is_sent_a_launcher_then_the_line_once_its_shell_is_ready() {
        let env = [("CODEALONG_PROMPT".to_string(), "Fix the 'flaky' test".to_string())];
        let near = [vec!["/r/other".to_string()]];
        let answer = |s: &Sent| {
            Ok(match s.what.as_str() {
                "ITERM_LIST" => ITERM_TABS.to_string(),
                _ => "/dev/ttys210\n".to_string(),
            })
        };
        let p = played(answer, || terminal_run("iterm2", Placement { new_terminal: NewTerminal::Project, near: &near, behind: false, label: None }, Path::new("/r/my app"), "claude2 --session-id x\n", &env));
        p.got.unwrap();
        assert_eq!(
            p.log,
            [
                "ITERM_LIST []",
                r#"ITERM_NEW ["beside", "/dev/ttys202", "/bin/sh 'LAUNCHER'", "front"]"#,
                "wait for shell /dev/ttys210 after LAUNCHER",
                r#"ITERM_TYPE_LINE+SESSION_ON ["/dev/ttys210", "\u{15}claude2 --session-id x"]"#,
            ]
        );
        assert_eq!(p.launchers, ["rm -f 'LAUNCHER'\ncd '/r/my app' 2>/dev/null\nexport CODEALONG_PROMPT='Fix the '\\''flaky'\\'' test'\nexec SHELL -l"]);

        // A window, a tab in the front window, and nothing to type.
        let p = played(answer, || terminal_run("iterm2", window(), Path::new("/r/app"), "ls\n", &[]));
        p.got.unwrap();
        assert_eq!(p.log[0], r#"ITERM_NEW ["window", "", "/bin/sh 'LAUNCHER'", "front"]"#);
        assert_eq!(p.log.len(), 3);
        let p = played(answer, || terminal_open("iterm2", Placement { new_terminal: NewTerminal::Tab, near: &[], behind: false, label: None }, Path::new("/r/app"), Some("ls\n")));
        p.got.unwrap();
        assert_eq!(p.log, [r#"ITERM_NEW ["front", "", "/bin/sh 'LAUNCHER'", "front"]"#, "wait for shell /dev/ttys210 after LAUNCHER", r#"ITERM_TYPE_LINE+SESSION_ON ["/dev/ttys210", "\u{15}ls"]"#]);
        let p = played(answer, || terminal_open("iterm2", window(), Path::new("/r/app"), None));
        p.got.unwrap();
        assert_eq!(p.log, [r#"ITERM_NEW ["window", "", "/bin/sh 'LAUNCHER'", "front"]"#]);
        let p = played(answer, || terminal_open("iterm2", Placement { behind: true, ..window() }, Path::new("/r/app"), None));
        p.got.unwrap();
        assert_eq!(p.log, [r#"ITERM_NEW ["window", "", "/bin/sh 'LAUNCHER'", "behind"]"#], "a Background start doesn't bring iTerm2 forward");
        assert_eq!(p.launchers, ["rm -f 'LAUNCHER'\ncd '/r/app' 2>/dev/null\nexec SHELL -l"]);

        // What goes wrong, and the launcher (which can hold a prompt) taken away with it.
        let p = played(|_| Err(NOT_ALLOWED.into()), || terminal_run("iterm2", window(), Path::new("/r/app"), "ls\n", &[]));
        assert_eq!(err(p.got), "Codealong isn't allowed to control iTerm2. Turn it on in System Settings → Privacy & Security → Automation.");
        assert!(p.launchers.is_empty(), "the launcher is gone");
        let p = play(Setup { shell_ready: false, ..setup() }, answer, || terminal_run("iterm2", window(), Path::new("/r/app"), "ls\n", &[]));
        assert_eq!(err(p.got), "iTerm2's new tab didn't start its shell within 15 seconds, so nothing was typed there.");
        assert_eq!(p.log.len(), 2);
        assert!(p.launchers.is_empty(), "the launcher is gone");
        let p = played(|s| Ok(if s.what == "ITERM_NEW" { "/dev/ttys210\n".into() } else { String::new() }), || terminal_run("iterm2", window(), Path::new("/r/app"), "ls\n", &[]));
        assert_eq!(err(p.got), "iTerm2's new tab closed before Codealong could type into it.");
    }

    #[test]
    fn terminal_is_sent_a_window_then_the_launcher_and_the_line_as_typing() {
        let env = [("CODEALONG_PROMPT".to_string(), "Fix it".to_string())];
        let near = [vec!["/r/app".to_string()]];
        let answer = |_: &Sent| Ok("/dev/ttys310\n".to_string());
        // Terminal can't open a tab from a script: wherever it's meant to go, a window.
        let p = played(answer, || terminal_run("terminal", Placement { new_terminal: NewTerminal::Project, near: &near, behind: false, label: None }, Path::new("/r/my app"), "codex -- \"$CODEALONG_PROMPT\"\n", &env));
        p.got.unwrap();
        assert_eq!(
            p.log,
            [
                r#"TERMINAL_NEW ["front"]"#,
                "wait for shell /dev/ttys310",
                r#"TERMINAL_TYPE+TAB_ON ["/dev/ttys310", "\u{15} exec /bin/sh 'LAUNCHER'"]"#,
                "wait for shell /dev/ttys310 after LAUNCHER",
                r#"TERMINAL_TYPE+TAB_ON ["/dev/ttys310", "\u{15}codex -- \"$CODEALONG_PROMPT\""]"#,
            ]
        );
        assert_eq!(p.launchers, ["rm -f 'LAUNCHER'\ncd '/r/my app' 2>/dev/null\nexport CODEALONG_PROMPT='Fix it'\nexec SHELL -l"]);

        let p = played(answer, || terminal_open("terminal", Placement { behind: true, ..window() }, Path::new("/r/app"), None));
        p.got.unwrap();
        assert_eq!(p.log[0], r#"TERMINAL_NEW ["behind"]"#, "a Background start doesn't bring Terminal forward");
        let p = played(answer, || terminal_open("terminal", window(), Path::new("/r/app"), None));
        p.got.unwrap();
        assert_eq!(p.log, [r#"TERMINAL_NEW ["front"]"#, "wait for shell /dev/ttys310", r#"TERMINAL_TYPE+TAB_ON ["/dev/ttys310", "\u{15} exec /bin/sh 'LAUNCHER'"]"#, "wait for shell /dev/ttys310 after LAUNCHER"]);
        let p = played(answer, || terminal_open("terminal", Placement { new_terminal: NewTerminal::Tab, near: &[], behind: false, label: None }, Path::new("/r/app"), Some("ls\n")));
        p.got.unwrap();
        assert_eq!(p.log.len(), 5);
        assert_eq!(p.log[4], r#"TERMINAL_TYPE+TAB_ON ["/dev/ttys310", "\u{15}ls"]"#);

        let p = played(|_| Err(NOT_ALLOWED.into()), || terminal_run("terminal", window(), Path::new("/r/app"), "ls\n", &[]));
        assert_eq!(err(p.got), "Codealong isn't allowed to control Terminal. Turn it on in System Settings → Privacy & Security → Automation.");
        assert!(p.launchers.is_empty(), "the launcher is gone");
        let p = play(Setup { shell_ready: false, ..setup() }, answer, || terminal_run("terminal", window(), Path::new("/r/app"), "ls\n", &[]));
        assert_eq!(err(p.got), "Terminal's new window didn't run Codealong's setup within 15 seconds, so nothing was typed there.");
        assert_eq!(p.log.len(), 4);
        let p = played(|s| Ok(if s.what == "TERMINAL_NEW" { "/dev/ttys310\n".into() } else { String::new() }), || terminal_run("terminal", window(), Path::new("/r/app"), "ls\n", &[]));
        assert_eq!(err(p.got), "Terminal's new window closed before Codealong could type into it.");
        assert_eq!(p.log.len(), 3);
    }

    #[test]
    fn a_profile_that_is_unknown_or_not_installed_is_said_before_anything_is_sent() {
        let p = played(|_| Ok(String::new()), || (terminal_run("kitty", window(), Path::new("/r/app"), "ls\n", &[]), terminal_open("tmux", window(), Path::new("/r/app"), None)));
        assert_eq!(err(p.got.0), "There's no terminal profile called \"kitty\". Use \"ghostty\", \"iterm2\" or \"terminal\" in config.toml.");
        assert_eq!(err(p.got.1), "There's no terminal profile called \"tmux\". Use \"ghostty\", \"iterm2\" or \"terminal\" in config.toml.");
        assert!(p.log.is_empty());
        let p = play(Setup { installed: false, ..setup() }, |_| Ok("/dev/ttys310\n".into()), || {
            (terminal_run("ghostty", window(), Path::new("/r/app"), "ls\n", &[]), terminal_open("iterm2", window(), Path::new("/r/app"), None), terminal_open("terminal", window(), Path::new("/r/app"), None))
        });
        assert_eq!(err(p.got.0), "Ghostty isn't installed. Install it, or pick another terminal in Settings.");
        assert_eq!(err(p.got.1), "iTerm2 isn't installed. Install it, or pick another terminal in Settings.");
        p.got.2.unwrap();
        assert_eq!(p.log[0], r#"TERMINAL_NEW ["front"]"#, "Terminal comes with macOS");
        let p = play(Setup { installed: false, ..setup() }, |_| Ok(String::new()), || (crate::config::terminal_profiles(), default_terminal()));
        assert_eq!(p.got.0.iter().map(|t| (t.id.as_str(), t.name.as_str())).collect::<Vec<_>>(), [("terminal", "Terminal")]);
        assert_eq!(p.got.1, "terminal");
        let p = played(|_| Ok(String::new()), || (crate::config::terminal_profiles(), default_terminal()));
        assert_eq!(p.got.0.iter().map(|t| (t.id.as_str(), t.name.as_str())).collect::<Vec<_>>(), [("ghostty", "Ghostty"), ("iterm2", "iTerm2"), ("terminal", "Terminal")]);
        assert_eq!(p.got.1, "ghostty");
    }

    #[test]
    fn a_session_is_known_by_the_terminal_its_process_runs_under() {
        let p = played(|_| Ok(String::new()), || {
            [IN_GHOSTTY, IN_ITERM, IN_TERMINAL, IN_TMUX, IN_BACKGROUND, IN_KITTY].map(|pid| terminal_kind(pid).map(|k| (k.name(), k.bundle(), k.presses_keys())))
        });
        assert_eq!(
            p.got,
            [
                Some(("Ghostty", Some("com.mitchellh.ghostty"), true)),
                Some(("iTerm2", Some("com.googlecode.iterm2"), true)),
                Some(("Terminal", Some("com.apple.Terminal"), false)),
                Some(("tmux", None, true)),
                None,
                None,
            ]
        );
        assert!(p.log.is_empty(), "nothing is asked of any app");
        assert_eq!(presses_keys_in(), "Ghostty, iTerm2 or tmux");
    }

    #[test]
    fn focusing_a_ghostty_session_marks_its_tty_and_focuses_the_terminal_showing_the_mark() {
        let found = |s: &Sent| {
            Ok(match (s.what.as_str(), s.args.first().map(String::as_str)) {
                ("GHOSTTY_LIST", _) => GHOSTTY_TABS.to_string(),
                ("GHOSTTY_FOCUS", Some("tag")) => "T1\n".to_string(),
                _ => panic!("unexpected {} {:?}", s.what, s.args),
            })
        };
        let p = played(found, || terminal_focus_session("iterm2", IN_GHOSTTY, "Fix the login", Path::new("/r/elsewhere")));
        assert!(p.got.unwrap());
        assert_eq!(
            p.log,
            [
                "GHOSTTY_LIST []",
                "osc7 /dev/ttys101 /tmp/codealong-100010-NOW",
                r#"GHOSTTY_FOCUS ["tag", "/tmp/codealong-100010-NOW", "8"]"#,
                // The folder the terminal showed before goes back, not the session's.
                "osc7 /dev/ttys101 /r/app",
            ]
        );

        // The mark never shows: the tab is picked by title, then by folder, and focused by id.
        let unmarked = |s: &Sent| {
            Ok(match (s.what.as_str(), s.args.first().map(String::as_str)) {
                ("GHOSTTY_LIST", _) => GHOSTTY_TABS.to_string(),
                ("GHOSTTY_FOCUS", Some("tag")) => "\n".to_string(),
                ("GHOSTTY_FOCUS", Some("id")) => format!("{}\n", s.args[1]),
                _ => panic!("unexpected {} {:?}", s.what, s.args),
            })
        };
        let p = played(unmarked, || terminal_focus_session("ghostty", IN_GHOSTTY, "Bump dependencies", Path::new("/r/elsewhere")));
        assert!(p.got.unwrap());
        assert_eq!(
            p.log,
            [
                "GHOSTTY_LIST []",
                "osc7 /dev/ttys101 /tmp/codealong-100010-NOW",
                r#"GHOSTTY_FOCUS ["tag", "/tmp/codealong-100010-NOW", "8"]"#,
                "osc7 /dev/ttys101 /r/elsewhere",
                r#"GHOSTTY_FOCUS ["id", "T2"]"#,
            ]
        );
        // No tab is the only one titled or placed like it: not found, and no guess.
        let p = played(unmarked, || terminal_focus_session("ghostty", IN_GHOSTTY, "Something else", Path::new("/r/app")));
        assert!(!p.got.unwrap());
        assert_eq!(p.log.len(), 4);

        // Ghostty too slow to show the mark: said, unless the title finds the tab.
        let slow = |s: &Sent| match (s.what.as_str(), s.args.first().map(String::as_str)) {
            ("GHOSTTY_LIST", _) => Ok(GHOSTTY_TABS.to_string()),
            ("GHOSTTY_FOCUS", Some("tag")) => Err("/usr/bin/osascript timed out".to_string()),
            ("GHOSTTY_FOCUS", Some("id")) => Ok(format!("{}\n", s.args[1])),
            _ => panic!("unexpected {} {:?}", s.what, s.args),
        };
        let p = played(slow, || terminal_focus_session("ghostty", IN_GHOSTTY, "Something else", Path::new("/r/app")));
        assert_eq!(err(p.got), "Ghostty didn't answer within a minute. It may still open the window; if macOS asked whether Codealong may control Ghostty, allow it and try again.");
        let p = played(slow, || terminal_focus_session("ghostty", IN_GHOSTTY, "Fix the login", Path::new("/r/app")));
        assert!(p.got.unwrap());
        assert_eq!(p.log[3..], ["osc7 /dev/ttys101 /r/app", r#"GHOSTTY_FOCUS ["id", "T1"]"#]);
        // Ghostty not listing its terminals is the end of it.
        let p = played(|_| Err("boom".into()), || terminal_focus_session("ghostty", IN_GHOSTTY, "Fix the login", Path::new("/r/app")));
        assert_eq!(err(p.got), "Ghostty didn't respond: boom");
        assert_eq!(p.log, ["GHOSTTY_LIST []"]);

        // A mark left over as a terminal's folder isn't put back: the session's folder is.
        let leftover = |s: &Sent| {
            Ok(match s.what.as_str() {
                "GHOSTTY_LIST" => "T1\t/tmp/codealong-7-7\t✳ Fix the login\n".to_string(),
                _ => "T1\n".to_string(),
            })
        };
        let p = played(leftover, || terminal_focus_session("ghostty", IN_GHOSTTY, "Fix the login", Path::new("/r/app")));
        assert!(p.got.unwrap());
        assert_eq!(p.log[3], "osc7 /dev/ttys101 /r/app");
    }

    #[test]
    fn focusing_a_session_in_iterm_or_terminal_goes_by_its_tty() {
        let there = |s: &Sent| Ok(format!("{}\n", s.args[0]));
        let p = played(there, || terminal_focus_session("ghostty", IN_ITERM, "Fix the login", Path::new("/r/app")));
        assert!(p.got.unwrap());
        assert_eq!(p.log, [r#"ITERM_FOCUS+SESSION_ON ["/dev/ttys201"]"#]);
        let p = played(there, || terminal_focus_session("ghostty", IN_TERMINAL, "Fix the login", Path::new("/r/app")));
        assert!(p.got.unwrap());
        assert_eq!(p.log, [r#"TERMINAL_FOCUS+TAB_ON ["/dev/ttys301"]"#]);

        // No tab on that TTY.
        let p = played(|_| Ok("\n".into()), || (terminal_focus_session("ghostty", IN_ITERM, "", Path::new("/r/app")), terminal_focus_session("ghostty", IN_TERMINAL, "", Path::new("/r/app"))));
        assert!(!p.got.0.unwrap());
        assert!(!p.got.1.unwrap());
        assert_eq!(p.log.len(), 2);
        let p = played(|_| Err(NOT_ALLOWED.into()), || (terminal_focus_session("ghostty", IN_ITERM, "", Path::new("/r/app")), terminal_focus_session("ghostty", IN_TERMINAL, "", Path::new("/r/app"))));
        assert_eq!(err(p.got.0), "Codealong isn't allowed to control iTerm2. Turn it on in System Settings → Privacy & Security → Automation.");
        assert_eq!(err(p.got.1), "Codealong isn't allowed to control Terminal. Turn it on in System Settings → Privacy & Security → Automation.");
    }

    #[test]
    fn focusing_a_session_in_an_unknown_terminal_goes_by_title_in_the_profiles_app() {
        let answer = |s: &Sent| {
            Ok(match s.what.as_str() {
                "GHOSTTY_LIST" => GHOSTTY_TABS.to_string(),
                "ITERM_LIST" => ITERM_TABS.to_string(),
                "GHOSTTY_FOCUS" => format!("{}\n", s.args[1]),
                "ITERM_FOCUS+SESSION_ON" => format!("{}\n", s.args[0]),
                _ => panic!("unexpected {} {:?}", s.what, s.args),
            })
        };
        let p = played(answer, || terminal_focus_session("ghostty", IN_KITTY, "Bump dependencies", Path::new("/r/elsewhere")));
        assert!(p.got.unwrap());
        assert_eq!(p.log, ["GHOSTTY_LIST []", r#"GHOSTTY_FOCUS ["id", "T2"]"#]);
        let p = played(answer, || terminal_focus_session("ghostty", IN_KITTY, "Something else", Path::new("/r/app")));
        assert!(!p.got.unwrap());
        assert_eq!(p.log, ["GHOSTTY_LIST []"]);
        let p = played(answer, || terminal_focus_session("iterm2", IN_KITTY, "Something else", Path::new("/r/other")));
        assert!(p.got.unwrap());
        assert_eq!(p.log, ["ITERM_LIST []", r#"ITERM_FOCUS+SESSION_ON ["/dev/ttys202"]"#]);
        // Terminal lists no titles or folders, and an unknown profile has no app to ask.
        let p = played(answer, || (terminal_focus_session("terminal", IN_KITTY, "Fix the login", Path::new("/r/app")), terminal_focus_session("kitty", IN_KITTY, "Fix the login", Path::new("/r/app"))));
        assert!(!p.got.0.unwrap());
        assert!(!p.got.1.unwrap());
        assert!(p.log.is_empty());
        // The tab gone between the list and the focus.
        let p = played(|s| Ok(if s.what == "GHOSTTY_LIST" { GHOSTTY_TABS.to_string() } else { "\n".into() }), || terminal_focus_session("ghostty", IN_KITTY, "Bump dependencies", Path::new("/r/elsewhere")));
        assert!(!p.got.unwrap());
        let p = played(answer, || terminal_focus_session("ghostty", IN_BACKGROUND, "Fix the login", Path::new("/r/app")));
        assert_eq!(err(p.got), "This session runs in the background, so it has no terminal tab.");
        assert!(p.log.is_empty());
    }

    const TMUX_PANES: &str = r##"/opt/homebrew/bin/tmux ["list-panes", "-a", "-F", "#{pane_tty} #{pane_id} #{window_id} #{session_attached} #{session_name}"] (5s)"##;
    const TMUX_CLIENTS: &str = r##"/opt/homebrew/bin/tmux ["list-clients", "-F", "#{client_activity} #{client_pid} #{client_tty} #{client_session}"] (5s)"##;
    /// Pane %3 brought up in the session "my agents" shows.
    const TMUX_SHOW: [&str; 2] = [r#"/opt/homebrew/bin/tmux ["select-window", "-t", "=my agents:@2"] (5s)"#, r#"/opt/homebrew/bin/tmux ["select-pane", "-t", "%3"] (5s)"#];

    /// tmux answering with `panes` and `clients`, and Ghostty with its tabs, T3 showing a mark.
    fn tmux_answers(panes: &'static str, clients: &'static str) -> impl Fn(&Sent) -> Result<String, String> {
        move |s: &Sent| {
            Ok(match (s.what.as_str(), s.args.first().map(String::as_str)) {
                ("/opt/homebrew/bin/tmux", Some("list-panes")) => panes.to_string(),
                ("/opt/homebrew/bin/tmux", Some("list-clients")) => clients.to_string(),
                ("/opt/homebrew/bin/tmux", _) => String::new(),
                ("GHOSTTY_LIST", _) => GHOSTTY_TABS.to_string(),
                ("GHOSTTY_FOCUS", Some("tag")) => "T3\n".to_string(),
                ("GHOSTTY_FOCUS", _) => format!("{}\n", s.args[1]),
                ("ITERM_FOCUS+SESSION_ON", _) => format!("{}\n", s.args[0]),
                _ => panic!("unexpected {} {:?}", s.what, s.args),
            })
        }
    }

    #[test]
    fn focusing_a_session_in_tmux_brings_up_its_pane_in_the_tab_its_client_runs_in() {
        // Two clients show its session and one another session: the one used last is the one in
        // Ghostty, whose tab is found by a mark on the client's TTY.
        let panes = "/dev/ttys400 %1 @1 1 other\n/dev/ttys401 %3 @2 2 my agents\n";
        let clients = "1760000300 100030 /dev/ttys103 my agents\n1760000400 600030 /dev/ttys602 other\n1760000100 400030 /dev/ttys402 my agents\n";
        let p = played(tmux_answers(panes, clients), || terminal_focus_session("iterm2", IN_TMUX, "Bump dependencies", Path::new("/r/elsewhere")));
        assert!(p.got.unwrap());
        assert_eq!(
            p.log,
            [
                TMUX_PANES,
                TMUX_CLIENTS,
                TMUX_SHOW[0],
                TMUX_SHOW[1],
                "GHOSTTY_LIST []",
                "osc7 /dev/ttys103 /tmp/codealong-100030-NOW",
                r#"GHOSTTY_FOCUS ["tag", "/tmp/codealong-100030-NOW", "8"]"#,
                "osc7 /dev/ttys103 /r/app",
            ]
        );

        // The client used last is in a terminal Codealong doesn't know, or in tmux again: its
        // pane is brought up, and the tab is the one titled like the session in the profile's app.
        for clients in ["1760000300 600030 /dev/ttys602 my agents\n1760000100 100030 /dev/ttys103 my agents\n", "1760000300 400030 /dev/ttys402 my agents\n"] {
            let p = played(tmux_answers(panes, clients), || terminal_focus_session("ghostty", IN_TMUX, "Bump dependencies", Path::new("/r/elsewhere")));
            assert!(p.got.unwrap());
            assert_eq!(p.log, [TMUX_PANES, TMUX_CLIENTS, TMUX_SHOW[0], TMUX_SHOW[1], "GHOSTTY_LIST []", r#"GHOSTTY_FOCUS ["id", "T2"]"#]);
        }

        // Its window is in two sessions (a group's), listed once in each: the one listed first
        // isn't attached, the other is, by a client in iTerm2, whatever config.toml names. The
        // window comes up in the client's session: the pane alone would name the session with
        // it that was active last, which can be the one nobody sees.
        let grouped = "/dev/ttys401 %3 @2 0 my agents\n/dev/ttys401 %3 @2 1 view\n";
        let p = played(tmux_answers(grouped, "1760000300 200030 /dev/ttys203 view\n"), || terminal_focus_session("ghostty", IN_TMUX, "Bump dependencies", Path::new("/r/elsewhere")));
        assert!(p.got.unwrap());
        assert_eq!(
            p.log,
            [
                TMUX_PANES,
                TMUX_CLIENTS,
                r#"/opt/homebrew/bin/tmux ["select-window", "-t", "=view:@2"] (5s)"#,
                TMUX_SHOW[1],
                r#"ITERM_FOCUS+SESSION_ON ["/dev/ttys203"]"#,
            ]
        );

        // No client shows its session, so no tab can: none is asked for, and the title finds
        // only a tab that happens to be titled like it.
        let p = played(tmux_answers("/dev/ttys401 %3 @2 0 my agents\n", ""), || terminal_focus_session("ghostty", IN_TMUX, "Something else", Path::new("/r/app")));
        assert!(!p.got.unwrap());
        assert_eq!(p.log, [TMUX_PANES, "GHOSTTY_LIST []"]);
        // Its pane isn't on the server Codealong asks.
        let p = played(tmux_answers("/dev/ttys400 %1 @1 1 other\n", ""), || terminal_focus_session("ghostty", IN_TMUX, "Bump dependencies", Path::new("/r/elsewhere")));
        assert!(p.got.unwrap());
        assert_eq!(p.log, [TMUX_PANES, "GHOSTTY_LIST []", r#"GHOSTTY_FOCUS ["id", "T2"]"#]);
    }

    #[test]
    fn a_tmux_session_no_client_shows_is_attached_to_in_a_new_tab() {
        // By its pane, which brings up the pane's window and the pane, not the session's last.
        let p = played(tmux_answers("/dev/ttys400 %1 @1 1 other\n/dev/ttys401 %3 @2 0 Bob's agents\n/dev/ttys401 %3 @2 0 view\n", ""), || attach_command(IN_TMUX));
        assert_eq!(p.got.as_deref(), Some("/opt/homebrew/bin/tmux attach -t '%3'"));
        assert_eq!(p.log, [TMUX_PANES]);
        // A client shows a session it's in, or its pane isn't found: it may have a tab.
        for panes in ["/dev/ttys401 %3 @2 0 Bob's agents\n/dev/ttys401 %3 @2 1 view\n", "/dev/ttys400 %1 @1 0 other\n"] {
            let p = played(tmux_answers(panes, ""), || attach_command(IN_TMUX));
            assert_eq!(p.got, None);
        }
        // An app's sessions are its tabs, and tmux isn't asked about them.
        let p = played(tmux_answers("", ""), || (attach_command(IN_GHOSTTY), attach_command(IN_ITERM), attach_command(IN_KITTY), attach_command(IN_BACKGROUND)));
        assert_eq!(p.got, (None, None, None, None));
        assert!(p.log.is_empty());
    }

    #[test]
    fn a_tmux_sessions_app_is_the_one_its_client_runs_in() {
        let panes = "/dev/ttys401 %3 @2 1 my agents\n";
        for (client, app) in [("100030 /dev/ttys103", Some("com.mitchellh.ghostty")), ("200030 /dev/ttys203", Some("com.googlecode.iterm2")), ("600030 /dev/ttys602", None), ("400030 /dev/ttys402", None)] {
            let clients: &'static str = Box::leak(format!("1760000300 {client} my agents\n").into_boxed_str());
            let p = played(tmux_answers(panes, clients), || session_app(IN_TMUX));
            assert_eq!(p.got, app, "client {client}");
            assert_eq!(p.log, [TMUX_PANES, TMUX_CLIENTS]);
        }
        // No client shows it: a new tab opens in the profile's app, which the caller falls back on.
        let p = played(tmux_answers("/dev/ttys401 %3 @2 0 my agents\n", ""), || session_app(IN_TMUX));
        assert_eq!(p.got, None);
        assert_eq!(p.log, [TMUX_PANES]);
        // A terminal's own sessions are in its app, and tmux isn't asked.
        let p = played(tmux_answers("", ""), || (session_app(IN_GHOSTTY), session_app(IN_ITERM), session_app(IN_TERMINAL), session_app(IN_KITTY)));
        assert_eq!(p.got, (Some("com.mitchellh.ghostty"), Some("com.googlecode.iterm2"), Some("com.apple.Terminal"), None));
        assert!(p.log.is_empty());
    }

    #[test]
    fn a_tab_raised_again_is_found_the_way_its_app_finds_it() {
        let p = played(|_| Ok("x\n".into()), || {
            for (profile, at) in [("ghostty", "T2"), ("iterm2", "/dev/ttys201"), ("terminal", "/dev/ttys301")] {
                TerminalHost { terminal: terminal_app(profile).unwrap(), at: at.into() }.again();
            }
        });
        assert_eq!(p.log, [r#"GHOSTTY_FOCUS ["id", "T2"]"#, r#"ITERM_FOCUS+SESSION_ON ["/dev/ttys201"]"#, r#"TERMINAL_FOCUS+TAB_ON ["/dev/ttys301"]"#]);
    }

    #[test]
    fn typing_into_a_ghostty_session_finds_its_terminal_by_a_mark_and_pastes_there() {
        let answer = |s: &Sent| {
            Ok(match s.what.as_str() {
                "GHOSTTY_LIST" => GHOSTTY_TABS.to_string(),
                "GHOSTTY_FIND" => "T1\n".to_string(),
                "GHOSTTY_TYPE" => "T1\n".to_string(),
                _ => panic!("unexpected {} {:?}", s.what, s.args),
            })
        };
        let p = played(answer, || terminal_type_session(IN_GHOSTTY, Path::new("/r/elsewhere"), "Fix it\nand \u{1b}that", false, Submit::Return, false));
        assert_eq!(p.got.unwrap(), Typed::Into("Ghostty"));
        assert_eq!(
            p.log,
            [
                "GHOSTTY_LIST []",
                "osc7 /dev/ttys101 /tmp/codealong-100010-NOW",
                r#"GHOSTTY_FIND ["/tmp/codealong-100010-NOW", "8"]"#,
                "osc7 /dev/ttys101 /r/app",
                r#"GHOSTTY_TYPE ["T1", "Fix it\nand \u{1b}that", "", ""]"#,
            ]
        );
        // Clearing the box first is Ghostty's Ctrl+C action, in the same script.
        let p = played(answer, || terminal_type_session(IN_GHOSTTY, Path::new("/r/app"), "Fix it", true, Submit::Return, false));
        assert_eq!(p.got.unwrap(), Typed::Into("Ghostty"));
        assert_eq!(p.log[4], r#"GHOSTTY_TYPE ["T1", "Fix it", "text:\\x03", ""]"#);
        assert_eq!(p.log.len(), 5);
        // Sent with keys in place of Return: Ghostty's text action types them.
        let p = played(answer, || terminal_type_session(IN_GHOSTTY, Path::new("/r/app"), "Fix it", false, Submit::Keys("\x18\x13"), false));
        assert_eq!(p.got.unwrap(), Typed::Into("Ghostty"));
        assert_eq!(p.log[4], r#"GHOSTTY_TYPE ["T1", "Fix it", "", "text:\\x18\\x13"]"#);
        // With another way in after it, finding the tab gets `FIND_WAIT` in all; without one,
        // each question gets its minute, as typing always does once the tab is found.
        let given = |passes: bool| {
            let given = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
            let noted = given.clone();
            played(
                move |s: &Sent| {
                    noted.lock().unwrap().push((s.what.clone(), s.timeout));
                    answer(s)
                },
                move || terminal_type_session(IN_GHOSTTY, Path::new("/r/app"), "Fix it", false, Submit::Return, passes),
            );
            let given = given.lock().unwrap().clone();
            assert_eq!(given.iter().map(|(what, _)| what.as_str()).collect::<Vec<_>>(), ["GHOSTTY_LIST", "GHOSTTY_FIND", "GHOSTTY_TYPE"]);
            given.into_iter().map(|(_, t)| t).collect::<Vec<_>>()
        };
        let minute = std::time::Duration::from_secs(60);
        let hurried = given(true);
        assert!(hurried[..2].iter().all(|t| *t <= super::super::ghostty::FIND_WAIT), "{hurried:?}");
        assert_eq!(hurried[2], minute);
        assert_eq!(given(false), [minute; 3]);

        // Nothing typed, and why: another way can take it.
        let nowhere = |find: Result<&'static str, &'static str>, typed: Result<&'static str, &'static str>| {
            played(
                move |s: &Sent| {
                    let r = match s.what.as_str() {
                        "GHOSTTY_LIST" => Ok(GHOSTTY_TABS),
                        "GHOSTTY_FIND" => find,
                        _ => typed,
                    };
                    r.map(String::from).map_err(String::from)
                },
                || terminal_type_session(IN_GHOSTTY, Path::new("/r/elsewhere"), "Fix it", false, Submit::Return, false),
            )
        };
        let p = nowhere(Ok("\n"), Ok("T1\n"));
        assert_eq!(p.got.unwrap(), Typed::Nowhere("Codealong couldn't find its Ghostty tab".into()));
        assert_eq!(p.log[3], "osc7 /dev/ttys101 /r/elsewhere");
        assert_eq!(p.log.len(), 4);
        let p = nowhere(Err(NOT_ALLOWED), Ok("T1\n"));
        assert_eq!(p.got.unwrap(), Typed::Nowhere("Codealong isn't allowed to control Ghostty (System Settings → Privacy & Security → Automation)".into()));
        assert_eq!(p.log.len(), 4, "the folder is put back all the same");
        let p = nowhere(Err("/usr/bin/osascript timed out"), Ok("T1\n"));
        assert_eq!(p.got.unwrap(), Typed::Nowhere("Ghostty didn't answer in time".into()));
        let p = nowhere(Ok("T1\n"), Ok("\n"));
        assert_eq!(p.got.unwrap(), Typed::Nowhere("its Ghostty tab closed".into()));
        // Once typing started, a failure is a failure: the text may be there.
        let p = nowhere(Ok("T1\n"), Err(NOT_ALLOWED));
        assert_eq!(err(p.got), "Codealong isn't allowed to control Ghostty. Turn it on in System Settings → Privacy & Security → Automation.");
        let p = nowhere(Ok("T1\n"), Err("boom"));
        assert_eq!(err(p.got), "Ghostty didn't respond: boom");
        let p = played(|_| Err("boom".into()), || terminal_type_session(IN_GHOSTTY, Path::new("/r/app"), "Fix it", false, Submit::Return, false));
        assert_eq!(p.got.unwrap(), Typed::Nowhere("Ghostty didn't answer in time".into()));
        assert_eq!(p.log, ["GHOSTTY_LIST []"]);

        // Where macOS naps Ghostty, the likeliest reason it was slow, it says how to stop that.
        let napping = |find: Result<&'static str, &'static str>| {
            let answer = move |s: &Sent| if s.what == "GHOSTTY_LIST" { Ok(GHOSTTY_TABS.to_string()) } else { find.map(String::from).map_err(String::from) };
            play(Setup { ghostty_naps: true, ..setup() }, answer, || terminal_type_session(IN_GHOSTTY, Path::new("/r/app"), "Fix it", false, Submit::Return, false)).got.unwrap()
        };
        let stops = "; macOS naps Ghostty behind other windows, which `defaults write com.mitchellh.ghostty NSAppSleepDisabled -bool YES` stops once Ghostty restarts";
        assert_eq!(napping(Ok("\n")), Typed::Nowhere(format!("Codealong couldn't find its Ghostty tab{stops}")));
        assert_eq!(napping(Err("/usr/bin/osascript timed out")), Typed::Nowhere(format!("Ghostty didn't answer in time{stops}")));
        assert_eq!(napping(Err(NOT_ALLOWED)), Typed::Nowhere("Codealong isn't allowed to control Ghostty (System Settings → Privacy & Security → Automation)".into()));
    }

    #[test]
    fn typing_into_a_session_in_iterm_or_terminal_pastes_on_its_tty() {
        let there = |s: &Sent| Ok(if s.what.contains("PRESS") { "ok\n".to_string() } else { format!("{}\n", s.args[0]) });
        let p = played(there, || terminal_type_session(IN_ITERM, Path::new("/r/app"), "Fix it\nand \u{1b}that", false, Submit::Return, false));
        assert_eq!(p.got.unwrap(), Typed::Into("iTerm2"));
        assert_eq!(p.log, [r#"ITERM_TYPE+SESSION_ON ["/dev/ttys201", "\u{1b}[200~Fix it\nand that\u{1b}[201~", ""]"#]);
        // iTerm2 presses Ctrl+C alone first; Terminal can't press a key alone, so it only types.
        let p = played(there, || terminal_type_session(IN_ITERM, Path::new("/r/app"), "Fix it", true, Submit::Return, false));
        assert_eq!(p.got.unwrap(), Typed::Into("iTerm2"));
        assert_eq!(p.log, [r#"ITERM_PRESS+SESSION_ON ["/dev/ttys201", "\u{3}"]"#, r#"ITERM_TYPE+SESSION_ON ["/dev/ttys201", "\u{1b}[200~Fix it\u{1b}[201~", ""]"#]);
        // Sent with keys in place of Return: iTerm2 types them after the paste. Terminal can't
        // press keys alone, so nothing is typed there.
        let p = played(there, || terminal_type_session(IN_ITERM, Path::new("/r/app"), "Fix it", false, Submit::Keys("\x18\x13"), false));
        assert_eq!(p.got.unwrap(), Typed::Into("iTerm2"));
        assert_eq!(p.log, [r#"ITERM_TYPE+SESSION_ON ["/dev/ttys201", "\u{1b}[200~Fix it\u{1b}[201~", "\u{18}\u{13}"]"#]);
        let p = played(there, || terminal_type_session(IN_TERMINAL, Path::new("/r/app"), "Fix it", false, Submit::Keys("\x18\x13"), false));
        assert_eq!(p.got.unwrap(), Typed::Nowhere("Terminal can't press the keys that send it now".into()));
        assert!(p.log.is_empty());
        for clear in [false, true] {
            let p = played(there, move || terminal_type_session(IN_TERMINAL, Path::new("/r/app"), "Fix it\nand \u{1b}that", clear, Submit::Return, false));
            assert_eq!(p.got.unwrap(), Typed::Into("Terminal"));
            assert_eq!(p.log, [r#"TERMINAL_TYPE+TAB_ON ["/dev/ttys301", "\u{1b}[200~Fix it\nand that\u{1b}[201~"]"#]);
        }

        // No tab on the TTY, or macOS refusing the script: nothing went in, and why.
        let p = played(|_| Ok("\n".into()), || (terminal_type_session(IN_ITERM, Path::new("/r/app"), "Fix it", false, Submit::Return, false), terminal_type_session(IN_TERMINAL, Path::new("/r/app"), "Fix it", false, Submit::Return, false)));
        assert_eq!(p.got.0.unwrap(), Typed::Nowhere("Codealong couldn't find its iTerm2 tab".into()));
        assert_eq!(p.got.1.unwrap(), Typed::Nowhere("Codealong couldn't find its Terminal tab".into()));
        let p = played(|_| Err(NOT_ALLOWED.into()), || (terminal_type_session(IN_ITERM, Path::new("/r/app"), "Fix it", false, Submit::Return, false), terminal_type_session(IN_TERMINAL, Path::new("/r/app"), "Fix it", false, Submit::Return, false)));
        assert_eq!(p.got.0.unwrap(), Typed::Nowhere("Codealong isn't allowed to control iTerm2 (System Settings → Privacy & Security → Automation)".into()));
        assert_eq!(p.got.1.unwrap(), Typed::Nowhere("Codealong isn't allowed to control Terminal (System Settings → Privacy & Security → Automation)".into()));
        let p = played(|_| Err("boom".into()), || (terminal_type_session(IN_ITERM, Path::new("/r/app"), "Fix it", false, Submit::Return, false), terminal_type_session(IN_TERMINAL, Path::new("/r/app"), "Fix it", false, Submit::Return, false)));
        assert_eq!(err(p.got.0), "iTerm2 didn't respond: boom");
        assert_eq!(err(p.got.1), "Terminal didn't respond: boom");
        // The key that clears the box failing is a failure, refused or not: nothing more is sent.
        let p = played(|_| Err(NOT_ALLOWED.into()), || terminal_type_session(IN_ITERM, Path::new("/r/app"), "Fix it", true, Submit::Return, false));
        assert_eq!(err(p.got), "Codealong isn't allowed to control iTerm2. Turn it on in System Settings → Privacy & Security → Automation.");
        assert_eq!(p.log.len(), 1);
    }

    #[test]
    fn typing_into_a_session_in_tmux_pastes_into_its_pane() {
        let answer = |s: &Sent| Ok(if s.args[0] == "list-panes" { "/dev/ttys400 %1 @1 1 other\n/dev/ttys401 %3 @2 1 my agents\n".to_string() } else { String::new() });
        let p = played(answer, || terminal_type_session(IN_TMUX, Path::new("/r/app"), "Fix it\nand that", false, Submit::Return, false));
        assert_eq!(p.got.unwrap(), Typed::Into("tmux"));
        assert_eq!(
            p.log,
            [
                TMUX_PANES,
                r#"/opt/homebrew/bin/tmux ["load-buffer", "-b", "codealong", "-"] < "Fix it\nand that" (5s)"#,
                r#"/opt/homebrew/bin/tmux ["paste-buffer", "-p", "-d", "-b", "codealong", "-t", "%3"] (5s)"#,
                r#"/opt/homebrew/bin/tmux ["send-keys", "-t", "%3", "Enter"] (5s)"#,
            ]
        );
        let p = played(answer, || terminal_type_session(IN_TMUX, Path::new("/r/app"), "Fix it", true, Submit::Return, false));
        assert_eq!(p.got.unwrap(), Typed::Into("tmux"));
        assert_eq!(p.log[1], r#"/opt/homebrew/bin/tmux ["send-keys", "-t", "%3", "C-c"] (5s)"#);
        assert_eq!(p.log.len(), 5);
        // Sent with keys in place of Return: each by its code, so a control character goes as
        // the key it is.
        let p = played(answer, || terminal_type_session(IN_TMUX, Path::new("/r/app"), "Fix it", false, Submit::Keys("\x18\x13"), false));
        assert_eq!(p.got.unwrap(), Typed::Into("tmux"));
        assert_eq!(p.log[3], r#"/opt/homebrew/bin/tmux ["send-keys", "-t", "%3", "-H", "18", "13"] (5s)"#);
        assert_eq!(p.log.len(), 4);

        let p = played(|_| Ok("/dev/ttys400 %1 @1 1 other\n".into()), || terminal_type_session(IN_TMUX, Path::new("/r/app"), "Fix it", true, Submit::Return, false));
        assert_eq!(p.got.unwrap(), Typed::Nowhere("Codealong couldn't find its tmux pane".into()));
        assert_eq!(p.log.len(), 1);
        let p = played(|s| if s.args[0] == "list-panes" { Ok("/dev/ttys401 %3 @2 0 my agents\n".into()) } else { Err("no server".into()) }, || terminal_type_session(IN_TMUX, Path::new("/r/app"), "Fix it", false, Submit::Return, false));
        assert_eq!(err(p.got), "no server");

        // In no terminal Codealong types into.
        let p = played(|_| Ok(String::new()), || (terminal_type_session(IN_KITTY, Path::new("/r/app"), "Fix it", false, Submit::Return, false), terminal_type_session(IN_BACKGROUND, Path::new("/r/app"), "Fix it", true, Submit::Return, false)));
        assert_eq!(p.got.0.unwrap(), Typed::Nowhere("it isn't in a Ghostty, iTerm2, Terminal or tmux tab".into()));
        assert_eq!(p.got.1.unwrap(), Typed::Nowhere("it isn't in a Ghostty, iTerm2, Terminal or tmux tab".into()));
        assert!(p.log.is_empty());
    }

    /// Finds the session's terminal and presses each key there, as answering a prompt and
    /// stopping a turn do.
    fn press_all(pid: u32) -> anyhow::Result<()> {
        let host = terminal_host(pid, Path::new("/r/elsewhere"))?.ok_or_else(|| anyhow!("no terminal"))?;
        for key in [Key::Digit(1), Key::Escape, Key::CtrlC, Key::End, Key::Return, Key::Digit(4), Key::Left] {
            press_key(&host, key)?;
        }
        Ok(())
    }

    #[test]
    fn a_key_is_pressed_the_way_each_terminal_takes_one() {
        let answer = |s: &Sent| {
            Ok(match s.what.as_str() {
                "GHOSTTY_LIST" => GHOSTTY_TABS.to_string(),
                "GHOSTTY_FIND" => "T1\n".to_string(),
                "/opt/homebrew/bin/tmux" if s.args[0] == "list-panes" => "/dev/ttys401 %3 @2 1 my agents\n".to_string(),
                _ => "ok\n".to_string(),
            })
        };
        let p = played(answer, || press_all(IN_GHOSTTY));
        p.got.unwrap();
        assert_eq!(
            p.log,
            [
                "GHOSTTY_LIST []",
                "osc7 /dev/ttys101 /tmp/codealong-100010-NOW",
                r#"GHOSTTY_FIND ["/tmp/codealong-100010-NOW", "8"]"#,
                "osc7 /dev/ttys101 /r/app",
                r#"GHOSTTY_ACTION ["T1", "text:1"]"#,
                r#"GHOSTTY_KEY ["T1", "escape"]"#,
                r#"GHOSTTY_ACTION ["T1", "text:\\x03"]"#,
                r#"GHOSTTY_ACTION ["T1", "text:\\x1b[F"]"#,
                r#"GHOSTTY_KEY ["T1", "enter"]"#,
                r#"GHOSTTY_ACTION ["T1", "text:4"]"#,
                r#"GHOSTTY_ACTION ["T1", "text:\\x1b[D"]"#,
            ]
        );
        let p = played(answer, || press_all(IN_ITERM));
        p.got.unwrap();
        assert_eq!(p.log, [r#"ITERM_PRESS+SESSION_ON ["/dev/ttys201", "1"]"#, r#"ITERM_PRESS+SESSION_ON ["/dev/ttys201", "\u{1b}"]"#, r#"ITERM_PRESS+SESSION_ON ["/dev/ttys201", "\u{3}"]"#, r#"ITERM_PRESS+SESSION_ON ["/dev/ttys201", "\u{1b}[F"]"#, r#"ITERM_PRESS+SESSION_ON ["/dev/ttys201", "\r"]"#, r#"ITERM_PRESS+SESSION_ON ["/dev/ttys201", "4"]"#, r#"ITERM_PRESS+SESSION_ON ["/dev/ttys201", "\u{1b}[D"]"#]);
        let p = played(answer, || press_all(IN_TMUX));
        p.got.unwrap();
        assert_eq!(
            p.log,
            [
                TMUX_PANES,
                r#"/opt/homebrew/bin/tmux ["send-keys", "-t", "%3", "1"] (5s)"#,
                r#"/opt/homebrew/bin/tmux ["send-keys", "-t", "%3", "Escape"] (5s)"#,
                r#"/opt/homebrew/bin/tmux ["send-keys", "-t", "%3", "C-c"] (5s)"#,
                r#"/opt/homebrew/bin/tmux ["send-keys", "-t", "%3", "End"] (5s)"#,
                r#"/opt/homebrew/bin/tmux ["send-keys", "-t", "%3", "Enter"] (5s)"#,
                r#"/opt/homebrew/bin/tmux ["send-keys", "-t", "%3", "4"] (5s)"#,
                r#"/opt/homebrew/bin/tmux ["send-keys", "-t", "%3", "Left"] (5s)"#,
            ]
        );

        // Terminal presses Return after whatever it types, so it has no single key.
        let p = played(answer, || press_all(IN_TERMINAL));
        assert_eq!(err(p.got), "Terminal can't press a single key for Codealong. Open it to answer.");
        assert!(p.log.is_empty());
        // Nowhere to press one: no terminal Codealong knows, no tab found, no pane.
        let p = played(answer, || (terminal_host(IN_KITTY, Path::new("/r/app")).map(|h| h.is_none()), terminal_host(IN_BACKGROUND, Path::new("/r/app")).map(|h| h.is_none())));
        assert!(p.got.0.unwrap() && p.got.1.unwrap());
        assert!(p.log.is_empty());
        let p = played(|s| Ok(if s.what == "GHOSTTY_LIST" { GHOSTTY_TABS.to_string() } else { "\n".into() }), || (terminal_host(IN_GHOSTTY, Path::new("/r/app")).map(|h| h.is_none()), terminal_host(IN_TMUX, Path::new("/r/app")).map(|h| h.is_none())));
        assert!(p.got.0.unwrap() && p.got.1.unwrap());
        // Ghostty not answering while the tab is looked for is a failure here, and so is a key
        // it didn't take.
        let p = played(|s| if s.what == "GHOSTTY_LIST" { Ok(GHOSTTY_TABS.to_string()) } else { Err(NOT_ALLOWED.into()) }, || terminal_host(IN_GHOSTTY, Path::new("/r/app")).map(|_| ()));
        assert_eq!(err(p.got), "Codealong isn't allowed to control Ghostty. Turn it on in System Settings → Privacy & Security → Automation.");
        assert_eq!(p.log.len(), 4, "the folder is put back all the same");
        let gone = |s: &Sent| match s.what.as_str() {
            "GHOSTTY_LIST" => Ok(GHOSTTY_TABS.to_string()),
            "GHOSTTY_FIND" => Ok("T1\n".to_string()),
            _ => Err("execution error: That terminal is gone. (-2700)".to_string()),
        };
        let p = played(gone, || press_all(IN_GHOSTTY));
        assert_eq!(err(p.got), "Ghostty didn't respond: execution error: That terminal is gone. (-2700)");
        assert_eq!(p.log.len(), 5);
        let p = played(|_| Err("execution error: That session is gone. (-2700)".into()), || press_all(IN_ITERM));
        assert_eq!(err(p.got), "iTerm2 didn't respond: execution error: That session is gone. (-2700)");
    }

    #[test]
    fn closing_sessions_terminals_hangs_up_their_processes_then_closes_the_tabs_app_by_app() {
        let lists = Rc::new(Cell::new(0));
        let answer = move |s: &Sent| {
            Ok(match s.what.as_str() {
                // What Ghostty has before the marks, then with each mark as its terminal's folder.
                "GHOSTTY_LIST" => {
                    lists.set(lists.get() + 1);
                    match (lists.get(), marks().as_slice()) {
                        (1, _) => GHOSTTY_TABS.to_string(),
                        (_, [one, two]) => format!("T1\t{one}\t✳ Fix the login\nT2\t{two}\t✳ Bump dependencies\nT3\t/r/app\tzsh\n"),
                        (_, other) => panic!("marks: {other:?}"),
                    }
                }
                // The second Ghostty session's tab is the one you're in; so is Terminal's.
                "GHOSTTY_IN_FRONT" => "T2\n".to_string(),
                "ITERM_TTYS" => "/dev/ttys200\n/dev/ttys201\n".to_string(),
                "ITERM_IN_FRONT" => "\n".to_string(),
                "TERMINAL_TTYS" => "/dev/ttys301\n".to_string(),
                "TERMINAL_IN_FRONT" => "/dev/ttys301\n".to_string(),
                "GHOSTTY_CLOSE" | "ITERM_CLOSE+SESSION_ON" | "TERMINAL_CLOSE+TAB_ON" => "ok\n".to_string(),
                _ => panic!("unexpected {} {:?}", s.what, s.args),
            })
        };
        let dir = Path::new("/r/elsewhere");
        let sessions = [(IN_TMUX, dir), (IN_TERMINAL, dir), (ALSO_IN_ITERM, dir), (IN_GHOSTTY, dir), (IN_BACKGROUND, dir), (IN_ITERM, dir), (ALSO_IN_GHOSTTY, dir), (IN_KITTY, dir)];
        let p = played(answer, move || close_session_terminals(&sessions));
        assert_eq!(
            p.got.iter().map(|r| r.as_ref().map_err(|e| e.to_string()).err().unwrap_or_else(|| "closed".into())).collect::<Vec<_>>(),
            [
                "It isn't in a Ghostty, iTerm2 or Terminal tab.",
                "It's the tab you're in.",
                "Couldn't find its iTerm2 tab.",
                "closed",
                "It isn't in a Ghostty, iTerm2 or Terminal tab.",
                "closed",
                "It's the tab you're in.",
                "It isn't in a Ghostty, iTerm2 or Terminal tab.",
            ]
        );
        assert_eq!(
            p.log,
            [
                "GHOSTTY_LIST []",
                "osc7 /dev/ttys101 /tmp/codealong-100010-NOW",
                "osc7 /dev/ttys102 /tmp/codealong-100020-NOW",
                "GHOSTTY_LIST []",
                "osc7 /dev/ttys101 /r/app",
                "osc7 /dev/ttys102 /r/other",
                "GHOSTTY_IN_FRONT []",
                "hang up [100010]",
                r#"GHOSTTY_CLOSE ["T1"]"#,
                "ITERM_TTYS []",
                "ITERM_IN_FRONT []",
                "hang up [200010]",
                r#"ITERM_CLOSE+SESSION_ON ["/dev/ttys201"]"#,
                "TERMINAL_TTYS []",
                "TERMINAL_IN_FRONT []",
                "hang up []",
            ]
        );
    }

    #[test]
    fn a_tab_stays_when_its_process_does_or_its_app_says_no() {
        let dir = Path::new("/r/app");
        let said = |results: &[Result<()>]| results.iter().map(|r| r.as_ref().map_err(|e| e.to_string()).err().unwrap_or_else(|| "closed".into())).collect::<Vec<_>>();
        // One process of two doesn't exit: its tab stays, the other's closes.
        let answer = |s: &Sent| {
            Ok(match s.what.as_str() {
                "ITERM_TTYS" => "/dev/ttys201\n/dev/ttys202\n".to_string(),
                "TERMINAL_TTYS" => "/dev/ttys301\n".to_string(),
                _ => "\n".to_string(),
            })
        };
        let p = play(Setup { stays: &[IN_ITERM, IN_TERMINAL], ..setup() }, answer, move || close_session_terminals(&[(IN_ITERM, dir), (ALSO_IN_ITERM, dir), (IN_TERMINAL, dir)]));
        assert_eq!(said(&p.got), ["Its process (pid 200010) didn't exit, so the tab stays.", "closed", "Its process (pid 300010) didn't exit, so the tab stays."]);
        assert_eq!(
            p.log,
            ["ITERM_TTYS []", "ITERM_IN_FRONT []", "hang up [200010, 200020]", r#"ITERM_CLOSE+SESSION_ON ["/dev/ttys202"]"#, "TERMINAL_TTYS []", "TERMINAL_IN_FRONT []", "hang up [300010]"]
        );

        // The app refusing the close, or not listing its tabs at all.
        let refuses = |s: &Sent| match s.what.as_str() {
            "ITERM_TTYS" => Ok("/dev/ttys201\n".to_string()),
            "ITERM_CLOSE+SESSION_ON" | "TERMINAL_TTYS" => Err("boom".to_string()),
            _ => Ok("\n".to_string()),
        };
        let p = played(refuses, move || close_session_terminals(&[(IN_ITERM, dir), (IN_TERMINAL, dir)]));
        assert_eq!(said(&p.got), ["Its process ended, but the tab didn't close: iTerm2 didn't respond: boom", "Terminal didn't respond: boom"]);

        // Ghostty: a process that stays, a close refused, and its terminals not listed at all.
        let marked = |s: &Sent| {
            Ok(match s.what.as_str() {
                "GHOSTTY_LIST" => match marks().as_slice() {
                    [one, two] => format!("T1\t{one}\ta\nT2\t{two}\tb\n"),
                    _ => GHOSTTY_TABS.to_string(),
                },
                _ => "\n".to_string(),
            })
        };
        let p = play(Setup { stays: &[IN_GHOSTTY], ..setup() }, marked, move || close_session_terminals(&[(IN_GHOSTTY, dir), (ALSO_IN_GHOSTTY, dir)]));
        assert_eq!(said(&p.got), ["Its process (pid 100010) didn't exit, so the tab stays.", "closed"]);
        assert_eq!(p.log[6..], ["GHOSTTY_IN_FRONT []", "hang up [100010, 100020]", r#"GHOSTTY_CLOSE ["T2"]"#]);
        let refuses = |s: &Sent| match s.what.as_str() {
            "GHOSTTY_LIST" => Ok(match marks().as_slice() {
                [one] => format!("T1\t{one}\ta\n"),
                _ => GHOSTTY_TABS.to_string(),
            }),
            "GHOSTTY_CLOSE" => Err("boom".to_string()),
            _ => Ok("\n".to_string()),
        };
        let p = played(refuses, move || close_session_terminals(&[(IN_GHOSTTY, dir)]));
        assert_eq!(said(&p.got), ["Its process ended, but the tab didn't close: Ghostty didn't respond: boom"]);
        let p = played(|_| Err("boom".into()), move || close_session_terminals(&[(IN_GHOSTTY, dir), (IN_TMUX, dir)]));
        assert_eq!(said(&p.got), ["Ghostty didn't respond: boom", "It isn't in a Ghostty, iTerm2 or Terminal tab."]);
        assert_eq!(p.log, ["GHOSTTY_LIST []"]);
    }

    #[test]
    fn things_open_through_macos_open_with_each_ones_arguments_wait_and_words() {
        let ok = |_: &Sent| Ok(String::new());
        let no = |s: &Sent| if s.what == "/usr/bin/open" { Err("/usr/bin/open no such app".to_string()) } else { Ok(String::new()) };
        let p = played(ok, || (open_url("https://example.com/a b"), reveal(Path::new("/r/my app"))));
        p.got.0.unwrap();
        p.got.1.unwrap();
        assert_eq!(p.log, [r#"/usr/bin/open ["https://example.com/a b"] (10s)"#, r#"/usr/bin/open ["-R", "/r/my app"] (10s)"#]);
        let p = played(no, || (open_url("https://example.com"), reveal(Path::new("/r/app"))));
        assert_eq!(err(p.got.0), "Couldn't open https://example.com: /usr/bin/open no such app");
        assert_eq!(err(p.got.1), "/usr/bin/open no such app");

        // A file a conversation names opens in its app; one that opening would run is shown in
        // the Finder instead, and one that's gone opens nothing.
        let dir = crate::fixtures::scratch("open-file");
        let file = |name: &str, mode: u32| {
            use std::os::unix::fs::PermissionsExt;
            let p = dir.join(name);
            std::fs::write(&p, b"x").unwrap();
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(mode)).unwrap();
            p
        };
        let (shot, script, program, app) = (file("after shot.png", 0o644), file("fix.command", 0o644), file("build", 0o755), dir.join("Tool.app"));
        std::fs::create_dir_all(&app).unwrap();
        // A link named as a picture, to a script, is the script's.
        let disguised = dir.join("chart.png");
        std::os::unix::fs::symlink(&script, &disguised).unwrap();
        let p = played(ok, || [&shot, &script, &program, &app, &disguised].map(|f| open_file(f).is_ok()));
        assert_eq!(p.got, [true; 5]);
        let shown = |f: &Path| f.to_string_lossy().into_owned();
        let revealed = |f: &Path| format!("/usr/bin/open [\"-R\", {:?}] (10s)", shown(f));
        assert_eq!(p.log, [format!("/usr/bin/open [{:?}] (10s)", shown(&shot)), revealed(&script), revealed(&program), revealed(&app), revealed(&disguised)]);
        let p = played(ok, || (open_file(&dir.join("gone.pdf")), open_file(Path::new("shots/after.png"))));
        assert_eq!(err(p.got.0), "gone.pdf isn't on this Mac any more.");
        assert_eq!(err(p.got.1), "shots/after.png isn't a full path.");
        assert!(p.log.is_empty());
        let p = played(no, || open_file(&shot));
        assert_eq!(err(p.got), "Couldn't open after shot.png: /usr/bin/open no such app");

        // A Codex link goes to the Codex app that's running, by either name, else to whatever
        // takes the link.
        let running = |app: &'static str| move |s: &Sent| if s.what == "/usr/bin/pgrep" && !s.args[1].contains(app) { Err("no process".to_string()) } else { Ok(String::new()) };
        let p = played(running("ChatGPT"), || open_codex("codex://threads/1"));
        p.got.unwrap();
        assert_eq!(p.log, [r#"/usr/bin/pgrep ["-f", "/Applications/ChatGPT.app/Contents/MacOS/"] (3s)"#, r#"/usr/bin/open ["-a", "/Applications/ChatGPT.app", "codex://threads/1"] (10s)"#]);
        let p = played(running("Codex"), || open_codex("codex://threads/1"));
        p.got.unwrap();
        assert_eq!(
            p.log,
            [
                r#"/usr/bin/pgrep ["-f", "/Applications/ChatGPT.app/Contents/MacOS/"] (3s)"#,
                r#"/usr/bin/pgrep ["-f", "/Applications/Codex.app/Contents/MacOS/"] (3s)"#,
                r#"/usr/bin/open ["-a", "/Applications/Codex.app", "codex://threads/1"] (10s)"#,
            ]
        );
        let p = played(running("neither"), || open_codex("codex://threads/1"));
        p.got.unwrap();
        assert_eq!(p.log[2], r#"/usr/bin/open ["codex://threads/1"] (10s)"#);
        let p = played(no, || open_codex("codex://threads/1"));
        assert_eq!(err(p.got), "Couldn't open Codex: /usr/bin/open no such app");

        // The ones in other files: a page in a Chrome profile, a folder in an editor.
        let profile = crate::chrome::Profile { id: "Work".into(), name: "Work".into(), dir: "Profile 12".into(), email: None };
        let other = profile.clone();
        let p = played(ok, move || (crate::chrome::open("http://localhost:3000", &profile), crate::editors::open("An Editor", Path::new("/r/my app"))));
        p.got.0.unwrap();
        p.got.1.unwrap();
        assert_eq!(
            p.log,
            [r#"/usr/bin/open ["-n", "-b", "com.google.Chrome", "--args", "--profile-directory=Profile 12", "http://localhost:3000"] (10s)"#, r#"/usr/bin/open ["-a", "An Editor", "/r/my app"] (15s)"#]
        );
        let p = played(no, move || (crate::chrome::open("http://localhost:3000", &other), crate::editors::open("An Editor", Path::new("/r/app"))));
        assert_eq!(err(p.got.0), "Couldn't open http://localhost:3000 in Chrome's Work profile: /usr/bin/open no such app");
        assert_eq!(err(p.got.1), "Couldn't open An Editor. Is it installed?");
    }
}
