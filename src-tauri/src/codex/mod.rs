//! Codex, in its app and in its terminal UI. Everything Codealong knows of its files, sockets
//! and commands is in this folder: here the adapter (`Codex`), which answers for a thread and
//! for an account, and the reader, which keeps the thread names and the rollouts read for the
//! board. Beside it: rollouts and the thread index (rollout.rs), the conversation a rollout
//! holds (conversation.rs), its app-server, for what Codex does itself (appserver.rs), the
//! socket the app's windows talk on (ipc.rs), its terminal UIs (terminals.rs), side
//! conversations (side.rs), the models an account offers (models.rs), plan usage (usage.rs) and
//! whether OpenAI may train on the account (training.rs). See .claude/rules/codex.md.

mod appserver;
mod conversation;
pub mod ipc;
mod models;
mod rollout;
mod side;
mod terminals;
mod training;
mod trust;
mod usage;

use std::collections::{BTreeSet, HashMap, HashSet};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::config::AccountCfg;
use crate::harness::{Applied, Look, NewLines};
use crate::model::{Harness, Session, Usage};
use crate::transcript::Page;
use crate::util::{mtime_ms, parallel_map, parse_rfc3339_ms, read_first_line};

use conversation::open_calls;
use ipc::{codex_interrupt_turn, codex_start_turn};
use rollout::{app_project_roots, archived_rollout_files, chats_dir, parse_meta, parse_rollout, prose_of, read_index, rollout_files, thread_id_of};
use side::{last_turn, SideThread};
use terminals::{codex_cli, held_rollouts, latest_day, pair_terminals, terminals, Terminal};

pub use conversation::{conversation_items, with_images_before};
#[cfg(test)]
pub use rollout::PendingCall;
pub use rollout::ThreadInfo;

/// The Codex app's bundle ID. The ChatGPT chat app has the same name, so the app is known by this.
pub const APP_BUNDLE_ID: &str = "com.openai.codex";

/// Codex (`harness::Adapter`): its rollout for the conversation. Approvals aren't recorded
/// anywhere Codealong can read, so they're answered in Codex.
/// How many folders down from a skills folder Codex looks for a `SKILL.md`.
const SKILL_DEPTH: usize = 6;

pub struct Codex;

impl crate::harness::Adapter for Codex {
    fn harness(&self) -> crate::model::Harness {
        crate::model::Harness::Codex
    }

    fn spec(&self) -> &'static crate::detect::Spec {
        const SPEC: crate::detect::Spec = crate::detect::Spec { harness: "codex", program: Some("codex"), default_command: None, var: Some(("CODEX_HOME", None)), default_dir: "~/.codex", siblings: Some(".codex") };
        &SPEC
    }

    /// Its sign-in is `auth.json` in its folder.
    fn signed_in(&self, home: &Path) -> (bool, Option<String>) {
        (home.join("auth.json").is_file(), None)
    }

    /// Codex wants its folder to exist before it will use it.
    fn sign_in_command(&self, account: &crate::config::AccountCfg) -> String {
        format!("mkdir -p {} && {} login", account.home, codex_cli(account.command()))
    }

    /// Improve the model for everyone, in ChatGPT's data controls.
    fn training(&self, account: &crate::config::AccountCfg) -> crate::training::Training {
        training::read(account)
    }

    fn stop_training(&self, account: &crate::config::AccountCfg) -> anyhow::Result<()> {
        training::stop(account)
    }

    fn installed(&self, _home: &Path, on_path: &dyn Fn(&str) -> bool) -> bool {
        on_path("codex") || crate::detect::codex_app_cli().is_some()
    }

    fn folder_use(&self, dir: &Path) -> crate::harness::FolderUse {
        let sessions = dir.join("sessions");
        crate::harness::FolderUse {
            has_data: sessions.is_dir() || dir.join("auth.json").is_file(),
            last_used: [dir.join("session_index.jsonl"), latest_day(&sessions).unwrap_or_default()].iter().filter_map(|p| mtime_ms(p)).max(),
            signed_in: false,
        }
    }

    fn skills_dir(&self, account: &crate::config::AccountCfg) -> Option<PathBuf> {
        Some(account.home_path().join("skills"))
    }

    /// `.agents/skills` in each folder from the project's root to the session's, and the
    /// project's `.codex/skills` (its core-skills loader, which looks six folders down).
    fn worktree_skills(&self, folders: &[PathBuf]) -> Vec<crate::skills::Root> {
        folders.iter().flat_map(|dir| [crate::skills::Root::skills(dir.join(".agents/skills"), SKILL_DEPTH), crate::skills::Root::skills(dir.join(".codex/skills"), SKILL_DEPTH)]).collect()
    }

    /// Its home's `skills` (the ones Codex ships are in `.system` there) and `~/.agents/skills`.
    fn account_skills(&self, account: &crate::config::AccountCfg) -> Vec<crate::skills::Root> {
        let own = account.home_path().join("skills");
        vec![crate::skills::Root::skills(own.clone(), SKILL_DEPTH), crate::skills::Root::skills(own.join(".system"), SKILL_DEPTH), crate::skills::Root::skills(crate::util::home().join(".agents/skills"), SKILL_DEPTH)]
    }

    /// `$name` anywhere in a prompt's text loads the skill, however the prompt gets there: its
    /// core scans every text input for them. (Two skills of one name are both passed over; the
    /// TUI writes a link to the file then, which needs the path as the session sees it.)
    fn skill_mention(&self, name: &str) -> Option<String> {
        Some(format!("${name}"))
    }

    fn acp_command(&self, account: &crate::config::AccountCfg) -> std::result::Result<(String, Vec<(String, String)>), String> {
        let program = crate::acp::package_program("codex-acp", self.harness())?;
        Ok((program, vec![("CODEX_HOME".into(), account.home_path().to_string_lossy().into_owned())]))
    }

    /// The Codex app runs the account in `~/.codex`, where its threads usually start.
    fn places(&self, account: &crate::config::AccountCfg, home: &Path) -> std::result::Result<Vec<crate::start::Place>, String> {
        use crate::start::Place;
        Ok(if account.home_path() == home.join(".codex") { vec![Place::CodexApp, Place::Terminal, Place::Background, Place::Codealong] } else { vec![Place::Terminal, Place::Background, Place::Codealong] })
    }

    fn session_cwds(&self, home: &Path, since: i64) -> Vec<String> {
        crate::util::parallel_map(rollout_files(home, since, &HashSet::new()), |(p, _)| read_first_line(&p, 4 * 1024 * 1024).and_then(|l| parse_meta(&l)).map(|m| m.cwd))
    }

    /// Its rollout, by the id in the file name, archived or not.
    fn records(&self, account: &crate::config::AccountCfg, s: &crate::model::Session) -> Vec<PathBuf> {
        let suffix = format!("-{}.jsonl", s.id);
        let mut out = Vec::new();
        for root in ["sessions", "archived_sessions"] {
            walk_files(&account.home_path().join(root), 4, &mut |p| {
                if p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("rollout-") && n.ends_with(&suffix)) {
                    out.push(p.to_path_buf());
                }
            });
        }
        out
    }

    fn conversation(&self, ctx: &crate::harness::Ctx, s: &crate::model::Session, before: Option<u64>, limit: usize) -> anyhow::Result<Page> {
        let rollout = record(self, ctx, s)?;
        Ok(with_images_before(&rollout, crate::transcript::file_page(&rollout, conversation_items, before, limit)?))
    }

    /// A thread in a terminal is typed into; any other goes through the app.
    fn routes(&self, _account: &crate::config::AccountCfg, s: &crate::model::Session) -> Vec<crate::send::Route> {
        // A thread open in a terminal gets it typed there; one made in the CLI and closed
        // resumes in a new terminal; the app's own go to the app.
        match (s.surface, s.pid) {
            (crate::model::Surface::Cli, Some(pid)) => vec![crate::send::Route::Type { pid }],
            (crate::model::Surface::Cli, None) => vec![crate::send::Route::Resume],
            _ => vec![ipc::InApp.into()],
        }
    }

    fn conversation_after(&self, ctx: &crate::harness::Ctx, s: &crate::model::Session, after: u64) -> anyhow::Result<Page> {
        let rollout = record(self, ctx, s)?;
        Ok(with_images_before(&rollout, crate::transcript::file_newer(&rollout, conversation_items, after)?))
    }

    fn open_calls(&self, ctx: &crate::harness::Ctx, s: &crate::model::Session) -> Vec<String> {
        record(self, ctx, s).map(|p| open_calls(&p)).unwrap_or_default()
    }

    /// A terminal's thread runs its commands under that terminal's `codex`; the app's threads
    /// have no process of their own.
    fn runs_its_commands(&self) -> bool {
        true
    }

    /// `codex fork <id>` opens its terminal UI on a copy of the thread, an app's thread too.
    /// Codex picks the copy's id, so it shows on the board once it's written.
    fn fork(&self, ctx: &crate::harness::Ctx, s: &crate::model::Session) -> anyhow::Result<crate::harness::Typed> {
        Ok(fork_of(ctx.account, &s.id))
    }

    /// Codex has no background of its own: Codealong runs the copy over the account's ACP
    /// adapter, which forks the thread through its app-server (`acp::fork`).
    fn fork_background(&self, ctx: &crate::harness::Ctx, s: &crate::model::Session, cwd: &Path, prompt: &str) -> anyhow::Result<Option<String>> {
        crate::acp::fork(ctx.account, &s.id, cwd, prompt).map(Some)
    }

    /// An ephemeral fork of the thread on an app-server of Codealong's own, as Codex's `/side`
    /// makes one (`SideThread`), on the model the thread last ran on and with its sandbox and
    /// approval policy, so the copy may do what the thread may.
    fn side(&self, ctx: &crate::harness::Ctx, s: &crate::model::Session, cwd: &Path) -> anyhow::Result<crate::side::Recipe> {
        let turn = self.records(ctx.account, s).first().map(|p| last_turn(p)).unwrap_or_default();
        Ok(crate::side::Recipe {
            program: appserver::codex_binary(),
            args: ["app-server", "--listen", "stdio://"].iter().map(|a| a.to_string()).collect(),
            env: vec![("CODEX_HOME".into(), ctx.account.home_path().to_string_lossy().into_owned())],
            unset: Vec::new(),
            dialect: Box::new(SideThread::new(&s.id, cwd, turn)),
        })
    }

    /// An ephemeral thread of its own on the same app-server, read-only, which reads anywhere,
    /// so what it's handed needs no more than its path.
    fn fresh_side(&self, ctx: &crate::harness::Ctx, cwd: &Path, rules: &str, _reads: &Path) -> anyhow::Result<crate::side::Recipe> {
        Ok(crate::side::Recipe {
            program: appserver::codex_binary(),
            args: ["app-server", "--listen", "stdio://"].iter().map(|a| a.to_string()).collect(),
            env: vec![("CODEX_HOME".into(), ctx.account.home_path().to_string_lossy().into_owned())],
            unset: Vec::new(),
            dialect: Box::new(SideThread::fresh(cwd, rules)),
        })
    }

    /// Names the thread through Codex's app-server, as the Codex app does (`appserver::set_name`).
    /// The board reads names from the thread index; the app shows the name the next time it
    /// reads its threads.
    fn rename(&self, ctx: &crate::harness::Ctx, s: &crate::model::Session, name: &str) -> anyhow::Result<Vec<PathBuf>> {
        if s.archived {
            anyhow::bail!("Codex can't rename an archived thread. Unarchive it first.");
        }
        let home = ctx.account.home_path();
        appserver::set_name(&home, &s.id, name)?;
        Ok(vec![home.join("session_index.jsonl")])
    }

    /// Its terminal UI sends what's typed into the running turn on Return, and queues it for
    /// after the turn only on Tab ("Press {key:composer.queue} to queue a message when a task is
    /// running; otherwise it sends immediately", 0.157.1), so a prompt typed there goes in now
    /// whichever terminal it's in. The app's own threads take a steer through the window that
    /// runs them (`ipc::codex_steer_turn`).
    fn steers(&self, ctx: &crate::harness::Ctx, s: &crate::model::Session) -> bool {
        match (s.surface, s.pid) {
            (crate::model::Surface::Cli, Some(pid)) => crate::launch::terminal_kind(pid).is_some(),
            (crate::model::Surface::Cli, None) => false,
            _ => ctx.account.home_path().join("ipc/ipc.sock").exists(),
        }
    }

    /// The app's own Stop, through its windows' socket; Escape in a terminal, the key its
    /// terminal UI stops a turn with.
    fn interrupts(&self, ctx: &crate::harness::Ctx, s: &crate::model::Session) -> bool {
        match (s.surface, s.pid) {
            (crate::model::Surface::Cli, Some(pid)) => crate::launch::terminal_kind(pid).is_some_and(|k| k.presses_keys()),
            (crate::model::Surface::Cli, None) => false,
            _ => ctx.account.home_path().join("ipc/ipc.sock").exists(),
        }
    }

    fn interrupt(&self, ctx: &crate::harness::Ctx, s: &crate::model::Session) -> anyhow::Result<String> {
        match (s.surface, s.pid) {
            (crate::model::Surface::Cli, Some(pid)) => {
                let Some(host) = crate::launch::terminal_host(pid, Path::new(&s.cwd))? else {
                    anyhow::bail!("It isn't in a terminal Codealong can press keys in ({}). Open it to stop it there.", crate::launch::presses_keys_in());
                };
                crate::launch::press_key(&host, crate::launch::Key::Escape)?;
            }
            (crate::model::Surface::Cli, None) => anyhow::bail!("It isn't running any more."),
            _ => codex_interrupt_turn(&ctx.account.home_path(), &s.id)?,
        }
        Ok(format!("Stopped “{}”.", s.title))
    }

    /// `codex resume -i <image>… -- <id> <prompt>`; its other files go as paths, and `--` keeps a
    /// prompt that starts with a dash from reading as an option. On the model and effort asked
    /// for, as a start is (`tuned`), and without a prompt it opens on the thread and waits for one.
    fn resume(&self, account: &AccountCfg, s: &Session, ask: &crate::harness::StartAsk) -> anyhow::Result<(String, Vec<(String, String)>)> {
        let (prompt, images) = crate::attachments::take_images_in(ask.prompt, ask.images_dir, ask.files_dir, crate::attachments::Style::Path);
        let prompt = prompt.trim();
        let mut input = format!("{} resume{}", terminal_ui(account), tuned(ask));
        for p in &images {
            input.push_str(&format!(" -i {}", crate::util::sh_quote(&p.to_string_lossy())));
        }
        if prompt.is_empty() {
            return Ok((format!("{input} -- {}\n", s.id), Vec::new()));
        }
        let input = format!("{input} -- {} \"${}\"\n", s.id, crate::start::PROMPT_VAR);
        Ok((input, vec![(crate::start::PROMPT_VAR.to_string(), prompt.to_string())]))
    }

    /// Its resume command without the update offer, as every terminal UI Codealong opens
    /// (`terminal_ui`); the one its header shows and its menu copies is plain, for Codex you run
    /// yourself.
    fn resume_typed(&self, account: &AccountCfg, s: &Session) -> Option<String> {
        s.resume_command.as_ref().map(|_| format!("{} resume {}", terminal_ui(account), s.id))
    }

    /// A terminal (Background one behind your windows), the Codex app, which reads the same
    /// threads for the account it runs, and Codealong (codex-acp loads a thread by its ID).
    fn resumes_in(&self, place: crate::start::Place) -> bool {
        use crate::start::Place;
        matches!(place, Place::Terminal | Place::Background | Place::CodexApp | Place::Codealong)
    }

    fn app_way(&self) -> Option<crate::send::Route> {
        Some(ipc::InApp.into())
    }

    fn app(&self) -> &'static str {
        "the Codex app"
    }

    /// A terminal's thread opens in its tab, else `codex resume` opens a new one; the Codex app
    /// reads the same threads, for the account it runs.
    fn targets(&self, s: &Session, o: &crate::harness::Opens) -> Vec<crate::model::Target> {
        let (term, who) = (o.term, o.who.clone());
        let mut t = Vec::new();
        match s.surface {
            crate::model::Surface::Cli => {
                t.push(o.terminal(s, who.clone(), false));
                // A terminal UI holds its thread's writer lock (`thread-writer-locks/<id>.lock`)
                // until it quits, so one that runs moves to the app.
                if o.first && s.app_account {
                    t.push(match s.pid {
                        Some(_) => o.to_app(s, "Codex"),
                        None => o.way(s, "codex", "app", "Open in Codex".into(), who, false, false),
                    });
                }
            }
            crate::model::Surface::App => {
                let verb = if o.respond { "Respond in" } else { "Open in" };
                t.push(o.way(s, "codex", "app", format!("{verb} Codex"), who.clone(), true, false));
                if o.first {
                    let (why, busy) = if s.live { ("Working in the Codex app".to_string(), true) } else { (who, false) };
                    t.push(o.way(s, "resume", "terminal", format!("Resume in {term}"), why, false, busy));
                }
            }
        }
        t
    }

    fn open(&self, s: &Session, action: &str) -> anyhow::Result<()> {
        match action {
            "codex" => crate::launch::open_codex(&crate::launch::codex_thread_url(&s.id)),
            other => anyhow::bail!("Unknown action {other:?}"),
        }
    }

    fn app_bundle(&self, action: &str) -> Option<&'static str> {
        matches!(action, "codex" | "to-app").then_some(APP_BUNDLE_ID)
    }

    fn search_files(&self, account: &AccountCfg, since: i64) -> Vec<(PathBuf, String, i64)> {
        let files = rollout_files(&account.home_path(), since, &HashSet::new());
        files.into_iter().filter_map(|(p, m)| thread_id_of(&p).map(|id| (p, Session::key_for(Harness::Codex, &account.name, &id), m))).collect()
    }

    fn records_tokens(&self) -> bool {
        true
    }

    fn token_files(&self, account: &AccountCfg) -> Vec<(PathBuf, String)> {
        rollout::all_rollouts(&account.home_path()).into_iter().filter_map(|p| thread_id_of(&p).map(|id| (p, Session::key_for(Harness::Codex, &account.name, &id)))).collect()
    }

    fn token_use(&self, line: &[u8], state: &mut crate::ledger::TokenState) -> Option<crate::ledger::TokenUse> {
        rollout::token_use(line, state)
    }

    fn prose(&self, line: &[u8]) -> Option<(i64, bool, String)> {
        let has = |needle: &[u8]| memchr::memmem::find(line, needle).is_some();
        if !has(b"agent_message") && !has(b"user_message") && !has(b"AgentMessage") && !has(b"UserMessage") {
            return None;
        }
        let v: Value = serde_json::from_slice(line).ok()?;
        let at = v.get("timestamp").and_then(Value::as_str).and_then(parse_rfc3339_ms).unwrap_or(0);
        prose_of(&v).map(|(you, t)| (at, you, t))
    }

    // ---- Starting a thread ----

    /// Trusts the folder for the account (`trust::trust_folder`): in a folder it has no answer
    /// for, its terminal UI asks first whether to trust it, and a repo it hasn't been run in, or
    /// any of Codealong's chats (each a folder of its own, in no repo), would stop there. You
    /// chose the folder, as answering would. A start goes ahead without it, and Codex asks.
    fn before_start(&self, account: &AccountCfg, folder: &Path) {
        if let Err(e) = trust::trust_folder(&account.home_path().join("config.toml"), folder, &crate::util::home()) {
            eprintln!("Couldn't trust {} for {}: {e:#}", crate::util::tilde(folder), account.name);
        }
    }

    /// The models its own `/model` picker offers the account (models.rs).
    fn models(&self, account: &AccountCfg) -> Vec<crate::start::ModelOption> {
        models::listed(&account.home_path())
    }

    fn takes_model(&self) -> bool {
        true
    }

    /// `-m` takes the models its picker hides too.
    fn offers_model(&self, account: &AccountCfg, model: &str) -> bool {
        models::known(&account.home_path(), model)
    }

    fn efforts(&self) -> &'static [(&'static str, &'static str)] {
        &models::EFFORTS
    }

    /// The nearest level the model takes, by the account's list (`models::nearest_effort`).
    fn effort_for<'a>(&self, account: &AccountCfg, model: Option<&str>, effort: &'a str) -> &'a str {
        let home = account.home_path();
        match model.map(str::to_string).or_else(|| models::default_model(&home)) {
            Some(m) => models::nearest_effort(&home, &m, effort),
            None => effort,
        }
    }

    fn default_model(&self, account: &AccountCfg) -> Option<String> {
        models::default_model(&account.home_path())
    }

    /// Codex picks its own thread id. Its images go with `-i`, which takes several, so `--`
    /// ends them before the prompt; other files are paths it can read.
    fn start_in_terminal(&self, account: &AccountCfg, ask: &crate::harness::StartAsk) -> crate::harness::Typed {
        use crate::attachments::{self, Style};
        let (prompt, images) = attachments::take_images_in(ask.prompt, ask.images_dir, ask.files_dir, Style::Path);
        let prompt = prompt.trim();
        let mut input = format!("{}{}", terminal_ui(account), tuned(ask));
        let mut env = Vec::new();
        for img in &images {
            input.push_str(&format!(" -i {}", crate::util::sh_quote(&img.to_string_lossy())));
        }
        if !prompt.is_empty() {
            input.push_str(&format!(" -- \"${}\"", crate::start::PROMPT_VAR));
            env.push((crate::start::PROMPT_VAR.to_string(), prompt.to_string()));
        }
        input.push('\n');
        crate::harness::Typed { input, env, key: None }
    }

    /// `codex://threads/new` opens the app's new-thread page with the folder and prompt filled
    /// in; a folder the app has no project for becomes one.
    fn app_link(&self, folder: &Path, ask: &crate::harness::StartAsk) -> Option<String> {
        let prompt = crate::attachments::expand_in(ask.prompt, ask.images_dir, ask.files_dir, crate::attachments::Style::Path);
        Some(crate::start::with_param(format!("codex://threads/new?path={}", urlencoding::encode(&folder.to_string_lossy())), "prompt", prompt.trim()))
    }

    /// The app's new-thread link only fills a prompt in, so on a prompt Codealong makes the
    /// thread itself, on the model asked for, and the app, once it has the thread open, runs the
    /// first turn as if you'd sent it there, at the effort asked for. When that fails the link
    /// still fills the prompt in: in the thread just made if there is one, else on the
    /// new-thread page, where the app's own model is picked, as it is for a start with no prompt.
    fn start_in_app(&self, account: &AccountCfg, lane: &str, folder: &Path, ask: &crate::harness::StartAsk, link: &str) -> anyhow::Result<crate::start::Started> {
        use crate::start::Started;
        // What the app picks for itself, said where something else was asked for: the new-thread
        // page's link carries neither.
        let unasked = || {
            let what = match (ask.model.map(|m| crate::start::model_label(self, account, m)), ask.effort) {
                (Some(m), Some(e)) => format!("{m} at {e} effort"),
                (Some(m), None) => m,
                (None, Some(e)) => format!("{e} effort"),
                (None, None) => return String::new(),
            };
            format!(" Pick {what} there: the app chooses for a thread it starts.")
        };
        if ask.prompt.trim().is_empty() {
            crate::launch::open_codex(link)?;
            return Ok(Started { key: None, message: Some(format!("A new Codex thread is open in {lane}.{}", unasked())) });
        }
        let waiting = |then: String| Some(format!("Your prompt is waiting in a new Codex thread. Send it from there.{then}"));
        let home = account.home_path();
        let text = crate::attachments::expand(ask.prompt.trim(), crate::attachments::Style::Path);
        let thread = match appserver::new_thread(&home, folder, ask.model) {
            Ok(t) => t,
            Err(_) => {
                crate::launch::open_codex(link)?;
                return Ok(Started { key: None, message: waiting(unasked()) });
            }
        };
        let key = Some(Session::key_for(Harness::Codex, &account.name, &thread.id));
        // The model the thread was made on (the one asked for, else the account's), which its
        // first turn names, or the window sends an empty one.
        let model = thread.model.as_deref().map(|m| (m, ask.effort.or(thread.effort.as_deref())));
        match codex_start_turn(&home, &thread.id, &text, model) {
            Ok(()) => Ok(Started { key, message: None }),
            Err(_) => {
                crate::launch::open_codex(&format!("{}?prompt={}", crate::launch::codex_thread_url(&thread.id), urlencoding::encode(&text)))?;
                // The thread was made on the model; the effort goes with a turn, which the app's
                // box sends at its own.
                let effort = ask.effort.map(|e| format!(" Set its effort to {e} there first.")).unwrap_or_default();
                Ok(Started { key, message: waiting(effort) })
            }
        }
    }

    /// Codealong makes the thread a start in the app runs (`start_in_app`), on the model asked
    /// for, and starts its first turn at the effort asked for.
    fn app_takes_model(&self) -> bool {
        true
    }

    fn app_projects(&self, account: &AccountCfg) -> Vec<String> {
        app_project_roots(&account.home_path())
    }

    /// A thread the app starts outside a project runs in a folder it makes for it.
    fn chats(&self) -> Option<(PathBuf, &'static str)> {
        Some((chats_dir(), "Codex chats"))
    }

    /// A call's output is a line of its own, with the call's id.
    fn output_marks(&self) -> &'static [&'static str] {
        &["\"type\":\"function_call_output\"", "\"type\":\"custom_tool_call_output\""]
    }

    /// A command's `workdir` (in a call's arguments, which are JSON in a string), and the `cwd`
    /// of a `CommandExecution` or of a turn.
    fn folder_marks(&self) -> &'static [&'static str] {
        &["\"workdir\":\"", "\\\"workdir\\\":\\\"", "\"cwd\":\"", "\\\"cwd\\\":\\\""]
    }

    fn pr_needles(&self) -> &'static [&'static str] {
        &["gh pr create", "\"attach_artifact\""]
    }

    fn pr_made(&self, line: &str) -> Option<(String, i64)> {
        rollout::pr_made(line)
    }

    /// Codex archives a thread itself, so the Codex app shows it too. A thread Codealong runs is
    /// held by codex-acp's own app-server, which answers another's `thread/archive` with "thread
    /// … already has an active writer", even after the session's `session/close` (codex-acp
    /// 1.13.1, checked 2026-10-11); its `session/delete` archives the thread through that server
    /// and lets go of it.
    fn archive(&self, account: &AccountCfg, id: &str, archived: bool) -> anyhow::Result<()> {
        if archived && crate::acp::delete_held(&account.name, self.harness(), id)? {
            return Ok(());
        }
        appserver::set_archived(&account.home_path(), id, archived)
    }

    fn reader(&self, _wake: crate::harness::Wake) -> Box<dyn crate::harness::Reader> {
        Box::new(Reader::default())
    }
}

/* ---- Reading threads for the board ---- */

/// Codex's threads for the board (`harness::Reader`): each account's thread names, the rollouts
/// read so far, and the terminals that show threads.
#[derive(Default)]
pub struct Reader {
    /// Thread names by account (`session_index.jsonl`).
    index: HashMap<String, HashMap<String, (String, i64)>>,
    /// Each rollout with its account, what it said, and how far it's been read. `None` marks a
    /// thread that's never shown (a guardian review, a subagent), so its constant writes are
    /// ignored.
    rollouts: HashMap<PathBuf, (String, Option<(ThreadInfo, u64)>)>,
    /// Codex's terminal UIs: which threads are open in a terminal, and which one.
    terminals: Vec<Terminal>,
    /// When Codex's processes were last looked at, and the rollouts of open turns none of them
    /// held then, by file name (`let_go_now`); None when that couldn't be told.
    let_go: Option<(i64, HashSet<OsString>)>,
    /// How long each open turn's rollout was at that look.
    open_lens: HashMap<PathBuf, u64>,
}

impl Reader {
    /// The rollouts of turns left open that no Codex process holds now (`held_rollouts`), as of
    /// this look, and keeps how long each open turn's rollout is for the next. Codex's processes
    /// are looked at only while some turn is open.
    fn let_go_now(&mut self, look: &Look) -> Option<(i64, HashSet<OsString>)> {
        // Each as long as it was at the last look, else as far as it's been read.
        let open: Vec<(&Path, u64)> = self
            .rollouts
            .iter()
            .filter_map(|(path, (_, parsed))| parsed.as_ref().filter(|(th, _)| th.turn_open).map(|(_, read)| (path.as_path(), self.open_lens.get(path).copied().unwrap_or(*read))))
            .collect();
        let len_now = |p: &Path| std::fs::metadata(p).ok().map(|m| m.len());
        let lens: HashMap<PathBuf, u64> = open.iter().filter_map(|(p, _)| Some((p.to_path_buf(), len_now(p)?))).collect();
        let gone = if open.is_empty() { Some(HashSet::new()) } else { held_rollouts(look.ps.all()).map(|held| let_go(&open, &held, |p| lens.get(p).copied())) };
        self.open_lens = lens;
        Some((look.now, gone?))
    }

    /// Whether a Codex process held a thread's rollout when last looked, None when that isn't
    /// known. One written since the look is taken as held: something is running it.
    fn held(&self, path: &Path, th: &ThreadInfo) -> Option<bool> {
        let (at, gone) = self.let_go.as_ref()?;
        Some(th.last_ts > *at || !path.file_name().is_some_and(|n| gone.contains(n)))
    }

    /// Loads the threads archived from Codealong from `archived_sessions`, so its Archived list
    /// can show them, and drops ones that were unarchived or are no longer listed. One unarchived
    /// is read where Codex put it back, under `sessions`, at once: the watcher may not have said
    /// so yet, and a thread quiet since before the look isn't read there by itself
    /// (`rollout_files`), so it would leave the board, and a message sent as it's unarchived
    /// (actions.rs `unarchive_to_send`) wouldn't find it.
    fn refresh_archived(&mut self, accounts: &[AccountCfg], look: &Look) {
        for a in accounts {
            let home = a.home_path();
            let dir = home.join("archived_sessions");
            let ids = look.seen.archived_ids("codex", &a.name);
            let back: HashSet<String> = self.rollouts.iter().filter(|(path, (acct, _))| acct == &a.name && path.starts_with(&dir)).filter_map(|(path, _)| thread_id_of(path)).filter(|id| !ids.contains(id)).collect();
            self.rollouts.retain(|path, (acct, _)| acct != &a.name || !path.starts_with(&dir) || (path.exists() && thread_id_of(path).map(|id| ids.contains(&id)).unwrap_or(false)));
            let back = if back.is_empty() { Vec::new() } else { rollout_files(&home, i64::MAX, &back) };
            for (path, m) in archived_rollout_files(&home, &ids).into_iter().chain(back) {
                if !self.rollouts.contains_key(&path) {
                    let parsed = parse_rollout(&path, m, None);
                    self.rollouts.insert(path, (a.name.clone(), parsed));
                }
            }
        }
    }

    /// FSEvents can hold back appends until a writer closes its file. Codex keeps rollouts open
    /// across turns, so their lengths are checked too, and only bytes not yet read are read.
    fn poll_rollouts(&mut self, accounts: &[AccountCfg], look: &Look) -> Applied {
        let mut out = Applied::default();
        for a in accounts {
            let home = a.home_path();
            let sessions = home.join("sessions");
            let mut paths: BTreeSet<PathBuf> = rollout_files(&home, look.since, &look.seen.pinned_ids("codex", &a.name)).into_iter().map(|(p, _)| p).collect();
            // Include known files so a removal or truncation is noticed as well.
            paths.extend(self.rollouts.iter().filter(|(p, (acct, _))| acct == &a.name && p.starts_with(&sessions)).map(|(p, _)| p.clone()));
            for p in paths {
                let unread = match self.rollouts.get(&p) {
                    Some((_, Some((_, offset)))) => std::fs::metadata(&p).map(|m| m.len() != *offset).unwrap_or(true),
                    // Guardian reviews and subagents still don't need to be parsed.
                    Some((_, None)) => false,
                    None => true,
                };
                if unread {
                    self.read_rollout(&a.name, &p, &mut out);
                }
            }
            let index = read_index(&home);
            if self.index.get(&a.name) != Some(&index) {
                self.index.insert(a.name.clone(), index);
                out.dirty = true;
            }
        }
        out
    }

    /// Reads on in a rollout that changed, or for the first time in a new one.
    fn read_rollout(&mut self, account: &str, p: &Path, out: &mut Applied) {
        let prev = match self.rollouts.get(p) {
            // A thread that's never shown: skip its writes.
            Some((_, None)) => return,
            Some((_, Some(parsed))) => Some(parsed.clone()),
            None => None,
        };
        let is_new = prev.is_none();
        let from = prev.as_ref().map(|(_, off)| *off);
        match mtime_ms(p) {
            Some(m) => {
                let parsed = parse_rollout(p, m, prev);
                out.added |= is_new && parsed.is_some();
                if let (Some(from), Some((th, to))) = (from, parsed.as_ref()) {
                    out.lines.push(NewLines { path: p.to_path_buf(), from, to: *to, key: Session::key_for(Harness::Codex, account, &th.id) });
                }
                self.rollouts.insert(p.to_path_buf(), (account.to_string(), parsed));
            }
            None => {
                self.rollouts.remove(p);
            }
        }
        out.dirty = true;
    }
}

impl crate::harness::Reader for Reader {
    fn harness(&self) -> Harness {
        Harness::Codex
    }

    fn scan(&mut self, accounts: &[AccountCfg], look: &Look) {
        self.index.clear();
        self.rollouts.clear();
        for a in accounts {
            let home = a.home_path();
            self.index.insert(a.name.clone(), read_index(&home));
            let files = rollout_files(&home, look.since, &look.seen.pinned_ids("codex", &a.name));
            for (path, parsed) in parallel_map(files, |(p, m)| {
                let parsed = parse_rollout(&p, m, None);
                Some((p, parsed))
            }) {
                self.rollouts.insert(path, (a.name.clone(), parsed));
            }
        }
        if !accounts.is_empty() {
            self.terminals = terminals(look.ps.all());
            self.let_go = self.let_go_now(look);
        }
        self.refresh_archived(accounts, look);
    }

    fn watch(&self, account: &AccountCfg) -> Vec<(PathBuf, bool)> {
        let home = account.home_path();
        vec![(home.clone(), false), (home.join("sessions"), true)]
    }

    fn owns(&self, account: &AccountCfg, path: &Path) -> bool {
        path.starts_with(account.home_path())
    }

    fn apply(&mut self, account: &AccountCfg, paths: &[PathBuf], _look: &Look) -> Applied {
        let home = account.home_path();
        let (index, sessions) = (home.join("session_index.jsonl"), home.join("sessions"));
        let mut out = Applied::default();
        if paths.contains(&index) {
            self.index.insert(account.name.clone(), read_index(&home));
            out.dirty = true;
        }
        for p in paths.iter().filter(|p| p.starts_with(&sessions) && p.extension().and_then(|x| x.to_str()) == Some("jsonl")) {
            self.read_rollout(&account.name, p, &mut out);
        }
        out
    }

    fn tick(&mut self, accounts: &[AccountCfg], look: &Look) -> Applied {
        let mut out = self.poll_rollouts(accounts, look);
        if !accounts.is_empty() {
            let fresh = terminals(look.ps.all());
            out.dirty |= fresh != self.terminals;
            self.terminals = fresh;
            let let_go = self.let_go_now(look);
            out.dirty |= let_go.as_ref().map(|(_, g)| g) != self.let_go.as_ref().map(|(_, g)| g);
            self.let_go = let_go;
        }
        out
    }

    fn archives_changed(&mut self, accounts: &[AccountCfg], look: &Look) {
        self.refresh_archived(accounts, look);
    }

    fn sessions(&self, account: &AccountCfg, look: &Look) -> Vec<Session> {
        let index = self.index.get(&account.name);
        // Codex decides what's archived: its archived threads live in `archived_sessions`.
        let archived_dir = account.home_path().join("archived_sessions");
        let mine = || self.rollouts.iter().filter_map(|(path, (acct, info))| info.as_ref().filter(|_| acct == &account.name).map(|(th, _)| (path, th)));
        let threads: Vec<(&str, &str, i64, i64, bool)> = mine().map(|(_, th)| (th.id.as_str(), th.cwd.as_str(), th.started_at.unwrap_or(0), th.last_ts, th.cli)).collect();
        let terminals = pair_terminals(&self.terminals, &threads);
        mine()
            .map(|(path, th)| {
                let key = Session::key_for(Harness::Codex, &account.name, &th.id);
                let name = index.and_then(|i| i.get(&th.id)).map(|(n, _)| n.as_str());
                let mut s = crate::board::codex_session(account, th, name, terminals.get(&th.id).copied(), self.held(path, th), look.seen.seen_at(&key), look.now);
                s.archived = path.starts_with(&archived_dir);
                s
            })
            .collect()
    }

    fn cwds(&self) -> Vec<String> {
        self.rollouts.values().filter_map(|(_, t)| t.as_ref().map(|(t, _)| t.cwd.clone())).collect()
    }

    fn logs(&self) -> Vec<(PathBuf, String)> {
        self.rollouts.iter().filter_map(|(p, (a, parsed))| parsed.as_ref().map(|(th, _)| (p.clone(), Session::key_for(Harness::Codex, a, &th.id)))).collect()
    }

    /// Codex's limits come with each turn, in the rollouts read anyway.
    fn usage(&self, account: &AccountCfg, now: i64) -> Option<Usage> {
        let reports = self.rollouts.values().filter(|(acct, _)| acct == &account.name).filter_map(|(_, p)| p.as_ref().map(|(th, _)| &th.rate_limits));
        Some(usage::codex(account, reports, now))
    }
}

/// The rollouts of open turns, of `open` (each with how long it was when last looked at, else as
/// far as it's been read), that no Codex process holds (`held`), by file name. One that has grown
/// since is left out: a turn that ends writes `task_complete` and may let go of its rollout
/// between the read and the look at the processes, and it's read again next. A writer killed
/// partway through a line leaves a rollout longer than the lines read for good, which the next
/// look finds the same length.
fn let_go(open: &[(&Path, u64)], held: &HashSet<OsString>, len_now: impl Fn(&Path) -> Option<u64>) -> HashSet<OsString> {
    open.iter()
        .filter(|(path, read)| len_now(path) == Some(*read))
        .filter_map(|(path, _)| path.file_name())
        .filter(|n| !held.contains(*n))
        .map(OsString::from)
        .collect()
}

fn fork_of(account: &crate::config::AccountCfg, id: &str) -> crate::harness::Typed {
    crate::harness::Typed { input: format!("{} fork {id}\n", terminal_ui(account)), env: Vec::new(), key: None }
}

/// The account's command as Codealong types it to open Codex's terminal UI (a start, a resume, a
/// fork), without the update offer the UI can stop on before it makes its session: in a terminal
/// behind your windows nobody answers it, and a review sat 66 minutes on a screen of its own
/// (2026-10-14). Codex you run yourself still offers updates. `-c` goes before a subcommand
/// (0.162.1); a key Codex doesn't know is ignored, so an older one starts as before.
fn terminal_ui(account: &AccountCfg) -> String {
    format!("{} -c check_for_update_on_startup=false", account.command())
}

/// The model and effort a start or a resume asks for, as its terminal UI takes them: `-m`, and
/// the effort as config.toml's `model_reasoning_effort`, a TOML string. Both are names start.rs
/// let through (`valid_model`, `EFFORTS`), so nothing in them leaves the quotes.
fn tuned(ask: &crate::harness::StartAsk) -> String {
    let mut out = String::new();
    if let Some(m) = ask.model.filter(|m| crate::start::valid_model(m)) {
        out.push_str(&format!(" -m '{m}'"));
    }
    if let Some(e) = ask.effort.filter(|e| models::EFFORTS.iter().any(|(id, _)| id == e)) {
        out.push_str(&format!(" -c 'model_reasoning_effort=\"{e}\"'"));
    }
    out
}

fn record(a: &Codex, ctx: &crate::harness::Ctx, s: &crate::model::Session) -> anyhow::Result<PathBuf> {
    use crate::harness::Adapter;
    a.records(ctx.account, s).into_iter().next().ok_or_else(|| anyhow::anyhow!("Its rollout isn't on this Mac any more."))
}

fn walk_files(dir: &Path, depth: usize, visit: &mut dyn FnMut(&Path)) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            if depth > 0 {
                walk_files(&p, depth - 1, visit);
            }
        } else {
            visit(&p);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codex::terminals::terminal_args;

    #[test]
    fn an_open_turn_no_process_holds_is_let_go_unless_its_rollout_grew_or_ps_failed() {
        let (a, b, c) = (Path::new("/c/sessions/rollout-a.jsonl"), Path::new("/c/sessions/rollout-b.jsonl"), Path::new("/c/sessions/rollout-c.jsonl"));
        let held: HashSet<OsString> = [OsString::from("rollout-b.jsonl")].into();
        // c wrote its end since it was read.
        let lens = |p: &Path| Some(if p == c { 350 } else if p == a { 100 } else { 200 });
        assert_eq!(let_go(&[(a, 100), (b, 200), (c, 300)], &held, lens), [OsString::from("rollout-a.jsonl")].into());
        // Gone since it was read: nothing to say.
        assert!(let_go(&[(a, 100)], &held, |_| None).is_empty());
        // A half-written last line that hasn't grown since the last look (a writer killed partway).
        assert_eq!(let_go(&[(c, 350)], &held, lens), [OsString::from("rollout-c.jsonl")].into());
        // A ps that failed lists nothing, which says nothing about who holds what.
        assert_eq!(held_rollouts(&[]), None);
    }

    #[test]
    fn forks_a_thread_in_its_accounts_terminal_ui() {
        let a = crate::config::AccountCfg { name: "codex2".into(), harness: "codex".into(), home: "~/.codex2".into(), command: Some("CODEX_HOME=~/.codex2 codex".into()), start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false };
        let f = fork_of(&a, "01a0e375-1c2d-7e3f-8a9b-0c1d2e3f4a5b");
        assert_eq!(f.input, "CODEX_HOME=~/.codex2 codex -c check_for_update_on_startup=false fork 01a0e375-1c2d-7e3f-8a9b-0c1d2e3f4a5b\n");
        assert_eq!(f.key, None);
        // The terminal it opens is paired with its folder's newest thread, not the one it forked.
        assert_eq!(terminal_args("codex -c check_for_update_on_startup=false fork 01a0e375-1c2d-7e3f-8a9b-0c1d2e3f4a5b"), Some((None, None)));
    }

    /// A Codex review started in a terminal behind your windows sat 66 minutes on a screen of its
    /// own before it made its session (2026-10-14).
    #[test]
    fn its_terminal_ui_opens_without_the_update_offer_and_is_still_known() {
        let a = crate::config::AccountCfg { name: "codex".into(), harness: "codex".into(), home: "~/.codex".into(), command: Some("codex".into()), start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false };
        use crate::harness::Adapter;
        let ask = crate::harness::StartAsk { prompt: "Review the branch", session_id: "unused", model: None, effort: None, worktree: false, images_dir: Path::new("/i"), files_dir: Path::new("/f") };
        let start = Codex.start_in_terminal(&a, &ask);
        assert_eq!(start.input, "codex -c check_for_update_on_startup=false -- \"$CODEALONG_PROMPT\"\n");
        let s = Session::new(Harness::Codex, "codex", "01a0e375-1c2d-7e3f-8a9b-0c1d2e3f4a5b");
        let (resume, _) = Codex.resume(&a, &s, &ask).unwrap();
        assert_eq!(resume, "codex -c check_for_update_on_startup=false resume -- 01a0e375-1c2d-7e3f-8a9b-0c1d2e3f4a5b \"$CODEALONG_PROMPT\"\n");
        // A click on one that isn't running types it so too; the command shown and copied for you
        // to run yourself stays plain.
        let s = Session { resume_command: Some("codex resume 01a0e375-1c2d-7e3f-8a9b-0c1d2e3f4a5b".into()), ..s };
        assert_eq!(Codex.resume_typed(&a, &s).as_deref(), Some("codex -c check_for_update_on_startup=false resume 01a0e375-1c2d-7e3f-8a9b-0c1d2e3f4a5b"));
        assert_eq!(Codex.resume_typed(&a, &Session { resume_command: None, ..s.clone() }), None);
        let other = Session { resume_command: Some("claude --resume x".into()), ..Session::new(Harness::Claude, "claude", "x") };
        assert_eq!(crate::harness::adapter(Harness::Claude).resume_typed(&a, &other).as_deref(), Some("claude --resume x"));
        // Its process is still Codex's terminal UI, on the thread it resumed.
        assert_eq!(terminal_args("codex -c check_for_update_on_startup=false Review the branch"), Some((None, None)));
        assert_eq!(terminal_args("codex -c check_for_update_on_startup=false resume 01a0e375-1c2d-7e3f-8a9b-0c1d2e3f4a5b"), Some((Some("01a0e375-1c2d-7e3f-8a9b-0c1d2e3f4a5b".into()), None)));
    }

    /// `-m` and the effort as config.toml's setting, which its terminal UI takes after `resume`
    /// too (0.162.1's `codex resume --help`).
    #[test]
    fn starts_and_resumes_on_the_model_and_effort_asked_for() {
        let a = crate::config::AccountCfg { name: "codex".into(), harness: "codex".into(), home: "~/.codex".into(), command: Some("codex".into()), start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false };
        use crate::harness::Adapter;
        let ask = crate::harness::StartAsk { prompt: "Review the branch", session_id: "unused", model: Some("gpt-6-sol"), effort: Some("xhigh"), worktree: false, images_dir: Path::new("/i"), files_dir: Path::new("/f") };
        let start = Codex.start_in_terminal(&a, &ask);
        assert_eq!(start.input, "codex -c check_for_update_on_startup=false -m 'gpt-6-sol' -c 'model_reasoning_effort=\"xhigh\"' -- \"$CODEALONG_PROMPT\"\n");
        let s = Session::new(Harness::Codex, "codex", "01a0e375-1c2d-7e3f-8a9b-0c1d2e3f4a5b");
        let (resume, _) = Codex.resume(&a, &s, &crate::harness::StartAsk { prompt: "", ..ask }).unwrap();
        assert_eq!(resume, "codex -c check_for_update_on_startup=false resume -m 'gpt-6-sol' -c 'model_reasoning_effort=\"xhigh\"' -- 01a0e375-1c2d-7e3f-8a9b-0c1d2e3f4a5b\n");
        // Its process, as `ps` shows it without the quotes, is still Codex's terminal UI, on the
        // thread it resumed.
        assert_eq!(terminal_args("codex -c check_for_update_on_startup=false -m gpt-6-sol -c model_reasoning_effort=\"xhigh\" Review the branch"), Some((None, None)));
        assert_eq!(terminal_args("codex -c check_for_update_on_startup=false resume -m gpt-6-sol -c model_reasoning_effort=\"xhigh\" -- 01a0e375-1c2d-7e3f-8a9b-0c1d2e3f4a5b"), Some((Some("01a0e375-1c2d-7e3f-8a9b-0c1d2e3f4a5b".into()), None)));
        // Nothing that could leave the quotes is typed.
        let odd = crate::harness::StartAsk { model: Some("x'; rm -rf ~; '"), effort: Some("high\"; x"), ..ask };
        assert_eq!(Codex.start_in_terminal(&a, &odd).input, "codex -c check_for_update_on_startup=false -- \"$CODEALONG_PROMPT\"\n");
    }

    #[test]
    fn polls_turns_while_the_rollout_stays_open() {
        use crate::harness::Reader as _;
        use crate::model::Status;
        use std::io::Write;

        let home = crate::fixtures::scratch("codex-poll");
        let dir = home.join("sessions/2026/09/24");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("rollout-2026-09-24T18-00-00-01a0d54c-7f02-7623-95b3-a3110c0f9672.jsonl");
        let mut file = std::fs::File::create(&path).unwrap();
        writeln!(file, r#"{{"type":"session_meta","payload":{{"id":"thread","cwd":"/codealong-test-missing-repo","thread_source":"user"}}}}"#).unwrap();
        writeln!(file, r#"{{"timestamp":"2026-09-24T18:00:00.000Z","type":"event_msg","payload":{{"type":"task_complete","last_agent_message":"Earlier result"}}}}"#).unwrap();
        let account = AccountCfg { name: "studio".into(), harness: "codex".into(), home: home.to_string_lossy().into_owned(), command: None, start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false };
        let accounts = std::slice::from_ref(&account);
        let seen = crate::state::SeenState::default();
        let now = parse_rfc3339_ms("2026-09-24T18:35:00.000Z").unwrap();
        let look = Look { seen: &seen, since: now - 14 * crate::util::DAY_MS, now, ps: Default::default() };
        // A startup scan has the previous completed turn. No filesystem events follow.
        let mut reader = Reader::default();
        reader.rollouts.insert(path.clone(), ("studio".into(), parse_rollout(&path, 0, None)));
        reader.index.insert("studio".into(), HashMap::new());
        assert_eq!(reader.sessions(&account, &look)[0].status, Status::Unread);

        writeln!(file, r#"{{"timestamp":"2026-09-24T18:34:00.000Z","type":"event_msg","payload":{{"type":"task_started"}}}}"#).unwrap();
        writeln!(file, r#"{{"timestamp":"2026-09-24T18:34:01.000Z","type":"event_msg","payload":{{"type":"user_message","message":"Continue the work"}}}}"#).unwrap();
        let read = reader.poll_rollouts(accounts, &look);
        let session = &reader.sessions(&account, &look)[0];
        assert_eq!(session.status, Status::Working);
        assert!(session.live);
        assert_eq!(session.excerpt.as_deref(), Some("Continue the work"));
        // The board is published again, and the engine gets the new lines to check.
        assert!(read.dirty && !read.added);
        assert_eq!(read.lines.iter().map(|l| l.key.as_str()).collect::<Vec<_>>(), ["codex:studio:thread"]);

        assert!(!reader.poll_rollouts(accounts, &look).dirty, "unchanged files don't need another update");

        // A poll between writes must leave the incomplete line for the next one.
        write!(file, r#"{{"timestamp":"2026-09-24T18:34:59.000Z","type":"event_msg","payload":{{"type":"task_complete","last_agent_message":"Done"}}}}"#).unwrap();
        reader.poll_rollouts(accounts, &look);
        assert_eq!(reader.sessions(&account, &look)[0].status, Status::Working);
        writeln!(file).unwrap();
        reader.poll_rollouts(accounts, &look);
        let session = &reader.sessions(&account, &look)[0];
        assert_eq!(session.status, Status::Unread);
        assert!(!session.live);
        assert_eq!(session.excerpt.as_deref(), Some("Done"));

        drop(file);
        std::fs::remove_file(&path).unwrap();
        reader.poll_rollouts(accounts, &look);
        assert!(reader.sessions(&account, &look).is_empty());

        // Discovery also works without a create event, including files from older day folders.
        std::fs::write(&path, "{\"type\":\"session_meta\",\"payload\":{\"id\":\"new-thread\",\"cwd\":\"/codealong-test-missing-repo\"}}\n").unwrap();
        std::fs::write(home.join("session_index.jsonl"), "{\"id\":\"new-thread\",\"thread_name\":\"New work\"}\n").unwrap();
        assert!(reader.poll_rollouts(accounts, &look).added, "a thread that's new may need a lane");
        assert_eq!(reader.sessions(&account, &look)[0].title, "New work");

        // Hidden subagent logs stay ignored even if they grow.
        reader.rollouts.insert(path.clone(), ("studio".into(), None));
        let mut hidden = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
        writeln!(hidden, "not a user thread").unwrap();
        assert!(!reader.poll_rollouts(accounts, &look).dirty);
        assert!(reader.sessions(&account, &look).is_empty());
        drop(hidden);
    }

    /// A thread unarchived from Codealong stays on the board as Codex moves it back under
    /// `sessions`, though it's quiet since before the look and nothing on disk said so yet: a
    /// message sent as it's unarchived goes by the board.
    #[test]
    fn a_thread_taken_out_of_the_archive_stays_on_the_board() {
        use crate::harness::Reader as _;
        let home = crate::fixtures::scratch("codex-unarchive");
        let id = "01a0d54c-7f02-7623-95b3-a3110c0f9672";
        let name = format!("rollout-2026-09-01T18-00-00-{id}.jsonl");
        let archived = home.join("archived_sessions");
        std::fs::create_dir_all(&archived).unwrap();
        std::fs::write(archived.join(&name), format!("{{\"type\":\"session_meta\",\"payload\":{{\"id\":\"{id}\",\"cwd\":\"/codealong-test-missing-repo\",\"thread_source\":\"user\"}}}}\n")).unwrap();
        let account = AccountCfg { name: "studio".into(), harness: "codex".into(), home: home.to_string_lossy().into_owned(), command: None, start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false };
        let accounts = std::slice::from_ref(&account);
        let key = Session::key_for(Harness::Codex, "studio", id);
        let now = crate::util::now_ms();
        let shelved = crate::state::SeenState { archived: vec![crate::state::ArchivedSession { key: key.clone(), at: now }], ..Default::default() };
        // Every file here is older than the look reads by itself.
        let look = Look { seen: &shelved, since: now + crate::util::DAY_MS, now, ps: Default::default() };
        let mut reader = Reader::default();
        reader.archives_changed(accounts, &look);
        let listed = |r: &Reader, look: &Look| r.sessions(&account, look).iter().map(|s| (s.key.clone(), s.archived)).collect::<Vec<_>>();
        assert_eq!(listed(&reader, &look), [(key.clone(), true)]);

        let back = home.join("sessions/2026/09/01");
        std::fs::create_dir_all(&back).unwrap();
        std::fs::rename(archived.join(&name), back.join(&name)).unwrap();
        let seen = crate::state::SeenState::default();
        let look = Look { seen: &seen, ..look };
        reader.archives_changed(accounts, &look);
        assert_eq!(listed(&reader, &look), [(key, false)]);
    }

    #[test]
    fn a_prompt_for_an_app_thread_goes_to_the_app() {
        use crate::harness::Adapter as _;
        let account = AccountCfg { name: "codex".into(), harness: "codex".into(), home: "/h".into(), command: None, start: None, acp: None, monthly: None, per_token: false, usage_cap: None, disabled: false };
        let s = Session { surface: crate::model::Surface::App, status: crate::model::Status::Working, live: true, ..Session::new(Harness::Codex, "codex", "abc") };
        assert_eq!(Codex.routes(&account, &s), vec![ipc::InApp.into()]);
    }
}
