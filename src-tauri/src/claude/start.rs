//! The commands that start, fork, resume and stop Claude Code sessions, and what its command
//! line takes.

use std::path::Path;
use std::time::{Duration, Instant};

use crate::attachments::{self, Style};
use crate::config::AccountCfg;
use crate::model::{Harness, Session};

/// The models a start can ask for, as `--model` takes them: aliases for the newest
/// model of each family, the way Claude Code's own `/model` picker offers them (2.1.282 knows
/// sonnet, opus, haiku, fable, their `[1m]` variants, best and opusplan). The apps' new-session
/// links take no model; you pick it in the app.
pub(super) const MODELS: [(&str, &str); 8] = [
    ("fable", "Fable"),
    ("fable[1m]", "Fable (1M context)"),
    ("opus", "Opus"),
    ("opus[1m]", "Opus (1M context)"),
    ("sonnet", "Sonnet"),
    ("sonnet[1m]", "Sonnet (1M context)"),
    ("haiku", "Haiku"),
    ("opusplan", "Opus Plan Mode: Opus to plan, Sonnet to build"),
];

/// The effort levels a start can ask for, as `--effort` takes them, named the way
/// Claude Code's own picker names them (2.1.284: `xhigh` is Extra). A model that doesn't take
/// a level gets the nearest it does (`max` and `xhigh` become `high`), and Haiku takes none.
pub(super) const EFFORTS: [(&str, &str); 5] = [("low", "Low"), ("medium", "Medium"), ("high", "High"), ("xhigh", "Extra"), ("max", "Max")];

/// The `model` in an account's settings.json, which Claude Code starts on without
/// `--model`. A repo's own `.claude/settings.json` can still override it.
pub fn default_model(a: &AccountCfg) -> Option<String> {
    let bytes = std::fs::read(a.home_path().join("settings.json")).ok()?;
    let v: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    v.get("model").and_then(serde_json::Value::as_str).map(str::trim).filter(|m| !m.is_empty()).map(str::to_string)
}

/// What follows the account's command in a background start, quoted for the shell:
/// the new worktree and attachment folders, `--bg`, the model and effort, and the prompt last.
/// `--bg` picks the session's ID itself and ignores `--session-id`.
pub(super) fn background_args(prompt: &str, model: Option<&str>, effort: Option<&str>, worktree: bool, (images_dir, files_dir): (&Path, &Path)) -> String {
    let (has_images, has_files) = crate::attachments::kinds(prompt);
    let prompt = crate::attachments::expand_in(prompt, images_dir, files_dir, crate::attachments::Style::Mention);
    let prompt = prompt.trim();
    let mut args = String::new();
    if worktree {
        args.push_str(" --worktree");
    }
    for (has, dir) in [(has_images, images_dir), (has_files, files_dir)] {
        if has {
            args.push_str(&format!(" --add-dir {}", crate::util::sh_quote(&dir.to_string_lossy())));
        }
    }
    args.push_str(" --bg");
    if let Some(m) = model.filter(|m| crate::start::valid_model(m)) {
        args.push_str(&format!(" --model '{m}'"));
    }
    if let Some(e) = effort {
        args.push_str(&format!(" --effort {e}"));
    }
    if !prompt.is_empty() {
        args.push_str(&format!(" {}", crate::util::sh_quote(&crate::start::as_argument(prompt))));
    }
    args
}

/// The command that forks session `id` into a background job, on `prompt` when
/// there's one: `--bg` with `--resume` and `--fork-session` starts a copy of the conversation
/// as far as it's written, the original carrying on (checked on 2.1.286). Without a prompt the
/// job waits for one, and writes its transcript, the conversation so far included, with it.
pub fn fork_command(account: &AccountCfg, id: &str, prompt: &str) -> String {
    fork_command_in(account, id, prompt, (&crate::attachments::images_dir(), &crate::attachments::files_dir()))
}

pub(crate) fn fork_command_in(account: &AccountCfg, id: &str, prompt: &str, dirs: (&Path, &Path)) -> String {
    format!("{} --resume {id} --fork-session{}", account.command(), background_args(prompt, None, None, false, dirs))
}

/// The job ID `--bg` prints (`backgrounded · 34b864ad`), which is also where the session's own
/// ID begins.
pub fn background_job(output: &str) -> Option<String> {
    output.lines().find_map(|l| {
        let rest = l.trim().strip_prefix("backgrounded")?;
        let job: String = rest.trim_start_matches(|c: char| !c.is_ascii_hexdigit()).chars().take_while(char::is_ascii_hexdigit).collect();
        (job.len() >= 8).then_some(job)
    })
}

/// A background job's session ID, from the service's roster (`<home>/daemon/roster.json`,
/// `workers.<job>.sessionId`), where it's written as the job starts.
pub fn background_session(roster: &serde_json::Value, job: &str) -> Option<String> {
    roster.get("workers")?.get(job)?.get("sessionId")?.as_str().filter(|id| id.starts_with(job)).map(str::to_string)
}

/// Runs a background start in the login shell, which knows the account's command even when it's
/// an alias, and returns the job's session ID once the service has it. Output goes to a file,
/// not a pipe: a service the command starts would hold a pipe open long after it returns.
pub(crate) fn start_background(account: &AccountCfg, cwd: &Path, command: &str) -> anyhow::Result<(String, Option<String>)> {
    use std::process::{Command, Stdio};
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
    let out_path = std::env::temp_dir().join(format!("codealong-bg-{}-{}.txt", std::process::id(), crate::start::new_session_id()));
    let out = std::fs::File::create(&out_path)?;
    let mut child = Command::new(&shell)
        .args(["-ilc", command])
        .current_dir(cwd)
        .env_remove(crate::start::SESSION_VAR)
        .stdin(Stdio::null())
        .stdout(out.try_clone()?)
        .stderr(out)
        .spawn()
        .map_err(|e| anyhow::anyhow!("couldn't start {shell}: {e}"))?;
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if started.elapsed() > Duration::from_secs(60) {
            let _ = child.kill();
            let _ = std::fs::remove_file(&out_path);
            anyhow::bail!("{} didn't start in the background within a minute.", account.name);
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let output = std::fs::read_to_string(&out_path).unwrap_or_default();
    let _ = std::fs::remove_file(&out_path);
    let Some(job) = background_job(&output) else {
        // Claude Code says why: an untrusted folder, a bad option, not signed in.
        let why = output.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with("Starting background service")).last().unwrap_or("").to_string();
        anyhow::bail!(if why.is_empty() { format!("{} didn't start in the background (exit {}).", account.name, status.code().unwrap_or(-1)) } else { why });
    };
    let roster = account.home_path().join("daemon/roster.json");
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let id = std::fs::read(&roster).ok().and_then(|b| serde_json::from_slice(&b).ok()).and_then(|v| background_session(&v, &job));
        if id.is_some() || Instant::now() > deadline {
            return Ok((job, id));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// `<account command> stop <job>` for a background job, its job read from its attach command.
/// None for anything else, or a job id that isn't plain letters and digits, since it's typed
/// into a shell.
pub(super) fn stop_command(account: &crate::config::AccountCfg, attach: &str) -> Option<String> {
    let (_, job) = attach.rsplit_once(" attach ")?;
    (!job.is_empty() && job.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')).then(|| format!("{} stop {job}", account.command()))
}

/// Forks session `from` (its ID, or its transcript's path quoted for the shell) into a new one
/// named `new_id` under `account`: `--fork-session` copies the transcript, and takes
/// `--session-id` for the copy's ID (resuming takes it only with `--fork-session`), so the
/// copy's key is known before it's written, as a start's is.
pub(super) fn fork_as(account: &crate::config::AccountCfg, from: &str, new_id: &str) -> crate::harness::Typed {
    let key = Session::key_for(Harness::Claude, &account.name, new_id);
    crate::harness::Typed {
        input: format!("{} --resume {from} --fork-session --session-id {new_id}\n", account.command()),
        env: vec![(crate::start::SESSION_VAR.to_string(), key.clone())],
        key: Some(key),
    }
}

/// The terminal input that resumes a Claude Code session with a prompt, and the environment
/// that carries the prompt (`Adapter::resume`).
pub fn claude_resume_input(account: &AccountCfg, session_id: &str, text: &str) -> (String, Vec<(String, String)>) {
    let (has_images, has_files) = attachments::kinds(text);
    let mut input = account.command().to_string();
    let mut env = Vec::new();
    for (has, var, dir) in [(has_images, crate::start::IMAGES_VAR, attachments::images_dir()), (has_files, crate::start::FILES_VAR, attachments::files_dir())] {
        if has {
            input.push_str(&format!(" --add-dir \"${var}\""));
            env.push((var.to_string(), dir.to_string_lossy().to_string()));
        }
    }
    input.push_str(&format!(" --resume {session_id} \"${}\"\n", crate::start::PROMPT_VAR));
    env.push((crate::start::PROMPT_VAR.to_string(), crate::start::as_argument(attachments::expand(text, Style::Mention).trim())));
    (input, env)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn stops_a_job_by_the_id_it_attaches_to() {
        let a = crate::config::AccountCfg { name: "claude2".into(), harness: "claude".into(), home: "~/.claude2".into(), command: None, start: None, preferred: false, acp: None };
        assert_eq!(stop_command(&a, "claude2 attach 6e07c149").as_deref(), Some("claude2 stop 6e07c149"));
        assert_eq!(stop_command(&a, "claude2 attach 6e07c149; rm -rf ~"), None);
        assert_eq!(stop_command(&a, "claude2 --resume 6e07c149"), None);
    }

    #[test]
    fn forks_under_an_id_it_picks() {
        let a = crate::config::AccountCfg { name: "claude2".into(), harness: "claude".into(), home: "~/.claude2".into(), command: None, start: None, preferred: false, acp: None };
        let f = fork_as(&a, "4f1c2a9e-0b7d-4e21-9c55-1a2b3c4d5e6f", "9d8c7b6a-5f4e-4d3c-8b2a-1f0e9d8c7b6a");
        assert_eq!(f.input, "claude2 --resume 4f1c2a9e-0b7d-4e21-9c55-1a2b3c4d5e6f --fork-session --session-id 9d8c7b6a-5f4e-4d3c-8b2a-1f0e9d8c7b6a\n");
        assert_eq!(f.key.as_deref(), Some("claude:claude2:9d8c7b6a-5f4e-4d3c-8b2a-1f0e9d8c7b6a"));
        assert_eq!(f.env, vec![("CODEALONG_SESSION".to_string(), "claude:claude2:9d8c7b6a-5f4e-4d3c-8b2a-1f0e9d8c7b6a".to_string())]);
    }

    #[test]
    fn moves_a_session_by_resuming_its_transcript_under_the_other_account() {
        use crate::harness::Adapter;
        let home = crate::fixtures::scratch("move");
        let id = "4f1c2a9e-0b7d-4e21-9c55-1a2b3c4d5e6f";
        // Resumed from another folder once, it has a transcript in each; the later one goes on.
        let (old, new) = (home.join("projects/-r-old"), home.join("projects/-r-it's"));
        for dir in [&old, &new] {
            std::fs::create_dir_all(dir).unwrap();
            std::fs::write(dir.join(format!("{id}.jsonl")), "{}\n").unwrap();
        }
        let earlier = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
        std::fs::File::options().write(true).open(old.join(format!("{id}.jsonl"))).unwrap().set_modified(earlier).unwrap();
        let from = crate::config::AccountCfg { name: "claude".into(), harness: "claude".into(), home: home.to_string_lossy().into(), command: None, start: None, preferred: false, acp: None };
        let to = crate::config::AccountCfg { name: "claude2".into(), harness: "claude".into(), home: "~/.claude2".into(), command: None, start: None, preferred: false, acp: None };
        let ctx = crate::harness::Ctx { account: &from };
        let moved = super::super::ClaudeCode.move_to(&ctx, &Session::new(Harness::Claude, "claude", id), &to).unwrap();
        let path = new.join(format!("{id}.jsonl")).to_string_lossy().replace('\'', "'\\''");
        let key = moved.key.clone().unwrap();
        let copy = key.strip_prefix("claude:claude2:").unwrap();
        assert_ne!(copy, id);
        assert_eq!(moved.input, format!("claude2 --resume '{path}' --fork-session --session-id {copy}\n"));
        assert_eq!(moved.env, vec![("CODEALONG_SESSION".to_string(), key)]);
        // A live session that hasn't said anything has no transcript yet.
        let quiet = Session { live: true, ..Session::new(Harness::Claude, "claude", "0f5e1d2c") };
        assert!(super::super::ClaudeCode.move_to(&ctx, &quiet, &to).unwrap_err().to_string().contains("Nothing's been said"));
    }

    #[test]
    fn forks_into_the_background_with_the_prompt_quoted_last() {
        let a = AccountCfg { name: "claude2".into(), harness: "claude".into(), home: "~/.claude2".into(), command: Some("claude2".into()), start: None, preferred: false, acp: None };
        let dirs = (Path::new("/i"), Path::new("/f"));
        let id = "3628c9e6-0309-4a30-aed9-3e68d0e66b7e";
        assert_eq!(fork_command_in(&a, id, "", dirs), format!("claude2 --resume {id} --fork-session --bg"));
        assert_eq!(fork_command_in(&a, id, "  try it the other way's round  ", dirs), format!("claude2 --resume {id} --fork-session --bg 'try it the other way'\\''s round'"));
        assert_eq!(
            fork_command_in(&a, id, "look\n![image](images/0123456789abcdef.png)", dirs),
            format!("claude2 --resume {id} --fork-session --add-dir '/i' --bg 'look\n@\"/i/0123456789abcdef.png\"'")
        );
    }

    #[test]
    fn finds_the_background_session_it_started() {
        let out = "backgrounded · 34b864ad\n  claude agents             list sessions\n  claude attach 34b864ad    open in this terminal\n";
        assert_eq!(background_job(out).as_deref(), Some("34b864ad"));
        assert_eq!(background_job("backgrounded · 3a6096ed (idle — send a prompt to start)\n").as_deref(), Some("3a6096ed"));
        assert_eq!(background_job("Workspace not trusted. Run `claude` in /r/x once and accept the trust prompt, then retry.\n"), None);
        let roster = serde_json::json!({"workers": {"34b864ad": {"sessionId": "34b864ad-b397-4f98-96fe-b06f41bb2653"}, "11111111": {"sessionId": "22222222-0000-4000-8000-000000000000"}}});
        assert_eq!(background_session(&roster, "34b864ad").as_deref(), Some("34b864ad-b397-4f98-96fe-b06f41bb2653"));
        assert_eq!(background_session(&roster, "11111111"), None);
        assert_eq!(background_session(&roster, "99999999"), None);
    }

    /// Starts a real background session on Haiku in a folder Claude Code trusts, checks that its
    /// transcript has the prompt exactly as given, then stops and removes it. Opt in:
    /// `CODEALONG_TEST_FOLDER=<trusted git folder> cargo test background_round_trip -- --ignored`.
    #[test]
    #[ignore]
    fn background_round_trip() {
        let folder = PathBuf::from(std::env::var("CODEALONG_TEST_FOLDER").expect("CODEALONG_TEST_FOLDER"));
        let a = AccountCfg { name: "claude".into(), harness: "claude".into(), home: "~/.claude".into(), command: Some("claude".into()), start: None, preferred: false, acp: None };
        let prompt = "Reply with just ok. Don't expand $HOME or `this` or $(say nope) -v";
        let crate::start::Plan::Background { cwd, command } = crate::start::plan(&a, crate::start::Place::Background, &folder, prompt, "unused", Some("haiku"), None, false) else { panic!() };
        let (job, id) = start_background(&a, &cwd, &command).unwrap();
        let id = id.expect("the roster names the session");
        assert!(id.starts_with(&job));
        let mut seen = false;
        for _ in 0..100 {
            let found = crate::util::home().join(".claude/projects").read_dir().unwrap().flatten().map(|d| d.path().join(format!("{id}.jsonl"))).find(|p| p.exists());
            if let Some(text) = found.and_then(|p| std::fs::read_to_string(p).ok()) {
                // Nothing in it needs escaping in JSON.
                if text.contains(prompt) {
                    seen = true;
                    break;
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
        let _ = crate::procs::run("claude", &["stop", &job], Some(&folder), std::time::Duration::from_secs(20));
        let _ = crate::procs::run("claude", &["rm", &job], Some(&folder), std::time::Duration::from_secs(20));
        assert!(seen, "the transcript never showed the prompt");
    }
}
