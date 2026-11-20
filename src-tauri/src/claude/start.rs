//! The commands that start, fork, resume and stop Claude Code sessions, and what its command
//! line takes.

use std::path::{Path, PathBuf};
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

/// Whether a background job of the account whose folder is `home`, started in the checkout
/// `folder`, moves into a worktree of its own before it edits anything (2.1.325's code, the
/// setting's description and the job's own prompt). Such a job is told to call EnterWorktree
/// before its first edit, and an Edit or Write in the shared checkout is refused until it has
/// ("This background session hasn't isolated its changes yet…"); it reads and answers in place.
/// Its worktree is based on a freshly fetched `origin/<default branch>` unless `worktree.baseRef`
/// says `head`, so a checkout on another branch with work of its own doesn't carry into it.
///
/// It goes by `CLAUDE_BG_ISOLATION`, then `worktree.bgIsolation` in its merged settings, each
/// `worktree` or `none` and `worktree` when neither says. The variable is what the job was
/// launched with, which a settings file's `env` replaces, the highest-ranked file that sets it
/// winning, and only then is its value looked at: empty or another word, it says nothing and the
/// setting decides. Each file's setting is read for itself, another word there leaving it unset
/// in that file. Codealong starts background jobs from the login shell (`background_command`),
/// so the variable a job is launched with is taken as the login shell sets it; a background
/// service already running keeps the environment of the shell that started it, which may differ.
/// Not read: the settings `settings_files` leaves out.
pub(super) fn isolates_in_background(home: &Path, folder: &Path) -> bool {
    isolated_by(&super::mode::settings_files(home, folder), launched_with())
}

/// `CLAUDE_BG_ISOLATION` as the login shell sets it, asked once: it takes a login shell.
fn launched_with() -> Option<&'static str> {
    static SAID: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    SAID.get_or_init(|| crate::procs::login_var("CLAUDE_BG_ISOLATION")).as_deref()
}

/// Whether the settings files, lowest-ranked first as `settings_files` lists them, and the
/// variable a job is `launched` with leave a background job isolating itself
/// (`isolates_in_background`).
fn isolated_by(files: &[PathBuf], launched: Option<&str>) -> bool {
    let read: Vec<serde_json::Value> = files.iter().rev().map(|f| std::fs::read(f).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or(serde_json::Value::Null)).collect();
    bg_isolation(&read, launched) != Some("none")
}

/// What the settings files, highest-ranked first, and the variable a job is `launched` with
/// say of a background job's isolation: `worktree`, `none`, or nothing.
fn bg_isolation<'a>(files: &'a [serde_json::Value], launched: Option<&'a str>) -> Option<&'a str> {
    let taken = |v: &&str| matches!(*v, "worktree" | "none");
    let var = files.iter().find_map(|f| f.pointer("/env/CLAUDE_BG_ISOLATION").and_then(serde_json::Value::as_str)).or(launched);
    var.filter(taken).or_else(|| files.iter().find_map(|f| f.pointer("/worktree/bgIsolation").and_then(serde_json::Value::as_str).filter(taken)))
}

/// What follows the account's command in a background start, quoted for the shell:
/// the new worktree and attachment folders, `--bg`, the model and effort, and the prompt last,
/// after `--`. `--bg` takes itself out of the arguments before it reads them, so without the
/// `--` the prompt would follow whatever came before `--bg`: `--add-dir`, which takes several
/// folders, took it as one more and the job started with no prompt, and `--worktree`, which
/// takes an optional name, named the worktree after it. `--bg` picks the session's ID itself
/// and ignores `--session-id`.
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
        args.push_str(&format!(" -- {}", crate::util::sh_quote(&crate::start::as_argument(prompt))));
    }
    args
}

/// The session's title to carry onto a moved copy (`--name`), so Claude Code doesn't auto-title
/// the fork onto something else on its next turn. Leaves out empty titles, the fallback
/// `Session <short id>`, and titles that are just the session's first prompt (which Claude Code
/// hasn't titled yet).
pub(super) fn move_name(s: &Session) -> Option<&str> {
    let title = s.title.trim();
    if title.is_empty()
        || title == format!("Session {}", s.short_id)
        || s.first_prompt.as_deref().map(str::trim) == Some(title)
    {
        None
    } else {
        Some(title)
    }
}

/// ` --name <quoted>` when `name` is non-empty, else empty.
fn name_arg(name: Option<&str>) -> String {
    name.map(str::trim).filter(|n| !n.is_empty()).map(|n| format!(" --name {}", crate::util::sh_quote(n))).unwrap_or_default()
}

/// The command that forks session `id` into a background job, on `prompt` when
/// there's one: `--bg` with `--resume` and `--fork-session` starts a copy of the conversation
/// as far as it's written, the original carrying on (checked on 2.1.286). Without a prompt the
/// job waits for one, and writes its transcript, the conversation so far included, with it.
/// With `name`, `--name` carries the session's title onto the copy so Claude Code keeps it
/// (checked on 2.1.329: sets `nameSource: "user"` in the job record).
pub fn fork_command(account: &AccountCfg, id: &str, prompt: &str, name: Option<&str>) -> String {
    fork_command_in(account, id, prompt, name, (&crate::attachments::images_dir(), &crate::attachments::files_dir()))
}

pub(crate) fn fork_command_in(account: &AccountCfg, id: &str, prompt: &str, name: Option<&str>, dirs: (&Path, &Path)) -> String {
    format!("{} --resume {id} --fork-session{}{}", account.command(), name_arg(name), background_args(prompt, None, None, false, dirs))
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
/// copy's key is known before it's written, as a start's is. With `name`, `--name` carries the
/// session's title onto the copy. With a prompt the copy starts on it at once (it comes in the
/// environment, as a resume's does); without one it waits for one.
pub(super) fn fork_as(account: &crate::config::AccountCfg, from: &str, new_id: &str, prompt: &str, name: Option<&str>) -> crate::harness::Typed {
    let key = Session::key_for(Harness::Claude, &account.name, new_id);
    let mut input = format!("{} --resume {}", account.command(), fork_args(from, new_id, name));
    let mut env = vec![(crate::start::SESSION_VAR.to_string(), key.clone())];
    let prompt = prompt.trim();
    if !prompt.is_empty() {
        input.push_str(&format!(" \"${}\"", crate::start::PROMPT_VAR));
        env.push((crate::start::PROMPT_VAR.to_string(), crate::start::as_argument(prompt)));
    }
    input.push('\n');
    crate::harness::Typed { input, env, key: Some(key) }
}

/// What `--resume` takes to fork `from` (an ID, or a transcript's path quoted for the shell)
/// into a copy named `new_id`, with `--name` when `name` is set (checked on 2.1.329: sets the
/// title in memory and writes `custom-title` on the copy's first write, leaving a closed copy with
/// no transcript). While nothing has been written under that ID, the same arguments make the copy
/// again: a copy whose process ended before its first prompt left no transcript, and a second run
/// with its `--session-id` started it as the first had (checked on 2.1.288, 2026-10-03).
pub(super) fn fork_args(from: &str, new_id: &str, name: Option<&str>) -> String {
    format!("{from} --fork-session --session-id {new_id}{}", name_arg(name))
}

/// The terminal input that resumes a Claude Code session, on `model` and `effort` when they're
/// asked for and with a prompt when there's one, and the environment that carries the prompt
/// (`Adapter::resume`). `from` is what `--resume` takes: the session's ID, or `fork_args` for a
/// moved copy made again. Without a prompt it opens on the conversation and waits for one.
pub fn claude_resume_input(account: &AccountCfg, from: &str, text: &str, model: Option<&str>, effort: Option<&str>) -> (String, Vec<(String, String)>) {
    let (has_images, has_files) = attachments::kinds(text);
    let mut input = account.command().to_string();
    let mut env = Vec::new();
    for (has, var, dir) in [(has_images, crate::start::IMAGES_VAR, attachments::images_dir()), (has_files, crate::start::FILES_VAR, attachments::files_dir())] {
        if has {
            input.push_str(&format!(" --add-dir \"${var}\""));
            env.push((var.to_string(), dir.to_string_lossy().to_string()));
        }
    }
    if let Some(m) = model.filter(|m| crate::start::valid_model(m)) {
        input.push_str(&format!(" --model '{m}'"));
    }
    if let Some(e) = effort {
        input.push_str(&format!(" --effort {e}"));
    }
    input.push_str(&format!(" --resume {from}"));
    let prompt = attachments::expand(text, Style::Mention);
    if !prompt.trim().is_empty() {
        input.push_str(&format!(" \"${}\"", crate::start::PROMPT_VAR));
        env.push((crate::start::PROMPT_VAR.to_string(), crate::start::as_argument(prompt.trim())));
    }
    input.push('\n');
    (input, env)
}

/// The command that goes on with a session as a background job: `--bg` with `--resume`, on
/// `model` and `effort` when they're asked for, with `keep` a folder it may still read, and on
/// `prompt` when there's one, else waiting for one. `from` is what `--resume` takes. While no
/// background job has had the session it goes on as itself; one that has is copied into a job
/// under a new ID (`fork: true` in the roster; checked on 2.1.289, 2026-10-05).
pub(super) fn resume_job_command(account: &AccountCfg, from: &str, keep: Option<&Path>, prompt: &str, model: Option<&str>, effort: Option<&str>) -> String {
    let keep = keep.map(|d| format!(" --add-dir={}", crate::util::sh_quote(&d.to_string_lossy()))).unwrap_or_default();
    let dirs = (attachments::images_dir(), attachments::files_dir());
    format!("{} --resume {from}{keep}{}", account.command(), background_args(prompt, model, effort, false, (&dirs.0, &dirs.1)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_background_job_isolates_itself_unless_its_settings_say_none() {
        let dir = crate::fixtures::scratch("bg-isolation");
        // The account's, the repo's, its local ones and the managed ones: scratch files all.
        let files: Vec<PathBuf> = ["account.json", "repo.json", "local.json", "managed.json"].iter().map(|f| dir.join(f)).collect();
        let set = |i: usize, text: &str| std::fs::write(&files[i], text).unwrap();
        let isolated = |launched: Option<&str>| isolated_by(&files, launched);
        assert!(isolated(None), "with no file, it isolates");
        set(0, r#"{"worktree":{"bgIsolation":"none"}}"#);
        assert!(!isolated(None));
        set(1, r#"{"worktree":{"bgIsolation":"worktree"}}"#);
        assert!(isolated(None), "the repo's over the account's");
        set(2, r#"{"worktree":{"bgIsolation":"none"}}"#);
        assert!(!isolated(None), "the local ones over the repo's");
        set(3, r#"{"worktree":{"bgIsolation":"worktree"}}"#);
        assert!(isolated(None), "the managed ones over all");
        set(3, r#"{"worktree":{"bgIsolation":"off"}}"#);
        assert!(!isolated(None), "a word it doesn't take leaves the setting unset in that file");
        // The variable, launched with or from any file's `env`, wins over the setting in a file
        // ranked higher, and a file's over the one it was launched with.
        assert!(isolated(Some("worktree")));
        set(0, r#"{"env":{"CLAUDE_BG_ISOLATION":"worktree"}}"#);
        assert!(isolated(None));
        assert!(isolated(Some("none")), "a file's variable over the one it was launched with");
        set(3, "not json");
        set(1, r#"{"env":{"CLAUDE_BG_ISOLATION":"none"}}"#);
        assert!(!isolated(None), "the higher file's variable, a broken file passed over");
        // The variable is replaced before its value is looked at: an empty one in a higher file
        // says nothing, and the setting decides.
        set(1, "{}");
        set(3, r#"{"env":{"CLAUDE_BG_ISOLATION":""},"worktree":{"bgIsolation":"none"}}"#);
        assert!(!isolated(None), "the managed variable empties the account's, and its setting decides");
        assert!(!isolated(Some("worktree")));
        set(0, "{}");
        set(2, "{}");
        set(3, "{}");
        assert!(!isolated(Some("none")), "launched with none, nothing replacing it");
        assert!(isolated(Some("")), "an empty one says nothing");
    }

    #[test]
    fn stops_a_job_by_the_id_it_attaches_to() {
        let a = crate::config::AccountCfg { name: "claude2".into(), harness: "claude".into(), home: "~/.claude2".into(), command: None, start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false, chrome_profile: None };
        assert_eq!(stop_command(&a, "claude2 attach 6e07c149").as_deref(), Some("claude2 stop 6e07c149"));
        assert_eq!(stop_command(&a, "claude2 attach 6e07c149; rm -rf ~"), None);
        assert_eq!(stop_command(&a, "claude2 --resume 6e07c149"), None);
    }

    #[test]
    fn forks_under_an_id_it_picks() {
        let a = crate::config::AccountCfg { name: "claude2".into(), harness: "claude".into(), home: "~/.claude2".into(), command: None, start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false, chrome_profile: None };
        let f = fork_as(&a, "4f1c2a9e-0b7d-4e21-9c55-1a2b3c4d5e6f", "9d8c7b6a-5f4e-4d3c-8b2a-1f0e9d8c7b6a", "", None);
        assert_eq!(f.input, "claude2 --resume 4f1c2a9e-0b7d-4e21-9c55-1a2b3c4d5e6f --fork-session --session-id 9d8c7b6a-5f4e-4d3c-8b2a-1f0e9d8c7b6a\n");
        assert_eq!(f.key.as_deref(), Some("claude:claude2:9d8c7b6a-5f4e-4d3c-8b2a-1f0e9d8c7b6a"));
        assert_eq!(f.env, vec![("CODEALONG_SESSION".to_string(), "claude:claude2:9d8c7b6a-5f4e-4d3c-8b2a-1f0e9d8c7b6a".to_string())]);
    }

    #[test]
    fn moves_a_session_to_another_folder_as_itself() {
        use crate::harness::Adapter;
        let id = "4f1c2a9e-0b7d-4e21-9c55-1a2b3c4d5e6f";
        let home = crate::fixtures::scratch("move-folder");
        let a = crate::config::AccountCfg { name: "claude2".into(), harness: "claude".into(), home: home.to_string_lossy().into(), command: None, start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false, chrome_profile: None };
        let ctx = crate::harness::Ctx { account: &a };
        let s = Session::new(Harness::Claude, "claude2", id);
        // It resumes by its own ID wherever the terminal opens, so its key stays.
        let moved = super::super::ClaudeCode.move_folder(&ctx, &s, None, "You moved").unwrap();
        assert_eq!(moved.input, format!("claude2 --resume {id} \"$CODEALONG_PROMPT\"\n"));
        assert_eq!(moved.key.as_deref(), Some(s.key.as_str()));
        assert!(moved.env.contains(&("CODEALONG_SESSION".to_string(), s.key.clone())));
        assert!(moved.env.contains(&("CODEALONG_PROMPT".to_string(), "You moved".to_string())));
        // A chat's folder stays readable, given whole before --resume so it takes nothing more.
        let from_chat = super::super::ClaudeCode.move_folder(&ctx, &s, Some(Path::new("/c/chats/2026-10-05-a1b2c3")), "You moved").unwrap();
        assert_eq!(from_chat.input, format!("claude2 --add-dir='/c/chats/2026-10-05-a1b2c3' --resume {id} \"$CODEALONG_PROMPT\"\n"));
    }

    #[test]
    fn moves_a_session_by_resuming_its_transcript_under_the_other_account() {
        use crate::harness::Adapter;
        let home = crate::fixtures::scratch("move");
        let id = "4f1c2a9e-0b7d-4e21-9c55-1a2b3c4d5e6f";
        // Resumed from another folder once, it has a transcript in each; the later one goes on.
        let (old, new) = (home.join("projects").join("-r-old"), home.join("projects").join("-r-it's"));
        for dir in [&old, &new] {
            std::fs::create_dir_all(dir).unwrap();
            std::fs::write(dir.join(format!("{id}.jsonl")), "{}\n").unwrap();
        }
        let earlier = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
        std::fs::File::options().write(true).open(old.join(format!("{id}.jsonl"))).unwrap().set_modified(earlier).unwrap();
        let from = crate::config::AccountCfg { name: "claude".into(), harness: "claude".into(), home: home.to_string_lossy().into(), command: None, start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false, chrome_profile: None };
        let to = crate::config::AccountCfg { name: "claude2".into(), harness: "claude".into(), home: "~/.claude2".into(), command: None, start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false, chrome_profile: None };
        let ctx = crate::harness::Ctx { account: &from };
        let moved = super::super::ClaudeCode.move_to(&ctx, &Session::new(Harness::Claude, "claude", id), &to, "").unwrap();
        let path = new.join(format!("{id}.jsonl")).to_string_lossy().replace('\'', "'\\''");
        let key = moved.key.clone().unwrap();
        let copy = key.strip_prefix("claude:claude2:").unwrap();
        assert_ne!(copy, id);
        assert_eq!(moved.input, format!("claude2 --resume '{path}' --fork-session --session-id {copy}\n"));
        assert_eq!(moved.env, vec![("CODEALONG_SESSION".to_string(), key.clone())]);
        // Moved with a prompt, the copy starts on it, which comes in the environment.
        let going = super::super::ClaudeCode.move_to(&ctx, &Session::new(Harness::Claude, "claude", id), &to, "-go on").unwrap();
        let copy = going.key.clone().unwrap();
        let copy_id = copy.strip_prefix("claude:claude2:").unwrap();
        assert_eq!(going.input, format!("claude2 --resume '{path}' --fork-session --session-id {copy_id} \"$CODEALONG_PROMPT\"\n"));
        assert_eq!(going.env, vec![("CODEALONG_SESSION".to_string(), copy.clone()), ("CODEALONG_PROMPT".to_string(), " -go on".to_string())]);
        // A titled session carries its name onto the moved copy so Claude Code doesn't re-title it.
        let named = Session { title: "northwind 3d model's mocks".into(), ..Session::new(Harness::Claude, "claude", id) };
        let kept = super::super::ClaudeCode.move_to(&ctx, &named, &to, "").unwrap();
        let kept_key = kept.key.clone().unwrap();
        let kept_id = kept_key.strip_prefix("claude:claude2:").unwrap();
        assert_eq!(kept.input, format!("claude2 --resume '{path}' --fork-session --session-id {kept_id} --name 'northwind 3d model'\\''s mocks'\n"));
        // The fallback "Session <short id>" and an untitled first prompt aren't locked in as a name.
        let fallback = Session { title: format!("Session {}", crate::model::short_id(id)), ..Session::new(Harness::Claude, "claude", id) };
        assert!(!super::super::ClaudeCode.move_to(&ctx, &fallback, &to, "").unwrap().input.contains("--name"));
        let prompt_only = Session { title: "Fix the billing chart".into(), first_prompt: Some("Fix the billing chart".into()), ..Session::new(Harness::Claude, "claude", id) };
        assert!(!super::super::ClaudeCode.move_to(&ctx, &prompt_only, &to, "").unwrap().input.contains("--name"));
        // A live session that hasn't said anything has no transcript yet.
        let quiet = Session { live: true, ..Session::new(Harness::Claude, "claude", "0f5e1d2c") };
        assert!(super::super::ClaudeCode.move_to(&ctx, &quiet, &to, "").unwrap_err().to_string().contains("Nothing's been said"));
    }

    #[test]
    fn forks_into_the_background_with_the_prompt_quoted_last() {
        let a = AccountCfg { name: "claude2".into(), harness: "claude".into(), home: "~/.claude2".into(), command: Some("claude2".into()), start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false, chrome_profile: None };
        let dirs = (Path::new("/i"), Path::new("/f"));
        let id = "3628c9e6-0309-4a30-aed9-3e68d0e66b7e";
        assert_eq!(fork_command_in(&a, id, "", None, dirs), format!("claude2 --resume {id} --fork-session --bg"));
        assert_eq!(fork_command_in(&a, id, "  try it the other way's round  ", None, dirs), format!("claude2 --resume {id} --fork-session --bg -- 'try it the other way'\\''s round'"));
        assert_eq!(
            fork_command_in(&a, id, "look\n![image](images/0123456789abcdef.png)", None, dirs),
            format!("claude2 --resume {id} --fork-session --add-dir '/i' --bg -- 'look\n@\"/i/0123456789abcdef.png\"'")
        );
        assert_eq!(
            fork_command_in(&a, id, "go on", Some("northwind 3d model ui mocks"), dirs),
            format!("claude2 --resume {id} --fork-session --name 'northwind 3d model ui mocks' --bg -- 'go on'")
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
        let a = AccountCfg { name: "claude".into(), harness: "claude".into(), home: "~/.claude".into(), command: Some("claude".into()), start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false, chrome_profile: None };
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
