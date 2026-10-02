//! Codex, in its app and in its terminal UI. Everything Codealong knows of its files, sockets
//! and commands is in this folder: here the adapter (`Codex`), which answers for a thread and
//! for an account, and the reader, which keeps the thread names and the rollouts read for the
//! board. Beside it: rollouts and the thread index (rollout.rs), the conversation a rollout
//! holds (conversation.rs), its app-server, for what Codex does itself (appserver.rs), the
//! socket the app's windows talk on (ipc.rs), its terminal UIs (terminals.rs), side
//! conversations (side.rs) and plan usage (usage.rs). See CLAUDE.md's *Harness facts*.

mod appserver;
mod conversation;
pub mod ipc;
mod rollout;
mod side;
mod terminals;
mod usage;

use std::collections::{BTreeSet, HashMap, HashSet};
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
use terminals::{codex_cli, latest_day, pair_terminals, terminals, Terminal};

pub use conversation::{conversation_items, with_images_before};
#[cfg(test)]
pub use rollout::PendingCall;
pub use rollout::ThreadInfo;

/// The Codex app's bundle ID. The ChatGPT chat app has the same name, so the app is known by this.
pub const APP_BUNDLE_ID: &str = "com.openai.codex";

/// Codex (`harness::Adapter`): its rollout for the conversation. Approvals aren't recorded
/// anywhere Codealong can read, so they're answered in Codex.
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

    fn acp_command(&self, account: &crate::config::AccountCfg) -> std::result::Result<(String, Vec<(String, String)>), String> {
        let program = crate::acp::package_program("codex-acp", self.harness())?;
        Ok((program, vec![("CODEX_HOME".into(), account.home_path().to_string_lossy().into_owned())]))
    }

    /// The Codex app runs the account in `~/.codex`, where its threads usually start.
    fn places(&self, account: &crate::config::AccountCfg, home: &Path) -> std::result::Result<Vec<crate::start::Place>, String> {
        use crate::start::Place;
        Ok(if account.home_path() == home.join(".codex") { vec![Place::CodexApp, Place::Terminal, Place::Codealong] } else { vec![Place::Terminal, Place::Codealong] })
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

    /// `codex resume <id> <prompt>`; its files go as paths, and `--` keeps a prompt that starts
    /// with a dash from reading as an option.
    fn resume(&self, account: &AccountCfg, id: &str, text: &str) -> anyhow::Result<(String, Vec<(String, String)>)> {
        let input = format!("{} resume -- {id} \"${}\"\n", account.command(), crate::start::PROMPT_VAR);
        Ok((input, vec![(crate::start::PROMPT_VAR.to_string(), crate::attachments::expand(text, crate::attachments::Style::Path).trim().to_string())]))
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
                if o.first && s.app_account {
                    let (why, busy) = if s.pid.is_some() { (format!("Open in {term}, finish there first"), true) } else { (who, false) };
                    t.push(o.way(s, "codex", "app", "Open in Codex".into(), why, false, busy));
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
        (action == "codex").then_some(APP_BUNDLE_ID)
    }

    fn search_files(&self, account: &AccountCfg, since: i64) -> Vec<(PathBuf, String, i64)> {
        let files = rollout_files(&account.home_path(), since, &HashSet::new());
        files.into_iter().filter_map(|(p, m)| thread_id_of(&p).map(|id| (p, Session::key_for(Harness::Codex, &account.name, &id), m))).collect()
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

    /// Codex picks its own thread id. Its images go with `-i`, which takes several, so `--`
    /// ends them before the prompt; other files are paths it can read.
    fn start_in_terminal(&self, account: &AccountCfg, ask: &crate::harness::StartAsk) -> crate::harness::Typed {
        use crate::attachments::{self, Style};
        let prompt = attachments::expand_in(ask.prompt, ask.images_dir, ask.files_dir, Style::Path);
        let prompt = prompt.trim();
        let mut input = account.command().to_string();
        let mut env = Vec::new();
        for rel in attachments::referenced(ask.prompt).iter().filter(|r| r.starts_with("images/")) {
            if let Some(name) = rel.strip_prefix("images/") {
                input.push_str(&format!(" -i {}", crate::util::sh_quote(&ask.images_dir.join(name).to_string_lossy())));
            }
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
    /// thread itself, and the app, once it has the thread open, runs the first turn as if you'd
    /// sent it there. When that fails the link still fills the prompt in: in the thread just made
    /// if there is one, else on the new-thread page.
    fn start_in_app(&self, account: &AccountCfg, lane: &str, folder: &Path, prompt: &str, link: &str) -> anyhow::Result<crate::start::Started> {
        use crate::start::Started;
        if prompt.trim().is_empty() {
            crate::launch::open_codex(link)?;
            return Ok(Started { key: None, message: Some(format!("A new Codex thread is open in {lane}.")) });
        }
        let waiting = || Some("Your prompt is waiting in a new Codex thread. Send it from there.".to_string());
        let home = account.home_path();
        let text = crate::attachments::expand(prompt.trim(), crate::attachments::Style::Path);
        let thread = match appserver::new_thread(&home, folder, None) {
            Ok(t) => t,
            Err(_) => {
                crate::launch::open_codex(link)?;
                return Ok(Started { key: None, message: waiting() });
            }
        };
        let key = Some(Session::key_for(Harness::Codex, &account.name, &thread.id));
        let model = thread.model.as_deref().map(|m| (m, thread.effort.as_deref()));
        match codex_start_turn(&home, &thread.id, &text, model) {
            Ok(()) => Ok(Started { key, message: None }),
            Err(_) => {
                crate::launch::open_codex(&format!("{}?prompt={}", crate::launch::codex_thread_url(&thread.id), urlencoding::encode(&text)))?;
                Ok(Started { key, message: waiting() })
            }
        }
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

    /// Codex archives a thread itself, so the Codex app shows it too.
    fn archive(&self, account: &AccountCfg, id: &str, archived: bool) -> anyhow::Result<()> {
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
}

impl Reader {
    /// Loads the threads archived from Codealong from `archived_sessions`, so its Archived list
    /// can show them, and drops ones that were unarchived or are no longer listed.
    fn refresh_archived(&mut self, accounts: &[AccountCfg], look: &Look) {
        for a in accounts {
            let dir = a.home_path().join("archived_sessions");
            let ids = look.seen.archived_ids("codex", &a.name);
            self.rollouts.retain(|path, (acct, _)| acct != &a.name || !path.starts_with(&dir) || (path.exists() && thread_id_of(path).map(|id| ids.contains(&id)).unwrap_or(false)));
            for (path, m) in archived_rollout_files(&a.home_path(), &ids) {
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
                let mut s = crate::board::codex_session(account, th, name, terminals.get(&th.id).copied(), look.seen.seen_at(&key), look.now);
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

fn fork_of(account: &crate::config::AccountCfg, id: &str) -> crate::harness::Typed {
    crate::harness::Typed { input: format!("{} fork {id}\n", account.command()), env: Vec::new(), key: None }
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
    fn forks_a_thread_in_its_accounts_terminal_ui() {
        let a = crate::config::AccountCfg { name: "codex2".into(), harness: "codex".into(), home: "~/.codex2".into(), command: Some("CODEX_HOME=~/.codex2 codex".into()), start: None, preferred: false, acp: None };
        let f = fork_of(&a, "01a0e375-1c2d-7e3f-8a9b-0c1d2e3f4a5b");
        assert_eq!(f.input, "CODEX_HOME=~/.codex2 codex fork 01a0e375-1c2d-7e3f-8a9b-0c1d2e3f4a5b\n");
        assert_eq!(f.key, None);
        // The terminal it opens is paired with its folder's newest thread, not the one it forked.
        assert_eq!(terminal_args("codex fork 01a0e375-1c2d-7e3f-8a9b-0c1d2e3f4a5b"), Some((None, None)));
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
        let account = AccountCfg { name: "studio".into(), harness: "codex".into(), home: home.to_string_lossy().into_owned(), command: None, start: None, preferred: false, acp: None };
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

    #[test]
    fn a_prompt_for_an_app_thread_goes_to_the_app() {
        use crate::harness::Adapter as _;
        let account = AccountCfg { name: "codex".into(), harness: "codex".into(), home: "/h".into(), command: None, start: None, preferred: false, acp: None };
        let s = Session { surface: crate::model::Surface::App, status: crate::model::Status::Working, live: true, ..Session::new(Harness::Codex, "codex", "abc") };
        assert_eq!(Codex.routes(&account, &s), vec![ipc::InApp.into()]);
    }
}
