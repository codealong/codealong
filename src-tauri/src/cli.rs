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
use serde_json::{json, Value};

use crate::actions::{self, params};
use crate::api;
use crate::attachments;
use crate::board;
use crate::config::{self, AccountCfg, Config};
use crate::engine;
use crate::model::{Board, Lane, Origin, PreviewServer, Reuse, Session, Status};
use crate::procs;
use crate::search;
use crate::server::{self, Request};
use crate::start::{self, Place, StartSpec};
use crate::todos::{self, Label, Todo, TodoOp, TodoPatch, TodoStatus, TodoStore, Waiting, LABEL_COLORS};
use crate::usage;
use crate::util::{self, expand, now_ms, one_line, tilde, truncate};

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
    /// Which session runs this command, its worktree, the other agents running in that worktree now (`others`: none means no other agent shares the checkout, however many sessions worked there before), and its open todos: the ones linked to the session, then the worktree's when no other agent is running there (in a checkout several agents share, those are the others' work). Run it first.
    Whoami {
        /// Plain text for a session-start hook, even when piped: a line or two when a session or todo is found, nothing otherwise.
        #[arg(long)]
        context: bool,
    },
    /// Todos: work meant for an agent. Each has a title, a description, labels, a status (todo, in-progress, done) and the worktrees and sessions that took it on. The app lists open ones under In progress, Ready and Waiting.
    Todo {
        #[command(subcommand)]
        cmd: TodoCmd,
    },
    /// Sessions on the board: every agent, live or not, with its status and where it works. Lists by default; `find` searches.
    Sessions(SessionsArgs),
    /// One session: in full (`show`: status, excerpt, folder, branch, PR, how to resume it, and where its own records are), in the Codealong app (`open`), a message to it where it runs (`send`), what it waits on its user for (`pending`) and denying it (`answer`), stopping its turn (`stop`), naming it (`rename`) and forking it into the background (`fork`).
    Session {
        #[command(subcommand)]
        cmd: SessionCmd,
    },
    /// Worktrees the board knows: each with its branch, PR, status, and the agents running in it now (`agents`: sessions a process has open, or that are working or waiting on you; `working`: those working or waiting on you). `sessions` counts every session that worked there, running or not: it's history, not who's there.
    Worktrees(WorktreesArgs),
    /// A PR and the sessions that worked on it, and linking a session to a PR by hand. A session is linked by itself when its records show it made, edited or commented on the PR (`gh pr create` and the like), or when it worked in a worktree on the PR's branch while the PR was open.
    Pr {
        #[command(subcommand)]
        cmd: PrCmd,
    },
    /// A worktree's app, to open in the browser: the URL set for it, the Chrome profile it opens in, and whether a server in its folder serves it now. Set the URL when you know where the worktree's app opens (you started its dev server, or the repo's instructions say where it's served): the worktree's row in the app links to it and shows whether it's running.
    Preview {
        #[command(subcommand)]
        cmd: PreviewCmd,
    },
    /// Whether a worktree is reusable: it takes one piece of work after another, where others are kept for one piece of work each. Jev suggests new work only in reusable worktrees. A main checkout, and a worktree that has taken new work three times or more (its reflog), are reusable unless marked otherwise. The app's worktree menu has the same switch (Reuse for new work).
    Reuse {
        #[command(subcommand)]
        cmd: ReuseCmd,
    },
    /// Start a session for a todo (or several, on one prompt) or on a prompt, the way the app's New session does, and link it to the todos.
    Start(StartArgs),
    /// Codealong's own settings (config.toml): show them as the app reads them, and check a file before or after editing it. Change settings by editing the file; the app watches it.
    Config {
        #[command(subcommand)]
        cmd: ConfigCmd,
    },
    /// Put the command on PATH, Codealong's plugin in each Claude Code account (the skill, the command for its Bash tool, and the hooks that let Codealong answer its permission prompts and questions) and the skill in each other account's skills folder, and the adapters Codealong runs sessions through.
    Install(InstallArgs),
    /// Your other devices (Settings › Devices): this device, the devices it's linked with, your devices on your tailnet that could be, and devices asking to link. Linking is how devices see each other's sessions; needs the app running and Devices on.
    Devices {
        #[command(subcommand)]
        cmd: Option<DevicesCmd>,
    },
    /// The reference for the skill: every subcommand's help, as Markdown.
    #[command(hide = true)]
    Reference,
}

#[derive(Subcommand, Debug)]
enum DevicesCmd {
    /// Ask one of your devices on your tailnet to link, by name or short id. Prints the six digits both devices show; then say yes on that device (its app, or `codealong devices answer` there).
    Link { device: String },
    /// Say yes to a device asking to link, by name or short id, once it shows the same six digits (--no to say no).
    Answer {
        device: String,
        #[arg(long)]
        no: bool,
    },
    /// Unlink a device, here and on it.
    Unlink { device: String },
    /// A link code for a device that isn't on your tailnet; enter it there with `use-code`. It works once, for ten minutes.
    Code,
    /// Link with the device that made this link code.
    UseCode { code: String },
    /// Run a request on a linked device as this device's windows do (session-page, session-newer, session-pending, search, session-answer, mark-seen, start-options), with its parameters as JSON, less what the command can't ask for (a prompt, which would arrive there as its user's own; an answer other than deny). For checking a link by hand.
    #[command(hide = true)]
    Ask { device: String, method: String, params: Option<String> },
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
    /// Mark a todo done, and link the session running this command to it: finishing a todo says you worked on it.
    Done { id: String },
    /// Put a done or in-progress todo back to todo.
    Reopen { id: String },
    /// Mark a todo as waiting, on something outside the list: a person's answer, a release, a date. It stays under Waiting until `todo ready`, until it's started, or until it's done.
    Wait(WaitArgs),
    /// Say a todo marked as waiting is ready. One that waits on todos that aren't done stays under Waiting until they are.
    Ready { id: String },
    /// Move a todo in the list: to the top or bottom, or before or after another. The app shows todos in this order within each section.
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
    /// Only todos under Ready: not started, not marked as waiting, and waiting on no todo that isn't done. Pick work from these.
    #[arg(long, conflicts_with = "waiting")]
    ready: bool,
    /// Only todos under Waiting: marked as waiting, or not started and waiting on a todo that isn't done.
    #[arg(long)]
    waiting: bool,
    /// Only todos due by this day, overdue ones included: a date (2026-10-03), today, tomorrow, a weekday, next week, or 3d / 2w from today.
    #[arg(long, value_name = "WHEN")]
    due_by: Option<String>,
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
    /// Link a session: a key, an id, or `me`. A linked session is one working on it, so this marks it in progress; leave it out when noting work for later.
    #[arg(long)]
    session: Option<String>,
    /// A todo this one waits on: it's blocked until that one is done. Repeatable.
    #[arg(long, value_name = "ID")]
    depends_on: Vec<String>,
    /// Mark it as waiting from the start, on this: a person's answer, a release, a date.
    #[arg(long, value_name = "WHAT")]
    wait_for: Option<String>,
    /// The day it's due: a date (2026-10-03), today, tomorrow, a weekday (the next one), next week, or 3d / 2w from today.
    #[arg(long, value_name = "WHEN")]
    due: Option<String>,
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
    /// todo, in-progress or done. in-progress links the session running this command and its worktree, as `todo start` does; done links the session, as `todo done` does.
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
    /// The day it's due: a date (2026-10-03), today, tomorrow, a weekday (the next one), next week, or 3d / 2w from today.
    #[arg(long, value_name = "WHEN", conflicts_with = "no_due")]
    due: Option<String>,
    /// Take its due date off.
    #[arg(long)]
    no_due: bool,
}

#[derive(Args, Debug)]
struct WaitArgs {
    id: String,
    /// What it waits for, in a few words. Keeps the note it has when left out.
    #[arg(long = "for", value_name = "WHAT")]
    what: Option<String>,
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
    /// Sessions whose title, excerpt, account, worktree, branch or PR contains the words (all of them, any case), or whose prompts and replies do, for sessions active in the last `[search] days`.
    Find {
        query: Vec<String>,
        #[command(flatten)]
        filter: SessionFilter,
    },
    /// Close the Ghostty, iTerm2 and Terminal tabs of sessions that sat idle and want nothing from you: idle or unread, quiet for --after, nothing left running in the background, and a last reply that asks nothing (Jev's read with `[jev] sessions` on, else no question mark at its end). A tab closes only when every session in it qualifies. Its agent's process ends; the conversation stays and resumes as usual. Lists what it would close with --dry-run; run that first.
    CloseIdle {
        /// How long a session has been quiet: 90m, 24h, 3d, 1w.
        #[arg(long, default_value = "24h")]
        after: String,
        /// Only say what would close and what stays open, and why.
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Subcommand, Debug)]
enum SessionCmd {
    /// One session in full.
    Show { key: String },
    /// Show a session in the Codealong app: its window comes forward on the session's row in Sessions, with its conversation in the panel beside it, as Show in Codealong does in the floating bar. Nothing is opened where the session runs (its terminal, the Claude or Codex app). Needs the app running.
    Open {
        /// The session: its key, its id, a unique start of its id, or `me`.
        key: String,
    },
    /// Send a message to another session where it runs, on any account or harness. It's never your user's word: the message starts by saying it's from another agent and not the user, which session sent it, and how to answer (`codealong session send <sender's key> "…"`), so an answer arrives in your session as a new message. Claude Code sessions get it on their socket for messages between sessions; one that skips permission prompts takes it at once only from a session that skips them too, and otherwise asks its user first (a Claude app session can't, so nothing is sent). Codex and OpenCode sessions get it as a prompt, typed into their terminal or posted to their service, and queued while they work. Nothing is sent to a session that isn't running, one waiting on its user to answer a prompt, or an Antigravity one.
    Send(SendArgs),
    /// What a session waits on its user for, when Codealong can see it (`pending`): the tool call it asks to run (`id`, which `session answer` names; `label`, the call in a line; `input`, all of it; `cannot`, why Codealong can't answer it, when it can't) or the questions it asks (`questions`); null when it waits on nothing Codealong can see. Codex records no approvals, so a Codex session never shows one.
    Pending {
        /// The session: its key, its id, a unique start of its id, or `me`.
        key: String,
    },
    /// Deny the tool call a session waits on its user to allow, by the id `session pending` shows, if it's still the one waiting: Claude Code ends the turn, as No at its prompt does, and an OpenCode session is told no. Only `deny`: letting a call through, or answering the questions a session asks, is its user's to do, in Codealong or where the session runs.
    Answer(AnswerArgs),
    /// Stop the turn a session is taking, as its Stop in Codealong does: the conversation stays, and the session waits for its next message. A command it had handed to the background may keep running. A Claude app or Antigravity session can't be stopped from outside; the error says so.
    Stop {
        /// The session: its key, its id, a unique start of its id, or `me`.
        key: String,
    },
    /// Name a session where its harness keeps names (Claude Code's /rename, a Codex thread's name, an OpenCode session's title), so the harness shows the name as Codealong does.
    Rename {
        /// The session: its key, its id, a unique start of its id, or `me`.
        key: String,
        name: String,
    },
    /// Fork a session into a copy that goes on in the background, as /fork in its box in Codealong does: a Claude Code background job, an OpenCode session, or a Codex thread Codealong runs (that one needs the app running). The session carries on unaware. Given a prompt, the copy starts on it; without one it waits for its first message. Prints the copy's key when its harness says it at once.
    Fork {
        /// The session: its key, its id, a unique start of its id, or `me`.
        key: String,
        /// What the copy starts on.
        prompt: Option<String>,
    },
    /// For the Codealong plugin's hooks, with the hook's input on stdin. `permission-request`
    /// hands Claude Code's permission request to the app, to be answered from Codealong as well
    /// as the prompt, and prints the answer for Claude Code. It prints nothing, and exits 0,
    /// when the app isn't running or nobody answers there.
    #[command(hide = true)]
    Hook { event: String },
}

#[derive(Args, Debug)]
struct SendArgs {
    /// The session: its key, its id, or a unique start of its id.
    key: String,
    /// The message. Or --message-file.
    #[arg(conflicts_with = "message_file")]
    message: Option<String>,
    /// Read the message from a file, or from stdin with `-`.
    #[arg(long, value_name = "FILE")]
    message_file: Option<String>,
    /// A file to send with it, such as a screenshot or a log: copied into Codealong's store, and the session gets its full path to open (Claude Code's Read tool shows images). Repeatable.
    #[arg(long, value_name = "FILE")]
    attach: Vec<String>,
}

#[derive(Args, Debug)]
struct AnswerArgs {
    /// The session: its key, its id, a unique start of its id, or `me`.
    key: String,
    /// The id of the call it waits on, as `session pending` shows it.
    id: String,
    /// What to answer: `deny`, the only answer the command gives.
    answer: AnswerArg,
}

#[derive(ValueEnum, Clone, Copy, Debug)]
enum AnswerArg {
    Deny,
}

#[derive(Subcommand, Debug)]
enum PrCmd {
    /// One PR: its state, branches, the worktrees on its branch and the sessions that worked on it. Needs the app, which reads GitHub.
    Show {
        /// The PR: its URL, owner/name#number, or a number (#123) in the repo of the current folder's worktree.
        pr: String,
    },
    /// Link a session to a PR it worked on that Codealong can't see by itself: a PR opened another way (the website, an API tool), or one worked on from a worktree that isn't on its branch.
    Link(PrLinkArgs),
    /// Take a session off a PR it's listed on but didn't work on. It stays off, however it matched, until linked again.
    Unlink(PrLinkArgs),
}

#[derive(Args, Debug)]
struct PrLinkArgs {
    /// The PR: its URL, owner/name#number, or a number (#123) in the repo of the current folder's worktree.
    pr: String,
    /// A session key, id, or `me`.
    #[arg(long, default_value = "me")]
    session: String,
}

#[derive(Subcommand, Debug)]
enum PreviewCmd {
    /// A worktree's preview: where it opens, whether that URL was set or found, the server serving it now, and the Chrome profile it opens in. `running` false means nothing listening in the worktree's folder serves it: its app isn't running.
    Show(PreviewAt),
    /// Set the URL a worktree's app opens at, or the Chrome profile it opens in. It's linked to the server listening in the worktree's folder: the one on --port, else the port a localhost URL names, else the one that serves a page. A project's `chrome_profile` in config.toml opens all its worktrees' previews in that profile; --chrome-profile sets one worktree's own.
    Set(PreviewSetArgs),
    /// Take a worktree's URL away. A page a server in its folder serves still shows, at localhost.
    Clear(PreviewAt),
}

#[derive(Subcommand, Debug)]
enum ReuseCmd {
    /// Whether a worktree is reusable, and what it holds: `free` (nothing uncommitted or unlanded and no agent at work: new work can start there), `open` (work under way), `review` (its PR is open) or `landed` (its PR merged; new work starts once it's back on the default branch).
    Show(ReuseAt),
    /// Make a worktree reusable: new work can start there once its work lands, and agents can join the work under way.
    On(ReuseAt),
    /// Keep a worktree for its own piece of work: Jev stops suggesting new work there.
    Off(ReuseAt),
}

#[derive(Args, Debug)]
struct ReuseAt {
    /// The worktree: its path, or `.` for the one the current folder is in.
    #[arg(long, default_value = ".")]
    worktree: String,
}

#[derive(Args, Debug)]
struct PreviewAt {
    /// The worktree: its path, or `.` for the one the current folder is in.
    #[arg(long, default_value = ".")]
    worktree: String,
}

#[derive(Args, Debug)]
struct PreviewSetArgs {
    /// Where the app opens: `https://feature.app.test`, `localhost:5173/admin`, or a port alone. Without a scheme, local names take http and others https. Leave it out to keep the URL (and port) already set and change only the Chrome profile.
    url: Option<String>,
    /// Which of the worktree's servers is the app, when more than one serves a page (a dev server and Storybook, say), or when a proxy's URL names none.
    #[arg(long)]
    port: Option<u16>,
    /// The Chrome profile it opens in, over its project's: the name Chrome's profile picker shows, the Google account signed in to it, or its folder (`Profile 2`). Kept when the URL changes.
    #[arg(long, value_name = "NAME")]
    chrome_profile: Option<String>,
    /// Open it in its project's Chrome profile again (or the default browser, when the project names none), not one of its own.
    #[arg(long, conflicts_with = "chrome_profile")]
    no_chrome_profile: bool,
    #[command(flatten)]
    at: PreviewAt,
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
    /// Only reusable ones (`codealong reuse`): where new work can start, or join the work under way.
    #[arg(long)]
    reusable: bool,
}

#[derive(Args, Debug)]
struct StartArgs {
    /// The todo to start: its title and description become the prompt, and the session is linked to it. Give it more than once to start several todos in one session, on one prompt that has each of them.
    #[arg(long, conflicts_with = "prompt")]
    todo: Vec<String>,
    /// The prompt, when not starting a todo.
    #[arg(long)]
    prompt: Option<String>,
    /// The account to run it on. `auto` picks as the quick panel does: a preferred account with room for the model (config.toml's `preferred`), and among those the one whose weekly limit resets soonest.
    #[arg(long, default_value = "auto")]
    account: String,
    /// The worktree to run in: a path, or `.` (the default).
    #[arg(long, default_value = ".")]
    worktree: String,
    /// Have Claude Code make a fresh worktree for it (`claude --worktree`).
    #[arg(long)]
    new_worktree: bool,
    /// Where it runs. Without it, where the account's `start` setting says, else the terminal (the Codex app for Codex).
    #[arg(long = "in", value_enum)]
    place: Option<PlaceArg>,
    /// A model for a terminal or background start: fable, opus, sonnet, haiku, or a full name.
    #[arg(long)]
    model: Option<String>,
    /// How hard Claude Code thinks (`claude --effort`); without it, the account's own setting. xhigh is what Claude Code's picker calls Extra.
    #[arg(long, value_parser = clap::builder::PossibleValuesParser::new(start::effort_levels()))]
    effort: Option<String>,
    /// Print what the start would do (the command and environment, or the link) without starting anything.
    #[arg(long)]
    dry_run: bool,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq)]
enum PlaceArg {
    Terminal,
    /// Claude Code's background service, with no terminal until you attach one.
    Background,
    ClaudeApp,
    CodexApp,
    /// In Codealong itself, over the account's Agent Client Protocol adapter. Needs the app running.
    Codealong,
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
    /// Only Codealong for the accounts' agents: the plugin in a Claude Code account (replacing a skill copy Codealong put there before), the skill in the others' skills folders.
    #[arg(long, alias = "plugin")]
    skill: bool,
    /// Only the Agent Client Protocol adapters that let Codealong run Claude Code and Codex sessions itself (npm packages, in Codealong's folder).
    #[arg(long)]
    acp: bool,
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
    // A hook needs none of what the client reads, and must never fail Claude Code's prompt.
    if let Cmd::Session { cmd: SessionCmd::Hook { event } } = &cli.cmd {
        session_hook(event);
        return 0;
    }
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
    /// The session running this command and how it was found, once `find_me` has looked.
    me: Option<(Option<String>, Option<&'static str>)>,
}

impl Client {
    fn new(verbose: bool) -> Client {
        let app = match server::ask(&Request::Status) {
            Some(Ok(v)) => serde_json::from_value(v).ok(),
            _ => None,
        };
        Client { app, verbose, cfg: None, board: None, said: false, me: None }
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
            self.cfg = Some(config::load_or_default());
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

    /// Session key → a line of what was said, for sessions whose prompts and replies have
    /// every word: the app's index, or one built from the files now.
    fn search_text(&mut self, words: &[String]) -> HashMap<String, String> {
        let from_app = match self.app.is_some() {
            true => match server::ask(&Request::Search { words: words.to_vec() }) {
                Some(Ok(v)) => serde_json::from_value::<search::Found>(v).ok(),
                _ => None,
            },
            false => None,
        };
        let found = from_app.unwrap_or_else(|| {
            let cfg = self.cfg().clone();
            search::find_now(search::Scope { accounts: cfg.accounts, days: cfg.search.days }, words, 1000)
        });
        found.hits.into_iter().map(|h| (h.key, h.snippet)).collect()
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
                // Only a link looks a worktree up, and the board takes a moment to read.
                let lanes = match needs_lanes {
                    true => todos::LaneIndex::of(&self.board()?.lanes),
                    false => todos::LaneIndex::default(),
                };
                let now = now_ms();
                todos::change(|store| Ok((store.apply(op, &lanes, now)?, ()))).map(|(_, changed, _)| changed)
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
        if spec.place == Place::Codealong {
            // The session would run in this command's process and end with it.
            bail!("Starting a session in Codealong needs the app running.");
        }
        let cfg = self.cfg().clone();
        let board = self.board()?;
        // No engine to hand the todos' start to: it's written to todos.json from here, under
        // its lock.
        let started = start::start_and_link(board, &cfg, spec, |link| {
            let lanes = todos::LaneIndex::of(&board.lanes);
            let now = now_ms();
            todos::change(|store| {
                let mut changed = false;
                for id in &link.ids {
                    changed |= store.started(id, link.lane.as_deref(), link.session.as_deref(), link.pending.clone(), &lanes, now);
                }
                Ok((changed, ()))
            })?;
            Ok(())
        })?;
        if !spec.todos.is_empty() {
            self.board = None;
        }
        Ok(started)
    }

    /// One of the app's requests about the session `key` (actions.rs), asked as the command
    /// (`actions::From::Command`, with `from` the session running it when that's been looked
    /// for): through the app, which does what its windows do, or from here over the board when
    /// it's closed or older than the command.
    fn act(&mut self, name: &str, key: &str, params: impl Serialize, from: Option<String>) -> Result<Value> {
        self.note();
        let params = serde_json::to_value(params)?;
        if self.app.is_some() {
            match server::ask(&Request::Run { name: name.to_string(), key: key.to_string(), params: params.clone(), from: from.clone() }) {
                Some(Ok(v)) => return Ok(v),
                // An app from before `run` says it doesn't know the request.
                Some(Err(e)) if !e.starts_with(server::UNKNOWN_REQUEST) => bail!("{e}"),
                Some(Err(_)) => {}
                None => bail!("The app stopped answering."),
            }
        }
        let engine = engine::Handle::over(self.board()?.clone());
        actions::local(&engine, &actions::From::Command(from), name, key, params).map_err(|e| anyhow!(e))
    }

    /// Brings the app's main window forward on a session, worktree, PR or todo (`Reveal` in
    /// api.ts). The app shows it, then this opens the app by its bundle ID: macOS may not let an
    /// app come forward by itself when nobody clicked in it, and an open from here is the doing of
    /// the app this command runs in (the terminal, or the agent's app). An open by ID goes to the
    /// copy that's running, where one by path could start a newer build of it.
    fn show(&mut self, reveal: Value) -> Result<()> {
        self.note();
        let id = match server::ask(&Request::Show { reveal }) {
            Some(Ok(v)) => v.as_str().map(str::to_string),
            Some(Err(e)) if e.starts_with(server::UNKNOWN_REQUEST) => bail!("The Codealong that's running is older than this command and can't show a session. Update it, or quit it and open it again."),
            Some(Err(e)) => bail!("{e}"),
            None => bail!("Codealong isn't running, so there's no window to show it in."),
        };
        if let Some(id) = id {
            let _ = procs::open(&["-b", &id], std::time::Duration::from_secs(10));
        }
        Ok(())
    }
}

fn run(cmd: Cmd, c: &mut Client, json: bool) -> Result<()> {
    match cmd {
        Cmd::Status => status(c, json),
        Cmd::Accounts { usage } => accounts(c, usage, json),
        Cmd::Whoami { context } => whoami(c, context, json),
        Cmd::Todo { cmd } => todo(cmd, c, json),
        Cmd::Sessions(args) => match args.cmd {
            Some(SessionsCmd::Find { query, filter }) => sessions(c, &filter, &query, json),
            Some(SessionsCmd::CloseIdle { after, dry_run }) => close_idle(c, &after, dry_run, json),
            None => sessions(c, &args.filter, &[], json),
        },
        Cmd::Session { cmd: SessionCmd::Show { key } } => session_show(c, &key, json),
        Cmd::Session { cmd: SessionCmd::Open { key } } => session_open(c, &key, json),
        Cmd::Session { cmd: SessionCmd::Send(args) } => session_send(c, args, json),
        Cmd::Session { cmd: SessionCmd::Pending { key } } => session_pending(c, &key, json),
        Cmd::Session { cmd: SessionCmd::Answer(a) } => {
            let answer = match a.answer {
                AnswerArg::Deny => api::Answer::Deny,
            };
            session_act(c, &a.key, api::SESSION_ANSWER, params::Answer { id: a.id, answer: Some(answer), ..Default::default() }, json)
        }
        Cmd::Session { cmd: SessionCmd::Stop { key } } => session_act(c, &key, api::SESSION_CANCEL, params::Session::default(), json),
        Cmd::Session { cmd: SessionCmd::Rename { key, name } } => session_act(c, &key, api::SESSION_RENAME, params::Rename { name, ..Default::default() }, json),
        Cmd::Session { cmd: SessionCmd::Fork { key, prompt } } => session_act(c, &key, api::SESSION_FORK, params::Fork { prompt: prompt.unwrap_or_default(), ..Default::default() }, json),
        Cmd::Session { cmd: SessionCmd::Hook { event } } => {
            session_hook(&event);
            Ok(())
        }
        Cmd::Worktrees(args) => worktrees(c, &args, json),
        Cmd::Pr { cmd: PrCmd::Show { pr } } => pr_show(c, &pr, json),
        Cmd::Pr { cmd: PrCmd::Link(a) } => pr_link(c, &a, true, json),
        Cmd::Pr { cmd: PrCmd::Unlink(a) } => pr_link(c, &a, false, json),
        Cmd::Preview { cmd: PreviewCmd::Show(a) } => preview_show(c, &a.worktree, json),
        Cmd::Preview { cmd: PreviewCmd::Set(a) } => {
            // What's left out stays: the URL set (and its port, unless one is given), and the worktree's own profile.
            let chrome_profile = if a.no_chrome_profile { Some(None) } else { a.chrome_profile.map(Some) };
            preview_set(c, &a.at.worktree, crate::previews::Change { url: a.url, port: a.port.map(Some), chrome_profile, ..Default::default() }, json)
        }
        Cmd::Preview { cmd: PreviewCmd::Clear(a) } => preview_set(c, &a.worktree, crate::previews::Change { clear: true, ..Default::default() }, json),
        Cmd::Reuse { cmd: ReuseCmd::Show(a) } => reuse_show(c, &a.worktree, json),
        Cmd::Reuse { cmd: ReuseCmd::On(a) } => reuse_set(c, &a.worktree, true, json),
        Cmd::Reuse { cmd: ReuseCmd::Off(a) } => reuse_set(c, &a.worktree, false, json),
        Cmd::Start(args) => start_cmd(c, &args, json),
        Cmd::Config { cmd } => config_cmd(cmd, json),
        Cmd::Install(args) => install(c, &args, json),
        Cmd::Devices { cmd } => devices_cmd(c, cmd, json),
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

// ---- your other devices ----

fn ask_app(req: &Request) -> Result<Value> {
    match server::ask(req) {
        Some(Ok(v)) => Ok(v),
        Some(Err(e)) => Err(anyhow!("{e}")),
        None => bail!("Devices need the Codealong app running on this device."),
    }
}

/// A list in the devices view.
#[derive(Clone, Copy)]
enum Listed {
    /// Linked Macs.
    Linked,
    Phones,
    /// Your devices on your tailnet that aren't linked.
    Found,
    /// Devices asking to link.
    Asks,
}

/// A device named on the command line, by name, short id or the start of its key, among the ones
/// in the view's `lists`. Its key and name.
fn which_device(view: &server::DevicesSummary, named: &str, lists: &[Listed]) -> Result<(String, String)> {
    let named = named.trim();
    let lower = named.to_lowercase();
    let one = |key: &str, short: &str, name: &str| (key.to_string(), short.to_string(), name.to_string());
    let all: Vec<(String, String, String)> = lists
        .iter()
        .flat_map(|list| -> Vec<(String, String, String)> {
            match list {
                Listed::Linked => view.devices.iter().map(|d| one(&d.device.key, &d.device.short, &d.device.name)).collect(),
                Listed::Phones => view.phones.iter().map(|d| one(&d.key, &d.short, &d.name)).collect(),
                Listed::Found => view.found.iter().map(|d| one(&d.key, &d.short, &d.name)).collect(),
                Listed::Asks => view.asks.iter().map(|d| one(&d.key, &d.short, &d.name)).collect(),
            }
        })
        .collect();
    let hits: Vec<&(String, String, String)> = all.iter().filter(|(k, short, name)| name.to_lowercase() == lower || short == named || (named.len() >= 4 && k.starts_with(named))).collect();
    match hits.as_slice() {
        [one] => Ok((one.0.clone(), one.2.clone())),
        [] => bail!("No device here matches {named:?}; `codealong devices` lists them."),
        _ => bail!("More than one device matches {named:?}; give its short id."),
    }
}

fn devices_cmd(c: &mut Client, cmd: Option<DevicesCmd>, json: bool) -> Result<()> {
    if c.app.is_none() {
        bail!("Devices need the Codealong app running on this device.");
    }
    let view: server::DevicesSummary = serde_json::from_value(ask_app(&Request::Devices)?).context("reading the app's devices")?;
    let said = |text: String, v: Value| -> Result<()> {
        if json {
            print_json(&v)
        } else {
            println!("{text}");
            Ok(())
        }
    };
    match cmd {
        None => devices_list(&view, json),
        Some(DevicesCmd::Link { device }) => {
            let (key, name) = which_device(&view, &device, &[Listed::Found])?;
            let code = ask_app(&Request::DeviceLink { key })?;
            let code = code.as_str().unwrap_or_default().to_string();
            said(format!("Asked {name}. Say yes there when it shows {code}."), json!({ "device": name, "code": code }))
        }
        Some(DevicesCmd::Answer { device, no }) => {
            let (key, name) = which_device(&view, &device, &[Listed::Asks])?;
            ask_app(&Request::DeviceAnswer { key, accept: !no })?;
            said(if no { format!("Said no to {name}.") } else { format!("Linked with {name}.") }, json!({ "device": name, "linked": !no }))
        }
        Some(DevicesCmd::Unlink { device }) => {
            let (key, name) = which_device(&view, &device, &[Listed::Linked, Listed::Phones])?;
            ask_app(&Request::DeviceUnlink { key })?;
            said(format!("Unlinked {name}."), json!({ "device": name, "linked": false }))
        }
        Some(DevicesCmd::Code) => {
            let code = ask_app(&Request::DeviceCode)?;
            let code = code.as_str().unwrap_or_default().to_string();
            said(format!("{code}\nEnter it on the other device within ten minutes: codealong devices use-code <code>, or Settings › Devices."), json!({ "code": code }))
        }
        Some(DevicesCmd::UseCode { code }) => {
            let name = ask_app(&Request::DeviceUseCode { code })?;
            let name = name.as_str().unwrap_or_default().to_string();
            said(format!("Linked with {name}."), json!({ "device": name, "linked": true }))
        }
        Some(DevicesCmd::Ask { device, method, params }) => {
            let (key, _) = which_device(&view, &device, &[Listed::Linked])?;
            let params: Value = match params {
                Some(p) => serde_json::from_str(&p).context("the parameters aren't JSON")?,
                None => json!({}),
            };
            let got = ask_app(&Request::DeviceAsk { device: key.chars().take(10).collect(), request: method, params })?;
            print_json(&got)
        }
    }
}

fn devices_list(view: &server::DevicesSummary, json: bool) -> Result<()> {
    if json {
        // As a JSON value, whose keys come out in order, as the app's reply's did.
        return print_json(&serde_json::to_value(view)?);
    }
    if !view.enabled {
        println!("Devices is off on this device: turn it on in Settings › Devices, or [devices] enabled = true in config.toml.");
        return Ok(());
    }
    let me = &view.me;
    let tailscale = match me.tailscale.as_str() {
        "running" => format!("on {}{}", me.tailnet.as_deref().unwrap_or_default(), me.address.as_deref().map(|a| format!(", listening at {a}")).unwrap_or_default()),
        "missing" => "not installed".into(),
        "stopped" => "not connected".into(),
        _ => "off".into(),
    };
    println!("This device: {} ({}). Tailscale: {tailscale}. iroh: {}. Remote control: {}.", me.name, me.short, me.iroh, if me.remote_control { "on" } else { "off" });
    if let Some(e) = &me.error {
        println!("{e}");
    }
    let mut rows = Vec::new();
    for d in &view.devices {
        let via = d.device.via.and_then(|v| serde_json::to_value(v).ok()).and_then(|v| v.as_str().map(String::from)).unwrap_or_default();
        let state = if d.device.online { format!("connected, {via}") } else { d.device.last_seen.map(|t| format!("last seen {} ago", ago(now_ms() - t))).unwrap_or_else(|| "not connected".into()) };
        rows.push(vec![d.device.name.clone(), d.device.short.clone(), state, if d.device.remote_control { "on".into() } else { "off".into() }, d.worktrees.to_string(), d.sessions.to_string(), d.needs.to_string()]);
    }
    // Phones only watch: no worktrees, and remote control is this device's to give, not theirs.
    for p in &view.phones {
        let kind = if p.kind == "tablet" { "tablet" } else { "phone" };
        let state = if p.online { format!("{kind}, connected") } else { p.last_seen.map(|t| format!("{kind}, last seen {} ago", ago(now_ms() - t))).unwrap_or_else(|| format!("{kind}, not connected")) };
        rows.push(vec![p.name.clone(), p.short.clone(), state, String::new(), String::new(), String::new(), String::new()]);
    }
    for f in &view.found {
        rows.push(vec![f.name.clone(), f.short.clone(), if f.asking { format!("asked to link, code {}", f.code) } else { "on your tailnet, not linked".into() }, String::new(), String::new(), String::new(), String::new()]);
    }
    for a in &view.asks {
        rows.push(vec![a.name.clone(), a.short.clone(), format!("asks to link, code {}", a.code), String::new(), String::new(), String::new(), String::new()]);
    }
    if rows.is_empty() {
        println!("No other devices yet.");
    } else {
        print!("{}", table_text(&["DEVICE", "ID", "STATE", "REMOTE CONTROL", "WORKTREES", "SESSIONS", "NEED YOU"], rows));
    }
    Ok(())
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
    /// A Claude Code account's plugin, which is what Codealong installs there in place of the skill.
    #[serde(skip_serializing_if = "Option::is_none")]
    plugin: Option<crate::install::PluginState>,
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
                .map(|a| crate::install::account_skill(source.as_deref(), a))
                .map(|s| SkillStatus { account: s.name, path: s.path, installed: s.installed, current: s.current, edited: s.edited, plugin: s.plugin })
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
    let shipped = crate::install::shipped_version();
    for s in &out.skills {
        match &s.plugin {
            Some(p) => println!("Plugin for {}: {}", s.account, plugin_words(p, shipped.as_deref(), s.installed.then_some(s.path.as_deref()).flatten())),
            None => println!("Skill for {}: {}", s.account, match (s.installed, s.current, s.edited, &s.path) { (true, _, true, Some(p)) => format!("edited at {}, so updates leave it alone", tilde(Path::new(p))), (true, true, _, Some(p)) => format!("installed at {}", tilde(Path::new(p))), (true, false, _, Some(p)) => format!("out of date at {}", tilde(Path::new(p))), (_, _, _, Some(_)) => "not installed".into(), _ => "no skills folder known".into() }),
        }
    }
    Ok(())
}

/// A Claude Code account's plugin in a few words, as Settings › Agents says it (`pluginState`
/// in AgentsSection.tsx): against the version this build carries, and with the skill copy
/// Codealong made there before the plugin, when it's still in the skills folder.
fn plugin_words(p: &crate::install::PluginState, shipped: Option<&str>, old_copy: Option<&str>) -> String {
    let state = match (&p.version, &p.from) {
        (None, _) => "not installed".to_string(),
        (Some(_), _) if p.disabled => "turned off".to_string(),
        (Some(v), Some(from)) => format!("{v}, installed from {from}"),
        (Some(v), None) if shipped.is_some_and(|s| s != v) => format!("{v}, out of date"),
        (Some(v), None) => format!("installed, {v}"),
    };
    match old_copy {
        Some(path) => format!("{state}; an older copy of the skill is still at {}", tilde(Path::new(path))),
        None => state,
    }
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
            false => usage::read_all(&cfg.accounts),
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
    /// Where a search found the words in what was said, when it did: a line of the message.
    #[serde(skip_serializing_if = "Option::is_none")]
    said: Option<String>,
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
        said: None,
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
    let now = now_ms();
    let words: Vec<String> = query.iter().flat_map(|q| q.split_whitespace()).map(str::to_lowercase).collect();
    let said = if words.is_empty() { HashMap::new() } else { c.search_text(&words) };
    let board = c.board()?;
    let mut found: Vec<(&Session, &Lane)> = all_sessions(board)
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
                    words.iter().all(|w| search::has_word(&hay, w, words.len())) || said.contains_key(&s.key)
                })
        })
        .collect();
    // Those that need you first, the longest wait first as on the bar; then the one you last
    // prompted.
    let waited = |s: &Session| board::waiting_since(s).map_or((1, 0), |t| (0, t));
    found.sort_by(|(a, _), (b, _)| waited(a).cmp(&waited(b)).then(b.last_prompt_at.cmp(&a.last_prompt_at)));
    found.truncate(f.limit);
    let rows: Vec<SessionRow> = found.into_iter().map(|(s, lane)| SessionRow { said: said.get(&s.key).cloned(), ..session_row(s, lane, &board.todos) }).collect();
    if json {
        return print_json(&rows);
    }
    let table: Vec<Vec<String>> = rows
        .iter()
        .map(|r| vec![r.status.clone(), r.account.clone(), ago(now - r.last_activity), r.worktree_short(), truncate(&r.title, 50), truncate(r.said.as_deref().or(r.excerpt.as_deref()).unwrap_or(""), 60), r.key.clone()])
        .collect();
    print!("{}", table_text(&["STATUS", "ACCOUNT", "AGO", "WORKTREE", "TITLE", "EXCERPT", "KEY"], table));
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CloseIdleOut {
    closed: Vec<crate::idle::Outcome>,
    kept: Vec<crate::idle::Kept>,
}

fn close_idle(c: &mut Client, after: &str, dry_run: bool, json: bool) -> Result<()> {
    use crate::idle;
    let after_ms = idle::parse_after(after).ok_or_else(|| anyhow!("--after is like 90m, 24h, 3d or 1w, not {after}."))?;
    c.note();
    let via_app = c.app.is_some();
    let plan: idle::Plan = match via_app {
        true => match server::ask(&Request::IdleTerminals { after_ms }) {
            Some(Ok(v)) => serde_json::from_value(v).context("reading the app's reply")?,
            Some(Err(e)) => bail!("{e}"),
            None => bail!("The app stopped answering."),
        },
        false => idle::plan_now(c.board()?, after_ms),
    };
    let now = now_ms();
    if dry_run {
        if json {
            return print_json(&plan);
        }
        if plan.close.is_empty() {
            println!("No terminal has sat idle for {after} with nothing asked of you.");
        } else {
            let rows = plan.close.iter().map(|i| vec![i.account.clone(), ago(now - i.last_activity), i.callsign.clone(), truncate(&i.title, 50), i.why.clone(), i.key.clone()]).collect();
            print!("{}", table_text(&["ACCOUNT", "IDLE", "WORKTREE", "TITLE", "WHY", "KEY"], rows));
        }
        print_kept(&plan.kept);
        return Ok(());
    }
    let keys: Vec<String> = plan.close.iter().map(|i| i.key.clone()).collect();
    let closed = match (keys.is_empty(), via_app) {
        (true, _) => Vec::new(),
        (false, true) => match server::ask(&Request::CloseIdle { keys, after_ms }) {
            Some(Ok(v)) => serde_json::from_value(v).context("reading the app's reply")?,
            Some(Err(e)) => bail!("{e}"),
            None => bail!("The app stopped answering."),
        },
        (false, false) => idle::close(c.board()?, &keys, after_ms),
    };
    if json {
        return print_json(&CloseIdleOut { closed, kept: plan.kept });
    }
    if closed.is_empty() {
        println!("No terminal has sat idle for {after} with nothing asked of you.");
    } else {
        let rows = closed.iter().map(|o| vec![if o.closed { "closed" } else { "kept" }.to_string(), truncate(&o.title, 50), o.message.clone(), o.key.clone()]).collect();
        print!("{}", table_text(&["RESULT", "TITLE", "MESSAGE", "KEY"], rows));
    }
    print_kept(&plan.kept);
    Ok(())
}

/// One line on the terminals that stay open, by reason.
fn print_kept(kept: &[crate::idle::Kept]) {
    if kept.is_empty() {
        return;
    }
    let mut why: Vec<(String, usize)> = Vec::new();
    for k in kept {
        // "active 3 hours ago" differs per session; count those together.
        let reason = if k.why.starts_with("active ") { "active recently".to_string() } else { k.why.clone() };
        match why.iter_mut().find(|(r, _)| *r == reason) {
            Some((_, n)) => *n += 1,
            None => why.push((reason, 1)),
        }
    }
    let parts: Vec<String> = why.iter().map(|(r, n)| format!("{n} {r}")).collect();
    println!("Stay open: {}.", parts.join(", "));
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
    /// PRs it worked on (`owner/name#number`), as `pr show` lists their sessions.
    prs: Vec<String>,
    /// The session it goes on as, when it moved to another account: messages go there.
    moved_to: Option<String>,
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
        prs: board.prs.iter().filter(|p| p.sessions.iter().any(|x| x.key == s.key)).map(|p| p.key.clone()).collect(),
        moved_to: crate::origins::current().moved_to(&s.key, |k| board.session(k).is_some()),
        row,
    };
    if json {
        return print_json(&out);
    }
    let r = &out.row;
    println!("{}  {} on {}{}", r.key, r.status, r.account, if r.live { " (live)" } else { "" });
    println!("Title: {}", r.title);
    if let Some(copy) = &out.moved_to {
        println!("Moved to: {copy} (send there)");
    }
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
    for p in &out.prs {
        println!("Worked on: {p}");
    }
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OpenOut {
    /// The session the app shows.
    shown: String,
    title: String,
    worktree: String,
}

fn session_open(c: &mut Client, key: &str, json: bool) -> Result<()> {
    if c.app.is_none() {
        bail!("Codealong isn't running, so there's no window to show a session in.");
    }
    let key = resolve_session(c, key)?;
    let board = c.board()?;
    let (s, lane) = all_sessions(board).into_iter().find(|(s, _)| s.key == key).ok_or_else(|| anyhow!("There's no session {key}."))?;
    let out = OpenOut { shown: s.key.clone(), title: session_row(s, lane, &board.todos).title, worktree: lane.path.clone() };
    let reveal = json!({ "kind": "session", "laneId": lane.id, "key": s.key });
    c.show(reveal)?;
    if json {
        return print_json(&out);
    }
    println!("Codealong shows {} ({}).", out.title, out.shown);
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SendOut {
    /// The session it went to.
    to: String,
    /// The session it says it's from: the one running this command, or null outside one.
    from: Option<String>,
    #[serde(flatten)]
    sent: crate::send::Sent,
}

fn session_send(c: &mut Client, a: SendArgs, json: bool) -> Result<()> {
    let to = resolve_session(c, &a.key)?;
    let from = find_me(c)?.0;
    if from.as_deref() == Some(to.as_str()) {
        bail!("{to} is the session running this command.");
    }
    let text = with_attachments(read_text(a.message, a.message_file)?.unwrap_or_default(), &a.attach)?;
    if text.trim().is_empty() {
        bail!("Give a message, --message-file or --attach.");
    }
    let sent = serde_json::from_value(c.act(api::SESSION_SEND, &to, params::Send { text, ..Default::default() }, from.clone())?).context("reading what the send said")?;
    if json {
        return print_json(&SendOut { to, from, sent });
    }
    println!("{}", sent.message);
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PendingOut {
    session: String,
    pending: Option<api::Pending>,
}

fn session_pending(c: &mut Client, key: &str, json: bool) -> Result<()> {
    let key = resolve_session(c, key)?;
    let pending = serde_json::from_value(c.act(api::SESSION_PENDING, &key, params::Session::default(), None)?).context("reading what it waits on")?;
    let out = PendingOut { session: key, pending };
    if json {
        return print_json(&out);
    }
    let Some(p) = out.pending else {
        println!("{} waits on nothing Codealong can see.", out.session);
        return Ok(());
    };
    for q in &p.questions {
        let choices: Vec<&str> = q.options.iter().map(|o| o.label.as_str()).collect();
        println!("Asks: {} ({})", q.question, choices.join(", "));
    }
    if p.questions.is_empty() {
        println!("Waits to run: {} (id {})", p.label, p.id);
    }
    if let Some(input) = p.input.as_deref().filter(|i| !i.trim().is_empty()) {
        println!("{input}");
    }
    if let Some(why) = &p.cannot {
        println!("Codealong can't answer it: {why}");
    }
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ActOut {
    session: String,
    /// What happened, in a sentence, when the action says.
    said: Option<String>,
    /// A fork's copy, when its harness says it at once.
    #[serde(skip_serializing_if = "Option::is_none")]
    copy: Option<String>,
}

/// Stops, renames, forks or denies, through the app's own action (actions.rs), and says what
/// happened.
fn session_act(c: &mut Client, key: &str, name: &str, params: impl Serialize, json: bool) -> Result<()> {
    let key = resolve_session(c, key)?;
    let got = c.act(name, &key, params, None)?;
    let (said, copy) = match got {
        Value::String(s) => (Some(s), None),
        // A fork says what it started (`start::Started`).
        Value::Object(o) => (o.get("message").and_then(Value::as_str).map(String::from), o.get("key").and_then(Value::as_str).map(String::from)),
        _ => (None, None),
    };
    let said = said.or_else(|| (name == api::SESSION_RENAME).then(|| "Renamed.".to_string()));
    let out = ActOut { session: key, said, copy };
    if json {
        return print_json(&out);
    }
    if let Some(said) = &out.said {
        println!("{said}");
    }
    if let Some(copy) = &out.copy {
        println!("The copy: {copy}");
    }
    Ok(())
}

// ---- PRs ----

/// `owner/name#number` for a PR given as its URL, that, or a number in the repo of the current
/// folder's worktree.
fn resolve_pr(c: &mut Client, pr: &str) -> Result<String> {
    let pr = pr.trim();
    if let Some(key) = crate::prlinks::from_url(pr) {
        return Ok(key);
    }
    if let Some((slug, n)) = pr.split_once('#') {
        if slug.contains('/') && !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()) {
            return Ok(pr.to_string());
        }
    }
    let n = pr.trim_start_matches('#');
    if n.is_empty() || !n.chars().all(|c| c.is_ascii_digit()) {
        bail!("{pr} isn't a PR. Give its URL, owner/name#number, or a number in this folder's repo.");
    }
    let cwd = std::env::current_dir()?;
    let board = c.board()?;
    let slug = lane_for_path(board, &cwd).and_then(|l| l.repo_slug.clone());
    slug.map(|s| format!("{s}#{n}")).ok_or_else(|| anyhow!("This folder isn't in a GitHub repo the board knows, so #{n} names no PR. Give owner/name#{n}."))
}

fn find_pr<'a>(board: &'a Board, key: &str) -> Option<&'a crate::model::PrItem> {
    board.prs.iter().find(|p| p.key.eq_ignore_ascii_case(key))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PrDetail<'a> {
    #[serde(flatten)]
    pr: &'a crate::model::PrItem,
    /// The sessions in `sessions`, in full.
    session_rows: Vec<SessionRow>,
}

fn pr_show(c: &mut Client, pr: &str, json: bool) -> Result<()> {
    let key = resolve_pr(c, pr)?;
    if c.app.is_none() {
        bail!("Codealong isn't running, and PRs come through it.");
    }
    let board = c.board()?;
    let p = find_pr(board, &key).ok_or_else(|| anyhow!("Codealong doesn't read {key}: its repo isn't in a project, or it's closed and older than GitHub's recent list."))?;
    let all = all_sessions(board);
    let rows: Vec<SessionRow> = p.sessions.iter().filter_map(|ps| all.iter().find(|(s, _)| s.key == ps.key)).map(|(s, l)| session_row(s, l, &board.todos)).collect();
    if json {
        return print_json(&PrDetail { pr: p, session_rows: rows });
    }
    println!("{} {} ({})", p.key, p.title, lower_debug(&p.state));
    println!("{} → {}  {}", p.head, p.base.as_deref().unwrap_or("?"), p.url);
    for l in &p.lanes {
        println!("Worktree: {}", tilde(Path::new(l)));
    }
    for r in &rows {
        println!("Session: {} {} ({}, {})", r.key, r.title, r.status, tilde(Path::new(&r.worktree)));
    }
    if rows.is_empty() {
        println!("No session is linked. `codealong pr link {} --session <key>` links one.", p.key);
    }
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PrLinkOut {
    pr: String,
    session: String,
    linked: bool,
    /// False when it was already so.
    changed: bool,
}

fn pr_link(c: &mut Client, a: &PrLinkArgs, linked: bool, json: bool) -> Result<()> {
    let key = resolve_pr(c, &a.pr)?;
    let session = resolve_session(c, &a.session)?;
    if c.app.is_some() && find_pr(c.board()?, &key).is_none() {
        eprintln!("Note: Codealong doesn't read {key} (its repo isn't in a project, or it's old), so the link won't show until it does.");
    }
    let mut changed = false;
    crate::prlinks::change(|store| {
        changed = store.set(&key, &session, linked, now_ms());
        changed
    })?;
    let out = PrLinkOut { pr: crate::prlinks::pr_key(&key), session, linked, changed };
    if json {
        return print_json(&out);
    }
    let what = if linked { "Linked" } else { "Unlinked" };
    println!("{what} {} {} {}{}", out.session, if linked { "to" } else { "from" }, key, if changed { "" } else { " (it already was)" });
    Ok(())
}

/// Where a session's own harness keeps it (`harness::Adapter::records`).
fn records_for(account: &AccountCfg, s: &Session) -> Vec<String> {
    crate::harness::adapter(s.harness).records(account, s).into_iter().map(|p| p.to_string_lossy().into_owned()).collect()
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
    /// Agents running in it now (`agents_in`).
    agents: usize,
    /// Of those, the ones working or waiting on you.
    working: usize,
    /// Every session that worked in it and isn't archived, running or not.
    sessions: usize,
    last_activity: i64,
    pinned: bool,
    /// Ids of todos linked to it.
    todos: Vec<String>,
    /// Where its app opens (`codealong preview`), and whether a server in its folder serves it now.
    preview: Option<String>,
    preview_running: Option<bool>,
    /// What it holds, when it's reusable (`codealong reuse`): free, open, review or landed. None
    /// for a worktree kept for one piece of work.
    reuse: Option<String>,
}

/// Whether a session is running now: a process has it open (a terminal, the Claude app, a
/// background job), or it's in a turn or waiting on you to go on. A long-lived worktree
/// gathers dozens of sessions that ended days ago; counted as its agents, they made it look
/// crowded to an agent deciding whether to share it.
fn running(s: &Session) -> bool {
    s.live || matches!(s.status, Status::Working | Status::Needs)
}

/// The agents running in a worktree: its own sessions (not visitors from other worktrees)
/// that are running and not archived.
fn agents_in(l: &Lane) -> impl Iterator<Item = &Session> {
    l.sessions.iter().filter(|s| s.visitor.is_none() && !s.archived && running(s))
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
        agents: agents_in(l).count(),
        working: agents_in(l).filter(|s| matches!(s.status, Status::Working | Status::Needs)).count(),
        sessions: l.sessions.iter().filter(|s| s.visitor.is_none() && !s.archived).count(),
        last_activity: l.last_activity,
        pinned: board.pinned_worktrees.contains(&l.id),
        todos: board.todos.iter().filter(|t| t.worktrees.contains(&l.id)).map(|t| t.id.clone()).collect(),
        preview: l.preview.as_ref().map(|p| p.url.clone()),
        preview_running: l.preview.as_ref().map(|p| p.server.is_some()),
        reuse: l.reuse.as_ref().map(lower_debug),
    }
}

fn worktrees(c: &mut Client, a: &WorktreesArgs, json: bool) -> Result<()> {
    let board = c.board()?;
    let rows: Vec<LaneRow> = board
        .lanes
        .iter()
        .filter(|l| (a.all || !l.hidden) && (!a.reusable || l.reuse.is_some()) && a.project.as_deref().map_or(true, |p| p.eq_ignore_ascii_case(&l.project)) && a.bay.as_deref().map_or(true, |b| b.eq_ignore_ascii_case(&lower_debug(&l.bay))))
        .map(|l| lane_row(l, board))
        .collect();
    if json {
        return print_json(&rows);
    }
    let now = now_ms();
    // Columns for reuse and previews only when a worktree has them.
    let reuse = rows.iter().any(|r| r.reuse.is_some());
    let previews = rows.iter().any(|r| r.preview.is_some());
    let table: Vec<Vec<String>> = rows
        .iter()
        .map(|r| {
            let mut row = vec![r.bay.clone(), r.callsign.clone(), r.branch.clone().unwrap_or_default(), r.pr.map(|n| format!("#{n} {}", r.pr_state.clone().unwrap_or_default())).unwrap_or_default(), r.agents.to_string()];
            if reuse {
                row.push(r.reuse.clone().unwrap_or_default());
            }
            row.extend([ago(now - r.last_activity), truncate(&r.headline, 60)]);
            if previews {
                row.push(r.preview.as_deref().map(|u| format!("{}{}", crate::previews::short_url(u), if r.preview_running == Some(true) { "" } else { " (not running)" })).unwrap_or_default());
            }
            row.push(tilde(Path::new(&r.id)));
            row
        })
        .collect();
    let mut head = vec!["BAY", "WORKTREE", "BRANCH", "PR", "AGENTS"];
    if reuse {
        head.push("REUSE");
    }
    head.extend(["AGO", "HEADLINE"]);
    if previews {
        head.push("PREVIEW");
    }
    head.push("PATH");
    print!("{}", table_text(&head, table));
    Ok(())
}

// ---- preview ----

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PreviewOut {
    worktree: String,
    /// Where it opens; none when no URL is set and nothing in the worktree serves a page.
    url: Option<String>,
    /// The URL was set by hand; false when it's a page found serving there.
    set: bool,
    /// The port set with the URL.
    port: Option<u16>,
    /// A server in the worktree's folder serves it now.
    running: bool,
    server: Option<PreviewServer>,
    /// The Chrome profile it opens in; none opens it in the default browser.
    chrome_profile: Option<String>,
    /// The profile was set for the worktree, not taken from its project's `chrome_profile`.
    chrome_profile_set: bool,
}

fn preview_show(c: &mut Client, worktree: &str, json: bool) -> Result<()> {
    let id = resolve_worktree(c, worktree)?;
    let lane = c.board()?.lanes.iter().find(|l| l.id == id).ok_or_else(|| anyhow!("That worktree just changed. Try again."))?;
    let p = lane.preview.as_ref();
    let out = PreviewOut {
        worktree: id.clone(),
        url: p.map(|p| p.url.clone()),
        set: p.is_some_and(|p| p.set),
        port: p.and_then(|p| p.port),
        running: p.is_some_and(|p| p.server.is_some()),
        server: p.and_then(|p| p.server.clone()),
        chrome_profile: p.and_then(|p| p.chrome_profile.clone()),
        chrome_profile_set: p.is_some_and(|p| p.chrome_profile_set),
    };
    if json {
        return print_json(&out);
    }
    let Some(p) = p else {
        println!("{} has no preview: no URL is set (`codealong preview set <url>`) and nothing in its folder serves a page.", tilde(Path::new(&id)));
        return Ok(());
    };
    println!("{}{}", p.url, if p.set { "" } else { " (found: a page served in the worktree)" });
    match &p.server {
        Some(s) => println!("Running: {} on port {}, pid {}, for {}{}", s.name, s.port, s.pid, ago(now_ms() - s.started_at), s.title.as_deref().map(|t| format!(" · {t}")).unwrap_or_default()),
        None => println!("Not running: nothing listening in {} serves it{}.", tilde(Path::new(&id)), p.port.map(|n| format!(" on port {n}")).unwrap_or_default()),
    }
    match &p.chrome_profile {
        Some(c) if p.chrome_profile_set => println!("Opens in Chrome's {c} profile, set for this worktree."),
        Some(c) => println!("Opens in Chrome's {c} profile, set for {} in config.toml.", lane.project),
        None => println!("Opens in the default browser."),
    }
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PreviewSetOut {
    worktree: String,
    url: Option<String>,
    port: Option<u16>,
    /// The worktree's own Chrome profile; none opens it in its project's, or the default browser.
    chrome_profile: Option<String>,
    /// False when it already was so.
    changed: bool,
}

/// Changes a worktree's URL, port and Chrome profile, or clears them (`previews::set`): through
/// the app when it's up, as its sheet does, else in the file, which the app watches.
fn preview_set(c: &mut Client, worktree: &str, change: crate::previews::Change, json: bool) -> Result<()> {
    let id = resolve_worktree(c, worktree)?;
    let set: crate::previews::Set = serde_json::from_value(c.act(actions::PREVIEW, &id, change, None)?).context("reading what the change did")?;
    let (e, changed) = (set.preview.as_ref(), set.changed);
    let out = PreviewSetOut { worktree: id.clone(), url: e.map(|e| e.url.clone()), port: e.and_then(|e| e.port), chrome_profile: e.and_then(|e| e.chrome_profile.clone()), changed };
    if json {
        return print_json(&out);
    }
    let at = tilde(Path::new(&out.worktree));
    let profile = out.chrome_profile.as_deref().map(|p| format!(", in Chrome's {p} profile")).unwrap_or_default();
    match (&out.url, changed) {
        (Some(u), true) => println!("{at} opens at {u}{}{profile}.", out.port.map(|p| format!(", served on port {p}")).unwrap_or_default()),
        (Some(u), false) => println!("{at} already opens at {u}{profile}."),
        (None, true) => println!("Took the preview URL off {at}."),
        (None, false) => println!("{at} had no preview URL."),
    }
    Ok(())
}

// ---- reuse ----

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReuseOut {
    worktree: String,
    reusable: bool,
    /// How it was marked, by the menu or `codealong reuse on|off`; none when it goes by the
    /// default (a main checkout is reusable, the rest aren't).
    marked: Option<bool>,
    /// What it holds, when it's reusable: free, open, review or landed.
    state: Option<String>,
    /// Not marked, but its branch has carried several merged PRs: it looks like one that's
    /// reused.
    looks_reusable: bool,
    /// False when it already was so (`on` and `off`).
    #[serde(skip_serializing_if = "Option::is_none")]
    changed: Option<bool>,
}

/// What a reusable worktree's state says, after its path.
fn reuse_words(state: Reuse) -> &'static str {
    match state {
        Reuse::Free => "free: nothing uncommitted or unlanded and no agent at work, so new work can start there",
        Reuse::Open => "has work under way: new work that belongs with it can join it",
        Reuse::Review => "has its PR in review",
        Reuse::Landed => "has its work landed: new work starts once it's back on the default branch",
    }
}

fn reuse_out(c: &mut Client, id: &str) -> Result<ReuseOut> {
    let marked = crate::reusable::load()?.get(id);
    let lane = c.board()?.lanes.iter().find(|l| l.id == id).ok_or_else(|| anyhow!("That worktree just changed. Try again."))?;
    Ok(ReuseOut { worktree: id.to_string(), reusable: lane.reuse.is_some(), marked, state: lane.reuse.as_ref().map(lower_debug), looks_reusable: lane.looks_reusable, changed: None })
}

fn print_reuse(out: &ReuseOut, lane: Option<&Lane>) {
    let at = tilde(Path::new(&out.worktree));
    match lane.and_then(|l| l.reuse) {
        Some(state) => {
            // Not marked, it's reusable by default: a main checkout, or one that has been reused.
            let why = match (out.marked, lane) {
                (Some(_), _) => String::new(),
                (None, Some(l)) if l.origin == Origin::Main => " (a main checkout is, unless marked otherwise)".into(),
                (None, l) => format!(" (it has taken new work {} times, so it is unless marked otherwise)", l.and_then(|l| l.git.as_ref()).and_then(|g| g.new_work_count).unwrap_or(0)),
            };
            println!("{at} is reusable{why} and {}.", reuse_words(state))
        }
        None if out.marked == Some(true) => println!("{at} is marked reusable, but only a repo's worktree in one of your projects can be."),
        None => println!("{at} is kept for its own piece of work: Jev doesn't suggest new work there."),
    }
    if out.looks_reusable {
        println!("Its branch has carried several merged PRs, as a reused worktree's does: `codealong reuse on` makes it reusable.");
    }
}

fn reuse_show(c: &mut Client, worktree: &str, json: bool) -> Result<()> {
    let id = resolve_worktree(c, worktree)?;
    let out = reuse_out(c, &id)?;
    if json {
        return print_json(&out);
    }
    print_reuse(&out, c.board()?.lanes.iter().find(|l| l.id == id));
    Ok(())
}

/// Marks a worktree reusable or not (`reusable::mark`): through the app when it's up, as its
/// menu does, else in the file, which the app watches.
fn reuse_set(c: &mut Client, worktree: &str, on: bool, json: bool) -> Result<()> {
    let id = resolve_worktree(c, worktree)?;
    let done = c.act(actions::REUSABLE, &id, params::Reusable { on }, None)?;
    let changed = done["changed"].as_bool().unwrap_or(false);
    // What it holds comes from the app, which reads the file once the watcher says: `reuse
    // show` has it then.
    if json {
        return print_json(&ReuseOut { worktree: id.clone(), reusable: on, marked: Some(on), state: None, looks_reusable: false, changed: Some(changed) });
    }
    let at = tilde(Path::new(&id));
    match (on, changed) {
        (true, true) => println!("{at} is reusable now: new work can start there once its work lands, and Jev suggests it."),
        (true, false) => println!("{at} was reusable already."),
        (false, true) => println!("{at} is kept for its own piece of work now: Jev stops suggesting new work there."),
        (false, false) => println!("{at} was kept for its own piece of work already."),
    }
    Ok(())
}

// ---- whoami ----

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Whoami {
    found: bool,
    /// `process` (a parent process is a session the board knows), `record` (a session's
    /// unanswered tool call runs this command) or `environment` (CODEALONG_SESSION was set by
    /// the app's start).
    how: Option<String>,
    session: Option<SessionRow>,
    worktree: Option<LaneRow>,
    /// The other agents running in the worktree now (`agents_in`), this session aside. None
    /// means no other agent shares the checkout, however many sessions worked there before.
    others: Vec<SessionRow>,
    /// Open todos linked to the session, then open ones linked to the worktree when no other
    /// agent is running there (`my_todos`).
    todos: Vec<TodoRow>,
}

/// The session running this command: the parent process that's a live session on the board,
/// else the session whose unanswered tool call runs this command (`running_this`), else
/// `CODEALONG_SESSION` from a start the app made, for a session the board hasn't shown yet.
/// The variable comes last and only for such a session: a terminal keeps it after its session
/// ends, and long-lived processes started from one (Claude Code's background service,
/// OpenCode's) pass whichever session's variable they had on to everything they run.
fn find_me(c: &mut Client) -> Result<(Option<String>, Option<&'static str>)> {
    if let Some(me) = &c.me {
        return Ok(me.clone());
    }
    // The nearest: a session started from another's shell (`claude -p` run by an agent) is inside it.
    let live: Vec<&Session> = all_sessions(c.board()?).into_iter().map(|(s, _)| s).filter(|s| s.live).collect();
    let by_process = procs::ancestors(std::process::id()).into_iter().find_map(|p| live.iter().find(|s| s.pid == Some(p))).map(|s| s.key.clone());
    let me = match by_process {
        Some(key) => (Some(key), Some("process")),
        None => match running_this(c)? {
            Some(key) => (Some(key), Some("record")),
            None => match std::env::var(start::SESSION_VAR) {
                Ok(key) if !key.trim().is_empty() && !all_sessions(c.board()?).iter().any(|(s, _)| s.key == key) => (Some(key), Some("environment")),
                _ => (None, None),
            },
        },
    };
    c.me = Some(me.clone());
    Ok(me)
}

/// How recently a session that isn't in a turn by the board's last look was active, for it to
/// be running this command anyway: the board can lag a turn that just started.
const RUNNING_MS: i64 = 5 * 60 * 1000;

/// The session whose unanswered tool call runs this command, when exactly one has such a call:
/// how a session is found when no process above this one is a session the board knows. The
/// Codex app runs every thread's commands under one process, and OpenCode runs its tools in its
/// service; each writes a call to its record before running it. A live session whose commands
/// are its own process's children isn't asked: that process isn't above this one. Otherwise an
/// agent that hands another the command in a prompt would match as well.
fn running_this(c: &mut Client) -> Result<Option<String>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cfg = c.cfg().clone();
    let now = now_ms();
    let hits: Vec<String> = all_sessions(c.board()?)
        .into_iter()
        .map(|(s, _)| s)
        .filter(|s| may_run_this(s, now))
        .filter(|s| {
            let Some(account) = cfg.accounts.iter().find(|a| a.name == s.account) else { return false };
            crate::harness::adapter(s.harness).open_calls(&crate::harness::Ctx { account }, s).iter().any(|call| runs_command(call, &args))
        })
        .map(|s| s.key.clone())
        .collect();
    Ok(match hits.as_slice() {
        [one] => Some(one.clone()),
        _ => None,
    })
}

/// Whether a session could be running this command although no process above it is the
/// session's: it's in a turn, or was active a moment ago, and its commands don't run under a
/// process of its own that the board knows.
fn may_run_this(s: &Session, now: i64) -> bool {
    let turn = matches!(s.status, Status::Working | Status::Needs) || now - s.last_activity < RUNNING_MS;
    !s.archived && turn && !(s.live && s.pid.is_some() && crate::harness::adapter(s.harness).runs_its_commands())
}

/// Whether a recorded tool call runs this command, given its arguments: `codealong` as a
/// command (not a folder such as `projects/codealong`) followed by the subcommand, past any
/// options, and every one-word argument (ids, options, values) somewhere in the call. Free
/// text is left out, since quoting and escapes change how it's written; the words may sit
/// apart, as in a loop over ids.
fn runs_command(call: &str, args: &[String]) -> bool {
    let words: Vec<&str> = args.iter().map(String::as_str).filter(|a| !a.is_empty() && a.is_ascii() && !a.contains(|c: char| c.is_whitespace() || "\"'`\\$".contains(c))).collect();
    let Some(sub) = words.iter().find(|w| !w.starts_with('-')) else { return false };
    let invoked = call.match_indices("codealong").any(|(i, m)| {
        // Past a quote that closed the command's path, escaped or not.
        let rest = call[i + m.len()..].trim_start_matches(['"', '\'', '\\']);
        let next = rest.split_whitespace().find(|w| !w.starts_with('-'));
        rest.starts_with(char::is_whitespace) && next.and_then(|w| w.strip_prefix(*sub)).is_some_and(|after| !after.starts_with(|c: char| c.is_ascii_alphanumeric() || c == '-' || c == '_'))
    });
    invoked && words.iter().all(|w| call.contains(w))
}

/// A hook of the Codealong plugin's (plugin/hooks/hooks.json). Whatever goes wrong it prints
/// nothing, so Claude Code's own prompt stays the way to answer.
fn session_hook(event: &str) {
    if event != "permission-request" {
        return;
    }
    let mut input = String::new();
    if std::io::stdin().take(16 << 20).read_to_string(&mut input).is_err() {
        return;
    }
    let Ok(hook) = serde_json::from_str::<Value>(&input) else { return };
    // Plans are left to the prompt: the app isn't asked about them.
    let Some((asked, record)) = crate::claude::asks::parse(&hook, now_ms()) else { return };
    // The session's account folder and process, from the environment Claude Code gives its
    // hooks and its registry: the app watches the registry for the prompt going away, and so
    // does the hook while no app has it. The app answers none for a session it has no process
    // of (one it runs itself asks through its adapter).
    let home = std::env::var_os("CLAUDE_CONFIG_DIR").map(std::path::PathBuf::from).unwrap_or_else(|| crate::util::home().join(".claude"));
    let Some(pid) = crate::claude::asks::session_process(&home, &asked.session) else { return };
    let watch = crate::claude::asks::Watch::new(home.clone(), pid, record);
    let ask = || server::ask_permission(&hook, pid, &home, asked.at, watch.from);
    let decision = crate::claude::asks::ask_until_replied(&asked, &watch, std::time::Duration::from_secs(1), ask);
    if let Some(out) = crate::claude::asks::hook_output(&decision) {
        println!("{out}");
    }
}

fn whoami(c: &mut Client, context: bool, json: bool) -> Result<()> {
    let (key, how) = find_me(c)?;
    let cwd = std::env::current_dir().ok();
    let board = c.board()?;
    let session = key.as_deref().and_then(|k| all_sessions(board).into_iter().find(|(s, _)| s.key == k));
    let lane = session.map(|(_, l)| l).or_else(|| cwd.as_deref().and_then(|d| lane_for_path(board, d)));
    let out = Whoami {
        found: session.is_some(),
        how: session.and(how).map(str::to_string),
        session: session.map(|(s, l)| session_row(s, l, &board.todos)),
        worktree: lane.map(|l| lane_row(l, board)),
        others: lane.map(|l| others_in(l, key.as_deref()).map(|s| session_row(s, l, &board.todos)).collect()).unwrap_or_default(),
        todos: my_todos(board, key.as_deref(), lane).into_iter().map(|t| todo_row(t, &store_of(board), false)).collect(),
    };
    // Before JSON: a hook reads the command's output through a pipe.
    if context {
        print!("{}", context_text(&out, key.as_deref()));
        return Ok(());
    }
    if json {
        return print_json(&out);
    }
    match &out.session {
        Some(s) => {
            let by = match out.how.as_deref() {
                Some("process") => "its process",
                Some("record") => "its own record of this command",
                _ => "CODEALONG_SESSION",
            };
            println!("Session {} ({} on {}, {}), found by {by}.", s.key, s.harness, s.account, s.status)
        }
        None => println!("No session of this shell is on the board.{}", if key.is_some() { " CODEALONG_SESSION names one the board doesn't show yet." } else { "" }),
    }
    if let Some(w) = &out.worktree {
        println!("Worktree {} ({}{}){}", tilde(Path::new(&w.id)), w.callsign, w.branch.as_deref().map(|b| format!(", {b}")).unwrap_or_default(), w.pr.map(|n| format!(", PR #{n}")).unwrap_or_default());
        println!("{}", others_text(&out.others, w.sessions.saturating_sub(w.agents)));
    }
    for t in &out.todos {
        let mine = key.as_deref().is_some_and(|k| t.sessions.iter().any(|s| s == k));
        println!("Todo {} {} ({}{})", t.id, t.title, status_text(t), if mine { "" } else { ", linked to the worktree" });
    }
    Ok(())
}

/// The agents running in a worktree other than the caller.
fn others_in<'a>(l: &'a Lane, key: Option<&'a str>) -> impl Iterator<Item = &'a Session> {
    agents_in(l).filter(move |s| Some(s.key.as_str()) != key)
}

/// `whoami`'s line under the worktree: the other agents running there, or that there are none
/// and how many sessions there have ended.
fn others_text(others: &[SessionRow], ended: usize) -> String {
    if others.is_empty() {
        let before = match ended {
            0 => String::new(),
            1 => " (1 earlier session has ended)".to_string(),
            n => format!(" ({n} earlier sessions have ended)"),
        };
        return format!("No other agent is running here{before}.");
    }
    others.iter().map(|s| format!("Also running here: {} ({} on {}, {}) {}", s.key, s.harness, s.account, s.status, s.title)).collect::<Vec<_>>().join("\n")
}

/// The todos `whoami` names: the open ones linked to the session, then the open ones linked to
/// its worktree when no other agent is running there. In a checkout several agents share, the
/// worktree's todos are the others' work. When the caller isn't known, one agent running there
/// may be the caller itself.
fn my_todos<'a>(board: &'a Board, key: Option<&str>, lane: Option<&Lane>) -> Vec<&'a Todo> {
    let open = |t: &&Todo| t.status != TodoStatus::Done;
    let mut out: Vec<&Todo> = board.todos.iter().filter(open).filter(|t| key.is_some_and(|k| t.sessions.iter().any(|s| s == k))).collect();
    let alone = lane.filter(|l| others_in(l, key).count() <= usize::from(key.is_none()));
    if let Some(l) = alone {
        let more: Vec<&Todo> = board.todos.iter().filter(open).filter(|t| t.worktrees.contains(&l.id) && !out.iter().any(|x| x.id == t.id)).collect();
        out.extend(more);
    }
    out
}

/// What `whoami --context` adds to a session's context as it starts: which session it is, and
/// its todos. `key` is the session's even when the board doesn't show it yet, as happens for
/// a session the app just started.
fn context_text(out: &Whoami, key: Option<&str>) -> String {
    let mut lines = Vec::new();
    if let Some(s) = &out.session {
        lines.push(format!("This is Codealong session {} ({} on {}), in worktree {}.", s.key, s.harness, s.account, tilde(Path::new(&s.worktree))));
    }
    for t in &out.todos {
        if key.is_some_and(|k| t.sessions.iter().any(|s| s == k)) {
            let done = if t.status == "in-progress" { format!(" When the work is merged or the user says it's done, mark it done with `codealong todo done {}`.", t.id) } else { String::new() };
            lines.push(format!("Todo {} \"{}\" is linked to this session ({}).{done}", t.id, t.title, status_text(t)));
        } else {
            lines.push(format!("Todo {} \"{}\" is linked to this worktree ({}). If you take it on, `codealong todo start {}` links this session to it.", t.id, t.title, status_text(t), t.id));
        }
    }
    lines.into_iter().map(|l| l + "\n").collect()
}

/// A todo's status for a line of text, with what it's marked as waiting for and when it's due.
fn status_text(r: &TodoRow) -> String {
    let status = match &r.waiting {
        Some(w) if w.note.is_empty() => format!("{}, marked as waiting", r.status),
        Some(w) => format!("{}, waiting for {}", r.status, w.note),
        None => r.status.clone(),
    };
    match &r.due {
        Some(due) => format!("{status}, due {} ({due})", todos::due_phrase(due, util::today_local())),
        None => status,
    }
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
    /// Marked as waiting: `note` says on what, `since` when. Null when it isn't.
    waiting: Option<Waiting>,
    /// Where the app lists it: in-progress, ready, waiting or done.
    section: &'static str,
    /// The day it's due, as 2026-10-03, and how many days off that is on this Mac's clock:
    /// 0 today, below 0 once it's past. Null when it has none.
    due: Option<String>,
    due_in_days: Option<i64>,
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
        waiting: t.waiting.clone(),
        section: section_of(store, t),
        due: t.due.clone(),
        due_in_days: t.due.as_deref().and_then(todos::due_day).map(|d| d - util::today_local()),
        created_at: t.created_at,
        updated_at: t.updated_at,
        done_at: t.done_at,
        waits_on: full.then(|| t.depends_on.iter().filter_map(|d| store.todos.iter().find(|x| &x.id == d)).map(todo_ref).collect()),
        needed_by: full.then(|| store.todos.iter().filter(|x| x.depends_on.contains(&t.id)).map(todo_ref).collect()),
        description: full.then(|| attachments::expand(&t.description, attachments::Style::Path)),
        attachments: full.then(|| attachments::referenced(&t.description).iter().filter_map(|p| attachments::locate(p)).map(|p| p.to_string_lossy().into_owned()).collect()),
    }
}

/// The app's section for a todo, as `todo list` filters and `section` in the JSON name it.
fn section_of(store: &TodoStore, t: &Todo) -> &'static str {
    match t.status {
        TodoStatus::Done => "done",
        _ if store.is_waiting(t) => "waiting",
        TodoStatus::InProgress => "in-progress",
        TodoStatus::Todo => "ready",
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

/// `text`, or the text of `file`, or of stdin when `file` is `-`.
fn read_text(text: Option<String>, file: Option<String>) -> Result<Option<String>> {
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

/// `text` with each file kept in Codealong's store and referred to on a line of its own after
/// it, as the app's windows attach them.
fn with_attachments(mut text: String, files: &[String]) -> Result<String> {
    for f in files {
        let kept = attachments::import(&expand(f))?;
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        text.push_str(&kept.reference());
        text.push('\n');
    }
    Ok(text)
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
            let description = with_attachments(read_text(a.description, a.description_file)?.unwrap_or_default(), &a.attach)?;
            let id = new_todo_id();
            let now = now_ms();
            let waiting = a.wait_for.map(|note| Waiting { note, since: now });
            let due = a.due.as_deref().map(|w| todos::due_from_words(w, util::today_local())).transpose()?;
            let todo = Todo { id: id.clone(), project: a.project.unwrap_or_default(), title: a.title, description, status: TodoStatus::Todo, labels, worktrees: vec![], sessions: vec![], created_at: now, updated_at: now, done_at: None, pending: vec![], depends_on, waiting, due };
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
            let status = a.status.as_deref().map(parse_todo_status).transpose()?;
            if status == Some(TodoStatus::InProgress) {
                warn_blocked(&store, &id, &[]);
            }
            let due = match (a.due.as_deref(), a.no_due) {
                (Some(w), _) => Some(Some(todos::due_from_words(w, util::today_local())?)),
                (None, true) => Some(None),
                (None, false) => None,
            };
            let patch = TodoPatch { depends_on, title: a.title, description: read_text(a.description, a.description_file)?, status, labels, project: a.project, waiting: None, due };
            c.apply(TodoOp::Update { id: id.clone(), patch })?;
            if let Some(s) = status {
                link_caller(c, &id, s, json);
            }
            show_after(c, &id, json)
        }
        TodoCmd::Start { id } => {
            let store = c.todos()?;
            let id = resolve_todo(&store, &id)?;
            warn_blocked(&store, &id, &[]);
            set_status(c, &id, TodoStatus::InProgress, json)
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
        TodoCmd::Wait(a) => {
            let store = c.todos()?;
            let id = resolve_todo(&store, &a.id)?;
            let t = store.todos.iter().find(|t| t.id == id).unwrap();
            if t.status == TodoStatus::Done {
                bail!("{id} is done; `codealong todo reopen {id}` first.");
            }
            let note = a.what.or_else(|| t.waiting.as_ref().map(|w| w.note.clone())).unwrap_or_default();
            c.apply(TodoOp::Update { id: id.clone(), patch: TodoPatch { waiting: Some(Some(Waiting { note, since: 0 })), ..Default::default() } })?;
            show_after(c, &id, json)
        }
        TodoCmd::Ready { id } => {
            let store = c.todos()?;
            let id = resolve_todo(&store, &id)?;
            c.apply(TodoOp::Update { id: id.clone(), patch: TodoPatch { waiting: Some(None), ..Default::default() } })?;
            let store = c.todos()?;
            if let Some(t) = store.todos.iter().find(|t| t.id == id).filter(|t| t.status == TodoStatus::Todo) {
                let open = store.blocked_by(t);
                if !open.is_empty() {
                    let names: Vec<String> = open.iter().map(|x| format!("{} \"{}\"", x.id, x.title)).collect();
                    eprintln!("note: {id} still waits on {}, so it stays under Waiting. `todo update {id} --remove-dependency <id>` takes one off.", names.join(", "));
                }
            }
            show_after(c, &id, json)
        }
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
/// Says what a todo being started still waits on, leaving out the todos started with it.
fn warn_blocked(store: &TodoStore, id: &str, with: &[String]) {
    let Some(t) = store.todos.iter().find(|t| t.id == id) else { return };
    let open: Vec<&Todo> = store.blocked_by(t).into_iter().filter(|x| !with.contains(&x.id)).collect();
    if !open.is_empty() {
        let names: Vec<String> = open.iter().map(|x| format!("{} \"{}\" ({})", x.id, x.title, status_name(x.status))).collect();
        eprintln!("note: {} waits on {}, not done yet.", t.id, names.join(", "));
    }
    if let Some(w) = &t.waiting {
        let what = if w.note.is_empty() { String::new() } else { format!(" for \"{}\"", w.note) };
        eprintln!("note: {} was marked as waiting{what}; starting it ends that.", t.id);
    }
}

fn set_status(c: &mut Client, id: &str, status: TodoStatus, json: bool) -> Result<()> {
    let store = c.todos()?;
    let id = resolve_todo(&store, id)?;
    c.apply(TodoOp::Update { id: id.clone(), patch: TodoPatch { status: Some(status), ..Default::default() } })?;
    link_caller(c, &id, status, json);
    show_after(c, &id, json)
}

/// Links the session running this command to a todo it just said it's working on or has
/// finished. In progress links the session and the worktree the command runs in; done links
/// only the session, since a finished todo is found through its sessions, and a worktree
/// several agents share would list it for all of them. Best effort, after the status has
/// changed: a shell outside any session still changes it, and a link that fails is a note,
/// not an error. An agent is told when its session wasn't found, so it can link one itself.
fn link_caller(c: &mut Client, id: &str, status: TodoStatus, json: bool) {
    if status == TodoStatus::Todo {
        return;
    }
    let mut links = Vec::new();
    if status == TodoStatus::InProgress {
        let here = std::env::current_dir().ok().and_then(|d| lane_for_path(c.board().ok()?, &d).map(|l| l.id.clone()));
        links.extend(here.map(|lane| TodoOp::Link { id: id.into(), worktree: Some(lane), session: None, linked: true }));
    }
    match find_me(c).ok().and_then(|(key, _)| key) {
        // Linked as found, not looked up: a session the app just started isn't on the board yet.
        Some(key) => links.push(TodoOp::Link { id: id.into(), worktree: None, session: Some(key), linked: true }),
        // Output that isn't a terminal means an agent or a script; a person at a terminal expects none.
        None if json => eprintln!("note: Codealong couldn't tell which session runs this command, so no session is linked to {id}. `codealong todo link {id} --session <key>` links one."),
        None => {}
    }
    for op in links {
        if let Err(e) = c.apply(op) {
            eprintln!("note: {id} changed, but linking it failed: {e:#}");
        }
    }
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
    if let Some(due) = &r.due {
        let late = r.status != "done" && r.due_in_days.is_some_and(|n| n < 0);
        println!("Due: {due} ({}{})", todos::due_phrase(due, util::today_local()), if late { ", overdue" } else { "" });
    }
    if let Some(w) = &r.waiting {
        let since = match w.since {
            0 => String::new(),
            at => match ago(now_ms() - at).as_str() {
                "now" => " (marked just now)".into(),
                a => format!(" (marked {a} ago)"),
            },
        };
        match w.note.as_str() {
            "" => println!("Waiting{since}"),
            note => println!("Waiting for: {note}{since}"),
        }
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
    let today = util::today_local();
    let due_by = a.due_by.as_deref().map(|w| todos::due_from_words(w, today)).transpose()?.and_then(|d| todos::due_day(&d));
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
        .filter(|t| !a.ready || store.is_ready(t))
        .filter(|t| !a.waiting || store.is_waiting(t))
        .filter(|t| due_by.map_or(true, |by| t.due.as_deref().and_then(todos::due_day).is_some_and(|d| d <= by)))
        .map(|t| todo_row(t, &store, false))
        .collect();
    if !["all", "open", "todo", "in-progress", "done"].contains(&a.status.as_str()) {
        bail!("--status is todo, in-progress, done, open or all.");
    }
    if json {
        return print_json(&rows);
    }
    // What each waits on: the todos not done yet, then what it was marked as waiting for.
    let waits = |r: &TodoRow| {
        let mut parts = r.blocked_by.clone();
        if let Some(w) = &r.waiting {
            parts.push(if w.note.is_empty() { "(marked)".into() } else { format!("\"{}\"", truncate(&w.note, 40)) });
        }
        parts.join(", ")
    };
    // A due column only when something's due, so lists without dates stay as narrow as they were.
    let dated = rows.iter().any(|r| r.due.is_some());
    let due = |r: &TodoRow| r.due.as_deref().map(|d| format!("{d} ({})", todos::due_phrase(d, today))).unwrap_or_default();
    let table: Vec<Vec<String>> = rows
        .iter()
        .map(|r| {
            let mut row = vec![r.id.clone(), r.status.clone(), r.project.clone(), truncate(&r.title, 60), r.labels.join(", "), waits(r)];
            if dated {
                row.push(due(r));
            }
            row.push(r.worktrees.iter().map(|w| tilde(Path::new(w))).collect::<Vec<_>>().join(", "));
            row
        })
        .collect();
    let head: &[&str] = if dated { &["ID", "STATUS", "PROJECT", "TITLE", "LABELS", "WAITS ON", "DUE", "WORKTREES"] } else { &["ID", "STATUS", "PROJECT", "TITLE", "LABELS", "WAITS ON", "WORKTREES"] };
    print!("{}", table_text(head, table));
    Ok(())
}

// ---- start ----

/// What a session started on a todo is asked: the title, the description, then which todo it
/// is, so the agent can mark it done. `todoPrompt` in `src/todos.ts` writes the same text.
fn todo_prompt(t: &Todo) -> String {
    let footer = format!("This is Codealong todo {id}. When the work is merged or I say it's done, mark it done with `codealong todo done {id}`.", id = t.id);
    [t.title.trim(), t.description.trim(), &footer].iter().filter(|s| !s.is_empty()).cloned().collect::<Vec<_>>().join("\n\n")
}

/// What one session started on several todos is asked: each one's title and description,
/// numbered, then which todos they are. One todo is `todo_prompt`'s. `todosPrompt` in
/// `src/todos.ts` writes the same text.
fn todos_prompt(ts: &[&Todo]) -> String {
    if let [t] = ts {
        return todo_prompt(t);
    }
    let mut parts = vec![format!("Work on these {} todos:", ts.len())];
    for (i, t) in ts.iter().enumerate() {
        let title = t.title.trim();
        parts.push(format!("{}. {}", i + 1, if title.is_empty() { "Untitled" } else { title }));
        if !t.description.trim().is_empty() {
            parts.push(t.description.trim().to_string());
        }
    }
    let ids: Vec<String> = ts.iter().enumerate().map(|(i, t)| format!("{} ({})", t.id, i + 1)).collect();
    let (last, rest) = ids.split_last().map(|(l, r)| (l.clone(), r.join(", "))).unwrap_or_default();
    parts.push(format!("These are Codealong todos {rest} and {last}. When the work on one is merged or I say it's done, mark it done with `codealong todo done <id>`."));
    parts.join("\n\n")
}

fn start_cmd(c: &mut Client, a: &StartArgs, json: bool) -> Result<()> {
    let lane_id = resolve_worktree(c, &a.worktree)?;
    let (prompt, todo_ids) = match (a.todo.as_slice(), &a.prompt) {
        ([], Some(p)) => (p.clone(), Vec::new()),
        ([], None) => bail!("Give --todo or --prompt."),
        (given, _) => {
            let store = c.todos()?;
            let mut ids: Vec<String> = Vec::new();
            for id in given {
                let id = resolve_todo(&store, id)?;
                if !ids.contains(&id) {
                    ids.push(id);
                }
            }
            for id in &ids {
                warn_blocked(&store, id, &ids);
            }
            let ts: Vec<&Todo> = ids.iter().filter_map(|id| store.todos.iter().find(|t| &t.id == id)).collect();
            (todos_prompt(&ts), ids)
        }
    };
    let place = a.place.map(|p| match p {
        PlaceArg::Terminal => Place::Terminal,
        PlaceArg::Background => Place::Background,
        PlaceArg::ClaudeApp => Place::ClaudeApp,
        PlaceArg::CodexApp => Place::CodexApp,
        PlaceArg::Codealong => Place::Codealong,
    });
    let account = match a.account.as_str() {
        "auto" => {
            let cfg = c.cfg().clone();
            let usages = match c.app.is_some() {
                true => c.board()?.usage.clone(),
                false => Vec::new(),
            };
            pick_account(&cfg, &usages, place, a.model.as_deref()).ok_or_else(|| anyhow!("No account can start a session there. `codealong accounts` lists them."))?
        }
        name => name.to_string(),
    };
    // Without --in, the account's own first place: the one its `start` names, else the usual.
    let place = match place {
        Some(p) => p,
        None => {
            let cfg = c.cfg().clone();
            let acct = cfg.accounts.iter().find(|x| x.name == account).ok_or_else(|| anyhow!("There's no account named {account}."))?;
            let places = start::ordered_places(acct, &crate::util::home()).map_err(|why| anyhow!(why))?;
            places.first().copied().ok_or_else(|| anyhow!("{account} can't start a session from here."))?
        }
    };
    // Run by an agent, the new session was started by that agent's.
    let from = find_me(c)?.0;
    let spec = StartSpec { lane_id, account, place, prompt, model: a.model.clone(), effort: a.effort.clone(), worktree: a.new_worktree, todos: todo_ids, from };
    if a.dry_run {
        let cfg = c.cfg().clone();
        let account = cfg.accounts.iter().find(|x| x.name == spec.account).ok_or_else(|| anyhow!("There's no account named {}.", spec.account))?;
        let plan = start::plan(account, spec.place, Path::new(&spec.lane_id), &spec.prompt, &start::new_session_id(), spec.model.as_deref(), spec.effort.as_deref(), spec.worktree);
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

/// Auto's account, by the quick panel's rule (`start::auto_pick`): among the accounts that can
/// start a session (in `place`, when given; on `model`, when given, for the accounts whose
/// command line takes it), preferred ones with room first, and among those the one whose week resets soonest.
fn pick_account(cfg: &Config, usages: &[crate::model::Usage], place: Option<Place>, model: Option<&str>) -> Option<String> {
    let home = crate::util::home();
    let startable: Vec<(&AccountCfg, Vec<Place>)> = cfg.accounts.iter().filter_map(|a| start::ordered_places(a, &home).ok().filter(|p| !p.is_empty()).map(|p| (a, p))).collect();
    let preferred = start::preferred_names(&startable.iter().map(|(a, _)| *a).collect::<Vec<_>>());
    let models: Vec<Option<String>> = startable.iter().map(|(a, _)| model.map(str::to_string).or_else(|| crate::harness::of(a).and_then(|h| h.default_model(a)))).collect();
    let candidates: Vec<start::Candidate> = startable
        .iter()
        .zip(&models)
        .filter(|((a, places), _)| place.is_none_or(|p| places.contains(&p)) && model.is_none_or(|m| crate::harness::of(a).is_some_and(|h| h.offers_model(a, m))))
        .map(|((a, _), m)| start::Candidate { name: &a.name, preferred: preferred.contains(&a.name.as_str()), model: m.as_deref() })
        .collect();
    start::auto_pick(&candidates, usages, crate::util::now_ms()).map(|p| p.account)
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
    plugins: Vec<PluginInstall>,
    skills: Vec<SkillInstall>,
    acp: Option<String>,
    warnings: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PluginInstall {
    account: String,
    version: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SkillInstall {
    account: String,
    path: String,
}

fn install(c: &mut Client, a: &InstallArgs, json: bool) -> Result<()> {
    let both = !a.command && !a.skill && !a.acp;
    let mut out = Installed { command: None, plugins: Vec::new(), skills: Vec::new(), acp: None, warnings: Vec::new() };
    if a.acp || both {
        match crate::acp::install_adapters() {
            Ok(done) => out.acp = Some(done),
            Err(e) => out.warnings.push(format!("{e:#}")),
        }
    }
    let accounts: Vec<String> = match a.skill || both {
        true => {
            let names: Vec<String> = c.cfg().accounts.iter().filter(|x| a.account.is_empty() || a.account.contains(&x.name)).map(|x| x.name.clone()).collect();
            if names.is_empty() {
                bail!("No account matches. `codealong accounts` lists them.");
            }
            if crate::install::skill_source().is_none() {
                bail!("The skill folder isn't next to this binary. Run from the app bundle, or from the repo.");
            }
            names
        }
        false => Vec::new(),
    };
    // Installing for every account leaves an edited skill copy alone; naming the account replaces it.
    let ask = crate::install::Ask { command: a.command || both, bin: a.bin.as_deref().map(expand), accounts, keep_edited: a.account.is_empty() };
    let done = crate::install::run(&ask);
    out.command = done.command.map(|p| p.to_string_lossy().into_owned());
    out.warnings.extend(done.errors);
    let status = crate::install::status();
    for name in &done.accounts {
        let Some(acct) = status.accounts.iter().find(|s| &s.name == name) else { continue };
        match &acct.plugin {
            Some(plugin) => out.plugins.push(PluginInstall { account: name.clone(), version: plugin.version.clone() }),
            None => out.skills.push(SkillInstall { account: name.clone(), path: acct.path.clone().unwrap_or_default() }),
        }
    }
    if json {
        return print_json(&out);
    }
    if let Some(p) = &out.command {
        println!("Command: {}", tilde(Path::new(p)));
    }
    for p in &out.plugins {
        println!("Plugin for {}: codealong {}", p.account, p.version.as_deref().unwrap_or("(version unknown)"));
    }
    for s in &out.skills {
        println!("Skill for {}: {}", s.account, tilde(Path::new(&s.path)));
    }
    if let Some(done) = &out.acp {
        println!("{done}");
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
    use crate::fixtures::lane;

    #[test]
    fn status_says_a_claude_accounts_plugin() {
        use crate::install::PluginState;
        let p = |version: Option<&str>, disabled: bool, from: Option<&str>| PluginState { version: version.map(String::from), disabled, from: from.map(String::from) };
        assert_eq!(plugin_words(&p(None, false, None), Some("0.2.1+aaaa"), None), "not installed");
        assert_eq!(plugin_words(&p(Some("0.2.1+aaaa"), false, None), Some("0.2.1+aaaa"), None), "installed, 0.2.1+aaaa");
        assert_eq!(plugin_words(&p(Some("0.2.0+bbbb"), false, None), Some("0.2.1+aaaa"), None), "0.2.0+bbbb, out of date");
        assert_eq!(plugin_words(&p(Some("0.2.1+aaaa"), true, None), Some("0.2.1+aaaa"), None), "turned off");
        assert_eq!(plugin_words(&p(Some("0.2.1"), false, Some("someone/plugins")), Some("0.2.1+aaaa"), None), "0.2.1, installed from someone/plugins");
        // A plugin installed with no build to compare against isn't called out of date.
        assert_eq!(plugin_words(&p(Some("0.2.1+aaaa"), false, None), None, None), "installed, 0.2.1+aaaa");
        assert!(plugin_words(&p(Some("0.2.1+aaaa"), false, None), None, Some("/x/skills/codealong")).ends_with("; an older copy of the skill is still at /x/skills/codealong"));
    }
    use clap::CommandFactory;

    #[test]
    fn the_command_line_parses() {
        Cli::command().debug_assert();
        let cli = Cli::try_parse_from(["codealong", "todo", "add", "Fix the tests", "-l", "bug", "--worktree", "."]).unwrap();
        let Cmd::Todo { cmd: TodoCmd::Add(a) } = cli.cmd else { panic!() };
        assert_eq!((a.title.as_str(), a.label.as_slice(), a.worktree.as_deref()), ("Fix the tests", &["bug".to_string()][..], Some(".")));
        let cli = Cli::try_parse_from(["codealong", "sessions", "find", "vault", "evidence", "--account", "claude2", "--since", "3d"]).unwrap();
        let Cmd::Sessions(s) = cli.cmd else { panic!() };
        let Some(SessionsCmd::Find { query, filter }) = s.cmd else { panic!() };
        assert_eq!((query.join(" "), filter.account.as_deref(), filter.since.as_str()), ("vault evidence".into(), Some("claude2"), "3d"));
        let cli = Cli::try_parse_from(["codealong", "sessions", "--live", "--json"]).unwrap();
        assert!(cli.json && matches!(cli.cmd, Cmd::Sessions(SessionsArgs { cmd: None, .. })));
        assert!(Cli::try_parse_from(["codealong", "todo", "add"]).is_err());
        // The plugin's hook, under a subcommand every older app knows: one that doesn't know
        // `hook` fails with a usage error, where an unknown first word would start the app.
        let cli = Cli::try_parse_from(["codealong", "session", "hook", "permission-request"]).unwrap();
        assert!(matches!(cli.cmd, Cmd::Session { cmd: SessionCmd::Hook { ref event } } if event == "permission-request"));
        assert!(is_cli(&["codealong".into(), "session".into(), "hook".into()]));
        let cli = Cli::try_parse_from(["codealong", "todo", "move", "t-1", "--after", "t-2"]).unwrap();
        let Cmd::Todo { cmd: TodoCmd::Move(m) } = cli.cmd else { panic!() };
        assert_eq!((m.id.as_str(), m.after.as_deref(), m.top), ("t-1", Some("t-2"), false));
        assert!(Cli::try_parse_from(["codealong", "todo", "move", "t-1"]).is_err());
        assert!(Cli::try_parse_from(["codealong", "todo", "move", "t-1", "--top", "--bottom"]).is_err());
        let cli = Cli::try_parse_from(["codealong", "todo", "wait", "t-1", "--for", "Graham's letter"]).unwrap();
        let Cmd::Todo { cmd: TodoCmd::Wait(w) } = cli.cmd else { panic!() };
        assert_eq!((w.id.as_str(), w.what.as_deref()), ("t-1", Some("Graham's letter")));
        assert!(matches!(Cli::try_parse_from(["codealong", "todo", "ready", "t-1"]).unwrap().cmd, Cmd::Todo { cmd: TodoCmd::Ready { .. } }));
        assert!(Cli::try_parse_from(["codealong", "todo", "list", "--ready", "--waiting"]).is_err());
        let cli = Cli::try_parse_from(["codealong", "session", "send", "claude:claude2:abc", "Which test fails?", "--attach", "shot.png", "--attach", "run.log"]).unwrap();
        let Cmd::Session { cmd: SessionCmd::Send(s) } = cli.cmd else { panic!() };
        assert_eq!((s.key.as_str(), s.message.as_deref(), s.attach.len()), ("claude:claude2:abc", Some("Which test fails?"), 2));
        let cli = Cli::try_parse_from(["codealong", "session", "send", "abc", "--message-file", "-"]).unwrap();
        let Cmd::Session { cmd: SessionCmd::Send(s) } = cli.cmd else { panic!() };
        assert_eq!((s.message, s.message_file.as_deref()), (None, Some("-")));
        assert!(Cli::try_parse_from(["codealong", "session", "send", "abc", "hi", "--message-file", "m.txt"]).is_err());
        // What the command does to a session is what a window does, through the app's actions.
        let cli = Cli::try_parse_from(["codealong", "session", "answer", "abc", "toolu_1", "deny"]).unwrap();
        let Cmd::Session { cmd: SessionCmd::Answer(a) } = cli.cmd else { panic!() };
        assert_eq!((a.key.as_str(), a.id.as_str()), ("abc", "toolu_1"));
        // Allowing is its user's to do.
        assert!(Cli::try_parse_from(["codealong", "session", "answer", "abc", "toolu_1", "allow"]).is_err());
        assert!(matches!(Cli::try_parse_from(["codealong", "session", "stop", "me"]).unwrap().cmd, Cmd::Session { cmd: SessionCmd::Stop { ref key } } if key == "me"));
        assert!(matches!(Cli::try_parse_from(["codealong", "session", "pending", "abc"]).unwrap().cmd, Cmd::Session { cmd: SessionCmd::Pending { .. } }));
        let cli = Cli::try_parse_from(["codealong", "session", "rename", "abc", "Fix the flaky test"]).unwrap();
        assert!(matches!(cli.cmd, Cmd::Session { cmd: SessionCmd::Rename { ref name, .. } } if name == "Fix the flaky test"));
        assert!(Cli::try_parse_from(["codealong", "session", "rename", "abc"]).is_err());
        let cli = Cli::try_parse_from(["codealong", "session", "fork", "abc"]).unwrap();
        assert!(matches!(cli.cmd, Cmd::Session { cmd: SessionCmd::Fork { prompt: None, .. } }));
        let cli = Cli::try_parse_from(["codealong", "session", "fork", "abc", "Try it with the cache off"]).unwrap();
        assert!(matches!(cli.cmd, Cmd::Session { cmd: SessionCmd::Fork { prompt: Some(ref p), .. } } if p.starts_with("Try")));
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
        let t = |id: &str| Todo { id: id.into(), project: String::new(), title: String::new(), description: String::new(), status: TodoStatus::Todo, labels: vec![], worktrees: vec![], sessions: vec![], created_at: 0, updated_at: 0, done_at: None, pending: vec![], depends_on: vec![], waiting: None, due: None };
        let store = TodoStore { todos: vec![t("t-abc1"), t("t-abd2")], labels: vec![] };
        assert_eq!(resolve_todo(&store, "t-abc").unwrap(), "t-abc1");
        assert!(resolve_todo(&store, "t-ab").unwrap_err().to_string().contains("2 todos"));
        assert!(resolve_todo(&store, "zz").is_err());
        assert!(resolve_todo(&store, "").is_err() && resolve_todo(&store, " ").is_err());
    }

    #[test]
    fn several_todos_start_on_one_prompt() {
        let t = |id: &str, title: &str, description: &str| Todo { id: id.into(), project: String::new(), title: title.into(), description: description.into(), status: TodoStatus::Todo, labels: vec![], worktrees: vec![], sessions: vec![], created_at: 0, updated_at: 0, done_at: None, pending: vec![], depends_on: vec![], waiting: None, due: None };
        let (a, b, c) = (t("t-a", " Fix the header ", "\nIt wraps.\n"), t("t-b", "Add a logout button", ""), t("t-c", "", "No title."));
        // The same text as `todosPrompt`'s test in src/todos.test.ts.
        let footer = "When the work on one is merged or I say it's done, mark it done with `codealong todo done <id>`.";
        assert_eq!(todos_prompt(&[&a, &b]), format!("Work on these 2 todos:\n\n1. Fix the header\n\nIt wraps.\n\n2. Add a logout button\n\nThese are Codealong todos t-a (1) and t-b (2). {footer}"));
        assert!(todos_prompt(&[&a, &b, &c]).contains("3. Untitled\n\nNo title.\n\nThese are Codealong todos t-a (1), t-b (2) and t-c (3)."));
        assert_eq!(todos_prompt(&[&a]), todo_prompt(&a));
        let cli = Cli::try_parse_from(["codealong", "start", "--todo", "t-a", "--todo", "t-b"]).unwrap();
        let Cmd::Start(s) = cli.cmd else { panic!() };
        assert_eq!(s.todo, vec!["t-a".to_string(), "t-b".to_string()]);
        assert!(Cli::try_parse_from(["codealong", "start", "--todo", "t-a", "--prompt", "Fix it"]).is_err());
    }

    fn session(key: &str, live: bool, visitor: Option<&str>) -> Session {
        Session { live, status_since: Some(0), visitor: visitor.map(String::from), ..crate::fixtures::session(key) }
    }


    fn todo(id: &str, status: TodoStatus, sessions: &[&str], worktrees: &[&str]) -> Todo {
        let strings = |xs: &[&str]| xs.iter().map(|x| x.to_string()).collect();
        Todo { id: id.into(), project: String::new(), title: id.to_uppercase(), description: String::new(), status, labels: vec![], worktrees: strings(worktrees), sessions: strings(sessions), created_at: 0, updated_at: 0, done_at: None, pending: vec![], depends_on: vec![], waiting: None, due: None }
    }

    /// The main checkout, where this session and another live one work, and a worktree of its
    /// own for a second session, with an ended session and a live visitor.
    fn shared_and_own(other_live: bool) -> Board {
        let todos = vec![
            todo("a", TodoStatus::InProgress, &["me"], &["/r/main"]),
            todo("b", TodoStatus::Done, &["me"], &[]),
            todo("c", TodoStatus::InProgress, &["other"], &["/r/main"]),
            todo("d", TodoStatus::Todo, &[], &["/r/wt"]),
            todo("e", TodoStatus::Done, &["gone"], &["/r/wt"]),
        ];
        let lanes = vec![
            lane("/r/main", vec![session("me", true, None), session("other", other_live, None), session("old", false, None)]),
            lane("/r/wt", vec![session("me2", true, None), session("gone", false, None), session("other", true, Some("worked here"))]),
        ];
        Board { lanes, todos, ..Board::default() }
    }

    #[test]
    fn whoami_names_its_own_open_todos_and_a_worktrees_only_when_no_one_else_is_there() {
        let ids = |ts: Vec<&Todo>| ts.iter().map(|t| t.id.clone()).collect::<Vec<_>>();
        let b = shared_and_own(true);
        let (main, wt) = (&b.lanes[0], &b.lanes[1]);
        // Its done todo is left out, and so is the shared checkout's other todo.
        assert_eq!(ids(my_todos(&b, Some("me"), Some(main))), ["a"]);
        // A worktree of its own: ended sessions and visitors don't share it; done todos stay out.
        assert_eq!(ids(my_todos(&b, Some("me2"), Some(wt))), ["d"]);
        // A caller that isn't known may be the one live session there, but not one of two.
        assert_eq!(ids(my_todos(&b, None, Some(wt))), ["d"]);
        assert!(my_todos(&b, None, Some(main)).is_empty());
        assert!(my_todos(&b, None, None).is_empty());
        // Once the other session ends, the checkout's todos are this one's to see.
        let b = shared_and_own(false);
        assert_eq!(ids(my_todos(&b, Some("me"), Some(&b.lanes[0]))), ["a", "c"]);
    }

    #[test]
    fn a_worktrees_agents_are_the_sessions_running_in_it_not_every_one_that_worked_there() {
        let with = |key: &str, status: &str, live: bool| Session { status: serde_json::from_value(json!(status)).unwrap(), ..session(key, live, None) };
        let long_lived = |sessions: Vec<Session>| {
            let mut all = vec![with("me", "working", true), with("ended", "stopped", false), with("seen", "idle", false)];
            all.extend(sessions);
            // Put away, and a session from another worktree that worked here too.
            all.push(Session { archived: true, ..with("put-away", "idle", true) });
            all.push(session("visitor", true, Some("worked here")));
            Board { lanes: vec![lane("/r/long", all)], todos: vec![todo("t", TodoStatus::Todo, &[], &["/r/long"])], ..Board::default() }
        };
        // Alone in a checkout where sessions ended long ago: none of them is an agent there.
        let b = long_lived(vec![]);
        let l = &b.lanes[0];
        let row = lane_row(l, &b);
        assert_eq!((row.agents, row.working, row.sessions), (1, 1, 3));
        assert_eq!(others_in(l, Some("me")).count(), 0);
        assert_eq!(others_text(&[], row.sessions - row.agents), "No other agent is running here (2 earlier sessions have ended).");
        assert_eq!(others_text(&[], 0), "No other agent is running here.");
        assert_eq!(my_todos(&b, Some("me"), Some(l)).len(), 1, "the worktree's todo is this session's to see");
        // A terminal left open between turns, and a Codex thread waiting on an approval, whose
        // app has no process for it: both share the checkout.
        let b = long_lived(vec![with("open", "idle", true), with("asking", "needs", false)]);
        let l = &b.lanes[0];
        let row = lane_row(l, &b);
        assert_eq!((row.agents, row.working, row.sessions), (3, 2, 5));
        let others: Vec<SessionRow> = others_in(l, Some("me")).map(|s| session_row(s, l, &b.todos)).collect();
        assert_eq!(others_text(&others, row.sessions - row.agents), "Also running here: open (claude on claude, idle) Title open\nAlso running here: asking (claude on claude, needs) Title asking");
        assert!(my_todos(&b, Some("me"), Some(l)).is_empty(), "the worktree's todo may be theirs");
    }

    #[test]
    fn a_starting_session_is_told_which_todos_are_its_own() {
        let b = shared_and_own(false);
        let (main, store) = (&b.lanes[0], store_of(&b));
        let rows = |ids: &[&str]| ids.iter().map(|id| todo_row(b.todos.iter().find(|t| t.id == *id).unwrap(), &store, false)).collect::<Vec<_>>();
        let me = &main.sessions[0];
        let out = Whoami { found: true, how: Some("process".into()), session: Some(session_row(me, main, &b.todos)), worktree: Some(lane_row(main, &b)), others: vec![], todos: rows(&["a", "c"]) };
        assert_eq!(
            context_text(&out, Some("me")),
            "This is Codealong session me (claude on claude), in worktree /r/main.\n\
             Todo a \"A\" is linked to this session (in-progress). When the work is merged or the user says it's done, mark it done with `codealong todo done a`.\n\
             Todo c \"C\" is linked to this worktree (in-progress). If you take it on, `codealong todo start c` links this session to it.\n"
        );
        // A session the app just started isn't on the board yet, but its todo is its own.
        let out = Whoami { found: false, how: None, session: None, worktree: Some(lane_row(main, &b)), others: vec![], todos: rows(&["a"]) };
        assert!(context_text(&out, Some("me")).starts_with("Todo a \"A\" is linked to this session"));
        let out = Whoami { found: false, how: None, session: None, worktree: None, others: vec![], todos: vec![] };
        assert_eq!(context_text(&out, None), "");
    }

    #[test]
    fn only_sessions_without_a_process_of_their_own_above_this_one_are_asked() {
        let now = 10 * RUNNING_MS;
        let with = |harness: &str, status: &str, live: bool, pid: Option<u32>, active: i64| -> Session {
            let mut s = session("s", live, None);
            s.harness = serde_json::from_value(serde_json::json!(harness)).unwrap();
            s.status = serde_json::from_value(serde_json::json!(status)).unwrap();
            s.pid = pid;
            s.last_activity = active;
            s
        };
        // A Codex app thread in a turn has no process; OpenCode's tools run in its service,
        // whatever terminal shows the session.
        assert!(may_run_this(&with("codex", "working", true, None, 0), now));
        assert!(may_run_this(&with("opencode", "working", true, Some(42), 0), now));
        // Claude Code and a Codex terminal run commands under their own process, which the
        // process check already looked for.
        assert!(!may_run_this(&with("claude", "working", true, Some(42), now), now));
        assert!(!may_run_this(&with("codex", "working", true, Some(42), now), now));
        // Out of its turn and quiet for a while, or archived, it's running nothing.
        assert!(!may_run_this(&with("codex", "idle", false, None, now - RUNNING_MS - 1), now));
        assert!(may_run_this(&with("codex", "idle", false, None, now - 1000), now));
        assert!(!may_run_this(&Session { archived: true, ..with("codex", "working", true, None, now) }, now));
    }

    #[test]
    fn a_tool_call_runs_this_command_only_when_it_invokes_it() {
        let args = |line: &str| line.split(' ').map(String::from).collect::<Vec<_>>();
        let done = args("todo done t-abc");
        // Claude Code's command, with a `cd` first, or through the plugin's quoted shim.
        assert!(runs_command("cd /Users/me/projects/codealong-x && codealong todo done t-abc", &done));
        assert!(runs_command("\"${CLAUDE_PLUGIN_ROOT}/bin/codealong\" --json todo done t-abc", &done));
        assert!(runs_command("/Users/me/codealong-target/debug/codealong todo done t-abc", &done));
        // A loop puts the id before the command.
        assert!(runs_command("for id in t-abc t-def; do codealong todo done $id; done", &done));
        // Codex's code mode, escaped in a string, after other commands; OpenCode's input as JSON.
        assert!(runs_command(r#"text(await tools.exec_command({cmd:\"sed -n '1,20p' a.rs\ncodealong whoami\",\"max_output_tokens\":8000}));"#, &args("whoami")));
        assert!(runs_command(r#"{"command":"codealong todo start t-abc","description":"Start it"}"#, &args("todo start t-abc")));
        // Free text isn't matched word for word; ids and options are.
        let add = vec!["todo".to_string(), "update".into(), "t-abc".into(), "--title".into(), "It's \"done\"".into()];
        assert!(runs_command("codealong todo update t-abc --title 'It'\\''s \"done\"'", &add));
        // The repo's folder isn't the command; another id or subcommand isn't this call.
        assert!(!runs_command("cd ~/projects/codealong && grep -n whoami src-tauri/src/cli.rs", &args("whoami")));
        assert!(!runs_command("cat ~/Library/Application\\ Support/Codealong/codealong.sock whoami", &args("whoami")));
        assert!(!runs_command("codealong todo done t-other", &done));
        assert!(!runs_command("codealong todo start t-abc", &done));
        assert!(!runs_command("codealong whoamix", &args("whoami")));
        assert!(!runs_command("anything", &[]));
    }

    #[test]
    fn the_skill_reference_is_current() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../skills/codealong/references/reference.md");
        let on_disk = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(on_disk == reference_text(), "skills/codealong/references/reference.md is out of date: run scripts/skill-reference.sh");
    }

    #[test]
    fn devices_are_named_by_name_short_id_or_key() {
        // As the app sends the view, and what an app of another build leaves out reads as empty.
        let view: server::DevicesSummary = serde_json::from_value(json!({
            "devices": [{ "key": "9f8e7d6c5b4a3928", "short": "9f8e7d6c5b", "name": "Mac mini", "worktrees": 3 }],
            "phones": [{ "key": "b7c6d5e4f3a29180", "short": "b7c6d5e4f3", "name": "Sam's iPhone", "kind": "phone" }],
            "found": [{ "key": "7c6b5a4938271605", "short": "7c6b5a4938", "name": "iMac" }, { "key": "7c6b00000000", "short": "7c6b000000", "name": "iMac Pro" }]
        }))
        .unwrap();
        assert_eq!((view.devices[0].worktrees, view.devices[0].sessions), (3, 0));
        assert_eq!(which_device(&view, "mac MINI", &[Listed::Linked]).unwrap().1, "Mac mini");
        assert_eq!(which_device(&view, "9f8e7d6c5b", &[Listed::Linked]).unwrap().0, "9f8e7d6c5b4a3928");
        assert_eq!(which_device(&view, "7c6b5a", &[Listed::Found]).unwrap().1, "iMac");
        assert!(which_device(&view, "7c6b", &[Listed::Found]).is_err(), "two keys start that way");
        assert!(which_device(&view, "iMac", &[Listed::Linked]).is_err(), "only the lists asked for");
        // Unlinking takes a phone too.
        assert_eq!(which_device(&view, "sam's iphone", &[Listed::Linked, Listed::Phones]).unwrap().0, "b7c6d5e4f3a29180");
        // The summary keeps a device's own fields beside its counts, as the app wrote them by hand.
        let back = serde_json::to_value(&view).unwrap();
        assert_eq!((back["devices"][0]["name"].as_str(), back["devices"][0]["worktrees"].as_u64()), (Some("Mac mini"), Some(3)));
        assert!(back.get("lanes").is_none());
    }

    #[test]
    fn tables_line_up() {
        let text = table_text(&["A", "BB"], vec![vec!["x".into(), "y".into()], vec!["long".into(), "z".into()]]);
        assert_eq!(text, "A     BB\nx     y\nlong  z\n");
        assert!(table_text(&["A"], vec![]).contains("(none)"));
    }
}
