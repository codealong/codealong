//! Claude Code, and the Claude app's Code sessions. Everything Codealong knows of their files,
//! sockets and commands is in this folder: here the adapter (`ClaudeCode`), which answers for a
//! session and for an account, and the reader, which keeps the registry and the transcripts
//! read for the board. Beside it: the live registry and the background service's records
//! (registry.rs), transcripts (transcript.rs) and the conversation one holds (conversation.rs),
//! the permission mode a session runs in (mode.rs), the requests the plugin's hook hands the app
//! (asks.rs), the Claude app's own records (app.rs), the ways a prompt gets in (routes.rs, by
//! the background service's socket in daemon.rs, a session's own in peer.rs, or the app), starting,
//! forking and stopping (start.rs), names (names.rs), side conversations (side.rs), what a
//! session's Bash tool is running (shells.rs), the folders an account trusts (trust.rs), the skills it
//! reads (skills.rs), plan usage (usage.rs), whether Anthropic may train on the account
//! (training.rs), and a repo's hooks and a job's record as a worktree
//! is removed (teardown.rs). See .claude/rules/claude-code.md.

pub mod app;
pub mod asks;
mod builtin;
mod conversation;
pub mod daemon;
pub mod held;
mod mode;
mod names;
pub mod peer;
mod registry;
mod routes;
mod shells;
mod side;
mod skills;
mod start;
mod switch;
mod teardown;
mod training;
mod transcript;
mod trust;
mod usage;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::config::AccountCfg;
use crate::harness::{Applied, Look, NewLines};
use crate::model::{Harness, OriginHow, Session, SessionOrigin, Usage};
use crate::procs;
use crate::transcript::{Item, Page};
use crate::util::{file_name, mtime_ms, one_line, parallel_map, parse_rfc3339_ms};

use conversation::{idle_since, last_prompt_taken_back, mark_taken_back, open_prompt_before, questions_of};
use daemon::{daemon_press_escape, daemon_socket, daemon_type};
use held::held_in;
use mode::permission_mode;
pub use mode::session_mode;
use names::name_transcript;
use registry::{read_registry, registry_of, session_entry, Forks, INPUT_NEEDED, PERMISSION_PROMPT};
use routes::claude_routes;
use side::{side_mode, SideCopy};
use start::{background_args, claude_resume_input, default_model, fork_args, fork_as, fork_command, resume_job_command, start_background, stop_command, EFFORTS, MODELS};
use transcript::{parse_transcript, reply_text, tool_request, transcript_cwd, transcript_files, typed_text, PLAN, Parsed};
use trust::{state_file, trust_folder};

pub use conversation::conversation_items;
pub use held::Held;
pub use registry::RegistryEntry;
pub use transcript::TranscriptInfo;

/// Claude Code's send-now key, Ctrl+X Ctrl+S (`chat:sendNow`; Ctrl+Enter is bound to it too, but
/// a terminal without the kitty keyboard protocol sends it as Return). Pressed with a prompt in
/// the box, it submits it, and the running turn takes it in at once: a command it was running
/// carries on in the background ("moved to the background so that a message that arrived while it
/// was running can reach you"), where Return would wait for the command to end. Checked on
/// 2.1.287 in tmux and through the background service's attach (2026-10-02). The two keys may
/// arrive together, but not with the paste before them, which they'd be taken as part of.
pub const SEND_NOW: &str = "\x18\x13";

/// Whether keys pressed for it reach it: in the terminal it runs in, where Codealong can press
/// one there, or, for a background job, through its account's service.
fn keys_reach(ctx: &crate::harness::Ctx, s: &Session) -> bool {
    if s.surface != crate::model::Surface::Cli {
        return false;
    }
    if s.attach_command.is_some() {
        return daemon_socket(&ctx.account.home_path()).is_some();
    }
    s.pid.and_then(crate::launch::terminal_kind).is_some_and(|k| k.presses_keys())
}

/// Claude Code (`harness::Adapter`): its transcript for the conversation, and its permission
/// prompt answered through the plugin's hook when the hook handed the app the request
/// (asks.rs), else by a keypress in the terminal it runs in, once its own record says it's
/// waiting on that prompt and its transcript that the call is the one you saw.
pub struct ClaudeCode;

impl ClaudeCode {
    /// The transcript a move goes on from: a session resumed from another folder can have one
    /// in each, and the newest goes on.
    fn newest_record(&self, ctx: &crate::harness::Ctx, s: &Session) -> anyhow::Result<PathBuf> {
        use crate::harness::Adapter;
        let newest = self.records(ctx.account, s).into_iter().max_by_key(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok());
        newest.ok_or_else(|| anyhow::anyhow!(if s.live { "Nothing's been said in it yet, so there's nothing to move." } else { "Its transcript isn't on this Mac any more." }))
    }
}

impl crate::harness::Adapter for ClaudeCode {
    fn harness(&self) -> crate::model::Harness {
        crate::model::Harness::Claude
    }

    fn spec(&self) -> &'static crate::detect::Spec {
        const SPEC: crate::detect::Spec = crate::detect::Spec { harness: "claude", program: Some("claude"), default_command: Some("claude"), var: Some(("CLAUDE_CONFIG_DIR", None)), default_dir: "~/.claude", siblings: Some(".claude") };
        &SPEC
    }

    /// The keychain holds its sign-in (only whether it does is asked), and its state file the
    /// email.
    fn signed_in(&self, home: &Path) -> (bool, Option<String>) {
        let email = std::fs::read(state_file(home))
            .ok()
            .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
            .and_then(|v| v.pointer("/oauthAccount/emailAddress").and_then(Value::as_str).map(str::to_string));
        (usage::claude_signed_in(home), email)
    }

    fn installed(&self, home: &Path, on_path: &dyn Fn(&str) -> bool) -> bool {
        on_path("claude") || crate::detect::is_executable(&home.join(".local/bin/claude")) || crate::detect::is_executable(&home.join(".claude/local/claude"))
    }

    fn folder_use(&self, dir: &Path) -> crate::harness::FolderUse {
        let projects = dir.join("projects");
        let mut paths = vec![dir.join("history.jsonl"), dir.join("sessions")];
        paths.extend(std::fs::read_dir(&projects).into_iter().flatten().flatten().map(|e| e.path()));
        crate::harness::FolderUse {
            has_data: projects.is_dir() || dir.join("history.jsonl").is_file(),
            last_used: paths.iter().filter_map(|p| mtime_ms(p)).max(),
            // Claude Code keeps sign-ins in the keychain on a Mac, and in the folder elsewhere.
            signed_in: dir.join(".credentials.json").is_file(),
        }
    }

    /// `<config dir>/skills`: a second account's config dir is its home here.
    fn skills_dir(&self, account: &crate::config::AccountCfg) -> Option<PathBuf> {
        Some(account.home_path().join("skills"))
    }

    /// The `.claude` folder of each, as Claude Code looks up the tree.
    fn worktree_skills(&self, folders: &[PathBuf]) -> Vec<crate::skills::Root> {
        folders.iter().flat_map(|dir| skills::in_folder(&dir.join(".claude"))).collect()
    }

    fn account_skills(&self, account: &crate::config::AccountCfg) -> Vec<crate::skills::Root> {
        skills::of_account(&account.home_path())
    }

    fn builtin_skills(&self, account: &crate::config::AccountCfg, wait: std::time::Duration) -> Vec<crate::skills::Builtin> {
        builtin::listed(&account.home_path(), wait)
    }

    /// A skill or a command is run as a slash command, as a first prompt or typed.
    fn skill_mention(&self, name: &str) -> Option<String> {
        Some(format!("/{name}"))
    }

    fn skill_uses(&self, account: &crate::config::AccountCfg) -> std::collections::HashMap<String, (u32, i64)> {
        skills::uses(&account.home_path())
    }

    /// Codealong's plugin (`claude plugin`): its skill, its `bin/codealong` on the Bash tool's
    /// PATH, and the hooks that hand a session its todo.
    fn installs_plugin(&self) -> bool {
        true
    }

    fn acp_command(&self, account: &crate::config::AccountCfg) -> std::result::Result<(String, Vec<(String, String)>), String> {
        Ok(acp_launch(crate::acp::package_program("claude-agent-acp", self.harness())?, &account.home_path()))
    }

    /// The Claude app runs the account in `~/.claude`.
    fn places(&self, account: &crate::config::AccountCfg, home: &Path) -> std::result::Result<Vec<crate::start::Place>, String> {
        use crate::start::Place;
        Ok(if account.home_path() == home.join(".claude") {
            vec![Place::Terminal, Place::Background, Place::ClaudeApp, Place::Codealong]
        } else {
            vec![Place::Terminal, Place::Background, Place::Codealong]
        })
    }

    fn session_cwds(&self, home: &Path, since: i64) -> Vec<String> {
        crate::util::parallel_map(transcript_files(home, since, &HashSet::new()), |(p, _)| transcript_cwd(&p))
    }

    /// Its transcript, looked up by id under every project folder: for a job that has written
    /// none of its own yet, the one of the conversation it goes on from.
    fn records(&self, account: &crate::config::AccountCfg, s: &crate::model::Session) -> Vec<PathBuf> {
        records_in(account, s, home_of_account)
    }

    fn conversation(&self, ctx: &crate::harness::Ctx, s: &crate::model::Session, before: Option<u64>, limit: usize) -> anyhow::Result<Page> {
        match record(self, ctx, s) {
            Ok(path) => {
                let mut page = crate::transcript::file_page(&path, conversation_items, before, limit)?;
                mark_taken_back(&path, &mut page.items, idle_since(&ctx.account.home_path(), s));
                Ok(page)
            }
            // A live session writes its transcript at its first prompt.
            Err(_) if s.live => Ok(Page { items: Vec::new(), before: None, after: 0 }),
            Err(e) => Err(e),
        }
    }

    fn routes(&self, account: &crate::config::AccountCfg, s: &crate::model::Session) -> Vec<crate::send::Route> {
        let home = account.home_path();
        let registry = session_entry(&home, s);
        let mode = permission_mode(registry.as_ref(), &self.records(account, s));
        claude_routes(s, registry.as_ref(), &home, daemon_socket(&home).as_deref(), mode)
    }

    fn skips_permission_prompts(&self, ctx: &crate::harness::Ctx, s: &crate::model::Session) -> bool {
        let registry = session_entry(&ctx.account.home_path(), s);
        registry.is_some() && permission_mode(registry.as_ref(), &self.records(ctx.account, s)).as_deref() == Some("bypassPermissions")
    }

    fn conversation_after(&self, ctx: &crate::harness::Ctx, s: &crate::model::Session, after: u64) -> anyhow::Result<Page> {
        match record(self, ctx, s) {
            // A prompt with nothing back for it may have been taken back since it was read, so it
            // comes again; the panel puts it in place of the one it has, by its cursor.
            Ok(path) => {
                let from = open_prompt_before(&path, after).unwrap_or(after);
                let mut page = crate::transcript::file_newer(&path, conversation_items, from)?;
                mark_taken_back(&path, &mut page.items, idle_since(&ctx.account.home_path(), s));
                Ok(page)
            }
            Err(_) if s.live => Ok(Page { items: Vec::new(), before: None, after }),
            Err(e) => Err(e),
        }
    }

    fn pending(&self, ctx: &crate::harness::Ctx, s: &crate::model::Session) -> Option<crate::harness::Pending> {
        // A request the plugin's hook handed the app is answered through the hook, wherever the
        // session runs (asks.rs).
        if let Some(a) = asks::of_session(&s.id) {
            let mut p = asks::asked_pending(&a);
            // Claude Code writes the message a question comes in only once it's answered, so what
            // the agent wrote with it is in its terminal but not yet in the conversation, and the
            // panel says where to read it. Only questions: another call's line is written while
            // its prompt is up, maybe a moment after the hook asks. Reading that text live
            // through a `MessageDisplay` hook was tried and left out (2026-10-06): Claude Code
            // reads hooks once, as a session starts, so one can't be on only for the sessions
            // Codealong shows, and any such hook makes every session's terminal show a reply a
            // paragraph at a time instead of a line at a time (.claude/rules/claude-code.md).
            // Worth another look if Claude Code writes the message before it asks, or gets a hook
            // that watches without holding its display.
            if a.tool == asks::QUESTIONS {
                let page = record(self, ctx, s).ok().and_then(|path| crate::transcript::file_page(&path, conversation_items, None, 40).ok());
                p.unwritten = page.is_some_and(|page| !asks::written_in(&page.items, &a));
            }
            return Some(p);
        }
        // Questions the hook didn't hand over (the plugin isn't in the account, or the app started
        // after they were asked) still show here, to answer where the session runs.
        if let Some((call, _)) = waiting_on(self, ctx, s, INPUT_NEEDED).filter(|(c, _)| !c.questions.is_empty()) {
            return Some(crate::harness::Pending {
                id: call.id.clone().unwrap_or_default(),
                label: format!("Asks you · {}", one_line(&call.questions[0].question, 160)),
                input: None,
                cannot: Some("Codealong didn't get these as they were asked, so they're answered where the session runs.".into()),
                always: false,
                no_once: false,
                plan: false,
                message: false,
                always_rule: None,
                questions: call.questions,
                unwritten: false,
                terminal: None,
            });
        }
        // Another session's message it holds for you: its dialog comes before any call made
        // before it, and a turn goes on underneath it.
        if let Some((held, reg)) = waiting_hold(self, ctx, s) {
            return Some(held_pending(ctx, s, &held, &reg));
        }
        let (call, reg) = waiting_call(self, ctx, s)?;
        // A background job's prompt is answered through its account's background service, as a
        // held message is; a terminal's, by a key pressed there.
        let cannot = cannot_press(ctx, &reg);
        // Not every prompt has "Yes, and always allow" as its second choice, and a 2 pressed at
        // one without it is something else, so Always isn't offered in a terminal.
        let plan = call.text == PLAN;
        Some(crate::harness::Pending { id: call.id.clone().unwrap_or_default(), label: call.text, input: call.input, cannot, always: false, no_once: false, plan, message: false, always_rule: None, questions: Vec::new(), unwritten: false, terminal: None })
    }

    fn answer(&self, ctx: &crate::harness::Ctx, s: &crate::model::Session, id: &str, answer: crate::harness::Answer) -> anyhow::Result<String> {
        if let Some(held) = id.strip_prefix(HELD_ID) {
            return answer_held(self, ctx, s, held, answer);
        }
        let decision = match answer {
            crate::harness::Answer::Allow => asks::Decision::Allow,
            crate::harness::Answer::Always => asks::Decision::Always,
            crate::harness::Answer::Deny => asks::Decision::Deny,
        };
        if let Some(r) = asks::answer(id, decision) {
            let a = r?;
            let call = tool_request(&a.tool, &a.input);
            return Ok(match answer {
                crate::harness::Answer::Allow => format!("Allowed {call}."),
                crate::harness::Answer::Always => format!("Allowed {call}, and from now on."),
                crate::harness::Answer::Deny => format!("Denied {call}; it stops and waits for you."),
            });
        }
        if asks::is_ask_id(id) {
            anyhow::bail!("It isn't waiting on that any more.");
        }
        let Some((call, reg)) = waiting_call(self, ctx, s) else {
            anyhow::bail!("It isn't waiting on a permission prompt any more.");
        };
        if call.id.as_deref() != Some(id) {
            anyhow::bail!("It's waiting on a different call now: {}. Look again before answering.", call.text);
        }
        // 1 is "Yes" in every prompt; Escape cancels it, which denies the call and ends the turn.
        // But with `showClearContextOnPlanAccept` on, a plan's 1 is "Yes, clear context…", which
        // would wipe the conversation: that one is answered in its terminal, never here.
        if call.text == PLAN && answer == crate::harness::Answer::Allow && clears_context_first(&ctx.account.home_path(), Path::new(&s.cwd)) {
            anyhow::bail!("Its plan prompt puts \"clear context\" first (showClearContextOnPlanAccept is on in its settings), so Codealong doesn't press a key for it. Answer it in its terminal.");
        }
        let key = match answer {
            crate::harness::Answer::Allow => crate::launch::Key::Digit(1),
            crate::harness::Answer::Deny => crate::launch::Key::Escape,
            crate::harness::Answer::Always => anyhow::bail!("Claude Code's prompt in a terminal is answered here with Allow or Deny only."),
        };
        if reg.kind.as_deref() == Some("bg") {
            // A background job has no terminal of its own: the key is typed into the one its
            // account's background service keeps for it, as `claude attach` would.
            let home = ctx.account.home_path();
            let job = reg.job_id.as_deref().ok_or_else(|| anyhow::anyhow!("Claude Code didn't record which job it is."))?;
            let socket = daemon_socket(&home).ok_or_else(|| anyhow::anyhow!("Its account's background service isn't running."))?;
            daemon_type(&socket, &home.join("daemon/control.key"), job, &[key.typed().as_bytes()])?;
        } else {
            let Some(host) = crate::launch::terminal_host(reg.pid, Path::new(&s.cwd))? else {
                anyhow::bail!("It isn't in a terminal Codealong can press keys in ({}). Open it to answer.", crate::launch::presses_keys_in());
            };
            crate::launch::press_key(&host, key)?;
        }
        Ok(match answer {
            crate::harness::Answer::Allow | crate::harness::Answer::Always => format!("Allowed {}.", call.text),
            crate::harness::Answer::Deny => format!("Denied {}; it stops and waits for you.", call.text),
        })
    }

    /// Through the hook, which refuses the call with your words as its result and lets the turn
    /// go on. A prompt answered by a keypress can't take them: Escape ends the turn, and
    /// actions.rs types them once it no longer waits.
    fn deny_saying(&self, _ctx: &crate::harness::Ctx, _s: &crate::model::Session, id: &str, words: &str) -> anyhow::Result<Option<String>> {
        let Some(r) = asks::answer(id, asks::Decision::DenySaying(words.to_string())) else { return Ok(None) };
        let a = r?;
        Ok(Some(format!("Denied {}, and told it what to do instead.", tool_request(&a.tool, &a.input))))
    }

    fn answer_questions(&self, _ctx: &crate::harness::Ctx, s: &crate::model::Session, id: &str, answers: &Value) -> anyhow::Result<String> {
        let Some(asked) = asks::of_session(&s.id).filter(|a| a.id == id) else {
            anyhow::bail!("It isn't waiting on that any more.");
        };
        let answers = asks::picked(&questions_of(&asked.input), answers)?;
        let said = answers.iter().map(|(q, a)| format!("{} → {}", one_line(q, 80), a.as_str().map(String::from).unwrap_or_else(|| a.as_array().map(|l| l.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(", ")).unwrap_or_default()))).collect::<Vec<_>>().join("; ");
        match asks::answer(id, asks::Decision::Answers(Value::Object(answers))) {
            Some(r) => r.map(|_| format!("Answered: {said}.")),
            None => anyhow::bail!("It isn't waiting on that any more."),
        }
    }

    fn open_calls(&self, ctx: &crate::harness::Ctx, s: &crate::model::Session) -> Vec<String> {
        record(self, ctx, s).map(|p| open_calls(&p)).unwrap_or_default()
    }

    /// Its Bash tool's shells, background ones included, are the session process's children.
    fn runs_its_commands(&self) -> bool {
        true
    }

    /// Set in its Bash tool's shells and kept by what they start: an eval's driver an agent
    /// started on its own (`start_new_session`) and left to launchd still had it, and so did
    /// each `claude -p` its judge ran (2.1.296).
    fn session_var(&self) -> Option<&'static str> {
        Some("CLAUDE_CODE_SESSION_ID")
    }

    fn fork(&self, ctx: &crate::harness::Ctx, s: &crate::model::Session) -> anyhow::Result<crate::harness::Typed> {
        Ok(fork_as(ctx.account, &shell_word(&resume_from(ctx.account, s)), &crate::start::new_session_id(), ""))
    }

    /// A background job under the account's service, whichever way the session runs: Claude
    /// Code copies the conversation from its transcript (`fork_command`).
    fn fork_background(&self, ctx: &crate::harness::Ctx, s: &crate::model::Session, cwd: &Path, prompt: &str) -> anyhow::Result<Option<String>> {
        let command = fork_command(ctx.account, &shell_word(&resume_from(ctx.account, s)), prompt);
        let (_, id) = start_background(ctx.account, cwd, &command)?;
        Ok(id.map(|id| Session::key_for(Harness::Claude, &ctx.account.name, &id)))
    }

    /// `--resume` takes a transcript's path as well as a session's ID (2.1.287), from any
    /// account's folder, and the copy is written under the account that runs it. Another org's
    /// account goes on with it, signed thinking blocks and all (checked 2026-10-02: a Sonnet
    /// conversation from one org's account, resumed under another's in a terminal and with
    /// `-p`). Its first reply there reads the whole conversation uncached, since a prompt cache
    /// is its org's own.
    fn move_to(&self, ctx: &crate::harness::Ctx, s: &crate::model::Session, to: &AccountCfg, prompt: &str) -> anyhow::Result<crate::harness::Typed> {
        Ok(fork_as(to, &crate::util::sh_quote(&self.newest_record(ctx, s)?.to_string_lossy()), &crate::start::new_session_id(), prompt))
    }

    fn moves_accounts(&self) -> bool {
        true
    }

    /// Anthropic's API, which a Claude.ai sign-in uses. An account sent elsewhere
    /// (`ANTHROPIC_BASE_URL`, Bedrock, Vertex) is tried here all the same: a Mac that reaches
    /// one after a sleep reaches the other.
    fn api_address(&self) -> Option<&'static str> {
        Some("api.anthropic.com:443")
    }

    /// A background job takes the transcript's path too: `--bg --resume <path> --fork-session`
    /// under the other account starts a job there that goes on from it (checked on 2.1.287,
    /// 2026-10-02), its session ID the service's choice, as any `--bg` start's is.
    fn move_to_background(&self, ctx: &crate::harness::Ctx, s: &crate::model::Session, to: &AccountCfg, cwd: &Path, prompt: &str) -> anyhow::Result<Option<String>> {
        let command = fork_command(to, &crate::util::sh_quote(&self.newest_record(ctx, s)?.to_string_lossy()), prompt);
        let (_, id) = start_background(to, cwd, &command)?;
        Ok(id.map(|id| Session::key_for(Harness::Claude, &to.name, &id)))
    }

    /// Claude Code goes on with a session in whatever folder `--resume` runs in, as itself: its
    /// transcript stays where it was, in the first folder's project, and its lines say the
    /// folder it's in now (checked on 2.1.289, 2026-10-05, a session that had been a background
    /// job too). The folder it may still read comes before `--resume`, whole, so it takes
    /// nothing after it.
    fn move_folder(&self, ctx: &crate::harness::Ctx, s: &crate::model::Session, keep: Option<&Path>, prompt: &str) -> anyhow::Result<crate::harness::Typed> {
        let (input, mut env) = claude_resume_input(ctx.account, &resume_args(ctx.account, s, home_of_account), prompt, None, None);
        let input = match keep {
            Some(dir) => input.replacen(" --resume ", &format!(" --add-dir={} --resume ", crate::util::sh_quote(&dir.to_string_lossy())), 1),
            None => input,
        };
        env.push((crate::start::SESSION_VAR.to_string(), s.key.clone()));
        Ok(crate::harness::Typed { input, env, key: Some(s.key.clone()) })
    }

    /// As itself, unless a background job has had it (`resume_job_command`); the roster says which.
    fn move_folder_background(&self, ctx: &crate::harness::Ctx, s: &crate::model::Session, cwd: &Path, keep: Option<&Path>, prompt: &str) -> anyhow::Result<Option<String>> {
        let command = resume_job_command(ctx.account, &resume_args(ctx.account, s, home_of_account), keep, prompt, None, None);
        let (_, id) = start_background(ctx.account, cwd, &command)?;
        Ok(id.map(|id| Session::key_for(Harness::Claude, &ctx.account.name, &id)))
    }

    /// Claude Code run headless on a fork of the session that keeps no record (checked on
    /// 2.1.286, 2026-09-30): `--resume <id> --fork-session --no-session-persistence` reads the
    /// conversation as far as it's written and writes no transcript. It runs in the session's
    /// own permission mode (`side_mode`), so it may do what the session may without asking, and
    /// with nobody to ask, whatever would ask is refused: in the usual mode an edit or a command
    /// that changes things, while reads and commands that only look go ahead. Nothing else
    /// about the run is changed (its tools, its system prompt, the account's usual model), so
    /// its first question reads the session's prompt cache rather than paying for the whole
    /// conversation again: a 35,000-token haiku session's copy read 35,120 tokens from the cache
    /// and wrote 594. A session on another model than the account's usual one gets its answers
    /// from the usual one: a transcript names the model without its context size, so the
    /// session's own can't be asked for by name.
    fn side(&self, ctx: &crate::harness::Ctx, s: &crate::model::Session, _cwd: &Path) -> anyhow::Result<crate::side::Recipe> {
        let id = resume_from(ctx.account, s);
        let home = ctx.account.home_path();
        let registry = session_entry(&home, s);
        let mode = side_mode(permission_mode(registry.as_ref(), &self.records(ctx.account, s)).as_deref());
        let args = ["-p", "--input-format", "stream-json", "--output-format", "stream-json", "--verbose", "--include-partial-messages", "--resume", &id, "--fork-session", "--no-session-persistence", "--permission-mode", mode];
        Ok(headless(&home, &args, SideCopy::default()))
    }

    /// The same headless Claude Code with nothing to resume: a conversation of its own that
    /// keeps no record, in the usual mode, where with nobody to ask whatever would ask is
    /// refused: edits and commands that change things, and commands it doesn't know only look
    /// (a fresh run asked to `gh pr diff` was refused, 2.1.292). So Codealong hands it what it
    /// reads in `reads`, which `--add-dir` lets it read without asking (`=`, since the flag takes
    /// several folders). It runs on the account's usual model, and has its rules with its first
    /// question as a copy does.
    fn fresh_side(&self, ctx: &crate::harness::Ctx, _cwd: &Path, rules: &str, reads: &Path) -> anyhow::Result<crate::side::Recipe> {
        let reads = format!("--add-dir={}", reads.display());
        let args = ["-p", "--input-format", "stream-json", "--output-format", "stream-json", "--verbose", "--include-partial-messages", "--no-session-persistence", "--permission-mode", "default", &reads];
        Ok(headless(&ctx.account.home_path(), &args, SideCopy::told(rules)))
    }

    /// A background job stops under its account's service, which keeps the conversation
    /// (`claude stop <job>`). Run through a login shell, as a start is: the account's command
    /// can be an alias.
    fn stop(&self, ctx: &crate::harness::Ctx, s: &crate::model::Session) -> anyhow::Result<String> {
        let Some(command) = s.attach_command.as_deref().and_then(|attach| stop_command(ctx.account, attach)) else {
            anyhow::bail!("It isn't a background job, so it closes with its terminal.");
        };
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
        if let Err(e) = procs::run(&shell, &["-ilc", &command], None, std::time::Duration::from_secs(30)) {
            // Claude Code's own reason comes last, after whatever the shell's startup printed.
            let said = e.to_string();
            let why = said.lines().map(str::trim).filter(|l| !l.is_empty()).last().unwrap_or("it failed").to_string();
            anyhow::bail!("Couldn't stop the job: {why}");
        }
        Ok(format!("Stopped “{}”. Its conversation is kept and resumes like any other.", s.title))
    }

    /// Names it as Claude Code's `/rename` does (`name_transcript`). A running session reads the
    /// name back the next time it reads the end of its transcript, after about 32 KB of its own
    /// writes, at a compaction or as it exits, and goes by it from then on, its terminal's title
    /// too (2.1.285's code, which says other processes may write it). A job with no conversation
    /// of its own yet gets it in the transcript it goes on from as well, which the board reads
    /// for it until then.
    fn rename(&self, ctx: &crate::harness::Ctx, s: &crate::model::Session, name: &str) -> anyhow::Result<Vec<PathBuf>> {
        let dirs: Vec<PathBuf> = std::fs::read_dir(ctx.account.home_path().join("projects")).map(|d| d.flatten().map(|e| e.path()).collect()).unwrap_or_default();
        let ids = std::iter::once(s.id.as_str()).chain(s.continues.as_deref());
        let paths: Vec<PathBuf> = ids.flat_map(|id| dirs.iter().map(move |d| d.join(format!("{id}.jsonl")))).filter(|p| p.is_file()).collect();
        if paths.is_empty() {
            anyhow::bail!("“{}” has no transcript yet to keep a name in: Claude Code writes one at its first prompt.", s.title);
        }
        for p in &paths {
            name_transcript(p, name)?;
        }
        Ok(paths)
    }

    /// Escape, Claude Code's own interrupt: pressed in the terminal it runs in, or, for a
    /// background job, through its account's service.
    fn interrupts(&self, ctx: &crate::harness::Ctx, s: &crate::model::Session) -> bool {
        keys_reach(ctx, s)
    }

    /// Its send-now key, typed after the prompt where keys reach it (`sends_now_with`). A
    /// Claude app session takes messages only on its socket, where `priority: "now"` waits for
    /// a running command as `next` does, then ends the turn and starts another on the message.
    fn steers(&self, ctx: &crate::harness::Ctx, s: &crate::model::Session) -> bool {
        keys_reach(ctx, s)
    }

    fn sends_now_with(&self) -> Option<&'static str> {
        Some(SEND_NOW)
    }

    /// Presses Escape only while its own record says a turn is running: busy, or waiting on a
    /// prompt, which Escape answers No to, ending the turn. With nothing running, an Escape would
    /// go to whatever the terminal shows then (two in a row open Claude Code's rewind).
    fn interrupt(&self, ctx: &crate::harness::Ctx, s: &crate::model::Session) -> anyhow::Result<String> {
        let home = ctx.account.home_path();
        let registry = session_entry(&home, s).ok_or_else(|| anyhow::anyhow!("It isn't running any more."))?;
        if !matches!(registry.status.as_str(), "busy" | "waiting") {
            anyhow::bail!("Its turn has already ended.");
        }
        if registry.kind.as_deref() == Some("bg") {
            let job = registry.job_id.as_deref().ok_or_else(|| anyhow::anyhow!("Claude Code didn't record which job it is."))?;
            let socket = daemon_socket(&home).ok_or_else(|| anyhow::anyhow!("Its account's background service isn't running."))?;
            daemon_press_escape(&socket, &home.join("daemon/control.key"), job)?;
        } else {
            let pid = s.pid.ok_or_else(|| anyhow::anyhow!("It isn't running any more."))?;
            let Some(host) = crate::launch::terminal_host(pid, Path::new(&s.cwd))? else {
                anyhow::bail!("It isn't in a terminal Codealong can press keys in ({}). Open it to stop it there.", crate::launch::presses_keys_in());
            };
            crate::launch::press_key(&host, crate::launch::Key::Escape)?;
        }
        Ok(format!("Stopped “{}”.", s.title))
    }

    fn app(&self) -> &'static str {
        "the Claude app"
    }

    /// A terminal session opens in its tab, a background job by attaching, one that isn't
    /// running by resuming; the Claude app continues any of them for the account it's signed
    /// into, and opens its own.
    fn targets(&self, s: &Session, o: &crate::harness::Opens) -> Vec<crate::model::Target> {
        use crate::model::Surface;
        let (term, who) = (o.term, o.who.clone());
        let mut t = Vec::new();
        match s.surface {
            Surface::Cli if s.live && s.attach_command.is_some() => {
                t.push(o.way(s, "attach", "terminal", format!("Attach in {term}"), format!("{who} · runs in the background"), true, false));
            }
            Surface::Cli if s.live => {
                let verb = if o.respond { "Respond in" } else { "Show in" };
                t.push(o.way(s, "focus", "terminal", format!("{verb} {term}"), who, true, false));
                if o.first && s.app_account {
                    t.push(o.way(s, "claude-app", "app", "Open in Claude app".into(), format!("Live in {term}, finish there first"), false, true));
                }
            }
            Surface::Cli => {
                t.push(o.way(s, "resume", "terminal", format!("Resume in {term}"), who.clone(), true, false));
                // Only the account the Claude app is signed into can continue there.
                if o.first && s.app_account {
                    t.push(o.way(s, "claude-app", "app", "Continue in Claude app".into(), who, false, false));
                }
            }
            Surface::App => {
                let verb = if o.respond { "Respond in" } else { "Open in" };
                t.push(o.way(s, "claude-app", "app", format!("{verb} Claude app"), who.clone(), true, false));
                if o.first {
                    let (why, busy) = if s.live { ("Live in the Claude app".to_string(), true) } else { (who, false) };
                    t.push(o.way(s, "resume", "terminal", format!("Resume in {term}"), why, false, busy));
                }
            }
        }
        t
    }

    fn open(&self, s: &Session, action: &str) -> anyhow::Result<()> {
        match action {
            "claude-app" => crate::launch::open_url(&app::claude_app_url(&s.id)),
            other => anyhow::bail!("Unknown action {other:?}"),
        }
    }

    fn app_bundle(&self, action: &str) -> Option<&'static str> {
        (action == "claude-app").then_some(app::BUNDLE_ID)
    }

    fn search_files(&self, account: &AccountCfg, since: i64) -> Vec<(PathBuf, String, i64)> {
        let files = transcript_files(&account.home_path(), since, &HashSet::new());
        files
            .into_iter()
            .map(|(p, m)| {
                let key = Session::key_for(Harness::Claude, &account.name, file_name(&p).trim_end_matches(".jsonl"));
                (p, key, m)
            })
            .collect()
    }

    fn records_tokens(&self) -> bool {
        true
    }

    /// Every transcript, and its subagents' logs under its session's key: their calls are the
    /// session's.
    fn token_files(&self, account: &AccountCfg) -> Vec<(PathBuf, String)> {
        let mut out = Vec::new();
        for (p, _) in transcript_files(&account.home_path(), 0, &HashSet::new()) {
            let id = file_name(&p).trim_end_matches(".jsonl").to_string();
            let key = Session::key_for(Harness::Claude, &account.name, &id);
            out.extend(subagent_logs(&p, &id).into_iter().map(|log| (log, key.clone())));
            out.push((p, key));
        }
        out
    }

    fn token_use(&self, line: &[u8], state: &mut crate::ledger::TokenState) -> Option<crate::ledger::TokenUse> {
        transcript::token_use(line, state)
    }

    /// What you typed, and what the agent replied.
    fn prose(&self, line: &[u8]) -> Option<(i64, bool, String)> {
        let has = |needle: &[u8]| memchr::memmem::find(line, needle).is_some();
        if !has(br#""type":"user""#) && !has(br#""type":"assistant""#) {
            return None;
        }
        let v: Value = serde_json::from_slice(line).ok()?;
        let at = v.get("timestamp").and_then(Value::as_str).and_then(parse_rfc3339_ms).unwrap_or(0);
        typed_text(&v).map(|t| (at, true, t)).or_else(|| reply_text(&v).map(|t| (at, false, t)))
    }

    /// Typed text goes through the same path as a prompt argument: `@"<path>"` attaches a file.
    fn typed_attachments(&self) -> crate::attachments::Style {
        crate::attachments::Style::Mention
    }

    /// A prompt Claude Code took back may be in its input box again, where what's typed would
    /// be added to it and both go as one prompt.
    fn clears_before_typing(&self, account: &AccountCfg, s: &Session) -> bool {
        last_prompt_taken_back(account, s)
    }

    /// Every live session listens on a socket for other sessions' messages.
    fn takes_messages(&self) -> bool {
        true
    }

    fn would_hold_messages(&self, ctx: &crate::harness::Ctx, s: &crate::model::Session) -> bool {
        let registry = session_entry(&ctx.account.home_path(), s);
        holds_in(permission_mode(registry.as_ref(), &self.records(ctx.account, s)).as_deref())
    }

    fn resume(&self, account: &AccountCfg, s: &Session, ask: &crate::harness::StartAsk) -> anyhow::Result<(String, Vec<(String, String)>)> {
        Ok(claude_resume_input(account, &resume_args(account, s, home_of_account), ask.prompt, ask.model, ask.effort))
    }

    /// A terminal, the background service, the Claude app (`claude://resume` takes a session
    /// the command line started) and Codealong (the ACP session is the Claude session).
    fn resumes_in(&self, place: crate::start::Place) -> bool {
        use crate::start::Place;
        matches!(place, Place::Terminal | Place::Background | Place::ClaudeApp | Place::Codealong)
    }

    /// As itself, unless a background job has had it (`resume_job_command`); the roster says which.
    fn resume_in_background(&self, ctx: &crate::harness::Ctx, s: &Session, cwd: &Path, ask: &crate::harness::StartAsk) -> anyhow::Result<Option<String>> {
        let command = resume_job_command(ctx.account, &resume_args(ctx.account, s, home_of_account), None, ask.prompt, ask.model, ask.effort);
        let (_, id) = start_background(ctx.account, cwd, &command)?;
        Ok(id.map(|id| Session::key_for(Harness::Claude, &ctx.account.name, &id)))
    }

    fn app_way(&self) -> Option<crate::send::Route> {
        Some(app::InApp.into())
    }

    /// `/model <name>` switches the session alone; its picker, `/model` with no name, saves the
    /// pick as the default for new sessions too (2.1.280: "Set model to `opus[1m]
    /// (claude-opus-5-5[1m])`" against "… and saved as your default for new sessions").
    fn model_command(&self) -> Option<&'static str> {
        Some("/model")
    }

    /// `/effort <level>` sets the session's level, and saves low to xhigh as the account's
    /// default for its model too, which `type_switch` puts back (2.1.299).
    fn effort_command(&self) -> Option<&'static str> {
        Some("/effort")
    }

    fn mode(&self, id: &str) -> Option<crate::model::SessionMode> {
        session_mode(id)
    }

    /// The question Claude Code asks before a switch, answered, and the defaults `/model` and
    /// `/effort` saved put back: `switch::typed`. What the command says is in the session's own
    /// transcript, not one whose conversation it `continues`.
    fn type_switch(&self, account: &crate::config::AccountCfg, session: &crate::model::Session, to: crate::harness::Switch, text: &str, typ: &mut dyn FnMut(&str) -> anyhow::Result<bool>) -> anyhow::Result<Option<String>> {
        switch::typed(account, session, &|| transcripts_named(&account.home_path(), &session.id), to, text, typ)
    }

    /// `/exit` asks what to do with the worktree the session made, or removes it when nothing in
    /// it would be lost, before the process ends (`transcript::own_worktree`).
    fn exit_command(&self) -> Option<&'static str> {
        Some("/exit")
    }

    /// The background service (`claude daemon run`) keeps spare processes ready for the next
    /// job, each in a pty host of its own (`claude bg-pty-host --bg-pty-host <sock> … --bg-spare
    /// <sock>`, its spare `claude bg-spare --bg-spare <sock>`), and a job runs in a spare it
    /// claimed. With the service gone, launchd is their parent, and nothing claims them again
    /// (2.1.296).
    fn leftover(&self, command: &str) -> bool {
        command.split_whitespace().any(|w| w == "--bg-spare" || w == "--bg-pty-host")
    }

    /// A Code session the Claude app starts without a folder runs in a scratch workspace it
    /// makes for it.
    fn chats(&self) -> Option<(PathBuf, &'static str)> {
        Some((app::scratch_dir(), "Claude app chats"))
    }

    /// `/private/tmp/claude-<uid>/<encoded cwd>/<session id>/scratchpad`, which Claude Code makes
    /// for each session, and a background job's own folder, `<home>/jobs/<job>/`
    /// (`registry::job_folder_owner`).
    fn scratch_owner(&self, path: &Path) -> Option<String> {
        let s = path.to_string_lossy();
        let scratchpad = || {
            let rest = s.strip_prefix("/private/tmp/").or_else(|| s.strip_prefix("/tmp/"))?;
            let mut parts = rest.split('/');
            if !parts.next()?.starts_with("claude-") {
                return None;
            }
            let _cwd = parts.next()?;
            let id = parts.next()?;
            let is_uuid = id.len() == 36 && id.chars().filter(|c| *c == '-').count() == 4;
            (parts.next()? == "scratchpad" && is_uuid).then(|| id.to_string())
        };
        scratchpad().or_else(|| registry::job_folder_owner(path))
    }

    /// A tool's result is a `user` line of its own.
    fn output_marks(&self) -> &'static [&'static str] {
        &["\"type\":\"tool_result\""]
    }

    /// Every line has the folder its shell was in.
    fn folder_marks(&self) -> &'static [&'static str] {
        &["\"cwd\":\""]
    }

    fn pr_needles(&self) -> &'static [&'static str] {
        &["\"gitOperation\"", "\"pr-link\""]
    }


    fn pr_made(&self, line: &str) -> Option<(String, i64)> {
        transcript::pr_made(line)
    }

    fn pushed(&self, line: &str) -> Option<(String, i64)> {
        transcript::pushed(line)
    }

    fn artifact_needles(&self) -> &'static [&'static str] {
        &conversation::ARTIFACT_NEEDLES
    }

    fn artifact_made(&self, line: &str) -> Option<crate::model::Artifact> {
        conversation::artifact_made(line)
    }

    // ---- Starting a session ----

    fn models(&self, _account: &AccountCfg) -> Vec<crate::start::ModelOption> {
        MODELS.iter().map(|(id, label)| crate::start::ModelOption { id: id.to_string(), label: label.to_string() }).collect()
    }

    fn takes_model(&self) -> bool {
        true
    }

    /// The list is its aliases; `--model` takes a model's full name too.
    fn offers_model(&self, _account: &AccountCfg, _model: &str) -> bool {
        true
    }

    fn efforts(&self) -> &'static [(&'static str, &'static str)] {
        &EFFORTS
    }

    fn default_model(&self, account: &AccountCfg) -> Option<String> {
        default_model(account)
    }

    /// `-w/--worktree` makes one and chooses its folder, branch and base itself (or a repo's
    /// WorktreeCreate hook does).
    fn makes_worktrees(&self) -> bool {
        true
    }

    /// The repo's `WorktreeRemove` hooks (`teardown::remove_hooks`), which Claude Code runs only
    /// in a folder the account trusts.
    fn teardown(&self, ctx: &crate::harness::Ctx, main: &Path, worktree: &Path) -> Result<Vec<crate::cleanup::Hook>, String> {
        let hooks = teardown::remove_hooks(main, worktree);
        if hooks.is_empty() || trust::trusts(&ctx.account.home_path(), main) {
            return Ok(hooks);
        }
        Err(format!("{} doesn't trust {}, so Claude Code wouldn't run its WorktreeRemove hook", ctx.account.name, crate::util::tilde(main)))
    }

    /// `claude rm <job>` for each background job whose record names the worktree: once the
    /// folder is gone it drops the job's state and keeps its transcript (2.1.296).
    fn forget_worktree(&self, ctx: &crate::harness::Ctx, path: &Path) -> Vec<String> {
        teardown::jobs_in(&ctx.account.home_path(), path).into_iter().map(|job| format!("{} rm {job}", ctx.account.command())).collect()
    }

    /// Trusts the folder for the account (`trust_folder`): Claude Code stops a new session at
    /// its trust prompt in a folder it hasn't trusted, where the default answer quits, and
    /// `--bg` refuses to start there. You chose the folder, as answering the prompt would. A
    /// start goes ahead without it, and Claude Code asks. A chat of Codealong's goes by its
    /// chats folder, where trust goes by the folders above: trusting it trusts every chat in it.
    fn before_start(&self, account: &AccountCfg, folder: &Path) {
        let chats = crate::config::chats_dir();
        let folder = if folder.starts_with(&chats) { chats.as_path() } else { folder };
        if let Err(e) = trust_folder(&account.home_path(), folder) {
            eprintln!("Couldn't trust {} for {}: {e:#}", crate::util::tilde(folder), account.name);
        }
    }

    /// Claude Code takes `session_id` as its own (`--session-id`), so the board knows the
    /// session the moment it appears. `--worktree` takes an optional name, so it goes first:
    /// followed by the prompt it would take the prompt as a name. Attachments go as file
    /// mentions, with their folders added so it can read them without asking; `--add-dir` takes
    /// several folders, so each comes before `--session-id`, which ends it.
    fn start_in_terminal(&self, account: &AccountCfg, ask: &crate::harness::StartAsk) -> crate::harness::Typed {
        use crate::start::{FILES_VAR, IMAGES_VAR, PROMPT_VAR, SESSION_VAR};
        let (has_images, has_files) = crate::attachments::kinds(ask.prompt);
        let prompt = crate::attachments::expand_in(ask.prompt, ask.images_dir, ask.files_dir, crate::attachments::Style::Mention);
        let prompt = prompt.trim();
        let key = Session::key_for(Harness::Claude, &account.name, ask.session_id);
        let mut input = account.command().to_string();
        let mut env = Vec::new();
        if ask.worktree {
            input.push_str(" --worktree");
        }
        for (has, var, dir) in [(has_images, IMAGES_VAR, ask.images_dir), (has_files, FILES_VAR, ask.files_dir)] {
            if has {
                input.push_str(&format!(" --add-dir \"${var}\""));
                env.push((var.to_string(), dir.to_string_lossy().to_string()));
            }
        }
        input.push_str(&format!(" --session-id {}", ask.session_id));
        env.push((SESSION_VAR.to_string(), key.clone()));
        if let Some(m) = ask.model {
            input.push_str(&format!(" --model '{m}'"));
        }
        if let Some(e) = ask.effort {
            input.push_str(&format!(" --effort {e}"));
        }
        if !prompt.is_empty() {
            input.push_str(&format!(" \"${PROMPT_VAR}\""));
            env.push((PROMPT_VAR.to_string(), crate::start::as_argument(prompt)));
        }
        input.push('\n');
        crate::harness::Typed { input, env, key: Some(key) }
    }

    /// A background session, under the account's background service. Its arguments are quoted
    /// in the command itself: the service the session runs under may start from this shell and
    /// keep its environment, so nothing is handed over in that.
    /// `--bg` runs it under the account's background service.
    fn runs_in_background(&self) -> bool {
        true
    }

    fn background_command(&self, account: &AccountCfg, ask: &crate::harness::StartAsk) -> String {
        format!("{}{}", account.command(), background_args(ask.prompt, ask.model, ask.effort, ask.worktree, (ask.images_dir, ask.files_dir)))
    }

    fn start_in_background(&self, account: &AccountCfg, cwd: &Path, command: &str) -> anyhow::Result<crate::start::Started> {
        let (job, id) = start_background(account, cwd, command)?;
        Ok(match id {
            Some(id) => crate::start::Started { key: Some(Session::key_for(Harness::Claude, &account.name, &id)), message: None },
            None => crate::start::Started { key: None, message: Some(format!("{} is starting in the background as {job}.", account.name)) },
        })
    }

    /// `claude://code/new` opens the Claude app's new-session page with the folder and prompt
    /// filled in; it takes no model or worktree, which the page has its own choices for.
    fn app_link(&self, folder: &Path, ask: &crate::harness::StartAsk) -> Option<String> {
        let prompt = crate::attachments::expand_in(ask.prompt, ask.images_dir, ask.files_dir, crate::attachments::Style::Mention);
        Some(crate::start::with_param(format!("claude://code/new?folder={}", urlencoding::encode(&folder.to_string_lossy())), "q", prompt.trim()))
    }

    fn start_in_app(&self, _account: &AccountCfg, lane: &str, _folder: &Path, ask: &crate::harness::StartAsk, link: &str) -> anyhow::Result<crate::start::Started> {
        crate::launch::open_url(link)?;
        let message = if ask.prompt.trim().is_empty() { format!("A new Claude app session is open in {lane}.") } else { "Your prompt is waiting in a new Claude app session. Send it from there.".to_string() };
        Ok(crate::start::Started { key: None, message: Some(message) })
    }

    /// The endpoint behind its `/usage`, which turns away calls that come often.
    fn polls_usage(&self, _account: &AccountCfg) -> bool {
        true
    }

    fn read_usage(&self, account: &AccountCfg, now: i64) -> Usage {
        usage::claude(account, now)
    }

    /// Help improve Claude, in the account's privacy settings.
    fn training(&self, account: &AccountCfg) -> crate::training::Training {
        training::read(account)
    }

    fn stop_training(&self, account: &AccountCfg) -> anyhow::Result<()> {
        training::stop(account)
    }

    fn reader(&self, _wake: crate::harness::Wake) -> Box<dyn crate::harness::Reader> {
        Box::new(Reader::default())
    }
}

/* ---- Reading sessions for the board ---- */

/// How often the commands sessions left running in the background are looked at.
const BACKGROUND_EVERY: Duration = Duration::from_secs(60);
/// How often the Claude app's records are read again, for what you archived there.
const APP_RECORDS_EVERY: Duration = Duration::from_secs(30);

/// A command a session left running in the background, as last seen.
struct BackgroundWatch {
    /// The registry's status time: a new turn that leaves a command running starts afresh.
    since: i64,
    tree: procs::CommandTree,
    /// When its processes were last seen starting a process or using CPU.
    active_at: i64,
}

/// Claude Code's sessions for the board (`harness::Reader`): each account's live registry and
/// transcripts as last read, and what a look at processes said about the sessions that have
/// gone quiet.
#[derive(Default)]
pub struct Reader {
    registry: HashMap<String, Vec<RegistryEntry>>,
    /// What each account's background jobs were forked from, read from their records.
    forks: HashMap<String, Forks>,
    transcripts: HashMap<PathBuf, (String, Parsed)>,
    /// How far each subagent log has been handed to the engine to check.
    subagent_offsets: HashMap<PathBuf, u64>,
    /// The Claude app's session records, for what you archived there.
    app_records: app::Records,
    app_records_at: Option<Instant>,
    /// (pid, status time) → the command a stuck session's shell is running.
    stuck: Mutex<HashMap<(u32, i64), Option<String>>>,
    /// Sessions whose turn left a command running in the background, by pid: when their
    /// processes last did something, from a look at them every minute.
    background: HashMap<u32, BackgroundWatch>,
    background_at: Option<Instant>,
}

/// The conversation a session with no transcript of its own yet goes on from: its ID, its key
/// and its transcript.
struct Source<'a> {
    id: String,
    key: String,
    transcript: (&'a Path, &'a TranscriptInfo),
}

impl Reader {
    /// What a session of `account` moved from another account goes on from: the transcript there
    /// that `origin` (Codealong's record of it, origins.rs) names, a fork from another Claude Code
    /// account being a move. Claude Code writes the copy's own transcript at its first prompt.
    fn moved_copy(&self, account: &str, origin: Option<SessionOrigin>) -> Option<Source<'_>> {
        let origin = origin.filter(|o| o.how == OriginHow::Fork)?;
        let mut parts = origin.key.splitn(3, ':');
        let (harness, from, id) = (parts.next()?, parts.next()?, parts.next()?);
        if harness != Harness::Claude.config() || from == account {
            return None;
        }
        let newest = self.transcripts.iter().filter(|(_, (acct, (_, t, _)))| acct == from && t.session_id == id).max_by_key(|(_, (_, (_, t, _)))| t.last_ts);
        newest.map(|(path, (_, (_, t, _)))| Source { id: id.to_string(), key: origin.key.clone(), transcript: (path.as_path(), t) })
    }

    /// The key of `id`, the session a job of `account` was forked from. The job's record and
    /// the roster name it by its ID alone, and it's another account's when the job is a copy
    /// moved here from there (start.rs `move_to`): then only that account's transcripts have
    /// it, and only those are looked through.
    fn parent_key(&self, account: &str, ours: bool, id: &str) -> String {
        let theirs = || self.transcripts.values().find(|(acct, (_, t, _))| t.session_id == id && acct != account).map(|(acct, _)| acct.as_str());
        let at = if ours { account } else { theirs().unwrap_or(account) };
        Session::key_for(Harness::Claude, at, id)
    }

    /// Copies moved here from another account (start.rs `move_to`) whose process ended before
    /// their first prompt, with `known` the IDs the account's registry and transcripts have.
    /// Claude Code wrote nothing of such a copy, and once its process is gone the registry
    /// doesn't have it either, so Codealong's own note of the move (`origins`) is all there is.
    /// It shows stopped, dated and worded as the conversation it was moved with, and going on
    /// with it makes the copy again from that conversation under its own ID, so a move holds
    /// from the moment it's noted, whatever becomes of the copy's terminal.
    fn closed_copies(&self, a: &AccountCfg, known: &HashSet<&str>, origins: &crate::origins::Store, look: &Look) -> Vec<Session> {
        let prefix = format!("{}:{}:", Harness::Claude.config(), a.name);
        let mut out = Vec::new();
        for key in origins.sessions.keys() {
            let Some(id) = key.strip_prefix(&prefix).filter(|id| !known.contains(id)) else { continue };
            let Some(from) = self.moved_copy(&a.name, origins.origin_of(key)) else { continue };
            let info = TranscriptInfo { session_id: id.to_string(), limit: None, connection_lost: None, ..from.transcript.1.clone() };
            let seen_at = look.seen.seen_at(&from.key).max(look.seen.seen_at(key));
            let Some(mut s) = crate::board::claude_session(a, None, Some(&info), seen_at, look.now, &crate::board::Quiet::default()) else { continue };
            s.archived = look.archived_here(key, s.last_activity);
            s.resume_command = Some(format!("{} --resume {}", a.command(), fork_args(&crate::util::sh_quote(&from.transcript.0.to_string_lossy()), id)));
            s.continues = Some(from.id);
            s.origin = Some(SessionOrigin { key: from.key, how: OriginHow::Fork });
            out.push(s);
        }
        out
    }

    fn refresh_registry(&mut self, a: &AccountCfg, now: i64) -> bool {
        let home = a.home_path();
        let fresh = read_registry(&home);
        let changed = self.registry.get(&a.name) != Some(&fresh);
        self.registry.insert(a.name.clone(), fresh);
        // A new job shows in the registry as it starts, and its record says what it's a fork of.
        let forked = self.forks.entry(a.name.clone()).or_default().refresh(&home, now);
        changed || forked
    }

    /// Looks at what the commands sessions left running in the background are doing, once a
    /// minute and only while there are some: one `ps` for all of them.
    fn watch_background(&mut self, now: i64) {
        if self.background_at.is_some_and(|t| t.elapsed() < BACKGROUND_EVERY) {
            return;
        }
        self.background_at = Some(Instant::now());
        let shells: Vec<(u32, i64)> = self.registry.values().flatten().filter(|r| r.status == "shell").map(|r| (r.pid, r.status_updated_at.unwrap_or(0))).collect();
        self.background.retain(|pid, _| shells.iter().any(|(p, _)| p == pid));
        if shells.is_empty() {
            return;
        }
        let pids: Vec<u32> = shells.iter().map(|(p, _)| *p).collect();
        let mut trees = shells::command_trees(&pids);
        for (pid, since) in shells {
            let Some(tree) = trees.remove(&pid) else { continue };
            match self.background.get_mut(&pid) {
                Some(w) if w.since == since => {
                    if tree.active_since(&w.tree) {
                        w.active_at = now;
                    }
                    w.tree = tree;
                }
                _ => {
                    // Seen for the first time, perhaps long after the turn ended (the app just
                    // started): give it a few looks to show it's doing something.
                    let active_at = since.max(now - crate::board::BACKGROUND_QUIET_MS + 3 * BACKGROUND_EVERY.as_millis() as i64);
                    self.background.insert(pid, BackgroundWatch { since, tree, active_at });
                }
            }
        }
    }

    /// What a stuck session's shell is running, read once per stall.
    fn stuck_command(&self, r: &RegistryEntry) -> Option<String> {
        let mut cache = self.stuck.lock().ok()?;
        cache.entry((r.pid, r.status_updated_at.unwrap_or(0))).or_insert_with(|| shells::running_command(r.pid)).clone()
    }
}

impl crate::harness::Reader for Reader {
    fn harness(&self) -> Harness {
        Harness::Claude
    }

    fn scan(&mut self, accounts: &[AccountCfg], look: &Look) {
        self.registry.clear();
        self.transcripts.clear();
        if !accounts.is_empty() {
            self.app_records.refresh(&app::records_dir());
            self.app_records_at = Some(Instant::now());
        }
        for a in accounts {
            let home = a.home_path();
            self.registry.insert(a.name.clone(), read_registry(&home));
            self.forks.entry(a.name.clone()).or_default().refresh(&home, look.now);
            let files = transcript_files(&home, look.since, &look.seen.pinned_ids("claude", &a.name));
            for (path, parsed) in parallel_map(files, |(p, m)| parse_transcript(&p, m, None).map(|r| (p, r))) {
                self.transcripts.insert(path, (a.name.clone(), parsed));
            }
        }
    }

    fn watch(&self, account: &AccountCfg) -> Vec<(PathBuf, bool)> {
        let home = account.home_path();
        vec![(home.join("sessions"), false), (home.join("projects"), true)]
    }

    fn owns(&self, account: &AccountCfg, path: &Path) -> bool {
        let home = account.home_path();
        path.starts_with(home.join("sessions")) || path.starts_with(home.join("projects"))
    }

    fn apply(&mut self, account: &AccountCfg, paths: &[PathBuf], look: &Look) -> Applied {
        let home = account.home_path();
        let (sessions, projects) = (home.join("sessions"), home.join("projects"));
        let mut out = Applied::default();
        if paths.iter().any(|p| p.starts_with(&sessions)) && self.refresh_registry(account, look.now) {
            out.dirty = true;
            out.added = true;
        }
        for p in paths.iter().filter(|p| p.starts_with(&projects) && p.extension().and_then(|x| x.to_str()) == Some("jsonl")) {
            if p.parent().and_then(Path::parent) == Some(projects.as_path()) {
                let prev = self.transcripts.remove(p).map(|(_, parsed)| parsed);
                let is_new = prev.is_none();
                let from = prev.as_ref().map(|(_, _, st)| st.offset);
                if let Some(parsed) = mtime_ms(p).and_then(|m| parse_transcript(p, m, prev)) {
                    let key = Session::key_for(Harness::Claude, &account.name, &parsed.1.session_id);
                    match from {
                        Some(from) => out.lines.push(NewLines { path: p.clone(), from, to: parsed.2.offset, key }),
                        None => {
                            // Its subagents' logs so far go with it: one resumed from before the
                            // window has them, unchanged, wherever its session was as it started
                            // each (`helper_logs`). Only what they write after is handed on.
                            for log in helper_logs(&home, &parsed.1.session_id) {
                                if let Ok(len) = std::fs::metadata(&log).map(|m| m.len()) {
                                    self.subagent_offsets.entry(log.clone()).or_insert(len);
                                }
                                out.found.push((log, key.clone()));
                            }
                            out.found.push((p.clone(), key));
                        }
                    }
                    self.transcripts.insert(p.clone(), (account.name.clone(), parsed));
                }
                out.added |= is_new;
                out.dirty = true;
            } else if p.parent().map(file_name).as_deref() == Some("subagents") {
                // `<session>/subagents/agent-*.jsonl`: a subagent's lines count for its session.
                let Some(id) = p.parent().and_then(Path::parent).map(file_name) else { continue };
                let Ok(len) = std::fs::metadata(p).map(|m| m.len()) else { continue };
                // A log first seen now is new, or its earlier lines were read by the last full search.
                let from = self.subagent_offsets.get(p).copied().unwrap_or(len.saturating_sub(1 << 20));
                out.lines.push(NewLines { path: p.clone(), from, to: len, key: Session::key_for(Harness::Claude, &account.name, &id) });
                self.subagent_offsets.insert(p.clone(), len);
            }
        }
        out
    }

    fn tick(&mut self, accounts: &[AccountCfg], look: &Look) -> Applied {
        let mut out = Applied::default();
        for a in accounts {
            out.dirty |= self.refresh_registry(a, look.now);
        }
        self.watch_background(look.now);
        if !accounts.is_empty() && self.app_records_at.map_or(true, |t| t.elapsed() >= APP_RECORDS_EVERY) {
            self.app_records_at = Some(Instant::now());
            out.dirty |= self.app_records.refresh(&app::records_dir());
        }
        out
    }

    fn sessions(&self, account: &AccountCfg, look: &Look) -> Vec<Session> {
        use crate::board;
        let (a, now) = (account, look.now);
        let mut out = Vec::new();
        let app_archived = self.app_records.archived();
        let app_origins = self.app_records.origins();
        let app_modes = self.app_records.modes();
        let mut by_id: HashMap<&str, (Option<&RegistryEntry>, Option<(&Path, &TranscriptInfo)>)> = HashMap::new();
        for r in self.registry.get(&a.name).map(|v| v.as_slice()).unwrap_or(&[]) {
            by_id.entry(r.session_id.as_str()).or_default().0 = Some(r);
        }
        for (path, (acct, (_, t, _))) in &self.transcripts {
            if acct == &a.name {
                let slot = by_id.entry(t.session_id.as_str()).or_default();
                if slot.1.map(|(_, old)| old.last_ts < t.last_ts).unwrap_or(true) {
                    slot.1 = Some((path.as_path(), t));
                }
            }
        }
        let live: HashSet<&str> = by_id.iter().filter(|(_, (r, _))| r.is_some()).map(|(id, _)| *id).collect();
        let said: HashMap<&str, &TranscriptInfo> = by_id.iter().filter_map(|(id, (_, t))| t.map(|(_, t)| (*id, t))).collect();
        let moves = board::claude_moves(&live, &said);
        for (&id, &(reg, tr)) in &by_id {
            // Its conversation went on in a job, which shows it.
            if moves.moved.contains_key(id) {
                continue;
            }
            let key = Session::key_for(Harness::Claude, &a.name, id);
            let copied_from = tr.and_then(|(_, t)| t.copied_from.as_deref());
            // A job that hasn't written its own transcript yet shows the one it goes on
            // from: a window's conversation it took over, or one it's a fork of.
            let resumes = reg.and_then(|r| r.resumes.as_ref()).filter(|_| tr.is_none());
            let from = moves.from.get(id).or(resumes).and_then(|f| Some((f, by_id.get(f.as_str())?.1?)));
            let from = from.map(|(f, t)| Source { id: f.clone(), key: Session::key_for(Harness::Claude, &a.name, f), transcript: t });
            // And a copy moved from another account (start.rs `move_to`), until it writes its
            // own, the one it goes on from there.
            let from = from.or_else(|| if tr.is_none() && reg.is_some() { self.moved_copy(&a.name, crate::origins::current().origin_of(&key)) } else { None });
            let tr = tr.or(from.as_ref().map(|f| f.transcript));
            let moved = reg.zip(from.as_ref()).map(|(r, f)| board::moved_registry(r, f.transcript.1));
            let reg = moved.as_ref().or(reg);
            let seen_at = from.as_ref().map(|f| look.seen.seen_at(&f.key)).unwrap_or(0).max(look.seen.seen_at(&key));
            let info = tr.map(|(_, t)| t);
            // A limit or a lost connection in the conversation it goes on from stopped that
            // session's turn, maybe on another account; this one hasn't taken a turn yet.
            let unlimited = from.as_ref().and(info).filter(|t| t.limit.is_some() || t.connection_lost.is_some()).map(|t| TranscriptInfo { limit: None, connection_lost: None, ..t.clone() });
            let info = unlimited.as_ref().or(info);
            // Only a busy session that's gone quiet costs a look outside its transcript,
            // and one whose turn left a command running in the background (read once
            // per status change).
            let background = reg.filter(|r| r.status == "shell");
            let (subagent_at, running) = match board::claude_busy_since(reg, info) {
                _ if background.is_some() => (None, background.and_then(|r| self.stuck_command(r))),
                Some(since) if now - since > board::CLAUDE_STALL_MS => {
                    let sub = tr.and_then(|(p, _)| subagent_activity(p, id));
                    let quiet = now - since.max(sub.unwrap_or(0)) > board::CLAUDE_STALL_MS;
                    (sub, if quiet { reg.and_then(|r| self.stuck_command(r)) } else { None })
                }
                _ => (None, None),
            };
            let background_active_at = background.and_then(|r| self.background.get(&r.pid)).filter(|w| Some(w.since) == background.and_then(|r| r.status_updated_at)).map(|w| w.active_at);
            let quiet = board::Quiet { subagent_at, running: running.as_deref(), background_active_at };
            if let Some(mut s) = board::claude_session(a, reg, info, seen_at, now, &quiet) {
                // Archived here until it's active again, or archived in the Claude app.
                s.archived = look.archived_here(&key, s.last_activity) || app_archived.contains(id);
                // The app keeps the mode you pick for a session it runs (`permission_mode`).
                if reg.is_some_and(|r| r.host_session.is_some()) {
                    s.mode = app_modes.get(id).and_then(|m| session_mode(m)).or(s.mode);
                }
                s.continues = from.map(|f| f.id);
                // A job's record says what it's a fork of; the roster says for a
                // running one whose record hasn't been read yet. The Claude app's record says
                // what it forked one of its sessions from, or whose agent started it, and a fork
                // it made starts with the lines it copied.
                let job = self.forks.get(&a.name).and_then(|f| f.parent_of(id)).or(reg.and_then(|r| r.resumes.as_deref()));
                let app = app_origins.get(id).map(|(p, how)| (p.as_str(), *how));
                let parent = job.map(|p| (p, OriginHow::Fork)).or(app).or(copied_from.map(|p| (p, OriginHow::Fork)));
                s.origin = parent.filter(|(p, _)| *p != id).map(|(p, how)| SessionOrigin { key: self.parent_key(&a.name, by_id.contains_key(p), p), how });
                out.push(s);
            }
        }
        out.extend(self.closed_copies(a, &by_id.keys().copied().collect(), &crate::origins::current(), look));
        out
    }

    fn cwds(&self) -> Vec<String> {
        let live = self.registry.values().flatten().map(|e| e.cwd.clone());
        live.chain(self.transcripts.values().map(|(_, (_, t, _))| t.cwd.clone())).collect()
    }

    fn logs(&self) -> Vec<(PathBuf, String)> {
        let mut files = Vec::new();
        // Each session's key by its projects folder and ID, for its subagents' logs.
        let mut keys: HashMap<(&Path, &str), String> = HashMap::new();
        for (p, (a, (_, info, _))) in &self.transcripts {
            let key = Session::key_for(Harness::Claude, a, &info.session_id);
            if let Some(projects) = p.parent().and_then(Path::parent) {
                keys.insert((projects, info.session_id.as_str()), key.clone());
            }
            files.push((p.clone(), key));
        }
        // A subagent works where its session sent it, so its log counts for the session. It's
        // under the project of the folder the session was in as it started the subagent, which
        // needn't be its transcript's (`helper_logs`), so each projects folder is looked through
        // once rather than beside each transcript.
        let folders: HashSet<&Path> = keys.keys().map(|(projects, _)| *projects).collect();
        for projects in folders {
            for (id, log) in subagent_logs_under(projects) {
                if let Some(key) = keys.get(&(projects, id.as_str())) {
                    files.push((log, key.clone()));
                }
            }
        }
        files
    }
}

/// Every subagent log in an account's projects folder (`<project>/<session>/subagents/*.jsonl`),
/// with its session's ID. A transcript isn't a folder, so it's passed over without a look inside.
fn subagent_logs_under(projects: &Path) -> Vec<(String, PathBuf)> {
    let Ok(dirs) = std::fs::read_dir(projects) else { return Vec::new() };
    let mut out = Vec::new();
    for project in dirs.flatten() {
        let Ok(entries) = std::fs::read_dir(project.path()) else { continue };
        for e in entries.flatten().filter(|e| e.file_type().is_ok_and(|t| t.is_dir())) {
            let Ok(logs) = std::fs::read_dir(e.path().join("subagents")) else { continue };
            let id = e.file_name().to_string_lossy().into_owned();
            out.extend(logs.flatten().map(|l| l.path()).filter(|p| p.extension().and_then(|x| x.to_str()) == Some("jsonl")).map(|p| (id.clone(), p)));
        }
    }
    out
}

/// The newest write to a session's subagent logs (`<transcript dir>/<id>/subagents`).
fn subagent_activity(transcript: &Path, session_id: &str) -> Option<i64> {
    let dir = transcript.parent()?.join(session_id).join("subagents");
    std::fs::read_dir(dir).ok()?.flatten().filter_map(|e| mtime_ms(&e.path())).max()
}

/// A session's subagent logs.
fn subagent_logs(transcript: &Path, session_id: &str) -> Vec<PathBuf> {
    let Some(dir) = transcript.parent().map(|d| d.join(session_id).join("subagents")) else { return vec![] };
    let Ok(entries) = std::fs::read_dir(dir) else { return vec![] };
    entries.flatten().map(|e| e.path()).filter(|p| p.extension().and_then(|x| x.to_str()) == Some("jsonl")).collect()
}

/// The inputs of the calls at the end of a transcript with no result yet: Claude Code writes a
/// call there before running it.
fn open_calls(path: &Path) -> Vec<String> {
    let Ok(page) = crate::transcript::file_page(path, conversation_items, None, 20) else { return Vec::new() };
    crate::transcript::open_tools(&page.items).into_iter().filter_map(|i| i.input.clone()).collect()
}

/// A session's transcripts: in its account's folder by its ID, or, while it has none of its own,
/// by the one it goes on from (`Session::continues`). A copy moved from another account
/// (start.rs `move_to`) goes on from that account's, in that account's folder, which `home_of`
/// finds by the account's name.
fn records_in(account: &AccountCfg, s: &Session, home_of: impl Fn(&str) -> Option<PathBuf>) -> Vec<PathBuf> {
    let id = s.continues.as_deref().unwrap_or(&s.id);
    let own = transcripts_named(&account.home_path(), id);
    if !own.is_empty() || s.continues.is_none() {
        return own;
    }
    let from = s.origin.as_ref().filter(|o| o.how == OriginHow::Fork).and_then(|o| o.key.strip_prefix("claude:")?.split(':').next());
    from.filter(|from| *from != account.name).and_then(home_of).map(|home| transcripts_named(&home, id)).unwrap_or_default()
}

fn transcripts_named(home: &Path, id: &str) -> Vec<PathBuf> {
    let Ok(dirs) = std::fs::read_dir(home.join("projects")) else { return Vec::new() };
    dirs.flatten().map(|d| d.path().join(format!("{id}.jsonl"))).filter(|p| p.is_file()).collect()
}

/// An account's folder by its name, as config.toml has it.
fn home_of_account(name: &str) -> Option<PathBuf> {
    crate::config::current().ok()?.accounts.into_iter().find(|a| a.name == name).map(|a| a.home_path())
}

/// Whether `home` is `~/.claude`, where Claude Code looks when no folder is named: it's never
/// named, whatever the app was started with, but any `CLAUDE_CONFIG_DIR` is taken off. A named
/// folder keeps its sign-in under a keychain item of its own (`Claude Code-credentials-` and
/// the first 8 hex of the folder's sha256), so Claude Code told `~/.claude` looks for a sign-in
/// that isn't there, and every turn ends "Not logged in · Please run /login" (2.1.280,
/// 2026-10-13).
pub(crate) fn usual_folder(home: &Path) -> bool {
    home == crate::util::home().join(".claude")
}

/// How the adapter `program` starts for the account in `home`: its Claude Code told the folder,
/// or for `~/.claude` (`usual_folder`) with none, through `env -u`, since the adapter's
/// environment is only added to.
fn acp_launch(program: String, home: &Path) -> (String, Vec<(String, String)>) {
    if usual_folder(home) {
        (format!("/usr/bin/env -u CLAUDE_CONFIG_DIR {program}"), Vec::new())
    } else {
        (program, vec![("CLAUDE_CONFIG_DIR".into(), home.to_string_lossy().into_owned())])
    }
}

/// A headless Claude Code on the account in `home`, run with `args` and talked to in `dialect`
/// (a side conversation's copy, or a PR's conversation).
fn headless(home: &Path, args: &[&str], dialect: SideCopy) -> crate::side::Recipe {
    let (env, unset) = headless_env(home);
    crate::side::Recipe { program: "claude".into(), args: args.iter().map(|a| a.to_string()).collect(), env, unset, dialect: Box::new(dialect) }
}

/// What a headless Claude Code for the account in `home` is given and mustn't inherit from the
/// app: its folder, unless it's the usual one, which goes without.
pub(crate) fn headless_env(home: &Path) -> (Vec<(String, String)>, Vec<&'static str>) {
    let usual = usual_folder(home);
    let env = if usual { Vec::new() } else { vec![("CLAUDE_CONFIG_DIR".into(), home.to_string_lossy().into_owned())] };
    // And it's no child of whatever session the app may have been started from.
    let unset = [usual.then_some("CLAUDE_CONFIG_DIR"), Some("CLAUDE_CODE_SESSION_ID"), Some("CLAUDE_CODE_CHILD_SESSION"), Some("CODEALONG_SESSION")].into_iter().flatten().collect();
    (env, unset)
}

/// What `--resume` goes on from for a session under `account`: its ID, or, while it has no
/// transcript of its own, the one it goes on from; for a copy moved from another account, the
/// transcript there by its path, which `--resume` takes too.
fn resume_from(account: &AccountCfg, s: &Session) -> String {
    match moved_source(account, s, home_of_account) {
        Some(path) => path.to_string_lossy().into_owned(),
        None => s.continues.clone().unwrap_or_else(|| s.id.clone()),
    }
}

/// The transcript in another account's folder that a copy moved under `account` goes on from,
/// while it has none of its own, that account's folder found by `home_of` (`records_in`).
fn moved_source(account: &AccountCfg, s: &Session, home_of: impl Fn(&str) -> Option<PathBuf>) -> Option<PathBuf> {
    let home = account.home_path();
    records_in(account, s, home_of).into_iter().find(|p| !p.starts_with(&home))
}

/// What `--resume` takes to go on with the session itself: its ID, or, for a copy moved here
/// that has written nothing yet, the conversation it was moved with, forked again under the
/// copy's ID (`fork_args`; `Reader::closed_copies`).
fn resume_args(account: &AccountCfg, s: &Session, home_of: impl Fn(&str) -> Option<PathBuf>) -> String {
    match moved_source(account, s, home_of) {
        Some(path) => fork_args(&crate::util::sh_quote(&path.to_string_lossy()), &s.id),
        None => s.id.clone(),
    }
}

/// `resume_from` for a command line typed into a shell: an ID as it is, a path quoted.
fn shell_word(from: &str) -> String {
    if from.contains('/') {
        crate::util::sh_quote(from)
    } else {
        from.to_string()
    }
}

fn record(a: &ClaudeCode, ctx: &crate::harness::Ctx, s: &crate::model::Session) -> anyhow::Result<PathBuf> {
    use crate::harness::Adapter;
    a.records(ctx.account, s).into_iter().next().ok_or_else(|| anyhow::anyhow!("Its transcript isn't on this Mac any more."))
}

/// The tool call a session's permission prompt is about, with its process: only while Claude
/// Code's own record of the session says it's waiting on a permission prompt, and the call is
/// the last one in its transcript with no result yet.
fn waiting_call(a: &ClaudeCode, ctx: &crate::harness::Ctx, s: &crate::model::Session) -> Option<(Item, RegistryEntry)> {
    waiting_on(a, ctx, s, PERMISSION_PROMPT)
}

/// The call a live session's prompt is up for, when its registry says it waits `for` that
/// (a "permission prompt", or "input needed" for questions), and its registry entry, which says
/// where it runs.
///
/// The call is the last in the session's transcript with no result yet; with none there, it's a
/// background helper's (a subagent run in the background, whose prompt shows in its session once
/// the plugin's hook has let it go), when one helper's is plainly the one (`helper_waiting`).
fn waiting_on(a: &ClaudeCode, ctx: &crate::harness::Ctx, s: &crate::model::Session, what: &str) -> Option<(Item, RegistryEntry)> {
    let home = ctx.account.home_path();
    let reg = registry_of(&home, s.pid?).filter(|r| r.session_id == s.id && r.waits_on(what))?;
    let page = crate::transcript::file_page(&record(a, ctx, s).ok()?, conversation_items, None, 40).ok()?;
    if let Some(call) = crate::transcript::open_tools(&page.items).last() {
        return Some(((*call).clone(), reg));
    }
    let call = helper_waiting(&helper_logs(&home, &s.id), reg.status_updated_at?)?;
    Some((call, reg))
}

/// A session's helpers' logs (`<session>/subagents/agent-*.jsonl`), in whichever of its
/// account's projects Claude Code wrote them: a helper's goes under the folder the session was
/// working in as it started the helper, which needn't be where its own transcript is (a session
/// that went on into a worktree).
fn helper_logs(home: &Path, session_id: &str) -> Vec<PathBuf> {
    let Ok(projects) = std::fs::read_dir(home.join("projects")) else { return Vec::new() };
    projects
        .flatten()
        .filter_map(|p| std::fs::read_dir(p.path().join(session_id).join("subagents")).ok())
        .flat_map(|logs| logs.flatten().map(|e| e.path()))
        .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("jsonl"))
        .collect()
}

/// How long before its session said it waits a helper's call can have been made and still be
/// the one the prompt is for: the prompt waits a few seconds for the plugin's hook first. A call
/// made longer before is a command still running.
const HELPER_PROMPT_MS: i64 = 60_000;

/// The call a helper's permission prompt is up for, its line saying whose: only when it's
/// plainly the one, since a key pressed for it answers whatever prompt is up. That's when one
/// helper's log, and only one, ends with a call that has no result, made within
/// `HELPER_PROMPT_MS` before the session said it waits (`waiting_since`); another helper's call
/// open then could as well be a command it's running.
fn helper_waiting(logs: &[PathBuf], waiting_since: i64) -> Option<Item> {
    let mut open = Vec::new();
    for log in logs {
        // A log not written to since well before can't hold the call.
        let fresh = std::fs::metadata(log).and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).is_some_and(|t| t.as_millis() as i64 >= waiting_since - HELPER_PROMPT_MS);
        if !fresh {
            continue;
        }
        let Ok(page) = crate::transcript::file_page(log, helper_items, None, 20) else { continue };
        let Some(last) = page.items.last().filter(|i| i.kind == "tool") else { continue };
        if !crate::transcript::open_tools(&page.items).iter().any(|c| c.id == last.id) {
            continue;
        }
        if last.at < waiting_since - HELPER_PROMPT_MS || last.at > waiting_since {
            continue;
        }
        let whose = helper_name(log).map(|n| format!(" · its helper “{n}”")).unwrap_or_else(|| " · one of its helpers".into());
        open.push(Item { text: format!("{}{whose}", last.text), ..last.clone() });
    }
    (open.len() == 1).then(|| open.remove(0))
}

/// A helper's log line as conversation items. Every line of a helper's own log says it's a
/// sidechain, which a session's transcript reading leaves out (`conversation_items`).
fn helper_items(v: &Value, at: i64) -> Vec<Item> {
    if v.get("isSidechain").and_then(Value::as_bool) != Some(true) {
        return conversation_items(v, at);
    }
    let mut main = v.clone();
    if let Some(line) = main.as_object_mut() {
        line.remove("isSidechain");
    }
    conversation_items(&main, at)
}

/// What a helper was started to do, from the `.meta.json` beside its log.
fn helper_name(log: &Path) -> Option<String> {
    let meta = log.with_extension("meta.json");
    let v: Value = serde_json::from_slice(&std::fs::read(meta).ok()?).ok()?;
    v.get("description").and_then(Value::as_str).filter(|d| !d.is_empty()).map(String::from)
}

/// How the id of a held message's request starts: the held line's uuid follows.
const HELD_ID: &str = "held:";

/// The message a live session holds for you, while Claude Code's own record of it says it waits
/// on a prompt, with that record. A call made since, in a session that asks about calls, may be
/// what the prompt is for, so then it's none.
fn waiting_hold(a: &ClaudeCode, ctx: &crate::harness::Ctx, s: &crate::model::Session) -> Option<(Held, RegistryEntry)> {
    let reg = registry_of(&ctx.account.home_path(), s.pid?).filter(|r| r.session_id == s.id && r.waits_on(PERMISSION_PROMPT))?;
    let path = record(a, ctx, s).ok()?;
    let held = held_in(&path)?;
    let page = crate::transcript::file_page(&path, conversation_items, None, 40).ok()?;
    let call_since = crate::transcript::open_tools(&page.items).last().is_some_and(|c| c.at > held.at);
    if call_since && permission_mode(Some(&reg), std::slice::from_ref(&path)).as_deref() != Some("bypassPermissions") {
        return None;
    }
    Some((held, reg))
}

/// A held message as the panel shows it: all of it when Codealong sent it, else the first line
/// Claude Code kept.
fn held_pending(ctx: &crate::harness::Ctx, s: &crate::model::Session, held: &Held, reg: &RegistryEntry) -> crate::harness::Pending {
    let kept = || match &held.more {
        Some(more) => format!("{}…\n\n({more} in all; its first line is all that's on record here.)", held.preview),
        None => held.preview.clone(),
    };
    let input = held::sent_text(&s.key, held).or_else(|| (!held.preview.is_empty()).then(kept));
    let cannot = if held.count > 1 { Some(format!("It holds {} messages, and only where it runs shows which comes first.", held.count)) } else { cannot_press(ctx, reg) };
    crate::harness::Pending {
        id: format!("{HELD_ID}{}", held.id),
        // Several: the line names the latest, and the terminal shows the oldest first.
        label: if held.count > 1 { format!("{} messages, the latest from {}", held.count, held.from) } else { format!("From {}", held.from) },
        input,
        cannot,
        always: false,
        no_once: false,
        plan: false,
        message: true,
        always_rule: None,
        questions: Vec::new(),
        unwritten: false,
        terminal: None,
    }
}

/// Why Codealong can't press keys where a session runs, when it can't: its terminal, or a
/// background job through its account's service.
fn cannot_press(ctx: &crate::harness::Ctx, reg: &RegistryEntry) -> Option<String> {
    if reg.kind.as_deref() == Some("bg") {
        return daemon_socket(&ctx.account.home_path()).is_none().then(|| "Its account's background service isn't running.".to_string());
    }
    match crate::launch::terminal_kind(reg.pid) {
        Some(k) if k.presses_keys() => None,
        Some(k) => Some(format!("{} can't press a single key for Codealong. Open it to answer.", k.name())),
        None => Some(format!("It isn't in a terminal Codealong can press keys in ({}).", crate::launch::presses_keys_in())),
    }
}

/// Delivers or denies the message a session holds, by its dialog's keys: End then Return picks
/// Deliver wherever the focus is (its choices wrap), and Escape denies. Only while it's the only
/// one held, since denying one leaves nothing on record to say which comes next.
fn answer_held(a: &ClaudeCode, ctx: &crate::harness::Ctx, s: &crate::model::Session, id: &str, answer: crate::harness::Answer) -> anyhow::Result<String> {
    use crate::launch::Key;
    let Some((held, reg)) = waiting_hold(a, ctx, s) else {
        anyhow::bail!("It isn't holding a message for you any more.");
    };
    if held.id != id {
        anyhow::bail!("It holds another message now, from {}. Look again before answering.", held.from);
    }
    if held.count > 1 {
        anyhow::bail!("It holds {} messages, and only where it runs shows which comes first. Answer them there.", held.count);
    }
    // A call the plugin's hook handed over has its prompt up in front of this one.
    if asks::of_session(&s.id).is_some() {
        anyhow::bail!("It's asking you about something else first. Answer that, then this.");
    }
    let keys: &[Key] = match answer {
        crate::harness::Answer::Allow => &[Key::End, Key::Return],
        crate::harness::Answer::Deny => &[Key::Escape],
        crate::harness::Answer::Always => anyhow::bail!("A held message is delivered or denied one at a time."),
    };
    if reg.kind.as_deref() == Some("bg") {
        let home = ctx.account.home_path();
        let job = reg.job_id.as_deref().ok_or_else(|| anyhow::anyhow!("Claude Code didn't record which job it is."))?;
        let socket = daemon_socket(&home).ok_or_else(|| anyhow::anyhow!("Its account's background service isn't running."))?;
        let writes: Vec<&[u8]> = keys.iter().map(|k| k.typed().as_bytes()).collect();
        daemon_type(&socket, &home.join("daemon/control.key"), job, &writes)?;
    } else {
        let Some(host) = crate::launch::terminal_host(reg.pid, Path::new(&s.cwd))? else {
            anyhow::bail!("It isn't in a terminal Codealong can press keys in ({}). Open it to answer.", crate::launch::presses_keys_in());
        };
        for key in keys {
            crate::launch::press_key(&host, *key)?;
        }
    }
    Ok(match answer {
        crate::harness::Answer::Deny => format!("Denied the message from {}; Claude Code tells its sender it was declined.", held.from),
        _ => format!("Delivered the message from {}.", held.from),
    })
}

/// Whether Claude Code's plan prompt puts "Yes, clear context…" first, as 1, for a session in
/// `cwd` on the account at `home`: `showClearContextOnPlanAccept` in its settings (2.1.292; off by
/// default), the account's settings.json, then the repo's `.claude/settings.json` and
/// `.claude/settings.local.json`, the later winning, as Claude Code reads them.
fn clears_context_first(home: &Path, cwd: &Path) -> bool {
    let files = [home.join("settings.json"), cwd.join(".claude/settings.json"), cwd.join(".claude/settings.local.json")];
    clears_context_in(files.iter().filter_map(|f| std::fs::read_to_string(f).ok()))
}

/// What settings files' texts, in order, say of `showClearContextOnPlanAccept`: the last that says.
fn clears_context_in(texts: impl Iterator<Item = String>) -> bool {
    texts.filter_map(|t| serde_json::from_str::<Value>(&t).ok()?.get("showClearContextOnPlanAccept")?.as_bool()).last().unwrap_or(false)
}

/// Whether a session in `mode` holds another session's message whose sender doesn't claim
/// `bypass` (claude/peer.rs): in `bypassPermissions` it does, and in `plan` it may, since plan
/// entered from bypass keeps bypass's class. A mode its transcript doesn't say (none in the part
/// `permission_mode` reads) could be either, so it counts as holding.
fn holds_in(mode: Option<&str>) -> bool {
    !matches!(mode, Some(m) if m != "bypassPermissions" && m != "plan")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The sessions Codealong runs sign in as the account's terminal sessions do: `~/.claude` is
    /// never named, and another folder always is.
    #[test]
    fn the_adapter_names_the_folder_only_when_it_isnt_the_usual_one() {
        let usual = crate::util::home().join(".claude");
        assert_eq!(acp_launch("'/a/claude-agent-acp'".into(), &usual), ("/usr/bin/env -u CLAUDE_CONFIG_DIR '/a/claude-agent-acp'".to_string(), Vec::new()));
        let other = crate::util::home().join(".claude2");
        let (program, env) = acp_launch("'/a/claude-agent-acp'".into(), &other);
        assert_eq!((program.as_str(), env), ("'/a/claude-agent-acp'", vec![("CLAUDE_CONFIG_DIR".to_string(), other.to_string_lossy().into_owned())]));
    }

    /// A session holds another's message in bypass, may in plan, which keeps the class it was
    /// entered from, and is taken to where its mode isn't known; in the others it doesn't.
    #[test]
    fn holds_messages_in_bypass_in_plan_and_where_its_mode_isnt_known() {
        for mode in [Some("bypassPermissions"), Some("plan"), None] {
            assert!(holds_in(mode), "{mode:?}");
        }
        for mode in ["default", "acceptEdits", "dontAsk"] {
            assert!(!holds_in(Some(mode)), "{mode}");
        }
    }

    /// A plan's 1 clears the conversation when `showClearContextOnPlanAccept` is on, so Allow
    /// is refused then; the later settings file wins, and it's off when none says.
    #[test]
    fn a_plan_whose_first_answer_clears_context_isnt_pressed() {
        let t = |s: &[&str]| clears_context_in(s.iter().map(|x| x.to_string()));
        assert!(!t(&[]));
        assert!(!t(&[r#"{"model":"opus"}"#]));
        assert!(t(&[r#"{"showClearContextOnPlanAccept":true}"#]));
        assert!(!t(&[r#"{"showClearContextOnPlanAccept":true}"#, r#"{"showClearContextOnPlanAccept":false}"#]));
        assert!(t(&[r#"{"showClearContextOnPlanAccept":false}"#, "not json", r#"{"showClearContextOnPlanAccept":true}"#]));
    }

    #[test]
    fn questions_show_in_the_conversation_and_wait_where_the_session_runs() {
        use crate::harness::Adapter;
        let home = crate::fixtures::scratch("questions");
        let dir = home.join("projects/-x");
        std::fs::create_dir_all(home.join("sessions")).unwrap();
        std::fs::create_dir_all(&dir).unwrap();
        let id = "5c1d7e2a";
        let ask = r#"{"type":"assistant","timestamp":"2026-09-30T20:33:55Z","message":{"content":[{"type":"tool_use","id":"toolu_q","name":"AskUserQuestion","input":{"questions":[{"question":"Which limit?","header":"Limit","multiSelect":false,"options":[{"label":"100 days (Recommended)","description":"A quarter and a bit."},{"label":"45 days"}]},{"question":"Which days?","multiSelect":true,"options":[{"label":"Mon"},{"label":"Tue"}]}]}}]}}"#;
        let answered = r#"{"type":"user","timestamp":"2026-09-30T20:40:10Z","message":{"content":[{"type":"tool_result","tool_use_id":"toolu_q","content":"Your questions have been answered"}]},"toolUseResult":{"questions":[],"answers":{"Which limit?":"100 days (Recommended)","Which days?":["Mon","Tue"],"Blank":" "},"annotations":{}}}"#;
        std::fs::write(dir.join(format!("{id}.jsonl")), format!("{ask}\n")).unwrap();
        let account = crate::config::AccountCfg { name: "claude".into(), harness: "claude".into(), home: home.to_string_lossy().into(), command: None, start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false };
        let ctx = crate::harness::Ctx { account: &account };
        let session = Session { status: crate::model::Status::Needs, live: true, pid: Some(4242), title: "Limits".into(), cwd: "/x".into(), ..Session::new(Harness::Claude, "claude", id) };
        let registry = |waiting_for: &str| std::fs::write(home.join("sessions/4242.json"), serde_json::json!({ "pid": 4242, "sessionId": id, "cwd": "/x", "status": "waiting", "waitingFor": waiting_for }).to_string()).unwrap();

        // The call's questions come with it, for the panel to show as questions.
        let page = ClaudeCode.conversation(&ctx, &session, None, 50).unwrap();
        let qs = &page.items[0].questions;
        assert_eq!((qs.len(), qs[0].header.as_deref(), qs[0].options[0].description.as_deref(), qs[1].multi_select), (2, Some("Limit"), Some("A quarter and a bit."), true));

        // Asked with no hook to hand them over: they show, and are answered where the session runs.
        registry("input needed");
        let p = ClaudeCode.pending(&ctx, &session).unwrap();
        assert_eq!((p.id.as_str(), p.label.as_str(), p.questions.len(), p.cannot.is_some()), ("toolu_q", "Asks you · Which limit?", 2, true));
        // A permission prompt isn't a question, and nor is a call without any.
        registry("permission prompt");
        assert!(ClaudeCode.pending(&ctx, &session).map_or(true, |p| p.questions.is_empty()));

        // Answered: the result has your picks, several where it took them.
        std::fs::write(dir.join(format!("{id}.jsonl")), format!("{ask}\n{answered}\n")).unwrap();
        let page = ClaudeCode.conversation(&ctx, &session, None, 50).unwrap();
        let r = &page.items[1];
        assert_eq!((r.kind, r.id.as_deref()), ("result", Some("toolu_q")));
        assert_eq!(r.answers.get("Which limit?"), Some(&vec!["100 days (Recommended)".to_string()]));
        assert_eq!(r.answers.get("Which days?"), Some(&vec!["Mon".to_string(), "Tue".to_string()]));
        assert!(!r.answers.contains_key("Blank"));
        registry("input needed");
        assert!(ClaudeCode.pending(&ctx, &session).is_none(), "nothing waits once it's answered");
    }

    /// A background helper's prompt shows in its session, its call only in the helper's own log,
    /// which can be under another project's folder than the session's transcript. It's taken as
    /// the one only when nothing else could be: a key pressed for it answers whatever is up.
    #[test]
    fn a_background_helpers_prompt_is_found_in_its_log_when_its_plainly_the_one() {
        let home = crate::fixtures::scratch("claude-helpers");
        let logs_in = |project: &str| {
            let dir = home.join("projects").join(project).join("s1").join("subagents");
            std::fs::create_dir_all(&dir).unwrap();
            dir
        };
        let (worktree, own) = (logs_in("-Users-me-app--claude-worktrees-mail"), logs_in("-Users-me-app"));
        // Every line of a helper's log is a sidechain.
        let call = |id: &str, at: &str, command: &str| serde_json::json!({ "type": "assistant", "isSidechain": true, "timestamp": at, "message": { "content": [{ "type": "tool_use", "id": id, "name": "Bash", "input": { "command": command } }] } }).to_string();
        let result = |id: &str, at: &str| serde_json::json!({ "type": "user", "isSidechain": true, "timestamp": at, "message": { "content": [{ "type": "tool_result", "tool_use_id": id, "content": "ok" }] } }).to_string();
        let write = |path: &Path, lines: &[String]| std::fs::write(path, lines.join("\n") + "\n").unwrap();
        let cli = worktree.join("agent-a1.jsonl");
        std::fs::write(worktree.join("agent-a1.meta.json"), r#"{"agentType":"fork","description":"Build the CLI","requestShape":"background"}"#).unwrap();
        write(&cli, &[call("t1", "2026-10-08T23:39:40Z", "ls"), result("t1", "2026-10-08T23:39:41Z"), call("t2", "2026-10-08T23:39:53Z", "python3 edit.py")]);
        let ui = own.join("agent-b2.jsonl");
        write(&ui, &[call("u1", "2026-10-08T23:39:00Z", "npm test"), result("u1", "2026-10-08T23:39:30Z")]);
        let waiting = parse_rfc3339_ms("2026-10-08T23:40:06Z").unwrap();
        let logs = helper_logs(&home, "s1");
        assert_eq!(logs.len(), 2, "both folders' helpers");
        // One helper ends on a call with no result, made a moment before the prompt showed.
        let found = helper_waiting(&logs, waiting).unwrap();
        assert_eq!((found.id.as_deref(), found.text.as_str()), (Some("t2"), "Bash · `python3 edit.py` · its helper “Build the CLI”"));
        // Made long before the session said it waits, it's a command still running.
        assert!(helper_waiting(&logs, waiting + 10 * 60_000).is_none());
        // Two helpers on calls with no result: either could be the one, so neither is.
        write(&ui, &[call("u1", "2026-10-08T23:40:00Z", "npm test")]);
        assert!(helper_waiting(&logs, waiting).is_none());
        // A helper that went on after its call isn't waiting on it.
        write(&ui, &[call("u1", "2026-10-08T23:39:00Z", "npm test"), result("u1", "2026-10-08T23:39:30Z")]);
        write(&cli, &[call("t2", "2026-10-08T23:39:53Z", "python3 edit.py"), serde_json::json!({ "type": "assistant", "isSidechain": true, "timestamp": "2026-10-08T23:39:59Z", "message": { "content": [{ "type": "text", "text": "Done." }] } }).to_string()]);
        assert!(helper_waiting(&logs, waiting).is_none());
        assert!(helper_logs(&home, "s2").is_empty());
    }

    #[test]
    fn finds_the_call_running_at_the_end_of_a_transcript() {
        let dir = crate::fixtures::scratch("claude-open-calls");
        let path = dir.join("s.jsonl");
        let call = |id: &str, command: &str| serde_json::json!({ "type": "assistant", "timestamp": "2026-09-27T16:00:00.000Z", "message": { "content": [{ "type": "tool_use", "id": id, "name": "Bash", "input": { "command": command } }] } }).to_string();
        let result = |id: &str| serde_json::json!({ "type": "user", "timestamp": "2026-09-27T16:00:01.000Z", "message": { "content": [{ "type": "tool_result", "tool_use_id": id, "content": "ok" }] } }).to_string();
        std::fs::write(&path, [call("t1", "ls"), result("t1"), call("t2", "codealong todo done t-abc")].join("\n") + "\n").unwrap();
        assert_eq!(open_calls(&path), vec!["codealong todo done t-abc".to_string()]);
        std::fs::write(&path, [call("t1", "ls"), result("t1")].join("\n") + "\n").unwrap();
        assert!(open_calls(&path).is_empty());
    }

    #[test]
    fn a_copy_moved_from_another_account_goes_on_from_its_transcript_there_until_it_has_its_own() {
        use crate::harness::Reader as _;
        let (old_home, new_home) = (crate::fixtures::scratch("moved-from"), crate::fixtures::scratch("moved-to"));
        let account = |name: &str, home: &Path| AccountCfg { name: name.into(), harness: "claude".into(), home: home.to_string_lossy().into_owned(), command: None, start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false };
        let (from, to) = (account("claude", &old_home), account("claude2", &new_home));
        let project = old_home.join("projects/-Users-me-app");
        std::fs::create_dir_all(&project).unwrap();
        let transcript = project.join("s1.jsonl");
        std::fs::write(&transcript, r#"{"type":"user","sessionId":"s1","cwd":"/Users/me/app","timestamp":"2026-09-23T15:00:00.000Z","message":{"role":"user","content":"Fix the build"}}
"#).unwrap();
        let seen = crate::state::SeenState::default();
        let look = Look { seen: &seen, since: 0, now: parse_rfc3339_ms("2026-09-23T16:00:00.000Z").unwrap(), ps: Default::default() };
        let mut reader = Reader::default();
        reader.apply(&from, std::slice::from_ref(&transcript), &look);
        let moved = |key: &str, how| Some(SessionOrigin { key: key.into(), how });
        // The reader shows the copy with the conversation it was moved with.
        let source = reader.moved_copy("claude2", moved("claude:claude:s1", OriginHow::Fork)).unwrap();
        assert_eq!((source.id.as_str(), source.key.as_str(), source.transcript.0), ("s1", "claude:claude:s1", transcript.as_path()));
        assert_eq!(source.transcript.1.first_prompt.as_deref(), Some("Fix the build"));
        // A fork in its own account, a session another one started, and none at all aren't moves.
        assert!(reader.moved_copy("claude", moved("claude:claude:s1", OriginHow::Fork)).is_none());
        assert!(reader.moved_copy("claude2", moved("claude:claude:s1", OriginHow::Started)).is_none());
        assert!(reader.moved_copy("claude2", None).is_none());
        // Moved as a background job, the job's roster names the conversation by its ID alone, and
        // the copy is said to come from the account that has it; a fork of one of its own
        // account's sessions, or of one no account has any more, from its own.
        assert_eq!(reader.parent_key("claude2", false, "s1"), "claude:claude:s1");
        assert_eq!(reader.parent_key("claude2", true, "s1"), "claude:claude2:s1");
        assert_eq!(reader.parent_key("claude2", false, "gone"), "claude:claude2:gone");

        // Its panel, a fork of it and a side question read that transcript, in the other account's folder.
        let copy = Session { live: true, continues: Some("s1".into()), origin: moved("claude:claude:s1", OriginHow::Fork), ..Session::new(Harness::Claude, "claude2", "c0") };
        let home_of = |name: &str| (name == "claude").then(|| old_home.to_path_buf());
        assert_eq!(records_in(&to, &copy, home_of), [transcript.clone()]);
        assert_eq!(shell_word(&transcript.to_string_lossy()), crate::util::sh_quote(&transcript.to_string_lossy()));
        assert_eq!(shell_word("s1"), "s1");
        // Once it has written its own, the board goes on from that.
        let own = new_home.join("projects/-Users-me-app");
        std::fs::create_dir_all(&own).unwrap();
        std::fs::write(own.join("c0.jsonl"), "{}\n").unwrap();
        assert_eq!(records_in(&to, &Session { continues: None, ..copy.clone() }, home_of), [own.join("c0.jsonl")]);
        // A job in its own account whose conversation is gone has none.
        let job = Session { origin: moved("claude:claude2:s0", OriginHow::Fork), continues: Some("s0".into()), ..copy.clone() };
        assert!(records_in(&to, &job, home_of).is_empty());
    }

    #[test]
    fn the_claude_apps_forks_and_the_tasks_it_starts_come_from_their_sessions() {
        use crate::harness::Reader as _;
        let home = crate::fixtures::scratch("app-forks");
        let account = AccountCfg { name: "claude".into(), harness: "claude".into(), home: home.to_string_lossy().into_owned(), command: None, start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false };
        let project = home.join("projects/-Users-me-app");
        std::fs::create_dir_all(&project).unwrap();
        let line = |session: &str, at: &str, text: &str| serde_json::json!({ "type": "user", "sessionId": session, "cwd": "/Users/me/app", "entrypoint": "claude-desktop", "timestamp": at, "origin": { "kind": "human" }, "message": { "role": "user", "content": text } }).to_string() + "\n";
        let title = |session: &str, title: &str| serde_json::json!({ "type": "custom-title", "customTitle": title, "sessionId": session }).to_string() + "\n";
        let parent = line("p1", "2026-10-09T10:00:00.000Z", "Plan the coordinator") + &title("p1", "Coordinator");
        // A btw copies the conversation as it was, then goes on, later than the session it copied.
        let btw = parent.clone() + &title("f1", "btw: who keeps the drawings?") + &line("f1", "2026-10-09T11:00:00.000Z", "who keeps the drawings?");
        let task = line("t1", "2026-10-09T10:30:00.000Z", "Write the spec");
        let paths = [("p1", parent), ("f1", btw), ("t1", task)].map(|(id, text)| {
            let path = project.join(format!("{id}.jsonl"));
            std::fs::write(&path, text).unwrap();
            path
        });
        let seen = crate::state::SeenState::default();
        let look = Look { seen: &seen, since: 0, now: parse_rfc3339_ms("2026-10-09T12:00:00.000Z").unwrap(), ps: Default::default() };
        let mut reader = Reader::default();
        reader.apply(&account, &paths, &look);
        let board = |reader: &Reader| reader.sessions(&account, &look).into_iter().map(|s| (s.id.clone(), (s.title.clone(), s.origin.map(|o| (o.key, o.how))))).collect::<HashMap<_, _>>();
        let sessions = board(&reader);
        // The fork's transcript is its own, and the session it copied keeps its own too.
        assert_eq!(sessions["p1"], ("Coordinator".to_string(), None));
        assert_eq!(sessions["f1"], ("btw: who keeps the drawings?".to_string(), Some(("claude:claude:p1".to_string(), OriginHow::Fork))));
        assert_eq!(sessions["t1"].1, None);

        // The app's records say so too, and which session's agent offered a task you started.
        let records = home.join("records/acct/org");
        std::fs::create_dir_all(&records).unwrap();
        std::fs::write(records.join("local_p.json"), r#"{"sessionId":"local_p","cliSessionId":"p1"}"#).unwrap();
        std::fs::write(records.join("local_f.json"), r#"{"sessionId":"local_f","cliSessionId":"f1","forkedFromSessionId":"local_p","spawnedFrom":{"sessionId":"local_p","btw":true}}"#).unwrap();
        std::fs::write(records.join("local_t.json"), r#"{"sessionId":"local_t","cliSessionId":"t1","spawnedFrom":{"sessionId":"local_p","taskId":"task_1"}}"#).unwrap();
        reader.app_records.refresh(&home.join("records"));
        let sessions = board(&reader);
        assert_eq!(sessions["f1"].1, Some(("claude:claude:p1".to_string(), OriginHow::Fork)));
        assert_eq!(sessions["t1"].1, Some(("claude:claude:p1".to_string(), OriginHow::Started)));
        assert_eq!(sessions["p1"].1, None);
    }

    #[test]
    fn a_moved_copy_closed_before_its_first_message_stays_to_go_on_with() {
        use crate::harness::Reader as _;
        use crate::origins::{Entry, Store};
        let (old_home, new_home) = (crate::fixtures::scratch("closed-copy-from"), crate::fixtures::scratch("closed-copy-to"));
        let account = |name: &str, home: &Path| AccountCfg { name: name.into(), harness: "claude".into(), home: home.to_string_lossy().into_owned(), command: None, start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false };
        let (from, to) = (account("claude", &old_home), account("claude2", &new_home));
        let project = old_home.join("projects/-Users-me-app");
        std::fs::create_dir_all(&project).unwrap();
        let transcript = project.join("s1.jsonl");
        std::fs::write(&transcript, r#"{"type":"user","sessionId":"s1","cwd":"/Users/me/app","timestamp":"2026-09-23T15:00:00.000Z","message":{"role":"user","content":"Fix the build"}}
{"type":"assistant","sessionId":"s1","cwd":"/Users/me/app","timestamp":"2026-09-23T15:01:00.000Z","message":{"content":[{"type":"text","text":"Fixed it."}]}}
"#).unwrap();
        // You'd seen the conversation's last turn before moving it.
        let now = parse_rfc3339_ms("2026-09-23T16:00:00.000Z").unwrap();
        let mut seen = crate::state::SeenState::default();
        seen.mark("claude:claude:s1", now - 60_000);
        let look = Look { seen: &seen, since: 0, now, ps: Default::default() };
        let mut reader = Reader::default();
        reader.apply(&from, std::slice::from_ref(&transcript), &look);
        let noted = |key: &str, from: &str, how| (key.to_string(), Entry { from: from.into(), how, at: 0, helper: false, report: false, known: false });
        let origins = Store {
            sessions: [noted("claude:claude2:c0", "claude:claude:s1", OriginHow::Fork), noted("claude:claude:f1", "claude:claude:s1", OriginHow::Fork), noted("claude:claude2:b1", "claude:claude:s1", OriginHow::Started)].into(),
            pending: Vec::new(),
        };

        // Its process ended before its first message, so Claude Code has nothing of it: it shows
        // stopped, with the conversation it was moved with. A fork in the same account and a
        // session the other one started aren't moves.
        let copies = reader.closed_copies(&to, &HashSet::new(), &origins, &look);
        assert_eq!(copies.iter().map(|s| s.key.as_str()).collect::<Vec<_>>(), ["claude:claude2:c0"]);
        let copy = &copies[0];
        assert_eq!((copy.status, copy.live, copy.cwd.as_str(), copy.first_prompt.as_deref(), copy.continues.as_deref()), (crate::model::Status::Stopped, false, "/Users/me/app", Some("Fix the build"), Some("s1")));
        assert_eq!(copy.reply_tail.as_deref(), Some("Fixed it."));
        assert_eq!(copy.origin, Some(SessionOrigin { key: "claude:claude:s1".into(), how: OriginHow::Fork }));
        assert!(reader.closed_copies(&from, &HashSet::new(), &origins, &look).is_empty());

        // Opened, or sent a message, it's made again from that conversation under its own ID.
        let path = crate::util::sh_quote(&transcript.to_string_lossy());
        assert_eq!(copy.resume_command, Some(format!("claude2 --resume {path} --fork-session --session-id c0")));
        let home_of = |name: &str| (name == "claude").then(|| old_home.to_path_buf());
        let (input, _) = claude_resume_input(&to, &resume_args(&to, copy, home_of), "Go on", None, None);
        assert_eq!(input, format!("claude2 --resume {path} --fork-session --session-id c0 \"${}\"\n", crate::start::PROMPT_VAR));

        // While its process runs the registry has it, and the reader shows it from there.
        assert!(reader.closed_copies(&to, &HashSet::from(["c0"]), &origins, &look).is_empty());
        // With the conversation it was moved with gone too, there's nothing left to go on with.
        assert!(Reader::default().closed_copies(&to, &HashSet::new(), &origins, &look).is_empty());
        // Archived, it stays archived until it's active again.
        let archived = crate::state::SeenState { archived: vec![crate::state::ArchivedSession { key: "claude:claude2:c0".into(), at: copy.last_activity }], ..Default::default() };
        let look_archived = Look { seen: &archived, ..look };
        assert!(reader.closed_copies(&to, &HashSet::new(), &origins, &look_archived)[0].archived);

        // Once its first message has written its own transcript, it resumes by its own ID.
        let own = new_home.join("projects/-Users-me-app");
        std::fs::create_dir_all(&own).unwrap();
        std::fs::write(own.join("c0.jsonl"), "{}\n").unwrap();
        assert_eq!(resume_args(&to, &Session { continues: None, ..copy.clone() }, home_of), "c0");
    }

    #[test]
    fn the_reader_takes_in_transcripts_and_hands_new_lines_on() {
        use crate::harness::Reader as _;
        use std::io::Write;

        let home = crate::fixtures::scratch("claude-reader");
        let project = home.join("projects/-Users-me-app");
        std::fs::create_dir_all(project.join("s1/subagents")).unwrap();
        let account = AccountCfg { name: "claude9".into(), harness: "claude".into(), home: home.to_string_lossy().into_owned(), command: None, start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false };
        let seen = crate::state::SeenState::default();
        let now = parse_rfc3339_ms("2026-09-23T16:00:00.000Z").unwrap();
        let look = Look { seen: &seen, since: 0, now, ps: Default::default() };
        let mut reader = Reader::default();
        let transcript = project.join("s1.jsonl");
        assert!(reader.owns(&account, &transcript) && reader.owns(&account, &home.join("sessions/41.json")));
        assert!(!reader.owns(&account, &home.join("settings.json")), "only its sessions and transcripts are its to read");

        let mut file = std::fs::File::create(&transcript).unwrap();
        writeln!(file, r#"{{"type":"user","sessionId":"s1","cwd":"/Users/me/app","timestamp":"2026-09-23T15:00:00.000Z","message":{{"role":"user","content":"Fix the build"}}}}"#).unwrap();
        let first = reader.apply(&account, std::slice::from_ref(&transcript), &look);
        assert!(first.dirty && first.added && first.lines.is_empty(), "a transcript read for the first time is a new session");
        // Its lines so far are handed on whole, for the engine to read once.
        assert_eq!(first.found, [(transcript.clone(), "claude:claude9:s1".to_string())]);
        assert_eq!(reader.cwds(), ["/Users/me/app"]);
        let sessions = reader.sessions(&account, &look);
        assert_eq!((sessions.len(), sessions[0].key.as_str()), (1, "claude:claude9:s1"));

        // What it wrote since is handed on, from where the last read ended.
        let before = std::fs::metadata(&transcript).unwrap().len();
        writeln!(file, r#"{{"type":"assistant","timestamp":"2026-09-23T15:01:00.000Z","message":{{"content":[{{"type":"text","text":"Done."}}]}}}}"#).unwrap();
        let next = reader.apply(&account, std::slice::from_ref(&transcript), &look);
        assert!(next.dirty && !next.added && next.found.is_empty());
        assert_eq!(next.lines.iter().map(|l| (l.key.as_str(), l.from, l.to)).collect::<Vec<_>>(), [("claude:claude9:s1", before, std::fs::metadata(&transcript).unwrap().len())]);

        // A subagent's log counts for its session, and each stretch of it is handed on once.
        let log = project.join("s1/subagents/agent-a.jsonl");
        std::fs::write(&log, "{}\n").unwrap();
        let sub = reader.apply(&account, std::slice::from_ref(&log), &look);
        assert_eq!(sub.lines.iter().map(|l| (l.key.as_str(), l.from, l.to)).collect::<Vec<_>>(), [("claude:claude9:s1", 0, 3)]);
        assert!(reader.apply(&account, std::slice::from_ref(&log), &look).lines.iter().all(|l| l.from == l.to));
        assert!(reader.logs().contains(&(log.clone(), "claude:claude9:s1".to_string())));

        // A subagent started once the session had gone into a worktree logs under that
        // worktree's project; a session the reader doesn't keep has none listed.
        let elsewhere = home.join("projects/-Users-me-app-wt/s1/subagents/agent-b.jsonl");
        let stranger = home.join("projects/-Users-me-app-wt/s2/subagents/agent-c.jsonl");
        for f in [&elsewhere, &stranger] {
            std::fs::create_dir_all(f.parent().unwrap()).unwrap();
            std::fs::write(f, "{}\n").unwrap();
        }
        let mut logs = reader.logs();
        logs.sort();
        let key = |id: &str| format!("claude:claude9:{id}");
        // Paths sort by their parts: the project before the one whose name runs on, a folder before
        // the file named as it is with more after.
        assert_eq!(logs, [(log, key("s1")), (transcript.clone(), key("s1")), (elsewhere, key("s1"))]);

        // A transcript come on late, as one resumed from before the window is, goes with its
        // subagents' logs so far, wherever they are; after that only their new lines are handed on.
        let old = project.join("s3.jsonl");
        let old_sub = home.join("projects/-Users-me-app-wt/s3/subagents/agent-d.jsonl");
        std::fs::create_dir_all(old_sub.parent().unwrap()).unwrap();
        std::fs::write(&old_sub, "{}\n").unwrap();
        std::fs::write(&old, "{\"type\":\"user\",\"sessionId\":\"s3\",\"cwd\":\"/Users/me/app\",\"timestamp\":\"2026-09-20T15:00:00.000Z\",\"message\":{\"role\":\"user\",\"content\":\"Publish the report\"}}\n").unwrap();
        let mut late = reader.apply(&account, std::slice::from_ref(&old), &look).found;
        late.sort();
        assert_eq!(late, [(old, key("s3")), (old_sub.clone(), key("s3"))]);
        assert!(reader.apply(&account, std::slice::from_ref(&old_sub), &look).lines.iter().all(|l| l.from == l.to));
    }
}
