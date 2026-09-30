//! What a device asks another for, and what comes back, as types both sides share: the engine
//! answers with them (its actions.rs) and a device that only watches reads them
//! (client.rs). A conversation page is built by the engine's adapters as its `transcript::Page`,
//! with borrowed strings; `Page` here reads one back, and the engine's tests check the two agree.

use serde::{Deserialize, Serialize};

use crate::model::Harness;

/// The requests, by name.
pub const SESSION_PAGE: &str = "session-page";
pub const SESSION_NEWER: &str = "session-newer";
pub const SESSION_PENDING: &str = "session-pending";
pub const SESSION_ANSWER: &str = "session-answer";
pub const SESSION_SEND: &str = "session-send";
pub const SESSION_ARCHIVE: &str = "session-archive";
pub const MARK_SEEN: &str = "mark-seen";
pub const SEARCH: &str = "search";
pub const START_OPTIONS: &str = "start-options";
pub const START: &str = "start";
/// The reply a session Codealong runs (over ACP) is writing, as it stands.
pub const SESSION_STREAM: &str = "session-stream";
/// The sessions whose replies to send this connection as they're written (`Msg::Stream`), in
/// place of the ones before; `keys` empty stops them.
pub const SESSION_WATCH: &str = "session-watch";
/// Stops the turn a session is taking (one whose feed says `stoppable`), answering what happened
/// in a sentence. Macs before 2026-09-29 stop only sessions Codealong runs, and answer null.
pub const SESSION_CANCEL: &str = "session-cancel";
/// Where what you typed goes, as the quick panel suggests it: Jev's answer about this device's
/// board (`Route`). Macs before 2026-09-29 refuse it as a request they don't know.
pub const ROUTE: &str = "route";
/// Names a session (`key`, `name`) where its harness keeps its name. Macs before 2026-09-30
/// refuse it as a request they don't know.
pub const SESSION_RENAME: &str = "session-rename";
/// How a message sent to a session would reach it (`Way`), or null for one Codealong runs itself.
/// Macs before 2026-09-30 refuse it as a request they don't know.
pub const SESSION_WAY: &str = "session-way";

/// Who may make a request of a device, as the device asked decides it (the engine's peers.rs).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Reach {
    /// Any linked device: it only reads.
    Read,
    /// Any linked device: it changes how a session is listed there (seen, archived, named), not
    /// what an agent does.
    Organize,
    /// Only with remote control on at the device asked: it drives an agent (a prompt, an answer,
    /// a start, a stop). The device asked records each one in its devices.log.
    Control,
}

/// Every request, with who may make it. A request missing here is one a device doesn't know.
pub const REQUESTS: [(&str, Reach); 16] = [
    (SESSION_PAGE, Reach::Read),
    (SESSION_NEWER, Reach::Read),
    (SESSION_PENDING, Reach::Read),
    (SESSION_WAY, Reach::Read),
    (SESSION_STREAM, Reach::Read),
    (SESSION_WATCH, Reach::Read),
    (SEARCH, Reach::Read),
    (START_OPTIONS, Reach::Read),
    (ROUTE, Reach::Read),
    (MARK_SEEN, Reach::Organize),
    (SESSION_ARCHIVE, Reach::Organize),
    (SESSION_RENAME, Reach::Organize),
    (SESSION_SEND, Reach::Control),
    (SESSION_ANSWER, Reach::Control),
    (SESSION_CANCEL, Reach::Control),
    (START, Reach::Control),
];

/// Who may make `method` of a device; None for a request this crate doesn't know.
pub fn reach(method: &str) -> Option<Reach> {
    REQUESTS.iter().find(|(m, _)| *m == method).map(|(_, r)| *r)
}

/// The reply a session Codealong runs is writing, as the Mac's acp.rs has it: what it said since
/// its last tool call, and the tool it's running. Both empty once the turn ends.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Stream {
    pub key: String,
    /// The reply so far since the turn's last tool call; empty when there's none.
    pub text: String,
    /// The tool it's running, as a line.
    pub tool: Option<String>,
}

/// One thing said or done in a session, as a conversation page carries it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub at: i64,
    /// `you`, `agent`, `tool` (a call, with its output when the record has it there), `result`
    /// (a call's output, by `id`) or `note` (compaction, a message from another session…).
    pub kind: String,
    /// Prose for you, agent and note; for a tool, a one-line label such as "Bash · `cargo test`".
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// A tool's whole input: the command, the file, the arguments.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    #[serde(default)]
    pub failed: bool,
    /// A prompt of yours the session took back before its agent answered, so the agent never
    /// saw it (Claude Code, when a turn is stopped before any reply).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub withdrawn: bool,
    /// Where its line starts in the record (or its message number); with `n`, unique in the session.
    pub cursor: u64,
    pub n: u32,
}

/// A page of a session's conversation, oldest first.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Page {
    pub items: Vec<Item>,
    /// Where the next page back ends; none at the session's start.
    pub before: Option<u64>,
    /// Where reading on, for what comes next, starts.
    pub after: u64,
}

/// What a session waits on you for, when it's a tool call you can allow or deny.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Pending {
    /// The request's own id: the tool call's (Claude Code) or the permission request's (OpenCode).
    /// An answer names it, so a request that changed meanwhile isn't answered by mistake.
    pub id: String,
    /// The tool call as a line, e.g. "Bash · `rm -rf build`".
    pub label: String,
    /// All of it: the command, the file and the change.
    pub input: Option<String>,
    /// Why Codealong can't answer it here, when it can't; the panel then offers to open it.
    pub cannot: Option<String>,
    /// It can also be allowed from now on (`Answer::Always`), not just this once.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub always: bool,
    /// It's a plan to approve (Claude Code's plan mode): `input` is the plan, in Markdown, to read
    /// as prose rather than as a command.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub plan: bool,
    /// What `Answer::Always` adds, in a sentence, when Codealong can say: "Allows
    /// `Bash(npm test:*)` in this project, for you."
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub always_rule: Option<String>,
    /// Questions it asks you (Claude Code's `AskUserQuestion`): they're answered with your picks
    /// (the `answers` of `session-answer`), not allowed. Devices that don't know them offer Allow
    /// and Deny, and an Allow is refused.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub questions: Vec<Question>,
}

/// A question an agent asks you, with choices to pick from.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Question {
    /// Its text, which its answer is keyed by.
    pub question: String,
    /// A word or two naming it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub header: Option<String>,
    pub options: Vec<Choice>,
    /// More than one choice can be picked.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub multi_select: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Choice {
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Your answer to a pending request. `Always` is offered only where `Pending::always` says so;
/// devices from before it don't know it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Answer {
    Allow,
    Deny,
    /// Allow this, and calls like it from now on.
    Always,
}

/// How a prompt sent to a session went.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Sent {
    /// One sentence for the panel: where the prompt went, or what to do next.
    pub message: String,
    /// The session or its app was opened for you to send it there, rather than sent.
    pub opened: bool,
    /// It couldn't be typed into the session's terminal, so it went another way, as `message`
    /// says: worth showing where a send usually goes without a word (the floating bar's peek).
    /// Missing from devices that don't know it yet.
    #[serde(default)]
    pub fell_back: bool,
}

/// How a message sent to a session would reach it, for its box to say before you send: the first
/// of its ways in that applies (the engine's send.rs `way_in`). Typing can still find no tab and
/// go another way, as the send then says.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Way {
    /// Typed into its tab or pane in this terminal: Ghostty, iTerm2, Terminal, tmux.
    Typed { app: String },
    /// Pasted into its background job by Claude Code's background service.
    Background,
    /// A message from Codealong on Claude Code's socket for messages between sessions.
    Message,
    /// It isn't running: a new tab in this terminal resumes it with the prompt.
    Resume { app: String },
    /// A Claude app session that isn't running: the app opens it with the prompt on the clipboard.
    ClaudeApp,
    /// A turn the Codex app starts, or, when it can't, the thread opens with the prompt filled in.
    CodexApp,
    /// Posted to OpenCode's service.
    Opencode,
    /// Nothing takes a prompt from outside: the app comes forward with it on the clipboard.
    Antigravity,
}

// ---- starting a session (`start-options`, `start`) ----

/// Where a new session runs.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "kebab-case")]
pub enum Place {
    /// A new terminal window running the account's command.
    Terminal,
    /// Claude Code's background service (`--bg`), with no terminal; `attach` opens one later.
    Background,
    /// The Claude app's new-session page, with the folder and prompt filled in.
    ClaudeApp,
    /// A new thread in the Codex app, with the folder and prompt filled in.
    CodexApp,
    /// In Codealong itself, through the account's Agent Client Protocol adapter (acp.rs).
    Codealong,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModelOption {
    /// What `--model` gets.
    pub id: String,
    pub label: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PlaceOption {
    pub place: Place,
    /// The app's name, the way buttons say it: Ghostty, Claude app, Codex app.
    pub label: String,
}

/// An account as the new-session sheet offers it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StartAccount {
    pub name: String,
    pub harness: Harness,
    /// Where this account can start a session, the usual place first: the one its `start`
    /// setting names, when it has one.
    pub places: Vec<PlaceOption>,
    /// The place its `start` setting names. A start goes there rather than where the last
    /// one went.
    pub chosen: Option<Place>,
    /// Why there's nowhere, when `places` is empty.
    pub unavailable: Option<String>,
    /// The model a session starts on without `--model`: the account's settings.json `model`.
    pub default_model: Option<String>,
    /// Its `preferred` setting: Auto starts it over accounts without one while it has room.
    pub preferred: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StartOptions {
    /// In config order.
    pub accounts: Vec<StartAccount>,
    /// Folders the Codex app has as projects. A thread started anywhere else adds one.
    pub codex_projects: Vec<String>,
    /// The models a Claude Code start in the terminal can ask for.
    pub claude_models: Vec<ModelOption>,
    /// The effort levels a Claude Code start can ask for, as `--effort` takes them. Missing from
    /// devices that don't know `effort` yet, which start on the account's own.
    #[serde(default)]
    pub claude_efforts: Vec<ModelOption>,
}

/// What the sheet asks for.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct StartSpec {
    pub lane_id: String,
    pub account: String,
    pub place: Place,
    #[serde(default)]
    pub prompt: String,
    /// `--model` for a terminal start; None starts on the account's default.
    #[serde(default)]
    pub model: Option<String>,
    /// `--effort` for a Claude Code start; None starts on the account's own (settings.json's
    /// `effortLevel`, or the model's).
    #[serde(default)]
    pub effort: Option<String>,
    /// A terminal start in a new worktree Claude Code makes (`--worktree`). The apps have
    /// their own worktree choice on their new-session pages.
    #[serde(default)]
    pub worktree: bool,
    /// The todos this session is for, which it's linked to once it starts: one, or several
    /// started together on one prompt. Older windows and commands send one as `todo`.
    #[serde(default, alias = "todo", deserialize_with = "one_or_more")]
    pub todos: Vec<String>,
}

/// A list of todo ids, or the one id (or null) that `todo` held.
fn one_or_more<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Ids {
        One(String),
        More(Vec<String>),
    }
    Ok(match Option::<Ids>::deserialize(d)? {
        None => Vec::new(),
        Some(Ids::One(id)) => vec![id],
        Some(Ids::More(ids)) => ids,
    })
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Started {
    /// The new session's key when Codealong chose its ID, so the board can select it as soon
    /// as it shows.
    pub key: Option<String>,
    /// What to do next, when the app is waiting on you.
    pub message: Option<String>,
}

impl Place {
    pub const ALL: [Place; 5] = [Place::Terminal, Place::Background, Place::ClaudeApp, Place::CodexApp, Place::Codealong];

    /// As config.toml and the command line write it.
    pub fn word(self) -> &'static str {
        match self {
            Place::Terminal => "terminal",
            Place::Background => "background",
            Place::ClaudeApp => "claude-app",
            Place::CodexApp => "codex-app",
            Place::Codealong => "codealong",
        }
    }

    pub fn from_word(w: &str) -> Option<Place> {
        Place::ALL.into_iter().find(|p| p.word() == w.trim())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_reads_back_what_it_wrote() {
        let page = Page {
            items: vec![
                Item { at: 1, kind: "you".into(), text: "Fix the build".into(), id: None, input: None, output: None, failed: false, withdrawn: false, cursor: 0, n: 0 },
                Item { at: 2, kind: "tool".into(), text: "Bash · `cargo test`".into(), id: Some("t1".into()), input: Some("cargo test".into()), output: Some("ok".into()), failed: false, withdrawn: false, cursor: 120, n: 1 },
            ],
            before: Some(0),
            after: 400,
        };
        let back: Page = serde_json::from_value(serde_json::to_value(&page).unwrap()).unwrap();
        assert_eq!(back, page);
        let json = serde_json::to_string(&page).unwrap();
        assert!(json.contains(r#""before":0"#) && !json.contains("\"input\":null"), "{json}");
        assert_eq!(serde_json::to_value(Answer::Allow).unwrap(), "allow");
    }
}

// ---- the quick panel: what was said, and where typed text goes ----

/// A session whose text has every word.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hit {
    pub key: String,
    /// The message that matched best, cut to a line around the first match.
    pub snippet: String,
    /// You wrote it; otherwise the agent did.
    pub you: bool,
    pub at: i64,
    /// How well that message matched (the engine's `search::match_score`): higher is better.
    /// Devices of earlier builds don't send it.
    #[serde(default)]
    pub score: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Found {
    pub hits: Vec<Hit>,
    /// Still reading sessions for the first time, so more may match soon.
    pub building: bool,
    /// Searching text is turned off (`days = 0`).
    pub off: bool,
}


// Routing, as the Mac's route.rs reads it from Jev.

/// What Jev thinks you want done with what you typed.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    /// Send it to a session that's already going.
    Send,
    /// Start a new session with it.
    Start,
    /// It names a session to open.
    Find,
    /// Keep it as a todo.
    Todo,
}

impl Action {
    pub fn key(self) -> &'static str {
        match self {
            Action::Send => "send",
            Action::Start => "start",
            Action::Find => "find",
            Action::Todo => "todo",
        }
    }
    pub const ALL: [Action; 4] = [Action::Send, Action::Start, Action::Find, Action::Todo];
}

/// One option Jev weighed, with its probability.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Pick {
    pub id: String,
    pub p: f64,
}

/// A repo's latest starts from the quick panel: how many went into a new worktree, of how many.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug, Default)]
pub struct Habit {
    pub new: usize,
    pub of: usize,
}

/// The suggestion, with the runners-up, for the panel to show.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Route {
    pub action: Action,
    /// How likely each action is, most likely first.
    pub actions: Vec<(Action, f64)>,
    /// Session keys, most likely first; those Jev gives any real chance, for ranking the search.
    pub sessions: Vec<Pick>,
    /// How likely it is that no listed session is meant.
    pub no_session: f64,
    /// Worktrees (lane ids), most likely first; the top ones only.
    pub lanes: Vec<Pick>,
    /// Repos, the worktrees' probabilities added up: which repo is clearer than which worktree.
    pub repos: Vec<Pick>,
    /// How likely it is that new work wants a worktree of its own: Jev's answer weighed by how
    /// you've started work in the likeliest repo (the engine's starts.rs).
    pub own_worktree: f64,
    /// Jev's answer before that. Macs of earlier builds don't send it.
    #[serde(default)]
    pub own_worktree_asked: f64,
    /// Your latest starts in that repo.
    #[serde(default)]
    pub habit: Habit,
    /// A live session that may already be doing this work, when Jev thinks one is.
    pub duplicate: Option<Pick>,
    /// The versioned model that answered, such as `jev-1.13.0`.
    pub model: String,
    pub ms: u64,
    pub input_tokens: u64,
}

