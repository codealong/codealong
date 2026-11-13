//! Starting Codealong's server (serve.rs) as a process of its own, which outlives whoever started
//! it and is nobody's child but the system's: the app, or the `codealong` command run in a
//! terminal or an agent's shell, which may be in a sandbox the server mustn't inherit. On macOS
//! it's a launchd job in your login session, given the environment the Dock gives an app; on Linux
//! a process in a session of its own (`setsid`), which a closing terminal doesn't take with it, and
//! on Windows one detached from the starter's console, process group and job.
//! Stopping it is asking it to quit over its socket (server.rs); on macOS the job is then taken
//! out of launchd too (`unload`), so nothing starts it again.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{anyhow, Context, Result};

use super::Os;
use crate::procs;

/// What's started: `<exe> serve`.
pub struct Job<'a> {
    pub exe: &'a Path,
    /// Its whole environment.
    pub env: &'a [(String, String)],
    /// Where what it prints goes.
    pub log: &'a Path,
}

/// How long one `launchctl` may take.
const TIMEOUT: Duration = Duration::from_secs(20);

/// The job's name in launchd: one for each folder of Codealong's files (`support`), so a copy tried
/// with a home of its own has a server of its own, and Codealong and Codealong Dev, which share
/// theirs, have one between them.
pub fn label(support: &Path) -> String {
    use sha2::{Digest, Sha256};
    let hash = Sha256::digest(support.to_string_lossy().as_bytes());
    format!("app.codealong.server.{}", hash[..4].iter().map(|b| format!("{b:02x}")).collect::<String>())
}

/// Starts the server as a job of its own. Whether it came up is for the caller to see, by asking
/// it over its socket: a job launchd took can still fail as it starts.
pub fn start(job: &Job, support: &Path) -> Result<()> {
    match super::current() {
        Os::MacOs => start_in_launchd(job, support),
        Os::Linux | Os::Windows => start_detached(job),
    }
}

/// Takes the server's job out of launchd, stopping it if it still runs. Nothing to do on Linux
/// and Windows, where it isn't anyone's job and asking it to quit was all.
pub fn unload(support: &Path) {
    if super::is_macos() {
        let label = label(support);
        for domain in domains() {
            let _ = procs::run("/bin/launchctl", &["bootout", &format!("{domain}/{label}")], None, TIMEOUT);
        }
    }
}

/// The launchd domains a job of yours goes in, the login session's first: `gui/<uid>` is there
/// while you're logged in at the screen, and has what the server asks of the Mac (other apps
/// scripted, the keychain); `user/<uid>` is there over SSH alone.
fn domains() -> [String; 2] {
    let uid = super::tty::user_id();
    [format!("gui/{uid}"), format!("user/{uid}")]
}

/// The job's property list, written beside Codealong's files, and loaded into launchd. A job
/// already loaded, running or not, is taken out first: it may name another binary.
fn start_in_launchd(job: &Job, support: &Path) -> Result<()> {
    let label = label(support);
    let file = support.join("server.plist");
    std::fs::create_dir_all(support).with_context(|| format!("making {}", support.display()))?;
    plist_for(job, &label).to_file_xml(&file).with_context(|| format!("writing {}", file.display()))?;
    unload(support);
    let path = file.to_string_lossy().into_owned();
    let mut why = String::new();
    for domain in domains() {
        // launchd takes a moment to let go of a job just taken out, and refuses to load it again
        // meanwhile (5: Input/output error).
        for attempt in 0..4 {
            match procs::run("/bin/launchctl", &["bootstrap", &domain, &path], None, TIMEOUT) {
                Ok(_) => return Ok(()),
                Err(e) => why = e.to_string(),
            }
            if attempt < 3 {
                std::thread::sleep(Duration::from_millis(300));
            }
        }
    }
    Err(anyhow!("launchd didn't take Codealong's server: {}", why.trim()))
}

/// What launchd is told to run, and how: at once, kept running if it ends by crashing (not when
/// it quits, asked to), as a process you're waiting on rather than one throttled in the
/// background, and leaving what it started running as it ends, as an app's do.
fn plist_for(job: &Job, label: &str) -> plist::Value {
    use plist::{Dictionary, Value};
    let mut env = Dictionary::new();
    for (name, value) in job.env {
        env.insert(name.clone(), Value::String(value.clone()));
    }
    let mut keep_alive = Dictionary::new();
    keep_alive.insert("SuccessfulExit".into(), Value::Boolean(false));
    let log = job.log.to_string_lossy().into_owned();
    let mut d = Dictionary::new();
    d.insert("Label".into(), Value::String(label.to_string()));
    d.insert("ProgramArguments".into(), Value::Array(vec![Value::String(job.exe.to_string_lossy().into_owned()), Value::String("serve".into())]));
    d.insert("EnvironmentVariables".into(), Value::Dictionary(env));
    d.insert("StandardOutPath".into(), Value::String(log.clone()));
    d.insert("StandardErrorPath".into(), Value::String(log));
    d.insert("RunAtLoad".into(), Value::Boolean(true));
    d.insert("KeepAlive".into(), Value::Dictionary(keep_alive));
    d.insert("ProcessType".into(), Value::String("Interactive".into()));
    d.insert("AbandonProcessGroup".into(), Value::Boolean(true));
    Value::Dictionary(d)
}

/// `<exe> serve` on its own (`detach`), with nothing of this process's but what `job` gives it.
/// Its parent waits for it on a thread, so it isn't left a zombie should it end while the app that
/// started it runs on.
fn start_detached(job: &Job) -> Result<()> {
    let log = std::fs::OpenOptions::new().create(true).append(true).open(job.log).with_context(|| format!("opening {}", job.log.display()))?;
    let mut cmd = std::process::Command::new(job.exe);
    cmd.arg("serve").env_clear().envs(job.env.iter().map(|(k, v)| (k, v))).stdin(std::process::Stdio::null()).stdout(log.try_clone()?).stderr(log);
    let mut child = detach(&mut cmd).with_context(|| format!("starting {}", job.exe.display()))?;
    std::thread::spawn(move || child.wait());
    Ok(())
}

/// Starts it in a session of its own.
#[cfg(unix)]
fn detach(cmd: &mut std::process::Command) -> std::io::Result<std::process::Child> {
    use std::os::unix::process::CommandExt;
    // SAFETY: setsid is async-signal-safe, and is all that runs between fork and exec.
    unsafe {
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    cmd.spawn()
}

/// Starts it with no console and in a process group of its own, so a closing console or its
/// Ctrl+C doesn't reach it, and out of the job the starter runs in (a terminal's, an IDE's),
/// which may end every process in it as it closes. A job that lets nothing out refuses the
/// breakaway: then it's started in the job after all.
#[cfg(windows)]
fn detach(cmd: &mut std::process::Command) -> std::io::Result<std::process::Child> {
    use std::os::windows::process::CommandExt;
    use windows_sys::Win32::System::Threading::{CREATE_BREAKAWAY_FROM_JOB, CREATE_NEW_PROCESS_GROUP, CREATE_NO_WINDOW};
    let flags = CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP;
    cmd.creation_flags(flags | CREATE_BREAKAWAY_FROM_JOB).spawn().or_else(|_| cmd.creation_flags(flags).spawn())
}

/// Where the server's output goes: beside Codealong's other logs.
pub fn log_path() -> PathBuf {
    super::dirs::logs_dir().join("server.log")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_folder_of_files_has_a_job_of_its_own() {
        let mine = label(Path::new("/Users/me/Library/Application Support/Codealong"));
        assert!(mine.starts_with("app.codealong.server.") && mine.len() == "app.codealong.server.".len() + 8, "{mine}");
        assert_eq!(mine, label(Path::new("/Users/me/Library/Application Support/Codealong")));
        assert_ne!(mine, label(Path::new("/tmp/try/home/Library/Application Support/Codealong")));
    }

    #[test]
    fn launchd_runs_the_binary_as_the_server_with_the_environment_given() {
        let env = vec![("HOME".to_string(), "/Users/me".to_string()), ("PATH".to_string(), "/usr/bin:/bin".to_string())];
        let job = Job { exe: Path::new("/Applications/Codealong.app/Contents/MacOS/codealong"), env: &env, log: Path::new("/Users/me/Library/Logs/Codealong/server.log") };
        let p = plist_for(&job, "app.codealong.server.0a1b2c3d");
        let d = p.as_dictionary().unwrap();
        let args: Vec<&str> = d["ProgramArguments"].as_array().unwrap().iter().filter_map(|v| v.as_string()).collect();
        assert_eq!(args, ["/Applications/Codealong.app/Contents/MacOS/codealong", "serve"]);
        assert_eq!(d["EnvironmentVariables"].as_dictionary().unwrap()["HOME"].as_string(), Some("/Users/me"));
        assert_eq!(d["StandardErrorPath"].as_string(), Some("/Users/me/Library/Logs/Codealong/server.log"));
        // A crash brings it back; a quit it was asked for doesn't.
        assert_eq!(d["KeepAlive"].as_dictionary().unwrap()["SuccessfulExit"].as_boolean(), Some(false));
        assert_eq!(d["AbandonProcessGroup"].as_boolean(), Some(true));
    }

    #[test]
    fn a_job_already_loaded_is_taken_out_before_it_is_loaded_again() {
        let dir = std::env::temp_dir().join(format!("codealong-background-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let asked = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let log = asked.clone();
        procs::stand_in::set(Some(Box::new(move |program, args, _, _| {
            log.borrow_mut().push(format!("{program} {}", args.first().copied().unwrap_or_default()));
            Ok(String::new())
        })));
        let env = vec![("HOME".to_string(), "/Users/me".to_string())];
        let job = Job { exe: Path::new("/x/codealong"), env: &env, log: &dir.join("server.log") };
        crate::os::as_if(Os::MacOs, || start(&job, &dir)).unwrap();
        procs::stand_in::set(None);
        assert_eq!(*asked.borrow(), ["/bin/launchctl bootout", "/bin/launchctl bootout", "/bin/launchctl bootstrap"]);
        assert!(dir.join("server.plist").is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
