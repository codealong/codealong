//! The `codealong` command, for agents and people: todos, sessions, worktrees, accounts, and
//! which session the caller is. It asks the running app through its socket (server.rs), so
//! answers are the board the windows show and todo changes go through the engine; with the app
//! closed it reads the files itself (`engine::light_snapshot`, `todos::change`).
//!
//! Output is JSON when stdout isn't a terminal or with `--json`, and tables otherwise. Ids take
//! a unique prefix. `--help` on any subcommand is the reference the agent skill points at, so
//! its text is written for an agent reading it cold.

use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::Path;

use anyhow::{anyhow, bail, Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use serde::Serialize;
use serde_json::Value;

use crate::attachments;
use crate::config::{self, AccountCfg, Config};
use crate::engine;
use crate::model::{Board, Lane, Session, Status};
use crate::procs;
use crate::server::{self, Request};
use crate::start::{self, Place, StartSpec};
use crate::todos::{self, Label, Todo, TodoOp, TodoPatch, TodoStatus, TodoStore, LABEL_COLORS};
use crate::usage;
use crate::util::{expand, now_ms, one_line, tilde, truncate};

const ABOUT: &str = "Codealong from the command line: the todo list, every agent session on this Mac, the worktrees they run in, and the accounts they run on.";
const LONG_ABOUT: &str = "Codealong from the command line: the todo list, every agent session on this Mac, the worktrees they run in, and the accounts they run on.

With the Codealong app running, answers come from it and todo changes go through it. Without it, the files are read directly and sessions show without git status or PRs.

Output is JSON when stdout isn't a terminal (so always, from an agent) or with --json. Times are epoch milliseconds. Ids take a unique prefix. A session is named by its key (claude:claude2:<uuid>) or its id; `me` means the session running this command (see whoami). A worktree is named by its path; `.` means the one the current folder is in.";

#[derive(Parser, Debug)]
#[command(name = "codealong", version, about = ABOUT, long_about = LONG_ABOUT, max_term_width = 100)]
pub struct Cli {
    /// JSON output. The default when stdout isn't a terminal.
    #[arg(long, global = true, conflicts_with = "table")]
    json: bool,
    /// Tables, even when piped.
    #[arg(long, global = true)]
    table: bool,
    /// Say on stderr whether the app or the files answered.
    #[arg(long, global = true)]
    verbose: bool,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Whether the app is running, and where the data lives.
    Status,
    /// The configured accounts: harness, sign-in, and with --usage how much of each plan limit is used.
    Accounts {
        /// Read plan usage: from the app when it runs, else one call per Claude account. Don't loop it; the usage endpoint rate-limits.
        #[arg(long)]
        usage: bool,
    },
    /// Which session runs this command, its worktree, and the todos linked to either. Run it first.
    Whoami {
        /// Plain text for a session-start hook: a line or two when a session or todo is found, nothing otherwise.
        #[arg(long)]
        context: bool,
    },
    /// Todos: work meant for an agent. Each has a title, a description, labels, a status (todo, in-progress, done) and the worktrees and sessions that took it on.
    Todo {
        #[command(subcommand)]
        cmd: TodoCmd,
    },
    /// Sessions on the board: every agent, live or not, with its status and where it works. Lists by default; `find` searches.
    Sessions(SessionsArgs),
    /// One session in full: status, excerpt, folder, branch, PR, how to resume it, and where its own records are.
    Session {
        #[command(subcommand)]
        cmd: SessionCmd,
    },
    /// Worktrees the board knows: each with its branch, PR, status and the agents in it.
    Worktrees(WorktreesArgs),
    /// Start a session for a todo or on a prompt, the way the app's New session does, and link it to the todo.
    Start(StartArgs),
    /// Codealong's own settings (config.toml): show them as the app reads them, and check a file before or after editing it. Change settings by editing the file; the app watches it.
    Config {
        #[command(subcommand)]
        cmd: ConfigCmd,
    },
    /// Put the command on PATH and the skill into each account's skills folder.
    Install(InstallArgs),
    /// The reference for the skill: every subcommand's help, as Markdown.
    #[command(hide = true)]
    Reference,
}

#[derive(Subcommand, Debug)]
enum TodoCmd {
    /// Todos that aren't done, in the app's order. --status all for every one.
    List(TodoListArgs),
    /// One todo in full, with attachments as full paths.
    Show { id: String },
    /// A new todo at the top of the list. Prints it.
    Add(TodoAddArgs),
    /// Change a todo's fields. Only the ones given change.
    Update(TodoUpdateArgs),
    /// Mark a todo in progress, and link the session running this command and its worktree to it.
    Start { id: String },
    /// Mark a todo done.
    Done { id: String },
    /// Put a done or in-progress todo back to todo.
    Reopen { id: String },
    /// Move a todo in the list: to the top or bottom, or before or after another. The app shows todos in this order within each status.
    Move(MoveArgs),
    /// Link a session or a worktree to a todo.
    Link(LinkArgs),
    /// Unlink a session or a worktree from a todo.
    Unlink(LinkArgs),
    /// Delete a todo. The app can't undo this once its window's undo is gone, so ask first.
    Delete { id: String },
    /// The labels, or add and remove one.
    Labels {
        #[command(subcommand)]
        cmd: Option<LabelCmd>,
    },
}

#[derive(Subcommand, Debug)]
enum LabelCmd {
    /// Add a label. Color is one of gray, red, pink, purple, indigo, blue, teal, green; the next unused one when left out.
    Add {
        name: String,
        #[arg(long)]
        color: Option<String>,
    },
    /// Remove a label. Todos keep it until they change, so the app can undo.
    Remove { name: String },
}

#[derive(Args, Debug)]
struct TodoListArgs {
    /// todo, in-progress, done, open (todo and in-progress; the default) or all.
    #[arg(long, default_value = "open")]
    status: String,
    /// Only todos of this project.
    #[arg(long)]
    project: Option<String>,
    /// Only todos with this label (by name).
    #[arg(long)]
    label: Option<String>,
    /// Only todos linked to this worktree (a path, or `.`).
    #[arg(long)]
    worktree: Option<String>,
    /// Only todos linked to the session running this command, or to its worktree.
    #[arg(long)]
    mine: bool,
    /// Only open todos that wait on nothing still open: the ones that can be started now.
    #[arg(long)]
    ready: bool,
}

#[derive(Args, Debug)]
struct TodoAddArgs {
    /// One line.
    title: String,
    /// Plain text. Markdown reads fine in the app.
    #[arg(long, short = 'd', conflicts_with = "description_file")]
    description: Option<String>,
    /// Read the description from a file, or from stdin with `-`.
    #[arg(long, value_name = "FILE")]
    description_file: Option<String>,
    /// The project it belongs to. Set from the worktree when one is linked.
    #[arg(long)]
    project: Option<String>,
    /// A label by name, made if it doesn't exist. Repeatable.
    #[arg(long, short = 'l')]
    label: Vec<String>,
    /// A file to attach: copied into Codealong's store and referenced from the description. Repeatable.
    #[arg(long, value_name = "FILE")]
    attach: Vec<String>,
    /// Link a worktree: a path, or `.` for the current folder's.
    #[arg(long)]
    worktree: Option<String>,
    /// Link a session: a key, an id, or `me`.
    #[arg(long)]
    session: Option<String>,
    /// A todo this one waits on: it's blocked until that one is done. Repeatable.
    #[arg(long, value_name = "ID")]
    depends_on: Vec<String>,
}

#[derive(Args, Debug)]
struct TodoUpdateArgs {
    id: String,
    #[arg(long)]
    title: Option<String>,
    /// Replaces the description. Keep any attachment references you want to keep.
    #[arg(long, short = 'd', conflicts_with = "description_file")]
    description: Option<String>,
    /// Read the new description from a file, or stdin with `-`.
    #[arg(long, value_name = "FILE")]
    description_file: Option<String>,
    /// todo, in-progress or done.
    #[arg(long)]
    status: Option<String>,
    #[arg(long)]
    project: Option<String>,
    /// Add a label by name (made if new). Repeatable.
    #[arg(long)]
    add_label: Vec<String>,
    /// Remove a label by name. Repeatable.
    #[arg(long)]
    remove_label: Vec<String>,
    /// Wait on another todo too. Repeatable. Refused if that one already waits on this one.
    #[arg(long, value_name = "ID")]
    add_dependency: Vec<String>,
    /// Stop waiting on a todo. Repeatable.
    #[arg(long, value_name = "ID")]
    remove_dependency: Vec<String>,
}

#[derive(Args, Debug)]
#[command(group(clap::ArgGroup::new("to").required(true).multiple(false)))]
struct MoveArgs {
    id: String,
    #[arg(long, group = "to")]
    top: bool,
    #[arg(long, group = "to")]
    bottom: bool,
    /// Just before this todo.
    #[arg(long, value_name = "ID", group = "to")]
    before: Option<String>,
    /// Just after this todo.
    #[arg(long, value_name = "ID", group = "to")]
    after: Option<String>,
}

#[derive(Args, Debug)]
struct LinkArgs {
    id: String,
    /// A session key, id, or `me`.
    #[arg(long)]
    session: Option<String>,
    /// A worktree path, or `.`.
    #[arg(long)]
    worktree: Option<String>,
}

#[derive(Args, Debug)]
struct SessionFilter {
    /// Only this account (claude, claude2, codex, opencode, antigravity).
    #[arg(long)]
    account: Option<String>,
    /// Only this status: needs (waiting on you), error, working, unread (its turn ended, not seen), idle, stopped.
    #[arg(long)]
    status: Option<String>,
    /// Only this project.
    #[arg(long)]
    project: Option<String>,
    /// Only sessions in this worktree (a path, or `.`).
    #[arg(long)]
    worktree: Option<String>,
    /// Only sessions with a running process.
    #[arg(long)]
    live: bool,
    /// Active within this long: 90m, 24h, 3d, 7d (the default), or all.
    #[arg(long, default_value = "7d")]
    since: String,
    /// Archived ones too.
    #[arg(long)]
    archived: bool,
    /// At most this many, most recently prompted first.
    #[arg(long, default_value_t = 50)]
    limit: usize,
}

#[derive(Args, Debug)]
#[command(args_conflicts_with_subcommands = false)]
struct SessionsArgs {
    #[command(subcommand)]
    cmd: Option<SessionsCmd>,
    #[command(flatten)]
    filter: SessionFilter,
}

#[derive(Subcommand, Debug)]
enum SessionsCmd {
    /// Sessions whose title, excerpt, account, worktree, branch or PR contains the words (all of them, any case).
    Find {
        query: Vec<String>,
        #[command(flatten)]
        filter: SessionFilter,
    },
}

#[derive(Subcommand, Debug)]
enum SessionCmd {
    /// One session in full.
    Show { key: String },
}

#[derive(Args, Debug)]
struct WorktreesArgs {
    #[arg(long)]
    project: Option<String>,
    /// Only this bay: needs, working, unread, review, idle, merged.
    #[arg(long)]
    bay: Option<String>,
    /// Hidden ones too (worktrees outside every project that need nothing).
    #[arg(long)]
    all: bool,
}

#[derive(Args, Debug)]
struct StartArgs {
    /// The todo to start: its title and description become the prompt, and the session is linked to it.
    #[arg(long, conflicts_with = "prompt")]
    todo: Option<String>,
    /// The prompt, when not starting a todo.
    #[arg(long)]
    prompt: Option<String>,
    /// The account to run it on. `auto` picks as the app does: the Claude account with the most room.
    #[arg(long, default_value = "auto")]
    account: String,
    /// The worktree to run in: a path, or `.` (the default).
    #[arg(long, default_value = ".")]
    worktree: String,
    /// Have Claude Code make a fresh worktree for it (`claude --worktree`).
    #[arg(long)]
    new_worktree: bool,
    /// Where it runs.
    #[arg(long = "in", value_enum, default_value_t = PlaceArg::Terminal)]
    place: PlaceArg,
    /// A model for a terminal start: fable, opus, sonnet, haiku, or a full name.
    #[arg(long)]
    model: Option<String>,
    /// Print what the start would do (the command and environment, or the link) without starting anything.
    #[arg(long)]
    dry_run: bool,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq)]
enum PlaceArg {
    Terminal,
    ClaudeApp,
    CodexApp,
}

#[derive(Subcommand, Debug)]
enum ConfigCmd {
    /// The settings as the app reads them, the file's path and text, its error if it doesn't parse, the terminal and editor profiles, and accounts found on this Mac that aren't configured.
    Show,
    /// Whether a config.toml parses and passes the app's checks: the live one, or FILE.
    Check { file: Option<String> },
}

#[derive(Args, Debug)]
struct InstallArgs {
    /// Only the command: a symlink on PATH to this binary.
    #[arg(long)]
    command: bool,
    /// Only the skill, into the accounts' skills folders.
    #[arg(long)]
    skill: bool,
    /// Only these accounts (by name). Every configured one by default.
    #[arg(long)]
    account: Vec<String>,
    /// Where the symlink goes. /usr/local/bin when writable, else ~/.local/bin.
    #[arg(long, value_name = "DIR")]
    bin: Option<String>,
}

/// Whether a first argument is one of the subcommands (hidden ones included), or asks for help
/// or the version. `main.rs` starts the app for anything else.
pub fn is_cli_arg(arg: &str) -> bool {
    use clap::CommandFactory;
    matches!(arg, "help" | "--help" | "-h" | "--version" | "-V") || Cli::command().get_subcommands().any(|c| c.get_name() == arg || c.get_all_aliases().any(|a| a == arg))
}

/// Whether a whole command line (program name first) is for the command rather than the app:
/// its first argument is a subcommand, help or the version, or one of the global flags, which
/// may come before the subcommand (`codealong --verbose todo add …`). A line that starts with
/// a global flag and names no subcommand still goes to the command, so clap says what's
/// missing instead of a second app starting.
pub fn is_cli(args: &[String]) -> bool {
    use clap::CommandFactory;
    let globals: Vec<String> = Cli::command().get_arguments().filter(|a| a.is_global_set()).filter_map(|a| a.get_long().map(|l| format!("--{l}"))).collect();
    match args.get(1) {
        Some(first) => is_cli_arg(first) || globals.contains(first),
        None => false,
    }
}

/// Runs the command line and returns the exit code: 0, 1 for an error, 2 for usage.
pub fn main(args: &[String]) -> i32 {
    let cli = match Cli::try_parse_from(args) {
        Ok(c) => c,
        Err(e) => {
            let _ = e.print();
            return if e.use_stderr() { 2 } else { 0 };
        }
    };
    // SAFETY: isatty reads a descriptor's kind and touches nothing.
    let json = !cli.table && (cli.json || unsafe { libc::isatty(1) } == 0);
    let mut client = Client::new(cli.verbose);
    match run(cli.cmd, &mut client, json) {
        Ok(()) => 0,
        Err(e) => {
            if json {
                eprintln!("{}", serde_json::json!({ "error": format!("{e:#}") }));
            } else {
                eprintln!("error: {e:#}");
            }
            1
        }
    }
}

/// Where answers come from: the app over its socket, or the files.
struct Client {
    app: Option<server::Status>,
    verbose: bool,
    cfg: Option<Config>,
    board: Option<Board>,
    said: bool,
}

impl Client {
    fn new(verbose: bool) -> Client {
        let app = match server::ask(&Request::Status) {
            Some(Ok(v)) => serde_json::from_value(v).ok(),
            _ => None,
        };
        Client { app, verbose, cfg: None, board: None, said: false }
    }

    fn note(&mut self) {
        if self.verbose && !self.said {
            self.said = true;
            match &self.app {
                Some(s) => eprintln!("via the app (pid {}, version {})", s.pid, s.version),
                None => eprintln!("via the files (the app isn't running)"),
            }
        }
    }

    fn cfg(&mut self) -> &Config {
        if self.cfg.is_none() {
            self.cfg = Some(config::load().ok().flatten().unwrap_or_default());
        }
        self.cfg.as_ref().unwrap()
    }

    /// The board: the app's, or a light one from the files (no git, PRs or visitors).
    fn board(&mut self) -> Result<&Board> {
        self.note();
        if self.board.is_none() {
            let board = match self.app.is_some() {
                true => match server::ask(&Request::Board) {
                    Some(Ok(v)) => serde_json::from_value(v).context("reading the app's board")?,
                    Some(Err(e)) => bail!("{e}"),
                    None => engine::light_snapshot(),
                },
                false => engine::light_snapshot(),
            };
            self.board = Some(board);
        }
        Ok(self.board.as_ref().unwrap())
    }

    fn todos(&mut self) -> Result<TodoStore> {
        self.note();
        if self.app.is_some() {
            let b = self.board()?;
            return Ok(TodoStore { todos: b.todos.clone(), labels: b.labels.clone() });
        }
        Ok(todos::load()?.0)
    }

    /// Applies a change: through the engine, or to the file under the lock. Returns whether
    /// anything changed. The cached board is dropped, so the next read shows the change.
    fn apply(&mut self, op: TodoOp) -> Result<bool> {
        self.note();
        let needs_lanes = matches!(op, TodoOp::Link { .. });
        let out = match self.app.is_some() {
            true => match server::ask(&Request::TodoApply { op }) {
                Some(Ok(_)) => Ok(true),
                Some(Err(e)) => Err(anyhow!("{e}")),
                None => bail!("The app stopped answering."),
            },
            false => {
                let lanes: Box<dyn todos::Lanes> = match needs_lanes {
                    true => Box::new(BoardLanes(self.board()?.lanes.iter().map(|l| (l.id.clone(), (l.project.clone(), repo_identity(l)))).collect())),
                    false => Box::new(BoardLanes(HashMap::new())),
                };
                let now = now_ms();
                todos::change(|store| Ok((store.apply(op, lanes.as_ref(), now)?, ()))).map(|(_, changed, _)| changed)
            }
        };
        self.board = None;
        out
    }

    fn start(&mut self, spec: &StartSpec) -> Result<start::Started> {
        self.note();
        if self.app.is_some() {
            return match server::ask(&Request::Start { spec: spec.clone() }) {
                Some(Ok(v)) => serde_json::from_value(v).context("reading the app's reply"),
                Some(Err(e)) => Err(anyhow!("{e}")),
                None => bail!("The app stopped answering."),
            };
        }
        let cfg = self.cfg().clone();
        let started = start::start(self.board()?, &cfg, spec)?;
        if let Some(id) = spec.todo.clone() {
            let pending = match (&started.key, spec.place) {
                (Some(_), _) => None,
                (None, place) => Some(todos::PendingStart { lane: spec.lane_id.clone(), harness: if place == Place::CodexApp { "codex" } else { "claude" }.into(), account: spec.account.clone(), at: now_ms() }),
            };
            let lane = (!(spec.worktree && spec.place == Place::Terminal)).then(|| spec.lane_id.clone());
            let lanes = BoardLanes(self.board()?.lanes.iter().map(|l| (l.id.clone(), (l.project.clone(), repo_identity(l)))).collect());
            let key = started.key.clone();
            todos::change(|store| Ok((store.started(&id, lane.as_deref(), key.as_deref(), pending, &lanes, now_ms()), ())))?;
            self.board = None;
        }
        Ok(started)
    }
}

struct BoardLanes(HashMap<String, (String, String)>);

impl todos::Lanes for BoardLanes {
    fn project_of(&self, lane: &str) -> Option<String> {
        self.0.get(lane).map(|(p, _)| p.clone()).filter(|p| p != crate::board::ELSEWHERE)
    }
    fn repo_of(&self, lane: &str) -> Option<String> {
        self.0.get(lane).map(|(_, r)| r.clone())
    }
}

fn repo_identity(l: &Lane) -> String {
    l.repo_slug.clone().unwrap_or_else(|| format!("{}/{}", l.project, l.repo))
}

fn run(cmd: Cmd, c: &mut Client, json: bool) -> Result<()> {
    match cmd {
        Cmd::Status => status(c, json),
        Cmd::Accounts { usage } => accounts(c, usage, json),
        Cmd::Whoami { context } => whoami(c, context, json),
        Cmd::Todo { cmd } => todo(cmd, c, json),
        Cmd::Sessions(args) => match args.cmd {
            Some(SessionsCmd::Find { query, filter }) => sessions(c, &filter, &query, json),
            None => sessions(c, &args.filter, &[], json),
        },
        Cmd::Session { cmd: SessionCmd::Show { key } } => session_show(c, &key, json),
        Cmd::Worktrees(args) => worktrees(c, &args, json),
        Cmd::Start(args) => start_cmd(c, &args, json),
        Cmd::Config { cmd } => config_cmd(cmd, json),
        Cmd::Install(args) => install(c, &args, json),
        Cmd::Reference => {
            print!("{}", reference_text());
            Ok(())
        }
    }
}

/// Every subcommand's help, as the skill's reference. `scripts/skill-reference.sh` writes it to
/// `skills/codealong/references/reference.md`; a test checks the file is current.
pub fn reference_text() -> String {
    use clap::CommandFactory;
    fn walk(cmd: &mut clap::Command, path: &str, out: &mut String) {
        let name = cmd.get_name().to_string();
        let full = if path.is_empty() { name.clone() } else { format!("{path} {name}") };
        let mut help = Vec::new();
        cmd.write_long_help(&mut help).ok();
        let help = String::from_utf8_lossy(&help).trim_end().to_string();
        out.push_str(&format!("## {full}\n\n```\n{help}\n```\n\n"));
        let subs: Vec<String> = cmd.get_subcommands().filter(|c| !c.is_hide_set() && c.get_name() != "help").map(|c| c.get_name().to_string()).collect();
        for s in subs {
            let mut sub = cmd.find_subcommand_mut(&s).unwrap().clone();
            sub.build();
            walk(&mut sub, &full, out);
        }
    }
    let mut cmd = Cli::command().term_width(88);
    cmd.build();
    let mut out = String::from("# codealong command reference\n\nGenerated from `codealong --help` by `scripts/skill-reference.sh`. Don't edit by hand.\n\n");
    walk(&mut cmd, "", &mut out);
    out
}

// ---- status and accounts ----

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StatusOut {
    app_running: bool,
    app: Option<server::Status>,
    socket: String,
    version: String,
    data_dir: String,
    config: String,
    todos: String,
    accounts: usize,
    /// The symlink `install` made, when it's in place.
    command_link: Option<String>,
    /// Where the skill is installed, per account.
    skills: Vec<SkillStatus>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SkillStatus {
    account: String,
    path: Option<String>,
    installed: bool,
    current: bool,
    edited: bool,
}

fn status(c: &mut Client, json: bool) -> Result<()> {
    let out = StatusOut {
        app_running: c.app.is_some(),
        app: c.app.clone(),
        socket: server::path().to_string_lossy().into(),
        version: env!("CARGO_PKG_VERSION").into(),
        data_dir: config::support_dir().to_string_lossy().into(),
        config: config::config_path().to_string_lossy().into(),
        todos: todos::path().to_string_lossy().into(),
        accounts: c.cfg().accounts.len(),
        command_link: crate::install::command_link().map(|p| p.to_string_lossy().into_owned()),
        skills: {
            let source = crate::install::skill_source();
            c.cfg()
                .accounts
                .iter()
                .map(|a| {
                    let dir = crate::install::skill_dir(a);
                    let state = match (&source, &dir) {
                        (Some(s), Some(d)) => crate::install::copy_state(s, d),
                        _ => crate::install::CopyState::default(),
                    };
                    SkillStatus { account: a.name.clone(), path: dir.map(|d| d.to_string_lossy().into_owned()), installed: state.installed, current: state.current, edited: state.edited }
                })
                .collect()
        },
    };
    if json {
        return print_json(&out);
    }
    match &out.app {
        Some(a) => println!("Codealong {} is running (pid {}){}.", a.version, a.pid, if a.loading { ", still scanning" } else { "" }),
        None => println!("Codealong isn't running; the files are read directly."),
    }
    println!("Data: {}", tilde(Path::new(&out.data_dir)));
    println!("Accounts: {}", out.accounts);
    match &out.command_link {
        Some(l) => println!("Command: {}", tilde(Path::new(l))),
        None => println!("Command: not on PATH (codealong install --command)"),
    }
    for s in &out.skills {
        println!("Skill for {}: {}", s.account, match (s.installed, s.current, s.edited, &s.path) { (true, _, true, Some(p)) => format!("edited at {}, so updates leave it alone", tilde(Path::new(p))), (true, true, _, Some(p)) => format!("installed at {}", tilde(Path::new(p))), (true, false, _, Some(p)) => format!("out of date at {}", tilde(Path::new(p))), (_, _, _, Some(_)) => "not installed".into(), _ => "no skills folder known".into() });
    }
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AccountRow {
    name: String,
    harness: String,
    command: Option<String>,
    signed_in: bool,
    email: Option<String>,
    home: String,
    /// The plan limits, when read: label, percent used, when it resets.
    #[serde(skip_serializing_if = "Option::is_none")]
    usage: Option<Vec<MeterRow>>,
    /// How much of the tightest limit is left, when read.
    #[serde(skip_serializing_if = "Option::is_none")]
    room: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    usage_error: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MeterRow {
    label: String,
    window: String,
    percent: f64,
    resets_at: Option<i64>,
}

fn accounts(c: &mut Client, with_usage: bool, json: bool) -> Result<()> {
    let cfg = c.cfg().clone();
    let mut rows: Vec<AccountRow> = cfg
        .accounts
        .iter()
        .map(|a| {
            let info = usage::describe(a);
            AccountRow { name: info.name, harness: info.harness, command: info.command, signed_in: info.signed_in, email: info.email, home: info.home, usage: None, room: None, usage_error: None }
        })
        .collect();
    if with_usage {
        let usages = match c.app.is_some() {
            true => c.board()?.usage.clone(),
            false => usage::claude_all(&cfg.accounts),
        };
        for u in usages {
            let Some(row) = rows.iter_mut().find(|r| r.name == u.account) else { continue };
            if u.error.is_some() && u.meters.is_empty() {
                row.usage_error = u.error.clone();
                continue;
            }
            row.room = u.meters.iter().map(|m| 100.0 - m.percent).fold(None, |acc: Option<f64>, x| Some(acc.map_or(x, |a| a.min(x))));
            row.usage = Some(u.meters.iter().map(|m| MeterRow { label: m.label.clone(), window: m.window.clone(), percent: m.percent, resets_at: m.resets_at }).collect());
        }
    }
    if json {
        return print_json(&rows);
    }
    let now = now_ms();
    let table: Vec<Vec<String>> = rows
        .iter()
        .map(|r| {
            let usage = match (&r.usage, &r.usage_error) {
                (Some(ms), _) => ms.iter().map(|m| format!("{} {:.0}%{}", m.label, m.percent, m.resets_at.map(|t| format!(" (resets in {})", ago(t - now))).unwrap_or_default())).collect::<Vec<_>>().join(", "),
                (None, Some(e)) => e.clone(),
                _ => String::new(),
            };
            vec![r.name.clone(), r.harness.clone(), if r.signed_in { "signed in".into() } else { "not signed in".into() }, r.email.clone().unwrap_or_default(), usage]
        })
        .collect();
    print!("{}", table_text(&["ACCOUNT", "HARNESS", "SIGN-IN", "EMAIL", "USAGE"], table));
    Ok(())
}

// ---- sessions ----

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct SessionRow {
    key: String,
    id: String,
    account: String,
    harness: String,
    status: String,
    live: bool,
    archived: bool,
    title: String,
    excerpt: Option<String>,
    headline: String,
    cwd: String,
    worktree: String,
    project: String,
    branch: Option<String>,
    pr: Option<u64>,
    started_at: Option<i64>,
    last_activity: i64,
    last_prompt_at: i64,
    resume_command: Option<String>,
    attach_command: Option<String>,
    /// Ids of todos linked to it.
    todos: Vec<String>,
}

fn session_row(s: &Session, lane: &Lane, todos: &[Todo]) -> SessionRow {
    SessionRow {
        key: s.key.clone(),
        id: s.id.clone(),
        account: s.account.clone(),
        harness: lower_debug(&s.harness),
        status: lower_debug(&s.status),
        live: s.live,
        archived: s.archived,
        title: s.title.clone(),
        excerpt: s.excerpt.clone(),
        headline: s.headline.clone(),
        cwd: s.cwd.clone(),
        worktree: lane.id.clone(),
        project: lane.project.clone(),
        branch: s.branch.clone().or_else(|| lane.branch.clone()),
        pr: lane.pr.as_ref().map(|p| p.number),
        started_at: s.started_at,
        last_activity: s.last_activity,
        last_prompt_at: s.last_prompt_at,
        resume_command: s.resume_command.clone(),
        attach_command: s.attach_command.clone(),
        todos: todos.iter().filter(|t| t.sessions.contains(&s.key)).map(|t| t.id.clone()).collect(),
    }
}

/// Every session once, with the lane it's listed under as its own (not as a visitor).
fn all_sessions(board: &Board) -> Vec<(&Session, &Lane)> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for lane in &board.lanes {
        for s in lane.sessions.iter().filter(|s| s.visitor.is_none()) {
            if seen.insert(s.key.clone()) {
                out.push((s, lane));
            }
        }
    }
    for lane in &board.lanes {
        for s in lane.sessions.iter().filter(|s| s.visitor.is_some()) {
            if seen.insert(s.key.clone()) {
                out.push((s, lane));
            }
        }
    }
    out
}

fn sessions(c: &mut Client, f: &SessionFilter, query: &[String], json: bool) -> Result<()> {
    let since = parse_since(&f.since)?;
    let status = f.status.as_deref().map(parse_status).transpose()?;
    let worktree = f.worktree.as_deref().map(|w| resolve_worktree(c, w)).transpose()?;
    let board = c.board()?;
    let now = now_ms();
    let words: Vec<String> = query.iter().flat_map(|q| q.split_whitespace()).map(str::to_lowercase).collect();
    let mut rows: Vec<SessionRow> = all_sessions(board)
        .into_iter()
        .filter(|(s, lane)| {
            (f.archived || !s.archived)
                && f.account.as_deref().map_or(true, |a| a == s.account)
                && status.map_or(true, |st| st == s.status)
                && f.project.as_deref().map_or(true, |p| p.eq_ignore_ascii_case(&lane.project))
                && worktree.as_deref().map_or(true, |w| w == lane.id)
                && (!f.live || s.live)
                && since.map_or(true, |ms| now - s.last_activity <= ms)
                && (words.is_empty() || {
                    let hay = [s.title.as_str(), s.account.as_str(), s.excerpt.as_deref().unwrap_or(""), lane.callsign.as_str(), lane.repo.as_str(), lane.branch.as_deref().unwrap_or(""), &lane.pr.as_ref().map(|p| format!("#{} {}", p.number, p.title)).unwrap_or_default()].join(" ").to_lowercase();
                    words.iter().all(|w| hay.contains(w))
                })
        })
        .map(|(s, lane)| session_row(s, lane, &board.todos))
        .collect();
    rows.sort_by(|a, b| (b.status == "needs").cmp(&(a.status == "needs")).then(b.last_prompt_at.cmp(&a.last_prompt_at)));
    rows.truncate(f.limit);
    if json {
        return print_json(&rows);
    }
    let table: Vec<Vec<String>> = rows
        .iter()
        .map(|r| vec![r.status.clone(), r.account.clone(), ago(now - r.last_activity), r.worktree_short(), truncate(&r.title, 50), truncate(r.excerpt.as_deref().unwrap_or(""), 60), r.key.clone()])
        .collect();
    print!("{}", table_text(&["STATUS", "ACCOUNT", "AGO", "WORKTREE", "TITLE", "EXCERPT", "KEY"], table));
    Ok(())
}

impl SessionRow {
    fn worktree_short(&self) -> String {
        tilde(Path::new(&self.worktree))
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionDetail {
    #[serde(flatten)]
    row: SessionRow,
    detail: Option<String>,
    name: Option<String>,
    status_since: Option<i64>,
    stalled_since: Option<i64>,
    pid: Option<u32>,
    worktree_callsign: String,
    repo: String,
    pr_title: Option<String>,
    pr_state: Option<String>,
    /// The session's own files: the transcript for Claude Code, the rollout for Codex, the
    /// database for OpenCode and Antigravity.
    records: Vec<String>,
    linked_todos: Vec<TodoRow>,
}

fn session_show(c: &mut Client, key: &str, json: bool) -> Result<()> {
    let cfg = c.cfg().clone();
    let key = resolve_session(c, key)?;
    let board = c.board()?;
    let (s, lane) = all_sessions(board).into_iter().find(|(s, _)| s.key == key).ok_or_else(|| anyhow!("There's no session {key}."))?;
    let row = session_row(s, lane, &board.todos);
    let account = cfg.accounts.iter().find(|a| a.name == s.account);
    let out = SessionDetail {
        detail: s.detail.clone(),
        name: s.name.clone(),
        status_since: s.status_since,
        stalled_since: s.stalled_since,
        pid: s.pid,
        worktree_callsign: lane.callsign.clone(),
        repo: lane.repo.clone(),
        pr_title: lane.pr.as_ref().map(|p| p.title.clone()),
        pr_state: lane.pr.as_ref().map(|p| lower_debug(&p.state)),
        records: account.map(|a| records_for(a, s)).unwrap_or_default(),
        linked_todos: board.todos.iter().filter(|t| t.sessions.contains(&s.key)).map(|t| todo_row(t, &store_of(board), false)).collect(),
        row,
    };
    if json {
        return print_json(&out);
    }
    let r = &out.row;
    println!("{}  {} on {}{}", r.key, r.status, r.account, if r.live { " (live)" } else { "" });
    println!("Title: {}", r.title);
    if let Some(e) = &r.excerpt {
        println!("Now: {e}");
    }
    println!("Worktree: {} ({}{})", tilde(Path::new(&r.worktree)), out.worktree_callsign, r.branch.as_deref().map(|b| format!(", {b}")).unwrap_or_default());
    if let Some(n) = r.pr {
        println!("PR: #{n} {} ({})", out.pr_title.clone().unwrap_or_default(), out.pr_state.clone().unwrap_or_default());
    }
    if let Some(cmd) = &r.resume_command {
        println!("Resume: {cmd}");
    }
    for p in &out.records {
        println!("Record: {}", tilde(Path::new(p)));
    }
    for t in &out.linked_todos {
        println!("Todo: {} {} ({})", t.id, t.title, t.status);
    }
    Ok(())
}

/// Where a session's own harness keeps it. Claude Code's transcript is looked up by id
/// under every project folder; Codex's rollout by its id in the file name.
fn records_for(account: &AccountCfg, s: &Session) -> Vec<String> {
    let home = account.home_path();
    let mut out = Vec::new();
    match account.harness.as_str() {
        "claude" => {
            if let Ok(dirs) = std::fs::read_dir(home.join("projects")) {
                for d in dirs.flatten() {
                    let p = d.path().join(format!("{}.jsonl", s.id));
                    if p.is_file() {
                        out.push(p.to_string_lossy().into_owned());
                    }
                }
            }
        }
        "codex" => {
            let suffix = format!("-{}.jsonl", s.id);
            for root in ["sessions", "archived_sessions"] {
                walk(&home.join(root), 4, &mut |p| {
                    if p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("rollout-") && n.ends_with(&suffix)) {
                        out.push(p.to_string_lossy().into_owned());
                    }
                });
            }
        }
        "opencode" => out.push(home.join("opencode.db").to_string_lossy().into_owned()),
        "antigravity" => out.push(home.join("conversation_summaries.db").to_string_lossy().into_owned()),
        _ => {}
    }
    out
}

fn walk(dir: &Path, depth: usize, visit: &mut dyn FnMut(&Path)) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            if depth > 0 {
                walk(&p, depth - 1, visit);
            }
        } else {
            visit(&p);
        }
    }
}

// ---- worktrees ----

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LaneRow {
    id: String,
    project: String,
    repo: String,
    callsign: String,
    branch: Option<String>,
    bay: String,
    status: String,
    headline: String,
    pr: Option<u64>,
    pr_state: Option<String>,
    pr_title: Option<String>,
    agents: usize,
    last_activity: i64,
    pinned: bool,
    /// Ids of todos linked to it.
    todos: Vec<String>,
}

fn lane_row(l: &Lane, board: &Board) -> LaneRow {
    LaneRow {
        id: l.id.clone(),
        project: l.project.clone(),
        repo: l.repo.clone(),
        callsign: l.callsign.clone(),
        branch: l.branch.clone(),
        bay: lower_debug(&l.bay),
        status: lower_debug(&l.glyph),
        headline: l.headline.clone(),
        pr: l.pr.as_ref().map(|p| p.number),
        pr_state: l.pr.as_ref().map(|p| lower_debug(&p.state)),
        pr_title: l.pr.as_ref().map(|p| p.title.clone()),
        agents: l.sessions.iter().filter(|s| s.visitor.is_none() && !s.archived).count(),
        last_activity: l.last_activity,
        pinned: board.pinned_worktrees.contains(&l.id),
        todos: board.todos.iter().filter(|t| t.worktrees.contains(&l.id)).map(|t| t.id.clone()).collect(),
    }
}

fn worktrees(c: &mut Client, a: &WorktreesArgs, json: bool) -> Result<()> {
    let board = c.board()?;
    let rows: Vec<LaneRow> = board
        .lanes
        .iter()
        .filter(|l| (a.all || !l.hidden) && a.project.as_deref().map_or(true, |p| p.eq_ignore_ascii_case(&l.project)) && a.bay.as_deref().map_or(true, |b| b.eq_ignore_ascii_case(&lower_debug(&l.bay))))
        .map(|l| lane_row(l, board))
        .collect();
    if json {
        return print_json(&rows);
    }
    let now = now_ms();
    let table: Vec<Vec<String>> = rows
        .iter()
        .map(|r| vec![r.bay.clone(), r.callsign.clone(), r.branch.clone().unwrap_or_default(), r.pr.map(|n| format!("#{n} {}", r.pr_state.clone().unwrap_or_default())).unwrap_or_default(), r.agents.to_string(), ago(now - r.last_activity), truncate(&r.headline, 60), tilde(Path::new(&r.id))])
        .collect();
    print!("{}", table_text(&["BAY", "WORKTREE", "BRANCH", "PR", "AGENTS", "AGO", "HEADLINE", "PATH"], table));
    Ok(())
}

// ---- whoami ----

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Whoami {
    found: bool,
    /// `environment` (CODEALONG_SESSION was set by the app's start) or `process` (a parent process is a session the board knows).
    how: Option<String>,
    session: Option<SessionRow>,
    worktree: Option<LaneRow>,
    /// Todos linked to the session, then to the worktree.
    todos: Vec<TodoRow>,
}

/// The session running this command: `CODEALONG_SESSION` from a start the app made, else
/// the parent process that's a live session on the board.
fn find_me(c: &mut Client) -> Result<(Option<String>, Option<&'static str>)> {
    if let Ok(key) = std::env::var(start::SESSION_VAR) {
        if !key.trim().is_empty() {
            return Ok((Some(key), Some("environment")));
        }
    }
    let mine = procs::ancestors(std::process::id());
    let board = c.board()?;
    let hit = all_sessions(board).into_iter().filter(|(s, _)| s.live).find(|(s, _)| s.pid.is_some_and(|p| mine.contains(&p)));
    Ok(match hit {
        Some((s, _)) => (Some(s.key.clone()), Some("process")),
        None => (None, None),
    })
}

fn whoami(c: &mut Client, context: bool, json: bool) -> Result<()> {
    let (key, how) = find_me(c)?;
    let cwd = std::env::current_dir().ok();
    let board = c.board()?;
    let session = key.as_deref().and_then(|k| all_sessions(board).into_iter().find(|(s, _)| s.key == k));
    let lane = session.map(|(_, l)| l).or_else(|| cwd.as_deref().and_then(|d| lane_for_path(board, d)));
    let mut todos: Vec<&Todo> = board.todos.iter().filter(|t| key.as_deref().is_some_and(|k| t.sessions.contains(&k.to_string()))).collect();
    if let Some(l) = lane {
        let more: Vec<&Todo> = board.todos.iter().filter(|t| t.worktrees.contains(&l.id) && !todos.iter().any(|x| x.id == t.id)).collect();
        todos.extend(more);
    }
    let out = Whoami {
        found: session.is_some(),
        how: session.and(how).map(str::to_string),
        session: session.map(|(s, l)| session_row(s, l, &board.todos)),
        worktree: lane.map(|l| lane_row(l, board)),
        todos: todos.iter().map(|t| todo_row(t, &store_of(board), false)).collect(),
    };
    if json {
        return print_json(&out);
    }
    if context {
        if let Some(s) = &out.session {
            println!("This is Codealong session {} ({} on {}), in worktree {}.", s.key, s.harness, s.account, tilde(Path::new(&s.worktree)));
        }
        for t in &out.todos {
            println!("Todo {} \"{}\" is linked here ({}).{}", t.id, t.title, t.status, if t.status == "in-progress" { " Mark it done with `codealong todo done` when the work is merged or the user says so." } else { "" });
        }
        return Ok(());
    }
    match &out.session {
        Some(s) => println!("Session {} ({} on {}, {}), found by {}.", s.key, s.harness, s.account, s.status, out.how.clone().unwrap_or_default()),
        None => println!("No session of this shell is on the board.{}", if key.is_some() { " CODEALONG_SESSION names one the board doesn't show yet." } else { "" }),
    }
    if let Some(w) = &out.worktree {
        println!("Worktree {} ({}{}){}", tilde(Path::new(&w.id)), w.callsign, w.branch.as_deref().map(|b| format!(", {b}")).unwrap_or_default(), w.pr.map(|n| format!(", PR #{n}")).unwrap_or_default());
    }
    for t in &out.todos {
        println!("Todo {} {} ({})", t.id, t.title, t.status);
    }
    Ok(())
}

/// The worktree the folder is in: the lane whose path contains it, the deepest one.
fn lane_for_path<'a>(board: &'a Board, dir: &Path) -> Option<&'a Lane> {
    let dir = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
    board.lanes.iter().filter(|l| dir.starts_with(&l.path) || std::fs::canonicalize(&l.path).is_ok_and(|p| dir.starts_with(p))).max_by_key(|l| l.path.len())
}

fn resolve_worktree(c: &mut Client, w: &str) -> Result<String> {
    let path = if w == "." { std::env::current_dir()? } else { expand(w) };
    let board = c.board()?;
    if let Some(l) = board.lanes.iter().find(|l| Path::new(&l.id) == path) {
        return Ok(l.id.clone());
    }
    lane_for_path(board, &path).map(|l| l.id.clone()).ok_or_else(|| anyhow!("{} isn't a worktree the board knows. `codealong worktrees` lists them.", tilde(&path)))
}

fn resolve_session(c: &mut Client, k: &str) -> Result<String> {
    if k == "me" {
        return find_me(c)?.0.ok_or_else(|| anyhow!("This shell isn't running inside a session the board knows, so `me` names nothing. Give a session key."));
    }
    if k.trim().is_empty() {
        bail!("Give a session key.");
    }
    let board = c.board()?;
    let all = all_sessions(board);
    if let Some((s, _)) = all.iter().find(|(s, _)| s.key == k || s.id == k) {
        return Ok(s.key.clone());
    }
    let hits: Vec<&Session> = all.iter().map(|(s, _)| *s).filter(|s| s.id.starts_with(k) || s.key.starts_with(k) || s.short_id == k).collect();
    match hits.len() {
        1 => Ok(hits[0].key.clone()),
        0 => bail!("There's no session {k}. `codealong sessions` lists them."),
        n => bail!("{n} sessions start with {k}; give more of the id."),
    }
}

// ---- todos ----

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct TodoRow {
    id: String,
    title: String,
    status: String,
    project: String,
    /// Label names.
    labels: Vec<String>,
    worktrees: Vec<String>,
    sessions: Vec<String>,
    /// Ids of the todos it waits on.
    depends_on: Vec<String>,
    /// The ones of those not done yet: while any are, it's blocked.
    blocked_by: Vec<String>,
    created_at: i64,
    updated_at: i64,
    done_at: Option<i64>,
    /// Titles of the todos it waits on, and ids and titles of the todos waiting on it. Only on `show`.
    #[serde(skip_serializing_if = "Option::is_none")]
    waits_on: Option<Vec<TodoRef>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    needed_by: Option<Vec<TodoRef>>,
    /// The description, with attachments as full paths. Only on `show`.
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    attachments: Option<Vec<String>>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct TodoRef {
    id: String,
    title: String,
    status: String,
}

fn todo_ref(t: &Todo) -> TodoRef {
    TodoRef { id: t.id.clone(), title: t.title.clone(), status: status_name(t.status).into() }
}

fn status_name(s: TodoStatus) -> &'static str {
    match s {
        TodoStatus::Todo => "todo",
        TodoStatus::InProgress => "in-progress",
        TodoStatus::Done => "done",
    }
}

fn todo_row(t: &Todo, store: &TodoStore, full: bool) -> TodoRow {
    TodoRow {
        id: t.id.clone(),
        title: t.title.clone(),
        status: status_name(t.status).into(),
        project: t.project.clone(),
        labels: t.labels.iter().map(|id| store.labels.iter().find(|l| &l.id == id).map(|l| l.name.clone()).unwrap_or_else(|| id.clone())).collect(),
        worktrees: t.worktrees.clone(),
        sessions: t.sessions.clone(),
        depends_on: t.depends_on.iter().filter(|d| store.todos.iter().any(|x| &x.id == *d)).cloned().collect(),
        blocked_by: store.blocked_by(t).iter().map(|x| x.id.clone()).collect(),
        created_at: t.created_at,
        updated_at: t.updated_at,
        done_at: t.done_at,
        waits_on: full.then(|| t.depends_on.iter().filter_map(|d| store.todos.iter().find(|x| &x.id == d)).map(todo_ref).collect()),
        needed_by: full.then(|| store.todos.iter().filter(|x| x.depends_on.contains(&t.id)).map(todo_ref).collect()),
        description: full.then(|| attachments::expand(&t.description, attachments::Style::Path)),
        attachments: full.then(|| attachments::referenced(&t.description).iter().filter_map(|p| attachments::locate(p)).map(|p| p.to_string_lossy().into_owned()).collect()),
    }
}

fn store_of(board: &Board) -> TodoStore {
    TodoStore { todos: board.todos.clone(), labels: board.labels.clone() }
}

fn resolve_todo(store: &TodoStore, id: &str) -> Result<String> {
    if id.trim().is_empty() {
        bail!("Give a todo id.");
    }
    if store.todos.iter().any(|t| t.id == id) {
        return Ok(id.to_string());
    }
    let hits: Vec<&Todo> = store.todos.iter().filter(|t| t.id.starts_with(id)).collect();
    match hits.len() {
        1 => Ok(hits[0].id.clone()),
        0 => bail!("There's no todo {id}. `codealong todo list --status all` lists them."),
        n => bail!("{n} todos start with {id}; give more of the id."),
    }
}

fn parse_todo_status(s: &str) -> Result<TodoStatus> {
    serde_json::from_value(Value::String(s.to_string())).map_err(|_| anyhow!("Status is todo, in-progress or done, not {s}."))
}

fn new_todo_id() -> String {
    let now = now_ms() as u64;
    let mut b = [0u8; 4];
    // SAFETY: arc4random_buf writes exactly the length it's given into the buffer.
    unsafe { libc::arc4random_buf(b.as_mut_ptr().cast(), b.len()) };
    format!("t-{}{}", base36(now), base36(u32::from_le_bytes(b) as u64 % 36u64.pow(4)))
}

fn base36(mut n: u64) -> String {
    const DIGITS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    if n == 0 {
        return "0".into();
    }
    let mut s = Vec::new();
    while n > 0 {
        s.push(DIGITS[(n % 36) as usize]);
        n /= 36;
    }
    s.reverse();
    String::from_utf8(s).unwrap_or_default()
}

/// Ids for the label names, making any that don't exist (through a Labels change).
fn label_ids(c: &mut Client, store: &mut TodoStore, names: &[String]) -> Result<Vec<String>> {
    let mut ids = Vec::new();
    let mut new_labels = store.labels.clone();
    let mut added = false;
    for name in names {
        let name = one_line(name, 40);
        if let Some(l) = new_labels.iter().find(|l| l.name.eq_ignore_ascii_case(&name)) {
            ids.push(l.id.clone());
            continue;
        }
        let used: HashSet<&str> = new_labels.iter().map(|l| l.color.as_str()).collect();
        let color = LABEL_COLORS.iter().find(|c| !used.contains(*c)).unwrap_or(&LABEL_COLORS[new_labels.len() % LABEL_COLORS.len()]);
        let id = format!("l-{}", base36(now_ms() as u64 + new_labels.len() as u64));
        new_labels.push(Label { id: id.clone(), name, color: color.to_string() });
        ids.push(id);
        added = true;
    }
    if added {
        c.apply(TodoOp::Labels { labels: new_labels.clone() })?;
        store.labels = new_labels;
    }
    Ok(ids)
}

fn read_description(text: Option<String>, file: Option<String>) -> Result<Option<String>> {
    if let Some(t) = text {
        return Ok(Some(t));
    }
    let Some(f) = file else { return Ok(None) };
    if f == "-" {
        let mut s = String::new();
        std::io::stdin().read_to_string(&mut s)?;
        return Ok(Some(s));
    }
    Ok(Some(std::fs::read_to_string(expand(&f)).with_context(|| format!("reading {f}"))?))
}

fn todo(cmd: TodoCmd, c: &mut Client, json: bool) -> Result<()> {
    match cmd {
        TodoCmd::List(a) => todo_list(c, &a, json),
        TodoCmd::Show { id } => {
            let store = c.todos()?;
            let id = resolve_todo(&store, &id)?;
            let t = store.todos.iter().find(|t| t.id == id).unwrap();
            let row = todo_row(t, &store, true);
            if json {
                return print_json(&row);
            }
            print_todo(&row);
            Ok(())
        }
        TodoCmd::Add(a) => {
            // Links are resolved first, so a wrong worktree or session leaves no half-made todo.
            let worktree = a.worktree.as_deref().map(|w| resolve_worktree(c, w)).transpose()?;
            let session = a.session.as_deref().map(|s| resolve_session(c, s)).transpose()?;
            let mut store = c.todos()?;
            let depends_on = a.depends_on.iter().map(|d| resolve_todo(&store, d)).collect::<Result<Vec<_>>>()?;
            let labels = label_ids(c, &mut store, &a.label)?;
            let mut description = read_description(a.description, a.description_file)?.unwrap_or_default();
            for f in &a.attach {
                let kept = attachments::import(&expand(f))?;
                let reference = match kept.kind {
                    attachments::Kind::Image => format!("![{}]({})", kept.name, kept.path),
                    attachments::Kind::File => format!("[{}]({})", kept.name, kept.path),
                };
                if !description.is_empty() && !description.ends_with('\n') {
                    description.push('\n');
                }
                description.push_str(&reference);
                description.push('\n');
            }
            let id = new_todo_id();
            let now = now_ms();
            let todo = Todo { id: id.clone(), project: a.project.unwrap_or_default(), title: a.title, description, status: TodoStatus::Todo, labels, worktrees: vec![], sessions: vec![], created_at: now, updated_at: now, done_at: None, pending: vec![], depends_on };
            c.apply(TodoOp::Create { todo })?;
            link(c, &id, worktree.as_deref(), session.as_deref(), true)?;
            show_after(c, &id, json)
        }
        TodoCmd::Update(a) => {
            let mut store = c.todos()?;
            let id = resolve_todo(&store, &a.id)?;
            let current = store.todos.iter().find(|t| t.id == id).unwrap().clone();
            let mut labels = None;
            if !a.add_label.is_empty() || !a.remove_label.is_empty() {
                let add = label_ids(c, &mut store, &a.add_label)?;
                let remove: Vec<String> = a.remove_label.iter().filter_map(|n| store.labels.iter().find(|l| l.name.eq_ignore_ascii_case(n)).map(|l| l.id.clone())).collect();
                let mut list: Vec<String> = current.labels.iter().filter(|l| !remove.contains(l)).cloned().collect();
                for l in add {
                    if !list.contains(&l) {
                        list.push(l);
                    }
                }
                labels = Some(list);
            }
            let mut depends_on = None;
            if !a.add_dependency.is_empty() || !a.remove_dependency.is_empty() {
                let add = a.add_dependency.iter().map(|d| resolve_todo(&store, d)).collect::<Result<Vec<_>>>()?;
                let remove: Vec<String> = a.remove_dependency.iter().map(|d| resolve_todo(&store, d).unwrap_or_else(|_| d.clone())).collect();
                let mut list: Vec<String> = current.depends_on.iter().filter(|d| !remove.contains(d)).cloned().collect();
                for d in add {
                    if !list.contains(&d) {
                        list.push(d);
                    }
                }
                depends_on = Some(list);
            }
            let patch = TodoPatch { depends_on, title: a.title, description: read_description(a.description, a.description_file)?, status: a.status.as_deref().map(parse_todo_status).transpose()?, labels, project: a.project };
            c.apply(TodoOp::Update { id: id.clone(), patch })?;
            show_after(c, &id, json)
        }
        TodoCmd::Start { id } => {
            let store = c.todos()?;
            let id = resolve_todo(&store, &id)?;
            warn_blocked(&store, &id);
            c.apply(TodoOp::Update { id: id.clone(), patch: TodoPatch { status: Some(TodoStatus::InProgress), ..Default::default() } })?;
            // Linking `me` and `.` is best effort: a shell outside any session still starts the todo.
            let me = find_me(c)?.0;
            let here = std::env::current_dir().ok().and_then(|d| lane_for_path(c.board().ok()?, &d).map(|l| l.id.clone()));
            link(c, &id, here.as_deref(), me.as_deref(), true)?;
            show_after(c, &id, json)
        }
        TodoCmd::Move(a) => {
            let store = c.todos()?;
            let id = resolve_todo(&store, &a.id)?;
            let mut ids: Vec<String> = store.todos.iter().map(|t| t.id.clone()).filter(|x| x != &id).collect();
            let at = match (a.top, a.bottom, &a.before, &a.after) {
                (true, ..) => 0,
                (_, true, ..) => ids.len(),
                (_, _, Some(other), _) | (_, _, _, Some(other)) => {
                    let other = resolve_todo(&store, other)?;
                    let i = ids.iter().position(|x| x == &other).ok_or_else(|| anyhow!("A todo can't move next to itself."))?;
                    if a.after.is_some() { i + 1 } else { i }
                }
                _ => bail!("Give --top, --bottom, --before or --after."),
            };
            ids.insert(at, id.clone());
            c.apply(TodoOp::Order { ids })?;
            show_after(c, &id, json)
        }
        TodoCmd::Done { id } => set_status(c, &id, TodoStatus::Done, json),
        TodoCmd::Reopen { id } => set_status(c, &id, TodoStatus::Todo, json),
        TodoCmd::Link(a) => {
            let store = c.todos()?;
            let id = resolve_todo(&store, &a.id)?;
            if a.session.is_none() && a.worktree.is_none() {
                bail!("Give --session or --worktree.");
            }
            link(c, &id, a.worktree.as_deref(), a.session.as_deref(), true)?;
            show_after(c, &id, json)
        }
        TodoCmd::Unlink(a) => {
            let store = c.todos()?;
            let id = resolve_todo(&store, &a.id)?;
            if a.session.is_none() && a.worktree.is_none() {
                bail!("Give --session or --worktree.");
            }
            link(c, &id, a.worktree.as_deref(), a.session.as_deref(), false)?;
            show_after(c, &id, json)
        }
        TodoCmd::Delete { id } => {
            let store = c.todos()?;
            let id = resolve_todo(&store, &id)?;
            c.apply(TodoOp::Delete { id: id.clone() })?;
            if json {
                print_json(&serde_json::json!({ "deleted": id }))
            } else {
                println!("Deleted {id}.");
                Ok(())
            }
        }
        TodoCmd::Labels { cmd } => {
            let store = c.todos()?;
            match cmd {
                None => {}
                Some(LabelCmd::Add { name, color }) => {
                    let mut labels = store.labels.clone();
                    let name = one_line(&name, 40);
                    if labels.iter().any(|l| l.name.eq_ignore_ascii_case(&name)) {
                        bail!("There's a label called {name} already.");
                    }
                    let color = match color {
                        Some(c) if LABEL_COLORS.contains(&c.as_str()) => c,
                        Some(c) => bail!("Colors are {}, not {c}.", LABEL_COLORS.join(", ")),
                        None => {
                            let used: HashSet<&str> = labels.iter().map(|l| l.color.as_str()).collect();
                            LABEL_COLORS.iter().find(|c| !used.contains(*c)).unwrap_or(&LABEL_COLORS[labels.len() % LABEL_COLORS.len()]).to_string()
                        }
                    };
                    labels.push(Label { id: format!("l-{}", base36(now_ms() as u64)), name, color });
                    c.apply(TodoOp::Labels { labels })?;
                }
                Some(LabelCmd::Remove { name }) => {
                    let labels: Vec<Label> = store.labels.iter().filter(|l| !l.name.eq_ignore_ascii_case(&name)).cloned().collect();
                    if labels.len() == store.labels.len() {
                        bail!("There's no label called {name}.");
                    }
                    c.apply(TodoOp::Labels { labels })?;
                }
            }
            let labels = c.todos()?.labels;
            if json {
                return print_json(&labels);
            }
            for l in &labels {
                println!("{:<12} {:<8} {}", l.name, l.color, l.id);
            }
            Ok(())
        }
    }
}

/// A note on stderr, so JSON on stdout stays clean: starting a blocked todo is allowed, but
/// whoever starts it should know.
fn warn_blocked(store: &TodoStore, id: &str) {
    let Some(t) = store.todos.iter().find(|t| t.id == id) else { return };
    let open = store.blocked_by(t);
    if !open.is_empty() {
        let names: Vec<String> = open.iter().map(|x| format!("{} \"{}\" ({})", x.id, x.title, status_name(x.status))).collect();
        eprintln!("note: {} waits on {}, not done yet.", t.id, names.join(", "));
    }
}

fn set_status(c: &mut Client, id: &str, status: TodoStatus, json: bool) -> Result<()> {
    let store = c.todos()?;
    let id = resolve_todo(&store, id)?;
    c.apply(TodoOp::Update { id: id.clone(), patch: TodoPatch { status: Some(status), ..Default::default() } })?;
    show_after(c, &id, json)
}

/// Links (or unlinks) the worktree and session named, `.` and `me` included.
fn link(c: &mut Client, id: &str, worktree: Option<&str>, session: Option<&str>, linked: bool) -> Result<()> {
    if let Some(w) = worktree {
        let lane = resolve_worktree(c, w)?;
        c.apply(TodoOp::Link { id: id.into(), worktree: Some(lane), session: None, linked })?;
    }
    if let Some(s) = session {
        let key = resolve_session(c, s)?;
        c.apply(TodoOp::Link { id: id.into(), worktree: None, session: Some(key), linked })?;
    }
    Ok(())
}

fn show_after(c: &mut Client, id: &str, json: bool) -> Result<()> {
    let store = c.todos()?;
    let Some(t) = store.todos.iter().find(|t| t.id == id) else { bail!("The todo {id} isn't in the list any more.") };
    let row = todo_row(t, &store, true);
    if json {
        return print_json(&row);
    }
    print_todo(&row);
    Ok(())
}

fn print_todo(r: &TodoRow) {
    println!("{}  {}  [{}]{}", r.id, r.title, r.status, if r.project.is_empty() { String::new() } else { format!("  {}", r.project) });
    if !r.labels.is_empty() {
        println!("Labels: {}", r.labels.join(", "));
    }
    for w in &r.worktrees {
        println!("Worktree: {}", tilde(Path::new(w)));
    }
    for s in &r.sessions {
        println!("Session: {s}");
    }
    for w in r.waits_on.iter().flatten() {
        println!("Waits on: {} {} ({})", w.id, w.title, w.status);
    }
    for n in r.needed_by.iter().flatten() {
        println!("Needed by: {} {} ({})", n.id, n.title, n.status);
    }
    if let Some(d) = r.description.as_deref().filter(|d| !d.trim().is_empty()) {
        println!();
        println!("{}", d.trim_end());
    }
}

fn todo_list(c: &mut Client, a: &TodoListArgs, json: bool) -> Result<()> {
    let worktree = a.worktree.as_deref().map(|w| resolve_worktree(c, w)).transpose()?;
    let (me, here) = match a.mine {
        true => {
            let me = find_me(c)?.0;
            let here = std::env::current_dir().ok().and_then(|d| lane_for_path(c.board().ok()?, &d).map(|l| l.id.clone()));
            (me, here)
        }
        false => (None, None),
    };
    let store = c.todos()?;
    let label = a.label.as_deref().map(|n| store.labels.iter().find(|l| l.name.eq_ignore_ascii_case(n)).map(|l| l.id.clone()).ok_or_else(|| anyhow!("There's no label called {n}."))).transpose()?;
    let rows: Vec<TodoRow> = store
        .todos
        .iter()
        .filter(|t| match a.status.as_str() {
            "all" => true,
            "open" => t.status != TodoStatus::Done,
            s => parse_todo_status(s).map(|st| st == t.status).unwrap_or(false),
        })
        .filter(|t| a.project.as_deref().map_or(true, |p| p.eq_ignore_ascii_case(&t.project)))
        .filter(|t| label.as_deref().map_or(true, |l| t.labels.iter().any(|x| x == l)))
        .filter(|t| worktree.as_deref().map_or(true, |w| t.worktrees.iter().any(|x| x == w)))
        .filter(|t| !a.mine || me.as_deref().is_some_and(|k| t.sessions.iter().any(|s| s == k)) || here.as_deref().is_some_and(|w| t.worktrees.iter().any(|x| x == w)))
        .filter(|t| !a.ready || (t.status != TodoStatus::Done && store.blocked_by(t).is_empty()))
        .map(|t| todo_row(t, &store, false))
        .collect();
    if !["all", "open", "todo", "in-progress", "done"].contains(&a.status.as_str()) {
        bail!("--status is todo, in-progress, done, open or all.");
    }
    if json {
        return print_json(&rows);
    }
    let table: Vec<Vec<String>> = rows.iter().map(|r| vec![r.id.clone(), r.status.clone(), r.project.clone(), truncate(&r.title, 60), r.labels.join(", "), r.blocked_by.join(", "), r.worktrees.iter().map(|w| tilde(Path::new(w))).collect::<Vec<_>>().join(", ")]).collect();
    print!("{}", table_text(&["ID", "STATUS", "PROJECT", "TITLE", "LABELS", "WAITS ON", "WORKTREES"], table));
    Ok(())
}

// ---- start ----

fn start_cmd(c: &mut Client, a: &StartArgs, json: bool) -> Result<()> {
    let lane_id = resolve_worktree(c, &a.worktree)?;
    let (prompt, todo_id) = match (&a.todo, &a.prompt) {
        (Some(id), _) => {
            let store = c.todos()?;
            let id = resolve_todo(&store, id)?;
            let t = store.todos.iter().find(|t| t.id == id).unwrap();
            warn_blocked(&store, &id);
            let body = t.description.trim();
            (if body.is_empty() { t.title.trim().to_string() } else { format!("{}\n\n{body}", t.title.trim()) }, Some(id))
        }
        (None, Some(p)) => (p.clone(), None),
        (None, None) => bail!("Give --todo or --prompt."),
    };
    let place = match a.place {
        PlaceArg::Terminal => Place::Terminal,
        PlaceArg::ClaudeApp => Place::ClaudeApp,
        PlaceArg::CodexApp => Place::CodexApp,
    };
    let account = match a.account.as_str() {
        "auto" => {
            let cfg = c.cfg().clone();
            let usages = match c.app.is_some() {
                true => c.board()?.usage.clone(),
                false => Vec::new(),
            };
            pick_account(&cfg, &usages, place).ok_or_else(|| anyhow!("No account can start a session there. `codealong accounts` lists them."))?
        }
        name => name.to_string(),
    };
    let spec = StartSpec { lane_id, account, place, prompt, model: a.model.clone(), worktree: a.new_worktree, todo: todo_id };
    if a.dry_run {
        let cfg = c.cfg().clone();
        let account = cfg.accounts.iter().find(|x| x.name == spec.account).ok_or_else(|| anyhow!("There's no account named {}.", spec.account))?;
        let plan = start::plan(account, spec.place, Path::new(&spec.lane_id), &spec.prompt, &start::new_session_id(), spec.model.as_deref(), spec.worktree);
        if json {
            return print_json(&serde_json::json!({ "spec": spec, "plan": plan }));
        }
        println!("{}", serde_json::to_string_pretty(&plan)?);
        return Ok(());
    }
    let started = c.start(&spec)?;
    if json {
        return print_json(&started);
    }
    match (&started.key, &started.message) {
        (Some(k), _) => println!("Started {k}."),
        (None, Some(m)) => println!("{m}"),
        (None, None) => println!("Started."),
    }
    Ok(())
}

/// Auto's pick, simplified: among Claude accounts (the terminal and the Claude app), the one
/// with the most room by its tightest limit; without usage numbers, the first. Codex for its app.
fn pick_account(cfg: &Config, usages: &[crate::model::Usage], place: Place) -> Option<String> {
    let harness = match place {
        Place::CodexApp => "codex",
        _ => "claude",
    };
    let candidates: Vec<&AccountCfg> = cfg.accounts.iter().filter(|a| a.harness == harness).collect();
    let room = |a: &AccountCfg| usages.iter().find(|u| u.account == a.name).and_then(|u| u.meters.iter().map(|m| 100.0 - m.percent).fold(None, |acc: Option<f64>, x| Some(acc.map_or(x, |a| a.min(x))))).unwrap_or(50.0);
    candidates.iter().filter(|a| room(a) > 10.0).max_by(|a, b| room(a).partial_cmp(&room(b)).unwrap_or(std::cmp::Ordering::Equal)).or(candidates.first()).map(|a| a.name.clone())
}

// ---- config ----

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ConfigOut {
    #[serde(flatten)]
    view: config::ConfigView,
    /// Accounts found on this Mac that config.toml doesn't have yet.
    discovered: Vec<crate::detect::Found>,
}

fn config_view() -> ConfigOut {
    ConfigOut { view: config::view(), discovered: crate::detect::unlisted() }
}

fn config_cmd(cmd: ConfigCmd, json: bool) -> Result<()> {
    match cmd {
        ConfigCmd::Show => {
            let out = config_view();
            if json {
                return print_json(&out);
            }
            println!("# {}", tilde(Path::new(&out.view.path)));
            if let Some(e) = &out.view.error {
                println!("# Doesn't parse: {e}");
            }
            print!("{}", out.view.text);
            if !out.view.text.ends_with('\n') {
                println!();
            }
            for d in &out.discovered {
                let started = d.command.as_deref().map(|c| format!(", started with `{c}`")).unwrap_or_default();
                println!("# Found but not configured: {} ({} at {}{started})", d.name, d.harness, d.home);
            }
            Ok(())
        }
        ConfigCmd::Check { file } => {
            let (path, text) = match file {
                Some(f) => (expand(&f), std::fs::read_to_string(expand(&f)).with_context(|| format!("reading {f}"))?),
                None => (config::config_path(), std::fs::read_to_string(config::config_path()).unwrap_or_default()),
            };
            let result = config::parse(&text).map(|_| ()).map_err(|e| format!("{e:#}"));
            if json {
                return print_json(&serde_json::json!({ "path": path, "ok": result.is_ok(), "error": result.clone().err() })).and_then(|_| result.map_err(|e| anyhow!("{e}")));
            }
            match result {
                Ok(()) => println!("{} reads as settings.", tilde(&path)),
                Err(e) => bail!("{}: {e}", tilde(&path)),
            }
            Ok(())
        }
    }
}

// ---- install ----

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Installed {
    command: Option<String>,
    skills: Vec<SkillInstall>,
    warnings: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SkillInstall {
    account: String,
    path: String,
}

fn install(c: &mut Client, a: &InstallArgs, json: bool) -> Result<()> {
    let both = !a.command && !a.skill;
    let mut out = Installed { command: None, skills: Vec::new(), warnings: Vec::new() };
    if a.command || both {
        match crate::install::link_command(a.bin.as_deref().map(expand)) {
            Ok(p) => out.command = Some(p.to_string_lossy().into_owned()),
            Err(e) => out.warnings.push(format!("The command wasn't linked: {e:#}")),
        }
    }
    if a.skill || both {
        let cfg = c.cfg().clone();
        let accounts: Vec<&AccountCfg> = cfg.accounts.iter().filter(|x| a.account.is_empty() || a.account.contains(&x.name)).collect();
        if accounts.is_empty() {
            bail!("No account matches. `codealong accounts` lists them.");
        }
        let source = crate::install::skill_source().ok_or_else(|| anyhow!("The skill folder isn't next to this binary. Run from the app bundle, or from the repo."))?;
        for acct in accounts {
            match crate::install::skill_dir(acct) {
                // Installing for every account leaves an edited copy alone; naming the account replaces it.
                Some(dir) if a.account.is_empty() && crate::install::copy_state(&source, &dir).edited => {
                    out.warnings.push(format!("{}: {} was edited, so it was left alone; `codealong install --skill --account {}` replaces it.", acct.name, tilde(&dir), acct.name));
                }
                Some(dir) => match crate::install::copy_skill(&source, &dir) {
                    Ok(()) => out.skills.push(SkillInstall { account: acct.name.clone(), path: dir.to_string_lossy().into_owned() }),
                    Err(e) => out.warnings.push(format!("{}: {e:#}", acct.name)),
                },
                None => out.warnings.push(format!("{}: {} has no skills folder Codealong knows.", acct.name, acct.harness)),
            }
        }
    }
    if json {
        return print_json(&out);
    }
    if let Some(p) = &out.command {
        println!("Command: {}", tilde(Path::new(p)));
    }
    for s in &out.skills {
        println!("Skill for {}: {}", s.account, tilde(Path::new(&s.path)));
    }
    for w in &out.warnings {
        println!("! {w}");
    }
    Ok(())
}

// ---- shared bits ----

fn parse_since(s: &str) -> Result<Option<i64>> {
    if s == "all" {
        return Ok(None);
    }
    let (num, unit) = s.split_at(s.trim_end_matches(|c: char| c.is_ascii_alphabetic()).len());
    let n: i64 = num.parse().map_err(|_| anyhow!("--since is like 90m, 24h, 3d, or all, not {s}."))?;
    let ms = match unit {
        "m" => n * 60_000,
        "h" => n * 3_600_000,
        "d" => n * 86_400_000,
        "w" => n * 7 * 86_400_000,
        _ => bail!("--since is like 90m, 24h, 3d, or all, not {s}."),
    };
    Ok(Some(ms))
}

fn parse_status(s: &str) -> Result<Status> {
    serde_json::from_value(Value::String(s.to_lowercase())).map_err(|_| anyhow!("--status is needs, error, working, unread, idle or stopped, not {s}."))
}

fn lower_debug(v: &impl std::fmt::Debug) -> String {
    format!("{v:?}").to_lowercase()
}

/// `3m`, `2h`, `5d`; `now` under a minute.
fn ago(ms: i64) -> String {
    let s = ms.max(0) / 1000;
    match s {
        0..=59 => "now".into(),
        60..=3599 => format!("{}m", s / 60),
        3600..=86_399 => format!("{}h", s / 3600),
        _ => format!("{}d", s / 86_400),
    }
}

fn print_json(v: &impl Serialize) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(v)?);
    Ok(())
}

/// Columns padded to their widest cell, two spaces apart; the last column isn't padded.
fn table_text(head: &[&str], rows: Vec<Vec<String>>) -> String {
    let cols = head.len();
    let mut width = vec![0usize; cols];
    for row in std::iter::once(&head.iter().map(|h| h.to_string()).collect::<Vec<_>>()).chain(rows.iter()) {
        for (i, cell) in row.iter().enumerate().take(cols) {
            width[i] = width[i].max(cell.chars().count());
        }
    }
    let mut out = String::new();
    let line = |row: &[String], out: &mut String| {
        let mut parts = Vec::new();
        for (i, cell) in row.iter().enumerate().take(cols) {
            if i + 1 == cols {
                parts.push(cell.clone());
            } else {
                parts.push(format!("{cell:<w$}", w = width[i]));
            }
        }
        out.push_str(parts.join("  ").trim_end());
        out.push('\n');
    };
    line(&head.iter().map(|h| h.to_string()).collect::<Vec<_>>(), &mut out);
    if rows.is_empty() {
        out.push_str("(none)\n");
    }
    for r in &rows {
        line(r, &mut out);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn the_command_line_parses() {
        Cli::command().debug_assert();
        let cli = Cli::try_parse_from(["codealong", "todo", "add", "Fix the tests", "-l", "bug", "--worktree", "."]).unwrap();
        let Cmd::Todo { cmd: TodoCmd::Add(a) } = cli.cmd else { panic!() };
        assert_eq!((a.title.as_str(), a.label.as_slice(), a.worktree.as_deref()), ("Fix the tests", &["bug".to_string()][..], Some(".")));
        let cli = Cli::try_parse_from(["codealong", "sessions", "find", "osprey", "evidence", "--account", "claude2", "--since", "3d"]).unwrap();
        let Cmd::Sessions(s) = cli.cmd else { panic!() };
        let Some(SessionsCmd::Find { query, filter }) = s.cmd else { panic!() };
        assert_eq!((query.join(" "), filter.account.as_deref(), filter.since.as_str()), ("osprey evidence".into(), Some("claude2"), "3d"));
        let cli = Cli::try_parse_from(["codealong", "sessions", "--live", "--json"]).unwrap();
        assert!(cli.json && matches!(cli.cmd, Cmd::Sessions(SessionsArgs { cmd: None, .. })));
        assert!(Cli::try_parse_from(["codealong", "todo", "add"]).is_err());
        let cli = Cli::try_parse_from(["codealong", "todo", "move", "t-1", "--after", "t-2"]).unwrap();
        let Cmd::Todo { cmd: TodoCmd::Move(m) } = cli.cmd else { panic!() };
        assert_eq!((m.id.as_str(), m.after.as_deref(), m.top), ("t-1", Some("t-2"), false));
        assert!(Cli::try_parse_from(["codealong", "todo", "move", "t-1"]).is_err());
        assert!(Cli::try_parse_from(["codealong", "todo", "move", "t-1", "--top", "--bottom"]).is_err());
    }

    #[test]
    fn the_app_knows_its_own_subcommands() {
        for a in ["status", "todo", "sessions", "reference", "install", "config", "--help", "-V"] {
            assert!(is_cli_arg(a), "{a}");
        }
        for a in ["-psn_0_12345", "/Users/me/file.txt", "--snapshot", ""] {
            assert!(!is_cli_arg(a), "{a}");
        }
    }

    #[test]
    fn global_flags_before_the_subcommand_still_mean_the_command() {
        let line = |a: &[&str]| std::iter::once("codealong").chain(a.iter().copied()).map(String::from).collect::<Vec<_>>();
        assert!(is_cli(&line(&["--verbose", "todo", "add", "x"])));
        assert!(is_cli(&line(&["--json", "sessions"])));
        assert!(is_cli(&line(&["--table"])));
        assert!(is_cli(&line(&["todo", "list"])));
        assert!(!is_cli(&line(&[])));
        assert!(!is_cli(&line(&["-psn_0_12345"])));
        assert!(!is_cli(&line(&["/Users/me/file.txt"])));
        assert!(Cli::try_parse_from(line(&["--verbose", "todo", "list"])).is_ok());
    }

    #[test]
    fn since_and_status_read_plainly() {
        assert_eq!(parse_since("90m").unwrap(), Some(90 * 60_000));
        assert_eq!(parse_since("3d").unwrap(), Some(3 * 86_400_000));
        assert_eq!(parse_since("all").unwrap(), None);
        assert!(parse_since("soon").is_err());
        assert_eq!(parse_status("Needs").unwrap(), Status::Needs);
        assert!(parse_status("busy").is_err());
        assert_eq!(parse_todo_status("in_progress").unwrap(), TodoStatus::InProgress);
        assert_eq!(ago(30_000), "now");
        assert_eq!(ago(5 * 3_600_000), "5h");
    }

    #[test]
    fn ids_are_usable_and_prefixes_resolve() {
        let id = new_todo_id();
        assert!(id.starts_with("t-") && id.len() > 8 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'));
        let t = |id: &str| Todo { id: id.into(), project: String::new(), title: String::new(), description: String::new(), status: TodoStatus::Todo, labels: vec![], worktrees: vec![], sessions: vec![], created_at: 0, updated_at: 0, done_at: None, pending: vec![], depends_on: vec![] };
        let store = TodoStore { todos: vec![t("t-abc1"), t("t-abd2")], labels: vec![] };
        assert_eq!(resolve_todo(&store, "t-abc").unwrap(), "t-abc1");
        assert!(resolve_todo(&store, "t-ab").unwrap_err().to_string().contains("2 todos"));
        assert!(resolve_todo(&store, "zz").is_err());
        assert!(resolve_todo(&store, "").is_err() && resolve_todo(&store, " ").is_err());
    }

    #[test]
    fn the_skill_reference_is_current() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../skills/codealong/references/reference.md");
        let on_disk = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(on_disk == reference_text(), "skills/codealong/references/reference.md is out of date: run scripts/skill-reference.sh");
    }

    #[test]
    fn tables_line_up() {
        let text = table_text(&["A", "BB"], vec![vec!["x".into(), "y".into()], vec!["long".into(), "z".into()]]);
        assert_eq!(text, "A     BB\nx     y\nlong  z\n");
        assert!(table_text(&["A"], vec![]).contains("(none)"));
    }
}
